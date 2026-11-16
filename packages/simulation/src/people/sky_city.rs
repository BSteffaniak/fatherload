//! The people of the sky cities (Wave 4 T21).
//!
//! Every V9 sky continent's city
//! ([`crate::worldgen::sky::continent::SkyCity`]) is a settlement whose residents live by the
//! same routines as village folk (talk, trade, gifts, memories), with sky trades: aeronauts
//! (mechanics), cloud monks (shrine keepers), cloud merchants (traders), smugglers (couriers),
//! storm-chasers (cartographers), innkeepers, tinkerers, scholars and wardens.
//!
//! Sky city ids carry [`SKY_CITY_FLAG`](crate::worldgen::sky::continent::SKY_CITY_FLAG) so
//! they never name a village or a camp; [`super::routine::settlement`] builds them.
//!
//! **Seam for T20** (sky towns and sky cultures): the residents use the existing
//! [`Culture::Highland`] names and existing roles standing in for sky occupations; when T20's
//! sky cultures and occupations land, [`sky_city_population`] is the one place to switch them.

use crate::{
    village::BuildingKind,
    worldgen::sky::continent::{SkyBuildingKind, SkyCity},
};

use super::{Culture, PeopleRng, Population, Role, generate_population};

const SALT_SKY_FOLK: u64 = 0x5045_4F50_4C45_5C1E;

/// The trades of a sky city's folk, in the order residents take them (the first few are in
/// every city).
const SKY_TRADES: [Role; 9] = [
    Role::Mechanic,
    Role::ShrineKeeper,
    Role::Trader,
    Role::Courier,
    Role::Cartographer,
    Role::Innkeeper,
    Role::Tinkerer,
    Role::Scholar,
    Role::Warden,
];

/// What a sky trade is called up there (presentation; the role keeps its offers).
#[must_use]
pub const fn sky_trade_name(role: Role) -> Option<&'static str> {
    Some(match role {
        Role::Mechanic => "aeronaut",
        Role::ShrineKeeper => "cloud monk",
        Role::Trader => "cloud merchant",
        Role::Courier => "smuggler",
        Role::Cartographer => "storm-chaser",
        _ => return None,
    })
}

/// The routine lot kind of a sky building (people work at counters by their kind).
#[must_use]
pub const fn lot_kind(kind: SkyBuildingKind) -> BuildingKind {
    match kind {
        SkyBuildingKind::Tower => BuildingKind::Tower,
        SkyBuildingKind::Hall => BuildingKind::Hall,
        SkyBuildingKind::Market => BuildingKind::Market,
        SkyBuildingKind::Monastery => BuildingKind::Shrine,
        SkyBuildingKind::Hangar => BuildingKind::Workshop,
        SkyBuildingKind::Home => BuildingKind::House,
    }
}

/// The folk of `city`: [`SkyCity::population`] residents in sky trades, named for the city
/// ("Eldermoor Skyhold").
#[must_use]
pub fn sky_city_population(seed: u64, city: &SkyCity) -> Population {
    let mut population = generate_population(seed, city.id, Culture::Highland, city.population());
    let mut rng = PeopleRng::new(seed, SALT_SKY_FOLK, u64::from(city.id.0), 1);
    for (index, character) in population.characters.iter_mut().enumerate() {
        let role = SKY_TRADES[index % SKY_TRADES.len()];
        if character.role != role {
            character.role = role;
            let (youngest, oldest) = role.ages();
            character.age = character.age.clamp(youngest, oldest);
            character.offer = role.offer();
            character.history = super::character::history(seed, character);
        }
    }
    let suffix = *rng.pick(&["Skyhold", "Aerie", "Cloudport", "Highwind"]);
    population.village_name = format!("{} {suffix}", population.village_name);
    population
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        people::routine,
        worldgen::{GeneratorVersion, WorldSettings, sky::continent},
    };

    #[test]
    fn sky_cities_are_settlements_of_sky_folk() {
        let settings = WorldSettings {
            generator: GeneratorVersion::V9,
            ..WorldSettings::new(0x5EED)
        };
        let cities = continent::sky_cities_between(&settings, -8_000, 8_000);
        assert!(!cities.is_empty());
        for city in cities {
            let settlement = routine::settlement(&settings, city.id).expect("a settlement");
            assert_eq!(settlement.ground_row, city.street_row);
            assert_eq!(settlement.residents.len(), city.population());
            assert_eq!(settlement.lots.len(), city.buildings.len());
            let population = &settlement.population;
            assert_eq!(population.characters[0].role, Role::Mechanic);
            assert!(population.village_name.contains(' '));
            for character in &population.characters {
                assert_eq!(character.id.village, city.id);
                let back = routine::character(&settings, character.id).expect("found");
                assert_eq!(back.name, character.name);
            }
        }
        assert_eq!(sky_trade_name(Role::ShrineKeeper), Some("cloud monk"));
    }
}
