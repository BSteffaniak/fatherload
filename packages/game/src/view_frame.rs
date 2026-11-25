//! The local pixel frame presentation draws in (expansion plan §15 F7).
//!
//! Every position the simulation hands out (rigs, pilots, entities, the camera built from them)
//! is relative to its world's [`Frame`]: a sector that follows the anchor rig, so the camera's
//! sector is the frame origin and `f32` drawing stays exact however far from the world origin
//! play goes. Tiles stay absolute, so every tile-to-pixel conversion in presentation goes
//! through the frame: `frame().col_px(column)`, `rel_col`, `rel_row`, `tile_center`,
//! `tile_at`, ... (see [`frame`]).
//!
//! The frame is set once per presented world ([`set`], from `RenderSnapshot::build` and after
//! each runtime step), so renderers, effects, audio and touch hit tests read it without
//! threading it through every call. Presentation is single threaded; the simulation never
//! reads this.

use std::cell::Cell;

use fatherload_simulation::position::Frame;

thread_local! {
    static FRAME: Cell<Frame> = const { Cell::new(Frame::FIXED) };
}

/// The frame presentation is drawing in now.
#[must_use]
pub fn frame() -> Frame {
    FRAME.with(Cell::get)
}

/// Makes `frame` the presented world's frame. Returns the pixel shift from the previous one
/// (subtract it from presentation state kept in pixels across frames: particles, flashes,
/// flights), zero when it did not move.
pub fn set(frame: Frame) -> fatherload_simulation::position::FrameShift {
    let previous = FRAME.with(|cell| cell.replace(frame));
    if previous.sector_x == frame.sector_x && previous.sector_y == frame.sector_y {
        fatherload_simulation::position::FrameShift::default()
    } else {
        previous.shift_to(frame)
    }
}

/// Presentation in `frame` until the guard drops, then back in the frame before (F11: a seat
/// away from the home store draws and listens in its place's frame).
#[must_use]
pub fn scoped(frame: Frame) -> Scoped {
    let previous = self::frame();
    let _ = set(frame);
    Scoped(previous)
}

/// Restores the previous presentation frame when dropped (see [`scoped`]).
pub struct Scoped(Frame);

impl Drop for Scoped {
    fn drop(&mut self) {
        let _ = set(self.0);
    }
}

/// The origin of a parallax layer that moves `1 - parallax` as fast as the camera at local `x`:
/// `x * (1 - parallax)` in world terms. The parallax term reads the camera's absolute place, so
/// the layer never jumps when the frame moves (and is the old expression in the origin frame).
#[must_use]
pub fn parallax_x(x: f32, parallax: f32) -> f32 {
    let frame = frame();
    if frame.at_origin() {
        x * (1.0 - parallax)
    } else {
        frame.legacy_x(x).mul_add(-parallax, x)
    }
}

/// Where a pattern repeating every `period` pixels, anchored to the world grid, starts at or
/// left of local `x`: `(x / period).floor() * period` in world terms. The frame origin's place
/// in the pattern is taken from its exact absolute position, so the pattern never jumps when the
/// frame moves (and the expression is the old one in the origin frame).
#[must_use]
pub fn pattern_start_x(x: f32, period: f32) -> f32 {
    let phase = pattern_phase_x(period);
    if phase == 0.0 {
        return (x / period).floor() * period;
    }
    ((x + phase) / period).floor().mul_add(period, -phase)
}

/// How far the frame origin is into a horizontal pattern of `period` pixels anchored at world
/// pixel 0: add it to a local `x` before taking the pattern cell, subtract it after (zero in the
/// origin frame, so the old expressions are unchanged there).
#[must_use]
pub fn pattern_phase_x(period: f32) -> f32 {
    pattern_phase(frame().origin_col(), period)
}

/// [`pattern_phase_x`] for a vertical pattern.
#[must_use]
pub fn pattern_phase_y(period: f32) -> f32 {
    pattern_phase(frame().origin_row(), period)
}

/// The world cell (of `period` pixels, anchored at world pixel 0) holding the frame origin, and
/// how far into it the origin is: local cell `c` is world cell `c + cells`, and its left edge is
/// at local `c * period - phase`. Patterns hashed by cell use it so every cell keeps its look
/// when the frame moves. `(0, 0.0)` in the origin frame.
#[must_use]
pub fn pattern_anchor_x(period: f32) -> (i64, f32) {
    pattern_anchor(frame().origin_col(), period)
}

/// The frame origin's offset into a pattern of `period` pixels anchored at world pixel 0.
fn pattern_phase(origin_tiles: i64, period: f32) -> f32 {
    pattern_anchor(origin_tiles, period).1
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a cell count and an offset smaller than one period"
)]
fn pattern_anchor(origin_tiles: i64, period: f32) -> (i64, f32) {
    if origin_tiles == 0 || period <= 0.0 {
        return (0, 0.0);
    }
    let origin_px = origin_tiles as f64 * f64::from(fatherload_simulation::terrain::TILE_SIZE);
    let period = f64::from(period);
    let cells = (origin_px / period).floor();
    let phase = cells.mul_add(-period, origin_px);
    if phase >= period || phase < 0.0 {
        (cells as i64, 0.0)
    } else {
        (cells as i64, phase as f32)
    }
}

/// A presentation-only phase (animation offsets, flicker) for local `x`: the old world pixel,
/// so the phase never jumps when the frame moves. Exactly `x` in the origin frame.
#[must_use]
pub fn phase_x(x: f32) -> f32 {
    frame().legacy_x(x)
}

/// [`phase_x`] for a vertical position.
#[must_use]
pub fn phase_y(y: f32) -> f32 {
    frame().legacy_y(y)
}

/// [`parallax_x`] for a vertical camera position.
#[must_use]
pub fn parallax_y(y: f32, parallax: f32) -> f32 {
    let frame = frame();
    if frame.at_origin() {
        y * (1.0 - parallax)
    } else {
        frame.legacy_y(y).mul_add(-parallax, y)
    }
}
