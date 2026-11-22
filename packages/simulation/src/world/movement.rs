//! Rig movement, collision, and the movement tuning constants.

use crate::{
    attitude::{AttitudeAssists, AttitudeContext, TILT_STEER_GAIN, step_attitude},
    environment::Environment,
    field::{FieldSample, trig::sin_cos},
    ids::PlayerId,
    player::{ENGINE_CLIMB_CAP, drill_bite, engine_power, fuel_burn_efficiency},
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::WorldState;

pub mod drive;
#[cfg(test)]
mod feel_report;

pub use drive::{
    AIR_TOP_SPEED_FACTOR, DRIVE_STEP_SECONDS, DriveTuning, ENGINE_TOP_SPEED_TILES,
    GOD_MODE_SPEED_MULTIPLIER, GOD_MODE_THRUST_MULTIPLIER, engine_top_speed_tiles,
};

/// Hard cap on a rig's speed along either axis (px/s), whatever flings it.
pub const AUTHORITATIVE_ABSOLUTE_MAX_SPEED: f32 = 1_200.0;
/// Longest collision sub-step (px): a fast rig is swept in steps this long, so it can never skip
/// over a tile however fast it moves or however low the tick rate.
pub const AUTHORITATIVE_COLLISION_STEP: f32 = TILE_SIZE * 0.5;

pub const AUTHORITATIVE_HORIZONTAL_SPEED: f32 = 140.0;
pub const AUTHORITATIVE_HORIZONTAL_ACCELERATION: f32 = 900.0;
pub const AUTHORITATIVE_MAX_HORIZONTAL_SPEED: f32 = 260.0;
/// Airborne horizontal velocity halves every this many seconds without input.
pub const AUTHORITATIVE_AIR_DRAG_HALF_LIFE_SECONDS: f32 = 0.076_596_15;
/// Grounded horizontal velocity halves every this many seconds without input.
pub const AUTHORITATIVE_GROUND_DRAG_HALF_LIFE_SECONDS: f32 = 0.046_495_97;
/// Airborne drag rate: terminal horizontal speed is `acceleration / rate`.
pub const AUTHORITATIVE_AIR_DRAG_RATE_PER_SECOND: f32 = 9.767_442;
/// Grounded drag rate: terminal horizontal speed is `acceleration / rate`.
pub const AUTHORITATIVE_GROUND_DRAG_RATE_PER_SECOND: f32 = 16.923_077;
pub const AUTHORITATIVE_SAFE_LANDING_SPEED: f32 = 330.0;
pub const AUTHORITATIVE_CRASH_DAMAGE_SCALE: f32 = 0.11;
/// Gravity applied to falling boulders and debris (not the rig; see
/// [`AUTHORITATIVE_RIG_GRAVITY`]).
pub const AUTHORITATIVE_GRAVITY: f32 = 160.0;
/// Downward acceleration on the rig in flight (px/s²).
pub const AUTHORITATIVE_RIG_GRAVITY: f32 = 440.0;
/// Upward acceleration from the thruster at engine level 1 (px/s²). Thrust is a force, not a
/// set speed: net climb acceleration is `thrust - gravity`, so short taps feather and hover.
pub const AUTHORITATIVE_THRUST_ACCELERATION: f32 = 1_000.0;
/// Thrust multiplier while the rig is still moving downward, so catching a fall feels responsive.
pub const AUTHORITATIVE_DESCENT_BRAKE_MULTIPLIER: f32 = 1.25;
/// Maximum climb speed at engine level 1 (px/s); scales with the engine multiplier.
pub const AUTHORITATIVE_MAX_CLIMB_SPEED: f32 = 190.0;
/// Vertical air resistance (per second): velocity relaxes toward `acceleration / rate`.
pub const AUTHORITATIVE_VERTICAL_DRAG_RATE_PER_SECOND: f32 = 0.7;
/// Warning time before a boulder loosened by mining starts to fall (it shakes and sheds dust
/// first; was 1.5 s).
pub const AUTHORITATIVE_BOULDER_WARNING_MILLIS: u64 = 2_000;
/// Lifetime of a boulder loosened by mining.
pub const AUTHORITATIVE_BOULDER_LIFE_MILLIS: u64 = 4_200;
/// One in this many mined stone tiles loosens a boulder.
///
/// Rolled per mined tile, not per tick, so it is independent of the tick rate. Was one in 16, then one in 80; boulders are a rare surprise
/// now, and a loosened one also needs open ground to hang in and unsettled ground
/// (`world/hazards.rs`).
pub const AUTHORITATIVE_BOULDER_SPAWN_CHANCE: u64 = 400;

/// One in this many stone tiles mined by a drill of `drill_strength` loosens a boulder.
///
/// A drill that bites deeper cuts cleaner: the odds fall with the square of the bite (as fast as
/// the dig rate rises), so a top drill plowing through rock loosens no more boulders per minute
/// than a mid-game one.
#[must_use]
pub const fn boulder_spawn_chance(drill_strength: u8) -> u64 {
    let bite = drill_bite(drill_strength) as u64;
    AUTHORITATIVE_BOULDER_SPAWN_CHANCE * bite * bite
}
/// Warning time of the centre boulder of a cave-in (was 0.9 s).
pub const AUTHORITATIVE_CAVE_IN_WARNING_MILLIS: u64 = 1_500;
/// Extra warning time per tile away from the centre of a cave-in.
pub const AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS: u64 = 250;
/// Lifetime of cave-in boulders.
pub const AUTHORITATIVE_CAVE_IN_LIFE_MILLIS: u64 = 4_000;
pub const AUTHORITATIVE_FUEL_BURN_PER_SECOND: f32 = 5.0;
pub const AUTHORITATIVE_MAX_FALL_SPEED: f32 = 560.0;
pub const AUTHORITATIVE_PLAYER_RADIUS: f32 = 10.5;
/// How high above row 0 a rig's position may go, in tiles: **not a flight ceiling**.
///
/// Fuel is the natural limit on how high a rig climbs; this is the numeric edge of space, as
/// far up as the world extends sideways ([`STREAMED_COLUMN_LIMIT`]), where `f32` positions
/// still resolve a fraction of a pixel. No fuel tank or god-mode climb reaches it in practice.
///
/// [`STREAMED_COLUMN_LIMIT`]: crate::worldgen::streamed::STREAMED_COLUMN_LIMIT
#[allow(
    clippy::cast_precision_loss,
    reason = "the column limit is a power of two, exact in f32"
)]
pub const AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES: f32 =
    crate::worldgen::streamed::STREAMED_COLUMN_LIMIT as f32;
/// The highest world pixel row a rig's position may reach ([`AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES`]).
pub const AUTHORITATIVE_MIN_PLAYER_Y: f32 = -AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES * TILE_SIZE;

/// Drag on a rig in cave water (per second; velocity is divided by `1 + rate * dt` each tick).
pub const WATER_DRAG_PER_SECOND: f32 = 3.0;
/// Share of the rig's gravity cave water buoys away.
pub const WATER_BUOYANCY: f32 = 0.8;
/// Slowest fall a mushroom cap bounces (px/s); slower landings just rest on it.
pub const MUSHROOM_BOUNCE_MIN_IMPACT: f32 = 120.0;
/// Share of the landing speed a mushroom cap gives back.
pub const MUSHROOM_BOUNCE_KEEP: f32 = 0.85;
/// Least upward speed a bounce gives (px/s).
pub const MUSHROOM_BOUNCE_SPEED: f32 = 300.0;

/// Whether a rig at `(x, y)` would rest on a mushroom cap (either bottom corner).
fn lands_on_mushroom(terrain: &Terrain, x: f32, y: f32) -> bool {
    let points = authoritative_collision_points(terrain.frame(), x, y);
    points[2..].iter().any(|position| {
        terrain
            .tile(*position)
            .is_some_and(|tile| tile.kind == TileKind::Cave(crate::cave::CaveTile::MushroomCap))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AuthoritativeMovementStep {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
}

impl WorldState {
    #[allow(
        clippy::too_many_lines,
        reason = "movement parity keeps modifier, integration, collision, and damage ordering explicit"
    )]
    #[cfg(test)]
    pub(crate) fn step_authoritative_movement(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        thrust: bool,
    ) -> Option<AuthoritativeMovementStep> {
        self.step_rig_movement(
            player_id,
            RigControls {
                horizontal,
                thrust,
                rotate: 0.0,
                assists: AttitudeAssists::default(),
            },
        )
    }

    /// Moves `player_id`'s rig one tick with `controls`: the craft rotation step (F9), then
    /// the drive, thrust, gravity and collision. Where the fields are neutral and the rig is
    /// level and still this is the classic movement, bit for bit.
    #[allow(
        clippy::too_many_lines,
        reason = "movement parity keeps modifier, integration, collision, and damage ordering explicit"
    )]
    pub(crate) fn step_rig_movement(
        &mut self,
        player_id: PlayerId,
        controls: RigControls,
    ) -> Option<AuthoritativeMovementStep> {
        let RigControls {
            horizontal,
            thrust,
            rotate,
            assists,
        } = controls;
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        let parts = self
            .equipped_rig_parts
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        let mechanic_level = self.mechanic_levels.get(&player_id).copied().unwrap_or(0);
        // A dive charge's plunge lands without crash damage (Part III F10).
        let immune = self.damage_immune() || self.logistics.is_diving(player_id);
        let god_mode = self.is_test_mode();
        let thrift = self.player_blessing(player_id) == Some(crate::village::BlessingKind::Thrift);
        let settings = *self.terrain.settings();
        let drilling = self.active_drills.contains_key(&player_id);
        // Wave 8, Descent worlds: thrusters below the rig's Pressure Rating burn more.
        let pressure_burn = self.pressure_burn_factor(player_id);
        let player = self.players.get_mut(&player_id)?;
        // Field effects (F9): neutral everywhere without zones or stratum profiles.
        let field = crate::field::sample_in(
            &self.fields,
            &settings,
            self.terrain.frame(),
            player.x,
            player.y,
        );
        let attitude_before = player.attitude;
        let level = field.down_angle();
        let grounded = match level {
            Some(angle) if angle == 0.0 && attitude_before.orientation == 0.0 => {
                authoritative_player_collides(&self.terrain, player.x, player.y + 2.0)
            }
            Some(angle) => {
                // Resting "on the ground" along the local gravity.
                let (sin, cos) = sin_cos(angle);
                rig_collides(
                    &self.terrain,
                    (-sin).mul_add(2.0, player.x),
                    cos.mul_add(2.0, player.y),
                    attitude_before.orientation,
                )
            }
            None => false,
        };
        // The field's time scale: exactly the tick where time runs normally.
        let seconds = delta_seconds * field.time_scale;
        // Craft rotation (F9): the player's rotation thrusters, limits, settling, assists.
        let attitude_gear = player.hazard_gear.attitude;
        let proposed = step_attitude(
            attitude_before,
            &AttitudeContext {
                level: level.unwrap_or(attitude_before.orientation),
                limit: (!field.rotation_unlimited())
                    .then(|| crate::attitude::tilt_limit(attitude_gear.tilt_rating)),
                resting: grounded || drilling,
                input: rotate,
                gyro: assists.gyro && attitude_gear.gyro_stabiliser > 0,
                horizon: assists.horizon && attitude_gear.horizon_hold > 0,
                seconds,
            },
        );
        if proposed.orientation.to_bits() != attitude_before.orientation.to_bits()
            && rig_collides(&self.terrain, player.x, player.y, proposed.orientation)
            && !rig_collides(
                &self.terrain,
                player.x,
                player.y,
                attitude_before.orientation,
            )
        {
            // Turning would push a corner into rock: the rotation stops.
            player.attitude.angular_velocity = 0.0;
        } else {
            player.attitude = proposed;
        }
        let orientation = player.attitude.orientation;
        let classic =
            field.is_neutral_physics() && attitude_before.is_level() && player.attitude.is_level();
        // Altitude physics (F6): thin air, wind, updrafts and gravity fields. Standard (and
        // so the classic path, bit for bit) everywhere but a V8 world's sky.
        let frame = self.terrain.frame();
        let mut environment =
            crate::environment::environment_in(&settings, frame, player.x, player.y);
        // Free lift is rationed like sky fuel (`milestone::free_lift_share`): a laden rig
        // that cannot cross the dead-air gap is too heavy for the updraft.
        if environment.updraft > 0.0
            && let Some(bands) = crate::worldgen::sky::bands_of(&settings)
        {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "altitudes are a few thousand rows"
            )]
            let altitude = (-frame.legacy_y(player.y) / TILE_SIZE).floor() as i32;
            environment.updraft *= crate::milestone::free_lift_share(player, altitude, &bands);
        }
        let thin_air = !environment.is_standard();
        let efficiency = if thin_air {
            environment.thrust_efficiency(player.hazard_gear.altitude_intake)
        } else {
            1.0
        };
        let mut effective_cargo_capacity = player.cargo_capacity;
        if parts.contains(&RigPartKind::CargoBalloon) {
            effective_cargo_capacity = effective_cargo_capacity.saturating_add(20);
        }
        if parts.contains(&RigPartKind::ArmoredCargoBay) {
            effective_cargo_capacity = effective_cargo_capacity.saturating_sub(4).max(1);
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "bounded cargo counts intentionally become a movement load ratio"
        )]
        let cargo_ratio = player.cargo_used() as f32 / effective_cargo_capacity.max(1) as f32;
        let cargo_pressure = if parts.contains(&RigPartKind::HaulerEngine) {
            0.08
        } else if parts.contains(&RigPartKind::LightweightEngine) {
            0.28
        } else {
            0.18
        };
        let cargo_penalty = cargo_ratio.min(1.0).mul_add(-cargo_pressure, 1.0);
        let mut engine_multiplier = engine_power(player.engine_level) * cargo_penalty;
        if parts.contains(&RigPartKind::LightweightEngine) {
            engine_multiplier *= 1.12;
        }
        if parts.contains(&RigPartKind::HaulerEngine) {
            engine_multiplier *= 0.92;
        }
        if parts.contains(&RigPartKind::BurstEngine) {
            engine_multiplier *= 1.25;
        }
        if parts.contains(&RigPartKind::CargoBalloon) {
            engine_multiplier *= 0.88;
        }
        // Top speed, spool-up and braking all scale with the engine tier (see `drive`); rig
        // parts and the cargo load scale the tier's top speed as they scale its thrust.
        let drive_modifier = engine_multiplier / engine_power(player.engine_level);
        let (god_speed, god_thrust) = if god_mode {
            (GOD_MODE_SPEED_MULTIPLIER, GOD_MODE_THRUST_MULTIPLIER)
        } else {
            (1.0, 1.0)
        };
        let ground_top =
            engine_top_speed_tiles(player.engine_level) * drive_modifier * god_speed * TILE_SIZE;
        let top = if grounded {
            ground_top
        } else {
            ground_top * AIR_TOP_SPEED_FACTOR
        };
        let tuning = DriveTuning::new(top, grounded);
        // The top engines lift far more, but climb speed is capped (higher in god mode).
        let climb = engine_multiplier.min(ENGINE_CLIMB_CAP) * god_thrust;
        // A rig only dents itself on a wall when flung into it faster than it can drive.
        let wall_crash_speed = (AUTHORITATIVE_SAFE_LANDING_SPEED * 0.75).max(ground_top * 1.1);
        let thrusting = thrust && player.fuel > 0.0;
        // Exact integration of dv/dt = a - k v over the tick (positive y is down).
        let lift = if thrusting {
            let brake = if player.velocity_y > 0.0 {
                AUTHORITATIVE_DESCENT_BRAKE_MULTIPLIER
            } else {
                1.0
            };
            AUTHORITATIVE_THRUST_ACCELERATION * engine_multiplier * brake * god_thrust
        } else {
            0.0
        };
        if classic {
            let driven = if thin_air && !grounded {
                // Airborne in a V8 sky: thrusters push less in thin air, the air brakes less, and
                // the rig moves relative to the wind (it coasts with it). Aether thrusters (WP-SKY,
                // sky-crafted) win back side thrust and grip the wind; they never add lift.
                let aether = player.hazard_gear.aether_thrusters;
                let air = if aether > 0 {
                    environment.wind_x * (1.0 - crate::skycraft::aether_wind_grip(aether))
                } else {
                    environment.wind_x
                };
                let side_efficiency = if aether > 0 {
                    (1.0 - efficiency)
                        .mul_add(crate::skycraft::aether_drive_factor(aether), efficiency)
                } else {
                    efficiency
                };
                tuning
                    .in_thin_air(environment.air_density, side_efficiency)
                    .step_tick(player.velocity_x - air, horizontal, timing.rate)
                    + air
            } else {
                tuning.step_tick(player.velocity_x, horizontal, timing.rate)
            };
            // A V9 jet stream grabs an airborne rig whatever the air's density (WP-SKY
            // wind-riding): its velocity eases toward the stream's (aether thrusters hold some).
            let driven = if thin_air && !grounded && settings.generator.has_sky_isles() {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_precision_loss,
                    reason = "pixel positions map to tiles; stream speeds are small integers"
                )]
                let jet = crate::environment::jet_stream_in_air(
                    &settings,
                    frame.col(player.x),
                    frame.row(player.y),
                );
                if jet == 0.0 {
                    driven
                } else {
                    let grip = 1.0
                        - crate::skycraft::aether_wind_grip(player.hazard_gear.aether_thrusters);
                    let pull =
                        (crate::worldgen::sky::winds::JET_STREAM_GRIP * delta_seconds).min(1.0);
                    // Only ever toward the stream (a rig flying faster with it keeps its speed).
                    if (jet > 0.0 && driven < jet) || (jet < 0.0 && driven > jet) {
                        jet.mul_add(grip, -driven).mul_add(pull, driven)
                    } else {
                        driven
                    }
                }
            } else {
                driven
            };
            player.velocity_x = quantize_simulation_scalar(driven.clamp(
                -AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
                AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
            ));
            player.velocity_y = quantize_simulation_scalar(if thin_air {
                // Gravity fields and updrafts pull and push; thin air lifts less, drags less and
                // lets the rig climb (and fall) faster once it has the thrust.
                let gravity = environment.body_gravity(AUTHORITATIVE_RIG_GRAVITY);
                (gravity - lift * efficiency)
                    .mul_add(
                        timing.vertical_acceleration_gain,
                        player.velocity_y
                            * environment.drag_retention(timing.vertical_drag_per_tick),
                    )
                    .clamp(
                        -AUTHORITATIVE_MAX_CLIMB_SPEED
                            * climb
                            * environment.climb_cap_factor(efficiency),
                        AUTHORITATIVE_MAX_FALL_SPEED * environment.fall_cap_factor(),
                    )
            } else {
                (AUTHORITATIVE_RIG_GRAVITY - lift)
                    .mul_add(
                        timing.vertical_acceleration_gain,
                        player.velocity_y * timing.vertical_drag_per_tick,
                    )
                    .clamp(
                        -AUTHORITATIVE_MAX_CLIMB_SPEED * climb,
                        AUTHORITATIVE_MAX_FALL_SPEED,
                    )
            });
        } else {
            let (velocity_x, velocity_y) = field_velocity(&FieldDriveInput {
                velocity: (player.velocity_x, player.velocity_y),
                level,
                orientation,
                horizontal,
                thrusting,
                lift: lift * efficiency,
                grounded,
                tuning,
                field: &field,
                environment: &environment,
                efficiency,
                climb,
                seconds,
            });
            player.velocity_x = quantize_simulation_scalar(velocity_x.clamp(
                -AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
                AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
            ));
            player.velocity_y = quantize_simulation_scalar(velocity_y.clamp(
                -AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
                AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
            ));
        }
        // Cave water (flooded caves, V6): buoyant and draggy, so a rig drifts down slowly and
        // swims rather than flies. `1 / (1 + k dt)` keeps the step exact in plain arithmetic.
        if self
            .terrain
            .water_at(authoritative_point_to_tile(frame, player.x, player.y))
        {
            let drag = 1.0 / WATER_DRAG_PER_SECOND.mul_add(seconds, 1.0);
            player.velocity_x = quantize_simulation_scalar(player.velocity_x * drag);
            player.velocity_y = quantize_simulation_scalar(
                (-AUTHORITATIVE_RIG_GRAVITY * WATER_BUOYANCY).mul_add(seconds, player.velocity_y)
                    * drag,
            );
        }
        if thrusting {
            let thrust_efficiency = efficiency;
            // The lunar colony's Helium-3 Reactor (WP-MOON) burns a quarter less.
            let mut efficiency =
                fuel_burn_efficiency(player.fuel_tank_level) * player.lunar.burn_factor();
            if parts.contains(&RigPartKind::BurstEngine) {
                efficiency *= 1.22;
            }
            if parts.contains(&RigPartKind::TitanDrill) {
                efficiency *= 1.08;
            }
            if thrift {
                // A shrine's Blessing of Thrift.
                #[allow(clippy::cast_precision_loss, reason = "a small percentage")]
                let share = crate::village::services::THRIFT_FUEL_PERCENT as f32 / 100.0;
                efficiency *= share;
            }
            if thin_air {
                // Thin air runs the engine rich.
                efficiency *= environment.burn_factor(thrust_efficiency);
            }
            efficiency *= pressure_burn;
            player.fuel = quantize_simulation_scalar(
                (AUTHORITATIVE_FUEL_BURN_PER_SECOND * efficiency)
                    .mul_add(-seconds, player.fuel)
                    .max(0.0),
            );
        }

        let mut mitigation = (f32::from(mechanic_level) * 0.08).min(0.32);
        if parts.contains(&RigPartKind::ImpactHull) {
            mitigation += 0.22;
        }
        if parts.contains(&RigPartKind::ThermalHull) {
            mitigation -= 0.10;
        }
        let mitigated = |damage: f32| damage * (1.0 - mitigation.clamp(0.0, 0.55));

        let delta_x = quantize_simulation_scalar(player.velocity_x * seconds);
        let delta_y = quantize_simulation_scalar(player.velocity_y * seconds);
        // A fast rig is swept in short sub-steps (a slow one takes the whole tick in one).
        let steps = collision_steps(delta_x, delta_y);
        let (step_x, step_y) = if steps == 1 {
            (delta_x, delta_y)
        } else {
            #[allow(clippy::cast_precision_loss, reason = "a handful of sub-steps")]
            let steps = steps as f32;
            (
                quantize_simulation_scalar(delta_x / steps),
                quantize_simulation_scalar(delta_y / steps),
            )
        };
        let (mut blocked_x, mut blocked_y) = (false, false);
        for _ in 0..steps {
            if !blocked_x {
                let (next_x, next_remainder) = integrate_x(player.x, player.x_remainder, step_x);
                if rig_collides(&self.terrain, next_x, player.y, orientation) {
                    let impact_speed = player.velocity_x.abs();
                    if impact_speed > wall_crash_speed && !immune {
                        let damage = (impact_speed - wall_crash_speed)
                            * AUTHORITATIVE_CRASH_DAMAGE_SCALE
                            * 0.5;
                        player.hull =
                            quantize_simulation_scalar((player.hull - mitigated(damage)).max(0.0));
                    }
                    player.velocity_x = 0.0;
                    blocked_x = true;
                } else {
                    let clamped = clamp_authoritative_player_x(&self.terrain, next_x);
                    if clamped.to_bits() == next_x.to_bits() {
                        player.x = next_x;
                        player.x_remainder = next_remainder;
                    } else {
                        // Pressed against the world's edge: no speed left to carry.
                        player.velocity_x = 0.0;
                        blocked_x = true;
                        player.x = quantize_simulation_scalar(clamped);
                        player.x_remainder = 0.0;
                    }
                }
            }
            if !blocked_y {
                let next_y = quantize_simulation_scalar(player.y + step_y);
                if rig_collides(&self.terrain, player.x, next_y, orientation) {
                    // A giant mushroom's cap (fungal grottos) springs a falling rig back up.
                    if delta_y > 0.0
                        && player.velocity_y >= MUSHROOM_BOUNCE_MIN_IMPACT
                        && lands_on_mushroom(&self.terrain, player.x, next_y)
                    {
                        player.velocity_y = quantize_simulation_scalar(
                            -(player.velocity_y * MUSHROOM_BOUNCE_KEEP).max(MUSHROOM_BOUNCE_SPEED),
                        );
                        blocked_y = true;
                        continue;
                    }
                    if delta_y > 0.0 && player.velocity_y > AUTHORITATIVE_SAFE_LANDING_SPEED {
                        let damage = (player.velocity_y - AUTHORITATIVE_SAFE_LANDING_SPEED)
                            * AUTHORITATIVE_CRASH_DAMAGE_SCALE;
                        if !immune {
                            player.hull = quantize_simulation_scalar(
                                (player.hull - mitigated(damage)).max(0.0),
                            );
                        }
                    }
                    player.velocity_y = 0.0;
                    blocked_y = true;
                } else {
                    let clamped = clamp_authoritative_player_y(&self.terrain, next_y);
                    if clamped.to_bits() != next_y.to_bits() {
                        // Pressed against the world's floor (or the far edge of space).
                        player.velocity_y = 0.0;
                        blocked_y = true;
                    }
                    player.y = quantize_simulation_scalar(clamped);
                }
            }
        }

        Some(AuthoritativeMovementStep {
            player_id,
            x: player.x,
            y: player.y,
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
        })
    }
}

/// One tick of a rig's held controls, as movement reads them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RigControls {
    /// Stick, `-1..=1` (already mirrored by a mirror field).
    pub horizontal: f32,
    pub thrust: bool,
    /// Rotation thrusters, `-1..=1`, clockwise positive (already mirrored).
    pub rotate: f32,
    /// Assists the seat has switched on.
    pub assists: AttitudeAssists,
}

/// The four corners of a rig at `(x, y)` turned to `orientation` (radians, clockwise), as
/// tiles: the classic corners when level.
fn rig_corners(
    frame: crate::position::Frame,
    x: f32,
    y: f32,
    orientation: f32,
) -> [TilePosition; 4] {
    if orientation == 0.0 {
        return authoritative_collision_points(frame, x, y);
    }
    let (sin, cos) = sin_cos(orientation);
    let r = AUTHORITATIVE_PLAYER_RADIUS;
    [(-r, -r), (r, -r), (-r, r), (r, r)].map(|(dx, dy)| {
        // Clockwise on screen (+y down).
        let rx = cos.mul_add(dx, -(sin * dy));
        let ry = sin.mul_add(dx, cos * dy);
        authoritative_point_to_tile(frame, x + rx, y + ry)
    })
}

/// Whether a rig at `(x, y)` turned to `orientation` overlaps solid terrain (the classic
/// check when level), in the terrain's frame (F7).
pub(super) fn rig_collides(terrain: &Terrain, x: f32, y: f32, orientation: f32) -> bool {
    rig_corners(terrain.frame(), x, y, orientation)
        .iter()
        .any(|position| terrain.is_solid_at(*position))
}

/// What the field drive (a rig in non-neutral fields, or leaning) needs.
struct FieldDriveInput<'a> {
    velocity: (f32, f32),
    /// The gravity frame's angle (`None`: weightless, the rig's own frame).
    level: Option<f32>,
    orientation: f32,
    horizontal: f32,
    thrusting: bool,
    /// Main thruster acceleration along the rig's up axis (px/s², thin-air efficiency in).
    lift: f32,
    grounded: bool,
    tuning: DriveTuning,
    field: &'a FieldSample,
    environment: &'a Environment,
    efficiency: f32,
    climb: f32,
    seconds: f32,
}

/// The rig's new velocity in a field (F9): the classic drive and thrust model worked in the
/// local gravity's frame. The stick drives along the frame's horizontal (leaning while
/// thrusting steers too, [`TILT_STEER_GAIN`]); the main thruster pushes along the rig's
/// own up axis, so its share along the frame's up fights gravity; ground grip, air drag and
/// the time scale come from the field. Level, in neutral fields, it matches the classic step
/// closely (the classic step itself runs there).
fn field_velocity(input: &FieldDriveInput<'_>) -> (f32, f32) {
    let field = input.field;
    let level = input.level.unwrap_or(input.orientation);
    let (sin_level, cos_level) = sin_cos(level);
    // Frame: right r = (cos, sin), down d = (-sin, cos) (clockwise on screen, +y down).
    let right = (cos_level, sin_level);
    let down = (-sin_level, cos_level);
    let (sin_lean, cos_lean) = sin_cos(crate::field::trig::wrap_angle(input.orientation - level));
    let (vx, vy) = input.velocity;
    let along = vx.mul_add(right.0, vy * right.1);
    let fall = vx.mul_add(down.0, vy * down.1);
    let environment = input.environment;
    // Gravity in world space (the altitude field's strength, the field's direction), less
    // any updraft (world up), projected on the frame.
    let gravity = AUTHORITATIVE_RIG_GRAVITY * environment.gravity;
    let gravity_world = (
        field.gravity_x * gravity,
        field.gravity_y.mul_add(gravity, -environment.updraft),
    );
    let gravity_along = gravity_world.0.mul_add(right.0, gravity_world.1 * right.1);
    let gravity_fall = gravity_world.0.mul_add(down.0, gravity_world.1 * down.1);
    // Sideways: the stick plus leaning thrust, braked by grip (grounded) or drag.
    let steer = if input.thrusting {
        sin_lean * TILT_STEER_GAIN
    } else {
        0.0
    };
    let stick = (input.horizontal + steer).clamp(-1.0, 1.0);
    let brake = if input.grounded {
        field.friction
    } else {
        field.drag
    };
    let mut tuning = input.tuning.with_braking(brake);
    let thin_air = !environment.is_standard();
    let air = if thin_air && !input.grounded {
        tuning = tuning.in_thin_air(environment.air_density, input.efficiency);
        environment.wind_x * right.0
    } else {
        0.0
    };
    let along = gravity_along.mul_add(
        input.seconds,
        tuning.step_seconds(along - air, stick, input.seconds) + air,
    );
    // Along gravity: exact integration of dv/dt = a - k v over the (time-scaled) tick.
    let acceleration = input.lift.mul_add(-cos_lean, gravity_fall);
    let drag_rate =
        AUTHORITATIVE_VERTICAL_DRAG_RATE_PER_SECOND * environment.air_density * field.drag;
    let retention = crate::field::trig::exp_neg(drag_rate * input.seconds);
    let gain = if drag_rate > 0.0 {
        (1.0 - retention) / drag_rate
    } else {
        input.seconds
    };
    let climb_cap = AUTHORITATIVE_MAX_CLIMB_SPEED
        * input.climb
        * if thin_air {
            environment.climb_cap_factor(input.efficiency)
        } else {
            1.0
        };
    let fall_cap = AUTHORITATIVE_MAX_FALL_SPEED * environment.fall_cap_factor();
    let fall = acceleration
        .mul_add(gain, fall * retention)
        .clamp(-climb_cap, fall_cap);
    (
        right.0.mul_add(along, down.0 * fall),
        right.1.mul_add(along, down.1 * fall),
    )
}

/// Moves a rig at `x` (with its [`crate::Player::x_remainder`]) by `delta_x` world pixels:
/// the new `(x, remainder)`.
///
/// The exact position `x + remainder` advances on the simulation's sub-pixel grid in `f64`;
/// `x` is its nearest `f32` and the remainder the rest. Near spawn `f32` holds the grid
/// exactly, so the remainder stays zero and this is plain quantized addition; tens of thousands
/// of tiles out (endless worlds) it keeps a rig's travel identical to travel near spawn instead
/// of rounding every step to the coarser `f32` spacing there.
#[must_use]
pub fn integrate_x(x: f32, remainder: f32, delta_x: f32) -> (f32, f32) {
    let subpixels = f64::from(crate::quantize::SIMULATION_SUBPIXELS_PER_PIXEL);
    let exact = f64::from(x) + f64::from(remainder) + f64::from(delta_x);
    let exact = (exact * subpixels).round() / subpixels;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the nearest f32 is the point; the remainder keeps the rest"
    )]
    let next = exact as f32;
    #[allow(clippy::cast_possible_truncation, reason = "a sub-ulp remainder")]
    let rest = (exact - f64::from(next)) as f32;
    (next, rest)
}

/// Collision sub-steps for a move of `(delta_x, delta_y)` px: one for any move shorter than
/// [`AUTHORITATIVE_COLLISION_STEP`] on both axes (every pre-retune speed), more for fast rigs.
#[must_use]
pub fn collision_steps(delta_x: f32, delta_y: f32) -> u32 {
    let longest = delta_x.abs().max(delta_y.abs());
    if longest.is_nan() || longest <= AUTHORITATIVE_COLLISION_STEP {
        return 1;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a capped speed covers a handful of sub-steps per tick"
    )]
    let steps = (longest / AUTHORITATIVE_COLLISION_STEP).ceil().min(64.0) as u32;
    steps.max(1)
}

pub(super) fn authoritative_player_collides(terrain: &Terrain, x: f32, y: f32) -> bool {
    authoritative_collision_points(terrain.frame(), x, y)
        .iter()
        .any(|position| terrain.is_solid_at(*position))
}

pub(super) fn authoritative_collision_points(
    frame: crate::position::Frame,
    x: f32,
    y: f32,
) -> [TilePosition; 4] {
    [
        authoritative_point_to_tile(
            frame,
            x - AUTHORITATIVE_PLAYER_RADIUS,
            y - AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            frame,
            x + AUTHORITATIVE_PLAYER_RADIUS,
            y - AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            frame,
            x - AUTHORITATIVE_PLAYER_RADIUS,
            y + AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            frame,
            x + AUTHORITATIVE_PLAYER_RADIUS,
            y + AUTHORITATIVE_PLAYER_RADIUS,
        ),
    ]
}

/// The tile under local pixel `(x, y)` of a world in `frame`.
pub(super) fn authoritative_point_to_tile(
    frame: crate::position::Frame,
    x: f32,
    y: f32,
) -> TilePosition {
    frame.tile_at(x, y)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authoritative movement clamps integer terrain bounds in floating player space"
)]
pub(super) fn clamp_authoritative_player_x(terrain: &Terrain, x: f32) -> f32 {
    let frame = terrain.frame();
    x.clamp(frame.col_px(terrain.min_x()), frame.col_px(terrain.max_x()))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authoritative movement clamps integer terrain bounds in floating player space"
)]
pub(super) fn clamp_authoritative_player_y(terrain: &Terrain, y: f32) -> f32 {
    let frame = terrain.frame();
    // The flight ceiling is a place (absolute rows), like the floor.
    let ceiling = if frame.at_origin() {
        AUTHORITATIVE_MIN_PLAYER_Y
    } else {
        frame.from_legacy_px(0.0, AUTHORITATIVE_MIN_PLAYER_Y).1
    };
    y.clamp(ceiling, frame.row_px(terrain.height() - 1))
}
