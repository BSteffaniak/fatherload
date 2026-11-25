//! The glacier world (Wave 12 planets-a; the space generator's ice tag): ice sheets and frozen
//! seas, and below them Glacier Ice, the Under-Ice Sea and the Permafrost Caves.
//!
//! * **Surface**: one elevation field round the body ([`elevation`]). Low ground is a **frozen
//!   sea**: a lid of sea ice at row 0 over cold water and a permafrost seabed. High ground is
//!   the **glacier**: ice sheets in terraces five rows high under a crust of snow, split by
//!   **crevasses** (narrow and deep, a third of them hidden under a snow bridge of rime that
//!   breaks when a rig rests on it) and broken by **ice spires**.
//! * **Glacier Ice**: glacier and blue ice; signature rule **slick ice and refreeze**: rigs
//!   slide on ice (Descent IV's ice physics), and at night a tunnel through the glacier
//!   refreezes as thin rime, which the day thaws again (`world/wilds.rs`).
//! * **Under-Ice Sea**: sea ice round great lenses of water; signature rule **cold water**:
//!   water here bites three times as cold (the cold hazard; the Cryo Hull counters it).
//! * **Permafrost Caves**: hard frozen ground with frost crystals and ice caves lined with
//!   blue ice; signature rule **icicles**: an icicle over a passing rig cracks and falls.
//! * **Landmark: the Frozen Titan**, half a lap from the settlement: a beast the size of a
//!   hill frozen in clear ice under the glacier, its tusk breaking the surface, its heart
//!   in its ribs.
//! * **Settlement site**: level glacier, never over a crevasse.

use super::{
    Cell, SurfaceSite, WildTile, air, chunk_with, core_floor_row, ease, is_small_body,
    landmark_column, roll, secret_tile, settlement_column, site_weight, tile, water, wild,
    wrap_delta,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, PlanetThemeId,
        ThemeWorld,
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, SurfaceMaterial, Tile, TileKind},
};

/// The glacier's strata (registry data).
pub const GLACIER_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Glacier Ice",
        share_permille: 300,
        rock: TileKind::Wild(WildTile::GlacierIce),
        accent: TileKind::Wild(WildTile::BlueIce),
        accent_permille: 200,
        ores: &[
            (MineralKind::Silver, 50),
            (MineralKind::Emerald, 14),
            (MineralKind::Platinum, 10),
        ],
    },
    PlanetStratum {
        name: "Under-Ice Sea",
        share_permille: 300,
        rock: TileKind::Wild(WildTile::SeaIce),
        accent: TileKind::Wild(WildTile::BlueIce),
        accent_permille: 120,
        ores: &[
            (MineralKind::Silver, 30),
            (MineralKind::Gold, 30),
            (MineralKind::Ruby, 12),
        ],
    },
    PlanetStratum {
        name: "Permafrost Caves",
        share_permille: 400,
        rock: TileKind::Wild(WildTile::Permafrost),
        accent: TileKind::Wild(WildTile::BlueIce),
        accent_permille: 100,
        ores: &[
            (MineralKind::Diamond, 20),
            (MineralKind::Platinum, 26),
            (MineralKind::Mythril, 8),
        ],
    },
];

/// The glacier theme (registry entry [`PlanetThemeId::GLACIER`]).
pub const GLACIER_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::GLACIER,
    name: "Glacier World",
    palette: PlanetPalette {
        sky: [150, 180, 214],
        atmosphere: [200, 236, 255],
        surface: [240, 246, 252],
        rock: [140, 186, 214],
        deep: [70, 90, 120],
        core: [170, 240, 255],
    },
    crust: TileKind::Surface(SurfaceMaterial::Snow),
    crust_rows: 3,
    strata: &GLACIER_STRATA,
    core_ore: TileKind::Ore(MineralKind::Diamond),
    caves_per_256_columns: 1,
    hill_permille: 18,
};

/// The glacier's theme world.
pub struct GlacierWorld;

/// The registered glacier world.
pub static GLACIER_WORLD: GlacierWorld = GlacierWorld;

/// The frozen seas' level: the top of their ice lid.
pub const FROZEN_SEA_LEVEL: i32 = 0;
/// Rows of sea ice over a frozen sea.
pub const LID_ROWS: i32 = 3;
/// Below this elevation (0-1023) the ground is frozen sea.
const SEA_BELOW: i64 = 330;
/// The ice terraces' height.
pub const TERRACE_ROWS: i32 = 5;
/// The Frozen Titan's plateau (half width).
const TITAN_PLATEAU: i64 = 34;
/// Extra sky over the hills (ice spires).
const EXTRA_SKY: i32 = 20;

/// The elevation field at `column` (0-1023; below [`SEA_BELOW`] is frozen sea), the
/// settlement and the titan always on the glacier.
#[must_use]
pub fn elevation(ground: &PlanetGround, column: i64) -> i64 {
    let width = i64::from(ground.strip_width());
    let broad = ground.periodic_noise(70, column, 256);
    let fine = ground.periodic_noise(71, column, 32);
    let mut value = (broad * 3 + fine) / 4;
    let half = i64::from(super::SETTLEMENT_WIDTH / 2);
    let delta = wrap_delta(column, settlement_column(ground) + half, width).abs();
    if delta < half + 50 {
        value = value.max(700 - (delta - half).max(0) * 6);
    }
    let titan = wrap_delta(column, landmark_column(ground), width).abs();
    if titan < TITAN_PLATEAU + 50 {
        value = value.max(760 - (titan - TITAN_PLATEAU).max(0) * 6);
    }
    value
}

/// Whether `column` is frozen sea.
#[must_use]
pub fn frozen_sea_at(ground: &PlanetGround, column: i64) -> bool {
    elevation(ground, column) < SEA_BELOW
}

/// Rows of water under a frozen sea's lid at `column` (0 on the glacier).
#[must_use]
pub fn sea_rows(ground: &PlanetGround, column: i64) -> i32 {
    let value = elevation(ground, column);
    if value >= SEA_BELOW {
        return 0;
    }
    4 + i32::try_from((SEA_BELOW - value) * 12 / SEA_BELOW).unwrap_or(0)
}

/// The glacier's natural surface (terraced) at `column`.
fn natural(ground: &PlanetGround, column: i64) -> i32 {
    let value = elevation(ground, column);
    if value < SEA_BELOW {
        return FROZEN_SEA_LEVEL;
    }
    let height = i64::from(ground.max_hill_rows());
    let rows = i32::try_from((value - SEA_BELOW) * height / (1_023 - SEA_BELOW)).unwrap_or(0);
    -(rows - rows.rem_euclid(TERRACE_ROWS))
}

/// The glacier's surface row at `column` (the first ground row).
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let half = i64::from(super::SETTLEMENT_WIDTH / 2);
    let settlement = settlement_column(ground) + half;
    if let Some(weight) = site_weight(column, settlement, half, width) {
        return ease(natural(ground, column), natural(ground, settlement), weight);
    }
    let titan = landmark_column(ground);
    if let Some(weight) = site_weight(column, titan, TITAN_PLATEAU, width) {
        return ease(natural(ground, column), natural(ground, titan), weight);
    }
    natural(ground, column)
}

/// A crevasse: `(first column, width, depth, bridged)`.
type Crevasse = (i64, i64, i32, bool);

/// The crevasse of 64-column cell `cell`, if it has one (never at the sites, never at sea).
#[must_use]
pub fn crevasse_in(ground: &PlanetGround, cell: i64) -> Option<Crevasse> {
    let width = i64::from(ground.strip_width());
    let cells = width / 64;
    let hash = roll(ground, 40, cell.rem_euclid(cells), 0);
    if hash % 100 >= 50 {
        return None;
    }
    let column = cell * 64 + 8 + i64::try_from((hash >> 8) % 48).unwrap_or(0);
    let crack = 2 + i64::try_from((hash >> 16) % 2).unwrap_or(0);
    let half = i64::from(super::SETTLEMENT_WIDTH / 2);
    let settlement = settlement_column(ground) + half;
    if wrap_delta(column, settlement, width).abs() <= half + 16
        || wrap_delta(column, landmark_column(ground), width).abs() <= TITAN_PLATEAU + 14
    {
        return None;
    }
    if (-1..=crack).any(|dx| elevation(ground, column + dx) < SEA_BELOW + 40) {
        return None;
    }
    let depth = 12 + i32::try_from((hash >> 24) % 30).unwrap_or(0);
    Some((column, crack, depth, (hash >> 40).is_multiple_of(3)))
}

/// An ice spire: `(column, height)`.
fn spire_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i32)> {
    let width = i64::from(ground.strip_width());
    let cells = width / 96;
    let hash = roll(ground, 41, cell.rem_euclid(cells), 0);
    if hash % 100 >= 25 {
        return None;
    }
    let column = cell * 96 + 10 + i64::try_from((hash >> 8) % 76).unwrap_or(0);
    let half = i64::from(super::SETTLEMENT_WIDTH / 2);
    if wrap_delta(column, settlement_column(ground) + half, width).abs() <= half + 14
        || wrap_delta(column, landmark_column(ground), width).abs() <= TITAN_PLATEAU + 6
        || frozen_sea_at(ground, column)
    {
        return None;
    }
    Some((column, 10 + i32::try_from((hash >> 16) % 9).unwrap_or(0)))
}

/// The Frozen Titan's tile at `(dx, depth)` from its centre column and the surface there.
fn titan_tile(dx: i64, depth: i32) -> Option<Tile> {
    let bone = || Some(wild(WildTile::TitanBone));
    // Its tusk curls up out of the ice: two tiles thick, each step overlapping the last so it
    // reads as one curved horn, not loose blocks.
    if depth < 0 {
        return match (dx, depth) {
            (-20 | -19, -1 | -2) | (-19 | -18, -3 | -4) | (-18 | -17, -5) | (-17 | -16, -6) => {
                bone()
            }
            (-16, -7) => bone(),
            _ => None,
        };
    }
    if !(-24..=26).contains(&dx) || depth > 24 {
        return None;
    }
    let d = i64::from(depth);
    // The skull.
    let (sx, sy) = (dx + 14, d - 10);
    if sx * sx * 9 + sy * sy * 25 <= 225 {
        // An eye socket.
        if sx == -1 && sy == -1 {
            return Some(wild(WildTile::BlueIce));
        }
        return bone();
    }
    // The tusk's root down into the jaw.
    if (dx == -19 || dx == -18) && (0..=6).contains(&depth) {
        return bone();
    }
    // The spine.
    if (-9..=24).contains(&dx) && depth == 9 {
        return bone();
    }
    // The ribs: arches down from the spine.
    if (-2..=18).contains(&dx) && dx.rem_euclid(4) == 2 && (9..=19).contains(&depth) {
        return bone();
    }
    if (-2..=18).contains(&dx) && depth == 20 && dx.rem_euclid(4) != 0 {
        return bone();
    }
    // Its heart, in the ribs.
    if dx == 8 && depth == 15 {
        return Some(wild(WildTile::TitanHeart));
    }
    // Clear ice all round it.
    if (3..=22).contains(&depth) && (-22..=24).contains(&dx) {
        return Some(wild(WildTile::BlueIce));
    }
    None
}

/// Whether `cell` (in the Permafrost Caves) is in an ice cave.
fn in_ice_cave(ground: &PlanetGround, first: i64, row: i32, span: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let (cell_x, cell_y) = (first.div_euclid(72), row.div_euclid(28));
    let cells = width / 72;
    let hash = roll(ground, 42, cell_x.rem_euclid(cells), i64::from(cell_y));
    if hash % 100 >= 50 {
        return false;
    }
    let x = cell_x * 72 + 18 + i64::try_from((hash >> 8) % 36).unwrap_or(0);
    let y = cell_y * 28 + 8 + i32::try_from((hash >> 16) % 12).unwrap_or(0);
    let half_width = 8 + i64::try_from((hash >> 24) % 10).unwrap_or(0);
    let half_height = 4 + i64::try_from((hash >> 32) % 4).unwrap_or(0);
    let dx = wrap_delta(first, x, width) / span.max(1);
    let dy = i64::from(row - y);
    dx * dx * half_height * half_height + dy * dy * half_width * half_width
        <= half_width * half_width * half_height * half_height
}

/// Whether `cell` (in the Under-Ice Sea) is in a lens of water: `Some(true)` water,
/// `Some(false)` its ice lid, `None` neither.
fn in_lens(ground: &PlanetGround, first: i64, row: i32, span: i64) -> Option<bool> {
    let width = i64::from(ground.strip_width());
    let (cell_x, cell_y) = (first.div_euclid(128), row.div_euclid(36));
    let cells = width / 128;
    let hash = roll(ground, 43, cell_x.rem_euclid(cells), i64::from(cell_y));
    if hash % 100 >= 60 {
        return None;
    }
    let x = cell_x * 128 + 40 + i64::try_from((hash >> 8) % 48).unwrap_or(0);
    let y = cell_y * 36 + 10 + i32::try_from((hash >> 16) % 16).unwrap_or(0);
    let half_width = 20 + i64::try_from((hash >> 24) % 18).unwrap_or(0);
    let half_height = 4 + i64::try_from((hash >> 32) % 5).unwrap_or(0);
    let dx = wrap_delta(first, x, width) / span.max(1);
    let inside = |dy: i64| {
        dx * dx * half_height * half_height + dy * dy * half_width * half_width
            <= half_width * half_width * half_height * half_height
    };
    let dy = i64::from(row - y);
    if inside(dy) {
        return Some(true);
    }
    inside(dy + 1).then_some(false)
}

impl ThemeWorld for GlacierWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::GLACIER
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
        reason = "one pass over the glacier's features"
    )]
    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile> {
        let width = i64::from(ground.strip_width());
        let origin = chunk.origin();
        let first_cell = i64::from(origin.x).div_euclid(64) - 1;
        let crevasses: Vec<Crevasse> = (first_cell..=first_cell + 2)
            .filter_map(|cell| crevasse_in(ground, cell))
            .collect();
        let spire_cell = i64::from(origin.x).div_euclid(96) - 1;
        let spires: Vec<(i64, i32)> = (spire_cell..=spire_cell + 2)
            .filter_map(|cell| spire_in(ground, cell))
            .collect();
        let titan = landmark_column(ground);
        let near_titan =
            wrap_delta(i64::from(origin.x) + 16, titan, width).abs() <= TITAN_PLATEAU + 24;
        let small = is_small_body(ground.params());
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_titan && cell.span == 1 {
                    let floor = surface(ground, titan);
                    let dx = wrap_delta(cell.first, titan, width);
                    if let Some(tile) = titan_tile(dx, cell.row - floor) {
                        return tile;
                    }
                }
                if small
                    && let Some(tile) = secret_tile(
                        ground,
                        cell,
                        wild(WildTile::BlueIce),
                        wild(WildTile::BlueIce),
                    )
                {
                    return tile;
                }
                if cell.span == 1 {
                    for &(column, height) in &spires {
                        let dx = wrap_delta(cell.first, column, width);
                        let up = cell.surface - cell.row;
                        if up > 0 && up <= height {
                            let half = i64::from(up <= height * 2 / 3);
                            if dx.abs() <= half {
                                return wild(if up % 4 == 0 {
                                    WildTile::BlueIce
                                } else {
                                    WildTile::GlacierIce
                                });
                            }
                        }
                    }
                    for &(column, crack, depth, bridged) in &crevasses {
                        let dx = wrap_delta(cell.first, column, width);
                        let down = cell.row - cell.surface;
                        if (0..crack).contains(&dx) && (0..depth).contains(&down) {
                            return if bridged && down == 0 {
                                wild(WildTile::Rime)
                            } else {
                                air()
                            };
                        }
                        if (dx == -1 || dx == crack) && (1..=depth).contains(&down) {
                            return wild(WildTile::BlueIce);
                        }
                    }
                }
                if cell.row < cell.surface {
                    return cell.base;
                }
                // A frozen sea: the ice lid, the cold water, then the seabed.
                if cell.surface == FROZEN_SEA_LEVEL && cell.span == 1 {
                    let water_rows = sea_rows(ground, cell.first);
                    if water_rows > 0 {
                        if cell.depth < LID_ROWS {
                            return wild(WildTile::SeaIce);
                        }
                        if cell.depth < LID_ROWS + water_rows {
                            return water();
                        }
                        if cell.depth < LID_ROWS + water_rows + 2 {
                            return wild(WildTile::Permafrost);
                        }
                    }
                }
                match cell.stratum {
                    1 => match in_lens(ground, cell.first, cell.row, cell.span) {
                        Some(true) => water(),
                        Some(false) => wild(WildTile::SeaIce),
                        None => cell.base,
                    },
                    2 => {
                        if in_ice_cave(ground, cell.first, cell.row, cell.span) {
                            let ceiling = !in_ice_cave(ground, cell.first, cell.row - 1, cell.span);
                            if ceiling
                                && roll(ground, 44, cell.first, i64::from(cell.row))
                                    .is_multiple_of(3)
                            {
                                return wild(WildTile::Icicle);
                            }
                            return air();
                        }
                        let span = cell.span;
                        let lining = in_ice_cave(ground, cell.first, cell.row - 1, span)
                            || in_ice_cave(ground, cell.first, cell.row + 1, span)
                            || in_ice_cave(ground, cell.first - span, cell.row, span)
                            || in_ice_cave(ground, cell.first + span, cell.row, span);
                        if lining {
                            return wild(WildTile::BlueIce);
                        }
                        if matches!(cell.base.kind, TileKind::Wild(WildTile::Permafrost))
                            && roll(ground, 45, cell.first, i64::from(cell.row)) % 10_000 < 40
                        {
                            return wild(WildTile::FrostCrystal);
                        }
                        cell.base
                    }
                    _ => {
                        // Snow on the glacier's terraces stays snow; ice under it.
                        if matches!(cell.base.kind, TileKind::Surface(SurfaceMaterial::Snow))
                            && cell.depth >= 2
                            && cell.surface < FROZEN_SEA_LEVEL - TERRACE_ROWS
                        {
                            return tile(TileKind::Wild(WildTile::GlacierIce));
                        }
                        cell.base
                    }
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
            first: i32::try_from(centre - 24).unwrap_or(0),
            width: 51,
            floor: surface(ground, centre),
        })
    }
}
