//! Where the species live (Wave 12 aliens track): a pure function of the world seed and the
//! space generator's bodies.
//!
//! * Every species has a **first home** ([`first_home`]): the first landable planet of the home
//!   system, then of the systems round it (nearest first), whose theme it likes (any free planet
//!   if none does). So each species is met within a short flight of home: first contact comes
//!   early in the galaxy, as the plan wants it.
//! * Every other landable body rolls for folk ([`folk_of_body`]): a planet of a theme a species
//!   likes is settled about one time in five, a moon about one in ten.
//!
//! The answer is written into the body's [`crate::planet::PlanetParams::folk`] by
//! [`crate::place::planet_params_for`], which is what the planet generator (flattened town
//! ground, paving, lexicon stones) and the towns ([`super::town`]) read.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "hashed rolls reduced into small bounded ranges"
)]

use std::{
    collections::BTreeMap,
    sync::{Mutex, PoisonError},
};

use crate::space::{BodyKind, HOME_SECTOR, Space, SpaceBody, SpaceBodyId, SpaceSector, SystemRef};

use super::AlienSpecies;

/// Per mille of liked planets a species settles (beyond the first homes).
pub const PLANET_SETTLE_PERMILLE: u64 = 200;
/// Per mille of liked moons a species settles.
pub const MOON_SETTLE_PERMILLE: u64 = 100;
/// How far round home (sectors, Chebyshev) the first homes are looked for.
pub const FIRST_HOME_REACH_SECTORS: i32 = 1;

/// A splitmix step.
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Whether a body could hold a town (landable, not the home world or the Moon).
#[must_use]
pub const fn settleable(body: &SpaceBody) -> bool {
    !body.is_home() && body.radius >= crate::polar::MIN_LANDABLE_RADIUS
}

/// The first homes of every species in the world of `seed` (memoized per seed).
#[must_use]
pub fn first_homes(seed: u64) -> [Option<SpaceBodyId>; 3] {
    static CACHE: Mutex<BTreeMap<u64, [Option<SpaceBodyId>; 3]>> = Mutex::new(BTreeMap::new());
    if let Some(found) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&seed)
    {
        return *found;
    }
    let homes = compute_first_homes(seed);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert(seed, homes);
    homes
}

/// The planets first homes are picked from: the home system's, then the systems within
/// [`FIRST_HOME_REACH_SECTORS`] of home by distance from the home sun.
fn candidates(seed: u64) -> Vec<(SpaceBodyId, SpaceBody)> {
    // Planets alone: every space generator agrees on them (Wave 14 finish).
    let space = Space::shared(crate::space::SpaceSeed::new(
        seed,
        crate::space::SpaceGenerator::V1,
    ));
    let home_sector = space.sector(HOME_SECTOR);
    let home_centre = home_sector
        .systems
        .first()
        .map_or_else(|| HOME_SECTOR.centre(), |system| system.centre);
    let mut systems: Vec<(i128, SystemRef)> = Vec::new();
    for dy in -FIRST_HOME_REACH_SECTORS..=FIRST_HOME_REACH_SECTORS {
        for dx in -FIRST_HOME_REACH_SECTORS..=FIRST_HOME_REACH_SECTORS {
            let coords = SpaceSector::new(HOME_SECTOR.x + dx, HOME_SECTOR.y + dy);
            for system in &space.sector(coords).systems {
                let distance = if system.home {
                    -1
                } else {
                    home_centre.distance2(system.centre)
                };
                systems.push((
                    distance,
                    SystemRef {
                        sector: coords,
                        system: system.index,
                    },
                ));
            }
        }
    }
    systems.sort();
    let mut out = Vec::new();
    for (_, reference) in systems {
        let Some(system) = space.system(reference) else {
            continue;
        };
        for body in &system.bodies {
            if body.kind == BodyKind::Planet && settleable(body) {
                out.push((
                    SpaceBodyId {
                        sector: reference.sector,
                        system: reference.system,
                        body: body.index,
                    },
                    body.clone(),
                ));
            }
        }
    }
    out
}

fn compute_first_homes(seed: u64) -> [Option<SpaceBodyId>; 3] {
    let candidates = candidates(seed);
    let mut homes = [None; 3];
    let mut taken: Vec<SpaceBodyId> = Vec::new();
    // The rocky-world smiths first (the most themes), then the singers, then the keepers: each
    // takes the nearest planet it likes that nobody took.
    for species in [
        AlienSpecies::Tokkari,
        AlienSpecies::Vessi,
        AlienSpecies::Siphra,
    ] {
        let def = species.def();
        let pick = candidates
            .iter()
            .find(|(id, body)| !taken.contains(id) && def.likes(body.theme))
            .or_else(|| candidates.iter().find(|(id, _)| !taken.contains(id)))
            .map(|(id, _)| *id);
        if let Some(id) = pick {
            taken.push(id);
        }
        homes[usize::from(species.index())] = pick;
    }
    homes
}

/// The first homes computed afresh (no cache), for tests.
#[cfg(test)]
pub(crate) fn compute_first_homes_for_tests(seed: u64) -> [Option<SpaceBodyId>; 3] {
    compute_first_homes(seed)
}

/// The first home of `species` in the world of `seed`.
#[must_use]
pub fn first_home(seed: u64, species: AlienSpecies) -> Option<SpaceBodyId> {
    first_homes(seed)[usize::from(species.index())]
}

/// The folk of `body` (`id` in the world of `seed`): `0` for nobody, else
/// [`AlienSpecies::folk`].
#[must_use]
pub fn folk_of_body(seed: u64, id: SpaceBodyId, body: &SpaceBody) -> u8 {
    if !settleable(body) {
        return 0;
    }
    let homes = first_homes(seed);
    for species in AlienSpecies::ALL {
        if homes[usize::from(species.index())] == Some(id) {
            return species.folk();
        }
    }
    // Not anyone's first home: roll for each species that likes the theme.
    let chance = match body.kind {
        BodyKind::Planet => PLANET_SETTLE_PERMILLE,
        BodyKind::Moon | BodyKind::CraterRock => MOON_SETTLE_PERMILLE,
        BodyKind::HomeWorld | BodyKind::HomeMoon => 0,
    };
    let roll = mix(seed ^ id.mix() ^ 0x464F_4C4B_0000_0001) % 1_000;
    if roll >= chance {
        return 0;
    }
    let liking: Vec<AlienSpecies> = AlienSpecies::ALL
        .into_iter()
        .filter(|species| species.def().likes(body.theme))
        .collect();
    if liking.is_empty() {
        return 0;
    }
    let pick = (mix(roll ^ id.mix()) % liking.len() as u64) as usize;
    liking[pick].folk()
}
