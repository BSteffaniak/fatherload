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
/// The Archive Post's settlement id (Wave 10 ancients-a).
pub const ARCHIVE_POST: VillageId = VillageId(0x3FFF_FC00);
/// Heart's Edge's settlement id (Wave 11 carryover: Descent VIII's outpost has people).
pub const HEARTS_EDGE: VillageId = VillageId(0x3FFF_FB00);

/// One of the deep settlements with residents.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeepSettlement {
    CathedralStation,
    AmberCourt,
    /// The Descent VII outpost in the Buried Megacity (Wave 10 ancients-a): the Lamplighters
    /// and the builders' patient machines.
    ArchivePost,
    /// Heart's Edge, Descent VIII's outpost over the Dark Matter Veins (Wave 11 carryover): the
    /// last camp above the Heart, kept by folk who came down after the Fathers and stayed with
    /// what Amos left behind.
    HeartsEdge,
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
    /// Their everyday talk about their own place (Wave 11 carryover), said in place of the
    /// people system's generic chatter; empty: the generic chatter (Cathedral Station's
    /// frontier folk) or none (the Hive's folk keep to their own words).
    pub chatter: &'static [&'static str],
}

const STATION_CAST: [Cast; 5] = [
    Cast {
        speaker: "quartermaster",
        given: "Hesper",
        family: "Quill",
        role: Role::Quartermaster,
        greeting: "Fuel's dear and patience is dearer. What do you need, driller?",
        chatter: &[],
    },
    Cast {
        speaker: "prospector",
        given: "Oskar",
        family: "Danner",
        role: Role::Prospector,
        greeting: "Pull up a crate. Nobody down here talks about anything but the Fathers \
                   and the next seam, and I'm out of seams.",
        chatter: &[],
    },
    Cast {
        speaker: "medic",
        given: "Lida",
        family: "Marr",
        role: Role::Healer,
        greeting: "Hold still. You've a cut you haven't noticed yet. They all do, this deep.",
        chatter: &[],
    },
    Cast {
        speaker: "surveyor",
        given: "Corwin",
        family: "Vale",
        role: Role::Cartographer,
        greeting: "Plumb to the cable, every mark. Calloway's rule. Mind my chalk lines.",
        chatter: &[],
    },
    Cast {
        speaker: "cook",
        given: "Mags",
        family: "Pell",
        role: Role::Innkeeper,
        greeting: "There's stew. There's always stew. Brannigan's recipe, near as I can guess \
                   it from the tin she left.",
        chatter: &[],
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
        chatter: &[],
    },
    Cast {
        speaker: "pump_tender",
        given: "Wrench-Thrice",
        family: "of the Nursery",
        role: Role::Mechanic,
        greeting: "Tk-tk. You smell of oil, like the Wrench-One did. Good smell. Pump smell.",
        chatter: &[],
    },
    Cast {
        speaker: "nurse",
        given: "Hum-Softly",
        family: "of the Brood",
        role: Role::Healer,
        greeting: "Hm-hm-hm. Quiet, soft-shell, the brood sleeps. Speak low and I will \
                   answer low.",
        chatter: &[],
    },
    Cast {
        speaker: "soldier",
        given: "Mandible",
        family: "of the Guard",
        role: Role::Warden,
        greeting: "Click. You may stand in the Court. You may not touch the larder. Click.",
        chatter: &[],
    },
    Cast {
        speaker: "queen",
        given: "Voice",
        family: "of the Queen",
        role: Role::ShrineKeeper,
        greeting: "The queen hears you through me. She says: little light, you came far. She \
                   says: so did the others.",
        chatter: &[],
    },
];

/// The Archive Post's residents (Wave 10 ancients-a): two Lamplighters, descendants of a crew
/// that came down long before the Fathers and stayed to keep the street lamps burning, and
/// three of the builders' machines who still keep the city's ways.
const POST_CAST: [Cast; 5] = [
    Cast {
        speaker: "keeper",
        given: "Keeper-Unit",
        family: "Nine",
        role: Role::Quartermaster,
        greeting: "Welcome to the Archive Post. Please sign the register. Fuel, repair and a \
                   bench: in that order, or any order. I am not particular.",
        chatter: &[
            "The register holds four hundred and twelve names. Eleven came back up to sign out. I keep the page open for the rest.",
            "Fuel is drawn from the city's old mains. The mains do not know the city is asleep. Please do not tell them.",
            "Repairs are free on a first visit. I have decided this just now. Policy is whatever I decide just now.",
        ],
    },
    Cast {
        speaker: "lamplighter",
        given: "Ada",
        family: "Lantry",
        role: Role::Storyteller,
        greeting: "Mind the wick. Every lamp you see burning, a Lantry lit. Sit; the oil's \
                   cheap and the stories are cheaper.",
        chatter: &[
            "Forty-one lamps on this avenue. I light them at what the town calls dusk. Nobody down here knows when dusk is but me.",
            "Gran said a lit street is a street somebody means to come back to. So we keep them lit.",
            "The trams still run their timetable, empty, every one. I wave at them anyway. It's manners.",
        ],
    },
    Cast {
        speaker: "tinker",
        given: "Pell",
        family: "Corran",
        role: Role::Mechanic,
        greeting: "Builder alloy, surface steel, it all welds if you sweet-talk it. What's \
                   rattling on that rig of yours?",
        chatter: &[
            "Builder bolts are threaded backwards. Took my da twenty years to find that out, and me twenty minutes, because he told me.",
            "Bring me a rivet hound's jaw and I'll make you a wrench that never slips. Bring me two and I'll make you a spare.",
            "That crusher line in the Engine Halls keeps better time than any clock up top. I set my watch by its slam.",
        ],
    },
    Cast {
        speaker: "sweeper",
        given: "Sweep",
        family: "of the Post",
        role: Role::Courier,
        greeting: "Sweep-sweep! Visitor! Sweep will tidy around you. Please lift your treads.",
        chatter: &[
            "Sweep-sweep! The floor is ninety-eight percent clean. The other two percent is you. Sweep is not complaining.",
            "Sweep found a button under the counter. Sweep keeps it. Sweep has three hundred and six buttons.",
            "Dust comes down from the city every day. Sweep sweeps it every day. This is called a career.",
        ],
    },
    Cast {
        speaker: "curator",
        given: "Curator",
        family: "Thessaly",
        role: Role::Scholar,
        greeting: "A reader. How rare. The Archive is below the halls; the charts are behind \
                   their locks. I can tell you how the locks think.",
        chatter: &[
            "The Archive holds a copy of your world, shelved by depth. Your town is on a high shelf, near the dusting.",
            "The vault locks are not puzzles. They are questions. A plate asks: are you patient? Glass asks: can you see?",
            "I have no age to tell you. I have a commissioning date, and it is rude to ask a machine its commissioning date.",
        ],
    },
];

/// Heart's Edge's residents (Wave 11 carryover): folk who followed the Fathers' trail down long
/// after them, found the builders' last lift station dark with Amos's plate on its door, and
/// stayed. They remember the Fathers by what they found: the folded sled, the letters in the
/// cairns, the crutch's marks in the rock, the chair by the cable root.
const EDGE_CAST: [Cast; 5] = [
    Cast {
        speaker: "keeper",
        given: "Bryn",
        family: "Ashcombe",
        role: Role::Quartermaster,
        greeting: "Heart's Edge. Last fuel, last bunk, last kind word before the bottom of the \
                   world. Take all three.",
        chatter: &[
            "Fuel comes down the lift twice a week, if the lift feels like it. The rest of the week I sell promises.",
            "Nobody's buried here. Folk go home, or they go down. Down's further than buried.",
            "Mind your weight going up: carry too much of the black ore and you fall up the shaft. Happened twice. Both laughed.",
        ],
    },
    Cast {
        speaker: "drifter",
        given: "Coll",
        family: "Fenn",
        role: Role::Courier,
        greeting: "Don't mind me. I'm only passing through. Been passing through eleven years.",
        chatter: &[
            "Wells to the east, the metal sea below them, the Heart under all. Everything else is walking between.",
            "I sell maps of places that move. They're out of date by the time you pay. That's the charm.",
            "Slept in a well once. Woke three feet from where I lay down, on the ceiling. Wouldn't recommend it.",
        ],
    },
    Cast {
        speaker: "listener",
        given: "Nan",
        family: "Orrin",
        role: Role::Storyteller,
        greeting: "Shh. Hear it? Slow as a sleeper. Sit, but sit quiet. I'm counting.",
        chatter: &[
            "The hum's not a sound, really. It's the floor agreeing with itself.",
            "Some nights it skips. We all sit up. Then it carries on, and we all pretend we weren't scared.",
            "If it ever wakes, I'd like to be awake for it. I've been practising.",
        ],
    },
    Cast {
        speaker: "railhand",
        given: "Ivo",
        family: "Strand",
        role: Role::Mechanic,
        greeting: "Mind the magnet strip, it'll have the fillings out of your teeth. Riding the \
                   rails down?",
        chatter: &[
            "The magnetic catch grabs you if you come down on a rail fast. Slow, and you go in the metal. So: fast.",
            "There's a booster at the top of every basin. Hold on to something. Your stomach, ideally.",
            "I'd swap a week's wages for one cold drink. Nothing's cold down here but the dark.",
        ],
    },
    Cast {
        speaker: "medic",
        given: "Elm",
        family: "Maddox",
        role: Role::Healer,
        greeting: "You're light on your feet. That's the black ore, not good health. Let me look \
                   at you anyway.",
        chatter: &[
            "Burns, mostly. The metal sea doesn't forgive and the rails don't apologise.",
            "Drink water. I know. Drink it anyway.",
            "Your hull's bruised and so are you. One of those I can fix.",
        ],
    },
];

/// Whether `character` of the Archive Post is one of the builders' machines (the keeper, the
/// sweeper and the curator): the game draws them as automatons.
#[must_use]
pub const fn is_automaton(id: VillageId, index: u16) -> bool {
    id.0 == ARCHIVE_POST.0 && matches!(index, 0 | 3 | 4)
}

impl DeepSettlement {
    pub const ALL: [Self; 4] = [
        Self::CathedralStation,
        Self::AmberCourt,
        Self::ArchivePost,
        Self::HeartsEdge,
    ];

    #[must_use]
    pub const fn id(self) -> VillageId {
        match self {
            Self::CathedralStation => CATHEDRAL_STATION,
            Self::AmberCourt => AMBER_COURT,
            Self::ArchivePost => ARCHIVE_POST,
            Self::HeartsEdge => HEARTS_EDGE,
        }
    }

    /// The settlement `id` names, if it is one of these.
    #[must_use]
    pub const fn of_id(id: VillageId) -> Option<Self> {
        match id.0 {
            value if value == CATHEDRAL_STATION.0 => Some(Self::CathedralStation),
            value if value == AMBER_COURT.0 => Some(Self::AmberCourt),
            value if value == ARCHIVE_POST.0 => Some(Self::ArchivePost),
            value if value == HEARTS_EDGE.0 => Some(Self::HeartsEdge),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CathedralStation => "Cathedral Station",
            Self::AmberCourt => "The Amber Court",
            Self::ArchivePost => "The Archive Post",
            Self::HeartsEdge => "Heart's Edge",
        }
    }

    /// The memories its people tell.
    #[must_use]
    pub const fn remembering(self) -> Remembering {
        match self {
            Self::CathedralStation => Remembering::OldDeepCamp,
            Self::AmberCourt => Remembering::TheHive,
            Self::ArchivePost => Remembering::ArchivePost,
            Self::HeartsEdge => Remembering::HeartCamp,
        }
    }

    /// Its authored residents, in resident order.
    #[must_use]
    pub const fn cast(self) -> &'static [Cast] {
        match self {
            Self::CathedralStation => &STATION_CAST,
            Self::AmberCourt => &COURT_CAST,
            Self::ArchivePost => &POST_CAST,
            Self::HeartsEdge => &EDGE_CAST,
        }
    }

    const fn culture(self) -> Culture {
        match self {
            Self::CathedralStation | Self::ArchivePost | Self::HeartsEdge => Culture::Frontier,
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

/// `character`'s everyday talk about their own place on their `visit`-th talk on `day` (a deep
/// settlement's resident with authored chatter, Wave 11 carryover), else `None`.
#[must_use]
pub fn chatter(character: &Character, day: u32, visit: u32) -> Option<&'static str> {
    let place = DeepSettlement::of_id(character.id.village)?;
    let lines = place.cast().get(usize::from(character.id.index))?.chatter;
    let count = u32::try_from(lines.len()).ok().filter(|count| *count > 0)?;
    lines
        .get(usize::try_from(day.wrapping_add(visit) % count).ok()?)
        .copied()
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
            // The Court's hall, clear of its doors (its floor is the last two rows; the comb
            // pillars are worn through at the floor, so its people walk bay to bay).
            let court = living::court_hall(settings, &layout)?;
            Some((court.max.y - 1, court.min.x + 6, court.max.x - 6))
        }
        DeepSettlement::ArchivePost | DeepSettlement::HeartsEdge => {
            // Descent VII's and VIII's outpost halls, like Cathedral Station: the half away
            // from the lift's door (Wave 10 ancients-a; Heart's Edge, Wave 11 carryover).
            let descent = if place == DeepSettlement::ArchivePost {
                Descent::Ancients
            } else {
                Descent::Heart
            };
            let hall = landmarks::outpost(settings, &layout, descent)?.bounds;
            let middle = i32::midpoint(hall.min.x, hall.max.x);
            if landmarks::lift_column(settings) > middle {
                Some((hall.max.y, hall.min.x + 3, middle - 2))
            } else {
                Some((hall.max.y, middle + 2, hall.max.x - 3))
            }
        }
    }
}

/// `place`'s routine view in a world with `settings`.
#[must_use]
pub fn settlement(settings: &WorldSettings, place: DeepSettlement) -> Option<Settlement> {
    let (ground, first, last) = hall(settings, place)?;
    // The plaza (where they lunch, stroll and sleep) spans the hall, so they spread out.
    Some(Settlement::deep_hall(
        place.id(),
        ground,
        (first, last),
        (first + 1, last - 1),
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
            vec![CATHEDRAL_STATION, AMBER_COURT, ARCHIVE_POST, HEARTS_EDGE]
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
            // At work they spread along the hall instead of standing in one heap.
            let mut columns: Vec<i32> = (0..settlement.residents.len())
                .filter_map(|index| settlement.stop_at(index, DayTime::at(1, 10, 0)))
                .map(|stop| settlement.place_column(stop.place))
                .collect();
            columns.sort_unstable();
            columns.dedup();
            assert!(
                columns.len() >= 4
                    && columns.last().copied().unwrap_or(0) - columns[0]
                        >= (settlement.last_column - settlement.first_column) / 2,
                "{} works in a heap: {columns:?}",
                place.name()
            );
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
            20,
            "every resident has a line once the trail is read"
        );
    }
}
