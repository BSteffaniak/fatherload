//! The world's entities: spawning, the per-tick entity phases, waking, blasts and entity
//! replication (see [`crate::entity`] for the store and [`crate::physics`] for motion).
//!
//! # Tick
//!
//! `simulate_tick` runs [`EntityPhase::Early`] right after inputs (gas clouds age before hazard
//! damage reads them) and [`EntityPhase::Late`] after drilling, bombs and hazard spawns (expiry
//! first, then boulders and drops). Within a phase, awake entities update in id order.
//! Detonations push blast-feeling entities ([`EntityKind::feels_blasts`]) inside the bomb pass
//! (`push_from_blasts`, with every explosive kind's reach and push). At the end of the tick,
//! every tile the tick changed wakes the sleeping entities around it, so a drop resting on a
//! dug-out tile falls on the next tick.
//!
//! # Adding a kind (Wave 2)
//!
//! 1. Add a variant to [`crate::entity::EntityState`] / [`EntityKind`] with its phase, sleep
//!    and blast rules.
//! 2. Add its update to [`WorldState::advance_entities`]'s match (and a spawn helper here).
//! 3. Use [`crate::physics`] for motion; mutate only through [`EntityStore::update`].
//!
//! Replication, saves, keyframes, hashing and relevance then cover it automatically.

use crate::{
    entity::{
        Body, BoulderState, DropState, Entity, EntityDelta, EntityId, EntityKind, EntityPhase,
        EntityState, EntityStore, GasCloudState,
    },
    event::{GameplayEventKind, TickOutput},
    hazard::{FallingBoulder, HazardCloud},
    ids::SimulationTick,
    item::{BlockKind, ItemKind, ItemStack},
    physics::{self, Blast},
    quantize::quantize_simulation_scalar,
    terrain::{TILE_SIZE, TileChange},
};

use super::{
    WorldState,
    hazards::{BOULDER_HIT_DAMAGE, BOULDER_RADIUS},
    movement::{AUTHORITATIVE_GRAVITY, AUTHORITATIVE_PLAYER_RADIUS},
};

/// A sleep-capable body that has rested this long falls asleep.
pub const ENTITY_SLEEP_AFTER_MILLIS: u64 = 300;
/// Sleeping entities this close (a square, in pixels) to a changed tile's centre wake up.
const TERRAIN_WAKE_REACH: f32 = TILE_SIZE * 1.5;

impl WorldState {
    /// The world's entities.
    #[must_use]
    pub const fn entities(&self) -> &EntityStore {
        &self.entities
    }

    /// Mutable entities for building a scenario world before play (test harnesses, scene
    /// capture); gameplay only changes entities inside [`Self::simulate_tick`].
    pub const fn entities_mut(&mut self) -> &mut EntityStore {
        &mut self.entities
    }

    /// Spawns an entity now (inside a tick: during the transition to the next tick).
    pub(crate) fn spawn_entity(
        &mut self,
        body: Body,
        state: EntityState,
        expires_at: Option<SimulationTick>,
    ) -> EntityId {
        self.entities
            .spawn(self.simulation_tick, body, state, expires_at)
    }

    /// Spawns a gas cloud.
    pub(crate) fn spawn_gas_cloud(&mut self, cloud: HazardCloud) -> EntityId {
        spawn_gas_cloud_into(&mut self.entities, self.simulation_tick, cloud)
    }

    /// Spawns a falling boulder with its tick deadlines.
    pub(crate) fn spawn_boulder(
        &mut self,
        boulder: FallingBoulder,
        warning_deadline: SimulationTick,
        expiry_tick: SimulationTick,
    ) -> EntityId {
        self.spawn_entity(
            Body::at(boulder.x, boulder.y).moving(0.0, boulder.velocity_y),
            EntityState::Boulder(BoulderState {
                warning_seconds: boulder.warning_seconds,
                life: boulder.life,
                warning_deadline,
                expiry_tick,
            }),
            None,
        )
    }

    /// Spawns a floating item stack at `(x, y)` moving at `(velocity_x, velocity_y)`, which
    /// despawns after `lifetime_millis` (none: never;
    /// [`crate::world::drops::drop_lifetime_millis`] gives each kind's usual lifetime). Rigs
    /// collect it like any drop (`world/drops.rs`).
    pub fn spawn_item_drop(
        &mut self,
        x: f32,
        y: f32,
        velocity: (f32, f32),
        stack: ItemStack,
        lifetime_millis: Option<u64>,
    ) -> EntityId {
        let expires_at = lifetime_millis.map(|millis| {
            SimulationTick::new(
                self.simulation_tick
                    .get()
                    .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
            )
        });
        self.spawn_entity(
            Body::at(x, y).moving(velocity.0, velocity.1),
            EntityState::Drop(DropState::new(stack)),
            expires_at,
        )
    }

    /// Gas clouds, in spawn order.
    #[must_use]
    pub fn hazards(&self) -> Vec<HazardCloud> {
        self.entities
            .of_kind(EntityKind::GasCloud)
            .filter_map(|entity| match entity.state {
                EntityState::GasCloud(cloud) => Some(HazardCloud {
                    x: entity.body.x,
                    y: entity.body.y,
                    life: cloud.life,
                    radius: cloud.radius,
                }),
                _ => None,
            })
            .collect()
    }

    /// Falling boulders, in spawn order.
    #[must_use]
    pub fn falling_boulders(&self) -> Vec<FallingBoulder> {
        self.entities
            .of_kind(EntityKind::Boulder)
            .filter_map(|entity| match entity.state {
                EntityState::Boulder(boulder) => Some(FallingBoulder {
                    x: entity.body.x,
                    y: entity.body.y,
                    velocity_y: entity.body.velocity_y,
                    warning_seconds: boulder.warning_seconds,
                    life: boulder.life,
                }),
                _ => None,
            })
            .collect()
    }

    /// Adds gas clouds and boulders from the pre-entity save format (index-aligned boulder
    /// deadline lists; a missing deadline means "now").
    pub fn restore_legacy_live_entities(
        &mut self,
        hazards: Vec<HazardCloud>,
        boulders: Vec<FallingBoulder>,
        warning_deadlines: &[SimulationTick],
        expiry_ticks: &[SimulationTick],
    ) {
        for cloud in hazards {
            self.spawn_gas_cloud(cloud);
        }
        let now = self.simulation_tick;
        for (index, boulder) in boulders.into_iter().enumerate() {
            self.spawn_boulder(
                boulder,
                warning_deadlines.get(index).copied().unwrap_or(now),
                expiry_ticks.get(index).copied().unwrap_or(now),
            );
        }
    }

    /// Runs one entity phase of the tick producing `next_tick`.
    pub(super) fn advance_entities(
        &mut self,
        phase: EntityPhase,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if phase == EntityPhase::Late {
            let _expired = self.entities.take_expired(next_tick);
        }
        // Drops merged away during this phase (never merged into again).
        let mut merged = std::collections::BTreeSet::new();
        for id in self.entities.awake_in_phase(phase) {
            let Some(entity) = self.entities.get(id) else {
                continue;
            };
            let kind = entity.kind();
            // F11: outside every awake region the entity is frozen (kept, not ticked).
            if self.frozen_at(entity.body.x, entity.body.y) {
                continue;
            }
            match kind {
                EntityKind::GasCloud => self.advance_gas_cloud(id),
                EntityKind::Boulder => self.advance_boulder(id, next_tick, output),
                EntityKind::Drop => self.advance_drop(id, &mut merged),
                EntityKind::Enemy => self.advance_enemy(id, next_tick, output),
                EntityKind::EnemyShot => self.advance_enemy_shot(id, next_tick, output),
                EntityKind::Projectile => self.advance_projectile(id, next_tick, output),
                // A critter's motion is a pure function of time (`CritterState::offset_at`);
                // its population is kept by `advance_critters` (`world/caves.rs`). Charges move
                // in the bomb pass (`EntityPhase::Charges`), never here; practice targets stand
                // still.
                // Pilots move in their own pass with their owners' controls (`world/pilot.rs`);
                // people walk in `advance_people` (`world/people.rs`), settlement by settlement.
                EntityKind::Critter
                | EntityKind::Charge
                | EntityKind::Target
                | EntityKind::Pilot
                | EntityKind::Npc => {}
            }
        }
        // Merged-away drops (their stacks already moved onto their neighbours).
        self.entities.despawn_many(&merged);
        if phase == EntityPhase::Late {
            // WP-DROPS: magnets, pickups and "full" feedback (`world/drops.rs`).
            self.advance_drop_pickups(output);
        }
    }

    fn advance_gas_cloud(&mut self, id: EntityId) {
        let delta_seconds = self.timing.delta_seconds;
        let alive = self
            .entities
            .update(id, |entity| {
                let EntityState::GasCloud(cloud) = &mut entity.state else {
                    return true;
                };
                cloud.life -= delta_seconds;
                cloud.radius = 8.0f32.mul_add(delta_seconds, cloud.radius);
                cloud.life > 0.0
            })
            .unwrap_or(true);
        if !alive {
            self.entities.despawn(id);
        }
    }

    /// A boulder hangs until its warning deadline, then falls; it shatters on a rig (damaging
    /// it, less with the boulder deflector) or when its lower edge meets solid ground, and
    /// crumbles at its expiry. Spawns only hang boulders in open ground, so nothing shatters
    /// without falling onto something.
    fn advance_boulder(
        &mut self,
        id: EntityId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let timing = self.timing;
        let Some((mut body, state)) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Boulder(state) => Some((entity.body, state)),
            _ => None,
        }) else {
            return;
        };
        // Interpolation hook: the step starts here (gas clouds and hanging boulders keep still).
        body.previous_x = body.x;
        body.previous_y = body.y;
        if next_tick >= state.expiry_tick {
            self.entities.despawn(id);
            return;
        }
        if next_tick >= state.warning_deadline {
            // Gravity fields (F6): exactly 1.0 underground, where boulders hang.
            let field = crate::environment::environment_in(
                self.terrain.settings(),
                self.frame(),
                body.x,
                body.y,
            )
            .gravity;
            body.velocity_y = quantize_simulation_scalar(
                (AUTHORITATIVE_GRAVITY * 0.8 * field)
                    .mul_add(timing.delta_seconds, body.velocity_y)
                    .min(520.0),
            );
            body.y =
                quantize_simulation_scalar(body.velocity_y.mul_add(timing.delta_seconds, body.y));
            let hit_player = self.players.iter().find_map(|(player_id, player)| {
                let dx = player.x - body.x;
                let dy = player.y - body.y;
                (dx.mul_add(dx, dy * dy) <= (AUTHORITATIVE_PLAYER_RADIUS + 8.0).powi(2))
                    .then_some((*player_id, player.hazard_gear))
            });
            if let Some((player_id, gear)) = hit_player {
                // The boulder shatters on the rig; the deflector turns aside its weight.
                self.entities.despawn(id);
                if self.damage_player(player_id, gear.boulder_damage(BOULDER_HIT_DAMAGE)) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                return;
            }
            #[allow(
                clippy::cast_possible_truncation,
                reason = "bounded boulder coordinates intentionally map to terrain tiles"
            )]
            let tile = self.frame().tile_at(body.x, body.y + BOULDER_RADIUS);
            if self.terrain.is_solid_at(tile) {
                // Its lower edge meets the ground: it shatters there, leaving a stone block of
                // rubble (WP-DROPS) on top of what it hit.
                self.entities.despawn(id);
                #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
                let ground = self.frame().rel_row(tile.y) * TILE_SIZE;
                let _rubble = self.spawn_loot(
                    body.x,
                    (ground - BOULDER_RADIUS).min(body.y),
                    (0.0, -60.0),
                    ItemStack::new(ItemKind::Block(BlockKind::Stone), 1),
                    None,
                );
                return;
            }
        }
        self.entities.update(id, |entity| {
            entity.body = body;
            if let EntityState::Boulder(boulder) = &mut entity.state {
                boulder.warning_seconds =
                    timing.rate.seconds_until(next_tick, state.warning_deadline);
                boulder.life = timing.rate.seconds_until(next_tick, state.expiry_tick);
            }
        });
    }

    /// Pushes every blast-feeling entity in reach of `blast` (in id order; waking it).
    pub(crate) fn push_entities_with_blast(&mut self, blast: &Blast) {
        let terrain = &self.terrain;
        for id in self.entities.ids_in_radius(blast.x, blast.y, blast.reach) {
            let feels = self
                .entities
                .get(id)
                .is_some_and(|entity| entity.kind().feels_blasts());
            if !feels {
                continue;
            }
            self.entities.update(id, |entity| {
                physics::apply_blast(&mut entity.body, blast, terrain, id.get() % 2 == 1)
            });
        }
    }

    /// Wakes sleeping entities around every changed tile.
    pub(super) fn wake_entities_near(&mut self, changes: &[TileChange]) {
        if self.entities.awake_count() == self.entities.len() {
            return;
        }
        for change in changes {
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let (x, y) = (
                (self.frame().rel_col(change.position.x) + 0.5) * TILE_SIZE,
                (self.frame().rel_row(change.position.y) + 0.5) * TILE_SIZE,
            );
            self.entities.wake_around(x, y, TERRAIN_WAKE_REACH);
        }
    }

    /// `(id, revision)` of the entities within `radius` tiles of `center` (tile units; `None`:
    /// every entity), in id order: what a client should hold. A client holding the same pairs
    /// is up to date.
    #[must_use]
    pub fn entity_revisions(
        &self,
        center: Option<(f32, f32)>,
        radius: Option<f32>,
    ) -> Vec<(EntityId, u64)> {
        match (center, radius) {
            (Some((cx, cy)), Some(radius)) => {
                let (x, y, reach) = (cx * TILE_SIZE, cy * TILE_SIZE, radius * TILE_SIZE);
                self.entities.revisions(
                    Some((x - reach, y - reach, x + reach, y + reach)),
                    |ex, ey| (ex - x).hypot(ey - y) <= reach,
                )
            }
            _ => self.entities.revisions(None, |_, _| true),
        }
    }

    /// One entity's replication record.
    #[must_use]
    pub fn entity_record(&self, id: EntityId) -> Option<Entity> {
        self.entities.get(id).cloned()
    }

    /// Applies replicated entity spawns, changes and despawns.
    pub fn apply_entity_delta(&mut self, delta: EntityDelta) {
        self.entities.apply_delta(delta);
    }

    /// Every entity as one delta (a full copy for a replica that has none).
    #[must_use]
    pub fn full_entity_delta(&self) -> EntityDelta {
        EntityDelta {
            updates: self.entities.iter().cloned().collect(),
            despawned: Vec::new(),
        }
    }
}

/// Spawns a gas cloud into `store` (for callers holding another borrow of the world).
pub(super) fn spawn_gas_cloud_into(
    store: &mut EntityStore,
    tick: SimulationTick,
    cloud: HazardCloud,
) -> EntityId {
    store.spawn(
        tick,
        Body::at(cloud.x, cloud.y),
        EntityState::GasCloud(GasCloudState {
            life: cloud.life,
            radius: cloud.radius,
        }),
        None,
    )
}
