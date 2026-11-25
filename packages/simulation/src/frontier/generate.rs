//! Frontier ground generation: the hooks [`crate::planet::PlanetGround`] calls for a frontier
//! theme (a pure function of the planet's params; integer noise only).
//!
//! * [`surface_height`]: the surface's shape (rows above the datum; negative in a basin).
//! * [`feature_tile`]: everything that is not plain crust, rock or ore: water and palms at the
//!   oases, lava seas and crater lakes, the spires, the landmarks, the secrets, the ruins,
//!   galleries, lava tubes, geodes, light wells and magma pools. `None` leaves the tile to the
//!   generic generator (crust, strata rock and accent, ores, caves).
//!
//! Everything is laid out in whole cells of the strip (64, 128, 512 and 1,024 columns, which
//! divide every lap: a lap is a whole number of 512-column sectors), so nothing tears at the
//! seam. Structures that need square tiles (ruins, light wells, secrets) stay above the first
//! ring-halving row.

use super::{FrontierStyle, FrontierTile, Landmark, Secret};
use crate::{
    fluid::FluidKind,
    planet::{PlanetGround, hash3},
    terrain::{Tile, TileKind, TilePosition, tile_durability},
};

/// The caves of a planet: `(centre column, centre row, half width, half height)`.
pub type Caves = [(i64, i32, i64, i32)];

/// Rows above the datum of a settlement site's flat ground.
pub const SETTLEMENT_HEIGHT: i32 = 2;

/// Columns of a dune (the wind's wavelength).
pub const DUNE_COLUMNS: i64 = 64;
/// Columns of a crystal world's surface cell (the settlement and the Choir Spire take one).
const MEADOW_COLUMNS: i64 = 128;
/// Columns of a spire cell (at most one spire each).
const SPIRE_COLUMNS: i64 = 24;
/// Columns of a volcano cell (one cone each).
pub const CONE_COLUMNS: i64 = 512;
/// Columns of a light-well cell (one well each).
const WELL_COLUMNS: i64 = 1_024;
/// Rows of a gallery or lava-tube band (one tunnel each).
pub const BAND_ROWS: i32 = 40;

// Purposes for `hash3` (distinct from the planet generator's 1-41).
const P_DUNE: u64 = 60;
const P_SETTLEMENT: u64 = 61;
const P_OASIS: u64 = 62;
const P_PALM: u64 = 63;
const P_CONE: u64 = 64;
const P_SPIRE: u64 = 65;
const P_WELL: u64 = 66;
const P_RUIN: u64 = 67;
const P_GEODE: u64 = 68;
const P_GAS: u64 = 71;
const P_HEART: u64 = 72;
const P_SECRET: u64 = 73;
const P_MEANDER: u64 = 74;
pub(crate) const P_TUBE_WAY: u64 = 75;
const P_GAP: u64 = 76;

const fn tile(kind: FrontierTile) -> Tile {
    let kind = TileKind::Frontier(kind);
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

const fn plain(kind: TileKind) -> Tile {
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

const AIR: Tile = plain(TileKind::Air);
const LAVA: Tile = plain(TileKind::Lava);
const WATER: Tile = crate::fluid::FluidCell::full(FluidKind::Water).tile();

fn width(ground: &PlanetGround) -> i64 {
    i64::from(ground.grid().surface_columns())
}

/// Signed distance from `from` to `column` round the strip (shortest way).
fn round_dx(ground: &PlanetGround, column: i64, from: i64) -> i64 {
    let w = width(ground);
    let mut dx = (column - from).rem_euclid(w);
    if dx > w / 2 {
        dx -= w;
    }
    dx
}

const fn seed(ground: &PlanetGround) -> u64 {
    ground.params().seed
}

/// The cell index of `column` for cells of `size` columns.
fn cell(ground: &PlanetGround, column: i64, size: i64) -> i64 {
    column.rem_euclid(width(ground)).div_euclid(size)
}

fn cells(ground: &PlanetGround, size: i64) -> i64 {
    (width(ground) / size).max(1)
}

/// The generic rolling hills (the crystal world keeps them).
fn generic_hill(ground: &PlanetGround, column: i64) -> i32 {
    let fine = ground.periodic_noise(1, column, 32);
    let broad = ground.periodic_noise(2, column, 128);
    let max = i64::from(ground.max_hill_rows());
    i32::try_from((broad * 3 + fine) * max / (4 * 1_023)).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Where things are.
// ---------------------------------------------------------------------------------------------

/// The cell size each style lays its settlement and landmark out in.
const fn site_cell(style: FrontierStyle) -> i64 {
    match style {
        FrontierStyle::Desert => DUNE_COLUMNS,
        FrontierStyle::Volcanic => CONE_COLUMNS,
        FrontierStyle::Crystal => MEADOW_COLUMNS,
    }
}

/// The settlement's cell index.
fn settlement_cell(ground: &PlanetGround, style: FrontierStyle) -> i64 {
    let count = cells(ground, site_cell(style));
    i64::try_from(hash3(seed(ground), P_SETTLEMENT, 0, 0) % count.cast_unsigned()).unwrap_or(0)
}

/// The landmark's cell index (half a lap from the settlement; the Caldera is cone 0's cell
/// unless that is the settlement's).
fn landmark_cell(ground: &PlanetGround, style: FrontierStyle) -> i64 {
    let count = cells(ground, site_cell(style));
    let settlement = settlement_cell(ground, style);
    if count <= 1 {
        return 0;
    }
    (settlement + count / 2).rem_euclid(count)
}

/// The centre column of the settlement site.
#[must_use]
pub fn settlement_centre(ground: &PlanetGround, style: FrontierStyle) -> i32 {
    let size = site_cell(style);
    let cell = settlement_cell(ground, style);
    let centre = match style {
        // Between the cell's cone and the next (the cone stays in 96..416 of its cell).
        FrontierStyle::Volcanic => cell * size + 456,
        _ => cell * size + size / 2,
    };
    i32::try_from(centre).unwrap_or(0)
}

/// The landmark's centre column.
fn landmark_centre(ground: &PlanetGround, style: FrontierStyle) -> i64 {
    if style == FrontierStyle::Volcanic {
        return cone_at(ground, landmark_cell(ground, style)).centre;
    }
    let size = site_cell(style);
    landmark_cell(ground, style) * size + size / 2
}

/// One volcano cone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cone {
    /// Its index (one per 512-column cell).
    pub index: i64,
    pub centre: i64,
    /// Half its base width, columns.
    pub radius: i64,
    /// Its peak, rows above the datum.
    pub peak: i32,
    /// Half the crater's width.
    pub crater: i64,
    /// Rows the crater dips below the peak.
    pub crater_depth: i32,
    /// The Caldera (the landmark; a lava lake in its crater).
    pub caldera: bool,
}

/// The cone of 512-column cell `index`.
#[must_use]
pub fn cone_at(ground: &PlanetGround, index: i64) -> Cone {
    let roll = hash3(seed(ground), P_CONE, index, 0);
    let max = ground.max_hill_rows();
    let caldera = index == landmark_cell_volcanic(ground);
    let radius = if caldera {
        64
    } else {
        36 + i64::try_from(roll % 24).unwrap_or(0)
    };
    let peak = if caldera {
        max
    } else {
        max * (7 + i32::try_from((roll >> 8) % 3).unwrap_or(0)) / 10
    };
    Cone {
        index,
        centre: index * CONE_COLUMNS + 176 + i64::try_from((roll >> 16) % 160).unwrap_or(0),
        radius,
        peak,
        crater: if caldera { 12 } else { (radius / 7).max(4) },
        crater_depth: if caldera { 8 } else { 4 },
        caldera,
    }
}

fn landmark_cell_volcanic(ground: &PlanetGround) -> i64 {
    landmark_cell(ground, FrontierStyle::Volcanic)
}

/// Every cone of a volcanic world, in order round the strip.
#[must_use]
pub fn volcano_cones(ground: &PlanetGround) -> Vec<Cone> {
    (0..cells(ground, CONE_COLUMNS))
        .map(|index| cone_at(ground, index))
        .collect()
}

/// A cone's height (rows above the datum) at `dx` columns from its centre, if inside it.
fn cone_height(cone: &Cone, dx: i64) -> Option<i32> {
    let adx = dx.abs();
    if adx >= cone.radius {
        return None;
    }
    let peak = i64::from(cone.peak);
    let slope = peak - adx * peak / cone.radius;
    let height = if adx < cone.crater {
        let dip = i64::from(cone.crater_depth) * (cone.crater * cone.crater - adx * adx)
            / (cone.crater * cone.crater);
        peak - dip
    } else {
        slope
    };
    i32::try_from(height).ok()
}

/// A crystal light well: a shaft of sunlight down to a lens, a prism below it and a receptor
/// that opens a sealed geode vault.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LightWell {
    /// The well's column.
    pub column: i32,
    /// The lens's row (it sends the beam down).
    pub lens_row: i32,
    /// The prism (it turns the beam toward the receptor).
    pub prism: TilePosition,
    /// The receptor.
    pub receptor: TilePosition,
    /// The vault's walls: first and last column, top and bottom row (inclusive).
    pub vault: (i32, i32, i32, i32),
    /// Whether the beam turns east (else west).
    pub east: bool,
}

impl LightWell {
    /// Whether `tile` is one of the vault's seal tiles (its walls).
    #[must_use]
    pub const fn is_seal(&self, tile: TilePosition) -> bool {
        let (x0, x1, y0, y1) = self.vault;
        tile.x >= x0
            && tile.x <= x1
            && tile.y >= y0
            && tile.y <= y1
            && (tile.x == x0 || tile.x == x1 || tile.y == y0 || tile.y == y1)
    }

    /// Whether `tile` is inside the vault (walls excluded).
    #[must_use]
    pub const fn inside(&self, tile: TilePosition) -> bool {
        let (x0, x1, y0, y1) = self.vault;
        tile.x > x0 && tile.x < x1 && tile.y > y0 && tile.y < y1
    }
}

/// The light well of 1,024-column cell `index`, if the body has room for it above its first
/// ring-halving row.
#[must_use]
pub fn light_well_at(ground: &PlanetGround, index: i64) -> Option<LightWell> {
    let w = width(ground);
    let roll = hash3(seed(ground), P_WELL, index, 0);
    let mut column = index * WELL_COLUMNS + 180 + i64::try_from(roll % 560).unwrap_or(0);
    // Clear of the settlement and the Choir Spire.
    for centre in [
        i64::from(settlement_centre(ground, FrontierStyle::Crystal)),
        landmark_centre(ground, FrontierStyle::Crystal),
    ] {
        if round_dx(ground, column, centre).abs() < 48 {
            column += 96;
        }
    }
    if column + 16 >= (index + 1) * WELL_COLUMNS.min(w) && w >= WELL_COLUMNS {
        column -= 192;
    }
    let column = column.rem_euclid(w);
    let surface = -surface_height_of(ground, FrontierStyle::Crystal, column);
    let meadows = super::strata_rows(ground.grid().radius()) * 250 / 1_000;
    let lens_row = surface + meadows + 8;
    let east = (roll >> 20) & 1 == 0;
    let way: i32 = if east { 1 } else { -1 };
    let drop = 2 + i32::try_from((roll >> 24) % 3).unwrap_or(0);
    let run = 3 + i32::try_from((roll >> 28) % 4).unwrap_or(0);
    let column = i32::try_from(column).ok()?;
    let prism = TilePosition {
        x: column,
        y: lens_row + 3 + drop,
    };
    let receptor = TilePosition {
        x: column + way * (run + 1),
        y: prism.y,
    };
    let near = receptor.x + way;
    let far = receptor.x + way * 9;
    let vault = (near.min(far), near.max(far), prism.y - 3, prism.y + 3);
    let halving = ground
        .grid()
        .halving_rows()
        .first()
        .copied()
        .unwrap_or(i32::MAX);
    if vault.3 + 4 >= halving || ground.in_core_chamber(vault.3) {
        return None;
    }
    Some(LightWell {
        column,
        lens_row,
        prism,
        receptor,
        vault,
        east,
    })
}

/// Every light well of a crystal world.
#[must_use]
pub fn light_wells(ground: &PlanetGround) -> Vec<LightWell> {
    (0..cells(ground, WELL_COLUMNS))
        .filter_map(|index| light_well_at(ground, index))
        .collect()
}

/// The light well whose layout covers `(column, row)`, if any (the well's own cell and its
/// neighbours: a vault may lean across a cell edge).
fn well_near(ground: &PlanetGround, column: i64) -> Option<LightWell> {
    let count = cells(ground, WELL_COLUMNS);
    let here = cell(ground, column, WELL_COLUMNS);
    [
        here,
        (here + 1).rem_euclid(count),
        (here - 1).rem_euclid(count),
    ]
    .into_iter()
    .filter_map(|index| light_well_at(ground, index))
    .find(|well| round_dx(ground, column, i64::from(well.column)).abs() <= 24)
}

/// The secret's layout on a small body: the pocket's centre column and its top row.
fn secret_layout(ground: &PlanetGround, style: FrontierStyle) -> (i64, i32) {
    let w = width(ground);
    let mut column =
        i64::try_from(hash3(seed(ground), P_SECRET, 0, 0) % w.cast_unsigned()).unwrap_or(0);
    for centre in [
        i64::from(settlement_centre(ground, style)),
        landmark_centre(ground, style),
    ] {
        if round_dx(ground, column, centre).abs() < 96 {
            column = (column + w / 3).rem_euclid(w);
        }
    }
    // Keep it off a spire, a palm or a cone's slope: a spot of plain ground (the next few
    // columns over are as good).
    let surface = -surface_height_of(ground, style, column);
    let halving = ground
        .grid()
        .halving_rows()
        .first()
        .copied()
        .unwrap_or(i32::MAX);
    let depth = (super::strata_rows(ground.grid().radius()) * 45 / 100)
        .clamp(20, (halving - surface - 12).max(20));
    (column, surface + depth)
}

/// The secret of a small frontier body (see [`super::secret`]).
#[must_use]
pub fn secret_of(ground: &PlanetGround, style: FrontierStyle) -> Secret {
    let (column, top) = secret_layout(ground, style);
    let column = i32::try_from(column).unwrap_or(0);
    Secret {
        style,
        prize: TilePosition {
            x: column,
            y: top + 3,
        },
        column,
    }
}

/// The landmark of a frontier body (see [`super::landmark`]).
#[must_use]
pub fn landmark_of(ground: &PlanetGround, style: FrontierStyle) -> Landmark {
    let centre = landmark_centre(ground, style);
    let surface = -surface_height_of(ground, style, centre);
    let column = i32::try_from(centre).unwrap_or(0);
    match style {
        FrontierStyle::Desert => Landmark {
            style,
            column,
            top_row: SENTINEL_CENTRE_ROW - SENTINEL_HALF_HEIGHT,
            treasure: TilePosition {
                x: column,
                y: surface + SENTINEL_VAULT_DEPTH + 4,
            },
        },
        FrontierStyle::Volcanic => {
            let cone = cone_at(ground, landmark_cell(ground, style));
            Landmark {
                style,
                column,
                top_row: -cone.peak,
                // The magma heart in the Caldera's throat, under its lake.
                treasure: TilePosition {
                    x: column,
                    y: -cone.peak + cone.crater_depth + CALDERA_THROAT_ROWS,
                },
            }
        }
        FrontierStyle::Crystal => Landmark {
            style,
            column,
            top_row: surface - CHOIR_HEIGHT,
            treasure: TilePosition {
                x: column,
                y: surface + CHOIR_GEODE_DEPTH + CHOIR_GEODE_RADIUS - 2,
            },
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The surface.
// ---------------------------------------------------------------------------------------------

/// Rows above the datum of the surface at strip column `column` of a frontier world.
#[must_use]
pub fn surface_height(ground: &PlanetGround, style: FrontierStyle, column: i64) -> i32 {
    surface_height_of(ground, style, column)
}

fn surface_height_of(ground: &PlanetGround, style: FrontierStyle, column: i64) -> i32 {
    let natural = match style {
        FrontierStyle::Desert => dune_height(ground, column),
        FrontierStyle::Volcanic => island_height(ground, column),
        FrontierStyle::Crystal => generic_hill(ground, column),
    };
    // The settlement's flat ground, blended into the land over eight columns each side.
    let dx = round_dx(ground, column, i64::from(settlement_centre(ground, style))).abs();
    let flat_half = i64::from(super::SETTLEMENT_WIDTH / 2);
    if dx <= flat_half {
        return SETTLEMENT_HEIGHT;
    }
    if dx < flat_half + 8 {
        let t = dx - flat_half;
        let blended = (i64::from(SETTLEMENT_HEIGHT) * (8 - t) + i64::from(natural) * t) / 8;
        return i32::try_from(blended).unwrap_or(natural);
    }
    // The landmarks stand on flat ground of their own.
    if style != FrontierStyle::Volcanic {
        let dx = round_dx(ground, column, landmark_centre(ground, style)).abs();
        let half = if style == FrontierStyle::Desert {
            26
        } else {
            12
        };
        if dx <= half {
            return 1;
        }
        if dx < half + 6 {
            let t = dx - half;
            return i32::try_from((6 - t + i64::from(natural) * t) / 6).unwrap_or(natural);
        }
    }
    natural
}

/// The dune world's dunes: a long windward rise and a steep slip face every 64 columns, each
/// dune its own height, over a broad swell; an oasis bowl in some dune hollows.
fn dune_height(ground: &PlanetGround, column: i64) -> i32 {
    let max = i64::from(ground.max_hill_rows());
    let index = cell(ground, column, DUNE_COLUMNS);
    let t = column.rem_euclid(DUNE_COLUMNS);
    let broad = ground.periodic_noise(2, column, 128);
    let base = broad * max / (4 * 1_023);
    let amp_roll = i64::try_from(hash3(seed(ground), P_DUNE, index, 0) % 1_000).unwrap_or(0);
    let amp = max * 3 / 4 * (450 + amp_roll * 55 / 100) / 1_000;
    // Windward rise over 48 columns, the slip face over 16 (the wind blows east).
    let p = if t < 48 {
        t * 1_024 / 48
    } else {
        (DUNE_COLUMNS - t) * 1_024 / 16
    };
    // Smooth the crest and the foot.
    let p = p * p * (3 * 1_024 - 2 * p) / (1_024 * 1_024);
    let natural = i32::try_from(base + amp * p / 1_024).unwrap_or(0);
    if is_oasis(ground, index) {
        let dx = (t - 32).abs();
        if dx < OASIS_RADIUS {
            let dip = OASIS_DEPTH * (OASIS_RADIUS * OASIS_RADIUS - dx * dx)
                / (OASIS_RADIUS * OASIS_RADIUS);
            return -i32::try_from(dip).unwrap_or(0);
        }
        return natural.min(i32::try_from((dx - OASIS_RADIUS) / 2).unwrap_or(0));
    }
    natural
}

/// Half an oasis bowl's width.
const OASIS_RADIUS: i64 = 18;
/// Rows an oasis bowl dips below the datum.
const OASIS_DEPTH: i64 = 5;

/// Whether dune `index` holds an oasis.
fn is_oasis(ground: &PlanetGround, index: i64) -> bool {
    let style = FrontierStyle::Desert;
    index != settlement_cell(ground, style)
        && index != landmark_cell(ground, style)
        && hash3(seed(ground), P_OASIS, index, 0).is_multiple_of(5)
}

/// Every oasis's centre column (the bowl's middle), for the creatures and the renderer.
#[must_use]
pub fn oases(ground: &PlanetGround) -> Vec<i64> {
    (0..cells(ground, DUNE_COLUMNS))
        .filter(|index| is_oasis(ground, *index))
        .map(|index| index * DUNE_COLUMNS + 32)
        .collect()
}

/// The volcanic world's islands and seas, with a cone per 512 columns.
fn island_height(ground: &PlanetGround, column: i64) -> i32 {
    let max = i64::from(ground.max_hill_rows());
    let n = ground.periodic_noise(2, column, 128);
    let rough = (ground.periodic_noise(1, column, 32) - 512) * 2 / 512;
    let island = (n - 420) * (max / 3) / 603;
    let base = if island < 0 {
        island.max(-6)
    } else {
        island + rough.max(0)
    };
    let base = i32::try_from(base).unwrap_or(0);
    let index = cell(ground, column, CONE_COLUMNS);
    let count = cells(ground, CONE_COLUMNS);
    [
        index,
        (index + 1).rem_euclid(count),
        (index - 1).rem_euclid(count),
    ]
    .into_iter()
    .map(|index| cone_at(ground, index))
    .filter_map(|cone| cone_height(&cone, round_dx(ground, column, cone.centre)))
    .fold(base, i32::max)
}

/// Rows from the Caldera's crater floor down to its magma heart.
const CALDERA_THROAT_ROWS: i32 = 6;

// The Sleeping Sentinel: a giant stone head half buried in the dune world's sand.
const SENTINEL_CENTRE_ROW: i32 = -8;
const SENTINEL_HALF_WIDTH: i64 = 12;
const SENTINEL_HALF_HEIGHT: i32 = 9;
const SENTINEL_VAULT_DEPTH: i32 = 9;

// The Choir Spire: two tall prongs over a crystal plinth.
const CHOIR_HEIGHT: i32 = 22;
const CHOIR_GEODE_DEPTH: i32 = 14;
const CHOIR_GEODE_RADIUS: i32 = 5;

/// A frontier tile at strip tile `(column, row)` (the ring tile's first column) that is not
/// plain crust, strata rock or ore, if any (see the module docs).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one block per feature")]
pub fn feature_tile(
    ground: &PlanetGround,
    style: FrontierStyle,
    caves: &Caves,
    column: i64,
    row: i32,
    surface: i32,
) -> Option<Tile> {
    // The secret of a small body first (it sits in any stratum).
    if ground.grid().radius() <= super::SECRET_MAX_RADIUS
        && let Some(tile) = secret_tile(ground, style, column, row, surface)
    {
        return Some(tile);
    }
    let feature = match style {
        FrontierStyle::Desert => desert_tile(ground, column, row, surface),
        FrontierStyle::Volcanic => volcanic_tile(ground, caves, column, row, surface),
        FrontierStyle::Crystal => crystal_tile(ground, column, row, surface),
    };
    if feature.is_some() || row < surface + ground.theme().crust_rows {
        return feature;
    }
    if ground.in_cave_at(caves, column, row) {
        return None;
    }
    Some(fill_tile(ground, style, column, row))
}

/// How a stratum's accent rock lies in its common rock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pattern {
    /// Wavy layers a few rows thick (sand beds, lava flows, prismatic bands).
    Layers,
    /// Rounded blobs (ash pockets, crystal clusters, magma).
    Blobs,
    /// Thin wandering veins (glass, obsidian, lattice crystal).
    Veins,
}

const fn pattern(style: FrontierStyle, stratum: usize) -> Pattern {
    match (style, stratum) {
        (FrontierStyle::Desert, 0) | (_, 1) => Pattern::Layers,
        (FrontierStyle::Desert | FrontierStyle::Crystal, 3) | (FrontierStyle::Volcanic, 2) => {
            Pattern::Veins
        }
        _ => Pattern::Blobs,
    }
}

const P_FILL: u64 = 77;
const P_ORE: u64 = 40;

/// Smooth 2-D value noise (`0..=1023`) on a lattice of `cx` columns by `cy` rows; `cx` divides
/// 512, so it wraps with the strip.
fn noise2(ground: &PlanetGround, purpose: u64, x: i64, y: i32, cx: i64, cy: i32) -> i64 {
    let w = width(ground);
    let cells = (w / cx).max(1);
    let (ix, iy) = (x.rem_euclid(w).div_euclid(cx), y.div_euclid(cy));
    let (fx, fy) = (x.rem_euclid(w).rem_euclid(cx), i64::from(y.rem_euclid(cy)));
    let cy = i64::from(cy);
    let value = |a: i64, b: i32| -> i64 {
        (hash3(seed(ground), purpose, a.rem_euclid(cells), i64::from(b)) >> 54).cast_signed()
    };
    let smooth = |t: i64, n: i64| t * t * (3 * n - 2 * t) * 1_024 / (n * n * n);
    let (sx, sy) = (smooth(fx, cx), smooth(fy, cy));
    let top = value(ix, iy) + (value(ix + 1, iy) - value(ix, iy)) * sx / 1_024;
    let bottom = value(ix, iy + 1) + (value(ix + 1, iy + 1) - value(ix, iy + 1)) * sx / 1_024;
    top + (bottom - top) * sy / 1_024
}

/// A stratum's ground at `(column, row)`: its ores, then its accent rock in the stratum's
/// pattern, else its common rock.
fn fill_tile(ground: &PlanetGround, style: FrontierStyle, column: i64, row: i32) -> Tile {
    let index = ground.stratum_index(i32::try_from(column).unwrap_or(0), row);
    let stratum = &ground.theme().strata[index];
    let roll = hash3(seed(ground), P_ORE, column, i64::from(row));
    let ore_roll = u16::try_from(roll % 10_000).unwrap_or(0);
    let mut threshold = 0_u16;
    for (mineral, chance) in stratum.ores {
        threshold = threshold.saturating_add(*chance);
        if ore_roll < threshold {
            return plain(TileKind::Ore(*mineral));
        }
    }
    let purpose = P_FILL + u64::try_from(index).unwrap_or(0) * 3;
    let accent = i64::from(stratum.accent_permille);
    let is_accent = match pattern(style, index) {
        Pattern::Layers => {
            let wave = ground.periodic_noise(purpose, column, 64) * 8 / 1_024;
            let layer = (row + i32::try_from(wave).unwrap_or(0)).div_euclid(3);
            i64::try_from(
                hash3(
                    seed(ground),
                    purpose + 1,
                    column.div_euclid(32),
                    i64::from(layer),
                ) % 1_000,
            )
            .unwrap_or(0)
                < accent
        }
        Pattern::Blobs => noise2(ground, purpose, column, row, 8, 6) < accent * 1_023 / 1_000 + 40,
        Pattern::Veins => {
            let n = noise2(ground, purpose, column, row, 16, 10);
            (n - 512).abs() < accent / 6 + 10
        }
    };
    plain(if is_accent {
        stratum.accent
    } else {
        stratum.rock
    })
}

fn secret_tile(
    ground: &PlanetGround,
    style: FrontierStyle,
    column: i64,
    row: i32,
    surface: i32,
) -> Option<Tile> {
    let (centre, top) = secret_layout(ground, style);
    let dx = round_dx(ground, column, centre);
    if dx.abs() > 3 {
        return None;
    }
    // The marker on the surface right above.
    if dx == 0 && row < surface {
        let marker = match style {
            FrontierStyle::Desert => match surface - row {
                1..=3 => Some(FrontierTile::RuinBrick),
                4 => Some(FrontierTile::SunGlyph),
                _ => None,
            },
            FrontierStyle::Volcanic => (1..=3)
                .contains(&(surface - row))
                .then_some(FrontierTile::Obsidian),
            FrontierStyle::Crystal => match surface - row {
                1..=4 => Some(FrontierTile::SpireCrystal),
                5 => Some(FrontierTile::PrismCw),
                _ => None,
            },
        };
        return marker.map(tile);
    }
    if row < top || row > top + 4 {
        return None;
    }
    let (shell, prize) = match style {
        FrontierStyle::Desert => (FrontierTile::RuinBrick, FrontierTile::RuinCache),
        FrontierStyle::Volcanic => (FrontierTile::Obsidian, FrontierTile::MagmaHeart),
        FrontierStyle::Crystal => (FrontierTile::GeodeShell, FrontierTile::GeodeCore),
    };
    if dx.abs() == 3 || row == top || row == top + 4 {
        return Some(tile(shell));
    }
    if row == top + 3 && dx.abs() <= 1 {
        return Some(tile(if dx == 0 {
            prize
        } else {
            FrontierTile::SunGlyph
        }));
    }
    Some(AIR)
}

// ---------------------------------------------------------------------------------------------
// The dune world.
// ---------------------------------------------------------------------------------------------

fn desert_tile(ground: &PlanetGround, column: i64, row: i32, surface: i32) -> Option<Tile> {
    let index = cell(ground, column, DUNE_COLUMNS);
    let t = column.rem_euclid(DUNE_COLUMNS);
    // The Sleeping Sentinel.
    let landmark = landmark_centre(ground, FrontierStyle::Desert);
    let ldx = round_dx(ground, column, landmark);
    if ldx.abs() <= SENTINEL_HALF_WIDTH + 1
        && let Some(found) = sentinel_tile(ldx, row, surface)
    {
        return Some(found);
    }
    if row < surface {
        if is_oasis(ground, index) {
            // Water in the bowl, palms on the banks.
            if row >= 1 && (t - 32).abs() < OASIS_RADIUS {
                return Some(WATER);
            }
            return palm_tile(ground, index, t, row);
        }
        return None;
    }
    let stratum = ground.stratum_index(i32::try_from(column).unwrap_or(0), row);
    match stratum {
        // The Buried City's ruins.
        1 => ruin_tile(ground, column, row),
        // The Worm Galleries.
        2 => band_tunnel(ground, column, row).map(|edge| {
            if edge {
                tile(FrontierTile::Wormstone)
            } else {
                AIR
            }
        }),
        _ => None,
    }
}

/// The Sentinel's tile at `dx` from its centre column, `row` (the head over the sand, its
/// buried chin, the vault under it).
fn sentinel_tile(dx: i64, row: i32, surface: i32) -> Option<Tile> {
    let dy = i64::from(row - SENTINEL_CENTRE_ROW);
    let hw = SENTINEL_HALF_WIDTH;
    let hh = i64::from(SENTINEL_HALF_HEIGHT);
    let in_head = dx * dx * hh * hh + dy * dy * hw * hw <= hw * hw * hh * hh;
    if in_head && row <= surface + 2 {
        // Eyes (closed: a glowing lid line), the brow, the nose, the mouth.
        let eye_row = SENTINEL_CENTRE_ROW - 2;
        if row == eye_row && (2..=5).contains(&dx.abs()) {
            return Some(tile(FrontierTile::SunGlyph));
        }
        if row == SENTINEL_CENTRE_ROW + 3 && dx.abs() <= 3 {
            return Some(AIR);
        }
        // The crown's teeth.
        return Some(tile(FrontierTile::RuinBrick));
    }
    if row == SENTINEL_CENTRE_ROW - SENTINEL_HALF_HEIGHT - 1 && dx.abs() <= 8 && dx % 3 == 0 {
        return Some(tile(FrontierTile::RuinBrick));
    }
    // The vault under the head: brick walls, caches on its floor, a glyph at its back.
    let top = surface + SENTINEL_VAULT_DEPTH;
    if row >= top && row <= top + 5 && dx.abs() <= 7 {
        if dx.abs() == 7 || row == top || row == top + 5 {
            return Some(tile(FrontierTile::RuinBrick));
        }
        if row == top + 4 && matches!(dx, -4 | 0 | 4) {
            return Some(tile(FrontierTile::RuinCache));
        }
        if row == top + 2 && dx == 0 {
            return Some(tile(FrontierTile::SunGlyph));
        }
        return Some(AIR);
    }
    None
}

/// A palm at an oasis: the trunk on the bank, its fronds spread over the top.
fn palm_tile(ground: &PlanetGround, index: i64, t: i64, row: i32) -> Option<Tile> {
    for spot in [10_i64, 15, 49, 54] {
        let roll = hash3(seed(ground), P_PALM, index, spot);
        if roll.is_multiple_of(3) {
            continue;
        }
        let dx = t - spot;
        if dx.abs() > 2 {
            continue;
        }
        let base = -dune_height(ground, index * DUNE_COLUMNS + spot);
        let height = 4 + i32::try_from(roll % 3).unwrap_or(0);
        let crown = base - height - 1;
        if dx == 0 && row > crown && row < base {
            return Some(tile(FrontierTile::PalmTrunk));
        }
        if row == crown || (row == crown + 1 && dx.abs() == 2) {
            return Some(tile(FrontierTile::PalmFrond));
        }
    }
    None
}

/// A buried building of the Buried City: brick walls, a hall, a cracked floor and the vault
/// under it with its caches (`None` outside every building).
fn ruin_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    if ground.grid().span_at(row) > 1 {
        return None;
    }
    let (cx, cy) = (column.div_euclid(40), row.div_euclid(20));
    let roll = hash3(seed(ground), P_RUIN, cx, i64::from(cy));
    if !roll.is_multiple_of(3) {
        return None;
    }
    let x0 = cx * 40 + 4 + i64::try_from(roll % 8).unwrap_or(0);
    let w = 14 + i64::try_from((roll >> 8) % 12).unwrap_or(0);
    let y0 = cy * 20 + 3 + i32::try_from((roll >> 16) % 3).unwrap_or(0);
    let h = 10 + i32::try_from((roll >> 24) % 5).unwrap_or(0);
    let (x1, y1) = (x0 + w - 1, y0 + h - 1);
    if x1 >= width(ground) || column < x0 || column > x1 || row < y0 || row > y1 {
        return None;
    }
    // The building must lie in the Buried City (not poke into the dunes or the galleries).
    let x0_32 = i32::try_from(x0).unwrap_or(0);
    if ground.stratum_index(x0_32, y0) != 1 || ground.stratum_index(x0_32, y1) != 1 {
        return None;
    }
    let floor = y1 - 3;
    if column == x0 || column == x1 || row == y0 || row == y1 {
        return Some(tile(FrontierTile::RuinBrick));
    }
    if row == floor {
        return Some(tile(FrontierTile::RuinFloor));
    }
    if row == y1 - 1 {
        let slot = column - x0;
        if slot % 5 == 2 && !(roll >> 32).is_multiple_of(3) {
            return Some(tile(FrontierTile::RuinCache));
        }
    }
    // Pillars hold the hall's roof.
    if row < floor && (column - x0) % 6 == 0 {
        return Some(tile(FrontierTile::RuinBrick));
    }
    Some(AIR)
}

/// A horizontal tunnel of a 40-row band (worm galleries, lava tubes): `Some(false)` inside it
/// (open), `Some(true)` on its lining, `None` elsewhere.
fn band_tunnel(ground: &PlanetGround, column: i64, row: i32) -> Option<bool> {
    let band = row.div_euclid(BAND_ROWS);
    let purpose = P_MEANDER + u64::try_from(band.rem_euclid(16)).unwrap_or(0) * 7;
    let meander = (ground.periodic_noise(purpose, column, 128) - 512) * 10 / 512;
    let centre = band * BAND_ROWS + BAND_ROWS / 2 + i32::try_from(meander).unwrap_or(0);
    let gap = ground.periodic_noise(
        P_GAP + u64::try_from(band.rem_euclid(16)).unwrap_or(0) * 5,
        column,
        64,
    );
    if gap < 220 {
        return None;
    }
    match (row - centre).abs() {
        0 | 1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// Whether row `row` is in a lava tube or gallery band's tunnel at `column` and which way
/// its draught blows (`Some(1)` east, `Some(-1)` west).
#[must_use]
pub fn tube_way(ground: &PlanetGround, column: i64, row: i32) -> Option<i32> {
    if band_tunnel(ground, column, row) != Some(false) {
        return None;
    }
    let band = row.div_euclid(BAND_ROWS);
    Some(
        if hash3(seed(ground), P_TUBE_WAY, i64::from(band), 0).is_multiple_of(2) {
            1
        } else {
            -1
        },
    )
}

// ---------------------------------------------------------------------------------------------
// The volcanic world.
// ---------------------------------------------------------------------------------------------

fn volcanic_tile(
    ground: &PlanetGround,
    caves: &Caves,
    column: i64,
    row: i32,
    surface: i32,
) -> Option<Tile> {
    if row < surface {
        // The lava seas (their surface at the datum).
        if row >= 0 {
            return Some(LAVA);
        }
        // The crater lakes and vents.
        let index = cell(ground, column, CONE_COLUMNS);
        let cone = cone_at(ground, index);
        let dx = round_dx(ground, column, cone.centre);
        if dx.abs() < cone.crater {
            let lake = -cone.peak + 2;
            if cone.caldera && row >= lake {
                return Some(LAVA);
            }
            if !cone.caldera && row == surface - 1 && dx.abs() == cone.crater - 1 {
                return Some(tile(FrontierTile::SulphurCrust));
            }
        }
        return None;
    }
    // The Caldera's throat: a magma heart under its lake.
    let index = cell(ground, column, CONE_COLUMNS);
    let cone = cone_at(ground, index);
    if cone.caldera {
        let dx = round_dx(ground, column, cone.centre);
        let throat = -cone.peak + cone.crater_depth + CALDERA_THROAT_ROWS;
        if dx == 0 && row == throat {
            return Some(tile(FrontierTile::MagmaHeart));
        }
        if dx.abs() <= 1 && (row - throat).abs() <= 1 {
            return Some(tile(FrontierTile::MagmaRock));
        }
    }
    let depth = row - surface;
    let stratum = ground.stratum_index(i32::try_from(column).unwrap_or(0), row);
    let roll = hash3(seed(ground), P_GAS, column, i64::from(row));
    match stratum {
        0 if depth > 3 && roll.is_multiple_of(150) => Some(plain(TileKind::Gas)),
        1 => band_tunnel(ground, column, row).map(|edge| {
            if edge {
                tile(FrontierTile::TubeCrust)
            } else {
                AIR
            }
        }),
        3 => {
            if ground.in_cave_at(caves, column, row) {
                // Magma pools on the chambers' floors.
                if !ground.in_cave_at(caves, column, row + 1) {
                    return Some(LAVA);
                }
                return Some(AIR);
            }
            hash3(seed(ground), P_HEART, column, i64::from(row))
                .is_multiple_of(2_500)
                .then_some(tile(FrontierTile::MagmaHeart))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The crystal world.
// ---------------------------------------------------------------------------------------------

fn crystal_tile(ground: &PlanetGround, column: i64, row: i32, surface: i32) -> Option<Tile> {
    // The Choir Spire and its geode.
    let landmark = landmark_centre(ground, FrontierStyle::Crystal);
    let ldx = round_dx(ground, column, landmark);
    if ldx.abs() <= 8 {
        if row < surface {
            let up = surface - row;
            let plinth = up <= 5 && ldx.abs() <= 3;
            let prong = (ldx.abs() == 2) && up <= CHOIR_HEIGHT;
            let fork = up == CHOIR_HEIGHT - 1 && ldx.abs() <= 2;
            if plinth || prong {
                return Some(tile(if up == CHOIR_HEIGHT {
                    FrontierTile::SunLens
                } else {
                    FrontierTile::SpireCrystal
                }));
            }
            if fork {
                return Some(tile(FrontierTile::PrismCcw));
            }
            return Some(AIR);
        }
        let centre = surface + CHOIR_GEODE_DEPTH;
        if let Some(found) = geode_tile(ldx, row - centre, CHOIR_GEODE_RADIUS, true) {
            return Some(found);
        }
    }
    // A light well.
    if let Some(well) = well_near(ground, column)
        && let Some(found) = well_tile(&well, column, row, surface)
    {
        return Some(found);
    }
    if row < surface {
        return spire_tile(ground, column, row, surface);
    }
    let stratum = ground.stratum_index(i32::try_from(column).unwrap_or(0), row);
    if stratum == 2 {
        return geode_cell_tile(ground, column, row);
    }
    None
}

/// The tile of a geode `dy` rows below and `dx` columns across from its centre (`None`
/// outside it): a hard shell, a crystal lining, a hollow and, if `heart`, a geode heart on
/// its floor.
fn geode_tile(dx: i64, dy: i32, radius: i32, heart: bool) -> Option<Tile> {
    let r = i64::from(radius);
    let d2 = dx * dx + i64::from(dy) * i64::from(dy);
    if d2 > r * r {
        return None;
    }
    if d2 > (r - 1) * (r - 1) {
        return Some(tile(FrontierTile::GeodeShell));
    }
    if heart && dx == 0 && dy == radius - 2 {
        return Some(tile(FrontierTile::GeodeCore));
    }
    if d2 > (r - 2) * (r - 2) {
        return Some(tile(FrontierTile::Crystal));
    }
    Some(AIR)
}

/// The geodes of the Growing Geodes (one in some 24-column, 24-row cells).
fn geode_cell_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    let span = i64::from(ground.grid().span_at(row));
    let (cx, cy) = (column.div_euclid(24), row.div_euclid(24));
    let roll = hash3(seed(ground), P_GEODE, cx, i64::from(cy));
    if !roll.is_multiple_of(3) {
        return None;
    }
    let centre_x = cx * 24 + 12;
    let centre_y = cy * 24 + 12 + i32::try_from((roll >> 8) % 5).unwrap_or(0) - 2;
    let radius = 4 + i32::try_from((roll >> 16) % 4).unwrap_or(0);
    let dx = round_dx(ground, column, centre_x) / span;
    geode_tile(dx, row - centre_y, radius, (roll >> 24).is_multiple_of(4))
}

/// A crystal spire over the meadows (one in a third of the 24-column cells).
fn spire_tile(ground: &PlanetGround, column: i64, row: i32, surface: i32) -> Option<Tile> {
    let index = cell(ground, column, SPIRE_COLUMNS);
    let roll = hash3(seed(ground), P_SPIRE, index, 0);
    if !roll.is_multiple_of(3) {
        return None;
    }
    let base = index * SPIRE_COLUMNS + 6 + i64::try_from(roll % 12).unwrap_or(0);
    for centre in [
        i64::from(settlement_centre(ground, FrontierStyle::Crystal)),
        landmark_centre(ground, FrontierStyle::Crystal),
    ] {
        if round_dx(ground, base, centre).abs() < 40 {
            return None;
        }
    }
    let dx = column - base;
    if !(0..=1).contains(&dx) {
        return None;
    }
    let ground_row = -surface_height_of(ground, FrontierStyle::Crystal, base);
    let height = 5 + i32::try_from((roll >> 8) % 14).unwrap_or(0);
    let up = ground_row.max(surface) - row;
    if up < 1 || up > height {
        return None;
    }
    // Two wide for the lower part, one wide to the tip.
    if dx == 1 && up > height * 3 / 5 {
        return None;
    }
    Some(tile(FrontierTile::SpireCrystal))
}

/// A light well's tile: its shaft of sunlight, the lens, the stone the light must be dug a
/// path through, the prism, the receptor and the sealed vault.
fn well_tile(well: &LightWell, column: i64, row: i32, surface: i32) -> Option<Tile> {
    let at = TilePosition {
        x: i32::try_from(column).ok()?,
        y: row,
    };
    if at.x == well.column {
        if row >= surface && row < well.lens_row {
            return Some(AIR);
        }
        if row == well.lens_row {
            return Some(tile(FrontierTile::SunLens));
        }
        if row > well.lens_row && row <= well.lens_row + 2 {
            return Some(AIR);
        }
        if row > well.lens_row + 2 && row < well.prism.y {
            return Some(tile(FrontierTile::PrismaticStone));
        }
    }
    // The funnel at the top of the shaft.
    if (at.x - well.column).abs() == 1 && row >= surface && row < surface + 2 {
        return Some(AIR);
    }
    if at == well.prism {
        return Some(tile(if well.east {
            FrontierTile::PrismCcw
        } else {
            FrontierTile::PrismCw
        }));
    }
    if at == well.receptor {
        return Some(tile(FrontierTile::Receptor));
    }
    // The stone between the prism and the receptor.
    if row == well.prism.y {
        let (a, b) = if well.east {
            (well.prism.x, well.receptor.x)
        } else {
            (well.receptor.x, well.prism.x)
        };
        if at.x > a && at.x < b {
            return Some(tile(FrontierTile::PrismaticStone));
        }
    }
    if well.is_seal(at) {
        return Some(tile(FrontierTile::VaultSeal));
    }
    if well.inside(at) {
        let (x0, x1, _, y1) = well.vault;
        let mid = i32::midpoint(x0, x1);
        if row == y1 - 1 {
            if at.x == mid {
                return Some(tile(FrontierTile::GeodeCore));
            }
            if (at.x - mid).abs() == 2 {
                return Some(plain(TileKind::Ore(crate::terrain::MineralKind::Gold)));
            }
        }
        return Some(AIR);
    }
    None
}
