//! A joined online client: [`ClientSession`] over the real world, wired to its own
//! `quic_transport` network thread through the net bridge.
//!
//! Per frame the game loop calls [`OnlineClient::pump`] (non-blocking drain of transport events
//! into the session) and [`OnlineClient::advance`] (fixed client ticks: prediction plus the input
//! datagram). Rendering reads [`OnlineClient::world`] (the predicted world: terrain and HUD) and
//! [`OnlineClient::pose_overrides`] (local rig predicted + smoothed, remote rigs interpolated).

use std::{collections::BTreeMap, time::Duration};

use drillgame_netcode::{
    ClientConfig, ClientPhase, ClientSession, ReconnectToken, SeatId,
    world::{ActionResolution, WorldEvent, to_sim_player},
};
use drillgame_quic_transport::{ConnId, JoinTarget, LinkStats, NetErrorKind, NetEvent, NetHandle};
use drillgame_simulation::{
    ControlState, PlayerCommand, PlayerId, TILE_SIZE, WorldState,
    terrain::{Tile, TilePosition},
};

use super::{
    bridge::{self, RateMeter},
    host::reason_text,
    net_config,
};
use crate::runtime::{LocalSeat, RuntimeEvent, RuntimeEventKind, RuntimeFrame};

/// The transport connection to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientLink {
    /// Handshaking.
    Connecting,
    /// Connected; the control stream is open.
    Connected(ConnId),
    /// The connection ended (or never opened), with a reason.
    Closed(String),
}

/// A joined client and its transport.
#[derive(Debug)]
pub struct OnlineClient {
    net: Option<NetHandle>,
    link: ClientLink,
    session: ClientSession<WorldState>,
    target: JoinTarget,
    link_stats: Option<LinkStats>,
    fallback: WorldState,
    seats: Vec<LocalSeat>,
    controls: ControlState,
    actions: Vec<PlayerCommand>,
    up: RateMeter,
    down: RateMeter,
    corrections: RateMeter,
    counted_corrections: u64,
    drawn_key: Option<(u64, u64)>,
    drawn_tiles: Vec<Tile>,
    unsent: Vec<bridge::WorldClientMessage>,
    notices: Vec<String>,
}

impl OnlineClient {
    /// Starts a network thread and joins `target` as `name`, reclaiming a previous player with
    /// `token`. `fallback` is rendered until the host's keyframe arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if the network thread cannot be started.
    pub fn join(
        target: JoinTarget,
        name: impl Into<String>,
        token: Option<ReconnectToken>,
        fallback: WorldState,
    ) -> std::io::Result<Self> {
        let net = NetHandle::start(net_config())?;
        net.join(target.clone()).map_err(std::io::Error::other)?;
        let session = ClientSession::new(ClientConfig {
            name: name.into(),
            reconnect_token: token,
            initial_rtt: None,
        });
        Ok(Self {
            net: Some(net),
            link: ClientLink::Connecting,
            session,
            target,
            link_stats: None,
            fallback,
            seats: Vec::new(),
            controls: ControlState::default(),
            actions: Vec::new(),
            up: RateMeter::default(),
            down: RateMeter::default(),
            corrections: RateMeter::default(),
            counted_corrections: 0,
            drawn_key: None,
            drawn_tiles: Vec::new(),
            unsent: Vec::new(),
            notices: Vec::new(),
        })
    }

    /// The netcode session.
    #[must_use]
    pub const fn session(&self) -> &ClientSession<WorldState> {
        &self.session
    }

    /// The transport link.
    #[must_use]
    pub const fn link(&self) -> &ClientLink {
        &self.link
    }

    /// Where this client joined (for reconnecting).
    #[must_use]
    pub const fn target(&self) -> &JoinTarget {
        &self.target
    }

    /// Latest transport statistics.
    #[must_use]
    pub const fn link_stats(&self) -> Option<LinkStats> {
        self.link_stats
    }

    /// Bytes per second sent / received over the last second.
    #[must_use]
    pub const fn byte_rates(&self) -> (f32, f32) {
        (self.up.rate(), self.down.rate())
    }

    /// Reconciliation corrections per second over the last second.
    #[must_use]
    pub const fn corrections_per_second(&self) -> f32 {
        self.corrections.rate()
    }

    /// The token that reclaims this client's player after a disconnect.
    #[must_use]
    pub const fn reconnect_token(&self) -> Option<ReconnectToken> {
        self.session.reconnect_token()
    }

    /// The local player, once welcomed.
    #[must_use]
    pub fn player(&self) -> Option<PlayerId> {
        self.session.player().map(to_sim_player)
    }

    /// The world to render and to drive menus from: the predicted world once welcomed.
    #[must_use]
    pub fn world(&self) -> &WorldState {
        self.session
            .predicted()
            .or_else(|| self.session.authority())
            .unwrap_or(&self.fallback)
    }

    /// The single local seat, once welcomed.
    #[must_use]
    pub fn seats(&self) -> &[LocalSeat] {
        &self.seats
    }

    /// Notices (connection errors, decode failures) since the last call.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// Sets the held controls and queues one-shot actions for the next client tick.
    pub fn submit(&mut self, controls: ControlState, mut actions: Vec<PlayerCommand>) {
        self.controls = controls;
        self.actions.append(&mut actions);
    }

    /// Toggles the lobby ready flag.
    pub fn set_ready(&mut self, ready: bool) {
        self.session.set_ready(ready);
        self.flush_outbound();
    }

    /// Drains every pending transport event into the session. Never blocks.
    pub fn pump(&mut self, delta_seconds: f32) {
        self.up.tick(delta_seconds);
        self.down.tick(delta_seconds);
        self.corrections.tick(delta_seconds);
        let Some(net) = self.net.as_ref() else {
            return;
        };
        for event in net.drain() {
            match event {
                NetEvent::Connected { conn, .. } => {
                    if self.link == ClientLink::Connecting {
                        self.link = ClientLink::Connected(conn);
                    }
                }
                NetEvent::Message { bytes, .. } => {
                    self.down.add(bytes.len() as u64);
                    if let Err(error) = self.session.handle_server_bytes(&bytes) {
                        self.notices
                            .push(format!("Dropped a malformed host message: {error}"));
                    }
                }
                NetEvent::Disconnected { reason, .. } => {
                    self.link = ClientLink::Closed(reason_text(&reason));
                }
                NetEvent::Stats { stats, .. } => {
                    self.link_stats = Some(stats);
                    self.session.set_rtt(stats.rtt);
                }
                NetEvent::Error(error) => {
                    if error.kind == NetErrorKind::Join {
                        self.link = ClientLink::Closed(error.message.clone());
                    }
                    self.notices.push(error.to_string());
                }
                NetEvent::HostReady { .. } | NetEvent::ShutdownComplete => {}
            }
        }
        if self.seats.is_empty()
            && let Some(player) = self.player()
        {
            self.seats.push(LocalSeat {
                seat: SeatId(0),
                player,
            });
        }
        self.flush_outbound();
    }

    /// Advances real time: runs the due client ticks with the submitted input, sends the input
    /// datagram and returns this frame's presentation events and changed terrain.
    pub fn advance(&mut self, dt: Duration) -> RuntimeFrame {
        let tick_before = self.session.view().client_tick;
        let actions = std::mem::take(&mut self.actions);
        let messages = self.session.advance(dt, &self.controls, actions);
        self.send_all(&messages);
        let ticks_run = u32::try_from(
            self.session
                .view()
                .client_tick
                .0
                .saturating_sub(tick_before.0),
        )
        .unwrap_or(u32::MAX);
        let corrections = self.session.stats().corrections;
        self.corrections
            .add(corrections.saturating_sub(self.counted_corrections));
        self.counted_corrections = corrections;

        let epoch = self.session.epoch().0;
        let events = self
            .session
            .drain_events()
            .into_iter()
            .map(|presented| {
                let event = presented.event;
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
            })
            .collect();
        let (changed_tiles, world_replaced) = self.terrain_changes();
        RuntimeFrame {
            ticks_run,
            events,
            changed_tiles,
            world_replaced,
        }
    }

    /// Presentation poses in world pixels: the local rig (predicted, interpolated between fixed
    /// ticks, plus the smoothing offset) and every remote rig (interpolated).
    #[must_use]
    pub fn pose_overrides(&self) -> BTreeMap<PlayerId, (f32, f32)> {
        let view = self.session.view();
        let mut poses: BTreeMap<PlayerId, (f32, f32)> = view
            .remote_poses
            .iter()
            .map(|(player, pose)| {
                (
                    to_sim_player(*player),
                    (pose.x * TILE_SIZE, pose.y * TILE_SIZE),
                )
            })
            .collect();
        if let (Some(player), Some(pose)) = (view.local_player, view.local_pose) {
            poses.insert(
                to_sim_player(player),
                (pose.x * TILE_SIZE, pose.y * TILE_SIZE),
            );
        }
        poses
    }

    /// Leaves for good: sends `Leave` (flushed before the connection closes) and stops the
    /// network thread in the background.
    pub fn leave(mut self) {
        let leave = self.session.leave();
        self.send_all(&[leave]);
    }

    /// [`Self::leave`], waiting for the network thread to flush and stop (headless tools, tests).
    pub fn leave_blocking(mut self) {
        let leave = self.session.leave();
        self.send_all(&[leave]);
        self.shutdown_blocking();
    }

    /// Drops the transport connection without leaving, as a crash or network loss would: the
    /// host keeps the player reserved for the reconnect token.
    pub fn drop_connection(&mut self) {
        if let (Some(net), ClientLink::Connected(conn)) = (self.net.as_ref(), &self.link) {
            let _ignored = net.disconnect(*conn, "connection dropped");
        }
    }

    /// Stops the network thread and waits for it (queued reliable messages are flushed first).
    /// For headless tools and tests; the game thread drops the client instead.
    pub fn shutdown_blocking(mut self) {
        if let Some(net) = self.net.take() {
            let _events = net.shutdown();
        }
    }

    /// Whether the session is over from this client's point of view (ended, rejected, or the
    /// connection closed).
    #[must_use]
    pub const fn finished(&self) -> bool {
        matches!(
            self.session.phase(),
            ClientPhase::Ended(_) | ClientPhase::Rejected(_)
        ) || matches!(self.link, ClientLink::Closed(_))
    }

    fn flush_outbound(&mut self) {
        let messages = self.session.take_outbound();
        self.send_all(&messages);
    }

    fn send_all(&mut self, messages: &[bridge::WorldClientMessage]) {
        let (Some(net), ClientLink::Connected(conn)) = (self.net.as_ref(), &self.link) else {
            if self.link == ClientLink::Connecting {
                // Keep reliable messages (the `Hello`) until the control stream opens; input
                // datagrams are worthless by then.
                self.unsent.extend(
                    messages
                        .iter()
                        .filter(|message| !message.is_unreliable())
                        .cloned(),
                );
            }
            return;
        };
        let queued = std::mem::take(&mut self.unsent);
        for message in queued.iter().chain(messages) {
            match bridge::send_client(net, *conn, message) {
                Ok(size) => self.up.add(size as u64),
                Err(error) => self.notices.push(format!("Send failed: {error}")),
            }
        }
    }

    /// Tiles of the rendered (predicted) terrain that changed since the last call, and whether
    /// the whole terrain must be redrawn (first keyframe, new epoch, resized terrain).
    fn terrain_changes(&mut self) -> (Vec<TilePosition>, bool) {
        let Some(world) = self
            .session
            .predicted()
            .or_else(|| self.session.authority())
        else {
            return (Vec::new(), false);
        };
        let terrain = world.terrain();
        let key = (self.session.epoch().0, terrain.digest());
        if self.drawn_key == Some(key) {
            return (Vec::new(), false);
        }
        let epoch_changed = self.drawn_key.is_none_or(|(epoch, _)| epoch != key.0);
        self.drawn_key = Some(key);
        let tiles = terrain.tiles();
        if epoch_changed || tiles.len() != self.drawn_tiles.len() {
            self.drawn_tiles = tiles.to_vec();
            return (Vec::new(), true);
        }
        let width = usize::try_from(terrain.width().max(1)).unwrap_or(1);
        let mut changed = Vec::new();
        for (index, (drawn, tile)) in self.drawn_tiles.iter_mut().zip(tiles).enumerate() {
            if drawn != tile {
                *drawn = *tile;
                changed.push(TilePosition {
                    x: i32::try_from(index % width).unwrap_or(i32::MAX),
                    y: i32::try_from(index / width).unwrap_or(i32::MAX),
                });
            }
        }
        (changed, false)
    }
}

impl Drop for OnlineClient {
    fn drop(&mut self) {
        if let Some(net) = self.net.take() {
            // Shutting down flushes queued reliable messages (e.g. `Leave`) and waits for the
            // host, bounded by the close timeout: never on the game thread.
            let spawned = std::thread::Builder::new()
                .name("drillgame-net-stop".to_owned())
                .spawn(move || drop(net));
            if let Err(error) = spawned {
                eprintln!("Could not stop the network thread in the background: {error}");
            }
        }
    }
}
