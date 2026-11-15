//! Gameplay commands submitted by players (intent, not raw input).

use serde::{Deserialize, Serialize};

use crate::{
    explosive::{ExplosiveKind, ThrowAim},
    ids::InputSequence,
    item::BlockKind,
    service::SessionServiceRequest,
    test_mode::TestModeAction,
};

/// One-shot gameplay commands submitted by a player.
///
/// This represents gameplay intent rather than keyboard/gamepad state, so local seats, online
/// clients, replays and bots all submit the same commands. Held movement is not a command: it
/// travels in [`crate::ControlState`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlayerCommand {
    UseScanner,
    PlaceBomb,
    /// Deploys one carried explosive of `kind` along `aim` (dropped, thrown or placed, see
    /// [`ExplosiveKind::deployment`]). `kind: Bomb` drops a plain bomb like [`Self::PlaceBomb`].
    DeployExplosive {
        kind: ExplosiveKind,
        aim: ThrowAim,
    },
    /// Fires the player's remote detonator: every waiting remote charge (C4) they placed goes
    /// off at once, or one after another in placement order when `sequenced`.
    RemoteDetonate {
        sequenced: bool,
    },
    PlaceInfrastructure {
        slot: u8,
    },
    BuyUpgrade {
        index: usize,
    },
    Refuel,
    Repair,
    SellCargo,
    Rescue,
    /// A town service transaction (shop, bank, salvage, contracts, expeditions, ...).
    Service(SessionServiceRequest),
    /// A test-mode toggle or warp (ignored outside a test-mode world).
    TestMode(TestModeAction),
    /// Tips the block hopper's blocks (one kind, or every kind) out beside the rig as drops
    /// the rig will not pick up again (WP-DROPS).
    DumpBlocks {
        block: Option<BlockKind>,
    },
    /// The rig's "collect blocks" toggle: off, the magnet ignores plain blocks (they still
    /// drop and other rigs can collect them).
    SetCollectBlocks {
        enabled: bool,
    },
}

/// A command submitted for one tick, with its position in that player's input for the tick.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TickAction {
    pub sequence: InputSequence,
    pub command: PlayerCommand,
}
