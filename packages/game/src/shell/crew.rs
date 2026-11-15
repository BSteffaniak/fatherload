//! Multiplayer persistence in the menus (WP-PERSIST): the "host this world online" offer after
//! loading a save that holds other players, the lobby's rig-claim entries (a client asks for a
//! saved rig, the host approves or denies) and their labels.

use crate::runtime::{LobbyInfo, OnlineRole};

use super::{LobbyAction, OnlineShell, RunMode, ShellRequest, ShellState, SoundCue};

/// Entries of the [`super::OnlineScreen::ResumeHost`] offer.
pub const RESUME_HOST_MENU: [&str; 2] = ["Host this world online", "Play offline for now"];

/// The lobby's crew entries: a host's pending claims (approve, deny), a client's "claim" entry
/// per saved player (or the one it is waiting on).
pub(super) fn crew_actions(online: &OnlineShell) -> Vec<LobbyAction> {
    let Some(lobby) = online.lobby.as_ref() else {
        return Vec::new();
    };
    if online.role == Some(OnlineRole::Host) {
        lobby
            .claims
            .iter()
            .flat_map(|claim| {
                [
                    LobbyAction::ApproveClaim(claim.client),
                    LobbyAction::DenyClaim(claim.client),
                ]
            })
            .collect()
    } else if let Some(pending) = lobby.pending_claim {
        vec![LobbyAction::ClaimRig(pending)]
    } else {
        lobby
            .roster
            .iter()
            .filter(|entry| entry.parked)
            .map(|entry| LobbyAction::ClaimRig(entry.player))
            .collect()
    }
}

/// What a crew lobby entry asks the app to do.
pub(super) const fn crew_request(action: LobbyAction) -> Option<ShellRequest> {
    match action {
        LobbyAction::ClaimRig(player) => Some(ShellRequest::ClaimRig(player)),
        LobbyAction::ApproveClaim(client) => Some(ShellRequest::ResolveClaim {
            client,
            approve: true,
        }),
        LobbyAction::DenyClaim(client) => Some(ShellRequest::ResolveClaim {
            client,
            approve: false,
        }),
        _ => None,
    }
}

/// The label of a lobby entry, naming the players a crew entry is about.
#[must_use]
pub fn lobby_action_text(action: LobbyAction, host: bool, lobby: Option<&LobbyInfo>) -> String {
    let saved_name = |player| {
        lobby
            .and_then(|lobby| lobby.roster.iter().find(|entry| entry.player == player))
            .map(|entry| entry.name.clone())
    };
    let claim =
        |client| lobby.and_then(|lobby| lobby.claims.iter().find(|claim| claim.client == client));
    match action {
        LobbyAction::ClaimRig(player) => {
            let waiting = lobby.and_then(|lobby| lobby.pending_claim) == Some(player);
            match (saved_name(player), waiting) {
                (Some(name), true) => format!("Waiting for the host to hand over {name}'s rig..."),
                (None, true) => "Waiting for the host...".to_owned(),
                (Some(name), false) => format!("Claim {name}'s saved rig (new device)"),
                (None, false) => action.label(host).to_owned(),
            }
        }
        LobbyAction::ApproveClaim(client) => claim(client).map_or_else(
            || action.label(host).to_owned(),
            |claim| {
                if claim.name == claim.target_name {
                    format!("Approve: hand {} their saved rig", claim.name)
                } else {
                    format!(
                        "Approve: {} takes over {}'s rig",
                        claim.name, claim.target_name
                    )
                }
            },
        ),
        LobbyAction::DenyClaim(client) => claim(client).map_or_else(
            || action.label(host).to_owned(),
            |claim| {
                format!(
                    "Deny: keep {}'s rig waiting for its owner",
                    claim.target_name
                )
            },
        ),
        _ => action.label(host).to_owned(),
    }
}

impl ShellState {
    /// After loading a save that holds other players: offer to host it online right away.
    pub fn offer_resume_hosting(&mut self, crew: Vec<String>) {
        self.online.resume_crew = crew;
        self.show_online(super::OnlineScreen::ResumeHost);
    }

    pub(super) fn handle_resume_host(
        &mut self,
        input: crate::input::PlayerInput,
        requests: &mut Vec<ShellRequest>,
    ) {
        let play_offline = |shell: &mut Self| {
            shell.run_mode = RunMode::Playing;
            shell.message = format!(
                "Playing offline. {} saved crewmate(s) wait in this world: Pause > Open to LAN / Online.",
                shell.online.resume_crew.len()
            );
        };
        if input.backs_out() {
            play_offline(self);
            self.sound_cues.push(SoundCue::UiBack);
            return;
        }
        match self.online_menu_choice(input, RESUME_HOST_MENU.len()) {
            Some(0) => requests.push(ShellRequest::HostLoadedWorld),
            Some(_) => play_offline(self),
            None => {}
        }
    }
}
