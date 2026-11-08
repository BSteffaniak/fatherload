#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Canonical, deterministic Drillgame gameplay simulation.
//!
//! [`WorldState::simulate_tick`] advances the world exactly one fixed tick from a set of
//! [`TickInputs`]. The crate performs no IO, owns no clocks, and has no rendering, async runtime, or
//! networking dependencies, so it can run identically on a host, inside client prediction, in
//! replays, and in tests.

pub mod command;
pub mod contract;
pub mod economy;
pub mod event;
pub mod hash;
pub mod hazard;
pub mod ids;
pub mod infrastructure;
pub mod input;
pub mod player;
pub mod progression;
pub mod quantize;
pub mod rig;
pub mod rng;
pub mod service;
pub mod summary;
pub mod surface;
pub mod terrain;
pub mod tick;
pub mod world;

pub use command::{PlayerCommand, TickAction};
pub use event::{ActionOutcome, GameplayEvent, GameplayEventId, GameplayEventKind, TickOutput};
pub use hash::SimulationStateHash;
pub use hazard::{FallingBoulder, HazardCloud, PlacedBomb, ScanMarker};
pub use ids::{InputSequence, PlayerId, SimulationTick};
pub use infrastructure::{
    InfrastructureKind, PlacedInfrastructure, default_infrastructure_durability,
};
pub use input::{ControlState, PlayerTickInput, TickInputs};
pub use player::Player;
pub use progression::{
    ActiveWorldEvent, Expedition, ExpeditionObjectiveKind, LegendaryBlueprint, SideContract,
    SideContractKind, WorldEventKind,
};
pub use rig::{DrillDirection, DrillState, RigPartKind, RigSlot};
pub use rng::DeterministicRngState;
pub use service::{
    PlayerServiceTransaction, PlayerTransactionKind, RecipeKind, SessionServiceRequest,
};
pub use summary::{PlayerScopedCommandOutcome, PlayerSurvivalSnapshot};
pub use terrain::{TILE_SIZE, TileChange};
pub use tick::{
    DEFAULT_TICK_RATE_HZ, FIXED_DELTA_SECONDS, SIMULATION_HZ, SimulationParams, TickRate,
};
pub use world::{
    ActiveDrillTimer, AuthoritativeMovementStep, CanonicalProgressionState, DeepRunState,
    LiveEntities, LostCargoSite, PlayerLoadout, PlayerTickState, SectionKey, SectionPayload,
    TickTiming, WorldImport, WorldState,
};
