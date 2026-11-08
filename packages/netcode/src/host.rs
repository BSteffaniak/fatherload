//! The authoritative host session (plan §6).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::ids::{
    ActionId, ClientId, InputSeq, PlayerId, ReconnectToken, SeatId, SessionEpoch, Tick,
};
use crate::ledger::ActionLedger;
use crate::link::DetRng;
use crate::params::{NetTuning, SessionParams};
use crate::protocol::{
    self, ClientMessage, CodecError, EndReason, InputFrame, LobbyEntry, NetEvent, PROTOCOL_VERSION,
    PlayerMarker, RejectReason, ServerFrame, ServerMessage, TaggedAction, Welcome,
};
use crate::simulation::{
    ActionEnvelope, NetSimulation, PlayerTickInput, Relevance, TickEvent, TickInputs,
};

/// Host configuration.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// Fixed session parameters (tick rate).
    pub params: SessionParams,
    /// Host-side tuning and the default tuning for new clients.
    pub tuning: NetTuning,
    /// Start in the lobby and wait for [`HostSession::start_game`].
    pub start_in_lobby: bool,
    /// How long a disconnected player stays reserved for its reconnect token.
    pub reconnect_grace: Duration,
    /// Report the world hash after every tick in [`HostOutput::tick_hashes`].
    pub record_tick_hashes: bool,
    /// Seed for reconnect token generation.
    ///
    /// Reconnect tokens are the only credential for reclaiming a reserved
    /// player, and this crate has no entropy source of its own. Production
    /// callers must fill this from OS randomness; the default is fixed so
    /// tests stay deterministic, which makes tokens guessable.
    pub token_seed: u64,
    /// Cap on late actions queued per client.
    pub max_late_actions: usize,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            params: SessionParams::default(),
            tuning: NetTuning::default(),
            start_in_lobby: false,
            reconnect_grace: Duration::from_secs(30),
            record_tick_hashes: false,
            token_seed: 0x5eed,
            max_late_actions: 256,
        }
    }
}

/// Host lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPhase {
    /// Waiting for `start_game`; the world does not tick.
    Lobby,
    /// Ticking.
    InGame,
    /// `end_session` was called.
    Ended,
}

/// What one [`HostSession::advance`] produced.
#[derive(Debug)]
pub struct HostOutput<S: NetSimulation> {
    /// Messages to send, in order, per client (reliable stream).
    pub outbound: Vec<(ClientId, ServerMessage<S>)>,
    /// All gameplay events from the ticks run (host presentation).
    pub events: Vec<NetEvent<S::Event>>,
    /// Fixed ticks run.
    pub ticks_run: u32,
    /// `(tick, world hash)` after each tick, if enabled.
    pub tick_hashes: Vec<(Tick, u64)>,
}

impl<S: NetSimulation> Default for HostOutput<S> {
    fn default() -> Self {
        Self {
            outbound: Vec::new(),
            events: Vec::new(),
            ticks_run: 0,
            tick_hashes: Vec::new(),
        }
    }
}

/// Per-client host counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostClientStats {
    /// Input datagrams accepted.
    pub input_datagrams: u64,
    /// Frames consumed on their target tick.
    pub frames_on_time: u64,
    /// Frames received after their target tick (controls dropped).
    pub frames_late: u64,
    /// Ticks that repeated held controls.
    pub held_ticks: u64,
    /// Ticks that used neutral controls.
    pub neutral_ticks: u64,
    /// Frames rejected for targeting too far ahead.
    pub frames_too_far_ahead: u64,
    /// Duplicate actions suppressed.
    pub duplicate_actions: u64,
    /// Late actions dropped because the queue was full.
    pub late_actions_dropped: u64,
    /// Server frames sent.
    pub frames_sent: u64,
    /// Most recent reported input margin.
    pub last_margin: Option<i64>,
    /// Largest number of buffered future input frames.
    pub max_buffered_inputs: usize,
}

/// Host-wide counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostStats {
    /// Ticks simulated.
    pub ticks: u64,
    /// Input datagrams rejected for a stale epoch.
    pub stale_epoch_inputs: u64,
    /// Messages from unknown or not-yet-joined clients that were ignored.
    pub ignored_messages: u64,
    /// `advance` calls that hit the catch-up clamp.
    pub catch_up_clamps: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Local(SeatId),
    Remote(ClientId),
    Reserved { expires: Tick },
}

#[derive(Debug)]
struct PlayerSlot<S: NetSimulation> {
    owner: Owner,
    name: String,
    token: ReconnectToken,
    ready: bool,
    last_controls: S::Controls,
    last_input_tick: Option<Tick>,
    ledger: ActionLedger,
    local_actions: Vec<S::Action>,
    next_local_action: ActionId,
}

impl<S: NetSimulation> PlayerSlot<S> {
    fn new(owner: Owner, name: String, token: ReconnectToken) -> Self {
        Self {
            owner,
            name,
            token,
            ready: false,
            last_controls: S::Controls::default(),
            last_input_tick: None,
            ledger: ActionLedger::new(),
            local_actions: Vec::new(),
            next_local_action: ActionId(1),
        }
    }
}

#[derive(Debug)]
struct RemoteClient<S: NetSimulation> {
    player: Option<PlayerId>,
    tuning: NetTuning,
    inputs: BTreeMap<Tick, InputFrame<S>>,
    timed_actions: BTreeMap<Tick, Vec<ActionEnvelope<S::Action>>>,
    late_actions: Vec<ActionEnvelope<S::Action>>,
    ack_seq: Option<InputSeq>,
    newest_seen: Option<InputSeq>,
    margin_min: Option<i64>,
    margin_seq: InputSeq,
    frame_acc: u64,
    far_acc: u64,
    far_due: bool,
    section_hashes: BTreeMap<S::SectionKey, u64>,
    roster_sent: Vec<PlayerId>,
    pending_ops: Vec<(Tick, S::TerrainOp)>,
    pending_events: Vec<NetEvent<S::Event>>,
    stats: HostClientStats,
}

impl<S: NetSimulation> RemoteClient<S> {
    const fn new(tuning: NetTuning) -> Self {
        Self {
            player: None,
            tuning,
            inputs: BTreeMap::new(),
            timed_actions: BTreeMap::new(),
            late_actions: Vec::new(),
            ack_seq: None,
            newest_seen: None,
            margin_min: None,
            margin_seq: InputSeq(0),
            frame_acc: 0,
            far_acc: 0,
            far_due: true,
            section_hashes: BTreeMap::new(),
            roster_sent: Vec::new(),
            pending_ops: Vec::new(),
            pending_events: Vec::new(),
            stats: HostClientStats {
                input_datagrams: 0,
                frames_on_time: 0,
                frames_late: 0,
                held_ticks: 0,
                neutral_ticks: 0,
                frames_too_far_ahead: 0,
                duplicate_actions: 0,
                late_actions_dropped: 0,
                frames_sent: 0,
                last_margin: None,
                max_buffered_inputs: 0,
            },
        }
    }

    /// Resets replication baselines to "the client has the current world".
    fn reset_baseline(&mut self, world: &S) {
        self.section_hashes = world
            .section_keys()
            .into_iter()
            .filter_map(|key| world.section_hash(&key).map(|hash| (key, hash)))
            .collect();
        self.roster_sent = world.player_ids();
        self.pending_ops.clear();
        self.frame_acc = 0;
        self.far_acc = 0;
        self.far_due = true;
    }

    fn clear_inputs(&mut self) {
        self.inputs.clear();
        self.timed_actions.clear();
        self.late_actions.clear();
        self.margin_min = None;
    }
}

/// The authoritative host.
///
/// Single-player and split-screen are this same session with zero remote
/// clients. The world only advances inside [`HostSession::advance`], at the
/// fixed tick rate; message handling never advances it.
#[derive(Debug)]
pub struct HostSession<S: NetSimulation> {
    config: HostConfig,
    world: S,
    phase: HostPhase,
    paused: bool,
    epoch: SessionEpoch,
    acc: u128,
    players: BTreeMap<PlayerId, PlayerSlot<S>>,
    seats: BTreeMap<SeatId, PlayerId>,
    clients: BTreeMap<ClientId, RemoteClient<S>>,
    next_player: u32,
    token_rng: DetRng,
    outbox: Vec<(ClientId, ServerMessage<S>)>,
    stats: HostStats,
}

impl<S: NetSimulation> HostSession<S> {
    /// Creates a host around an initial world.
    ///
    /// # Panics
    ///
    /// Panics if the world reports a tick rate different from `config.params` (its per-tick
    /// constants would not match the host clock).
    #[must_use]
    pub fn new(world: S, config: HostConfig) -> Self {
        assert_world_rate(&world, config.params);
        let next_player = world
            .player_ids()
            .iter()
            .map(|id| id.0.saturating_add(1))
            .max()
            .unwrap_or(0);
        let phase = if config.start_in_lobby {
            HostPhase::Lobby
        } else {
            HostPhase::InGame
        };
        let token_rng = DetRng::new(config.token_seed);
        Self {
            config,
            world,
            phase,
            paused: false,
            epoch: SessionEpoch(1),
            acc: 0,
            players: BTreeMap::new(),
            seats: BTreeMap::new(),
            clients: BTreeMap::new(),
            next_player,
            token_rng,
            outbox: Vec::new(),
            stats: HostStats::default(),
        }
    }

    // ----- accessors -------------------------------------------------------

    /// The authoritative world (host rendering reads this).
    #[must_use]
    pub const fn world(&self) -> &S {
        &self.world
    }

    /// Current host tick.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.world.tick()
    }

    /// Current epoch.
    #[must_use]
    pub const fn epoch(&self) -> SessionEpoch {
        self.epoch
    }

    /// Lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> HostPhase {
        self.phase
    }

    /// Session parameters.
    #[must_use]
    pub const fn params(&self) -> &SessionParams {
        &self.config.params
    }

    /// Host configuration.
    #[must_use]
    pub const fn config(&self) -> &HostConfig {
        &self.config
    }

    /// Host-wide counters.
    #[must_use]
    pub const fn stats(&self) -> &HostStats {
        &self.stats
    }

    /// Counters for one client.
    #[must_use]
    pub fn client_stats(&self, client: ClientId) -> Option<&HostClientStats> {
        self.clients.get(&client).map(|c| &c.stats)
    }

    /// The player a client controls, once joined.
    #[must_use]
    pub fn client_player(&self, client: ClientId) -> Option<PlayerId> {
        self.clients.get(&client).and_then(|c| c.player)
    }

    /// Connected clients.
    pub fn clients(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients.keys().copied()
    }

    /// The player bound to a local seat.
    #[must_use]
    pub fn seat_player(&self, seat: SeatId) -> Option<PlayerId> {
        self.seats.get(&seat).copied()
    }

    /// Lobby roster.
    #[must_use]
    pub fn roster(&self) -> Vec<LobbyEntry> {
        self.players
            .iter()
            .map(|(player, slot)| LobbyEntry {
                player: *player,
                name: slot.name.clone(),
                ready: slot.ready,
                connected: !matches!(slot.owner, Owner::Reserved { .. }),
                local: matches!(slot.owner, Owner::Local(_)),
            })
            .collect()
    }

    /// Buffered future input frames for a client.
    #[must_use]
    pub fn buffered_inputs(&self, client: ClientId) -> usize {
        self.clients.get(&client).map_or(0, |c| c.inputs.len())
    }

    /// Size of a client's action ledger backlog.
    #[must_use]
    pub fn action_backlog(&self, client: ClientId) -> usize {
        self.client_player(client)
            .and_then(|player| self.players.get(&player))
            .map_or(0, |slot| slot.ledger.backlog())
    }

    /// The relevance set used for a client's frames (plan §15).
    #[must_use]
    pub fn relevance_for(&self, client: ClientId) -> Option<Relevance> {
        self.clients
            .get(&client)
            .map(|c| relevance_for(&self.world, c))
    }

    // ----- local seats -----------------------------------------------------

    /// Adds a local player for a seat (single-player / split-screen).
    pub fn add_local_player(&mut self, seat: SeatId) -> PlayerId {
        if let Some(player) = self.seats.get(&seat) {
            return *player;
        }
        let player = self.allocate_player();
        self.world.add_player(player);
        let token = self.new_token();
        self.players.insert(
            player,
            PlayerSlot::new(Owner::Local(seat), format!("Seat {}", seat.0), token),
        );
        self.seats.insert(seat, player);
        self.broadcast_lobby();
        player
    }

    /// Removes a local seat's player.
    pub fn remove_local_player(&mut self, seat: SeatId) {
        if let Some(player) = self.seats.remove(&seat) {
            self.players.remove(&player);
            self.world.remove_player(player);
            self.broadcast_lobby();
        }
    }

    /// Sets a local player's held controls and queues one-shot actions for the
    /// next tick. Ignored for players that are not local.
    pub fn submit_local_input(
        &mut self,
        player: PlayerId,
        controls: S::Controls,
        actions: Vec<S::Action>,
    ) {
        if let Some(slot) = self.players.get_mut(&player)
            && matches!(slot.owner, Owner::Local(_))
        {
            slot.last_controls = controls;
            slot.local_actions.extend(actions);
        }
    }

    /// Pauses or resumes ticking (single-player pause).
    pub const fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    // ----- connection lifecycle ---------------------------------------------

    /// A transport connection was established; expect `Hello` next.
    pub fn client_connected(&mut self, client: ClientId) {
        self.clients
            .entry(client)
            .or_insert_with(|| RemoteClient::new(self.config.tuning));
    }

    /// A transport connection was lost. The player stays in the world (idle)
    /// and reserved for its reconnect token for `reconnect_grace`.
    pub fn client_disconnected(&mut self, client: ClientId) {
        self.remove_client(client, false);
    }

    /// Changes one client's tuning (snapshot rate, relevance, ...).
    pub fn set_client_tuning(&mut self, client: ClientId, tuning: NetTuning) {
        if let Some(remote) = self.clients.get_mut(&client) {
            remote.tuning = tuning;
            if remote.player.is_some() {
                self.outbox
                    .push((client, ServerMessage::TuningChanged { tuning }));
            }
        }
    }

    /// Decodes and handles bytes from a client.
    ///
    /// # Errors
    ///
    /// Returns the codec error for malformed or wrong-version bytes.
    pub fn handle_client_bytes(
        &mut self,
        client: ClientId,
        bytes: &[u8],
    ) -> Result<(), CodecError> {
        let message = protocol::decode::<ClientMessage<S>>(bytes)?;
        self.handle_client_message(client, message);
        Ok(())
    }

    /// Handles one client message. Never advances the world.
    pub fn handle_client_message(&mut self, client: ClientId, message: ClientMessage<S>) {
        let Some(remote) = self.clients.get(&client) else {
            self.stats.ignored_messages += 1;
            return;
        };
        let joined = remote.player.is_some();
        match message {
            ClientMessage::Hello {
                token,
                name,
                version,
            } => {
                if !joined {
                    self.handle_hello(client, token, name, version);
                }
            }
            ClientMessage::Inputs {
                epoch,
                frames,
                actions,
                action_floor,
            } if joined => self.handle_inputs(client, epoch, frames, actions, action_floor),
            ClientMessage::Ready { ready } if joined => {
                if let Some(slot) = remote.player.and_then(|p| self.players.get_mut(&p)) {
                    slot.ready = ready;
                }
                self.broadcast_lobby();
            }
            ClientMessage::KeyframeRequest { epoch } if joined => {
                if epoch == self.epoch {
                    self.send_keyframe(client);
                }
            }
            ClientMessage::Leave => self.remove_client(client, true),
            _ => self.stats.ignored_messages += 1,
        }
    }

    /// Starts the game from the lobby.
    pub fn start_game(&mut self) {
        if self.phase != HostPhase::Lobby {
            return;
        }
        self.phase = HostPhase::InGame;
        self.acc = 0;
        let tick = self.world.tick();
        for (client, remote) in &self.clients {
            if remote.player.is_some() {
                self.outbox.push((
                    *client,
                    ServerMessage::StartGame {
                        epoch: self.epoch,
                        tick,
                    },
                ));
            }
        }
    }

    /// Ends the session for everyone.
    pub fn end_session(&mut self, reason: EndReason) {
        for client in self.clients.keys() {
            self.outbox
                .push((*client, ServerMessage::SessionEnded { reason }));
        }
        self.phase = HostPhase::Ended;
    }

    /// Replaces the world (e.g. loading a save) and starts a new epoch. Stale
    /// inputs from the previous epoch are rejected; every client gets a
    /// keyframe of the new world.
    ///
    /// # Panics
    ///
    /// Panics if the new world reports a different tick rate than the session.
    pub fn new_epoch(&mut self, world: S) {
        assert_world_rate(&world, self.config.params);
        let old_tick = self.world.tick();
        self.world = world;
        let new_tick = self.world.tick();
        // Reservation deadlines are world ticks; keep the remaining grace
        // when the new world's tick differs (e.g. a save loaded at tick 0).
        for slot in self.players.values_mut() {
            if let Owner::Reserved { expires } = &mut slot.owner {
                *expires = new_tick.plus(expires.0.saturating_sub(old_tick.0));
            }
        }
        self.epoch = self.epoch.next();
        let max_world = self
            .world
            .player_ids()
            .iter()
            .map(|id| id.0.saturating_add(1))
            .max()
            .unwrap_or(0);
        self.next_player = self.next_player.max(max_world);
        for player in self.players.keys() {
            self.world.add_player(*player);
        }
        for slot in self.players.values_mut() {
            slot.last_input_tick = None;
            slot.last_controls = S::Controls::default();
        }
        let clients: Vec<ClientId> = self.clients.keys().copied().collect();
        for client in clients {
            if let Some(remote) = self.clients.get_mut(&client) {
                remote.clear_inputs();
            }
            self.send_keyframe(client);
        }
    }

    // ----- ticking ---------------------------------------------------------

    /// Advances real time by `dt`, running 0..n fixed ticks (subtracting
    /// accumulator, catch-up clamped to `max_catch_up_ticks`), and returns all
    /// outbound messages queued since the previous call.
    pub fn advance(&mut self, dt: Duration) -> HostOutput<S> {
        let mut output = HostOutput::default();
        if self.phase == HostPhase::InGame && !self.paused {
            let unit = SessionParams::accumulator_unit();
            self.acc += dt.as_nanos() * u128::from(self.config.params.rate());
            let max = self.config.tuning.max_catch_up_ticks.max(1);
            while self.acc >= unit && output.ticks_run < max {
                self.acc -= unit;
                self.step_tick(&mut output);
                output.ticks_run += 1;
            }
            if self.acc >= unit {
                self.acc %= unit;
                self.stats.catch_up_clamps += 1;
            }
        }
        output.outbound = std::mem::take(&mut self.outbox);
        output
    }

    /// Takes queued outbound messages without advancing time.
    pub fn take_outbound(&mut self) -> Vec<(ClientId, ServerMessage<S>)> {
        std::mem::take(&mut self.outbox)
    }

    fn step_tick(&mut self, output: &mut HostOutput<S>) {
        let tick = self.world.tick().next();
        self.expire_reservations(tick);
        let held_ticks = self
            .config
            .params
            .ms_to_ticks_ceil(self.config.tuning.held_input_ms);
        let mut inputs: TickInputs<S::Controls, S::Action> = TickInputs::empty(tick);
        let mut seqs: BTreeMap<PlayerId, InputSeq> = BTreeMap::new();

        for (player, slot) in &mut self.players {
            let mut actions = Vec::new();
            let controls = match slot.owner {
                Owner::Local(_) => {
                    for action in slot.local_actions.drain(..) {
                        let id = slot.next_local_action;
                        slot.next_local_action = id.next();
                        if slot.ledger.try_apply(id) {
                            actions.push(ActionEnvelope { id, action });
                        }
                    }
                    slot.last_controls.clone()
                }
                Owner::Remote(client) => {
                    let Some(remote) = self.clients.get_mut(&client) else {
                        continue;
                    };
                    // Anything keyed before this tick is late now.
                    let current = remote.inputs.split_off(&tick);
                    let stale = std::mem::replace(&mut remote.inputs, current);
                    let mut candidates: Vec<ActionEnvelope<S::Action>> = Vec::new();
                    for (_, frame) in stale {
                        candidates.extend(frame.actions);
                    }
                    let current_timed = remote.timed_actions.split_off(&tick);
                    for (_, timed) in std::mem::replace(&mut remote.timed_actions, current_timed) {
                        candidates.extend(timed);
                    }
                    let frame = remote.inputs.remove(&tick);
                    let controls = if let Some(frame) = frame {
                        remote.stats.frames_on_time += 1;
                        remote.ack_seq =
                            Some(remote.ack_seq.map_or(frame.seq, |a| a.max(frame.seq)));
                        seqs.insert(*player, frame.seq);
                        let mut ordered = frame.actions;
                        ordered.append(&mut candidates);
                        candidates = ordered;
                        slot.last_controls = frame.controls.clone();
                        slot.last_input_tick = Some(tick);
                        frame.controls
                    } else if slot
                        .last_input_tick
                        .is_some_and(|last| tick.0.saturating_sub(last.0) <= held_ticks)
                    {
                        remote.stats.held_ticks += 1;
                        slot.last_controls.clone()
                    } else {
                        remote.stats.neutral_ticks += 1;
                        S::Controls::default()
                    };
                    if let Some(timed) = remote.timed_actions.remove(&tick) {
                        candidates.extend(timed);
                    }
                    candidates.append(&mut remote.late_actions);
                    for envelope in candidates {
                        if slot.ledger.try_apply(envelope.id) {
                            actions.push(envelope);
                        } else {
                            remote.stats.duplicate_actions += 1;
                        }
                    }
                    controls
                }
                Owner::Reserved { .. } => S::Controls::default(),
            };
            inputs
                .players
                .insert(*player, PlayerTickInput { controls, actions });
        }

        let result = self.world.simulate_tick(&inputs);
        debug_assert_eq!(
            self.world.tick(),
            tick,
            "simulate_tick must advance to inputs.tick"
        );
        self.stats.ticks += 1;
        let events = tag_events(tick, result.events, &seqs);
        if self.config.record_tick_hashes {
            output.tick_hashes.push((tick, self.world.state_hash()));
        }
        for remote in self.clients.values_mut() {
            if remote.player.is_none() {
                continue;
            }
            remote
                .pending_ops
                .extend(result.terrain_ops.iter().map(|op| (tick, op.clone())));
            remote.pending_events.extend(events.iter().cloned());
        }
        output.events.extend(events);
        self.emit_due_frames();
    }

    fn emit_due_frames(&mut self) {
        let rate = self.config.params.rate();
        let epoch = self.epoch;
        let debug_world_hash = self
            .clients
            .values()
            .any(|c| c.tuning.debug_hash_interval > 0);
        let mut world_hash = None;
        for (client, remote) in &mut self.clients {
            if remote.player.is_none() {
                continue;
            }
            remote.frame_acc += u64::from(remote.tuning.snapshot_rate_hz.max(1)).min(rate);
            remote.far_acc += u64::from(remote.tuning.far_player_rate_hz.max(1)).min(rate);
            if remote.far_acc >= rate {
                remote.far_acc -= rate;
                remote.far_due = true;
            }
            if remote.frame_acc < rate {
                continue;
            }
            remote.frame_acc -= rate;
            let ack_action = remote
                .player
                .and_then(|player| self.players.get(&player))
                .map_or(ActionId(0), |slot| slot.ledger.watermark());
            let mut frame = build_frame(&self.world, epoch, ack_action, remote);
            let interval = u64::from(remote.tuning.debug_hash_interval);
            if debug_world_hash
                && interval > 0
                && remote.stats.frames_sent % interval == 0
                && relevance_for(&self.world, remote).is_infinite()
            {
                frame.debug_hash = Some(*world_hash.get_or_insert_with(|| self.world.state_hash()));
            }
            remote.stats.frames_sent += 1;
            self.outbox
                .push((*client, ServerMessage::Frame(Box::new(frame))));
        }
    }

    fn expire_reservations(&mut self, tick: Tick) {
        let expired: Vec<PlayerId> = self
            .players
            .iter()
            .filter(
                |(_, slot)| matches!(slot.owner, Owner::Reserved { expires } if expires <= tick),
            )
            .map(|(player, _)| *player)
            .collect();
        if expired.is_empty() {
            return;
        }
        for player in expired {
            self.players.remove(&player);
            self.world.remove_player(player);
        }
        self.broadcast_lobby();
    }

    // ----- message handling ------------------------------------------------

    fn handle_hello(
        &mut self,
        client: ClientId,
        token: Option<ReconnectToken>,
        name: String,
        version: u8,
    ) {
        if self.phase == HostPhase::Ended {
            self.reject(client, RejectReason::NotAccepting);
            return;
        }
        if version != PROTOCOL_VERSION {
            self.reject(
                client,
                RejectReason::VersionMismatch {
                    expected: PROTOCOL_VERSION,
                    found: version,
                },
            );
            return;
        }
        let reclaimed = token.and_then(|token| {
            self.players
                .iter()
                .find(|(_, slot)| slot.token == token)
                .map(|(player, slot)| (*player, slot.owner))
        });
        let player = match reclaimed {
            Some((player, Owner::Reserved { .. })) => {
                if let Some(slot) = self.players.get_mut(&player) {
                    slot.owner = Owner::Remote(client);
                    slot.name = name;
                    slot.last_input_tick = None;
                    // A new client session restarts action ids at 1.
                    slot.ledger = ActionLedger::new();
                }
                player
            }
            Some(_) => {
                self.reject(client, RejectReason::TokenInUse);
                return;
            }
            None => {
                let player = self.allocate_player();
                self.world.add_player(player);
                let token = self.new_token();
                self.players
                    .insert(player, PlayerSlot::new(Owner::Remote(client), name, token));
                player
            }
        };
        let Some(remote) = self.clients.get_mut(&client) else {
            return;
        };
        remote.player = Some(player);
        remote.clear_inputs();
        remote.reset_baseline(&self.world);
        remote.pending_events.clear();
        let token = self
            .players
            .get(&player)
            .map_or(ReconnectToken(0), |s| s.token);
        let welcome = Welcome {
            client,
            player,
            epoch: self.epoch,
            token,
            host_tick: self.world.tick(),
            started: self.phase == HostPhase::InGame,
            session_params: self.config.params,
            tuning: remote.tuning,
            keyframe: self.world.keyframe(),
        };
        self.outbox
            .push((client, ServerMessage::Welcome(Box::new(welcome))));
        self.broadcast_lobby();
    }

    fn handle_inputs(
        &mut self,
        client: ClientId,
        epoch: SessionEpoch,
        frames: Vec<InputFrame<S>>,
        actions: Vec<TaggedAction<S>>,
        action_floor: ActionId,
    ) {
        if epoch != self.epoch {
            self.stats.stale_epoch_inputs += 1;
            return;
        }
        let host_tick = self.world.tick();
        let max_lead = self
            .config
            .params
            .ms_to_ticks_ceil(self.config.tuning.max_input_lead_ms);
        let max_late = self.config.max_late_actions;
        let Some(remote) = self.clients.get_mut(&client) else {
            return;
        };
        let Some(player) = remote.player else {
            return;
        };
        let Some(slot) = self.players.get_mut(&player) else {
            return;
        };
        slot.ledger.raise_floor(action_floor);
        remote.stats.input_datagrams += 1;

        if let Some(newest) = frames.iter().max_by_key(|frame| frame.seq)
            && remote.newest_seen.is_none_or(|seen| newest.seq > seen)
        {
            remote.newest_seen = Some(newest.seq);
            let margin = newest.target_tick.diff(host_tick);
            remote.margin_min = Some(remote.margin_min.map_or(margin, |m| m.min(margin)));
            remote.margin_seq = remote.margin_seq.max(newest.seq);
            remote.stats.last_margin = Some(margin);
        }

        for frame in frames {
            if remote.ack_seq.is_some_and(|ack| frame.seq <= ack) {
                continue;
            }
            if frame.target_tick > host_tick {
                if frame.target_tick.0 > host_tick.0 + max_lead {
                    remote.stats.frames_too_far_ahead += 1;
                    continue;
                }
                remote.inputs.entry(frame.target_tick).or_insert(frame);
            } else {
                remote.stats.frames_late += 1;
                for envelope in frame.actions {
                    push_late(remote, &slot.ledger, envelope, max_late);
                }
            }
        }
        for tagged in actions {
            if slot.ledger.contains(tagged.envelope.id) {
                continue;
            }
            if tagged.target_tick > host_tick && tagged.target_tick.0 <= host_tick.0 + max_lead {
                remote
                    .timed_actions
                    .entry(tagged.target_tick)
                    .or_default()
                    .push(tagged.envelope);
            } else {
                push_late(remote, &slot.ledger, tagged.envelope, max_late);
            }
        }
        remote.stats.max_buffered_inputs =
            remote.stats.max_buffered_inputs.max(remote.inputs.len());
    }

    fn remove_client(&mut self, client: ClientId, leaving: bool) {
        let Some(remote) = self.clients.remove(&client) else {
            return;
        };
        let Some(player) = remote.player else {
            return;
        };
        if leaving || self.config.reconnect_grace.is_zero() {
            self.players.remove(&player);
            self.world.remove_player(player);
        } else if let Some(slot) = self.players.get_mut(&player) {
            let grace = self
                .config
                .params
                .duration_to_ticks_f64(self.config.reconnect_grace);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let grace = grace.ceil() as u64;
            slot.owner = Owner::Reserved {
                expires: self.world.tick().plus(grace),
            };
            slot.last_input_tick = None;
        }
        self.broadcast_lobby();
    }

    fn send_keyframe(&mut self, client: ClientId) {
        let Some(remote) = self.clients.get_mut(&client) else {
            return;
        };
        if remote.player.is_none() {
            return;
        }
        remote.reset_baseline(&self.world);
        self.outbox.push((
            client,
            ServerMessage::Keyframe {
                epoch: self.epoch,
                tick: self.world.tick(),
                world: Box::new(self.world.keyframe()),
            },
        ));
    }

    fn reject(&mut self, client: ClientId, reason: RejectReason) {
        self.outbox
            .push((client, ServerMessage::Rejected { reason }));
    }

    fn broadcast_lobby(&mut self) {
        let roster = self.roster();
        for (client, remote) in &self.clients {
            if remote.player.is_some() {
                self.outbox.push((
                    *client,
                    ServerMessage::Lobby {
                        roster: roster.clone(),
                    },
                ));
            }
        }
    }

    const fn allocate_player(&mut self) -> PlayerId {
        let player = PlayerId(self.next_player);
        self.next_player = self.next_player.saturating_add(1);
        player
    }

    const fn new_token(&mut self) -> ReconnectToken {
        ReconnectToken(self.token_rng.next_u64() | 1)
    }
}

fn push_late<S: NetSimulation>(
    remote: &mut RemoteClient<S>,
    ledger: &ActionLedger,
    envelope: ActionEnvelope<S::Action>,
    max_late: usize,
) {
    if ledger.contains(envelope.id) || remote.late_actions.iter().any(|e| e.id == envelope.id) {
        return;
    }
    if remote.late_actions.len() >= max_late {
        remote.stats.late_actions_dropped += 1;
        return;
    }
    remote.late_actions.push(envelope);
}

fn relevance_for<S: NetSimulation>(world: &S, remote: &RemoteClient<S>) -> Relevance {
    let center = remote
        .player
        .and_then(|player| world.player_pose(player))
        .map(|pose| (pose.x, pose.y));
    Relevance {
        viewer: remote.player,
        center,
        radius: remote.tuning.relevance_radius,
    }
}

/// Attaches tick, input seq and per-`(player, kind)` ordinals to events.
pub fn tag_events<E>(
    tick: Tick,
    events: Vec<TickEvent<E>>,
    seqs: &BTreeMap<PlayerId, InputSeq>,
) -> Vec<NetEvent<E>> {
    let mut ordinals: BTreeMap<(Option<PlayerId>, u32), u32> = BTreeMap::new();
    events
        .into_iter()
        .map(|event| {
            let ordinal = ordinals.entry((event.player, event.kind)).or_insert(0);
            let tagged = NetEvent {
                tick,
                player: event.player,
                seq: event.player.and_then(|player| seqs.get(&player).copied()),
                kind: event.kind,
                ordinal: *ordinal,
                payload: event.payload,
            };
            *ordinal += 1;
            tagged
        })
        .collect()
}

fn build_frame<S: NetSimulation>(
    world: &S,
    epoch: SessionEpoch,
    ack_action: ActionId,
    remote: &mut RemoteClient<S>,
) -> ServerFrame<S> {
    let relevance = relevance_for(world, remote);
    let mut players = Vec::new();
    let mut far = Vec::new();
    let mut relevant_players = BTreeSet::new();
    let roster = world.player_ids();
    for player in &roster {
        let pose = world.player_pose(*player);
        let relevant = Some(*player) == remote.player
            || pose.is_none_or(|pose| relevance.contains(pose.x, pose.y));
        if relevant {
            if let Some(state) = world.player_state(*player) {
                players.push((*player, state));
                relevant_players.insert(*player);
            }
        } else if let Some(pose) = pose {
            far.push(PlayerMarker {
                player: *player,
                pose,
            });
        }
    }
    let far_players = if remote.far_due {
        remote.far_due = false;
        Some(far)
    } else {
        None
    };

    let mut sections = Vec::new();
    let keys = world.section_keys();
    for key in &keys {
        let Some(hash) = world.section_hash(key) else {
            continue;
        };
        if remote.section_hashes.get(key) != Some(&hash)
            && let Some(section) = world.section(key)
        {
            sections.push((key.clone(), section));
            remote.section_hashes.insert(key.clone(), hash);
        }
    }
    let live: BTreeSet<&S::SectionKey> = keys.iter().collect();
    let removed_sections: Vec<S::SectionKey> = remote
        .section_hashes
        .keys()
        .filter(|key| !live.contains(key))
        .cloned()
        .collect();
    for key in &removed_sections {
        remote.section_hashes.remove(key);
    }

    let roster_change = if roster == remote.roster_sent {
        None
    } else {
        remote.roster_sent.clone_from(&roster);
        Some(roster)
    };

    let terrain_ops = std::mem::take(&mut remote.pending_ops)
        .into_iter()
        .filter(|(_, op)| world.terrain_op_relevant(op, &relevance))
        .collect();
    let events = std::mem::take(&mut remote.pending_events)
        .into_iter()
        .filter(|event| {
            event
                .player
                .is_none_or(|player| relevant_players.contains(&player))
        })
        .collect();

    let input_margin = remote
        .margin_min
        .take()
        .map(|m| i32::try_from(m).unwrap_or(if m < 0 { i32::MIN } else { i32::MAX }));

    ServerFrame {
        epoch,
        tick: world.tick(),
        ack_seq: remote.ack_seq,
        ack_action,
        input_margin,
        margin_seq: remote.margin_seq,
        roster: roster_change,
        players,
        far_players,
        entities: world.entities(&relevance),
        terrain_ops,
        sections,
        removed_sections,
        events,
        debug_hash: None,
    }
}

fn assert_world_rate<S: NetSimulation>(world: &S, params: SessionParams) {
    if let Some(hz) = world.tick_rate_hz() {
        assert_eq!(
            hz, params.tick_rate_hz,
            "world tick rate must match the session tick rate"
        );
    }
}
