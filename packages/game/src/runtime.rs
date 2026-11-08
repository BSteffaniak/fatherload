//! The app's gameplay authority (plan §10).
//!
//! Single-player, local split-screen and online hosting all run on the same [`HostSession`] the
//! netcode harness uses: local seats plus zero or more remote clients (through
//! [`HostNet`]). A joined online client runs [`OnlineClient`] (a `ClientSession`). There is no
//! other gameplay path: the shell submits held controls and one-shot actions per seat, the
//! runtime advances fixed ticks from real time, and rendering reads the world it exposes.

use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

use drillgame_netcode::{
    ClientPhase, EndReason, HostConfig, HostPhase, HostSession, InterpDelay, LobbyEntry, NetEvent,
    NetTuning, SeatId, SessionParams,
    world::{ActionResolution, WorldEvent, from_sim_player, to_sim_player},
};
use drillgame_simulation::{
    ControlState, GameplayEvent, PlayerCommand, PlayerId, TileChange, WorldState,
    terrain::TilePosition,
};

use crate::online::{
    client::{ClientLink, OnlineClient},
    host::{HostNet, HostNetStatus},
    random_token_seed,
};

/// Most local seats (split-screen views) the shell will add. Seats, views and layout are generic
/// over any count; this is the only place the cap is enforced.
pub const MAX_LOCAL_SEATS: usize = 4;

/// Full-world debug hash interval (frames) sent to clients in dev builds.
const DEV_DEBUG_HASH_INTERVAL: u32 = 30;

/// A local seat and the player it controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSeat {
    /// The seat (one per local input device group / split-screen view).
    pub seat: SeatId,
    /// The world player bound to the seat.
    pub player: PlayerId,
}

/// One seat's gameplay input for the next tick(s).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatInput {
    /// Held, quantized movement controls.
    pub controls: ControlState,
    /// One-shot actions (bombs, scanner, infrastructure, services, rescue).
    pub actions: Vec<PlayerCommand>,
}

/// A gameplay event produced by a fixed tick, with its stable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeEvent {
    /// Session epoch the event belongs to (bumps on load / new game).
    pub epoch: u64,
    /// Tick that produced the event.
    pub tick: u64,
    /// Owning player, if any.
    pub player: Option<PlayerId>,
    /// Stable event-kind discriminant.
    pub kind: u32,
    /// Index among events with the same `(player, kind)` on that tick.
    pub ordinal: u32,
    /// The event itself.
    pub event: RuntimeEventKind,
}

impl RuntimeEvent {
    /// Identity used to present each event exactly once.
    #[must_use]
    pub const fn key(&self) -> (u64, u64, Option<u64>, u32, u32) {
        let player = match self.player {
            Some(player) => Some(player.get()),
            None => None,
        };
        (self.epoch, self.tick, player, self.kind, self.ordinal)
    }
}

/// What a runtime event says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEventKind {
    /// A gameplay event (audio/vfx cue, notification).
    Gameplay(GameplayEvent),
    /// A submitted action and whether it changed the world.
    ActionResolved {
        /// The action.
        command: PlayerCommand,
        /// Whether the world applied it.
        applied: bool,
    },
}

/// What one [`Runtime::advance`] produced.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeFrame {
    /// Fixed ticks simulated this frame.
    pub ticks_run: u32,
    /// Gameplay events from those ticks, in order.
    pub events: Vec<RuntimeEvent>,
    /// Terrain tiles changed by those ticks, in order (may repeat).
    pub changed_tiles: Vec<TilePosition>,
    /// The rendered world was replaced wholesale (a client's first or new keyframe).
    pub world_replaced: bool,
}

/// Which side of an online session this runtime is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnlineRole {
    /// Hosting remote clients.
    Host,
    /// Joined someone else's session.
    Client,
}

/// Where a joined client's session stands (drives the shell's lobby and notices).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientState {
    /// Connecting / waiting for the welcome.
    Connecting,
    /// In the host's lobby.
    Lobby,
    /// Playing.
    InGame,
    /// The session is over (host ended it, rejected us, or the connection closed).
    Finished {
        /// What to tell the player.
        message: String,
        /// Whether reconnecting with the token makes sense.
        can_reconnect: bool,
    },
}

/// Lobby facts for the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LobbyInfo {
    /// Players, with names and ready flags.
    pub roster: Vec<LobbyEntry>,
    /// Host address peers connect to.
    pub addr: Option<SocketAddr>,
    /// Host descriptor JSON (pinned certificate).
    pub descriptor: Option<String>,
    /// Advertised on the LAN.
    pub lan_published: bool,
    /// The game has started.
    pub started: bool,
    /// One-line transport status.
    pub status: String,
    /// The roster entry that is this app's (first) player.
    pub local_player: Option<drillgame_netcode::PlayerId>,
}

/// The gameplay authority the app runs on.
#[derive(Debug)]
pub enum Runtime {
    /// Single-player / split-screen / online host.
    Host(Box<LocalHost>),
    /// Joined online client.
    Client(Box<OnlineClient>),
}

impl Runtime {
    /// A local host for `world` with `seats` local players (at least one, at most
    /// [`MAX_LOCAL_SEATS`]).
    #[must_use]
    pub fn local(world: WorldState, seats: usize) -> Self {
        Self::Host(Box::new(LocalHost::new(world, seats)))
    }

    /// The world rendering and the shell read (a client's predicted world).
    #[must_use]
    pub fn world(&self) -> &WorldState {
        match self {
            Self::Host(host) => host.world(),
            Self::Client(client) => client.world(),
        }
    }

    /// Local seats in seat order.
    #[must_use]
    pub fn local_seats(&self) -> &[LocalSeat] {
        match self {
            Self::Host(host) => host.seats(),
            Self::Client(client) => client.seats(),
        }
    }

    /// Current session epoch.
    #[cfg(test)]
    #[must_use]
    pub fn epoch(&self) -> u64 {
        match self {
            Self::Host(host) => host.epoch(),
            Self::Client(client) => client.session().epoch().0,
        }
    }

    /// Online role, or `None` for purely local play.
    #[must_use]
    pub const fn online_role(&self) -> Option<OnlineRole> {
        match self {
            Self::Host(host) => {
                if host.net.is_some() {
                    Some(OnlineRole::Host)
                } else {
                    None
                }
            }
            Self::Client(_) => Some(OnlineRole::Client),
        }
    }

    /// Adds another local seat. Returns `None` at [`MAX_LOCAL_SEATS`] or on a joined client.
    pub fn add_local_seat(&mut self) -> Option<LocalSeat> {
        match self {
            Self::Host(host) => host.add_seat(),
            Self::Client(_) => None,
        }
    }

    /// Queues one seat's input for the next tick(s). Controls are held until replaced; actions
    /// are applied exactly once.
    pub fn submit(&mut self, seat: SeatId, input: SeatInput) {
        match self {
            Self::Host(host) => host.submit(seat, input),
            Self::Client(client) => {
                if seat == SeatId(0) {
                    client.submit(input.controls, input.actions);
                }
            }
        }
    }

    /// Stops (or resumes) ticking. Only a purely local host pauses: online play keeps running.
    pub fn set_paused(&mut self, paused: bool) {
        if let Self::Host(host) = self {
            host.set_paused(paused && host.net.is_none());
        }
    }

    /// Drains network events into the session (frame step 1). Never blocks.
    pub fn pump_network(&mut self, delta_seconds: f32) {
        match self {
            Self::Host(host) => host.pump(delta_seconds),
            Self::Client(client) => client.pump(delta_seconds),
        }
    }

    /// Advances real time, running 0..n fixed ticks, and sends what the session queued.
    pub fn advance(&mut self, dt: Duration) -> RuntimeFrame {
        match self {
            Self::Host(host) => host.advance(dt),
            Self::Client(client) => client.advance(dt),
        }
    }

    /// Whether this runtime owns the world and may write saves.
    #[must_use]
    pub const fn can_save(&self) -> bool {
        matches!(self, Self::Host(_))
    }

    /// Replaces the world (loading a save) and starts a new epoch. Returns `false` when this
    /// runtime cannot load (a joined client).
    pub fn load_world(&mut self, world: WorldState) -> bool {
        match self {
            Self::Host(host) => host.load_world(world),
            Self::Client(_) => false,
        }
    }

    /// Presentation pose overrides in world pixels (a client's predicted/smoothed local rig and
    /// interpolated remote rigs). Empty for a host, which renders its authoritative world.
    #[must_use]
    pub fn pose_overrides(&self) -> BTreeMap<PlayerId, (f32, f32)> {
        match self {
            Self::Host(_) => BTreeMap::new(),
            Self::Client(client) => client.pose_overrides(),
        }
    }

    /// Lobby facts, for online sessions.
    #[must_use]
    pub fn lobby_info(&self) -> Option<LobbyInfo> {
        match self {
            Self::Host(host) => host.lobby_info(),
            Self::Client(client) => Some(client_lobby_info(client)),
        }
    }

    /// A joined client's session state.
    #[must_use]
    pub fn client_state(&self) -> Option<ClientState> {
        match self {
            Self::Host(_) => None,
            Self::Client(client) => Some(client_state(client)),
        }
    }

    /// Network notices (errors, disconnects) since the last call.
    pub fn take_notices(&mut self) -> Vec<String> {
        match self {
            Self::Host(host) => host
                .net
                .as_mut()
                .map(HostNet::take_notices)
                .unwrap_or_default(),
            Self::Client(client) => client.take_notices(),
        }
    }

    /// Client: sets the lobby ready flag.
    pub fn set_ready(&mut self, ready: bool) {
        if let Self::Client(client) = self {
            client.set_ready(ready);
        }
    }

    /// Host: starts the online game from the lobby.
    pub fn start_online_game(&mut self) {
        if let Self::Host(host) = self {
            host.start_game();
        }
    }

    /// Dev: steps one live tuning value on the host (applied to every client). Returns what
    /// changed, or why nothing did.
    pub fn adjust_tuning(&mut self, parameter: usize, step: i32) -> String {
        match self {
            Self::Host(host) if host.net.is_some() => host.adjust_tuning(parameter, step),
            Self::Host(_) => "Live tuning needs an online session.".to_owned(),
            Self::Client(_) => "Network tuning is chosen by the host; tune it there.".to_owned(),
        }
    }

    /// Why hosting failed (the endpoint could not be bound), if it did.
    #[must_use]
    pub fn host_failure(&self) -> Option<String> {
        match self {
            Self::Host(host) => match host.net.as_ref().map(HostNet::status) {
                Some(HostNetStatus::Failed(error)) => Some(error.clone()),
                _ => None,
            },
            Self::Client(_) => None,
        }
    }

    /// Lines for the F3 network debug overlay.
    #[must_use]
    pub fn net_debug_lines(&self) -> Vec<String> {
        match self {
            Self::Host(host) => host.net_debug_lines(),
            Self::Client(client) => client_debug_lines(client),
        }
    }
}

/// [`HostSession`] over the real world with local seats and, when online, remote clients.
#[derive(Debug)]
pub struct LocalHost {
    session: HostSession<WorldState>,
    seats: Vec<LocalSeat>,
    net: Option<HostNet>,
}

impl LocalHost {
    /// Hosts `world` with `seats` local players. Existing world players are adopted by the first
    /// seats in id order (so a loaded save keeps its rigs); further seats spawn new players and
    /// world players beyond the seat count are dropped.
    #[must_use]
    pub fn new(world: WorldState, seats: usize) -> Self {
        Self::with_lobby(world, seats, false)
    }

    /// Hosts `world` online: binds `bind` (optionally advertising on the LAN) and waits in the
    /// lobby for [`LocalHost::start_game`]. `seats` local players play beside remote clients.
    ///
    /// # Errors
    ///
    /// Returns an error if the network thread cannot be started.
    pub fn online(
        world: WorldState,
        seats: usize,
        bind: SocketAddr,
        publish_lan: bool,
    ) -> std::io::Result<Self> {
        let mut host = Self::with_lobby(world, seats, true);
        host.net = Some(HostNet::start(bind, publish_lan)?);
        Ok(host)
    }

    fn with_lobby(world: WorldState, seats: usize, start_in_lobby: bool) -> Self {
        let seats = seats.clamp(1, MAX_LOCAL_SEATS);
        let (world, existing) = keep_first_players(world, seats);
        let tuning = NetTuning {
            debug_hash_interval: if cfg!(debug_assertions) {
                DEV_DEBUG_HASH_INTERVAL
            } else {
                0
            },
            ..NetTuning::default()
        };
        let config = HostConfig {
            params: SessionParams::new(world.tick_rate_hz()),
            tuning,
            start_in_lobby,
            token_seed: random_token_seed(),
            ..HostConfig::default()
        };
        let mut host = Self {
            session: HostSession::new(world, config),
            seats: Vec::new(),
            net: None,
        };
        host.bind_seats(existing, seats);
        host
    }

    /// Opens a running local game to remote players (join in progress), keeping the world.
    ///
    /// # Errors
    ///
    /// Returns an error if the network thread cannot be started.
    pub fn open_online(&mut self, bind: SocketAddr, publish_lan: bool) -> std::io::Result<()> {
        if self.net.is_none() {
            self.net = Some(HostNet::start(bind, publish_lan)?);
            self.session.set_paused(false);
        }
        Ok(())
    }

    /// Names the host's first seat in the roster.
    pub fn set_host_name(&mut self, name: &str) {
        if let Some(first) = self.seats.first() {
            self.session.set_local_player_name(first.seat, name);
        }
    }

    /// Detaches the network side (to shut it down explicitly).
    pub const fn take_net(&mut self) -> Option<HostNet> {
        self.net.take()
    }

    /// The authoritative world.
    #[must_use]
    pub const fn world(&self) -> &WorldState {
        self.session.world()
    }

    /// Local seats in seat order.
    #[must_use]
    pub fn seats(&self) -> &[LocalSeat] {
        &self.seats
    }

    /// Current session epoch.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.session.epoch().0
    }

    /// Starts the game from the lobby.
    pub fn start_game(&mut self) {
        self.session.start_game();
    }

    /// Ends the session for every client (they receive `SessionEnded` before the connections
    /// close, which happens on a helper thread when the host is dropped).
    pub fn end_session(&mut self) {
        if self.net.is_some() {
            self.session.end_session(EndReason::HostClosed);
            let outbound = self.session.take_outbound();
            if let Some(net) = self.net.as_mut() {
                net.send(outbound);
            }
        }
    }

    /// Adds a seat with a freshly spawned player, up to [`MAX_LOCAL_SEATS`].
    pub fn add_seat(&mut self) -> Option<LocalSeat> {
        if self.seats.len() >= MAX_LOCAL_SEATS {
            return None;
        }
        let seat = (0..MAX_LOCAL_SEATS)
            .map(seat_id)
            .find(|seat| self.seats.iter().all(|local| local.seat != *seat))?;
        let player = to_sim_player_id(self.session.add_local_player(seat));
        let local = LocalSeat { seat, player };
        self.seats.push(local);
        Some(local)
    }

    /// Queues a seat's input.
    pub fn submit(&mut self, seat: SeatId, input: SeatInput) {
        let Some(player) = self.session.seat_player(seat) else {
            return;
        };
        self.session
            .submit_local_input(player, input.controls, input.actions);
    }

    /// Pauses or resumes ticking.
    pub const fn set_paused(&mut self, paused: bool) {
        self.session.set_paused(paused);
    }

    /// Drains transport events into the session.
    pub fn pump(&mut self, delta_seconds: f32) {
        if let Some(net) = self.net.as_mut() {
            net.pump(&mut self.session, delta_seconds);
        }
    }

    /// Runs the fixed ticks due for `dt` and sends the outbound messages.
    pub fn advance(&mut self, dt: Duration) -> RuntimeFrame {
        let epoch = self.epoch();
        let output = self.session.advance(dt);
        if let Some(net) = self.net.as_mut() {
            net.send(output.outbound);
        } else {
            debug_assert!(
                output.outbound.is_empty(),
                "local play has no remote clients"
            );
        }
        RuntimeFrame {
            ticks_run: output.ticks_run,
            events: output
                .events
                .into_iter()
                .map(|event| runtime_event(epoch, event))
                .collect(),
            changed_tiles: output
                .terrain_ops
                .iter()
                .map(|(_, change): &(_, TileChange)| change.position)
                .collect(),
            world_replaced: false,
        }
    }

    /// Replaces the world and starts a new epoch, keeping the seat count.
    ///
    /// The seats adopt the saved rigs in id order, exactly like loading from the title: saved
    /// rigs beyond the seat count are dropped and seats without a saved rig get a fresh one. When
    /// the save runs at another tick rate the host is rebuilt for that rate instead (its epoch
    /// restarts; only a purely local host can do that, since remote peers hold the old rate), so
    /// a load never trips the session's rate assertion.
    ///
    /// Returns `false` (and changes nothing) for a save at another tick rate while hosting
    /// remote clients, which run at the session's rate.
    pub fn load_world(&mut self, world: WorldState) -> bool {
        let seat_count = self.seats.len();
        if world.tick_rate_hz() != self.session.params().tick_rate_hz {
            if self.net.is_some() {
                return false;
            }
            *self = Self::new(world, seat_count);
            return true;
        }
        for local in std::mem::take(&mut self.seats) {
            self.session.remove_local_player(local.seat);
        }
        // Remote players are re-added to the new world by the session (`new_epoch`).
        let (world, saved) = keep_first_players(world, seat_count);
        self.session.new_epoch(world);
        self.bind_seats(saved, seat_count);
        true
    }

    /// Changes one live tuning value session-wide (dev F3 keys): host-side knobs such as the held
    /// input window take effect immediately, every connected client is told, and clients that
    /// join later start with it. Returns a description.
    pub fn adjust_tuning(&mut self, parameter: usize, step: i32) -> String {
        let mut tuning = self.session.config().tuning;
        let description = tweak_tuning(&mut tuning, parameter, step);
        self.session.set_tuning(tuning);
        description
    }

    fn lobby_info(&self) -> Option<LobbyInfo> {
        let net = self.net.as_ref()?;
        let status = match net.status() {
            HostNetStatus::Starting => "Opening the host endpoint...".to_owned(),
            HostNetStatus::Ready { addr, .. } => format!(
                "Hosting on {addr} ({} remote player(s) connected)",
                net.connections().len()
            ),
            HostNetStatus::Failed(error) => format!("Hosting failed: {error}"),
        };
        Some(LobbyInfo {
            roster: self.session.roster(),
            addr: net.addr(),
            descriptor: net.descriptor_json(),
            lan_published: net.lan_published(),
            started: self.session.phase() == HostPhase::InGame,
            status,
            local_player: self
                .seats
                .first()
                .and_then(|seat| from_sim_player(seat.player)),
        })
    }

    fn net_debug_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "HOST tick {} | epoch {} | phase {:?} | clamps {}",
            self.session.tick().0,
            self.epoch(),
            self.session.phase(),
            self.session.stats().catch_up_clamps
        )];
        let Some(net) = self.net.as_ref() else {
            lines.push("Local session (no network). Open to LAN from the pause menu.".to_owned());
            return lines;
        };
        let (up, down) = net.byte_rates();
        lines.push(format!(
            "clients {} | up {:.1} KB/s | down {:.1} KB/s",
            net.connections().len(),
            up / 1024.0,
            down / 1024.0
        ));
        for (client, connection) in net.connections() {
            let stats = self.session.client_stats(*client);
            let player = self
                .session
                .client_player(*client)
                .map_or_else(|| "-".to_owned(), |player| player.0.to_string());
            let (rtt, loss) = connection
                .stats
                .map_or((0, 0.0), |link| (link.rtt.as_millis(), link.loss * 100.0));
            lines.push(format!(
                "  {client} p{player} {} rtt {rtt}ms loss {loss:.1}% margin {} late {} held {} buf {}",
                connection.remote,
                stats
                    .and_then(|stats| stats.last_margin)
                    .map_or_else(|| "-".to_owned(), |margin| margin.to_string()),
                stats.map_or(0, |stats| stats.frames_late),
                stats.map_or(0, |stats| stats.held_ticks),
                self.session.buffered_inputs(*client),
            ));
        }
        lines.push(tuning_summary(&self.session.config().tuning));
        lines
    }

    /// Binds seats `0..` to `saved` world players in order, then spawns players for the rest up
    /// to `seat_count`.
    fn bind_seats(&mut self, saved: Vec<PlayerId>, seat_count: usize) {
        for (index, player) in saved.into_iter().enumerate() {
            let seat = seat_id(index);
            if let Some(net_player) = from_sim_player(player)
                && self.session.adopt_local_player(seat, net_player)
            {
                self.seats.push(LocalSeat { seat, player });
            }
        }
        while self.seats.len() < seat_count {
            if self.add_seat().is_none() {
                break;
            }
        }
    }
}

/// Live-tunable parameters (dev F3 keys), in cycle order.
pub const TUNABLE_PARAMETERS: [&str; 6] = [
    "snapshot_rate_hz",
    "input_lead_target_ms",
    "interp_delay_ms",
    "smoothing_half_life_ms",
    "held_input_ms",
    "input_redundancy",
];

/// Steps one tunable parameter and describes the result.
fn tweak_tuning(tuning: &mut NetTuning, parameter: usize, step: i32) -> String {
    let bump = |value: u32, unit: u32, min: u32, max: u32| -> u32 {
        let delta = unit.saturating_mul(step.unsigned_abs());
        if step >= 0 {
            value.saturating_add(delta).min(max)
        } else {
            value.saturating_sub(delta).max(min)
        }
    };
    let name = TUNABLE_PARAMETERS[parameter % TUNABLE_PARAMETERS.len()];
    match parameter % TUNABLE_PARAMETERS.len() {
        0 => tuning.snapshot_rate_hz = bump(tuning.snapshot_rate_hz, 5, 5, 120),
        1 => tuning.input_lead_target_ms = bump(tuning.input_lead_target_ms, 5, 0, 500),
        2 => {
            let current = match tuning.interp_delay {
                InterpDelay::Auto => 0,
                InterpDelay::FixedMs(ms) => ms,
            };
            let next = bump(current, 10, 0, 1_000);
            tuning.interp_delay = if next == 0 {
                InterpDelay::Auto
            } else {
                InterpDelay::FixedMs(next)
            };
        }
        3 => tuning.smoothing_half_life_ms = bump(tuning.smoothing_half_life_ms, 10, 0, 1_000),
        4 => tuning.held_input_ms = bump(tuning.held_input_ms, 10, 0, 1_000),
        _ => tuning.input_redundancy = bump(tuning.input_redundancy, 1, 1, 32),
    }
    format!("{name} -> {}", tuning_summary(tuning))
}

fn tuning_summary(tuning: &NetTuning) -> String {
    format!(
        "tuning: snap {}Hz lead {}ms interp {} smooth {}ms held {}ms redund {}",
        tuning.snapshot_rate_hz,
        tuning.input_lead_target_ms,
        match tuning.interp_delay {
            InterpDelay::Auto => "auto".to_owned(),
            InterpDelay::FixedMs(ms) => format!("{ms}ms"),
        },
        tuning.smoothing_half_life_ms,
        tuning.held_input_ms,
        tuning.input_redundancy
    )
}

fn client_state(client: &OnlineClient) -> ClientState {
    match client.session().phase() {
        ClientPhase::Ended(EndReason::HostClosed) => ClientState::Finished {
            message: "The host ended the session.".to_owned(),
            can_reconnect: false,
        },
        ClientPhase::Ended(EndReason::Kicked) => ClientState::Finished {
            message: "The host removed you from the session.".to_owned(),
            can_reconnect: false,
        },
        ClientPhase::Ended(EndReason::Left) => ClientState::Finished {
            message: "You left the session.".to_owned(),
            can_reconnect: false,
        },
        ClientPhase::Rejected(reason) => ClientState::Finished {
            message: format!("The host rejected the connection: {}.", reject_text(reason)),
            can_reconnect: false,
        },
        phase => match client.link() {
            ClientLink::Closed(reason) => ClientState::Finished {
                message: format!("Disconnected from the host ({reason})."),
                can_reconnect: client.reconnect_token().is_some(),
            },
            _ => match phase {
                ClientPhase::Lobby => ClientState::Lobby,
                ClientPhase::InGame => ClientState::InGame,
                _ => ClientState::Connecting,
            },
        },
    }
}

const fn reject_text(reason: drillgame_netcode::RejectReason) -> &'static str {
    match reason {
        drillgame_netcode::RejectReason::VersionMismatch { .. } => "game version mismatch",
        drillgame_netcode::RejectReason::TokenInUse => "that player is already connected",
        drillgame_netcode::RejectReason::NotAccepting => "the session is not accepting players",
    }
}

fn client_lobby_info(client: &OnlineClient) -> LobbyInfo {
    let status = match client.link() {
        ClientLink::Connecting => "Connecting to the host...".to_owned(),
        ClientLink::Connected(_) => match client.session().phase() {
            ClientPhase::Connecting => "Connected; waiting for the host's welcome...".to_owned(),
            ClientPhase::Lobby => "In the host's lobby. Waiting for the host to start.".to_owned(),
            _ => "In game.".to_owned(),
        },
        ClientLink::Closed(reason) => format!("Disconnected: {reason}"),
    };
    LobbyInfo {
        roster: client.session().lobby().to_vec(),
        addr: None,
        descriptor: None,
        lan_published: false,
        started: client.session().phase() == ClientPhase::InGame,
        status,
        local_player: client.session().player(),
    }
}

fn client_debug_lines(client: &OnlineClient) -> Vec<String> {
    let session = client.session();
    let view = session.view();
    let stats = session.stats();
    let (rtt, loss) = client
        .link_stats()
        .map_or((0, 0.0), |link| (link.rtt.as_millis(), link.loss * 100.0));
    let (up, down) = client.byte_rates();
    let hash = if stats.hash_checks == 0 {
        "no checks yet".to_owned()
    } else if stats.hash_mismatches == 0 {
        format!("ok ({} checks)", stats.hash_checks)
    } else {
        format!(
            "MISMATCH {} / {} checks",
            stats.hash_mismatches, stats.hash_checks
        )
    };
    vec![
        format!(
            "CLIENT tick {} | server tick {} | lead {} | phase {:?}",
            view.client_tick.0,
            view.server_tick.0,
            view.client_tick.diff(view.server_tick),
            view.phase
        ),
        format!(
            "input margin {} | rtt {rtt}ms | loss {loss:.1}% | up {:.1} KB/s | down {:.1} KB/s",
            stats
                .last_margin
                .map_or_else(|| "-".to_owned(), |margin| margin.to_string()),
            up / 1024.0,
            down / 1024.0
        ),
        format!(
            "corrections {:.1}/s (total {}, snaps {}) | last {:.3} tiles | max {:.3}",
            client.corrections_per_second(),
            stats.corrections,
            stats.snaps,
            stats.last_correction,
            stats.correction_max
        ),
        format!(
            "pending {} | snapshot buffer {} | interp delay {:.1} ticks | resyncs {} | stalls {}",
            session.pending_len(),
            session.remote_buffer().max_depth(),
            session.interp_delay_ticks(),
            stats.clock_resyncs,
            stats.stalled_ticks
        ),
        format!("hash {hash} | keyframes {}", stats.keyframes_received),
        tuning_summary(session.tuning()),
    ]
}

/// Drops world players beyond the first `seats` (in id order) and returns the kept ids.
fn keep_first_players(mut world: WorldState, seats: usize) -> (WorldState, Vec<PlayerId>) {
    let mut kept: Vec<PlayerId> = world.player_ids().collect();
    for extra in kept.split_off(seats.min(kept.len())) {
        world.remove_player(extra);
    }
    (world, kept)
}

fn seat_id(index: usize) -> SeatId {
    SeatId(u32::try_from(index).unwrap_or(u32::MAX))
}

fn to_sim_player_id(player: drillgame_netcode::PlayerId) -> PlayerId {
    to_sim_player(player)
}

fn runtime_event(epoch: u64, event: NetEvent<WorldEvent>) -> RuntimeEvent {
    RuntimeEvent {
        epoch,
        tick: event.tick.0,
        player: event.player.map(to_sim_player),
        kind: event.kind,
        ordinal: event.ordinal,
        event: match event.payload {
            WorldEvent::Gameplay(gameplay) => RuntimeEventKind::Gameplay(gameplay),
            WorldEvent::ActionResolved {
                command,
                resolution,
                ..
            } => RuntimeEventKind::ActionResolved {
                command,
                applied: resolution == ActionResolution::Applied,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use drillgame_simulation::SimulationParams;

    use super::*;

    const RIG: PlayerId = PlayerId::new(1);

    #[test]
    fn loading_a_save_with_other_rig_ids_adopts_them_instead_of_respawning() {
        let mut runtime = Runtime::local(WorldState::new_game(RIG), 1);
        let epoch = runtime.epoch();
        let other = PlayerId::new(9);
        let mut save = WorldState::new_game(other);
        save.player_mut(other).expect("saved rig").credits = 4_321;

        assert!(runtime.load_world(save));

        assert_eq!(runtime.epoch(), epoch + 1, "same session, new epoch");
        assert_eq!(
            runtime.local_seats(),
            &[LocalSeat {
                seat: SeatId(0),
                player: other,
            }]
        );
        assert_eq!(runtime.world().player_ids().collect::<Vec<_>>(), [other]);
        assert_eq!(
            runtime.world().player(other).map(|rig| rig.credits),
            Some(4_321)
        );
    }

    #[test]
    fn loading_keeps_the_seat_count_and_drops_extra_saved_rigs() {
        let mut runtime = Runtime::local(WorldState::new_game(RIG), 2);
        let mut save = WorldState::new_game(RIG);
        for extra in [2, 3, 4] {
            save.add_player(PlayerId::new(extra));
        }
        assert!(runtime.load_world(save));
        let seated: Vec<PlayerId> = runtime
            .local_seats()
            .iter()
            .map(|seat| seat.player)
            .collect();
        assert_eq!(seated, [RIG, PlayerId::new(2)]);
        assert_eq!(runtime.world().player_ids().collect::<Vec<_>>(), seated);

        // A save with fewer rigs than seats respawns the missing seat.
        assert!(runtime.load_world(WorldState::new_game(RIG)));
        assert_eq!(runtime.local_seats().len(), 2);
        assert_eq!(runtime.world().player_ids().count(), 2);
    }

    #[test]
    fn service_actions_away_from_a_building_resolve_as_refused() {
        let mut world = WorldState::new_game(RIG);
        // Well below the surface: no building door here.
        world.player_mut(RIG).expect("rig").y = 30.0 * drillgame_simulation::TILE_SIZE;
        world
            .player_mut(RIG)
            .expect("rig")
            .cargo
            .insert(drillgame_simulation::terrain::MineralKind::Gold, 1);
        let mut runtime = Runtime::local(world, 1);
        let seat = runtime.local_seats()[0].seat;
        runtime.submit(
            seat,
            SeatInput {
                controls: ControlState::default(),
                actions: vec![PlayerCommand::Service(
                    drillgame_simulation::SessionServiceRequest::SellCargo,
                )],
            },
        );
        let frame = runtime.advance(Duration::from_millis(50));
        let resolved: Vec<bool> = frame
            .events
            .iter()
            .filter_map(|event| match &event.event {
                RuntimeEventKind::ActionResolved { applied, .. } => Some(*applied),
                RuntimeEventKind::Gameplay(_) => None,
            })
            .collect();
        assert_eq!(resolved, [false], "the sale resolves exactly once, refused");
        assert_eq!(
            runtime.world().player(RIG).map(|rig| rig.cargo.len()),
            Some(1),
            "nothing was sold"
        );
    }

    #[test]
    fn loading_a_save_at_another_tick_rate_rebuilds_the_host() {
        let mut runtime = Runtime::local(WorldState::new_game(RIG), 1);
        let save = WorldState::new_game_with_params(RIG, SimulationParams::new(30));
        assert!(runtime.load_world(save));
        assert_eq!(runtime.world().tick_rate_hz(), 30);
        assert_eq!(runtime.local_seats().len(), 1);
        let ticks: u32 = (0..60)
            .map(|_| runtime.advance(Duration::from_secs(1) / 60).ticks_run)
            .sum();
        // `1s / 60` truncates to whole nanoseconds, so allow one tick of rounding.
        assert!(
            (29..=30).contains(&ticks),
            "the rebuilt host ticks at the save's rate: {ticks}"
        );
    }
}
