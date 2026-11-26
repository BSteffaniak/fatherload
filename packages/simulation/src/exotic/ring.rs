//! The ring world (Wave 13 planets-d; the space generator's ring world tag): a planet under
//! great rings, its plains scarred by what falls from them.
//!
//! The polar engine plays a body as one strip, so the ring's walkable part is **the
//! Skyring** ([`SKYRING_TOP`]..[`SKYRING_BOTTOM`]): a band of floating debris islands (ring ice
//! and ringstone, studded with meteorite and cloud silver) circling the world in its low sky,
//! where the pull is a third of the ground's and the debris drifts east
//! ([`super::ExoticRule::Microgravity`]): you hop across it, landing by hand. From space the
//! space generator's great ring circles the body (the renderer's disc and ring).
//!
//! * **Surface**: ringfall plains of ring dust, impact craters with glassy rims, and fallen
//!   ring monoliths leaning out of the ground.
//! * **Strata** (concentric rings down to the core chamber): Ringfall Plains (**meteor
//!   showers**: debris falls under growing shadows and leaves meteorite), Impact Breccia
//!   (**venting ice**: buried ring-ice lenses blast vapour when drilled), Tether Roots (**live
//!   conduits**: the builders' cables shock on touch; cut a junction to kill a run) and the
//!   Dense Core (**dense core**: half again the pull).
//! * **Landmark: the Tether**, half a lap from the settlement: a builder column of
//!   indestructible plate from a level plaza up to an anchor platform in the Skyring (a relic
//!   on it), the Ring Warden's seal at its foot. **The Tether Station** (a builder outpost)
//!   stands off the plaza.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use super::{
    ExoticTile, exotic, lap_column, outpost_room_tile, roll, secret_tile, settlement_column,
    settlement_half, tile, value_noise2, wrap_delta,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, ThemeWorld,
        wilds::{Cell, chunk_with, core_floor_row, ease, site_weight},
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind, TilePosition},
};

/// The ring world's strata (registry data).
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Ringfall Plains",
        share_permille: 230,
        rock: TileKind::Exotic(ExoticTile::RingDust),
        accent: TileKind::Exotic(ExoticTile::ImpactGlass),
        accent_permille: 60,
        ores: &[
            (MineralKind::Iron, 80),
            (MineralKind::Copper, 60),
            (MineralKind::Silver, 30),
        ],
    },
    PlanetStratum {
        name: "Impact Breccia",
        share_permille: 260,
        rock: TileKind::Exotic(ExoticTile::Breccia),
        accent: TileKind::Exotic(ExoticTile::ImpactGlass),
        accent_permille: 120,
        ores: &[
            (MineralKind::Gold, 40),
            (MineralKind::Emerald, 20),
            (MineralKind::Ruby, 15),
        ],
    },
    PlanetStratum {
        name: "Tether Roots",
        share_permille: 270,
        rock: TileKind::Exotic(ExoticTile::Ringstone),
        accent: TileKind::Exotic(ExoticTile::Breccia),
        accent_permille: 120,
        ores: &[
            (MineralKind::Platinum, 30),
            (MineralKind::CloudSilver, 20),
            (MineralKind::Uranium, 12),
        ],
    },
    PlanetStratum {
        name: "Dense Core",
        share_permille: 240,
        rock: TileKind::Exotic(ExoticTile::DenseRock),
        accent: TileKind::Exotic(ExoticTile::Ringstone),
        accent_permille: 100,
        ores: &[
            (MineralKind::Diamond, 22),
            (MineralKind::Platinum, 30),
            (MineralKind::Mythril, 12),
        ],
    },
];

/// The ring world theme (registry entry [`super::RING_WORLD`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::RING_WORLD,
    name: "Ring World",
    palette: PlanetPalette {
        sky: [120, 140, 196],
        atmosphere: [176, 196, 255],
        surface: [176, 160, 150],
        rock: [124, 116, 124],
        deep: [60, 56, 76],
        core: [220, 200, 255],
    },
    crust: TileKind::Exotic(ExoticTile::RingDust),
    crust_rows: 3,
    strata: &STRATA,
    core_ore: TileKind::Ore(MineralKind::Diamond),
    caves_per_256_columns: 2,
    hill_permille: 10,
};

/// The ring world's generator.
pub struct RingWorldGen;

/// The registered ring world.
pub static RING_WORLD_GEN: RingWorldGen = RingWorldGen;

/// The Skyring's top and bottom rows (above the datum).
pub const SKYRING_TOP: i32 = -132;
pub const SKYRING_BOTTOM: i32 = -64;
/// The Tether's anchor platform (its top row) and half width.
pub const ANCHOR_ROW: i32 = -98;
pub const ANCHOR_HALF: i64 = 16;
/// The Tether's half width.
pub const TETHER_HALF: i64 = 2;
/// The plaza round the Tether's foot (the Ring Warden's arena; half width).
pub const PLAZA_HALF: i64 = 40;
/// The seal's offset from the Tether.
pub const SEAL_OFFSET: i64 = 9;
/// The Tether Station's offset from the Tether and its level ground.
const STATION_OFFSET: i64 = -64;
const STATION_HALF: i64 = 12;
/// Extra sky over the hills (the Skyring).
const EXTRA_SKY: i32 = 124;

/// The Tether's column (half a lap from the settlement).
#[must_use]
pub fn tether_column(ground: &PlanetGround) -> i64 {
    lap_column(ground, settlement_column(ground), 500)
}

/// The Tether Station's column.
#[must_use]
pub fn station_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (tether_column(ground) + STATION_OFFSET).rem_euclid(width)
}

/// A crater of the 128-column cell `cell`: `(centre, radius)`.
fn crater_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i64)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 30, cell.rem_euclid(width / 128), 0);
    if hash % 100 >= 45 {
        return None;
    }
    let centre = cell * 128 + 40 + i64::try_from((hash >> 8) % 48).unwrap_or(0);
    let radius = 10 + i64::try_from((hash >> 16) % 16).unwrap_or(0);
    if near_site(ground, centre, radius + 6) {
        return None;
    }
    Some((centre, radius))
}

/// A monolith of the 72-column cell `cell`: `(column, height, lean)`.
fn monolith_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i32, i64)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 31, cell.rem_euclid(width / 72), 0);
    if hash % 100 >= 30 {
        return None;
    }
    let column = cell * 72 + 12 + i64::try_from((hash >> 8) % 48).unwrap_or(0);
    if near_site(ground, column, 8) {
        return None;
    }
    let lean = if (hash >> 20).is_multiple_of(2) {
        1
    } else {
        -1
    };
    Some((
        column,
        8 + i32::try_from((hash >> 16) % 9).unwrap_or(0),
        lean,
    ))
}

/// Whether `column` lies within `margin` of a levelled site.
fn near_site(ground: &PlanetGround, column: i64, margin: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let settlement = settlement_column(ground);
    wrap_delta(column, settlement, width).abs()
        <= i64::from(settlement_half(ground, 24)) + 12 + margin
        || wrap_delta(column, tether_column(ground), width).abs() <= PLAZA_HALF + 12 + margin
        || wrap_delta(column, station_column(ground), width).abs() <= STATION_HALF + 12 + margin
}

/// The natural surface (craters dug in) at `column`.
fn natural(ground: &PlanetGround, column: i64) -> i32 {
    let mut height = ground.rolling_hills(column) * 2 / 3;
    let cell = column.div_euclid(128);
    for c in cell - 1..=cell + 1 {
        if let Some((centre, radius)) = crater_in(ground, c) {
            let dx = (column - centre).abs();
            let depth = radius / 3;
            if dx < radius {
                height -= i32::try_from(depth * (radius * radius - dx * dx) / (radius * radius))
                    .unwrap_or(0);
                height += 2;
            } else if dx < radius + 5 {
                height += i32::try_from((radius + 5 - dx) * 3 / 5).unwrap_or(0);
            }
        }
    }
    -height
}

/// The surface row at `column` (the settlement, the plaza and the station levelled).
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let sites = [
        (
            settlement_column(ground),
            i64::from(settlement_half(ground, 24)),
        ),
        (tether_column(ground), PLAZA_HALF),
        (station_column(ground), STATION_HALF),
    ];
    for (centre, half) in sites {
        if let Some(weight) = site_weight(column, centre, half, width) {
            return ease(natural(ground, column), natural(ground, centre), weight);
        }
    }
    natural(ground, column)
}

/// Whether `column` is on a crater's rim or floor (impact glass on top).
fn crater_glass(ground: &PlanetGround, column: i64) -> bool {
    let cell = column.div_euclid(128);
    (cell - 1..=cell + 1).any(|c| {
        crater_in(ground, c).is_some_and(|(centre, radius)| {
            let dx = (column - centre).abs();
            (radius - 2..radius + 3).contains(&dx) || dx < radius / 3
        })
    })
}

/// A Skyring island of the 48 x 20 cell `(cell_x, cell_y)`: `(centre column, centre row, half
/// width, half height, ice)`.
#[must_use]
pub fn island_in(
    ground: &PlanetGround,
    cell_x: i64,
    cell_y: i32,
) -> Option<(i64, i32, i64, i64, bool)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 32, cell_x.rem_euclid(width / 48), i64::from(cell_y));
    if hash % 100 >= 55 {
        return None;
    }
    let x = cell_x * 48 + 10 + i64::try_from((hash >> 8) % 28).unwrap_or(0);
    let y = cell_y * 20 + 6 + i32::try_from((hash >> 16) % 8).unwrap_or(0);
    if !(SKYRING_TOP + 6..=SKYRING_BOTTOM - 6).contains(&y) {
        return None;
    }
    // The Tether's column keeps clear (its anchor platform is the ring's one dock).
    if wrap_delta(x, tether_column(ground), width).abs() <= ANCHOR_HALF + 20 {
        return None;
    }
    let half_width = 5 + i64::try_from((hash >> 24) % 10).unwrap_or(0);
    let half_height = 2 + i64::try_from((hash >> 32) % 3).unwrap_or(0);
    Some((
        x,
        y,
        half_width,
        half_height,
        (hash >> 40).is_multiple_of(2),
    ))
}

/// The Skyring's tile at `(first, row)`, if an island holds it.
fn skyring_tile(ground: &PlanetGround, first: i64, row: i32) -> Option<Tile> {
    if !(SKYRING_TOP..=SKYRING_BOTTOM).contains(&row) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let cell_x = first.div_euclid(48);
    let cell_y = row.div_euclid(20);
    for cx in cell_x - 1..=cell_x + 1 {
        for cy in cell_y - 1..=cell_y + 1 {
            let Some((x, y, hw, hh, ice)) = island_in(ground, cx, cy) else {
                continue;
            };
            let dx = wrap_delta(first, x, width);
            let dy = i64::from(row - y);
            // Lumpy: the edge wobbles by the noise.
            let wobble = value_noise2(ground, 33, first, row, 8, 2) * 3 / 1_024;
            let (hw, hh) = (hw + wobble - 1, hh + i64::from(dy < 0));
            if dx * dx * hh * hh + dy * dy * hw * hw > hw * hw * hh * hh {
                continue;
            }
            let hash = roll(ground, 34, first, i64::from(row));
            if hash % 1_000 < 40 {
                return Some(exotic(ExoticTile::Meteorite));
            }
            if hash % 1_000 < 70 {
                return Some(tile(TileKind::Ore(MineralKind::CloudSilver)));
            }
            return Some(exotic(if ice {
                ExoticTile::RingIce
            } else {
                ExoticTile::Ringstone
            }));
        }
    }
    None
}

/// The Tether's tile at `dx` from it, row `row` (`floor` the plaza's surface).
fn tether_tile(dx: i64, row: i32, floor: i32) -> Option<Tile> {
    // The anchor platform in the Skyring, a relic on it.
    if dx.abs() <= ANCHOR_HALF && (ANCHOR_ROW..ANCHOR_ROW + 3).contains(&row) {
        return Some(exotic(ExoticTile::TetherPlate));
    }
    if dx == 0 && row == ANCHOR_ROW - 1 {
        return Some(exotic(ExoticTile::LandmarkRelic));
    }
    // The column from the plaza up to the platform (and its foot a dozen rows down).
    if dx.abs() <= TETHER_HALF && row > ANCHOR_ROW && row < floor + 12 {
        return Some(exotic(ExoticTile::TetherPlate));
    }
    // Buttresses at its foot.
    if dx.abs() <= TETHER_HALF + 3
        && row < floor
        && row >= floor - (TETHER_HALF + 4 - dx.abs()) as i32
    {
        return Some(exotic(ExoticTile::TetherPlate));
    }
    // The Ring Warden's seal on the plaza.
    if dx == SEAL_OFFSET && row == floor - 1 {
        return Some(exotic(ExoticTile::BossSeal));
    }
    None
}

/// A conduit run of the 96 x 40 cell `(cell_x, cell_y)`: `(first column, length, row)`.
fn conduit_in(ground: &PlanetGround, cell_x: i64, cell_y: i32) -> Option<(i64, i64, i32)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 35, cell_x.rem_euclid(width / 96), i64::from(cell_y));
    if hash % 100 >= 55 {
        return None;
    }
    let first = cell_x * 96 + i64::try_from((hash >> 8) % 30).unwrap_or(0);
    let length = 30 + i64::try_from((hash >> 16) % 40).unwrap_or(0);
    let row = cell_y * 40 + 8 + i32::try_from((hash >> 24) % 24).unwrap_or(0);
    Some((first, length, row))
}

/// Columns between a conduit's junctions.
pub const JUNCTION_EVERY: i64 = 16;

fn conduit_tile(ground: &PlanetGround, first: i64, row: i32, span: i64) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let cell_x = first.div_euclid(96);
    let cell_y = row.div_euclid(40);
    for cx in cell_x - 1..=cell_x + 1 {
        let Some((start, length, at)) = conduit_in(ground, cx, cell_y) else {
            continue;
        };
        if row != at {
            continue;
        }
        let along = wrap_delta(first, start, width) / span.max(1);
        if (0..length).contains(&along) {
            return Some(exotic(if along % JUNCTION_EVERY == JUNCTION_EVERY / 2 {
                ExoticTile::ConduitJunction
            } else {
                ExoticTile::ConduitLive
            }));
        }
    }
    None
}

/// A ring-ice lens of the 56 x 22 cell (the Impact Breccia).
fn in_lens(ground: &PlanetGround, first: i64, row: i32, span: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let (cell_x, cell_y) = (first.div_euclid(56), row.div_euclid(22));
    let hash = roll(ground, 36, cell_x.rem_euclid(width / 56), i64::from(cell_y));
    if hash % 100 >= 50 {
        return false;
    }
    let x = cell_x * 56 + 14 + i64::try_from((hash >> 8) % 28).unwrap_or(0);
    let y = cell_y * 22 + 6 + i32::try_from((hash >> 16) % 10).unwrap_or(0);
    let hw = 6 + i64::try_from((hash >> 24) % 7).unwrap_or(0);
    let hh = 2 + i64::try_from((hash >> 32) % 2).unwrap_or(0);
    let dx = wrap_delta(first, x, width) / span.max(1);
    let dy = i64::from(row - y);
    dx * dx * hh * hh + dy * dy * hw * hw <= hw * hw * hh * hh
}

/// The builder outpost of a ring world (see [`super::builder_outpost`]).
#[must_use]
pub fn outpost(ground: &PlanetGround) -> super::BuilderOutpost {
    let column = station_column(ground);
    let floor = surface(ground, column);
    let column = i32::try_from(column).unwrap_or(0);
    super::BuilderOutpost {
        style: super::ExoticStyle::Ring,
        column,
        floor,
        beacon: TilePosition {
            x: column,
            y: floor - 1,
        },
    }
}

impl ThemeWorld for RingWorldGen {
    fn theme(&self) -> crate::planet::PlanetThemeId {
        super::RING_WORLD
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

    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the ring world's features"
    )]
    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile> {
        let width = i64::from(ground.strip_width());
        let origin = chunk.origin();
        let middle = i64::from(origin.x) + 8;
        let tether = tether_column(ground);
        let near_tether = wrap_delta(middle, tether, width).abs() <= PLAZA_HALF + 16;
        let tether_floor = surface(ground, tether);
        let station = station_column(ground);
        let near_station = wrap_delta(middle, station, width).abs() <= 32;
        let station_floor = surface(ground, station);
        let monolith_cell = i64::from(origin.x).div_euclid(72) - 1;
        let monoliths: Vec<(i64, i32, i64)> = (monolith_cell..=monolith_cell + 2)
            .filter_map(|cell| monolith_in(ground, cell))
            .collect();
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_tether
                    && cell.span == 1
                    && let Some(tile) = tether_tile(
                        wrap_delta(cell.first, tether, width),
                        cell.row,
                        tether_floor,
                    )
                {
                    return tile;
                }
                if near_station
                    && let Some(tile) = outpost_room_tile(
                        wrap_delta(cell.first, station, width),
                        cell.row,
                        station_floor,
                    )
                {
                    return tile;
                }
                if let Some(tile) = secret_tile(
                    ground,
                    cell.first,
                    cell.row,
                    cell.surface,
                    exotic(ExoticTile::Ringstone),
                    exotic(ExoticTile::Monolith),
                ) {
                    return tile;
                }
                if cell.row < cell.surface {
                    if cell.span == 1 {
                        if let Some(tile) = skyring_tile(ground, cell.first, cell.row) {
                            return tile;
                        }
                        for &(column, height, lean) in &monoliths {
                            let up = cell.surface - cell.row;
                            if up <= 0 || up > height {
                                continue;
                            }
                            let shift = lean * i64::from(up) / 4;
                            let dx = wrap_delta(cell.first, column + shift, width);
                            if (0..=2).contains(&dx) {
                                return exotic(ExoticTile::Monolith);
                            }
                        }
                    }
                    return cell.base;
                }
                if cell.base.kind == TileKind::Air {
                    return cell.base;
                }
                if cell.depth == 0 && crater_glass(ground, cell.first) {
                    return exotic(ExoticTile::ImpactGlass);
                }
                match cell.stratum {
                    1 if in_lens(ground, cell.first, cell.row, cell.span) => {
                        return exotic(ExoticTile::RingIce);
                    }
                    2 => {
                        if let Some(tile) = conduit_tile(ground, cell.first, cell.row, cell.span) {
                            return tile;
                        }
                    }
                    0 if roll(ground, 37, cell.first, i64::from(cell.row)) % 10_000 < 40 => {
                        return exotic(ExoticTile::Meteorite);
                    }
                    _ => {}
                }
                cell.base
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let centre = settlement_column(ground);
        let half = i64::from(settlement_half(ground, 24));
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(centre - half).unwrap_or(0),
            width: i32::try_from(half * 2).unwrap_or(48),
            floor: surface(ground, centre),
        })
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let centre = tether_column(ground);
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(centre - PLAZA_HALF).unwrap_or(0),
            width: i32::try_from(PLAZA_HALF * 2).unwrap_or(80),
            floor: surface(ground, centre),
        })
    }
}
