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
//!   (sight with a clear line, never through rock; flares luring bats) every
//!   [`THINK_MILLIS`], a small state machine ([`EnemyMode`]: idle, chase, wind-up, strike,
//!   recover, return home on the leash, flee, EMP stun, dormant mimics), movement by
//!   locomotion (flying, walking with step hops, burrowing: crawling along rock and now and then
//!   biting a tile of soft rock out of the way, stationary) with budgeted path searches ([`physics::find_tile_path`], at most one per
//!   enemy every [`PATH_SLOT_TICKS`] ticks, staggered by id) and contact attacks.
//!
//! # Bodies
//!
//! Enemies collide with exactly the tiles a rig does (swept [`physics::step_body`] motion):
//! nothing passes through rock, built pieces or closed doors. Burrowers bite the soft rock in
//! their way out of the terrain (one tile, rarely) instead of ghosting. Spawns land in open space, and an enemy
//! left in rock by a terrain change is resolved out the shortest way at the start of its tick.
//! Enemies touch rigs and pilots on foot but are pushed out of them to exactly touching after
//! moving; contact attacks reach `CONTACT_MARGIN` beyond that, a landed hit springs the
//! attacker back and melee enemies hang back between attacks.
//! * Shots ([`WorldState::advance_enemy_shot`]) fly, fall (acid) and hit rock or rigs.
//!
//! # Fairness
//!
//! Rigs inside a safe zone ([`WorldState::enemy_safe_zone`]: the surface strip, the town and
//! its shafts, around infrastructure and bases) are never targeted or hurt by contact, nothing
//! spawns there or in view of a rig, and a rig that was hit is invulnerable for
//! [`RIG_GUARD_MILLIS`] (i-frames) while it is knocked back. Test-mode invulnerability and
//! no-clip (god mode) make rigs immune: enemies touch them harmlessly.
//!
//! # Damage API (for WP-WEAPONS)
//!
//! [`WorldState::damage_enemy`] applies a [`Damage`] (amount, [`DamageKind`], source,
//! impulse) to an entity and returns a [`DamageOutcome`]. It applies the target's resistance,
//! shields, EMP stuns, burning, aggro toward the attacking player, flinch and fleeing, and on a
//! kill drops the loot (`spawn_loot`, WP-DROPS' rules), clears the lair and emits
//! [`GameplayEventKind::EnemyKilled`]. Call it from inside the tick (projectiles, melee, beams);
//! [`WorldState::enemies_near`] finds targets.

use crate::{
    enemy::{
        AttackStyle, Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty,
        EnemyDirector, EnemyKind, EnemyMode, EnemyShotState, EnemyState, Locomotion, ShotKind,
        hazard_density, lair_occupant, loot, tier_scale,
    },
    entity::{Body, Entity, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::{ExplosiveKind, SCARE_MILLIS, SCARE_RADIUS, blast_impulse},
    ids::{PlayerId, SimulationTick},
    physics::{self, BodyParams},
    pilot::PILOT_HALF_WIDTH,
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    surface::town_span,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, TILE_SIZE, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{WorldState, bombs::Blast, movement::AUTHORITATIVE_PLAYER_RADIUS};

mod ancient_city;
mod crypt;
mod distortion;
mod drowned;
mod far_strange;
mod frontier;
mod garrison;
mod heart;
pub(super) mod hollow;
mod living;
mod sky;
mod tomb;
mod wilds;

pub use crypt::SLAM_REACH;
pub(super) use distortion::distortion_event;
#[cfg(test)]
pub use far_strange::prism_shot;
pub use sky::{JOLT_MILLIS, NEST_GUARD_TILES, SKY_CITY_SAFE_TILES, TURRET_MOUNTS};
pub use tomb::{GuardianMove, MAX_BATS, MAX_RAISED, guardian_move, is_tomb_guardian};

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
pub(super) const FLEE_MILLIS: u64 = 2_000;

/// How far a lit flare lures bats, pixels.
const FLARE_LURE: f32 = TILE_SIZE * 10.0;
/// Most bats a nest keeps alive.
const NEST_BROOD: usize = 4;
/// Gravity on walkers, falling flyers and burrowers in open air.
const ENEMY_GRAVITY: f32 = 560.0;
/// Speed a walker hops a one-tile step with.
const HOP_SPEED: f32 = 250.0;
/// Fastest push a hit leaves a rig with (under the safe landing speed: no crash damage).
const RIG_KNOCKBACK_MAX: f32 = 260.0;
/// How often a digging enemy (a burrower, an excavator) may bite one tile out of its way while
/// it hunts. Was every 350 ms for excavators and every tick for burrowers, which riddled the
/// ground around any rig they chased; chewing through a block is now a rare thing.
pub const BITE_MILLIS: u64 = 4_000;
/// Sound reaches a footstep-feeling enemy along an open path at most this many times its
/// hearing range long (around corners, never through rock).
const SOUND_PATH_STRETCH: f32 = 1.5;
/// A burrower clings to rock (no gravity) while solid terrain is within this many pixels of its
/// body: anywhere inside a tunnel it dug (whole tiles around a smaller body), not in a cavern.
const CLING_PIXELS: f32 = 8.0;
/// How far ahead of its body a burrower bites rock out of its way, pixels.
const DIG_LEAD_PIXELS: f32 = 6.0;
/// Farthest an enemy left inside rock is moved out of it, pixels.
const UNSTICK_PIXELS: f32 = TILE_SIZE;
/// Half extents of a rig's contact box (its drawn body, a little wider than its terrain square),
/// pixels: enemies are pushed out of it to exactly touching.
pub const RIG_CONTACT_HALF: (f32, f32) = (15.0, 13.0);
/// Contact reach beyond touching (bodies are pushed apart to exactly touching), pixels.
const CONTACT_MARGIN: f32 = 2.0;
/// Speed an enemy springs back off a body it just hit, pixels per second.
const RECOIL_SPEED: f32 = 150.0;
/// A melee enemy waiting for its next attack hangs back this far beyond touching, pixels.
const STANDOFF_PIXELS: f32 = TILE_SIZE * 1.5;

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

/// Fuel a mud leech drinks from a rig per bite (Wave 8, the Crust).
pub const MUD_LEECH_FUEL_SIP: f32 = 9.0;

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

/// Rock an excavator bites away and a burrower tunnels through (never ore: a creature does not
/// eat a rig's prize).
const fn diggable(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

/// Ground a burrowing `enemy` tunnels through: [`diggable`] rock for burrowers, the soft
/// surface crusts for a dune stalker (WP-NIGHT, [`crate::night::stalker_ground`]).
const fn diggable_by(enemy: EnemyKind, kind: TileKind) -> bool {
    match enemy {
        EnemyKind::DuneStalker => crate::night::stalker_ground(kind),
        // Wave 8: bark beetles eat living root, maw worms bore through flesh.
        EnemyKind::BarkBeetle => {
            diggable(kind)
                || matches!(
                    kind,
                    TileKind::Cave(
                        crate::cave::CaveTile::Root | crate::cave::CaveTile::CharredRoot
                    )
                )
        }
        EnemyKind::MawWorm => {
            diggable(kind) || matches!(kind, TileKind::Cave(crate::cave::CaveTile::Flesh))
        }
        // Wave 12 planets-a: a crevassewyrm tunnels through ice and permafrost, nothing else.
        EnemyKind::Crevassewyrm => matches!(
            kind,
            TileKind::Wild(
                crate::planet::wilds::WildTile::GlacierIce
                    | crate::planet::wilds::WildTile::BlueIce
                    | crate::planet::wilds::WildTile::SeaIce
                    | crate::planet::wilds::WildTile::Rime
                    | crate::planet::wilds::WildTile::Permafrost
            )
        ),
        // Wave 12: the dune wyrm swims the dune world's sand.
        EnemyKind::DuneWyrm => frontier::wyrm_ground(kind),
        _ => diggable(kind),
    }
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
        gravity_x: 0.0,
        time_scale: 1.0,
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

    /// Marks lair chunk `chunk` as cleared, so it never gets an occupant (scenario setup: the
    /// balance harness's sealed duel arenas).
    pub(crate) fn mark_lair_cleared(&mut self, chunk: ChunkPosition) {
        self.enemies.cleared_lairs.insert(chunk);
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
        // Tomb run spaces (WP-TOMBS) are never safe.
        if self.in_run_space(tile) {
            return false;
        }
        // The Moon (WP-MOON): hostile everywhere but the colony.
        if let Some(safe) = self.lunar_safe_zone(tile) {
            return safe;
        }
        // A wild world (Wave 12 planets-a): its surface is the creatures' too; only the
        // settlement site is safe.
        if let Some(safe) = self.wilds_safe_zone(tile) {
            return safe;
        }
        // A frontier world (Wave 12): hostile everywhere but its settlement and your bases.
        if let Some(safe) = self.frontier_safe_zone(tile) {
            return safe;
        }
        let surface = self.terrain.surface_row(tile.x);
        if tile.y < surface + SAFE_SURFACE_ROWS {
            // The hostile sky of a V9 world (archipelago and up, T21) is safe only in sky
            // cities, bases and around infrastructure.
            if self.hostile_sky(tile) {
                return self.sky_safe_zone(tile);
            }
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
        // Claimed bases (WP-BASE), home plots (WP-HOME) and villages (WP-VILLAGE).
        self.bases.is_safe_zone(tile)
            || self.homes.is_safe_zone(tile)
            || crate::worldgen::streamed::in_village_area(
                self.terrain.settings(),
                tile,
                VILLAGE_SAFE_TILES,
            )
    }

    /// Spawns an enemy of `kind` centred at world pixel `(x, y)`, at the depth tier of that
    /// spot (scenario setup and tests; gameplay spawns through the director).
    pub fn spawn_enemy(&mut self, kind: EnemyKind, x: f32, y: f32) -> EntityId {
        let tier = self.tier_at(self.frame().tile_at(x, y));
        self.spawn_enemy_with(kind, (x, y), tier, None, None)
    }

    pub(super) fn tier_at(&self, tile: TilePosition) -> u8 {
        // A wild world (Wave 12 planets-a) is past the Heart: its own tiers.
        if let Some(tier) = self.wild_tier(tile) {
            return tier;
        }
        if self.terrain.settings().generator.has_descents() {
            return self.terrain.stratum_at(tile).enemy_tier();
        }
        u8::try_from(self.terrain.band_at(tile).index()).unwrap_or(u8::MAX)
    }

    /// Health multiplier for the crew size: every extra rig adds 30 %, up to 2.2x.
    #[allow(clippy::cast_precision_loss, reason = "a handful of players")]
    fn crew_health_scale(&self) -> f32 {
        let extra = self.players.len().saturating_sub(1) as f32;
        extra.mul_add(0.3, 1.0).min(2.2)
    }

    pub(super) fn spawn_enemy_with(
        &mut self,
        kind: EnemyKind,
        (x, y): (f32, f32),
        tier: u8,
        lair: Option<ChunkPosition>,
        parent: Option<EntityId>,
    ) -> EntityId {
        let now = self.simulation_tick;
        // A burrower appearing in soft rock digs its den there first (its mechanic: it lives
        // in the rock and comes for drilling rigs through real tunnels), so it starts in open
        // space without being moved out of the rock it lurks in.
        if matches!(kind.stats().locomotion, Locomotion::Burrowing) {
            let _dug = self.dig_box(kind, (x, y), kind.stats().radius);
        }
        // Nothing appears inside rock: a spot that overlaps it (a brood hatching against a
        // ceiling, an escort above a lair floor, a scripted spawn) moves to the nearest open
        // space. Stationary fixtures stay where they were placed.
        let (x, y) = if matches!(kind.stats().locomotion, Locomotion::Stationary) {
            (x, y)
        } else {
            let radius = kind.stats().radius;
            // The nearest open spot within a tile, else within a few (a scripted spawn deep in
            // ground a burrower cannot dig, such as a stalker under hard surface rock).
            physics::free_spot_near(&self.terrain, x, y, radius, UNSTICK_PIXELS)
                .or_else(|| {
                    physics::free_spot_near(&self.terrain, x, y, radius, 4.0 * UNSTICK_PIXELS)
                })
                .unwrap_or((x, y))
        };
        let home = self.frame().tile_at(x, y);
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
            .map(|(id, player)| (*id, self.frame().tile_at(player.x, player.y)))
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
            self.spawn_sky_enemies(next_tick);
            self.spawn_lunar_enemies(next_tick);
        }
        if rate.schedule_fires(next_tick, LAIR_CHECK_MILLIS, 500) {
            self.fill_lairs();
            self.fill_sky_lairs();
        }
    }

    fn despawn_far_enemies(&mut self) {
        // Rigs and pilots on foot both keep enemies around (a pilot deep in the Moon's facility
        // keeps its Warden).
        let rigs: Vec<(f32, f32)> = self
            .players
            .values()
            .map(|p| (p.x, p.y))
            .chain(self.pilots().map(|(_, body, pilot)| pilot.middle(body)))
            .collect();
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
            // A tomb floor's creatures (WP-TOMBS) stay however far the party walks: the floor
            // is wider than the despawn reach, and a guardian that vanished would count as
            // felled. Floors clear their own slot when they change or the run ends.
            .filter(|entity| !self.in_run_space(self.frame().tile_at(entity.body.x, entity.body.y)))
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
            let (cx, cy) = self.frame().centre_of(tile.x, tile.y);
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
                TileKind::Air
                    | TileKind::Fluid(_)
                    | TileKind::Lava
                    | TileKind::Gas
                    | TileKind::MagmaVent
            )
        })
    }

    /// Whether an enemy of `kind` can appear at `tile`: the habitat fits and its body is in
    /// open space there (nothing spawns inside rock; a burrower digs its den out of the soft
    /// tile it appears in, see [`Self::spawn_enemy_with`]).
    pub(super) fn site_fits(&self, kind: EnemyKind, tile: TilePosition) -> bool {
        let (x, y) = Self::spawn_point(self.frame(), kind, tile);
        self.habitat_fits(kind, tile)
            && (matches!(kind.stats().locomotion, Locomotion::Burrowing)
                || !physics::collides(&self.terrain, x, y, kind.stats().radius))
    }

    /// Whether the ground around `tile` suits an enemy of `kind`.
    fn habitat_fits(&self, kind: EnemyKind, tile: TilePosition) -> bool {
        if !self.living_spawn_allowed(kind)
            || !Self::drowned_spawn_allowed(kind)
            || !Self::hollow_spawn_allowed(kind)
            || !Self::far_spawn_allowed(kind)
            || !Self::garrison_spawn_allowed(kind)
            || !Self::heart_spawn_allowed(kind)
        {
            return false;
        }
        if let Some(fits) = self.wilds_habitat(kind, tile) {
            return fits;
        }
        if let Some(fits) = self.hollow_habitat(kind, tile) {
            return fits;
        }
        if let Some(fits) = self.heart_habitat(kind, tile) {
            return fits;
        }
        if let Some(fits) = self.frontier_habitat(kind, tile) {
            return fits;
        }
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
            // A burrower lies in wait inside soft rock (it digs its den there when it appears)
            // or in an open pocket against a soft wall it can dig into.
            Locomotion::Burrowing => {
                // The Living Dark's diggers den in what they bore (root, flesh; Wave 8).
                let soft = |ground: TileKind| {
                    matches!(ground, TileKind::Dirt | TileKind::Clay | TileKind::Stone)
                        || (kind.is_living_dark() && diggable_by(kind, ground))
                };
                self.terrain.tile(tile).is_some_and(|t| soft(t.kind))
                    || self.air(tile)
                        && [(0, -1), (1, 0), (0, 1), (-1, 0)]
                            .into_iter()
                            .any(|(dx, dy)| {
                                self.terrain
                                    .tile(TilePosition {
                                        x: tile.x + dx,
                                        y: tile.y + dy,
                                    })
                                    .is_some_and(|t| soft(t.kind))
                            })
            }
            Locomotion::Flying => {
                (self.air(tile) && self.open_area(tile) >= if big { 20 } else { 12 })
                    // The Drowned and Frozen's sea creatures appear swimming (Wave 9).
                    || self.drowned_swim_site(kind, tile)
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
            let roster = self.terrain.creature_roster(tile);
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
    pub(super) fn spawn_point(
        frame: crate::position::Frame,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> (f32, f32) {
        let spec = kind.stats();
        let (x, y) = frame.centre_of(tile.x, tile.y);
        match spec.locomotion {
            Locomotion::Walking | Locomotion::Stationary => {
                #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
                let floor = frame.rel_row(tile.y + 1) * TILE_SIZE;
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
            let point = Self::spawn_point(self.frame(), kind, tile);
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
                        Self::spawn_point(self.frame(), kind, site),
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
                            let point = Self::spawn_point(self.frame(), escort, tile);
                            let _escort = self.spawn_enemy_with(escort, point, tier, None, None);
                        }
                    }
                }
            }
        }
    }

    /// A scare charge's bang (WP-WEAPONS): every creature within [`SCARE_RADIUS`] that can
    /// move runs from the blast for [`SCARE_MILLIS`] (machines and nests don't care).
    fn scare_creatures(&mut self, blast: &Blast) {
        let now = self.simulation_tick;
        let until = self.after(now, SCARE_MILLIS);
        for id in self.enemies_near(blast.x, blast.y, SCARE_RADIUS) {
            let Some((_, mut state)) = self.enemy_parts(id) else {
                continue;
            };
            let spec = state.kind.stats();
            if state.kind.is_machine()
                || spec.elite
                || matches!(spec.locomotion, Locomotion::Stationary)
                || state.mode == EnemyMode::Stunned
            {
                continue;
            }
            state.mode = EnemyMode::Flee;
            state.mode_until = until;
            state.target = state.target.or(blast.owner);
            state.waypoint = None;
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
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

    pub(super) fn enemy_parts(&self, id: EntityId) -> Option<(Body, EnemyState)> {
        self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Enemy(state) => Some((entity.body, state)),
            _ => None,
        })
    }

    pub(super) fn after(&self, now: SimulationTick, millis: u64) -> SimulationTick {
        SimulationTick::new(
            now.get()
                .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
        )
    }

    /// The nearest rig `body` notices, skipping rigs in safe zones and disabled rigs. Nothing
    /// is noticed through rock: prey is seen within sight with a clear line
    /// ([`physics::line_clear`]), and a footstep-feeling enemy ([`crate::night::feels_footsteps`])
    /// also hears prey within earshot around corners, along an open path
    /// ([`Self::sound_reaches`]). Drilling gives nothing away any more.
    fn perceive(&self, body: &Body, state: &EnemyState) -> Option<PlayerId> {
        let spec = state.kind.stats();
        let sight = spec.sight_tiles * TILE_SIZE;
        let hearing = spec.hearing_tiles * TILE_SIZE;
        let reach = sight.max(hearing);
        if self.lure_for(state.kind, body.x, body.y).is_some() {
            // A lit flare is irresistible to bats, and any light to moths: they see nothing
            // else.
            return None;
        }
        // A tomb guardian (WP-TOMBS T28) knows where intruders in its crypt are without seeing
        // them: otherwise a pilot under one of the crypt's platforms makes it forget them, walk
        // home and heal.
        let feels_steps =
            crate::night::feels_footsteps(state.kind) || tomb::is_tomb_guardian(state.kind);
        // Smoke screens (WP-WEAPONS) hide rigs inside them from sight (not from hearing).
        let smoke = self.smoke_screens();
        let hidden = |x: f32, y: f32| {
            smoke.iter().any(|&(sx, sy, radius)| {
                let (dx, dy) = (x - sx, y - sy);
                dx.mul_add(dx, dy * dy) <= radius * radius
            })
        };
        let mut best: Option<(f32, PlayerId)> = None;
        // Rigs (parked ones too) and pilots on foot (Part II F5) are both prey.
        let bodies = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.x, player.y))
            .chain(self.pilots().map(|(_, pilot_body, pilot)| {
                let (x, y) = pilot.middle(pilot_body);
                (pilot.owner, x, y)
            }));
        for (player_id, x, y) in bodies {
            let dx = x - body.x;
            let dy = y - body.y;
            if dx.abs() > reach || dy.abs() > reach {
                continue;
            }
            let distance = dx.hypot(dy);
            if best.is_some_and(|(nearest, _)| nearest <= distance) {
                continue;
            }
            if self.hostile_safe_zone(Some(state.kind), self.frame().tile_at(x, y)) {
                continue;
            }
            let seen = distance <= sight
                && !hidden(x, y)
                && physics::sight_clear(&self.terrain, (body.x, body.y), (x, y));
            // A dune stalker hears footsteps around corners (WP-NIGHT), never through rock.
            let heard = !seen
                && feels_steps
                && distance <= hearing
                && self.sound_reaches((body.x, body.y), (x, y), spec.hearing_tiles);
            if heard || seen {
                best = Some((distance, player_id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Whether a sound at `to` carries to a listener at `from`: an open path of tiles joins them
    /// (around corners, never through rock) no longer than [`SOUND_PATH_STRETCH`] times
    /// `hearing_tiles`. A listener sealed off behind blocks hears nothing.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive tile count"
    )]
    fn sound_reaches(&self, from: (f32, f32), to: (f32, f32), hearing_tiles: f32) -> bool {
        let longest = (hearing_tiles * SOUND_PATH_STRETCH).ceil().max(1.0) as usize;
        physics::find_tile_path(
            &self.terrain,
            self.frame().tile_at(from.0, from.1),
            self.frame().tile_at(to.0, to.1),
            PATH_NODES,
        )
        .is_some_and(|path| path.len() <= longest)
    }

    /// A lit flare within lure range of `(x, y)`, if any (bats circle it instead of hunting).
    pub(super) fn flare_near(&self, x: f32, y: f32) -> Option<(f32, f32)> {
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

    /// Where `player` is to an enemy at `from`: the nearer of its rig and its pilot on foot.
    fn rig_position(&self, player: Option<PlayerId>, from: (f32, f32)) -> Option<(f32, f32)> {
        let player = player?;
        let rig = self
            .players
            .get(&player)
            .filter(|rig| rig.hull > 0.0)
            .map(|rig| (rig.x, rig.y));
        let pilot = self
            .pilot_of(player)
            .map(|(_, body, state)| state.middle(body));
        let distance = |(x, y): (f32, f32)| (x - from.0).hypot(y - from.1);
        match (rig, pilot) {
            (Some(rig), Some(pilot)) => Some(if distance(pilot) < distance(rig) {
                pilot
            } else {
                rig
            }),
            (rig, pilot) => rig.or(pilot),
        }
    }

    /// One enemy's tick (see the module docs), at the creature's own tempo: a far Strange
    /// Layers creature that lives fast or slow, or any creature in a time bubble (Wave 9),
    /// integrates its motion with its own seconds and keeps its timers on its own clock.
    pub(super) fn advance_enemy(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let tempo = self
            .enemy_parts(id)
            .map_or(1.0, |(body, state)| self.far_tempo(&body, &state));
        if (tempo - 1.0).abs() <= f32::EPSILON {
            self.advance_enemy_at_tempo(id, now, output);
            return;
        }
        let dt = self.timing.delta_seconds;
        self.timing.delta_seconds = dt * tempo;
        self.advance_enemy_at_tempo(id, now, output);
        self.timing.delta_seconds = dt;
        self.shift_enemy_timers(id, now, tempo);
    }

    fn advance_enemy_at_tempo(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(start_kind) = self.enemy_parts(id).map(|(_, state)| state.kind) else {
            return;
        };
        // Crypt creatures (WP-TOMBS T29) move on the room grid seen from above.
        if start_kind.is_crypt() {
            self.advance_crypt_enemy(id, now, output);
            return;
        }
        // The Hollow World (Wave 9) carries a ridden crestback, the Sun Serpent and clinging
        // lode mites itself (`world/hollow.rs`, `world/enemies/hollow.rs`).
        if start_kind.is_hollow() && self.hollow_holds(id, start_kind) {
            return;
        }
        // The distortion strata's creatures (Wave 9): twins, rollers, teleporters, grid
        // runners, hanging mantises and gazers drive themselves; a lodestar pulls first.
        if start_kind.is_distortion() && self.advance_distortion_creature(id, now, output) {
            return;
        }
        // The Heart's core and coils (Wave 10) are carried by its fight (`world/enemies/heart.rs`).
        if Self::heart_holds(start_kind) {
            return;
        }
        let Some((start_body, start_state)) = self.enemy_parts(id) else {
            return;
        };
        let mut body = start_body;
        let mut state = start_state;
        let dt = self.timing.delta_seconds;

        // An airship's turret rides its hull, and falls with it.
        if state.kind == EnemyKind::AirshipTurret && !self.ride_airship(&mut body, &state) {
            self.entities.despawn(id);
            return;
        }

        // Fire eats creatures.
        if state.burning_until > now {
            state.health = BURN_DAMAGE_PER_SECOND.mul_add(-dt, state.health);
            if state.health <= 0.0 {
                self.kill_enemy(id, body, state, output);
                return;
            }
        }
        // A side-view tomb guardian's landing slam (WP-TOMBS T28).
        if tomb::is_tomb_guardian(state.kind) {
            self.guardian_tick(id, &body, &state, now, output);
        }
        self.advance_timers(&mut state, now);
        let think_ticks = self.timing.ticks_for_millis(THINK_MILLIS).max(1);
        if (now.get() + id.get()).is_multiple_of(think_ticks) {
            self.think(id, &body, &mut state, now, output);
            // A timber beetle at rest chews the mine timber over it (Wave 8, the Crust).
            if state.kind == EnemyKind::TimberBeetle && !state.mode.hostile() {
                self.beetle_chews(&body, &state, now, output);
            }
        }
        self.free_from_rock(&mut body, state.kind);
        let intent = self.behave(id, &mut body, &mut state, now, output);
        self.move_enemy(id, &mut body, &mut state, intent, now);
        if state.kind.is_drowned() {
            // Seals slide, beetles ride geysers, snails seal tunnels (Wave 9).
            self.drowned_after_move(id, &mut body, &mut state, now);
        }
        if state.kind.is_wilds() {
            // The wild worlds' swimmers stay in the sea (Wave 12 planets-a).
            self.wilds_after_move(&mut body, &state);
        }
        self.keep_off_bodies(id, &mut body, &state);
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
        // A light-shunning night monster runs from a lit flare (WP-NIGHT).
        if self.flare_scares(state.kind, body.x, body.y) {
            state.mode = EnemyMode::Flee;
            state.mode_until = self.after(now, FLEE_MILLIS);
            state.waypoint = None;
            return;
        }
        let spec = state.kind.stats();
        // The Living Dark (Wave 8): factions, harmless kinds and the Mind's thralls.
        // The far Strange Layers (Wave 9): blind hunters hear, The Glitch knows its hall.
        let noticed = self
            // Deep supplies (Wave 10 polish): flares dazzle anglers, bait holds predators.
            .supply_notice(body, state)
            .or_else(|| self.living_notice(body, state))
            // The Drowned and Frozen (Wave 9): the Leviathan knows its arena.
            .or_else(|| self.drowned_notice(body, state))
            .or_else(|| self.hollow_notice(body, state, || self.perceive(body, state)))
            .or_else(|| self.far_notice(body, state))
            // The buried city (Wave 10): the city's watch and the sentinels' searchlights.
            .or_else(|| self.city_notice(body, state))
            // Descent VII part B (Wave 10): a fort's machines know its holders, the Colossus
            // its hall.
            .or_else(|| self.garrison_notice(body, state))
            // The wild worlds (Wave 12 planets-a): the peaceful fight back, ambushers look
            // below, packs see farther in a blizzard.
            .or_else(|| self.wilds_notice(body, state, || self.perceive(body, state)))
            // The frontier worlds (Wave 12): the fauna never hunt, the wyrm hears drilling.
            .or_else(|| self.frontier_notice(body, state))
            .unwrap_or_else(|| self.perceive(body, state));
        let noticed = self.sky_notice(id, body, state, noticed, output);
        let (home_x, home_y) = self.frame().centre_of(state.home.0, state.home.1);
        let leashed = spec.leash_tiles > 0.0
            && state.phase != crate::living_dark::POSSESSED_PHASE
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
                    .rig_position(state.target, (body.x, body.y))
                    .is_none_or(|(x, y)| {
                        self.hostile_safe_zone(Some(state.kind), self.frame().tile_at(x, y))
                    });
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
        if state.mode == EnemyMode::Chase
            && self.hostile_safe_zone(Some(state.kind), self.frame().tile_at(body.x, body.y))
        {
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
        // The Hollow World's creatures have ways of their own (Wave 9).
        if state.kind.is_hollow()
            && let Some(intent) = self.hollow_behave(body, state, now)
        {
            return intent;
        }
        // The wild worlds' (Wave 12 planets-a): herds graze, ambushers wait, packs circle,
        // schools scatter.
        if state.kind.is_wilds()
            && let Some(intent) = self.wilds_behave(body, state, now)
        {
            return intent;
        }
        // And the Heart's (Wave 10): moths to dark matter, beetles to roofs, orbits, eels.
        if state.kind.is_heart()
            && let Some(intent) = self.heart_behave(body, state, now)
        {
            return intent;
        }
        // And the frontier worlds' (Wave 12): fauna run or shell up, moths circle lamps, the
        // drifter rings.
        if state.kind.is_frontier()
            && let Some(intent) = self.frontier_behave(id, body, state, now, output)
        {
            return intent;
        }
        let spec = state.kind.stats();
        let (home_x, home_y) = self.frame().centre_of(state.home.0, state.home.1);
        let target = self.rig_position(state.target, (body.x, body.y));
        let cooldown = self.attack_cooldown_millis(state.kind);
        match state.mode {
            EnemyMode::Dormant | EnemyMode::Stunned => Intent::default(),
            EnemyMode::Idle => {
                if let Some(flare) = self.lure_for(state.kind, body.x, body.y) {
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
                if let Some(flare) = self.lure_for(state.kind, body.x, body.y) {
                    // A flare (or, to moths, any lamp) is irresistible: the swarm forgets
                    // the rig.
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
                // Side-view tomb guardians have patterns of their own (WP-TOMBS T28).
                if tomb::is_tomb_guardian(state.kind) {
                    return self.guardian_chase(id, body, state, (tx, ty), distance, now, output);
                }
                // The Drowned and Frozen (Wave 9): reef sharks circle between dashes.
                if let Some(intent) = self.drowned_chase(state, (tx, ty), ready, now) {
                    return intent;
                }
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
                        // Between attacks a melee enemy hangs back just out of contact instead
                        // of grinding on the rig.
                        let standoff =
                            state.kind.contact_half().0 + RIG_CONTACT_HALF.0 + STANDOFF_PIXELS;
                        let goal = if !ready && distance < standoff {
                            let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                            (nx.mul_add(standoff, tx), ny.mul_add(standoff, ty))
                        } else {
                            (tx, ty)
                        };
                        Intent {
                            goal: Some(goal),
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
                    AttackStyle::Swoop | AttackStyle::Lightning | AttackStyle::Boarding => {
                        self.sky_chase(id, body, state, (tx, ty), distance, now, output)
                    }
                    AttackStyle::Spawner => {
                        if ready && distance <= range {
                            self.hatch_brood(id, body, state.kind, state.tier);
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
                if tomb::is_tomb_guardian(state.kind) {
                    if let Some(aim) = target {
                        self.guardian_release(id, body, state, aim, now, output);
                    } else {
                        state.mode = EnemyMode::Idle;
                    }
                    return Intent::default();
                }
                state.attack_ready = self.after(now, cooldown);
                match (spec.attack, target) {
                    (AttackStyle::Swoop | AttackStyle::Lightning, Some(aim)) => {
                        let _released = self.sky_release(body, state, aim, now, output);
                    }
                    (AttackStyle::Ranged(shot, count), Some(aim)) => {
                        self.fire(id, body, state, shot, count, aim, output);
                        state.mode = EnemyMode::Recover;
                        state.mode_until = self.after(now, RECOVER_MILLIS);
                    }
                    (AttackStyle::Lunge, Some((tx, ty))) => {
                        let ((nx, ny), distance) = direction((body.x, body.y), (tx, ty));
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
                        // The dash lasts long enough to carry it to contact (plus a little), so
                        // a lunge from the edge of its range still lands.
                        let gap =
                            (distance - state.kind.contact_half().0 - RIG_CONTACT_HALF.0).max(0.0);
                        #[allow(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "a dash lasts a fraction of a second"
                        )]
                        let reach_millis = (gap / dash.max(1.0) * 1_000.0) as u64 + 120;
                        let base = if matches!(spec.locomotion, Locomotion::Burrowing) {
                            STRIKE_MILLIS + 250
                        } else {
                            STRIKE_MILLIS
                        };
                        state.mode_until = self.after(now, base.max(reach_millis.min(1_200)));
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
        // The dune wyrm's telegraph (Wave 12 planets-b): the sand ripples and hisses over it.
        if state.kind == EnemyKind::DuneWyrm {
            let tile = self.frame().tile_at(body.x, body.y);
            output.push_event(GameplayEventKind::Frontier {
                what: crate::frontier::FrontierEvent::WyrmRipple,
                player_id: None,
                x: tile.x,
                y: tile.y,
            });
        }
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
        let (home_x, home_y) = self.frame().centre_of(state.home.0, state.home.1);
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
            // A burrower lurks where it is until something draws it: wandering would riddle the
            // ground with tunnels nobody asked for.
            Locomotion::Stationary | Locomotion::Burrowing => Intent::default(),
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
            Locomotion::Flying => Intent {
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
        // The colossus (WP-TOMBS T28) heads straight for its prey across its crypt: paths round
        // the arena's platforms stalled its wide body against them; its leap clears ledges.
        let goal = if state.kind == EnemyKind::SunColossus {
            intent.goal
        } else {
            intent
                .goal
                .map(|goal| self.route(id, body, state, goal, now))
        };
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
                let params = body_params(state.kind, stunned).at_fields(
                    &self.terrain,
                    &self.fields,
                    body.x,
                    body.y,
                );
                let before = (body.x, body.y);
                let _report = physics::step_body(body, &params, &self.terrain, dt);
                self.keep_out_of_fluid(state.kind, before, body);
            }
            Locomotion::Walking => self.walk(id, body, state, goal, intent.speed, now),
            Locomotion::Burrowing => self.burrow(id, now, body, state, goal, intent.speed),
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
        if matches!(spec.locomotion, Locomotion::Stationary) {
            return goal;
        }
        // Follow a waypoint until it is reached or goes stale (no line test per tick while
        // pathing).
        let here = self.frame().tile_at(body.x, body.y);
        if let Some(waypoint) = state.waypoint {
            if waypoint == (here.x, here.y) || now >= state.next_path {
                state.waypoint = None;
            } else {
                return self.frame().centre_of(waypoint.0, waypoint.1);
            }
        }
        if physics::line_clear(&self.terrain, body.x, body.y, goal.0, goal.1) {
            return goal;
        }
        if (now.get() + id.get()).is_multiple_of(PATH_SLOT_TICKS) && now >= state.next_path {
            state.next_path = self.after(now, PATH_RETRY_MILLIS);
            let to = self.frame().tile_at(goal.0, goal.1);
            if let Some(path) = physics::find_tile_path(&self.terrain, here, to, PATH_NODES) {
                // Cut corners: the farthest of the next few path tiles in clear line.
                let next = path
                    .iter()
                    .take(8)
                    .rev()
                    .find(|tile| {
                        let (x, y) = self.frame().centre_of(tile.x, tile.y);
                        physics::line_clear(&self.terrain, body.x, body.y, x, y)
                    })
                    .or_else(|| path.first());
                if let Some(next) = next {
                    state.waypoint = Some((next.x, next.y));
                    return self.frame().centre_of(next.x, next.y);
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
        if state.mode == EnemyMode::Recover && grounded && body.velocity_x.abs() > spec.speed {
            // Done dashing: skid off the dash's speed before backing off.
            body.velocity_x = quantize(body.velocity_x * 6.0f32.mul_add(-dt, 1.0).max(0.0));
        }
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
                if blocked
                    && state.kind == EnemyKind::RogueExcavator
                    && self.bite_due(id, state, now)
                {
                    self.dig_ahead(body, desired.signum());
                } else if blocked || goal_above {
                    let room =
                        !physics::collides(&self.terrain, body.x, body.y - TILE_SIZE, spec.radius);
                    if room {
                        body.velocity_y = -HOP_SPEED;
                    }
                }
            }
        } else if grounded && state.mode != EnemyMode::Strike {
            // Stuns skid to a halt on the floor (a dash keeps its speed until it lands or ends).
            body.velocity_x = quantize(body.velocity_x * 6.0f32.mul_add(-dt, 1.0).max(0.0));
        }
        let params = params.at_fields(&self.terrain, &self.fields, body.x, body.y);
        let before = (body.x, body.y);
        let _report = physics::step_body(body, &params, &self.terrain, dt);
        self.keep_out_of_fluid(state.kind, before, body);
    }

    /// Whether the digging enemy `id` may bite a tile out of its way on the tick `now`: only
    /// while it hunts (never idling, returning or fleeing), and only on its own slot of the
    /// [`BITE_MILLIS`] cycle (staggered by id, like path searches).
    fn bite_due(&self, id: EntityId, state: &EnemyState, now: SimulationTick) -> bool {
        matches!(
            state.mode,
            EnemyMode::Chase | EnemyMode::Windup | EnemyMode::Strike
        ) && (now.get() + id.get()).is_multiple_of(self.timing.ticks_for_millis(BITE_MILLIS).max(1))
    }

    /// An excavator bites the one tile in front of it (its own row first, else the row above;
    /// soft rock only, never ore, fixtures or bedrock). The caller rations it
    /// ([`Self::bite_due`]).
    fn dig_ahead(&mut self, body: &Body, side: f32) {
        let radius = EnemyKind::RogueExcavator.stats().radius;
        let ahead = self
            .frame()
            .tile_at(side.mul_add(radius + 6.0, body.x), body.y);
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
                && self.terrain.set_kind(position, TileKind::Air)
            {
                return;
            }
        }
    }

    /// Burrowing: clinging to rock (any solid tile within a few pixels of its body) a burrower
    /// crawls toward the goal without gravity, along the open tunnels and caves its route finds;
    /// in open air (a lunge, a cave) it falls. Only now and then, while it hunts
    /// ([`Self::bite_due`]), does it bite the one tile of soft rock right ahead of it out of the
    /// terrain ([`diggable`]: never ore, fixtures, built pieces or bedrock, never inside a safe
    /// zone). Its body collides with everything it has not dug, like any other enemy: nothing
    /// ghosts through rock.
    fn burrow(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        body: &mut Body,
        state: &EnemyState,
        goal: Option<(f32, f32)>,
        speed: f32,
    ) {
        let dt = self.timing.delta_seconds;
        let radius = state.kind.stats().radius;
        let clinging = physics::collides(&self.terrain, body.x, body.y, radius + CLING_PIXELS);
        let striking = state.mode == EnemyMode::Strike;
        let controlled = !matches!(
            state.mode,
            EnemyMode::Strike | EnemyMode::Stunned | EnemyMode::Dormant
        );
        if clinging && controlled {
            // Crawling, it goes where it means to (a fall's leftover speed is soaked up at once).
            let (gx, gy) = goal.unwrap_or((body.x, body.y));
            physics::steer_toward(body, gx, gy, speed, speed * 20.0, dt);
        }
        // A rare bite at the rock just ahead along the way it means to go (toward the goal, or
        // along a lunge): one step at its speed plus a short lead, so it opens its way from
        // rest too. Falling through open air digs nothing (it would bore through the floor it
        // lands on).
        let heading = if !self.bite_due(id, state, now) {
            None
        } else if striking {
            let ((nx, ny), _) = direction((0.0, 0.0), (body.velocity_x, body.velocity_y));
            let step = body.velocity_x.hypot(body.velocity_y) * dt;
            Some((nx, ny, step))
        } else if controlled && clinging {
            goal.and_then(|goal| {
                let ((nx, ny), distance) = direction((body.x, body.y), goal);
                (distance >= 1.0).then_some((nx, ny, speed * dt))
            })
        } else {
            None
        };
        if let Some((nx, ny, step)) = heading {
            let ahead = (
                nx.mul_add(step + DIG_LEAD_PIXELS, body.x),
                ny.mul_add(step + DIG_LEAD_PIXELS, body.y),
            );
            let _dug = self.bite_box(state.kind, ahead, radius);
        }
        let params = BodyParams {
            radius,
            gravity: if clinging { 0.0 } else { ENEMY_GRAVITY },
            max_speed: 480.0,
            bounce: 0.0,
            rest_speed: 60.0,
            floor_friction: if clinging { 0.0 } else { 4.0 },
            stop_speed: 0.0,
            gravity_x: 0.0,
            time_scale: 1.0,
        }
        .at_fields(&self.terrain, &self.fields, body.x, body.y);
        let _report = physics::step_body(body, &params, &self.terrain, dt);
    }

    /// Bites out one tile a burrower of `kind` can tunnel ([`diggable_by`]): of those a square
    /// body of half-size `radius` centred on `(x, y)` overlaps (outside safe zones), the one
    /// nearest that centre. Returns whether a tile was dug.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "world pixels saturate into tile coordinates; tile coordinates are small"
    )]
    fn bite_box(&mut self, kind: EnemyKind, (x, y): (f32, f32), radius: f32) -> bool {
        let low = self.frame().tile_at(x - radius, y - radius);
        let high = self.frame().tile_at(x + radius, y + radius);
        let mut nearest: Option<(f32, TilePosition)> = None;
        for ty in low.y..=high.y {
            for tx in low.x..=high.x {
                let position = TilePosition { x: tx, y: ty };
                if !self
                    .terrain
                    .tile(position)
                    .is_some_and(|tile| diggable_by(kind, tile.kind))
                    || self.enemy_safe_zone(position)
                {
                    continue;
                }
                let (cx, cy) = self.frame().centre_of(tx, ty);
                let distance = (cx - x).hypot(cy - y);
                if nearest.is_none_or(|(best, _)| distance < best) {
                    nearest = Some((distance, position));
                }
            }
        }
        nearest.is_some_and(|(_, position)| self.terrain.set_kind(position, TileKind::Air))
    }

    /// Digs out every tile a burrower of `kind` can tunnel ([`diggable_by`]) a square body of
    /// half-size `radius` centred on `(x, y)` overlaps (outside safe zones): its den when it
    /// appears, a pocket when it is wedged. Returns whether a tile was dug.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "world pixels saturate into tile coordinates"
    )]
    fn dig_box(&mut self, kind: EnemyKind, (x, y): (f32, f32), radius: f32) -> bool {
        let low = self.frame().tile_at(x - radius, y - radius);
        let high = self.frame().tile_at(x + radius, y + radius);
        let mut dug = false;
        for ty in low.y..=high.y {
            for tx in low.x..=high.x {
                let position = TilePosition { x: tx, y: ty };
                if self
                    .terrain
                    .tile(position)
                    .is_some_and(|tile| diggable_by(kind, tile.kind))
                    && !self.enemy_safe_zone(position)
                {
                    dug |= self.terrain.set_kind(position, TileKind::Air);
                }
            }
        }
        dug
    }

    /// Resolves an enemy left overlapping rock (a boulder or a built piece landed on it, an old
    /// save, a scripted spawn) out of it the shortest way ([`physics::free_spot_near`], at most
    /// [`UNSTICK_PIXELS`]) instead of letting it pass through. A burrower with no room nearby
    /// digs itself a pocket (its mechanic); anything else stays wedged until there is room.
    /// Stationary enemies (turrets, nests) are fixtures and never move.
    fn free_from_rock(&mut self, body: &mut Body, kind: EnemyKind) {
        let spec = kind.stats();
        if matches!(spec.locomotion, Locomotion::Stationary)
            || !physics::collides(&self.terrain, body.x, body.y, spec.radius)
        {
            return;
        }
        if let Some((x, y)) =
            physics::free_spot_near(&self.terrain, body.x, body.y, spec.radius, UNSTICK_PIXELS)
        {
            body.x = x;
            body.y = y;
            body.previous_x = x;
            body.previous_y = y;
        } else if matches!(spec.locomotion, Locomotion::Burrowing) {
            let _dug = self.dig_box(kind, (body.x, body.y), spec.radius);
        }
    }

    /// The bodies enemies may touch but never overlap: every live rig ([`RIG_CONTACT_HALF`])
    /// and every pilot on foot (its standing or crouching box), as `(centre, half extents)`.
    fn prey_boxes(&self) -> Vec<((f32, f32), (f32, f32))> {
        self.players
            .values()
            .filter(|player| player.hull > 0.0)
            .map(|player| ((player.x, player.y), RIG_CONTACT_HALF))
            .chain(self.pilots().map(|(_, body, pilot)| {
                (pilot.middle(body), (PILOT_HALF_WIDTH, pilot.height() * 0.5))
            }))
            .collect()
    }

    /// Enemies touch rigs and pilots but never sit on top of or inside them: an enemy whose
    /// contact box ([`EnemyKind::contact_half`]) overlaps one is pushed out along the shallower
    /// axis to exactly touching (only into open space: rock wins over the push), and its
    /// velocity into that body is cancelled. Melee attacks reach [`CONTACT_MARGIN`] beyond
    /// touching, so a pressed enemy still bites.
    fn keep_off_bodies(&self, id: EntityId, body: &mut Body, state: &EnemyState) {
        let radius = state.kind.stats().radius;
        if matches!(state.kind.stats().locomotion, Locomotion::Stationary) {
            return;
        }
        let (own_w, own_h) = state.kind.contact_half();
        // Coincident centres split by id so a swarm fans out to both sides.
        let tie = if id.get().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        for ((cx, cy), (half_w, half_h)) in self.prey_boxes() {
            let (dx, dy) = (body.x - cx, body.y - cy);
            let reach_x = own_w + half_w;
            let reach_y = own_h + half_h;
            let (depth_x, depth_y) = (reach_x - dx.abs(), reach_y - dy.abs());
            if depth_x <= 0.0 || depth_y <= 0.0 {
                continue;
            }
            let side_x = if dx.abs() < 0.01 { tie } else { dx.signum() };
            let side_y = if dy.abs() < 0.01 { -1.0 } else { dy.signum() };
            let along_x = (quantize(side_x.mul_add(reach_x, cx)), body.y);
            let along_y = (body.x, quantize(side_y.mul_add(reach_y, cy)));
            let tries = if depth_x <= depth_y {
                [(along_x, true), (along_y, false)]
            } else {
                [(along_y, false), (along_x, true)]
            };
            for ((x, y), horizontal) in tries {
                if physics::collides(&self.terrain, x, y, radius) {
                    continue;
                }
                body.x = x;
                body.y = y;
                if horizontal && body.velocity_x * side_x < 0.0 {
                    body.velocity_x = 0.0;
                } else if !horizontal && body.velocity_y * side_y < 0.0 {
                    body.velocity_y = 0.0;
                }
                break;
            }
        }
    }

    /// Whether an enemy contact box with half extents `own` at `body` touches the box centred
    /// on `centre` with half extents `half` (within [`CONTACT_MARGIN`]).
    fn touching(body: &Body, own: (f32, f32), centre: (f32, f32), half: (f32, f32)) -> bool {
        (body.x - centre.0).abs() <= own.0 + half.0 + CONTACT_MARGIN
            && (body.y - centre.1).abs() <= own.1 + half.1 + CONTACT_MARGIN
    }

    /// Bites, rams and charges: touching a rig (or a pilot on foot) hurts while the attack is
    /// live. A hit springs the attacker back off its victim (lunges then recover, swarmers circle
    /// until their next bite), so nothing keeps grinding on a rig inside its i-frames.
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
            // A ranged enemy only strikes in a tomb guardian's dive (WP-TOMBS T28).
            AttackStyle::Lunge | AttackStyle::Swoop | AttackStyle::Ranged(..) => {
                state.mode == EnemyMode::Strike
            }
            AttackStyle::Spawner | AttackStyle::Lightning | AttackStyle::Boarding => false,
        };
        if !live {
            return;
        }
        let own = state.kind.contact_half();
        // A rig still inside its i-frames doesn't shield a crewmate touching the same enemy.
        let victim = self.players.iter().find_map(|(player_id, player)| {
            (Self::touching(body, own, (player.x, player.y), RIG_CONTACT_HALF)
                && !self.enemies.guarded(*player_id, now))
            .then_some((*player_id, (player.x, player.y)))
        });
        let damage = spec.contact_damage * tier_scale(state.tier);
        let (landed, from) = if let Some((player_id, at)) = victim {
            let landed = self.strike_rig(
                player_id,
                damage,
                (body.x, body.y),
                spec.knockback,
                state.kind,
                now,
                output,
            );
            if landed && state.kind == EnemyKind::GlowGrub {
                self.glow_grub_bite(player_id);
            }
            if landed && state.kind.is_drowned() {
                // Frost, ink and gulps (Wave 9, the Drowned and Frozen).
                self.drowned_bite(body, state, player_id, now, output);
            }
            if landed && state.kind.is_heart() {
                // A null moth drinks dark matter (Wave 10, the Heart).
                self.heart_bite(body, state, player_id, now, output);
            }
            (landed, at)
        } else if let Some((pilot, middle)) = self
            .pilots_near(body.x, body.y, spec.radius + PILOT_HALF_WIDTH + 14.0)
            .first()
            .copied()
            .or_else(|| {
                self.pilots().find_map(|(pilot, pilot_body, pilot_state)| {
                    let middle = pilot_state.middle(pilot_body);
                    Self::touching(
                        body,
                        own,
                        middle,
                        (PILOT_HALF_WIDTH, pilot_state.height() * 0.5),
                    )
                    .then_some((pilot, middle))
                })
            })
        {
            // A pilot on foot is softer: no hull between it and the teeth.
            let landed = self.strike_pilot_as(
                Some(state.kind),
                pilot,
                damage,
                (body.x, body.y),
                spec.knockback,
                output,
            );
            (landed, middle)
        } else {
            return;
        };
        if landed {
            state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
            if state.mode == EnemyMode::Strike {
                state.mode = EnemyMode::Recover;
                state.mode_until = self.after(now, RECOVER_MILLIS);
            }
            // Recoil off the victim.
            let ((nx, ny), _) = direction(from, (body.x, body.y));
            let recoil = RECOIL_SPEED.max(spec.speed);
            if matches!(spec.locomotion, Locomotion::Walking) {
                body.velocity_x = quantize(if nx.abs() < 0.05 {
                    f32::from(state.facing.signum()) * -recoil
                } else {
                    nx.signum() * recoil
                });
                body.velocity_y = quantize(body.velocity_y.min(-90.0));
            } else {
                body.velocity_x = quantize(nx * recoil);
                body.velocity_y = quantize(ny * recoil);
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
        if player.hull <= 0.0
            || self.hostile_safe_zone(Some(kind), self.frame().tile_at(player.x, player.y))
        {
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
        let mut damage = damage * self.enemies.difficulty.damage();
        if kind.is_lunar()
            && self.players.get(&player_id).is_some_and(|player| {
                player.hazard_gear.pressurized_hull >= crate::moon::HULL_ARMOUR_TIER
            })
        {
            // A void-rated hull turns a fifth of Moon creatures' bites (WP-MOON).
            damage *= 0.8;
        }
        if self.damage_player(player_id, damage) {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.hull = quantize(player.hull);
            }
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
        }
        if kind == EnemyKind::AmberSloth {
            // An amber sloth's touch holds the rig in amber (Wave 9, the Time Sludge).
            self.amber_rig(player_id, output);
        }
        if kind.is_ancient_city() {
            // A scrap crawler's theft, a spark wisp's jolt (Wave 10, the buried city).
            self.city_strike(player_id, kind, from, output);
        }
        if kind == EnemyKind::MudLeech
            && let Some(player) = self.players.get_mut(&player_id)
        {
            // A mud leech drinks fuel rather than denting the hull (Wave 8, the Crust).
            player.fuel = quantize((player.fuel - MUD_LEECH_FUEL_SIP).max(0.0));
            #[allow(clippy::cast_possible_truncation, reason = "world pixels fit i32")]
            output.push_event(GameplayEventKind::CrustCreature {
                action: crate::crust::CrustCreatureAction::LeechLatched,
                x: from.0 as i32,
                y: from.1 as i32,
            });
        }
        true
    }

    /// A resting timber beetle now and then chews through a timber set a few rows over it
    /// (about one in six thoughts; the set then fails like any cut beam, see `world/crust.rs`).
    fn beetle_chews(
        &mut self,
        body: &Body,
        state: &EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let roll = hash2(
            u64::from(state.seed),
            0xC705_BEE7,
            i32::try_from(now.get() & 0x7FFF_FFFF).unwrap_or(0),
            0,
        );
        if !roll.is_multiple_of(6) {
            return;
        }
        let tile = self.frame().tile_at(body.x, body.y);
        let beam = (1..=5).find_map(|rise| {
            let at = TilePosition {
                x: tile.x,
                y: tile.y - rise,
            };
            (self.terrain.tile(at).map(|found| found.kind)
                == Some(TileKind::Crust(crate::crust::CrustTile::Timber)))
            .then_some(at)
        });
        if let Some(beam) = beam
            && self.terrain.set_kind(beam, TileKind::Air)
        {
            #[allow(clippy::cast_possible_truncation, reason = "world pixels fit i32")]
            output.push_event(GameplayEventKind::CrustCreature {
                action: crate::crust::CrustCreatureAction::BeetleChewed,
                x: body.x as i32,
                y: body.y as i32,
            });
        }
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
        for (index, (cos, sin)) in turns.iter().enumerate() {
            // A splitter spire's fan is one bolt of each colour (Wave 9, the Prism Desert).
            let shot = if state.kind == EnemyKind::SplitterSpire {
                far_strange::prism_shot(index)
            } else {
                shot
            };
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
    fn hatch_brood(&mut self, id: EntityId, body: &Body, spawner: EnemyKind, tier: u8) {
        let brood = self
            .enemies()
            .filter(|(_, state)| state.parent == Some(id.get()))
            .count();
        if brood >= NEST_BROOD || self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
            return;
        }
        let radius = spawner.stats().radius;
        let hatchling = self.spawn_enemy_with(
            living::brood_of(spawner),
            (body.x, body.y - radius - 6.0),
            tier,
            None,
            Some(id),
        );
        self.entities.update(hatchling, |entity| {
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
        // Hex bolts, arrows and boulders of the crypts (WP-TOMBS T29).
        if shot.kind.is_crypt() {
            self.advance_crypt_shot(id, now, output);
            return;
        }
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        let gravity = shot.kind.gravity()
            * crate::environment::environment_in(
                self.terrain.settings(),
                self.frame(),
                body.x,
                body.y,
            )
            .gravity;
        body.velocity_y = quantize(gravity.mul_add(dt, body.velocity_y));
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
                self.burst_spore_shot(shot.kind, body.previous_x, body.previous_y, output);
                // A siege shell breaks the defences it lands among (Wave 10).
                self.garrison_shell_burst(shot.kind, body.previous_x, body.previous_y, output);
                self.entities.despawn(id);
                return;
            }
            // A prism bolt passes through a rig whose lamp shines its colour (Wave 9).
            let passes = |player_id: PlayerId| {
                shot.kind
                    .prism_colour()
                    .is_some_and(|colour| self.lamp_filter_of(player_id) == colour)
            };
            let victim = self.players.iter().find_map(|(player_id, player)| {
                ((player.x - body.x).hypot(player.y - body.y) <= reach && !passes(*player_id))
                    .then_some(*player_id)
            });
            if let Some(player_id) = victim {
                let landed = self.strike_rig(
                    player_id,
                    shot.damage,
                    (body.x, body.y),
                    120.0,
                    shot.shooter,
                    now,
                    output,
                );
                // A dead pixel's rot pulse rots the view of the rig it hits (Wave 9).
                if landed && shot.kind == ShotKind::RotPulse {
                    self.rot_burst(player_id, now, output);
                }
                if !self.damage_immune() {
                    self.burst_spore_shot(shot.kind, body.x, body.y, output);
                    self.garrison_shell_burst(shot.kind, body.x, body.y, output);
                    self.entities.despawn(id);
                    return;
                }
            } else if let Some((pilot, _)) = self
                .pilots_near(body.x, body.y, shot.kind.radius() + PILOT_HALF_WIDTH + 6.0)
                .first()
                .copied()
            {
                let _landed = self.strike_pilot_as(
                    Some(shot.shooter),
                    pilot,
                    shot.damage,
                    (body.x, body.y),
                    120.0,
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

    /// Applies the enemy damage model's `damage` to entity `id` (resistances, shields, EMP
    /// stuns, burning, aggro, flinch, fleeing, loot; see the module docs). Anything but an enemy
    /// is [`DamageOutcome::Ignored`]. Weapons and other callers use
    /// `WorldState::damage_entity` (`crate::combat`), which routes enemies here.
    #[allow(clippy::too_many_lines, reason = "the damage model's steps in order")]
    pub fn damage_enemy(
        &mut self,
        id: EntityId,
        damage: Damage,
        output: &mut TickOutput,
    ) -> DamageOutcome {
        let Some((mut body, mut state)) = self.enemy_parts(id) else {
            return DamageOutcome::Ignored;
        };
        // The Mycelial Mind cannot be hurt while its nerve nodes live (Wave 8), nor The Glitch
        // while it rewrites its arena (Wave 9).
        if self.mind_shields(state.kind)
            || self.far_ignores_damage(&state)
            || self.garrison_ignores_damage(&state)
        {
            return DamageOutcome::Ignored;
        }
        // A hit on one of the Colossus's parts passes to its core (Wave 10).
        if state.kind.is_colossus_part()
            && let Some(outcome) = self.garrison_damage(id, &state, damage, output)
        {
            return outcome;
        }
        // The Heart's core is sealed behind its coils and armoured between breaths (Wave 10).
        if let Some(outcome) = self.heart_damage(id, &state) {
            return outcome;
        }
        // A glassback's mirrored face throws a frontal shot back at its shooter (Wave 9).
        if let Some(at) =
            self.glassback_reflects(&body, &state, damage.source.player(), damage.kind)
        {
            state.last_attacker = damage.source.player();
            self.reflect_shot(id, &body, &state, at, output);
            return DamageOutcome::Ignored;
        }
        let now = self.simulation_tick.next();
        let spec = state.kind.stats();
        let mut amount = (damage.amount
            * state.kind.resistance(damage.kind)
            * self.far_damage_scale(id, &state, damage.kind, damage.source))
        .max(0.0)
            // The Leviathan's open maw takes double (Wave 9).
            * self.drowned_damage_factor(state.kind)
            // The unbound Colossus's bare core (Wave 10).
            * self.garrison_damage_factor(state.kind);
        // The Sun Serpent's armour and scales (Wave 9).
        if state.kind.is_hollow()
            && let Some(outcome) = self.hollow_damage(id, &state, amount, output)
        {
            return outcome;
        }
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
        // A rewind newt badly hurt rewinds three seconds (Wave 9, the Time Sludge).
        let _rewound = self.try_rewind(id, &mut body, &mut state, output);
        let skittish = matches!(
            state.kind,
            EnemyKind::CaveBat
                | EnemyKind::AcidSpitter
                | EnemyKind::MagmaWisp
                | EnemyKind::WillOWisp
                | EnemyKind::MothSwarm
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
        if state.kind == EnemyKind::PirateAirship {
            self.sink_airship(id, &body, output);
        }
        let roll = hash2(
            self.terrain.seed() ^ u64::from(state.seed),
            SALT_LOOT,
            i32::try_from(id.get() & 0x7FFF_FFFF).unwrap_or(0),
            i32::from(state.tier),
        );
        for (index, stack) in (0_u8..).zip(loot(state.kind, state.tier, roll)) {
            let spread = (f32::from(index % 5) - 2.0) * 45.0;
            // Through the drop rules (WP-DROPS): the kind's lifetime and the per-cell cap.
            let _drop = self.spawn_loot(
                body.x,
                body.y,
                (spread, -150.0 - f32::from(index % 3) * 30.0),
                stack,
                None,
            );
        }
        if let Some(lair) = state.lair {
            self.enemies.cleared_lairs.insert(lair);
        }
        if state.kind == EnemyKind::Warden {
            // WP-MOON: the core seal opens.
            self.warden_fell(state.last_attacker, output);
        }
        if crate::night::is_nocturnal(state.kind) {
            self.enemies.night.record_kill(state.kind);
        }
        if state.kind.is_living_dark() {
            // Errands, the Swarm, the Court and the Mind's fight (Wave 8).
            let tile = self.frame().tile_at(body.x, body.y);
            self.living_dark_kill(id.get(), state.kind, tile, state.last_attacker, output);
        }
        if state.kind.is_drowned() {
            // The eel-dragon's bounty and the Leviathan's gate (Wave 9).
            let tile = self.frame().tile_at(body.x, body.y);
            self.drowned_kill(state.kind, tile, state.last_attacker, output);
        }
        if state.kind.is_hollow() {
            // Bladders burst, the serpent falls (Wave 9).
            self.hollow_kill(state.kind, &body, state.last_attacker, output);
        }
        if state.kind.is_distortion() {
            // A twin's hall rests (Wave 9).
            let tile = self.frame().tile_at(body.x, body.y);
            self.distortion_kill(state.kind, state.home, tile, state.last_attacker, output);
        }
        if state.kind.is_far_strange() {
            // The Glitch's fight and its shards, the newts' memories (Wave 9).
            let tile = self.frame().tile_at(body.x, body.y);
            self.far_kill(id.get(), state.kind, tile, state.last_attacker, output);
        }
        if state.kind.is_ancient_city() {
            // A scrap crawler drops the ore it carried off (Wave 10).
            self.city_kill(id.get(), body.x, body.y);
        }
        if state.kind.is_garrison() {
            // A fort's wave thins, the Colossus falls (Wave 10).
            let tile = self.frame().tile_at(body.x, body.y);
            self.garrison_kill(id.get(), state.kind, tile, state.last_attacker, output);
        }
        if state.kind.is_heart() {
            // The Heart's coils and sparks, and the Heart waking for good (Wave 10).
            let tile = self.frame().tile_at(body.x, body.y);
            self.heart_kill(id.get(), state.kind, tile, state.last_attacker, output);
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
                    let _outcome = self.damage_enemy(
                        id,
                        Damage::new(EMP_DAMAGE, DamageKind::Emp, source),
                        output,
                    );
                }
                continue;
            }
            if blast.kind == ExplosiveKind::Scare {
                self.scare_creatures(blast);
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
                let _outcome = self.damage_enemy(
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
        ExplosiveKind::Shaped | ExplosiveKind::Rocket => 45.0,
        ExplosiveKind::Mining => 50.0,
        ExplosiveKind::Incendiary => 28.0,
        ExplosiveKind::Emp
        | ExplosiveKind::Flare
        | ExplosiveKind::Smoke
        | ExplosiveKind::Scare
        | ExplosiveKind::TimeBubble => {
            return None;
        }
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
#[allow(
    clippy::match_same_arms,
    clippy::too_many_lines,
    reason = "each family's habitat reads by itself"
)]
const fn cave_habitat_fits(kind: EnemyKind, tags: crate::cave::CaveSpawnTags) -> bool {
    match kind {
        EnemyKind::CaveBat | EnemyKind::CrystalMite => tags.swarmers,
        EnemyKind::Burrower | EnemyKind::RegolithCrawler => tags.burrowers,
        EnemyKind::AcidSpitter | EnemyKind::RockMimic | EnemyKind::BroodNest => tags.nests,
        EnemyKind::Shellback => tags.aquatic || tags.burrowers,
        EnemyKind::MagmaWisp => tags.heat,
        EnemyKind::MiningDrone
        | EnemyKind::SentryTurret
        | EnemyKind::RogueExcavator
        | EnemyKind::AncientGuardian
        | EnemyKind::DerelictAutomaton
        | EnemyKind::Warden
        | EnemyKind::PistonWarden => tags.machines,
        // The Old Deep's creatures (Wave 8) roost where their kin do.
        EnemyKind::PrismMoth | EnemyKind::JetRay | EnemyKind::VoidJelly => tags.swarmers,
        EnemyKind::FossilEel => tags.burrowers || tags.aquatic,
        EnemyKind::GlowGrub => tags.nests || tags.swarmers,
        EnemyKind::MagmaSalamander => tags.heat || tags.nests,
        EnemyKind::Glowmother => false,
        EnemyKind::BroodLord => tags.elite_lair,
        // The Crust (Wave 8): moles in burrows, leeches and wasps by water, bats and beetles in
        // the old works' caves, skinks and crabs on cave floors, tube things in the heat.
        EnemyKind::BurrowMole => tags.burrowers,
        EnemyKind::MudLeech | EnemyKind::PotterWasp => tags.aquatic || tags.swarmers,
        EnemyKind::MineBat => tags.swarmers,
        EnemyKind::TimberBeetle | EnemyKind::SlateSkink | EnemyKind::ShelfCrab => tags.nests,
        EnemyKind::TubeCrawler | EnemyKind::CinderMoth => tags.heat,
        // Sky families never live in caves.
        EnemyKind::SkyRay
        | EnemyKind::Wyvern
        | EnemyKind::StormElemental
        | EnemyKind::PirateAirship
        | EnemyKind::AirshipTurret
        | EnemyKind::SkyPirate
        | EnemyKind::HarpyNester
        | EnemyKind::ShadowHound
        | EnemyKind::DuneStalker
        | EnemyKind::WillOWisp
        | EnemyKind::MothSwarm
        | EnemyKind::UmbralAlpha
        // Tomb creatures (T28) live only in tomb runs.
        | EnemyKind::Mummy
        | EnemyKind::CryptBat
        | EnemyKind::SkeletalArcher
        | EnemyKind::AnimatedStatue
        | EnemyKind::SunColossus
        | EnemyKind::HollowMatriarch
        // Crypt creatures (T29) live only in crypt runs.
        | EnemyKind::CryptSkeleton
        | EnemyKind::Spectre
        | EnemyKind::Cultist
        | EnemyKind::CryptGolem
        | EnemyKind::CryptLord => false,
        // The Living Dark (Wave 8): swarmers roost in caves, burrowers tunnel soft walls,
        // nest-builders and ambushers nest; the bosses and the hive folk live in their
        // set pieces.
        EnemyKind::GlowGnat
        | EnemyKind::HiveSoldier
        | EnemyKind::Leechling
        | EnemyKind::ThornWisp
        | EnemyKind::GlowJelly
        | EnemyKind::LureFish => tags.swarmers || tags.aquatic,
        EnemyKind::BarkBeetle | EnemyKind::MawWorm => tags.burrowers,
        EnemyKind::SporePuffer
        | EnemyKind::CapCrawler
        | EnemyKind::RootSnapper
        | EnemyKind::AcidLancer
        | EnemyKind::BilePolyp
        | EnemyKind::Angler => tags.nests,
        EnemyKind::HiveDrone
        | EnemyKind::SwarmMatriarch
        | EnemyKind::MycelialMind
        | EnemyKind::MindNode => false,
        // The Drowned and Frozen (Wave 9): the sea's swimmers in watery caves, mites with the
        // swarmers, the walkers in roomy caves; the bosses live in their set pieces.
        EnemyKind::ReefShark
        | EnemyKind::InkSquid
        | EnemyKind::Spinepuff
        | EnemyKind::LureAngler
        | EnemyKind::GulperEel => tags.aquatic || tags.swarmers,
        EnemyKind::FrostMite => tags.swarmers,
        EnemyKind::TuskSeal | EnemyKind::ScaldBeetle | EnemyKind::SinterSnail => {
            tags.nests || tags.burrowers
        }
        EnemyKind::EelDragon | EnemyKind::Leviathan => false,
        // The Hollow World (Wave 9): its creatures live in the open caverns, placed by the
        // Hollow World's own roster (`world/enemies/hollow.rs`), not in cave lairs.
        EnemyKind::SkyManta
        | EnemyKind::StoopRoc
        | EnemyKind::GustBladder
        | EnemyKind::Crestback
        | EnemyKind::Skyneck
        | EnemyKind::Fangrunner
        | EnemyKind::Hangclaw
        | EnemyKind::LodeMite
        | EnemyKind::CinderDrake
        | EnemyKind::ShadeStalker
        | EnemyKind::Flarefly
        | EnemyKind::SunSerpent
        | EnemyKind::SerpentCoil => false,
        // The distortion strata (Wave 9): flyers roost, floor-dwellers nest; twins only ever
        // step out of a mirror hall.
        EnemyKind::Lodestar | EnemyKind::Glitchling | EnemyKind::Bitworm | EnemyKind::DeadPixel => {
            tags.swarmers || tags.nests
        }
        EnemyKind::Glassback | EnemyKind::Gazer | EnemyKind::Tumbler | EnemyKind::ShearMantis => {
            tags.nests
        }
        EnemyKind::MirrorTwin => false,
        // The far Strange Layers (Wave 9): fliers roost in caves, walkers nest on floors, the
        // spire roots where nests do; The Glitch and its shards live only in its arena.
        EnemyKind::Shrieker | EnemyKind::HushMoth | EnemyKind::LensWraith => tags.swarmers,
        EnemyKind::TickMite | EnemyKind::RewindNewt => tags.swarmers || tags.nests,
        EnemyKind::AmberSloth
        | EnemyKind::EchoStalker
        | EnemyKind::PrismScarab
        | EnemyKind::SplitterSpire => tags.nests,
        EnemyKind::TheGlitch | EnemyKind::GlitchShard => false,
        // The buried city (Wave 10): fliers roost in any open space, walkers on floors.
        EnemyKind::MaintenanceDrone | EnemyKind::SparkWisp | EnemyKind::PageMoth => {
            tags.swarmers || tags.nests
        }
        EnemyKind::ScrapCrawler
        | EnemyKind::StreetSentinel
        | EnemyKind::CogRoller
        | EnemyKind::RivetHound
        | EnemyKind::ArchiveWarden
        | EnemyKind::IndexSpider => tags.nests || tags.swarmers,
        // Descent VII part B (Wave 10): sentries and walkers stand on floors, drones and
        // wardens hover in caves, crawlers nest; the Colossus lives only in its hall.
        EnemyKind::WarSentry | EnemyKind::SiegeWalker | EnemyKind::GildedCrawler => tags.nests,
        EnemyKind::LancerDrone | EnemyKind::SealWarden => tags.swarmers,
        EnemyKind::GatekeeperColossus
        | EnemyKind::ColossusHead
        | EnemyKind::ColossusFist
        | EnemyKind::ColossusLegs => false,
        // The Heart (Wave 10): fliers roost in caves, the gnawer tunnels, the lurker and the
        // warden nest; the eels live in the metal sea and the leeches on the rails (the Heart's
        // own roster places them); the boss and its parts live only in the chamber.
        EnemyKind::NullMoth | EnemyKind::AccretionMote | EnemyKind::CoronaWisp => tags.swarmers,
        EnemyKind::VeinGnawer => tags.burrowers,
        EnemyKind::UpfallBeetle | EnemyKind::Orbiter | EnemyKind::SlagSkimmer => {
            tags.swarmers || tags.nests
        }
        EnemyKind::HorizonLurker | EnemyKind::CableWarden => tags.nests || tags.machines,
        EnemyKind::MercuryEel
        | EnemyKind::RailLeech
        | EnemyKind::StarHeart
        | EnemyKind::DriveCoil
        | EnemyKind::HeartSpark => false,
        // The wild worlds (Wave 12 planets-a): placed by their planet's own roster
        // (`world/enemies/wilds.rs`), never in the home world's cave lairs.
        EnemyKind::Trundlehorn
        | EnemyKind::Vinestalker
        | EnemyKind::Mossprowler
        | EnemyKind::Sporewing
        | EnemyKind::Shellstrider
        | EnemyKind::Glimmerfin
        | EnemyKind::Sawjaw
        | EnemyKind::Tidesnapper
        | EnemyKind::Woolhorn
        | EnemyKind::Rimeskipper
        | EnemyKind::Rimewolf
        | EnemyKind::Crevassewyrm
        // The frontier worlds' creatures have their own spawner (Wave 12 planets-b).
        | EnemyKind::DuneSkipper
        | EnemyKind::DuneWyrm
        | EnemyKind::GlassScorpion
        | EnemyKind::MirageMoth
        | EnemyKind::MagmaSlug
        | EnemyKind::CinderBat
        | EnemyKind::EmberStrider
        | EnemyKind::BasaltTortoise
        | EnemyKind::ShardCrab
        | EnemyKind::GlintMoth
        | EnemyKind::ChimeDrifter
        | EnemyKind::GeodeMimic => false,
    }
}
