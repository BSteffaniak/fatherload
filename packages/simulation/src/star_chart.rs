//! Star charts: the first coordinates of the sky beyond the Moon (expansion plan §14, the
//! Archive of Stars; Wave 10 ancients-a). **The seam Wave 11's space generator reads.**
//!
//! A chart is read at a plinth in the Archive of Stars (a puzzle vault's prize, or the
//! Orrery's own). Everything about it is a pure function of the world seed and the plinth's
//! tile ([`chart_at_site`]), so every peer agrees and a chart read twice is the same chart.
//! The crew's discovered list is per world, in the order found, saved and replicated in the
//! buried city's shared section ([`crate::ancient_city::AncientCity::charts`]); read it with
//! `WorldState::star_charts()`.
//!
//! # Charts lead to real systems (Wave 11)
//!
//! * [`StarChart::sector`] is a **space sector** ([`crate::space::SpaceSector`]) relative to
//!   the home system's at `(0, 0)` ([`HOME_SECTOR`]). Charts lie [`CHART_RING`] sectors out,
//!   nearer charts first: the Archive's shallower vaults point to nearer stars.
//! * [`StarChart::system`] is the id of a **real system the space generator places in that
//!   sector** ([`crate::space::StarSystem::id`]): [`chart_at_site`] asks the generator
//!   ([`crate::space::chart_target`]) and picks one of the sector's systems with worlds.
//!   [`crate::space::resolve_chart`] (or `Space::chart_system`) maps a chart to its
//!   `(sector, system index)`; a chart read before Wave 11 (an id hashed from its plinth)
//!   resolves to a system of its sector picked by that id, so every chart leads somewhere.
//! * [`StarChart::class`] and [`StarChart::worlds`] are the generator's star class and planet
//!   count for that system (at least one world).
//! * [`StarChart::name`] is the builders' name for it, as the archivist reads it (the same
//!   name the generator gives the system).
//! * The list never shrinks or reorders; new charts append. Ids are unique in a list (two
//!   plinths charting the same star add it once).

use serde::{Deserialize, Serialize};

use crate::{ids::SimulationTick, worldgen::noise::hash2};

/// The home system's sector (the planet you dig into, the Moon its first stop).
pub const HOME_SECTOR: (i32, i32) = (0, 0);
/// How far out charted systems lie, sectors `(nearest, farthest)`.
pub const CHART_RING: (i32, i32) = (3, 48);
/// Most charts a world remembers (far more than a crew will ever read).
pub const MAX_CHARTS: usize = 512;

const SALT_SYSTEM: u64 = 0x57A2_C0DE;
const SALT_PLACE: u64 = 0x57A2_0001;
const SALT_NAME: u64 = 0x57A2_0002;
const SALT_PICK: u64 = 0x57A2_0003;

/// What kind of star a chart shows.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum StarClass {
    /// A yellow sun like home's.
    #[default]
    Yellow,
    /// A small red star.
    Red,
    /// A great blue-white star.
    Blue,
    /// A white dwarf.
    White,
    /// Two suns circling each other.
    Binary,
    /// A star deep in a glowing nebula.
    Nebula,
}

impl StarClass {
    pub const ALL: [Self; 6] = [
        Self::Yellow,
        Self::Red,
        Self::Blue,
        Self::White,
        Self::Binary,
        Self::Nebula,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Yellow => "yellow sun",
            Self::Red => "red dwarf",
            Self::Blue => "blue giant",
            Self::White => "white dwarf",
            Self::Binary => "twin suns",
            Self::Nebula => "nebula star",
        }
    }

    /// The star's colour (presentation; `[r, g, b]`).
    #[must_use]
    pub const fn colour(self) -> [u8; 3] {
        match self {
            Self::Yellow => [255, 226, 130],
            Self::Red => [255, 120, 90],
            Self::Blue => [150, 200, 255],
            Self::White => [235, 240, 255],
            Self::Binary => [255, 200, 160],
            Self::Nebula => [220, 150, 255],
        }
    }
}

/// One charted star system (see the module docs).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StarChart {
    /// The system's stable id (seeds its generation).
    pub system: u64,
    /// Its galactic sector, relative to home.
    pub sector: (i32, i32),
    pub class: StarClass,
    /// Worlds circling it (at least one).
    pub worlds: u8,
    /// The builders' name for it.
    pub name: String,
    /// The plinth it was read at.
    pub site: (i32, i32),
    /// When it was read.
    pub found: SimulationTick,
}

impl StarChart {
    /// How far from home it lies, in sectors (rounded).
    #[must_use]
    pub fn distance(&self) -> u32 {
        let dx = i64::from(self.sector.0 - HOME_SECTOR.0);
        let dy = i64::from(self.sector.1 - HOME_SECTOR.1);
        u32::try_from(isqrt(dx * dx + dy * dy)).unwrap_or(u32::MAX)
    }
}

/// The integer square root of `value` (0 for negatives).
#[must_use]
pub const fn isqrt(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    let mut root = 0_i64;
    let mut bit = 1_i64 << 30;
    let mut rest = value;
    while bit > value {
        bit >>= 2;
    }
    while bit != 0 {
        if rest >= root + bit {
            rest -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

const ONSETS: [&str; 16] = [
    "Ar", "Bel", "Cor", "Dra", "El", "Fen", "Gal", "Hes", "Ith", "Ka", "Lun", "Mer", "Nor", "Or",
    "Sol", "Ve",
];
const MIDDLES: [&str; 8] = ["a", "e", "i", "o", "ae", "ia", "u", "y"];
const ENDS: [&str; 16] = [
    "ris", "thon", "dar", "mir", "nex", "lis", "rath", "vel", "sor", "tia", "quin", "dun", "phae",
    "lon", "zar", "nis",
];

/// The builders' name for system `id`.
#[must_use]
pub fn star_name(id: u64) -> String {
    let roll = hash2(id, SALT_NAME, 0, 0);
    #[allow(clippy::cast_possible_truncation, reason = "masked indices")]
    let (a, b, c) = (
        (roll & 15) as usize,
        ((roll >> 4) & 7) as usize,
        ((roll >> 8) & 15) as usize,
    );
    let mut name = format!("{}{}{}", ONSETS[a], MIDDLES[b], ENDS[c]);
    // One in four carries a catalogue number, as the builders wrote them.
    if (roll >> 12).is_multiple_of(4) {
        name.push(' ');
        name.push_str(&(2 + (roll >> 16) % 97).to_string());
    }
    name
}

/// The plinth hash of the chart at tile `(x, y)` of a world seeded `seed` (before Wave 11 a
/// chart's system id; now it only seeds which of its sector's systems the chart names).
#[must_use]
pub const fn system_id(seed: u64, x: i32, y: i32) -> u64 {
    // Never zero (zero could read as "no system" to a later reader).
    hash2(seed, SALT_SYSTEM, x, y) | 1
}

/// The chart on the plinth at tile `(x, y)` (a pure function; `found` is when it was read).
/// `depth_share` (per mille, how deep in the Archive the plinth is) pushes deeper charts
/// further out.
#[must_use]
pub fn chart_at_site(
    seed: u64,
    x: i32,
    y: i32,
    depth_share: u16,
    found: SimulationTick,
) -> StarChart {
    let system = system_id(seed, x, y);
    let roll = hash2(seed, SALT_PLACE, x, y);
    let (near, far) = CHART_RING;
    let share = i64::from(depth_share.min(1_000));
    // The ring widens with depth: shallow plinths chart near stars, deep ones far.
    let reach_min = i64::from(near) + (i64::from(far - near) * share / 2_000);
    let reach_max = i64::from(near) + (i64::from(far - near) * (500 + share / 2) / 1_000);
    let span = (reach_max - reach_min).max(1);
    #[allow(clippy::cast_possible_wrap, reason = "masked")]
    let radius = reach_min + ((roll & 0xFFFF) as i64 % span);
    // A point on the circle of that radius, in integers (every peer agrees bit for bit).
    #[allow(clippy::cast_possible_wrap, reason = "masked")]
    let dx = ((roll >> 16) & 0xFFFF) as i64 % (2 * radius + 1) - radius;
    let dy = isqrt(radius * radius - dx * dx);
    let dy = if (roll >> 48).is_multiple_of(2) {
        dy
    } else {
        -dy
    };
    #[allow(clippy::cast_possible_truncation, reason = "a few dozen sectors")]
    let sector = (dx as i32, dy as i32);
    // Never home itself.
    let sector = if sector == HOME_SECTOR {
        (near, 0)
    } else {
        sector
    };
    // The space generator's own system there (Wave 11): its id, star and world count.
    let target = crate::space::chart_target(seed, sector, hash2(system, SALT_PICK, x, y));
    StarChart {
        system: target.id,
        sector,
        class: target.class,
        worlds: target.planets,
        name: star_name(target.id),
        site: (x, y),
        found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chart_is_a_pure_function_of_its_site() {
        let a = chart_at_site(7, 100, 9_000, 300, SimulationTick::new(5));
        let b = chart_at_site(7, 100, 9_000, 300, SimulationTick::new(99));
        assert_eq!(a.system, b.system);
        assert_eq!(a.sector, b.sector);
        assert_eq!(a.name, b.name);
        assert_ne!(a.system, 0);
        // Another world, another star.
        assert_ne!(chart_at_site(8, 100, 9_000, 300, a.found).system, a.system);
        assert!(a.worlds >= 1);
    }

    #[test]
    fn charts_lie_in_their_ring_and_deeper_ones_further() {
        let mut shallow = 0;
        let mut deep = 0;
        for index in 0..200 {
            let near = chart_at_site(3, index, 9_500, 0, SimulationTick::new(0));
            let far = chart_at_site(3, index, 9_500, 1_000, SimulationTick::new(0));
            for chart in [&near, &far] {
                let distance = chart.distance();
                assert!(distance <= u32::try_from(CHART_RING.1).unwrap_or(0) + 1);
                assert_ne!(chart.sector, HOME_SECTOR);
            }
            shallow += near.distance();
            deep += far.distance();
        }
        assert!(
            deep > shallow,
            "deeper charts reach further: {shallow} vs {deep}"
        );
    }

    #[test]
    fn names_are_stable_and_varied() {
        assert_eq!(star_name(42), star_name(42));
        let names: std::collections::BTreeSet<String> = (0..64).map(star_name).collect();
        assert!(names.len() > 40);
    }
}
