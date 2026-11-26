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
    /// A player drilled open a cave cache: `credits` paid, its loot spilled as item drops,
    /// and an antique piece of furniture for their home stock when it held one (WP-HOME).
    CaveCacheOpened {
        player_id: PlayerId,
        tile: CaveTile,
        credits: u32,
        #[serde(default)]
        furniture: Option<crate::home::Furniture>,
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
    /// A player spoke to a person (WP-PEOPLE). `npc` is the person's entity id.
    PersonTalked {
        player_id: PlayerId,
        npc: u64,
        /// The first time this person met this player.
        first_meeting: bool,
    },
    /// A player bought something from a person's trade list.
    PersonTraded {
        player_id: PlayerId,
        npc: u64,
        kind: crate::people::OfferKind,
    },
    /// A player handed a person a gift.
    PersonGifted {
        player_id: PlayerId,
        npc: u64,
        reaction: crate::people::offers::GiftReaction,
    },
    /// How a person regards a player moved to a new standing (`warmer`: up, else down).
    AffinityChanged {
        player_id: PlayerId,
        npc: u64,
        standing: crate::people::Standing,
        warmer: bool,
    },
    /// A person's rumour marked a site on a player's map.
    RumourMarked {
        player_id: PlayerId,
        npc: u64,
        feature: FeatureId,
    },
    /// Any applied people command (the specific events above carry the detail).
    PeopleInteraction {
        player_id: PlayerId,
    },
    /// A person pitched a story arc to a player (WP-STORY; the offer waits for a yes).
    ArcOffered {
        player_id: PlayerId,
        arc: crate::people::ArcId,
    },
    /// A player took on a story arc.
    ArcAccepted {
        player_id: PlayerId,
        arc: crate::people::ArcId,
    },
    /// A beat of a story arc is done (`step`: the beat that just finished, from 0).
    ArcStepDone {
        player_id: PlayerId,
        arc: crate::people::ArcId,
        step: u8,
    },
    /// The last beat of a story arc is done: its climax lands; go back to the giver.
    ArcClimax {
        player_id: PlayerId,
        arc: crate::people::ArcId,
    },
    /// A story arc ended at its giver with ending `outcome`.
    ArcCompleted {
        player_id: PlayerId,
        arc: crate::people::ArcId,
        outcome: u8,
    },
    /// A story arc was given up or failed.
    ArcAbandoned {
        player_id: PlayerId,
        arc: crate::people::ArcId,
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
    // --- Sky isles (WP-SKY, T20, `world/sky_isles.rs`) -----------------------------------
    /// A player drilled open a sky cache or a nest: `credits` went to them, the loot spilled.
    SkyCacheOpened {
        player_id: PlayerId,
        tile: crate::sky::SkyTile,
        credits: u32,
    },
    /// A player bought a fuel buoy's refill.
    BuoyRefuelled {
        player_id: PlayerId,
    },
    /// A crumbling isle's keystone at tile `(x, y)` broke: the island falls apart.
    IsleCollapsing {
        x: i32,
        y: i32,
    },
    /// A rig slipped into a jet stream (`worldgen::sky::winds`).
    JetStreamCaught {
        player_id: PlayerId,
    },
    // --- The Moon (WP-MOON, `world/moon.rs`) -----------------------------------------------
    /// A rig met vacuum (orbit or the Moon) without the gear for it: the hull starts to
    /// buckle ([`crate::moon::VacuumBreach`] says what is missing).
    MoonVacuumBreach {
        player_id: PlayerId,
        breach: crate::moon::VacuumBreach,
    },
    /// A rig touched down on the Moon's top side for the first time: the set-piece landing.
    MoonLanded {
        player_id: PlayerId,
    },
    /// A deal at the lunar research colony's counter.
    ColonyServiceUsed {
        player_id: PlayerId,
        offer: crate::moon::ColonyOffer,
    },
    /// A pilot struck a pylon in the alien facility's puzzle hall: `lit` pylons now glow, or
    /// the wrong one went dark and `reset` every pylon.
    FacilityPylon {
        player_id: PlayerId,
        lit: u8,
        reset: bool,
    },
    /// Every pylon glows: the facility's gate slid open.
    FacilityGateOpened {
        player_id: PlayerId,
    },
    /// The Warden woke in its arena (enemy entity `entity`).
    WardenAwakened {
        entity: u64,
    },
    /// A broken conduit (or an EMP) knocked the Warden's shield down.
    WardenStaggered {
        entity: u64,
    },
    /// The Warden fell: the core seal opens. `by` landed the last hit.
    WardenDefeated {
        by: Option<PlayerId>,
    },
    /// A pilot reached the strange core at the Moon's heart.
    MoonCoreReached {
        player_id: PlayerId,
    },
    /// A lunar uplink beamed a rig to (`to_moon`) or from the Moon.
    UplinkBeamed {
        player_id: PlayerId,
        to_moon: bool,
    },
    // --- The pilot on foot (Part II F5, `world/pilot.rs`) ---------------------------------
    /// The pilot climbed out of its rig at world pixel `(x, y)` (its feet).
    PilotExited {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// The pilot boarded its rig; `transferred` items moved from the personal pack into the
    /// rig on the way in.
    PilotBoarded {
        player_id: PlayerId,
        transferred: u32,
    },
    /// The pilot jumped.
    PilotJumped {
        player_id: PlayerId,
    },
    /// The pilot landed after a fall (`hard`: fast enough to hurt).
    PilotLanded {
        player_id: PlayerId,
        hard: bool,
    },
    /// The pilot grabbed a ledge.
    PilotLedgeGrab {
        player_id: PlayerId,
    },
    /// The pilot went into water.
    PilotSplashed {
        player_id: PlayerId,
    },
    /// The pilot took `amount` damage at world pixel `(x, y)`.
    PilotHurt {
        player_id: PlayerId,
        amount: u16,
        x: i32,
        y: i32,
    },
    /// The pilot died at world pixel `(x, y)` (it respawns in its rig or at the home point,
    /// leaving a small drop of carried items behind).
    PilotDied {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// The pilot's air ran low (a quarter left) or out (`empty`).
    PilotOxygenLow {
        player_id: PlayerId,
        empty: bool,
    },
    /// The jetpack lit.
    JetpackIgnited {
        player_id: PlayerId,
    },
    /// The jetpack ran dry in the air.
    JetpackEmpty {
        player_id: PlayerId,
    },
    /// The hand pick chipped tile `(x, y)` (`broke`: it came out and dropped).
    PickStruck {
        player_id: PlayerId,
        x: i32,
        y: i32,
        broke: bool,
    },
    /// The hand pick clinked off a tile it cannot break.
    PickClinked {
        player_id: PlayerId,
    },
    /// Items moved between the personal pack and the rig (`to_rig`: into the rig).
    PackTransferred {
        player_id: PlayerId,
        count: u32,
        to_rig: bool,
    },
    /// A player bought an upgrade at the shop: `upgrade` is now fitted at `level` (bomb packs
    /// report level 0). Follows the purchase's [`Self::PurchaseCompleted`]; the game plays the
    /// purchase moment (fanfare, rig transform, stat bars) from it. See
    /// [`crate::upgrade_preview`].
    UpgradeInstalled {
        player_id: PlayerId,
        upgrade: crate::economy::UpgradeKind,
        level: u8,
    },
    /// A player earned a first-time milestone; its reward cosmetics are already unlocked (see
    /// [`crate::milestone`]).
    MilestoneEarned {
        player_id: PlayerId,
        milestone: crate::milestone::Milestone,
    },
    // --- Sky continents, sky enemies and Act II weapons (Wave 4 T21) ----------------------
    /// A storm elemental's lightning arc from `(x, y)` to `(end_x, end_y)`; `struck` is the
    /// player whose rig or pilot it hit (rigs are jolted: capacitor drained, weapon jammed).
    LightningArc {
        x: i32,
        y: i32,
        end_x: i32,
        end_y: i32,
        struck: Option<PlayerId>,
    },
    /// One link of an arc rifle's chain lightning, `(x, y)` to `(end_x, end_y)`.
    ArcChained {
        player_id: PlayerId,
        x: i32,
        y: i32,
        end_x: i32,
        end_y: i32,
    },
    /// A gravity gun let go of what it held, flinging `count` things from `(x, y)`.
    GravityThrow {
        player_id: PlayerId,
        x: i32,
        y: i32,
        count: u16,
    },
    /// A tether harpoon hooked terrain at `(x, y)`: the rig swings from it while the trigger
    /// is held.
    TetherAttached {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A rig let go of its tether.
    TetherReleased {
        player_id: PlayerId,
    },
    /// A flak shell burst at `(x, y)`.
    FlakBurst {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A pilot pulled a temple lever at tile `(x, y)`, opening `opened` gate tiles.
    TempleLeverPulled {
        player_id: PlayerId,
        x: i32,
        y: i32,
        opened: u16,
    },
    /// A temple's keystone gate at tile `(x, y)` opened for a pilot carrying the keystone.
    TempleGateOpened {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A pilot broke open a temple's relic chest at tile `(x, y)`.
    RelicChestOpened {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A sky enemy did something worth a cue: an airship went down, dropped boarders, a harpy
    /// nester flew into a rage, a flyer began a swoop.
    SkyEnemyAction {
        entity: u64,
        kind: crate::enemy::EnemyKind,
        action: crate::enemy::SkyEnemyAction,
        x: i32,
        y: i32,
    },
    // --- WP-PILOT (T23, `world/pilot_arms.rs`, `world/pilot_gear.rs`, `world/mechanisms.rs`) --
    /// The pilot fired (or swung) a personal arm from `(x, y)` toward `(end_x, end_y)`.
    PilotFired {
        player_id: PlayerId,
        weapon: crate::weapon::WeaponKind,
        x: i32,
        y: i32,
        end_x: i32,
        end_y: i32,
    },
    /// The pilot pulled the trigger with no ammo for `weapon`.
    PilotWeaponDry {
        player_id: PlayerId,
        weapon: crate::weapon::WeaponKind,
    },
    /// The pilot took `weapon` in hand (`None`: the hand pick).
    PilotWeaponSwapped {
        player_id: PlayerId,
        weapon: Option<crate::weapon::WeaponKind>,
    },
    /// A rope action at world pixel `(x, y)` (the hook or the winch).
    PilotRope {
        player_id: PlayerId,
        action: crate::pilot::RopeEvent,
        x: i32,
        y: i32,
    },
    /// The glider opened, or (`hover`) the jetpack started hovering.
    PilotGlide {
        player_id: PlayerId,
        hover: bool,
    },
    /// Something happened at a ruin mechanism at tile `(x, y)` (`player_id`: who caused it,
    /// if anyone; plates release on their own).
    MechanismUsed {
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
        mechanism: crate::mechanism::MechanismEvent,
    },
    /// A pilot opened the treasure chest at tile `(x, y)`.
    ChestOpened {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A rig recall started, arrived or found no path.
    RigRecall {
        player_id: PlayerId,
        outcome: crate::pilot::RecallOutcome,
    },
    /// The pilot's suit stopped keeping out the cold (or, `heat`, the deep heat).
    PilotExposed {
        player_id: PlayerId,
        heat: bool,
    },
    /// The pilot kicked off a wall (climbing boots).
    PilotWallJumped {
        player_id: PlayerId,
    },
    /// A player bought the deed to a plot (WP-HOME, see [`crate::home`]).
    PlotBought {
        player_id: PlayerId,
        plot: crate::home::PlotId,
        size: crate::home::PlotSize,
        setting: crate::home::PlotSetting,
    },
    /// A contract was signed: a contractor starts work on a plot.
    ContractSigned {
        player_id: PlayerId,
        plot: crate::home::PlotId,
        job: crate::home::HomeJob,
    },
    /// Construction on a plot reached a new visible stage.
    ConstructionStage {
        plot: crate::home::PlotId,
        owner: PlayerId,
        stage: crate::home::BuildStage,
    },
    /// The work on a plot is finished: a new home, or an expansion.
    HomeCompleted {
        plot: crate::home::PlotId,
        owner: PlayerId,
        job: crate::home::HomeJob,
    },
    /// A small story beat on a building site (a shortage, a request, an anecdote).
    SiteEventRaised {
        plot: crate::home::PlotId,
        owner: PlayerId,
        event: crate::home::SiteEventKind,
    },
    /// A player answered a building site open event.
    SiteEventAnswered {
        player_id: PlayerId,
        plot: crate::home::PlotId,
    },
    /// A piece of furniture (or a trophy) went into a home.
    FurniturePlaced {
        player_id: PlayerId,
        plot: crate::home::PlotId,
        furniture: crate::home::Furniture,
    },
    /// A piece of furniture was picked up.
    FurnitureRemoved {
        player_id: PlayerId,
        plot: crate::home::PlotId,
        furniture: crate::home::Furniture,
    },
    /// A player paid to rush construction.
    HomeRushed {
        player_id: PlayerId,
        plot: crate::home::PlotId,
    },
    /// Any applied home command (the specific events above carry the detail).
    HomeInteraction {
        player_id: PlayerId,
    },
    // --- Night-time gameplay (WP-NIGHT, `world/night.rs`) ---------------------------------
    /// Night monsters began to rise on the open surface (late dusk of day `night`) under
    /// `moon`.
    NightFell {
        night: u32,
        moon: crate::clock::MoonPhase,
    },
    /// Dawn came after night `night`: its monsters burrow and flee; `kills` night monsters fell
    /// to the crew that night.
    NightSurvived {
        night: u32,
        kills: u32,
    },
    /// The rare spawn of a new-moon night walks at world pixel `(x, y)`.
    RareNightSpawn {
        kind: crate::enemy::EnemyKind,
        x: i32,
        y: i32,
    },
    /// A night monster burrowed into the ground or fled into the dark at dawn (and left the
    /// world) at world pixel `(x, y)`.
    NightMonsterBurrowed {
        entity: u64,
        kind: crate::enemy::EnemyKind,
        x: i32,
        y: i32,
    },
    // --- Tombs and crypts (WP-TOMBS, Wave 5 T27; netcode codes 270-279) -----------------
    /// A party went into a tomb site (`player_id` opened the door; `party` members).
    TombEntered {
        player_id: PlayerId,
        style: crate::tomb::SiteStyle,
        tier: u8,
        party: u8,
    },
    /// A site's seal at door tile `(x, y)` broke (`opened`), or held against a try.
    TombSeal {
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
        opening: crate::tomb::SiteOpening,
        opened: bool,
    },
    /// An ambush sprang (or the guardian woke, `boss`) at world pixel `(x, y)`.
    TombAmbush {
        x: i32,
        y: i32,
        boss: bool,
    },
    /// A tomb's guardian fell.
    TombGuardianDefeated {
        by: Option<PlayerId>,
    },
    /// `player_id` took the stairs off floor `floor` (0-based): relic picks follow.
    TombFloorCleared {
        player_id: PlayerId,
        floor: u8,
    },
    /// A member picked a relic between floors (`None`: passed).
    TombRelicPicked {
        player_id: PlayerId,
        relic: Option<crate::tomb::Relic>,
    },
    /// A member rested at a shrine: the party's loot banked (`banked` credits in all), the
    /// downed revived, the run saved.
    TombShrineRested {
        player_id: PlayerId,
        banked: u32,
    },
    /// A member opened a cache worth `value` (unbanked), maybe with a tomb key or a lore page.
    TombCacheOpened {
        player_id: PlayerId,
        value: u32,
        key: bool,
        lore: Option<u8>,
    },
    /// A member went down (or was revived, `revived`).
    TombMemberDowned {
        player_id: PlayerId,
        revived: bool,
    },
    /// A member's run ended: what was banked and lost.
    TombRunEnded {
        player_id: PlayerId,
        outcome: crate::tomb::RunOutcome,
        banked: u32,
        lost: u32,
    },
    // --- Side-view tombs (WP-TOMBS, Wave 5 T28; netcode codes 280-289) -------------------
    /// A side-view trap near the party went off at world pixel `(x, y)`: a crusher slammed, a
    /// dart flew, a vent burst, a sluice began to pour, a slab fell (presentation: cues and
    /// dust; the traps themselves are pure functions of the clock).
    TombTrapSprung {
        kind: crate::tomb::realize::TrapKind,
        x: i32,
        y: i32,
    },
    /// A tomb guardian was hurt into phase `phase` (2: its second pattern, 3: enraged) at world
    /// pixel `(x, y)`.
    TombGuardianPhase {
        kind: crate::enemy::EnemyKind,
        phase: u8,
        x: i32,
        y: i32,
    },
    // --- Top-down crypts (WP-TOMBS, Wave 5 T29; netcode codes 290-299) ------------------
    /// A crypt pilot dodge-rolled at world pixel `(x, y)` (its feet).
    CryptRoll {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// A crypt trap at tile `(x, y)` gave its warning (`fired: false`: a rattle, a glow, a
    /// click, a rumble, cracks) or went off (one event per room and trap kind at a time).
    CryptTrap {
        kind: crate::tomb::crypt::CryptTrapKind,
        x: i32,
        y: i32,
        fired: bool,
    },
    /// A crypt pilot fell into a pit at tile `(x, y)` (it climbs out at the nearest safe floor).
    CryptPitFall {
        player_id: PlayerId,
        x: i32,
        y: i32,
    },
    /// The crypt lord woke or entered phase `phase` (1..=3) at world pixel `(x, y)`.
    CryptLordPhase {
        phase: u8,
        x: i32,
        y: i32,
    },
    /// A crypt creature began a telegraphed attack at world pixel `(x, y)`, aimed at
    /// `(aim_x, aim_y)`.
    CryptTell {
        kind: crate::enemy::EnemyKind,
        attack: crate::tomb::crypt::CryptAttack,
        x: i32,
        y: i32,
        aim_x: i32,
        aim_y: i32,
    },
    /// A golem's slam landed around world pixel `(x, y)` (reach `radius` px).
    CryptSlam {
        x: i32,
        y: i32,
        radius: u16,
    },
    /// The crypt lord called `count` creatures up out of the floor around `(x, y)`.
    CryptSummon {
        x: i32,
        y: i32,
        count: u8,
    },
    // Fluids (Part III F9, wave 7; netcode codes 380-389).
    /// A terrain change opened a fluid body at tile `(x, y)`: the flood begins.
    FluidBreached {
        kind: crate::fluid::FluidKind,
        x: i32,
        y: i32,
    },
    /// A fluid reaction happened at tile `(x, y)` (at most one event per reaction per step).
    FluidReacted {
        reaction: crate::fluid::FluidReaction,
        x: i32,
        y: i32,
    },
    /// A rig went under (two thirds submerged: submarine mode).
    RigSubmerged {
        player_id: PlayerId,
        kind: crate::fluid::FluidKind,
    },
    /// A rig came back up out of fluid.
    RigSurfaced {
        player_id: PlayerId,
        kind: crate::fluid::FluidKind,
    },
    // --- Depth logistics (Part III F10, Wave 7; netcode codes 390-399) -------------------
    /// A rig boarded lift `shaft`'s cab for the station at `to_row`.
    LiftRideStarted {
        player_id: PlayerId,
        shaft: u32,
        to_row: i32,
    },
    /// A lift cab arrived with its rider.
    LiftArrived {
        player_id: PlayerId,
        shaft: u32,
    },
    /// An ancient lift was restored and runs.
    LiftRestored {
        player_id: PlayerId,
        shaft: u32,
    },
    /// A lift station (`beacon: false`) or depth beacon went up on tile `(x, y)`.
    LogisticsPlaced {
        player_id: PlayerId,
        x: i32,
        y: i32,
        beacon: bool,
    },
    /// A rig reached a deep outpost for the first time (it joins the travel network).
    OutpostReached {
        player_id: PlayerId,
        outpost: u32,
    },
    /// The crew claimed a deep outpost.
    OutpostClaimed {
        player_id: PlayerId,
        outpost: u32,
    },
    /// A counter service at an outpost (`outpost: u32::MAX`: dive charges bought in town).
    OutpostServiced {
        player_id: PlayerId,
        outpost: u32,
        service: crate::logistics::OutpostService,
    },
    /// A rig travelled the beacon network from world pixel `(from_x, from_y)` to `(to_x,
    /// to_y)`.
    BeaconTravelled {
        player_id: PlayerId,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    },
    /// A dive charge went off under the rig at world pixel `(x, y)`, cutting `rows` rows.
    DiveChargeFired {
        player_id: PlayerId,
        x: i32,
        y: i32,
        rows: u8,
    },
    /// A rig went into Descent `descent` without `system` (Descent worlds): the depths crush it.
    DescentGateBlocked {
        player_id: PlayerId,
        descent: u8,
        system: crate::depth_progression::RigSystem,
    },
    // --- The authored Crust (Wave 8, `crate::crust`; netcode codes 400-419) ---------------
    /// A rig hooked onto a rail of the old mine works.
    CartBoarded {
        player_id: PlayerId,
    },
    /// A riding rig took the lower branch at a junction.
    CartSwitched {
        player_id: PlayerId,
    },
    /// A rig left its cart (jumped off, or the line ended) at `speed` tiles a second.
    CartLeft {
        player_id: PlayerId,
        speed: u8,
    },
    /// A timber set lost its support at tile `(x, y)`: it creaks before the roof comes down.
    SupportCreaked {
        x: i32,
        y: i32,
    },
    /// A timber set came down at tile `(x, y)`.
    SupportCollapsed {
        x: i32,
        y: i32,
    },
    /// A slate shelf left hanging at tile `(x, y)` shudders before it slides.
    SlateShudder {
        x: i32,
        y: i32,
    },
    /// A slate shelf slid down at tile `(x, y)`.
    SlateSlid {
        x: i32,
        y: i32,
    },
    /// A strongbox was drilled open: expedition log `log` (`first`: the crew had not read it).
    StrongboxOpened {
        player_id: PlayerId,
        log: u8,
        first: bool,
    },
    /// The Sinkhole's core came out: the ground rumbles before the collapse climbs.
    SinkholeRumble {
        player_id: PlayerId,
    },
    /// A rig got clear of the Sinkhole's collapse.
    SinkholeEscaped {
        player_id: PlayerId,
    },
    /// The Sinkhole's collapse caught a rig (it was dug out onto the lip).
    SinkholeCaught {
        player_id: PlayerId,
    },
    /// Nobody got clear: the Sinkhole is as it was, its core back in the floor.
    SinkholeReset {
        player_id: PlayerId,
    },
    /// The Sinkhole topped out with someone clear: the crew holds the Deep Claim and Drill II.
    DeepClaimGranted {
        player_id: PlayerId,
    },
    /// A rig went below Descent I without the crew's Deep Claim (Descent worlds).
    DeepClaimMissing {
        player_id: PlayerId,
    },
    /// A rig went into a lava tube's heat.
    LavaTubeEntered {
        player_id: PlayerId,
    },
    /// A Crust creature did something the game cues, at world pixel `(x, y)`.
    CrustCreature {
        action: crate::crust::CrustCreatureAction,
        x: i32,
        y: i32,
    },
    // --- Descent II, the Old Deep (Wave 8, codes 420-439; see `crate::old_deep`) ---------
    /// Drilling resonant crystal rang the fault: `shattered` crystals broke around tile
    /// `(x, y)`; `harvested` when a Resonance Drill caught their shards.
    ResonanceChain {
        player_id: PlayerId,
        x: i32,
        y: i32,
        shattered: u8,
        harvested: bool,
    },
    /// A fossil bone came out of the rock at tile `(x, y)`, whole or crumbled.
    FossilBoneFreed {
        player_id: PlayerId,
        x: i32,
        y: i32,
        intact: bool,
    },
    /// A whole skeleton was dug out without a broken bone: the museum's bounty.
    SkeletonCompleted {
        player_id: PlayerId,
        bones: u8,
        bounty: u32,
    },
    /// The Fossil Oceans' gas tide turned (`high`: the pockets breathe gas).
    GasTideTurned {
        high: bool,
    },
    /// A rig touched amber and is held fast.
    AmberStuck {
        player_id: PlayerId,
    },
    /// A Cathedral Lift valve opened (`opened` of three).
    CathedralValveOpened {
        player_id: PlayerId,
        valve: u8,
        opened: u8,
    },
    /// The Cathedral Lift runs again: a fast route between Cathedral Station and the nave.
    CathedralLiftRestored {
        player_id: PlayerId,
    },
    /// A rig's dose meter crossed warning step `level` (1-3).
    DoseWarning {
        player_id: PlayerId,
        level: u8,
    },
    /// A mutation relic changed a rig.
    MutationGained {
        player_id: PlayerId,
        mutation: crate::old_deep::Mutation,
    },
    /// A machine hall woke as a rig came near (its top-left tile).
    MachineryWoke {
        x: i32,
        y: i32,
    },
    /// A piston's ram slammed onto a rig.
    PistonStruck {
        player_id: PlayerId,
    },
    /// The Mantle Storm Zone's weather turned.
    HeatStormChanged {
        phase: crate::old_deep::StormPhase,
    },
    /// A storm vent poured molten metal at tile `(x, y)`.
    LavaPoured {
        x: i32,
        y: i32,
    },
    /// A story log was read: an expedition cache, a builder tablet or the core's revelation
    /// (`crate::old_deep::LOG_*`).
    DeepLogRead {
        player_id: PlayerId,
        log: u8,
    },
    /// The Star Core woke as a rig entered its arena.
    StarCoreAwoke,
    /// The Star Core's fight changed phase (`strikes_left` before it is spent).
    StarCorePhase {
        phase: crate::old_deep::CorePhase,
        strikes_left: u8,
    },
    /// The Star Core is spent: the Pressure Hull is the crew's and the way below opens.
    StarCoreBeaten {
        player_id: PlayerId,
    },
    /// The Glowmother woke in her den.
    GlowmotherAwoke {
        x: i32,
        y: i32,
    },
    /// The Glowmother fell.
    GlowmotherDefeated {
        player_id: PlayerId,
    },
    // The Living Dark (Wave 8, Descent III; netcode codes 440-459).
    /// Something of the Living Dark happened at tile `(x, y)`; `player_id` is whose deed or
    /// whom it concerns, if anyone's.
    LivingDark {
        what: crate::living_dark::LivingDarkEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The Drowned and Frozen (Wave 9, Descent IV; netcode codes 520-539).
    /// Something of the Drowned and Frozen happened at tile `(x, y)`; `player_id` is whose
    /// deed or whom it concerns, if anyone's.
    Drowned {
        what: crate::drowned::DrownedEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The distortion strata (Wave 9, Descent VI part A; netcode codes 560-579).
    /// Something of the distortion strata happened at tile `(x, y)`; `player_id` is whose deed
    /// or whom it concerns, if anyone's.
    Distortion {
        what: crate::distortion::DistortionEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    /// The crew found a landmark of the distortion strata (its story beat); `player_id` found
    /// it.
    DistortionLandmarkFound {
        landmark: crate::distortion::DistortionLandmark,
        player_id: PlayerId,
    },
    /// The crew read log `log` (1-based) of the Fathers' expedition; `player_id` opened the
    /// canister (or reached a sealed one that the log before has just opened). Wave 8 story.
    ExpeditionLogFound {
        player_id: PlayerId,
        log: u8,
    },
    /// `player_id` reached the canister of log `log` before the log before it was read: it
    /// stays sealed until then.
    ExpeditionLogSealed {
        player_id: PlayerId,
        log: u8,
    },
    /// Echo `echo` (1-based: a radio echo or ghost recording) played as `player_id`'s rig
    /// reached its depth.
    ExpeditionEcho {
        player_id: PlayerId,
        echo: u8,
    },
    /// `player_id` entered an anomaly room of `kind` for the first time (`first_of_kind`: the
    /// crew had never entered one of that kind).
    AnomalyEntered {
        player_id: PlayerId,
        kind: crate::expedition::anomaly::AnomalyKind,
        first_of_kind: bool,
    },
    // The Hollow World (Wave 9, Descent V; netcode codes 540-559).
    /// Something of the Hollow World happened at tile `(x, y)`; `player_id` is whose deed or
    /// whom it concerns, if anyone's.
    Hollow {
        what: crate::hollow::HollowEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The far Strange Layers (Wave 9 strange-b, Descent VI; netcode codes 580-599).
    /// Something of the far Strange Layers happened at tile `(x, y)`; `player_id` is whose
    /// deed or whom it concerns, if anyone's.
    FarStrange {
        what: crate::far_strange::FarStrangeEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // Deep supplies at work (Wave 10 polish; netcode codes 720-739).
    /// A deep supply was spent at tile `(x, y)` for `player_id`'s rig.
    Supply {
        what: crate::workshop::SupplyEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The builders' buried city (Wave 10 ancients-a, Descent VII; netcode codes 630-649).
    /// Something of the buried city happened at tile `(x, y)`; `player_id` is whose deed or
    /// whom it concerns, if anyone's; `detail` is the event's number (a stop, a line id, a
    /// chart's index, a landmark's index, an amount; see [`crate::ancient_city::CityEvent`]).
    AncientCity {
        what: crate::ancient_city::CityEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
        detail: u16,
    },
    // Descent VII part B (Wave 10 ancients-b; netcode codes 650-669).
    /// Something of the Sleeping Garrison, the Seal Gate or the Colossus's fight happened at
    /// tile `(x, y)`; `player_id` is whose deed or whom it concerns, if anyone's.
    Garrison {
        what: crate::garrison::GarrisonEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The Heart (Wave 10, Descent VIII; netcode codes 670-699).
    /// Something of Descent VIII happened at tile `(x, y)`; `player_id` is whose deed or whom
    /// it concerns, if anyone's.
    Heart {
        what: crate::heart::HeartEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    /// The crew found one of Descent VIII's landmarks (its story card shows; netcode 699).
    HeartLandmarkFound {
        player_id: PlayerId,
        landmark: crate::heart::HeartLandmark,
    },
    // The story's finale (Wave 10 story; netcode codes 700-719).
    /// The crew's progress through the Descent changed: a Descent reached, a gate boss beaten,
    /// or the Heart woken and the cable lit (the ending). `player_id` is whose rig did it, if
    /// known.
    Story {
        what: crate::expedition::progress::StoryEvent,
        player_id: Option<PlayerId>,
    },
    // Places (Wave 11 regions; netcode code 740).
    /// A player arrived in another place (F11, `WorldState::move_to_place`): landed on a body,
    /// took off into space, or came home.
    PlaceChanged {
        player_id: PlayerId,
        from: crate::place::Place,
        to: crate::place::Place,
    },
    // Round bodies (Wave 11 polar track; netcode codes 780-799).
    /// Something of a round body's ground happened at tile `(x, y)` (a touchdown and how it
    /// went, a tipped craft righted); `player_id` is whose craft.
    Planet {
        what: crate::planet::landing::PlanetEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // Space flight (Wave 11 flight; netcode codes 800-819).
    /// Something happened to `player_id`'s rig in flight; `detail` is the event's number (air
    /// percent, a refused warp's [`crate::flight::WarpRefusal`] code, damage, ...).
    Flight {
        what: crate::flight::FlightEvent,
        player_id: PlayerId,
        detail: u32,
    },
    // Space's things (Wave 12 systems; netcode codes 860-879).
    /// A star, black hole, wormhole, nebula, asteroid or comet did something to `player_id`'s
    /// rig; `detail` is the event's number (seconds to a flare, an ore, a fee, ...).
    SpaceSystems {
        what: crate::flight::systems::SystemsEvent,
        player_id: PlayerId,
        detail: u32,
    },
    // Living bodies (Wave 12 bodylife; netcode codes 880-899).
    /// Something of a body's life reached `player_id` (its weather turned, dawn, dusk, a sale
    /// at a body's market); `detail` is the event's number (the new weather id, credits).
    BodyLife {
        what: crate::world::BodyLifeEvent,
        player_id: PlayerId,
        detail: u32,
    },
    // The wild worlds (Wave 12 planets-a; netcode codes 900-919).
    /// Something of a jungle, ocean archipelago or glacier world happened at tile `(x, y)`
    /// (a puffcap burst, lifesap mended, an icicle cracked, the cold bit, a storm rose, ...);
    /// `player_id` is whose rig, if anyone's.
    Wilds {
        what: crate::planet::wilds::WildsEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The frontier worlds (Wave 12 planets-b; netcode codes 920-939).
    /// Something happened on a dune, volcanic or crystal world at tile `(x, y)` of its ground
    /// (sand slid, a floor gave way, glass or crystal shattered, a vault opened, a cone
    /// rumbled or erupted, a bomb landed, a storm rose); `player_id` is whose doing, if
    /// anyone's.
    Frontier {
        what: crate::frontier::FrontierEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // Friendly aliens (Wave 12 aliens track; netcode codes 940-959).
    /// Something happened between `player_id` and `species` (first contact, words learned, a
    /// trade, a chapter, a home); `detail` is the event's number (see
    /// [`crate::alien::AlienEvent`]).
    Alien {
        what: crate::alien::AlienEvent,
        player_id: PlayerId,
        species: crate::alien::AlienSpecies,
        detail: u32,
    },
    // Space stations (Wave 12 stations; netcode codes 960-979).
    /// Something happened at a station or to a mission of `player_id`'s; `detail` is the
    /// event's number (credits, a tier, a [`crate::station::StationRefusal`] code, ...).
    Station {
        what: crate::station::StationEvent,
        player_id: PlayerId,
        detail: u32,
    },
    // Hostile factions and space combat (Wave 12 hostiles; netcode codes 980-999).
    /// Something happened in a fight in space or a raid on a body; `player_id` is whose rig
    /// (if any), `detail` the event's number (a faction, a class, damage, credits, ...).
    Hostile {
        what: crate::hostiles::HostileEvent,
        player_id: Option<PlayerId>,
        detail: u32,
    },
    // The outland worlds (Wave 13 planets-c; netcode codes 1000-1019).
    /// Something happened on a fungal, machine or dead world at tile `(x, y)` of its ground (a
    /// cap bounced a rig, mycelium regrew, a glowcap flashed, a vent blasted, the gears
    /// clanked, a relay flipped, a vault or a tomb opened, a ghost light chilled a hull, a
    /// theme boss woke, called its brood or fell); `player_id` is whose rig, if anyone's.
    Outlands {
        what: crate::planet::outlands::OutlandsEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The exotic worlds (Wave 13 planets-d; netcode codes 1020-1039).
    /// Something happened on a gas giant, locked or ring world at tile `(x, y)` of its ground
    /// (a bolt, a hailstone or meteor, a geyser, a quake, a vent, a conduit, a hazard, a boss's
    /// tell or fall, a relic, a docking); `player_id` is whose rig, if anyone's.
    Exotic {
        what: crate::exotic::ExoticEvent,
        player_id: Option<PlayerId>,
        x: i32,
        y: i32,
    },
    // The endgame (Wave 13 endgame; netcode codes 1060-1079).
    /// Something happened in the endgame: a Works order, an estate, the gate, a world landed
    /// on, a quarry down; `player_id` is whose (if anyone's), `detail` the event's number (see
    /// [`crate::endgame::EndgameEvent`]).
    Endgame {
        what: crate::endgame::EndgameEvent,
        player_id: Option<PlayerId>,
        detail: u32,
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
    /// The store whose pass raised the event (Wave 13 QA: events are routed by place). An
    /// event of a body's or free flight's store with no player attached reaches only the
    /// seats and clients in that store; the home pass's events (`Home`, world-wide news
    /// included) reach everyone as before.
    #[serde(default)]
    pub store: crate::place::StoreKey,
}

impl GameplayEvent {
    /// Whether this event reaches a viewer in `viewer_store`: an event with a player attached
    /// is that player's (callers route it), a home-pass event reaches everyone, and a
    /// player-less event of another store reaches only viewers in that store.
    #[must_use]
    pub fn reaches_store(&self, has_player: bool, viewer_store: crate::place::StoreKey) -> bool {
        has_player || self.store.is_home() || self.store == viewer_store
    }
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
    /// Every tile write this tick in a store other than the home world's, tagged with its
    /// store (F11; the op log of bodies' ground).
    pub store_terrain_ops: Vec<(crate::place::StoreKey, crate::terrain::TileChange)>,
}

impl TickOutput {
    /// Appends a later pass's output (F11: one pass per store): events keep their order and
    /// are numbered on from this output's.
    pub(crate) fn merge_pass(&mut self, other: Self) {
        let Self {
            tick: _,
            events,
            action_outcomes,
            terrain_ops,
            discoveries,
            store_terrain_ops,
        } = other;
        for event in events {
            self.push_event(event.kind);
            if let Some(last) = self.events.last_mut() {
                last.store = event.store;
            }
        }
        self.action_outcomes.extend(action_outcomes);
        self.terrain_ops.extend(terrain_ops);
        self.discoveries.extend(discoveries);
        self.store_terrain_ops.extend(store_terrain_ops);
    }

    pub(crate) fn push_event(&mut self, kind: GameplayEventKind) {
        let ordinal = u32::try_from(self.events.len()).unwrap_or(u32::MAX);
        self.events.push(GameplayEvent {
            id: GameplayEventId {
                tick: self.tick,
                ordinal,
            },
            kind,
            store: crate::place::StoreKey::Home,
        });
    }

    /// Tags the events from index `from` on with the store `store_of` names for each (Wave 13
    /// QA: events are routed by place).
    pub(crate) fn stamp_stores(
        &mut self,
        from: usize,
        mut store_of: impl FnMut(&GameplayEventKind) -> crate::place::StoreKey,
    ) {
        for event in self.events.iter_mut().skip(from) {
            event.store = store_of(&event.kind);
        }
    }
}

pub(crate) const fn gameplay_event_for_applied_command(
    player_id: PlayerId,
    command: &PlayerCommand,
) -> Option<GameplayEventKind> {
    Some(match command {
        // The pilot's exit / board and pack swaps report their own events (with what moved).
        // The colony reports its own event too (`ColonyServiceUsed`, with the offer).
        PlayerCommand::TogglePilot
        | PlayerCommand::PilotPack { .. }
        | PlayerCommand::Moon(_)
        | PlayerCommand::Pilot(_)
        | PlayerCommand::Tomb(_)
        | PlayerCommand::Logistics(_)
        | PlayerCommand::Flight(_)
        | PlayerCommand::Alien(_)
        | PlayerCommand::Combat(_) => {
            return None;
        }
        PlayerCommand::Service(crate::service::SessionServiceRequest::CustomizeRig { .. }) => {
            GameplayEventKind::RigCustomized { player_id }
        }
        PlayerCommand::Service(crate::service::SessionServiceRequest::SkyBuoy { .. }) => {
            GameplayEventKind::BuoyRefuelled { player_id }
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
        PlayerCommand::People(_) => GameplayEventKind::PeopleInteraction { player_id },
        PlayerCommand::Home(_) => GameplayEventKind::HomeInteraction { player_id },
        PlayerCommand::Build(crate::base::BuildCommand::Use { structure, service }) => {
            GameplayEventKind::BaseServiceUsed {
                player_id,
                structure: *structure,
                service: *service,
            }
        }
    })
}
