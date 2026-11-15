//! Where villages go: pure, integer-only site geometry (no terrain, no caches).
//!
//! **Surface sites** sit on a ladder either side of town: site `n` (`n > 0` east, `n < 0` west,
//! never 0) is centred [`FIRST_SITE_OFFSET`] `+ (|n| - 1) *` [`SITE_SPACING`] columns from the
//! town's centre, jittered by up to [`SITE_JITTER`] columns, so neighbouring villages are 400-800
//! columns apart (plan §7 decision 5) and the first ones are a short trip from town. Everything
//! about a site's footprint (size, flat span) is a hash of `(seed, n)`, so any column can tell in
//! O(1) whether it lies in (or near) a village: the surface heightmap asks this for every column
//! ([`crate::worldgen::streamed::surface_row`]).
//!
//! **Underground settlements** are rare: about one site in four (and always one of the two
//! second sites, so a new world has one within reach) also has a settlement hall carved into
//! the strata a few hundred columns away. Its nominal geometry is here; the streamed generator
//! slides it clear of points of interest ([`crate::worldgen::streamed::village_region`]).

use crate::{
    people::VillageId,
    worldgen::{WorldSettings, noise::hash2, strata::scaled_row, streamed::STREAMED_COLUMN_LIMIT},
};

/// Column the site ladder is measured from: the middle of the town's lots.
///
/// The middle of [`crate::surface::town_span`], rounded up (column 115). Every village site of
/// every world is measured from it, so the town's lots must not move.
pub const TOWN_CENTER_COLUMN: i32 = {
    let (first, last) = crate::surface::town_span();
    (first + last + 1) / 2
};
/// Columns between neighbouring sites' nominal centres.
pub const SITE_SPACING: i32 = 600;
/// Columns from the town's centre to the first site's nominal centre (either side).
pub const FIRST_SITE_OFFSET: i32 = 450;
/// How far a site's centre may move off its nominal column (either way).
pub const SITE_JITTER: i32 = 100;
/// Columns over which the ground eases from a village's flat span back into the heightmap.
pub const FLATTEN_FADE_COLUMNS: i32 = 16;
/// The furthest site index (either side) that still lies inside the streamed world.
pub const MAX_SITE: i32 =
    (STREAMED_COLUMN_LIMIT - FIRST_SITE_OFFSET - SITE_JITTER - 256) / SITE_SPACING;

/// Air rows inside an underground settlement hall (floor to ceiling).
pub const HALL_AIR_ROWS: i32 = 10;
/// Columns of an underground settlement hall.
pub const HALL_WIDTH: i32 = 60;

const SALT_SITE: u64 = 0x7111_A6E5_0001;
const SALT_SIZE: u64 = 0x7111_A6E5_0002;
const SALT_UNDERGROUND: u64 = 0x7111_A6E5_0003;

/// How big a village is: its flat span and how many buildings the layout grammar fits.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum VillageSize {
    /// A handful of buildings around a small plaza.
    Hamlet,
    Village,
    /// The big ones: a full street either side of the plaza.
    Town,
}

impl VillageSize {
    pub const ALL: [Self; 3] = [Self::Hamlet, Self::Village, Self::Town];

    /// Columns of flat ground the village stands on.
    #[must_use]
    pub const fn width_tiles(self) -> i32 {
        match self {
            Self::Hamlet => 64,
            Self::Village => 88,
            Self::Town => 112,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hamlet => "hamlet",
            Self::Village => "village",
            Self::Town => "town",
        }
    }

    /// Residents a village of this size houses (inclusive range).
    #[must_use]
    pub const fn residents(self) -> (usize, usize) {
        match self {
            Self::Hamlet => (4, 6),
            Self::Village => (6, 9),
            Self::Town => (9, 12),
        }
    }
}

/// A surface site's footprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceSite {
    /// The site index (`> 0` east of town, `< 0` west).
    pub site: i32,
    /// The centre column.
    pub center: i32,
    pub size: VillageSize,
    /// First and last column of the flat span (inclusive).
    pub first: i32,
    pub last: i32,
}

impl SurfaceSite {
    /// Columns from `x` to the flat span (0 inside it).
    #[must_use]
    pub const fn distance(&self, x: i32) -> i32 {
        if x < self.first {
            self.first - x
        } else if x > self.last {
            x - self.last
        } else {
            0
        }
    }

    #[must_use]
    pub const fn id(&self) -> VillageId {
        surface_id(self.site)
    }
}

/// An underground settlement's nominal hall (before the generator slides it clear of points of
/// interest).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UndergroundSite {
    pub site: i32,
    /// Nominal centre column of the hall.
    pub center: i32,
    /// First air row of the hall (its ceiling is the row above).
    pub top: i32,
    /// Which way the generator slides the hall when it has to (`-1` or `1`).
    pub slide: i32,
}

impl UndergroundSite {
    #[must_use]
    pub const fn id(&self) -> VillageId {
        underground_id(self.site)
    }

    /// The hall's floor row (the first solid row under the air).
    #[must_use]
    pub const fn floor_row(&self) -> i32 {
        self.top + HALL_AIR_ROWS
    }
}

/// Zigzag-encodes a site index (`0, -1, 1, -2, 2, ...` -> `0, 1, 2, 3, 4, ...`).
const fn zigzag(site: i32) -> u32 {
    ((site << 1) ^ (site >> 31)).cast_unsigned()
}

const fn unzigzag(code: u32) -> i32 {
    (code >> 1).cast_signed() ^ -((code & 1).cast_signed())
}

/// The id of the surface village of site `site`.
#[must_use]
pub const fn surface_id(site: i32) -> VillageId {
    VillageId(zigzag(site) << 1)
}

/// The id of the underground settlement of site `site`.
#[must_use]
pub const fn underground_id(site: i32) -> VillageId {
    VillageId((zigzag(site) << 1) | 1)
}

/// The site an id names and whether it is the underground settlement.
#[must_use]
pub const fn decode_id(id: VillageId) -> (i32, bool) {
    (unzigzag(id.0 >> 1), id.0 & 1 == 1)
}

/// Whether `site` is a site index this world places a village at.
#[must_use]
pub const fn valid_site(site: i32) -> bool {
    site != 0 && site.abs() <= MAX_SITE
}

/// The surface site `site` of `seed` (`None` for 0 and indices past the world's reach).
#[must_use]
#[allow(clippy::similar_names, reason = "site and size are different things")]
pub const fn surface_site(seed: u64, site: i32) -> Option<SurfaceSite> {
    if !valid_site(site) {
        return None;
    }
    let offset = FIRST_SITE_OFFSET + (site.abs() - 1) * SITE_SPACING;
    let roll = hash2(seed, SALT_SITE, site, 0);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the jitter roll is reduced below 2 * SITE_JITTER + 1"
    )]
    let jitter = (roll % (2 * SITE_JITTER as u64 + 1)) as i32 - SITE_JITTER;
    let center = TOWN_CENTER_COLUMN + site.signum() * offset + jitter;
    let size_roll = hash2(seed, SALT_SIZE, site, 0) % 100;
    let size = if site.abs() == 1 {
        // The first villages either side are proper villages (a reason to head out).
        if size_roll < 30 {
            VillageSize::Town
        } else {
            VillageSize::Village
        }
    } else if size_roll < 45 {
        VillageSize::Hamlet
    } else if size_roll < 85 {
        VillageSize::Village
    } else {
        VillageSize::Town
    };
    let width = size.width_tiles();
    let first = center - width / 2;
    Some(SurfaceSite {
        site,
        center,
        size,
        first,
        last: first + width - 1,
    })
}

/// The site index whose nominal centre is nearest column `x` (0 near town).
#[must_use]
pub const fn site_index_near(x: i32) -> i32 {
    let offset = x - TOWN_CENTER_COLUMN;
    let distance = offset.abs();
    if distance < FIRST_SITE_OFFSET - SITE_SPACING / 2 {
        return 0;
    }
    let index = (distance - FIRST_SITE_OFFSET + SITE_SPACING / 2) / SITE_SPACING + 1;
    index * offset.signum()
}

/// The surface site whose flat span, grown by `margin` columns, contains column `x`.
#[must_use]
pub const fn surface_site_at(seed: u64, x: i32, margin: i32) -> Option<SurfaceSite> {
    let near = site_index_near(x);
    let mut site = near - 1;
    // Half the widest village: a column further than this (plus the jitter and margin) from a
    // site's nominal centre cannot be in it, so most columns skip the hashing altogether.
    let reach = SITE_JITTER + VillageSize::Town.width_tiles() / 2 + 1 + margin;
    while site <= near + 1 {
        if valid_site(site) {
            let nominal = TOWN_CENTER_COLUMN
                + site.signum() * (FIRST_SITE_OFFSET + (site.abs() - 1) * SITE_SPACING);
            if (x - nominal).abs() <= reach
                && let Some(found) = surface_site(seed, site)
                && found.distance(x) <= margin
            {
                return Some(found);
            }
        }
        site += 1;
    }
    None
}

/// The site indices whose surface village may lie (wholly or partly) in columns `first..=last`.
#[must_use]
pub const fn sites_between(first: i32, last: i32) -> (i32, i32) {
    (site_index_near(first) - 1, site_index_near(last) + 1)
}

/// Site `site`'s nominal underground settlement, if it has one.
#[must_use]
#[allow(clippy::similar_names, reason = "site and side are different things")]
pub const fn underground_site(settings: &WorldSettings, site: i32) -> Option<UndergroundSite> {
    let Some(surface) = surface_site(settings.seed, site) else {
        return None;
    };
    let roll = hash2(settings.seed, SALT_UNDERGROUND, site, 0);
    // One of the two second sites always has one (the side is per seed); elsewhere one in four.
    let near_side = if hash2(settings.seed, SALT_UNDERGROUND, 0, 1).is_multiple_of(2) {
        2
    } else {
        -2
    };
    if site != near_side && roll % 100 >= 25 {
        return None;
    }
    // The first sites' halls lie beyond them, away from town (never under its flat ground).
    let side = if site.abs() == 1 || (roll >> 8).is_multiple_of(2) {
        site.signum()
    } else {
        -site.signum()
    };
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the rolls are reduced to a few hundred"
    )]
    let (along, nominal) = (
        150 + ((roll >> 16) % 120) as i32,
        // Clay beds to the slate shelves of the classic stack.
        40 + ((roll >> 32) % 80) as i32,
    );
    Some(UndergroundSite {
        site,
        center: surface.center + side * along,
        top: scaled_row(settings, nominal),
        slide: side,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_for_both_kinds() {
        for site in -MAX_SITE..=MAX_SITE {
            assert_eq!(decode_id(surface_id(site)), (site, false));
            assert_eq!(decode_id(underground_id(site)), (site, true));
        }
    }

    #[test]
    fn neighbouring_sites_are_four_to_eight_hundred_columns_apart() {
        for seed in 0..40_u64 {
            for site in -12..12 {
                let (Some(a), Some(b)) = (surface_site(seed, site), surface_site(seed, site + 1))
                else {
                    continue;
                };
                let gap = b.center - a.center;
                assert!((400..=800).contains(&gap), "seed {seed} site {site}: {gap}");
                assert!(a.last < b.first);
            }
            // The first sites either side are a short trip from town but clear of its flat ground.
            let east = surface_site(seed, 1).expect("east");
            let west = surface_site(seed, -1).expect("west");
            assert!(east.first - FLATTEN_FADE_COLUMNS > 256, "{east:?}");
            assert!(west.last + FLATTEN_FADE_COLUMNS < -16, "{west:?}");
            assert!(east.center - TOWN_CENTER_COLUMN <= 560);
        }
    }

    #[test]
    fn the_site_ladder_is_measured_from_the_town_centre() {
        // Village sites of saved worlds are measured from here: moving it moves every village.
        assert_eq!(TOWN_CENTER_COLUMN, 115);
        let (first, last) = crate::surface::town_span();
        assert!(first < TOWN_CENTER_COLUMN && TOWN_CENTER_COLUMN < last);
    }

    #[test]
    fn every_column_finds_the_site_it_lies_in() {
        for seed in [3_u64, 77, 0xBEEF] {
            for site in [-9, -2, -1, 1, 2, 9] {
                let found = surface_site(seed, site).expect("site");
                for x in found.first - 20..=found.last + 20 {
                    let at = surface_site_at(seed, x, 0);
                    assert_eq!(
                        at.map(|s| s.site),
                        (found.distance(x) == 0).then_some(site),
                        "seed {seed} x {x}"
                    );
                }
            }
            for x in -16..=255 {
                assert!(surface_site_at(seed, x, FLATTEN_FADE_COLUMNS).is_none());
            }
        }
    }

    #[test]
    fn underground_settlements_are_rare_but_one_is_near() {
        for seed in 0..30_u64 {
            let settings = WorldSettings {
                generator: crate::worldgen::GeneratorVersion::V6,
                ..WorldSettings::new(seed)
            };
            assert!(
                underground_site(&settings, 2).is_some()
                    || underground_site(&settings, -2).is_some()
            );
            let count = (-40..=40)
                .filter(|site| underground_site(&settings, *site).is_some())
                .count();
            assert!((6..=40).contains(&count), "seed {seed}: {count}");
        }
    }
}
