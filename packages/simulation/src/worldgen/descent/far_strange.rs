//! The far Strange Layers' generated places (Wave 9 strange-b; Descent VI strata 4-6 and its
//! gate, [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11) only).
//!
//! Pure functions of the settings and the tile (so chunks carve the same in any order), run
//! by `streamed::assemble` after the Old Deep's pass and before the expedition's story pass:
//!
//! * **Stamps** ([`stamps_with`], [`stamp_tile`]): at most one per [`FAR_REGION`] cell, rolled
//!   per stratum and more likely (and harsher) deeper in it, plus fixed ones per world:
//!   * Time Sludge **slow pools** (round caverns rimmed with chronoglass, time crawling inside)
//!     and **fast lanes** (long low tunnels where time races); the "Slow Pools" and "Fast
//!     Lanes" variants lean the mix. The **Stopped Clock** ([`clock_hall`]): a builders' hall
//!     whose floor is striped slow and fast.
//!   * Echo Caves **echo hollows** (dark caverns with echo quartz pillars) and the **Choir**
//!     ([`choir`]): a great hall of ringing quartz.
//!   * Prism Desert **keyed curtains** (a cave split by one or two colour-keyed walls, a cache
//!     behind) and, deeper, **spectrum vaults** (a room inside three nested keyed walls: red,
//!     then green, then blue). The **Rewrite Hall** ([`glitch_arena`]): Descent VI's gate
//!     chamber grown into The Glitch's arena, walled and floored with builder alloy, doors in
//!     both side walls and alloy ledges to stand on whichever way gravity points.
//! * **Rock** ([`far_rock`]): chronoglass veins in the Time Sludge, quartz specks in the Echo
//!   Caves, crystal in the Prism Desert.
//! * **The alloy seal** ([`alloy_seal`]): three ragged rows of builder alloy under the Prism
//!   Desert's boundary, across every column: only the Phase Drill (The Glitch's reward) opens
//!   Descent VII.
//!
//! The time pockets and the clock hall's stripes are also a field source
//! ([`far_field_source`], registered in [`crate::field::STRATUM_FIELD_SOURCES`]): every reader of
//! `fields_at` (rigs, pilots, creatures, drops) feels their time scale.

use crate::{
    cave::CaveTile,
    descent::{Descent, Stratum},
    far_strange::{TimePocketKind, time_pocket_scale},
    field::{self, FieldProfile, ScreenEffect},
    position::Frame,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, ridge, value_noise},
    },
};

use super::{DescentLayout, landmarks, stratum_in, variant};

/// The cells far stamps are rolled per, `(width, height)` in tiles.
pub const FAR_REGION: (i32, i32) = (48, 36);
/// `(width, height)` of the Stopped Clock's hall.
pub const CLOCK_HALL_SIZE: (i32, i32) = (54, 16);
/// Columns per stripe of the clock hall's floor (slow, then fast, ...).
pub const CLOCK_STRIPE: i32 = 6;
/// `(width, height)` of the Choir.
pub const CHOIR_SIZE: (i32, i32) = (64, 24);
/// How far the Rewrite Hall reaches past Descent VI's gate chamber, `(columns, rows)` each side.
pub const ARENA_MARGIN: (i32, i32) = (14, 9);
/// Rows of the alloy seal.
pub const ALLOY_SEAL_ROWS: i32 = 3;

const SALT_REGION: u64 = 0x5B9_0001;
const SALT_SHAPE: u64 = 0x5B9_0002;
const SALT_FIXED: u64 = 0x5B9_0003;
const SALT_DECOR: u64 = 0x5B9_0004;
const SALT_GLASS: u64 = 0x5B9_0005;
const SALT_QUARTZ: u64 = 0x5B9_0006;
const SALT_CRYSTAL: u64 = 0x5B9_0007;

/// What a far stamp is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FarStampKind {
    /// A slow pool or a fast lane (Time Sludge).
    TimePocket(TimePocketKind),
    /// The Stopped Clock (Time Sludge, one per world).
    ClockHall,
    /// An echo hollow (Echo Caves).
    EchoHollow,
    /// The Choir (Echo Caves, one per world).
    Choir,
    /// A cave split by keyed walls (Prism Desert).
    KeyedCurtain,
    /// A room inside three nested keyed walls (Prism Desert, deeper).
    SpectrumVault,
    /// The Rewrite Hall: The Glitch's arena (Descent VI's gate).
    GlitchArena,
}

/// One far set piece.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FarStamp {
    pub kind: FarStampKind,
    /// Every tile it may change.
    pub bounds: TileRect,
    /// Per-stamp variation.
    pub seed: u64,
    /// How deep in its stratum it is, per mille (deeper stamps are bigger and harsher).
    pub along: u16,
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

/// Whether `stratum` is one of the far Strange Layers'.
#[must_use]
pub const fn is_far(stratum: Stratum) -> bool {
    matches!(
        stratum,
        Stratum::TimeSludge | Stratum::EchoCaves | Stratum::PrismDesert
    )
}

/// The rows the far Strange Layers' passes touch (with slack for the ragged boundaries and the
/// alloy seal under them).
#[must_use]
pub const fn carve_rows(layout: &DescentLayout) -> (i32, i32) {
    let top = layout.rows(Stratum::TimeSludge).0 - 40;
    let bottom = layout.rows(Stratum::PrismDesert).1 + 48;
    (top, bottom)
}

/// The side of the spawn column a fixed set piece sits on (away from the outposts' column).
const fn away_side(settings: &WorldSettings) -> i32 {
    if landmarks::outpost_column(settings) >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    }
}

/// The Stopped Clock's hall (one per Deep Descent world, a third of the way down the Time
/// Sludge).
#[must_use]
pub const fn clock_hall(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::TimeSludge);
    let roll = hash2(settings.seed, SALT_FIXED, 1, 0);
    let centre = crate::worldgen::SPAWN_TILE_X + away_side(settings) * roll_range(roll, 0, 60, 110);
    let middle = top + (bottom - top) * 3 / 10 + roll_range(roll, 12, -10, 10);
    Some(rect(
        centre - CLOCK_HALL_SIZE.0 / 2,
        middle - CLOCK_HALL_SIZE.1 / 2,
        CLOCK_HALL_SIZE,
    ))
}

/// The Choir (one per Deep Descent world, halfway down the Echo Caves).
#[must_use]
pub const fn choir(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::EchoCaves);
    let roll = hash2(settings.seed, SALT_FIXED, 2, 0);
    let centre = crate::worldgen::SPAWN_TILE_X - away_side(settings) * roll_range(roll, 0, 50, 100);
    let middle = top + (bottom - top) / 2 + roll_range(roll, 12, -12, 12);
    Some(rect(
        centre - CHOIR_SIZE.0 / 2,
        middle - CHOIR_SIZE.1 / 2,
        CHOIR_SIZE,
    ))
}

/// The Rewrite Hall: Descent VI's gate chamber grown into The Glitch's arena.
#[must_use]
pub fn glitch_arena(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !settings.generator.has_descents() {
        return None;
    }
    let chamber = landmarks::gate_chamber(settings, layout, Descent::StrangeLayers).bounds;
    let (mx, my) = ARENA_MARGIN;
    Some(TileRect {
        min: TilePosition {
            x: chamber.min.x - mx,
            y: chamber.min.y - my,
        },
        max: TilePosition {
            x: chamber.max.x + mx,
            y: chamber.max.y + 2,
        },
    })
}

/// The open inside of the Rewrite Hall (inside its alloy walls, above its floor).
#[must_use]
pub const fn arena_inside(arena: &TileRect) -> TileRect {
    TileRect {
        min: TilePosition {
            x: arena.min.x + 2,
            y: arena.min.y + 2,
        },
        max: TilePosition {
            x: arena.max.x - 2,
            y: arena.max.y - 3,
        },
    }
}

/// The fixed stamps (the clock hall, the Choir, the Rewrite Hall).
fn fixed_stamps(settings: &WorldSettings, layout: &DescentLayout) -> Vec<FarStamp> {
    let mut stamps = Vec::new();
    if let Some(bounds) = clock_hall(settings, layout) {
        stamps.push(FarStamp {
            kind: FarStampKind::ClockHall,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -1, 0),
            along: 300,
        });
    }
    if let Some(bounds) = choir(settings, layout) {
        stamps.push(FarStamp {
            kind: FarStampKind::Choir,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -2, 0),
            along: 500,
        });
    }
    if let Some(bounds) = glitch_arena(settings, layout) {
        stamps.push(FarStamp {
            kind: FarStampKind::GlitchArena,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -3, 0),
            along: 1_000,
        });
    }
    stamps
}

/// The stamp of region `(rx, ry)`, if it rolled one.
#[allow(clippy::cast_possible_wrap, reason = "per mille")]
fn region_stamp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    fixed: &[FarStamp],
    rx: i32,
    ry: i32,
) -> Option<FarStamp> {
    let (width, height) = FAR_REGION;
    let centre_row = ry * height + height / 2;
    let stratum = layout.stratum_of_row(centre_row);
    if !is_far(stratum) {
        return None;
    }
    let along = u32::from(layout.along(centre_row));
    let deep = along as i32;
    let roll = hash2(settings.seed, SALT_REGION, rx, ry);
    let pick = variant(settings, stratum);
    // (kind, chance per mille at the stratum's top, extra by its bottom, size ranges)
    let (kind, base, extra, w, h) = match stratum {
        Stratum::TimeSludge => {
            // "Slow Pools" leans slow, "Fast Lanes" leans fast.
            let slow_share = if pick == 0 { 640 } else { 360 };
            if (roll >> 50) % 1_000 < slow_share {
                (
                    FarStampKind::TimePocket(TimePocketKind::Slow),
                    300,
                    420,
                    (14 + deep / 100, 22 + deep / 80),
                    (10 + deep / 200, 14 + deep / 120),
                )
            } else {
                (
                    FarStampKind::TimePocket(TimePocketKind::Fast),
                    300,
                    420,
                    (28, 40),
                    (6, 8),
                )
            }
        }
        Stratum::EchoCaves => (
            FarStampKind::EchoHollow,
            if pick == 0 { 380 } else { 300 },
            260,
            (20, 36),
            (10, 18),
        ),
        Stratum::PrismDesert => {
            if along > 450 && (roll >> 52) % 1_000 < u64::from(260 + along / 4) {
                (FarStampKind::SpectrumVault, 220, 300, (16, 18), (10, 11))
            } else {
                (
                    FarStampKind::KeyedCurtain,
                    if pick == 1 { 420 } else { 330 },
                    300,
                    (20, 30),
                    (9, 13),
                )
            }
        }
        _ => return None,
    };
    let chance = u64::from(base + extra * along / 1_000);
    if roll % 1_000 >= chance {
        return None;
    }
    let sw = roll_range(roll, 10, w.0, w.1.min(width - 8));
    let sh = roll_range(roll, 20, h.0, h.1.min(height - 6));
    let x = rx * width + roll_range(roll, 30, 3, (width - sw - 3).max(3));
    let y = ry * height + roll_range(roll, 40, 3, (height - sh - 3).max(3));
    let bounds = rect(x, y, (sw, sh));
    // Whole stamps inside their stratum's rows (clear of the ragged boundaries).
    let (top, bottom) = layout.rows(stratum);
    if bounds.min.y < top + 16 || bounds.max.y > bottom - 16 {
        return None;
    }
    if fixed
        .iter()
        .any(|stamp| stamp.bounds.expanded(6).intersects(&bounds))
    {
        return None;
    }
    // Keep the outposts and the builders' cable clear.
    let column = landmarks::outpost_column(settings);
    if bounds.max.x >= column - 24 && bounds.min.x <= column + 24 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, reason = "per mille")]
    Some(FarStamp {
        kind,
        bounds,
        seed: hash2(settings.seed, SALT_SHAPE, rx, ry),
        along: along as u16,
    })
}

/// Every far stamp whose bounds overlap `area` (fixed stamps first, then by region). Empty for
/// worlds without Descents.
#[must_use]
pub fn stamps_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<FarStamp> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let (first, last) = carve_rows(layout);
    if area.max.y < first || area.min.y > last {
        return Vec::new();
    }
    let fixed = fixed_stamps(settings, layout);
    let mut found: Vec<FarStamp> = fixed
        .iter()
        .copied()
        .filter(|stamp| stamp.bounds.intersects(area))
        .collect();
    let (width, height) = FAR_REGION;
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(stamp) = region_stamp(settings, layout, &fixed, rx, ry)
                && stamp.bounds.intersects(area)
            {
                found.push(stamp);
            }
        }
    }
    found
}

/// The far stamp holding tile `(x, y)`, if any (field lookups: only the tile's own region and
/// the fixed stamps can hold it).
#[must_use]
pub fn stamp_at(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
) -> Option<FarStamp> {
    let at = TilePosition { x, y };
    stamps_with(settings, layout, &TileRect { min: at, max: at })
        .into_iter()
        .find(|stamp| stamp.bounds.contains(at))
}

/// Whether a stamp may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Poi(_) | TileKind::Bedrock | TileKind::Fluid(_)
    )
}

/// Whether `kind` is plain rock the rock pass may dress.
const fn plain_rock(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

/// `(centre x, centre y, half width, half height)` of the ellipse inscribed in `bounds`.
fn ellipse(bounds: &TileRect) -> (i32, i32, i32, i32) {
    let rx = (bounds.max.x - bounds.min.x + 1) / 2;
    let ry = (bounds.max.y - bounds.min.y + 1) / 2;
    (bounds.min.x + rx, bounds.min.y + ry, rx.max(1), ry.max(1))
}

/// How far `(x, y)` is inside the ellipse, per mille of its radius squared (below 1000 inside).
fn ellipse_permille(bounds: &TileRect, x: i32, y: i32) -> i64 {
    let (cx, cy, rx, ry) = ellipse(bounds);
    let dx = i64::from(x - cx) * 1_000 / i64::from(rx);
    let dy = i64::from(y - cy) * 1_000 / i64::from(ry);
    (dx * dx + dy * dy) / 1_000
}

/// The tile `stamp` puts at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn stamp_tile(stamp: &FarStamp, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    // The Rewrite Hall clears everything but bedrock and built pieces (a derelict's wreckage
    // or a pool inside the boss arena would wall its fight in).
    let clears = stamp.kind == FarStampKind::GlitchArena
        && !matches!(base, TileKind::Built(_) | TileKind::Bedrock);
    if !stamp.bounds.contains(at) || !(carvable(base) || clears) {
        return None;
    }
    match stamp.kind {
        FarStampKind::TimePocket(TimePocketKind::Slow) => slow_pool(stamp, x, y),
        FarStampKind::TimePocket(TimePocketKind::Fast) => fast_lane(stamp, x, y),
        FarStampKind::ClockHall => Some(clock_hall_tile(stamp, x, y)),
        FarStampKind::EchoHollow => echo_hollow(stamp, x, y, false),
        FarStampKind::Choir => echo_hollow(stamp, x, y, true),
        FarStampKind::KeyedCurtain => keyed_curtain(stamp, x, y),
        FarStampKind::SpectrumVault => Some(spectrum_vault(stamp, x, y)),
        FarStampKind::GlitchArena => Some(arena_tile(stamp, x, y)),
    }
}

/// Whether `(x, y)` is open inside a time pocket (where its field applies).
#[must_use]
pub fn pocket_open(stamp: &FarStamp, x: i32, y: i32) -> bool {
    matches!(
        stamp_tile(stamp, x, y, TileKind::Stone),
        Some(TileKind::Air)
    )
}

/// A slow pool: a round cavern rimmed with chronoglass, still air inside.
fn slow_pool(stamp: &FarStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    if inside >= 800 {
        // The rim: chronoglass, broken here and there to plain rock.
        return hash2(stamp.seed, SALT_DECOR, x, y)
            .is_multiple_of(3)
            .then_some(TileKind::Cave(CaveTile::Chronoglass));
    }
    let (_, cy, _, ry) = ellipse(&stamp.bounds);
    // A chronoglass floor to stand on, a hanging frozen drip or two.
    if y > cy + ry * 6 / 10 {
        return Some(TileKind::Cave(CaveTile::Chronoglass));
    }
    if inside < 120 && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(29) {
        return Some(TileKind::Cave(CaveTile::Chronoglass));
    }
    Some(TileKind::Air)
}

/// A fast lane: a long low tunnel with a ribbed floor.
fn fast_lane(stamp: &FarStamp, x: i32, y: i32) -> Option<TileKind> {
    let b = stamp.bounds;
    // Rounded ends.
    let from_end = (x - b.min.x).min(b.max.x - x);
    let shrink = (3 - from_end).max(0);
    if y < b.min.y + 1 + shrink || y > b.max.y - 1 - shrink {
        return None;
    }
    if y == b.max.y - 1 - shrink && (x - b.min.x).rem_euclid(5) == 0 {
        // Ribs on the floor: the lane's markers (chronoglass studs).
        return Some(TileKind::Cave(CaveTile::Chronoglass));
    }
    Some(TileKind::Air)
}

/// The Stopped Clock: a builders' hall of brick, glyphs every few columns, a striped floor.
const fn clock_hall_tile(stamp: &FarStamp, x: i32, y: i32) -> TileKind {
    let b = stamp.bounds;
    let wall = y == b.min.y || y == b.max.y || x == b.min.x || x == b.max.x;
    let door = (x == b.min.x || x == b.max.x) && y >= b.max.y - 3 && y < b.max.y;
    if wall && !door {
        return if (x - b.min.x).rem_euclid(CLOCK_STRIPE) == 0 {
            TileKind::Cave(CaveTile::RuinGlyph)
        } else {
            TileKind::Cave(CaveTile::RuinBrick)
        };
    }
    // Pillars between the stripes, short of the floor.
    let column = (x - b.min.x).rem_euclid(CLOCK_STRIPE * 3);
    if column == 0 && y < b.min.y + 3 {
        return TileKind::Cave(CaveTile::RuinPillar);
    }
    // Studs of chronoglass in the floor mark the stripes.
    if y == b.max.y - 1 && (x - b.min.x).rem_euclid(CLOCK_STRIPE) == 0 {
        return TileKind::Cave(CaveTile::Chronoglass);
    }
    TileKind::Air
}

/// Which time a column of the clock hall keeps (stripes of [`CLOCK_STRIPE`]).
#[must_use]
pub const fn clock_stripe(hall: &TileRect, x: i32) -> TimePocketKind {
    if (x - hall.min.x).div_euclid(CLOCK_STRIPE) % 2 == 0 {
        TimePocketKind::Slow
    } else {
        TimePocketKind::Fast
    }
}

/// An echo hollow (or the Choir): a dark cavern of echo quartz pillars.
fn echo_hollow(stamp: &FarStamp, x: i32, y: i32, choir: bool) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (cx, cy, rx, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 6 / 10;
    if inside >= 860 {
        // The rim glitters with quartz in the Choir.
        return (choir && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(4))
            .then_some(TileKind::Cave(CaveTile::EchoQuartz));
    }
    if y > floor {
        return None;
    }
    // Pillars: floor to ceiling, two wide, evenly spaced.
    let spacing = if choir { 9 } else { (rx * 2 / 3).max(6) };
    let offset = (x - (cx - rx)).rem_euclid(spacing);
    let roll = hash2(
        stamp.seed,
        SALT_DECOR,
        (x - (cx - rx)).div_euclid(spacing),
        0,
    );
    let pillar = offset < 2 && (choir || roll.is_multiple_of(2));
    if pillar && (x - cx).abs() < rx - 3 {
        return Some(TileKind::Cave(CaveTile::EchoQuartz));
    }
    // Stalactites in the dark.
    if y == cy - ry * 6 / 10 && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(7) {
        return Some(TileKind::Cave(CaveTile::Stalactite));
    }
    Some(TileKind::Air)
}

/// The colour of keyed wall number `index` of `stamp`.
const fn keyed_colour(seed: u64, index: u64) -> CaveTile {
    match (seed >> 8).wrapping_add(index) % 3 {
        0 => CaveTile::PrismRed,
        1 => CaveTile::PrismGreen,
        _ => CaveTile::PrismBlue,
    }
}

/// A keyed curtain: a cave cut by one keyed wall (two, deeper), a cache behind the last.
fn keyed_curtain(stamp: &FarStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (cx, cy, rx, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 6 / 10;
    if y > floor {
        // Crystal sand floor.
        return Some(TileKind::Cave(CaveTile::Crystal));
    }
    let side = if stamp.seed.is_multiple_of(2) { 1 } else { -1 };
    let walls = if stamp.along > 500 { 2 } else { 1 };
    for index in 0..walls {
        let wall_x = cx + side * (rx / 3 + index * 4);
        if x == wall_x || x == wall_x + side {
            #[allow(clippy::cast_sign_loss, reason = "a small non-negative index")]
            return Some(TileKind::Cave(keyed_colour(stamp.seed, index as u64)));
        }
    }
    // The prize behind the last curtain.
    let far = cx + side * (rx - 3);
    if x == far && y == floor {
        return Some(TileKind::Cave(if stamp.along > 650 {
            CaveTile::RelicCache
        } else {
            CaveTile::TreasureCache
        }));
    }
    if (x - far).abs() <= 1 && y == floor - 1 && stamp.seed.is_multiple_of(3) {
        return Some(TileKind::Ore(MineralKind::Diamond));
    }
    Some(TileKind::Air)
}

/// A spectrum vault: three nested keyed walls (red outside, then green, then blue) round a
/// room with a cache; plain rock between the rings is left as rock so the way in is the walls.
fn spectrum_vault(stamp: &FarStamp, x: i32, y: i32) -> TileKind {
    let b = stamp.bounds;
    let ring = (x - b.min.x)
        .min(b.max.x - x)
        .min(y - b.min.y)
        .min(b.max.y - y);
    match ring {
        0 => TileKind::Cave(CaveTile::PrismRed),
        2 => TileKind::Cave(CaveTile::PrismGreen),
        4 => TileKind::Cave(CaveTile::PrismBlue),
        1 | 3 => TileKind::Air,
        _ => {
            let centre = b.center();
            if y == b.max.y - 5 && x == centre.x {
                TileKind::Cave(CaveTile::RelicCache)
            } else if y == b.max.y - 5 && (x - centre.x).abs() == 2 {
                TileKind::Ore(MineralKind::Diamond)
            } else {
                TileKind::Air
            }
        }
    }
}

/// The Rewrite Hall: builder alloy walls two thick, an alloy floor, doors in both side walls,
/// alloy ledges (a floor whichever way gravity points), glyphs in the walls.
fn arena_tile(stamp: &FarStamp, x: i32, y: i32) -> TileKind {
    let hall = stamp.bounds;
    let inside = arena_inside(&hall);
    let at = TilePosition { x, y };
    if !inside.contains(at) {
        // Doorways three rows tall at the floor in both side walls.
        let side = x < inside.min.x || x > inside.max.x;
        if side && y >= inside.max.y - 2 && y <= inside.max.y {
            return TileKind::Air;
        }
        // Glyphs every eighth tile of the walls: the rules written on them.
        if (x + y).rem_euclid(8) == 0 && y > hall.min.y && y < hall.max.y {
            return TileKind::Cave(CaveTile::RuinGlyph);
        }
        return TileKind::Cave(CaveTile::BuilderAlloy);
    }
    // Ledges: a short alloy shelf in each quarter, so a rig has footing whichever way gravity
    // points (the ceiling under gravity-up, a wall under sideways gravity).
    let wide = inside.width();
    let tall = inside.height();
    let ledges = [
        (inside.min.x + wide / 5, inside.min.y + tall / 3),
        (inside.max.x - wide / 5, inside.min.y + tall / 3),
        (inside.min.x + wide / 2, inside.min.y + tall * 2 / 3),
    ];
    for (lx, ly) in ledges {
        if y == ly && (x - lx).abs() <= 3 {
            return TileKind::Cave(CaveTile::BuilderAlloy);
        }
    }
    let _ = stamp.seed;
    TileKind::Air
}

/// Far rock: the tile the plain rock `base` at `(x, y)` of `stratum` becomes (chronoglass veins,
/// quartz specks, crystal), or `None` to leave it.
#[must_use]
pub fn far_rock(
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
        Stratum::TimeSludge => {
            let line = ridge(value_noise(seed, SALT_GLASS, x, y, 16, 9));
            (line > NOISE_MAX - 1_400 - along * 2).then_some(TileKind::Cave(CaveTile::Chronoglass))
        }
        Stratum::EchoCaves => hash2(seed, SALT_QUARTZ, x, y)
            .is_multiple_of(97)
            .then_some(TileKind::Cave(CaveTile::EchoQuartz)),
        Stratum::PrismDesert => {
            let line = ridge(value_noise(seed, SALT_CRYSTAL, x, y, 10, 14));
            (line > NOISE_MAX - 1_100 - along).then_some(TileKind::Cave(CaveTile::Crystal))
        }
        _ => None,
    }
}

/// Whether `(x, y)` is in the alloy seal: three ragged rows of builder alloy just under the
/// Prism Desert's boundary zone, across every column (Descent VII's door: the Phase Drill).
#[must_use]
pub const fn alloy_seal(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let index = Stratum::PrismDesert.index();
    let (_, half) = super::boundary_shape(index);
    let top = super::boundary_column_depth(settings, layout, index, x) + half;
    y >= top && y < top + ALLOY_SEAL_ROWS
}

/// The far Strange Layers' tile at `(x, y)` over `base`: a stamp's tile, the alloy seal, else
/// far rock, else `None` (leave `base`). `stamps` are the chunk's [`stamps_with`].
#[must_use]
pub fn far_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stamps: &[FarStamp],
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    for stamp in stamps {
        if let Some(kind) = stamp_tile(stamp, x, y, base) {
            return Some(kind);
        }
    }
    if carvable(base) && alloy_seal(settings, layout, x, y) {
        return Some(TileKind::Cave(CaveTile::BuilderAlloy));
    }
    let stratum = stratum_in(settings, layout, x, y);
    if !is_far(stratum) {
        return None;
    }
    far_rock(settings, layout, stratum, x, y, base)
}

// --- The time field ---------------------------------------------------------------------------

/// The field inside a time pocket of `kind` at `along` per mille through the Time Sludge, over
/// the stratum's own profile `base`.
#[must_use]
pub fn pocket_profile(base: FieldProfile, kind: TimePocketKind, along: u16) -> FieldProfile {
    let mut inside = base;
    inside.time_scale = time_pocket_scale(kind, along);
    inside.screen = match kind {
        // Slow: honey-gold, a little drained, edges heavy.
        TimePocketKind::Slow => inside
            .screen
            .with(ScreenEffect::ColourGrade, 46)
            .with(ScreenEffect::Desaturate, 22)
            .with(ScreenEffect::Vignette, 30)
            .graded([255, 186, 92]),
        // Fast: a cold blue-white wash and a shiver.
        TimePocketKind::Fast => inside
            .screen
            .with(ScreenEffect::ColourGrade, 38)
            .with(ScreenEffect::Wobble, 16)
            .with(ScreenEffect::Bloom, 18)
            .graded([150, 214, 255]),
    };
    inside
}

/// The registered far field source: the time pockets' and the clock hall's time scale; `None`
/// everywhere else (the stratum source decides).
#[must_use]
pub fn far_field_source(
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> Option<FieldProfile> {
    if !settings.generator.has_descents() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame origins of a 13,000-row world are small"
    )]
    let (column, row) = (
        frame.origin_col() as f32 + x / TILE_SIZE,
        frame.origin_row() as f32 + y / TILE_SIZE,
    );
    #[allow(clippy::cast_possible_truncation, reason = "tile coordinates fit i32")]
    let (tx, ty) = (column.floor() as i32, row.floor() as i32);
    let (top, bottom) = layout.rows(Stratum::TimeSludge);
    if ty < top || ty > bottom {
        return None;
    }
    let stamp = stamp_at(settings, &layout, tx, ty)?;
    let kind = match stamp.kind {
        FarStampKind::TimePocket(kind) if pocket_open(&stamp, tx, ty) => kind,
        FarStampKind::ClockHall if arena_like_inside(&stamp.bounds, tx, ty) => {
            clock_stripe(&stamp.bounds, tx)
        }
        _ => return None,
    };
    let base = crate::descent::fields::stratum_field_source(settings, frame, x, y)
        .unwrap_or(field::FieldProfile::NEUTRAL);
    // Feather: a tile in from the pocket's open edge the field is whole.
    let weight = edge_weight(&stamp, tx, ty);
    Some(base.lerp(&pocket_profile(base, kind, layout.along(ty)), weight))
}

/// Whether `(x, y)` is inside a walled hall's walls.
const fn arena_like_inside(hall: &TileRect, x: i32, y: i32) -> bool {
    x > hall.min.x && x < hall.max.x && y > hall.min.y && y < hall.max.y
}

/// How whole a pocket's field is at `(x, y)`: full a tile in from its open edge.
fn edge_weight(stamp: &FarStamp, x: i32, y: i32) -> f32 {
    let open = |dx: i32, dy: i32| match stamp.kind {
        FarStampKind::ClockHall => arena_like_inside(&stamp.bounds, x + dx, y + dy),
        _ => pocket_open(stamp, x + dx, y + dy),
    };
    let neighbours = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter(|(dx, dy)| open(*dx, *dy))
        .count();
    if neighbours == 4 { 1.0 } else { 0.6 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::{GeneratorVersion, WorldSettings};

    fn settings() -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::default()
        }
    }

    #[test]
    fn every_far_stratum_rolls_its_own_stamps() {
        let settings = settings();
        let layout = super::super::layout(&settings);
        for (stratum, wanted) in [
            (
                Stratum::TimeSludge,
                FarStampKind::TimePocket(TimePocketKind::Slow),
            ),
            (
                Stratum::TimeSludge,
                FarStampKind::TimePocket(TimePocketKind::Fast),
            ),
            (Stratum::EchoCaves, FarStampKind::EchoHollow),
            (Stratum::PrismDesert, FarStampKind::KeyedCurtain),
            (Stratum::PrismDesert, FarStampKind::SpectrumVault),
        ] {
            let (top, bottom) = layout.rows(stratum);
            let area = TileRect {
                min: TilePosition { x: -600, y: top },
                max: TilePosition { x: 600, y: bottom },
            };
            let stamps = stamps_with(&settings, &layout, &area);
            assert!(
                stamps.iter().any(|stamp| stamp.kind == wanted),
                "{stratum:?} has no {wanted:?}"
            );
        }
        assert!(clock_hall(&settings, &layout).is_some());
        assert!(choir(&settings, &layout).is_some());
        let arena = glitch_arena(&settings, &layout).expect("arena");
        assert_eq!(
            layout.stratum_of_row(arena.center().y),
            Stratum::PrismDesert
        );
        // Older worlds have none of it.
        let classic = WorldSettings::default();
        assert!(clock_hall(&classic, &layout).is_none());
    }

    #[test]
    fn the_alloy_seal_crosses_every_column_under_the_prism_desert() {
        let settings = settings();
        let layout = super::super::layout(&settings);
        for x in (-400..400).step_by(37) {
            let rows: Vec<i32> = (layout.rows(Stratum::PrismDesert).1 - 60
                ..layout.rows(Stratum::PrismDesert).1 + 80)
                .filter(|y| alloy_seal(&settings, &layout, x, *y))
                .collect();
            assert_eq!(
                rows.len(),
                usize::try_from(ALLOY_SEAL_ROWS).unwrap_or(0),
                "column {x}"
            );
            assert_eq!(
                super::super::stratum_in(&settings, &layout, x, rows[0]).descent(),
                Descent::Ancients
            );
        }
    }

    #[test]
    fn a_spectrum_vault_nests_red_green_blue_round_its_cache() {
        let stamp = FarStamp {
            kind: FarStampKind::SpectrumVault,
            bounds: rect(0, 0, (16, 11)),
            seed: 1,
            along: 800,
        };
        assert_eq!(
            stamp_tile(&stamp, 0, 5, TileKind::Stone),
            Some(TileKind::Cave(CaveTile::PrismRed))
        );
        assert_eq!(
            stamp_tile(&stamp, 2, 5, TileKind::Stone),
            Some(TileKind::Cave(CaveTile::PrismGreen))
        );
        assert_eq!(
            stamp_tile(&stamp, 4, 5, TileKind::Stone),
            Some(TileKind::Cave(CaveTile::PrismBlue))
        );
        let cache = (0..16)
            .flat_map(|x| (0..11).map(move |y| (x, y)))
            .filter(|(x, y)| {
                stamp_tile(&stamp, *x, *y, TileKind::Stone)
                    == Some(TileKind::Cave(CaveTile::RelicCache))
            })
            .count();
        assert_eq!(cache, 1);
    }

    #[test]
    #[allow(
        clippy::cast_precision_loss,
        reason = "tile coordinates of a test world"
    )]
    fn pockets_answer_their_time_and_nothing_outside() {
        let settings = settings();
        let layout = super::super::layout(&settings);
        let (top, bottom) = layout.rows(Stratum::TimeSludge);
        let area = TileRect {
            min: TilePosition { x: -600, y: top },
            max: TilePosition { x: 600, y: bottom },
        };
        let frame = Frame::default();
        let mut slow = false;
        let mut fast = false;
        for stamp in stamps_with(&settings, &layout, &area) {
            let FarStampKind::TimePocket(kind) = stamp.kind else {
                continue;
            };
            let centre = stamp.bounds.center();
            let (cx, cy) = (
                (centre.x as f32 + 0.5) * TILE_SIZE,
                (centre.y as f32 + 0.5) * TILE_SIZE,
            );
            if !pocket_open(&stamp, centre.x, centre.y) {
                continue;
            }
            let field = far_field_source(&settings, frame, cx, cy).expect("pocket field");
            match kind {
                TimePocketKind::Slow => {
                    slow = true;
                    assert!(field.time_scale < 0.7);
                }
                TimePocketKind::Fast => {
                    fast = true;
                    assert!(field.time_scale > 1.15);
                }
            }
        }
        assert!(slow && fast);
        // Above the Time Sludge: no answer.
        let above = (top - 50) as f32 * TILE_SIZE;
        assert!(far_field_source(&settings, frame, 0.0, above).is_none());
    }
}
