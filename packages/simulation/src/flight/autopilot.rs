//! A bot pilot for tests and tools (Wave 11 integration): it flies, lands, drives, digs and
//! docks with **the player's controls only** (rotation thrusters, main, retro and side thrust,
//! cruise, warp, use; no assist), reading the world as a player sees it.
//!
//! [`SpaceBot`] is a list of [`Leg`]s; each tick [`SpaceBot::next`] reads the world and gives
//! the controls (and at most one command) for `player`'s next tick, so the same bot drives a
//! local world, a split-screen seat or an online client. It never moves anything itself.

use std::collections::VecDeque;

use crate::{
    ControlState, PlayerCommand, PlayerId, WorldState, field::trig::wrap_angle, terrain::TILE_SIZE,
};

use super::{
    FlightCommand, FlightPlace, StarDrive,
    nav::{self, NavTarget},
    physics::{WARP_CLEARANCE, axes, cruise_limit},
    space::{self, BodyKind, FlightBody, SpaceQuery, SystemRef},
};

/// One leg of a journey.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Leg {
    /// Climb over the space line into space (from under it over the Moon).
    Leave,
    /// Set the waypoint on a system and warp there (clearing wells first).
    Warp(SystemRef),
    /// Set the nav waypoint on a body.
    Waypoint(NavTarget),
    /// Come in over a body, re-enter onto its round ground and touch down.
    Land(FlightBody),
    /// Drive `columns` strip columns clockwise round the body (hopping the hills).
    Drive(i64),
    /// Dig `rows` rows straight down (toward the core).
    Dig(i64),
    /// Thrust up out of the strip until handed back to space.
    TakeOff,
    /// Fly to a station's port and dock.
    Dock(FlightBody),
    /// Undock (the use button).
    Undock,
    /// Fly into the home world's approach sphere (back to the home sky).
    Home,
}

/// What a leg remembers between ticks.
#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools, reason = "a bot's scratch flags")]
struct LegState {
    ticks: u32,
    started: bool,
    spooled: bool,
    arrived: bool,
    landed: bool,
    still: u32,
    last: i64,
    travelled: i64,
    last_travelled: i64,
    start_row: i64,
    burn: u32,
    stuck: u32,
    nudge: u32,
}

/// The bot (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct SpaceBot {
    legs: VecDeque<Leg>,
    state: LegState,
    /// Legs finished, in order, with the tick count each took.
    pub finished: Vec<(Leg, u32)>,
}

/// The strip tile `(column, row)` of `player`'s rig on a body's ground.
#[must_use]
pub fn strip_tile(world: &WorldState, player: PlayerId) -> Option<(i64, i64)> {
    let store = world.store_of(player);
    let frame = world.store_frame(store)?;
    let rig = world.player(player)?;
    let (u, v) = crate::world::strip_point(frame.world(rig.x, rig.y));
    #[allow(clippy::cast_possible_truncation, reason = "strip tiles")]
    Some((u.floor() as i64, v.floor() as i64))
}

/// The controls that fly `player` toward `target` (impulse, or cruise far out), nose at
/// `heading` (`None`: at the target), at most `top_speed` px/s on the thrusters.
#[must_use]
pub fn steer(
    world: &WorldState,
    player: PlayerId,
    target: crate::position::WorldPos,
    top_speed: f32,
    heading: Option<f32>,
) -> (ControlState, Option<PlayerCommand>) {
    let Some(rig) = world.player(player) else {
        return (ControlState::default(), None);
    };
    let flight = rig.flight;
    let attitude = rig.attitude;
    let distance = flight.pos.span(target);
    let (dx, dy) = flight.pos.delta(target);
    let (dir_x, dir_y) = (dx / distance.max(1.0), dy / distance.max(1.0));
    let heading = heading.unwrap_or_else(|| nav::bearing(flight.pos, target));
    let error = wrap_angle(heading - attitude.orientation);
    let rotate = 2.5f32.mul_add(error, -0.9 * attitude.angular_velocity);
    let cruising = matches!(flight.drive, StarDrive::Cruise { .. });
    if !cruising
        && matches!(flight.place, FlightPlace::Space)
        && distance > space::tiles(4_000.0)
        && error.abs() < 0.05
        && attitude.angular_velocity.abs() < 0.05
    {
        return (
            ControlState::default(),
            Some(PlayerCommand::Flight(FlightCommand::ToggleCruise)),
        );
    }
    if cruising {
        if distance < space::tiles(3_000.0) {
            return (
                ControlState::default(),
                Some(PlayerCommand::Flight(FlightCommand::ToggleCruise)),
            );
        }
        let speed = flight.speed();
        let limit = cruise_limit(distance);
        return (
            ControlState::new(0.0, speed < limit * 0.9, speed > limit * 1.1).with_rotation(rotate),
            None,
        );
    }
    let speed_wanted = (distance * 0.6).min(top_speed);
    let (ex, ey) = (
        dir_x.mul_add(speed_wanted, -flight.velocity_x),
        dir_y.mul_add(speed_wanted, -flight.velocity_y),
    );
    let (up, right) = axes(attitude.orientation);
    let forward = ex.mul_add(up.0, ey * up.1);
    let strafe = ex.mul_add(right.0, ey * right.1);
    (
        ControlState::new(
            (strafe / 40.0).clamp(-1.0, 1.0),
            forward > 8.0,
            forward < -8.0,
        )
        .with_rotation(rotate),
        None,
    )
}

/// Rotation that levels the rig on the ground (to its local up).
fn level(world: &WorldState, player: PlayerId) -> f32 {
    world.player(player).map_or(0.0, |rig| {
        let error = wrap_angle(-rig.attitude.orientation);
        2.5f32.mul_add(error, -0.9 * rig.attitude.angular_velocity)
    })
}

impl SpaceBot {
    /// A bot that flies `legs` in order.
    #[must_use]
    pub fn new(legs: impl IntoIterator<Item = Leg>) -> Self {
        Self {
            legs: legs.into_iter().collect(),
            ..Self::default()
        }
    }

    /// The leg it is on (`None`: done).
    #[must_use]
    pub fn leg(&self) -> Option<Leg> {
        self.legs.front().copied()
    }

    /// Whether every leg is done.
    #[must_use]
    pub fn done(&self) -> bool {
        self.legs.is_empty()
    }

    fn finish(&mut self) {
        if let Some(leg) = self.legs.pop_front() {
            self.finished.push((leg, self.state.ticks));
        }
        self.state = LegState::default();
    }

    /// Reads the tick's events (touchdowns, warp arrivals).
    pub fn observe(&mut self, events: &[crate::GameplayEventKind]) {
        for event in events {
            match event {
                crate::GameplayEventKind::Flight {
                    what: super::FlightEvent::WarpSpooling,
                    ..
                } => self.state.spooled = true,
                crate::GameplayEventKind::Flight {
                    what: super::FlightEvent::WarpArrived,
                    ..
                } => self.state.arrived = true,
                crate::GameplayEventKind::Planet {
                    what: crate::planet::landing::PlanetEvent::Landed,
                    ..
                } => self.state.landed = true,
                _ => {}
            }
        }
    }

    /// The controls (and command) for `player`'s next tick. Panics past a leg's time budget
    /// (a test bot: a stuck journey is a failure).
    #[allow(clippy::too_many_lines, reason = "one match over the legs")]
    pub fn next(
        &mut self,
        world: &WorldState,
        player: PlayerId,
    ) -> (ControlState, Option<PlayerCommand>) {
        let hz = world.timing().rate.hz();
        loop {
            let Some(leg) = self.leg() else {
                return (ControlState::default(), None);
            };
            self.state.ticks += 1;
            assert!(
                self.state.ticks < hz * 900,
                "the bot's {leg:?} leg ran out of time: {:?}",
                world.player(player).map(|rig| rig.flight)
            );
            let Some(rig) = world.player(player) else {
                return (ControlState::default(), None);
            };
            let flight = rig.flight;
            match leg {
                Leg::Leave => {
                    if world.in_space(player) {
                        self.finish();
                        continue;
                    }
                    return (
                        ControlState::new(0.0, true, false).with_rotation(level(world, player)),
                        None,
                    );
                }
                Leg::Waypoint(target) => {
                    self.finish();
                    return (
                        ControlState::default(),
                        Some(PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(
                            target,
                        )))),
                    );
                }
                Leg::Warp(target) => {
                    if !self.state.started {
                        self.state.started = true;
                        return (
                            ControlState::default(),
                            Some(PlayerCommand::Flight(FlightCommand::SetWaypoint(Some(
                                NavTarget::System(target),
                            )))),
                        );
                    }
                    if self.state.spooled && self.state.arrived {
                        // The warp arrived (it drops into cruise at the system's edge).
                        self.finish();
                        continue;
                    }
                    return Self::warp_controls(world, player, target);
                }
                Leg::Land(body) => {
                    if let Some(controls) = self.land_controls(world, player, body) {
                        return controls;
                    }
                    self.finish();
                }
                Leg::Drive(columns) => {
                    if let Some(controls) = self.drive_controls(world, player, columns) {
                        return controls;
                    }
                    self.finish();
                }
                Leg::Dig(rows) => {
                    if let Some(controls) = self.dig_controls(world, player, rows) {
                        return controls;
                    }
                    self.finish();
                }
                Leg::TakeOff => {
                    if !matches!(world.store_of(player), crate::place::StoreKey::Body(_)) {
                        self.finish();
                        continue;
                    }
                    return (
                        ControlState::new(0.0, true, false).with_rotation(level(world, player)),
                        None,
                    );
                }
                Leg::Dock(station) => {
                    if matches!(flight.place, FlightPlace::Docked(_)) {
                        self.finish();
                        continue;
                    }
                    return Self::dock_controls(world, player, station);
                }
                Leg::Undock => {
                    if !matches!(flight.place, FlightPlace::Docked(_)) {
                        self.finish();
                        continue;
                    }
                    return (
                        ControlState::default(),
                        Some(PlayerCommand::Flight(FlightCommand::Use)),
                    );
                }
                Leg::Home => {
                    if !world.in_space(player) {
                        self.finish();
                        continue;
                    }
                    return steer(world, player, space::home_centre(), 1_500.0, None);
                }
            }
        }
    }

    fn warp_controls(
        world: &WorldState,
        player: PlayerId,
        target: SystemRef,
    ) -> (ControlState, Option<PlayerCommand>) {
        let Some(rig) = world.player(player) else {
            return (ControlState::default(), None);
        };
        let Some(star) = world.space().system(target).map(|system| system.star) else {
            return (ControlState::default(), None);
        };
        let flight = rig.flight;
        let attitude = rig.attitude;
        let error = wrap_angle(nav::bearing(flight.pos, star) - attitude.orientation);
        let rotate = 2.5f32.mul_add(error, -0.9 * attitude.angular_velocity);
        let wells: Vec<FlightBody> = world
            .space_bodies_near(flight.pos)
            .into_iter()
            .filter(|body| body.kind != BodyKind::Station)
            .collect();
        let (gap, _) = space::nearest_surface(&wells, flight.pos);
        match flight.drive {
            StarDrive::Warp(_) | StarDrive::Spool { .. } => {
                (ControlState::default().with_rotation(rotate), None)
            }
            _ if !matches!(flight.place, FlightPlace::Space) => (
                ControlState::new(0.0, error.abs() < 0.3, false).with_rotation(rotate),
                None,
            ),
            _ if gap < WARP_CLEARANCE + space::tiles(500.0) => {
                if matches!(flight.drive, StarDrive::Off)
                    && error.abs() < 0.05
                    && attitude.angular_velocity.abs() < 0.05
                {
                    (
                        ControlState::default(),
                        Some(PlayerCommand::Flight(FlightCommand::ToggleCruise)),
                    )
                } else {
                    (
                        ControlState::new(0.0, error.abs() < 0.3, false).with_rotation(rotate),
                        None,
                    )
                }
            }
            _ if error.abs() < 0.04 && attitude.angular_velocity.abs() < 0.03 => (
                ControlState::default(),
                Some(PlayerCommand::Flight(FlightCommand::Warp)),
            ),
            _ => (ControlState::default().with_rotation(rotate), None),
        }
    }

    /// `None` once down and still on the ground.
    fn land_controls(
        &self,
        world: &WorldState,
        player: PlayerId,
        body: FlightBody,
    ) -> Option<(ControlState, Option<PlayerCommand>)> {
        let rig = world.player(player)?;
        if !matches!(world.store_of(player), crate::place::StoreKey::Body(_)) {
            // Still in space: come in over the body, nose up, and descend to the hand-over.
            let ground = world.body_ground(body.body)?;
            #[allow(clippy::cast_possible_truncation, reason = "a radius in tiles")]
            let land_radius = space::tiles(ground.land_radius_tiles() as f32);
            let flight = rig.flight;
            let (dx, dy) = body.centre.delta(flight.pos);
            let r = crate::field::trig::hypot(dx, dy).max(1.0);
            let (ux, uy) = (dx / r, dy / r);
            let up = nav::bearing(body.centre, flight.pos);
            let altitude = r - land_radius;
            let (target, speed) = if altitude > space::tiles(400.0) {
                let reach = land_radius + space::tiles(200.0);
                (body.centre.offset(ux * reach, uy * reach), 1_500.0)
            } else {
                let reach = land_radius - space::tiles(40.0);
                (
                    body.centre.offset(ux * reach, uy * reach),
                    (altitude * 0.4).clamp(60.0, 400.0),
                )
            };
            // Nose at the target on the way in (the retro thrusters only brake), then up, away
            // from the centre, for the last stretch: gravity brings it down.
            let heading = (altitude < space::tiles(450.0)).then_some(up);
            return Some(steer(world, player, target, speed, heading));
        }
        // On the ground: the fall (slowed by the rig's drag under a small moon's pull) ends in
        // a touchdown; the thrusters only catch a fall that runs too fast.
        if self.state.landed && rig.velocity_y.abs() < 1.0 {
            return None;
        }
        Some((
            ControlState::new(0.0, rig.velocity_y > 300.0, false)
                .with_rotation(level(world, player)),
            None,
        ))
    }

    /// `None` once `columns` are driven.
    fn drive_controls(
        &mut self,
        world: &WorldState,
        player: PlayerId,
        columns: i64,
    ) -> Option<(ControlState, Option<PlayerCommand>)> {
        let hz = world.timing().rate.hz();
        let store = world.store_of(player);
        let terrain = world.store_terrain(store)?;
        let width = i64::from(terrain.planet_ground()?.strip_width());
        let (column, _) = strip_tile(world, player)?;
        if self.state.started {
            let mut delta = column - self.state.last;
            if delta > width / 2 {
                delta -= width;
            } else if delta < -width / 2 {
                delta += width;
            }
            self.state.travelled += delta;
        }
        self.state.started = true;
        self.state.last = column;
        if self.state.travelled >= columns {
            return None;
        }
        let rig = world.player(player)?;
        let frame = world.store_frame(store)?;
        let ahead = frame.tile_at(TILE_SIZE.mul_add(0.6, rig.x), rig.y);
        let wall_ahead = terrain.is_solid_at(ahead);
        if wall_ahead {
            self.state.burn = self.state.burn.max(1);
        }
        if rig.velocity_x.abs() < 30.0 {
            self.state.stuck += 1;
        } else {
            self.state.stuck = 0;
        }
        if self.state.stuck > hz / 10 && self.state.burn == 0 {
            // A short hop: a small moon's pull is light, and a long burn would leave the
            // ground (the strip's top hands a rig back to space). Hops that get nowhere grow
            // (out of a pit), and progress resets them.
            if self.state.travelled > self.state.start_row {
                self.state.start_row = self.state.travelled;
                self.state.nudge = 0;
            } else {
                self.state.nudge = (self.state.nudge + 1).min(hz / 4);
            }
            self.state.burn = (hz / 15).max(2) + self.state.nudge;
            self.state.stuck = 0;
        }
        // No progress for a second (wedged under an overhang): drill on through the wall.
        if self.state.travelled == self.state.last_travelled {
            self.state.still += 1;
        } else {
            self.state.still = 0;
            self.state.last_travelled = self.state.travelled;
        }
        if self.state.still > hz {
            return Some((ControlState::new(1.0, false, false), None));
        }
        let thrust = self.state.burn > 0 && rig.velocity_y > -120.0;
        self.state.burn = self.state.burn.saturating_sub(1);
        let push = if wall_ahead { 0.0 } else { 1.0 };
        Some((ControlState::new(push, thrust, false), None))
    }

    /// `None` once `rows` deeper than where it started.
    fn dig_controls(
        &mut self,
        world: &WorldState,
        player: PlayerId,
        rows: i64,
    ) -> Option<(ControlState, Option<PlayerCommand>)> {
        let hz = world.timing().rate.hz();
        let (_, row) = strip_tile(world, player)?;
        if !self.state.started {
            self.state.started = true;
            self.state.start_row = row;
            self.state.last = row;
            // Settle a moment before the drill bites.
            self.state.nudge = 0;
            self.state.still = 0;
            return Some((ControlState::default(), None));
        }
        if row >= self.state.start_row + rows {
            return None;
        }
        if row == self.state.last {
            self.state.still += 1;
        } else {
            self.state.still = 0;
            self.state.last = row;
        }
        if self.state.still > hz * 2 {
            self.state.still = 0;
            self.state.nudge = hz / 6;
        }
        if self.state.nudge > 0 {
            self.state.nudge -= 1;
            let fraction = (world.player(player)?.x / TILE_SIZE).rem_euclid(1.0);
            let toward = if fraction < 0.5 { 0.4 } else { -0.4 };
            return Some((ControlState::new(toward, false, false), None));
        }
        Some((ControlState::new(0.0, false, true), None))
    }

    fn dock_controls(
        world: &WorldState,
        player: PlayerId,
        station: FlightBody,
    ) -> (ControlState, Option<PlayerCommand>) {
        let (Some(rig), Some(port)) = (world.player(player), station.port()) else {
            return (ControlState::default(), None);
        };
        let flight = rig.flight;
        let dock_heading = station.dock_orientation();
        let (up, _) = axes(dock_heading);
        let lineup = port.offset(-up.0 * space::tiles(25.0), -up.1 * space::tiles(25.0));
        let to_lineup = flight.pos.span(lineup);
        let to_port = flight.pos.span(port);
        let final_leg = to_lineup < space::tiles(4.0) || to_port < space::tiles(25.0);
        let target = if final_leg { port } else { lineup };
        let heading = (to_port < space::tiles(60.0)).then_some(dock_heading);
        let error =
            wrap_angle(heading.unwrap_or(rig.attitude.orientation) - rig.attitude.orientation);
        if final_leg && to_port < space::tiles(2.5) && flight.speed() < 60.0 && error.abs() < 0.2 {
            return (
                ControlState::default(),
                Some(PlayerCommand::Flight(FlightCommand::Use)),
            );
        }
        let top = if final_leg { 55.0 } else { 1_200.0 };
        steer(world, player, target, top, heading)
    }
}

/// A chart from the Archive (a plinth of the world seeded `seed`) whose real system has a
/// small, light moon with round ground: a quick landing and lap.
#[must_use]
pub fn chart_with_a_small_moon(
    seed: u64,
) -> Option<(crate::star_chart::StarChart, SystemRef, FlightBody)> {
    let galaxy = super::GeneratedSpace::new(crate::space::SpaceSeed::current(seed));
    for x in 0..400 {
        let chart = crate::star_chart::chart_at_site(
            seed,
            x * 37,
            9_900,
            400,
            crate::ids::SimulationTick::new(0),
        );
        let system = galaxy.chart_system(&chart);
        let Some(found) = galaxy.system(system) else {
            continue;
        };
        let small = galaxy
            .bodies_of(&found)
            .into_iter()
            .filter(|body| body.kind == BodyKind::Planet && body.radius <= space::tiles(560.0))
            .filter(|body| {
                galaxy.space_body(body.body).is_some_and(|id| {
                    crate::place::planet_params_for(crate::space::SpaceSeed::current(seed), id)
                        .is_some_and(|params| params.gravity() <= 0.5)
                })
            })
            .min_by(|a, b| a.radius.total_cmp(&b.radius));
        if let Some(body) = small {
            return Some((chart, system, body));
        }
    }
    None
}

/// Halcyon Dock as flight sees it (the home system's first station).
#[must_use]
pub fn home_dock(world: &WorldState) -> Option<FlightBody> {
    let galaxy = world.space();
    galaxy
        .bodies_of(&galaxy.system(space::HOME_SYSTEM)?)
        .into_iter()
        .find(|body| body.kind == BodyKind::Station)
}
