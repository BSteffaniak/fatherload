//! The tidally locked world (Wave 13 planets-d; the space generator's tidally locked tag): one
//! face in eternal noon, the other in eternal night, and a living twilight ring between them.
//!
//! Column 0 faces the star (the body clock's convention, [`crate::bodylife::clock`]); the
//! terminators lie a quarter lap either way and the night's heart half a lap round. Every
//! column is on one [`Side`], and the two halves play apart from the surface to the core:
//!
//! | | Day side | Twilight ring | Night side |
//! |---|---|---|---|
//! | Surface | cracked hardpan, salt pans of crust and sun glass, flat-topped mesas | rolling lichen hills, glowmoss, meltwater pools; the settlement | nitrogen plains, pressure ridges, ice spikes |
//! | Divided Crust (**sun and shadow**) | burns (heat gear, or shade underground) | mild | freezes (cold gear) |
//! | Geyser Fields (**geysers**) | vent rock; steam geysers | | cryo rock; nitrogen geysers |
//! | Stress Fractures (**tidal quakes**) | fractured rock over rubble-roofed cracks | | the same rock, its own ores |
//! | Lodestone Core (**lodestone drift**) | lodestone pulling toward the sunward meridian | | |
//! | Ores | gold, ruby, copper, uranium | | silver, emerald, diamond, platinum |
//! | Creatures | sunbaskers, scorch mites | dusk grazers | frost stalkers |
//!
//! * **Landmark: the Gloam Gate** on the morning terminator: an arch with one pillar scorched
//!   and one frozen over a level arena, the Gloamwarden's seal at its foot and a relic in its
//!   keystone.
//! * **The Dark Observatory** (a builder outpost) stands out on the night side.

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
    ExoticTile, air, exotic, outpost_room_tile, roll, secret_tile, tile, value_noise2, wrap_delta,
};
use crate::{
    fluid::{FLUID_FULL, FluidKind},
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, ThemeWorld,
        wilds::{Cell, chunk_with, core_floor_row, ease, site_weight},
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind, TilePosition},
};

/// The locked world's strata (registry data; the materials and ores by side are
/// [`stratum_tile`]'s).
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Divided Crust",
        share_permille: 240,
        rock: TileKind::Exotic(ExoticTile::Hardpan),
        accent: TileKind::Exotic(ExoticTile::SaltCrust),
        accent_permille: 120,
        ores: &[
            (MineralKind::Copper, 60),
            (MineralKind::Gold, 40),
            (MineralKind::Silver, 40),
        ],
    },
    PlanetStratum {
        name: "Geyser Fields",
        share_permille: 260,
        rock: TileKind::Exotic(ExoticTile::VentRock),
        accent: TileKind::Exotic(ExoticTile::CryoRock),
        accent_permille: 100,
        ores: &[
            (MineralKind::Gold, 40),
            (MineralKind::Platinum, 30),
            (MineralKind::Uranium, 12),
        ],
    },
    PlanetStratum {
        name: "Stress Fractures",
        share_permille: 260,
        rock: TileKind::Exotic(ExoticTile::FractureRock),
        accent: TileKind::Exotic(ExoticTile::FaultRubble),
        accent_permille: 80,
        ores: &[
            (MineralKind::Ruby, 26),
            (MineralKind::Emerald, 26),
            (MineralKind::Mythril, 10),
        ],
    },
    PlanetStratum {
        name: "Lodestone Core",
        share_permille: 240,
        rock: TileKind::Exotic(ExoticTile::Lodestone),
        accent: TileKind::Exotic(ExoticTile::FractureRock),
        accent_permille: 100,
        ores: &[
            (MineralKind::Iron, 120),
            (MineralKind::Platinum, 40),
            (MineralKind::Mythril, 14),
        ],
    },
];

/// The locked world theme (registry entry [`super::LOCKED_WORLD`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::LOCKED_WORLD,
    name: "Locked World",
    palette: PlanetPalette {
        sky: [206, 150, 104],
        atmosphere: [255, 178, 120],
        surface: [196, 120, 74],
        rock: [132, 96, 84],
        deep: [56, 54, 72],
        core: [150, 190, 255],
    },
    crust: TileKind::Exotic(ExoticTile::Hardpan),
    crust_rows: 0,
    strata: &STRATA,
    core_ore: TileKind::Ore(MineralKind::Platinum),
    caves_per_256_columns: 1,
    hill_permille: 12,
};

/// The locked world's generator.
pub struct LockedWorld;

/// The registered locked world.
pub static LOCKED_WORLD_GEN: LockedWorld = LockedWorld;

/// Which face of the world a column is on.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Side {
    /// Eternal noon: scorched.
    Day,
    /// The terminator's ring: mild, alive.
    Twilight,
    /// Eternal night: frozen.
    Night,
}

/// Columns of the twilight ring either side of a terminator on `ground` (3 % of the lap, at
/// least 120).
#[must_use]
pub fn twilight_half(ground: &PlanetGround) -> i64 {
    (i64::from(ground.strip_width()) * 3 / 100).max(120)
}

/// How warm `column` is (`0..=1024`): 1,024 on the day side, 0 on the night side, falling
/// across the twilight ring (512 on the terminator).
#[must_use]
pub fn warmth(ground: &PlanetGround, column: i64) -> i64 {
    let width = i64::from(ground.strip_width());
    let quarter = width / 4;
    let phi = wrap_delta(column, 0, width).abs();
    let twilight = twilight_half(ground);
    ((quarter - phi) * 512 / twilight + 512).clamp(0, 1_024)
}

/// The side `column` is on.
#[must_use]
pub fn side_of(ground: &PlanetGround, column: i64) -> Side {
    match warmth(ground, column) {
        640..=1_024 => Side::Day,
        0..=383 => Side::Night,
        _ => Side::Twilight,
    }
}

/// The settlement's middle column: on the evening terminator.
#[must_use]
pub fn settlement_centre(ground: &PlanetGround) -> i64 {
    i64::from(ground.strip_width()) / 4
}

/// The Gloam Gate's middle column: on the morning terminator.
#[must_use]
pub fn gate_column(ground: &PlanetGround) -> i64 {
    i64::from(ground.strip_width()) * 3 / 4
}

/// The Dark Observatory's middle column: out on the night side.
#[must_use]
pub fn observatory_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    width / 2 + width / 16
}

/// The settlement's half width.
#[must_use]
pub fn settlement_half(ground: &PlanetGround) -> i64 {
    i64::from(super::settlement_half(ground, 24))
}

/// The arena's half width at the Gloam Gate.
pub const GATE_HALF: i64 = 40;
/// The Gate's pillars: their offset from the middle and their height.
pub const GATE_PILLAR: i64 = 22;
pub const GATE_HEIGHT: i32 = 26;
/// The Observatory's level ground (half width).
const OBSERVATORY_HALF: i64 = 12;
/// Extra sky over the hills (mesas, the Gate).
const EXTRA_SKY: i32 = 24;

/// A mesa of the 96-column cell `cell`: `(centre, half width, height)`.
fn mesa_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i64, i32)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 10, cell.rem_euclid(width / 96), 0);
    if hash % 100 >= 40 {
        return None;
    }
    let centre = cell * 96 + 24 + i64::try_from((hash >> 8) % 48).unwrap_or(0);
    let half = 8 + i64::try_from((hash >> 16) % 12).unwrap_or(0);
    // Only deep in the day (the whole mesa).
    if warmth(ground, centre - half - 6) < 1_024 || warmth(ground, centre + half + 6) < 1_024 {
        return None;
    }
    Some((
        centre,
        half,
        10 + i32::try_from((hash >> 24) % 9).unwrap_or(0),
    ))
}

/// A salt pan of the 64-column cell `cell`: `(centre, half width)`.
fn pan_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i64)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 11, cell.rem_euclid(width / 64), 0);
    if hash % 100 >= 35 {
        return None;
    }
    let centre = cell * 64 + 20 + i64::try_from((hash >> 8) % 24).unwrap_or(0);
    let half = 5 + i64::try_from((hash >> 16) % 8).unwrap_or(0);
    (warmth(ground, centre - half) >= 900 && warmth(ground, centre + half) >= 900)
        .then_some((centre, half))
}

/// A meltwater pool of the 80-column cell `cell`: `(centre, half width)`.
fn pool_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i64)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 12, cell.rem_euclid(width / 80), 0);
    if hash % 100 >= 45 {
        return None;
    }
    let centre = cell * 80 + 20 + i64::try_from((hash >> 8) % 40).unwrap_or(0);
    let half = 4 + i64::try_from((hash >> 16) % 5).unwrap_or(0);
    let mild = |column: i64| (200..=824).contains(&warmth(ground, column));
    let settlement =
        wrap_delta(centre, settlement_centre(ground), width).abs() <= settlement_half(ground) + 20;
    (mild(centre - half) && mild(centre + half) && !settlement).then_some((centre, half))
}

/// An ice spike of the 24-column cell `cell`: `(column, height)`.
fn spike_in(ground: &PlanetGround, cell: i64) -> Option<(i64, i32)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 13, cell.rem_euclid(width / 24), 0);
    if hash % 100 >= 30 {
        return None;
    }
    let column = cell * 24 + 4 + i64::try_from((hash >> 8) % 16).unwrap_or(0);
    if warmth(ground, column) > 100
        || wrap_delta(column, observatory_column(ground), width).abs() <= OBSERVATORY_HALF + 6
    {
        return None;
    }
    Some((column, 4 + i32::try_from((hash >> 16) % 6).unwrap_or(0)))
}

/// The natural surface (before the sites level it) at `column`.
fn natural(ground: &PlanetGround, column: i64) -> i32 {
    let warm = warmth(ground, column);
    let hills = ground.rolling_hills(column);
    // Pressure ridges on the night side: a sawtooth that grows into the dark.
    let saw = (column.rem_euclid(40) - 20).abs();
    let ridge = i32::try_from((20 - saw) * 6 / 20 * (1_024 - warm) / 1_024).unwrap_or(0);
    let mut height = hills / 2 + ridge;
    let cell = column.div_euclid(96);
    for c in cell - 1..=cell + 1 {
        if let Some((centre, half, rise)) = mesa_in(ground, c) {
            let dx = (column - centre).abs();
            if dx <= half {
                height += rise;
            } else if dx <= half + 2 {
                height += rise * i32::try_from(half + 3 - dx).unwrap_or(0) / 3;
            }
        }
    }
    let cell = column.div_euclid(64);
    for c in cell - 1..=cell + 1 {
        if let Some((centre, half)) = pan_in(ground, c)
            && (column - centre).abs() <= half
        {
            height -= 2;
        }
    }
    let cell = column.div_euclid(80);
    for c in cell - 1..=cell + 1 {
        if let Some((centre, half)) = pool_in(ground, c)
            && (column - centre).abs() <= half
        {
            height -= 3;
        }
    }
    -height
}

/// The surface row at `column` (the settlement, the Gate and the Observatory levelled).
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let sites = [
        (settlement_centre(ground), settlement_half(ground)),
        (gate_column(ground), GATE_HALF),
        (observatory_column(ground), OBSERVATORY_HALF),
    ];
    for (centre, half) in sites {
        if let Some(weight) = site_weight(column, centre, half, width) {
            return ease(natural(ground, column), natural(ground, centre), weight);
        }
    }
    natural(ground, column)
}

/// Whether `column` lies in a salt pan.
fn in_pan(ground: &PlanetGround, column: i64) -> bool {
    let cell = column.div_euclid(64);
    (cell - 1..=cell + 1)
        .any(|c| pan_in(ground, c).is_some_and(|(centre, half)| (column - centre).abs() <= half))
}

/// Whether `(column, row)` is meltwater (in a pool under the twilight's sky).
fn in_pool(ground: &PlanetGround, column: i64, row: i32, surface_row: i32) -> bool {
    let cell = column.div_euclid(80);
    (cell - 1..=cell + 1).any(|c| {
        pool_in(ground, c).is_some_and(|(centre, half)| {
            (column - centre).abs() <= half && row < surface_row && row >= surface_row - 3
        })
    })
}

/// A geyser vent near `(column, row)` in the Geyser Fields: the vent tile's position, from the
/// 40 x 30 cell it belongs to (pure: the rules find the vents the same way).
#[must_use]
pub fn vent_in(ground: &PlanetGround, cell_x: i64, cell_y: i32) -> Option<TilePosition> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 14, cell_x.rem_euclid(width / 40), i64::from(cell_y));
    if hash % 100 >= 45 {
        return None;
    }
    let x = cell_x * 40 + 8 + i64::try_from((hash >> 8) % 24).unwrap_or(0);
    let y = cell_y * 30 + 10 + i32::try_from((hash >> 16) % 14).unwrap_or(0);
    Some(TilePosition {
        x: i32::try_from(x.rem_euclid(width)).unwrap_or(0),
        y,
    })
}

/// Rows of open chimney over a vent.
pub const VENT_CHIMNEY: i32 = 6;

/// The vent feature's tile at `(first, row)`, if any: the vent, its chimney (open) and a
/// lining.
fn vent_tile(ground: &PlanetGround, first: i64, row: i32, side: Side) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let cell_x = first.div_euclid(40);
    let cell_y = row.div_euclid(30);
    for cx in cell_x - 1..=cell_x + 1 {
        for cy in cell_y - 1..=cell_y + 1 {
            let Some(vent) = vent_in(ground, cx, cy) else {
                continue;
            };
            let dx = wrap_delta(first, i64::from(vent.x), width);
            let dy = row - vent.y;
            if dx == 0 && dy == 0 {
                return Some(exotic(ExoticTile::GeyserVent));
            }
            if dx.abs() <= 2 && (-VENT_CHIMNEY..0).contains(&dy) {
                return Some(air());
            }
            if dx.abs() <= 3 && (-VENT_CHIMNEY - 1..=0).contains(&dy) {
                return Some(exotic(if side == Side::Night {
                    ExoticTile::CryoRock
                } else {
                    ExoticTile::VentRock
                }));
            }
        }
    }
    None
}

/// A fracture of the 64 x 24 cell `(cell_x, cell_y)`: `(first column, length, row)`: a crack
/// two rows high under a roof of fault rubble.
fn fracture_in(ground: &PlanetGround, cell_x: i64, cell_y: i32) -> Option<(i64, i64, i32)> {
    let width = i64::from(ground.strip_width());
    let hash = roll(ground, 15, cell_x.rem_euclid(width / 64), i64::from(cell_y));
    if hash % 100 >= 50 {
        return None;
    }
    let first = cell_x * 64 + i64::try_from((hash >> 8) % 20).unwrap_or(0);
    let length = 24 + i64::try_from((hash >> 16) % 28).unwrap_or(0);
    let row = cell_y * 24 + 6 + i32::try_from((hash >> 24) % 14).unwrap_or(0);
    Some((first, length, row))
}

fn fracture_tile(ground: &PlanetGround, first: i64, row: i32, span: i64) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let cell_x = first.div_euclid(64);
    let cell_y = row.div_euclid(24);
    for cx in cell_x - 1..=cell_x + 1 {
        for cy in cell_y - 1..=cell_y + 1 {
            let Some((start, length, crack)) = fracture_in(ground, cx, cy) else {
                continue;
            };
            let along = wrap_delta(first, start, width) / span.max(1);
            if !(0..length).contains(&along) {
                continue;
            }
            if row == crack || row == crack + 1 {
                return Some(air());
            }
            if row == crack - 1 || (row == crack - 2 && along % 3 != 0) {
                return Some(exotic(ExoticTile::FaultRubble));
            }
        }
    }
    None
}

/// Ores by side: the day's are hot metals, the night's cold stones.
const fn side_ores(stratum: usize, side: Side) -> &'static [(MineralKind, u16)] {
    match (stratum, side) {
        (0, Side::Day) => &[(MineralKind::Copper, 70), (MineralKind::Gold, 45)],
        (0, Side::Twilight) => &[(MineralKind::Copper, 50), (MineralKind::Iron, 60)],
        (0, Side::Night) => &[(MineralKind::Silver, 60), (MineralKind::Emerald, 18)],
        (1, Side::Night) => &[(MineralKind::Platinum, 34), (MineralKind::Diamond, 10)],
        (1, _) => &[(MineralKind::Gold, 46), (MineralKind::Uranium, 14)],
        (2, Side::Night) => &[(MineralKind::Emerald, 30), (MineralKind::Diamond, 14)],
        (2, _) => &[(MineralKind::Ruby, 30), (MineralKind::Mythril, 10)],
        _ => &[
            (MineralKind::Iron, 120),
            (MineralKind::Platinum, 40),
            (MineralKind::Mythril, 14),
        ],
    }
}

/// The stratum's own tile at `cell` on `side` (rock, accent, ores).
fn stratum_tile(ground: &PlanetGround, cell: &Cell, side: Side) -> Tile {
    let hash = roll(ground, 16, cell.first, i64::from(cell.row));
    let ore_roll = u16::try_from(hash % 10_000).unwrap_or(0);
    let mut threshold = 0_u16;
    for (mineral, chance) in side_ores(cell.stratum.min(3), side) {
        threshold += *chance;
        if ore_roll < threshold {
            return tile(TileKind::Ore(*mineral));
        }
    }
    let patch = value_noise2(ground, 17, cell.first, cell.row, 8, 4);
    let (rock, accent) = match (cell.stratum, side) {
        (0, Side::Day) => (
            ExoticTile::Hardpan,
            if patch > 760 {
                ExoticTile::SunGlass
            } else {
                ExoticTile::MesaStone
            },
        ),
        (0, Side::Twilight) => (ExoticTile::MesaStone, ExoticTile::FrostRegolith),
        (0, Side::Night) => (ExoticTile::NitrogenIce, ExoticTile::FrostRegolith),
        (1, Side::Night) => (ExoticTile::CryoRock, ExoticTile::NitrogenIce),
        (1, _) => (ExoticTile::VentRock, ExoticTile::Hardpan),
        (2, _) => (ExoticTile::FractureRock, ExoticTile::FaultRubble),
        _ => (ExoticTile::Lodestone, ExoticTile::FractureRock),
    };
    if patch > 700 && patch <= 760 || patch > 820 {
        return exotic(accent);
    }
    exotic(rock)
}

/// The Gloam Gate's tile at `dx` from its middle, `up` rows above its floor (negative: below).
fn gate_tile(dx: i64, up: i32) -> Option<Tile> {
    if up <= 0 {
        return None;
    }
    // The seal at the arena's middle, on the floor.
    if dx == 0 && up == 1 {
        return Some(exotic(ExoticTile::BossSeal));
    }
    let pillar = (dx.abs() - GATE_PILLAR).abs() <= 1;
    // The sunward pillar (toward column 0, rising columns from the morning terminator) is
    // scorched glass and mesa stone; the nightward one frozen.
    let (sun, night) = if dx > 0 {
        (ExoticTile::SunGlass, ExoticTile::MesaStone)
    } else {
        (ExoticTile::NitrogenIce, ExoticTile::IceSpike)
    };
    if pillar && up <= GATE_HEIGHT {
        return Some(exotic(if up % 5 == 0 { night } else { sun }));
    }
    // The arch: a half ellipse over the pillars, three rows thick.
    let a = GATE_PILLAR + 1;
    let b = 8_i64;
    let rise = i64::from(up - GATE_HEIGHT);
    if (0..=b + 2).contains(&rise) && dx.abs() <= a {
        let outer = dx * dx * (b + 2) * (b + 2) + rise * rise * (a + 1) * (a + 1)
            <= (a + 1) * (a + 1) * (b + 2) * (b + 2);
        let inner = dx * dx * b * b + rise * rise * (a - 2) * (a - 2) <= (a - 2) * (a - 2) * b * b;
        if outer && !inner {
            // The keystone holds the relic.
            if dx == 0 && rise > b - 1 {
                return Some(exotic(ExoticTile::LandmarkRelic));
            }
            return Some(exotic(if dx > 0 { sun } else { night }));
        }
    }
    None
}

/// The builder outpost of a locked world (see [`super::builder_outpost`]).
#[must_use]
pub fn outpost(ground: &PlanetGround) -> super::BuilderOutpost {
    let column = observatory_column(ground);
    let floor = surface(ground, column);
    let column = i32::try_from(column).unwrap_or(0);
    super::BuilderOutpost {
        style: super::ExoticStyle::Locked,
        column,
        floor,
        beacon: TilePosition {
            x: column,
            y: floor - 1,
        },
    }
}

impl ThemeWorld for LockedWorld {
    fn theme(&self) -> crate::planet::PlanetThemeId {
        super::LOCKED_WORLD
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
        let middle = i64::from(origin.x) + 8;
        let gate = gate_column(ground);
        let near_gate = wrap_delta(middle, gate, width).abs() <= GATE_HALF + 16;
        let gate_floor = surface(ground, gate);
        let observatory = observatory_column(ground);
        let near_observatory = wrap_delta(middle, observatory, width).abs() <= 32;
        let observatory_floor = surface(ground, observatory);
        let spike_cell = i64::from(origin.x).div_euclid(24) - 1;
        let spikes: Vec<(i64, i32)> = (spike_cell..=spike_cell + 2)
            .filter_map(|cell| spike_in(ground, cell))
            .collect();
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                let side = side_of(ground, cell.first);
                if near_gate
                    && cell.span == 1
                    && let Some(tile) =
                        gate_tile(wrap_delta(cell.first, gate, width), gate_floor - cell.row)
                {
                    return tile;
                }
                if near_observatory
                    && let Some(tile) = outpost_room_tile(
                        wrap_delta(cell.first, observatory, width),
                        cell.row,
                        observatory_floor,
                    )
                {
                    return tile;
                }
                if let Some(tile) = secret_tile(
                    ground,
                    cell.first,
                    cell.row,
                    cell.surface,
                    exotic(ExoticTile::MesaStone),
                    exotic(ExoticTile::IceSpike),
                ) {
                    return tile;
                }
                if cell.row < cell.surface {
                    if side == Side::Twilight && in_pool(ground, cell.first, cell.row, cell.surface)
                    {
                        return Tile {
                            kind: TileKind::Fluid(FluidKind::Water),
                            durability: FLUID_FULL,
                        };
                    }
                    if cell.span == 1 {
                        for &(column, height) in &spikes {
                            let dx = wrap_delta(cell.first, column, width);
                            let up = cell.surface - cell.row;
                            if up > 0 && up <= height && dx.abs() <= i64::from(up < height / 2) {
                                return exotic(ExoticTile::IceSpike);
                            }
                        }
                    }
                    return cell.base;
                }
                // The surface's top tile: salt in the pans, glowmoss in the twilight.
                if cell.depth == 0 {
                    match side {
                        Side::Twilight => return exotic(ExoticTile::Glowmoss),
                        Side::Day if in_pan(ground, cell.first) => {
                            return exotic(ExoticTile::SaltCrust);
                        }
                        _ => {}
                    }
                }
                if side == Side::Twilight && cell.depth < 3 {
                    return exotic(ExoticTile::LichenSoil);
                }
                // The caves of the generic pass stay open.
                if cell.base.kind == TileKind::Air {
                    return cell.base;
                }
                match cell.stratum {
                    1 => {
                        if let Some(tile) = vent_tile(ground, cell.first, cell.row, side) {
                            return tile;
                        }
                    }
                    2 => {
                        if let Some(tile) = fracture_tile(ground, cell.first, cell.row, cell.span) {
                            return tile;
                        }
                    }
                    _ => {}
                }
                stratum_tile(ground, cell, side)
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let centre = settlement_centre(ground);
        let half = settlement_half(ground);
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(centre - half).unwrap_or(0),
            width: i32::try_from(half * 2).unwrap_or(48),
            floor: surface(ground, centre),
        })
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let centre = gate_column(ground);
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(centre - GATE_HALF).unwrap_or(0),
            width: i32::try_from(GATE_HALF * 2).unwrap_or(80),
            floor: surface(ground, centre),
        })
    }
}
