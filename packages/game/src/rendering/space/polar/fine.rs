//! The fine window's tile art, kept on the CPU and repainted a little at a time.
//!
//! Painting tile art is the expensive part of the polar ground (about 10 µs a tile on a
//! desktop: a 96 x 80 window is some 60 ms, a zoomed-out 240 x 240 one half a second), so the
//! window is never repainted whole on the frame it changes:
//!
//! * when the window moves or grows, every cell it already held is copied across (each band
//!   row by its own column shift) and only the new cells are painted;
//! * a changed tile ([`FineArt::mark_dirty`], from the frame's terrain changes) repaints only its
//!   rows, at once;
//! * cells not painted yet hold a marker texel ([`PENDING`]: alpha 1/255) that the polar shader
//!   reads as "take the coarse map here", and are painted nearest the craft first, at most
//!   [`BUDGET`] a frame. A strip whose revision changes with no tile marked repaints every cell
//!   that way (its old art stays until then).

use std::collections::BTreeSet;

use fatherload_simulation::terrain::TilePosition;

use super::{MAX_HALVINGS, PolarStrip, TILE_ART, Window, strip_tile};
use crate::rendering::{
    pixel_art::shade::{TRANSPARENT, color_at},
    terrain::tile_art,
};

/// Cells painted a frame at most (besides the rows of a changed tile, painted at once).
pub const BUDGET: usize = 384;

/// The texel a cell holds until its art is painted (the shader falls back to the coarse map).
pub const PENDING: [u8; 4] = [0, 0, 0, 1];

const ART: usize = TILE_ART as usize;

/// The fine window's art and what is left to paint.
#[derive(Default)]
pub struct FineArt {
    pub window: Option<Window>,
    revision: u64,
    /// RGBA, `size.0 * 16` art pixels across.
    pub rgba: Vec<u8>,
    /// Per cell (row-major): its art is painted.
    painted: Vec<bool>,
    /// Cells left to paint, the nearest last.
    pending: Vec<(i32, i32)>,
    /// Rows (terrain rows) changed since the last sync.
    dirty_rows: BTreeSet<i32>,
    /// Cell rows to upload this frame (first, last), or everything.
    pub upload: Upload,
}

/// What of the texture must be uploaded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Upload {
    #[default]
    Nothing,
    /// Cell rows `first..=last`.
    Rows(i32, i32),
    Everything,
}

impl Upload {
    pub(super) const fn add_row(self, row: i32) -> Self {
        match self {
            Self::Nothing => Self::Rows(row, row),
            Self::Rows(first, last) => Self::Rows(
                if row < first { row } else { first },
                if row > last { row } else { last },
            ),
            Self::Everything => Self::Everything,
        }
    }
}

impl FineArt {
    /// Tile `tile` changed: its rows (and its neighbours', whose edges depend on it) repaint.
    #[allow(
        dead_code,
        reason = "called through PolarGround::mark_tile_dirty, a seam"
    )]
    pub fn mark_dirty(&mut self, tile: TilePosition) {
        let reach = tile_art::TELLTALE_RADIUS.max(1);
        self.dirty_rows.extend(tile.y - reach..=tile.y + reach);
    }

    /// Whether some cells still show the coarse map.
    pub const fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Brings the art to `window` (the camera at band columns `camera` of `row`) and paints
    /// what this frame's budget allows.
    pub fn sync(
        &mut self,
        strip: &dyn PolarStrip,
        window: &Window,
        camera: (f64, f32),
        revision: u64,
    ) {
        match &self.window {
            Some(old) if old.body == window.body => {
                if old != window {
                    let old = old.clone();
                    self.reframe(strip, &old, window);
                }
            }
            _ => {
                self.dirty_rows.clear();
                self.reset(window);
                self.revision = revision;
            }
        }
        if !self.dirty_rows.is_empty() {
            let rows = std::mem::take(&mut self.dirty_rows);
            for row in rows {
                let y = row - window.row0;
                if (0..window.size.1).contains(&y) {
                    for x in 0..window.size.0 {
                        self.paint(strip, window, x, y);
                    }
                }
            }
        } else if revision != self.revision {
            // Something changed and nobody said what: every cell repaints, over a few frames.
            self.painted.iter_mut().for_each(|painted| *painted = false);
        }
        self.revision = revision;
        if self.painted.iter().any(|painted| !painted) && self.pending.is_empty() {
            self.queue(strip, window, camera);
        }
        let mut budget = BUDGET;
        while budget > 0 {
            let Some((x, y)) = self.pending.pop() else {
                break;
            };
            let index = (y * window.size.0 + x) as usize;
            if !self.painted[index] {
                self.paint(strip, window, x, y);
                budget -= 1;
            }
        }
    }

    /// A new window with nothing painted.
    fn reset(&mut self, window: &Window) {
        let (width, height) = window.size;
        let cells = (width * height) as usize;
        self.rgba = PENDING.repeat(cells * ART * ART);
        self.painted = vec![false; cells];
        self.pending.clear();
        self.window = Some(window.clone());
        self.upload = Upload::Everything;
    }

    /// Moves the art from `old` to `new`: each row keeps what both windows hold (shifted by
    /// its band's change of origin); the rest waits to be painted.
    fn reframe(&mut self, strip: &dyn PolarStrip, old: &Window, new: &Window) {
        let geometry = strip.geometry();
        let old_rgba = std::mem::take(&mut self.rgba);
        let old_painted = std::mem::take(&mut self.painted);
        self.reset(new);
        let (old_stride, new_stride) = (old.size.0 as usize * ART * 4, new.size.0 as usize * ART);
        for y in 0..new.size.1 {
            let row = new.row0 + y;
            let old_y = row - old.row0;
            if !(0..old.size.1).contains(&old_y) {
                continue;
            }
            let band = (geometry.band(row) as usize).min(MAX_HALVINGS);
            let shift = new.origins[band] - old.origins[band];
            let Ok(shift) = i32::try_from(shift) else {
                continue;
            };
            let first = 0.max(-shift);
            let last = new.size.0.min(old.size.0 - shift);
            if first >= last {
                continue;
            }
            for art_row in 0..ART {
                let to = ((y as usize * ART + art_row) * new_stride + first as usize * ART) * 4;
                let from = (old_y as usize * ART + art_row) * old_stride
                    + (first + shift) as usize * ART * 4;
                let length = (last - first) as usize * ART * 4;
                self.rgba[to..to + length].copy_from_slice(&old_rgba[from..from + length]);
            }
            for x in first..last {
                self.painted[(y * new.size.0 + x) as usize] =
                    old_painted[(old_y * old.size.0 + x + shift) as usize];
            }
        }
    }

    /// Queues every unpainted cell, the nearest the camera last (painted first).
    fn queue(&mut self, strip: &dyn PolarStrip, window: &Window, (column, row): (f64, f32)) {
        let geometry = strip.geometry();
        let camera_y = row - window.row0 as f32;
        let mut cells: Vec<(f32, i32, i32)> = Vec::new();
        for y in 0..window.size.1 {
            let band = geometry.band(window.row0 + y);
            let camera_x = (geometry.band_column(band, column)
                - window.origins[(band as usize).min(MAX_HALVINGS)] as f64)
                as f32;
            for x in 0..window.size.0 {
                if !self.painted[(y * window.size.0 + x) as usize] {
                    let distance = (x as f32 - camera_x).abs() + (y as f32 - camera_y).abs();
                    cells.push((distance, x, y));
                }
            }
        }
        cells.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.pending = cells.into_iter().map(|(_, x, y)| (x, y)).collect();
    }

    /// Paints cell `(x, y)` of `window` with its tile's art (clear for sky and past the core).
    fn paint(&mut self, strip: &dyn PolarStrip, window: &Window, x: i32, y: i32) {
        let geometry = strip.geometry();
        let row = window.row0 + y;
        let mut art = [TRANSPARENT; ART * ART];
        if row <= geometry.last_row() {
            let band = (geometry.band(row) as usize).min(MAX_HALVINGS);
            if let Some(tile) = strip_tile(strip, window.origins[band] + i64::from(x), row) {
                tile_art::paint_tile(strip.terrain(), tile, &mut art, ART, (0, 0));
            }
        }
        let stride = window.size.0 as usize * ART;
        for art_row in 0..ART {
            let start = ((y as usize * ART + art_row) * stride + x as usize * ART) * 4;
            for (offset, value) in art[art_row * ART..(art_row + 1) * ART].iter().enumerate() {
                let texel = if *value == TRANSPARENT {
                    [0; 4]
                } else {
                    let colour = color_at(*value);
                    [colour.r, colour.g, colour.b, colour.a]
                };
                let at = start + offset * 4;
                self.rgba[at..at + 4].copy_from_slice(&texel);
            }
        }
        self.painted[(y * window.size.0 + x) as usize] = true;
        self.upload = self.upload.add_row(y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::space::{fixtures, polar::fine_window};

    fn moon_ground() -> fixtures::SpaceFixture {
        fixtures::build("space_moon_surface").expect("fixture")
    }

    #[test]
    fn a_moved_window_keeps_its_art_and_paints_only_the_new_cells() {
        let fixture = moon_ground();
        let view = fixture.view();
        let mut ground = view.ground.expect("ground");
        let strip = ground.strip;
        let geometry = strip.geometry();
        let window = fine_window(&ground, geometry, 96);
        let mut art = FineArt::default();
        let camera = (ground.column, ground.row);
        // The first frame paints the nearest cells; the rest wait for the coarse map.
        art.sync(strip, &window, camera, 0);
        assert!(art.has_pending());
        while art.has_pending() {
            art.sync(strip, &window, camera, 0);
        }
        let full = art.rgba.clone();
        // A full paint of the same window agrees texel for texel.
        let mut fresh = FineArt::default();
        fresh.reset(&window);
        for y in 0..window.size.1 {
            for x in 0..window.size.0 {
                fresh.paint(strip, &window, x, y);
            }
        }
        assert!(full == fresh.rgba);
        // One window step east: everything but a 16-column strip per row is copied across.
        ground.column += f64::from(super::super::WINDOW_STEP);
        let moved = fine_window(&ground, geometry, 96);
        assert_ne!(moved, window);
        art.sync(strip, &moved, (ground.column, ground.row), 0);
        let unpainted = art.painted.iter().filter(|painted| !**painted).count();
        assert!(unpainted <= (moved.size.1 * super::super::WINDOW_STEP) as usize);
        while art.has_pending() {
            art.sync(strip, &moved, (ground.column, ground.row), 0);
        }
        let mut repainted = FineArt::default();
        repainted.reset(&moved);
        for y in 0..moved.size.1 {
            for x in 0..moved.size.0 {
                repainted.paint(strip, &moved, x, y);
            }
        }
        assert!(
            art.rgba == repainted.rgba,
            "shifted art matches a fresh paint"
        );
    }

    #[test]
    fn a_changed_tile_repaints_its_rows_at_once() {
        let fixture = moon_ground();
        let view = fixture.view();
        let ground = view.ground.expect("ground");
        let strip = ground.strip;
        let window = fine_window(&ground, strip.geometry(), 96);
        let mut art = FineArt::default();
        let camera = (ground.column, ground.row);
        while art.window.is_none() || art.has_pending() {
            art.sync(strip, &window, camera, 0);
        }
        art.upload = Upload::Nothing;
        let row = ground.row.floor() as i32 + 3;
        art.mark_dirty(TilePosition { x: 1_200, y: row });
        art.sync(strip, &window, camera, 1);
        let reach = tile_art::TELLTALE_RADIUS.max(1);
        let first = row - reach - window.row0;
        assert_eq!(art.upload, Upload::Rows(first, first + 2 * reach));
        assert!(!art.has_pending());
        // A change nobody described repaints everything, over a few frames.
        art.sync(strip, &window, camera, 2);
        assert!(art.has_pending());
    }
}
