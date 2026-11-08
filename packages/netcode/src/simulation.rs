//! The simulation contract netcode is generic over.
//!
//! The real `WorldState` (plan §4) implements [`NetSimulation`] in
//! [`crate::world`]; sessions only ever see the trait. A small toy
//! implementation lives in [`crate::toy`] for unit tests.
//!
//! # Replication model
//!
//! The trait splits world state into the three replication section kinds from
//! plan §4, which is what makes "forgot to replicate X" structurally
//! impossible as long as every field belongs to one of them:
//!
//! | Kind | Trait surface | Netcode behaviour |
//! |---|---|---|
//! | Per-tick | [`NetSimulation::player_state`], [`NetSimulation::entities`] | Full copy in every frame, relevance-filtered (§15) |
//! | Op log | [`TickOutput::ops`], [`NetSimulation::apply_op`] | Exact ops since the client's previous frame, in order |
//! | On-change | [`NetSimulation::section_keys`] / [`NetSimulation::section_hash`] | Whole section resent when its hash changes (per client baseline) |
//!
//! The player roster (which players exist) is replicated by netcode itself
//! whenever it changes, using [`NetSimulation::add_player`] /
//! [`NetSimulation::remove_player`] on the client.
//!
//! The initial sketch's `encode_replication` / `apply_replication` pair is
//! deliberately split into these fine-grained hooks so that netcode owns the
//! per-client baselines (section hashes, pending ops, roster) and the
//! simulation never needs to know about clients.

use std::collections::BTreeMap;
use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, PlayerId, Tick};

/// Presentation pose of a player, in world units.
///
/// `mode` is a discrete presentation state (for example "which tile the drill
/// is targeting"). When a reconciliation changes `mode`, the client snaps
/// instead of smoothing (plan §8 step 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
    /// Discrete presentation state; a change forces a snap.
    pub mode: u32,
}

impl Pose {
    /// Creates a pose.
    #[must_use]
    pub const fn new(x: f32, y: f32, mode: u32) -> Self {
        Self { x, y, mode }
    }

    /// Euclidean distance between the positions (ignores `mode`).
    #[must_use]
    pub fn distance(&self, other: &Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    /// Linear interpolation; `mode` switches at the midpoint.
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            x: (other.x - self.x).mul_add(t, self.x),
            y: (other.y - self.y).mul_add(t, self.y),
            mode: if t < 0.5 { self.mode } else { other.mode },
        }
    }

    /// Returns the pose translated by `(dx, dy)`.
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            mode: self.mode,
        }
    }
}

/// A one-shot action with its stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope<A> {
    /// Unique per client session; the host applies each id exactly once.
    pub id: ActionId,
    /// The gameplay action.
    pub action: A,
}

/// One player's input for one tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTickInput<C, A> {
    /// Held, quantized controls.
    pub controls: C,
    /// One-shot actions applied this tick.
    pub actions: Vec<ActionEnvelope<A>>,
}

/// All inputs for one simulated tick.
///
/// `tick` is the tick being produced: after `simulate_tick` the world is at
/// `tick`. Players absent from `players` use `C::default()` and no actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickInputs<C, A> {
    /// The tick being simulated (the world's tick after the call).
    pub tick: Tick,
    /// Per-player input, deterministic order.
    pub players: BTreeMap<PlayerId, PlayerTickInput<C, A>>,
}

impl<C, A> TickInputs<C, A> {
    /// Empty inputs for `tick` (everyone neutral).
    #[must_use]
    pub const fn empty(tick: Tick) -> Self {
        Self {
            tick,
            players: BTreeMap::new(),
        }
    }
}

/// A gameplay event emitted by one tick.
///
/// `player` + `kind` are used for prediction deduplication: the client plays
/// local-player events from prediction immediately and suppresses the
/// authoritative copy keyed by `(player, tick, kind, ordinal)` (and the input
/// seq, when the host consumed one that tick).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickEvent<E> {
    /// The player the event belongs to, if any.
    pub player: Option<PlayerId>,
    /// A stable small discriminant of the event type.
    pub kind: u32,
    /// The event itself.
    pub payload: E,
}

/// Everything one tick produced besides the state change itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickOutput<E, O> {
    /// Gameplay events, in deterministic order.
    pub events: Vec<TickEvent<E>>,
    /// Terrain (op-log section) changes made this tick, in order.
    pub ops: Vec<O>,
}

impl<E, O> Default for TickOutput<E, O> {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            ops: Vec::new(),
        }
    }
}

/// Interest set for one client (plan §15).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Relevance {
    /// The client's own player, always relevant.
    pub viewer: Option<PlayerId>,
    /// Centre of interest (normally the viewer's pose).
    pub center: Option<(f32, f32)>,
    /// Interest radius in world units. `None` is infinite.
    pub radius: Option<f32>,
}

impl Relevance {
    /// Everything is relevant.
    #[must_use]
    pub const fn everything() -> Self {
        Self {
            viewer: None,
            center: None,
            radius: None,
        }
    }

    /// Whether this relevance set covers the whole world.
    #[must_use]
    pub const fn is_infinite(&self) -> bool {
        self.radius.is_none()
    }

    /// Whether a world position is inside the interest radius.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match (self.radius, self.center) {
            (None, _) | (Some(_), None) => true,
            (Some(radius), Some((cx, cy))) => (x - cx).hypot(y - cy) <= radius,
        }
    }
}

/// A gameplay simulation that can be hosted, replicated and predicted.
///
/// Requirements:
///
/// * [`NetSimulation::simulate_tick`] runs exactly one fixed tick, with no IO
///   or wall clock, and must be deterministic for identical state and inputs.
///   After the call `self.tick() == inputs.tick`.
/// * Every piece of state must be covered by exactly one replication kind
///   (see the module docs), otherwise client `authority` hashes will diverge
///   from the host and the harness will catch it.
/// * [`NetSimulation::state_hash`] must hash *all* replicated state.
pub trait NetSimulation: Clone + Debug {
    /// Held, quantized controls. `Default` is neutral input.
    type Controls: Clone + Debug + PartialEq + Default + Serialize + DeserializeOwned;
    /// One-shot actions (bomb, buy, sell, ...), carried with an [`ActionId`].
    type Action: Clone + Debug + Serialize + DeserializeOwned;
    /// Gameplay events (audio/vfx cues, notifications).
    type Event: Clone + Debug + Serialize + DeserializeOwned;
    /// Per-tick replicated state of one player.
    type PlayerState: Clone + Debug + Serialize + DeserializeOwned;
    /// Per-tick replicated live entities (bombs, falling rocks, ...).
    type Entities: Clone + Debug + Serialize + DeserializeOwned;
    /// One op-log change (normally a terrain tile change).
    type Op: Clone + Debug + Serialize + DeserializeOwned;
    /// Key of an on-change section (e.g. `Market`, `Wallet(PlayerId)`).
    type SectionKey: Clone + Debug + Ord + Serialize + DeserializeOwned;
    /// Payload of an on-change section.
    type Section: Clone + Debug + Serialize + DeserializeOwned;
    /// Full-world snapshot used for `Welcome` and `Keyframe`.
    type Keyframe: Clone + Debug + Serialize + DeserializeOwned;

    /// Current tick (number of ticks simulated).
    fn tick(&self) -> Tick;
    /// The tick rate the world's per-tick constants were derived for, if it has any. The host
    /// refuses a world whose rate differs from its [`crate::SessionParams`].
    fn tick_rate_hz(&self) -> Option<u32> {
        None
    }
    /// Overwrites the current tick (used when applying server frames).
    fn set_tick(&mut self, tick: Tick);

    /// All players currently in the world, in ascending order.
    fn player_ids(&self) -> Vec<PlayerId>;
    /// Adds a player with default spawn state. Idempotent.
    fn add_player(&mut self, id: PlayerId);
    /// Removes a player and all of its per-player state. Idempotent.
    fn remove_player(&mut self, id: PlayerId);

    /// Runs exactly one fixed tick.
    fn simulate_tick(
        &mut self,
        inputs: &TickInputs<Self::Controls, Self::Action>,
    ) -> TickOutput<Self::Event, Self::Op>;

    /// Whether the client should predict this action locally. Economy
    /// transactions typically return `false` and wait for the server result
    /// (plan §8, "Menus and transactions").
    fn predicts_action(_action: &Self::Action) -> bool {
        true
    }

    /// Presentation pose of a player (interpolation, smoothing, camera,
    /// relevance).
    fn player_pose(&self, id: PlayerId) -> Option<Pose>;
    /// Per-tick replicated state of a player.
    fn player_state(&self, id: PlayerId) -> Option<Self::PlayerState>;
    /// Overwrites a player's per-tick state (adds the player if needed).
    fn apply_player_state(&mut self, id: PlayerId, state: Self::PlayerState);

    /// Per-tick entities relevant to `relevance`.
    fn entities(&self, relevance: &Relevance) -> Self::Entities;
    /// Replaces the per-tick entities with a received copy.
    fn apply_entities(&mut self, entities: Self::Entities);

    /// Whether an op-log entry should be sent to a client with `relevance`.
    /// Defaults to always (filtering requires a far-chunk refresh mechanism).
    fn op_relevant(&self, _op: &Self::Op, _relevance: &Relevance) -> bool {
        true
    }
    /// Applies one op-log entry.
    fn apply_op(&mut self, op: &Self::Op);

    /// Keys of all on-change sections that currently exist.
    fn section_keys(&self) -> Vec<Self::SectionKey>;
    /// Stable hash of one section (netcode diffs these per client).
    fn section_hash(&self, key: &Self::SectionKey) -> Option<u64>;
    /// Copy of one section for sending.
    fn section(&self, key: &Self::SectionKey) -> Option<Self::Section>;
    /// Replaces one section with a received copy.
    fn apply_section(&mut self, key: &Self::SectionKey, section: Self::Section);
    /// Removes a section that no longer exists on the host.
    fn remove_section(&mut self, key: &Self::SectionKey);

    /// Full-world snapshot.
    fn keyframe(&self) -> Self::Keyframe;
    /// Rebuilds a world from a snapshot.
    fn from_keyframe(keyframe: Self::Keyframe) -> Self;

    /// Hash over all replicated state (see [`crate::StableHasher`]).
    fn state_hash(&self) -> u64;
}
