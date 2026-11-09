//! Exploration darkness: how dark each unexplored tile is, and how that darkness is laid over the
//! tile art.
//!
//! Darkness is a per-tile value (0 = fully visible, 255 = black) from two rules: tiles get darker
//! the deeper they are, and the farther (Manhattan distance) they are from anything explored.
//! Explored tiles are never dark. The values are interpolated between tile centres and applied
//! per art pixel with an ordered dither that walks colours down their palette ramps toward `INK`,
//! so the edge of the known world fades in crisp pixel steps instead of hard tile squares.

#![allow(
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "procedural art mixes small hashed integers and short coordinate names"
)]

use fatherload_simulation::{DiscoveredTiles, terrain::TilePosition};

use crate::rendering::pixel_art::palette::INK;
use crate::rendering::pixel_art::shade::{
    PaletteIndex, TRANSPARENT, bayer4, darker_by, index_of, steps_to_ink,
};

use super::tile_art::{SKY_MAX_TILE_Y, TILE_ART};

const GRADIENT_START_TILE_Y: i32 = SKY_MAX_TILE_Y + 1;
const FULL_DARK_TILE_Y: i32 = 44;
const MIN_DARK_ALPHA: i32 = 48;
const MAX_DARK_ALPHA: i32 = 255;
const DISTANCE_FADE_START_TILES: i32 = 1;
/// Tiles this far (Manhattan) from anything explored are fully dark.
pub const DISTANCE_FULL_DARK_TILES: i32 = 12;
/// Darkness caps for the top ground rows, so the grass line and the ground's silhouette stay
/// readable along the whole surface (row 5, 6, 7).
const SURFACE_DARKNESS_CAP: [u8; 3] = [110, 170, 220];

/// Depth darkness of an unexplored tile in row `tile_y`.
#[must_use]
pub const fn depth_darkness(tile_y: i32) -> u8 {
    if tile_y <= SKY_MAX_TILE_Y {
        return 0;
    }
    let span = FULL_DARK_TILE_Y - GRADIENT_START_TILE_Y;
    if span <= 0 || tile_y >= FULL_DARK_TILE_Y {
        return MAX_DARK_ALPHA as u8;
    }
    let distance = tile_y - GRADIENT_START_TILE_Y;
    (MIN_DARK_ALPHA + ((MAX_DARK_ALPHA - MIN_DARK_ALPHA) * distance) / span) as u8
}

/// Darkness of an unexplored tile `distance` tiles (Manhattan) from the nearest explored tile.
#[must_use]
pub const fn distance_darkness(distance: i32) -> u8 {
    if distance >= DISTANCE_FULL_DARK_TILES {
        return MAX_DARK_ALPHA as u8;
    }
    if distance <= DISTANCE_FADE_START_TILES {
        return 0;
    }
    let span = DISTANCE_FULL_DARK_TILES - DISTANCE_FADE_START_TILES;
    ((MAX_DARK_ALPHA * (distance - DISTANCE_FADE_START_TILES)) / span) as u8
}

/// Darkness of tile row `tile_y` given whether it is explored and its distance to exploration.
#[must_use]
pub fn tile_darkness(tile_y: i32, explored: bool, distance: i32) -> u8 {
    if explored || tile_y <= SKY_MAX_TILE_Y {
        return 0;
    }
    let darkness = depth_darkness(tile_y).max(distance_darkness(distance));
    usize::try_from(tile_y - GRADIENT_START_TILE_Y)
        .ok()
        .and_then(|row| SURFACE_DARKNESS_CAP.get(row))
        .map_or(darkness, |cap| darkness.min(*cap))
}

/// Per-tile darkness over a rectangle of tiles, with a one-tile border for interpolation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DarknessGrid {
    /// Tile coordinates of grid cell (0, 0) (one tile up-left of the covered rectangle).
    min_x: i32,
    min_y: i32,
    width: i32,
    height: i32,
    values: Vec<u8>,
}

impl DarknessGrid {
    /// Darkness for the tiles `min..min + size` (plus a border), from `explored`.
    #[must_use]
    pub fn compute(
        explored: &DiscoveredTiles,
        min: TilePosition,
        size: (i32, i32),
        terrain_height: i32,
    ) -> Self {
        let margin = DISTANCE_FULL_DARK_TILES;
        let (grid_x, grid_y) = (min.x - 1, min.y - 1);
        let (grid_width, grid_height) = (size.0 + 2, size.1 + 2);
        // Distance transform over the grid plus a margin wide enough to see any explored tile
        // that could make a grid tile less than fully dark.
        let (field_x, field_y) = (grid_x - margin, grid_y - margin);
        let (field_width, field_height) = (grid_width + margin * 2, grid_height + margin * 2);
        let far = i32::MAX / 4;
        let mut distance = vec![far; (field_width * field_height) as usize];
        let at = |x: i32, y: i32| (y * field_width + x) as usize;
        for y in 0..field_height {
            let tile_y = field_y + y;
            if tile_y < 0 {
                continue;
            }
            for x in 0..field_width {
                if explored.contains(TilePosition {
                    x: field_x + x,
                    y: tile_y,
                }) {
                    distance[at(x, y)] = 0;
                }
            }
        }
        for y in 0..field_height {
            for x in 0..field_width {
                let mut best = distance[at(x, y)];
                if x > 0 {
                    best = best.min(distance[at(x - 1, y)] + 1);
                }
                if y > 0 {
                    best = best.min(distance[at(x, y - 1)] + 1);
                }
                distance[at(x, y)] = best;
            }
        }
        for y in (0..field_height).rev() {
            for x in (0..field_width).rev() {
                let mut best = distance[at(x, y)];
                if x + 1 < field_width {
                    best = best.min(distance[at(x + 1, y)] + 1);
                }
                if y + 1 < field_height {
                    best = best.min(distance[at(x, y + 1)] + 1);
                }
                distance[at(x, y)] = best;
            }
        }
        let mut values = Vec::with_capacity((grid_width * grid_height) as usize);
        for y in 0..grid_height {
            for x in 0..grid_width {
                let tile_y = (grid_y + y).min(terrain_height - 1);
                let d = distance[at(x + margin, y + margin)];
                values.push(tile_darkness(tile_y, d == 0, d));
            }
        }
        Self {
            min_x: grid_x,
            min_y: grid_y,
            width: grid_width,
            height: grid_height,
            values,
        }
    }

    /// Darkness of tile `(x, y)` (clamped to the grid).
    #[must_use]
    pub fn tile(&self, x: i32, y: i32) -> u8 {
        if self.values.is_empty() {
            return 0;
        }
        let gx = (x - self.min_x).clamp(0, self.width - 1);
        let gy = (y - self.min_y).clamp(0, self.height - 1);
        self.values[(gy * self.width + gx) as usize]
    }

    /// Darkness at global art pixel `(x, y)`, interpolated between tile centres.
    #[must_use]
    pub fn at_art_pixel(&self, x: i32, y: i32) -> f32 {
        let half = TILE_ART as f32 * 0.5;
        let u = (x as f32 + 0.5 - half) / TILE_ART as f32;
        let v = (y as f32 + 0.5 - half) / TILE_ART as f32;
        let (tx, ty) = (u.floor(), v.floor());
        let (fx, fy) = (u - tx, v - ty);
        let (tx, ty) = (tx as i32, ty as i32);
        let a = f32::from(self.tile(tx, ty));
        let b = f32::from(self.tile(tx + 1, ty));
        let c = f32::from(self.tile(tx, ty + 1));
        let d = f32::from(self.tile(tx + 1, ty + 1));
        let top = (b - a).mul_add(fx, a);
        let bottom = (d - c).mul_add(fx, c);
        (bottom - top).mul_add(fy, top)
    }
}

/// Dither levels the darkness range is split into.
const DARK_LEVELS: f32 = 5.0;
/// Level at which every colour has reached `INK`. Ramps are walked in proportion to their length,
/// so a bright ore colour (seven steps from `INK`) goes black at the same darkness as dirt (three
/// steps) instead of glowing through fog that already hides the ground around it.
const INK_LEVEL: f32 = 3.0;

/// `index` seen through `darkness` (0..255) at art pixel `(x, y)`: dithered steps down its ramp,
/// ending in `INK`.
#[must_use]
pub fn apply(index: PaletteIndex, darkness: f32, x: i32, y: i32) -> PaletteIndex {
    if index == TRANSPARENT || darkness <= 0.5 {
        return index;
    }
    let level = (darkness / 255.0).mul_add(DARK_LEVELS, f32::from(bayer4(x, y)) / 16.0);
    let ramp = steps_to_ink(index);
    let steps = (level * f32::from(ramp) / INK_LEVEL) as u8;
    if steps >= ramp {
        index_of(INK)
    } else {
        darker_by(index, steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute_force_distance(explored: &DiscoveredTiles, position: TilePosition) -> i32 {
        let radius = DISTANCE_FULL_DARK_TILES;
        let mut best = radius + 1;
        for y in position.y - radius..=position.y + radius {
            for x in position.x - radius..=position.x + radius {
                let distance = (position.x - x).abs() + (position.y - y).abs();
                if distance < best && explored.contains(TilePosition { x, y }) {
                    best = distance;
                }
            }
        }
        best
    }

    #[test]
    fn depth_darkness_keeps_sky_clear_and_grows_with_depth() {
        assert_eq!(depth_darkness(SKY_MAX_TILE_Y), 0);
        let shallow = depth_darkness(GRADIENT_START_TILE_Y);
        let middle = depth_darkness(GRADIENT_START_TILE_Y + 10);
        let deep = depth_darkness(FULL_DARK_TILE_Y);
        assert!(shallow < middle && middle < deep);
        assert_eq!(deep, 255);
    }

    #[test]
    fn distance_darkness_fades_from_explored_tiles() {
        assert_eq!(distance_darkness(1), 0);
        assert!(distance_darkness(4) < distance_darkness(8));
        assert_eq!(distance_darkness(DISTANCE_FULL_DARK_TILES), 255);
    }

    #[test]
    fn grid_matches_brute_force_distances() {
        let explored = DiscoveredTiles::from_iter([
            TilePosition { x: 10, y: 20 },
            TilePosition { x: 11, y: 20 },
            TilePosition { x: 30, y: 26 },
        ]);
        let min = TilePosition { x: 4, y: 14 };
        let grid = DarknessGrid::compute(&explored, min, (32, 20), 200);
        for y in min.y - 1..min.y + 21 {
            for x in min.x - 1..min.x + 33 {
                let position = TilePosition { x, y };
                let distance = brute_force_distance(&explored, position);
                let expected = tile_darkness(y, explored.contains(position), distance);
                assert_eq!(grid.tile(x, y), expected, "{position:?}");
            }
        }
    }

    #[test]
    fn unexplored_ground_far_away_is_black_but_the_surface_stays_readable() {
        let explored = DiscoveredTiles::default();
        let grid = DarknessGrid::compute(&explored, TilePosition { x: 0, y: 0 }, (16, 40), 200);
        assert_eq!(grid.tile(5, 30), 255);
        assert!(grid.tile(5, GRADIENT_START_TILE_Y) < 128);
        assert_eq!(grid.tile(5, SKY_MAX_TILE_Y), 0);
        assert_eq!(apply(index_of(INK), 255.0, 0, 0), index_of(INK));
        let lit = crate::rendering::pixel_art::shade::index_of(
            crate::rendering::pixel_art::palette::GOLD,
        );
        assert_eq!(apply(lit, 255.0, 3, 2), index_of(INK));
        assert_eq!(apply(lit, 0.0, 3, 2), lit);
    }

    #[test]
    fn bright_colours_go_black_no_later_than_dirt() {
        use crate::rendering::pixel_art::palette::{CREAM, SOIL};
        for darkness in (0..=255u8).step_by(5) {
            for (x, y) in [(0, 0), (1, 2), (3, 1), (2, 3)] {
                let dirt_black = apply(index_of(SOIL), f32::from(darkness), x, y) == index_of(INK);
                let ore_black = apply(index_of(CREAM), f32::from(darkness), x, y) == index_of(INK);
                assert!(
                    !dirt_black || ore_black,
                    "darkness {darkness} at ({x}, {y})"
                );
            }
        }
    }
}
