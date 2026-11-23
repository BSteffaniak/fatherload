//! The Hollow World's generated places (Wave 9, Descent V of
//! [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)).
//!
//! The Inner Sky's great cavern is part of the base layer ([`super::hollow_open`]); this pass,
//! run by `streamed::assemble` after the Living Dark's and before the Old Deep's, writes the
//! rest, all pure functions of the settings and the tile (so chunks carve the same in any
//! order):
//!
//! * **Inner Sky**: its rock sky and floating stones are sky stone (the registry's rock) with
//!   **sky amber** in their skins; the **Breach** ([`breach`]), the Fathers' winch platform
//!   hanging under the rock sky near the outposts' column, where they broke in.
//! * **Megafauna Jungle**: **jungle halls** ([`jungle_open`]), great open caverns floored with
//!   ferns, **giant trees** whose crowns spread under the hall's roof, **tar pits** on the
//!   floors (more of them deeper and in the "Tar Pits" variant), giant bones in the loam, and
//!   the **Sunfolk valley** ([`sunfolk_valley`]) round the jungle tribes' village.
//! * **Upside-Down Isles**: the **isle sea** ([`isle_sea_rows`]), an open cavern where **isles**
//!   ([`Isle`]) hang from the roof on stalks or float lower down: flat-bottomed, crowned
//!   upward, grass on their undersides. Under each one gravity points up
//!   ([`crate::hollow::fields`]).
//! * **The Sunforge**: the **forge sky** ([`forge_rows`]), open air full of small floating
//!   stones of sun rock (shade to hop between) over a floor of lava lakes and slag, sun glass
//!   in the rock; and at its bottom the **Sun Well** ([`sun_well`]): a ring of builder stone
//!   round the inner sun itself, four shade pillars inside, where the Sun Serpent orbits.
//!
//! Only plain rock, ore, hazards and cave tiles are carved; built pieces, points of interest,
//! bedrock, fluid, the Descent landmarks and the jungle tribes' hall stay.

use crate::{
    descent::{Descent, Stratum},
    hollow::{HollowTile, PILLAR_DISTANCE, SUN_RADIUS, WELL_RADIUS},
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, centred, hash2, line_noise, value_noise},
    },
};

use super::{DescentLayout, landmarks, stratum_in, variant};

const SALT_JUNGLE: u64 = 0x5744_0910;
const SALT_TREE: u64 = 0x5744_0911;
const SALT_TAR: u64 = 0x5744_0912;
const SALT_BONE: u64 = 0x5744_0913;
const SALT_ISLE: u64 = 0x5744_0914;
const SALT_SEA: u64 = 0x5744_0915;
const SALT_FORGE: u64 = 0x5744_0916;
const SALT_GLASS: u64 = 0x5744_0917;
const SALT_AMBER: u64 = 0x5744_0918;
const SALT_VALLEY: u64 = 0x5744_0919;

/// The cells isles are rolled per, `(width, height)` in tiles.
pub const ISLE_CELL: (i32, i32) = (56, 64);
/// Rows under an isle's underside its upside-down gravity reaches.
pub const ISLE_BUBBLE_ROWS: i32 = 18;
/// Columns either side of an isle its upside-down gravity reaches past its body.
pub const ISLE_BUBBLE_SIDE: i32 = 3;
/// Columns between giant tree sites in the jungle.
pub const TREE_SPACING: i32 = 22;
/// `(width, height)` of the Sunfolk valley (the jungle hall round the tribes' village).
pub const VALLEY_SIZE: (i32, i32) = (124, 26);

/// Whether the world has the Hollow World (a Deep Descent world).
#[must_use]
pub const fn has_hollow(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// Whether `stratum` is one of the Hollow World's.
#[must_use]
pub fn is_hollow(stratum: Stratum) -> bool {
    stratum.descent() == Descent::HollowWorld
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

/// `(ceiling, floor)` of an open cavern in `stratum` at column `x`: `margins` rows in from the
/// stratum's top and bottom, both waving by `wave` rows.
const fn cavern_rows(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    salt: u64,
    margins: (i32, i32),
    wave: i32,
    x: i32,
) -> (i32, i32) {
    let (top, bottom) = layout.rows(stratum);
    let seed = settings.seed;
    let ceiling = centred(
        (line_noise(seed, salt, x, 41) * 2 + line_noise(seed, salt ^ 1, x, 11)) / 3,
        wave,
    );
    let floor = centred(
        (line_noise(seed, salt ^ 2, x, 53) * 2 + line_noise(seed, salt ^ 3, x, 13)) / 3,
        wave,
    );
    (
        top + margins.0 + wave + ceiling,
        bottom - margins.1 - wave + floor,
    )
}

// ---------------------------------------------------------------------------------------------
// Inner Sky.
// ---------------------------------------------------------------------------------------------

/// The Breach: the Fathers' winch platform hanging under the Inner Sky's rock sky, beside the
/// outposts' column (the deck is its bottom row).
#[must_use]
pub fn breach(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_hollow(settings) {
        return None;
    }
    let column = landmarks::outpost_column(settings);
    let side = if column >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    };
    let centre = column + side * 34;
    let (left, right) = (centre - 7, centre + 7);
    // Under the lowest stalactite of the span.
    let deck = (left - 1..=right + 1)
        .map(|x| super::hollow_rows(settings, layout, x).0 + 7)
        .max()
        .unwrap_or(0);
    let roof = (left..=right)
        .map(|x| super::hollow_rows(settings, layout, x).0)
        .min()
        .unwrap_or(deck)
        - 1;
    Some(TileRect {
        min: TilePosition { x: left, y: roof },
        max: TilePosition { x: right, y: deck },
    })
}

/// Whether the Inner Sky's open air at `(x, y)` holds one of the floating stones this pass adds.
///
/// An archipelago of landing places, rocs' perches and amber, denser in the "Stone
/// Archipelago" variant and deeper down, clear of the rock sky and the ground.
#[must_use]
pub fn sky_stone(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let (ceiling, floor) = super::hollow_rows(settings, layout, x);
    if y < ceiling + 14 || y > floor - 16 {
        return false;
    }
    let seed = settings.seed;
    let along = i32::from(layout.along(y));
    let archipelago = variant(settings, Stratum::InnerSky) == 1;
    let blob = (value_noise(seed, SALT_AMBER ^ 0x51, x, y, 30, 9) * 3
        + value_noise(seed, SALT_AMBER ^ 0x52, x, y, 7, 4))
        / 4;
    let threshold = NOISE_MAX * if archipelago { 70 } else { 74 } / 100 - along * 5;
    blob > threshold
}

const fn breach_tile(rect: &TileRect, x: i32, y: i32) -> Option<TileKind> {
    if !rect.contains(TilePosition { x, y }) {
        return None;
    }
    let timber = TileKind::Hollow(HollowTile::BreachTimber);
    if y == rect.max.y {
        return Some(timber);
    }
    // Two winch ropes up to the rock sky, and the winch's frame over one end.
    if (x == rect.min.x + 1 || x == rect.max.x - 1) && y < rect.max.y {
        return Some(timber);
    }
    if y == rect.max.y - 3 && (x - rect.min.x - 1).abs() <= 1 {
        return Some(timber);
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Megafauna Jungle.
// ---------------------------------------------------------------------------------------------

/// Whether `(x, y)` is open air of a jungle hall (before trees and floors).
#[must_use]
pub fn jungle_open(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let (top, bottom) = layout.rows(Stratum::MegafaunaJungle);
    if y < top + 14 || y > bottom - 18 {
        return false;
    }
    let seed = settings.seed;
    // Halls close up toward the stratum's edges.
    let edge = (y - top - 14).min(bottom - 18 - y);
    let squeeze = if edge < 12 { (12 - edge) * 900 } else { 0 };
    let along = i32::from(layout.along(y));
    let big = value_noise(seed, SALT_JUNGLE, x, y, 52, 20);
    let detail = value_noise(seed, SALT_JUNGLE ^ 1, x, y, 11, 6);
    let value = (big * 3 + detail) / 4;
    // Bigger halls deeper (introduced gently, then pushed).
    let threshold = NOISE_MAX * 56 / 100 - along * 4 + squeeze;
    value > threshold
}

/// The Sunfolk valley: the jungle hall round the jungle tribes' village (its hall's floor is
/// the valley's floor row, `max.y`).
#[must_use]
pub const fn sunfolk_valley(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_hollow(settings) {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::MegafaunaJungle);
    let column = landmarks::outpost_column(settings);
    let side = if column >= crate::worldgen::SPAWN_TILE_X {
        1
    } else {
        -1
    };
    let roll = hash2(settings.seed, SALT_VALLEY, 0, 0);
    let centre = column + side * roll_range(roll, 0, 100, 150);
    let floor = top + (bottom - top) * 2 / 5 + roll_range(roll, 12, -10, 10);
    Some(TileRect {
        min: TilePosition {
            x: centre - VALLEY_SIZE.0 / 2,
            y: floor - VALLEY_SIZE.1 + 1,
        },
        max: TilePosition {
            x: centre + VALLEY_SIZE.0 / 2 - 1,
            y: floor,
        },
    })
}

/// Whether the valley at `valley` is open at `(x, y)`: a broad dome over a flat floor.
fn valley_open(valley: &TileRect, x: i32, y: i32) -> bool {
    if !valley.contains(TilePosition { x, y }) || y >= valley.max.y {
        return false;
    }
    let half = valley.width() / 2;
    let centre = valley.min.x + half;
    let height = valley.height() - 1;
    let dx = i64::from((x - centre).abs()) * 1_000 / i64::from(half.max(1));
    let dy = i64::from(valley.max.y - y) * 1_000 / i64::from(height.max(1));
    // A flattened dome: wide at the floor, rounded at the top.
    dx * dx / 1_000 + dy * dy / 1_000 < 1_000
}

/// The jungle's tile at `(x, y)` over plain rock `base`, `None` to leave it.
fn jungle_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    valley: Option<&TileRect>,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let open_at = |cx: i32, cy: i32| {
        jungle_open(settings, layout, cx, cy) || valley.is_some_and(|v| valley_open(v, cx, cy))
    };
    let seed = settings.seed;
    if open_at(x, y) {
        // A giant tree: a two-wide trunk floor to roof, its crown under the roof.
        let cell = x.div_euclid(TREE_SPACING);
        let roll = hash2(seed, SALT_TREE, cell, 0);
        if !roll.is_multiple_of(3) {
            let trunk = cell * TREE_SPACING + roll_range(roll, 4, 4, TREE_SPACING - 6);
            let reach = 4 + roll_range(roll, 12, 0, 3);
            if x == trunk || x == trunk + 1 {
                return Some(TileKind::Hollow(HollowTile::Trunk));
            }
            if (x - trunk).abs() <= reach + 1
                && (1..=3).any(|up| !open_at(x, y - up))
                && !hash2(seed, SALT_TREE ^ 1, x, y).is_multiple_of(5)
            {
                return Some(TileKind::Hollow(HollowTile::Canopy));
            }
        }
        return Some(TileKind::Air);
    }
    if !plain(base) {
        return None;
    }
    // Floors: ferns over loam, tar in the pits.
    let along = i32::from(layout.along(y));
    let tar_pick = variant(settings, Stratum::MegafaunaJungle) == 1;
    let tar = value_noise(seed, SALT_TAR, x, 0, 26, 1)
        > NOISE_MAX * if tar_pick { 58 } else { 66 } / 100 - along * 6;
    if open_at(x, y - 1) {
        return Some(TileKind::Hollow(if tar {
            HollowTile::Tar
        } else {
            HollowTile::Fern
        }));
    }
    if tar && open_at(x, y - 2) {
        return Some(TileKind::Hollow(HollowTile::Tar));
    }
    // Bones of old giants in the loam.
    let bone = hash2(seed, SALT_BONE, x.div_euclid(5), y.div_euclid(3));
    if bone.is_multiple_of(97) && (x.rem_euclid(5) < 4) && y.rem_euclid(3) == 1 {
        return Some(TileKind::Hollow(HollowTile::GiantBone));
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Upside-Down Isles.
// ---------------------------------------------------------------------------------------------

/// `(ceiling, floor)`: the first and last open rows of the isle sea in column `x`.
#[must_use]
pub const fn isle_sea_rows(settings: &WorldSettings, layout: &DescentLayout, x: i32) -> (i32, i32) {
    cavern_rows(
        settings,
        layout,
        Stratum::UpsideDownIsles,
        SALT_SEA,
        (22, 34),
        10,
        x,
    )
}

/// One hanging isle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Isle {
    /// Its body (the underside is `bounds.max.y`).
    pub bounds: TileRect,
    /// The stalk it hangs from, `(column, first row)`, if it reaches the roof.
    pub stalk: Option<(i32, i32)>,
    pub seed: u64,
}

impl Isle {
    /// The isle's underside row (its ground, under upside-down gravity).
    #[must_use]
    pub const fn underside(&self) -> i32 {
        self.bounds.max.y
    }

    /// Where its upside-down gravity holds (the body and the air under it).
    #[must_use]
    pub const fn bubble(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.bounds.min.x - ISLE_BUBBLE_SIDE,
                y: self.bounds.min.y,
            },
            max: TilePosition {
                x: self.bounds.max.x + ISLE_BUBBLE_SIDE,
                y: self.bounds.max.y + ISLE_BUBBLE_ROWS,
            },
        }
    }

    /// The isle's half width at row `y` (crowned upward: widest at its underside), or `None`
    /// outside its rows.
    #[must_use]
    pub fn half_width_at(&self, y: i32) -> Option<i32> {
        if y < self.bounds.min.y || y > self.bounds.max.y {
            return None;
        }
        let half = self.bounds.width() / 2;
        let height = self.bounds.height();
        let from_top = y - self.bounds.min.y + 1;
        // A rounded crown: the half width grows with the square root-ish of the depth.
        let share = from_top * 1_000 / height.max(1);
        let shaped = 1_000 - (1_000 - share) * (1_000 - share) / 1_000;
        Some((half * shaped / 1_000).max(1))
    }

    /// Whether `(x, y)` is part of its body.
    #[must_use]
    pub fn body_at(&self, x: i32, y: i32) -> bool {
        let centre = self.bounds.min.x + self.bounds.width() / 2;
        self.half_width_at(y)
            .is_some_and(|half| (x - centre).abs() <= half)
    }
}

/// The isle of cell `(cx, cy)`, if it rolled one.
#[must_use]
pub fn isle_in_cell(
    settings: &WorldSettings,
    layout: &DescentLayout,
    cx: i32,
    cy: i32,
) -> Option<Isle> {
    let (width, height) = ISLE_CELL;
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    let roll = hash2(settings.seed, SALT_ISLE, cx, cy);
    let underside = cy * height + roll_range(roll, 0, 22, height - 10);
    if underside < top || underside > bottom {
        return None;
    }
    let along = u64::from(layout.along(underside));
    let chance = if variant(settings, Stratum::UpsideDownIsles) == 0 {
        520
    } else {
        440
    } + along * 280 / 1_000;
    if (roll >> 8) % 1_000 >= chance {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, reason = "per mille")]
    let grow = (along / 200) as i32;
    let isle_width = roll_range(roll, 20, 14, 22) + grow * 2;
    let thickness = roll_range(roll, 28, 7, 10) + grow / 2;
    let centre = cx * width + roll_range(roll, 34, isle_width / 2 + 4, width - isle_width / 2 - 4);
    let left = centre - isle_width / 2;
    let right = left + isle_width - 1;
    // Room to fly under it and above the sea's floor; under the roof.
    let ceiling = (left..=right)
        .map(|x| isle_sea_rows(settings, layout, x).0)
        .max()
        .unwrap_or(top);
    let floor = (left..=right)
        .map(|x| isle_sea_rows(settings, layout, x).1)
        .min()
        .unwrap_or(bottom);
    let body_top = underside - thickness + 1;
    if body_top < ceiling + 5 || underside + ISLE_BUBBLE_ROWS + 8 > floor {
        return None;
    }
    // Keep the outposts' cable clear.
    let column = landmarks::lift_column(settings);
    if left - ISLE_BUBBLE_SIDE - 6 <= column && right + ISLE_BUBBLE_SIDE + 6 >= column {
        return None;
    }
    // Upper isles hang from the roof on a stalk; lower ones float (the inverted pull holds
    // them).
    let stalk = (body_top - ceiling < 56).then(|| {
        let roof = isle_sea_rows(settings, layout, centre).0;
        (centre, roof - 2)
    });
    Some(Isle {
        bounds: TileRect {
            min: TilePosition {
                x: left,
                y: body_top,
            },
            max: TilePosition {
                x: right,
                y: underside,
            },
        },
        stalk,
        seed: roll,
    })
}

/// Every isle whose body or bubble overlaps `area`.
#[must_use]
pub fn isles_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<Isle> {
    if !has_hollow(settings) {
        return Vec::new();
    }
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    if area.max.y < top - 80 || area.min.y > bottom + 20 {
        return Vec::new();
    }
    let (width, height) = ISLE_CELL;
    let grown = area.expanded(ISLE_BUBBLE_ROWS + 4);
    let mut found = Vec::new();
    for cy in (grown.min.y - 80).div_euclid(height)..=grown.max.y.div_euclid(height) {
        for cx in grown.min.x.div_euclid(width)..=grown.max.x.div_euclid(width) {
            if let Some(isle) = isle_in_cell(settings, layout, cx, cy) {
                let stalk_box = isle.stalk.map(|(x, roof)| TileRect {
                    min: TilePosition { x: x - 1, y: roof },
                    max: TilePosition {
                        x: x + 1,
                        y: isle.bounds.min.y,
                    },
                });
                if isle.bubble().intersects(area)
                    || stalk_box.is_some_and(|stalk| stalk.intersects(area))
                {
                    found.push(isle);
                }
            }
        }
    }
    found
}

fn isle_tile(isle: &Isle, settings: &WorldSettings, x: i32, y: i32) -> Option<TileKind> {
    if let Some((column, roof)) = isle.stalk
        && (x == column || x == column + 1)
        && y >= roof
        && y < isle.bounds.min.y + 2
    {
        return Some(TileKind::Hollow(HollowTile::IsleStalk));
    }
    if !isle.body_at(x, y) {
        return None;
    }
    if y == isle.underside() {
        return Some(TileKind::Hollow(HollowTile::IsleGrass));
    }
    let lode = hash2(settings.seed, SALT_ISLE ^ 7, x, y);
    if y > isle.bounds.min.y + 1 && y < isle.underside() - 1 && lode.is_multiple_of(9) {
        return Some(TileKind::Hollow(HollowTile::Lodestone));
    }
    Some(TileKind::Hollow(HollowTile::IsleRock))
}

// ---------------------------------------------------------------------------------------------
// The Sunforge.
// ---------------------------------------------------------------------------------------------

/// `(ceiling, floor)`: the first and last open rows of the forge sky in column `x`.
#[must_use]
pub const fn forge_rows(settings: &WorldSettings, layout: &DescentLayout, x: i32) -> (i32, i32) {
    cavern_rows(
        settings,
        layout,
        Stratum::TheSunforge,
        SALT_FORGE,
        (16, 24),
        8,
        x,
    )
}

/// Whether `(x, y)` is open air of the forge sky (not a floating stone).
#[must_use]
pub const fn forge_open(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let (ceiling, floor) = forge_rows(settings, layout, x);
    if y < ceiling || y > floor {
        return false;
    }
    let seed = settings.seed;
    let fringe = value_noise(seed, SALT_FORGE ^ 4, x, 0, 4, 1) * 7 / NOISE_MAX;
    if y < ceiling + fringe / 2 || y > floor - fringe / 3 {
        return false;
    }
    // Small floating stones, close enough to hop between: shade from the light below.
    let blob = (value_noise(seed, SALT_FORGE ^ 5, x, y, 15, 7) * 3
        + value_noise(seed, SALT_FORGE ^ 6, x, y, 5, 3))
        / 4;
    let threshold = if variant(settings, Stratum::TheSunforge) == 0 {
        67
    } else {
        71
    };
    blob < NOISE_MAX * threshold / 100
}

/// The Sun Well: the Descent V gate arena, a ring round the inner sun at the bottom of the
/// Sunforge. Its centre tile.
#[must_use]
pub fn sun_well(settings: &WorldSettings, layout: &DescentLayout) -> Option<TilePosition> {
    if !has_hollow(settings) {
        return None;
    }
    let chamber = landmarks::gate_chamber(settings, layout, Descent::HollowWorld).bounds;
    Some(TilePosition {
        x: chamber.center().x,
        y: chamber.max.y - WELL_RADIUS + 3,
    })
}

/// The Sun Well's bounding box.
#[must_use]
pub const fn sun_well_rect(centre: TilePosition) -> TileRect {
    TileRect::around(centre, WELL_RADIUS, WELL_RADIUS)
}

/// The centres of the Sun Well's four shade pillars (on the diagonals).
#[must_use]
pub fn shade_pillars(centre: TilePosition) -> [TilePosition; 4] {
    // 45 degrees: about 0.707 of the distance on each axis.
    let d = PILLAR_DISTANCE * 707 / 1_000;
    [(-1, -1), (1, -1), (-1, 1), (1, 1)].map(|(sx, sy)| TilePosition {
        x: centre.x + sx * d,
        y: centre.y + sy * d,
    })
}

/// The Sun Well's tile at `(x, y)` (inside its box), or `None` outside it.
#[must_use]
pub fn well_tile(centre: TilePosition, x: i32, y: i32) -> Option<TileKind> {
    let (dx, dy) = (x - centre.x, y - centre.y);
    let d2 = dx * dx + dy * dy;
    if d2 > WELL_RADIUS * WELL_RADIUS {
        return None;
    }
    if d2 <= SUN_RADIUS * SUN_RADIUS {
        return Some(TileKind::Hollow(HollowTile::InnerSun));
    }
    for pillar in shade_pillars(centre) {
        if (x - pillar.x).abs() <= 1 && (y - pillar.y).abs() <= 2 {
            return Some(TileKind::Hollow(HollowTile::ShadePillar));
        }
    }
    let inner = WELL_RADIUS - 2;
    if d2 > inner * inner {
        // Four doorways: top, bottom and both sides.
        let door = dx.abs() <= 3 || dy.abs() <= 3;
        return Some(if door {
            TileKind::Air
        } else {
            TileKind::Hollow(HollowTile::WellWall)
        });
    }
    // A ledge under each side doorway to land on.
    if dy == 4 && dx.abs() >= inner - 4 {
        return Some(TileKind::Hollow(HollowTile::WellWall));
    }
    Some(TileKind::Air)
}

const fn forge_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if forge_open(settings, layout, x, y) {
        return Some(TileKind::Air);
    }
    if !plain(base) && !matches!(base, TileKind::Lava | TileKind::MagmaVent) {
        return None;
    }
    let seed = settings.seed;
    // The forge's floor: lava lakes under a crust of slag.
    let (_, floor) = forge_rows(settings, layout, x);
    if y > floor - 4 && y <= floor + 3 && forge_open(settings, layout, x, y - 1) {
        let lake = value_noise(seed, SALT_FORGE ^ 9, x, 0, 18, 1) > NOISE_MAX * 45 / 100;
        return Some(if lake {
            TileKind::Lava
        } else {
            TileKind::Hollow(HollowTile::Slag)
        });
    }
    if !plain(base) {
        return None;
    }
    // Sun glass seams in the rock.
    let seam = value_noise(seed, SALT_GLASS, x, y, 9, 5);
    if seam > NOISE_MAX * 86 / 100 {
        return Some(TileKind::Hollow(HollowTile::SunGlass));
    }
    None
}

// ---------------------------------------------------------------------------------------------
// The pass.
// ---------------------------------------------------------------------------------------------

/// What one chunk's pass needs (computed once per chunk).
#[derive(Clone, Debug, Default)]
pub struct HollowChunk {
    pub isles: Vec<Isle>,
    pub well: Option<TilePosition>,
    pub breach: Option<TileRect>,
    pub valley: Option<TileRect>,
    /// Tiles the pass never carves (the Descent landmarks and the tribes' hall).
    pub keep: Vec<TileRect>,
}

/// The rows the pass touches: Descent V's, with a margin.
#[must_use]
pub const fn pass_rows(layout: &DescentLayout) -> (i32, i32) {
    let (first, last) = layout.descent_rows(Descent::HollowWorld);
    (first - 4, last + 8)
}

/// The pass's view of chunk `rect` (empty outside the Hollow World).
#[must_use]
pub fn chunk_context(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rect: &TileRect,
) -> HollowChunk {
    let mut context = HollowChunk::default();
    if !has_hollow(settings) {
        return context;
    }
    context.isles = isles_touching(settings, layout, rect);
    context.well =
        sun_well(settings, layout).filter(|centre| sun_well_rect(*centre).intersects(rect));
    context.breach = breach(settings, layout).filter(|breach| breach.intersects(rect));
    context.valley =
        sunfolk_valley(settings, layout).filter(|valley| valley.expanded(2).intersects(rect));
    for descent in [
        Descent::DrownedAndFrozen,
        Descent::HollowWorld,
        Descent::StrangeLayers,
    ] {
        if let Some(outpost) = landmarks::outpost(settings, layout, descent) {
            context.keep.push(outpost.bounds.expanded(1));
        }
    }
    let grown = rect.expanded(4);
    for landmark in landmarks::landmarks_with(settings, layout, &grown) {
        if landmark.kind == crate::descent::LandmarkKind::AnomalyRoom {
            context.keep.push(landmark.bounds.expanded(1));
        }
    }
    if let Some((site, center)) = crate::worldgen::streamed::villages::sunfolk_hall(settings) {
        let first = center - crate::village::placement::HALL_WIDTH / 2;
        context.keep.push(TileRect {
            min: TilePosition {
                x: first - 1,
                y: site.top - 1,
            },
            max: TilePosition {
                x: first + crate::village::placement::HALL_WIDTH,
                y: site.top + crate::village::placement::HALL_AIR_ROWS + 1,
            },
        });
    }
    context
}

/// Whether the pass may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_)
            | TileKind::Poi(_)
            | TileKind::Bedrock
            | TileKind::Fluid(_)
            | TileKind::Crust(_)
            | TileKind::Deep(_)
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
        TileKind::Hollow(tile) => tile.plain_rock(),
        _ => false,
    }
}

/// The Hollow World's tile at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn hollow_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    chunk: &HollowChunk,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !carvable(base) || chunk.keep.iter().any(|keep| keep.contains(at)) {
        return None;
    }
    if let Some(centre) = chunk.well
        && let Some(kind) = well_tile(centre, x, y)
    {
        return Some(kind);
    }
    if let Some(breach) = &chunk.breach
        && let Some(kind) = breach_tile(breach, x, y)
    {
        return Some(kind);
    }
    let stratum = stratum_in(settings, layout, x, y);
    match stratum {
        Stratum::InnerSky => {
            let solid = |cx: i32, cy: i32| {
                !super::hollow_open(settings, layout, cx, cy) || sky_stone(settings, layout, cx, cy)
            };
            let stone = base == TileKind::Air && sky_stone(settings, layout, x, y);
            if !stone && !matches!(base, TileKind::Hollow(HollowTile::SkyStone)) {
                return None;
            }
            // Sky amber in the skins of the floating stones and the rock sky.
            if solid(x, y + 1) != solid(x, y - 1)
                && hash2(settings.seed, SALT_AMBER, x, y).is_multiple_of(7)
            {
                return Some(TileKind::Hollow(HollowTile::SkyAmber));
            }
            stone.then_some(TileKind::Hollow(HollowTile::SkyStone))
        }
        Stratum::MegafaunaJungle => {
            jungle_tile(settings, layout, chunk.valley.as_ref(), x, y, base)
        }
        Stratum::UpsideDownIsles => {
            for isle in &chunk.isles {
                if let Some(kind) = isle_tile(isle, settings, x, y) {
                    return Some(kind);
                }
            }
            let (ceiling, floor) = isle_sea_rows(settings, layout, x);
            let fringe = value_noise(settings.seed, SALT_SEA ^ 4, x, 0, 5, 1) * 9 / NOISE_MAX;
            (y >= ceiling + fringe / 2 && y <= floor - fringe / 3).then_some(TileKind::Air)
        }
        Stratum::TheSunforge => forge_tile(settings, layout, x, y, base),
        _ => None,
    }
}

/// Whether `area` crosses one of the Hollow World's set pieces or open caverns (the story's
/// camps and rooms keep off them: a room would float in the open air).
#[must_use]
pub fn crosses_set_pieces(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    if !has_hollow(settings) {
        return false;
    }
    let (first, last) = pass_rows(layout);
    if area.max.y < first || area.min.y > last {
        return false;
    }
    if sun_well(settings, layout)
        .is_some_and(|centre| sun_well_rect(centre).expanded(4).intersects(area))
        || breach(settings, layout).is_some_and(|breach| breach.expanded(4).intersects(area))
        || sunfolk_valley(settings, layout)
            .is_some_and(|valley| valley.expanded(6).intersects(area))
        || !isles_touching(settings, layout, &area.expanded(4)).is_empty()
    {
        return true;
    }
    // Any open air of a cavern in the area (sampled on a coarse grid).
    let step = 3;
    let mut y = area.min.y;
    while y <= area.max.y {
        let mut x = area.min.x;
        while x <= area.max.x {
            let open = match stratum_in(settings, layout, x, y) {
                Stratum::InnerSky => super::hollow_open(settings, layout, x, y),
                Stratum::MegafaunaJungle => jungle_open(settings, layout, x, y),
                Stratum::UpsideDownIsles => {
                    let (ceiling, floor) = isle_sea_rows(settings, layout, x);
                    y >= ceiling - 2 && y <= floor + 2
                }
                Stratum::TheSunforge => {
                    let (ceiling, floor) = forge_rows(settings, layout, x);
                    y >= ceiling - 2 && y <= floor + 4
                }
                _ => false,
            };
            if open {
                return true;
            }
            x += step;
        }
        y += step;
    }
    false
}

#[cfg(test)]
mod tests;
