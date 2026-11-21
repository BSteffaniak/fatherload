//! The pilot's top-down mode inside a crypt run (WP-TOMBS, Wave 5 T29).
//!
//! A crypt is seen from above: there is no gravity, the held directions walk eight ways, the
//! jump button is a **dodge roll** and the trigger aims anywhere (the aim is the seat's free
//! aim, see `world/pilot_arms.rs`). [`super::step_pilot`] hands every internal step of a pilot
//! whose [`PilotState::view`] is [`PilotView::TopDown`] to [`step_top_down`]; nothing else in
//! the body step needs to know.
//!
//! # Footprint
//!
//! The body keeps the side view's convention (`(x, y)` is the middle of its feet) but the box
//! is a square footprint [`TOP_DOWN_FOOTPRINT`] deep and `2 x PILOT_HALF_WIDTH` wide
//! ([`PilotState::height`] answers the footprint in top-down), smaller than a tile so a
//! one-tile corridor is roomy. It collides with exactly the tiles a pilot does
//! ([`PilotTerrain::blocks`]). Moving is swept per axis in pieces of at most six pixels, so
//! diagonal walking slides along walls, and walking straight at a corner that the box clips by
//! up to [`CORNER_NUDGE`] pixels nudges the pilot round it.
//!
//! # The roll
//!
//! A fresh press of the jump button rolls [`ROLL_SECONDS`] along the held direction (or the
//! heading when nothing is held) at [`ROLL_SPEED`]. For all but the roll's last
//! [`ROLL_RECOVERY_SECONDS`] the pilot cannot be hurt ([`PilotState::roll_guarded`]): strikes,
//! shots, blasts, traps and pits pass through it. A roll can't be steered and is followed by
//! [`ROLL_COOLDOWN_SECONDS`] before the next one. Relics (crypt meanings of side-view perks):
//! each extra jump ([`PilotBoons::extra_jumps`], Spring Heels, Restless Soul) rolls
//! [`ROLL_SECONDS_PER_EXTRA_JUMP`] longer, the glider boost ([`PilotBoons::glide_pct`]) rolls
//! faster (a longer dash), move speed scales the walk.
//!
//! # Tick-rate safety
//!
//! Like the platformer, the mode runs in the fixed [`super::PILOT_STEP_SECONDS`] steps, and
//! every timer counts seconds per step, so 30, 60 and 120 Hz sessions trace the same path.

use crate::{entity::Body, quantize::quantize_simulation_scalar as quantize};

use super::{
    PILOT_HALF_WIDTH, PilotControls, PilotMode, PilotState, PilotStepReport, PilotTerrain,
    PilotView, boon_scale, box_blocked,
};

/// Walking speed in the top-down mode (px/s, about 5.5 tiles a second).
pub const TOP_DOWN_SPEED: f32 = 175.0;
/// Depth of the top-down footprint (px): with the side view's width, a 25 px square.
pub const TOP_DOWN_FOOTPRINT: f32 = 25.0;
/// How fast walking reaches its speed (px/s²).
pub const TOP_DOWN_ACCEL: f32 = 2_600.0;
/// How fast letting go (or a knock) comes to rest (px/s²).
pub const TOP_DOWN_BRAKE: f32 = 3_200.0;
/// A roll lasts this long (seconds).
pub const ROLL_SECONDS: f32 = 0.34;
/// Rolling speed (px/s): a roll covers about 4.6 tiles.
pub const ROLL_SPEED: f32 = 430.0;
/// The roll's last stretch, where the pilot can be hurt again (seconds).
pub const ROLL_RECOVERY_SECONDS: f32 = 0.07;
/// Wait after a roll before the next (seconds).
pub const ROLL_COOLDOWN_SECONDS: f32 = 0.42;
/// Extra roll time per relic extra jump (seconds).
pub const ROLL_SECONDS_PER_EXTRA_JUMP: f32 = 0.09;
/// Farthest a pilot walking straight at a corner is nudged round it (px).
pub const CORNER_NUDGE: f32 = 10.0;

const INPUT_DEADZONE: f32 = 0.2;
/// Longest swept piece of one step (px).
const SWEEP: f32 = 6.0;

/// `(x, y)` as a quantized direction (127 per unit), `None` for a zero vector.
#[must_use]
pub fn quantize_direction(x: f32, y: f32) -> Option<(i8, i8)> {
    let length = x.hypot(y);
    if length < 1e-3 {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a unit vector times 127 fits an i8"
    )]
    let pack = |value: f32| (value / length * 127.0).round().clamp(-127.0, 127.0) as i8;
    Some((pack(x), pack(y)))
}

/// A quantized direction back to a unit vector (zero stays zero).
#[must_use]
pub fn unit_of(x: i8, y: i8) -> (f32, f32) {
    let (x, y) = (f32::from(x), f32::from(y));
    let length = x.hypot(y);
    if length < 1e-3 {
        (0.0, 0.0)
    } else {
        (x / length, y / length)
    }
}

/// How long a roll lasts for `state`'s boons (seconds).
#[must_use]
pub fn roll_seconds(state: &PilotState) -> f32 {
    f32::from(state.boons.extra_jumps).mul_add(ROLL_SECONDS_PER_EXTRA_JUMP, ROLL_SECONDS)
}

/// How fast `state` rolls (px/s).
#[must_use]
pub fn roll_speed(state: &PilotState) -> f32 {
    ROLL_SPEED * boon_scale(state.boons.glide_pct)
}

/// The direction a top-down pilot faces: its heading, else its side-view facing.
#[must_use]
pub fn heading_of(state: &PilotState) -> (f32, f32) {
    let (x, y) = unit_of(state.heading_x, state.heading_y);
    if x == 0.0 && y == 0.0 {
        (f32::from(state.facing.signum()), 0.0)
    } else {
        (x, y)
    }
}

/// The held direction (-1..=1 each way, +y down) of `controls` in the top-down mode.
fn held_direction(controls: PilotControls) -> (f32, f32) {
    let horizontal = controls.horizontal.clamp(-1.0, 1.0);
    let vertical = f32::from(i8::from(controls.down) - i8::from(controls.up));
    let length = horizontal.hypot(vertical);
    if length <= INPUT_DEADZONE {
        (0.0, 0.0)
    } else {
        // A full stick (or a diagonal of two keys) is unit length: no faster diagonals.
        let scale = 1.0 / length.max(1.0);
        (horizontal * scale, vertical * scale)
    }
}

fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

/// Whether the top-down footprint with feet at `(x, feet)` overlaps anything solid.
#[must_use]
pub fn footprint_blocked(world: &impl PilotTerrain, x: f32, feet: f32) -> bool {
    box_blocked(world, x, feet, TOP_DOWN_FOOTPRINT)
}

/// One internal step of the top-down mode (see the module docs).
pub fn step_top_down(
    body: &mut Body,
    state: &mut PilotState,
    world: &impl PilotTerrain,
    controls: PilotControls,
    dt: f32,
    report: &mut PilotStepReport,
) {
    state.anim = (state.anim + dt) % 600.0;
    state.hurt = (state.hurt - dt).max(0.0);
    state.roll_cooldown = (state.roll_cooldown - dt).max(0.0);
    state.mode = PilotMode::Free;
    state.grounded = true;
    state.crouching = false;
    state.jetting = false;
    state.hovering = false;
    state.gliding = false;
    state.tether = None;
    state.air_jumps = 0;
    state.wall = 0;
    state.ledge_timer = 0.0;
    let (dx, dy) = held_direction(controls);
    let held = dx != 0.0 || dy != 0.0;
    if held {
        if let Some((hx, hy)) = quantize_direction(dx, dy) {
            state.heading_x = hx;
            state.heading_y = hy;
        }
        if dx.abs() > INPUT_DEADZONE {
            state.facing = if dx < 0.0 { -1 } else { 1 };
        }
        state.run_time += dt;
    } else {
        state.run_time = 0.0;
    }
    let pressed = controls.jump && !state.jump_held;
    state.jump_held = controls.jump;
    if pressed && state.roll <= 0.0 && state.roll_cooldown <= 0.0 {
        let (rx, ry) = if held { (dx, dy) } else { heading_of(state) };
        if let Some((qx, qy)) = quantize_direction(rx, ry) {
            state.roll_x = qx;
            state.roll_y = qy;
            state.roll = roll_seconds(state);
            report.rolled = true;
        }
    }
    let (vx, vy) = if state.roll > 0.0 {
        let (ux, uy) = unit_of(state.roll_x, state.roll_y);
        let speed = roll_speed(state);
        state.roll = (state.roll - dt).max(0.0);
        if state.roll <= 0.0 {
            state.roll_cooldown = ROLL_COOLDOWN_SECONDS;
        }
        (ux * speed, uy * speed)
    } else {
        let speed = TOP_DOWN_SPEED * boon_scale(state.boons.move_pct);
        let (tx, ty) = (dx * speed, dy * speed);
        let rate = |target: f32| {
            if target == 0.0 {
                TOP_DOWN_BRAKE
            } else {
                TOP_DOWN_ACCEL
            }
        };
        (
            approach(body.velocity_x, tx, rate(tx) * dt),
            approach(body.velocity_y, ty, rate(ty) * dt),
        )
    };
    body.velocity_x = vx;
    body.velocity_y = vy;
    sweep(body, world, vx * dt, true);
    sweep(body, world, vy * dt, false);
    body.x = quantize(body.x);
    body.y = quantize(body.y);
    body.velocity_x = quantize(body.velocity_x);
    body.velocity_y = quantize(body.velocity_y);
    debug_assert!(state.view == PilotView::TopDown);
}

/// Moves `body` by `delta` along one axis in pieces, stopping at walls and nudging round a
/// corner the footprint only clips.
fn sweep(body: &mut Body, world: &impl PilotTerrain, delta: f32, horizontal: bool) {
    if delta == 0.0 {
        return;
    }
    let pieces = (delta.abs() / SWEEP).ceil().max(1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few pieces"
    )]
    let count = pieces as u32;
    let part = delta / pieces;
    // Moving mostly along this axis (the other one still): a clipped corner nudges.
    let straight = if horizontal {
        body.velocity_y.abs() < 1.0
    } else {
        body.velocity_x.abs() < 1.0
    };
    for _ in 0..count {
        let (next_x, next_y) = if horizontal {
            (body.x + part, body.y)
        } else {
            (body.x, body.y + part)
        };
        if !footprint_blocked(world, next_x, next_y) {
            body.x = next_x;
            body.y = next_y;
            continue;
        }
        if straight && let Some(side) = corner_side(world, body, part, horizontal) {
            // Slide sideways round the corner at the pace of the blocked move.
            let shift = side * part.abs().min(CORNER_NUDGE);
            let (sx, sy) = if horizontal {
                (body.x, body.y + shift)
            } else {
                (body.x + shift, body.y)
            };
            if !footprint_blocked(world, sx, sy) {
                body.x = sx;
                body.y = sy;
                continue;
            }
        }
        if horizontal {
            body.velocity_x = 0.0;
        } else {
            body.velocity_y = 0.0;
        }
        break;
    }
}

/// The side (-1 or 1 along the other axis) to slide toward when a move of `part` along one axis
/// is blocked by a corner the footprint clips by at most [`CORNER_NUDGE`] pixels.
fn corner_side(world: &impl PilotTerrain, body: &Body, part: f32, horizontal: bool) -> Option<f32> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few pixels"
    )]
    for offset in 1..=(CORNER_NUDGE as u32) {
        #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
        let offset = offset as f32;
        for side in [-1.0_f32, 1.0] {
            let shift = side * offset;
            let (sx, sy) = if horizontal {
                (body.x, body.y + shift)
            } else {
                (body.x + shift, body.y)
            };
            let (ax, ay) = if horizontal {
                (sx + part, sy)
            } else {
                (sx, sy + part)
            };
            if !footprint_blocked(world, sx, sy) && !footprint_blocked(world, ax, ay) {
                return Some(side);
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "top_down_tests.rs"]
mod tests;

// Keep the footprint and the side view's half width in step (the footprint is square).
const _: () = assert!(PILOT_HALF_WIDTH * 2.0 >= TOP_DOWN_FOOTPRINT);
