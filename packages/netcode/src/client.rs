//! The client session: prediction, reconciliation, clock sync and remote
//! interpolation (plan §8).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use crate::clock::{ClientClock, MarginResponse};
use crate::host::tag_events;
use crate::ids::{ActionId, ClientId, InputSeq, PlayerId, ReconnectToken, SessionEpoch, Tick};
use crate::interpolation::{InterpTimeline, SnapshotBuffer, VisualOffset};
use crate::params::{NetTuning, SessionParams};
use crate::protocol::{
    self, ClientMessage, CodecError, EndReason, InputFrame, LobbyEntry, NetEvent, PROTOCOL_VERSION,
    PlayerMarker, RejectReason, ServerFrame, ServerMessage, TaggedAction, Welcome,
};
use crate::simulation::{ActionEnvelope, NetSimulation, PlayerTickInput, Pose, TickInputs};

/// Client configuration.
#[derive(Clone, Debug, Default)]
pub struct ClientConfig {
    /// Display name sent in `Hello`.
    pub name: String,
    /// Token from a previous session to reclaim the same player.
    pub reconnect_token: Option<ReconnectToken>,
    /// Initial round-trip estimate (e.g. Quinn's `connection.rtt()`), used to
    /// pick the initial run-ahead lead.
    pub initial_rtt: Option<Duration>,
}

/// Client lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientPhase {
    /// `Hello` sent, waiting for `Welcome`.
    Connecting,
    /// Joined, waiting for `StartGame`.
    Lobby,
    /// Predicting and sending inputs.
    InGame,
    /// The host refused the connection.
    Rejected(RejectReason),
    /// The session ended (or the client left).
    Ended(EndReason),
}

/// Where a presented event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSource {
    /// Local prediction (played immediately).
    Predicted,
    /// An authoritative server frame.
    Authoritative,
}

/// An event ready for audio/vfx.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentedEvent<E> {
    /// Source.
    pub source: EventSource,
    /// The event with its identity.
    pub event: NetEvent<E>,
}

/// Client counters (debug overlay / harness).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientStats {
    /// Server frames applied.
    pub frames_received: u64,
    /// Frames rejected for stale epoch or tick.
    pub stale_frames: u64,
    /// Keyframes applied.
    pub keyframes_received: u64,
    /// Reconciliations that moved the local pose by at least the epsilon.
    pub corrections: u64,
    /// Corrections that snapped instead of smoothing.
    pub snaps: u64,
    /// Sum of correction magnitudes.
    pub correction_total: f64,
    /// Largest correction magnitude.
    pub correction_max: f32,
    /// Most recent correction magnitude.
    pub last_correction: f32,
    /// Debug hashes compared.
    pub hash_checks: u64,
    /// Debug hash mismatches (each requests a keyframe).
    pub hash_mismatches: u64,
    /// Hard clock resyncs.
    pub clock_resyncs: u64,
    /// Most recent input margin from the host.
    pub last_margin: Option<i64>,
    /// Largest `pending` length seen.
    pub max_pending: usize,
    /// Ticks predicted forward.
    pub predicted_ticks: u64,
    /// Ticks replayed during reconciliation.
    pub replayed_ticks: u64,
    /// Ticks skipped because too much input was unacknowledged.
    pub stalled_ticks: u64,
    /// Authoritative events suppressed because prediction already played them.
    pub deduplicated_events: u64,
    /// Input datagrams produced.
    pub input_datagrams: u64,
}

/// Everything the renderer needs from a client.
#[derive(Debug)]
pub struct ClientView<'a, S: NetSimulation> {
    /// Lifecycle phase.
    pub phase: ClientPhase,
    /// The local player.
    pub local_player: Option<PlayerId>,
    /// Local pose: interpolated between the previous and current predicted
    /// fixed ticks, plus the smoothing offset. The camera follows this.
    pub local_pose: Option<Pose>,
    /// Remote players interpolated on the interpolation timeline (bounded
    /// extrapolation).
    pub remote_poses: BTreeMap<PlayerId, Pose>,
    /// Position-only markers for far players.
    pub far_players: &'a [PlayerMarker],
    /// Predicted world (terrain, HUD).
    pub predicted: Option<&'a S>,
    /// Last confirmed server state.
    pub authority: Option<&'a S>,
    /// Client (predicted) tick.
    pub client_tick: Tick,
    /// Newest server tick received.
    pub server_tick: Tick,
    /// Remote interpolation time in fractional ticks.
    pub interp_time: Option<f64>,
    /// Current smoothing offset.
    pub smoothing: VisualOffset,
}

/// `(player, tick, kind, ordinal)`. The client predicts input `seq` on exactly
/// `target_tick`, so tick and seq identify the same input; keying by tick also
/// matches authoritative events the host produced under held input (which
/// carry no seq).
type EventKey = (PlayerId, Tick, u32, u32);

/// A connected client.
#[derive(Debug)]
pub struct ClientSession<S: NetSimulation> {
    config: ClientConfig,
    phase: ClientPhase,
    now: Duration,
    client: Option<ClientId>,
    player: Option<PlayerId>,
    epoch: SessionEpoch,
    token: Option<ReconnectToken>,
    params: SessionParams,
    tuning: NetTuning,
    authority: Option<S>,
    predicted: Option<S>,
    pending: VecDeque<InputFrame<S>>,
    unacked_actions: VecDeque<TaggedAction<S>>,
    queued_actions: Vec<S::Action>,
    next_seq: InputSeq,
    next_action: ActionId,
    ack_seq: Option<InputSeq>,
    ack_action: ActionId,
    clock: ClientClock,
    interp: InterpTimeline,
    remote: SnapshotBuffer,
    smoothing: VisualOffset,
    prev_pose: Option<Pose>,
    cur_pose: Option<Pose>,
    far_players: Vec<PlayerMarker>,
    lobby: Vec<LobbyEntry>,
    events: Vec<PresentedEvent<S::Event>>,
    predicted_events: BTreeMap<EventKey, InputSeq>,
    outbox: Vec<ClientMessage<S>>,
    stats: ClientStats,
    rtt: Option<Duration>,
    newest_server_tick: Tick,
}

impl<S: NetSimulation> ClientSession<S> {
    /// Creates a session; `Hello` is queued in the outbox.
    #[must_use]
    pub fn new(config: ClientConfig) -> Self {
        let hello = ClientMessage::Hello {
            token: config.reconnect_token,
            name: config.name.clone(),
            version: PROTOCOL_VERSION,
        };
        let rtt = config.initial_rtt;
        Self {
            config,
            phase: ClientPhase::Connecting,
            now: Duration::ZERO,
            client: None,
            player: None,
            epoch: SessionEpoch(0),
            token: None,
            params: SessionParams::default(),
            tuning: NetTuning::default(),
            authority: None,
            predicted: None,
            pending: VecDeque::new(),
            unacked_actions: VecDeque::new(),
            queued_actions: Vec::new(),
            next_seq: InputSeq(1),
            next_action: ActionId(1),
            ack_seq: None,
            ack_action: ActionId(0),
            clock: ClientClock::new(),
            interp: InterpTimeline::default(),
            remote: SnapshotBuffer::default(),
            smoothing: VisualOffset::default(),
            prev_pose: None,
            cur_pose: None,
            far_players: Vec::new(),
            lobby: Vec::new(),
            events: Vec::new(),
            predicted_events: BTreeMap::new(),
            outbox: vec![hello],
            stats: ClientStats::default(),
            rtt,
            newest_server_tick: Tick(0),
        }
    }

    // ----- accessors -------------------------------------------------------

    /// Lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> ClientPhase {
        self.phase
    }

    /// Configuration.
    #[must_use]
    pub const fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// The local player, once welcomed.
    #[must_use]
    pub const fn player(&self) -> Option<PlayerId> {
        self.player
    }

    /// Transport id reported by the host.
    #[must_use]
    pub const fn client_id(&self) -> Option<ClientId> {
        self.client
    }

    /// Current epoch.
    #[must_use]
    pub const fn epoch(&self) -> SessionEpoch {
        self.epoch
    }

    /// Reconnect token from `Welcome`.
    #[must_use]
    pub const fn reconnect_token(&self) -> Option<ReconnectToken> {
        self.token
    }

    /// Session parameters from `Welcome`.
    #[must_use]
    pub const fn params(&self) -> &SessionParams {
        &self.params
    }

    /// Current tuning.
    #[must_use]
    pub const fn tuning(&self) -> &NetTuning {
        &self.tuning
    }

    /// Counters.
    #[must_use]
    pub const fn stats(&self) -> &ClientStats {
        &self.stats
    }

    /// Last confirmed server state.
    #[must_use]
    pub const fn authority(&self) -> Option<&S> {
        self.authority.as_ref()
    }

    /// Predicted world.
    #[must_use]
    pub const fn predicted(&self) -> Option<&S> {
        self.predicted.as_ref()
    }

    /// Unacknowledged input frames.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Unacknowledged actions.
    #[must_use]
    pub fn unacked_actions_len(&self) -> usize {
        self.unacked_actions.len()
    }

    /// Number of actions issued so far (ids `1..=n`).
    #[must_use]
    pub const fn actions_issued(&self) -> u64 {
        self.next_action.0 - 1
    }

    /// The tick clock.
    #[must_use]
    pub const fn clock(&self) -> &ClientClock {
        &self.clock
    }

    /// Remote snapshot buffer.
    #[must_use]
    pub const fn remote_buffer(&self) -> &SnapshotBuffer {
        &self.remote
    }

    /// Size of the prediction-event dedup set.
    #[must_use]
    pub fn predicted_event_keys(&self) -> usize {
        self.predicted_events.len()
    }

    /// Lobby roster.
    #[must_use]
    pub fn lobby(&self) -> &[LobbyEntry] {
        &self.lobby
    }

    /// Updates the round-trip estimate used for the initial lead.
    pub const fn set_rtt(&mut self, rtt: Duration) {
        self.rtt = Some(rtt);
    }

    /// Current remote interpolation delay in ticks.
    #[must_use]
    pub fn interp_delay_ticks(&self) -> f64 {
        self.tuning
            .interp_delay_ticks(&self.params, self.interp.jitter())
    }

    // ----- outbound ----------------------------------------------------------

    /// Takes queued messages (e.g. `Hello`, `KeyframeRequest`).
    pub fn take_outbound(&mut self) -> Vec<ClientMessage<S>> {
        std::mem::take(&mut self.outbox)
    }

    /// Toggles lobby ready state.
    pub fn set_ready(&mut self, ready: bool) {
        self.outbox.push(ClientMessage::Ready { ready });
    }

    /// Leaves the session for good and returns the message to send.
    pub const fn leave(&mut self) -> ClientMessage<S> {
        self.phase = ClientPhase::Ended(EndReason::Left);
        ClientMessage::Leave
    }

    /// Drains events for presentation (prediction first, deduplicated).
    pub fn drain_events(&mut self) -> Vec<PresentedEvent<S::Event>> {
        std::mem::take(&mut self.events)
    }

    // ----- inbound -----------------------------------------------------------

    /// Decodes and handles bytes from the host.
    ///
    /// # Errors
    ///
    /// Returns the codec error for malformed or wrong-version bytes.
    pub fn handle_server_bytes(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        let message = protocol::decode::<ServerMessage<S>>(bytes)?;
        self.handle_server_message(message);
        Ok(())
    }

    /// Handles one host message.
    pub fn handle_server_message(&mut self, message: ServerMessage<S>) {
        match message {
            ServerMessage::Welcome(welcome) => self.handle_welcome(*welcome),
            ServerMessage::Frame(frame) => self.handle_frame(*frame),
            ServerMessage::Lobby { roster } => self.lobby = roster,
            ServerMessage::StartGame { epoch, tick } => {
                if epoch == self.epoch && self.phase == ClientPhase::Lobby {
                    self.phase = ClientPhase::InGame;
                    self.clock.start(tick.plus(self.initial_lead()));
                }
            }
            ServerMessage::Keyframe { epoch, tick, world } => {
                self.handle_keyframe(epoch, tick, *world);
            }
            ServerMessage::TuningChanged { tuning } => self.tuning = tuning,
            ServerMessage::Rejected { reason } => self.phase = ClientPhase::Rejected(reason),
            ServerMessage::SessionEnded { reason } => self.phase = ClientPhase::Ended(reason),
        }
    }

    fn handle_welcome(&mut self, welcome: Welcome<S>) {
        if self.phase != ClientPhase::Connecting {
            return;
        }
        self.client = Some(welcome.client);
        self.player = Some(welcome.player);
        self.epoch = welcome.epoch;
        self.token = Some(welcome.token);
        self.params = welcome.session_params;
        self.tuning = welcome.tuning;
        let authority = S::from_keyframe(welcome.keyframe);
        self.newest_server_tick = authority.tick();
        self.cur_pose = authority.player_pose(welcome.player);
        self.prev_pose = self.cur_pose;
        self.predicted = Some(authority.clone());
        self.authority = Some(authority);
        self.stats.keyframes_received += 1;
        if welcome.started {
            self.phase = ClientPhase::InGame;
            self.clock
                .start(welcome.host_tick.plus(self.initial_lead()));
        } else {
            self.phase = ClientPhase::Lobby;
        }
    }

    fn handle_keyframe(&mut self, epoch: SessionEpoch, tick: Tick, world: S::Keyframe) {
        if epoch < self.epoch || self.authority.is_none() {
            self.stats.stale_frames += 1;
            return;
        }
        let world = S::from_keyframe(world);
        self.stats.keyframes_received += 1;
        if epoch > self.epoch {
            // New world: everything from the previous epoch is void.
            self.epoch = epoch;
            self.pending.clear();
            self.unacked_actions.clear();
            self.queued_actions.clear();
            self.predicted_events.clear();
            self.remote.clear();
            self.interp.reset();
            self.smoothing.reset();
            self.newest_server_tick = tick;
            self.cur_pose = self.player.and_then(|p| world.player_pose(p));
            self.prev_pose = self.cur_pose;
            self.predicted = Some(world.clone());
            self.authority = Some(world);
            if self.phase == ClientPhase::InGame {
                self.clock.start(tick.plus(self.initial_lead()));
            }
            return;
        }
        if self.authority.as_ref().is_some_and(|a| tick < a.tick()) {
            self.stats.stale_frames += 1;
            return;
        }
        self.authority = Some(world);
        self.newest_server_tick = self.newest_server_tick.max(tick);
        self.trim_pending(tick);
        self.reconcile(tick);
    }

    fn handle_frame(&mut self, frame: ServerFrame<S>) {
        if frame.epoch != self.epoch {
            self.stats.stale_frames += 1;
            return;
        }
        let Some(authority) = self.authority.as_mut() else {
            self.stats.stale_frames += 1;
            return;
        };
        if frame.tick <= authority.tick() {
            self.stats.stale_frames += 1;
            return;
        }
        let ServerFrame {
            epoch: _,
            tick,
            ack_seq,
            ack_action,
            input_margin,
            margin_seq,
            roster,
            players,
            far_players,
            entities,
            ops,
            chunk_refreshes,
            sections,
            removed_sections,
            events,
            debug_hash,
        } = frame;
        self.stats.frames_received += 1;

        // 1. Apply to authority.
        let relevant = apply_replication(
            authority,
            tick,
            roster,
            players,
            entities,
            &ops,
            chunk_refreshes,
            sections,
            &removed_sections,
        );
        if let Some(expected) = debug_hash {
            self.stats.hash_checks += 1;
            if authority.visible_state_hash(self.player) != expected {
                self.stats.hash_mismatches += 1;
                self.outbox
                    .push(ClientMessage::KeyframeRequest { epoch: self.epoch });
            }
        }
        // Remote players that are still in the world *and* inside this
        // client's relevance set. Players that left relevance stop being
        // interpolated (they would otherwise freeze at the edge forever) and
        // are shown through `far_players` markers instead.
        let roster: BTreeSet<PlayerId> = authority.player_ids().into_iter().collect();
        let interpolated: BTreeSet<PlayerId> = relevant
            .iter()
            .copied()
            .filter(|player| roster.contains(player))
            .collect();
        let remote_poses: Vec<(PlayerId, Pose)> = relevant
            .iter()
            .filter(|player| Some(**player) != self.player)
            .filter_map(|player| authority.player_pose(*player).map(|pose| (*player, pose)))
            .collect();

        // 2. Acknowledgements.
        if let Some(ack) = ack_seq {
            self.ack_seq = Some(self.ack_seq.map_or(ack, |a| a.max(ack)));
        }
        self.ack_action = self.ack_action.max(ack_action);
        let acked = self.ack_action;
        self.unacked_actions.retain(|t| t.envelope.id > acked);
        self.trim_pending(tick);

        // Clock feedback.
        if let Some(margin) = input_margin {
            self.stats.last_margin = Some(i64::from(margin));
            let response = self.clock.on_margin(
                i64::from(margin),
                margin_seq,
                self.next_seq,
                &self.params,
                &self.tuning,
            );
            if matches!(response, MarginResponse::Resynced { .. }) {
                self.stats.clock_resyncs += 1;
            }
        }
        if self.phase == ClientPhase::InGame && tick >= self.clock.tick() {
            // Running behind the server: catch up so inputs can land on time.
            let lead =
                u64::try_from(self.tuning.input_lead_target_ticks(&self.params)).unwrap_or(1);
            self.clock.catch_up_to(tick.plus(lead));
        }

        // 6. Remote interpolation buffer.
        self.newest_server_tick = self.newest_server_tick.max(tick);
        self.interp.on_frame(tick, self.now, &self.params);
        for (player, pose) in remote_poses {
            self.remote.push(player, tick, pose);
        }
        self.remote.retain_players(&interpolated);
        if let Some(far) = far_players {
            self.far_players = far;
        }

        self.present_authoritative_events(events, tick);

        // 3-5. Re-predict and correct.
        self.reconcile(tick);
    }

    /// Queues authoritative events, suppressing local-player events already
    /// played from prediction: same `(player, tick, kind, ordinal)` and, when
    /// the host consumed an input that tick, the same input seq.
    fn present_authoritative_events(&mut self, events: Vec<NetEvent<S::Event>>, tick: Tick) {
        for event in events {
            let key = event
                .player
                .filter(|player| Some(*player) == self.player)
                .map(|player| (player, event.tick, event.kind, event.ordinal));
            let predicted_seq = key.and_then(|key| self.predicted_events.get(&key).copied());
            if let (Some(key), Some(predicted_seq)) = (key, predicted_seq)
                && event.seq.is_none_or(|seq| seq == predicted_seq)
            {
                self.predicted_events.remove(&key);
                self.stats.deduplicated_events += 1;
                continue;
            }
            self.events.push(PresentedEvent {
                source: EventSource::Authoritative,
                event,
            });
        }
        self.predicted_events
            .retain(|(_, event_tick, _, _), _| *event_tick > tick);
    }

    fn trim_pending(&mut self, tick: Tick) {
        let ack = self.ack_seq;
        self.pending
            .retain(|frame| frame.target_tick > tick && ack.is_none_or(|a| frame.seq > a));
    }

    /// Rebuilds `predicted` from `authority` and replays pending inputs, then
    /// applies the snap/smooth policy to the local pose.
    fn reconcile(&mut self, authority_tick: Tick) {
        let Some(authority) = self.authority.as_ref() else {
            return;
        };
        let old_tick = self.predicted.as_ref().map(NetSimulation::tick);
        let old_pose = self.cur_pose;
        let mut predicted = authority.clone();
        let target = self.clock.tick();
        let mut pending = self.pending.iter().peekable();
        let mut replayed = 0u64;
        let mut tick = authority_tick.next();
        while tick <= target {
            let mut inputs: TickInputs<S::Controls, S::Action> = TickInputs::empty(tick);
            while pending.peek().is_some_and(|frame| frame.target_tick < tick) {
                pending.next();
            }
            if let Some(local) = self.player {
                let mut input = PlayerTickInput {
                    controls: S::Controls::default(),
                    actions: Vec::new(),
                };
                if tick == authority_tick.next() {
                    // Mirror the host: late (unacked, past) actions land on the
                    // next tick.
                    input.actions.extend(
                        self.unacked_actions
                            .iter()
                            .filter(|t| t.target_tick <= authority_tick)
                            .filter(|t| S::predicts_action(&t.envelope.action))
                            .map(|t| t.envelope.clone()),
                    );
                }
                if let Some(frame) = pending.next_if(|frame| frame.target_tick == tick) {
                    input.controls = frame.controls.clone();
                    input.actions.extend(predicted_actions::<S>(&frame.actions));
                }
                inputs.players.insert(local, input);
            }
            let _ = predicted.simulate_tick(&inputs);
            replayed += 1;
            tick = tick.next();
        }
        self.stats.replayed_ticks += replayed;
        let new_pose = self.player.and_then(|p| predicted.player_pose(p));
        let comparable = old_tick == Some(predicted.tick());
        self.predicted = Some(predicted);
        match (old_pose, new_pose) {
            (Some(old), Some(new)) if comparable => self.apply_correction(old, new),
            _ => {
                self.cur_pose = new_pose;
                self.prev_pose = new_pose;
            }
        }
    }

    fn apply_correction(&mut self, old: Pose, new: Pose) {
        let error = old.distance(&new);
        let mode_changed = old.mode != new.mode;
        if !mode_changed && error < self.tuning.correction_epsilon {
            self.cur_pose = Some(new);
            return;
        }
        self.stats.corrections += 1;
        self.stats.correction_total += f64::from(error);
        self.stats.correction_max = self.stats.correction_max.max(error);
        self.stats.last_correction = error;
        if mode_changed || error >= self.tuning.snap_threshold {
            self.stats.snaps += 1;
            self.smoothing.reset();
            self.prev_pose = Some(new);
            self.cur_pose = Some(new);
            return;
        }
        let (dx, dy) = (old.x - new.x, old.y - new.y);
        self.smoothing.add(dx, dy);
        self.prev_pose = self.prev_pose.map(|prev| prev.translated(-dx, -dy));
        self.cur_pose = Some(new);
    }

    // ----- ticking -----------------------------------------------------------

    /// Advances real time by `dt`. Runs 0..n fixed client ticks with the
    /// sampled `controls`; `actions` are issued on the next tick that runs.
    /// Returns the messages to send (an `Inputs` datagram when ticks ran).
    pub fn advance(
        &mut self,
        dt: Duration,
        controls: &S::Controls,
        actions: Vec<S::Action>,
    ) -> Vec<ClientMessage<S>> {
        self.now += dt;
        self.queued_actions.extend(actions);
        if self.phase != ClientPhase::InGame || self.predicted.is_none() {
            self.queued_actions.clear();
            return self.take_outbound();
        }
        let due = self.clock.advance(dt, &self.params, &self.tuning);
        let cap = usize::try_from(self.params.ms_to_ticks_ceil(self.tuning.max_prediction_ms))
            .unwrap_or(usize::MAX)
            .max(1);
        let mut new_frames = 0usize;
        for _ in 0..due {
            if self.pending.len() >= cap {
                self.stats.stalled_ticks += 1;
                continue;
            }
            let tick = self.clock.step();
            let actions = std::mem::take(&mut self.queued_actions);
            self.run_tick(tick, controls.clone(), actions);
            new_frames += 1;
        }
        if new_frames > 0 {
            let datagram = self.build_inputs(new_frames);
            self.outbox.push(datagram);
        }
        self.smoothing.decay(dt, self.tuning.smoothing_half_life_ms);
        let delay = self.interp_delay_ticks();
        self.interp
            .advance(dt, self.now, delay, &self.params, &self.tuning);
        if let Some(time) = self.interp.time() {
            self.remote.prune_before(time - 1.0);
        }
        self.take_outbound()
    }

    fn run_tick(&mut self, tick: Tick, controls: S::Controls, actions: Vec<S::Action>) {
        let seq = self.next_seq;
        self.next_seq = seq.next();
        let mut envelopes = Vec::with_capacity(actions.len());
        for action in actions {
            let id = self.next_action;
            self.next_action = id.next();
            let envelope = ActionEnvelope { id, action };
            self.unacked_actions.push_back(TaggedAction {
                seq,
                target_tick: tick,
                envelope: envelope.clone(),
            });
            envelopes.push(envelope);
        }
        let frame: InputFrame<S> = InputFrame {
            seq,
            target_tick: tick,
            controls,
            actions: envelopes,
        };
        if let (Some(predicted), Some(local)) = (self.predicted.as_mut(), self.player) {
            // Fill ticks the client never sent input for (join / resync gaps).
            while predicted.tick().next() < tick {
                let gap: TickInputs<S::Controls, S::Action> =
                    TickInputs::empty(predicted.tick().next());
                let _ = predicted.simulate_tick(&gap);
            }
            if predicted.tick().next() == tick {
                let mut inputs: TickInputs<S::Controls, S::Action> = TickInputs::empty(tick);
                inputs.players.insert(
                    local,
                    PlayerTickInput {
                        controls: frame.controls.clone(),
                        actions: predicted_actions::<S>(&frame.actions),
                    },
                );
                let output = predicted.simulate_tick(&inputs);
                self.stats.predicted_ticks += 1;
                let seqs = BTreeMap::from([(local, seq)]);
                for event in tag_events(tick, output.events, &seqs) {
                    if event.player != Some(local) {
                        continue;
                    }
                    self.predicted_events
                        .insert((local, tick, event.kind, event.ordinal), seq);
                    self.events.push(PresentedEvent {
                        source: EventSource::Predicted,
                        event,
                    });
                }
                self.prev_pose = self.cur_pose;
                self.cur_pose = predicted.player_pose(local);
            }
        }
        self.pending.push_back(frame);
        self.stats.max_pending = self.stats.max_pending.max(self.pending.len());
    }

    fn build_inputs(&mut self, new_frames: usize) -> ClientMessage<S> {
        self.stats.input_datagrams += 1;
        let count = usize::try_from(self.tuning.input_redundancy)
            .unwrap_or(usize::MAX)
            .max(1)
            .max(new_frames);
        let ack = self.ack_seq;
        let mut frames: Vec<InputFrame<S>> = self
            .pending
            .iter()
            .rev()
            .filter(|frame| ack.is_none_or(|a| frame.seq > a))
            .take(count)
            .cloned()
            .collect();
        frames.reverse();
        let carried: BTreeSet<InputSeq> = frames.iter().map(|frame| frame.seq).collect();
        let actions = self
            .unacked_actions
            .iter()
            .filter(|tagged| !carried.contains(&tagged.seq))
            .cloned()
            .collect();
        let action_floor = self
            .unacked_actions
            .front()
            .map_or(self.next_action, |tagged| tagged.envelope.id);
        ClientMessage::Inputs {
            epoch: self.epoch,
            frames,
            actions,
            action_floor,
        }
    }

    fn initial_lead(&self) -> u64 {
        let lead_ms = self
            .rtt
            .map_or(0, |rtt| u32::try_from(rtt.as_millis()).unwrap_or(u32::MAX));
        self.params
            .ms_to_ticks_ceil(lead_ms.saturating_add(self.tuning.input_lead_target_ms))
            + 1
    }

    // ----- presentation ------------------------------------------------------

    /// Builds the render view.
    #[must_use]
    pub fn view(&self) -> ClientView<'_, S> {
        let alpha = self.clock.alpha();
        let local_pose = match (self.prev_pose, self.cur_pose) {
            (Some(prev), Some(cur)) => Some(prev.lerp(&cur, alpha)),
            (None, cur) => cur,
            (prev, None) => prev,
        }
        .map(|pose| pose.translated(self.smoothing.x, self.smoothing.y));
        let max_extrapolation = self
            .params
            .ms_to_ticks_f64(f64::from(self.tuning.max_extrapolation_ms));
        let remote_poses = self.interp.time().map_or_else(BTreeMap::new, |time| {
            self.remote
                .players()
                .filter_map(|player| {
                    self.remote
                        .sample(player, time, max_extrapolation)
                        .map(|pose| (player, pose))
                })
                .collect()
        });
        ClientView {
            phase: self.phase,
            local_player: self.player,
            local_pose,
            remote_poses,
            far_players: &self.far_players,
            predicted: self.predicted.as_ref(),
            authority: self.authority.as_ref(),
            client_tick: self.clock.tick(),
            server_tick: self.newest_server_tick,
            interp_time: self.interp.time(),
            smoothing: self.smoothing,
        }
    }
}

/// Applies one frame's replicated sections to `authority` and returns the
/// players whose per-tick state was included.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn apply_replication<S: NetSimulation>(
    authority: &mut S,
    tick: Tick,
    roster: Option<Vec<PlayerId>>,
    players: Vec<(PlayerId, S::PlayerState)>,
    entities: S::Entities,
    ops: &[(Tick, S::Op)],
    chunk_refreshes: Vec<(S::Chunk, S::ChunkSnapshot)>,
    sections: Vec<(S::SectionKey, S::Section)>,
    removed_sections: &[S::SectionKey],
) -> Vec<PlayerId> {
    authority.set_tick(tick);
    if let Some(roster) = roster {
        let wanted: BTreeSet<PlayerId> = roster.into_iter().collect();
        for player in authority.player_ids() {
            if !wanted.contains(&player) {
                authority.remove_player(player);
            }
        }
        for player in &wanted {
            authority.add_player(*player);
        }
    }
    let relevant: Vec<PlayerId> = players.iter().map(|(player, _)| *player).collect();
    for (player, state) in players {
        authority.apply_player_state(player, state);
    }
    authority.apply_entities(entities);
    for (_, op) in ops {
        authority.apply_op(op);
    }
    for (chunk, snapshot) in chunk_refreshes {
        authority.apply_chunk_snapshot(&chunk, snapshot);
    }
    for (key, section) in sections {
        authority.apply_section(&key, section);
    }
    for key in removed_sections {
        authority.remove_section(key);
    }
    relevant
}

fn predicted_actions<S: NetSimulation>(
    actions: &[ActionEnvelope<S::Action>],
) -> Vec<ActionEnvelope<S::Action>> {
    actions
        .iter()
        .filter(|envelope| S::predicts_action(&envelope.action))
        .cloned()
        .collect()
}
