//! The wild worlds (Wave 12 planets-a, expansion plan §16 "planet themes"): **jungle**,
//! **ocean archipelago** and **glacier** planets, each a world like the home ground level.
//!
//! Each theme is registered twice: as data in the planet theme registry
//! ([`super::theme::PLANET_THEMES`]: palette, crust, three strata with their rock, accent and
//! ores, the core ore) and as a [`super::ThemeWorld`] in [`super::world_of`]: its own surface
//! shape, the features over its strata, its strata depth, its settlement site and its
//! landmark. The space generator's tags map onto them in [`crate::place::planet_theme_of`]
//! (jungle, ocean archipelago, ice).
//!
//! | Theme | Surface | Strata (signature rule) | Landmark |
//! |---|---|---|---|
//! | [`jungle`] | rolling hills under giant trees: trunks, canopy, vines to climb | Root Tangle (living roots snag the tracks), Spore Caves (puffcaps burst into spore clouds), Heartwood Deep (lifesap mends the hull) | the Elder Tree |
//! | [`ocean`] | islands over a planet-wide sea, reefs and pearl shells on the seabed | Reef Shelf (waterlogged: drilled pockets flood the shaft), Drowned Ruins (sealed halls of air in flooded caverns), the Trench (black smokers scald, pressure crushes) | the Drowned Spire |
//! | [`glacier`] | terraced ice sheets, crevasses under snow bridges, frozen seas, ice spires | Glacier Ice (slick, and tunnels refreeze at night), the Under-Ice Sea (water so cold it bites three times as hard), Permafrost Caves (icicles fall on what passes under) | the Frozen Titan |
//!
//! **Depth** ([`core_floor_row`]): a planet's strata run 800-2,500 rows (scaled with its
//! radius) down to the core chamber, which sits on the indestructible heart of the body; a
//! small moon or crater rock (radius up to 1,000) is strata all the way to its true core.
//!
//! **Generation** is a pure function of the [`super::PlanetParams`] like every planet's: the
//! generic generator lays out the strata, ores and caves ([`super::PlanetGround`]), and each
//! theme's chunk pass adds its features at the ring tile's first column (so a ring tile of a
//! deep band still reads alike in every strip cell). Features are found from hashed cells
//! (trees per 32 columns, ruins per 96, ...), so a chunk never searches the body.
//!
//! The play rules (hazards, signature rules, weather, creatures) are [`life`]'s data and
//! `world/wilds.rs`'s tick.

pub mod glacier;
pub mod hooks;
pub mod jungle;
pub mod life;
pub mod ocean;
#[cfg(test)]
mod tests;
pub mod tile;

pub use life::{
    PlanetHazard, SignatureRule, WeatherKind, WildCreature, WildStratum, WildTheme, WildsEvent,
    wild_theme,
};
pub use tile::{WILD_TILE_CODE_BASE, WildTile};

use super::{CORE_CHAMBER_ROWS, CORE_FLOOR_ROWS, PlanetGround, PlanetParams, SurfaceSite, hash3};
use crate::{
    fluid::{FLUID_FULL, FluidKind},
    polar::PolarGrid,
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, Tile, TileKind, tile_durability},
};

/// The largest radius of a small body (moons and crater rocks: strata to the true core, and a
/// secret).
pub const SMALL_BODY_MAX_RADIUS: u32 = 1_000;
/// Strata depth of the smallest planet and the largest (rows from the datum to the core floor).
pub const STRATA_ROWS: (i32, i32) = (800, 2_500);
/// Planet radii the strata depth scales over (1,500 tiles: 800 rows; 8,000 and up: 2,500).
const STRATA_RADII: (i32, i32) = (1_500, 8_000);
/// The settlement site's width (columns of level ground).
pub const SETTLEMENT_WIDTH: i32 = 40;
/// Columns over which the ground eases from the hills to a site's level.
const SITE_BLEND: i32 = 12;
/// Rows of the secret's chamber under its marker.
pub const SECRET_DEPTH: i32 = 18;

/// The first row of the core floor of a wild world of `params` (see the module docs).
#[must_use]
pub fn core_floor_row(params: &PlanetParams, grid: &PolarGrid) -> i32 {
    let bottom = grid.bottom_row() - CORE_FLOOR_ROWS + 1;
    strata_rows(params.radius).min(bottom)
}

/// Rows from the datum to the core floor of a body of `radius` (small bodies: unbounded, the
/// grid's core floor decides).
#[must_use]
pub fn strata_rows(radius: u32) -> i32 {
    if radius <= SMALL_BODY_MAX_RADIUS {
        return i32::MAX;
    }
    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
    let span = STRATA_RADII.1 - STRATA_RADII.0;
    let scaled = STRATA_ROWS.0
        + (radius.clamp(STRATA_RADII.0, STRATA_RADII.1) - STRATA_RADII.0)
            * (STRATA_ROWS.1 - STRATA_ROWS.0)
            / span;
    scaled.clamp(STRATA_ROWS.0, STRATA_ROWS.1)
}

/// Whether `params` describe a small body (a moon or crater rock: it hides a secret).
#[must_use]
pub const fn is_small_body(params: &PlanetParams) -> bool {
    params.radius <= SMALL_BODY_MAX_RADIUS
}

/// `column - centre` folded into `-width/2..width/2` (the short way round the strip).
#[must_use]
pub const fn wrap_delta(column: i64, centre: i64, width: i64) -> i64 {
    let delta = (column - centre).rem_euclid(width);
    if delta >= width / 2 {
        delta - width
    } else {
        delta
    }
}

/// A hash of `(purpose, a, b)` for `ground`'s body.
#[must_use]
pub const fn roll(ground: &PlanetGround, purpose: u64, a: i64, b: i64) -> u64 {
    hash3(ground.params().seed, 0x5EED_0000 + purpose, a, b)
}

/// The settlement site's width.
///
/// [`SETTLEMENT_WIDTH`], or wide enough for the alien town that stands on it when a species
/// settled the body (its span and the fades either side, Wave 12 integration: the aliens
/// build on the theme's site).
#[must_use]
pub fn settlement_width(ground: &PlanetGround) -> i32 {
    crate::alien::AlienSpecies::from_folk(ground.params().folk).map_or(
        SETTLEMENT_WIDTH,
        |species| {
            SETTLEMENT_WIDTH.max(
                2 * (crate::alien::town::span_half(species) + crate::alien::town::FADE_COLUMNS + 2),
            )
        },
    )
}

/// The settlement site's first column (a pure function of the body).
#[must_use]
pub fn settlement_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (roll(ground, 1, 0, 0) % width.cast_unsigned()).cast_signed()
}

/// Where a settled wild world's other towns stand (Wave 13 QA: several alien towns), in
/// eighths of a lap on from the theme's own settlement: three quarters on (a quarter back),
/// then three eighths on. Clear of the landmark half a lap on and of a small body's secret a
/// quarter on.
const OTHER_SITE_EIGHTHS: [i64; 2] = [6, 3];

/// The centre columns of every settlement site of a wild world, the theme's own first.
///
/// A pure function of the body. A body the aliens settled holds as many sites as it has towns
/// ([`crate::alien::town::town_count`]: two or three on a planet, one on a moon), spread round
/// the lap; each is levelled, dry and clear like the first.
#[must_use]
pub fn settlement_centres(ground: &PlanetGround) -> Vec<i64> {
    let width = i64::from(ground.strip_width());
    let first =
        (settlement_column(ground) + i64::from(settlement_width(ground) / 2)).rem_euclid(width);
    let towns = usize::from(crate::alien::town::town_count(ground.params())).max(1);
    let mut centres = vec![first];
    for eighths in OTHER_SITE_EIGHTHS.iter().take(towns - 1) {
        centres.push((first + width * eighths / 8).rem_euclid(width));
    }
    centres
}

/// The settlement site's centre nearest `column` round the lap (each site shapes the ground
/// round it).
#[must_use]
pub fn nearest_settlement(ground: &PlanetGround, column: i64) -> i64 {
    let width = i64::from(ground.strip_width());
    let centres = settlement_centres(ground);
    let mut best = centres[0];
    for centre in centres.into_iter().skip(1) {
        if wrap_delta(column, centre, width).abs() < wrap_delta(column, best, width).abs() {
            best = centre;
        }
    }
    best
}

/// Every settlement site of a wild world whose surface is `surface` (see
/// [`settlement_centres`]).
#[must_use]
pub fn settlement_sites_of(
    ground: &PlanetGround,
    surface: impl Fn(i64) -> i32,
) -> Vec<SurfaceSite> {
    let width = settlement_width(ground);
    settlement_centres(ground)
        .into_iter()
        .map(|centre| SurfaceSite {
            first: i32::try_from(centre - i64::from(width / 2)).unwrap_or(0),
            width,
            floor: surface(centre),
        })
        .collect()
}

/// The landmark's centre column: half a lap from the settlement.
#[must_use]
pub fn landmark_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (settlement_column(ground) + i64::from(settlement_width(ground) / 2) + width / 2)
        .rem_euclid(width)
}

/// The secret's column on a small body: a quarter lap from the settlement.
#[must_use]
pub fn secret_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (settlement_column(ground) + width / 4).rem_euclid(width)
}

/// How far `column` lies inside a level site centred on `centre` (`half` columns either
/// side): `Some(1024)` on the site, falling to 0 across the blend, `None` beyond.
#[must_use]
pub fn site_weight(column: i64, centre: i64, half: i64, width: i64) -> Option<i64> {
    let delta = wrap_delta(column, centre, width).abs();
    if delta <= half {
        Some(1_024)
    } else if delta <= half + i64::from(SITE_BLEND) {
        Some((half + i64::from(SITE_BLEND) - delta) * 1_024 / i64::from(SITE_BLEND))
    } else {
        None
    }
}

/// `natural` eased toward `level` by `weight` (0-1024).
#[must_use]
pub fn ease(natural: i32, level: i32, weight: i64) -> i32 {
    let natural64 = i64::from(natural);
    let eased = natural64 + (i64::from(level) - natural64) * weight / 1_024;
    i32::try_from(eased).unwrap_or(natural)
}

/// The settlement site of a wild world whose surface is `surface`.
#[must_use]
pub fn settlement_site_of(ground: &PlanetGround, surface: impl Fn(i64) -> i32) -> SurfaceSite {
    let first = settlement_column(ground);
    let width = settlement_width(ground);
    let floor = surface(first + i64::from(width / 2));
    SurfaceSite {
        first: i32::try_from(first).unwrap_or(0),
        width,
        floor,
    }
}

/// A full tile of `kind`.
#[must_use]
pub const fn tile(kind: TileKind) -> Tile {
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

/// A wild tile.
#[must_use]
pub const fn wild(tile_kind: WildTile) -> Tile {
    tile(TileKind::Wild(tile_kind))
}

/// A full tile of water.
#[must_use]
pub const fn water() -> Tile {
    Tile {
        kind: TileKind::Fluid(FluidKind::Water),
        durability: FLUID_FULL,
    }
}

/// The open air.
#[must_use]
pub const fn air() -> Tile {
    tile(TileKind::Air)
}

/// What a theme's chunk pass sees of one ring tile.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    /// The ring tile's first strip column (every cell of it reads alike).
    pub first: i64,
    pub row: i32,
    /// The first ground row of the column (the theme's surface).
    pub surface: i32,
    /// The generic generator's tile (strata rock, ores, caves, core chamber).
    pub base: Tile,
    /// The stratum index (0..3; above ground: 0).
    pub stratum: usize,
    /// Rows below the surface.
    pub depth: i32,
    /// Columns one strip column spans at this row (2^band).
    pub span: i64,
}

impl Cell {
    /// Whether the cell is in the core chamber or the floor under it (left as generated).
    #[must_use]
    pub const fn in_core(&self, ground: &PlanetGround) -> bool {
        self.row >= ground.core_chamber_top()
    }
}

/// The chunk `chunk` of `ground`, each ring tile's generic tile passed through `overlay`.
pub fn chunk_with(
    ground: &PlanetGround,
    chunk: ChunkPosition,
    surface: impl Fn(i64) -> i32,
    mut overlay: impl FnMut(&Cell) -> Tile,
) -> Vec<Tile> {
    let caves = ground.caves();
    let origin = chunk.origin();
    let grid = ground.grid();
    let mut tiles = Vec::with_capacity((CHUNK_SIZE_TILES * CHUNK_SIZE_TILES) as usize);
    // One ring tile may cover several cells of a chunk row: reuse its answer.
    let mut last: Option<(i64, i32, Tile)> = None;
    for y in origin.y..origin.y + CHUNK_SIZE_TILES {
        let span = i64::from(grid.span_at(y));
        for x in origin.x..origin.x + CHUNK_SIZE_TILES {
            let ring = grid.tile_of(i64::from(x), y);
            let first = i64::from(grid.first_strip_column(ring));
            if let Some((column, row, tile)) = last
                && column == first
                && row == y
            {
                tiles.push(tile);
                continue;
            }
            let base = ground.generated_with(&caves, x, y);
            let surface_row = surface(first);
            let tile = if y > ground.bottom_row() || y >= ground.core_floor_row() {
                base
            } else {
                let stratum = if y < surface_row {
                    0
                } else {
                    ground.stratum_index(i32::try_from(first).unwrap_or(0), y)
                };
                overlay(&Cell {
                    first,
                    row: y,
                    surface: surface_row,
                    base,
                    stratum,
                    depth: y - surface_row,
                    span,
                })
            };
            last = Some((first, y, tile));
            tiles.push(tile);
        }
    }
    tiles
}

/// The secret's tile at `cell`, if it is part of it (small bodies only).
///
/// A marker stone on the surface over a small chamber [`SECRET_DEPTH`] rows down holding a
/// [`WildTile::Cache`]. `rock` lines the chamber, `marker` is the cairn.
#[must_use]
pub fn secret_tile(ground: &PlanetGround, cell: &Cell, rock: Tile, marker: Tile) -> Option<Tile> {
    if !is_small_body(ground.params()) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let delta = wrap_delta(cell.first, secret_column(ground), width);
    if !(-3..=3).contains(&delta) {
        return None;
    }
    // The cairn: two stones on the surface over the chamber.
    if delta == 0 && (cell.row == cell.surface - 1 || cell.row == cell.surface - 2) {
        return Some(marker);
    }
    let top = cell.surface + SECRET_DEPTH;
    if cell.row < top - 1 || cell.row > top + 3 {
        return None;
    }
    let inside = (-2..=2).contains(&delta) && (top..=top + 2).contains(&cell.row);
    if inside {
        if delta == 0 && cell.row == top + 2 {
            return Some(wild(WildTile::Cache));
        }
        return Some(air());
    }
    Some(rock)
}

/// Whether row `row` lies in the open core chamber of `ground`.
#[must_use]
pub const fn in_core_chamber(ground: &PlanetGround, row: i32) -> bool {
    row >= ground.core_floor_row() - CORE_CHAMBER_ROWS && row < ground.core_floor_row()
}
