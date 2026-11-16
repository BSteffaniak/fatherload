//! Sky towns (expansion plan WP-SKY, T20): one per archipelago block of a V9 sky.
//!
//! The cloud archipelago's cells are grouped into **blocks** of [`TOWN_BLOCK`] cells (three
//! columns by two rows: 960 columns by 352 rows). One cell of every block, picked by a hash of
//! the block, is the **town cell**: instead of V8's chain it holds a wide, flat-topped
//! [`SkyLandKind::TownIsle`] with one or two small isles beside it, and a sky town stands on
//! the town isle. So above any stretch of ground a sky town hangs every few hundred rows up the
//! archipelago, and one every 960 columns along it.
//!
//! The town itself is a village ([`crate::village`]) with
//! [`VillageSetting::Sky`](crate::village::VillageSetting::Sky): the village grammar lays out
//! its street on the town isle's turf in a sky culture, with residents, counters and routines
//! like any village. Its id is [`placement::sky_town_id`] of the block, so it is found from an
//! id alone. This module only answers where the towns are ([`SkyTownSite`]).

use crate::{
    people::VillageId,
    sky::SkyTownKind,
    terrain::TilePosition,
    village::placement::{self, VillageSize},
    worldgen::{
        FeatureId, TileRect, WorldSettings,
        noise::{RegionRng, hash2},
        streamed::{STREAMED_COLUMN_LIMIT, index::feature_id},
    },
};

use super::{
    ARCHIPELAGO_CELL, ID_ARCHIPELAGO, Layer, SkyLand, SkyLandKind, band_cell_rows, biome_for,
    cell_altitudes, cell_lands, row_of,
};

/// Archipelago cells per town block: `(columns, rows)`.
pub const TOWN_BLOCK: (i32, i32) = (3, 2);
/// Columns of one town block.
pub const TOWN_BLOCK_COLUMNS: i32 = TOWN_BLOCK.0 * ARCHIPELAGO_CELL.0;
/// Rows above a sky town's ground that still count as being in it (as a surface village).
pub const SKY_TOWN_AIR_ROWS: i32 = 24;

const SALT_TOWN_CELL: u64 = 0x5C1E_0201;
const SALT_TOWN: u64 = 0x5C1E_0202;
const SALT_TOWN_KIND: u64 = 0x5C1E_0203;

/// Where a sky town stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkyTownSite {
    pub id: VillageId,
    /// The archipelago block `(bx, by)` it belongs to.
    pub block: (i32, i32),
    pub kind: SkyTownKind,
    pub size: VillageSize,
    /// First and last column of the town's street (inclusive).
    pub first: i32,
    pub last: i32,
    /// The town isle's turf row: the buildings stand on it.
    pub ground_row: i32,
    /// The town isle's feature id.
    pub isle: FeatureId,
}

impl SkyTownSite {
    /// The centre column.
    #[must_use]
    pub const fn center(&self) -> i32 {
        i32::midpoint(self.first, self.last)
    }

    /// The rectangle a rig counts as "in the town" in (like a surface village's area).
    #[must_use]
    pub const fn area(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.first,
                y: self.ground_row - SKY_TOWN_AIR_ROWS,
            },
            max: TilePosition {
                x: self.last,
                y: self.ground_row + 1,
            },
        }
    }
}

/// Town block rows (the archipelago's cell rows, two to a block).
const fn block_rows() -> i32 {
    let rows = band_cell_rows(ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1);
    (rows + TOWN_BLOCK.1 - 1) / TOWN_BLOCK.1
}

/// The town cell `(rx, ry)` of block `(bx, by)`.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the rolls are reduced below the block size"
)]
pub const fn town_cell_of_block(seed: u64, bx: i32, by: i32) -> (i32, i32) {
    let roll = hash2(seed, SALT_TOWN_CELL, bx, by);
    let rows = band_cell_rows(ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1);
    let mut ry = by * TOWN_BLOCK.1 + ((roll >> 8) & 1) as i32;
    if ry >= rows {
        ry = rows - 1;
    }
    (bx * TOWN_BLOCK.0 + (roll % 3) as i32, ry)
}

/// Whether archipelago cell `(rx, ry)` is its block's town cell (and inside the streamed
/// world).
#[must_use]
pub const fn is_town_cell(seed: u64, rx: i32, ry: i32) -> bool {
    let x0 = rx * ARCHIPELAGO_CELL.0;
    if ry < 0
        || ry >= band_cell_rows(ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1)
        || x0.abs() > STREAMED_COLUMN_LIMIT - ARCHIPELAGO_CELL.0
    {
        return false;
    }
    let (bx, by) = (rx.div_euclid(TOWN_BLOCK.0), ry.div_euclid(TOWN_BLOCK.1));
    let (tx, ty) = town_cell_of_block(seed, bx, by);
    tx == rx && ty == ry
}

/// The kind and size of the town of town cell `(rx, ry)`.
fn town_roll(seed: u64, rx: i32, ry: i32) -> (SkyTownKind, VillageSize) {
    let mut rng = RegionRng::new(seed, SALT_TOWN_KIND, rx, ry);
    let upper = ry * 2 >= band_cell_rows(ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1);
    // Low in the archipelago the farmers and markets; higher up the monks and the docks.
    let weights: [u32; 4] = if upper { [3, 2, 3, 1] } else { [2, 3, 1, 3] };
    let kind = SkyTownKind::ALL[rng.pick(&weights).unwrap_or(0)];
    // Village or town: a hamlet's street is too short for a sky town's big specialty building
    // and its landing pad.
    let size = if rng.chance(300) {
        VillageSize::Town
    } else {
        VillageSize::Village
    };
    (kind, size)
}

/// The town cell `(rx, ry)`'s islands: a wide flat town isle with one or two small isles
/// beside it (local ids 0.., west to east).
pub(super) fn town_cell(seed: u64, rx: i32, ry: i32) -> Vec<SkyLand> {
    let mut rng = RegionRng::new(seed, SALT_TOWN, rx, ry);
    let (_, size) = town_roll(seed, rx, ry);
    let (alt_bottom, alt_top) = cell_altitudes(ID_ARCHIPELAGO, ry, ARCHIPELAGO_CELL.1);
    let x0 = rx * ARCHIPELAGO_CELL.0;
    let town_half = size.width_tiles() / 2 + 6;
    let west_half = rng.range(8, 16);
    let east_half = if rng.chance(600) { rng.range(8, 16) } else { 0 };
    let (gap_west, gap_east) = (rng.range(12, 22), rng.range(12, 22));
    let total = 2 * west_half
        + gap_west
        + 2 * town_half
        + if east_half > 0 {
            gap_east + 2 * east_half
        } else {
            0
        };
    let mut cursor = x0 + rng.range(6, (ARCHIPELAGO_CELL.0 - 8 - total).max(6));
    let town_alt = rng.range(alt_bottom + 70, alt_top - 34);
    let mut lands = Vec::new();
    let mut local = 0_u16;
    let mut isle = |half: i32, kind: SkyLandKind, alt: i32, rng: &mut RegionRng, cursor: i32| {
        let depth = if kind == SkyLandKind::TownIsle {
            rng.range(16, 22)
        } else {
            rng.range(5, 9) + half / 3
        };
        let top_alt = alt.clamp(alt_bottom + depth * 2 + 12, alt_top - 8);
        let land = SkyLand {
            id: feature_id(Layer::Sky, rx, ID_ARCHIPELAGO * 1000 + ry, local),
            kind,
            biome: biome_for(top_alt, rng.next_u64()),
            center_x: cursor + half,
            top_row: row_of(top_alt),
            half_width: half,
            depth,
            shape: rng.next_u64(),
            content: None,
            plan: None,
            lunar: false,
        };
        local += 1;
        land
    };
    let west_alt = town_alt + rng.range(-24, 24);
    lands.push(isle(
        west_half,
        SkyLandKind::ArchipelagoIsle,
        west_alt,
        &mut rng,
        cursor,
    ));
    cursor += 2 * west_half + 1 + gap_west;
    lands.push(isle(
        town_half,
        SkyLandKind::TownIsle,
        town_alt,
        &mut rng,
        cursor,
    ));
    cursor += 2 * town_half + 1 + gap_east;
    if east_half > 0 && cursor + 2 * east_half < x0 + ARCHIPELAGO_CELL.0 - 4 {
        let east_alt = town_alt + rng.range(-24, 24);
        lands.push(isle(
            east_half,
            SkyLandKind::ArchipelagoIsle,
            east_alt,
            &mut rng,
            cursor,
        ));
    }
    lands
}

/// The sky town of archipelago block `(bx, by)` (`None` on generators without sky towns, or
/// blocks outside the archipelago or the streamed world).
#[must_use]
pub fn sky_town_site(settings: &WorldSettings, bx: i32, by: i32) -> Option<SkyTownSite> {
    if !settings.generator.has_sky_isles() || by < 0 || by >= block_rows() {
        return None;
    }
    let (rx, ry) = town_cell_of_block(settings.seed, bx, by);
    if !is_town_cell(settings.seed, rx, ry) {
        return None;
    }
    let lands = cell_lands(settings, ID_ARCHIPELAGO, rx, ry);
    let isle = lands
        .iter()
        .find(|land| land.kind == SkyLandKind::TownIsle)?;
    let (kind, size) = town_roll(settings.seed, rx, ry);
    let first = isle.center_x - size.width_tiles() / 2;
    Some(SkyTownSite {
        id: placement::sky_town_id(bx, by),
        block: (bx, by),
        kind,
        size,
        first,
        last: first + size.width_tiles() - 1,
        ground_row: isle.top_row,
        isle: isle.id,
    })
}

/// The sky town with village id `id`, if `id` names one this world has.
#[must_use]
pub fn sky_town_of(settings: &WorldSettings, id: VillageId) -> Option<SkyTownSite> {
    let (bx, by) = placement::decode_sky_town(id)?;
    sky_town_site(settings, bx, by)
}

/// The town blocks whose towns could reach `rect` (block rows from its altitudes).
fn blocks_touching(rect: &TileRect) -> (std::ops::RangeInclusive<i32>, std::ops::Range<i32>) {
    let columns = (rect.min.x - 8).div_euclid(TOWN_BLOCK_COLUMNS)
        ..=(rect.max.x + 8).div_euclid(TOWN_BLOCK_COLUMNS);
    let cell_rows = super::band_cells(ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1, &rect.expanded(32));
    let rows = cell_rows.start.div_euclid(TOWN_BLOCK.1)
        ..(cell_rows.end + TOWN_BLOCK.1 - 1).div_euclid(TOWN_BLOCK.1);
    (columns, rows)
}

/// Every sky town whose area overlaps `rect`, west to east then bottom to top (empty on
/// generators without sky towns).
#[must_use]
pub fn sky_towns_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<SkyTownSite> {
    if !settings.generator.has_sky_isles() || rect.min.y > -super::ARCHIPELAGO_BOTTOM_ROWS {
        return Vec::new();
    }
    let (columns, rows) = blocks_touching(rect);
    let mut found = Vec::new();
    for bx in columns {
        for by in rows.clone() {
            if let Some(site) = sky_town_site(settings, bx, by)
                && site.area().intersects(rect)
            {
                found.push(site);
            }
        }
    }
    found
}

/// The sky town whose area contains `position`, if any.
#[must_use]
pub fn sky_town_at(settings: &WorldSettings, position: TilePosition) -> Option<SkyTownSite> {
    let rect = TileRect {
        min: position,
        max: position,
    };
    sky_towns_touching(settings, &rect).into_iter().next()
}

/// The sky towns whose centre column lies in `first..=last`, at any altitude.
#[must_use]
pub fn sky_towns_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<SkyTownSite> {
    if !settings.generator.has_sky_isles() || first > last {
        return Vec::new();
    }
    let mut found = Vec::new();
    let columns = (first - TOWN_BLOCK_COLUMNS).div_euclid(TOWN_BLOCK_COLUMNS)
        ..=(last + TOWN_BLOCK_COLUMNS).div_euclid(TOWN_BLOCK_COLUMNS);
    for bx in columns {
        for by in 0..block_rows() {
            if let Some(site) = sky_town_site(settings, bx, by)
                && (first..=last).contains(&site.center())
            {
                found.push(site);
            }
        }
    }
    found
}
