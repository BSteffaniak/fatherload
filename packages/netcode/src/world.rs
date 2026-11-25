//! [`NetSimulation`] for the real gameplay world, [`fatherload_simulation::WorldState`].
//!
//! * Controls are the quantized held movement controls ([`ControlState`]: horizontal axis,
//!   thrust, drill).
//! * Actions are the one-shot [`PlayerCommand`]s: bombs, scanner, infrastructure, and the
//!   service/economy transactions. Economy transactions are not predicted (plan §8).
//! * Events carry every [`GameplayEvent`] unchanged, plus one [`WorldEvent::ActionResolved`]
//!   per submitted action so every action's fate is observable exactly once (an ignored action,
//!   e.g. a purchase without credits, produces no gameplay event).
//! * Replication maps one-to-one onto the world's replication sections: per-tick
//!   [`PlayerTickState`] / [`LiveEntities`] (bombs and id counters), the entity store as
//!   incremental per-client deltas ([`Entity`] records by [`EntityId`] and revision), the op log ([`WorldOp`]: terrain [`TileChange`]s
//!   and per-player [`Discovery`] reveals) and on-change [`SectionKey`] / [`SectionPayload`]
//!   sections. The keyframe is the world itself, whose terrain serializes as generation
//!   settings plus the tiles that differ from generation (the client regenerates the rest and
//!   checks the terrain digest); a chunk refresh is that chunk's [`TileDiffs`] the same way.
//! * A player's loadout section, discovery ops and action results go only to that player's
//!   client; other
//!   clients get the redacted keyframe and compare the client-visible state hash
//!   ([`WorldState::visible_state_hash`]).
//! * Poses are in tiles, so the netcode's distance tunables (snap threshold, correction epsilon,
//!   relevance radius) are expressed in tiles. A pose's origin is the world's frame origin in
//!   tiles (F7, [`frame_origin`]): `x`/`y` are the frame-local position, quantized relative to
//!   the frame's sector, and relevance compares poses and chunks through exact origin deltas,
//!   so it holds however far from the world origin (and from each other) players are.

use fatherload_simulation::{
    ControlState, CrewMember, Discovery, GameplayEvent, GameplayEventKind, InputSequence,
    LiveEntities, PlayerCommand, PlayerScopedCommandOutcome, PlayerTickState, SectionKey,
    SectionPayload, SimulationTick, TILE_SIZE, TickAction, TileChange, WorldState,
    entity::{Entity, EntityDelta, EntityId},
    input,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, TileDiffs},
};
use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, PlayerId, ProfileId, Tick};
use crate::simulation::{NetSimulation, Pose, Relevance, TickEvent, TickInputs, TickOutput};

/// Event kind discriminants for [`TickEvent::kind`].
pub mod event_kind {
    /// Offset added to a gameplay event's variant index.
    pub const GAMEPLAY_BASE: u32 = 1;
    /// [`super::WorldEvent::ActionResolved`].
    pub const ACTION_RESOLVED: u32 = 100;
}

/// How a submitted action was resolved by the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionResolution {
    /// The command changed the world.
    Applied,
    /// The command was valid but had no effect (e.g. not enough credits, no bombs left).
    Ignored,
}

/// Events produced by the real world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldEvent {
    /// A gameplay event (audio/vfx cue, notification).
    Gameplay(GameplayEvent),
    /// One submitted action and its outcome.
    ActionResolved {
        /// The action's identity.
        id: ActionId,
        /// The command.
        command: PlayerCommand,
        /// What happened.
        resolution: ActionResolution,
    },
}

/// One op-log entry of the real world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldOp {
    /// A terrain tile write (shared).
    Tile(TileChange),
    /// Tiles one player newly discovered (only that player's map).
    Discovery(Discovery),
}

/// Converts a netcode player id to the simulation's.
#[must_use]
pub fn to_sim_player(id: PlayerId) -> fatherload_simulation::PlayerId {
    fatherload_simulation::PlayerId::new(u64::from(id.0))
}

/// Converts a simulation player id to netcode's (`None` if it does not fit).
#[must_use]
pub fn from_sim_player(id: fatherload_simulation::PlayerId) -> Option<PlayerId> {
    u32::try_from(id.get()).ok().map(PlayerId)
}

/// Converts a netcode profile id to the simulation's.
#[must_use]
pub const fn to_sim_profile(id: ProfileId) -> fatherload_simulation::ProfileId {
    fatherload_simulation::ProfileId::new(id.0)
}

/// Converts a simulation profile id to netcode's.
#[must_use]
pub const fn from_sim_profile(id: fatherload_simulation::ProfileId) -> ProfileId {
    ProfileId(id.get())
}

/// Builds held controls from analog input.
#[must_use]
pub fn controls(horizontal: f32, thrust: bool, drill_down: bool) -> ControlState {
    ControlState::new(horizontal, thrust, drill_down)
}

#[allow(clippy::too_many_lines, reason = "one arm per gameplay event kind")]
const fn gameplay_kind(kind: &GameplayEventKind) -> (u32, Option<fatherload_simulation::PlayerId>) {
    match *kind {
        GameplayEventKind::DrillStarted { player_id } => (0, Some(player_id)),
        GameplayEventKind::PurchaseCompleted { player_id } => (1, Some(player_id)),
        GameplayEventKind::CargoChanged { player_id } => (2, Some(player_id)),
        GameplayEventKind::BombPlaced { player_id } => (3, Some(player_id)),
        GameplayEventKind::InfrastructurePlaced { player_id } => (4, Some(player_id)),
        GameplayEventKind::ScannerUsed { player_id } => (5, Some(player_id)),
        GameplayEventKind::EscapeStarted { player_id } => (6, Some(player_id)),
        GameplayEventKind::EscapeTunnelChanged => (7, None),
        GameplayEventKind::DeepRewardGranted { player_id } => (8, Some(player_id)),
        GameplayEventKind::RescueTriggered { player_id } => (9, Some(player_id)),
        GameplayEventKind::PlayerDamaged { player_id } => (10, Some(player_id)),
        GameplayEventKind::HazardChanged => (11, None),
        GameplayEventKind::BouldersChanged => (12, None),
        GameplayEventKind::BombDetonated { .. } => (13, None),
        GameplayEventKind::BombBounced { .. } => (32, None),
        GameplayEventKind::ExplosiveDeployed { player_id, .. } => (33, Some(player_id)),
        GameplayEventKind::RemoteDetonation { player_id } => (34, Some(player_id)),
        GameplayEventKind::ChargeStuck { .. } => (35, None),
        GameplayEventKind::ExplosiveDetonated { .. } => (36, None),
        GameplayEventKind::ClusterSplit { .. } => (37, None),
        GameplayEventKind::FireSpread { .. } => (38, None),
        GameplayEventKind::PoiOpened { player_id, .. } => (14, Some(player_id)),
        GameplayEventKind::PoiDiscovered { player_id, .. } => (15, Some(player_id)),
        GameplayEventKind::VaultOpened { .. } => (16, None),
        GameplayEventKind::VaultLockReset { .. } => (17, None),
        GameplayEventKind::TestModeChanged { player_id } => (18, Some(player_id)),
        GameplayEventKind::QuestAccepted { player_id, .. } => (19, Some(player_id)),
        GameplayEventKind::QuestStageDone { player_id, .. } => (20, Some(player_id)),
        GameplayEventKind::QuestReady { player_id, .. } => (21, Some(player_id)),
        GameplayEventKind::QuestCompleted { player_id, .. } => (22, Some(player_id)),
        GameplayEventKind::QuestFailed { player_id, .. } => (23, Some(player_id)),
        GameplayEventKind::BlueprintCompleted { player_id, .. } => (24, Some(player_id)),
        GameplayEventKind::RigCustomized { player_id } => (44, Some(player_id)),
        // WP-DROPS (60+: clear of the other wave-2 tracks' codes).
        GameplayEventKind::ItemsCollected { player_id, .. } => (60, Some(player_id)),
        GameplayEventKind::PickupBlocked { player_id, .. } => (61, Some(player_id)),
        GameplayEventKind::BlocksDumped { player_id } => (62, Some(player_id)),
        GameplayEventKind::CollectBlocksToggled { player_id, .. } => (63, Some(player_id)),
        GameplayEventKind::CaveDiscovered { player_id, .. } => (70, Some(player_id)),
        GameplayEventKind::CaveCacheOpened { player_id, .. } => (71, Some(player_id)),
        GameplayEventKind::StalactiteFell { .. } => (72, None),
        GameplayEventKind::VillageDiscovered { player_id, .. } => (45, Some(player_id)),
        GameplayEventKind::VillageServiceUsed { player_id, .. } => (46, Some(player_id)),
        // Enemies (T10): 80-87.
        GameplayEventKind::EnemyAlerted { .. } => (80, None),
        GameplayEventKind::EnemyWindup { .. } => (81, None),
        GameplayEventKind::EnemyFired { .. } => (82, None),
        GameplayEventKind::EnemyHurt { .. } => (83, None),
        GameplayEventKind::EnemyKilled { .. } => (84, None),
        GameplayEventKind::EnemyStunned { .. } => (85, None),
        GameplayEventKind::RigStruck { player_id, .. } => (86, Some(player_id)),
        GameplayEventKind::EnemyDifficultyChanged { player_id, .. } => (87, Some(player_id)),
        // Weapons (wave 2, T11): 90-95.
        GameplayEventKind::WeaponFired { player_id, .. } => (90, Some(player_id)),
        GameplayEventKind::WeaponDry { player_id, .. } => (91, Some(player_id)),
        GameplayEventKind::WeaponMounted { player_id, .. } => (92, Some(player_id)),
        GameplayEventKind::ProjectileImpact { .. } => (93, None),
        GameplayEventKind::EntityDamaged { .. } => (94, None),
        GameplayEventKind::EntityDestroyed { .. } => (95, None),
        // Base building (WP-BASE): 100-102.
        GameplayEventKind::BasePlaced { player_id, .. } => (100, Some(player_id)),
        GameplayEventKind::BaseRemoved { player_id, .. } => (101, Some(player_id)),
        GameplayEventKind::BaseServiceUsed { player_id, .. } => (102, Some(player_id)),
        // World clock (wave 3, T14): 110-115.
        GameplayEventKind::Dawn { .. } => (110, None),
        GameplayEventKind::Dusk { .. } => (111, None),
        GameplayEventKind::NewDay { .. } => (112, None),
        GameplayEventKind::NightSkipped { .. } => (113, None),
        GameplayEventKind::GotUp { player_id } => (114, Some(player_id)),
        GameplayEventKind::SleepRuleChanged { player_id, .. } => (115, Some(player_id)),
        // Altitude (T16, V8 skies): 116-118.
        GameplayEventKind::SkyBandEntered { player_id, .. } => (116, Some(player_id)),
        GameplayEventKind::UpdraftCaught { player_id } => (117, Some(player_id)),
        GameplayEventKind::RigIcing { player_id } => (118, Some(player_id)),
        // Sky isles (wave 4, T20): 200-203.
        GameplayEventKind::SkyCacheOpened { player_id, .. } => (200, Some(player_id)),
        GameplayEventKind::BuoyRefuelled { player_id } => (201, Some(player_id)),
        GameplayEventKind::IsleCollapsing { .. } => (202, None),
        GameplayEventKind::JetStreamCaught { player_id } => (203, Some(player_id)),
        // The pilot on foot (Part II F5): 120-133.
        GameplayEventKind::PilotExited { player_id, .. } => (120, Some(player_id)),
        GameplayEventKind::PilotBoarded { player_id, .. } => (121, Some(player_id)),
        GameplayEventKind::PilotJumped { player_id } => (122, Some(player_id)),
        GameplayEventKind::PilotLanded { player_id, .. } => (123, Some(player_id)),
        GameplayEventKind::PilotLedgeGrab { player_id } => (124, Some(player_id)),
        GameplayEventKind::PilotSplashed { player_id } => (125, Some(player_id)),
        GameplayEventKind::PilotHurt { player_id, .. } => (126, Some(player_id)),
        GameplayEventKind::PilotDied { player_id, .. } => (127, Some(player_id)),
        GameplayEventKind::PilotOxygenLow { player_id, .. } => (128, Some(player_id)),
        GameplayEventKind::JetpackIgnited { player_id } => (129, Some(player_id)),
        GameplayEventKind::JetpackEmpty { player_id } => (130, Some(player_id)),
        GameplayEventKind::PickStruck { player_id, .. } => (131, Some(player_id)),
        GameplayEventKind::PickClinked { player_id } => (132, Some(player_id)),
        GameplayEventKind::PackTransferred { player_id, .. } => (133, Some(player_id)),
        // People in the world (WP-PEOPLE, wave 3 T17): 140-145.
        GameplayEventKind::PersonTalked { player_id, .. } => (140, Some(player_id)),
        GameplayEventKind::PersonTraded { player_id, .. } => (141, Some(player_id)),
        GameplayEventKind::PersonGifted { player_id, .. } => (142, Some(player_id)),
        GameplayEventKind::AffinityChanged { player_id, .. } => (143, Some(player_id)),
        GameplayEventKind::RumourMarked { player_id, .. } => (144, Some(player_id)),
        GameplayEventKind::PeopleInteraction { player_id } => (145, Some(player_id)),
        // Story arcs (WP-STORY, wave 4 T26): 260-265.
        GameplayEventKind::ArcOffered { player_id, .. } => (260, Some(player_id)),
        GameplayEventKind::ArcAccepted { player_id, .. } => (261, Some(player_id)),
        GameplayEventKind::ArcStepDone { player_id, .. } => (262, Some(player_id)),
        GameplayEventKind::ArcClimax { player_id, .. } => (263, Some(player_id)),
        GameplayEventKind::ArcCompleted { player_id, .. } => (264, Some(player_id)),
        GameplayEventKind::ArcAbandoned { player_id, .. } => (265, Some(player_id)),
        // Upgrade reward moments (wave 3, T18): 180-181.
        GameplayEventKind::UpgradeInstalled { player_id, .. } => (180, Some(player_id)),
        GameplayEventKind::MilestoneEarned { player_id, .. } => (181, Some(player_id)),
        // Sky continents, sky enemies and Act II weapons (wave 4, T21): 210-219.
        GameplayEventKind::LightningArc { struck, .. } => (210, struck),
        GameplayEventKind::ArcChained { player_id, .. } => (211, Some(player_id)),
        GameplayEventKind::GravityThrow { player_id, .. } => (212, Some(player_id)),
        GameplayEventKind::TetherAttached { player_id, .. } => (213, Some(player_id)),
        GameplayEventKind::TetherReleased { player_id } => (214, Some(player_id)),
        GameplayEventKind::FlakBurst { player_id, .. } => (215, Some(player_id)),
        GameplayEventKind::TempleLeverPulled { player_id, .. } => (216, Some(player_id)),
        GameplayEventKind::TempleGateOpened { player_id, .. } => (217, Some(player_id)),
        GameplayEventKind::RelicChestOpened { player_id, .. } => (218, Some(player_id)),
        GameplayEventKind::SkyEnemyAction { .. } => (219, None),
        // The Moon (wave 4, T22 WP-MOON): 220-229.
        GameplayEventKind::MoonVacuumBreach { player_id, .. } => (220, Some(player_id)),
        GameplayEventKind::MoonLanded { player_id } => (221, Some(player_id)),
        GameplayEventKind::ColonyServiceUsed { player_id, .. } => (222, Some(player_id)),
        GameplayEventKind::FacilityPylon { player_id, .. } => (223, Some(player_id)),
        GameplayEventKind::FacilityGateOpened { player_id } => (224, Some(player_id)),
        GameplayEventKind::WardenAwakened { .. } => (225, None),
        GameplayEventKind::WardenStaggered { .. } => (226, None),
        GameplayEventKind::WardenDefeated { by } => (227, by),
        GameplayEventKind::MoonCoreReached { player_id } => (228, Some(player_id)),
        GameplayEventKind::UplinkBeamed { player_id, .. } => (229, Some(player_id)),
        // WP-PILOT (wave 4, T23): 230-239.
        GameplayEventKind::PilotFired { player_id, .. } => (230, Some(player_id)),
        GameplayEventKind::PilotWeaponDry { player_id, .. } => (231, Some(player_id)),
        GameplayEventKind::PilotWeaponSwapped { player_id, .. } => (232, Some(player_id)),
        GameplayEventKind::PilotRope { player_id, .. } => (233, Some(player_id)),
        GameplayEventKind::PilotGlide { player_id, .. } => (234, Some(player_id)),
        GameplayEventKind::MechanismUsed { player_id, .. } => (235, player_id),
        GameplayEventKind::ChestOpened { player_id, .. } => (236, Some(player_id)),
        GameplayEventKind::RigRecall { player_id, .. } => (237, Some(player_id)),
        GameplayEventKind::PilotExposed { player_id, .. } => (238, Some(player_id)),
        GameplayEventKind::PilotWallJumped { player_id } => (239, Some(player_id)),
        // Contractor-built homes (WP-HOME, wave 4 T24): 240-249.
        GameplayEventKind::PlotBought { player_id, .. } => (240, Some(player_id)),
        GameplayEventKind::ContractSigned { player_id, .. } => (241, Some(player_id)),
        GameplayEventKind::ConstructionStage { owner, .. } => (242, Some(owner)),
        GameplayEventKind::HomeCompleted { owner, .. } => (243, Some(owner)),
        GameplayEventKind::SiteEventRaised { owner, .. } => (244, Some(owner)),
        GameplayEventKind::SiteEventAnswered { player_id, .. } => (245, Some(player_id)),
        GameplayEventKind::FurniturePlaced { player_id, .. } => (246, Some(player_id)),
        GameplayEventKind::FurnitureRemoved { player_id, .. } => (247, Some(player_id)),
        GameplayEventKind::HomeRushed { player_id, .. } => (248, Some(player_id)),
        GameplayEventKind::HomeInteraction { player_id } => (249, Some(player_id)),
        // Night-time gameplay (wave 4, T25 WP-NIGHT): 250-253.
        GameplayEventKind::NightFell { .. } => (250, None),
        GameplayEventKind::NightSurvived { .. } => (251, None),
        GameplayEventKind::RareNightSpawn { .. } => (252, None),
        GameplayEventKind::NightMonsterBurrowed { .. } => (253, None),
        // Tombs and crypts (wave 5, T27 WP-TOMBS): 270-279.
        GameplayEventKind::TombEntered { player_id, .. } => (270, Some(player_id)),
        GameplayEventKind::TombSeal { player_id, .. } => (271, player_id),
        GameplayEventKind::TombAmbush { .. } => (272, None),
        GameplayEventKind::TombGuardianDefeated { by } => (273, by),
        GameplayEventKind::TombFloorCleared { player_id, .. } => (274, Some(player_id)),
        GameplayEventKind::TombRelicPicked { player_id, .. } => (275, Some(player_id)),
        GameplayEventKind::TombShrineRested { player_id, .. } => (276, Some(player_id)),
        GameplayEventKind::TombCacheOpened { player_id, .. } => (277, Some(player_id)),
        GameplayEventKind::TombMemberDowned { player_id, .. } => (278, Some(player_id)),
        GameplayEventKind::TombRunEnded { player_id, .. } => (279, Some(player_id)),
        // Side-view tombs (wave 5, T28): 280-289.
        GameplayEventKind::TombTrapSprung { .. } => (280, None),
        GameplayEventKind::TombGuardianPhase { .. } => (281, None),
        // Top-down crypts (wave 5, T29 WP-TOMBS): 290-296.
        GameplayEventKind::CryptRoll { player_id, .. } => (290, Some(player_id)),
        GameplayEventKind::CryptTrap { .. } => (291, None),
        GameplayEventKind::CryptPitFall { player_id, .. } => (292, Some(player_id)),
        GameplayEventKind::CryptLordPhase { .. } => (293, None),
        GameplayEventKind::CryptTell { .. } => (294, None),
        GameplayEventKind::CryptSlam { .. } => (295, None),
        GameplayEventKind::CryptSummon { .. } => (296, None),
        // Fluids (Part III F9, wave 7): 380-389.
        GameplayEventKind::FluidBreached { .. } => (380, None),
        GameplayEventKind::FluidReacted { .. } => (381, None),
        GameplayEventKind::RigSubmerged { player_id, .. } => (382, Some(player_id)),
        GameplayEventKind::RigSurfaced { player_id, .. } => (383, Some(player_id)),
        // Depth logistics (wave 7, Part III F10): 390-399.
        GameplayEventKind::LiftRideStarted { player_id, .. } => (390, Some(player_id)),
        GameplayEventKind::LiftArrived { player_id, .. } => (391, Some(player_id)),
        GameplayEventKind::LiftRestored { player_id, .. } => (392, Some(player_id)),
        GameplayEventKind::LogisticsPlaced { player_id, .. } => (393, Some(player_id)),
        GameplayEventKind::OutpostReached { player_id, .. } => (394, Some(player_id)),
        GameplayEventKind::OutpostClaimed { player_id, .. } => (395, Some(player_id)),
        GameplayEventKind::OutpostServiced { player_id, .. } => (396, Some(player_id)),
        GameplayEventKind::BeaconTravelled { player_id, .. } => (397, Some(player_id)),
        GameplayEventKind::DiveChargeFired { player_id, .. } => (398, Some(player_id)),
        GameplayEventKind::DescentGateBlocked { player_id, .. } => (399, Some(player_id)),
        // The authored Crust (wave 8): 400-419.
        GameplayEventKind::CartBoarded { player_id } => (400, Some(player_id)),
        GameplayEventKind::CartSwitched { player_id } => (401, Some(player_id)),
        GameplayEventKind::CartLeft { player_id, .. } => (402, Some(player_id)),
        GameplayEventKind::SupportCreaked { .. } => (403, None),
        GameplayEventKind::SupportCollapsed { .. } => (404, None),
        GameplayEventKind::SlateShudder { .. } => (405, None),
        GameplayEventKind::SlateSlid { .. } => (406, None),
        GameplayEventKind::StrongboxOpened { player_id, .. } => (407, Some(player_id)),
        GameplayEventKind::SinkholeRumble { player_id } => (408, Some(player_id)),
        GameplayEventKind::SinkholeEscaped { player_id } => (409, Some(player_id)),
        GameplayEventKind::SinkholeCaught { player_id } => (410, Some(player_id)),
        GameplayEventKind::SinkholeReset { player_id } => (411, Some(player_id)),
        GameplayEventKind::DeepClaimGranted { player_id } => (412, Some(player_id)),
        GameplayEventKind::DeepClaimMissing { player_id } => (413, Some(player_id)),
        GameplayEventKind::LavaTubeEntered { player_id } => (414, Some(player_id)),
        GameplayEventKind::CrustCreature { .. } => (415, None),
        // Descent II, the Old Deep (wave 8): 420-439.
        GameplayEventKind::ResonanceChain { player_id, .. } => (420, Some(player_id)),
        GameplayEventKind::FossilBoneFreed { player_id, .. } => (421, Some(player_id)),
        GameplayEventKind::SkeletonCompleted { player_id, .. } => (422, Some(player_id)),
        GameplayEventKind::GasTideTurned { .. } => (423, None),
        GameplayEventKind::AmberStuck { player_id } => (424, Some(player_id)),
        GameplayEventKind::CathedralValveOpened { player_id, .. } => (425, Some(player_id)),
        GameplayEventKind::CathedralLiftRestored { player_id } => (426, Some(player_id)),
        GameplayEventKind::DoseWarning { player_id, .. } => (427, Some(player_id)),
        GameplayEventKind::MutationGained { player_id, .. } => (428, Some(player_id)),
        GameplayEventKind::MachineryWoke { .. } => (429, None),
        GameplayEventKind::PistonStruck { player_id } => (430, Some(player_id)),
        GameplayEventKind::HeatStormChanged { .. } => (431, None),
        GameplayEventKind::LavaPoured { .. } => (432, None),
        GameplayEventKind::DeepLogRead { player_id, .. } => (433, Some(player_id)),
        GameplayEventKind::StarCoreAwoke => (434, None),
        GameplayEventKind::StarCorePhase { .. } => (435, None),
        GameplayEventKind::StarCoreBeaten { player_id } => (436, Some(player_id)),
        GameplayEventKind::GlowmotherAwoke { .. } => (437, None),
        GameplayEventKind::GlowmotherDefeated { player_id } => (438, Some(player_id)),
        // The Living Dark (wave 8, Descent III): 440-459.
        GameplayEventKind::LivingDark {
            what, player_id, ..
        } => (440 + what.code() as u32, player_id),
        // The Fathers' expedition and anomaly rooms (wave 8 story): 490-493.
        GameplayEventKind::ExpeditionLogFound { player_id, .. } => (490, Some(player_id)),
        GameplayEventKind::ExpeditionLogSealed { player_id, .. } => (491, Some(player_id)),
        GameplayEventKind::ExpeditionEcho { player_id, .. } => (492, Some(player_id)),
        GameplayEventKind::AnomalyEntered { player_id, .. } => (493, Some(player_id)),
        // The Drowned and Frozen (wave 9, Descent IV): 520-539.
        GameplayEventKind::Drowned {
            what, player_id, ..
        } => (520 + what.code() as u32, player_id),
        // The Hollow World (wave 9, Descent V): 540-559.
        GameplayEventKind::Hollow {
            what, player_id, ..
        } => (540 + what.code() as u32, player_id),
        // The distortion strata (wave 9, Descent VI part A): 560-579.
        GameplayEventKind::Distortion {
            what, player_id, ..
        } => (560 + what.code() as u32, player_id),
        GameplayEventKind::DistortionLandmarkFound { player_id, .. } => (579, Some(player_id)),
        // The far Strange Layers (wave 9 strange-b, Descent VI): 580-599.
        GameplayEventKind::FarStrange {
            what, player_id, ..
        } => (580 + what.code() as u32, player_id),
        // Deep supplies at work (wave 10 polish): 720-739.
        GameplayEventKind::Supply {
            what, player_id, ..
        } => (720 + what.code() as u32, player_id),
        // The builders' buried city (wave 10 ancients-a, Descent VII): 630-649.
        GameplayEventKind::AncientCity {
            what, player_id, ..
        } => (630 + what.code() as u32, player_id),
    }
}

/// The tile offset of `world`'s frame origin: the [`Pose::origin`] of its players (F7).
#[must_use]
pub const fn frame_origin(world: &WorldState) -> [i64; 2] {
    let frame = world.frame();
    [frame.origin_col(), frame.origin_row()]
}

/// [`Pose::mode`] of a player on foot (drill modes are `(y << 16 ^ x) + 1`; this high bit
/// pattern is never one of theirs in practice).
pub const ON_FOOT_POSE_MODE: u32 = 0xF00F_0001;

const fn drill_mode(target: fatherload_simulation::terrain::TilePosition) -> u32 {
    // Distinct non-zero code per target tile (zero means "not drilling").
    ((target.y.cast_unsigned() << 16) ^ (target.x.cast_unsigned() & 0xFFFF)).wrapping_add(1)
}

impl NetSimulation for WorldState {
    type Controls = ControlState;
    type Action = PlayerCommand;
    type Event = WorldEvent;
    type PlayerState = PlayerTickState;
    type Entities = LiveEntities;
    type EntityKey = EntityId;
    type EntityRecord = Entity;
    type Op = WorldOp;
    type Chunk = ChunkPosition;
    type ChunkSnapshot = TileDiffs;
    type SectionKey = SectionKey;
    type Section = SectionPayload;
    type Keyframe = Self;

    fn tick(&self) -> Tick {
        Tick(self.simulation_tick().get())
    }

    fn set_tick(&mut self, tick: Tick) {
        self.set_simulation_tick(SimulationTick::new(tick.0));
    }

    fn tick_rate_hz(&self) -> Option<u32> {
        Some(Self::tick_rate_hz(self))
    }

    /// The enemy difficulty is a session setting: only the host (and its local seats) may
    /// change it, whatever a client sends.
    fn host_only_action(action: &PlayerCommand) -> bool {
        matches!(action, PlayerCommand::SetEnemyDifficulty(_))
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        Self::player_ids(self).filter_map(from_sim_player).collect()
    }

    fn add_player(&mut self, id: PlayerId) {
        Self::add_player(self, to_sim_player(id));
    }

    fn remove_player(&mut self, id: PlayerId) {
        Self::remove_player(self, to_sim_player(id));
    }

    fn identify_player(
        &mut self,
        id: PlayerId,
        profile: Option<ProfileId>,
        name: &str,
        seat: Option<u32>,
    ) {
        self.set_crew_member(
            to_sim_player(id),
            CrewMember {
                profile: profile.map(to_sim_profile),
                name: name.to_owned(),
                seat,
            },
        );
    }

    fn crew_record(&self, id: PlayerId) -> Option<Option<ProfileId>> {
        self.crew_member(to_sim_player(id))
            .map(|member| member.profile.map(from_sim_profile))
    }

    fn player_for_profile(&self, profile: ProfileId) -> Option<PlayerId> {
        Self::player_for_profile(self, to_sim_profile(profile)).and_then(from_sim_player)
    }

    fn park_player(&mut self, id: PlayerId) -> bool {
        Self::park_player(self, to_sim_player(id))
    }

    fn unpark_player(&mut self, id: PlayerId) -> bool {
        Self::unpark_player(self, to_sim_player(id))
    }

    fn parked_players(&self) -> Vec<(PlayerId, String)> {
        Self::parked_players(self)
            .filter_map(|(id, parked)| {
                from_sim_player(id).map(|net| (net, parked.member.display_name(id)))
            })
            .collect()
    }

    fn simulate_tick(
        &mut self,
        inputs: &TickInputs<ControlState, PlayerCommand>,
    ) -> TickOutput<WorldEvent, WorldOp> {
        debug_assert_eq!(
            self.simulation_tick().get().saturating_add(1),
            inputs.tick.0,
            "inputs target the next tick"
        );
        let current = self.simulation_tick();
        let mut sim_inputs = fatherload_simulation::TickInputs::default();
        // Per player: the ids of the actions handed to the world, indexed by sequence.
        let mut submitted: Vec<(fatherload_simulation::PlayerId, Vec<ActionId>)> = Vec::new();
        for player in Self::player_ids(self) {
            let Some(net_player) = from_sim_player(player) else {
                continue;
            };
            let input = inputs.players.get(&net_player);
            let held = input.map(|input| input.controls).unwrap_or_default();
            let mut ids = Vec::new();
            let actions = input
                .map(|input| {
                    input
                        .actions
                        .iter()
                        .map(|envelope| {
                            let sequence =
                                InputSequence::new(u32::try_from(ids.len()).unwrap_or(u32::MAX));
                            ids.push(envelope.id);
                            TickAction {
                                sequence,
                                command: envelope.action.clone(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            sim_inputs.players.insert(
                player,
                input::PlayerTickInput {
                    controls: Some(held),
                    actions,
                },
            );
            submitted.push((player, ids));
        }

        let output = Self::simulate_tick(self, current, &sim_inputs);

        let mut events = Vec::with_capacity(output.events.len() + output.action_outcomes.len());
        for outcome in &output.action_outcomes {
            let Some(id) = submitted
                .iter()
                .find(|(player, _)| *player == outcome.player_id)
                .and_then(|(_, ids)| {
                    usize::try_from(outcome.sequence.get())
                        .ok()
                        .and_then(|index| ids.get(index))
                })
                .copied()
            else {
                continue;
            };
            let command = sim_inputs
                .players
                .get(&outcome.player_id)
                .and_then(|input| {
                    input
                        .actions
                        .iter()
                        .find(|action| action.sequence == outcome.sequence)
                })
                .map(|action| action.command.clone());
            let Some(command) = command else {
                continue;
            };
            events.push(TickEvent {
                player: from_sim_player(outcome.player_id),
                kind: event_kind::ACTION_RESOLVED,
                payload: WorldEvent::ActionResolved {
                    id,
                    command,
                    resolution: if outcome.outcome == PlayerScopedCommandOutcome::Applied {
                        ActionResolution::Applied
                    } else {
                        ActionResolution::Ignored
                    },
                },
            });
        }
        for event in output.events {
            let (index, player) = gameplay_kind(&event.kind);
            events.push(TickEvent {
                player: player.and_then(from_sim_player),
                kind: event_kind::GAMEPLAY_BASE + index,
                payload: WorldEvent::Gameplay(event),
            });
        }
        let mut ops: Vec<WorldOp> = output.terrain_ops.into_iter().map(WorldOp::Tile).collect();
        ops.extend(output.discoveries.into_iter().map(WorldOp::Discovery));
        TickOutput { events, ops }
    }

    fn predicts_action(action: &PlayerCommand) -> bool {
        matches!(
            action,
            PlayerCommand::PlaceBomb
                | PlayerCommand::DeployExplosive { .. }
                | PlayerCommand::RemoteDetonate { .. }
                | PlayerCommand::UseScanner
                | PlayerCommand::PlaceInfrastructure { .. }
                | PlayerCommand::MountWeapon { .. }
                | PlayerCommand::TogglePilot
        )
    }

    fn player_pose(&self, id: PlayerId) -> Option<Pose> {
        let sim_id = to_sim_player(id);
        // On foot the pose is the pilot's (relevance and smoothing follow the pilot); getting
        // in or out changes the mode, so the client snaps instead of smoothing across bodies.
        // The mode is one constant (not the entity id: a predicted pilot may get a different id
        // than the host's until the authority arrives, and that is no reason to snap).
        let origin = frame_origin(self);
        if self.is_on_foot(sim_id) {
            let (x, y) = self.actor_position(sim_id)?;
            return Some(Pose::at(
                origin,
                x / TILE_SIZE,
                y / TILE_SIZE,
                ON_FOOT_POSE_MODE,
            ));
        }
        let player = self.player(sim_id)?;
        let mode = self
            .active_drill(sim_id)
            .map_or(0, |drill| drill_mode(drill.target));
        Some(Pose::at(
            origin,
            player.x / TILE_SIZE,
            player.y / TILE_SIZE,
            mode,
        ))
    }

    fn player_state(&self, id: PlayerId) -> Option<PlayerTickState> {
        self.player_tick_state(to_sim_player(id))
    }

    fn apply_player_state(&mut self, id: PlayerId, state: PlayerTickState) {
        self.apply_player_tick_state(to_sim_player(id), state);
    }

    fn op_relevant(&self, op: &WorldOp, relevance: &Relevance) -> bool {
        match op {
            WorldOp::Tile(_) => true,
            // Discovery is private to its player.
            WorldOp::Discovery(discovery) => relevance
                .viewer
                .is_some_and(|viewer| to_sim_player(viewer) == discovery.player_id),
        }
    }

    fn entities(&self, _relevance: &Relevance) -> LiveEntities {
        // Charges are entities now (relevance-filtered entity deltas); only counters remain.
        self.live_entities()
    }

    fn apply_entities(&mut self, entities: LiveEntities) {
        self.apply_live_entities(entities);
    }

    fn entity_revisions(&self, relevance: &Relevance) -> Vec<(EntityId, u64)> {
        if relevance.is_infinite() {
            return Self::entity_revisions(self, None, None);
        }
        // The centre in this world's frame (tiles from the frame origin).
        let center = relevance.center.map(|(x, y)| {
            let local = Pose::at(relevance.origin, x, y, 0).rebased(frame_origin(self));
            (local.x, local.y)
        });
        Self::entity_revisions(self, center, relevance.radius)
    }

    fn entity_record(&self, key: &EntityId) -> Option<Entity> {
        Self::entity_record(self, *key)
    }

    fn apply_entity_delta(&mut self, updates: Vec<Entity>, despawned: Vec<EntityId>) {
        Self::apply_entity_delta(self, EntityDelta { updates, despawned });
    }

    fn op_chunk(&self, op: &WorldOp) -> Option<ChunkPosition> {
        match op {
            WorldOp::Tile(change) => Some(ChunkPosition::of(change.position)),
            WorldOp::Discovery(_) => None,
        }
    }

    fn chunk_relevant(&self, chunk: &ChunkPosition, relevance: &Relevance) -> bool {
        let origin = chunk.origin();
        // Relative to the relevance origin, exactly (players far from the world origin).
        #[allow(
            clippy::cast_precision_loss,
            reason = "chunks near the viewer are a few tiles from its origin"
        )]
        let (left, top, size) = (
            (i64::from(origin.x) - relevance.origin[0]) as f32,
            (i64::from(origin.y) - relevance.origin[1]) as f32,
            CHUNK_SIZE_TILES as f32,
        );
        relevance.touches_box(left, top, left + size, top + size)
    }

    fn chunk_snapshot(&self, chunk: &ChunkPosition) -> Option<TileDiffs> {
        Some(self.terrain_chunk(*chunk))
    }

    fn apply_chunk_snapshot(&mut self, chunk: &ChunkPosition, snapshot: TileDiffs) {
        self.apply_terrain_chunk(*chunk, &snapshot);
    }

    fn apply_op(&mut self, op: &WorldOp) {
        match op {
            WorldOp::Tile(change) => {
                let _applied = self.apply_tile_change(change);
            }
            WorldOp::Discovery(discovery) => self.apply_discovery(discovery),
        }
    }

    fn section_keys(&self) -> Vec<SectionKey> {
        Self::section_keys(self)
    }

    fn section_hash(&self, key: &SectionKey) -> Option<u64> {
        Self::section_hash(self, *key)
    }

    fn section(&self, key: &SectionKey) -> Option<SectionPayload> {
        Self::section(self, *key)
    }

    fn apply_section(&mut self, key: &SectionKey, section: SectionPayload) {
        Self::apply_section(self, *key, section);
    }

    fn remove_section(&mut self, key: &SectionKey) {
        Self::remove_section(self, *key);
    }

    fn section_visible(&self, key: &SectionKey, viewer: Option<PlayerId>) -> bool {
        key.visible_to(viewer.map(to_sim_player))
    }

    fn event_visible(
        &self,
        event: &WorldEvent,
        player: Option<PlayerId>,
        viewer: Option<PlayerId>,
    ) -> bool {
        match event {
            WorldEvent::Gameplay(_) => true,
            // An action result names the command (a purchase, a loan, ...) and whether the
            // player could afford it: private to the player who submitted it.
            WorldEvent::ActionResolved { .. } => player.is_some() && player == viewer,
        }
    }

    fn keyframe(&self) -> Self {
        self.clone()
    }

    fn keyframe_for(&self, viewer: Option<PlayerId>) -> Self {
        self.redacted_for(viewer.map(to_sim_player))
    }

    fn from_keyframe(keyframe: Self) -> Self {
        keyframe
    }

    fn state_hash(&self) -> u64 {
        self.simulation_state_hash().get()
    }

    fn visible_state_hash(&self, viewer: Option<PlayerId>) -> u64 {
        Self::visible_state_hash(self, viewer.map(to_sim_player)).get()
    }
}
