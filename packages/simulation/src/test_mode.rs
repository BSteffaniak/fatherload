//! Test mode ("god mode"): a world setting for exploring the layers quickly.
//!
//! A test-mode world starts every rig with every upgrade maxed (the drill mount included), every
//! rig part in the shared parts locker and a large credit balance. It is tuned to feel like the
//! game is beaten: rigs drive and climb well above the top engine tier (see
//! [`crate::world::DriveTuning`]) and drill anything but bedrock instantly. While it is on,
//! any player may toggle an invulnerable hull, infinite fuel and no-clip flight for the whole
//! crew, and warp their own rig to the top of any band. Depth records and deep-reward milestones
//! do not count in a test-mode world.
//!
//! The setting is chosen when the world is created (the host's choice online) and is part of the
//! world: it replicates as its own shared section and is saved with the world.

use serde::{Deserialize, Serialize};

use crate::worldgen::Band;

/// Credits each rig starts a test-mode world with.
pub const TEST_MODE_CREDITS: u32 = 1_000_000;

/// The test-mode toggles of a world (present only in a test-mode world).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TestModeState {
    /// Hulls never lose integrity (refilled every tick).
    pub invulnerable: bool,
    /// Fuel tanks never run dry (refilled every tick).
    pub infinite_fuel: bool,
    /// Rigs fly freely through solid rock: no gravity, no collision, no drilling (thrust flies
    /// up, drill flies down). Turning it off frees any rig left inside rock.
    #[serde(default)]
    pub no_clip: bool,
}

/// A test-mode action submitted as [`crate::PlayerCommand::TestMode`]. Ignored outside a
/// test-mode world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TestModeAction {
    /// Turn the crew's invulnerable hulls on or off.
    SetInvulnerable(bool),
    /// Turn the crew's infinite fuel on or off.
    SetInfiniteFuel(bool),
    /// Move the submitting rig into a freshly carved, safe pocket at the top of `Band`.
    WarpToBand(Band),
    /// Turn the crew's no-clip flight on or off.
    SetNoClip(bool),
    /// Place a field zone of the preset around the submitting rig or pilot (F9 dev zones).
    PlaceFieldZone(crate::field::FieldPreset),
    /// Remove every field zone.
    ClearFieldZones,
    /// Carve a row of test pools, one per fluid kind, beside the submitting rig (Part III F9:
    /// water, acid, brine and liquid metal to swim, dive and react in).
    PourTestPools,
    /// Move the submitting rig to another place (F11): over a space body's arrival point,
    /// into free flight, onto the Moon or back to the spawn point at home.
    WarpToPlace(crate::place::Place),
    /// Fit the submitting rig with the Star Drive and open space (Wave 11's debug grant; on
    /// Deep Descent worlds only).
    GrantStarDrive,
    /// Call a `Faction`'s encounter on the submitting rig in space (Wave 12 hostiles).
    SpawnHostiles(crate::hostiles::Faction),
    /// Call a `Faction`'s boss on the submitting rig in space (Wave 12 hostiles).
    SpawnBoss(crate::hostiles::Faction),
    /// Land a `Faction`'s raid by the submitting rig on a body's ground (Wave 12 hostiles).
    SpawnRaid(crate::hostiles::Faction),
}
