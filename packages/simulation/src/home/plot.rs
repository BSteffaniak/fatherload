//! Building plots: sizes, settings, the land office's listed sites and pricing.
//!
//! The land office lists **sites** ([`PlotSite`]): a few around the town, one at each end of
//! every surface village, and scenic spots (lakesides, clifftops, quiet meadows) scattered
//! along the surface every [`SCENIC_SPACING`] columns. Everything about a listed site is a
//! pure integer function of the world settings, so every peer lists the same sites. Players
//! can also survey any free spot of open ground outside the town, villages and claims (the
//! world checks the ground; [`survey_price`] prices it by size, setting, distance from town
//! and the earth to move), including floating sky lands and the Moon (V8 skies).

use serde::{Deserialize, Serialize};

use crate::{
    people::VillageId,
    worldgen::{WorldSettings, noise::hash2},
};

/// A plot's size.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PlotSize {
    Small,
    Medium,
    Large,
    Estate,
}

impl PlotSize {
    pub const ALL: [Self; 4] = [Self::Small, Self::Medium, Self::Large, Self::Estate];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Small => "Small plot",
            Self::Medium => "Medium plot",
            Self::Large => "Large plot",
            Self::Estate => "Estate",
        }
    }

    /// Tiles wide.
    #[must_use]
    pub const fn width(self) -> i32 {
        match self {
            Self::Small => 11,
            Self::Medium => 16,
            Self::Large => 24,
            Self::Estate => 34,
        }
    }

    /// The deed's list price before the setting's markup.
    #[must_use]
    pub const fn base_price(self) -> u32 {
        match self {
            Self::Small => 900,
            Self::Medium => 2_200,
            Self::Large => 5_200,
            Self::Estate => 12_000,
        }
    }

    /// Extra build time for the grounds (gardens, paths, a gate), tenths of a day.
    #[must_use]
    pub const fn grounds_tenth_days(self) -> u32 {
        match self {
            Self::Small | Self::Medium => 0,
            Self::Large => 5,
            Self::Estate => 20,
        }
    }

    /// Extra price for the grounds.
    #[must_use]
    pub const fn grounds_price(self) -> u32 {
        match self {
            Self::Small | Self::Medium => 0,
            Self::Large => 1_500,
            Self::Estate => 6_000,
        }
    }

    /// The next size up (wrapping), for the survey picker.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Small => Self::Medium,
            Self::Medium => Self::Large,
            Self::Large => Self::Estate,
            Self::Estate => Self::Small,
        }
    }
}

/// Where a plot lies.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PlotSetting {
    /// Beside the town.
    Town,
    /// On the edge of a village.
    Village(VillageId),
    /// By a lake.
    Lakeside,
    /// On a clifftop with a view.
    Clifftop,
    /// A quiet meadow.
    Meadow,
    /// A surveyed spot in the wilds.
    Wilds,
    /// On a floating sky land.
    SkyIsland,
    /// On the Moon.
    Moon,
}

impl PlotSetting {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Town => "Edge of town",
            Self::Village(_) => "Village outskirts",
            Self::Lakeside => "Lakeside",
            Self::Clifftop => "Clifftop",
            Self::Meadow => "Meadow",
            Self::Wilds => "The wilds",
            Self::SkyIsland => "Sky island",
            Self::Moon => "The Moon",
        }
    }

    /// Price markup in percent.
    #[must_use]
    pub const fn price_percent(self) -> u32 {
        match self {
            Self::Town => 150,
            Self::Village(_) => 110,
            Self::Lakeside => 135,
            Self::Clifftop => 140,
            Self::Meadow => 100,
            Self::Wilds => 75,
            Self::SkyIsland => 170,
            Self::Moon => 240,
        }
    }

    /// Whether a scenic setting (the land office lists these as views).
    #[must_use]
    pub const fn scenic(self) -> bool {
        matches!(self, Self::Lakeside | Self::Clifftop | Self::Meadow)
    }
}

/// How a site is known.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SiteKey {
    /// The town's sites, west then east.
    Town(u8),
    /// A village's sites (0 west, 1 east).
    Village(VillageId, u8),
    /// The scenic spot of a slot of [`SCENIC_SPACING`] columns.
    Scenic(i32),
    /// A spot a player surveyed.
    Surveyed,
}

/// A piece of land a deed can cover.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PlotSite {
    pub key: SiteKey,
    pub size: PlotSize,
    pub setting: PlotSetting,
    /// The leftmost column.
    pub left: i32,
    /// The ground row: the first solid row under the plot (the house stands on it).
    pub ground_row: i32,
    /// Credits the deed costs.
    pub price: u32,
}

impl PlotSite {
    #[must_use]
    pub const fn width(&self) -> i32 {
        self.size.width()
    }

    /// The rightmost column (inclusive).
    #[must_use]
    pub const fn right(&self) -> i32 {
        self.left + self.size.width() - 1
    }

    #[must_use]
    pub const fn middle(&self) -> i32 {
        self.left + self.size.width() / 2
    }

    #[must_use]
    pub const fn contains_column(&self, column: i32) -> bool {
        column >= self.left && column <= self.right()
    }

    /// Whether the plot's columns overlap `first..=last`.
    #[must_use]
    pub const fn overlaps_columns(&self, first: i32, last: i32) -> bool {
        self.left <= last && self.right() >= first
    }
}

/// Columns per scenic slot.
pub const SCENIC_SPACING: i32 = 420;
/// Percent of scenic slots that hold a spot.
const SCENIC_CHANCE: u64 = 55;
/// Tiles kept clear around villages, the town and camps.
const CLEARANCE: i32 = 8;
/// How far a site search slides looking for flat ground.
const SEARCH_COLUMNS: i32 = 40;
const SALT_SITES: u64 = 0x484F_4D45_0000_0010;
/// Columns beyond which sites are not listed (far beyond any reachable distance).
const MAX_COLUMN: i32 = 1 << 26;

/// The town's sites: `(size, offset from the town edge, east)`.
const TOWN_SITES: [(PlotSize, i32, bool); 5] = [
    (PlotSize::Small, 6, false),
    (PlotSize::Medium, 22, false),
    (PlotSize::Small, 6, true),
    (PlotSize::Medium, 22, true),
    (PlotSize::Large, 44, true),
];

/// Every listed site with columns in `first..=last`, west to east.
#[must_use]
pub fn listed_sites_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<PlotSite> {
    if first > last {
        return Vec::new();
    }
    let first = first.max(-MAX_COLUMN);
    let last = last.min(MAX_COLUMN);
    let mut sites: Vec<PlotSite> = (0..TOWN_SITES.len())
        .filter_map(|index| town_site(settings, u8::try_from(index).unwrap_or(0)))
        .collect();
    for found in crate::village::villages_between(settings, first - 60, last + 60) {
        if found.setting != crate::village::VillageSetting::Surface {
            continue;
        }
        for side in 0..2 {
            sites.extend(village_site(settings, found.id, side));
        }
    }
    if settings.generator.has_biomes() {
        let first_slot = first.div_euclid(SCENIC_SPACING);
        let last_slot = last.div_euclid(SCENIC_SPACING);
        sites.extend((first_slot..=last_slot).filter_map(|slot| scenic_site(settings, slot)));
    }
    sites.retain(|site| site.overlaps_columns(first, last));
    sites.sort_by_key(|site| (site.left, site.key));
    sites.dedup_by_key(|site| site.key);
    sites
}

/// The listed site `key`, if it exists in this world.
#[must_use]
pub fn listed_site(settings: &WorldSettings, key: SiteKey) -> Option<PlotSite> {
    match key {
        SiteKey::Town(index) => town_site(settings, index),
        SiteKey::Village(village, end) => village_site(settings, village, end),
        SiteKey::Scenic(slot) => {
            if settings.generator.has_biomes() {
                scenic_site(settings, slot)
            } else {
                None
            }
        }
        SiteKey::Surveyed => None,
    }
}

fn surface(settings: &WorldSettings, x: i32) -> i32 {
    crate::worldgen::surface_row(settings, x)
}

/// Whether columns `left..left + width` are level ground, dry, clear of villages, camps, the
/// town and trail milestones.
fn buildable(settings: &WorldSettings, left: i32, width: i32) -> Option<i32> {
    let row = surface(settings, left);
    let (town_first, town_last) = crate::surface::town_span();
    if left <= town_last + 2 && left + width > town_first - 2 {
        return None;
    }
    if settings.generator.is_streamed() {
        let first = left - CLEARANCE;
        let last = left + width - 1 + CLEARANCE;
        let crowded = crate::village::villages_between(settings, first - 80, last + 80)
            .iter()
            .any(|found| found.first_column <= last && found.last_column >= first);
        if crowded {
            return None;
        }
        let camped = crate::people::camps::camps_between(settings, first - 20, last + 20)
            .iter()
            .any(|camp| {
                let (camp_first, camp_last) = camp.span();
                camp_first <= last && camp_last >= first
            });
        if camped {
            return None;
        }
    }
    let level = (left..left + width).all(|x| {
        surface(settings, x) == row
            && (!settings.generator.has_biomes()
                || crate::worldgen::biome::lake_row(settings, x).is_none())
            && crate::village::milestone_at(settings, x).is_none()
    });
    level.then_some(row)
}

/// Slides from `start` (east when `east`, else west) looking for a buildable stretch.
fn find_level(settings: &WorldSettings, start: i32, width: i32, east: bool) -> Option<(i32, i32)> {
    (0..SEARCH_COLUMNS).find_map(|step| {
        let left = if east { start + step } else { start - step };
        buildable(settings, left, width).map(|row| (left, row))
    })
}

fn town_site(settings: &WorldSettings, index: u8) -> Option<PlotSite> {
    let (size, offset, east) = *TOWN_SITES.get(usize::from(index))?;
    let (town_first, town_last) = crate::surface::town_span();
    let start = if east {
        town_last + offset
    } else {
        town_first - offset - size.width()
    };
    let (left, ground_row) = find_level(settings, start, size.width(), east)?;
    if !settings.generator.is_streamed() {
        // Fixed-width worlds: stay inside the map.
        let width = settings.width_tiles();
        if left < 1 || left + size.width() >= width - 1 {
            return None;
        }
    }
    Some(PlotSite {
        key: SiteKey::Town(index),
        size,
        setting: PlotSetting::Town,
        left,
        ground_row,
        price: list_price(size, PlotSetting::Town),
    })
}

fn village_site(settings: &WorldSettings, village: VillageId, end: u8) -> Option<PlotSite> {
    let found = crate::village::village(settings, village)?;
    if found.setting != crate::village::VillageSetting::Surface || end > 1 {
        return None;
    }
    let roll = hash2(
        settings.seed ^ SALT_SITES,
        u64::from(village.0),
        i32::from(end),
        1,
    );
    let size = if roll.is_multiple_of(3) {
        PlotSize::Medium
    } else {
        PlotSize::Small
    };
    let east = end == 1;
    let start = if east {
        found.last_column + CLEARANCE + 1
    } else {
        found.first_column - CLEARANCE - size.width()
    };
    let (left, ground_row) = find_level(settings, start, size.width(), east)?;
    let setting = PlotSetting::Village(village);
    Some(PlotSite {
        key: SiteKey::Village(village, end),
        size,
        setting,
        left,
        ground_row,
        price: list_price(size, setting),
    })
}

fn scenic_site(settings: &WorldSettings, slot: i32) -> Option<PlotSite> {
    let roll = hash2(settings.seed ^ SALT_SITES, 7, slot, 0);
    if roll % 100 >= SCENIC_CHANCE {
        return None;
    }
    let size = if roll % 7 < 3 {
        PlotSize::Estate
    } else {
        PlotSize::Large
    };
    (1..=6).find_map(|attempt| {
        let pick = hash2(settings.seed ^ SALT_SITES, 8, slot, attempt);
        let span = u64::try_from(SCENIC_SPACING - size.width() - 40)
            .unwrap_or(1)
            .max(1);
        let left = slot * SCENIC_SPACING + 20 + i32::try_from(pick % span).unwrap_or(0);
        let ground_row = buildable(settings, left, size.width())?;
        if crate::worldgen::biome::biome_at(settings, left)
            == crate::worldgen::biome::Biome::Mountains
        {
            return None;
        }
        // Clear of the town's and the villages' own sites.
        let right = left + size.width() - 1;
        let town = (0..TOWN_SITES.len())
            .filter_map(|index| town_site(settings, u8::try_from(index).unwrap_or(0)));
        let villages: Vec<PlotSite> =
            crate::village::villages_between(settings, left - 120, right + 120)
                .iter()
                .flat_map(|found| {
                    [
                        village_site(settings, found.id, 0),
                        village_site(settings, found.id, 1),
                    ]
                })
                .flatten()
                .collect();
        if town
            .chain(villages)
            .any(|site| site.overlaps_columns(left - CLEARANCE, right + CLEARANCE))
        {
            return None;
        }
        let setting = scenic_setting(settings, left, size.width(), ground_row);
        Some(PlotSite {
            key: SiteKey::Scenic(slot),
            size,
            setting,
            left,
            ground_row,
            price: list_price(size, setting),
        })
    })
}

/// A lake within reach makes a lakeside; a drop of 5+ rows beside the plot a clifftop; else a
/// meadow.
fn scenic_setting(settings: &WorldSettings, left: i32, width: i32, row: i32) -> PlotSetting {
    let near = (left - 12..left).chain(left + width..left + width + 12);
    let mut lake = false;
    let mut cliff = false;
    for x in near {
        lake |= crate::worldgen::biome::lake_row(settings, x).is_some();
        cliff |= surface(settings, x) >= row + 5;
    }
    if lake {
        PlotSetting::Lakeside
    } else if cliff {
        PlotSetting::Clifftop
    } else {
        PlotSetting::Meadow
    }
}

/// A listed deed's price.
#[must_use]
pub const fn list_price(size: PlotSize, setting: PlotSetting) -> u32 {
    size.base_price() * setting.price_percent() / 100
}

/// Credits per tile of earth moved to level a surveyed plot.
pub const GRADING_PRICE_PER_TILE: u32 = 8;

/// A surveyed spot's price: the size and setting's list price, cheaper the farther from town
/// (down to 60 % past 6,000 columns), plus the earth to move.
#[must_use]
pub fn survey_price(size: PlotSize, setting: PlotSetting, distance: i32, graded_tiles: u32) -> u32 {
    let list = list_price(size, setting);
    let far = (distance.unsigned_abs() / 150).min(40);
    list * (100 - far) / 100 + graded_tiles * GRADING_PRICE_PER_TILE
}

#[cfg(test)]
mod tests {
    use super::{PlotSetting, PlotSize, SiteKey, listed_site, listed_sites_between, survey_price};
    use crate::worldgen::{
        GeneratorVersion, HazardIntensity, ResourceRichness, WorldSettings, WorldSize,
    };

    fn settings(seed: u64, generator: GeneratorVersion) -> WorldSettings {
        WorldSettings {
            seed,
            size: WorldSize::Standard,
            richness: ResourceRichness::Standard,
            hazards: HazardIntensity::Standard,
            generator,
        }
    }

    #[test]
    fn listed_sites_are_deterministic_level_and_clear() {
        for generator in [GeneratorVersion::V7, GeneratorVersion::V8] {
            let settings = settings(11, generator);
            let sites = listed_sites_between(&settings, -2_500, 2_500);
            assert_eq!(sites, listed_sites_between(&settings, -2_500, 2_500));
            assert!(sites.iter().any(|site| site.setting == PlotSetting::Town));
            assert!(
                sites
                    .iter()
                    .any(|site| matches!(site.setting, PlotSetting::Village(_))),
                "{generator:?}"
            );
            for site in &sites {
                assert_eq!(listed_site(&settings, site.key), Some(*site));
                for x in site.left..=site.right() {
                    assert_eq!(crate::worldgen::surface_row(&settings, x), site.ground_row);
                }
                let overlapping = sites
                    .iter()
                    .filter(|other| other.overlaps_columns(site.left, site.right()))
                    .count();
                assert_eq!(overlapping, 1, "{site:?}");
            }
        }
    }

    #[test]
    fn older_worlds_still_list_town_sites() {
        let settings = settings(3, GeneratorVersion::V4);
        let sites = listed_sites_between(&settings, -400, 400);
        assert!(
            sites
                .iter()
                .all(|site| matches!(site.key, SiteKey::Town(_)))
        );
        assert!(!sites.is_empty());
    }

    #[test]
    fn far_wilds_are_cheaper_and_grading_costs() {
        let near = survey_price(PlotSize::Medium, PlotSetting::Wilds, 100, 0);
        let far = survey_price(PlotSize::Medium, PlotSetting::Wilds, 9_000, 0);
        assert!(far < near);
        assert!(survey_price(PlotSize::Medium, PlotSetting::Wilds, 100, 20) > near);
    }
}
