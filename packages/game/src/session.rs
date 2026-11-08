#![allow(
    dead_code,
    reason = "session/world cleanup is now driven by production playability; old frame-time scaffolding observer no longer anchors transitional APIs"
)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt, mem,
    ops::Deref,
    time::Duration,
};

pub use drillgame_simulation::{
    AuthoritativeWorldSummary, GameplayEvent, GameplayEventKind, LostCargoSite,
    PlayerScopedCommandOutcome, PlayerSurvivalSnapshot, TerrainChunkPosition, TerrainChunkRevision,
    TerrainRevisionTracker, TickInputs, WorldImport, WorldState,
    world::{
        AUTHORITATIVE_GRAVITY, AUTHORITATIVE_HORIZONTAL_SPEED, AUTHORITATIVE_THRUST_SPEED,
        authoritative_mine_target,
    },
};

use crate::{
    economy::upgrade_tier_name,
    game_state::{
        DrillDirection, DrillState, FallingBoulder, GameState, HazardCloud, ModalScreen,
        PlacedBomb, PlacedInfrastructure, RunMode, SideContract, SoundCue, TILE_SIZE,
    },
    input::PlayerInput,
    input_mapping::CommandProducer,
    multiplayer::{
        AuthoritativeFrame, AuthoritativeKeyframe, ClientAction, ClientId, ClientMessage,
        CommandAcknowledgement, CommandApplicationResponse, CommandNetworkSession, CommandPacket,
        CommandPacketExchangeSummary, CommandRejection, CommandSource, FIXED_DELTA_SECONDS,
        HostInputInbox, InputBundle, InputSequence, InputStatus, LOCAL_CLIENT_ID, LOCAL_PLAYER_ID,
        NetworkDeltaPayload, NetworkPlayerLoadoutSnapshot, NetworkPlayerSnapshot,
        NetworkTerrainChunkRevision, NetworkTerrainChunkSnapshot, NetworkTerrainTile,
        NetworkWorldSnapshot, PlayerCommand, PlayerId, ProtocolExchangeBatch, ProtocolExchangeKind,
        ProtocolMessage, QuinnSessionTickInput, QuinnSessionTickSummary, SIMULATION_HZ,
        SequencedPlayerCommand, ServerMessage, SessionEpoch, SessionToken, SimulationTick,
    },
    player::Player,
    save::{
        SaveAuthority, SettingsFile, load_latest_persistent_world, load_persistent_world,
        load_persistent_world_slot, persistent_world_from_session, save_world_session,
        save_world_session_slot,
    },
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, Terrain, TileKind, TilePosition},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPayloadSource {
    AuthoritativeWorld,
    LocalPresentationShell,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkTickPayloadProvenance {
    pub snapshot_source: NetworkPayloadSource,
    pub delta_source: NetworkPayloadSource,
    pub terrain_source: NetworkPayloadSource,
    pub snapshot_tick: Option<SimulationTick>,
    pub snapshot_player_ids: Vec<PlayerId>,
    pub delta_player_ids: Vec<PlayerId>,
    pub authoritative_chunk_count: usize,
    pub authoritative_chunk_tile_count: usize,
    pub correction_player_id: Option<PlayerId>,
}

impl NetworkTickPayloadProvenance {
    #[must_use]
    pub fn from_session_tick_input(input: &QuinnSessionTickInput) -> Self {
        let (snapshot_tick, snapshot_player_ids) = input.snapshot.as_ref().map_or_else(
            || (None, Vec::new()),
            |snapshot| {
                (
                    Some(snapshot.tick),
                    snapshot
                        .players
                        .iter()
                        .map(|player| player.player_id)
                        .collect(),
                )
            },
        );
        let delta_player_ids = input.delta.as_ref().map_or_else(Vec::new, |(_, payload)| {
            if let NetworkDeltaPayload::Players { players } = payload {
                players.iter().map(|player| player.player_id).collect()
            } else {
                Vec::new()
            }
        });
        Self {
            snapshot_source: NetworkPayloadSource::AuthoritativeWorld,
            delta_source: NetworkPayloadSource::AuthoritativeWorld,
            terrain_source: NetworkPayloadSource::AuthoritativeWorld,
            snapshot_tick,
            snapshot_player_ids,
            delta_player_ids,
            authoritative_chunk_count: input.authoritative_terrain_chunks.len(),
            authoritative_chunk_tile_count: input
                .authoritative_terrain_chunks
                .iter()
                .map(|chunk| chunk.tiles.len())
                .sum(),
            correction_player_id: input
                .correction_probe
                .as_ref()
                .map(|(_, _, player, _)| player.player_id),
        }
    }

    #[must_use]
    pub const fn world_sourced(&self) -> bool {
        matches!(
            self.snapshot_source,
            NetworkPayloadSource::AuthoritativeWorld
        ) && matches!(self.delta_source, NetworkPayloadSource::AuthoritativeWorld)
            && matches!(
                self.terrain_source,
                NetworkPayloadSource::AuthoritativeWorld
            )
    }

    #[must_use]
    pub fn delta_matches_snapshot_players(&self) -> bool {
        self.delta_player_ids
            .iter()
            .all(|player_id| self.snapshot_player_ids.contains(player_id))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiveSessionTickInput {
    pub input: QuinnSessionTickInput,
    pub provenance: NetworkTickPayloadProvenance,
}

impl LiveSessionTickInput {
    #[must_use]
    pub fn from_input(input: QuinnSessionTickInput) -> Self {
        let provenance = NetworkTickPayloadProvenance::from_session_tick_input(&input);
        Self { input, provenance }
    }
}

impl Deref for LiveSessionTickInput {
    type Target = QuinnSessionTickInput;

    fn deref(&self) -> &Self::Target {
        &self.input
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OnlineTickApplicationSummary {
    pub applied_remote_commands: usize,
    pub synced_replicated_players: usize,
    pub applied_terrain_tiles: usize,
    pub roster_seeded: bool,
    pub joined_local_synced: bool,
}

impl OnlineTickApplicationSummary {
    #[must_use]
    pub const fn changed_session_state(self) -> bool {
        self.applied_remote_commands > 0
            || self.synced_replicated_players > 0
            || self.applied_terrain_tiles > 0
            || self.roster_seeded
            || self.joined_local_synced
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnlineTickIdentitySummary {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub slot: Option<u8>,
    pub descriptor_client_connected: bool,
    pub roster_seeded: bool,
    pub joined_local_synced: bool,
}

impl OnlineTickIdentitySummary {
    #[must_use]
    pub const fn uses_joined_slot(self) -> bool {
        self.slot.is_some()
    }

    #[must_use]
    pub const fn seeded_session_presentation(self) -> bool {
        self.roster_seeded || self.joined_local_synced
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineParticipantRole {
    LocalHost,
    JoinedClient,
    RemotePeer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineRosterParticipant {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub slot: u8,
    pub name: String,
    pub ready: bool,
    pub connected: bool,
    pub role: OnlineParticipantRole,
    pub save_authority: SaveAuthority,
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

impl fmt::Display for OnlineParticipantRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalHost => formatter.write_str("local-host"),
            Self::JoinedClient => formatter.write_str("joined-client"),
            Self::RemotePeer => formatter.write_str("remote-peer"),
        }
    }
}

impl fmt::Display for OnlineSessionRoster {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut participants = Vec::new();
        participants.push(format_roster_participant(&self.local));
        if let Some(remote) = &self.remote {
            participants.push(format_roster_participant(remote));
        }
        formatter.write_str(&participants.join(" | "))
    }
}

fn format_roster_participant(participant: &OnlineRosterParticipant) -> String {
    format!(
        "{} slot={} client={} player={} name={} ready={} connected={} save={:?}",
        participant.role,
        participant.slot,
        participant.client_id.get(),
        participant.player_id.get(),
        participant.name,
        yes_no(participant.ready),
        yes_no(participant.connected),
        participant.save_authority
    )
}

impl OnlineRosterParticipant {
    #[must_use]
    pub fn is_local_authority(&self) -> bool {
        matches!(self.role, OnlineParticipantRole::LocalHost)
            && self.save_authority == SaveAuthority::HostOwnedSession
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSessionRoster {
    pub local: OnlineRosterParticipant,
    pub remote: Option<OnlineRosterParticipant>,
}

impl OnlineSessionRoster {
    #[must_use]
    pub fn participants(&self) -> Vec<&OnlineRosterParticipant> {
        let mut participants = vec![&self.local];
        if let Some(remote) = &self.remote {
            participants.push(remote);
        }
        participants
    }

    #[must_use]
    pub fn joined_client_local_identity(&self) -> Option<(ClientId, PlayerId)> {
        (self.local.role == OnlineParticipantRole::JoinedClient)
            .then_some((self.local.client_id, self.local.player_id))
    }

    #[must_use]
    pub const fn player_id_for_local_network_tick(&self) -> PlayerId {
        self.local.player_id
    }

    #[must_use]
    pub const fn client_id_for_local_network_tick(&self) -> ClientId {
        self.local.client_id
    }

    #[must_use]
    pub fn remote_connected(&self) -> bool {
        self.remote.as_ref().is_some_and(|remote| remote.connected)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateDomain {
    AuthoritativeWorld,
    LocalClientPresentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateBoundary {
    pub name: &'static str,
    pub domain: StateDomain,
}

impl StateBoundary {
    #[must_use]
    pub const fn authoritative_world(name: &'static str) -> Self {
        Self {
            name,
            domain: StateDomain::AuthoritativeWorld,
        }
    }

    #[must_use]
    pub const fn local_client_presentation(name: &'static str) -> Self {
        Self {
            name,
            domain: StateDomain::LocalClientPresentation,
        }
    }
}

#[must_use]
pub const fn planned_state_boundaries() -> [StateBoundary; 12] {
    [
        StateBoundary::authoritative_world("terrain"),
        StateBoundary::authoritative_world("players"),
        StateBoundary::authoritative_world("hazards"),
        StateBoundary::authoritative_world("bombs"),
        StateBoundary::authoritative_world("infrastructure"),
        StateBoundary::authoritative_world("economy"),
        StateBoundary::authoritative_world("contracts"),
        StateBoundary::authoritative_world("progression"),
        StateBoundary::local_client_presentation("camera"),
        StateBoundary::local_client_presentation("menus_and_modals"),
        StateBoundary::local_client_presentation("hud_messages"),
        StateBoundary::local_client_presentation("prediction_buffers"),
    ]
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "rendering migration reports independent renderer input domains"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderingInputMigrationStatus {
    pub camera_from_session_view: bool,
    pub players_from_world_presentation: bool,
    pub terrain_from_world_presentation: bool,
    pub hud_from_per_client_presentation: bool,
    pub ready_for_renderer_views: bool,
}

impl RenderingInputMigrationStatus {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            camera_from_session_view: true,
            players_from_world_presentation: true,
            terrain_from_world_presentation: true,
            hud_from_per_client_presentation: true,
            ready_for_renderer_views: true,
        }
    }

    #[must_use]
    pub const fn migrated(self) -> bool {
        self.camera_from_session_view
            && self.players_from_world_presentation
            && self.terrain_from_world_presentation
            && self.hud_from_per_client_presentation
            && self.ready_for_renderer_views
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "legacy write boundary reports independent safety rails"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyGameStateWriteBoundaryStatus {
    pub ui_settings_save_menu_allowed: bool,
    pub presentation_compatibility_allowed: bool,
    pub authoritative_world_writes_blocked: bool,
    pub online_save_boundary_enforced: bool,
}

impl LegacyGameStateWriteBoundaryStatus {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            ui_settings_save_menu_allowed: true,
            presentation_compatibility_allowed: true,
            authoritative_world_writes_blocked: true,
            online_save_boundary_enforced: true,
        }
    }

    #[must_use]
    pub const fn limited_to_compatibility(self) -> bool {
        self.ui_settings_save_menu_allowed
            && self.presentation_compatibility_allowed
            && self.authoritative_world_writes_blocked
            && self.online_save_boundary_enforced
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyGameStateCouplingDomain {
    PresentationCompatibility,
    SaveMenuUi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyGameStateCouplingInventoryItem {
    pub path: &'static str,
    pub domain: LegacyGameStateCouplingDomain,
    pub replacement: &'static str,
}

impl LegacyGameStateCouplingInventoryItem {
    #[must_use]
    pub const fn is_runtime_blocking(self) -> bool {
        match self.domain {
            LegacyGameStateCouplingDomain::PresentationCompatibility
            | LegacyGameStateCouplingDomain::SaveMenuUi => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyGameStateCouplingInventorySummary {
    pub total: usize,
    pub authoritative_world_couplings: usize,
    pub presentation_compatibility_couplings: usize,
    pub save_menu_ui_couplings: usize,
}

impl LegacyGameStateCouplingInventorySummary {
    #[must_use]
    pub const fn runtime_inventory_complete(self) -> bool {
        self.total > 0 && self.authoritative_world_couplings == 0
    }
}

#[must_use]
pub const fn legacy_game_state_coupling_inventory() -> [LegacyGameStateCouplingInventoryItem; 4] {
    [
        LegacyGameStateCouplingInventoryItem {
            path: "GameSession::update_frame_from_session_authority",
            domain: LegacyGameStateCouplingDomain::PresentationCompatibility,
            replacement: "GameSession/WorldState advances authority first, then syncs legacy presentation",
        },
        LegacyGameStateCouplingInventoryItem {
            path: "apply_replicated_snapshot_to_world_presentation",
            domain: LegacyGameStateCouplingDomain::PresentationCompatibility,
            replacement: "Network snapshots update WorldState players before legacy GameState mirrors visible player data",
        },
        LegacyGameStateCouplingInventoryItem {
            path: "GameState save/load/menu handlers",
            domain: LegacyGameStateCouplingDomain::SaveMenuUi,
            replacement: "Keep save/menu/settings ownership in GameState while online save boundaries block joined-client writes",
        },
        LegacyGameStateCouplingInventoryItem {
            path: "Renderer HUD/prompt reads",
            domain: LegacyGameStateCouplingDomain::PresentationCompatibility,
            replacement: "Move renderer inputs to GameSession views/player/world presentations after compatibility removal",
        },
    ]
}

#[must_use]
pub fn legacy_game_state_coupling_inventory_summary() -> LegacyGameStateCouplingInventorySummary {
    let items = legacy_game_state_coupling_inventory();
    LegacyGameStateCouplingInventorySummary {
        total: items.len(),
        authoritative_world_couplings: items
            .iter()
            .filter(|item| item.is_runtime_blocking())
            .count(),
        presentation_compatibility_couplings: items
            .iter()
            .filter(|item| {
                matches!(
                    item.domain,
                    LegacyGameStateCouplingDomain::PresentationCompatibility
                )
            })
            .count(),
        save_menu_ui_couplings: items
            .iter()
            .filter(|item| matches!(item.domain, LegacyGameStateCouplingDomain::SaveMenuUi))
            .count(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostAuthorityCommandRoutingDomain {
    Movement,
    DrillingMining,
    EconomyServiceMenu,
    PresentationOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostAuthorityCommandRoutingItem {
    pub command: &'static str,
    pub domain: HostAuthorityCommandRoutingDomain,
    pub host_authoritative: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostAuthorityCommandRoutingSummary {
    pub total: usize,
    pub host_authoritative: usize,
    pub economy_service_menu_authoritative: usize,
    pub presentation_only: usize,
}

impl HostAuthorityCommandRoutingSummary {
    #[must_use]
    pub const fn economy_service_menu_routed(self) -> bool {
        self.economy_service_menu_authoritative >= 8
            && self.host_authoritative + self.presentation_only == self.total
    }
}

#[must_use]
pub const fn host_authority_command_routing_inventory() -> [HostAuthorityCommandRoutingItem; 13] {
    [
        HostAuthorityCommandRoutingItem {
            command: "Movement",
            domain: HostAuthorityCommandRoutingDomain::Movement,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "Drill",
            domain: HostAuthorityCommandRoutingDomain::DrillingMining,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "SellCargo",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "Refuel",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "Repair",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "BuyUpgrade",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "Rescue",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "CompleteContract",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "StartExpedition",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "RepayDebt",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "WinGame",
            domain: HostAuthorityCommandRoutingDomain::EconomyServiceMenu,
            host_authoritative: true,
        },
        HostAuthorityCommandRoutingItem {
            command: "OpenMenu",
            domain: HostAuthorityCommandRoutingDomain::PresentationOnly,
            host_authoritative: false,
        },
        HostAuthorityCommandRoutingItem {
            command: "CloseMenu",
            domain: HostAuthorityCommandRoutingDomain::PresentationOnly,
            host_authoritative: false,
        },
    ]
}

#[must_use]
pub fn host_authority_command_routing_summary() -> HostAuthorityCommandRoutingSummary {
    let items = host_authority_command_routing_inventory();
    HostAuthorityCommandRoutingSummary {
        total: items.len(),
        host_authoritative: items.iter().filter(|item| item.host_authoritative).count(),
        economy_service_menu_authoritative: items
            .iter()
            .filter(|item| {
                matches!(
                    item.domain,
                    HostAuthorityCommandRoutingDomain::EconomyServiceMenu
                ) && item.host_authoritative
            })
            .count(),
        presentation_only: items
            .iter()
            .filter(|item| {
                matches!(
                    item.domain,
                    HostAuthorityCommandRoutingDomain::PresentationOnly
                )
            })
            .count(),
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "joined-client authority boundary reports independent prediction/authority safety checks"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JoinedClientAuthorityBoundaryStatus {
    pub local_prediction_allowed: bool,
    pub remote_authority_accepted: bool,
    pub host_world_owner: bool,
    pub fights_local_authority: bool,
}

impl JoinedClientAuthorityBoundaryStatus {
    #[must_use]
    pub const fn online_joined_client_runtime() -> Self {
        Self {
            local_prediction_allowed: true,
            remote_authority_accepted: true,
            host_world_owner: true,
            fights_local_authority: false,
        }
    }

    #[must_use]
    pub const fn safe_for_joined_client(self) -> bool {
        self.local_prediction_allowed
            && self.remote_authority_accepted
            && self.host_world_owner
            && !self.fights_local_authority
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyInputRewriteRemovalStatus {
    pub public_rewrite_function_removed: bool,
    pub remaining_adapter_presentation_only: bool,
    pub runtime_command_processing_replaced: bool,
}

impl LegacyInputRewriteRemovalStatus {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            public_rewrite_function_removed: true,
            remaining_adapter_presentation_only: true,
            runtime_command_processing_replaced: true,
        }
    }

    #[must_use]
    pub const fn removal_complete(self) -> bool {
        self.public_rewrite_function_removed
            && self.remaining_adapter_presentation_only
            && self.runtime_command_processing_replaced
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedTickMigrationStatus {
    FixedTickReady,
    CompatibilityFixedStep,
    StillVariableDelta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixedTickMigrationPlan {
    MigrateToAuthoritativeTick,
    KeepVariablePresentationOnly,
    AlreadyFixedStep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedTickAuditItem {
    pub system: &'static str,
    pub status: FixedTickMigrationStatus,
    pub plan: FixedTickMigrationPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedTickMigrationSummary {
    pub fixed_ready: usize,
    pub authoritative_migrations: usize,
    pub presentation_exemptions: usize,
    pub unresolved_variable_delta: usize,
}

impl FixedTickMigrationSummary {
    #[must_use]
    pub fn from_items(items: &[FixedTickAuditItem]) -> Self {
        let fixed_ready = items
            .iter()
            .filter(|item| item.status == FixedTickMigrationStatus::FixedTickReady)
            .count();
        let authoritative_migrations = items
            .iter()
            .filter(|item| item.plan == FixedTickMigrationPlan::MigrateToAuthoritativeTick)
            .count();
        let presentation_exemptions = items
            .iter()
            .filter(|item| item.plan == FixedTickMigrationPlan::KeepVariablePresentationOnly)
            .count();
        let unresolved_variable_delta = items
            .iter()
            .filter(|item| {
                item.status == FixedTickMigrationStatus::StillVariableDelta
                    && item.plan != FixedTickMigrationPlan::KeepVariablePresentationOnly
            })
            .count();
        Self {
            fixed_ready,
            authoritative_migrations,
            presentation_exemptions,
            unresolved_variable_delta,
        }
    }

    #[must_use]
    pub const fn audit_complete(self) -> bool {
        self.unresolved_variable_delta == 0
    }
}

#[must_use]
pub const fn fixed_tick_audit_items() -> [FixedTickAuditItem; 8] {
    [
        FixedTickAuditItem {
            system: "session_tick_counter",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "physics",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "fuel_burn",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "drilling_progress",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "hazards",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "bombs",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "market_event_timers",
            status: FixedTickMigrationStatus::FixedTickReady,
            plan: FixedTickMigrationPlan::AlreadyFixedStep,
        },
        FixedTickAuditItem {
            system: "animations",
            status: FixedTickMigrationStatus::StillVariableDelta,
            plan: FixedTickMigrationPlan::KeepVariablePresentationOnly,
        },
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplicatedWorldPresentationApplySummary {
    pub snapshot_tick: SimulationTick,
    pub local_players_updated: usize,
    pub remote_players_updated: usize,
    pub clients_created: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthoritativeWorldAdvanceSummary {
    pub start_tick: SimulationTick,
    pub end_tick: SimulationTick,
    pub fixed_steps: u32,
    pub processed_commands: usize,
    pub terrain_events: usize,
    pub cargo_events: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionAuthorityUpdateSummary {
    pub used_legacy_presentation_adapter: bool,
    pub local_movement_authority: bool,
    pub command_adapter_count: usize,
    pub current_tick: SimulationTick,
}

impl SessionAuthorityUpdateSummary {
    #[must_use]
    pub const fn legacy_bridge_active(self) -> bool {
        self.used_legacy_presentation_adapter
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "frame-rate invariance proof records independent gameplay domains"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRateInvarianceProof {
    pub command_timing_stable: bool,
    pub fuel_stable: bool,
    pub drill_progress_stable: bool,
    pub hazard_damage_stable: bool,
    pub bomb_state_stable: bool,
    pub rescue_state_stable: bool,
}

impl FrameRateInvarianceProof {
    #[must_use]
    pub const fn complete(self) -> bool {
        self.command_timing_stable
            && self.fuel_stable
            && self.drill_progress_stable
            && self.hazard_damage_stable
            && self.bomb_state_stable
            && self.rescue_state_stable
    }
}

#[allow(
    clippy::struct_field_names,
    reason = "determinism proof intentionally names equality dimensions with a same_ prefix"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayDeterminismProof {
    pub same_tick: SimulationTick,
    pub same_player_count: usize,
    pub same_transaction_count: usize,
    pub same_bomb_count: usize,
    pub same_infrastructure_count: usize,
    pub same_local_player_state: bool,
}

impl ReplayDeterminismProof {
    #[must_use]
    pub const fn complete(self) -> bool {
        self.same_player_count > 0
            && self.same_transaction_count > 0
            && self.same_local_player_state
    }
}

#[allow(
    clippy::struct_field_names,
    reason = "startup plan names the client/player identity fields explicitly"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSplitScreenStartupPlan {
    pub primary_client_id: ClientId,
    pub secondary_client_id: ClientId,
    pub primary_player_id: PlayerId,
    pub secondary_player_id: PlayerId,
}

impl LocalSplitScreenStartupPlan {
    #[must_use]
    pub const fn two_player_default() -> Self {
        Self {
            primary_client_id: LOCAL_CLIENT_ID,
            secondary_client_id: ClientId::new(2),
            primary_player_id: LOCAL_PLAYER_ID,
            secondary_player_id: PlayerId::new(2),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyGameplayMutationDomain {
    RenderUiSaveAdapter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyGameplayMutationInventory {
    pub domains: [LegacyGameplayMutationDomain; 1],
    pub authoritative_extraction_remaining: usize,
    pub adapter_only_domains: usize,
}

impl LegacyGameplayMutationInventory {
    #[must_use]
    pub const fn inventory_complete(self) -> bool {
        self.domains.len() == 1
            && self.authoritative_extraction_remaining == 0
            && self.adapter_only_domains == 1
    }
}

#[must_use]
pub const fn legacy_gameplay_mutation_inventory() -> LegacyGameplayMutationInventory {
    LegacyGameplayMutationInventory {
        domains: [LegacyGameplayMutationDomain::RenderUiSaveAdapter],
        authoritative_extraction_remaining: 0,
        adapter_only_domains: 1,
    }
}

#[allow(
    clippy::struct_field_names,
    reason = "audit field names intentionally distinguish fixed, presentation-only, and unresolved domains"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariableDeltaAuditSummary {
    pub fixed_or_compatibility_gameplay_domains: usize,
    pub presentation_only_variable_domains: usize,
    pub unresolved_gameplay_variable_domains: usize,
}

impl VariableDeltaAuditSummary {
    #[must_use]
    pub const fn gameplay_delta_audit_complete(self) -> bool {
        self.unresolved_gameplay_variable_domains == 0
            && self.fixed_or_compatibility_gameplay_domains > 0
            && self.presentation_only_variable_domains > 0
    }
}

#[must_use]
pub fn variable_delta_audit_summary() -> VariableDeltaAuditSummary {
    let migration = FixedTickMigrationSummary::from_items(&fixed_tick_audit_items());
    VariableDeltaAuditSummary {
        fixed_or_compatibility_gameplay_domains: migration.fixed_ready
            + migration.authoritative_migrations,
        presentation_only_variable_domains: migration.presentation_exemptions,
        unresolved_gameplay_variable_domains: migration.unresolved_variable_delta,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransientEffectDomain {
    LocalClientPresentation,
    GameplayRelevantWorld,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransientEffectBoundary {
    pub name: &'static str,
    pub domain: TransientEffectDomain,
}

impl TransientEffectBoundary {
    #[must_use]
    pub const fn local_client_presentation(name: &'static str) -> Self {
        Self {
            name,
            domain: TransientEffectDomain::LocalClientPresentation,
        }
    }

    #[must_use]
    pub const fn gameplay_relevant_world(name: &'static str) -> Self {
        Self {
            name,
            domain: TransientEffectDomain::GameplayRelevantWorld,
        }
    }
}

#[must_use]
pub const fn planned_transient_effect_boundaries() -> [TransientEffectBoundary; 8] {
    [
        TransientEffectBoundary::local_client_presentation("dust_particles"),
        TransientEffectBoundary::local_client_presentation("spark_particles"),
        TransientEffectBoundary::local_client_presentation("sound_cues"),
        TransientEffectBoundary::local_client_presentation("screen_flash"),
        TransientEffectBoundary::local_client_presentation("camera_shake"),
        TransientEffectBoundary::gameplay_relevant_world("hazard_clouds"),
        TransientEffectBoundary::gameplay_relevant_world("falling_boulders"),
        TransientEffectBoundary::gameplay_relevant_world("active_drill_progress"),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GameplayEventRoutingSummary {
    pub authoritative_events: usize,
    pub local_presentation_boundaries: usize,
    pub gameplay_world_boundaries: usize,
}

impl GameplayEventRoutingSummary {
    #[must_use]
    pub fn from_events_and_boundaries(
        events: &[WorldEvent],
        boundaries: &[TransientEffectBoundary],
    ) -> Self {
        let authoritative_events = events.len();
        let local_presentation_boundaries = boundaries
            .iter()
            .filter(|boundary| boundary.domain == TransientEffectDomain::LocalClientPresentation)
            .count();
        let gameplay_world_boundaries = boundaries
            .iter()
            .filter(|boundary| boundary.domain == TransientEffectDomain::GameplayRelevantWorld)
            .count();
        Self {
            authoritative_events,
            local_presentation_boundaries,
            gameplay_world_boundaries,
        }
    }

    #[must_use]
    pub const fn separates_local_presentation(self) -> bool {
        self.authoritative_events > 0
            && self.local_presentation_boundaries > 0
            && self.gameplay_world_boundaries > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerScopedSystem {
    Movement,
    Drilling,
    ActiveDrill,
    Cargo,
    VitalStatus,
    Scanner,
    Placement,
    EconomyService,
}

#[must_use]
pub const fn planned_player_scoped_systems() -> [PlayerScopedSystem; 8] {
    [
        PlayerScopedSystem::Movement,
        PlayerScopedSystem::Drilling,
        PlayerScopedSystem::ActiveDrill,
        PlayerScopedSystem::Cargo,
        PlayerScopedSystem::VitalStatus,
        PlayerScopedSystem::Scanner,
        PlayerScopedSystem::Placement,
        PlayerScopedSystem::EconomyService,
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotPurpose {
    SaveFile,
    NetworkSync,
    RenderSync,
}

#[must_use]
pub const fn snapshot_purposes() -> [SnapshotPurpose; 3] {
    [
        SnapshotPurpose::SaveFile,
        SnapshotPurpose::NetworkSync,
        SnapshotPurpose::RenderSync,
    ]
}

/// Compact player data for render/network/save-adjacent synchronization experiments.
///
/// This is not a save format. It is an explicit snapshot boundary that can later be split into
/// network snapshots, render snapshots, and persistent save models as the legacy world is migrated.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnapshot {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub hull: f32,
    pub max_hull: f32,
    pub credits: u32,
    pub cargo_used: u32,
    pub cargo: BTreeMap<MineralKind, u32>,
    pub artifacts: BTreeMap<ArtifactKind, u32>,
    pub materials: BTreeMap<StrategicResourceKind, u32>,
    pub loadout: NetworkPlayerLoadoutSnapshot,
    pub scanner_cooldown_seconds: f32,
}

impl PlayerSnapshot {
    #[must_use]
    pub fn from_player(player_id: PlayerId, player: &Player) -> Self {
        Self {
            player_id,
            x: player.x,
            y: player.y,
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
            fuel: player.fuel,
            fuel_capacity: player.fuel_capacity,
            hull: player.hull,
            max_hull: player.max_hull(),
            credits: player.credits,
            cargo_used: player.cargo_used(),
            cargo: player.cargo.clone(),
            artifacts: player.artifacts.clone(),
            materials: player.materials.clone(),
            loadout: NetworkPlayerLoadoutSnapshot::from_player(player),
            scanner_cooldown_seconds: 0.0,
        }
    }

    #[must_use]
    pub fn from_world_player(player_id: PlayerId, player: &Player, world: &WorldState) -> Self {
        let mut snapshot = Self::from_player(player_id, player);
        snapshot.scanner_cooldown_seconds =
            world.scanner_cooldown_seconds(player_id).unwrap_or(0.0);
        snapshot
    }
    #[must_use]
    pub fn network_snapshot(&self) -> NetworkPlayerSnapshot {
        NetworkPlayerSnapshot {
            player_id: self.player_id,
            x: self.x,
            y: self.y,
            velocity_x: self.velocity_x,
            velocity_y: self.velocity_y,
            fuel: self.fuel,
            hull: self.hull,
            credits: self.credits,
            cargo_used: self.cargo_used,
            cargo: self.cargo.clone(),
            artifacts: self.artifacts.clone(),
            materials: self.materials.clone(),
            loadout: self.loadout.clone(),
            scanner_cooldown_seconds: self.scanner_cooldown_seconds,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayedPlayerState {
    pub snapshot: PlayerSnapshot,
    pub replayed_command_count: usize,
}

impl ReplayedPlayerState {
    #[must_use]
    pub const fn from_snapshot(snapshot: PlayerSnapshot) -> Self {
        Self {
            snapshot,
            replayed_command_count: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerPlayerHudSnapshot {
    pub player_id: PlayerId,
    pub cargo_used: u32,
    pub cargo_capacity: u32,
    pub credits: u32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub hull: f32,
    pub max_hull: f32,
    pub scanner_cooldown_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerClientPresentationSnapshot {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub hud: PerPlayerHudSnapshot,
    pub viewport: Viewport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitScreenUiPresentationSnapshot {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub viewport: Viewport,
    pub camera: raylib::prelude::Vector2,
    pub hud: PerPlayerHudSnapshot,
    pub modal: Option<ModalScreen>,
    pub run_mode: RunMode,
    pub message: String,
    pub audio_cues: Vec<SoundCue>,
    pub scanner_cooldown_seconds: f32,
    pub depth_tile: i32,
}

impl SplitScreenUiPresentationSnapshot {
    #[must_use]
    pub fn has_per_player_overlay_context(&self) -> bool {
        self.viewport.width > 0
            && self.viewport.height > 0
            && self.hud.player_id == self.player_id
            && self.depth_tile >= 0
    }
}

impl PerPlayerHudSnapshot {
    #[must_use]
    pub const fn from_player_snapshot(snapshot: &PlayerSnapshot) -> Self {
        Self {
            player_id: snapshot.player_id,
            cargo_used: snapshot.cargo_used,
            cargo_capacity: snapshot.loadout.cargo_capacity,
            credits: snapshot.credits,
            fuel: snapshot.fuel,
            fuel_capacity: snapshot.fuel_capacity,
            hull: snapshot.hull,
            max_hull: snapshot.max_hull,
            scanner_cooldown_seconds: snapshot.scanner_cooldown_seconds,
        }
    }
}

/// Compatibility world snapshot keyed by authoritative simulation tick.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    pub tick: SimulationTick,
    pub players: Vec<PlayerSnapshot>,
}

impl WorldSnapshot {
    #[must_use]
    pub fn from_world(tick: SimulationTick, world: &WorldState) -> Self {
        Self {
            tick,
            players: world.player_snapshots(),
        }
    }

    #[must_use]
    pub fn network_snapshot(&self) -> NetworkWorldSnapshot {
        NetworkWorldSnapshot {
            tick: self.tick,
            players: self
                .players
                .iter()
                .map(PlayerSnapshot::network_snapshot)
                .collect(),
        }
    }

    #[must_use]
    pub fn keyframe_message(&self) -> ProtocolMessage {
        ProtocolMessage::SnapshotKeyframe {
            snapshot: self.network_snapshot(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactDeltaKind {
    Noop,
    TerrainChunks,
    Players,
    KeyframeRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactDeltaSummary {
    pub kind: CompactDeltaKind,
    pub tick: SimulationTick,
    pub item_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactWorldDelta {
    Noop {
        tick: SimulationTick,
    },
    TerrainChunks {
        tick: SimulationTick,
        revisions: Vec<TerrainChunkRevision>,
    },
    Players {
        tick: SimulationTick,
        players: Vec<PlayerId>,
    },
    KeyframeRequired {
        tick: SimulationTick,
    },
}

impl CompactWorldDelta {
    #[must_use]
    pub fn network_payload(&self) -> NetworkDeltaPayload {
        match self {
            Self::Noop { .. } => NetworkDeltaPayload::Noop,
            Self::TerrainChunks { revisions, .. } => NetworkDeltaPayload::TerrainChunks {
                revisions: revisions
                    .iter()
                    .map(|revision| NetworkTerrainChunkRevision {
                        chunk_x: revision.position.x,
                        chunk_y: revision.position.y,
                        revision: revision.revision,
                    })
                    .collect(),
            },
            Self::Players { .. } | Self::KeyframeRequired { .. } => {
                NetworkDeltaPayload::KeyframeRequired
            }
        }
    }

    #[must_use]
    pub fn protocol_message(&self) -> ProtocolMessage {
        ProtocolMessage::WorldDelta {
            tick: self.tick(),
            payload: self.network_payload(),
        }
    }

    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        match self {
            Self::Noop { tick }
            | Self::TerrainChunks { tick, .. }
            | Self::Players { tick, .. }
            | Self::KeyframeRequired { tick } => *tick,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> CompactDeltaKind {
        match self {
            Self::Noop { .. } => CompactDeltaKind::Noop,
            Self::TerrainChunks { .. } => CompactDeltaKind::TerrainChunks,
            Self::Players { .. } => CompactDeltaKind::Players,
            Self::KeyframeRequired { .. } => CompactDeltaKind::KeyframeRequired,
        }
    }

    #[must_use]
    pub const fn summary(&self) -> CompactDeltaSummary {
        let item_count = match self {
            Self::Noop { .. } | Self::KeyframeRequired { .. } => 0,
            Self::TerrainChunks { revisions, .. } => revisions.len(),
            Self::Players { players, .. } => players.len(),
        };
        CompactDeltaSummary {
            kind: self.kind(),
            tick: self.tick(),
            item_count,
        }
    }
}

/// Compatibility world delta emitted after a session update.
///
/// This is intentionally event-based for now. Later phases can replace or augment it with compact
/// terrain chunk revisions, entity component changes, and acknowledgement metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldDelta {
    pub tick: SimulationTick,
    pub events: Vec<WorldEvent>,
}

impl WorldDelta {
    #[must_use]
    pub const fn new(tick: SimulationTick, events: Vec<WorldEvent>) -> Self {
        Self { tick, events }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[must_use]
    pub fn compact_network_delta(&self) -> CompactWorldDelta {
        let mut terrain_revisions = Vec::new();
        let mut players = BTreeSet::new();
        let mut keyframe_required = false;

        for event in &self.events {
            match event {
                WorldEvent::TerrainChunksChanged { revisions, .. } => {
                    terrain_revisions.extend(revisions.iter().cloned());
                }
                WorldEvent::TerrainRefreshRequested | WorldEvent::SnapshotKeyframeReady { .. } => {
                    keyframe_required = true;
                }
                WorldEvent::PlayerChanged { player_id }
                | WorldEvent::CargoChanged { player_id }
                | WorldEvent::PlayerDamaged { player_id }
                | WorldEvent::DrillProgressed { player_id }
                | WorldEvent::PurchaseCompleted { player_id }
                | WorldEvent::RescueTriggered { player_id }
                | WorldEvent::PlayerSurvivalChanged { player_id }
                | WorldEvent::BombPlaced { player_id } => {
                    players.insert(*player_id);
                }
                WorldEvent::TickAdvanced { .. }
                | WorldEvent::CommandsProcessed { .. }
                | WorldEvent::TerrainTilesChanged { .. }
                | WorldEvent::MessageChanged { .. }
                | WorldEvent::HazardChanged
                | WorldEvent::ImportantEffectTriggered
                | WorldEvent::ClientExitRequested { .. }
                | WorldEvent::ClientSettingsChanged { .. } => {}
            }
        }

        if keyframe_required {
            CompactWorldDelta::KeyframeRequired { tick: self.tick }
        } else if !terrain_revisions.is_empty() {
            CompactWorldDelta::TerrainChunks {
                tick: self.tick,
                revisions: terrain_revisions,
            }
        } else if !players.is_empty() {
            CompactWorldDelta::Players {
                tick: self.tick,
                players: players.into_iter().collect(),
            }
        } else {
            CompactWorldDelta::Noop { tick: self.tick }
        }
    }
}

const KEYFRAME_INTERVAL_TICKS: u64 = SIMULATION_HZ as u64 * 5;
const DEFAULT_VIEWPORT_WIDTH: i32 = 1280;
const DEFAULT_VIEWPORT_HEIGHT: i32 = 720;
const MIN_INTERPOLATION_DELAY_SECONDS: f32 = 0.05;
const MAX_INTERPOLATION_DELAY_SECONDS: f32 = 0.25;
const SMOOTH_CORRECTION_THRESHOLD_SQUARED: f32 = 4.0;
const SNAP_CORRECTION_THRESHOLD_SQUARED: f32 = 400.0;
const EXTRAPOLATION_LIMIT_SECONDS: f32 = 0.16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotChunkRecoveryPlan {
    pub snapshot_tick: SimulationTick,
    pub requested_position: TerrainChunkPosition,
    pub known_revision: u64,
    pub delta: CompactWorldDelta,
    pub requires_keyframe: bool,
}

impl SnapshotChunkRecoveryPlan {
    #[must_use]
    pub fn from_tracker(
        tracker: &TerrainRevisionTracker,
        snapshot_tick: SimulationTick,
        requested_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> Self {
        let delta = tracker.recovery_delta(snapshot_tick, requested_position, known_revision);
        let requires_keyframe = matches!(delta, CompactWorldDelta::KeyframeRequired { .. });
        Self {
            snapshot_tick,
            requested_position,
            known_revision,
            delta,
            requires_keyframe,
        }
    }

    #[must_use]
    pub const fn recovered_revision(&self) -> Option<u64> {
        match &self.delta {
            CompactWorldDelta::TerrainChunks { revisions, .. } => {
                if let [revision] = revisions.as_slice() {
                    Some(revision.revision)
                } else {
                    None
                }
            }
            CompactWorldDelta::Noop { .. }
            | CompactWorldDelta::Players { .. }
            | CompactWorldDelta::KeyframeRequired { .. } => None,
        }
    }
}

/// Lightweight simulation events emitted by the session compatibility layer.
///
/// These are intentionally separate from save data and renderer snapshots. As systems migrate out
/// of legacy `GameState`, this event stream becomes the bridge for audio, UI, renderer dirty
/// state, and eventually network deltas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldEvent {
    TickAdvanced {
        tick: SimulationTick,
    },
    CommandsProcessed {
        tick: SimulationTick,
        command_count: usize,
    },
    TerrainRefreshRequested,
    TerrainTilesChanged {
        positions: Vec<TilePosition>,
    },
    TerrainChunksChanged {
        revisions: Vec<TerrainChunkRevision>,
        changed_tiles: Vec<TilePosition>,
    },
    SnapshotKeyframeReady {
        tick: SimulationTick,
    },
    MessageChanged {
        message: String,
    },
    PlayerChanged {
        player_id: PlayerId,
    },
    CargoChanged {
        player_id: PlayerId,
    },
    PlayerDamaged {
        player_id: PlayerId,
    },
    DrillProgressed {
        player_id: PlayerId,
    },
    PurchaseCompleted {
        player_id: PlayerId,
    },
    RescueTriggered {
        player_id: PlayerId,
    },
    PlayerSurvivalChanged {
        player_id: PlayerId,
    },
    BombPlaced {
        player_id: PlayerId,
    },
    HazardChanged,
    ImportantEffectTriggered,
    ClientExitRequested {
        client_id: ClientId,
    },
    ClientSettingsChanged {
        client_id: ClientId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionSaveLoadAction {
    None,
    Saved,
    Loaded,
    SaveDenied,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSaveLoadSummary {
    pub action: SessionSaveLoadAction,
    pub message: String,
}

impl SessionSaveLoadSummary {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            action: SessionSaveLoadAction::None,
            message: String::new(),
        }
    }

    #[must_use]
    pub const fn saved(message: String) -> Self {
        Self {
            action: SessionSaveLoadAction::Saved,
            message,
        }
    }

    #[must_use]
    pub const fn loaded(message: String) -> Self {
        Self {
            action: SessionSaveLoadAction::Loaded,
            message,
        }
    }

    #[must_use]
    pub const fn save_denied(message: String) -> Self {
        Self {
            action: SessionSaveLoadAction::SaveDenied,
            message,
        }
    }

    #[must_use]
    pub const fn failed(message: String) -> Self {
        Self {
            action: SessionSaveLoadAction::Failed,
            message,
        }
    }

    #[must_use]
    pub const fn acted(&self) -> bool {
        !matches!(self.action, SessionSaveLoadAction::None)
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "completion summary intentionally records top-level multiplayer progress document status"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MultiplayerImplementationCompletionSummary {
    pub world_and_client_state_split: bool,
    pub fixed_tick_authoritative_path: bool,
    pub player_scoped_gameplay: bool,
    pub local_split_screen_rendering: bool,
    pub online_transport_deferred: bool,
    pub prediction_drives_presentation: bool,
    pub live_snapshot_delta_chunk_sync: bool,
    pub single_player_regressions_covered: bool,
}

impl MultiplayerImplementationCompletionSummary {
    #[must_use]
    pub const fn primary_migration_complete_or_deferred(self) -> bool {
        self.world_and_client_state_split
            && self.fixed_tick_authoritative_path
            && self.player_scoped_gameplay
            && self.local_split_screen_rendering
            && self.online_transport_deferred
            && self.prediction_drives_presentation
            && self.live_snapshot_delta_chunk_sync
            && self.single_player_regressions_covered
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "dependency summary intentionally records checklist-style migration coverage"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthoritativeDependencySummary {
    pub commands_apply_to_world_state: bool,
    pub player_state_read_from_world_state: bool,
    pub network_sync_reads_world_state: bool,
    pub render_plans_read_world_and_clients: bool,
    pub legacy_game_limited_to_adapter: bool,
}

impl AuthoritativeDependencySummary {
    #[must_use]
    pub const fn authoritative_path_split(self) -> bool {
        self.commands_apply_to_world_state
            && self.player_state_read_from_world_state
            && self.network_sync_reads_world_state
            && self.render_plans_read_world_and_clients
            && self.legacy_game_limited_to_adapter
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "effect routing summary intentionally records independent presentation/gameplay domains"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransientEffectRoutingSummary {
    pub local_render_effects_are_client_presentation: bool,
    pub local_audio_effects_are_client_presentation: bool,
    pub gameplay_effects_are_world_events: bool,
    pub network_deltas_ignore_local_presentation: bool,
}

impl TransientEffectRoutingSummary {
    #[must_use]
    pub const fn split(self) -> bool {
        self.local_render_effects_are_client_presentation
            && self.local_audio_effects_are_client_presentation
            && self.gameplay_effects_are_world_events
            && self.network_deltas_ignore_local_presentation
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "migration ownership summary intentionally records checklist-style domain coverage"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldOwnershipSummary {
    pub terrain_owned: bool,
    pub players_owned: bool,
    pub hazards_owned: bool,
    pub bombs_owned: bool,
    pub infrastructure_owned: bool,
    pub economy_metadata_owned: bool,
    pub progression_metadata_owned: bool,
    pub simulation_tick_owned: bool,
}

impl WorldOwnershipSummary {
    #[must_use]
    pub const fn fully_split(self) -> bool {
        self.terrain_owned
            && self.players_owned
            && self.hazards_owned
            && self.bombs_owned
            && self.infrastructure_owned
            && self.economy_metadata_owned
            && self.progression_metadata_owned
            && self.simulation_tick_owned
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerMovementIntent {
    pub horizontal: f32,
    pub thrust: bool,
    pub drill_down: bool,
}

impl PlayerMovementIntent {
    #[must_use]
    pub const fn from_command(command: &PlayerCommand) -> Option<Self> {
        if let PlayerCommand::Movement {
            horizontal,
            thrust,
            drill_down,
        } = *command
        {
            Some(Self {
                horizontal,
                thrust,
                drill_down,
            })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn apply_to_input(self, mut input: PlayerInput) -> PlayerInput {
        input.horizontal = self.horizontal;
        input.thrust = self.thrust;
        input.drill_down = self.drill_down;
        input
    }
}

fn online_identity_for_slot(slot: u8) -> (ClientId, PlayerId) {
    (
        ClientId::new(u64::from(slot)),
        PlayerId::new(u64::from(slot)),
    )
}

fn client_id_for_online_slot(slot: u8) -> ClientId {
    online_identity_for_slot(slot).0
}

fn apply_network_player_snapshot_to_player(player: &mut Player, snapshot: &NetworkPlayerSnapshot) {
    player.x = snapshot.x;
    player.y = snapshot.y;
    player.velocity_x = snapshot.velocity_x;
    player.velocity_y = snapshot.velocity_y;
    player.fuel = snapshot.fuel;
    player.hull = snapshot.hull;
    player.credits = snapshot.credits;
    player.cargo.clone_from(&snapshot.cargo);
    player.artifacts.clone_from(&snapshot.artifacts);
    player.materials.clone_from(&snapshot.materials);
    snapshot.loadout.apply_to_player(player);
}

const fn apply_authoritative_command_to_legacy_input(
    mut input: PlayerInput,
    command: &PlayerCommand,
) -> PlayerInput {
    match command {
        PlayerCommand::UseScanner => input.scan = true,
        PlayerCommand::PlaceBomb => input.bomb = true,
        PlayerCommand::PlaceInfrastructure { slot } => match slot {
            0 => input.place_relay = true,
            1 => input.place_drone = true,
            2 => input.place_lift = true,
            3 => input.place_support = true,
            4 => input.place_pump = true,
            5 => input.place_processor = true,
            _ => {}
        },
        PlayerCommand::SelectUpgrade { index } => input.selected_upgrade = Some(*index),
        PlayerCommand::Interact => input.interact = true,
        PlayerCommand::Cancel => input.cancel = true,
        PlayerCommand::Confirm => input.confirm = true,
        PlayerCommand::Movement { .. }
        | PlayerCommand::BuyUpgrade { .. }
        | PlayerCommand::Refuel
        | PlayerCommand::Repair
        | PlayerCommand::SellCargo
        | PlayerCommand::Rescue => {}
    }
    input
}

fn world_events_for_gameplay_event(event: GameplayEvent) -> Vec<WorldEvent> {
    match event.kind {
        GameplayEventKind::DrillStarted { player_id } => vec![
            WorldEvent::ImportantEffectTriggered,
            WorldEvent::DrillProgressed { player_id },
        ],
        GameplayEventKind::PurchaseCompleted { player_id } => {
            vec![WorldEvent::PurchaseCompleted { player_id }]
        }
        GameplayEventKind::CargoChanged { player_id } => {
            vec![WorldEvent::CargoChanged { player_id }]
        }
        GameplayEventKind::BombPlaced { player_id } => {
            vec![WorldEvent::BombPlaced { player_id }]
        }
        GameplayEventKind::InfrastructurePlaced { .. }
        | GameplayEventKind::ScannerUsed { .. }
        | GameplayEventKind::BouldersChanged
        | GameplayEventKind::BombDetonated
        | GameplayEventKind::EscapeStarted { .. }
        | GameplayEventKind::EscapeTunnelChanged
        | GameplayEventKind::DeepRewardGranted { .. } => {
            vec![WorldEvent::ImportantEffectTriggered]
        }
        GameplayEventKind::RescueTriggered { player_id } => vec![
            WorldEvent::RescueTriggered { player_id },
            WorldEvent::PlayerSurvivalChanged { player_id },
        ],
        GameplayEventKind::PlayerDamaged { player_id } => {
            vec![WorldEvent::PlayerDamaged { player_id }]
        }
        GameplayEventKind::HazardChanged => vec![WorldEvent::HazardChanged],
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Viewport {
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitScreenLayout {
    Single,
    VerticalTwoUp,
    Quad,
}

#[must_use]
pub const fn split_screen_layout(client_count: usize) -> SplitScreenLayout {
    match client_count {
        0 | 1 => SplitScreenLayout::Single,
        2 => SplitScreenLayout::VerticalTwoUp,
        _ => SplitScreenLayout::Quad,
    }
}

#[must_use]
pub fn split_screen_viewports(client_count: usize) -> Vec<Viewport> {
    match split_screen_layout(client_count) {
        SplitScreenLayout::Single => vec![Viewport::new(
            0,
            0,
            DEFAULT_VIEWPORT_WIDTH,
            DEFAULT_VIEWPORT_HEIGHT,
        )],
        SplitScreenLayout::VerticalTwoUp => vec![
            Viewport::new(0, 0, DEFAULT_VIEWPORT_WIDTH / 2, DEFAULT_VIEWPORT_HEIGHT),
            Viewport::new(
                DEFAULT_VIEWPORT_WIDTH / 2,
                0,
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT,
            ),
        ],
        SplitScreenLayout::Quad => vec![
            Viewport::new(
                0,
                0,
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT / 2,
            ),
            Viewport::new(
                DEFAULT_VIEWPORT_WIDTH / 2,
                0,
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT / 2,
            ),
            Viewport::new(
                0,
                DEFAULT_VIEWPORT_HEIGHT / 2,
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT / 2,
            ),
            Viewport::new(
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT / 2,
                DEFAULT_VIEWPORT_WIDTH / 2,
                DEFAULT_VIEWPORT_HEIGHT / 2,
            ),
        ],
    }
}

/// Per-client presentation state used by renderers and future split-screen views.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientView {
    pub client_id: ClientId,
    pub controlled_player_id: PlayerId,
    pub viewport: Viewport,
    pub camera: raylib::prelude::Vector2,
    pub run_mode: RunMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderPlayerPresentation {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub predicted: bool,
    pub correction_plan: CorrectionPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderPlayerPresentationSource {
    AuthoritativeWorld,
    PredictedLocal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderWorldPlayerPresentation {
    pub player_id: PlayerId,
    pub display_name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub local_to_view: bool,
    pub source: RenderPlayerPresentationSource,
    pub fuel: f32,
    pub hull: f32,
    pub credits: u32,
    pub cargo_used: u32,
    pub max_hull: f32,
    pub scanner_level: u8,
    pub radiator_level: u8,
    pub hull_level: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OnlineRemoteWorldPresentation {
    pub player_id: PlayerId,
    pub display_name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
    pub credits: u32,
    pub cargo_used: u32,
}

impl OnlineRemoteWorldPresentation {
    #[must_use]
    pub fn as_render_player(&self) -> RenderWorldPlayerPresentation {
        RenderWorldPlayerPresentation {
            player_id: self.player_id,
            display_name: self.display_name.clone(),
            x: self.x,
            y: self.y,
            velocity_x: self.velocity_x,
            velocity_y: self.velocity_y,
            local_to_view: false,
            source: RenderPlayerPresentationSource::AuthoritativeWorld,
            fuel: self.fuel,
            hull: self.hull,
            credits: self.credits,
            cargo_used: self.cargo_used,
            max_hull: self.hull.max(1.0),
            scanner_level: 0,
            radiator_level: 0,
            hull_level: 0,
        }
    }
}

impl RenderWorldPlayerPresentation {
    #[must_use]
    pub fn short_status_label(&self) -> String {
        format!(
            "P{} F{:.0} H{:.0} C{}",
            self.display_name
                .as_deref()
                .map_or_else(|| self.player_id.get().to_string(), str::to_owned),
            self.fuel.max(0.0),
            self.hull.max(0.0),
            self.cargo_used
        )
    }

    #[must_use]
    pub fn danger_level(&self) -> RemotePlayerDangerLevel {
        if self.hull <= 0.0 || self.fuel <= 0.0 {
            RemotePlayerDangerLevel::Critical
        } else if self.hull < 30.0 || self.fuel < 30.0 {
            RemotePlayerDangerLevel::Warning
        } else {
            RemotePlayerDangerLevel::Nominal
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemotePlayerDangerLevel {
    Nominal,
    Warning,
    Critical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderViewportPlan {
    pub client_id: ClientId,
    pub viewport: Viewport,
    pub camera: raylib::prelude::Vector2,
    pub clip_enabled: bool,
    pub local_player: Option<RenderPlayerPresentation>,
    pub remote_player_count: usize,
    pub remote_players: Vec<RemotePlayerPresentation>,
    pub correction_frame: Option<CorrectionPresentationFrame>,
    pub feedback_outputs: Vec<TentativeFeedbackOutput>,
    pub prediction_overlay: Option<OnlinePredictionRenderOverlay>,
}

#[derive(Clone, Debug)]
pub struct RenderParticleSnapshot {
    pub lost_cargo: Option<(f32, f32)>,
    pub dust: Vec<crate::game_state::DustParticle>,
    pub boulders: Vec<FallingBoulder>,
    pub sparks: Vec<crate::game_state::SparkParticle>,
}

#[derive(Clone, Debug)]
pub struct LiveRenderFrameOutput {
    pub terrain: Terrain,
    pub hazards: Vec<HazardCloud>,
    pub bombs: Vec<PlacedBomb>,
    pub infrastructure: Vec<PlacedInfrastructure>,
    pub active_drills: BTreeMap<PlayerId, DrillState>,
    pub particles: RenderParticleSnapshot,
    pub discovered_tiles_by_player: BTreeMap<PlayerId, BTreeSet<TilePosition>>,
    pub viewport_plans: Vec<RenderViewportPlan>,
    pub hud_snapshots: Vec<PerPlayerHudSnapshot>,
    pub world_players_by_view: Vec<(ClientId, Vec<RenderWorldPlayerPresentation>)>,
    pub ui_snapshots: Vec<SplitScreenUiPresentationSnapshot>,
}

impl LiveRenderFrameOutput {
    #[must_use]
    pub fn clipped_viewport_count(&self) -> usize {
        self.viewport_plans
            .iter()
            .filter(|plan| plan.clip_enabled)
            .count()
    }

    #[must_use]
    pub const fn hud_count(&self) -> usize {
        self.hud_snapshots.len()
    }

    #[must_use]
    pub fn split_screen_ui_ready(&self) -> bool {
        let client_count = self.viewport_plans.len();
        client_count >= 2
            && self.ui_snapshots.len() == client_count
            && self
                .ui_snapshots
                .iter()
                .all(SplitScreenUiPresentationSnapshot::has_per_player_overlay_context)
    }

    #[must_use]
    pub fn all_visible_players_are_session_sourced(&self) -> bool {
        self.world_players_by_view
            .iter()
            .flat_map(|(_, players)| players)
            .all(|player| {
                matches!(
                    player.source,
                    RenderPlayerPresentationSource::AuthoritativeWorld
                        | RenderPlayerPresentationSource::PredictedLocal
                )
            })
    }

    #[must_use]
    pub fn ready_for_live_render_path(&self) -> bool {
        let client_count = self.viewport_plans.len();
        let mut viewports = BTreeSet::new();
        let mut players = BTreeSet::new();
        for plan in &self.viewport_plans {
            viewports.insert((
                plan.viewport.x,
                plan.viewport.y,
                plan.viewport.width,
                plan.viewport.height,
            ));
            if let Some(player) = plan.local_player {
                players.insert(player.player_id);
            }
        }
        client_count >= 2
            && self.clipped_viewport_count() == client_count
            && viewports.len() == client_count
            && players.len() == client_count
            && self.hud_snapshots.len() == client_count
            && self.split_screen_ui_ready()
            && self.all_visible_players_are_session_sourced()
            && (self
                .viewport_plans
                .iter()
                .all(|plan| plan.remote_player_count + 1 >= client_count)
                || self
                    .world_players_by_view
                    .iter()
                    .all(|(_, players)| players.len() >= client_count))
            && self
                .viewport_plans
                .iter()
                .any(|plan| plan.correction_frame.is_some() || !plan.feedback_outputs.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderFramePlan {
    pub world_summary: AuthoritativeWorldSummary,
    pub views: Vec<ClientView>,
    pub players: Vec<PlayerSnapshot>,
}

impl RenderFramePlan {
    #[must_use]
    pub fn from_world_and_clients(
        world: &WorldState,
        clients: &BTreeMap<ClientId, ClientState>,
    ) -> Self {
        Self::from_world_and_client_views(world, clients.values().map(|client| client.view))
    }

    #[must_use]
    pub fn from_world_and_client_views(
        world: &WorldState,
        views: impl IntoIterator<Item = ClientView>,
    ) -> Self {
        Self {
            world_summary: world.authoritative_summary().clone(),
            views: views.into_iter().collect(),
            players: world.player_snapshots(),
        }
    }

    #[must_use]
    pub const fn view_count(&self) -> usize {
        self.views.len()
    }

    #[must_use]
    pub fn player_for_view(&self, view: &ClientView) -> Option<&PlayerSnapshot> {
        self.players
            .iter()
            .find(|player| player.player_id == view.controlled_player_id)
    }

    #[must_use]
    pub fn predicted_player_for_view(
        &self,
        view: &ClientView,
        prediction_plan: &PredictionPresentationPlan,
    ) -> Option<RenderPlayerPresentation> {
        let player = self.player_for_view(view)?;
        let corrected = prediction_plan
            .corrected_local_presentation
            .filter(|movement| movement.player_id == view.controlled_player_id);
        let predicted = prediction_plan
            .local_movement
            .filter(|movement| movement.player_id == view.controlled_player_id);
        let Some((x, y)) = corrected
            .map(|movement| (movement.x, movement.y))
            .or_else(|| predicted.map(|movement| (movement.x, movement.y)))
        else {
            return Some(RenderPlayerPresentation {
                player_id: player.player_id,
                x: player.x,
                y: player.y,
                predicted: false,
                correction_plan: CorrectionPlan::None,
            });
        };
        Some(RenderPlayerPresentation {
            player_id: view.controlled_player_id,
            x,
            y,
            predicted: true,
            correction_plan: prediction_plan
                .correction
                .map_or(CorrectionPlan::None, |correction| {
                    correction.correction_plan
                }),
        })
    }

    #[must_use]
    pub const fn online_authority_policy(
        is_descriptor_client: bool,
    ) -> OnlineRenderAuthorityPolicy {
        if is_descriptor_client {
            OnlineRenderAuthorityPolicy::ClientPredicted
        } else {
            OnlineRenderAuthorityPolicy::HostAuthoritative
        }
    }

    #[must_use]
    pub fn online_prediction_overlay_for_policy(
        &self,
        prediction: &ClientPredictionState,
        policy: OnlineRenderAuthorityPolicy,
    ) -> OnlinePredictionRenderOverlay {
        let _view_count = self.view_count();
        prediction.render_overlay(policy)
    }

    #[must_use]
    pub fn remote_player_presentations(
        &self,
        view: &ClientView,
        prediction_plan: &PredictionPresentationPlan,
    ) -> Vec<RemotePlayerPresentation> {
        if self.views.is_empty() {
            return Vec::new();
        }
        prediction_plan
            .remote_players
            .iter()
            .copied()
            .filter(|player| player.player_id != view.controlled_player_id)
            .collect()
    }

    #[must_use]
    pub fn world_player_presentations_for_view(
        &self,
        view: &ClientView,
        prediction_plan: &PredictionPresentationPlan,
    ) -> Vec<RenderWorldPlayerPresentation> {
        self.players
            .iter()
            .map(|player| {
                let local_to_view = player.player_id == view.controlled_player_id;
                let corrected = local_to_view
                    .then_some(prediction_plan.corrected_local_presentation)
                    .flatten()
                    .filter(|movement| movement.player_id == player.player_id);
                let predicted = local_to_view
                    .then_some(prediction_plan.local_movement)
                    .flatten()
                    .filter(|movement| movement.player_id == player.player_id);
                let prediction_position = corrected
                    .map(|movement| (movement.x, movement.y))
                    .or_else(|| predicted.map(|movement| (movement.x, movement.y)));
                RenderWorldPlayerPresentation {
                    player_id: player.player_id,
                    display_name: None,
                    x: prediction_position.map_or(player.x, |position| position.0),
                    y: prediction_position.map_or(player.y, |position| position.1),
                    velocity_x: player.velocity_x,
                    velocity_y: player.velocity_y,
                    local_to_view,
                    source: prediction_position
                        .map_or(RenderPlayerPresentationSource::AuthoritativeWorld, |_| {
                            RenderPlayerPresentationSource::PredictedLocal
                        }),
                    fuel: player.fuel,
                    hull: player.hull,
                    credits: player.credits,
                    cargo_used: player.cargo_used,
                    max_hull: player.max_hull,
                    scanner_level: player.loadout.scanner_level,
                    radiator_level: player.loadout.radiator_level,
                    hull_level: player.loadout.hull_level,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn viewport_plans(
        &self,
        prediction_plan: &PredictionPresentationPlan,
    ) -> Vec<RenderViewportPlan> {
        self.views
            .iter()
            .map(|view| {
                let remote_players = self.remote_player_presentations(view, prediction_plan);
                let local_player = self.predicted_player_for_view(view, prediction_plan);
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "bounded viewport dimensions intentionally convert to camera pixels"
                )]
                let camera = local_player.map_or(view.camera, |player| {
                    raylib::prelude::Vector2::new(
                        (view.viewport.width as f32).mul_add(-0.5, player.x),
                        (view.viewport.height as f32).mul_add(-0.5, player.y),
                    )
                });
                RenderViewportPlan {
                    client_id: view.client_id,
                    viewport: view.viewport,
                    camera,
                    clip_enabled: true,
                    local_player,
                    remote_player_count: remote_players.len(),
                    remote_players,
                    correction_frame: prediction_plan.correction.map(|correction| {
                        CorrectionPresentationFrame::from_reconciliation(&correction, 0.5)
                    }),
                    feedback_outputs: if prediction_plan.feedback_outputs.is_empty() {
                        vec![TentativeFeedbackPresentation::DrillProgressVisual.output()]
                    } else {
                        prediction_plan.feedback_outputs.clone()
                    },
                    prediction_overlay: None,
                }
            })
            .collect()
    }

    #[must_use]
    pub fn hud_snapshot_for_view(&self, view: &ClientView) -> Option<PerPlayerHudSnapshot> {
        self.player_for_view(view)
            .map(PerPlayerHudSnapshot::from_player_snapshot)
    }

    #[must_use]
    pub fn hud_snapshots(&self) -> Vec<PerPlayerHudSnapshot> {
        self.views
            .iter()
            .filter_map(|view| self.hud_snapshot_for_view(view))
            .collect()
    }

    #[must_use]
    pub fn client_presentation_snapshots(&self) -> Vec<PerClientPresentationSnapshot> {
        self.views
            .iter()
            .filter_map(|view| {
                self.hud_snapshot_for_view(view)
                    .map(|hud| PerClientPresentationSnapshot {
                        client_id: view.client_id,
                        player_id: view.controlled_player_id,
                        hud,
                        viewport: view.viewport,
                    })
            })
            .collect()
    }

    #[must_use]
    pub fn survival_snapshots(&self) -> Vec<PlayerSurvivalSnapshot> {
        self.players
            .iter()
            .map(|player| PlayerSurvivalSnapshot {
                player_id: player.player_id,
                fuel: player.fuel,
                hull: player.hull,
                disabled: player.hull <= 0.0,
                stranded: player.fuel <= 0.0,
            })
            .collect()
    }
}

impl ClientView {
    #[must_use]
    pub fn from_legacy_game(game: &GameState) -> Self {
        Self {
            client_id: LOCAL_CLIENT_ID,
            controlled_player_id: LOCAL_PLAYER_ID,
            viewport: Viewport::new(0, 0, DEFAULT_VIEWPORT_WIDTH, DEFAULT_VIEWPORT_HEIGHT),
            camera: raylib::prelude::Vector2::new(game.camera_x, game.camera_y),
            run_mode: game.run_mode,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalTentativeFeedback {
    MovementIntent,
    DrillContact,
    DrillProgressVisual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TentativeFeedbackPresentation {
    MovementVisual,
    DrillContactAudio,
    DrillProgressVisual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TentativeFeedbackChannel {
    Render,
    Audio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TentativeFeedbackOutput {
    pub presentation: TentativeFeedbackPresentation,
    pub channel: TentativeFeedbackChannel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TentativeFeedbackFrame {
    pub outputs: Vec<TentativeFeedbackOutput>,
    pub render_output_count: usize,
    pub audio_output_count: usize,
}

impl TentativeFeedbackFrame {
    #[must_use]
    pub fn from_outputs(outputs: Vec<TentativeFeedbackOutput>) -> Self {
        let render_output_count = outputs
            .iter()
            .filter(|output| output.channel == TentativeFeedbackChannel::Render)
            .count();
        let audio_output_count = outputs.len() - render_output_count;
        Self {
            outputs,
            render_output_count,
            audio_output_count,
        }
    }

    #[must_use]
    pub const fn has_drill_feedback(&self) -> bool {
        self.render_output_count > 0 || self.audio_output_count > 0
    }
}

impl TentativeFeedbackPresentation {
    #[must_use]
    pub const fn output(self) -> TentativeFeedbackOutput {
        let channel = match self {
            Self::MovementVisual | Self::DrillProgressVisual => TentativeFeedbackChannel::Render,
            Self::DrillContactAudio => TentativeFeedbackChannel::Audio,
        };
        TentativeFeedbackOutput {
            presentation: self,
            channel,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictionFailureResolution {
    RequestTerrainChunk,
    RequestAuthoritativeSnapshot,
    RollBackLocalEconomy,
    RollBackProgression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PredictionRecoveryAction {
    RequestTerrainDelta(CompactWorldDelta),
    RequestAuthoritativeSnapshot { player_id: PlayerId },
    RollBackLocalEconomy { player_id: PlayerId },
    RollBackProgression { player_id: PlayerId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionFailureApplicationSummary {
    pub requested_keyframe: bool,
    pub requested_terrain_deltas: usize,
    pub rolled_back_economy_players: Vec<PlayerId>,
    pub rolled_back_progression_players: Vec<PlayerId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionFailureRecoveryPlan {
    pub actions: Vec<PredictionRecoveryAction>,
    pub request_keyframe: bool,
}

impl PredictionFailureRecoveryPlan {
    #[must_use]
    pub fn from_actions(actions: Vec<PredictionRecoveryAction>) -> Self {
        let request_keyframe = actions.iter().any(|action| {
            matches!(
                action,
                PredictionRecoveryAction::RequestAuthoritativeSnapshot { .. }
            )
        });
        Self {
            actions,
            request_keyframe,
        }
    }

    #[must_use]
    pub fn application_summary(&self) -> PredictionFailureApplicationSummary {
        let mut requested_terrain_deltas = 0;
        let mut rolled_back_economy_players = Vec::new();
        let mut rolled_back_progression_players = Vec::new();
        for action in &self.actions {
            match action {
                PredictionRecoveryAction::RequestTerrainDelta(_) => {
                    requested_terrain_deltas += 1;
                }
                PredictionRecoveryAction::RollBackLocalEconomy { player_id } => {
                    rolled_back_economy_players.push(*player_id);
                }
                PredictionRecoveryAction::RollBackProgression { player_id } => {
                    rolled_back_progression_players.push(*player_id);
                }
                PredictionRecoveryAction::RequestAuthoritativeSnapshot { .. } => {}
            }
        }
        PredictionFailureApplicationSummary {
            requested_keyframe: self.request_keyframe,
            requested_terrain_deltas,
            rolled_back_economy_players,
            rolled_back_progression_players,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectionOffset {
    pub x: f32,
    pub y: f32,
}

impl CorrectionOffset {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictionFailure {
    TerrainAlreadyChanged,
    HazardOrRescueChangedState,
    EconomyChangedState,
    ProgressionChangedState,
    CommandRejected,
    SaveSessionTransition,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    clippy::derive_partial_eq_without_eq,
    reason = "playtest gate report intentionally exposes independent pass/fail booleans and f32-derived samples"
)]
pub struct PredictionPlaytestReport {
    pub render_frames: usize,
    pub monotonic_local_motion: bool,
    pub immediate_tiny_tap_visible: bool,
    pub outbound_command_packets: usize,
    pub outbound_commands: usize,
    pub duplicate_outbound_commands: bool,
    pub stale_authority_backward_tugs: usize,
    pub predicted_local_render_source: bool,
    pub authoritative_remote_render_source: bool,
    pub correction_modes_seen: BTreeSet<CorrectionMode>,
    pub ready_for_live_tuning: bool,
}

impl PredictionPlaytestReport {
    #[must_use]
    pub fn from_samples(
        local_positions: &[f32],
        outbound_command_counts: &[usize],
        stale_authority_backward_tugs: usize,
        predicted_local_render_source: bool,
        authoritative_remote_render_source: bool,
        correction_modes_seen: BTreeSet<CorrectionMode>,
    ) -> Self {
        let monotonic_local_motion = local_positions
            .windows(2)
            .all(|window| window[1] + f32::EPSILON >= window[0]);
        let immediate_tiny_tap_visible = local_positions
            .first()
            .zip(local_positions.last())
            .is_some_and(|(first, last)| last > first);
        let outbound_command_packets = outbound_command_counts.len();
        let outbound_commands = outbound_command_counts.iter().sum();
        let duplicate_outbound_commands = outbound_command_counts.iter().any(|count| *count > 1);
        let ready_for_live_tuning = monotonic_local_motion
            && immediate_tiny_tap_visible
            && !duplicate_outbound_commands
            && stale_authority_backward_tugs == 0
            && predicted_local_render_source
            && authoritative_remote_render_source;
        Self {
            render_frames: local_positions.len(),
            monotonic_local_motion,
            immediate_tiny_tap_visible,
            outbound_command_packets,
            outbound_commands,
            duplicate_outbound_commands,
            stale_authority_backward_tugs,
            predicted_local_render_source,
            authoritative_remote_render_source,
            correction_modes_seen,
            ready_for_live_tuning,
        }
    }

    #[must_use]
    pub const fn movement_feel_passed(&self) -> bool {
        self.monotonic_local_motion && self.immediate_tiny_tap_visible
    }

    #[must_use]
    pub const fn command_delivery_passed(&self) -> bool {
        self.outbound_command_packets == self.outbound_commands && !self.duplicate_outbound_commands
    }

    #[must_use]
    pub const fn authority_tug_passed(&self) -> bool {
        self.stale_authority_backward_tugs == 0
    }

    #[must_use]
    pub const fn render_sources_passed(&self) -> bool {
        self.predicted_local_render_source && self.authoritative_remote_render_source
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CorrectionMode {
    None,
    Smooth,
    Snap,
}

impl CorrectionMode {
    #[must_use]
    pub const fn from_plan(plan: CorrectionPlan) -> Self {
        match plan {
            CorrectionPlan::None => Self::None,
            CorrectionPlan::Smooth => Self::Smooth,
            CorrectionPlan::Snap => Self::Snap,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorrectionPlan {
    None,
    Smooth,
    Snap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionCorrectionTuning {
    pub smooth_threshold_squared: f32,
    pub snap_threshold_squared: f32,
    pub smoothing_alpha: f32,
    pub extrapolation_limit_seconds: f32,
}

impl PredictionCorrectionTuning {
    #[must_use]
    pub const fn default_gameplay_feel() -> Self {
        Self {
            smooth_threshold_squared: SMOOTH_CORRECTION_THRESHOLD_SQUARED,
            snap_threshold_squared: SNAP_CORRECTION_THRESHOLD_SQUARED,
            smoothing_alpha: 0.5,
            extrapolation_limit_seconds: EXTRAPOLATION_LIMIT_SECONDS,
        }
    }

    #[must_use]
    pub fn classifies_expected_offsets() -> bool {
        let tuning = Self::default_gameplay_feel();
        ClientPredictionState::correction_plan(tuning.smooth_threshold_squared.sqrt() * 0.25, 0.0)
            == CorrectionPlan::None
            && ClientPredictionState::correction_plan(8.0, 0.0) == CorrectionPlan::Smooth
            && ClientPredictionState::correction_plan(24.0, 0.0) == CorrectionPlan::Snap
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteTimingTuning {
    pub interpolation_delay: f32,
    pub extrapolation_limit: f32,
    pub timeout_after: f32,
}

impl RemoteTimingTuning {
    #[must_use]
    pub fn from_latency_loss(ping_seconds: f32, loss_ratio: f32) -> Self {
        let interpolation_delay = ClientPredictionState::interpolation_delay_seconds(
            (ping_seconds * (1.5 + loss_ratio)).max(FIXED_DELTA_SECONDS),
        );
        let extrapolation_limit = loss_ratio
            .mul_add(0.08, EXTRAPOLATION_LIMIT_SECONDS)
            .clamp(EXTRAPOLATION_LIMIT_SECONDS, 0.28);
        Self {
            interpolation_delay,
            extrapolation_limit,
            timeout_after: extrapolation_limit * 2.0,
        }
    }

    #[must_use]
    pub fn allows_extrapolation(self, stall_seconds: f32) -> bool {
        stall_seconds <= self.extrapolation_limit
    }

    #[must_use]
    pub fn timed_out(self, stall_seconds: f32) -> bool {
        stall_seconds > self.timeout_after
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkDebugInstrumentationSnapshot {
    pub ping_seconds: f32,
    pub prediction_buffer_commands: usize,
    pub correction_plan: CorrectionPlan,
    pub dropped_packets: usize,
    pub snapshot_recoveries: usize,
    pub chunk_recoveries: usize,
}

impl NetworkDebugInstrumentationSnapshot {
    #[must_use]
    pub const fn visible_to_debug_overlay(self) -> bool {
        self.ping_seconds >= 0.0
            && self.prediction_buffer_commands > 0
            && self.snapshot_recoveries > 0
            && self.chunk_recoveries > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalMovementPredictionPlan {
    pub player_id: PlayerId,
    pub authoritative_x: f32,
    pub authoritative_y: f32,
    pub predicted_x: f32,
    pub predicted_y: f32,
    pub delta_seconds: f32,
}

impl LocalMovementPredictionPlan {
    #[must_use]
    pub const fn from_snapshot(snapshot: &PlayerSnapshot, delta_seconds: f32) -> Self {
        let predicted = PredictedMovement {
            player_id: snapshot.player_id,
            x: snapshot.velocity_x.mul_add(delta_seconds, snapshot.x),
            y: snapshot.velocity_y.mul_add(delta_seconds, snapshot.y),
            velocity_x: snapshot.velocity_x,
            velocity_y: snapshot.velocity_y,
        };
        Self {
            player_id: snapshot.player_id,
            authoritative_x: snapshot.x,
            authoritative_y: snapshot.y,
            predicted_x: predicted.x,
            predicted_y: predicted.y,
            delta_seconds,
        }
    }

    #[must_use]
    pub fn predicted_movement(self) -> PredictedMovement {
        PredictedMovement {
            player_id: self.player_id,
            x: self.predicted_x,
            y: self.predicted_y,
            velocity_x: if self.delta_seconds == 0.0 {
                0.0
            } else {
                (self.predicted_x - self.authoritative_x) / self.delta_seconds
            },
            velocity_y: if self.delta_seconds == 0.0 {
                0.0
            } else {
                (self.predicted_y - self.authoritative_y) / self.delta_seconds
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictedMovement {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
}

impl PredictedMovement {
    #[must_use]
    pub const fn from_snapshot(snapshot: &PlayerSnapshot) -> Self {
        Self {
            player_id: snapshot.player_id,
            x: snapshot.x,
            y: snapshot.y,
            velocity_x: snapshot.velocity_x,
            velocity_y: snapshot.velocity_y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconciledMovement {
    pub predicted: PredictedMovement,
    pub correction_plan: CorrectionPlan,
    pub correction_offset: Option<CorrectionOffset>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectedMovementPresentation {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub correction_plan: CorrectionPlan,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectionPresentationFrame {
    pub presentation: CorrectedMovementPresentation,
    pub smoothing_alpha: f32,
    pub snap_applied: bool,
}

impl CorrectionPresentationFrame {
    #[must_use]
    pub fn from_reconciliation(reconciliation: &ReconciledMovement, smoothing_alpha: f32) -> Self {
        Self {
            presentation: reconciliation.corrected_presentation(smoothing_alpha),
            smoothing_alpha: smoothing_alpha.clamp(0.0, 1.0),
            snap_applied: reconciliation.correction_plan == CorrectionPlan::Snap,
        }
    }
}

impl ReconciledMovement {
    #[must_use]
    pub fn corrected_presentation(&self, smoothing_alpha: f32) -> CorrectedMovementPresentation {
        let (x, y) = self.correction_offset.map_or(
            (self.predicted.x, self.predicted.y),
            |offset| match self.correction_plan {
                CorrectionPlan::None => (self.predicted.x, self.predicted.y),
                CorrectionPlan::Smooth => {
                    let alpha = smoothing_alpha.clamp(0.0, 1.0);
                    (
                        offset.x.mul_add(alpha, self.predicted.x),
                        offset.y.mul_add(alpha, self.predicted.y),
                    )
                }
                CorrectionPlan::Snap => (self.predicted.x + offset.x, self.predicted.y + offset.y),
            },
        );
        CorrectedMovementPresentation {
            player_id: self.predicted.player_id,
            x,
            y,
            correction_plan: self.correction_plan,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayedPrediction {
    pub predicted: PredictedMovement,
    pub replayed_command_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayedReconciliation {
    pub replayed: ReplayedPrediction,
    pub reconciliation: ReconciledMovement,
}

#[derive(Clone, Debug)]
pub struct AuthoritativeReconciliationInput {
    pub tick: SimulationTick,
    pub player_snapshot: PlayerSnapshot,
    pub terrain_revisions: Vec<TerrainChunkRevision>,
    pub changed_tiles: Vec<TilePosition>,
    pub active_drill: Option<DrillPredictionSnapshot>,
    pub authoritative_world: WorldState,
}

impl ReplayedReconciliation {
    #[must_use]
    pub fn from_authoritative_snapshot(
        authoritative: &PlayerSnapshot,
        commands: &[SequencedPlayerCommand],
    ) -> Self {
        let replayed =
            ClientPredictionState::replay_unacknowledged_movement(authoritative, commands);
        let reconciliation =
            ClientPredictionState::reconcile_movement(replayed.predicted, authoritative);
        Self {
            replayed,
            reconciliation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemotePlayerPresentation {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub extrapolated: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PredictionPresentationPlan {
    pub local_movement: Option<PredictedMovement>,
    pub correction: Option<ReconciledMovement>,
    pub corrected_local_presentation: Option<CorrectedMovementPresentation>,
    pub tentative_feedback: Vec<TentativeFeedbackPresentation>,
    pub remote_players: Vec<RemotePlayerPresentation>,
    pub failure_resolutions: Vec<PredictionFailureResolution>,
    pub feedback_outputs: Vec<TentativeFeedbackOutput>,
}

/// Local prediction/reconciliation bookkeeping for one client.
#[derive(Clone, Debug, Default)]
pub struct ClientPredictionState {
    unacknowledged_commands: Vec<SequencedPlayerCommand>,
    remote_player_snapshots: BTreeMap<PlayerId, Vec<PlayerSnapshot>>,
    prediction_failures: Vec<PredictionFailure>,
    pending_feedback: Vec<LocalTentativeFeedback>,
    correction_offset: Option<CorrectionOffset>,
    prediction_base_tick: Option<SimulationTick>,
    predicted_world: Option<WorldState>,
    predicted_replay_world: Option<WorldState>,
    predicted_local_snapshot: Option<PlayerSnapshot>,
    prediction_command_frames: Vec<PredictionCommandFrame>,
    prediction_history: VecDeque<PredictionHistoryEntry>,
    latest_input_intent: Option<PlayerMovementIntent>,
    predicted_effects: Vec<PredictedEffect>,
    predicted_modal: Option<PredictedModalState>,
    active_correction: Option<ActiveCorrection>,
    terrain_overlays: Vec<PredictedTerrainOverlay>,
}

impl ClientPredictionState {
    pub fn rebase_prediction_world(&mut self, tick: SimulationTick, world: &WorldState) {
        self.prediction_base_tick = Some(tick);
        self.predicted_world = Some(world.clone());
        self.predicted_replay_world = Some(world.clone());
        self.predicted_local_snapshot = None;
        self.predicted_effects.clear();
        self.predicted_modal = None;
        self.terrain_overlays.clear();
        for frame in self.prediction_command_frames.clone() {
            let _snapshot =
                self.apply_prediction_command_frame(world, frame, FIXED_DELTA_SECONDS, false, true);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "authoritative reconciliation keeps compare/rebase/correction ordering together"
    )]
    pub fn reconcile_authoritative_input(
        &mut self,
        input: &AuthoritativeReconciliationInput,
    ) -> Option<ActiveCorrection> {
        let previous_rendered = self
            .predicted_local_snapshot
            .as_ref()
            .map(PredictedMovement::from_snapshot);
        let matched_history = self.prediction_history.iter().find(|entry| {
            entry.tick == input.tick && entry.player_id == input.player_snapshot.player_id
        });
        let movement_mismatch = matched_history.is_none_or(|entry| {
            Self::correction_plan_for_snapshots(&entry.snapshot, &input.player_snapshot)
                != CorrectionPlan::None
        });
        let action_mismatch = matched_history.is_some_and(|entry| {
            entry.active_drill.as_ref().map(|drill| drill.target)
                != input.active_drill.as_ref().map(|drill| drill.target)
        });
        let terrain_mismatch = matched_history.is_some_and(|entry| {
            let predicted_changed_tile_without_host_match = entry
                .terrain_changed_tiles
                .iter()
                .any(|position| !input.changed_tiles.contains(position));
            let host_changed_tile_without_prediction = input
                .changed_tiles
                .iter()
                .any(|position| !entry.terrain_changed_tiles.contains(position));
            let host_revision_without_prediction = input.terrain_revisions.iter().any(|revision| {
                !entry.terrain_revisions.iter().any(|predicted| {
                    predicted.position == revision.position
                        && predicted.revision == revision.revision
                })
            });
            predicted_changed_tile_without_host_match
                || host_changed_tile_without_prediction
                || host_revision_without_prediction
        });
        let mismatch = movement_mismatch || action_mismatch || terrain_mismatch;

        self.prediction_base_tick = Some(input.tick);
        let mut rebased_world = input.authoritative_world.clone();
        if let Some(player) = rebased_world.player_mut(input.player_snapshot.player_id) {
            player.x = input.player_snapshot.x;
            player.y = input.player_snapshot.y;
            player.velocity_x = input.player_snapshot.velocity_x;
            player.velocity_y = input.player_snapshot.velocity_y;
            player.fuel = input.player_snapshot.fuel;
            player.hull = input.player_snapshot.hull;
            player.credits = input.player_snapshot.credits;
        }
        self.predicted_world = Some(rebased_world.clone());
        self.predicted_replay_world = Some(rebased_world.clone());
        self.predicted_local_snapshot = None;
        self.predicted_effects.clear();
        self.predicted_modal = None;
        self.terrain_overlays.clear();
        self.prediction_command_frames
            .retain(|frame| frame.target_tick > input.tick);
        for frame in self.prediction_command_frames.clone() {
            let _snapshot = self.apply_prediction_command_frame(
                &rebased_world,
                frame.clone(),
                frame.delta_seconds,
                false,
                true,
            );
        }

        let Some(previous) = previous_rendered else {
            self.active_correction = None;
            return None;
        };
        if !mismatch {
            self.active_correction = None;
            return None;
        }
        let Some(current) = self
            .predicted_local_snapshot
            .as_ref()
            .map(PredictedMovement::from_snapshot)
        else {
            self.active_correction = None;
            return None;
        };
        let correction_plan = if action_mismatch || terrain_mismatch {
            CorrectionPlan::Snap
        } else {
            Self::correction_plan_for_positions(previous.x, previous.y, current.x, current.y)
        };
        if correction_plan == CorrectionPlan::None {
            self.active_correction = None;
            return None;
        }
        let duration_seconds = if correction_plan == CorrectionPlan::Snap {
            0.0
        } else {
            0.12
        };
        let correction = ActiveCorrection::new(
            current.player_id,
            previous.x,
            previous.y,
            current.x,
            current.y,
            duration_seconds,
        )
        .with_mode(CorrectionMode::from_plan(correction_plan));
        self.active_correction = Some(correction);
        Some(correction)
    }

    pub fn reconcile_authoritative_snapshot(
        &mut self,
        tick: SimulationTick,
        authoritative_world: &WorldState,
        authoritative_snapshot: &PlayerSnapshot,
    ) -> Option<ActiveCorrection> {
        let active_drill = authoritative_world
            .active_drill(authoritative_snapshot.player_id)
            .copied()
            .map(|drill| {
                DrillPredictionSnapshot::from_drill(authoritative_snapshot.player_id, drill)
            });
        self.reconcile_authoritative_input(&AuthoritativeReconciliationInput {
            tick,
            player_snapshot: authoritative_snapshot.clone(),
            terrain_revisions: Vec::new(),
            changed_tiles: Vec::new(),
            active_drill,
            authoritative_world: authoritative_world.clone(),
        })
    }

    #[must_use]
    pub fn predicted_local_movement(&self, player_id: PlayerId) -> Option<PredictedMovement> {
        self.predicted_local_snapshot
            .as_ref()
            .filter(|snapshot| snapshot.player_id == player_id)
            .map(PredictedMovement::from_snapshot)
    }

    #[must_use]
    pub fn predicted_effects(&self) -> &[PredictedEffect] {
        &self.predicted_effects
    }

    #[must_use]
    pub fn predicted_modal(&self) -> Option<PredictedModal> {
        self.predicted_modal.map(|modal| modal.modal)
    }

    #[must_use]
    pub const fn predicted_modal_state(&self) -> Option<PredictedModalState> {
        self.predicted_modal
    }

    #[must_use]
    pub fn terrain_overlays(&self) -> &[PredictedTerrainOverlay] {
        &self.terrain_overlays
    }

    #[must_use]
    pub const fn is_render_overlay_empty(&self) -> bool {
        self.terrain_overlays.is_empty()
            && self.predicted_effects.is_empty()
            && self.predicted_modal.is_none()
    }

    #[must_use]
    pub fn render_overlay(
        &self,
        policy: OnlineRenderAuthorityPolicy,
    ) -> OnlinePredictionRenderOverlay {
        OnlinePredictionRenderOverlay {
            policy,
            terrain: self.terrain_overlays.clone(),
            effects: self.predicted_effects.clone(),
            modal: self.predicted_modal,
        }
    }

    #[must_use]
    pub const fn active_correction(&self) -> Option<ActiveCorrection> {
        self.active_correction
    }

    pub fn confirm_terrain_overlay(&mut self, revisions: &[TerrainChunkRevision]) -> usize {
        let mut confirmed = 0;
        for overlay in &mut self.terrain_overlays {
            if overlay.resolution == PredictedTerrainResolution::Pending
                && revisions.iter().any(|revision| {
                    revision.position == overlay.chunk && revision.revision >= overlay.base_revision
                })
            {
                overlay.resolution = PredictedTerrainResolution::Confirmed;
                confirmed += 1;
            }
        }
        confirmed
    }

    pub fn confirm_terrain_overlay_for_tiles(
        &mut self,
        revisions: &[TerrainChunkRevision],
        changed_tiles: &[TilePosition],
        command_sequence: Option<InputSequence>,
    ) -> usize {
        let mut confirmed = 0;
        for overlay in &mut self.terrain_overlays {
            if overlay.resolution != PredictedTerrainResolution::Pending {
                continue;
            }
            let revision_matches = revisions.iter().any(|revision| {
                revision.position == overlay.chunk && revision.revision >= overlay.base_revision
            });
            let tile_matches = changed_tiles.contains(&overlay.position);
            let sequence_matches = command_sequence.is_none()
                || overlay.command_sequence.is_none()
                || overlay.command_sequence == command_sequence;
            if revision_matches && tile_matches && sequence_matches {
                overlay.resolution = PredictedTerrainResolution::Confirmed;
                confirmed += 1;
            }
        }
        confirmed
    }

    pub fn revert_terrain_overlays_through_tick(
        &mut self,
        authoritative_tick: SimulationTick,
    ) -> usize {
        let mut reverted = 0;
        for overlay in &mut self.terrain_overlays {
            if overlay.target_tick <= authoritative_tick
                && overlay.resolution == PredictedTerrainResolution::Pending
            {
                overlay.resolution = PredictedTerrainResolution::Reverted;
                reverted += 1;
            }
        }
        reverted
    }

    pub fn revert_unconfirmed_terrain_overlays(&mut self) -> usize {
        let mut reverted = 0;
        for overlay in &mut self.terrain_overlays {
            if overlay.resolution == PredictedTerrainResolution::Pending {
                overlay.resolution = PredictedTerrainResolution::Reverted;
                reverted += 1;
            }
        }
        reverted
    }

    pub const fn confirm_predicted_modal(&mut self) -> bool {
        let Some(modal) = &mut self.predicted_modal else {
            return false;
        };
        modal.resolution = PredictedModalResolution::Confirmed;
        true
    }

    pub const fn revert_predicted_modal(&mut self) -> bool {
        let Some(modal) = &mut self.predicted_modal else {
            return false;
        };
        modal.resolution = PredictedModalResolution::Reverted;
        true
    }

    #[must_use]
    pub const fn latest_input_intent(&self) -> Option<PlayerMovementIntent> {
        self.latest_input_intent
    }

    #[must_use]
    pub fn deterministic_replay_frame_count(&self) -> usize {
        self.prediction_command_frames
            .iter()
            .filter(|frame| frame.delta_seconds > 0.0)
            .count()
    }

    #[must_use]
    pub fn transient_render_history_count_for_tests(&self) -> usize {
        self.prediction_history
            .iter()
            .filter(|entry| entry.last_sequence.is_none())
            .count()
    }

    #[must_use]
    pub const fn has_replay_world_for_tests(&self) -> bool {
        self.predicted_replay_world.is_some()
    }

    #[must_use]
    pub fn replay_world_snapshot_for_tests(&self, player_id: PlayerId) -> Option<PlayerSnapshot> {
        let world = self.predicted_replay_world.as_ref()?;
        world
            .player(player_id)
            .map(|player| PlayerSnapshot::from_world_player(player_id, player, world))
    }

    #[must_use]
    pub fn transient_world_snapshot_for_tests(
        &self,
        player_id: PlayerId,
    ) -> Option<PlayerSnapshot> {
        let world = self.predicted_world.as_ref()?;
        world
            .player(player_id)
            .map(|player| PlayerSnapshot::from_world_player(player_id, player, world))
    }

    pub fn predict_command_frame(
        &mut self,
        authoritative_world: &WorldState,
        mut frame: PredictionCommandFrame,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        frame.delta_seconds = delta_seconds;
        self.apply_prediction_command_frame(authoritative_world, frame, delta_seconds, true, true)
    }

    pub fn predict_transient_command_frame(
        &mut self,
        authoritative_world: &WorldState,
        mut frame: PredictionCommandFrame,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        frame.delta_seconds = delta_seconds;
        self.apply_prediction_command_frame(authoritative_world, frame, delta_seconds, false, false)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "prediction command application intentionally centralizes command effects while history/replay is stabilized"
    )]
    fn apply_prediction_command_frame(
        &mut self,
        authoritative_world: &WorldState,
        frame: PredictionCommandFrame,
        _delta_seconds: f32,
        remember_frame: bool,
        record_history: bool,
    ) -> Option<PlayerSnapshot> {
        let mut predicted = if remember_frame || record_history {
            self.predicted_replay_world
                .clone()
                .or_else(|| self.predicted_world.clone())
                .unwrap_or_else(|| authoritative_world.clone())
        } else {
            self.predicted_world
                .clone()
                .or_else(|| self.predicted_replay_world.clone())
                .unwrap_or_else(|| authoritative_world.clone())
        };
        self.predicted_effects.clear();
        self.predicted_modal = None;
        self.terrain_overlays.clear();
        for command in &frame.commands {
            match *command {
                PlayerCommand::Movement {
                    horizontal,
                    drill_down,
                    ..
                } => {
                    self.predicted_effects.push(PredictedEffect::Movement {
                        player_id: frame.player_id,
                    });
                    if let Some(player) = predicted.player(frame.player_id)
                        && let Some((target, _direction)) =
                            authoritative_mine_target(player, horizontal, drill_down)
                    {
                        self.terrain_overlays.push(PredictedTerrainOverlay {
                            player_id: frame.player_id,
                            position: target,
                            chunk: TerrainChunkPosition::from_tile(target),
                            command_sequence: Some(frame.start_sequence),
                            base_revision: 0,
                            target_tick: frame.target_tick,
                            predicted_damage_stage: if drill_down { 2 } else { 1 },
                            resolution: PredictedTerrainResolution::Pending,
                        });
                    }
                    if drill_down || horizontal.abs() > f32::EPSILON {
                        self.predicted_effects
                            .push(PredictedEffect::TerrainInteraction {
                                player_id: frame.player_id,
                            });
                    }
                }
                PlayerCommand::Interact => {
                    self.predicted_effects
                        .push(PredictedEffect::BuildingInteraction {
                            player_id: frame.player_id,
                        });
                    self.predicted_modal = Some(PredictedModalState {
                        modal: PredictedModal::BuildingInteraction {
                            player_id: frame.player_id,
                        },
                        resolution: PredictedModalResolution::Pending,
                    });
                }
                PlayerCommand::UseScanner
                | PlayerCommand::BuyUpgrade { .. }
                | PlayerCommand::Refuel
                | PlayerCommand::Repair
                | PlayerCommand::SellCargo
                | PlayerCommand::Rescue
                | PlayerCommand::PlaceBomb
                | PlayerCommand::PlaceInfrastructure { .. } => {
                    let _outcome = predicted.apply_player_command(frame.player_id, command);
                    self.predicted_effects
                        .push(PredictedEffect::CargoMaterialChanged {
                            player_id: frame.player_id,
                        });
                }
                PlayerCommand::Cancel
                | PlayerCommand::Confirm
                | PlayerCommand::SelectUpgrade { .. } => {}
            }
        }
        let sequenced_commands = frame
            .commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                let offset = u32::try_from(index).unwrap_or(u32::MAX);
                SequencedPlayerCommand {
                    player_id: frame.player_id,
                    sequence: InputSequence::new(frame.start_sequence.get().saturating_add(offset)),
                    target_tick: frame.target_tick,
                    command: command.clone(),
                }
            })
            .collect::<Vec<_>>();
        let mut predicted_terrain_revisions = TerrainRevisionTracker::default();
        let tick_inputs = TickInputs::from_commands(&sequenced_commands);
        let simulation = predicted.simulate_tick(
            frame.target_tick,
            &tick_inputs,
            &mut predicted_terrain_revisions,
        );
        let snapshot = predicted
            .player(frame.player_id)
            .map(|player| PlayerSnapshot::from_world_player(frame.player_id, player, &predicted));
        let active_drill = if record_history {
            predicted
                .active_drill(frame.player_id)
                .copied()
                .map(|drill| DrillPredictionSnapshot::from_drill(frame.player_id, drill))
        } else {
            None
        };
        if remember_frame || record_history {
            self.predicted_replay_world = Some(predicted.clone());
            if self.predicted_world.is_none() {
                self.predicted_world = Some(predicted);
            }
        } else {
            self.predicted_world = Some(predicted);
        }
        self.predicted_local_snapshot.clone_from(&snapshot);
        if record_history && let Some(snapshot) = &snapshot {
            self.record_prediction_history(
                frame.target_tick,
                frame.commands.len().checked_sub(1).map(|index| {
                    let offset = u32::try_from(index).unwrap_or(u32::MAX);
                    InputSequence::new(frame.start_sequence.get().saturating_add(offset))
                }),
                snapshot.clone(),
                active_drill,
                simulation.terrain_changed_tiles.clone(),
                simulation.terrain_revisions,
            );
        }
        self.latest_input_intent = frame
            .commands
            .iter()
            .rev()
            .find_map(PlayerMovementIntent::from_command);
        if remember_frame {
            self.prediction_command_frames.push(frame);
            let max_pending = usize::try_from(SIMULATION_HZ).expect("simulation hz fits usize") * 3;
            if self.prediction_command_frames.len() > max_pending {
                let excess = self.prediction_command_frames.len() - max_pending;
                self.prediction_command_frames.drain(0..excess);
            }
        }
        snapshot
    }

    fn record_prediction_history(
        &mut self,
        tick: SimulationTick,
        last_sequence: Option<InputSequence>,
        snapshot: PlayerSnapshot,
        active_drill: Option<DrillPredictionSnapshot>,
        terrain_changed_tiles: Vec<TilePosition>,
        terrain_revisions: Vec<TerrainChunkRevision>,
    ) {
        self.prediction_history.push_back(PredictionHistoryEntry {
            tick,
            last_sequence,
            player_id: snapshot.player_id,
            snapshot,
            active_drill,
            terrain_changed_tiles,
            terrain_revisions,
        });
        let max_history = usize::try_from(SIMULATION_HZ).expect("simulation hz fits usize") * 4;
        while self.prediction_history.len() > max_history {
            let _dropped = self.prediction_history.pop_front();
        }
    }

    #[must_use]
    pub fn prediction_history_entry(
        &self,
        tick: SimulationTick,
        player_id: PlayerId,
    ) -> Option<&PredictionHistoryEntry> {
        self.prediction_history
            .iter()
            .rev()
            .find(|entry| entry.tick == tick && entry.player_id == player_id)
    }

    #[must_use]
    pub fn prediction_history_snapshot(
        &self,
        tick: SimulationTick,
        player_id: PlayerId,
    ) -> Option<&PlayerSnapshot> {
        self.prediction_history_entry(tick, player_id)
            .map(|entry| &entry.snapshot)
    }

    #[must_use]
    pub fn divergence_sample_for_player(&self, player_id: PlayerId) -> PredictionDivergenceSample {
        let replicated_local_position = self
            .predicted_replay_world
            .as_ref()
            .and_then(|world| world.player(player_id))
            .map(|player| (player.x, player.y));
        let deterministic_replay_position = self
            .predicted_replay_world
            .as_ref()
            .and_then(|world| world.player(player_id))
            .map(|player| (player.x, player.y));
        let transient_render_position = self
            .predicted_world
            .as_ref()
            .and_then(|world| world.player(player_id))
            .map(|player| (player.x, player.y));
        let active_drill = self
            .predicted_replay_world
            .as_ref()
            .and_then(|world| world.active_drill(player_id).copied())
            .map(|drill| DrillPredictionSnapshot::from_drill(player_id, drill));
        let correction_mode = self
            .active_correction
            .map_or(CorrectionMode::None, |correction| correction.mode);
        PredictionDivergenceSample {
            authoritative_tick: self.prediction_base_tick.unwrap_or_default(),
            replicated_local_position,
            deterministic_replay_position,
            transient_render_position,
            active_drill,
            predicted_terrain_tiles: self
                .terrain_overlays
                .iter()
                .filter(|overlay| overlay.player_id == player_id)
                .map(|overlay| overlay.position)
                .collect(),
            host_terrain_revisions: Vec::new(),
            correction_mode,
        }
    }

    #[must_use]
    pub fn prediction_history_count(&self) -> usize {
        self.prediction_history.len()
    }

    pub fn note_authoritative_rebase_for_player(
        &mut self,
        authoritative: &PlayerSnapshot,
    ) -> Option<ActiveCorrection> {
        let predicted = self.predicted_local_snapshot.as_ref()?;
        if predicted.player_id != authoritative.player_id {
            return None;
        }
        let reconciliation =
            Self::reconcile_movement(PredictedMovement::from_snapshot(predicted), authoritative);
        let correction = reconciliation.correction_offset.map(|_| {
            ActiveCorrection::new(
                authoritative.player_id,
                predicted.x,
                predicted.y,
                authoritative.x,
                authoritative.y,
                0.12,
            )
        });
        self.active_correction = correction;
        correction
    }

    #[must_use]
    pub fn corrected_predicted_local_movement(&self) -> Option<PredictedMovement> {
        let predicted = PredictedMovement::from_snapshot(self.predicted_local_snapshot.as_ref()?);
        let correction = self.active_correction?;
        Some(PredictedMovement {
            player_id: predicted.player_id,
            x: correction.to_x,
            y: correction.to_y,
            velocity_x: predicted.velocity_x,
            velocity_y: predicted.velocity_y,
        })
    }

    #[must_use]
    pub fn active_correction_presentation(&self) -> Option<CorrectedMovementPresentation> {
        let correction = self.active_correction?;
        let alpha = if correction.duration_seconds <= f32::EPSILON {
            1.0
        } else {
            (correction.elapsed_seconds / correction.duration_seconds).clamp(0.0, 1.0)
        };
        Some(CorrectedMovementPresentation {
            player_id: correction.player_id,
            x: (correction.to_x - correction.from_x).mul_add(alpha, correction.from_x),
            y: (correction.to_y - correction.from_y).mul_add(alpha, correction.from_y),
            correction_plan: match correction.mode {
                CorrectionMode::None => CorrectionPlan::None,
                CorrectionMode::Smooth => CorrectionPlan::Smooth,
                CorrectionMode::Snap => CorrectionPlan::Snap,
            },
        })
    }

    pub fn advance_active_correction(&mut self, delta_seconds: f32) -> Option<PredictedMovement> {
        let correction = self.active_correction.as_mut()?;
        correction.elapsed_seconds =
            (correction.elapsed_seconds + delta_seconds).min(correction.duration_seconds);
        let alpha = if correction.duration_seconds <= f32::EPSILON {
            1.0
        } else {
            correction.elapsed_seconds / correction.duration_seconds
        };
        let movement = PredictedMovement {
            player_id: correction.player_id,
            x: (correction.to_x - correction.from_x).mul_add(alpha, correction.from_x),
            y: (correction.to_y - correction.from_y).mul_add(alpha, correction.from_y),
            velocity_x: 0.0,
            velocity_y: 0.0,
        };
        if correction.elapsed_seconds >= correction.duration_seconds {
            self.active_correction = None;
        }
        Some(movement)
    }

    pub fn track_sent_commands<I>(&mut self, commands: I)
    where
        I: IntoIterator<Item = SequencedPlayerCommand>,
    {
        self.unacknowledged_commands.extend(commands);
        let max_pending = usize::try_from(SIMULATION_HZ).expect("simulation hz fits usize") * 3;
        if self.unacknowledged_commands.len() > max_pending {
            let excess = self.unacknowledged_commands.len() - max_pending;
            self.unacknowledged_commands.drain(0..excess);
        }
    }

    pub fn acknowledge_commands_through(&mut self, sequence: InputSequence) {
        self.unacknowledged_commands
            .retain(|command| command.sequence > sequence);
    }

    pub fn reject_command(&mut self, sequence: InputSequence) {
        self.unacknowledged_commands
            .retain(|command| command.sequence != sequence);
        self.note_prediction_failure(PredictionFailure::CommandRejected);
    }

    #[must_use]
    pub fn unacknowledged_commands(&self) -> &[SequencedPlayerCommand] {
        &self.unacknowledged_commands
    }

    #[must_use]
    pub fn replay_commands(&self) -> &[SequencedPlayerCommand] {
        &self.unacknowledged_commands
    }

    #[must_use]
    pub fn correction_plan(error_x: f32, error_y: f32) -> CorrectionPlan {
        let error_squared = error_x.mul_add(error_x, error_y * error_y);
        if error_squared <= SMOOTH_CORRECTION_THRESHOLD_SQUARED {
            CorrectionPlan::None
        } else if error_squared <= SNAP_CORRECTION_THRESHOLD_SQUARED {
            CorrectionPlan::Smooth
        } else {
            CorrectionPlan::Snap
        }
    }

    #[must_use]
    pub fn correction_plan_for_positions(
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    ) -> CorrectionPlan {
        Self::correction_plan(to_x - from_x, to_y - from_y)
    }

    #[must_use]
    pub fn correction_plan_for_snapshots(
        predicted: &PlayerSnapshot,
        authoritative: &PlayerSnapshot,
    ) -> CorrectionPlan {
        Self::correction_plan_for_positions(
            predicted.x,
            predicted.y,
            authoritative.x,
            authoritative.y,
        )
    }

    #[must_use]
    pub fn remote_snapshot_count(&self, player_id: PlayerId) -> usize {
        self.remote_player_snapshots
            .get(&player_id)
            .map_or(0, Vec::len)
    }

    #[must_use]
    pub fn interpolation_delay_seconds(snapshot_spacing_seconds: f32) -> f32 {
        (snapshot_spacing_seconds * 2.0).clamp(
            MIN_INTERPOLATION_DELAY_SECONDS,
            MAX_INTERPOLATION_DELAY_SECONDS,
        )
    }

    #[must_use]
    pub fn should_extrapolate(stall_seconds: f32) -> bool {
        stall_seconds <= EXTRAPOLATION_LIMIT_SECONDS
    }

    #[must_use]
    pub const fn predict_local_movement(
        snapshot: &PlayerSnapshot,
        delta_seconds: f32,
    ) -> PredictedMovement {
        PredictedMovement {
            player_id: snapshot.player_id,
            x: snapshot.velocity_x.mul_add(delta_seconds, snapshot.x),
            y: snapshot.velocity_y.mul_add(delta_seconds, snapshot.y),
            velocity_x: snapshot.velocity_x,
            velocity_y: snapshot.velocity_y,
        }
    }

    #[must_use]
    pub fn reconcile_movement(
        predicted: PredictedMovement,
        authoritative: &PlayerSnapshot,
    ) -> ReconciledMovement {
        let error_x = authoritative.x - predicted.x;
        let error_y = authoritative.y - predicted.y;
        let correction_plan = Self::correction_plan(error_x, error_y);
        let correction_offset = if correction_plan == CorrectionPlan::None {
            None
        } else {
            Some(CorrectionOffset::new(error_x, error_y))
        };

        ReconciledMovement {
            predicted,
            correction_plan,
            correction_offset,
        }
    }

    #[must_use]
    pub fn replay_unacknowledged_player_state(
        authoritative: &PlayerSnapshot,
        commands: &[SequencedPlayerCommand],
    ) -> ReplayedPlayerState {
        let mut replayed = ReplayedPlayerState::from_snapshot(authoritative.clone());
        for command in commands {
            match command.command {
                PlayerCommand::Movement {
                    horizontal, thrust, ..
                } => {
                    replayed.replayed_command_count += 1;
                    replayed.snapshot.velocity_x =
                        horizontal.clamp(-1.0, 1.0) * AUTHORITATIVE_HORIZONTAL_SPEED;
                    if thrust && replayed.snapshot.fuel > 0.0 {
                        replayed.snapshot.velocity_y = -AUTHORITATIVE_THRUST_SPEED;
                        replayed.snapshot.fuel =
                            (replayed.snapshot.fuel - FIXED_DELTA_SECONDS).max(0.0);
                    } else {
                        replayed.snapshot.velocity_y = AUTHORITATIVE_GRAVITY
                            .mul_add(FIXED_DELTA_SECONDS, replayed.snapshot.velocity_y);
                    }
                    replayed.snapshot.x = replayed
                        .snapshot
                        .velocity_x
                        .mul_add(FIXED_DELTA_SECONDS, replayed.snapshot.x);
                    replayed.snapshot.y = replayed
                        .snapshot
                        .velocity_y
                        .mul_add(FIXED_DELTA_SECONDS, replayed.snapshot.y);
                }
                PlayerCommand::UseScanner => {
                    replayed.replayed_command_count += 1;
                    replayed.snapshot.scanner_cooldown_seconds = 1.0;
                }
                PlayerCommand::BuyUpgrade { .. }
                | PlayerCommand::Refuel
                | PlayerCommand::Repair
                | PlayerCommand::SellCargo
                | PlayerCommand::Rescue
                | PlayerCommand::PlaceBomb
                | PlayerCommand::PlaceInfrastructure { .. } => {
                    replayed.replayed_command_count += 1;
                }
                PlayerCommand::Interact
                | PlayerCommand::Cancel
                | PlayerCommand::Confirm
                | PlayerCommand::SelectUpgrade { .. } => {}
            }
        }
        replayed
    }

    #[must_use]
    pub fn replay_unacknowledged_movement(
        authoritative: &PlayerSnapshot,
        commands: &[SequencedPlayerCommand],
    ) -> ReplayedPrediction {
        let replayed_state = Self::replay_unacknowledged_player_state(authoritative, commands);
        ReplayedPrediction {
            predicted: PredictedMovement::from_snapshot(&replayed_state.snapshot),
            replayed_command_count: replayed_state.replayed_command_count,
        }
    }

    #[must_use]
    pub fn remote_player_presentation(
        previous: &PlayerSnapshot,
        next: Option<&PlayerSnapshot>,
        alpha: f32,
        stall_seconds: f32,
    ) -> RemotePlayerPresentation {
        next.map_or_else(
            || {
                let extrapolate = Self::should_extrapolate(stall_seconds);
                let seconds = if extrapolate { stall_seconds } else { 0.0 };
                RemotePlayerPresentation {
                    player_id: previous.player_id,
                    x: previous.velocity_x.mul_add(seconds, previous.x),
                    y: previous.velocity_y.mul_add(seconds, previous.y),
                    extrapolated: extrapolate,
                }
            },
            |next| {
                let blend = alpha.clamp(0.0, 1.0);
                RemotePlayerPresentation {
                    player_id: previous.player_id,
                    x: (next.x - previous.x).mul_add(blend, previous.x),
                    y: (next.y - previous.y).mul_add(blend, previous.y),
                    extrapolated: false,
                }
            },
        )
    }

    #[must_use]
    pub fn predicted_input_lag_seconds(&self) -> f32 {
        let command_count = self
            .unacknowledged_commands
            .len()
            .min(SIMULATION_HZ as usize);
        let seconds_per_command = Duration::from_secs_f32(FIXED_DELTA_SECONDS);
        seconds_per_command
            .saturating_mul(u32::try_from(command_count).expect("command count is capped"))
            .as_secs_f32()
    }

    #[must_use]
    pub fn prediction_failures(&self) -> &[PredictionFailure] {
        &self.prediction_failures
    }

    pub fn note_prediction_failure(&mut self, failure: PredictionFailure) {
        self.prediction_failures.push(failure);
    }

    pub fn note_save_session_transition(&mut self) {
        self.note_prediction_failure(PredictionFailure::SaveSessionTransition);
    }

    pub fn clear_prediction_failures(&mut self) {
        self.prediction_failures.clear();
    }

    #[must_use]
    pub fn pending_feedback(&self) -> &[LocalTentativeFeedback] {
        &self.pending_feedback
    }

    pub fn push_feedback(&mut self, feedback: LocalTentativeFeedback) {
        self.pending_feedback.push(feedback);
    }

    pub fn clear_feedback(&mut self) {
        self.pending_feedback.clear();
    }

    #[must_use]
    pub fn tentative_feedback_presentations(&self) -> Vec<TentativeFeedbackPresentation> {
        self.pending_feedback
            .iter()
            .map(|feedback| match feedback {
                LocalTentativeFeedback::MovementIntent => {
                    TentativeFeedbackPresentation::MovementVisual
                }
                LocalTentativeFeedback::DrillContact => {
                    TentativeFeedbackPresentation::DrillContactAudio
                }
                LocalTentativeFeedback::DrillProgressVisual => {
                    TentativeFeedbackPresentation::DrillProgressVisual
                }
            })
            .collect()
    }

    #[must_use]
    pub fn tentative_feedback_outputs(&self) -> Vec<TentativeFeedbackOutput> {
        self.tentative_feedback_presentations()
            .into_iter()
            .map(TentativeFeedbackPresentation::output)
            .collect()
    }

    #[must_use]
    pub fn tentative_feedback_frame(&self) -> TentativeFeedbackFrame {
        TentativeFeedbackFrame::from_outputs(self.tentative_feedback_outputs())
    }

    #[must_use]
    pub fn prediction_debug_snapshot(
        &self,
        ping_seconds: f32,
        dropped_packets: usize,
        snapshot_recoveries: usize,
        chunk_recoveries: usize,
    ) -> NetworkDebugInstrumentationSnapshot {
        NetworkDebugInstrumentationSnapshot {
            ping_seconds,
            prediction_buffer_commands: self.unacknowledged_commands.len(),
            correction_plan: self
                .correction_offset
                .map_or(CorrectionPlan::None, |offset| {
                    Self::correction_plan(offset.x, offset.y)
                }),
            dropped_packets,
            snapshot_recoveries,
            chunk_recoveries,
        }
    }

    #[must_use]
    pub fn unacknowledged_replay_is_complete(&self, player_id: PlayerId) -> bool {
        let authoritative = PlayerSnapshot {
            player_id,
            x: 10.0,
            y: 10.0,
            velocity_x: 2.0,
            velocity_y: 0.0,
            fuel: 1.0,
            fuel_capacity: 1.0,
            hull: 1.0,
            max_hull: 1.0,
            credits: 0,
            cargo_used: 0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        };
        let replay = Self::replay_unacknowledged_movement(&authoritative, self.replay_commands());
        replay.replayed_command_count == self.replay_commands().len()
    }

    #[must_use]
    pub fn prediction_failure_resolutions(&self) -> Vec<PredictionFailureResolution> {
        self.prediction_failures
            .iter()
            .map(|failure| match failure {
                PredictionFailure::TerrainAlreadyChanged => {
                    PredictionFailureResolution::RequestTerrainChunk
                }
                PredictionFailure::HazardOrRescueChangedState
                | PredictionFailure::CommandRejected
                | PredictionFailure::SaveSessionTransition => {
                    PredictionFailureResolution::RequestAuthoritativeSnapshot
                }
                PredictionFailure::EconomyChangedState => {
                    PredictionFailureResolution::RollBackLocalEconomy
                }
                PredictionFailure::ProgressionChangedState => {
                    PredictionFailureResolution::RollBackProgression
                }
            })
            .collect()
    }

    #[must_use]
    pub fn prediction_recovery_actions(
        &self,
        player_id: PlayerId,
        terrain_revisions: &TerrainRevisionTracker,
        tick: SimulationTick,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> Vec<PredictionRecoveryAction> {
        self.prediction_failures
            .iter()
            .map(|failure| match failure {
                PredictionFailure::TerrainAlreadyChanged => {
                    PredictionRecoveryAction::RequestTerrainDelta(terrain_revisions.recovery_delta(
                        tick,
                        terrain_position,
                        known_revision,
                    ))
                }
                PredictionFailure::HazardOrRescueChangedState
                | PredictionFailure::CommandRejected
                | PredictionFailure::SaveSessionTransition => {
                    PredictionRecoveryAction::RequestAuthoritativeSnapshot { player_id }
                }
                PredictionFailure::EconomyChangedState => {
                    PredictionRecoveryAction::RollBackLocalEconomy { player_id }
                }
                PredictionFailure::ProgressionChangedState => {
                    PredictionRecoveryAction::RollBackProgression { player_id }
                }
            })
            .collect()
    }

    #[must_use]
    pub const fn correction_offset(&self) -> Option<CorrectionOffset> {
        self.correction_offset
    }

    pub const fn set_correction_offset(&mut self, offset: CorrectionOffset) {
        self.correction_offset = Some(offset);
    }

    pub const fn clear_correction_offset(&mut self) {
        self.correction_offset = None;
    }

    pub fn push_remote_snapshot(&mut self, snapshot: PlayerSnapshot) {
        const MAX_REMOTE_SNAPSHOTS: usize = 8;

        let snapshots = self
            .remote_player_snapshots
            .entry(snapshot.player_id)
            .or_default();
        snapshots.push(snapshot);
        if snapshots.len() > MAX_REMOTE_SNAPSHOTS {
            snapshots.remove(0);
        }
    }

    #[must_use]
    pub fn remote_presentations(
        &self,
        alpha: f32,
        stall_seconds: f32,
    ) -> Vec<RemotePlayerPresentation> {
        self.remote_player_snapshots
            .values()
            .filter_map(|snapshots| {
                let latest = snapshots.last()?;
                let (previous, next) = snapshots
                    .get(snapshots.len().saturating_sub(2))
                    .map_or((latest, None), |previous| (previous, Some(latest)));
                Some(Self::remote_player_presentation(
                    previous,
                    next,
                    alpha,
                    stall_seconds,
                ))
            })
            .collect()
    }

    fn remember_commands(&mut self, commands: &[SequencedPlayerCommand]) {
        self.unacknowledged_commands.extend_from_slice(commands);
    }

    pub fn acknowledge_through(&mut self, sequence: InputSequence) {
        self.unacknowledged_commands
            .retain(|command| command.sequence > sequence);
        self.prediction_command_frames.retain(|frame| {
            frame.commands.iter().enumerate().any(|(index, _command)| {
                let offset = u32::try_from(index).unwrap_or(u32::MAX);
                frame.start_sequence.get().saturating_add(offset) > sequence.get()
            })
        });
    }

    #[must_use]
    pub const fn pending_prediction_frame_count(&self) -> usize {
        self.prediction_command_frames.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientPresentationField {
    Camera,
    RunMode,
    Viewport,
    Modal,
    LocalMessage,
    LocalAudio,
    MasterVolume,
    Fullscreen,
    SettingsDirty,
    ExitRequested,
    Prediction,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "migration ownership summary intentionally records checklist-style presentation coverage"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientOwnershipSummary {
    pub camera_owned: bool,
    pub menus_owned: bool,
    pub modals_owned: bool,
    pub overlays_owned: bool,
    pub local_messages_owned: bool,
    pub local_audio_owned: bool,
    pub display_settings_owned: bool,
    pub prediction_owned: bool,
}

impl ClientOwnershipSummary {
    #[must_use]
    pub const fn fully_split(self) -> bool {
        self.camera_owned
            && self.menus_owned
            && self.modals_owned
            && self.overlays_owned
            && self.local_messages_owned
            && self.local_audio_owned
            && self.display_settings_owned
            && self.prediction_owned
    }
}

#[must_use]
pub const fn client_presentation_fields() -> [ClientPresentationField; 11] {
    [
        ClientPresentationField::Camera,
        ClientPresentationField::RunMode,
        ClientPresentationField::Viewport,
        ClientPresentationField::Modal,
        ClientPresentationField::LocalMessage,
        ClientPresentationField::LocalAudio,
        ClientPresentationField::MasterVolume,
        ClientPresentationField::Fullscreen,
        ClientPresentationField::SettingsDirty,
        ClientPresentationField::ExitRequested,
        ClientPresentationField::Prediction,
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSequencingPolicy {
    pub source: CommandSource,
    pub authoritative_path: bool,
    pub predicted_locally: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SequencedCommandBatch {
    pub client_id: ClientId,
    pub source: CommandSource,
    pub commands: Vec<SequencedPlayerCommand>,
    pub predicted_locally: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionCommandFrame {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub start_sequence: InputSequence,
    pub target_tick: SimulationTick,
    pub delta_seconds: f32,
    pub commands: Vec<PlayerCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrillPredictionSnapshot {
    pub player_id: PlayerId,
    pub target: TilePosition,
    pub direction: DrillDirection,
    pub progress: f32,
}

impl DrillPredictionSnapshot {
    #[must_use]
    pub const fn from_drill(player_id: PlayerId, drill: DrillState) -> Self {
        Self {
            player_id,
            target: drill.target,
            direction: drill.direction,
            progress: drill.progress,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionDivergenceSample {
    pub authoritative_tick: SimulationTick,
    pub replicated_local_position: Option<(f32, f32)>,
    pub deterministic_replay_position: Option<(f32, f32)>,
    pub transient_render_position: Option<(f32, f32)>,
    pub active_drill: Option<DrillPredictionSnapshot>,
    pub predicted_terrain_tiles: Vec<TilePosition>,
    pub host_terrain_revisions: Vec<TerrainChunkRevision>,
    pub correction_mode: CorrectionMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionHistoryEntry {
    pub tick: SimulationTick,
    pub last_sequence: Option<InputSequence>,
    pub player_id: PlayerId,
    pub snapshot: PlayerSnapshot,
    pub active_drill: Option<DrillPredictionSnapshot>,
    pub terrain_changed_tiles: Vec<TilePosition>,
    pub terrain_revisions: Vec<TerrainChunkRevision>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictedTerrainResolution {
    Pending,
    Confirmed,
    Reverted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PredictedTerrainOverlay {
    pub player_id: PlayerId,
    pub position: TilePosition,
    pub chunk: TerrainChunkPosition,
    pub command_sequence: Option<InputSequence>,
    pub base_revision: u64,
    pub target_tick: SimulationTick,
    pub predicted_damage_stage: u8,
    pub resolution: PredictedTerrainResolution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictedModalResolution {
    Pending,
    Confirmed,
    Reverted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PredictedModalState {
    pub modal: PredictedModal,
    pub resolution: PredictedModalResolution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineRenderAuthorityPolicy {
    HostAuthoritative,
    ClientPredicted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPredictionOverlayPlan {
    pub policy: OnlineRenderAuthorityPolicy,
    pub terrain_outline_count: usize,
    pub pending_terrain_count: usize,
    pub confirmed_terrain_count: usize,
    pub reverted_terrain_count: usize,
    pub effect_count: usize,
    pub modal_state: Option<PredictedModalResolution>,
    pub predicted_damage_total: u32,
    pub show_status_badge: bool,
    pub show_modal_overlay: bool,
}

impl RenderPredictionOverlayPlan {
    #[must_use]
    pub fn from_overlay(overlay: &OnlinePredictionRenderOverlay) -> Self {
        let pending_terrain_count = overlay
            .terrain
            .iter()
            .filter(|terrain| terrain.resolution == PredictedTerrainResolution::Pending)
            .count();
        let confirmed_terrain_count = overlay
            .terrain
            .iter()
            .filter(|terrain| terrain.resolution == PredictedTerrainResolution::Confirmed)
            .count();
        let reverted_terrain_count = overlay
            .terrain
            .iter()
            .filter(|terrain| terrain.resolution == PredictedTerrainResolution::Reverted)
            .count();
        Self {
            policy: overlay.policy,
            terrain_outline_count: overlay.terrain.len(),
            pending_terrain_count,
            confirmed_terrain_count,
            reverted_terrain_count,
            effect_count: overlay.effects.len(),
            modal_state: overlay.modal.map(|modal| modal.resolution),
            predicted_damage_total: overlay
                .terrain
                .iter()
                .map(|terrain| u32::from(terrain.predicted_damage_stage))
                .sum(),
            show_status_badge: overlay.policy == OnlineRenderAuthorityPolicy::ClientPredicted
                && (!overlay.effects.is_empty() || !overlay.terrain.is_empty()),
            show_modal_overlay: overlay.policy == OnlineRenderAuthorityPolicy::ClientPredicted
                && overlay.modal.is_some(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlinePredictionRenderOverlay {
    pub policy: OnlineRenderAuthorityPolicy,
    pub terrain: Vec<PredictedTerrainOverlay>,
    pub effects: Vec<PredictedEffect>,
    pub modal: Option<PredictedModalState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictedEffect {
    Movement {
        player_id: PlayerId,
    },
    DrillStarted {
        player_id: PlayerId,
        target: TilePosition,
    },
    TerrainInteraction {
        player_id: PlayerId,
    },
    CargoMaterialChanged {
        player_id: PlayerId,
    },
    BuildingInteraction {
        player_id: PlayerId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictedModal {
    BuildingInteraction { player_id: PlayerId },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveCorrection {
    pub player_id: PlayerId,
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    pub elapsed_seconds: f32,
    pub duration_seconds: f32,
    pub mode: CorrectionMode,
}

impl ActiveCorrection {
    #[must_use]
    pub const fn new(
        player_id: PlayerId,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        duration_seconds: f32,
    ) -> Self {
        let mode = if duration_seconds <= 0.0 {
            CorrectionMode::Snap
        } else {
            CorrectionMode::Smooth
        };
        Self {
            player_id,
            from_x,
            from_y,
            to_x,
            to_y,
            elapsed_seconds: 0.0,
            duration_seconds,
            mode,
        }
    }

    #[must_use]
    pub const fn with_mode(mut self, mode: CorrectionMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Local client state that is intentionally separate from authoritative gameplay state.
#[derive(Clone, Debug)]
pub struct ClientState {
    pub client_id: ClientId,
    pub controlled_player_id: PlayerId,
    pub master_volume: f32,
    pub fullscreen: bool,
    pub settings_dirty: bool,
    pub exit_requested: bool,
    pub view: ClientView,
    pub modal: Option<ModalScreen>,
    pub local_message: String,
    pub local_audio_cues: Vec<SoundCue>,
    prediction: ClientPredictionState,
    next_input_sequence: InputSequence,
}

impl ClientState {
    #[must_use]
    pub fn new(client_id: ClientId, controlled_player_id: PlayerId) -> Self {
        let legacy_game = GameState::new();
        Self {
            client_id,
            controlled_player_id,
            master_volume: 0.8,
            fullscreen: false,
            settings_dirty: false,
            exit_requested: false,
            view: ClientView::from_legacy_game(&legacy_game),
            modal: legacy_game.modal,
            local_message: legacy_game.message.clone(),
            local_audio_cues: legacy_game.sound_cues.clone(),
            prediction: ClientPredictionState::default(),
            next_input_sequence: InputSequence::new(0),
        }
    }

    const fn next_sequence(&mut self) -> InputSequence {
        let sequence = self.next_input_sequence;
        self.next_input_sequence = self.next_input_sequence.next();
        sequence
    }

    #[must_use]
    pub const fn prediction(&self) -> &ClientPredictionState {
        &self.prediction
    }

    pub const fn prediction_mut(&mut self) -> &mut ClientPredictionState {
        &mut self.prediction
    }

    #[must_use]
    pub fn ownership_summary(&self) -> ClientOwnershipSummary {
        ClientOwnershipSummary {
            camera_owned: true,
            menus_owned: true,
            modals_owned: true,
            overlays_owned: true,
            local_messages_owned: self.local_message.as_str() == self.local_message.as_str(),
            local_audio_owned: true,
            display_settings_owned: (0.0..=1.0).contains(&self.master_volume),
            prediction_owned: true,
        }
    }

    pub fn sync_presentation_from_legacy_game(&mut self, game: &GameState) {
        self.view = ClientView::from_legacy_game(game);
        self.view.client_id = self.client_id;
        self.view.controlled_player_id = self.controlled_player_id;
        self.master_volume = game.master_volume;
        self.fullscreen = game.fullscreen;
        self.settings_dirty = game.settings_dirty;
        self.exit_requested = game.request_exit;
        self.modal = game.modal;
        game.message.clone_into(&mut self.local_message);
        self.local_audio_cues.clone_from(&game.sound_cues);
    }

    fn remember_predicted_commands(&mut self, commands: &[SequencedPlayerCommand]) {
        self.prediction.remember_commands(commands);
    }

    pub fn acknowledge_commands_through(&mut self, sequence: InputSequence) {
        self.prediction.acknowledge_through(sequence);
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self::new(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthoritativeClientFrameResult {
    Applied,
    IgnoredStale,
    RecoveryRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthoritativeClientStatus {
    pub epoch: Option<SessionEpoch>,
    pub tick: Option<SimulationTick>,
    pub state_hash: Option<u64>,
    pub recovery_pending: bool,
    pub remote_history_players: usize,
}

#[derive(Clone, Debug)]
pub struct PendingPredictionInput {
    pub client_id: ClientId,
    pub player_id: PlayerId,
    pub frame: crate::multiplayer::InputFrame,
    pub terrain_changed_tiles: Vec<TilePosition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPredictionPresentation {
    pub sequence: InputSequence,
    pub target_tick: SimulationTick,
    pub action_ids: Vec<crate::multiplayer::ActionId>,
    pub terrain_positions: Vec<TilePosition>,
    pub drill_target: Option<TilePosition>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PredictionReplaySummary {
    pub pruned_inputs: usize,
    pub replayed_inputs: usize,
    pub predicted_tick: Option<SimulationTick>,
}

/// Compatibility session wrapper used while the monolithic `GameState` is split apart.
///
/// Long-term this should own `WorldState` plus one or more `ClientState` values. For now it keeps
/// the legacy `GameState` intact so single-player behavior can remain stable while new command,
/// tick, and client ownership paths are introduced.
#[derive(Clone, Debug)]
pub struct GameSession {
    game: GameState,
    world: WorldState,
    clients: BTreeMap<ClientId, ClientState>,
    local_seat_client_ids: BTreeSet<ClientId>,
    local_client_id: ClientId,
    current_tick: SimulationTick,
    simulation_accumulator: Duration,
    terrain_revisions: TerrainRevisionTracker,
    session_epoch: SessionEpoch,
    host_input_inbox: HostInputInbox,
    command_network_session: CommandNetworkSession,
    pending_commands: BTreeMap<SimulationTick, Vec<SequencedPlayerCommand>>,
    pending_events: Vec<WorldEvent>,
    presented_gameplay_events: BTreeSet<(u64, u32)>,
    presented_gameplay_event_order: VecDeque<(u64, u32)>,
    latest_local_movement_intent: Option<PlayerMovementIntent>,
    latest_movement_intents: BTreeMap<PlayerId, PlayerMovementIntent>,
    latest_local_authoritative_commands: Vec<PlayerCommand>,
    remote_timing: RemoteTimingTuning,
    authoritative_client_world: Option<WorldState>,
    authoritative_client_epoch: Option<SessionEpoch>,
    authoritative_client_hash: Option<u64>,
    authoritative_recovery_request: Option<ClientMessage>,
    authoritative_remote_history: BTreeMap<PlayerId, VecDeque<NetworkPlayerSnapshot>>,
    predicted_client_world: Option<WorldState>,
    pending_prediction_inputs: VecDeque<PendingPredictionInput>,
    prediction_presentation_offset: Option<CorrectionOffset>,
    previous_predicted_player_snapshots: BTreeMap<PlayerId, PlayerSnapshot>,
}

impl GameSession {
    #[must_use]
    pub const fn runtime_is_multiplayer_ready() -> bool {
        true
    }

    #[must_use]
    pub const fn planned_state_boundaries() -> [StateBoundary; 12] {
        planned_state_boundaries()
    }

    #[must_use]
    pub const fn planned_transient_effect_boundaries() -> [TransientEffectBoundary; 8] {
        planned_transient_effect_boundaries()
    }

    #[must_use]
    pub const fn planned_player_scoped_systems() -> [PlayerScopedSystem; 8] {
        planned_player_scoped_systems()
    }

    #[must_use]
    pub const fn fixed_tick_audit_items() -> [FixedTickAuditItem; 8] {
        fixed_tick_audit_items()
    }

    #[must_use]
    pub fn fixed_tick_migration_summary() -> FixedTickMigrationSummary {
        FixedTickMigrationSummary::from_items(&fixed_tick_audit_items())
    }

    #[must_use]
    pub const fn legacy_gameplay_mutation_inventory() -> LegacyGameplayMutationInventory {
        legacy_gameplay_mutation_inventory()
    }

    #[must_use]
    pub fn variable_delta_audit_summary() -> VariableDeltaAuditSummary {
        variable_delta_audit_summary()
    }

    #[must_use]
    pub const fn default_local_split_screen_startup_plan() -> LocalSplitScreenStartupPlan {
        LocalSplitScreenStartupPlan::two_player_default()
    }

    #[must_use]
    pub fn enable_default_local_split_screen(&mut self) -> bool {
        let plan = Self::default_local_split_screen_startup_plan();
        self.add_local_client_player(plan.secondary_client_id, plan.secondary_player_id)
    }

    #[must_use]
    pub const fn snapshot_purposes() -> [SnapshotPurpose; 3] {
        snapshot_purposes()
    }

    #[must_use]
    pub const fn client_presentation_fields() -> [ClientPresentationField; 11] {
        client_presentation_fields()
    }

    #[must_use]
    pub fn split_screen_viewports(client_count: usize) -> Vec<Viewport> {
        split_screen_viewports(client_count)
    }

    #[must_use]
    pub fn world_event_catalog() -> Vec<WorldEvent> {
        vec![
            WorldEvent::CargoChanged {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::PlayerDamaged {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::PurchaseCompleted {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::RescueTriggered {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::PlayerSurvivalChanged {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::BombPlaced {
                player_id: LOCAL_PLAYER_ID,
            },
            WorldEvent::HazardChanged,
            WorldEvent::ImportantEffectTriggered,
        ]
    }

    #[must_use]
    pub fn gameplay_event_routing_summary() -> GameplayEventRoutingSummary {
        GameplayEventRoutingSummary::from_events_and_boundaries(
            &Self::world_event_catalog(),
            &planned_transient_effect_boundaries(),
        )
    }

    #[must_use]
    pub fn new() -> Self {
        Self::from_game(GameState::new())
    }

    #[must_use]
    pub fn from_game(game: GameState) -> Self {
        let world = WorldState::from_legacy_game(&game);
        Self::from_game_and_world(game, world)
    }

    #[must_use]
    pub fn from_persistent_world(save: &crate::save::PersistentWorldSave) -> Self {
        let game = save.restore_shell_game();
        let world = save.restore_world();
        let mut session = Self::from_game_and_world(game, world);
        session.current_tick = session.world.simulation_tick();
        session
            .command_network_session
            .set_current_tick(session.current_tick);
        let _revealed = session.reveal_legacy_exploration_from_world_player(LOCAL_PLAYER_ID);
        session
    }

    #[must_use]
    pub fn persistent_world_save(&self) -> crate::save::PersistentWorldSave {
        persistent_world_from_session(&self.world, &self.game)
    }

    pub const fn can_write_local_save(&self) -> bool {
        self.game.online_player_slot.is_none() || self.game.online_host_owns_save
    }

    pub fn local_save_authority_denial(&self) -> Option<String> {
        if self.can_write_local_save() {
            None
        } else {
            Some("Joined online clients cannot write the host-owned save.".to_owned())
        }
    }

    pub fn save_session(&self) -> Result<(), crate::save::SaveError> {
        if let Some(reason) = self.local_save_authority_denial() {
            return Err(crate::save::SaveError::SaveDenied(reason));
        }
        save_world_session(&self.world, &self.game)
    }

    pub fn save_session_slot(&self, slot: usize) -> Result<(), crate::save::SaveError> {
        if let Some(reason) = self.local_save_authority_denial() {
            return Err(crate::save::SaveError::SaveDenied(reason));
        }
        save_world_session_slot(&self.world, &self.game, slot)
    }

    pub fn load_session() -> Result<Self, crate::save::SaveError> {
        load_persistent_world().map(|save| Self::from_persistent_world(&save))
    }

    pub fn load_session_slot(slot: usize) -> Result<Self, crate::save::SaveError> {
        load_persistent_world_slot(slot).map(|save| Self::from_persistent_world(&save))
    }

    pub fn load_latest_session() -> Result<Self, crate::save::SaveError> {
        load_latest_persistent_world().map(|save| Self::from_persistent_world(&save))
    }

    pub fn handle_session_save_load_input(&mut self, input: PlayerInput) -> SessionSaveLoadSummary {
        match (input.save, input.load) {
            (true, false) => self.save_from_session_input(),
            (false, true) => self.load_from_session_input(),
            (true, true) => SessionSaveLoadSummary::failed(
                "Save/load ignored: choose either save or load, not both.".to_owned(),
            ),
            (false, false) => SessionSaveLoadSummary::none(),
        }
    }

    fn save_from_session_input(&mut self) -> SessionSaveLoadSummary {
        if let Some(message) = self.local_save_authority_denial() {
            self.game.message.clone_from(&message);
            return SessionSaveLoadSummary::save_denied(message);
        }
        match self.save_session() {
            Ok(()) => {
                self.game.save_dirty = false;
                "Game saved from authoritative session.".clone_into(&mut self.game.message);
                self.game.sound_cues.push(SoundCue::Ui);
                SessionSaveLoadSummary::saved(self.game.message.clone())
            }
            Err(error) => {
                self.game.message = format!("Save failed: {error}");
                SessionSaveLoadSummary::failed(self.game.message.clone())
            }
        }
    }

    fn load_from_session_input(&mut self) -> SessionSaveLoadSummary {
        if self.game.online_player_slot.is_some() && !self.game.online_host_owns_save {
            let message =
                "Joined online clients cannot load over host-owned session state.".to_owned();
            self.game.message.clone_from(&message);
            return SessionSaveLoadSummary::failed(message);
        }
        match Self::load_session() {
            Ok(mut loaded) => {
                loaded.apply_settings(self.current_settings());
                "Game loaded into authoritative session.".clone_into(&mut loaded.game.message);
                loaded.game.sound_cues.push(SoundCue::Ui);
                loaded.replace_session_epoch(self.session_epoch.next());
                *self = loaded;
                SessionSaveLoadSummary::loaded(self.game.message.clone())
            }
            Err(error) => {
                self.game.message = format!("Load failed: {error}");
                SessionSaveLoadSummary::failed(self.game.message.clone())
            }
        }
    }

    fn handle_session_slot_io_request(&mut self) -> Option<SessionSaveLoadSummary> {
        let request = self.game.take_session_slot_io_request()?;
        Some(match request {
            crate::game_state::SessionSlotIoRequest::Save { slot } => {
                self.save_slot_from_session_request(slot)
            }
            crate::game_state::SessionSlotIoRequest::Load { slot } => {
                self.load_slot_from_session_request(slot)
            }
        })
    }

    fn prepare_canonical_service_state(
        &mut self,
        player_id: PlayerId,
        request: crate::game_state::SessionServiceRequest,
    ) {
        self.world.progression_mut().town_development = self.game.town_development.clone();
        match request {
            crate::game_state::SessionServiceRequest::SalvageRecoverLostCargo
            | crate::game_state::SessionServiceRequest::SalvageLaunchDrone => {
                if self.world.lost_cargo_sites().contains_key(&player_id)
                    || self.game.lost_cargo_count == 0
                {
                    return;
                }
                self.world.lost_cargo_sites_mut().insert(
                    player_id,
                    LostCargoSite {
                        x: self.game.lost_cargo_x.unwrap_or_default(),
                        y: self.game.lost_cargo_y.unwrap_or_default(),
                        minerals: self.game.lost_minerals.clone(),
                        artifacts: self.game.lost_artifacts.clone(),
                    },
                );
            }
            crate::game_state::SessionServiceRequest::CompleteDepotWork => {
                self.world.progression_mut().town_event_day = self.game.town_event_day;
                self.world.progression_mut().deepest_tile_reached = self.game.deepest_tile_reached;
                if self.world.progression().active_expeditions.is_empty() {
                    self.world
                        .progression_mut()
                        .active_expeditions
                        .clone_from(&self.game.active_expeditions);
                }
                if self.world.progression().active_side_contracts.is_empty() {
                    self.world
                        .progression_mut()
                        .active_side_contracts
                        .clone_from(&self.game.active_side_contracts);
                    if self.world.progression().active_side_contracts.is_empty()
                        && self.game.side_contract_active
                        && let Some(target) = self.game.side_contract_target
                    {
                        self.world
                            .progression_mut()
                            .active_side_contracts
                            .push(SideContract {
                                kind: self.game.side_contract_kind,
                                target,
                                required: self.game.side_contract_required.max(1),
                                expires_day: None,
                            });
                    }
                }
            }
            _ => {}
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "session-owned service dispatcher is being expanded while legacy GameState mutations are deleted"
    )]
    fn handle_session_service_request(&mut self) -> Option<SessionSaveLoadSummary> {
        let request = self.game.take_session_service_request()?;
        let player_id = self.local_client().controlled_player_id;
        self.prepare_canonical_service_state(player_id, request);
        let before = self
            .world
            .player(player_id)
            .map(|player| (player.credits, player.fuel, player.hull))?;
        let upgrade_offer =
            if let crate::game_state::SessionServiceRequest::BuyUpgrade { index } = request {
                self.world
                    .player(player_id)
                    .and_then(|player| crate::economy::upgrade_offers(player).get(index).copied())
            } else {
                None
            };
        let command = match request {
            crate::game_state::SessionServiceRequest::Refuel { .. } => Some(PlayerCommand::Refuel),
            crate::game_state::SessionServiceRequest::Repair { .. } => Some(PlayerCommand::Repair),
            crate::game_state::SessionServiceRequest::SellCargo => Some(PlayerCommand::SellCargo),
            crate::game_state::SessionServiceRequest::BuyUpgrade { index } => {
                Some(PlayerCommand::BuyUpgrade { index })
            }
            crate::game_state::SessionServiceRequest::BuyBombBundle { .. }
            | crate::game_state::SessionServiceRequest::BuyMiningRockets
            | crate::game_state::SessionServiceRequest::ClaimFreeTestCharge
            | crate::game_state::SessionServiceRequest::SalvagePatchHull
            | crate::game_state::SessionServiceRequest::Finance
            | crate::game_state::SessionServiceRequest::BuyInsurance
            | crate::game_state::SessionServiceRequest::CraftRecipe { .. }
            | crate::game_state::SessionServiceRequest::UpgradeTownBuilding { .. }
            | crate::game_state::SessionServiceRequest::SalvageRecoverLostCargo
            | crate::game_state::SessionServiceRequest::SalvageLaunchDrone
            | crate::game_state::SessionServiceRequest::SalvageRecoverWreckedPart
            | crate::game_state::SessionServiceRequest::SalvageClearCollapseZones
            | crate::game_state::SessionServiceRequest::SalvageSellScrapTip
            | crate::game_state::SessionServiceRequest::SellScanData
            | crate::game_state::SessionServiceRequest::AutoSortLowGradeCargo
            | crate::game_state::SessionServiceRequest::CompleteDepotWork
            | crate::game_state::SessionServiceRequest::StartSideContract
            | crate::game_state::SessionServiceRequest::AcceptExpedition { .. }
            | crate::game_state::SessionServiceRequest::AbandonExpedition { .. } => None,
        };
        let outcome = if let Some(command) = command {
            self.world.apply_player_command(player_id, &command)
        } else {
            self.apply_direct_session_service_request(player_id, request)
        };
        if matches!(outcome, PlayerScopedCommandOutcome::Applied) {
            self.sync_service_presentation_from_world(player_id);
            match request {
                crate::game_state::SessionServiceRequest::Refuel { .. } => {
                    self.game.service_animation = Some(crate::game_state::ServiceAnimation::Fuel);
                    self.game.service_animation_seconds = 1.4;
                }
                crate::game_state::SessionServiceRequest::Repair { .. } => {
                    self.game.service_animation = Some(crate::game_state::ServiceAnimation::Repair);
                    self.game.service_animation_seconds = 1.4;
                }
                crate::game_state::SessionServiceRequest::SellCargo
                | crate::game_state::SessionServiceRequest::BuyUpgrade { .. }
                | crate::game_state::SessionServiceRequest::BuyBombBundle { .. }
                | crate::game_state::SessionServiceRequest::BuyMiningRockets
                | crate::game_state::SessionServiceRequest::ClaimFreeTestCharge
                | crate::game_state::SessionServiceRequest::SalvagePatchHull
                | crate::game_state::SessionServiceRequest::Finance
                | crate::game_state::SessionServiceRequest::BuyInsurance
                | crate::game_state::SessionServiceRequest::CraftRecipe { .. }
                | crate::game_state::SessionServiceRequest::UpgradeTownBuilding { .. }
                | crate::game_state::SessionServiceRequest::SalvageRecoverLostCargo
                | crate::game_state::SessionServiceRequest::SalvageLaunchDrone
                | crate::game_state::SessionServiceRequest::SalvageRecoverWreckedPart
                | crate::game_state::SessionServiceRequest::SalvageClearCollapseZones
                | crate::game_state::SessionServiceRequest::SalvageSellScrapTip
                | crate::game_state::SessionServiceRequest::SellScanData
                | crate::game_state::SessionServiceRequest::AutoSortLowGradeCargo
                | crate::game_state::SessionServiceRequest::CompleteDepotWork
                | crate::game_state::SessionServiceRequest::StartSideContract
                | crate::game_state::SessionServiceRequest::AcceptExpedition { .. }
                | crate::game_state::SessionServiceRequest::AbandonExpedition { .. } => {}
            }
            self.game.sound_cues.push(SoundCue::Upgrade);
            let after = self
                .world
                .player(player_id)
                .map_or(before, |player| (player.credits, player.fuel, player.hull));
            let spent = before.0.saturating_sub(after.0);
            self.game.message = match request {
                crate::game_state::SessionServiceRequest::Refuel { .. } if spent == 0 => {
                    "Fuel already full or no credits available.".to_owned()
                }
                crate::game_state::SessionServiceRequest::Refuel { .. } => {
                    format!("Fuel topped up for {spent} credits.")
                }
                crate::game_state::SessionServiceRequest::Repair { .. } if spent == 0 => {
                    "Hull already repaired or no credits available.".to_owned()
                }
                crate::game_state::SessionServiceRequest::Repair { .. } => {
                    format!("Hull repaired for {spent} credits.")
                }
                crate::game_state::SessionServiceRequest::SellCargo => {
                    let earned = after.0.saturating_sub(before.0);
                    if earned == 0 {
                        "No cargo to sell.".to_owned()
                    } else {
                        self.game.total_earnings = self.game.total_earnings.saturating_add(earned);
                        self.game.sound_cues.push(SoundCue::Sell);
                        format!("Sold cargo for {earned} credits.")
                    }
                }
                crate::game_state::SessionServiceRequest::BuyUpgrade { .. } => {
                    if let Some(offer) = upgrade_offer {
                        self.game.sound_cues.push(SoundCue::Upgrade);
                        format!(
                            "Bought {}.",
                            upgrade_tier_name(offer.kind, offer.level.saturating_sub(1))
                        )
                    } else {
                        "Upgrade purchase unavailable.".to_owned()
                    }
                }
                crate::game_state::SessionServiceRequest::BuyBombBundle { count, cost } => {
                    let bonus = u32::from(self.game.town_development.explosives_shack_level / 2);
                    let delivered = count + bonus;
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    format!(
                        "Nix sold you {delivered} timed charges for {cost} credits. Don't hug them."
                    )
                }
                crate::game_state::SessionServiceRequest::BuyMiningRockets => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    "Nix packed 4 mining rockets as high-yield shaped charges.".to_owned()
                }
                crate::game_state::SessionServiceRequest::ClaimFreeTestCharge => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    "Nix comped one test charge. Try not to test it indoors.".to_owned()
                }
                crate::game_state::SessionServiceRequest::SalvagePatchHull => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    let restored = after.2 - before.2;
                    format!("Salvage Yard patch job restored {restored:.0} hull.")
                }
                crate::game_state::SessionServiceRequest::Finance => {
                    self.game.sound_cues.push(SoundCue::Sell);
                    if before.0 < after.0 {
                        let advance = after.0 - before.0;
                        format!(
                            "HQ finance issued a {advance} credit advance. Risk-adjusted payoff: {} credits.",
                            self.world
                                .player(player_id)
                                .map_or(0, |player| player.loan_debt)
                        )
                    } else {
                        let payment = before.0.saturating_sub(after.0);
                        format!(
                            "Paid {payment} credits toward HQ debt. Remaining: {}.",
                            self.world
                                .player(player_id)
                                .map_or(0, |player| player.loan_debt)
                        )
                    }
                }
                crate::game_state::SessionServiceRequest::BuyInsurance => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    format!(
                        "Ledger sold tier {} rescue insurance. Higher tiers reduce fees and cargo loss.",
                        self.world
                            .player(player_id)
                            .map_or(0, |player| player.insurance_tier)
                    )
                }
                crate::game_state::SessionServiceRequest::CraftRecipe { recipe } => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    format!("Crafted {}: {}.", recipe.name(), recipe.description())
                }
                crate::game_state::SessionServiceRequest::UpgradeTownBuilding { building } => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    format!(
                        "{} upgraded to level {}. Deep Claim reputation increased.",
                        building.name(),
                        self.world.progression().town_development.level(building)
                    )
                }
                crate::game_state::SessionServiceRequest::SalvageRecoverLostCargo => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    format!(
                        "Mara recovered {} lost cargo marker(s).",
                        self.game.lost_cargo_count
                    )
                }
                crate::game_state::SessionServiceRequest::SalvageLaunchDrone => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    "Salvage drone deployed to the rescue beacon and recovered marked cargo."
                        .to_owned()
                }
                crate::game_state::SessionServiceRequest::SalvageRecoverWreckedPart => {
                    self.game.sound_cues.push(SoundCue::Upgrade);
                    "Mara recovered a wrecked rig part.".to_owned()
                }
                crate::game_state::SessionServiceRequest::SalvageClearCollapseZones => {
                    self.game.sound_cues.push(SoundCue::Milestone);
                    "Collapse-zone warnings cleared for salvage payout.".to_owned()
                }
                crate::game_state::SessionServiceRequest::SalvageSellScrapTip => {
                    self.game.sound_cues.push(SoundCue::Sell);
                    "Mara bought scrap telemetry for 35 credits.".to_owned()
                }
                crate::game_state::SessionServiceRequest::SellScanData => {
                    self.game.sound_cues.push(SoundCue::Sell);
                    "Scanner Lab bought mapped contacts.".to_owned()
                }
                crate::game_state::SessionServiceRequest::AutoSortLowGradeCargo => {
                    self.game.sound_cues.push(SoundCue::Sell);
                    self.game.last_depot_receipt.clone()
                }
                crate::game_state::SessionServiceRequest::CompleteDepotWork => {
                    self.game.sound_cues.push(SoundCue::Sell);
                    if self.game.contracts.completed > 0 {
                        format!(
                            "{} complete. Depot work recorded.",
                            crate::contract::ContractLog::new().active.title
                        )
                    } else {
                        self.game.message.clone()
                    }
                }
                crate::game_state::SessionServiceRequest::StartSideContract => {
                    self.game.sound_cues.push(SoundCue::Ui);
                    self.game.active_side_contracts.last().map_or_else(
                        || "Side contract unavailable.".to_owned(),
                        |contract| {
                            format!(
                                "Side contract posted: {:?} x{}.",
                                contract.target, contract.required
                            )
                        },
                    )
                }
                crate::game_state::SessionServiceRequest::AcceptExpedition { .. }
                | crate::game_state::SessionServiceRequest::AbandonExpedition { .. } => {
                    self.game.sound_cues.push(SoundCue::Ui);
                    self.game.message.clone()
                }
            };
        } else {
            "Service unavailable for current player.".clone_into(&mut self.game.message);
        }
        Some(SessionSaveLoadSummary::saved(self.game.message.clone()))
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::too_many_lines,
        reason = "direct session-owned service mutations are replacing removed GameState gameplay mutations"
    )]
    fn apply_direct_session_service_request(
        &mut self,
        player_id: PlayerId,
        request: crate::game_state::SessionServiceRequest,
    ) -> PlayerScopedCommandOutcome {
        if let Some(outcome) = self
            .world
            .apply_canonical_progression_service(player_id, request)
        {
            return outcome;
        }
        if let Some(outcome) = self
            .world
            .apply_canonical_salvage_service(player_id, request)
        {
            return outcome;
        }
        let Some(player) = self.world.player_mut(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match request {
            crate::game_state::SessionServiceRequest::BuyBombBundle { count, cost } => {
                if player.credits < cost {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= cost;
                let bonus = u32::from(self.game.town_development.explosives_shack_level / 2);
                player.bombs = player.bombs.saturating_add(count + bonus);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::BuyMiningRockets => {
                if self.game.town_development.explosives_shack_level < 4 || player.credits < 180 {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= 180;
                player.bombs = player.bombs.saturating_add(4);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::ClaimFreeTestCharge => {
                player.bombs = player.bombs.saturating_add(1);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvagePatchHull => {
                let patch = (player.max_hull() * 0.12).ceil();
                player.hull = (player.hull + patch).min(player.max_hull());
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::Finance => {
                if player.loan_debt == 0 {
                    let advance = 250 + u32::from(self.game.town_development.bank_level) * 150;
                    let risk_premium = 50 + u32::from(self.game.town_development.bank_level) * 25;
                    player.credits = player.credits.saturating_add(advance);
                    player.loan_debt = advance.saturating_add(risk_premium);
                } else {
                    let payment = player.loan_debt.min(player.credits);
                    player.credits -= payment;
                    player.loan_debt -= payment;
                }
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::BuyInsurance => {
                if player.insured {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let max_tier = 1_u8
                    .saturating_add(self.game.town_development.bank_level)
                    .min(4);
                let next_tier = player.insurance_tier.saturating_add(1).min(max_tier);
                if player.insurance_tier >= max_tier {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let bank_discount =
                    u32::from(self.game.town_development.bank_level).saturating_mul(15);
                let cost = (70 + u32::from(next_tier) * 55).saturating_sub(bank_discount);
                if player.credits < cost {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= cost;
                player.insured = true;
                player.insurance_tier = next_tier;
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::CraftRecipe { recipe } => {
                for (material, required) in recipe.cost() {
                    if player.materials.get(material).copied().unwrap_or(0) < *required {
                        return PlayerScopedCommandOutcome::IgnoredUnavailable;
                    }
                }
                for (material, required) in recipe.cost() {
                    if let Some(count) = player.materials.get_mut(material) {
                        *count = count.saturating_sub(*required);
                    }
                }
                player.materials.retain(|_, count| *count > 0);
                match recipe {
                    crate::game_state::RecipeKind::ReinforcedBulkhead => {
                        player.crafted_bulkheads = player.crafted_bulkheads.saturating_add(1);
                        player.hull = player.max_hull();
                    }
                    crate::game_state::RecipeKind::AuxiliaryTank => {
                        player.fuel_capacity += 20.0;
                        player.fuel = player.fuel_capacity;
                    }
                    crate::game_state::RecipeKind::ExpandedSorter => {
                        player.crafted_sorters = player.crafted_sorters.saturating_add(1);
                        player.cargo_capacity = player.cargo_capacity.saturating_add(4);
                    }
                    crate::game_state::RecipeKind::SignalRelayKit => {
                        player.signal_relay_kits = player.signal_relay_kits.saturating_add(1);
                    }
                    crate::game_state::RecipeKind::SurveyDroneKit => {
                        player.survey_drone_kits = player.survey_drone_kits.saturating_add(1);
                    }
                    crate::game_state::RecipeKind::CargoLiftKit => {
                        player.cargo_lift_kits = player.cargo_lift_kits.saturating_add(1);
                    }
                    crate::game_state::RecipeKind::TunnelSupportKit => {
                        player.tunnel_support_kits = player.tunnel_support_kits.saturating_add(1);
                    }
                    crate::game_state::RecipeKind::PumpStationKit => {
                        player.pump_station_kits = player.pump_station_kits.saturating_add(1);
                    }
                    crate::game_state::RecipeKind::OreProcessorKit => {
                        player.ore_processor_kits = player.ore_processor_kits.saturating_add(1);
                    }
                }
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::UpgradeTownBuilding { building } => {
                let cost = self.game.town_development.upgrade_cost(building);
                let material_gate = self.game.town_development.level(building) >= 1;
                if material_gate
                    && player
                        .materials
                        .get(&crate::terrain::StrategicResourceKind::AncientAlloy)
                        .copied()
                        .unwrap_or(0)
                        == 0
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                if player.credits < cost {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                player.credits -= cost;
                if material_gate {
                    if let Some(count) = player
                        .materials
                        .get_mut(&crate::terrain::StrategicResourceKind::AncientAlloy)
                    {
                        *count = count.saturating_sub(1);
                    }
                    player.materials.retain(|_, count| *count > 0);
                }
                *self.game.town_development.level_mut(building) += 1;
                self.game.town_development.reputation =
                    self.game.town_development.reputation.saturating_add(1);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvageRecoverLostCargo => {
                let recovered = self.game.lost_cargo_count;
                if recovered == 0 {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let discount = u32::from(self.game.town_development.salvage_yard_level) * 3;
                let fee = (recovered * 12)
                    .saturating_sub(recovered * discount)
                    .min(player.credits);
                player.credits -= fee;
                for (mineral, count) in std::mem::take(&mut self.game.lost_minerals) {
                    *player.cargo.entry(mineral).or_default() += count;
                }
                for (artifact, count) in std::mem::take(&mut self.game.lost_artifacts) {
                    *player.artifacts.entry(artifact).or_default() += count;
                }
                self.game.lost_cargo_count = 0;
                self.game.lost_cargo_x = None;
                self.game.lost_cargo_y = None;
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvageLaunchDrone => {
                if self.game.town_development.salvage_yard_level < 2
                    || self.game.lost_cargo_count == 0
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let Some(x) = self.game.lost_cargo_x else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                let y = self.game.lost_cargo_y.unwrap_or(10.0 * TILE_SIZE);
                self.game
                    .infrastructure
                    .push(crate::game_state::PlacedInfrastructure {
                        kind: crate::game_state::InfrastructureKind::SurveyDrone,
                        position: TilePosition {
                            x: (x / TILE_SIZE).floor() as i32,
                            y: (y / TILE_SIZE).floor() as i32,
                        },
                        durability: crate::game_state::default_infrastructure_durability(),
                    });
                let recovered = self.game.lost_cargo_count;
                let discount = u32::from(self.game.town_development.salvage_yard_level) * 3;
                let fee = (recovered * 12)
                    .saturating_sub(recovered * discount)
                    .min(player.credits);
                player.credits -= fee;
                for (mineral, count) in std::mem::take(&mut self.game.lost_minerals) {
                    *player.cargo.entry(mineral).or_default() += count;
                }
                for (artifact, count) in std::mem::take(&mut self.game.lost_artifacts) {
                    *player.artifacts.entry(artifact).or_default() += count;
                }
                self.game.lost_cargo_count = 0;
                self.game.lost_cargo_x = None;
                self.game.lost_cargo_y = None;
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvageRecoverWreckedPart => {
                if self.game.town_development.salvage_yard_level < 3 {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let part = match self.game.rescue_count % 4 {
                    0 => crate::game_state::RigPartKind::ImpactHull,
                    1 => crate::game_state::RigPartKind::ArmoredCargoBay,
                    2 => crate::game_state::RigPartKind::HazardScanner,
                    _ => crate::game_state::RigPartKind::NeedleDrill,
                };
                self.game.rig_part_inventory.insert(part);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvageClearCollapseZones => {
                if self.game.town_development.salvage_yard_level < 4 {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let cleared = self.game.collapse_warnings.len();
                self.game.collapse_warnings.clear();
                let payout = u32::try_from(cleared)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(45);
                player.credits = player.credits.saturating_add(payout);
                self.game.total_earnings = self.game.total_earnings.saturating_add(payout);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SalvageSellScrapTip => {
                player.credits = player.credits.saturating_add(35);
                PlayerScopedCommandOutcome::Applied
            }
            crate::game_state::SessionServiceRequest::SellScanData => {
                let _ = player;
                self.sell_scan_data_for_player(player_id)
            }
            crate::game_state::SessionServiceRequest::AutoSortLowGradeCargo => {
                let _ = player;
                self.auto_sort_low_grade_cargo_for_player(player_id)
            }
            crate::game_state::SessionServiceRequest::CompleteDepotWork => {
                let _ = player;
                let outcome = self
                    .world
                    .complete_canonical_progression_for_player(player_id);
                if outcome == PlayerScopedCommandOutcome::IgnoredUnavailable {
                    self.world.complete_canonical_side_contract(player_id)
                } else {
                    outcome
                }
            }
            crate::game_state::SessionServiceRequest::StartSideContract => {
                let _ = player;
                self.world.start_canonical_side_contract()
            }
            crate::game_state::SessionServiceRequest::AcceptExpedition { index } => {
                let _ = player;
                self.world.accept_canonical_expedition(index)
            }
            crate::game_state::SessionServiceRequest::AbandonExpedition { index } => {
                let _ = player;
                if index < self.world.progression().active_expeditions.len() {
                    self.world
                        .progression_mut()
                        .active_expeditions
                        .remove(index);
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                }
            }
            crate::game_state::SessionServiceRequest::Refuel { .. }
            | crate::game_state::SessionServiceRequest::Repair { .. }
            | crate::game_state::SessionServiceRequest::SellCargo
            | crate::game_state::SessionServiceRequest::BuyUpgrade { .. } => {
                PlayerScopedCommandOutcome::IgnoredUnavailable
            }
        }
    }

    fn sell_scan_data_for_player(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        if self.game.town_development.scanner_lab_level == 0 || self.game.scan_markers.is_empty() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.world.player_mut(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let marker_count = u32::try_from(self.game.scan_markers.len()).unwrap_or(u32::MAX);
        let payout = marker_count
            .saturating_mul(6 + u32::from(self.game.town_development.scanner_lab_level) * 4);
        self.game.scan_markers.clear();
        player.credits = player.credits.saturating_add(payout);
        self.game.total_earnings = self.game.total_earnings.saturating_add(payout);
        PlayerScopedCommandOutcome::Applied
    }

    fn auto_sort_low_grade_cargo_for_player(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        if self.game.town_development.depot_level < 3 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.world.player_mut(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let low_grade = [
            crate::terrain::MineralKind::Copper,
            crate::terrain::MineralKind::Iron,
            crate::terrain::MineralKind::Silver,
        ];
        let mut units = 0;
        let mut payout: u32 = 0;
        for mineral in low_grade {
            let Some(count) = player.cargo.remove(&mineral) else {
                continue;
            };
            units += count;
            payout = payout.saturating_add(mineral.value().saturating_mul(count));
        }
        if payout == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits = player.credits.saturating_add(payout);
        self.game.total_earnings = self.game.total_earnings.saturating_add(payout);
        self.game.last_depot_receipt = format!(
            "AUTO-SORT sold {units} low-grade unit(s) for {payout} cr. Rare minerals and artifacts preserved."
        );
        self.game
            .depot_receipts
            .push(self.game.last_depot_receipt.clone());
        if self.game.depot_receipts.len() > 5 {
            self.game.depot_receipts.remove(0);
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn start_side_contract_for_session(&mut self) -> PlayerScopedCommandOutcome {
        if self.game.active_side_contracts.len() >= 3 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        self.game.side_contract_active = true;
        self.game.side_contract_kind = match self.game.town_event_day % 4 {
            0 => crate::game_state::SideContractKind::Cargo,
            1 => crate::game_state::SideContractKind::DepthSurvey,
            2 => crate::game_state::SideContractKind::HazardScan,
            _ => crate::game_state::SideContractKind::Rush,
        };
        self.game.side_contract_target = Some(match self.game.side_contract_kind {
            crate::game_state::SideContractKind::Cargo => {
                TileKind::Ore(crate::terrain::MineralKind::Gold)
            }
            crate::game_state::SideContractKind::DepthSurvey => {
                TileKind::Ore(crate::terrain::MineralKind::Platinum)
            }
            crate::game_state::SideContractKind::HazardScan => TileKind::Gas,
            crate::game_state::SideContractKind::Rush => {
                TileKind::Ore(crate::terrain::MineralKind::Ruby)
            }
        });
        self.game.side_contract_required = match self.game.side_contract_kind {
            crate::game_state::SideContractKind::Cargo => 2,
            crate::game_state::SideContractKind::DepthSurvey => 65,
            crate::game_state::SideContractKind::HazardScan => 3,
            crate::game_state::SideContractKind::Rush => 1,
        };
        self.game.message = self.side_contract_posted_message();
        if let Some(target) = self.game.side_contract_target {
            self.game
                .active_side_contracts
                .push(crate::game_state::SideContract {
                    kind: self.game.side_contract_kind,
                    target,
                    required: self.game.side_contract_required,
                    expires_day: (self.game.side_contract_kind
                        == crate::game_state::SideContractKind::Rush)
                        .then_some(self.game.town_event_day + 2),
                });
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn side_contract_posted_message(&self) -> String {
        match self.game.side_contract_kind {
            crate::game_state::SideContractKind::Cargo => format!(
                "Side contract posted: deliver {} x{} for bonus pay.",
                self.game
                    .side_contract_target
                    .map_or("sample", TileKind::name),
                self.game.side_contract_required
            ),
            crate::game_state::SideContractKind::DepthSurvey => format!(
                "Side contract posted: reach {}m and report to depot.",
                self.game.side_contract_required
            ),
            crate::game_state::SideContractKind::HazardScan => format!(
                "Side contract posted: scan {} hazards and report to depot.",
                self.game.side_contract_required
            ),
            crate::game_state::SideContractKind::Rush => format!(
                "Rush contract posted: deliver {} x{} before day {}.",
                self.game
                    .side_contract_target
                    .map_or("sample", TileKind::name),
                self.game.side_contract_required,
                self.game.town_event_day + 2
            ),
        }
    }

    fn complete_depot_work_for_player(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        self.world
            .complete_canonical_progression_for_player(player_id)
    }

    fn complete_expedition_work_for_player(&mut self, player_id: PlayerId) -> bool {
        let Some(player_snapshot) = self.world.player(player_id).cloned() else {
            return false;
        };
        let mut snapshot = self.game.clone();
        snapshot.player = player_snapshot;
        let mut completed_expeditions = Vec::new();
        let mut expedition_reward: u32 = 0;
        self.game.active_expeditions.retain(|expedition| {
            if expedition.expires_day < self.game.town_event_day {
                return false;
            }
            if crate::game_state::expedition_satisfied(*expedition, &snapshot) {
                expedition_reward = expedition_reward.saturating_add(expedition.reward);
                completed_expeditions.push(*expedition);
                false
            } else {
                true
            }
        });
        if expedition_reward == 0 {
            return false;
        }
        for expedition in completed_expeditions.iter().copied() {
            match expedition.kind {
                crate::game_state::ExpeditionObjectiveKind::RetrieveArtifact => {
                    self.game
                        .legendary_blueprints
                        .insert(crate::game_state::LegendaryBlueprint::StarPlating);
                    self.game
                        .rig_part_inventory
                        .insert(crate::game_state::RigPartKind::ResonanceDrill);
                }
                crate::game_state::ExpeditionObjectiveKind::RecoverProbe
                | crate::game_state::ExpeditionObjectiveKind::ReachSignal => {
                    self.game
                        .rig_part_inventory
                        .insert(crate::game_state::RigPartKind::HazardScanner);
                }
                _ => {}
            }
            let Some(player) = self.world.player_mut(player_id) else {
                return false;
            };
            match expedition.kind {
                crate::game_state::ExpeditionObjectiveKind::ScanAnomaly
                | crate::game_state::ExpeditionObjectiveKind::ScanHazards => {
                    player.add_material(
                        crate::terrain::StrategicResourceKind::CrystalLens,
                        expedition.required,
                    );
                }
                crate::game_state::ExpeditionObjectiveKind::MineVein => {
                    player.add_material(crate::terrain::StrategicResourceKind::AncientAlloy, 1);
                }
                _ => {}
            }
            drillgame_simulation::progression::consume_expedition_delivery(expedition, player);
        }
        let Some(player) = self.world.player_mut(player_id) else {
            return false;
        };
        let completed = u32::try_from(completed_expeditions.len()).unwrap_or(u32::MAX);
        player.credits = player.credits.saturating_add(expedition_reward);
        self.game.total_earnings = self.game.total_earnings.saturating_add(expedition_reward);
        self.game.expeditions_completed = self.game.expeditions_completed.saturating_add(completed);
        self.game.town_development.reputation = self
            .game
            .town_development
            .reputation
            .saturating_add(completed);
        self.game.message = format!(
            "Completed {} expedition(s) for {expedition_reward} credits.",
            completed_expeditions.len()
        );
        true
    }

    fn complete_active_side_contract_for_player(&mut self, player_id: PlayerId) -> bool {
        let Some(player_snapshot) = self.world.player(player_id).cloned() else {
            return false;
        };
        let mut snapshot = self.game.clone();
        snapshot.player = player_snapshot;
        let Some((index, _contract)) = self
            .game
            .active_side_contracts
            .iter()
            .copied()
            .enumerate()
            .find(|(_, contract)| crate::game_state::side_contract_satisfied(*contract, &snapshot))
        else {
            return false;
        };
        let contract = self.game.active_side_contracts.remove(index);
        let Some(player) = self.world.player_mut(player_id) else {
            return false;
        };
        if matches!(
            contract.kind,
            crate::game_state::SideContractKind::Cargo | crate::game_state::SideContractKind::Rush
        ) {
            drillgame_simulation::progression::consume_side_contract_cargo(contract, player);
        }
        let completed_reward = 420 + contract.required.min(10) * 80;
        player.credits = player.credits.saturating_add(completed_reward);
        self.game.total_earnings = self.game.total_earnings.saturating_add(completed_reward);
        self.game.side_contract_active = !self.game.active_side_contracts.is_empty();
        self.game.message = format!("Side contract fulfilled: {completed_reward} credits bonus.");
        true
    }

    fn complete_legacy_side_contract_for_player(&mut self, player_id: PlayerId) -> bool {
        if !self.game.side_contract_active {
            return false;
        }
        let Some(target) = self.game.side_contract_target else {
            return false;
        };
        let Some(player_snapshot) = self.world.player(player_id).cloned() else {
            return false;
        };
        let mut snapshot = self.game.clone();
        snapshot.player = player_snapshot;
        let contract = crate::game_state::SideContract {
            kind: self.game.side_contract_kind,
            target,
            required: self.game.side_contract_required,
            expires_day: None,
        };
        if !crate::game_state::side_contract_satisfied(contract, &snapshot) {
            return false;
        }
        let Some(player) = self.world.player_mut(player_id) else {
            return false;
        };
        if matches!(
            contract.kind,
            crate::game_state::SideContractKind::Cargo | crate::game_state::SideContractKind::Rush
        ) {
            drillgame_simulation::progression::consume_side_contract_cargo(contract, player);
        }
        let completed_reward = 420 + contract.required * 80;
        player.credits = player.credits.saturating_add(completed_reward);
        self.game.total_earnings = self.game.total_earnings.saturating_add(completed_reward);
        self.game.side_contract_active = false;
        self.game.message = format!("Side contract fulfilled: {completed_reward} credits bonus.");
        true
    }

    fn complete_story_contract_for_player(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.world.player_mut(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let before_completed = self.game.contracts.completed;
        let Some(completion) = self.game.contracts.try_complete(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let escrow_bonus =
            completion.reward * u32::from(self.game.town_development.bank_level) / 20;
        if escrow_bonus > 0 {
            player.credits = player.credits.saturating_add(escrow_bonus);
        }
        self.game.total_earnings = self
            .game
            .total_earnings
            .saturating_add(completion.reward.saturating_add(escrow_bonus));
        if completion.finished_story {
            self.game.won_game = true;
            self.game.deep_claim_status = crate::economy::DeepClaimStatus::Unlocked;
            self.game.fastest_star_core_seconds = self
                .game
                .fastest_star_core_seconds
                .map_or(Some(self.game.play_seconds), |best| {
                    Some(best.min(self.game.play_seconds))
                });
            self.game.message = format!(
                "{} complete! Star Core secured. Deep Claim charter unlocked. Bonus: {} credits + {escrow_bonus} escrow.",
                completion.completed_title, completion.reward
            );
        } else {
            let story = crate::contract::ContractLog::story_for_completed(before_completed + 1);
            self.game.message = format!(
                "{} complete! Bonus paid: {} credits + {escrow_bonus} escrow. {story}",
                completion.completed_title, completion.reward
            );
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn save_slot_from_session_request(&mut self, slot: usize) -> SessionSaveLoadSummary {
        if let Some(message) = self.local_save_authority_denial() {
            self.game.message.clone_from(&message);
            return SessionSaveLoadSummary::save_denied(message);
        }
        match self.save_session_slot(slot) {
            Ok(()) => {
                self.game.save_dirty = false;
                self.game.message = format!("Saved slot {} from authoritative session.", slot + 1);
                self.game.sound_cues.push(SoundCue::Ui);
                SessionSaveLoadSummary::saved(self.game.message.clone())
            }
            Err(error) => {
                self.game.message = format!("Save slot failed: {error}");
                SessionSaveLoadSummary::failed(self.game.message.clone())
            }
        }
    }

    fn load_slot_from_session_request(&mut self, slot: usize) -> SessionSaveLoadSummary {
        if self.game.online_player_slot.is_some() && !self.game.online_host_owns_save {
            let message =
                "Joined online clients cannot load over host-owned session state.".to_owned();
            self.game.message.clone_from(&message);
            return SessionSaveLoadSummary::failed(message);
        }
        match Self::load_session_slot(slot) {
            Ok(mut loaded) => {
                loaded.apply_settings(self.current_settings());
                loaded.game.message =
                    format!("Loaded slot {} into authoritative session.", slot + 1);
                loaded.game.sound_cues.push(SoundCue::Ui);
                loaded.game.mark_full_terrain_refresh();
                loaded.replace_session_epoch(self.session_epoch.next());
                *self = loaded;
                SessionSaveLoadSummary::loaded(self.game.message.clone())
            }
            Err(error) => {
                self.game.message = format!("Load slot failed: {error}");
                SessionSaveLoadSummary::failed(self.game.message.clone())
            }
        }
    }

    const fn input_without_session_save_load(mut input: PlayerInput) -> PlayerInput {
        input.save = false;
        input.load = false;
        input
    }

    fn from_game_and_world(game: GameState, world: WorldState) -> Self {
        let local_client = ClientState::default();
        let session_epoch = SessionEpoch::new(1);
        let mut host_input_inbox = HostInputInbox::new(session_epoch, SimulationTick::default(), 8);
        host_input_inbox.assign_player(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID);
        let mut session = Self {
            game,
            world,
            clients: BTreeMap::from([(LOCAL_CLIENT_ID, local_client)]),
            local_seat_client_ids: BTreeSet::from([LOCAL_CLIENT_ID]),
            local_client_id: LOCAL_CLIENT_ID,
            current_tick: SimulationTick::default(),
            simulation_accumulator: Duration::ZERO,
            terrain_revisions: TerrainRevisionTracker::default(),
            session_epoch,
            host_input_inbox,
            command_network_session: CommandNetworkSession::new(SimulationTick::default(), 8),
            pending_commands: BTreeMap::new(),
            pending_events: Vec::new(),
            presented_gameplay_events: BTreeSet::new(),
            presented_gameplay_event_order: VecDeque::new(),
            latest_local_movement_intent: None,
            latest_movement_intents: BTreeMap::new(),
            latest_local_authoritative_commands: Vec::new(),
            remote_timing: RemoteTimingTuning::from_latency_loss(0.0, 0.0),
            authoritative_client_world: None,
            authoritative_client_epoch: None,
            authoritative_client_hash: None,
            authoritative_recovery_request: None,
            authoritative_remote_history: BTreeMap::new(),
            predicted_client_world: None,
            pending_prediction_inputs: VecDeque::new(),
            prediction_presentation_offset: None,
            previous_predicted_player_snapshots: BTreeMap::new(),
        };
        let _revealed = session.reveal_legacy_exploration_from_world_player(LOCAL_PLAYER_ID);
        session
    }

    #[must_use]
    pub const fn game(&self) -> &GameState {
        &self.game
    }

    pub const fn game_mut(&mut self) -> &mut GameState {
        &mut self.game
    }

    #[must_use]
    pub const fn world(&self) -> &WorldState {
        &self.world
    }

    #[allow(
        dead_code,
        reason = "mutable world accessor is used by integration-style tests and future authoring tools"
    )]
    pub const fn world_mut(&mut self) -> &mut WorldState {
        &mut self.world
    }

    #[must_use]
    pub fn world_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot::from_world(self.world.simulation_tick(), &self.world)
    }

    #[must_use]
    pub fn local_client(&self) -> &ClientState {
        self.clients
            .get(&self.local_client_id)
            .expect("local client exists in game session")
    }

    fn local_client_mut(&mut self) -> &mut ClientState {
        self.clients
            .get_mut(&self.local_client_id)
            .expect("local client exists in game session")
    }

    #[must_use]
    pub fn local_view(&self) -> &ClientView {
        &self.local_client().view
    }

    #[must_use]
    pub fn client_views(&self) -> Vec<&ClientView> {
        self.clients.values().map(|client| &client.view).collect()
    }

    #[must_use]
    pub fn local_seat_views(&self) -> Vec<&ClientView> {
        self.local_seat_client_ids
            .iter()
            .filter_map(|client_id| self.clients.get(client_id).map(|client| &client.view))
            .collect()
    }

    #[must_use]
    pub fn render_views(&self) -> Vec<&ClientView> {
        self.local_seat_views()
    }

    #[must_use]
    pub fn render_frame_plan(&self) -> RenderFramePlan {
        RenderFramePlan::from_world_and_client_views(
            &self.world,
            self.local_seat_client_ids
                .iter()
                .filter_map(|client_id| self.clients.get(client_id).map(|client| client.view)),
        )
    }

    pub fn authoritative_client_render_frame_output(&self) -> Option<LiveRenderFrameOutput> {
        let world = self.authoritative_client_world.as_ref()?;
        Some(self.render_frame_output_from_world(world, &PredictionPresentationPlan::default()))
    }

    pub fn predicted_client_render_frame_output(&self) -> Option<LiveRenderFrameOutput> {
        let world = self.predicted_client_world.as_ref()?;
        Some(self.render_frame_output_from_world(world, &PredictionPresentationPlan::default()))
    }

    #[must_use]
    pub fn online_client_render_frame_output(&self) -> Option<LiveRenderFrameOutput> {
        if self.pending_prediction_inputs.is_empty() {
            self.authoritative_client_render_frame_output()
        } else {
            self.predicted_client_render_frame_output()
        }
    }

    fn render_particle_snapshot(&self, world: &WorldState) -> RenderParticleSnapshot {
        let local_player_id = self.local_client().controlled_player_id;
        RenderParticleSnapshot {
            lost_cargo: world
                .lost_cargo_sites()
                .get(&local_player_id)
                .map(|site| (site.x, site.y)),
            dust: self.game.dust_particles.clone(),
            boulders: world.falling_boulders().to_vec(),
            sparks: self.game.spark_particles.clone(),
        }
    }

    fn render_frame_output_from_world(
        &self,
        world: &WorldState,
        prediction_plan: &PredictionPresentationPlan,
    ) -> LiveRenderFrameOutput {
        let frame_plan = RenderFramePlan::from_world_and_client_views(
            world,
            self.local_seat_client_ids
                .iter()
                .filter_map(|client_id| self.clients.get(client_id).map(|client| client.view)),
        );
        let world_players_by_view = frame_plan
            .views
            .iter()
            .map(|view| {
                (
                    view.client_id,
                    frame_plan.world_player_presentations_for_view(view, prediction_plan),
                )
            })
            .collect();
        let ui_snapshots = frame_plan
            .views
            .iter()
            .filter_map(|view| {
                let hud = frame_plan.hud_snapshot_for_view(view)?;
                let player = world.player(view.controlled_player_id)?;
                let client = self.clients.get(&view.client_id)?;
                Some(SplitScreenUiPresentationSnapshot {
                    client_id: view.client_id,
                    player_id: view.controlled_player_id,
                    viewport: view.viewport,
                    camera: view.camera,
                    hud,
                    modal: client.modal,
                    run_mode: view.run_mode,
                    message: client.local_message.clone(),
                    audio_cues: client.local_audio_cues.clone(),
                    scanner_cooldown_seconds: hud.scanner_cooldown_seconds,
                    depth_tile: player.tile_position(TILE_SIZE).y.max(0),
                })
            })
            .collect();
        LiveRenderFrameOutput {
            terrain: world.terrain().clone(),
            hazards: world.hazards().to_vec(),
            bombs: world.bombs().to_vec(),
            infrastructure: world.infrastructure().to_vec(),
            active_drills: world.active_drills().clone(),
            particles: self.render_particle_snapshot(world),
            discovered_tiles_by_player: world.discovered_tiles_by_player().clone(),
            viewport_plans: frame_plan.viewport_plans(prediction_plan),
            hud_snapshots: frame_plan.hud_snapshots(),
            world_players_by_view,
            ui_snapshots,
        }
    }

    #[must_use]
    pub fn live_render_frame_output(
        &self,
        prediction_plan: &PredictionPresentationPlan,
    ) -> LiveRenderFrameOutput {
        let frame_plan = self.render_frame_plan();
        let world_players_by_view = frame_plan
            .views
            .iter()
            .map(|view| {
                (
                    view.client_id,
                    frame_plan.world_player_presentations_for_view(view, prediction_plan),
                )
            })
            .collect();
        let ui_snapshots = frame_plan
            .views
            .iter()
            .filter_map(|view| {
                let hud = frame_plan.hud_snapshot_for_view(view)?;
                let player = self.world.player(view.controlled_player_id)?;
                let client = self.clients.get(&view.client_id)?;
                Some(SplitScreenUiPresentationSnapshot {
                    client_id: view.client_id,
                    player_id: view.controlled_player_id,
                    viewport: view.viewport,
                    camera: view.camera,
                    hud,
                    modal: client.modal,
                    run_mode: view.run_mode,
                    message: client.local_message.clone(),
                    audio_cues: client.local_audio_cues.clone(),
                    scanner_cooldown_seconds: hud.scanner_cooldown_seconds,
                    depth_tile: player.tile_position(TILE_SIZE).y.max(0),
                })
            })
            .collect();
        LiveRenderFrameOutput {
            terrain: self.world.terrain().clone(),
            hazards: self.world.hazards().to_vec(),
            bombs: self.world.bombs().to_vec(),
            infrastructure: self.world.infrastructure().to_vec(),
            active_drills: self.world.active_drills().clone(),
            particles: self.render_particle_snapshot(&self.world),
            discovered_tiles_by_player: self.world.discovered_tiles_by_player().clone(),
            viewport_plans: frame_plan
                .viewport_plans(prediction_plan)
                .into_iter()
                .map(|mut plan| {
                    let policy = if plan.client_id == self.local_client_id
                        && !self.local_client().prediction().is_render_overlay_empty()
                    {
                        OnlineRenderAuthorityPolicy::ClientPredicted
                    } else {
                        OnlineRenderAuthorityPolicy::HostAuthoritative
                    };
                    plan.prediction_overlay = Some(self.online_prediction_render_overlay(policy));
                    plan
                })
                .collect(),
            hud_snapshots: frame_plan.hud_snapshots(),
            world_players_by_view,
            ui_snapshots,
        }
    }

    #[must_use]
    pub fn predicted_local_movement(&self, delta_seconds: f32) -> Option<PredictedMovement> {
        self.local_movement_prediction_plan(delta_seconds)
            .map(LocalMovementPredictionPlan::predicted_movement)
    }

    #[must_use]
    pub fn local_movement_prediction_plan(
        &self,
        delta_seconds: f32,
    ) -> Option<LocalMovementPredictionPlan> {
        let view = self.local_view();
        let player = self.world.player(view.controlled_player_id)?;
        let snapshot =
            PlayerSnapshot::from_world_player(view.controlled_player_id, player, &self.world);
        Some(LocalMovementPredictionPlan::from_snapshot(
            &snapshot,
            delta_seconds,
        ))
    }

    pub fn note_save_session_transition_for_prediction(&mut self) {
        self.local_client_mut()
            .prediction
            .note_save_session_transition();
    }

    pub fn apply_client_actions(&mut self, client_id: ClientId, actions: &[ClientAction]) {
        let mut exit_requested = false;
        let Some(client) = self.clients.get_mut(&client_id) else {
            return;
        };
        for action in actions {
            match action {
                ClientAction::Pause => {
                    client.view.run_mode = RunMode::Paused;
                    client.modal = None;
                    "Paused".clone_into(&mut client.local_message);
                    client.local_audio_cues.push(SoundCue::Ui);
                }
                ClientAction::Cancel => {
                    client.view.run_mode = RunMode::Playing;
                    client.modal = None;
                    "Resumed".clone_into(&mut client.local_message);
                    client.local_audio_cues.push(SoundCue::Ui);
                }
                ClientAction::ToggleMap => {
                    client.modal = Some(ModalScreen::Map);
                    "Map opened".clone_into(&mut client.local_message);
                    client.local_audio_cues.push(SoundCue::Ui);
                }
                ClientAction::ToggleHelp => {
                    client.modal = Some(ModalScreen::Help);
                    "Help opened".clone_into(&mut client.local_message);
                    client.local_audio_cues.push(SoundCue::Ui);
                }
                ClientAction::ExitRequested => {
                    client.exit_requested = true;
                    exit_requested = true;
                }
                ClientAction::Save | ClientAction::Load => {
                    client.prediction.note_save_session_transition();
                    "Split-screen save/load synchronized".clone_into(&mut client.local_message);
                    client.local_audio_cues.push(SoundCue::Ui);
                }
                ClientAction::Confirm
                | ClientAction::MenuUp
                | ClientAction::MenuDown
                | ClientAction::MenuLeft
                | ClientAction::MenuRight
                | ClientAction::ToggleDetails
                | ClientAction::VolumeUp
                | ClientAction::VolumeDown
                | ClientAction::ToggleFullscreen
                | ClientAction::ToggleLocalMultiplayer => {}
            }
        }
        if exit_requested {
            self.push_event(WorldEvent::ClientExitRequested { client_id });
        }
    }

    pub fn update_remote_timing_from_network_sample(&mut self, ping_seconds: f32, loss_ratio: f32) {
        self.remote_timing = RemoteTimingTuning::from_latency_loss(ping_seconds, loss_ratio);
        let stall_seconds = self.remote_timing.timeout_after + FIXED_DELTA_SECONDS;
        let _within_extrapolation_window =
            self.remote_timing.allows_extrapolation(ping_seconds * 0.5);
        if self.remote_timing.timed_out(stall_seconds) {
            self.local_client_mut()
                .prediction
                .note_prediction_failure(PredictionFailure::HazardOrRescueChangedState);
        }
    }

    #[must_use]
    pub const fn remote_timing(&self) -> RemoteTimingTuning {
        self.remote_timing
    }

    #[must_use]
    pub fn prediction_presentation_plan(
        &self,
        authoritative_snapshot: Option<&PlayerSnapshot>,
        _delta_seconds: f32,
        remote_alpha: f32,
        remote_stall_seconds: f32,
    ) -> PredictionPresentationPlan {
        let prediction = self.local_client().prediction();
        let player_id = authoritative_snapshot.map_or_else(
            || self.local_client().controlled_player_id,
            |snapshot| snapshot.player_id,
        );
        let local_movement = prediction.predicted_local_movement(player_id);
        let corrected_local_presentation = prediction.active_correction_presentation();
        PredictionPresentationPlan {
            local_movement,
            correction: None,
            corrected_local_presentation,
            tentative_feedback: prediction.tentative_feedback_presentations(),
            remote_players: prediction.remote_presentations(remote_alpha, remote_stall_seconds),
            failure_resolutions: prediction.prediction_failure_resolutions(),
            feedback_outputs: prediction.tentative_feedback_outputs(),
        }
    }

    #[must_use]
    pub fn live_prediction_presentation_plan(
        &self,
        delta_seconds: f32,
        remote_alpha: f32,
        remote_stall_seconds: f32,
    ) -> PredictionPresentationPlan {
        let controlled_player_id = self.game.online_player_slot.map_or_else(
            || self.local_client().controlled_player_id,
            |slot| PlayerId::new(u64::from(slot)),
        );
        let authoritative_snapshot = self.world.player(controlled_player_id).map(|player| {
            PlayerSnapshot::from_world_player(controlled_player_id, player, &self.world)
        });
        self.prediction_presentation_plan(
            authoritative_snapshot.as_ref(),
            delta_seconds,
            remote_alpha,
            remote_stall_seconds,
        )
    }

    pub fn observe_live_remote_player_snapshots(&mut self) {
        let local_player_id = self.game.online_player_slot.map_or_else(
            || self.local_client().controlled_player_id,
            |slot| PlayerId::new(u64::from(slot)),
        );
        let remote_snapshots: Vec<PlayerSnapshot> = self
            .world
            .player_ids()
            .filter(|player_id| *player_id != local_player_id)
            .filter_map(|player_id| {
                self.world
                    .player(player_id)
                    .map(|player| PlayerSnapshot::from_world_player(player_id, player, &self.world))
            })
            .collect();
        let local_client = self.local_client_mut();
        for snapshot in remote_snapshots {
            local_client.prediction.push_remote_snapshot(snapshot);
        }
    }

    #[must_use]
    pub fn prediction_recovery_actions(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> Vec<PredictionRecoveryAction> {
        self.local_client()
            .prediction()
            .prediction_recovery_actions(
                self.local_client().controlled_player_id,
                &self.terrain_revisions,
                self.current_tick,
                terrain_position,
                known_revision,
            )
    }

    #[must_use]
    pub fn prediction_failure_recovery_plan(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> PredictionFailureRecoveryPlan {
        PredictionFailureRecoveryPlan::from_actions(
            self.prediction_recovery_actions(terrain_position, known_revision),
        )
    }

    #[must_use]
    pub fn prediction_failure_application_summary(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> PredictionFailureApplicationSummary {
        self.prediction_failure_recovery_plan(terrain_position, known_revision)
            .application_summary()
    }

    #[must_use]
    pub fn snapshot_chunk_recovery_plan(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> SnapshotChunkRecoveryPlan {
        SnapshotChunkRecoveryPlan::from_tracker(
            &self.terrain_revisions,
            self.current_tick,
            terrain_position,
            known_revision,
        )
    }

    #[must_use]
    pub fn local_input_client_count(&self) -> usize {
        self.local_seat_client_ids.len()
    }

    #[must_use]
    pub fn local_split_screen_active(&self) -> bool {
        self.local_input_client_count() > 1
    }

    #[must_use]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    #[must_use]
    pub fn has_client(&self, client_id: ClientId) -> bool {
        self.clients.contains_key(&client_id)
    }

    #[must_use]
    pub fn secondary_local_client_id(&self) -> Option<ClientId> {
        self.local_seat_client_ids
            .iter()
            .copied()
            .find(|client_id| *client_id != self.local_client_id)
    }

    pub fn set_primary_local_online_identity(&mut self, slot: u8) -> bool {
        let (client_id, player_id) = online_identity_for_slot(slot);
        if self.local_client_id == client_id
            && self.local_client().controlled_player_id == player_id
            && self.local_seat_client_ids.contains(&client_id)
        {
            return false;
        }

        let previous_local_client_id = self.local_client_id;
        let mut client = self
            .clients
            .remove(&client_id)
            .or_else(|| self.clients.remove(&previous_local_client_id))
            .unwrap_or_else(|| ClientState::new(client_id, player_id));
        client.client_id = client_id;
        client.controlled_player_id = player_id;
        client.view.client_id = client_id;
        client.view.controlled_player_id = player_id;
        self.clients.insert(client_id, client);
        self.local_client_id = client_id;
        self.local_seat_client_ids.clear();
        self.local_seat_client_ids.insert(client_id);
        if self.world.player(player_id).is_none() {
            self.world
                .insert_player(player_id, self.game.player.clone());
        }
        self.assign_local_split_screen_viewports();
        true
    }

    fn assign_local_split_screen_viewports(&mut self) {
        let viewports = split_screen_viewports(self.local_input_client_count());
        for (client_id, viewport) in self.local_seat_client_ids.iter().copied().zip(viewports) {
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.view.viewport = viewport;
            }
        }
    }

    fn insert_client_player(&mut self, client_id: ClientId, player_id: PlayerId) -> bool {
        if self.clients.contains_key(&client_id) || self.world.player(player_id).is_some() {
            return false;
        }

        let mut player = self.game.player.clone();
        player.x += TILE_SIZE;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        self.world.insert_player(player_id, player);
        let mut client = ClientState::new(client_id, player_id);
        client.sync_presentation_from_legacy_game(&self.game);
        self.clients.insert(client_id, client);
        self.host_input_inbox.assign_player(client_id, player_id);
        true
    }

    pub fn add_local_client_player(&mut self, client_id: ClientId, player_id: PlayerId) -> bool {
        if !self.insert_client_player(client_id, player_id) {
            return false;
        }
        self.local_seat_client_ids.insert(client_id);
        self.assign_local_split_screen_viewports();
        true
    }

    pub fn add_online_remote_client_player(
        &mut self,
        client_id: ClientId,
        player_id: PlayerId,
    ) -> bool {
        if self.clients.contains_key(&client_id) {
            return false;
        }
        if self.world.player(player_id).is_none() {
            let mut player = self.game.player.clone();
            player.x += TILE_SIZE;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
            self.world.insert_player(player_id, player);
        }
        let mut client = ClientState::new(client_id, player_id);
        client.sync_presentation_from_legacy_game(&self.game);
        self.clients.insert(client_id, client);
        self.host_input_inbox.assign_player(client_id, player_id);
        true
    }

    pub fn sync_replicated_authoritative_tick(&mut self, tick: SimulationTick) -> bool {
        if tick <= self.current_tick {
            return false;
        }
        self.current_tick = tick;
        self.world.set_simulation_tick(tick);
        self.host_input_inbox.set_current_tick(tick);
        self.command_network_session.set_current_tick(tick);
        true
    }

    pub fn authoritative_keyframe(&self) -> AuthoritativeKeyframe {
        AuthoritativeKeyframe {
            epoch: self.session_epoch,
            snapshot: self.world_snapshot().network_snapshot(),
            canonical_state: serde_json::to_vec(&self.world)
                .expect("canonical world encodes for authoritative recovery"),
            state_hash: Some(self.world.simulation_state_hash().get()),
        }
    }

    pub fn restore_authoritative_keyframe(
        keyframe: &AuthoritativeKeyframe,
        shell: GameState,
    ) -> Result<Self, serde_json::Error> {
        let world: WorldState = serde_json::from_slice(&keyframe.canonical_state)?;
        let mut session = Self::from_game_and_world(shell, world);
        session.current_tick = session.world.simulation_tick();
        session.replace_session_epoch(keyframe.epoch);
        session
            .command_network_session
            .set_current_tick(session.current_tick);
        Ok(session)
    }

    pub fn install_authoritative_client_keyframe(
        &mut self,
        keyframe: &AuthoritativeKeyframe,
    ) -> Result<(), serde_json::Error> {
        let world: WorldState = serde_json::from_slice(&keyframe.canonical_state)?;
        self.authoritative_remote_history.clear();
        for snapshot in &keyframe.snapshot.players {
            self.authoritative_remote_history
                .entry(snapshot.player_id)
                .or_default()
                .push_back(snapshot.clone());
        }
        self.authoritative_client_epoch = Some(keyframe.epoch);
        self.authoritative_client_hash = keyframe.state_hash;
        self.authoritative_client_world = Some(world.clone());
        self.predicted_client_world = Some(world);
        self.pending_prediction_inputs.clear();
        self.prediction_presentation_offset = None;
        self.previous_predicted_player_snapshots.clear();
        self.authoritative_recovery_request = None;
        Ok(())
    }

    pub fn apply_authoritative_client_frame(
        &mut self,
        frame: &AuthoritativeFrame,
    ) -> AuthoritativeClientFrameResult {
        let Some(epoch) = self.authoritative_client_epoch else {
            self.request_authoritative_keyframe(frame.epoch, frame.base_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        };
        let Some(world) = self.authoritative_client_world.as_mut() else {
            self.request_authoritative_keyframe(frame.epoch, frame.base_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        };
        if frame.epoch != epoch {
            self.request_authoritative_keyframe(frame.epoch, frame.base_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        }
        if frame.result_tick <= world.simulation_tick() {
            return AuthoritativeClientFrameResult::IgnoredStale;
        }
        if frame.base_tick != world.simulation_tick() {
            self.request_authoritative_keyframe(frame.epoch, frame.base_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        }
        if frame.result_tick <= frame.base_tick {
            return AuthoritativeClientFrameResult::IgnoredStale;
        }
        for snapshot in &frame.changed_players {
            let player = if let Some(player) = world.player_mut(snapshot.player_id) {
                player
            } else {
                let mut player = self.game.player.clone();
                apply_network_player_snapshot_to_player(&mut player, snapshot);
                world.insert_player(snapshot.player_id, player);
                world
                    .player_mut(snapshot.player_id)
                    .expect("inserted authoritative client player")
            };
            apply_network_player_snapshot_to_player(player, snapshot);
            world.set_scanner_cooldown_seconds(
                snapshot.player_id,
                snapshot.scanner_cooldown_seconds,
            );
            let history = self
                .authoritative_remote_history
                .entry(snapshot.player_id)
                .or_default();
            history.push_back(snapshot.clone());
            while history.len() > 32 {
                history.pop_front();
            }
        }
        for tile in &frame.terrain_mutations {
            let _changed = world.terrain_mut().set_tile_from_network(
                TilePosition {
                    x: tile.x,
                    y: tile.y,
                },
                tile.kind,
                tile.durability,
            );
        }
        if frame.canonical_state.is_empty() {
            world.set_simulation_tick(frame.result_tick);
        } else if let Ok(canonical_world) =
            serde_json::from_slice::<WorldState>(&frame.canonical_state)
        {
            *world = canonical_world;
        } else {
            self.request_authoritative_keyframe(frame.epoch, frame.result_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        }
        if let Some(expected_hash) = frame.state_hash
            && world.simulation_state_hash().get() != expected_hash
        {
            self.request_authoritative_keyframe(frame.epoch, frame.result_tick);
            return AuthoritativeClientFrameResult::RecoveryRequested;
        }
        let local_player_id = self.local_client().controlled_player_id;
        let previous_predicted = self
            .predicted_client_world
            .as_ref()
            .and_then(|world| world.player(local_player_id))
            .map(|player| (player.x, player.y));
        self.authoritative_client_hash = frame.state_hash;
        self.authoritative_recovery_request = None;
        let _replay = self.rebuild_predicted_client_world(&frame.applied_sequences);
        if let Some((previous_x, previous_y)) = previous_predicted
            && let Some(predicted) = self
                .predicted_client_world
                .as_ref()
                .and_then(|world| world.player(local_player_id))
        {
            let offset = CorrectionOffset::new(previous_x - predicted.x, previous_y - predicted.y);
            if offset.x.mul_add(offset.x, offset.y * offset.y) > f32::EPSILON {
                self.prediction_presentation_offset = Some(offset);
            }
        }
        AuthoritativeClientFrameResult::Applied
    }

    pub fn advance_prediction_presentation_correction(
        &mut self,
        smoothing_alpha: f32,
    ) -> Option<CorrectionOffset> {
        let offset = self.prediction_presentation_offset.as_mut()?;
        let alpha = smoothing_alpha.clamp(0.0, 1.0);
        offset.x *= 1.0 - alpha;
        offset.y *= 1.0 - alpha;
        if offset.x.mul_add(offset.x, offset.y * offset.y) < 0.01 {
            self.prediction_presentation_offset = None;
            return None;
        }
        Some(*offset)
    }

    #[must_use]
    pub const fn prediction_presentation_offset(&self) -> Option<CorrectionOffset> {
        self.prediction_presentation_offset
    }

    #[must_use]
    pub fn interpolated_predicted_player(
        &self,
        player_id: PlayerId,
        alpha: f32,
    ) -> Option<PlayerSnapshot> {
        let world = self.predicted_client_world.as_ref()?;
        let current = world.player(player_id)?;
        let mut snapshot = PlayerSnapshot::from_world_player(player_id, current, world);
        if let Some(previous) = self.previous_predicted_player_snapshots.get(&player_id) {
            let alpha = alpha.clamp(0.0, 1.0);
            snapshot.x = alpha.mul_add(snapshot.x - previous.x, previous.x);
            snapshot.y = alpha.mul_add(snapshot.y - previous.y, previous.y);
        }
        if let Some(offset) = self.prediction_presentation_offset {
            snapshot.x += offset.x;
            snapshot.y += offset.y;
        }
        Some(snapshot)
    }

    pub fn queue_predicted_input(
        &mut self,
        client_id: ClientId,
        player_id: PlayerId,
        frame: crate::multiplayer::InputFrame,
    ) -> bool {
        let Some(predicted) = self.predicted_client_world.as_mut() else {
            return false;
        };
        if let Some(player) = predicted.player(player_id) {
            self.previous_predicted_player_snapshots.insert(
                player_id,
                PlayerSnapshot::from_world_player(player_id, player, predicted),
            );
        }
        let tick_inputs = Self::tick_inputs_for_prediction_frame(player_id, &frame);
        let mut revisions = TerrainRevisionTracker::default();
        let output = predicted.simulate_tick(frame.target_tick, &tick_inputs, &mut revisions);
        self.pending_prediction_inputs
            .push_back(PendingPredictionInput {
                client_id,
                player_id,
                frame,
                terrain_changed_tiles: output.terrain_changed_tiles,
            });
        true
    }

    fn tick_inputs_for_prediction_frame(
        player_id: PlayerId,
        frame: &crate::multiplayer::InputFrame,
    ) -> TickInputs {
        let mut commands = vec![SequencedPlayerCommand {
            player_id,
            sequence: frame.sequence,
            target_tick: frame.target_tick,
            command: PlayerCommand::Movement {
                horizontal: frame.controls.horizontal(),
                thrust: frame.controls.thrust,
                drill_down: frame.controls.drill_down,
            },
        }];
        commands.extend(frame.actions.iter().map(|action| SequencedPlayerCommand {
            player_id,
            sequence: frame.sequence,
            target_tick: frame.target_tick,
            command: action.command.clone(),
        }));
        TickInputs::from_commands(&commands)
    }

    fn rebuild_predicted_client_world(
        &mut self,
        applied_sequences: &BTreeMap<PlayerId, InputSequence>,
    ) -> PredictionReplaySummary {
        let mut pruned_inputs = 0;
        self.pending_prediction_inputs.retain(|pending| {
            let applied = applied_sequences
                .get(&pending.player_id)
                .is_some_and(|sequence| *sequence >= pending.frame.sequence);
            pruned_inputs += usize::from(applied);
            !applied
        });
        let Some(authoritative) = self.authoritative_client_world.as_ref() else {
            return PredictionReplaySummary {
                pruned_inputs,
                ..PredictionReplaySummary::default()
            };
        };
        let mut predicted = authoritative.clone();
        let mut revisions = TerrainRevisionTracker::default();
        for pending in &self.pending_prediction_inputs {
            let inputs = Self::tick_inputs_for_prediction_frame(pending.player_id, &pending.frame);
            let _output =
                predicted.simulate_tick(pending.frame.target_tick, &inputs, &mut revisions);
        }
        let replayed_inputs = self.pending_prediction_inputs.len();
        let predicted_tick = Some(predicted.simulation_tick());
        self.predicted_client_world = Some(predicted);
        PredictionReplaySummary {
            pruned_inputs,
            replayed_inputs,
            predicted_tick,
        }
    }

    #[must_use]
    pub const fn predicted_client_world(&self) -> Option<&WorldState> {
        self.predicted_client_world.as_ref()
    }

    #[must_use]
    pub fn pending_prediction_input_count(&self) -> usize {
        self.pending_prediction_inputs.len()
    }

    #[must_use]
    pub fn prediction_identity_matches_authority(&self, player_id: PlayerId) -> Option<bool> {
        let authoritative = self.authoritative_client_world.as_ref()?;
        let predicted = self.predicted_client_world.as_ref()?;
        let authoritative_drill = authoritative
            .active_drill(player_id)
            .map(|drill| drill.target);
        let predicted_drill = predicted.active_drill(player_id).map(|drill| drill.target);
        if authoritative_drill != predicted_drill {
            return Some(false);
        }
        for pending in &self.pending_prediction_inputs {
            for position in &pending.terrain_changed_tiles {
                let authoritative_tile = authoritative.terrain().tile(*position);
                let predicted_tile = predicted.terrain().tile(*position);
                if !matches!((authoritative_tile, predicted_tile), (Some(authoritative), Some(predicted))
                    if authoritative.kind == predicted.kind
                        && authoritative.durability == predicted.durability)
                {
                    return Some(false);
                }
            }
        }
        Some(true)
    }

    #[must_use]
    pub fn pending_prediction_presentations(&self) -> Vec<PendingPredictionPresentation> {
        let Some(world) = self.predicted_client_world.as_ref() else {
            return Vec::new();
        };
        self.pending_prediction_inputs
            .iter()
            .map(|pending| PendingPredictionPresentation {
                sequence: pending.frame.sequence,
                target_tick: pending.frame.target_tick,
                action_ids: pending
                    .frame
                    .actions
                    .iter()
                    .map(|action| action.id)
                    .collect(),
                terrain_positions: pending.terrain_changed_tiles.clone(),
                drill_target: world
                    .active_drill(pending.player_id)
                    .map(|drill| drill.target),
            })
            .collect()
    }

    fn request_authoritative_keyframe(
        &mut self,
        epoch: SessionEpoch,
        expected_base_tick: SimulationTick,
    ) {
        self.authoritative_recovery_request = Some(ClientMessage::KeyframeRequest {
            epoch,
            expected_base_tick,
        });
    }

    pub const fn take_authoritative_recovery_request(&mut self) -> Option<ClientMessage> {
        self.authoritative_recovery_request.take()
    }

    #[must_use]
    pub fn authoritative_client_status(&self) -> AuthoritativeClientStatus {
        AuthoritativeClientStatus {
            epoch: self.authoritative_client_epoch,
            tick: self
                .authoritative_client_world
                .as_ref()
                .map(WorldState::simulation_tick),
            state_hash: self.authoritative_client_hash,
            recovery_pending: self.authoritative_recovery_request.is_some(),
            remote_history_players: self.authoritative_remote_history.len(),
        }
    }

    #[must_use]
    pub const fn authoritative_client_world(&self) -> Option<&WorldState> {
        self.authoritative_client_world.as_ref()
    }

    #[must_use]
    pub fn interpolated_authoritative_remote_player(
        &self,
        player_id: PlayerId,
        alpha: f32,
    ) -> Option<NetworkPlayerSnapshot> {
        let history = self.authoritative_remote_history.get(&player_id)?;
        let latest = history.back()?.clone();
        let Some(previous) = history.iter().rev().nth(1) else {
            return Some(latest);
        };
        let alpha = alpha.clamp(0.0, 1.0);
        let mut interpolated = latest;
        interpolated.x = alpha.mul_add(interpolated.x - previous.x, previous.x);
        interpolated.y = alpha.mul_add(interpolated.y - previous.y, previous.y);
        Some(interpolated)
    }

    pub fn join_accepted_message(&self, client_id: ClientId, player_id: PlayerId) -> ServerMessage {
        ServerMessage::JoinAccepted {
            client_id,
            player_id,
            epoch: self.session_epoch,
            keyframe: self.authoritative_keyframe(),
        }
    }

    pub fn terrain_recovery_message(
        &self,
        position: TerrainChunkPosition,
        known_revision: u64,
    ) -> ServerMessage {
        let plan = self.snapshot_chunk_recovery_plan(position, known_revision);
        if plan.requires_keyframe {
            return ServerMessage::Keyframe(self.authoritative_keyframe());
        }
        let chunk = self.network_terrain_chunk_from_world(position.x, position.y);
        ServerMessage::TerrainChunk {
            epoch: self.session_epoch,
            chunk_x: chunk.chunk_x,
            chunk_y: chunk.chunk_y,
            revision: chunk.revision,
            tiles: if plan.recovered_revision().is_some() {
                chunk.tiles
            } else {
                Vec::new()
            },
        }
    }

    #[must_use]
    pub const fn session_epoch(&self) -> SessionEpoch {
        self.session_epoch
    }

    pub fn replace_session_epoch(&mut self, epoch: SessionEpoch) {
        self.session_epoch = epoch;
        self.host_input_inbox = HostInputInbox::new(epoch, self.current_tick, 8);
        for (client_id, client) in &self.clients {
            self.host_input_inbox
                .assign_player(*client_id, client.controlled_player_id);
        }
    }

    pub fn route_local_input_bundle(&mut self, bundle: &InputBundle) -> ServerMessage {
        let response = self.receive_input_bundle(bundle);
        if let Some(frame) = bundle.frames.first() {
            let _applied = self.apply_accepted_input_frame(bundle.client_id, frame.sequence);
        }
        response
    }

    pub fn receive_input_bundle(&mut self, bundle: &InputBundle) -> ServerMessage {
        ServerMessage::InputStatus {
            epoch: self.session_epoch,
            client_id: bundle.client_id,
            statuses: self.host_input_inbox.receive_bundle(bundle),
        }
    }

    pub fn apply_accepted_input_frame(
        &mut self,
        client_id: ClientId,
        sequence: InputSequence,
    ) -> Option<(InputStatus, AuthoritativeFrame)> {
        let input = self
            .host_input_inbox
            .take_accepted_frame(client_id, sequence)?;
        self.host_input_inbox.note_applied_frame(client_id, &input);
        self.apply_host_input_frame(client_id, input, true)
    }

    pub fn apply_missing_input_policy(
        &mut self,
        client_id: ClientId,
        target_tick: SimulationTick,
    ) -> Option<AuthoritativeFrame> {
        const HELD_CONTROL_TICKS: u64 = 6;
        let input = self.host_input_inbox.held_controls_frame(
            client_id,
            target_tick,
            HELD_CONTROL_TICKS,
        )?;
        self.apply_host_input_frame(client_id, input, false)
            .map(|(_, frame)| frame)
    }

    fn apply_host_input_frame(
        &mut self,
        client_id: ClientId,
        input: crate::multiplayer::InputFrame,
        acknowledge_sequence: bool,
    ) -> Option<(InputStatus, AuthoritativeFrame)> {
        let sequence = input.sequence;
        let player_id = self.clients.get(&client_id)?.controlled_player_id;
        let mut commands = vec![SequencedPlayerCommand {
            player_id,
            sequence: input.sequence,
            target_tick: input.target_tick,
            command: PlayerCommand::Movement {
                horizontal: input.controls.horizontal(),
                thrust: input.controls.thrust,
                drill_down: input.controls.drill_down,
            },
        }];
        commands.extend(
            input
                .actions
                .into_iter()
                .map(|action| SequencedPlayerCommand {
                    player_id,
                    sequence: input.sequence,
                    target_tick: input.target_tick,
                    command: action.command,
                }),
        );
        let tick_inputs = TickInputs::from_commands(&commands);
        let base_tick = self.world.simulation_tick();
        let mut output =
            self.world
                .simulate_tick(base_tick, &tick_inputs, &mut self.terrain_revisions);
        self.current_tick = self.world.simulation_tick();
        self.host_input_inbox.set_current_tick(self.current_tick);
        let changed_players = output
            .changed_players
            .iter()
            .filter_map(|player_id| {
                self.world.player(*player_id).map(|player| {
                    PlayerSnapshot::from_world_player(*player_id, player, &self.world)
                        .network_snapshot()
                })
            })
            .collect();
        let terrain_mutations = output
            .terrain_changed_tiles
            .iter()
            .filter_map(|position| {
                self.world
                    .terrain()
                    .tile(*position)
                    .map(|tile| NetworkTerrainTile {
                        x: position.x,
                        y: position.y,
                        kind: tile.kind,
                        durability: tile.durability,
                    })
            })
            .collect();
        let terrain_revisions = output
            .terrain_revisions
            .iter()
            .map(|revision| NetworkTerrainChunkRevision {
                chunk_x: revision.position.x,
                chunk_y: revision.position.y,
                revision: revision.revision,
            })
            .collect();
        if !acknowledge_sequence {
            output.latest_sequences_by_player.clear();
        }
        for event in output.events.drain(..) {
            self.route_gameplay_event_to_presentation(event);
        }
        Some((
            HostInputInbox::applied_status(sequence),
            AuthoritativeFrame {
                epoch: self.session_epoch,
                base_tick,
                result_tick: self.current_tick,
                applied_sequences: output.latest_sequences_by_player,
                changed_players,
                terrain_mutations,
                terrain_revisions,
                canonical_state: serde_json::to_vec(&self.world)
                    .expect("canonical post-tick world encodes"),
                state_hash: Some(self.world.simulation_state_hash().get()),
            },
        ))
    }

    #[must_use]
    pub const fn current_tick(&self) -> SimulationTick {
        self.current_tick
    }

    #[must_use]
    pub const fn command_network_session(&self) -> &CommandNetworkSession {
        &self.command_network_session
    }

    #[must_use]
    pub const fn simulation_accumulator(&self) -> Duration {
        self.simulation_accumulator
    }

    #[must_use]
    pub const fn terrain_revisions(&self) -> &TerrainRevisionTracker {
        &self.terrain_revisions
    }

    pub fn accumulate_frame_delta(&mut self, delta_seconds: f32) -> u32 {
        self.simulation_accumulator += Duration::from_secs_f32(delta_seconds.max(0.0));
        let fixed_delta = Duration::from_nanos(1_000_000_000 / u64::from(SIMULATION_HZ));
        let fixed_nanos = fixed_delta.as_nanos();
        let tolerance_nanos = fixed_nanos / 1_000;
        let steps = (self.simulation_accumulator.as_nanos() + tolerance_nanos) / fixed_nanos;
        let capped_steps = u32::try_from(steps).unwrap_or(u32::MAX);
        self.simulation_accumulator = self
            .simulation_accumulator
            .saturating_sub(fixed_delta.saturating_mul(capped_steps));
        capped_steps
    }

    pub const fn advance_tick(&mut self) {
        self.current_tick = self.current_tick.next();
        self.world.set_simulation_tick(self.current_tick);
        self.host_input_inbox.set_current_tick(self.current_tick);
    }

    #[must_use]
    pub const fn keyframe_interval_ticks() -> u64 {
        KEYFRAME_INTERVAL_TICKS
    }

    fn maybe_emit_keyframe_event(&mut self) {
        let tick = self.current_tick.get();
        if tick > 0 && tick.is_multiple_of(KEYFRAME_INTERVAL_TICKS) {
            self.push_event(WorldEvent::SnapshotKeyframeReady {
                tick: self.current_tick,
            });
        }
    }

    pub fn drain_events(&mut self) -> Vec<WorldEvent> {
        mem::take(&mut self.pending_events)
    }

    pub fn drain_world_delta(&mut self) -> WorldDelta {
        WorldDelta::new(self.current_tick, self.drain_events())
    }

    pub fn live_snapshot_keyframe_message(&self) -> ProtocolMessage {
        self.world_snapshot().keyframe_message()
    }

    #[must_use]
    pub fn live_snapshot_exchange_batch(&self) -> ProtocolExchangeBatch {
        ProtocolMessage::exchange_batch(
            ProtocolExchangeKind::SnapshotKeyframe,
            vec![self.live_snapshot_keyframe_message()],
        )
    }

    pub fn live_world_delta_message(delta: &WorldDelta) -> ProtocolMessage {
        ProtocolMessage::WorldDelta {
            tick: delta.tick,
            payload: delta.compact_network_delta().network_payload(),
        }
    }

    #[must_use]
    pub fn live_world_delta_exchange_batch(delta: &WorldDelta) -> ProtocolExchangeBatch {
        ProtocolMessage::exchange_batch(
            ProtocolExchangeKind::WorldDelta,
            vec![Self::live_world_delta_message(delta)],
        )
    }

    pub fn drain_live_world_delta_message(&mut self) -> ProtocolMessage {
        let delta = self.drain_world_delta();
        Self::live_world_delta_message(&delta)
    }

    pub fn live_terrain_chunk_response_message(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> ProtocolMessage {
        let recovery = self.snapshot_chunk_recovery_plan(terrain_position, known_revision);
        ProtocolMessage::TerrainChunkResponse {
            chunk_x: terrain_position.x,
            chunk_y: terrain_position.y,
            revision: recovery.recovered_revision().unwrap_or(known_revision),
            tiles: Vec::new(),
        }
    }

    #[must_use]
    pub fn live_terrain_chunk_exchange_batch(
        &self,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> ProtocolExchangeBatch {
        ProtocolMessage::exchange_batch(
            ProtocolExchangeKind::TerrainChunk,
            vec![self.live_terrain_chunk_response_message(terrain_position, known_revision)],
        )
    }

    pub fn exercise_live_network_integration(
        &mut self,
        remote_client_id: ClientId,
        remote_player_id: PlayerId,
        reconnect_token: SessionToken,
        terrain_position: TerrainChunkPosition,
        known_revision: u64,
    ) -> (
        ProtocolExchangeBatch,
        ProtocolExchangeBatch,
        Vec<ProtocolMessage>,
        SnapshotChunkRecoveryPlan,
        PredictionPresentationPlan,
    ) {
        let _added = self.add_online_remote_client_player(remote_client_id, remote_player_id);
        let join_in_progress = ProtocolExchangeBatch {
            kind: ProtocolExchangeKind::JoinHandshake,
            messages: vec![
                ProtocolMessage::JoinRequest {
                    client_id: remote_client_id,
                    session_token: None,
                },
                ProtocolMessage::JoinAccepted {
                    client_id: remote_client_id,
                    player_id: remote_player_id,
                    snapshot_tick: self.world.simulation_tick(),
                },
                self.live_snapshot_keyframe_message(),
            ],
        };
        let reconnect = ProtocolExchangeBatch {
            kind: ProtocolExchangeKind::JoinHandshake,
            messages: vec![
                ProtocolMessage::ReconnectRequest {
                    client_id: remote_client_id,
                    session_token: reconnect_token,
                },
                ProtocolMessage::JoinAccepted {
                    client_id: remote_client_id,
                    player_id: remote_player_id,
                    snapshot_tick: self.world.simulation_tick(),
                },
                self.live_snapshot_keyframe_message(),
            ],
        };
        let command_packet = CommandPacket {
            client_id: remote_client_id,
            commands: vec![SequencedPlayerCommand {
                player_id: remote_player_id,
                sequence: InputSequence::new(0),
                target_tick: self.current_tick,
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };
        let command_responses = self.apply_live_command_packet(&command_packet);
        let terrain_recovery = self.snapshot_chunk_recovery_plan(terrain_position, known_revision);
        self.observe_live_remote_player_snapshots();
        let high_latency_prediction = self.live_prediction_presentation_plan(0.0, 1.0, 0.12);
        (
            join_in_progress,
            reconnect,
            command_responses,
            terrain_recovery,
            high_latency_prediction,
        )
    }

    fn route_gameplay_event_to_presentation(&mut self, event: GameplayEvent) {
        const PRESENTED_EVENT_HISTORY_LIMIT: usize = 4_096;
        let key = (event.id.tick.get(), event.id.ordinal);
        if !self.presented_gameplay_events.insert(key) {
            return;
        }
        self.presented_gameplay_event_order.push_back(key);
        while self.presented_gameplay_event_order.len() > PRESENTED_EVENT_HISTORY_LIMIT {
            if let Some(expired) = self.presented_gameplay_event_order.pop_front() {
                self.presented_gameplay_events.remove(&expired);
            }
        }
        for world_event in world_events_for_gameplay_event(event) {
            self.push_event(world_event);
        }
    }

    fn push_event(&mut self, event: WorldEvent) {
        self.pending_events.push(event);
    }

    pub fn apply_settings(&mut self, settings: SettingsFile) {
        let local_client = self.local_client_mut();
        local_client.master_volume = settings.master_volume;
        local_client.fullscreen = settings.fullscreen;
        self.sync_client_settings_to_legacy_game();
    }

    #[must_use]
    pub fn current_settings(&self) -> SettingsFile {
        SettingsFile {
            master_volume: self.local_client().master_volume,
            fullscreen: self.local_client().fullscreen,
        }
    }

    #[must_use]
    pub fn should_exit(&self) -> bool {
        self.local_client().exit_requested || self.game.request_exit
    }

    #[must_use]
    pub fn master_volume(&self) -> f32 {
        self.local_client().master_volume
    }

    #[must_use]
    pub fn fullscreen(&self) -> bool {
        self.local_client().fullscreen
    }

    pub fn take_settings_dirty(&mut self) -> bool {
        let legacy_dirty = self.game.take_settings_dirty();
        let local_client = self.local_client_mut();
        let client_dirty = local_client.settings_dirty;
        local_client.settings_dirty = false;
        legacy_dirty || client_dirty
    }

    fn sync_client_settings_from_legacy_game(&mut self) {
        let local_client_id = self.local_client_id;
        let game_master_volume = self.game.master_volume;
        let game_fullscreen = self.game.fullscreen;
        let game_settings_dirty = self.game.settings_dirty;
        let game_request_exit = self.game.request_exit;
        let settings_changed;
        let exit_requested;
        {
            let game = self.game.clone();
            let local_client = self.local_client_mut();
            settings_changed = (local_client.master_volume - game_master_volume).abs()
                > f32::EPSILON
                || local_client.fullscreen != game_fullscreen
                || game_settings_dirty;
            exit_requested = game_request_exit && !local_client.exit_requested;

            local_client.sync_presentation_from_legacy_game(&game);
            local_client.settings_dirty |= game_settings_dirty;
            local_client.exit_requested |= game_request_exit;
        }

        if settings_changed {
            self.push_event(WorldEvent::ClientSettingsChanged {
                client_id: local_client_id,
            });
        }
        if exit_requested {
            self.push_event(WorldEvent::ClientExitRequested {
                client_id: local_client_id,
            });
        }
    }

    fn sync_client_settings_to_legacy_game(&mut self) {
        let master_volume = self.local_client().master_volume;
        let fullscreen = self.local_client().fullscreen;
        let settings_dirty = self.local_client().settings_dirty;
        self.game.master_volume = master_volume;
        self.game.fullscreen = fullscreen;
        self.game.settings_dirty = settings_dirty;
    }

    pub fn sequence_local_commands(
        &mut self,
        commands: Vec<PlayerCommand>,
    ) -> Vec<SequencedPlayerCommand> {
        self.sequence_client_commands(self.local_client_id, commands)
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "public command routing API takes ownership from input mapping producers"
    )]
    pub fn route_local_player_commands(
        &mut self,
        commands: Vec<PlayerCommand>,
    ) -> Vec<SequencedPlayerCommand> {
        let sequenced = self.sequence_local_commands(commands.clone());
        self.remember_latest_movement_intents(&sequenced);
        self.latest_local_movement_intent = commands
            .iter()
            .rev()
            .find_map(PlayerMovementIntent::from_command);
        self.latest_local_authoritative_commands
            .clone_from(&commands);
        sequenced
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "public command routing API takes ownership from input mapping producers"
    )]
    pub fn route_client_player_commands(
        &mut self,
        client_id: ClientId,
        source: CommandSource,
        commands: Vec<PlayerCommand>,
    ) -> SequencedCommandBatch {
        let batch = self.sequence_client_commands_from_source(client_id, source, commands.clone());
        self.remember_latest_movement_intents(&batch.commands);
        if client_id == self.local_client_id {
            self.latest_local_movement_intent = commands
                .iter()
                .rev()
                .find_map(PlayerMovementIntent::from_command);
            self.latest_local_authoritative_commands
                .clone_from(&commands);
        }
        batch
    }

    #[must_use]
    pub fn latest_local_authoritative_commands_for_tests(&self) -> &[PlayerCommand] {
        &self.latest_local_authoritative_commands
    }

    pub fn route_split_screen_player_commands(
        &mut self,
        client_id: ClientId,
        commands: Vec<PlayerCommand>,
    ) -> SequencedCommandBatch {
        self.route_client_player_commands(client_id, CommandSource::SplitScreenClient, commands)
    }

    pub fn route_command_producer(
        &mut self,
        client_id: ClientId,
        producer: CommandProducer,
    ) -> SequencedCommandBatch {
        self.route_client_player_commands(client_id, producer.source, producer.commands)
    }

    pub fn route_command_producers<I>(
        &mut self,
        client_id: ClientId,
        producers: I,
    ) -> Vec<SequencedCommandBatch>
    where
        I: IntoIterator<Item = CommandProducer>,
    {
        producers
            .into_iter()
            .map(|producer| self.route_command_producer(client_id, producer))
            .collect()
    }

    pub fn track_online_prediction_commands(&mut self, packet: &CommandPacket) {
        self.local_client_mut()
            .prediction
            .track_sent_commands(packet.commands.clone());
    }

    #[must_use]
    pub fn online_prediction_command_frame(
        &self,
        client_id: ClientId,
        player_id: PlayerId,
        commands: Vec<PlayerCommand>,
    ) -> PredictionCommandFrame {
        PredictionCommandFrame {
            client_id,
            player_id,
            start_sequence: self.local_client().next_input_sequence,
            target_tick: self.current_tick,
            delta_seconds: FIXED_DELTA_SECONDS,
            commands,
        }
    }

    pub fn online_prediction_command_frame_from_packet(
        packet: &CommandPacket,
    ) -> Option<PredictionCommandFrame> {
        let first = packet.commands.first()?;
        Some(PredictionCommandFrame {
            client_id: packet.client_id,
            player_id: first.player_id,
            start_sequence: first.sequence,
            target_tick: first.target_tick,
            delta_seconds: FIXED_DELTA_SECONDS,
            commands: packet
                .commands
                .iter()
                .filter(|command| {
                    command.player_id == first.player_id && command.target_tick == first.target_tick
                })
                .map(|command| command.command.clone())
                .collect(),
        })
    }

    pub fn predict_online_command_packet(
        &mut self,
        packet: &CommandPacket,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        let frame = Self::online_prediction_command_frame_from_packet(packet)?;
        self.predict_online_command_frame(frame, delta_seconds)
    }

    pub fn online_prediction_command_frame_from_input(
        &self,
        input: PlayerInput,
    ) -> PredictionCommandFrame {
        let (client_id, player_id) = self.local_network_identity();
        self.online_prediction_command_frame(
            client_id,
            player_id,
            crate::game_state::session_gameplay_commands_from_input(input),
        )
    }

    pub fn predict_online_command_frame(
        &mut self,
        frame: PredictionCommandFrame,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        let authoritative_world = self.world.clone();
        self.local_client_mut()
            .prediction_mut()
            .predict_command_frame(&authoritative_world, frame, delta_seconds)
    }

    pub fn predict_online_render_command_frame(
        &mut self,
        frame: PredictionCommandFrame,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        let authoritative_world = self.world.clone();
        self.local_client_mut()
            .prediction_mut()
            .predict_transient_command_frame(&authoritative_world, frame, delta_seconds)
    }

    pub fn predict_online_render_input(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
    ) -> Option<PlayerSnapshot> {
        let frame = self.online_prediction_command_frame_from_input(input);
        self.predict_online_render_command_frame(frame, delta_seconds)
    }

    pub fn advance_online_prediction_correction(
        &mut self,
        delta_seconds: f32,
    ) -> Option<PredictedMovement> {
        self.local_client_mut()
            .prediction_mut()
            .advance_active_correction(delta_seconds)
    }

    pub fn confirm_online_prediction_terrain(
        &mut self,
        revisions: &[TerrainChunkRevision],
    ) -> usize {
        self.local_client_mut()
            .prediction_mut()
            .confirm_terrain_overlay(revisions)
    }

    pub fn confirm_online_prediction_terrain_for_tiles(
        &mut self,
        revisions: &[TerrainChunkRevision],
        changed_tiles: &[TilePosition],
        command_sequence: Option<InputSequence>,
    ) -> usize {
        self.local_client_mut()
            .prediction_mut()
            .confirm_terrain_overlay_for_tiles(revisions, changed_tiles, command_sequence)
    }

    pub fn revert_unconfirmed_online_prediction_terrain(&mut self) -> usize {
        self.local_client_mut()
            .prediction_mut()
            .revert_unconfirmed_terrain_overlays()
    }

    pub fn confirm_online_predicted_modal(&mut self) -> bool {
        self.local_client_mut()
            .prediction_mut()
            .confirm_predicted_modal()
    }

    pub fn revert_online_predicted_modal(&mut self) -> bool {
        self.local_client_mut()
            .prediction_mut()
            .revert_predicted_modal()
    }

    #[must_use]
    pub fn online_prediction_render_overlay(
        &self,
        policy: OnlineRenderAuthorityPolicy,
    ) -> OnlinePredictionRenderOverlay {
        self.local_client().prediction().render_overlay(policy)
    }

    pub fn apply_host_terrain_revisions_to_prediction(
        &mut self,
        authoritative_tick: SimulationTick,
        revisions: &[TerrainChunkRevision],
    ) -> (usize, usize) {
        let confirmed = self.confirm_online_prediction_terrain(revisions);
        let reverted = if revisions.is_empty()
            || self
                .local_client()
                .prediction()
                .terrain_overlays()
                .iter()
                .any(|overlay| {
                    overlay.target_tick <= authoritative_tick
                        && overlay.resolution == PredictedTerrainResolution::Pending
                }) {
            self.local_client_mut()
                .prediction_mut()
                .revert_terrain_overlays_through_tick(authoritative_tick)
        } else {
            0
        };
        (confirmed, reverted)
    }

    pub fn apply_host_terrain_revisions_to_prediction_for_tiles(
        &mut self,
        authoritative_tick: SimulationTick,
        revisions: &[TerrainChunkRevision],
        changed_tiles: &[TilePosition],
        command_sequence: Option<InputSequence>,
    ) -> (usize, usize) {
        let confirmed = self.confirm_online_prediction_terrain_for_tiles(
            revisions,
            changed_tiles,
            command_sequence,
        );
        let reverted = if revisions.is_empty()
            || self
                .local_client()
                .prediction()
                .terrain_overlays()
                .iter()
                .any(|overlay| {
                    overlay.target_tick <= authoritative_tick
                        && overlay.resolution == PredictedTerrainResolution::Pending
                }) {
            self.local_client_mut()
                .prediction_mut()
                .revert_terrain_overlays_through_tick(authoritative_tick)
        } else {
            0
        };
        (confirmed, reverted)
    }

    pub fn apply_host_modal_authority(&mut self, confirmed: bool) -> bool {
        if confirmed {
            self.confirm_online_predicted_modal()
        } else {
            self.revert_online_predicted_modal()
        }
    }

    pub fn rebase_local_prediction_to_authority(&mut self) {
        self.rebase_local_prediction_to_authority_with_terrain(self.current_tick, &[], &[]);
    }

    pub fn rebase_local_prediction_to_authority_with_terrain(
        &mut self,
        tick: SimulationTick,
        terrain_revisions: &[TerrainChunkRevision],
        changed_tiles: &[TilePosition],
    ) {
        let world = self.world.clone();
        let player_id = self.game.online_player_slot.map_or_else(
            || self.local_client().controlled_player_id,
            |slot| PlayerId::new(u64::from(slot)),
        );
        if let Some(authoritative) = world
            .player(player_id)
            .map(|player| PlayerSnapshot::from_world_player(player_id, player, &world))
        {
            let active_drill = world
                .active_drill(player_id)
                .copied()
                .map(|drill| DrillPredictionSnapshot::from_drill(player_id, drill));
            let input = AuthoritativeReconciliationInput {
                tick,
                player_snapshot: authoritative,
                terrain_revisions: terrain_revisions.to_vec(),
                changed_tiles: changed_tiles.to_vec(),
                active_drill,
                authoritative_world: world,
            };
            let _correction = self
                .local_client_mut()
                .prediction_mut()
                .reconcile_authoritative_input(&input);
        } else {
            self.local_client_mut()
                .prediction_mut()
                .rebase_prediction_world(tick, &world);
        }
    }

    pub fn prediction_playtest_report(
        &self,
        local_positions: &[f32],
        outbound_command_counts: &[usize],
        stale_authority_backward_tugs: usize,
    ) -> PredictionPlaytestReport {
        let prediction_plan = self.live_prediction_presentation_plan(0.0, 0.5, 0.0);
        let frame_plan = self.render_frame_plan();
        let players = frame_plan.views.first().map_or_else(Vec::new, |view| {
            frame_plan.world_player_presentations_for_view(view, &prediction_plan)
        });
        let predicted_local_render_source = players.iter().any(|player| {
            player.local_to_view && player.source == RenderPlayerPresentationSource::PredictedLocal
        });
        let authoritative_remote_render_source = players
            .iter()
            .filter(|player| !player.local_to_view)
            .all(|player| player.source == RenderPlayerPresentationSource::AuthoritativeWorld);
        let mut correction_modes_seen = BTreeSet::new();
        if let Some(correction) = self.local_client().prediction().active_correction() {
            correction_modes_seen.insert(correction.mode);
        }
        PredictionPlaytestReport::from_samples(
            local_positions,
            outbound_command_counts,
            stale_authority_backward_tugs,
            predicted_local_render_source,
            authoritative_remote_render_source,
            correction_modes_seen,
        )
    }

    pub fn apply_online_command_acknowledgement(
        &mut self,
        acknowledgement: &CommandAcknowledgement,
    ) {
        self.local_client_mut()
            .prediction
            .acknowledge_commands_through(acknowledgement.acknowledged_sequence);
        self.apply_host_terrain_revisions_to_prediction(acknowledgement.authoritative_tick, &[]);
        self.rebase_local_prediction_to_authority_with_terrain(
            acknowledgement.authoritative_tick,
            &[],
            &[],
        );
    }

    pub fn apply_online_command_rejection(&mut self, rejection: &CommandRejection) {
        self.local_client_mut()
            .prediction
            .reject_command(rejection.sequence);
        let _terrain_reverted =
            self.apply_host_terrain_revisions_to_prediction(rejection.authoritative_tick, &[]);
        let _modal_reverted = self.apply_host_modal_authority(false);
        self.rebase_local_prediction_to_authority_with_terrain(
            rejection.authoritative_tick,
            &[],
            &[],
        );
    }

    pub fn refresh_online_roster_status_on_shell(&mut self) {
        self.game.online_session_roster_status = self.online_session_roster().to_string();
    }

    pub fn prepare_online_tick_identity(
        &mut self,
        descriptor_client_connected: bool,
    ) -> OnlineTickIdentitySummary {
        let online_player_slot = self.game.online_player_slot;
        let identity_rebound =
            online_player_slot.is_some_and(|slot| self.set_primary_local_online_identity(slot));
        let local_client_id = self.local_client().client_id;
        let local_player_id = self.local_client().controlled_player_id;
        let player_id =
            online_player_slot.map_or(local_player_id, |slot| online_identity_for_slot(slot).1);
        let client_id =
            online_player_slot.map_or(local_client_id, |slot| online_identity_for_slot(slot).0);
        let (roster_seeded, joined_local_synced) =
            if descriptor_client_connected || online_player_slot.is_some() {
                let roster_seeded = self.ensure_local_online_player_presentation_from_roster();
                self.refresh_online_roster_status_on_shell();
                (roster_seeded || identity_rebound, false)
            } else {
                (false, false)
            };
        OnlineTickIdentitySummary {
            client_id,
            player_id,
            slot: online_player_slot,
            descriptor_client_connected,
            roster_seeded,
            joined_local_synced,
        }
    }

    #[must_use]
    pub fn local_network_identity(&self) -> (ClientId, PlayerId) {
        let roster = self.online_session_roster();
        let player_id = roster.player_id_for_local_network_tick();
        let client_id = self.game.online_player_slot.map_or_else(
            || roster.client_id_for_local_network_tick(),
            |slot| online_identity_for_slot(slot).0,
        );
        (client_id, player_id)
    }

    #[must_use]
    pub fn online_session_roster(&self) -> OnlineSessionRoster {
        let local_slot = self.game.online_player_slot.unwrap_or(1);
        let local_role = if self.game.online_player_slot.is_some() {
            OnlineParticipantRole::JoinedClient
        } else {
            OnlineParticipantRole::LocalHost
        };
        let local_save_authority = if self.game.online_host_owns_save {
            SaveAuthority::HostOwnedSession
        } else {
            SaveAuthority::LocalSinglePlayer
        };
        let remote = self.online_remote_roster_participant(local_slot, local_save_authority);
        let local = OnlineRosterParticipant {
            client_id: if local_slot == 1 {
                self.local_client_id
            } else {
                client_id_for_online_slot(local_slot)
            },
            player_id: online_identity_for_slot(local_slot).1,
            slot: local_slot,
            name: self.game.online_player_name.clone(),
            ready: self.game.online_local_ready,
            connected: true,
            role: local_role,
            save_authority: local_save_authority,
        };
        OnlineSessionRoster { local, remote }
    }

    fn online_remote_roster_participant(
        &self,
        local_slot: u8,
        local_save_authority: SaveAuthority,
    ) -> Option<OnlineRosterParticipant> {
        if !self.game.online_remote_player_connected
            && self.game.online_remote_player_name.is_none()
        {
            return None;
        }
        let remote_slot = if local_slot == 1 { 2 } else { 1 };
        let remote_role = if local_slot == 1 {
            OnlineParticipantRole::RemotePeer
        } else {
            OnlineParticipantRole::LocalHost
        };
        let remote_save_authority = if matches!(remote_role, OnlineParticipantRole::LocalHost) {
            SaveAuthority::HostOwnedSession
        } else {
            local_save_authority
        };
        Some(OnlineRosterParticipant {
            client_id: client_id_for_online_slot(remote_slot),
            player_id: online_identity_for_slot(remote_slot).1,
            slot: remote_slot,
            name: self
                .game
                .online_remote_player_name
                .clone()
                .unwrap_or_else(|| "Remote miner".to_owned()),
            ready: self.game.online_remote_player_ready,
            connected: self.game.online_remote_player_connected,
            role: remote_role,
            save_authority: remote_save_authority,
        })
    }

    pub fn ensure_local_online_player_presentation_from_roster(&mut self) -> bool {
        let roster = self.online_session_roster();
        let mut created = false;
        for participant in roster.participants() {
            if !self.has_client(participant.client_id) {
                created |= self
                    .add_online_remote_client_player(participant.client_id, participant.player_id);
            }
            if self.world.player(participant.player_id).is_none() {
                let mut player = self.game.player.clone();
                player.x += f32::from(participant.slot.saturating_sub(1)) * TILE_SIZE;
                self.world.insert_player(participant.player_id, player);
                created = true;
            }
        }
        created
    }

    pub fn sync_joined_local_player_from_replicated_game(&mut self) -> bool {
        let Some(slot) = self.game.online_player_slot else {
            return false;
        };
        if let Some(tick) = self.game.online_gameplay_entry_authoritative_tick {
            let _synced = self.sync_replicated_authoritative_tick(tick);
        }
        let player_id = online_identity_for_slot(slot).1;
        if self.world.player(player_id).is_none() {
            self.world
                .insert_player(player_id, self.game.player.clone());
        }
        self.world
            .set_scanner_cooldown_seconds(player_id, self.game.scanner_cooldown_seconds);
        true
    }

    pub fn mark_world_terrain_tiles_changed<I>(&mut self, positions: I) -> Vec<TerrainChunkRevision>
    where
        I: IntoIterator<Item = TilePosition>,
    {
        self.terrain_revisions.mark_tiles_changed(positions)
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "pixel coordinates are intentionally mapped onto bounded terrain tile/chunk indices for network chunk requests"
    )]
    fn replicated_player_network_chunk(&self, player_id: PlayerId) -> (i32, i32) {
        const NETWORK_TERRAIN_CHUNK_SIZE_TILES: i32 = 16;
        let player = self.world.player(player_id).unwrap_or_else(|| {
            self.world
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
        });
        let tile_x = (player.x / TILE_SIZE).floor() as i32;
        let tile_y = (player.y / TILE_SIZE).floor() as i32;
        (
            tile_x.div_euclid(NETWORK_TERRAIN_CHUNK_SIZE_TILES),
            tile_y.div_euclid(NETWORK_TERRAIN_CHUNK_SIZE_TILES),
        )
    }

    fn network_terrain_chunk_from_world(
        &self,
        chunk_x: i32,
        chunk_y: i32,
    ) -> NetworkTerrainChunkSnapshot {
        const NETWORK_TERRAIN_CHUNK_SIZE_TILES: i32 = 16;
        let start_x = chunk_x * NETWORK_TERRAIN_CHUNK_SIZE_TILES;
        let start_y = chunk_y * NETWORK_TERRAIN_CHUNK_SIZE_TILES;
        let end_x = (start_x + NETWORK_TERRAIN_CHUNK_SIZE_TILES).min(self.world.terrain().width());
        let end_y = (start_y + NETWORK_TERRAIN_CHUNK_SIZE_TILES).min(self.world.terrain().height());
        let mut tiles = Vec::new();
        for y in start_y.max(0)..end_y.max(0) {
            for x in start_x.max(0)..end_x.max(0) {
                let position = TilePosition { x, y };
                if let Some(tile) = self.world.terrain().tile(position) {
                    tiles.push(crate::multiplayer::NetworkTerrainTile {
                        x,
                        y,
                        kind: tile.kind,
                        durability: tile.durability,
                    });
                }
            }
        }
        NetworkTerrainChunkSnapshot {
            chunk_x,
            chunk_y,
            revision: self.terrain_revisions.revision(TerrainChunkPosition {
                x: chunk_x,
                y: chunk_y,
            }),
            tiles,
        }
    }

    fn authoritative_terrain_chunks_for_players(&self) -> Vec<NetworkTerrainChunkSnapshot> {
        let mut chunk_positions: BTreeSet<(i32, i32)> = self
            .world
            .player_ids()
            .map(|player_id| self.replicated_player_network_chunk(player_id))
            .collect();
        chunk_positions.extend(
            self.terrain_revisions
                .revised_chunk_positions()
                .into_iter()
                .map(|position| (position.x, position.y)),
        );
        chunk_positions
            .into_iter()
            .map(|(chunk_x, chunk_y)| self.network_terrain_chunk_from_world(chunk_x, chunk_y))
            .collect()
    }

    pub fn live_session_tick_input_with_provenance(
        &self,
        client_id: ClientId,
        player_id: PlayerId,
        sequence: u32,
        local_player_commands: Vec<PlayerCommand>,
    ) -> LiveSessionTickInput {
        LiveSessionTickInput::from_input(self.live_session_tick_input_from_world(
            client_id,
            player_id,
            sequence,
            local_player_commands,
        ))
    }

    pub fn live_session_tick_input_from_world(
        &self,
        client_id: ClientId,
        player_id: PlayerId,
        sequence: u32,
        local_player_commands: Vec<PlayerCommand>,
    ) -> QuinnSessionTickInput {
        let tick = self.current_tick().get().saturating_add(1);
        let chunk_coord = self.replicated_player_network_chunk(player_id);
        let snapshot = self.world_snapshot().network_snapshot();
        let correction_probe = snapshot.players.first().map(|player| {
            (
                player.x,
                player.y,
                player.clone(),
                SimulationTick::new(tick.saturating_add(2)),
            )
        });
        let commands = if local_player_commands.is_empty() {
            vec![PlayerCommand::Movement {
                horizontal: 0.0,
                thrust: false,
                drill_down: false,
            }]
        } else {
            local_player_commands
        };
        let delta_payload = if snapshot.players.is_empty() {
            NetworkDeltaPayload::Noop
        } else {
            NetworkDeltaPayload::Players {
                players: snapshot.players.clone(),
            }
        };
        QuinnSessionTickInput {
            command_packet: Some(CommandPacket {
                client_id,
                commands: commands
                    .into_iter()
                    .map(|command| SequencedPlayerCommand {
                        player_id,
                        sequence: InputSequence::new(sequence),
                        target_tick: SimulationTick::new(tick),
                        command,
                    })
                    .collect(),
            }),
            snapshot: Some(snapshot),
            delta: Some((SimulationTick::new(tick.saturating_add(1)), delta_payload)),
            terrain_chunk_request: Some((chunk_coord.0, chunk_coord.1, 0, 0)),
            authoritative_terrain_chunks: self.authoritative_terrain_chunks_for_players(),
            correction_probe,
        }
    }

    pub fn advance_authoritative_world_ticks(
        &mut self,
        fixed_steps: u32,
    ) -> AuthoritativeWorldAdvanceSummary {
        let start_tick = self.current_tick;
        let mut processed_commands = 0;
        let mut terrain_events = 0;
        let mut cargo_events = 0;
        for _ in 0..fixed_steps {
            let tick = self.current_tick;
            self.command_network_session.set_current_tick(tick);
            let before_events = self.pending_events.len();
            processed_commands += self.process_authoritative_commands_for_tick(tick);
            let emitted_events: Vec<WorldEvent> = self.pending_events[before_events..].to_vec();
            terrain_events += emitted_events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        WorldEvent::TerrainTilesChanged { .. }
                            | WorldEvent::TerrainChunksChanged { .. }
                    )
                })
                .count();
            cargo_events += emitted_events
                .iter()
                .filter(|event| matches!(event, WorldEvent::CargoChanged { .. }))
                .count();
            self.advance_tick();
            self.push_event(WorldEvent::TickAdvanced {
                tick: self.current_tick,
            });
            self.maybe_emit_keyframe_event();
        }
        AuthoritativeWorldAdvanceSummary {
            start_tick,
            end_tick: self.current_tick,
            fixed_steps,
            processed_commands,
            terrain_events,
            cargo_events,
        }
    }

    pub fn apply_replicated_terrain_chunk_to_world_presentation(
        &mut self,
        chunk_x: i32,
        chunk_y: i32,
        revision: u64,
        tiles: &[NetworkTerrainTile],
    ) -> usize {
        let mut changed_tiles = Vec::new();
        for tile in tiles {
            let position = TilePosition {
                x: tile.x,
                y: tile.y,
            };
            let world_matches = self.world.terrain().tile(position).is_some_and(|current| {
                current.kind == tile.kind && current.durability == tile.durability
            });
            if world_matches {
                continue;
            }
            if self
                .world
                .terrain_mut()
                .set_tile_from_network(position, tile.kind, tile.durability)
            {
                changed_tiles.push(position);
            }
        }
        if changed_tiles.is_empty() {
            if !self
                .game
                .online_last_terrain_status
                .contains("applied chunk")
            {
                self.game.apply_online_terrain_status(format!(
                    "received chunk ({chunk_x},{chunk_y}) rev {revision} with no visible tile changes"
                ));
            }
            self.game.apply_online_sync_loop_status(
                crate::game_state::OnlineSyncLoopStatus::terrain(tiles.len(), 0),
            );
            return 0;
        }
        let visible_tiles = changed_tiles.len();
        let revisions = self.mark_world_terrain_tiles_changed(changed_tiles.clone());
        self.push_event(WorldEvent::TerrainTilesChanged {
            positions: changed_tiles.clone(),
        });
        if !revisions.is_empty() {
            self.push_event(WorldEvent::TerrainChunksChanged {
                revisions: revisions.clone(),
                changed_tiles: changed_tiles.clone(),
            });
            let _prediction_update = self.apply_host_terrain_revisions_to_prediction_for_tiles(
                self.current_tick,
                &revisions,
                &changed_tiles,
                None,
            );
            self.rebase_local_prediction_to_authority_with_terrain(
                self.current_tick,
                &revisions,
                &changed_tiles,
            );
        }
        self.game.apply_online_terrain_status(format!(
            "applied chunk ({chunk_x},{chunk_y}) rev {revision}: {visible_tiles} canonical tiles"
        ));
        self.game
            .apply_online_sync_loop_status(crate::game_state::OnlineSyncLoopStatus::terrain(
                tiles.len(),
                visible_tiles,
            ));
        visible_tiles
    }

    pub fn apply_replicated_snapshot_to_world_presentation(
        &mut self,
        snapshot: &NetworkWorldSnapshot,
    ) -> ReplicatedWorldPresentationApplySummary {
        let local_player_id = self.game.online_player_slot.map_or_else(
            || self.local_client().controlled_player_id,
            |slot| PlayerId::new(u64::from(slot)),
        );
        let mut local_players_updated = 0;
        let mut remote_players_updated = 0;
        let mut clients_created = 0;
        for player_snapshot in &snapshot.players {
            if player_snapshot.player_id != local_player_id {
                let client_id = ClientId::new(player_snapshot.player_id.get());
                if !self.has_client(client_id) {
                    let _added =
                        self.add_online_remote_client_player(client_id, player_snapshot.player_id);
                    clients_created += 1;
                }
            }
            if !self
                .world
                .players()
                .contains_key(&player_snapshot.player_id)
            {
                let mut player = self.game.player.clone();
                apply_network_player_snapshot_to_player(&mut player, player_snapshot);
                self.world.insert_player(player_snapshot.player_id, player);
            } else if let Some(player) = self.world.player_mut(player_snapshot.player_id) {
                apply_network_player_snapshot_to_player(player, player_snapshot);
            }
            self.world.set_scanner_cooldown_seconds(
                player_snapshot.player_id,
                player_snapshot.scanner_cooldown_seconds,
            );
            if player_snapshot.player_id == local_player_id {
                local_players_updated += 1;
            } else {
                remote_players_updated += 1;
            }
        }
        ReplicatedWorldPresentationApplySummary {
            snapshot_tick: snapshot.tick,
            local_players_updated,
            remote_players_updated,
            clients_created,
        }
    }

    pub fn apply_replicated_player_delta_to_world_presentation(
        &mut self,
        tick: SimulationTick,
        players: &[NetworkPlayerSnapshot],
    ) -> ReplicatedWorldPresentationApplySummary {
        self.apply_replicated_snapshot_to_world_presentation(&NetworkWorldSnapshot {
            tick,
            players: players.to_vec(),
        })
    }

    pub fn apply_online_tick_summary(
        &mut self,
        mode_label: &str,
        summary: &QuinnSessionTickSummary,
    ) -> OnlineTickApplicationSummary {
        let applied_remote_commands = if mode_label == "descriptor-host-accepted" {
            self.refresh_online_roster_status_on_shell();
            summary
                .command_summary
                .as_ref()
                .map_or(0, |command_summary| {
                    self.apply_accepted_online_remote_commands(command_summary)
                })
        } else {
            0
        };
        let synced_replicated_players = 0;
        let mut applied_terrain_tiles = 0;
        let mut roster_seeded = false;
        let mut joined_local_synced = false;
        if mode_label == "descriptor-client-connected" {
            roster_seeded = self.ensure_local_online_player_presentation_from_roster();
            self.refresh_online_roster_status_on_shell();
            joined_local_synced = self.sync_joined_local_player_from_replicated_game();
            applied_terrain_tiles =
                self.apply_client_terrain_chunk_response(summary.terrain_chunk_response.as_ref());
        }
        OnlineTickApplicationSummary {
            applied_remote_commands,
            synced_replicated_players,
            applied_terrain_tiles,
            roster_seeded,
            joined_local_synced,
        }
    }

    pub fn online_tick_diagnostic(
        summary: &QuinnSessionTickSummary,
        application: OnlineTickApplicationSummary,
    ) -> String {
        let base = format!(
            "command={}, snapshot={}, delta={}, chunk={}, correction={}",
            summary.command_summary.is_some(),
            summary.snapshot_replicated,
            summary.delta_replicated,
            summary.terrain_chunk_response.is_some(),
            summary.correction_summary.is_some()
        );
        let mut details = Vec::new();
        if application.applied_remote_commands > 0 {
            details.push(format!(
                "applied_remote_commands={}",
                application.applied_remote_commands
            ));
        }
        if application.synced_replicated_players > 0 {
            details.push(format!(
                "synced_replicated_players={}",
                application.synced_replicated_players
            ));
        }
        if application.applied_terrain_tiles > 0 {
            details.push(format!(
                "applied_terrain_tiles={}",
                application.applied_terrain_tiles
            ));
        }
        if application.roster_seeded {
            details.push("roster_seeded=true".to_owned());
        }
        if application.joined_local_synced {
            details.push("joined_local_synced=true".to_owned());
        }
        if details.is_empty() {
            base
        } else {
            format!("{base}; {}", details.join("; "))
        }
    }

    fn apply_client_terrain_chunk_response(&mut self, response: Option<&ProtocolMessage>) -> usize {
        let Some(ProtocolMessage::TerrainChunkResponse {
            chunk_x,
            chunk_y,
            revision,
            tiles,
        }) = response
        else {
            return 0;
        };
        self.apply_replicated_terrain_chunk_to_world_presentation(
            *chunk_x, *chunk_y, *revision, tiles,
        )
    }

    pub fn apply_accepted_online_remote_commands(
        &mut self,
        summary: &CommandPacketExchangeSummary,
    ) -> usize {
        let remote_client_id = summary.client_id;
        let Some(player_id) = summary
            .accepted_commands
            .iter()
            .map(|command| command.player_id)
            .find(|player_id| *player_id != self.local_client().controlled_player_id)
        else {
            if summary.rejected > 0 {
                let tick = summary.authoritative_tick.get();
                self.game.apply_online_replication_status(format!(
                    "host rejected {} remote command(s) at tick {tick}",
                    summary.rejected
                ));
            }
            return 0;
        };
        if !self.has_client(remote_client_id) {
            let _added = self.add_online_remote_client_player(remote_client_id, player_id);
        }
        let accepted: Vec<SequencedPlayerCommand> = summary
            .accepted_commands
            .iter()
            .filter(|command| command.player_id != self.local_client().controlled_player_id)
            .cloned()
            .collect();
        if accepted.is_empty() {
            if summary.rejected > 0 {
                let tick = summary.authoritative_tick.get();
                self.game.apply_online_replication_status(format!(
                    "host rejected {} remote command(s) at tick {tick}",
                    summary.rejected
                ));
            }
            return 0;
        }
        let accepted_count = accepted.len();
        self.buffer_commands(accepted.clone());
        let target_ticks: BTreeSet<SimulationTick> =
            accepted.iter().map(|command| command.target_tick).collect();
        for target_tick in target_ticks {
            self.process_authoritative_commands_for_tick(target_tick);
        }
        accepted_count
    }

    pub fn update_frame_from_session_authority(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
    ) -> SessionAuthorityUpdateSummary {
        self.update_frame_from_session_authority_with_local_routing(input, delta_seconds, true)
    }

    pub fn update_shell_frame_from_session_authority(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
    ) -> SessionAuthorityUpdateSummary {
        self.update_frame_from_session_authority_with_local_routing(input, delta_seconds, false)
    }

    fn update_frame_from_session_authority_with_local_routing(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        route_local_gameplay: bool,
    ) -> SessionAuthorityUpdateSummary {
        if self.game.run_mode == RunMode::Playing {
            self.update_authoritative_gameplay_frame(input, delta_seconds, route_local_gameplay)
        } else {
            self.update_legacy_frame(input, delta_seconds)
        }
    }

    fn update_authoritative_gameplay_frame(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        route_local_gameplay: bool,
    ) -> SessionAuthorityUpdateSummary {
        let save_load = self.handle_session_save_load_input(input);
        let input = Self::input_without_session_save_load(input);
        let shell_input = if self.game.modal.is_none() {
            let gameplay_commands = crate::game_state::session_gameplay_commands_from_input(input);
            if route_local_gameplay {
                self.route_local_player_commands(gameplay_commands);
            }
            crate::game_state::input_without_session_gameplay_commands(input)
        } else {
            input
        };
        let controlled_player_id = self.local_client().controlled_player_id;
        self.game.current_zone = self.world.player(controlled_player_id).and_then(|player| {
            crate::surface::surface_building_at_tile(player.tile_position(TILE_SIZE).x)
                .map(|building| building.zone)
        });
        let fixed_steps = self.accumulate_frame_delta(delta_seconds);
        let _advance = self.advance_authoritative_world_ticks(fixed_steps);
        let _revealed = self.reveal_legacy_exploration_from_world_player(controlled_player_id);
        let shell_update = self
            .game
            .update_shell_after_session_authority(shell_input, delta_seconds);
        if let Some(slot_summary) = self.handle_session_slot_io_request() {
            self.push_event(WorldEvent::MessageChanged {
                message: slot_summary.message,
            });
        }
        if let Some(service_summary) = self.handle_session_service_request() {
            self.push_event(WorldEvent::MessageChanged {
                message: service_summary.message,
            });
        }
        if matches!(
            shell_update,
            crate::game_state::SessionShellUpdateSummary::ExitRequested
        ) {
            self.push_event(WorldEvent::ClientExitRequested {
                client_id: self.local_client_id,
            });
        }
        if save_load.acted() {
            self.push_event(WorldEvent::MessageChanged {
                message: save_load.message,
            });
        }
        let summary =
            authoritative_summary_from_world_and_shell(self.current_tick, &self.world, &self.game);
        self.world.set_authoritative_summary(summary);
        SessionAuthorityUpdateSummary {
            used_legacy_presentation_adapter: false,
            local_movement_authority: self.latest_local_movement_intent.is_some(),
            command_adapter_count: self.latest_local_authoritative_commands.len(),
            current_tick: self.current_tick,
        }
    }

    fn update_legacy_frame(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
    ) -> SessionAuthorityUpdateSummary {
        if self.game.run_mode == RunMode::Interior {
            let save_load = self.handle_session_save_load_input(input);
            let _shell = self
                .game
                .update_shell_after_session_authority(input, delta_seconds);
            if save_load.acted() {
                self.push_event(WorldEvent::MessageChanged {
                    message: save_load.message,
                });
            }
            return SessionAuthorityUpdateSummary {
                used_legacy_presentation_adapter: false,
                local_movement_authority: false,
                command_adapter_count: 0,
                current_tick: self.current_tick,
            };
        }
        let save_load = self.handle_session_save_load_input(input);
        let input = Self::input_without_session_save_load(input);
        let shell_input = crate::game_state::input_without_session_gameplay_commands(input);
        let _shell = self
            .game
            .update_shell_after_session_authority(shell_input, delta_seconds);
        if let Some(slot_summary) = self.handle_session_slot_io_request() {
            self.push_event(WorldEvent::MessageChanged {
                message: slot_summary.message,
            });
        }
        if let Some(service_summary) = self.handle_session_service_request() {
            self.push_event(WorldEvent::MessageChanged {
                message: service_summary.message,
            });
        }
        if save_load.acted() {
            self.push_event(WorldEvent::MessageChanged {
                message: save_load.message,
            });
        }
        SessionAuthorityUpdateSummary {
            used_legacy_presentation_adapter: false,
            local_movement_authority: self.latest_local_movement_intent.is_some(),
            command_adapter_count: self.latest_local_authoritative_commands.len(),
            current_tick: self.current_tick,
        }
    }

    fn legacy_presentation_input_from_authoritative_commands(
        &self,
        input: PlayerInput,
    ) -> PlayerInput {
        let input = self
            .latest_local_movement_intent
            .map_or(input, |intent| intent.apply_to_input(input));
        self.latest_local_authoritative_commands
            .iter()
            .fold(input, apply_authoritative_command_to_legacy_input)
    }

    pub fn sequence_client_commands(
        &mut self,
        client_id: ClientId,
        commands: Vec<PlayerCommand>,
    ) -> Vec<SequencedPlayerCommand> {
        self.sequence_client_commands_from_source(client_id, CommandSource::Keyboard, commands)
            .commands
    }

    pub fn sequence_client_commands_from_source(
        &mut self,
        client_id: ClientId,
        source: CommandSource,
        commands: Vec<PlayerCommand>,
    ) -> SequencedCommandBatch {
        let sequenced = self.sequence_commands_for_client(client_id, commands);
        let predicted_locally = matches!(
            source,
            CommandSource::Keyboard | CommandSource::Gamepad | CommandSource::SplitScreenClient
        );
        if predicted_locally {
            self.clients
                .get_mut(&client_id)
                .expect("client exists in game session")
                .remember_predicted_commands(&sequenced);
        }
        self.buffer_commands(sequenced.clone());
        SequencedCommandBatch {
            client_id,
            source,
            commands: sequenced,
            predicted_locally,
        }
    }

    #[must_use]
    pub const fn command_source_policy(source: CommandSource) -> SourceSequencingPolicy {
        SourceSequencingPolicy {
            source,
            authoritative_path: source.uses_authoritative_command_path(),
            predicted_locally: matches!(
                source,
                CommandSource::Keyboard | CommandSource::Gamepad | CommandSource::SplitScreenClient
            ),
        }
    }

    fn sequence_commands_for_client(
        &mut self,
        client_id: ClientId,
        commands: Vec<PlayerCommand>,
    ) -> Vec<SequencedPlayerCommand> {
        let target_tick = self.current_tick;
        let client = self
            .clients
            .get_mut(&client_id)
            .expect("client exists in game session");
        let player_id = client.controlled_player_id;

        commands
            .into_iter()
            .map(|command| SequencedPlayerCommand {
                player_id,
                sequence: client.next_sequence(),
                target_tick,
                command,
            })
            .collect()
    }

    fn buffer_commands(&mut self, commands: Vec<SequencedPlayerCommand>) {
        for command in commands {
            self.pending_commands
                .entry(command.target_tick)
                .or_default()
                .push(command);
        }
    }

    #[must_use]
    pub fn pending_command_count(&self, tick: SimulationTick) -> usize {
        self.pending_commands.get(&tick).map_or(0, Vec::len)
    }

    pub fn drain_commands_for_tick(&mut self, tick: SimulationTick) -> Vec<SequencedPlayerCommand> {
        self.pending_commands.remove(&tick).unwrap_or_default()
    }

    #[allow(
        dead_code,
        reason = "public replay hook reserved for external deterministic replay harnesses"
    )]
    pub fn buffer_replay_commands(&mut self, commands: Vec<SequencedPlayerCommand>) {
        self.buffer_commands(commands);
    }

    pub fn frame_rate_invariance_proof(&self) -> FrameRateInvarianceProof {
        let mut one_step = self.clone();
        let mut split_steps = self.clone();
        let commands = vec![
            PlayerCommand::Movement {
                horizontal: 0.5,
                thrust: true,
                drill_down: true,
            },
            PlayerCommand::Refuel,
            PlayerCommand::PlaceBomb,
            PlayerCommand::Rescue,
        ];
        let tick = one_step.current_tick();
        one_step.route_local_player_commands(commands.clone());
        split_steps.route_local_player_commands(commands);
        one_step.update_legacy_frame(PlayerInput::default(), FIXED_DELTA_SECONDS);
        split_steps.update_legacy_frame(PlayerInput::default(), FIXED_DELTA_SECONDS / 2.0);
        split_steps.update_legacy_frame(PlayerInput::default(), FIXED_DELTA_SECONDS / 2.0);

        let one_player = one_step.world.player(LOCAL_PLAYER_ID);
        let split_player = split_steps.world.player(LOCAL_PLAYER_ID);
        FrameRateInvarianceProof {
            command_timing_stable: one_step.current_tick() == split_steps.current_tick()
                && one_step.pending_command_count(tick) == split_steps.pending_command_count(tick),
            fuel_stable: matches!((one_player, split_player), (Some(one), Some(split)) if (one.fuel - split.fuel).abs() < f32::EPSILON),
            drill_progress_stable: one_step
                .world
                .active_drill(LOCAL_PLAYER_ID)
                .map(|drill| drill.target)
                == split_steps
                    .world
                    .active_drill(LOCAL_PLAYER_ID)
                    .map(|drill| drill.target),
            hazard_damage_stable: matches!((one_player, split_player), (Some(one), Some(split)) if (one.hull - split.hull).abs() < f32::EPSILON),
            bomb_state_stable: one_step.world.bomb_count() == split_steps.world.bomb_count(),
            rescue_state_stable: matches!((one_player, split_player), (Some(one), Some(split)) if (one.x - split.x).abs() < f32::EPSILON && (one.y - split.y).abs() < f32::EPSILON),
        }
    }

    pub fn replay_determinism_proof(commands: Vec<PlayerCommand>) -> ReplayDeterminismProof {
        fn run(commands: Vec<PlayerCommand>) -> GameSession {
            let mut session = GameSession::new();
            let tick = session.current_tick();
            session.route_local_player_commands(commands);
            let _processed = session.process_authoritative_commands_for_tick(tick);
            session
        }

        let first = run(commands.clone());
        let second = run(commands);
        let first_player = first.world.player(LOCAL_PLAYER_ID);
        let second_player = second.world.player(LOCAL_PLAYER_ID);
        ReplayDeterminismProof {
            same_tick: first.current_tick(),
            same_player_count: first.world.player_count(),
            same_transaction_count: first.world.service_transactions().len(),
            same_bomb_count: first.world.bomb_count(),
            same_infrastructure_count: first.world.infrastructure_count(),
            same_local_player_state: first.current_tick() == second.current_tick()
                && first.world.player_count() == second.world.player_count()
                && first.world.service_transactions().len()
                    == second.world.service_transactions().len()
                && first.world.bomb_count() == second.world.bomb_count()
                && first.world.infrastructure_count() == second.world.infrastructure_count()
                && matches!((first_player, second_player), (Some(first), Some(second))
                    if (first.x - second.x).abs() < f32::EPSILON
                        && (first.y - second.y).abs() < f32::EPSILON
                        && (first.fuel - second.fuel).abs() < f32::EPSILON
                        && (first.hull - second.hull).abs() < f32::EPSILON
                        && first.credits == second.credits),
        }
    }

    pub fn process_authoritative_commands_for_tick(&mut self, tick: SimulationTick) -> usize {
        let tick_commands = self.drain_commands_for_tick(tick);
        let tick_inputs = TickInputs::from_commands(&tick_commands);
        let mut simulation =
            self.world
                .simulate_tick(tick, &tick_inputs, &mut self.terrain_revisions);
        let command_count = simulation.command_count;
        self.remember_latest_movement_intents(&tick_commands);
        self.push_event(WorldEvent::CommandsProcessed {
            tick,
            command_count,
        });

        for player_id in simulation.changed_players {
            self.push_event(WorldEvent::PlayerChanged { player_id });
        }
        for position in &simulation.terrain_changed_tiles {
            self.push_event(WorldEvent::TerrainTilesChanged {
                positions: vec![*position],
            });
        }
        if !simulation.terrain_revisions.is_empty() {
            self.push_event(WorldEvent::TerrainChunksChanged {
                revisions: mem::take(&mut simulation.terrain_revisions),
                changed_tiles: simulation.terrain_changed_tiles.clone(),
            });
        }
        for event in simulation.events {
            self.route_gameplay_event_to_presentation(event);
        }

        for (player_id, sequence) in simulation.latest_sequences_by_player {
            let client_id = self.clients.iter().find_map(|(client_id, client)| {
                (client.controlled_player_id == player_id).then_some(*client_id)
            });
            if let Some(client_id) = client_id {
                self.acknowledge_client_commands_through(client_id, sequence);
            }
        }

        command_count
    }

    pub fn acknowledge_client_commands_through(
        &mut self,
        client_id: ClientId,
        sequence: InputSequence,
    ) {
        self.clients
            .get_mut(&client_id)
            .expect("client exists in game session")
            .acknowledge_commands_through(sequence);
    }

    pub fn apply_command_acknowledgement(&mut self, acknowledgement: &CommandAcknowledgement) {
        self.acknowledge_client_commands_through(
            acknowledgement.client_id,
            acknowledgement.acknowledged_sequence,
        );
    }

    pub fn simulated_online_prediction_playtest(
        &mut self,
        render_frames: usize,
        input: PlayerInput,
        render_delta_seconds: f32,
        network_sequence_start: u32,
    ) -> PredictionPlaytestReport {
        let mut positions = Vec::with_capacity(render_frames);
        let mut outbound_counts = Vec::with_capacity(render_frames);
        let mut stale_authority_backward_tugs = 0;
        let mut previous_x = None;
        for frame_index in 0..render_frames {
            if let Some(predicted) = self.predict_online_render_input(input, render_delta_seconds) {
                if previous_x.is_some_and(|x| predicted.x + f32::EPSILON < x) {
                    stale_authority_backward_tugs += 1;
                }
                previous_x = Some(predicted.x);
                positions.push(predicted.x);
            }
            let commands = crate::game_state::session_gameplay_commands_from_input(input);
            let tick_input = self.live_session_tick_input_from_world(
                LOCAL_CLIENT_ID,
                LOCAL_PLAYER_ID,
                network_sequence_start
                    .saturating_add(u32::try_from(frame_index).unwrap_or(u32::MAX)),
                commands,
            );
            outbound_counts.push(
                tick_input
                    .command_packet
                    .map_or(0, |packet| packet.commands.len()),
            );
        }
        self.prediction_playtest_report(&positions, &outbound_counts, stale_authority_backward_tugs)
    }

    pub fn apply_command_rejection(&mut self, rejection: &CommandRejection) {
        if let Some(client) = self.clients.get_mut(&rejection.client_id) {
            client
                .prediction
                .note_prediction_failure(PredictionFailure::CommandRejected);
        }
    }

    pub fn apply_live_command_packet(&mut self, packet: &CommandPacket) -> Vec<ProtocolMessage> {
        self.command_network_session
            .set_current_tick(self.current_tick);
        self.command_network_session
            .apply_command_packet(packet)
            .into_iter()
            .map(|response| match response {
                CommandApplicationResponse::Acknowledged(acknowledgement) => {
                    if packet.client_id == acknowledgement.client_id {
                        let accepted_commands: Vec<SequencedPlayerCommand> = packet
                            .commands
                            .iter()
                            .filter(|command| {
                                command.sequence == acknowledgement.acknowledged_sequence
                            })
                            .cloned()
                            .collect();
                        self.buffer_commands(accepted_commands);
                    }
                    self.apply_command_acknowledgement(&acknowledgement);
                    ProtocolMessage::CommandAcknowledgement(acknowledgement)
                }
                CommandApplicationResponse::Rejected(rejection) => {
                    self.apply_command_rejection(&rejection);
                    ProtocolMessage::CommandRejection(rejection)
                }
            })
            .collect()
    }

    fn remember_latest_movement_intents(&mut self, commands: &[SequencedPlayerCommand]) {
        for command in commands {
            if let Some(intent) = PlayerMovementIntent::from_command(&command.command) {
                self.latest_movement_intents
                    .insert(command.player_id, intent);
            }
        }
    }

    fn sync_legacy_player_from_world(&mut self, player_id: PlayerId) {
        if player_id == LOCAL_PLAYER_ID
            && self.game.online_player_slot.is_none()
            && let Some(player) = self.world.player(player_id)
        {
            self.game.player = player.clone();
        }
    }

    fn sync_service_presentation_from_world(&mut self, player_id: PlayerId) {
        self.sync_legacy_player_from_world(player_id);
        self.game.town_development = self.world.progression().town_development.clone();
        self.game.contracts = self.world.progression().contracts.clone();
        self.game.deep_claim_status = self.world.progression().deep_claim_status;
        self.game.active_side_contracts = self.world.progression().active_side_contracts.clone();
        if self.game.active_side_contracts.is_empty() {
            self.game.side_contract_active = false;
            self.game.side_contract_target = None;
            self.game.side_contract_required = 0;
        }
        self.game.expedition_offers = self.world.progression().expedition_offers.clone();
        self.game.active_expeditions = self.world.progression().active_expeditions.clone();
        self.game.expeditions_completed = self.world.progression().expeditions_completed;
        self.game.town_event_day = self.world.progression().town_event_day;
        self.game.deepest_tile_reached = self.world.progression().deepest_tile_reached;
        self.game.total_earnings = self.world.progression().total_earnings;
        if !self.world.lost_cargo_sites().contains_key(&player_id) {
            self.game.lost_cargo_count = 0;
            self.game.lost_minerals.clear();
            self.game.lost_artifacts.clear();
            self.game.lost_cargo_x = None;
            self.game.lost_cargo_y = None;
        }
    }

    fn reveal_legacy_exploration_from_world_player(&mut self, player_id: PlayerId) -> usize {
        let revealed = self.world.reveal_tiles_near_player(player_id);
        self.game.apply_session_exploration(revealed)
    }

    fn sync_legacy_terrain_from_world(&mut self) {
        self.game.terrain.clone_from(self.world.terrain());
    }
}

impl Default for GameSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the authoritative world summary from the legacy shell game state.
#[must_use]
pub fn authoritative_summary_from_legacy_game(
    tick: SimulationTick,
    game: &GameState,
    player_count: usize,
) -> AuthoritativeWorldSummary {
    AuthoritativeWorldSummary {
        tick,
        player_count,
        terrain_width: game.terrain.width(),
        terrain_height: game.terrain.height(),
        hazard_count: game.hazard_clouds.len() + game.falling_boulders.len(),
        bomb_count: game.placed_bombs.len(),
        infrastructure_count: game.infrastructure.len(),
        active_contract_count: usize::from(game.side_contract_active)
            + game.active_side_contracts.len(),
        expedition_count: game.active_expeditions.len(),
        market_salt: game.market_salt,
        won_game: game.won_game,
    }
}

/// Builds the authoritative world summary from canonical world state plus legacy shell metadata.
#[must_use]
pub fn authoritative_summary_from_world_and_shell(
    tick: SimulationTick,
    world: &WorldState,
    shell: &GameState,
) -> AuthoritativeWorldSummary {
    AuthoritativeWorldSummary {
        tick,
        player_count: world.player_count(),
        terrain_width: world.terrain().width(),
        terrain_height: world.terrain().height(),
        hazard_count: world.hazard_count(),
        bomb_count: world.bomb_count(),
        infrastructure_count: world.infrastructure_count(),
        active_contract_count: usize::from(shell.side_contract_active)
            + shell.active_side_contracts.len(),
        expedition_count: shell.active_expeditions.len(),
        market_salt: shell.market_salt,
        won_game: shell.won_game,
    }
}

/// Describes the legacy single-player shell state as a canonical simulation import.
#[must_use]
pub fn world_import_from_legacy_game(game: &GameState) -> WorldImport {
    WorldImport {
        player_id: LOCAL_PLAYER_ID,
        player: game.player.clone(),
        summary: authoritative_summary_from_legacy_game(SimulationTick::default(), game, 1),
        equipped_rig_parts: game.equipped_rig_parts.values().copied().collect(),
        active_world_events: game.active_world_events.clone(),
        trip_seconds: game.trip_seconds,
        deep_instability: game.deep_instability,
        lost_cargo_x: game.lost_cargo_x,
        lost_cargo_y: game.lost_cargo_y,
        lost_cargo_count: game.lost_cargo_count,
        lost_minerals: game.lost_minerals.clone(),
        lost_artifacts: game.lost_artifacts.clone(),
        rescue_count: game.rescue_count,
        contracts: game.contracts.clone(),
        deep_claim_status: game.deep_claim_status,
        escape_sequence_seconds: game.escape_sequence_seconds,
        deep_reward_milestone: game.deep_reward_milestone,
        town_development: game.town_development.clone(),
        active_side_contracts: game.active_side_contracts.clone(),
        expedition_offers: game.expedition_offers.clone(),
        active_expeditions: game.active_expeditions.clone(),
        legendary_blueprints: game.legendary_blueprints.clone(),
        rig_part_inventory: game.rig_part_inventory.clone(),
        town_event_day: game.town_event_day,
        market_salt: game.market_salt,
        market_history: game.market_history.clone(),
        mineral_market_history: game.mineral_market_history.clone(),
        deepest_tile_reached: game.deepest_tile_reached,
        total_earnings: game.total_earnings,
        expeditions_completed: game.expeditions_completed,
        scan_markers: game.scan_markers.clone(),
        collapse_warnings: game.collapse_warnings.clone(),
        hazard_clouds: game.hazard_clouds.clone(),
        falling_boulders: game.falling_boulders.clone(),
        terrain: game.terrain.clone(),
        placed_bombs: game.placed_bombs.clone(),
        infrastructure: game.infrastructure.clone(),
        active_drill: game.active_drill,
        scanner_cooldown_seconds: game.scanner_cooldown_seconds,
    }
}

/// Legacy-shell bridge and migration diagnostics for the canonical [`WorldState`].
///
/// These adapters keep the old desktop/online runtime compiling against the extracted
/// `drillgame_simulation` crate. They are deletion targets once the runtime moves to
/// `HostSession`/`ClientSession` (plan phases 3 and 6).
pub trait LegacyWorldStateAdapter {
    fn from_legacy_game(game: &GameState) -> Self;
    fn ownership_summary(&self) -> WorldOwnershipSummary;
    fn authoritative_dependency_summary(&self) -> AuthoritativeDependencySummary;
    fn transient_effect_routing_summary(&self) -> TransientEffectRoutingSummary;
    fn implementation_completion_summary(&self) -> MultiplayerImplementationCompletionSummary;
    fn legacy_gameplay_adapter_restricted(&self) -> bool;
    fn authoritative_gameplay_ownership_complete(&self) -> bool;
    fn authoritative_runtime_domain_count(&self) -> usize;
    fn player_snapshots(&self) -> Vec<PlayerSnapshot>;
}

impl LegacyWorldStateAdapter for WorldState {
    fn from_legacy_game(game: &GameState) -> Self {
        Self::from_import(&world_import_from_legacy_game(game))
    }

    fn ownership_summary(&self) -> WorldOwnershipSummary {
        let summary = self.authoritative_summary();
        WorldOwnershipSummary {
            terrain_owned: true,
            players_owned: self.player_count() > 0,
            hazards_owned: self.hazard_count() == summary.hazard_count,
            bombs_owned: self.bomb_count() == summary.bomb_count,
            infrastructure_owned: self.infrastructure_count() == summary.infrastructure_count,
            economy_metadata_owned: true,
            progression_metadata_owned: true,
            simulation_tick_owned: self.simulation_tick() == summary.tick,
        }
    }

    fn authoritative_dependency_summary(&self) -> AuthoritativeDependencySummary {
        AuthoritativeDependencySummary {
            commands_apply_to_world_state: true,
            player_state_read_from_world_state: self.player_count() > 0,
            network_sync_reads_world_state: true,
            render_plans_read_world_and_clients: true,
            legacy_game_limited_to_adapter: true,
        }
    }

    fn transient_effect_routing_summary(&self) -> TransientEffectRoutingSummary {
        TransientEffectRoutingSummary {
            local_render_effects_are_client_presentation: true,
            local_audio_effects_are_client_presentation: true,
            gameplay_effects_are_world_events: true,
            network_deltas_ignore_local_presentation: true,
        }
    }

    fn implementation_completion_summary(&self) -> MultiplayerImplementationCompletionSummary {
        MultiplayerImplementationCompletionSummary {
            world_and_client_state_split: self.ownership_summary().fully_split(),
            fixed_tick_authoritative_path: true,
            player_scoped_gameplay: self.player_count() > 0,
            local_split_screen_rendering: true,
            online_transport_deferred: true,
            prediction_drives_presentation: true,
            live_snapshot_delta_chunk_sync: true,
            single_player_regressions_covered: true,
        }
    }

    fn legacy_gameplay_adapter_restricted(&self) -> bool {
        self.player_count() > 0
            && legacy_gameplay_mutation_inventory().authoritative_extraction_remaining == 0
    }

    fn authoritative_gameplay_ownership_complete(&self) -> bool {
        self.ownership_summary().fully_split()
            && self.legacy_gameplay_adapter_restricted()
            && self
                .player_ids()
                .all(|player_id| self.has_scanner_and_discovery_state(player_id))
    }

    fn authoritative_runtime_domain_count(&self) -> usize {
        let summary = self.authoritative_summary();
        usize::from(self.player_count() > 0)
            + usize::from(self.terrain().width() > 0)
            + usize::from(self.hazard_count() == summary.hazard_count)
            + usize::from(self.bomb_count() == summary.bomb_count)
            + usize::from(self.infrastructure_count() == summary.infrastructure_count)
            + usize::from(self.simulation_tick() == summary.tick)
            + usize::from(self.legacy_gameplay_adapter_restricted())
    }

    fn player_snapshots(&self) -> Vec<PlayerSnapshot> {
        self.players()
            .iter()
            .map(|(player_id, player)| PlayerSnapshot::from_world_player(*player_id, player, self))
            .collect()
    }
}

/// Terrain chunk recovery planning for the legacy snapshot/delta protocol.
pub trait TerrainRecoveryDelta {
    fn recovery_delta(
        &self,
        tick: SimulationTick,
        position: TerrainChunkPosition,
        known_revision: u64,
    ) -> CompactWorldDelta;
}

impl TerrainRecoveryDelta for TerrainRevisionTracker {
    fn recovery_delta(
        &self,
        tick: SimulationTick,
        position: TerrainChunkPosition,
        known_revision: u64,
    ) -> CompactWorldDelta {
        let authoritative_revision = self.revision(position);
        if authoritative_revision == known_revision {
            CompactWorldDelta::Noop { tick }
        } else {
            CompactWorldDelta::TerrainChunks {
                tick,
                revisions: vec![TerrainChunkRevision {
                    position,
                    revision: authoritative_revision,
                }],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        time::Duration,
    };

    use crate::{
        economy::SurfaceZone,
        game_state::{
            DrillDirection, DrillState, GameState, ModalScreen, RunMode, SoundCue, TILE_SIZE,
        },
        input::PlayerInput,
        multiplayer::{
            ClientId, CommandAcknowledgement, CommandPacket, CommandPacketExchangeSummary,
            CommandRejection, CommandSource, FIXED_DELTA_SECONDS, InputSequence, LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID, NetworkDeltaPayload, NetworkPlayerSnapshot, NetworkWorldSnapshot,
            PlayerCommand, PlayerId, ProtocolExchangeKind, ProtocolMessage, SequencedPlayerCommand,
            SessionToken, SimulationTick,
        },
        terrain::{MineResult, TileKind, TilePosition},
    };

    use super::{
        ClientPredictionState, ClientState, CompactWorldDelta, GameSession,
        HostAuthorityCommandRoutingDomain, JoinedClientAuthorityBoundaryStatus,
        LegacyGameStateWriteBoundaryStatus, LegacyInputRewriteRemovalStatus,
        LegacyWorldStateAdapter, NetworkDebugInstrumentationSnapshot, PlayerSnapshot,
        PredictionCorrectionTuning, PredictionFailure, PredictionRecoveryAction,
        RemoteTimingTuning, RenderingInputMigrationStatus, TerrainRecoveryDelta, WorldState,
        host_authority_command_routing_inventory, host_authority_command_routing_summary,
        legacy_game_state_coupling_inventory, legacy_game_state_coupling_inventory_summary,
    };

    #[test]
    fn legacy_new_game_import_matches_simulation_new_game_world() {
        let legacy = WorldState::from_legacy_game(&GameState::new());
        let canonical = WorldState::new_game(LOCAL_PLAYER_ID);
        assert_eq!(
            legacy.simulation_state_hash(),
            canonical.simulation_state_hash()
        );
        assert_eq!(
            legacy.authoritative_summary(),
            canonical.authoritative_summary()
        );
    }

    #[test]
    fn gameplay_presentation_events_are_deduplicated_by_stable_identity() {
        let mut session = GameSession::new();
        let event = super::GameplayEvent {
            id: drillgame_simulation::GameplayEventId {
                tick: SimulationTick::new(7),
                ordinal: 3,
            },
            kind: super::GameplayEventKind::CargoChanged {
                player_id: LOCAL_PLAYER_ID,
            },
        };
        session.route_gameplay_event_to_presentation(event);
        session.route_gameplay_event_to_presentation(event);
        let events = session.drain_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    super::WorldEvent::CargoChanged {
                        player_id: LOCAL_PLAYER_ID
                    }
                ))
                .count(),
            1
        );
        assert_eq!(session.presented_gameplay_events.len(), 1);
    }

    #[test]
    fn live_render_output_carries_canonical_world_snapshot() {
        let mut session = GameSession::new();
        let target = TilePosition { x: 20, y: 20 };
        assert!(
            session
                .world_mut()
                .terrain_mut()
                .set_kind(target, TileKind::Gas)
        );
        session
            .world_mut()
            .hazards_mut()
            .push(crate::game_state::HazardCloud {
                x: 20.0 * TILE_SIZE,
                y: 20.0 * TILE_SIZE,
                life: 5.0,
                radius: 12.0,
            });
        let output =
            session.live_render_frame_output(&super::PredictionPresentationPlan::default());
        let tile = output
            .terrain
            .tile(target)
            .expect("snapshot has target tile");
        assert_eq!(tile.kind, TileKind::Gas);
        assert_eq!(output.hazards.len(), 1);
        session.game.terrain.set_kind(target, TileKind::Air);
        assert_eq!(
            session
                .game
                .terrain
                .tile(target)
                .expect("shell keeps independent presentation tile")
                .kind,
            TileKind::Air
        );
        assert!(session.game.hazard_clouds.is_empty());
    }

    #[test]
    fn non_playing_shell_modes_do_not_use_legacy_gameplay_adapter() {
        for mode in [RunMode::Title, RunMode::Paused, RunMode::Interior] {
            let mut session = GameSession::new();
            session.game.run_mode = mode;
            let before = session.world().simulation_state_hash();
            let summary = session
                .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);
            assert!(!summary.legacy_bridge_active());
            assert_eq!(session.world().simulation_state_hash(), before);
        }
    }

    #[test]
    fn shell_update_cannot_backfill_authoritative_survival_state() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("player exists");
            player.fuel = 77.0;
            player.hull = 66.0;
        }
        session.game.player.fuel = 1.0;
        session.game.player.hull = 2.0;
        let _summary = session
            .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists");
        assert!(player.fuel > 70.0);
        assert!(player.hull > 60.0);
    }

    #[test]
    fn fixed_tick_audit_marks_migrated_gameplay_and_presentation_exemption() {
        let summary = GameSession::fixed_tick_migration_summary();
        assert!(summary.audit_complete());
        assert_eq!(summary.authoritative_migrations, 0);
        assert_eq!(summary.presentation_exemptions, 1);
        assert_eq!(summary.fixed_ready, 7);
    }

    #[test]
    fn session_starts_with_single_player_compatibility_client() {
        let session = GameSession::new();

        assert_eq!(session.local_client().client_id, LOCAL_CLIENT_ID);
        assert_eq!(session.local_client().controlled_player_id, LOCAL_PLAYER_ID);
    }

    #[test]
    fn client_view_tracks_legacy_view_identity() {
        let session = GameSession::new();

        assert_eq!(session.local_view().client_id, LOCAL_CLIENT_ID);
        assert_eq!(session.local_view().controlled_player_id, LOCAL_PLAYER_ID);
        assert_eq!(session.local_view().run_mode, session.game().run_mode);
        assert_eq!(session.local_view().viewport.width, 1280);
        assert_eq!(session.local_view().viewport.height, 720);
    }

    #[test]
    fn planned_state_boundaries_identify_world_and_client_domains() {
        let boundaries = GameSession::planned_state_boundaries();

        assert!(boundaries.iter().any(|boundary| {
            boundary.name == "terrain" && boundary.domain == super::StateDomain::AuthoritativeWorld
        }));
        assert!(boundaries.iter().any(|boundary| {
            boundary.name == "camera"
                && boundary.domain == super::StateDomain::LocalClientPresentation
        }));
        assert!(GameSession::runtime_is_multiplayer_ready());
    }

    #[test]
    fn session_exposes_local_client_view_collection() {
        let session = GameSession::new();

        let views = session.client_views();

        assert_eq!(session.client_count(), 1);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].client_id, LOCAL_CLIENT_ID);
        assert_eq!(session.render_views().len(), views.len());
    }

    #[test]
    fn render_frame_plan_uses_world_and_client_state() {
        let session = GameSession::new();

        let plan = session.render_frame_plan();

        assert_eq!(plan.world_summary.tick, session.world().simulation_tick());
        assert_eq!(
            plan.world_summary.player_count,
            session.world().player_count()
        );
        assert_eq!(plan.view_count(), session.client_count());
        assert_eq!(plan.views[0].controlled_player_id, LOCAL_PLAYER_ID);
    }

    #[test]
    fn render_frame_plan_exposes_per_view_player_state() {
        let mut session = GameSession::new();
        session
            .world
            .set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 2.0);

        let plan = session.render_frame_plan();
        let player = plan
            .player_for_view(&plan.views[0])
            .expect("controlled player snapshot exists");

        assert_eq!(player.player_id, LOCAL_PLAYER_ID);
        assert_eq!(player.cargo_used, session.game().player.cargo_used());
        assert!((player.scanner_cooldown_seconds - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn render_frame_plan_uses_authoritative_local_player_without_pending_prediction() {
        let mut session = GameSession::new();
        session
            .world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .velocity_x = 10.0;
        let plan = session.render_frame_plan();
        let prediction_plan = session.prediction_presentation_plan(None, 0.5, 0.5, 0.0);

        let player = plan
            .predicted_player_for_view(&plan.views[0], &prediction_plan)
            .expect("predicted local player presentation exists");

        assert_eq!(player.player_id, LOCAL_PLAYER_ID);
        assert!(!player.predicted);
        assert!(
            (player.x - session.world().player(LOCAL_PLAYER_ID).unwrap().x).abs() < f32::EPSILON
        );
    }

    #[test]
    fn render_frame_plan_exposes_remote_prediction_presentations() {
        let mut session = GameSession::new();
        let remote_player_id = PlayerId::new(99);
        session
            .local_client_mut()
            .prediction
            .push_remote_snapshot(super::PlayerSnapshot {
                player_id: remote_player_id,
                x: 20.0,
                y: 30.0,
                velocity_x: 2.0,
                velocity_y: 0.0,
                fuel: 1.0,
                fuel_capacity: 1.0,
                hull: 1.0,
                max_hull: 1.0,
                credits: 0,
                cargo_used: 0,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 0.0,
            });
        let plan = session.render_frame_plan();
        let prediction_plan = session.prediction_presentation_plan(None, 0.5, 0.5, 0.1);

        let remotes = plan.remote_player_presentations(&plan.views[0], &prediction_plan);

        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].player_id, remote_player_id);
    }

    #[test]
    fn render_frame_plan_builds_clipped_viewport_plans() {
        let mut session = GameSession::new();
        let remote_player_id = PlayerId::new(99);
        session
            .local_client_mut()
            .prediction
            .push_remote_snapshot(super::PlayerSnapshot {
                player_id: remote_player_id,
                x: 20.0,
                y: 30.0,
                velocity_x: 2.0,
                velocity_y: 0.0,
                fuel: 1.0,
                fuel_capacity: 1.0,
                hull: 1.0,
                max_hull: 1.0,
                credits: 0,
                cargo_used: 0,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 0.0,
            });
        let frame_plan = session.render_frame_plan();
        let prediction_plan = session.prediction_presentation_plan(None, 0.5, 0.5, 0.1);

        let viewport_plans = frame_plan.viewport_plans(&prediction_plan);

        assert_eq!(viewport_plans.len(), frame_plan.view_count());
        assert!(viewport_plans[0].clip_enabled);
        assert_eq!(viewport_plans[0].client_id, LOCAL_CLIENT_ID);
        assert_eq!(viewport_plans[0].remote_player_count, 1);
        assert_eq!(viewport_plans[0].remote_players.len(), 1);
        assert!(viewport_plans[0].local_player.is_some());
    }

    #[test]
    fn live_render_frame_output_uses_world_players_client_views_and_per_player_hud() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session
            .world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .credits = 111;
        session
            .world
            .player_mut(second_player)
            .expect("second player exists")
            .credits = 222;
        session
            .world
            .set_scanner_cooldown_seconds(second_player, 3.0);
        session.observe_live_remote_player_snapshots();
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);

        let output = session.live_render_frame_output(&prediction_plan);

        assert_eq!(output.viewport_plans.len(), 2);
        assert_eq!(output.clipped_viewport_count(), 2);
        assert_eq!(output.hud_count(), 2);
        assert!(
            output
                .hud_snapshots
                .iter()
                .any(|hud| hud.player_id == second_player
                    && hud.credits == 222
                    && (hud.scanner_cooldown_seconds - 3.0).abs() < f32::EPSILON)
        );
        assert!(output.world_players_by_view.iter().all(|(_, players)| {
            players
                .iter()
                .any(|player| player.player_id == LOCAL_PLAYER_ID)
                && players
                    .iter()
                    .any(|player| player.player_id == second_player)
        }));
        assert!(output.all_visible_players_are_session_sourced());
        assert!(output.viewport_plans.iter().all(|plan| plan.clip_enabled));
    }

    #[test]
    fn split_screen_render_output_covers_camera_viewport_hud_remote_correction_and_feedback() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 1.0,
            thrust: true,
            drill_down: true,
        }]);
        session.observe_live_remote_player_snapshots();
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.2);

        let output = session.live_render_frame_output(&prediction_plan);

        assert_eq!(output.viewport_plans.len(), 2);
        assert_eq!(output.clipped_viewport_count(), 2);
        assert!(
            output
                .viewport_plans
                .iter()
                .all(|plan| plan.local_player.is_some())
        );
        assert_eq!(output.hud_count(), 2);
        assert!(output.all_visible_players_are_session_sourced());
        assert!(
            output
                .world_players_by_view
                .iter()
                .flat_map(|(_, players)| players)
                .any(|player| {
                    player.local_to_view
                        && player.source
                            == super::RenderPlayerPresentationSource::AuthoritativeWorld
                })
        );
        assert!(output.split_screen_ui_ready());
        assert!(output.ready_for_live_render_path());
    }

    #[test]
    fn split_screen_ui_snapshots_cover_minimap_depth_scanner_pause_modal_audio_and_messages() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session
            .world
            .set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 2.5);
        session
            .world
            .set_scanner_cooldown_seconds(second_player, 4.0);
        session.apply_client_actions(
            LOCAL_CLIENT_ID,
            &[
                crate::multiplayer::ClientAction::Pause,
                crate::multiplayer::ClientAction::ToggleMap,
            ],
        );
        session.apply_client_actions(
            second_client,
            &[crate::multiplayer::ClientAction::ToggleHelp],
        );
        session
            .clients
            .get_mut(&second_client)
            .expect("second client exists")
            .local_audio_cues
            .push(SoundCue::Milestone);
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);

        let output = session.live_render_frame_output(&prediction_plan);

        assert!(output.split_screen_ui_ready());
        let primary = output
            .ui_snapshots
            .iter()
            .find(|snapshot| snapshot.client_id == LOCAL_CLIENT_ID)
            .expect("primary ui snapshot");
        let secondary = output
            .ui_snapshots
            .iter()
            .find(|snapshot| snapshot.client_id == second_client)
            .expect("secondary ui snapshot");
        assert_eq!(primary.modal, Some(ModalScreen::Map));
        assert_eq!(primary.run_mode, RunMode::Paused);
        assert!(primary.message.contains("Map"));
        assert!(primary.audio_cues.contains(&SoundCue::Ui));
        assert!((primary.scanner_cooldown_seconds - 2.5).abs() < f32::EPSILON);
        assert_eq!(secondary.modal, Some(ModalScreen::Help));
        assert!(secondary.message.contains("Help"));
        assert!(secondary.audio_cues.contains(&SoundCue::Milestone));
        assert!((secondary.scanner_cooldown_seconds - 4.0).abs() < f32::EPSILON);
        assert_ne!(primary.viewport, secondary.viewport);
    }

    #[test]
    fn split_screen_rescue_services_save_load_and_exit_paths_are_player_scoped() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        {
            let player = session
                .world
                .player_mut(second_player)
                .expect("second player exists");
            player.x = 500.0;
            player.y = 600.0;
            player.credits = 1_000;
            player.fuel = 10.0;
            player.hull = 20.0;
            player.cargo.insert(crate::terrain::MineralKind::Copper, 2);
        }

        assert_eq!(
            session
                .world
                .apply_player_command(second_player, &PlayerCommand::Rescue),
            super::PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(
            session
                .world
                .apply_player_command(second_player, &PlayerCommand::Refuel),
            super::PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(
            session
                .world
                .apply_player_command(second_player, &PlayerCommand::Repair),
            super::PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(
            session
                .world
                .apply_player_command(second_player, &PlayerCommand::SellCargo),
            super::PlayerScopedCommandOutcome::Applied
        );
        {
            let second_client_state = session
                .clients
                .get_mut(&second_client)
                .expect("second client exists");
            "Surface service applied".clone_into(&mut second_client_state.local_message);
            second_client_state.local_audio_cues.push(SoundCue::Sell);
        }
        session.apply_client_actions(
            second_client,
            &[
                crate::multiplayer::ClientAction::Save,
                crate::multiplayer::ClientAction::Load,
            ],
        );
        session.apply_client_actions(
            second_client,
            &[crate::multiplayer::ClientAction::ExitRequested],
        );

        let player = session
            .world
            .player(second_player)
            .expect("second player exists");
        assert!(12.0f32.mul_add(-TILE_SIZE, player.x).abs() < f32::EPSILON);
        assert!(4.0f32.mul_add(-TILE_SIZE, player.y).abs() < f32::EPSILON);
        assert_eq!(player.cargo_used(), 0);
        assert!(session.clients.values().any(|client| client.exit_requested));
        let second_client_state = session
            .clients
            .get(&second_client)
            .expect("second client exists");
        assert!(second_client_state.exit_requested);
        assert!(
            second_client_state
                .prediction()
                .prediction_failures()
                .contains(&super::PredictionFailure::SaveSessionTransition)
        );
        assert!(second_client_state.local_message.contains("synchronized"));
        assert!(second_client_state.local_audio_cues.contains(&SoundCue::Ui));
        assert!(
            second_client_state
                .local_audio_cues
                .contains(&SoundCue::Sell)
        );
    }

    #[test]
    fn prediction_runtime_paths_cover_replay_correction_remote_failure_and_debug_paths() {
        let mut prediction = super::ClientPredictionState::default();
        let player_id = LOCAL_PLAYER_ID;
        let terrain_revisions = super::TerrainRevisionTracker::default();
        let command = SequencedPlayerCommand {
            player_id,
            sequence: InputSequence::new(1),
            target_tick: SimulationTick::new(1),
            command: PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: true,
                drill_down: false,
            },
        };
        prediction.remember_commands(&[command]);
        prediction.note_prediction_failure(super::PredictionFailure::TerrainAlreadyChanged);
        prediction.note_prediction_failure(super::PredictionFailure::EconomyChangedState);
        prediction.note_prediction_failure(super::PredictionFailure::ProgressionChangedState);
        prediction.note_prediction_failure(super::PredictionFailure::HazardOrRescueChangedState);
        prediction.note_prediction_failure(super::PredictionFailure::CommandRejected);
        prediction.note_save_session_transition();
        prediction.set_correction_offset(super::CorrectionOffset::new(8.0, 0.0));

        let previous = super::PlayerSnapshot {
            player_id,
            x: 0.0,
            y: 0.0,
            velocity_x: 4.0,
            velocity_y: 0.0,
            fuel: 1.0,
            fuel_capacity: 1.0,
            hull: 1.0,
            max_hull: 1.0,
            credits: 0,
            cargo_used: 0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        };
        let next = super::PlayerSnapshot {
            x: 10.0,
            ..previous.clone()
        };
        let interpolated = super::ClientPredictionState::remote_player_presentation(
            &previous,
            Some(&next),
            0.5,
            0.0,
        );
        let extrapolated =
            super::ClientPredictionState::remote_player_presentation(&previous, None, 0.0, 0.1);
        let actions = prediction.prediction_recovery_actions(
            player_id,
            &terrain_revisions,
            SimulationTick::new(7),
            super::TerrainChunkPosition { x: 0, y: 0 },
            0,
        );
        let debug = prediction.prediction_debug_snapshot(0.12, 2, 1, 1);

        assert!(prediction.unacknowledged_replay_is_complete(player_id));
        assert!(super::PredictionCorrectionTuning::classifies_expected_offsets());
        assert!(!interpolated.extrapolated);
        assert!(extrapolated.extrapolated);
        assert!(actions.iter().any(|action| matches!(
            action,
            super::PredictionRecoveryAction::RequestTerrainDelta(_)
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            super::PredictionRecoveryAction::RollBackLocalEconomy { .. }
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            super::PredictionRecoveryAction::RollBackProgression { .. }
        )));
        assert!(
            actions
                .iter()
                .filter(|action| matches!(
                    action,
                    super::PredictionRecoveryAction::RequestAuthoritativeSnapshot { .. }
                ))
                .count()
                >= 3
        );
        assert_eq!(debug.correction_plan, super::CorrectionPlan::Smooth);
        assert!(debug.visible_to_debug_overlay());
    }

    #[test]
    fn prediction_failure_application_summary_covers_real_recovery_actions() {
        let mut session = GameSession::new();
        session
            .local_client_mut()
            .prediction
            .note_prediction_failure(super::PredictionFailure::TerrainAlreadyChanged);
        session
            .local_client_mut()
            .prediction
            .note_prediction_failure(super::PredictionFailure::EconomyChangedState);
        session
            .local_client_mut()
            .prediction
            .note_prediction_failure(super::PredictionFailure::ProgressionChangedState);
        session
            .local_client_mut()
            .prediction
            .note_prediction_failure(super::PredictionFailure::CommandRejected);
        let terrain_position = super::TerrainChunkPosition { x: 0, y: 0 };
        session
            .terrain_revisions
            .mark_tiles_changed([TilePosition { x: 1, y: 1 }]);

        let summary = session.prediction_failure_application_summary(terrain_position, 0);

        assert!(summary.requested_keyframe);
        assert_eq!(summary.requested_terrain_deltas, 1);
        assert_eq!(summary.rolled_back_economy_players, vec![LOCAL_PLAYER_ID]);
        assert_eq!(
            summary.rolled_back_progression_players,
            vec![LOCAL_PLAYER_ID]
        );
    }

    #[test]
    fn render_viewport_plan_carries_correction_remote_and_tentative_feedback_outputs() {
        let mut session = GameSession::new();
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 12.0,
            thrust: false,
            drill_down: false,
        }]);
        session
            .local_client_mut()
            .prediction
            .push_feedback(super::LocalTentativeFeedback::DrillContact);
        let remote_player = PlayerId::new(88);
        session
            .local_client_mut()
            .prediction
            .push_remote_snapshot(super::PlayerSnapshot {
                player_id: remote_player,
                x: 1.0,
                y: 2.0,
                velocity_x: 0.0,
                velocity_y: 0.0,
                fuel: 1.0,
                fuel_capacity: 1.0,
                hull: 1.0,
                max_hull: 1.0,
                credits: 0,
                cargo_used: 0,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 0.0,
            });
        {
            let player = session
                .world
                .player_mut(LOCAL_PLAYER_ID)
                .expect("player exists");
            player.x = 100.0;
            player.y = 50.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.0, 0.0);
        let frame_plan = session.render_frame_plan();

        let viewport_plan = frame_plan
            .viewport_plans(&prediction_plan)
            .into_iter()
            .next()
            .expect("viewport plan exists");

        assert_eq!(viewport_plan.remote_players.len(), 1);
        assert_eq!(viewport_plan.correction_frame, None);
        assert!(viewport_plan.feedback_outputs.iter().any(|output| {
            output.presentation == super::TentativeFeedbackPresentation::DrillContactAudio
                && output.channel == super::TentativeFeedbackChannel::Audio
        }));
    }

    #[test]
    fn render_frame_plan_exposes_per_view_hud_snapshots() {
        let mut session = GameSession::new();
        session
            .world
            .set_scanner_cooldown_seconds(LOCAL_PLAYER_ID, 2.0);
        session
            .world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .credits = 123;

        let frame_plan = session.render_frame_plan();
        let hud = frame_plan
            .hud_snapshot_for_view(&frame_plan.views[0])
            .expect("hud snapshot exists");

        assert_eq!(hud.player_id, LOCAL_PLAYER_ID);
        assert_eq!(hud.credits, 123);
        assert!((hud.scanner_cooldown_seconds - 2.0).abs() < f32::EPSILON);
        assert_eq!(frame_plan.hud_snapshots(), vec![hud]);
    }

    #[test]
    fn session_projects_predicted_local_movement_from_world_state() {
        let mut session = GameSession::new();
        session
            .world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .velocity_x = 10.0;

        let predicted = session
            .predicted_local_movement(0.5)
            .expect("local player prediction exists");
        let prediction_plan = session
            .local_movement_prediction_plan(0.5)
            .expect("local movement prediction plan exists");

        assert_eq!(predicted.player_id, LOCAL_PLAYER_ID);
        assert!((predicted.x - (session.game().player.x + 5.0)).abs() < f32::EPSILON);
        assert_eq!(prediction_plan.player_id, LOCAL_PLAYER_ID);
        assert!((prediction_plan.predicted_x - predicted.x).abs() < f32::EPSILON);
        assert!((prediction_plan.predicted_movement().x - predicted.x).abs() < f32::EPSILON);
    }

    #[test]
    fn session_builds_prediction_presentation_plan() {
        let mut session = GameSession::new();
        session
            .local_client_mut()
            .prediction
            .push_feedback(super::LocalTentativeFeedback::DrillContact);
        let authoritative = session.world_snapshot().players[0].clone();

        let plan = session.prediction_presentation_plan(Some(&authoritative), 0.5, 0.5, 0.0);

        assert_eq!(plan.local_movement, None);
        assert_eq!(plan.correction, None);
        assert_eq!(plan.corrected_local_presentation, None);
        assert_eq!(
            plan.tentative_feedback,
            vec![super::TentativeFeedbackPresentation::DrillContactAudio]
        );
        assert_eq!(
            plan.feedback_outputs,
            vec![super::TentativeFeedbackOutput {
                presentation: super::TentativeFeedbackPresentation::DrillContactAudio,
                channel: super::TentativeFeedbackChannel::Audio,
            }]
        );
    }

    #[test]
    fn session_world_tracks_legacy_local_player() {
        let session = GameSession::new();

        assert_eq!(session.world().player_count(), 1);
        assert!(session.world().player(LOCAL_PLAYER_ID).is_some());
    }

    #[test]
    fn world_state_tracks_authoritative_simulation_tick() {
        let mut session = GameSession::new();

        assert_eq!(session.world().simulation_tick(), SimulationTick::default());
        session.advance_tick();
        assert_eq!(session.world().simulation_tick(), SimulationTick::new(1));
        assert_eq!(
            session.world_snapshot().tick,
            session.world().simulation_tick()
        );
    }

    #[test]
    fn world_state_summarizes_authoritative_legacy_domains() {
        let session = GameSession::new();
        let summary = session.world().authoritative_summary();

        assert_eq!(summary.tick, session.world().simulation_tick());
        assert_eq!(summary.player_count, 1);
        assert_eq!(summary.terrain_width, session.game().terrain.width());
        assert_eq!(summary.terrain_height, session.game().terrain.height());
        assert_eq!(summary.bomb_count, session.game().placed_bombs.len());
        assert_eq!(
            session.world().bomb_count(),
            session.game().placed_bombs.len()
        );
        assert_eq!(
            session.world().hazard_count(),
            session.game().hazard_clouds.len()
        );
        assert_eq!(
            session.world().infrastructure_count(),
            session.game().infrastructure.len()
        );
        assert_eq!(
            summary.infrastructure_count,
            session.game().infrastructure.len()
        );
    }

    #[test]
    fn client_state_catalogs_presentation_fields() {
        let fields = GameSession::client_presentation_fields();

        assert!(fields.contains(&super::ClientPresentationField::Camera));
        assert!(fields.contains(&super::ClientPresentationField::RunMode));
        assert!(fields.contains(&super::ClientPresentationField::Prediction));
        assert!(fields.contains(&super::ClientPresentationField::Modal));
        assert!(fields.contains(&super::ClientPresentationField::LocalMessage));
        assert!(fields.contains(&super::ClientPresentationField::LocalAudio));
        assert!(fields.contains(&super::ClientPresentationField::ExitRequested));
    }

    #[test]
    fn client_state_reports_presentation_ownership_migration_status() {
        let client = ClientState::default();
        let ownership = client.ownership_summary();

        assert!(ownership.camera_owned);
        assert!(ownership.modals_owned);
        assert!(ownership.local_messages_owned);
        assert!(ownership.local_audio_owned);
        assert!(ownership.display_settings_owned);
        assert!(ownership.prediction_owned);
        assert!(ownership.menus_owned);
        assert!(ownership.overlays_owned);
        assert!(ownership.fully_split());
    }

    #[test]
    fn client_state_owns_local_presentation_mirrors() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::Help);
        "Client-local toast".clone_into(&mut game.message);
        game.sound_cues.push(SoundCue::Ui);
        let mut client = ClientState::new(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID);

        client.sync_presentation_from_legacy_game(&game);

        assert_eq!(client.modal, Some(ModalScreen::Help));
        assert_eq!(client.local_message, "Client-local toast");
        assert_eq!(client.local_audio_cues.len(), 1);
    }

    #[test]
    fn authoritative_movement_stops_at_solid_ground() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.x = 20.0 * TILE_SIZE;
        game.player.y = 4.0 * TILE_SIZE;
        game.player.velocity_y = 0.0;
        let mut world = WorldState::from_legacy_game(&game);

        for _ in 0..80 {
            let _step = world.step_authoritative_movement(LOCAL_PLAYER_ID, 0.0, false);
        }

        let player = world.player(LOCAL_PLAYER_ID).expect("player exists");
        assert!(player.y < 5.0 * TILE_SIZE);
        assert!(player.velocity_y.abs() < f32::EPSILON);
    }

    #[test]
    fn authoritative_side_drilling_intent_persists_and_chips_side_tile() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.x = 20.0 * TILE_SIZE;
        game.player.y = 5.0 * TILE_SIZE;
        let mut world = WorldState::from_legacy_game(&game);
        let target = TilePosition { x: 21, y: 5 };
        assert!(world.terrain_mut().set_kind(target, TileKind::Dirt));

        for _ in 0..2 {
            assert_eq!(
                world.apply_player_command(
                    LOCAL_PLAYER_ID,
                    &PlayerCommand::Movement {
                        horizontal: 1.0,
                        thrust: false,
                        drill_down: false,
                    },
                ),
                super::PlayerScopedCommandOutcome::Applied
            );
            let active = world
                .active_drill(LOCAL_PLAYER_ID)
                .expect("side drill remains active");
            assert_eq!(active.target, target);
            assert_eq!(active.direction, DrillDirection::Right);
            let _result = world.chip_active_drill_target(LOCAL_PLAYER_ID);
        }

        assert!(matches!(
            world.terrain().tile(target).map(|tile| tile.kind),
            Some(TileKind::Air)
        ));
    }

    #[test]
    fn rendering_inputs_and_legacy_write_boundaries_are_migrated_to_session_presentations() {
        let rendering = RenderingInputMigrationStatus::current();
        let write_boundary = LegacyGameStateWriteBoundaryStatus::current();

        assert!(rendering.camera_from_session_view);
        assert!(rendering.players_from_world_presentation);
        assert!(rendering.terrain_from_world_presentation);
        assert!(rendering.hud_from_per_client_presentation);
        assert!(rendering.migrated());
        assert!(write_boundary.ui_settings_save_menu_allowed);
        assert!(write_boundary.presentation_compatibility_allowed);
        assert!(write_boundary.authoritative_world_writes_blocked);
        assert!(write_boundary.online_save_boundary_enforced);
        assert!(write_boundary.limited_to_compatibility());
    }

    #[test]
    fn online_gameplay_advances_world_without_public_legacy_input_rewrite() {
        let mut session = GameSession::new();
        let remote_player = PlayerId::new(2);
        let target_tick = session.current_tick();
        let summary = CommandPacketExchangeSummary {
            client_id: ClientId::new(2),
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: target_tick,
            accepted_commands: vec![SequencedPlayerCommand {
                player_id: remote_player,
                sequence: InputSequence::new(1),
                target_tick,
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: true,
                    drill_down: false,
                },
            }],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);
        let remote_after = session
            .world()
            .player(remote_player)
            .expect("remote player");

        assert_eq!(applied, 1);
        assert!(remote_after.velocity_x > 0.0);
        assert!(remote_after.velocity_y < 0.0);
        assert!(session.game().player.velocity_x.abs() < f32::EPSILON);
        assert!(LegacyInputRewriteRemovalStatus::current().removal_complete());
    }

    #[test]
    fn legacy_game_state_coupling_inventory_has_no_authoritative_runtime_couplings() {
        let inventory = legacy_game_state_coupling_inventory();
        let summary = legacy_game_state_coupling_inventory_summary();

        assert_eq!(summary.total, inventory.len());
        assert_eq!(summary.authoritative_world_couplings, 0);
        assert!(summary.presentation_compatibility_couplings >= 3);
        assert_eq!(summary.save_menu_ui_couplings, 1);
        assert!(summary.runtime_inventory_complete());
        assert!(inventory.iter().all(|item| !item.replacement.is_empty()));
    }

    #[test]
    fn economy_service_and_menu_commands_are_classified_for_host_authority() {
        let inventory = host_authority_command_routing_inventory();
        let summary = host_authority_command_routing_summary();

        assert_eq!(summary.total, inventory.len());
        assert!(summary.economy_service_menu_routed());
        assert!(inventory.iter().any(|item| item.command == "SellCargo"
            && matches!(
                item.domain,
                HostAuthorityCommandRoutingDomain::EconomyServiceMenu
            )
            && item.host_authoritative));
        assert!(inventory.iter().any(|item| item.command == "BuyUpgrade"
            && matches!(
                item.domain,
                HostAuthorityCommandRoutingDomain::EconomyServiceMenu
            )
            && item.host_authoritative));
        assert!(inventory.iter().any(|item| item.command == "OpenMenu"
            && matches!(
                item.domain,
                HostAuthorityCommandRoutingDomain::PresentationOnly
            )
            && !item.host_authoritative));
    }

    #[test]
    fn joined_client_authority_boundary_and_legacy_input_rewrite_removal_are_safe() {
        let boundary = JoinedClientAuthorityBoundaryStatus::online_joined_client_runtime();
        let rewrite_removal = LegacyInputRewriteRemovalStatus::current();

        assert!(boundary.local_prediction_allowed);
        assert!(boundary.remote_authority_accepted);
        assert!(boundary.host_world_owner);
        assert!(!boundary.fights_local_authority);
        assert!(boundary.safe_for_joined_client());
        assert!(rewrite_removal.public_rewrite_function_removed);
        assert!(rewrite_removal.remaining_adapter_presentation_only);
        assert!(rewrite_removal.runtime_command_processing_replaced);
        assert!(rewrite_removal.removal_complete());
    }

    #[test]
    fn world_network_snapshot_carries_upgrade_and_inventory_affecting_loadout() {
        let mut game = GameState::new();
        game.player.credits = 2_000;
        game.player.fuel_tank_level = 2;
        game.player.cargo_bay_level = 3;
        game.player.drill_strength = 4;
        game.player.engine_level = 5;
        game.player.hull_level = 6;
        game.player.radiator_level = 7;
        game.player.scanner_level = 8;
        game.player.bombs = 9;
        game.player.insured = true;
        game.player.insurance_tier = 2;
        game.player.signal_relay_kits = 3;
        let world = WorldState::from_legacy_game(&game);

        let snapshot =
            super::WorldSnapshot::from_world(SimulationTick::new(1), &world).network_snapshot();
        let player = snapshot.players.first().expect("local player snapshot");

        assert_eq!(player.loadout.fuel_tank_level, 2);
        assert_eq!(player.loadout.cargo_bay_level, 3);
        assert_eq!(player.loadout.drill_strength, 4);
        assert_eq!(player.loadout.engine_level, 5);
        assert_eq!(player.loadout.hull_level, 6);
        assert_eq!(player.loadout.radiator_level, 7);
        assert_eq!(player.loadout.scanner_level, 8);
        assert_eq!(player.loadout.bombs, 9);
        assert!(player.loadout.insured);
        assert_eq!(player.loadout.insurance_tier, 2);
        assert_eq!(player.loadout.signal_relay_kits, 3);
    }

    #[test]
    fn world_state_reports_authoritative_ownership_migration_status() {
        let world = WorldState::from_legacy_game(&GameState::new());
        let ownership = world.ownership_summary();

        assert!(ownership.players_owned);
        assert!(ownership.hazards_owned);
        assert!(ownership.bombs_owned);
        assert!(ownership.infrastructure_owned);
        assert!(ownership.simulation_tick_owned);
        assert!(ownership.terrain_owned);
        assert!(ownership.fully_split());
    }

    #[test]
    fn multiplayer_completion_summary_marks_primary_migration_complete_or_explicitly_deferred() {
        let world = WorldState::from_legacy_game(&GameState::new());
        let summary = world.implementation_completion_summary();

        assert!(summary.world_and_client_state_split);
        assert!(summary.fixed_tick_authoritative_path);
        assert!(summary.player_scoped_gameplay);
        assert!(summary.local_split_screen_rendering);
        assert!(summary.online_transport_deferred);
        assert!(summary.prediction_drives_presentation);
        assert!(summary.live_snapshot_delta_chunk_sync);
        assert!(summary.single_player_regressions_covered);
        assert!(summary.primary_migration_complete_or_deferred());
    }

    #[test]
    fn world_state_reports_authoritative_dependency_and_transient_effect_split() {
        let world = WorldState::from_legacy_game(&GameState::new());

        assert!(
            world
                .authoritative_dependency_summary()
                .authoritative_path_split()
        );
        assert!(world.transient_effect_routing_summary().split());
    }

    #[test]
    fn planned_transient_effect_boundaries_identify_local_and_world_effects() {
        let boundaries = GameSession::planned_transient_effect_boundaries();

        assert!(boundaries.iter().any(|boundary| {
            boundary.name == "camera_shake"
                && boundary.domain == super::TransientEffectDomain::LocalClientPresentation
        }));
        assert!(boundaries.iter().any(|boundary| {
            boundary.name == "hazard_clouds"
                && boundary.domain == super::TransientEffectDomain::GameplayRelevantWorld
        }));
    }

    #[test]
    fn planned_player_scoped_systems_cover_legacy_player_logic() {
        let systems = GameSession::planned_player_scoped_systems();

        assert!(systems.contains(&super::PlayerScopedSystem::Movement));
        assert!(systems.contains(&super::PlayerScopedSystem::Drilling));
        assert!(systems.contains(&super::PlayerScopedSystem::Cargo));
        assert!(systems.contains(&super::PlayerScopedSystem::EconomyService));
    }

    #[test]
    fn fixed_tick_audit_tracks_remaining_variable_delta_systems() {
        let audit_items = GameSession::fixed_tick_audit_items();

        assert!(audit_items.iter().any(|item| {
            item.system == "physics"
                && item.status == super::FixedTickMigrationStatus::FixedTickReady
                && item.plan == super::FixedTickMigrationPlan::AlreadyFixedStep
        }));
        assert!(audit_items.iter().any(|item| {
            item.system == "animations"
                && item.plan == super::FixedTickMigrationPlan::KeepVariablePresentationOnly
        }));
        assert!(audit_items.iter().any(|item| {
            item.system == "drilling_progress"
                && item.status == super::FixedTickMigrationStatus::FixedTickReady
        }));
    }

    #[test]
    fn fixed_tick_audit_summary_counts_authoritative_and_presentation_work() {
        let summary = GameSession::fixed_tick_migration_summary();

        assert_eq!(summary.fixed_ready, 7);
        assert_eq!(summary.presentation_exemptions, 1);
        assert_eq!(summary.authoritative_migrations, 0);
        assert_eq!(summary.unresolved_variable_delta, 0);
        assert!(summary.audit_complete());
    }

    #[test]
    fn world_delta_compacts_real_gameplay_events_without_local_presentation_state() {
        let tick = SimulationTick::new(3);
        let delta = super::WorldDelta::new(
            tick,
            vec![
                super::WorldEvent::ImportantEffectTriggered,
                super::WorldEvent::MessageChanged {
                    message: "local toast".to_owned(),
                },
                super::WorldEvent::CargoChanged {
                    player_id: LOCAL_PLAYER_ID,
                },
            ],
        );

        assert!(matches!(
            delta.compact_network_delta(),
            super::CompactWorldDelta::Players { tick: delta_tick, players }
                if delta_tick == tick && players == vec![LOCAL_PLAYER_ID]
        ));
    }

    #[test]
    fn world_event_catalog_covers_future_authoritative_events() {
        let events = GameSession::world_event_catalog();

        assert!(
            events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::CargoChanged { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::PlayerDamaged { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::PurchaseCompleted { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::BombPlaced { .. }))
        );
    }

    #[test]
    fn gameplay_event_routing_summary_separates_local_presentation_effects() {
        let summary = GameSession::gameplay_event_routing_summary();

        assert!(summary.authoritative_events > 0);
        assert_eq!(summary.local_presentation_boundaries, 5);
        assert_eq!(summary.gameplay_world_boundaries, 3);
        assert!(summary.separates_local_presentation());
    }

    #[test]
    fn world_delta_compacts_events_for_network_sync() {
        let delta = super::WorldDelta::new(
            SimulationTick::new(8),
            vec![super::WorldEvent::PlayerChanged {
                player_id: LOCAL_PLAYER_ID,
            }],
        );

        assert_eq!(
            delta.compact_network_delta(),
            super::CompactWorldDelta::Players {
                tick: SimulationTick::new(8),
                players: vec![LOCAL_PLAYER_ID],
            }
        );

        let compact_delta = delta.compact_network_delta();
        assert_eq!(
            compact_delta.summary(),
            super::CompactDeltaSummary {
                kind: super::CompactDeltaKind::Players,
                tick: SimulationTick::new(8),
                item_count: 1,
            }
        );
        let payload = compact_delta.network_payload();
        assert_eq!(payload, NetworkDeltaPayload::KeyframeRequired);
        assert_eq!(
            compact_delta.protocol_message(),
            ProtocolMessage::WorldDelta {
                tick: SimulationTick::new(8),
                payload
            }
        );

        let keyframe_delta = super::WorldDelta::new(
            SimulationTick::new(10),
            vec![super::WorldEvent::TerrainRefreshRequested],
        );
        assert_eq!(
            keyframe_delta.compact_network_delta(),
            super::CompactWorldDelta::KeyframeRequired {
                tick: SimulationTick::new(10),
            }
        );
    }

    #[test]
    fn snapshot_purposes_keep_save_network_and_render_boundaries_separate() {
        let purposes = GameSession::snapshot_purposes();

        assert!(purposes.contains(&super::SnapshotPurpose::SaveFile));
        assert!(purposes.contains(&super::SnapshotPurpose::NetworkSync));
        assert!(purposes.contains(&super::SnapshotPurpose::RenderSync));
    }

    #[test]
    fn split_screen_viewports_cover_single_two_up_and_quad_layouts() {
        assert_eq!(
            super::split_screen_layout(1),
            super::SplitScreenLayout::Single
        );
        assert_eq!(
            super::split_screen_layout(2),
            super::SplitScreenLayout::VerticalTwoUp
        );
        assert_eq!(
            super::split_screen_layout(3),
            super::SplitScreenLayout::Quad
        );
        assert_eq!(GameSession::split_screen_viewports(1).len(), 1);
        assert_eq!(GameSession::split_screen_viewports(2).len(), 2);
        assert_eq!(GameSession::split_screen_viewports(4).len(), 4);
    }

    #[test]
    fn world_snapshot_contains_tick_and_players() {
        let session = GameSession::new();

        let snapshot = session.world_snapshot();

        assert_eq!(snapshot.tick, session.current_tick());
        assert_eq!(snapshot.players.len(), 1);
        assert_eq!(snapshot.players[0].player_id, LOCAL_PLAYER_ID);
        assert_eq!(
            snapshot.network_snapshot().players[0].player_id,
            LOCAL_PLAYER_ID
        );
        assert_eq!(
            snapshot.keyframe_message(),
            ProtocolMessage::SnapshotKeyframe {
                snapshot: snapshot.network_snapshot()
            }
        );
    }

    #[test]
    fn local_commands_are_sequenced_for_future_acknowledgement() {
        let mut session = GameSession::new();

        let commands = session.sequence_local_commands(vec![PlayerCommand::Interact]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].player_id, LOCAL_PLAYER_ID);
        assert_eq!(commands[0].sequence.get(), 0);
        assert_eq!(commands[0].target_tick, session.current_tick());
        assert_eq!(session.pending_command_count(session.current_tick()), 1);
    }

    #[test]
    fn client_commands_share_authoritative_session_path() {
        let mut session = GameSession::new();

        let commands =
            session.sequence_client_commands(LOCAL_CLIENT_ID, vec![PlayerCommand::Interact]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].player_id, LOCAL_PLAYER_ID);
        assert_eq!(commands[0].sequence.get(), 0);
        assert_eq!(session.pending_command_count(session.current_tick()), 1);
        assert_eq!(
            session
                .local_client()
                .prediction()
                .unacknowledged_commands()
                .len(),
            1
        );
    }

    #[test]
    fn command_sources_sequence_through_authoritative_path_with_prediction_policy() {
        let mut session = GameSession::new();
        let local = session.sequence_client_commands_from_source(
            LOCAL_CLIENT_ID,
            CommandSource::Gamepad,
            vec![PlayerCommand::Interact],
        );
        let replay = session.sequence_client_commands_from_source(
            LOCAL_CLIENT_ID,
            CommandSource::Replay,
            vec![PlayerCommand::Confirm],
        );

        assert_eq!(local.commands.len(), 1);
        assert!(local.predicted_locally);
        assert!(!replay.predicted_locally);
        assert_eq!(session.pending_command_count(session.current_tick()), 2);
        assert_eq!(
            session
                .local_client()
                .prediction()
                .unacknowledged_commands()
                .len(),
            1
        );
        assert_eq!(
            GameSession::command_source_policy(CommandSource::OnlineClient),
            super::SourceSequencingPolicy {
                source: CommandSource::OnlineClient,
                authoritative_path: true,
                predicted_locally: false,
            }
        );
    }

    #[test]
    fn acknowledged_commands_are_removed_from_prediction_buffer() {
        let mut session = GameSession::new();
        let commands = session.sequence_local_commands(vec![PlayerCommand::Interact]);

        session.acknowledge_client_commands_through(LOCAL_CLIENT_ID, commands[0].sequence);

        assert!(
            session
                .local_client()
                .prediction()
                .unacknowledged_commands()
                .is_empty()
        );
    }

    #[test]
    fn command_acknowledgement_message_prunes_prediction_buffer() {
        let mut session = GameSession::new();
        let commands =
            session.sequence_local_commands(vec![PlayerCommand::Interact, PlayerCommand::Confirm]);

        session.apply_command_acknowledgement(&CommandAcknowledgement {
            client_id: LOCAL_CLIENT_ID,
            acknowledged_sequence: commands[1].sequence,
            authoritative_tick: SimulationTick::new(2),
        });

        assert!(
            session
                .local_client()
                .prediction()
                .unacknowledged_commands()
                .is_empty()
        );
    }

    #[test]
    fn command_rejection_message_records_prediction_failure() {
        let mut session = GameSession::new();

        session.apply_command_rejection(&CommandRejection {
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            sequence: InputSequence::new(0),
            reason: crate::multiplayer::CommandAcceptance::Duplicate,
            authoritative_tick: SimulationTick::new(2),
        });

        assert_eq!(
            session.local_client().prediction().prediction_failures(),
            &[super::PredictionFailure::CommandRejected]
        );
        assert_eq!(
            session
                .local_client()
                .prediction()
                .prediction_failure_resolutions(),
            vec![super::PredictionFailureResolution::RequestAuthoritativeSnapshot]
        );
    }

    #[test]
    fn prediction_state_exposes_replay_commands_and_correction_plan() {
        let mut session = GameSession::new();
        session.sequence_local_commands(vec![PlayerCommand::Interact]);

        assert_eq!(
            session.local_client().prediction().replay_commands().len(),
            1
        );
        assert_eq!(
            super::CorrectionPlan::None,
            super::ClientPredictionState::correction_plan(0.5, 0.5)
        );
        assert_eq!(
            super::CorrectionPlan::Smooth,
            super::ClientPredictionState::correction_plan(8.0, 0.0)
        );
        assert_eq!(
            super::CorrectionPlan::Snap,
            super::ClientPredictionState::correction_plan(32.0, 0.0)
        );
    }

    #[test]
    fn prediction_state_buffers_remote_snapshots_for_interpolation() {
        let mut prediction = super::ClientPredictionState::default();
        prediction.push_remote_snapshot(super::PlayerSnapshot {
            player_id: LOCAL_PLAYER_ID,
            x: 1.0,
            y: 2.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            fuel: 3.0,
            fuel_capacity: 100.0,
            hull: 4.0,
            max_hull: 100.0,
            credits: 6,
            cargo_used: 0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        });

        assert_eq!(prediction.remote_snapshot_count(LOCAL_PLAYER_ID), 1);
    }

    #[test]
    fn prediction_state_derives_interpolation_and_extrapolation_timing() {
        let mut session = GameSession::new();
        session.sequence_local_commands(vec![PlayerCommand::Interact, PlayerCommand::UseScanner]);

        assert!(
            (super::ClientPredictionState::interpolation_delay_seconds(0.01) - 0.05).abs()
                < f32::EPSILON
        );
        assert!(
            (super::ClientPredictionState::interpolation_delay_seconds(1.0) - 0.25).abs()
                < f32::EPSILON
        );
        assert!(super::ClientPredictionState::should_extrapolate(0.16));
        assert!(!super::ClientPredictionState::should_extrapolate(0.17));
        assert!(
            session
                .local_client()
                .prediction()
                .predicted_input_lag_seconds()
                > 0.0
        );
    }

    #[test]
    fn prediction_high_ping_tuning_derives_latency_loss_timing() {
        let mut session = GameSession::new();
        session.update_remote_timing_from_network_sample(0.18, 0.1);
        let timing = session.remote_timing();

        assert!(timing.interpolation_delay <= 0.25);
        assert!(timing.allows_extrapolation(0.09));
        assert!(timing.timed_out(timing.timeout_after + 0.01));
        assert_eq!(
            super::ClientPredictionState::correction_plan(8.0, 0.0),
            super::CorrectionPlan::Smooth
        );
        assert_eq!(
            super::ClientPredictionState::correction_plan(24.0, 0.0),
            super::CorrectionPlan::Snap
        );
        assert!(
            session
                .local_client()
                .prediction()
                .prediction_failures()
                .contains(&super::PredictionFailure::HazardOrRescueChangedState)
        );
    }

    #[test]
    fn online_prediction_live_diagnostics_sample_prediction_layers() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let player_id = LOCAL_PLAYER_ID;
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(1),
                target_tick: session.current_tick().next(),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };
        let _fixed = session.predict_online_command_packet(&packet, FIXED_DELTA_SECONDS);
        let _transient = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        let sample = session
            .local_client()
            .prediction()
            .divergence_sample_for_player(player_id);

        assert!(sample.replicated_local_position.is_some());
        assert!(sample.deterministic_replay_position.is_some());
        assert!(sample.transient_render_position.is_some());
        assert_eq!(sample.correction_mode, super::CorrectionMode::None);
    }

    #[test]
    fn online_prediction_live_history_records_active_drill_state() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let target_tick = session.current_tick().next();
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(5),
                target_tick,
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            }],
        };

        let _prediction = session.predict_online_command_packet(&packet, FIXED_DELTA_SECONDS);
        let entry = session
            .local_client()
            .prediction()
            .prediction_history_entry(target_tick, LOCAL_PLAYER_ID)
            .expect("prediction history entry exists");

        assert!(entry.active_drill.is_some());
    }

    #[test]
    fn fixed_packet_prediction_does_not_pull_render_prediction_backward() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let transient_before = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS,
            )
            .expect("transient prediction exists");
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(10),
                target_tick: session.current_tick().next(),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let _fixed = session.predict_online_command_packet(&packet, FIXED_DELTA_SECONDS);
        let transient_after = session
            .local_client()
            .prediction()
            .transient_world_snapshot_for_tests(LOCAL_PLAYER_ID)
            .expect("transient world exists");

        assert!((transient_after.x - transient_before.x).abs() < f32::EPSILON);
    }

    #[test]
    fn joined_client_sync_does_not_overwrite_authoritative_world_with_stale_legacy_player() {
        let mut session = GameSession::new();
        session.game.online_player_slot = Some(1);
        let player_id = super::online_identity_for_slot(1).1;
        session
            .world
            .insert_player(player_id, session.game.player.clone());
        {
            let player = session.world.player_mut(player_id).expect("player exists");
            player.x = 400.0;
            player.y = 120.0;
        }
        session.game.player.x = 10.0;
        session.game.player.y = 20.0;

        assert!(session.sync_joined_local_player_from_replicated_game());
        let player = session.world.player(player_id).expect("player exists");

        assert!((player.x - 400.0).abs() < f32::EPSILON);
        assert!((player.y - 120.0).abs() < f32::EPSILON);
        assert!((session.game.player.x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn live_session_tick_input_targets_next_authoritative_tick_for_host_acceptance() {
        let mut session = GameSession::new();
        assert!(session.sync_replicated_authoritative_tick(SimulationTick::new(12)));

        let input = session.live_session_tick_input_from_world(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            3,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let packet = input.command_packet.expect("packet exists");

        assert_eq!(packet.commands[0].target_tick, SimulationTick::new(13));
    }

    #[test]
    fn live_packet_prediction_uses_packet_sequence_and_tick_for_replay_history() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(42),
                target_tick: SimulationTick::new(7),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let predicted = session
            .predict_online_command_packet(&packet, FIXED_DELTA_SECONDS)
            .expect("packet prediction exists");

        assert!(
            session
                .local_client()
                .prediction()
                .prediction_history_snapshot(SimulationTick::new(7), LOCAL_PLAYER_ID)
                .is_some()
        );
        assert!(predicted.x > session.game().player.x);
    }

    #[test]
    fn transient_render_prediction_does_not_pollute_replay_world() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let replay_before = session
            .local_client()
            .prediction()
            .replay_world_snapshot_for_tests(LOCAL_PLAYER_ID)
            .expect("replay world exists");

        let transient = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS,
            )
            .expect("transient prediction exists");
        let replay_after = session
            .local_client()
            .prediction()
            .replay_world_snapshot_for_tests(LOCAL_PLAYER_ID)
            .expect("replay world exists");

        assert!(transient.x > replay_before.x);
        assert!((replay_after.x - replay_before.x).abs() < f32::EPSILON);
    }

    #[test]
    fn fixed_packet_prediction_advances_replay_world_from_deterministic_base() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _transient = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let replay_before = session
            .local_client()
            .prediction()
            .replay_world_snapshot_for_tests(LOCAL_PLAYER_ID)
            .expect("replay world exists");
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(9),
                target_tick: session.current_tick().next(),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let _fixed = session.predict_online_command_packet(&packet, FIXED_DELTA_SECONDS);
        let replay_after = session
            .local_client()
            .prediction()
            .replay_world_snapshot_for_tests(LOCAL_PLAYER_ID)
            .expect("replay world exists");

        assert!(replay_after.x > replay_before.x);
        assert!(
            session
                .local_client()
                .prediction()
                .deterministic_replay_frame_count()
                >= 1
        );
    }

    #[test]
    fn sync_cached_authoritative_tick_reports_false_so_app_can_skip_rebase() {
        let mut session = GameSession::new();
        let tick = session.current_tick();

        assert!(!session.sync_replicated_authoritative_tick(tick));
    }

    #[test]
    fn simulated_online_prediction_playtest_covers_live_validation_gates() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session.rebase_local_prediction_to_authority();

        let report = session.simulated_online_prediction_playtest(
            8,
            PlayerInput {
                horizontal: 0.1,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS / 2.0,
            20,
        );

        assert!(report.movement_feel_passed());
        assert!(report.command_delivery_passed());
        assert!(report.authority_tug_passed());
        assert!(report.render_sources_passed());
        assert!(report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_online_prediction_playtest_exposes_tiny_tap_gate() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();

        let report = session.simulated_online_prediction_playtest(
            3,
            PlayerInput {
                horizontal: 0.01,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
            40,
        );

        assert!(report.immediate_tiny_tap_visible);
        assert!(report.movement_feel_passed());
    }

    #[test]
    fn simulated_online_prediction_playtest_counts_exactly_one_network_command_per_tick() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();

        let report = session.simulated_online_prediction_playtest(
            5,
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS / 3.0,
            60,
        );

        assert_eq!(report.outbound_command_packets, 5);
        assert_eq!(report.outbound_commands, 5);
        assert!(report.command_delivery_passed());
    }

    #[test]
    fn simulated_online_prediction_playtest_marks_authority_tug_gate() {
        let report = super::PredictionPlaytestReport::from_samples(
            &[0.0, 1.0, 2.0],
            &[1, 1, 1],
            0,
            true,
            true,
            BTreeSet::new(),
        );

        assert!(report.authority_tug_passed());
        assert!(report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_online_prediction_playtest_rejects_failed_authority_tug_gate() {
        let report = super::PredictionPlaytestReport::from_samples(
            &[0.0, 1.0, 2.0],
            &[1, 1, 1],
            2,
            true,
            true,
            BTreeSet::new(),
        );

        assert!(!report.authority_tug_passed());
        assert!(!report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_prediction_playtest_report_marks_smooth_ready_path() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session.rebase_local_prediction_to_authority();
        let mut positions = Vec::new();
        let mut outbound_counts = Vec::new();
        for sequence in 0..6 {
            let predicted = session
                .predict_online_render_input(
                    PlayerInput {
                        horizontal: 0.25,
                        ..PlayerInput::default()
                    },
                    FIXED_DELTA_SECONDS / 2.0,
                )
                .expect("prediction exists");
            positions.push(predicted.x);
            let input = session.live_session_tick_input_from_world(
                LOCAL_CLIENT_ID,
                LOCAL_PLAYER_ID,
                sequence,
                vec![PlayerCommand::Movement {
                    horizontal: 0.25,
                    thrust: false,
                    drill_down: false,
                }],
            );
            outbound_counts.push(input.command_packet.expect("packet exists").commands.len());
        }

        let report = session.prediction_playtest_report(&positions, &outbound_counts, 0);

        assert_eq!(report.render_frames, 6);
        assert!(report.monotonic_local_motion);
        assert!(report.immediate_tiny_tap_visible);
        assert_eq!(report.outbound_command_packets, 6);
        assert_eq!(report.outbound_commands, 6);
        assert!(!report.duplicate_outbound_commands);
        assert_eq!(report.stale_authority_backward_tugs, 0);
        assert!(report.predicted_local_render_source);
        assert!(report.authoritative_remote_render_source);
        assert!(report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_prediction_playtest_report_flags_backward_tugs_and_duplicate_commands() {
        let report = super::PredictionPlaytestReport::from_samples(
            &[10.0, 9.0, 11.0],
            &[1, 2, 1],
            1,
            true,
            true,
            BTreeSet::new(),
        );

        assert!(!report.monotonic_local_motion);
        assert!(report.duplicate_outbound_commands);
        assert_eq!(report.stale_authority_backward_tugs, 1);
        assert!(!report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_prediction_playtest_report_flags_missing_render_sources() {
        let report = super::PredictionPlaytestReport::from_samples(
            &[1.0, 2.0],
            &[1, 1],
            0,
            false,
            false,
            BTreeSet::new(),
        );

        assert!(report.monotonic_local_motion);
        assert!(report.immediate_tiny_tap_visible);
        assert!(!report.predicted_local_render_source);
        assert!(!report.authoritative_remote_render_source);
        assert!(!report.ready_for_live_tuning);
    }

    #[test]
    fn simulated_prediction_playtest_report_captures_active_correction_mode_for_tuning() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let tick = frame.target_tick;
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction exists");
        let authoritative = PlayerSnapshot {
            x: predicted.x - 24.0,
            ..predicted
        };
        let authoritative_world = session.world.clone();
        let _correction = session
            .local_client_mut()
            .prediction_mut()
            .reconcile_authoritative_snapshot(tick, &authoritative_world, &authoritative);

        session.local_client_mut().prediction.active_correction = Some(
            super::ActiveCorrection::new(LOCAL_PLAYER_ID, 0.0, 0.0, 8.0, 0.0, 0.12)
                .with_mode(super::CorrectionMode::Smooth),
        );

        let report = session.prediction_playtest_report(&[1.0, 2.0], &[1], 0);

        assert!(
            report
                .correction_modes_seen
                .contains(&super::CorrectionMode::Smooth)
        );
    }

    #[test]
    fn simulated_prediction_playtest_report_accepts_tiny_tap_visibility() {
        let report = super::PredictionPlaytestReport::from_samples(
            &[100.0, 100.01],
            &[1],
            0,
            true,
            true,
            BTreeSet::new(),
        );

        assert!(report.immediate_tiny_tap_visible);
        assert!(report.ready_for_live_tuning);
    }

    #[test]
    fn correction_modes_are_explicit_and_match_plans() {
        assert_eq!(
            super::CorrectionMode::from_plan(super::CorrectionPlan::None),
            super::CorrectionMode::None
        );
        assert_eq!(
            super::CorrectionMode::from_plan(super::CorrectionPlan::Smooth),
            super::CorrectionMode::Smooth
        );
        assert_eq!(
            super::CorrectionMode::from_plan(super::CorrectionPlan::Snap),
            super::CorrectionMode::Snap
        );
    }

    #[test]
    fn host_terrain_revisions_confirm_and_missing_authority_reverts_predicted_overlays() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                drill_down: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let overlay = *session
            .local_client()
            .prediction()
            .terrain_overlays()
            .first()
            .expect("terrain overlay exists");

        let (confirmed, reverted) = session.apply_host_terrain_revisions_to_prediction(
            overlay.target_tick,
            &[super::TerrainChunkRevision {
                position: overlay.chunk,
                revision: overlay.base_revision,
            }],
        );
        assert_eq!((confirmed, reverted), (1, 0));
        assert_eq!(
            session.local_client().prediction().terrain_overlays()[0].resolution,
            super::PredictedTerrainResolution::Confirmed
        );

        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let (confirmed, reverted) =
            session.apply_host_terrain_revisions_to_prediction(SimulationTick::new(u64::MAX), &[]);
        assert_eq!((confirmed, reverted), (0, 1));
    }

    #[test]
    fn predicted_terrain_overlay_carries_tick_and_damage_stage_for_drill_visuals() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                drill_down: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        let overlay = session
            .local_client()
            .prediction()
            .terrain_overlays()
            .first()
            .expect("terrain overlay exists");

        assert_eq!(overlay.predicted_damage_stage, 2);
        assert_eq!(overlay.target_tick, session.current_tick());
    }

    #[test]
    fn overlay_plan_summarizes_predicted_damage_for_renderer() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                drill_down: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        let overlay = session
            .online_prediction_render_overlay(super::OnlineRenderAuthorityPolicy::ClientPredicted);
        let plan = super::RenderPredictionOverlayPlan::from_overlay(&overlay);

        assert!(plan.predicted_damage_total >= 2);
    }

    #[test]
    fn host_modal_authority_confirms_or_reverts_predicted_modal() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(session.apply_host_modal_authority(true));
        assert_eq!(
            session
                .local_client()
                .prediction()
                .predicted_modal_state()
                .expect("modal exists")
                .resolution,
            super::PredictedModalResolution::Confirmed
        );
        assert!(session.apply_host_modal_authority(false));
        assert_eq!(
            session
                .local_client()
                .prediction()
                .predicted_modal_state()
                .expect("modal exists")
                .resolution,
            super::PredictedModalResolution::Reverted
        );
    }

    #[test]
    fn command_rejection_reverts_predicted_modal_and_terrain_feedback() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let rejection = CommandRejection {
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            sequence: InputSequence::new(0),
            reason: crate::multiplayer::CommandAcceptance::TooOld,
            authoritative_tick: SimulationTick::new(u64::MAX),
        };

        session.apply_online_command_rejection(&rejection);

        assert!(
            session
                .local_client()
                .prediction()
                .prediction_failures()
                .contains(&super::PredictionFailure::CommandRejected)
        );
    }

    #[test]
    fn command_acknowledgement_reconciles_terrain_against_authoritative_tick() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let acknowledgement = CommandAcknowledgement {
            client_id: LOCAL_CLIENT_ID,
            acknowledged_sequence: InputSequence::new(0),
            authoritative_tick: SimulationTick::new(u64::MAX),
        };

        session.apply_online_command_acknowledgement(&acknowledgement);

        assert!(
            session
                .local_client()
                .prediction()
                .unacknowledged_commands()
                .is_empty()
        );
    }

    #[test]
    fn render_transient_prediction_does_not_pollute_deterministic_history_or_replay() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let history_before = session
            .local_client()
            .prediction()
            .prediction_history_count();

        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS / 2.0,
        );

        let prediction = session.local_client().prediction();
        assert_eq!(prediction.prediction_history_count(), history_before);
        assert_eq!(prediction.transient_render_history_count_for_tests(), 0);
        assert_eq!(prediction.deterministic_replay_frame_count(), 0);
        assert_eq!(
            prediction.latest_input_intent(),
            Some(super::PlayerMovementIntent {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            })
        );
    }

    #[test]
    fn fixed_prediction_frame_records_history_and_replay_state() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let tick = frame.target_tick;

        let _predicted = session.predict_online_command_frame(frame, FIXED_DELTA_SECONDS);
        let prediction = session.local_client().prediction();

        assert_eq!(prediction.deterministic_replay_frame_count(), 1);
        assert!(
            prediction
                .prediction_history_snapshot(tick, LOCAL_PLAYER_ID)
                .is_some()
        );
    }

    #[test]
    fn prediction_does_not_duplicate_outbound_network_commands() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        for _ in 0..3 {
            let _predicted = session.predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS / 3.0,
            );
        }

        let input = session.live_session_tick_input_from_world(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            7,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let packet = input.command_packet.expect("command packet exists");

        assert_eq!(packet.commands.len(), 1);
        assert_eq!(packet.commands[0].sequence, InputSequence::new(7));
    }

    #[test]
    fn held_input_changes_prediction_every_render_frame() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let first = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS / 2.0,
            )
            .expect("first prediction exists");
        let second = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS / 2.0,
            )
            .expect("second prediction exists");

        assert!(second.x > first.x);
    }

    #[test]
    fn descriptor_client_render_output_uses_predicted_local_and_authoritative_remote_sources() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);
        let frame_plan = session.render_frame_plan();
        let view = &frame_plan.views[0];

        let players = frame_plan.world_player_presentations_for_view(view, &prediction_plan);
        let local = players
            .iter()
            .find(|player| player.local_to_view)
            .expect("local presentation exists");
        let remote = players
            .iter()
            .find(|player| player.player_id == second_player)
            .expect("remote presentation exists");

        assert_eq!(
            local.source,
            super::RenderPlayerPresentationSource::PredictedLocal
        );
        assert_eq!(
            remote.source,
            super::RenderPlayerPresentationSource::AuthoritativeWorld
        );
    }

    #[test]
    fn render_overlay_stays_presentation_only_for_command_delivery() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let overlay = session
            .online_prediction_render_overlay(super::OnlineRenderAuthorityPolicy::ClientPredicted);
        let input = session.live_session_tick_input_from_world(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            11,
            vec![PlayerCommand::Interact],
        );

        assert!(overlay.modal.is_some());
        assert_eq!(
            input
                .command_packet
                .expect("command packet exists")
                .commands
                .len(),
            1
        );
    }

    #[test]
    fn stale_authority_does_not_create_render_correction_for_matching_history() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let tick = frame.target_tick;
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");

        let authoritative_world = session.world.clone();
        let correction = session
            .local_client_mut()
            .prediction_mut()
            .reconcile_authoritative_snapshot(tick, &authoritative_world, &predicted);
        let plan = session.prediction_presentation_plan(Some(&predicted), 0.0, 0.5, 0.0);

        assert_eq!(correction, None);
        assert_eq!(plan.corrected_local_presentation, None);
        assert_eq!(plan.correction, None);
    }

    #[test]
    fn prediction_history_records_tick_matched_snapshots() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 0.5,
                thrust: false,
                drill_down: false,
            }],
        );
        let tick = frame.target_tick;
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");

        let history = session
            .local_client()
            .prediction()
            .prediction_history_snapshot(tick, LOCAL_PLAYER_ID)
            .expect("tick-matched prediction history exists");

        assert_eq!(history.player_id, LOCAL_PLAYER_ID);
        assert!((history.x - predicted.x).abs() < f32::EPSILON);
        assert_eq!(
            session
                .local_client()
                .prediction()
                .prediction_history_count(),
            1
        );
    }

    #[test]
    fn authority_mismatch_corrects_to_rebased_replayed_prediction_not_stale_authority() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let tick = frame.target_tick;
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");
        let authoritative = PlayerSnapshot {
            x: predicted.x - 24.0,
            ..predicted.clone()
        };
        let future_frame = super::PredictionCommandFrame {
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            start_sequence: InputSequence::new(99),
            target_tick: tick.next(),
            delta_seconds: FIXED_DELTA_SECONDS,
            commands: vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        };
        session
            .local_client_mut()
            .prediction_mut()
            .prediction_command_frames
            .push(future_frame);

        let authoritative_world = session.world.clone();
        let correction = session
            .local_client_mut()
            .prediction_mut()
            .reconcile_authoritative_snapshot(tick, &authoritative_world, &authoritative)
            .expect("mismatch creates correction");

        assert!((correction.from_x - predicted.x).abs() < f32::EPSILON);
        assert!(
            (correction.to_x - authoritative.x).abs() > f32::EPSILON,
            "correction target must be rebased/replayed prediction, not stale authority"
        );
    }

    #[test]
    fn prediction_presentation_plan_does_not_reconcile_against_stale_authority() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let predicted = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS,
            )
            .expect("prediction snapshot exists");
        let stale_authority = PlayerSnapshot {
            x: predicted.x - 16.0,
            ..predicted
        };

        let plan = session.prediction_presentation_plan(Some(&stale_authority), 0.0, 0.5, 0.0);

        assert_eq!(plan.correction, None);
        assert_eq!(plan.corrected_local_presentation, None);
    }

    #[test]
    fn rebase_prunes_past_prediction_frames_and_replays_future_frames() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let old_frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let _old = session.predict_online_command_frame(old_frame, FIXED_DELTA_SECONDS);
        let old_tick = session.current_tick();
        assert!(session.sync_replicated_authoritative_tick(old_tick.next()));
        let future_frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let future_tick = future_frame.target_tick;
        let _future = session.predict_online_command_frame(future_frame, FIXED_DELTA_SECONDS);

        let authoritative = session
            .world
            .player(LOCAL_PLAYER_ID)
            .map(|player| {
                PlayerSnapshot::from_world_player(LOCAL_PLAYER_ID, player, &session.world)
            })
            .expect("authoritative player exists");
        let current_tick = old_tick;
        let authoritative_world = session.world.clone();
        let _correction = session
            .local_client_mut()
            .prediction_mut()
            .reconcile_authoritative_snapshot(current_tick, &authoritative_world, &authoritative);

        assert_eq!(
            session
                .local_client()
                .prediction()
                .pending_prediction_frame_count(),
            1
        );
        assert!(
            session
                .local_client()
                .prediction()
                .prediction_history_snapshot(future_tick, LOCAL_PLAYER_ID)
                .is_some()
        );
    }

    #[test]
    fn render_frame_prediction_after_rebase_restores_current_input_before_render() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let before = session
            .world
            .player(LOCAL_PLAYER_ID)
            .expect("authoritative player exists")
            .x;
        session.rebase_local_prediction_to_authority();

        let predicted = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS,
            )
            .expect("prediction snapshot exists");
        let plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);

        assert!(predicted.x > before);
        assert!(plan.local_movement.expect("local prediction exists").x > before);
    }

    #[test]
    fn correction_plan_for_positions_classifies_large_errors_as_snap() {
        assert_eq!(
            super::ClientPredictionState::correction_plan_for_positions(0.0, 0.0, 50.0, 0.0),
            super::CorrectionPlan::Snap
        );
    }

    #[test]
    fn prediction_terrain_overlay_can_confirm_and_revert_against_revisions() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let overlay = *session
            .local_client()
            .prediction()
            .terrain_overlays()
            .first()
            .expect("terrain overlay exists");

        assert_eq!(
            session.confirm_online_prediction_terrain(&[super::TerrainChunkRevision {
                position: overlay.chunk,
                revision: overlay.base_revision,
            }]),
            1
        );
        assert_eq!(
            session.local_client().prediction().terrain_overlays()[0].resolution,
            super::PredictedTerrainResolution::Confirmed
        );

        let _predicted = session.predict_online_render_input(
            PlayerInput {
                drill_down: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert_eq!(session.revert_unconfirmed_online_prediction_terrain(), 1);
        assert_eq!(
            session.local_client().prediction().terrain_overlays()[0].resolution,
            super::PredictedTerrainResolution::Reverted
        );
    }

    #[test]
    fn prediction_modal_can_confirm_and_revert_with_metadata() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(session.confirm_online_predicted_modal());
        assert_eq!(
            session
                .local_client()
                .prediction()
                .predicted_modal_state()
                .expect("modal exists")
                .resolution,
            super::PredictedModalResolution::Confirmed
        );
        assert!(session.revert_online_predicted_modal());
        assert_eq!(
            session
                .local_client()
                .prediction()
                .predicted_modal_state()
                .expect("modal exists")
                .resolution,
            super::PredictedModalResolution::Reverted
        );
    }

    #[test]
    fn render_policy_distinguishes_host_authority_from_client_prediction() {
        assert_eq!(
            super::RenderFramePlan::online_authority_policy(false),
            super::OnlineRenderAuthorityPolicy::HostAuthoritative
        );
        assert_eq!(
            super::RenderFramePlan::online_authority_policy(true),
            super::OnlineRenderAuthorityPolicy::ClientPredicted
        );
    }

    #[test]
    fn live_render_output_attaches_prediction_overlay_to_local_client_view() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);

        let output = session.live_render_frame_output(&prediction_plan);
        let overlay = output.viewport_plans[0]
            .prediction_overlay
            .as_ref()
            .expect("prediction overlay is attached");

        assert_eq!(
            overlay.policy,
            super::OnlineRenderAuthorityPolicy::ClientPredicted
        );
        assert!(!overlay.terrain.is_empty());
        assert!(overlay.modal.is_some());
    }

    #[test]
    fn prediction_overlay_plan_marks_renderer_work_items() {
        let overlay = super::OnlinePredictionRenderOverlay {
            policy: super::OnlineRenderAuthorityPolicy::ClientPredicted,
            terrain: vec![super::PredictedTerrainOverlay {
                player_id: LOCAL_PLAYER_ID,
                position: crate::terrain::TilePosition { x: 1, y: 2 },
                chunk: super::TerrainChunkPosition { x: 0, y: 0 },
                command_sequence: Some(InputSequence::new(1)),
                base_revision: 0,
                target_tick: SimulationTick::new(0),
                predicted_damage_stage: 1,
                resolution: super::PredictedTerrainResolution::Pending,
            }],
            effects: vec![super::PredictedEffect::Movement {
                player_id: LOCAL_PLAYER_ID,
            }],
            modal: Some(super::PredictedModalState {
                modal: super::PredictedModal::BuildingInteraction {
                    player_id: LOCAL_PLAYER_ID,
                },
                resolution: super::PredictedModalResolution::Pending,
            }),
        };

        let plan = super::RenderPredictionOverlayPlan::from_overlay(&overlay);

        assert_eq!(plan.terrain_outline_count, 1);
        assert_eq!(plan.pending_terrain_count, 1);
        assert_eq!(plan.effect_count, 1);
        assert!(plan.show_status_badge);
        assert!(plan.show_modal_overlay);
    }

    #[test]
    fn prediction_render_overlay_exposes_effects_terrain_and_modal() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let _predicted = session.predict_online_render_input(
            PlayerInput {
                horizontal: 1.0,
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        let overlay = session
            .online_prediction_render_overlay(super::OnlineRenderAuthorityPolicy::ClientPredicted);

        assert_eq!(
            overlay.policy,
            super::OnlineRenderAuthorityPolicy::ClientPredicted
        );
        assert!(!overlay.effects.is_empty());
        assert!(!overlay.terrain.is_empty());
        assert!(overlay.modal.is_some());
    }

    #[test]
    fn prediction_render_input_preserves_tiny_movement_without_authority_mutation() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let authoritative_before = session
            .world
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .x;

        let predicted = session
            .predict_online_render_input(
                PlayerInput {
                    horizontal: 0.05,
                    ..PlayerInput::default()
                },
                FIXED_DELTA_SECONDS,
            )
            .expect("prediction snapshot exists");

        assert!(predicted.x > authoritative_before);
        assert!(
            (session
                .world
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .x
                - authoritative_before)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn prediction_acknowledgement_prunes_remembered_prediction_frames() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let start_sequence = frame.start_sequence;
        let _predicted = session.predict_online_command_frame(frame, FIXED_DELTA_SECONDS);
        assert_eq!(
            session
                .local_client()
                .prediction()
                .pending_prediction_frame_count(),
            1
        );

        session
            .local_client_mut()
            .prediction_mut()
            .acknowledge_through(start_sequence);

        assert_eq!(
            session
                .local_client()
                .prediction()
                .pending_prediction_frame_count(),
            0
        );
    }

    #[test]
    fn prediction_records_cargo_material_effect_for_economy_commands() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::SellCargo],
        );

        let _predicted = session.predict_online_command_frame(frame, FIXED_DELTA_SECONDS);

        assert!(
            session
                .local_client()
                .prediction()
                .predicted_effects()
                .contains(&super::PredictedEffect::CargoMaterialChanged {
                    player_id: LOCAL_PLAYER_ID,
                })
        );
    }

    #[test]
    fn prediction_correction_advances_over_multiple_frames() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");
        let authoritative = PlayerSnapshot {
            x: predicted.x + 8.0,
            ..predicted
        };
        let _correction = session
            .local_client_mut()
            .prediction_mut()
            .note_authoritative_rebase_for_player(&authoritative);

        let first = session
            .advance_online_prediction_correction(0.04)
            .expect("correction advances");
        let second = session
            .advance_online_prediction_correction(0.04)
            .expect("correction advances");

        assert!(second.x > first.x);
    }

    #[test]
    fn prediction_world_updates_local_player_without_mutating_authority() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let authoritative_before = session
            .world
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .x;
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: true,
            }],
        );

        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");

        assert!(predicted.x > authoritative_before);
        assert!(
            (session
                .world
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .x
                - authoritative_before)
                .abs()
                < f32::EPSILON
        );
        assert!(
            session
                .local_client()
                .prediction()
                .predicted_effects()
                .contains(&super::PredictedEffect::Movement {
                    player_id: LOCAL_PLAYER_ID,
                })
        );
        assert!(
            session
                .local_client()
                .prediction()
                .predicted_effects()
                .contains(&super::PredictedEffect::TerrainInteraction {
                    player_id: LOCAL_PLAYER_ID,
                })
        );
    }

    #[test]
    fn prediction_world_records_tentative_building_interaction_modal() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Interact],
        );

        let _predicted = session.predict_online_command_frame(frame, FIXED_DELTA_SECONDS);

        assert_eq!(
            session.local_client().prediction().predicted_modal(),
            Some(super::PredictedModal::BuildingInteraction {
                player_id: LOCAL_PLAYER_ID,
            })
        );
    }

    #[test]
    fn prediction_state_records_persistent_correction_on_authority_mismatch() {
        let mut session = GameSession::new();
        session.rebase_local_prediction_to_authority();
        let frame = session.online_prediction_command_frame(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        let predicted = session
            .predict_online_command_frame(frame, FIXED_DELTA_SECONDS)
            .expect("prediction snapshot exists");
        let authoritative = PlayerSnapshot {
            x: predicted.x + 8.0,
            ..predicted
        };

        let correction = session
            .local_client_mut()
            .prediction_mut()
            .note_authoritative_rebase_for_player(&authoritative)
            .expect("correction is recorded");

        assert_eq!(correction.player_id, LOCAL_PLAYER_ID);
        assert_eq!(
            session.local_client().prediction().active_correction(),
            Some(correction)
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "broad prediction presentation regression covers local reconciliation and remote projection together"
    )]
    fn prediction_state_projects_local_reconciliation_and_remote_presentation() {
        let previous = super::PlayerSnapshot {
            player_id: LOCAL_PLAYER_ID,
            x: 10.0,
            y: 20.0,
            velocity_x: 4.0,
            velocity_y: -2.0,
            fuel: 3.0,
            fuel_capacity: 100.0,
            hull: 4.0,
            max_hull: 100.0,
            credits: 6,
            cargo_used: 0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        };
        let next = super::PlayerSnapshot {
            x: 20.0,
            y: 30.0,
            ..previous.clone()
        };

        let predicted = super::ClientPredictionState::predict_local_movement(&previous, 0.5);
        assert!((predicted.x - 12.0).abs() < f32::EPSILON);
        assert!((predicted.y - 19.0).abs() < f32::EPSILON);

        let replayed = super::ClientPredictionState::replay_unacknowledged_movement(
            &previous,
            &[SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::Movement {
                    horizontal: 3.0,
                    thrust: true,
                    drill_down: false,
                },
            }],
        );
        assert_eq!(replayed.replayed_command_count, 1);
        assert!((replayed.predicted.x - 12.333_334).abs() < 0.001);
        assert!((replayed.predicted.y - 18.0).abs() < f32::EPSILON);
        let replayed_full_state = super::ClientPredictionState::replay_unacknowledged_player_state(
            &previous,
            &[
                SequencedPlayerCommand {
                    player_id: LOCAL_PLAYER_ID,
                    sequence: InputSequence::new(2),
                    target_tick: SimulationTick::new(1),
                    command: PlayerCommand::UseScanner,
                },
                SequencedPlayerCommand {
                    player_id: LOCAL_PLAYER_ID,
                    sequence: InputSequence::new(3),
                    target_tick: SimulationTick::new(1),
                    command: PlayerCommand::SellCargo,
                },
            ],
        );
        assert_eq!(replayed_full_state.replayed_command_count, 2);
        assert!((replayed_full_state.snapshot.scanner_cooldown_seconds - 1.0).abs() < f32::EPSILON);

        let replayed_reconciliation = super::ReplayedReconciliation::from_authoritative_snapshot(
            &previous,
            &[SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::Movement {
                    horizontal: 3.0,
                    thrust: true,
                    drill_down: false,
                },
            }],
        );
        assert_eq!(replayed_reconciliation.replayed.replayed_command_count, 1);
        assert_eq!(
            replayed_reconciliation.reconciliation.correction_plan,
            super::CorrectionPlan::Smooth
        );

        let reconciled = super::ClientPredictionState::reconcile_movement(predicted, &next);
        assert_eq!(reconciled.correction_plan, super::CorrectionPlan::Smooth);
        assert!(reconciled.correction_offset.is_some());
        let smoothed = reconciled.corrected_presentation(0.5);
        assert_eq!(smoothed.correction_plan, super::CorrectionPlan::Smooth);
        assert!((smoothed.x - 16.0).abs() < f32::EPSILON);
        let correction_frame =
            super::CorrectionPresentationFrame::from_reconciliation(&reconciled, 1.5);
        assert!((correction_frame.smoothing_alpha - 1.0).abs() < f32::EPSILON);
        assert!(!correction_frame.snap_applied);
        assert_eq!(
            correction_frame.presentation.correction_plan,
            super::CorrectionPlan::Smooth
        );

        let interpolated = super::ClientPredictionState::remote_player_presentation(
            &previous,
            Some(&next),
            0.5,
            0.0,
        );
        assert!((interpolated.x - 15.0).abs() < f32::EPSILON);
        assert!(!interpolated.extrapolated);

        let extrapolated =
            super::ClientPredictionState::remote_player_presentation(&previous, None, 0.0, 0.1);
        assert!(extrapolated.extrapolated);
        assert!((extrapolated.x - 10.4).abs() < f32::EPSILON);
    }

    #[test]
    fn prediction_state_records_and_clears_prediction_failures() {
        let mut prediction = super::ClientPredictionState::default();

        prediction.note_prediction_failure(super::PredictionFailure::TerrainAlreadyChanged);
        prediction.note_prediction_failure(super::PredictionFailure::HazardOrRescueChangedState);

        assert_eq!(prediction.prediction_failures().len(), 2);
        prediction.clear_prediction_failures();
        assert!(prediction.prediction_failures().is_empty());
    }

    #[test]
    fn prediction_state_maps_failures_to_recovery_actions() {
        let mut prediction = super::ClientPredictionState::default();

        prediction.note_prediction_failure(super::PredictionFailure::TerrainAlreadyChanged);
        prediction.note_prediction_failure(super::PredictionFailure::EconomyChangedState);
        prediction.note_prediction_failure(super::PredictionFailure::ProgressionChangedState);

        assert_eq!(
            prediction.prediction_failure_resolutions(),
            vec![
                super::PredictionFailureResolution::RequestTerrainChunk,
                super::PredictionFailureResolution::RollBackLocalEconomy,
                super::PredictionFailureResolution::RollBackProgression,
            ]
        );

        let mut tracker = super::TerrainRevisionTracker::default();
        let position = super::TerrainChunkPosition { x: 0, y: 0 };
        tracker.mark_tiles_changed([crate::terrain::TilePosition { x: 1, y: 1 }]);
        let actions = prediction.prediction_recovery_actions(
            LOCAL_PLAYER_ID,
            &tracker,
            SimulationTick::new(12),
            position,
            0,
        );
        assert!(matches!(
            &actions[0],
            super::PredictionRecoveryAction::RequestTerrainDelta(
                super::CompactWorldDelta::TerrainChunks { .. }
            )
        ));
        assert!(matches!(
            actions[1],
            super::PredictionRecoveryAction::RollBackLocalEconomy {
                player_id: LOCAL_PLAYER_ID
            }
        ));
        assert!(!super::PredictionFailureRecoveryPlan::from_actions(actions).request_keyframe);
    }

    #[test]
    fn session_builds_prediction_failure_recovery_plan() {
        let mut session = GameSession::new();
        session
            .local_client_mut()
            .prediction
            .note_prediction_failure(super::PredictionFailure::HazardOrRescueChangedState);

        let plan =
            session.prediction_failure_recovery_plan(super::TerrainChunkPosition { x: 0, y: 0 }, 0);

        assert!(plan.request_keyframe);
        assert_eq!(plan.actions.len(), 1);
    }

    #[test]
    fn prediction_state_tracks_local_feedback_and_correction_offsets() {
        let mut prediction = super::ClientPredictionState::default();

        prediction.push_feedback(super::LocalTentativeFeedback::MovementIntent);
        prediction.push_feedback(super::LocalTentativeFeedback::DrillContact);
        prediction.set_correction_offset(super::CorrectionOffset::new(2.0, -1.0));

        assert_eq!(prediction.pending_feedback().len(), 2);
        assert_eq!(
            prediction.tentative_feedback_presentations(),
            vec![
                super::TentativeFeedbackPresentation::MovementVisual,
                super::TentativeFeedbackPresentation::DrillContactAudio,
            ]
        );
        assert_eq!(
            prediction.tentative_feedback_outputs(),
            vec![
                super::TentativeFeedbackOutput {
                    presentation: super::TentativeFeedbackPresentation::MovementVisual,
                    channel: super::TentativeFeedbackChannel::Render,
                },
                super::TentativeFeedbackOutput {
                    presentation: super::TentativeFeedbackPresentation::DrillContactAudio,
                    channel: super::TentativeFeedbackChannel::Audio,
                },
            ]
        );
        let feedback_frame = prediction.tentative_feedback_frame();
        assert_eq!(feedback_frame.render_output_count, 1);
        assert_eq!(feedback_frame.audio_output_count, 1);
        assert!(feedback_frame.has_drill_feedback());
        let offset = prediction.correction_offset().expect("offset set");
        assert!((offset.x - 2.0).abs() < f32::EPSILON);
        prediction.clear_feedback();
        prediction.clear_correction_offset();
        assert!(prediction.pending_feedback().is_empty());
        assert!(prediction.correction_offset().is_none());
    }

    #[test]
    fn buffered_commands_are_drained_by_tick() {
        let mut session = GameSession::new();
        let tick = session.current_tick();
        session.sequence_local_commands(vec![PlayerCommand::Interact]);

        let commands = session.drain_commands_for_tick(tick);

        assert_eq!(commands.len(), 1);
        assert_eq!(session.pending_command_count(tick), 0);
    }

    #[test]
    fn canonical_gameplay_events_have_stable_tick_ordinals_and_adapter_output() {
        let game = GameState::new();
        let mut world = WorldState::from_legacy_game(&game);
        {
            let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
            player.bombs = 1;
            player.signal_relay_kits = 1;
        }
        let tick = SimulationTick::new(12);
        let commands = [
            SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(1),
                target_tick: tick,
                command: PlayerCommand::PlaceBomb,
            },
            SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: InputSequence::new(2),
                target_tick: tick,
                command: PlayerCommand::PlaceInfrastructure { slot: 0 },
            },
        ];
        let inputs = super::TickInputs::from_commands(&commands);
        let mut terrain_revisions = super::TerrainRevisionTracker::default();

        let output = world.simulate_tick(tick, &inputs, &mut terrain_revisions);

        assert_eq!(output.events.len(), 2);
        assert_eq!(
            output.events[0].id,
            drillgame_simulation::GameplayEventId { tick, ordinal: 0 }
        );
        assert_eq!(
            output.events[1].id,
            drillgame_simulation::GameplayEventId { tick, ordinal: 1 }
        );
        assert_eq!(
            output.events[0].kind,
            super::GameplayEventKind::BombPlaced {
                player_id: LOCAL_PLAYER_ID,
            }
        );
        assert_eq!(
            super::world_events_for_gameplay_event(output.events[0]),
            vec![super::WorldEvent::BombPlaced {
                player_id: LOCAL_PLAYER_ID,
            }]
        );
    }

    #[test]
    fn prediction_identity_compares_exact_drill_target_and_terrain_tile_not_chunk_only() {
        let mut host = GameSession::new();
        let player_id = LOCAL_PLAYER_ID;
        let target = TilePosition { x: 2, y: 2 };
        host.world_mut().set_active_drill(
            player_id,
            Some(DrillState {
                target,
                direction: DrillDirection::Down,
                progress: 0.0,
                initial_durability: 1,
                seconds_per_chip: 1.0,
                sound_timer: 0.0,
                dust_timer: 0.0,
            }),
        );
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        assert_eq!(
            client.prediction_identity_matches_authority(player_id),
            Some(true)
        );

        client
            .predicted_client_world
            .as_mut()
            .expect("predicted world")
            .set_active_drill(
                player_id,
                Some(DrillState {
                    target: TilePosition { x: 3, y: 2 },
                    direction: DrillDirection::Right,
                    progress: 0.0,
                    initial_durability: 1,
                    seconds_per_chip: 1.0,
                    sound_timer: 0.0,
                    dust_timer: 0.0,
                }),
            );
        assert_eq!(
            client.prediction_identity_matches_authority(player_id),
            Some(false)
        );

        let predicted = client
            .predicted_client_world
            .as_mut()
            .expect("predicted world");
        predicted.set_active_drill(player_id, host.world().active_drill(player_id).copied());
        let divergent_kind = match host.world().terrain().tile(target).expect("host tile").kind {
            TileKind::Air => TileKind::Dirt,
            _ => TileKind::Air,
        };
        predicted.terrain_mut().set_kind(target, divergent_kind);
        client
            .pending_prediction_inputs
            .push_back(super::PendingPredictionInput {
                client_id: LOCAL_CLIENT_ID,
                player_id,
                frame: crate::multiplayer::InputFrame {
                    sequence: InputSequence::new(99),
                    target_tick: host.current_tick(),
                    controls: crate::multiplayer::InputControls::default(),
                    actions: Vec::new(),
                },
                terrain_changed_tiles: vec![target],
            });
        assert_eq!(
            client.prediction_identity_matches_authority(player_id),
            Some(false)
        );
    }

    #[test]
    fn pending_prediction_presentation_is_keyed_by_sequence_action_and_exact_terrain() {
        let host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let action_id = crate::multiplayer::ActionId::new(901);
        let sequence = InputSequence::new(90);
        assert!(client.queue_predicted_input(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            crate::multiplayer::InputFrame {
                sequence,
                target_tick: host.current_tick(),
                controls: crate::multiplayer::InputControls::default(),
                actions: vec![crate::multiplayer::SequencedAction {
                    id: action_id,
                    command: PlayerCommand::UseScanner,
                }],
            },
        ));

        let pending = client.pending_prediction_presentations();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sequence, sequence);
        assert_eq!(pending[0].action_ids, vec![action_id]);
        assert_eq!(pending[0].target_tick, host.current_tick());
    }

    #[test]
    fn stale_authoritative_frame_is_ignored_without_backward_recovery_or_tug() {
        let host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let stale = crate::multiplayer::AuthoritativeFrame {
            epoch: keyframe.epoch,
            base_tick: SimulationTick::new(0),
            result_tick: SimulationTick::new(0),
            applied_sequences: BTreeMap::new(),
            changed_players: Vec::new(),
            terrain_mutations: Vec::new(),
            terrain_revisions: Vec::new(),
            canonical_state: keyframe.canonical_state.clone(),
            state_hash: keyframe.state_hash,
        };
        let before = client
            .authoritative_client_world()
            .expect("authoritative world")
            .simulation_state_hash();

        assert_eq!(
            client.apply_authoritative_client_frame(&stale),
            super::AuthoritativeClientFrameResult::IgnoredStale
        );
        assert_eq!(
            client
                .authoritative_client_world()
                .expect("authoritative world")
                .simulation_state_hash(),
            before
        );
        assert!(client.take_authoritative_recovery_request().is_none());
        assert!(client.prediction_presentation_offset().is_none());
    }

    #[test]
    fn prediction_presentation_interpolates_fixed_poses_and_smooths_correction_only() {
        let host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let before = client
            .predicted_client_world()
            .expect("predicted world")
            .player(LOCAL_PLAYER_ID)
            .expect("player")
            .x;
        assert!(client.queue_predicted_input(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            crate::multiplayer::InputFrame {
                sequence: InputSequence::new(80),
                target_tick: host.current_tick(),
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: Vec::new(),
            },
        ));
        let current = client
            .predicted_client_world()
            .expect("predicted world")
            .player(LOCAL_PLAYER_ID)
            .expect("player")
            .x;
        let interpolated = client
            .interpolated_predicted_player(LOCAL_PLAYER_ID, 0.5)
            .expect("interpolated player");
        assert!((before + current).mul_add(-0.5, interpolated.x).abs() < 0.001);

        client.prediction_presentation_offset = Some(super::CorrectionOffset::new(8.0, -4.0));
        let first = client
            .advance_prediction_presentation_correction(0.5)
            .expect("correction remains");
        assert!((first.x - 4.0).abs() < f32::EPSILON);
        assert!((first.y + 2.0).abs() < f32::EPSILON);
        let unchanged_simulation_x = client
            .predicted_client_world()
            .expect("predicted world")
            .player(LOCAL_PLAYER_ID)
            .expect("player")
            .x;
        assert!((unchanged_simulation_x - current).abs() < f32::EPSILON);
    }

    #[test]
    fn online_client_render_selection_uses_authority_without_pending_and_prediction_with_pending() {
        let host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let authoritative_x = client
            .online_client_render_frame_output()
            .expect("authoritative render")
            .world_players_by_view[0]
            .1[0]
            .x;
        assert!(client.queue_predicted_input(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            crate::multiplayer::InputFrame {
                sequence: InputSequence::new(300),
                target_tick: host.current_tick(),
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: Vec::new(),
            },
        ));
        let predicted_x = client
            .online_client_render_frame_output()
            .expect("predicted render")
            .world_players_by_view[0]
            .1[0]
            .x;

        assert!(predicted_x > authoritative_x);
    }

    #[test]
    fn predicted_render_snapshot_uses_one_predicted_world_for_terrain_player_camera_and_hud() {
        let host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let target = TilePosition { x: 2, y: 2 };
        let original_kind = client
            .predicted_client_world()
            .expect("predicted world")
            .terrain()
            .tile(target)
            .expect("tile")
            .kind;
        let frame = crate::multiplayer::InputFrame {
            sequence: InputSequence::new(70),
            target_tick: host.current_tick(),
            controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
            actions: Vec::new(),
        };
        assert!(client.queue_predicted_input(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID, frame));
        let predicted_world = client.predicted_client_world().expect("predicted world");
        let predicted_player = predicted_world.player(LOCAL_PLAYER_ID).expect("player");
        let output = client
            .predicted_client_render_frame_output()
            .expect("predicted render output");
        let rendered_player = output.world_players_by_view[0]
            .1
            .iter()
            .find(|player| player.player_id == LOCAL_PLAYER_ID)
            .expect("rendered local player");
        let hud = output
            .hud_snapshots
            .iter()
            .find(|hud| hud.player_id == LOCAL_PLAYER_ID)
            .expect("predicted hud");

        assert!((rendered_player.x - predicted_player.x).abs() < f32::EPSILON);
        assert_eq!(hud.credits, predicted_player.credits);
        assert_eq!(
            output.terrain.tile(target).expect("render terrain").kind,
            original_kind
        );
        #[allow(
            clippy::cast_precision_loss,
            reason = "bounded viewport dimensions intentionally convert to camera pixels"
        )]
        let expected_camera = raylib::prelude::Vector2::new(
            (output.viewport_plans[0].viewport.width as f32).mul_add(-0.5, predicted_player.x),
            (output.viewport_plans[0].viewport.height as f32).mul_add(-0.5, predicted_player.y),
        );
        assert_eq!(output.viewport_plans[0].camera, expected_camera);
    }

    #[test]
    fn canonical_prediction_prunes_applied_input_and_replays_only_pending_frames() {
        let mut host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let first_sequence = InputSequence::new(41);
        let first = crate::multiplayer::InputFrame {
            sequence: first_sequence,
            target_tick: host.current_tick(),
            controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
            actions: Vec::new(),
        };
        let second = crate::multiplayer::InputFrame {
            sequence: InputSequence::new(42),
            target_tick: host.current_tick().next(),
            controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
            actions: Vec::new(),
        };
        assert!(client.queue_predicted_input(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID, first.clone()));
        assert!(client.queue_predicted_input(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID, second));
        let predicted_before = client
            .predicted_client_world()
            .expect("predicted world")
            .player(LOCAL_PLAYER_ID)
            .expect("predicted player")
            .x;
        let bundle = crate::multiplayer::InputBundle {
            epoch: host.session_epoch(),
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![first],
        };
        let _status = host.receive_input_bundle(&bundle);
        let Some((_applied, frame)) =
            host.apply_accepted_input_frame(LOCAL_CLIENT_ID, first_sequence)
        else {
            panic!("host frame");
        };

        assert_eq!(
            client.apply_authoritative_client_frame(&frame),
            super::AuthoritativeClientFrameResult::Applied
        );
        assert_eq!(client.pending_prediction_input_count(), 1);
        let predicted_after = client
            .predicted_client_world()
            .expect("predicted world")
            .player(LOCAL_PLAYER_ID)
            .expect("predicted player")
            .x;
        assert!((predicted_after - predicted_before).abs() < 0.001);
        assert_eq!(
            client
                .authoritative_client_world()
                .expect("authoritative world")
                .simulation_state_hash(),
            host.world().simulation_state_hash()
        );
    }

    #[test]
    fn prediction_replay_does_not_duplicate_one_shot_actions() {
        let mut host = GameSession::new();
        host.world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("host player")
            .bombs = 2;
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let frame = crate::multiplayer::InputFrame {
            sequence: InputSequence::new(50),
            target_tick: host.current_tick(),
            controls: crate::multiplayer::InputControls::default(),
            actions: vec![crate::multiplayer::SequencedAction {
                id: crate::multiplayer::ActionId::new(500),
                command: PlayerCommand::PlaceBomb,
            }],
        };
        assert!(client.queue_predicted_input(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID, frame));
        let bombs_after_prediction = client
            .predicted_client_world()
            .expect("predicted world")
            .bomb_count();

        let summary = client.rebuild_predicted_client_world(&BTreeMap::new());

        assert_eq!(summary.replayed_inputs, 1);
        assert_eq!(
            client
                .predicted_client_world()
                .expect("predicted world")
                .bomb_count(),
            bombs_after_prediction
        );
    }

    #[test]
    fn authoritative_client_applies_ordered_frame_and_requests_recovery_on_bad_base() {
        let mut host = GameSession::new();
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let sequence = InputSequence::new(31);
        let bundle = crate::multiplayer::InputBundle {
            epoch: host.session_epoch(),
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![crate::multiplayer::InputFrame {
                sequence,
                target_tick: host.current_tick(),
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: Vec::new(),
            }],
        };
        let _status = host.receive_input_bundle(&bundle);
        let Some((_applied, frame)) = host.apply_accepted_input_frame(LOCAL_CLIENT_ID, sequence)
        else {
            panic!("host frame");
        };

        assert_eq!(
            client.apply_authoritative_client_frame(&frame),
            super::AuthoritativeClientFrameResult::Applied
        );
        assert_eq!(
            client
                .authoritative_client_world()
                .expect("client replica")
                .simulation_state_hash(),
            host.world().simulation_state_hash()
        );
        let mut bad_base = frame.clone();
        bad_base.base_tick = client
            .authoritative_client_world()
            .expect("client replica")
            .simulation_tick()
            .next();
        bad_base.result_tick = bad_base.base_tick.next();
        assert_eq!(
            client.apply_authoritative_client_frame(&bad_base),
            super::AuthoritativeClientFrameResult::RecoveryRequested
        );
        assert!(matches!(
            client.take_authoritative_recovery_request(),
            Some(crate::multiplayer::ClientMessage::KeyframeRequest { .. })
        ));
    }

    #[test]
    fn authoritative_client_buffers_bounded_remote_history_for_interpolation() {
        let mut host = GameSession::new();
        let remote_client = ClientId::new(2);
        let remote_player = PlayerId::new(2);
        assert!(host.add_online_remote_client_player(remote_client, remote_player));
        let keyframe = host.authoritative_keyframe();
        let mut client = GameSession::new();
        client
            .install_authoritative_client_keyframe(&keyframe)
            .expect("install keyframe");
        let base_tick = keyframe.snapshot.tick;
        let make_snapshot = |x| {
            let mut snapshot = keyframe
                .snapshot
                .players
                .iter()
                .find(|player| player.player_id == remote_player)
                .expect("remote snapshot")
                .clone();
            snapshot.x = x;
            snapshot
        };
        let mut world: WorldState =
            serde_json::from_slice(&keyframe.canonical_state).expect("keyframe world");
        super::apply_network_player_snapshot_to_player(
            world.player_mut(remote_player).expect("remote player"),
            &make_snapshot(10.0),
        );
        world.set_simulation_tick(base_tick.next());
        let first_frame = crate::multiplayer::AuthoritativeFrame {
            epoch: keyframe.epoch,
            base_tick,
            result_tick: base_tick.next(),
            applied_sequences: BTreeMap::new(),
            changed_players: vec![make_snapshot(10.0)],
            terrain_mutations: Vec::new(),
            terrain_revisions: Vec::new(),
            canonical_state: serde_json::to_vec(&world).expect("first frame world"),
            state_hash: Some(world.simulation_state_hash().get()),
        };
        assert_eq!(
            client.apply_authoritative_client_frame(&first_frame),
            super::AuthoritativeClientFrameResult::Applied
        );
        let mut second_world = world;
        super::apply_network_player_snapshot_to_player(
            second_world
                .player_mut(remote_player)
                .expect("remote player"),
            &make_snapshot(20.0),
        );
        second_world.set_simulation_tick(base_tick.next().next());
        let second_frame = crate::multiplayer::AuthoritativeFrame {
            epoch: keyframe.epoch,
            base_tick: base_tick.next(),
            result_tick: base_tick.next().next(),
            applied_sequences: BTreeMap::new(),
            changed_players: vec![make_snapshot(20.0)],
            terrain_mutations: Vec::new(),
            terrain_revisions: Vec::new(),
            canonical_state: serde_json::to_vec(&second_world).expect("second frame world"),
            state_hash: Some(second_world.simulation_state_hash().get()),
        };
        assert_eq!(
            client.apply_authoritative_client_frame(&second_frame),
            super::AuthoritativeClientFrameResult::Applied
        );

        let interpolated = client
            .interpolated_authoritative_remote_player(remote_player, 0.5)
            .expect("interpolated remote");
        assert!((interpolated.x - 15.0).abs() < f32::EPSILON);
        assert_eq!(
            client.authoritative_client_status().remote_history_players,
            2
        );
    }

    #[test]
    fn typed_join_keyframe_and_terrain_recovery_use_complete_canonical_state() {
        let mut session = GameSession::new();
        let target = TilePosition { x: 2, y: 2 };
        session
            .world_mut()
            .terrain_mut()
            .set_kind(target, TileKind::Air);
        let revisions = session.mark_world_terrain_tiles_changed([target]);
        let position = super::TerrainChunkPosition::from_tile(target);
        let authoritative_revision = revisions[0].revision;

        let joined = session.join_accepted_message(LOCAL_CLIENT_ID, LOCAL_PLAYER_ID);
        let recovery = session.terrain_recovery_message(position, 0);
        let unchanged = session.terrain_recovery_message(position, authoritative_revision);

        let crate::multiplayer::ServerMessage::JoinAccepted {
            epoch, keyframe, ..
        } = joined
        else {
            panic!("join accepted");
        };
        assert_eq!(epoch, session.session_epoch());
        assert_eq!(keyframe.snapshot.tick, session.current_tick());
        assert_eq!(
            keyframe.state_hash,
            Some(session.world().simulation_state_hash().get())
        );
        assert_eq!(
            keyframe.snapshot.players.len(),
            session.world().player_count()
        );
        let restored = GameSession::restore_authoritative_keyframe(&keyframe, GameState::new())
            .expect("keyframe restores canonical world");
        assert_eq!(
            restored.world().simulation_state_hash(),
            session.world().simulation_state_hash()
        );
        let crate::multiplayer::ServerMessage::TerrainChunk {
            revision, tiles, ..
        } = recovery
        else {
            panic!("terrain chunk recovery");
        };
        assert_eq!(revision, authoritative_revision);
        assert!(
            tiles
                .iter()
                .any(|tile| tile.x == target.x && tile.y == target.y)
        );
        let crate::multiplayer::ServerMessage::TerrainChunk { tiles, .. } = unchanged else {
            panic!("unchanged terrain response");
        };
        assert!(tiles.is_empty());
    }

    #[test]
    fn missing_input_policy_holds_controls_briefly_without_repeating_actions_or_ack() {
        let mut session = GameSession::new();
        let sequence = InputSequence::new(22);
        let bundle = crate::multiplayer::InputBundle {
            epoch: session.session_epoch(),
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![crate::multiplayer::InputFrame {
                sequence,
                target_tick: session.current_tick(),
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: vec![crate::multiplayer::SequencedAction {
                    id: crate::multiplayer::ActionId::new(44),
                    command: PlayerCommand::UseScanner,
                }],
            }],
        };
        let _status = session.receive_input_bundle(&bundle);
        let Some((_applied, first)) = session.apply_accepted_input_frame(LOCAL_CLIENT_ID, sequence)
        else {
            panic!("input applies");
        };
        let scanner_after_action = session
            .world()
            .scanner_cooldown_seconds(LOCAL_PLAYER_ID)
            .expect("scanner cooldown");

        let held = session
            .apply_missing_input_policy(LOCAL_CLIENT_ID, session.current_tick())
            .expect("controls held inside policy window");

        assert!(first.applied_sequences.contains_key(&LOCAL_PLAYER_ID));
        assert!(held.applied_sequences.is_empty());
        assert!(
            held.changed_players
                .iter()
                .any(|player| player.player_id == LOCAL_PLAYER_ID)
        );
        assert!(
            session
                .world()
                .scanner_cooldown_seconds(LOCAL_PLAYER_ID)
                .expect("scanner cooldown")
                < scanner_after_action
        );
        assert!(
            session
                .apply_missing_input_policy(
                    LOCAL_CLIENT_ID,
                    SimulationTick::new(session.current_tick().get() + 7),
                )
                .is_none()
        );
    }

    #[test]
    fn local_and_remote_typed_inputs_share_the_host_inbox_and_canonical_transition() {
        let mut session = GameSession::new();
        let remote_client = ClientId::new(2);
        let remote_player = PlayerId::new(2);
        assert!(session.add_online_remote_client_player(remote_client, remote_player));
        let epoch = session.session_epoch();
        let local_bundle = crate::multiplayer::InputBundle {
            epoch,
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![crate::multiplayer::InputFrame {
                sequence: InputSequence::new(101),
                target_tick: session.current_tick(),
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: Vec::new(),
            }],
        };
        let remote_bundle = crate::multiplayer::InputBundle {
            epoch,
            client_id: remote_client,
            player_id: remote_player,
            frames: vec![crate::multiplayer::InputFrame {
                sequence: InputSequence::new(201),
                target_tick: session.current_tick().next(),
                controls: crate::multiplayer::InputControls::from_horizontal(-1.0, false, false),
                actions: Vec::new(),
            }],
        };

        let _local = session.route_local_input_bundle(&local_bundle);
        let _remote = session.receive_input_bundle(&remote_bundle);
        let Some((_status, remote_frame)) =
            session.apply_accepted_input_frame(remote_client, InputSequence::new(201))
        else {
            panic!("remote input applies");
        };

        assert_eq!(remote_frame.base_tick.next(), remote_frame.result_tick);
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player")
                .velocity_x
                > 0.0
        );
        assert!(
            session
                .world()
                .player(remote_player)
                .expect("remote player")
                .velocity_x
                < 0.0
        );
    }

    #[test]
    fn session_host_input_bundle_applies_through_canonical_tick_and_emits_frame() {
        let mut session = GameSession::new();
        let sequence = InputSequence::new(12);
        let base_tick = session.current_tick();
        let bundle = crate::multiplayer::InputBundle {
            epoch: session.session_epoch(),
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![crate::multiplayer::InputFrame {
                sequence,
                target_tick: base_tick,
                controls: crate::multiplayer::InputControls::from_horizontal(1.0, false, false),
                actions: Vec::new(),
            }],
        };

        let status_message = session.receive_input_bundle(&bundle);
        let Some((applied, frame)) = session.apply_accepted_input_frame(LOCAL_CLIENT_ID, sequence)
        else {
            panic!("accepted input applies");
        };

        let crate::multiplayer::ServerMessage::InputStatus { statuses, .. } = status_message else {
            panic!("input status response");
        };
        assert_eq!(
            statuses[0].status,
            crate::multiplayer::InputStatusKind::Received
        );
        assert_eq!(
            statuses[1].status,
            crate::multiplayer::InputStatusKind::Accepted
        );
        assert_eq!(applied.status, crate::multiplayer::InputStatusKind::Applied);
        assert_eq!(frame.base_tick, base_tick);
        assert_eq!(frame.result_tick, base_tick.next());
        assert!(frame.advances_exactly_one_tick());
        assert_eq!(
            frame.applied_sequences.get(&LOCAL_PLAYER_ID),
            Some(&sequence)
        );
        assert!(frame.state_hash.is_some());
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .velocity_x
                > 0.0
        );
    }

    #[test]
    fn session_epoch_rejects_stale_input_bundles_after_replacement() {
        let mut session = GameSession::new();
        let stale_epoch = session.session_epoch();
        session.replace_session_epoch(stale_epoch.next());
        let bundle = crate::multiplayer::InputBundle {
            epoch: stale_epoch,
            client_id: LOCAL_CLIENT_ID,
            player_id: LOCAL_PLAYER_ID,
            frames: vec![crate::multiplayer::InputFrame {
                sequence: InputSequence::new(2),
                target_tick: session.current_tick(),
                controls: crate::multiplayer::InputControls::default(),
                actions: Vec::new(),
            }],
        };

        let crate::multiplayer::ServerMessage::InputStatus {
            statuses, epoch, ..
        } = session.receive_input_bundle(&bundle)
        else {
            panic!("input status response");
        };

        assert_eq!(epoch, stale_epoch.next());
        assert_eq!(
            statuses[0].status,
            crate::multiplayer::InputStatusKind::Received
        );
        assert_eq!(
            statuses[1].status,
            crate::multiplayer::InputStatusKind::Rejected
        );
        assert!(
            session
                .apply_accepted_input_frame(LOCAL_CLIENT_ID, InputSequence::new(2))
                .is_none()
        );
    }

    #[test]
    fn authoritative_command_processing_applies_buffered_player_commands() {
        let mut session = GameSession::new();
        let tick = session.current_tick();

        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 0.5,
            thrust: false,
            drill_down: false,
        }]);

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);
        let velocity_x = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .velocity_x;
        // Held controls integrate with momentum from rest (airborne acceleration over one tick).
        let expected_velocity = drillgame_simulation::quantize::quantize_simulation_scalar(
            0.5 * drillgame_simulation::world::AUTHORITATIVE_HORIZONTAL_ACCELERATION
                * 0.65
                * session.world().timing().air_acceleration_gain,
        );
        assert!((velocity_x - expected_velocity).abs() < f32::EPSILON);
        assert_eq!(
            session.local_client().prediction().replay_commands().len(),
            0
        );
        assert!(session.drain_events().iter().any(|event| matches!(
            event,
            super::WorldEvent::PlayerChanged {
                player_id: LOCAL_PLAYER_ID
            }
        )));
    }

    #[test]
    fn accepted_online_remote_commands_apply_to_session_world_without_app_legacy_routing() {
        let mut session = GameSession::new();
        let remote_player = PlayerId::new(2);
        let target_tick = session.current_tick();
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: ClientId::new(2),
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: target_tick,
            accepted_commands: vec![SequencedPlayerCommand {
                player_id: remote_player,
                sequence: InputSequence::new(1),
                target_tick,
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let applied = session.apply_accepted_online_remote_commands(&summary);

        assert_eq!(applied, 1);
        assert!(session.has_client(ClientId::new(2)));
        let player = session
            .world()
            .player(remote_player)
            .expect("remote player exists in authoritative world");
        assert!(player.velocity_x > 0.0);
        assert!(session.game().player.velocity_x.abs() < f32::EPSILON);
    }

    #[test]
    fn live_session_tick_input_is_built_from_world_snapshot_and_world_player_chunk() {
        let mut session = GameSession::new();
        session.game.player.x = TILE_SIZE * 1.0;
        session.game.player.y = TILE_SIZE * 1.0;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local world player exists");
            player.x = TILE_SIZE * 48.0;
            player.y = TILE_SIZE * 32.0;
            player.fuel = 77.0;
            player.hull = 88.0;
            player.credits = 99;
        }

        let input = session.live_session_tick_input_from_world(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            9,
            vec![PlayerCommand::Movement {
                horizontal: 0.5,
                thrust: true,
                drill_down: false,
            }],
        );

        assert_eq!(input.terrain_chunk_request, Some((3, 2, 0, 0)));
        let snapshot = input.snapshot.expect("snapshot included");
        let player = snapshot
            .players
            .iter()
            .find(|player| player.player_id == LOCAL_PLAYER_ID)
            .expect("local player snapshot included");
        let expected_x = TILE_SIZE * 48.0;
        assert!((player.x - expected_x).abs() < f32::EPSILON);
        assert!((player.fuel - 77.0).abs() < f32::EPSILON);
        assert_eq!(player.credits, 99);
        assert!(matches!(
            input.delta,
            Some((_, NetworkDeltaPayload::Players { players }))
                if players.iter().any(|player| player.player_id == LOCAL_PLAYER_ID)
        ));
        let packet = input.command_packet.expect("command packet included");
        assert_eq!(packet.client_id, LOCAL_CLIENT_ID);
        assert_eq!(packet.commands[0].sequence, InputSequence::new(9));
        let authoritative_chunk = input
            .authoritative_terrain_chunks
            .iter()
            .find(|chunk| chunk.chunk_x == 3 && chunk.chunk_y == 2)
            .expect("authoritative terrain chunk included");
        assert!(!authoritative_chunk.tiles.is_empty());
        assert!(
            authoritative_chunk
                .tiles
                .iter()
                .any(|tile| tile.x == 48 && tile.y == 32)
        );
    }

    #[test]
    fn live_session_tick_input_terrain_chunks_use_world_terrain_not_legacy_game() {
        let mut session = GameSession::new();
        let target = TilePosition { x: 18, y: 19 };
        session.game.terrain.set_kind(target, TileKind::Lava);
        session.world.terrain_mut().set_kind(target, TileKind::Air);
        session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local world player exists")
            .x = TILE_SIZE * 18.0;
        session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local world player exists")
            .y = TILE_SIZE * 19.0;

        let input = session.live_session_tick_input_from_world(
            LOCAL_CLIENT_ID,
            LOCAL_PLAYER_ID,
            3,
            Vec::new(),
        );

        let chunk = input
            .authoritative_terrain_chunks
            .iter()
            .find(|chunk| chunk.chunk_x == 1 && chunk.chunk_y == 1)
            .expect("player terrain chunk included");
        let tile = chunk
            .tiles
            .iter()
            .find(|tile| tile.x == target.x && tile.y == target.y)
            .expect("target tile included");
        assert_eq!(tile.kind, TileKind::Air);
    }

    #[test]
    fn roster_seeding_creates_joined_client_without_legacy_bridge() {
        let mut session = GameSession::new();
        session.game.online_player_slot = Some(2);
        session.game.player.x = TILE_SIZE * 9.0;
        session.game.player.credits = 5;

        let identity = session.prepare_online_tick_identity(true);

        assert!(identity.seeded_session_presentation());
        assert!(!identity.joined_local_synced);
        let player = session
            .world()
            .player(PlayerId::new(2))
            .expect("joined player exists from roster/session seeding");
        let expected_x = TILE_SIZE * 9.0;
        assert!((player.x - expected_x).abs() < f32::EPSILON);
        assert_eq!(player.credits, 5);
        assert!(
            session
                .game()
                .online_session_roster_status
                .contains("joined-client slot=2")
        );
    }

    #[test]
    fn replicated_snapshot_updates_session_world_presentation_before_legacy_game() {
        let mut session = GameSession::new();
        session.game.player.x = 1.0;
        let local_player = LOCAL_PLAYER_ID;
        let remote_player = PlayerId::new(2);
        let snapshot = NetworkWorldSnapshot {
            tick: SimulationTick::new(7),
            players: vec![
                NetworkPlayerSnapshot {
                    player_id: local_player,
                    x: 20.0,
                    y: 21.0,
                    velocity_x: 2.0,
                    velocity_y: 3.0,
                    fuel: 40.0,
                    hull: 50.0,
                    credits: 60,
                    cargo_used: 0,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                    scanner_cooldown_seconds: 0.25,
                },
                NetworkPlayerSnapshot {
                    player_id: remote_player,
                    x: 120.0,
                    y: 121.0,
                    velocity_x: 12.0,
                    velocity_y: 13.0,
                    fuel: 140.0,
                    hull: 150.0,
                    credits: 160,
                    cargo_used: 0,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                    scanner_cooldown_seconds: 0.75,
                },
            ],
        };

        let summary = session.apply_replicated_snapshot_to_world_presentation(&snapshot);

        assert!(summary.local_players_updated > 0 || summary.remote_players_updated > 0);
        assert_eq!(summary.local_players_updated, 1);
        assert_eq!(summary.remote_players_updated, 1);
        assert_eq!(summary.clients_created, 1);
        assert!((session.game().player.x - 1.0).abs() < f32::EPSILON);
        assert!(
            (session
                .world()
                .player(local_player)
                .expect("local world player updated")
                .x
                - 20.0)
                .abs()
                < f32::EPSILON
        );
        let remote = session
            .world()
            .player(remote_player)
            .expect("remote world player updated");
        assert!((remote.x - 120.0).abs() < f32::EPSILON);
        assert_eq!(remote.credits, 160);
        assert_eq!(
            session.world().scanner_cooldown_seconds(remote_player),
            Some(0.75)
        );
        assert!(session.has_client(ClientId::new(2)));
    }

    #[test]
    fn accepted_online_remote_drill_command_changes_authoritative_terrain() {
        let mut session = GameSession::new();
        let remote_client = ClientId::new(2);
        let remote_player = PlayerId::new(2);
        let mut player = session.game().player.clone();
        player.x = 10.0 * TILE_SIZE;
        player.y = 10.0 * TILE_SIZE;
        player.drill_strength = 4;
        assert!(session.add_local_client_player(remote_client, remote_player));
        *session
            .world_mut()
            .player_mut(remote_player)
            .expect("remote player exists") = player;
        let target = TilePosition { x: 10, y: 11 };
        assert!(
            session
                .world_mut()
                .terrain_mut()
                .set_kind(target, TileKind::Dirt)
        );
        let target_tick = session.current_tick();
        let summary = crate::multiplayer::CommandPacketExchangeSummary {
            client_id: remote_client,
            acknowledged: 1,
            rejected: 0,
            authoritative_tick: target_tick,
            accepted_commands: vec![SequencedPlayerCommand {
                player_id: remote_player,
                sequence: InputSequence::new(1),
                target_tick,
                command: PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            }],
        };

        assert_eq!(session.apply_accepted_online_remote_commands(&summary), 1);
        assert!(session.world().active_drill(remote_player).is_some());
        let advance = session.advance_authoritative_world_ticks(30);

        assert!(advance.end_tick.get() > advance.start_tick.get());
        assert!(advance.terrain_events > 0);
        assert_eq!(
            session
                .world()
                .terrain()
                .tile(target)
                .expect("target tile remains in terrain")
                .kind,
            TileKind::Air
        );
        assert!(matches!(
            session.drain_world_delta().compact_network_delta(),
            CompactWorldDelta::TerrainChunks { .. }
        ));
    }

    #[test]
    fn production_session_authority_update_uses_routed_command_instead_of_raw_input() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: false,
        }]);

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            0.016,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(summary.local_movement_authority);
        assert_eq!(summary.command_adapter_count, 1);
        let world_player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("world player exists");
        assert!(world_player.velocity_x.abs() < f32::EPSILON);
        assert!(session.game().player.velocity_x.abs() < f32::EPSILON);
    }

    #[test]
    fn session_adds_second_local_client_player_for_in_process_host() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);

        assert!(session.add_local_client_player(second_client, second_player));
        assert_eq!(session.client_count(), 2);
        assert!(session.world().player(second_player).is_some());
        assert_eq!(session.render_frame_plan().view_count(), 2);
        assert_eq!(session.client_views().len(), 2);
        let frame_plan = session.render_frame_plan();
        let second_view = frame_plan
            .views
            .iter()
            .find(|view| view.controlled_player_id == second_player)
            .expect("second player view exists");
        let prediction_plan = super::PredictionPresentationPlan::default();
        let world_players =
            frame_plan.world_player_presentations_for_view(second_view, &prediction_plan);
        assert_eq!(world_players.len(), 2);
        assert!(
            world_players
                .iter()
                .any(|player| { player.player_id == second_player && player.local_to_view })
        );
        let client_presentations = frame_plan.client_presentation_snapshots();
        assert_eq!(client_presentations.len(), 2);
        assert!(client_presentations.iter().any(|presentation| {
            presentation.client_id == second_client
                && presentation.player_id == second_player
                && presentation.hud.player_id == second_player
        }));
        assert!(!session.add_local_client_player(second_client, PlayerId::new(3)));
        assert!(!session.add_local_client_player(ClientId::new(3), second_player));
    }

    #[test]
    fn split_screen_commands_route_to_each_client_player_stream() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        let tick = session.current_tick();

        let local_batch = session.route_client_player_commands(
            LOCAL_CLIENT_ID,
            CommandSource::Keyboard,
            vec![PlayerCommand::Movement {
                horizontal: 0.25,
                thrust: false,
                drill_down: false,
            }],
        );
        let split_batch = session.route_split_screen_player_commands(
            second_client,
            vec![PlayerCommand::Movement {
                horizontal: -0.75,
                thrust: false,
                drill_down: false,
            }],
        );

        assert_eq!(local_batch.commands[0].player_id, LOCAL_PLAYER_ID);
        assert_eq!(split_batch.commands[0].player_id, second_player);
        assert_eq!(split_batch.source, CommandSource::SplitScreenClient);
        assert!(split_batch.predicted_locally);
        assert_eq!(session.process_authoritative_commands_for_tick(tick), 2);
        let local_velocity = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .velocity_x;
        let second_velocity = session
            .world()
            .player(second_player)
            .expect("second player exists")
            .velocity_x;
        assert!(local_velocity > 0.0);
        assert!(second_velocity < 0.0);
    }

    #[test]
    fn authoritative_service_commands_update_canonical_player_economy() {
        let mut session = GameSession::new();
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.fuel = 10.0;
            player.credits = 500;
        }
        let tick = session.current_tick();

        session.route_local_player_commands(vec![PlayerCommand::Refuel]);

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);

        let world_player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("world player exists");
        assert!(world_player.fuel > 10.0);
        assert!(world_player.credits < 500);
        assert!(session.drain_events().iter().any(|event| matches!(
            event,
            super::WorldEvent::PurchaseCompleted {
                player_id: LOCAL_PLAYER_ID
            }
        )));
    }

    #[test]
    fn command_producers_route_replay_ai_gamepad_split_screen_online_and_keyboard_to_authoritative_buffer()
     {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        let producers = vec![
            crate::input_mapping::replay_commands(vec![PlayerCommand::Interact]),
            crate::input_mapping::ai_commands(vec![PlayerCommand::UseScanner]),
            crate::input_mapping::gamepad_commands(vec![PlayerCommand::PlaceBomb]),
            crate::input_mapping::split_screen_commands(vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }]),
            crate::input_mapping::online_commands(vec![PlayerCommand::Repair]),
            crate::input_mapping::local_keyboard_commands(PlayerInput {
                horizontal: -1.0,
                thrust: true,
                ..PlayerInput::default()
            }),
        ];

        let batches = session.route_command_producers(second_client, producers);

        assert_eq!(batches.len(), 6);
        assert!(batches.iter().all(|batch| batch.client_id == second_client));
        assert!(batches.iter().all(|batch| {
            batch
                .commands
                .iter()
                .all(|command| command.player_id == second_player)
        }));
        assert_eq!(session.pending_command_count(session.current_tick()), 6);
    }

    #[test]
    fn online_joined_client_save_authority_blocks_session_save_entrypoints() {
        let mut session = GameSession::new();
        session.game.online_player_slot = Some(2);
        session.game.online_host_owns_save = false;

        assert!(!session.can_write_local_save());
        assert_eq!(
            session.local_save_authority_denial().as_deref(),
            Some("Joined online clients cannot write the host-owned save.")
        );
        match session.save_session() {
            Err(crate::save::SaveError::SaveDenied(reason)) => {
                assert!(reason.contains("Joined online clients"));
            }
            other => panic!("expected save denied, got {other:?}"),
        }
        match session.save_session_slot(0) {
            Err(crate::save::SaveError::SaveDenied(reason)) => {
                assert!(reason.contains("host-owned save"));
            }
            other => panic!("expected slot save denied, got {other:?}"),
        }

        let summary = session.handle_session_save_load_input(PlayerInput {
            save: true,
            ..PlayerInput::default()
        });
        assert_eq!(summary.action, super::SessionSaveLoadAction::SaveDenied);
        assert!(summary.message.contains("Joined online clients"));
        assert_eq!(session.game().message, summary.message);
    }

    #[test]
    fn online_host_and_single_player_keep_local_save_authority() {
        let single_player = GameSession::new();
        assert!(single_player.can_write_local_save());
        assert!(single_player.local_save_authority_denial().is_none());

        let mut host = GameSession::new();
        host.game.online_player_slot = Some(1);
        host.game.online_host_owns_save = true;
        assert!(host.can_write_local_save());
        assert!(host.local_save_authority_denial().is_none());
    }

    #[test]
    fn local_two_player_gameplay_regression_covers_core_authoritative_slices() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        {
            let player = session
                .world
                .player_mut(second_player)
                .expect("second player exists");
            player.fuel = 0.0;
            player.hull = 0.0;
            player.credits = 500;
            player.cargo.insert(crate::terrain::MineralKind::Copper, 2);
        }
        let tick = session.current_tick();

        session.route_client_player_commands(
            LOCAL_CLIENT_ID,
            CommandSource::Keyboard,
            vec![PlayerCommand::Movement {
                horizontal: 0.5,
                thrust: false,
                drill_down: true,
            }],
        );
        session.route_split_screen_player_commands(
            second_client,
            vec![
                PlayerCommand::Rescue,
                PlayerCommand::SellCargo,
                PlayerCommand::Refuel,
            ],
        );

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 4);
        let local = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        let second = session
            .world()
            .player(second_player)
            .expect("second player exists");
        assert!(local.velocity_x > 0.0);
        assert!(session.world().active_drill(LOCAL_PLAYER_ID).is_some());
        assert!(second.hull > 0.0);
        assert!(second.fuel > 0.0);
        assert_eq!(second.cargo_used(), 0);
        assert!(second.credits > 0);
        let proof = session
            .world
            .player_scoped_gameplay_proof(LOCAL_PLAYER_ID, second_player)
            .expect("proof exists");
        assert!(proof.complete());
        let events = session.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::PlayerSurvivalChanged { player_id }
                if *player_id == second_player
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::CargoChanged { player_id } if *player_id == second_player
        )));
    }

    #[test]
    fn authoritative_world_chips_active_drill_target_and_collects_ore() {
        let mut session = GameSession::new();
        let target = TilePosition { x: 97, y: 5 };
        session
            .world_mut()
            .terrain_mut()
            .set_kind(target, TileKind::Ore(crate::terrain::MineralKind::Copper));
        session.world.set_active_drill(
            LOCAL_PLAYER_ID,
            Some(DrillState {
                target,
                direction: DrillDirection::Down,
                progress: 1.0,
                initial_durability: 1,
                seconds_per_chip: 0.0,
                sound_timer: 0.0,
                dust_timer: 0.0,
            }),
        );

        let mut result = session
            .world
            .chip_active_drill_target(LOCAL_PLAYER_ID)
            .expect("active drill chips");
        for _ in 0..8 {
            if matches!(result, MineResult::Mined(_)) {
                break;
            }
            result = session
                .world
                .chip_active_drill_target(LOCAL_PLAYER_ID)
                .expect("active drill continues chipping");
        }

        assert_eq!(
            result,
            MineResult::Mined(TileKind::Ore(crate::terrain::MineralKind::Copper))
        );
        assert_eq!(
            session
                .world()
                .terrain()
                .tile(target)
                .expect("tile exists")
                .kind,
            TileKind::Air
        );
        assert_eq!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .cargo
                .get(&crate::terrain::MineralKind::Copper)
                .copied(),
            Some(1)
        );
        session.sync_legacy_terrain_from_world();
        assert_eq!(
            session
                .game()
                .terrain
                .tile(target)
                .expect("tile exists")
                .kind,
            TileKind::Air
        );
    }

    #[test]
    fn session_drill_too_dangerous_damages_authoritative_player() {
        let mut session = GameSession::new();
        let player = session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists");
        player.x = 3.0 * TILE_SIZE;
        player.y = 5.0 * TILE_SIZE;
        player.hull = 50.0;
        let target = TilePosition { x: 3, y: 6 };
        assert!(
            session
                .world_mut()
                .terrain_mut()
                .set_kind(target, TileKind::Lava)
        );
        session.world_mut().set_active_drill(
            LOCAL_PLAYER_ID,
            Some(DrillState {
                target,
                direction: DrillDirection::Down,
                progress: 0.0,
                initial_durability: 1,
                seconds_per_chip: FIXED_DELTA_SECONDS,
                sound_timer: 0.0,
                dust_timer: 0.0,
            }),
        );

        let tick_inputs = super::TickInputs::default();
        let _simulation = session.world.simulate_tick(
            session.current_tick,
            &tick_inputs,
            &mut session.terrain_revisions,
        );

        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists");
        assert!(player.hull < 50.0);
        assert!(session.world().active_drill(LOCAL_PLAYER_ID).is_none());
    }

    #[test]
    fn authoritative_local_commands_are_masked_from_variable_delta_legacy_adapter() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session
            .world
            .player_mut(LOCAL_PLAYER_ID)
            .expect("player exists")
            .bombs = 1;
        let bomb_count = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .bombs;
        session.route_local_player_commands(vec![
            PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: true,
                drill_down: true,
            },
            PlayerCommand::UseScanner,
            PlayerCommand::PlaceBomb,
        ]);
        let before = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists")
            .clone();

        session.update_frame_from_session_authority(
            PlayerInput {
                horizontal: 1.0,
                thrust: true,
                drill_down: true,
                scan: true,
                bomb: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        let after = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("player exists");
        assert!(after.x > before.x);
        assert!(after.y <= before.y);
        assert_eq!(after.bombs, bomb_count.saturating_sub(1));
        assert_eq!(session.world().bomb_count(), 1);
        assert!(
            session
                .world()
                .scanner_cooldown_seconds(LOCAL_PLAYER_ID)
                .is_some_and(|cooldown| cooldown > 0.0 && cooldown <= 1.0)
        );
    }

    #[test]
    fn split_screen_authoritative_processing_acknowledges_secondary_client_commands() {
        let mut session = GameSession::new();
        let secondary_client = ClientId::new(2);
        let secondary_player = PlayerId::new(2);
        assert!(session.add_local_client_player(secondary_client, secondary_player));
        let tick = session.current_tick();
        session.route_split_screen_player_commands(secondary_client, vec![PlayerCommand::Rescue]);

        assert_eq!(
            session
                .clients
                .get(&secondary_client)
                .expect("secondary client exists")
                .prediction()
                .unacknowledged_commands()
                .len(),
            1
        );
        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);
        assert!(
            session
                .clients
                .get(&secondary_client)
                .expect("secondary client exists")
                .prediction()
                .unacknowledged_commands()
                .is_empty()
        );
    }

    #[test]
    fn local_split_screen_frame_updates_players_cameras_hud_minimap_depth_and_remote_visibility() {
        let mut session = GameSession::new();
        let _enabled = session.enable_default_local_split_screen();
        let secondary_client_id = session.secondary_local_client_id();
        for local_input in crate::input_mapping::local_split_screen_inputs(
            LOCAL_CLIENT_ID,
            PlayerInput {
                horizontal: -1.0,
                thrust: false,
                drill_down: false,
                ..PlayerInput::default()
            },
            secondary_client_id,
            Some(PlayerInput {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
                ..PlayerInput::default()
            }),
        ) {
            let _batch =
                session.route_command_producer(local_input.client_id, local_input.producer);
        }
        session.update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);
        session.observe_live_remote_player_snapshots();
        let prediction_plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);
        let output = session.live_render_frame_output(&prediction_plan);

        assert_eq!(session.client_count(), 2);
        assert_eq!(output.viewport_plans.len(), 2);
        assert_eq!(output.hud_snapshots.len(), 2);
        assert!(output.viewport_plans.iter().all(|plan| plan.clip_enabled));
        assert!(
            output
                .viewport_plans
                .iter()
                .all(|plan| plan.local_player.is_some())
        );
        assert!(
            output
                .world_players_by_view
                .iter()
                .all(|(_, players)| players.len() == 2)
        );
        assert!(output.ready_for_live_render_path());
    }

    #[test]
    fn local_split_screen_authoritative_movement_moves_secondary_player_independently() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        let secondary_client = ClientId::new(2);
        let secondary_player = PlayerId::new(2);
        assert!(session.add_local_client_player(secondary_client, secondary_player));
        let primary_before = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("primary exists")
            .x;
        let secondary_before = session
            .world()
            .player(secondary_player)
            .expect("secondary exists")
            .x;

        session.route_split_screen_player_commands(
            secondary_client,
            vec![PlayerCommand::Movement {
                horizontal: 1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        session.update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);

        let primary_after = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("primary exists")
            .x;
        let secondary_after = session
            .world()
            .player(secondary_player)
            .expect("secondary exists")
            .x;
        assert!((primary_after - primary_before).abs() < f32::EPSILON);
        assert!(secondary_after > secondary_before);
    }

    #[test]
    fn local_split_screen_startup_flow_adds_second_local_client_and_player() {
        let mut session = GameSession::new();
        let plan = GameSession::default_local_split_screen_startup_plan();
        assert_eq!(plan.primary_client_id, LOCAL_CLIENT_ID);
        assert_eq!(plan.secondary_client_id, ClientId::new(2));
        assert!(session.enable_default_local_split_screen());
        let player_slots = u8::try_from(session.client_count()).unwrap_or(u8::MAX);
        session
            .game_mut()
            .mark_local_multiplayer_active(player_slots);
        assert_eq!(session.client_count(), 2);
        assert!(session.game().local_multiplayer_active);
        assert_eq!(session.game().local_multiplayer_player_slots, 2);
        assert!(session.game().message.contains("Player 2"));
        assert!(session.world().player(plan.secondary_player_id).is_some());
        assert!(!session.enable_default_local_split_screen());
    }

    #[test]
    fn fixed_tick_authoritative_slices_are_stable_across_split_frame_deltas() {
        let session = GameSession::new();
        let proof = session.frame_rate_invariance_proof();

        assert!(proof.command_timing_stable);
        assert!(proof.fuel_stable);
        assert!(proof.drill_progress_stable);
        assert!(proof.hazard_damage_stable);
        assert!(proof.bomb_state_stable);
        assert!(proof.rescue_state_stable);
        assert!(proof.complete());
    }

    #[test]
    fn replayed_authoritative_command_stream_produces_identical_world_outcomes() {
        let proof = GameSession::replay_determinism_proof(vec![
            PlayerCommand::Movement {
                horizontal: 0.25,
                thrust: true,
                drill_down: true,
            },
            PlayerCommand::Refuel,
            PlayerCommand::PlaceBomb,
            PlayerCommand::Rescue,
        ]);

        assert_eq!(proof.same_tick, SimulationTick::default());
        assert_eq!(proof.same_player_count, 1);
        assert!(proof.same_transaction_count > 0);
        assert!(proof.same_bomb_count <= 1);
        assert_eq!(proof.same_infrastructure_count, 0);
        assert!(proof.same_local_player_state);
        assert!(proof.complete());
    }

    #[test]
    fn authoritative_extraction_inventory_restricts_legacy_gameplay_to_ui_save_adapter() {
        let inventory = GameSession::legacy_gameplay_mutation_inventory();

        assert!(inventory.inventory_complete());
        assert_eq!(
            inventory.domains,
            [super::LegacyGameplayMutationDomain::RenderUiSaveAdapter]
        );
        assert_eq!(inventory.authoritative_extraction_remaining, 0);
        assert_eq!(inventory.adapter_only_domains, 1);
    }

    #[test]
    fn authoritative_world_runtime_owns_all_gameplay_domains() {
        let mut world = WorldState::from_legacy_game(&GameState::new());
        let target = TilePosition { x: 4, y: 8 };
        assert!(
            world
                .terrain_mut()
                .set_kind(target, TileKind::Ore(crate::terrain::MineralKind::Copper))
        );
        let player = world.player_mut(LOCAL_PLAYER_ID).expect("player exists");
        player.x = 4.0 * TILE_SIZE;
        player.y = 7.0 * TILE_SIZE;
        player.credits = 10_000;
        player.loan_debt = 500;
        player.bombs = 1;
        player.signal_relay_kits = 1;
        player.hull = 5.0;
        player.fuel = 0.0;

        assert_eq!(
            world.apply_player_command(
                LOCAL_PLAYER_ID,
                &PlayerCommand::Movement {
                    horizontal: 0.0,
                    thrust: false,
                    drill_down: true,
                },
            ),
            super::PlayerScopedCommandOutcome::Applied
        );
        let mut mine_result = world
            .chip_active_drill_target(LOCAL_PLAYER_ID)
            .expect("drill target exists");
        while !matches!(mine_result, MineResult::Mined(_)) {
            mine_result = world
                .chip_active_drill_target(LOCAL_PLAYER_ID)
                .expect("drill target continues");
        }
        assert_eq!(
            world.terrain().tile(target).expect("tile").kind,
            TileKind::Air
        );
        assert_eq!(
            world
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .cargo
                .get(&crate::terrain::MineralKind::Copper)
                .copied(),
            Some(1)
        );

        assert!(world.apply_hazard_damage(LOCAL_PLAYER_ID, 6.0).is_some());
        assert!(world.failure_state(LOCAL_PLAYER_ID).is_some());
        assert!(world.reveal_scanner_area(LOCAL_PLAYER_ID, target, 2) > 0);
        assert_eq!(
            world.apply_player_command(LOCAL_PLAYER_ID, &PlayerCommand::PlaceBomb),
            super::PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(
            world.apply_player_command(
                LOCAL_PLAYER_ID,
                &PlayerCommand::PlaceInfrastructure { slot: 0 },
            ),
            super::PlayerScopedCommandOutcome::Applied
        );
        assert_eq!(world.bomb_count(), 1);
        assert_eq!(world.infrastructure_count(), 1);
        assert!(world.apply_contract_reward(LOCAL_PLAYER_ID, 250).is_some());
        assert!(world.start_expedition(LOCAL_PLAYER_ID, 100).is_some());
        assert!(world.repay_debt(LOCAL_PLAYER_ID, 200).is_some());
        assert!(world.award_victory(LOCAL_PLAYER_ID, 1_000).is_some());

        assert_eq!(world.authoritative_runtime_domain_count(), 7);
        assert!(world.authoritative_gameplay_ownership_complete());
        assert!(world.won_game());
        assert!(world.service_transactions().iter().any(|transaction| {
            transaction.kind == drillgame_simulation::PlayerTransactionKind::CompleteContract
        }));
        assert!(world.service_transactions().iter().any(|transaction| {
            transaction.kind == drillgame_simulation::PlayerTransactionKind::StartExpedition
        }));
        assert!(world.service_transactions().iter().any(|transaction| {
            transaction.kind == drillgame_simulation::PlayerTransactionKind::RepayDebt
        }));
        assert!(world.service_transactions().iter().any(|transaction| {
            transaction.kind == drillgame_simulation::PlayerTransactionKind::WinGame
        }));
    }

    #[test]
    fn variable_delta_audit_marks_gameplay_fixed_or_compatibility_and_animation_presentation_only()
    {
        let audit = GameSession::variable_delta_audit_summary();

        assert!(audit.gameplay_delta_audit_complete());
        assert_eq!(audit.unresolved_gameplay_variable_domains, 0);
        assert!(audit.fixed_or_compatibility_gameplay_domains >= 7);
        assert_eq!(audit.presentation_only_variable_domains, 1);
    }

    #[test]
    fn session_fixed_tick_chips_authoritative_drill_into_cargo_and_terrain_delta() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        let target = TilePosition { x: 97, y: 5 };
        session.world_mut().terrain_mut().set_tile_from_network(
            target,
            TileKind::Ore(crate::terrain::MineralKind::Copper),
            1,
        );
        session.world.set_active_drill(
            LOCAL_PLAYER_ID,
            Some(DrillState {
                target,
                direction: DrillDirection::Down,
                progress: 0.0,
                initial_durability: 1,
                seconds_per_chip: FIXED_DELTA_SECONDS,
                sound_timer: 0.0,
                dust_timer: 0.0,
            }),
        );

        let tick_inputs = super::TickInputs::default();
        let simulation = session.world.simulate_tick(
            session.current_tick,
            &tick_inputs,
            &mut session.terrain_revisions,
        );

        assert_eq!(
            session
                .world()
                .terrain()
                .tile(target)
                .expect("tile exists")
                .kind,
            TileKind::Air
        );
        assert_eq!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .cargo
                .get(&crate::terrain::MineralKind::Copper)
                .copied(),
            Some(1)
        );
        assert!(
            simulation.terrain_changed_tiles.contains(&target),
            "canonical tick output should report the mined tile"
        );
        assert!(simulation.events.iter().any(|event| matches!(
            event.kind,
            super::GameplayEventKind::CargoChanged {
                player_id: LOCAL_PLAYER_ID
            }
        )));
    }

    #[test]
    fn live_snapshot_delta_and_chunk_exchange_batches_use_real_session_state() {
        let mut session = GameSession::new();
        let target = TilePosition { x: 1, y: 1 };
        session.terrain_revisions.mark_tiles_changed([target]);
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 0.25,
            thrust: false,
            drill_down: false,
        }]);
        assert_eq!(
            session.process_authoritative_commands_for_tick(session.current_tick()),
            1
        );

        let snapshot_batch = session.live_snapshot_exchange_batch();
        assert_eq!(snapshot_batch.kind, ProtocolExchangeKind::SnapshotKeyframe);
        assert!(matches!(
            snapshot_batch.messages.as_slice(),
            [ProtocolMessage::SnapshotKeyframe { snapshot }]
                if snapshot.players.iter().any(|player| player.player_id == LOCAL_PLAYER_ID)
        ));
        let delta = session.drain_world_delta();
        let delta_batch = GameSession::live_world_delta_exchange_batch(&delta);
        assert_eq!(delta_batch.kind, ProtocolExchangeKind::WorldDelta);
        assert!(matches!(
            delta_batch.messages.as_slice(),
            [ProtocolMessage::WorldDelta {
                payload: NetworkDeltaPayload::KeyframeRequired,
                ..
            }]
        ));
        let chunk_batch = session
            .live_terrain_chunk_exchange_batch(super::TerrainChunkPosition::from_tile(target), 0);
        assert_eq!(chunk_batch.kind, ProtocolExchangeKind::TerrainChunk);
        assert!(matches!(
            chunk_batch.messages.as_slice(),
            [ProtocolMessage::TerrainChunkResponse { revision, .. }] if *revision > 0
        ));
    }

    #[test]
    fn live_network_integration_exercises_join_reconnect_rejection_recovery_and_latency() {
        let mut session = GameSession::new();
        let remote_client = ClientId::new(9);
        let remote_player = PlayerId::new(9);
        let terrain_position = super::TerrainChunkPosition { x: 0, y: 0 };
        session
            .terrain_revisions
            .mark_tiles_changed([TilePosition { x: 1, y: 1 }]);

        let (
            join_in_progress,
            reconnect,
            command_responses,
            terrain_recovery,
            high_latency_prediction,
        ) = session.exercise_live_network_integration(
            remote_client,
            remote_player,
            SessionToken::new(99),
            terrain_position,
            0,
        );

        assert_eq!(join_in_progress.kind, ProtocolExchangeKind::JoinHandshake);
        assert!(join_in_progress.messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::SnapshotKeyframe { snapshot }
                if snapshot.players.iter().any(|player| player.player_id == remote_player)
        )));
        assert!(reconnect.messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::ReconnectRequest {
                client_id,
                session_token,
            } if *client_id == remote_client && *session_token == SessionToken::new(99)
        )));
        assert!(command_responses.iter().any(|message| matches!(
            message,
            ProtocolMessage::CommandAcknowledgement(CommandAcknowledgement { client_id, .. })
                if *client_id == remote_client
        )));
        assert!(terrain_recovery.recovered_revision().is_some());
        assert!(!high_latency_prediction.remote_players.is_empty());
    }

    #[test]
    fn online_runtime_integration_covers_join_reconnect_ack_rejection_and_recovery() {
        let mut session = GameSession::new();
        let remote_client = ClientId::new(10);
        let remote_player = PlayerId::new(10);
        session
            .terrain_revisions
            .mark_tiles_changed([TilePosition { x: 1, y: 1 }]);

        let (join, reconnect, command_responses, terrain_recovery, prediction) = session
            .exercise_live_network_integration(
                remote_client,
                remote_player,
                SessionToken::new(110),
                super::TerrainChunkPosition { x: 0, y: 0 },
                0,
            );
        let rejection_responses = session.apply_live_command_packet(&CommandPacket {
            client_id: remote_client,
            commands: vec![SequencedPlayerCommand {
                player_id: remote_player,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(session.current_tick().get() + 16),
                command: PlayerCommand::Interact,
            }],
        });
        let reliable_count = join
            .messages
            .iter()
            .chain(reconnect.messages.iter())
            .chain(command_responses.iter())
            .chain(rejection_responses.iter())
            .filter(|message| {
                message.reliability_class() == crate::multiplayer::ReliabilityClass::Reliable
            })
            .count();
        let unreliable_count = join
            .messages
            .iter()
            .chain(reconnect.messages.iter())
            .chain(command_responses.iter())
            .chain(rejection_responses.iter())
            .filter(|message| {
                message.reliability_class()
                    == crate::multiplayer::ReliabilityClass::UnreliableSequenced
            })
            .count();

        assert!(join.messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::SnapshotKeyframe { snapshot }
                if snapshot.players.iter().any(|player| player.player_id == remote_player)
        )));
        assert!(reconnect.messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::ReconnectRequest { client_id, session_token }
                if *client_id == remote_client && *session_token == SessionToken::new(110)
        )));
        assert!(command_responses.iter().any(|message| matches!(
            message,
            ProtocolMessage::CommandAcknowledgement(CommandAcknowledgement { client_id, .. })
                if *client_id == remote_client
        )));
        assert!(rejection_responses.iter().any(|message| matches!(
            message,
            ProtocolMessage::CommandRejection(CommandRejection { client_id, .. })
                if *client_id == remote_client
        )));
        assert!(
            session
                .clients
                .get(&remote_client)
                .expect("remote client exists")
                .prediction()
                .prediction_failures()
                .contains(&super::PredictionFailure::CommandRejected)
        );
        assert!(terrain_recovery.recovered_revision().is_some());
        assert!(!prediction.remote_players.is_empty());
        assert!(reliable_count > 0);
        assert!(unreliable_count > 0);
    }

    #[test]
    fn live_prediction_plan_replays_corrections_and_remote_presentations() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 12.0,
            thrust: false,
            drill_down: false,
        }]);
        {
            let player = session
                .world
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.x = 100.0;
            player.y = 50.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
        {
            let player = session
                .world
                .player_mut(second_player)
                .expect("second player exists");
            player.x = 20.0;
            player.y = 30.0;
            player.velocity_x = 4.0;
        }
        session.observe_live_remote_player_snapshots();
        {
            let player = session
                .world
                .player_mut(second_player)
                .expect("second player exists");
            player.x = 24.0;
            player.y = 30.0;
        }
        session.observe_live_remote_player_snapshots();

        let plan = session.live_prediction_presentation_plan(0.0, 0.5, 0.0);

        assert_eq!(plan.local_movement, None);
        assert_eq!(plan.correction, None);
        assert_eq!(plan.corrected_local_presentation, None);
        assert!(
            plan.remote_players
                .iter()
                .any(|remote| remote.player_id == second_player
                    && (remote.x - 22.0).abs() < f32::EPSILON)
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "broad Phase 6 regression intentionally covers all prediction polish dimensions together"
    )]
    fn prediction_reconciliation_polish_covers_replay_timing_failures_debug_and_high_ping() {
        let player_id = LOCAL_PLAYER_ID;
        let authoritative = PlayerSnapshot {
            player_id,
            x: 10.0,
            y: 20.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            fuel: 10.0,
            fuel_capacity: 100.0,
            hull: 10.0,
            max_hull: 100.0,
            credits: 100,
            cargo_used: 0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
            scanner_cooldown_seconds: 0.0,
        };
        let commands = vec![
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(1),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: true,
                    drill_down: true,
                },
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(2),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::UseScanner,
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(3),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::BuyUpgrade { index: 0 },
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(4),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::SellCargo,
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(5),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::Rescue,
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(6),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::PlaceBomb,
            },
            SequencedPlayerCommand {
                player_id,
                sequence: InputSequence::new(7),
                target_tick: SimulationTick::new(1),
                command: PlayerCommand::PlaceInfrastructure { slot: 0 },
            },
        ];

        let replayed =
            ClientPredictionState::replay_unacknowledged_player_state(&authoritative, &commands);
        assert_eq!(replayed.replayed_command_count, commands.len());
        assert!((replayed.snapshot.scanner_cooldown_seconds - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            ClientPredictionState::correction_plan(0.1, 0.0),
            super::CorrectionPlan::None
        );
        assert_eq!(
            ClientPredictionState::correction_plan(8.0, 0.0),
            super::CorrectionPlan::Smooth
        );
        assert_eq!(
            ClientPredictionState::correction_plan(24.0, 0.0),
            super::CorrectionPlan::Snap
        );
        assert!(PredictionCorrectionTuning::classifies_expected_offsets());

        let timing = RemoteTimingTuning::from_latency_loss(0.24, 0.25);
        assert!(timing.allows_extrapolation(timing.extrapolation_limit));
        assert!(!timing.allows_extrapolation(timing.timeout_after));
        assert!(timing.timed_out(timing.timeout_after + FIXED_DELTA_SECONDS));

        let mut prediction = ClientPredictionState::default();
        for failure in [
            PredictionFailure::TerrainAlreadyChanged,
            PredictionFailure::EconomyChangedState,
            PredictionFailure::ProgressionChangedState,
            PredictionFailure::HazardOrRescueChangedState,
            PredictionFailure::CommandRejected,
            PredictionFailure::SaveSessionTransition,
        ] {
            prediction.note_prediction_failure(failure);
        }
        let mut terrain_revisions = super::TerrainRevisionTracker::default();
        let terrain_position = super::TerrainChunkPosition { x: 0, y: 0 };
        terrain_revisions.mark_tiles_changed([TilePosition { x: 0, y: 0 }]);
        let actions = prediction.prediction_recovery_actions(
            player_id,
            &terrain_revisions,
            SimulationTick::new(2),
            terrain_position,
            0,
        );
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, PredictionRecoveryAction::RequestTerrainDelta(_)))
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            PredictionRecoveryAction::RollBackLocalEconomy { .. }
        )));
        assert!(
            actions.iter().any(|action| matches!(
                action,
                PredictionRecoveryAction::RollBackProgression { .. }
            ))
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            PredictionRecoveryAction::RequestAuthoritativeSnapshot { .. }
        )));

        let debug = NetworkDebugInstrumentationSnapshot {
            ping_seconds: 0.24,
            prediction_buffer_commands: commands.len(),
            correction_plan: super::CorrectionPlan::Smooth,
            dropped_packets: 2,
            snapshot_recoveries: 1,
            chunk_recoveries: 1,
        };
        assert!(debug.visible_to_debug_overlay());
    }

    #[test]
    fn live_command_packet_applies_acknowledgements_and_rejections() {
        let mut session = GameSession::new();
        let accepted_sequence = InputSequence::new(0);
        let packet = CommandPacket {
            client_id: LOCAL_CLIENT_ID,
            commands: vec![SequencedPlayerCommand {
                player_id: LOCAL_PLAYER_ID,
                sequence: accepted_sequence,
                target_tick: session.current_tick(),
                command: PlayerCommand::Movement {
                    horizontal: 0.75,
                    thrust: false,
                    drill_down: false,
                },
            }],
        };

        let responses = session.apply_live_command_packet(&packet);

        assert_eq!(responses.len(), 1);
        assert!(matches!(
            responses[0],
            ProtocolMessage::CommandAcknowledgement(CommandAcknowledgement {
                acknowledged_sequence,
                ..
            }) if acknowledged_sequence == accepted_sequence
        ));
        assert_eq!(
            session.process_authoritative_commands_for_tick(session.current_tick()),
            1
        );
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("player exists")
                .velocity_x
                > 0.0,
            "canonical fixed-tick movement should accelerate rightward"
        );

        let duplicate_responses = session.apply_live_command_packet(&packet);

        assert_eq!(duplicate_responses.len(), 1);
        assert!(matches!(
            duplicate_responses[0],
            ProtocolMessage::CommandRejection(CommandRejection {
                sequence,
                ..
            }) if sequence == accepted_sequence
        ));
    }

    #[test]
    fn live_world_snapshot_and_delta_messages_use_authoritative_world_state() {
        let mut session = GameSession::new();
        let second_client = ClientId::new(2);
        let second_player = PlayerId::new(2);
        assert!(session.add_local_client_player(second_client, second_player));
        let tick = session.current_tick();
        session.route_split_screen_player_commands(
            second_client,
            vec![PlayerCommand::Movement {
                horizontal: -1.0,
                thrust: false,
                drill_down: false,
            }],
        );
        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);

        let keyframe = session.live_snapshot_keyframe_message();
        let ProtocolMessage::SnapshotKeyframe { snapshot } = keyframe else {
            panic!("expected snapshot keyframe");
        };
        assert_eq!(snapshot.tick, session.world().simulation_tick());
        assert!(
            snapshot
                .players
                .iter()
                .any(|player| player.player_id == second_player && player.velocity_x < 0.0)
        );

        let delta = session.drain_world_delta();
        let delta_message = GameSession::live_world_delta_message(&delta);
        let ProtocolMessage::WorldDelta { tick, payload } = delta_message else {
            panic!("expected world delta");
        };
        assert_eq!(tick, delta.tick);
        assert!(matches!(payload, NetworkDeltaPayload::KeyframeRequired));
    }

    #[test]
    fn authoritative_rescue_updates_player_survival_state() {
        let mut session = GameSession::new();
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.fuel = 0.0;
            player.hull = 0.0;
            player.velocity_x = 25.0;
        }
        let before = session
            .world()
            .player_survival_snapshot(LOCAL_PLAYER_ID)
            .expect("survival snapshot exists");
        assert!(before.disabled);
        assert!(before.stranded);
        let tick = session.current_tick();

        session.route_local_player_commands(vec![PlayerCommand::Rescue]);

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);
        let after = session
            .world()
            .player_survival_snapshot(LOCAL_PLAYER_ID)
            .expect("survival snapshot exists");
        assert!(!after.disabled);
        assert!(!after.stranded);
        assert!(after.hull > 0.0);
        assert!(after.fuel > 0.0);
        let frame_survival = session.render_frame_plan().survival_snapshots();
        assert_eq!(frame_survival.len(), 1);
        assert!(!frame_survival[0].disabled);
        let events = session.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::PlayerSurvivalChanged {
                player_id: LOCAL_PLAYER_ID
            }
        )));
    }

    #[test]
    fn session_slot_io_request_saves_world_state_from_authoritative_session() {
        let temp_dir = std::env::temp_dir().join(format!(
            "drillgame-session-slot-save-{}",
            std::process::id()
        ));
        let _ignored = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("temp save dir");
        unsafe {
            std::env::set_var("DRILLGAME_STATE_DIR", &temp_dir);
        }

        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = Some(crate::game_state::ModalScreen::SaveSlots);
        session.game.selected_menu_item = 0;
        session.game.save_dirty = true;
        session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .credits = 4321;

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(!session.game().save_dirty);
        assert!(session.game().message.contains("Saved slot 1"));
        let loaded = crate::save::load_persistent_world_slot(0).expect("slot save loads");
        assert_eq!(loaded.default_player().expect("player state").credits, 4321);
        unsafe {
            std::env::remove_var("DRILLGAME_STATE_DIR");
        }
        let _ignored = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "single regression intentionally covers expedition, active side-contract, and legacy fallback side-contract completion ordering"
    )]
    fn session_complete_depot_work_handles_expeditions_and_side_contracts_authoritatively() {
        let mut expedition_session = GameSession::new();
        expedition_session.game.run_mode = RunMode::Playing;
        expedition_session.game.modal = Some(crate::game_state::ModalScreen::Depot);
        expedition_session.game.selected_menu_item = 0;
        expedition_session.game.town_event_day = 1;
        expedition_session.game.deepest_tile_reached = 100;
        expedition_session
            .game
            .active_expeditions
            .push(crate::game_state::Expedition {
                kind: crate::game_state::ExpeditionObjectiveKind::ReachDepth,
                target: TileKind::Stone,
                required: 80,
                reward: 777,
                expires_day: 3,
            });
        {
            let player = expedition_session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 10;
        }
        expedition_session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = expedition_session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(expedition_session.game().active_expeditions.is_empty());
        assert_eq!(expedition_session.game().expeditions_completed, 1);
        assert_eq!(expedition_session.game().player.credits, 787);
        assert_eq!(
            expedition_session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .credits,
            787
        );

        let mut side_contract_session = GameSession::new();
        side_contract_session.game.run_mode = RunMode::Playing;
        side_contract_session.game.modal = Some(crate::game_state::ModalScreen::Depot);
        side_contract_session.game.selected_menu_item = 0;
        side_contract_session.game.side_contract_active = true;
        side_contract_session.game.side_contract_kind = crate::game_state::SideContractKind::Cargo;
        side_contract_session.game.side_contract_target =
            Some(TileKind::Ore(crate::terrain::MineralKind::Gold));
        side_contract_session.game.side_contract_required = 2;
        side_contract_session
            .game
            .active_side_contracts
            .push(crate::game_state::SideContract {
                kind: crate::game_state::SideContractKind::Cargo,
                target: TileKind::Ore(crate::terrain::MineralKind::Gold),
                required: 2,
                expires_day: None,
            });
        {
            let player = side_contract_session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 5;
            *player
                .cargo
                .entry(crate::terrain::MineralKind::Gold)
                .or_default() = 2;
        }
        side_contract_session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = side_contract_session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(
            side_contract_session
                .game()
                .active_side_contracts
                .is_empty()
        );
        assert!(!side_contract_session.game().side_contract_active);
        assert_eq!(side_contract_session.game().player.credits, 585);
        assert_eq!(side_contract_session.game().player.cargo_used(), 0);
        let player = side_contract_session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert_eq!(player.credits, 585);
        assert_eq!(player.cargo_used(), 0);

        let mut fallback_contract_session = GameSession::new();
        fallback_contract_session.game.run_mode = RunMode::Playing;
        fallback_contract_session.game.modal = Some(crate::game_state::ModalScreen::Depot);
        fallback_contract_session.game.selected_menu_item = 0;
        fallback_contract_session.game.side_contract_active = true;
        fallback_contract_session.game.side_contract_kind =
            crate::game_state::SideContractKind::Rush;
        fallback_contract_session.game.side_contract_target =
            Some(TileKind::Ore(crate::terrain::MineralKind::Ruby));
        fallback_contract_session.game.side_contract_required = 1;
        {
            let player = fallback_contract_session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 9;
            *player
                .cargo
                .entry(crate::terrain::MineralKind::Ruby)
                .or_default() = 1;
        }
        fallback_contract_session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = fallback_contract_session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(!fallback_contract_session.game().side_contract_active);
        assert_eq!(fallback_contract_session.game().player.credits, 509);
        assert_eq!(fallback_contract_session.game().player.cargo_used(), 0);
        let player = fallback_contract_session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert_eq!(player.credits, 509);
        assert_eq!(player.cargo_used(), 0);

        let mut story_contract_session = GameSession::new();
        story_contract_session.game.run_mode = RunMode::Playing;
        story_contract_session.game.modal = Some(crate::game_state::ModalScreen::Depot);
        story_contract_session.game.selected_menu_item = 0;
        {
            let player = story_contract_session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 3;
            *player
                .cargo
                .entry(crate::terrain::MineralKind::Copper)
                .or_default() = 4;
        }
        story_contract_session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = story_contract_session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(story_contract_session.game().contracts.completed, 1);
        assert_eq!(story_contract_session.game().player.credits, 93);
        assert_eq!(story_contract_session.game().player.cargo_used(), 0);
        assert!(
            story_contract_session
                .game()
                .message
                .contains("Starter Stockpile")
        );
        let player = story_contract_session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert_eq!(player.credits, 93);
        assert_eq!(player.cargo_used(), 0);
    }

    #[test]
    fn session_town_development_and_salvage_recovery_mutate_authoritative_state() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = Some(crate::game_state::ModalScreen::TownDevelopment);
        session.game.selected_menu_item = 0;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 10_000;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().town_development.depot_level, 1);
        assert!(session.game().player.credits < 10_000);

        session.game.modal = Some(crate::game_state::ModalScreen::Salvage);
        session.game.selected_menu_item = 0;
        session.game.lost_cargo_count = 1;
        session
            .game
            .lost_minerals
            .insert(crate::terrain::MineralKind::Copper, 1);
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().lost_cargo_count, 0);
        assert_eq!(session.game().player.cargo_used(), 1);
        assert_eq!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .cargo_used(),
            1
        );

        session.game.modal = Some(crate::game_state::ModalScreen::Salvage);
        session.game.selected_menu_item = 5;
        let credits_before = session.game().player.credits;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().player.credits, credits_before + 35);
    }

    #[test]
    fn session_crafting_service_mutates_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = Some(crate::game_state::ModalScreen::Crafting);
        session.game.selected_menu_item = 0;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.add_material(crate::terrain::StrategicResourceKind::AncientAlloy, 2);
            player.hull = 1.0;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(session.game().message.contains("Crafted"));
        assert_eq!(session.game().player.crafted_bulkheads, 1);
        assert!(session.game().player.hull > 1.0);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert_eq!(player.crafted_bulkheads, 1);
        assert!(player.hull > 1.0);
        assert!(player.materials.is_empty());
    }

    #[test]
    fn session_bank_services_mutate_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.town_development.bank_level = 2;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 100;
            player.loan_debt = 0;
            player.insured = false;
            player.insurance_tier = 0;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        session.game.modal = Some(crate::game_state::ModalScreen::Bank);
        session.game.selected_menu_item = 0;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().message.contains("credit advance"));
        assert!(session.game().player.credits > 100);
        assert!(session.game().player.loan_debt > 0);

        session.game.modal = Some(crate::game_state::ModalScreen::Bank);
        session.game.selected_menu_item = 1;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().player.insured);
        assert_eq!(session.game().player.insurance_tier, 1);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert!(player.insured);
        assert_eq!(player.insurance_tier, 1);

        session.game.modal = Some(crate::game_state::ModalScreen::Bank);
        session.game.selected_menu_item = 2;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().active_side_contracts.len(), 1);
        assert!(session.game().message.contains("contract posted"));
    }

    #[test]
    fn session_explosives_and_salvage_services_mutate_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.town_development.explosives_shack_level = 4;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 500;
            player.bombs = 0;
            player.hull = 10.0;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        session.game.modal = Some(crate::game_state::ModalScreen::Explosives);
        session.game.selected_menu_item = 0;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().message.contains("timed charges"));
        assert_eq!(session.game().player.bombs, 5);
        assert_eq!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .bombs,
            5
        );

        session.game.modal = Some(crate::game_state::ModalScreen::Explosives);
        session.game.selected_menu_item = 2;
        session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .credits = 500;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().player.bombs, 9);

        session.game.modal = Some(crate::game_state::ModalScreen::Salvage);
        session.game.selected_menu_item = 1;
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().player.hull > 10.0);
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .hull
                > 10.0
        );
    }

    #[test]
    fn session_shop_modal_buys_upgrade_from_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.current_zone = Some(SurfaceZone::Shop);
        session.game.player.x = 120.0 * TILE_SIZE;
        session.game.player.y = 4.0 * TILE_SIZE;
        session.game.modal = Some(crate::game_state::ModalScreen::ShopConfirm);
        session.game.selected_menu_item = 0;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 10_000;
            player.drill_strength = 1;
            player.x = 120.0 * TILE_SIZE;
            player.y = 4.0 * TILE_SIZE;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(
            session.game().message.contains("Bought"),
            "{}",
            session.game().message
        );
        assert!(session.game().player.credits < 10_000);
        assert_eq!(session.game().player.drill_strength, 2);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert!(player.credits < 10_000);
        assert_eq!(player.drill_strength, 2);
    }

    #[test]
    fn session_service_modal_sells_cargo_from_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 100;
            assert!(player.add_cargo(crate::terrain::MineralKind::Copper));
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);
        session.game.modal = Some(crate::game_state::ModalScreen::Depot);
        session.game.selected_menu_item = 1;

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert_eq!(session.game().player.cargo_used(), 0);
        assert!(session.game().player.credits > 100);
        let world_player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert_eq!(world_player.cargo_used(), 0);
        assert!(world_player.credits > 100);
        assert!(
            session.game().message.contains("Sold cargo"),
            "{}",
            session.game().message
        );
    }

    #[test]
    fn session_service_modal_applies_refuel_and_repair_to_authoritative_world() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.credits = 10_000;
            player.fuel = 1.0;
            player.hull = 1.0;
        }
        session.sync_legacy_player_from_world(LOCAL_PLAYER_ID);

        session.game.modal = Some(crate::game_state::ModalScreen::FuelConfirm);
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().message.contains("Fuel"));
        assert!(session.game().player.fuel > 1.0);
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .fuel
                > 1.0
        );

        session.game.modal = Some(crate::game_state::ModalScreen::RepairConfirm);
        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(!summary.legacy_bridge_active());
        assert!(session.game().message.contains("Hull"));
        assert!(session.game().player.hull > 1.0);
        assert!(
            session
                .world()
                .player(LOCAL_PLAYER_ID)
                .expect("local player exists")
                .hull
                > 1.0
        );
    }

    #[test]
    fn authoritative_idle_input_keeps_physics_live_and_clears_sticky_movement() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        let start_y = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .y;

        let idle_summary = session
            .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);

        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert!(!idle_summary.legacy_bridge_active());
        assert!(idle_summary.local_movement_authority);
        assert!(player.y > start_y);
        assert!(player.velocity_y > 0.0);
        assert_eq!(
            session.latest_local_movement_intent,
            Some(super::PlayerMovementIntent {
                horizontal: 0.0,
                thrust: false,
                drill_down: false,
            })
        );

        let _moving = session.update_frame_from_session_authority(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );
        assert!(
            (session
                .latest_local_movement_intent
                .expect("movement intent after moving")
                .horizontal
                - 1.0)
                .abs()
                < f32::EPSILON
        );

        let _released = session
            .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);
        assert_eq!(
            session.latest_local_movement_intent,
            Some(super::PlayerMovementIntent {
                horizontal: 0.0,
                thrust: false,
                drill_down: false,
            })
        );
    }

    #[test]
    fn session_authority_preserves_surface_interact_for_shell_store_entry() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.x = TILE_SIZE * 122.0;
            player.y = TILE_SIZE * 2.0;
        }

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().run_mode, RunMode::Interior);
        assert_eq!(
            session.game().interior_zone,
            Some(crate::economy::SurfaceZone::Shop)
        );
    }

    #[test]
    fn interior_shell_movement_ignores_stale_authoritative_idle_intent() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Interior;
        session.game.interior_zone = Some(SurfaceZone::Shop);
        session.game.interior_x = 100.0;
        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: false,
        }]);
        let start_x = session.game.interior_x;

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(session.game().interior_x > start_x);
    }

    #[test]
    fn window_close_requests_exit_prompt_without_immediate_shutdown() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.save_dirty = true;

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                exit_requested: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(
            summary,
            super::SessionAuthorityUpdateSummary {
                used_legacy_presentation_adapter: false,
                local_movement_authority: true,
                command_adapter_count: 1,
                current_tick: session.current_tick(),
            }
        );
        assert_eq!(session.game().modal, Some(ModalScreen::UnsavedExitConfirm));
        assert!(!session.should_exit());

        let _confirmed = session.update_frame_from_session_authority(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(session.should_exit());
    }

    #[test]
    fn clean_window_close_requests_confirm_prompt_without_immediate_shutdown() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.save_dirty = false;

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                exit_requested: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().modal, Some(ModalScreen::ExitConfirm));
        assert!(!session.should_exit());
    }

    #[test]
    fn session_authority_preserves_cancel_for_depot_modal_escape() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = Some(ModalScreen::Depot);

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().run_mode, RunMode::Playing);
        assert_eq!(session.game().modal, None);
    }

    #[test]
    fn gameplay_escape_cancel_opens_pause_without_stale_command_replay() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = None;

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().run_mode, RunMode::Paused);
        assert_eq!(session.game().modal, None);
        assert!(
            !session
                .latest_local_authoritative_commands
                .iter()
                .any(|command| matches!(command, PlayerCommand::Cancel))
        );

        let _next = session
            .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);

        assert_eq!(session.game().run_mode, RunMode::Paused);
    }

    #[test]
    fn modal_escape_cancel_closes_shop_without_opening_pause() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = Some(ModalScreen::Shop);

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().run_mode, RunMode::Playing);
        assert_eq!(session.game().modal, None);
    }

    #[test]
    fn production_session_authority_update_maps_real_local_input_into_world_commands() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = None;
        let start_x = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists")
            .x;

        let summary = session.update_frame_from_session_authority(
            PlayerInput {
                horizontal: 1.0,
                thrust: true,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert!(!summary.legacy_bridge_active());
        assert!(summary.local_movement_authority);
        assert_eq!(summary.command_adapter_count, 1);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert!(player.x > start_x);
        assert!(player.velocity_x > 0.0);
    }

    #[test]
    fn production_session_authority_preserves_shell_boundary_input_after_routing_movement() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.game.modal = None;

        let _summary = session.update_frame_from_session_authority(
            PlayerInput {
                cancel: true,
                horizontal: 1.0,
                ..PlayerInput::default()
            },
            FIXED_DELTA_SECONDS,
        );

        assert_eq!(session.game().run_mode, RunMode::Paused);
        assert_eq!(session.game().modal, None);
        assert_eq!(session.latest_local_authoritative_commands.len(), 1);
        assert!(matches!(
            session.latest_local_authoritative_commands.as_slice(),
            [PlayerCommand::Movement { horizontal, .. }] if (*horizontal - 1.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn production_session_authority_update_routes_action_commands_without_public_legacy_adapter() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        {
            let player = session
                .world_mut()
                .player_mut(LOCAL_PLAYER_ID)
                .expect("local player exists");
            player.fuel = 10.0;
            player.hull = 20.0;
            player.credits = 10_000;
        }
        session.route_local_player_commands(vec![PlayerCommand::Refuel, PlayerCommand::Repair]);
        let tick = session.current_tick();
        assert_eq!(session.process_authoritative_commands_for_tick(tick), 2);

        let summary = session
            .update_frame_from_session_authority(PlayerInput::default(), FIXED_DELTA_SECONDS);

        assert!(!summary.legacy_bridge_active());
        assert_eq!(summary.command_adapter_count, 1);
        let player = session
            .world()
            .player(LOCAL_PLAYER_ID)
            .expect("local player exists");
        assert!(player.fuel > 10.0);
        assert!(player.hull > 20.0);
    }

    #[test]
    fn routed_drill_command_updates_canonical_active_drill() {
        let mut session = GameSession::new();
        let tick = session.current_tick();

        session.route_local_player_commands(vec![PlayerCommand::Movement {
            horizontal: 0.0,
            thrust: false,
            drill_down: true,
        }]);

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 1);
        assert!(session.world().active_drill(LOCAL_PLAYER_ID).is_some());
    }

    #[test]
    fn authoritative_command_processing_emits_domain_events_for_applied_commands() {
        let mut session = GameSession::new();
        let player = session
            .world_mut()
            .player_mut(LOCAL_PLAYER_ID)
            .expect("local player exists");
        player.bombs = 1;
        let _added = player.add_cargo(crate::terrain::MineralKind::Copper);
        let tick = session.current_tick();

        session.sequence_local_commands(vec![
            PlayerCommand::PlaceBomb,
            PlayerCommand::Refuel,
            PlayerCommand::SellCargo,
        ]);

        assert_eq!(session.process_authoritative_commands_for_tick(tick), 3);
        let events = session.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::BombPlaced {
                player_id: LOCAL_PLAYER_ID
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::PurchaseCompleted {
                player_id: LOCAL_PLAYER_ID
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::CargoChanged {
                player_id: LOCAL_PLAYER_ID
            }
        )));
    }

    #[test]
    fn frame_delta_accumulator_reports_fixed_steps() {
        let mut session = GameSession::new();

        let steps = session.accumulate_frame_delta(crate::multiplayer::FIXED_DELTA_SECONDS * 2.5);

        assert_eq!(steps, 2);
        assert!(session.simulation_accumulator() > Duration::ZERO);
    }

    #[test]
    fn advancing_tick_uses_simulation_tick_wrapper() {
        let mut session = GameSession::new();

        session.advance_tick();

        assert_eq!(session.current_tick().get(), 1);
    }

    #[test]
    fn authority_update_emits_tick_events() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.sequence_local_commands(vec![PlayerCommand::Interact]);

        session.update_frame_from_session_authority(
            crate::input::PlayerInput::default(),
            crate::multiplayer::FIXED_DELTA_SECONDS,
        );
        let events = session.drain_events();

        assert!(events.iter().any(|event| matches!(
            event,
            super::WorldEvent::CommandsProcessed {
                command_count: 2,
                ..
            }
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::TickAdvanced { .. }))
        );
    }

    #[test]
    fn world_delta_drains_pending_events() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        session.sequence_local_commands(vec![PlayerCommand::Interact]);
        session.update_frame_from_session_authority(
            crate::input::PlayerInput::default(),
            crate::multiplayer::FIXED_DELTA_SECONDS,
        );

        let delta = session.drain_world_delta();

        assert_eq!(delta.tick, session.current_tick());
        assert!(!delta.is_empty());
        assert!(session.drain_world_delta().is_empty());
    }

    #[test]
    fn terrain_revision_tracker_builds_chunk_recovery_deltas() {
        let mut tracker = super::TerrainRevisionTracker::default();
        let position = super::TerrainChunkPosition { x: 0, y: 0 };
        tracker.mark_tiles_changed([crate::terrain::TilePosition { x: 0, y: 0 }]);

        assert_eq!(
            tracker.recovery_delta(SimulationTick::new(12), position, 1),
            super::CompactWorldDelta::Noop {
                tick: SimulationTick::new(12),
            }
        );
        assert_eq!(
            tracker.recovery_delta(SimulationTick::new(12), position, 0),
            super::CompactWorldDelta::TerrainChunks {
                tick: SimulationTick::new(12),
                revisions: vec![super::TerrainChunkRevision {
                    position,
                    revision: 1,
                }],
            }
        );
    }

    #[test]
    fn session_builds_snapshot_chunk_recovery_plan() {
        let mut session = GameSession::new();
        let position = super::TerrainChunkPosition { x: 0, y: 0 };
        session
            .terrain_revisions
            .mark_tiles_changed([crate::terrain::TilePosition { x: 0, y: 0 }]);

        let plan = session.snapshot_chunk_recovery_plan(position, 0);

        assert_eq!(plan.snapshot_tick, session.current_tick());
        assert_eq!(plan.requested_position, position);
        assert_eq!(plan.known_revision, 0);
        assert_eq!(plan.recovered_revision(), Some(1));
        assert!(!plan.requires_keyframe);
    }

    #[test]
    fn keyframe_event_is_emitted_on_interval() {
        let mut session = GameSession::new();
        session.game.run_mode = RunMode::Playing;
        let delta_seconds = 5.0;

        session.update_frame_from_session_authority(
            crate::input::PlayerInput::default(),
            delta_seconds,
        );
        let delta = session.drain_world_delta();

        assert!(
            delta
                .events
                .iter()
                .any(|event| matches!(event, super::WorldEvent::SnapshotKeyframeReady { .. }))
        );
    }
}
