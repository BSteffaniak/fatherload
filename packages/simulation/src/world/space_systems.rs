//! Space's things in the world (Wave 12 systems, see [`crate::flight::systems`]): what a rig
//! in flight meets each tick after its flight step (a black hole's horizon, a wormhole's
//! mouth, stars' heat and flares, nebula fog, belt rocks and comet tails), the wormhole
//! transit, the Moon's handover and the crew's shared record.
//!
//! Everything reads the rig's exact space position, the world clock and the crew's record
//! ([`SystemsShared`]), so the host and every predicting client agree.

use crate::{
    event::{GameplayEventKind, TickOutput},
    flight::{
        FlightPlace, RIG_CONTACT, RigFlight, SPACE_THRUST, StarDrive, TransitRun, WarpRun,
        physics::{FlightOutcome, warp_position},
        space::{FlightBody, SpaceQuery},
        systems::{
            BEAM_FUEL_PER_SECOND, BEAM_MINE_SECONDS, BodyExtra, COMET_GATHER_SECONDS,
            COMET_HARVEST_CAP, COMET_MINERAL, FlarePhase, HoleWarning, KnownWormhole,
            MAX_MINED_RECORDS, ROCK_QUERY_TILES, SystemsEvent, SystemsShared, TRANSIT_MILLIS,
            asteroid_mineral, beam_target, contact_mine_seconds, drill_contact, flare_heat,
            hole_warning, in_comet_tail, transit_exit,
        },
    },
    ids::PlayerId,
    input::ControlState,
    position::WorldPos,
    terrain::TILE_SIZE,
};

use super::WorldState;

/// Fog (per mille) past which a rig counts as inside a nebula (the HUD's edge).
pub const NEBULA_EDGE_FOG: u16 = 120;
/// Bits of [`RigFlight::marks`].
const MARK_NEBULA: u8 = 1;
const MARK_HOLD_FULL: u8 = 1 << 2;
const MARK_STAR_HEAT: u8 = 1 << 3;
/// Bit of [`RigFlight::alerts`] while a flare's warning stands.
const ALERT_FLARE: u8 = 1 << 7;
/// Every hole warning's alert bit.
const ALERT_HOLES: u8 = (1 << 4) | (1 << 5) | (1 << 6);
/// A [`crate::flight::SystemRef::id`] marking a wormhole transit's run (no star system).
pub const TRANSIT_TARGET_ID: u64 = u64::MAX;

const fn systems_event(what: SystemsEvent, player_id: PlayerId, detail: u32) -> GameplayEventKind {
    GameplayEventKind::SpaceSystems {
        what,
        player_id,
        detail,
    }
}

impl WorldState {
    /// What the crew did to space's things: asteroids mined, comet dust, wormholes flown.
    #[must_use]
    pub const fn space_systems(&self) -> &SystemsShared {
        &self.space_systems
    }

    /// Replaces the crew's record (scenes, tests, tools).
    pub fn set_space_systems(&mut self, shared: SystemsShared) {
        self.space_systems = shared;
    }

    /// Every flight body near `pos` for a rig moving at `speed` px/s: the systems' bodies,
    /// black holes and wormhole mouths round it, and the belt rocks in reach the crew has not
    /// mined out.
    #[must_use]
    pub fn flight_bodies_near(&self, pos: WorldPos, speed: f32) -> Vec<FlightBody> {
        let space = self.space();
        let mut bodies = space.bodies_near(pos);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a tick's travel in tiles, clamped"
        )]
        let travel = (speed / TILE_SIZE * 0.25).clamp(0.0, 20_000.0) as i64;
        bodies.extend(space.rocks_near(pos, ROCK_QUERY_TILES + travel, &self.space_systems));
        bodies
    }

    /// How fast time runs at `pos` (1 but near a black hole; the HUD shows it).
    #[must_use]
    pub fn time_scale_at(&self, pos: WorldPos) -> f32 {
        if !self.space_enabled() {
            return 1.0;
        }
        self.space().space().time_scale_at(pos)
    }

    /// Nebula cover at `pos`, per mille: how hidden a ship there is from sensors.
    ///
    /// TODO-SEAM(w12-hostiles): pirates lurking in a nebula read this to hide (and to ambush
    /// from); the nav HUD already fades their markers with [`crate::flight::systems::
    /// marker_visibility`].
    #[must_use]
    pub fn nebula_cover(&self, pos: WorldPos) -> u16 {
        if !self.space_enabled() {
            return 0;
        }
        self.space().fog_at(pos)
    }

    /// A wormhole transit's step (a closed form of time): where the run puts the rig, and
    /// out of the far mouth when it is done. Returns whether it came out this tick.
    pub(super) fn step_transit(flight: &mut RigFlight, transit: TransitRun, now: u64) -> bool {
        let (pos, (vx, vy), done) = warp_position(&transit.run, now);
        flight.pos = pos;
        flight.velocity_x = vx;
        flight.velocity_y = vy;
        if !done {
            return false;
        }
        flight.drive = StarDrive::Off;
        flight.place = FlightPlace::Space;
        flight.velocity_x = transit.heading_x * crate::flight::systems::TRANSIT_EXIT_SPEED;
        flight.velocity_y = transit.heading_y * crate::flight::systems::TRANSIT_EXIT_SPEED;
        true
    }

    /// What space's things did to `player_id`'s rig after its flight step (see the module
    /// docs). Returns `true` when the rig left flight (rescued home or handed to the Moon).
    #[allow(
        clippy::too_many_lines,
        reason = "every kind of space thing in one ordered pass"
    )]
    pub(super) fn after_flight_step(
        &mut self,
        player_id: PlayerId,
        controls: ControlState,
        bodies: &[FlightBody],
        outcome: &FlightOutcome,
        marks_before: u8,
        output: &mut TickOutput,
    ) -> bool {
        let now = self.millis_at(self.simulation_tick.next());
        let seconds = self.timing.delta_seconds;
        let space = self.space();
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let flight = player.flight;
        // The event horizon: a fair rescue, never a game over.
        if outcome.swallowed.is_some() {
            self.hole_rescue(player_id, output);
            return true;
        }
        if outcome.reached_moon {
            self.return_to_moon(player_id, output);
            return true;
        }
        // Into a wormhole's mouth: the transit starts.
        if let Some(index) = outcome.mouth
            && let Some(BodyExtra::Mouth(mouth)) = bodies.get(index).map(|body| body.extra)
            && !matches!(flight.drive, StarDrive::Transit(_))
        {
            let (arrival, (hx, hy)) = transit_exit(&mouth, (flight.velocity_x, flight.velocity_y));
            let speed = crate::flight::systems::TRANSIT_EXIT_SPEED;
            let new = self.space_systems.found_wormhole(KnownWormhole {
                mouth: bodies[index].centre,
                sector: mouth.sector,
                exit: mouth.exit,
                exit_sector: mouth.exit_sector,
            });
            if let Some(player) = self.players.get_mut(&player_id) {
                player.flight.drive = StarDrive::Transit(TransitRun {
                    run: WarpRun {
                        origin: player.flight.pos,
                        arrival,
                        target: crate::flight::SystemRef {
                            id: TRANSIT_TARGET_ID,
                            sector: mouth.exit_sector,
                        },
                        start: now,
                        duration_millis: TRANSIT_MILLIS,
                    },
                    heading_x: hx / speed,
                    heading_y: hy / speed,
                });
                player.flight.place = FlightPlace::Space;
                player.attitude.angular_velocity = 0.0;
            }
            output.push_event(systems_event(
                SystemsEvent::WormholeEntered,
                player_id,
                u32::from(new),
            ));
            return false;
        }
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let orientation = player.attitude.orientation;
        let engine = crate::player::engine_power(player.engine_level);
        let drill_level = player.drill_strength;
        let beam = player.hazard_gear.flight.mining_beam > 0;
        let has_room = player.has_cargo_space();
        let infinite_fuel = self.test_mode.is_some_and(|state| state.infinite_fuel);
        let mut events: Vec<(SystemsEvent, u32)> = Vec::new();
        let mut flight = flight;
        // The parking hold took over.
        let holding = crate::flight::physics::MARK_HOLDING;
        if outcome.holding && marks_before & holding == 0 {
            events.push((SystemsEvent::ParkingHold, 0));
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small damage figure"
        )]
        if outcome.rock_damage >= 0.5 {
            events.push((SystemsEvent::RockBump, outcome.rock_damage.round() as u32));
        }
        // A star's radiance (edge, re-armed once it fades).
        if outcome.star_heat > crate::flight::HEAT_COOL_PER_SECOND {
            if flight.marks & MARK_STAR_HEAT == 0 {
                flight.marks |= MARK_STAR_HEAT;
                events.push((SystemsEvent::StarHeat, 0));
            }
        } else if outcome.star_heat < 0.1 {
            flight.marks &= !MARK_STAR_HEAT;
        }
        // Flares: the warning, then struck or sheltered when it bursts.
        let (_, near) = flare_heat(bodies, flight.pos, now);
        match near.map(|near| (near.phase, near.sheltered)) {
            Some((FlarePhase::Warning { in_millis }, _)) => {
                if flight.alerts & ALERT_FLARE == 0 {
                    flight.alerts |= ALERT_FLARE;
                    #[allow(clippy::cast_possible_truncation, reason = "seconds of a warning")]
                    events.push((SystemsEvent::FlareWarning, in_millis.div_ceil(1_000) as u32));
                }
            }
            Some((FlarePhase::Burst { .. }, sheltered)) => {
                if flight.alerts & ALERT_FLARE != 0 {
                    flight.alerts &= !ALERT_FLARE;
                    events.push((
                        if sheltered {
                            SystemsEvent::FlareSheltered
                        } else {
                            SystemsEvent::FlareStruck
                        },
                        0,
                    ));
                }
            }
            Some((FlarePhase::Quiet { .. }, _)) | None => flight.alerts &= !ALERT_FLARE,
        }
        // Black holes: three warnings, each once until the rig climbs back out of it.
        let thrust = crate::flight::SPACE_THRUST * engine;
        let level = hole_warning(bodies, flight.pos, thrust).map(|(level, _)| level);
        for warning in [
            HoleWarning::Well,
            HoleWarning::Strain,
            HoleWarning::NoReturn,
        ] {
            let bit = warning.alert_bit();
            if level.is_some_and(|level| level >= warning) {
                if flight.alerts & bit == 0 {
                    flight.alerts |= bit;
                    events.push((
                        match warning {
                            HoleWarning::Well => SystemsEvent::HoleWell,
                            HoleWarning::Strain => SystemsEvent::HoleStrain,
                            HoleWarning::NoReturn => SystemsEvent::HoleNoReturn,
                        },
                        0,
                    ));
                }
            } else {
                flight.alerts &= !bit;
            }
        }
        if level.is_none() {
            flight.alerts &= !ALERT_HOLES;
        }
        // Nebula fog (edge).
        let fog = space.fog_at(flight.pos);
        if fog >= NEBULA_EDGE_FOG {
            if flight.marks & MARK_NEBULA == 0 {
                flight.marks |= MARK_NEBULA;
                events.push((SystemsEvent::NebulaEntered, u32::from(fog)));
            }
        } else if fog < NEBULA_EDGE_FOG / 2 && flight.marks & MARK_NEBULA != 0 {
            flight.marks &= !MARK_NEBULA;
            events.push((SystemsEvent::NebulaLeft, 0));
        }
        // Mining a belt rock from flight: the drill against it, or the beam from the nose.
        let free = !flight.drive.engaged() && !matches!(flight.place, FlightPlace::Docked(_));
        let drilled = free
            .then(|| drill_contact(bodies, flight.pos, orientation, RIG_CONTACT))
            .flatten()
            .map(|index| (index, contact_mine_seconds(drill_level)));
        let beamed = (free && beam && controls.weapon.fire)
            .then(|| beam_target(bodies, flight.pos, orientation))
            .flatten()
            .map(|index| (index, BEAM_MINE_SECONDS));
        let mut fuel_burned = 0.0;
        if beam && controls.weapon.fire && free {
            fuel_burned = BEAM_FUEL_PER_SECOND.mul_add(seconds, fuel_burned);
        }
        let mut mined_unit = None;
        match drilled.or(beamed) {
            Some((index, unit_seconds)) => {
                if has_room {
                    flight.marks &= !MARK_HOLD_FULL;
                    flight.mining += seconds / unit_seconds.max(0.05);
                    if flight.mining >= 1.0 {
                        flight.mining -= 1.0;
                        if let BodyExtra::Rock(rock) = bodies[index].extra {
                            mined_unit = Some(rock);
                        }
                    }
                } else if flight.marks & MARK_HOLD_FULL == 0 {
                    flight.marks |= MARK_HOLD_FULL;
                    events.push((SystemsEvent::HoldFull, 0));
                }
            }
            None => flight.mining = 0.0,
        }
        // Comet dust in a tail.
        let mut dust = None;
        match in_comet_tail(bodies, flight.pos) {
            Some(index) if has_room => {
                if let BodyExtra::Comet(comet) = bodies[index].extra {
                    let taken = self
                        .space_systems
                        .comets
                        .get(&comet.key)
                        .copied()
                        .unwrap_or(0);
                    if taken < COMET_HARVEST_CAP {
                        flight.gathering += seconds / COMET_GATHER_SECONDS;
                        if flight.gathering >= 1.0 {
                            flight.gathering -= 1.0;
                            dust = Some((comet.key, taken + 1));
                        }
                    }
                }
            }
            _ => flight.gathering = 0.0,
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        player.flight = flight;
        if !infinite_fuel {
            player.fuel = (player.fuel - fuel_burned).max(0.0);
        }
        let mut cargo_changed = false;
        if let Some(rock) = mined_unit
            && player.add_cargo(asteroid_mineral(rock.ore))
        {
            cargo_changed = true;
            let taken = self.space_systems.mined.entry(rock.id).or_insert(0);
            *taken = taken.saturating_add(1);
            let depleted = *taken >= rock.yield_units;
            #[allow(clippy::cast_possible_truncation, reason = "seven ores")]
            let ore = crate::space::AsteroidOre::ALL
                .iter()
                .position(|ore| *ore == rock.ore)
                .unwrap_or(0) as u32;
            events.push((SystemsEvent::OreMined, ore));
            if depleted {
                events.push((SystemsEvent::AsteroidDepleted, 0));
            }
            // Past its bound the record drops its lowest-keyed rock (it regrows): a
            // deterministic choice, not the oldest (the record keeps no ages).
            if self.space_systems.mined.len() > MAX_MINED_RECORDS
                && let Some(lowest) = self.space_systems.mined.keys().next().copied()
                && lowest != rock.id
            {
                self.space_systems.mined.remove(&lowest);
            }
        }
        if let Some((key, taken)) = dust
            && let Some(player) = self.players.get_mut(&player_id)
            && player.add_cargo(COMET_MINERAL)
        {
            cargo_changed = true;
            self.space_systems.comets.insert(key, taken);
            events.push((SystemsEvent::CometDust, u32::from(taken)));
        }
        for (what, detail) in events {
            output.push_event(systems_event(what, player_id, detail));
        }
        if cargo_changed {
            output.push_event(GameplayEventKind::CargoChanged { player_id });
        }
        false
    }

    /// A rig that crossed a black hole's event horizon: the beacon's tug pulls it out and home,
    /// for the beacon's fee, with the hold's ore and finds lost to the hole. Plenty of warning
    /// came first ([`HoleWarning`]); never a game over.
    fn hole_rescue(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let fee = self
            .players
            .get(&player_id)
            .map_or(0, |player| crate::flight::beacon_fee(player.credits));
        if let Some(player) = self.players.get_mut(&player_id) {
            player.cargo_bay.clear_ore_and_artifacts();
        }
        self.emergency_beacon(player_id, output);
        output.push_event(systems_event(SystemsEvent::HoleRescued, player_id, fee));
        output.push_event(GameplayEventKind::CargoChanged { player_id });
    }

    /// Time dilation for a rig away: its own clock's lag behind the crew's grows by what the
    /// hole slows it (Wave 12 systems). Returns the time scale (life support drains by it).
    pub(super) fn dilate(&mut self, player_id: PlayerId, seconds: f32) -> f32 {
        let Some(pos) = self.players.get(&player_id).map(|player| player.flight.pos) else {
            return 1.0;
        };
        let scale = self.time_scale_at(pos);
        if scale < 1.0
            && let Some(player) = self.players.get_mut(&player_id)
        {
            player.flight.lag_seconds = (1.0 - scale).mul_add(seconds, player.flight.lag_seconds);
        }
        scale
    }

    /// Where `player_id`'s space position comes down on the home world, as a home column: the
    /// arc round the round home world from the departure's column (straight "up" is the
    /// column the Moon hangs over), so a rig comes home wherever it chose to approach.
    #[must_use]
    pub fn home_column_under(&self, pos: WorldPos) -> Option<i32> {
        let settings = self.home_settings();
        let moon_column = crate::worldgen::sky::moon_column(&settings)?;
        let (dx, dy) = crate::flight::space::home_centre().delta_f64(pos);
        let angle = crate::polar::trig::atan2(dx, -dy);
        let arc = angle * f64::from(crate::flight::space::HOME_RADIUS_TILES);
        #[allow(clippy::cast_possible_truncation, reason = "a column within the world")]
        let column = (f64::from(moon_column) + arc).round() as i64;
        let limit = i64::from(settings.generator.column_limit()) - 64;
        i32::try_from(column.clamp(-limit, limit)).ok()
    }

    /// Where on the Moon a rig coming down to it from space arrives (home column, row): over
    /// the point of the Moon under it (the round Moon's upper half maps across its width; from
    /// below it comes in under the Moon).
    #[must_use]
    pub fn moon_arrival(&self, pos: WorldPos) -> Option<(i32, i32)> {
        let settings = self.home_settings();
        let moon = crate::worldgen::sky::moon(&settings)?;
        let (dx, dy) = crate::flight::space::moon_centre().delta_f64(pos);
        let distance = crate::polar::trig::hypot(dx, dy).max(1.0);
        let across = (dx / distance).clamp(-1.0, 1.0);
        #[allow(clippy::cast_possible_truncation, reason = "within the Moon's width")]
        let column = moon.center_x + (across * f64::from(moon.half_width) * 0.85).round() as i32;
        let row = if dy <= 0.0 {
            moon.top_row - MOON_ARRIVAL_ABOVE_ROWS
        } else {
            moon.top_row + moon.depth + MOON_ARRIVAL_ABOVE_ROWS
        };
        Some((column, row))
    }

    /// `player_id`'s rig came down to the Moon from space: into the home sky over the point of
    /// the Moon it approached, turned and moving as it came in.
    fn return_to_moon(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let pos = player.flight.pos;
        let Some((column, row)) = self.moon_arrival(pos) else {
            return;
        };
        self.return_home_at(
            player_id,
            crate::flight::space::moon_centre(),
            column,
            row,
            output,
        );
        output.push_event(systems_event(SystemsEvent::MoonApproach, player_id, 0));
    }

    /// `player_id`'s rig's speed limit along `SPACE_THRUST` (the HUD's no-return ring).
    #[must_use]
    pub fn rig_thrust(&self, player_id: PlayerId) -> f32 {
        self.players.get(&player_id).map_or(SPACE_THRUST, |player| {
            SPACE_THRUST * crate::player::engine_power(player.engine_level)
        })
    }
}

/// Rows over the Moon's top (or under its bottom) a rig coming down to it arrives at.
pub const MOON_ARRIVAL_ABOVE_ROWS: i32 = 30;
