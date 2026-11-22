//! Generator hooks for depth logistics: ancient lifts and deep outposts as data a world
//! generator places and carves.
//!
//! A generator builds a [`LogisticsSites`] with [`LogisticsSites::place_ancient_lift`] and
//! [`LogisticsSites::place_outpost`] (or takes the default layout, [`LogisticsSites::plan`]),
//! asks [`LogisticsSites::tile_at`] for every tile it generates (the carve: `Some(kind)`
//! overrides its own choice), and the new world registers the records with
//! [`crate::WorldState::install_logistics_sites`] (`stamp: false`, since generation already
//! carved them). Scenarios and tests on worlds whose generator knows nothing of the sites pass
//! `stamp: true` and the tiles are written into the terrain instead.
//!
//! Every site is a pure function of its fields, so the carve is the same on every machine and
//! every chunk, in any order.

use crate::{
    base::BuiltTile,
    depth_progression::DescentLayout,
    terrain::{TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{OUTPOST_HALF_WIDTH, OUTPOST_HEIGHT};

/// An ancient lift: a rail of builder stone on `column` from `top_row` to `bottom_row`, with a
/// station pocket at each of `stations` (rows a rig stands on).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AncientLiftSite {
    pub column: i32,
    pub top_row: i32,
    pub bottom_row: i32,
    pub stations: Vec<i32>,
}

/// A deep outpost's room: floor row `floor_row` under columns `column ± OUTPOST_HALF_WIDTH`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutpostSite {
    pub column: i32,
    pub floor_row: i32,
    pub descent: u8,
    pub name: String,
}

/// The logistics sites a generator placed in a world.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LogisticsSites {
    pub lifts: Vec<AncientLiftSite>,
    pub outposts: Vec<OutpostSite>,
}

/// Rows between the stations of a generated ancient lift.
pub const ANCIENT_STATION_SPACING: i32 = 300;
/// Rows under a Descent's top at which its outpost's floor lies (Descent I: its middle).
pub const OUTPOST_DEPTH_BELOW_TOP: i32 = 40;
/// Columns from the outpost's middle to its lift's rail.
pub const LIFT_COLUMN_OFFSET: i32 = OUTPOST_HALF_WIDTH + 2;

const SALT_LOGISTICS_COLUMN: u64 = 0xF10_0001;

/// The deep outposts' names, one per Descent (the plan's settlements and camps).
pub const OUTPOST_NAMES: [&str; 8] = [
    "Gravel Hollow Camp",
    "Cathedral Station",
    "Hive Gate Outpost",
    "Merrow Air Dock",
    "Sunward Camp",
    "Prism Relay",
    "Megacity Archive Post",
    "Heart's Edge",
];

impl LogisticsSites {
    /// Hook: places an ancient lift on `column` from `top_row` (its first station) down to
    /// `bottom_row` (its last), with stations every `station_spacing` rows between.
    pub fn place_ancient_lift(
        &mut self,
        column: i32,
        top_row: i32,
        bottom_row: i32,
        station_spacing: i32,
    ) {
        let (top_row, bottom_row) = (top_row.min(bottom_row), top_row.max(bottom_row));
        let spacing = station_spacing.max(8);
        let mut stations = vec![top_row];
        let mut row = top_row + spacing;
        while row < bottom_row - spacing / 2 {
            stations.push(row);
            row += spacing;
        }
        if bottom_row > top_row {
            stations.push(bottom_row);
        }
        self.lifts.push(AncientLiftSite {
            column,
            top_row,
            bottom_row,
            stations,
        });
    }

    /// Hook: places a deep outpost whose floor is `floor_row`, centred on `column`, serving
    /// Descent `descent`.
    pub fn place_outpost(&mut self, column: i32, floor_row: i32, descent: u8, name: &str) {
        self.outposts.push(OutpostSite {
            column,
            floor_row,
            descent,
            name: name.to_owned(),
        });
    }

    /// The default layout for a Descent world: one outpost per Descent on one seeded column
    /// near the town (Descent I's at its middle, the rest 40 rows under their Descent's top),
    /// and the builders' cable: an ancient lift from just under the surface down to the first
    /// outpost, and one from each outpost down to the next, on the column beside them.
    #[must_use]
    pub fn plan(seed: u64, layout: &dyn DescentLayout, spawn_column: i32) -> Self {
        let mut sites = Self::default();
        let side = if hash2(seed, SALT_LOGISTICS_COLUMN, 0, 0) & 1 == 0 {
            1
        } else {
            -1
        };
        let offset = 70 + i32::try_from(hash2(seed, SALT_LOGISTICS_COLUMN, 1, 0) % 50).unwrap_or(0);
        let column = spawn_column + side * offset;
        let lift_column = column + LIFT_COLUMN_OFFSET;
        let mut previous_station = 12;
        for span in layout.descents() {
            let floor_row = if span.index == 0 {
                span.top + span.rows() / 2
            } else {
                span.top + OUTPOST_DEPTH_BELOW_TOP
            };
            let name = OUTPOST_NAMES[usize::from(span.index).min(OUTPOST_NAMES.len() - 1)];
            sites.place_outpost(column, floor_row, span.index, name);
            let station = floor_row - 1;
            sites.place_ancient_lift(
                lift_column,
                previous_station,
                station,
                ANCIENT_STATION_SPACING,
            );
            previous_station = station;
        }
        sites
    }

    /// The carve: the tile a generator must put at `position` for these sites, or `None` to
    /// keep its own.
    ///
    /// An outpost is an open room (air) on a metal floor one tile wider each side; a lift is a
    /// rail of builder stone (indestructible, so nobody falls down a 1,000-row shaft) broken at
    /// each station by a three-wide, two-high pocket on a metal floor. The rider of a cab is
    /// pinned to it, so the rail between stations never blocks a ride.
    #[must_use]
    pub fn tile_at(&self, position: TilePosition) -> Option<TileKind> {
        for outpost in &self.outposts {
            let dx = (position.x - outpost.column).abs();
            if position.y == outpost.floor_row && dx <= OUTPOST_HALF_WIDTH + 1 {
                return Some(TileKind::Built(BuiltTile::Plating));
            }
            let room_top = outpost.floor_row - OUTPOST_HEIGHT;
            if dx <= OUTPOST_HALF_WIDTH && (room_top..outpost.floor_row).contains(&position.y) {
                return Some(TileKind::Air);
            }
        }
        for lift in &self.lifts {
            let dx = (position.x - lift.column).abs();
            if dx > 1 || position.y < lift.top_row - 1 || position.y > lift.bottom_row + 1 {
                continue;
            }
            for station in &lift.stations {
                if position.y == station + 1 {
                    return Some(TileKind::Built(BuiltTile::Plating));
                }
                if position.y == *station || position.y == station - 1 {
                    return Some(TileKind::Air);
                }
            }
            if dx == 0 && position.y > lift.top_row && position.y < lift.bottom_row {
                return Some(TileKind::Built(BuiltTile::RuinWall));
            }
        }
        None
    }

    /// Every tile the sites carve (for stamping into a world that was not generated with
    /// them).
    #[must_use]
    pub fn carved_tiles(&self) -> Vec<(TilePosition, TileKind)> {
        let mut tiles = Vec::new();
        let mut visit = |x0: i32, x1: i32, y0: i32, y1: i32| {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let position = TilePosition { x, y };
                    if let Some(kind) = self.tile_at(position) {
                        tiles.push((position, kind));
                    }
                }
            }
        };
        for outpost in &self.outposts {
            visit(
                outpost.column - OUTPOST_HALF_WIDTH - 1,
                outpost.column + OUTPOST_HALF_WIDTH + 1,
                outpost.floor_row - OUTPOST_HEIGHT,
                outpost.floor_row,
            );
        }
        for lift in &self.lifts {
            visit(
                lift.column - 1,
                lift.column + 1,
                lift.top_row - 1,
                lift.bottom_row + 1,
            );
        }
        tiles.sort_by_key(|(position, _)| (position.y, position.x));
        tiles.dedup_by_key(|(position, _)| *position);
        tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::depth_progression::PlanLayout;

    #[test]
    fn the_plan_puts_an_outpost_in_every_descent_linked_by_lifts() {
        let sites = LogisticsSites::plan(7, &PlanLayout, 97);
        assert_eq!(sites.outposts.len(), 8);
        assert_eq!(sites.lifts.len(), 8);
        for (outpost, span) in sites.outposts.iter().zip(PlanLayout.descents()) {
            assert!(outpost.floor_row > span.top && outpost.floor_row < span.bottom);
        }
        for pair in sites.lifts.windows(2) {
            assert_eq!(pair[0].bottom_row, pair[1].top_row, "lifts chain");
        }
        for (lift, outpost) in sites.lifts.iter().zip(&sites.outposts) {
            assert_eq!(lift.bottom_row, outpost.floor_row - 1);
            assert!(lift.stations.windows(2).all(|pair| pair[0] < pair[1]));
            assert_eq!(lift.stations.last(), Some(&lift.bottom_row));
        }
        // The same seed gives the same layout; another seed may move it.
        assert_eq!(sites, LogisticsSites::plan(7, &PlanLayout, 97));
    }

    #[test]
    fn the_carve_opens_rooms_and_pockets_and_rails_the_rest() {
        let mut sites = LogisticsSites::default();
        sites.place_outpost(10, 50, 1, "Test Camp");
        sites.place_ancient_lift(16, 20, 49, 10);
        let at = |x, y| sites.tile_at(TilePosition { x, y });
        assert_eq!(at(10, 50), Some(TileKind::Built(BuiltTile::Plating)));
        assert_eq!(at(10, 49), Some(TileKind::Air));
        assert_eq!(at(10, 46), Some(TileKind::Air));
        assert_eq!(at(10, 45), None);
        // Stations at 20, 30, 40 and 49 (pockets at those rows and the one above).
        assert_eq!(at(16, 20), Some(TileKind::Air));
        assert_eq!(at(16, 21), Some(TileKind::Built(BuiltTile::Plating)));
        assert_eq!(at(16, 25), Some(TileKind::Built(BuiltTile::RuinWall)));
        assert_eq!(at(17, 30), Some(TileKind::Air));
        assert_eq!(at(17, 25), None);
        assert_eq!(at(16, 49), Some(TileKind::Air));
        assert_eq!(at(15, 50), Some(TileKind::Built(BuiltTile::Plating)));
        let carved = sites.carved_tiles();
        assert!(
            carved
                .iter()
                .all(|(position, kind)| at(position.x, position.y) == Some(*kind))
        );
    }
}
