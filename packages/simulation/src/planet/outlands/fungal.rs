//! The fungal world (Wave 13 planets-c): puffball mounds under a forest of giant mushrooms, and
//! below them the Mycelium Mat, the Glowcap Grottos and the Sporeheart Deep.
//!
//! * **Surface**: the generic hills raised into rounded puffball mounds, a giant mushroom in
//!   most 28-column cells (a stalk two or three wide, 7-20 rows tall, a domed cap of springy
//!   flesh with soft gills under it; one in three glows, glowcaps hanging under its gills), and
//!   puffballs on the ground that burst into spore clouds when drilled. A cap throws a rig
//!   that lands on it back up (`world/outlands.rs`).
//! * **Mycelium Mat** (the top third): a felt of mycelium knotted with hyphae; signature rule
//!   **regrowth**: dug mycelium grows back into the tunnel once the rigs have moved on, so a
//!   way back up is a way to keep open (or to dig through hyphae, which never regrow).
//! * **Glowcap Grottos**: ribbed gillstone and lumen moss round extra grottos whose floors and
//!   ceilings sprout **glowcaps**; signature rule **glowflash**: a drilled glowcap flashes and
//!   stuns every hunter round it, so a grotto's lights are a weapon.
//! * **Sporeheart Deep**: dense spore-packed rock and **spore vents** under open chimneys;
//!   signature rule **spore vents**: each vent swells (a hiss and a glow), then blasts a column
//!   of spores up its chimney that throws a rig high and fouls its air.
//! * **Landmark: the Mother Cap**, half a lap from the settlement: a stalk 9 wide and 30 tall
//!   under a cap 48 wide of glowing flesh, a door at its foot into its hollow stalk, and the
//!   shaft down to the Sporemother's hollow ([`super::boss_lair`]).
//! * **Settlement site**: a level clearing with no mushrooms (the Tokkari like the fungal
//!   worlds).

use super::{
    OutlandTile, air, boss_lair, lair_tile, landmark_column, oroll, outland, outpost_column,
    outpost_tile, pick, secret_tile,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, PlanetThemeId,
        SurfaceSite, ThemeWorld,
        wilds::{
            Cell, chunk_with, core_floor_row, ease, is_small_body, secret_column,
            settlement_column, settlement_site_of, settlement_width, site_weight, wrap_delta,
        },
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind},
};

/// The fungal world's strata (registry data).
pub const FUNGAL_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Mycelium Mat",
        share_permille: 330,
        rock: TileKind::Outland(OutlandTile::MyceliumMat),
        accent: TileKind::Outland(OutlandTile::Hyphae),
        accent_permille: 200,
        ores: &[
            (MineralKind::Copper, 90),
            (MineralKind::Silver, 55),
            (MineralKind::Emerald, 16),
        ],
    },
    PlanetStratum {
        name: "Glowcap Grottos",
        share_permille: 330,
        rock: TileKind::Outland(OutlandTile::Gillstone),
        accent: TileKind::Outland(OutlandTile::Lumenmoss),
        accent_permille: 110,
        ores: &[
            (MineralKind::Gold, 45),
            (MineralKind::Ruby, 20),
            (MineralKind::Uranium, 12),
        ],
    },
    PlanetStratum {
        name: "Sporeheart Deep",
        share_permille: 340,
        rock: TileKind::Outland(OutlandTile::Sporeheart),
        accent: TileKind::Outland(OutlandTile::Hyphae),
        accent_permille: 90,
        ores: &[
            (MineralKind::Diamond, 22),
            (MineralKind::Platinum, 26),
            (MineralKind::Mythril, 9),
        ],
    },
];

/// The fungal theme (registry entry [`PlanetThemeId::FUNGAL`]).
pub const FUNGAL_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::FUNGAL,
    name: "Fungal World",
    palette: PlanetPalette {
        sky: [150, 120, 176],
        atmosphere: [206, 150, 230],
        surface: [120, 82, 138],
        rock: [112, 96, 120],
        deep: [70, 52, 84],
        core: [140, 255, 220],
    },
    crust: TileKind::Outland(OutlandTile::SporeLoam),
    crust_rows: 3,
    strata: &FUNGAL_STRATA,
    core_ore: TileKind::Ore(MineralKind::Emerald),
    caves_per_256_columns: 2,
    hill_permille: 22,
};

/// The fungal theme world.
pub struct FungalWorld;

/// The registered fungal world.
pub static FUNGAL_WORLD: FungalWorld = FungalWorld;

/// Columns per mushroom cell (one giant mushroom at most in each).
pub const MUSHROOM_CELL: i64 = 28;
/// Columns per puffball mound cell.
const MOUND_CELL: i64 = 48;
/// The Mother Cap: stalk half width, height, the cap's half width and height, its rise.
pub const MOTHER_STALK_HALF: i64 = 4;
pub const MOTHER_HEIGHT: i32 = 30;
pub const MOTHER_CAP_HALF: i64 = 24;
const MOTHER_CAP_HEIGHT: i32 = 10;
const MOTHER_RISE: i64 = 36;
/// Extra sky over the hills (the Mother Cap's dome).
const EXTRA_SKY: i32 = 72;

/// One giant mushroom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mushroom {
    /// The stalk's first column and width.
    pub column: i64,
    pub width: i64,
    /// The cap's underside row (the stalk's top).
    pub top: i32,
    /// The cap's half width and height.
    pub cap: (i64, i32),
    /// Whether it glows (glowcaps hang under its gills).
    pub glowing: bool,
}

/// Whether `column` is kept clear of growth: the settlement, the landmark's rise, the
/// outpost, and a small body's secret.
fn kept_clear(ground: &PlanetGround, column: i64, margin: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let near = |centre: i64, half: i64| wrap_delta(column, centre, width).abs() <= half + margin;
    let settlement_half = i64::from(settlement_width(ground) / 2);
    near(settlement_column(ground) + settlement_half, settlement_half)
        || near(landmark_column(ground), MOTHER_RISE)
        || near(outpost_column(ground), super::OUTPOST_HALF + 2)
        || (is_small_body(ground.params()) && near(secret_column(ground), 4))
}

/// The mushroom of cell `cell`, if it has one.
#[must_use]
pub fn mushroom_in_cell(ground: &PlanetGround, cell: i64) -> Option<Mushroom> {
    let width = i64::from(ground.strip_width());
    let cells = (width / MUSHROOM_CELL).max(1);
    let cell = cell.rem_euclid(cells);
    let hash = oroll(ground, 10, cell, 0);
    if hash % 10 >= 7 {
        return None;
    }
    let column = cell * MUSHROOM_CELL + 6 + pick(hash, 8, 14);
    let stalk = 2 + pick(hash, 16, 2);
    if kept_clear(ground, column, 12) || kept_clear(ground, column + stalk, 12) {
        return None;
    }
    let height = 7 + i32::try_from(pick(hash, 24, 14)).unwrap_or(0);
    let floor = surface(ground, column + stalk / 2);
    Some(Mushroom {
        column,
        width: stalk,
        top: floor - height,
        cap: (
            5 + pick(hash, 32, 6),
            3 + i32::try_from(pick(hash, 40, 2)).unwrap_or(0),
        ),
        glowing: pick(hash, 48, 3) == 0,
    })
}

/// A puffball mound's rise at `column` (rows).
fn mound(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let cells = (width / MOUND_CELL).max(1);
    let cell = column.div_euclid(MOUND_CELL);
    let mut rise = 0;
    for offset in -1..=1 {
        let index = (cell + offset).rem_euclid(cells);
        let hash = oroll(ground, 11, index, 0);
        if hash % 10 >= 6 {
            continue;
        }
        let centre = (cell + offset) * MOUND_CELL + pick(hash, 8, MOUND_CELL.cast_unsigned());
        let radius = 8 + pick(hash, 16, 11);
        let height = radius / 3;
        let dx = wrap_delta(column, centre, width);
        if dx.abs() < radius {
            let here = height * (radius * radius - dx * dx) / (radius * radius);
            rise = rise.max(i32::try_from(here).unwrap_or(0));
        }
    }
    rise
}

/// The fungal surface row at `column`: the hills and puffball mounds, levelled at the
/// clearing, the Mother Cap's rise and the outpost.
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let natural = -ground.hill(column) - mound(ground, column);
    let half = i64::from(settlement_width(ground) / 2);
    let settlement = settlement_column(ground) + half;
    if let Some(weight) = site_weight(column, settlement, half, width) {
        return ease(natural, -ground.hill(settlement), weight);
    }
    let landmark = landmark_column(ground);
    if let Some(weight) = site_weight(column, landmark, MOTHER_RISE, width) {
        return ease(natural, -ground.hill(landmark) - 2, weight);
    }
    let outpost = outpost_column(ground);
    if let Some(weight) = site_weight(column, outpost, super::OUTPOST_HALF, width) {
        return ease(natural, -ground.hill(outpost), weight);
    }
    natural
}

/// A mushroom's tile at `(column, row)`, if any (`floor` is the surface there).
fn mushroom_tile(
    mushroom: &Mushroom,
    column: i64,
    row: i32,
    floor: i32,
    width: i64,
) -> Option<Tile> {
    let centre = mushroom.column + mushroom.width / 2;
    let dx = wrap_delta(column, centre, width);
    let in_stalk = (0..mushroom.width).contains(&wrap_delta(column, mushroom.column, width));
    let (half_width, half_height) = mushroom.cap;
    let dy = i64::from(row - mushroom.top);
    // The cap: a dome over the stalk's top.
    if dy <= 0
        && dx * dx * i64::from(half_height * half_height) + dy * dy * half_width * half_width
            <= half_width * half_width * i64::from(half_height * half_height)
    {
        return Some(outland(OutlandTile::CapFlesh));
    }
    // The gills under it, and a glowing mushroom's glowcaps hanging from them.
    if dy == 1 && dx.abs() < half_width - 1 && !in_stalk {
        return Some(outland(OutlandTile::Gills));
    }
    if mushroom.glowing && dy == 2 && dx.abs() < half_width - 2 && !in_stalk && dx % 3 == 0 {
        return Some(outland(OutlandTile::Glowcap));
    }
    if in_stalk && row > mushroom.top && row < floor {
        return Some(outland(OutlandTile::Stalk));
    }
    None
}

/// The Mother Cap's tile at `(column, row)`, if any (its shaft and lair are the lair's).
fn mother_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let centre = landmark_column(ground);
    let dx = wrap_delta(column, centre, width);
    if dx.abs() > MOTHER_CAP_HALF + 2 {
        return None;
    }
    let floor = super::landmark_floor(ground);
    let top = floor - MOTHER_HEIGHT;
    // The cap: a great dome of springy flesh, glowing spots across its skin.
    let dy = i64::from(row - top);
    let (hw, hh) = (MOTHER_CAP_HALF, i64::from(MOTHER_CAP_HEIGHT));
    if dy <= 0 && dx * dx * hh * hh + dy * dy * hw * hw <= hw * hw * hh * hh {
        let skin = dx * dx * (hh - 2) * (hh - 2) + dy * dy * (hw - 3) * (hw - 3)
            > (hw - 3) * (hw - 3) * (hh - 2) * (hh - 2);
        let spot = (column.rem_euclid(7) == 0 && row.rem_euclid(3) == 0) || skin && dx % 5 == 0;
        return Some(outland(if spot {
            OutlandTile::MotherCap
        } else {
            OutlandTile::CapFlesh
        }));
    }
    if dy == 1 && dx.abs() < MOTHER_CAP_HALF - 2 && dx.abs() > MOTHER_STALK_HALF {
        return Some(outland(OutlandTile::Gills));
    }
    if dy == 2 && dx.abs() < MOTHER_CAP_HALF - 4 && dx.abs() > MOTHER_STALK_HALF && dx % 4 == 0 {
        return Some(outland(OutlandTile::Glowcap));
    }
    // The stalk: hollow, a door at its foot on the west side.
    if dx.abs() <= MOTHER_STALK_HALF && row > top && row < floor {
        let hollow = dx.abs() <= super::LAIR_SHAFT_HALF && row >= floor - 20;
        let door = dx < 0 && (floor - 4..floor).contains(&row);
        if hollow || door {
            return Some(air());
        }
        return Some(outland(OutlandTile::Stalk));
    }
    None
}

/// Whether strip tile `(first, row)` is in one of the Glowcap Grottos' own grottos.
#[must_use]
pub fn in_grotto(ground: &PlanetGround, first: i64, row: i32, span: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let cells = (width / 56).max(1);
    let cell_x = first.div_euclid(56);
    let cell_y = row.div_euclid(22);
    for ox in -1..=1 {
        for oy in -1..=1 {
            let (cx, cy) = (cell_x + ox, cell_y + oy);
            let hash = oroll(ground, 20, cx.rem_euclid(cells), i64::from(cy));
            if hash % 100 >= 55 {
                continue;
            }
            let centre_x = cx * 56 + pick(hash, 8, 56);
            let centre_y = cy * 22 + i32::try_from(pick(hash, 16, 22)).unwrap_or(0);
            let half_width = 6 + pick(hash, 24, 8);
            let half_height = 3 + pick(hash, 32, 3);
            let dx = wrap_delta(first, centre_x, width) / span.max(1);
            let dy = i64::from(row - centre_y);
            if dx * dx * half_height * half_height + dy * dy * half_width * half_width
                <= half_width * half_width * half_height * half_height
            {
                return true;
            }
        }
    }
    false
}

/// A spore vent in the Sporeheart Deep: its tile and the chimney over it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SporeVent {
    pub column: i64,
    /// The vent's row; the chimney is open from `row - chimney` to `row - 1`.
    pub row: i32,
    pub chimney: i32,
}

/// Columns and rows per spore vent cell.
pub const VENT_CELL: (i64, i32) = (40, 30);

/// The vent of the cell holding strip tile `(column, row)`, if it has one.
#[must_use]
pub fn vent_in_cell(ground: &PlanetGround, column: i64, row: i32) -> Option<SporeVent> {
    let width = i64::from(ground.strip_width());
    let cells = (width / VENT_CELL.0).max(1);
    let cx = column.div_euclid(VENT_CELL.0);
    let cy = row.div_euclid(VENT_CELL.1);
    let hash = oroll(ground, 30, cx.rem_euclid(cells), i64::from(cy));
    if hash % 10 >= 5 {
        return None;
    }
    let vent = SporeVent {
        column: (cx * VENT_CELL.0 + 6 + pick(hash, 8, 28)).rem_euclid(width),
        row: cy * VENT_CELL.1 + 18 + i32::try_from(pick(hash, 16, 10)).unwrap_or(0),
        chimney: 10 + i32::try_from(pick(hash, 24, 6)).unwrap_or(0),
    };
    if kept_clear(ground, vent.column, 6) {
        return None;
    }
    Some(vent)
}

impl ThemeWorld for FungalWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::FUNGAL
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

    #[allow(clippy::too_many_lines, reason = "one pass over the world's features")]
    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile> {
        let width = i64::from(ground.strip_width());
        let origin = chunk.origin();
        let first_cell = i64::from(origin.x).div_euclid(MUSHROOM_CELL) - 1;
        let mushrooms: Vec<Mushroom> = (first_cell..=first_cell + 3)
            .filter_map(|cell| mushroom_in_cell(ground, cell))
            .collect();
        let near_landmark = wrap_delta(i64::from(origin.x) + 16, landmark_column(ground), width)
            .abs()
            <= MOTHER_CAP_HALF + super::LAIR_HALF_WIDTH + 30;
        let lair = if near_landmark {
            boss_lair(ground)
        } else {
            None
        };
        let near_outpost =
            wrap_delta(i64::from(origin.x) + 16, outpost_column(ground), width).abs() <= 40;
        let outpost_floor = surface(ground, outpost_column(ground));
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_landmark {
                    if let Some(tile) = mother_tile(ground, cell.first, cell.row) {
                        return tile;
                    }
                    if let Some(lair) = &lair
                        && let Some(tile) =
                            lair_tile(ground, lair, cell, outland(OutlandTile::Stalk))
                    {
                        return tile;
                    }
                }
                if near_outpost && let Some(tile) = outpost_tile(ground, cell, outpost_floor) {
                    return tile;
                }
                if let Some(tile) = secret_tile(
                    ground,
                    cell,
                    outland(OutlandTile::Sporeheart),
                    outland(OutlandTile::Stalk),
                ) {
                    return tile;
                }
                if cell.span == 1 {
                    for mushroom in &mushrooms {
                        if let Some(tile) =
                            mushroom_tile(mushroom, cell.first, cell.row, cell.surface, width)
                        {
                            return tile;
                        }
                    }
                }
                if cell.row < cell.surface {
                    // Puffballs on the open ground.
                    if cell.row == cell.surface - 1
                        && cell.span == 1
                        && !kept_clear(ground, cell.first, 2)
                        && oroll(ground, 12, cell.first, 0).is_multiple_of(11)
                    {
                        return outland(OutlandTile::Puffball);
                    }
                    return cell.base;
                }
                match cell.stratum {
                    1 if cell.depth > 6 => {
                        if in_grotto(ground, cell.first, cell.row, cell.span) {
                            let below = in_grotto(ground, cell.first, cell.row + 1, cell.span);
                            let above = in_grotto(ground, cell.first, cell.row - 1, cell.span);
                            let hash = oroll(ground, 21, cell.first, i64::from(cell.row));
                            if (!below || !above) && hash.is_multiple_of(3) {
                                return outland(OutlandTile::Glowcap);
                            }
                            return air();
                        }
                        cell.base
                    }
                    2 => {
                        if let Some(vent) = vent_in_cell(ground, cell.first, cell.row) {
                            let dx = wrap_delta(cell.first, vent.column, width) / cell.span;
                            if dx == 0 && cell.row == vent.row {
                                return outland(OutlandTile::SporeVent);
                            }
                            if dx.abs() <= 1
                                && (vent.row - vent.chimney..vent.row).contains(&cell.row)
                            {
                                return air();
                            }
                        }
                        // Gold truffles, few and far between.
                        if !matches!(cell.base.kind, TileKind::Air)
                            && oroll(ground, 31, cell.first, i64::from(cell.row))
                                .is_multiple_of(900)
                        {
                            return outland(OutlandTile::Truffle);
                        }
                        cell.base
                    }
                    _ => cell.base,
                }
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        Some(settlement_site_of(ground, |column| surface(ground, column)))
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        let centre = landmark_column(ground);
        Some(SurfaceSite {
            first: i32::try_from(centre - MOTHER_STALK_HALF).unwrap_or(0),
            width: i32::try_from(MOTHER_STALK_HALF * 2 + 1).unwrap_or(9),
            floor: surface(ground, centre),
        })
    }
}
