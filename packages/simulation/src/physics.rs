//! Shared deterministic motion rules for entities (expansion plan §3 F1).
//!
//! Generalized from the loose-bomb physics (`world/bombs.rs`): [`step_body`] with
//! [`BodyParams::BOMB`] reproduces a bomb's motion bit for bit (gravity, axis-separated swept
//! steps of at most two pixels, bounce, floor friction, rest snapping, lift-out of an
//! overlapping spawn, quantization), which a test checks against live bombs. Other kinds pick
//! their own [`BodyParams`].
//!
//! Also here: tile collision queries ([`solid_at`], [`collides`], [`line_clear`]), blast
//! impulses ([`Blast`], [`apply_blast`], generalized from the bomb chain kick), rest and sleep
//! bookkeeping ([`settle`]), simple steering ([`steer_toward`]) and a budgeted tile-grid path
//! search ([`find_tile_path`]) for creatures.
//!
//! Everything is a pure function of its inputs: no RNG, no clocks, fixed iteration orders.

use std::collections::{BTreeMap, VecDeque};

use crate::{
    entity::Body,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

/// Motion constants of one kind of body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyParams {
    /// Collision half-size, in pixels (a square sampled at its corners, edges and centre).
    pub radius: f32,
    /// Downward acceleration, pixels per second squared.
    pub gravity: f32,
    /// Speed cap on each axis, pixels per second.
    pub max_speed: f32,
    /// Fraction of speed kept (and reversed) on a bounce.
    pub bounce: f32,
    /// Vertical impacts slower than this stop dead instead of bouncing.
    pub rest_speed: f32,
    /// Horizontal friction per second while touching a floor (`v *= 1 - friction * dt`).
    pub floor_friction: f32,
    /// Horizontal speed below which floor friction stops the body.
    pub stop_speed: f32,
}

impl BodyParams {
    /// A loose bomb (the original physics).
    pub const BOMB: Self = Self {
        radius: 6.0,
        gravity: 560.0,
        max_speed: 600.0,
        bounce: 0.28,
        rest_speed: 28.0,
        floor_friction: 12.0,
        stop_speed: 3.0,
    };

    /// A floating item stack: a little smaller and less bouncy than a bomb.
    pub const DROP: Self = Self {
        radius: 5.0,
        gravity: 560.0,
        max_speed: 600.0,
        bounce: 0.2,
        rest_speed: 32.0,
        floor_friction: 14.0,
        stop_speed: 4.0,
    };
}

/// Whether world pixel `(x, y)` is inside solid terrain. The sky above the world is open; the
/// side edges and the bottom are walls.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world coordinates address terrain"
)]
pub fn solid_at(terrain: &Terrain, x: f32, y: f32) -> bool {
    let position = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    if !terrain.contains_column(position.x) || position.y >= terrain.height() {
        return true;
    }
    position.y >= 0 && terrain.is_solid_at(position)
}

/// Whether a square body of half-size `radius` centred on `(x, y)` overlaps solid terrain
/// (sampled at its corners, edge midpoints and centre).
#[must_use]
pub fn collides(terrain: &Terrain, x: f32, y: f32, radius: f32) -> bool {
    [-radius, 0.0, radius].into_iter().any(|dx| {
        [-radius, 0.0, radius]
            .into_iter()
            .any(|dy| solid_at(terrain, x + dx, y + dy))
    })
}

/// Whether the body rests on something: solid terrain one pixel below it.
#[must_use]
pub fn supported(terrain: &Terrain, body: &Body, params: &BodyParams) -> bool {
    collides(terrain, body.x, body.y + 1.0, params.radius)
}

/// What one [`step_body`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepReport {
    /// Fastest axis speed lost to a collision this step (impact sounds, damage).
    pub impact_speed: f32,
    /// The body was wedged in rock with no room to lift out, so it was stopped in place.
    pub wedged: bool,
}

/// Advances a body one fixed step of `dt` seconds under gravity with tile collisions.
///
/// Axis-separated swept substeps never travel more than two pixels, even at the speed cap. A
/// body overlapping rock first lifts out up to 32 pixels (a spawn slightly inside its support);
/// with no room it is stopped in place. Positions and velocities are quantized afterwards.
/// With [`BodyParams::BOMB`] this is exactly the bomb motion.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "bounded physics substep count, at most 128"
)]
pub fn step_body(body: &mut Body, params: &BodyParams, terrain: &Terrain, dt: f32) -> StepReport {
    let radius = params.radius;
    body.previous_x = body.x;
    body.previous_y = body.y;
    if collides(terrain, body.x, body.y, radius) {
        let lift = (1_u8..=32)
            .find(|offset| !collides(terrain, body.x, body.y - f32::from(*offset), radius));
        if let Some(lift) = lift {
            body.y -= f32::from(lift);
            body.previous_y = body.y;
        } else {
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
            return StepReport {
                impact_speed: 0.0,
                wedged: true,
            };
        }
    }
    let max_speed = params.max_speed;
    body.velocity_x = body.velocity_x.clamp(-max_speed, max_speed);
    body.velocity_y = params
        .gravity
        .mul_add(dt, body.velocity_y)
        .clamp(-max_speed, max_speed);
    let steps =
        ((body.velocity_x.abs().max(body.velocity_y.abs()) * dt / 2.0).ceil() as u32).clamp(1, 128);
    let step = dt / steps as f32;
    let mut impact: f32 = 0.0;
    for _ in 0..steps {
        let x = body.velocity_x.mul_add(step, body.x);
        if collides(terrain, x, body.y, radius) {
            impact = impact.max(body.velocity_x.abs());
            body.velocity_x *= -params.bounce;
        } else {
            body.x = x;
        }
        let y = body.velocity_y.mul_add(step, body.y);
        if collides(terrain, body.x, y, radius) {
            impact = impact.max(body.velocity_y.abs());
            let hit_floor = body.velocity_y > 0.0;
            body.velocity_y = if body.velocity_y.abs() < params.rest_speed {
                0.0
            } else {
                -body.velocity_y * params.bounce
            };
            // Friction on contact with a floor, not in mid-air.
            if hit_floor {
                body.velocity_x *= (params.floor_friction.mul_add(-step, 1.0)).max(0.0);
                if body.velocity_x.abs() < params.stop_speed {
                    body.velocity_x = 0.0;
                }
            }
        } else {
            body.y = y;
        }
    }
    body.x = quantize(body.x);
    body.y = quantize(body.y);
    body.velocity_x = quantize(body.velocity_x);
    body.velocity_y = quantize(body.velocity_y);
    StepReport {
        impact_speed: impact,
        wedged: false,
    }
}

/// Rest bookkeeping after a step.
///
/// A motionless, supported body counts resting ticks and falls asleep after
/// `sleep_after_ticks` of them; anything else resets the count. Returns whether the body fell
/// asleep this call.
#[allow(
    clippy::float_cmp,
    reason = "quantized positions are compared exactly: did the step move the body at all"
)]
pub fn settle(
    body: &mut Body,
    params: &BodyParams,
    terrain: &Terrain,
    sleep_after_ticks: u16,
) -> bool {
    let still = body.velocity_x == 0.0
        && body.velocity_y == 0.0
        && body.x == body.previous_x
        && body.y == body.previous_y;
    if still && supported(terrain, body, params) {
        body.rest_ticks = body.rest_ticks.saturating_add(1);
        if body.rest_ticks >= sleep_after_ticks.max(1) {
            body.asleep = true;
            return true;
        }
    } else {
        body.rest_ticks = 0;
    }
    false
}

/// Whether a straight line between two world points is free of solid terrain.
///
/// Samples at most every three pixels and ignores the start point's own tile. A rock wall
/// blocks a blast even if that rock is about to be removed by it.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "blast rays are bounded to a few tiles"
)]
pub fn line_clear(terrain: &Terrain, x: f32, y: f32, end_x: f32, end_y: f32) -> bool {
    let distance = (end_x - x).hypot(end_y - y);
    let steps = (distance / 3.0).ceil().max(1.0) as u32;
    let origin = (
        (x / TILE_SIZE).floor() as i32,
        (y / TILE_SIZE).floor() as i32,
    );
    (1..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        let px = (end_x - x).mul_add(t, x);
        let py = (end_y - y).mul_add(t, y);
        let tile = (
            (px / TILE_SIZE).floor() as i32,
            (py / TILE_SIZE).floor() as i32,
        );
        tile == origin || !solid_at(terrain, px, py)
    })
}

/// A blast that pushes bodies away from its centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub x: f32,
    pub y: f32,
    /// Bodies at or beyond this distance, in pixels, are untouched.
    pub reach: f32,
    /// Speed added at the centre, falling off linearly to 0 at `reach`.
    pub strength: f32,
    /// Extra upward kick as a fraction of the strength (blasts throw things up).
    pub lift: f32,
}

impl Blast {
    /// A bomb's blast at `(x, y)` (the same kick bombs give each other).
    #[must_use]
    pub const fn bomb(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            reach: TILE_SIZE * 4.0,
            strength: 460.0,
            lift: 0.3,
        }
    }

    /// The blast of an explosive of `kind` at `(x, y)`: the same reach and peak push the
    /// explosives rules give rigs ([`crate::explosive::blast_impulse`]); a plain bomb equals
    /// [`Self::bomb`]. `None` for kinds that push nothing (EMP, flare).
    #[must_use]
    pub fn explosive(kind: crate::explosive::ExplosiveKind, x: f32, y: f32) -> Option<Self> {
        let reach = kind.push_reach();
        (reach > 0.0).then(|| Self {
            x,
            y,
            reach,
            strength: kind.push_strength(),
            lift: 0.3,
        })
    }
}

/// Pushes `body` away from `blast` if it is in reach with a clear line of sight.
///
/// A body at the very centre goes up and to one side, picked by `tie_break_left` so coincident
/// bodies split. Returns the distance from the centre when the body was pushed.
pub fn apply_blast(
    body: &mut Body,
    blast: &Blast,
    terrain: &Terrain,
    tie_break_left: bool,
) -> Option<f32> {
    let dx = body.x - blast.x;
    let dy = body.y - blast.y;
    let distance = dx.hypot(dy);
    if distance >= blast.reach || !line_clear(terrain, blast.x, blast.y, body.x, body.y) {
        return None;
    }
    let (nx, ny) = if distance < 1.0 {
        (if tie_break_left { -0.7 } else { 0.7 }, -0.7)
    } else {
        (dx / distance, dy / distance)
    };
    let strength = blast.strength * (1.0 - distance / blast.reach);
    body.velocity_x = nx.mul_add(strength, body.velocity_x);
    body.velocity_y += strength.mul_add(ny, -strength * blast.lift);
    body.wake();
    Some(distance)
}

/// Accelerates a body toward a target point: its velocity moves toward `max_speed` along the
/// direction to `(target_x, target_y)` by at most `acceleration * dt`. Stops within a pixel.
pub fn steer_toward(
    body: &mut Body,
    target_x: f32,
    target_y: f32,
    max_speed: f32,
    acceleration: f32,
    dt: f32,
) {
    let dx = target_x - body.x;
    let dy = target_y - body.y;
    let distance = dx.hypot(dy);
    let (desired_x, desired_y) = if distance < 1.0 {
        (0.0, 0.0)
    } else {
        let speed = max_speed.min(distance / dt.max(f32::EPSILON));
        (dx / distance * speed, dy / distance * speed)
    };
    let change_x = desired_x - body.velocity_x;
    let change_y = desired_y - body.velocity_y;
    let change = change_x.hypot(change_y);
    let limit = acceleration * dt;
    let scale = if change > limit && change > 0.0 {
        limit / change
    } else {
        1.0
    };
    body.velocity_x = quantize(change_x.mul_add(scale, body.velocity_x));
    body.velocity_y = quantize(change_y.mul_add(scale, body.velocity_y));
    if body.velocity_x != 0.0 || body.velocity_y != 0.0 {
        body.wake();
    }
}

/// Shortest 4-connected path of open (non-solid, in-world or sky) tiles from `from` to `to`.
///
/// The path excludes `from` and includes `to`; at most `max_nodes` tiles are explored (a
/// per-call budget for creature AI). Neighbours are tried in a fixed order, so the result is
/// deterministic. `None` when `to` is solid, unreachable or out of budget.
#[must_use]
pub fn find_tile_path(
    terrain: &Terrain,
    from: TilePosition,
    to: TilePosition,
    max_nodes: usize,
) -> Option<Vec<TilePosition>> {
    let open = |tile: TilePosition| {
        terrain.contains_column(tile.x)
            && tile.y < terrain.height()
            && (tile.y < 0 || !terrain.is_solid_at(tile))
    };
    if !open(to) {
        return None;
    }
    if from == to {
        return Some(Vec::new());
    }
    let key = |tile: TilePosition| (tile.y, tile.x);
    let mut came_from: BTreeMap<(i32, i32), TilePosition> = BTreeMap::new();
    let mut queue = VecDeque::from([from]);
    came_from.insert(key(from), from);
    while let Some(current) = queue.pop_front() {
        if current == to {
            let mut path = vec![current];
            let mut step = current;
            while let Some(previous) = came_from.get(&key(step)).copied() {
                if previous == from {
                    break;
                }
                path.push(previous);
                step = previous;
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let next = TilePosition {
                x: current.x + dx,
                y: current.y + dy,
            };
            if came_from.len() >= max_nodes {
                return None;
            }
            if open(next) && !came_from.contains_key(&key(next)) {
                came_from.insert(key(next), current);
                queue.push_back(next);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TileKind;

    /// A small closed room: stone everywhere, an air box from (2, 2) to (17, 9).
    fn room() -> Terrain {
        let mut terrain = Terrain::new(20, 12);
        for y in 0..12 {
            for x in 0..20 {
                let open = (2..=17).contains(&x) && (2..=9).contains(&y);
                let _ = terrain.set_kind(
                    TilePosition { x, y },
                    if open { TileKind::Air } else { TileKind::Stone },
                );
            }
        }
        terrain
    }

    fn px(tile: i32) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "small tile index")]
        let tile = tile as f32;
        tile.mul_add(TILE_SIZE, TILE_SIZE * 0.5)
    }

    #[test]
    fn a_dropped_body_falls_bounces_and_falls_asleep_on_the_floor() {
        let terrain = room();
        let mut body = Body::at(px(8), px(3)).moving(120.0, 0.0);
        let mut slept_at = None;
        for tick in 0..600_u32 {
            if body.asleep {
                break;
            }
            let _ = step_body(&mut body, &BodyParams::DROP, &terrain, 1.0 / 60.0);
            if settle(&mut body, &BodyParams::DROP, &terrain, 15) {
                slept_at = Some(tick);
            }
        }
        assert!(slept_at.is_some(), "the drop came to rest and slept");
        let floor = 10.0 * TILE_SIZE;
        assert!(
            (body.y + BodyParams::DROP.radius - floor).abs() <= 1.0,
            "rests on the floor: {}",
            body.y
        );
        assert!(body.x > px(8), "it slid along the floor");
    }

    #[test]
    fn blasts_push_away_and_respect_walls() {
        let terrain = room();
        let mut near = Body::at(px(6), px(9));
        let blast = Blast::bomb(px(5), px(9));
        assert!(apply_blast(&mut near, &blast, &terrain, false).is_some());
        assert!(near.velocity_x > 0.0 && near.velocity_y < 0.0);
        // Outside the room, behind rock: untouched.
        let mut walled = Body::at(px(5), px(11));
        assert!(apply_blast(&mut walled, &Blast::bomb(px(5), px(8)), &terrain, false).is_none());
        let mut far = Body::at(px(15), px(9));
        assert!(apply_blast(&mut far, &blast, &terrain, false).is_none());
    }

    #[test]
    fn paths_go_around_rock_within_budget() {
        let mut terrain = room();
        for y in 2..=8 {
            let _ = terrain.set_kind(TilePosition { x: 10, y }, TileKind::Stone);
        }
        let from = TilePosition { x: 3, y: 3 };
        let to = TilePosition { x: 15, y: 3 };
        let path = find_tile_path(&terrain, from, to, 500).expect("a way under the wall");
        assert_eq!(path.last(), Some(&to));
        assert!(path.iter().any(|tile| tile.x == 10 && tile.y == 9));
        assert!(path.iter().all(|tile| !terrain.is_solid_at(*tile)));
        assert_eq!(find_tile_path(&terrain, from, to, 500), Some(path));
        assert_eq!(
            find_tile_path(&terrain, from, to, 10),
            None,
            "out of budget"
        );
        assert_eq!(
            find_tile_path(&terrain, from, TilePosition { x: 0, y: 0 }, 500),
            None
        );
    }

    #[test]
    fn steering_accelerates_toward_the_target_without_overshooting_the_cap() {
        let mut body = Body::at(0.0, 0.0);
        for _ in 0..120 {
            steer_toward(&mut body, 1_000.0, 0.0, 200.0, 400.0, 1.0 / 60.0);
            body.x += body.velocity_x / 60.0;
        }
        assert!((body.velocity_x - 200.0).abs() < 0.5);
        assert!(body.velocity_y.abs() < f32::EPSILON);
    }

    /// Endless (V5) worlds extend west of column 0: bodies there see the terrain, not a wall.
    #[test]
    fn streamed_worlds_have_no_wall_at_column_zero() {
        let terrain = Terrain::generate(&crate::worldgen::WorldSettings::new(7));
        assert!(terrain.is_streamed());
        let sky_y = -2.0 * TILE_SIZE;
        assert!(!solid_at(&terrain, px(-40), sky_y), "open sky west of town");
        let deep = TilePosition { x: -40, y: 200 };
        assert_eq!(
            solid_at(&terrain, px(deep.x), px(deep.y)),
            terrain.is_solid_at(deep)
        );
        assert!(
            find_tile_path(
                &terrain,
                TilePosition { x: 2, y: -3 },
                TilePosition { x: -3, y: -3 },
                100
            )
            .is_some()
        );
    }
}
