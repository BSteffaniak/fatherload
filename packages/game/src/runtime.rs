//! The app's gameplay authority (plan §10).
//!
//! Single-player and local split-screen run on the same [`HostSession`] the netcode harness and
//! online hosting use, with one local seat per player and zero remote clients. There is no other
//! gameplay path: the shell submits held controls and one-shot actions per seat, the runtime
//! advances fixed ticks from real time, and rendering reads the world it exposes.

use std::time::Duration;

use drillgame_netcode::{
    HostConfig, HostSession, NetEvent, SeatId, SessionParams,
    world::{ActionResolution, WorldEvent, from_sim_player, to_sim_player},
};
use drillgame_simulation::{
    ControlState, GameplayEvent, PlayerCommand, PlayerId, TileChange, WorldState,
    terrain::TilePosition,
};

/// Most local seats (split-screen views) the shell will add. Seats, views and layout are generic
/// over any count; this is the only place the cap is enforced.
pub const MAX_LOCAL_SEATS: usize = 4;

/// A local seat and the player it controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalSeat {
    /// The seat (one per local input device group / split-screen view).
    pub seat: SeatId,
    /// The world player bound to the seat.
    pub player: PlayerId,
}

/// One seat's gameplay input for the next tick(s).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeatInput {
    /// Held, quantized movement controls.
    pub controls: ControlState,
    /// One-shot actions (bombs, scanner, infrastructure, services, rescue).
    pub actions: Vec<PlayerCommand>,
}

/// A gameplay event produced by a fixed tick, with its stable identity.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeFrame {
    /// Fixed ticks simulated this frame.
    pub ticks_run: u32,
    /// Gameplay events from those ticks, in order.
    pub events: Vec<RuntimeEvent>,
    /// Terrain tiles changed by those ticks, in order (may repeat).
    pub changed_tiles: Vec<TilePosition>,
}

/// The gameplay authority the app runs on.
#[derive(Debug)]
pub enum Runtime {
    /// Single-player / split-screen / (next step) online host.
    Host(Box<LocalHost>),
    /// Joined online client (next step: `drillgame_netcode::ClientSession`).
    #[allow(
        dead_code,
        reason = "the client runtime is wired in the online step; the variant keeps every match exhaustive now"
    )]
    Client(ClientRuntime),
}

/// Placeholder for the joined-client runtime (plan phase 5). Uninhabited until then.
#[derive(Debug)]
pub enum ClientRuntime {}

impl ClientRuntime {
    /// No value of this type exists yet, so no client code path can run.
    fn absurd(_client: &Self) -> ! {
        unreachable!("the client runtime is not wired yet")
    }
}

impl Runtime {
    /// A local host for `world` with `seats` local players (at least one, at most
    /// [`MAX_LOCAL_SEATS`]).
    #[must_use]
    pub fn local(world: WorldState, seats: usize) -> Self {
        Self::Host(Box::new(LocalHost::new(world, seats)))
    }

    /// The world rendering and the shell read.
    #[must_use]
    pub fn world(&self) -> &WorldState {
        match self {
            Self::Host(host) => host.world(),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Local seats in seat order.
    #[must_use]
    pub fn local_seats(&self) -> &[LocalSeat] {
        match self {
            Self::Host(host) => host.seats(),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Current session epoch.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "read by tests now and by the online runtime next")
    )]
    #[must_use]
    pub fn epoch(&self) -> u64 {
        match self {
            Self::Host(host) => host.epoch(),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Adds another local seat. Returns `None` at [`MAX_LOCAL_SEATS`].
    pub fn add_local_seat(&mut self) -> Option<LocalSeat> {
        match self {
            Self::Host(host) => host.add_seat(),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Queues one seat's input for the next tick(s). Controls are held until replaced; actions
    /// are applied exactly once.
    pub fn submit(&mut self, seat: SeatId, input: SeatInput) {
        match self {
            Self::Host(host) => host.submit(seat, input),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Stops (or resumes) ticking. Only meaningful for a local host: online play keeps running.
    pub fn set_paused(&mut self, paused: bool) {
        match self {
            Self::Host(host) => host.set_paused(paused),
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }

    /// Advances real time, running 0..n fixed ticks.
    pub fn advance(&mut self, dt: Duration) -> RuntimeFrame {
        match self {
            Self::Host(host) => host.advance(dt),
            Self::Client(client) => ClientRuntime::absurd(client),
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
            Self::Host(host) => {
                host.load_world(world);
                true
            }
            Self::Client(client) => ClientRuntime::absurd(client),
        }
    }
}

/// [`HostSession`] over the real world with local seats.
#[derive(Debug)]
pub struct LocalHost {
    session: HostSession<WorldState>,
    seats: Vec<LocalSeat>,
}

impl LocalHost {
    /// Hosts `world` with `seats` local players. Existing world players are adopted by the first
    /// seats in id order (so a loaded save keeps its rigs); further seats spawn new players and
    /// world players beyond the seat count are dropped.
    #[must_use]
    pub fn new(mut world: WorldState, seats: usize) -> Self {
        let seats = seats.clamp(1, MAX_LOCAL_SEATS);
        let existing: Vec<PlayerId> = world.player_ids().collect();
        for extra in existing.iter().skip(seats) {
            world.remove_player(*extra);
        }
        let config = HostConfig {
            params: SessionParams::new(world.tick_rate_hz()),
            ..HostConfig::default()
        };
        let mut host = Self {
            session: HostSession::new(world, config),
            seats: Vec::new(),
        };
        for (index, player) in existing.into_iter().take(seats).enumerate() {
            let seat = seat_id(index);
            if let Some(net_player) = from_sim_player(player)
                && host.session.adopt_local_player(seat, net_player)
            {
                host.seats.push(LocalSeat { seat, player });
            }
        }
        while host.seats.len() < seats {
            if host.add_seat().is_none() {
                break;
            }
        }
        host
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

    /// Runs the fixed ticks due for `dt`.
    pub fn advance(&mut self, dt: Duration) -> RuntimeFrame {
        let epoch = self.epoch();
        let output = self.session.advance(dt);
        debug_assert!(
            output.outbound.is_empty(),
            "local play has no remote clients"
        );
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
        }
    }

    /// Replaces the world and starts a new epoch, keeping the seats. World players without a
    /// seat are dropped; seats whose player is missing from the new world respawn.
    pub fn load_world(&mut self, mut world: WorldState) {
        let seated: Vec<PlayerId> = self.seats.iter().map(|seat| seat.player).collect();
        let unseated: Vec<PlayerId> = world
            .player_ids()
            .filter(|player| !seated.contains(player))
            .collect();
        for player in unseated {
            world.remove_player(player);
        }
        self.session.new_epoch(world);
    }
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
