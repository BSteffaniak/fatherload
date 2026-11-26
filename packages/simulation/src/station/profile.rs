//! Who runs a station and what it offers: a pure function of the world seed and the space
//! generator's station (its id, [`StationKind`] and system).
//!
//! | Owner | Shipyard | Planet gear | Alien tech | Market | Bounties | Bar | Ore rate |
//! |---|---|---|---|---|---|---|---|
//! | Halcyon Dock (home) | lines to tier 2 | tier 1 | - | yes | starter board | yes | 100 % |
//! | human trade post | tier 1 | tier 1 | - | yes, more goods | yes | yes | 125 % |
//! | human shipyard | every tier (5) | tier 2 | - | yes | yes | yes | 105 % |
//! | human outpost | tier 3 | tier 2 | - | yes | yes, more pay | yes | 115 % |
//! | human relay | - | tier 1 | - | yes | yes | yes (more rumours) | 110 % |
//! | alien-run | - (a shipyard: tier 3) | tier 2 | yes | alien goods | outposts only | yes | 135 % |
//! | derelict | - | - | - | - | - | - | salvage once |
//!
//! Relays are derelict about half the time; any other station about one in eight; alien-run
//! about three in ten. The home system's first station is always Halcyon Dock.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use serde::{Deserialize, Serialize};

use super::{StationId, StationKind, station_hash};
use crate::{
    flight::{
        BodyRef, GeneratedSpace,
        space::{STATION_INDEX, SystemRef as FlightSystemRef},
    },
    space::{SpaceSector, StarSystem, Station},
    star_chart::StarClass,
};

const SALT_OWNER: u64 = 0x57A7_0001;
const SALT_NAME: u64 = 0x57A7_0002;
const SALT_KIN: u64 = 0x57A7_0003;

/// An alien people that runs stations.
///
/// TODO-SEAM(w12-aliens): the aliens track's species registry replaces these placeholders
/// (the id is an index into it; [`Self::name`] reads it).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct AlienKin(pub u8);

impl AlienKin {
    /// Placeholder peoples (until the aliens track's registry is wired).
    pub const PLACEHOLDERS: [&'static str; 3] = ["the Veth", "the Orrun", "the Sillae"];

    /// Their name ("the Veth").
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::PLACEHOLDERS[usize::from(self.0) % Self::PLACEHOLDERS.len()]
    }
}

/// Who runs a station.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum StationOwner {
    /// Human spacers (Halcyon Dock and most stations).
    #[default]
    Human,
    /// An alien people ([`AlienKin`]).
    Alien(AlienKin),
    /// Nobody: a dead hulk to strip.
    Derelict,
}

impl StationOwner {
    /// A label for the hub's header ("human outpost", "alien-run (the Veth)", "derelict").
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Human => "human-run".to_owned(),
            Self::Alien(kin) => format!("run by {}", kin.name()),
            Self::Derelict => "derelict".to_owned(),
        }
    }

    /// Whether anyone is aboard.
    #[must_use]
    pub const fn crewed(self) -> bool {
        !matches!(self, Self::Derelict)
    }
}

/// What one station offers (see the module docs).
#[allow(
    clippy::struct_excessive_bools,
    reason = "a station's services, one switch each"
)]
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct StationProfile {
    pub id: StationId,
    pub kind: StationKind,
    pub owner: StationOwner,
    pub name: String,
    /// The home system's dock.
    pub halcyon: bool,
    /// Highest tier of every ship line its yard fits (0: no shipyard).
    pub yard_tier: u8,
    /// Highest planet-gear tier it fits (0: none).
    pub gear_tier: u8,
    /// Sells alien tech (alien-run only).
    pub alien_tech: bool,
    pub market: bool,
    pub bounties: bool,
    pub bar: bool,
    /// Refuel, repair and flight gear (Wave 11's counter).
    pub dock_services: bool,
    /// A derelict to strip.
    pub salvage: bool,
    /// What it pays for ore, percent of its base value.
    pub ore_percent: u32,
    /// Credits a unit of fuel and a point of hull.
    pub fuel_price: u32,
    pub repair_price: u32,
    /// Its system's star class (the look's tint).
    pub class: StarClass,
    /// The planet it keeps station with (an index into its system's bodies).
    pub planet: u8,
    /// A small art variant (0-3).
    pub look: u8,
}

/// Name parts for human stations.
const HUMAN_NAMES: [&str; 20] = [
    "Meridian",
    "Tycho",
    "Bellwether",
    "Corvid",
    "Lantern",
    "Kestrel",
    "Juniper",
    "Anvil",
    "Harrow",
    "Cinder",
    "Wayfarer",
    "Gannet",
    "Sable",
    "Thimble",
    "Orison",
    "Larkspur",
    "Copperline",
    "Farhold",
    "Saltmarsh",
    "Northlight",
];
const ALIEN_FIRST: [&str; 10] = [
    "Vey", "Oru", "Sil", "Kha", "Ith", "Mor", "Quel", "Ash", "Ul", "Zhe",
];
const ALIEN_SECOND: [&str; 10] = [
    "'thal", "nesh", "lae", "'vor", "qua", "rinn", "'sei", "dao", "mmet", "'iri",
];
const ALIEN_PLACE: [&str; 5] = ["Spire", "Bloom", "Lattice", "Hollow", "Choir"];

/// The kind's word in a station's name.
const fn kind_word(kind: StationKind, roll: u64) -> &'static str {
    let pick = (roll % 3) as usize;
    match kind {
        StationKind::TradePost => ["Exchange", "Market", "Post"][pick],
        StationKind::Shipyard => ["Yards", "Drydock", "Slipway"][pick],
        StationKind::Outpost => ["Outpost", "Station", "Watch"][pick],
        StationKind::Relay => ["Relay", "Beacon", "Relay"][pick],
    }
}

/// A uniform pick of `n` from a hash.
const fn pick(roll: u64, n: usize) -> usize {
    ((roll >> 7) % n as u64) as usize
}

/// The station's profile (see the module docs).
#[must_use]
pub fn profile_of(seed: u64, system: &StarSystem, station: &Station) -> StationProfile {
    let id = station.id;
    let halcyon = system.home && id.index == 0;
    let owner_roll = station_hash(seed, id, SALT_OWNER) % 100;
    let owner = if halcyon {
        StationOwner::Human
    } else {
        let derelict = match station.kind {
            StationKind::Relay => owner_roll < 45,
            _ => owner_roll < 12,
        };
        if derelict {
            StationOwner::Derelict
        } else if owner_roll >= 70 {
            let kin = station_hash(seed, id, SALT_KIN) % AlienKin::PLACEHOLDERS.len() as u64;
            StationOwner::Alien(AlienKin(kin as u8))
        } else {
            StationOwner::Human
        }
    };
    let name_roll = station_hash(seed, id, SALT_NAME);
    let name = if halcyon {
        "Halcyon Dock".to_owned()
    } else {
        match owner {
            StationOwner::Human => format!(
                "{} {}",
                HUMAN_NAMES[pick(name_roll, HUMAN_NAMES.len())],
                kind_word(station.kind, name_roll >> 20)
            ),
            StationOwner::Alien(_) => format!(
                "{}{} {}",
                ALIEN_FIRST[pick(name_roll, ALIEN_FIRST.len())],
                ALIEN_SECOND[pick(name_roll >> 12, ALIEN_SECOND.len())],
                ALIEN_PLACE[pick(name_roll >> 24, ALIEN_PLACE.len())]
            ),
            StationOwner::Derelict => format!(
                "Wreck of {} {}",
                HUMAN_NAMES[pick(name_roll, HUMAN_NAMES.len())],
                kind_word(station.kind, name_roll >> 20)
            ),
        }
    };
    let crewed = owner.crewed();
    let human = matches!(owner, StationOwner::Human);
    let alien = matches!(owner, StationOwner::Alien(_));
    let yard_tier = match (owner, station.kind) {
        _ if halcyon => 2,
        (StationOwner::Human, StationKind::Shipyard) => super::shipyard::LINE_MAX,
        (StationOwner::Human, StationKind::Outpost)
        | (StationOwner::Alien(_), StationKind::Shipyard) => 3,
        (StationOwner::Human, StationKind::TradePost) => 1,
        _ => 0,
    };
    let gear_tier = match (owner, station.kind) {
        _ if halcyon => 1,
        (StationOwner::Derelict, _) => 0,
        (StationOwner::Alien(_), _)
        | (StationOwner::Human, StationKind::Shipyard | StationKind::Outpost) => {
            super::shipyard::GEAR_MAX
        }
        (StationOwner::Human, _) => 1,
    };
    let ore_percent = match (owner, station.kind) {
        _ if halcyon => 100,
        (StationOwner::Alien(_), _) => 135,
        (_, StationKind::TradePost) => 125,
        (_, StationKind::Outpost) => 115,
        (_, StationKind::Relay) => 110,
        (_, StationKind::Shipyard) => 105,
    };
    StationProfile {
        id,
        kind: station.kind,
        owner,
        name,
        halcyon,
        yard_tier,
        gear_tier,
        alien_tech: alien,
        market: crewed,
        bounties: human || (alien && station.kind == StationKind::Outpost),
        bar: crewed,
        dock_services: crewed,
        salvage: !crewed,
        ore_percent,
        fuel_price: if alien { 3 } else { 2 },
        repair_price: if alien { 5 } else { 4 },
        class: system.class,
        planet: station.planet,
        look: (station_hash(seed, id, SALT_NAME ^ 0xA5) % 4) as u8,
    }
}

/// The generator's sector of flight's galactic sector.
fn space_sector(sector: (i64, i64)) -> Option<SpaceSector> {
    Some(SpaceSector::new(
        i32::try_from(sector.0).ok()?,
        i32::try_from(sector.1).ok()?,
    ))
}

/// The generator's system and station a flight station body names.
#[must_use]
pub fn station_at(space: &GeneratedSpace, body: BodyRef) -> Option<(StarSystem, Station)> {
    let index = body.index.checked_sub(STATION_INDEX)?;
    let system = space.star_system(body.system)?;
    let station = *system.stations.get(usize::from(index))?;
    Some((system, station))
}

/// The id of the station a flight body names.
#[must_use]
pub fn station_id(space: &GeneratedSpace, body: BodyRef) -> Option<StationId> {
    station_at(space, body).map(|(_, station)| station.id)
}

/// The profile of the station a flight body names.
#[must_use]
pub fn profile_at(space: &GeneratedSpace, body: BodyRef) -> Option<StationProfile> {
    let (system, station) = station_at(space, body)?;
    Some(profile_of(space.space().seed(), &system, &station))
}

/// The flight body of station `id`.
#[must_use]
pub fn station_ref(space: &GeneratedSpace, id: StationId) -> Option<BodyRef> {
    let system = space.space().system(crate::space::SystemRef {
        sector: id.sector,
        system: id.system,
    })?;
    if usize::from(id.index) >= system.stations.len() {
        return None;
    }
    Some(BodyRef {
        system: GeneratedSpace::reference(id.sector, &system),
        index: STATION_INDEX.checked_add(id.index)?,
    })
}

/// The profile of station `id`.
#[must_use]
pub fn profile_of_id(space: &GeneratedSpace, id: StationId) -> Option<StationProfile> {
    let system = space.space().system(crate::space::SystemRef {
        sector: id.sector,
        system: id.system,
    })?;
    let station = *system.stations.get(usize::from(id.index))?;
    Some(profile_of(space.space().seed(), &system, &station))
}

/// Flight's system of a station.
#[must_use]
pub fn flight_system(space: &GeneratedSpace, id: StationId) -> Option<FlightSystemRef> {
    station_ref(space, id).map(|body| body.system)
}

/// Flight's galactic sector of a station's (as `(i64, i64)`) from the generator's.
#[must_use]
pub fn galactic(sector: SpaceSector) -> (i64, i64) {
    (i64::from(sector.x), i64::from(sector.y))
}

/// The generator's sector of a flight system.
#[must_use]
pub fn generator_sector(system: FlightSystemRef) -> Option<SpaceSector> {
    space_sector(system.sector)
}
