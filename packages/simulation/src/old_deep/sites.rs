//! Where the Old Deep's content lies in a V11 world: pure functions of the settings, so the
//! generator carves the same tiles in any chunk order and the tick finds the same set pieces.
//!
//! Each Descent II stratum lays a grid of regions over the world; a region whose centre row is
//! in that stratum may hold one feature (a crystal fault, a skeleton, a glow pit, a machine
//! hall, a geode...), drawn wholly inside it. A tile asks only the regions of its own stratum
//! and its two neighbours, so features never depend on chunk edges. The set pieces (the
//! Cathedral Lift's nave, the Star Core arena, the Glowmother's den and the Fathers' camps)
//! are placed once per world.

use crate::{
    base::BuiltTile,
    cave::CaveTile,
    descent::{Descent, Stratum},
    terrain::{TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        descent::{
            DescentLayout,
            landmarks::{self, OUTPOST_SIZE, lift_column, outpost_column},
            variant,
        },
        noise::hash2,
    },
};

use super::DeepTile;

const SALT_REGION: u64 = 0x01D_DEE9;
const SALT_SCATTER: u64 = 0x01D_5CA7;
const SALT_CAMP: u64 = 0x01D_CA39;

const fn deep(tile: DeepTile) -> TileKind {
    TileKind::Deep(tile)
}

/// Plain solid rock a vein-like feature may replace.
const fn plain_solid(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Stone | TileKind::HardRock | TileKind::Dirt | TileKind::Clay | TileKind::Ore(_)
    )
}

/// Ground a carved feature may replace: rock, ore, hazards, cave air and the soft cave
/// dressing; never built pieces, ruins, caches, points of interest, fluid or the floor.
const fn carvable(kind: TileKind) -> bool {
    match kind {
        TileKind::Air
        | TileKind::Stone
        | TileKind::HardRock
        | TileKind::Dirt
        | TileKind::Clay
        | TileKind::Ore(_)
        | TileKind::Gas
        | TileKind::Lava
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => true,
        TileKind::Cave(tile) => matches!(
            tile,
            CaveTile::Crystal
                | CaveTile::Obsidian
                | CaveTile::Stalactite
                | CaveTile::Mycelium
                | CaveTile::GlowCap
                | CaveTile::Silt
                | CaveTile::GlowAlgae
                | CaveTile::FossilRib
        ),
        _ => false,
    }
}

/// The region grid of a Descent II stratum (width, height in tiles).
#[must_use]
pub const fn region_size(stratum: Stratum) -> (i32, i32) {
    match stratum {
        Stratum::CrystalFaults => (26, 24),
        Stratum::PressureCathedrals => (64, 44),
        Stratum::AncientMachineLayer => (48, 30),
        Stratum::VoidGeodeFields => (40, 40),
        _ => (32, 30),
    }
}

/// Whether `stratum` is one of the Old Deep's.
#[must_use]
pub const fn is_old_deep(stratum: Stratum) -> bool {
    matches!(stratum.descent(), Descent::OldDeep)
}

const fn roll(settings: &WorldSettings, stratum: Stratum, rx: i32, ry: i32) -> u64 {
    hash2(
        settings.seed,
        SALT_REGION ^ (stratum.index() as u64) << 24,
        rx,
        ry,
    )
}

/// `n` in `0..span` from `bits` of a roll.
fn pick(roll: u64, shift: u32, span: i32) -> i32 {
    i32::try_from((roll >> shift) % u64::try_from(span.max(1)).unwrap_or(1)).unwrap_or(0)
}

/// A region's bounds, inside a margin.
const fn region_box(stratum: Stratum, rx: i32, ry: i32) -> TileRect {
    let (w, h) = region_size(stratum);
    TileRect {
        min: TilePosition {
            x: rx * w,
            y: ry * h,
        },
        max: TilePosition {
            x: rx * w + w - 1,
            y: ry * h + h - 1,
        },
    }
}

/// Whether region `(rx, ry)` of `stratum`'s grid belongs to it (its centre row lies in the
/// stratum's layout rows, a little inside the boundaries).
const fn region_in(layout: &DescentLayout, stratum: Stratum, ry: i32) -> bool {
    let (_, h) = region_size(stratum);
    let centre = ry * h + h / 2;
    let (top, bottom) = layout.rows(stratum);
    centre >= top + 6 && centre <= bottom - 6
}

/// Squared distance from `(px, py)` to the segment `a`..`b`, in sixteenths of a tile squared.
fn segment_distance_sq(px: i32, py: i32, a: (i32, i32), b: (i32, i32)) -> i64 {
    let (ax, ay) = (i64::from(a.0) * 4, i64::from(a.1) * 4);
    let (bx, by) = (i64::from(b.0) * 4, i64::from(b.1) * 4);
    let (px, py) = (i64::from(px) * 4, i64::from(py) * 4);
    let (dx, dy) = (bx - ax, by - ay);
    let length = dx * dx + dy * dy;
    let t = if length == 0 {
        0
    } else {
        (((px - ax) * dx + (py - ay) * dy) * 1_000 / length).clamp(0, 1_000)
    };
    let (cx, cy) = (ax + dx * t / 1_000, ay + dy * t / 1_000);
    (px - cx).pow(2) + (py - cy).pow(2)
}

/// Inside an ellipse of half-axes `(a, b)` around `(cx, cy)`, as per-mille of its radius
/// squared (1000 on the rim).
fn ellipse(x: i32, y: i32, (cx, cy): (i32, i32), (a, b): (i32, i32)) -> i64 {
    let dx = i64::from(x - cx);
    let dy = i64::from(y - cy);
    let a2 = i64::from(a.max(1)).pow(2);
    let b2 = i64::from(b.max(1)).pow(2);
    (dx * dx * b2 + dy * dy * a2) * 1_000 / (a2 * b2)
}

// ---------------------------------------------------------------------------------------------
// Regional features.
// ---------------------------------------------------------------------------------------------

/// A fossil skeleton: its bones (absolute tiles) and the shale bed around them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Skeleton {
    pub region: (i32, i32),
    pub bones: Vec<TilePosition>,
    pub bed: TileRect,
}

/// Bone templates (relative tiles): a sea serpent, a fish-lizard and a coiled shell-beast.
const SKELETONS: [&[(i32, i32)]; 3] = [
    &[
        (0, 2),
        (1, 1),
        (2, 1),
        (3, 2),
        (4, 2),
        (5, 1),
        (6, 1),
        (7, 2),
        (8, 2),
        (9, 1),
        (10, 0),
        (11, 0),
        (11, 1),
        (2, 2),
        (5, 2),
        (8, 3),
    ],
    &[
        (0, 2),
        (1, 2),
        (2, 2),
        (3, 2),
        (4, 2),
        (5, 2),
        (6, 2),
        (2, 1),
        (2, 3),
        (4, 1),
        (4, 3),
        (6, 1),
        (6, 3),
        (7, 1),
        (8, 1),
        (8, 2),
    ],
    &[
        (2, 0),
        (3, 0),
        (4, 1),
        (4, 2),
        (3, 3),
        (2, 3),
        (1, 2),
        (2, 1),
        (5, 3),
        (6, 3),
        (7, 2),
        (8, 2),
        (9, 3),
    ],
];

/// The skeleton of Fossil Oceans region `(rx, ry)`, if it holds one.
#[must_use]
pub fn skeleton(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
) -> Option<Skeleton> {
    let stratum = Stratum::FossilOceans;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 1 {
        55
    } else {
        38
    };
    if roll % 100 >= share {
        return None;
    }
    let template = SKELETONS[usize::try_from((roll >> 8) % 3).unwrap_or(0)];
    let area = region_box(stratum, rx, ry);
    let x0 = area.min.x + 4 + pick(roll, 12, 12);
    let y0 = area.min.y + 4 + pick(roll, 20, 16);
    let flip = (roll >> 30) & 1 == 1;
    let bones: Vec<TilePosition> = template
        .iter()
        .map(|&(dx, dy)| TilePosition {
            x: if flip { x0 + 11 - dx } else { x0 + dx },
            y: y0 + dy,
        })
        .collect();
    Some(Skeleton {
        region: (rx, ry),
        bones,
        bed: TileRect {
            min: TilePosition {
                x: x0 - 2,
                y: y0 - 2,
            },
            max: TilePosition {
                x: x0 + 13,
                y: y0 + 5,
            },
        },
    })
}

/// A machine hall: its interior and the region that keys its wake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineHall {
    pub region: (i32, i32),
    /// The walls' outer bounds.
    pub bounds: TileRect,
}

impl MachineHall {
    /// The floor row (the conveyors).
    #[must_use]
    pub const fn floor(&self) -> i32 {
        self.bounds.max.y
    }

    /// The ceiling row (the pistons).
    #[must_use]
    pub const fn ceiling(&self) -> i32 {
        self.bounds.min.y
    }

    /// The builder tablet in its left wall, above the door.
    #[must_use]
    pub const fn tablet(&self) -> TilePosition {
        TilePosition {
            x: self.bounds.min.x,
            y: self.bounds.max.y - 4,
        }
    }

    /// The machine tile at `(x, y)` of the hall's shell, if any.
    fn tile(&self, x: i32, y: i32) -> Option<TileKind> {
        let b = self.bounds;
        if !b.contains(TilePosition { x, y }) {
            return None;
        }
        let at = TilePosition { x, y };
        if at == self.tablet() {
            return Some(deep(DeepTile::BuilderTablet));
        }
        let (left, right) = (x == b.min.x, x == b.max.x);
        if (left || right) && y >= b.max.y - 3 && y < b.max.y {
            return Some(TileKind::Air); // doors
        }
        if y == b.max.y {
            if left || right {
                return Some(deep(DeepTile::MachineCasing));
            }
            // Belts in runs of six, each run its own way.
            let run = (x - b.min.x - 1).div_euclid(6);
            let way = hash2(
                u64::from(self.region.0.unsigned_abs()),
                0xBE17,
                run,
                self.region.1,
            );
            return Some(deep(if way.is_multiple_of(2) {
                DeepTile::ConveyorLeft
            } else {
                DeepTile::ConveyorRight
            }));
        }
        if y == b.min.y {
            if !left && !right && (x - b.min.x) % 6 == 3 {
                return Some(deep(DeepTile::PistonHousing));
            }
            return Some(deep(DeepTile::MachineCasing));
        }
        if left || right {
            return Some(deep(DeepTile::MachineCasing));
        }
        Some(TileKind::Air)
    }
}

/// The machine hall of Ancient Machine Layer region `(rx, ry)`, if it holds one.
#[must_use]
pub fn machine_hall(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
) -> Option<MachineHall> {
    let stratum = Stratum::AncientMachineLayer;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 0 {
        55
    } else {
        42
    };
    if roll % 100 >= share {
        return None;
    }
    let area = region_box(stratum, rx, ry);
    let width = 26 + pick(roll, 10, 3) * 6;
    let x = area.min.x + 2 + pick(roll, 16, (48 - width - 4).max(1));
    let y = area.min.y + 3 + pick(roll, 24, 30 - 12 - 3);
    Some(MachineHall {
        region: (rx, ry),
        bounds: TileRect {
            min: TilePosition { x, y },
            // Four rows inside: a slamming ram reaches a rig riding the belts.
            max: TilePosition {
                x: x + width - 1,
                y: y + 5,
            },
        },
    })
}

/// A geode: a void-glass ball with a low-gravity heart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Geode {
    pub centre: TilePosition,
    /// Outer radius in tiles (the shell is the outer two).
    pub radius: i32,
}

impl Geode {
    /// The geode's open radius.
    #[must_use]
    pub const fn hollow(&self) -> i32 {
        self.radius - 2
    }
}

/// The geode of Void Geode Fields region `(rx, ry)`, if it holds one.
#[must_use]
pub fn geode(settings: &WorldSettings, layout: &DescentLayout, rx: i32, ry: i32) -> Option<Geode> {
    let stratum = Stratum::VoidGeodeFields;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 0 {
        62
    } else {
        48
    };
    if roll % 100 >= share {
        return None;
    }
    let radius = 8 + pick(roll, 8, 6);
    let area = region_box(stratum, rx, ry);
    let span = 40 - 2 * (radius + 1);
    Some(Geode {
        centre: TilePosition {
            x: area.min.x + radius + 1 + pick(roll, 14, span),
            y: area.min.y + radius + 1 + pick(roll, 22, span),
        },
        radius,
    })
}

/// The geode around tile `(x, y)` (its region's), if any.
#[must_use]
pub fn geode_at(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> Option<Geode> {
    let (w, h) = region_size(Stratum::VoidGeodeFields);
    geode(settings, layout, x.div_euclid(w), y.div_euclid(h)).filter(|geode| {
        let dx = x - geode.centre.x;
        let dy = y - geode.centre.y;
        dx * dx + dy * dy <= geode.radius * geode.radius
    })
}

fn crystal_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let stratum = Stratum::CrystalFaults;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let area = region_box(stratum, rx, ry);
    let faults = if variant(settings, stratum) == 0 {
        60
    } else {
        40
    };
    let kind = roll % 100;
    if kind < faults {
        // A fault line: resonant crystal two wide, latent gems behind it, prisms at its ends.
        let length = 10 + pick(roll, 8, 9);
        let (dx, dy) = match (roll >> 16) % 3 {
            0 => (1, 1),
            1 => (1, -1),
            _ => (2, 1),
        };
        let steps = length / dx.max(1);
        let ax = area.min.x + 3 + pick(roll, 20, (26 - 6 - length).max(1));
        let ay = if dy > 0 {
            area.min.y + 3 + pick(roll, 28, (24 - 6 - steps).max(1))
        } else {
            area.max.y - 3 - pick(roll, 28, (24 - 6 - steps).max(1))
        };
        let a = (ax, ay);
        let b = (ax + steps * dx, ay + steps * dy);
        let at = TilePosition { x, y };
        let end = |p: (i32, i32)| at.x == p.0 && at.y == p.1;
        if end(a) || end(b) {
            return carvable(current).then_some(deep(DeepTile::Prism));
        }
        let distance = segment_distance_sq(x, y, a, b);
        if distance <= 16 {
            return carvable(current).then_some(deep(DeepTile::ResonantCrystal));
        }
        if distance <= 80
            && plain_solid(current)
            && hash2(settings.seed, SALT_SCATTER, x, y).is_multiple_of(3)
        {
            return Some(deep(DeepTile::LatentGem));
        }
        return None;
    }
    if kind < faults + 25 {
        // A prism grotto: a small hollow with a prism on its floor and gems in the rock.
        let centre = (area.min.x + 13, area.min.y + 9 + pick(roll, 20, 6));
        let inside = ellipse(x, y, centre, (6, 3));
        if x == centre.0 && y == centre.1 + 3 {
            return carvable(current).then_some(deep(DeepTile::Prism));
        }
        if inside < 1_000 {
            return carvable(current).then_some(TileKind::Air);
        }
        if inside < 3_600
            && plain_solid(current)
            && hash2(settings.seed, SALT_SCATTER, x, y).is_multiple_of(4)
        {
            return Some(deep(DeepTile::LatentGem));
        }
    }
    None
}

fn fossil_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let stratum = Stratum::FossilOceans;
    if let Some(skeleton) = skeleton(settings, layout, rx, ry) {
        let at = TilePosition { x, y };
        if skeleton.bones.contains(&at) {
            return carvable(current).then_some(deep(DeepTile::FossilBone));
        }
        if skeleton.bed.contains(at) && carvable(current) && current != TileKind::Air {
            return Some(deep(DeepTile::Shale));
        }
        return None;
    }
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let area = region_box(stratum, rx, ry);
    let kind = roll % 100;
    if (55..75).contains(&kind) {
        // An amber blob with a darker heart.
        let centre = (area.min.x + 16, area.min.y + 15);
        let size = (4 + pick(roll, 10, 3), 2 + pick(roll, 14, 2));
        if ellipse(x, y, centre, size) <= 1_000 && carvable(current) {
            return Some(deep(DeepTile::Amber));
        }
    } else if (75..95).contains(&kind) {
        // A gas basin: a bowl with gas pooled in its bottom and a shale rim.
        let centre = (area.min.x + 16, area.min.y + 13);
        let inside = ellipse(x, y, centre, (7, 4));
        if inside < 1_000 && y > centre.1 {
            if !carvable(current) {
                return None;
            }
            return Some(if y >= centre.1 + 2 {
                TileKind::Gas
            } else {
                TileKind::Air
            });
        }
        if inside < 1_000 {
            return carvable(current).then_some(TileKind::Air);
        }
        if inside < 1_700 && y > centre.1 && carvable(current) {
            return Some(deep(DeepTile::Shale));
        }
    }
    None
}

fn cathedral_region_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let stratum = Stratum::PressureCathedrals;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 0 {
        50
    } else {
        36
    };
    if roll % 100 >= share {
        return None;
    }
    let area = region_box(stratum, rx, ry);
    let width = 40 + pick(roll, 8, 3) * 6;
    let bounds = TileRect {
        min: TilePosition {
            x: area.min.x + (64 - width) / 2,
            y: area.min.y + 6,
        },
        max: TilePosition {
            x: area.min.x + (64 - width) / 2 + width - 1,
            y: area.min.y + 6 + 26,
        },
    };
    nave_tile(&bounds, x, y, current, &[])
}

/// The tile of a cathedral nave in `bounds` at `(x, y)`: a vaulted hall with a stone floor,
/// pillars, ledges and pressure jets.
fn nave_tile(
    bounds: &TileRect,
    x: i32,
    y: i32,
    current: TileKind,
    keep_open: &[i32],
) -> Option<TileKind> {
    if !bounds.contains(TilePosition { x, y }) {
        return None;
    }
    let width = bounds.max.x - bounds.min.x + 1;
    let column = x - bounds.min.x;
    let floor = bounds.max.y;
    // The vault: the ceiling arches up in the middle (a parabola), two rows of stone under it.
    let half = width / 2;
    let off = i64::from(column - half);
    let rise = i32::try_from(off * off * 9 / i64::from(half.max(1)).pow(2)).unwrap_or(9);
    let ceiling = bounds.min.y + rise;
    if y < ceiling {
        return None;
    }
    if y <= ceiling + 1 || y == floor || column == 0 || column == width - 1 {
        return carvable(current).then_some(deep(DeepTile::NaveStone));
    }
    if !carvable(current) {
        return None;
    }
    // Pillars every 12 columns (from the floor and hanging from the vault), jets between.
    let pillar = (column - 6).rem_euclid(12) == 0 && column > 3 && column < width - 4;
    if pillar && !keep_open.contains(&x) && (y >= floor - 4 || y <= ceiling + 4) {
        return Some(deep(DeepTile::NaveStone));
    }
    if y == floor - 1
        && (column - 12).rem_euclid(12) == 0
        && column > 3
        && column < width - 4
        && !keep_open.contains(&x)
    {
        return Some(deep(DeepTile::JetVent));
    }
    Some(TileKind::Air)
}

fn hollow_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let stratum = Stratum::RadioactiveHollow;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 0 {
        50
    } else {
        38
    };
    if roll % 100 >= share {
        return None;
    }
    let area = region_box(stratum, rx, ry);
    let centre = (area.min.x + 16, area.min.y + 14);
    let size = (6 + pick(roll, 8, 4), 4 + pick(roll, 12, 2));
    let inside = ellipse(x, y, centre, size);
    let relic = (roll >> 20).is_multiple_of(3);
    if relic && x == centre.0 && y == centre.1 + size.1 {
        return carvable(current).then_some(deep(DeepTile::MutationRelic));
    }
    if inside < 1_000 {
        return carvable(current).then_some(TileKind::Air);
    }
    if inside < 2_400 && carvable(current) && current != TileKind::Air {
        let h = hash2(settings.seed, SALT_SCATTER, x, y);
        return match h % 6 {
            0 => Some(deep(DeepTile::IsotopeCluster)),
            1..=3 => Some(deep(DeepTile::GlowRock)),
            _ => None,
        };
    }
    None
}

fn geode_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let geode = geode(settings, layout, rx, ry)?;
    let dx = x - geode.centre.x;
    let dy = y - geode.centre.y;
    let d2 = dx * dx + dy * dy;
    let r = geode.radius;
    if d2 > r * r || !carvable(current) {
        return None;
    }
    let hollow = geode.hollow();
    if d2 >= hollow * hollow {
        return Some(deep(DeepTile::GeodeShell));
    }
    if d2 >= (hollow - 1) * (hollow - 1)
        && hash2(settings.seed, SALT_SCATTER, x, y).is_multiple_of(5)
    {
        return Some(deep(DeepTile::VoidBloom));
    }
    Some(TileKind::Air)
}

fn storm_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let stratum = Stratum::MantleStormZone;
    if !region_in(layout, stratum, ry) {
        return None;
    }
    let roll = roll(settings, stratum, rx, ry);
    let share = if variant(settings, stratum) == 0 {
        48
    } else {
        34
    };
    if roll % 100 >= share {
        return None;
    }
    let area = region_box(stratum, rx, ry);
    let centre = (area.min.x + 10 + pick(roll, 8, 12), area.min.y + 12);
    let size = (5 + pick(roll, 12, 3), 3);
    if x == centre.0 && y == centre.1 + size.1 {
        return carvable(current).then_some(deep(DeepTile::StormVent));
    }
    let inside = ellipse(x, y, centre, size);
    if inside < 1_000 {
        return carvable(current).then_some(TileKind::Air);
    }
    if inside < 2_000 && plain_solid(current) {
        return Some(deep(DeepTile::CinderRock));
    }
    None
}

/// The feature tile of `stratum`'s region `(rx, ry)` at `(x, y)`.
fn region_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let (w, h) = region_size(stratum);
    let (rx, ry) = (x.div_euclid(w), y.div_euclid(h));
    match stratum {
        Stratum::CrystalFaults => crystal_tile(settings, layout, rx, ry, x, y, current),
        Stratum::FossilOceans => fossil_tile(settings, layout, rx, ry, x, y, current),
        Stratum::PressureCathedrals => {
            cathedral_region_tile(settings, layout, rx, ry, x, y, current)
        }
        Stratum::RadioactiveHollow => hollow_tile(settings, layout, rx, ry, x, y, current),
        Stratum::AncientMachineLayer => machine_hall(settings, layout, rx, ry)
            .and_then(|hall| hall.tile(x, y))
            .filter(|_| carvable(current)),
        Stratum::VoidGeodeFields => geode_tile(settings, layout, rx, ry, x, y, current),
        Stratum::MantleStormZone => storm_tile(settings, layout, rx, ry, x, y, current),
        _ => None,
    }
}

/// Single tiles strewn through a stratum's plain rock.
const fn scatter_tile(
    settings: &WorldSettings,
    stratum: Stratum,
    x: i32,
    y: i32,
) -> Option<TileKind> {
    let h = hash2(settings.seed, SALT_SCATTER ^ 0x77, x, y);
    match stratum {
        Stratum::CrystalFaults if h.is_multiple_of(170) => Some(deep(DeepTile::LatentGem)),
        Stratum::CrystalFaults if h % 97 == 1 => Some(deep(DeepTile::ResonantCrystal)),
        Stratum::FossilOceans if h % 60 < 3 => Some(deep(DeepTile::Shale)),
        Stratum::FossilOceans if h % 211 == 5 => Some(deep(DeepTile::Amber)),
        Stratum::RadioactiveHollow if h.is_multiple_of(90) => Some(deep(DeepTile::GlowRock)),
        Stratum::MantleStormZone if h.is_multiple_of(70) => Some(deep(DeepTile::CinderRock)),
        Stratum::VoidGeodeFields if h.is_multiple_of(260) => Some(deep(DeepTile::GeodeShell)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Set pieces.
// ---------------------------------------------------------------------------------------------

/// The Cathedral Lift's nave: the chamber, its valves and the lift that it restores.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CathedralSite {
    pub bounds: TileRect,
    /// The three valves, in the order the set piece numbers them (floor, right ledge, high
    /// left ledge).
    pub valves: [TilePosition; 3],
    /// The lift's rail column.
    pub lift_column: i32,
    /// The lift's top station (beside Descent II's outpost) and bottom station (the nave's
    /// floor), rows a rig stands on.
    pub top_station: i32,
    pub bottom_station: i32,
}

impl CathedralSite {
    /// Ledges: (row, first column, last column).
    #[must_use]
    pub const fn ledges(&self) -> [(i32, i32, i32); 2] {
        let b = self.bounds;
        [
            (b.min.y + 16, b.max.x - 11, b.max.x - 1),
            (b.min.y + 9, b.min.x + 1, b.min.x + 11),
        ]
    }

    /// The tile at `(x, y)`, if the nave places one.
    #[must_use]
    pub fn tile(&self, x: i32, y: i32, current: TileKind) -> Option<TileKind> {
        let at = TilePosition { x, y };
        if !self.bounds.contains(at) {
            return None;
        }
        if let Some(index) = self.valves.iter().position(|&valve| valve == at) {
            let _ = index;
            return Some(deep(DeepTile::LiftValve));
        }
        for (row, from, to) in self.ledges() {
            if y == row && (from..=to).contains(&x) {
                return Some(deep(DeepTile::NaveStone));
            }
            // The jet that climbs to each ledge.
            if y == self.bounds.max.y - 1
                && x == if from == self.bounds.min.x + 1 {
                    to + 2
                } else {
                    from - 2
                }
            {
                return Some(deep(DeepTile::JetVent));
            }
        }
        let keep = [
            self.lift_column - 1,
            self.lift_column,
            self.lift_column + 1,
            self.valves[0].x,
            self.valves[0].x + 1,
            self.valves[0].x - 1,
        ];
        // The nave carves everything (it is the set piece), except the floor under the valves.
        let current = if carvable(current) {
            current
        } else {
            TileKind::Stone
        };
        nave_tile(&self.bounds, x, y, current, &keep)
    }
}

/// The cathedral's lift column: two columns outside Descent II's outpost hall on the side
/// away from the builders' cable.
#[must_use]
pub const fn cathedral_lift_column(settings: &WorldSettings) -> i32 {
    let center = outpost_column(settings);
    let half = OUTPOST_SIZE.0 / 2;
    if lift_column(settings) > center {
        center - half - 2
    } else {
        center + half + 1
    }
}

/// The Cathedral Lift set piece of a V11 world.
#[must_use]
pub fn cathedral(settings: &WorldSettings, layout: &DescentLayout) -> Option<CathedralSite> {
    if !settings.generator.has_descents() {
        return None;
    }
    let outpost = landmarks::outpost(settings, layout, Descent::OldDeep)?;
    let column = cathedral_lift_column(settings);
    let (top, bottom) = layout.rows(Stratum::PressureCathedrals);
    let floor = top + (bottom - top) / 2 + 13;
    let (width, height) = (54, 28);
    let bounds = TileRect {
        min: TilePosition {
            x: column - width / 2,
            y: floor - height + 1,
        },
        max: TilePosition {
            x: column - width / 2 + width - 1,
            y: floor,
        },
    };
    let mut site = CathedralSite {
        bounds,
        valves: [TilePosition { x: 0, y: 0 }; 3],
        lift_column: column,
        top_station: outpost.bounds.max.y - 1,
        bottom_station: floor - 1,
    };
    let [(right_row, _, right_to), (left_row, left_from, _)] = site.ledges();
    site.valves = [
        TilePosition {
            x: bounds.min.x + 7,
            y: floor - 1,
        },
        TilePosition {
            x: right_to - 2,
            y: right_row - 1,
        },
        TilePosition {
            x: left_from + 3,
            y: left_row - 1,
        },
    ];
    Some(site)
}

/// The Star Core arena: Descent II's gate chamber rebuilt as the boss's hall.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArenaSite {
    pub bounds: TileRect,
    pub core: TilePosition,
    pub pylons: [TilePosition; 4],
}

impl ArenaSite {
    /// The floor row.
    #[must_use]
    pub const fn floor(&self) -> i32 {
        self.bounds.max.y
    }

    /// Whether `tile` is inside the arena's walls.
    #[must_use]
    pub const fn inside(&self, tile: TilePosition) -> bool {
        tile.x > self.bounds.min.x
            && tile.x < self.bounds.max.x
            && tile.y > self.bounds.min.y
            && tile.y < self.bounds.max.y
    }

    /// The floor tiles that open when the core is beaten (the way below).
    #[must_use]
    pub const fn breach(&self) -> [TilePosition; 5] {
        let y = self.bounds.max.y;
        let x = self.core.x;
        [
            TilePosition { x: x - 4, y },
            TilePosition { x: x - 3, y },
            TilePosition { x: x + 3, y },
            TilePosition { x: x + 4, y },
            TilePosition { x: x + 5, y },
        ]
    }

    /// The arena tile at `(x, y)`.
    #[must_use]
    pub fn tile(&self, x: i32, y: i32) -> Option<TileKind> {
        let b = self.bounds;
        let at = TilePosition { x, y };
        if !b.contains(at) {
            return None;
        }
        if at == self.core {
            return Some(deep(DeepTile::FirstCoreShielded));
        }
        if self.pylons.contains(&at) {
            return Some(deep(DeepTile::CorePylon));
        }
        let (left, right, top, bottom) = (x == b.min.x, x == b.max.x, y == b.min.y, y == b.max.y);
        if top {
            // The roof can be dug through (the way in), its corners cannot.
            return Some(deep(if x - b.min.x < 3 || b.max.x - x < 3 {
                DeepTile::CoreWall
            } else {
                DeepTile::CinderRock
            }));
        }
        if left || right || bottom {
            return Some(deep(DeepTile::CoreWall));
        }
        // The core's pedestal.
        if x == self.core.x && y > self.core.y {
            return Some(deep(DeepTile::CoreWall));
        }
        // Two side platforms to dodge flares on (each holds a pylon).
        let platform_row = b.min.y + 9;
        if y == platform_row
            && ((8..=14).contains(&(x - b.min.x)) || (8..=14).contains(&(b.max.x - x)))
        {
            return Some(deep(DeepTile::CoreWall));
        }
        Some(TileKind::Air)
    }
}

/// The Star Core arena of a V11 world (over Descent II's gate chamber).
#[must_use]
pub fn star_core_arena(settings: &WorldSettings, layout: &DescentLayout) -> Option<ArenaSite> {
    if !settings.generator.has_descents() {
        return None;
    }
    let chamber = landmarks::gate_chamber(settings, layout, Descent::OldDeep);
    let g = chamber.bounds;
    let bounds = TileRect {
        min: TilePosition {
            x: g.min.x - 2,
            y: g.min.y - 6,
        },
        max: TilePosition {
            x: g.max.x + 2,
            y: g.max.y,
        },
    };
    let floor = bounds.max.y;
    let core = TilePosition {
        x: bounds.min.x + 24,
        y: floor - 5,
    };
    Some(ArenaSite {
        bounds,
        core,
        pylons: [
            TilePosition {
                x: bounds.min.x + 5,
                y: floor - 1,
            },
            TilePosition {
                x: bounds.max.x - 5,
                y: floor - 1,
            },
            TilePosition {
                x: bounds.min.x + 11,
                y: bounds.min.y + 8,
            },
            TilePosition {
                x: bounds.max.x - 11,
                y: bounds.min.y + 8,
            },
        ],
    })
}

/// The Glowmother's den (Radioactive Hollow): its centre and half-size.
#[must_use]
pub fn glowmother_den(
    settings: &WorldSettings,
    layout: &DescentLayout,
) -> Option<(TilePosition, (i32, i32))> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::RadioactiveHollow);
    let side = if lift_column(settings) > outpost_column(settings) {
        1
    } else {
        -1
    };
    let roll = hash2(settings.seed, SALT_CAMP, 4, 4);
    Some((
        TilePosition {
            x: outpost_column(settings) + side * (34 + pick(roll, 4, 20)),
            y: top + (bottom - top) / 2 + pick(roll, 12, 20) - 10,
        },
        (14, 7),
    ))
}

fn den_tile(centre: TilePosition, size: (i32, i32), x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse(x, y, (centre.x, centre.y), size);
    if inside < 1_000 {
        return Some(if y == centre.y + size.1 - 1 && (x - centre.x).abs() <= 6 {
            deep(DeepTile::GlowRock)
        } else {
            TileKind::Air
        });
    }
    if inside < 1_500 {
        return Some(deep(if (x + y).rem_euclid(5) == 0 {
            DeepTile::IsotopeCluster
        } else {
            DeepTile::GlowRock
        }));
    }
    None
}

/// The Fathers' expedition camp of `stratum` (one per Old Deep stratum): the cache that holds
/// its log, on a plated ledge in a little niche.
#[must_use]
pub fn expedition_camp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
) -> Option<TilePosition> {
    if !settings.generator.has_descents() || !is_old_deep(stratum) {
        return None;
    }
    let (top, bottom) = layout.rows(stratum);
    let index = i32::try_from(stratum.index()).unwrap_or(0);
    let roll = hash2(settings.seed, SALT_CAMP, index, 0);
    let side = if roll & 1 == 0 { 1 } else { -1 };
    let x = SPAWN_TILE_X + side * (20 + pick(roll, 4, 60));
    let y = top + (bottom - top) / 3 + pick(roll, 12, (bottom - top) / 3);
    Some(TilePosition { x, y })
}

/// The log number of `stratum`'s camp.
#[must_use]
pub fn camp_log(stratum: Stratum) -> u8 {
    let first = Stratum::CrystalFaults.index();
    u8::try_from(stratum.index().saturating_sub(first)).unwrap_or(0)
}

fn camp_tile(cache: TilePosition, x: i32, y: i32) -> Option<TileKind> {
    let dx = x - cache.x;
    let dy = y - cache.y;
    if !(-3..=3).contains(&dx) || !(-3..=1).contains(&dy) {
        return None;
    }
    Some(if dx == 0 && dy == 0 {
        deep(DeepTile::ExpeditionLog)
    } else if dy == 1 {
        TileKind::Built(BuiltTile::Plating)
    } else {
        TileKind::Air
    })
}

// ---------------------------------------------------------------------------------------------
// The carve.
// ---------------------------------------------------------------------------------------------

/// The rows of a world the Old Deep's carve may touch (Descent II and a margin).
#[must_use]
pub const fn carve_rows(layout: &DescentLayout) -> (i32, i32) {
    let (top, bottom) = layout.descent_rows(Descent::OldDeep);
    (top - 24, bottom + 24)
}

/// The Old Deep tile the V11 generator puts at `(x, y)` over `current`, or `None` to keep it.
///
/// Set pieces first (the Star Core arena, the cathedral, the den, the camps), then the
/// regional features of the stratum the tile is in and its two neighbours, then the scattered
/// singles.
#[must_use]
pub fn carve_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
    current: TileKind,
) -> Option<TileKind> {
    let (top, bottom) = carve_rows(layout);
    if y < top || y > bottom {
        return None;
    }
    if let Some(arena) = star_core_arena(settings, layout)
        && let Some(kind) = arena.tile(x, y)
    {
        return Some(kind);
    }
    if let Some(site) = cathedral(settings, layout)
        && let Some(kind) = site.tile(x, y, current)
    {
        return Some(kind);
    }
    if let Some((centre, size)) = glowmother_den(settings, layout)
        && carvable(current)
        && let Some(kind) = den_tile(centre, size, x, y)
    {
        return Some(kind);
    }
    let row_stratum = layout.stratum_of_row(y);
    for stratum in [
        Some(row_stratum),
        row_stratum.index().checked_sub(1).map(Stratum::from_index),
        (row_stratum.index() + 1 < Stratum::ALL.len())
            .then(|| Stratum::from_index(row_stratum.index() + 1)),
    ]
    .into_iter()
    .flatten()
    {
        if !is_old_deep(stratum) {
            continue;
        }
        if let Some(cache) = expedition_camp(settings, layout, stratum)
            && let Some(kind) = camp_tile(cache, x, y)
        {
            return Some(kind);
        }
        if let Some(kind) = region_tile(settings, layout, stratum, x, y, current) {
            return Some(kind);
        }
    }
    if is_old_deep(row_stratum) && plain_solid(current) {
        return scatter_tile(settings, row_stratum, x, y);
    }
    None
}

/// Every machine hall whose bounds (grown by `margin`) touch `area`.
#[must_use]
pub fn machine_halls_near(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
    margin: i32,
) -> Vec<MachineHall> {
    let (w, h) = region_size(Stratum::AncientMachineLayer);
    let mut halls = Vec::new();
    for ry in (area.min.y - margin).div_euclid(h)..=(area.max.y + margin).div_euclid(h) {
        for rx in (area.min.x - margin).div_euclid(w)..=(area.max.x + margin).div_euclid(w) {
            if let Some(hall) = machine_hall(settings, layout, rx, ry) {
                let b = hall.bounds;
                if b.min.x - margin <= area.max.x
                    && b.max.x + margin >= area.min.x
                    && b.min.y - margin <= area.max.y
                    && b.max.y + margin >= area.min.y
                {
                    halls.push(hall);
                }
            }
        }
    }
    halls
}

/// The skeleton whose bones include `tile`, if any.
#[must_use]
pub fn skeleton_of(
    settings: &WorldSettings,
    layout: &DescentLayout,
    tile: TilePosition,
) -> Option<Skeleton> {
    let (w, h) = region_size(Stratum::FossilOceans);
    skeleton(settings, layout, tile.x.div_euclid(w), tile.y.div_euclid(h))
        .filter(|skeleton| skeleton.bones.contains(&tile))
}

#[cfg(test)]
mod tests;
