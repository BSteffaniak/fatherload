//! The jungle world (Wave 12 planets-a): rolling hills under giant trees, and below them the
//! Root Tangle, the Spore Caves and the Heartwood Deep.
//!
//! * **Surface**: the generic hills, with a giant tree in most 32-column cells: a trunk two or
//!   three tiles wide, 14-31 rows tall, a canopy of soft leaves, **vines** hanging from the
//!   canopy's underside (open air a pilot climbs: [`vine_at`]) and a **taproot** of living
//!   root snaking down into the Root Tangle.
//! * **Root Tangle** (the top third): root soil knotted with living roots; signature rule
//!   **grasping roots**: a rig rubbing against living root is held back (`world/wilds.rs`).
//! * **Spore Caves**: damp spore stone and mycelium, riddled with extra caves whose floors are
//!   lined with **puffcaps**; signature rule **spore bursts**: a drilled puffcap bursts into a
//!   spore cloud (toxic air; the Bio-Filter shrugs it off).
//! * **Heartwood Deep**: the world tree's ancient root mass, hard as hard rock, with amber
//!   (fossils) and glowing **lifesap** nodes; signature rule **lifesap**: drilling a node
//!   mends the hull, so a dive here is routed from node to node.
//! * **Landmark: the Elder Tree**, half a lap from the settlement: an eight-wide trunk 58 rows
//!   tall on a levelled rise, a crown of canopy, giant roots, and a hollow at its foot (a door
//!   on its west side) holding the Elder Tree's seed.
//! * **Settlement site**: a level clearing with no trees ([`super::SETTLEMENT_WIDTH`]).

use super::{
    Cell, SurfaceSite, WildTile, air, chunk_with, core_floor_row, ease, is_small_body,
    landmark_column, roll, secret_tile, settlement_column, settlement_site_of, site_weight, wild,
    wrap_delta,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, PlanetThemeId,
        ThemeWorld,
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind},
};

/// The jungle's strata (registry data).
pub const JUNGLE_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Root Tangle",
        share_permille: 330,
        rock: TileKind::Wild(WildTile::RootSoil),
        accent: TileKind::Wild(WildTile::LivingRoot),
        accent_permille: 140,
        ores: &[
            (MineralKind::Copper, 90),
            (MineralKind::Iron, 60),
            (MineralKind::Emerald, 18),
        ],
    },
    PlanetStratum {
        name: "Spore Caves",
        share_permille: 330,
        rock: TileKind::Wild(WildTile::SporeStone),
        accent: TileKind::Wild(WildTile::Mycel),
        accent_permille: 220,
        ores: &[
            (MineralKind::Silver, 60),
            (MineralKind::Gold, 40),
            (MineralKind::Ruby, 16),
        ],
    },
    PlanetStratum {
        name: "Heartwood Deep",
        share_permille: 340,
        rock: TileKind::Wild(WildTile::Heartwood),
        accent: TileKind::Wild(WildTile::AmberResin),
        accent_permille: 60,
        ores: &[
            (MineralKind::Diamond, 22),
            (MineralKind::Platinum, 26),
            (MineralKind::Mythril, 8),
        ],
    },
];

/// The jungle theme (registry entry [`PlanetThemeId::JUNGLE`]).
pub const JUNGLE_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::JUNGLE,
    name: "Jungle World",
    palette: PlanetPalette {
        sky: [120, 186, 150],
        atmosphere: [140, 230, 160],
        surface: [58, 120, 52],
        rock: [92, 70, 48],
        deep: [70, 46, 30],
        core: [140, 255, 170],
    },
    crust: TileKind::Wild(WildTile::JungleLoam),
    crust_rows: 4,
    strata: &JUNGLE_STRATA,
    core_ore: TileKind::Ore(MineralKind::Emerald),
    caves_per_256_columns: 2,
    hill_permille: 26,
};

/// The jungle's theme world.
pub struct JungleWorld;

/// The registered jungle world.
pub static JUNGLE_WORLD: JungleWorld = JungleWorld;

/// Columns per tree cell (one tree at most in each).
pub const TREE_CELL: i64 = 32;
/// The Elder Tree's trunk width, height and the level rise it stands on (half width).
pub const ELDER_TRUNK: i64 = 8;
pub const ELDER_HEIGHT: i32 = 58;
const ELDER_RISE: i64 = 28;
/// Extra sky over the hills (the Elder Tree's crown).
const EXTRA_SKY: i32 = 56;

/// One giant tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tree {
    /// The trunk's first column.
    pub column: i64,
    pub width: i64,
    /// The trunk's top row (the canopy's centre).
    pub top: i32,
    /// The canopy's half width and half height.
    pub crown: (i64, i32),
    /// Rows the taproot reaches below the surface.
    pub root: i32,
}

/// The tree of cell `cell`, if it has one.
#[must_use]
pub fn tree_in_cell(ground: &PlanetGround, cell: i64) -> Option<Tree> {
    let width = i64::from(ground.strip_width());
    let cells = width / TREE_CELL;
    let cell = cell.rem_euclid(cells);
    let hash = roll(ground, 10, cell, 0);
    if hash % 10 >= 7 {
        return None;
    }
    let column = cell * TREE_CELL + 4 + i64::try_from((hash >> 8) % 22).unwrap_or(0);
    let trunk = 2 + i64::try_from((hash >> 16) % 2).unwrap_or(0);
    // Not in the clearing, nor on the Elder Tree's rise.
    let clear = |centre: i64, half: i64| {
        wrap_delta(column, centre, width).abs() <= half + 14
            || wrap_delta(column + trunk, centre, width).abs() <= half + 14
    };
    let settlement = settlement_column(ground) + i64::from(super::SETTLEMENT_WIDTH / 2);
    if clear(settlement, i64::from(super::SETTLEMENT_WIDTH / 2))
        || clear(landmark_column(ground), ELDER_RISE)
    {
        return None;
    }
    let height = 14 + i32::try_from((hash >> 24) % 18).unwrap_or(0);
    let floor = surface(ground, column + trunk / 2);
    Some(Tree {
        column,
        width: trunk,
        top: floor - height,
        crown: (
            5 + i64::try_from((hash >> 32) % 5).unwrap_or(0),
            3 + i32::try_from((hash >> 40) % 2).unwrap_or(0),
        ),
        root: 10 + i32::try_from((hash >> 44) % 16).unwrap_or(0),
    })
}

/// The jungle's surface row at `column`: the generic hills, levelled at the clearing and the
/// Elder Tree's rise.
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let natural = -ground.hill(column);
    let half = i64::from(super::SETTLEMENT_WIDTH / 2);
    let settlement = settlement_column(ground) + half;
    if let Some(weight) = site_weight(column, settlement, half, width) {
        return ease(natural, -ground.hill(settlement), weight);
    }
    let landmark = landmark_column(ground);
    if let Some(weight) = site_weight(column, landmark, ELDER_RISE, width) {
        return ease(natural, -ground.hill(landmark) - 2, weight);
    }
    natural
}

/// Whether a vine hangs in the open air at strip tile `(column, row)` (pilots climb vines; the
/// renderer draws them).
#[must_use]
pub fn vine_at(ground: &PlanetGround, column: i64, row: i32) -> bool {
    if ground.params().theme != PlanetThemeId::JUNGLE {
        return false;
    }
    let width = i64::from(ground.strip_width());
    let cell = column.div_euclid(TREE_CELL);
    for offset in -1..=1 {
        let Some(tree) = tree_in_cell(ground, cell + offset) else {
            continue;
        };
        let centre = tree.column + tree.width / 2;
        let dx = wrap_delta(column, centre, width);
        if dx.abs() > tree.crown.0 - 1 || dx.abs() <= tree.width / 2 + 1 {
            continue;
        }
        let strand = roll(ground, 11, column.rem_euclid(width), 0);
        if !strand.is_multiple_of(3) {
            continue;
        }
        let bottom = canopy_bottom(&tree, dx);
        let length = 4 + i32::try_from((strand >> 8) % 12).unwrap_or(0);
        let floor = surface(ground, column);
        if row > bottom && row <= (bottom + length).min(floor - 2) {
            return true;
        }
    }
    // The Elder Tree's crown trails long vines.
    let landmark = landmark_column(ground);
    let dx = wrap_delta(column, landmark, width);
    if (6..=22).contains(&dx.abs())
        && roll(ground, 12, column.rem_euclid(width), 0).is_multiple_of(2)
    {
        let floor = surface(ground, landmark);
        let crown_bottom = floor - ELDER_HEIGHT + 8;
        let length = 10
            + i32::try_from((roll(ground, 13, column.rem_euclid(width), 0) >> 8) % 24).unwrap_or(0);
        return row > crown_bottom && row <= (crown_bottom + length).min(floor - 3);
    }
    false
}

/// The canopy's lowest row at `dx` columns from the tree's centre.
fn canopy_bottom(tree: &Tree, dx: i64) -> i32 {
    let (half_width, half_height) = tree.crown;
    let half_width = half_width.max(1);
    // An ellipse: the underside rises toward the edges.
    let lift =
        i32::try_from(dx * dx * i64::from(half_height) / (half_width * half_width)).unwrap_or(0);
    tree.top + half_height - lift
}

/// The tree's tile at `(column, row)`, if any (`floor` is the surface there).
fn tree_tile(tree: &Tree, column: i64, row: i32, floor: i32, width: i64) -> Option<Tile> {
    let centre = tree.column + tree.width / 2;
    let dx = wrap_delta(column, centre, width);
    let in_trunk = (0..tree.width).contains(&wrap_delta(column, tree.column, width));
    if row < floor && row >= tree.top && in_trunk {
        return Some(wild(WildTile::Trunk));
    }
    // The canopy: an ellipse round the trunk's top, a little ragged.
    let (half_width, half_height) = tree.crown;
    let dy = i64::from(row - tree.top);
    let ragged = i64::from(u8::from((column + i64::from(row)).rem_euclid(5) == 0));
    if dx * dx * i64::from(half_height * half_height) + dy * dy * half_width * half_width
        <= half_width * half_width * i64::from(half_height * half_height) + ragged * half_width
        && row < floor
    {
        return Some(wild(WildTile::Canopy));
    }
    // The taproot snakes down under the trunk.
    if row >= floor && row < floor + tree.root {
        let wiggle = i64::from((row - floor) / 4 % 3) - 1;
        let at = centre + wiggle;
        if wrap_delta(column, at, width).abs() <= i64::from(row - floor < tree.root / 2) {
            return Some(wild(WildTile::LivingRoot));
        }
    }
    None
}

/// The Elder Tree's tile at `(column, row)`, if any.
fn elder_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let centre = landmark_column(ground);
    let dx = wrap_delta(column, centre, width);
    if dx.abs() > ELDER_RISE + 4 {
        return None;
    }
    let floor = surface(ground, centre);
    let top = floor - ELDER_HEIGHT;
    // The hollow at its foot (a door on the west side) and the seed in it.
    let hollow = (-3..=2).contains(&dx) && (floor - 6..floor).contains(&row);
    let door = (-ELDER_TRUNK / 2..-2).contains(&dx) && (floor - 4..floor).contains(&row);
    if hollow || door {
        if dx == 0 && row == floor - 1 {
            return Some(wild(WildTile::ElderSeed));
        }
        return Some(air());
    }
    if (-ELDER_TRUNK / 2..ELDER_TRUNK / 2).contains(&dx) && row >= top && row < floor {
        return Some(wild(WildTile::ElderBark));
    }
    // A crown of three great clouds of leaves.
    for (cx, cy, half_width, half_height) in [
        (0_i64, top, 24_i64, 9_i64),
        (-14, top + 6, 12, 6),
        (15, top + 5, 13, 6),
    ] {
        let ddx = dx - cx;
        let ddy = i64::from(row - cy);
        if ddx * ddx * half_height * half_height + ddy * ddy * half_width * half_width
            <= half_width * half_width * half_height * half_height
            && row < floor - 8
        {
            return Some(wild(WildTile::Canopy));
        }
    }
    // Giant roots: two thick arms spreading down and out, one deep taproot.
    let depth = row - floor;
    if (-2..36).contains(&depth) {
        let spread = i64::from(depth.max(0)) * 3 / 4 + 3;
        for side in [-1_i64, 1] {
            if (dx - side * spread).abs() <= 1 {
                return Some(wild(WildTile::LivingRoot));
            }
        }
        if (0..60).contains(&depth) && dx.abs() <= 1 {
            return Some(wild(WildTile::ElderBark));
        }
    }
    if (36..60).contains(&depth) && dx.abs() <= 1 {
        return Some(wild(WildTile::ElderBark));
    }
    None
}

/// Whether strip tile `(first, row)` is in one of the Spore Caves' own caves.
fn in_spore_cave(ground: &PlanetGround, first: i64, row: i32, span: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let cell_x = first.div_euclid(64);
    let cell_y = row.div_euclid(24);
    for ox in -1..=1 {
        for oy in -1..=1 {
            let (cx, cy) = (cell_x + ox, cell_y + oy);
            let cells = width / 64;
            let hash = roll(ground, 20, cx.rem_euclid(cells), i64::from(cy));
            if hash % 100 >= 55 {
                continue;
            }
            let centre_x = cx * 64 + i64::try_from((hash >> 8) % 64).unwrap_or(0);
            let centre_y = cy * 24 + i32::try_from((hash >> 16) % 24).unwrap_or(0);
            let half_width = 6 + i64::try_from((hash >> 24) % 9).unwrap_or(0);
            let half_height = 3 + i64::try_from((hash >> 32) % 4).unwrap_or(0);
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

impl ThemeWorld for JungleWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::JUNGLE
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
        // The trees whose crowns or roots reach this chunk.
        let first_cell = i64::from(origin.x).div_euclid(TREE_CELL) - 1;
        let trees: Vec<Tree> = (first_cell..=first_cell + 3)
            .filter_map(|cell| tree_in_cell(ground, cell))
            .collect();
        let near_landmark = wrap_delta(i64::from(origin.x) + 16, landmark_column(ground), width)
            .abs()
            <= ELDER_RISE + 24;
        let small = is_small_body(ground.params());
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_landmark && let Some(tile) = elder_tile(ground, cell.first, cell.row) {
                    return tile;
                }
                if small
                    && let Some(tile) = secret_tile(
                        ground,
                        cell,
                        wild(WildTile::Heartwood),
                        wild(WildTile::AmberResin),
                    )
                {
                    return tile;
                }
                if cell.span == 1 {
                    for tree in &trees {
                        if let Some(tile) =
                            tree_tile(tree, cell.first, cell.row, cell.surface, width)
                        {
                            return tile;
                        }
                    }
                }
                if cell.row < cell.surface {
                    return cell.base;
                }
                match cell.stratum {
                    1 if cell.depth > 6 => {
                        if in_spore_cave(ground, cell.first, cell.row, cell.span) {
                            // A puffcap on the cave floor, now and then.
                            let floor = !in_spore_cave(ground, cell.first, cell.row + 1, cell.span);
                            if floor
                                && roll(ground, 21, cell.first, i64::from(cell.row))
                                    .is_multiple_of(4)
                            {
                                return wild(WildTile::Puffcap);
                            }
                            return air();
                        }
                        cell.base
                    }
                    2 => {
                        // Lifesap nodes: small glowing pockets in the heartwood.
                        let (cx, cy) = (cell.first.div_euclid(24), cell.row.div_euclid(16));
                        let hash = roll(ground, 22, cx, i64::from(cy));
                        if hash % 100 < 35 && !matches!(cell.base.kind, TileKind::Air) {
                            let nx = cx * 24 + i64::try_from((hash >> 8) % 20).unwrap_or(0);
                            let ny = cy * 16 + i32::try_from((hash >> 16) % 14).unwrap_or(0);
                            let dx = wrap_delta(cell.first, nx, width) / cell.span.max(1);
                            if (0..=1).contains(&dx) && (ny..=ny + 1).contains(&cell.row) {
                                return wild(WildTile::Lifesap);
                            }
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
            first: i32::try_from(centre - ELDER_TRUNK / 2).unwrap_or(0),
            width: i32::try_from(ELDER_TRUNK).unwrap_or(8),
            floor: surface(ground, centre),
        })
    }
}
