//! The ocean archipelago (Wave 12 planets-a): islands over a planet-wide sea, and below the
//! seabed the Reef Shelf, the Drowned Ruins and the Trench.
//!
//! * **Surface**: one elevation field round the body ([`elevation`]): where it rises above the
//!   sea there are islands (sandy beaches easing into hills), elsewhere the seabed lies under
//!   a full sea (fluid water from the sea level, row 0, down to the seabed). The rig's
//!   **submarine mode** (Descent IV's ballast) works in it. In the shallows the seabed grows
//!   **reefs** (branching and brain coral) and now and then a **pearl shell**.
//! * **Reef Shelf**: limestone with fossil coral; signature rule **waterlogged**: the shelf is
//!   full of sea-water pockets, so a shaft drilled through it floods.
//! * **Drowned Ruins**: dark silt stone; every cave here is flooded, and the ruins' **sealed
//!   halls** still hold air (walls of cut stone and glowing glyphs round a relic), each in a
//!   flooded gallery: break a wall and the sea pours in.
//! * **The Trench**: black basalt, flooded caves and **black smokers**, chimneys that scald
//!   what comes close (heat) with rich ore round them; the water's **pressure** crushes a hull
//!   without the Pressure Hull.
//! * **Landmark: the Drowned Spire**, half a lap from the settlement, out in the open sea: a
//!   ruined tower rising from the seabed 22 rows over the waves, flooded below its floor, its
//!   dry room (windows on both sides, a hole in the roof) holding a drowned relic.
//! * **Settlement site**: the top of an island, always there and level.

use super::{
    Cell, SurfaceSite, WildTile, air, chunk_with, core_floor_row, ease, is_small_body,
    landmark_column, roll, secret_tile, settlement_column, site_weight, water, wild, wrap_delta,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, PlanetThemeId,
        ThemeWorld,
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind},
};

/// The ocean's strata (registry data).
pub const OCEAN_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Reef Shelf",
        share_permille: 330,
        rock: TileKind::Wild(WildTile::Limestone),
        accent: TileKind::Wild(WildTile::Coral),
        accent_permille: 100,
        ores: &[
            (MineralKind::Copper, 70),
            (MineralKind::Silver, 60),
            (MineralKind::Gold, 25),
        ],
    },
    PlanetStratum {
        name: "Drowned Ruins",
        share_permille: 330,
        rock: TileKind::Wild(WildTile::SiltStone),
        accent: TileKind::Wild(WildTile::SeaSand),
        accent_permille: 120,
        ores: &[
            (MineralKind::Gold, 40),
            (MineralKind::Emerald, 25),
            (MineralKind::Ruby, 14),
        ],
    },
    PlanetStratum {
        name: "The Trench",
        share_permille: 340,
        rock: TileKind::Wild(WildTile::TrenchBasalt),
        accent: TileKind::Wild(WildTile::VentRock),
        accent_permille: 80,
        ores: &[
            (MineralKind::Platinum, 30),
            (MineralKind::Diamond, 22),
            (MineralKind::Uranium, 12),
        ],
    },
];

/// The ocean archipelago theme (registry entry [`PlanetThemeId::OCEAN`]).
pub const OCEAN_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::OCEAN,
    name: "Ocean Archipelago",
    palette: PlanetPalette {
        sky: [130, 190, 230],
        atmosphere: [120, 200, 255],
        surface: [40, 110, 180],
        rock: [170, 160, 130],
        deep: [40, 50, 70],
        core: [90, 220, 255],
    },
    crust: TileKind::Wild(WildTile::SeaSand),
    crust_rows: 3,
    strata: &OCEAN_STRATA,
    core_ore: TileKind::Ore(MineralKind::Platinum),
    caves_per_256_columns: 2,
    hill_permille: 7,
};

/// The ocean's theme world.
pub struct OceanWorld;

/// The registered ocean world.
pub static OCEAN_WORLD: OceanWorld = OceanWorld;

/// The sea level: water fills every open tile from this row down to the seabed.
pub const SEA_LEVEL: i32 = 0;
/// The elevation at the shoreline (0-1023 noise; above it is land).
const SHORE: i64 = 600;
/// The Drowned Spire: half width, rows over the sea, and the open water kept round it.
pub const SPIRE_HALF: i64 = 4;
pub const SPIRE_HEIGHT: i32 = 22;
const SPIRE_SEA: i64 = 40;
/// Extra sky over the hills (the spire).
const EXTRA_SKY: i32 = 26;

/// The elevation field at `column` (0-1023; above [`SHORE`] is land), with the settlement's
/// island and the spire's open sea forced.
#[must_use]
pub fn elevation(ground: &PlanetGround, column: i64) -> i64 {
    let width = i64::from(ground.strip_width());
    let broad = ground.periodic_noise(60, column, 256);
    let fine = ground.periodic_noise(61, column, 32);
    let mut value = (broad * 3 + fine) / 4;
    let half = i64::from(super::settlement_width(ground) / 2);
    let island = settlement_column(ground) + half;
    let delta = wrap_delta(column, island, width).abs();
    if delta < half + 60 {
        value = value.max(900 - (delta - half).max(0) * 5);
    }
    let spire = wrap_delta(column, landmark_column(ground), width).abs();
    if spire < SPIRE_SEA + 30 {
        value = value.min(250 + (spire - SPIRE_SEA).max(0) * 10);
    }
    value
}

/// The deepest seabed (rows under the sea).
fn max_depth(ground: &PlanetGround) -> i64 {
    i64::from(ground.grid().radius() / 150).clamp(10, 40)
}

/// The ocean's surface row (the first ground row: an island's top or the seabed).
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let height = i64::from(ground.max_hill_rows());
    let natural = |column: i64| -> i32 {
        let value = elevation(ground, column);
        let rows = if value >= SHORE {
            -(value - SHORE) * height / (1_023 - SHORE)
        } else {
            (SHORE - value) * max_depth(ground) / SHORE
        };
        i32::try_from(rows).unwrap_or(0)
    };
    let half = i64::from(super::settlement_width(ground) / 2);
    let island = settlement_column(ground) + half;
    if let Some(weight) = site_weight(column, island, half, width) {
        return ease(natural(column), natural(island).min(-2), weight);
    }
    natural(column)
}

/// The seabed's depth under the sea at `column` (0 on land).
#[must_use]
pub fn sea_depth(ground: &PlanetGround, column: i64) -> i32 {
    (surface(ground, column) - SEA_LEVEL).max(0)
}

/// The Drowned Spire's tile at `(column, row)`, if any.
fn spire_tile(ground: &PlanetGround, column: i64, row: i32, floor: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let dx = wrap_delta(column, landmark_column(ground), width);
    if dx.abs() > SPIRE_HALF + 1 {
        return None;
    }
    let top = SEA_LEVEL - SPIRE_HEIGHT;
    // Its footing in the seabed.
    if row >= floor {
        return (row < floor + 3).then_some(wild(WildTile::RuinStone));
    }
    if row < top - 2 || dx.abs() > SPIRE_HALF {
        return None;
    }
    let wall = dx.abs() == SPIRE_HALF;
    if row < top {
        // Merlons on the crown.
        return (wall || dx.abs() == 1).then_some(wild(WildTile::RuinStone));
    }
    if row == top {
        // The roof, with a hole in the middle to drop in through.
        return Some(if dx.abs() <= 1 {
            air()
        } else {
            wild(WildTile::RuinStone)
        });
    }
    if wall {
        let window = (SEA_LEVEL - 6..SEA_LEVEL - 3).contains(&row);
        if window {
            return Some(air());
        }
        let band = row == SEA_LEVEL - 8 || row == SEA_LEVEL - 15 || row == SEA_LEVEL + 3;
        return Some(wild(if band {
            WildTile::RuinGlyph
        } else {
            WildTile::RuinStone
        }));
    }
    if row == SEA_LEVEL {
        return Some(wild(WildTile::RuinStone));
    }
    if row == SEA_LEVEL - 1 && dx == 0 {
        return Some(wild(WildTile::RuinRelic));
    }
    Some(if row > SEA_LEVEL { water() } else { air() })
}

/// A sealed hall of the Drowned Ruins: ring-tile rectangle `(first column, top row, width,
/// height)`.
type Hall = (i64, i32, i64, i32);

/// The hall of ruin cell `(cell_x, cell_y)`, if it has one.
fn hall_in(ground: &PlanetGround, cell_x: i64, cell_y: i32) -> Option<Hall> {
    let width = i64::from(ground.strip_width());
    let cells = width / 96;
    let hash = roll(ground, 30, cell_x.rem_euclid(cells), i64::from(cell_y));
    if hash % 100 >= 45 {
        return None;
    }
    let x = cell_x * 96 + 10 + i64::try_from((hash >> 8) % 50).unwrap_or(0);
    let y = cell_y * 40 + 6 + i32::try_from((hash >> 16) % 20).unwrap_or(0);
    let hall_width = 12 + i64::try_from((hash >> 24) % 9).unwrap_or(0);
    let hall_height = 6 + i32::try_from((hash >> 32) % 4).unwrap_or(0);
    Some((x, y, hall_width, hall_height))
}

/// The ruin tile at `cell` (in the Drowned Ruins), if a hall or its gallery is there.
fn ruin_tile(ground: &PlanetGround, cell: &Cell) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let span = cell.span.max(1);
    let (cell_x, cell_y) = (cell.first.div_euclid(96), cell.row.div_euclid(40));
    let (x, y, hall_width, hall_height) = hall_in(ground, cell_x, cell_y)?;
    let local = wrap_delta(cell.first, x, width) / span;
    let row = cell.row - y;
    let margin = 3;
    if local < -margin || local >= hall_width + margin || row < -3 || row >= hall_height + 3 {
        return None;
    }
    // Only halls whose middle lies in the ruins' stratum.
    let middle = i32::try_from(x + hall_width * span / 2).unwrap_or(0);
    if ground.stratum_index(middle, y + hall_height / 2) != 1 {
        return None;
    }
    let inside_x = (0..hall_width).contains(&local);
    let inside_y = (0..hall_height).contains(&row);
    if inside_x && inside_y {
        let wall = local == 0 || local == hall_width - 1 || row == 0 || row == hall_height - 1;
        if wall {
            let glyph = roll(ground, 31, cell.first, i64::from(cell.row)).is_multiple_of(8);
            return Some(wild(if glyph {
                WildTile::RuinGlyph
            } else {
                WildTile::RuinStone
            }));
        }
        if row == hall_height - 2 && local == hall_width / 2 {
            return Some(wild(WildTile::RuinRelic));
        }
        return Some(air());
    }
    // The flooded gallery round the hall.
    Some(water())
}

/// A black smoker: `(column, top row, height)` in ring tiles.
fn smoker_in(ground: &PlanetGround, cell_x: i64, cell_y: i32) -> Option<(i64, i32, i32)> {
    let width = i64::from(ground.strip_width());
    let cells = width / 48;
    let hash = roll(ground, 32, cell_x.rem_euclid(cells), i64::from(cell_y));
    if hash % 100 >= 50 {
        return None;
    }
    let x = cell_x * 48 + 8 + i64::try_from((hash >> 8) % 32).unwrap_or(0);
    let y = cell_y * 30 + 8 + i32::try_from((hash >> 16) % 14).unwrap_or(0);
    let height = 4 + i32::try_from((hash >> 24) % 5).unwrap_or(0);
    Some((x, y, height))
}

/// The smoker tile at `cell` (in the Trench), if one is there.
fn smoker_tile(ground: &PlanetGround, cell: &Cell) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let span = cell.span.max(1);
    let (x, y, height) = smoker_in(ground, cell.first.div_euclid(48), cell.row.div_euclid(30))?;
    let local = wrap_delta(cell.first, x, width) / span;
    let row = cell.row - y;
    if !(-6..=7).contains(&local) || !(-7..=height + 2).contains(&row) {
        return None;
    }
    if (0..=1).contains(&local) && (0..height).contains(&row) {
        return Some(wild(WildTile::Smoker));
    }
    if (-1..=2).contains(&local) && (-1..=height).contains(&row) {
        return Some(wild(WildTile::VentRock));
    }
    // The hot water over the chimney.
    let dx = local * 2 - 1;
    let dy = i64::from(row + 4) * 2;
    if dx * dx + dy * dy <= 100 && row < 0 {
        return Some(water());
    }
    // Rich ore round the vent.
    if (-4..=5).contains(&local) && (0..=height + 2).contains(&row) {
        let ore = roll(ground, 33, cell.first, i64::from(cell.row)) % 100;
        if ore < 14 {
            return Some(super::tile(TileKind::Ore(MineralKind::Platinum)));
        }
        if ore < 22 {
            return Some(super::tile(TileKind::Ore(MineralKind::Uranium)));
        }
    }
    None
}

/// Whether `cell` (in the Reef Shelf) is in a water pocket.
fn in_pocket(ground: &PlanetGround, cell: &Cell) -> bool {
    let width = i64::from(ground.strip_width());
    let span = cell.span.max(1);
    let (cell_x, cell_y) = (cell.first.div_euclid(32), cell.row.div_euclid(20));
    let cells = width / 32;
    let hash = roll(ground, 34, cell_x.rem_euclid(cells), i64::from(cell_y));
    if hash % 100 >= 40 {
        return false;
    }
    let x = cell_x * 32 + 6 + i64::try_from((hash >> 8) % 20).unwrap_or(0);
    let y = cell_y * 20 + 5 + i32::try_from((hash >> 16) % 10).unwrap_or(0);
    let half_width = 3 + i64::try_from((hash >> 24) % 4).unwrap_or(0);
    let half_height = 2 + i64::try_from((hash >> 32) % 3).unwrap_or(0);
    let dx = wrap_delta(cell.first, x, width) / span;
    let dy = i64::from(cell.row - y);
    dx * dx * half_height * half_height + dy * dy * half_width * half_width
        <= half_width * half_width * half_height * half_height
}

impl ThemeWorld for OceanWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::OCEAN
    }

    fn core_floor_row(&self, params: &PlanetParams, grid: &PolarGrid) -> Option<i32> {
        Some(core_floor_row(params, grid))
    }

    fn extra_sky_rows(&self) -> i32 {
        EXTRA_SKY
    }

    fn surface_row(&self, ground: &PlanetGround, column: i64) -> i32 {
        surface(ground, column)
    }

    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile> {
        let width = i64::from(ground.strip_width());
        let origin = chunk.origin();
        let near_spire = wrap_delta(i64::from(origin.x) + 16, landmark_column(ground), width).abs()
            <= SPIRE_HALF + 20;
        let small = is_small_body(ground.params());
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_spire
                    && cell.span == 1
                    && let Some(tile) = spire_tile(ground, cell.first, cell.row, cell.surface)
                {
                    return tile;
                }
                if small
                    && let Some(tile) = secret_tile(
                        ground,
                        cell,
                        wild(WildTile::RuinStone),
                        wild(WildTile::RuinGlyph),
                    )
                {
                    return tile;
                }
                if cell.row < cell.surface {
                    // The sea over the seabed; air over the waves.
                    if cell.row >= SEA_LEVEL {
                        // A reef's coral branch reaching up from the seabed.
                        if cell.row == cell.surface - 1
                            && cell.surface - SEA_LEVEL >= 3
                            && cell.surface - SEA_LEVEL <= 16
                            && roll(ground, 35, cell.first, 0).is_multiple_of(5)
                        {
                            return wild(WildTile::Coral);
                        }
                        return water();
                    }
                    return cell.base;
                }
                // The seabed's reefs and pearl shells.
                if cell.row == cell.surface && cell.surface > SEA_LEVEL {
                    let hash = roll(ground, 36, cell.first, 0);
                    if hash.is_multiple_of(97) {
                        return wild(WildTile::Pearlshell);
                    }
                    if cell.surface - SEA_LEVEL <= 16 && hash.is_multiple_of(3) {
                        return wild(if (hash >> 8).is_multiple_of(2) {
                            WildTile::Coral
                        } else {
                            WildTile::BrainCoral
                        });
                    }
                }
                match cell.stratum {
                    0 if cell.depth > 4 && in_pocket(ground, cell) => water(),
                    1 => ruin_tile(ground, cell).unwrap_or_else(|| {
                        // Every cave of the ruins is flooded.
                        if matches!(cell.base.kind, TileKind::Air) && cell.depth > 2 {
                            water()
                        } else {
                            cell.base
                        }
                    }),
                    2 => smoker_tile(ground, cell).unwrap_or_else(|| {
                        if matches!(cell.base.kind, TileKind::Air) {
                            water()
                        } else {
                            cell.base
                        }
                    }),
                    _ => cell.base,
                }
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        Some(super::settlement_site_of(ground, |column| {
            surface(ground, column)
        }))
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        let centre = landmark_column(ground);
        Some(SurfaceSite {
            first: i32::try_from(centre - SPIRE_HALF).unwrap_or(0),
            width: i32::try_from(SPIRE_HALF * 2 + 1).unwrap_or(9),
            floor: SEA_LEVEL,
        })
    }
}

/// Whether the black smoker nearest `(column, row)` is within `reach` ring tiles (the heat
/// rule reads the terrain instead; this is for tests and tools).
#[must_use]
pub fn smoker_near(ground: &PlanetGround, column: i64, row: i32, reach: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let (cell_x, cell_y) = (column.div_euclid(48), row.div_euclid(30));
    for ox in -1..=1 {
        for oy in -1..=1 {
            if let Some((x, y, height)) = smoker_in(ground, cell_x + ox, cell_y + oy) {
                let dx = wrap_delta(column, x, width).abs();
                if dx <= reach && row >= y - i32::try_from(reach).unwrap_or(0) && row < y + height {
                    return true;
                }
            }
        }
    }
    false
}
