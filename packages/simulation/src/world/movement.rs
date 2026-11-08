//! Rig movement, collision, and the movement tuning constants.

use crate::{
    ids::PlayerId,
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

use super::WorldState;

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
pub const AUTHORITATIVE_THRUST_SPEED: f32 = 120.0;
pub const AUTHORITATIVE_GRAVITY: f32 = 160.0;
/// Warning time before a boulder loosened by mining starts to fall.
pub const AUTHORITATIVE_BOULDER_WARNING_MILLIS: u64 = 850;
/// Lifetime of a boulder loosened by mining.
pub const AUTHORITATIVE_BOULDER_LIFE_MILLIS: u64 = 3_600;
/// One in this many mined stone tiles loosens a boulder (per mined tile, not per tick, so it is
/// independent of the tick rate).
pub const AUTHORITATIVE_BOULDER_SPAWN_CHANCE: u64 = 16;
/// Warning time of the centre boulder of a cave-in.
pub const AUTHORITATIVE_CAVE_IN_WARNING_MILLIS: u64 = 450;
/// Extra warning time per tile away from the centre of a cave-in.
pub const AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS: u64 = 150;
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
    pub fn step_authoritative_movement(
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
        let mut engine_multiplier =
            (1.0 + f32::from(player.engine_level.saturating_sub(1)) * 0.28) * cargo_penalty;
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
        let horizontal_acceleration = if grounded {
            AUTHORITATIVE_HORIZONTAL_ACCELERATION * 1.35
        } else {
            AUTHORITATIVE_HORIZONTAL_ACCELERATION * 0.65
        };
        let (drag, gain) = if grounded {
            (timing.ground_drag_per_tick, timing.ground_acceleration_gain)
        } else {
            (timing.air_drag_per_tick, timing.air_acceleration_gain)
        };
        player.velocity_x = quantize_simulation_scalar(
            (horizontal.clamp(-1.0, 1.0) * horizontal_acceleration * engine_multiplier)
                .mul_add(gain, player.velocity_x * drag),
        )
        .clamp(
            -AUTHORITATIVE_MAX_HORIZONTAL_SPEED * engine_multiplier,
            AUTHORITATIVE_MAX_HORIZONTAL_SPEED * engine_multiplier,
        );
        if thrust && player.fuel > 0.0 {
            player.velocity_y =
                quantize_simulation_scalar(-AUTHORITATIVE_THRUST_SPEED * engine_multiplier);
            let mut efficiency = 1.0 - f32::from(player.fuel_tank_level.saturating_sub(1)) * 0.06;
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
        } else {
            player.velocity_y = quantize_simulation_scalar(
                AUTHORITATIVE_GRAVITY
                    .mul_add(delta_seconds, player.velocity_y)
                    .clamp(-AUTHORITATIVE_MAX_FALL_SPEED, AUTHORITATIVE_MAX_FALL_SPEED),
            );
        }

        let delta_x = quantize_simulation_scalar(player.velocity_x * delta_seconds);
        let delta_y = quantize_simulation_scalar(player.velocity_y * delta_seconds);
        let next_x = quantize_simulation_scalar(player.x + delta_x);
        let next_y = quantize_simulation_scalar(player.y + delta_y);

        if authoritative_player_collides(&self.terrain, next_x, player.y) {
            let impact_speed = player.velocity_x.abs();
            if impact_speed > AUTHORITATIVE_SAFE_LANDING_SPEED * 0.75 {
                let damage = AUTHORITATIVE_SAFE_LANDING_SPEED.mul_add(-0.75, impact_speed)
                    * AUTHORITATIVE_CRASH_DAMAGE_SCALE
                    * 0.5;
                let mut mitigation = (f32::from(mechanic_level) * 0.08).min(0.32);
                if parts.contains(&RigPartKind::ImpactHull) {
                    mitigation += 0.22;
                }
                if parts.contains(&RigPartKind::ThermalHull) {
                    mitigation -= 0.10;
                }
                let damage = damage * (1.0 - mitigation.clamp(0.0, 0.55));
                player.hull = quantize_simulation_scalar((player.hull - damage).max(0.0));
            }
            player.velocity_x = 0.0;
        } else {
            player.x =
                quantize_simulation_scalar(clamp_authoritative_player_x(&self.terrain, next_x));
        }

        if authoritative_player_collides(&self.terrain, player.x, next_y) {
            if delta_y > 0.0 && player.velocity_y > AUTHORITATIVE_SAFE_LANDING_SPEED {
                let damage = (player.velocity_y - AUTHORITATIVE_SAFE_LANDING_SPEED)
                    * AUTHORITATIVE_CRASH_DAMAGE_SCALE;
                let mut mitigation = (f32::from(mechanic_level) * 0.08).min(0.32);
                if parts.contains(&RigPartKind::ImpactHull) {
                    mitigation += 0.22;
                }
                if parts.contains(&RigPartKind::ThermalHull) {
                    mitigation -= 0.10;
                }
                let damage = damage * (1.0 - mitigation.clamp(0.0, 0.55));
                player.hull = quantize_simulation_scalar((player.hull - damage).max(0.0));
            }
            player.velocity_y = 0.0;
        } else {
            player.y =
                quantize_simulation_scalar(clamp_authoritative_player_y(&self.terrain, next_y));
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
    x.clamp(0.0, (terrain.width() as f32 - 1.0) * TILE_SIZE)
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
