//! Daily routines (WP-PEOPLE): where each resident of a settlement should be at a given time.
//!
//! A [`Settlement`] is the routine's view of a village (or the town): its ground, plaza and
//! building lots, and for each resident a home, a workplace and a few habits rolled from their
//! personality. [`Settlement::stop_at`] is a pure function of the resident and the clock
//! ([`DayTime`]): home and asleep at night, out by their door at dawn, at work by day (keepers
//! behind their counters, everyone else at a spot that fits their trade), on the plaza at
//! lunch, strolling after work, at the inn in the evening (homebodies go home instead, night
//! owls stay late), and a rest day every seventh day. The world (`world/people.rs`) walks NPC
//! entities toward their stop, so the same seed and the same time always put the same people in
//! the same places, on every peer.

use std::sync::Arc;

use crate::{
    surface::SURFACE_BUILDINGS,
    village::{BuildingKind, Village, VillageId},
    worldgen::{WorldSettings, streamed::cache::MemoCache},
};

use super::{
    Character, CharacterId, PeopleRng, Population, Role, clock::DayTime, generate_population,
    names::Culture,
};

/// The town's settlement id (village id 0 is never a village site: site 0 is the town).
pub const TOWN_VILLAGE: VillageId = VillageId(0);
/// What the town's folk call it.
pub const TOWN_NAME: &str = "Fatherload";
/// How many town folk walk the town's street.
pub const TOWN_FOLK: usize = 7;
/// Every this many days is a rest day: nobody works.
pub const REST_DAY_EVERY: u32 = 7;
/// Salt for routine habit rolls.
const SALT_ROUTINE: u64 = 0x5045_4F50_4C45_0010;

/// One building lot as routines see it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lot {
    pub kind: BuildingKind,
    pub first_column: i32,
    pub width: i32,
    /// Whether people can go inside (hidden from the street until they come out).
    pub enterable: bool,
}

impl Lot {
    /// The column of the lot's door (its middle).
    #[must_use]
    pub const fn door_column(&self) -> i32 {
        self.first_column + self.width / 2
    }
}

/// Where a resident works.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Work {
    /// Behind the counter of lot `0` (indoors).
    Counter(usize),
    /// Outdoors around a column (a farm, a mine head, a workshop door, the market).
    Outdoors(i32),
    /// Walking the whole street (couriers, wardens).
    Roam,
}

/// One resident's routine data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resident {
    pub character: CharacterId,
    /// The lot they live in (`None`: they sleep at the inn or the hall).
    pub home: Option<usize>,
    pub work: Work,
    /// Minutes their day runs early (negative) or late (positive), `-45..=45`.
    pub offset: i16,
    /// Skips the inn and goes home after work.
    pub homebody: bool,
    /// Stays at the inn until late.
    pub night_owl: bool,
    /// Walking pace in hundredths of a tile per second.
    pub pace: u16,
}

/// What a resident is doing.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum Activity {
    Sleeping,
    /// Up early, pottering by the door.
    Morning,
    Working,
    Lunch,
    /// After work: a stroll down the street.
    Unwinding,
    AtInn,
    /// A rest day: the plaza and the street.
    Resting,
}

impl Activity {
    /// A short phrase for UI ("at work").
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        match self {
            Self::Sleeping => "asleep",
            Self::Morning => "up early",
            Self::Working => "at work",
            Self::Lunch => "on a lunch break",
            Self::Unwinding => "out for a stroll",
            Self::AtInn => "at the inn",
            Self::Resting => "taking the day off",
        }
    }
}

/// Where a resident should be.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Place {
    /// Inside lot `0` (hidden from the street).
    Inside(usize),
    /// Outdoors around `column`, wandering up to `roam` columns either side.
    Outside { column: i32, roam: u8 },
}

/// A resident's routine stop at some time: the activity and its place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stop {
    pub activity: Activity,
    pub place: Place,
}

/// A village or the town, as routines see it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settlement {
    pub village: VillageId,
    /// The first solid row under the street (people stand on it).
    pub ground_row: i32,
    pub first_column: i32,
    pub last_column: i32,
    pub plaza: (i32, i32),
    pub lots: Vec<Lot>,
    /// One per resident, in resident index order.
    pub residents: Vec<Resident>,
    /// The inn's lot, if there is one.
    pub inn: Option<usize>,
    /// Whether this is an underground settlement (a carved hall).
    pub underground: bool,
    /// The residents themselves.
    pub population: Population,
}

impl Settlement {
    /// The routine view of `village`.
    #[must_use]
    pub fn of_village(village: &Village) -> Self {
        let lots: Vec<Lot> = village
            .buildings
            .iter()
            .map(|building| Lot {
                kind: building.kind,
                first_column: building.first_column,
                width: building.width,
                enterable: building.enterable(),
            })
            .collect();
        let homes: Vec<Option<usize>> = (0..village.population.characters.len())
            .map(|index| {
                let resident = u16::try_from(index).unwrap_or(u16::MAX);
                village
                    .buildings
                    .iter()
                    .position(|building| building.residents.contains(&resident))
            })
            .collect();
        let keepers: Vec<Option<usize>> = (0..village.population.characters.len())
            .map(|index| {
                let resident = u16::try_from(index).unwrap_or(u16::MAX);
                village
                    .buildings
                    .iter()
                    .position(|building| building.keeper == Some(resident))
            })
            .collect();
        Self::assemble(
            village.id,
            village.ground_row,
            (village.first_column, village.last_column),
            village.plaza,
            lots,
            &village.population,
            &homes,
            &keepers,
            village.setting == crate::village::VillageSetting::Underground,
        )
    }

    /// The town's routine view: its eight service buildings on the flat street, and
    /// [`TOWN_FOLK`] residents of [`town_population`] who live above the shops.
    #[must_use]
    pub fn town(population: &Population, ground_row: i32) -> Self {
        let lots: Vec<Lot> = SURFACE_BUILDINGS
            .iter()
            .map(|building| Lot {
                kind: town_kind(building.zone),
                first_column: building.tile_x,
                width: building.tile_width,
                enterable: true,
            })
            .collect();
        let (first, last) = crate::surface::town_span();
        // The plaza is the open ground around HQ and the shop.
        let plaza = (SURFACE_BUILDINGS[3].tile_x, SURFACE_BUILDINGS[4].tile_x + 7);
        let homes: Vec<Option<usize>> = (0..population.characters.len())
            .map(|index| Some((index * 3 + 1) % lots.len()))
            .collect();
        let keepers = vec![None; population.characters.len()];
        Self::assemble(
            TOWN_VILLAGE,
            ground_row,
            (first, last),
            plaza,
            lots,
            population,
            &homes,
            &keepers,
            false,
        )
    }

    /// A wanderers' camp's routine view ([`super::camps`]): no buildings, so its people sleep,
    /// work and gather around the campfire.
    #[must_use]
    pub fn camp(camp: &super::camps::Camp, population: &Population) -> Self {
        let (first, last) = camp.span();
        let homes = vec![None; population.characters.len()];
        let keepers = vec![None; population.characters.len()];
        Self::assemble(
            camp.id,
            camp.ground_row,
            (first, last),
            (camp.column - 2, camp.column + 2),
            Vec::new(),
            population,
            &homes,
            &keepers,
            false,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "one internal assembly step")]
    fn assemble(
        village: VillageId,
        ground_row: i32,
        span: (i32, i32),
        plaza: (i32, i32),
        lots: Vec<Lot>,
        population: &Population,
        homes: &[Option<usize>],
        keepers: &[Option<usize>],
        underground: bool,
    ) -> Self {
        let inn = lots.iter().position(|lot| lot.kind == BuildingKind::Inn);
        let residents = population
            .characters
            .iter()
            .enumerate()
            .map(|(index, character)| {
                let work = keepers.get(index).copied().flatten().map_or_else(
                    || outdoor_work(character, &lots, plaza, index),
                    Work::Counter,
                );
                habits(
                    character,
                    homes.get(index).copied().flatten(),
                    work,
                    population.village,
                )
            })
            .collect();
        Self {
            village,
            ground_row,
            first_column: span.0,
            last_column: span.1,
            plaza,
            lots,
            residents,
            inn,
            underground,
            population: population.clone(),
        }
    }

    /// The centre column.
    #[must_use]
    pub const fn center_column(&self) -> i32 {
        i32::midpoint(self.first_column, self.last_column)
    }

    /// Where resident `index` should be at `time` (`None` for an unknown resident).
    #[must_use]
    pub fn stop_at(&self, index: usize, time: DayTime) -> Option<Stop> {
        let resident = self.residents.get(index)?;
        Some(self.stop_for(resident, index, time))
    }

    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "clock minutes are tiny"
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "the whole day's schedule in one place"
    )]
    fn stop_for(&self, resident: &Resident, index: usize, time: DayTime) -> Stop {
        const fn hm(hour: i32, minute: i32) -> i32 {
            hour * 60 + minute
        }
        let minute = (i32::from(time.minute) - i32::from(resident.offset)).rem_euclid(24 * 60);
        let innkeeper = matches!(resident.work, Work::Counter(lot) if Some(lot) == self.inn);
        let rest_day = time.day % REST_DAY_EVERY == REST_DAY_EVERY - 1 && !innkeeper;
        let wake = hm(6, 0);
        let bedtime = if resident.night_owl || innkeeper {
            hm(23, 40)
        } else {
            hm(22, 20)
        };
        let activity = if minute < wake || minute >= bedtime {
            Activity::Sleeping
        } else if innkeeper {
            if minute < hm(9, 0) {
                Activity::Morning
            } else {
                Activity::Working
            }
        } else if minute
            < if matches!(resident.work, Work::Counter(_)) {
                hm(7, 0)
            } else {
                hm(7, 30)
            }
        {
            Activity::Morning
        } else if minute >= hm(19, 0) {
            if resident.homebody {
                Activity::Sleeping
            } else {
                Activity::AtInn
            }
        } else if rest_day {
            Activity::Resting
        } else if minute < hm(12, 0) || (hm(13, 0)..hm(17, 30)).contains(&minute) {
            Activity::Working
        } else if minute < hm(13, 0) {
            Activity::Lunch
        } else {
            Activity::Unwinding
        };
        let plaza_spot = |spread: i32| {
            let width = (self.plaza.1 - self.plaza.0).max(1);
            let slot = i32::try_from(index).unwrap_or(0) * 5 % (width + 1);
            Place::Outside {
                column: self.plaza.0 + slot,
                roam: u8::try_from(spread.clamp(1, 12)).unwrap_or(1),
            }
        };
        let home_place = |fallback_inside: bool| -> Place {
            resident
                .home
                .or(self.inn)
                .filter(|lot| self.lots.get(*lot).is_some_and(|lot| lot.enterable))
                .map_or_else(
                    || {
                        if fallback_inside {
                            plaza_spot(1)
                        } else {
                            plaza_spot(3)
                        }
                    },
                    Place::Inside,
                )
        };
        let place = match activity {
            Activity::Sleeping => home_place(true),
            Activity::Morning => resident
                .home
                .and_then(|lot| self.lots.get(lot))
                .map_or_else(
                    || plaza_spot(4),
                    |lot| Place::Outside {
                        column: lot.door_column() + if index.is_multiple_of(2) { 1 } else { -1 },
                        roam: 2,
                    },
                ),
            Activity::Working => match resident.work {
                Work::Counter(lot) => Place::Inside(lot),
                Work::Outdoors(column) => Place::Outside { column, roam: 2 },
                Work::Roam => Place::Outside {
                    column: self.center_column(),
                    roam: u8::try_from(
                        ((self.last_column - self.first_column) / 2 - 4).clamp(4, 60),
                    )
                    .unwrap_or(30),
                },
            },
            Activity::Lunch => plaza_spot((self.plaza.1 - self.plaza.0) / 2),
            Activity::Unwinding | Activity::Resting => {
                let half = (self.last_column - self.first_column) / 2;
                let column = if index.is_multiple_of(3) {
                    i32::midpoint(self.plaza.0, self.plaza.1)
                } else {
                    self.center_column() + (i32::try_from(index).unwrap_or(0) % 5 - 2) * half / 4
                };
                Place::Outside {
                    column,
                    roam: u8::try_from((half / 3).clamp(3, 20)).unwrap_or(6),
                }
            }
            Activity::AtInn => self.inn.map_or_else(
                || plaza_spot((self.plaza.1 - self.plaza.0) / 2),
                Place::Inside,
            ),
        };
        Stop { activity, place }
    }

    /// The column a place's people walk to (a lot's door, or the place's column).
    #[must_use]
    pub fn place_column(&self, place: Place) -> i32 {
        match place {
            Place::Inside(lot) => self
                .lots
                .get(lot)
                .map_or_else(|| self.center_column(), Lot::door_column),
            Place::Outside { column, .. } => column.clamp(self.first_column, self.last_column),
        }
    }
}

/// The town building a town zone stands in for (routines only need a kind).
const fn town_kind(zone: crate::economy::SurfaceZone) -> BuildingKind {
    use crate::economy::SurfaceZone;
    match zone {
        SurfaceZone::Fuel => BuildingKind::Depot,
        SurfaceZone::Repair | SurfaceZone::Salvage => BuildingKind::Workshop,
        SurfaceZone::Depot | SurfaceZone::Shop => BuildingKind::Market,
        SurfaceZone::Headquarters | SurfaceZone::Bank => BuildingKind::Hall,
        SurfaceZone::Explosives => BuildingKind::Smithy,
    }
}

/// Where a non-keeper works, from their trade and the lots there are.
fn outdoor_work(character: &Character, lots: &[Lot], plaza: (i32, i32), index: usize) -> Work {
    let kinds: &[BuildingKind] = match character.role {
        Role::Farmer => &[BuildingKind::Farm],
        Role::Prospector | Role::RetiredMiner => &[BuildingKind::MineHead, BuildingKind::Assay],
        Role::Smith | Role::Mechanic | Role::Tinkerer | Role::Apprentice => &[
            BuildingKind::Smithy,
            BuildingKind::Workshop,
            BuildingKind::Depot,
        ],
        Role::Trader | Role::Baker => &[BuildingKind::Market],
        Role::Healer | Role::ShrineKeeper | Role::Hermit => &[BuildingKind::Shrine],
        Role::Scholar | Role::Cartographer => &[BuildingKind::MapRoom, BuildingKind::Hall],
        Role::Innkeeper | Role::Brewer | Role::Storyteller => &[BuildingKind::Inn],
        Role::Warden => &[BuildingKind::Tower],
        Role::Courier => return Work::Roam,
    };
    let jitter = i32::try_from(index % 3).unwrap_or(0) - 1;
    kinds
        .iter()
        .find_map(|kind| lots.iter().find(|lot| lot.kind == *kind))
        .map_or_else(
            || {
                if character.role == Role::Warden {
                    Work::Roam
                } else {
                    Work::Outdoors(i32::midpoint(plaza.0, plaza.1) + jitter * 3)
                }
            },
            |lot| {
                let side = if index.is_multiple_of(2) { 1 } else { -1 };
                let column = if lot.enterable {
                    lot.door_column() + side * (lot.width / 2 + 1)
                } else {
                    lot.door_column() + jitter
                };
                Work::Outdoors(column)
            },
        )
}

/// A resident's habits, rolled from their personality and age.
fn habits(character: &Character, home: Option<usize>, work: Work, village: VillageId) -> Resident {
    let mut rng = PeopleRng::new(
        character.voice_seed,
        SALT_ROUTINE,
        u64::from(village.0),
        u64::from(character.id.index),
    );
    let p = character.personality;
    // Conscientious people run early, easygoing ones late.
    let lean = (i32::from(50_u8.saturating_sub(p.conscientiousness))
        - i32::from(p.conscientiousness.saturating_sub(50)))
        / 3;
    let offset = (rng.range(-25, 25) + lean).clamp(-45, 45);
    let homebody = p.extraversion < 30 || character.role == Role::Hermit;
    let night_owl = !homebody && (p.extraversion > 70 || rng.chance(15));
    let base = if character.age >= 65 {
        110
    } else if character.age < 20 {
        185
    } else {
        150
    };
    let brisk = i32::from(p.conscientiousness) / 4 - 12;
    Resident {
        character: character.id,
        home,
        work,
        offset: i16::try_from(offset).unwrap_or(0),
        homebody,
        night_owl,
        pace: u16::try_from((base + brisk + rng.range(-10, 10)).clamp(90, 220)).unwrap_or(150),
    }
}

/// The settlement `id` of a world with `settings`, memoized per process like villages.
///
/// A village (worlds with villages), the town ([`TOWN_VILLAGE`], on the same worlds) or a
/// wanderers' camp ([`super::camps`]).
#[must_use]
pub fn settlement(settings: &WorldSettings, id: VillageId) -> Option<Arc<Settlement>> {
    if !settings.generator.has_villages() {
        return None;
    }
    let key = (*settings, id.0.cast_signed(), 0);
    let built = SETTLEMENTS.get_or_compute(key, || {
        if super::camps::is_camp(id) {
            return super::camps::camp(settings, id).map(|camp| {
                let population = super::camps::camp_population(settings.seed, &camp);
                Arc::new(Settlement::camp(&camp, &population))
            });
        }
        if id == TOWN_VILLAGE {
            let population = town_population(settings.seed);
            let ground = crate::worldgen::surface_row(
                settings,
                crate::village::placement::TOWN_CENTER_COLUMN,
            );
            return Some(Arc::new(Settlement::town(&population, ground)));
        }
        crate::village::village(settings, id).map(|found| Arc::new(Settlement::of_village(&found)))
    });
    built.as_ref().clone()
}

/// The character `id` of a world with `settings` (a village resident or one of the town folk).
#[must_use]
pub fn character(settings: &WorldSettings, id: CharacterId) -> Option<Character> {
    settlement(settings, id.village)?
        .population
        .characters
        .get(usize::from(id.index))
        .cloned()
}

static SETTLEMENTS: MemoCache<Option<Arc<Settlement>>> = MemoCache::new(64);

/// The town's folk: [`TOWN_FOLK`] frontier residents of [`TOWN_VILLAGE`] named for
/// [`TOWN_NAME`].
#[must_use]
pub fn town_population(seed: u64) -> Population {
    let mut population = generate_population(seed, TOWN_VILLAGE, Culture::Frontier, TOWN_FOLK);
    TOWN_NAME.clone_into(&mut population.village_name);
    population
}

#[cfg(test)]
mod tests {
    use super::{Activity, Place, Settlement, town_population};
    use crate::{
        people::clock::DayTime,
        village::{self, placement},
        worldgen::{GeneratorVersion, WorldSettings},
    };

    fn sample() -> std::sync::Arc<village::Village> {
        let settings = WorldSettings {
            generator: GeneratorVersion::V7,
            ..WorldSettings::default()
        };
        village::village(&settings, placement::surface_id(1)).expect("first village")
    }

    #[test]
    fn residents_sleep_at_home_work_by_day_and_gather_in_the_evening() {
        let found = sample();
        let settlement = Settlement::of_village(&found);
        assert_eq!(
            settlement.residents.len(),
            found.population.characters.len()
        );
        for index in 0..settlement.residents.len() {
            let night = settlement
                .stop_at(index, DayTime::at(2, 2, 30))
                .expect("stop");
            assert_eq!(night.activity, Activity::Sleeping);
            assert!(matches!(night.place, Place::Inside(_)), "{night:?}");
            let noon = settlement
                .stop_at(index, DayTime::at(2, 10, 0))
                .expect("stop");
            assert_eq!(noon.activity, Activity::Working, "resident {index}");
        }
        let evening: Vec<Activity> = (0..settlement.residents.len())
            .filter_map(|index| settlement.stop_at(index, DayTime::at(2, 20, 30)))
            .map(|stop| stop.activity)
            .collect();
        assert!(
            evening.contains(&Activity::AtInn),
            "somebody goes to the inn: {evening:?}"
        );
        let lunch = (0..settlement.residents.len())
            .filter_map(|index| settlement.stop_at(index, DayTime::at(2, 12, 30)))
            .filter(|stop| stop.activity == Activity::Lunch)
            .count();
        assert!(
            lunch * 2 >= settlement.residents.len(),
            "most lunch on the plaza"
        );
    }

    #[test]
    fn rest_days_and_places_stay_in_the_village() {
        let found = sample();
        let settlement = Settlement::of_village(&found);
        for day in 0..14 {
            for hour in 0..24 {
                for index in 0..settlement.residents.len() {
                    let stop = settlement
                        .stop_at(index, DayTime::at(day, hour, 15))
                        .expect("stop");
                    let column = settlement.place_column(stop.place);
                    assert!(
                        (settlement.first_column..=settlement.last_column).contains(&column),
                        "{stop:?}"
                    );
                    if let Place::Inside(lot) = stop.place {
                        assert!(settlement.lots[lot].enterable);
                    }
                }
            }
        }
        let rest = (0..settlement.residents.len())
            .filter_map(|index| settlement.stop_at(index, DayTime::at(6, 10, 0)))
            .filter(|stop| stop.activity == Activity::Resting)
            .count();
        assert!(rest > 0, "day 6 is a rest day");
    }

    #[test]
    fn town_folk_have_homes_on_the_street() {
        let population = town_population(9);
        assert_eq!(population.characters.len(), super::TOWN_FOLK);
        let settlement = Settlement::town(&population, 6);
        for index in 0..settlement.residents.len() {
            let stop = settlement
                .stop_at(index, DayTime::at(0, 1, 0))
                .expect("stop");
            assert!(matches!(stop.place, Place::Inside(_)));
        }
    }
}
