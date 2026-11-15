//! Enemies in the world (WP-ENEMIES): spawning, AI, attacks on rigs, the damage API, blasts and
//! deaths. The data (kinds, spec, habitats, loot, the director state) lives in
//! [`crate::enemy`]; enemies and their shots are entities of the entity store.
//!
//! # Tick
//!
//! * `advance_enemy_director` runs once per tick right before the late entity phase. With a
//!   hostile difficulty it periodically despawns enemies far from every rig, spawns roaming
//!   enemies in chunks around rigs (in caves, in the dark, out of view, outside safe zones, under
//!   per-rig caps and per-chunk cooldowns) and fills lair caverns near rigs with their occupant
//!   (nests, turrets, elites). Peaceful removes every hostile.
//! * Each enemy updates in the late entity phase ([`WorldState::advance_enemy`]): perception
//!   (sight with line of sight, drilling vibration through rock, flares luring bats) every
//!   [`THINK_MILLIS`], a small state machine ([`EnemyMode`]: idle, chase, wind-up, strike,
//!   recover, return home on the leash, flee, EMP stun, dormant mimics), movement by
//!   locomotion (flying, walking with step hops, burrowing through soft rock, stationary) with
//!   budgeted path searches ([`physics::find_tile_path`], at most one per enemy every
//!   [`PATH_SLOT_TICKS`] ticks, staggered by id) and contact attacks.
//! * Shots ([`WorldState::advance_enemy_shot`]) fly, fall (acid) and hit rock or rigs.
//!
//! # Fairness
//!
//! Rigs inside a safe zone ([`WorldState::enemy_safe_zone`]: the surface strip, the town and
//! its shafts, around infrastructure and bases) are never targeted or hurt by contact, nothing
//! spawns there or in view of a rig, and a rig that was hit is invulnerable for
//! [`RIG_GUARD_MILLIS`] (i-frames) while it is knocked back. Test-mode invulnerability and
//! no-clip (god mode) make rigs immune: enemies pass through harmlessly.
//!
//! # Damage API (for WP-WEAPONS)
//!
//! [`WorldState::damage_entity`] applies a [`Damage`] (amount, [`DamageKind`], source,
//! impulse) to an entity and returns a [`DamageOutcome`]. It applies the target's resistance,
//! shields, EMP stuns, burning, aggro toward the attacking player, flinch and fleeing, and on a
//! kill drops the loot (`spawn_item_drop`), clears the lair and emits
//! [`GameplayEventKind::EnemyKilled`]. Call it from inside the tick (projectiles, melee, beams);
//! [`WorldState::enemies_near`] finds targets.

use crate::{
    enemy::{
        AttackStyle, Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty,
        EnemyDirector, EnemyKind, EnemyMode, EnemyShotState, EnemyState, Locomotion, ShotKind,
        band_roster, hazard_density, lair_occupant, loot, tier_scale, tile_centre,
    },
    entity::{Body, Entity, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::{ExplosiveKind, blast_impulse},
    ids::{PlayerId, SimulationTick},
    physics::{self, BodyParams},
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    surface::town_span,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, TILE_SIZE, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{WorldState, bombs::Blast, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// A rig hit by an enemy cannot be hit by another for this long (i-frames).
pub const RIG_GUARD_MILLIS: u64 = 800;
/// How often enemies look and listen for rigs.
pub const THINK_MILLIS: u64 = 150;
/// An enemy may search for a path at most once every this many ticks (staggered by id), so a
/// tick runs only a few searches however many enemies there are.
pub const PATH_SLOT_TICKS: u64 = 12;
/// Tiles a path search may explore.
const PATH_NODES: usize = 360;
/// Earliest repeat of an enemy's path search.
const PATH_RETRY_MILLIS: u64 = 700;
/// How often the director spawns roaming enemies and despawns far ones.
pub const SPAWN_CHECK_MILLIS: u64 = 1_000;
/// How often the director fills lair caverns.
const LAIR_CHECK_MILLIS: u64 = 2_000;
/// Rows below the surface where the dark (and enemies) begin.
pub const MIN_SPAWN_DEPTH_ROWS: i32 = 10;
/// Rows below the surface that stay safe.
pub const SAFE_SURFACE_ROWS: i32 = 6;
/// Columns either side of the town that stay safe down to [`TOWN_SAFE_ROWS`].
const TOWN_SAFE_MARGIN: i32 = 12;
/// Rows below the surface the town's safety reaches.
const TOWN_SAFE_ROWS: i32 = 16;
/// Tiles around placed infrastructure (outposts, bases) that stay safe.
pub const BASE_SAFE_TILES: i32 = 7;
/// Tiles around a village's area (surface village or underground hall, WP-VILLAGE) that stay
/// safe.
pub const VILLAGE_SAFE_TILES: i32 = 10;
/// A chunk centred in a cave with room for an elite lair (WP-CAVES `elite_lair`) is a lair one
/// time in this many (on top of the plain [`LAIR_ODDS`] caverns).
const CAVE_LAIR_ODDS: u64 = 3;
/// A roster kind that fits a cave's spawn tags is this many times as likely there.
const CAVE_HABITAT_WEIGHT: u32 = 3;
/// Spawns stay out of every rig's view: at least this many columns or rows away.
const VIEW_HALF_TILES: (i32, i32) = (21, 12);
/// Roaming spawns land this far from the rig they spawn for, in tiles.
const SPAWN_DISTANCE_TILES: (i32, i32) = (22, 30);
/// Enemies counted toward a rig's local cap within this many tiles.
const LOCAL_RADIUS_TILES: f32 = 32.0;
/// Enemies farther than this from every rig leave the world.
pub const DESPAWN_TILES: f32 = 64.0;
/// Most enemies alive at once.
pub const MAX_ENEMIES: usize = 64;
/// Most enemy shots in flight at once.
const MAX_SHOTS: usize = 48;
/// A chunk that spawned may not spawn again for this long.
const CHUNK_COOLDOWN_MILLIS: u64 = 40_000;
/// One chunk in this many (in a band with lairs, holding a cavern) is a lair.
const LAIR_ODDS: u64 = 7;
/// A rig out of sight and earshot this long is forgotten.
const LOSE_TARGET_MILLIS: u64 = 4_000;
/// How long a lunge or charge lasts.
const STRIKE_MILLIS: u64 = 450;
/// How long an enemy backs off after an attack.
const RECOVER_MILLIS: u64 = 500;
/// How long an EMP knocks a machine out (elites half as long).
pub const EMP_STUN_MILLIS: u64 = 4_000;
/// EMP reach, pixels (it passes through rock).
pub const EMP_RADIUS: f32 = TILE_SIZE * 4.5;
/// EMP damage to machines.
const EMP_DAMAGE: f32 = 25.0;
/// How long fire keeps burning a creature.
const BURN_MILLIS: u64 = 3_000;
/// Fire damage per second while burning.
const BURN_DAMAGE_PER_SECOND: f32 = 6.0;
/// How long a damage flash lasts (presentation).
const HURT_FLASH_MILLIS: u64 = 160;
/// How long a badly hurt creature runs.
const FLEE_MILLIS: u64 = 2_000;
/// How far a lit flare lures bats, pixels.
const FLARE_LURE: f32 = TILE_SIZE * 10.0;
/// Most bats a nest keeps alive.
const NEST_BROOD: usize = 4;
/// Loot drops last this long.
const LOOT_LIFETIME_MILLIS: u64 = 300_000;
/// Gravity on walkers, falling flyers and burrowers in open air.
const ENEMY_GRAVITY: f32 = 560.0;
/// Speed a walker hops a one-tile step with.
const HOP_SPEED: f32 = 250.0;
/// Fastest push a hit leaves a rig with (under the safe landing speed: no crash damage).
const RIG_KNOCKBACK_MAX: f32 = 260.0;
/// How often an excavator bites a tile out of its way.
const DIG_MILLIS: u64 = 350;

/// `(cos, sin)` turns of a three-shot volley's spread (about ±10°), no trigonometry.
const VOLLEY_TURNS: [(f32, f32); 3] = [(0.984_808, -0.173_648), (1.0, 0.0), (0.984_808, 0.173_648)];
/// Sixteen directions around a ring (16 = unit length) for picking spawn spots.
const RING: [(i32, i32); 16] = [
    (16, 0),
    (15, 6),
    (11, 11),
    (6, 15),
    (0, 16),
    (-6, 15),
    (-11, 11),
    (-15, 6),
    (-16, 0),
    (-15, -6),
    (-11, -11),
    (-6, -15),
    (0, -16),
    (6, -15),
    (11, -11),
    (15, -6),
];

/// Salts for the director's hashes.
const SALT_SPAWN: u64 = 0xE7E1_0001;
const SALT_SITE: u64 = 0xE7E1_0002;
const SALT_LAIR: u64 = 0xE7E1_0003;
const SALT_WANDER: u64 = 0xE7E1_0004;
const SALT_LOOT: u64 = 0xE7E1_0005;
const SALT_SEED: u64 = 0xE7E1_0006;

/// Where an enemy wants to go this tick.
#[derive(Clone, Copy, Debug, Default)]
struct Intent {
    goal: Option<(f32, f32)>,
    speed: f32,
}

/// The tile holding world pixel `(x, y)`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "world pixels saturate into tile coordinates"
)]
fn tile_of(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

/// Unit vector from `from` to `to` and the distance (straight up for coincident points).
fn direction(from: (f32, f32), to: (f32, f32)) -> ((f32, f32), f32) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let distance = dx.hypot(dy);
    if distance < 0.5 {
        ((0.0, -1.0), distance)
    } else {
        ((dx / distance, dy / distance), distance)
    }
}

/// Rock a burrower swims through.
const fn burrowable(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock | TileKind::Ore(_)
    )
}

/// Rock an excavator bites away.
const fn diggable(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

/// The motion constants of a flying or walking enemy of `kind` (stunned flyers fall).
const fn body_params(kind: EnemyKind, falling: bool) -> BodyParams {
    let spec = kind.stats();
    let walking = matches!(spec.locomotion, Locomotion::Walking);
    BodyParams {
        radius: spec.radius,
        gravity: if walking || falling {
            ENEMY_GRAVITY
        } else {
            0.0
        },
        max_speed: 480.0,
        bounce: if walking { 0.0 } else { 0.35 },
        rest_speed: 60.0,
        floor_friction: if walking { 0.0 } else { 4.0 },
        stop_speed: 0.0,
    }
}

impl WorldState {
    /// The world's enemy setting.
    #[must_use]
    pub const fn enemy_difficulty(&self) -> EnemyDifficulty {
        self.enemies.difficulty
    }

    /// The enemy director (difficulty, rig i-frames, cooldowns, cleared lairs).
    #[must_use]
    pub const fn enemy_director(&self) -> &EnemyDirector {
        &self.enemies
    }

    /// Sets the enemy setting before play (world setup); in play it changes through
    /// [`crate::PlayerCommand::SetEnemyDifficulty`].
    pub fn set_enemy_difficulty(&mut self, difficulty: EnemyDifficulty) {
        self.enemies.difficulty = difficulty;
        if !difficulty.hostile() {
            self.clear_hostiles();
        }
    }

    pub(super) fn apply_enemy_difficulty(
        &mut self,
        difficulty: EnemyDifficulty,
    ) -> PlayerScopedCommandOutcome {
        if self.enemies.difficulty == difficulty {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        self.set_enemy_difficulty(difficulty);
        PlayerScopedCommandOutcome::Applied
    }

    /// Every live enemy with its state, in id order.
    pub fn enemies(&self) -> impl Iterator<Item = (&Entity, &EnemyState)> + '_ {
        self.entities
            .of_kind(EntityKind::Enemy)
            .filter_map(|entity| match &entity.state {
                EntityState::Enemy(state) => Some((entity, state)),
                _ => None,
            })
    }

    /// Ids of the enemies whose centre is within `radius` pixels of `(x, y)`, in id order.
    #[must_use]
    pub fn enemies_near(&self, x: f32, y: f32, radius: f32) -> Vec<EntityId> {
        self.entities
            .ids_in_radius(x, y, radius)
            .into_iter()
            .filter(|id| {
                self.entities
                    .get(*id)
                    .is_some_and(|entity| entity.kind() == EntityKind::Enemy)
            })
            .collect()
    }

    /// Seconds left of `player`'s post-hit invulnerability (presentation: the rig blinks).
    #[must_use]
    pub fn rig_guard_seconds(&self, player: PlayerId) -> f32 {
        self.enemies.rig_guard.get(&player).map_or(0.0, |until| {
            self.timing.rate.seconds_until(self.simulation_tick, *until)
        })
    }

    /// Whether enemies leave `tile` alone: the surface strip and the sky, the town and the top
    /// of its shafts, and around placed infrastructure (outposts and bases). The one place to
    /// add villages and claimed bases (WP-VILLAGE, WP-BASE).
    #[must_use]
    pub fn enemy_safe_zone(&self, tile: TilePosition) -> bool {
        let surface = self.terrain.surface_row(tile.x);
        if tile.y < surface + SAFE_SURFACE_ROWS {
            return true;
        }
        let (first, last) = town_span();
        if tile.x >= first - TOWN_SAFE_MARGIN
            && tile.x <= last + TOWN_SAFE_MARGIN
            && tile.y < surface + TOWN_SAFE_ROWS
        {
            return true;
        }
        if self.infrastructure.iter().any(|item| {
            (item.position.x - tile.x).abs() <= BASE_SAFE_TILES
                && (item.position.y - tile.y).abs() <= BASE_SAFE_TILES
        }) {
            return true;
        }
        crate::worldgen::streamed::in_village_area(
            self.terrain.settings(),
            tile,
            VILLAGE_SAFE_TILES,
        )
    }

    /// Spawns an enemy of `kind` centred at world pixel `(x, y)`, at the depth tier of that
    /// spot (scenario setup and tests; gameplay spawns through the director).
    pub fn spawn_enemy(&mut self, kind: EnemyKind, x: f32, y: f32) -> EntityId {
        let tier = self.tier_at(tile_of(x, y));
        self.spawn_enemy_with(kind, (x, y), tier, None, None)
    }

    fn tier_at(&self, tile: TilePosition) -> u8 {
        u8::try_from(self.terrain.band_at(tile).index()).unwrap_or(u8::MAX)
    }

    /// Health multiplier for the crew size: every extra rig adds 30 %, up to 2.2x.
    #[allow(clippy::cast_precision_loss, reason = "a handful of players")]
    fn crew_health_scale(&self) -> f32 {
        let extra = self.players.len().saturating_sub(1) as f32;
        extra.mul_add(0.3, 1.0).min(2.2)
    }

    fn spawn_enemy_with(
        &mut self,
        kind: EnemyKind,
        (x, y): (f32, f32),
        tier: u8,
        lair: Option<ChunkPosition>,
        parent: Option<EntityId>,
    ) -> EntityId {
        let now = self.simulation_tick;
        let home = tile_of(x, y);
        let seed = u32::try_from(
            hash2(
                self.terrain.seed(),
                SALT_SEED,
                i32::try_from(self.entities.next_id() & 0x7FFF_FFFF).unwrap_or(0),
                home.y,
            ) & 0xFFFF_FFFF,
        )
        .unwrap_or(0);
        let health_scale = self.enemies.difficulty.health() * self.crew_health_scale();
        let mut state = EnemyState::new(kind, tier, (home.x, home.y), health_scale, seed, now);
        state.lair = lair;
        state.parent = parent.map(EntityId::get);
        self.spawn_entity(
            Body::at(quantize(x), quantize(y)),
            EntityState::Enemy(state),
            None,
        )
    }

    /// Removes every enemy and enemy shot (Peaceful).
    fn clear_hostiles(&mut self) {
        let ids: std::collections::BTreeSet<EntityId> = self
            .entities
            .ids_of(EntityKind::Enemy)
            .chain(self.entities.ids_of(EntityKind::EnemyShot))
            .collect();
        if !ids.is_empty() {
            let _gone = self.entities.despawn_many(&ids);
        }
    }

    fn rig_tiles(&self) -> Vec<(PlayerId, TilePosition)> {
        self.players
            .iter()
            .map(|(id, player)| (*id, tile_of(player.x, player.y)))
            .collect()
    }

    /// The director's pass: despawning, roaming spawns and lairs (see the module docs).
    pub(super) fn advance_enemy_director(&mut self, next_tick: SimulationTick) {
        if !self.enemies.difficulty.hostile() {
            if self.entities.count_of(EntityKind::Enemy)
                + self.entities.count_of(EntityKind::EnemyShot)
                > 0
            {
                self.clear_hostiles();
            }
            return;
        }
        if self.players.is_empty() {
            return;
        }
        let rate = self.timing.rate;
        if rate.schedule_fires(next_tick, SPAWN_CHECK_MILLIS, 0) {
            let _changed = self.enemies.prune(next_tick);
            self.despawn_far_enemies();
            self.spawn_roaming_enemies(next_tick);
        }
        if rate.schedule_fires(next_tick, LAIR_CHECK_MILLIS, 500) {
            self.fill_lairs();
        }
    }

    fn despawn_far_enemies(&mut self) {
        let rigs: Vec<(f32, f32)> = self.players.values().map(|p| (p.x, p.y)).collect();
        let reach = DESPAWN_TILES * TILE_SIZE;
        let far: std::collections::BTreeSet<EntityId> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .chain(self.entities.of_kind(EntityKind::EnemyShot))
            .filter(|entity| {
                rigs.iter().all(|(x, y)| {
                    (entity.body.x - x).abs() > reach || (entity.body.y - y).abs() > reach
                })
            })
            .map(|entity| entity.id)
            .collect();
        if !far.is_empty() {
            let _gone = self.entities.despawn_many(&far);
        }
    }

    /// Whether no rig can see `tile` (it is outside every rig's view box).
    fn out_of_view(tile: TilePosition, rigs: &[(PlayerId, TilePosition)]) -> bool {
        rigs.iter().all(|(_, rig)| {
            (tile.x - rig.x).abs() > VIEW_HALF_TILES.0 || (tile.y - rig.y).abs() > VIEW_HALF_TILES.1
        })
    }

    fn spawn_roaming_enemies(&mut self, now: SimulationTick) {
        let difficulty = self.enemies.difficulty;
        let density = difficulty.density() * hazard_density(self.terrain.settings().hazards);
        let rigs = self.rig_tiles();
        let seed = self.terrain.seed();
        for (index, (player_id, tile)) in rigs.iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                break;
            }
            let surface = self.terrain.surface_row(tile.x);
            if tile.y - surface < MIN_SPAWN_DEPTH_ROWS || self.enemy_safe_zone(*tile) {
                continue;
            }
            let tier = self.tier_at(*tile);
            let (cx, cy) = tile_centre(tile.x, tile.y);
            let local = self
                .enemies_near(cx, cy, LOCAL_RADIUS_TILES * TILE_SIZE)
                .len();
            let cap = 3 + usize::from(tier) / 2 + difficulty.extra_cap();
            if local >= cap.min(10) {
                continue;
            }
            let roll = hash2(
                seed ^ now.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_SPAWN,
                i32::try_from(player_id.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(index).unwrap_or(0),
            );
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a per-mille chance in 0..=1000"
            )]
            let chance =
                (15.0f32.mul_add(f32::from(tier), 120.0) * density).clamp(0.0, 1000.0) as u64;
            if roll % 1000 >= chance {
                continue;
            }
            let (dx, dy) = RING[usize::try_from((roll >> 12) % 16).unwrap_or(0)];
            let span = SPAWN_DISTANCE_TILES.1 - SPAWN_DISTANCE_TILES.0;
            let distance = SPAWN_DISTANCE_TILES.0
                + i32::try_from((roll >> 20) % u64::try_from(span).unwrap_or(1)).unwrap_or(0);
            // Views are wider than tall: squash the ring vertically.
            let target = TilePosition {
                x: tile.x + dx * distance / 16,
                y: tile.y + dy * distance * 10 / (16 * 16),
            };
            let chunk = ChunkPosition::of(target);
            if self.enemies.chunk_cooling(chunk, now) {
                continue;
            }
            if let Some((kind, site)) = self.find_spawn_site(chunk, roll, &rigs) {
                self.spawn_pack(kind, site, roll);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss,
                    reason = "bounded milliseconds"
                )]
                let cooldown = (CHUNK_COOLDOWN_MILLIS as f32 / density.max(0.25)) as u64;
                let until = SimulationTick::new(
                    now.get()
                        .saturating_add(self.timing.ticks_for_millis(cooldown)),
                );
                self.enemies.cool_chunk(chunk, until);
            }
        }
    }

    /// Air tiles in the 5x5 square around `tile` (a cave, not a drilled shaft).
    fn open_area(&self, tile: TilePosition) -> usize {
        (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| {
                let position = TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                };
                self.terrain.contains_column(position.x)
                    && self
                        .terrain
                        .tile(position)
                        .is_some_and(|t| t.kind == TileKind::Air)
            })
            .count()
    }

    fn air(&self, tile: TilePosition) -> bool {
        self.terrain.contains_column(tile.x)
            && self
                .terrain
                .tile(tile)
                .is_some_and(|t| t.kind == TileKind::Air)
    }

    fn floor(&self, tile: TilePosition) -> bool {
        self.terrain.tile(tile).is_some_and(|t| {
            !matches!(
                t.kind,
                TileKind::Air | TileKind::Lava | TileKind::Gas | TileKind::MagmaVent
            )
        })
    }

    /// Whether an enemy of `kind` can appear at `tile`.
    fn site_fits(&self, kind: EnemyKind, tile: TilePosition) -> bool {
        let spec = kind.stats();
        let big = spec.radius > TILE_SIZE * 0.5;
        let above = TilePosition {
            x: tile.x,
            y: tile.y - 1,
        };
        let below = TilePosition {
            x: tile.x,
            y: tile.y + 1,
        };
        match spec.locomotion {
            Locomotion::Burrowing => self.terrain.tile(tile).is_some_and(|t| {
                matches!(t.kind, TileKind::Dirt | TileKind::Clay | TileKind::Stone)
            }),
            Locomotion::Flying => {
                self.air(tile) && self.open_area(tile) >= if big { 20 } else { 12 }
            }
            Locomotion::Walking | Locomotion::Stationary => {
                self.air(tile)
                    && self.air(above)
                    && self.floor(below)
                    && (!big
                        || (self.air(TilePosition {
                            x: tile.x - 1,
                            y: tile.y,
                        }) && self.air(TilePosition {
                            x: tile.x + 1,
                            y: tile.y,
                        })))
                    && self.open_area(tile) >= 12
            }
        }
    }

    /// A spot in `chunk` for one of the kinds roaming there (habitat by band, caves only).
    fn find_spawn_site(
        &self,
        chunk: ChunkPosition,
        roll: u64,
        rigs: &[(PlayerId, TilePosition)],
    ) -> Option<(EnemyKind, TilePosition)> {
        let origin = chunk.origin();
        let seed = self.terrain.seed();
        for attempt in 0..24_i32 {
            let hash = hash2(
                seed ^ roll,
                SALT_SITE,
                chunk.x.wrapping_mul(31) ^ attempt,
                chunk.y,
            );
            let tile = TilePosition {
                x: origin.x + i32::try_from(hash % 16).unwrap_or(0),
                y: origin.y + i32::try_from((hash >> 8) % 16).unwrap_or(0),
            };
            if !self.terrain.contains(tile) {
                continue;
            }
            let surface = self.terrain.surface_row(tile.x);
            if tile.y - surface < MIN_SPAWN_DEPTH_ROWS
                || self.enemy_safe_zone(tile)
                || !Self::out_of_view(tile, rigs)
            {
                continue;
            }
            // WP-CAVES: inside a cave ecosystem its spawn tags favour the kinds that live there
            // and its danger thins (or keeps) the spawns.
            let tags = self
                .terrain
                .cave_kind_at(tile)
                .map(crate::cave::CaveKind::spawn_tags);
            if let Some(tags) = tags
                && (hash >> 40) % 1000 >= u64::from(tags.danger_per_mille)
            {
                continue;
            }
            let roster = band_roster(self.terrain.band_at(tile));
            let weight_of = |kind: EnemyKind, weight: u32| {
                if tags.is_some_and(|tags| cave_habitat_fits(kind, tags)) {
                    weight * CAVE_HABITAT_WEIGHT
                } else {
                    weight
                }
            };
            let total: u32 = roster
                .iter()
                .map(|(kind, weight)| weight_of(*kind, *weight))
                .sum();
            if total == 0 {
                continue;
            }
            let mut pick = u32::try_from((hash >> 16) % u64::from(total)).unwrap_or(0);
            let mut kind = roster[0].0;
            for (candidate, weight) in roster {
                let weight = weight_of(*candidate, *weight);
                if pick < weight {
                    kind = *candidate;
                    break;
                }
                pick -= weight;
            }
            if self.site_fits(kind, tile) {
                return Some((kind, tile));
            }
        }
        None
    }

    /// The body centre of an enemy of `kind` standing in (or swimming through) `tile`.
    fn spawn_point(kind: EnemyKind, tile: TilePosition) -> (f32, f32) {
        let spec = kind.stats();
        let (x, y) = tile_centre(tile.x, tile.y);
        match spec.locomotion {
            Locomotion::Walking | Locomotion::Stationary => {
                #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
                let floor = (tile.y + 1) as f32 * TILE_SIZE;
                (x, floor - spec.radius - 1.0)
            }
            Locomotion::Flying | Locomotion::Burrowing => (x, y),
        }
    }

    fn spawn_pack(&mut self, kind: EnemyKind, site: TilePosition, roll: u64) {
        let tier = self.tier_at(site);
        let count = if kind == EnemyKind::CaveBat {
            3 + usize::try_from((roll >> 28) % 3).unwrap_or(0)
        } else {
            1
        };
        let offsets = [(0, 0), (1, 0), (-1, 0), (0, -1), (1, -1)];
        for (dx, dy) in offsets.into_iter().take(count) {
            let tile = TilePosition {
                x: site.x + dx,
                y: site.y + dy,
            };
            let tile = if self.air(tile) || kind != EnemyKind::CaveBat {
                tile
            } else {
                site
            };
            let point = Self::spawn_point(kind, tile);
            let _id = self.spawn_enemy_with(kind, point, tier, None, None);
        }
    }

    /// Fills lair caverns near rigs (out of view) with their occupant, unless cleared or
    /// already occupied.
    fn fill_lairs(&mut self) {
        let rigs = self.rig_tiles();
        let seed = self.terrain.seed();
        for (_, rig) in &rigs {
            if self.enemy_safe_zone(*rig) {
                continue;
            }
            let center = ChunkPosition::of(*rig);
            for cy in center.y - 2..=center.y + 2 {
                for cx in center.x - 2..=center.x + 2 {
                    if (cx - center.x).abs().max((cy - center.y).abs()) < 2 {
                        continue;
                    }
                    let chunk = ChunkPosition { x: cx, y: cy };
                    let hash = hash2(seed, SALT_LAIR, cx, cy);
                    if !(hash.is_multiple_of(LAIR_ODDS) || self.cave_lair_chunk(chunk, hash))
                        || self.enemies.cleared_lairs.contains(&chunk)
                    {
                        continue;
                    }
                    if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                        return;
                    }
                    let occupied = self.enemies().any(|(_, state)| state.lair == Some(chunk));
                    if occupied {
                        continue;
                    }
                    let origin = chunk.origin();
                    let middle = TilePosition {
                        x: origin.x + CHUNK_SIZE_TILES / 2,
                        y: origin.y + CHUNK_SIZE_TILES / 2,
                    };
                    if !self.terrain.contains(middle)
                        || middle.y - self.terrain.surface_row(middle.x) < MIN_SPAWN_DEPTH_ROWS
                    {
                        continue;
                    }
                    let Some(kind) = lair_occupant(self.terrain.band_at(middle), hash >> 8) else {
                        continue;
                    };
                    let Some(site) = self.lair_site(chunk, kind, hash, &rigs) else {
                        continue;
                    };
                    let tier = self.tier_at(site);
                    let _id = self.spawn_enemy_with(
                        kind,
                        Self::spawn_point(kind, site),
                        tier,
                        Some(chunk),
                        None,
                    );
                    // A guardian comes with drones, a lord with a few bats.
                    let escort = match kind {
                        EnemyKind::AncientGuardian => Some((EnemyKind::MiningDrone, 2)),
                        EnemyKind::BroodLord => Some((EnemyKind::CaveBat, 3)),
                        _ => None,
                    };
                    if let Some((escort, count)) = escort {
                        for index in 0..count {
                            let tile = TilePosition {
                                x: site.x - 1 + index,
                                y: site.y - 2,
                            };
                            let tile = if self.air(tile) { tile } else { site };
                            let point = Self::spawn_point(escort, tile);
                            let _escort = self.spawn_enemy_with(escort, point, tier, None, None);
                        }
                    }
                }
            }
        }
    }

    /// Whether `chunk` is a lair because its middle lies in a cave with room for an elite lair
    /// (WP-CAVES spawn tags; grand caverns, ruins and chasms).
    fn cave_lair_chunk(&self, chunk: ChunkPosition, hash: u64) -> bool {
        if !(hash >> 32).is_multiple_of(CAVE_LAIR_ODDS) {
            return false;
        }
        let origin = chunk.origin();
        let middle = TilePosition {
            x: origin.x + CHUNK_SIZE_TILES / 2,
            y: origin.y + CHUNK_SIZE_TILES / 2,
        };
        self.terrain
            .cave_kind_at(middle)
            .is_some_and(|cave| cave.spawn_tags().elite_lair)
    }

    /// A cavern floor in `chunk` for a lair occupant, out of view (every second tile, from a
    /// hashed start so lairs don't all sit in a corner).
    fn lair_site(
        &self,
        chunk: ChunkPosition,
        kind: EnemyKind,
        hash: u64,
        rigs: &[(PlayerId, TilePosition)],
    ) -> Option<TilePosition> {
        let origin = chunk.origin();
        let start = i32::try_from((hash >> 20) % 64).unwrap_or(0);
        (0..64)
            .map(|step| {
                let index = (start + step) % 64;
                TilePosition {
                    x: origin.x + (index % 8) * 2,
                    y: origin.y + (index / 8) * 2,
                }
            })
            .find(|tile| {
                self.terrain.contains(*tile)
                    && !self.enemy_safe_zone(*tile)
                    && Self::out_of_view(*tile, rigs)
                    && self.site_fits(kind, *tile)
                    && self.open_area(*tile) >= 16
            })
    }

    // --- The AI -------------------------------------------------------------------------

    fn enemy_parts(&self, id: EntityId) -> Option<(Body, EnemyState)> {
        self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.body, state)),
            _ => None,
        })
    }

    fn after(&self, now: SimulationTick, millis: u64) -> SimulationTick {
        SimulationTick::new(
            now.get()
                .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
        )
    }

    /// The nearest rig `body` notices (seen with a clear line, or felt drilling within
    /// earshot), skipping rigs in safe zones and disabled rigs.
    fn perceive(&self, body: &Body, state: &EnemyState) -> Option<PlayerId> {
        let spec = state.kind.stats();
        let sight = spec.sight_tiles * TILE_SIZE;
        let hearing = spec.hearing_tiles * TILE_SIZE;
        let reach = sight.max(hearing);
        if state.kind == EnemyKind::CaveBat && self.flare_near(body.x, body.y).is_some() {
            // A lit flare is irresistible: bats see nothing else.
            return None;
        }
        let mut best: Option<(f32, PlayerId)> = None;
        for (player_id, player) in &self.players {
            if player.hull <= 0.0 {
                continue;
            }
            let dx = player.x - body.x;
            let dy = player.y - body.y;
            if dx.abs() > reach || dy.abs() > reach {
                continue;
            }
            let distance = dx.hypot(dy);
            if best.is_some_and(|(nearest, _)| nearest <= distance) {
                continue;
            }
            if self.enemy_safe_zone(tile_of(player.x, player.y)) {
                continue;
            }
            let heard = distance <= hearing && self.active_drills.contains_key(player_id);
            let seen = distance <= sight
                && (matches!(spec.locomotion, Locomotion::Burrowing)
                    && distance <= TILE_SIZE * 3.0
                    || physics::line_clear(&self.terrain, body.x, body.y, player.x, player.y));
            if heard || seen {
                best = Some((distance, *player_id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// A lit flare within lure range of `(x, y)`, if any (bats circle it instead of hunting).
    fn flare_near(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        // Charges are entities (WP-WEAPONS): read the store directly, no `bombs()` copy.
        self.entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| match entity.state {
                EntityState::Charge(charge)
                    if charge.kind == ExplosiveKind::Flare && charge.stage > 0 =>
                {
                    Some((entity.body.x, entity.body.y))
                }
                _ => None,
            })
            .find(|(fx, fy)| (fx - x).hypot(fy - y) <= FLARE_LURE)
    }

    fn rig_position(&self, player: Option<PlayerId>) -> Option<(f32, f32)> {
        player
            .and_then(|id| self.players.get(&id))
            .map(|player| (player.x, player.y))
    }

    /// One enemy's tick (see the module docs).
    pub(super) fn advance_enemy(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((start_body, start_state)) = self.enemy_parts(id) else {
            return;
        };
        let mut body = start_body;
        let mut state = start_state;
        let dt = self.timing.delta_seconds;

        // Fire eats creatures.
        if state.burning_until > now {
            state.health = BURN_DAMAGE_PER_SECOND.mul_add(-dt, state.health);
            if state.health <= 0.0 {
                self.kill_enemy(id, body, state, output);
                return;
            }
        }
        self.advance_timers(&mut state, now);
        let think_ticks = self.timing.ticks_for_millis(THINK_MILLIS).max(1);
        if (now.get() + id.get()).is_multiple_of(think_ticks) {
            self.think(id, &body, &mut state, now, output);
        }
        let intent = self.behave(id, &mut body, &mut state, now, output);
        self.move_enemy(id, &mut body, &mut state, intent, now);
        self.contact_attack(&mut body, &mut state, now, output);
        if body != start_body || state != start_state {
            self.entities.update(id, |entity| {
                entity.body = body;
                entity.state = EntityState::Enemy(state);
            });
        }
    }

    /// Ends timed modes.
    fn advance_timers(&self, state: &mut EnemyState, now: SimulationTick) {
        if now < state.mode_until {
            return;
        }
        match state.mode {
            EnemyMode::Stunned | EnemyMode::Recover => {
                state.mode = if state.target.is_some() {
                    EnemyMode::Chase
                } else {
                    EnemyMode::Idle
                };
            }
            EnemyMode::Strike => {
                state.mode = EnemyMode::Recover;
                state.mode_until = self.after(now, RECOVER_MILLIS);
            }
            EnemyMode::Flee => {
                state.mode = EnemyMode::Return;
                state.target = None;
            }
            _ => {}
        }
    }

    /// Perception and the transitions it drives.
    fn think(
        &self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if matches!(state.mode, EnemyMode::Stunned | EnemyMode::Flee) {
            return;
        }
        let spec = state.kind.stats();
        let noticed = self.perceive(body, state);
        let (home_x, home_y) = tile_centre(state.home.0, state.home.1);
        let leashed = spec.leash_tiles > 0.0
            && (body.x - home_x).hypot(body.y - home_y) > spec.leash_tiles * TILE_SIZE;
        match (state.mode, noticed) {
            (EnemyMode::Idle | EnemyMode::Dormant | EnemyMode::Return, Some(player)) => {
                if state.mode == EnemyMode::Return && leashed {
                    return;
                }
                state.target = Some(player);
                state.last_seen = now;
                state.mode = EnemyMode::Chase;
                output.push_event(enemy_event(id, state.kind, body, EventShape::Alerted));
            }
            (EnemyMode::Chase | EnemyMode::Windup | EnemyMode::Strike | EnemyMode::Recover, _) => {
                if let Some(player) = noticed {
                    state.target = Some(player);
                    state.last_seen = now;
                }
                let target_safe = self
                    .rig_position(state.target)
                    .is_none_or(|(x, y)| self.enemy_safe_zone(tile_of(x, y)));
                let forgotten = now.get().saturating_sub(state.last_seen.get())
                    > self.timing.ticks_for_millis(LOSE_TARGET_MILLIS);
                if (leashed || target_safe || forgotten) && state.mode == EnemyMode::Chase {
                    state.target = None;
                    state.waypoint = None;
                    state.mode = if spec.leash_tiles > 0.0 {
                        EnemyMode::Return
                    } else {
                        EnemyMode::Idle
                    };
                }
            }
            _ => {}
        }
        // An enemy that strayed into a safe zone turns back.
        if state.mode == EnemyMode::Chase && self.enemy_safe_zone(tile_of(body.x, body.y)) {
            state.mode = EnemyMode::Return;
            state.target = None;
        }
    }

    /// Mode behaviour: where to go, attacks to start or release.
    #[allow(clippy::too_many_lines, reason = "one arm per mode")]
    fn behave(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let (home_x, home_y) = tile_centre(state.home.0, state.home.1);
        let target = self.rig_position(state.target);
        let cooldown = self.attack_cooldown_millis(state.kind);
        match state.mode {
            EnemyMode::Dormant | EnemyMode::Stunned => Intent::default(),
            EnemyMode::Idle => {
                if state.kind == EnemyKind::CaveBat
                    && let Some(flare) = self.flare_near(body.x, body.y)
                {
                    return Intent {
                        goal: Some(self.orbit(flare, state, now, TILE_SIZE * 2.0)),
                        speed: spec.speed,
                    };
                }
                self.wander(body, state, now)
            }
            EnemyMode::Return => {
                let distance = (body.x - home_x).hypot(body.y - home_y);
                state.health = (state.max_health * 0.08)
                    .mul_add(self.timing.delta_seconds, state.health)
                    .min(state.max_health);
                if distance < TILE_SIZE {
                    state.mode = EnemyMode::Idle;
                    state.waypoint = None;
                }
                Intent {
                    goal: Some((home_x, home_y)),
                    speed: spec.speed,
                }
            }
            EnemyMode::Flee => {
                let goal = target.map_or((home_x, home_y), |(tx, ty)| {
                    let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                    (
                        nx.mul_add(TILE_SIZE * 6.0, body.x),
                        ny.mul_add(TILE_SIZE * 6.0, body.y),
                    )
                });
                Intent {
                    goal: Some(goal),
                    speed: spec.speed * 1.2,
                }
            }
            EnemyMode::Chase => {
                let Some((tx, ty)) = target else {
                    state.mode = EnemyMode::Idle;
                    return Intent::default();
                };
                if state.kind == EnemyKind::CaveBat
                    && let Some(flare) = self.flare_near(body.x, body.y)
                {
                    // A flare is irresistible: the swarm forgets the rig.
                    state.target = None;
                    state.mode = EnemyMode::Idle;
                    return Intent {
                        goal: Some(self.orbit(flare, state, now, TILE_SIZE * 2.0)),
                        speed: spec.speed,
                    };
                }
                state.facing = if tx < body.x { -1 } else { 1 };
                let (_, distance) = direction((body.x, body.y), (tx, ty));
                let range = spec.attack_range_tiles * TILE_SIZE;
                let ready = now >= state.attack_ready;
                match spec.attack {
                    AttackStyle::Contact => {
                        let goal = if ready {
                            (tx, ty)
                        } else {
                            // Between bites the swarm circles the rig.
                            self.orbit((tx, ty), state, now, TILE_SIZE * 2.5)
                        };
                        Intent {
                            goal: Some(goal),
                            speed: spec.speed,
                        }
                    }
                    AttackStyle::Lunge => {
                        let clear = matches!(spec.locomotion, Locomotion::Burrowing)
                            || physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
                        if ready && distance <= range && clear {
                            self.start_windup(id, body, state, now, output);
                            return Intent::default();
                        }
                        Intent {
                            goal: Some((tx, ty)),
                            speed: spec.speed,
                        }
                    }
                    AttackStyle::Ranged(..) => {
                        let clear = physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
                        if ready && distance <= range && clear {
                            self.start_windup(id, body, state, now, output);
                            return Intent::default();
                        }
                        let goal = if distance < range * 0.35 && clear {
                            // Too close: back away (but keep the high ground).
                            let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                            (
                                nx.mul_add(TILE_SIZE * 3.0, body.x),
                                ny.mul_add(TILE_SIZE, body.y),
                            )
                        } else if distance < range * 0.7 && clear {
                            (body.x, body.y)
                        } else {
                            (tx, ty)
                        };
                        Intent {
                            goal: Some(goal),
                            speed: spec.speed,
                        }
                    }
                    AttackStyle::Spawner => {
                        if ready && distance <= range {
                            self.hatch_brood(id, body, state.tier);
                            state.attack_ready = self.after(now, cooldown);
                            output.push_event(enemy_event(
                                id,
                                state.kind,
                                body,
                                EventShape::Windup,
                            ));
                        }
                        Intent::default()
                    }
                }
            }
            EnemyMode::Windup => {
                if now < state.mode_until {
                    if let Some((tx, _)) = target {
                        state.facing = if tx < body.x { -1 } else { 1 };
                    }
                    return Intent::default();
                }
                state.attack_ready = self.after(now, cooldown);
                match (spec.attack, target) {
                    (AttackStyle::Ranged(shot, count), Some(aim)) => {
                        self.fire(id, body, state, shot, count, aim, output);
                        state.mode = EnemyMode::Recover;
                        state.mode_until = self.after(now, RECOVER_MILLIS);
                    }
                    (AttackStyle::Lunge, Some((tx, ty))) => {
                        let ((nx, ny), _) = direction((body.x, body.y), (tx, ty));
                        let dash = spec.dash_speed;
                        if matches!(spec.locomotion, Locomotion::Walking) {
                            body.velocity_x = quantize(nx.signum() * dash);
                            body.velocity_y = quantize(if ty < TILE_SIZE.mul_add(-0.5, body.y) {
                                -HOP_SPEED
                            } else {
                                -90.0
                            });
                        } else {
                            body.velocity_x = quantize(nx * dash);
                            body.velocity_y = quantize(ny * dash);
                        }
                        state.mode = EnemyMode::Strike;
                        state.mode_until = self.after(
                            now,
                            if matches!(spec.locomotion, Locomotion::Burrowing) {
                                STRIKE_MILLIS + 250
                            } else {
                                STRIKE_MILLIS
                            },
                        );
                    }
                    _ => state.mode = EnemyMode::Idle,
                }
                Intent::default()
            }
            EnemyMode::Strike => Intent {
                goal: None,
                speed: 0.0,
            },
            EnemyMode::Recover => {
                let goal = target.map(|(tx, ty)| {
                    let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                    (
                        nx.mul_add(TILE_SIZE * 2.0, body.x),
                        ny.mul_add(TILE_SIZE, body.y),
                    )
                });
                Intent {
                    goal,
                    speed: spec.speed * 0.6,
                }
            }
        }
    }

    /// Milliseconds between attacks of `kind` at the world's difficulty.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "bounded milliseconds"
    )]
    fn attack_cooldown_millis(&self, kind: EnemyKind) -> u64 {
        (kind.stats().cooldown_millis as f32 * self.enemies.difficulty.cooldown()) as u64
    }

    fn start_windup(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = state.kind.stats();
        state.mode = EnemyMode::Windup;
        state.mode_until = self.after(now, spec.windup_millis);
        if !matches!(spec.locomotion, Locomotion::Walking) {
            body.velocity_x = quantize(body.velocity_x * 0.3);
            body.velocity_y = quantize(body.velocity_y * 0.3);
        }
        output.push_event(enemy_event(id, state.kind, body, EventShape::Windup));
    }

    /// A point circling `center` at `radius` (deterministic from the tick and the enemy's
    /// seed: eight stations visited in turn, two a second).
    fn orbit(
        &self,
        center: (f32, f32),
        state: &EnemyState,
        now: SimulationTick,
        radius: f32,
    ) -> (f32, f32) {
        let step_ticks = self.timing.ticks_for_millis(500).max(1);
        let station = (now.get() / step_ticks + u64::from(state.seed)) % 16;
        let (dx, dy) = RING[usize::try_from(station).unwrap_or(0)];
        #[allow(clippy::cast_precision_loss, reason = "ring offsets are tiny")]
        let (dx, dy) = (dx as f32 / 16.0, dy as f32 / 16.0);
        (
            dx.mul_add(radius, center.0),
            (dy * 0.6).mul_add(radius, center.1),
        )
    }

    /// Idle drifting near home (flyers and burrowers) or pacing (walkers).
    fn wander(&self, body: &Body, state: &mut EnemyState, now: SimulationTick) -> Intent {
        let spec = state.kind.stats();
        let (home_x, home_y) = tile_centre(state.home.0, state.home.1);
        let period = self.timing.ticks_for_millis(2_500).max(1);
        let phase = now.get() / period;
        let hash = hash2(
            u64::from(state.seed),
            SALT_WANDER,
            i32::try_from(phase & 0x7FFF_FFFF).unwrap_or(0),
            0,
        );
        #[allow(clippy::cast_precision_loss, reason = "small hashed offsets")]
        let (ox, oy) = (
            ((hash % 9) as f32 - 4.0) * TILE_SIZE,
            (((hash >> 8) % 5) as f32 - 2.0) * TILE_SIZE,
        );
        match spec.locomotion {
            Locomotion::Stationary => Intent::default(),
            Locomotion::Walking => {
                if hash.is_multiple_of(3) {
                    return Intent::default();
                }
                let goal_x = home_x + ox;
                state.facing = if goal_x < body.x { -1 } else { 1 };
                Intent {
                    goal: Some((goal_x, body.y)),
                    speed: spec.speed * 0.4,
                }
            }
            Locomotion::Flying | Locomotion::Burrowing => Intent {
                goal: Some((home_x + ox, home_y + oy)),
                speed: spec.speed * 0.45,
            },
        }
    }

    /// Movement by locomotion toward the intent's goal (through a path waypoint when the way
    /// is blocked).
    fn move_enemy(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        intent: Intent,
        now: SimulationTick,
    ) {
        let spec = state.kind.stats();
        let dt = self.timing.delta_seconds;
        let goal = intent
            .goal
            .map(|goal| self.route(id, body, state, goal, now));
        match spec.locomotion {
            Locomotion::Stationary => {}
            Locomotion::Flying => {
                let stunned = state.mode == EnemyMode::Stunned;
                if !stunned && state.mode != EnemyMode::Strike {
                    let (gx, gy) = goal.unwrap_or((body.x, body.y));
                    physics::steer_toward(
                        body,
                        gx,
                        gy,
                        intent.speed,
                        intent.speed.mul_add(4.0, 60.0),
                        dt,
                    );
                }
                let _report =
                    physics::step_body(body, &body_params(state.kind, stunned), &self.terrain, dt);
            }
            Locomotion::Walking => self.walk(id, body, state, goal, intent.speed, now),
            Locomotion::Burrowing => self.burrow(body, state, goal, intent.speed),
        }
        if let Some((gx, _)) = goal
            && (gx - body.x).abs() > 2.0
            && state.mode != EnemyMode::Windup
        {
            state.facing = if gx < body.x { -1 } else { 1 };
        }
    }

    /// The point to steer at for `goal`: the goal itself in clear line, else the next tile of a
    /// budgeted path (searched at most once every [`PATH_SLOT_TICKS`] per enemy).
    fn route(
        &self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        goal: (f32, f32),
        now: SimulationTick,
    ) -> (f32, f32) {
        let spec = state.kind.stats();
        if matches!(
            spec.locomotion,
            Locomotion::Burrowing | Locomotion::Stationary
        ) {
            return goal;
        }
        // Follow a waypoint until it is reached or goes stale (no line test per tick while
        // pathing).
        let here = tile_of(body.x, body.y);
        if let Some(waypoint) = state.waypoint {
            if waypoint == (here.x, here.y) || now >= state.next_path {
                state.waypoint = None;
            } else {
                return tile_centre(waypoint.0, waypoint.1);
            }
        }
        if physics::line_clear(&self.terrain, body.x, body.y, goal.0, goal.1) {
            return goal;
        }
        if (now.get() + id.get()).is_multiple_of(PATH_SLOT_TICKS) && now >= state.next_path {
            state.next_path = self.after(now, PATH_RETRY_MILLIS);
            let to = tile_of(goal.0, goal.1);
            if let Some(path) = physics::find_tile_path(&self.terrain, here, to, PATH_NODES) {
                // Cut corners: the farthest of the next few path tiles in clear line.
                let next = path
                    .iter()
                    .take(8)
                    .rev()
                    .find(|tile| {
                        let (x, y) = tile_centre(tile.x, tile.y);
                        physics::line_clear(&self.terrain, body.x, body.y, x, y)
                    })
                    .or_else(|| path.first());
                if let Some(next) = next {
                    state.waypoint = Some((next.x, next.y));
                    return tile_centre(next.x, next.y);
                }
            }
        }
        goal
    }

    /// Walking: approach the goal's column, hop one-tile steps, and (excavators) bite through
    /// soft rock in the way.
    #[allow(clippy::too_many_arguments, reason = "the walker's whole context")]
    fn walk(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &EnemyState,
        goal: Option<(f32, f32)>,
        speed: f32,
        now: SimulationTick,
    ) {
        let spec = state.kind.stats();
        let dt = self.timing.delta_seconds;
        let params = body_params(state.kind, false);
        let grounded = physics::supported(&self.terrain, body, &params);
        let controlled = !matches!(
            state.mode,
            EnemyMode::Strike | EnemyMode::Stunned | EnemyMode::Dormant
        );
        if controlled {
            let desired = goal.map_or(0.0, |(gx, _)| {
                let dx = gx - body.x;
                if dx.abs() < 3.0 {
                    0.0
                } else {
                    dx.signum() * speed
                }
            });
            let accel = speed.mul_add(6.0, 40.0) * dt;
            let change = (desired - body.velocity_x).clamp(-accel, accel);
            body.velocity_x = quantize(body.velocity_x + change);
            if grounded && desired.abs() > 0.0 {
                let ahead_x = desired.signum().mul_add(spec.radius + 3.0, body.x);
                let blocked = physics::solid_at(&self.terrain, ahead_x, body.y)
                    || physics::solid_at(&self.terrain, ahead_x, body.y + spec.radius - 2.0);
                let goal_above = goal.is_some_and(|(_, gy)| gy < TILE_SIZE.mul_add(-0.75, body.y));
                if blocked && state.kind == EnemyKind::RogueExcavator {
                    self.dig_ahead(id, body, desired.signum(), now);
                } else if blocked || goal_above {
                    let room =
                        !physics::collides(&self.terrain, body.x, body.y - TILE_SIZE, spec.radius);
                    if room {
                        body.velocity_y = -HOP_SPEED;
                    }
                }
            }
        } else if grounded {
            // Dashes and stuns skid to a halt on the floor.
            body.velocity_x = quantize(body.velocity_x * 6.0f32.mul_add(-dt, 1.0).max(0.0));
        }
        let _report = physics::step_body(body, &params, &self.terrain, dt);
    }

    /// An excavator chews the tiles in front of it (soft rock only; never ore, fixtures or
    /// bedrock).
    fn dig_ahead(&mut self, id: EntityId, body: &Body, side: f32, now: SimulationTick) {
        let period = self.timing.ticks_for_millis(DIG_MILLIS).max(1);
        if !(now.get() + id.get()).is_multiple_of(period) {
            return;
        }
        let radius = EnemyKind::RogueExcavator.stats().radius;
        let ahead = tile_of(side.mul_add(radius + 6.0, body.x), body.y);
        for dy in [0, -1] {
            let position = TilePosition {
                x: ahead.x,
                y: ahead.y + dy,
            };
            if self
                .terrain
                .tile(position)
                .is_some_and(|tile| diggable(tile.kind))
                && !self.enemy_safe_zone(position)
            {
                let _dug = self.terrain.set_kind(position, TileKind::Air);
            }
        }
    }

    /// Burrowing: swim through soft rock toward the goal; in open air (a lunge, a cave) fall
    /// back down.
    fn burrow(&self, body: &mut Body, state: &EnemyState, goal: Option<(f32, f32)>, speed: f32) {
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        let inside = self
            .terrain
            .tile(tile_of(body.x, body.y))
            .is_some_and(|tile| burrowable(tile.kind));
        let striking = state.mode == EnemyMode::Strike;
        if inside && !striking {
            if let Some((gx, gy)) = goal {
                physics::steer_toward(body, gx, gy, speed, speed * 3.0, dt);
            } else {
                physics::steer_toward(body, body.x, body.y, speed, speed * 3.0, dt);
            }
        } else if !inside {
            body.velocity_y = quantize(ENEMY_GRAVITY.mul_add(dt, body.velocity_y).min(480.0));
        }
        let passable = |x: f32, y: f32| {
            let tile = tile_of(x, y);
            if tile.y < 0 {
                return true;
            }
            self.terrain.contains(tile)
                && self
                    .terrain
                    .tile(tile)
                    .is_some_and(|t| t.kind == TileKind::Air || burrowable(t.kind))
        };
        let next_x = body.velocity_x.mul_add(dt, body.x);
        if passable(next_x, body.y) {
            body.x = next_x;
        } else {
            body.velocity_x = 0.0;
        }
        let next_y = body.velocity_y.mul_add(dt, body.y);
        if passable(body.x, next_y) {
            body.y = next_y;
        } else {
            body.velocity_y = 0.0;
        }
        body.x = quantize(body.x);
        body.y = quantize(body.y);
        body.velocity_x = quantize(body.velocity_x);
        body.velocity_y = quantize(body.velocity_y);
    }

    /// Bites, rams and charges: touching a rig hurts while the attack is live.
    fn contact_attack(
        &mut self,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = state.kind.stats();
        let live = match spec.attack {
            AttackStyle::Contact => now >= state.attack_ready && state.mode == EnemyMode::Chase,
            AttackStyle::Lunge => state.mode == EnemyMode::Strike,
            AttackStyle::Ranged(..) | AttackStyle::Spawner => false,
        };
        if !live {
            return;
        }
        let reach = spec.radius + AUTHORITATIVE_PLAYER_RADIUS + 2.0;
        // A rig still inside its i-frames doesn't shield a crewmate touching the same enemy.
        let victim = self.players.iter().find_map(|(player_id, player)| {
            ((player.x - body.x).hypot(player.y - body.y) <= reach
                && !self.enemies.guarded(*player_id, now))
            .then_some(*player_id)
        });
        let Some(player_id) = victim else {
            return;
        };
        let damage = spec.contact_damage * tier_scale(state.tier);
        if self.strike_rig(
            player_id,
            damage,
            (body.x, body.y),
            spec.knockback,
            state.kind,
            now,
            output,
        ) {
            state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
            if state.mode == EnemyMode::Strike {
                state.mode = EnemyMode::Recover;
                state.mode_until = self.after(now, RECOVER_MILLIS);
                body.velocity_x = quantize(-body.velocity_x * 0.3);
                body.velocity_y = quantize(-body.velocity_y * 0.3);
            }
        }
    }

    /// Hurts a rig from `from` (i-frames, safe zones and god mode respected) and knocks it
    /// back. Returns whether the hit landed.
    #[allow(clippy::too_many_arguments, reason = "one hit's whole description")]
    fn strike_rig(
        &mut self,
        player_id: PlayerId,
        damage: f32,
        from: (f32, f32),
        knockback: f32,
        kind: EnemyKind,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> bool {
        if self.damage_immune() || self.enemies.guarded(player_id, now) {
            return false;
        }
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        if player.hull <= 0.0 || self.enemy_safe_zone(tile_of(player.x, player.y)) {
            return false;
        }
        let guard = self.after(now, RIG_GUARD_MILLIS);
        self.enemies.rig_guard.insert(player_id, guard);
        output.push_event(GameplayEventKind::RigStruck { player_id, kind });
        if let Some(player) = self.players.get_mut(&player_id) {
            let ((nx, ny), _) = direction(from, (player.x, player.y));
            let push = knockback.min(RIG_KNOCKBACK_MAX);
            player.velocity_x = quantize(
                nx.mul_add(push, player.velocity_x)
                    .clamp(-RIG_KNOCKBACK_MAX, RIG_KNOCKBACK_MAX),
            );
            player.velocity_y = quantize(
                ny.mul_add(push * 0.6, push.mul_add(-0.25, player.velocity_y))
                    .clamp(-RIG_KNOCKBACK_MAX, RIG_KNOCKBACK_MAX),
            );
        }
        let damage = damage * self.enemies.difficulty.damage();
        if self.damage_player(player_id, damage) {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.hull = quantize(player.hull);
            }
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
        }
        true
    }

    /// Fires `count` shots of `shot` at `aim` (a lob for acid), spread in a volley.
    #[allow(clippy::too_many_arguments, reason = "one volley's whole description")]
    fn fire(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        shot: ShotKind,
        count: u8,
        aim: (f32, f32),
        output: &mut TickOutput,
    ) {
        if self.entities.count_of(EntityKind::EnemyShot) >= MAX_SHOTS {
            return;
        }
        let ((nx, ny), distance) = direction((body.x, body.y), aim);
        let speed = shot.speed();
        let damage = shot.damage() * tier_scale(state.tier);
        let lifetime = self.timing.ticks_for_millis(shot.life_millis()).max(1);
        let expires = SimulationTick::new(self.simulation_tick.get().saturating_add(lifetime));
        let turns: &[(f32, f32)] = if count >= 3 {
            &VOLLEY_TURNS
        } else {
            &VOLLEY_TURNS[1..2]
        };
        let muzzle = state.kind.stats().radius + shot.radius() + 1.0;
        for (cos, sin) in turns {
            let dx = nx.mul_add(*cos, -ny * sin);
            let dy = nx.mul_add(*sin, ny * cos);
            let mut vx = dx * speed;
            let mut vy = dy * speed;
            if shot.gravity() > 0.0 {
                // Lob: aim up by the fall over the flight time.
                let flight = (distance / speed).clamp(0.1, 1.6);
                vy = (shot.gravity() * flight).mul_add(-0.5, vy);
                vx = dx * speed;
            }
            let x = quantize(dx.mul_add(muzzle, body.x));
            let y = quantize(dy.mul_add(muzzle, body.y));
            let _shot = self.spawn_entity(
                Body::at(x, y).moving(quantize(vx), quantize(vy)),
                EntityState::EnemyShot(EnemyShotState {
                    kind: shot,
                    damage,
                    shooter: state.kind,
                }),
                Some(expires),
            );
        }
        #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
        output.push_event(GameplayEventKind::EnemyFired {
            entity: id.get(),
            shot,
            x: body.x.round() as i32,
            y: body.y.round() as i32,
        });
    }

    /// A nest hatches a bat (while it has fewer than [`NEST_BROOD`] alive).
    fn hatch_brood(&mut self, id: EntityId, body: &Body, tier: u8) {
        let brood = self
            .enemies()
            .filter(|(_, state)| state.parent == Some(id.get()))
            .count();
        if brood >= NEST_BROOD || self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
            return;
        }
        let radius = EnemyKind::BroodNest.stats().radius;
        let spawned = self.spawn_enemy_with(
            EnemyKind::CaveBat,
            (body.x, body.y - radius - 6.0),
            tier,
            None,
            Some(id),
        );
        self.entities.update(spawned, |entity| {
            entity.body.velocity_y = -120.0;
        });
    }

    /// One enemy shot's tick: fly (acid falls), splash on rock, hit rigs.
    pub(super) fn advance_enemy_shot(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((mut body, shot)) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::EnemyShot(shot) => Some((entity.body, shot)),
            _ => None,
        }) else {
            return;
        };
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        body.velocity_y = quantize(shot.kind.gravity().mul_add(dt, body.velocity_y));
        // Substeps of at most four pixels so fast bolts never skip a thin wall or a rig.
        let travel = (body.velocity_x.abs().max(body.velocity_y.abs()) * dt).max(1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bounded handful of substeps"
        )]
        let steps = ((travel / 4.0).ceil() as u32).clamp(1, 32);
        #[allow(clippy::cast_precision_loss, reason = "a handful of substeps")]
        let step = dt / steps as f32;
        let reach = shot.kind.radius() + AUTHORITATIVE_PLAYER_RADIUS;
        for _ in 0..steps {
            body.x = body.velocity_x.mul_add(step, body.x);
            body.y = body.velocity_y.mul_add(step, body.y);
            if physics::solid_at(&self.terrain, body.x, body.y) {
                self.entities.despawn(id);
                return;
            }
            let victim = self.players.iter().find_map(|(player_id, player)| {
                ((player.x - body.x).hypot(player.y - body.y) <= reach).then_some(*player_id)
            });
            if let Some(player_id) = victim {
                let _landed = self.strike_rig(
                    player_id,
                    shot.damage,
                    (body.x, body.y),
                    120.0,
                    shot.shooter,
                    now,
                    output,
                );
                if !self.damage_immune() {
                    self.entities.despawn(id);
                    return;
                }
            }
        }
        body.x = quantize(body.x);
        body.y = quantize(body.y);
        self.entities.update(id, |entity| entity.body = body);
    }

    // --- Damage -------------------------------------------------------------------------

    /// Applies `damage` to entity `id` (the damage API; see the module docs). Only enemies
    /// take damage today; anything else is [`DamageOutcome::Ignored`].
    pub fn damage_entity(
        &mut self,
        id: EntityId,
        damage: Damage,
        output: &mut TickOutput,
    ) -> DamageOutcome {
        let Some((mut body, mut state)) = self.enemy_parts(id) else {
            return DamageOutcome::Ignored;
        };
        let now = self.simulation_tick.next();
        let spec = state.kind.stats();
        let mut amount = (damage.amount * state.kind.resistance(damage.kind)).max(0.0);
        let mut stunned = false;
        if damage.kind == DamageKind::Emp && state.kind.is_machine() {
            stunned = true;
            state.shield = 0.0;
            state.mode = EnemyMode::Stunned;
            state.mode_until = self.after(
                now,
                if spec.elite {
                    EMP_STUN_MILLIS / 2
                } else {
                    EMP_STUN_MILLIS
                },
            );
            output.push_event(enemy_event(id, state.kind, &body, EventShape::Stunned));
        }
        if state.shield > 0.0 && damage.kind != DamageKind::Emp {
            let absorbed = state.shield.min(amount);
            state.shield = quantize(state.shield - absorbed);
            amount -= absorbed;
        }
        if amount <= 0.0 && !stunned {
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
            return DamageOutcome::Ignored;
        }
        state.health = quantize(state.health - amount);
        state.hurt_until = self.after(now, HURT_FLASH_MILLIS);
        if let Some(player) = damage.source.player() {
            state.last_attacker = Some(player);
            if matches!(
                state.mode,
                EnemyMode::Idle | EnemyMode::Dormant | EnemyMode::Return
            ) {
                state.mode = EnemyMode::Chase;
                state.target = Some(player);
                state.last_seen = now;
            }
        }
        if damage.kind == DamageKind::Fire && !state.kind.is_machine() && amount > 0.0 {
            state.burning_until = self.after(now, BURN_MILLIS);
        }
        if !matches!(spec.locomotion, Locomotion::Stationary) {
            let scale = if spec.elite { 0.35 } else { 1.0 };
            body.velocity_x = quantize(damage.impulse.0.mul_add(scale, body.velocity_x));
            body.velocity_y = quantize(damage.impulse.1.mul_add(scale, body.velocity_y));
        }
        if state.health <= 0.0 {
            self.kill_enemy(id, body, state, output);
            return DamageOutcome::Killed { dealt: amount };
        }
        let skittish = matches!(
            state.kind,
            EnemyKind::CaveBat | EnemyKind::AcidSpitter | EnemyKind::MagmaWisp
        );
        if skittish && state.health_fraction() < 0.25 && state.mode != EnemyMode::Stunned {
            state.mode = EnemyMode::Flee;
            state.mode_until = self.after(now, FLEE_MILLIS);
        }
        output.push_event(enemy_event(id, state.kind, &body, EventShape::Hurt));
        self.entities.update(id, |entity| {
            entity.body = body;
            entity.state = EntityState::Enemy(state);
        });
        DamageOutcome::Hurt { dealt: amount }
    }

    /// Removes a dead enemy, drops its loot, clears its lair and announces the kill.
    fn kill_enemy(&mut self, id: EntityId, body: Body, state: EnemyState, output: &mut TickOutput) {
        self.entities.despawn(id);
        let roll = hash2(
            self.terrain.seed() ^ u64::from(state.seed),
            SALT_LOOT,
            i32::try_from(id.get() & 0x7FFF_FFFF).unwrap_or(0),
            i32::from(state.tier),
        );
        for (index, stack) in (0_u8..).zip(loot(state.kind, state.tier, roll)) {
            let spread = (f32::from(index % 5) - 2.0) * 45.0;
            let _drop = self.spawn_item_drop(
                body.x,
                body.y,
                (spread, -150.0 - f32::from(index % 3) * 30.0),
                stack,
                Some(LOOT_LIFETIME_MILLIS),
            );
        }
        if let Some(lair) = state.lair {
            self.enemies.cleared_lairs.insert(lair);
        }
        self.enemies.kills = self.enemies.kills.saturating_add(1);
        #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
        output.push_event(GameplayEventKind::EnemyKilled {
            entity: id.get(),
            kind: state.kind,
            x: body.x.round() as i32,
            y: body.y.round() as i32,
            by: state.last_attacker,
        });
    }

    /// This tick's blasts hurt, push and stun enemies (called from the bomb pass on the
    /// pre-blast terrain, like rig pushes). Explosives hurt everything in reach with a clear
    /// line (burrowers feel them through rock), incendiaries set creatures alight, EMPs stun
    /// and hurt machines through rock, flares do nothing here (they lure bats).
    pub(super) fn enemies_feel_blasts(&mut self, blasts: &[Blast], output: &mut TickOutput) {
        if self.entities.count_of(EntityKind::Enemy) == 0 {
            return;
        }
        for blast in blasts {
            let source = blast
                .owner
                .map_or(DamageSource::World, DamageSource::Player);
            if blast.kind == ExplosiveKind::Emp {
                for id in self.enemies_near(blast.x, blast.y, EMP_RADIUS) {
                    let _outcome = self.damage_entity(
                        id,
                        Damage::new(EMP_DAMAGE, DamageKind::Emp, source),
                        output,
                    );
                }
                continue;
            }
            let Some(base) = blast_damage(blast.kind) else {
                continue;
            };
            let reach = blast.kind.push_reach().max(TILE_SIZE * 2.0);
            for id in self.enemies_near(blast.x, blast.y, reach + TILE_SIZE) {
                let Some((body, state)) = self.enemy_parts(id) else {
                    continue;
                };
                let spec = state.kind.stats();
                let (_, distance) = direction((blast.x, blast.y), (body.x, body.y));
                let edge = reach + spec.radius;
                if distance >= edge {
                    continue;
                }
                let through_rock = matches!(spec.locomotion, Locomotion::Burrowing);
                if !through_rock
                    && !physics::line_clear(&self.terrain, blast.x, blast.y, body.x, body.y)
                {
                    continue;
                }
                let falloff = (1.0 - distance / edge).clamp(0.25, 1.0);
                let (ix, iy) = blast_impulse(
                    blast.kind,
                    (blast.x, blast.y),
                    (body.x, body.y),
                    id.get() % 2 == 1,
                )
                .unwrap_or((0.0, 0.0));
                let kind = if blast.kind == ExplosiveKind::Incendiary {
                    DamageKind::Fire
                } else {
                    DamageKind::Explosive
                };
                let _outcome = self.damage_entity(
                    id,
                    Damage::new(base * falloff, kind, source).with_impulse(ix * 0.8, iy * 0.8),
                    output,
                );
            }
        }
    }
}

/// Peak damage of an explosive's blast on an enemy (`None`: no blast damage).
#[must_use]
pub const fn blast_damage(kind: ExplosiveKind) -> Option<f32> {
    Some(match kind {
        ExplosiveKind::Bomb => 60.0,
        ExplosiveKind::Sticky => 55.0,
        ExplosiveKind::Cluster => 30.0,
        ExplosiveKind::Bomblet => 24.0,
        ExplosiveKind::C4 => 95.0,
        ExplosiveKind::Shaped => 45.0,
        ExplosiveKind::Mining => 50.0,
        ExplosiveKind::Incendiary => 28.0,
        ExplosiveKind::Emp | ExplosiveKind::Flare => return None,
    })
}

#[derive(Clone, Copy)]
enum EventShape {
    Alerted,
    Windup,
    Hurt,
    Stunned,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::missing_const_for_fn,
    reason = "bounded world pixels; float rounding is not const"
)]
fn enemy_event(id: EntityId, kind: EnemyKind, body: &Body, shape: EventShape) -> GameplayEventKind {
    let (entity, x, y) = (id.get(), body.x.round() as i32, body.y.round() as i32);
    match shape {
        EventShape::Alerted => GameplayEventKind::EnemyAlerted { entity, kind, x, y },
        EventShape::Windup => GameplayEventKind::EnemyWindup { entity, kind, x, y },
        EventShape::Hurt => GameplayEventKind::EnemyHurt { entity, kind, x, y },
        EventShape::Stunned => GameplayEventKind::EnemyStunned { entity, kind, x, y },
    }
}

/// Whether a cave with spawn tags `tags` is home ground for `kind` (WP-CAVES): swarmers roost,
/// burrowers tunnel soft walls, spitters and mimics nest, heat-lovers live by lava and machines
/// guard built places.
const fn cave_habitat_fits(kind: EnemyKind, tags: crate::cave::CaveSpawnTags) -> bool {
    match kind {
        EnemyKind::CaveBat => tags.swarmers,
        EnemyKind::Burrower => tags.burrowers,
        EnemyKind::AcidSpitter | EnemyKind::RockMimic | EnemyKind::BroodNest => tags.nests,
        EnemyKind::Shellback => tags.aquatic || tags.burrowers,
        EnemyKind::MagmaWisp => tags.heat,
        EnemyKind::MiningDrone
        | EnemyKind::SentryTurret
        | EnemyKind::RogueExcavator
        | EnemyKind::AncientGuardian => tags.machines,
        EnemyKind::BroodLord => tags.elite_lair,
    }
}
