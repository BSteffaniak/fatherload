//! Descent VIII's generated places (Wave 10, V11 only; the rules live in [`crate::heart`]).
//!
//! One pass in `streamed::assemble` after the far Strange Layers' (pure functions of the
//! settings and the tile, so every chunk agrees in any order):
//!
//! * **Dark Matter Veins**: veins of dark matter in the shale, **weightless halls** with dark
//!   matter hanging from their roofs (it falls up), and **the Upfall**, a tall shaft whose roof
//!   is crusted with it, a builder hauler stuck wheels-up there.
//! * **Singularity Fields**: **gravity wells** ([`Well`]): round caverns with a singularity at
//!   the middle and event glass round the rim, on a seeded grid, bigger deeper; **the Lens**,
//!   the greatest of them.
//! * **Core Ocean**: the **metal sea** ([`ocean_rows`]): a band under a rock sky where liquid
//!   metal fills basins between rock pillars; the **magnetic rails**: an upper line across the
//!   whole sea (tunnelled through the pillars, boosters at each basin), a lower line in every
//!   basin reached by switches with a jump in it, pylons standing in the metal; ore islands;
//!   **the Ferry Terminal** at the cable.
//! * **The Heart**: **the builders' cable** running down from Heart's Edge to the Heart (only
//!   through rock: it hangs free over the open places), and **the Heart's chamber** round the
//!   Star Drive at the cable's root: a shell nothing breaks, the Heart Key's seals in its three
//!   doors, a bowl floor and ledges; star shards in the rock round it.
//!
//! The pass never carves the outposts or anomaly rooms.

use crate::{
    descent::{Descent, LandmarkKind, Stratum},
    fluid::FluidKind,
    heart::{CHAMBER_RADIUS, CoreTile, FLOOR_BELOW_TILES, SHELL_TILES},
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        descent::{DescentLayout, landmarks, stratum_in, variant},
        noise::{NOISE_MAX, hash2, value_noise},
    },
};

const SALT_VEIN: u64 = 0x4EA7_0001;
const SALT_HALL: u64 = 0x4EA7_0002;
const SALT_WELL: u64 = 0x4EA7_0003;
const SALT_OCEAN: u64 = 0x4EA7_0004;
const SALT_SHARD: u64 = 0x4EA7_0005;
const SALT_PLACE: u64 = 0x4EA7_0006;
const SALT_HANG: u64 = 0x4EA7_0007;
const SALT_GLASS: u64 = 0x4EA7_0008;

const fn core(tile: CoreTile) -> TileKind {
    TileKind::Core(tile)
}

/// Rows kept clear of a stratum's ragged edges by the set pieces.
const EDGE_ROWS: i32 = 34;

// ---------------------------------------------------------------------------------------------
// Dark Matter Veins.
// ---------------------------------------------------------------------------------------------

/// The weightless halls' grid cell (columns, rows).
pub const HALL_CELL: (i32, i32) = (92, 70);

/// A weightless hall: an open ellipse whose roof holds dark matter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hall {
    pub centre: TilePosition,
    pub half_width: i32,
    pub half_height: i32,
}

impl Hall {
    /// Its bounding box.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        TileRect::around(self.centre, self.half_width, self.half_height)
    }

    /// Whether `(x, y)` is inside the hall's air (an ellipse).
    #[must_use]
    pub const fn open(&self, x: i32, y: i32) -> bool {
        let dx = (x - self.centre.x) as i64;
        let dy = (y - self.centre.y) as i64;
        let w = self.half_width as i64;
        let h = self.half_height as i64;
        dx * dx * h * h + dy * dy * w * w <= w * w * h * h
    }
}

/// The weightless hall in grid cell `(cx, cy)`, if it holds one.
#[must_use]
pub fn hall_in_cell(
    settings: &WorldSettings,
    layout: &DescentLayout,
    cx: i32,
    cy: i32,
) -> Option<Hall> {
    let (top, bottom) = layout.rows(Stratum::DarkMatterVeins);
    let big = variant(settings, Stratum::DarkMatterVeins) == 1;
    let roll = hash2(settings.seed, SALT_HALL, cx, cy);
    if roll % 1_000 >= if big { 680 } else { 520 } {
        return None;
    }
    let x = cx * HALL_CELL.0 + 24 + i32::try_from((roll >> 10) % 44).unwrap_or(0);
    let y = top + cy * HALL_CELL.1 + 18 + i32::try_from((roll >> 20) % 34).unwrap_or(0);
    let half_width = 12 + i32::try_from((roll >> 30) % 9).unwrap_or(0) + if big { 6 } else { 0 };
    let half_height = 6 + i32::try_from((roll >> 36) % 4).unwrap_or(0) + if big { 2 } else { 0 };
    (y - half_height > top + EDGE_ROWS / 2 && y + half_height < bottom - EDGE_ROWS / 2).then_some(
        Hall {
            centre: TilePosition { x, y },
            half_width,
            half_height,
        },
    )
}

/// The weightless halls touching `rect`.
#[must_use]
pub fn halls_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rect: &TileRect,
) -> Vec<Hall> {
    let (top, bottom) = layout.rows(Stratum::DarkMatterVeins);
    if rect.max.y < top || rect.min.y > bottom {
        return Vec::new();
    }
    let reach = 34;
    let mut halls = Vec::new();
    let x0 = (rect.min.x - reach).div_euclid(HALL_CELL.0);
    let x1 = (rect.max.x + reach).div_euclid(HALL_CELL.0);
    let y0 = (rect.min.y - reach - top).div_euclid(HALL_CELL.1).max(0);
    let y1 = (rect.max.y + reach - top).div_euclid(HALL_CELL.1);
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            if let Some(hall) = hall_in_cell(settings, layout, cx, cy)
                && hall.bounds().expanded(3).intersects(rect)
            {
                halls.push(hall);
            }
        }
    }
    halls
}

/// The Upfall's shaft (Dark Matter Veins' landmark): beside the cable, the far side from the
/// outposts.
#[must_use]
pub fn upfall(settings: &WorldSettings, layout: &DescentLayout) -> TileRect {
    let (top, bottom) = layout.rows(Stratum::DarkMatterVeins);
    let side = cable_side(settings);
    let roll = hash2(settings.seed, SALT_PLACE, 1, 0);
    let x = landmarks::lift_column(settings) + side * (40 + i32::try_from(roll % 30).unwrap_or(0));
    let mid = top.midpoint(bottom);
    TileRect {
        min: TilePosition {
            x: x - 11,
            y: mid - 34,
        },
        max: TilePosition {
            x: x + 11,
            y: mid + 34,
        },
    }
}

/// The side of the cable (`1` east, `-1` west) away from the outposts' halls.
#[must_use]
pub const fn cable_side(settings: &WorldSettings) -> i32 {
    if landmarks::lift_column(settings) >= landmarks::outpost_column(settings) {
        1
    } else {
        -1
    }
}

fn veins_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    chunk: &HeartChunk,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    // The Upfall: open, its roof crusted with dark matter, the hauler stuck to it.
    if let Some(shaft) = chunk.upfall
        && shaft.expanded(3).contains(TilePosition { x, y })
    {
        let inside = shaft.contains(TilePosition { x, y });
        let roof = y < shaft.min.y;
        let centre = shaft.center().x;
        if inside {
            // The hauler: a plated block stuck to the roof, wheels up.
            if (x - centre).abs() <= 3 && y - shaft.min.y <= 2 {
                return Some(core(CoreTile::TerminalPlate));
            }
            return Some(TileKind::Air);
        }
        if roof && plain(base) {
            let hang = !hash2(settings.seed, SALT_HANG, x, y).is_multiple_of(3);
            return Some(core(if hang {
                CoreTile::DarkMatter
            } else {
                CoreTile::NullStone
            }));
        }
        return None;
    }
    for hall in &chunk.halls {
        if hall.open(x, y) {
            return Some(TileKind::Air);
        }
        // Dark matter clings to the roof (it falls up): the rock just over the ellipse.
        if plain(base)
            && y < hall.centre.y
            && (hall.open(x, y + 1) || hall.open(x, y + 2))
            && hash2(settings.seed, SALT_HANG, x, y) % 5 < 3
        {
            return Some(core(CoreTile::DarkMatter));
        }
    }
    if !plain(base) {
        return None;
    }
    // Veins: thin dark seams, denser deeper.
    let along = i32::from(layout.along(y));
    let n = value_noise(settings.seed, SALT_VEIN, x, y, 13, 7);
    let width = 900 + along * 900 / 1_000;
    ((n - NOISE_MAX / 2).abs() < width && !hash2(settings.seed, SALT_VEIN, x, y).is_multiple_of(4))
        .then_some(core(CoreTile::DarkMatter))
}

// ---------------------------------------------------------------------------------------------
// Singularity Fields.
// ---------------------------------------------------------------------------------------------

/// The wells' grid cell (columns, rows).
pub const WELL_CELL: (i32, i32) = (66, 62);
/// The Lens's radius, tiles.
pub const LENS_RADIUS: i32 = 17;

/// A gravity well: a round cavern round a singularity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Well {
    pub centre: TilePosition,
    /// The open cavern's radius, tiles (the pull reaches [`crate::heart::WELL_REACH_TILES`]
    /// past it).
    pub radius: i32,
}

impl Well {
    /// The pull's reach, tiles from the point.
    #[must_use]
    pub const fn reach(&self) -> i32 {
        self.radius + crate::heart::WELL_REACH_TILES
    }

    /// The box the pull covers.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        TileRect::around(self.centre, self.reach(), self.reach())
    }
}

/// The well in grid cell `(cx, cy)`, if it holds one.
#[must_use]
pub fn well_in_cell(
    settings: &WorldSettings,
    layout: &DescentLayout,
    cx: i32,
    cy: i32,
) -> Option<Well> {
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    let lens_variant = variant(settings, Stratum::SingularityFields) == 1;
    let roll = hash2(settings.seed, SALT_WELL, cx, cy);
    if roll % 1_000 >= if lens_variant { 480 } else { 600 } {
        return None;
    }
    let x = cx * WELL_CELL.0 + 18 + i32::try_from((roll >> 10) % 30).unwrap_or(0);
    let y = top + cy * WELL_CELL.1 + 18 + i32::try_from((roll >> 20) % 26).unwrap_or(0);
    let along = i32::from(layout.along(y.clamp(top, bottom)));
    let radius = 7 + i32::try_from((roll >> 30) % 4).unwrap_or(0) + along / 300;
    let radius = if lens_variant { radius + 2 } else { radius };
    let centre = TilePosition { x, y };
    let lens = lens(settings, layout);
    (y - radius > top + EDGE_ROWS / 2
        && y + radius < bottom - EDGE_ROWS / 2
        && (x - lens.centre.x).abs() > lens.reach() + radius + 4)
        .then_some(Well { centre, radius })
}

/// The Lens: the greatest well, on the far side of the cable from the outposts.
#[must_use]
pub fn lens(settings: &WorldSettings, layout: &DescentLayout) -> Well {
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    let roll = hash2(settings.seed, SALT_PLACE, 2, 0);
    let x = landmarks::lift_column(settings)
        + cable_side(settings) * (56 + i32::try_from(roll % 30).unwrap_or(0));
    Well {
        centre: TilePosition {
            x,
            y: top.midpoint(bottom),
        },
        radius: LENS_RADIUS,
    }
}

/// The wells (the Lens too) whose pull touches `rect`.
#[must_use]
pub fn wells_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rect: &TileRect,
) -> Vec<Well> {
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    if rect.max.y < top - 32 || rect.min.y > bottom + 32 {
        return Vec::new();
    }
    let reach = 40;
    let mut wells = Vec::new();
    let x0 = (rect.min.x - reach).div_euclid(WELL_CELL.0);
    let x1 = (rect.max.x + reach).div_euclid(WELL_CELL.0);
    let y0 = (rect.min.y - reach - top).div_euclid(WELL_CELL.1).max(0);
    let y1 = (rect.max.y + reach - top).div_euclid(WELL_CELL.1);
    for cy in y0..=y1 {
        for cx in x0..=x1 {
            if let Some(well) = well_in_cell(settings, layout, cx, cy)
                && well.bounds().intersects(rect)
            {
                wells.push(well);
            }
        }
    }
    let lens = lens(settings, layout);
    if lens.bounds().intersects(rect) {
        wells.push(lens);
    }
    wells
}

const fn well_tile(
    settings: &WorldSettings,
    well: &Well,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let (dx, dy) = (x - well.centre.x, y - well.centre.y);
    let d2 = dx * dx + dy * dy;
    if d2 == 0 {
        return Some(core(CoreTile::Singularity));
    }
    let r = well.radius;
    if d2 <= r * r {
        return Some(TileKind::Air);
    }
    let rim = (r + 2) * (r + 2);
    if d2 <= rim && plain(base) {
        // The rim: rings of event glass in horizon rock.
        let glass = hash2(settings.seed, SALT_GLASS, x, y) % 5 < 3;
        return Some(core(if glass {
            CoreTile::EventGlass
        } else {
            CoreTile::HorizonRock
        }));
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Core Ocean.
// ---------------------------------------------------------------------------------------------

/// Columns per basin of the metal sea (a pillar of rock at each one's west end).
pub const BASIN_PERIOD: i32 = 300;
/// Rows from the sea's surface up to the upper rail line.
pub const UPPER_RAIL_ABOVE_SEA: i32 = 8;
/// Rows from the sea's surface up to the lower rail line.
pub const LOWER_RAIL_ABOVE_SEA: i32 = 3;
/// Columns between rail pylons.
pub const PYLON_SPACING: i32 = 24;

/// The sea's rows in column `x`: `(ceiling, sea level, floor)`: open air from the ceiling down
/// to the sea level, liquid metal below it down to the floor (both inclusive).
#[must_use]
pub const fn ocean_rows(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
) -> (i32, i32, i32) {
    let (top, _) = layout.rows(Stratum::CoreOcean);
    let band = top + 44;
    let ceiling = band - value_noise(settings.seed, SALT_OCEAN, x, 0, 9, 1) * 5 / NOISE_MAX;
    let sea = band + 30;
    let floor = sea + 18 + value_noise(settings.seed, SALT_OCEAN ^ 1, x, 0, 11, 1) * 6 / NOISE_MAX;
    (ceiling, sea, floor)
}

/// The rail rows: `(upper, lower)`.
#[must_use]
pub const fn rail_rows(settings: &WorldSettings, layout: &DescentLayout) -> (i32, i32) {
    let (_, sea, _) = ocean_rows(settings, layout, 0);
    (sea - UPPER_RAIL_ABOVE_SEA, sea - LOWER_RAIL_ABOVE_SEA)
}

/// One basin's plan: the pillar at its west end, the lower line's junctions, its jump and its
/// ore island.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Basin {
    /// Its first column.
    pub start: i32,
    /// The pillar's width.
    pub pillar: i32,
    /// The lower line's west junction (on the upper line) and east junction.
    pub junctions: (i32, i32),
    /// The jump's first open column (3 wide); the line beyond it is one row lower.
    pub gap: i32,
    /// The ore island's first column (8 wide).
    pub island: i32,
}

/// The basin holding column `x`.
#[must_use]
pub fn basin_at(settings: &WorldSettings, x: i32) -> Basin {
    let index = x.div_euclid(BASIN_PERIOD);
    let start = index * BASIN_PERIOD;
    let roll = hash2(settings.seed, SALT_OCEAN ^ 7, index, 0);
    let pillar = 16 + i32::try_from(roll % 9).unwrap_or(0);
    let west = start + pillar + 34 + i32::try_from((roll >> 8) % 16).unwrap_or(0);
    let east = start + BASIN_PERIOD - 44 - i32::try_from((roll >> 16) % 16).unwrap_or(0);
    let gap = west.midpoint(east) + i32::try_from((roll >> 24) % 21).unwrap_or(0) - 10;
    Basin {
        start,
        pillar,
        junctions: (west, east),
        gap,
        island: east + 14,
    }
}

/// Whether `(x, y)` is a rail of the lower line (ramps included) in `basin`, and which kind.
fn lower_rail(basin: &Basin, upper: i32, x: i32, y: i32) -> Option<CoreTile> {
    let (west, east) = basin.junctions;
    let lower = upper + (UPPER_RAIL_ABOVE_SEA - LOWER_RAIL_ABOVE_SEA);
    let drop = lower - upper;
    // Down ramp from the west junction.
    let k = x - west;
    if (1..=drop).contains(&k) && y == upper + k {
        return Some(CoreTile::MagRail);
    }
    // The first run, a booster before the jump.
    if x > west + drop && x < basin.gap && y == lower {
        return Some(if x >= basin.gap - 4 {
            CoreTile::MagBooster
        } else {
            CoreTile::MagRail
        });
    }
    // Past the jump, one row lower, up to the up ramp.
    let rise = drop + 1;
    if x >= basin.gap + 3 && x <= east - rise && y == lower + 1 {
        return Some(CoreTile::MagRail);
    }
    // The up ramp back to the east junction.
    let k = x - (east - rise);
    if (1..rise).contains(&k) && y == lower + 1 - k {
        return Some(CoreTile::MagRail);
    }
    None
}

/// The Ferry Terminal (Core Ocean's landmark): a station over the sea at the cable.
#[must_use]
pub const fn ferry_terminal(settings: &WorldSettings, layout: &DescentLayout) -> TileRect {
    let (upper, _) = rail_rows(settings, layout);
    let column = landmarks::lift_column(settings);
    TileRect {
        min: TilePosition {
            x: column - 15,
            y: upper - 7,
        },
        max: TilePosition {
            x: column + 15,
            y: upper + 1,
        },
    }
}

fn ocean_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    chunk: &HeartChunk,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let (ceiling, sea, floor) = ocean_rows(settings, layout, x);
    if y < ceiling - 1 || y > floor + 1 {
        return None;
    }
    let (upper, lower) = rail_rows(settings, layout);
    let basin = basin_at(settings, x);
    let p = x - basin.start;
    // The Ferry Terminal: a platform under the rails, a roof on posts.
    if let Some(terminal) = chunk.terminal
        && terminal.contains(TilePosition { x, y })
    {
        if y == upper {
            return Some(core(CoreTile::MagRail));
        }
        if y == terminal.max.y || (y == terminal.min.y && (x - terminal.center().x).abs() <= 12) {
            return Some(core(CoreTile::TerminalPlate));
        }
        if (x - terminal.min.x == 2 || terminal.max.x - x == 2) && y > terminal.min.y {
            return Some(core(CoreTile::TerminalPlate));
        }
        return Some(TileKind::Air);
    }
    // The rock sky over the sea and the bed under it.
    if y < ceiling || y > floor {
        return None;
    }
    // The upper line: across everything, boosters early in each basin.
    if y == upper {
        let boost = (basin.pillar + 6..basin.pillar + 9).contains(&p);
        let junction = x == basin.junctions.0 || x == basin.junctions.1;
        return Some(core(if junction {
            CoreTile::MagSwitch
        } else if boost {
            CoreTile::MagBooster
        } else {
            CoreTile::MagRail
        }));
    }
    if p < basin.pillar {
        // The pillar: solid, tunnelled for the upper line, with core metal in its skin.
        if y < upper && y >= upper - 2 {
            return Some(TileKind::Air);
        }
        let edge = p == 0 || p == basin.pillar - 1 || y == ceiling;
        if edge && hash2(settings.seed, SALT_OCEAN ^ 3, x, y).is_multiple_of(3) {
            return Some(core(CoreTile::CoreMetal));
        }
        return Some(core(if (p + y) % 7 == 0 {
            CoreTile::IronCrust
        } else {
            CoreTile::CoreBasalt
        }));
    }
    if let Some(rail) = lower_rail(&basin, upper, x, y) {
        return Some(core(rail));
    }
    // The ore island near the east end: rock from the bed to two rows over the sea.
    if x >= basin.island && x < basin.island + 8 && y >= sea - 1 {
        let top = y <= sea;
        return Some(core(if top {
            CoreTile::CoreMetal
        } else {
            CoreTile::CoreBasalt
        }));
    }
    // Pylons: under the upper line down to the bed, arched over the lower line.
    if p % PYLON_SPACING == PYLON_SPACING / 2 && y > upper {
        // Clear of the ramps and the lower line, and a row of head room over them.
        let (west, east) = basin.junctions;
        let over_lower = x >= west - 1 && x <= east + 1 && y <= lower + 3;
        if !over_lower {
            return Some(core(CoreTile::RailPylon));
        }
    }
    let _ = base;
    if y > sea {
        return Some(TileKind::Fluid(FluidKind::LiquidMetal));
    }
    Some(TileKind::Air)
}

// ---------------------------------------------------------------------------------------------
// The Heart.
// ---------------------------------------------------------------------------------------------

/// The Heart chamber's middle: at the builders' cable's root, near the world's bottom.
#[must_use]
pub const fn heart_centre(settings: &WorldSettings, layout: &DescentLayout) -> TilePosition {
    let (_, bottom) = layout.rows(Stratum::TheHeart);
    TilePosition {
        x: landmarks::lift_column(settings),
        y: bottom - CHAMBER_RADIUS - SHELL_TILES - 10,
    }
}

/// The Heart chamber's box, shell included.
#[must_use]
pub const fn heart_chamber(settings: &WorldSettings, layout: &DescentLayout) -> TileRect {
    let reach = CHAMBER_RADIUS + SHELL_TILES;
    TileRect::around(heart_centre(settings, layout), reach, reach)
}

/// The rows the cable runs down from (Heart's Edge's lift station) to the Heart's shell.
#[must_use]
pub fn cable_rows(settings: &WorldSettings, layout: &DescentLayout) -> (i32, i32) {
    // Under the last lift station's floor (Heart's Edge's hall floor).
    let floor = landmarks::outpost(settings, layout, Descent::Heart).map_or_else(
        || layout.rows(Descent::Heart.first_stratum()).0 + 40,
        |hall| hall.bounds.max.y,
    );
    let centre = heart_centre(settings, layout);
    (floor + 1, centre.y - CHAMBER_RADIUS - SHELL_TILES - 1)
}

/// The Heart chamber's ledges `(centre column, row)` (five wide).
#[must_use]
pub const fn ledges(centre: TilePosition) -> [(i32, i32); 4] {
    [
        (centre.x - 15, centre.y + 4),
        (centre.x + 15, centre.y + 4),
        (centre.x - 9, centre.y - 9),
        (centre.x + 9, centre.y - 9),
    ]
}

/// Whether `(x, y)` is in one of the chamber's three doorways (the Heart Key's seals): over
/// the top, where the cable comes in, and both sides at the floor.
#[must_use]
pub const fn in_doorway(centre: TilePosition, x: i32, y: i32) -> bool {
    let (dx, dy) = (x - centre.x, y - centre.y);
    let top = dx.abs() <= 2 && dy < 0;
    let side = dy >= FLOOR_BELOW_TILES - 4 && dy < FLOOR_BELOW_TILES;
    top || side
}

/// The Heart chamber's tile at `(x, y)`, `None` outside it.
#[must_use]
pub const fn chamber_tile(centre: TilePosition, x: i32, y: i32) -> Option<TileKind> {
    let (dx, dy) = (x - centre.x, y - centre.y);
    let d2 = dx * dx + dy * dy;
    let outer = CHAMBER_RADIUS + SHELL_TILES;
    if d2 > outer * outer {
        return None;
    }
    if d2 > CHAMBER_RADIUS * CHAMBER_RADIUS {
        return Some(if in_doorway(centre, x, y) {
            core(CoreTile::HeartSeal)
        } else {
            core(CoreTile::HeartShell)
        });
    }
    // The Star Drive's body is no rock: it is the fight's core (an entity weapons hit), drawn
    // over the open middle.
    if dy > FLOOR_BELOW_TILES {
        return Some(core(CoreTile::HeartShell));
    }
    let ledges = ledges(centre);
    let mut index = 0;
    while index < ledges.len() {
        let (lx, ly) = ledges[index];
        if y == ly && (x - lx).abs() <= 2 {
            return Some(core(CoreTile::HeartShell));
        }
        index += 1;
    }
    Some(TileKind::Air)
}

fn heart_tile(
    settings: &WorldSettings,
    chunk: &HeartChunk,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !plain(base) {
        return None;
    }
    // Star shards: thick round the chamber, scattered elsewhere.
    let near = chunk.heart.is_some_and(|centre| {
        let (dx, dy) = (x - centre.x, y - centre.y);
        dx * dx + dy * dy <= (CHAMBER_RADIUS + 14) * (CHAMBER_RADIUS + 14)
    });
    let odds = if near { 9 } else { 40 };
    hash2(settings.seed, SALT_SHARD, x, y)
        .is_multiple_of(odds)
        .then_some(core(CoreTile::StarShard))
}

// ---------------------------------------------------------------------------------------------
// The pass.
// ---------------------------------------------------------------------------------------------

/// What one chunk's pass needs (computed once per chunk).
#[derive(Clone, Debug, Default)]
pub struct HeartChunk {
    pub halls: Vec<Hall>,
    pub upfall: Option<TileRect>,
    pub wells: Vec<Well>,
    pub terminal: Option<TileRect>,
    pub heart: Option<TilePosition>,
    pub cable: Option<(i32, i32, i32)>,
    /// Tiles the pass never carves (the outposts and anomaly rooms).
    pub keep: Vec<TileRect>,
}

/// The rows the pass touches: Descent VIII's, with a margin.
#[must_use]
pub const fn pass_rows(layout: &DescentLayout) -> (i32, i32) {
    let (first, last) = layout.descent_rows(Descent::Heart);
    (first - 4, last + 8)
}

/// The pass's view of chunk `rect` (empty outside Descent VIII).
#[must_use]
pub fn chunk_context(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rect: &TileRect,
) -> HeartChunk {
    let mut context = HeartChunk::default();
    if !crate::heart::has_heart(settings) {
        return context;
    }
    context.halls = halls_touching(settings, layout, rect);
    context.upfall =
        Some(upfall(settings, layout)).filter(|shaft| shaft.expanded(4).intersects(rect));
    context.wells = wells_touching(settings, layout, rect);
    context.terminal =
        Some(ferry_terminal(settings, layout)).filter(|terminal| terminal.intersects(rect));
    context.heart = Some(heart_centre(settings, layout))
        .filter(|_| heart_chamber(settings, layout).intersects(rect));
    let (cable_top, cable_bottom) = cable_rows(settings, layout);
    let column = landmarks::lift_column(settings);
    if column >= rect.min.x
        && column <= rect.max.x
        && rect.max.y >= cable_top
        && rect.min.y <= cable_bottom
    {
        context.cable = Some((column, cable_top, cable_bottom));
    }
    for descent in [Descent::Ancients, Descent::Heart] {
        if let Some(outpost) = landmarks::outpost(settings, layout, descent) {
            context.keep.push(outpost.bounds.expanded(1));
        }
    }
    for landmark in landmarks::landmarks_with(settings, layout, &rect.expanded(4)) {
        if landmark.kind == LandmarkKind::AnomalyRoom || landmark.kind == LandmarkKind::Outpost {
            context.keep.push(landmark.bounds.expanded(1));
        }
    }
    context
}

/// Whether the pass may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Poi(_) | TileKind::Bedrock | TileKind::Fluid(_)
    )
}

/// Plain rock (and what stands in for it) the pass may turn into its own materials.
const fn plain(kind: TileKind) -> bool {
    match kind {
        TileKind::Dirt
        | TileKind::Clay
        | TileKind::Stone
        | TileKind::HardRock
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => true,
        TileKind::Core(tile) => tile.plain_rock(),
        _ => false,
    }
}

/// Descent VIII's tile at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn heart_pass_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    chunk: &HeartChunk,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if chunk.keep.iter().any(|keep| keep.contains(at)) {
        return None;
    }
    // The Heart's chamber over everything (it stands at the world's bottom).
    if let Some(centre) = chunk.heart
        && let Some(kind) = chamber_tile(centre, x, y)
    {
        return Some(kind);
    }
    if !carvable(base) {
        return None;
    }
    let stratum = stratum_in(settings, layout, x, y);
    let carved = match stratum {
        Stratum::DarkMatterVeins => veins_tile(settings, layout, chunk, x, y, base),
        Stratum::SingularityFields => chunk
            .wells
            .iter()
            .find_map(|well| well_tile(settings, well, x, y, base)),
        Stratum::CoreOcean => ocean_tile(settings, layout, chunk, x, y, base),
        Stratum::TheHeart => heart_tile(settings, chunk, x, y, base),
        _ => None,
    };
    // The cable: through rock only (it hangs free over the open places).
    if let Some((column, top, bottom)) = chunk.cable
        && x == column
        && y >= top
        && y <= bottom
    {
        let kind = carved.unwrap_or(base);
        if !kind.is_open() && !matches!(kind, TileKind::Core(tile) if tile.is_mag_rail()) {
            return Some(core(CoreTile::Cable));
        }
    }
    carved
}

/// Whether `area` crosses one of Descent VIII's set pieces or open places (the story's camps
/// and rooms keep off them).
#[must_use]
pub fn crosses_set_pieces(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    if !crate::heart::has_heart(settings) {
        return false;
    }
    let (first, last) = pass_rows(layout);
    if area.max.y < first || area.min.y > last {
        return false;
    }
    if upfall(settings, layout).expanded(4).intersects(area)
        || lens(settings, layout).bounds().intersects(area)
        || ferry_terminal(settings, layout)
            .expanded(4)
            .intersects(area)
        || heart_chamber(settings, layout).expanded(4).intersects(area)
        || !halls_touching(settings, layout, &area.expanded(3)).is_empty()
        || wells_touching(settings, layout, area).iter().any(|well| {
            TileRect::around(well.centre, well.radius + 4, well.radius + 4).intersects(area)
        })
    {
        return true;
    }
    // The metal sea's band, anywhere along it.
    let (ceiling, _, floor) = ocean_rows(settings, layout, area.center().x);
    area.max.y >= ceiling - 6 && area.min.y <= floor + 4
}

#[cfg(test)]
mod tests;
