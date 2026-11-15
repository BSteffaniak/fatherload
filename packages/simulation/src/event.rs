//! Gameplay events and per-tick simulation output with stable `(tick, ordinal)` identity.

use serde::{Deserialize, Serialize};

use crate::{
    cave::{CaveKind, CaveTile},
    command::PlayerCommand,
    explosive::{BlastDirection, ExplosiveKind},
    ids::{InputSequence, PlayerId, SimulationTick},
    item::{ItemKind, Stowage},
    poi::{PoiKind, PoiReward},
    quest::{BlueprintId, QuestFailure, QuestId},
    summary::PlayerScopedCommandOutcome,
    worldgen::FeatureId,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameplayEventId {
    pub tick: SimulationTick,
    pub ordinal: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GameplayEventKind {
    DrillStarted {
        player_id: PlayerId,
    },
    PurchaseCompleted {
        player_id: PlayerId,
    },
    CargoChanged {
        player_id: PlayerId,
    },
    BombPlaced {
        player_id: PlayerId,
    },
    InfrastructurePlaced {
        player_id: PlayerId,
    },
    ScannerUsed {
        player_id: PlayerId,
    },
    EscapeStarted {
        player_id: PlayerId,
    },
    EscapeTunnelChanged,
    DeepRewardGranted {
        player_id: PlayerId,
    },
    RescueTriggered {
        player_id: PlayerId,
    },
    PlayerDamaged {
        player_id: PlayerId,
    },
    HazardChanged,
    BouldersChanged,
    BombDetonated {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    BombBounced {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    /// A player deployed (dropped, threw or placed) an explosive other than via
    /// [`PlayerCommand::PlaceBomb`].
    ExplosiveDeployed {
        player_id: PlayerId,
        kind: ExplosiveKind,
    },
    /// A player fired their remote detonator and at least one charge was armed by it.
    RemoteDetonation {
        player_id: PlayerId,
    },
    /// A sticky charge (sticky bomb, C4, shaped charge) stuck to a surface.
    ChargeStuck {
        bomb_id: u64,
        kind: ExplosiveKind,
        x: i32,
        y: i32,
    },
    /// An explosive other than the plain bomb went off ([`Self::BombDetonated`] stays the plain
    /// bomb's event). For a flare this is it lighting up; for an EMP, its pulse.
    ExplosiveDetonated {
        bomb_id: u64,
        kind: ExplosiveKind,
        x: i32,
        y: i32,
        /// Which way a shaped charge cut (meaningless for the other kinds).
        direction: BlastDirection,
    },
    /// A cluster bomb burst and released its bomblets.
    ClusterSplit {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    /// An incendiary's fire spread one ring further.
    FireSpread {
        bomb_id: u64,
        x: i32,
        y: i32,
    },
    /// A player opened a POI tile (a crate, a derelict's core, a vault cache, an outpost
    /// locker) and received `reward`.
    PoiOpened {
        player_id: PlayerId,
        poi: PoiKind,
        reward: PoiReward,
    },
    /// A player entered a point of interest for the first time.
    PoiDiscovered {
        player_id: PlayerId,
        feature: FeatureId,
        poi: PoiKind,
    },
    /// A vault's lock opened (its doors are gone).
    VaultOpened {
        feature: FeatureId,
    },
    /// A vault's lock reset (wrong switch, or a relay cooled down).
    VaultLockReset {
        feature: FeatureId,
    },
    /// A player changed a test-mode toggle or warped (test-mode worlds only).
    TestModeChanged {
        player_id: PlayerId,
    },
    /// A player took on a quest at its giver's counter.
    QuestAccepted {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest moved on to its next stage.
    QuestStageDone {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// Every stage of a quest is done: it can be turned in.
    QuestReady {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest was turned in and paid (the completion fanfare).
    QuestCompleted {
        player_id: PlayerId,
        quest: QuestId,
    },
    /// A quest failed (its twist caught the player out).
    QuestFailed {
        player_id: PlayerId,
        quest: QuestId,
        reason: QuestFailure,
    },
    /// A player finished assembling a blueprint from its fragments.
    BlueprintCompleted {
        player_id: PlayerId,
        blueprint: BlueprintId,
    },
    /// A player repainted or refitted their rig at the garage.
    RigCustomized {
        player_id: PlayerId,
    },
    /// A rig picked up `count` of `item` from a drop at world pixel `(x, y)` (WP-DROPS; see
    /// `world/drops.rs`).
    ItemsCollected {
        player_id: PlayerId,
        item: ItemKind,
        count: u32,
        x: i32,
        y: i32,
    },
    /// A drop sits in a rig's pickup reach but its cargo bay or block hopper (`stowage`) is
    /// full (once per stack that appears, not every tick).
    PickupBlocked {
        player_id: PlayerId,
        stowage: Stowage,
    },
    /// A rig tipped blocks out of its hopper.
    BlocksDumped {
        player_id: PlayerId,
    },
    /// A rig switched its "collect blocks" toggle.
    CollectBlocksToggled {
        player_id: PlayerId,
        enabled: bool,
    },
    /// A player entered a cave ecosystem for the first time ("Found: Crystal Geode").
    CaveDiscovered {
        player_id: PlayerId,
        feature: FeatureId,
        cave: CaveKind,
    },
    /// A player drilled open a cave cache: `credits` paid, its loot spilled as item drops.
    CaveCacheOpened {
        player_id: PlayerId,
        tile: CaveTile,
        credits: u32,
    },
    /// A stalactite broke off the ceiling above a rig at tile `(x, y)` and is falling.
    StalactiteFell {
        x: i32,
        y: i32,
    },
    /// A player's rig entered a village for the first time.
    VillageDiscovered {
        player_id: PlayerId,
        village: crate::village::VillageId,
    },
    /// A player bought something at a village counter.
    VillageServiceUsed {
        player_id: PlayerId,
        village: crate::village::VillageId,
        service: crate::village::VillageService,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameplayEvent {
    pub id: GameplayEventId,
    pub kind: GameplayEventKind,
}

/// How one one-shot action submitted for a tick was resolved.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionOutcome {
    pub player_id: PlayerId,
    pub sequence: InputSequence,
    pub outcome: PlayerScopedCommandOutcome,
}

/// Everything one tick produced besides the state change itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickOutput {
    pub tick: SimulationTick,
    pub events: Vec<GameplayEvent>,
    /// Every action in the tick's inputs, in input order, with its outcome.
    pub action_outcomes: Vec<ActionOutcome>,
    /// Every terrain tile write this tick, in order (the op-log replication section).
    pub terrain_ops: Vec<crate::terrain::TileChange>,
    /// Tiles each player newly discovered this tick (op log, private to that player).
    pub discoveries: Vec<crate::world::Discovery>,
}

impl TickOutput {
    pub(crate) fn push_event(&mut self, kind: GameplayEventKind) {
        let ordinal = u32::try_from(self.events.len()).unwrap_or(u32::MAX);
        self.events.push(GameplayEvent {
            id: GameplayEventId {
                tick: self.tick,
                ordinal,
            },
            kind,
        });
    }
}

pub(crate) const fn gameplay_event_for_applied_command(
    player_id: PlayerId,
    command: &PlayerCommand,
) -> GameplayEventKind {
    match command {
        PlayerCommand::Service(crate::service::SessionServiceRequest::CustomizeRig { .. }) => {
            GameplayEventKind::RigCustomized { player_id }
        }
        PlayerCommand::Service(crate::service::SessionServiceRequest::Village {
            village,
            service,
            ..
        }) => GameplayEventKind::VillageServiceUsed {
            player_id,
            village: *village,
            service: *service,
        },
        PlayerCommand::Refuel
        | PlayerCommand::Repair
        | PlayerCommand::BuyUpgrade { .. }
        | PlayerCommand::Service(_) => GameplayEventKind::PurchaseCompleted { player_id },
        PlayerCommand::SellCargo => GameplayEventKind::CargoChanged { player_id },
        PlayerCommand::PlaceBomb => GameplayEventKind::BombPlaced { player_id },
        PlayerCommand::DeployExplosive { kind, .. } => GameplayEventKind::ExplosiveDeployed {
            player_id,
            kind: *kind,
        },
        PlayerCommand::RemoteDetonate { .. } => GameplayEventKind::RemoteDetonation { player_id },
        PlayerCommand::PlaceInfrastructure { .. } => {
            GameplayEventKind::InfrastructurePlaced { player_id }
        }
        PlayerCommand::UseScanner => GameplayEventKind::ScannerUsed { player_id },
        PlayerCommand::Rescue => GameplayEventKind::RescueTriggered { player_id },
        PlayerCommand::TestMode(_) => GameplayEventKind::TestModeChanged { player_id },
        PlayerCommand::DumpBlocks { .. } => GameplayEventKind::BlocksDumped { player_id },
        PlayerCommand::SetCollectBlocks { enabled } => GameplayEventKind::CollectBlocksToggled {
            player_id,
            enabled: *enabled,
        },
    }
}
