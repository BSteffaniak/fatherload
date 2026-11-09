//! Deterministic loose-bomb physics and delayed blast cascades. No rendering dependencies.

use crate::{
    event::{GameplayEventKind, TickOutput},
    hazard::PlacedBomb,
    ids::SimulationTick,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

use super::{WorldState, commands::bomb_tile_position, quests::QuestNotes};

/// Collision half-size, in world pixels (the fuse is cosmetic).
pub const BOMB_RADIUS: f32 = 6.0;
const GRAVITY: f32 = 560.0;
const MAX_SPEED: f32 = 600.0;
const BOUNCE: f32 = 0.28;
const REST_SPEED: f32 = 28.0;
const IMPACT_SOUND_SPEED: f32 = 90.0;
const BLAST_REACH: f32 = TILE_SIZE * 4.0;
const CHAIN_REACH: f32 = TILE_SIZE * 2.5;
/// Small but visible delay: a kicked bomb moves before its secondary explosion.
const CHAIN_DELAY_MILLIS: u64 = 220;
/// Limit work and network/particle traffic without dropping existing bombs.
pub const MAX_ACTIVE_BOMBS: usize = 256;

impl WorldState {
    pub(super) fn advance_bombs(
        &mut self,
        next_tick: SimulationTick,
        notes: &mut QuestNotes,
        output: &mut TickOutput,
    ) {
        let mut bombs = std::mem::take(&mut self.bombs);
        bombs.sort_by_key(|bomb| bomb.id);
        for bomb in &mut bombs {
            if move_bomb(bomb, &self.terrain, self.timing.delta_seconds) {
                output.push_event(impact_event(*bomb));
            }
        }
        let (detonating, mut survivors): (Vec<_>, Vec<_>) = bombs
            .into_iter()
            .partition(|bomb| next_tick >= bomb.detonation_tick);
        // Every blast in a tick sees the SAME pre-blast terrain for occlusion. Terrain is
        // cleared only after all impulses are resolved, so chain behavior is order-independent.
        for source in &detonating {
            for bomb in &mut survivors {
                kick_bomb(
                    bomb,
                    *source,
                    &self.terrain,
                    next_tick,
                    self.timing.rate.ticks_for_millis(CHAIN_DELAY_MILLIS),
                );
            }
        }
        for bomb in detonating {
            let center = bomb_tile_position(bomb);
            notes.detonations.push(center);
            let blast = self.detonate_bomb_at(center, 2);
            output.push_event(explosion_event(bomb));
            self.vent_vault_seals(&blast.changed_tiles, output);
        }
        for bomb in &mut survivors {
            bomb.velocity_x = quantize(bomb.velocity_x.clamp(-MAX_SPEED, MAX_SPEED));
            bomb.velocity_y = quantize(bomb.velocity_y.clamp(-MAX_SPEED, MAX_SPEED));
            bomb.timer_seconds = self
                .timing
                .rate
                .seconds_until(next_tick, bomb.detonation_tick);
        }
        self.bombs = survivors;
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn explosion_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::BombDetonated {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn impact_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::BombBounced {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world coordinates address terrain"
)]
fn solid(terrain: &Terrain, x: f32, y: f32) -> bool {
    let position = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    // The sky is open; the side edges and the bottom are not.
    if position.x < 0 || position.x >= terrain.width() || position.y >= terrain.height() {
        return true;
    }
    position.y >= 0 && terrain.is_solid_at(position)
}

fn collides(terrain: &Terrain, x: f32, y: f32) -> bool {
    [-BOMB_RADIUS, 0.0, BOMB_RADIUS].into_iter().any(|dx| {
        [-BOMB_RADIUS, 0.0, BOMB_RADIUS]
            .into_iter()
            .any(|dy| solid(terrain, x + dx, y + dy))
    })
}

/// Axis-separated swept steps never travel more than two pixels, even at the speed cap.
/// Resting bombs are checked each tick: removing their support wakes them automatically.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "bounded physics substep count, at most 128"
)]
fn move_bomb(bomb: &mut PlacedBomb, terrain: &Terrain, dt: f32) -> bool {
    bomb.previous_x = bomb.x;
    bomb.previous_y = bomb.y;
    if collides(terrain, bomb.x, bomb.y) {
        // A drop starts at the rig's feet and can slightly overlap the supporting tile.
        // Only lift out locally; do not teleport bombs through walls or search entire caves.
        let lift =
            (1_u8..=32).find(|offset| !collides(terrain, bomb.x, bomb.y - f32::from(*offset)));
        if let Some(lift) = lift {
            bomb.y -= f32::from(lift);
            bomb.previous_y = bomb.y;
        } else {
            bomb.velocity_x = 0.0;
            bomb.velocity_y = 0.0;
            return false;
        }
    }
    bomb.velocity_x = bomb.velocity_x.clamp(-MAX_SPEED, MAX_SPEED);
    bomb.velocity_y = GRAVITY
        .mul_add(dt, bomb.velocity_y)
        .clamp(-MAX_SPEED, MAX_SPEED);
    let steps =
        ((bomb.velocity_x.abs().max(bomb.velocity_y.abs()) * dt / 2.0).ceil() as u32).clamp(1, 128);
    let step = dt / steps as f32;
    let mut impact: f32 = 0.0;
    for _ in 0..steps {
        let x = bomb.velocity_x.mul_add(step, bomb.x);
        if collides(terrain, x, bomb.y) {
            impact = impact.max(bomb.velocity_x.abs());
            bomb.velocity_x *= -BOUNCE;
        } else {
            bomb.x = x;
        }
        let y = bomb.velocity_y.mul_add(step, bomb.y);
        if collides(terrain, bomb.x, y) {
            impact = impact.max(bomb.velocity_y.abs());
            let hit_floor = bomb.velocity_y > 0.0;
            bomb.velocity_y = if bomb.velocity_y.abs() < REST_SPEED {
                0.0
            } else {
                -bomb.velocity_y * BOUNCE
            };
            // Friction on contact with a floor, not in mid-air.
            if hit_floor {
                bomb.velocity_x *= (12.0f32.mul_add(-step, 1.0)).max(0.0);
                if bomb.velocity_x.abs() < 3.0 {
                    bomb.velocity_x = 0.0;
                }
            }
        } else {
            bomb.y = y;
        }
    }
    bomb.x = quantize(bomb.x);
    bomb.y = quantize(bomb.y);
    bomb.velocity_x = quantize(bomb.velocity_x);
    bomb.velocity_y = quantize(bomb.velocity_y);
    bomb.angle = quantize(
        (bomb.angle + (bomb.x - bomb.previous_x) / BOMB_RADIUS).rem_euclid(std::f32::consts::TAU),
    );
    impact >= IMPACT_SOUND_SPEED
}

fn kick_bomb(
    bomb: &mut PlacedBomb,
    source: PlacedBomb,
    terrain: &Terrain,
    tick: SimulationTick,
    delay: u64,
) {
    let dx = bomb.x - source.x;
    let dy = bomb.y - source.y;
    let distance = dx.hypot(dy);
    if distance >= BLAST_REACH || !line_clear(terrain, source.x, source.y, bomb.x, bomb.y) {
        return;
    }
    // Coincident bombs split reproducibly in opposite directions instead of dividing by zero.
    let (nx, ny) = if distance < 1.0 {
        (if bomb.id.is_multiple_of(2) { 0.7 } else { -0.7 }, -0.7)
    } else {
        (dx / distance, dy / distance)
    };
    let strength = 460.0 * (1.0 - distance / BLAST_REACH);
    bomb.velocity_x = nx.mul_add(strength, bomb.velocity_x);
    bomb.velocity_y += strength.mul_add(ny, -strength * 0.3);
    if distance <= CHAIN_REACH {
        bomb.chain_triggered = true;
        // Small deterministic staggering keeps a pile readable and preserves a visible fuse.
        let stagger = bomb.id % 4 * (delay / 4).max(1);
        let deadline = SimulationTick::new(
            tick.get()
                .saturating_add(delay.max(1))
                .saturating_add(stagger),
        );
        bomb.detonation_tick = bomb.detonation_tick.min(deadline);
    }
}

/// Sample at <= half a collision radius, excluding the source's own tile. A rock wall blocks
/// the kick/chain trigger even if that rock is about to be removed by this blast.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "blast rays are bounded to four tiles"
)]
fn line_clear(terrain: &Terrain, x: f32, y: f32, end_x: f32, end_y: f32) -> bool {
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
        tile == origin || !solid(terrain, px, py)
    })
}
