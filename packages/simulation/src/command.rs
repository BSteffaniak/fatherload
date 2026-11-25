//! Gameplay commands submitted by players (intent, not raw input).

use serde::{Deserialize, Serialize};

use crate::{
    enemy::EnemyDifficulty,
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
    /// Rescue a stranded or wrecked rig. On foot (Part II F5) it radios for a pickup instead:
    /// the pilot is hauled back aboard its rig for [`crate::pilot::PILOT_RECALL_FEE`].
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
    /// Changes the world's enemy setting for everyone (the game offers it to the host or a
    /// local session; Peaceful clears every hostile).
    SetEnemyDifficulty(EnemyDifficulty),
    /// Mounts an owned weapon on the rig's hardpoint (`None` holsters it).
    MountWeapon {
        weapon: Option<crate::weapon::WeaponKind>,
    },
    /// A base-building action: place, deconstruct or use a module (see [`crate::base`]).
    Build(crate::base::BuildCommand),
    /// Get out of bed, or change who must be in bed to skip the night (see [`crate::clock`];
    /// lying down is a bed's own service).
    Sleep(crate::clock::SleepCommand),
    /// The one exit / board button (Part II F5): climbs out of a slow rig as a pilot on foot,
    /// or boards the player's rig when the pilot stands close to it (the personal pack empties
    /// into the rig). Ignored when neither is possible.
    TogglePilot,
    /// Moves up to `count` of `item` between the pilot's personal pack and the rig's cargo bay
    /// or block hopper (`to_rig`: pack to rig). Only while the pilot is in or beside the rig.
    PilotPack {
        item: crate::item::ItemKind,
        count: u32,
        to_rig: bool,
    },
    /// Talk to, trade with or give a gift to a person (WP-PEOPLE, see
    /// [`crate::people::PeopleCommand`]).
    People(crate::people::PeopleCommand),
    /// A Moon action (WP-MOON): a deal at the lunar colony's counter, validated inside the
    /// tick (the rig or the pilot must be in the colony).
    Moon(crate::moon::MoonCommand),
    /// An on-foot action (WP-PILOT, see [`crate::pilot::PilotAction`]): use a lever, chest or
    /// keycard door, fire or drop a rope, recall the rig, swap the weapon in hand.
    Pilot(crate::pilot::PilotAction),
    /// Plots, contracts, decorating and the home's services (WP-HOME, see
    /// [`crate::home::HomeCommand`]).
    Home(crate::home::HomeCommand),
    /// A tomb run action (WP-TOMBS, see [`crate::tomb::TombCommand`]): a relic pick between
    /// floors. Entering, shrines, caches, the stairs and the way out are the use button
    /// ([`crate::pilot::PilotAction::Use`]).
    Tomb(crate::tomb::TombCommand),
    /// A depth logistics action (Part III F10, see [`crate::logistics::LogisticsCommand`]):
    /// lifts, beacons and travel, an outpost's counter, dive charges.
    Logistics(crate::logistics::LogisticsCommand),
    /// A space flight action (Wave 11, see [`crate::flight::FlightCommand`]): the Star Drive,
    /// docking, the beacon, waypoints and a station's services.
    Flight(crate::flight::FlightCommand),
}

/// A command submitted for one tick, with its position in that player's input for the tick.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TickAction {
    pub sequence: InputSequence,
    pub command: PlayerCommand,
}
