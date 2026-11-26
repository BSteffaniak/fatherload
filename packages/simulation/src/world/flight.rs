//! Space flight in the world (Wave 11, see [`crate::flight`]): leaving the home world over
//! the Moon, the flight step for rigs away, the Star Drive's spool and warp, docking, life
//! support and the emergency beacon, scanner pings and waypoints.
//!
//! A rig away from home ([`crate::flight::RigFlight::away`]) skips the classic movement and
//! the home world's hazard pass: its home position is parked over the Moon
//! ([`WorldState::space_park`]) and its real position is its space position.

use crate::{
    PlayerCommand,
    attitude::RigAttitude,
    event::{GameplayEventKind, TickOutput},
    field::trig::wrap_angle,
    flight::{
        AIR_REFILL_PER_SECOND, AIR_WARNINGS, BodyKind, DOCK_ALIGN, DOCK_RANGE, DOCK_SPEED,
        FlightCommand, FlightEvent, FlightGearKind, FlightPlace, FlightShared, GeneratedSpace,
        HEAT_WARNING, PARK_ABOVE_ROWS, RETURN_BELOW_ROWS, RigFlight, SPACE_ENTRY_ROWS, SpaceQuery,
        StarDrive, StationService, SystemRef, UNDOCK_SPEED, WarpRefusal, WarpRun, air_capacity,
        beacon_fee,
        nav::{self, NavTarget, ScanPing},
        physics::{
            self, FlightInput, FlightRig, WARP_ABORT_ANGLE, WARP_ALIGN, WARP_ARRIVAL,
            WARP_CLEARANCE, axes, cruise_limit, warp_duration_millis, warp_fuel, warp_position,
        },
        space::{self, FlightBody, nearest_surface},
    },
    ids::{PlayerId, SimulationTick},
    input::ControlState,
    player::engine_power,
    star_chart::StarChart,
    summary::PlayerScopedCommandOutcome,
    terrain::TILE_SIZE,
};

use super::WorldState;

/// Shortest time between two scanner pings (ms).
pub const PING_COOLDOWN_MILLIS: u64 = 2_000;
/// Air over this share of the tank re-arms the low-air warnings.
pub(super) const AIR_REARM_SHARE: f32 = 0.6;

const fn flight_event(what: FlightEvent, player_id: PlayerId, detail: u32) -> GameplayEventKind {
    GameplayEventKind::Flight {
        what,
        player_id,
        detail,
    }
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// Space flight's shared state: waypoints, systems found, scanner pings.
    #[must_use]
    pub const fn flight_shared(&self) -> &FlightShared {
        &self.flight
    }

    /// Replaces space flight's shared state (scenes, tests, tools).
    pub fn set_flight_shared(&mut self, shared: FlightShared) {
        self.flight = shared;
    }

    /// `player_id`'s rig's flight state.
    #[must_use]
    pub fn rig_flight(&self, player_id: PlayerId) -> Option<&RigFlight> {
        self.players.get(&player_id).map(|player| &player.flight)
    }

    /// Whether `player_id`'s rig is away from the home world (in space, approaching, landed
    /// on another body or docked).
    #[must_use]
    pub fn in_space(&self, player_id: PlayerId) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.flight.away())
            || self.moving_places(player_id)
    }

    /// Whether this world has space beyond the Moon (Deep Descent worlds, V11+).
    #[must_use]
    pub fn space_enabled(&self) -> bool {
        self.home_settings().generator.has_descents()
    }

    /// The space flight reads: the space generator's galaxy of this world's seed (one shared
    /// cache per seed, [`crate::space::Space::shared`]).
    #[must_use]
    pub fn space(&self) -> GeneratedSpace {
        GeneratedSpace::new(self.world_seed())
    }

    /// The world's seed (the home terrain's, also inside a body store's ground pass).
    #[must_use]
    pub const fn world_seed(&self) -> crate::space::SpaceSeed {
        crate::space::SpaceSeed::new(self.home_seed(), self.space_generator)
    }

    /// The space generator this world's galaxy is made by (Wave 14 finish).
    #[must_use]
    pub const fn space_generator(&self) -> crate::space::SpaceGenerator {
        self.space_generator
    }

    /// Sets the space generator (tests and tools that load or stage an old world's galaxy).
    pub const fn set_space_generator(&mut self, generator: crate::space::SpaceGenerator) {
        self.space_generator = generator;
    }

    /// Every body near `pos` (owned, so the world can be borrowed mutably after).
    #[must_use]
    pub fn space_bodies_near(&self, pos: crate::position::WorldPos) -> Vec<FlightBody> {
        self.space().bodies_near(pos)
    }

    /// The row space begins above (rigs with the Star Drive cross into it), `None` on worlds
    /// without space or a Moon.
    #[must_use]
    pub fn space_line_row(&self) -> Option<i32> {
        if !self.space_enabled() {
            return None;
        }
        // The home world's (also from inside another store's pass: free flight's store has
        // its own ground; Wave 12 systems).
        let moon = crate::worldgen::sky::moon(&self.home_settings())?;
        Some(moon.top_row - SPACE_ENTRY_ROWS)
    }

    /// Where a rig away in space is parked in the home world (local pixels): high over the
    /// Moon, where nothing reaches it. Every away rig parks at the same spot, so the netcode's
    /// relevance keeps every flyer in reach of the others.
    #[must_use]
    pub fn space_park(&self) -> Option<(f32, f32)> {
        let line = self.space_line_row()?;
        let column = crate::worldgen::sky::moon_column(self.terrain.settings())?;
        Some(self.frame().centre_of(column, line - PARK_ABOVE_ROWS))
    }

    /// Every system the crew knows: home, the Archive's charts, and those found by flying or
    /// scanning (no repeats, in that order).
    #[must_use]
    pub fn known_systems(&self) -> Vec<SystemRef> {
        let mut known = vec![space::HOME_SYSTEM];
        let space = self.space();
        for chart in self.star_charts() {
            let system = space.chart_system(chart);
            if !known.contains(&system) {
                known.push(system);
            }
        }
        for system in &self.flight.found_systems {
            if !known.contains(system) {
                known.push(*system);
            }
        }
        known
    }

    /// The chart the Archive gave for `system`, if any.
    #[must_use]
    pub fn chart_of(&self, system: SystemRef) -> Option<&StarChart> {
        let space = self.space();
        self.star_charts()
            .iter()
            .find(|chart| chart.system == system.id || space.chart_system(chart) == system)
    }

    /// `player_id`'s waypoint: their own, else the crew's.
    #[must_use]
    pub fn waypoint_of(&self, player_id: PlayerId) -> Option<NavTarget> {
        self.flight.waypoint_of(player_id)
    }

    /// Seconds of air `player_id`'s rig carries.
    #[must_use]
    pub fn air_capacity_of(&self, player_id: PlayerId) -> f32 {
        self.players.get(&player_id).map_or_else(
            || air_capacity(0),
            |player| {
                air_capacity(player.hazard_gear.life_support) + player.hazard_gear.ship.extra_air()
            },
        )
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of flight for `player_id`'s rig away from home, with `controls` (instead of
    /// the classic movement).
    #[allow(
        clippy::too_many_lines,
        reason = "the drive's modes, the step and its edges in order"
    )]
    pub(super) fn step_star_flight(
        &mut self,
        player_id: PlayerId,
        controls: ControlState,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let now = self.millis_at(next_tick);
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let mut flight = player.flight;
        let mut attitude = player.attitude;
        let heat_before = flight.heat;
        let infinite_fuel = self.test_mode.is_some_and(|state| state.infinite_fuel);
        let mut fuel = player.fuel;
        // The ship's drive range and graviton spool (Wave 12 stations), with the endgame's
        // marks and the Heartline Drive (Wave 13).
        let effects = self.ship_effects(player_id);
        let range = effects.range_scale;
        let spool_millis = effects.spool_millis;
        let mut events: Vec<(FlightEvent, u32)> = Vec::new();
        // The warp's spool and jump (a pure function of time).
        match flight.drive {
            StarDrive::Spool { target, since } => {
                let space = self.space();
                match space.system(target) {
                    Some(system)
                        if wrap_angle(
                            attitude.orientation - nav::bearing(flight.pos, system.star),
                        )
                        .abs()
                            <= WARP_ABORT_ANGLE =>
                    {
                        if now.saturating_sub(since) >= spool_millis {
                            let bodies = space.bodies_near(flight.pos);
                            // Test mode's infinite fuel spools as the command did.
                            let aboard = if infinite_fuel {
                                f32::MAX
                            } else {
                                fuel * range
                            };
                            match warp_check(&flight, attitude, &bodies, system.star, aboard) {
                                Ok(cost) => {
                                    let arrival = warp_arrival(flight.pos, system.star);
                                    let distance = flight.pos.span(arrival);
                                    if !infinite_fuel {
                                        fuel = (fuel - cost / range).max(0.0);
                                    }
                                    flight.drive = StarDrive::Warp(WarpRun {
                                        origin: flight.pos,
                                        arrival,
                                        target,
                                        start: now,
                                        duration_millis: warp_duration_millis(f64::from(distance)),
                                    });
                                    events.push((FlightEvent::WarpEngaged, 0));
                                }
                                Err(refusal) => {
                                    flight.drive = StarDrive::Off;
                                    events.push((FlightEvent::WarpRefused, refusal.code()));
                                }
                            }
                        }
                    }
                    _ => {
                        flight.drive = StarDrive::Off;
                        events.push((FlightEvent::WarpInterrupted, 0));
                    }
                }
            }
            StarDrive::Warp(run) => {
                let (pos, (vx, vy), done) = warp_position(&run, now);
                flight.pos = pos;
                flight.velocity_x = vx;
                flight.velocity_y = vy;
                let bodies = self.space_bodies_near(pos);
                let (gap, _) = nearest_surface(&bodies, pos);
                let (up, _) = axes(attitude.orientation);
                if done {
                    let speed = cruise_limit(gap);
                    flight.drive = StarDrive::Cruise { speed };
                    flight.velocity_x = up.0 * speed;
                    flight.velocity_y = up.1 * speed;
                    self.flight.found(run.target);
                    events.push((FlightEvent::WarpArrived, 0));
                } else {
                    let well = bodies.iter().any(|body| {
                        body.body.system != run.target
                            && pos.span(body.centre) - body.radius < WARP_CLEARANCE * 0.5
                    });
                    let bearing = nav::bearing(run.origin, run.arrival);
                    let turned =
                        wrap_angle(attitude.orientation - bearing).abs() > WARP_ABORT_ANGLE;
                    if well || turned {
                        let speed = cruise_limit(gap).min(flight.speed());
                        flight.drive = StarDrive::Cruise { speed };
                        flight.velocity_x = up.0 * speed;
                        flight.velocity_y = up.1 * speed;
                        events.push((FlightEvent::WarpInterrupted, u32::from(well)));
                    }
                }
            }
            StarDrive::Transit(transit) => {
                // Through a wormhole (Wave 12 systems): a closed form of time, like the warp.
                if Self::step_transit(&mut flight, transit, now) {
                    output.push_event(GameplayEventKind::SpaceSystems {
                        what: crate::flight::systems::SystemsEvent::WormholeExited,
                        player_id,
                        detail: 0,
                    });
                }
            }
            StarDrive::Off | StarDrive::Cruise { .. } => {}
        }
        // Wave 12 systems: black holes, wormhole mouths and belt rocks are flight bodies too.
        let bodies = self.flight_bodies_near(flight.pos, flight.speed());
        let marks_before = flight.marks;
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let gear = player.hazard_gear;
        let effective_capacity = player.cargo_capacity.max(1);
        #[allow(clippy::cast_precision_loss, reason = "a load ratio")]
        let cargo_ratio = (player.cargo_used() as f32 / effective_capacity as f32).min(1.0);
        let assists = controls.assists();
        let input = FlightInput {
            forward: f32::from(i8::from(controls.thrust) - i8::from(controls.drill_down)),
            strafe: controls.horizontal(),
            rotate: controls.rotation(),
            controls: controls.flight(),
            gyro: assists.gyro && gear.attitude.gyro_stabiliser > 0,
            horizon: assists.horizon && gear.attitude.horizon_hold > 0,
        };
        let rig = FlightRig {
            engine_power: engine_power(player.engine_level)
                * cargo_ratio.mul_add(-0.18, 1.0)
                * effects.engine_scale,
            cargo_ratio,
            // The phase lattice shields as two tiers more (Wave 12 alien tech; more with its
            // marks, Wave 13).
            shield: gear.flight.reentry_shield + effects.lattice_tiers,
            auto_land: gear.flight.auto_land > 0,
            fuel: if infinite_fuel { f32::MAX } else { fuel },
            now_millis: now,
        };
        let outcome = physics::step_flight(
            &mut flight,
            &mut attitude,
            &input,
            &rig,
            &bodies,
            self.timing.delta_seconds,
        );
        events.extend(outcome.events.iter().copied());
        if heat_before <= HEAT_WARNING && flight.heat > HEAT_WARNING {
            events.push((FlightEvent::HeatWarning, 0));
        }
        if !infinite_fuel {
            fuel = (fuel - outcome.fuel_burned).max(0.0);
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            player.flight = flight;
            player.attitude = attitude;
            player.fuel = fuel;
        }
        for (what, detail) in events {
            output.push_event(flight_event(what, player_id, detail));
        }
        // The ship's hull line takes a share off (Wave 12 stations).
        let hull_damage = outcome.hull_damage * effects.damage_scale;
        if hull_damage > 0.0 && self.damage_player(player_id, hull_damage) {
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
        }
        // Wave 12 systems: what space's things did (a horizon, a mouth, flares, rocks, ...).
        if self.after_flight_step(player_id, controls, &bodies, &outcome, marks_before, output) {
            return;
        }
        if outcome.reached_home {
            self.return_home(player_id, output);
            return;
        }
        self.sync_space_store_position(player_id);
    }

    /// Space flight's world pass (after the rigs moved): rigs crossing the space line leave
    /// home, life support for rigs away, and the emergency beacon.
    pub(super) fn advance_star_flight(&mut self, output: &mut TickOutput) {
        if !self.space_enabled() {
            return;
        }
        let Some(line) = self.space_line_row() else {
            return;
        };
        let frame = self.frame();
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in ids {
            // A rig handed to a body's ground this tick is on its way (F11's queued move).
            if self.moving_places(player_id) {
                continue;
            }
            let Some(player) = self.players.get(&player_id) else {
                continue;
            };
            if !player.flight.away() {
                let row = frame.row(player.y);
                if row < line
                    && self.space_access(player_id).may_leave()
                    && !self.is_on_foot(player_id)
                    && player.hull > 0.0
                {
                    self.enter_space(player_id, output);
                }
                continue;
            }
            // A rig away still in the home store's parked slot (just left, or from an older
            // save) moves into free flight's store (Wave 12 systems).
            if self.store_of(player_id).is_home() {
                self.move_into_space_store(player_id);
            }
            self.life_support(player_id, output);
        }
    }

    /// Free flight's store's pass (Wave 12 systems: a rig in free flight lives in the
    /// `StoreKey::Space` store): life support and the beacon for its rigs.
    pub(super) fn advance_space_store(&mut self, output: &mut TickOutput) {
        if self.active_store() != crate::place::StoreKey::Space {
            return;
        }
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in ids {
            if self.moving_places(player_id)
                || !self
                    .players
                    .get(&player_id)
                    .is_some_and(|player| player.flight.away())
            {
                continue;
            }
            self.life_support(player_id, output);
        }
    }

    /// Queues `player_id`'s rig (away, in the home store) into free flight's store at its
    /// space position, when that is within the store's ground (else it stays parked).
    pub(super) fn move_into_space_store(&mut self, player_id: PlayerId) {
        let Some(pos) = self.players.get(&player_id).map(|player| player.flight.pos) else {
            return;
        };
        if within_space_store(pos) {
            let _ = self.move_to_place(player_id, crate::place::Place::Space, pos);
        }
    }

    /// Keeps a rig in free flight's store where its exact space position is (its store
    /// position follows the flight model), and parks it home when it flies past the store's
    /// ground.
    pub(super) fn sync_space_store_position(&mut self, player_id: PlayerId) {
        if self.active_store() != crate::place::StoreKey::Space || self.moving_places(player_id) {
            return;
        }
        let frame = self.frame();
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if !player.flight.away() {
            return;
        }
        let pos = player.flight.pos;
        if !within_space_store(pos) {
            if let Some(park) = self.space_park_at_home() {
                let _ = self.move_to_place(player_id, crate::place::Place::HOME, park);
            }
            return;
        }
        let (x, y) = frame.local(pos);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.y = y;
            player.x_remainder = 0.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
    }

    /// Life support for `player_id`'s rig away (on its own clock), the low-air warnings and the
    /// beacon at zero air or a wrecked hull.
    fn life_support(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let seconds = self.timing.delta_seconds;
        {
            let Some(player) = self.players.get(&player_id) else {
                return;
            };
            let place = player.flight.place;
            // Life support, on the rig's own clock (a black hole slows it; Wave 12).
            let scale = self.dilate(player_id, seconds);
            let capacity = self.air_capacity_of(player_id);
            let Some(player) = self.players.get(&player_id) else {
                return;
            };
            let bodies = match place {
                FlightPlace::Landed(_) => self.space_bodies_near(player.flight.pos),
                _ => Vec::new(),
            };
            // The ship's own gear (Wave 12 stations: the symbiont weave).
            self.ship_upkeep(player_id, seconds);
            let Some(player) = self.players.get_mut(&player_id) else {
                return;
            };
            let flight = &mut player.flight;
            let breathable = match flight.place {
                FlightPlace::Landed(body) => bodies
                    .iter()
                    .any(|found| found.body == body && found.breathable()),
                _ => false,
            };
            if matches!(flight.place, FlightPlace::Docked(_)) {
                flight.air = capacity;
            } else if breathable {
                flight.air = AIR_REFILL_PER_SECOND
                    .mul_add(seconds, flight.air)
                    .min(capacity);
            } else {
                flight.air = seconds.mul_add(-scale, flight.air).max(0.0);
            }
            if flight.air > capacity * AIR_REARM_SHARE {
                flight.alerts &= !0b111;
            }
            for (bit, percent) in AIR_WARNINGS.iter().enumerate() {
                let mask = 1_u8 << bit;
                if flight.alerts & mask == 0 && flight.air <= capacity * f32::from(*percent) / 100.0
                {
                    flight.alerts |= mask;
                    output.push_event(flight_event(
                        FlightEvent::AirLow,
                        player_id,
                        u32::from(*percent),
                    ));
                }
            }
            let out_of_air = flight.air <= 0.0;
            let wrecked = player.hull <= 0.0;
            if out_of_air || wrecked {
                self.emergency_beacon(player_id, output);
            }
        }
    }

    /// `player_id`'s rig crosses the space line: into space over the home world, its home
    /// position parked over the Moon.
    fn enter_space(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let frame = self.frame();
        let capacity = self.air_capacity_of(player_id);
        let Some(park) = self.space_park() else {
            return;
        };
        self.drop_cart(player_id);
        self.active_drills.remove(&player_id);
        self.active_drill_timers.remove(&player_id);
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let column = frame.col(player.x);
        player.flight = RigFlight {
            place: FlightPlace::Space,
            pos: space::home_launch(),
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
            drive: StarDrive::Off,
            heat: 0.0,
            air: capacity,
            exit_column: column,
            alerts: 0,
            lag_seconds: player.flight.lag_seconds,
            ..RigFlight::default()
        };
        player.x = park.0;
        player.y = park.1;
        player.x_remainder = 0.0;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        output.push_event(flight_event(FlightEvent::EnteredSpace, player_id, 0));
        // Into free flight's store (Wave 12 systems); parked here only until the move applies.
        self.move_into_space_store(player_id);
    }

    /// `player_id`'s rig flew into the home world's approach sphere: back to the home sky over
    /// the column it left from, turned and moving as it came in (relative to the home world's
    /// down there).
    fn return_home(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let Some(line) = self.space_line_row() else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        // Wave 12 systems: home under the point of the round home world the rig approached
        // (the arc from the departure's column), not only the column it left from.
        let column = self
            .home_column_under(player.flight.pos)
            .unwrap_or(player.flight.exit_column);
        self.return_home_at(
            player_id,
            space::home_centre(),
            column,
            line + RETURN_BELOW_ROWS,
            output,
        );
    }

    /// Hands `player_id`'s rig back to the home sky at tile `(column, row)`, turned and moving
    /// as it came in toward `centre` (that body's up becomes the sky's up).
    pub(super) fn return_home_at(
        &mut self,
        player_id: PlayerId,
        centre: crate::position::WorldPos,
        column: i32,
        row: i32,
        output: &mut TickOutput,
    ) {
        let frame = self.frame();
        // A rig in free flight's store moves home (Wave 12 systems); one parked in the home
        // store is set down in place.
        let home_store_move = !self.store_of(player_id).is_home();
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let flight = player.flight;
        let (dx, dy) = centre.delta(flight.pos);
        // The home world's "up" where the rig came in becomes the sky's up.
        let up = physics::up_angle(dx, dy);
        let (sin, cos) = crate::field::trig::sin_cos(-up);
        let (vx, vy) = (
            flight.velocity_x.mul_add(cos, -flight.velocity_y * sin),
            flight.velocity_x.mul_add(sin, flight.velocity_y * cos),
        );
        let cap = super::AUTHORITATIVE_ABSOLUTE_MAX_SPEED;
        let at_home = !home_store_move;
        if at_home {
            let (x, y) = frame.centre_of(column, row);
            player.x = x;
            player.y = y;
        }
        player.x_remainder = 0.0;
        player.velocity_x = vx.clamp(-cap, cap);
        player.velocity_y = vy.clamp(-cap, cap);
        player.attitude = RigAttitude {
            orientation: crate::attitude::quantize_angle(wrap_angle(
                player.attitude.orientation - up,
            )),
            angular_velocity: player.attitude.angular_velocity,
        };
        player.flight = RigFlight {
            exit_column: flight.exit_column,
            air: flight.air,
            lag_seconds: flight.lag_seconds,
            ..RigFlight::default()
        };
        output.push_event(flight_event(FlightEvent::ReturnedHome, player_id, 0));
        if home_store_move {
            let at = crate::position::WorldPos::tile_center(i64::from(column), i64::from(row));
            let _ = self.move_to_place(player_id, crate::place::Place::HOME, at);
        }
    }

    /// The emergency beacon: `player_id`'s rig is towed home (where the rescue crew drops it)
    /// for [`crate::flight::beacon_fee`], with at least half a hull and half a tank. Never an
    /// unwinnable countdown.
    pub(super) fn emergency_beacon(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let frame = self.frame();
        let (home_x, home_y) = self
            .rescue_home(player_id)
            .unwrap_or_else(|| frame.from_legacy_px(12.0 * TILE_SIZE, 4.0 * TILE_SIZE));
        // From free flight's store (Wave 12 systems) the tow moves the rig home: the same
        // point, as an absolute position (the pass's frame measured it).
        let tow = (!self.store_of(player_id).is_home()).then(|| frame.world(home_x, home_y));
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let fee = beacon_fee(player.credits);
        player.credits -= fee;
        if tow.is_none() {
            player.x = home_x;
            player.y = home_y;
        }
        player.x_remainder = 0.0;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        player.attitude = RigAttitude::LEVEL;
        player.hull = player.hull.max(player.max_hull() * 0.5);
        player.fuel = player.fuel.max(player.fuel_capacity * 0.5);
        player.flight = RigFlight {
            exit_column: player.flight.exit_column,
            ..RigFlight::default()
        };
        output.push_event(flight_event(FlightEvent::BeaconReturn, player_id, fee));
        if let Some(at) = tow {
            let _ = self.move_to_place(player_id, crate::place::Place::HOME, at);
        }
    }

    // --- Commands -----------------------------------------------------------------------------

    /// A command from `player_id` that space flight answers: every [`PlayerCommand::Flight`],
    /// and while the rig is away the scanner (a ping), the rescue call (the beacon) and a
    /// refusal for whatever needs the home world. `None` lets the command through.
    pub(super) fn flight_command(
        &mut self,
        player_id: PlayerId,
        command: &PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        if let PlayerCommand::Flight(command) = command {
            return Some(self.apply_flight_command(player_id, *command, output));
        }
        if !self.in_space(player_id) {
            // The scanner on a body's ground scans the body too (Wave 12 stations).
            if matches!(command, PlayerCommand::UseScanner) {
                self.note_ground_scan(player_id, output);
            }
            return None;
        }
        Some(match command {
            PlayerCommand::UseScanner => self.scanner_ping(player_id, output),
            PlayerCommand::Rescue => {
                self.emergency_beacon(player_id, output);
                PlayerScopedCommandOutcome::Applied
            }
            // Settings and the hardpoint work anywhere.
            PlayerCommand::TestMode(_)
            | PlayerCommand::SetEnemyDifficulty(_)
            | PlayerCommand::SetCollectBlocks { .. }
            | PlayerCommand::MountWeapon { .. } => return None,
            _ => PlayerScopedCommandOutcome::IgnoredUnavailable,
        })
    }

    #[allow(clippy::too_many_lines, reason = "one match over the flight commands")]
    fn apply_flight_command(
        &mut self,
        player_id: PlayerId,
        command: FlightCommand,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let now = self.millis_at(self.simulation_tick.next());
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let flight = player.flight;
        let attitude = player.attitude;
        // The ship's drive range (Wave 12 stations): a warp costs its fuel over it (with the
        // Heartline Drive, Wave 13).
        let fuel = player.fuel * self.ship_effects(player_id).range_scale;
        let infinite_fuel = self.test_mode.is_some_and(|state| state.infinite_fuel);
        match command {
            FlightCommand::SetWaypoint(target) => {
                match target {
                    Some(target) => self.flight.waypoints.insert(player_id, target),
                    None => self.flight.waypoints.remove(&player_id),
                };
                output.push_event(flight_event(FlightEvent::NavUpdated, player_id, 0));
                PlayerScopedCommandOutcome::Applied
            }
            FlightCommand::ShareWaypoint => {
                self.flight.crew_waypoint = self.flight.waypoints.get(&player_id).copied();
                output.push_event(flight_event(FlightEvent::NavUpdated, player_id, 1));
                PlayerScopedCommandOutcome::Applied
            }
            FlightCommand::Beacon if matches!(flight.place, FlightPlace::Landed(_)) => {
                self.body_beacon(player_id, output);
                PlayerScopedCommandOutcome::Applied
            }
            _ if !flight.away() => PlayerScopedCommandOutcome::IgnoredUnavailable,
            FlightCommand::Beacon => {
                self.emergency_beacon(player_id, output);
                PlayerScopedCommandOutcome::Applied
            }
            FlightCommand::ToggleCruise => {
                let next = match flight.drive {
                    StarDrive::Off => {
                        if !matches!(flight.place, FlightPlace::Space) {
                            output.push_event(flight_event(
                                FlightEvent::CruiseDropped,
                                player_id,
                                1,
                            ));
                            return PlayerScopedCommandOutcome::IgnoredUnavailable;
                        }
                        output.push_event(flight_event(FlightEvent::CruiseEngaged, player_id, 0));
                        StarDrive::Cruise {
                            speed: flight.speed(),
                        }
                    }
                    StarDrive::Cruise { .. } | StarDrive::Spool { .. } => {
                        output.push_event(flight_event(FlightEvent::CruiseDropped, player_id, 0));
                        StarDrive::Off
                    }
                    StarDrive::Warp(_) | StarDrive::Transit(_) => {
                        return PlayerScopedCommandOutcome::IgnoredUnavailable;
                    }
                };
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.flight.drive = next;
                    if next == StarDrive::Off {
                        let speed = player.flight.speed();
                        let top = physics::impulse_top_speed(engine_power(player.engine_level));
                        if speed > top {
                            let scale = top / speed;
                            player.flight.velocity_x *= scale;
                            player.flight.velocity_y *= scale;
                        }
                    }
                }
                PlayerScopedCommandOutcome::Applied
            }
            FlightCommand::Warp => {
                let target = self
                    .waypoint_of(player_id)
                    .map(NavTarget::system)
                    .and_then(|system| self.space().system(system));
                let result = target.ok_or(WarpRefusal::NoTarget).and_then(|system| {
                    if flight.pos.span(system.star) < WARP_ARRIVAL * 1.2 {
                        return Err(WarpRefusal::NoTarget);
                    }
                    let bodies = self.space_bodies_near(flight.pos);
                    let fuel = if infinite_fuel { f32::MAX } else { fuel };
                    warp_check(&flight, attitude, &bodies, system.star, fuel).map(|_| system)
                });
                match result {
                    Ok(system) => {
                        if let Some(player) = self.players.get_mut(&player_id) {
                            player.flight.drive = StarDrive::Spool {
                                target: system.system,
                                since: now,
                            };
                        }
                        output.push_event(flight_event(FlightEvent::WarpSpooling, player_id, 0));
                        PlayerScopedCommandOutcome::Applied
                    }
                    Err(refusal) => {
                        output.push_event(flight_event(
                            FlightEvent::WarpRefused,
                            player_id,
                            refusal.code(),
                        ));
                        PlayerScopedCommandOutcome::IgnoredUnavailable
                    }
                }
            }
            FlightCommand::Use => self.dock_or_undock(player_id, output),
            FlightCommand::Station(service) => self.station_service(player_id, service, output),
        }
    }

    /// The use button in space: undock, or dock at a port in reach.
    fn dock_or_undock(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let capacity = self.air_capacity_of(player_id);
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let flight = player.flight;
        let orientation = player.attitude.orientation;
        let bodies = self.space_bodies_near(flight.pos);
        if let FlightPlace::Docked(station) = flight.place {
            let away_from =
                bodies
                    .iter()
                    .find(|body| body.body == station)
                    .map_or((0.0, -1.0), |body| {
                        let (dx, dy) = body.centre.delta(flight.pos);
                        let r = crate::field::trig::hypot(dx, dy).max(1.0);
                        (dx / r, dy / r)
                    });
            if let Some(player) = self.players.get_mut(&player_id) {
                player.flight.place = FlightPlace::Approach(station);
                player.flight.velocity_x = away_from.0 * UNDOCK_SPEED;
                player.flight.velocity_y = away_from.1 * UNDOCK_SPEED;
            }
            output.push_event(flight_event(FlightEvent::Undocked, player_id, 0));
            return PlayerScopedCommandOutcome::Applied;
        }
        let port = bodies.iter().find_map(|body| {
            let port = body.port()?;
            (flight.pos.span(port) <= DOCK_RANGE
                && flight.speed() <= DOCK_SPEED
                && wrap_angle(orientation - body.dock_orientation()).abs() <= DOCK_ALIGN
                && !flight.drive.engaged())
            .then_some((body.body, port))
        });
        let Some((station, port)) = port else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            player.flight.place = FlightPlace::Docked(station);
            player.flight.pos = port;
            player.flight.velocity_x = 0.0;
            player.flight.velocity_y = 0.0;
            player.flight.drive = StarDrive::Off;
            player.flight.air = capacity;
            player.attitude.angular_velocity = 0.0;
        }
        output.push_event(flight_event(FlightEvent::Docked, player_id, 0));
        PlayerScopedCommandOutcome::Applied
    }

    /// A docked station's service (the stub Wave 12 fills): fuel, repairs, flight gear.
    fn station_service(
        &mut self,
        player_id: PlayerId,
        service: StationService,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        if let StationService::Hub(action) = service {
            return self.station_hub(player_id, action, output);
        }
        // The station's own prices (Wave 12); a derelict has no counter.
        let Some((fuel_price, repair_price)) = self.dock_counter(player_id, service, output) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if !matches!(player.flight.place, FlightPlace::Docked(_)) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "whole units of a bounded tank or hull"
        )]
        let applied = match service {
            StationService::Refuel => {
                let missing = (player.fuel_capacity - player.fuel).max(0.0).ceil() as u32;
                let units = missing.min(player.credits / fuel_price.max(1));
                player.credits -= units * fuel_price;
                player.fuel = (player.fuel + units as f32).min(player.fuel_capacity);
                units > 0
            }
            StationService::Repair => {
                let missing = (player.max_hull() - player.hull).max(0.0).ceil() as u32;
                let points = missing.min(player.credits / repair_price.max(1));
                player.credits -= points * repair_price;
                player.hull = (player.hull + points as f32).min(player.max_hull());
                points > 0
            }
            StationService::Buy(kind) => {
                let level = kind.level(&player.hazard_gear);
                let price = kind.price(level);
                if level >= kind.max_level() || player.credits < price {
                    false
                } else {
                    player.credits -= price;
                    let gear = &mut player.hazard_gear;
                    match kind {
                        FlightGearKind::NavSuite => gear.flight.nav_suite += 1,
                        FlightGearKind::ReentryShield => gear.flight.reentry_shield += 1,
                        FlightGearKind::Gyro => gear.attitude.gyro_stabiliser = 1,
                        FlightGearKind::HorizonHold => gear.attitude.horizon_hold = 1,
                        FlightGearKind::AutoLand => gear.flight.auto_land = 1,
                        FlightGearKind::MiningBeam => gear.flight.mining_beam = 1,
                    }
                    true
                }
            }
            StationService::Hub(_) => false,
        };
        if !applied {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        output.push_event(GameplayEventKind::PurchaseCompleted { player_id });
        PlayerScopedCommandOutcome::Applied
    }

    /// The scanner in space: a ping that finds every system whose star is in reach (a nebula's
    /// fog halves it) and shows the bodies around on the nav HUD.
    fn scanner_ping(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let now = self.millis_at(self.simulation_tick.next());
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if self
            .flight
            .pings
            .get(&player_id)
            .is_some_and(|ping| now.saturating_sub(ping.at) < PING_COOLDOWN_MILLIS)
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let centre = player.flight.pos;
        let tier = player.hazard_gear.flight.nav_suite;
        let space = self.space();
        let fogged = nav::in_nebula(&space, centre);
        // The xeno lens and its marks (Wave 12 stations, Wave 13 endgame).
        let lens = self.ship_effects(player_id).lens;
        // Wave 12 systems: the fog where the rig is cuts the reach further, by its thickness.
        let range = nav::ping_range(tier, fogged)
            * crate::flight::systems::fog_range_share(space.fog_at(centre))
            * lens;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "a handful of galactic sectors"
        )]
        let reach = (f64::from(range) / (space::GALAXY_SECTOR_TILES as f64 * f64::from(TILE_SIZE)))
            .ceil() as i64;
        let found: Vec<SystemRef> = space
            .systems_near(centre, reach.max(1))
            .into_iter()
            .filter(|system| centre.span(system.star) <= range)
            .map(|system| system.system)
            .collect();
        let mut new = 0_u32;
        for system in found {
            if !system.is_home() && self.flight.found(system) {
                new += 1;
            }
        }
        self.flight.pings.insert(
            player_id,
            ScanPing {
                at: now,
                centre,
                range,
                fogged,
            },
        );
        output.push_event(flight_event(FlightEvent::ScannerPing, player_id, new));
        // Bodies near the ping are scanned (Wave 12 stations: scan missions).
        self.note_ping_scans(player_id, centre, output);
        PlayerScopedCommandOutcome::Applied
    }

    /// Test mode's grant (Wave 11): the Star Drive on `player_id`'s rig and space open.
    pub(super) fn grant_star_drive(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        if !self.space_enabled() || !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        self.heart.space_unlocked = true;
        self.heart.star_drive.insert(player_id);
        PlayerScopedCommandOutcome::Applied
    }
}

/// Whether `pos` lies within free flight's store's ground (its tiles end where `i32` tile
/// coordinates do; farther out a rig flies parked in the home store as in Wave 11).
#[must_use]
pub fn within_space_store(pos: crate::position::WorldPos) -> bool {
    let reach = i64::from(crate::place::EmptySpaceGenerator::REACH / 2);
    let tile = crate::space::SpaceTile::of_pos(pos);
    tile.x.abs() < reach && tile.y.abs() < reach
}

/// Where a warp toward `star` from `from` drops out ([`WARP_ARRIVAL`] short of it).
#[must_use]
pub fn warp_arrival(
    from: crate::position::WorldPos,
    star: crate::position::WorldPos,
) -> crate::position::WorldPos {
    let (dx, dy) = from.delta_f64(star);
    let distance = crate::polar::trig::hypot(dx, dy).max(1.0);
    let keep = (distance - f64::from(WARP_ARRIVAL)).max(0.0) / distance;
    space::advance(from, dx * keep, dy * keep)
}

/// Whether a rig may warp toward `star` now; `Ok` holds the fuel it costs.
///
/// # Errors
///
/// Why not ([`WarpRefusal`]).
pub fn warp_check(
    flight: &RigFlight,
    attitude: RigAttitude,
    bodies: &[FlightBody],
    star: crate::position::WorldPos,
    fuel: f32,
) -> Result<f32, WarpRefusal> {
    if !matches!(flight.place, FlightPlace::Space)
        || matches!(flight.drive, StarDrive::Warp(_) | StarDrive::Transit(_))
    {
        return Err(WarpRefusal::NotFree);
    }
    if wrap_angle(attitude.orientation - nav::bearing(flight.pos, star)).abs() > WARP_ALIGN {
        return Err(WarpRefusal::NotAligned);
    }
    let clear = bodies
        .iter()
        .filter(|body| body.kind != BodyKind::Station)
        .all(|body| flight.pos.span(body.centre) - body.radius >= WARP_CLEARANCE);
    if !clear {
        return Err(WarpRefusal::GravityWell);
    }
    let arrival = warp_arrival(flight.pos, star);
    let cost = warp_fuel(f64::from(flight.pos.span(arrival)));
    if fuel < cost {
        return Err(WarpRefusal::Fuel);
    }
    Ok(cost)
}
