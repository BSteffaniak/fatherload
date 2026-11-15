//! Wanderers' camps (WP-PEOPLE "wandering characters"): traders, prospectors, surveyors and
//! smugglers met out on the open surface between settlements.
//!
//! The surface is cut into slots of [`CAMP_SPACING`] columns. Each slot may hold one camp
//! ([`camp`]): a campfire on a flat stretch of ground, well clear of the town and every village,
//! with one to three wanderers around it. Everything is a pure integer function of the world
//! seed and the slot, so every peer agrees where camps are and who sits at them.
//!
//! A camp is a [`super::routine::Settlement`] with no buildings ([`super::routine::settlement`]
//! builds it), so wanderers live by the same routines as village folk: they sleep by the fire,
//! work around the camp by day and talk, trade, take gifts and remember players exactly like
//! residents do. Camp ids live in their own range of [`VillageId`] ([`CAMP_FLAG`]) so they
//! never collide with a village site.
//!
//! The APIs: [`is_camp`], [`camp_id`], [`camp`], [`camps_between`] and [`camp_population`].

use crate::{
    village::{self, VillageId},
    worldgen::{WorldSettings, biome::Biome, noise::hash2, streamed::cache::MemoCache},
};

use super::{Culture, PeopleRng, Population, Role, generate_population};

/// Set on every camp's [`VillageId`] (village ids are small: `site << 1`).
pub const CAMP_FLAG: u32 = 1 << 31;
/// Columns per camp slot.
pub const CAMP_SPACING: i32 = 360;
/// Percent of slots that hold a camp (before the site checks).
const CAMP_CHANCE: u64 = 60;
/// Columns a camp keeps from the town and every village.
const CAMP_CLEARANCE: i32 = 70;
/// Half the camp's width in columns (the ground must be flat across it).
pub const CAMP_HALF_WIDTH: i32 = 6;
/// Spots tried in a slot before it is left empty.
const CAMP_TRIES: i32 = 10;
/// Furthest slot from the town a camp is placed in (matches the streamed world's reach).
const MAX_SLOT: i32 = 1 << 20;
const SALT_CAMP: u64 = 0x5045_4F50_4C45_0050;

/// What kind of camp it is (the first wanderer's trade; it sets the camp's props).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CampKind {
    /// A trader's wagon with a canvas hood.
    Trader,
    /// A prospector's tent, pans and a pick.
    Prospector,
    /// A surveyor's (a travelling scientist's) tent with instruments on a tripod.
    Surveyor,
    /// A smuggler's lean-to with crates nobody asks about.
    Smuggler,
}

impl CampKind {
    pub const ALL: [Self; 4] = [
        Self::Trader,
        Self::Prospector,
        Self::Surveyor,
        Self::Smuggler,
    ];

    /// The trade of the camp's lead wanderer.
    #[must_use]
    pub const fn lead_role(self) -> Role {
        match self {
            Self::Trader => Role::Trader,
            Self::Prospector => Role::Prospector,
            Self::Surveyor => Role::Scholar,
            Self::Smuggler => Role::Courier,
        }
    }

    /// The trades of anyone travelling with them.
    const fn companions(self) -> &'static [Role] {
        match self {
            Self::Trader => &[Role::Trader, Role::Courier, Role::Tinkerer],
            Self::Prospector => &[Role::Prospector, Role::RetiredMiner, Role::Apprentice],
            Self::Surveyor => &[Role::Cartographer, Role::Scholar, Role::Apprentice],
            Self::Smuggler => &[Role::Courier, Role::Mechanic, Role::Hermit],
        }
    }

    /// A short label ("trader's wagon").
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Trader => "trader's wagon",
            Self::Prospector => "prospector's camp",
            Self::Surveyor => "surveyor's tent",
            Self::Smuggler => "smuggler's hideout",
        }
    }
}

/// One wanderers' camp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Camp {
    pub id: VillageId,
    pub kind: CampKind,
    /// The campfire's column.
    pub column: i32,
    /// The first solid row under the camp (people stand on it).
    pub ground_row: i32,
    /// How many wanderers sit at it (1..=3).
    pub size: u8,
    pub culture: Culture,
}

impl Camp {
    /// The camp's first and last columns.
    #[must_use]
    pub const fn span(&self) -> (i32, i32) {
        (self.column - CAMP_HALF_WIDTH, self.column + CAMP_HALF_WIDTH)
    }
}

/// Whether `id` names a camp rather than a village or the town.
#[must_use]
pub const fn is_camp(id: VillageId) -> bool {
    id.0 & CAMP_FLAG != 0
}

/// The id of the camp in slot `slot`.
#[must_use]
#[allow(clippy::cast_sign_loss, reason = "zigzag encoding")]
pub const fn camp_id(slot: i32) -> VillageId {
    let zigzag = ((slot << 1) ^ (slot >> 31)) as u32;
    VillageId(CAMP_FLAG | (zigzag & !CAMP_FLAG))
}

/// The slot a camp id names.
#[must_use]
#[allow(clippy::cast_possible_wrap, reason = "zigzag decoding")]
const fn camp_slot(id: VillageId) -> i32 {
    let zigzag = id.0 & !CAMP_FLAG;
    ((zigzag >> 1) as i32) ^ -((zigzag & 1) as i32)
}

/// The camp `id` of a world with `settings` (`None` if the slot has none, or `id` is not a
/// camp). Only worlds with villages have camps.
#[must_use]
pub fn camp(settings: &WorldSettings, id: VillageId) -> Option<Camp> {
    if !is_camp(id) || !settings.generator.has_villages() {
        return None;
    }
    let slot = camp_slot(id);
    if slot.abs() > MAX_SLOT {
        return None;
    }
    let roll = hash2(settings.seed, SALT_CAMP, slot, 0);
    if roll % 100 >= CAMP_CHANCE {
        return None;
    }
    let key = (*settings, slot, 1);
    let pitch = *CAMP_SITES.get_or_compute(key, || camp_site(settings, slot));
    let (column, ground_row, biome) = pitch?;
    let mut rng = PeopleRng::new(settings.seed, SALT_CAMP, u64::from(id.0), 1);
    let kind = *rng.pick(&CampKind::ALL);
    let culture = match biome {
        Biome::Desert | Biome::Badlands => *rng.pick(&[Culture::Mesa, Culture::Scrapper]),
        Biome::Tundra => *rng.pick(&[Culture::Highland, Culture::Frontier]),
        Biome::Forest | Biome::Wetlands => *rng.pick(&[Culture::Riverside, Culture::Frontier]),
        Biome::Plains | Biome::Mountains => {
            *rng.pick(&[Culture::Frontier, Culture::Scrapper, Culture::Riverside])
        }
    };
    let size = u8::try_from(1 + rng.below(3)).unwrap_or(1);
    Some(Camp {
        id,
        kind,
        column,
        ground_row,
        size,
        culture,
    })
}

/// Where slot `slot`'s camp pitches: the first of a few seeded spots in the slot that is well
/// clear of the town, every village and the trail milestones, flat across the camp, dry and off
/// the mountains.
/// Returns `(column, ground row, biome)`.
fn camp_site(settings: &WorldSettings, slot: i32) -> Option<(i32, i32, Biome)> {
    let inner = u64::try_from((CAMP_SPACING - 2 * CAMP_CLEARANCE).max(1)).unwrap_or(1);
    let (town_first, town_last) = crate::surface::town_span();
    let nearby = village::villages_between(
        settings,
        slot * CAMP_SPACING - 80,
        (slot + 1) * CAMP_SPACING + 80,
    );
    (1..=CAMP_TRIES).find_map(|attempt| {
        let roll = hash2(settings.seed, SALT_CAMP, slot, attempt);
        let offset = i32::try_from(roll % inner).unwrap_or(0) + CAMP_CLEARANCE;
        let column = slot * CAMP_SPACING + offset;
        if column + CAMP_CLEARANCE * 2 >= town_first && column - CAMP_CLEARANCE * 2 <= town_last {
            return None;
        }
        let crowded = nearby.iter().any(|found| {
            found.last_column + CAMP_CLEARANCE >= column
                && found.first_column - CAMP_CLEARANCE <= column
        });
        if crowded {
            return None;
        }
        let ground_row = crate::worldgen::surface_row(settings, column);
        let flat = (column - CAMP_HALF_WIDTH..=column + CAMP_HALF_WIDTH).all(|x| {
            crate::worldgen::surface_row(settings, x) == ground_row
                && crate::worldgen::biome::lake_row(settings, x).is_none()
        });
        // Clear of the trail milestones, so a post never stands in the camp.
        let milestone = (column - CAMP_HALF_WIDTH - 3..=column + CAMP_HALF_WIDTH + 3)
            .any(|x| village::milestone_at(settings, x).is_some());
        let biome = crate::worldgen::biome::biome_at(settings, column);
        (flat && !milestone && biome != Biome::Mountains).then_some((column, ground_row, biome))
    })
}

static CAMP_SITES: MemoCache<Option<(i32, i32, Biome)>> = MemoCache::new(64);

/// Every camp whose campfire lies in `first..=last`, west to east.
#[must_use]
pub fn camps_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<Camp> {
    if !settings.generator.has_villages() || first > last {
        return Vec::new();
    }
    let first_slot = first.div_euclid(CAMP_SPACING);
    let last_slot = last.div_euclid(CAMP_SPACING);
    (first_slot..=last_slot)
        .filter_map(|slot| camp(settings, camp_id(slot)))
        .filter(|found| (first..=last).contains(&found.column))
        .collect()
}

/// The wanderers of `camp`: a population whose lead has the camp's trade and whose companions
/// travel with them, named for the camp ("Ashford Wagons").
#[must_use]
pub fn camp_population(seed: u64, camp: &Camp) -> Population {
    let mut population = generate_population(seed, camp.id, camp.culture, usize::from(camp.size));
    let mut rng = PeopleRng::new(seed, SALT_CAMP, u64::from(camp.id.0), 2);
    for (index, character) in population.characters.iter_mut().enumerate() {
        let role = if index == 0 {
            camp.kind.lead_role()
        } else {
            *rng.pick(camp.kind.companions())
        };
        if character.role != role {
            character.role = role;
            let (youngest, oldest) = role.ages();
            character.age = character.age.clamp(youngest, oldest);
            character.offer = role.offer();
            character.history = super::character::history(seed, character);
        }
    }
    let place = match camp.kind {
        CampKind::Trader => "Wagons",
        CampKind::Prospector => "Diggings",
        CampKind::Surveyor => "Survey Camp",
        CampKind::Smuggler => "Hollow",
    };
    population.village_name = format!("{} {place}", population.village_name);
    population
}

#[cfg(test)]
mod tests {
    use super::{CAMP_SPACING, camp, camp_id, camp_population, camp_slot, camps_between, is_camp};
    use crate::{
        village::{self, placement},
        worldgen::{GeneratorVersion, WorldSettings},
    };

    fn settings() -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V7,
            ..WorldSettings::new(0x5EED_0017)
        }
    }

    #[test]
    fn camp_ids_round_trip_and_never_name_a_village() {
        for slot in [-70_000, -3, -1, 0, 1, 2, 9, 70_000] {
            let id = camp_id(slot);
            assert!(is_camp(id));
            assert_eq!(camp_slot(id), slot);
        }
        for site in -40..40 {
            assert!(!is_camp(placement::surface_id(site)));
            assert!(!is_camp(placement::underground_id(site)));
        }
        assert!(village::village(&settings(), camp_id(3)).is_none());
    }

    #[test]
    fn camps_are_deterministic_clear_of_settlements_and_on_flat_ground() {
        let settings = settings();
        let span = 40 * CAMP_SPACING;
        let camps = camps_between(&settings, -span, span);
        assert!(camps.len() >= 10, "a fair number of camps: {}", camps.len());
        assert_eq!(camps, camps_between(&settings, -span, span));
        let (town_first, town_last) = crate::surface::town_span();
        for found in &camps {
            assert!(found.column < town_first - 100 || found.column > town_last + 100);
            for nearby in
                village::villages_between(&settings, found.column - 200, found.column + 200)
            {
                assert!(
                    nearby.last_column < found.column - 60
                        || nearby.first_column > found.column + 60,
                    "{found:?} crowds {}",
                    nearby.name
                );
            }
            let (first, last) = found.span();
            for x in first..=last {
                assert_eq!(crate::worldgen::surface_row(&settings, x), found.ground_row);
            }
            assert_eq!(camp(&settings, found.id), Some(*found));
        }
        let kinds: std::collections::BTreeSet<_> = camps.iter().map(|found| found.kind).collect();
        assert!(kinds.len() >= 3, "varied camps: {kinds:?}");
    }

    #[test]
    fn wanderers_follow_their_camp_trade() {
        let settings = settings();
        for found in camps_between(&settings, -20 * CAMP_SPACING, 20 * CAMP_SPACING) {
            let population = camp_population(settings.seed, &found);
            assert_eq!(population.characters.len(), usize::from(found.size));
            assert_eq!(population.characters[0].role, found.kind.lead_role());
            assert!(population.village_name.contains(' '));
            for (index, character) in population.characters.iter().enumerate() {
                assert_eq!(usize::from(character.id.index), index);
                assert_eq!(character.id.village, found.id);
                let (youngest, oldest) = character.role.ages();
                assert!((youngest..=oldest).contains(&character.age));
            }
        }
    }
}
