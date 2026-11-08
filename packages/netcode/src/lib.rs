#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Sans-IO multiplayer netcode for Drillgame.
//!
//! This crate implements the server-authoritative replication model from
//! `multiplayer-architecture-plan.md` (sections 5-8, 11.2, 15 and 16):
//!
//! * [`HostSession`] owns the authoritative simulation, advances it at a fixed
//!   tick rate from its own accumulator (never on packet arrival), buffers
//!   client inputs by target tick and emits relevance-filtered [`ServerFrame`]s
//!   at a per-client snapshot rate.
//! * [`ClientSession`] keeps the last confirmed server state (`authority`), a
//!   predicted copy (`authority` + unacknowledged local inputs), runs ahead of
//!   the server using [`ClientClock`] and interpolates remote players through a
//!   [`SnapshotBuffer`].
//! * [`protocol`] defines the wire messages and the versioned postcard codec.
//! * [`LinkSim`] is a deterministic, seeded lossy link used by the end-to-end
//!   harness to prove the whole stack under latency, jitter, loss, duplication
//!   and reordering.
//!
//! The crate is generic over the gameplay simulation through the
//! [`NetSimulation`] trait, so it never depends on the real game crate.
//!
//! No sockets, threads, async runtimes or wall clock are used here: callers
//! pass elapsed time in as a [`std::time::Duration`].

mod hashing;
mod ids;
mod ledger;
mod params;
mod simulation;



pub use hashing::{StableHasher, stable_hash};
pub use ids::{ActionId, ClientId, InputSeq, PlayerId, ReconnectToken, SeatId, SessionEpoch, Tick};
pub use ledger::ActionLedger;
pub use params::{InterpDelay, NetTuning, SessionParams};
pub use simulation::{
    ActionEnvelope, NetSimulation, PlayerTickInput, Pose, Relevance, TickEvent, TickInputs,
    TickOutput,
};
