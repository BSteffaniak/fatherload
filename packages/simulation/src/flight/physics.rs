//! One tick of flight in space: rotation, thrust, cruise, gravity, contact and heat.
//!
//! These are pure functions of the rig's flight state, its controls and the bodies around it
//! (landing, scraping, a star's burn, re-entry heat), integrated in fixed internal steps of at
//! most [`FLIGHT_STEP_SECONDS`], so every tick rate agrees.
//!
//! Warp positions are a closed form of time ([`warp_position`]), not integrated.

use crate::{
    attitude::{AttitudeContext, RigAttitude, step_attitude},
    field::trig::{atan2, sin_cos, wrap_angle},
    position::WorldPos,
    terrain::TILE_SIZE,
};

use super::{
    BOUNCE_KEEP, CRASH_DAMAGE_SCALE, FlightControls, FlightEvent, FlightPlace, HEAT_BURN,
    HEAT_COOL_PER_SECOND, HEAT_GAIN, HEAT_HULL_DAMAGE, HEAT_MAX, IMPULSE_DECELERATION,
    IMPULSE_SPEED_PER_POWER, IMPULSE_TOP_SPEED, LANDING_SAFE_SPEED, LANDING_TILT, MAX_SPACE_SPIN,
    REENTRY_SAFE_SPEED, REVERSE_SHARE, RIG_CONTACT, ROTATION_FUEL_SHARE, RigFlight, SCRAPE_DAMAGE,
    SHIELD_SAFE_SHARE, SPACE_FUEL_PER_SECOND, SPACE_THRUST, STRAFE_SHARE, StarDrive,
    TANGENTIAL_HEAT_SHARE, TIP_SPIN, WarpRun,
    space::{BodyKind, FlightBody, GALAXY_SECTOR_TILES, advance, gravity_at, nearest_surface},
    systems::{self, ROCK_DAMAGE_MAX, ROCK_DAMAGE_SCALE, ROCK_SAFE_SPEED},
};

/// Longest internal integration step (240 Hz, as the attitude's).
pub const FLIGHT_STEP_SECONDS: f32 = 1.0 / 240.0;

/// Cruise: speed is the distance to the nearest surface over this many seconds, so it ramps
/// up leaving a body and down approaching one (a system crossing takes a minute or two).
pub const CRUISE_TAU_SECONDS: f32 = 10.0;
/// Slowest and fastest cruise (px/s; the top is 20,000 tiles a second).
pub const CRUISE_MIN_SPEED: f32 = 600.0;
pub const CRUISE_MAX_SPEED: f32 = 20_000.0 * TILE_SIZE;
/// Cruise always gains at least this much speed a second while the throttle is open (px/s²).
pub const CRUISE_MIN_ACCELERATION: f32 = 500.0;
/// The retro thrusters halve a cruise speed in about this many seconds.
pub const CRUISE_BRAKE_SECONDS: f32 = 1.5;
/// Fuel a second while cruising.
pub const CRUISE_FUEL_PER_SECOND: f32 = 2.0;

/// Warp: the spool before the jump (ms), the jump's base time and its time per galactic
/// sector, and its longest (ms).
pub const WARP_SPOOL_MILLIS: u64 = 2_000;
pub const WARP_BASE_MILLIS: u32 = 6_000;
pub const WARP_MILLIS_PER_SECTOR: u32 = 400;
pub const WARP_MAX_MILLIS: u32 = 30_000;
/// The heading must be within this of the target's bearing to warp (radians, 6 degrees), and
/// turning further than [`WARP_ABORT_ANGLE`] breaks a spool or a warp.
pub const WARP_ALIGN: f32 = 0.105;
pub const WARP_ABORT_ANGLE: f32 = 0.44;
/// No surface may be nearer than this to spool a warp (px, 5,000 tiles); a warp passing within
/// half of it of a body outside the target system is pulled out.
pub const WARP_CLEARANCE: f32 = 5_000.0 * TILE_SIZE;
/// A warp drops out this far from the target star (outside its outermost orbit; px).
pub const WARP_ARRIVAL: f32 = 200_000.0 * TILE_SIZE;
/// Fuel a warp costs: a base plus a share per galactic sector crossed.
pub const WARP_FUEL_BASE: f32 = 60.0;
pub const WARP_FUEL_PER_SECTOR: f32 = 8.0;

/// Auto-land takes over below this altitude over a landable surface (px), holding the descent
/// to [`AUTO_LAND_SPEED`].
pub const AUTO_LAND_ALTITUDE: f32 = 10.0 * TILE_SIZE;
pub const AUTO_LAND_SPEED: f32 = 150.0;
/// Gravity under this (px/s²) gives no "down" (horizon hold has nothing to level to).
pub const NO_DOWN_GRAVITY: f32 = 2.0;
/// A star's surface burns the hull this much a second (it pushes the rig back out).
pub const STAR_SURFACE_DAMAGE: f32 = 40.0;
/// The parking hold (Wave 12 systems).
///
/// An idle rig in open space where gravity is under [`PARK_HOLD_MAX_GRAVITY`] (px/s², about a quarter g) and slower than
/// [`PARK_HOLD_SPEED`] holds station on its translation thrusters (its turn stays the
/// player's): it never drifts back home from the departure point. Any thrust lets go.
pub const PARK_HOLD_MAX_GRAVITY: f32 = 110.0;
pub const PARK_HOLD_SPEED: f32 = 30.0;
/// How fast a holding rig's drift dies away (per second).
pub const PARK_HOLD_DAMPING: f32 = 3.0;
/// The Moon hands a rig over to the home sky over it this far above its surface (px): its
/// approach sphere only zooms the view in (Wave 12 systems: no snap home on a fly-by).
pub const MOON_LAND_HEIGHT: f32 = 60.0 * TILE_SIZE;
/// Bit of [`RigFlight::marks`] while the parking hold holds.
pub const MARK_HOLDING: u8 = 1 << 1;

/// The seat's controls for one tick of flight.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlightInput {
    /// Main (positive) and retro (negative) thrusters, `-1..=1`.
    pub forward: f32,
    /// Side thrusters, `-1..=1` (positive to the craft's right).
    pub strafe: f32,
    /// Rotation thrusters, `-1..=1` (clockwise positive).
    pub rotate: f32,
    pub controls: FlightControls,
    /// Gyro stabiliser and horizon hold (fitted and switched on).
    pub gyro: bool,
    pub horizon: bool,
}

/// What the rig brings to one tick of flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightRig {
    /// The engine's power (with the cargo load and parts, as on the home world).
    pub engine_power: f32,
    /// Share of the cargo bay used (0-1): a laden rig spins slower.
    pub cargo_ratio: f32,
    /// Re-entry shield tier.
    pub shield: u8,
    /// Auto-land fitted.
    pub auto_land: bool,
    /// Fuel aboard at the start of the tick.
    pub fuel: f32,
    /// The world's millisecond at the end of the tick (stars' flares, Wave 12 systems).
    pub now_millis: u64,
}

/// What one tick of flight did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlightOutcome {
    pub fuel_burned: f32,
    pub hull_damage: f32,
    /// Edges, in order, with a detail value.
    pub events: Vec<(FlightEvent, u32)>,
    /// The rig flew into the home world's approach sphere (hand it back to the home sky).
    pub reached_home: bool,
    /// The rig came down to the Moon (hand it back to the home sky over the Moon; Wave 12).
    pub reached_moon: bool,
    /// The black hole (index in the body list) whose horizon the rig crossed (Wave 12).
    pub swallowed: Option<usize>,
    /// The wormhole mouth (index in the body list) the rig flew into (Wave 12).
    pub mouth: Option<usize>,
    /// The parking hold held the rig this tick (Wave 12).
    pub holding: bool,
    /// A bump against a rock dented the hull this much (Wave 12).
    pub rock_damage: f32,
    /// Heat a second from stars' radiance and flares at the end of the tick (Wave 12).
    pub star_heat: f32,
}

impl FlightOutcome {
    fn push(&mut self, event: FlightEvent, detail: u32) {
        if !self.events.iter().any(|(seen, _)| *seen == event) {
            self.events.push((event, detail));
        }
    }
}

/// The orientation whose up axis points along `(x, y)` (radians, clockwise from up).
#[must_use]
pub fn up_angle(x: f32, y: f32) -> f32 {
    atan2(x, -y)
}

/// The rig's up and right axes at `orientation`.
#[must_use]
pub fn axes(orientation: f32) -> ((f32, f32), (f32, f32)) {
    let (sin, cos) = sin_cos(orientation);
    ((sin, -cos), (cos, sin))
}

/// Cruise speed `gap` px from the nearest surface allows.
#[must_use]
pub fn cruise_limit(gap: f32) -> f32 {
    (gap / CRUISE_TAU_SECONDS).clamp(CRUISE_MIN_SPEED, CRUISE_MAX_SPEED)
}

/// Top impulse speed with `engine_power`.
#[must_use]
pub fn impulse_top_speed(engine_power: f32) -> f32 {
    (engine_power - 1.0)
        .max(0.0)
        .mul_add(IMPULSE_SPEED_PER_POWER, IMPULSE_TOP_SPEED)
}

/// Galactic sectors in `distance` px.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a power of two")]
pub fn sectors_in(distance: f64) -> f64 {
    distance / (GALAXY_SECTOR_TILES as f64 * f64::from(TILE_SIZE))
}

/// How long a warp across `distance` px takes (ms).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded milliseconds"
)]
pub fn warp_duration_millis(distance: f64) -> u32 {
    let extra = (sectors_in(distance) * f64::from(WARP_MILLIS_PER_SECTOR)).max(0.0);
    (f64::from(WARP_BASE_MILLIS) + extra).min(f64::from(WARP_MAX_MILLIS)) as u32
}

/// Fuel a warp across `distance` px costs.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "a bounded fuel amount")]
pub fn warp_fuel(distance: f64) -> f32 {
    (sectors_in(distance) as f32).mul_add(WARP_FUEL_PER_SECTOR, WARP_FUEL_BASE)
}

/// The warp's eased progress at `share` (0-1) of its time: slow out, fast across, slow in.
#[must_use]
pub fn warp_ease(share: f64) -> f64 {
    let t = share.clamp(0.0, 1.0);
    t * t * 2.0f64.mul_add(-t, 3.0)
}

/// Where a warp is at world millisecond `now`, its velocity (px/s) and whether it is done.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "a velocity as f32")]
#[allow(
    clippy::cast_precision_loss,
    reason = "elapsed milliseconds of a short warp"
)]
pub fn warp_position(run: &WarpRun, now: u64) -> (WorldPos, (f32, f32), bool) {
    let duration = f64::from(run.duration_millis.max(1));
    let elapsed = now.saturating_sub(run.start) as f64;
    let share = (elapsed / duration).min(1.0);
    let (dx, dy) = run.origin.delta_f64(run.arrival);
    let eased = warp_ease(share);
    let pos = advance(run.origin, dx * eased, dy * eased);
    // d/dt of the ease: 6 t (1 - t) / duration (seconds).
    let rate = if share >= 1.0 {
        0.0
    } else {
        6.0 * share * (1.0 - share) / (duration / 1_000.0)
    };
    (pos, ((dx * rate) as f32, (dy * rate) as f32), share >= 1.0)
}

/// Advances `flight` and `attitude` one tick of `seconds` (see the module docs). Docked rigs
/// and warps do not move here (a warp's position is [`warp_position`]).
#[allow(
    clippy::too_many_lines,
    reason = "one substep keeps rotation, thrust, gravity, contact and heat in order"
)]
pub fn step_flight(
    flight: &mut RigFlight,
    attitude: &mut RigAttitude,
    input: &FlightInput,
    rig: &FlightRig,
    bodies: &[FlightBody],
    seconds: f32,
) -> FlightOutcome {
    let mut outcome = FlightOutcome::default();
    if seconds <= 0.0 || matches!(flight.place, FlightPlace::Docked(_)) {
        if matches!(flight.place, FlightPlace::Docked(_)) {
            attitude.angular_velocity = 0.0;
            flight.velocity_x = 0.0;
            flight.velocity_y = 0.0;
        }
        return outcome;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a bounded handful of internal steps"
    )]
    let steps = ((seconds / FLIGHT_STEP_SECONDS).ceil() as u32).clamp(1, 64);
    #[allow(clippy::cast_precision_loss, reason = "a handful of steps")]
    let dt = seconds / steps as f32;
    let inertia = super::rotation_inertia(rig.cargo_ratio);
    let shield = f32::from(rig.shield);
    let top_speed = impulse_top_speed(rig.engine_power);
    for _ in 0..steps {
        let has_fuel = rig.fuel - outcome.fuel_burned > 0.0;
        let ((gx, gy), strongest) = gravity_at(bodies, flight.pos);
        let g_strength = crate::field::trig::hypot(gx, gy);
        let down_body = strongest.map(|index| &bodies[index]);
        let local_up = down_body.map(|body| {
            let (dx, dy) = body.centre.delta(flight.pos);
            up_angle(dx, dy)
        });
        let landed = matches!(flight.place, FlightPlace::Landed(_));
        // Auto-land: low over a landable surface, descending, fitted and switched on.
        let (height, radial_in) = down_body.map_or((f32::INFINITY, 0.0), |body| {
            let (dx, dy) = body.centre.delta(flight.pos);
            let r = crate::field::trig::hypot(dx, dy).max(1.0);
            let v_out = flight.velocity_y.mul_add(dy, flight.velocity_x * dx) / r;
            (r - body.radius - RIG_CONTACT, -v_out)
        });
        let auto = rig.auto_land
            && input.controls.auto_land
            && !landed
            && down_body.is_some_and(FlightBody::landable)
            && height < AUTO_LAND_ALTITUDE
            && radial_in > 0.0
            && !flight.drive.engaged();
        // Rotation: the player's thrusters, no lean limit, no angular drag in vacuum.
        let rotate = if has_fuel { input.rotate } else { 0.0 };
        let has_down = g_strength > NO_DOWN_GRAVITY;
        *attitude = step_attitude(
            *attitude,
            &AttitudeContext {
                level: local_up
                    .filter(|_| has_down)
                    .unwrap_or(attitude.orientation),
                limit: None,
                resting: landed,
                input: rotate,
                gyro: input.gyro,
                horizon: (input.horizon || auto) && has_down,
                seconds: dt,
                drag_scale: 0.0,
                inertia,
                max_spin: MAX_SPACE_SPIN,
            },
        );
        outcome.fuel_burned = (rotate.abs() * SPACE_FUEL_PER_SECOND * ROTATION_FUEL_SHARE)
            .mul_add(dt, outcome.fuel_burned);
        let (up, right) = axes(attitude.orientation);
        match flight.drive {
            StarDrive::Warp(_) | StarDrive::Transit(_) => continue,
            StarDrive::Cruise { speed } => {
                let (gap, _) = nearest_surface(bodies, flight.pos);
                let limit = cruise_limit(gap);
                let mut speed = if input.forward > 0.0 {
                    (speed / CRUISE_TAU_SECONDS + CRUISE_MIN_ACCELERATION).mul_add(dt, speed)
                } else if input.forward < 0.0 {
                    (speed / CRUISE_BRAKE_SECONDS + CRUISE_MIN_ACCELERATION).mul_add(-dt, speed)
                } else {
                    speed
                };
                speed = speed.min(limit).max(0.0);
                flight.velocity_x = up.0 * speed;
                flight.velocity_y = up.1 * speed;
                flight.pos = flight
                    .pos
                    .offset(flight.velocity_x * dt, flight.velocity_y * dt);
                outcome.fuel_burned = CRUISE_FUEL_PER_SECOND.mul_add(dt, outcome.fuel_burned);
                flight.drive = StarDrive::Cruise { speed };
                // Into a body's approach sphere, or braked to a crawl: the thrusters take over.
                let inside = bodies
                    .iter()
                    .any(|body| flight.pos.span(body.centre) < body.approach);
                if inside || (input.forward < 0.0 && speed <= CRUISE_MIN_SPEED) {
                    flight.drive = StarDrive::Off;
                    let speed = flight.speed();
                    if speed > top_speed {
                        let scale = top_speed / speed;
                        flight.velocity_x *= scale;
                        flight.velocity_y *= scale;
                    }
                    outcome.push(FlightEvent::CruiseDropped, 0);
                }
            }
            StarDrive::Off | StarDrive::Spool { .. } => {
                let spooling = matches!(flight.drive, StarDrive::Spool { .. });
                let push = SPACE_THRUST * rig.engine_power;
                let (mut ax, mut ay) = (0.0, 0.0);
                if has_fuel && !spooling {
                    let forward = if input.forward < 0.0 {
                        input.forward * REVERSE_SHARE
                    } else {
                        input.forward
                    };
                    let strafe = input.strafe * STRAFE_SHARE;
                    let (tx, ty) = if input.controls.screen_relative {
                        (strafe, -forward)
                    } else {
                        (
                            up.0.mul_add(forward, right.0 * strafe),
                            up.1.mul_add(forward, right.1 * strafe),
                        )
                    };
                    ax = tx * push;
                    ay = ty * push;
                    outcome.fuel_burned = (input.strafe.abs().mul_add(0.5, input.forward.abs())
                        * SPACE_FUEL_PER_SECOND)
                        .mul_add(dt, outcome.fuel_burned);
                }
                if auto && radial_in > AUTO_LAND_SPEED {
                    // The last few metres: hold the descent to a safe speed.
                    if let Some(body) = down_body {
                        let (dx, dy) = body.centre.delta(flight.pos);
                        let r = crate::field::trig::hypot(dx, dy).max(1.0);
                        let brake = ((radial_in - AUTO_LAND_SPEED) / dt).min(push);
                        ax = (dx / r).mul_add(brake, ax);
                        ay = (dy / r).mul_add(brake, ay);
                        outcome.fuel_burned =
                            SPACE_FUEL_PER_SECOND.mul_add(dt, outcome.fuel_burned);
                    }
                }
                // The parking hold: idle, slow, in open space with little pull.
                let idle = input.forward == 0.0 && input.strafe == 0.0 && !spooling;
                // Open space, or a station's sphere: never a world's, a hole's or a mouth's.
                let open = matches!(flight.place, FlightPlace::Space)
                    || matches!(flight.place, FlightPlace::Approach(body) if body.index >= super::space::STATION_INDEX && body.index < super::space::COMET_INDEX);
                let holding_before = flight.marks & MARK_HOLDING != 0;
                let slow = flight.speed()
                    < if holding_before {
                        PARK_HOLD_SPEED * 2.0
                    } else {
                        PARK_HOLD_SPEED
                    };
                if idle && open && slow && g_strength < PARK_HOLD_MAX_GRAVITY && !landed {
                    let keep = crate::field::trig::exp_neg(PARK_HOLD_DAMPING * dt);
                    flight.velocity_x *= keep;
                    flight.velocity_y *= keep;
                    flight.marks |= MARK_HOLDING;
                    outcome.holding = true;
                    flight.pos = flight
                        .pos
                        .offset(flight.velocity_x * dt, flight.velocity_y * dt);
                    contact(flight, attitude, bodies, dt, &mut outcome);
                    heat(flight, bodies, shield, rig.now_millis, dt, &mut outcome);
                    continue;
                }
                flight.marks &= !MARK_HOLDING;
                if landed {
                    let Some(body) = down_body else { continue };
                    let (dx, dy) = body.centre.delta(flight.pos);
                    let r = crate::field::trig::hypot(dx, dy).max(1.0);
                    let lift = (ay + gy).mul_add(dy, (ax + gx) * dx) / r;
                    if lift <= 0.0 {
                        flight.velocity_x = 0.0;
                        flight.velocity_y = 0.0;
                        continue;
                    }
                }
                flight.velocity_x = (ax + gx).mul_add(dt, flight.velocity_x);
                flight.velocity_y = (ay + gy).mul_add(dt, flight.velocity_y);
                let speed = flight.speed();
                if speed > top_speed {
                    let slowed = IMPULSE_DECELERATION.mul_add(-dt, speed).max(top_speed);
                    let scale = slowed / speed;
                    flight.velocity_x *= scale;
                    flight.velocity_y *= scale;
                }
                flight.pos = flight
                    .pos
                    .offset(flight.velocity_x * dt, flight.velocity_y * dt);
            }
        }
        contact(flight, attitude, bodies, dt, &mut outcome);
        heat(flight, bodies, shield, rig.now_millis, dt, &mut outcome);
        if outcome.swallowed.is_some() || outcome.mouth.is_some() {
            break;
        }
    }
    if !outcome.holding {
        flight.marks &= !MARK_HOLDING;
    }
    update_place(flight, bodies, &mut outcome);
    outcome
}

/// Touching bodies: landing, scraping, a star's burn, a station's hull.
fn contact(
    flight: &mut RigFlight,
    attitude: &mut RigAttitude,
    bodies: &[FlightBody],
    dt: f32,
    outcome: &mut FlightOutcome,
) {
    for (index, body) in bodies.iter().enumerate() {
        let (dx, dy) = body.centre.delta(flight.pos);
        let r = crate::field::trig::hypot(dx, dy);
        let floor = body.radius + RIG_CONTACT;
        // A wormhole's mouth swallows the rig's centre, not its hull's edge; a black hole's
        // horizon likewise (Wave 12 systems).
        if matches!(body.kind, BodyKind::Wormhole | BodyKind::BlackHole) {
            if r < body.radius {
                if body.kind == BodyKind::Wormhole {
                    outcome.mouth.get_or_insert(index);
                } else {
                    outcome.swallowed.get_or_insert(index);
                }
            }
            continue;
        }
        if r >= floor || r <= 0.0 {
            continue;
        }
        let (nx, ny) = (dx / r, dy / r);
        let v_out = flight.velocity_x.mul_add(nx, flight.velocity_y * ny);
        let (tx, ty) = (
            nx.mul_add(-v_out, flight.velocity_x),
            ny.mul_add(-v_out, flight.velocity_y),
        );
        // Back onto the surface.
        flight.pos = body.centre.offset(nx * floor, ny * floor);
        match body.kind {
            BodyKind::Planet => {
                if matches!(flight.place, FlightPlace::Landed(_)) || v_out >= 0.0 {
                    continue;
                }
                let impact = -v_out;
                let tilt = wrap_angle(attitude.orientation - up_angle(nx, ny));
                if impact <= LANDING_SAFE_SPEED && tilt.abs() <= LANDING_TILT {
                    flight.velocity_x = 0.0;
                    flight.velocity_y = 0.0;
                    attitude.angular_velocity = 0.0;
                    flight.place = FlightPlace::Landed(body.body);
                    flight.drive = StarDrive::Off;
                    outcome.push(FlightEvent::Landed, 0);
                } else {
                    let mut damage = (impact - LANDING_SAFE_SPEED).max(0.0) * CRASH_DAMAGE_SCALE;
                    if tilt.abs() > LANDING_TILT {
                        // Touching down tilted scrapes the hull and tips the rig over.
                        damage = (impact / 100.0).mul_add(SCRAPE_DAMAGE, damage);
                        attitude.angular_velocity += TIP_SPIN.copysign(tilt);
                    }
                    outcome.hull_damage += damage;
                    let back = impact * BOUNCE_KEEP;
                    flight.velocity_x = nx.mul_add(back, tx * 0.6);
                    flight.velocity_y = ny.mul_add(back, ty * 0.6);
                    if damage > 0.0 {
                        #[allow(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "a small damage figure"
                        )]
                        outcome.push(FlightEvent::HullScraped, damage.round() as u32);
                    }
                }
            }
            BodyKind::Star | BodyKind::Home => {
                if v_out < 0.0 {
                    flight.velocity_x = nx.mul_add(-v_out, tx);
                    flight.velocity_y = ny.mul_add(-v_out, ty);
                }
                flight.drive = StarDrive::Off;
                if body.kind == BodyKind::Star {
                    outcome.hull_damage = STAR_SURFACE_DAMAGE.mul_add(dt, outcome.hull_damage);
                    flight.heat = HEAT_MAX;
                }
            }
            BodyKind::Station => {
                if v_out < 0.0 {
                    flight.velocity_x = tx;
                    flight.velocity_y = ty;
                }
            }
            BodyKind::Asteroid | BodyKind::Comet => {
                // A rock (or a comet's nucleus) is solid: the rig rests against it, or bounces
                // off a hard bump with a dent; a cruise or warp drops out.
                if v_out < 0.0 {
                    let impact = -v_out;
                    if impact > ROCK_SAFE_SPEED {
                        let damage =
                            ((impact - ROCK_SAFE_SPEED) * ROCK_DAMAGE_SCALE).min(ROCK_DAMAGE_MAX);
                        outcome.hull_damage += damage;
                        outcome.rock_damage += damage;
                        let back = impact * BOUNCE_KEEP;
                        flight.velocity_x = nx.mul_add(back, tx * 0.6);
                        flight.velocity_y = ny.mul_add(back, ty * 0.6);
                    } else {
                        flight.velocity_x = tx;
                        flight.velocity_y = ty;
                    }
                }
                if flight.drive.engaged() {
                    flight.drive = StarDrive::Off;
                    outcome.push(FlightEvent::CruiseDropped, 0);
                }
            }
            BodyKind::BlackHole | BodyKind::Wormhole => {}
        }
    }
}

/// Re-entry and corona heat for one substep.
fn heat(
    flight: &mut RigFlight,
    bodies: &[FlightBody],
    shield: f32,
    now: u64,
    dt: f32,
    outcome: &mut FlightOutcome,
) {
    // A star's radiance past its heat radius and its flares (Wave 12 systems); the re-entry
    // shield turns them aside too.
    let (flare, _) = systems::flare_heat(bodies, flight.pos, now);
    let star = (systems::radiant_heat(bodies, flight.pos) + flare) / (1.0 + shield);
    outcome.star_heat = star;
    let mut rate = star;
    for body in bodies {
        if body.atmosphere <= 0.0 {
            continue;
        }
        let (dx, dy) = body.centre.delta(flight.pos);
        let r = crate::field::trig::hypot(dx, dy);
        let top = body.radius + body.atmosphere;
        if r >= top || r <= 0.0 {
            continue;
        }
        let density = (1.0 - (r - body.radius) / body.atmosphere).clamp(0.0, 1.0);
        let v_out = flight.velocity_y.mul_add(dy, flight.velocity_x * dx) / r;
        let speed_sq = flight
            .velocity_x
            .mul_add(flight.velocity_x, flight.velocity_y * flight.velocity_y);
        let tangential = v_out.mul_add(-v_out, speed_sq).max(0.0).sqrt();
        let effective = (-v_out).max(0.0) + tangential * TANGENTIAL_HEAT_SHARE;
        let safe = REENTRY_SAFE_SPEED * shield.mul_add(SHIELD_SAFE_SHARE, 1.0);
        let gain = HEAT_GAIN / (1.0 + shield);
        rate += ((effective - safe).max(0.0) * gain)
            .mul_add(density, body.corona_heat * density / (1.0 + shield));
    }
    if rate > 0.0 {
        flight.heat = rate.mul_add(dt, flight.heat).min(HEAT_MAX);
    } else {
        flight.heat = HEAT_COOL_PER_SECOND.mul_add(-dt, flight.heat).max(0.0);
    }
    if flight.heat > HEAT_BURN {
        outcome.hull_damage =
            ((flight.heat - HEAT_BURN) * HEAT_HULL_DAMAGE).mul_add(dt, outcome.hull_damage);
    }
}

/// Which body's approach sphere the rig is in, after the tick's moves.
fn update_place(flight: &mut RigFlight, bodies: &[FlightBody], outcome: &mut FlightOutcome) {
    match flight.place {
        FlightPlace::Docked(_) | FlightPlace::Home => {}
        FlightPlace::Landed(body) => {
            let Some(found) = bodies.iter().find(|found| found.body == body) else {
                flight.place = FlightPlace::Space;
                return;
            };
            let r = flight.pos.span(found.centre);
            if r > found.radius + RIG_CONTACT + 2.0 {
                flight.place = FlightPlace::Approach(body);
                outcome.push(FlightEvent::TookOff, 0);
            }
        }
        FlightPlace::Space | FlightPlace::Approach(_) => {
            // The smallest approach sphere the rig is inside (a station near a world wins).
            let inside = bodies
                .iter()
                .filter(|body| flight.pos.span(body.centre) < body.approach)
                .min_by(|a, b| a.approach.total_cmp(&b.approach));
            let next = inside.map_or(FlightPlace::Space, |body| FlightPlace::Approach(body.body));
            if let Some(body) = inside
                && body.kind == BodyKind::Home
            {
                if body.body == super::space::MOON_BODY_REF {
                    // The Moon's sphere only zooms in: coming down to it lands on it.
                    if flight.pos.span(body.centre) < body.radius + MOON_LAND_HEIGHT {
                        outcome.reached_moon = true;
                    }
                } else {
                    outcome.reached_home = true;
                }
            }
            if next != flight.place {
                if matches!(next, FlightPlace::Approach(_)) {
                    outcome.push(FlightEvent::ApproachBody, 0);
                }
                flight.place = next;
            }
        }
    }
}
