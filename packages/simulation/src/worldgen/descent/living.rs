//! The Living Dark's generated places (Wave 8, Descent III of
//! [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)): the set pieces and the
//! living rock of its five strata.
//!
//! Two passes, both pure functions of the settings and the tile (so chunks carve the same in
//! any order), run by `streamed::assemble` after the Descent landmarks and before the
//! logistics sites:
//!
//! * **Stamps** ([`stamps_with`], [`stamp_tile`]): one per [`LIVING_REGION`] cell at most,
//!   rolled per stratum and more likely deeper in it ("introduced gently, then pushed"), plus
//!   three fixed ones per world:
//!   * Fungal Forest **cap groves**: a mycelium-floored cavern of giant mushrooms whose caps
//!     bounce a rig, spore pods hanging under the caps; deeper groves are taller, so climbing
//!     out means bouncing cap to cap.
//!   * Root Labyrinth **heartroots**: a chamber round a taproot whose roots spread into the
//!     walls, a knot of diamond at its heart.
//!   * The Hive's **brood galleries** (the Swarm's comb halls, jelly in their walls), the
//!     Amber Court's **court hall** ([`court_hall`], the friendly faction's settlement, one
//!     per world) and the **matriarch's gallery** ([`matriarch_gallery`], the Swarm's
//!     mini-boss).
//!   * Flesh Caverns **gullets**: chambers with an empty basin and acid glands in its walls
//!     that pump the acid tide in and drink it back ([`crate::living_dark`]).
//!   * Glow Sea **abyssal halls**: great dark caverns (the fluid pass pools water in them).
//!   * The **Mind's hollow** ([`mind_hollow`]): Descent III's gate chamber overgrown into a
//!     cavern of mycelium round the builders' pillars, where the Mycelial Mind waits.
//! * **Living rock** ([`living_rock`]): mycelium veins in the Fungal Forest, wandering root
//!   lines in the Root Labyrinth (denser deeper, vertical taproots in the "Taproot Shafts"
//!   variant), comb seams in the Hive. The Flesh Caverns' rock is flesh itself (the
//!   registry's soft rock).
//!
//! Only plain rock, ore and hazards are carved; built pieces, points of interest, bedrock and
//! fluid stay.

use crate::{
    cave::CaveTile,
    descent::{Descent, Stratum},
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, ridge, value_noise},
    },
};

use super::{DescentLayout, landmarks, stratum_in, variant};

/// The cells living stamps are rolled per, `(width, height)` in tiles.
pub const LIVING_REGION: (i32, i32) = (56, 40);
/// `(width, height)` of the Amber Court's hall.
pub const COURT_SIZE: (i32, i32) = (64, 18);
/// `(width, height)` of the matriarch's gallery.
pub const MATRIARCH_SIZE: (i32, i32) = (40, 14);
/// How far the Mind's hollow reaches past its gate chamber, `(columns, rows)` each side.
pub const MIND_HOLLOW_MARGIN: (i32, i32) = (10, 8);

const SALT_REGION: u64 = 0x11D4_0001;
const SALT_SHAPE: u64 = 0x11D4_0002;
const SALT_MYCELIUM: u64 = 0x11D4_0003;
const SALT_ROOTS: u64 = 0x11D4_0004;
const SALT_COMB: u64 = 0x11D4_0005;
const SALT_COURT: u64 = 0x11D4_0006;
const SALT_DECOR: u64 = 0x11D4_0007;
const SALT_TAPROOT: u64 = 0x11D4_0008;

/// What a living stamp is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LivingStampKind {
    CapGrove,
    Heartroot,
    BroodGallery,
    Court,
    MatriarchGallery,
    Gullet,
    AbyssHall,
    MindHollow,
}

/// One living set piece.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LivingStamp {
    pub kind: LivingStampKind,
    /// Every tile it may change.
    pub bounds: TileRect,
    /// Per-stamp variation.
    pub seed: u64,
    /// How deep in its stratum it is, per mille (deeper stamps are bigger and harsher).
    pub along: u16,
}

impl LivingStamp {
    /// The floor row of a gullet's basin and the columns of its glands (the acid tide's
    /// sources), or `None` for other stamps.
    #[must_use]
    pub fn gullet_glands(&self) -> Option<(i32, [i32; 2])> {
        if self.kind != LivingStampKind::Gullet {
            return None;
        }
        let (cx, _, rx, ry) = ellipse(&self.bounds);
        let floor = self.bounds.min.y + ry * 2 - 1;
        let reach = rx - 2;
        Some((floor - 1, [cx - reach, cx + reach]))
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

/// Whether the world has the Living Dark (a Deep Descent world).
#[must_use]
pub const fn has_living_dark(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// Whether `stratum` is one of the Living Dark's.
#[must_use]
pub fn is_living(stratum: Stratum) -> bool {
    stratum.descent() == Descent::LivingDark
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

/// The side of the spawn the Court and the matriarch live on: opposite the outposts' column,
/// so the Hive is its own place.
const fn hive_side(settings: &WorldSettings) -> i32 {
    if landmarks::outpost_column(settings) >= SPAWN_TILE_X {
        -1
    } else {
        1
    }
}

/// The Amber Court's hall: the friendly faction's settlement in the middle of the Hive (one per
/// Deep Descent world).
#[must_use]
pub const fn court_hall(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_living_dark(settings) {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::TheHive);
    let roll = hash2(settings.seed, SALT_COURT, 0, 0);
    let side = hive_side(settings);
    let centre = SPAWN_TILE_X + side * roll_range(roll, 0, 70, 120);
    let middle = top + (bottom - top) * 2 / 5 + roll_range(roll, 12, -12, 12);
    Some(rect(
        centre - COURT_SIZE.0 / 2,
        middle - COURT_SIZE.1 / 2,
        COURT_SIZE,
    ))
}

/// The Swarm's brood matriarch's gallery: past the Court on the same side, deeper in the Hive.
#[must_use]
pub fn matriarch_gallery(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    let court = court_hall(settings, layout)?;
    let (top, bottom) = layout.rows(Stratum::TheHive);
    let roll = hash2(settings.seed, SALT_COURT, 1, 0);
    let side = hive_side(settings);
    let centre = court.center().x + side * roll_range(roll, 0, 110, 160);
    let row = top + (bottom - top) * 7 / 10 + roll_range(roll, 12, -10, 10);
    Some(rect(
        centre - MATRIARCH_SIZE.0 / 2,
        row - MATRIARCH_SIZE.1 / 2,
        MATRIARCH_SIZE,
    ))
}

/// The Mind's hollow: Descent III's gate chamber overgrown into a cavern (the boss arena).
#[must_use]
pub fn mind_hollow(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_living_dark(settings) {
        return None;
    }
    let chamber = landmarks::gate_chamber(settings, layout, Descent::LivingDark).bounds;
    let (mx, my) = MIND_HOLLOW_MARGIN;
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

/// The floor row of the Mind's hollow (its mycelium floor; the last open row is above it).
#[must_use]
pub fn mind_floor_row(hollow: &TileRect) -> i32 {
    let (_, cy, _, ry) = ellipse(hollow);
    cy + ry * 7 / 10
}

/// The fixed stamps (the Court, the matriarch's gallery, the Mind's hollow).
fn fixed_stamps(settings: &WorldSettings, layout: &DescentLayout) -> Vec<LivingStamp> {
    let mut stamps = Vec::new();
    if let Some(bounds) = court_hall(settings, layout) {
        stamps.push(LivingStamp {
            kind: LivingStampKind::Court,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -1, 0),
            along: 400,
        });
    }
    if let Some(bounds) = matriarch_gallery(settings, layout) {
        stamps.push(LivingStamp {
            kind: LivingStampKind::MatriarchGallery,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -2, 0),
            along: 700,
        });
    }
    if let Some(bounds) = mind_hollow(settings, layout) {
        stamps.push(LivingStamp {
            kind: LivingStampKind::MindHollow,
            bounds,
            seed: hash2(settings.seed, SALT_SHAPE, -3, 0),
            along: 1_000,
        });
    }
    stamps
}

/// The stamp of region `(rx, ry)`, if it rolled one.
fn region_stamp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    fixed: &[LivingStamp],
    rx: i32,
    ry: i32,
) -> Option<LivingStamp> {
    let (width, height) = LIVING_REGION;
    let centre_row = ry * height + height / 2;
    let stratum = layout.stratum_of_row(centre_row);
    if !is_living(stratum) {
        return None;
    }
    let along = u32::from(layout.along(centre_row));
    let roll = hash2(settings.seed, SALT_REGION, rx, ry);
    let pick = variant(settings, stratum);
    // (kind, chance per mille at the stratum's top, extra by its bottom, size ranges)
    let (kind, base, extra, w, h) = match stratum {
        Stratum::FungalForest => (
            LivingStampKind::CapGrove,
            if pick == 0 { 320 } else { 240 },
            520,
            (22 + (along as i32) / 80, 34 + (along as i32) / 60),
            (12 + (along as i32) / 120, 16 + (along as i32) / 70),
        ),
        Stratum::RootLabyrinth => (LivingStampKind::Heartroot, 220, 260, (18, 26), (14, 20)),
        Stratum::TheHive => (
            LivingStampKind::BroodGallery,
            if pick == 0 { 300 } else { 360 },
            200,
            (24, 34),
            (9, 12),
        ),
        Stratum::FleshCaverns => (
            LivingStampKind::Gullet,
            if pick == 1 { 420 } else { 320 },
            220,
            (18, 28),
            (12, 16),
        ),
        Stratum::GlowSea => (
            LivingStampKind::AbyssHall,
            if pick == 1 { 460 } else { 380 },
            180,
            (30, 44),
            (16, 24),
        ),
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
    Some(LivingStamp {
        kind,
        bounds,
        seed: hash2(settings.seed, SALT_SHAPE, rx, ry),
        along: along as u16,
    })
}

/// Every living stamp whose bounds overlap `area` (fixed stamps first, then by region). Empty
/// for worlds without Descents.
#[must_use]
pub fn stamps_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<LivingStamp> {
    if !has_living_dark(settings) {
        return Vec::new();
    }
    let (first, last) = layout.descent_rows(Descent::LivingDark);
    if area.max.y < first - 40 || area.min.y > last + 40 {
        return Vec::new();
    }
    let fixed = fixed_stamps(settings, layout);
    let mut found: Vec<LivingStamp> = fixed
        .iter()
        .copied()
        .filter(|stamp| stamp.bounds.intersects(area))
        .collect();
    let (width, height) = LIVING_REGION;
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

/// Whether a stamp may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Poi(_) | TileKind::Bedrock | TileKind::Fluid(_)
    )
}

/// Whether `kind` is plain rock the living rock pass may turn living.
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

/// How far `(x, y)` is inside the ellipse, per mille of its radius (below 1000 inside).
fn ellipse_permille(bounds: &TileRect, x: i32, y: i32) -> i64 {
    let (cx, cy, rx, ry) = ellipse(bounds);
    let dx = i64::from(x - cx) * 1_000 / i64::from(rx);
    let dy = i64::from(y - cy) * 1_000 / i64::from(ry);
    // sqrt-free: compare squared distances (1_000_000 on the rim).
    (dx * dx + dy * dy) / 1_000
}

/// The tile `stamp` puts at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn stamp_tile(stamp: &LivingStamp, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !stamp.bounds.contains(at) || !carvable(base) {
        return None;
    }
    match stamp.kind {
        LivingStampKind::CapGrove => cap_grove(stamp, x, y),
        LivingStampKind::Heartroot => heartroot(stamp, x, y),
        LivingStampKind::BroodGallery | LivingStampKind::MatriarchGallery => {
            comb_gallery(stamp, x, y)
        }
        LivingStampKind::Court => Some(court(stamp, x, y)),
        LivingStampKind::Gullet => gullet(stamp, x, y),
        LivingStampKind::AbyssHall => abyss_hall(stamp, x, y),
        LivingStampKind::MindHollow => mind_hollow_tile(stamp, x, y, base),
    }
}

/// A cap grove: a mycelium-floored cavern of giant mushrooms.
fn cap_grove(stamp: &LivingStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (cx, cy, rx, ry) = ellipse(&stamp.bounds);
    // The rim of the cavern is fungal soil.
    if inside >= 780 {
        return Some(TileKind::Cave(CaveTile::Mycelium));
    }
    // The floor: two rows of mycelium where the ellipse bottoms out, glowcaps on it.
    let floor = cy + ry * 6 / 10;
    if y > floor {
        return Some(TileKind::Cave(CaveTile::Mycelium));
    }
    // Mushrooms: two to four, spaced across the grove, taller deeper.
    let count = 2 + i32::try_from(stamp.seed % 3).unwrap_or(0);
    let spacing = (rx * 2) / (count + 1);
    let tall = i32::from(stamp.along) / 250;
    for index in 0..count {
        let roll = hash2(stamp.seed, SALT_DECOR, index, 0);
        let stem_x = cx - rx + spacing * (index + 1) + roll_range(roll, 0, -1, 1);
        let height = (roll_range(roll, 8, 2, 4) + tall).min(ry);
        let cap_y = floor - height;
        let half = roll_range(roll, 16, 2, 4) + i32::from(stamp.along) / 400;
        // A domed cap: a narrower crown over the full brim.
        if (y == cap_y && (x - stem_x).abs() <= half)
            || (y == cap_y - 1 && (x - stem_x).abs() <= half - 2)
        {
            return Some(TileKind::Cave(CaveTile::MushroomCap));
        }
        if x == stem_x && y > cap_y && y <= floor {
            return Some(TileKind::Cave(CaveTile::MushroomStem));
        }
        // Spore pods hang under the cap's ends.
        if y == cap_y + 1 && (x - stem_x).abs() == half && !roll.is_multiple_of(3) {
            return Some(TileKind::Cave(CaveTile::SporePod));
        }
    }
    if y == floor && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(5) {
        return Some(TileKind::Cave(CaveTile::GlowCap));
    }
    Some(TileKind::Air)
}

/// A heartroot chamber: a taproot through a round chamber, roots spreading into its walls.
fn heartroot(stamp: &LivingStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    let (cx, cy, _, ry) = ellipse(&stamp.bounds);
    if inside >= 1_000 {
        return None;
    }
    // The taproot, swaying a little.
    let sway = (y - cy).signum() * i32::from((y - cy).abs() > ry / 2);
    let core = (x - cx - sway).abs();
    if core <= 1 {
        // A knot of diamond at the heart.
        if (y - cy).abs() <= 1 && core == 0 {
            return Some(TileKind::Ore(MineralKind::Diamond));
        }
        return Some(TileKind::Cave(CaveTile::Root));
    }
    // Branch roots: diagonal lines from the taproot to the walls.
    let dx = (x - cx).abs();
    let dy = y - cy;
    if inside >= 640 || (dx > 2 && (dy - dx / 2).rem_euclid(5) == 0 && inside >= 300) {
        return Some(TileKind::Cave(CaveTile::Root));
    }
    if inside >= 560 && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(4) {
        return Some(TileKind::Cave(CaveTile::Root));
    }
    Some(TileKind::Air)
}

/// A comb gallery (the Swarm's brood halls and the matriarch's gallery): comb walls two thick,
/// ledges of comb, jelly cells in the walls.
fn comb_gallery(stamp: &LivingStamp, x: i32, y: i32) -> Option<TileKind> {
    let bounds = stamp.bounds;
    let width = bounds.width();
    let column = x - bounds.min.x;
    let row = y - bounds.min.y;
    let height = bounds.height();
    // Rounded ends.
    let end = column.min(width - 1 - column);
    let edge_row = row.min(height - 1 - row);
    if end + edge_row < 2 {
        return None;
    }
    let wall = end < 2 || edge_row < 2;
    if wall {
        let cell = hash2(stamp.seed, SALT_DECOR, x, y);
        if cell.is_multiple_of(if stamp.kind == LivingStampKind::MatriarchGallery {
            3
        } else {
            6
        }) {
            return Some(TileKind::Cave(CaveTile::JellyCell));
        }
        return Some(TileKind::Cave(CaveTile::Comb));
    }
    // Ledges: comb shelves every four rows, broken in the middle.
    if row > 2 && row < height - 3 && (row - 2) % 4 == 0 {
        let gap = (column - width / 2).abs() < 4 || (column % 9) < 2;
        if !gap {
            return Some(TileKind::Cave(CaveTile::Comb));
        }
    }
    Some(TileKind::Air)
}

/// The Amber Court: a great comb hall with arches, a royal larder of jelly cells along its
/// ceiling and doors at both ends.
const fn court(stamp: &LivingStamp, x: i32, y: i32) -> TileKind {
    let bounds = stamp.bounds;
    let width = bounds.width();
    let height = bounds.height();
    let column = x - bounds.min.x;
    let row = y - bounds.min.y;
    let wall = column < 2 || column >= width - 2 || row < 2 || row >= height - 2;
    // Doors four rows tall at the floor, both sides.
    let door = (column < 2 || column >= width - 2) && row >= height - 6 && row < height - 2;
    if door {
        return TileKind::Air;
    }
    if wall {
        return TileKind::Cave(CaveTile::Comb);
    }
    // The larder: a row of jelly cells under the ceiling.
    if row == 2 && column % 3 != 0 {
        return TileKind::Cave(CaveTile::JellyCell);
    }
    // Arches: comb pillars every twelve columns with a rounded crown.
    let bay = (column - 2).rem_euclid(12);
    if bay == 0 && row > 3 {
        return TileKind::Cave(CaveTile::Comb);
    }
    let crown = (bay - 6).abs();
    if row == 3 && crown >= 4 {
        return TileKind::Cave(CaveTile::Comb);
    }
    if row == 4 && crown >= 5 {
        return TileKind::Cave(CaveTile::Comb);
    }
    TileKind::Air
}

/// A gullet: a flesh chamber with an empty basin; its glands pump the acid tide.
fn gullet(stamp: &LivingStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    if let Some((row, columns)) = stamp.gullet_glands()
        && y == row
        && columns.contains(&x)
    {
        return Some(TileKind::Cave(CaveTile::AcidGland));
    }
    if inside >= 860 {
        return Some(TileKind::Cave(CaveTile::Flesh));
    }
    Some(TileKind::Air)
}

/// An abyssal hall: a great dark cavern with a ragged floor.
fn abyss_hall(stamp: &LivingStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    let wobble =
        i64::from(value_noise(stamp.seed, SALT_DECOR, x, y, 5, 4)) * 160 / i64::from(NOISE_MAX);
    if inside + wobble >= 1_000 {
        return None;
    }
    Some(TileKind::Air)
}

/// The Mind's hollow: the gate chamber's builders' pillars stand in a mycelium cavern.
fn mind_hollow_tile(stamp: &LivingStamp, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    if base == TileKind::Cave(CaveTile::RuinPillar) {
        return Some(base);
    }
    let floor = mind_floor_row(&stamp.bounds);
    if inside >= 820 || y > floor {
        return Some(TileKind::Cave(CaveTile::Mycelium));
    }
    if y == floor && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(3) {
        return Some(TileKind::Cave(CaveTile::GlowCap));
    }
    Some(TileKind::Air)
}

/// Living rock: the tile the plain rock `base` at `(x, y)` of `stratum` becomes (mycelium
/// veins, root lines, comb seams), or `None` to leave it.
#[must_use]
pub fn living_rock(
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
        Stratum::FungalForest => {
            let mass = value_noise(seed, SALT_MYCELIUM, x, y, 9, 6);
            let threshold = 47_000 - along * 6;
            (mass > threshold).then_some(TileKind::Cave(CaveTile::Mycelium))
        }
        Stratum::RootLabyrinth => {
            let taproots = variant(settings, stratum) == 1;
            let line = ridge(value_noise(seed, SALT_ROOTS, x, y, 14, 10));
            let width = 2_400 + along * 4;
            if line > NOISE_MAX - width {
                return Some(TileKind::Cave(CaveTile::Root));
            }
            // Taproots: vertical root columns every so often.
            let column = x.div_euclid(11);
            let roll = hash2(seed, SALT_TAPROOT, column, y.div_euclid(48));
            let chance = if taproots { 3 } else { 9 };
            (roll.is_multiple_of(chance)
                && x.rem_euclid(11) == i32::try_from(roll >> 8 & 7).unwrap_or(0))
            .then_some(TileKind::Cave(CaveTile::Root))
        }
        Stratum::TheHive => {
            let seam = value_noise(seed, SALT_COMB, x, y, 12, 8);
            // Hexagonal comb: a seam band, broken on a hex lattice.
            let hex = (x + (y.div_euclid(2) % 2) * 2).rem_euclid(4) != 0 || y.rem_euclid(2) == 0;
            (seam > 30_000 && seam < 30_000 + 4_200 + along * 4 && hex)
                .then_some(TileKind::Cave(CaveTile::Comb))
        }
        _ => None,
    }
}

/// The Living Dark's tile at `(x, y)` over `base`: a stamp's tile, else living rock, else
/// `None` (leave `base`). `stamps` are the chunk's [`stamps_with`].
#[must_use]
pub fn living_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stamps: &[LivingStamp],
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    for stamp in stamps {
        if let Some(kind) = stamp_tile(stamp, x, y, base) {
            return Some(kind);
        }
    }
    let stratum = stratum_in(settings, layout, x, y);
    if !is_living(stratum) {
        return None;
    }
    living_rock(settings, layout, stratum, x, y, base)
}

#[cfg(test)]
mod tests;
