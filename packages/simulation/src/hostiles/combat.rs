//! The geometry and numbers of space combat.
//!
//! Hit tests against the drawn bodies, the rig's guns by tier, damage through shields, and ship
//! motion (pure functions; the world's step in `world/hostiles.rs` calls them).

#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "plain IEEE arithmetic: the same bits on every platform (no fused multiply-add or \
              platform hypot)"
)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "steps, ids and hashes fold into hash inputs and small counts"
)]

use crate::{
    field::trig::{atan2, sin_cos, wrap_angle},
    position::WorldPos,
};

use super::{
    COMBAT_STEP_SECONDS, HitCircle, HostileShip, SHIELD_REGEN_DELAY_STEPS, ShipClass, ShipGear,
    ShotKind,
};

/// The nose direction of something turned `angle` (radians clockwise from up).
#[must_use]
pub fn nose(angle: f32) -> (f32, f32) {
    let (sin, cos) = sin_cos(angle);
    (sin, -cos)
}

/// The angle (clockwise from up) of the direction `(dx, dy)`.
#[must_use]
pub fn bearing(dx: f32, dy: f32) -> f32 {
    atan2(dx, -dy)
}

/// Length of `(x, y)` (plain IEEE: no platform `hypot`).
#[must_use]
pub fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Turns `angle` toward `goal` by at most `rate * seconds`.
#[must_use]
pub fn turn_toward(angle: f32, goal: f32, rate: f32, seconds: f32) -> f32 {
    let delta = wrap_angle(goal - angle);
    let most = rate * seconds;
    wrap_angle(angle + delta.clamp(-most, most))
}

/// Whether the segment `a -> b` passes within `r` of `c` (all in one frame).
#[must_use]
pub fn segment_hits(from: (f32, f32), to: (f32, f32), centre: (f32, f32), radius: f32) -> bool {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let (fx, fy) = (centre.0 - from.0, centre.1 - from.1);
    let len2 = dx * dx + dy * dy;
    let along = if len2 > 0.0 {
        ((fx * dx + fy * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (px, py) = (
        from.0 + dx * along - centre.0,
        from.1 + dy * along - centre.1,
    );
    px * px + py * py <= radius * radius
}

/// A world offset from a ship's centre in the ship's own frame (x right, y toward the tail).
#[must_use]
pub fn to_ship_frame(angle: f32, dx: f32, dy: f32) -> (f32, f32) {
    let (sin, cos) = sin_cos(angle);
    // Rotate by -angle: the nose (sin, -cos) maps to (0, -1).
    (dx * cos + dy * sin, -dx * sin + dy * cos)
}

/// A point in a ship's frame as a world offset from its centre.
#[must_use]
pub fn from_ship_frame(angle: f32, x: f32, y: f32) -> (f32, f32) {
    let (sin, cos) = sin_cos(angle);
    (x * cos - y * sin, x * sin + y * cos)
}

/// Whether a shot of radius `radius` moving from `from` to `to` (offsets from the ship's
/// centre, world axes) touches one of `class`'s hit circles with the ship turned `angle`.
#[must_use]
pub fn hits_ship(
    class: ShipClass,
    angle: f32,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
) -> bool {
    let reach = class.reach() + radius;
    if !segment_hits(from, to, (0.0, 0.0), reach) {
        return false;
    }
    let a = to_ship_frame(angle, from.0, from.1);
    let b = to_ship_frame(angle, to.0, to.1);
    class
        .hits()
        .iter()
        .any(|HitCircle { x, y, r }| segment_hits(a, b, (*x, *y), r + radius))
}

/// Whether two ships' (or a ship's and a circle's) bodies touch: `other` is the centre offset
/// of a circle of radius `radius` from the ship's centre.
#[must_use]
pub fn body_touches(class: ShipClass, angle: f32, other: (f32, f32), radius: f32) -> bool {
    hits_ship(class, angle, other, other, radius)
}

/// The rig's cannon at gear `gear`: (damage, steps between shots).
#[must_use]
pub const fn cannon(gear: ShipGear) -> (f32, u64) {
    match gear.cannon {
        0 => (10.0, 26),
        1 => (13.0, 22),
        _ => (16.0, 18),
    }
}

/// The rig's turret at gear `gear` (with the Corsair tech `twin`): (damage, steps between
/// shots, barrels), or `None` with no turret.
#[must_use]
pub const fn turret(gear: ShipGear, twin: bool) -> Option<(f32, u64, u8)> {
    let barrels = if twin { 2 } else { 1 };
    match gear.turret {
        0 if twin => Some((6.0, 20, 2)),
        0 => None,
        1 => Some((6.0, 18, barrels)),
        _ => Some((8.0, 14, barrels)),
    }
}

/// A missile's damage (the Arbiter's lance tech hits harder).
#[must_use]
pub const fn missile_damage(lance: bool) -> f32 {
    if lance {
        ShotKind::Missile.damage() * 1.5
    } else {
        ShotKind::Missile.damage()
    }
}

/// Most a missile may be off the nose to lock (radians, 18 degrees).
pub const MISSILE_CONE: f32 = 0.31;
/// Farthest a missile locks (px).
pub const MISSILE_LOCK_RANGE: f32 = 1_600.0;
/// A missile's turn rate (rad/s) and top speed (px/s).
pub const MISSILE_TURN: f32 = 3.2;
pub const MISSILE_TOP_SPEED: f32 = 1_350.0;
pub const MISSILE_ACCEL: f32 = 1_500.0;

/// Applies `damage` to a ship's shield and then its hull at `step`. Returns whether the
/// shield took it.
pub fn damage_ship(ship: &mut HostileShip, damage: f32, step: u64) -> bool {
    ship.hit_at = step;
    if ship.shield > 0.0 {
        let soaked = ship.shield.min(damage);
        ship.shield -= soaked;
        let rest = damage - soaked;
        ship.hull = (ship.hull - rest).max(0.0);
        rest <= 0.0
    } else {
        ship.hull = (ship.hull - damage).max(0.0);
        false
    }
}

/// One combat step of a ship's shield regrowing.
pub fn regrow_shield(ship: &mut HostileShip, step: u64) {
    let stats = ship.class.stats();
    if stats.shield > 0.0 && step >= ship.hit_at + SHIELD_REGEN_DELAY_STEPS {
        ship.shield = (ship.shield + stats.shield_regen * COMBAT_STEP_SECONDS).min(stats.shield);
    }
}

/// What the AI asks of a ship for one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Order {
    /// Wanted acceleration (px/s², capped at the class's thrust).
    pub ax: f32,
    pub ay: f32,
    /// Wanted heading (radians clockwise from up).
    pub face: f32,
    /// Fire the gun when ready.
    pub fire: bool,
}

/// Moves a ship one combat step under `order` (turn, thrust, speed cap). A ship charging
/// (`charge_speed` set) flies straight at that speed instead.
pub fn move_ship(ship: &mut HostileShip, order: &Order, charge_speed: Option<f32>) {
    let stats = ship.class.stats();
    let dt = COMBAT_STEP_SECONDS;
    if let Some(speed) = charge_speed {
        let (nx, ny) = nose(ship.angle);
        ship.vx = nx * speed;
        ship.vy = ny * speed;
    } else {
        ship.angle = turn_toward(ship.angle, order.face, stats.turn_rate, dt);
        let mut ax = order.ax;
        let mut ay = order.ay;
        let a = length(ax, ay);
        if a > stats.thrust && a > 0.0 {
            ax *= stats.thrust / a;
            ay *= stats.thrust / a;
        }
        ship.vx += ax * dt;
        ship.vy += ay * dt;
        let speed = length(ship.vx, ship.vy);
        if speed > stats.top_speed && speed > 0.0 {
            ship.vx *= stats.top_speed / speed;
            ship.vy *= stats.top_speed / speed;
        }
    }
    ship.pos = ship.pos.offset(ship.vx * dt, ship.vy * dt);
}

/// The acceleration that brings velocity `(vx, vy)` to `(wx, wy)` briskly.
#[must_use]
pub fn match_velocity(vx: f32, vy: f32, wx: f32, wy: f32) -> (f32, f32) {
    const GAIN: f32 = 4.0;
    ((wx - vx) * GAIN, (wy - vy) * GAIN)
}

/// The velocity that holds `range` px from a target `(dx, dy)` away moving at `(tvx, tvy)`:
/// closes or opens the gap at up to `top` px/s on top of the target's motion.
#[must_use]
pub fn hold_range(dx: f32, dy: f32, tvx: f32, tvy: f32, range: f32, top: f32) -> (f32, f32) {
    let distance = length(dx, dy).max(1.0);
    let gap = distance - range;
    let speed = (gap * 2.0).clamp(-top, top);
    (tvx + dx / distance * speed, tvy + dy / distance * speed)
}

/// The point a shot at `speed` should be aimed at to meet a target `(dx, dy)` away moving at
/// `(vx, vy)` relative to the shooter, scaled by `lead` (0 none, 1 full).
#[must_use]
pub fn lead_point(dx: f32, dy: f32, vx: f32, vy: f32, speed: f32, lead: f32) -> (f32, f32) {
    let distance = length(dx, dy);
    let time = if speed > 0.0 { distance / speed } else { 0.0 };
    (dx + vx * time * lead, dy + vy * time * lead)
}

/// Offset from `a` to `b` (px, `f32`).
#[must_use]
pub fn offset(a: WorldPos, b: WorldPos) -> (f32, f32) {
    a.delta(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ship_frame_round_trips_and_puts_the_nose_up() {
        for angle in [0.0_f32, 0.7, -2.1, 3.0] {
            let (nx, ny) = nose(angle);
            let (x, y) = to_ship_frame(angle, nx * 10.0, ny * 10.0);
            assert!(
                x.abs() < 1e-4 && (y + 10.0).abs() < 1e-4,
                "{angle}: {x} {y}"
            );
            let (bx, by) = from_ship_frame(angle, 3.0, -7.0);
            let (rx, ry) = to_ship_frame(angle, bx, by);
            assert!((rx - 3.0).abs() < 1e-4 && (ry + 7.0).abs() < 1e-4);
        }
    }

    #[test]
    fn swept_shots_hit_and_miss_the_drawn_body() {
        // A shot crossing the cutter's middle within one step hits; one passing wide misses.
        assert!(hits_ship(
            ShipClass::Cutter,
            0.0,
            (-40.0, 0.0),
            (40.0, 0.0),
            3.0
        ));
        assert!(!hits_ship(
            ShipClass::Cutter,
            0.0,
            (-40.0, -60.0),
            (40.0, -60.0),
            3.0
        ));
        // Turned a quarter, its long axis lies along x: a shot down x at its tail hits.
        assert!(hits_ship(
            ShipClass::Lancer,
            std::f32::consts::FRAC_PI_2,
            (-30.0, -60.0),
            (-30.0, 60.0),
            3.0
        ));
    }

    #[test]
    fn shields_soak_first_then_the_hull() {
        let mut ship = HostileShip::new(1, ShipClass::Marauder, WorldPos::default(), 0.0, 1);
        assert!(damage_ship(&mut ship, 30.0, 5));
        assert!((ship.shield - 10.0).abs() < 1e-4);
        assert!(!damage_ship(&mut ship, 30.0, 6));
        assert!((ship.hull - 120.0).abs() < 1e-4);
        regrow_shield(&mut ship, 6 + SHIELD_REGEN_DELAY_STEPS - 1);
        assert!(ship.shield.abs() < 1e-6, "no regrowth before the delay");
        regrow_shield(&mut ship, 6 + SHIELD_REGEN_DELAY_STEPS);
        assert!(ship.shield > 0.0);
    }
}
