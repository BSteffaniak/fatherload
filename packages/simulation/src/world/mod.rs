//! The canonical [`WorldState`] and its fixed-tick transition, split by gameplay domain.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    contract::ContractLog,
    economy::{DeepClaimStatus, TownDevelopment},
    entity::EntityStore,
    hazard::{HazardCloud, PlacedBomb, ScanMarker},
    ids::{PlayerId, SimulationTick},
    infrastructure::PlacedInfrastructure,
    player::Player,
    progression::{Expedition, SideContract, WorldEventKind},
    rig::{DrillState, RigPartKind},
    rng::DeterministicRngState,
    service::PlayerServiceTransaction,
    summary::PlayerSurvivalSnapshot,
    terrain::{MineralKind, Terrain, TilePosition},
};

mod altitude;
mod ancient_city;
#[cfg(test)]
mod ancient_city_tests;
#[cfg(test)]
mod arc_tests;
mod arcs;
#[cfg(test)]
mod base_tests;
mod bases;
#[cfg(test)]
mod bomb_tests;
mod bombs;
#[cfg(test)]
mod boulder_tests;
#[cfg(test)]
mod cave_tests;
mod caves;
#[cfg(test)]
mod charge_equivalence_tests;
#[cfg(test)]
mod climb_tests;
mod clock;
#[cfg(test)]
mod clock_tests;
mod commands;
mod crew;
mod crust;
#[cfg(test)]
mod crust_tests;
#[cfg(test)]
mod crypt_tests;
mod crypts;
#[cfg(test)]
mod deep_arc_tests;
#[cfg(test)]
mod deep_folk_tests;
#[cfg(test)]
mod deep_progression_tests;
#[cfg(test)]
mod deep_wiring_tests;
#[cfg(test)]
mod descent_tests;
#[cfg(test)]
mod determinism_tests;
mod discovery;
mod distortion;
#[cfg(test)]
mod distortion_tests;
#[cfg(test)]
mod drill_tests;
#[cfg(test)]
mod drop_economy_tests;
#[cfg(test)]
mod drop_tests;
pub mod drops;
mod drowned;
#[cfg(test)]
mod drowned_tests;
mod enemies;
#[cfg(test)]
mod enemy_body_tests;
#[cfg(test)]
mod enemy_tests;
mod entities;
#[cfg(test)]
mod entity_tests;
pub mod expedition;
#[cfg(test)]
mod expedition_tests;
#[cfg(test)]
mod explosive_tests;
mod far_strange;
#[cfg(test)]
mod far_strange_tests;
#[cfg(test)]
mod field_tests;
mod fields;
mod flight;
#[cfg(test)]
mod flight_tests;
#[cfg(test)]
mod fluid_tests;
mod fluids;
mod frame;
#[cfg(test)]
mod frame_tests;
mod garage;
mod garrison;
#[cfg(test)]
mod garrison_tests;
#[cfg(test)]
mod god_mode_tests;
#[cfg(test)]
mod hazard_counter_tests;
mod hazards;
mod heart;
#[cfg(test)]
mod heart_tests;
#[cfg(test)]
mod star_flight_tests;
pub use flight::warp_arrival as flight_warp_arrival;
pub use heart::CablePath;
mod body_flight;
#[cfg(test)]
mod body_map_tests;
mod bodylife;
#[cfg(test)]
mod bodylife_tests;
pub use bodylife::{BodyLifeEvent, body_life_of};
mod hollow;
#[cfg(test)]
mod hollow_tests;
#[cfg(test)]
mod home_tests;
mod homes;
mod import;
mod living_dark;
#[cfg(test)]
mod living_dark_tests;
mod logistics;
#[cfg(test)]
mod logistics_tests;
mod mechanisms;
mod milestones;
mod moon;
#[cfg(test)]
mod moon_tests;
mod movement;
mod night;
#[cfg(test)]
mod night_tests;
mod no_clip;
mod old_deep;
#[cfg(test)]
mod old_deep_tests;
mod people;
#[cfg(test)]
mod people_tests;
#[cfg(test)]
mod persistence_tests;
pub mod pilot;
mod pilot_arms;
#[cfg(test)]
mod pilot_content_tests;
mod pilot_gear;
#[cfg(test)]
mod pilot_tests;
mod pilot_weather;
pub(crate) mod places;
#[cfg(test)]
mod places_tests;
mod space_systems;
#[cfg(test)]
mod space_systems_tests;
mod wilds;
#[cfg(test)]
mod wilds_tests;
pub use body_flight::{
    BodyGroundInfo, LAND_INSIDE_ROWS, TAKE_OFF_ABOVE_ROWS, strip_point, strip_position,
};
pub mod planet;
#[cfg(test)]
mod planet_tests;
mod player_sections;
#[cfg(test)]
mod poi_tests;
mod pois;
mod progression;
#[cfg(test)]
mod quest_tests;
mod quests;
mod replication;
#[cfg(test)]
mod replication_tests;
mod rescue;
mod roster;
mod services;
#[cfg(test)]
mod side_tomb_tests;
mod simulate;
#[cfg(test)]
mod sky_continent_tests;
#[cfg(test)]
mod sky_isle_tests;
mod sky_isles;
mod sky_temples;
#[cfg(test)]
mod sky_tests;
mod sky_weapons;
#[cfg(test)]
mod soak_tests;
mod sonar;
#[cfg(test)]
mod sonar_tests;
#[cfg(test)]
mod space_journey_tests;
mod state_hash;
#[cfg(test)]
mod story_finale_tests;
#[cfg(test)]
mod streaming_tests;
mod supplies;
mod test_mode;
#[cfg(test)]
mod test_mode_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tick_rate_tests;
mod timing;
#[cfg(test)]
mod tomb_tests;
mod tomb_traps;
mod tombs;
#[cfg(test)]
mod village_tests;
mod villages;
mod visibility;
#[cfg(test)]
mod wave10_integration_tests;
#[cfg(test)]
mod wave10_polish_tests;
#[cfg(test)]
mod wave6_content_tests;
#[cfg(test)]
mod wave8_seam_tests;
#[cfg(test)]
mod wave9_progression_tests;
#[cfg(test)]
mod wave9_seam_tests;
#[cfg(test)]
mod weapon_tests;
mod weapons;

pub use arcs::{ArcTalk, tile_center_px};
pub use bases::{BuildQuote, TOWN_NO_BUILD_MARGIN, TOWN_NO_BUILD_ROWS, payment_for};
pub use crypts::{CRUMBLE_MILLIS, PIT_FALL_SHARE, PLATE_DELAY_MILLIS, PLATE_REARM_MILLIS};
pub use discovery::{DiscoveredTiles, Discovery};
pub use enemies::{
    GuardianMove, JOLT_MILLIS, MAX_BATS, MAX_RAISED, NEST_GUARD_TILES, SKY_CITY_SAFE_TILES,
    SLAM_REACH, TURRET_MOUNTS, guardian_move, is_tomb_guardian,
};
// The Hollow World's creatures (Wave 9): how a coil's sun scale and a clinging mite show.
pub use enemies::hollow::{
    CLINGING_PHASE as LODE_MITE_CLINGING_PHASE, CRACKED_PHASE as SERPENT_CRACKED_PHASE,
    SCALE_PHASE as SERPENT_SCALE_PHASE,
};
pub use fluids::FluidSchedule;
pub use homes::{
    CELLAR_ROWS, CONTRACTOR_REACH_COLUMNS, ContractorOffer, HireQuote, MAX_CONTRACTORS_LISTED,
};
pub use people::{
    DEFAULT_PLAYER_NAME, DESPAWN_MARGIN_COLUMNS, PEOPLE_REFRESH_MILLIS, SPAWN_MARGIN_COLUMNS,
    TALK_REACH_TILES,
};
pub use player_sections::{PlayerInventory, PlayerRig, RigBuild, join_player, split_player};
pub use quests::{QuestAvailability, QuestOffer};
pub use replication::{
    ContractsSection, EconomySection, LiveEntities, MarkersSection, PlayerLoadout, PlayerTickState,
    ProgressionSection, SectionKey, SectionPayload, SessionSection, TimersSection,
    WorldEventsSection, hash_serialized,
};
pub use roster::SPAWN_SPACING;
pub use services::{
    BOMB_BUNDLES, MINING_ROCKETS_COST, MINING_ROCKETS_COUNT, MINING_ROCKETS_SHACK_LEVEL,
    command_services_rig,
};
pub use sky_temples::{GATE_REACH, LEVER_REACH, relic_loot};
pub use sky_weapons::{HOLD_RADIUS, HOLD_REACH, MIN_TETHER, REEL_SPEED, RIP_MILLIS};
pub use sonar::SonarMedium;
pub use timing::TickTiming;
pub use tombs::{
    PARTY_REACH, PICK_TIMEOUT_MILLIS, REVIVE_MILLIS, REVIVE_REACH, cache_value, guardian_hoard,
};

pub use commands::{
    DrillContact, DrillIntent, MIN_DRILL_SECONDS_PER_CHIP, authoritative_mine_target,
    base_drill_seconds_per_chip,
};
pub use import::{
    NEW_WORLD_HEIGHT_TILES, NEW_WORLD_SEED, NEW_WORLD_SPAWN, NEW_WORLD_WIDTH_TILES, WorldImport,
};
pub use movement::{
    AUTHORITATIVE_AIR_DRAG_HALF_LIFE_SECONDS, AUTHORITATIVE_AIR_DRAG_RATE_PER_SECOND,
    AUTHORITATIVE_BOULDER_LIFE_MILLIS, AUTHORITATIVE_BOULDER_SPAWN_CHANCE,
    AUTHORITATIVE_BOULDER_WARNING_MILLIS, AUTHORITATIVE_CAVE_IN_LIFE_MILLIS,
    AUTHORITATIVE_CAVE_IN_WARNING_MILLIS, AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS,
    AUTHORITATIVE_CRASH_DAMAGE_SCALE, AUTHORITATIVE_DESCENT_BRAKE_MULTIPLIER,
    AUTHORITATIVE_FUEL_BURN_PER_SECOND, AUTHORITATIVE_GRAVITY,
    AUTHORITATIVE_GROUND_DRAG_HALF_LIFE_SECONDS, AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND,
    AUTHORITATIVE_HORIZONTAL_ACCELERATION, AUTHORITATIVE_HORIZONTAL_SPEED,
    AUTHORITATIVE_MAX_CLIMB_SPEED, AUTHORITATIVE_MAX_FALL_SPEED,
    AUTHORITATIVE_MAX_HORIZONTAL_SPEED, AUTHORITATIVE_MIN_PLAYER_Y, AUTHORITATIVE_PLAYER_RADIUS,
    AUTHORITATIVE_RIG_GRAVITY, AUTHORITATIVE_SAFE_LANDING_SPEED,
    AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES, AUTHORITATIVE_THRUST_ACCELERATION,
    AUTHORITATIVE_VERTICAL_DRAG_RATE_PER_SECOND, AuthoritativeMovementStep, integrate_x,
};
// Movement feel (WP-FEEL): the eased drive model, per-tier top speeds and god-mode tuning.
pub use movement::{
    AIR_TOP_SPEED_FACTOR, AUTHORITATIVE_ABSOLUTE_MAX_SPEED, AUTHORITATIVE_COLLISION_STEP,
    DRIVE_STEP_SECONDS, DriveTuning, ENGINE_TOP_SPEED_TILES, GOD_MODE_SPEED_MULTIPLIER,
    GOD_MODE_THRUST_MULTIPLIER, collision_steps, engine_top_speed_tiles,
};
pub use no_clip::NO_CLIP_SPEED;
pub use pilot_arms::{PILOT_AUTO_AIM_RANGE, SWING_SHOW_SECONDS as PILOT_SWING_SHOW_SECONDS};
pub use pilot_gear::{RECALL_SPEED, SENTRY_COVER};
pub use weapons::MAX_HIT_RADIUS;
/// Tick deadline of a rig's next drill chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ActiveDrillTimer {
    pub next_chip_tick: SimulationTick,
    pub period_ticks: u64,
}

/// Number of most recent service transactions the world keeps.
///
/// The log is an audit trail and part of the replicated Economy section (resent in full whenever
/// it changes), so it is bounded instead of growing for the whole session.
pub const SERVICE_TRANSACTION_HISTORY: usize = 64;

/// Appends a service transaction, dropping the oldest entries beyond
/// [`SERVICE_TRANSACTION_HISTORY`].
fn record_service_transaction(
    log: &mut Vec<PlayerServiceTransaction>,
    transaction: PlayerServiceTransaction,
) {
    log.push(transaction);
    if log.len() > SERVICE_TRANSACTION_HISTORY {
        let excess = log.len() - SERVICE_TRANSACTION_HISTORY;
        log.drain(..excess);
    }
}

const fn canonical_minerals() -> [MineralKind; 10] {
    [
        MineralKind::Copper,
        MineralKind::Iron,
        MineralKind::Silver,
        MineralKind::Gold,
        MineralKind::Emerald,
        MineralKind::Ruby,
        MineralKind::Diamond,
        MineralKind::Platinum,
        MineralKind::Uranium,
        MineralKind::Mythril,
    ]
}

const fn canonical_market_factor(salt: u32, day: u32) -> u32 {
    let base = 85 + salt.wrapping_mul(37).wrapping_add(11) % 41;
    if day % 5 == 1 { base + 20 } else { base }
}

const fn canonical_mineral_market_factor(salt: u32, day: u32, mineral: MineralKind) -> u32 {
    let base = canonical_market_factor(salt.wrapping_add(mineral.value().wrapping_mul(13)), day);
    match (day % 5, mineral) {
        (1, MineralKind::Gold | MineralKind::Silver | MineralKind::Platinum) => base + 16,
        (0, MineralKind::Copper | MineralKind::Iron) => base + 8,
        (4, MineralKind::Ruby | MineralKind::Emerald | MineralKind::Diamond) => base + 10,
        _ => base,
    }
}

/// Pressure accumulated by a rig on a deep run.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DeepRunState {
    pub trip_ticks: u64,
    pub instability: f32,
}

/// Cargo dropped by a rescued rig, recoverable by returning to the drop site.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LostCargoSite {
    pub x: f32,
    pub y: f32,
    /// The spilled ore and artifacts.
    pub items: crate::item::ItemContainer,
}

impl crate::position::Reframe for LostCargoSite {
    fn reframe(&mut self, shift: crate::position::FrameShift) {
        shift.apply(&mut self.x, &mut self.y);
    }
}

impl LostCargoSite {
    #[must_use]
    pub fn count(&self) -> u32 {
        self.items.total()
    }
}

/// Shared progression, economy, contract, and expedition state owned by the world.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CanonicalProgressionState {
    pub contracts: ContractLog,
    pub deep_claim_status: DeepClaimStatus,
    pub escape_deadline: Option<SimulationTick>,
    pub deep_reward_milestones: BTreeMap<PlayerId, i32>,
    pub town_development: TownDevelopment,
    pub active_side_contracts: Vec<SideContract>,
    pub expedition_offers: Vec<Expedition>,
    pub active_expeditions: Vec<Expedition>,
    pub legendary_blueprints: BTreeSet<crate::LegendaryBlueprint>,
    pub rig_part_inventory: BTreeSet<RigPartKind>,
    pub town_event_day: u32,
    pub market_salt: u32,
    pub market_history: Vec<u32>,
    pub mineral_market_history: BTreeMap<MineralKind, Vec<u32>>,
    pub deepest_tile_reached: i32,
    pub total_earnings: u32,
    pub expeditions_completed: u32,
    pub next_day_tick: SimulationTick,
    /// Twin-relay vaults with one relay lit, and the tick their lock resets at.
    #[serde(default)]
    pub vault_relay_deadlines: BTreeMap<crate::worldgen::FeatureId, SimulationTick>,
    /// Quests accepted in crew mode, shared with every player.
    #[serde(default)]
    pub crew_quests: crate::quest::CrewQuests,
    /// Ground that recently came down and holds for a while (see
    /// [`crate::hazard::SettledGround`]).
    #[serde(default)]
    pub settled_ground: crate::hazard::SettledGround,
}

/// The canonical gameplay world.
///
/// [`Self::simulate_tick`] is the only gameplay transition. Everything else either reads the
/// world, replicates it (`replication`, `discovery`, `visibility`), manages the roster
/// (`roster`) or builds a world before play (import, save migration, test scenarios).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldState {
    timing: TickTiming,
    player_template: Player,
    simulation_tick: SimulationTick,
    won_game: bool,
    players: BTreeMap<PlayerId, Player>,
    equipped_rig_parts: BTreeMap<PlayerId, BTreeSet<RigPartKind>>,
    mechanic_levels: BTreeMap<PlayerId, u8>,
    world_event_severity: BTreeMap<WorldEventKind, u32>,
    world_event_days_remaining: BTreeMap<WorldEventKind, u32>,
    deep_run_states: BTreeMap<PlayerId, DeepRunState>,
    lost_cargo_sites: BTreeMap<PlayerId, LostCargoSite>,
    rescue_counts: BTreeMap<PlayerId, u32>,
    progression: CanonicalProgressionState,
    scan_markers: Vec<ScanMarker>,
    collapse_warnings: Vec<TilePosition>,
    /// Live entities: gas clouds, falling boulders, drops (see [`crate::entity`]).
    #[serde(default)]
    entities: EntityStore,
    terrain: Terrain,
    infrastructure: Vec<PlacedInfrastructure>,
    active_drills: BTreeMap<PlayerId, DrillState>,
    active_drill_timers: BTreeMap<PlayerId, ActiveDrillTimer>,
    scanner_cooldowns: BTreeMap<PlayerId, f32>,
    scanner_cooldown_deadlines: BTreeMap<PlayerId, SimulationTick>,
    /// Serial number of the next explosive charge (charges are [`EntityKind::Charge`]
    /// entities; see `world/bombs.rs`).
    ///
    /// [`EntityKind::Charge`]: crate::entity::EntityKind::Charge
    #[serde(default)]
    next_bomb_id: u64,
    rng: DeterministicRngState,
    discovered_tiles: BTreeMap<PlayerId, DiscoveredTiles>,
    service_transactions: Vec<PlayerServiceTransaction>,
    /// Test-mode toggles; `None` outside a test-mode world (see [`crate::test_mode`]).
    #[serde(default)]
    test_mode: Option<crate::test_mode::TestModeState>,
    /// Enemy difficulty, rig i-frames, spawn cooldowns and cleared lairs (see
    /// [`crate::enemy::EnemyDirector`]; the enemies themselves are entities).
    #[serde(default)]
    enemies: crate::enemy::EnemyDirector,
    /// Claims, base modules and fixtures (WP-BASE, see [`crate::base`]); solid building pieces
    /// live in the terrain.
    #[serde(default)]
    bases: crate::base::Bases,
    /// Crew identities and parked players (WP-PERSIST, see [`crate::crew`]). Saved with the
    /// world, never hashed or sent to clients.
    #[serde(default)]
    crew: crate::crew::CrewRoster,
    /// The world clock's day length, skipped nights and who is in bed (Part II F4, see
    /// [`crate::clock`]; the time of day itself is derived from the tick).
    #[serde(default)]
    clock: crate::clock::ClockState,
    /// What players did with people: every character's memory of every player, and each
    /// player's conversation, bought offers and perks (WP-PEOPLE, see `world/people.rs`). The
    /// people themselves are [`crate::entity::EntityKind::Npc`] entities.
    #[serde(default)]
    people: crate::people::PeopleLedger,
    /// Plots, deeds, contracted construction, finished homes and furniture (WP-HOME, see
    /// [`crate::home`]); the site crews are NPC entities.
    #[serde(default)]
    homes: crate::home::Homes,
    /// Tomb and crypt runs, site counters and seals, and permanent tomb unlocks (WP-TOMBS,
    /// see [`crate::tomb`]); the floors live in the terrain's run vault.
    #[serde(default)]
    tombs: crate::tomb::Tombs,
    /// Authored field zones (F9, see [`crate::field`]): anomaly rooms and dev zones blended
    /// over the stratum fields.
    #[serde(default)]
    fields: crate::field::FieldZones,
    /// Which fluid cells the next fluid step looks at (Part III F9, see `world/fluids.rs`; the
    /// fluid itself lives in the terrain). Saved with the world, never hashed or replicated.
    #[serde(default)]
    fluids: FluidSchedule,
    /// Depth logistics and the progression rules (Part III F10, see [`crate::logistics`]):
    /// lifts, outposts, beacons, rides, dives and dive charges.
    #[serde(default)]
    logistics: crate::logistics::Logistics,
    /// The authored Crust (Wave 8, see [`crate::crust`]): failing timber sets and slate
    /// shelves, the Sinkhole, the Deep Claim, the logs read.
    #[serde(default)]
    crust: crate::crust::CrustState,
    /// Descent II's live state (Wave 8, see [`crate::old_deep`]): dose meters, mutations,
    /// woken machine halls, skeleton digs, the Cathedral Lift and the Star Core fight.
    #[serde(default)]
    old_deep: crate::old_deep::OldDeep,
    /// The Living Dark's shared state (Wave 8, Descent III, see [`crate::living_dark`]): root
    /// axes, the Hive's politics, the Mycelial Mind's fight, the spore clouds' expiries.
    #[serde(default)]
    living_dark: crate::living_dark::LivingDark,
    /// The Living Dark's host bookkeeping (tiles waiting to regrow or heal, scorched tiles,
    /// the tides). Saved with the world, never hashed or replicated (like the fluids').
    #[serde(default)]
    living_schedule: crate::living_dark::LivingSchedule,
    /// The Fathers' expedition (Wave 8 story, see [`crate::expedition`]): logs read, sealed
    /// canisters, echoes heard and anomaly rooms entered, shared by the crew.
    #[serde(default)]
    expedition: crate::expedition::ExpeditionState,
    /// The Drowned and Frozen's shared state (Wave 9, Descent IV, see [`crate::drowned`]):
    /// sonar pings, ink clouds, the eel-dragon, the Leviathan's fight, the lore found.
    #[serde(default)]
    drowned: crate::drowned::DrownedState,
    /// The Hollow World's shared state (Wave 9, Descent V, see [`crate::hollow`]): the crew's
    /// breakthroughs, exposure to the inner sun, Gravity Anchors, rides and the Sun Serpent's
    /// fight.
    #[serde(default)]
    hollow: crate::hollow::Hollow,
    /// The distortion strata's shared state (Wave 9, Descent VI part A, see
    /// [`crate::distortion`]): gazers' holds, rot pulses, the mirror halls' twin cooldowns and
    /// the landmarks found.
    #[serde(default)]
    distortion: crate::distortion::Distortion,
    /// The far Strange Layers' shared state (Wave 9 strange-b, Descent VI, see
    /// [`crate::far_strange`]): lamp filters, sonar pings, amber, landmarks found and The
    /// Glitch's fight.
    #[serde(default)]
    far_strange: crate::far_strange::FarStrange,
    /// The far Strange Layers' host bookkeeping (open keyed walls, rewind newts' histories,
    /// pings and quartz rings). Saved with the world, never hashed or replicated.
    #[serde(default)]
    far_schedule: crate::far_strange::FarSchedule,
    /// The builders' buried city's shared state (Wave 10 ancients-a, Descent VII strata 1-3,
    /// see [`crate::ancient_city`]): rides, alarms, archivists, star charts, vaults and
    /// landmarks found.
    #[serde(default)]
    ancient_city: crate::ancient_city::AncientCity,
    /// The buried city's host bookkeeping (crusher hits, crawlers' loot, launches). Saved
    /// with the world, never hashed or replicated.
    #[serde(default)]
    city_schedule: crate::ancient_city::CitySchedule,
    /// Descent VII part B's shared state (Wave 10 ancients-b, see [`crate::garrison`]): garrison
    /// forts and their waves, the build kit's defences, the Seal Gate, the Gatekeeper
    /// Colossus's fight, falling stones and landmarks found.
    #[serde(default)]
    garrison: crate::garrison::Garrison,
    /// Descent VII part B's host bookkeeping (whom the Seal Gate refused). Saved with the
    /// world, never hashed or replicated.
    #[serde(default)]
    garrison_schedule: crate::garrison::GarrisonSchedule,
    /// Descent VIII's shared state (Wave 10, see [`crate::heart`]): floating rigs, wells, the
    /// landmarks found, the Heart's fight, `space_unlocked`, the Star Drives and the ending's
    /// start.
    #[serde(default)]
    heart: crate::heart::Heart,
    /// Every player's place and the ground of every store other than the home world's (Part
    /// III F11, see `world/places.rs`): space bodies' terrain, entities and fluids, kept while
    /// nobody is there. Empty (and not hashed) until someone leaves the home store.
    #[serde(default)]
    places: places::Places,
    /// The running pass of the tick (never saved, hashed or replicated).
    #[serde(skip)]
    place_pass: places::PlacePass,
    /// Space flight's shared state (Wave 11, see [`crate::flight`]): waypoints, the crew's
    /// waypoint, systems found and scanner pings. Each rig's own flight is per tick
    /// ([`crate::Player::flight`]).
    #[serde(default)]
    flight: crate::flight::FlightShared,
    /// What the crew did to space's things (Wave 12 systems, see
    /// [`crate::flight::systems`]): asteroids mined, comet dust gathered, wormholes flown.
    #[serde(default)]
    space_systems: crate::flight::systems::SystemsShared,
}

impl WorldState {
    /// Per-tick constants derived from the session tick rate.
    #[must_use]
    pub const fn timing(&self) -> &TickTiming {
        &self.timing
    }

    /// The session tick rate in Hz.
    #[must_use]
    pub const fn tick_rate_hz(&self) -> u32 {
        self.timing.rate.hz()
    }

    /// Template for players added with [`Self::add_player`].
    #[must_use]
    pub const fn player_template(&self) -> &Player {
        &self.player_template
    }

    /// Replaces the template for players added with [`Self::add_player`].
    /// Load-time fixup for saves from the old five-tier upgrade curves: lifts every rig's (and
    /// the player template's) stored fuel and cargo capacity onto the current curve for its
    /// fitted tiers (see [`Player::catch_up_capacity_curves`]). A no-op for current saves.
    pub fn catch_up_capacity_curves(&mut self) {
        self.player_template.catch_up_capacity_curves();
        for player in self.players.values_mut() {
            player.catch_up_capacity_curves();
        }
    }

    pub fn set_player_template(&mut self, template: Player) {
        self.player_template = template;
    }

    #[must_use]
    pub const fn progression(&self) -> &CanonicalProgressionState {
        &self.progression
    }

    #[must_use]
    pub const fn lost_cargo_sites(&self) -> &BTreeMap<PlayerId, LostCargoSite> {
        &self.lost_cargo_sites
    }

    #[must_use]
    pub const fn players(&self) -> &BTreeMap<PlayerId, Player> {
        &self.players
    }

    #[must_use]
    pub const fn active_drills(&self) -> &BTreeMap<PlayerId, DrillState> {
        &self.active_drills
    }

    #[must_use]
    pub const fn simulation_tick(&self) -> SimulationTick {
        self.simulation_tick
    }

    /// Overwrites the tick counter (replicas following the authority, save migration).
    pub const fn set_simulation_tick(&mut self, tick: SimulationTick) {
        self.simulation_tick = tick;
    }

    #[must_use]
    pub const fn infrastructure_count(&self) -> usize {
        self.infrastructure.len()
    }

    #[must_use]
    pub const fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    /// Mutable terrain for building a scenario world before play starts (test harnesses);
    /// gameplay only changes terrain inside [`Self::simulate_tick`].
    pub const fn terrain_mut(&mut self) -> &mut Terrain {
        &mut self.terrain
    }

    /// Claims, base modules and fixtures (see [`crate::base`]).
    #[must_use]
    pub const fn bases(&self) -> &crate::base::Bases {
        &self.bases
    }

    #[must_use]
    pub fn infrastructure(&self) -> &[PlacedInfrastructure] {
        &self.infrastructure
    }

    #[must_use]
    pub fn service_transactions(&self) -> &[PlayerServiceTransaction] {
        &self.service_transactions
    }

    #[must_use]
    pub fn active_drill(&self, player_id: PlayerId) -> Option<&DrillState> {
        self.active_drills.get(&player_id)
    }

    pub(crate) fn set_active_drill(&mut self, player_id: PlayerId, drill: Option<DrillState>) {
        if let Some(drill) = drill {
            // Lap-aware on a round body: the same tile round the seam is the same work.
            let continuing = self.active_drills.get(&player_id).is_some_and(|current| {
                self.terrain.same_tile(current.target, drill.target)
                    && current.direction == drill.direction
            }) && self.active_drill_timers.contains_key(&player_id);
            if continuing {
                // Keep the tick deadline; only presentation fields and the period change.
                self.active_drills.insert(player_id, drill);
                return;
            }
            let period_ticks = self.timing.rate.ticks_for_seconds(drill.seconds_per_chip);
            let period_u16 = u16::try_from(period_ticks).unwrap_or(u16::MAX).max(1);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped drill presentation progress intentionally maps onto a bounded tick period"
            )]
            let elapsed_ticks =
                u64::from((drill.progress.clamp(0.0, 1.0) * f32::from(period_u16)).floor() as u16);
            self.active_drill_timers.insert(
                player_id,
                ActiveDrillTimer {
                    next_chip_tick: SimulationTick::new(
                        self.simulation_tick
                            .get()
                            .saturating_add(period_ticks.saturating_sub(elapsed_ticks).max(1)),
                    ),
                    period_ticks,
                },
            );
            self.active_drills.insert(player_id, drill);
        } else {
            self.active_drills.remove(&player_id);
            self.active_drill_timers.remove(&player_id);
        }
    }

    #[must_use]
    pub fn scanner_cooldown_seconds(&self, player_id: PlayerId) -> Option<f32> {
        self.scanner_cooldowns.get(&player_id).copied()
    }

    #[cfg(test)]
    pub(crate) fn set_scanner_cooldown_seconds(&mut self, player_id: PlayerId, seconds: f32) {
        let seconds = seconds.max(0.0);
        self.scanner_cooldowns.insert(player_id, seconds);
        self.scanner_cooldown_deadlines.insert(
            player_id,
            SimulationTick::new(
                self.simulation_tick
                    .get()
                    .saturating_add(self.timing.rate.ticks_for_seconds(seconds)),
            ),
        );
    }

    #[must_use]
    pub fn discovered_tile_count(&self, player_id: PlayerId) -> usize {
        self.discovered_tiles
            .get(&player_id)
            .map_or(0, DiscoveredTiles::len)
    }

    /// Current sale value of one unit of `mineral` (market factor, depot level and world events).
    #[must_use]
    pub fn mineral_market_value(&self, mineral: MineralKind) -> u32 {
        self.canonical_mineral_market_value(mineral)
    }

    /// Today's market factor (percent) for `mineral`.
    #[must_use]
    pub const fn mineral_market_factor(&self, mineral: MineralKind) -> u32 {
        canonical_mineral_market_factor(
            self.progression.market_salt,
            self.progression.town_event_day,
            mineral,
        )
    }

    /// The previous day's market factor (percent) for `mineral`, if recorded.
    #[must_use]
    pub fn previous_mineral_market_factor(&self, mineral: MineralKind) -> Option<u32> {
        self.progression
            .mineral_market_history
            .get(&mineral)
            .and_then(|history| history.iter().rev().nth(1).copied())
    }

    /// Active world events as `(kind, severity, days remaining)`, in deterministic order.
    #[must_use]
    pub fn active_world_events(&self) -> Vec<(WorldEventKind, u32, u32)> {
        self.world_event_severity
            .iter()
            .map(|(kind, severity)| {
                (
                    *kind,
                    *severity,
                    self.world_event_days_remaining
                        .get(kind)
                        .copied()
                        .unwrap_or(0),
                )
            })
            .collect()
    }

    /// Scanner markers placed by players.
    #[must_use]
    pub fn scan_markers(&self) -> &[ScanMarker] {
        &self.scan_markers
    }

    /// Tiles flagged as collapse risks.
    #[must_use]
    pub fn collapse_warnings(&self) -> &[TilePosition] {
        &self.collapse_warnings
    }

    /// A player's deep-run pressure state, if they are on a run.
    #[must_use]
    pub fn deep_run_state(&self, player_id: PlayerId) -> Option<&DeepRunState> {
        self.deep_run_states.get(&player_id)
    }

    /// How many rescues a player has called.
    #[must_use]
    pub fn rescue_count(&self, player_id: PlayerId) -> u32 {
        self.rescue_counts.get(&player_id).copied().unwrap_or(0)
    }

    #[must_use]
    pub const fn won_game(&self) -> bool {
        self.won_game
    }

    #[must_use]
    pub fn player(&self, player_id: PlayerId) -> Option<&Player> {
        self.players.get(&player_id)
    }

    pub fn player_ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.players.keys().copied()
    }

    /// Mutable rig for building a scenario world or migrating a save; gameplay only changes
    /// rigs inside [`Self::simulate_tick`].
    pub fn player_mut(&mut self, player_id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(&player_id)
    }

    #[cfg(test)]
    pub(crate) fn set_player_movement_modifiers(
        &mut self,
        player_id: PlayerId,
        mechanic_level: u8,
        parts: impl IntoIterator<Item = RigPartKind>,
    ) {
        self.mechanic_levels.insert(player_id, mechanic_level);
        self.equipped_rig_parts
            .insert(player_id, parts.into_iter().collect());
    }

    #[must_use]
    pub fn player_survival_snapshot(&self, player_id: PlayerId) -> Option<PlayerSurvivalSnapshot> {
        self.player(player_id).map(|player| PlayerSurvivalSnapshot {
            player_id,
            fuel: player.fuel,
            hull: player.hull,
            disabled: player.hull <= 0.0,
            stranded: player.fuel <= 0.0,
        })
    }

    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Replaces terrain and world objects wholesale (migrating a v2 save).
    pub fn restore_static_world_state(
        &mut self,
        terrain: Terrain,
        hazards: Vec<HazardCloud>,
        bombs: Vec<PlacedBomb>,
        infrastructure: Vec<PlacedInfrastructure>,
        active_drills: BTreeMap<PlayerId, DrillState>,
        scanner_cooldowns: BTreeMap<PlayerId, f32>,
    ) {
        self.terrain = terrain;
        self.entities = EntityStore::new();
        self.restore_legacy_live_entities(hazards, Vec::new(), &[], &[]);
        for bomb in bombs {
            self.insert_charge(bomb);
        }
        self.infrastructure = infrastructure;
        self.active_drills = active_drills;
        self.scanner_cooldowns = scanner_cooldowns;
    }
}
