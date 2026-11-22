//! The local pixel frame presentation draws in (expansion plan §15 F7).
//!
//! Every position the simulation hands out (rigs, pilots, entities, the camera built from them)
//! is relative to its world's [`Frame`]: a sector that follows the anchor rig, so the camera's
//! sector is the frame origin and `f32` drawing stays exact however far from the world origin
//! play goes. Tiles stay absolute, so every tile-to-pixel conversion in presentation goes
//! through the frame: [`frame`]`().col_px(column)`, `rel_col`, `rel_row`, `tile_center`,
//! `tile_at`, ...
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
