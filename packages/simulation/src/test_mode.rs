//! Test mode: a world setting for exploring the layers quickly.
//!
//! A test-mode world starts every rig with every upgrade maxed (the drill mount included), every
//! rig part in the shared parts locker and a large credit balance. While it is on, any player may
//! toggle an invulnerable hull and infinite fuel for the whole crew, and warp their own rig to
//! the top of any band. Depth records and deep-reward milestones do not count in a test-mode
//! world.
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
}
