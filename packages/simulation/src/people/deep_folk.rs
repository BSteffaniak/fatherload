//! The residents of the Deep Descent's settlements (Wave 9): Cathedral Station's prospectors
//! at the Descent II outpost, and the Amber Court's workers in the friendly half of the Hive.
//!
//! In Wave 8 both places told their memories of the Fathers' expedition only on a story card.
//! Now each is a settlement like the Moon's colony ([`super::colony`]): an authored cast,
//! one resident per speaker of [`crate::expedition::memories`], who keep routines in their hall
//! (they work and sleep there, nobody leaves the deep), talk, trade, take gifts, remember
//! players and give and take part in stories, and who tell **their own** memories of the
//! Fathers in conversation ([`resident_speaker`]).
//!
//! * **Cathedral Station** ([`CATHEDRAL_STATION`]): the outpost hall of Descent II on the
//!   builders' cable, the quartermaster, a prospector, the medic, the surveyor and the cook.
//!   Frontier folk from the town who came down after the Fathers and stayed.
//! * **The Amber Court** ([`AMBER_COURT`]): the friendly faction's comb hall in the Hive, a
//!   worker elder, a pump tender, a nurse, a soldier of the guard and the queen's voice. They
//!   are insects (the game draws them so, [`is_hive_folk`]); they speak in clicks and hums and
//!   call people soft-shells. While the Court is hostile (the crew stole from it) nobody comes
//!   out to talk.
//!
//! Both ids are fixed (like the colony's) and never name a village, camp, sky town or sky city.
//! Everything is a pure function of the world settings, so every peer agrees who lives where.

use super::{
    Character, Culture, PersonName, Population, Role, VillageId, generate_population,
    routine::Settlement,
};
use crate::{
    descent::Descent,
    expedition::memories::Settlement as Remembering,
    worldgen::{
        WorldSettings,
        descent::{landmarks, living},
    },
};

/// Cathedral Station's settlement id (even, far past every site index, no flags set).
pub const CATHEDRAL_STATION: VillageId = VillageId(0x3FFF_FE00);
/// The Amber Court's settlement id.
pub const AMBER_COURT: VillageId = VillageId(0x3FFF_FD00);

/// One of the deep settlements with residents.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeepSettlement {
    CathedralStation,
    AmberCourt,
}

/// One authored resident: who they are and which memories they tell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cast {
    /// The speaker key of [`crate::expedition::memories`] they speak for.
    pub speaker: &'static str,
    pub given: &'static str,
    pub family: &'static str,
    pub role: Role,
    /// What they say first when someone comes to talk (their own voice, before the memory).
    pub greeting: &'static str,
}

const STATION_CAST: [Cast; 5] = [
    Cast {
        speaker: "quartermaster",
        given: "Hesper",
        family: "Quill",
        role: Role::Quartermaster,
        greeting: "Fuel's dear and patience is dearer. What do you need, driller?",
    },
    Cast {
        speaker: "prospector",
        given: "Oskar",
        family: "Danner",
        role: Role::Prospector,
        greeting: "Pull up a crate. Nobody down here talks about anything but the Fathers \
                   and the next seam, and I'm out of seams.",
    },
    Cast {
        speaker: "medic",
        given: "Lida",
        family: "Marr",
        role: Role::Healer,
        greeting: "Hold still. You've a cut you haven't noticed yet. They all do, this deep.",
    },
    Cast {
        speaker: "surveyor",
        given: "Corwin",
        family: "Vale",
        role: Role::Cartographer,
        greeting: "Plumb to the cable, every mark. Calloway's rule. Mind my chalk lines.",
    },
    Cast {
        speaker: "cook",
        given: "Mags",
        family: "Pell",
        role: Role::Innkeeper,
        greeting: "There's stew. There's always stew. Brannigan's recipe, near as I can guess \
                   it from the tin she left.",
    },
];

const COURT_CAST: [Cast; 5] = [
    Cast {
        speaker: "elder",
        given: "Thrum",
        family: "of the Comb",
        role: Role::Storyteller,
        greeting: "Click-hum. Soft-shell. Sit by the comb, it is warm. The old ones talk \
                   slowly; listen slowly.",
    },
    Cast {
        speaker: "pump_tender",
        given: "Wrench-Thrice",
        family: "of the Nursery",
        role: Role::Mechanic,
        greeting: "Tk-tk. You smell of oil, like the Wrench-One did. Good smell. Pump smell.",
    },
    Cast {
        speaker: "nurse",
        given: "Hum-Softly",
        family: "of the Brood",
        role: Role::Healer,
        greeting: "Hm-hm-hm. Quiet, soft-shell, the brood sleeps. Speak low and I will \
                   answer low.",
    },
    Cast {
        speaker: "soldier",
        given: "Mandible",
        family: "of the Guard",
        role: Role::Warden,
        greeting: "Click. You may stand in the Court. You may not touch the larder. Click.",
    },
    Cast {
        speaker: "queen",
        given: "Voice",
        family: "of the Queen",
        role: Role::ShrineKeeper,
        greeting: "The queen hears you through me. She says: little light, you came far. She \
                   says: so did the others.",
    },
];

impl DeepSettlement {
    pub const ALL: [Self; 2] = [Self::CathedralStation, Self::AmberCourt];

    #[must_use]
    pub const fn id(self) -> VillageId {
        match self {
            Self::CathedralStation => CATHEDRAL_STATION,
            Self::AmberCourt => AMBER_COURT,
        }
    }

    /// The settlement `id` names, if it is one of these.
    #[must_use]
    pub const fn of_id(id: VillageId) -> Option<Self> {
        match id.0 {
            value if value == CATHEDRAL_STATION.0 => Some(Self::CathedralStation),
            value if value == AMBER_COURT.0 => Some(Self::AmberCourt),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CathedralStation => "Cathedral Station",
            Self::AmberCourt => "The Amber Court",
        }
    }

    /// The memories its people tell.
    #[must_use]
    pub const fn remembering(self) -> Remembering {
        match self {
            Self::CathedralStation => Remembering::OldDeepCamp,
            Self::AmberCourt => Remembering::TheHive,
        }
    }

    /// Its authored residents, in resident order.
    #[must_use]
    pub const fn cast(self) -> &'static [Cast] {
        match self {
            Self::CathedralStation => &STATION_CAST,
            Self::AmberCourt => &COURT_CAST,
        }
    }

    const fn culture(self) -> Culture {
        match self {
            Self::CathedralStation => Culture::Frontier,
            Self::AmberCourt => Culture::Deepfolk,
        }
    }
}

/// Whether `id` is one of the deep settlements with residents.
#[must_use]
pub const fn is_deep_folk(id: VillageId) -> bool {
    DeepSettlement::of_id(id).is_some()
}

/// Whether `id` is the Amber Court (its residents are insects: the game draws them so).
#[must_use]
pub const fn is_hive_folk(id: VillageId) -> bool {
    id.0 == AMBER_COURT.0
}

/// The residents of `place`: a population cast from [`DeepSettlement::cast`] (their trades,
/// names and ages fixed; personality, looks and history still from the seed).
#[must_use]
pub fn population(seed: u64, place: DeepSettlement) -> Population {
    let cast = place.cast();
    let mut population = generate_population(seed, place.id(), place.culture(), cast.len());
    for (character, role) in population.characters.iter_mut().zip(cast) {
        character.name = PersonName {
            given: role.given.to_owned(),
            family: role.family.to_owned(),
            nickname: None,
        };
        if character.role != role.role {
            character.role = role.role;
            let (youngest, oldest) = role.role.ages();
            character.age = character.age.clamp(youngest, oldest);
            character.offer = role.role.offer();
            character.history = super::character::history(seed, character);
        }
    }
    place.name().clone_into(&mut population.village_name);
    population
}

/// The speaker key `character` tells the memories of, and which settlement's (a resident of a
/// deep settlement), else `None`.
#[must_use]
pub fn resident_speaker(character: &Character) -> Option<(Remembering, &'static str)> {
    let place = DeepSettlement::of_id(character.id.village)?;
    let cast = place.cast().get(usize::from(character.id.index))?;
    Some((place.remembering(), cast.speaker))
}

/// What `character` says first when someone comes to talk (a deep settlement's resident).
#[must_use]
pub fn greeting(character: &Character) -> Option<&'static str> {
    let place = DeepSettlement::of_id(character.id.village)?;
    place
        .cast()
        .get(usize::from(character.id.index))
        .map(|cast| cast.greeting)
}

/// The settlement ids with residents in a world with `settings` (Deep Descent worlds with
/// people only).
#[must_use]
pub fn settlement_ids(settings: &WorldSettings) -> Vec<VillageId> {
    DeepSettlement::ALL
        .into_iter()
        .filter(|place| hall(settings, *place).is_some())
        .map(DeepSettlement::id)
        .collect()
}

/// `(ground row, first column, last column)` of `place`'s hall: where its people stand and
/// how far they walk (`None` on worlds without the Deep Descent or without people).
#[must_use]
pub fn hall(settings: &WorldSettings, place: DeepSettlement) -> Option<(i32, i32, i32)> {
    if !settings.generator.has_villages() || !settings.generator.has_descents() {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    match place {
        DeepSettlement::CathedralStation => {
            // The outpost hall on the cable: they keep to the half away from the lift's door,
            // so a rig stepping off the lift finds the counter first and the people further in.
            let hall = landmarks::outpost(settings, &layout, Descent::OldDeep)?.bounds;
            let middle = i32::midpoint(hall.min.x, hall.max.x);
            if landmarks::lift_column(settings) > middle {
                Some((hall.max.y, hall.min.x + 3, middle - 2))
            } else {
                Some((hall.max.y, middle + 2, hall.max.x - 3))
            }
        }
        DeepSettlement::AmberCourt => {
            // The Court's middle bay, between two comb pillars (the hall's floor is its last
            // two rows; pillars stand every twelve columns from its third).
            let court = living::court_hall(settings, &layout)?;
            let first = court.min.x + 27;
            Some((court.max.y - 1, first, first + 10))
        }
    }
}

/// `place`'s routine view in a world with `settings`.
#[must_use]
pub fn settlement(settings: &WorldSettings, place: DeepSettlement) -> Option<Settlement> {
    let (ground, first, last) = hall(settings, place)?;
    let middle = i32::midpoint(first, last);
    Some(Settlement::deep_hall(
        place.id(),
        ground,
        (first, last),
        (middle - 2, middle + 2),
        &population(settings.seed, place),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expedition::{ExpeditionState, memories},
        people::{clock::DayTime, routine},
        worldgen::GeneratorVersion,
    };

    fn v11(seed: u64) -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(seed)
        }
    }

    #[test]
    fn the_deep_halls_have_their_cast_on_deep_descent_worlds_only() {
        let settings = v11(0x5EED);
        assert_eq!(
            settlement_ids(&settings),
            vec![CATHEDRAL_STATION, AMBER_COURT]
        );
        for place in DeepSettlement::ALL {
            let settlement = routine::settlement(&settings, place.id()).expect("a settlement");
            assert_eq!(settlement.population.village_name, place.name());
            assert_eq!(settlement.residents.len(), place.cast().len());
            assert!(settlement.underground);
            for (character, cast) in settlement.population.characters.iter().zip(place.cast()) {
                assert_eq!(character.role, cast.role);
                assert_eq!(character.name.given, cast.given);
                assert_eq!(character.id.village, place.id());
                assert_eq!(
                    resident_speaker(character),
                    Some((place.remembering(), cast.speaker))
                );
                // Each speaks for lines that exist, and has one from the start or soon.
                assert!(
                    memories::MEMORIES
                        .iter()
                        .any(|memory| memory.settlement == place.remembering()
                            && memory.speaker_key() == cast.speaker),
                    "{} has nothing to say",
                    cast.speaker
                );
                let back = routine::character(&settings, character.id).expect("found again");
                assert_eq!(back.name, character.name);
            }
            // Day and night they keep to their hall.
            for hour in [2, 9, 13, 19, 23] {
                for index in 0..settlement.residents.len() {
                    let stop = settlement
                        .stop_at(index, DayTime::at(1, hour, 0))
                        .expect("a stop");
                    let column = settlement.place_column(stop.place);
                    assert!(
                        (settlement.first_column - 2..=settlement.last_column + 2)
                            .contains(&column),
                        "{} resident {index} at {column} at {hour}h",
                        place.name()
                    );
                }
            }
            // Never another kind of settlement.
            assert!(crate::village::village(&settings, place.id()).is_none());
            assert!(!crate::people::camps::is_camp(place.id()));
            assert!(!crate::people::colony::is_colony(place.id()));
            assert!(!crate::worldgen::sky::continent::is_sky_city(place.id()));
            assert!(crate::village::placement::decode_sky_town(place.id()).is_none());
        }
        assert!(is_hive_folk(AMBER_COURT) && !is_hive_folk(CATHEDRAL_STATION));
        // Older worlds have no deep halls.
        let classic = WorldSettings {
            generator: GeneratorVersion::V10,
            ..settings
        };
        assert!(settlement_ids(&classic).is_empty());
        assert!(routine::settlement(&classic, CATHEDRAL_STATION).is_none());
    }

    #[test]
    fn every_resident_speaks_their_own_memories_as_the_trail_opens() {
        let settings = v11(0x00C0_FFEE);
        let mut state = ExpeditionState::default();
        let spoken = |state: &ExpeditionState| -> usize {
            DeepSettlement::ALL
                .iter()
                .flat_map(|place| population(settings.seed, *place).characters)
                .filter_map(|character| {
                    let (settlement, speaker) = resident_speaker(&character)?;
                    memories::line_by_for_visit(settlement, speaker, state, 0)
                })
                .count()
        };
        let early = spoken(&state);
        state.found = 60;
        assert!(spoken(&state) > early);
        assert_eq!(
            spoken(&state),
            10,
            "every resident has a line once the trail is read"
        );
    }
}
