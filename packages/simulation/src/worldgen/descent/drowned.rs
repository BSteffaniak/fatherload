//! The Drowned and Frozen's generated places (Wave 9, Descent IV of
//! [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)).
//!
//! A pure function of the settings and the tile (chunks carve the same in any order), run by
//! `streamed::assemble` after the Living Dark's pass, only over chunks near Descent IV:
//!
//! * **Region stamps** ([`stamps_with`]): at most one per [`DROWNED_REGION`] cell whose centre
//!   lies in a Drowned stratum, rolled more often deeper in it (introduced gently, then
//!   pushed):
//!   * Underground Ocean **basins**: great flooded caverns, a breath of air under their roofs,
//!     kelp forests and coral on their floors; each holds a gyre current ([`Basin`]). Deeper
//!     basins are bigger and their currents stronger.
//!   * Brine Glaciers **ice galleries** (slick floors of brine ice, pack-ice walls, icicles)
//!     and **vent grottos** (thermal vents: the warm safe spots, rarer deeper).
//!   * Geyser Fields **chimneys**: a vent chamber under a tall shaft the geyser throws rigs up
//!     (taller deeper).
//!   * Merrow City **reef terraces**: coral-lined pools on the city's flanks.
//!   * Pearl Abyss **pearl beds**: flooded caverns floored with giant clams.
//! * **Fixed stamps** (one per world): the eel-dragon's **grotto** and the **drowned builder
//!   sub** (the ocean), the **frozen figures** round the builders' cable (the glaciers),
//!   **Old Faithful** (the great geyser and its 60-row shaft), **Merrow City's bubble**
//!   ([`merrow_city`], round the Descent IV outpost and the merrow's settlement hall) and the
//!   **Leviathan's arena** ([`leviathan_arena`], the flooded gate of Descent IV).
//!
//! Only plain rock, ore, hazards and soft cave dressing are carved; built pieces (the outpost,
//! the cable), points of interest, bedrock and existing fluid stay.

use crate::{
    cave::CaveTile,
    descent::{Descent, Stratum},
    fluid::FluidKind,
    terrain::{TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X, TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, value_noise},
    },
};

use super::{DescentLayout, landmarks, variant};

/// The cells drowned stamps are rolled per, `(width, height)` in tiles.
pub const DROWNED_REGION: (i32, i32) = (72, 48);
/// `(width, height)` of the eel-dragon's grotto.
pub const GROTTO_SIZE: (i32, i32) = (52, 24);
/// `(width, height)` of the drowned builder sub's wreck basin.
pub const WRECK_SIZE: (i32, i32) = (30, 14);
/// `(width, height)` of the frozen figures' gallery round the cable.
pub const FIGURES_SIZE: (i32, i32) = (40, 14);
/// Old Faithful: its throat chamber `(width, height)` and the shaft's height in rows.
pub const OLD_FAITHFUL_CHAMBER: (i32, i32) = (18, 7);
pub const OLD_FAITHFUL_SHAFT_ROWS: i32 = 60;
/// Width of Old Faithful's shaft.
pub const OLD_FAITHFUL_SHAFT_WIDTH: i32 = 5;
/// `(width, height)` of the Leviathan's arena.
pub const ARENA_SIZE: (i32, i32) = (104, 40);
/// Rows of air under a basin's or the arena's roof.
pub const AIR_ROWS: i32 = 2;
/// Rows of Merrow City's bubble above its floor.
pub const DOME_ROWS: i32 = 24;
/// Thickness of the sea held round the bubble's glass, tiles.
pub const DOME_SEA: i32 = 6;

const SALT_REGION: u64 = 0xD20_0001;
const SALT_SHAPE: u64 = 0xD20_0002;
const SALT_DECOR: u64 = 0xD20_0003;
const SALT_FIXED: u64 = 0xD20_0004;
const SALT_CORAL: u64 = 0xD20_0005;

/// What a drowned stamp is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DrownedStampKind {
    Basin,
    EelGrotto,
    Wreck,
    IceGallery,
    VentGrotto,
    FrozenFigures,
    Chimney,
    OldFaithful,
    ReefTerrace,
    PearlBed,
    LeviathanArena,
    /// A flooded channel joining two neighbouring Underground Ocean basins (Wave 10 polish:
    /// the basins are one sea): sideways under both basins' surfaces, or a shaft down from one
    /// basin into the one below.
    Channel,
}

/// One drowned set piece.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrownedStamp {
    pub kind: DrownedStampKind,
    /// Every tile it may change.
    pub bounds: TileRect,
    /// Per-stamp variation.
    pub seed: u64,
    /// How deep in its stratum it is, per mille.
    pub along: u16,
}

/// A basin's gyre: where it circles, how wide, which way (the tick reads it for currents).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Basin {
    pub bounds: TileRect,
    pub clockwise: bool,
    pub along: u16,
}

impl DrownedStamp {
    /// The gyre of a flooded stamp (basins, the grotto and the arena), `None` for others.
    #[must_use]
    pub const fn basin(&self) -> Option<Basin> {
        match self.kind {
            DrownedStampKind::Basin
            | DrownedStampKind::EelGrotto
            | DrownedStampKind::LeviathanArena => Some(Basin {
                bounds: self.bounds,
                clockwise: self.seed.is_multiple_of(2),
                along: self.along,
            }),
            _ => None,
        }
    }

    /// Old Faithful's (or a chimney's) vent row and columns: the throat the geyser blows from.
    #[must_use]
    pub const fn vents(&self) -> Option<(i32, i32, i32)> {
        match self.kind {
            DrownedStampKind::Chimney => {
                let cx = self.bounds.min.x + self.bounds.width() / 2;
                Some((self.bounds.max.y, cx - 1, cx + 1))
            }
            DrownedStampKind::OldFaithful => {
                let cx = self.bounds.min.x + self.bounds.width() / 2;
                let half = OLD_FAITHFUL_SHAFT_WIDTH / 2;
                Some((self.bounds.max.y, cx - half, cx + half))
            }
            _ => None,
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

/// Whether the world has the Drowned and Frozen.
#[must_use]
pub const fn has_drowned(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// The side of the spawn the outposts' cable stands on (`1` east, `-1` west).
const fn cable_side(settings: &WorldSettings) -> i32 {
    if landmarks::outpost_column(settings) >= SPAWN_TILE_X {
        1
    } else {
        -1
    }
}

/// A row `per_mille` of the way down `stratum`, clamped clear of its ragged edges for a stamp
/// `height` tall.
fn row_in(layout: &DescentLayout, stratum: Stratum, per_mille: i32, height: i32) -> i32 {
    let (top, bottom) = layout.rows(stratum);
    let span = bottom - top + 1;
    let row = top + span * per_mille / 1_000 - height / 2;
    row.clamp(top + 18, (bottom - 18 - height).max(top + 18))
}

#[allow(clippy::cast_possible_truncation, reason = "per mille")]
const fn along_of(layout: &DescentLayout, bounds: &TileRect) -> u16 {
    layout.along(bounds.center().y)
}

/// The eel-dragon's grotto: a great flooded cave in the ocean, away from the cable.
#[must_use]
pub fn eel_grotto(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_drowned(settings) {
        return None;
    }
    let roll = hash2(settings.seed, SALT_FIXED, 1, 0);
    let centre = SPAWN_TILE_X - cable_side(settings) * roll_range(roll, 0, 90, 140);
    let row = row_in(layout, Stratum::UndergroundOcean, 640, GROTTO_SIZE.1);
    Some(rect(centre - GROTTO_SIZE.0 / 2, row, GROTTO_SIZE))
}

/// The drowned builder sub: a wreck in a flooded hollow of the ocean, on the cable's side.
#[must_use]
pub fn drowned_sub(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_drowned(settings) {
        return None;
    }
    let roll = hash2(settings.seed, SALT_FIXED, 2, 0);
    let cable = landmarks::lift_column(settings);
    let centre = cable - cable_side(settings) * roll_range(roll, 0, 36, 60);
    let row = row_in(layout, Stratum::UndergroundOcean, 340, WRECK_SIZE.1);
    Some(rect(centre - WRECK_SIZE.0 / 2, row, WRECK_SIZE))
}

/// The wreck's hull inside its basin: `(left, top, right, bottom)` of the plating box.
#[must_use]
pub const fn wreck_hull(basin: &TileRect) -> TileRect {
    let cx = basin.min.x + basin.width() / 2;
    let floor = basin.max.y - 2;
    TileRect {
        min: TilePosition {
            x: cx - 8,
            y: floor - 5,
        },
        max: TilePosition {
            x: cx + 8,
            y: floor - 1,
        },
    }
}

/// The frozen figures' gallery, round the builders' cable in the middle of the glaciers.
#[must_use]
pub fn frozen_figures(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_drowned(settings) {
        return None;
    }
    let cable = landmarks::lift_column(settings);
    let row = row_in(layout, Stratum::BrineGlaciers, 520, FIGURES_SIZE.1);
    Some(rect(cable - FIGURES_SIZE.0 / 2, row, FIGURES_SIZE))
}

/// Old Faithful: the whole stamp (the throat chamber at its bottom, the shaft above, a ledge
/// chamber at its top).
#[must_use]
pub fn old_faithful(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_drowned(settings) {
        return None;
    }
    let roll = hash2(settings.seed, SALT_FIXED, 3, 0);
    let centre = SPAWN_TILE_X - cable_side(settings) * roll_range(roll, 0, 110, 150);
    let (top, bottom) = layout.rows(Stratum::GeyserFields);
    let floor = (top + (bottom - top) * 85 / 100).min(bottom - 20);
    let height = OLD_FAITHFUL_SHAFT_ROWS
        .min(floor - top - 16)
        .max(OLD_FAITHFUL_CHAMBER.1 + 12);
    Some(TileRect {
        min: TilePosition {
            x: centre - OLD_FAITHFUL_CHAMBER.0 / 2,
            y: floor - height,
        },
        max: TilePosition {
            x: centre - OLD_FAITHFUL_CHAMBER.0 / 2 + OLD_FAITHFUL_CHAMBER.0 - 1,
            y: floor,
        },
    })
}

/// Merrow City's geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MerrowSite {
    /// The merrow's settlement hall: its centre column and first air row (it shares the
    /// outpost's floor).
    pub hall_center: i32,
    pub hall_top: i32,
    /// The floor row (the first solid row under the city).
    pub floor: i32,
    /// The bubble: its columns (the cable's rail bounds it on the far side) and its top row.
    pub dome: TileRect,
    /// Which way the city runs from the outpost (`-1` west, `1` east): the hall's side.
    pub side: i32,
    /// The outpost's hall.
    pub outpost: TileRect,
}

impl MerrowSite {
    /// The columns of the two moon pools in the city floor (each three wide): where a
    /// submarine surfaces into the city from the harbour below.
    #[must_use]
    pub const fn moon_pools(&self) -> [i32; 2] {
        let hall_left = self.hall_center - crate::village::placement::HALL_WIDTH / 2;
        let hall_right = hall_left + crate::village::placement::HALL_WIDTH - 1;
        if self.side < 0 {
            [hall_left - 5, hall_right + 2]
        } else {
            [hall_right + 3, hall_left - 4]
        }
    }

    /// The harbour under the city: a flooded hollow the moon pools open into.
    #[must_use]
    pub const fn harbour(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.dome.min.x + 3,
                y: self.floor + 1,
            },
            max: TilePosition {
                x: self.dome.max.x - 3,
                y: self.floor + 10,
            },
        }
    }

    /// Everything the city's stamp may change.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.dome.min.x - DOME_SEA - 2,
                y: self.dome.min.y - DOME_SEA - 2,
            },
            max: TilePosition {
                x: self.dome.max.x + DOME_SEA + 2,
                y: self.floor + 12,
            },
        }
    }

    /// Whether `tile` is inside the city's air (under the glass, over the floor).
    #[must_use]
    pub fn in_city(&self, tile: TilePosition) -> bool {
        tile.y < self.floor && dome_permille(self, tile.x, tile.y) < 1_000
    }
}

/// Merrow City's site round the Descent IV outpost (`None` before V11).
#[must_use]
pub fn merrow_city(settings: &WorldSettings) -> Option<MerrowSite> {
    use crate::village::placement::{HALL_AIR_ROWS, HALL_WIDTH};
    use landmarks::{OUTPOST_SIZE, outpost, outpost_column};
    if !has_drowned(settings) {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let hall = outpost(settings, &layout, Descent::DrownedAndFrozen)?;
    let floor = hall.bounds.max.y;
    let column = outpost_column(settings);
    let side = -cable_side(settings);
    let hall_center = column + side * (OUTPOST_SIZE.0 / 2 + 4 + HALL_WIDTH / 2);
    let cable = landmarks::lift_column(settings);
    let near = hall_center + side * (HALL_WIDTH / 2 + 10);
    // The far end stops short of the cable's rail.
    let far = cable + side;
    let (left, right) = if side < 0 { (near, far) } else { (far, near) };
    Some(MerrowSite {
        hall_center,
        hall_top: floor - HALL_AIR_ROWS,
        floor,
        dome: TileRect {
            min: TilePosition {
                x: left,
                y: floor - DOME_ROWS,
            },
            max: TilePosition { x: right, y: floor },
        },
        side,
        outpost: hall.bounds,
    })
}

/// How far `(x, y)` is inside the city's bubble, per mille (below 1000 inside): a half
/// ellipse over the floor, tall enough that the outpost and the hall stand under it, whose
/// rim runs a few columns past the far end, which the cable cuts off.
fn dome_permille(site: &MerrowSite, x: i32, y: i32) -> i64 {
    dome_permille_scaled(site, x, y, 0)
}

/// The Leviathan's arena: a flooded cavern at the end of Descent IV (its gate), clear of the
/// cable.
#[must_use]
pub fn leviathan_arena(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !has_drowned(settings) {
        return None;
    }
    let chamber = landmarks::gate_chamber(settings, layout, Descent::DrownedAndFrozen).bounds;
    let cable = landmarks::lift_column(settings);
    let mut centre = chamber.center().x;
    let reach = ARENA_SIZE.0 / 2 + 8;
    if (centre - cable).abs() < reach {
        centre = if centre <= cable {
            cable - reach
        } else {
            cable + reach
        };
    }
    let bottom = chamber.max.y + 2;
    Some(rect(
        centre - ARENA_SIZE.0 / 2,
        bottom - ARENA_SIZE.1 + 1,
        ARENA_SIZE,
    ))
}

/// The arena's floor row (its last open row is above it).
#[must_use]
pub const fn arena_floor_row(arena: &TileRect) -> i32 {
    arena.max.y - 3
}

/// The fixed stamps.
fn fixed_stamps(settings: &WorldSettings, layout: &DescentLayout) -> Vec<DrownedStamp> {
    let mut stamps = Vec::new();
    let mut push = |kind, bounds: Option<TileRect>, index: i32| {
        if let Some(bounds) = bounds {
            stamps.push(DrownedStamp {
                kind,
                bounds,
                seed: hash2(settings.seed, SALT_SHAPE, -index, 0),
                along: along_of(layout, &bounds),
            });
        }
    };
    push(DrownedStampKind::EelGrotto, eel_grotto(settings, layout), 1);
    push(DrownedStampKind::Wreck, drowned_sub(settings, layout), 2);
    push(
        DrownedStampKind::FrozenFigures,
        frozen_figures(settings, layout),
        3,
    );
    push(
        DrownedStampKind::OldFaithful,
        old_faithful(settings, layout),
        4,
    );
    push(
        DrownedStampKind::LeviathanArena,
        leviathan_arena(settings, layout),
        5,
    );
    stamps
}

/// The stamp of region `(rx, ry)`, if it rolled one.
#[allow(clippy::too_many_lines, reason = "one row per stratum's stamp")]
fn region_stamp(
    settings: &WorldSettings,
    layout: &DescentLayout,
    fixed: &[DrownedStamp],
    city: Option<&MerrowSite>,
    rx: i32,
    ry: i32,
) -> Option<DrownedStamp> {
    let (width, height) = DROWNED_REGION;
    let centre_row = ry * height + height / 2;
    let stratum = layout.stratum_of_row(centre_row);
    if !crate::drowned::is_drowned(stratum) {
        return None;
    }
    let along = i32::from(layout.along(centre_row));
    let roll = hash2(settings.seed, SALT_REGION, rx, ry);
    let pick = variant(settings, stratum);
    // (kind, chance per mille at the top, extra by the bottom, width and height ranges)
    let (kind, base, extra, w, h) = match stratum {
        // Wave 10 polish: a basin in nearly every cell, wide and deep, so that with the
        // channels between neighbours the Underground Ocean is one sea.
        Stratum::UndergroundOcean => (
            DrownedStampKind::Basin,
            900,
            100,
            (50 + along / 60, 60 + along / 80),
            (22 + along / 100, 30 + along / 80),
        ),
        Stratum::BrineGlaciers => {
            // Vent grottos are common at the top (the frost is introduced gently) and rare at
            // the bottom; ice galleries the other way round.
            let vent = (roll >> 50) % 1_000 < u64::try_from(620 - along / 2).unwrap_or(0);
            if vent {
                (DrownedStampKind::VentGrotto, 520, 0, (12, 18), (7, 9))
            } else {
                (
                    DrownedStampKind::IceGallery,
                    if pick == 0 { 420 } else { 340 },
                    260,
                    (26, 44),
                    (10, 14),
                )
            }
        }
        Stratum::GeyserFields => (
            DrownedStampKind::Chimney,
            if pick == 0 { 560 } else { 460 },
            300,
            (9, 11),
            (16 + along / 50, 22 + along / 40),
        ),
        Stratum::MerrowCity => (
            DrownedStampKind::ReefTerrace,
            if pick == 1 { 520 } else { 380 },
            120,
            (20, 34),
            (9, 13),
        ),
        Stratum::PearlAbyss => (
            DrownedStampKind::PearlBed,
            if pick == 1 { 460 } else { 360 },
            300,
            (24, 40),
            (12, 18),
        ),
        _ => return None,
    };
    let chance = u64::try_from(base + extra * along / 1_000).unwrap_or(0);
    if roll % 1_000 >= chance {
        return None;
    }
    let sw = roll_range(roll, 10, w.0, w.1.min(width - 8));
    let sh = roll_range(roll, 20, h.0, h.1.min(height - 6));
    let x = rx * width + roll_range(roll, 30, 3, (width - sw - 3).max(3));
    let y = ry * height + roll_range(roll, 40, 3, (height - sh - 3).max(3));
    let bounds = rect(x, y, (sw, sh));
    let (top, bottom) = layout.rows(stratum);
    if bounds.min.y < top + 16 || bounds.max.y > bottom - 16 {
        return None;
    }
    if fixed
        .iter()
        .any(|stamp| stamp.bounds.expanded(6).intersects(&bounds))
        || city.is_some_and(|site| site.bounds().expanded(6).intersects(&bounds))
    {
        return None;
    }
    // Keep the outposts' column and the builders' cable clear.
    let column = landmarks::outpost_column(settings);
    if bounds.max.x >= column - 24 && bounds.min.x <= column + 24 {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "per mille"
    )]
    Some(DrownedStamp {
        kind,
        bounds,
        seed: hash2(settings.seed, SALT_SHAPE, rx, ry),
        along: along as u16,
    })
}

/// Every drowned stamp whose bounds overlap `area` (fixed stamps first, then by region).
/// Empty for worlds without Descents.
#[must_use]
pub fn stamps_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<DrownedStamp> {
    if !has_drowned(settings) {
        return Vec::new();
    }
    let (first, last) = layout.descent_rows(Descent::DrownedAndFrozen);
    if area.max.y < first - 48 || area.min.y > last + 48 {
        return Vec::new();
    }
    let fixed = fixed_stamps(settings, layout);
    let city = merrow_city(settings);
    let mut found: Vec<DrownedStamp> = fixed
        .iter()
        .copied()
        .filter(|stamp| stamp.bounds.intersects(area))
        .collect();
    let (width, height) = DROWNED_REGION;
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(stamp) = region_stamp(settings, layout, &fixed, city.as_ref(), rx, ry)
                && stamp.bounds.intersects(area)
            {
                found.push(stamp);
            }
        }
    }
    // The channels joining neighbouring basins (after every region stamp: a channel's
    // walls never cut into a basin, its water runs through their rims).
    let basin_at = |rx: i32, ry: i32| {
        region_stamp(settings, layout, &fixed, city.as_ref(), rx, ry)
            .filter(|stamp| stamp.kind == DrownedStampKind::Basin)
    };
    for ry in area.min.y.div_euclid(height) - 1..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width) - 1..=area.max.x.div_euclid(width) {
            let Some(here) = basin_at(rx, ry) else {
                continue;
            };
            let joins = [
                basin_at(rx + 1, ry).and_then(|east| side_channel(&here, &east)),
                basin_at(rx, ry + 1).and_then(|below| shaft_channel(&here, &below)),
            ];
            for channel in joins.into_iter().flatten() {
                let clear = channel.bounds.intersects(area)
                    && !fixed
                        .iter()
                        .any(|stamp| stamp.bounds.expanded(3).intersects(&channel.bounds))
                    && !city
                        .is_some_and(|site| site.bounds().expanded(3).intersects(&channel.bounds));
                let column = landmarks::outpost_column(settings);
                let crosses_column =
                    channel.bounds.max.x >= column - 24 && channel.bounds.min.x <= column + 24;
                if clear && !crosses_column {
                    found.push(channel);
                }
            }
        }
    }
    found
}

/// Rows of a channel's water (its walls are one more row or column each side).
const CHANNEL_WATER: i32 = 5;

/// `(water surface row, floor row)` of a basin (as [`basin`] carves it).
fn basin_water_rows(stamp: &DrownedStamp) -> (i32, i32) {
    let (_, cy, _, ry) = ellipse(&stamp.bounds);
    let surface = stamp.bounds.min.y + (cy - stamp.bounds.min.y) / 4 + AIR_ROWS;
    (surface, cy + ry * 6 / 10)
}

/// A sideways channel from basin `west` to basin `east` under both their surfaces, or `None`
/// when their water rows do not overlap enough.
fn side_channel(west: &DrownedStamp, east: &DrownedStamp) -> Option<DrownedStamp> {
    let (west_surface, west_floor) = basin_water_rows(west);
    let (east_surface, east_floor) = basin_water_rows(east);
    let low = west_surface.max(east_surface) + 2;
    let high = west_floor.min(east_floor) - 2;
    if high - low < CHANNEL_WATER {
        return None;
    }
    let top = i32::midpoint(low, high) - CHANNEL_WATER / 2;
    let (west_x, _, _, _) = ellipse(&west.bounds);
    let (east_x, _, _, _) = ellipse(&east.bounds);
    Some(DrownedStamp {
        kind: DrownedStampKind::Channel,
        bounds: TileRect {
            min: TilePosition {
                x: west_x,
                y: top - 1,
            },
            max: TilePosition {
                x: east_x,
                y: top + CHANNEL_WATER,
            },
        },
        seed: west.seed ^ east.seed.rotate_left(17),
        along: west.along,
    })
}

/// A shaft from basin `upper`'s water down into basin `lower`'s, or `None` when they do not
/// overlap enough sideways.
fn shaft_channel(upper: &DrownedStamp, lower: &DrownedStamp) -> Option<DrownedStamp> {
    let left = upper.bounds.min.x.max(lower.bounds.min.x) + 6;
    let right = upper.bounds.max.x.min(lower.bounds.max.x) - 6;
    if right - left < CHANNEL_WATER {
        return None;
    }
    let x = i32::midpoint(left, right) - CHANNEL_WATER / 2;
    let (_, upper_y, _, _) = ellipse(&upper.bounds);
    let (lower_surface, _) = basin_water_rows(lower);
    if lower_surface <= upper_y {
        return None;
    }
    Some(DrownedStamp {
        kind: DrownedStampKind::Channel,
        bounds: TileRect {
            min: TilePosition {
                x: x - 1,
                y: upper_y,
            },
            max: TilePosition {
                x: x + CHANNEL_WATER,
                y: lower_surface + 1,
            },
        },
        seed: upper.seed ^ lower.seed.rotate_left(29),
        along: upper.along,
    })
}

/// Whether `(x, y)` is in a channel's water (rather than its walls).
const fn channel_water(stamp: &DrownedStamp, x: i32, y: i32) -> bool {
    let bounds = stamp.bounds;
    if bounds.max.x - bounds.min.x > bounds.max.y - bounds.min.y {
        y > bounds.min.y && y < bounds.max.y
    } else {
        x > bounds.min.x && x < bounds.max.x
    }
}

/// A channel's tile: water inside, seabed (now and then coral) walls.
const fn channel(stamp: &DrownedStamp, x: i32, y: i32) -> TileKind {
    if channel_water(stamp, x, y) {
        return WATER;
    }
    if hash2(stamp.seed, SALT_CORAL, x, y).is_multiple_of(6) {
        cave(CaveTile::Coral)
    } else {
        cave(CaveTile::Seabed)
    }
}

/// Every fixed set piece's bounds (the story's sites keep off them).
#[must_use]
pub fn set_piece_bounds(settings: &WorldSettings) -> Vec<TileRect> {
    if !has_drowned(settings) {
        return Vec::new();
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let mut found: Vec<TileRect> = fixed_stamps(settings, &layout)
        .iter()
        .map(|stamp| stamp.bounds)
        .collect();
    found.extend(merrow_city(settings).map(|site| site.bounds()));
    found
}

/// Whether a stamp may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Poi(_) | TileKind::Bedrock | TileKind::Fluid(_)
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
    (dx * dx + dy * dy) / 1_000
}

const WATER: TileKind = TileKind::Fluid(FluidKind::Water);
const fn cave(tile: CaveTile) -> TileKind {
    TileKind::Cave(tile)
}

/// The tile `stamp` puts at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn stamp_tile(stamp: &DrownedStamp, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !stamp.bounds.contains(at) || !carvable(base) {
        return None;
    }
    match stamp.kind {
        DrownedStampKind::Basin | DrownedStampKind::EelGrotto => basin(stamp, x, y),
        DrownedStampKind::Wreck => wreck(stamp, x, y),
        DrownedStampKind::IceGallery => ice_gallery(stamp, x, y),
        DrownedStampKind::VentGrotto => vent_grotto(stamp, x, y),
        DrownedStampKind::FrozenFigures => figures_gallery(stamp, x, y),
        DrownedStampKind::Chimney | DrownedStampKind::OldFaithful => chimney(stamp, x, y),
        DrownedStampKind::ReefTerrace => reef_terrace(stamp, x, y),
        DrownedStampKind::PearlBed => pearl_bed(stamp, x, y),
        DrownedStampKind::LeviathanArena => arena(stamp, x, y),
        DrownedStampKind::Channel => Some(channel(stamp, x, y)),
    }
}

/// A flooded basin (and the eel-dragon's grotto): water under a breath of air, a ragged seabed
/// with kelp forests and coral, a rim of seabed rock.
fn basin(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    let wobble =
        i64::from(value_noise(stamp.seed, SALT_DECOR, x, y, 6, 4)) * 120 / i64::from(NOISE_MAX);
    if inside + wobble >= 1_000 {
        return None;
    }
    if inside + wobble >= 880 {
        // The rim: seabed, coral here and there.
        return Some(if hash2(stamp.seed, SALT_CORAL, x, y).is_multiple_of(7) {
            cave(CaveTile::Coral)
        } else {
            cave(CaveTile::Seabed)
        });
    }
    let (cx, cy, rx, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 6 / 10;
    if y > floor {
        return Some(cave(CaveTile::Seabed));
    }
    // Kelp: strands rising from the floor, every few columns (thicker in the Kelp Trench).
    let strand = hash2(stamp.seed, SALT_DECOR, x, 0);
    let kelpy = stamp.kind == DrownedStampKind::Basin && (x - cx).abs() < rx - 3;
    if kelpy && strand % 5 < 2 {
        let tall = i32::try_from(strand >> 8 & 7).unwrap_or(0) + 2;
        if y > floor - tall {
            return Some(cave(CaveTile::Kelp));
        }
    }
    if y == floor && strand % 9 == 4 {
        return Some(cave(CaveTile::Coral));
    }
    let surface = stamp.bounds.min.y + (cy - stamp.bounds.min.y) / 4 + AIR_ROWS;
    Some(if y < surface { TileKind::Air } else { WATER })
}

/// The drowned builder sub: a flooded hollow with the wreck on its floor (plating hull, glass
/// ports, an air pocket and a cache inside), its hatch facing the cable.
fn wreck(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let hull = wreck_hull(&stamp.bounds);
    let at = TilePosition { x, y };
    if hull.contains(at) {
        let edge = x == hull.min.x || x == hull.max.x || y == hull.min.y || y == hull.max.y;
        if edge {
            // Round ports along the side.
            if y == hull.min.y + 2 && (x - hull.min.x) % 4 == 2 {
                return Some(TileKind::Built(crate::base::BuiltTile::Glass));
            }
            // The hatch: an opening on one end.
            if x == hull.max.x && y >= hull.max.y - 2 && y < hull.max.y {
                return Some(TileKind::Air);
            }
            return Some(TileKind::Built(crate::base::BuiltTile::Plating));
        }
        if x == hull.min.x + 2 && y == hull.max.y - 1 {
            return Some(cave(CaveTile::TreasureCache));
        }
        return Some(TileKind::Air);
    }
    if inside >= 860 {
        return Some(cave(CaveTile::Seabed));
    }
    if y > hull.max.y {
        return Some(cave(CaveTile::Seabed));
    }
    Some(if y < stamp.bounds.min.y + AIR_ROWS + 1 {
        TileKind::Air
    } else {
        WATER
    })
}

/// An ice gallery: pack-ice walls, a slick brine-ice floor, icicles under the roof and now and
/// then a builder figure locked in the wall.
fn ice_gallery(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (_, cy, _, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 5 / 10;
    if inside >= 820 {
        if inside < 880 && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(61) && y < floor {
            return Some(cave(CaveTile::FrozenFigure));
        }
        return Some(cave(CaveTile::PackIce));
    }
    if y > floor {
        return Some(cave(CaveTile::BrineIce));
    }
    // Icicles hang from the roof.
    let roof = cy - ry * 7 / 10;
    if y <= roof + 1 && hash2(stamp.seed, SALT_DECOR, x, 1).is_multiple_of(4) {
        return Some(cave(CaveTile::Stalactite));
    }
    Some(TileKind::Air)
}

/// A vent grotto: a small warm cave with thermal vents in its floor (a safe spot).
fn vent_grotto(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (cx, cy, _, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 5 / 10;
    if y > floor {
        return Some(if y == floor + 1 && (x - cx).abs() <= 1 {
            cave(CaveTile::ThermalVent)
        } else {
            cave(CaveTile::Sinter)
        });
    }
    if inside >= 860 {
        return Some(cave(CaveTile::Sinter));
    }
    Some(TileKind::Air)
}

/// The frozen figures' gallery round the cable: a long hall of clear ice whose back wall holds
/// the builders' figures, all facing the cable.
fn figures_gallery(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let bounds = stamp.bounds;
    let column = x - bounds.min.x;
    let row = y - bounds.min.y;
    let width = bounds.width();
    let height = bounds.height();
    let end = column.min(width - 1 - column);
    let edge_row = row.min(height - 1 - row);
    if end + edge_row < 2 {
        return None;
    }
    if row >= height - 3 {
        return Some(cave(CaveTile::BrineIce));
    }
    if end < 2 || row < 2 {
        return Some(cave(CaveTile::PackIce));
    }
    // The figures: stood in alcoves along the floor, a column apart every six, the middle left
    // for the cable.
    let middle = (column - width / 2).abs();
    if middle > 3 && column % 6 == 3 && row >= height - 7 && row < height - 3 {
        return Some(cave(CaveTile::FrozenFigure));
    }
    Some(TileKind::Air)
}

/// A geyser chimney (and Old Faithful): a throat chamber at the bottom with the vents in its
/// floor, a shaft straight up, a ledge chamber at the top.
fn chimney(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let (vent_row, first, last) = stamp.vents()?;
    let bounds = stamp.bounds;
    let great = stamp.kind == DrownedStampKind::OldFaithful;
    let cx = bounds.min.x + bounds.width() / 2;
    let chamber_rows = if great { OLD_FAITHFUL_CHAMBER.1 } else { 5 };
    let half_shaft = if great {
        OLD_FAITHFUL_SHAFT_WIDTH / 2
    } else {
        1
    };
    if y == vent_row {
        return Some(if (first..=last).contains(&x) {
            cave(CaveTile::GeyserVent)
        } else {
            cave(CaveTile::Sinter)
        });
    }
    let chamber_top = vent_row - chamber_rows;
    let dx = (x - cx).abs();
    let chamber_half = bounds.width() / 2 - 1;
    if y >= chamber_top {
        if dx > chamber_half {
            return Some(cave(CaveTile::Sinter));
        }
        return Some(TileKind::Air);
    }
    // The ledge chamber at the top (a step off the jet).
    let ledge_rows = if great { 5 } else { 3 };
    if y <= bounds.min.y + ledge_rows {
        if y == bounds.min.y || dx > chamber_half {
            return Some(cave(CaveTile::Sinter));
        }
        return Some(TileKind::Air);
    }
    // The shaft, sinter-crusted.
    if dx <= half_shaft {
        return Some(TileKind::Air);
    }
    if dx == half_shaft + 1 {
        return Some(cave(CaveTile::Sinter));
    }
    None
}

/// A reef terrace: a coral-lined pool on Merrow City's flanks.
fn reef_terrace(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    if inside >= 1_000 {
        return None;
    }
    let (_, cy, _, _) = ellipse(&stamp.bounds);
    if inside >= 800 {
        return Some(cave(CaveTile::Coral));
    }
    Some(if y < cy { TileKind::Air } else { WATER })
}

/// A pearl bed: a flooded cavern of pillow basalt floored with giant clams.
fn pearl_bed(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let inside = ellipse_permille(&stamp.bounds, x, y);
    let wobble =
        i64::from(value_noise(stamp.seed, SALT_DECOR, x, y, 5, 4)) * 140 / i64::from(NOISE_MAX);
    if inside + wobble >= 1_000 {
        return None;
    }
    let (_, cy, _, ry) = ellipse(&stamp.bounds);
    let floor = cy + ry * 5 / 10;
    if inside + wobble >= 860 {
        return Some(cave(CaveTile::PillowBasalt));
    }
    if y > floor {
        return Some(cave(CaveTile::PillowBasalt));
    }
    if y == floor && hash2(stamp.seed, SALT_DECOR, x, y).is_multiple_of(3) {
        return Some(cave(CaveTile::PearlClam));
    }
    Some(if y < stamp.bounds.min.y + AIR_ROWS + 1 {
        TileKind::Air
    } else {
        WATER
    })
}

/// The Leviathan's arena: a vast flooded hall of pillow basalt, a breath of air under its roof,
/// broken basalt pillars on its floor.
fn arena(stamp: &DrownedStamp, x: i32, y: i32) -> Option<TileKind> {
    let bounds = stamp.bounds;
    let floor = arena_floor_row(&bounds);
    let inside = ellipse_permille(&bounds, x, y);
    // A flattened bowl: the ellipse, but the floor is flat.
    let rim = if y >= floor { 0 } else { inside };
    if y > bounds.max.y || inside >= 1_000 && y < floor {
        return None;
    }
    if y >= floor {
        return Some(cave(CaveTile::PillowBasalt));
    }
    if rim >= 900 {
        return Some(cave(CaveTile::PillowBasalt));
    }
    // Two broken pillars stand on the floor, a third of the way in from each side.
    let (cx, _, rx, _) = ellipse(&bounds);
    let pillar = (x - (cx - rx / 2)).abs() <= 1 || (x - (cx + rx / 2)).abs() <= 1;
    let height = 4 + i32::try_from(stamp.seed % 4).unwrap_or(0);
    if pillar && y >= floor - height {
        return Some(cave(CaveTile::PillowBasalt));
    }
    Some(if y < bounds.min.y + AIR_ROWS + 2 {
        TileKind::Air
    } else {
        WATER
    })
}

/// Merrow City's tile at `(x, y)` over `base` (the bubble, its glass and the sea round it,
/// the floor, the harbour and its moon pools, and the glass airlock tube), or `None`.
#[must_use]
pub fn city_tile(site: &MerrowSite, x: i32, y: i32, base: TileKind) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !site.bounds().contains(at) {
        return None;
    }
    // The cable's side: nothing past the far end (the rail stands there).
    let past_far = if site.side < 0 {
        x > site.dome.max.x
    } else {
        x < site.dome.min.x
    };
    if past_far {
        return None;
    }
    // Keep what the outpost and the settlement built.
    if matches!(
        base,
        TileKind::Built(_) | TileKind::Foundation | TileKind::Bedrock
    ) {
        return None;
    }
    let floor = site.floor;
    let pools = site.moon_pools();
    let in_pool = pools.iter().any(|left| x >= *left && x < left + 3);
    if y >= floor {
        if y <= floor + 1 {
            if in_pool && !site.outpost.contains(at) {
                return Some(TileKind::Air);
            }
            return Some(cave(CaveTile::MerrowStone));
        }
        let harbour = site.harbour();
        if harbour.contains(at) {
            let edge = x == harbour.min.x || x == harbour.max.x || y == harbour.max.y;
            return Some(if edge { cave(CaveTile::Coral) } else { WATER });
        }
        return None;
    }
    let inside = dome_permille(site, x, y);
    if inside < 1_000 {
        // The city's air (village lots and the outpost keep their tiles above).
        return Some(TileKind::Air);
    }
    // The airlock tube: a glass tube on the hall's side through the sea at the floor.
    let near_side = if site.side < 0 {
        x < site.dome.min.x + 8
    } else {
        x > site.dome.max.x - 8
    };
    if near_side && y >= floor - 5 {
        return Some(if y == floor - 5 {
            TileKind::Built(crate::base::BuiltTile::Glass)
        } else {
            TileKind::Air
        });
    }
    // The glass: a band just past the dome's rim.
    let outer = dome_permille_scaled(site, x, y, 1);
    if outer < 1_000 {
        return Some(TileKind::Built(crate::base::BuiltTile::Glass));
    }
    let sea = dome_permille_scaled(site, x, y, DOME_SEA);
    if sea < 1_000 {
        return Some(WATER);
    }
    let rim = dome_permille_scaled(site, x, y, DOME_SEA + 2);
    (rim < 1_000).then_some(cave(CaveTile::Coral))
}

/// [`dome_permille`] of a dome grown by `grow` tiles all round.
fn dome_permille_scaled(site: &MerrowSite, x: i32, y: i32, grow: i32) -> i64 {
    let dome = site.dome;
    let far_x = if site.side < 0 {
        dome.max.x + 5 + grow
    } else {
        dome.min.x - 5 - grow
    };
    let near_x = if site.side < 0 {
        dome.min.x - grow
    } else {
        dome.max.x + grow
    };
    let cx = i32::midpoint(far_x, near_x);
    let rx = i64::from((far_x - near_x).abs() / 2).max(1);
    let ry = i64::from(DOME_ROWS + grow);
    let dx = i64::from(x - cx) * 1_000 / rx;
    let dy = i64::from(site.floor - y) * 1_000 / ry;
    (dx * dx + dy * dy) / 1_000
}

/// Drowned rock: coral seams in the ocean and round the city (rarer than rock), or `None`.
#[must_use]
pub fn drowned_rock(
    settings: &WorldSettings,
    stratum: Stratum,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !matches!(
        base,
        TileKind::Stone
            | TileKind::HardRock
            | TileKind::Cave(CaveTile::Seabed | CaveTile::MerrowStone)
    ) {
        return None;
    }
    match stratum {
        Stratum::UndergroundOcean | Stratum::MerrowCity => {
            let mass = value_noise(settings.seed, SALT_CORAL, x, y, 10, 7);
            (mass > NOISE_MAX - 3_200).then_some(cave(CaveTile::Coral))
        }
        _ => None,
    }
}

/// The Drowned and Frozen's tile at `(x, y)` over `base`, or `None` (leave `base`).
///
/// The city's, a stamp's, else drowned rock. `stamps` are the chunk's [`stamps_with`], `city`
/// the world's [`merrow_city`].
#[must_use]
pub fn drowned_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    city: Option<&MerrowSite>,
    stamps: &[DrownedStamp],
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if let Some(site) = city
        && let Some(kind) = city_tile(site, x, y, base)
    {
        return Some(kind);
    }
    // A channel's water runs through the basins' rims and floors it joins (Wave 10 polish).
    let at = TilePosition { x, y };
    if carvable(base)
        && stamps.iter().any(|stamp| {
            stamp.kind == DrownedStampKind::Channel
                && stamp.bounds.contains(at)
                && channel_water(stamp, x, y)
        })
    {
        return Some(WATER);
    }
    for stamp in stamps {
        if let Some(kind) = stamp_tile(stamp, x, y, base) {
            return Some(kind);
        }
    }
    let stratum = super::stratum_in(settings, layout, x, y);
    if !crate::drowned::is_drowned(stratum) {
        return None;
    }
    drowned_rock(settings, stratum, x, y, base)
}

/// The basins (gyres) touching `area` (the tick's currents).
#[must_use]
pub fn basins_in(settings: &WorldSettings, layout: &DescentLayout, area: &TileRect) -> Vec<Basin> {
    stamps_with(settings, layout, area)
        .iter()
        .filter_map(DrownedStamp::basin)
        .collect()
}

#[cfg(test)]
mod tests;
