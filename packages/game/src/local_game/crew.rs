//! Multiplayer persistence in the app (WP-PERSIST): the install's identity, the "host this world
//! online" offer after loading a save with other players, hosting that world, and the crew
//! toasts ("Welcome back, <name>", crewmates returning, rig claims).

use fatherload_simulation::{PlayerId, WorldState};

use super::{FrameReport, LocalGame};
use crate::{
    online,
    profile::PlayerProfile,
    runtime::{CrewToast, LocalHost, LocalIdentity, MAX_LOCAL_SEATS, Runtime, choose_seat_rigs},
    shell::{FanfareKind, OnlineScreen, RunMode, SoundCue},
};

impl LocalGame {
    /// Plays as the install's persistent profile from now on (the app sets it at startup).
    pub fn set_profile(&mut self, profile: PlayerProfile) {
        self.profile = Some(profile);
    }

    /// Who this app's seats are.
    pub(super) fn identity(&self) -> LocalIdentity {
        self.profile.as_ref().map_or_else(
            || LocalIdentity {
                profile: None,
                name: online::player_name(),
            },
            |profile| LocalIdentity {
                profile: Some(profile.id),
                name: profile.name.clone(),
            },
        )
    }

    /// After a load from the title: a save holding other players offers to host it online.
    pub(super) fn offer_resume_hosting(&mut self) {
        if cfg!(target_os = "emscripten") {
            return;
        }
        let crew: Vec<String> = self
            .runtime
            .world()
            .parked_players()
            .map(|(id, parked)| parked.member.display_name(id))
            .collect();
        if !crew.is_empty() {
            self.shell.offer_resume_hosting(crew);
        }
    }

    /// Hosts the loaded world online, lobby first: the saved crewmates are listed as offline
    /// and waiting, and each gets their own rig back on joining.
    pub(super) fn host_loaded_world(&mut self, report: &mut FrameReport) {
        let world = self.runtime.world().clone();
        let seats = self.runtime.local_seats().len().max(1);
        let play_seconds = self.shell.play_seconds;
        let identity = self.identity();
        match LocalHost::online(world, seats, identity.clone(), self.host_bind, true) {
            Ok(mut host) => {
                host.set_host_name(&identity.name);
                self.go_online(Runtime::Host(Box::new(host)), report);
                self.resumed_play_seconds = Some(play_seconds);
                self.shell.online.ready = false;
                self.shell.show_online(OnlineScreen::Lobby);
                "Hosting your saved world: friends get their own rigs back when they join."
                    .clone_into(&mut self.shell.message);
                self.shell.sound_cues.push(SoundCue::Ui);
            }
            Err(error) => {
                self.shell.message = format!("Could not host: {error}");
                self.shell.run_mode = RunMode::Playing;
                self.shell.sound_cues.push(SoundCue::UiError);
            }
        }
    }

    /// Turns the runtime's crew news into banners, messages and sounds.
    pub(super) fn present_crew_toasts(&mut self) {
        for toast in self.runtime.take_crew_toasts() {
            match toast {
                CrewToast::WelcomeBack { name } => {
                    self.pending_welcome = Some((
                        format!("Welcome back, {name}"),
                        "Your rig, cargo, credits and upgrades are right where you left them."
                            .to_owned(),
                    ));
                }
                CrewToast::ClaimApproved { name } => {
                    self.pending_welcome = Some((
                        format!("Welcome back, {name}"),
                        "The host handed you your saved rig. Good to have you back.".to_owned(),
                    ));
                }
                CrewToast::ClaimDenied => {
                    "The host kept that rig for its owner.".clone_into(&mut self.shell.message);
                    self.shell.sound_cues.push(SoundCue::UiError);
                }
                CrewToast::CrewmateReturned { name } => {
                    self.shell.message =
                        format!("{name} is back: their rig is right where they left it.");
                    self.shell.sound_cues.push(SoundCue::CrewReturn);
                }
                CrewToast::CrewmateJoined { name } => {
                    self.shell.message = format!("{name} joined with a fresh rig.");
                    self.shell.sound_cues.push(SoundCue::Ui);
                }
                CrewToast::CrewmateLeft { name, parked } => {
                    self.shell.message = if parked {
                        format!("{name} left; their rig waits in the save for them.")
                    } else {
                        format!("{name} left the session.")
                    };
                }
                CrewToast::ClaimRequested { name, target_name } => {
                    self.shell.message = format!(
                        "{name} (new device) asks for {target_name}'s saved rig: answer in Pause > Online Session."
                    );
                    self.shell.sound_cues.push(SoundCue::Ui);
                }
            }
        }
    }

    /// Raises a waiting "Welcome back" banner once the gameplay view shows.
    pub(super) fn show_pending_welcome(&mut self) {
        if self.shell.run_mode != RunMode::Playing || self.seats().is_empty() {
            return;
        }
        if let Some((title, detail)) = self.pending_welcome.take() {
            self.shell
                .quest_fanfare(0, FanfareKind::WelcomeBack, title, detail);
            self.shell.sound_cues.push(SoundCue::CrewReturn);
        }
    }
}

/// How many local seats a save loaded from the title starts with: one per split-screen seat
/// whose saved rig this install gets back (at least one).
#[must_use]
pub fn saved_seat_count(world: &WorldState, identity: &LocalIdentity) -> usize {
    let member_seat = |id: PlayerId| {
        world
            .crew_member(id)
            .or_else(|| world.parked_player(id).map(|parked| &parked.member))
            .and_then(|member| member.seat)
    };
    choose_seat_rigs(world, MAX_LOCAL_SEATS, identity)
        .iter()
        .enumerate()
        .filter(|(index, rig)| {
            rig.and_then(member_seat)
                .is_some_and(|seat| usize::try_from(seat).ok() == Some(*index))
        })
        .map(|(index, _)| index + 1)
        .max()
        .unwrap_or(1)
}
