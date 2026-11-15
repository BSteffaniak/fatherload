//! Gameplay events and per-tick simulation output with stable `(tick, ordinal)` identity.

use serde::{Deserialize, Serialize};

use crate::weapon::WeaponKind;

use crate::{
    cave::{CaveKind, CaveTile},
    command::PlayerCommand,
    enemy::{EnemyDifficulty, EnemyKind, ShotKind},
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
    /// An enemy noticed a rig (its tell: a screech, a beep). `x`, `y` in world pixels.
    EnemyAlerted {
        entity: u64,
        kind: EnemyKind,
        x: i32,
        y: i32,
    },
    /// An enemy started telegraphing an attack (wind-up).
    EnemyWindup {
        entity: u64,
        kind: EnemyKind,
        x: i32,
        y: i32,
    },
    /// An enemy fired a projectile.
    EnemyFired {
        entity: u64,
        shot: ShotKind,
        x: i32,
        y: i32,
    },
    /// An enemy took damage and lives.
    EnemyHurt {
        entity: u64,
        kind: EnemyKind,
        x: i32,
        y: i32,
    },
    /// An enemy died (its loot dropped); `by` is the player credited, if any.
    EnemyKilled {
        entity: u64,
        kind: EnemyKind,
        x: i32,
        y: i32,
        by: Option<PlayerId>,
    },
    /// An EMP knocked a machine out.
    EnemyStunned {
        entity: u64,
        kind: EnemyKind,
        x: i32,
        y: i32,
    },
    /// An enemy (or its shot) hit a rig ([`Self::PlayerDamaged`] follows when hull was lost).
    RigStruck {
        player_id: PlayerId,
        kind: EnemyKind,
    },
    /// A player changed the world's enemy setting.
    EnemyDifficultyChanged {
        player_id: PlayerId,
        difficulty: EnemyDifficulty,
    },
    /// A rig fired its hardpoint weapon: from the muzzle `(x, y)` toward `(end_x, end_y)` (a
    /// hitscan ray's end, or a point along a projectile's launch direction).
    WeaponFired {
        player_id: PlayerId,
        weapon: WeaponKind,
        x: i32,
        y: i32,
        end_x: i32,
        end_y: i32,
    },
    /// A trigger pull with too little capacitor energy or no ammo (the empty click).
    WeaponDry {
        player_id: PlayerId,
        weapon: WeaponKind,
    },
    /// A rig swapped (or holstered) the weapon on its hardpoint.
    WeaponMounted {
        player_id: PlayerId,
        weapon: Option<WeaponKind>,
    },
    /// A shot or ray struck something at `(x, y)`.
    ProjectileImpact {
        weapon: WeaponKind,
        x: i32,
        y: i32,
        surface: ImpactSurface,
    },
    /// A damageable entity took `amount` damage (see [`crate::combat`]; enemies report
    /// [`Self::EnemyHurt`] instead).
    EntityDamaged {
        entity: u64,
        x: i32,
        y: i32,
        amount: u16,
    },
    /// A damageable entity was destroyed (and despawned); `by` is the player credited (enemies
    /// report [`Self::EnemyKilled`] instead).
    EntityDestroyed {
        entity: u64,
        x: i32,
        y: i32,
        by: Option<PlayerId>,
    },
    /// A player placed a building piece, fixture or module with its bottom-left tile at
    /// `(x, y)` (a claim beacon stakes a claim). See [`crate::base`].
    BasePlaced {
        player_id: PlayerId,
        item: crate::base::Buildable,
        x: i32,
        y: i32,
    },
    /// A player deconstructed what stood at tile `(x, y)` for a refund.
    BaseRemoved {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A player used one of a base module's services.
    BaseServiceUsed {
        player_id: PlayerId,
        structure: crate::base::StructureId,
        service: crate::base::ModuleService,
    },
    /// The sun came up: dawn of day `day` began (see [`crate::clock`]).
    Dawn {
        day: u32,
    },
    /// Dusk of day `day` began: night is coming.
    Dusk {
        day: u32,
    },
    /// Midnight passed: day `day` began.
    NewDay {
        day: u32,
    },
    /// Everyone needed was in bed: the clock jumped to the dawn of day `day`.
    NightSkipped {
        day: u32,
    },
    /// A player got out of bed (by choice, by driving off, or because morning came).
    GotUp {
        player_id: PlayerId,
    },
    /// A player changed who must be in bed to skip the night.
    SleepRuleChanged {
        player_id: PlayerId,
        rule: crate::clock::SleepRule,
    },
    /// A rig climbed into a higher altitude band of a V8 sky (see [`crate::sky::SkyBand`]).
    SkyBandEntered {
        player_id: PlayerId,
        band: crate::sky::SkyBand,
    },
    /// A rig flew into an updraft ([`crate::worldgen::sky::Updraft`]).
    UpdraftCaught {
        player_id: PlayerId,
    },
    /// A rig flew into cold its de-icer does not cover: the hull starts icing up (see
    /// [`crate::environment::cold_excess`]).
    RigIcing {
        player_id: PlayerId,
    },
}

/// What a weapon shot struck.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ImpactSurface {
    Terrain,
    /// A damageable entity.
    Target,
    /// A live charge (which the hit sets off).
    Charge,
    /// A loose drop speared by a harpoon.
    Loot,
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
        PlayerCommand::SetEnemyDifficulty(difficulty) => {
            GameplayEventKind::EnemyDifficultyChanged {
                player_id,
                difficulty: *difficulty,
            }
        }
        PlayerCommand::MountWeapon { weapon } => GameplayEventKind::WeaponMounted {
            player_id,
            weapon: *weapon,
        },
        PlayerCommand::Build(crate::base::BuildCommand::Place { item, at, .. }) => {
            GameplayEventKind::BasePlaced {
                player_id,
                item: *item,
                x: at.x,
                y: at.y,
            }
        }
        PlayerCommand::Build(crate::base::BuildCommand::Remove { at }) => {
            GameplayEventKind::BaseRemoved {
                player_id,
                x: at.x,
                y: at.y,
            }
        }
        PlayerCommand::Sleep(crate::clock::SleepCommand::GetUp) => {
            GameplayEventKind::GotUp { player_id }
        }
        PlayerCommand::Sleep(crate::clock::SleepCommand::SetRule(rule)) => {
            GameplayEventKind::SleepRuleChanged {
                player_id,
                rule: *rule,
            }
        }
        PlayerCommand::Build(crate::base::BuildCommand::Use { structure, service }) => {
            GameplayEventKind::BaseServiceUsed {
                player_id,
                structure: *structure,
                service: *service,
            }
        }
    }
}
