#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Canonical, deterministic Fatherload gameplay simulation.
//!
//! [`WorldState::simulate_tick`] advances the world exactly one fixed tick from a set of
//! [`TickInputs`]. The crate performs no IO, owns no clocks, and has no rendering, async runtime, or
//! networking dependencies, so it can run identically on a host, inside client prediction, in
//! replays, and in tests.

pub mod attitude;
pub mod balance;
pub mod base;
pub mod bodylife;
pub mod cave;
pub mod climb;
pub mod clock;
pub mod combat;
pub mod command;
pub mod contract;
pub mod cosmetics;

pub mod alien;
pub mod ancient_city;
pub mod crew;
pub mod crust;
pub mod depth_progression;
pub mod descent;
pub mod distortion;
pub mod drowned;
pub mod economy;
pub mod enemy;
pub mod entity;
pub mod environment;
pub mod event;
pub mod expedition;
pub mod explosive;
pub mod far_strange;
pub mod field;
pub mod flight;
pub mod fluid;
pub mod frontier;
pub mod garrison;
pub mod hash;
pub mod hazard;
pub mod heart;
pub mod hollow;
pub mod home;
pub mod ids;
pub mod infrastructure;
pub mod input;
pub mod item;
pub mod living_dark;
pub mod logistics;
pub mod mechanism;
pub mod milestone;
pub mod moon;
pub mod night;
pub mod old_deep;
pub mod pair_list;
pub mod people;
pub mod physics;
pub mod pilot;
pub mod place;
pub mod planet;
pub mod player;
pub mod poi;
pub mod polar;
pub mod position;
pub mod progression;
pub mod protection;
pub mod quantize;
pub mod quest;
pub mod rig;
pub mod rng;
pub mod service;
pub mod sky;
pub mod skycraft;
pub mod space;
pub mod star_chart;
pub mod station;
pub mod summary;
pub mod surface;
pub mod terrain;
pub mod test_mode;
pub mod tick;
pub mod tomb;
pub mod upgrade_preview;
pub mod village;
pub mod weapon;
pub mod workshop;
pub mod world;
pub mod worldgen;

pub use command::{PlayerCommand, TickAction};
pub use cosmetics::{CosmeticItem, CosmeticUnlocks, RigCosmetics};
pub use crew::{CrewMember, CrewRoster, ParkedPlayer, ProfileId};
pub use enemy::{
    Damage, DamageKind, DamageOutcome, DamageSource, EnemyDifficulty, EnemyKind, ShotKind,
};
pub use event::{ActionOutcome, GameplayEvent, GameplayEventId, GameplayEventKind, TickOutput};
pub use explosive::{BlastDirection, ExplosiveKind, ThrowAim};
pub use hash::SimulationStateHash;
pub use hazard::{FallingBoulder, HazardCloud, PlacedBomb, ScanMarker};
pub use ids::{InputSequence, PlayerId, SimulationTick};
pub use infrastructure::{
    InfrastructureKind, PlacedInfrastructure, default_infrastructure_durability,
};
pub use input::{ControlState, PlayerTickInput, TickInputs, WeaponControl};
pub use player::Player;
pub use poi::{PoiJournal, PoiKind, PoiKnowledge, PoiReward, PoiTile, PoiVariant};
pub use progression::{
    ActiveWorldEvent, Expedition, ExpeditionObjectiveKind, LegendaryBlueprint, SideContract,
    SideContractKind, WorldEventKind,
};
pub use quest::{
    ActiveQuest, BlueprintId, CrewQuests, QuestDef, QuestFailure, QuestGiver, QuestId, QuestLog,
};
pub use rig::{DrillDirection, DrillState, RigPartKind, RigSlot};
pub use rng::DeterministicRngState;
pub use service::{
    PlayerServiceTransaction, PlayerTransactionKind, RecipeKind, SessionServiceRequest,
};
pub use summary::{PlayerScopedCommandOutcome, PlayerSurvivalSnapshot};
pub use terrain::{TILE_SIZE, TileChange};
pub use test_mode::{TestModeAction, TestModeState};
pub use tick::{
    DEFAULT_TICK_RATE_HZ, FIXED_DELTA_SECONDS, SIMULATION_HZ, SimulationParams, TickRate,
};
pub use weapon::{AmmoKind, MountedWeapon, RigWeaponState, WeaponKind, WeaponRack};
pub use world::{
    ActiveDrillTimer, AuthoritativeMovementStep, CanonicalProgressionState, DeepRunState,
    DiscoveredTiles, Discovery, LiveEntities, LostCargoSite, PlayerLoadout, PlayerTickState,
    SectionKey, SectionPayload, TickTiming, WorldImport, WorldState,
};
