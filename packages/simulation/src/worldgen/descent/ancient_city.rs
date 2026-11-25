//! The builders' buried city's generated places (Wave 10 ancients-a; Descent VII strata 1-3,
//! [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11) only).
//!
//! Pure functions of the settings and the tile (so chunks carve the same in any order), run
//! by `streamed::assemble` after the far Strange Layers' pass and before the expedition's
//! story pass. [`plan_for`] gathers what touches a chunk once; [`city_tile`] answers per tile.
//!
//! * **Buried Megacity**: **avenues** on fixed levels ([`avenue_levels`]) in segments of
//!   [`SEGMENT`] columns, paved, lamp-lit, many with a **tram line** (rail, platforms at
//!   stations; the "Tram Avenues" variant has more, "Fallen Towers" more gaps and rubble);
//!   **towers** between one avenue and the next, glass facades on builder alloy frames, floors
//!   every six rows and a **lift** shaft from the upper street through every floor to the
//!   lower street; the **Lantern Library** ([`lantern_library`]), the archivist's tower beside
//!   the builders' cable; **alloy portals** framing the way in under the alloy seal.
//! * **Engine Halls**: **engine halls** (piston staircases up to a cache, belts, crushers over
//!   them), **piston shafts** (a zig-zag of pistons and ledges to ride up; "Piston Shafts"
//!   leans to them), **crusher lines** (low corridors of crushers over belts; "Crusher Lines")
//!   and the **Great Engine** ([`great_engine`]), every machine on its hall's beat
//!   ([`crate::ancient_city::engine_beat_millis`]).
//! * **The Archive of Stars**: **stacks** (shelf halls with mezzanines), **puzzle vaults**
//!   ([`VaultKind`]: plate, crawlway lever, coloured light, grand) built of the builders'
//!   unbreakable masonry and mechanisms, a star chart's plinth inside each, and the
//!   **Orrery** ([`orrery`]) with three free charts.
//! * **Rock**: alloy ore veins, pipes and engine core ore, star glass specks.
//!
//! The movers (trams, lifts, pistons, crushers) come from the same stamps ([`movers_with`]).

use crate::{
    ancient_city::{
        CityTile,
        archivist::VaultKind,
        engine_beat_millis,
        movers::{
            CityMover, Cycle, LIFT_DWELL_MILLIS, LIFT_SPEED, MILLI, MoverKind, MoverRef,
            TRAM_DWELL_MILLIS, TRAM_SPEED, Track, track_period,
        },
    },
    base::BuiltTile,
    cave::CaveTile,
    descent::Stratum,
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, ridge, value_noise},
    },
};

use super::{DescentLayout, landmarks, stratum_in, variant};

/// Columns per avenue segment.
pub const SEGMENT: i32 = 120;
/// Open rows of an avenue (its street to its ceiling).
pub const AVENUE_ROWS: i32 = 7;
/// Columns per tower cell (at most one tower in each, between two avenue levels).
pub const TOWER_CELL: i32 = 44;
/// Rows between a tower's floors.
pub const TOWER_FLOOR_ROWS: i32 = 6;
/// The cells engine set pieces are rolled per, `(width, height)` in tiles.
pub const ENGINE_REGION: (i32, i32) = (64, 48);
/// The cells archive set pieces are rolled per.
pub const ARCHIVE_REGION: (i32, i32) = (60, 40);
/// A piston's stroke (rows).
pub const PISTON_STROKE: i32 = 6;
/// Columns between the alloy portals under the seal.
pub const PORTAL_SPACING: i32 = 56;
/// `(width, height)` of the Great Engine's hall.
pub const GREAT_ENGINE_SIZE: (i32, i32) = (76, 30);
/// `(width, height)` of the Orrery.
pub const ORRERY_SIZE: (i32, i32) = (64, 28);

const SALT_LEVEL: u64 = 0xA4C1_0001;
const SALT_AVENUE: u64 = 0xA4C1_0002;
const SALT_TOWER: u64 = 0xA4C1_0003;
const SALT_REGION: u64 = 0xA4C1_0004;
const SALT_FIXED: u64 = 0xA4C1_0005;
const SALT_DECOR: u64 = 0xA4C1_0006;
const SALT_ROCK: u64 = 0xA4C1_0007;
const SALT_PHASE: u64 = 0xA4C1_0008;

/// What a city stamp is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CityStampKind {
    /// An avenue segment (Buried Megacity).
    Avenue,
    /// A tower between two avenue levels (Buried Megacity).
    Tower,
    /// The Lantern Library (one per world).
    LanternLibrary,
    /// An engine hall (Engine Halls).
    EngineHall,
    /// A piston shaft.
    PistonShaft,
    /// A crusher line.
    CrusherLine,
    /// The Great Engine (one per world).
    GreatEngine,
    /// Archive stacks (The Archive of Stars).
    Stacks,
    /// A puzzle vault.
    Vault(VaultKind),
    /// The Orrery (one per world).
    Orrery,
}

/// One city set piece.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CityStamp {
    pub kind: CityStampKind,
    /// Every tile it may change.
    pub bounds: TileRect,
    /// Per-stamp variation.
    pub seed: u64,
    /// How deep in its stratum it is, per mille.
    pub along: u16,
    /// Mirrored left to right.
    pub mirrored: bool,
    /// Avenues: the street's floor row. Towers: the upper avenue's floor row.
    pub floor: i32,
    /// Towers: the lower avenue's floor row.
    pub lower: i32,
    /// Towers: the lift shaft's centre column.
    pub shaft: i32,
    /// Avenues: whether a tram runs on it.
    pub tram: bool,
}

impl CityStamp {
    const fn plain(kind: CityStampKind, bounds: TileRect, seed: u64, along: u16) -> Self {
        Self {
            kind,
            bounds,
            seed,
            along,
            mirrored: seed & 0x100 != 0,
            floor: 0,
            lower: 0,
            shaft: 0,
            tram: false,
        }
    }

    /// Its column `x` read in the unmirrored layout.
    const fn local_x(&self, x: i32) -> i32 {
        if self.mirrored {
            self.bounds.max.x - x
        } else {
            x - self.bounds.min.x
        }
    }

    /// The world column of unmirrored layout column `lx`.
    const fn world_x(&self, lx: i32) -> i32 {
        if self.mirrored {
            self.bounds.max.x - lx
        } else {
            self.bounds.min.x + lx
        }
    }
}

const fn rect(x: i32, y: i32, (width, height): (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + width - 1,
            y: y + height - 1,
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "small bounded rolls"
)]
const fn roll_range(roll: u64, shift: u32, low: i32, high: i32) -> i32 {
    if high <= low {
        return low;
    }
    low + ((roll >> shift) % ((high - low + 1) as u64)) as i32
}

/// Whether `stratum` is one of the buried city's.
#[must_use]
pub const fn is_city(stratum: Stratum) -> bool {
    matches!(
        stratum,
        Stratum::BuriedMegacity | Stratum::EngineHalls | Stratum::ArchiveOfStars
    )
}

/// The rows the buried city's pass touches (with slack for the ragged boundaries and the
/// portals under the alloy seal).
#[must_use]
pub const fn carve_rows(layout: &DescentLayout) -> (i32, i32) {
    let index = Stratum::PrismDesert.index();
    let (_, deepest) = super::boundary_depth_range(layout, index);
    let top = layout.rows(Stratum::BuriedMegacity).0 - 60;
    let top = if deepest - 60 < top {
        deepest - 60
    } else {
        top
    };
    (top, layout.rows(Stratum::ArchiveOfStars).1 + 20)
}

/// The columns the city keeps clear for the builders' cable and the outposts' halls.
#[must_use]
pub const fn clear_columns(settings: &WorldSettings) -> (i32, i32) {
    let outpost = landmarks::outpost_column(settings);
    let lift = landmarks::lift_column(settings);
    let half = landmarks::OUTPOST_SIZE.0 / 2 + 4;
    let low = if outpost - half < lift - 3 {
        outpost - half
    } else {
        lift - 3
    };
    let high = if outpost + half > lift + 3 {
        outpost + half
    } else {
        lift + 3
    };
    (low, high)
}

/// The side of the cable a fixed set piece sits on (towards the spawn: where diggers come
/// down).
const fn spawn_side(settings: &WorldSettings) -> i32 {
    if landmarks::outpost_column(settings) >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    }
}

// --- The Megacity -------------------------------------------------------------------------------

/// The floor rows of the Megacity's avenue levels, top to bottom ("Tram Avenues" packs them a
/// little closer than "Fallen Towers").
#[must_use]
pub fn avenue_levels(settings: &WorldSettings, layout: &DescentLayout) -> Vec<i32> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let prism = Stratum::PrismDesert.index();
    let (_, half) = super::boundary_shape(prism);
    let (_, seal_deepest) = super::boundary_depth_range(layout, prism);
    let first = seal_deepest + half + crate::worldgen::descent::far_strange::ALLOY_SEAL_ROWS + 14;
    let below = Stratum::BuriedMegacity.index();
    let (last_ok, _) = super::boundary_depth_range(layout, below);
    let spacing = if variant(settings, Stratum::BuriedMegacity) == 0 {
        38
    } else {
        44
    };
    let mut rows = Vec::new();
    let mut row = first;
    let mut index = 0;
    while row + 2 < last_ok - 6 {
        let jitter = roll_range(hash2(settings.seed, SALT_LEVEL, index, 0), 0, -3, 3);
        rows.push(row + jitter);
        row += spacing;
        index += 1;
    }
    rows
}

/// The avenue of level `level` (floor row `floor`) in segment `segment`, if it has one.
fn avenue(
    settings: &WorldSettings,
    layout: &DescentLayout,
    level: usize,
    floor: i32,
    segment: i32,
) -> Option<CityStamp> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "a dozen levels"
    )]
    let roll = hash2(settings.seed, SALT_AVENUE, segment, level as i32);
    let along = layout.along(floor);
    let fallen = variant(settings, Stratum::BuriedMegacity) == 1;
    let chance = 900 - u64::from(along) * 3 / 10 - if fallen { 140 } else { 0 };
    if roll % 1_000 >= chance {
        return None;
    }
    let mut x0 = segment * SEGMENT + roll_range(roll, 10, 2, 22);
    let mut x1 = (segment + 1) * SEGMENT - 1 - roll_range(roll, 20, 2, 22);
    let (low, high) = clear_columns(settings);
    if x1 >= low - 6 && x0 <= high + 6 {
        // Split round the cable and keep the longer side.
        let left = (low - 7) - x0;
        let right = x1 - (high + 7);
        if left >= right {
            x1 = low - 7;
        } else {
            x0 = high + 7;
        }
    }
    if x1 - x0 < 30 {
        return None;
    }
    let tram_chance = if fallen { 480 } else { 760 };
    let bounds = TileRect {
        min: TilePosition {
            x: x0,
            y: floor - AVENUE_ROWS - 1,
        },
        max: TilePosition {
            x: x1,
            y: floor + 1,
        },
    };
    // No tram through a pilot space's masonry (its line would be blocked).
    let tram = x1 - x0 >= 56
        && (roll >> 30) % 1_000 < tram_chance
        && !touches_pilot_space(settings, &bounds);
    Some(CityStamp {
        floor,
        tram,
        ..CityStamp::plain(CityStampKind::Avenue, bounds, roll, along)
    })
}

/// A tram line's stations (columns of the car's centre), from its avenue.
#[must_use]
pub fn stations(avenue: &CityStamp) -> Vec<i32> {
    let first = avenue.bounds.min.x + 5;
    let last = avenue.bounds.max.x - 5;
    let count = ((last - first) / 32).max(1);
    let spacing = (last - first) / count;
    (0..=count).map(|index| first + index * spacing).collect()
}

/// The avenue of `level` covering column `x`, if any.
fn avenue_at(
    settings: &WorldSettings,
    layout: &DescentLayout,
    levels: &[i32],
    level: usize,
    x: i32,
) -> Option<CityStamp> {
    let floor = *levels.get(level)?;
    avenue(settings, layout, level, floor, x.div_euclid(SEGMENT))
        .filter(|stamp| stamp.bounds.min.x <= x && x <= stamp.bounds.max.x)
}

/// The tower between avenue levels `gap` and `gap + 1` in cell `cell`, if it has one.
fn tower(
    settings: &WorldSettings,
    layout: &DescentLayout,
    levels: &[i32],
    gap: usize,
    cell: i32,
) -> Option<CityStamp> {
    let (upper, lower) = (*levels.get(gap)?, *levels.get(gap + 1)?);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "a dozen levels"
    )]
    let roll = hash2(settings.seed, SALT_TOWER, cell, gap as i32);
    let fallen = variant(settings, Stratum::BuriedMegacity) == 1;
    if roll % 1_000 >= if fallen { 420 } else { 560 } {
        return None;
    }
    let width = roll_range(roll, 10, 12, 17);
    let x0 = cell * TOWER_CELL + roll_range(roll, 20, 2, TOWER_CELL - width - 2);
    let x1 = x0 + width - 1;
    let (low, high) = clear_columns(settings);
    if x1 >= low - 4 && x0 <= high + 4 {
        return None;
    }
    if touches_pilot_space(settings, &tower_bounds(x0, x1, upper, lower)) {
        return None;
    }
    // Both avenues must run past the tower.
    let above = avenue_at(settings, layout, levels, gap, x0 - 1)?;
    let below = avenue_at(settings, layout, levels, gap + 1, x0 - 1)?;
    if above.bounds.max.x < x1 + 1 || below.bounds.max.x < x1 + 1 {
        return None;
    }
    let shaft = if (roll >> 40).is_multiple_of(2) {
        x0 + 3
    } else {
        x1 - 3
    };
    // No shaft under a tram station's platform.
    if above.tram
        && stations(&above)
            .iter()
            .any(|station| (station - shaft).abs() <= 4)
    {
        return None;
    }
    Some(CityStamp {
        floor: upper,
        lower,
        shaft,
        ..CityStamp::plain(
            CityStampKind::Tower,
            tower_bounds(x0, x1, upper, lower),
            roll,
            layout.along(upper),
        )
    })
}

/// A tower's bounds: its body between the avenues plus its lift's room at both stops (the
/// cab's space over the upper street and down to the lower street).
const fn tower_bounds(x0: i32, x1: i32, upper: i32, lower: i32) -> TileRect {
    TileRect {
        min: TilePosition {
            x: x0,
            y: upper - LIFT_ROOM_ROWS,
        },
        max: TilePosition {
            x: x1,
            y: lower - 1,
        },
    }
}

/// Rows of room a lift cab needs over a stop (its height).
const LIFT_ROOM_ROWS: i32 = 3;

/// A tower's base slab (the lower avenue's ceiling).
const fn tower_base(stamp: &CityStamp) -> i32 {
    stamp.lower - AVENUE_ROWS - 1
}

/// The Lantern Library: the archivist's tower between the first two avenue levels, a short
/// way from the cable on the spawn's side.
#[must_use]
pub fn lantern_library(settings: &WorldSettings, layout: &DescentLayout) -> Option<CityStamp> {
    let levels = avenue_levels(settings, layout);
    let (upper, lower) = (*levels.first()?, *levels.get(1)?);
    let (low, high) = clear_columns(settings);
    let side = spawn_side(settings);
    let roll = hash2(settings.seed, SALT_FIXED, 1, 0);
    let width = 19;
    let x0 = if side < 0 {
        low - 10 - width - roll_range(roll, 0, 0, 12)
    } else {
        high + 10 + roll_range(roll, 0, 0, 12)
    };
    let x1 = x0 + width - 1;
    let shaft = if side < 0 { x1 - 3 } else { x0 + 3 };
    Some(CityStamp {
        floor: upper,
        lower,
        shaft,
        mirrored: false,
        ..CityStamp::plain(
            CityStampKind::LanternLibrary,
            tower_bounds(x0, x1, upper, lower),
            roll,
            layout.along(upper),
        )
    })
}

/// The tile of the archivist's core in the Lantern Library (on its top floor, mid-hall).
#[must_use]
pub const fn library_core(library: &CityStamp) -> TilePosition {
    let roof = library.floor + 2;
    let centre = i32::midpoint(library.bounds.min.x, library.bounds.max.x);
    let x = if (centre - library.shaft).abs() <= 2 {
        centre + 3
    } else {
        centre
    };
    TilePosition {
        x,
        y: roof + TOWER_FLOOR_ROWS - 1,
    }
}

/// Every archivist core of a world: the Lantern Library's and the Archive Post's (in the
/// Descent VII outpost hall's ceiling).
#[must_use]
pub fn archivist_cores(settings: &WorldSettings, layout: &DescentLayout) -> Vec<TilePosition> {
    let mut cores = Vec::new();
    if let Some(library) = lantern_library(settings, layout) {
        cores.push(library_core(&library));
    }
    if let Some(post) = post_core(settings, layout) {
        cores.push(post);
    }
    cores
}

/// The Archive Post's core: set in the ceiling of Descent VII's outpost hall, over its middle.
#[must_use]
pub fn post_core(settings: &WorldSettings, layout: &DescentLayout) -> Option<TilePosition> {
    let hall = landmarks::outpost(settings, layout, crate::descent::Descent::Ancients)?.bounds;
    Some(TilePosition {
        x: i32::midpoint(hall.min.x, hall.max.x),
        y: hall.min.y,
    })
}

/// The tower floors (slab rows) of a tower or the library: the roof under the upper paving,
/// then every [`TOWER_FLOOR_ROWS`], and the base (the lower avenue's ceiling).
fn tower_slabs(stamp: &CityStamp) -> Vec<i32> {
    let roof = stamp.floor + 2;
    let base = tower_base(stamp);
    let mut slabs = vec![roof];
    let mut row = roof + TOWER_FLOOR_ROWS;
    while base - row >= 4 {
        slabs.push(row);
        row += TOWER_FLOOR_ROWS;
    }
    slabs.push(base);
    slabs
}

/// A tower's lift stops: the upper street, every inner floor and the lower street (floor rows
/// a rig stands on).
#[must_use]
pub fn lift_stops(stamp: &CityStamp) -> Vec<i32> {
    let slabs = tower_slabs(stamp);
    let mut stops = vec![stamp.floor];
    // Inner floors: neither the roof (right under the paving) nor the base.
    if slabs.len() > 2 {
        stops.extend_from_slice(&slabs[1..slabs.len() - 1]);
    }
    stops.push(stamp.lower);
    stops
}

fn tower_tile(stamp: &CityStamp, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if x < b.min.x || x > b.max.x || y < b.min.y || y > b.max.y {
        return None;
    }
    let library = stamp.kind == CityStampKind::LanternLibrary;
    let in_shaft = (x - stamp.shaft).abs() <= 1;
    // The cab's room over the upper street, the upper avenue's paving and the lower avenue
    // under the base: only the shaft is the tower's there (kept clear for the cab).
    let base = tower_base(stamp);
    // The library has porches at both stops, so its lift works even where no avenue runs.
    let porch = (y < stamp.floor) || (y > base && y >= stamp.lower - LIFT_ROOM_ROWS);
    if library && porch {
        return Some(TileKind::Air);
    }
    if y <= stamp.floor + 1 || y > base {
        return in_shaft.then_some(TileKind::Air);
    }
    let roof = stamp.floor + 2;
    let slabs = tower_slabs(stamp);
    let slab = slabs.contains(&y);
    let facade = x == b.min.x || x == b.max.x;
    if facade {
        // Builder alloy at every floor (the frame: the Phase Drill's work), glass between.
        return Some(if slab {
            TileKind::Cave(CaveTile::BuilderAlloy)
        } else {
            TileKind::City(CityTile::TowerGlass)
        });
    }
    if slab {
        return Some(if in_shaft {
            TileKind::Air
        } else {
            TileKind::City(CityTile::CityStone)
        });
    }
    // The lift's guide rail between the facade and the shaft.
    let rail = if stamp.shaft - b.min.x < b.max.x - stamp.shaft {
        stamp.shaft - 2
    } else {
        stamp.shaft + 2
    };
    if x == rail && rail != b.min.x && rail != b.max.x {
        return Some(TileKind::City(CityTile::LiftRail));
    }
    if in_shaft {
        return Some(TileKind::Air);
    }
    // The floor this row stands over and how high above it.
    let floor_below = slabs.iter().copied().find(|row| *row > y).unwrap_or(base);
    let height = floor_below - y;
    // A lamp under every slab, mid-floor (the library's burn on every second column).
    let ceiling = slabs.iter().copied().rfind(|row| *row < y).unwrap_or(roof);
    let mid = i32::midpoint(b.min.x, b.max.x);
    if y == ceiling + 1 && (x == mid || (library && (x - b.min.x).rem_euclid(4) == 2)) {
        return Some(TileKind::City(CityTile::StreetLamp));
    }
    // Things left on the floors.
    if height == 1 {
        let roll = hash2(stamp.seed, SALT_DECOR, x, y);
        if library && y == roof + TOWER_FLOOR_ROWS - 1 && x == library_core(stamp).x {
            return Some(TileKind::City(CityTile::ArchivistCore));
        }
        return Some(match roll % 37 {
            0 => TileKind::Cave(if stamp.along > 600 {
                CaveTile::RelicCache
            } else {
                CaveTile::TreasureCache
            }),
            1..=2 => TileKind::Ore(MineralKind::Mythril),
            3 => TileKind::Ore(MineralKind::Platinum),
            4 => TileKind::City(CityTile::AlloyOre),
            _ => TileKind::Air,
        });
    }
    Some(TileKind::Air)
}

fn avenue_tile(stamp: &CityStamp, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if !b.contains(TilePosition { x, y }) {
        return None;
    }
    let floor = stamp.floor;
    let column = x - b.min.x;
    if y == b.min.y {
        // The ceiling: paving with lamps hanging from it.
        return Some(if column.rem_euclid(9) == 4 {
            TileKind::City(CityTile::StreetLamp)
        } else {
            TileKind::City(CityTile::CityStone)
        });
    }
    if y == floor + 1 {
        return Some(TileKind::City(CityTile::CityStone));
    }
    if y == floor {
        if stamp.tram {
            let platform = stations(stamp)
                .iter()
                .any(|station| (x - station).abs() <= 3);
            return Some(TileKind::City(if platform {
                CityTile::Platform
            } else {
                CityTile::TramRail
            }));
        }
        return Some(TileKind::City(CityTile::CityStone));
    }
    // Rounded ends: the street runs out under fallen rock.
    let from_end = column.min(b.max.x - x);
    if from_end < 2 && y < floor - 3 + from_end * 2 {
        return None;
    }
    if stamp.tram {
        // Buffers at the line's ends.
        if (from_end == 1) && y == floor - 1 {
            return Some(TileKind::City(CityTile::CityStone));
        }
        return Some(TileKind::Air);
    }
    // Fallen masonry on streets no tram keeps clear.
    let heap = hash2(stamp.seed, SALT_DECOR, x.div_euclid(11), floor);
    if heap.is_multiple_of(4) {
        let centre = x.div_euclid(11) * 11 + 5;
        let spread = 2 - (floor - 1 - y);
        if y >= floor - 2 && (x - centre).abs() <= spread {
            return Some(TileKind::City(CityTile::CityStone));
        }
    }
    Some(TileKind::Air)
}

/// Whether `(x, y)` is part of an alloy portal under the seal (two builder alloy pillars and a
/// lintel framing the way into Descent VII, a lamp burning under it). `None`: not a portal.
const fn portal_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
) -> Option<TileKind> {
    let index = Stratum::PrismDesert.index();
    let (_, half) = super::boundary_shape(index);
    let centre = x.div_euclid(PORTAL_SPACING) * PORTAL_SPACING + PORTAL_SPACING / 2;
    if (x - centre).abs() > 3 {
        return None;
    }
    let (low, high) = clear_columns(settings);
    if centre >= low - 6 && centre <= high + 6 {
        return None;
    }
    let seal_top = super::boundary_column_depth(settings, layout, index, centre) + half;
    let top = seal_top + crate::worldgen::descent::far_strange::ALLOY_SEAL_ROWS;
    if y < top || y > top + 8 {
        return None;
    }
    let dx = (x - centre).abs();
    if dx == 3 {
        return Some(TileKind::Cave(CaveTile::BuilderAlloy));
    }
    if y == top {
        return Some(if dx == 0 {
            TileKind::City(CityTile::StreetLamp)
        } else {
            TileKind::Cave(CaveTile::BuilderAlloy)
        });
    }
    if y == top + 8 {
        return Some(TileKind::City(CityTile::CityStone));
    }
    Some(TileKind::Air)
}

// --- Engine Halls ---------------------------------------------------------------------------------

/// The fixed machinery of one engine set piece (unmirrored layout coordinates).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EngineParts {
    /// `(centre column, base row, stroke, phase step)`.
    pub pistons: Vec<(i32, i32, i32, u8)>,
    /// `(first column, last column, row)`.
    pub ledges: Vec<(i32, i32, i32)>,
    /// `(centre column, housing row, travel, phase step)`.
    pub crushers: Vec<(i32, i32, i32, u8)>,
    /// `(first column, last column, row, direction)`.
    pub belts: Vec<(i32, i32, i32, i8)>,
    /// `(column, row, tile)`.
    pub caches: Vec<(i32, i32, TileKind)>,
    /// The beat (ms).
    pub beat: u64,
}

/// A staircase of `steps` pistons rising from `(start, floor)` towards `dir` (1 right, -1
/// left), each onto a ledge [`PISTON_STROKE`] rows higher.
fn staircase(parts: &mut EngineParts, start: i32, floor: i32, steps: i32, dir: i32, phase0: u8) {
    for step in 0..steps {
        let centre = start + dir * 6 * step;
        let base = floor - PISTON_STROKE * step;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few steps"
        )]
        parts
            .pistons
            .push((centre, base, PISTON_STROKE, phase0 + step as u8));
        let ledge_row = floor - PISTON_STROKE * (step + 1);
        let (a, b) = (centre + dir * 2, centre + dir * 7);
        parts.ledges.push((a.min(b), a.max(b), ledge_row));
    }
}

/// The machinery of `stamp` (an engine set piece).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one block per engine set piece")]
pub fn engine_parts(stamp: &CityStamp) -> EngineParts {
    let mut parts = EngineParts {
        beat: engine_beat_millis(stamp.along),
        ..EngineParts::default()
    };
    let width = stamp.bounds.width();
    let height = stamp.bounds.height();
    let floor = height - 2;
    let prize = if stamp.along > 500 {
        CaveTile::RelicCache
    } else {
        CaveTile::TreasureCache
    };
    match stamp.kind {
        CityStampKind::EngineHall => {
            let steps = ((floor - 4) / PISTON_STROKE).min(3);
            staircase(&mut parts, 5, floor, steps, 1, 0);
            let top = floor - PISTON_STROKE * steps;
            let end = 6 * steps + 6;
            parts.caches.push((end, top - 1, TileKind::Cave(prize)));
            parts
                .caches
                .push((end - 1, top - 1, TileKind::Ore(MineralKind::Mythril)));
            parts
                .caches
                .push((end - 2, top - 1, TileKind::City(CityTile::EngineCoreOre)));
            // Belts over the rest of the floor, crushers over them.
            let mut x = end + 2;
            let mut dir: i8 = if stamp.seed.is_multiple_of(2) { 1 } else { -1 };
            while x + 3 < width - 3 {
                let last = (x + 7).min(width - 4);
                parts.belts.push((x, last, floor, dir));
                dir = -dir;
                x = last + 1;
            }
            let mut column = end + 6;
            let mut step = 0;
            while column <= width - 6 {
                parts.crushers.push((column, 0, floor - 3, step * 2));
                column += 9;
                step += 1;
            }
        }
        CityStampKind::PistonShaft => {
            let steps = (floor - 5) / PISTON_STROKE;
            for step in 0..steps {
                let left = step % 2 == 0;
                let centre = if left { 3 } else { 8 };
                let base = floor - PISTON_STROKE * step;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a few steps"
                )]
                parts
                    .pistons
                    .push((centre, base, PISTON_STROKE, step as u8));
                let row = floor - PISTON_STROKE * (step + 1);
                parts.ledges.push(if left {
                    (5, width - 2, row)
                } else {
                    (1, 6, row)
                });
            }
            let top = floor - PISTON_STROKE * steps;
            let (cx, ox) = if steps % 2 == 1 {
                (width - 2, width - 3)
            } else {
                (1, 2)
            };
            parts.caches.push((cx, top - 1, TileKind::Cave(prize)));
            parts
                .caches
                .push((ox, top - 1, TileKind::City(CityTile::EngineCoreOre)));
        }
        CityStampKind::CrusherLine => {
            let dir: i8 = 1;
            parts.belts.push((2, width - 4, floor, dir));
            let mut column = 6;
            let mut step = 0;
            while column <= width - 7 {
                parts.crushers.push((column, 0, floor - 3, step));
                column += 7;
                step += 1;
            }
            parts
                .caches
                .push((width - 3, floor - 1, TileKind::Cave(prize)));
            parts
                .caches
                .push((width - 4, floor - 1, TileKind::Ore(MineralKind::Uranium)));
        }
        CityStampKind::GreatEngine => {
            staircase(&mut parts, 5, floor, 3, 1, 0);
            staircase(&mut parts, width - 6, floor, 3, -1, 3);
            let gallery = floor - PISTON_STROKE * 3;
            parts.ledges.push((24, width - 25, gallery));
            let mid = width / 2;
            parts
                .caches
                .push((mid, gallery - 1, TileKind::Cave(CaveTile::RelicCache)));
            for dx in [-3, -2, 2, 3] {
                parts.caches.push((
                    mid + dx,
                    gallery - 1,
                    TileKind::City(CityTile::EngineCoreOre),
                ));
            }
            parts.belts.push((26, mid - 1, floor, 1));
            parts.belts.push((mid + 1, width - 27, floor, -1));
        }
        _ => {}
    }
    parts
}

fn engine_tile(stamp: &CityStamp, parts: &EngineParts, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if !b.contains(TilePosition { x, y }) {
        return None;
    }
    let lx = stamp.local_x(x);
    let ly = y - b.min.y;
    let width = b.width();
    let height = b.height();
    let floor = height - 2;
    let wall = lx == 0 || lx == width - 1 || ly == 0 || ly >= floor;
    // Doors at the floor in both side walls (and, in a shaft, at the top ledge's level).
    let side = lx == 0 || lx == width - 1;
    if side && ly >= floor - 3 && ly < floor {
        return Some(TileKind::Air);
    }
    if stamp.kind == CityStampKind::PistonShaft && side {
        let top = parts.ledges.last().map_or(0, |ledge| ledge.2);
        if ly >= top - 3 && ly < top {
            return Some(TileKind::Air);
        }
    }
    // Housings: under every piston head at rest, over every crusher.
    for &(centre, base, _, _) in &parts.pistons {
        if ly == base && (lx - centre).abs() <= 1 {
            return Some(TileKind::City(CityTile::PistonHousing));
        }
    }
    for &(centre, housing, _, _) in &parts.crushers {
        if ly == housing && (lx - centre).abs() <= 1 {
            return Some(TileKind::City(CityTile::PistonHousing));
        }
    }
    if ly == floor {
        for &(first, last, row, dir) in &parts.belts {
            if row == floor && lx >= first && lx <= last {
                return Some(TileKind::City(if dir < 0 {
                    CityTile::ConveyorLeft
                } else {
                    CityTile::ConveyorRight
                }));
            }
        }
    }
    if wall {
        let roll = hash2(stamp.seed, SALT_DECOR, x, y);
        return Some(TileKind::City(if roll.is_multiple_of(23) {
            CityTile::EngineCoreOre
        } else if ly == 0 && roll % 7 == 1 {
            CityTile::Pipe
        } else {
            CityTile::EngineSteel
        }));
    }
    for &(first, last, row) in &parts.ledges {
        if ly == row && lx >= first && lx <= last {
            return Some(TileKind::City(CityTile::EngineSteel));
        }
    }
    for &(cx, cy, kind) in &parts.caches {
        if lx == cx && ly == cy {
            return Some(kind);
        }
    }
    Some(TileKind::Air)
}

/// The Great Engine's hall (one per Deep Descent world, mid-way down the Engine Halls on the
/// spawn's side of the cable).
#[must_use]
pub const fn great_engine(settings: &WorldSettings, layout: &DescentLayout) -> Option<CityStamp> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::EngineHalls);
    let roll = hash2(settings.seed, SALT_FIXED, 2, 0);
    let (low, high) = clear_columns(settings);
    let side = spawn_side(settings);
    let x = if side < 0 {
        low - 20 - GREAT_ENGINE_SIZE.0 - roll_range(roll, 0, 0, 30)
    } else {
        high + 20 + roll_range(roll, 0, 0, 30)
    };
    let middle = top + (bottom - top) / 2 + roll_range(roll, 12, -12, 12);
    let bounds = rect(x, middle - GREAT_ENGINE_SIZE.1 / 2, GREAT_ENGINE_SIZE);
    Some(CityStamp {
        mirrored: false,
        ..CityStamp::plain(CityStampKind::GreatEngine, bounds, roll, 500)
    })
}

fn engine_stamp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    fixed: &[CityStamp],
    rx: i32,
    ry: i32,
) -> Option<CityStamp> {
    let (width, height) = ENGINE_REGION;
    let centre_row = ry * height + height / 2;
    if layout.stratum_of_row(centre_row) != Stratum::EngineHalls {
        return None;
    }
    let along = layout.along(centre_row);
    let roll = hash2(settings.seed, SALT_REGION, rx, ry);
    if roll % 1_000 >= 460 + 300 * u64::from(along) / 1_000 {
        return None;
    }
    let pick = variant(settings, Stratum::EngineHalls);
    let shafts = if pick == 1 { 430 } else { 200 };
    let lines = if pick == 0 { 400 } else { 180 };
    let kind_roll = (roll >> 50) % 1_000;
    let (kind, w, h) = if kind_roll < shafts {
        (CityStampKind::PistonShaft, (12, 12), (34, 42))
    } else if kind_roll < shafts + lines {
        (CityStampKind::CrusherLine, (40, 56), (7, 7))
    } else {
        (CityStampKind::EngineHall, (44, 56), (20, 24))
    };
    let sw = roll_range(roll, 10, w.0, w.1);
    let sh = roll_range(roll, 20, h.0, h.1);
    let x = rx * width + roll_range(roll, 30, 3, (width - sw - 3).max(3));
    let y = ry * height + roll_range(roll, 40, 3, (height - sh - 3).max(3));
    let bounds = rect(x, y, (sw, sh));
    let (top, bottom) = layout.rows(Stratum::EngineHalls);
    if bounds.min.y < top + 16 || bounds.max.y > bottom - 16 {
        return None;
    }
    if fixed
        .iter()
        .any(|stamp| stamp.bounds.expanded(6).intersects(&bounds))
    {
        return None;
    }
    let (low, high) = clear_columns(settings);
    if bounds.max.x >= low - 6 && bounds.min.x <= high + 6 {
        return None;
    }
    if touches_pilot_space(settings, &bounds) {
        return None;
    }
    Some(CityStamp::plain(
        kind,
        bounds,
        hash2(settings.seed, SALT_DECOR, rx, ry),
        along,
    ))
}

// --- The Archive of Stars ---------------------------------------------------------------------------

/// Vault layouts (unmirrored, the way in on the left). Legend: `#` keeps the rock, `.` air,
/// `R` ruin masonry (unbreakable), `A` archive stone, `V` vent grate, `G` gate bars, `0`/`1`/
/// `2` gate lintels of that channel, `L` a lever (channel 0), `P` a pressure plate (channel 1),
/// `Q` a pressure plate (channel 2), `r`/`g`/`b` red, green and blue keyed glass, `*` a star
/// chart's plinth, `s` star glass, `o` diamond, `T` a treasure cache.
const PLATE_VAULT: [&str; 7] = [
    "##########RRRRRRRRRRRRRRRRRR",
    "##########R....s.....s.....R",
    "AAAAAAAAAA1................R",
    "..........G................R",
    "..........G......o.o.......R",
    "..........G.......*......T.R",
    "AAPAAAAAAARRRRRRRRRRRRRRRRRR",
];

const LEVER_VAULT: [&str; 8] = [
    "RRRRRRRRRRRRRRRRRRRRRRRR",
    "R..RRRRRRRRRRRRRRRRRRRRR",
    "V..LRRRRRRRRRRRRRRRRRRRR",
    "0RRRRRRRRs.....s.....sRR",
    "G.....................RR",
    "G.........o.o.........RR",
    "G..........*.......T..RR",
    "RRRRRRRRRRRRRRRRRRRRRRRR",
];

const LIGHT_VAULT: [&str; 7] = [
    "RRRRRRRRRRRRRRRRRRRRRRRRRR",
    "R....r...g...b......s...RR",
    "R....r...g...b..........RR",
    ".....r...g...b..........RR",
    ".....r...g...b....o.o...RR",
    ".....r...g...b.....*..T.RR",
    "RRRRRRRRRRRRRRRRRRRRRRRRRR",
];

const GRAND_VAULT: [&str; 7] = [
    "##########RRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR",
    "##########R.......R...r...g...b.......s.......RR",
    "AAAAAAAAAA1.......2...r...g...b...............RR",
    "..........G.......G...r...g...b...............RR",
    "..........G.......G...r...g...b.....o.o.o.....RR",
    "..........G.......G...r...g...b......*....T...RR",
    "AAPAAAAAAARQRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR",
];

/// The layout of a vault of `kind`.
#[must_use]
pub const fn vault_layout(kind: VaultKind) -> &'static [&'static str] {
    match kind {
        VaultKind::Plate => &PLATE_VAULT,
        VaultKind::Lever => &LEVER_VAULT,
        VaultKind::Light => &LIGHT_VAULT,
        VaultKind::Grand => &GRAND_VAULT,
    }
}

/// `(width, height)` of a vault of `kind`.
#[must_use]
pub fn vault_size(kind: VaultKind) -> (i32, i32) {
    let layout = vault_layout(kind);
    let width = layout.iter().map(|row| row.len()).max().unwrap_or(0);
    (
        i32::try_from(width).unwrap_or(0),
        i32::try_from(layout.len()).unwrap_or(0),
    )
}

/// The tile legend character `c` stands for (`None`: keep the rock).
const fn legend(c: u8) -> Option<TileKind> {
    Some(match c {
        b'.' => TileKind::Air,
        b'R' => TileKind::Built(BuiltTile::RuinWall),
        b'A' => TileKind::City(CityTile::ArchiveStone),
        b'V' => TileKind::Built(BuiltTile::VentGrate),
        b'G' => TileKind::Built(BuiltTile::Gate),
        b'0' => TileKind::Built(BuiltTile::GateLintel(0)),
        b'1' => TileKind::Built(BuiltTile::GateLintel(1)),
        b'2' => TileKind::Built(BuiltTile::GateLintel(2)),
        b'L' => TileKind::Built(BuiltTile::Lever {
            channel: 0,
            on: false,
        }),
        b'P' => TileKind::Built(BuiltTile::PressurePlate {
            channel: 1,
            down: false,
        }),
        b'Q' => TileKind::Built(BuiltTile::PressurePlate {
            channel: 2,
            down: false,
        }),
        b'r' => TileKind::Cave(CaveTile::PrismRed),
        b'g' => TileKind::Cave(CaveTile::PrismGreen),
        b'b' => TileKind::Cave(CaveTile::PrismBlue),
        b'*' => TileKind::City(CityTile::ChartPlinth),
        b's' => TileKind::City(CityTile::StarGlass),
        b'o' => TileKind::Ore(MineralKind::Diamond),
        b'T' => TileKind::Cave(CaveTile::TreasureCache),
        _ => return None,
    })
}

fn vault_tile(stamp: &CityStamp, kind: VaultKind, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if !b.contains(TilePosition { x, y }) {
        return None;
    }
    let layout = vault_layout(kind);
    let row = layout.get(usize::try_from(y - b.min.y).ok()?)?;
    let c = *row
        .as_bytes()
        .get(usize::try_from(stamp.local_x(x)).ok()?)?;
    legend(c)
}

/// The plinth of a vault stamp (its world tile).
#[must_use]
pub fn vault_plinth(stamp: &CityStamp) -> Option<TilePosition> {
    let CityStampKind::Vault(kind) = stamp.kind else {
        return None;
    };
    for (row_index, row) in vault_layout(kind).iter().enumerate() {
        if let Some(column) = row.bytes().position(|c| c == b'*') {
            return Some(TilePosition {
                x: stamp.world_x(i32::try_from(column).ok()?),
                y: stamp.bounds.min.y + i32::try_from(row_index).ok()?,
            });
        }
    }
    None
}

const fn stacks_tile(stamp: &CityStamp, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if !b.contains(TilePosition { x, y }) {
        return None;
    }
    let lx = x - b.min.x;
    let ly = y - b.min.y;
    let width = b.width();
    let height = b.height();
    let floor = height - 2;
    let side = lx == 0 || lx == width - 1;
    if side && ly >= floor - 3 && ly < floor {
        return Some(TileKind::Air);
    }
    if side || ly == 0 || ly >= floor {
        // Rough walls: here and there the rock shows through.
        let roll = hash2(stamp.seed, SALT_DECOR, x, y);
        if roll.is_multiple_of(11) {
            return None;
        }
        return Some(TileKind::City(CityTile::ArchiveStone));
    }
    let roll = hash2(stamp.seed, SALT_DECOR, x, y);
    // Lamps of star glass in the ceiling.
    if ly == 1 && lx.rem_euclid(8) == 4 {
        return Some(TileKind::City(CityTile::StarGlass));
    }
    let bay = (lx - 2).rem_euclid(7);
    let shelf_column = (bay == 0 || bay == 1) && lx >= 2 && lx <= width - 3;
    // Floor shelves, four tall.
    if shelf_column && ly >= floor - 4 {
        return Some(TileKind::City(CityTile::ArchiveShelf));
    }
    // A mezzanine between the shelves, with more shelving on it in tall stacks.
    let mezzanine = floor - 8;
    if mezzanine > 3 {
        if ly == mezzanine && bay >= 3 && lx > 2 && lx < width - 3 {
            return Some(TileKind::City(CityTile::ArchiveStone));
        }
        if shelf_column && ly < mezzanine && ly >= mezzanine - 3 && height >= 15 {
            return Some(TileKind::City(CityTile::ArchiveShelf));
        }
    }
    // Something left on a shelf top or the floor.
    if ly == floor - 5 && shelf_column && roll.is_multiple_of(9) {
        return Some(TileKind::Ore(MineralKind::Diamond));
    }
    if ly == floor - 1 && bay == 4 && roll.is_multiple_of(23) {
        return Some(TileKind::Cave(CaveTile::TreasureCache));
    }
    Some(TileKind::Air)
}

/// The Orrery (one per Deep Descent world, deep in the Archive of Stars on the spawn's side).
#[must_use]
pub const fn orrery(settings: &WorldSettings, layout: &DescentLayout) -> Option<CityStamp> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::ArchiveOfStars);
    let roll = hash2(settings.seed, SALT_FIXED, 3, 0);
    let (low, high) = clear_columns(settings);
    let side = spawn_side(settings);
    let x = if side < 0 {
        low - 16 - ORRERY_SIZE.0 - roll_range(roll, 0, 0, 24)
    } else {
        high + 16 + roll_range(roll, 0, 0, 24)
    };
    let middle = top + (bottom - top) * 2 / 5 + roll_range(roll, 12, -10, 10);
    let bounds = rect(x, middle - ORRERY_SIZE.1 / 2, ORRERY_SIZE);
    Some(CityStamp {
        mirrored: false,
        ..CityStamp::plain(CityStampKind::Orrery, bounds, roll, 400)
    })
}

/// The Orrery's three plinths (on its floor, mid-hall and a quarter to each side).
#[must_use]
pub fn orrery_plinths(orrery: &CityStamp) -> [TilePosition; 3] {
    let b = orrery.bounds;
    let centre = b.center().x;
    let floor = orrery_floor(orrery);
    [-14, 0, 14].map(|dx| TilePosition {
        x: centre + dx,
        y: floor - 1,
    })
}

/// The Orrery's floor row (the plinths stand on it).
const fn orrery_floor(orrery: &CityStamp) -> i32 {
    let b = orrery.bounds;
    let ry = (b.max.y - b.min.y + 1) / 2;
    b.min.y + ry + ry * 6 / 10 + 1
}

fn orrery_tile(stamp: &CityStamp, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    if !b.contains(TilePosition { x, y }) {
        return None;
    }
    let rx = (b.max.x - b.min.x + 1) / 2;
    let ry = (b.max.y - b.min.y + 1) / 2;
    let (cx, cy) = (b.min.x + rx, b.min.y + ry);
    let dx = i64::from(x - cx) * 1_000 / i64::from(rx.max(1));
    let dy = i64::from(y - cy) * 1_000 / i64::from(ry.max(1));
    let inside = (dx * dx + dy * dy) / 1_000;
    let floor = orrery_floor(stamp);
    // Doors at the floor at both ends.
    if y >= floor - 3 && y < floor && (x - cx).abs() >= rx - 3 {
        return Some(TileKind::Air);
    }
    if inside >= 1_000 {
        return None;
    }
    if inside >= 880 {
        return Some(TileKind::City(CityTile::ArchiveStone));
    }
    if inside >= 800 {
        // The ring of star glass lining the dome.
        return Some(if hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(3) {
            TileKind::City(CityTile::StarGlass)
        } else {
            TileKind::City(CityTile::ArchiveStone)
        });
    }
    if y >= floor {
        return Some(TileKind::City(CityTile::ArchiveStone));
    }
    if orrery_plinths(stamp).contains(&TilePosition { x, y }) {
        return Some(TileKind::City(CityTile::ChartPlinth));
    }
    // Spheres on their arms: little knots of star glass round the hall.
    for (index, (sx, sy)) in [(-20, -6), (-9, -9), (6, -8), (18, -5), (-14, 0), (13, 1)]
        .into_iter()
        .enumerate()
    {
        let size = 1 + i32::from(index % 2 == 0);
        if (x - (cx + sx)).abs() < size && (y - (cy + sy)).abs() < size {
            return Some(TileKind::City(CityTile::StarGlass));
        }
    }
    Some(TileKind::Air)
}

fn archive_stamp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    fixed: &[CityStamp],
    rx: i32,
    ry: i32,
) -> Option<CityStamp> {
    let (width, height) = ARCHIVE_REGION;
    let centre_row = ry * height + height / 2;
    if layout.stratum_of_row(centre_row) != Stratum::ArchiveOfStars {
        return None;
    }
    let along = layout.along(centre_row);
    let roll = hash2(settings.seed, SALT_REGION ^ 0xA2, rx, ry);
    if roll % 1_000 >= 500 + 260 * u64::from(along) / 1_000 {
        return None;
    }
    let vault_roll = (roll >> 50) % 1_000;
    let vault_chance = if variant(settings, Stratum::ArchiveOfStars) == 1 {
        360
    } else {
        300
    } + u64::from(along) / 5;
    let kind = if along >= 120 && vault_roll < vault_chance {
        let pick = (roll >> 44) % 3;
        CityStampKind::Vault(match along {
            0..=349 => {
                if pick == 0 {
                    VaultKind::Lever
                } else {
                    VaultKind::Plate
                }
            }
            350..=699 => match pick {
                0 => VaultKind::Plate,
                1 => VaultKind::Lever,
                _ => VaultKind::Light,
            },
            _ => {
                if pick == 0 {
                    VaultKind::Light
                } else {
                    VaultKind::Grand
                }
            }
        })
    } else {
        CityStampKind::Stacks
    };
    let (sw, sh) = match kind {
        CityStampKind::Vault(vault) => vault_size(vault),
        _ => (roll_range(roll, 10, 26, 40), roll_range(roll, 20, 12, 16)),
    };
    let x = rx * width + roll_range(roll, 30, 3, (width - sw - 3).max(3));
    let y = ry * height + roll_range(roll, 40, 3, (height - sh - 3).max(3));
    let bounds = rect(x, y, (sw, sh));
    let (top, bottom) = layout.rows(Stratum::ArchiveOfStars);
    if bounds.min.y < top + 16 || bounds.max.y > bottom - 16 {
        return None;
    }
    if fixed
        .iter()
        .any(|stamp| stamp.bounds.expanded(6).intersects(&bounds))
    {
        return None;
    }
    let (low, high) = clear_columns(settings);
    if bounds.max.x >= low - 6 && bounds.min.x <= high + 6 {
        return None;
    }
    if touches_pilot_space(settings, &bounds) {
        return None;
    }
    Some(CityStamp::plain(
        kind,
        bounds,
        hash2(settings.seed, SALT_DECOR ^ 0xA2, rx, ry),
        along,
    ))
}

// --- Gathering --------------------------------------------------------------------------------------

/// The fixed stamps (the Lantern Library, the Great Engine, the Orrery).
fn fixed_stamps(settings: &WorldSettings, layout: &DescentLayout) -> Vec<CityStamp> {
    [
        lantern_library(settings, layout),
        great_engine(settings, layout),
        orrery(settings, layout),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Every city stamp whose bounds overlap `area` (empty for worlds without Descents).
///
/// Towers and the library come first (they cut the avenues' paving for their shafts), then
/// avenues, then the engine and archive set pieces.
#[must_use]
pub fn stamps_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<CityStamp> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let (first, last) = carve_rows(layout);
    if area.max.y < first || area.min.y > last {
        return Vec::new();
    }
    let fixed = fixed_stamps(settings, layout);
    let mut found: Vec<CityStamp> = Vec::new();
    if let Some(library) = fixed
        .iter()
        .find(|stamp| stamp.kind == CityStampKind::LanternLibrary)
        && library.bounds.intersects(area)
    {
        found.push(*library);
    }
    let levels = avenue_levels(settings, layout);
    let library = lantern_library(settings, layout);
    // Towers.
    for gap in 0..levels.len().saturating_sub(1) {
        let (upper, lower) = (levels[gap], levels[gap + 1]);
        if lower - 1 < area.min.y || upper - LIFT_ROOM_ROWS > area.max.y {
            continue;
        }
        for cell in (area.min.x - TOWER_CELL).div_euclid(TOWER_CELL)
            ..=(area.max.x + TOWER_CELL).div_euclid(TOWER_CELL)
        {
            if let Some(stamp) = tower(settings, layout, &levels, gap, cell)
                && stamp.bounds.intersects(area)
                && library.is_none_or(|lib| !lib.bounds.expanded(3).intersects(&stamp.bounds))
            {
                found.push(stamp);
            }
        }
    }
    // Avenues.
    for (level, &floor) in levels.iter().enumerate() {
        if floor + 1 < area.min.y || floor - AVENUE_ROWS - 1 > area.max.y {
            continue;
        }
        for segment in area.min.x.div_euclid(SEGMENT)..=area.max.x.div_euclid(SEGMENT) {
            if let Some(stamp) = avenue(settings, layout, level, floor, segment)
                && stamp.bounds.intersects(area)
            {
                found.push(stamp);
            }
        }
    }
    // The fixed engine and archive pieces, then the regions.
    found.extend(
        fixed
            .iter()
            .filter(|stamp| stamp.kind != CityStampKind::LanternLibrary)
            .filter(|stamp| stamp.bounds.intersects(area))
            .copied(),
    );
    let (width, height) = ENGINE_REGION;
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(stamp) = engine_stamp(settings, layout, &fixed, rx, ry)
                && stamp.bounds.intersects(area)
            {
                found.push(stamp);
            }
        }
    }
    let (width, height) = ARCHIVE_REGION;
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(stamp) = archive_stamp(settings, layout, &fixed, rx, ry)
                && stamp.bounds.intersects(area)
            {
                found.push(stamp);
            }
        }
    }
    found
}

/// Everything of the buried city touching one chunk, gathered once.
#[derive(Clone, Debug, Default)]
pub struct CityPlan {
    pub stamps: Vec<CityStamp>,
    /// Each engine stamp's machinery (parallel to `stamps`; empty for the others).
    pub parts: Vec<EngineParts>,
    /// Descent landmarks (the outpost hall) the pass leaves alone.
    pub keep: Vec<TileRect>,
    /// Whether the chunk lies in the pass's rows at all.
    pub active: bool,
    /// The Archive Post's archivist core, if it is in the chunk (set in the outpost hall's
    /// ceiling, which the pass otherwise leaves alone).
    pub post: Option<TilePosition>,
}

/// The plan for `area` (see [`CityPlan`]).
#[must_use]
pub fn plan_for(settings: &WorldSettings, layout: &DescentLayout, area: &TileRect) -> CityPlan {
    if !settings.generator.has_descents() {
        return CityPlan::default();
    }
    let (first, last) = carve_rows(layout);
    if area.max.y < first || area.min.y > last {
        return CityPlan::default();
    }
    let grown = area.expanded(1);
    let stamps = stamps_with(settings, layout, &grown);
    let parts = stamps
        .iter()
        .map(|stamp| match stamp.kind {
            CityStampKind::EngineHall
            | CityStampKind::PistonShaft
            | CityStampKind::CrusherLine
            | CityStampKind::GreatEngine => engine_parts(stamp),
            _ => EngineParts::default(),
        })
        .collect();
    let keep = landmarks::landmarks_with(settings, layout, area)
        .iter()
        .map(|landmark| landmark.bounds.expanded(1))
        .collect();
    CityPlan {
        stamps,
        parts,
        keep,
        active: true,
        post: post_core(settings, layout).filter(|core| grown.contains(*core)),
    }
}

/// Whether a pilot-only space (V9's sealed ruins, rig vaults and crawlway caves, built of
/// unbreakable masonry) lies within two tiles of `bounds`: the engine and archive set pieces
/// give way to them.
fn touches_pilot_space(settings: &WorldSettings, bounds: &TileRect) -> bool {
    !crate::worldgen::streamed::pilot_spaces::spaces_touching(settings, &bounds.expanded(2))
        .is_empty()
}

/// Whether a stamp may carve over `kind`.
/// (Prefab points of interest give way: the city's set pieces are whole; built pieces, the
/// bedrock and the alloy seal stay.)
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Bedrock | TileKind::Cave(CaveTile::BuilderAlloy)
    )
}

/// Whether `kind` is plain rock the rock pass may dress.
const fn plain_rock(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

/// The tile `stamp` puts at `(x, y)`, or `None` to leave it.
#[must_use]
pub fn stamp_tile(stamp: &CityStamp, parts: &EngineParts, x: i32, y: i32) -> Option<TileKind> {
    match stamp.kind {
        CityStampKind::Tower | CityStampKind::LanternLibrary => tower_tile(stamp, x, y),
        CityStampKind::Avenue => avenue_tile(stamp, x, y),
        CityStampKind::EngineHall
        | CityStampKind::PistonShaft
        | CityStampKind::CrusherLine
        | CityStampKind::GreatEngine => engine_tile(stamp, parts, x, y),
        CityStampKind::Stacks => stacks_tile(stamp, x, y),
        CityStampKind::Vault(kind) => vault_tile(stamp, kind, x, y),
        CityStampKind::Orrery => orrery_tile(stamp, x, y),
    }
}

/// City rock: what the plain rock `base` at `(x, y)` of `stratum` becomes (alloy ore veins,
/// pipes and engine core ore, star glass), or `None` to leave it.
#[must_use]
pub fn city_rock(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !plain_rock(base) {
        return None;
    }
    let along = i32::from(layout.along(y));
    let seed = settings.seed;
    match stratum {
        Stratum::BuriedMegacity => {
            let line = ridge(value_noise(seed, SALT_ROCK, x, y, 14, 8));
            (line > NOISE_MAX - 900 - along).then_some(TileKind::City(CityTile::AlloyOre))
        }
        Stratum::EngineHalls => {
            if (y.rem_euclid(29) == 11)
                && value_noise(seed, SALT_ROCK ^ 1, x, y, 24, 4) > NOISE_MAX / 2
            {
                return Some(TileKind::City(CityTile::Pipe));
            }
            hash2(seed, SALT_ROCK, x, y)
                .is_multiple_of(71)
                .then_some(TileKind::City(CityTile::EngineCoreOre))
        }
        Stratum::ArchiveOfStars => {
            if hash2(seed, SALT_ROCK, x, y).is_multiple_of(83) {
                return Some(TileKind::City(CityTile::StarGlass));
            }
            (value_noise(seed, SALT_ROCK ^ 2, x, y, 9, 6) > NOISE_MAX - 5_000)
                .then_some(TileKind::City(CityTile::ArchiveStone))
        }
        _ => None,
    }
}

/// The buried city's tile at `(x, y)` over `base`: a stamp's tile, a portal under the seal,
/// else city rock, else `None` (leave `base`).
#[must_use]
pub fn city_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    plan: &CityPlan,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !plan.active {
        return None;
    }
    let at = TilePosition { x, y };
    if plan.post == Some(at) {
        return Some(TileKind::City(CityTile::ArchivistCore));
    }
    if plan.keep.iter().any(|keep| keep.contains(at)) {
        return None;
    }
    let built = matches!(base, TileKind::Built(_));
    if !carvable(base) && !built {
        return None;
    }
    for (stamp, parts) in plan.stamps.iter().zip(&plan.parts) {
        // Only the Lantern Library cuts through a pilot space's masonry (it must stand whole).
        if built && stamp.kind != CityStampKind::LanternLibrary {
            continue;
        }
        if let Some(kind) = stamp_tile(stamp, parts, x, y) {
            return Some(kind);
        }
    }
    if built {
        return None;
    }
    if let Some(kind) = portal_tile(settings, layout, x, y) {
        return Some(kind);
    }
    let stratum = stratum_in(settings, layout, x, y);
    if !is_city(stratum) {
        return None;
    }
    city_rock(settings, layout, stratum, x, y, base)
}

/// Whether `area` (grown by two tiles) crosses a city set piece (later passes keep their rooms
/// off them).
#[must_use]
pub fn crosses_city_set_pieces(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    let grown = area.expanded(2);
    !stamps_with(settings, layout, &grown).is_empty()
}

// --- Movers -------------------------------------------------------------------------------------------

/// The phase of a mover anchored at `(x, y)` within a period (so neighbouring lines do not run
/// in step).
fn phase(settings: &WorldSettings, x: i32, y: i32, period: u64) -> u64 {
    hash2(settings.seed, SALT_PHASE, x, y) % period.max(1)
}

/// The movers of `stamp` (with its machinery, for engine stamps).
#[must_use]
pub fn stamp_movers(
    settings: &WorldSettings,
    stamp: &CityStamp,
    parts: &EngineParts,
) -> Vec<CityMover> {
    let mut movers = Vec::new();
    match stamp.kind {
        CityStampKind::Avenue if stamp.tram => {
            let stops = stations(stamp);
            let first = stops.first().copied().unwrap_or(stamp.bounds.min.x);
            let mut track = Track {
                stops: stops
                    .iter()
                    .map(|column| i64::from(*column) * MILLI + MILLI / 2)
                    .collect(),
                speed: TRAM_SPEED,
                dwell_millis: TRAM_DWELL_MILLIS,
                phase_millis: 0,
            };
            track.phase_millis = phase(settings, first, stamp.floor, track_period(&track));
            movers.push(CityMover::Tram {
                anchor: MoverRef {
                    kind: MoverKind::Tram,
                    x: first,
                    y: stamp.floor,
                },
                rail_row: stamp.floor,
                track,
            });
        }
        CityStampKind::Tower | CityStampKind::LanternLibrary => {
            let mut track = Track {
                stops: lift_stops(stamp)
                    .iter()
                    .map(|row| i64::from(*row) * MILLI)
                    .collect(),
                speed: LIFT_SPEED,
                dwell_millis: LIFT_DWELL_MILLIS,
                phase_millis: 0,
            };
            track.phase_millis = phase(settings, stamp.shaft, stamp.floor, track_period(&track));
            movers.push(CityMover::Lift {
                anchor: MoverRef {
                    kind: MoverKind::Lift,
                    x: stamp.shaft,
                    y: stamp.floor,
                },
                column: stamp.shaft,
                track,
            });
        }
        CityStampKind::EngineHall
        | CityStampKind::PistonShaft
        | CityStampKind::CrusherLine
        | CityStampKind::GreatEngine => {
            let beat = parts.beat.max(1);
            let offset = phase(settings, stamp.bounds.min.x, stamp.bounds.min.y, beat);
            for &(centre, base, stroke, step) in &parts.pistons {
                let column = stamp.world_x(centre);
                let floor_row = stamp.bounds.min.y + base;
                movers.push(CityMover::Piston {
                    anchor: MoverRef {
                        kind: MoverKind::Piston,
                        x: column,
                        y: floor_row,
                    },
                    column,
                    floor_row,
                    cycle: Cycle {
                        period_millis: beat,
                        phase_millis: (offset + beat * u64::from(step) / 3) % beat,
                        travel: stroke,
                    },
                });
            }
            for &(centre, housing, travel, step) in &parts.crushers {
                let column = stamp.world_x(centre);
                let housing_row = stamp.bounds.min.y + housing;
                movers.push(CityMover::Crusher {
                    anchor: MoverRef {
                        kind: MoverKind::Crusher,
                        x: column,
                        y: housing_row,
                    },
                    column,
                    housing_row,
                    cycle: Cycle {
                        period_millis: beat,
                        phase_millis: (offset + beat * u64::from(step) / 6 + beat / 2) % beat,
                        travel,
                    },
                });
            }
        }
        _ => {}
    }
    movers
}

/// Every mover of the stamps overlapping `area`.
#[must_use]
pub fn movers_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<CityMover> {
    let plan = plan_for(settings, layout, area);
    plan.stamps
        .iter()
        .zip(&plan.parts)
        .flat_map(|(stamp, parts)| stamp_movers(settings, stamp, parts))
        .filter(|mover| mover.swept().intersects(area))
        .collect()
}

/// The mover `reference` names, if it still stands where the generator put it.
#[must_use]
pub fn mover_at(
    settings: &WorldSettings,
    layout: &DescentLayout,
    reference: MoverRef,
) -> Option<CityMover> {
    let at = TilePosition {
        x: reference.x,
        y: reference.y,
    };
    movers_with(settings, layout, &TileRect::around(at, 2, 2))
        .into_iter()
        .find(|mover| mover.anchor() == reference)
}

/// The vault stamp (and its kind) within `reach` tiles of `tile`, if any.
#[must_use]
pub fn vault_near(
    settings: &WorldSettings,
    layout: &DescentLayout,
    tile: TilePosition,
    reach: i32,
) -> Option<(CityStamp, VaultKind)> {
    stamps_with(settings, layout, &TileRect::around(tile, reach, reach))
        .into_iter()
        .find_map(|stamp| match stamp.kind {
            CityStampKind::Vault(kind) => Some((stamp, kind)),
            _ => None,
        })
}

#[cfg(test)]
mod tests;
