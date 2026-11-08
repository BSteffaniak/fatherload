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
    pub fn new(world: WorldState, seats: usize) -> Self {
        let seats = seats.clamp(1, MAX_LOCAL_SEATS);
        let (world, existing) = keep_first_players(world, seats);
        let config = HostConfig {
            params: SessionParams::new(world.tick_rate_hz()),
            ..HostConfig::default()
        };
        let mut host = Self {
            session: HostSession::new(world, config),
            seats: Vec::new(),
        };
        host.bind_seats(existing, seats);
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

    /// Replaces the world and starts a new epoch, keeping the seat count.
    ///
    /// The seats adopt the saved rigs in id order, exactly like loading from the title: saved
    /// rigs beyond the seat count are dropped and seats without a saved rig get a fresh one. When
    /// the save runs at another tick rate the host is rebuilt for that rate instead (its epoch
    /// restarts; local play has no remote peers holding the old one), so a load never trips the
    /// session's rate assertion.
    pub fn load_world(&mut self, world: WorldState) {
        let seat_count = self.seats.len();
        if world.tick_rate_hz() != self.session.params().tick_rate_hz {
            *self = Self::new(world, seat_count);
            return;
        }
        for local in std::mem::take(&mut self.seats) {
            self.session.remove_local_player(local.seat);
        }
        let (world, saved) = keep_first_players(world, seat_count);
        self.session.new_epoch(world);
        self.bind_seats(saved, seat_count);
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
