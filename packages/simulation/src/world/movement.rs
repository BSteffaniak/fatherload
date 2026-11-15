//! Rig movement, collision, and the movement tuning constants.

use crate::{
    ids::PlayerId,
    player::{ENGINE_CLIMB_CAP, drill_bite, engine_power, fuel_burn_efficiency},
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    terrain::{TILE_SIZE, Terrain, TilePosition},
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
/// Warning time before a boulder loosened by mining starts to fall.
pub const AUTHORITATIVE_BOULDER_WARNING_MILLIS: u64 = 1_500;
/// Lifetime of a boulder loosened by mining.
pub const AUTHORITATIVE_BOULDER_LIFE_MILLIS: u64 = 4_200;
/// One in this many mined stone tiles loosens a boulder (per mined tile, not per tick, so it is
/// independent of the tick rate). Was one in 16; boulders are a rare surprise now.
pub const AUTHORITATIVE_BOULDER_SPAWN_CHANCE: u64 = 80;

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
/// Warning time of the centre boulder of a cave-in.
pub const AUTHORITATIVE_CAVE_IN_WARNING_MILLIS: u64 = 900;
/// Extra warning time per tile away from the centre of a cave-in.
pub const AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS: u64 = 200;
/// Lifetime of cave-in boulders.
pub const AUTHORITATIVE_CAVE_IN_LIFE_MILLIS: u64 = 4_000;
pub const AUTHORITATIVE_FUEL_BURN_PER_SECOND: f32 = 5.0;
pub const AUTHORITATIVE_MAX_FALL_SPEED: f32 = 560.0;
pub const AUTHORITATIVE_PLAYER_RADIUS: f32 = 10.5;
pub const AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES: f32 = 12.0;
pub const AUTHORITATIVE_MIN_PLAYER_Y: f32 = -AUTHORITATIVE_SKY_FLIGHT_HEIGHT_TILES * TILE_SIZE;

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
    pub(crate) fn step_authoritative_movement(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        thrust: bool,
    ) -> Option<AuthoritativeMovementStep> {
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        let parts = self
            .equipped_rig_parts
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        let mechanic_level = self.mechanic_levels.get(&player_id).copied().unwrap_or(0);
        let immune = self.damage_immune();
        let god_mode = self.is_test_mode();
        let player = self.players.get_mut(&player_id)?;
        let grounded = authoritative_player_collides(&self.terrain, player.x, player.y + 2.0);
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
        player.velocity_x = quantize_simulation_scalar(
            DriveTuning::new(top, grounded)
                .step_tick(player.velocity_x, horizontal, timing.rate)
                .clamp(
                    -AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
                    AUTHORITATIVE_ABSOLUTE_MAX_SPEED,
                ),
        );
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
        player.velocity_y = quantize_simulation_scalar(
            (AUTHORITATIVE_RIG_GRAVITY - lift)
                .mul_add(
                    timing.vertical_acceleration_gain,
                    player.velocity_y * timing.vertical_drag_per_tick,
                )
                .clamp(
                    -AUTHORITATIVE_MAX_CLIMB_SPEED * climb,
                    AUTHORITATIVE_MAX_FALL_SPEED,
                ),
        );
        if thrusting {
            let mut efficiency = fuel_burn_efficiency(player.fuel_tank_level);
            if parts.contains(&RigPartKind::BurstEngine) {
                efficiency *= 1.22;
            }
            if parts.contains(&RigPartKind::TitanDrill) {
                efficiency *= 1.08;
            }
            player.fuel = quantize_simulation_scalar(
                (AUTHORITATIVE_FUEL_BURN_PER_SECOND * efficiency)
                    .mul_add(-delta_seconds, player.fuel)
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

        let delta_x = quantize_simulation_scalar(player.velocity_x * delta_seconds);
        let delta_y = quantize_simulation_scalar(player.velocity_y * delta_seconds);
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
                if authoritative_player_collides(&self.terrain, next_x, player.y) {
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
                if authoritative_player_collides(&self.terrain, player.x, next_y) {
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
                        // Pressed against the sky ceiling (or the world's floor).
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
    authoritative_collision_points(x, y)
        .iter()
        .any(|position| terrain.is_solid_at(*position))
}

pub(super) fn authoritative_collision_points(x: f32, y: f32) -> [TilePosition; 4] {
    [
        authoritative_point_to_tile(
            x - AUTHORITATIVE_PLAYER_RADIUS,
            y - AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            x + AUTHORITATIVE_PLAYER_RADIUS,
            y - AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            x - AUTHORITATIVE_PLAYER_RADIUS,
            y + AUTHORITATIVE_PLAYER_RADIUS,
        ),
        authoritative_point_to_tile(
            x + AUTHORITATIVE_PLAYER_RADIUS,
            y + AUTHORITATIVE_PLAYER_RADIUS,
        ),
    ]
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "authoritative collision bridges floating player coordinates to integer terrain tiles"
)]
pub(super) fn authoritative_point_to_tile(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authoritative movement clamps integer terrain bounds in floating player space"
)]
pub(super) fn clamp_authoritative_player_x(terrain: &Terrain, x: f32) -> f32 {
    x.clamp(
        terrain.min_x() as f32 * TILE_SIZE,
        terrain.max_x() as f32 * TILE_SIZE,
    )
}

#[allow(
    clippy::cast_precision_loss,
    reason = "authoritative movement clamps integer terrain bounds in floating player space"
)]
pub(super) fn clamp_authoritative_player_y(terrain: &Terrain, y: f32) -> f32 {
    y.clamp(
        AUTHORITATIVE_MIN_PLAYER_Y,
        (terrain.height() as f32 - 1.0) * TILE_SIZE,
    )
}
