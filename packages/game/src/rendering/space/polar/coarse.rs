//! The coarse map, kept up a little at a time (Wave 13 QA).
//!
//! The coarse map is one texel per tile of a 2,048 x 1,024 window of a big world (or every ring
//! of a small one). Painting it reads every one of those tiles, and a tile nobody has loaded yet
//! is generated with its whole chunk: repainting the window at once when the camera moved a
//! window step cost a dune world up to two seconds in the browser. Like the fine window
//! ([`super::fine`]), a moved window now keeps what both windows hold (each row shifted by its
//! band's change of origin) and the rest is painted within a time budget a frame, the rows
//! nearest the camera first; a row a tile write changed repaints at once, and a change nobody
//! explained repaints everything over a few frames. Until a texel is painted it is clear (the
//! shader shows the fine window or the sky there).

use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

use raylib::prelude::Color;

use fatherload_simulation::terrain::TileKind;

use super::{MAX_HALVINGS, PolarStrip, Window, art_mean, fine::Upload, strip_tile};
use crate::rendering::terrain::tile_art;

/// Painting time a frame may spend on the coarse map (besides the rows of a changed tile).
pub const FRAME_BUDGET: Duration = Duration::from_millis(3);

/// Whether every frame paints the whole map (scene capture: a still shows it all).
static PAINT_WHOLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Makes every frame paint the whole coarse map (`true`: scene capture) or keep to
/// [`FRAME_BUDGET`].
pub fn paint_whole_frames(whole: bool) {
    PAINT_WHOLE.store(whole, std::sync::atomic::Ordering::Relaxed);
}

/// This frame's painting budget.
pub fn frame_budget() -> Duration {
    if PAINT_WHOLE.load(std::sync::atomic::Ordering::Relaxed) {
        Duration::MAX
    } else {
        FRAME_BUDGET
    }
}

/// The coarse map's texels and what is left to paint.
#[derive(Default)]
pub struct CoarseMap {
    pub window: Option<Window>,
    /// RGBA, one texel per tile of the window.
    pub pixels: Vec<u8>,
    /// Per texel (row-major): painted.
    painted: Vec<bool>,
    /// Window rows with texels left to paint, the nearest the camera last.
    pending: Vec<i32>,
    /// The strip revision the map shows.
    revision: u64,
    /// Terrain rows changed since the last sync (marked by tile writes).
    pub dirty_rows: BTreeSet<i32>,
    /// The mean colour of each tile kind's art (per body).
    means: BTreeMap<(TileKind, [u8; 3]), Color>,
    /// What of the texture to upload this frame.
    pub upload: Upload,
}

impl CoarseMap {
    /// Whether texels are still waiting for their paint.
    pub fn has_pending(&self) -> bool {
        self.painted.iter().any(|painted| !painted)
    }

    /// Brings the map to `window` (the camera at strip row `camera_row`) and paints what this
    /// frame's `budget` allows.
    pub fn sync(
        &mut self,
        strip: &dyn PolarStrip,
        window: &Window,
        camera_row: f32,
        revision: u64,
        budget: Duration,
    ) {
        let started = Instant::now();
        match &self.window {
            Some(old) if old.body == window.body => {
                if old != window {
                    let old = old.clone();
                    self.reframe(strip, &old, window);
                }
            }
            _ => {
                self.dirty_rows.clear();
                self.means.clear();
                self.reset(window);
                self.revision = revision;
            }
        }
        if !self.dirty_rows.is_empty() {
            // What a tile write changed repaints at once (its rows only).
            let rows = std::mem::take(&mut self.dirty_rows);
            for row in rows {
                let y = row - window.row0;
                if (0..window.size.1).contains(&y) {
                    self.unpaint_row(window, y);
                    self.paint_row(strip, window, y);
                }
            }
        } else if window.ring && revision != self.revision {
            // A whole-ring map whose strip changed and nobody said where: everything repaints,
            // over a few frames.
            self.painted.iter_mut().for_each(|painted| *painted = false);
            self.pending.clear();
        }
        self.revision = revision;
        if self.pending.is_empty() && self.has_pending() {
            self.queue(window, camera_row);
        }
        // At least one row a frame, then as many as the budget allows.
        let mut first = true;
        while first || started.elapsed() < budget {
            first = false;
            let Some(y) = self.pending.pop() else {
                break;
            };
            self.paint_row(strip, window, y);
        }
    }

    /// Paints everything at once (tests).
    #[cfg(test)]
    pub fn paint_all(&mut self, strip: &dyn PolarStrip, window: &Window, camera_row: f32) {
        while self.has_pending() {
            self.sync(strip, window, camera_row, self.revision, Duration::MAX);
        }
    }

    /// A new window with nothing painted.
    fn reset(&mut self, window: &Window) {
        let (width, height) = window.size;
        let texels = (width.max(0) * height.max(0)) as usize;
        self.pixels = vec![0; texels * 4];
        self.painted = vec![false; texels];
        self.pending.clear();
        self.window = Some(window.clone());
        self.upload = Upload::Everything;
    }

    /// Moves the map from `old` to `new`: each row keeps what both windows hold (shifted by its
    /// band's change of origin); the rest waits to be painted.
    fn reframe(&mut self, strip: &dyn PolarStrip, old: &Window, new: &Window) {
        let geometry = strip.geometry();
        let old_pixels = std::mem::take(&mut self.pixels);
        let old_painted = std::mem::take(&mut self.painted);
        self.reset(new);
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
            let to = (y * new.size.0 + first) as usize;
            let from = (old_y * old.size.0 + first + shift) as usize;
            let length = (last - first) as usize;
            self.pixels[to * 4..(to + length) * 4]
                .copy_from_slice(&old_pixels[from * 4..(from + length) * 4]);
            self.painted[to..to + length].copy_from_slice(&old_painted[from..from + length]);
        }
    }

    /// Queues every row with unpainted texels, the nearest the camera last (painted first).
    fn queue(&mut self, window: &Window, camera_row: f32) {
        let camera_y = camera_row - window.row0 as f32;
        let width = window.size.0.max(0) as usize;
        let mut rows: Vec<(f32, i32)> = (0..window.size.1)
            .filter(|y| {
                let start = *y as usize * width;
                self.painted[start..start + width]
                    .iter()
                    .any(|painted| !painted)
            })
            .map(|y| ((y as f32 - camera_y).abs(), y))
            .collect();
        rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
        self.pending = rows.into_iter().map(|(_, y)| y).collect();
    }

    fn unpaint_row(&mut self, window: &Window, y: i32) {
        let width = window.size.0.max(0) as usize;
        let start = y as usize * width;
        self.painted[start..start + width].fill(false);
    }

    /// Paints the unpainted texels of window row `y`: one texel per tile (the mean of its art),
    /// the sky and past the core clear.
    fn paint_row(&mut self, strip: &dyn PolarStrip, window: &Window, y: i32) {
        let geometry = strip.geometry();
        let terrain = strip.terrain();
        let width = window.size.0;
        let row = window.row0 + y;
        let start = (y * width) as usize;
        if self.painted[start..start + width as usize]
            .iter()
            .all(|painted| *painted)
        {
            return;
        }
        let band = geometry.band(row);
        let columns = if row > geometry.last_row() {
            0
        } else if window.ring {
            (geometry.columns_at(row) as i32).min(width)
        } else {
            width
        };
        let origin = window.origins[(band as usize).min(MAX_HALVINGS)];
        for x in 0..width {
            let at = start + x as usize;
            if self.painted[at] {
                continue;
            }
            self.painted[at] = true;
            let texel = if x < columns
                && let Some(tile) = strip_tile(strip, origin + i64::from(x), row)
                && let Some(found) = terrain.tile(tile)
                && !(found.kind.is_open() && tile.y < terrain.surface_row(tile.x))
            {
                // The mean of the tile art itself (one painted sample per material), so the
                // coarse map meets the fine window's art without a change of colour.
                let material = tile_art::material_color(found.kind, terrain.band_at(tile));
                let colour = *self
                    .means
                    .entry((found.kind, [material.r, material.g, material.b]))
                    .or_insert_with(|| art_mean(terrain, tile, material));
                [colour.r, colour.g, colour.b, 255]
            } else {
                [0_u8; 4]
            };
            self.pixels[at * 4..at * 4 + 4].copy_from_slice(&texel);
        }
        self.upload = self.upload.add_row(y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::space::{fixtures, polar::coarse_window};

    /// A fresh map of `window` painted whole, with `from`'s art means (a material's mean is
    /// taken once per body, from the first tile of it painted).
    fn fresh(
        strip: &dyn PolarStrip,
        window: &Window,
        camera_row: f32,
        from: &CoarseMap,
    ) -> Vec<u8> {
        let mut map = CoarseMap::default();
        map.reset(window);
        map.means.clone_from(&from.means);
        map.paint_all(strip, window, camera_row);
        map.pixels
    }

    #[test]
    fn a_moved_coarse_window_keeps_its_texels_and_paints_the_rest_over_frames() {
        let fixture = fixtures::build("space_planet_surface").expect("fixture");
        let view = fixture.view();
        let mut ground = view.ground.expect("ground");
        let strip = ground.strip;
        let geometry = strip.geometry();
        let window = coarse_window(&ground, geometry);
        assert!(!window.ring, "a big world's window");
        let mut map = CoarseMap::default();
        // No budget: the first frame paints one row, the nearest the camera.
        map.sync(strip, &window, ground.row, 0, Duration::ZERO);
        assert!(map.has_pending(), "the rest waits");
        let width = window.size.0 as usize;
        let painted: Vec<usize> = (0..window.size.1 as usize)
            .filter(|y| {
                map.painted[y * width..(y + 1) * width]
                    .iter()
                    .all(|done| *done)
            })
            .collect();
        let camera_y = (ground.row - window.row0 as f32).clamp(0.0, window.size.1 as f32 - 1.0);
        assert_eq!(painted.len(), 1, "one row");
        assert!(
            (painted[0] as f32 - camera_y).abs() <= 1.0,
            "the nearest the camera"
        );
        map.paint_all(strip, &window, ground.row);
        assert!(
            map.pixels == fresh(strip, &window, ground.row, &map),
            "the same texels"
        );
        // Two window steps east: the overlap is copied, only the new columns wait.
        ground.column += 512.0;
        let moved = coarse_window(&ground, geometry);
        assert_ne!(moved, window);
        map.sync(strip, &moved, ground.row, 0, Duration::ZERO);
        let unpainted = map.painted.iter().filter(|painted| !**painted).count();
        assert!(
            unpainted <= (moved.size.1 as usize) * 512 + width,
            "{unpainted} texels left of {}",
            map.painted.len()
        );
        map.paint_all(strip, &moved, ground.row);
        assert!(
            map.pixels == fresh(strip, &moved, ground.row, &map),
            "shifted texels match a fresh paint"
        );
    }

    #[test]
    fn a_changed_tile_repaints_its_coarse_rows_at_once() {
        let fixture = fixtures::build("space_planet_surface").expect("fixture");
        let view = fixture.view();
        let ground = view.ground.expect("ground");
        let strip = ground.strip;
        let window = coarse_window(&ground, strip.geometry());
        let mut map = CoarseMap::default();
        map.sync(strip, &window, ground.row, 0, Duration::ZERO);
        map.paint_all(strip, &window, ground.row);
        map.upload = Upload::Nothing;
        let row = ground.row.floor() as i32 + 3;
        map.dirty_rows.insert(row);
        map.sync(strip, &window, ground.row, 1, Duration::ZERO);
        let y = row - window.row0;
        assert_eq!(map.upload, Upload::Rows(y, y));
        assert!(!map.has_pending());
    }
}
