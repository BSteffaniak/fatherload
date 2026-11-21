//! The colonists of the Moon's research station (Wave 6).
//!
//! On a filled Moon ([`crate::worldgen::GeneratorVersion::has_moon_content`]) the colony
//! ([`crate::worldgen::moon::MoonLayout::colony`]) is a settlement like a village: its
//! colonists keep routines (they sleep in the hab module, work in the dome and on the pad, and
//! gather under the dome at lunch and after shift), talk, trade, take gifts and remember
//! players, and give and take part in stories (the Moon's story place binds to the colony, so
//! a [`crate::quest::Objective::Speak`] beat there finds a colonist).
//!
//! They are of the [`Culture::Lunar`] culture and roll from [`Role::LUNAR_POOL`] (the lunar
//! occupations: hydroponists, dome techs, selenographers and quartermasters, beside the trades
//! a station still needs). The colony's id is [`LUNAR_COLONY`], which never names a village, a
//! camp, a sky town or a sky city.

use super::{
    Culture, Population, Role, VillageId, generate_population,
    routine::{Lot, Settlement},
};
use crate::{village::BuildingKind, worldgen::WorldSettings};

/// The colony's settlement id (an even id far past every surface site index, with neither the
/// sky flag nor the camp flag set).
pub const LUNAR_COLONY: VillageId = VillageId(0x3FFF_FF00);
/// How many colonists live at the station.
pub const COLONY_FOLK: usize = 6;
/// What the colonists call the station when the layout gives it no name of its own.
pub const COLONY_NAME: &str = "Tranquility Station";

/// The trades the colonists take, in resident order (the first few are at every station).
const COLONY_TRADES: [Role; COLONY_FOLK] = [
    Role::Quartermaster,
    Role::Hydroponist,
    Role::DomeTech,
    Role::Selenographer,
    Role::Healer,
    Role::Mechanic,
];

/// Whether `id` is the Moon colony's settlement.
#[must_use]
pub const fn is_colony(id: VillageId) -> bool {
    id.0 == LUNAR_COLONY.0
}

/// The colony's settlement id on a world whose Moon has a colony (and people), else `None`.
#[must_use]
pub fn colony_settlement_id(settings: &WorldSettings) -> Option<VillageId> {
    (settings.generator.has_villages()
        && settings.generator.has_moon_content()
        && crate::worldgen::moon::layout(settings).is_some())
    .then_some(LUNAR_COLONY)
}

/// The colonists: [`COLONY_FOLK`] lunar residents in [`COLONY_TRADES`].
#[must_use]
pub fn colony_population(seed: u64) -> Population {
    let mut population = generate_population(seed, LUNAR_COLONY, Culture::Lunar, COLONY_FOLK);
    for (index, character) in population.characters.iter_mut().enumerate() {
        let role = COLONY_TRADES[index % COLONY_TRADES.len()];
        if character.role != role {
            character.role = role;
            let (youngest, oldest) = role.ages();
            character.age = character.age.clamp(youngest, oldest);
            character.offer = role.offer();
            character.history = super::character::history(seed, character);
        }
    }
    COLONY_NAME.clone_into(&mut population.village_name);
    population
}

/// The colony's routine view on a world with a colony: the dome (people work and gather
/// under it), the hab module (where they sleep), the hydroponics bench and the stores by the
/// pad.
#[must_use]
pub fn colony_settlement(settings: &WorldSettings) -> Option<Settlement> {
    colony_settlement_id(settings)?;
    let layout = crate::worldgen::moon::layout(settings)?;
    let cx = layout.colony.center_x;
    let ground = layout.colony.ground_row;
    let dome = crate::worldgen::moon::DOME_RADIUS;
    let (pad_first, _, _) = layout.pad();
    let lots = vec![
        Lot {
            kind: BuildingKind::Hall,
            first_column: cx - dome + 3,
            width: 2 * dome - 5,
            enterable: false,
        },
        Lot {
            kind: BuildingKind::House,
            first_column: cx + 17,
            width: 13,
            enterable: true,
        },
        Lot {
            kind: BuildingKind::Farm,
            first_column: cx - 8,
            width: 4,
            enterable: false,
        },
        Lot {
            kind: BuildingKind::Depot,
            first_column: pad_first + 12,
            width: 5,
            enterable: false,
        },
    ];
    let population = colony_population(settings.seed);
    Some(Settlement::colony(
        LUNAR_COLONY,
        ground,
        (pad_first + 10, cx + 29),
        (cx - 4, cx + 4),
        lots,
        1,
        &population,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        people::{clock::DayTime, routine},
        worldgen::GeneratorVersion,
    };

    #[test]
    fn the_colony_is_a_settlement_of_lunar_folk_on_filled_moons_only() {
        let settings = WorldSettings::new(0x00C0_1017);
        assert!(settings.generator.has_moon_content());
        let settlement = routine::settlement(&settings, LUNAR_COLONY).expect("the colony");
        assert_eq!(settlement.residents.len(), COLONY_FOLK);
        assert_eq!(settlement.population.village_name, COLONY_NAME);
        let layout = crate::worldgen::moon::layout(&settings).expect("a filled Moon");
        assert_eq!(settlement.ground_row, layout.colony.ground_row);
        for (index, character) in settlement.population.characters.iter().enumerate() {
            assert_eq!(character.culture, Culture::Lunar);
            assert_eq!(character.role, COLONY_TRADES[index]);
            assert_eq!(character.id.village, LUNAR_COLONY);
            let back = routine::character(&settings, character.id).expect("found again");
            assert_eq!(back.name, character.name);
        }
        // Routines: asleep in the hab at night, out and about in the colony by day.
        let night = settlement.stop_at(0, DayTime::at(1, 2, 0)).expect("a stop");
        assert_eq!(night.place, routine::Place::Inside(1));
        for index in 0..COLONY_FOLK {
            let day = settlement
                .stop_at(index, DayTime::at(1, 11, 0))
                .expect("a stop");
            let column = settlement.place_column(day.place);
            assert!(
                (settlement.first_column..=settlement.last_column).contains(&column),
                "colonist {index} wanders off to {column}"
            );
        }
        // Never a village, a camp, a sky town or a sky city.
        assert!(crate::village::village(&settings, LUNAR_COLONY).is_none());
        assert!(!crate::people::camps::is_camp(LUNAR_COLONY));
        assert!(!crate::worldgen::sky::continent::is_sky_city(LUNAR_COLONY));
        assert!(crate::village::placement::decode_sky_town(LUNAR_COLONY).is_none());
        // Worlds without a filled Moon have no colonists.
        let older = WorldSettings {
            generator: GeneratorVersion::V8,
            ..settings
        };
        assert!(colony_settlement_id(&older).is_none());
        assert!(routine::settlement(&older, LUNAR_COLONY).is_none());
    }
}
