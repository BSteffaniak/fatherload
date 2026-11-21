//! Tomb and crypt sites of [`GeneratorVersion::V10`] worlds, and the run vault (WP-TOMBS).
//!
//! **Sites.** Sealed entrances placed per region, each a pure function of the seed and its
//! cell ([`sites_touching`]):
//!
//! * [`SiteSetting::Deep`]: one cell in three of a [`DEEP_REGION`] grid below
//!   [`DEEP_MIN_ROW`], in a pocket carved out of the strata; the tier follows the depth.
//! * [`SiteSetting::Mesa`]: one per [`MESA_CELL`] columns where the biome is badlands or desert
//!   and the ground is level enough, standing on the surface (tier 1: Act I).
//! * [`SiteSetting::OldQuarter`]: under about half of the surface villages (tier 2).
//! * [`SiteSetting::SkyContinent`]: on most sky continents' turf, clear of the city, temple,
//!   lake and nests (tier 4).
//! * [`SiteSetting::Moon`]: two on the Moon's regolith, clear of the colony, habitats and the
//!   facility (tier 5).
//!
//! Each site's [`SiteStyle`] is a coin flip of its own roll (about half of each tier each kind)
//! and its entrance is drawn from that style's layout ([`entrance_cells`]): a stepped sandstone
//! pyramid with a sealed door in an alcove for a side-view tomb, a crypt-stone mausoleum on
//! carved pillars over a sealed floor hatch for a top-down crypt. The [`SiteOpening`] is rolled
//! too (a key half the time, explosives or a story step otherwise). Entrances are stamped last
//! (after pilot spaces); cells marked `.` carve air so a site underground sits in its own
//! pocket a rig can dig to.
//!
//! **The vault.** Runs are realized in instanced *run slots* inside [`VAULT`], a strip of
//! sealed bedrock generated deep under far-west columns of V10 worlds (columns
//! [`VAULT_FIRST_COLUMN`]..; close enough to the origin that `f32` positions keep 1/32 px).
//! Nothing digs, blasts or picks bedrock, so a slot is reached only by entering a site; a run
//! writes its floor into its slot ([`slot_origin`]) and puts the bedrock back when it ends.
//! [`in_vault`] tells the rest of the simulation where run spaces are (no cave water, critters,
//! heat or cave-ins there).
//!
//! [`GeneratorVersion::V10`]: crate::worldgen::GeneratorVersion::V10

use crate::{
    base::BuiltTile,
    terrain::{TileKind, TilePosition},
    tomb::{SiteId, SiteOpening, SiteSetting, SiteStyle, TombSite},
    worldgen::{
        WorldSettings,
        biome::{Biome, biome_at, flat_sites_in},
        features::TileRect,
        noise::{RegionRng, hash2},
    },
};

use super::{TOWN_FLAT_FIRST_COLUMN, TOWN_FLAT_LAST_COLUMN, cache::MemoCache};

static DEEP: MemoCache<Option<TombSite>> = MemoCache::new(512);
static MESA: MemoCache<Option<TombSite>> = MemoCache::new(256);
static SKY: MemoCache<Vec<TombSite>> = MemoCache::new(64);

const SALT_SITE: u64 = 0x70B5_517E_0004;
const SALT_MESA: u64 = 0x70B5_517E_0005;
const SALT_QUARTER: u64 = 0x70B5_517E_0006;
const SALT_CONTINENT: u64 = 0x70B5_517E_0007;
const SALT_MOON: u64 = 0x70B5_517E_0008;
const SALT_SEED: u64 = 0x70B5_517E_0009;

/// Size of a deep placement cell (columns, rows).
pub const DEEP_REGION: (i32, i32) = (420, 150);
/// Deep sites start at this row.
pub const DEEP_MIN_ROW: i32 = 140;
/// Deep sites stay this many rows above the world's floor.
pub const FLOOR_MARGIN_ROWS: i32 = 30;
/// Per-mille of deep cells holding a site.
pub const DEEP_PER_MILLE: u32 = 330;
/// Columns of a mesa placement cell.
pub const MESA_CELL: i32 = 640;
/// Per-mille of village old quarters with a site.
pub const QUARTER_PER_MILLE: u32 = 550;
/// Rows under a village's ground its old quarter's site sits.
pub const QUARTER_DEPTH_ROWS: i32 = 22;
/// Columns either side of the town kept free of sites.
pub const TOWN_MARGIN_COLUMNS: i32 = 60;
/// Most a surface site's ground may vary across its footprint (rows).
pub const MAX_GROUND_STEP: i32 = 3;

/// Size of one run slot (columns, rows).
pub const SLOT_SIZE: (i32, i32) = (168, 48);
/// Slots per vault row.
pub const SLOT_COLUMNS: i32 = 8;
/// Run slots in the vault (live runs at once).
pub const SLOTS: u8 = 16;
/// First column of the vault.
pub const VAULT_FIRST_COLUMN: i32 = -15_000;
/// First row of the vault.
pub const VAULT_FIRST_ROW: i32 = 180;
/// The vault strip (inclusive).
pub const VAULT: TileRect = TileRect {
    min: TilePosition {
        x: VAULT_FIRST_COLUMN,
        y: VAULT_FIRST_ROW,
    },
    max: TilePosition {
        x: VAULT_FIRST_COLUMN + SLOT_SIZE.0 * SLOT_COLUMNS - 1,
        y: VAULT_FIRST_ROW + SLOT_SIZE.1 * (SLOTS as i32 / SLOT_COLUMNS) - 1,
    },
};
/// Tiles kept between the vault and any site.
pub const VAULT_MARGIN: i32 = 40;

/// The top-left tile of run slot `slot`.
#[must_use]
pub const fn slot_origin(slot: u8) -> TilePosition {
    let slot = slot as i32 % SLOTS as i32;
    TilePosition {
        x: VAULT_FIRST_COLUMN + (slot % SLOT_COLUMNS) * SLOT_SIZE.0,
        y: VAULT_FIRST_ROW + (slot / SLOT_COLUMNS) * SLOT_SIZE.1,
    }
}

/// Whether `tile` is inside the run vault of a world with tombs.
#[must_use]
pub fn in_vault(settings: &WorldSettings, tile: TilePosition) -> bool {
    settings.generator.has_tombs() && VAULT.contains(tile)
}

/// The run slot holding `tile`, if it is in the vault.
#[must_use]
pub fn slot_at(settings: &WorldSettings, tile: TilePosition) -> Option<u8> {
    if !in_vault(settings, tile) {
        return None;
    }
    let column = (tile.x - VAULT_FIRST_COLUMN) / SLOT_SIZE.0;
    let row = (tile.y - VAULT_FIRST_ROW) / SLOT_SIZE.1;
    u8::try_from(row * SLOT_COLUMNS + column).ok()
}

/// A side-view tomb's entrance (legend: `.` carved air, `W` tomb masonry, `D` the sealed
/// door). The last row is the floor, on the ground row.
const SIDE_VIEW_ENTRANCE: [&str; 9] = [
    "........WW......",
    ".......WWWW.....",
    "......WWWWWW....",
    ".....WWWWWWWW...",
    "....WWWWWWWWWW..",
    "...WWWWWWWWWWWW.",
    ".........DWWWWWW",
    ".........DWWWWWW",
    "WWWWWWWWWWWWWWWW",
];
/// A top-down crypt's entrance: a mausoleum roof on carved pillars (`P`, pilots walk through)
/// over a sealed floor hatch (`D`). Row 6 is the floor, on the ground row.
const TOP_DOWN_ENTRANCE: [&str; 8] = [
    "...WWWWWWWW...",
    "..WWWWWWWWWW..",
    ".WWWWWWWWWWWW.",
    "..P........P..",
    "..P........P..",
    "..P........P..",
    "WWWWWWDWWWWWWW",
    "WWWWWWWWWWWWWW",
];

/// The entrance layout of `style` (unmirrored).
#[must_use]
pub const fn entrance_layout(style: SiteStyle) -> &'static [&'static str] {
    match style {
        SiteStyle::SideView => &SIDE_VIEW_ENTRANCE,
        SiteStyle::TopDown => &TOP_DOWN_ENTRANCE,
    }
}

/// The layout row of `style`'s floor (it lies on the ground row).
#[must_use]
pub const fn entrance_floor_row(style: SiteStyle) -> i32 {
    match style {
        SiteStyle::SideView => 8,
        SiteStyle::TopDown => 6,
    }
}

/// Width and height of `style`'s entrance.
#[must_use]
pub fn entrance_size(style: SiteStyle) -> (i32, i32) {
    let layout = entrance_layout(style);
    let width = layout.iter().map(|row| row.len()).max().unwrap_or(0);
    (
        i32::try_from(width).unwrap_or(0),
        i32::try_from(layout.len()).unwrap_or(0),
    )
}

/// The tiles a site's entrance box covers.
#[must_use]
pub fn entrance_bounds(site: &TombSite) -> TileRect {
    let (width, height) = entrance_size(site.style);
    TileRect {
        min: site.origin,
        max: TilePosition {
            x: site.origin.x + width - 1,
            y: site.origin.y + height - 1,
        },
    }
}

/// Every tile a site's entrance writes, in row order (the door sealed).
#[must_use]
pub fn entrance_cells(site: &TombSite) -> Vec<(TilePosition, TileKind)> {
    let style = site.style.index();
    let (width, _) = entrance_size(site.style);
    let mut cells = Vec::new();
    for (row, line) in (0..).zip(entrance_layout(site.style)) {
        for (column, cell) in (0..).zip(line.bytes()) {
            let x = if site.mirrored {
                site.origin.x + width - 1 - column
            } else {
                site.origin.x + column
            };
            let kind = match cell {
                b'.' => TileKind::Air,
                b'W' => TileKind::Built(BuiltTile::TombWall(style)),
                b'P' => TileKind::Built(BuiltTile::TombPillar(style)),
                b'D' => TileKind::Built(BuiltTile::TombDoor {
                    style,
                    sealed: true,
                }),
                _ => continue,
            };
            cells.push((
                TilePosition {
                    x,
                    y: site.origin.y + row,
                },
                kind,
            ));
        }
    }
    cells
}

/// The door tile of an entrance at `origin` (the first `D` of the layout).
fn door_of(style: SiteStyle, origin: TilePosition, mirrored: bool) -> TilePosition {
    let (width, _) = entrance_size(style);
    for (row, line) in (0..).zip(entrance_layout(style)) {
        if let Some(column) = line.bytes().position(|cell| cell == b'D') {
            let column = i32::try_from(column).unwrap_or(0);
            return TilePosition {
                x: if mirrored {
                    origin.x + width - 1 - column
                } else {
                    origin.x + column
                },
                y: origin.y + row,
            };
        }
    }
    origin
}

/// A site's style, opening, mirroring and seed from its roll.
fn finish_site(
    settings: &WorldSettings,
    setting: SiteSetting,
    cell: (i32, i32),
    roll: u64,
    tier: u8,
    style: SiteStyle,
    origin: TilePosition,
) -> TombSite {
    let mirrored = (roll >> 41) & 1 == 1;
    let opening = match (roll >> 44) % 10 {
        0..=4 => SiteOpening::Key,
        5..=7 => SiteOpening::Explosives,
        _ => SiteOpening::Story,
    };
    let id = SiteId::new(setting, cell.0, cell.1);
    TombSite {
        id,
        setting,
        style,
        tier,
        opening,
        origin,
        door: door_of(style, origin, mirrored),
        mirrored,
        seed: hash2(settings.seed, SALT_SEED, cell.0, cell.1) ^ setting.index(),
    }
}

/// The style rolled from bit 40 of a site's roll.
const fn style_of(roll: u64) -> SiteStyle {
    if (roll >> 40) & 1 == 1 {
        SiteStyle::TopDown
    } else {
        SiteStyle::SideView
    }
}

/// Whether a site box would overlap the town, a village, a pilot space or the vault.
fn crowded(settings: &WorldSettings, bounds: &TileRect, village_margin: Option<i32>) -> bool {
    if bounds.max.x >= TOWN_FLAT_FIRST_COLUMN - TOWN_MARGIN_COLUMNS
        && bounds.min.x <= TOWN_FLAT_LAST_COLUMN + TOWN_MARGIN_COLUMNS
    {
        return true;
    }
    if bounds.intersects(&VAULT.expanded(VAULT_MARGIN)) {
        return true;
    }
    if let Some(margin) = village_margin {
        let corners = [
            bounds.min,
            bounds.max,
            TilePosition {
                x: bounds.min.x,
                y: bounds.max.y,
            },
            TilePosition {
                x: bounds.max.x,
                y: bounds.min.y,
            },
        ];
        if corners
            .iter()
            .any(|corner| super::in_village_area(settings, *corner, margin))
        {
            return true;
        }
    }
    !super::pilot_spaces::spaces_touching(settings, &bounds.expanded(3)).is_empty()
}

/// The deep site of cell `(cx, cy)`, if any.
fn deep_in_cell(settings: &WorldSettings, cx: i32, cy: i32) -> Option<TombSite> {
    let roll = hash2(settings.seed, SALT_SITE, cx, cy);
    if roll % 1000 >= u64::from(DEEP_PER_MILLE) {
        return None;
    }
    let style = style_of(roll);
    let (width, height) = entrance_size(style);
    let ctx = super::ctx(settings);
    let (left, top) = (cx * DEEP_REGION.0, cy * DEEP_REGION.1);
    let first_row = top.max(DEEP_MIN_ROW).max(ctx.deepest_surface + 30);
    let last_row = (top + DEEP_REGION.1 - 1).min(ctx.height - FLOOR_MARGIN_ROWS) - height;
    if last_row < first_row {
        return None;
    }
    let span_x = u64::try_from(DEEP_REGION.0 - width - 8).ok()?.max(1);
    let span_y = u64::try_from(last_row - first_row + 1).ok()?.max(1);
    let origin = TilePosition {
        x: left + 4 + i32::try_from((roll >> 8) % span_x).ok()?,
        y: first_row + i32::try_from((roll >> 24) % span_y).ok()?,
    };
    let floor = origin.y + entrance_floor_row(style);
    let tier = u8::try_from(1 + ((floor - DEEP_MIN_ROW) / 140).clamp(0, 4)).unwrap_or(1);
    let site = finish_site(
        settings,
        SiteSetting::Deep,
        (cx, cy),
        roll,
        tier,
        style,
        origin,
    );
    (!crowded(settings, &entrance_bounds(&site), Some(8))).then_some(site)
}

/// The lowest ground row across `first..=last` from `surface`, if the ground is level enough.
fn level_ground(first: i32, last: i32, surface: impl Fn(i32) -> Option<i32>) -> Option<i32> {
    let mut low = i32::MIN;
    let mut high = i32::MAX;
    for x in first..=last {
        let row = surface(x)?;
        low = low.max(row);
        high = high.min(row);
    }
    (low - high <= MAX_GROUND_STEP).then_some(low)
}

/// The mesa site of cell `cx`, if any.
fn mesa_in_cell(settings: &WorldSettings, cx: i32) -> Option<TombSite> {
    if !settings.generator.has_biomes() {
        return None;
    }
    let mut rng = RegionRng::new(settings.seed, SALT_MESA, cx, 0);
    let roll = rng.next_u64();
    if roll % 1000 >= 700 {
        return None;
    }
    let style = style_of(roll);
    let (width, _) = entrance_size(style);
    let floor_row = entrance_floor_row(style);
    for _ in 0..8 {
        let x = cx * MESA_CELL + rng.range(40, MESA_CELL - 40 - width);
        if !matches!(biome_at(settings, x), Biome::Badlands | Biome::Desert)
            || !matches!(
                biome_at(settings, x + width - 1),
                Biome::Badlands | Biome::Desert
            )
        {
            continue;
        }
        let Some(ground) = level_ground(x, x + width - 1, |column| {
            Some(super::surface_row(settings, column))
        }) else {
            continue;
        };
        let origin = TilePosition {
            x,
            y: ground - floor_row,
        };
        let site = finish_site(settings, SiteSetting::Mesa, (cx, 0), roll, 1, style, origin);
        if crowded(settings, &entrance_bounds(&site), Some(16)) {
            continue;
        }
        return Some(site);
    }
    None
}

/// The old-quarter sites under the villages whose flat sites lie in `first..=last`.
fn quarters_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<TombSite> {
    let mut found = Vec::new();
    for village in flat_sites_in(settings, first, last) {
        let roll = hash2(settings.seed, SALT_QUARTER, village.center, 0);
        if roll % 1000 >= u64::from(QUARTER_PER_MILLE) {
            continue;
        }
        let style = style_of(roll);
        let (width, _) = entrance_size(style);
        let offset = i32::try_from((roll >> 8) % 41).unwrap_or(0) - 20;
        let floor = village.row + QUARTER_DEPTH_ROWS;
        let origin = TilePosition {
            x: village.center + offset - width / 2,
            y: floor - entrance_floor_row(style),
        };
        let site = finish_site(
            settings,
            SiteSetting::OldQuarter,
            (village.center, 1),
            roll,
            2,
            style,
            origin,
        );
        if !crowded(settings, &entrance_bounds(&site), None) {
            found.push(site);
        }
    }
    found
}

/// The site on the sky continent of continent cell `rx`, if any.
fn continent_sites(settings: &WorldSettings, rx: i32) -> Vec<TombSite> {
    let Some(land) = crate::worldgen::sky::continent_in_cell(settings, rx) else {
        return Vec::new();
    };
    let mut rng = RegionRng::new(settings.seed, SALT_CONTINENT, rx, 0);
    let roll = rng.next_u64();
    if roll % 1000 >= 750 {
        return Vec::new();
    }
    let style = style_of(roll);
    let (width, _) = entrance_size(style);
    let plan = land.plan();
    let blocked = |first: i32, last: i32| {
        plan.is_some_and(|plan| {
            let near = |a: i32, b: i32| last >= a - 12 && first <= b + 12;
            plan.city.is_some_and(|city| near(city.first, city.last))
                || plan.temple.is_some_and(|temple| {
                    near(
                        temple.left,
                        temple.left + crate::worldgen::sky::continent::TEMPLE_WIDTH,
                    )
                })
                || plan.lake.is_some_and(|lake| {
                    near(lake.center - lake.half_width, lake.center + lake.half_width)
                })
                || plan
                    .nests
                    .iter()
                    .flatten()
                    .any(|nest| near(nest.x - 4, nest.x + 4))
        })
    };
    for attempt in 0..10 {
        let side = if attempt % 2 == 0 { 1 } else { -1 };
        let reach = land.half_width * (30 + rng.range(0, 45)) / 100;
        let x = land.center_x + side * reach - width / 2;
        if blocked(x, x + width - 1) {
            continue;
        }
        let Some(ground) = level_ground(x, x + width - 1, |column| land.surface_row(column)) else {
            continue;
        };
        let origin = TilePosition {
            x,
            y: ground - entrance_floor_row(style),
        };
        return vec![finish_site(
            settings,
            SiteSetting::SkyContinent,
            (rx, 2),
            roll,
            4,
            style,
            origin,
        )];
    }
    Vec::new()
}

/// The Moon's two sites.
fn moon_sites(settings: &WorldSettings) -> Vec<TombSite> {
    let Some(layout) = crate::worldgen::moon::layout(settings) else {
        return Vec::new();
    };
    let moon = layout.moon;
    let facility = layout.facility.bounds();
    let mut found = Vec::new();
    for index in 0..2 {
        let mut rng = RegionRng::new(settings.seed, SALT_MOON, index, 0);
        let roll = rng.next_u64();
        // One site of each style on the Moon, so both show up there.
        let style = if index == 0 {
            style_of(roll)
        } else {
            match found.first() {
                Some(TombSite {
                    style: SiteStyle::SideView,
                    ..
                }) => SiteStyle::TopDown,
                _ => SiteStyle::SideView,
            }
        };
        let (width, _) = entrance_size(style);
        let side = if index == 0 { -1 } else { 1 };
        for _ in 0..16 {
            let reach = moon.half_width * rng.range(20, 80) / 100;
            let x = moon.center_x + side * reach - width / 2;
            let (first, last) = (x - 8, x + width + 8);
            let clear = !layout.colony.covers(first)
                && !layout.colony.covers(last)
                && layout
                    .habitats
                    .iter()
                    .all(|habitat| !habitat.covers(first) && !habitat.covers(last))
                && (last < facility.min.x - 8 || first > facility.max.x + 8)
                && layout
                    .tubes
                    .iter()
                    .all(|tube| last < tube.first - 4 || first > tube.last + 4);
            if !clear {
                continue;
            }
            let Some(ground) = level_ground(x, x + width - 1, |column| layout.surface_row(column))
            else {
                continue;
            };
            let origin = TilePosition {
                x,
                y: ground - entrance_floor_row(style),
            };
            found.push(finish_site(
                settings,
                SiteSetting::Moon,
                (index, 3),
                roll,
                5,
                style,
                origin,
            ));
            break;
        }
    }
    found
}

/// Every site whose entrance box touches `rect` (none before V10), in a stable order.
#[must_use]
pub fn sites_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<TombSite> {
    if !settings.generator.has_tombs() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let grown = rect.expanded(24);
    let ctx = super::ctx(settings);
    // Deep cells.
    if grown.max.y >= DEEP_MIN_ROW && grown.min.y < ctx.height {
        for cy in
            grown.min.y.max(0).div_euclid(DEEP_REGION.1)..=grown.max.y.div_euclid(DEEP_REGION.1)
        {
            for cx in grown.min.x.div_euclid(DEEP_REGION.0)..=grown.max.x.div_euclid(DEEP_REGION.0)
            {
                let site =
                    DEEP.get_or_compute((*settings, cx, cy), || deep_in_cell(settings, cx, cy));
                found.extend(*site);
            }
        }
    }
    // Mesas and old quarters live around the surface and just under it.
    let surface_band = TileRect {
        min: TilePosition {
            x: grown.min.x,
            y: -crate::worldgen::biome::SKY_HEADROOM_ROWS,
        },
        max: TilePosition {
            x: grown.max.x,
            y: ctx.deepest_surface + QUARTER_DEPTH_ROWS + 16,
        },
    };
    if grown.intersects(&surface_band) {
        for cx in grown.min.x.div_euclid(MESA_CELL)..=grown.max.x.div_euclid(MESA_CELL) {
            let site = MESA.get_or_compute((*settings, cx, 0), || mesa_in_cell(settings, cx));
            found.extend(*site);
        }
        found.extend(quarters_between(
            settings,
            grown.min.x - 200,
            grown.max.x + 200,
        ));
    }
    // The sky: continents and the Moon (only rows far above the ground).
    if grown.min.y < -1_000 {
        let cells = crate::worldgen::sky::CONTINENT_CELL_COLUMNS;
        for rx in grown.min.x.div_euclid(cells)..=grown.max.x.div_euclid(cells) {
            let sites = SKY.get_or_compute((*settings, rx, 2), || continent_sites(settings, rx));
            found.extend(sites.iter().copied());
        }
        let sites = SKY.get_or_compute((*settings, 0, 3), || moon_sites(settings));
        found.extend(sites.iter().copied());
    }
    found.retain(|site| entrance_bounds(site).intersects(rect));
    found.sort_by_key(|site| site.id);
    found.dedup();
    found
}

/// The site whose door is `door`, if any.
#[must_use]
pub fn site_with_door(settings: &WorldSettings, door: TilePosition) -> Option<TombSite> {
    sites_touching(
        settings,
        &TileRect {
            min: door,
            max: door,
        },
    )
    .into_iter()
    .find(|site| {
        entrance_cells(site).iter().any(|(tile, kind)| {
            *tile == door && matches!(kind, TileKind::Built(BuiltTile::TombDoor { .. }))
        })
    })
}

/// The nearest site to `from` (by its door) within `reach` tiles, if any (quests, stories and
/// the map bind to it).
#[must_use]
pub fn nearest_site(settings: &WorldSettings, from: TilePosition, reach: i32) -> Option<TombSite> {
    let rect = TileRect {
        min: TilePosition {
            x: from.x - reach,
            y: from.y - reach,
        },
        max: TilePosition {
            x: from.x + reach,
            y: from.y + reach,
        },
    };
    sites_touching(settings, &rect)
        .into_iter()
        .min_by_key(|site| {
            let (dx, dy) = (
                i64::from(site.door.x - from.x),
                i64::from(site.door.y - from.y),
            );
            (dx * dx + dy * dy, site.id)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::GeneratorVersion;

    fn v10() -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V10,
            ..WorldSettings::default()
        }
    }

    fn wide() -> TileRect {
        TileRect {
            min: TilePosition { x: -6_000, y: -100 },
            max: TilePosition { x: 6_000, y: 900 },
        }
    }

    #[test]
    fn sites_are_placed_only_on_v10_in_every_ground_setting() {
        let sites = sites_touching(&v10(), &wide());
        for setting in [
            SiteSetting::Deep,
            SiteSetting::Mesa,
            SiteSetting::OldQuarter,
        ] {
            assert!(
                sites.iter().any(|site| site.setting == setting),
                "no {setting:?} site"
            );
        }
        for style in SiteStyle::ALL {
            let share = sites.iter().filter(|site| site.style == style).count();
            assert!(
                share * 4 >= sites.len(),
                "{style:?}: {share} of {}",
                sites.len()
            );
        }
        let v9 = WorldSettings {
            generator: GeneratorVersion::V9,
            ..WorldSettings::default()
        };
        assert!(sites_touching(&v9, &wide()).is_empty());
        for site in &sites {
            let bounds = entrance_bounds(site);
            assert!(!bounds.intersects(&VAULT));
            assert!(
                bounds.max.x < TOWN_FLAT_FIRST_COLUMN - TOWN_MARGIN_COLUMNS
                    || bounds.min.x > TOWN_FLAT_LAST_COLUMN + TOWN_MARGIN_COLUMNS
            );
            assert!((1..=crate::tomb::MAX_TIER).contains(&site.tier));
            assert_eq!(site_with_door(&v10(), site.door), Some(*site));
        }
    }

    #[test]
    fn the_sky_and_the_moon_get_sites_of_both_styles() {
        let settings = v10();
        let moon = crate::worldgen::sky::moon(&settings).expect("a moon");
        let rect = moon.bounds();
        let sites = sites_touching(&settings, &rect);
        let lunar: Vec<_> = sites
            .iter()
            .filter(|site| site.setting == SiteSetting::Moon)
            .collect();
        assert_eq!(lunar.len(), 2, "{lunar:?}");
        assert_ne!(lunar[0].style, lunar[1].style);
        assert!(lunar.iter().all(|site| site.tier == 5));
        let sky = TileRect {
            min: TilePosition {
                x: -20_000,
                y: -16_000,
            },
            max: TilePosition {
                x: 20_000,
                y: -9_000,
            },
        };
        assert!(
            sites_touching(&settings, &sky)
                .iter()
                .any(|site| site.setting == SiteSetting::SkyContinent && site.tier == 4)
        );
    }

    #[test]
    fn a_site_reads_the_same_from_every_chunk_it_touches_and_its_style_is_fixed() {
        let settings = v10();
        for site in sites_touching(&settings, &wide()).into_iter().take(12) {
            assert_eq!(
                sites_touching(&settings, &entrance_bounds(&site)),
                vec![site],
                "the site is a function of its cell alone"
            );
            for (tile, kind) in entrance_cells(&site) {
                let chunk = crate::terrain::ChunkPosition::of(tile);
                let generated = super::super::generated_chunk(&settings, chunk);
                assert_eq!(
                    generated
                        .tile(crate::terrain::ChunkPosition::local_index(tile))
                        .kind,
                    kind,
                    "{tile:?} of {site:?}"
                );
                if let TileKind::Built(
                    BuiltTile::TombDoor { style, .. } | BuiltTile::TombWall(style),
                ) = kind
                {
                    assert_eq!(
                        style,
                        site.style.index(),
                        "every entrance tile shows its style"
                    );
                }
            }
        }
    }

    #[test]
    fn the_vault_is_sealed_bedrock_on_v10_only() {
        let settings = v10();
        for slot in [0, SLOTS - 1] {
            let origin = slot_origin(slot);
            assert!(in_vault(&settings, origin));
            assert_eq!(slot_at(&settings, origin), Some(slot));
            for (dx, dy) in [(0, 0), (SLOT_SIZE.0 - 1, SLOT_SIZE.1 - 1), (40, 20)] {
                let tile = TilePosition {
                    x: origin.x + dx,
                    y: origin.y + dy,
                };
                assert_eq!(
                    super::super::generated_tile(&settings, tile).kind,
                    TileKind::Bedrock
                );
            }
        }
        let v9 = WorldSettings {
            generator: GeneratorVersion::V9,
            ..WorldSettings::default()
        };
        assert!(!in_vault(&v9, slot_origin(0)));
        // f32 positions keep 1/32 px across the whole vault.
        assert!(VAULT.min.x.abs() < 16_384 && VAULT.max.x.abs() < 16_384);
    }

    /// V10 generation is pinned: the chunks around the first sites of each setting near town,
    /// a vault chunk and a plain ground chunk. Change it only with a new generator version.
    #[test]
    fn v10_generation_is_pinned() {
        let settings = v10();
        let sites = sites_touching(&settings, &wide());
        let mut chunks: Vec<crate::terrain::ChunkPosition> = Vec::new();
        for setting in [
            SiteSetting::Deep,
            SiteSetting::Mesa,
            SiteSetting::OldQuarter,
        ] {
            chunks.extend(
                sites
                    .iter()
                    .filter(|site| site.setting == setting)
                    .take(2)
                    .map(|site| crate::terrain::ChunkPosition::of(site.door)),
            );
        }
        let moon = crate::worldgen::sky::moon(&settings).expect("a moon");
        chunks.extend(
            sites_touching(&settings, &moon.bounds())
                .iter()
                .map(|site| crate::terrain::ChunkPosition::of(site.door)),
        );
        chunks.push(crate::terrain::ChunkPosition::of(slot_origin(3)));
        chunks.push(crate::terrain::ChunkPosition { x: 3, y: 2 });
        let mut combined = 0_u64;
        for chunk in chunks {
            for (index, tile) in super::super::generated_chunk(&settings, chunk)
                .tiles()
                .iter()
                .enumerate()
            {
                combined = combined
                    .rotate_left(7)
                    .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
            }
        }
        assert_eq!(
            combined, GOLDEN_V10_DIGEST,
            "V10 generation changed: {combined:#018x}"
        );
    }

    const GOLDEN_V10_DIGEST: u64 = 0xd86c_cc9d_d50b_8792;
}
