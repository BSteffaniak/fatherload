#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "world coordinates intentionally cross integer tile and floating render spaces"
)]

use std::{
    fmt::Write as _,
    mem,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub use drillgame_simulation::{
    ActiveWorldEvent, DrillDirection, DrillState, Expedition, ExpeditionObjectiveKind,
    FallingBoulder, HazardCloud, InfrastructureKind, LegendaryBlueprint, PlacedBomb,
    PlacedInfrastructure, RecipeKind, RigPartKind, RigSlot, ScanMarker, SessionServiceRequest,
    SideContract, SideContractKind, TILE_SIZE, WorldEventKind, default_infrastructure_durability,
};

use crate::{
    contract::ContractLog,
    economy::{DeepClaimStatus, SurfaceZone, TownBuilding, TownDevelopment, upgrade_offers},
    input::PlayerInput,
    multiplayer::{PlayerCommand, QuinnSessionTickSummary, SocketDrivenCorrectionSummary},
    online_session::{is_allowed_descriptor_path_character, is_allowed_socket_address_character},
    player::Player,
    save::{
        load_game, load_latest_game, save_exists, save_legacy_shell_game, save_slot_count,
        saves_exist,
    },
    surface::surface_building_at_tile,
    terrain::{
        ArtifactKind, DeepStratum, MineResult, MineralKind, StrategicResourceKind, Terrain,
        TileKind, TilePosition, deep_stratum_at_depth,
    },
};

const WORLD_WIDTH: i32 = 240;
const WORLD_HEIGHT: i32 = 220;
const GRAVITY: f32 = 780.0;
const HORIZONTAL_ACCELERATION: f32 = 900.0;
const THRUST_ACCELERATION: f32 = 1_250.0;
const MAX_HORIZONTAL_SPEED: f32 = 260.0;
const MAX_FALL_SPEED: f32 = 560.0;
const DRAG: f32 = 0.86;
const FUEL_BURN_PER_SECOND: f32 = 5.0;
const DRILL_FUEL_COST: f32 = 0.45;
const PLAYER_RADIUS: f32 = 10.5;
const SAFE_LANDING_SPEED: f32 = 330.0;
const CRASH_DAMAGE_SCALE: f32 = 0.11;
const BOULDER_DAMAGE: f32 = 8.0;
const BOULDER_WARNING_SECONDS: f32 = 0.85;
const BOULDER_SPAWN_CHANCE: u64 = 16;
const HEAT_START_DEPTH: f32 = 18.0 * TILE_SIZE;
const HEAT_DAMAGE_PER_SECOND: f32 = 3.5;
const CAMERA_SMOOTHING: f32 = 8.0;
const SKY_FLIGHT_HEIGHT_TILES: f32 = 12.0;
const MIN_PLAYER_Y: f32 = -SKY_FLIGHT_HEIGHT_TILES * TILE_SIZE;
const EXPLORATION_VISUAL_CHANGE_RADIUS_TILES: i32 = 12;
const CAMERA_INTRO_SECONDS: f32 = 1.0;
const CAMERA_INTRO_DROP_DISTANCE: f32 = 260.0;
const WORLD_SEED: u64 = 0xD1_11_6A_4E;
const PLAYER_SPAWN_X: f32 = 97.0 * TILE_SIZE;
const PLAYER_SPAWN_Y: f32 = 4.0 * TILE_SIZE;

const fn default_master_volume() -> f32 {
    0.8
}

const fn default_fullscreen() -> bool {
    false
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ChallengeBadge {
    DeepReturn,
    HighRiskReturn,
    ValuableHaul,
}

impl ChallengeBadge {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::DeepReturn => "Deep Return",
            Self::HighRiskReturn => "High-Risk Return",
            Self::ValuableHaul => "Valuable Haul",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CosmeticRigSkin {
    BronzeTrim,
    HazardStripes,
    StarChrome,
}

impl CosmeticRigSkin {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::BronzeTrim => "Bronze Trim",
            Self::HazardStripes => "Hazard Stripes",
            Self::StarChrome => "Star Chrome",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplayDomainStatus {
    pub movement: &'static str,
    pub terrain: &'static str,
    pub cargo_economy: &'static str,
    pub survival: &'static str,
    pub inventory: &'static str,
    pub menu_boundary: &'static str,
    pub reconnect_recovery: &'static str,
    pub authority_correction: &'static str,
    pub status: String,
}

impl OnlineGameplayDomainStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let movement = domain_status(!game.online_last_replicated_player_status.is_empty());
        let terrain = domain_status(
            !game.online_last_terrain_status.is_empty()
                || game.online_last_sync_loop_status.contains("terrain"),
        );
        let cargo_economy = domain_status(
            game.online_last_replicated_player_status.contains("cargo=")
                || game
                    .online_last_replicated_player_status
                    .contains("credits=")
                || game.online_last_sync_loop_status.contains("cargo=yes"),
        );
        let survival = domain_status(
            game.online_last_replicated_player_status.contains("fuel=")
                || game.online_last_replicated_player_status.contains("hull="),
        );
        let inventory = domain_status(
            !game.rig_part_inventory.is_empty()
                || !game.equipped_rig_parts.is_empty()
                || !game.cosmetic_skins.is_empty()
                || !game.challenge_badges.is_empty(),
        );
        let menu_boundary = domain_status(true);
        let reconnect_recovery = domain_status(
            game.online_last_ownership_status
                .contains("reconnect_context=preserved")
                || game.online_last_failure_status.contains("Reconnect")
                || game.online_session_state == OnlineSessionUxState::Reconnecting,
        );
        let authority_correction = domain_status(
            !game.online_last_authority_status.is_empty()
                || !game.online_last_correction_status.is_empty(),
        );
        let status = format!(
            "Online gameplay domains: movement={movement} terrain={terrain} cargo_economy={cargo_economy} survival={survival} inventory={inventory} menu_boundary={menu_boundary} reconnect_recovery={reconnect_recovery} authority_correction={authority_correction} | run_mode={:?} modal={} slot={} role={}",
            game.run_mode,
            game.modal
                .as_ref()
                .map_or_else(|| "none".to_owned(), |modal| format!("{modal:?}")),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            game.online_role_label()
        );
        Self {
            movement,
            terrain,
            cargo_economy,
            survival,
            inventory,
            menu_boundary,
            reconnect_recovery,
            authority_correction,
            status,
        }
    }
}

const fn domain_status(visible: bool) -> &'static str {
    if visible { "visible" } else { "missing" }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineSyncEvidenceQuality {
    Missing,
    DiagnosticOnly,
    LiveReplicated,
}

impl OnlineSyncEvidenceQuality {
    const fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::DiagnosticOnly => "diagnostic-only",
            Self::LiveReplicated => "live-replicated",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplaySyncEvidenceMatrix {
    pub movement: OnlineSyncEvidenceQuality,
    pub drilling_terrain: OnlineSyncEvidenceQuality,
    pub cargo_economy: OnlineSyncEvidenceQuality,
    pub survival_hazards: OnlineSyncEvidenceQuality,
    pub upgrades_inventory: OnlineSyncEvidenceQuality,
    pub pause_menu_boundaries: OnlineSyncEvidenceQuality,
    pub reconnect_save_boundaries: OnlineSyncEvidenceQuality,
    pub authority_corrections: OnlineSyncEvidenceQuality,
    pub complete_for_mvp_loop: bool,
    pub status: String,
}

impl OnlineGameplaySyncEvidenceMatrix {
    #[allow(
        clippy::too_many_lines,
        reason = "sync evidence matrix intentionally evaluates each gameplay domain in one place"
    )]
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let movement = evidence_quality(
            !game.online_last_replicated_player_status.is_empty(),
            game.online_last_live_verification_status
                .contains("movement=visible")
                || game
                    .online_last_gameplay_domain_status
                    .contains("movement=visible"),
        );
        let drilling_terrain = evidence_quality(
            !game.online_last_terrain_status.is_empty()
                || game
                    .online_last_sync_loop_status
                    .contains("terrain chunk applied"),
            game.online_last_live_verification_status
                .contains("terrain=visible")
                || game
                    .online_last_gameplay_domain_status
                    .contains("terrain=visible"),
        );
        let cargo_economy = evidence_quality(
            game.online_last_replicated_player_status.contains("cargo=")
                || game
                    .online_last_replicated_player_status
                    .contains("credits=")
                || game.online_last_sync_loop_status.contains("cargo=yes"),
            game.online_last_live_verification_status
                .contains("cargo_economy=visible")
                || game
                    .online_last_gameplay_domain_status
                    .contains("cargo_economy=visible"),
        );
        let survival_hazards = evidence_quality(
            game.online_last_replicated_player_status.contains("fuel=")
                || game.online_last_replicated_player_status.contains("hull="),
            game.online_last_live_verification_status
                .contains("survival=visible")
                || game
                    .online_last_gameplay_domain_status
                    .contains("survival=visible"),
        );
        let upgrades_inventory = evidence_quality(
            !game.rig_part_inventory.is_empty()
                || !game.equipped_rig_parts.is_empty()
                || !game.cosmetic_skins.is_empty()
                || !game.challenge_badges.is_empty(),
            game.online_last_live_verification_status
                .contains("inventory=visible")
                || game
                    .online_last_gameplay_domain_status
                    .contains("inventory=visible"),
        );
        let pause_menu_boundaries = evidence_quality(
            game.online_last_gameplay_domain_status
                .contains("menu_boundary=visible"),
            game.modal.is_some() || game.run_mode != RunMode::Playing,
        );
        let reconnect_save_boundaries = evidence_quality(
            game.online_last_ownership_status
                .contains("reconnect_context=preserved")
                || game
                    .online_last_save_boundary_status
                    .contains("Online save boundary")
                || game
                    .online_last_session_boundary_status
                    .contains("session boundary"),
            game.online_session_state == OnlineSessionUxState::Reconnecting
                || !game.online_last_save_boundary_status.is_empty(),
        );
        let authority_corrections = evidence_quality(
            !game.online_last_authority_status.is_empty()
                || !game.online_last_correction_status.is_empty(),
            game.online_last_gameplay_domain_status
                .contains("authority_correction=visible"),
        );
        let complete_for_mvp_loop = movement == OnlineSyncEvidenceQuality::LiveReplicated
            && drilling_terrain == OnlineSyncEvidenceQuality::LiveReplicated
            && cargo_economy == OnlineSyncEvidenceQuality::LiveReplicated;
        let status = format!(
            "Online gameplay sync evidence: movement={} drilling_terrain={} cargo_economy={} survival_hazards={} upgrades_inventory={} pause_menu_boundaries={} reconnect_save_boundaries={} authority_corrections={} mvp_loop_complete={}",
            movement.label(),
            drilling_terrain.label(),
            cargo_economy.label(),
            survival_hazards.label(),
            upgrades_inventory.label(),
            pause_menu_boundaries.label(),
            reconnect_save_boundaries.label(),
            authority_corrections.label(),
            yes_no(complete_for_mvp_loop)
        );
        Self {
            movement,
            drilling_terrain,
            cargo_economy,
            survival_hazards,
            upgrades_inventory,
            pause_menu_boundaries,
            reconnect_save_boundaries,
            authority_corrections,
            complete_for_mvp_loop,
            status,
        }
    }
}

const fn evidence_quality(live: bool, diagnostic: bool) -> OnlineSyncEvidenceQuality {
    if live {
        OnlineSyncEvidenceQuality::LiveReplicated
    } else if diagnostic {
        OnlineSyncEvidenceQuality::DiagnosticOnly
    } else {
        OnlineSyncEvidenceQuality::Missing
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "live verification status reports independent gameplay-system evidence flags"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLiveVerificationStatus {
    pub movement_visible: bool,
    pub terrain_visible: bool,
    pub cargo_economy_visible: bool,
    pub survival_visible: bool,
    pub inventory_visible: bool,
    pub ready_start_visible: bool,
    pub session_boundary_visible: bool,
    pub status: String,
}

impl OnlineLiveVerificationStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let movement_visible = !game.online_last_replicated_player_status.is_empty();
        let terrain_visible = !game.online_last_terrain_status.is_empty()
            || game.online_last_sync_loop_status.contains("terrain");
        let cargo_economy_visible = game.online_last_replicated_player_status.contains("cargo=")
            || game
                .online_last_replicated_player_status
                .contains("credits=")
            || game.online_last_sync_loop_status.contains("cargo=yes");
        let survival_visible = game.online_last_replicated_player_status.contains("fuel=")
            || game.online_last_replicated_player_status.contains("hull=");
        let inventory_visible = !game.rig_part_inventory.is_empty()
            || !game.equipped_rig_parts.is_empty()
            || !game.cosmetic_skins.is_empty()
            || !game.challenge_badges.is_empty();
        let start_gate = game.online_gameplay_start_gate();
        let ready_start_visible = start_gate.ready
            || matches!(
                start_gate.blocker,
                Some(
                    OnlineGameplayStartBlocker::NotConnected
                        | OnlineGameplayStartBlocker::RemoteNotConnected
                        | OnlineGameplayStartBlocker::LocalNotReady
                        | OnlineGameplayStartBlocker::RemoteNotReady
                        | OnlineGameplayStartBlocker::HostAuthorityRequired,
                )
            );
        let session_boundary_visible = !game.online_last_session_boundary_status.is_empty();
        let status = format!(
            "Online live verification: movement={} terrain={} cargo_economy={} survival={} inventory={} ready_start={} session_boundary={}",
            yes_no(movement_visible),
            yes_no(terrain_visible),
            yes_no(cargo_economy_visible),
            yes_no(survival_visible),
            yes_no(inventory_visible),
            yes_no(ready_start_visible),
            yes_no(session_boundary_visible)
        );
        Self {
            movement_visible,
            terrain_visible,
            cargo_economy_visible,
            survival_visible,
            inventory_visible,
            ready_start_visible,
            session_boundary_visible,
            status,
        }
    }
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineOwnershipStatus {
    pub identity: String,
    pub slot: Option<u8>,
    pub role_label: &'static str,
    pub save_authority: OnlineSaveAuthority,
    pub host_owns_save: bool,
    pub reconnect_allowed: bool,
    pub player_message: String,
}

impl OnlineOwnershipStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let save_authority = if game.online_host_owns_save {
            OnlineSaveAuthority::LocalPlayer
        } else {
            OnlineSaveAuthority::RemoteHost
        };
        let reconnect_allowed = matches!(
            game.online_session_state,
            OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
                | OnlineSessionUxState::Shutdown
        );
        let player_message = if game.online_host_owns_save {
            "You are the host/save owner; local save/load remains allowed when policy permits."
                .to_owned()
        } else {
            "You are a joined client; host owns the authoritative save, so local writes stay blocked."
                .to_owned()
        };
        Self {
            identity: game.online_player_name.clone(),
            slot: game.online_player_slot,
            role_label: game.online_role_label(),
            save_authority,
            host_owns_save: game.online_host_owns_save,
            reconnect_allowed,
            player_message,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Reconnect ownership / Online ownership: identity={} slot={} role={} save_authority={:?} save_owner={} host_owns_save={} reconnect_context={} | {}",
            self.identity,
            self.slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            self.role_label,
            self.save_authority,
            if self.host_owns_save {
                "local-host"
            } else {
                "remote-host"
            },
            if self.host_owns_save { "yes" } else { "no" },
            if self.reconnect_allowed {
                "preserved"
            } else {
                "pending"
            },
            self.player_message
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineFailureCategory {
    VersionMismatch,
    Certificate,
    Descriptor,
    Timeout,
    RefusedOrUnreachable,
    Reconnect,
    SessionEnded,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineFailureStatus {
    pub category: OnlineFailureCategory,
    pub player_message: String,
    pub troubleshooting_hint: &'static str,
}

impl OnlineFailureStatus {
    #[must_use]
    pub fn classify(error: &str) -> Self {
        let normalized = error.to_ascii_lowercase();
        if normalized.contains("no usable drillgame lan hosts") {
            return Self {
                category: OnlineFailureCategory::Descriptor,
                player_message: error.to_owned(),
                troubleshooting_hint: "The app mDNS scan did not produce a usable resolved host; compare these counts with dns-sd -L output.",
            };
        }
        if normalized.contains("version") || normalized.contains("protocol") {
            return Self {
                category: OnlineFailureCategory::VersionMismatch,
                player_message: "Connection error: game version/protocol mismatch. Update both players to the same build."
                    .to_owned(),
                troubleshooting_hint: "Both players must run the same build/protocol version.",
            };
        }
        if normalized.contains("certificate") || normalized.contains("cert") {
            return Self {
                category: OnlineFailureCategory::Certificate,
                player_message: "Connection error: host descriptor certificate could not be trusted. Regenerate and re-share the descriptor."
                    .to_owned(),
                troubleshooting_hint: "Use the current descriptor generated by the host; do not edit certificate fields.",
            };
        }
        if normalized.contains("descriptor")
            || normalized.contains("json")
            || normalized.contains("parse")
        {
            return Self {
                category: OnlineFailureCategory::Descriptor,
                player_message: "Connection error: host descriptor could not be read. Check the descriptor file/path and ask the host to share it again."
                    .to_owned(),
                troubleshooting_hint: "Verify descriptor path, file permissions, and that the host exported a fresh descriptor.",
            };
        }
        if normalized.contains("timeout") || normalized.contains("timed out") {
            return Self {
                category: OnlineFailureCategory::Timeout,
                player_message: "Connection timed out: verify the host is running, the advertised UDP port is open, and both machines are on the expected LAN/VPN."
                    .to_owned(),
                troubleshooting_hint: "Check LAN/VPN reachability, host advertise address, and UDP firewall rules.",
            };
        }
        if normalized.contains("refused") || normalized.contains("unreachable") {
            return Self {
                category: OnlineFailureCategory::RefusedOrUnreachable,
                player_message: "Connection refused or unreachable: verify host address, firewall, and UDP port forwarding/LAN routing."
                    .to_owned(),
                troubleshooting_hint: "Confirm the host listener is active and the advertised socket address is reachable.",
            };
        }
        if normalized.contains("reconnect") || normalized.contains("session token") {
            return Self {
                category: OnlineFailureCategory::Reconnect,
                player_message: "Reconnect failed: the previous online session is no longer available. Rejoin from the host descriptor."
                    .to_owned(),
                troubleshooting_hint: "Ask the host for the current descriptor and rejoin as a new client.",
            };
        }
        if normalized.contains("shutdown") || normalized.contains("closed") {
            return Self {
                category: OnlineFailureCategory::SessionEnded,
                player_message: "Online session ended: the host closed the session.".to_owned(),
                troubleshooting_hint: "Return to the online menu and host or join a new session.",
            };
        }
        Self {
            category: OnlineFailureCategory::Unknown,
            player_message: format!("Connection error: {error}"),
            troubleshooting_hint: "Capture the exact error text and retry from the online menu.",
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online failure: category={:?} | {} | hint={}",
            self.category, self.player_message, self.troubleshooting_hint
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineSessionBoundaryCause {
    HostEndedSession,
    ClientLeftSession,
    TransportClosed,
    LocalShutdownRequested,
    ShutdownAcknowledged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSessionBoundaryStatus {
    pub cause: OnlineSessionBoundaryCause,
    pub remote_connected: bool,
    pub local_session_active: bool,
    pub player_message: String,
}

impl OnlineSessionBoundaryStatus {
    #[must_use]
    pub fn host_ended(reason: &str) -> Self {
        Self {
            cause: OnlineSessionBoundaryCause::HostEndedSession,
            remote_connected: false,
            local_session_active: false,
            player_message: format!(
                "Online session ended by host: {reason}. Return to the online menu to reconnect or start a new session."
            ),
        }
    }

    #[must_use]
    pub fn client_left(reason: &str) -> Self {
        Self {
            cause: OnlineSessionBoundaryCause::ClientLeftSession,
            remote_connected: false,
            local_session_active: true,
            player_message: format!(
                "Joined client left the online session: {reason}. Host save/session remains local and safe."
            ),
        }
    }

    #[must_use]
    pub fn transport_closed(error: &str) -> Self {
        Self {
            cause: OnlineSessionBoundaryCause::TransportClosed,
            remote_connected: false,
            local_session_active: false,
            player_message: format!(
                "Online session ended by host or transport closed unexpectedly: {error}. The session is stopped; local save policy remains unchanged."
            ),
        }
    }

    #[must_use]
    pub fn local_shutdown_requested() -> Self {
        Self {
            cause: OnlineSessionBoundaryCause::LocalShutdownRequested,
            remote_connected: false,
            local_session_active: false,
            player_message: "Online session shutdown requested; notifying peer when connected."
                .to_owned(),
        }
    }

    #[must_use]
    pub fn shutdown_acknowledged() -> Self {
        Self {
            cause: OnlineSessionBoundaryCause::ShutdownAcknowledged,
            remote_connected: false,
            local_session_active: false,
            player_message:
                "Online session shutdown acknowledged; local save/session state preserved."
                    .to_owned(),
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online session boundary: cause={:?} remote_connected={} local_session_active={} | {}",
            self.cause,
            if self.remote_connected { "yes" } else { "no" },
            if self.local_session_active {
                "yes"
            } else {
                "no"
            },
            self.player_message
        )
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "sync checklist status reports independent snapshot/delta/terrain/cargo coverage"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSyncLoopStatus {
    pub snapshot_applied: bool,
    pub player_delta_applied: bool,
    pub terrain_applied: bool,
    pub cargo_applied: bool,
    pub status: String,
}

impl OnlineSyncLoopStatus {
    #[must_use]
    pub fn snapshot(player_count: usize, cargo_applied: bool) -> Self {
        Self {
            snapshot_applied: true,
            player_delta_applied: false,
            terrain_applied: false,
            cargo_applied,
            status: format!(
                "snapshot applied: players={player_count} cargo={} survival/economy=yes",
                if cargo_applied { "yes" } else { "no" }
            ),
        }
    }

    #[must_use]
    pub fn player_delta(player_count: usize, visible_count: usize) -> Self {
        Self {
            snapshot_applied: false,
            player_delta_applied: visible_count > 0,
            terrain_applied: false,
            cargo_applied: false,
            status: format!(
                "player delta applied: ids={player_count} visible_remote_updates={visible_count}"
            ),
        }
    }

    #[must_use]
    pub fn terrain(tile_count: usize, visible_tiles: usize) -> Self {
        Self {
            snapshot_applied: false,
            player_delta_applied: false,
            terrain_applied: visible_tiles > 0,
            cargo_applied: false,
            status: format!(
                "terrain chunk applied: network_tiles={tile_count} visible_tiles={visible_tiles}"
            ),
        }
    }

    #[must_use]
    pub fn keyframe_required() -> Self {
        Self {
            snapshot_applied: false,
            player_delta_applied: false,
            terrain_applied: false,
            cargo_applied: false,
            status: "delta requested keyframe: waiting for host snapshot".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlinePeerLobbyPresentation {
    pub name: String,
    pub slot: Option<u8>,
    pub role_label: &'static str,
    pub ready: bool,
    pub connected: bool,
    pub save_authority: OnlineSaveAuthority,
}

impl OnlinePeerLobbyPresentation {
    #[must_use]
    pub fn line(&self, peer_label: &str) -> String {
        format!(
            "{peer_label}: {} | slot {} | role {} | ready {} | connected {} | save_authority {:?}",
            self.name,
            self.slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            self.role_label,
            if self.ready { "yes" } else { "no" },
            if self.connected { "yes" } else { "no" },
            self.save_authority
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineLobbyReadinessState {
    Ready,
    NotReady,
    WaitingForConnection,
}

impl OnlineLobbyReadinessState {
    const fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotReady => "not-ready",
            Self::WaitingForConnection => "waiting-for-connection",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLobbyStatus {
    pub local: OnlinePeerLobbyPresentation,
    pub remote: OnlinePeerLobbyPresentation,
    pub local_readiness: OnlineLobbyReadinessState,
    pub remote_readiness: OnlineLobbyReadinessState,
    pub can_start: bool,
    pub blocker: Option<OnlineGameplayStartBlocker>,
    pub status: String,
}

impl OnlineLobbyStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let presentation = game.online_lobby_presentation();
        let local_readiness = if presentation.local.ready {
            OnlineLobbyReadinessState::Ready
        } else {
            OnlineLobbyReadinessState::NotReady
        };
        let remote_readiness = if !presentation.remote.connected {
            OnlineLobbyReadinessState::WaitingForConnection
        } else if presentation.remote.ready {
            OnlineLobbyReadinessState::Ready
        } else {
            OnlineLobbyReadinessState::NotReady
        };
        let can_start = presentation.start_gate.ready;
        let blocker = presentation.start_gate.blocker;
        let status = format!(
            "Online lobby status: local={} slot={} role={} ready={} connected={} save_authority={:?}; remote={} slot={} role={} ready={} connected={} save_authority={:?}; start_ready={} blocker={:?}",
            presentation.local.name,
            presentation
                .local
                .slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            presentation.local.role_label,
            local_readiness.label(),
            yes_no(presentation.local.connected),
            presentation.local.save_authority,
            presentation.remote.name,
            presentation
                .remote
                .slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            presentation.remote.role_label,
            remote_readiness.label(),
            yes_no(presentation.remote.connected),
            presentation.remote.save_authority,
            yes_no(can_start),
            blocker
        );
        Self {
            local: presentation.local,
            remote: presentation.remote,
            local_readiness,
            remote_readiness,
            can_start,
            blocker,
            status,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLobbyPresentation {
    pub local: OnlinePeerLobbyPresentation,
    pub remote: OnlinePeerLobbyPresentation,
    pub start_gate: OnlineGameplayStartGate,
    pub guidance: String,
}

impl OnlineLobbyPresentation {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        vec![
            self.local.line("Local player"),
            self.remote.line("Remote player"),
            format!(
                "Lobby start gate: ready={} blocker={:?}",
                if self.start_gate.ready { "yes" } else { "no" },
                self.start_gate.blocker
            ),
            self.guidance.clone(),
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct OnlineTerrainSyncMarker {
    pub position: TilePosition,
    pub seconds_remaining: f32,
}

impl OnlineTerrainSyncMarker {
    const LIFETIME_SECONDS: f32 = 1.75;

    #[must_use]
    pub const fn new(position: TilePosition) -> Self {
        Self {
            position,
            seconds_remaining: Self::LIFETIME_SECONDS,
        }
    }

    #[must_use]
    pub fn intensity(&self) -> f32 {
        (self.seconds_remaining / Self::LIFETIME_SECONDS).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineTerrainSyncMarkerBatch {
    pub markers_added: usize,
    pub marker_count: usize,
    pub latest_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSessionLifecyclePresentation {
    pub active: bool,
    pub heading: String,
    pub safe_exit_line: String,
    pub remote_line: String,
    pub boundary_lines: Vec<String>,
}

impl OnlineSessionLifecyclePresentation {
    #[must_use]
    pub fn inactive() -> Self {
        Self {
            active: false,
            heading: "Session lifecycle".to_owned(),
            safe_exit_line: "No online session is active; exit/save follows normal local rules."
                .to_owned(),
            remote_line: "Remote: none".to_owned(),
            boundary_lines: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlinePauseSessionPresentation {
    pub active: bool,
    pub heading: String,
    pub lines: Vec<String>,
    pub primary_action: String,
    pub save_warning: Option<String>,
}

impl OnlinePauseSessionPresentation {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            active: false,
            heading: "Online session".to_owned(),
            lines: vec!["No online session is active.".to_owned()],
            primary_action: "Open Online Session menu to host or join.".to_owned(),
            save_warning: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineGameplayEntrySource {
    #[default]
    None,
    HostUiRequested,
    HostStartSent,
    HostStartReceived,
}

impl OnlineGameplayEntrySource {
    const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::HostUiRequested => "host-ui-requested",
            Self::HostStartSent => "host-start-sent",
            Self::HostStartReceived => "host-start-received",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplayEntryPresentation {
    pub entered_gameplay: bool,
    pub source: OnlineGameplayEntrySource,
    pub authoritative_tick: Option<crate::multiplayer::SimulationTick>,
    pub host_authoritative: bool,
    pub status: String,
}

impl OnlineGameplayEntryPresentation {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let entered_gameplay = game.run_mode == RunMode::Playing
            && game.modal.is_none()
            && game.online_session_state == OnlineSessionUxState::Connected;
        let host_authoritative = game.online_host_owns_save && game.online_player_slot == Some(1);
        let authoritative_tick = game.online_gameplay_entry_authoritative_tick;
        let source = game.online_gameplay_entry_source;
        let status = format!(
            "Gameplay entry: entered={} source={} authoritative_tick={} host_authoritative={} role={} slot={}",
            yes_no(entered_gameplay),
            source.label(),
            authoritative_tick.map_or_else(|| "pending".to_owned(), |tick| tick.get().to_string()),
            yes_no(host_authoritative),
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        );
        Self {
            entered_gameplay,
            source,
            authoritative_tick,
            host_authoritative,
            status,
        }
    }

    #[must_use]
    pub fn hud_line(&self) -> String {
        format!(
            "Gameplay entry: {} via {} at tick {}",
            if self.entered_gameplay {
                "active"
            } else {
                "waiting"
            },
            self.source.label(),
            self.authoritative_tick
                .map_or_else(|| "pending".to_owned(), |tick| tick.get().to_string())
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplayClarityPresentation {
    pub visible: bool,
    pub local_player_line: String,
    pub remote_player_lines: Vec<String>,
    pub session_state_line: String,
    pub hud_clear_enough: bool,
    pub status: String,
}

impl OnlineGameplayClarityPresentation {
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "HUD depth and cargo summaries intentionally display compact integral values"
    )]
    pub fn from_game(game: &GameState) -> Self {
        let visible = matches!(
            game.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
                | OnlineSessionUxState::Timeout
                | OnlineSessionUxState::Error
                | OnlineSessionUxState::Shutdown
        );
        let local_depth = (game.player.y / TILE_SIZE).floor() as i32;
        let local_player_line = format!(
            "Local {} p{} depth={} fuel={:.0}/{:.0} hull={:.0}/{:.0} credits={} cargo={}/{}",
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "?".to_owned(), |slot| slot.to_string()),
            local_depth,
            game.player.fuel,
            game.player.fuel_capacity,
            game.player.hull,
            game.player.max_hull(),
            game.player.credits,
            game.player.cargo_used(),
            game.player.cargo_capacity
        );
        let remote_player_lines = vec![if game.online_remote_player_connected {
            "Remote player connected; canonical session presentation active.".to_owned()
        } else {
            "Remote player not connected yet.".to_owned()
        }];
        let save_policy = game.online_save_exit_policy();
        let session_state_line = format!(
            "Session {:?} | ready local={} remote={} | save_dirty={} local_save={} | {}",
            game.online_session_state,
            yes_no(game.online_local_ready),
            yes_no(game.online_remote_player_ready),
            yes_no(game.save_dirty),
            yes_no(save_policy.local_save_allowed),
            game.online_gameplay_entry_source.label()
        );
        let remote_state_clear = game.online_remote_player_connected;
        let hud_clear_enough = visible
            && game.online_player_slot.is_some()
            && game.player.fuel.is_finite()
            && game.player.hull.is_finite()
            && remote_state_clear;
        let status = format!(
            "Online gameplay HUD clarity: clear={} visible={} local_slot={} remote_connected={} save_dirty={} local_save={} role={}",
            yes_no(hud_clear_enough),
            yes_no(visible),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            yes_no(game.online_remote_player_connected),
            yes_no(game.save_dirty),
            yes_no(save_policy.local_save_allowed),
            game.online_role_label()
        );
        Self {
            visible,
            local_player_line,
            remote_player_lines,
            session_state_line,
            hud_clear_enough,
            status,
        }
    }

    #[must_use]
    pub fn hud_lines(&self) -> Vec<String> {
        if !self.visible {
            return Vec::new();
        }
        let mut lines = vec![self.local_player_line.clone()];
        lines.extend(self.remote_player_lines.iter().cloned());
        lines.push(self.session_state_line.clone());
        lines
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplayHudPresentation {
    pub visible: bool,
    pub role_label: &'static str,
    pub slot_label: String,
    pub local_ready_label: &'static str,
    pub remote_label: String,
    pub remote_ready_label: &'static str,
    pub save_policy_label: String,
    pub session_label: String,
    pub gameplay_entry_label: String,
    pub directional_sync_label: String,
    pub replication_label: String,
    pub terrain_label: String,
    pub authority_label: String,
    pub clarity_lines: Vec<String>,
}

impl OnlineGameplayHudPresentation {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        if !self.visible {
            return Vec::new();
        }
        let mut lines = vec![
            format!(
                "Online {} slot {} | local {} | remote {} {}",
                self.role_label,
                self.slot_label,
                self.local_ready_label,
                self.remote_label,
                self.remote_ready_label
            ),
            self.save_policy_label.clone(),
            self.session_label.clone(),
            self.gameplay_entry_label.clone(),
            self.directional_sync_label.clone(),
            self.replication_label.clone(),
            self.terrain_label.clone(),
            self.authority_label.clone(),
        ];
        lines.extend(self.clarity_lines.iter().cloned());
        lines
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RunMode {
    Title,
    Playing,
    Interior,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionShellUpdateSummary {
    ShellHandled,
    GameplayPresentationUpdated,
    Paused,
    ExitRequested,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineAddressEditTarget {
    HostBind,
    HostAdvertise,
    ClientBind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineAddressValidationSeverity {
    Accepted,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineAddressValidation {
    pub severity: OnlineAddressValidationSeverity,
    pub target: OnlineAddressEditTarget,
    pub address: Option<SocketAddr>,
    pub message: String,
}

impl OnlineAddressValidation {
    #[must_use]
    pub fn accepted(target: OnlineAddressEditTarget, address: SocketAddr) -> Self {
        Self {
            severity: OnlineAddressValidationSeverity::Accepted,
            target,
            address: Some(address),
            message: format!(
                "{} address accepted: {address}",
                GameState::online_address_edit_target_label(target)
            ),
        }
    }

    #[must_use]
    pub const fn warning(
        target: OnlineAddressEditTarget,
        address: SocketAddr,
        message: String,
    ) -> Self {
        Self {
            severity: OnlineAddressValidationSeverity::Warning,
            target,
            address: Some(address),
            message,
        }
    }

    #[must_use]
    pub const fn error(target: OnlineAddressEditTarget, message: String) -> Self {
        Self {
            severity: OnlineAddressValidationSeverity::Error,
            target,
            address: None,
            message,
        }
    }

    #[must_use]
    pub const fn is_accepted(&self) -> bool {
        matches!(
            self.severity,
            OnlineAddressValidationSeverity::Accepted | OnlineAddressValidationSeverity::Warning
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineDescriptorInputMode {
    HostWrite,
    JoinRead,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineDescriptorInputStatus {
    pub mode: OnlineDescriptorInputMode,
    pub path: PathBuf,
    pub accepted: bool,
    pub can_attempt_task: bool,
    pub message: String,
}

impl OnlineDescriptorInputStatus {
    #[must_use]
    pub fn validate(mode: OnlineDescriptorInputMode, path: &Path) -> Self {
        let display_path = path.display().to_string();
        if display_path.trim().is_empty() {
            return Self::blocked(
                mode,
                path,
                "Descriptor path is empty; choose a JSON descriptor file path.".to_owned(),
            );
        }
        if !display_path
            .chars()
            .all(is_allowed_descriptor_path_character)
        {
            return Self::blocked(
                mode,
                path,
                "Descriptor path contains unsupported characters; use letters, numbers, spaces, '.', '-', '_', ':', '~', '/', or '\\'."
                    .to_owned(),
            );
        }
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("json") {
            return Self::blocked(
                mode,
                path,
                "Descriptor path must end in .json so it can be shared and inspected safely."
                    .to_owned(),
            );
        }
        match mode {
            OnlineDescriptorInputMode::HostWrite => {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                    && !parent.exists()
                {
                    return Self::blocked(
                        mode,
                        path,
                        format!(
                            "Descriptor parent folder does not exist: {}",
                            parent.display()
                        ),
                    );
                }
                Self::accepted(
                    mode,
                    path,
                    format!(
                        "Host descriptor path accepted: {display_path}. Host will write this JSON file for sharing."
                    ),
                )
            }
            OnlineDescriptorInputMode::JoinRead => {
                if !path.exists() {
                    return Self::pending(
                        mode,
                        path,
                        format!(
                            "Join descriptor file not found yet: {display_path}. Queuing join will surface the task failure if the host has not shared the current JSON descriptor."
                        ),
                    );
                }
                if !path.is_file() {
                    return Self::blocked(
                        mode,
                        path,
                        format!("Join descriptor path is not a file: {display_path}"),
                    );
                }
                Self::accepted(
                    mode,
                    path,
                    format!(
                        "Join descriptor path accepted: {display_path}. Inspecting/connecting can proceed."
                    ),
                )
            }
        }
    }

    fn accepted(mode: OnlineDescriptorInputMode, path: &Path, message: String) -> Self {
        Self {
            mode,
            path: path.to_path_buf(),
            accepted: true,
            can_attempt_task: true,
            message,
        }
    }

    fn pending(mode: OnlineDescriptorInputMode, path: &Path, message: String) -> Self {
        Self {
            mode,
            path: path.to_path_buf(),
            accepted: false,
            can_attempt_task: true,
            message,
        }
    }

    fn blocked(mode: OnlineDescriptorInputMode, path: &Path, message: String) -> Self {
        Self {
            mode,
            path: path.to_path_buf(),
            accepted: false,
            can_attempt_task: false,
            message,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online descriptor input: mode={:?} path={} accepted={} can_attempt_task={} | {}",
            self.mode,
            self.path.display(),
            yes_no(self.accepted),
            yes_no(self.can_attempt_task),
            self.message
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineDescriptorInspectionSeverity {
    Pending,
    Valid,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineDescriptorInspectionPresentation {
    pub severity: OnlineDescriptorInspectionSeverity,
    pub heading: String,
    pub lines: Vec<String>,
    pub can_join: bool,
}

impl OnlineDescriptorInspectionPresentation {
    #[must_use]
    pub fn pending(path: &Path) -> Self {
        Self {
            severity: OnlineDescriptorInspectionSeverity::Pending,
            heading: "Descriptor inspection".to_owned(),
            lines: vec![format!(
                "No descriptor inspected yet. Current path: {}",
                path.display()
            )],
            can_join: false,
        }
    }

    #[must_use]
    pub fn valid(
        path: &Path,
        descriptor: &crate::multiplayer::QuinnHostConnectionDescriptor,
    ) -> Self {
        Self {
            severity: OnlineDescriptorInspectionSeverity::Valid,
            heading: "Descriptor OK".to_owned(),
            lines: vec![
                format!("Path: {}", path.display()),
                format!("Host address: {}", descriptor.host_addr),
                format!("Server name: {}", descriptor.server_name),
                format!("Certificate: {} bytes", descriptor.certificate_der.len()),
                format!("cert={} bytes", descriptor.certificate_der.len()),
                "Join will trust this descriptor certificate for direct connect.".to_owned(),
            ],
            can_join: true,
        }
    }

    #[must_use]
    pub fn warning(path: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: OnlineDescriptorInspectionSeverity::Warning,
            heading: "Descriptor pending".to_owned(),
            lines: vec![format!("Path: {}", path.display()), message.into()],
            can_join: false,
        }
    }

    #[must_use]
    pub fn error(path: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: OnlineDescriptorInspectionSeverity::Error,
            heading: "Descriptor problem".to_owned(),
            lines: vec![format!("Path: {}", path.display()), message.into()],
            can_join: false,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!("{}: {}", self.heading, self.lines.join(" | "))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineSaveAuthority {
    LocalPlayer,
    RemoteHost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineUnsavedExitAction {
    SaveAndExitAllowed,
    DiscardOrCancelOnly,
    CleanExitAllowed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnlineSaveExitPolicy {
    pub save_authority: OnlineSaveAuthority,
    pub local_save_allowed: bool,
    pub local_load_allowed: bool,
    pub save_before_exit_allowed: bool,
    pub unsaved_exit_action: OnlineUnsavedExitAction,
}

impl OnlineSaveExitPolicy {
    #[must_use]
    pub const fn status_line(self) -> &'static str {
        match (
            self.save_authority,
            self.local_save_allowed,
            self.local_load_allowed,
            self.unsaved_exit_action,
        ) {
            (
                OnlineSaveAuthority::LocalPlayer,
                true,
                true,
                OnlineUnsavedExitAction::SaveAndExitAllowed,
            ) => "Local host owns save: save/load/save-before-exit allowed.",
            (
                OnlineSaveAuthority::LocalPlayer,
                true,
                true,
                OnlineUnsavedExitAction::CleanExitAllowed,
            ) => "Local host owns save: clean exit allowed; save/load still available.",
            (
                OnlineSaveAuthority::RemoteHost,
                false,
                false,
                OnlineUnsavedExitAction::DiscardOrCancelOnly,
            ) => {
                "Joined client: host owns the online save; local save/load blocked, discard or cancel unsaved exit."
            }
            (
                OnlineSaveAuthority::RemoteHost,
                false,
                false,
                OnlineUnsavedExitAction::CleanExitAllowed,
            ) => {
                "Joined client: host owns the online save; local save/load blocked, clean exit allowed."
            }
            _ => "Online save policy: mixed state; verify connection ownership before save/load.",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineLocalPersistenceAction {
    Save,
    Load,
}

impl OnlineLocalPersistenceAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::Load => "load",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLocalPersistenceDecision {
    pub action: OnlineLocalPersistenceAction,
    pub allowed: bool,
    pub save_authority: OnlineSaveAuthority,
    pub player_message: String,
    pub status: String,
}

impl OnlineLocalPersistenceDecision {
    #[must_use]
    pub fn from_game(game: &GameState, action: OnlineLocalPersistenceAction) -> Self {
        let policy = game.online_save_exit_policy();
        let allowed = match action {
            OnlineLocalPersistenceAction::Save => policy.local_save_allowed,
            OnlineLocalPersistenceAction::Load => policy.local_load_allowed,
        };
        let player_message = match (action, allowed, policy.save_authority) {
            (OnlineLocalPersistenceAction::Save, true, OnlineSaveAuthority::LocalPlayer) => {
                "Save allowed: this instance owns the local/host session save.".to_owned()
            }
            (OnlineLocalPersistenceAction::Load, true, OnlineSaveAuthority::LocalPlayer) => {
                "Load allowed: this instance owns local save selection.".to_owned()
            }
            (OnlineLocalPersistenceAction::Save, false, OnlineSaveAuthority::RemoteHost) => {
                "Save blocked: host owns the online session save; joined clients cannot write local saves.".to_owned()
            }
            (OnlineLocalPersistenceAction::Load, false, OnlineSaveAuthority::RemoteHost) => {
                "Load blocked: host owns the online session save; leave the session before loading a local save.".to_owned()
            }
            _ => format!(
                "{} blocked: verify online save ownership before local persistence.",
                action.label()
            ),
        };
        let status = format!(
            "Online local persistence: action={} allowed={} authority={:?} dirty={} role={} slot={} | {}",
            action.label(),
            yes_no(allowed),
            policy.save_authority,
            yes_no(game.save_dirty),
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            player_message
        );
        Self {
            action,
            allowed,
            save_authority: policy.save_authority,
            player_message,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "leave/end safety reports independent shutdown and save-corruption safeguards"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLeaveEndSafetyStatus {
    pub session_active: bool,
    pub shutdown_requestable: bool,
    pub shutdown_pending: bool,
    pub shutdown_acknowledged: bool,
    pub peer_notification_evidence: bool,
    pub save_corruption_guarded: bool,
    pub force_kill_not_required: bool,
    pub status: String,
}

impl OnlineLeaveEndSafetyStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let session_active = matches!(
            game.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
        );
        let shutdown_requestable = session_active
            && game.online_network_task_request != Some(OnlineNetworkTaskRequest::Shutdown);
        let shutdown_pending = game.online_network_task_request
            == Some(OnlineNetworkTaskRequest::Shutdown)
            || game
                .online_last_session_boundary_status
                .contains("local-shutdown-requested");
        let shutdown_acknowledged = game.online_session_state == OnlineSessionUxState::Shutdown
            || game
                .online_last_session_boundary_status
                .contains("shutdown-acknowledged");
        let peer_notification_evidence =
            game.online_last_shutdown_summary.contains("peer notified")
                || game
                    .online_last_shutdown_summary
                    .contains("no peer notification needed")
                || game
                    .online_last_session_boundary_status
                    .contains("host-ended")
                || game
                    .online_last_session_boundary_status
                    .contains("client-left")
                || game
                    .online_last_session_boundary_status
                    .contains("transport-closed");
        let save_policy = game.online_save_exit_policy();
        let save_corruption_guarded = save_policy.local_save_allowed
            || save_policy.unsaved_exit_action == OnlineUnsavedExitAction::DiscardOrCancelOnly
            || shutdown_acknowledged;
        let force_kill_not_required = shutdown_requestable
            || shutdown_pending
            || shutdown_acknowledged
            || peer_notification_evidence;
        let status = format!(
            "Online leave/end safety: force_kill_not_required={} active={} requestable={} pending={} acknowledged={} peer_notice={} save_guarded={} role={} slot={}",
            yes_no(force_kill_not_required),
            yes_no(session_active),
            yes_no(shutdown_requestable),
            yes_no(shutdown_pending),
            yes_no(shutdown_acknowledged),
            yes_no(peer_notification_evidence),
            yes_no(save_corruption_guarded),
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        );
        Self {
            session_active,
            shutdown_requestable,
            shutdown_pending,
            shutdown_acknowledged,
            peer_notification_evidence,
            save_corruption_guarded,
            force_kill_not_required,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "save boundary UX reports independent save/load/exit permission flags"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSaveBoundaryStatus {
    pub save_authority: OnlineSaveAuthority,
    pub dirty: bool,
    pub local_save_allowed: bool,
    pub local_load_allowed: bool,
    pub save_before_exit_allowed: bool,
    pub unsaved_exit_action: OnlineUnsavedExitAction,
    pub player_message: String,
}

impl OnlineSaveBoundaryStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let policy = game.online_save_exit_policy();
        let player_message = match (
            policy.save_authority,
            game.save_dirty,
            policy.unsaved_exit_action,
        ) {
            (
                OnlineSaveAuthority::LocalPlayer,
                true,
                OnlineUnsavedExitAction::SaveAndExitAllowed,
            ) => "Host-owned dirty save: Save+Exit, local save, and local load are allowed."
                .to_owned(),
            (
                OnlineSaveAuthority::LocalPlayer,
                false,
                OnlineUnsavedExitAction::CleanExitAllowed,
            ) => "Host-owned clean save: exiting is safe and local save/load remain available."
                .to_owned(),
            (
                OnlineSaveAuthority::RemoteHost,
                true,
                OnlineUnsavedExitAction::DiscardOrCancelOnly,
            ) => "Joined client has unsaved local changes, but host owns the online save; Save+Exit is blocked, use Discard or Cancel."
                .to_owned(),
            (
                OnlineSaveAuthority::RemoteHost,
                false,
                OnlineUnsavedExitAction::CleanExitAllowed,
            ) => "Joined client is clean; host owns the online save, so local save/load remain blocked."
                .to_owned(),
            _ => "Online save boundary is mixed; verify host/client ownership before saving or loading."
                .to_owned(),
        };
        Self {
            save_authority: policy.save_authority,
            dirty: game.save_dirty,
            local_save_allowed: policy.local_save_allowed,
            local_load_allowed: policy.local_load_allowed,
            save_before_exit_allowed: policy.save_before_exit_allowed,
            unsaved_exit_action: policy.unsaved_exit_action,
            player_message,
        }
    }

    #[must_use]
    pub fn blocked_save(message: &str) -> Self {
        Self {
            save_authority: OnlineSaveAuthority::RemoteHost,
            dirty: true,
            local_save_allowed: false,
            local_load_allowed: false,
            save_before_exit_allowed: false,
            unsaved_exit_action: OnlineUnsavedExitAction::DiscardOrCancelOnly,
            player_message: message.to_owned(),
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online save boundary: authority={:?} dirty={} local_save_allowed={} local_load_allowed={} save_before_exit_allowed={} unsaved_exit={:?} | {}",
            self.save_authority,
            yes_no(self.dirty),
            yes_no(self.local_save_allowed),
            yes_no(self.local_load_allowed),
            yes_no(self.save_before_exit_allowed),
            self.unsaved_exit_action,
            self.player_message
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionSlotIoRequest {
    Save { slot: usize },
    Load { slot: usize },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineSessionUxState {
    Idle,
    Hosting,
    Joining,
    Connected,
    Reconnecting,
    Timeout,
    Error,
    Disconnected,
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineNetworkTaskRequest {
    HostDirectConnect,
    JoinDirectConnect,
    HostDescriptorFile { path: PathBuf },
    HostLanGame,
    JoinLanGame,
    JoinDescriptorFile { path: PathBuf },
    ReconnectDirectConnect,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OnlineMultiplayerView {
    #[default]
    MainMenu,
    HostLan,
    JoinLan,
    AdvancedDirectConnect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineCorrectionFeel {
    NoCorrection,
    SmoothReconcile,
    AuthoritativeSnap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineAuthorityCorrectionPresentation {
    pub authority: &'static str,
    pub correction_feel: OnlineCorrectionFeel,
    pub correction_label: &'static str,
    pub snap_applied: bool,
    pub player_message: String,
}

impl OnlineAuthorityCorrectionPresentation {
    #[must_use]
    pub fn from_plan(plan: crate::session::CorrectionPlan, snap_applied: bool) -> Self {
        let (correction_feel, correction_label, player_message) = match plan {
            crate::session::CorrectionPlan::None => (
                OnlineCorrectionFeel::NoCorrection,
                "in sync",
                "Host authority: local prediction matches the authoritative host; no correction needed."
                    .to_owned(),
            ),
            crate::session::CorrectionPlan::Smooth => (
                OnlineCorrectionFeel::SmoothReconcile,
                "smooth reconcile",
                "Host authority: small prediction drift is being smoothed toward the host position."
                    .to_owned(),
            ),
            crate::session::CorrectionPlan::Snap => (
                OnlineCorrectionFeel::AuthoritativeSnap,
                "authoritative snap",
                "Host authority: large prediction drift snapped to the host position to prevent desync."
                    .to_owned(),
            ),
        };
        Self {
            authority: "host authoritative simulation",
            correction_feel,
            correction_label,
            snap_applied,
            player_message,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online authority/correction: {} | correction={} | snap_applied={} | {}",
            self.authority, self.correction_label, self.snap_applied, self.player_message
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineTaskResultTransitionKind {
    HostWaitingForJoin,
    JoinedWaitingForStart,
    EnteredGameplay,
    Failed,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineTaskResultTransition {
    pub kind: OnlineTaskResultTransitionKind,
    pub state: OnlineSessionUxState,
    pub role_label: &'static str,
    pub modal_open: bool,
    pub entered_playing: bool,
    pub status_message: String,
}

impl OnlineTaskResultTransition {
    #[must_use]
    pub fn from_game(
        kind: OnlineTaskResultTransitionKind,
        game: &GameState,
        status_message: String,
    ) -> Self {
        Self {
            kind,
            state: game.online_session_state,
            role_label: game.online_role_label(),
            modal_open: game.modal.is_some(),
            entered_playing: kind == OnlineTaskResultTransitionKind::EnteredGameplay
                && game.run_mode == RunMode::Playing
                && game.modal.is_none(),
            status_message,
        }
    }

    #[allow(
        dead_code,
        reason = "transition summaries are consumed by reducer tests until runtime UI displays the reducer return"
    )]
    #[must_use]
    pub const fn player_facing_summary(&self) -> &'static str {
        match self.kind {
            OnlineTaskResultTransitionKind::HostWaitingForJoin => {
                "Host descriptor is ready; keep this window open and wait for the joined client."
            }
            OnlineTaskResultTransitionKind::JoinedWaitingForStart => {
                "Connected to the host; toggle ready and start when both players are ready."
            }
            OnlineTaskResultTransitionKind::EnteredGameplay => {
                "Online gameplay is active in the game window."
            }
            OnlineTaskResultTransitionKind::Failed => {
                "Online task failed; read the status message before retrying."
            }
            OnlineTaskResultTransitionKind::Shutdown => "Online session shutdown completed.",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineReconnectBlocker {
    NoPlayerSlot,
    NoReconnectableSession,
    PendingNetworkTask,
    HostOwnedCleanSession,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineReconnectAttemptDecision {
    pub can_attempt: bool,
    pub blocker: Option<OnlineReconnectBlocker>,
    pub preserves_dirty_save_state: bool,
    pub preserves_role_slot: bool,
    pub player_message: String,
    pub status: String,
}

impl OnlineReconnectAttemptDecision {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let has_slot = game.online_player_slot.is_some();
        let reconnectable_session = matches!(
            game.online_session_state,
            OnlineSessionUxState::Disconnected
                | OnlineSessionUxState::Timeout
                | OnlineSessionUxState::Error
                | OnlineSessionUxState::Shutdown
                | OnlineSessionUxState::Reconnecting
        );
        let no_pending_task = game.online_network_task_request.is_none();
        let blocker = if !has_slot {
            Some(OnlineReconnectBlocker::NoPlayerSlot)
        } else if !reconnectable_session {
            Some(OnlineReconnectBlocker::NoReconnectableSession)
        } else if !no_pending_task {
            Some(OnlineReconnectBlocker::PendingNetworkTask)
        } else if game.online_host_owns_save
            && !game.save_dirty
            && matches!(game.online_session_state, OnlineSessionUxState::Shutdown)
        {
            Some(OnlineReconnectBlocker::HostOwnedCleanSession)
        } else {
            None
        };
        let can_attempt = blocker.is_none();
        let preserves_dirty_save_state = game.save_dirty
            || !game.online_host_owns_save
            || matches!(
                game.online_session_state,
                OnlineSessionUxState::Reconnecting
            );
        let preserves_role_slot = has_slot
            && ((game.online_host_owns_save && game.online_player_slot == Some(1))
                || (!game.online_host_owns_save && game.online_player_slot == Some(2))
                || matches!(
                    game.online_session_state,
                    OnlineSessionUxState::Reconnecting
                ));
        let player_message = match blocker {
            None => {
                "Reconnect can be attempted with the preserved role, slot, descriptor path, and save boundary."
                    .to_owned()
            }
            Some(OnlineReconnectBlocker::NoPlayerSlot) => {
                "Reconnect blocked: no previous online player slot is available; join from the current host descriptor instead."
                    .to_owned()
            }
            Some(OnlineReconnectBlocker::NoReconnectableSession) => {
                "Reconnect blocked: the current session is still active; leave/end first or continue playing."
                    .to_owned()
            }
            Some(OnlineReconnectBlocker::PendingNetworkTask) => {
                "Reconnect blocked: another online network task is already pending.".to_owned()
            }
            Some(OnlineReconnectBlocker::HostOwnedCleanSession) => {
                "Reconnect blocked: this clean host-owned session has already shut down; host a new descriptor instead."
                    .to_owned()
            }
        };
        let status = format!(
            "Online reconnect attempt: can_attempt={} blocker={:?} dirty_preserved={} role_slot_preserved={} state={:?} role={} slot={} pending_task={} | {}",
            yes_no(can_attempt),
            blocker,
            yes_no(preserves_dirty_save_state),
            yes_no(preserves_role_slot),
            game.online_session_state,
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            yes_no(game.online_network_task_request.is_some()),
            player_message
        );
        Self {
            can_attempt,
            blocker,
            preserves_dirty_save_state,
            preserves_role_slot,
            player_message,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "LAN/VPN QA readiness mirrors independent manual network setup requirements"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineLanVpnQaReadinessStatus {
    pub descriptor_json_path: bool,
    pub host_bind_listens: bool,
    pub advertised_address_shareable: bool,
    pub client_bind_valid: bool,
    pub lan_or_vpn_address_configured: bool,
    pub host_join_commands_visible: bool,
    pub ready_for_same_machine: bool,
    pub ready_for_lan_or_vpn: bool,
    pub status: String,
}

impl OnlineLanVpnQaReadinessStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let descriptor_json_path = game
            .online_descriptor_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
        let host_bind_listens = game.online_host_bind_addr.port() != 0;
        let advertised_address_shareable = game.online_host_advertise_addr.port() != 0
            && !game.online_host_advertise_addr.ip().is_unspecified();
        let client_bind_valid = game.online_client_bind_addr.port() != 0;
        let advertise_ip = game.online_host_advertise_addr.ip();
        let lan_or_vpn_address_configured = advertised_address_shareable
            && !advertise_ip.is_loopback()
            && !advertise_ip.is_unspecified();
        let setup_lines = game.online_direct_connect_setup_lines();
        let host_join_commands_visible = setup_lines
            .iter()
            .any(|line| line.contains("Host CLI helper"))
            && setup_lines
                .iter()
                .any(|line| line.contains("Join CLI helper"));
        let ready_for_same_machine = descriptor_json_path
            && host_bind_listens
            && advertised_address_shareable
            && client_bind_valid
            && host_join_commands_visible;
        let ready_for_lan_or_vpn = ready_for_same_machine && lan_or_vpn_address_configured;
        let status = format!(
            "Online LAN/VPN QA readiness: same_machine={} lan_vpn={} descriptor_json={} host_bind={} advertise_shareable={} client_bind={} lan_vpn_addr={} commands_visible={} descriptor={} bind={} advertise={} client={}",
            yes_no(ready_for_same_machine),
            yes_no(ready_for_lan_or_vpn),
            yes_no(descriptor_json_path),
            yes_no(host_bind_listens),
            yes_no(advertised_address_shareable),
            yes_no(client_bind_valid),
            yes_no(lan_or_vpn_address_configured),
            yes_no(host_join_commands_visible),
            game.online_descriptor_path.display(),
            game.online_host_bind_addr,
            game.online_host_advertise_addr,
            game.online_client_bind_addr
        );
        Self {
            descriptor_json_path,
            host_bind_listens,
            advertised_address_shareable,
            client_bind_valid,
            lan_or_vpn_address_configured,
            host_join_commands_visible,
            ready_for_same_machine,
            ready_for_lan_or_vpn,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "soak readiness separates local, degraded, and long-run runtime evidence"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSoakReadinessStatus {
    pub local_smoke_ticks_configured: bool,
    pub long_soak_ticks_configured: bool,
    pub degraded_soak_evidence: bool,
    pub runtime_sync_evidence: bool,
    pub shutdown_recovery_evidence: bool,
    pub ready_for_local_soak: bool,
    pub ready_for_degraded_soak: bool,
    pub status: String,
}

impl OnlineSoakReadinessStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let local_smoke_ticks_configured = game.online_gameplay_ticks >= 60;
        let long_soak_ticks_configured = game.online_gameplay_ticks >= 300;
        let runtime_sync_evidence = !game.online_last_sync_loop_status.is_empty()
            || !game.online_last_replication_status.is_empty()
            || !game.online_last_gameplay_sync_evidence_status.is_empty();
        let degraded_soak_evidence = game.online_last_failure_status.contains("Online failure:")
            || game.online_diagnostic_last_tick.contains("degraded")
            || game
                .online_last_session_boundary_status
                .contains("transport-closed")
            || game.online_session_state == OnlineSessionUxState::Reconnecting;
        let leave_end = OnlineLeaveEndSafetyStatus::from_game(game);
        let shutdown_recovery_evidence = leave_end.force_kill_not_required
            && (leave_end.shutdown_acknowledged
                || leave_end.shutdown_pending
                || leave_end.peer_notification_evidence
                || game.online_session_state == OnlineSessionUxState::Connected);
        let ready_for_local_soak =
            local_smoke_ticks_configured && runtime_sync_evidence && shutdown_recovery_evidence;
        let ready_for_degraded_soak =
            ready_for_local_soak && long_soak_ticks_configured && degraded_soak_evidence;
        let status = format!(
            "Online soak readiness: local_soak={} degraded_soak={} smoke_ticks={} long_ticks={} degraded_evidence={} runtime_sync={} shutdown_recovery={} ticks={} state={:?}",
            yes_no(ready_for_local_soak),
            yes_no(ready_for_degraded_soak),
            yes_no(local_smoke_ticks_configured),
            yes_no(long_soak_ticks_configured),
            yes_no(degraded_soak_evidence),
            yes_no(runtime_sync_evidence),
            yes_no(shutdown_recovery_evidence),
            game.online_gameplay_ticks,
            game.online_session_state
        );
        Self {
            local_smoke_ticks_configured,
            long_soak_ticks_configured,
            degraded_soak_evidence,
            runtime_sync_evidence,
            shutdown_recovery_evidence,
            ready_for_local_soak,
            ready_for_degraded_soak,
            status,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineReconnectPolicy {
    UnsupportedForFirstPlayableMvp,
    SessionContextAvailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineReconnectPolicyStatus {
    pub policy: OnlineReconnectPolicy,
    pub can_attempt_rejoin: bool,
    pub preserves_dirty_save_state: bool,
    pub player_message: String,
}

impl OnlineReconnectPolicyStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let decision = OnlineReconnectAttemptDecision::from_game(game);
        let can_attempt_rejoin = decision.can_attempt;
        let policy = if can_attempt_rejoin {
            OnlineReconnectPolicy::SessionContextAvailable
        } else {
            OnlineReconnectPolicy::UnsupportedForFirstPlayableMvp
        };
        let player_message = match policy {
            OnlineReconnectPolicy::SessionContextAvailable => decision.player_message,
            OnlineReconnectPolicy::UnsupportedForFirstPlayableMvp => {
                "Reconnect is not automatic for the first playable MVP; return to Online Multiplayer and rejoin from the current host descriptor.".to_owned()
            }
        };
        Self {
            policy,
            can_attempt_rejoin,
            preserves_dirty_save_state: game.save_dirty,
            player_message,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Online reconnect policy: policy={:?} can_attempt_rejoin={} dirty_save_preserved={} | {}",
            self.policy,
            yes_no(self.can_attempt_rejoin),
            yes_no(self.preserves_dirty_save_state),
            self.player_message
        )
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "production online UI gate mirrors independent host/join modal requirements"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionOnlineUiRuntimeStatus {
    pub host_descriptor_from_real_state: bool,
    pub host_controller_persistent: bool,
    pub host_ui_details_visible: bool,
    pub host_descriptor_path_shareable: bool,
    pub host_errors_clear_pending_task: bool,
    pub join_reads_modal_path: bool,
    pub join_real_controller_connected: bool,
    pub join_identity_assigned: bool,
    pub join_controller_persistent: bool,
    pub join_ui_save_policy_visible: bool,
    pub join_errors_clear_pending_task: bool,
    pub ready: bool,
    pub status: String,
}

impl ProductionOnlineUiRuntimeStatus {
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let descriptor_path_present = !game.online_descriptor_path.as_os_str().is_empty();
        let descriptor_status = if game.online_last_descriptor_input_status.is_empty() {
            OnlineDescriptorInputStatus::validate(
                OnlineDescriptorInputMode::HostWrite,
                &game.online_descriptor_path,
            )
            .status_line()
        } else {
            game.online_last_descriptor_input_status.clone()
        };
        let host_descriptor_from_real_state = descriptor_path_present
            && descriptor_status
                .to_ascii_lowercase()
                .contains("descriptor")
            && (matches!(
                game.online_session_state,
                OnlineSessionUxState::Hosting | OnlineSessionUxState::Connected
            ) || game.online_host_owns_save);
        let host_controller_persistent = game.online_diagnostic_controller_mode.contains("host")
            || matches!(
                game.online_session_state,
                OnlineSessionUxState::Hosting | OnlineSessionUxState::Connected
            ) && game.online_host_owns_save;
        let host_ui_details_visible = descriptor_path_present
            && game
                .online_address_guidance_line()
                .to_ascii_lowercase()
                .contains("address")
            && game.online_player_slot.is_some()
            && game.online_host_owns_save;
        let host_descriptor_path_shareable =
            descriptor_path_present && !game.online_descriptor_path_editing;
        let host_errors_clear_pending_task = game.online_network_task_request.is_none()
            || !game.online_last_failure_status.is_empty();
        let join_reads_modal_path = descriptor_path_present
            && game
                .online_multiplayer_status_lines_without_production_ui_gate()
                .iter()
                .any(|line| {
                    line.contains("Descriptor path edit") || line.contains("Descriptor file:")
                });
        let join_real_controller_connected =
            game.online_diagnostic_controller_mode.contains("client")
                || matches!(game.online_session_state, OnlineSessionUxState::Connected)
                    && !game.online_host_owns_save;
        let join_identity_assigned =
            !game.online_host_owns_save && game.online_player_slot.is_some();
        let join_controller_persistent = join_real_controller_connected
            && matches!(game.online_session_state, OnlineSessionUxState::Connected);
        let save_boundary = OnlineSaveBoundaryStatus::from_game(game);
        let join_ui_save_policy_visible = !game.online_host_owns_save
            && save_boundary.save_authority == OnlineSaveAuthority::RemoteHost
            && game
                .online_multiplayer_status_lines_without_production_ui_gate()
                .iter()
                .any(|line| {
                    line.contains("Save/exit policy") || line.contains("Online save boundary")
                });
        let join_errors_clear_pending_task = game.online_network_task_request.is_none()
            || !game.online_last_failure_status.is_empty();
        let ready = (host_descriptor_from_real_state
            && host_controller_persistent
            && host_ui_details_visible
            && host_descriptor_path_shareable
            && host_errors_clear_pending_task)
            || (join_reads_modal_path
                && join_real_controller_connected
                && join_identity_assigned
                && join_controller_persistent
                && join_ui_save_policy_visible
                && join_errors_clear_pending_task);
        let status = format!(
            "Production online UI runtime: ready={} host_descriptor_from_real_state={} host_controller_persistent={} host_ui_details_visible={} host_descriptor_path_shareable={} host_errors_clear_pending_task={} join_reads_modal_path={} join_real_controller_connected={} join_identity_assigned={} join_controller_persistent={} join_ui_save_policy_visible={} join_errors_clear_pending_task={}",
            yes_no(ready),
            yes_no(host_descriptor_from_real_state),
            yes_no(host_controller_persistent),
            yes_no(host_ui_details_visible),
            yes_no(host_descriptor_path_shareable),
            yes_no(host_errors_clear_pending_task),
            yes_no(join_reads_modal_path),
            yes_no(join_real_controller_connected),
            yes_no(join_identity_assigned),
            yes_no(join_controller_persistent),
            yes_no(join_ui_save_policy_visible),
            yes_no(join_errors_clear_pending_task)
        );
        Self {
            host_descriptor_from_real_state,
            host_controller_persistent,
            host_ui_details_visible,
            host_descriptor_path_shareable,
            host_errors_clear_pending_task,
            join_reads_modal_path,
            join_real_controller_connected,
            join_identity_assigned,
            join_controller_persistent,
            join_ui_save_policy_visible,
            join_errors_clear_pending_task,
            ready,
            status,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineRuntimeStatusDeck {
    pub lines: Vec<String>,
}

impl OnlineRuntimeStatusDeck {
    #[must_use]
    pub const fn from_lines(lines: Vec<String>) -> Self {
        Self { lines }
    }

    #[cfg(test)]
    #[must_use]
    pub fn contains_line_matching(&self, needle: &str) -> bool {
        self.lines.iter().any(|line| line.contains(needle))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalMultiplayerRuntimePhase {
    Inactive,
    Requested,
    Active,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalMultiplayerRuntimeStatus {
    pub phase: LocalMultiplayerRuntimePhase,
    pub requested: bool,
    pub active: bool,
    pub player_slots: u8,
    pub online_isolated: bool,
    pub player_message: String,
}

impl LocalMultiplayerRuntimeStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let phase = if game.local_multiplayer_active {
            LocalMultiplayerRuntimePhase::Active
        } else if game.local_multiplayer_requested {
            LocalMultiplayerRuntimePhase::Requested
        } else {
            LocalMultiplayerRuntimePhase::Inactive
        };
        let online_isolated = !game.local_multiplayer_active
            || matches!(
                game.online_session_state,
                OnlineSessionUxState::Idle
                    | OnlineSessionUxState::Disconnected
                    | OnlineSessionUxState::Shutdown
            );
        let player_message = match phase {
            LocalMultiplayerRuntimePhase::Inactive => {
                "Local split-screen inactive; online direct-connect state is handled separately."
                    .to_owned()
            }
            LocalMultiplayerRuntimePhase::Requested => {
                "Local split-screen requested; waiting for the app/session layer to attach local players."
                    .to_owned()
            }
            LocalMultiplayerRuntimePhase::Active => format!(
                "Local split-screen active with {} players; no online direct-connect session owns these local-only slots.",
                game.local_multiplayer_player_slots
            ),
        };
        Self {
            phase,
            requested: game.local_multiplayer_requested,
            active: game.local_multiplayer_active,
            player_slots: game.local_multiplayer_player_slots,
            online_isolated,
            player_message,
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Local split-screen runtime: phase={:?} requested={} active={} slots={} online_isolated={} | {}",
            self.phase,
            yes_no(self.requested),
            yes_no(self.active),
            self.player_slots,
            yes_no(self.online_isolated),
            self.player_message
        )
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "manual working-game gate mirrors independent owner-validation checklist rows"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineManualWorkingGameGateStatus {
    pub two_instances_ready: bool,
    pub ui_host_join_ready: bool,
    pub ui_ready_start_ready: bool,
    pub movement_observable: bool,
    pub mining_observable: bool,
    pub survival_observable: bool,
    pub shutdown_safe: bool,
    pub failures_triaged: bool,
    pub ready: bool,
    pub status: String,
}

impl OnlineManualWorkingGameGateStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let playable = OnlinePlayableSessionStatus::from_game(game);
        let sustained = OnlineSustainedMiningSessionStatus::from_game(game);
        let save_boundary = OnlineSaveBoundaryStatus::from_game(game);
        let two_instances_ready = matches!(
            game.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
        ) && game.online_player_slot.is_some();
        let ui_host_join_ready = playable.host_from_ui || playable.joined_from_ui;
        let ui_ready_start_ready = playable.both_entered_gameplay
            || (game.online_local_ready && game.online_remote_player_ready)
            || game.run_mode == RunMode::Playing;
        let movement_observable = playable.movement_visible || sustained.movement_visible;
        let mining_observable = sustained.terrain_visible
            && sustained.cargo_or_economy_visible
            && (game
                .online_last_gameplay_sync_evidence_status
                .contains("terrain")
                || game.online_last_sync_loop_status.contains("terrain"));
        let survival_observable = sustained.survival_visible
            && game.player.fuel.is_finite()
            && game.player.hull.is_finite();
        let shutdown_safe = game
            .online_last_session_boundary_status
            .contains("Online session boundary")
            && matches!(
                save_boundary.unsaved_exit_action,
                OnlineUnsavedExitAction::CleanExitAllowed
                    | OnlineUnsavedExitAction::SaveAndExitAllowed
                    | OnlineUnsavedExitAction::DiscardOrCancelOnly
            );
        let failures_triaged = game.online_last_failure_status.is_empty()
            || game.online_last_failure_status.contains("Online failure:")
            || game.online_last_failure_status.contains("category=");
        let directional = OnlineDirectionalGameplaySyncStatus::from_game(game);
        let clarity = OnlineGameplayClarityPresentation::from_game(game);
        let leave_end = OnlineLeaveEndSafetyStatus::from_game(game);
        let ready = two_instances_ready
            && ui_host_join_ready
            && ui_ready_start_ready
            && movement_observable
            && mining_observable
            && survival_observable
            && shutdown_safe
            && failures_triaged
            && directional.both_directions_visible
            && clarity.hud_clear_enough
            && leave_end.force_kill_not_required
            && leave_end.save_corruption_guarded;
        let status = format!(
            "Online manual working-game gate: ready={} two_instances={} ui_host_join={} ui_ready_start={} movement={} mining={} survival={} shutdown_safe={} failures_triaged={} directional={} hud_clear={} leave_safe={} save_guarded={} role={} slot={}",
            yes_no(ready),
            yes_no(two_instances_ready),
            yes_no(ui_host_join_ready),
            yes_no(ui_ready_start_ready),
            yes_no(movement_observable),
            yes_no(mining_observable),
            yes_no(survival_observable),
            yes_no(shutdown_safe),
            yes_no(failures_triaged),
            yes_no(directional.both_directions_visible),
            yes_no(clarity.hud_clear_enough),
            yes_no(leave_end.force_kill_not_required),
            yes_no(leave_end.save_corruption_guarded),
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        );
        Self {
            two_instances_ready,
            ui_host_join_ready,
            ui_ready_start_ready,
            movement_observable,
            mining_observable,
            survival_observable,
            shutdown_safe,
            failures_triaged,
            ready,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "sustained-play status checks independent runtime sync signals for manual mining sessions"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSustainedMiningSessionStatus {
    pub playable: bool,
    pub gameplay_active: bool,
    pub movement_visible: bool,
    pub terrain_visible: bool,
    pub cargo_or_economy_visible: bool,
    pub survival_visible: bool,
    pub session_boundary_safe: bool,
    pub save_boundary_safe: bool,
    pub status: String,
}

impl OnlineSustainedMiningSessionStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let gameplay_active = game.run_mode == RunMode::Playing
            && game.online_session_state == OnlineSessionUxState::Connected;
        let movement_visible = game.online_remote_player_connected
            && game
                .online_last_replicated_player_status
                .contains("updated");
        let terrain_visible = game.online_last_terrain_status.contains("chunk")
            || game.online_last_sync_loop_status.contains("terrain");
        let cargo_or_economy_visible = game.online_last_replication_status.contains("cargo")
            || game.online_last_sync_loop_status.contains("cargo=yes")
            || game.player.credits > 0
            || game.player.cargo_used() > 0;
        let survival_visible = game.player.fuel >= 0.0
            && game.player.hull >= 0.0
            && (game.online_last_sync_loop_status.contains("snapshot")
                || game.online_last_replication_status.contains("replication"));
        let session_boundary_safe = game
            .online_last_session_boundary_status
            .contains("Online session boundary")
            || game.online_remote_player_connected;
        let save_boundary_safe = game
            .online_last_save_boundary_status
            .contains("Online save boundary");
        let playable = gameplay_active
            && movement_visible
            && terrain_visible
            && cargo_or_economy_visible
            && survival_visible
            && session_boundary_safe
            && save_boundary_safe;
        let status = format!(
            "Online sustained mining session: playable={} gameplay_active={} movement={} terrain={} cargo_or_economy={} survival={} session_boundary_safe={} save_boundary_safe={} ticks={}",
            yes_no(playable),
            yes_no(gameplay_active),
            yes_no(movement_visible),
            yes_no(terrain_visible),
            yes_no(cargo_or_economy_visible),
            yes_no(survival_visible),
            yes_no(session_boundary_safe),
            yes_no(save_boundary_safe),
            game.update_ticks
        );
        Self {
            playable,
            gameplay_active,
            movement_visible,
            terrain_visible,
            cargo_or_economy_visible,
            survival_visible,
            session_boundary_safe,
            save_boundary_safe,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "directional multiplayer status mirrors product validation rows for both peers"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineDirectionalGameplaySyncStatus {
    pub gameplay_active: bool,
    pub host_movement_visible: bool,
    pub client_movement_visible: bool,
    pub host_mining_visible: bool,
    pub client_mining_visible: bool,
    pub host_sees_client: bool,
    pub client_sees_host: bool,
    pub both_directions_visible: bool,
    pub status: String,
}

impl OnlineDirectionalGameplaySyncStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let gameplay_active = game.run_mode == RunMode::Playing
            && game.online_session_state == OnlineSessionUxState::Connected
            && game.modal.is_none();
        let is_host = game.online_host_owns_save && game.online_player_slot == Some(1);
        let is_client = !game.online_host_owns_save && game.online_player_slot == Some(2);
        let local_moving = game.player.velocity_x.abs() > f32::EPSILON
            || game.player.velocity_y.abs() > f32::EPSILON
            || game.online_last_replicated_player_status.contains("local")
            || game.update_ticks > 0;
        let remote_moving = game.online_remote_player_connected
            && (game
                .online_last_replicated_player_status
                .contains("updated")
                || game.online_last_sync_loop_status.contains("player_delta"));
        let terrain_changed = game.online_last_terrain_status.contains("applied chunk")
            || game.online_last_terrain_status.contains("visible tiles")
            || game.online_last_terrain_status.contains("highlighted")
            || game.online_last_sync_loop_status.contains("terrain")
            || !game.online_terrain_sync_markers.is_empty();
        let local_mining = terrain_changed
            || game.online_last_terrain_status.contains("answered chunk")
            || game.online_last_replication_status.contains("world delta")
            || game.online_last_sync_loop_status.contains("delta");
        let remote_mining = terrain_changed
            || game.online_last_terrain_status.contains("applied chunk")
            || game.online_last_terrain_status.contains("requested chunk");
        let host_movement_visible = (is_host && local_moving) || (is_client && remote_moving);
        let client_movement_visible = (is_client && local_moving) || (is_host && remote_moving);
        let host_mining_visible = (is_host && local_mining) || (is_client && remote_mining);
        let client_mining_visible = (is_client && local_mining) || (is_host && remote_mining);
        let host_sees_client = is_host && (client_movement_visible || client_mining_visible);
        let client_sees_host = is_client && (host_movement_visible || host_mining_visible);
        let both_directions_visible = gameplay_active
            && host_movement_visible
            && client_movement_visible
            && host_mining_visible
            && client_mining_visible;
        let status = format!(
            "Online directional gameplay sync: active={} host_moves={} client_moves={} host_mines={} client_mines={} host_sees_client={} client_sees_host={} both_directions={} role={} slot={}",
            yes_no(gameplay_active),
            yes_no(host_movement_visible),
            yes_no(client_movement_visible),
            yes_no(host_mining_visible),
            yes_no(client_mining_visible),
            yes_no(host_sees_client),
            yes_no(client_sees_host),
            yes_no(both_directions_visible),
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        );
        Self {
            gameplay_active,
            host_movement_visible,
            client_movement_visible,
            host_mining_visible,
            client_mining_visible,
            host_sees_client,
            client_sees_host,
            both_directions_visible,
            status,
        }
    }

    #[must_use]
    pub fn hud_line(&self) -> String {
        format!(
            "Directional sync: host move={} mine={} | client move={} mine={}",
            yes_no(self.host_movement_visible),
            yes_no(self.host_mining_visible),
            yes_no(self.client_movement_visible),
            yes_no(self.client_mining_visible)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlinePlayableSessionPhase {
    NotStarted,
    HostWaiting,
    JoinedWaiting,
    ReadyToStart,
    Playing,
    Blocked,
    Ended,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "playable-session gate reports independent UI milestone and sync-evidence flags"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlinePlayableSessionStatus {
    pub phase: OnlinePlayableSessionPhase,
    pub host_from_ui: bool,
    pub joined_from_ui: bool,
    pub both_entered_gameplay: bool,
    pub movement_visible: bool,
    pub terrain_or_cargo_visible: bool,
    pub blocker: Option<OnlineGameplayStartBlocker>,
    pub status: String,
}

impl OnlinePlayableSessionStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let start_gate = game.online_gameplay_start_gate();
        let host_from_ui = matches!(
            game.online_session_state,
            OnlineSessionUxState::Hosting | OnlineSessionUxState::Connected
        ) && game.online_host_owns_save
            && game.online_player_slot == Some(1);
        let joined_from_ui = matches!(
            game.online_session_state,
            OnlineSessionUxState::Joining | OnlineSessionUxState::Connected
        ) && !game.online_host_owns_save
            && game.online_player_slot == Some(2);
        let both_entered_gameplay = game.run_mode == RunMode::Playing
            && matches!(game.online_session_state, OnlineSessionUxState::Connected)
            && game.modal.is_none();
        let movement_visible = !game.online_last_replicated_player_status.is_empty();
        let terrain_or_cargo_visible = !game.online_last_terrain_status.is_empty()
            || game.online_last_sync_loop_status.contains("terrain")
            || game.online_last_sync_loop_status.contains("cargo=yes")
            || game.online_last_replicated_player_status.contains("cargo=");
        let phase = if matches!(
            game.online_session_state,
            OnlineSessionUxState::Shutdown
                | OnlineSessionUxState::Error
                | OnlineSessionUxState::Disconnected
        ) {
            OnlinePlayableSessionPhase::Ended
        } else if both_entered_gameplay {
            OnlinePlayableSessionPhase::Playing
        } else if start_gate.ready {
            OnlinePlayableSessionPhase::ReadyToStart
        } else if matches!(game.online_session_state, OnlineSessionUxState::Hosting) {
            OnlinePlayableSessionPhase::HostWaiting
        } else if matches!(
            game.online_session_state,
            OnlineSessionUxState::Joining | OnlineSessionUxState::Connected
        ) && !game.online_host_owns_save
        {
            OnlinePlayableSessionPhase::JoinedWaiting
        } else if matches!(game.online_session_state, OnlineSessionUxState::Idle) {
            OnlinePlayableSessionPhase::NotStarted
        } else {
            OnlinePlayableSessionPhase::Blocked
        };
        let status = format!(
            "Online playable session gate: phase={phase:?} host_from_ui={} joined_from_ui={} gameplay_active={} movement={} terrain_or_cargo={} start_ready={} blocker={:?} role={} slot={}",
            yes_no(host_from_ui),
            yes_no(joined_from_ui),
            yes_no(both_entered_gameplay),
            yes_no(movement_visible),
            yes_no(terrain_or_cargo_visible),
            yes_no(start_gate.ready),
            start_gate.blocker,
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        );
        Self {
            phase,
            host_from_ui,
            joined_from_ui,
            both_entered_gameplay,
            movement_visible,
            terrain_or_cargo_visible,
            blocker: start_gate.blocker,
            status,
        }
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "ready/start transition reports each production UI acceptance condition independently"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineReadyStartTransitionStatus {
    pub local_ready_sent: bool,
    pub remote_ready_received: bool,
    pub start_gated_on_connected_ready: bool,
    pub start_message_sendable: bool,
    pub host_enters_gameplay_from_ui: bool,
    pub joined_enters_gameplay_from_ui: bool,
    pub modal_closes_only_when_playing: bool,
    pub role_slot_save_authority_preserved: bool,
    pub transition_ready: bool,
    pub status: String,
}

impl OnlineReadyStartTransitionStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let start_gate = game.online_gameplay_start_gate();
        let connected = game.online_session_state == OnlineSessionUxState::Connected;
        let local_ready_sent = game.online_local_ready
            && (game.online_diagnostic_last_tick.contains("ready")
                || game.online_session_status_message.contains("ready")
                || connected);
        let remote_ready_received =
            game.online_remote_player_connected && game.online_remote_player_ready;
        let start_gated_on_connected_ready = start_gate.ready
            || matches!(
                start_gate.blocker,
                Some(
                    OnlineGameplayStartBlocker::NotConnected
                        | OnlineGameplayStartBlocker::LocalNotReady
                        | OnlineGameplayStartBlocker::RemoteNotConnected
                        | OnlineGameplayStartBlocker::RemoteNotReady
                        | OnlineGameplayStartBlocker::HostAuthorityRequired
                )
            );
        let start_message_sendable = start_gate.ready
            && game.online_host_owns_save
            && game.online_remote_player_connected
            && game.online_remote_player_ready;
        let host_enters_gameplay_from_ui = game.online_host_owns_save
            && (game.run_mode == RunMode::Playing)
            && matches!(game.online_session_state, OnlineSessionUxState::Connected);
        let joined_enters_gameplay_from_ui = !game.online_host_owns_save
            && (game.run_mode == RunMode::Playing)
            && matches!(game.online_session_state, OnlineSessionUxState::Connected)
            && !game.online_start_session_requested;
        let modal_closes_only_when_playing =
            game.run_mode == RunMode::Playing || game.modal.is_some();
        let save_boundary = OnlineSaveBoundaryStatus::from_game(game);
        let role_slot_save_authority_preserved = game.online_player_slot.is_some()
            && ((game.online_host_owns_save
                && save_boundary.save_authority == OnlineSaveAuthority::LocalPlayer)
                || (!game.online_host_owns_save
                    && save_boundary.save_authority == OnlineSaveAuthority::RemoteHost));
        let start_transition_complete = if game.online_host_owns_save {
            start_message_sendable
        } else {
            joined_enters_gameplay_from_ui
        };
        let transition_ready = local_ready_sent
            && remote_ready_received
            && start_gated_on_connected_ready
            && start_transition_complete
            && modal_closes_only_when_playing
            && role_slot_save_authority_preserved;
        let status = format!(
            "Online ready/start transition: ready={} local_ready_sent={} remote_ready_received={} start_gated_on_connected_ready={} start_message_sendable={} host_enters_gameplay_from_ui={} joined_enters_gameplay_from_ui={} modal_closes_only_when_playing={} role_slot_save_authority_preserved={} blocker={:?}",
            yes_no(transition_ready),
            yes_no(local_ready_sent),
            yes_no(remote_ready_received),
            yes_no(start_gated_on_connected_ready),
            yes_no(start_message_sendable),
            yes_no(host_enters_gameplay_from_ui),
            yes_no(joined_enters_gameplay_from_ui),
            yes_no(modal_closes_only_when_playing),
            yes_no(role_slot_save_authority_preserved),
            start_gate.blocker
        );
        Self {
            local_ready_sent,
            remote_ready_received,
            start_gated_on_connected_ready,
            start_message_sendable,
            host_enters_gameplay_from_ui,
            joined_enters_gameplay_from_ui,
            modal_closes_only_when_playing,
            role_slot_save_authority_preserved,
            transition_ready,
            status,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineGameplayStartBlocker {
    NotConnected,
    LocalNotReady,
    RemoteNotConnected,
    RemoteNotReady,
    HostAuthorityRequired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineGameplayStartGate {
    pub ready: bool,
    pub blocker: Option<OnlineGameplayStartBlocker>,
    pub message: String,
}

impl OnlineGameplayStartGate {
    #[must_use]
    pub fn ready() -> Self {
        Self {
            ready: true,
            blocker: None,
            message: "Starting online gameplay from connected direct-connect session.".to_owned(),
        }
    }

    #[must_use]
    pub fn blocked(blocker: OnlineGameplayStartBlocker, message: &'static str) -> Self {
        Self {
            ready: false,
            blocker: Some(blocker),
            message: message.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiplayerPortabilityBoundaryStatus {
    pub gameplay_state_render_input_decoupled: bool,
    pub network_adapter_isolated: bool,
    pub save_boundary_explicit: bool,
    pub status: String,
}

impl MultiplayerPortabilityBoundaryStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let gameplay_state_render_input_decoupled = game
            .online_last_gameplay_domain_status
            .contains("Online gameplay domains")
            || OnlineGameplayDomainStatus::from_game(game)
                .status
                .contains("Online gameplay domains");
        let network_adapter_isolated = game
            .online_last_ownership_status
            .contains("Online ownership")
            || OnlineOwnershipStatus::from_game(game)
                .status_line()
                .contains("Online ownership");
        let save_boundary_explicit = game
            .online_last_save_boundary_status
            .contains("Online save boundary")
            || OnlineSaveBoundaryStatus::from_game(game)
                .status_line()
                .contains("Online save boundary");
        let status = format!(
            "Multiplayer portability boundary: gameplay_state_render_input_decoupled={} network_adapter_isolated={} save_boundary_explicit={}",
            yes_no(gameplay_state_render_input_decoupled),
            yes_no(network_adapter_isolated),
            yes_no(save_boundary_explicit)
        );
        Self {
            gameplay_state_render_input_decoupled,
            network_adapter_isolated,
            save_boundary_explicit,
            status,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineSessionUxReducerStatus {
    pub state: OnlineSessionUxState,
    pub role_label: &'static str,
    pub player_slot: Option<u8>,
    pub host_owns_save: bool,
    pub gameplay_mutation_free: bool,
    pub status: String,
}

impl OnlineSessionUxReducerStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let gameplay_mutation_free = !matches!(
            game.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Reconnecting
        ) || game.run_mode != RunMode::Playing;
        let status = format!(
            "Online UX reducer: state={:?} role={} slot={} host_owns_save={} gameplay_mutation_free={} message={}",
            game.online_session_state,
            game.online_role_label(),
            game.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string()),
            yes_no(game.online_host_owns_save),
            yes_no(gameplay_mutation_free),
            game.online_session_status_message
        );
        Self {
            state: game.online_session_state,
            role_label: game.online_role_label(),
            player_slot: game.online_player_slot,
            host_owns_save: game.online_host_owns_save,
            gameplay_mutation_free,
            status,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineRuntimeTickPresentation {
    pub controller_mode: String,
    pub tick_status: String,
}

impl OnlineRuntimeTickPresentation {
    #[must_use]
    pub fn new(controller_mode: impl Into<String>, tick_status: impl Into<String>) -> Self {
        Self {
            controller_mode: controller_mode.into(),
            tick_status: tick_status.into(),
        }
    }

    #[must_use]
    pub fn awaiting(controller_mode: impl Into<String>) -> Self {
        Self::new(controller_mode, "connected; awaiting ticks")
    }

    #[must_use]
    pub fn reconnected(controller_mode: impl Into<String>) -> Self {
        Self::new(controller_mode, "reconnected; awaiting ticks")
    }

    #[must_use]
    pub fn controller_label(&self) -> &str {
        if self.controller_mode.is_empty() {
            "none"
        } else {
            &self.controller_mode
        }
    }

    #[must_use]
    pub fn tick_label(&self) -> &str {
        if self.tick_status.is_empty() {
            "none"
        } else {
            &self.tick_status
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "Runtime tick: controller={} status={}",
            self.controller_label(),
            self.tick_label()
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineNetworkTaskOrchestrationStatus {
    pub pending_task: Option<OnlineNetworkTaskRequest>,
    pub controller_mode: String,
    pub last_tick: String,
    pub ui_presenter_separated: bool,
    pub status: String,
}

impl OnlineNetworkTaskOrchestrationStatus {
    #[must_use]
    pub fn from_game(game: &GameState) -> Self {
        let runtime_tick = OnlineRuntimeTickPresentation::new(
            if game.online_runtime_controller_mode.is_empty() {
                game.online_diagnostic_controller_mode.clone()
            } else {
                game.online_runtime_controller_mode.clone()
            },
            if game.online_runtime_tick_status.is_empty() {
                game.online_diagnostic_last_tick.clone()
            } else {
                game.online_runtime_tick_status.clone()
            },
        );
        let controller_mode = runtime_tick.controller_label().to_owned();
        let last_tick = runtime_tick.tick_label().to_owned();
        let ui_presenter_separated =
            !controller_mode.contains("modal") && !controller_mode.contains("status-line");
        let pending_label = game
            .online_network_task_request
            .as_ref()
            .map_or_else(|| "none".to_owned(), |task| format!("{task:?}"));
        let status = format!(
            "Online network orchestration: pending={} {} ui_presenter_separated={}",
            pending_label,
            runtime_tick.status_line(),
            yes_no(ui_presenter_separated)
        );
        Self {
            pending_task: game.online_network_task_request.clone(),
            controller_mode,
            last_tick,
            ui_presenter_separated,
            status,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnlineShutdownSummary {
    pub role_label: String,
    pub peer_notification_attempted: bool,
    pub peer_notified: bool,
    pub warning: Option<String>,
    pub save_policy_line: String,
}

impl OnlineShutdownSummary {
    #[must_use]
    #[allow(
        dead_code,
        reason = "used by shutdown reducer tests without requiring a live controller"
    )]
    pub fn offline() -> Self {
        Self {
            role_label: "offline".to_owned(),
            peer_notification_attempted: false,
            peer_notified: false,
            warning: None,
            save_policy_line: "No online peer was active; local save policy unchanged.".to_owned(),
        }
    }

    #[must_use]
    pub fn from_notification(
        role_label: impl Into<String>,
        peer_notification_attempted: bool,
        peer_notified: bool,
        warning: Option<String>,
        host_owns_save: bool,
    ) -> Self {
        Self {
            role_label: role_label.into(),
            peer_notification_attempted,
            peer_notified,
            warning,
            save_policy_line: if host_owns_save {
                "Host-owned save remains local and may be saved after shutdown.".to_owned()
            } else {
                "Joined-client local save writes remain blocked until the session closes."
                    .to_owned()
            },
        }
    }

    #[must_use]
    pub fn status_line(&self) -> String {
        let notification = if self.peer_notification_attempted {
            if self.peer_notified {
                "peer notified"
            } else {
                "peer notification failed"
            }
        } else {
            "no peer notification needed"
        };
        self.warning.as_ref().map_or_else(
            || {
                format!(
                    "Shutdown summary: role={} {notification}; {}",
                    self.role_label, self.save_policy_line
                )
            },
            |warning| {
                format!(
                    "Shutdown summary: role={} {notification}; {}; warning={warning}",
                    self.role_label, self.save_policy_line
                )
            },
        )
    }
}

#[allow(
    dead_code,
    reason = "online task reducer is exercised by tests until desktop event-loop ownership calls it"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnlineNetworkTaskResult {
    Hosted(RealOnlineSessionUxSnapshot),
    JoinedDescriptor(RealOnlineSessionUxSnapshot),
    Connected(RealOnlineSessionUxSnapshot),
    Reconnected(RealOnlineSessionUxSnapshot),
    Failed(String),
    Shutdown(OnlineShutdownSummary),
}

#[allow(
    dead_code,
    reason = "real online UX bridge is exercised by tests until desktop async UI ownership calls it"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealOnlineSessionUxSnapshot {
    pub state: OnlineSessionUxState,
    pub host_owns_save: bool,
    pub player_slot: Option<u8>,
    pub status_message: String,
}

#[allow(
    dead_code,
    reason = "real online UX bridge is exercised by tests until desktop async UI ownership calls it"
)]
impl RealOnlineSessionUxSnapshot {
    #[must_use]
    pub fn from_joined_session(player_slot: Option<u8>) -> Self {
        Self {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot,
            status_message: "Connected through real localhost Quinn session.".to_owned(),
        }
    }

    #[must_use]
    pub fn from_host_descriptor_ready(player_slot: Option<u8>, path: &Path) -> Self {
        Self {
            state: OnlineSessionUxState::Hosting,
            host_owns_save: true,
            player_slot,
            status_message: format!(
                "Host descriptor ready at {}; waiting for remote miner to join.",
                path.display()
            ),
        }
    }

    #[must_use]
    pub fn from_descriptor_host_accepted(player_slot: Option<u8>) -> Self {
        Self {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot,
            status_message: "Remote miner joined descriptor host; gameplay sync is ready to start."
                .to_owned(),
        }
    }

    #[must_use]
    pub fn from_descriptor_client_connected(player_slot: Option<u8>, path: &Path) -> Self {
        Self {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot,
            status_message: format!(
                "Connected to host descriptor {}; waiting for gameplay sync.",
                path.display()
            ),
        }
    }

    #[must_use]
    pub fn from_reconnect(player_slot: Option<u8>) -> Self {
        Self {
            state: OnlineSessionUxState::Connected,
            host_owns_save: player_slot != Some(2),
            player_slot,
            status_message: "Reconnected through real localhost Quinn session.".to_owned(),
        }
    }

    #[must_use]
    pub fn from_descriptor_host_tick_summary(
        summary: &QuinnSessionTickSummary,
        player_slot: Option<u8>,
    ) -> Self {
        Self {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot,
            status_message: format!(
                "Descriptor host tick: command={}, snapshot={}, delta={}, chunk={}, correction={}",
                summary.command_summary.is_some(),
                summary.snapshot_replicated,
                summary.delta_replicated,
                summary.terrain_chunk_response.is_some(),
                summary.correction_summary.is_some()
            ),
        }
    }

    #[must_use]
    pub fn from_tick_summary(summary: &QuinnSessionTickSummary, player_slot: Option<u8>) -> Self {
        let state = if summary.advanced_authoritative_runtime() {
            OnlineSessionUxState::Connected
        } else {
            OnlineSessionUxState::Hosting
        };
        Self {
            state,
            host_owns_save: true,
            player_slot,
            status_message: format!(
                "Real Quinn tick: command={}, snapshot={}, delta={}, chunk={}, correction={}",
                summary.command_summary.is_some(),
                summary.snapshot_replicated,
                summary.delta_replicated,
                summary.terrain_chunk_response.is_some(),
                summary.correction_summary.is_some()
            ),
        }
    }

    #[must_use]
    pub fn from_correction(
        summary: SocketDrivenCorrectionSummary,
        player_slot: Option<u8>,
    ) -> Self {
        let state = if summary.exercised_socket_correction() {
            OnlineSessionUxState::Connected
        } else {
            OnlineSessionUxState::Error
        };
        let presentation = OnlineAuthorityCorrectionPresentation::from_plan(
            summary.correction_plan,
            summary.snap_applied,
        );
        Self {
            state,
            host_owns_save: true,
            player_slot,
            status_message: presentation.status_line(),
        }
    }
}

const fn default_online_session_state() -> OnlineSessionUxState {
    OnlineSessionUxState::Idle
}

fn default_online_player_name() -> String {
    "Player".to_owned()
}

fn default_online_descriptor_path() -> PathBuf {
    crate::save::online_host_descriptor_path()
}

fn join_online_descriptor_path() -> PathBuf {
    crate::save::online_join_descriptor_path()
}

fn default_online_host_bind_addr() -> SocketAddr {
    "0.0.0.0:4242"
        .parse()
        .expect("default online host bind address parses")
}

fn default_online_host_advertise_addr() -> SocketAddr {
    "127.0.0.1:4242"
        .parse()
        .expect("default online host advertise address parses")
}

fn lan_online_host_bind_addr() -> SocketAddr {
    "0.0.0.0:5252"
        .parse()
        .expect("LAN online host bind address parses")
}

fn lan_online_host_advertise_addr() -> SocketAddr {
    "192.168.1.10:5252"
        .parse()
        .expect("LAN online host advertise address parses")
}

fn localhost_ephemeral_online_host_bind_addr() -> SocketAddr {
    "127.0.0.1:0"
        .parse()
        .expect("localhost ephemeral online host bind address parses")
}

fn localhost_ephemeral_online_host_advertise_addr() -> SocketAddr {
    "127.0.0.1:0"
        .parse()
        .expect("localhost ephemeral online host advertise address parses")
}

fn default_online_client_bind_addr() -> SocketAddr {
    "0.0.0.0:0"
        .parse()
        .expect("default online client bind address parses")
}

const fn default_online_gameplay_ticks() -> u32 {
    60
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ModalScreen {
    Fuel,
    FuelConfirm,
    Repair,
    RepairConfirm,
    Depot,
    Headquarters,
    DepotReceiptHistory,
    Shop,
    ShopConfirm,
    Bank,
    Explosives,
    Salvage,
    Options,
    SaveSlots,
    LoadSlots,
    ExitConfirm,
    UnsavedExitConfirm,
    Map,
    Help,
    TownDevelopment,
    ExpeditionBoard,
    ResearchLog,
    OnlineMultiplayer,
    Crafting,
    Inventory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PauseOption {
    Resume,
    Save,
    Load,
    OnlineSession,
    Options,
    ExitToDesktop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TitleOption {
    Resume,
    NewGame,
    OnlineMultiplayer,
    LocalMultiplayer,
    LoadSlot,
    Options,
    Exit,
}

impl TitleOption {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume Saved Session",
            Self::NewGame => "New Game",
            Self::OnlineMultiplayer => "Online Multiplayer",
            Self::LocalMultiplayer => "Local Split-Screen",
            Self::LoadSlot => "Load Slot",
            Self::Options => "Options",
            Self::Exit => "Exit",
        }
    }
}

impl PauseOption {
    pub const ALL: [Self; 6] = [
        Self::Resume,
        Self::Save,
        Self::Load,
        Self::OnlineSession,
        Self::Options,
        Self::ExitToDesktop,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Save => "Save Game",
            Self::Load => "Load Game",
            Self::OnlineSession => "Online Session",
            Self::Options => "Options",
            Self::ExitToDesktop => "Exit to Desktop",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct DustParticle {
    pub x: f32,
    pub y: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum NpcStoryRecord {
    ValeIntro,
    IonaSilverWarning,
    KadeRelicSignal,
    ValeThermalWarning,
    KadeStarCoreSignal,
    ValeStarCoreSecured,
}

impl NpcStoryRecord {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::ValeIntro => "Vale: Profitable Shaft",
            Self::IonaSilverWarning => "Iona: Silver Strata",
            Self::KadeRelicSignal => "Kade: Relic Signals",
            Self::ValeThermalWarning => "Vale: Thermal Readings",
            Self::KadeStarCoreSignal => "Kade: Star Core Harmonics",
            Self::ValeStarCoreSecured => "Vale: Star Core Secured",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CollectionRewardKind {
    Minerals,
    Artifacts,
    Hazards,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CollectionLog {
    #[serde(default)]
    pub minerals: std::collections::BTreeSet<MineralKind>,
    #[serde(default)]
    pub artifacts: std::collections::BTreeSet<ArtifactKind>,
    #[serde(default)]
    pub hazards: std::collections::BTreeSet<TileKind>,
    #[serde(default)]
    pub strata: std::collections::BTreeSet<i32>,
    #[serde(default)]
    pub rewards_claimed: std::collections::BTreeSet<CollectionRewardKind>,
    #[serde(default)]
    pub story_records: std::collections::BTreeSet<NpcStoryRecord>,
}

impl CollectionLog {
    fn discover_tile(&mut self, tile: TileKind) {
        match tile {
            TileKind::Ore(mineral) => {
                self.minerals.insert(mineral);
            }
            TileKind::Artifact(artifact) => {
                self.artifacts.insert(artifact);
            }
            TileKind::Lava
            | TileKind::Gas
            | TileKind::ExplosivePocket
            | TileKind::PressurePocket
            | TileKind::MagmaVent => {
                self.hazards.insert(tile);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServiceAnimation {
    Fuel,
    Repair,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SparkParticle {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SoundCue {
    Drill,
    Sell,
    Upgrade,
    Damage,
    Milestone,
    Rescue,
    Explosion,
    Ui,
}

#[derive(Clone, Debug, Default)]
pub struct VisualChanges {
    pub full_terrain_refresh: bool,
    pub changed_tiles: Vec<TilePosition>,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "game state tracks several orthogonal UI/progression flags"
)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GameState {
    pub terrain: Terrain,
    pub player: Player,
    #[serde(default = "current_save_version")]
    pub save_version: u32,
    pub message: String,
    #[serde(default = "default_online_session_state")]
    pub online_session_state: OnlineSessionUxState,
    #[serde(default)]
    pub online_multiplayer_view: OnlineMultiplayerView,
    #[serde(default)]
    pub online_session_status_message: String,
    #[serde(default)]
    pub online_host_owns_save: bool,
    #[serde(default)]
    pub online_player_slot: Option<u8>,
    #[serde(default)]
    pub online_session_limitations: Vec<String>,
    #[serde(default = "default_online_player_name")]
    pub online_player_name: String,
    #[serde(default)]
    pub online_remote_player_name: Option<String>,
    #[serde(default)]
    pub online_remote_player_ready: bool,
    #[serde(default)]
    pub online_remote_player_connected: bool,
    #[serde(default)]
    pub online_descriptor_inspection_status: String,
    #[serde(default)]
    pub online_descriptor_inspection_can_join: bool,
    #[serde(default)]
    pub online_descriptor_inspection_lines: Vec<String>,
    #[serde(default)]
    pub online_descriptor_inspection_severity: Option<String>,
    #[serde(default = "default_online_descriptor_path")]
    pub online_descriptor_path: PathBuf,
    #[serde(default)]
    pub online_descriptor_path_editing: bool,
    #[serde(default)]
    pub online_descriptor_path_draft: String,
    #[serde(default)]
    pub online_address_edit_target: Option<OnlineAddressEditTarget>,
    #[serde(default)]
    pub online_address_edit_draft: String,
    #[serde(default = "default_online_host_bind_addr")]
    pub online_host_bind_addr: SocketAddr,
    #[serde(default = "default_online_host_advertise_addr")]
    pub online_host_advertise_addr: SocketAddr,
    #[serde(default = "default_online_client_bind_addr")]
    pub online_client_bind_addr: SocketAddr,
    #[serde(default = "default_online_gameplay_ticks")]
    pub online_gameplay_ticks: u32,
    #[serde(default)]
    pub online_runtime_controller_mode: String,
    #[serde(default)]
    pub online_runtime_tick_status: String,
    #[serde(default)]
    pub online_diagnostic_controller_mode: String,
    #[serde(default)]
    pub online_diagnostic_last_tick: String,
    #[serde(default)]
    pub online_last_replication_status: String,
    #[serde(default)]
    pub online_last_replicated_player_status: String,
    #[serde(default)]
    pub online_last_terrain_status: String,
    #[serde(default)]
    pub online_last_authority_status: String,
    #[serde(default)]
    pub online_last_correction_status: String,
    #[serde(default)]
    pub online_last_sync_loop_status: String,
    #[serde(default)]
    pub online_last_session_boundary_status: String,
    #[serde(default)]
    pub online_last_shutdown_summary: String,
    #[serde(default)]
    pub online_session_boundary_history: Vec<String>,
    #[serde(default)]
    pub online_last_failure_status: String,
    #[serde(default)]
    pub online_last_ownership_status: String,
    #[serde(default)]
    pub online_last_live_verification_status: String,
    #[serde(default)]
    pub online_last_gameplay_domain_status: String,
    #[serde(default)]
    pub online_last_gameplay_sync_evidence_status: String,
    #[serde(default)]
    pub online_last_save_boundary_status: String,
    #[serde(default)]
    pub online_last_descriptor_input_status: String,
    #[serde(default)]
    pub online_last_lobby_status: String,
    #[serde(default)]
    pub online_session_roster_status: String,
    #[serde(default)]
    pub online_last_playable_session_status: String,
    #[serde(default)]
    pub online_local_ready: bool,
    #[serde(default, skip)]
    pub online_network_task_request: Option<OnlineNetworkTaskRequest>,
    #[serde(default, skip)]
    pub online_start_session_requested: bool,
    #[serde(default, skip)]
    pub session_slot_io_request: Option<SessionSlotIoRequest>,
    #[serde(default, skip)]
    pub session_service_request: Option<SessionServiceRequest>,
    #[serde(default)]
    pub online_gameplay_entry_source: OnlineGameplayEntrySource,
    #[serde(default)]
    pub online_gameplay_entry_authoritative_tick: Option<crate::multiplayer::SimulationTick>,
    #[serde(default)]
    pub online_last_gameplay_entry_status: String,
    #[serde(default)]
    pub local_multiplayer_requested: bool,
    #[serde(default)]
    pub local_multiplayer_active: bool,
    #[serde(default)]
    pub local_multiplayer_player_slots: u8,
    #[serde(default)]
    pub local_multiplayer_status_message: String,
    #[serde(default)]
    pub online_terrain_sync_markers: Vec<OnlineTerrainSyncMarker>,
    pub current_zone: Option<SurfaceZone>,
    #[serde(default)]
    pub interior_zone: Option<SurfaceZone>,
    #[serde(default)]
    pub interior_x: f32,
    #[serde(default)]
    pub interior_facing: f32,
    pub contracts: ContractLog,
    pub run_mode: RunMode,
    pub modal: Option<ModalScreen>,
    pub selected_menu_item: usize,
    #[serde(default)]
    pub selected_title_item: usize,
    #[serde(default)]
    pub selected_pause_item: usize,
    pub show_details: bool,
    #[serde(default)]
    pub request_exit: bool,
    #[serde(default)]
    pub won_game: bool,
    #[serde(default)]
    pub deep_claim_status: DeepClaimStatus,
    #[serde(default)]
    pub town_development: TownDevelopment,
    #[serde(default)]
    pub collection_log: CollectionLog,
    #[serde(default)]
    pub escape_sequence_seconds: f32,
    #[serde(default)]
    pub explored_tiles: Vec<bool>,
    #[serde(default)]
    pub last_depot_receipt: String,
    #[serde(default)]
    pub depot_receipts: Vec<String>,
    pub deepest_tile_reached: i32,
    #[serde(default)]
    pub total_earnings: u32,
    #[serde(default)]
    pub total_resources_refined: u32,
    #[serde(default)]
    pub best_return_depth: i32,
    #[serde(default)]
    pub most_valuable_cargo_run: u32,
    #[serde(default)]
    pub expeditions_completed: u32,
    #[serde(default)]
    pub infrastructure_built: u32,
    #[serde(default)]
    pub last_run_summary: String,
    #[serde(default)]
    pub fastest_star_core_seconds: Option<f32>,
    #[serde(default)]
    pub legendary_blueprints: std::collections::BTreeSet<LegendaryBlueprint>,
    #[serde(default)]
    pub rig_part_inventory: std::collections::BTreeSet<RigPartKind>,
    #[serde(default)]
    pub equipped_rig_parts: std::collections::BTreeMap<RigSlot, RigPartKind>,
    #[serde(default)]
    pub challenge_badges: std::collections::BTreeSet<ChallengeBadge>,
    #[serde(default)]
    pub cosmetic_skins: std::collections::BTreeSet<CosmeticRigSkin>,
    #[serde(default)]
    pub rescue_count: u32,
    #[serde(default)]
    pub artifacts_found: u32,
    #[serde(default)]
    pub trip_best_depth: i32,
    #[serde(default)]
    pub trip_seconds: f32,
    #[serde(default)]
    pub deep_instability: f32,
    #[serde(default)]
    pub deep_reward_milestone: i32,
    #[serde(default)]
    pub return_streak: u32,
    #[serde(default)]
    pub play_seconds: f32,
    #[serde(default)]
    pub last_delta_seconds: f32,
    #[serde(default)]
    pub update_ticks: u64,
    pub next_milestone_tile: i32,
    #[serde(default)]
    pub current_layer_band: i32,
    pub game_over: bool,
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    #[serde(default = "default_fullscreen")]
    pub fullscreen: bool,
    #[serde(default)]
    pub last_rescue_x: Option<f32>,
    #[serde(default)]
    pub last_rescue_y: Option<f32>,
    #[serde(default)]
    pub last_rescue_summary: String,
    #[serde(default)]
    pub lost_cargo_x: Option<f32>,
    #[serde(default)]
    pub lost_cargo_y: Option<f32>,
    #[serde(default)]
    pub lost_cargo_count: u32,
    #[serde(default)]
    pub lost_minerals: std::collections::BTreeMap<MineralKind, u32>,
    #[serde(default)]
    pub lost_artifacts: std::collections::BTreeMap<ArtifactKind, u32>,
    pub camera_x: f32,
    pub camera_y: f32,
    #[serde(default)]
    pub camera_intro_seconds: f32,
    pub drill_flash_seconds: f32,
    #[serde(default)]
    pub active_drill: Option<DrillState>,
    pub dust_particles: Vec<DustParticle>,
    #[serde(default)]
    pub hazard_clouds: Vec<HazardCloud>,
    #[serde(default)]
    pub placed_bombs: Vec<PlacedBomb>,
    #[serde(default)]
    pub infrastructure: Vec<PlacedInfrastructure>,
    #[serde(default)]
    pub service_animation: Option<ServiceAnimation>,
    #[serde(default)]
    pub service_animation_seconds: f32,
    #[serde(default)]
    pub market_salt: u32,
    #[serde(default)]
    pub market_history: Vec<u32>,
    #[serde(default)]
    pub mineral_market_history: std::collections::BTreeMap<MineralKind, Vec<u32>>,
    #[serde(default)]
    pub scanner_pulse_seconds: f32,
    #[serde(default)]
    pub scanner_cooldown_seconds: f32,
    #[serde(default)]
    pub town_event_day: u32,
    #[serde(default)]
    pub town_event: String,
    #[serde(default)]
    pub active_world_events: Vec<ActiveWorldEvent>,
    #[serde(default)]
    pub scan_markers: Vec<ScanMarker>,
    #[serde(default)]
    pub collapse_warnings: Vec<TilePosition>,
    #[serde(default)]
    pub side_contract_active: bool,
    #[serde(default)]
    pub side_contract_kind: SideContractKind,
    #[serde(default)]
    pub side_contract_target: Option<TileKind>,
    #[serde(default)]
    pub side_contract_required: u32,
    #[serde(default)]
    pub active_side_contracts: Vec<SideContract>,
    #[serde(default)]
    pub expedition_offers: Vec<Expedition>,
    #[serde(default)]
    pub active_expeditions: Vec<Expedition>,
    #[serde(default)]
    pub falling_boulders: Vec<FallingBoulder>,
    #[serde(default)]
    pub spark_particles: Vec<SparkParticle>,
    #[serde(default)]
    pub camera_shake_seconds: f32,
    #[serde(default)]
    pub camera_shake_strength: f32,
    #[serde(default)]
    pub screen_flash_seconds: f32,
    pub sound_cues: Vec<SoundCue>,
    #[serde(default)]
    pub settings_dirty: bool,
    #[serde(default)]
    pub save_dirty: bool,
    #[serde(skip)]
    pub visual_changes: VisualChanges,
}

#[allow(
    dead_code,
    reason = "app shell still being cut over while legacy runtime scaffolding observer is removed"
)]
impl GameState {
    #[must_use]
    pub fn clone_for_save(&self) -> Self {
        let mut saved = self.clone();
        saved.run_mode = RunMode::Playing;
        saved.interior_zone = None;
        saved.modal = None;
        saved.request_exit = false;
        saved.show_details = false;
        saved.save_dirty = false;
        saved.active_drill = None;
        saved.dust_particles.clear();
        saved.hazard_clouds.clear();
        saved.falling_boulders.clear();
        saved.spark_particles.clear();
        saved.camera_shake_seconds = 0.0;
        saved.camera_shake_strength = 0.0;
        saved.camera_intro_seconds = 0.0;
        saved.screen_flash_seconds = 0.0;
        saved.sound_cues.clear();
        saved.settings_dirty = false;
        saved.visual_changes = VisualChanges::default();
        saved.last_delta_seconds = 0.0;
        saved
    }

    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "new game state initializes all saved systems"
    )]
    pub fn new() -> Self {
        Self {
            terrain: Terrain::new_seeded(WORLD_WIDTH, WORLD_HEIGHT, WORLD_SEED),
            player: Player::new(PLAYER_SPAWN_X, PLAYER_SPAWN_Y),
            save_version: current_save_version(),
            message: "Mine ore, sell cargo, and buy upgrades. Press E at surface buildings."
                .to_owned(),
            online_session_state: OnlineSessionUxState::Idle,
            online_multiplayer_view: OnlineMultiplayerView::MainMenu,
            online_session_status_message: String::new(),
            online_host_owns_save: false,
            online_player_slot: None,
            online_session_limitations: Self::online_session_limitations(),
            online_player_name: default_online_player_name(),
            online_remote_player_name: None,
            online_remote_player_ready: false,
            online_remote_player_connected: false,
            online_descriptor_inspection_status: String::new(),
            online_descriptor_inspection_can_join: false,
            online_descriptor_inspection_lines: Vec::new(),
            online_descriptor_inspection_severity: None,
            online_descriptor_path: default_online_descriptor_path(),
            online_descriptor_path_editing: false,
            online_descriptor_path_draft: String::new(),
            online_address_edit_target: None,
            online_address_edit_draft: String::new(),
            online_host_bind_addr: default_online_host_bind_addr(),
            online_host_advertise_addr: default_online_host_advertise_addr(),
            online_client_bind_addr: default_online_client_bind_addr(),
            online_gameplay_ticks: default_online_gameplay_ticks(),
            online_runtime_controller_mode: String::new(),
            online_runtime_tick_status: String::new(),
            online_diagnostic_controller_mode: String::new(),
            online_diagnostic_last_tick: String::new(),
            online_last_replication_status: String::new(),
            online_last_replicated_player_status: String::new(),
            online_last_terrain_status: String::new(),
            online_last_authority_status: String::new(),
            online_last_correction_status: String::new(),
            online_last_sync_loop_status: String::new(),
            online_last_session_boundary_status: String::new(),
            online_last_shutdown_summary: String::new(),
            online_session_boundary_history: Vec::new(),
            online_last_failure_status: String::new(),
            online_last_ownership_status: String::new(),
            online_last_live_verification_status: String::new(),
            online_last_gameplay_domain_status: String::new(),
            online_last_gameplay_sync_evidence_status: String::new(),
            online_last_save_boundary_status: String::new(),
            online_last_descriptor_input_status: String::new(),
            online_last_lobby_status: String::new(),
            online_session_roster_status: String::new(),
            online_last_playable_session_status: String::new(),
            online_local_ready: false,
            online_network_task_request: None,
            online_start_session_requested: false,
            session_slot_io_request: None,
            session_service_request: None,
            online_gameplay_entry_source: OnlineGameplayEntrySource::None,
            online_gameplay_entry_authoritative_tick: None,
            online_last_gameplay_entry_status: String::new(),
            local_multiplayer_requested: false,
            local_multiplayer_active: false,
            local_multiplayer_player_slots: 1,
            local_multiplayer_status_message: String::new(),
            online_terrain_sync_markers: Vec::new(),
            current_zone: None,
            interior_zone: None,
            interior_x: 88.0,
            interior_facing: 1.0,
            contracts: ContractLog::new(),
            run_mode: RunMode::Title,
            modal: None,
            selected_menu_item: 0,
            selected_title_item: 0,
            selected_pause_item: 0,
            show_details: false,
            request_exit: false,
            won_game: false,
            deep_claim_status: DeepClaimStatus::Locked,
            town_development: TownDevelopment::default(),
            collection_log: CollectionLog::default(),
            escape_sequence_seconds: 0.0,
            explored_tiles: vec![false; (WORLD_WIDTH * WORLD_HEIGHT) as usize],
            last_depot_receipt: String::new(),
            depot_receipts: Vec::new(),
            deepest_tile_reached: 0,
            total_earnings: 0,
            total_resources_refined: 0,
            best_return_depth: 0,
            most_valuable_cargo_run: 0,
            expeditions_completed: 0,
            infrastructure_built: 0,
            last_run_summary: String::new(),
            fastest_star_core_seconds: None,
            legendary_blueprints: std::collections::BTreeSet::new(),
            rig_part_inventory: std::collections::BTreeSet::from([
                RigPartKind::TitanDrill,
                RigPartKind::NeedleDrill,
                RigPartKind::ResonanceDrill,
                RigPartKind::LightweightEngine,
                RigPartKind::HaulerEngine,
                RigPartKind::BurstEngine,
                RigPartKind::ThermalHull,
                RigPartKind::ImpactHull,
                RigPartKind::PressureHull,
                RigPartKind::ProspectorScanner,
                RigPartKind::HazardScanner,
                RigPartKind::RelicScanner,
                RigPartKind::CargoBalloon,
                RigPartKind::ArmoredCargoBay,
                RigPartKind::SortedCargoRack,
            ]),
            equipped_rig_parts: std::collections::BTreeMap::new(),
            challenge_badges: std::collections::BTreeSet::new(),
            cosmetic_skins: std::collections::BTreeSet::new(),
            rescue_count: 0,
            artifacts_found: 0,
            trip_best_depth: 0,
            trip_seconds: 0.0,
            deep_instability: 0.0,
            deep_reward_milestone: 0,
            return_streak: 0,
            play_seconds: 0.0,
            last_delta_seconds: 0.0,
            update_ticks: 0,
            next_milestone_tile: 20,
            current_layer_band: 0,
            game_over: false,
            master_volume: default_master_volume(),
            fullscreen: default_fullscreen(),
            last_rescue_x: None,
            last_rescue_y: None,
            last_rescue_summary: String::new(),
            lost_cargo_x: None,
            lost_cargo_y: None,
            lost_cargo_count: 0,
            lost_minerals: std::collections::BTreeMap::new(),
            lost_artifacts: std::collections::BTreeMap::new(),
            camera_x: initial_camera_x(),
            camera_y: initial_camera_y(),
            camera_intro_seconds: CAMERA_INTRO_SECONDS,
            drill_flash_seconds: 0.0,
            active_drill: None,
            dust_particles: Vec::new(),
            hazard_clouds: Vec::new(),
            placed_bombs: Vec::new(),
            infrastructure: Vec::new(),
            service_animation: None,
            service_animation_seconds: 0.0,
            market_salt: 0,
            market_history: vec![market_factor(0, 0)],
            mineral_market_history: initial_mineral_market_history(0, 0),
            scanner_pulse_seconds: 0.0,
            scanner_cooldown_seconds: 0.0,
            town_event_day: 0,
            town_event: "Normal market conditions.".to_owned(),
            active_world_events: Vec::new(),
            scan_markers: Vec::new(),
            collapse_warnings: Vec::new(),
            side_contract_active: false,
            side_contract_kind: SideContractKind::Cargo,
            side_contract_target: None,
            side_contract_required: 0,
            active_side_contracts: Vec::new(),
            expedition_offers: Vec::new(),
            active_expeditions: Vec::new(),
            falling_boulders: Vec::new(),
            spark_particles: Vec::new(),
            camera_shake_seconds: 0.0,
            camera_shake_strength: 0.0,
            screen_flash_seconds: 0.0,
            sound_cues: Vec::new(),
            settings_dirty: false,
            save_dirty: false,
            visual_changes: VisualChanges {
                full_terrain_refresh: true,
                changed_tiles: Vec::new(),
            },
        }
    }

    #[must_use]
    pub const fn visual_changes(&self) -> &VisualChanges {
        &self.visual_changes
    }

    pub fn drain_visual_changes(&mut self) -> VisualChanges {
        mem::take(&mut self.visual_changes)
    }

    pub const fn mark_full_terrain_refresh(&mut self) {
        self.visual_changes.full_terrain_refresh = true;
    }

    fn mark_tile_visual_changed(&mut self, position: TilePosition) {
        self.visual_changes.changed_tiles.push(position);
    }

    fn mark_exploration_visual_changed(&mut self, position: TilePosition) {
        for y in position.y - EXPLORATION_VISUAL_CHANGE_RADIUS_TILES
            ..=position.y + EXPLORATION_VISUAL_CHANGE_RADIUS_TILES
        {
            for x in position.x - EXPLORATION_VISUAL_CHANGE_RADIUS_TILES
                ..=position.x + EXPLORATION_VISUAL_CHANGE_RADIUS_TILES
            {
                self.mark_tile_visual_changed(TilePosition { x, y });
            }
        }
    }

    pub fn apply_session_exploration<I>(&mut self, positions: I) -> usize
    where
        I: IntoIterator<Item = TilePosition>,
    {
        let mut revealed = 0;
        for position in positions {
            if let Some(index) = self.tile_index(position)
                && !self.explored_tiles[index]
            {
                self.explored_tiles[index] = true;
                self.mark_exploration_visual_changed(position);
                revealed += 1;
            }
        }
        revealed
    }

    fn mark_tiles_visual_changed<I>(&mut self, positions: I)
    where
        I: IntoIterator<Item = TilePosition>,
    {
        self.visual_changes.changed_tiles.extend(positions);
    }

    pub fn migrate_after_load(&mut self) {
        self.terrain.ensure_depth(WORLD_HEIGHT);
        let expected_tiles = (self.terrain.width() * self.terrain.height()) as usize;
        if self.explored_tiles.len() != expected_tiles {
            self.explored_tiles = vec![false; expected_tiles];
        }
        self.request_exit = false;
        self.save_dirty = false;
        self.visual_changes = VisualChanges {
            full_terrain_refresh: true,
            changed_tiles: Vec::new(),
        };
        self.contracts.migrate_after_load();
    }

    #[allow(
        clippy::too_many_lines,
        reason = "top-level mode dispatcher keeps frame order explicit"
    )]
    pub fn update_shell_after_session_authority(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
    ) -> SessionShellUpdateSummary {
        self.last_delta_seconds = delta_seconds;
        self.update_ticks = self.update_ticks.saturating_add(1);
        self.sound_cues.clear();
        self.show_details = input.details;
        self.handle_save_load(input);
        if self.handle_exit_modal(input) {
            return SessionShellUpdateSummary::ShellHandled;
        }
        if input.exit_requested {
            self.request_exit_or_prompt();
            return SessionShellUpdateSummary::ExitRequested;
        }
        if self.run_mode != RunMode::Title && input_changes_game(input) {
            self.save_dirty = true;
        }
        if input.inventory {
            self.modal = if self.modal == Some(ModalScreen::Inventory) {
                None
            } else {
                Some(ModalScreen::Inventory)
            };
        }
        if input.map {
            self.modal = if self.modal == Some(ModalScreen::Map) {
                None
            } else {
                Some(ModalScreen::Map)
            };
        }
        if input.help {
            self.modal = if self.modal == Some(ModalScreen::Help) {
                None
            } else {
                Some(ModalScreen::Help)
            };
        }
        if input.volume_up {
            self.master_volume = (self.master_volume + 0.1).min(1.0);
            self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if input.volume_down {
            self.master_volume = (self.master_volume - 0.1).max(0.0);
            self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if input.fullscreen {
            self.fullscreen = !self.fullscreen;
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        self.update_persistent_ore_prediction();
        self.update_online_terrain_sync_markers(delta_seconds);
        self.drill_flash_seconds = (self.drill_flash_seconds - delta_seconds).max(0.0);
        if matches!(self.run_mode, RunMode::Playing | RunMode::Interior)
            && !self.game_over
            && !self.won_game
        {
            self.play_seconds += delta_seconds;
        }
        match self.run_mode {
            RunMode::Title => {
                if self.handle_modal(input) {
                    return SessionShellUpdateSummary::ShellHandled;
                }
                self.handle_title_menu(input);
                return SessionShellUpdateSummary::ShellHandled;
            }
            RunMode::Paused => {
                self.handle_pause_menu(input);
                return SessionShellUpdateSummary::ShellHandled;
            }
            RunMode::Playing | RunMode::Interior => {}
        }
        if self.run_mode == RunMode::Interior {
            self.handle_interior(input, delta_seconds);
            return SessionShellUpdateSummary::ShellHandled;
        }
        if self.game_over {
            self.update_camera(delta_seconds);
            return SessionShellUpdateSummary::ShellHandled;
        }
        self.update_npc_story_records();
        if self.handle_modal(input) {
            self.update_camera(delta_seconds);
            return SessionShellUpdateSummary::ShellHandled;
        }
        if input.pause || input.cancel {
            self.run_mode = RunMode::Paused;
            return SessionShellUpdateSummary::Paused;
        }
        self.handle_interaction(input);
        self.update_warning_messages();
        self.update_status_messages();
        self.update_camera(delta_seconds);
        SessionShellUpdateSummary::GameplayPresentationUpdated
    }

    #[allow(
        clippy::too_many_lines,
        reason = "legacy app-shell update remains monolithic while gameplay authority is moved into GameSession/WorldState"
    )]
    pub fn update(&mut self, input: PlayerInput, delta_seconds: f32) {
        self.last_delta_seconds = delta_seconds;
        self.update_ticks = self.update_ticks.saturating_add(1);
        self.sound_cues.clear();
        self.show_details = input.details;
        self.handle_save_load(input);
        if self.handle_exit_modal(input) {
            return;
        }
        if input.exit_requested {
            self.request_exit_or_prompt();
            return;
        }
        if self.run_mode != RunMode::Title && input_changes_game(input) {
            self.save_dirty = true;
        }
        if input.inventory {
            self.modal = if self.modal == Some(ModalScreen::Inventory) {
                None
            } else {
                Some(ModalScreen::Inventory)
            };
        }
        if input.map {
            self.modal = if self.modal == Some(ModalScreen::Map) {
                None
            } else {
                Some(ModalScreen::Map)
            };
        }
        if input.help {
            self.modal = if self.modal == Some(ModalScreen::Help) {
                None
            } else {
                Some(ModalScreen::Help)
            };
        }
        if input.volume_up {
            self.master_volume = (self.master_volume + 0.1).min(1.0);
            self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if input.volume_down {
            self.master_volume = (self.master_volume - 0.1).max(0.0);
            self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        if input.fullscreen {
            self.fullscreen = !self.fullscreen;
            self.message = if self.fullscreen {
                "Fullscreen preference saved. Restart/toggle window integration pending.".to_owned()
            } else {
                "Windowed preference saved.".to_owned()
            };
            self.settings_dirty = true;
            self.sound_cues.push(SoundCue::Ui);
        }
        self.update_particles(delta_seconds);
        self.update_placed_bombs(delta_seconds);
        self.update_service_animation(delta_seconds);
        self.update_scanner_timers(delta_seconds);
        self.update_boulders(delta_seconds);
        self.update_world_event_hazards();
        self.camera_shake_seconds = (self.camera_shake_seconds - delta_seconds).max(0.0);
        self.screen_flash_seconds = (self.screen_flash_seconds - delta_seconds).max(0.0);
        self.update_hazards(delta_seconds);
        self.recover_lost_cargo_if_near();
        self.reveal_near_player();
        self.reveal_scanner_area();
        self.update_collection_rewards();
        self.update_survey_drones();
        self.update_persistent_ore_prediction();
        self.drill_flash_seconds = (self.drill_flash_seconds - delta_seconds).max(0.0);
        if matches!(self.run_mode, RunMode::Playing | RunMode::Interior)
            && !self.game_over
            && !self.won_game
        {
            self.play_seconds += delta_seconds;
        }

        match self.run_mode {
            RunMode::Title => {
                if self.handle_modal(input) {
                    return;
                }
                self.handle_title_menu(input);
                return;
            }
            RunMode::Paused => {
                self.handle_pause_menu(input);
                return;
            }
            RunMode::Playing | RunMode::Interior => {}
        }

        if self.run_mode == RunMode::Interior {
            self.handle_interior(input, delta_seconds);
            return;
        }

        if self.game_over {
            self.handle_rescue(input);
            self.update_camera(delta_seconds);
            return;
        }

        self.current_zone = surface_zone_at(self.player.x, self.player.y);
        self.update_npc_story_records();
        if self.handle_modal(input) {
            self.update_camera(delta_seconds);
            return;
        }

        if input.pause || input.cancel {
            self.run_mode = RunMode::Paused;
            return;
        }

        self.handle_interaction(input);
        self.handle_scanner(input);
        self.handle_bomb(input);
        self.handle_infrastructure_placement(input);
        self.apply_movement(input, delta_seconds);
        self.update_drilling(input, delta_seconds);
        self.apply_depth_pressure(delta_seconds);
        self.apply_lava_damage(delta_seconds);
        self.update_depth_milestones();
        self.update_deep_run_pressure(delta_seconds);
        self.update_escape_sequence(delta_seconds);
        self.update_layer_band();
        self.award_return_bonus();
        self.update_warning_messages();
        self.update_status_messages();
        self.check_failure();
        self.update_camera(delta_seconds);
    }

    fn handle_title_menu(&mut self, input: PlayerInput) {
        let options = Self::title_options();
        if input.menu_up {
            self.selected_title_item = self.selected_title_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_title_item = (self.selected_title_item + 1).min(options.len() - 1);
        }
        self.selected_title_item = self.selected_title_item.min(options.len() - 1);

        if input.cancel {
            self.request_exit = true;
            return;
        }
        if !input.confirm {
            return;
        }

        match options[self.selected_title_item] {
            TitleOption::Resume => self.load_latest_into_self(),
            TitleOption::NewGame => self.start_new_game(),
            TitleOption::OnlineMultiplayer => {
                self.modal = Some(ModalScreen::OnlineMultiplayer);
                self.selected_menu_item = 0;
            }
            TitleOption::LocalMultiplayer => self.start_local_multiplayer_request(),
            TitleOption::LoadSlot => {
                self.modal = Some(ModalScreen::LoadSlots);
                self.selected_menu_item = 0;
            }
            TitleOption::Options => {
                self.modal = Some(ModalScreen::Options);
                self.selected_menu_item = 0;
            }
            TitleOption::Exit => self.request_exit = true,
        }
    }

    #[must_use]
    pub fn title_options() -> Vec<TitleOption> {
        if saves_exist() {
            vec![
                TitleOption::Resume,
                TitleOption::NewGame,
                TitleOption::LocalMultiplayer,
                TitleOption::OnlineMultiplayer,
                TitleOption::LoadSlot,
                TitleOption::Options,
                TitleOption::Exit,
            ]
        } else {
            vec![
                TitleOption::NewGame,
                TitleOption::LocalMultiplayer,
                TitleOption::OnlineMultiplayer,
                TitleOption::Options,
                TitleOption::Exit,
            ]
        }
    }

    #[allow(
        dead_code,
        reason = "online task request drain is exercised by tests until desktop event-loop ownership calls it"
    )]
    pub fn request_online_shutdown_from_gameplay_exit(&mut self) -> bool {
        if !matches!(
            self.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
        ) {
            return false;
        }
        if self.online_network_task_request == Some(OnlineNetworkTaskRequest::Shutdown) {
            return false;
        }
        self.online_network_task_request = Some(OnlineNetworkTaskRequest::Shutdown);
        self.online_local_ready = false;
        self.online_remote_player_ready = false;
        self.online_remote_player_connected = false;
        self.apply_online_session_boundary_status(
            &OnlineSessionBoundaryStatus::local_shutdown_requested(),
        );
        self.message = format!(
            "shutdown requested; {}",
            OnlineLeaveEndSafetyStatus::from_game(self).status
        );
        true
    }

    pub fn take_online_network_task_request(&mut self) -> Option<OnlineNetworkTaskRequest> {
        mem::take(&mut self.online_network_task_request)
    }

    pub fn take_online_start_session_request(&mut self) -> bool {
        mem::take(&mut self.online_start_session_requested)
    }

    pub fn note_online_host_start_sent(
        &mut self,
        authoritative_tick: crate::multiplayer::SimulationTick,
    ) {
        self.online_gameplay_entry_source = OnlineGameplayEntrySource::HostStartSent;
        self.online_gameplay_entry_authoritative_tick = Some(authoritative_tick);
        self.refresh_online_gameplay_entry_status();
    }

    pub fn refresh_online_gameplay_entry_status(&mut self) {
        self.online_last_gameplay_entry_status =
            OnlineGameplayEntryPresentation::from_game(self).status;
    }

    #[allow(
        dead_code,
        reason = "online task reducer is exercised by tests until desktop event-loop ownership calls it"
    )]
    pub fn apply_online_network_task_result(
        &mut self,
        result: OnlineNetworkTaskResult,
    ) -> OnlineTaskResultTransition {
        let transition_kind = match result {
            OnlineNetworkTaskResult::Hosted(snapshot) => {
                self.apply_real_online_session_ux(snapshot);
                OnlineTaskResultTransitionKind::HostWaitingForJoin
            }
            OnlineNetworkTaskResult::JoinedDescriptor(snapshot) => {
                self.apply_real_online_session_ux(snapshot);
                OnlineTaskResultTransitionKind::JoinedWaitingForStart
            }
            OnlineNetworkTaskResult::Connected(snapshot)
            | OnlineNetworkTaskResult::Reconnected(snapshot) => {
                self.apply_real_online_session_ux(snapshot);
                self.enter_online_playing_session();
                OnlineTaskResultTransitionKind::EnteredGameplay
            }
            OnlineNetworkTaskResult::Failed(message) => {
                self.online_session_state = OnlineSessionUxState::Error;
                self.online_network_task_request = None;
                self.online_local_ready = false;
                self.online_remote_player_ready = false;
                self.online_remote_player_connected = false;
                self.clear_online_diagnostics();
                let failure_status = OnlineFailureStatus::classify(&message);
                self.apply_online_failure_status(&failure_status);
                OnlineTaskResultTransitionKind::Failed
            }
            OnlineNetworkTaskResult::Shutdown(summary) => {
                self.online_session_state = OnlineSessionUxState::Shutdown;
                self.online_network_task_request = None;
                self.online_local_ready = false;
                self.online_remote_player_ready = false;
                self.online_remote_player_connected = false;
                self.modal = None;
                self.clear_online_diagnostics();
                self.online_last_shutdown_summary = summary.status_line();
                self.apply_online_session_boundary_status(
                    &OnlineSessionBoundaryStatus::shutdown_acknowledged(),
                );
                OnlineTaskResultTransitionKind::Shutdown
            }
        };
        self.refresh_online_runtime_statuses();
        OnlineTaskResultTransition::from_game(
            transition_kind,
            self,
            self.online_session_status_message.clone(),
        )
    }

    #[allow(
        dead_code,
        reason = "real online UX bridge is exercised by tests until desktop async UI ownership calls it"
    )]
    pub fn apply_real_online_session_ux(&mut self, snapshot: RealOnlineSessionUxSnapshot) {
        self.online_session_state = snapshot.state;
        self.online_host_owns_save = snapshot.host_owns_save;
        self.online_player_slot = snapshot.player_slot;
        self.online_remote_player_connected = snapshot.state == OnlineSessionUxState::Connected;
        if self.online_remote_player_connected && self.online_remote_player_name.is_none() {
            self.online_remote_player_name = Some("Remote miner".to_owned());
        }
        self.online_session_status_message = snapshot.status_message;
        self.message = self.online_session_status_message.clone();
        self.refresh_online_runtime_statuses();
    }

    pub fn apply_online_remote_identity(
        &mut self,
        player_id: crate::multiplayer::PlayerId,
        name: &str,
    ) {
        let previous_name = self.online_remote_player_name.clone();
        self.online_remote_player_name = Some(name.to_owned());
        self.online_remote_player_connected = true;
        if previous_name.as_deref() != Some(name) {
            self.online_session_status_message =
                format!("Remote player identity synced: p{} {name}", player_id.get());
        }
        self.refresh_online_lobby_status();
    }

    pub fn apply_online_remote_ready_state(
        &mut self,
        player_id: crate::multiplayer::PlayerId,
        ready: bool,
    ) {
        let previous_ready = self.online_remote_player_ready;
        self.online_remote_player_ready = ready;
        self.online_remote_player_connected = true;
        if previous_ready != ready {
            self.online_session_status_message = format!(
                "Remote player p{} ready state synced: {}",
                player_id.get(),
                if ready { "ready" } else { "not ready" }
            );
        }
        self.refresh_online_lobby_status();
    }

    pub fn apply_online_runtime_tick_status(
        &mut self,
        presentation: OnlineRuntimeTickPresentation,
    ) {
        self.online_runtime_controller_mode = presentation.controller_mode;
        self.online_runtime_tick_status = presentation.tick_status;
        self.online_diagnostic_controller_mode
            .clone_from(&self.online_runtime_controller_mode);
        self.online_diagnostic_last_tick
            .clone_from(&self.online_runtime_tick_status);
    }

    pub fn apply_online_diagnostics(
        &mut self,
        controller_mode: impl Into<String>,
        last_tick: impl Into<String>,
    ) {
        self.apply_online_runtime_tick_status(OnlineRuntimeTickPresentation::new(
            controller_mode,
            last_tick,
        ));
    }

    pub fn apply_online_replication_status(&mut self, status: impl Into<String>) {
        self.online_last_replication_status = status.into();
        self.refresh_online_live_verification_status();
        self.refresh_online_gameplay_domain_status();
        self.refresh_online_playable_session_status();
    }

    pub fn apply_online_replicated_player_status(&mut self, status: impl Into<String>) {
        self.online_last_replicated_player_status = status.into();
        self.refresh_online_live_verification_status();
        self.refresh_online_gameplay_domain_status();
        self.refresh_online_playable_session_status();
    }

    pub fn apply_online_terrain_status(&mut self, status: impl Into<String>) {
        self.online_last_terrain_status = status.into();
        self.refresh_online_live_verification_status();
        self.refresh_online_gameplay_domain_status();
        self.refresh_online_playable_session_status();
    }

    pub fn mark_online_terrain_sync_positions(
        &mut self,
        positions: impl IntoIterator<Item = TilePosition>,
    ) -> OnlineTerrainSyncMarkerBatch {
        const MAX_ONLINE_TERRAIN_MARKERS: usize = 48;
        let mut markers_added = 0;
        for position in positions {
            if let Some(marker) = self
                .online_terrain_sync_markers
                .iter_mut()
                .find(|marker| marker.position == position)
            {
                marker.seconds_remaining = OnlineTerrainSyncMarker::LIFETIME_SECONDS;
                continue;
            }
            self.online_terrain_sync_markers
                .push(OnlineTerrainSyncMarker::new(position));
            markers_added += 1;
        }
        if self.online_terrain_sync_markers.len() > MAX_ONLINE_TERRAIN_MARKERS {
            let excess = self.online_terrain_sync_markers.len() - MAX_ONLINE_TERRAIN_MARKERS;
            self.online_terrain_sync_markers.drain(0..excess);
        }
        if markers_added > 0 {
            self.apply_online_terrain_status(format!(
                "{markers_added} replicated terrain tile(s) highlighted for remote mining visibility"
            ));
        }
        OnlineTerrainSyncMarkerBatch {
            markers_added,
            marker_count: self.online_terrain_sync_markers.len(),
            latest_status: self.online_last_terrain_status.clone(),
        }
    }

    fn update_online_terrain_sync_markers(&mut self, delta_seconds: f32) {
        for marker in &mut self.online_terrain_sync_markers {
            marker.seconds_remaining = (marker.seconds_remaining - delta_seconds).max(0.0);
        }
        self.online_terrain_sync_markers
            .retain(|marker| marker.seconds_remaining > 0.0);
    }

    pub fn apply_online_authority_correction_status(
        &mut self,
        presentation: &OnlineAuthorityCorrectionPresentation,
    ) {
        self.online_last_authority_status
            .clone_from(&presentation.player_message);
        self.online_last_correction_status = presentation.status_line();
        self.refresh_online_gameplay_domain_status();
    }

    pub fn apply_online_sync_loop_status(&mut self, status: OnlineSyncLoopStatus) {
        self.online_last_sync_loop_status = status.status;
        self.refresh_online_live_verification_status();
        self.refresh_online_gameplay_domain_status();
        self.refresh_online_playable_session_status();
    }

    pub fn surface_online_session_boundary_during_gameplay(
        &mut self,
        status: &OnlineSessionBoundaryStatus,
    ) {
        let was_playing = self.run_mode == RunMode::Playing;
        if matches!(
            status.cause,
            OnlineSessionBoundaryCause::HostEndedSession
                | OnlineSessionBoundaryCause::TransportClosed
        ) {
            self.online_session_state = OnlineSessionUxState::Disconnected;
        }
        if was_playing && !status.local_session_active {
            self.run_mode = RunMode::Paused;
            self.modal = Some(ModalScreen::OnlineMultiplayer);
        }
        self.refresh_online_save_boundary_status();
    }

    pub fn apply_online_session_boundary_status(&mut self, status: &OnlineSessionBoundaryStatus) {
        self.online_last_session_boundary_status = status.status_line();
        self.record_online_session_boundary_status();
        self.online_remote_player_connected = status.remote_connected;
        if !status.remote_connected {
            self.online_remote_player_ready = false;
        }
        self.online_session_status_message
            .clone_from(&status.player_message);
        self.message.clone_from(&self.online_session_status_message);
        self.surface_online_session_boundary_during_gameplay(status);
        self.refresh_online_runtime_statuses();
    }

    fn record_online_session_boundary_status(&mut self) {
        const MAX_ONLINE_BOUNDARY_HISTORY: usize = 6;
        if self.online_last_session_boundary_status.is_empty() {
            return;
        }
        self.online_session_boundary_history
            .push(self.online_last_session_boundary_status.clone());
        if self.online_session_boundary_history.len() > MAX_ONLINE_BOUNDARY_HISTORY {
            let excess = self.online_session_boundary_history.len() - MAX_ONLINE_BOUNDARY_HISTORY;
            self.online_session_boundary_history.drain(0..excess);
        }
    }

    pub fn apply_online_failure_status(&mut self, status: &OnlineFailureStatus) {
        self.online_last_failure_status = status.status_line();
        self.online_session_status_message
            .clone_from(&status.player_message);
        self.message.clone_from(&self.online_session_status_message);
        self.refresh_online_runtime_statuses();
    }

    pub fn refresh_online_ownership_status(&mut self) {
        self.online_last_ownership_status = OnlineOwnershipStatus::from_game(self).status_line();
    }

    pub fn refresh_online_live_verification_status(&mut self) {
        self.online_last_live_verification_status =
            OnlineLiveVerificationStatus::from_game(self).status;
    }

    pub fn refresh_online_gameplay_domain_status(&mut self) {
        self.online_last_gameplay_domain_status =
            OnlineGameplayDomainStatus::from_game(self).status;
        self.refresh_online_gameplay_sync_evidence_status();
    }

    pub fn refresh_online_gameplay_sync_evidence_status(&mut self) {
        self.online_last_gameplay_sync_evidence_status =
            OnlineGameplaySyncEvidenceMatrix::from_game(self).status;
    }

    pub fn refresh_online_save_boundary_status(&mut self) {
        self.online_last_save_boundary_status =
            OnlineSaveBoundaryStatus::from_game(self).status_line();
    }

    pub fn apply_online_save_boundary_status(&mut self, status: &OnlineSaveBoundaryStatus) {
        self.online_last_save_boundary_status = status.status_line();
        self.online_session_status_message
            .clone_from(&status.player_message);
        self.message.clone_from(&self.online_session_status_message);
    }

    pub fn apply_online_descriptor_input_status(&mut self, status: &OnlineDescriptorInputStatus) {
        self.online_last_descriptor_input_status = status.status_line();
        self.online_session_status_message
            .clone_from(&status.message);
        self.message.clone_from(&self.online_session_status_message);
    }

    #[allow(
        dead_code,
        reason = "used by modal/status tests and available to UI editing callbacks as descriptor input productizes"
    )]
    pub fn refresh_online_descriptor_input_status(&mut self, mode: OnlineDescriptorInputMode) {
        let status = OnlineDescriptorInputStatus::validate(mode, &self.online_descriptor_path);
        self.online_last_descriptor_input_status = status.status_line();
    }

    pub fn refresh_online_lobby_status(&mut self) {
        self.online_last_lobby_status = OnlineLobbyStatus::from_game(self).status;
    }

    pub fn refresh_online_playable_session_status(&mut self) {
        self.online_last_playable_session_status =
            OnlinePlayableSessionStatus::from_game(self).status;
    }

    pub fn refresh_online_runtime_statuses(&mut self) {
        self.refresh_online_ownership_status();
        self.refresh_online_live_verification_status();
        self.refresh_online_gameplay_domain_status();
        self.refresh_online_save_boundary_status();
        self.refresh_online_lobby_status();
        self.refresh_online_playable_session_status();
    }

    pub fn clear_online_diagnostics(&mut self) {
        self.online_runtime_controller_mode.clear();
        self.online_runtime_tick_status.clear();
        self.online_diagnostic_controller_mode.clear();
        self.online_diagnostic_last_tick.clear();
        self.online_last_replication_status.clear();
        self.online_last_replicated_player_status.clear();
        self.online_last_terrain_status.clear();
        self.online_last_authority_status.clear();
        self.online_last_correction_status.clear();
        self.online_last_sync_loop_status.clear();
        self.online_last_live_verification_status.clear();
        self.online_last_gameplay_domain_status.clear();
        self.online_last_gameplay_sync_evidence_status.clear();
    }

    #[allow(
        dead_code,
        reason = "kept as compatibility wrapper for tests and UI call sites while structured failure status rolls out"
    )]
    #[must_use]
    pub fn online_failure_status_message(error: &str) -> String {
        OnlineFailureStatus::classify(error).player_message
    }

    const fn online_local_save_allowed(&self) -> bool {
        !matches!(
            self.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
        ) || self.online_host_owns_save
    }

    #[must_use]
    pub const fn can_write_local_save(&self) -> bool {
        self.online_local_save_allowed()
    }

    #[must_use]
    pub const fn online_save_exit_policy(&self) -> OnlineSaveExitPolicy {
        let local_allowed = self.online_local_save_allowed();
        OnlineSaveExitPolicy {
            save_authority: if local_allowed {
                OnlineSaveAuthority::LocalPlayer
            } else {
                OnlineSaveAuthority::RemoteHost
            },
            local_save_allowed: local_allowed,
            local_load_allowed: local_allowed,
            save_before_exit_allowed: local_allowed && self.save_dirty,
            unsaved_exit_action: if self.save_dirty {
                if local_allowed {
                    OnlineUnsavedExitAction::SaveAndExitAllowed
                } else {
                    OnlineUnsavedExitAction::DiscardOrCancelOnly
                }
            } else {
                OnlineUnsavedExitAction::CleanExitAllowed
            },
        }
    }

    fn block_joined_client_load(&mut self) -> bool {
        let decision =
            OnlineLocalPersistenceDecision::from_game(self, OnlineLocalPersistenceAction::Load);
        if decision.allowed {
            return false;
        }
        decision.player_message.clone_into(&mut self.message);
        self.apply_online_save_boundary_status(&OnlineSaveBoundaryStatus::blocked_save(
            &decision.player_message,
        ));
        true
    }

    fn block_joined_client_save(&mut self) -> bool {
        let decision =
            OnlineLocalPersistenceDecision::from_game(self, OnlineLocalPersistenceAction::Save);
        if decision.allowed {
            return false;
        }
        decision.player_message.clone_into(&mut self.message);
        self.apply_online_save_boundary_status(&OnlineSaveBoundaryStatus::blocked_save(
            &decision.player_message,
        ));
        true
    }

    fn enter_online_playing_session(&mut self) {
        self.run_mode = RunMode::Playing;
        self.modal = None;
        self.selected_menu_item = 0;
        self.local_multiplayer_requested = false;
        self.local_multiplayer_active = false;
        self.online_local_ready = true;
        if self.online_session_status_message.is_empty() {
            "Online session connected; entering gameplay."
                .clone_into(&mut self.online_session_status_message);
        }
        self.message = self.online_session_status_message.clone();
    }

    #[allow(clippy::too_many_lines)]
    #[must_use]
    fn online_multiplayer_status_lines_without_production_ui_gate(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(10);
        lines.push(
            "Transport: Quinn/QUIC real socket IO enabled for direct-connect host/join/reconnect."
                .to_owned(),
        );
        if let Some(request) = &self.online_network_task_request {
            lines.push(format!("Pending network task: {request:?}"));
        } else {
            lines.push("Pending network task: none".to_owned());
        }
        lines.push(format!(
            "State: {:?} | Role: {} | Ready: {} | Host owns save: {} | Slot: {}",
            self.online_session_state,
            self.online_role_label(),
            if self.online_local_ready { "yes" } else { "no" },
            self.online_host_owns_save,
            self.online_player_slot
                .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string())
        ));
        if !self.online_session_roster_status.is_empty() {
            lines.push(format!(
                "Session roster: {}",
                self.online_session_roster_status
            ));
        }
        lines.push(LocalMultiplayerRuntimeStatus::from_game(self).status_line());
        lines.push(OnlineSessionUxReducerStatus::from_game(self).status);
        lines.push(OnlineNetworkTaskOrchestrationStatus::from_game(self).status);
        lines.push(MultiplayerPortabilityBoundaryStatus::from_game(self).status);
        lines.push(if self.online_last_ownership_status.is_empty() {
            OnlineOwnershipStatus::from_game(self).status_line()
        } else {
            self.online_last_ownership_status.clone()
        });
        lines.push(format!(
            "Role guidance: {}",
            self.online_role_guidance_line()
        ));
        lines.push(format!(
            "Gameplay start gate: ready={} blocker={:?}",
            if self.online_gameplay_start_gate().ready {
                "yes"
            } else {
                "no"
            },
            self.online_gameplay_start_gate().blocker
        ));
        lines.push(format!(
            "Save/exit policy: {}",
            self.online_save_exit_policy().status_line()
        ));
        lines.push(format!(
            "Descriptor file: {} | inspect before join: yes | share after host publish: yes",
            self.online_descriptor_path.display()
        ));
        lines.push(if self.online_last_descriptor_input_status.is_empty() {
            OnlineDescriptorInputStatus::validate(
                OnlineDescriptorInputMode::HostWrite,
                &self.online_descriptor_path,
            )
            .status_line()
        } else {
            self.online_last_descriptor_input_status.clone()
        });
        lines.push(self.online_address_guidance_line());
        lines.push(self.online_session_status_message.clone());
        lines.push(format!(
            "Online diagnostics: controller={}, last tick={}",
            if self.online_diagnostic_controller_mode.is_empty() {
                "none"
            } else {
                self.online_diagnostic_controller_mode.as_str()
            },
            if self.online_diagnostic_last_tick.is_empty() {
                "none"
            } else {
                self.online_diagnostic_last_tick.as_str()
            }
        ));
        lines.push(format!(
            "Online replication: {}",
            if self.online_last_replication_status.is_empty() {
                "none"
            } else {
                self.online_last_replication_status.as_str()
            }
        ));
        lines.push(format!(
            "Online replicated player: {}",
            if self.online_last_replicated_player_status.is_empty() {
                "none"
            } else {
                self.online_last_replicated_player_status.as_str()
            }
        ));
        lines.push(format!(
            "Online sync loop: {}",
            if self.online_last_sync_loop_status.is_empty() {
                "none yet; snapshot, delta, terrain, and cargo updates report here"
            } else {
                self.online_last_sync_loop_status.as_str()
            }
        ));
        lines.push(format!(
            "Online terrain sync: {}",
            if self.online_last_terrain_status.is_empty() {
                "none"
            } else {
                self.online_last_terrain_status.as_str()
            }
        ));
        lines.push(format!(
            "Online authority: {}",
            if self.online_last_authority_status.is_empty() {
                "host authoritative simulation; joined clients reconcile to host snapshots"
            } else {
                self.online_last_authority_status.as_str()
            }
        ));
        lines.push(format!(
            "Online correction feel: {}",
            if self.online_last_correction_status.is_empty() {
                "none yet; small offsets smooth, large offsets snap to prevent desync"
            } else {
                self.online_last_correction_status.as_str()
            }
        ));
        lines.push(format!(
            "Online session boundary: {}",
            if self.online_last_session_boundary_status.is_empty() {
                "none; active sessions will report host-ended, client-left, transport-closed, and shutdown acknowledgements here"
            } else {
                self.online_last_session_boundary_status.as_str()
            }
        ));
        lines.push(format!(
            "Online failure help: {}",
            if self.online_last_failure_status.is_empty() {
                "none; task failures will report category, player message, and troubleshooting hint here"
            } else {
                self.online_last_failure_status.as_str()
            }
        ));
        lines.push(if self.online_last_live_verification_status.is_empty() {
            OnlineLiveVerificationStatus::from_game(self).status
        } else {
            self.online_last_live_verification_status.clone()
        });
        lines.push(if self.online_last_gameplay_domain_status.is_empty() {
            OnlineGameplayDomainStatus::from_game(self).status
        } else {
            self.online_last_gameplay_domain_status.clone()
        });
        lines.push(
            if self.online_last_gameplay_sync_evidence_status.is_empty() {
                OnlineGameplaySyncEvidenceMatrix::from_game(self).status
            } else {
                self.online_last_gameplay_sync_evidence_status.clone()
            },
        );
        lines.push(if self.online_last_playable_session_status.is_empty() {
            OnlinePlayableSessionStatus::from_game(self).status
        } else {
            self.online_last_playable_session_status.clone()
        });
        lines.push(if self.online_last_gameplay_entry_status.is_empty() {
            OnlineGameplayEntryPresentation::from_game(self).status
        } else {
            self.online_last_gameplay_entry_status.clone()
        });
        lines.push(OnlineDirectionalGameplaySyncStatus::from_game(self).status);
        lines.push(OnlineGameplayClarityPresentation::from_game(self).status);
        lines.push(OnlineLeaveEndSafetyStatus::from_game(self).status);
        lines.push(OnlineSustainedMiningSessionStatus::from_game(self).status);
        lines.push(OnlineManualWorkingGameGateStatus::from_game(self).status);
        lines.push(OnlineReconnectPolicyStatus::from_game(self).status_line());
        lines.push(OnlineReconnectAttemptDecision::from_game(self).status);
        lines.push(OnlineLanVpnQaReadinessStatus::from_game(self).status);
        lines.push(OnlineSoakReadinessStatus::from_game(self).status);
        lines.push(format!(
            "Online inventory/upgrades: rig parts={} equipped={} cosmetics={} badges={}",
            self.rig_part_inventory.len(),
            self.equipped_rig_parts.len(),
            self.cosmetic_skins.len(),
            self.challenge_badges.len()
        ));
        lines.push(format!(
            "Online menu boundary: run mode={:?} modal={}",
            self.run_mode,
            self.modal
                .as_ref()
                .map_or_else(|| "none".to_owned(), |modal| format!("{modal:?}"))
        ));
        lines.push(OnlineSaveBoundaryStatus::from_game(self).status_line());
        let legacy_summary = crate::session::legacy_game_state_coupling_inventory_summary();
        let rewrite = crate::session::LegacyInputRewriteRemovalStatus::current();
        lines.push(format!(
            "Legacy architecture inventory: total={} authoritative_world_couplings={} presentation_compatibility={} save_menu_ui={} runtime_inventory_complete={} legacy_input_rewrite_removed={}",
            legacy_summary.total,
            legacy_summary.authoritative_world_couplings,
            legacy_summary.presentation_compatibility_couplings,
            legacy_summary.save_menu_ui_couplings,
            yes_no(legacy_summary.runtime_inventory_complete()),
            yes_no(rewrite.removal_complete())
        ));
        let command_routing = crate::session::host_authority_command_routing_summary();
        lines.push(format!(
            "Host authority command routing: total={} host_authoritative={} economy_service_menu_authoritative={} presentation_only={} economy_service_menu_routed={}",
            command_routing.total,
            command_routing.host_authoritative,
            command_routing.economy_service_menu_authoritative,
            command_routing.presentation_only,
            yes_no(command_routing.economy_service_menu_routed())
        ));
        let authority_boundary =
            crate::session::JoinedClientAuthorityBoundaryStatus::online_joined_client_runtime();
        lines.push(format!(
            "Joined-client authority boundary: local_prediction_allowed={} remote_authority_accepted={} host_world_owner={} fights_local_authority={} safe={}",
            yes_no(authority_boundary.local_prediction_allowed),
            yes_no(authority_boundary.remote_authority_accepted),
            yes_no(authority_boundary.host_world_owner),
            yes_no(authority_boundary.fights_local_authority),
            yes_no(authority_boundary.safe_for_joined_client())
        ));
        let rendering_migration = crate::session::RenderingInputMigrationStatus::current();
        let write_boundary = crate::session::LegacyGameStateWriteBoundaryStatus::current();
        lines.push(format!(
            "Rendering input migration: camera={} players={} terrain={} hud={} ready_for_renderer_views={}",
            yes_no(rendering_migration.camera_from_session_view),
            yes_no(rendering_migration.players_from_world_presentation),
            yes_no(rendering_migration.terrain_from_world_presentation),
            yes_no(rendering_migration.hud_from_per_client_presentation),
            yes_no(rendering_migration.migrated())
        ));
        lines.push(format!(
            "Legacy GameState write boundary: ui_settings_save_menu={} presentation_compatibility={} authoritative_world_writes_blocked={} online_save_boundary_enforced={} limited_to_compatibility={}",
            yes_no(write_boundary.ui_settings_save_menu_allowed),
            yes_no(write_boundary.presentation_compatibility_allowed),
            yes_no(write_boundary.authoritative_world_writes_blocked),
            yes_no(write_boundary.online_save_boundary_enforced),
            yes_no(write_boundary.limited_to_compatibility())
        ));
        if self.online_descriptor_path_editing {
            lines.push(format!(
                "Descriptor path edit: {}_",
                self.online_descriptor_path_draft
            ));
        }
        if let Some(target) = self.online_address_edit_target {
            lines.push(format!(
                "{} address edit: {}_",
                Self::online_address_edit_target_label(target),
                self.online_address_edit_draft
            ));
        }
        lines.push(self.online_start_readiness_line());
        lines.push(self.online_save_policy_line());
        lines.push(if self.online_last_lobby_status.is_empty() {
            OnlineLobbyStatus::from_game(self).status
        } else {
            self.online_last_lobby_status.clone()
        });
        lines.extend(self.online_lobby_participant_lines());
        lines.extend(self.online_direct_connect_setup_lines());
        lines.extend(Self::online_session_limitations());
        OnlineRuntimeStatusDeck::from_lines(lines).lines
    }

    #[must_use]
    pub fn online_multiplayer_status_lines(&self) -> Vec<String> {
        let mut lines = self.online_multiplayer_status_lines_without_production_ui_gate();
        lines.push(ProductionOnlineUiRuntimeStatus::from_game(self).status);
        lines.push(OnlineReadyStartTransitionStatus::from_game(self).status);
        OnlineRuntimeStatusDeck::from_lines(lines).lines
    }

    #[must_use]
    pub const fn online_role_guidance_line(&self) -> &'static str {
        if self.online_host_owns_save {
            "You are the authoritative host: keep this app running, share the descriptor, and write saves from here."
        } else if self.online_player_slot.is_some() {
            "You are a joined client: play through the host, do not write local saves, and reconnect with host approval."
        } else {
            "Choose Host to own the session/save or Join to connect with a descriptor from the host."
        }
    }

    #[must_use]
    pub fn online_start_readiness_line(&self) -> String {
        let start_state = if self.online_session_state != OnlineSessionUxState::Connected {
            "blocked: connect host and client"
        } else if !self.online_remote_player_connected {
            "blocked: waiting for remote connection"
        } else if !self.online_local_ready {
            "blocked: local player not ready"
        } else if !self.online_remote_player_ready {
            "blocked: remote player not ready"
        } else if !self.online_host_owns_save {
            "blocked: host must start gameplay"
        } else {
            "ready: authoritative host can start gameplay"
        };
        format!(
            "Start readiness: {start_state} | local_ready={} remote_ready={} remote_connected={}",
            if self.online_local_ready { "yes" } else { "no" },
            if self.online_remote_player_ready {
                "yes"
            } else {
                "no"
            },
            if self.online_remote_player_connected {
                "yes"
            } else {
                "no"
            }
        )
    }

    #[must_use]
    pub fn online_save_policy_line(&self) -> String {
        format!(
            "Save policy: {}",
            self.online_save_exit_policy().status_line()
        )
    }

    #[must_use]
    pub fn online_lobby_presentation(&self) -> OnlineLobbyPresentation {
        let local_slot = self.online_player_slot;
        let remote_slot = match self.online_player_slot {
            Some(1) => Some(2),
            Some(2) => Some(1),
            _ => None,
        };
        let local_save_authority = if self.online_host_owns_save {
            OnlineSaveAuthority::LocalPlayer
        } else {
            OnlineSaveAuthority::RemoteHost
        };
        let remote_save_authority = if self.online_host_owns_save {
            OnlineSaveAuthority::RemoteHost
        } else {
            OnlineSaveAuthority::LocalPlayer
        };
        let remote_role_label = if self.online_host_owns_save {
            "client"
        } else {
            "host"
        };
        OnlineLobbyPresentation {
            local: OnlinePeerLobbyPresentation {
                name: self.online_player_name.clone(),
                slot: local_slot,
                role_label: self.online_role_label(),
                ready: self.online_local_ready,
                connected: matches!(
                    self.online_session_state,
                    OnlineSessionUxState::Connected | OnlineSessionUxState::Hosting
                ),
                save_authority: local_save_authority,
            },
            remote: OnlinePeerLobbyPresentation {
                name: self
                    .online_remote_player_name
                    .clone()
                    .unwrap_or_else(|| "Waiting for player".to_owned()),
                slot: remote_slot,
                role_label: remote_role_label,
                ready: self.online_remote_player_ready,
                connected: self.online_remote_player_connected,
                save_authority: remote_save_authority,
            },
            start_gate: self.online_gameplay_start_gate(),
            guidance: if self.online_host_owns_save {
                "Lobby guidance: host owns save/session authority; wait for the client, verify readiness, then start online gameplay."
                    .to_owned()
            } else {
                "Lobby guidance: joined client uses the host save/session; toggle ready and wait for the host to start online gameplay."
                    .to_owned()
            },
        }
    }

    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "LAN diagnostics intentionally enumerate specific detected failure states"
    )]
    pub fn online_lan_troubleshooting_lines(&self) -> Vec<String> {
        let lan_ip = crate::lan_discovery::likely_lan_ip();
        let pending_task = self
            .online_network_task_request
            .as_ref()
            .map_or_else(|| "none".to_owned(), |task| format!("{task:?}"));
        let mut lines = vec![
            format!("Status: {}", self.online_session_status_message),
            if self.online_last_failure_status.is_empty() {
                "Last failure: none".to_owned()
            } else {
                format!("Last failure: {}", self.online_last_failure_status)
            },
            format!(
                "State: {:?} | pending task: {}",
                self.online_session_state, pending_task
            ),
            format!(
                "Detected LAN IP: {}",
                lan_ip.map_or_else(|| "not detected".to_owned(), |ip| ip.to_string())
            ),
            format!(
                "Game UDP: bind {} | advertise {}",
                self.online_host_bind_addr, self.online_host_advertise_addr
            ),
            format!(
                "Discovery: mDNS service {} over UDP {}",
                crate::lan_discovery::SERVICE_TYPE,
                crate::lan_discovery::MDNS_PORT
            ),
        ];

        let mut issues = Vec::new();
        if lan_ip.is_none() {
            issues.push(
                "Detected issue: no LAN IP could be determined from the default route. The host may be offline, VPN-only, or on an interface the game is not selecting."
                    .to_owned(),
            );
        }
        if self.online_host_bind_addr.ip().is_loopback() {
            issues.push(format!(
                "Detected issue: host bind address {} is loopback-only, so other computers cannot connect.",
                self.online_host_bind_addr
            ));
        }
        if matches!(self.online_session_state, OnlineSessionUxState::Hosting)
            && self.online_host_advertise_addr.ip().is_loopback()
            && lan_ip.is_none()
        {
            issues.push(format!(
                "Detected issue: advertised host address {} is loopback and no replacement LAN IP was found.",
                self.online_host_advertise_addr
            ));
        }
        if matches!(self.online_session_state, OnlineSessionUxState::Hosting)
            && self.online_network_task_request == Some(OnlineNetworkTaskRequest::HostLanGame)
        {
            issues.push(
                "In progress: LAN host request is queued but the networking task has not reported ready yet."
                    .to_owned(),
            );
        }
        if matches!(self.online_session_state, OnlineSessionUxState::Joining)
            && self.online_network_task_request == Some(OnlineNetworkTaskRequest::JoinLanGame)
        {
            issues.push(
                "In progress: LAN scan request is queued but has not completed yet.".to_owned(),
            );
        }
        if matches!(self.online_session_state, OnlineSessionUxState::Error) {
            issues.push(format!(
                "Detected issue: last LAN task failed: {}",
                self.online_session_status_message
            ));
        }
        if self
            .online_session_status_message
            .contains("No usable Drillgame LAN hosts found")
        {
            issues.push(self.online_session_status_message.clone());
        } else if self
            .online_session_status_message
            .contains("No Drillgame LAN hosts found")
        {
            issues.push(
                "Detected issue: LAN scan completed and found zero hosts. This points to host not publishing, mDNS/multicast isolation, or app/network permission blocking discovery."
                    .to_owned(),
            );
        }
        if self
            .online_session_status_message
            .contains("LAN descriptor fetch failed")
        {
            issues.push(
                "Detected issue: discovery found a host but the descriptor TCP endpoint was unreachable. This points to firewall or advertised address/port problems."
                    .to_owned(),
            );
        }
        if self
            .online_session_status_message
            .contains("LAN mDNS publish failed")
        {
            issues.push(
                "Detected issue: the host could not publish its mDNS service. Discovery will not work from another computer."
                    .to_owned(),
            );
        }

        lines.extend(issues);
        lines
    }

    #[must_use]
    pub fn online_descriptor_inspection_presentation(
        &self,
    ) -> OnlineDescriptorInspectionPresentation {
        if self.online_descriptor_inspection_status.is_empty() {
            return OnlineDescriptorInspectionPresentation::pending(&self.online_descriptor_path);
        }
        let severity = match self.online_descriptor_inspection_severity.as_deref() {
            Some("valid") => OnlineDescriptorInspectionSeverity::Valid,
            Some("warning") => OnlineDescriptorInspectionSeverity::Warning,
            Some("error") => OnlineDescriptorInspectionSeverity::Error,
            _ => OnlineDescriptorInspectionSeverity::Pending,
        };
        OnlineDescriptorInspectionPresentation {
            severity,
            heading: match severity {
                OnlineDescriptorInspectionSeverity::Pending => "Descriptor inspection".to_owned(),
                OnlineDescriptorInspectionSeverity::Valid => "Descriptor OK".to_owned(),
                OnlineDescriptorInspectionSeverity::Warning => "Descriptor pending".to_owned(),
                OnlineDescriptorInspectionSeverity::Error => "Descriptor problem".to_owned(),
            },
            lines: if self.online_descriptor_inspection_lines.is_empty() {
                vec![self.online_descriptor_inspection_status.clone()]
            } else {
                self.online_descriptor_inspection_lines.clone()
            },
            can_join: self.online_descriptor_inspection_can_join,
        }
    }

    fn apply_online_descriptor_inspection(
        &mut self,
        presentation: OnlineDescriptorInspectionPresentation,
    ) {
        self.online_descriptor_inspection_status = presentation.status_line();
        self.online_descriptor_inspection_can_join = presentation.can_join;
        self.online_descriptor_inspection_lines = presentation.lines;
        self.online_descriptor_inspection_severity = Some(
            match presentation.severity {
                OnlineDescriptorInspectionSeverity::Pending => "pending",
                OnlineDescriptorInspectionSeverity::Valid => "valid",
                OnlineDescriptorInspectionSeverity::Warning => "warning",
                OnlineDescriptorInspectionSeverity::Error => "error",
            }
            .to_owned(),
        );
        self.online_session_status_message = self.online_descriptor_inspection_status.clone();
    }

    #[must_use]
    pub fn online_session_lifecycle_presentation(&self) -> OnlineSessionLifecyclePresentation {
        if !self.online_session_active() && self.online_session_boundary_history.is_empty() {
            return OnlineSessionLifecyclePresentation::inactive();
        }
        let active = self.online_session_active();
        let heading = if active {
            format!("Lifecycle: {:?}", self.online_session_state)
        } else {
            "Lifecycle: last online session".to_owned()
        };
        let safe_exit_line = if self.online_network_task_request
            == Some(OnlineNetworkTaskRequest::Shutdown)
        {
            "Shutdown is queued; peer notification/transport close will be drained by the app loop."
                .to_owned()
        } else if active {
            "Use Shutdown session or Exit to request a graceful peer notification before closing."
                .to_owned()
        } else if self.online_host_owns_save {
            "Host save authority is local; normal save/exit is safe after shutdown completes."
                .to_owned()
        } else {
            "Joined-client local saves remain blocked until the online session is fully closed."
                .to_owned()
        };
        let remote_line = format!(
            "Remote connected={} ready={} | local ready={} | save authority={}",
            yes_no(self.online_remote_player_connected),
            yes_no(self.online_remote_player_ready),
            yes_no(self.online_local_ready),
            if self.online_host_owns_save {
                "local host"
            } else {
                "remote host"
            }
        );
        let boundary_lines = if self.online_session_boundary_history.is_empty() {
            if self.online_last_session_boundary_status.is_empty() {
                vec!["No leave/end boundary event recorded yet.".to_owned()]
            } else {
                vec![self.online_last_session_boundary_status.clone()]
            }
        } else {
            self.online_session_boundary_history
                .iter()
                .rev()
                .take(3)
                .cloned()
                .collect()
        };
        let mut boundary_lines = boundary_lines;
        if !self.online_last_shutdown_summary.is_empty() {
            boundary_lines.insert(0, self.online_last_shutdown_summary.clone());
            boundary_lines.truncate(4);
        }
        OnlineSessionLifecyclePresentation {
            active,
            heading,
            safe_exit_line,
            remote_line,
            boundary_lines,
        }
    }

    #[must_use]
    pub fn online_lobby_participant_lines(&self) -> Vec<String> {
        self.online_lobby_presentation().lines()
    }

    #[must_use]
    pub const fn online_session_active(&self) -> bool {
        matches!(
            self.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
                | OnlineSessionUxState::Timeout
                | OnlineSessionUxState::Error
        )
    }

    #[must_use]
    pub fn online_pause_session_presentation(&self) -> OnlinePauseSessionPresentation {
        if !self.online_session_active() {
            return OnlinePauseSessionPresentation::empty();
        }
        let save_policy = self.online_save_exit_policy();
        let remote_name = self
            .online_remote_player_name
            .clone()
            .unwrap_or_else(|| "waiting for remote player".to_owned());
        let slot = self
            .online_player_slot
            .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string());
        let heading = format!("Online {} session", self.online_role_label());
        let primary_action = if matches!(self.online_session_state, OnlineSessionUxState::Connected)
        {
            "Choose Online Session to manage ready/start/reconnect/shutdown without force-kill."
                .to_owned()
        } else {
            "Choose Online Session to inspect, reconnect, or safely shut down this session."
                .to_owned()
        };
        let save_warning = (!save_policy.local_save_allowed).then(|| {
            "Host owns this online save: local save/load are blocked on this joined client."
                .to_owned()
        });
        let mut lines = vec![
            format!(
                "State: {:?} | slot {} | {}",
                self.online_session_state,
                slot,
                if self.online_host_owns_save {
                    "host owns save"
                } else {
                    "remote host owns save"
                }
            ),
            format!(
                "Remote: {remote_name} | connected={} | ready={}",
                yes_no(self.online_remote_player_connected),
                yes_no(self.online_remote_player_ready)
            ),
            format!(
                "Local ready={} | save/load allowed={}",
                yes_no(self.online_local_ready),
                yes_no(save_policy.local_save_allowed)
            ),
        ];
        if !self.online_last_replicated_player_status.is_empty() {
            lines.push(format!(
                "Player sync: {}",
                self.online_last_replicated_player_status
            ));
        }
        if !self.online_last_terrain_status.is_empty() {
            lines.push(format!("Terrain sync: {}", self.online_last_terrain_status));
        }
        if !self.online_last_session_boundary_status.is_empty() {
            lines.push(format!(
                "Boundary: {}",
                self.online_last_session_boundary_status
            ));
        }
        if !self.online_last_shutdown_summary.is_empty() {
            lines.push(format!("Shutdown: {}", self.online_last_shutdown_summary));
        }
        OnlinePauseSessionPresentation {
            active: true,
            heading,
            lines,
            primary_action,
            save_warning,
        }
    }

    #[must_use]
    pub fn online_gameplay_hud_presentation(&self) -> OnlineGameplayHudPresentation {
        let visible = matches!(
            self.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
                | OnlineSessionUxState::Timeout
                | OnlineSessionUxState::Error
                | OnlineSessionUxState::Shutdown
        );
        let remote_label = self
            .online_remote_player_name
            .clone()
            .unwrap_or_else(|| "remote player".to_owned());
        let slot_label = self
            .online_player_slot
            .map_or_else(|| "unassigned".to_owned(), |slot| slot.to_string());
        let save_policy = self.online_save_exit_policy();
        let save_policy_label = if save_policy.local_save_allowed {
            "Save authority: local writes allowed for this host-owned session.".to_owned()
        } else {
            "Save authority: remote host owns save; local save/load blocked while joined."
                .to_owned()
        };
        let session_label = format!(
            "Session: {:?} | {}",
            self.online_session_state,
            if self.online_remote_player_connected {
                "remote connected"
            } else {
                "waiting for remote"
            }
        );
        let gameplay_entry_label = OnlineGameplayEntryPresentation::from_game(self).hud_line();
        let directional_sync_label =
            OnlineDirectionalGameplaySyncStatus::from_game(self).hud_line();
        let replication_label = if self.online_last_replicated_player_status.is_empty() {
            "Player sync: waiting for replicated player state.".to_owned()
        } else {
            format!("Player sync: {}", self.online_last_replicated_player_status)
        };
        let terrain_label = if self.online_last_terrain_status.is_empty() {
            if self.online_terrain_sync_markers.is_empty() {
                "Terrain sync: waiting for terrain chunk updates.".to_owned()
            } else {
                format!(
                    "Terrain sync: {} replicated tile highlight(s) active.",
                    self.online_terrain_sync_markers.len()
                )
            }
        } else if self.online_terrain_sync_markers.is_empty() {
            format!("Terrain sync: {}", self.online_last_terrain_status)
        } else {
            format!(
                "Terrain sync: {}; {} highlight(s) active",
                self.online_last_terrain_status,
                self.online_terrain_sync_markers.len()
            )
        };
        let authority_label = if self.online_last_correction_status.is_empty() {
            "Authority: host simulation owns corrections.".to_owned()
        } else {
            self.online_last_correction_status.clone()
        };
        let clarity_lines = OnlineGameplayClarityPresentation::from_game(self).hud_lines();
        OnlineGameplayHudPresentation {
            visible,
            role_label: self.online_role_label(),
            slot_label,
            local_ready_label: if self.online_local_ready {
                "ready"
            } else {
                "not-ready"
            },
            remote_label,
            remote_ready_label: if self.online_remote_player_ready {
                "ready"
            } else {
                "not-ready"
            },
            save_policy_label,
            session_label,
            gameplay_entry_label,
            directional_sync_label,
            replication_label,
            terrain_label,
            authority_label,
            clarity_lines,
        }
    }

    #[must_use]
    pub const fn online_role_label(&self) -> &'static str {
        if self.online_host_owns_save {
            "host"
        } else if self.online_player_slot.is_some() {
            "client"
        } else {
            "unassigned"
        }
    }

    #[must_use]
    pub fn online_direct_connect_setup_lines(&self) -> Vec<String> {
        let descriptor = self.online_descriptor_path.display();
        vec![
            format!(
                "Direct-connect config: descriptor `{descriptor}`, host bind {}, advertise {}, client bind {}, ticks {}.",
                self.online_host_bind_addr,
                self.online_host_advertise_addr,
                self.online_client_bind_addr,
                self.online_gameplay_ticks
            ),
            "Host flow: press Enter on descriptor path to type/edit it, then share the generated descriptor with the joining player after hosting starts.".to_owned(),
            format!(
                "Host CLI helper: drillgame --online-host-gameplay-descriptor-file-on-addr {descriptor} {} {} {}",
                self.online_host_bind_addr,
                self.online_host_advertise_addr,
                self.online_gameplay_ticks
            ),
            format!(
                "Join CLI helper: drillgame --online-join-gameplay-descriptor-file-on-addr {descriptor} {} {}",
                self.online_client_bind_addr, self.online_gameplay_ticks
            ),
            format!(
                "QA helper: drillgame --online-lan-qa-checklist-md {descriptor} {} {} {} {}",
                self.online_host_bind_addr,
                self.online_host_advertise_addr,
                self.online_client_bind_addr,
                self.online_gameplay_ticks
            ),
        ]
    }

    #[must_use]
    pub fn online_session_limitations() -> Vec<String> {
        vec![
            "Real localhost Quinn socket IO is available for host/join/tick/reconnect coverage; direct-address game-window UX remains desktop-first.".to_owned(),
            "NAT traversal, matchmaking, invites, and host migration are intentionally unsupported.".to_owned(),
        ]
    }

    fn online_gameplay_start_gate(&self) -> OnlineGameplayStartGate {
        if self.online_session_state != OnlineSessionUxState::Connected {
            OnlineGameplayStartGate::blocked(
                OnlineGameplayStartBlocker::NotConnected,
                "Start blocked: connect host and client before entering online gameplay.",
            )
        } else if !self.online_local_ready {
            OnlineGameplayStartGate::blocked(
                OnlineGameplayStartBlocker::LocalNotReady,
                "Start blocked: toggle local ready before entering online gameplay.",
            )
        } else if !self.online_remote_player_connected {
            OnlineGameplayStartGate::blocked(
                OnlineGameplayStartBlocker::RemoteNotConnected,
                "Start blocked: waiting for the remote player connection.",
            )
        } else if !self.online_remote_player_ready {
            OnlineGameplayStartGate::blocked(
                OnlineGameplayStartBlocker::RemoteNotReady,
                "Start blocked: waiting for the remote player to toggle ready.",
            )
        } else if !self.online_host_owns_save {
            OnlineGameplayStartGate::blocked(
                OnlineGameplayStartBlocker::HostAuthorityRequired,
                "Start blocked: only the authoritative host can start online gameplay; wait for the host start signal.",
            )
        } else {
            OnlineGameplayStartGate::ready()
        }
    }

    pub(crate) fn request_online_gameplay_start(&mut self) {
        let gate = self.online_gameplay_start_gate();
        gate.message
            .clone_into(&mut self.online_session_status_message);
        if gate.ready {
            self.online_start_session_requested = true;
            self.online_gameplay_entry_source = OnlineGameplayEntrySource::HostUiRequested;
            self.online_gameplay_entry_authoritative_tick = None;
            self.refresh_online_gameplay_entry_status();
            self.enter_online_playing_session();
        } else if gate.blocker == Some(OnlineGameplayStartBlocker::HostAuthorityRequired) {
            self.online_start_session_requested = false;
            self.refresh_online_lobby_status();
        }
        self.refresh_online_playable_session_status();
    }

    pub fn apply_online_start_session_from_host(
        &mut self,
        authoritative_tick: crate::multiplayer::SimulationTick,
    ) {
        self.online_session_state = OnlineSessionUxState::Connected;
        self.online_remote_player_connected = true;
        self.online_session_status_message = format!(
            "Host started online gameplay at authoritative tick {}.",
            authoritative_tick.get()
        );
        self.online_gameplay_entry_source = OnlineGameplayEntrySource::HostStartReceived;
        self.online_gameplay_entry_authoritative_tick = Some(authoritative_tick);
        self.enter_online_playing_session();
        self.refresh_online_gameplay_entry_status();
        self.refresh_online_playable_session_status();
    }

    fn close_online_multiplayer_menu(&mut self) {
        if self.online_network_task_request.is_some() {
            self.online_network_task_request = None;
            self.online_local_ready = false;
            self.online_remote_player_ready = false;
            self.online_remote_player_connected = false;
            self.online_session_state = OnlineSessionUxState::Idle;
            self.modal = None;
            "Closed online multiplayer menu; pending network task canceled."
                .clone_into(&mut self.online_session_status_message);
            self.message.clone_from(&self.online_session_status_message);
            return;
        }

        if matches!(
            self.online_session_state,
            OnlineSessionUxState::Hosting
                | OnlineSessionUxState::Joining
                | OnlineSessionUxState::Connected
                | OnlineSessionUxState::Reconnecting
        ) {
            let _requested = self.request_online_shutdown_from_gameplay_exit();
            self.modal = None;
            return;
        }

        self.online_network_task_request = None;
        self.online_local_ready = false;
        self.online_remote_player_ready = false;
        self.online_remote_player_connected = false;
        self.online_session_state = match self.online_session_state {
            OnlineSessionUxState::Shutdown => OnlineSessionUxState::Shutdown,
            _ => OnlineSessionUxState::Idle,
        };
        self.modal = None;
        "Closed online multiplayer menu; no network task queued."
            .clone_into(&mut self.online_session_status_message);
        self.message.clone_from(&self.online_session_status_message);
    }

    fn cycle_online_descriptor_path(&mut self) {
        self.online_descriptor_path =
            if self.online_descriptor_path == default_online_descriptor_path() {
                join_online_descriptor_path()
            } else {
                default_online_descriptor_path()
            };
        self.online_session_status_message = format!(
            "Descriptor path selected: {}",
            self.online_descriptor_path.display()
        );
    }

    fn inspect_online_descriptor_path(&mut self) {
        let input_status = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::JoinRead,
            &self.online_descriptor_path,
        );
        if !input_status.can_attempt_task {
            self.apply_online_descriptor_inspection(OnlineDescriptorInspectionPresentation::error(
                &self.online_descriptor_path,
                input_status.message,
            ));
            self.online_session_state = OnlineSessionUxState::Error;
            return;
        }
        if !input_status.accepted {
            self.apply_online_descriptor_inspection(
                OnlineDescriptorInspectionPresentation::warning(
                    &self.online_descriptor_path,
                    input_status.message,
                ),
            );
            return;
        }
        match std::fs::read_to_string(&self.online_descriptor_path)
            .map_err(|error| format!("descriptor read failed: {error}"))
            .and_then(|contents| {
                serde_json::from_str::<crate::multiplayer::QuinnHostConnectionDescriptor>(&contents)
                    .map_err(|error| format!("descriptor parse failed: {error}"))
            }) {
            Ok(descriptor) => {
                self.apply_online_descriptor_inspection(
                    OnlineDescriptorInspectionPresentation::valid(
                        &self.online_descriptor_path,
                        &descriptor,
                    ),
                );
            }
            Err(error) => {
                self.online_session_state = OnlineSessionUxState::Error;
                self.apply_online_descriptor_inspection(
                    OnlineDescriptorInspectionPresentation::error(
                        &self.online_descriptor_path,
                        format!("Descriptor inspect failed: {error}"),
                    ),
                );
            }
        }
    }

    fn cycle_online_host_address_preset(&mut self) {
        if self.online_host_bind_addr == default_online_host_bind_addr()
            && self.online_host_advertise_addr == default_online_host_advertise_addr()
        {
            self.online_host_bind_addr = lan_online_host_bind_addr();
            self.online_host_advertise_addr = lan_online_host_advertise_addr();
        } else if self.online_host_bind_addr == lan_online_host_bind_addr()
            && self.online_host_advertise_addr == lan_online_host_advertise_addr()
        {
            self.online_host_bind_addr = localhost_ephemeral_online_host_bind_addr();
            self.online_host_advertise_addr = localhost_ephemeral_online_host_advertise_addr();
        } else {
            self.online_host_bind_addr = default_online_host_bind_addr();
            self.online_host_advertise_addr = default_online_host_advertise_addr();
        }
        self.online_session_status_message = format!(
            "Host address preset selected: bind {}, advertise {}. Share the advertised address/descriptor with joiners; use 0.0.0.0 bind for LAN/VPN hosting.",
            self.online_host_bind_addr, self.online_host_advertise_addr
        );
    }

    fn cycle_online_gameplay_ticks(&mut self) {
        self.online_gameplay_ticks = match self.online_gameplay_ticks {
            0..=60 => 120,
            61..=120 => 300,
            _ => 60,
        };
        self.online_session_status_message = format!(
            "Gameplay smoke tick count selected: {} ticks",
            self.online_gameplay_ticks
        );
    }

    fn start_online_descriptor_path_edit(&mut self) {
        self.online_descriptor_path_editing = true;
        self.online_descriptor_path_draft = self.online_descriptor_path.display().to_string();
        "Editing descriptor path: type path, Backspace deletes, Enter accepts, Esc cancels."
            .clone_into(&mut self.online_session_status_message);
    }

    fn commit_online_descriptor_path_edit(&mut self) {
        let trimmed = self.online_descriptor_path_draft.trim();
        if trimmed.is_empty() {
            "Descriptor path cannot be empty; edit canceled."
                .clone_into(&mut self.online_session_status_message);
        } else {
            self.online_descriptor_path = PathBuf::from(trimmed);
            self.online_session_status_message = format!(
                "Descriptor path set from typed input: {}",
                self.online_descriptor_path.display()
            );
        }
        self.online_descriptor_path_editing = false;
    }

    fn cancel_online_descriptor_path_edit(&mut self) {
        self.online_descriptor_path_editing = false;
        self.online_descriptor_path_draft.clear();
        "Descriptor path edit canceled.".clone_into(&mut self.online_session_status_message);
    }

    fn handle_online_descriptor_path_text_input(&mut self, input: PlayerInput) -> bool {
        if !self.online_descriptor_path_editing {
            return false;
        }
        if input.cancel {
            self.cancel_online_descriptor_path_edit();
        } else if input.confirm {
            self.commit_online_descriptor_path_edit();
        } else if input.text_backspace {
            self.online_descriptor_path_draft.pop();
            self.online_session_status_message = format!(
                "Editing descriptor path: {}",
                self.online_descriptor_path_draft
            );
        } else if let Some(character) = input.text_input
            && is_allowed_descriptor_path_character(character)
            && self.online_descriptor_path_draft.len() < 240
        {
            self.online_descriptor_path_draft.push(character);
            self.online_session_status_message = format!(
                "Editing descriptor path: {}",
                self.online_descriptor_path_draft
            );
        } else {
            return false;
        }
        self.message = self.online_session_status_message.clone();
        self.sound_cues.push(SoundCue::Ui);
        true
    }

    fn start_online_address_edit(&mut self, target: OnlineAddressEditTarget) {
        self.online_address_edit_target = Some(target);
        self.online_address_edit_draft = match target {
            OnlineAddressEditTarget::HostBind => self.online_host_bind_addr.to_string(),
            OnlineAddressEditTarget::HostAdvertise => self.online_host_advertise_addr.to_string(),
            OnlineAddressEditTarget::ClientBind => self.online_client_bind_addr.to_string(),
        };
        self.online_session_status_message = format!(
            "Editing {} address: type host:port, Backspace deletes, Enter accepts, Esc cancels.",
            Self::online_address_edit_target_label(target)
        );
    }

    const fn online_address_edit_target_label(target: OnlineAddressEditTarget) -> &'static str {
        match target {
            OnlineAddressEditTarget::HostBind => "host bind",
            OnlineAddressEditTarget::HostAdvertise => "host advertise",
            OnlineAddressEditTarget::ClientBind => "client bind",
        }
    }

    fn online_address_guidance_line(&self) -> String {
        format!(
            "Address guidance: host bind listens locally; host advertise is what friends type; client bind is this machine's UDP socket. Current bind {}, advertise {}, client {}.",
            self.online_host_bind_addr,
            self.online_host_advertise_addr,
            self.online_client_bind_addr
        )
    }

    fn validate_online_address_edit(
        target: OnlineAddressEditTarget,
        raw_text: &str,
    ) -> OnlineAddressValidation {
        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return OnlineAddressValidation::error(
                target,
                format!(
                    "Invalid {} address: enter a host:port such as 0.0.0.0:5000 or 192.168.1.10:5000.",
                    Self::online_address_edit_target_label(target)
                ),
            );
        }
        let Ok(address) = trimmed.parse::<SocketAddr>() else {
            return OnlineAddressValidation::error(
                target,
                format!(
                    "Invalid {} address `{trimmed}`: use host:port, for example 0.0.0.0:5000, 127.0.0.1:5000, or [::1]:5000.",
                    Self::online_address_edit_target_label(target)
                ),
            );
        };
        if address.port() == 0
            && !matches!(
                target,
                OnlineAddressEditTarget::HostBind | OnlineAddressEditTarget::ClientBind
            )
        {
            return OnlineAddressValidation::error(
                target,
                format!(
                    "Invalid {} address `{address}`: advertised addresses need a real port, not :0.",
                    Self::online_address_edit_target_label(target)
                ),
            );
        }
        let ip = address.ip();
        if matches!(target, OnlineAddressEditTarget::HostAdvertise) && ip.is_unspecified() {
            return OnlineAddressValidation::warning(
                target,
                address,
                format!(
                    "Host advertise address `{address}` uses a wildcard IP. LAN/VPN joiners usually need this machine's reachable IP and a nonzero port."
                ),
            );
        }
        if matches!(target, OnlineAddressEditTarget::HostAdvertise) && ip.is_loopback() {
            return OnlineAddressValidation::warning(
                target,
                address,
                format!(
                    "Host advertise address `{address}` is loopback; only clients on this same machine can join. Use a LAN/VPN IP for another computer."
                ),
            );
        }
        if matches!(target, OnlineAddressEditTarget::HostBind)
            && !ip.is_unspecified()
            && !ip.is_loopback()
        {
            return OnlineAddressValidation::warning(
                target,
                address,
                format!(
                    "Host bind address `{address}` is specific to one interface. Use 0.0.0.0:{port} to listen on all IPv4 interfaces if LAN players cannot connect.",
                    port = address.port()
                ),
            );
        }
        OnlineAddressValidation::accepted(target, address)
    }

    fn apply_online_address_validation(&mut self, validation: OnlineAddressValidation) {
        if validation.is_accepted() {
            let address = validation.address.expect("accepted address has value");
            match validation.target {
                OnlineAddressEditTarget::HostBind => self.online_host_bind_addr = address,
                OnlineAddressEditTarget::HostAdvertise => self.online_host_advertise_addr = address,
                OnlineAddressEditTarget::ClientBind => self.online_client_bind_addr = address,
            }
        }
        self.online_session_status_message = validation.message;
    }

    fn commit_online_address_edit(&mut self) {
        let Some(target) = self.online_address_edit_target else {
            return;
        };
        let validation =
            Self::validate_online_address_edit(target, &self.online_address_edit_draft);
        self.apply_online_address_validation(validation);
        self.online_address_edit_target = None;
        self.online_address_edit_draft.clear();
    }

    fn cancel_online_address_edit(&mut self) {
        self.online_address_edit_target = None;
        self.online_address_edit_draft.clear();
        "Address edit canceled.".clone_into(&mut self.online_session_status_message);
    }

    fn handle_online_text_input(&mut self, input: PlayerInput) -> bool {
        if self.online_descriptor_path_editing {
            return self.handle_online_descriptor_path_text_input(input);
        }
        if self.online_address_edit_target.is_some() {
            return self.handle_online_address_text_input(input);
        }
        false
    }

    fn handle_online_address_text_input(&mut self, input: PlayerInput) -> bool {
        let Some(target) = self.online_address_edit_target else {
            return false;
        };
        if input.cancel {
            self.cancel_online_address_edit();
        } else if input.confirm {
            self.commit_online_address_edit();
        } else if input.text_backspace {
            self.online_address_edit_draft.pop();
            self.online_session_status_message = format!(
                "Editing {} address: {}",
                Self::online_address_edit_target_label(target),
                self.online_address_edit_draft
            );
        } else if let Some(character) = input.text_input
            && is_allowed_socket_address_character(character)
            && self.online_address_edit_draft.len() < 80
        {
            self.online_address_edit_draft.push(character);
            self.online_session_status_message = format!(
                "Editing {} address: {}",
                Self::online_address_edit_target_label(target),
                self.online_address_edit_draft
            );
        } else if let Some(character) = input.text_input {
            self.online_session_status_message = format!(
                "Ignored invalid {} address character `{character}`. Use digits, '.', ':', '[', and ']'.",
                Self::online_address_edit_target_label(target)
            );
        } else {
            return false;
        }
        self.message = self.online_session_status_message.clone();
        self.sound_cues.push(SoundCue::Ui);
        true
    }

    fn adjust_online_multiplayer_selection(&mut self) {
        match self.selected_menu_item {
            2 => self.cycle_online_descriptor_path(),
            4 => self.cycle_online_host_address_preset(),
            7 => self.cycle_online_gameplay_ticks(),
            _ => {
                "Select descriptor path, host address, or gameplay ticks to adjust with left/right."
                    .clone_into(&mut self.online_session_status_message);
            }
        }
        self.message = self.online_session_status_message.clone();
        self.sound_cues.push(SoundCue::Ui);
    }

    #[allow(
        clippy::too_many_lines,
        reason = "online modal reducer keeps adjacent menu action state transitions together for now"
    )]
    fn confirm_online_multiplayer(&mut self) {
        match self.online_multiplayer_view {
            OnlineMultiplayerView::MainMenu => match self.selected_menu_item {
                0 => self.start_lan_host_flow(),
                1 => self.start_lan_join_flow(),
                2 => {
                    self.online_multiplayer_view = OnlineMultiplayerView::AdvancedDirectConnect;
                    self.selected_menu_item = 0;
                    "Advanced direct-connect tools selected."
                        .clone_into(&mut self.online_session_status_message);
                }
                _ => {
                    self.close_online_multiplayer_menu();
                    self.sound_cues.push(SoundCue::Ui);
                    return;
                }
            },
            OnlineMultiplayerView::HostLan => match self.selected_menu_item {
                0 => {
                    self.online_local_ready = !self.online_local_ready;
                    if self.online_local_ready {
                        "Local host ready; waiting for client readiness."
                    } else {
                        "Local host not ready."
                    }
                    .clone_into(&mut self.online_session_status_message);
                    self.refresh_online_lobby_status();
                }
                1 => self.request_online_gameplay_start(),
                2 => {
                    self.online_session_state = OnlineSessionUxState::Shutdown;
                    self.online_network_task_request = Some(OnlineNetworkTaskRequest::Shutdown);
                    self.online_local_ready = false;
                    self.online_multiplayer_view = OnlineMultiplayerView::MainMenu;
                    self.selected_menu_item = 0;
                    self.apply_online_session_boundary_status(
                        &OnlineSessionBoundaryStatus::local_shutdown_requested(),
                    );
                }
                _ => {
                    self.online_multiplayer_view = OnlineMultiplayerView::MainMenu;
                    self.selected_menu_item = 0;
                    "Returned to Online Multiplayer."
                        .clone_into(&mut self.online_session_status_message);
                }
            },
            OnlineMultiplayerView::JoinLan => match self.selected_menu_item {
                0 => self.start_lan_join_flow(),
                1 => {
                    self.online_local_ready = !self.online_local_ready;
                    if self.online_local_ready {
                        "Local client ready; waiting for host to start."
                    } else {
                        "Local client not ready."
                    }
                    .clone_into(&mut self.online_session_status_message);
                    self.refresh_online_lobby_status();
                }
                2 => {
                    self.online_session_state = OnlineSessionUxState::Shutdown;
                    self.online_network_task_request = Some(OnlineNetworkTaskRequest::Shutdown);
                    self.online_local_ready = false;
                    self.online_multiplayer_view = OnlineMultiplayerView::MainMenu;
                    self.selected_menu_item = 0;
                    self.apply_online_session_boundary_status(
                        &OnlineSessionBoundaryStatus::local_shutdown_requested(),
                    );
                }
                _ => {
                    self.online_multiplayer_view = OnlineMultiplayerView::MainMenu;
                    self.selected_menu_item = 0;
                    "Returned to Online Multiplayer."
                        .clone_into(&mut self.online_session_status_message);
                }
            },
            OnlineMultiplayerView::AdvancedDirectConnect => match self.selected_menu_item {
                0 => {
                    let descriptor_status = OnlineDescriptorInputStatus::validate(
                        OnlineDescriptorInputMode::HostWrite,
                        &self.online_descriptor_path,
                    );
                    if !descriptor_status.can_attempt_task {
                        self.apply_online_descriptor_input_status(&descriptor_status);
                        self.sound_cues.push(SoundCue::Ui);
                        return;
                    }
                    self.apply_online_descriptor_input_status(&descriptor_status);
                    self.online_session_state = OnlineSessionUxState::Hosting;
                    self.online_network_task_request =
                        Some(OnlineNetworkTaskRequest::HostDescriptorFile {
                            path: self.online_descriptor_path.clone(),
                        });
                    self.online_host_owns_save = true;
                    self.online_player_slot = Some(1);
                    self.online_session_status_message = format!(
                        "Hosting direct-connect descriptor at {}.",
                        self.online_descriptor_path.display()
                    );
                }
                1 => {
                    let descriptor_status = OnlineDescriptorInputStatus::validate(
                        OnlineDescriptorInputMode::JoinRead,
                        &self.online_descriptor_path,
                    );
                    if !descriptor_status.can_attempt_task {
                        self.apply_online_descriptor_input_status(&descriptor_status);
                        self.sound_cues.push(SoundCue::Ui);
                        return;
                    }
                    self.apply_online_descriptor_input_status(&descriptor_status);
                    self.online_session_state = OnlineSessionUxState::Joining;
                    self.online_network_task_request =
                        Some(OnlineNetworkTaskRequest::JoinDescriptorFile {
                            path: self.online_descriptor_path.clone(),
                        });
                    self.online_host_owns_save = false;
                    self.online_player_slot = Some(2);
                    self.online_session_status_message = format!(
                        "Joining with descriptor {}.",
                        self.online_descriptor_path.display()
                    );
                }
                2 => self.start_online_descriptor_path_edit(),
                3 => self.inspect_online_descriptor_path(),
                4 => self.start_online_address_edit(OnlineAddressEditTarget::HostBind),
                5 => self.start_online_address_edit(OnlineAddressEditTarget::HostAdvertise),
                6 => self.start_online_address_edit(OnlineAddressEditTarget::ClientBind),
                7 => self.cycle_online_gameplay_ticks(),
                8 => {
                    let decision = OnlineReconnectAttemptDecision::from_game(self);
                    if decision.can_attempt {
                        self.online_session_state = OnlineSessionUxState::Reconnecting;
                        self.online_network_task_request =
                            Some(OnlineNetworkTaskRequest::ReconnectDirectConnect);
                    }
                    decision
                        .player_message
                        .clone_into(&mut self.online_session_status_message);
                }
                _ => {
                    self.online_multiplayer_view = OnlineMultiplayerView::MainMenu;
                    self.selected_menu_item = 0;
                    "Returned to Online Multiplayer."
                        .clone_into(&mut self.online_session_status_message);
                }
            },
        }
        self.message = self.online_session_status_message.clone();
        self.sound_cues.push(SoundCue::Ui);
    }

    fn start_lan_host_flow(&mut self) {
        self.online_multiplayer_view = OnlineMultiplayerView::HostLan;
        self.selected_menu_item = 0;
        self.online_session_state = OnlineSessionUxState::Hosting;
        self.online_network_task_request = Some(OnlineNetworkTaskRequest::HostLanGame);
        self.online_host_owns_save = true;
        self.online_player_slot = Some(1);
        self.online_local_ready = false;
        self.online_remote_player_name = None;
        self.online_remote_player_ready = false;
        self.online_remote_player_connected = false;
        self.online_session_status_message = format!(
            "Starting LAN host as {}; opening server, descriptor endpoint, and mDNS advertisement.",
            crate::lan_discovery::local_machine_name()
        );
    }

    fn start_lan_join_flow(&mut self) {
        self.online_multiplayer_view = OnlineMultiplayerView::JoinLan;
        self.selected_menu_item = 0;
        self.online_session_state = OnlineSessionUxState::Joining;
        self.online_network_task_request = Some(OnlineNetworkTaskRequest::JoinLanGame);
        self.online_host_owns_save = false;
        self.online_player_slot = Some(2);
        self.online_local_ready = false;
        self.online_remote_player_name = Some("Host miner".to_owned());
        self.online_remote_player_ready = false;
        self.online_remote_player_connected = false;
        "Scanning LAN for Drillgame hosts via mDNS."
            .clone_into(&mut self.online_session_status_message);
    }

    fn start_new_game(&mut self) {
        let master_volume = self.master_volume;
        let fullscreen = self.fullscreen;
        *self = Self::new();
        self.master_volume = master_volume;
        self.fullscreen = fullscreen;
        self.run_mode = RunMode::Playing;
        self.save_dirty = true;
        self.sound_cues.push(SoundCue::Milestone);
        "Welcome to the dig site. Visit the depot for contracts.".clone_into(&mut self.message);
    }

    fn apply_local_multiplayer_status(&mut self, status: LocalMultiplayerRuntimeStatus) {
        self.local_multiplayer_requested = status.requested;
        self.local_multiplayer_active = status.active;
        self.local_multiplayer_player_slots = status.player_slots.max(1);
        self.local_multiplayer_status_message = status.player_message;
        self.message = self.local_multiplayer_status_message.clone();
    }

    fn start_local_multiplayer_request(&mut self) {
        self.start_new_game();
        self.apply_local_multiplayer_status(LocalMultiplayerRuntimeStatus {
            phase: LocalMultiplayerRuntimePhase::Requested,
            requested: true,
            active: false,
            player_slots: 2,
            online_isolated: true,
            player_message:
                "Local split-screen starting: Player 1 uses WASD, Player 2 uses arrow keys."
                    .to_owned(),
        });
    }

    pub fn take_local_multiplayer_request(&mut self) -> bool {
        let requested = mem::take(&mut self.local_multiplayer_requested);
        if requested {
            self.local_multiplayer_status_message =
                LocalMultiplayerRuntimeStatus::from_game(self).status_line();
        }
        requested
    }

    pub fn mark_local_multiplayer_active(&mut self, player_slots: u8) {
        self.apply_local_multiplayer_status(LocalMultiplayerRuntimeStatus {
            phase: LocalMultiplayerRuntimePhase::Active,
            requested: false,
            active: true,
            player_slots,
            online_isolated: true,
            player_message: format!(
                "Local split-screen active with {player_slots} players: Player 1 WASD, Player 2 arrow keys."
            ),
        });
    }

    fn load_latest_into_self(&mut self) {
        if self.block_joined_client_load() {
            return;
        }
        match load_latest_game() {
            Ok(mut loaded) => {
                loaded.master_volume = self.master_volume;
                loaded.fullscreen = self.fullscreen;
                loaded.migrate_loaded_state();
                loaded.mark_full_terrain_refresh();
                *self = loaded;
                self.save_dirty = false;
                "Resumed saved session.".clone_into(&mut self.message);
            }
            Err(error) => self.message = format!("Resume failed: {error}"),
        }
    }

    const fn request_exit_or_prompt(&mut self) {
        if self.save_dirty {
            self.modal = Some(ModalScreen::UnsavedExitConfirm);
            self.selected_menu_item = 0;
        } else {
            self.modal = Some(ModalScreen::ExitConfirm);
        }
    }

    fn request_exit_and_online_shutdown(&mut self) {
        let _shutdown_requested = self.request_online_shutdown_from_gameplay_exit();
        self.request_exit = true;
    }

    fn handle_exit_modal(&mut self, input: PlayerInput) -> bool {
        match self.modal {
            Some(ModalScreen::ExitConfirm) => {
                if input.cancel {
                    self.modal = None;
                } else if input.confirm {
                    self.request_exit_and_online_shutdown();
                }
                true
            }
            Some(ModalScreen::UnsavedExitConfirm) => {
                if input.cancel {
                    self.modal = None;
                    return true;
                }
                if input.menu_up {
                    self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
                }
                if input.menu_down {
                    self.selected_menu_item = (self.selected_menu_item + 1).min(2);
                }
                if input.confirm {
                    match self.selected_menu_item {
                        0 => {
                            if self.block_joined_client_save() {
                                return true;
                            }
                            match save_legacy_shell_game(self) {
                                Ok(()) => {
                                    self.save_dirty = false;
                                    self.request_exit_and_online_shutdown();
                                }
                                Err(error) => {
                                    self.message = format!("Save before exit failed: {error}");
                                }
                            }
                        }
                        1 => self.request_exit_and_online_shutdown(),
                        _ => self.modal = None,
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn reveal_near_player(&mut self) {
        let center_x = (self.player.x / TILE_SIZE).floor() as i32;
        let center_y = (self.player.y / TILE_SIZE).floor() as i32;
        for y in center_y - 3..=center_y + 3 {
            for x in center_x - 4..=center_x + 4 {
                let position = TilePosition { x, y };
                if let Some(index) = self.tile_index(position)
                    && !self.explored_tiles[index]
                {
                    self.explored_tiles[index] = true;
                    self.mark_exploration_visual_changed(position);
                }
            }
        }
    }

    fn reveal_scanner_area(&mut self) {
        if self.player.scanner_level == 0 {
            return;
        }
        let center_x = (self.player.x / TILE_SIZE).floor() as i32;
        let center_y = (self.player.y / TILE_SIZE).floor() as i32;
        let radius = 3
            + i32::from(self.player.scanner_level) * 2
            + i32::from(self.town_development.scanner_lab_level);
        for y in center_y - radius..=center_y + radius {
            for x in center_x - radius..=center_x + radius {
                if (x - center_x).abs() + (y - center_y).abs() <= radius {
                    let position = TilePosition { x, y };
                    if let Some(index) = self.tile_index(position)
                        && !self.explored_tiles[index]
                    {
                        self.explored_tiles[index] = true;
                        self.mark_exploration_visual_changed(position);
                    }
                    if let Some(tile) = self.terrain.tile(position) {
                        self.collection_log.discover_tile(tile.kind);
                        let can_mark = scanner_can_mark(tile.kind, self.player.scanner_level)
                            || (self.has_equipped_part(RigPartKind::ProspectorScanner)
                                && matches!(tile.kind, TileKind::Ore(_)))
                            || (self.has_equipped_part(RigPartKind::HazardScanner)
                                && matches!(
                                    tile.kind,
                                    TileKind::Gas
                                        | TileKind::Lava
                                        | TileKind::MagmaVent
                                        | TileKind::ExplosivePocket
                                        | TileKind::PressurePocket
                                ))
                            || (self.has_equipped_part(RigPartKind::RelicScanner)
                                && matches!(tile.kind, TileKind::Artifact(_)));
                        if can_mark
                            && !self
                                .scan_markers
                                .iter()
                                .any(|marker| marker.position == position)
                        {
                            self.scan_markers.push(ScanMarker {
                                position,
                                kind: tile.kind,
                            });
                        }
                    }
                }
            }
        }
    }

    fn update_persistent_ore_prediction(&mut self) {
        if self.town_development.scanner_lab_level < 2 || !self.update_ticks.is_multiple_of(90) {
            return;
        }
        let center_x = (self.player.x / TILE_SIZE).floor() as i32;
        let center_y = (self.player.y / TILE_SIZE).floor() as i32;
        let depth_bias = 8 + i32::from(self.town_development.scanner_lab_level) * 4;
        let radius = 4 + i32::from(self.town_development.scanner_lab_level);
        for y in center_y..=center_y + depth_bias {
            for x in center_x - radius..=center_x + radius {
                let position = TilePosition { x, y };
                let Some(tile) = self.terrain.tile(position) else {
                    continue;
                };
                if !matches!(tile.kind, TileKind::Ore(_) | TileKind::Artifact(_)) {
                    continue;
                }
                if self
                    .scan_markers
                    .iter()
                    .any(|marker| marker.position == position)
                {
                    continue;
                }
                self.scan_markers.push(ScanMarker {
                    position,
                    kind: tile.kind,
                });
                if self.scan_markers.len() > 80 {
                    self.scan_markers.remove(0);
                }
                return;
            }
        }
    }

    fn update_survey_drones(&mut self) {
        if !self.update_ticks.is_multiple_of(30) {
            return;
        }
        let drones = self
            .infrastructure
            .iter()
            .filter(|item| item.kind == InfrastructureKind::SurveyDrone)
            .copied()
            .collect::<Vec<_>>();
        for drone in drones {
            let radius = 3 + i32::from(self.town_development.scanner_lab_level);
            for y in drone.position.y - radius..=drone.position.y + radius {
                for x in drone.position.x - radius..=drone.position.x + radius {
                    if (x - drone.position.x).abs() + (y - drone.position.y).abs() > radius {
                        continue;
                    }
                    let position = TilePosition { x, y };
                    if let Some(index) = self.tile_index(position)
                        && !self.explored_tiles[index]
                    {
                        self.explored_tiles[index] = true;
                        self.mark_exploration_visual_changed(position);
                    }
                    if let Some(tile) = self.terrain.tile(position) {
                        self.collection_log.discover_tile(tile.kind);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn is_explored(&self, position: TilePosition) -> bool {
        self.tile_index(position)
            .and_then(|index| self.explored_tiles.get(index))
            .copied()
            .unwrap_or(false)
    }

    #[must_use]
    pub fn expedition_progress(&self, expedition: Expedition) -> u32 {
        match expedition.kind {
            ExpeditionObjectiveKind::ReachDepth => {
                (self.deepest_tile_reached as u32).min(expedition.required)
            }
            ExpeditionObjectiveKind::DeliverCargo => match expedition.target {
                TileKind::Ore(mineral) => self
                    .player
                    .cargo
                    .get(&mineral)
                    .copied()
                    .unwrap_or(0)
                    .min(expedition.required),
                TileKind::Artifact(artifact) => self
                    .player
                    .artifacts
                    .get(&artifact)
                    .copied()
                    .unwrap_or(0)
                    .min(expedition.required),
                _ => 0,
            },
            ExpeditionObjectiveKind::ScanHazards => {
                self.scan_markers
                    .iter()
                    .filter(|marker| marker.kind == expedition.target)
                    .count()
                    .min(expedition.required as usize) as u32
            }
            ExpeditionObjectiveKind::BuildPumpStations => {
                self.infrastructure
                    .iter()
                    .filter(|item| item.kind == InfrastructureKind::PumpStation)
                    .count()
                    .min(expedition.required as usize) as u32
            }
            ExpeditionObjectiveKind::RecoverProbe
            | ExpeditionObjectiveKind::ScanAnomaly
            | ExpeditionObjectiveKind::RescueMiner
            | ExpeditionObjectiveKind::StabilizeCollapse
            | ExpeditionObjectiveKind::ReachSignal => u32::from(
                self.deepest_tile_reached
                    >= i32::try_from(60 + expedition.required * 10).unwrap_or(i32::MAX),
            ),
            ExpeditionObjectiveKind::MineVein => self.player.cargo_used().min(expedition.required),
            ExpeditionObjectiveKind::DeliverExplosives => {
                self.player.bombs.min(expedition.required)
            }
            ExpeditionObjectiveKind::RetrieveArtifact => self
                .player
                .artifacts
                .values()
                .sum::<u32>()
                .min(expedition.required),
            ExpeditionObjectiveKind::NoDamageReturn => {
                u32::from(self.player.hull >= self.player.max_hull())
            }
            ExpeditionObjectiveKind::FastReturn => {
                u32::from(self.town_event_day <= expedition.expires_day)
            }
        }
    }

    #[must_use]
    pub fn expedition_status_line(&self, expedition: Expedition) -> String {
        format!(
            "{} {}/{} | {} cr | {} risk | day {}",
            expedition.title(),
            self.expedition_progress(expedition),
            expedition.required,
            expedition.reward,
            expedition.risk_label(),
            expedition.expires_day
        )
    }

    #[must_use]
    pub fn warning_summary(&self) -> String {
        let mut warnings = Vec::new();
        if self.player.fuel <= self.player.fuel_capacity * 0.2 {
            warnings.push("fuel");
        }
        if self.player.hull <= self.player.max_hull() * 0.35 {
            warnings.push("hull");
        }
        if self.player.tile_position(TILE_SIZE).y >= 60 {
            warnings.push("heat");
        }
        if self.deep_instability >= 60.0 {
            warnings.push("instability");
        }
        if self.player.tile_position(TILE_SIZE).y >= 80 && self.signal_relay_count() == 0 {
            warnings.push("signal");
        }
        if warnings.is_empty() {
            "Warnings: nominal".to_owned()
        } else {
            format!("Warnings: {}", warnings.join(", "))
        }
    }

    #[must_use]
    pub const fn reputation_rank(&self) -> &'static str {
        match self.town_development.reputation {
            0..=2 => "Prospector",
            3..=6 => "Claim Lead",
            7..=11 => "Charter Operator",
            _ => "Deep Claim Founder",
        }
    }

    #[must_use]
    pub const fn advanced_permit_status(&self) -> &'static str {
        if self.town_development.reputation >= 8 && self.best_return_depth >= 80 {
            "Advanced permits unlocked"
        } else {
            "Advanced permits need rank 8 + 80m return"
        }
    }

    #[must_use]
    pub fn has_world_event(&self, kind: WorldEventKind) -> bool {
        self.active_world_events
            .iter()
            .any(|event| event.kind == kind && event.days_remaining > 0)
    }

    #[must_use]
    pub fn world_event_severity(&self, kind: WorldEventKind) -> u32 {
        self.active_world_events
            .iter()
            .filter(|event| event.kind == kind && event.days_remaining > 0)
            .map(|event| event.severity)
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn active_world_event_summary(&self) -> String {
        if self.active_world_events.is_empty() {
            return "No active world events.".to_owned();
        }
        self.active_world_events
            .iter()
            .map(|event| {
                format!(
                    "{} S{} ({}d)",
                    event.kind.label(),
                    event.severity,
                    event.days_remaining
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    #[must_use]
    pub fn mineral_market_value(&self, mineral: MineralKind) -> u32 {
        let mut factor =
            self.mineral_market_factor(mineral) + u32::from(self.town_development.depot_level) * 3;
        if self.has_world_event(WorldEventKind::MarketBoom) {
            factor += 25;
        }
        if self.has_world_event(WorldEventKind::RareBuyer)
            && matches!(
                mineral,
                MineralKind::Diamond | MineralKind::Mythril | MineralKind::Uranium
            )
        {
            factor += 45;
        }
        if self.has_world_event(WorldEventKind::MarketCrash) {
            factor = factor.saturating_sub(25).max(45);
        }
        (mineral.value() * factor) / 100
    }

    #[must_use]
    pub const fn mineral_market_factor(&self, mineral: MineralKind) -> u32 {
        market_factor_for(self.market_salt, self.town_event_day, mineral)
    }

    #[must_use]
    pub fn previous_mineral_market_factor(&self, mineral: MineralKind) -> Option<u32> {
        self.mineral_market_history
            .get(&mineral)
            .and_then(|history| history.iter().rev().nth(1).copied())
    }

    const fn tile_index(&self, position: TilePosition) -> Option<usize> {
        if position.x < 0
            || position.y < 0
            || position.x >= self.terrain.width()
            || position.y >= self.terrain.height()
        {
            return None;
        }
        Some((position.y * self.terrain.width() + position.x) as usize)
    }

    fn handle_pause_menu(&mut self, input: PlayerInput) {
        if self.modal == Some(ModalScreen::ExitConfirm)
            || self.modal == Some(ModalScreen::UnsavedExitConfirm)
        {
            self.handle_exit_modal(input);
            return;
        }

        if input.menu_up {
            self.selected_pause_item = self.selected_pause_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_pause_item =
                (self.selected_pause_item + 1).min(PauseOption::ALL.len() - 1);
        }

        if input.pause || input.cancel {
            self.run_mode = RunMode::Playing;
            return;
        }

        if !input.confirm {
            return;
        }

        match PauseOption::ALL[self.selected_pause_item] {
            PauseOption::Resume => self.run_mode = RunMode::Playing,
            PauseOption::Save => {
                if self.block_joined_client_save() {
                    return;
                }
                self.modal = Some(ModalScreen::SaveSlots);
                self.selected_menu_item = 0;
            }
            PauseOption::Load => {
                if self.block_joined_client_load() {
                    return;
                }
                self.modal = Some(ModalScreen::LoadSlots);
                self.selected_menu_item = 0;
            }
            PauseOption::OnlineSession => {
                self.modal = Some(ModalScreen::OnlineMultiplayer);
                self.selected_menu_item = if self.online_session_active() { 11 } else { 0 };
            }
            PauseOption::Options => {
                self.modal = Some(ModalScreen::Options);
                self.selected_menu_item = 0;
            }
            PauseOption::ExitToDesktop => self.request_exit_or_prompt(),
        }
    }

    fn handle_save_load(&mut self, input: PlayerInput) {
        if input.save {
            if self.block_joined_client_save() {
                return;
            }
            match save_legacy_shell_game(self) {
                Ok(()) => {
                    self.save_dirty = false;
                    "Game saved to drillgame-save.json.".clone_into(&mut self.message);
                }
                Err(error) => self.message = format!("Save failed: {error}"),
            }
        }

        if input.load {
            if self.block_joined_client_load() {
                return;
            }
            self.load_into_self();
        }
    }

    fn load_into_self(&mut self) {
        if self.block_joined_client_load() {
            return;
        }
        if !save_exists() {
            "No save file found.".clone_into(&mut self.message);
            return;
        }

        match load_game() {
            Ok(mut loaded) => {
                "Game loaded.".clone_into(&mut loaded.message);
                loaded.migrate_loaded_state();
                loaded.mark_full_terrain_refresh();
                *self = loaded;
                self.save_dirty = false;
            }
            Err(error) => self.message = format!("Load failed: {error}"),
        }
    }

    fn migrate_loaded_state(&mut self) {
        if self.save_version < current_save_version() {
            self.contracts.migrate_after_load();
            self.scan_markers
                .retain(|marker| scanner_can_mark(marker.kind, self.player.scanner_level));
            if self.side_contract_active && self.side_contract_required == 0 {
                self.side_contract_active = false;
            }
            if self.side_contract_active
                && self.active_side_contracts.is_empty()
                && let Some(target) = self.side_contract_target
            {
                self.active_side_contracts.push(SideContract {
                    kind: self.side_contract_kind,
                    target,
                    required: self.side_contract_required.max(1),
                    expires_day: None,
                });
            }
            if self.won_game {
                self.deep_claim_status = DeepClaimStatus::Unlocked;
            }
            if self.mineral_market_history.is_empty() {
                self.mineral_market_history =
                    initial_mineral_market_history(self.market_salt, self.town_event_day);
            }
            self.save_version = current_save_version();
        }
    }

    fn handle_interaction(&mut self, input: PlayerInput) {
        if !input.interact {
            return;
        }

        if self.try_use_ore_processor() {
            return;
        }
        if self.try_use_cargo_lift() {
            return;
        }

        if let Some(zone) = self.current_zone {
            self.enter_interior(zone);
        } else {
            "No surface service here.".clone_into(&mut self.message);
        }
    }

    fn try_use_ore_processor(&mut self) -> bool {
        let position = self.player.tile_position(TILE_SIZE);
        let Some(processor_index) = self.infrastructure.iter().position(|item| {
            item.kind == InfrastructureKind::OreProcessor
                && (item.position.x - position.x).abs() <= 1
                && (item.position.y - position.y).abs() <= 1
        }) else {
            return false;
        };
        let recipes = [
            (MineralKind::Copper, StrategicResourceKind::AncientAlloy),
            (MineralKind::Iron, StrategicResourceKind::AncientAlloy),
            (MineralKind::Silver, StrategicResourceKind::CrystalLens),
        ];
        for (mineral, material) in recipes {
            let Some(count) = self.player.cargo.get_mut(&mineral) else {
                continue;
            };
            if *count < 5 {
                continue;
            }
            *count -= 5;
            self.player.cargo.retain(|_, count| *count > 0);
            self.player.add_material(material, 1);
            self.total_resources_refined = self.total_resources_refined.saturating_add(1);
            if let Some(processor) = self.infrastructure.get_mut(processor_index) {
                processor.durability = processor.durability.saturating_sub(8);
            }
            self.message = format!(
                "Ore processor refined 5 {} into 1 {}.",
                mineral.name(),
                material.name()
            );
            self.sound_cues.push(SoundCue::Upgrade);
            return true;
        }
        "Ore processor needs 5 Copper, Iron, or Silver cargo.".clone_into(&mut self.message);
        true
    }

    fn try_use_cargo_lift(&mut self) -> bool {
        let position = self.player.tile_position(TILE_SIZE);
        let Some(lift_index) = self.infrastructure.iter().position(|item| {
            item.kind == InfrastructureKind::CargoLift
                && (item.position.x - position.x).abs() <= 1
                && (item.position.y - position.y).abs() <= 1
        }) else {
            return false;
        };
        if self.player.cargo.is_empty() {
            if self.fast_travel_between_cargo_lifts(lift_index) {
                return true;
            }
            "Cargo lift ready, but mineral cargo is empty.".clone_into(&mut self.message);
            return true;
        }
        let capacity = 8_u32;
        let mut remaining = capacity;
        let mut value = 0;
        for mineral in all_minerals() {
            if remaining == 0 {
                break;
            }
            let Some(count) = self.player.cargo.get_mut(&mineral) else {
                continue;
            };
            let sent = (*count).min(remaining);
            *count -= sent;
            remaining -= sent;
            value += self.mineral_market_value(mineral) * sent;
        }
        self.player.cargo.retain(|_, count| *count > 0);
        if value == 0 {
            "Cargo lift found no mineral cargo to send.".clone_into(&mut self.message);
            return true;
        }
        if let Some(lift) = self.infrastructure.get_mut(lift_index) {
            lift.durability = lift.durability.saturating_sub(5);
        }
        self.player.credits += value;
        self.message = format!(
            "Cargo lift sent {} unit(s) upward for {value} credits.",
            capacity - remaining
        );
        self.sound_cues.push(SoundCue::Sell);
        true
    }

    fn fast_travel_between_cargo_lifts(&mut self, current_lift_index: usize) -> bool {
        if self.town_development.reputation < 6 {
            return false;
        }
        let Some(current) = self.infrastructure.get(current_lift_index) else {
            return false;
        };
        let destination = self
            .infrastructure
            .iter()
            .enumerate()
            .filter(|(index, item)| {
                *index != current_lift_index && item.kind == InfrastructureKind::CargoLift
            })
            .min_by_key(|(_, item)| item.position.y.abs_diff(current.position.y));
        let Some((_, lift)) = destination else {
            return false;
        };
        self.player.x = lift.position.x as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        self.player.y = lift.position.y as f32 * TILE_SIZE + TILE_SIZE / 2.0;
        "Cargo lift network moved rig to the nearest linked station.".clone_into(&mut self.message);
        self.sound_cues.push(SoundCue::Milestone);
        true
    }

    fn enter_interior(&mut self, zone: SurfaceZone) {
        self.run_mode = RunMode::Interior;
        self.interior_zone = Some(zone);
        self.interior_x = 82.0;
        self.interior_facing = 1.0;
        self.modal = None;
        self.selected_menu_item = 0;
        self.sound_cues.push(SoundCue::Ui);
        self.message = format!(
            "Entered {}. Walk to a counter and press E; door exits.",
            surface_zone_label(zone)
        );
    }

    fn handle_interior(&mut self, input: PlayerInput, delta_seconds: f32) {
        if self.handle_modal(input) {
            return;
        }
        if input.pause {
            self.run_mode = RunMode::Paused;
            return;
        }
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            self.interior_facing = movement.signum();
        }
        self.interior_x = (self.interior_x + movement * 185.0 * delta_seconds).clamp(42.0, 598.0);
        if input.cancel || (input.interact && self.interior_x < 74.0) {
            self.exit_interior();
            return;
        }
        if input.interact {
            self.open_interior_hotspot();
        }
    }

    fn exit_interior(&mut self) {
        self.run_mode = RunMode::Playing;
        self.interior_zone = None;
        self.modal = None;
        self.sound_cues.push(SoundCue::Ui);
        "Back outside.".clone_into(&mut self.message);
    }

    fn open_interior_hotspot(&mut self) {
        let Some(zone) = self.interior_zone else {
            return;
        };
        if (self.interior_x - interior_service_x(zone)).abs() > 70.0 {
            "Walk to the service counter or the exit door.".clone_into(&mut self.message);
            return;
        }
        match zone {
            SurfaceZone::Fuel => self.modal = Some(ModalScreen::Fuel),
            SurfaceZone::Repair => self.modal = Some(ModalScreen::Repair),
            SurfaceZone::Depot => {
                self.modal = Some(ModalScreen::Depot);
                self.selected_menu_item = 1;
            }
            SurfaceZone::Headquarters => self.modal = Some(ModalScreen::Headquarters),
            SurfaceZone::Shop => self.modal = Some(ModalScreen::Shop),
            SurfaceZone::Bank => self.modal = Some(ModalScreen::Bank),
            SurfaceZone::Explosives => self.modal = Some(ModalScreen::Explosives),
            SurfaceZone::Salvage => self.modal = Some(ModalScreen::Salvage),
        }
        self.sound_cues.push(SoundCue::Ui);
    }

    fn handle_modal(&mut self, input: PlayerInput) -> bool {
        let Some(modal) = self.modal else {
            return false;
        };

        if modal == ModalScreen::OnlineMultiplayer && self.handle_online_text_input(input) {
            return true;
        }

        if input.cancel {
            if modal == ModalScreen::OnlineMultiplayer {
                self.close_online_multiplayer_menu();
            } else {
                self.modal = None;
            }
            return true;
        }

        if input.menu_up {
            self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
        }
        if input.menu_down {
            let max_item = match modal {
                ModalScreen::Depot => 4,
                ModalScreen::Fuel
                | ModalScreen::Repair
                | ModalScreen::Options
                | ModalScreen::Bank => 2,
                ModalScreen::Explosives => 3,
                ModalScreen::Salvage => 5,
                ModalScreen::Headquarters => {
                    if self.deep_claim_status == DeepClaimStatus::Unlocked {
                        6
                    } else {
                        2
                    }
                }
                ModalScreen::Crafting => RecipeKind::ALL.len() - 1,
                ModalScreen::TownDevelopment => TownBuilding::ALL.len() - 1,
                ModalScreen::ExpeditionBoard => self
                    .expedition_offers
                    .len()
                    .saturating_add(self.active_expeditions.len())
                    .saturating_sub(1),
                ModalScreen::SaveSlots | ModalScreen::LoadSlots => save_slot_count() - 1,
                ModalScreen::OnlineMultiplayer => match self.online_multiplayer_view {
                    OnlineMultiplayerView::MainMenu
                    | OnlineMultiplayerView::HostLan
                    | OnlineMultiplayerView::JoinLan => 3,
                    OnlineMultiplayerView::AdvancedDirectConnect => 9,
                },
                _ => 0,
            };
            self.selected_menu_item = (self.selected_menu_item + 1).min(max_item);
        }

        if matches!(modal, ModalScreen::Shop) {
            if input.menu_left {
                self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
            }
            if input.menu_right {
                self.selected_menu_item =
                    (self.selected_menu_item + 1).min(upgrade_offers(&self.player).len() - 1);
            }
        }

        if matches!(modal, ModalScreen::OnlineMultiplayer) && (input.menu_left || input.menu_right)
        {
            self.adjust_online_multiplayer_selection();
        }

        if let Some(index) = input.selected_upgrade {
            self.selected_menu_item = index.min(upgrade_offers(&self.player).len() - 1);
        }

        if input.confirm {
            match modal {
                ModalScreen::Fuel => self.modal = Some(ModalScreen::FuelConfirm),
                ModalScreen::FuelConfirm => self.confirm_refuel(),
                ModalScreen::Repair => self.modal = Some(ModalScreen::RepairConfirm),
                ModalScreen::RepairConfirm => self.confirm_repair(),
                ModalScreen::Depot => self.confirm_depot(),
                ModalScreen::Headquarters => self.confirm_headquarters(),
                ModalScreen::DepotReceiptHistory => self.modal = Some(ModalScreen::Depot),
                ModalScreen::Shop => self.modal = Some(ModalScreen::ShopConfirm),
                ModalScreen::ShopConfirm => self.queue_buy_upgrade(self.selected_menu_item),
                ModalScreen::Bank => self.confirm_bank_menu(),
                ModalScreen::Explosives => self.confirm_explosives_menu(),
                ModalScreen::Salvage => self.confirm_salvage_menu(),
                ModalScreen::TownDevelopment => self.confirm_town_development(),
                ModalScreen::ExpeditionBoard => self.accept_expedition_offer(),
                ModalScreen::Crafting => self.confirm_crafting(),
                ModalScreen::Options => self.confirm_options(),
                ModalScreen::SaveSlots => self.save_slot(self.selected_menu_item),
                ModalScreen::LoadSlots => self.load_slot(self.selected_menu_item),
                ModalScreen::ExitConfirm
                | ModalScreen::UnsavedExitConfirm
                | ModalScreen::Map
                | ModalScreen::Help
                | ModalScreen::Inventory
                | ModalScreen::ResearchLog => {}
                ModalScreen::OnlineMultiplayer => self.confirm_online_multiplayer(),
            }
        }

        true
    }

    fn confirm_options(&mut self) {
        match self.selected_menu_item {
            0 => {
                self.master_volume = (self.master_volume + 0.1).min(1.0);
                self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            }
            1 => {
                self.master_volume = (self.master_volume - 0.1).max(0.0);
                self.message = format!("Volume: {:.0}%", self.master_volume * 100.0);
            }
            _ => {
                self.fullscreen = !self.fullscreen;
                self.message = if self.fullscreen {
                    "Fullscreen preference enabled; F11 toggles immediately.".to_owned()
                } else {
                    "Windowed preference enabled; F11 toggles immediately.".to_owned()
                };
            }
        }
        self.settings_dirty = true;
        self.sound_cues.push(SoundCue::Ui);
    }

    fn save_slot(&mut self, slot: usize) {
        if self.block_joined_client_save() {
            self.modal = Some(ModalScreen::SaveSlots);
            return;
        }
        self.session_slot_io_request = Some(SessionSlotIoRequest::Save { slot });
        self.message = format!("Saving slot {} from authoritative session...", slot + 1);
        self.modal = Some(ModalScreen::SaveSlots);
    }

    fn load_slot(&mut self, slot: usize) {
        if self.block_joined_client_load() {
            self.modal = Some(ModalScreen::LoadSlots);
            return;
        }
        self.session_slot_io_request = Some(SessionSlotIoRequest::Load { slot });
        self.message = format!("Loading slot {} into authoritative session...", slot + 1);
    }

    #[must_use]
    pub const fn take_session_slot_io_request(&mut self) -> Option<SessionSlotIoRequest> {
        self.session_slot_io_request.take()
    }

    fn confirm_refuel(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::Refuel {
            menu_item: self.selected_menu_item,
        });
        "Refuel queued for authoritative session.".clone_into(&mut self.message);
        self.modal = Some(ModalScreen::Fuel);
    }

    fn confirm_repair(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::Repair {
            menu_item: self.selected_menu_item,
        });
        "Repair queued for authoritative session.".clone_into(&mut self.message);
        self.modal = Some(ModalScreen::Repair);
    }

    #[must_use]
    pub const fn take_session_service_request(&mut self) -> Option<SessionServiceRequest> {
        self.session_service_request.take()
    }

    fn confirm_headquarters(&mut self) {
        match self.selected_menu_item {
            0 => self.queue_complete_depot_work(),
            1 => {
                self.message = hq_story_message(self);
                self.sound_cues.push(SoundCue::Milestone);
            }
            2 => self.queue_finance(),
            3 if self.deep_claim_status == DeepClaimStatus::Unlocked => {
                self.modal = Some(ModalScreen::TownDevelopment);
                self.selected_menu_item = 0;
            }
            4 if self.deep_claim_status == DeepClaimStatus::Unlocked => {
                self.refresh_expedition_offers();
                self.modal = Some(ModalScreen::ExpeditionBoard);
                self.selected_menu_item = 0;
            }
            5 if self.deep_claim_status == DeepClaimStatus::Unlocked => {
                self.modal = Some(ModalScreen::ResearchLog);
                self.selected_menu_item = 0;
            }
            _ if self.deep_claim_status == DeepClaimStatus::Unlocked => {
                self.modal = Some(ModalScreen::Crafting);
                self.selected_menu_item = 0;
            }
            _ => self.queue_finance(),
        }
    }

    fn confirm_crafting(&mut self) {
        let recipe = RecipeKind::ALL[self.selected_menu_item.min(RecipeKind::ALL.len() - 1)];
        self.session_service_request = Some(SessionServiceRequest::CraftRecipe { recipe });
        self.message = format!(
            "Crafting {} queued for authoritative session.",
            recipe.name()
        );
    }

    fn confirm_town_development(&mut self) {
        let building = TownBuilding::ALL[self.selected_menu_item.min(TownBuilding::ALL.len() - 1)];
        self.session_service_request =
            Some(SessionServiceRequest::UpgradeTownBuilding { building });
        self.message = format!(
            "{} upgrade queued for authoritative session.",
            building.name()
        );
    }

    fn refresh_expedition_offers(&mut self) {
        if self.deep_claim_status != DeepClaimStatus::Unlocked {
            self.expedition_offers.clear();
            return;
        }
        if !self.expedition_offers.is_empty() {
            return;
        }
        let day = self.town_event_day;
        self.expedition_offers = vec![
            Expedition {
                kind: ExpeditionObjectiveKind::ReachDepth,
                target: TileKind::Stone,
                required: (self.deepest_tile_reached.max(80) as u32 + 15).min(140),
                reward: 320 + day * 12,
                expires_day: day + 4,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::DeliverCargo,
                target: TileKind::Ore(MineralKind::Platinum),
                required: 2,
                reward: 420 + day * 14,
                expires_day: day + 3,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::ScanHazards,
                target: TileKind::Gas,
                required: 4,
                reward: 360 + day * 10,
                expires_day: day + 3,
            },
            Expedition {
                kind: ExpeditionObjectiveKind::BuildPumpStations,
                target: TileKind::MagmaVent,
                required: 2,
                reward: 460 + day * 12,
                expires_day: day + 5,
            },
        ];
        let rotating_kind = match day % 10 {
            0 => ExpeditionObjectiveKind::RecoverProbe,
            1 => ExpeditionObjectiveKind::MineVein,
            2 => ExpeditionObjectiveKind::ScanAnomaly,
            3 => ExpeditionObjectiveKind::RescueMiner,
            4 => ExpeditionObjectiveKind::DeliverExplosives,
            5 => ExpeditionObjectiveKind::StabilizeCollapse,
            6 => ExpeditionObjectiveKind::RetrieveArtifact,
            7 => ExpeditionObjectiveKind::ReachSignal,
            8 => ExpeditionObjectiveKind::NoDamageReturn,
            _ => ExpeditionObjectiveKind::FastReturn,
        };
        self.expedition_offers.push(Expedition {
            kind: rotating_kind,
            target: TileKind::Artifact(ArtifactKind::BuriedIdol),
            required: 1 + day % 3,
            reward: 520 + day * 16,
            expires_day: day + 2,
        });
        if self.town_development.reputation >= 8 && self.best_return_depth >= 80 {
            self.expedition_offers.push(Expedition {
                kind: ExpeditionObjectiveKind::ReachDepth,
                target: TileKind::Artifact(ArtifactKind::OldCircuit),
                required: 150,
                reward: 950 + day * 20,
                expires_day: day + 2,
            });
        }
    }

    fn accept_expedition_offer(&mut self) {
        if self.selected_menu_item >= self.expedition_offers.len() {
            self.abandon_selected_expedition();
            return;
        }
        self.session_service_request = Some(SessionServiceRequest::AcceptExpedition {
            index: self.selected_menu_item,
        });
        "Expedition acceptance queued for authoritative session.".clone_into(&mut self.message);
    }

    fn abandon_selected_expedition(&mut self) {
        let active_index = self
            .selected_menu_item
            .saturating_sub(self.expedition_offers.len());
        self.session_service_request = Some(SessionServiceRequest::AbandonExpedition {
            index: active_index,
        });
        "Expedition abandonment queued for authoritative session.".clone_into(&mut self.message);
    }

    fn confirm_bank_menu(&mut self) {
        match self.selected_menu_item {
            0 => self.queue_finance(),
            1 => self.queue_insurance(),
            _ => self.queue_start_side_contract(),
        }
    }

    fn confirm_explosives_menu(&mut self) {
        match self.selected_menu_item {
            0 => self.queue_bomb_bundle(3, 55),
            1 => self.queue_bomb_bundle(7, 120),
            2 => self.queue_mining_rockets(),
            _ => self.queue_free_test_charge(),
        }
    }

    fn confirm_salvage_menu(&mut self) {
        match self.selected_menu_item {
            0 => self.queue_salvage_recover_lost_cargo(),
            1 => self.queue_salvage_patch_hull(),
            2 => self.queue_salvage_launch_drone(),
            3 => self.queue_salvage_recover_wrecked_part(),
            4 => self.queue_salvage_clear_collapse_zones(),
            _ => self.queue_salvage_sell_scrap_tip(),
        }
    }

    fn queue_bomb_bundle(&mut self, count: u32, cost: u32) {
        self.session_service_request = Some(SessionServiceRequest::BuyBombBundle { count, cost });
        self.message =
            format!("Bomb bundle queued for authoritative session ({count} for {cost} cr).");
    }

    fn queue_mining_rockets(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::BuyMiningRockets);
        "Mining rockets queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_free_test_charge(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::ClaimFreeTestCharge);
        "Test charge queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_patch_hull(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvagePatchHull);
        "Salvage patch queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_recover_lost_cargo(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvageRecoverLostCargo);
        "Lost cargo recovery queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_launch_drone(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvageLaunchDrone);
        "Salvage drone queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_recover_wrecked_part(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvageRecoverWreckedPart);
        "Wrecked rig recovery queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_clear_collapse_zones(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvageClearCollapseZones);
        "Collapse-zone cleanup queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_salvage_sell_scrap_tip(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SalvageSellScrapTip);
        "Scrap telemetry sale queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_finance(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::Finance);
        "Finance request queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_insurance(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::BuyInsurance);
        "Insurance purchase queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_start_side_contract(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::StartSideContract);
        "Side contract request queued for authoritative session.".clone_into(&mut self.message);
    }

    fn confirm_depot(&mut self) {
        match self.selected_menu_item {
            0 => self.queue_complete_depot_work(),
            1 => self.queue_sell_cargo(),
            2 => self.queue_auto_sort_low_grade_cargo(),
            3 => self.queue_sell_scan_data(),
            _ => self.modal = Some(ModalScreen::DepotReceiptHistory),
        }
    }

    fn queue_complete_depot_work(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::CompleteDepotWork);
        "Depot work completion queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_sell_cargo(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SellCargo);
        "Cargo sale queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_sell_scan_data(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::SellScanData);
        "Scan data sale queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_auto_sort_low_grade_cargo(&mut self) {
        self.session_service_request = Some(SessionServiceRequest::AutoSortLowGradeCargo);
        "Depot auto-sort queued for authoritative session.".clone_into(&mut self.message);
    }

    fn queue_buy_upgrade(&mut self, index: usize) {
        if self.current_zone != Some(SurfaceZone::Shop) {
            return;
        }
        self.session_service_request = Some(SessionServiceRequest::BuyUpgrade { index });
        "Upgrade purchase queued for authoritative session.".clone_into(&mut self.message);
        self.modal = Some(ModalScreen::Shop);
    }

    fn update_world_event_hazards(&mut self) {
        if self.run_mode != RunMode::Playing || self.game_over || self.player.y < TILE_SIZE * 8.0 {
            return;
        }
        if self.has_world_event(WorldEventKind::CollapseSurge)
            && self.update_ticks.is_multiple_of(180)
        {
            self.spawn_cave_in();
            "Collapse surge: falling rock detected nearby.".clone_into(&mut self.message);
        }
        if self.has_world_event(WorldEventKind::GasBloom) && self.update_ticks.is_multiple_of(240) {
            self.hazard_clouds.push(HazardCloud {
                x: self.player.x,
                y: self.player.y + TILE_SIZE,
                life: 6.0,
                radius: 12.0,
            });
            "Gas bloom: corrosive vapor is spreading through open tunnels."
                .clone_into(&mut self.message);
        }
    }

    fn update_npc_story_records(&mut self) {
        if self.current_zone != Some(SurfaceZone::Headquarters) {
            return;
        }
        let record = if self.won_game {
            NpcStoryRecord::ValeStarCoreSecured
        } else {
            match self.deepest_tile_reached {
                0..=19 => NpcStoryRecord::ValeIntro,
                20..=39 => NpcStoryRecord::IonaSilverWarning,
                40..=59 => NpcStoryRecord::KadeRelicSignal,
                60..=79 => NpcStoryRecord::ValeThermalWarning,
                _ => NpcStoryRecord::KadeStarCoreSignal,
            }
        };
        self.collection_log.story_records.insert(record);
    }

    fn update_collection_rewards(&mut self) {
        if self.collection_log.minerals.len() >= 10
            && self
                .collection_log
                .rewards_claimed
                .insert(CollectionRewardKind::Minerals)
        {
            self.player.credits = self.player.credits.saturating_add(500);
            self.player
                .add_material(StrategicResourceKind::CrystalLens, 2);
            "Mineral encyclopedia complete: awarded 500 credits and 2 Crystal Lenses."
                .clone_into(&mut self.message);
            self.sound_cues.push(SoundCue::Milestone);
        }
        if self.collection_log.artifacts.len() >= 4
            && self
                .collection_log
                .rewards_claimed
                .insert(CollectionRewardKind::Artifacts)
        {
            self.player.credits = self.player.credits.saturating_add(900);
            self.player
                .add_material(StrategicResourceKind::CoreShard, 2);
            "Artifact museum complete: awarded 900 credits and 2 Core Shards."
                .clone_into(&mut self.message);
            self.sound_cues.push(SoundCue::Milestone);
        }
        if self.collection_log.hazards.len() >= 5
            && self
                .collection_log
                .rewards_claimed
                .insert(CollectionRewardKind::Hazards)
        {
            self.town_development.scanner_lab_level =
                self.town_development.scanner_lab_level.saturating_add(1);
            "Hazard research complete: scanner lab upgraded.".clone_into(&mut self.message);
            self.sound_cues.push(SoundCue::Milestone);
        }
    }

    fn handle_scanner(&mut self, input: PlayerInput) {
        if !input.scan {
            return;
        }
        if self.has_world_event(WorldEventKind::DeepPressureStorm) {
            "Deep pressure storm is scrambling scanner returns today."
                .clone_into(&mut self.message);
            return;
        }
        if self.player.scanner_level == 0 {
            "No scanner installed. Buy one at the upgrade shop.".clone_into(&mut self.message);
            return;
        }
        if self.scanner_cooldown_seconds > 0.0 {
            self.message = format!("Scanner recharging: {:.1}s.", self.scanner_cooldown_seconds);
            return;
        }
        self.scanner_pulse_seconds = 1.2;
        self.scanner_cooldown_seconds = (7.0
            - f32::from(self.player.scanner_level)
            - f32::from(self.town_development.scanner_lab_level) * 0.35)
            .max(1.5);
        self.reveal_scanner_area();
        self.sound_cues.push(SoundCue::Ui);
        "Scanner pulse mapped ore, hazards, and artifacts nearby.".clone_into(&mut self.message);
    }

    fn update_scanner_timers(&mut self, delta_seconds: f32) {
        self.scanner_pulse_seconds = (self.scanner_pulse_seconds - delta_seconds).max(0.0);
        self.scanner_cooldown_seconds = (self.scanner_cooldown_seconds - delta_seconds).max(0.0);
    }

    fn handle_bomb(&mut self, input: PlayerInput) {
        if !input.bomb {
            return;
        }
        if self.player.bombs == 0 {
            "No bombs. Buy bomb packs at the upgrade shop.".clone_into(&mut self.message);
            return;
        }
        self.player.bombs -= 1;
        let remote_timer = if self.town_development.explosives_shack_level >= 3 {
            0.8
        } else {
            2.4
        };
        self.placed_bombs.push(PlacedBomb {
            x: self.player.x,
            y: self.player.y + TILE_SIZE * 0.4,
            timer_seconds: remote_timer,
        });
        self.sound_cues.push(SoundCue::Ui);
        self.message = format!(
            "Bomb armed: {remote_timer:.1} seconds. {} bombs left. Clear out!",
            self.player.bombs
        );
    }

    fn handle_infrastructure_placement(&mut self, input: PlayerInput) {
        if input.place_relay {
            self.place_infrastructure_kit(
                InfrastructureKind::SignalRelay,
                "No signal relay kits. Craft one at HQ first.",
            );
        }
        if input.place_drone {
            self.place_infrastructure_kit(
                InfrastructureKind::SurveyDrone,
                "No survey drone kits. Craft one at HQ first.",
            );
        }
        if input.place_lift {
            self.place_infrastructure_kit(
                InfrastructureKind::CargoLift,
                "No cargo lift kits. Craft one at HQ first.",
            );
        }
        if input.place_support {
            self.place_infrastructure_kit(
                InfrastructureKind::TunnelSupport,
                "No tunnel support kits. Craft one at HQ first.",
            );
        }
        if input.place_pump {
            self.place_infrastructure_kit(
                InfrastructureKind::PumpStation,
                "No pump station kits. Craft one at HQ first.",
            );
        }
        if input.place_processor {
            self.place_infrastructure_kit(
                InfrastructureKind::OreProcessor,
                "No ore processor kits. Craft one at HQ first.",
            );
        }
    }

    fn place_infrastructure_kit(&mut self, kind: InfrastructureKind, empty_message: &str) {
        let kits = match kind {
            InfrastructureKind::SignalRelay => self.player.signal_relay_kits,
            InfrastructureKind::SurveyDrone => self.player.survey_drone_kits,
            InfrastructureKind::CargoLift => self.player.cargo_lift_kits,
            InfrastructureKind::TunnelSupport => self.player.tunnel_support_kits,
            InfrastructureKind::PumpStation => self.player.pump_station_kits,
            InfrastructureKind::OreProcessor => self.player.ore_processor_kits,
        };
        if kits == 0 {
            empty_message.clone_into(&mut self.message);
            return;
        }
        let position = self.player.tile_position(TILE_SIZE);
        if position.y < 8 {
            format!("{} must be placed underground.", kind.name()).clone_into(&mut self.message);
            return;
        }
        if self
            .infrastructure
            .iter()
            .any(|item| item.position == position)
        {
            "Infrastructure already occupies this tile.".clone_into(&mut self.message);
            return;
        }
        match kind {
            InfrastructureKind::SignalRelay => {
                self.player.signal_relay_kits = self.player.signal_relay_kits.saturating_sub(1);
            }
            InfrastructureKind::SurveyDrone => {
                self.player.survey_drone_kits = self.player.survey_drone_kits.saturating_sub(1);
            }
            InfrastructureKind::CargoLift => {
                self.player.cargo_lift_kits = self.player.cargo_lift_kits.saturating_sub(1);
            }
            InfrastructureKind::TunnelSupport => {
                self.player.tunnel_support_kits = self.player.tunnel_support_kits.saturating_sub(1);
            }
            InfrastructureKind::PumpStation => {
                self.player.pump_station_kits = self.player.pump_station_kits.saturating_sub(1);
            }
            InfrastructureKind::OreProcessor => {
                self.player.ore_processor_kits = self.player.ore_processor_kits.saturating_sub(1);
            }
        }
        self.infrastructure.push(PlacedInfrastructure {
            kind,
            position,
            durability: default_infrastructure_durability(),
        });
        self.infrastructure_built = self.infrastructure_built.saturating_add(1);
        self.message = match kind {
            InfrastructureKind::SignalRelay => {
                "Placed Signal Relay. Deep rescue signal improved.".to_owned()
            }
            InfrastructureKind::SurveyDrone => {
                "Placed Survey Drone. Nearby map will reveal over time.".to_owned()
            }
            InfrastructureKind::CargoLift => {
                "Placed Cargo Lift. Press E on it to send cargo upward.".to_owned()
            }
            InfrastructureKind::TunnelSupport => {
                "Placed Tunnel Support. Nearby collapse warnings will be suppressed.".to_owned()
            }
            InfrastructureKind::PumpStation => {
                "Placed Pump Station. Nearby gas and heat hazards are suppressed.".to_owned()
            }
            InfrastructureKind::OreProcessor => {
                "Placed Ore Processor. Press E nearby to refine cheap ore.".to_owned()
            }
        };
        self.sound_cues.push(SoundCue::Upgrade);
    }

    fn effective_cargo_capacity(&self) -> u32 {
        let mut capacity = self.player.cargo_capacity;
        if self.has_equipped_part(RigPartKind::CargoBalloon) {
            capacity = capacity.saturating_add(20);
        }
        if self.has_equipped_part(RigPartKind::ArmoredCargoBay) {
            capacity = capacity.saturating_sub(4).max(1);
        }
        capacity
    }

    fn add_mined_cargo(&mut self, mineral: MineralKind) -> bool {
        if self.effective_cargo_capacity() == self.player.cargo_capacity {
            return self.player.add_cargo(mineral);
        }
        if self.player.cargo_used() >= self.effective_cargo_capacity() {
            return false;
        }
        *self.player.cargo.entry(mineral).or_default() += 1;
        true
    }

    fn add_mined_artifact(&mut self, artifact: ArtifactKind) -> bool {
        if self.effective_cargo_capacity() == self.player.cargo_capacity {
            return self.player.add_artifact(artifact);
        }
        if self.player.cargo_used() >= self.effective_cargo_capacity() {
            return false;
        }
        *self.player.artifacts.entry(artifact).or_default() += 1;
        true
    }

    fn has_equipped_part(&self, part: RigPartKind) -> bool {
        self.equipped_rig_parts
            .values()
            .any(|equipped| *equipped == part)
    }

    fn apply_movement(&mut self, input: PlayerInput, delta_seconds: f32) {
        let can_burn_fuel = self.player.fuel > 0.0;
        let grounded = self.is_grounded();
        let cargo_ratio =
            self.player.cargo_used() as f32 / self.effective_cargo_capacity().max(1) as f32;
        let cargo_pressure = if self.has_equipped_part(RigPartKind::HaulerEngine) {
            0.08
        } else if self.has_equipped_part(RigPartKind::LightweightEngine) {
            0.28
        } else {
            0.18
        };
        let cargo_penalty = 1.0 - cargo_ratio.min(1.0) * cargo_pressure;
        let mut engine_multiplier =
            (1.0 + f32::from(self.player.engine_level.saturating_sub(1)) * 0.28) * cargo_penalty;
        if self.has_equipped_part(RigPartKind::LightweightEngine) {
            engine_multiplier *= 1.12;
        }
        if self.has_equipped_part(RigPartKind::HaulerEngine) {
            engine_multiplier *= 0.92;
        }
        if self.has_equipped_part(RigPartKind::BurstEngine) {
            engine_multiplier *= 1.25;
        }
        if self.has_equipped_part(RigPartKind::CargoBalloon) {
            engine_multiplier *= 0.88;
        }
        let horizontal_acceleration = if grounded {
            HORIZONTAL_ACCELERATION * 1.35
        } else {
            HORIZONTAL_ACCELERATION * 0.65
        };

        self.player.velocity_x +=
            input.horizontal * horizontal_acceleration * engine_multiplier * delta_seconds;

        if input.thrust && can_burn_fuel {
            self.player.velocity_y -= THRUST_ACCELERATION * engine_multiplier * delta_seconds;
            let mut efficiency =
                1.0 - f32::from(self.player.fuel_tank_level.saturating_sub(1)) * 0.06;
            if self.has_equipped_part(RigPartKind::BurstEngine) {
                efficiency *= 1.22;
            }
            if self.has_equipped_part(RigPartKind::TitanDrill) {
                efficiency *= 1.08;
            }
            self.player.fuel =
                (self.player.fuel - FUEL_BURN_PER_SECOND * efficiency * delta_seconds).max(0.0);
        }

        self.player.velocity_y += GRAVITY * delta_seconds;
        let drag = if grounded { 0.78 } else { DRAG };
        self.player.velocity_x *= drag.powf(delta_seconds * 60.0);
        self.player.velocity_x = self.player.velocity_x.clamp(
            -MAX_HORIZONTAL_SPEED * engine_multiplier,
            MAX_HORIZONTAL_SPEED * engine_multiplier,
        );
        self.player.velocity_y = self
            .player
            .velocity_y
            .clamp(-MAX_FALL_SPEED, MAX_FALL_SPEED);

        self.move_axis(self.player.velocity_x * delta_seconds, 0.0);
        self.move_axis(0.0, self.player.velocity_y * delta_seconds);
    }

    fn move_axis(&mut self, delta_x: f32, delta_y: f32) {
        let next_x = self.player.x + delta_x;
        let next_y = self.player.y + delta_y;

        if self.collides(next_x, next_y) {
            if delta_x != 0.0 {
                if self.player.velocity_x.abs() > SAFE_LANDING_SPEED * 0.75 {
                    self.apply_bump_damage(self.player.velocity_x.abs());
                }
                self.player.velocity_x = 0.0;
            }
            if delta_y > 0.0 {
                self.apply_landing_damage();
            }
            if delta_y != 0.0 {
                self.player.velocity_y = 0.0;
            }
            return;
        }

        self.player.x = next_x.clamp(0.0, (self.terrain.width() as f32 - 1.0) * TILE_SIZE);
        self.player.y = next_y.clamp(
            MIN_PLAYER_Y,
            (self.terrain.height() as f32 - 1.0) * TILE_SIZE,
        );
    }

    fn apply_landing_damage(&mut self) {
        if self.player.velocity_y <= SAFE_LANDING_SPEED {
            return;
        }

        let raw_damage = (self.player.velocity_y - SAFE_LANDING_SPEED) * CRASH_DAMAGE_SCALE;
        let damage = self.mechanic_crash_damage(raw_damage);
        self.player.hull = (self.player.hull - damage).max(0.0);
        self.sound_cues.push(SoundCue::Damage);
        self.shake_camera(0.28, 7.0);
        self.spawn_sparks();
        self.message = format!("Hard landing! Hull took {damage:.0} damage.");
    }

    fn mechanic_crash_damage(&self, damage: f32) -> f32 {
        let mut mitigation = (f32::from(self.town_development.mechanic_level) * 0.08).min(0.32);
        if self.has_equipped_part(RigPartKind::ImpactHull) {
            mitigation += 0.22;
        }
        if self.has_equipped_part(RigPartKind::ThermalHull) {
            mitigation -= 0.10;
        }
        let mitigation = mitigation.clamp(0.0, 0.55);
        damage * (1.0 - mitigation)
    }

    fn apply_bump_damage(&mut self, speed: f32) {
        let raw_damage = (speed - SAFE_LANDING_SPEED * 0.75) * CRASH_DAMAGE_SCALE * 0.5;
        let damage = self.mechanic_crash_damage(raw_damage);
        self.player.hull = (self.player.hull - damage).max(0.0);
        self.sound_cues.push(SoundCue::Damage);
        self.shake_camera(0.2, 5.0);
        self.spawn_sparks();
        self.message = format!("Hull scraped the wall for {damage:.0} damage.");
    }

    fn collides(&self, x: f32, y: f32) -> bool {
        collision_points(x, y)
            .iter()
            .any(|position| self.terrain.is_solid_at(*position))
    }

    fn is_grounded(&self) -> bool {
        collision_points(self.player.x, self.player.y + 2.0)
            .iter()
            .any(|position| self.terrain.is_solid_at(*position))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "drilling update coordinates input, physics, terrain, feedback, and collection in one frame step"
    )]
    fn update_drilling(&mut self, input: PlayerInput, delta_seconds: f32) {
        let Some((target, direction)) = mine_target(&self.player, input) else {
            self.active_drill = None;
            return;
        };

        if direction != DrillDirection::Down
            && (!self.is_grounded() || self.player.velocity_y.abs() > 80.0)
        {
            self.active_drill = None;
            "Side drilling requires stable ground contact.".clone_into(&mut self.message);
            return;
        }

        if self.player.fuel <= 0.0 {
            self.active_drill = None;
            "Out of fuel. Reach a fuel station or await rescue.".clone_into(&mut self.message);
            return;
        }

        let Some(tile) = self.terrain.tile(target) else {
            self.active_drill = None;
            return;
        };
        if tile.kind == TileKind::Air {
            self.active_drill = None;
            return;
        }
        let effective_drill_strength = if self.has_equipped_part(RigPartKind::TitanDrill)
            || self.has_equipped_part(RigPartKind::ResonanceDrill)
        {
            self.player.drill_strength.saturating_add(1)
        } else {
            self.player.drill_strength
        };
        if self
            .terrain
            .hardness_at(target)
            .is_some_and(|hardness| hardness > effective_drill_strength)
        {
            self.active_drill = None;
            self.sound_cues.push(SoundCue::Damage);
            self.shake_camera(0.16, 4.0);
            self.spawn_sparks();
            "That layer is too hard. Upgrade your drill.".clone_into(&mut self.message);
            return;
        }

        let mut seconds_per_chip =
            drill_seconds_per_chip(tile.kind, effective_drill_strength, direction);
        if self.has_equipped_part(RigPartKind::NeedleDrill) {
            seconds_per_chip *= match tile.kind {
                TileKind::Dirt | TileKind::Clay => 0.72,
                TileKind::Stone | TileKind::HardRock => 1.25,
                _ => 0.95,
            };
        }
        if self.has_equipped_part(RigPartKind::TitanDrill) {
            seconds_per_chip *= 1.10;
        }
        if self.has_equipped_part(RigPartKind::ResonanceDrill)
            && matches!(
                deep_stratum_at_depth(target.y),
                Some(DeepStratum::CrystalFaults | DeepStratum::AncientMachineLayer)
            )
        {
            seconds_per_chip *= 0.65;
        }
        let reset = self
            .active_drill
            .is_none_or(|state| state.target != target || state.direction != direction);
        if reset {
            self.active_drill = Some(DrillState {
                target,
                direction,
                progress: 0.0,
                initial_durability: tile.durability.max(1),
                seconds_per_chip,
                sound_timer: 0.0,
                dust_timer: 0.0,
            });
        }

        self.player.fuel = (self.player.fuel - DRILL_FUEL_COST * 1.25 * delta_seconds).max(0.0);
        self.creep_into_drill(direction, delta_seconds);

        let mut should_chip = false;
        let mut should_spawn_dust = false;
        if let Some(state) = &mut self.active_drill {
            state.seconds_per_chip = seconds_per_chip;
            state.progress += delta_seconds / seconds_per_chip;
            state.sound_timer -= delta_seconds;
            state.dust_timer -= delta_seconds;
            if state.sound_timer <= 0.0 {
                self.sound_cues.push(SoundCue::Drill);
                state.sound_timer = 0.13;
            }
            if state.dust_timer <= 0.0 {
                should_spawn_dust = true;
                state.dust_timer = 0.09;
            }
            should_chip = state.progress >= 1.0;
            self.drill_flash_seconds = 0.09;
            let chipped = state.initial_durability.saturating_sub(tile.durability);
            let total_progress = ((f32::from(chipped) + state.progress.min(1.0))
                / f32::from(state.initial_durability.max(1)))
            .clamp(0.0, 1.0);
            self.message = format!(
                "Drilling {}... {:.0}%",
                tile.kind.name(),
                total_progress * 100.0
            );
        }
        if should_spawn_dust {
            self.spawn_dust();
        }

        if should_chip {
            if let Some(state) = &mut self.active_drill {
                state.progress -= 1.0;
            }
            let mine_result = self.terrain.chip(target);
            if !matches!(mine_result, MineResult::Blocked | MineResult::TooDangerous) {
                self.mark_tile_visual_changed(target);
            }
            match mine_result {
                MineResult::Blocked => self.active_drill = None,
                MineResult::TooDangerous => {
                    self.active_drill = None;
                    self.player.hull = (self.player.hull - 8.0).max(0.0);
                    self.sound_cues.push(SoundCue::Damage);
                    self.screen_flash_seconds = 0.1;
                    let warning = if self
                        .terrain
                        .tile(target)
                        .is_some_and(|tile| tile.kind == TileKind::MagmaVent)
                    {
                        "Magma vent! Hull scorched and heat rising."
                    } else {
                        "Lava pocket! Hull scorched."
                    };
                    warning.clone_into(&mut self.message);
                }
                MineResult::Exploded => {
                    self.active_drill = None;
                    self.trigger_gas_explosion();
                }
                MineResult::Blast => {
                    self.active_drill = None;
                    self.trigger_explosive_pocket();
                }
                MineResult::Chipped => {}
                MineResult::Mined(mined) => {
                    self.active_drill = None;
                    self.collect_mined_tile(mined, target);
                }
            }
        }
    }

    fn creep_into_drill(&mut self, direction: DrillDirection, delta_seconds: f32) {
        let creep = 32.0 * delta_seconds;
        match direction {
            DrillDirection::Down => self.move_axis(0.0, creep),
            DrillDirection::Left => self.move_axis(-creep * 0.65, 0.0),
            DrillDirection::Right => self.move_axis(creep * 0.65, 0.0),
        }
    }

    fn trigger_gas_explosion(&mut self) {
        let protected = self.is_pump_protected(self.player.tile_position(TILE_SIZE));
        self.player.fuel = (self.player.fuel - DRILL_FUEL_COST).max(0.0);
        self.player.velocity_x *= -0.25;
        self.player.velocity_y = -90.0;
        self.sound_cues.push(SoundCue::Damage);
        self.drill_flash_seconds = 0.2;
        self.screen_flash_seconds = 0.12;
        if !protected {
            self.hazard_clouds.push(HazardCloud {
                x: self.player.x,
                y: self.player.y + TILE_SIZE,
                life: 8.0,
                radius: 10.0,
            });
        }
        for _ in 0..5 {
            self.spawn_dust();
        }
        if protected {
            "Pump station vented nearby gas before it became corrosive."
                .clone_into(&mut self.message);
        } else {
            "Gas pocket venting! Clear the green leak before it turns corrosive."
                .clone_into(&mut self.message);
        }
    }

    fn trigger_explosive_pocket(&mut self) {
        self.player.fuel = (self.player.fuel - DRILL_FUEL_COST * 2.0).max(0.0);
        self.player.hull = (self.player.hull - 24.0).max(0.0);
        self.player.velocity_x *= -0.7;
        self.player.velocity_y = -260.0;
        self.sound_cues.push(SoundCue::Damage);
        self.drill_flash_seconds = 0.35;
        self.screen_flash_seconds = 0.22;
        self.shake_camera(0.45, 14.0);
        for _ in 0..12 {
            self.spawn_dust();
            self.spawn_sparks();
        }
        "Explosive pocket detonated! Hull damaged and tunnel destabilized."
            .clone_into(&mut self.message);
        self.spawn_cave_in();
    }

    fn collect_mined_tile(&mut self, mined: TileKind, target: TilePosition) {
        self.scan_markers.retain(|marker| marker.position != target);
        self.player.fuel -= DRILL_FUEL_COST;
        self.sound_cues.push(SoundCue::Drill);
        self.spawn_dust();
        self.drill_flash_seconds = 0.12;

        self.collection_log.discover_tile(mined);

        if let TileKind::Ore(mineral) = mined {
            if self.add_mined_cargo(mineral) {
                self.message = format!("Loaded {} ore worth {}.", mineral.name(), mineral.value());
                if self.deep_claim_status == DeepClaimStatus::Unlocked
                    && target.y >= 70
                    && let Some(material) = deep_claim_material_for(mineral, target)
                {
                    self.player.add_material(material, 1);
                    let _ = write!(self.message, " Found {}.", material.name());
                }
            } else {
                "Cargo full. Return to depot to sell.".clone_into(&mut self.message);
            }
        } else if let TileKind::Artifact(artifact) = mined {
            if self.add_mined_artifact(artifact) {
                self.artifacts_found += 1;
                if artifact == ArtifactKind::StarCore {
                    self.escape_sequence_seconds = 120.0;
                    self.shake_camera(1.0, 10.0);
                    "Star Core extracted! Core fracture cascade started: return to HQ before the mine collapses."
                        .clone_into(&mut self.message);
                } else {
                    self.message = format!(
                        "Recovered {} artifact worth {}.",
                        artifact.name(),
                        artifact.value()
                    );
                }
            } else {
                "Cargo full. Return to depot to sell.".clone_into(&mut self.message);
            }
        } else if mined == TileKind::PressurePocket {
            self.player.velocity_y = -360.0;
            self.player.velocity_x *= 1.4;
            self.player.hull = (self.player.hull - 10.0).max(0.0);
            self.shake_camera(0.3, 9.0);
            self.sound_cues.push(SoundCue::Damage);
            "Pressure pocket ruptured! The blast shoved the rig upward."
                .clone_into(&mut self.message);
        } else {
            "Tunnel opened.".clone_into(&mut self.message);
        }
        if matches!(mined, TileKind::Stone | TileKind::HardRock)
            && falling_rock_roll(target, self.terrain.seed())
        {
            self.falling_boulders.push(FallingBoulder {
                x: target.x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
                y: (target.y as f32 - 1.0) * TILE_SIZE,
                velocity_y: 0.0,
                warning_seconds: BOULDER_WARNING_SECONDS,
                life: 3.6,
            });
            self.sound_cues.push(SoundCue::Damage);
            self.shake_camera(0.18, 4.0);
            self.message.push_str(" Unstable rock falling!");
        }
    }

    fn spawn_cave_in(&mut self) {
        for offset in -1_i32..=1 {
            self.falling_boulders.push(FallingBoulder {
                x: self.player.x + offset as f32 * TILE_SIZE,
                y: self.player.y - TILE_SIZE * 2.0,
                velocity_y: 0.0,
                warning_seconds: 0.45 + offset.unsigned_abs() as f32 * 0.15,
                life: 4.0,
            });
        }
    }

    fn update_service_animation(&mut self, delta_seconds: f32) {
        self.service_animation_seconds = (self.service_animation_seconds - delta_seconds).max(0.0);
        if self.service_animation_seconds == 0.0 {
            self.service_animation = None;
        }
    }

    fn update_placed_bombs(&mut self, delta_seconds: f32) {
        let mut detonations = Vec::new();
        for bomb in &mut self.placed_bombs {
            bomb.timer_seconds -= delta_seconds;
            if bomb.timer_seconds <= 0.0 {
                detonations.push(TilePosition {
                    x: (bomb.x / TILE_SIZE).floor() as i32,
                    y: (bomb.y / TILE_SIZE).floor() as i32,
                });
            }
        }
        self.placed_bombs.retain(|bomb| bomb.timer_seconds > 0.0);
        for center in detonations {
            let radius = if self.town_development.explosives_shack_level >= 4 {
                3
            } else {
                2
            };
            self.detonate_bomb(center, radius);
        }
    }

    fn detonate_bomb(&mut self, center: TilePosition, radius: i32) {
        let blast = self.terrain.blast_radius(center, radius);
        self.mark_tiles_visual_changed(blast.changed_tiles);
        let cleared = blast.cleared;
        self.sound_cues.push(SoundCue::Explosion);
        self.screen_flash_seconds = self.screen_flash_seconds.max(0.22);
        self.shake_camera(0.45, 13.0);
        for _ in 0..14 {
            self.spawn_dust();
            self.spawn_sparks();
        }
        let distance = ((self.player.x / TILE_SIZE - center.x as f32).abs()
            + (self.player.y / TILE_SIZE - center.y as f32).abs())
        .max(0.0);
        if distance <= radius as f32 + 1.0 {
            self.player.hull = (self.player.hull - 22.0).max(0.0);
        }
        if self.town_development.explosives_shack_level >= 5 {
            return;
        }
        self.chain_react_near(center, radius + 2);
        self.message =
            format!("Bomb detonated. Cleared {cleared} tiles and rattled nearby pockets.");
        self.reveal_near_player();
    }

    fn chain_react_near(&mut self, center: TilePosition, radius: i32) {
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if (x - center.x).abs() + (y - center.y).abs() > radius {
                    continue;
                }
                let position = TilePosition { x, y };
                if matches!(
                    self.terrain.tile(position).map(|tile| tile.kind),
                    Some(TileKind::Gas | TileKind::ExplosivePocket | TileKind::PressurePocket)
                ) {
                    let blast = self.terrain.blast_radius(position, 1);
                    self.mark_tiles_visual_changed(blast.changed_tiles);
                    self.hazard_clouds.push(HazardCloud {
                        x: x as f32 * TILE_SIZE,
                        y: y as f32 * TILE_SIZE,
                        life: 6.0,
                        radius: 18.0,
                    });
                }
            }
        }
    }

    fn update_particles(&mut self, delta_seconds: f32) {
        for particle in &mut self.dust_particles {
            particle.life -= delta_seconds;
            particle.y -= 18.0 * delta_seconds;
        }
        self.dust_particles.retain(|particle| particle.life > 0.0);
        for spark in &mut self.spark_particles {
            spark.life -= delta_seconds;
            spark.x += spark.velocity_x * delta_seconds;
            spark.y += spark.velocity_y * delta_seconds;
            spark.velocity_y += 180.0 * delta_seconds;
        }
        self.spark_particles.retain(|particle| particle.life > 0.0);
    }

    fn update_boulders(&mut self, delta_seconds: f32) {
        for boulder in &mut self.falling_boulders {
            boulder.life -= delta_seconds;
            if boulder.warning_seconds > 0.0 {
                boulder.warning_seconds -= delta_seconds;
                continue;
            }
            boulder.velocity_y = (boulder.velocity_y + GRAVITY * 0.8 * delta_seconds).min(520.0);
            boulder.y += boulder.velocity_y * delta_seconds;
        }

        let mut hit_player = false;
        self.falling_boulders.retain(|boulder| {
            if boulder.warning_seconds > 0.0 {
                return true;
            }
            let dx = self.player.x - boulder.x;
            let dy = self.player.y - boulder.y;
            let hit = dx.hypot(dy) <= PLAYER_RADIUS + 8.0;
            hit_player |= hit;
            !hit
        });
        if hit_player {
            self.player.hull = (self.player.hull - BOULDER_DAMAGE).max(0.0);
            self.sound_cues.push(SoundCue::Damage);
            self.shake_camera(0.35, 9.0);
            self.spawn_sparks();
            "Falling boulder slammed the rig!".clone_into(&mut self.message);
        }

        self.falling_boulders.retain(|boulder| {
            boulder.life > 0.0
                && boulder.y < (self.terrain.height() as f32 - 1.0) * TILE_SIZE
                && !self.terrain.is_solid_at(TilePosition {
                    x: (boulder.x / TILE_SIZE).floor() as i32,
                    y: (boulder.y / TILE_SIZE).floor() as i32,
                })
        });
    }

    fn update_hazards(&mut self, delta_seconds: f32) {
        for cloud in &mut self.hazard_clouds {
            cloud.life -= delta_seconds;
            cloud.radius += 8.0 * delta_seconds;
        }
        self.hazard_clouds.retain(|cloud| cloud.life > 0.0);

        let in_gas = self.hazard_clouds.iter().any(|cloud| {
            if cloud.life >= 6.0 {
                return false;
            }
            let dx = self.player.x - cloud.x;
            let dy = self.player.y - cloud.y;
            dx.hypot(dy) <= cloud.radius
        });
        if in_gas {
            self.player.hull = (self.player.hull - 4.0 * delta_seconds).max(0.0);
            "Corrosive gas cloud eating hull plating!".clone_into(&mut self.message);
        }
    }

    #[allow(
        clippy::missing_const_for_fn,
        reason = "uses f32 max for camera shake state"
    )]
    fn shake_camera(&mut self, seconds: f32, strength: f32) {
        self.camera_shake_seconds = self.camera_shake_seconds.max(seconds);
        self.camera_shake_strength = self.camera_shake_strength.max(strength);
    }

    fn spawn_sparks(&mut self) {
        for index in 0..8 {
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            self.spark_particles.push(SparkParticle {
                x: self.player.x + side * 8.0,
                y: self.player.y,
                velocity_x: side * (45.0 + index as f32 * 8.0),
                velocity_y: -80.0 + index as f32 * 12.0,
                life: 0.45,
            });
        }
    }

    fn spawn_dust(&mut self) {
        let base_x = self.player.x;
        let base_y = self.player.y + 18.0;
        self.dust_particles.push(DustParticle {
            x: base_x - 7.0,
            y: base_y,
            life: 0.35,
        });
        self.dust_particles.push(DustParticle {
            x: base_x + 7.0,
            y: base_y + 2.0,
            life: 0.28,
        });
    }

    fn update_depth_milestones(&mut self) {
        let current_tile = (self.player.y / TILE_SIZE).floor() as i32;
        self.deepest_tile_reached = self.deepest_tile_reached.max(current_tile);
        if current_tile > 6 {
            self.trip_best_depth = self.trip_best_depth.max(current_tile);
        }
        if self.deepest_tile_reached < self.next_milestone_tile {
            return;
        }

        let reward = u32::try_from(self.next_milestone_tile).unwrap_or(0) * 2;
        self.player.credits += reward;
        self.total_earnings += reward;
        self.sound_cues.push(SoundCue::Milestone);
        let unlock = match self.next_milestone_tile {
            20 => "Silver seams now appear in useful quantities.",
            40 => "Gold and relic pockets are becoming common.",
            60 => "Emerald, ruby, and heat hazards intensify below.",
            _ => "Diamond traces and Star Core readings strengthen below.",
        };
        self.message = format!(
            "Depth milestone reached: {}m. Survey bonus: {reward} credits. {unlock}",
            self.next_milestone_tile - 5
        );
        self.next_milestone_tile += 20;
    }

    fn apply_depth_pressure(&mut self, delta_seconds: f32) {
        let safe_depth = HEAT_START_DEPTH
            + f32::from(self.player.radiator_level.saturating_sub(1)) * 12.0 * TILE_SIZE;
        if self.player.y <= safe_depth {
            return;
        }

        let depth_factor = ((self.player.y - safe_depth) / (12.0 * TILE_SIZE)).max(1.0);
        let mut heat_multiplier =
            1.0 + self.world_event_severity(WorldEventKind::HeatWave) as f32 * 0.18;
        if self.has_equipped_part(RigPartKind::ThermalHull) {
            heat_multiplier *= 0.55;
        }
        if self.has_equipped_part(RigPartKind::ImpactHull) {
            heat_multiplier *= 1.18;
        }
        if self.has_equipped_part(RigPartKind::PressureHull) && self.player.y >= 90.0 * TILE_SIZE {
            heat_multiplier *= 0.72;
        }
        let damage = HEAT_DAMAGE_PER_SECOND * depth_factor * heat_multiplier * delta_seconds;
        self.player.hull = (self.player.hull - damage).max(0.0);
        "Depth pressure overheating hull. Upgrade radiator.".clone_into(&mut self.message);
    }

    fn apply_lava_damage(&mut self, delta_seconds: f32) {
        let near_lava = (-2..=2).any(|dy| {
            (-2..=2).any(|dx| {
                let position = TilePosition {
                    x: (self.player.x / TILE_SIZE) as i32 + dx,
                    y: (self.player.y / TILE_SIZE) as i32 + dy,
                };
                self.terrain.is_lava_at(position)
            })
        });
        if !near_lava {
            return;
        }

        let player_tile = self.player.tile_position(TILE_SIZE);
        let mut heat_multiplier =
            1.0 + self.world_event_severity(WorldEventKind::HeatWave) as f32 * 0.18;
        if self.has_equipped_part(RigPartKind::ThermalHull) {
            heat_multiplier *= 0.55;
        }
        if self.has_equipped_part(RigPartKind::ImpactHull) {
            heat_multiplier *= 1.18;
        }
        if self.has_equipped_part(RigPartKind::PressureHull) && self.player.y >= 90.0 * TILE_SIZE {
            heat_multiplier *= 0.72;
        }
        let damage = if self.is_pump_protected(player_tile) {
            2.5 * heat_multiplier * delta_seconds
        } else {
            9.0 * heat_multiplier * delta_seconds
        };
        self.player.hull = (self.player.hull - damage).max(0.0);
        self.sound_cues.push(SoundCue::Damage);
        "Lava heat is burning the hull!".clone_into(&mut self.message);
    }

    fn update_camera(&mut self, delta_seconds: f32) {
        let (target_x, target_y) = target_camera_offset(self);
        if self.camera_intro_seconds > 0.0 {
            self.camera_intro_seconds = (self.camera_intro_seconds - delta_seconds).max(0.0);
            let progress = 1.0 - self.camera_intro_seconds / CAMERA_INTRO_SECONDS;
            let eased = 1.0 - (1.0 - progress).powi(3);
            self.camera_x = target_x;
            self.camera_y = target_y - CAMERA_INTRO_DROP_DISTANCE * (1.0 - eased);
            return;
        }

        let blend = (delta_seconds * CAMERA_SMOOTHING).clamp(0.0, 1.0);
        self.camera_x += (target_x - self.camera_x) * blend;
        self.camera_y += (target_y - self.camera_y) * blend;
    }

    fn update_warning_messages(&mut self) {
        let low_fuel = self.player.fuel <= self.player.fuel_capacity * 0.18;
        let low_hull = self.player.hull <= self.player.max_hull() * 0.25;
        match (low_fuel, low_hull) {
            (true, true) => "CRITICAL: low fuel and damaged hull. Return to surface!"
                .clone_into(&mut self.message),
            (true, false) => "Warning: fuel reserves low.".clone_into(&mut self.message),
            (false, true) => "Warning: hull integrity low.".clone_into(&mut self.message),
            (false, false) => {}
        }
    }

    fn update_deep_run_pressure(&mut self, delta_seconds: f32) {
        let depth = self.player.tile_position(TILE_SIZE).y;
        if depth < 80 || self.current_zone.is_some() {
            self.trip_seconds = 0.0;
            self.deep_instability = (self.deep_instability - delta_seconds * 2.0).max(0.0);
            return;
        }
        self.trip_seconds += delta_seconds;
        let rare_cargo = self.rare_cargo_count() as f32;
        self.deep_instability += delta_seconds
            * ((depth - 70) as f32 * 0.006 + self.trip_seconds * 0.0006 + rare_cargo * 0.02);
        if depth >= 105 {
            self.player.fuel = (self.player.fuel - delta_seconds * 0.25).max(0.0);
            self.scanner_cooldown_seconds = self.scanner_cooldown_seconds.max(1.2);
        }
        if rare_cargo > 0.0 && self.update_ticks.is_multiple_of(240) {
            self.hazard_clouds.push(HazardCloud {
                x: self.player.x,
                y: self.player.y + TILE_SIZE,
                life: 5.0,
                radius: 10.0 + rare_cargo.min(4.0),
            });
            "Rare cargo is attracting unstable deep vapors.".clone_into(&mut self.message);
        }
        if self.deep_instability >= 100.0 {
            self.deep_instability = 45.0;
            self.spawn_cave_in();
            self.player.hull = (self.player.hull - 4.0).max(0.0);
            "Claim instability spiked: cave-in and hull stress detected."
                .clone_into(&mut self.message);
        }
        self.maybe_spawn_deep_reward(depth);
    }

    fn rare_cargo_count(&self) -> u32 {
        let mineral_count: u32 = self
            .player
            .cargo
            .iter()
            .filter(|(mineral, _)| {
                matches!(
                    mineral,
                    MineralKind::Diamond
                        | MineralKind::Platinum
                        | MineralKind::Uranium
                        | MineralKind::Mythril
                )
            })
            .map(|(_, count)| *count)
            .sum();
        let artifact_count: u32 = self.player.artifacts.values().copied().sum();
        mineral_count + artifact_count
    }

    fn maybe_spawn_deep_reward(&mut self, depth: i32) {
        let milestone = depth / 20;
        if depth < 90 || milestone <= self.deep_reward_milestone {
            return;
        }
        self.deep_reward_milestone = milestone;
        let position = TilePosition {
            x: self
                .player
                .tile_position(TILE_SIZE)
                .x
                .saturating_add(2)
                .clamp(1, self.terrain.width() - 2),
            y: depth.clamp(10, self.terrain.height() - 2),
        };
        let reward_tile = if depth >= 120 {
            let material = match depth {
                180.. => StrategicResourceKind::RadiantFossil,
                160..=179 => StrategicResourceKind::VoidGlass,
                140..=159 => StrategicResourceKind::MachineRelic,
                _ => StrategicResourceKind::PressurePearl,
            };
            self.player.add_material(material, 1);
            self.player
                .add_material(StrategicResourceKind::CoreShard, 1);
            self.award_legendary_blueprint(depth);
            TileKind::Artifact(ArtifactKind::OldCircuit)
        } else {
            TileKind::Ore(MineralKind::Mythril)
        };
        if self.terrain.set_kind(position, reward_tile) {
            self.mark_tile_visual_changed(position);
        }
        self.message = if depth >= 120 {
            "Deep reward signal: unique relic exposed and Core Shard recovered.".to_owned()
        } else {
            "Deep reward signal: huge mythril vein migrated nearby.".to_owned()
        };
    }

    fn award_legendary_blueprint(&mut self, depth: i32) {
        let blueprint = if depth >= 160 {
            LegendaryBlueprint::RelicSorter
        } else if depth >= 140 {
            LegendaryBlueprint::VoidTank
        } else {
            LegendaryBlueprint::StarPlating
        };
        if !self.legendary_blueprints.insert(blueprint) {
            return;
        }
        match blueprint {
            LegendaryBlueprint::StarPlating => {
                self.rig_part_inventory.insert(RigPartKind::PressureHull);
                self.equipped_rig_parts
                    .insert(RigSlot::HullPlating, RigPartKind::PressureHull);
                self.player.crafted_bulkheads = self.player.crafted_bulkheads.saturating_add(1);
                self.player.hull = self.player.max_hull();
            }
            LegendaryBlueprint::VoidTank => {
                self.rig_part_inventory.insert(RigPartKind::HaulerEngine);
                self.equipped_rig_parts
                    .insert(RigSlot::Engine, RigPartKind::HaulerEngine);
                self.player.fuel_capacity += 20.0;
                self.player.fuel = self.player.fuel_capacity;
            }
            LegendaryBlueprint::RelicSorter => {
                self.rig_part_inventory.insert(RigPartKind::RelicScanner);
                self.rig_part_inventory.insert(RigPartKind::CargoBalloon);
                self.equipped_rig_parts
                    .insert(RigSlot::ScannerModule, RigPartKind::RelicScanner);
                self.equipped_rig_parts
                    .insert(RigSlot::CargoModule, RigPartKind::CargoBalloon);
                self.player.cargo_capacity = self.player.cargo_capacity.saturating_add(4);
            }
        }
        self.message = format!(
            "Legendary blueprint recovered: {} special rig part installed.",
            blueprint.title()
        );
        self.sound_cues.push(SoundCue::Milestone);
    }

    fn update_escape_sequence(&mut self, delta_seconds: f32) {
        if self.escape_sequence_seconds <= 0.0 || self.won_game {
            return;
        }
        self.escape_sequence_seconds = (self.escape_sequence_seconds - delta_seconds).max(0.0);
        if self.current_zone == Some(SurfaceZone::Headquarters) {
            self.escape_sequence_seconds = 0.0;
            return;
        }
        if self.update_ticks.is_multiple_of(45) {
            self.spawn_cave_in();
            self.shake_camera(
                0.25,
                6.0 + (120.0 - self.escape_sequence_seconds).max(0.0) * 0.05,
            );
        }
        if self.update_ticks.is_multiple_of(90) {
            self.seal_escape_tunnel();
        } else if self.update_ticks % 90 == 75 {
            self.warn_escape_tunnel_collapse();
        }
        if self.update_ticks.is_multiple_of(120) {
            "CORE CASCADE: tunnels are sealing. Climb now!".clone_into(&mut self.message);
        }
        if self.escape_sequence_seconds == 0.0 {
            self.player.hull = 0.0;
            self.game_over = true;
            "The mine collapsed around the Star Core. Emergency rescue required."
                .clone_into(&mut self.message);
        }
    }

    fn warn_escape_tunnel_collapse(&mut self) {
        self.collapse_warnings.clear();
        let px = (self.player.x / TILE_SIZE).floor() as i32;
        let py = (self.player.y / TILE_SIZE).floor() as i32 + 5;
        for dx in -2..=2 {
            let position = TilePosition { x: px + dx, y: py };
            if position.y > 7
                && !self.terrain.is_solid_at(position)
                && !self.is_tunnel_supported(position)
            {
                self.collapse_warnings.push(position);
            }
        }
        if !self.collapse_warnings.is_empty() {
            "Ceiling stress warning: marked tunnel will seal next.".clone_into(&mut self.message);
        }
    }

    fn seal_escape_tunnel(&mut self) {
        let px = (self.player.x / TILE_SIZE).floor() as i32;
        let py = (self.player.y / TILE_SIZE).floor() as i32 + 5;
        for dx in -2..=2 {
            let position = TilePosition { x: px + dx, y: py };
            if position.y <= 7
                || self.terrain.is_solid_at(position)
                || self.is_tunnel_supported(position)
            {
                continue;
            }
            if self.terrain.set_kind(position, TileKind::Stone) {
                self.mark_tile_visual_changed(position);
                self.scan_markers
                    .retain(|marker| marker.position != position);
            }
        }
        self.collapse_warnings.clear();
    }

    fn update_layer_band(&mut self) {
        let band = self.deepest_tile_reached / 20;
        if band <= self.current_layer_band {
            return;
        }
        self.current_layer_band = band;
        self.collection_log.strata.insert(band);
        let layer = match band {
            1 => "Clay Belt",
            2 => "Silver Caverns",
            3 => "Thermal Strata",
            _ => "Core Fracture Zone",
        };
        self.message = format!("Entering {layer}. Hazards and ore density increased.");
    }

    fn award_challenge_badges(&mut self, cargo_value: u32, risk_rating: &str) {
        let mut unlocked = Vec::new();
        if self.trip_best_depth >= 100 && self.challenge_badges.insert(ChallengeBadge::DeepReturn) {
            self.cosmetic_skins.insert(CosmeticRigSkin::BronzeTrim);
            unlocked.push(ChallengeBadge::DeepReturn.title());
        }
        if matches!(risk_rating, "High" | "Extreme")
            && self.challenge_badges.insert(ChallengeBadge::HighRiskReturn)
        {
            self.cosmetic_skins.insert(CosmeticRigSkin::HazardStripes);
            unlocked.push(ChallengeBadge::HighRiskReturn.title());
        }
        if cargo_value >= 1_500 && self.challenge_badges.insert(ChallengeBadge::ValuableHaul) {
            self.cosmetic_skins.insert(CosmeticRigSkin::StarChrome);
            unlocked.push(ChallengeBadge::ValuableHaul.title());
        }
        if !unlocked.is_empty() {
            self.last_run_summary.push_str(" Badges: ");
            self.last_run_summary.push_str(&unlocked.join(", "));
            self.last_run_summary.push('.');
            self.message = self.last_run_summary.clone();
            self.sound_cues.push(SoundCue::Milestone);
        }
    }

    fn current_cargo_value(&self) -> u32 {
        let minerals = self
            .player
            .cargo
            .iter()
            .map(|(mineral, count)| self.mineral_market_value(*mineral).saturating_mul(*count))
            .sum::<u32>();
        let artifacts = self
            .player
            .artifacts
            .iter()
            .map(|(artifact, count)| artifact.value().saturating_mul(*count))
            .sum::<u32>();
        minerals.saturating_add(artifacts)
    }

    fn award_return_bonus(&mut self) {
        if self.current_zone.is_none() || self.trip_best_depth < 15 {
            return;
        }
        self.return_streak += 1;
        if self.player.loan_debt > 0 {
            let risk_interest = 12 + self.player.loan_debt / 200;
            self.player.loan_debt = self.player.loan_debt.saturating_add(risk_interest);
        }
        let depth = u32::try_from(self.trip_best_depth).unwrap_or(0);
        let cargo_value = self.current_cargo_value();
        self.best_return_depth = self.best_return_depth.max(self.trip_best_depth);
        self.most_valuable_cargo_run = self.most_valuable_cargo_run.max(cargo_value);
        let reward = (depth / 4).saturating_mul(self.return_streak.min(5));
        self.player.credits += reward;
        self.total_earnings += reward;
        let risk_rating = if self.deep_instability >= 75.0 {
            "Extreme"
        } else if self.deep_instability >= 40.0 {
            "High"
        } else if self.trip_best_depth >= 80 {
            "Moderate"
        } else {
            "Low"
        };
        self.last_run_summary = format!(
            "Run summary: depth {}m | return bonus {reward} | cargo value {cargo_value} | hull damage {:.0}% | discoveries {} | risk {risk_rating} | streak x{}.",
            self.trip_best_depth,
            (1.0 - self.player.hull / self.player.max_hull()).max(0.0) * 100.0,
            self.collection_log.minerals.len() + self.collection_log.artifacts.len(),
            self.return_streak
        );
        self.message = self.last_run_summary.clone();
        self.award_challenge_badges(cargo_value, risk_rating);
        self.trip_best_depth = 0;
        self.trip_seconds = 0.0;
        self.deep_instability = 0.0;
        self.deep_reward_milestone = 0;
        self.advance_town_event();
    }

    fn tick_world_events(&mut self) {
        for event in &mut self.active_world_events {
            event.days_remaining = event.days_remaining.saturating_sub(1);
        }
        self.active_world_events
            .retain(|event| event.days_remaining > 0);
    }

    fn schedule_world_event(&mut self) {
        if !self.town_event_day.is_multiple_of(3) {
            return;
        }
        let kind = match (self.market_salt + self.town_event_day) % 13 {
            0 => WorldEventKind::MarketCrash,
            1 => WorldEventKind::MarketBoom,
            2 => WorldEventKind::RareBuyer,
            3 => WorldEventKind::FuelShortage,
            4 => WorldEventKind::RepairBacklog,
            5 => WorldEventKind::HeatWave,
            6 => WorldEventKind::CollapseSurge,
            7 => WorldEventKind::DeepPressureStorm,
            8 => WorldEventKind::GasBloom,
            9 => WorldEventKind::Earthquake,
            10 => WorldEventKind::MeteorShower,
            11 => WorldEventKind::RivalClaims,
            _ => WorldEventKind::AncientMachine,
        };
        if self.has_world_event(kind) {
            return;
        }
        let severity = 1 + (self.town_event_day + self.market_salt) % 3;
        let days_remaining = 2 + severity.min(2);
        self.active_world_events.push(ActiveWorldEvent {
            kind,
            days_remaining,
            severity,
        });
        self.apply_world_event_start(kind, severity);
        self.message = format!(
            "World event: {} severity {severity} for {days_remaining} days.",
            kind.label()
        );
    }

    fn apply_world_event_start(&mut self, kind: WorldEventKind, severity: u32) {
        match kind {
            WorldEventKind::Earthquake => {
                self.spawn_cave_in();
                self.apply_seismic_pump_strain();
                self.migrate_ore_veins(severity);
                self.shake_camera(0.6, 12.0 + severity as f32 * 4.0);
            }
            WorldEventKind::GasBloom => {
                self.hazard_clouds.push(HazardCloud {
                    x: self.player.x,
                    y: self.player.y + TILE_SIZE,
                    life: 8.0 + severity as f32,
                    radius: 12.0 + severity as f32 * 2.0,
                });
            }
            WorldEventKind::MeteorShower => {
                self.queue_start_side_contract();
                self.contracts.active.reward = self.contracts.active.reward.saturating_add(75);
            }
            WorldEventKind::RivalClaims => {
                self.expedition_offers.clear();
                self.refresh_expedition_offers();
            }
            WorldEventKind::AncientMachine => self.awaken_ancient_machine(severity),
            WorldEventKind::MarketCrash
            | WorldEventKind::MarketBoom
            | WorldEventKind::RareBuyer
            | WorldEventKind::FuelShortage
            | WorldEventKind::RepairBacklog
            | WorldEventKind::HeatWave
            | WorldEventKind::CollapseSurge
            | WorldEventKind::DeepPressureStorm => {}
        }
    }

    fn migrate_ore_veins(&mut self, severity: u32) {
        let center_x = (self.player.x / TILE_SIZE).floor() as i32;
        let base_y = (self.deepest_tile_reached + 8).clamp(12, self.terrain.height() - 2);
        let minerals = [MineralKind::Gold, MineralKind::Ruby, MineralKind::Platinum];
        let mut changed = 0_u32;
        for index in 0..(3 + severity) {
            let position = TilePosition {
                x: (center_x + i32::try_from(index).unwrap_or(0) * 3 - 5)
                    .clamp(1, self.terrain.width() - 2),
                y: (base_y + i32::try_from(index % 3).unwrap_or(0) - 1)
                    .clamp(8, self.terrain.height() - 2),
            };
            if !self.terrain.is_solid_at(position) {
                continue;
            }
            let mineral = minerals[usize::try_from(index).unwrap_or(0) % minerals.len()];
            if self.terrain.set_kind(position, TileKind::Ore(mineral)) {
                self.mark_tile_visual_changed(position);
                changed += 1;
            }
        }
        if changed > 0 {
            self.message = format!("Earthquake exposed {changed} shifted ore vein(s).");
        }
    }

    fn awaken_ancient_machine(&mut self, severity: u32) {
        self.player
            .add_material(StrategicResourceKind::CoreShard, severity.max(1));
        self.scanner_cooldown_seconds = self.scanner_cooldown_seconds.max(6.0);
        self.sound_cues.push(SoundCue::Milestone);
        self.shake_camera(0.35, 8.0 + severity as f32 * 2.0);
        self.message = format!(
            "Ancient machine awakened: recovered {} Core Shard(s), scanner interference rising.",
            severity.max(1)
        );
    }

    fn advance_town_event(&mut self) {
        self.town_event_day = self.town_event_day.saturating_add(1);
        self.tick_world_events();
        self.schedule_world_event();
        self.town_event = match self.town_event_day % 5 {
            0 => "Fuel sale: mechanics whisper about cheaper surface fuel.".to_owned(),
            1 => "Gold boom: depot buyers are bidding aggressively.".to_owned(),
            2 => "Repair backlog: Iona says don't dent anything expensive today.".to_owned(),
            3 => "Cave instability warning: HQ predicts more falling rock.".to_owned(),
            _ => "Explosive Shack overstock: Nix is pushing bomb bundles.".to_owned(),
        };
        self.market_history
            .push(market_factor(self.market_salt, self.town_event_day));
        if self.market_history.len() > 7 {
            self.market_history.remove(0);
        }
        if self.town_development.bank_level > 0 {
            let dividend = u32::from(self.town_development.bank_level) * 12;
            self.player.credits = self.player.credits.saturating_add(dividend);
            self.total_earnings = self.total_earnings.saturating_add(dividend);
            if self.player.loan_debt > 0 {
                self.message = format!(
                    "Bank office dividend: {dividend} credits. Debt remaining: {}.",
                    self.player.loan_debt
                );
            }
        }
        self.apply_infrastructure_maintenance();
        self.apply_seismic_pump_strain();
        for mineral in all_minerals() {
            let history = self.mineral_market_history.entry(mineral).or_default();
            history.push(market_factor_for(
                self.market_salt,
                self.town_event_day,
                mineral,
            ));
            if history.len() > 7 {
                history.remove(0);
            }
        }
    }

    fn apply_seismic_pump_strain(&mut self) {
        if self.town_event_day % 5 != 3 {
            return;
        }
        let mut damaged = 0_u32;
        let mut failed = 0_u32;
        for item in &mut self.infrastructure {
            if item.kind != InfrastructureKind::PumpStation {
                continue;
            }
            damaged += 1;
            let before = item.durability;
            item.durability = item.durability.saturating_sub(35);
            if before > 0 && item.durability == 0 {
                failed += 1;
            }
        }
        if damaged == 0 {
            return;
        }
        self.infrastructure.retain(|item| item.durability > 0);
        self.message = if failed == 0 {
            format!("Seismic tremor strained {damaged} pump station(s).")
        } else {
            format!("Seismic tremor strained {damaged} pump station(s); {failed} failed.")
        };
    }

    fn apply_infrastructure_maintenance(&mut self) {
        let cost = self.infrastructure.len() as u32 * 3;
        if cost == 0 {
            return;
        }
        let paid = self.player.credits.min(cost);
        self.player.credits -= paid;
        if paid < cost {
            let lost = self
                .infrastructure
                .len()
                .saturating_sub((paid / 3) as usize);
            for _ in 0..lost {
                self.infrastructure.pop();
            }
            self.message = format!(
                "Infrastructure maintenance shortfall: paid {paid}/{cost} credits, {lost} relays failed."
            );
        } else {
            self.message = format!("Paid {cost} credits to maintain signal relay network.");
        }
        self.apply_infrastructure_wear();
    }

    fn apply_infrastructure_wear(&mut self) {
        let hazard_positions = self
            .infrastructure
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let near_hazard = (-2..=2).any(|dy| {
                    (-2..=2).any(|dx| {
                        self.terrain
                            .tile(TilePosition {
                                x: item.position.x + dx,
                                y: item.position.y + dy,
                            })
                            .is_some_and(|tile| {
                                matches!(
                                    tile.kind,
                                    TileKind::Gas
                                        | TileKind::Lava
                                        | TileKind::MagmaVent
                                        | TileKind::ExplosivePocket
                                        | TileKind::PressurePocket
                                )
                            })
                    })
                });
                near_hazard.then_some(index)
            })
            .collect::<Vec<_>>();
        for index in hazard_positions {
            if let Some(item) = self.infrastructure.get_mut(index) {
                item.durability = item.durability.saturating_sub(20);
            }
        }
        let before = self.infrastructure.len();
        self.infrastructure.retain(|item| item.durability > 0);
        let lost = before.saturating_sub(self.infrastructure.len());
        if lost > 0 {
            self.message = format!("{lost} infrastructure unit(s) failed in dangerous ground.");
        }
    }

    fn update_status_messages(&mut self) {
        if self.message.starts_with("Warning:") || self.message.starts_with("CRITICAL:") {
            return;
        }
        if self.player.fuel <= self.player.fuel_capacity * 0.15 && self.player.y > 6.0 * TILE_SIZE {
            "CRITICAL: fuel reserve low. Return to the fuel station now."
                .clone_into(&mut self.message);
            return;
        }
        if self.player.cargo_used() >= self.player.cargo_capacity {
            "Warning: cargo hold full. Return to the depot or leave valuables behind."
                .clone_into(&mut self.message);
            return;
        }
        if let Some(zone) = self.current_zone {
            self.message = match zone {
                SurfaceZone::Fuel => {
                    "Fuel Station: press E to buy fuel (1 credit/unit).".to_owned()
                }
                SurfaceZone::Repair => {
                    "Repair Garage: press E to repair hull (2 credits/unit).".to_owned()
                }
                SurfaceZone::Depot => {
                    "Ore Depot: press E to sell cargo or review receipts.".to_owned()
                }
                SurfaceZone::Headquarters => depot_prompt(self),
                SurfaceZone::Shop => shop_prompt(&self.player),
                SurfaceZone::Bank => "Bank: press E for loan/debt service.".to_owned(),
                SurfaceZone::Explosives => {
                    "Explosive Shack: press E to buy 3 timed charges for 55 credits.".to_owned()
                }
                SurfaceZone::Salvage => {
                    "Salvage Yard: press E for cargo beacon or hull patch.".to_owned()
                }
            };
        }
    }

    fn check_failure(&mut self) {
        if self.player.hull <= 0.0 {
            self.game_over = true;
            "Hull destroyed! Press E for emergency rescue.".clone_into(&mut self.message);
        } else if self.player.fuel <= 0.0 && self.player.y > 6.0 * TILE_SIZE {
            self.game_over = true;
            "Out of fuel underground! Press E for emergency rescue.".clone_into(&mut self.message);
        }
    }

    #[must_use]
    pub fn signal_relay_count(&self) -> usize {
        self.infrastructure
            .iter()
            .filter(|item| item.kind == InfrastructureKind::SignalRelay)
            .count()
    }

    #[must_use]
    pub fn is_tunnel_supported(&self, position: TilePosition) -> bool {
        self.infrastructure.iter().any(|item| {
            item.kind == InfrastructureKind::TunnelSupport
                && (item.position.x - position.x).abs() <= 3
                && (item.position.y - position.y).abs() <= 3
        })
    }

    #[must_use]
    pub fn is_pump_protected(&self, position: TilePosition) -> bool {
        self.infrastructure.iter().any(|item| {
            item.kind == InfrastructureKind::PumpStation
                && (item.position.x - position.x).abs() <= 4
                && (item.position.y - position.y).abs() <= 4
        })
    }

    fn recover_lost_cargo_if_near(&mut self) {
        let (Some(x), Some(y)) = (self.lost_cargo_x, self.lost_cargo_y) else {
            return;
        };
        if self.lost_cargo_count == 0 {
            return;
        }
        let dx = self.player.x - x;
        let dy = self.player.y - y;
        if dx.hypot(dy) > TILE_SIZE * 0.9 || !self.player.has_cargo_space() {
            return;
        }
        let recovered =
            (self.player.cargo_capacity - self.player.cargo_used()).min(self.lost_cargo_count);
        if recovered == 0 {
            return;
        }
        *self
            .player
            .cargo
            .entry(crate::terrain::MineralKind::Iron)
            .or_default() += recovered;
        self.lost_cargo_count -= recovered;
        self.message = format!("Recovered {recovered} lost cargo crates from rescue site.");
        if self.lost_cargo_count == 0 {
            self.lost_cargo_x = None;
            self.lost_cargo_y = None;
        }
    }

    fn handle_rescue(&mut self, input: PlayerInput) {
        if !input.interact {
            return;
        }

        let base_fee = rescue_fee(self.player.y);
        let relay_count = self.signal_relay_count() as u32;
        let relay_discount_percent = relay_count.saturating_mul(10).min(50);
        let relayed_fee = base_fee.saturating_mul(100 - relay_discount_percent) / 100;
        let fee_divisor = if self.player.insured {
            u32::from(self.player.insurance_tier).saturating_add(1)
        } else {
            1
        };
        let fee = (relayed_fee / fee_divisor).min(self.player.credits);
        self.player.credits -= fee;
        self.rescue_count += 1;
        let before_minerals = self.player.cargo.clone();
        let before_artifacts = self.player.artifacts.clone();
        let armored_cargo = self.has_equipped_part(RigPartKind::ArmoredCargoBay);
        let mut lost_items = if self.player.insured && self.player.insurance_tier >= 2 {
            0
        } else if self.player.insured || armored_cargo {
            drop_quarter_cargo(&mut self.player)
        } else {
            drop_half_cargo(&mut self.player)
        };
        if self.player.y >= 70.0 * TILE_SIZE
            && relay_count == 0
            && (!self.player.insured || self.player.insurance_tier < 4)
        {
            lost_items = lost_items.saturating_add(drop_quarter_cargo(&mut self.player));
        }
        self.player.insured = false;
        self.last_rescue_x = Some(self.player.x);
        self.last_rescue_y = Some(self.player.y);
        if lost_items > 0 {
            self.lost_cargo_x = Some(self.player.x);
            self.lost_cargo_y = Some(self.player.y);
            self.lost_cargo_count = lost_items;
            self.lost_minerals = cargo_difference(&before_minerals, &self.player.cargo);
            self.lost_artifacts = cargo_difference(&before_artifacts, &self.player.artifacts);
        }
        self.last_rescue_summary =
            format!("Fee: {fee} credits. Cargo lost: {lost_items}. Relays online: {relay_count}.");
        self.depot_receipts.push(format!(
            "RESCUE INVOICE\nDepth: {}m\nFee: {fee} cr\nRelay discount: {relay_discount_percent}%\nCargo lost: {lost_items}",
            (self.player.y / TILE_SIZE).floor() as i32
        ));
        if self.depot_receipts.len() > 5 {
            self.depot_receipts.remove(0);
        }
        self.player.x = 12.0 * TILE_SIZE;
        self.player.y = 4.0 * TILE_SIZE;
        self.player.velocity_x = 0.0;
        self.player.velocity_y = 0.0;
        self.player.fuel = self.player.fuel_capacity * 0.5;
        self.player.hull = self.player.max_hull() * 0.5;
        self.game_over = false;
        self.sound_cues.push(SoundCue::Rescue);
        self.message = format!("Emergency rescue completed. {}", self.last_rescue_summary);
    }
    pub const fn take_settings_dirty(&mut self) -> bool {
        let dirty = self.settings_dirty;
        self.settings_dirty = false;
        dirty
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

fn hq_story_message(game: &GameState) -> String {
    if game.won_game {
        return "Director Vale: The Star Core is secure. Deep Claim operations are open: build relays, chase expeditions, catalogue strata, and push for legendary blueprints.".to_owned();
    }
    match game.deepest_tile_reached {
        0..=19 => "Director Vale: Bring us contract cargo and prove this shaft is profitable.".to_owned(),
        20..=39 => "Mechanic Iona: Silver strata ahead. Upgrade before chasing deep seams.".to_owned(),
        40..=59 => "Surveyor Kade: Relic signals are stronger. Gas pockets are no longer rumors.".to_owned(),
        60..=79 => "Director Vale: Thermal readings are ugly. Radiators and hull plating are survival gear.".to_owned(),
        _ => "Surveyor Kade: Star Core harmonics are below. Expect vents, blasts, and cave-ins.".to_owned(),
    }
}

fn rescue_fee(player_y: f32) -> u32 {
    50 + ((player_y / TILE_SIZE).max(0.0) as u32 * 3)
}

fn cargo_difference<K: Copy + Ord>(
    before: &std::collections::BTreeMap<K, u32>,
    after: &std::collections::BTreeMap<K, u32>,
) -> std::collections::BTreeMap<K, u32> {
    let mut difference = std::collections::BTreeMap::new();
    for (key, before_count) in before {
        let after_count = after.get(key).copied().unwrap_or(0);
        if *before_count > after_count {
            difference.insert(*key, *before_count - after_count);
        }
    }
    difference
}

fn drop_half_cargo(player: &mut Player) -> u32 {
    let mut lost = 0;
    for count in player.cargo.values_mut() {
        let dropped = (*count).div_ceil(2);
        *count -= dropped;
        lost += dropped;
    }
    player.cargo.retain(|_, count| *count > 0);

    for count in player.artifacts.values_mut() {
        let dropped = (*count).div_ceil(2);
        *count -= dropped;
        lost += dropped;
    }
    player.artifacts.retain(|_, count| *count > 0);
    lost
}

fn drop_quarter_cargo(player: &mut Player) -> u32 {
    let mut lost = 0;
    for count in player.cargo.values_mut() {
        let dropped = (*count).div_ceil(4);
        *count -= dropped;
        lost += dropped;
    }
    player.cargo.retain(|_, count| *count > 0);

    for count in player.artifacts.values_mut() {
        let dropped = (*count).div_ceil(4);
        *count -= dropped;
        lost += dropped;
    }
    player.artifacts.retain(|_, count| *count > 0);
    lost
}

fn drill_seconds_per_chip(kind: TileKind, drill_strength: u8, direction: DrillDirection) -> f32 {
    let base = match kind {
        TileKind::Air => 0.0,
        TileKind::Dirt => 0.09,
        TileKind::Clay => 0.12,
        TileKind::Stone => 0.15,
        TileKind::HardRock | TileKind::Foundation => 0.19,
        TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => 0.08,
        TileKind::Ore(_) => 0.16,
        TileKind::Artifact(_) => 0.21,
    };
    let drill_bonus = 1.0 + f32::from(drill_strength.saturating_sub(1)) * 0.4;
    let direction_penalty = if direction == DrillDirection::Down {
        1.0
    } else {
        1.45
    };
    (base * direction_penalty / drill_bonus).max(0.045)
}

fn mine_target(player: &Player, input: PlayerInput) -> Option<(TilePosition, DrillDirection)> {
    if !input.drill_down && input.horizontal == 0.0 {
        return None;
    }

    let current_tile = player.tile_position(TILE_SIZE);
    Some(if input.drill_down {
        (
            TilePosition {
                x: current_tile.x,
                y: current_tile.y + 1,
            },
            DrillDirection::Down,
        )
    } else {
        let facing = facing_direction(input.horizontal);
        (
            TilePosition {
                x: current_tile.x + facing,
                y: current_tile.y,
            },
            if facing < 0 {
                DrillDirection::Left
            } else {
                DrillDirection::Right
            },
        )
    })
}

const fn deep_claim_material_for(
    mineral: MineralKind,
    position: TilePosition,
) -> Option<StrategicResourceKind> {
    let roll = (position.x.unsigned_abs() + position.y as u32 + mineral.value()).wrapping_rem(6);
    match (mineral, roll) {
        (MineralKind::Mythril | MineralKind::Uranium, 0 | 1) => {
            Some(StrategicResourceKind::CoreShard)
        }
        (MineralKind::Diamond | MineralKind::Platinum, 0) => {
            Some(StrategicResourceKind::CrystalLens)
        }
        (MineralKind::Ruby | MineralKind::Emerald | MineralKind::Gold, 0) => {
            Some(StrategicResourceKind::AncientAlloy)
        }
        _ => None,
    }
}

pub fn expedition_satisfied(expedition: Expedition, game: &GameState) -> bool {
    match expedition.kind {
        ExpeditionObjectiveKind::ReachDepth => {
            game.deepest_tile_reached as u32 >= expedition.required
        }
        ExpeditionObjectiveKind::DeliverCargo => match expedition.target {
            TileKind::Ore(mineral) => {
                game.player.cargo.get(&mineral).copied().unwrap_or(0) >= expedition.required
            }
            TileKind::Artifact(artifact) => {
                game.player.artifacts.get(&artifact).copied().unwrap_or(0) >= expedition.required
            }
            _ => false,
        },
        ExpeditionObjectiveKind::ScanHazards => {
            game.scan_markers
                .iter()
                .filter(|marker| marker.kind == expedition.target)
                .count()
                >= expedition.required as usize
        }
        ExpeditionObjectiveKind::BuildPumpStations => {
            game.infrastructure
                .iter()
                .filter(|item| item.kind == InfrastructureKind::PumpStation)
                .count()
                >= expedition.required as usize
        }
        ExpeditionObjectiveKind::RecoverProbe
        | ExpeditionObjectiveKind::MineVein
        | ExpeditionObjectiveKind::ScanAnomaly
        | ExpeditionObjectiveKind::RescueMiner
        | ExpeditionObjectiveKind::DeliverExplosives
        | ExpeditionObjectiveKind::StabilizeCollapse
        | ExpeditionObjectiveKind::RetrieveArtifact
        | ExpeditionObjectiveKind::ReachSignal
        | ExpeditionObjectiveKind::NoDamageReturn
        | ExpeditionObjectiveKind::FastReturn => {
            game.expedition_progress(expedition) >= expedition.required
        }
    }
}

pub fn side_contract_satisfied(contract: SideContract, game: &GameState) -> bool {
    if contract
        .expires_day
        .is_some_and(|expires_day| game.town_event_day > expires_day)
    {
        return false;
    }
    match contract.kind {
        SideContractKind::Cargo | SideContractKind::Rush => match contract.target {
            TileKind::Ore(mineral) => {
                game.player.cargo.get(&mineral).copied().unwrap_or(0) >= contract.required
            }
            TileKind::Artifact(artifact) => {
                game.player.artifacts.get(&artifact).copied().unwrap_or(0) >= contract.required
            }
            _ => false,
        },
        SideContractKind::DepthSurvey => {
            u32::try_from(game.deepest_tile_reached).unwrap_or(0) >= contract.required
        }
        SideContractKind::HazardScan => {
            game.scan_markers
                .iter()
                .filter(|marker| {
                    matches!(
                        marker.kind,
                        TileKind::Gas
                            | TileKind::Lava
                            | TileKind::MagmaVent
                            | TileKind::ExplosivePocket
                            | TileKind::PressurePocket
                    )
                })
                .count()
                >= usize::try_from(contract.required).unwrap_or(usize::MAX)
        }
    }
}

const fn scanner_can_mark(kind: TileKind, scanner_level: u8) -> bool {
    match kind {
        TileKind::Ore(_) => scanner_level >= 1,
        TileKind::Gas
        | TileKind::Lava
        | TileKind::MagmaVent
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => scanner_level >= 2,
        TileKind::Artifact(_) => scanner_level >= 3,
        _ => false,
    }
}

const fn current_save_version() -> u32 {
    2
}

const fn all_minerals() -> [MineralKind; 10] {
    [
        MineralKind::Copper,
        MineralKind::Iron,
        MineralKind::Silver,
        MineralKind::Gold,
        MineralKind::Emerald,
        MineralKind::Ruby,
        MineralKind::Diamond,
        MineralKind::Platinum,
        MineralKind::Uranium,
        MineralKind::Mythril,
    ]
}

fn initial_mineral_market_history(
    salt: u32,
    event_day: u32,
) -> std::collections::BTreeMap<MineralKind, Vec<u32>> {
    all_minerals()
        .into_iter()
        .map(|mineral| (mineral, vec![market_factor_for(salt, event_day, mineral)]))
        .collect()
}

const fn market_factor_for(salt: u32, event_day: u32, mineral: MineralKind) -> u32 {
    let mineral_salt = salt.wrapping_add(mineral.value().wrapping_mul(13));
    let base = market_factor(mineral_salt, event_day);
    match (event_day % 5, mineral) {
        (1, MineralKind::Gold | MineralKind::Silver | MineralKind::Platinum) => base + 16,
        (0, MineralKind::Copper | MineralKind::Iron) => base + 8,
        (4, MineralKind::Ruby | MineralKind::Emerald | MineralKind::Diamond) => base + 10,
        _ => base,
    }
}

const fn market_factor(salt: u32, event_day: u32) -> u32 {
    let base = 85 + salt.wrapping_mul(37).wrapping_add(11) % 41;
    if event_day % 5 == 1 { base + 20 } else { base }
}

const fn surface_zone_label(zone: SurfaceZone) -> &'static str {
    match zone {
        SurfaceZone::Fuel => "Fuel Station",
        SurfaceZone::Repair => "Repair Garage",
        SurfaceZone::Depot => "Ore Depot",
        SurfaceZone::Headquarters => "HQ",
        SurfaceZone::Shop => "Upgrade Shop",
        SurfaceZone::Bank => "Bank",
        SurfaceZone::Explosives => "Explosive Shack",
        SurfaceZone::Salvage => "Salvage Yard",
    }
}

const fn interior_service_x(zone: SurfaceZone) -> f32 {
    match zone {
        SurfaceZone::Fuel => 430.0,
        SurfaceZone::Repair => 405.0,
        SurfaceZone::Depot => 455.0,
        SurfaceZone::Headquarters => 390.0,
        SurfaceZone::Shop => 450.0,
        SurfaceZone::Bank => 380.0,
        SurfaceZone::Explosives => 431.0,
        SurfaceZone::Salvage => 410.0,
    }
}

fn surface_zone_at(x: f32, y: f32) -> Option<SurfaceZone> {
    if y > 5.5 * TILE_SIZE {
        return None;
    }

    surface_building_at_tile((x / TILE_SIZE).floor() as i32).map(|building| building.zone)
}

fn depot_prompt(game: &GameState) -> String {
    let contract = &game.contracts.active;
    format!(
        "Depot: E completes contract ({}/{}) {} for {} cr, otherwise sells cargo.",
        contract.progress(&game.player),
        contract.required,
        contract.target.name(),
        contract.reward
    )
}

fn shop_prompt(player: &Player) -> String {
    let offers = upgrade_offers(player);
    let mut prompt = String::from("Upgrade Shop: ");
    for (index, offer) in offers.iter().enumerate() {
        let label = if offer.level >= crate::economy::MAX_UPGRADE_LEVEL {
            "MAX".to_owned()
        } else {
            offer.cost.to_string()
        };
        let _ = write!(prompt, "{}:{}({label}) ", index + 1, offer.name);
    }
    prompt
}

const fn falling_rock_roll(position: TilePosition, seed: u64) -> bool {
    let value = seed
        ^ ((position.x as u64).wrapping_mul(0x9E37))
        ^ ((position.y as u64).wrapping_mul(0x85EB));
    value.is_multiple_of(BOULDER_SPAWN_CHANCE)
}

const fn initial_camera_x() -> f32 {
    PLAYER_SPAWN_X - 1280.0 / 2.0
}

const fn initial_camera_y() -> f32 {
    PLAYER_SPAWN_Y - 720.0 / 2.0 - CAMERA_INTRO_DROP_DISTANCE
}

fn target_camera_offset(game: &GameState) -> (f32, f32) {
    let screen_width = 1280.0;
    let screen_height = 720.0;
    let max_x = game.terrain.width() as f32 * TILE_SIZE - screen_width;
    let max_y = game.terrain.height() as f32 * TILE_SIZE - screen_height;

    (
        (game.player.x - screen_width / 2.0).clamp(0.0, max_x),
        (game.player.y - screen_height / 2.0).clamp(MIN_PLAYER_Y, max_y),
    )
}

fn collision_points(x: f32, y: f32) -> [TilePosition; 4] {
    [
        point_to_tile(x - PLAYER_RADIUS, y - PLAYER_RADIUS),
        point_to_tile(x + PLAYER_RADIUS, y - PLAYER_RADIUS),
        point_to_tile(x - PLAYER_RADIUS, y + PLAYER_RADIUS),
        point_to_tile(x + PLAYER_RADIUS, y + PLAYER_RADIUS),
    ]
}

fn point_to_tile(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

fn facing_direction(horizontal: f32) -> i32 {
    if horizontal < 0.0 { -1 } else { 1 }
}

fn input_changes_game(input: PlayerInput) -> bool {
    input.horizontal.abs() > f32::EPSILON
        || input.thrust
        || input.drill_down
        || input.interact
        || input.confirm
        || input.bomb
        || input.scan
        || input.place_relay
        || input.place_drone
        || input.place_lift
        || input.place_support
        || input.place_pump
        || input.place_processor
}

pub fn session_gameplay_commands_from_input(input: PlayerInput) -> Vec<PlayerCommand> {
    let mut commands = Vec::new();
    commands.push(PlayerCommand::Movement {
        horizontal: input.horizontal.clamp(-1.0, 1.0),
        thrust: input.thrust,
        drill_down: input.drill_down,
    });
    if input.scan {
        commands.push(PlayerCommand::UseScanner);
    }
    if input.bomb {
        commands.push(PlayerCommand::PlaceBomb);
    }
    for (enabled, slot) in [
        (input.place_relay, 0),
        (input.place_drone, 1),
        (input.place_lift, 2),
        (input.place_support, 3),
        (input.place_pump, 4),
        (input.place_processor, 5),
    ] {
        if enabled {
            commands.push(PlayerCommand::PlaceInfrastructure { slot });
        }
    }
    if let Some(index) = input.selected_upgrade {
        commands.push(PlayerCommand::SelectUpgrade { index });
    }
    if input.interact {
        commands.push(PlayerCommand::Interact);
    }
    commands
}

pub const fn input_without_session_gameplay_commands(mut input: PlayerInput) -> PlayerInput {
    input.horizontal = 0.0;
    input.thrust = false;
    input.drill_down = false;
    input.bomb = false;
    input.scan = false;
    input.place_relay = false;
    input.place_drone = false;
    input.place_lift = false;
    input.place_support = false;
    input.place_pump = false;
    input.place_processor = false;
    input.selected_upgrade = None;
    input
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::online_session::{
        RealOnlineSessionController, apply_network_snapshot_local_player_presentation_adapter,
        apply_network_terrain_chunk_to_game, live_player_network_snapshot,
        write_online_descriptor_file,
    };
    use std::collections::BTreeMap;

    #[test]
    fn session_gameplay_commands_map_local_input_to_authoritative_commands_and_strip_shell_input() {
        let input = PlayerInput {
            horizontal: 2.0,
            thrust: true,
            drill_down: true,
            interact: true,
            confirm: true,
            cancel: true,
            scan: true,
            bomb: true,
            place_relay: true,
            place_processor: true,
            selected_upgrade: Some(3),
            map: true,
            ..PlayerInput::default()
        };

        let commands = session_gameplay_commands_from_input(input);
        assert!(commands.iter().any(|command| matches!(
            command,
            PlayerCommand::Movement {
                horizontal,
                thrust: true,
                drill_down: true,
            } if (*horizontal - 1.0).abs() < f32::EPSILON
        )));
        assert!(commands.contains(&PlayerCommand::Interact));
        assert!(!commands.contains(&PlayerCommand::Confirm));
        assert!(!commands.contains(&PlayerCommand::Cancel));
        assert!(commands.contains(&PlayerCommand::UseScanner));
        assert!(commands.contains(&PlayerCommand::PlaceBomb));
        assert!(commands.contains(&PlayerCommand::PlaceInfrastructure { slot: 0 }));
        assert!(commands.contains(&PlayerCommand::PlaceInfrastructure { slot: 5 }));
        assert!(commands.contains(&PlayerCommand::SelectUpgrade { index: 3 }));

        let shell_input = input_without_session_gameplay_commands(input);
        assert!(shell_input.horizontal.abs() < f32::EPSILON);
        assert!(!shell_input.thrust);
        assert!(!shell_input.drill_down);
        assert!(shell_input.interact);
        assert!(shell_input.confirm);
        assert!(shell_input.cancel);
        assert!(!shell_input.scan);
        assert!(!shell_input.bomb);
        assert!(!shell_input.place_relay);
        assert!(!shell_input.place_processor);
        assert_eq!(shell_input.selected_upgrade, None);
        assert!(shell_input.map);
    }

    #[test]
    fn title_menu_exposes_local_split_screen_entrypoint() {
        assert!(GameState::title_options().contains(&TitleOption::LocalMultiplayer));
    }

    #[test]
    fn online_ready_start_transition_blocks_start_until_connected_and_both_ready() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_remote_player_connected = true;
        game.online_local_ready = true;
        game.online_remote_player_ready = false;

        let blocked = game.online_gameplay_start_gate();
        let status = OnlineReadyStartTransitionStatus::from_game(&game);

        assert!(!blocked.ready);
        assert_eq!(
            blocked.blocker,
            Some(OnlineGameplayStartBlocker::RemoteNotReady)
        );
        assert!(status.local_ready_sent);
        assert!(!status.remote_ready_received);
        assert!(status.start_gated_on_connected_ready);
        assert!(!status.start_message_sendable);
        assert!(!status.transition_ready);
        assert!(status.status.contains("blocker=Some(RemoteNotReady)"));
    }

    #[test]
    fn online_ready_start_transition_enters_host_gameplay_when_ui_gate_is_ready() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_remote_player_connected = true;
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.modal = Some(ModalScreen::OnlineMultiplayer);

        game.request_online_gameplay_start();
        let status = OnlineReadyStartTransitionStatus::from_game(&game);

        assert_eq!(game.run_mode, RunMode::Playing);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(status.transition_ready);
        assert!(status.host_enters_gameplay_from_ui);
        assert!(!status.joined_enters_gameplay_from_ui);
        assert!(status.modal_closes_only_when_playing);
        assert!(status.role_slot_save_authority_preserved);
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online ready/start transition") && line.contains("ready=yes")
        }));
    }

    #[test]
    fn online_ready_start_transition_tracks_joined_client_gameplay_entry_and_save_authority() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_remote_player_connected = true;
        client.online_local_ready = true;
        client.online_remote_player_ready = true;
        client.run_mode = RunMode::Playing;
        client.modal = None;
        client.refresh_online_save_boundary_status();

        let status = OnlineReadyStartTransitionStatus::from_game(&client);
        let save_boundary = OnlineSaveBoundaryStatus::from_game(&client);

        assert!(status.transition_ready);
        assert!(!status.host_enters_gameplay_from_ui);
        assert!(status.joined_enters_gameplay_from_ui);
        assert!(status.modal_closes_only_when_playing);
        assert!(status.role_slot_save_authority_preserved);
        assert_eq!(
            save_boundary.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
    }

    #[test]
    fn production_online_ui_runtime_status_accepts_host_descriptor_modal_flow() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Hosting;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_descriptor_path = PathBuf::from("/tmp/drillgame-host.json");
        game.online_diagnostic_controller_mode = "descriptor-host-pending".to_owned();
        game.online_network_task_request = None;
        game.online_last_descriptor_input_status = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::HostWrite,
            &game.online_descriptor_path,
        )
        .status_line();

        let status = ProductionOnlineUiRuntimeStatus::from_game(&game);

        assert!(status.ready);
        assert!(status.host_descriptor_from_real_state);
        assert!(status.host_controller_persistent);
        assert!(status.host_ui_details_visible);
        assert!(status.host_descriptor_path_shareable);
        assert!(status.host_errors_clear_pending_task);
        assert!(status.status.contains("ready=yes"));
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Production online UI runtime")
                && line.contains("host_descriptor_from_real_state=yes")
        }));
    }

    #[test]
    fn production_online_ui_runtime_status_accepts_joined_client_modal_flow() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_descriptor_path = PathBuf::from("/tmp/drillgame-host.json");
        client.online_diagnostic_controller_mode = "descriptor-client-connected".to_owned();
        client.online_network_task_request = None;
        client.refresh_online_save_boundary_status();

        let status = ProductionOnlineUiRuntimeStatus::from_game(&client);
        let save_boundary = OnlineSaveBoundaryStatus::from_game(&client);

        assert!(status.ready);
        assert!(status.join_reads_modal_path);
        assert!(status.join_real_controller_connected);
        assert!(status.join_identity_assigned);
        assert!(status.join_controller_persistent);
        assert!(status.join_ui_save_policy_visible);
        assert!(status.join_errors_clear_pending_task);
        assert_eq!(
            save_boundary.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
        assert!(status.status.contains("join_identity_assigned=yes"));
    }

    #[test]
    fn production_online_ui_runtime_status_blocks_stale_pending_failure_state() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Hosting;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_descriptor_path = PathBuf::from("/tmp/drillgame-host.json");
        game.online_diagnostic_controller_mode = "descriptor-host-pending".to_owned();
        game.online_network_task_request = Some(OnlineNetworkTaskRequest::HostDescriptorFile {
            path: game.online_descriptor_path.clone(),
        });
        game.online_last_failure_status.clear();

        let stale = ProductionOnlineUiRuntimeStatus::from_game(&game);
        assert!(!stale.ready);
        assert!(!stale.host_errors_clear_pending_task);
        assert!(stale.status.contains("host_errors_clear_pending_task=no"));

        game.online_last_failure_status =
            OnlineFailureStatus::classify("descriptor read failed").status_line();
        let surfaced = ProductionOnlineUiRuntimeStatus::from_game(&game);
        assert!(surfaced.host_errors_clear_pending_task);
    }

    #[test]
    fn online_session_ux_and_network_orchestration_statuses_separate_runtime_reducers() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Hosting;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_session_status_message = "Hosting descriptor from UI".to_owned();
        game.online_network_task_request = Some(OnlineNetworkTaskRequest::HostDescriptorFile {
            path: PathBuf::from("host.json"),
        });
        game.online_diagnostic_controller_mode = "descriptor-host-pending".to_owned();
        game.online_diagnostic_last_tick = "42".to_owned();

        let ux_status = OnlineSessionUxReducerStatus::from_game(&game);
        let orchestration = OnlineNetworkTaskOrchestrationStatus::from_game(&game);

        assert_eq!(ux_status.state, OnlineSessionUxState::Hosting);
        assert_eq!(ux_status.role_label, "host");
        assert_eq!(ux_status.player_slot, Some(1));
        assert!(ux_status.host_owns_save);
        assert!(ux_status.gameplay_mutation_free);
        assert!(ux_status.status.contains("Online UX reducer"));
        assert!(orchestration.pending_task.is_some());
        assert_eq!(orchestration.controller_mode, "descriptor-host-pending");
        assert_eq!(orchestration.last_tick, "42");
        assert!(orchestration.ui_presenter_separated);
        assert!(
            orchestration
                .status
                .contains("Online network orchestration")
        );
        assert!(
            game.online_multiplayer_status_lines().iter().any(|line| {
                line.contains("Online UX reducer") && line.contains("state=Hosting")
            })
        );
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online network orchestration")
                && line.contains("descriptor-host-pending")
        }));
    }

    #[test]
    fn portability_boundary_status_tracks_runtime_dependencies() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.refresh_online_ownership_status();
        game.refresh_online_gameplay_domain_status();
        game.refresh_online_save_boundary_status();

        let portability = MultiplayerPortabilityBoundaryStatus::from_game(&game);

        assert!(portability.gameplay_state_render_input_decoupled);
        assert!(portability.network_adapter_isolated);
        assert!(portability.save_boundary_explicit);
        assert!(portability.status.contains("save_boundary_explicit=yes"));
        let lines = game.online_multiplayer_status_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Multiplayer portability boundary"))
        );
    }

    #[test]
    fn online_runtime_status_deck_wraps_consolidated_status_lines_for_named_checks() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_remote_player_connected = true;
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_last_session_boundary_status =
            OnlineSessionBoundaryStatus::client_left("deck check").status_line();
        game.refresh_online_save_boundary_status();

        let deck = OnlineRuntimeStatusDeck::from_lines(game.online_multiplayer_status_lines());

        assert!(deck.contains_line_matching("Transport: Quinn/QUIC"));
        assert!(deck.contains_line_matching("Local split-screen runtime"));
        assert!(deck.contains_line_matching("Online manual working-game gate"));
        assert!(deck.contains_line_matching("Online reconnect policy"));
        assert!(deck.contains_line_matching("Online save boundary"));
    }

    #[test]
    fn local_split_screen_runtime_status_isolated_from_online_direct_connect_state() {
        let mut game = GameState::new();
        assert_eq!(
            LocalMultiplayerRuntimeStatus::from_game(&game).phase,
            LocalMultiplayerRuntimePhase::Inactive
        );

        game.start_local_multiplayer_request();
        let requested = LocalMultiplayerRuntimeStatus::from_game(&game);
        assert_eq!(requested.phase, LocalMultiplayerRuntimePhase::Requested);
        assert!(requested.requested);
        assert!(!requested.active);
        assert_eq!(requested.player_slots, 2);
        assert!(requested.online_isolated);
        assert!(requested.status_line().contains("phase=Requested"));
        assert!(game.take_local_multiplayer_request());
        assert!(!game.take_local_multiplayer_request());

        game.mark_local_multiplayer_active(2);
        let active = LocalMultiplayerRuntimeStatus::from_game(&game);
        assert_eq!(active.phase, LocalMultiplayerRuntimePhase::Active);
        assert!(active.active);
        assert_eq!(active.player_slots, 2);
        assert!(active.online_isolated);
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Local split-screen runtime")
                    && line.contains("online_isolated=yes"))
        );

        game.online_session_state = OnlineSessionUxState::Connected;
        let mixed_state = LocalMultiplayerRuntimeStatus::from_game(&game);
        assert_eq!(mixed_state.phase, LocalMultiplayerRuntimePhase::Active);
        assert!(!mixed_state.online_isolated);
        assert!(mixed_state.status_line().contains("online_isolated=no"));
    }

    #[test]
    fn selecting_local_split_screen_starts_game_and_requests_session_activation() {
        let mut game = GameState::new();
        let options = GameState::title_options();
        game.selected_title_item = options
            .iter()
            .position(|option| *option == TitleOption::LocalMultiplayer)
            .expect("local split-screen option exists");

        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.0,
        );

        assert_eq!(game.run_mode, RunMode::Playing);
        assert!(game.take_local_multiplayer_request());
        assert!(game.message.contains("Player 2"));
    }

    #[test]
    fn joined_online_client_cannot_write_local_saves() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Joined host-owned online session.".to_owned(),
        });

        assert!(!game.can_write_local_save());

        game.save_dirty = true;
        game.handle_save_load(PlayerInput {
            save: true,
            ..PlayerInput::default()
        });
        assert!(game.save_dirty);
        assert!(game.message.contains("host owns"));

        game.save_slot(0);
        assert_eq!(game.take_session_slot_io_request(), None);
        assert!(matches!(game.modal, Some(ModalScreen::SaveSlots)));
        assert!(game.message.contains("host owns"));

        game.modal = Some(ModalScreen::UnsavedExitConfirm);
        game.selected_menu_item = 0;
        game.request_exit = false;
        assert!(game.handle_exit_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        }));
        assert!(!game.request_exit);
        assert!(game.message.contains("host owns"));
    }

    #[test]
    fn save_and_load_slot_modals_queue_authoritative_session_io_instead_of_touching_legacy_game() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::SaveSlots);
        game.selected_menu_item = 1;
        game.save_dirty = true;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_slot_io_request(),
            Some(SessionSlotIoRequest::Save { slot: 1 })
        );
        assert!(game.save_dirty);
        assert!(game.message.contains("authoritative session"));

        game.modal = Some(ModalScreen::LoadSlots);
        game.selected_menu_item = 2;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_slot_io_request(),
            Some(SessionSlotIoRequest::Load { slot: 2 })
        );
        assert!(game.message.contains("authoritative session"));
    }

    #[test]
    fn host_owned_online_session_can_write_local_save_policy() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot: Some(1),
            status_message: "Hosting online session.".to_owned(),
        });

        assert!(game.can_write_local_save());
        assert!(!game.block_joined_client_save());
    }

    #[test]
    fn default_online_descriptor_path_uses_drillgame_state_dir() {
        let path = default_online_descriptor_path();
        assert_eq!(
            path.file_name().and_then(std::ffi::OsStr::to_str),
            Some("drillgame-online-host.json")
        );
        assert!(
            path.parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty())
        );
    }

    #[test]
    fn online_join_descriptor_path_uses_drillgame_state_dir() {
        let path = join_online_descriptor_path();
        assert_eq!(
            path.file_name().and_then(std::ffi::OsStr::to_str),
            Some("drillgame-online-join.json")
        );
        assert!(
            path.parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty())
        );
    }

    #[test]
    fn descriptor_writer_creates_parent_directory_before_writing_descriptor() {
        let unique_root = std::env::temp_dir().join(format!(
            "drillgame-descriptor-parent-{}",
            std::process::id()
        ));
        let unique_path = unique_root.join("nested").join("host.json");
        let _ignored = std::fs::remove_dir_all(&unique_root);

        write_online_descriptor_file(&unique_path, "{\"host_addr\":\"127.0.0.1:4242\"}")
            .expect("descriptor writes into newly created parent directory");

        assert!(unique_path.exists());
        let _ignored = std::fs::remove_dir_all(unique_root);
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn descriptor_host_and_client_complete_join_handshake() {
        let unique_path = std::env::temp_dir().join(format!(
            "drillgame-descriptor-handshake-{}.json",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&unique_path);
        let mut host_game = GameState::new();
        host_game.online_host_bind_addr = "127.0.0.1:0".parse().expect("bind addr parses");
        host_game.online_host_advertise_addr =
            "127.0.0.1:0".parse().expect("advertise addr parses");
        let mut host_controller =
            RealOnlineSessionController::host_descriptor_file_pending(&mut host_game, &unique_path)
                .expect("host descriptor writes");
        let descriptor_json = std::fs::read_to_string(&unique_path).expect("descriptor written");
        let descriptor: crate::multiplayer::QuinnHostConnectionDescriptor =
            serde_json::from_str(&descriptor_json).expect("descriptor parses");
        host_game.online_host_advertise_addr = descriptor.host_addr;

        let mut accept_game = GameState::new();
        let mut client_game = GameState::new();
        let (accept_result, client_result) = tokio::join!(
            host_controller.accept_descriptor_client(&mut accept_game),
            RealOnlineSessionController::connect_descriptor_client(&mut client_game, &unique_path),
        );

        accept_result.expect("host accepts descriptor client");
        let client_controller = client_result.expect("client joins descriptor host");
        assert_eq!(host_controller.mode_label(), "descriptor-host-accepted");
        assert_eq!(
            client_controller.mode_label(),
            "descriptor-client-connected"
        );
        assert_eq!(
            accept_game.online_session_state,
            OnlineSessionUxState::Connected
        );
        assert_eq!(
            client_game.online_session_state,
            OnlineSessionUxState::Connected
        );
        assert!(accept_game.message.contains("Remote miner joined"));
        assert!(client_game.message.contains("Connected to host descriptor"));

        let command_packet = crate::multiplayer::CommandPacket {
            client_id: crate::multiplayer::LOCAL_CLIENT_ID,
            commands: vec![crate::multiplayer::SequencedPlayerCommand {
                player_id: crate::multiplayer::PlayerId::new(2),
                sequence: crate::multiplayer::InputSequence::new(1),
                target_tick: crate::multiplayer::SimulationTick::new(301),
                command: crate::multiplayer::PlayerCommand::Movement {
                    horizontal: 1.0,
                    thrust: true,
                    drill_down: false,
                },
            }],
        };
        let (host_summary, client_command_result) = tokio::join!(
            host_controller.descriptor_host_receive_command_packet(),
            async {
                let mut client_controller = client_controller;
                client_controller
                    .descriptor_client_send_command_packet(command_packet, 1)
                    .await
                    .map(|()| client_controller)
            },
        );
        let host_summary = host_summary.expect("host receives descriptor command packet");
        let client_controller = client_command_result.expect("client receives command ack");
        assert!(host_summary.all_accepted());
        assert_eq!(host_summary.acknowledged, 1);
        assert_eq!(
            client_controller.mode_label(),
            "descriptor-client-connected"
        );
        let mut client_controller = client_controller;

        let snapshot = live_player_network_snapshot(
            &client_game,
            crate::multiplayer::PlayerId::new(2),
            crate::multiplayer::SimulationTick::new(302),
        );
        host_controller
            .descriptor_host_send_snapshot(snapshot.clone())
            .await
            .expect("host sends descriptor snapshot");
        let snapshot_message = client_controller
            .descriptor_client_receive_replication()
            .await
            .expect("client receives descriptor snapshot");
        assert!(matches!(
            snapshot_message,
            crate::multiplayer::ProtocolMessage::SnapshotKeyframe { .. }
        ));

        host_controller
            .descriptor_host_send_world_delta(
                crate::multiplayer::SimulationTick::new(303),
                crate::multiplayer::NetworkDeltaPayload::Players {
                    players: snapshot.players.clone(),
                },
            )
            .await
            .expect("host sends descriptor delta");
        let delta_message = client_controller
            .descriptor_client_receive_replication()
            .await
            .expect("client receives descriptor delta");
        assert!(matches!(
            delta_message,
            crate::multiplayer::ProtocolMessage::WorldDelta { .. }
        ));

        let (terrain_response, client_response) = tokio::join!(
            host_controller.descriptor_host_answer_terrain_request(9),
            client_controller.descriptor_client_request_terrain_chunk(4, 5, 8),
        );
        let expected_response = crate::multiplayer::ProtocolMessage::TerrainChunkResponse {
            chunk_x: 4,
            chunk_y: 5,
            revision: 9,
            tiles: Vec::new(),
        };
        assert_eq!(
            terrain_response.expect("host responds to terrain request"),
            expected_response
        );
        assert_eq!(
            client_response.expect("client receives terrain response"),
            expected_response
        );
        let _ignored = std::fs::remove_file(unique_path);
    }

    #[tokio::test]
    async fn real_online_session_controller_applies_join_tick_and_reconnect_ux() {
        let mut game = GameState::new();
        game.player.x = 42.75;
        game.player.y = 17.25;
        game.player.velocity_x = 0.5;
        game.player.velocity_y = -0.25;
        game.player.fuel = 88.0;
        game.player.hull = 77.0;
        game.player.credits = 123;
        game.update_ticks = 9;
        game.total_resources_refined = 4;
        let mut controller = RealOnlineSessionController::connect_localhost(&mut game)
            .await
            .expect("real session connects");

        assert_eq!(controller.mode_label(), "combined-localhost");
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(
            game.message
                .contains("Connected through real localhost Quinn")
        );

        let telemetry = controller
            .drive_telemetry_tick(&mut game)
            .await
            .expect("telemetry tick runs");
        assert!(telemetry.local_smoke_passed());
        assert_eq!(
            telemetry.summary.terrain_chunk_response,
            Some(crate::multiplayer::ProtocolMessage::TerrainChunkResponse {
                chunk_x: 42,
                chunk_y: 17,
                revision: 4,
                tiles: Vec::new(),
            })
        );
        assert!(matches!(
            telemetry
                .summary
                .correction_summary
                .as_ref()
                .map(|summary| (summary.correction_plan, summary.snap_applied)),
            Some((crate::session::CorrectionPlan::Snap, true))
        ));
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(game.message.contains("Real Quinn tick"));

        controller
            .reconnect(&mut game, crate::multiplayer::SessionToken::new(301))
            .await
            .expect("session reconnects");
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(
            game.message
                .contains("Reconnected through real localhost Quinn")
        );
    }

    #[test]
    fn real_online_session_ux_snapshot_updates_game_state_from_tick_driver() {
        let mut game = GameState::new();
        let tick_summary = QuinnSessionTickSummary {
            command_summary: Some(crate::multiplayer::CommandPacketExchangeSummary {
                client_id: crate::multiplayer::ClientId::new(1),
                acknowledged: 1,
                rejected: 0,
                authoritative_tick: crate::multiplayer::SimulationTick::new(5),
                accepted_commands: Vec::new(),
            }),
            snapshot_replicated: true,
            delta_replicated: true,
            terrain_chunk_response: Some(
                crate::multiplayer::ProtocolMessage::TerrainChunkResponse {
                    chunk_x: 1,
                    chunk_y: 2,
                    revision: 3,
                    tiles: Vec::new(),
                },
            ),
            correction_summary: Some(SocketDrivenCorrectionSummary {
                snapshot_replicated: true,
                authoritative_tick: crate::multiplayer::SimulationTick::new(6),
                correction_plan: crate::session::CorrectionPlan::Smooth,
                presentation_x: 1.0,
                presentation_y: 2.0,
                snap_applied: false,
            }),
        };

        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot::from_tick_summary(
            &tick_summary,
            Some(2),
        ));

        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(game.online_host_owns_save);
        assert_eq!(game.online_player_slot, Some(2));
        assert!(game.message.contains("Real Quinn tick"));
    }

    #[test]
    fn online_limitations_report_real_local_quinn_socket_progress() {
        let limitations = GameState::online_session_limitations();

        assert!(
            limitations
                .iter()
                .any(|limitation| limitation.contains("Real localhost Quinn socket IO"))
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn online_multiplayer_status_lines_report_real_lifecycle_state() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.selected_menu_item = 1;
        game.confirm_online_multiplayer();

        let pending_lines = game.online_multiplayer_status_lines();
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Quinn/QUIC real socket IO enabled"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("JoinLanGame"))
        );
        assert!(pending_lines.iter().any(|line| line.contains("Joining")));
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Role: client"))
        );
        assert!(pending_lines.iter().any(|line| line.contains("Ready: no")));
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Local player: Player"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("joined client: play through the host"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Descriptor file:"))
        );
        assert!(
            pending_lines.iter().any(
                |line| line.contains("Remote player: Host miner") && line.contains("role host")
            )
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("host owns the online save"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online diagnostics"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online replication"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online replicated player"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online terrain sync"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online inventory/upgrades"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online menu boundary"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Online save boundary"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Reconnect ownership"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("Direct-connect config"))
        );
        assert!(
            pending_lines
                .iter()
                .any(|line| line.contains("online-lan-qa-checklist-md"))
        );
        assert!(
            !pending_lines
                .iter()
                .any(|line| line.contains("not enabled yet"))
        );

        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Connected through real localhost Quinn as player 2.".to_owned(),
        });
        let connected_lines = game.online_multiplayer_status_lines();
        assert!(
            connected_lines
                .iter()
                .any(|line| line.contains("Connected"))
        );
        assert!(connected_lines.iter().any(|line| line.contains("Slot: 2")));
        assert!(
            connected_lines
                .iter()
                .any(|line| line.contains("Host owns save: false"))
        );
    }

    #[test]
    fn online_status_lines_explain_address_roles_for_lan_and_vpn_setup() {
        let game = GameState::new();
        let lines = game.online_multiplayer_status_lines();

        assert!(lines.iter().any(|line| line.contains("Address guidance")
            && line.contains("host bind listens locally")
            && line.contains("host advertise is what friends type")
            && line.contains("client bind is this machine")));
        assert!(
            lines.iter().any(|line| line.contains("Host flow")
                && line.contains("share the generated descriptor"))
        );
    }

    #[test]
    fn online_address_validator_describes_errors_warnings_and_acceptance() {
        let invalid = GameState::validate_online_address_edit(
            OnlineAddressEditTarget::HostAdvertise,
            "not-an-address",
        );
        assert_eq!(invalid.severity, OnlineAddressValidationSeverity::Error);
        assert!(invalid.message.contains("use host:port"));

        let advertise_zero = GameState::validate_online_address_edit(
            OnlineAddressEditTarget::HostAdvertise,
            "127.0.0.1:0",
        );
        assert_eq!(
            advertise_zero.severity,
            OnlineAddressValidationSeverity::Error
        );
        assert!(advertise_zero.message.contains("real port"));

        let client_ephemeral = GameState::validate_online_address_edit(
            OnlineAddressEditTarget::ClientBind,
            "0.0.0.0:0",
        );
        assert_eq!(
            client_ephemeral.severity,
            OnlineAddressValidationSeverity::Accepted
        );
        assert!(client_ephemeral.is_accepted());

        let loopback_advertise = GameState::validate_online_address_edit(
            OnlineAddressEditTarget::HostAdvertise,
            "127.0.0.1:5000",
        );
        assert_eq!(
            loopback_advertise.severity,
            OnlineAddressValidationSeverity::Warning
        );
        assert!(loopback_advertise.message.contains("same machine"));
    }

    #[test]
    fn online_start_readiness_line_reports_blockers_and_ready_state() {
        let mut game = GameState::new();
        assert!(
            game.online_start_readiness_line()
                .contains("blocked: connect host and client")
        );

        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_remote_player_connected = true;
        assert!(
            game.online_start_readiness_line()
                .contains("blocked: local player not ready")
        );

        game.online_local_ready = true;
        assert!(
            game.online_start_readiness_line()
                .contains("blocked: remote player not ready")
        );

        game.online_remote_player_ready = true;
        game.online_host_owns_save = true;
        assert!(
            game.online_start_readiness_line()
                .contains("ready: authoritative host can start gameplay")
        );
    }

    #[test]
    fn online_direct_connect_setup_lines_use_game_state_configuration() {
        let mut game = GameState::new();
        game.online_descriptor_path = PathBuf::from("/tmp/custom-host.json");
        game.online_host_bind_addr = "0.0.0.0:5252".parse().expect("host bind parses");
        game.online_host_advertise_addr = "192.0.2.25:5252".parse().expect("host advertise parses");
        game.online_client_bind_addr = "0.0.0.0:0".parse().expect("client bind parses");
        game.online_gameplay_ticks = 90;

        let lines = game.online_direct_connect_setup_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("/tmp/custom-host.json"))
        );
        assert!(lines.iter().any(|line| line.contains("192.0.2.25:5252")));
        assert!(lines.iter().any(|line| line.contains("90")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("online-host-gameplay-descriptor-file-on-addr"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("online-join-gameplay-descriptor-file-on-addr"))
        );
    }

    #[test]
    fn online_failure_status_messages_are_player_actionable() {
        assert!(
            GameState::online_failure_status_message("protocol version mismatch")
                .contains("same build")
        );
        assert!(
            GameState::online_failure_status_message("certificate verify failed")
                .contains("Regenerate")
        );
        assert!(
            GameState::online_failure_status_message("descriptor JSON parse error")
                .contains("descriptor file")
        );
        assert!(
            GameState::online_failure_status_message("connection timed out").contains("UDP port")
        );
        assert!(
            GameState::online_failure_status_message("connection refused").contains("firewall")
        );
        assert!(
            GameState::online_failure_status_message("session token reconnect failed")
                .contains("Rejoin")
        );
    }

    #[test]
    fn online_network_task_results_reduce_into_game_state() {
        let mut game = GameState::new();

        let connected = game.apply_online_network_task_result(OnlineNetworkTaskResult::Connected(
            RealOnlineSessionUxSnapshot::from_joined_session(Some(1)),
        ));
        assert_eq!(
            connected.kind,
            OnlineTaskResultTransitionKind::EnteredGameplay
        );
        assert_eq!(connected.state, OnlineSessionUxState::Connected);
        assert!(connected.entered_playing);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert_eq!(game.run_mode, RunMode::Playing);
        assert_eq!(game.modal, None);
        assert_eq!(game.selected_menu_item, 0);

        let failed = game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
            "direct Quinn connection task failed".to_owned(),
        ));
        assert_eq!(failed.kind, OnlineTaskResultTransitionKind::Failed);
        assert_eq!(failed.state, OnlineSessionUxState::Error);
        assert!(!failed.entered_playing);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Error);
        assert_eq!(game.online_network_task_request, None);
        assert!(game.message.contains("direct Quinn connection task failed"));

        let shutdown = game.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(
            OnlineShutdownSummary::offline(),
        ));
        assert_eq!(shutdown.kind, OnlineTaskResultTransitionKind::Shutdown);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Shutdown);
    }

    #[test]
    fn descriptor_online_task_results_keep_modal_until_ready_start() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);

        let hosted = game.apply_online_network_task_result(OnlineNetworkTaskResult::Hosted(
            RealOnlineSessionUxSnapshot::from_host_descriptor_ready(
                Some(1),
                &default_online_descriptor_path(),
            ),
        ));
        assert_eq!(
            hosted.kind,
            OnlineTaskResultTransitionKind::HostWaitingForJoin
        );
        assert_eq!(hosted.state, OnlineSessionUxState::Hosting);
        assert_eq!(hosted.role_label, "host");
        assert!(hosted.modal_open);
        assert!(!hosted.entered_playing);
        assert!(
            hosted
                .player_facing_summary()
                .contains("wait for the joined client")
        );
        assert_eq!(game.run_mode, RunMode::Title);
        assert_eq!(game.modal, Some(ModalScreen::OnlineMultiplayer));

        let joined =
            game.apply_online_network_task_result(OnlineNetworkTaskResult::JoinedDescriptor(
                RealOnlineSessionUxSnapshot::from_descriptor_client_connected(
                    Some(2),
                    &default_online_descriptor_path(),
                ),
            ));
        assert_eq!(
            joined.kind,
            OnlineTaskResultTransitionKind::JoinedWaitingForStart
        );
        assert_eq!(joined.state, OnlineSessionUxState::Connected);
        assert_eq!(joined.role_label, "client");
        assert!(joined.modal_open);
        assert!(!joined.entered_playing);
        assert!(joined.player_facing_summary().contains("toggle ready"));
        assert_eq!(game.run_mode, RunMode::Title);
        assert_eq!(game.modal, Some(ModalScreen::OnlineMultiplayer));
    }

    #[test]
    fn online_gameplay_start_gate_reports_blockers_and_ready_transition() {
        let mut game = GameState::new();
        let not_connected = game.online_gameplay_start_gate();
        assert!(!not_connected.ready);
        assert_eq!(
            not_connected.blocker,
            Some(OnlineGameplayStartBlocker::NotConnected)
        );

        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_remote_player_connected = true;
        let local_not_ready = game.online_gameplay_start_gate();
        assert_eq!(
            local_not_ready.blocker,
            Some(OnlineGameplayStartBlocker::LocalNotReady)
        );

        game.online_local_ready = true;
        game.online_remote_player_connected = false;
        let remote_not_connected = game.online_gameplay_start_gate();
        assert_eq!(
            remote_not_connected.blocker,
            Some(OnlineGameplayStartBlocker::RemoteNotConnected)
        );

        game.online_remote_player_connected = true;
        let remote_not_ready = game.online_gameplay_start_gate();
        assert_eq!(
            remote_not_ready.blocker,
            Some(OnlineGameplayStartBlocker::RemoteNotReady)
        );

        game.online_remote_player_ready = true;
        game.online_host_owns_save = true;
        let ready = game.online_gameplay_start_gate();
        assert!(ready.ready);
        assert_eq!(ready.blocker, None);
        game.request_online_gameplay_start();
        assert_eq!(game.run_mode, RunMode::Playing);
        assert_eq!(game.modal, None);
        assert!(game.message.contains("Starting online gameplay"));
    }

    #[test]
    fn joined_client_cannot_start_authoritative_online_gameplay_from_ui() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_local_ready = true;
        client.online_remote_player_connected = true;
        client.online_remote_player_ready = true;

        let gate = client.online_gameplay_start_gate();
        assert!(!gate.ready);
        assert_eq!(
            gate.blocker,
            Some(OnlineGameplayStartBlocker::HostAuthorityRequired)
        );
        client.request_online_gameplay_start();

        assert_eq!(client.run_mode, RunMode::Title);
        assert!(!client.online_start_session_requested);
        assert!(
            client
                .online_session_status_message
                .contains("only the authoritative host")
        );
    }

    #[test]
    fn online_multiplayer_cancel_clears_queued_network_request() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.selected_menu_item = 0;
        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.0,
        );
        assert_eq!(
            game.online_network_task_request,
            Some(OnlineNetworkTaskRequest::HostLanGame)
        );

        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.update(
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
            0.0,
        );

        assert_eq!(game.online_network_task_request, None);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Idle);
        assert_eq!(game.modal, None);
        assert!(game.message.contains("pending network task canceled"));
    }

    #[test]
    fn online_save_policy_line_reports_host_owned_save_rules() {
        let mut game = GameState::new();
        assert!(game.online_save_policy_line().contains("save/load"));
        assert_eq!(
            game.online_save_exit_policy(),
            OnlineSaveExitPolicy {
                save_authority: OnlineSaveAuthority::LocalPlayer,
                local_save_allowed: true,
                local_load_allowed: true,
                save_before_exit_allowed: false,
                unsaved_exit_action: OnlineUnsavedExitAction::CleanExitAllowed,
            }
        );

        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Connected as joined client.".to_owned(),
        });
        game.save_dirty = true;
        assert!(
            game.online_save_policy_line()
                .contains("local save/load blocked")
        );
        assert_eq!(
            game.online_save_exit_policy(),
            OnlineSaveExitPolicy {
                save_authority: OnlineSaveAuthority::RemoteHost,
                local_save_allowed: false,
                local_load_allowed: false,
                save_before_exit_allowed: false,
                unsaved_exit_action: OnlineUnsavedExitAction::DiscardOrCancelOnly,
            }
        );

        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot: Some(1),
            status_message: "Connected as host.".to_owned(),
        });
        assert!(game.online_save_policy_line().contains("save/load"));
        assert!(game.online_save_exit_policy().save_before_exit_allowed);
    }

    #[test]
    fn joined_client_save_load_and_unsaved_exit_are_blocked_without_losing_session_state() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Connected as joined client.".to_owned(),
        });
        game.save_dirty = true;
        game.run_mode = RunMode::Playing;

        assert!(game.block_joined_client_save());
        assert!(game.message.contains("Save blocked"));
        assert!(game.online_session_status_message.contains("Save blocked"));
        assert_eq!(game.online_session_state, OnlineSessionUxState::Connected);
        assert!(!game.request_exit);

        assert!(game.block_joined_client_load());
        assert!(game.message.contains("Load blocked"));
        assert!(game.online_session_status_message.contains("Load blocked"));
        assert_eq!(game.run_mode, RunMode::Playing);

        game.modal = Some(ModalScreen::UnsavedExitConfirm);
        game.selected_menu_item = 0;
        assert!(game.handle_exit_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        }));
        assert_eq!(game.modal, Some(ModalScreen::UnsavedExitConfirm));
        assert!(!game.request_exit);
        assert!(game.message.contains("Save blocked"));

        game.selected_menu_item = 1;
        assert!(game.handle_exit_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        }));
        assert!(game.request_exit);
    }

    #[test]
    fn host_owned_online_save_policy_allows_save_exit_and_load_paths() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: true,
            player_slot: Some(1),
            status_message: "Connected as host.".to_owned(),
        });
        game.save_dirty = true;

        assert!(!game.block_joined_client_save());
        assert!(!game.block_joined_client_load());
        assert_eq!(
            game.online_save_exit_policy().save_authority,
            OnlineSaveAuthority::LocalPlayer
        );
        assert!(game.online_save_exit_policy().local_save_allowed);
        assert!(game.online_save_exit_policy().local_load_allowed);
        assert!(game.online_save_exit_policy().save_before_exit_allowed);
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("local_load_allowed=yes")
                    && line.contains("save_before_exit_allowed=yes"))
        );
    }

    #[test]
    fn joined_client_pause_save_and_load_entries_are_blocked_before_slot_modals() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Connected as joined client.".to_owned(),
        });
        game.run_mode = RunMode::Paused;

        game.selected_pause_item = PauseOption::ALL
            .iter()
            .position(|option| matches!(option, PauseOption::Save))
            .expect("save pause option exists");
        game.handle_pause_menu(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(game.modal, None);
        assert!(game.message.contains("Save blocked"));

        game.selected_pause_item = PauseOption::ALL
            .iter()
            .position(|option| matches!(option, PauseOption::Load))
            .expect("load pause option exists");
        game.handle_pause_menu(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(game.modal, None);
        assert!(game.message.contains("Load blocked"));
    }

    #[test]
    fn save_load_status_lines_explain_joined_client_unsaved_exit_choices() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(RealOnlineSessionUxSnapshot {
            state: OnlineSessionUxState::Connected,
            host_owns_save: false,
            player_slot: Some(2),
            status_message: "Connected as joined client.".to_owned(),
        });
        game.save_dirty = true;

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(|line| {
            line.contains("host owns the online save; local save/load blocked, discard or cancel unsaved exit")
        }));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("local_save_allowed=no")
                    && line.contains("local_load_allowed=no")
                    && line.contains("save_before_exit_allowed=no"))
        );
    }

    #[test]
    fn reconnect_failure_and_ownership_diagnostics_are_player_facing() {
        let mut game = GameState::new();
        game.online_player_name = "Tunnel Guest".to_owned();
        game.online_player_slot = Some(2);
        game.online_host_owns_save = false;
        game.online_session_state = OnlineSessionUxState::Connected;

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(|line| {
            line.contains("Reconnect ownership")
                && line.contains("Tunnel Guest")
                && line.contains("slot=2")
                && line.contains("remote-host")
        }));

        game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
            "reconnect failed: session token rejected".to_owned(),
        ));
        assert_eq!(game.online_session_state, OnlineSessionUxState::Error);
        assert!(game.message.contains("Reconnect failed"));
        assert!(!game.online_remote_player_connected);
    }

    #[test]
    fn reconnect_ownership_diagnostics_survive_shutdown_acknowledgement() {
        let mut game = GameState::new();
        game.online_player_name = "Host Keeper".to_owned();
        game.online_player_slot = Some(1);
        game.online_host_owns_save = true;
        game.online_session_state = OnlineSessionUxState::Connected;

        game.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(
            OnlineShutdownSummary::offline(),
        ));

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(|line| {
            line.contains("Reconnect ownership")
                && line.contains("Host Keeper")
                && line.contains("slot=1")
                && line.contains("local-host")
        }));
        assert_eq!(game.online_session_state, OnlineSessionUxState::Shutdown);
        assert!(game.message.contains("shutdown acknowledged"));
    }

    #[test]
    fn joined_online_client_unsaved_exit_save_choice_is_blocked_with_message() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = false;
        game.save_dirty = true;

        game.request_exit_or_prompt();
        assert_eq!(game.modal, Some(ModalScreen::UnsavedExitConfirm));
        game.selected_menu_item = 0;
        let handled = game.handle_exit_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });

        assert!(handled);
        assert!(!game.request_exit);
        assert!(game.save_dirty);
        assert!(game.message.contains("Save blocked"));
        assert_eq!(game.modal, Some(ModalScreen::UnsavedExitConfirm));
    }

    #[test]
    fn host_owned_online_unsaved_exit_discard_can_request_exit() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.save_dirty = true;

        game.request_exit_or_prompt();
        assert_eq!(game.modal, Some(ModalScreen::UnsavedExitConfirm));
        game.selected_menu_item = 1;
        let handled = game.handle_exit_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });

        assert!(handled);
        assert!(game.request_exit);
        assert!(game.save_dirty);
    }

    #[test]
    fn online_connected_back_requests_shutdown_instead_of_silent_close() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.online_diagnostic_controller_mode = "descriptor-client-connected".to_owned();
        game.selected_menu_item = 14;

        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.0,
        );

        assert_eq!(game.modal, None);
        assert_eq!(
            game.online_network_task_request,
            Some(OnlineNetworkTaskRequest::Shutdown)
        );
        assert!(!game.online_local_ready);
        assert!(!game.online_remote_player_ready);
        assert!(!game.online_remote_player_connected);
        assert!(game.message.contains("shutdown requested"));
        assert!(
            game.online_last_session_boundary_status
                .contains("LocalShutdownRequested")
        );
    }

    #[test]
    fn gameplay_sync_evidence_matrix_reports_missing_partial_and_full_mvp_loop() {
        let mut game = GameState::new();
        let missing = OnlineGameplaySyncEvidenceMatrix::from_game(&game);
        assert_eq!(missing.movement, OnlineSyncEvidenceQuality::Missing);
        assert_eq!(missing.drilling_terrain, OnlineSyncEvidenceQuality::Missing);
        assert_eq!(missing.cargo_economy, OnlineSyncEvidenceQuality::Missing);
        assert!(!missing.complete_for_mvp_loop);
        assert!(missing.status.contains("mvp_loop_complete=no"));

        game.online_last_gameplay_domain_status =
            "Online gameplay domains: movement=visible terrain=missing cargo_economy=missing survival=missing inventory=missing menu_boundary=visible reconnect_recovery=missing authority_correction=missing"
                .to_owned();
        let diagnostic = OnlineGameplaySyncEvidenceMatrix::from_game(&game);
        assert_eq!(
            diagnostic.movement,
            OnlineSyncEvidenceQuality::DiagnosticOnly
        );
        assert_eq!(
            diagnostic.pause_menu_boundaries,
            OnlineSyncEvidenceQuality::LiveReplicated
        );

        game.apply_online_replicated_player_status(
            "tick 3: replicated player p1 pos=(4,5) vel=(1,0) fuel=88 hull=77 credits=12 cargo=2",
        );
        game.apply_online_terrain_status(
            "terrain chunk applied: network_tiles=10 visible_tiles=10",
        );
        let full = OnlineGameplaySyncEvidenceMatrix::from_game(&game);
        assert_eq!(full.movement, OnlineSyncEvidenceQuality::LiveReplicated);
        assert_eq!(
            full.drilling_terrain,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert_eq!(
            full.cargo_economy,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert_eq!(
            full.survival_hazards,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert!(full.complete_for_mvp_loop);
        assert!(full.status.contains("mvp_loop_complete=yes"));
    }

    #[test]
    fn gameplay_sync_evidence_matrix_covers_inventory_reconnect_save_and_authority() {
        let mut game = GameState::new();
        game.rig_part_inventory.insert(RigPartKind::CargoBalloon);
        game.online_last_save_boundary_status =
            OnlineSaveBoundaryStatus::from_game(&game).status_line();
        game.online_last_ownership_status =
            "Online ownership: identity=known reconnect_context=preserved save_authority=LocalPlayer"
                .to_owned();
        game.apply_online_authority_correction_status(&OnlineAuthorityCorrectionPresentation {
            authority: "host authoritative simulation",
            correction_feel: OnlineCorrectionFeel::SmoothReconcile,
            correction_label: "smooth reconcile",
            snap_applied: false,
            player_message: "host authoritative simulation active".to_owned(),
        });

        let matrix = OnlineGameplaySyncEvidenceMatrix::from_game(&game);
        assert_eq!(
            matrix.upgrades_inventory,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert_eq!(
            matrix.reconnect_save_boundaries,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert_eq!(
            matrix.authority_corrections,
            OnlineSyncEvidenceQuality::LiveReplicated
        );
        assert!(matrix.status.contains("upgrades_inventory=live-replicated"));
        assert!(
            matrix
                .status
                .contains("reconnect_save_boundaries=live-replicated")
        );
    }

    #[test]
    fn gameplay_sync_evidence_status_is_refreshed_and_exposed_in_status_lines() {
        let mut game = GameState::new();
        game.apply_online_replicated_player_status(
            "tick 4: replicated player p1 pos=(7,8) fuel=80 hull=70 credits=44 cargo=3",
        );
        assert!(
            game.online_last_gameplay_sync_evidence_status
                .contains("movement=live-replicated")
        );
        assert!(
            game.online_last_gameplay_sync_evidence_status
                .contains("cargo_economy=live-replicated")
        );
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online gameplay sync evidence")
                    && line.contains("movement=live-replicated"))
        );

        game.clear_online_diagnostics();
        assert!(game.online_last_gameplay_sync_evidence_status.is_empty());
    }

    #[test]
    fn playable_session_status_tracks_host_join_ready_and_playing_phases() {
        let mut host = GameState::new();
        host.online_host_owns_save = true;
        host.online_player_slot = Some(1);
        host.online_session_state = OnlineSessionUxState::Hosting;
        let hosting = OnlinePlayableSessionStatus::from_game(&host);
        assert_eq!(hosting.phase, OnlinePlayableSessionPhase::HostWaiting);
        assert!(hosting.host_from_ui);
        assert!(!hosting.joined_from_ui);
        assert!(!hosting.both_entered_gameplay);
        assert_eq!(
            hosting.blocker,
            Some(OnlineGameplayStartBlocker::NotConnected)
        );

        let mut client = GameState::new();
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_local_ready = true;
        client.online_remote_player_connected = true;
        client.online_remote_player_ready = false;
        let joined = OnlinePlayableSessionStatus::from_game(&client);
        assert_eq!(joined.phase, OnlinePlayableSessionPhase::JoinedWaiting);
        assert!(joined.joined_from_ui);
        assert_eq!(
            joined.blocker,
            Some(OnlineGameplayStartBlocker::RemoteNotReady)
        );

        client.online_remote_player_ready = true;
        let ready = OnlinePlayableSessionStatus::from_game(&client);
        assert_eq!(ready.phase, OnlinePlayableSessionPhase::JoinedWaiting);
        assert!(ready.status.contains("start_ready=no"));
        assert_eq!(
            ready.blocker,
            Some(OnlineGameplayStartBlocker::HostAuthorityRequired)
        );

        client.apply_online_start_session_from_host(crate::multiplayer::SimulationTick::new(9));
        let playing = OnlinePlayableSessionStatus::from_game(&client);
        assert_eq!(playing.phase, OnlinePlayableSessionPhase::Playing);
        assert!(playing.both_entered_gameplay);
        assert!(
            client
                .online_last_playable_session_status
                .contains("phase=Playing")
        );
    }

    #[test]
    fn playable_session_status_tracks_gameplay_sync_evidence_for_mvp_loop() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.run_mode = RunMode::Playing;
        game.modal = None;
        game.online_host_owns_save = false;
        game.online_player_slot = Some(2);

        let empty = OnlinePlayableSessionStatus::from_game(&game);
        assert_eq!(empty.phase, OnlinePlayableSessionPhase::Playing);
        assert!(!empty.movement_visible);
        assert!(!empty.terrain_or_cargo_visible);

        game.apply_online_replicated_player_status(
            "tick 2: applied player 1 pos=(1.0,2.0) fuel=99 hull=100 credits=3 cargo=1",
        );
        assert!(
            game.online_last_playable_session_status
                .contains("movement=yes")
        );
        assert!(
            game.online_last_playable_session_status
                .contains("terrain_or_cargo=yes")
        );

        game.apply_online_terrain_status("applied chunk (0,0) rev 4: 5 visible tiles");
        let synced = OnlinePlayableSessionStatus::from_game(&game);
        assert!(synced.movement_visible);
        assert!(synced.terrain_or_cargo_visible);
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online playable session gate")
                    && line.contains("phase=Playing"))
        );
    }

    #[test]
    fn playable_session_status_reports_not_started_blocked_and_ended_states() {
        let idle = GameState::new();
        let not_started = OnlinePlayableSessionStatus::from_game(&idle);
        assert_eq!(not_started.phase, OnlinePlayableSessionPhase::NotStarted);
        assert_eq!(
            not_started.blocker,
            Some(OnlineGameplayStartBlocker::NotConnected)
        );

        let mut reconnecting = GameState::new();
        reconnecting.online_session_state = OnlineSessionUxState::Reconnecting;
        let blocked = OnlinePlayableSessionStatus::from_game(&reconnecting);
        assert_eq!(blocked.phase, OnlinePlayableSessionPhase::Blocked);

        let mut ended = GameState::new();
        ended.online_session_state = OnlineSessionUxState::Shutdown;
        let shutdown = OnlinePlayableSessionStatus::from_game(&ended);
        assert_eq!(shutdown.phase, OnlinePlayableSessionPhase::Ended);
        assert!(shutdown.status.contains("phase=Ended"));
    }

    #[test]
    fn online_lobby_status_summarizes_local_remote_readiness_and_start_gate() {
        let mut game = GameState::new();
        game.online_player_name = "Host Lobby".to_owned();
        game.online_player_slot = Some(1);
        game.online_host_owns_save = true;
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_local_ready = true;
        game.online_remote_player_name = Some("Client Lobby".to_owned());
        game.online_remote_player_connected = true;
        game.online_remote_player_ready = false;

        let blocked = OnlineLobbyStatus::from_game(&game);
        assert_eq!(blocked.local.name, "Host Lobby");
        assert_eq!(blocked.remote.name, "Client Lobby");
        assert_eq!(blocked.local_readiness, OnlineLobbyReadinessState::Ready);
        assert_eq!(
            blocked.remote_readiness,
            OnlineLobbyReadinessState::NotReady
        );
        assert!(!blocked.can_start);
        assert_eq!(
            blocked.blocker,
            Some(OnlineGameplayStartBlocker::RemoteNotReady)
        );
        assert!(blocked.status.contains("local=Host Lobby"));
        assert!(blocked.status.contains("remote=Client Lobby"));
        assert!(blocked.status.contains("ready=not-ready"));

        game.online_remote_player_ready = true;
        let ready = OnlineLobbyStatus::from_game(&game);
        assert!(ready.can_start);
        assert_eq!(ready.blocker, None);
        assert_eq!(ready.remote_readiness, OnlineLobbyReadinessState::Ready);
        assert!(ready.status.contains("start_ready=yes"));
    }

    #[test]
    fn online_lobby_status_reports_waiting_remote_connection_for_joined_client() {
        let mut game = GameState::new();
        game.online_player_name = "Joined Lobby".to_owned();
        game.online_player_slot = Some(2);
        game.online_host_owns_save = false;
        game.online_session_state = OnlineSessionUxState::Joining;
        game.online_remote_player_name = Some("Host Lobby".to_owned());
        game.online_remote_player_connected = false;

        let status = OnlineLobbyStatus::from_game(&game);
        assert_eq!(status.local.role_label, "client");
        assert_eq!(status.remote.role_label, "host");
        assert_eq!(
            status.remote_readiness,
            OnlineLobbyReadinessState::WaitingForConnection
        );
        assert_eq!(status.local.save_authority, OnlineSaveAuthority::RemoteHost);
        assert_eq!(
            status.remote.save_authority,
            OnlineSaveAuthority::LocalPlayer
        );
        assert!(status.status.contains("waiting-for-connection"));
        assert!(status.status.contains("save_authority=RemoteHost"));
    }

    #[test]
    fn online_lobby_status_refreshes_from_identity_ready_and_boundary_reducers() {
        let mut game = GameState::new();
        game.online_player_slot = Some(1);
        game.online_host_owns_save = true;
        game.online_session_state = OnlineSessionUxState::Connected;
        let remote_id = crate::multiplayer::PlayerId::new(2);

        game.apply_online_remote_identity(remote_id, "Remote Dana");
        assert!(game.online_last_lobby_status.contains("Remote Dana"));
        assert!(game.online_last_lobby_status.contains("ready=not-ready"));

        game.apply_online_remote_ready_state(remote_id, true);
        assert!(game.online_last_lobby_status.contains("remote=Remote Dana"));
        assert!(game.online_last_lobby_status.contains("ready=ready"));
        assert!(game.online_remote_player_connected);

        game.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::client_left(
            "Remote Dana disconnected",
        ));
        assert!(!game.online_remote_player_connected);
        assert!(!game.online_remote_player_ready);
        assert!(
            game.online_last_lobby_status
                .contains("waiting-for-connection")
        );
    }

    #[test]
    fn local_ready_toggle_refreshes_lobby_status_line() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_remote_player_connected = true;
        game.online_multiplayer_view = OnlineMultiplayerView::HostLan;
        game.selected_menu_item = 0;

        game.confirm_online_multiplayer();
        assert!(game.online_local_ready);
        assert!(game.online_last_lobby_status.contains("local="));
        assert!(game.online_last_lobby_status.contains("ready=ready"));

        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online lobby status") && line.contains("ready=ready"))
        );
    }

    #[test]
    fn descriptor_input_status_validates_host_write_and_join_read_paths() {
        let unique_path = std::env::temp_dir().join(format!(
            "drillgame-descriptor-input-{}.json",
            std::process::id()
        ));
        let _ignored = std::fs::remove_file(&unique_path);

        let host = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::HostWrite,
            &unique_path,
        );
        assert!(host.accepted);
        assert!(host.can_attempt_task);
        assert!(host.message.contains("Host descriptor path accepted"));
        assert!(host.status_line().contains("mode=HostWrite"));

        let missing_join = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::JoinRead,
            &unique_path,
        );
        assert!(!missing_join.accepted);
        assert!(missing_join.can_attempt_task);
        assert!(missing_join.message.contains("not found yet"));

        std::fs::write(&unique_path, "{}").expect("write descriptor test file");
        let join = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::JoinRead,
            &unique_path,
        );
        assert!(join.accepted);
        assert!(join.can_attempt_task);
        assert!(join.message.contains("Join descriptor path accepted"));
        assert!(join.status_line().contains("mode=JoinRead"));
        let _ignored = std::fs::remove_file(unique_path);
    }

    #[test]
    fn descriptor_input_status_rejects_empty_invalid_and_non_json_paths() {
        let empty = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::HostWrite,
            Path::new(""),
        );
        assert!(!empty.accepted);
        assert!(empty.message.contains("empty"));

        let invalid = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::HostWrite,
            Path::new("bad<descriptor>.json"),
        );
        assert!(!invalid.accepted);
        assert!(invalid.message.contains("unsupported characters"));

        let non_json = OnlineDescriptorInputStatus::validate(
            OnlineDescriptorInputMode::HostWrite,
            Path::new("descriptor.txt"),
        );
        assert!(!non_json.accepted);
        assert!(non_json.message.contains("must end in .json"));
    }

    #[test]
    fn descriptor_input_status_is_exposed_in_online_status_lines() {
        let mut game = GameState::new();
        game.online_descriptor_path = PathBuf::from("descriptor.txt");
        game.refresh_online_descriptor_input_status(OnlineDescriptorInputMode::HostWrite);
        assert!(
            game.online_last_descriptor_input_status
                .contains("must end in .json")
        );
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(
                    |line| line.contains("Online descriptor input") && line.contains("accepted=no")
                )
        );
    }

    #[test]
    fn online_save_boundary_status_explains_host_dirty_and_clean_exit_policy() {
        let mut host = GameState::new();
        host.online_session_state = OnlineSessionUxState::Connected;
        host.online_host_owns_save = true;
        host.save_dirty = true;
        let dirty = OnlineSaveBoundaryStatus::from_game(&host);
        assert_eq!(dirty.save_authority, OnlineSaveAuthority::LocalPlayer);
        assert!(dirty.local_save_allowed);
        assert!(dirty.local_load_allowed);
        assert!(dirty.save_before_exit_allowed);
        assert_eq!(
            dirty.unsaved_exit_action,
            OnlineUnsavedExitAction::SaveAndExitAllowed
        );
        assert!(dirty.player_message.contains("Save+Exit"));
        assert!(dirty.status_line().contains("dirty=yes"));

        host.save_dirty = false;
        let clean = OnlineSaveBoundaryStatus::from_game(&host);
        assert_eq!(
            clean.unsaved_exit_action,
            OnlineUnsavedExitAction::CleanExitAllowed
        );
        assert!(!clean.save_before_exit_allowed);
        assert!(clean.player_message.contains("clean save"));
        assert!(clean.status_line().contains("local_load_allowed=yes"));
    }

    #[test]
    fn online_save_boundary_status_explains_joined_client_unsaved_exit_block() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.save_dirty = true;
        let dirty = OnlineSaveBoundaryStatus::from_game(&client);
        assert_eq!(dirty.save_authority, OnlineSaveAuthority::RemoteHost);
        assert!(!dirty.local_save_allowed);
        assert!(!dirty.local_load_allowed);
        assert!(!dirty.save_before_exit_allowed);
        assert_eq!(
            dirty.unsaved_exit_action,
            OnlineUnsavedExitAction::DiscardOrCancelOnly
        );
        assert!(dirty.player_message.contains("Save+Exit is blocked"));
        assert!(dirty.player_message.contains("Discard or Cancel"));
        assert!(dirty.status_line().contains("authority=RemoteHost"));

        client.save_dirty = false;
        let clean = OnlineSaveBoundaryStatus::from_game(&client);
        assert_eq!(
            clean.unsaved_exit_action,
            OnlineUnsavedExitAction::CleanExitAllowed
        );
        assert!(
            clean
                .player_message
                .contains("local save/load remain blocked")
        );
    }

    #[test]
    fn joined_client_save_and_load_blocks_update_structured_save_boundary_status() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.save_dirty = true;

        assert!(client.block_joined_client_save());
        assert!(client.message.contains("Save blocked"));
        assert!(
            client
                .online_last_save_boundary_status
                .contains("authority=RemoteHost")
        );
        assert!(
            client
                .online_last_save_boundary_status
                .contains("local_save_allowed=no")
        );

        assert!(client.block_joined_client_load());
        assert!(client.message.contains("Load blocked"));
        assert!(
            client
                .online_last_save_boundary_status
                .contains("local_load_allowed=no")
        );
    }

    #[test]
    fn active_gameplay_session_ended_surfaces_modal_and_preserves_dirty_save_state() {
        let mut client = GameState::new();
        client.run_mode = RunMode::Playing;
        client.modal = None;
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_remote_player_connected = true;
        client.online_remote_player_ready = true;
        client.save_dirty = true;

        client.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::host_ended(
            "host closed app",
        ));

        assert_eq!(client.run_mode, RunMode::Paused);
        assert_eq!(client.modal, Some(ModalScreen::OnlineMultiplayer));
        assert_eq!(
            client.online_session_state,
            OnlineSessionUxState::Disconnected
        );
        assert!(client.save_dirty);
        assert!(!client.online_remote_player_connected);
        assert!(!client.online_remote_player_ready);
        assert!(client.message.contains("ended by host"));
        assert!(
            client
                .online_last_session_boundary_status
                .contains("HostEndedSession")
        );
        assert!(
            client
                .online_last_save_boundary_status
                .contains("Online save boundary")
        );
    }

    #[test]
    fn active_host_client_left_keeps_local_gameplay_and_save_authority_available() {
        let mut host = GameState::new();
        host.run_mode = RunMode::Playing;
        host.modal = None;
        host.online_session_state = OnlineSessionUxState::Connected;
        host.online_host_owns_save = true;
        host.online_player_slot = Some(1);
        host.online_remote_player_connected = true;
        host.online_remote_player_ready = true;
        host.save_dirty = true;

        host.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::client_left(
            "client quit",
        ));

        assert_eq!(host.run_mode, RunMode::Playing);
        assert_eq!(host.modal, None);
        assert_eq!(host.online_session_state, OnlineSessionUxState::Connected);
        assert!(host.save_dirty);
        assert!(host.can_write_local_save());
        assert!(
            host.online_last_save_boundary_status
                .contains("Save+Exit, local save, and local load are allowed")
        );
        assert!(
            host.message
                .contains("Host save/session remains local and safe")
        );
    }

    #[test]
    fn lan_vpn_qa_readiness_distinguishes_loopback_smoke_from_shareable_lan_config() {
        let mut same_machine = GameState::new();
        same_machine.online_descriptor_path = PathBuf::from("/tmp/drillgame-host.json");
        same_machine.online_host_bind_addr = "127.0.0.1:5000".parse().expect("bind");
        same_machine.online_host_advertise_addr = "127.0.0.1:5000".parse().expect("advertise");
        same_machine.online_client_bind_addr = "127.0.0.1:5001".parse().expect("client");

        let same_machine_status = OnlineLanVpnQaReadinessStatus::from_game(&same_machine);
        assert!(same_machine_status.ready_for_same_machine);
        assert!(!same_machine_status.ready_for_lan_or_vpn);
        assert!(same_machine_status.host_join_commands_visible);
        assert!(same_machine_status.status.contains("same_machine=yes"));
        assert!(same_machine_status.status.contains("lan_vpn=no"));

        let mut lan = same_machine;
        lan.online_host_bind_addr = "0.0.0.0:5252".parse().expect("lan bind");
        lan.online_host_advertise_addr = "192.168.50.25:5252".parse().expect("lan advertise");
        lan.online_client_bind_addr = "0.0.0.0:5253".parse().expect("lan client");
        let lan_status = OnlineLanVpnQaReadinessStatus::from_game(&lan);
        assert!(lan_status.ready_for_same_machine);
        assert!(lan_status.ready_for_lan_or_vpn);
        assert!(lan_status.lan_or_vpn_address_configured);
        assert!(lan_status.status.contains("lan_vpn=yes"));
        assert!(lan.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online LAN/VPN QA readiness") && line.contains("lan_vpn=yes")
        }));
    }

    #[test]
    fn soak_readiness_requires_runtime_shutdown_and_degraded_evidence_for_degraded_soak() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_remote_player_connected = true;
        game.online_gameplay_ticks = 120;
        game.online_last_sync_loop_status = "snapshot applied: players=2 cargo=yes".to_owned();
        game.online_last_session_boundary_status =
            OnlineSessionBoundaryStatus::client_left("prior leave check").status_line();

        let local = OnlineSoakReadinessStatus::from_game(&game);
        assert!(local.ready_for_local_soak);
        assert!(!local.ready_for_degraded_soak);
        assert!(local.runtime_sync_evidence);
        assert!(local.shutdown_recovery_evidence);

        game.online_gameplay_ticks = 300;
        game.online_last_failure_status =
            "Online failure: category=transport simulated degraded loss".to_owned();
        let degraded = OnlineSoakReadinessStatus::from_game(&game);
        assert!(degraded.ready_for_local_soak);
        assert!(degraded.ready_for_degraded_soak);
        assert!(degraded.degraded_soak_evidence);
        assert!(degraded.status.contains("degraded_soak=yes"));
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online soak readiness") && line.contains("degraded_soak=yes")
        }));
    }

    #[test]
    fn reconnect_attempt_decision_blocks_active_or_unknown_sessions_and_allows_preserved_context() {
        let mut active = GameState::new();
        active.online_session_state = OnlineSessionUxState::Connected;
        active.online_host_owns_save = false;
        active.online_player_slot = Some(2);
        active.save_dirty = true;

        let active_decision = OnlineReconnectAttemptDecision::from_game(&active);
        assert!(!active_decision.can_attempt);
        assert_eq!(
            active_decision.blocker,
            Some(OnlineReconnectBlocker::NoReconnectableSession)
        );
        assert!(active_decision.preserves_dirty_save_state);
        assert!(active_decision.preserves_role_slot);

        let mut unknown = GameState::new();
        unknown.online_session_state = OnlineSessionUxState::Disconnected;
        unknown.online_player_slot = None;
        let unknown_decision = OnlineReconnectAttemptDecision::from_game(&unknown);
        assert!(!unknown_decision.can_attempt);
        assert_eq!(
            unknown_decision.blocker,
            Some(OnlineReconnectBlocker::NoPlayerSlot)
        );
        assert!(unknown_decision.status.contains("can_attempt=no"));

        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Timeout;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.save_dirty = true;
        let client_decision = OnlineReconnectAttemptDecision::from_game(&client);
        assert!(client_decision.can_attempt);
        assert_eq!(client_decision.blocker, None);
        assert!(client_decision.preserves_dirty_save_state);
        assert!(client_decision.preserves_role_slot);
        assert!(client_decision.player_message.contains("preserved role"));
    }

    #[test]
    fn reconnect_policy_is_explicitly_unsupported_without_rejoin_context_and_preserves_dirty_flag()
    {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Disconnected;
        game.online_player_slot = None;
        game.save_dirty = true;

        let status = OnlineReconnectPolicyStatus::from_game(&game);

        assert_eq!(
            status.policy,
            OnlineReconnectPolicy::UnsupportedForFirstPlayableMvp
        );
        assert!(!status.can_attempt_rejoin);
        assert!(status.preserves_dirty_save_state);
        assert!(status.player_message.contains("not automatic"));
        assert!(status.status_line().contains("dirty_save_preserved=yes"));
    }

    #[test]
    fn manual_working_game_gate_passes_when_ui_runtime_sync_and_shutdown_evidence_are_present() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = None;
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.online_last_replicated_player_status = "updated remote player 2".to_owned();
        game.player.velocity_x = 0.25;
        game.online_last_terrain_status = "applied chunk (0,0) rev 2: 4 visible tiles".to_owned();
        game.online_last_replication_status = "host sent snapshot with cargo".to_owned();
        game.online_last_sync_loop_status =
            "snapshot applied: players=2 cargo=yes terrain=yes".to_owned();
        game.online_last_gameplay_sync_evidence_status =
            "Gameplay sync evidence: movement=good terrain=good cargo=good survival=good"
                .to_owned();
        game.online_last_session_boundary_status =
            OnlineSessionBoundaryStatus::client_left("prior manual check").status_line();
        game.refresh_online_save_boundary_status();

        let manual_status = OnlineManualWorkingGameGateStatus::from_game(&game);

        assert!(manual_status.ready);
        assert!(manual_status.two_instances_ready);
        assert!(manual_status.ui_host_join_ready);
        assert!(manual_status.ui_ready_start_ready);
        assert!(manual_status.movement_observable);
        assert!(manual_status.mining_observable);
        assert!(manual_status.survival_observable);
        assert!(manual_status.shutdown_safe);
        assert!(manual_status.failures_triaged);
        assert!(manual_status.status.contains("ready=yes"));
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online manual working-game gate") && line.contains("ready=yes")
        }));
    }

    #[test]
    fn local_persistence_decision_blocks_joined_client_and_allows_host() {
        let mut client = GameState::new();
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.save_dirty = true;

        let save_decision =
            OnlineLocalPersistenceDecision::from_game(&client, OnlineLocalPersistenceAction::Save);
        let load_decision =
            OnlineLocalPersistenceDecision::from_game(&client, OnlineLocalPersistenceAction::Load);

        assert!(!save_decision.allowed);
        assert!(!load_decision.allowed);
        assert_eq!(
            save_decision.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
        assert!(save_decision.status.contains("allowed=no"));
        assert!(load_decision.player_message.contains("Load blocked"));
        assert!(client.block_joined_client_save());
        assert!(client.message.contains("Save blocked"));

        let mut host = GameState::new();
        host.online_session_state = OnlineSessionUxState::Connected;
        host.online_host_owns_save = true;
        host.online_player_slot = Some(1);
        let host_decision =
            OnlineLocalPersistenceDecision::from_game(&host, OnlineLocalPersistenceAction::Save);
        assert!(host_decision.allowed);
        assert_eq!(
            host_decision.save_authority,
            OnlineSaveAuthority::LocalPlayer
        );
        assert!(!host.block_joined_client_save());
    }

    #[test]
    fn leave_end_safety_status_tracks_request_pending_ack_and_save_guard() {
        let mut host = GameState::new();
        host.online_session_state = OnlineSessionUxState::Connected;
        host.online_host_owns_save = true;
        host.online_player_slot = Some(1);
        host.online_remote_player_connected = true;
        host.save_dirty = true;

        let active = OnlineLeaveEndSafetyStatus::from_game(&host);
        assert!(active.session_active);
        assert!(active.shutdown_requestable);
        assert!(active.force_kill_not_required);
        assert!(active.save_corruption_guarded);

        assert!(host.request_online_shutdown_from_gameplay_exit());
        let pending = OnlineLeaveEndSafetyStatus::from_game(&host);
        assert!(pending.shutdown_pending);
        assert!(!pending.shutdown_requestable);
        assert!(pending.force_kill_not_required);
        assert!(pending.status.contains("pending=yes"));

        host.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(
            OnlineShutdownSummary::from_notification("descriptor host", true, true, None, true),
        ));
        let acknowledged = OnlineLeaveEndSafetyStatus::from_game(&host);
        assert!(acknowledged.shutdown_acknowledged);
        assert!(acknowledged.peer_notification_evidence);
        assert!(acknowledged.force_kill_not_required);
        assert!(acknowledged.status.contains("force_kill_not_required=yes"));
    }

    #[test]
    fn gameplay_clarity_presentation_summarizes_local_remote_session_and_save_state() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = None;
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.player.y = TILE_SIZE * 12.0;
        game.player.fuel = 77.0;
        game.player.hull = 88.0;
        game.player.credits = 123;
        game.player.cargo.insert(MineralKind::Copper, 2);

        let clarity = OnlineGameplayClarityPresentation::from_game(&game);
        let hud_lines = game.online_gameplay_hud_presentation().lines();

        assert!(clarity.visible);
        assert!(clarity.hud_clear_enough);
        assert!(clarity.local_player_line.contains("fuel=77"));
        assert!(clarity.local_player_line.contains("cargo=2"));
        assert!(clarity.remote_player_lines[0].contains("canonical session presentation"));
        assert!(clarity.session_state_line.contains("local_save=yes"));
        assert!(clarity.status.contains("clear=yes"));
        assert!(hud_lines.iter().any(|line| line.contains("Local host p1")));
        assert!(
            hud_lines
                .iter()
                .any(|line| line.contains("canonical session presentation"))
        );
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online gameplay HUD clarity") && line.contains("clear=yes")
        }));
    }

    #[test]
    fn directional_gameplay_sync_status_tracks_host_and_client_visibility_evidence() {
        let mut host = GameState::new();
        host.run_mode = RunMode::Playing;
        host.modal = None;
        host.online_session_state = OnlineSessionUxState::Connected;
        host.online_host_owns_save = true;
        host.online_player_slot = Some(1);
        host.online_remote_player_connected = true;
        host.player.velocity_x = 1.0;
        host.apply_online_replicated_player_status("updated remote player 2 from command packet");
        host.apply_online_terrain_status("applied chunk (0,0) rev 3: 6 visible tiles");

        let host_status = OnlineDirectionalGameplaySyncStatus::from_game(&host);
        assert!(host_status.gameplay_active);
        assert!(host_status.host_movement_visible);
        assert!(host_status.client_movement_visible);
        assert!(host_status.host_mining_visible);
        assert!(host_status.client_mining_visible);
        assert!(host_status.host_sees_client);
        assert!(host_status.both_directions_visible);
        assert!(host_status.status.contains("both_directions=yes"));
        assert!(
            host.online_gameplay_hud_presentation()
                .lines()
                .iter()
                .any(|line| line.contains("Directional sync") && line.contains("client move=yes"))
        );

        let mut client = host.clone();
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        let client_status = OnlineDirectionalGameplaySyncStatus::from_game(&client);
        assert!(client_status.client_sees_host);
        assert!(client_status.host_movement_visible);
        assert!(client_status.client_movement_visible);
        assert!(client_status.both_directions_visible);
    }

    #[test]
    fn manual_working_game_gate_blocks_when_ui_or_sync_evidence_is_missing() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.refresh_online_save_boundary_status();

        let manual_status = OnlineManualWorkingGameGateStatus::from_game(&game);

        assert!(!manual_status.ready);
        assert!(manual_status.two_instances_ready);
        assert!(manual_status.ui_host_join_ready);
        assert!(manual_status.ui_ready_start_ready);
        assert!(!manual_status.movement_observable);
        assert!(!manual_status.mining_observable);
        assert!(!manual_status.survival_observable);
        assert!(!manual_status.shutdown_safe);
        assert!(manual_status.status.contains("movement=no"));
        assert!(manual_status.status.contains("mining=no"));
    }

    #[test]
    fn manual_working_game_gate_accepts_joined_client_ui_path_and_discard_or_cancel_save_policy() {
        let mut client = GameState::new();
        client.run_mode = RunMode::Playing;
        client.modal = None;
        client.online_session_state = OnlineSessionUxState::Connected;
        client.online_host_owns_save = false;
        client.online_player_slot = Some(2);
        client.online_local_ready = true;
        client.online_remote_player_ready = true;
        client.online_remote_player_connected = true;
        client.save_dirty = true;
        client.player.velocity_x = -0.5;
        client.player.fuel = 55.0;
        client.player.hull = 66.0;
        client.online_last_replicated_player_status = "updated host player".to_owned();
        client.online_last_terrain_status = "applied chunk (0,0) rev 3: 5 visible tiles".to_owned();
        client.online_last_replication_status = "client received snapshot with cargo".to_owned();
        client.online_last_sync_loop_status =
            "snapshot applied: players=2 cargo=yes terrain=yes".to_owned();
        client.online_last_gameplay_sync_evidence_status =
            "Gameplay sync evidence: movement=good terrain=good cargo=good survival=good"
                .to_owned();
        client.online_last_session_boundary_status =
            OnlineSessionBoundaryStatus::host_ended("previous shutdown check").status_line();
        client.refresh_online_save_boundary_status();

        let manual_status = OnlineManualWorkingGameGateStatus::from_game(&client);
        let save_boundary = OnlineSaveBoundaryStatus::from_game(&client);

        assert!(manual_status.ready);
        assert!(manual_status.ui_host_join_ready);
        assert_eq!(
            save_boundary.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
        assert_eq!(
            save_boundary.unsaved_exit_action,
            OnlineUnsavedExitAction::DiscardOrCancelOnly
        );
        assert!(manual_status.status.contains("role=client"));
        assert!(manual_status.status.contains("shutdown_safe=yes"));
    }

    #[test]
    fn sustained_mining_session_status_requires_runtime_sync_and_save_boundaries() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_remote_player_connected = true;
        game.online_last_replicated_player_status = "updated remote player".to_owned();
        game.online_last_terrain_status = "answered chunk (0,0) rev 1".to_owned();
        game.online_last_replication_status = "host sent snapshot with cargo".to_owned();
        game.online_last_sync_loop_status =
            "snapshot applied: players=2 cargo=yes terrain=yes".to_owned();
        game.online_last_session_boundary_status =
            OnlineSessionBoundaryStatus::client_left("previous peer left").status_line();
        game.refresh_online_save_boundary_status();
        game.update_ticks = u64::from(crate::multiplayer::SIMULATION_HZ) * 60 * 5;

        let status = OnlineSustainedMiningSessionStatus::from_game(&game);

        assert!(status.playable);
        assert!(status.gameplay_active);
        assert!(status.movement_visible);
        assert!(status.terrain_visible);
        assert!(status.cargo_or_economy_visible);
        assert!(status.survival_visible);
        assert!(status.session_boundary_safe);
        assert!(status.save_boundary_safe);
        assert!(status.status.contains("playable=yes"));
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online sustained mining session") && line.contains("playable=yes")
        }));
    }

    #[test]
    fn online_save_boundary_status_lines_refresh_through_runtime_statuses() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_host_owns_save = false;
        game.save_dirty = true;
        game.refresh_online_runtime_statuses();

        assert!(
            game.online_last_save_boundary_status
                .contains("Save+Exit is blocked")
        );
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online save boundary") && line.contains("Discard or Cancel")
        }));

        game.online_host_owns_save = true;
        game.refresh_online_save_boundary_status();
        assert!(
            game.online_last_save_boundary_status
                .contains("Save+Exit, local save, and local load are allowed")
        );
    }

    #[test]
    fn gameplay_domain_status_summarizes_multiplayer_playtest_domains() {
        let mut game = GameState::new();
        game.online_player_slot = Some(2);
        game.online_host_owns_save = false;
        game.online_session_state = OnlineSessionUxState::Reconnecting;
        game.apply_online_replicated_player_status(
            "tick 11: applied player 2 pos=(5.0,6.0) fuel=44 hull=55 credits=66 cargo=7",
        );
        game.apply_online_terrain_status("applied chunk (1,2) rev 3: 9 visible tiles");
        game.apply_online_sync_loop_status(OnlineSyncLoopStatus::snapshot(2, true));
        game.apply_online_authority_correction_status(
            &OnlineAuthorityCorrectionPresentation::from_plan(
                crate::session::CorrectionPlan::Smooth,
                true,
            ),
        );
        game.refresh_online_runtime_statuses();

        let status = OnlineGameplayDomainStatus::from_game(&game);
        assert_eq!(status.movement, "visible");
        assert_eq!(status.terrain, "visible");
        assert_eq!(status.cargo_economy, "visible");
        assert_eq!(status.survival, "visible");
        assert_eq!(status.menu_boundary, "visible");
        assert_eq!(status.reconnect_recovery, "visible");
        assert_eq!(status.authority_correction, "visible");
        assert!(status.status.contains("role=client"));
        assert!(status.status.contains("slot=2"));
    }

    #[test]
    fn gameplay_domain_status_is_refreshed_and_exposed_in_status_lines() {
        let mut game = GameState::new();
        game.apply_online_replicated_player_status(
            "tick 12: applied player 1 pos=(1.0,1.0) fuel=90 hull=95 credits=10 cargo=2",
        );
        assert!(
            game.online_last_gameplay_domain_status
                .contains("movement=visible")
        );
        assert!(
            game.online_last_gameplay_domain_status
                .contains("cargo_economy=visible")
        );

        let lines = game.online_multiplayer_status_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Online gameplay domains")
                    && line.contains("movement=visible")
                    && line.contains("survival=visible"))
        );

        game.clear_online_diagnostics();
        assert!(game.online_last_gameplay_domain_status.is_empty());
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online gameplay domains")
                    && line.contains("movement=missing"))
        );
    }

    #[test]
    fn gameplay_domain_status_keeps_pause_menu_boundary_visible_even_without_network_evidence() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = None;

        let status = OnlineGameplayDomainStatus::from_game(&game);

        assert_eq!(status.menu_boundary, "visible");
        assert!(status.status.contains("run_mode=Playing"));
        assert!(status.status.contains("modal=none"));
    }

    #[test]
    fn live_verification_status_reports_visible_online_gameplay_systems() {
        let mut game = GameState::new();
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.online_session_state = OnlineSessionUxState::Connected;
        game.apply_online_replicated_player_status(
            "tick 5: applied player 2 pos=(10.0,20.0) fuel=80 hull=90 credits=123 cargo=4",
        );
        game.apply_online_terrain_status("applied chunk (0,0) rev 2: 3 visible tiles");
        game.apply_online_sync_loop_status(OnlineSyncLoopStatus::snapshot(2, true));
        game.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::client_left(
            "test boundary",
        ));

        let status = OnlineLiveVerificationStatus::from_game(&game);
        assert!(status.movement_visible);
        assert!(status.terrain_visible);
        assert!(status.cargo_economy_visible);
        assert!(status.survival_visible);
        assert!(status.ready_start_visible);
        assert!(status.session_boundary_visible);
        assert!(status.status.contains("movement=yes"));
        assert!(status.status.contains("cargo_economy=yes"));
        assert!(status.status.contains("session_boundary=yes"));
    }

    #[test]
    fn live_verification_status_is_exposed_and_refreshed_by_runtime_diagnostics() {
        let mut game = GameState::new();
        game.apply_online_replicated_player_status(
            "tick 7: applied player 1 pos=(1.0,2.0) fuel=60 hull=70 credits=8 cargo=1",
        );
        assert!(
            game.online_last_live_verification_status
                .contains("movement=yes")
        );
        assert!(
            game.online_last_live_verification_status
                .contains("survival=yes")
        );
        assert!(
            game.online_last_live_verification_status
                .contains("cargo_economy=yes")
        );

        let lines = game.online_multiplayer_status_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Online live verification")
                    && line.contains("movement=yes")
                    && line.contains("survival=yes"))
        );

        game.clear_online_diagnostics();
        assert!(game.online_last_live_verification_status.is_empty());
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online live verification")
                    && line.contains("movement=no"))
        );
    }

    #[test]
    fn live_verification_status_tracks_ready_start_gate_before_gameplay() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_remote_player_connected = true;
        game.online_local_ready = false;
        game.online_remote_player_ready = true;
        let blocked = OnlineLiveVerificationStatus::from_game(&game);
        assert!(blocked.ready_start_visible);
        assert!(blocked.status.contains("ready_start=yes"));

        game.online_host_owns_save = true;
        game.online_local_ready = true;
        let ready = OnlineLiveVerificationStatus::from_game(&game);
        assert!(ready.ready_start_visible);
        assert!(game.online_gameplay_start_gate().ready);
    }

    #[test]
    fn online_ownership_status_explains_host_and_joined_client_save_authority() {
        let mut host = GameState::new();
        host.online_player_name = "Host Ada".to_owned();
        host.online_player_slot = Some(1);
        host.online_host_owns_save = true;
        host.online_session_state = OnlineSessionUxState::Connected;
        let host_status = OnlineOwnershipStatus::from_game(&host);
        assert_eq!(host_status.identity, "Host Ada");
        assert_eq!(host_status.slot, Some(1));
        assert_eq!(host_status.role_label, "host");
        assert_eq!(host_status.save_authority, OnlineSaveAuthority::LocalPlayer);
        assert!(host_status.reconnect_allowed);
        assert!(host_status.status_line().contains("host/save owner"));

        let mut client = GameState::new();
        client.online_player_name = "Client Bert".to_owned();
        client.online_player_slot = Some(2);
        client.online_host_owns_save = false;
        client.online_session_state = OnlineSessionUxState::Connected;
        let client_status = OnlineOwnershipStatus::from_game(&client);
        assert_eq!(client_status.identity, "Client Bert");
        assert_eq!(client_status.slot, Some(2));
        assert_eq!(client_status.role_label, "client");
        assert_eq!(
            client_status.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
        assert!(client_status.reconnect_allowed);
        assert!(
            client_status
                .status_line()
                .contains("host owns the authoritative save")
        );
        assert!(
            client_status
                .status_line()
                .contains("local writes stay blocked")
        );
    }

    #[test]
    fn ownership_status_refreshes_through_join_reconnect_failure_and_shutdown_reducers() {
        let mut game = GameState::new();
        game.online_player_name = "Joined Miner".to_owned();
        game.apply_online_network_task_result(OnlineNetworkTaskResult::JoinedDescriptor(
            RealOnlineSessionUxSnapshot::from_descriptor_client_connected(
                Some(2),
                &default_online_descriptor_path(),
            ),
        ));
        assert!(game.online_last_ownership_status.contains("Joined Miner"));
        assert!(game.online_last_ownership_status.contains("slot=2"));
        assert!(game.online_last_ownership_status.contains("role=client"));
        assert!(
            game.online_last_ownership_status
                .contains("save_authority=RemoteHost")
        );

        game.apply_online_network_task_result(OnlineNetworkTaskResult::Reconnected(
            RealOnlineSessionUxSnapshot::from_reconnect(Some(2)),
        ));
        assert!(
            game.online_last_ownership_status
                .contains("reconnect_context=preserved")
        );

        game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
            "session token reconnect failed".to_owned(),
        ));
        assert!(game.online_last_failure_status.contains("Reconnect"));
        assert!(game.online_last_ownership_status.contains("Joined Miner"));
        assert!(game.online_last_ownership_status.contains("RemoteHost"));

        game.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(
            OnlineShutdownSummary::offline(),
        ));
        assert!(
            game.online_last_session_boundary_status
                .contains("ShutdownAcknowledged")
        );
        assert!(game.online_last_ownership_status.contains("Joined Miner"));
    }

    #[test]
    fn online_status_lines_use_structured_ownership_status() {
        let mut game = GameState::new();
        game.online_player_name = "Status Miner".to_owned();
        game.online_player_slot = Some(1);
        game.online_host_owns_save = true;
        game.refresh_online_ownership_status();

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(|line| line.contains("Online ownership")
            && line.contains("Status Miner")
            && line.contains("save_authority=LocalPlayer")));
    }

    #[test]
    fn online_failure_status_classifies_player_actionable_categories_and_hints() {
        let cases = [
            (
                "protocol version mismatch",
                OnlineFailureCategory::VersionMismatch,
                "same build",
            ),
            (
                "certificate verify failed",
                OnlineFailureCategory::Certificate,
                "current descriptor",
            ),
            (
                "descriptor JSON parse error",
                OnlineFailureCategory::Descriptor,
                "descriptor path",
            ),
            (
                "connection timed out",
                OnlineFailureCategory::Timeout,
                "UDP firewall",
            ),
            (
                "connection refused",
                OnlineFailureCategory::RefusedOrUnreachable,
                "listener is active",
            ),
            (
                "session token reconnect failed",
                OnlineFailureCategory::Reconnect,
                "rejoin as a new client",
            ),
            (
                "reliable channel closed",
                OnlineFailureCategory::SessionEnded,
                "new session",
            ),
        ];

        for (raw_error, expected_category, expected_hint) in cases {
            let status = OnlineFailureStatus::classify(raw_error);
            assert_eq!(status.category, expected_category);
            assert!(status.status_line().contains(expected_hint));
            assert!(!status.player_message.contains(raw_error));
        }

        let unknown = OnlineFailureStatus::classify("weird low-level error");
        assert_eq!(unknown.category, OnlineFailureCategory::Unknown);
        assert!(unknown.player_message.contains("weird low-level error"));
        assert!(unknown.status_line().contains("Capture the exact error"));
    }

    #[test]
    fn online_failed_task_stores_failure_help_in_status_lines() {
        let mut game = GameState::new();
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.online_last_replication_status = "stale replication".to_owned();

        let transition = game.apply_online_network_task_result(OnlineNetworkTaskResult::Failed(
            "connection timed out".to_owned(),
        ));

        assert_eq!(transition.kind, OnlineTaskResultTransitionKind::Failed);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Error);
        assert!(!game.online_local_ready);
        assert!(!game.online_remote_player_ready);
        assert!(!game.online_remote_player_connected);
        assert!(game.online_last_replication_status.is_empty());
        assert!(game.online_last_failure_status.contains("Timeout"));
        assert!(game.online_last_failure_status.contains("UDP firewall"));
        assert!(game.message.contains("Connection timed out"));
        assert!(game.online_multiplayer_status_lines().iter().any(|line| {
            line.contains("Online failure help")
                && line.contains("Timeout")
                && line.contains("LAN/VPN")
        }));
    }

    #[test]
    fn online_failure_status_message_remains_backward_compatible_wrapper() {
        assert_eq!(
            GameState::online_failure_status_message("connection refused"),
            OnlineFailureStatus::classify("connection refused").player_message
        );
    }

    #[test]
    fn online_session_boundary_statuses_are_player_facing_and_preserve_context() {
        let host_ended = OnlineSessionBoundaryStatus::host_ended("host clicked End Session");
        assert_eq!(
            host_ended.cause,
            OnlineSessionBoundaryCause::HostEndedSession
        );
        assert!(!host_ended.remote_connected);
        assert!(!host_ended.local_session_active);
        assert!(host_ended.player_message.contains("ended by host"));
        assert!(host_ended.status_line().contains("reconnect"));

        let client_left = OnlineSessionBoundaryStatus::client_left("client quit");
        assert_eq!(
            client_left.cause,
            OnlineSessionBoundaryCause::ClientLeftSession
        );
        assert!(!client_left.remote_connected);
        assert!(client_left.local_session_active);
        assert!(
            client_left
                .player_message
                .contains("Host save/session remains local and safe")
        );

        let transport_closed = OnlineSessionBoundaryStatus::transport_closed("network reset");
        assert_eq!(
            transport_closed.cause,
            OnlineSessionBoundaryCause::TransportClosed
        );
        assert!(transport_closed.player_message.contains("transport closed"));
        assert!(
            transport_closed
                .player_message
                .contains("save policy remains unchanged")
        );
    }

    #[test]
    fn applying_session_boundary_status_clears_remote_runtime_state_but_keeps_message_visible() {
        let mut game = GameState::new();
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;

        game.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::client_left(
            "test disconnect",
        ));

        assert!(!game.online_remote_player_ready);
        assert!(!game.online_remote_player_connected);
        assert!(game.message.contains("test disconnect"));
        assert!(
            game.online_last_session_boundary_status
                .contains("ClientLeftSession")
        );
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online session boundary")
                    && line.contains("Host save/session remains local and safe"))
        );
    }

    #[test]
    fn shutdown_result_preserves_boundary_status_after_diagnostic_clear() {
        let mut game = GameState::new();
        game.online_last_replication_status = "old replication".to_owned();
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;

        let transition = game.apply_online_network_task_result(OnlineNetworkTaskResult::Shutdown(
            OnlineShutdownSummary::offline(),
        ));

        assert_eq!(transition.kind, OnlineTaskResultTransitionKind::Shutdown);
        assert!(game.online_last_replication_status.is_empty());
        assert!(
            game.online_last_session_boundary_status
                .contains("ShutdownAcknowledged")
        );
        assert!(game.message.contains("local save/session state preserved"));
        assert!(
            game.online_multiplayer_status_lines()
                .iter()
                .any(|line| line.contains("Online session boundary")
                    && line.contains("ShutdownAcknowledged"))
        );
    }

    #[test]
    fn gameplay_exit_requests_online_transport_shutdown_once() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;

        assert!(game.request_online_shutdown_from_gameplay_exit());

        assert_eq!(
            game.online_network_task_request,
            Some(OnlineNetworkTaskRequest::Shutdown)
        );
        assert!(!game.online_local_ready);
        assert!(!game.online_remote_player_ready);
        assert!(!game.online_remote_player_connected);
        assert!(
            game.online_last_session_boundary_status
                .contains("LocalShutdownRequested")
        );
        assert!(!game.request_online_shutdown_from_gameplay_exit());
    }

    #[test]
    fn online_pending_back_cancels_queued_network_task() {
        let mut game = GameState::new();
        game.modal = Some(ModalScreen::OnlineMultiplayer);
        game.online_session_state = OnlineSessionUxState::Hosting;
        game.online_network_task_request = Some(OnlineNetworkTaskRequest::HostDescriptorFile {
            path: game.online_descriptor_path.clone(),
        });
        game.selected_menu_item = 14;

        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.0,
        );

        assert_eq!(game.modal, None);
        assert_eq!(game.online_network_task_request, None);
        assert_eq!(game.online_session_state, OnlineSessionUxState::Idle);
        assert!(game.message.contains("pending network task canceled"));
    }

    #[test]
    fn online_sync_loop_status_reports_snapshot_delta_terrain_and_keyframe_paths() {
        let snapshot_status = OnlineSyncLoopStatus::snapshot(2, true);
        assert!(snapshot_status.snapshot_applied);
        assert!(snapshot_status.cargo_applied);
        assert!(snapshot_status.status.contains("snapshot applied"));
        assert!(snapshot_status.status.contains("cargo=yes"));

        let delta_status = OnlineSyncLoopStatus::player_delta(1, 1);
        assert!(delta_status.player_delta_applied);
        assert!(delta_status.status.contains("visible_remote_updates=1"));

        let terrain_status = OnlineSyncLoopStatus::terrain(4, 3);
        assert!(terrain_status.terrain_applied);
        assert!(terrain_status.status.contains("visible_tiles=3"));

        let keyframe_status = OnlineSyncLoopStatus::keyframe_required();
        assert!(!keyframe_status.snapshot_applied);
        assert!(keyframe_status.status.contains("waiting for host snapshot"));
    }

    #[test]
    fn local_snapshot_application_uses_joined_player_slot_not_first_host_player() {
        let mut game = GameState::new();
        game.online_player_slot = Some(2);
        let snapshot = crate::multiplayer::NetworkWorldSnapshot {
            tick: crate::multiplayer::SimulationTick::new(45),
            players: vec![
                crate::multiplayer::NetworkPlayerSnapshot {
                    player_id: crate::multiplayer::PlayerId::new(1),
                    x: 10.0,
                    y: 20.0,
                    velocity_x: 1.0,
                    velocity_y: 0.0,
                    fuel: 90.0,
                    hull: 80.0,
                    credits: 70,
                    cargo_used: 0,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                    scanner_cooldown_seconds: 0.0,
                },
                crate::multiplayer::NetworkPlayerSnapshot {
                    player_id: crate::multiplayer::PlayerId::new(2),
                    x: 111.0,
                    y: 222.0,
                    velocity_x: -1.0,
                    velocity_y: 2.0,
                    fuel: 55.0,
                    hull: 66.0,
                    credits: 77,
                    cargo_used: 0,
                    cargo: BTreeMap::new(),
                    artifacts: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot {
                        fuel_capacity: 140.0,
                        cargo_capacity: 25,
                        fuel_tank_level: 2,
                        cargo_bay_level: 3,
                        drill_strength: 4,
                        engine_level: 5,
                        hull_level: 6,
                        radiator_level: 7,
                        scanner_level: 8,
                        bombs: 9,
                        loan_debt: 10,
                        insured: true,
                        insurance_tier: 2,
                        crafted_bulkheads: 1,
                        crafted_sorters: 2,
                        signal_relay_kits: 3,
                        survey_drone_kits: 4,
                        cargo_lift_kits: 5,
                        tunnel_support_kits: 6,
                        pump_station_kits: 7,
                        ore_processor_kits: 8,
                    },
                    scanner_cooldown_seconds: 0.5,
                },
            ],
        };

        assert!(apply_network_snapshot_local_player_presentation_adapter(
            &mut game, &snapshot
        ));

        assert!((game.player.x - 111.0).abs() < f32::EPSILON);
        assert!((game.player.y - 222.0).abs() < f32::EPSILON);
        assert!((game.player.fuel - 55.0).abs() < f32::EPSILON);
        assert_eq!(game.player.credits, 77);
        assert!((game.scanner_cooldown_seconds - 0.5).abs() < f32::EPSILON);
        assert!((game.player.fuel_capacity - 140.0).abs() < f32::EPSILON);
        assert_eq!(game.player.cargo_capacity, 25);
        assert_eq!(game.player.fuel_tank_level, 2);
        assert_eq!(game.player.cargo_bay_level, 3);
        assert_eq!(game.player.drill_strength, 4);
        assert_eq!(game.player.engine_level, 5);
        assert_eq!(game.player.hull_level, 6);
        assert_eq!(game.player.radiator_level, 7);
        assert_eq!(game.player.scanner_level, 8);
        assert_eq!(game.player.bombs, 9);
        assert_eq!(game.player.loan_debt, 10);
        assert!(game.player.insured);
        assert_eq!(game.player.insurance_tier, 2);
        assert_eq!(game.player.signal_relay_kits, 3);
    }

    #[test]
    fn local_snapshot_application_without_joined_slot_does_not_mutate_single_player_state() {
        let mut game = GameState::new();
        game.online_player_slot = None;
        game.player.x = 7.0;
        game.player.y = 8.0;
        game.player.fuel = 9.0;
        game.player.credits = 10;
        let snapshot = crate::multiplayer::NetworkWorldSnapshot {
            tick: crate::multiplayer::SimulationTick::new(46),
            players: vec![crate::multiplayer::NetworkPlayerSnapshot {
                player_id: crate::multiplayer::PlayerId::new(99),
                x: 1000.0,
                y: 2000.0,
                velocity_x: 3.0,
                velocity_y: 4.0,
                fuel: 5.0,
                hull: 6.0,
                credits: 700,
                cargo_used: 0,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 2.0,
            }],
        };

        assert!(!apply_network_snapshot_local_player_presentation_adapter(
            &mut game, &snapshot
        ));

        assert!((game.player.x - 7.0).abs() < f32::EPSILON);
        assert!((game.player.y - 8.0).abs() < f32::EPSILON);
        assert!((game.player.fuel - 9.0).abs() < f32::EPSILON);
        assert_eq!(game.player.credits, 10);
    }

    #[test]
    fn local_snapshot_presentation_adapter_never_falls_back_to_host_player() {
        let mut game = GameState::new();
        game.online_session_state = OnlineSessionUxState::Connected;
        game.online_player_slot = Some(2);
        let snapshot = crate::multiplayer::NetworkWorldSnapshot {
            tick: crate::multiplayer::SimulationTick::new(47),
            players: vec![crate::multiplayer::NetworkPlayerSnapshot {
                player_id: crate::multiplayer::PlayerId::new(1),
                x: 333.0,
                y: 444.0,
                velocity_x: 0.0,
                velocity_y: 0.0,
                fuel: 55.0,
                hull: 66.0,
                credits: 77,
                cargo_used: 0,
                cargo: BTreeMap::new(),
                artifacts: BTreeMap::new(),
                materials: BTreeMap::new(),
                loadout: crate::multiplayer::NetworkPlayerLoadoutSnapshot::default(),
                scanner_cooldown_seconds: 0.0,
            }],
        };

        assert!(!apply_network_snapshot_local_player_presentation_adapter(
            &mut game, &snapshot
        ));

        assert!((game.player.x - PLAYER_SPAWN_X).abs() < f32::EPSILON);
        assert_eq!(
            game.player.credits,
            Player::new(PLAYER_SPAWN_X, PLAYER_SPAWN_Y).credits
        );
    }

    #[test]
    fn terrain_chunk_application_reports_visible_sync_loop_status() {
        let mut game = GameState::new();
        let position = TilePosition { x: 3, y: 4 };
        let before = game.terrain.tile(position).expect("tile exists");
        let new_kind = if before.kind == TileKind::Dirt {
            TileKind::Stone
        } else {
            TileKind::Dirt
        };
        let changed = apply_network_terrain_chunk_to_game(
            &mut game,
            0,
            0,
            9,
            &[crate::multiplayer::NetworkTerrainTile {
                x: position.x,
                y: position.y,
                kind: new_kind,
                durability: before.durability,
            }],
        );

        assert_eq!(changed, 1);
        assert!(game.online_last_terrain_status.contains("visible tiles"));
        assert!(
            game.online_last_sync_loop_status
                .contains("terrain chunk applied")
        );
        assert!(
            game.online_last_sync_loop_status
                .contains("visible_tiles=1")
        );
    }

    #[test]
    fn online_status_lines_include_sync_loop_status_for_snapshot_delta_terrain_cargo() {
        let mut game = GameState::new();
        game.apply_online_sync_loop_status(OnlineSyncLoopStatus::snapshot(2, true));

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(|line| line.contains("Online sync loop")
            && line.contains("snapshot applied")
            && line.contains("cargo=yes")));
    }

    #[test]
    fn online_authority_correction_presentation_explains_smooth_and_snap_behavior() {
        let none = OnlineAuthorityCorrectionPresentation::from_plan(
            crate::session::CorrectionPlan::None,
            false,
        );
        assert_eq!(none.correction_feel, OnlineCorrectionFeel::NoCorrection);
        assert!(none.status_line().contains("in sync"));

        let smooth = OnlineAuthorityCorrectionPresentation::from_plan(
            crate::session::CorrectionPlan::Smooth,
            false,
        );
        assert_eq!(
            smooth.correction_feel,
            OnlineCorrectionFeel::SmoothReconcile
        );
        assert!(smooth.status_line().contains("small prediction drift"));
        assert!(smooth.status_line().contains("smooth reconcile"));

        let snap = OnlineAuthorityCorrectionPresentation::from_plan(
            crate::session::CorrectionPlan::Snap,
            true,
        );
        assert_eq!(
            snap.correction_feel,
            OnlineCorrectionFeel::AuthoritativeSnap
        );
        assert!(snap.status_line().contains("large prediction drift"));
        assert!(snap.status_line().contains("snap_applied=true"));
    }

    #[test]
    fn online_status_lines_show_host_authority_and_correction_feel() {
        let mut game = GameState::new();
        let presentation = OnlineAuthorityCorrectionPresentation::from_plan(
            crate::session::CorrectionPlan::Smooth,
            false,
        );
        game.apply_online_authority_correction_status(&presentation);

        let lines = game.online_multiplayer_status_lines();
        assert!(lines.iter().any(
            |line| line.contains("Online authority") && line.contains("small prediction drift")
        ));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Online correction feel")
                    && line.contains("smooth reconcile")
                    && line.contains("host authoritative simulation"))
        );
    }

    #[test]
    fn online_correction_snapshot_status_is_cleared_with_diagnostics() {
        let mut game = GameState::new();
        game.apply_online_authority_correction_status(
            &OnlineAuthorityCorrectionPresentation::from_plan(
                crate::session::CorrectionPlan::Snap,
                true,
            ),
        );
        assert!(!game.online_last_authority_status.is_empty());
        assert!(!game.online_last_correction_status.is_empty());

        game.clear_online_diagnostics();
        assert!(game.online_last_authority_status.is_empty());
        assert!(game.online_last_correction_status.is_empty());
        let lines = game.online_multiplayer_status_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Online correction feel")
                    && line.contains("small offsets smooth, large offsets snap"))
        );
    }

    #[test]
    fn correction_ux_snapshot_uses_player_facing_authority_language() {
        let snapshot = RealOnlineSessionUxSnapshot::from_correction(
            SocketDrivenCorrectionSummary {
                snapshot_replicated: true,
                authoritative_tick: crate::multiplayer::SimulationTick::new(77),
                correction_plan: crate::session::CorrectionPlan::Snap,
                presentation_x: 12.0,
                presentation_y: 34.0,
                snap_applied: true,
            },
            Some(2),
        );
        assert_eq!(snapshot.state, OnlineSessionUxState::Connected);
        assert!(snapshot.status_message.contains("Host authority"));
        assert!(snapshot.status_message.contains("large prediction drift"));
        assert!(snapshot.status_message.contains("snap_applied=true"));
    }

    #[test]
    fn online_lobby_participant_lines_report_names_slots_ready_and_connection() {
        let mut game = GameState::new();
        game.online_player_name = "Ada".to_owned();
        game.online_remote_player_name = Some("Bert".to_owned());
        game.online_player_slot = Some(1);
        game.online_host_owns_save = true;
        game.online_local_ready = true;
        game.online_remote_player_ready = true;
        game.online_remote_player_connected = true;
        game.online_session_state = OnlineSessionUxState::Connected;

        let lines = game.online_lobby_participant_lines();
        assert!(lines.iter().any(|line| line.contains("Ada")));
        assert!(lines.iter().any(|line| line.contains("slot 1")));
        assert!(lines.iter().any(|line| line.contains("role host")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("save_authority LocalPlayer"))
        );
        assert!(lines.iter().any(|line| line.contains("Bert")));
        assert!(lines.iter().any(|line| line.contains("slot 2")));
        assert!(lines.iter().any(|line| line.contains("connected yes")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Lobby start gate") && line.contains("ready=yes"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("host owns save/session authority"))
        );
    }

    #[test]
    fn joined_client_lobby_presentation_is_self_contained_without_cli_help() {
        let mut game = GameState::new();
        game.apply_real_online_session_ux(
            RealOnlineSessionUxSnapshot::from_descriptor_client_connected(
                Some(2),
                &default_online_descriptor_path(),
            ),
        );
        game.online_player_name = "Joined Miner".to_owned();
        game.apply_online_remote_identity(crate::multiplayer::PlayerId::new(1), "Host Miner");
        game.apply_online_remote_ready_state(crate::multiplayer::PlayerId::new(1), true);
        game.online_local_ready = true;

        let presentation = game.online_lobby_presentation();
        assert_eq!(presentation.local.name, "Joined Miner");
        assert_eq!(presentation.local.slot, Some(2));
        assert_eq!(presentation.local.role_label, "client");
        assert_eq!(
            presentation.local.save_authority,
            OnlineSaveAuthority::RemoteHost
        );
        assert_eq!(presentation.remote.name, "Host Miner");
        assert_eq!(presentation.remote.slot, Some(1));
        assert_eq!(presentation.remote.role_label, "host");
        assert_eq!(
            presentation.remote.save_authority,
            OnlineSaveAuthority::LocalPlayer
        );
        assert!(!presentation.start_gate.ready);
        assert_eq!(
            presentation.start_gate.blocker,
            Some(OnlineGameplayStartBlocker::HostAuthorityRequired)
        );
        assert!(presentation.guidance.contains("toggle ready"));

        let lines = presentation.lines();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Local player: Joined Miner")
                    && line.contains("slot 2")
                    && line.contains("role client")
                    && line.contains("ready yes"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Remote player: Host Miner")
                    && line.contains("slot 1")
                    && line.contains("role host")
                    && line.contains("connected yes"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("joined client uses the host save/session"))
        );
    }

    #[test]
    fn remote_identity_ready_and_disconnect_updates_are_single_source_for_lobby_ui() {
        let mut game = GameState::new();
        game.online_host_owns_save = true;
        game.online_player_slot = Some(1);
        game.online_session_state = OnlineSessionUxState::Connected;

        game.apply_online_remote_identity(crate::multiplayer::PlayerId::new(2), "Tunnel Guest");
        assert_eq!(
            game.online_remote_player_name.as_deref(),
            Some("Tunnel Guest")
        );
        assert!(game.online_remote_player_connected);
        assert!(
            game.online_session_status_message
                .contains("identity synced")
        );

        game.apply_online_remote_ready_state(crate::multiplayer::PlayerId::new(2), true);
        assert!(game.online_remote_player_ready);
        assert!(
            game.online_session_status_message
                .contains("ready state synced")
        );
        assert!(
            game.online_lobby_participant_lines()
                .iter()
                .any(|line| line.contains("Tunnel Guest") && line.contains("ready yes"))
        );

        game.apply_online_session_boundary_status(&OnlineSessionBoundaryStatus::client_left(
            "test",
        ));
        assert!(!game.online_remote_player_connected);
        assert!(!game.online_remote_player_ready);
        assert!(game.message.contains("Joined client left"));
        assert!(
            game.online_lobby_participant_lines()
                .iter()
                .any(|line| line.contains("Tunnel Guest") && line.contains("connected no"))
        );
    }

    #[test]
    fn title_menu_exposes_online_multiplayer_entrypoint() {
        assert!(GameState::title_options().contains(&TitleOption::OnlineMultiplayer));
    }

    #[test]
    fn selecting_online_multiplayer_opens_online_modal() {
        let mut game = GameState::new();
        let options = GameState::title_options();
        game.selected_title_item = options
            .iter()
            .position(|option| *option == TitleOption::OnlineMultiplayer)
            .expect("online multiplayer option exists");

        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.0,
        );

        assert_eq!(game.modal, Some(ModalScreen::OnlineMultiplayer));
    }

    #[test]
    fn hq_briefing_changes_with_depth() {
        let mut game = GameState::new();
        game.deepest_tile_reached = 65;
        assert!(hq_story_message(&game).contains("Thermal"));
    }

    #[test]
    fn return_bonus_resets_trip_depth_and_pays() {
        let mut game = GameState::new();
        game.current_zone = Some(SurfaceZone::Depot);
        game.trip_best_depth = 24;
        let initial_credits = game.player.credits;
        game.award_return_bonus();
        assert_eq!(game.trip_best_depth, 0);
        assert_eq!(game.return_streak, 1);
        assert!(game.player.credits > initial_credits);
    }

    #[test]
    fn lost_cargo_recovers_when_player_returns_to_site() {
        let mut game = GameState::new();
        game.lost_cargo_x = Some(game.player.x);
        game.lost_cargo_y = Some(game.player.y);
        game.lost_cargo_count = 2;
        game.recover_lost_cargo_if_near();
        assert_eq!(game.lost_cargo_count, 0);
        assert_eq!(game.player.cargo_used(), 2);
    }

    #[test]
    fn options_changes_mark_settings_dirty() {
        let mut game = GameState::new();
        game.selected_menu_item = 0;
        game.confirm_options();
        assert!(game.take_settings_dirty());
        assert!(!game.take_settings_dirty());
    }

    #[test]
    fn explosive_pocket_sets_flash_and_cave_in() {
        let mut game = GameState::new();
        game.trigger_explosive_pocket();
        assert!(game.screen_flash_seconds > 0.0);
        assert!(!game.falling_boulders.is_empty());
    }

    #[test]
    fn entering_surface_zone_opens_walkable_interior() {
        let mut game = GameState::new();
        game.enter_interior(SurfaceZone::Fuel);
        assert_eq!(game.run_mode, RunMode::Interior);
        assert_eq!(game.interior_zone, Some(SurfaceZone::Fuel));
        assert!(game.modal.is_none());
    }

    #[test]
    fn interior_counter_opens_existing_service_modal() {
        let mut game = GameState::new();
        game.enter_interior(SurfaceZone::Shop);
        game.interior_x = interior_service_x(SurfaceZone::Shop);
        game.open_interior_hotspot();
        assert_eq!(game.modal, Some(ModalScreen::Shop));
    }

    #[test]
    fn camera_can_follow_player_above_surface() {
        let mut game = GameState::new();
        game.player.y = MIN_PLAYER_Y;
        let (_, target_y) = target_camera_offset(&game);
        assert!(target_y < 0.0);
    }

    #[test]
    fn vertical_movement_allows_limited_sky_flight() {
        let mut game = GameState::new();
        game.player.y = 2.0;
        game.move_axis(0.0, MIN_PLAYER_Y * 2.0);
        assert!((game.player.y - MIN_PLAYER_Y).abs() < f32::EPSILON);
    }

    #[test]
    fn new_game_starts_camera_intro_above_player() {
        let game = GameState::new();
        let (target_x, target_y) = target_camera_offset(&game);

        assert!(game.camera_intro_seconds > 0.0);
        assert!((game.camera_x - target_x).abs() < f32::EPSILON);
        assert!(game.camera_y < target_y);
    }

    #[test]
    fn saved_game_disables_camera_intro() {
        let game = GameState::new();
        let saved = game.clone_for_save();

        assert!(saved.camera_intro_seconds <= f32::EPSILON);
    }

    #[test]
    fn camera_intro_drops_toward_target() {
        let mut game = GameState::new();
        let (_, target_y) = target_camera_offset(&game);
        let initial_y = game.camera_y;

        game.update_camera(CAMERA_INTRO_SECONDS * 0.5);

        assert!(game.camera_y > initial_y);
        assert!(game.camera_y < target_y);
    }

    #[test]
    fn revealing_exploration_marks_nearby_tiles_for_redraw() {
        let mut game = GameState::new();
        game.visual_changes.changed_tiles.clear();
        let position = TilePosition { x: 20, y: 20 };

        game.mark_exploration_visual_changed(position);

        assert!(game.visual_changes.changed_tiles.contains(&position));
        assert!(game.visual_changes.changed_tiles.contains(&TilePosition {
            x: position.x + EXPLORATION_VISUAL_CHANGE_RADIUS_TILES,
            y: position.y
        }));
    }

    #[test]
    fn movement_regression_updates_position_and_burns_fuel() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        let initial_x = game.player.x;
        let initial_fuel = game.player.fuel;

        game.update(
            PlayerInput {
                horizontal: 1.0,
                thrust: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert!(game.player.x > initial_x);
        assert!(game.player.fuel < initial_fuel);
    }

    #[test]
    fn drilling_regression_mines_target_tile_and_adds_cargo() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.x = 10.0 * TILE_SIZE;
        game.player.y = 10.0 * TILE_SIZE;
        game.terrain.set_kind(
            TilePosition { x: 10, y: 11 },
            TileKind::Ore(MineralKind::Copper),
        );

        for _ in 0..12 {
            game.update(
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
                0.1,
            );
        }

        assert!(matches!(
            game.terrain
                .tile(TilePosition { x: 10, y: 11 })
                .map(|tile| tile.kind),
            Some(TileKind::Air)
        ));
        assert!(game.player.cargo_used() > 0);
    }

    #[test]
    fn town_development_and_salvage_recovery_queue_authoritative_session_services() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = Some(ModalScreen::TownDevelopment);
        game.selected_menu_item = 0;
        game.player.credits = 10_000;
        let credits_before = game.player.credits;
        let depot_before = game.town_development.depot_level;

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::UpgradeTownBuilding {
                building: TownBuilding::Depot
            })
        );
        assert_eq!(game.player.credits, credits_before);
        assert_eq!(game.town_development.depot_level, depot_before);

        game.modal = Some(ModalScreen::Salvage);
        game.selected_menu_item = 0;
        game.lost_cargo_count = 1;
        game.lost_minerals.insert(MineralKind::Copper, 1);
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SalvageRecoverLostCargo)
        );
        assert_eq!(game.lost_cargo_count, 1);
        assert_eq!(game.player.cargo_used(), 0);

        game.modal = Some(ModalScreen::Salvage);
        game.selected_menu_item = 3;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SalvageRecoverWreckedPart)
        );

        game.modal = Some(ModalScreen::Salvage);
        game.selected_menu_item = 4;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SalvageClearCollapseZones)
        );

        game.modal = Some(ModalScreen::Salvage);
        game.selected_menu_item = 5;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SalvageSellScrapTip)
        );
    }

    #[test]
    fn crafting_menu_queues_authoritative_session_service() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = Some(ModalScreen::Crafting);
        game.selected_menu_item = 0;
        game.player
            .add_material(StrategicResourceKind::AncientAlloy, 2);
        let materials_before = game.player.materials.clone();

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });

        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::CraftRecipe {
                recipe: RecipeKind::ReinforcedBulkhead
            })
        );
        assert_eq!(game.player.materials, materials_before);
        assert_eq!(game.player.crafted_bulkheads, 0);
        assert!(game.message.contains("authoritative session"));
    }

    #[test]
    fn bank_menu_queues_authoritative_finance_and_insurance_without_mutating_player() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = Some(ModalScreen::Bank);
        game.selected_menu_item = 0;
        game.player.credits = 100;
        game.player.loan_debt = 0;

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::Finance)
        );
        assert_eq!(game.player.credits, 100);
        assert_eq!(game.player.loan_debt, 0);

        game.modal = Some(ModalScreen::Bank);
        game.selected_menu_item = 1;
        game.player.insured = false;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::BuyInsurance)
        );
        game.modal = Some(ModalScreen::Bank);
        game.selected_menu_item = 2;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::StartSideContract)
        );
        assert!(game.active_side_contracts.is_empty());
    }

    #[test]
    fn explosives_and_salvage_menus_queue_authoritative_session_services() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.modal = Some(ModalScreen::Explosives);
        game.selected_menu_item = 0;
        game.player.credits = 500;
        let credits_before = game.player.credits;
        let bombs_before = game.player.bombs;

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::BuyBombBundle { count: 3, cost: 55 })
        );
        assert_eq!(game.player.credits, credits_before);
        assert_eq!(game.player.bombs, bombs_before);

        game.modal = Some(ModalScreen::Explosives);
        game.selected_menu_item = 2;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::BuyMiningRockets)
        );

        game.modal = Some(ModalScreen::Salvage);
        game.selected_menu_item = 1;
        game.player.hull = 5.0;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SalvagePatchHull)
        );
        assert!((game.player.hull - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn shop_upgrade_confirm_queues_authoritative_session_purchase() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.current_zone = Some(SurfaceZone::Shop);
        game.modal = Some(ModalScreen::ShopConfirm);
        game.selected_menu_item = 0;
        game.player.credits = 10_000;
        let credits_before = game.player.credits;
        let drill_before = game.player.drill_strength;

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });

        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::BuyUpgrade { index: 0 })
        );
        assert_eq!(game.player.credits, credits_before);
        assert_eq!(game.player.drill_strength, drill_before);
        assert!(game.message.contains("authoritative session"));
    }

    #[test]
    fn cargo_and_economy_regression_queues_loaded_ore_sale_for_authoritative_session() {
        let mut game = GameState::new();
        assert!(game.player.add_cargo(MineralKind::Copper));
        game.modal = Some(ModalScreen::Depot);
        game.selected_menu_item = 0;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::CompleteDepotWork)
        );

        game.modal = Some(ModalScreen::Depot);
        game.selected_menu_item = 1;

        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });

        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SellCargo)
        );
        assert_eq!(game.player.cargo_used(), 1);
        game.modal = Some(ModalScreen::Depot);
        game.selected_menu_item = 2;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::AutoSortLowGradeCargo)
        );

        game.modal = Some(ModalScreen::Depot);
        game.selected_menu_item = 3;
        game.handle_modal(PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        });
        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::SellScanData)
        );
    }

    #[test]
    fn bombs_regression_arm_and_detonate() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.bombs = 1;
        game.player.y = MIN_PLAYER_Y;
        game.town_development.explosives_shack_level = 3;

        game.update(
            PlayerInput {
                bomb: true,
                ..PlayerInput::default()
            },
            0.1,
        );
        assert_eq!(game.placed_bombs.len(), 1);
        game.update(PlayerInput::default(), 1.0);

        assert!(game.placed_bombs.is_empty());
        assert!(
            game.sound_cues
                .iter()
                .any(|cue| matches!(cue, SoundCue::Explosion))
        );
    }

    #[test]
    fn rescue_regression_returns_player_to_surface_and_records_fee() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.game_over = true;
        game.player.y = 40.0 * TILE_SIZE;
        game.player.credits = 1_000;
        let initial_credits = game.player.credits;

        game.update(
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert!(!game.game_over);
        assert!(game.player.y <= 4.0 * TILE_SIZE);
        assert!(game.player.credits < initial_credits);
        assert_eq!(game.rescue_count, 1);
    }

    #[test]
    fn damage_and_repair_regression_queues_authoritative_session_repair() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.hull = 50.0;
        game.player.credits = 500;
        game.modal = Some(ModalScreen::RepairConfirm);

        game.update(
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert_eq!(
            game.take_session_service_request(),
            Some(SessionServiceRequest::Repair { menu_item: 0 })
        );
        assert!(game.message.contains("authoritative session"));
    }

    #[test]
    fn ui_transition_regression_pauses_from_playing() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;

        game.update(
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert_eq!(game.run_mode, RunMode::Paused);
    }

    #[test]
    fn scanner_regression_pulses_and_enters_cooldown() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.scanner_level = 1;

        game.update(
            PlayerInput {
                scan: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert!(game.scanner_pulse_seconds > 0.0);
        assert!(game.scanner_cooldown_seconds > 0.0);
        assert!(game.message.contains("Scanner pulse"));
    }

    #[test]
    fn infrastructure_regression_places_signal_relay() {
        let mut game = GameState::new();
        game.run_mode = RunMode::Playing;
        game.player.y = 12.0 * TILE_SIZE;
        game.player.signal_relay_kits = 1;

        game.update(
            PlayerInput {
                place_relay: true,
                ..PlayerInput::default()
            },
            0.1,
        );

        assert_eq!(game.infrastructure.len(), 1);
        assert_eq!(game.infrastructure[0].kind, InfrastructureKind::SignalRelay);
        assert_eq!(game.player.signal_relay_kits, 0);
    }
}
