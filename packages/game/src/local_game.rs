//! One frame of local play, independent of the window (plan §10 frame loop).
//!
//! `sample input -> shell actions to shell state, gameplay input per local seat ->
//! runtime.advance(dt) -> RenderSnapshot -> render`. The raylib app only samples devices, calls
//! [`LocalGame::frame`] and draws [`LocalGame::snapshot`], so tests drive the exact same path with
//! scripted inputs.

use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

use fatherload_netcode::ReconnectToken;
use fatherload_quic_transport::{HostDescriptor, JoinTarget, lan::LanDiscoveredGame};
use fatherload_simulation::{
    EnemyDifficulty, GameplayEventKind, PlayerCommand, PlayerId, WorldState, clock::DayLength,
    worldgen::WorldSettings,
};

mod crew;
mod old_deep_news;
mod rewards;

#[cfg(test)]
pub use crew::saved_seat_count;

use crate::{
    audio::cues::PlacedCue,
    camera_lead::CameraLeads,
    effects::{Effects, EventFeed},
    input::{PlayerInput, devices::InputDevice},
    input_mapping::seat_input_with,
    online::{
        DEFAULT_PORT,
        client::OnlineClient,
        lan::{self, Background},
    },
    render_snapshot::{RenderSnapshot, ScreenSize},
    runtime::{
        ClientState, LocalHost, LocalSeat, OnlineRole, Runtime, RuntimeEvent, RuntimeEventKind,
        SeatInput,
    },
    save::{self, SaveError, SettingsFile, WorldSave},
    shell::{OnlineScreen, RunMode, ShellContext, ShellRequest, ShellState, SoundCue},
};

/// The toast for a new split-screen game: which keys each keyboard seat drives and uses doors
/// with, and how a spare controller joins, named by the prompt system (see `input::seats` for
/// the policy). Kept short: it shows in every view.
fn split_screen_message(seats: usize) -> String {
    use crate::input::{
        devices::{KeyboardLayout, PadFamily},
        prompts::{Prompt, inline_text},
    };
    let keys =
        |prompt, layout| inline_text(prompt, InputDevice::Keyboard(layout)).unwrap_or_default();
    let pad = InputDevice::Gamepad {
        index: 0,
        family: PadFamily::Xbox,
    };
    let join = [Prompt::Join, Prompt::Pause]
        .into_iter()
        .filter_map(|prompt| inline_text(prompt, pad))
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "{seats}-player split-screen. P1: {} drive, {} use. P2: {} drive, {} use. \
         Spare controllers join with {join}.",
        keys(Prompt::Walk, KeyboardLayout::Wasd),
        keys(Prompt::Use, KeyboardLayout::Wasd),
        keys(Prompt::Walk, KeyboardLayout::Arrows),
        keys(Prompt::Use, KeyboardLayout::Arrows),
    )
}

/// How often an online host autosaves the session (clients never save).
const AUTOSAVE_SECONDS: f32 = 300.0;

/// Where saves go. The app uses the real save directory; tests use a temp path.
#[derive(Clone, Debug)]
pub enum SaveTarget {
    /// The user's save files (main file + slots).
    UserFiles,
    /// A single file, used for the main save and every slot (tests).
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "headless tests save to a temp file")
    )]
    File(std::path::PathBuf),
}

/// One frame of sampled input: the primary seat's full input (also drives the shell) and the
/// gameplay input of every further seat, in seat order.
#[derive(Clone, Debug, Default)]
pub struct FrameInput {
    pub primary: PlayerInput,
    pub other_seats: Vec<PlayerInput>,
    /// Clipboard text, sampled when the primary seat pressed paste.
    pub clipboard: Option<String>,
    /// Each seat's active input device (drives its button prompts), in seat order. Empty
    /// keeps the devices the shell already has.
    pub devices: Vec<InputDevice>,
    /// Connected controllers and their names (F3 overlay).
    pub gamepads: BTreeMap<u8, String>,
}

impl FrameInput {
    #[must_use]
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub const fn primary(primary: PlayerInput) -> Self {
        Self {
            primary,
            other_seats: Vec::new(),
            clipboard: None,
            devices: Vec::new(),
            gamepads: BTreeMap::new(),
        }
    }

    /// Every seat's input in seat order (the primary seat first).
    #[must_use]
    pub fn seat_inputs(&self) -> Vec<PlayerInput> {
        std::iter::once(self.primary)
            .chain(self.other_seats.iter().copied())
            .collect()
    }
}

/// What a frame did that the window layer must react to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FrameReport {
    /// Fixed ticks simulated.
    pub ticks_run: u32,
    /// The world was replaced (new game / load): redraw all terrain.
    pub world_replaced: bool,
    /// Terrain tiles changed this frame.
    pub changed_tiles: Vec<fatherload_simulation::terrain::TilePosition>,
    /// Tiles changed this frame in the ground of other places (F11: space bodies).
    pub changed_store_tiles: Vec<(
        fatherload_simulation::place::StoreKey,
        fatherload_simulation::terrain::TilePosition,
    )>,
    /// Sounds to play, with where they come from.
    pub sound_cues: Vec<PlacedCue>,
    /// Settings changed and should be persisted.
    pub settings_changed: bool,
    /// Text to put on the system clipboard.
    pub clipboard: Option<String>,
}

/// Shell + runtime + presentation effects for local play.
#[derive(Debug)]
pub struct LocalGame {
    pub shell: ShellState,
    runtime: Runtime,
    events: EventFeed,
    effects: Effects,
    /// Speed-scaled camera lead of each local seat (presentation only).
    camera_leads: CameraLeads,
    frame_events: Vec<RuntimeEvent>,
    save_target: SaveTarget,
    /// Where the last join went and the token to reclaim the player there.
    last_join: Option<(JoinTarget, Option<ReconnectToken>)>,
    lan_browse: Option<Background<Vec<LanDiscoveredGame>>>,
    lan_fetch: Option<Background<HostDescriptor>>,
    client_entered: bool,
    autosave_seconds: f32,
    host_bind: SocketAddr,
    /// Where each seat's view sat on screen when last drawn (mouse aim), in seat order.
    view_anchors: std::cell::RefCell<Vec<Option<crate::weapon_aim::ViewAnchor>>>,
    /// How each seat aimed this frame (its reticle), in seat order.
    seat_aims: Vec<crate::weapon_aim::SeatAim>,
    /// The install's persistent identity (WP-PERSIST); `None` plays anonymously (tests, scenes).
    profile: Option<crate::profile::PlayerProfile>,
    /// A "Welcome back" banner waiting for the gameplay view to show (title, detail).
    pending_welcome: Option<(String, String)>,
    /// Play time of a loaded save that is being hosted from the lobby.
    resumed_play_seconds: Option<f32>,
    /// Who lay in bed through tonight (no dawn banner for them).
    dawn_witnesses: crate::night_text::DawnWitnesses,
}

impl LocalGame {
    /// A game on the title screen with a fresh world idling behind it.
    #[must_use]
    pub fn new(settings: SettingsFile, save_target: SaveTarget) -> Self {
        Self {
            shell: ShellState::new(settings),
            runtime: Runtime::local(new_world(), 1),
            events: EventFeed::default(),
            effects: Effects::default(),
            camera_leads: CameraLeads::default(),
            frame_events: Vec::new(),
            save_target,
            last_join: None,
            lan_browse: None,
            lan_fetch: None,
            client_entered: false,
            autosave_seconds: 0.0,
            host_bind: SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), DEFAULT_PORT),
            view_anchors: std::cell::RefCell::new(Vec::new()),
            seat_aims: Vec::new(),
            profile: None,
            pending_welcome: None,
            resumed_play_seconds: None,
            dawn_witnesses: crate::night_text::DawnWitnesses::default(),
        }
    }

    /// Uses `bind` for hosting instead of the default port (tests bind an ephemeral port).
    #[cfg(test)]
    pub const fn set_host_bind(&mut self, bind: SocketAddr) {
        self.host_bind = bind;
    }

    /// Ends any online session cleanly before the app closes (the host tells its clients, a
    /// client leaves), waiting for the network thread to flush.
    pub fn shutdown(self) {
        match self.runtime {
            Runtime::Host(mut host) => {
                host.end_session();
                if let Some(net) = host.take_net() {
                    net.shutdown_blocking();
                }
            }
            Runtime::Client(client) => client.leave_blocking(),
        }
    }

    /// The gameplay authority.
    #[cfg(test)]
    #[must_use]
    pub const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The world the runtime exposes.
    #[must_use]
    pub fn world(&self) -> &WorldState {
        self.runtime.world()
    }

    /// Local seats in order.
    #[must_use]
    pub fn seats(&self) -> &[LocalSeat] {
        self.runtime.local_seats()
    }

    /// Whether the app should close.
    #[must_use]
    pub const fn should_exit(&self) -> bool {
        self.shell.request_exit
    }

    /// Runs one frame.
    #[allow(
        clippy::too_many_lines,
        reason = "the frame's order (shell, seats, runtime, presentation) reads best in one place"
    )]
    pub fn frame(&mut self, input: &FrameInput, delta_seconds: f32) -> FrameReport {
        let mut report = FrameReport::default();
        self.runtime.pump_network(delta_seconds);
        self.sync_online(&mut report);
        self.poll_lan(&mut report);
        self.shell.pasted.clone_from(&input.clipboard);
        if !input.devices.is_empty() {
            self.shell.seat_devices.clone_from(&input.devices);
        }
        self.shell.gamepads.clone_from(&input.gamepads);
        let players = self.local_players();
        let on_title = self.shell.run_mode == RunMode::Title;
        // Every seat's input in seat order; seats without a sampled input stay idle.
        let mut inputs = input.seat_inputs();
        inputs.resize_with(players.len().max(1), PlayerInput::default);
        let requests = self.shell.update(
            &inputs,
            delta_seconds,
            shell_context(&self.runtime, &self.save_target, &players, on_title),
        );
        let mut seat_actions = vec![Vec::new(); inputs.len()];
        let replaced_before = report.world_replaced;
        for request in requests {
            self.handle_request(request, &mut seat_actions, &mut report);
        }
        if report.world_replaced && !replaced_before {
            // A load or new game this frame reset every seat's menus: actions picked in the
            // menus of the world that was just replaced (a purchase confirmed on the frame
            // someone quick-loads) must not run in the new one.
            seat_actions.iter_mut().for_each(Vec::clear);
        }

        let seats = self.seats().to_vec();
        self.seat_aims
            .resize_with(seats.len(), crate::weapon_aim::SeatAim::default);
        for (index, seat) in seats.iter().enumerate() {
            let mut seat_frame = match inputs.get(index) {
                Some(sampled) if self.shell.gameplay_input_enabled(index) => {
                    let player = self.runtime.world().player(seat.player);
                    let selection = &mut self.shell.seat_mut(index).explosives;
                    if selection.observe(sampled, player) {
                        report
                            .sound_cues
                            .push(PlacedCue::interface(crate::shell::SoundCue::UiMove));
                    }
                    let mut frame = seat_input_with(sampled, *selection);
                    // The hardpoint weapon: trigger, aim (pointer, stick or auto) and swaps. On
                    // foot (Part II F5) the trigger swings the hand pick from the pilot instead.
                    let anchor = self.view_anchors.borrow().get(index).copied().flatten();
                    let pilot_aim = crate::weapon_aim::pilot_aim(
                        sampled,
                        self.runtime.world(),
                        seat.player,
                        anchor.as_ref(),
                    );
                    let on_foot = pilot_aim.is_some();
                    let aim = pilot_aim.unwrap_or_else(|| {
                        crate::weapon_aim::seat_aim(
                            sampled,
                            player,
                            selection.facing,
                            anchor.as_ref(),
                        )
                    });
                    frame.controls = frame
                        .controls
                        .with_weapon(aim.control)
                        .with_jump(sampled.jump)
                        // F9 craft rotation: the rotation thrusters and the seat's assists.
                        .with_rotation(if on_foot { 0.0 } else { sampled.rotate })
                        .with_assists(self.shell.assists);
                    if !on_foot && let Some(stick) = sampled.rig_horizontal() {
                        // A controller's d-pad turns the rig; a deflected stick drives alone.
                        frame.controls = fatherload_simulation::ControlState::new(
                            stick,
                            frame.controls.thrust,
                            frame.controls.drill_down,
                        )
                        .with_weapon(frame.controls.weapon)
                        .with_jump(frame.controls.jump)
                        .with_rotation(frame.controls.rotation())
                        .with_assists(frame.controls.assists());
                    }
                    if !on_foot && crate::weapon_aim::trigger_thrusts(sampled, player) {
                        frame.controls.thrust = true;
                    }
                    // Space flight (Wave 11): the seat's flight switches, and the rig's
                    // buttons fly the Star Drive while it is away.
                    frame.controls = frame.controls.with_flight(self.shell.flight);
                    if player.is_some_and(|player| player.flight.away()) {
                        crate::input_mapping::space_actions(sampled, &mut frame.actions);
                    }
                    frame
                        .actions
                        .extend(crate::weapon_aim::cycle_command(sampled, player, on_foot));
                    // WP-PILOT: the rope button on foot (grapple, winch, let go).
                    frame
                        .actions
                        .extend(crate::weapon_aim::rope_command(sampled, &aim, on_foot));
                    self.seat_aims[index] = aim;
                    frame
                }
                _ => {
                    self.seat_aims[index] = crate::weapon_aim::SeatAim::default();
                    SeatInput::default()
                }
            };
            if let Some(actions) = seat_actions.get_mut(index) {
                seat_frame.actions.append(actions);
            }
            self.runtime.submit(seat.seat, seat_frame);
        }

        self.runtime.set_paused(!self.shell.world_should_tick());
        let frame = self
            .runtime
            .advance(Duration::try_from_secs_f32(delta_seconds).unwrap_or_default());
        report.ticks_run = frame.ticks_run;
        report.world_replaced |= frame.world_replaced;
        // F7: presentation draws in the world's frame; effects kept in pixels follow its moves.
        let shift = crate::view_frame::set(self.runtime.world().frame());
        if !shift.is_zero() && !frame.world_replaced {
            self.effects.reframe(shift);
        }
        report.changed_tiles.extend(frame.changed_tiles);
        report.changed_store_tiles.extend(frame.changed_store_tiles);
        self.frame_events = self.events.present(frame.events);
        self.observe_events();
        self.save_at_shrines();
        self.show_pending_welcome();
        let local_players = self.local_players();
        self.effects.update(delta_seconds);
        self.camera_leads
            .update(delta_seconds, self.runtime.world(), &local_players);
        let effect_cues =
            self.effects
                .apply(&self.frame_events, self.runtime.world(), &local_players);
        report.sound_cues.extend(
            self.shell
                .sound_cues
                .iter()
                .copied()
                .map(PlacedCue::interface),
        );
        report.sound_cues.extend(effect_cues);
        report.settings_changed = self.shell.take_settings_dirty();
        self.autosave(delta_seconds);
        report
    }

    /// The immutable view of this frame for rendering.
    #[must_use]
    pub fn snapshot(&self, screen: impl Into<ScreenSize>) -> RenderSnapshot<'_> {
        let mut snapshot = RenderSnapshot::build(
            self.runtime.world(),
            self.runtime.local_seats(),
            screen,
            (self.effects.sparks(), self.effects.explosions()),
            &self.frame_events,
        );
        // A client draws its rig where prediction + smoothing put it (the camera follows that
        // same transform) and remote rigs where interpolation puts them.
        snapshot.bomb_alpha = self.runtime.interpolation_alpha();
        snapshot.effects = self.shell.effects;
        snapshot.pickups = self.effects.pickups();
        snapshot.apply_pose_overrides(&self.runtime.pose_overrides());
        snapshot.apply_camera_leads(&self.camera_leads);
        let mut anchors = self.view_anchors.borrow_mut();
        anchors.clear();
        let ending_elapsed = self.runtime.world().ending_elapsed();
        for view in &mut snapshot.views {
            if anchors.len() <= view.seat_index {
                anchors.resize(view.seat_index + 1, None);
            }
            // The seat's own ending (Wave 10): its phase from the world's clock and its skips.
            view.ending = ending_elapsed.map(|elapsed| {
                let seat_view = self.shell.seat(view.seat_index).ending;
                crate::rendering::heart_hud::EndingSeat {
                    phase: seat_view.phase(elapsed),
                    elapsed,
                    view: seat_view,
                }
            });
            // The rolled camera (F9) turns about the seat's rig as drawn.
            let pivot = snapshot
                .players
                .iter()
                .find(|player| player.player_id == view.player_id)
                .map_or((view.camera.x, view.camera.y), |player| {
                    (player.x, player.y)
                });
            anchors[view.seat_index] = Some(crate::weapon_aim::ViewAnchor {
                viewport: view.viewport,
                camera: view.camera,
                world_scale: view.world_scale,
                roll: view.roll,
                pivot,
            });
            if self.shell.gameplay_input_enabled(view.seat_index) {
                view.aim_preview = crate::render_snapshot::AimPreview::for_selection(
                    self.runtime.world(),
                    view.player_id,
                    self.shell.seat(view.seat_index).explosives,
                );
                if let Some(aim) = self.seat_aims.get(view.seat_index) {
                    view.reticle =
                        crate::weapon_aim::reticle(self.runtime.world(), view.player_id, aim);
                }
            }
            let seat = self.shell.seat(view.seat_index);
            view.build_preview = seat.build.as_ref().and_then(|draft| {
                crate::base_view::BuildPreview::for_draft(
                    self.runtime.world(),
                    view.player_id,
                    draft,
                )
            });
            view.base_module = seat
                .base_module
                .filter(|_| seat.modal == Some(crate::shell::ModalScreen::BaseModule))
                .and_then(|id| {
                    crate::base_view::BaseModuleView::for_module(
                        self.runtime.world(),
                        view.player_id,
                        id,
                    )
                });
        }
        drop(anchors);
        snapshot.shots = self.effects.shots();
        snapshot
    }

    /// Replaces the session with a fresh local world of `seats` seats, lets `arrange` set up that
    /// world (rig positions, terrain, cargo...) and enters it. Scene capture uses this to build
    /// scripted states; gameplay never calls it.
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub fn stage_local_world(
        &mut self,
        seats: usize,
        arrange: impl FnOnce(&mut WorldState, &[LocalSeat]),
    ) -> FrameReport {
        let mut runtime = Runtime::local(new_world(), seats);
        let staged_seats = runtime.local_seats().to_vec();
        let mut world = runtime.world().clone();
        arrange(&mut world, &staged_seats);
        runtime.load_world(world);
        end_runtime(std::mem::replace(&mut self.runtime, runtime));
        let mut report = FrameReport::default();
        self.reset_presentation(&mut report);
        self.shell.set_seat_count(staged_seats.len());
        self.shell.enter_world(String::new(), 0.0);
        report
    }

    /// Edits the running local world in place (scene capture: nudging a rig mid-scene, after
    /// the world has run a while). Gameplay never calls it.
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub fn edit_world(&mut self, edit: impl FnOnce(&mut WorldState)) {
        let mut world = self.runtime.world().clone();
        edit(&mut world);
        let _ = self.runtime.load_world(world);
    }

    /// The current world as a save.
    #[must_use]
    pub fn world_save(&self) -> WorldSave {
        WorldSave {
            world: self.runtime.world().clone(),
            play_seconds: self.shell.play_seconds,
        }
    }

    /// The player of every local seat, in seat order.
    fn local_players(&self) -> Vec<PlayerId> {
        self.seats().iter().map(|seat| seat.player).collect()
    }

    /// Hands every local seat the results of the service actions it submitted.
    fn observe_events(&mut self) {
        let players = self.local_players();
        let resolved: Vec<_> = self
            .frame_events
            .iter()
            .filter_map(|event| {
                let seat = players
                    .iter()
                    .position(|player| Some(*player) == event.player)?;
                match &event.event {
                    RuntimeEventKind::ActionResolved {
                        command: PlayerCommand::Service(request),
                        applied,
                    } => Some((seat, *request, *applied)),
                    _ => None,
                }
            })
            .collect();
        for (seat, request, applied) in resolved {
            self.shell.service_resolved(
                seat,
                request,
                applied,
                shell_context(&self.runtime, &self.save_target, &players, false),
            );
        }
        self.announce_points_of_interest(&players);
        self.announce_villages(&players);
        self.announce_moon(&players);
        self.announce_logistics(&players);
        self.announce_old_deep(&players);
        self.announce_living_dark(&players);
        self.announce_drowned(&players);
        self.announce_distortion(&players);
        self.announce_far_strange(&players);
        self.announce_supplies(&players);
        self.announce_ancient_city(&players);
        self.announce_garrison(&players);
        self.announce_heart(&players);
        self.announce_flight(&players);
        self.announce_systems(&players);
        self.announce_settlement_memories(&players);
        self.announce_night(&players);
        self.announce_quests(&players);
        self.announce_stories(&players);
        self.announce_expedition(&players);
        self.announce_rewards(&players);
    }

    /// The Fathers' expedition for local seats (Wave 8 story). The crew shares one journal, so
    /// a log read and an echo go up on every local seat's card; the finder's seat also hears
    /// what the canister held (or that a chapter is complete). An anomaly room's card is for
    /// the seat that stepped in (its rule is where that rig is); a sealed canister is a line.
    fn announce_expedition(&mut self, players: &[PlayerId]) {
        use crate::shell::expedition::{
            anomaly_words, descent_reached_words, echo_words, log_found_words, sealed_words,
        };
        use fatherload_simulation::expedition::progress::StoryEvent;
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        let mut ending = false;
        let found = self.runtime.world().expedition().found;
        let mut moments: Vec<(usize, crate::shell::StoryMomentKind, String, String)> = Vec::new();
        let mut messages: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::ExpeditionLogFound { player_id, log } => {
                    if let Some((kind, title, text, message)) = log_found_words(log) {
                        for seat in 0..players.len() {
                            moments.push((seat, kind, title.clone(), text.clone()));
                        }
                        if let Some(seat) = seat_of(player_id) {
                            messages.push((seat, message));
                        }
                    }
                }
                GameplayEventKind::ExpeditionLogSealed { player_id, log } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((seat, sealed_words(log, found)));
                    }
                }
                GameplayEventKind::ExpeditionEcho { echo, .. } => {
                    if let Some((kind, title, text)) = echo_words(echo) {
                        for seat in 0..players.len() {
                            moments.push((seat, kind, title.clone(), text.clone()));
                        }
                    }
                }
                // The finale (wave 10 story): a new Descent's card on every local seat (the
                // crew's progress is shared), and the ending's pages when the Heart wakes.
                GameplayEventKind::Story {
                    what: StoryEvent::DescentReached(descent),
                    ..
                } => {
                    // First in line: a log or echo raised on the same tick takes the card.
                    let (title, text) = descent_reached_words(descent);
                    for seat in 0..players.len() {
                        moments.insert(
                            0,
                            (
                                seat,
                                crate::shell::StoryMomentKind::DescentReached,
                                title.clone(),
                                text.clone(),
                            ),
                        );
                    }
                }
                GameplayEventKind::Story {
                    what: StoryEvent::CableLit,
                    ..
                } => ending = true,
                GameplayEventKind::AnomalyEntered {
                    player_id,
                    kind,
                    first_of_kind,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        if first_of_kind {
                            let (moment, title, text) = anomaly_words(kind);
                            moments.push((seat, moment, title, text));
                        } else {
                            messages
                                .push((seat, format!("{}: the rules slip again.", kind.name())));
                        }
                    }
                }
                _ => {}
            }
        }
        for (seat, kind, title, text) in moments {
            self.shell.story_moment(seat, kind, title, text);
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        // The story's pages follow the Heart's own sequence (Wave 10 integration): each seat
        // reads them once its awakening, cutaway, credits and epilogue are done or skipped.
        let ending_elapsed = self.runtime.world().ending_elapsed();
        for seat in 0..players.len() {
            let ui = self.shell.seat_mut(seat);
            ui.story_ending_pending |= ending;
            let sequence_done =
                ending_elapsed.is_none_or(|elapsed| !ui.ending.phase(elapsed).playing());
            if ui.story_ending_pending && sequence_done {
                ui.story_ending_pending = false;
                ui.modal = Some(crate::shell::ModalScreen::Ending);
                ui.selected_menu_item = 0;
            }
        }
    }

    /// Story moments for local seats (WP-STORY): the card when a story is taken on, reaches
    /// its climax, ends or is let go, and the next beat's text when a beat is done.
    fn announce_stories(&mut self, players: &[PlayerId]) {
        use crate::shell::StoryMomentKind;
        let world = self.runtime.world();
        let mut moments: Vec<(usize, StoryMomentKind, String, String)> = Vec::new();
        let mut messages: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
            let running = |player: PlayerId, arc| {
                world
                    .arc_log(player)
                    .and_then(|log| log.running(arc))
                    .cloned()
            };
            match gameplay.kind {
                GameplayEventKind::ArcAccepted { player_id, arc } => {
                    if let (Some(seat), Some(run)) = (seat_of(player_id), running(player_id, arc))
                        && let Some(template) = run.template()
                    {
                        moments.push((
                            seat,
                            StoryMomentKind::Begun,
                            template.title.to_owned(),
                            run.journal_text(),
                        ));
                    }
                }
                GameplayEventKind::ArcStepDone { player_id, arc, .. } => {
                    if let (Some(seat), Some(run)) = (seat_of(player_id), running(player_id, arc))
                        && let Some(template) = run.template()
                    {
                        messages
                            .push((seat, format!("{}: {}", template.title, run.journal_text())));
                    }
                }
                GameplayEventKind::ArcClimax { player_id, arc } => {
                    if let (Some(seat), Some(run)) = (seat_of(player_id), running(player_id, arc))
                        && let Some(template) = run.template()
                    {
                        moments.push((
                            seat,
                            StoryMomentKind::Climax,
                            template.title.to_owned(),
                            run.journal_text(),
                        ));
                        messages.push((
                            seat,
                            format!(
                                "{}: go back to {} in {}.",
                                template.title, run.cast.giver_name, run.cast.home
                            ),
                        ));
                    }
                }
                GameplayEventKind::ArcCompleted { player_id, arc, .. }
                | GameplayEventKind::ArcAbandoned { player_id, arc } => {
                    let lost = matches!(gameplay.kind, GameplayEventKind::ArcAbandoned { .. });
                    let Some(seat) = seat_of(player_id) else {
                        continue;
                    };
                    let record = world.arc_log(player_id).and_then(|log| {
                        log.history
                            .iter()
                            .rev()
                            .find(|record| record.cast.arc == arc)
                    });
                    if let (Some(record), Some(template)) = (record, arc.template()) {
                        let text = if lost && record.ending().is_none() {
                            format!(
                                "You let this one go. {} hasn't forgotten.",
                                record.cast.giver_name
                            )
                        } else {
                            record.epilogue()
                        };
                        moments.push((
                            seat,
                            if lost {
                                StoryMomentKind::Lost
                            } else {
                                StoryMomentKind::Ended
                            },
                            template.title.to_owned(),
                            text,
                        ));
                    }
                }
                _ => {}
            }
        }
        for (seat, kind, title, text) in moments {
            self.shell.story_moment(seat, kind, title, text);
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
    }

    /// Quest news for local seats: the owner's banner when a job is done, paid, failed or a
    /// blueprint finished, and the next objective when a stage is done (every local seat for a
    /// crew quest).
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per quest event keeps the announcements in one place"
    )]
    fn announce_quests(&mut self, players: &[PlayerId]) {
        use crate::shell::FanfareKind;
        type Banner = Option<(FanfareKind, String, String)>;
        use fatherload_simulation::quest::describe::current_lines;
        let world = self.runtime.world();
        let rate = world.timing().rate;
        let mut news: Vec<(usize, Banner, Option<String>)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
            match gameplay.kind {
                GameplayEventKind::QuestStageDone { player_id, quest }
                | GameplayEventKind::QuestReady { player_id, quest } => {
                    let Some(def) = quest.def() else {
                        continue;
                    };
                    let active = world
                        .visible_quests(player_id)
                        .into_iter()
                        .find(|active| active.quest == quest && active.owner == player_id);
                    let crew = active.is_some_and(|active| active.crew);
                    let ready = matches!(gameplay.kind, GameplayEventKind::QuestReady { .. });
                    let next = active
                        .and_then(|active| {
                            current_lines(active, world.player(player_id), rate)
                                .into_iter()
                                .find(|(_, done)| !done)
                        })
                        .map(|(line, _)| line)
                        .unwrap_or_default();
                    for (seat, player) in players.iter().enumerate() {
                        if *player != player_id && !crew {
                            continue;
                        }
                        if ready {
                            news.push((
                                seat,
                                Some((
                                    FanfareKind::Ready,
                                    def.title.to_owned(),
                                    format!("Return to {} to get paid.", def.giver.name()),
                                )),
                                None,
                            ));
                        } else {
                            news.push((seat, None, Some(format!("{}: next, {next}.", def.title))));
                        }
                    }
                }
                GameplayEventKind::QuestCompleted { player_id, quest } => {
                    if let (Some(seat), Some(def)) = (seat_of(player_id), quest.def()) {
                        news.push((
                            seat,
                            Some((
                                FanfareKind::Complete,
                                def.title.to_owned(),
                                def.reward.summary(),
                            )),
                            None,
                        ));
                    }
                }
                GameplayEventKind::QuestFailed {
                    player_id,
                    quest,
                    reason,
                } => {
                    if let (Some(seat), Some(def)) = (seat_of(player_id), quest.def()) {
                        news.push((
                            seat,
                            Some((
                                FanfareKind::Failed,
                                def.title.to_owned(),
                                format!(
                                    "Failed: {}. {} will offer it again.",
                                    reason.describe(),
                                    def.giver.name()
                                ),
                            )),
                            None,
                        ));
                    }
                }
                GameplayEventKind::BlueprintCompleted {
                    player_id,
                    blueprint,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        let unlocks = match blueprint.unlock() {
                            fatherload_simulation::quest::BlueprintUnlock::Part(part) => {
                                format!("Unlocked the {} rig part.", part.title())
                            }
                            fatherload_simulation::quest::BlueprintUnlock::Recipe(recipe) => {
                                format!("Recipe filed: {recipe}.")
                            }
                        };
                        news.push((
                            seat,
                            Some((FanfareKind::Blueprint, blueprint.name().to_owned(), unlocks)),
                            None,
                        ));
                    }
                }
                _ => {}
            }
        }
        for (seat, fanfare, message) in news {
            if let Some((kind, title, detail)) = fanfare {
                self.shell.quest_fanfare(seat, kind, title, detail);
            }
            if let Some(message) = message {
                self.shell.seat_mut(seat).message = message;
            }
        }
    }

    /// Night-time banners (WP-NIGHT) for every local seat: night falling (with what the Moon
    /// means tonight), the dawn after a night with monsters about, and the rare spawn's howl.
    fn announce_night(&mut self, players: &[PlayerId]) {
        use crate::shell::FanfareKind;
        let mut news: Vec<(FanfareKind, String, String)> = Vec::new();
        let world = self.runtime.world();
        // The dawn banner is for whoever stayed up through the night: not for a night slept
        // away, nor for a seat whose player lay in bed through it.
        self.dawn_witnesses.observe(
            players
                .iter()
                .copied()
                .filter(|player| world.is_in_bed(*player)),
        );
        let night_skipped = self.frame_events.iter().any(|event| {
            matches!(
                &event.event,
                RuntimeEventKind::Gameplay(gameplay)
                    if matches!(gameplay.kind, GameplayEventKind::NightSkipped { .. })
            )
        }) || world
            .seconds_since_night_skipped()
            .is_some_and(|seconds| seconds < crate::night_text::SKIPPED_DAWN_SECONDS);
        let mut dawn_seats: Vec<usize> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::NightFell { moon, .. } => {
                    self.dawn_witnesses.night_fell();
                    news.push((
                        FanfareKind::NightFalls,
                        moon.name().to_owned(),
                        crate::night_text::moon_warning(moon).to_owned(),
                    ));
                }
                GameplayEventKind::NightSurvived { kills, .. } => {
                    dawn_seats = players
                        .iter()
                        .enumerate()
                        .filter(|(_, player)| {
                            self.dawn_witnesses.sees_dawn(**player, night_skipped)
                        })
                        .map(|(seat, _)| seat)
                        .collect();
                    self.dawn_witnesses.dawn();
                    news.push((
                        FanfareKind::NightSurvived,
                        "You made it through the night".to_owned(),
                        crate::night_text::dawn_detail(kills),
                    ));
                }
                GameplayEventKind::RareNightSpawn { .. } => news.push((
                    FanfareKind::AlphaHunts,
                    "The Umbral Alpha hunts".to_owned(),
                    "It walks only on a new moon. Stay lit, or bring it down for its heart."
                        .to_owned(),
                )),
                _ => {}
            }
        }
        for (kind, title, detail) in news {
            for seat in 0..players.len() {
                if kind == FanfareKind::NightSurvived && !dawn_seats.contains(&seat) {
                    continue;
                }
                self.shell
                    .quest_fanfare(seat, kind, title.clone(), detail.clone());
            }
        }
    }

    /// Raises the discovery banner for each local seat whose rig entered a village for the
    /// first time this frame.
    fn announce_villages(&mut self, players: &[PlayerId]) {
        let found: Vec<(usize, fatherload_simulation::village::VillageId)> = self
            .frame_events
            .iter()
            .filter_map(|event| match &event.event {
                RuntimeEventKind::Gameplay(gameplay) => match gameplay.kind {
                    GameplayEventKind::VillageDiscovered { player_id, village } => players
                        .iter()
                        .position(|other| *other == player_id)
                        .map(|seat| (seat, village)),
                    _ => None,
                },
                RuntimeEventKind::ActionResolved { .. } => None,
            })
            .collect();
        for (seat, village) in found {
            self.shell
                .village_discovered(seat, self.runtime.world(), village);
        }
    }

    /// The Moon's moments for each local seat (WP-MOON): the vacuum warning, the landing's
    /// banner and sunrise, the facility's pylons and gate, the Warden, the core and the uplink.
    #[allow(clippy::too_many_lines, reason = "one arm per Moon event")]
    fn announce_moon(&mut self, players: &[PlayerId]) {
        use crate::shell::FanfareKind;
        /// A seat, its banner, its message and whether it just landed.
        type MoonNews = (
            usize,
            Option<(FanfareKind, String, String)>,
            Option<String>,
            bool,
        );
        let mut news: Vec<MoonNews> = Vec::new();
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::MoonVacuumBreach { player_id, breach } => {
                    if let Some(seat) = seat_of(player_id) {
                        news.push((seat, None, Some(breach.message().to_owned()), false));
                    }
                }
                GameplayEventKind::MoonLanded { player_id } => {
                    if let Some(seat) = seat_of(player_id) {
                        news.push((
                            seat,
                            Some((
                                FanfareKind::Lunar,
                                "Landfall".to_owned(),
                                "You stand on the Moon. The whole world hangs below you."
                                    .to_owned(),
                            )),
                            None,
                            true,
                        ));
                    }
                }
                GameplayEventKind::FacilityPylon {
                    player_id,
                    lit,
                    reset,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        let text = if reset {
                            "The pylons gutter out. The marks must mean something...".to_owned()
                        } else {
                            format!("A pylon hums to life ({lit} of 3).")
                        };
                        news.push((seat, None, Some(text), false));
                    }
                }
                GameplayEventKind::FacilityGateOpened { player_id } => {
                    if let Some(seat) = seat_of(player_id) {
                        news.push((
                            seat,
                            None,
                            Some("The gate grinds open. Something waits beyond.".to_owned()),
                            false,
                        ));
                    }
                }
                GameplayEventKind::WardenAwakened { .. } => {
                    for seat in 0..players.len() {
                        news.push((
                            seat,
                            None,
                            Some(
                                "The Warden wakes! Break its conduits to drop its shield."
                                    .to_owned(),
                            ),
                            false,
                        ));
                    }
                }
                GameplayEventKind::WardenDefeated { by } => {
                    for (seat, player) in players.iter().enumerate() {
                        let mine = by == Some(*player);
                        news.push((
                            seat,
                            Some((
                                FanfareKind::Lunar,
                                "The Warden falls".to_owned(),
                                if mine {
                                    "The core seal opens. The Void Lance is yours.".to_owned()
                                } else {
                                    "The core seal opens.".to_owned()
                                },
                            )),
                            None,
                            false,
                        ));
                    }
                }
                GameplayEventKind::MoonCoreReached { player_id } => {
                    if let Some(seat) = seat_of(player_id) {
                        news.push((
                            seat,
                            Some((
                                FanfareKind::Lunar,
                                "The heart of the Moon".to_owned(),
                                "It is warm. It hums. It is not rock at all.".to_owned(),
                            )),
                            None,
                            false,
                        ));
                    }
                }
                GameplayEventKind::UplinkBeamed { player_id, to_moon } => {
                    if let Some(seat) = seat_of(player_id) {
                        let text = if to_moon {
                            "Beamed up to the Moon."
                        } else {
                            "Beamed back down."
                        };
                        news.push((seat, None, Some(text.to_owned()), false));
                    }
                }
                _ => {}
            }
        }
        let tick = self.shell.update_ticks;
        for (seat, fanfare, message, landed) in news {
            if let Some((kind, title, detail)) = fanfare {
                self.shell.quest_fanfare(seat, kind, title, detail);
            }
            if let Some(message) = message {
                self.shell.seat_mut(seat).message = message;
            }
            if landed {
                self.shell.seat_mut(seat).moon_landing = Some(tick);
            }
        }
    }

    /// Depth logistics news for each local seat (Part III F10): an outpost reached (it joins
    /// the travel network), a lift's arrival, and a Descent gate the rig lacks the system for.
    fn announce_logistics(&mut self, players: &[PlayerId]) {
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let (player_id, text) = match gameplay.kind {
                GameplayEventKind::OutpostReached { player_id, outpost } => (
                    player_id,
                    world.logistics().outposts.get(&outpost).map_or_else(
                        || "A deep outpost joins the travel network.".to_owned(),
                        |found| format!("{} joins the travel network.", found.name),
                    ),
                ),
                GameplayEventKind::LiftArrived { player_id, .. } => (
                    player_id,
                    world.player(player_id).map_or_else(
                        || "The cab stops.".to_owned(),
                        |rig| {
                            format!(
                                "The cab stops at row {}.",
                                rig.tile_position(fatherload_simulation::TILE_SIZE).y
                            )
                        },
                    ),
                ),
                GameplayEventKind::DescentGateBlocked {
                    player_id,
                    descent,
                    system,
                } => (
                    player_id,
                    format!(
                        "Descent {}: the depths crush the rig. It needs the {}.",
                        fatherload_simulation::depth_progression::numeral(descent),
                        system.name()
                    ),
                ),
                _ => continue,
            };
            if let Some(seat) = seat_of(player_id) {
                messages.push((seat, text));
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
    }

    /// The Living Dark's news for local seats (Wave 8): the Mycelial Mind's fight and its
    /// rewards, the Amber Court's anger at theft, the Swarm's fall, and the acid tide turning
    /// for seats in the Flesh Caverns. News with no rig of its own reaches every local seat.
    /// A card for each local seat arriving at a deep outpost that remembers the Fathers'
    /// expedition (Wave 8 integration, `expedition::memories`): Gravel Hollow's. Everywhere else
    /// the people tell their memories in conversation instead: Cathedral Station's and the Amber
    /// Court's residents (`people::deep_folk`), and since the Wave 9 integration Merrow City's,
    /// the Sunfolk Kraal's and Parallax Station's people, registered by name
    /// (`memories::Settlement::place_names`). One line a day, opened as the crew reads the trail;
    /// it reads only replicated state, so it works online too.
    fn announce_settlement_memories(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::expedition::memories::{Settlement, line_for_visit};
        let world = self.runtime.world();
        if world.descent_strata_layout().is_none() {
            return;
        }
        let day = world.people_clock().day;
        let mut cards = Vec::new();
        for (seat, player) in players.iter().enumerate() {
            let Some(rig) = world.player(*player) else {
                continue;
            };
            let tile = rig.tile_in(crate::view_frame::frame());
            let settlement = world
                .logistics()
                .outpost_at(tile)
                .and_then(|outpost| (outpost.descent == 0).then_some(Settlement::GravelHollow));
            let Some(settlement) = settlement else {
                continue;
            };
            if self.shell.seat(seat).remembered == Some((settlement, day)) {
                continue;
            }
            let Some(memory) = line_for_visit(settlement, world.expedition(), day) else {
                continue;
            };
            let place = match settlement {
                Settlement::GravelHollow => "Gravel Hollow",
                Settlement::OldDeepCamp => "Cathedral Station",
                Settlement::TheHive => "The Amber Court",
                Settlement::MerrowCity => "Merrow City",
                Settlement::JungleTribes => "The Jungle Tribes",
                Settlement::PrismRelay => "The Prism Relay",
                Settlement::ArchivePost => "The Archive Post",
                Settlement::HeartCamp => "The Heart Camp",
            };
            cards.push((seat, settlement, place, memory.told()));
        }
        for (seat, settlement, place, text) in cards {
            self.shell.seat_mut(seat).remembered = Some((settlement, day));
            self.shell
                .story_moment(seat, crate::shell::StoryMomentKind::Remembered, place, text);
        }
    }

    /// The far Strange Layers' news (Wave 9): a landmark's story beat as a card on every local
    /// seat; The Glitch's waking, rewrites (with the coming rule) and fall; amber, a shriek
    /// and a crossing into a time pocket for the seat they concern.
    fn announce_far_strange(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::far_strange::FarStrangeEvent as E;
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(String, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::FarStrange {
                what,
                player_id,
                x,
                y,
            } = gameplay.kind
            else {
                continue;
            };
            let glitch = &world.far_strange().glitch;
            let text = match what {
                E::LandmarkFound => {
                    if let Some(landmark) =
                        world.far_landmark_at(fatherload_simulation::terrain::TilePosition { x, y })
                    {
                        cards.push((landmark.name().to_owned(), landmark.beat().to_owned()));
                    }
                    continue;
                }
                E::GlitchAwoke => {
                    "The Glitch wakes. Every wound makes it rewrite this hall's rules.".to_owned()
                }
                E::GlitchRewriting => glitch.rewriting.map_or_else(
                    || "The Glitch is rewriting the hall...".to_owned(),
                    |(next, _)| format!("REWRITING: {} (brace yourself)", next.label()),
                ),
                E::GlitchRewrote => format!("Rule rewritten: {}", glitch.rule.label()),
                E::GateRewards => "The Glitch is erased. Phase Drill fitted: builder alloy yields                                    to it, and Descent VII lies beyond."
                    .to_owned(),
                E::Ambered => "Held in amber! The rig crawls until it wears off.".to_owned(),
                E::Shriek => "A shrieker screams: every stalker near knows where you are.".to_owned(),
                E::TimeShift => {
                    let slow = player_id
                        .and_then(|player| world.player(player))
                        .is_some_and(|rig| world.fields_at(rig.x, rig.y).time_scale < 1.0);
                    if slow {
                        "Time thickens: a slow pool.".to_owned()
                    } else {
                        "Time races: a fast lane.".to_owned()
                    }
                }
                _ => continue,
            };
            for (seat, player) in players.iter().enumerate() {
                if player_id.is_none_or(|owner| owner == *player) {
                    messages.push((seat, text.clone()));
                }
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (title, text) in cards {
            for seat in 0..players.len() {
                self.shell.story_moment(
                    seat,
                    crate::shell::StoryMomentKind::Anomaly,
                    title.clone(),
                    text.clone(),
                );
            }
        }
    }

    /// The buried city's news (Wave 10): a landmark's story beat and a star chart as cards on
    /// every local seat; the alarm, a crusher, a theft, a jolt and a launch for the seat they
    /// concern. (The archivist's words are the seat's HUD speech box, not a message.)
    fn announce_ancient_city(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::ancient_city::{CityEvent as E, CityLandmark};
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(String, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::AncientCity {
                what,
                player_id,
                detail,
                ..
            } = gameplay.kind
            else {
                continue;
            };
            let text = match what {
                E::LandmarkFound => {
                    if let Some(landmark) = CityLandmark::ALL.get(usize::from(detail)) {
                        cards.push((landmark.name().to_owned(), landmark.beat().to_owned()));
                    }
                    continue;
                }
                E::ChartRead => {
                    if let Some(chart) = world.star_charts().get(usize::from(detail)) {
                        cards.push((
                            format!("STAR CHART: {}", chart.name),
                            crate::shell::star_charts::chart_words(chart),
                        ));
                    }
                    continue;
                }
                E::VaultOpened => {
                    "The vault's lock gives. Its chart waits on the plinth.".to_owned()
                }
                E::AlarmRising => {
                    "The city has noticed you cutting it. Ride instead, and it forgets.".to_owned()
                }
                E::AlarmHunt => "CITY ALARM: the drones and sentinels hunt you!".to_owned(),
                E::Crushed => "Crushed! Wait for the lamp: then you have a whole beat.".to_owned(),
                E::CargoStolen => {
                    format!("A scrap crawler made off with {detail} ore. Get it back!")
                }
                E::FuelJolted => "A spark wisp's jolt drains your fuel.".to_owned(),
                E::PistonLaunched => "Thrown off the piston!".to_owned(),
                E::ArchivistWoke => "The archivist wakes, and comes with you.".to_owned(),
                _ => continue,
            };
            for (seat, player) in players.iter().enumerate() {
                if player_id.is_none_or(|owner| owner == *player) {
                    messages.push((seat, text.clone()));
                }
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (title, text) in cards {
            for seat in 0..players.len() {
                self.shell.story_moment(
                    seat,
                    crate::shell::StoryMomentKind::Anomaly,
                    title.clone(),
                    text.clone(),
                );
            }
        }
    }

    /// Descent VII part B for local seats (Wave 10): a landmark's story beat as a card on every
    /// seat; a fort's waves, the gate's verdict and the Colossus's fight as news for the seats
    /// they concern (a fort's for every seat: the garrison is the crew's fight).
    #[allow(clippy::too_many_lines, reason = "one line of news per event")]
    fn announce_garrison(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::garrison::GarrisonEvent as E;
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(String, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::Garrison {
                what,
                player_id,
                x,
                y,
            } = gameplay.kind
            else {
                continue;
            };
            let tile = fatherload_simulation::terrain::TilePosition { x, y };
            let fort = world.fort_state_at(tile);
            let waves = fort.map_or(0, |fort| fort.kind.waves());
            let wave = fort.map_or(0, |fort| fort.wave);
            let colossus = &world.garrison().colossus;
            let text = match what {
                E::LandmarkFound => {
                    if let Some(landmark) = world.garrison_landmark_at(tile) {
                        cards.push((landmark.name().to_owned(), landmark.beat().to_owned()));
                    }
                    continue;
                }
                E::FortStirred => {
                    "The garrison stirs! Build your defences: turrets, barricades, traps (build \
                     kit, Defences). Hold the fort until it sleeps."
                        .to_owned()
                }
                E::WaveTelegraphed => format!("Wave {wave} of {waves}: the red bays are opening!"),
                E::WaveWoke => format!("Wave {wave} of {waves} is out. Hold the pylon!"),
                E::WaveCleared => format!("Wave {wave} beaten. Repair and rebuild: more coming."),
                E::GarrisonSlept => {
                    "The garrison sleeps again. Its armoury pays out: credits, war plate and \
                     alloy."
                        .to_owned()
                }
                E::StoodDown => {
                    "Nobody held the fort: the garrison stands down. Its waves start over."
                        .to_owned()
                }
                E::DefenceDestroyed => "A defence was battered down!".to_owned(),
                E::Tremor => "The ground shakes: watch the ceiling for falling stones.".to_owned(),
                E::SealRefused => player_id
                    .and_then(|player| world.missing_seal_key(player))
                    .map_or_else(
                        || "The Seal Gate wants the keys of every Descent.".to_owned(),
                        |key| format!("The Seal Gate stays shut: it wants the {}.", key.name()),
                    ),
                E::SealOpening => {
                    "Every key is here. The Seal Gate's sockets light, one by one...".to_owned()
                }
                E::ColossusAwoke => {
                    "The Gatekeeper Colossus rises. Strike its head; hit its fists while they \
                     rest."
                        .to_owned()
                }
                E::HallCollapsing => colossus.collapsing.map_or_else(
                    || "The hall is giving way!".to_owned(),
                    |(next, _)| match next {
                        fatherload_simulation::garrison::ColossusStage::Fallen => {
                            "The floor is cracking! Get off the cracks or ride them down."
                                .to_owned()
                        }
                        _ => {
                            "The vault is coming down! Its rubble will hang in the air.".to_owned()
                        }
                    },
                ),
                E::HallCollapsed => format!("The hall gave way: {}.", colossus.stage.name()),
                E::GateRewards => "The Gatekeeper falls. The Heart Key is yours: the Heart Seal \
                                   yields, and the floor opens on Descent VIII."
                    .to_owned(),
                _ => continue,
            };
            let crew_wide = matches!(
                what,
                E::FortStirred
                    | E::WaveTelegraphed
                    | E::WaveWoke
                    | E::WaveCleared
                    | E::StoodDown
                    | E::Tremor
                    | E::ColossusAwoke
                    | E::HallCollapsing
                    | E::HallCollapsed
                    | E::SealOpening
            );
            for (seat, player) in players.iter().enumerate() {
                if crew_wide || player_id.is_none_or(|owner| owner == *player) {
                    messages.push((seat, text.clone()));
                }
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (title, text) in cards {
            for seat in 0..players.len() {
                self.shell.story_moment(
                    seat,
                    crate::shell::StoryMomentKind::Anomaly,
                    title.clone(),
                    text.clone(),
                );
            }
        }
    }

    /// A deep supply spent (Wave 10 polish): its news line for the seat whose kit it came from.
    fn announce_supplies(&mut self, players: &[PlayerId]) {
        let mut messages: Vec<(usize, &'static str)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::Supply {
                what, player_id, ..
            } = gameplay.kind
            else {
                continue;
            };
            for (seat, player) in players.iter().enumerate() {
                if player_id == Some(*player) {
                    messages.push((seat, what.news()));
                }
            }
        }
        for (seat, message) in messages {
            message.clone_into(&mut self.shell.seat_mut(seat).message);
        }
    }

    fn announce_living_dark(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::living_dark::LivingDarkEvent as E;
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::LivingDark {
                what, player_id, ..
            } = gameplay.kind
            else {
                continue;
            };
            let standing = world.living_dark().hive.standing;
            let text = match (what, player_id) {
                (E::MindAwakened, _) => {
                    "The Mycelial Mind wakes! Its nerve nodes shield it: cut them down.".to_owned()
                }
                (E::MindExposed, _) => "The nodes are down: the Mind is exposed!".to_owned(),
                (E::MindRegrew, _) => "The Mind's nerve nodes grow back.".to_owned(),
                (E::GateRewards, _) => "The Mind falls. Bio-Filter and Lamp II fitted: spores \
                                         and acid cannot touch the rig now."
                    .to_owned(),
                (E::CourtHostile, _) => {
                    "The Amber Court turns on the crew! Make amends at its counter.".to_owned()
                }
                (E::CourtStanding, None) => {
                    format!("The Amber Court saw its comb taken (standing {standing}).")
                }
                (E::SwarmBroken, _) => "The matriarch is dead: the Swarm is broken.".to_owned(),
                (E::TideRising, _) => "The acid tide is rising.".to_owned(),
                _ => continue,
            };
            let tide = matches!(what, E::TideRising);
            for (seat, player) in players.iter().enumerate() {
                let mine = player_id.is_none_or(|owner| owner == *player);
                // The tide only matters to a seat in the Flesh Caverns.
                if mine && (!tide || world.heartbeat_for(*player).is_some()) {
                    messages.push((seat, text.clone()));
                }
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
    }

    /// The Drowned and Frozen's news for each local seat (Wave 9): the bosses' turns, the
    /// gate's rewards, warnings for its own rig (frost, pressure), and a story card for each
    /// story beat the crew finds (the beat of the stratum it was found in).
    fn announce_drowned(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::drowned::{DrownedEvent as E, DrownedLore};
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(usize, DrownedLore)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::Drowned {
                what,
                player_id,
                x,
                y,
            } = gameplay.kind
            else {
                continue;
            };
            if what == E::LoreFound {
                let stratum = world
                    .terrain()
                    .stratum_at(fatherload_simulation::terrain::TilePosition { x, y });
                if let Some(lore) = DrownedLore::ALL
                    .into_iter()
                    .find(|lore| lore.stratum() == stratum)
                {
                    for seat in 0..players.len() {
                        cards.push((seat, lore));
                    }
                }
                continue;
            }
            let text = match what {
                E::LeviathanAwoke => {
                    "The Leviathan rises from the trench! Watch for its roar: then get out of its \
                     line."
                }
                E::LeviathanCharge => "It roars: a charge is coming!",
                E::LeviathanPhase => match world.drowned().leviathan.phase {
                    fatherload_simulation::drowned::LeviathanPhase::Maelstrom => {
                        "The Leviathan sinks and opens its maw: the maelstrom pulls. Strike the maw!"
                    }
                    fatherload_simulation::drowned::LeviathanPhase::Thrashing => {
                        "The Leviathan thrashes! The roof shakes loose and sharks come."
                    }
                    fatherload_simulation::drowned::LeviathanPhase::Circling => continue,
                },
                E::GateRewards => {
                    "The Leviathan falls. Sonar Suite and Cryo Hull fitted: the way to the Hollow \
                     World is open."
                }
                E::EelDragonAwoke => "The eel-dragon uncoils in its grotto!",
                E::EelDragonPulse => "The eel-dragon crackles: a pulse is coming. Get clear!",
                E::EelDragonDefeated => "The eel-dragon is dead. Its bounty is yours.",
                E::FrostWarning => "Hull frost is biting: warm the rig at a thermal vent.",
                E::PressureWarning => "The seals are failing under the pressure: rise!",
                E::ClamPearl => "An Abyss Pearl!",
                E::ClamSnapped => "The clam snapped shut on the drill.",
                E::CityReached => "Merrow City: the fish-folk welcome you into their bubble.",
                _ => continue,
            };
            for (seat, player) in players.iter().enumerate() {
                if player_id.is_none_or(|owner| owner == *player) {
                    messages.push((seat, text.to_owned()));
                }
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (seat, lore) in cards {
            self.shell.story_moment(
                seat,
                crate::shell::StoryMomentKind::DeepLore,
                lore.title(),
                lore.text(),
            );
        }
    }

    /// Descent VIII's news for each local seat (Wave 10): dark matter's weight, wells, rails,
    /// the Heart's beats; a story card when the crew finds one of its places.
    /// Space flight for local seats (Wave 11): the seat whose rig it was hears what happened
    /// (a refused warp says why; low air says what to do), and docking opens the station's
    /// counter.
    /// Space's things for local seats (Wave 12 systems): what a star, a hole, a wormhole, a
    /// nebula, a rock or a comet did, in plain words for the seat whose rig it was.
    fn announce_systems(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::flight::systems::SystemsEvent as E;
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        let mut news: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::SpaceSystems {
                what,
                player_id,
                detail,
            } = gameplay.kind
            else {
                continue;
            };
            let Some(seat) = seat_of(player_id) else {
                continue;
            };
            let message = match what {
                E::FlareWarning => format!(
                    "Solar flare in {detail} s: get behind a world or fly out of its reach."
                ),
                E::HoleStrain => "Escape thrust failing: burn away from the hole now.".to_owned(),
                E::HoleNoReturn => {
                    "Past the point of no return: call the emergency beacon (nav chart).".to_owned()
                }
                E::HoleRescued => format!(
                    "The event horizon: the beacon's tug pulled you out for {detail} credits; \
                     the hold went into the hole."
                ),
                E::WormholeEntered if detail == 1 => {
                    "Wormhole transit: a new route, now on the nav HUD.".to_owned()
                }
                E::OreMined => {
                    let ore = fatherload_simulation::space::AsteroidOre::ALL
                        .get(usize::try_from(detail).unwrap_or(0))
                        .copied()
                        .unwrap_or_default();
                    format!(
                        "+1 {} from the rock ({}).",
                        fatherload_simulation::flight::systems::asteroid_mineral(ore).name(),
                        ore.name()
                    )
                }
                E::CometDust => format!(
                    "+1 comet dust ({detail}/{}).",
                    fatherload_simulation::flight::systems::COMET_HARVEST_CAP
                ),
                E::ParkingHold => "Parking hold: thrust to fly on.".to_owned(),
                other => other.headline().to_owned(),
            };
            news.push((seat, message));
        }
        for (seat, message) in news {
            self.shell.seat_mut(seat).message = message;
        }
    }

    fn announce_flight(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::flight::{FlightEvent, WarpRefusal};
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        let mut news: Vec<(usize, FlightEvent, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let GameplayEventKind::Flight {
                what,
                player_id,
                detail,
            } = gameplay.kind
            else {
                continue;
            };
            let Some(seat) = seat_of(player_id) else {
                continue;
            };
            let message = match what {
                FlightEvent::WarpRefused => format!(
                    "Warp unavailable: {}.",
                    WarpRefusal::from_code(detail).map_or("not now", WarpRefusal::reason)
                ),
                FlightEvent::AirLow => format!(
                    "Air at {detail}%: dock at a station, land somewhere breathable, or call \
                     the emergency beacon (nav chart)."
                ),
                FlightEvent::BeaconReturn => {
                    format!("Emergency beacon: towed home for {detail} credits.")
                }
                FlightEvent::EnteredSpace => {
                    "Open space. The map button opens the nav chart; the anchor button cruises."
                        .to_owned()
                }
                FlightEvent::ScannerPing => format!("Scanner ping: {detail} new star systems."),
                FlightEvent::WarpInterrupted if detail == 1 => {
                    "Warp interrupted: a gravity well pulled you out.".to_owned()
                }
                FlightEvent::CruiseDropped if detail == 1 => {
                    "Cruise needs open space: too close to a body.".to_owned()
                }
                other => other.headline().to_owned(),
            };
            news.push((seat, what, message));
        }
        for (seat, what, message) in news {
            let ui = self.shell.seat_mut(seat);
            ui.message = message;
            if what == FlightEvent::Docked && ui.modal.is_none() {
                ui.modal = Some(crate::shell::ModalScreen::Station);
                ui.selected_menu_item = 0;
            }
        }
    }

    fn announce_heart(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::heart::HeartEvent as E;
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(usize, &'static str, &'static str)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::HeartLandmarkFound { landmark, .. } => {
                    // The crew's find: every local seat reads the place's story.
                    for seat in 0..players.len() {
                        cards.push((seat, landmark.name(), landmark.story()));
                    }
                }
                GameplayEventKind::Heart {
                    what, player_id, ..
                } => {
                    let text = match what {
                        E::RigFloating => {
                            "Too much dark matter: the rig falls UP. Roll over and thrust, or load ore."
                        }
                        E::RigSettled => "Heavy enough again: the rig falls the right way.",
                        E::WellCaught => "A gravity well has you: thrust out before the horizon.",
                        E::HorizonCrush => "THE HORIZON is crushing the hull!",
                        E::RailCatch => "The magnetic rail catches the rig.",
                        E::MothDrank => "A null moth drank a lump of dark matter from the bay.",
                        E::HeartStirred => "The Star Drive stirs. Its coils begin to turn.",
                        E::Ignition => "IGNITION: the core is open. Strike it!",
                        E::Collapse => {
                            "The core collapses into a singularity: strike as it exhales."
                        }
                        E::Reforged => {
                            "The core reseals: break the inner coils while the drive pulls."
                        }
                        E::Eclipse => {
                            "ECLIPSE: a beam sweeps while it holds its breath. Strike as it exhales."
                        }
                        E::HeartSlept => "Nobody stayed. The Heart sinks back to sleep.",
                        E::StarDriveFitted => "The Star Drive is your engine now.",
                        E::StarsOpen => "Above the Moon: the stars are open.",
                        _ => continue,
                    };
                    for (seat, player) in players.iter().enumerate() {
                        if player_id.is_none_or(|owner| owner == *player) {
                            messages.push((seat, text.to_owned()));
                        }
                    }
                }
                // Round bodies (Wave 11): a bad touchdown tells the pilot what happened.
                GameplayEventKind::Planet {
                    what, player_id, ..
                } => {
                    use fatherload_simulation::planet::landing::PlanetEvent as P;
                    let text = match what {
                        P::Landed | P::Slid => continue,
                        P::HullScraped => "Hard, tilted touchdown: the hull scraped. Land level.",
                        P::Tipped => {
                            "TIPPED OVER! Hold the rotation thrusters toward upright to right it."
                        }
                        P::Righted => "Back on the tracks.",
                    };
                    for (seat, player) in players.iter().enumerate() {
                        if player_id.is_none_or(|owner| owner == *player) {
                            messages.push((seat, text.to_owned()));
                        }
                    }
                }
                _ => {}
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (seat, title, text) in cards {
            self.shell
                .story_moment(seat, crate::shell::StoryMomentKind::HeartPlace, title, text);
        }
    }

    /// The distortion strata's news for each local seat (Wave 9): the first mirror hall, twins,
    /// gazes, reflections, data cores and deeper rot; a story card when the crew finds one of
    /// the Strange Layers' places.
    fn announce_distortion(&mut self, players: &[PlayerId]) {
        use fatherload_simulation::distortion::DistortionEvent as E;
        let mut messages: Vec<(usize, String)> = Vec::new();
        let mut cards: Vec<(usize, &'static str, &'static str)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::DistortionLandmarkFound { landmark, .. } => {
                    // The crew's find: every local seat reads the place's story.
                    for seat in 0..players.len() {
                        cards.push((seat, landmark.name(), landmark.story()));
                    }
                }
                GameplayEventKind::Distortion {
                    what, player_id, ..
                } => {
                    let text = match what {
                        E::MirrorEntered => "A mirror hall: the screen and the controls flip.",
                        E::TwinAppeared => {
                            "Your reflection steps out of the glass. It moves as you move."
                        }
                        E::TwinBroken => "The twin shatters. The hall's glass goes quiet.",
                        E::GhostShattered => "Only a reflection: it shatters into nothing.",
                        E::Gazed => "A gazer has your eye: your controls are mirrored!",
                        E::ShotReflected => "The glassback's face throws your shot back!",
                        E::DataCore => "A whole block of data ore: a data core recovered.",
                        E::RotDeepened => "The rot deepens: the world loses resolution.",
                        E::ShearEntered => "Gravity turns here. Turn the rig to land on it.",
                        _ => continue,
                    };
                    for (seat, player) in players.iter().enumerate() {
                        if player_id.is_some_and(|owner| owner == *player) {
                            messages.push((seat, text.to_owned()));
                        }
                    }
                }
                _ => {}
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
        for (seat, title, text) in cards {
            self.shell.story_moment(
                seat,
                crate::shell::StoryMomentKind::StrangePlace,
                title,
                text,
            );
        }
    }

    /// Tells each local seat what its rig found or opened at a point of interest, and seats
    /// near a vault when its lock opens or resets.
    fn announce_points_of_interest(&mut self, players: &[PlayerId]) {
        let world = self.runtime.world();
        let mut messages: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
            match gameplay.kind {
                GameplayEventKind::PoiOpened {
                    player_id,
                    poi,
                    reward,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((seat, format!("{}: {}.", poi.name(), reward.summary())));
                    }
                }
                GameplayEventKind::PoiDiscovered {
                    player_id, feature, ..
                } => {
                    let Some(seat) = seat_of(player_id) else {
                        continue;
                    };
                    let Some(variant) = world
                        .world_feature(feature)
                        .as_ref()
                        .and_then(fatherload_simulation::worldgen::WorldFeature::poi)
                    else {
                        continue;
                    };
                    let text = match variant {
                        fatherload_simulation::PoiVariant::Vault(puzzle) => {
                            format!("Found a sealed vault. {}", puzzle.hint())
                        }
                        other => format!("Found: {}.", other.title()),
                    };
                    messages.push((seat, text));
                }
                GameplayEventKind::CaveDiscovered {
                    player_id, cave, ..
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((seat, format!("Found: {}! {}.", cave.name(), cave.blurb())));
                    }
                }
                GameplayEventKind::CaveCacheOpened {
                    player_id,
                    tile,
                    credits,
                    furniture,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((seat, cache_message(tile, credits, furniture)));
                    }
                }
                GameplayEventKind::VaultOpened { feature }
                | GameplayEventKind::VaultLockReset { feature } => {
                    let Some(vault) = world.world_feature(feature) else {
                        continue;
                    };
                    let opened = matches!(gameplay.kind, GameplayEventKind::VaultOpened { .. });
                    for (seat, player) in players.iter().enumerate() {
                        let near = world.player(*player).is_some_and(|rig| {
                            vault
                                .bounds
                                .expanded(12)
                                .contains(rig.tile_in(crate::view_frame::frame()))
                        });
                        if near {
                            messages.push((
                                seat,
                                if opened {
                                    "The vault's lock gives way: the doors are open!".to_owned()
                                } else {
                                    "The vault's lock resets.".to_owned()
                                },
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
    }

    fn handle_request(
        &mut self,
        request: ShellRequest,
        seat_actions: &mut [Vec<PlayerCommand>],
        report: &mut FrameReport,
    ) {
        match request {
            ShellRequest::NewGame {
                seats,
                settings,
                test_mode,
                enemies,
                day_length,
            } => self.new_game(seats, &settings, (test_mode, enemies, day_length), report),
            ShellRequest::ResumeLatest => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_latest_world(),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, "Resumed saved session.", report);
            }
            ShellRequest::QuickLoad => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_world(),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, "Game loaded.", report);
            }
            ShellRequest::LoadSlot(slot) => {
                let loaded = match &self.save_target {
                    SaveTarget::UserFiles => save::load_world_slot(slot),
                    SaveTarget::File(path) => save::load_world_from_path(path),
                };
                self.apply_load(loaded, &format!("Loaded slot {}.", slot + 1), report);
            }
            ShellRequest::QuickSave => {
                let result = self.write_save(None);
                self.report_save(result, "Game saved.");
            }
            ShellRequest::SaveSlot(slot) => {
                let result = self.write_save(Some(slot));
                self.report_save(result, &format!("Saved slot {}.", slot + 1));
            }
            ShellRequest::SaveAndExit => match self.write_save(None) {
                Ok(()) => self.shell.request_exit = true,
                Err(error) => self.shell.message = format!("Save before exit failed: {error}"),
            },
            ShellRequest::Action { seat, command } => {
                if let Some(actions) = seat_actions.get_mut(seat) {
                    actions.push(command);
                }
            }
            ShellRequest::AddLocalSeat => self.add_local_seat(),
            ShellRequest::HostOnline {
                settings,
                test_mode,
                enemies,
                day_length,
            } => self.host_online(&settings, (test_mode, enemies, day_length), report),
            ShellRequest::OpenOnline => self.open_online(),
            ShellRequest::JoinAddress(text) => match parse_join_addr(&text) {
                Some(addr) => self.start_join(JoinTarget::Addr(addr), None, report),
                None => self.shell.message = format!("Not an address: {text}"),
            },
            ShellRequest::JoinDescriptor(text) => match HostDescriptor::from_json(&text) {
                Ok(descriptor) => self.start_join(JoinTarget::Descriptor(descriptor), None, report),
                Err(error) => self.shell.message = format!("Not a host descriptor: {error}"),
            },
            ShellRequest::BrowseLan => self.lan_browse = Some(lan::browse()),
            ShellRequest::JoinLan(index) => {
                if let Some(game) = self.shell.online.lan_games.get(index) {
                    self.shell.online.lan_status = format!("Contacting {}...", game.instance_name);
                    self.lan_fetch = Some(lan::fetch(game));
                }
            }
            ShellRequest::SetReady(ready) => self.runtime.set_ready(ready),
            ShellRequest::StartOnline => {
                self.runtime.start_online_game();
                let play_seconds = self.resumed_play_seconds.take().unwrap_or(0.0);
                self.shell
                    .enter_world("The session started. Good digging!", play_seconds);
                self.shell.sound_cues.push(SoundCue::Milestone);
            }
            ShellRequest::LeaveOnline => self.leave_online(report),
            ShellRequest::Reconnect => {
                if let Some((target, token)) = self.last_join.clone() {
                    self.start_join(target, token, report);
                }
            }
            ShellRequest::CopyToClipboard(text) => report.clipboard = Some(text),
            ShellRequest::AdjustTuning { parameter, step } => {
                self.shell.message = self.runtime.adjust_tuning(parameter, step);
            }
            ShellRequest::Exit => self.shell.request_exit = true,
            ShellRequest::ClaimRig(player) => {
                self.runtime.claim_rig(player);
                "Asked the host for that saved rig...".clone_into(&mut self.shell.message);
            }
            ShellRequest::ResolveClaim { client, approve } => {
                self.runtime.resolve_claim(client, approve);
            }
            ShellRequest::HostLoadedWorld => self.host_loaded_world(report),
        }
    }

    fn new_game(
        &mut self,
        seats: usize,
        settings: &WorldSettings,
        (test_mode, enemies, day_length): (bool, EnemyDifficulty, DayLength),
        report: &mut FrameReport,
    ) {
        self.runtime = Runtime::local_as(
            world_from_setup(settings, test_mode, enemies, day_length),
            seats,
            self.identity(),
        );
        self.reset_presentation(report);
        self.shell.enter_world(
            if test_mode {
                "TEST MODE: maxed rig, every part, 1,000,000 cr. Pause for warps and god toggles."
            } else {
                "Welcome to the dig site. Visit the depot for contracts."
            },
            0.0,
        );
        if seats > 1 {
            self.shell.message = split_screen_message(seats);
        }
        self.shell.sound_cues.push(SoundCue::Milestone);
    }

    /// Adds a split-screen seat for a controller that claimed one; returns its index.
    pub fn add_seat_for_controller(&mut self) -> Option<usize> {
        if self.seats().len() <= 1 || !self.shell.gameplay_visible() {
            return None;
        }
        self.runtime.add_local_seat()?;
        Some(self.seats().len() - 1)
    }

    fn add_local_seat(&mut self) {
        if let Some(seat) = self.runtime.add_local_seat() {
            self.shell.message = format!(
                "Player {} joined local split-screen ({} seats).",
                seat.seat.0 + 1,
                self.seats().len()
            );
        } else {
            self.shell.message = format!(
                "Local split-screen is limited to {} seats.",
                crate::runtime::MAX_LOCAL_SEATS
            );
        }
    }

    fn leave_online(&mut self, report: &mut FrameReport) {
        let message = match self.runtime.online_role() {
            Some(OnlineRole::Host) => "You ended the online session.",
            Some(OnlineRole::Client) => "You left the online session.",
            None => "",
        };
        self.go_local(report);
        self.shell.return_to_title(message);
    }

    /// Mirrors the runtime's online facts into the shell and reacts to a client's session
    /// ending (host ended it, rejected us, connection lost).
    fn sync_online(&mut self, report: &mut FrameReport) {
        self.shell.online.role = self.runtime.online_role();
        self.shell.online.lobby = self.runtime.lobby_info();
        for notice in self.runtime.take_notices() {
            self.shell.message = notice;
        }
        self.present_crew_toasts();
        if self.shell.online.show_net_debug {
            self.shell.online.net_debug = self.runtime.net_debug_lines();
        }
        if let Some(error) = self.runtime.host_failure() {
            self.go_local(report);
            self.shell
                .online_notice(format!("Could not host: {error}"), false);
            return;
        }
        match self.runtime.client_state() {
            Some(ClientState::InGame) if !self.client_entered => {
                self.client_entered = true;
                if self.shell.run_mode == RunMode::Lobby
                    && self.shell.online.screen == OnlineScreen::Lobby
                {
                    self.shell.enter_world("Joined the session.", 0.0);
                    self.shell.sound_cues.push(SoundCue::Milestone);
                }
            }
            Some(ClientState::Finished {
                message,
                can_reconnect,
            }) => {
                if let Runtime::Client(client) = &self.runtime {
                    self.last_join = Some((client.target().clone(), client.reconnect_token()));
                }
                self.go_local(report);
                self.shell.online_notice(message, can_reconnect);
            }
            _ => {}
        }
    }

    fn poll_lan(&mut self, report: &mut FrameReport) {
        if let Some(result) = self.lan_browse.as_ref().and_then(Background::poll) {
            self.lan_browse = None;
            match result {
                Ok(games) => {
                    self.shell.online.lan_status = if games.is_empty() {
                        "No LAN games found. Refresh to search again.".to_owned()
                    } else {
                        format!("Found {} LAN game(s).", games.len())
                    };
                    self.shell.online.lan_games = games;
                }
                Err(error) => {
                    self.shell.online.lan_status = format!("LAN discovery failed: {error}");
                }
            }
        }
        if let Some(result) = self.lan_fetch.as_ref().and_then(Background::poll) {
            self.lan_fetch = None;
            match result {
                Ok(descriptor) => {
                    self.start_join(JoinTarget::Descriptor(descriptor), None, report);
                }
                Err(error) => {
                    self.shell.online.lan_status = format!("Could not reach that game: {error}");
                }
            }
        }
    }

    fn host_online(
        &mut self,
        settings: &WorldSettings,
        (test_mode, enemies, day_length): (bool, EnemyDifficulty, DayLength),
        report: &mut FrameReport,
    ) {
        let identity = self.identity();
        match LocalHost::online(
            world_from_setup(settings, test_mode, enemies, day_length),
            1,
            identity.clone(),
            self.host_bind,
            true,
        ) {
            Ok(mut host) => {
                host.set_host_name(&identity.name);
                self.go_online(Runtime::Host(Box::new(host)), report);
                self.shell.online.ready = false;
                self.shell.show_online(OnlineScreen::Lobby);
                "Hosting: share the address or descriptor, then start the game."
                    .clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not host: {error}"),
        }
    }

    fn open_online(&mut self) {
        let bind = self.host_bind;
        let name = self.identity().name;
        let Runtime::Host(host) = &mut self.runtime else {
            return;
        };
        match host.open_online(bind, true) {
            Ok(()) => {
                host.set_host_name(&name);
                "Opened to LAN/online. Pause > Online Session shows the address."
                    .clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not open the game: {error}"),
        }
    }

    fn start_join(
        &mut self,
        target: JoinTarget,
        token: Option<ReconnectToken>,
        report: &mut FrameReport,
    ) {
        let fallback = self.runtime.world().clone();
        let identity = self.identity();
        let profile = self
            .profile
            .as_ref()
            .map(crate::profile::PlayerProfile::net_id);
        match OnlineClient::join(target.clone(), identity.name, token, profile, fallback) {
            Ok(client) => {
                self.last_join = Some((target, token));
                self.go_online(Runtime::Client(Box::new(client)), report);
                self.shell.online.ready = false;
                self.shell.show_online(OnlineScreen::Lobby);
                "Connecting...".clone_into(&mut self.shell.message);
            }
            Err(error) => self.shell.message = format!("Could not join: {error}"),
        }
    }

    /// Replaces the runtime with an online one, ending any previous session.
    fn go_online(&mut self, runtime: Runtime, report: &mut FrameReport) {
        let previous = std::mem::replace(&mut self.runtime, runtime);
        end_runtime(previous);
        self.client_entered = false;
        self.autosave_seconds = 0.0;
        self.reset_presentation(report);
    }

    /// Ends the online session (host: tell clients; client: leave) and idles a fresh local world.
    fn go_local(&mut self, report: &mut FrameReport) {
        self.go_online(Runtime::local(new_world(), 1), report);
    }

    /// Best-effort checkpoint on tab suspension; explicit slot saves remain available.
    #[cfg(target_os = "emscripten")]
    pub fn save_browser_checkpoint(&mut self) {
        if self.shell.run_mode != RunMode::Title && self.runtime.can_save() {
            let result = self.write_save(None);
            self.report_save(result, "Saved this browser session.");
        }
    }

    /// A tomb shrine saves the run (WP-TOMBS): whoever can write the session save (a local
    /// game or the host) writes it when any member rests at one. Clients never save.
    fn save_at_shrines(&mut self) {
        let rested = self.frame_events.iter().any(|event| {
            matches!(
                &event.event,
                RuntimeEventKind::Gameplay(gameplay)
                    if matches!(gameplay.kind, GameplayEventKind::TombShrineRested { .. })
            )
        });
        if rested && self.runtime.can_save() && self.shell.run_mode != RunMode::Title {
            let result = self.write_save(None);
            self.report_save(result, "The shrine keeps your run: saved.");
        }
    }

    fn autosave(&mut self, delta_seconds: f32) {
        let hosting = self.runtime.online_role() == Some(OnlineRole::Host)
            && self.shell.online.started()
            && !matches!(self.shell.run_mode, RunMode::Title);
        #[cfg(target_os = "emscripten")]
        let hosting = hosting || self.shell.run_mode != RunMode::Title;
        if !hosting {
            self.autosave_seconds = 0.0;
            return;
        }
        self.autosave_seconds += delta_seconds;
        if self.autosave_seconds >= AUTOSAVE_SECONDS {
            self.autosave_seconds = 0.0;
            let result = self.write_save(None);
            self.report_save(result, "Autosaved the session.");
        }
    }

    fn write_save(&self, slot: Option<usize>) -> Result<(), SaveError> {
        if !self.runtime.can_save() {
            return Err(SaveError::SaveDenied(
                "only the host can write the session save".to_owned(),
            ));
        }
        let save = self.world_save();
        match (&self.save_target, slot) {
            (SaveTarget::UserFiles, None) => save::save_world(&save),
            (SaveTarget::UserFiles, Some(slot)) => save::save_world_slot(&save, slot),
            (SaveTarget::File(path), _) => save::save_world_to_path(&save, path),
        }
    }

    fn report_save(&mut self, result: Result<(), SaveError>, success: &str) {
        match result {
            Ok(()) => {
                self.shell.save_dirty = false;
                success.clone_into(&mut self.shell.message);
                self.shell.sound_cues.push(SoundCue::Ui);
            }
            Err(error) => {
                self.shell.message = format!("Save failed: {error}");
                self.shell.sound_cues.push(SoundCue::UiError);
            }
        }
    }

    fn apply_load(
        &mut self,
        loaded: Result<WorldSave, SaveError>,
        success: &str,
        report: &mut FrameReport,
    ) {
        match loaded {
            Ok(save) => {
                let from_title = self.shell.run_mode == RunMode::Title;
                if from_title {
                    // From the title, a load starts a fresh session with as many seats as the
                    // save's split-screen crew (each takes its saved rig back).
                    let identity = self.identity();
                    let seats = crew::saved_seat_count(&save.world, &identity);
                    self.runtime = Runtime::local_as(save.world, seats, identity);
                } else if !self.runtime.load_world(save.world) {
                    let refusal = if self.runtime.can_save() {
                        "That save runs at another tick rate than this online session."
                    } else {
                        "Joined clients cannot load over the host's session."
                    };
                    refusal.clone_into(&mut self.shell.message);
                    self.shell.sound_cues.push(SoundCue::UiError);
                    return;
                }
                self.reset_presentation(report);
                self.shell.enter_world(success, save.play_seconds);
                self.shell.sound_cues.push(SoundCue::Ui);
                if from_title {
                    self.offer_resume_hosting();
                }
            }
            Err(error) => {
                self.shell.message = format!("Load failed: {error}");
                self.shell.sound_cues.push(SoundCue::UiError);
            }
        }
    }

    fn reset_presentation(&mut self, report: &mut FrameReport) {
        self.shell.set_seat_count(self.runtime.local_seats().len());
        self.effects.clear();
        self.camera_leads.clear();
        self.events = EventFeed::default();
        self.frame_events.clear();
        report.world_replaced = true;
    }
}

/// Ends a runtime that is being replaced: a host ends its online session for its clients, a
/// client leaves (the network threads stop in the background).
fn end_runtime(runtime: Runtime) {
    match runtime {
        Runtime::Host(mut host) => host.end_session(),
        Runtime::Client(client) => client.leave(),
    }
}

/// Parses `ip:port`, or a bare IP with the default port.
fn parse_join_addr(text: &str) -> Option<SocketAddr> {
    let text = text.trim();
    text.parse().ok().or_else(|| {
        text.parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| SocketAddr::new(ip, DEFAULT_PORT))
    })
}

/// The shell's read-only view of the world. `probe_saves` controls whether save files are looked
/// up on disk: only the title menu reads `saves_exist`, so gameplay frames skip the file-system
/// probes on the game thread.
fn shell_context<'a>(
    runtime: &'a Runtime,
    save_target: &SaveTarget,
    players: &'a [PlayerId],
    probe_saves: bool,
) -> ShellContext<'a> {
    ShellContext {
        world: runtime.world(),
        players,
        saves_exist: probe_saves
            && match save_target {
                SaveTarget::UserFiles => save::saves_exist(),
                SaveTarget::File(path) => path.exists(),
            },
    }
}

/// A brand-new single-rig world with the default settings (the idle world behind the title,
/// tests and scripted scenes).
#[must_use]
pub fn new_world() -> WorldState {
    WorldState::new_game(save::LEGACY_PLAYER_ID)
}

/// A brand-new single-rig world generated from `settings` (a "New Game" from the world setup
/// dialog).
#[must_use]
pub fn world_from_settings(settings: &WorldSettings) -> WorldState {
    WorldState::new_game_with_settings(
        save::LEGACY_PLAYER_ID,
        settings,
        fatherload_simulation::SimulationParams::default(),
    )
}

/// A brand-new world from the world setup dialog: generated from `settings`, in test mode when
/// `test_mode` is set, with `enemies` as its enemy setting and `day_length` as its clock's day.
#[must_use]
pub fn world_from_setup(
    settings: &WorldSettings,
    test_mode: bool,
    enemies: EnemyDifficulty,
    day_length: DayLength,
) -> WorldState {
    let mut world = world_from_settings(settings);
    if test_mode {
        world.enable_test_mode();
    }
    world.set_enemy_difficulty(enemies);
    world.set_day_length(day_length);
    world
}

/// The message for opening a cave cache (and the antique piece it held, WP-HOME).
fn cache_message(
    tile: fatherload_simulation::cave::CaveTile,
    credits: u32,
    furniture: Option<fatherload_simulation::home::Furniture>,
) -> String {
    let name = if tile == fatherload_simulation::cave::CaveTile::RelicCache {
        "Relic cache"
    } else {
        "Treasure cache"
    };
    furniture.map_or_else(
        || format!("{name}: +{credits} credits, and loot spills out!"),
        |piece| {
            format!(
                "{name}: +{credits} credits, and an antique {} for your home!",
                piece.name().to_lowercase()
            )
        },
    )
}

#[cfg(test)]
mod tests {
    //! Headless app-level tests: scripted input through `LocalGame::frame`, exactly the path the
    //! window drives, with the runtime's `HostSession` as the only gameplay authority.

    use fatherload_simulation::{
        economy::SurfaceZone,
        surface::SURFACE_BUILDINGS,
        terrain::{MineralKind, TILE_SIZE},
    };

    use super::*;
    use crate::{
        render_snapshot::{SCREEN_HEIGHT, SCREEN_WIDTH},
        runtime::MAX_LOCAL_SEATS,
        shell::{ModalScreen, PauseOption, TitleOption, interior_service_x},
    };

    mod base_tests;
    mod crew_tests;
    mod expedition_tests;
    mod garage_tests;
    mod home_tests;
    mod logistics_tests;
    mod pilot_tests;
    mod quest_tests;
    mod space_seat_tests;
    mod split_seat_tests;
    mod test_mode_tests;
    mod tomb_tests;
    mod village_tests;

    const DT: f32 = 1.0 / 60.0;
    const PLAYER: PlayerId = save::LEGACY_PLAYER_ID;

    fn temp_save(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fatherload-local-game-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ))
    }

    fn title_game(name: &str) -> LocalGame {
        LocalGame::new(SettingsFile::default(), SaveTarget::File(temp_save(name)))
    }

    /// A game already playing on `world` with `seats` seats.
    fn playing(world: WorldState, seats: usize, name: &str) -> LocalGame {
        let mut game = title_game(name);
        game.runtime = Runtime::local(world, seats);
        game.shell.enter_world("test", 0.0);
        game
    }

    fn step(game: &mut LocalGame, input: PlayerInput) -> FrameReport {
        game.frame(&FrameInput::primary(input), DT)
    }

    fn idle(game: &mut LocalGame, frames: usize) {
        for _ in 0..frames {
            let _report = step(game, PlayerInput::default());
        }
    }

    fn press(game: &mut LocalGame, input: PlayerInput) -> FrameReport {
        let report = step(game, input);
        idle(game, 1);
        report
    }

    fn down() -> PlayerInput {
        PlayerInput {
            menu_down: true,
            ..PlayerInput::default()
        }
    }

    fn confirm_input() -> PlayerInput {
        PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        }
    }

    /// Starts the world from the open world setup dialog (moves to "Start" and confirms).
    pub(super) fn start_world_setup(game: &mut LocalGame) -> FrameReport {
        assert_eq!(game.shell.modal, Some(ModalScreen::WorldSetup));
        for _ in 0..crate::shell::world_setup_start_row() {
            let _report = press(game, down());
        }
        press(game, confirm_input())
    }

    /// Navigating the options rows with Down and confirming acts on the highlighted row.
    fn assert_options_rows_follow_navigation(game: &mut LocalGame) {
        assert_eq!(game.shell.modal, Some(ModalScreen::Options));
        assert_eq!(game.shell.selected_menu_item, 0, "opens on the volume row");
        let _ = press(game, down());
        assert_eq!(
            game.shell.selected_menu_item, 1,
            "Down highlights Fullscreen"
        );
        // Screen effects, gyro stabiliser and horizon hold (F9), auto-land and space thrust
        // (Wave 11), then Back.
        for _ in 0..6 {
            let _ = press(game, down());
        }
        assert_eq!(game.shell.selected_menu_item, 7, "Down highlights Back");
        let fullscreen = game.shell.fullscreen;
        let _ = press(game, confirm_input());
        assert_eq!(game.shell.modal, None, "confirming Back closes Options");
        assert_eq!(
            game.shell.fullscreen, fullscreen,
            "and leaves fullscreen alone"
        );
    }

    #[test]
    fn options_from_the_title_confirm_the_highlighted_row() {
        let mut game = title_game("options-title");
        idle(&mut game, 2);
        let options = TitleOption::all(save::saves_exist());
        let target = options
            .iter()
            .position(|option| *option == TitleOption::Options)
            .expect("title lists Options");
        for _ in 0..target {
            let _ = press(&mut game, down());
        }
        let _ = press(&mut game, confirm_input());
        assert_options_rows_follow_navigation(&mut game);
    }

    #[test]
    fn options_from_the_pause_menu_confirm_the_highlighted_row() {
        let mut game = playing(WorldState::new_game(PLAYER), 1, "options-pause");
        idle(&mut game, 2);
        let _ = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        let options = PauseOption::for_role(None);
        let target = options
            .iter()
            .position(|option| *option == PauseOption::Options)
            .expect("pause lists Options");
        for _ in 0..target {
            let _ = press(&mut game, down());
        }
        let _ = press(&mut game, confirm_input());
        assert_options_rows_follow_navigation(&mut game);
    }

    #[test]
    fn frame_devices_reach_the_shell_and_empty_keeps_them() {
        let mut game = title_game("devices");
        let pad = InputDevice::Gamepad {
            index: 1,
            family: crate::input::devices::PadFamily::Xbox,
        };
        let _report = game.frame(
            &FrameInput {
                devices: vec![pad],
                gamepads: BTreeMap::from([(1, "Xbox Wireless Controller".to_owned())]),
                ..FrameInput::default()
            },
            DT,
        );
        assert_eq!(game.shell.prompt_device(0), pad);
        idle(&mut game, 1);
        assert_eq!(
            game.shell.prompt_device(0),
            pad,
            "scripted frames keep the device"
        );
    }

    #[test]
    fn controllers_add_seats_only_in_split_screen_play() {
        let mut solo = playing(WorldState::new_game(PLAYER), 1, "claim-solo");
        assert_eq!(solo.add_seat_for_controller(), None);
        let mut split = playing(WorldState::new_game(PLAYER), 2, "claim-split");
        assert_eq!(split.add_seat_for_controller(), Some(2));
        assert_eq!(split.seats().len(), 3);
        split.shell.return_to_title("");
        assert_eq!(split.add_seat_for_controller(), None, "not from the title");
    }

    fn player(game: &LocalGame, id: PlayerId) -> fatherload_simulation::Player {
        game.world().player(id).expect("player exists").clone()
    }

    fn rig_at_building(
        zone: SurfaceZone,
        configure: impl FnOnce(&mut fatherload_simulation::Player),
    ) -> WorldState {
        let building = SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == zone)
            .expect("building exists");
        let mut world = new_world();
        let rig = world.player_mut(PLAYER).expect("rig");
        #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
        let x = (crate::view_frame::frame().rel_col(building.tile_x) + 2.5) * TILE_SIZE;
        rig.x = x;
        configure(rig);
        world
    }

    /// Enters the building the rig stands at and walks to its service counter.
    fn walk_to_counter(game: &mut LocalGame, zone: SurfaceZone) {
        let _report = press(
            game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        assert_eq!(game.shell.seat(0).interior_zone, Some(zone));
        let target = interior_service_x(zone);
        for _ in 0..600 {
            let delta = target - game.shell.seat(0).interior_x;
            if delta.abs() < 10.0 {
                break;
            }
            let _report = step(
                game,
                PlayerInput {
                    horizontal: delta.signum(),
                    ..PlayerInput::default()
                },
            );
        }
        let _report = press(
            game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
    }

    fn confirm(game: &mut LocalGame) {
        let _report = press(
            game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        // Let the submitted transaction tick through the host.
        idle(game, 3);
    }

    #[test]
    fn new_game_from_title_runs_a_single_seat_host() {
        let mut game = title_game("new-game");
        assert_eq!(game.shell.run_mode, RunMode::Title);
        let idle_report = step(&mut game, PlayerInput::default());
        assert_eq!(idle_report.ticks_run, 0, "the world idles behind the title");

        let report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        assert!(
            !report.world_replaced,
            "New Game opens the world setup first"
        );
        let seed = game
            .shell
            .world_setup
            .as_ref()
            .expect("world setup open")
            .settings
            .seed;
        let report = start_world_setup(&mut game);
        assert!(report.world_replaced);
        assert_eq!(game.world().world_settings().seed, seed);
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        assert!(matches!(game.runtime(), Runtime::Host(_)));
        assert_eq!(game.seats().len(), 1);
        assert_eq!(game.seats()[0].player, PLAYER);
        assert_eq!(game.runtime().epoch(), 1);
    }

    #[test]
    fn held_movement_moves_the_rig_and_the_camera_follows() {
        let mut game = playing(new_world(), 1, "move");
        idle(&mut game, 30);
        let start = player(&game, PLAYER);
        let mut ticks = 0;
        for _ in 0..60 {
            ticks += step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            )
            .ticks_run;
        }
        let moved = player(&game, PLAYER);
        assert!(moved.x > start.x + TILE_SIZE, "{} -> {}", start.x, moved.x);
        assert!(
            (55..=65).contains(&ticks),
            "fixed ticks track real time: {ticks}"
        );

        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        let view = snapshot.primary_view().expect("one view");
        assert_eq!(snapshot.views.len(), 1);
        assert!((view.camera.x - (moved.x - 640.0)).abs() < f32::EPSILON);
        assert!((view.camera.y - (moved.y - 360.0)).abs() < f32::EPSILON);
        assert!((view.hud.fuel - moved.fuel).abs() < f32::EPSILON);
    }

    #[test]
    fn drilling_down_digs_terrain_and_reports_changed_tiles() {
        let mut world = new_world();
        // Centre the rig over a column so it fits down the shaft it digs.
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = (rig.x / TILE_SIZE)
            .floor()
            .mul_add(TILE_SIZE, TILE_SIZE / 2.0);
        let mut game = playing(world, 1, "drill");
        idle(&mut game, 30);
        let start = player(&game, PLAYER);
        let mut changed = Vec::new();
        for _ in 0..600 {
            let report = step(
                &mut game,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
            changed.extend(report.changed_tiles);
        }
        let end = player(&game, PLAYER);
        assert!(!changed.is_empty(), "drilling changed terrain");
        assert!(
            end.y > start.y + TILE_SIZE,
            "rig dug down: {} -> {}",
            start.y,
            end.y
        );
        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        let view = snapshot.primary_view().expect("view");
        assert!(
            view.is_explored(end.tile_in(crate::view_frame::frame())),
            "ticks reveal tiles"
        );
    }

    #[test]
    fn selling_cargo_at_the_depot_goes_through_the_runtime() {
        let world = rig_at_building(SurfaceZone::Depot, |rig| {
            rig.cargo_bay.insert(MineralKind::Gold, 3);
            rig.credits = 0;
        });
        let mut game = playing(world, 1, "sell");
        idle(&mut game, 5);
        walk_to_counter(&mut game, SurfaceZone::Depot);
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Depot));
        assert_eq!(
            game.shell.seat(0).selected_menu_item,
            1,
            "depot opens on 'sell'"
        );
        let tick_before = game.world().simulation_tick();
        confirm(&mut game);

        let rig = player(&game, PLAYER);
        assert!(rig.cargo_bay.is_empty(), "cargo sold");
        assert!(rig.credits > 0, "credits earned");
        assert!(
            game.shell.seat(0).message.starts_with("Sold cargo for"),
            "{}",
            game.shell.seat(0).message
        );
        assert!(
            game.world().simulation_tick() > tick_before,
            "the sale resolved inside a fixed tick"
        );
        assert_eq!(game.shell.seat(0).depot_receipts.len(), 1);
        assert!(
            !game.shell.world_should_tick(),
            "the interior freezes the world again"
        );
    }

    #[test]
    fn buying_an_upgrade_at_the_shop_goes_through_the_runtime() {
        let world = rig_at_building(SurfaceZone::Shop, |rig| rig.credits = 10_000);
        let mut game = playing(world, 1, "buy");
        idle(&mut game, 5);
        let before = player(&game, PLAYER);
        walk_to_counter(&mut game, SurfaceZone::Shop);
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Shop));
        confirm(&mut game);
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ShopConfirm));
        confirm(&mut game);

        let after = player(&game, PLAYER);
        assert_eq!(after.drill_strength, before.drill_strength + 1);
        assert!(after.credits < before.credits);
        assert!(
            game.shell.seat(0).message.starts_with("Bought"),
            "{}",
            game.shell.seat(0).message
        );

        // Leaving the building resumes play.
        let _report = press(
            &mut game,
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
        );
        let _report = press(
            &mut game,
            PlayerInput {
                cancel: true,
                ..PlayerInput::default()
            },
        );
        assert!(!game.shell.seat(0).inside());
        assert!(game.shell.world_should_tick());
    }

    #[test]
    fn save_and_load_round_trip_restores_the_world_in_a_new_epoch() {
        let path = temp_save("round-trip");
        let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        let _report = start_world_setup(&mut game);
        for _ in 0..40 {
            let _report = step(
                &mut game,
                PlayerInput {
                    horizontal: -1.0,
                    ..PlayerInput::default()
                },
            );
        }
        idle(&mut game, 30);
        let _report = press(
            &mut game,
            PlayerInput {
                save: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.message, "Game saved.");
        assert!(path.exists());
        let saved = save::load_world_from_path(&path).expect("save readable");
        let saved_hash = saved.world.simulation_state_hash();
        let epoch_before = game.runtime().epoch();

        for _ in 0..60 {
            let _report = step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
        }
        assert_ne!(game.world().simulation_state_hash(), saved_hash);

        let mut load = PlayerInput {
            load: true,
            ..PlayerInput::default()
        };
        // Pause first so no tick runs between the load and the comparison.
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        load.pause = false;
        let report = step(&mut game, load);
        let _ignored = std::fs::remove_file(&path);
        assert!(report.world_replaced);
        assert_eq!(
            game.runtime().epoch(),
            epoch_before + 1,
            "loading starts a new epoch"
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        // The load resumed play, so at most this frame's ticks ran on top of the save.
        assert!(report.ticks_run <= 1);
        if report.ticks_run == 0 {
            assert_eq!(game.world().simulation_state_hash(), saved_hash);
        }
        assert_eq!(
            player(&game, PLAYER).credits,
            saved.world.player(PLAYER).expect("saved rig").credits
        );
        assert_eq!(game.seats().len(), 1);
    }

    #[test]
    fn local_split_screen_runs_two_independent_seats() {
        let mut game = title_game("split-two");
        let _report = press(
            &mut game,
            PlayerInput {
                menu_down: true,
                ..PlayerInput::default()
            },
        );
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        let _report = start_world_setup(&mut game);
        assert_eq!(game.seats().len(), 2);
        let first = game.seats()[0].player;
        let second = game.seats()[1].player;
        assert_ne!(first, second);
        let world_players: Vec<PlayerId> = game.world().player_ids().collect();
        assert_eq!(world_players, vec![first, second], "no orphan rigs");
        idle(&mut game, 20);
        let a0 = player(&game, first);
        let b0 = player(&game, second);
        for _ in 0..45 {
            let _report = game.frame(
                &FrameInput {
                    primary: PlayerInput::default(),
                    other_seats: vec![PlayerInput {
                        horizontal: -1.0,
                        ..PlayerInput::default()
                    }],
                    ..FrameInput::default()
                },
                DT,
            );
        }
        let a1 = player(&game, first);
        let b1 = player(&game, second);
        assert!((a1.x - a0.x).abs() < 1.0, "seat 1 stayed put");
        assert!(b1.x < b0.x - TILE_SIZE, "seat 2 moved left");

        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        assert_eq!(snapshot.views.len(), 2);
        for view in &snapshot.views {
            let rig = player(&game, view.player_id);
            #[allow(clippy::cast_precision_loss, reason = "viewport size")]
            let expected = rig.x - view.viewport.width as f32 / 2.0;
            assert!((view.camera.x - expected).abs() < f32::EPSILON);
        }
        assert_ne!(snapshot.views[0].viewport, snapshot.views[1].viewport);
    }

    #[test]
    fn split_screen_grows_to_four_seats_and_stops_at_the_cap() {
        let mut game = playing(new_world(), 2, "split-four");
        for expected in 3..=MAX_LOCAL_SEATS {
            let _report = press(
                &mut game,
                PlayerInput {
                    local_multiplayer_toggle: true,
                    ..PlayerInput::default()
                },
            );
            assert_eq!(game.seats().len(), expected);
        }
        let _report = press(
            &mut game,
            PlayerInput {
                local_multiplayer_toggle: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.seats().len(), MAX_LOCAL_SEATS);
        assert!(
            game.shell.message.contains("limited"),
            "{}",
            game.shell.message
        );

        idle(&mut game, 20);
        let fourth = game.seats()[3].player;
        let before = player(&game, fourth);
        for _ in 0..45 {
            let _report = game.frame(
                &FrameInput {
                    primary: PlayerInput::default(),
                    other_seats: vec![
                        PlayerInput::default(),
                        PlayerInput::default(),
                        PlayerInput {
                            horizontal: 1.0,
                            ..PlayerInput::default()
                        },
                    ],
                    ..FrameInput::default()
                },
                DT,
            );
        }
        assert!(player(&game, fourth).x > before.x + TILE_SIZE);
        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        assert_eq!(snapshot.views.len(), 4);
        assert_eq!(snapshot.players.len(), 4);
        let ids: std::collections::BTreeSet<PlayerId> =
            snapshot.views.iter().map(|view| view.player_id).collect();
        assert_eq!(ids.len(), 4, "every view follows its own rig");
    }

    #[test]
    fn pause_stops_the_host_and_resume_continues() {
        let mut game = playing(new_world(), 1, "pause");
        idle(&mut game, 10);
        let _report = step(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        let tick = game.world().simulation_tick();
        for _ in 0..30 {
            let report = step(
                &mut game,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
            assert_eq!(report.ticks_run, 0);
        }
        assert_eq!(
            game.world().simulation_tick(),
            tick,
            "paused host does not tick"
        );
        let _report = step(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        idle(&mut game, 10);
        assert!(game.world().simulation_tick() > tick);
    }

    #[test]
    fn a_stranded_rig_calls_rescue_through_the_runtime() {
        let mut world = new_world();
        world.player_mut(PLAYER).expect("rig").fuel = 0.0;
        let mut game = playing(world, 1, "rescue");
        idle(&mut game, 2);
        assert!(game.world().failure_state(PLAYER).is_some());
        let _report = press(
            &mut game,
            PlayerInput {
                interact: true,
                ..PlayerInput::default()
            },
        );
        idle(&mut game, 3);
        assert!(game.world().failure_state(PLAYER).is_none(), "rescued");
        assert_eq!(game.world().rescue_count(PLAYER), 1);
    }

    #[test]
    fn other_seats_call_rescue_only_while_gameplay_runs() {
        let second = PlayerId::new(2);
        let mut world = new_world();
        world.add_player(second);
        world.player_mut(second).expect("seat 2 rig").fuel = 0.0;
        let mut game = playing(world, 2, "seat-rescue");
        assert_eq!(game.seats()[1].player, second);
        idle(&mut game, 2);
        assert!(game.world().failure_state(second).is_some());
        let seat_two_interact = |game: &mut LocalGame| {
            let _report = game.frame(
                &FrameInput {
                    primary: PlayerInput::default(),
                    other_seats: vec![PlayerInput {
                        interact: true,
                        ..PlayerInput::default()
                    }],
                    ..FrameInput::default()
                },
                DT,
            );
        };

        // Pressed while the primary player has the game paused: nothing is queued for later.
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        seat_two_interact(&mut game);
        let _report = press(
            &mut game,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        idle(&mut game, 5);
        assert_eq!(game.world().rescue_count(second), 0, "no deferred rescue");

        seat_two_interact(&mut game);
        idle(&mut game, 3);
        assert_eq!(game.world().rescue_count(second), 1);
        assert!(game.world().failure_state(second).is_none(), "rescued");
    }

    #[test]
    fn title_menu_offers_host_and_join() {
        let options = TitleOption::all(false);
        assert!(options.contains(&TitleOption::HostOnline));
        assert!(options.contains(&TitleOption::JoinOnline));
        let mut game = title_game("join-menu");
        let join_index = options
            .iter()
            .position(|option| *option == TitleOption::JoinOnline)
            .expect("join entry");
        for _ in 0..join_index {
            let _report = press(
                &mut game,
                PlayerInput {
                    menu_down: true,
                    ..PlayerInput::default()
                },
            );
        }
        let _report = press(
            &mut game,
            PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Lobby);
        assert_eq!(game.shell.online.screen, OnlineScreen::JoinMenu);
    }

    /// A fresh world with one rig per entry of `spots`: rig `n` (player `n + 1`) parked at that
    /// building's door, or in open ground east of town for `None`.
    fn split_world(spots: &[Option<SurfaceZone>]) -> WorldState {
        let mut world = new_world();
        for (index, spot) in spots.iter().enumerate() {
            let player = PlayerId::new(u64::try_from(index + 1).expect("few seats"));
            if world.player(player).is_none() {
                world.add_player(player);
            }
            let tile_x = spot.map_or(196, |zone| {
                SURFACE_BUILDINGS
                    .iter()
                    .find(|building| building.zone == zone)
                    .expect("building exists")
                    .tile_x
                    + 2
            });
            let rig = world.player_mut(player).expect("rig");
            #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
            let x = (crate::view_frame::frame().rel_col(tile_x) + 0.5) * TILE_SIZE;
            rig.x = x;
        }
        world
    }

    /// One frame with `inputs[n]` held by seat `n` (missing seats idle).
    fn step_seats(game: &mut LocalGame, inputs: &[PlayerInput]) -> FrameReport {
        game.frame(
            &FrameInput {
                primary: inputs.first().copied().unwrap_or_default(),
                other_seats: inputs.iter().skip(1).copied().collect(),
                ..FrameInput::default()
            },
            DT,
        )
    }

    /// Seat `seat` presses `input` for one frame (everyone else idle), then one idle frame.
    fn press_seat(game: &mut LocalGame, seat: usize, input: PlayerInput) {
        let mut inputs = vec![PlayerInput::default(); game.seats().len()];
        inputs[seat] = input;
        let _report = step_seats(game, &inputs);
        idle(game, 1);
    }

    fn interact() -> PlayerInput {
        PlayerInput {
            interact: true,
            ..PlayerInput::default()
        }
    }

    fn back_input() -> PlayerInput {
        PlayerInput {
            back: true,
            ..PlayerInput::default()
        }
    }

    /// Walks every seat in `seats` (0-based, already inside) to its counter at once, then opens
    /// it with the use button, all seats in the same frames.
    fn walk_seats_to_counters(game: &mut LocalGame, seats: &[usize]) {
        for _ in 0..600 {
            let mut inputs = vec![PlayerInput::default(); game.seats().len()];
            let mut walking = false;
            for &seat in seats {
                let state = game.shell.seat(seat);
                let zone = state.interior_zone.expect("seat is inside");
                let delta = interior_service_x(zone) - state.interior_x;
                if delta.abs() >= 10.0 {
                    inputs[seat].horizontal = delta.signum();
                    walking = true;
                }
            }
            if !walking {
                break;
            }
            let _report = step_seats(game, &inputs);
        }
        let mut inputs = vec![PlayerInput::default(); game.seats().len()];
        for &seat in seats {
            inputs[seat] = interact();
        }
        let _report = step_seats(game, &inputs);
        idle(game, 1);
    }

    #[test]
    fn two_seats_enter_use_and_leave_buildings_independently() {
        let mut world = split_world(&[Some(SurfaceZone::Depot), Some(SurfaceZone::Shop)]);
        let second = PlayerId::new(2);
        world
            .player_mut(PLAYER)
            .expect("rig")
            .cargo_bay
            .insert(MineralKind::Gold, 3);
        world.player_mut(PLAYER).expect("rig").credits = 0;
        world.player_mut(second).expect("rig").credits = 10_000;
        let mut game = playing(world, 2, "split-counters");
        assert_eq!(game.seats()[1].player, second);
        idle(&mut game, 5);

        // Both walk in on the same frame, each into the building at their own rig.
        let _report = step_seats(&mut game, &[interact(), interact()]);
        idle(&mut game, 1);
        assert_eq!(game.shell.seat(0).interior_zone, Some(SurfaceZone::Depot));
        assert_eq!(game.shell.seat(1).interior_zone, Some(SurfaceZone::Shop));
        assert_eq!(game.shell.run_mode, RunMode::Playing);

        walk_seats_to_counters(&mut game, &[0, 1]);
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Depot));
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));

        // Selections are per seat.
        press_seat(&mut game, 1, down());
        press_seat(&mut game, 1, down());
        assert_eq!(game.shell.seat(1).selected_menu_item, 2);
        assert_eq!(
            game.shell.seat(0).selected_menu_item,
            1,
            "player 1's depot still opens on 'sell'"
        );

        // Player 1 sells their cargo: only they are paid and only their view reports it.
        let credits_two = player(&game, second).credits;
        press_seat(&mut game, 0, confirm_input());
        idle(&mut game, 3);
        assert!(player(&game, PLAYER).cargo_bay.is_empty());
        assert!(player(&game, PLAYER).credits > 0);
        assert_eq!(player(&game, second).credits, credits_two);
        assert!(
            game.shell.seat(0).message.starts_with("Sold cargo for"),
            "{}",
            game.shell.seat(0).message
        );
        assert!(!game.shell.seat(1).message.starts_with("Sold"));
        assert_eq!(game.shell.seat(0).depot_receipts.len(), 1);
        assert!(game.shell.seat(1).depot_receipts.is_empty());

        // Player 2 buys the cargo bay they selected, as their own player.
        let before = player(&game, second);
        press_seat(&mut game, 1, confirm_input());
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::ShopConfirm));
        press_seat(&mut game, 1, confirm_input());
        idle(&mut game, 3);
        let after = player(&game, second);
        assert_eq!(after.cargo_bay_level, before.cargo_bay_level + 1);
        assert!(after.credits < before.credits);
        assert_eq!(
            player(&game, PLAYER).cargo_bay_level,
            before.cargo_bay_level,
            "player 1's rig is untouched"
        );
        assert!(
            game.shell.seat(1).message.starts_with("Bought"),
            "{}",
            game.shell.seat(1).message
        );
        assert!(
            !game.shell.world_should_tick(),
            "both seats inside: the world rests"
        );

        // Player 1 leaves; player 2 stays at the counter and the world runs again.
        press_seat(&mut game, 0, back_input());
        press_seat(&mut game, 0, back_input());
        assert!(!game.shell.seat(0).inside());
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));
        assert!(game.shell.world_should_tick());
        let tick = game.world().simulation_tick();
        idle(&mut game, 5);
        assert!(game.world().simulation_tick() > tick);
    }

    #[test]
    fn seat_two_shops_while_seat_one_mines_and_the_world_keeps_ticking() {
        let mut world = split_world(&[None, Some(SurfaceZone::Shop)]);
        // Centre player 1's rig over a column so it fits down the shaft it digs.
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = (rig.x / TILE_SIZE)
            .floor()
            .mul_add(TILE_SIZE, TILE_SIZE / 2.0);
        world.player_mut(PlayerId::new(2)).expect("rig").credits = 10_000;
        let mut game = playing(world, 2, "split-mine-shop");
        idle(&mut game, 20);
        let start = player(&game, PLAYER);
        let shopper_before = player(&game, PlayerId::new(2));

        press_seat(&mut game, 1, interact());
        walk_seats_to_counters(&mut game, &[1]);
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));

        let drill = PlayerInput {
            drill_down: true,
            ..PlayerInput::default()
        };
        let mut ticks = 0;
        let mut changed = 0;
        for frame in 0..400 {
            let shopper = match frame {
                10 | 20 => confirm_input(),
                _ => PlayerInput::default(),
            };
            let report = step_seats(&mut game, &[drill, shopper]);
            assert!(
                report.ticks_run > 0,
                "the world never pauses for the shopper"
            );
            ticks += report.ticks_run;
            changed += report.changed_tiles.len();
        }
        assert!(ticks >= 380, "{ticks}");
        assert!(changed > 0, "player 1 kept digging");
        assert!(player(&game, PLAYER).y > start.y + TILE_SIZE);
        assert_eq!(
            player(&game, PlayerId::new(2)).drill_strength,
            shopper_before.drill_strength + 1,
            "player 2 bought the drill"
        );
        assert!(
            game.shell.seat(1).inside(),
            "the shopper is still at the counter"
        );
    }

    #[test]
    fn alone_a_building_still_pauses_the_world() {
        let world = rig_at_building(SurfaceZone::Shop, |rig| rig.credits = 10_000);
        let mut game = playing(world, 1, "solo-pause");
        idle(&mut game, 5);
        press_seat(&mut game, 0, interact());
        assert!(game.shell.seat(0).inside());
        for _ in 0..20 {
            assert_eq!(step(&mut game, PlayerInput::default()).ticks_run, 0);
        }
        // A purchase runs just the ticks it needs, then the world rests again.
        walk_to_counter_inside(&mut game);
        confirm(&mut game);
        confirm(&mut game);
        assert!(game.shell.seat(0).message.starts_with("Bought"));
        assert_eq!(step(&mut game, PlayerInput::default()).ticks_run, 0);
        // An outdoor dialog does not pause, as before.
        press_seat(&mut game, 0, back_input());
        press_seat(&mut game, 0, back_input());
        press_seat(
            &mut game,
            0,
            PlayerInput {
                inventory: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Inventory));
        idle(&mut game, 2);
        assert!(step(&mut game, PlayerInput::default()).ticks_run > 0);
    }

    /// Walks the only seat (already inside) to its counter and opens it.
    fn walk_to_counter_inside(game: &mut LocalGame) {
        walk_seats_to_counters(game, &[0]);
        assert!(game.shell.seat(0).modal.is_some());
    }

    #[test]
    fn any_seat_opens_the_shared_pause_menu_and_drives_it() {
        let mut game = playing(split_world(&[None, None]), 2, "split-pause");
        idle(&mut game, 5);
        press_seat(
            &mut game,
            1,
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        assert_eq!(game.shell.menu_seat, 1);
        for _ in 0..10 {
            assert_eq!(
                step_seats(
                    &mut game,
                    &[
                        PlayerInput {
                            horizontal: 1.0,
                            ..PlayerInput::default()
                        },
                        PlayerInput::default()
                    ]
                )
                .ticks_run,
                0,
                "the pause menu pauses everyone"
            );
        }
        press_seat(&mut game, 0, down());
        assert_eq!(
            game.shell.selected_pause_item, 0,
            "player 1 does not drive it"
        );
        press_seat(&mut game, 1, down());
        assert_eq!(game.shell.selected_pause_item, 1, "player 2 does");
        press_seat(&mut game, 0, back_input());
        assert_eq!(game.shell.run_mode, RunMode::Paused);
        press_seat(&mut game, 1, back_input());
        assert_eq!(game.shell.run_mode, RunMode::Playing);
        assert!(step(&mut game, PlayerInput::default()).ticks_run > 0);
    }

    #[test]
    fn each_seats_input_reaches_only_its_own_menus_and_rig() {
        let mut game = playing(split_world(&[None, None]), 2, "split-isolation");
        let second = PlayerId::new(2);
        idle(&mut game, 20);
        press_seat(
            &mut game,
            1,
            PlayerInput {
                inventory: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Inventory));
        assert_eq!(game.shell.seat(0).modal, None);
        assert!(game.shell.gameplay_input_enabled(0));
        assert!(!game.shell.gameplay_input_enabled(1));

        let (a0, b0) = (player(&game, PLAYER).x, player(&game, second).x);
        let right = PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        };
        for _ in 0..45 {
            let _report = step_seats(&mut game, &[right, right]);
        }
        assert!(player(&game, PLAYER).x > a0 + TILE_SIZE, "player 1 drives");
        assert!(
            (player(&game, second).x - b0).abs() < 1.0,
            "player 2's dialog holds their rig"
        );
        // Player 1's Back closes nothing of player 2's.
        press_seat(&mut game, 0, back_input());
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Inventory));
        press_seat(&mut game, 1, back_input());
        assert_eq!(game.shell.seat(1).modal, None);
    }

    #[test]
    fn four_seats_each_use_a_different_building() {
        let zones = [
            SurfaceZone::Fuel,
            SurfaceZone::Repair,
            SurfaceZone::Depot,
            SurfaceZone::Shop,
        ];
        let mut world = split_world(&zones.map(Some));
        for index in 1..=4 {
            let rig = world.player_mut(PlayerId::new(index)).expect("rig");
            rig.fuel = rig.fuel_capacity * 0.5;
            rig.credits = 5_000;
        }
        let mut game = playing(world, 4, "split-four-buildings");
        idle(&mut game, 5);
        let _report = step_seats(&mut game, &[interact(); 4]);
        idle(&mut game, 1);
        for (seat, zone) in zones.iter().enumerate() {
            assert_eq!(
                game.shell.seat(seat).interior_zone,
                Some(*zone),
                "seat {seat}"
            );
        }
        assert!(!game.shell.world_should_tick(), "everyone is inside");
        walk_seats_to_counters(&mut game, &[0, 1, 2, 3]);
        assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Fuel));
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Repair));
        assert_eq!(game.shell.seat(2).modal, Some(ModalScreen::Depot));
        assert_eq!(game.shell.seat(3).modal, Some(ModalScreen::Shop));

        // Player 1 fills up (counter, then confirm): only their rig is refuelled.
        press_seat(&mut game, 0, confirm_input());
        press_seat(&mut game, 0, confirm_input());
        idle(&mut game, 3);
        let full = |game: &LocalGame, index| {
            let rig = player(game, PlayerId::new(index));
            rig.fuel >= rig.fuel_capacity - 0.5
        };
        assert!(full(&game, 1));
        assert!(!full(&game, 4), "player 4's rig is still half empty");

        press_seat(&mut game, 3, back_input());
        press_seat(&mut game, 3, back_input());
        assert!(!game.shell.seat(3).inside());
        assert!(game.shell.world_should_tick(), "player 4 is outside again");
    }

    #[test]
    fn loading_puts_every_seat_back_outdoors() {
        let path = temp_save("split-load");
        let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
        game.runtime = Runtime::local(split_world(&[None, Some(SurfaceZone::Shop)]), 2);
        game.shell.enter_world("test", 0.0);
        idle(&mut game, 5);
        let _report = press(
            &mut game,
            PlayerInput {
                save: true,
                ..PlayerInput::default()
            },
        );
        press_seat(&mut game, 1, interact());
        walk_seats_to_counters(&mut game, &[1]);
        press_seat(
            &mut game,
            0,
            PlayerInput {
                map: true,
                ..PlayerInput::default()
            },
        );
        assert!(game.shell.seat(1).inside() && game.shell.seat(0).modal.is_some());

        let report = step(
            &mut game,
            PlayerInput {
                load: true,
                ..PlayerInput::default()
            },
        );
        let _ignored = std::fs::remove_file(&path);
        assert!(report.world_replaced);
        assert_eq!(game.seats().len(), 2);
        for seat in 0..2 {
            let state = game.shell.seat(seat);
            assert!(!state.inside() && state.modal.is_none(), "seat {seat}");
        }
        assert_eq!(game.shell.run_mode, RunMode::Playing);
    }

    #[test]
    fn an_online_host_with_local_seats_never_pauses_for_its_interiors() {
        let mut world = split_world(&[Some(SurfaceZone::Depot), Some(SurfaceZone::Shop)]);
        let second = PlayerId::new(2);
        world.player_mut(second).expect("rig").credits = 10_000;
        let mut game = playing(world, 2, "online-split");
        game.set_host_bind("127.0.0.1:0".parse().expect("addr"));
        game.handle_request(
            ShellRequest::OpenOnline,
            &mut Vec::new(),
            &mut FrameReport::default(),
        );
        assert_eq!(game.runtime.online_role(), Some(OnlineRole::Host));
        assert_eq!(
            game.seats().len(),
            2,
            "opening online keeps the local seats"
        );
        idle(&mut game, 5);

        // Both local players walk in: offline that would rest the world, online it never does.
        let _report = step_seats(&mut game, &[interact(), interact()]);
        idle(&mut game, 1);
        assert!(game.shell.seat(0).inside() && game.shell.seat(1).inside());
        assert!(!game.shell.world_should_tick());
        for _ in 0..10 {
            assert!(step(&mut game, PlayerInput::default()).ticks_run > 0);
        }

        // Player 2 still buys as themselves.
        walk_seats_to_counters(&mut game, &[1]);
        assert_eq!(game.shell.seat(1).modal, Some(ModalScreen::Shop));
        let before = player(&game, second);
        press_seat(&mut game, 1, confirm_input());
        press_seat(&mut game, 1, confirm_input());
        idle(&mut game, 3);
        assert_eq!(
            player(&game, second).drill_strength,
            before.drill_strength + 1
        );
        assert!(game.shell.seat(1).message.starts_with("Bought"));
        end_runtime(std::mem::replace(
            &mut game.runtime,
            Runtime::local(new_world(), 1),
        ));
    }

    /// Two whole apps (shell + runtime + net bridge) driven frame by frame in real time.
    struct Pair {
        host: LocalGame,
        client: LocalGame,
        last: std::time::Instant,
    }
    impl Pair {
        fn frame(&mut self, host_input: PlayerInput, client_input: PlayerInput) {
            let now = std::time::Instant::now();
            let dt = (now - self.last).as_secs_f32();
            self.last = now;
            let _host = self.host.frame(&FrameInput::primary(host_input), dt);
            let _client = self.client.frame(&FrameInput::primary(client_input), dt);
            std::thread::sleep(Duration::from_millis(4));
        }
        fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
            self.holding_until(what, PlayerInput::default(), done);
        }
        /// Frames with the client holding `client_input` until `done`. The runtimes tick
        /// from real time, so holding for a fixed frame count would cover a
        /// machine-dependent number of ticks.
        fn holding_until(
            &mut self,
            what: &str,
            client_input: PlayerInput,
            done: impl Fn(&Self) -> bool,
        ) {
            let deadline = std::time::Instant::now() + Duration::from_secs(8);
            while !done(self) {
                assert!(std::time::Instant::now() < deadline, "timed out: {what}");
                self.frame(PlayerInput::default(), client_input);
            }
        }
    }

    /// Hosts from the title, joins a second app by typing the address, readies up and starts:
    /// both apps end up playing. Returns the pair and the client's player.
    fn host_and_join_over_real_quic() -> (Pair, PlayerId) {
        let confirm = PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        };

        let mut host = title_game("online-host");
        host.set_host_bind("127.0.0.1:0".parse().expect("addr"));
        let mut requests = Vec::new();
        host.handle_request(
            ShellRequest::HostOnline {
                settings: WorldSettings::default(),
                test_mode: false,
                enemies: EnemyDifficulty::Normal,
                day_length: DayLength::Standard,
            },
            &mut requests,
            &mut FrameReport::default(),
        );
        assert_eq!(host.shell.run_mode, RunMode::Lobby);
        let mut pair = Pair {
            host,
            client: title_game("online-client"),
            last: std::time::Instant::now(),
        };
        pair.until("the host endpoint", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.addr.is_some())
        });
        let addr = pair
            .host
            .shell
            .online
            .lobby
            .as_ref()
            .and_then(|lobby| lobby.addr);
        let port = addr.expect("bound").port();

        // Join by typing the address into the form.
        pair.client.shell.show_online(OnlineScreen::JoinAddress);
        pair.client.shell.online.text.clear();
        for character in format!("127.0.0.1:{port}").chars() {
            pair.frame(
                PlayerInput::default(),
                PlayerInput {
                    text_input: Some(character),
                    ..PlayerInput::default()
                },
            );
        }
        pair.frame(PlayerInput::default(), confirm);
        pair.until("the client in the lobby", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.roster.len() == 2)
                && pair.client.runtime().client_state() == Some(ClientState::Lobby)
        });
        assert_eq!(pair.client.shell.run_mode, RunMode::Lobby);
        assert_eq!(pair.client.shell.online.role, Some(OnlineRole::Client));

        // Ready up (first lobby entry), then the host starts (first lobby entry).
        pair.frame(PlayerInput::default(), confirm);
        pair.until("the ready flag on the host", |pair| {
            pair.host
                .shell
                .online
                .lobby
                .as_ref()
                .is_some_and(|lobby| lobby.roster.iter().any(|entry| !entry.local && entry.ready))
        });
        pair.frame(confirm, PlayerInput::default());
        assert_eq!(pair.host.shell.run_mode, RunMode::Playing);
        pair.until("the client in game", |pair| {
            pair.client.shell.run_mode == RunMode::Playing
        });
        let client_player = pair.client.seats()[0].player;
        assert!(pair.host.world().player(client_player).is_some());
        (pair, client_player)
    }

    /// Two whole apps (shell + runtime + net bridge) on real QUIC: host, join by address, ready,
    /// start, play, local-only pause, client save refusal, and the host ending the session.
    #[test]
    fn two_apps_host_join_play_and_end_over_real_quic() {
        let (mut pair, client_player) = host_and_join_over_real_quic();

        // The client's rig moves on the host.
        let start_x = pair.host.world().player(client_player).expect("rig").x;
        let right = PlayerInput {
            horizontal: 1.0,
            ..PlayerInput::default()
        };
        pair.holding_until("the client's movement on the host", right, |pair| {
            pair.host.world().player(client_player).expect("rig").x > start_x + TILE_SIZE
        });

        // Pausing online is a local overlay: the world keeps running.
        pair.frame(
            PlayerInput::default(),
            PlayerInput {
                pause: true,
                ..PlayerInput::default()
            },
        );
        assert_eq!(pair.client.shell.run_mode, RunMode::Paused);
        let tick = pair.host.world().simulation_tick();
        pair.until("host ticks during the client's pause", |pair| {
            pair.host.world().simulation_tick().get() > tick.get() + 10
        });
        assert!(
            !PauseOption::for_role(Some(OnlineRole::Client)).contains(&PauseOption::Save),
            "joined clients have no save entry"
        );
        pair.frame(
            PlayerInput::default(),
            PlayerInput {
                save: true,
                ..PlayerInput::default()
            },
        );
        assert!(
            pair.client.shell.message.contains("only the host"),
            "{}",
            pair.client.shell.message
        );

        // The host ends the session: the client is told and returns to a local runtime.
        let mut requests = Vec::new();
        pair.host.handle_request(
            ShellRequest::LeaveOnline,
            &mut requests,
            &mut FrameReport::default(),
        );
        assert_eq!(pair.host.shell.run_mode, RunMode::Title);
        assert_eq!(pair.host.runtime().online_role(), None);
        pair.until("the client's notice", |pair| {
            pair.client.shell.run_mode == RunMode::Lobby
                && pair.client.shell.online.screen == OnlineScreen::Notice
        });
        assert!(
            pair.client.shell.online.notice.contains("host ended"),
            "{}",
            pair.client.shell.online.notice
        );
        assert_eq!(pair.client.runtime().online_role(), None);
    }
}
