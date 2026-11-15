//! WP-DROPS: Minecraft-style item drops and pickups.
//!
//! Every broken tile becomes a floating drop entity ([`crate::entity::EntityKind::Drop`]): ore
//! and artifacts drop their item, plain dirt / clay / stone / hard rock drop building blocks
//! ([`crate::item::BlockKind`]). The drill, every explosive's blast, incendiary fire and
//! boulders shattering on rock (cave-ins) all go through [`WorldState::spawn_tile_drop`].
//!
//! # Collection
//!
//! Each Late entity phase ends with a pickup pass ([`WorldState::advance_drop_pickups`]):
//!
//! * every drop within a rig's magnet reach ([`magnet_radius`], growing with the cargo bay
//!   tier) is claimed by the **nearest** rig that has room for it (ties go to the lower player
//!   id), so co-op pickup is fair;
//! * a claimed drop inside [`DROP_COLLECT_RADIUS`] is stowed ([`crate::Player::stow`]: ore,
//!   artifacts and materials into the cargo bay, blocks into the block hopper) and the rig
//!   gets a [`GameplayEventKind::ItemsCollected`];
//! * a claimed drop further out is pulled in by the magnet (`DropState::pulled_by`): it flies
//!   free of gravity and rock toward the rig and is collected a few ticks later;
//! * a drop inside a rig's collect radius that the rig has no room for stays floating
//!   (`DropState::blocked`), and the rig hears [`GameplayEventKind::PickupBlocked`] once, when
//!   the first such drop appears in its reach (no repeat while the stack sits there).
//!
//! The drill's own drop lands inside the collect radius, so drilled ore reaches the bay on the
//! same tick, exactly as before drops existed; a full bay now leaves the ore floating instead
//! of losing it. Rigs whose `collect_blocks` toggle is off ignore plain blocks (they still
//! drop). A rig never auto-collects blocks it dumped ([`PlayerCommand::DumpBlocks`]).
//!
//! The pass runs on the host and on predicting clients alike (it is part of the tick), so a
//! client hears its pickups immediately and the host's entity deltas confirm them.
//!
//! # Clutter control
//!
//! * **Merging:** a drop that comes to rest merges into a resting drop of the same kind within
//!   [`DROP_MERGE_RADIUS`] (one stack, summed count).
//! * **Despawn timers:** plain blocks last [`BLOCK_DROP_LIFETIME_MILLIS`], dumped blocks
//!   [`DUMPED_DROP_LIFETIME_MILLIS`], ore and materials [`VALUABLE_DROP_LIFETIME_MILLIS`];
//!   artifacts never despawn.
//! * **Per-cell cap:** a spatial cell (16x16 tiles) holds at most [`DROPS_PER_CELL_CAP`]
//!   drops; a new drop over the cap joins an existing stack of its kind, else the oldest
//!   same-kind pair merges, else the oldest plain-block stack goes.
//! * **Per-blast cap:** one blast sprays at most [`MAX_DROPS_PER_BLAST`] separate drops; the
//!   rest of its loot rides on those stacks.
//! * **Sleep:** resting drops sleep (no per-tick cost) until terrain changes nearby, a blast
//!   or a magnet wakes them.
//!
//! # Blasts and loot
//!
//! A blast throws its drops outward from the charge ([`blast_spray_velocity`]). The heat of a
//! plain blast shatters a share of the ore it breaks ([`ore_survives_blast`]: about one tile
//! in three, decided per tile from the world seed); mining charges
//! ([`crate::explosive::BlastShape::LootSafe`]) break ore cleanly, so every tile pops out intact.
//! Artifacts always survive.
//!
//! # Seams for other tracks
//!
//! * Base building (WP-BASE) reads blocks from [`crate::Player::block_hopper`].
//! * Enemies (WP-ENEMIES) drop loot with the public [`WorldState::spawn_item_drop`], which
//!   applies the same lifetimes rules when given [`drop_lifetime_millis`].

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    entity::{Body, DropState, EntityId, EntityKind, EntityState, SpatialCell},
    event::{GameplayEventKind, TickOutput},
    explosive::{BlastShape, ExplosiveKind},
    ids::{PlayerId, SimulationTick},
    item::{BlockKind, ItemKind, ItemStack, Stowage},
    physics::{self, BodyParams},
    player::Player,
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::WorldState;

/// A rig collects a drop whose centre is this close to the rig's centre (world pixels). Wide
/// enough that the tile the drill just broke is always inside it.
pub const DROP_COLLECT_RADIUS: f32 = TILE_SIZE * 1.6;
/// Magnet reach of a stock cargo bay, in tiles.
pub const MAGNET_BASE_RADIUS_TILES: f32 = 3.0;
/// Extra magnet reach per cargo bay tier past the first, in tiles.
pub const MAGNET_RADIUS_PER_TIER_TILES: f32 = 0.25;
/// Top speed of a drop the magnet pulls in (world pixels per second).
pub const MAGNET_PULL_SPEED: f32 = 420.0;
/// Acceleration of a drop the magnet pulls in (world pixels per second squared).
pub const MAGNET_PULL_ACCELERATION: f32 = 3_200.0;
/// A resting drop merges into a resting same-kind drop this close (world pixels).
pub const DROP_MERGE_RADIUS: f32 = TILE_SIZE * 0.75;
/// Most drops one spatial cell (16x16 tiles) holds before new ones fold into stacks.
pub const DROPS_PER_CELL_CAP: usize = 40;
/// Most separate drops one blast sprays (the rest of its loot rides on those stacks).
pub const MAX_DROPS_PER_BLAST: usize = 24;
/// Lifetime of a plain-block drop.
pub const BLOCK_DROP_LIFETIME_MILLIS: u64 = 45_000;
/// Lifetime of blocks a rig dumped out of its hopper.
pub const DUMPED_DROP_LIFETIME_MILLIS: u64 = 20_000;
/// Lifetime of ore, materials and other valuables.
pub const VALUABLE_DROP_LIFETIME_MILLIS: u64 = 600_000;
/// Block hopper capacity added per cargo bay tier past the first.
pub const BLOCK_HOPPER_PER_TIER: u32 = 20;
/// Pixels a drill drop pops toward the rig per second, before gravity.
const DRILL_POP_SPEED: f32 = 70.0;
/// Pickup claims are checked for drops this far past a rig's magnet reach, so a stale
/// "blocked" mark is cleared once the rig moves off.
const BLOCKED_CLEAR_MARGIN: f32 = TILE_SIZE * 2.0;

/// Magnet reach of `player`'s rig, in world pixels (grows with the cargo bay tier).
#[must_use]
pub fn magnet_radius(player: &Player) -> f32 {
    let tiers = f32::from(player.cargo_bay_level.saturating_sub(1));
    MAGNET_RADIUS_PER_TIER_TILES.mul_add(tiers, MAGNET_BASE_RADIUS_TILES) * TILE_SIZE
}

/// Block hopper capacity fitted with cargo bay tier `level`.
#[must_use]
pub fn block_hopper_capacity_for_level(level: u8) -> u32 {
    crate::player::BLOCK_HOPPER_BASE_CAPACITY
        .saturating_add(BLOCK_HOPPER_PER_TIER.saturating_mul(u32::from(level.saturating_sub(1))))
}

/// How long a drop of `kind` lasts untouched (`None`: forever).
#[must_use]
pub const fn drop_lifetime_millis(kind: ItemKind) -> Option<u64> {
    match kind {
        ItemKind::Block(_) => Some(BLOCK_DROP_LIFETIME_MILLIS),
        ItemKind::Artifact(_) | ItemKind::Quest(_) => None,
        ItemKind::Mineral(_)
        | ItemKind::Material(_)
        | ItemKind::Module(_)
        | ItemKind::Weapon(_)
        | ItemKind::Ammo(_)
        | ItemKind::Explosive(_)
        | ItemKind::Cosmetic(_) => Some(VALUABLE_DROP_LIFETIME_MILLIS),
    }
}

/// Credits the depot pays for one block.
#[must_use]
pub const fn block_sale_value(block: BlockKind) -> u32 {
    match block {
        BlockKind::Dirt => 1,
        BlockKind::Clay | BlockKind::Stone => 2,
        BlockKind::HardRock => 4,
    }
}

/// splitmix64 finalizer.
const fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[allow(clippy::cast_sign_loss, reason = "tile coordinates are hashed bitwise")]
const fn tile_hash(seed: u64, position: TilePosition, salt: u64) -> u64 {
    mix(seed
        ^ mix((position.x as u32 as u64) | ((position.y as u32 as u64) << 32))
        ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

/// Whether an ore tile at `position` broken by `kind`'s blast comes out intact.
///
/// A plain blast's heat shatters about one ore tile in three; mining charges keep every one.
/// Artifacts and blocks always survive.
#[must_use]
pub const fn ore_survives_blast(
    kind: ExplosiveKind,
    item: ItemKind,
    seed: u64,
    position: TilePosition,
) -> bool {
    if !matches!(item, ItemKind::Mineral(_)) {
        return true;
    }
    if matches!(kind.blast(), BlastShape::LootSafe(_)) {
        return true;
    }
    !tile_hash(seed, position, 0x5ba7).is_multiple_of(3)
}

/// Launch velocity of the drop of a tile at `position` thrown by a blast at `(x, y)` (world
/// pixels): outward from the charge with a deterministic per-tile wobble, plus a pop upward.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "tile coordinates and small hash slices are exact in f32"
)]
pub fn blast_spray_velocity(x: f32, y: f32, position: TilePosition, seed: u64) -> (f32, f32) {
    let (cx, cy) = tile_centre(position);
    let hash = tile_hash(seed, position, 0x0d0b);
    let wobble = ((hash & 0xff) as f32 / 255.0).mul_add(2.0, -1.0);
    let launch = (((hash >> 8) & 0xff) as f32 / 255.0).mul_add(90.0, 130.0);
    let (dx, dy) = (cx - x, cy - y);
    let distance = dx.hypot(dy);
    let (nx, ny) = if distance < 1.0 {
        (wobble, -1.0)
    } else {
        (dx / distance, dy / distance)
    };
    (
        quantize(wobble.mul_add(40.0, nx * launch)),
        quantize(ny.mul_add(launch, -150.0).max(-420.0)),
    )
}

/// Centre of a tile in world pixels.
#[allow(
    clippy::cast_precision_loss,
    reason = "tile coordinates are exact in f32"
)]
fn tile_centre(position: TilePosition) -> (f32, f32) {
    (
        (position.x as f32 + 0.5) * TILE_SIZE,
        (position.y as f32 + 0.5) * TILE_SIZE,
    )
}

/// One drop's best claim in the pickup pass.
#[derive(Clone, Copy)]
struct Claim {
    player: PlayerId,
    distance_squared: f32,
}

impl WorldState {
    /// Spawns the drop a broken tile yields (`None` if the tile yields nothing, e.g. gas or
    /// lava), at the tile's centre with `velocity`, subject to the per-cell cap.
    pub(crate) fn spawn_tile_drop(
        &mut self,
        position: TilePosition,
        kind: TileKind,
        velocity: (f32, f32),
    ) -> Option<EntityId> {
        let item = ItemKind::from_tile(kind)?;
        let (x, y) = tile_centre(position);
        self.spawn_loot(x, y, velocity, ItemStack::new(item, 1), None)
    }

    /// Spawns a drop of `stack` with its kind's lifetime ([`drop_lifetime_millis`]; dumped
    /// blocks shorter), folding it into an existing stack when its cell is at the cap. Returns
    /// the entity holding it.
    pub(crate) fn spawn_loot(
        &mut self,
        x: f32,
        y: f32,
        velocity: (f32, f32),
        stack: ItemStack,
        ignored_by: Option<PlayerId>,
    ) -> Option<EntityId> {
        if stack.count == 0 {
            return None;
        }
        if let Some(existing) = self.make_room_in_cell(x, y, stack.kind, ignored_by) {
            self.entities.update(existing, |entity| {
                if let EntityState::Drop(drop) = &mut entity.state {
                    drop.stack.count = drop.stack.count.saturating_add(stack.count);
                }
            });
            return Some(existing);
        }
        let lifetime = if ignored_by.is_some() {
            Some(DUMPED_DROP_LIFETIME_MILLIS)
        } else {
            drop_lifetime_millis(stack.kind)
        };
        let id = self.spawn_item_drop(x, y, velocity, stack, lifetime);
        if ignored_by.is_some() {
            self.entities.update(id, |entity| {
                if let EntityState::Drop(drop) = &mut entity.state {
                    drop.ignored_by = ignored_by;
                }
            });
        }
        Some(id)
    }

    /// Enforces [`DROPS_PER_CELL_CAP`] before a drop of `kind` spawns at `(x, y)`. Returns a
    /// drop of the same kind to add the new one to instead, if the cell is full and has one.
    fn make_room_in_cell(
        &mut self,
        x: f32,
        y: f32,
        kind: ItemKind,
        ignored_by: Option<PlayerId>,
    ) -> Option<EntityId> {
        let cell = SpatialCell::of(x, y);
        #[allow(clippy::cast_precision_loss, reason = "cell coordinates are small")]
        let span = crate::entity::ENTITY_CELL_TILES as f32 * TILE_SIZE;
        #[allow(clippy::cast_precision_loss, reason = "cell coordinates are small")]
        let (left, top) = (cell.x as f32 * span, cell.y as f32 * span);
        let drops: Vec<(EntityId, DropState, f32, f32)> = self
            .entities
            .ids_in_rect(left, top, left + span - 0.001, top + span - 0.001)
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                match entity.state {
                    EntityState::Drop(drop) => Some((id, drop, entity.body.x, entity.body.y)),
                    _ => None,
                }
            })
            .collect();
        if drops.len() < DROPS_PER_CELL_CAP {
            return None;
        }
        // Join the nearest same-kind stack.
        let nearest = drops
            .iter()
            .filter(|(_, drop, _, _)| drop.stack.kind == kind && drop.ignored_by == ignored_by)
            .min_by(|a, b| {
                let da = (a.2 - x).hypot(a.3 - y);
                let db = (b.2 - x).hypot(b.3 - y);
                da.total_cmp(&db).then(a.0.cmp(&b.0))
            })
            .map(|(id, ..)| *id);
        if nearest.is_some() {
            return nearest;
        }
        // Merge the oldest stack that has a same-kind partner into that partner.
        for (index, (id, drop, ..)) in drops.iter().enumerate() {
            if let Some((partner, ..)) = drops[index + 1..].iter().find(|(_, other, ..)| {
                other.stack.kind == drop.stack.kind && other.ignored_by == drop.ignored_by
            }) {
                self.merge_drop_into(*id, *partner);
                return None;
            }
        }
        // Otherwise the oldest plain-block stack goes.
        if let Some((oldest, ..)) = drops
            .iter()
            .find(|(_, drop, ..)| matches!(drop.stack.kind, ItemKind::Block(_)))
        {
            self.entities.despawn(*oldest);
        }
        None
    }

    /// Moves drop `from`'s stack onto drop `into` and removes `from`. Valuables keep the later
    /// of the two expiries; blocks keep `into`'s.
    fn merge_drop_into(&mut self, from: EntityId, into: EntityId) {
        let Some((count, expiry)) = self
            .entities
            .get(from)
            .and_then(|entity| match entity.state {
                EntityState::Drop(drop) => Some((drop.stack.count, entity.expires_at)),
                _ => None,
            })
        else {
            return;
        };
        self.entities.update(into, |entity| {
            if let EntityState::Drop(drop) = &mut entity.state {
                drop.stack.count = drop.stack.count.saturating_add(count);
                if !matches!(drop.stack.kind, ItemKind::Block(_)) {
                    entity.expires_at = match (entity.expires_at, expiry) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        _ => None,
                    };
                }
            }
        });
        self.entities.despawn(from);
    }

    /// The drop the drill just broke out of `target`, popping gently toward the rig.
    pub(crate) fn spawn_drill_drop(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        kind: TileKind,
    ) {
        let (cx, cy) = tile_centre(target);
        let velocity = self.players.get(&player_id).map_or((0.0, 0.0), |player| {
            let (dx, dy) = (player.x - cx, player.y - cy);
            let distance = dx.hypot(dy).max(1.0);
            (
                quantize(dx / distance * DRILL_POP_SPEED),
                quantize(dy / distance * DRILL_POP_SPEED),
            )
        });
        let _ = self.spawn_tile_drop(target, kind, velocity);
    }

    /// Sprays the drops of the tiles a blast of `kind` at `(x, y)` broke (`broken`: each tile's
    /// position and what it was). Plain blasts shatter some ore ([`ore_survives_blast`]); at
    /// most [`MAX_DROPS_PER_BLAST`] separate drops spawn.
    pub(crate) fn spawn_blast_drops(
        &mut self,
        kind: ExplosiveKind,
        x: f32,
        y: f32,
        broken: &[(TilePosition, TileKind)],
    ) {
        let seed = self.terrain.seed();
        let mut spawned: Vec<(ItemKind, EntityId)> = Vec::new();
        for (position, tile) in broken {
            let Some(item) = ItemKind::from_tile(*tile) else {
                continue;
            };
            if !ore_survives_blast(kind, item, seed, *position) {
                continue;
            }
            if spawned.len() >= MAX_DROPS_PER_BLAST
                && let Some((_, id)) = spawned
                    .iter()
                    .find(|(other, id)| *other == item && self.entities.contains(*id))
            {
                self.entities.update(*id, |entity| {
                    if let EntityState::Drop(drop) = &mut entity.state {
                        drop.stack.count = drop.stack.count.saturating_add(1);
                    }
                });
                continue;
            }
            let velocity = blast_spray_velocity(x, y, *position, seed);
            if let Some(id) = self.spawn_tile_drop(*position, *tile, velocity) {
                spawned.push((item, id));
            }
        }
    }

    /// A drop's step: pulled in by a magnet (free flight toward the rig), or falling, bouncing
    /// and sliding under [`BodyParams::DROP`]. A drop that comes to rest merges into a resting
    /// same-kind neighbour (recorded in `merged` for one batched removal).
    pub(super) fn advance_drop(&mut self, id: EntityId, merged: &mut BTreeSet<EntityId>) {
        if merged.contains(&id) {
            return;
        }
        let delta_seconds = self.timing.delta_seconds;
        let sleep_after = u16::try_from(
            self.timing
                .ticks_for_millis(super::entities::ENTITY_SLEEP_AFTER_MILLIS),
        )
        .unwrap_or(u16::MAX);
        let Some(drop) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Drop(drop) => Some(drop),
            _ => None,
        }) else {
            return;
        };
        if let Some(puller) = drop.pulled_by {
            if let Some((px, py)) = self.players.get(&puller).map(|p| (p.x, p.y)) {
                self.entities.update(id, |entity| {
                    let body = &mut entity.body;
                    body.previous_x = body.x;
                    body.previous_y = body.y;
                    physics::steer_toward(
                        body,
                        px,
                        py,
                        MAGNET_PULL_SPEED,
                        MAGNET_PULL_ACCELERATION,
                        delta_seconds,
                    );
                    body.x = quantize(body.velocity_x.mul_add(delta_seconds, body.x));
                    body.y = quantize(body.velocity_y.mul_add(delta_seconds, body.y));
                    body.rest_ticks = 0;
                    body.asleep = false;
                });
                return;
            }
            self.entities.update(id, |entity| {
                if let EntityState::Drop(drop) = &mut entity.state {
                    drop.pulled_by = None;
                }
            });
        }
        let terrain = &self.terrain;
        let landed = self
            .entities
            .update(id, |entity| {
                let params = BodyParams::DROP;
                let _report = physics::step_body(&mut entity.body, &params, terrain, delta_seconds);
                physics::settle(&mut entity.body, &params, terrain, sleep_after);
                entity.body.rest_ticks == 1
            })
            .unwrap_or(false);
        if landed && let Some(partner) = self.merge_partner(id, drop, merged) {
            self.merge_drop_into(id, partner);
            // `merge_drop_into` already removed it; remember it so no one merges into it.
            merged.insert(id);
        }
    }

    /// A resting, unpulled drop of the same kind (and dump owner) within
    /// [`DROP_MERGE_RADIUS`] of `id`, nearest first (lowest id on ties).
    fn merge_partner(
        &self,
        id: EntityId,
        drop: DropState,
        merged: &BTreeSet<EntityId>,
    ) -> Option<EntityId> {
        let entity = self.entities.get(id)?;
        let (x, y) = (entity.body.x, entity.body.y);
        self.entities
            .ids_in_radius(x, y, DROP_MERGE_RADIUS)
            .into_iter()
            .filter(|other| *other != id && !merged.contains(other))
            .filter_map(|other| {
                let candidate = self.entities.get(other)?;
                let EntityState::Drop(other_drop) = candidate.state else {
                    return None;
                };
                let resting = candidate.body.asleep || candidate.body.rest_ticks > 0;
                (resting
                    && other_drop.pulled_by.is_none()
                    && other_drop.stack.kind == drop.stack.kind
                    && other_drop.ignored_by == drop.ignored_by)
                    .then(|| ((candidate.body.x - x).hypot(candidate.body.y - y), other))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, other)| other)
    }

    /// The pickup pass at the end of the Late entity phase (see the module docs): nearest-rig
    /// claims, collection, magnet pulls and "full" feedback.
    #[allow(
        clippy::too_many_lines,
        reason = "one pass: claims, then collection, pulls and blocked marks in id order"
    )]
    pub(super) fn advance_drop_pickups(&mut self, output: &mut TickOutput) {
        if self.entities.count_of(EntityKind::Drop) == 0 || self.players.is_empty() {
            return;
        }
        let collect_squared = DROP_COLLECT_RADIUS * DROP_COLLECT_RADIUS;
        let mut claims: BTreeMap<EntityId, Claim> = BTreeMap::new();
        // Drops in some rig's collect radius that rig has no room for: drop -> (rig, stowage).
        let mut full: BTreeMap<EntityId, (PlayerId, Stowage)> = BTreeMap::new();
        // Every drop looked at (to clear stale pulls and blocked marks).
        let mut seen: BTreeSet<EntityId> = BTreeSet::new();
        for (player_id, player) in &self.players {
            let magnet = magnet_radius(player);
            let magnet_squared = magnet * magnet;
            for id in self.entities.ids_in_radius(
                player.x,
                player.y,
                magnet.max(DROP_COLLECT_RADIUS) + BLOCKED_CLEAR_MARGIN,
            ) {
                let Some(entity) = self.entities.get(id) else {
                    continue;
                };
                let EntityState::Drop(drop) = entity.state else {
                    continue;
                };
                seen.insert(id);
                let (dx, dy) = (entity.body.x - player.x, entity.body.y - player.y);
                let distance_squared = dx.mul_add(dx, dy * dy);
                if distance_squared > magnet_squared.max(collect_squared)
                    || drop.ignored_by == Some(*player_id)
                    || (drop.stack.kind.stowage() == Stowage::BlockHopper && !player.collect_blocks)
                {
                    continue;
                }
                if player.room_for(drop.stack.kind, 1) == 0 {
                    if distance_squared <= collect_squared {
                        full.entry(id)
                            .or_insert_with(|| (*player_id, drop.stack.kind.stowage()));
                    }
                    continue;
                }
                // A magnet only starts pulling what it can see; a pull in progress goes on.
                if distance_squared > collect_squared
                    && drop.pulled_by != Some(*player_id)
                    && !physics::line_clear(
                        &self.terrain,
                        player.x,
                        player.y,
                        entity.body.x,
                        entity.body.y,
                    )
                {
                    continue;
                }
                let better = claims
                    .get(&id)
                    .is_none_or(|claim| distance_squared < claim.distance_squared);
                if better {
                    claims.insert(
                        id,
                        Claim {
                            player: *player_id,
                            distance_squared,
                        },
                    );
                }
            }
        }
        // Rigs that already had a "full" drop in reach before this tick (no repeat warning).
        let mut warned: BTreeSet<(PlayerId, Stowage)> = BTreeSet::new();
        for (id, (player_id, stowage)) in &full {
            let already = self.entities.get(*id).is_some_and(
                |entity| matches!(entity.state, EntityState::Drop(drop) if drop.blocked),
            );
            if already {
                warned.insert((*player_id, *stowage));
            }
        }
        let mut collected: BTreeSet<EntityId> = BTreeSet::new();
        let mut newly_full: BTreeSet<(PlayerId, Stowage)> = BTreeSet::new();
        for id in seen {
            let Some(entity) = self.entities.get(id) else {
                continue;
            };
            let EntityState::Drop(drop) = entity.state else {
                continue;
            };
            let (x, y) = (entity.body.x, entity.body.y);
            if let Some(claim) = claims.get(&id) {
                if claim.distance_squared <= collect_squared {
                    let Some(player) = self.players.get_mut(&claim.player) else {
                        continue;
                    };
                    let taken = player.stow(drop.stack.kind, drop.stack.count);
                    if taken > 0 {
                        output.push_event(GameplayEventKind::ItemsCollected {
                            player_id: claim.player,
                            item: drop.stack.kind,
                            count: taken,
                            x: event_pixel(x),
                            y: event_pixel(y),
                        });
                    }
                    if taken >= drop.stack.count {
                        collected.insert(id);
                        continue;
                    }
                    // The rig filled up part way through the stack: the rest stays.
                    let stowage = drop.stack.kind.stowage();
                    if !drop.blocked && !warned.contains(&(claim.player, stowage)) {
                        newly_full.insert((claim.player, stowage));
                    }
                    self.entities.update(id, |entity| {
                        if let EntityState::Drop(drop) = &mut entity.state {
                            drop.stack.count -= taken;
                            drop.pulled_by = None;
                            drop.blocked = true;
                        }
                    });
                    continue;
                }
                if drop.pulled_by != Some(claim.player) || drop.blocked {
                    let puller = claim.player;
                    self.entities.update(id, |entity| {
                        if let EntityState::Drop(drop) = &mut entity.state {
                            drop.pulled_by = Some(puller);
                            drop.blocked = false;
                        }
                        entity.body.wake();
                    });
                }
                continue;
            }
            let blocked_by = full.get(&id).copied();
            if let Some((player_id, stowage)) = blocked_by
                && !drop.blocked
                && !warned.contains(&(player_id, stowage))
            {
                newly_full.insert((player_id, stowage));
            }
            let blocked = blocked_by.is_some();
            if drop.pulled_by.is_some() || drop.blocked != blocked {
                self.entities.update(id, |entity| {
                    if let EntityState::Drop(drop) = &mut entity.state {
                        drop.pulled_by = None;
                        drop.blocked = blocked;
                    }
                });
            }
        }
        self.entities.despawn_many(&collected);
        for (player_id, stowage) in newly_full {
            output.push_event(GameplayEventKind::PickupBlocked { player_id, stowage });
        }
    }

    /// [`PlayerCommand::DumpBlocks`]: tips the hopper's blocks (one kind, or all) out beside
    /// the rig as drops that rig will not pick up again.
    pub(super) fn dump_blocks(
        &mut self,
        player_id: PlayerId,
        block: Option<BlockKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let kinds: Vec<BlockKind> = block.map_or_else(
            || player.block_hopper.blocks().map(|(kind, _)| kind).collect(),
            |kind| vec![kind],
        );
        let (x, y) = (player.x, player.y);
        let mut dumped = Vec::new();
        for kind in kinds {
            let count = player.block_hopper.take_all(kind);
            if count > 0 {
                dumped.push(ItemStack::new(ItemKind::Block(kind), count));
            }
        }
        if dumped.is_empty() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        for (index, stack) in dumped.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a handful of block kinds")]
            let spread = (index as f32).mul_add(30.0, -45.0);
            let _ = self.spawn_loot(x, y, (spread, -140.0), stack, Some(player_id));
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// [`PlayerCommand::SetCollectBlocks`]: the rig's "collect blocks" toggle.
    pub(super) fn set_collect_blocks(
        &mut self,
        player_id: PlayerId,
        enabled: bool,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        player.collect_blocks = enabled;
        PlayerScopedCommandOutcome::Applied
    }

    /// The depot buys every block in the hopper ([`block_sale_value`] each).
    pub(super) fn sell_blocks(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let value = player
            .block_hopper
            .blocks()
            .fold(0_u32, |sum, (block, count)| {
                sum.saturating_add(block_sale_value(block).saturating_mul(count))
            });
        if player.block_hopper.is_empty() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.block_hopper.clear();
        player.credits = player.credits.saturating_add(value);
        PlayerScopedCommandOutcome::Applied
    }

    /// Drops within `radius` world pixels of `(x, y)`, in id order (presentation and tests).
    #[must_use]
    pub fn drops_near(&self, x: f32, y: f32, radius: f32) -> Vec<(EntityId, Body, DropState)> {
        self.entities
            .ids_in_radius(x, y, radius)
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                match entity.state {
                    EntityState::Drop(drop) => Some((id, entity.body, drop)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Every drop, in id order.
    pub fn drops(&self) -> impl Iterator<Item = (EntityId, &Body, DropState, SimulationTick)> {
        self.entities
            .of_kind(EntityKind::Drop)
            .filter_map(|entity| match entity.state {
                EntityState::Drop(drop) => Some((entity.id, &entity.body, drop, entity.spawn_tick)),
                _ => None,
            })
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "world pixels are far inside i32 for event positions"
)]
const fn event_pixel(value: f32) -> i32 {
    value.round() as i32
}
