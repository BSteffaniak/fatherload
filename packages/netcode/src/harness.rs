//! End-to-end harness: one [`HostSession`] and N [`ClientSession`]s over
//! seeded [`LinkSim`]s, driven by scripted bots (plan §11.2).
//!
//! Every message crosses the real codec, so the harness measures true wire
//! sizes and proves the encode/decode path too.
//!
//! This is test-support code: it panics (with context) on any codec failure
//! or lack of progress, which is exactly what a test wants.
#![allow(clippy::missing_panics_doc)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Duration;

use crate::client::{ClientConfig, ClientPhase, ClientSession, ClientStats};
use crate::host::{HostConfig, HostSession};
use crate::ids::{ActionId, ClientId, PlayerId, ReconnectToken, Tick};
use crate::link::{DetRng, LinkConditions, LinkSim};
use crate::params::{NetTuning, SessionParams};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::simulation::NetSimulation;
use crate::toy::{ToyAction, ToyControls, ToyEvent, ToySim};

/// Scripted bot: dig down, dig sideways, fly up, idle, with periodic bombs,
/// sells and buys.
#[derive(Clone, Debug)]
pub struct Bot {
    rng: DetRng,
    frame: u64,
    phase_offset: u64,
    /// Whether the bot issues actions.
    pub issue_actions: bool,
    /// Whether the bot moves at all.
    pub active: bool,
}

impl Bot {
    /// Creates a bot.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        let mut rng = DetRng::new(seed);
        let phase_offset = rng.below(600);
        Self {
            rng,
            frame: 0,
            phase_offset,
            issue_actions: true,
            active: true,
        }
    }

    /// Controls and actions for the next render frame.
    pub fn decide(&mut self) -> (ToyControls, Vec<ToyAction>) {
        self.frame += 1;
        if !self.active {
            return (ToyControls::default(), Vec::new());
        }
        let t = (self.frame + self.phase_offset) % 600;
        let controls = match t {
            0..250 => ToyControls {
                move_x: 0,
                move_y: 1,
                drill: true,
            },
            250..290 => ToyControls {
                move_x: 1,
                move_y: 0,
                drill: true,
            },
            290..330 => ToyControls {
                move_x: -1,
                move_y: 0,
                drill: true,
            },
            330..560 => ToyControls {
                move_x: 0,
                move_y: -1,
                drill: false,
            },
            _ => ToyControls::default(),
        };
        let mut actions = Vec::new();
        if self.issue_actions && self.rng.chance(1.0 / 90.0) {
            actions.push(match self.rng.below(3) {
                0 => ToyAction::Bomb,
                1 => ToyAction::Sell,
                _ => ToyAction::Buy,
            });
        }
        (controls, actions)
    }
}

/// Harness configuration.
#[derive(Clone, Debug)]
pub struct HarnessConfig {
    /// Session parameters.
    pub params: SessionParams,
    /// Tuning for host and clients.
    pub tuning: NetTuning,
    /// Clients to connect at start.
    pub clients: usize,
    /// Client-to-host link.
    pub uplink: LinkConditions,
    /// Host-to-client link.
    pub downlink: LinkConditions,
    /// Render frame duration range (seeded uniform).
    pub frame_dt: (Duration, Duration),
    /// Master seed.
    pub seed: u64,
    /// Start in the lobby.
    pub start_in_lobby: bool,
    /// World columns (toy world width / 8).
    pub columns: u32,
    /// Whether clients receive an RTT estimate before joining.
    pub provide_rtt: bool,
}

impl HarnessConfig {
    /// A config with symmetric link conditions.
    #[must_use]
    pub fn new(params: SessionParams, clients: usize, link: LinkConditions) -> Self {
        let tuning = NetTuning {
            debug_hash_interval: 1,
            ..NetTuning::default()
        };
        Self {
            params,
            tuning,
            clients,
            uplink: link,
            downlink: link,
            frame_dt: (Duration::from_millis(6), Duration::from_millis(17)),
            seed: 1,
            start_in_lobby: false,
            columns: u32::try_from(clients + 2).unwrap_or(u32::MAX).max(4),
            provide_rtt: true,
        }
    }
}

/// One simulated remote client.
#[derive(Debug)]
pub struct HarnessClient {
    /// Transport id.
    pub id: ClientId,
    /// The session under test.
    pub session: ClientSession<ToySim>,
    /// Datagram uplink (inputs).
    pub up_unreliable: LinkSim<Vec<u8>>,
    /// Stream uplink.
    pub up_reliable: LinkSim<Vec<u8>>,
    /// Stream downlink.
    pub down: LinkSim<Vec<u8>>,
    /// The bot driving it.
    pub bot: Bot,
    /// Whether the transport is connected.
    pub connected: bool,
    /// Bytes received from the host.
    pub bytes_down: u64,
    /// Bytes sent to the host.
    pub bytes_up: u64,
    /// Largest single frame received.
    pub max_frame_bytes: usize,
    /// Server frames received.
    pub frames_down: u64,
    /// Authority hash compared with the host's hash at the same tick.
    pub hash_compares: u64,
    /// Mismatches in those comparisons.
    pub hash_mismatches: u64,
    /// Input margins observed after warm-up.
    pub margins: Vec<i64>,
    /// Session stats at the last measurement reset.
    pub baseline: ClientStats,
}

/// Aggregate results.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HarnessSummary {
    /// Host ticks.
    pub ticks: u64,
    /// Simulated seconds.
    pub seconds: f64,
    /// Total corrections over all clients.
    pub corrections: u64,
    /// Total snaps.
    pub snaps: u64,
    /// Mean correction magnitude.
    pub mean_correction: f64,
    /// Max correction magnitude.
    pub max_correction: f32,
    /// Corrections per client per second.
    pub corrections_per_client_sec: f64,
    /// Mean downlink bytes per client per second.
    pub bytes_down_per_client_sec: f64,
    /// Mean uplink bytes per client per second.
    pub bytes_up_per_client_sec: f64,
    /// Largest single server frame.
    pub max_frame_bytes: usize,
    /// Harness-side hash comparisons and mismatches.
    pub hash_compares: u64,
    /// Harness-side hash mismatches.
    pub hash_mismatches: u64,
    /// Client-side debug hash checks.
    pub debug_hash_checks: u64,
    /// Client-side debug hash mismatches.
    pub debug_hash_mismatches: u64,
    /// Largest pending queue seen by any client.
    pub max_pending: usize,
    /// Clock resyncs over all clients.
    pub clock_resyncs: u64,
    /// Mean prediction depth (ticks replayed per reconciliation) in ms: how far
    /// the client runs ahead of confirmed state, i.e. perceived input latency
    /// hidden by prediction.
    pub mean_prediction_ms: f64,
}

/// The harness.
#[derive(Debug)]
pub struct Harness {
    /// Configuration.
    pub config: HarnessConfig,
    /// The host.
    pub host: HostSession<ToySim>,
    /// Remote clients.
    pub clients: Vec<HarnessClient>,
    /// Local time.
    pub now: Duration,
    rng: DetRng,
    host_hashes: BTreeMap<Tick, u64>,
    /// `(player, action id) -> times applied` from host events.
    pub applied_actions: BTreeMap<(PlayerId, ActionId), u32>,
    next_client: u64,
    start_tick: Tick,
    start_time: Duration,
}

impl Harness {
    /// Builds the host and connects `config.clients` clients.
    #[must_use]
    pub fn new(config: HarnessConfig) -> Self {
        let world = ToySim::new(&config.params, config.columns, 48);
        let host_config = HostConfig {
            params: config.params,
            tuning: config.tuning,
            start_in_lobby: config.start_in_lobby,
            record_tick_hashes: true,
            token_seed: config.seed ^ 0xabcd,
            ..HostConfig::default()
        };
        let host = HostSession::new(world, host_config);
        let rng = DetRng::new(config.seed);
        let mut harness = Self {
            config,
            host,
            clients: Vec::new(),
            now: Duration::ZERO,
            rng,
            host_hashes: BTreeMap::new(),
            applied_actions: BTreeMap::new(),
            next_client: 1,
            start_tick: Tick(0),
            start_time: Duration::ZERO,
        };
        for _ in 0..harness.config.clients {
            harness.add_client(None);
        }
        harness
    }

    /// Connects a new client (optionally reclaiming a player with `token`).
    /// Returns its index in `clients`.
    pub fn add_client(&mut self, token: Option<ReconnectToken>) -> usize {
        let id = ClientId(self.next_client);
        self.next_client += 1;
        let seed = self
            .config
            .seed
            .wrapping_mul(0x9e37_79b9)
            .wrapping_add(id.0 * 7919);
        let rtt = self.config.uplink.latency + self.config.downlink.latency;
        let session = ClientSession::new(ClientConfig {
            name: format!("bot-{}", id.0),
            reconnect_token: token,
            initial_rtt: self.config.provide_rtt.then_some(rtt),
        });
        let mut client = HarnessClient {
            id,
            session,
            up_unreliable: LinkSim::new(self.config.uplink, false, seed ^ 1),
            up_reliable: LinkSim::new(self.config.uplink, true, seed ^ 2),
            down: LinkSim::new(self.config.downlink, true, seed ^ 3),
            bot: Bot::new(seed ^ 4),
            connected: true,
            bytes_down: 0,
            bytes_up: 0,
            max_frame_bytes: 0,
            frames_down: 0,
            hash_compares: 0,
            hash_mismatches: 0,
            margins: Vec::new(),
            baseline: ClientStats::default(),
        };
        self.host.client_connected(id);
        let hello = client.session.take_outbound();
        send_client_messages(&mut client, self.now, hello);
        self.clients.push(client);
        self.clients.len() - 1
    }

    /// Drops a client's transport (host sees a disconnect).
    pub fn disconnect(&mut self, index: usize) {
        let client = &mut self.clients[index];
        client.connected = false;
        client.up_reliable.clear();
        client.up_unreliable.clear();
        client.down.clear();
        self.host.client_disconnected(client.id);
    }

    /// Advances by one render frame of seeded random length.
    pub fn step(&mut self) {
        let (min, max) = self.config.frame_dt;
        let span = u64::try_from(max.saturating_sub(min).as_micros()).unwrap_or(0);
        let dt = min + Duration::from_micros(self.rng.below(span + 1));
        self.step_dt(dt);
    }

    /// Advances by `dt`.
    pub fn step_dt(&mut self, dt: Duration) {
        self.now += dt;
        let now = self.now;

        for client in &mut self.clients {
            if !client.connected {
                continue;
            }
            let mut inbound = client.up_reliable.poll(now);
            inbound.extend(client.up_unreliable.poll(now));
            for bytes in inbound {
                self.host
                    .handle_client_bytes(client.id, &bytes)
                    .expect("host decodes client bytes");
            }
        }

        let output = self.host.advance(dt);
        for (tick, hash) in output.tick_hashes {
            self.host_hashes.insert(tick, hash);
        }
        for event in &output.events {
            if let (Some(player), ToyEvent::ActionApplied { id, .. }) =
                (event.player, event.payload)
            {
                *self.applied_actions.entry((player, id)).or_insert(0) += 1;
            }
        }
        let horizon = self
            .host
            .tick()
            .0
            .saturating_sub(self.config.params.rate() * 20);
        self.host_hashes = self.host_hashes.split_off(&Tick(horizon));

        for (client_id, message) in output.outbound {
            if let Some(client) = self
                .clients
                .iter_mut()
                .find(|c| c.id == client_id && c.connected)
            {
                let bytes = protocol::encode(&message).expect("encode server message");
                client.bytes_down += bytes.len() as u64;
                if matches!(message, ServerMessage::Frame(_)) {
                    client.max_frame_bytes = client.max_frame_bytes.max(bytes.len());
                }
                client.down.send(now, bytes);
            }
        }

        let warm_ticks = self.config.params.rate() * 3;
        let host_tick = self.host.tick();
        for client in &mut self.clients {
            if !client.connected {
                continue;
            }
            for bytes in client.down.poll(now) {
                let message: ServerMessage<ToySim> =
                    protocol::decode(&bytes).expect("client decodes server bytes");
                let is_frame = matches!(message, ServerMessage::Frame(_));
                client.session.handle_server_message(message);
                if is_frame {
                    client.frames_down += 1;
                    if let Some(authority) = client.session.authority()
                        && let Some(expected) = self.host_hashes.get(&authority.tick())
                    {
                        client.hash_compares += 1;
                        if authority.state_hash() != *expected {
                            client.hash_mismatches += 1;
                        }
                    }
                    if host_tick.0 > self.start_tick.0 + warm_ticks
                        && let Some(margin) = client.session.stats().last_margin
                    {
                        client.margins.push(margin);
                    }
                }
            }
            let (controls, actions) = client.bot.decide();
            let messages = client.session.advance(dt, &controls, actions);
            send_client_messages(client, now, messages);
        }
    }

    /// Runs until the host reaches `ticks` more ticks.
    pub fn run_ticks(&mut self, ticks: u64) {
        let target = self.host.tick().0 + ticks;
        let mut guard = 0u64;
        while self.host.tick().0 < target {
            self.step();
            guard += 1;
            assert!(guard < ticks * 20 + 10_000, "harness made no progress");
        }
    }

    /// Runs for a simulated duration.
    pub fn run_for(&mut self, duration: Duration) {
        let end = self.now + duration;
        while self.now < end {
            self.step();
        }
    }

    /// Stops bot actions and runs long enough for every in-flight action to
    /// be applied and acknowledged.
    pub fn settle(&mut self, duration: Duration) {
        for client in &mut self.clients {
            client.bot.issue_actions = false;
        }
        self.run_for(duration);
    }

    /// Resets measurement baselines (call after warm-up).
    pub fn reset_measurements(&mut self) {
        self.start_tick = self.host.tick();
        self.start_time = self.now;
    }

    /// Asserts every action issued by connected clients was applied exactly
    /// once by the host.
    ///
    /// # Panics
    ///
    /// Panics on a missing or duplicated action.
    pub fn assert_actions_exactly_once(&self) {
        for (player, count) in self.applied_actions.iter().map(|((p, _), c)| (p, c)) {
            assert_eq!(
                *count, 1,
                "player {player} had an action applied {count} times"
            );
        }
        for client in &self.clients {
            if !client.connected {
                continue;
            }
            let Some(player) = client.session.player() else {
                continue;
            };
            let issued = client.session.actions_issued();
            for id in 1..=issued {
                assert_eq!(
                    self.applied_actions.get(&(player, ActionId(id))),
                    Some(&1),
                    "client {} action {id} of {issued} not applied exactly once",
                    client.id
                );
            }
            assert_eq!(
                client.session.unacked_actions_len(),
                0,
                "actions left unacked"
            );
        }
    }

    /// Aggregates results since the last `reset_measurements`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn summary(&self) -> HarnessSummary {
        let seconds = self
            .now
            .saturating_sub(self.start_time)
            .as_secs_f64()
            .max(1e-9);
        let connected: Vec<&HarnessClient> = self.clients.iter().filter(|c| c.connected).collect();
        let n = connected.len().max(1) as f64;
        let mut summary = HarnessSummary {
            ticks: self.host.tick().0 - self.start_tick.0,
            seconds,
            ..HarnessSummary::default()
        };
        let mut total = 0.0;
        let mut replayed = 0u64;
        let mut frames = 0u64;
        for client in &connected {
            let stats = client.session.stats();
            summary.corrections += stats.corrections - client.baseline.corrections;
            summary.snaps += stats.snaps - client.baseline.snaps;
            total += stats.correction_total - client.baseline.correction_total;
            summary.max_correction = summary.max_correction.max(stats.correction_max);
            summary.bytes_down_per_client_sec += client.bytes_down as f64 / seconds / n;
            summary.bytes_up_per_client_sec += client.bytes_up as f64 / seconds / n;
            summary.max_frame_bytes = summary.max_frame_bytes.max(client.max_frame_bytes);
            summary.hash_compares += client.hash_compares;
            summary.hash_mismatches += client.hash_mismatches;
            summary.debug_hash_checks += stats.hash_checks - client.baseline.hash_checks;
            summary.debug_hash_mismatches +=
                stats.hash_mismatches - client.baseline.hash_mismatches;
            summary.max_pending = summary.max_pending.max(stats.max_pending);
            summary.clock_resyncs += stats.clock_resyncs - client.baseline.clock_resyncs;
            replayed += stats.replayed_ticks - client.baseline.replayed_ticks;
            frames += stats.frames_received - client.baseline.frames_received;
        }
        if frames > 0 {
            let ticks = replayed as f64 / frames as f64;
            summary.mean_prediction_ms = ticks * 1000.0 / self.config.params.rate() as f64;
        }
        summary.mean_correction = if summary.corrections == 0 {
            0.0
        } else {
            total / summary.corrections as f64
        };
        summary.corrections_per_client_sec = summary.corrections as f64 / seconds / n;
        summary
    }

    /// Starts a new measurement window for `summary` (byte counters, margins
    /// and session-stat baselines; `max_correction` stays lifetime).
    pub fn reset_client_counters(&mut self) {
        for client in &mut self.clients {
            client.bytes_down = 0;
            client.bytes_up = 0;
            client.margins.clear();
            client.baseline = client.session.stats().clone();
        }
        self.reset_measurements();
    }

    /// Whether every connected client is in game.
    #[must_use]
    pub fn all_in_game(&self) -> bool {
        self.clients
            .iter()
            .filter(|c| c.connected)
            .all(|c| c.session.phase() == ClientPhase::InGame)
    }

    /// The host world.
    #[must_use]
    pub const fn world(&self) -> &ToySim {
        self.host.world()
    }

    /// Hash of the host world at a recorded tick.
    #[must_use]
    pub fn host_hash(&self, tick: Tick) -> Option<u64> {
        self.host_hashes.get(&tick).copied()
    }

    /// Current host tick.
    #[must_use]
    pub fn host_tick(&self) -> Tick {
        self.host.world().tick()
    }
}

fn send_client_messages(
    client: &mut HarnessClient,
    now: Duration,
    messages: Vec<ClientMessage<ToySim>>,
) {
    for message in messages {
        let bytes = protocol::encode(&message).expect("encode client message");
        client.bytes_up += bytes.len() as u64;
        if message.is_unreliable() {
            client.up_unreliable.send(now, bytes);
        } else {
            client.up_reliable.send(now, bytes);
        }
    }
}

/// One row of the parameter sweep.
#[derive(Clone, Debug)]
pub struct SweepRow {
    /// Tick rate.
    pub tick_rate: u32,
    /// Snapshot rate.
    pub snapshot_rate: u32,
    /// Link label.
    pub link: &'static str,
    /// Results.
    pub summary: HarnessSummary,
}

/// Named link presets used by the sweep.
#[must_use]
pub fn sweep_links() -> Vec<(&'static str, LinkConditions)> {
    vec![
        ("lan 2ms", LinkConditions::new(2, 1, 0.0)),
        (
            "net 40±8ms 1%",
            LinkConditions::new(40, 8, 1.0).with_chaos(0.5, 1.0),
        ),
        (
            "bad 75±10ms 3%",
            LinkConditions::new(75, 10, 3.0).with_chaos(1.0, 2.0),
        ),
        (
            "awful 150±20ms 3%",
            LinkConditions::new(150, 20, 3.0).with_chaos(2.0, 5.0),
        ),
    ]
}

/// Runs the tick rate x snapshot rate x link grid for `seconds` each.
#[must_use]
pub fn parameter_sweep(
    tick_rates: &[u32],
    snapshot_rates: &[u32],
    links: &[(&'static str, LinkConditions)],
    clients: usize,
    seconds: u64,
) -> Vec<SweepRow> {
    let mut rows = Vec::new();
    for &tick_rate in tick_rates {
        for &snapshot_rate in snapshot_rates {
            if snapshot_rate > tick_rate {
                continue;
            }
            for (label, link) in links {
                let mut config = HarnessConfig::new(SessionParams::new(tick_rate), clients, *link);
                config.tuning.snapshot_rate_hz = snapshot_rate;
                config.tuning.debug_hash_interval = 0;
                let mut harness = Harness::new(config);
                harness.run_for(Duration::from_secs(3));
                harness.reset_client_counters();
                harness.run_for(Duration::from_secs(seconds));
                let summary = harness.summary();
                rows.push(SweepRow {
                    tick_rate,
                    snapshot_rate,
                    link: label,
                    summary,
                });
            }
        }
    }
    rows
}

/// Formats sweep rows as a table.
#[must_use]
pub fn format_sweep(rows: &[SweepRow]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:>5} {:>5} {:<18} {:>9} {:>9} {:>9} {:>6} {:>10} {:>9} {:>8} {:>8}",
        "tick",
        "snap",
        "link",
        "corr/s",
        "mean",
        "max",
        "snaps",
        "down B/s",
        "up B/s",
        "maxfrm",
        "pred ms"
    );
    for row in rows {
        let s = &row.summary;
        let _ = writeln!(
            out,
            "{:>5} {:>5} {:<18} {:>9.3} {:>9.4} {:>9.4} {:>6} {:>10.0} {:>9.0} {:>8} {:>8.1}",
            row.tick_rate,
            row.snapshot_rate,
            row.link,
            s.corrections_per_client_sec,
            s.mean_correction,
            s.max_correction,
            s.snaps,
            s.bytes_down_per_client_sec,
            s.bytes_up_per_client_sec,
            s.max_frame_bytes,
            s.mean_prediction_ms
        );
    }
    out
}
