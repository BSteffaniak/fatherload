//! Map mode's geometry: the zoomed-out depth map of the home world's eight Descents and the
//! galaxy map round the seat. A cursor at `(u, v)` (0..1 across and down the map) names a tile
//! of the home world or a position in space; the preview follows it.

use fatherload_simulation::{
    position::WorldPos,
    space::{self, SpaceTile},
    terrain::TilePosition,
    worldgen::{WorldSettings, descent as gen_descent},
};

/// Rows of sky the depth map shows over the surface.
pub const DEPTH_MAP_SKY_ROWS: i32 = 160;

/// Which map is open.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MapKind {
    /// The home world's depths, surface to floor.
    #[default]
    Depth,
    /// The galaxy round the seat (once space is open).
    Galaxy,
}

/// The depth map: a window of columns, surface sky to the world's floor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepthMap {
    pub first_column: i32,
    pub columns: i32,
    pub top_row: i32,
    pub bottom_row: i32,
}

impl DepthMap {
    /// The map round `centre_column` of a world with `settings`, as wide as a `aspect`
    /// (width / height) rectangle shows at the same scale as its depth, at least 256 columns.
    #[must_use]
    pub fn fit(settings: &WorldSettings, centre_column: i32, aspect: f32) -> Self {
        let bottom_row = if settings.generator.has_descents() {
            gen_descent::floor_row(settings)
        } else {
            settings.height_tiles()
        };
        let top_row = -DEPTH_MAP_SKY_ROWS;
        let rows = (bottom_row - top_row).max(1);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "map spans are thousands of tiles"
        )]
        let columns = ((rows as f32 * aspect.clamp(0.05, 8.0)).round() as i32).max(256);
        Self {
            first_column: centre_column - columns / 2,
            columns,
            top_row,
            bottom_row,
        }
    }

    /// The tile under `(u, v)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "map spans are thousands of tiles"
    )]
    pub fn tile_at(&self, (u, v): (f32, f32)) -> TilePosition {
        let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let rows = self.bottom_row - self.top_row - 1;
        TilePosition {
            x: self.first_column + (u * (self.columns - 1) as f32).round() as i32,
            y: self.top_row + (v * rows as f32).round() as i32,
        }
    }

    /// Where `tile` sits on the map (may lie outside 0..1).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "map spans are thousands of tiles"
    )]
    pub fn uv_of(&self, tile: TilePosition) -> (f32, f32) {
        let rows = (self.bottom_row - self.top_row - 1).max(1);
        (
            (tile.x - self.first_column) as f32 / (self.columns - 1).max(1) as f32,
            (tile.y - self.top_row) as f32 / rows as f32,
        )
    }
}

/// The galaxy map: a square of space centred on the seat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GalaxyMap {
    pub centre: SpaceTile,
    /// Half the square's side, tiles.
    pub radius: i64,
}

impl GalaxyMap {
    /// Three sectors either side of `centre`.
    #[must_use]
    pub const fn round(centre: SpaceTile) -> Self {
        Self {
            centre,
            radius: 3 * space::SPACE_SECTOR_TILES,
        }
    }

    /// The space position under `(u, v)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "the map spans a few sectors"
    )]
    pub fn pos_at(&self, (u, v): (f32, f32)) -> WorldPos {
        let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let span = (self.radius * 2) as f64;
        let x = self.centre.x - self.radius + (f64::from(u) * span).round() as i64;
        let y = self.centre.y - self.radius + (f64::from(v) * span).round() as i64;
        SpaceTile::new(x, y).pos()
    }

    /// Where space tile `tile` sits on the map (may lie outside 0..1).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "the map spans a few sectors"
    )]
    pub fn uv_of(&self, tile: SpaceTile) -> (f32, f32) {
        let span = (self.radius * 2).max(1) as f64;
        (
            ((tile.x - self.centre.x + self.radius) as f64 / span) as f32,
            ((tile.y - self.centre.y + self.radius) as f64 / span) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::worldgen::GeneratorVersion;

    #[test]
    fn the_depth_map_runs_from_the_sky_to_the_floor_and_back() {
        let settings = WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(7)
        };
        let map = DepthMap::fit(&settings, 100, 0.5);
        assert_eq!(map.top_row, -DEPTH_MAP_SKY_ROWS);
        assert!(map.bottom_row > 10_000, "all eight Descents");
        assert_eq!(map.tile_at((0.0, 0.0)).y, map.top_row);
        assert_eq!(map.tile_at((1.0, 1.0)).y, map.bottom_row - 1);
        let middle = map.tile_at((0.5, 0.5));
        let (u, v) = map.uv_of(middle);
        assert!((u - 0.5).abs() < 0.01 && (v - 0.5).abs() < 0.01);
        assert!((middle.x - 100).abs() <= 1, "centred on the column");
    }

    #[test]
    fn the_galaxy_map_round_trips() {
        let map = GalaxyMap::round(SpaceTile::new(1_000, -2_000));
        let pos = map.pos_at((0.25, 0.75));
        let (u, v) = map.uv_of(SpaceTile::of_pos(pos));
        assert!((u - 0.25).abs() < 1e-4 && (v - 0.75).abs() < 1e-4);
        assert_eq!(SpaceTile::of_pos(map.pos_at((0.5, 0.5))), map.centre);
    }
}
