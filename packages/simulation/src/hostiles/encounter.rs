//! Encounter pacing and placement.
//!
//! Where the factions fly, how often and how hard they come, the bosses' lairs, the machine
//! patrols' routes and the bounty targets (all pure functions of the world seed and positions).
//!
//! * **Distance from home** is the galactic ring: the Chebyshev distance in galactic sectors
//!   from the home sector. Ring 0 (the home sector) is calm; so is the space round every station
//!   ([`STATION_CALM_TILES`]).
//! * **How often**: every [`ROLL_STEPS`] a rig in free flight rolls
//!   [`chance_permille`] (rising with the ring); after an encounter it rests for
//!   [`REST_STEPS`].
//! * **Who**: [`faction_weights`]: pirates everywhere and more in nebulae and belts, the swarm
//!   round living worlds, the machines round machine worlds and far out.
//! * **How hard**: a budget of `2 + ring + tier / 2` (capped) spent on the faction's ships
//!   ([`compose`]), where the tier is the rig's [`super::ShipGear::tier`] plus its hull and
//!   engine upgrades' share.

#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "plain IEEE arithmetic: the same bits on every platform (no fused multiply-add or \
              platform hypot)"
)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "steps, ids and hashes fold into hash inputs and small counts"
)]

use serde::{Deserialize, Serialize};

use crate::{
    flight::{
        BodyKind, SpaceQuery, SystemRef,
        space::{GeneratedSpace, galaxy_sector_of, hash64, home_star},
    },
    position::WorldPos,
    space::{SpaceSector, Theme},
    star_chart::StarClass,
};

use super::{Faction, ShipClass, steps, tiles};

/// Steps between encounter rolls for a rig in free flight (5 s).
pub const ROLL_STEPS: u64 = 600;
/// Steps a rig rests after an encounter before the next can find it (60 s).
pub const REST_STEPS: u64 = 7_200;
/// Stations keep the space this near them calm (tiles).
pub const STATION_CALM_TILES: f32 = 9_000.0;
/// Most a roll's chance reaches (per mille).
pub const MAX_CHANCE_PERMILLE: u32 = 220;
/// Most an encounter's budget reaches.
pub const MAX_BUDGET: u32 = 14;
/// A boss's lair reaches this far (tiles): a rig in it is pulled out of cruise and met.
pub const LAIR_REACH_TILES: f32 = 45_000.0;
/// A lair sits this far from its star toward home (tiles).
pub const LAIR_OFFSET_TILES: f64 = 80_000.0;
/// A bounty target lurks within this of its point (tiles).
pub const BOUNTY_REACH_TILES: f32 = 30_000.0;
/// A machine patrol flies this far above its world's surface (tiles).
pub const PATROL_ALTITUDE_TILES: f32 = 60.0;
/// A machine patrol meets a rig this far above its route (tiles).
pub const PATROL_MEET_TILES: f32 = 600.0;
/// A beaten patrol is back after this long (steps; 10 minutes).
pub const PATROL_EPOCH_STEPS: u64 = 120 * 600;

/// The galactic ring of `pos`: 0 in the home sector, +1 a sector out (Chebyshev).
#[must_use]
pub fn danger_ring(pos: WorldPos) -> u8 {
    let (hx, hy) = galaxy_sector_of(home_star());
    let (sx, sy) = galaxy_sector_of(pos);
    let ring = (sx - hx).abs().max((sy - hy).abs());
    u8::try_from(ring.min(i64::from(u8::MAX))).unwrap_or(u8::MAX)
}

/// Whether nothing hostile finds a rig at `pos`: the home sector, and near stations.
#[must_use]
pub fn calm_at(space: &GeneratedSpace, pos: WorldPos) -> bool {
    if danger_ring(pos) == 0 {
        return true;
    }
    space.bodies_near(pos).iter().any(|body| {
        body.kind == BodyKind::Station && pos.distance(body.centre) < tiles(STATION_CALM_TILES)
    })
}

/// The chance a roll at ring `ring` finds a rig of tier `tier`, per mille: 30 at ring 1, +35 a
/// ring, a little more for well-armed rigs (they are worth it), capped.
#[must_use]
pub fn chance_permille(ring: u8, tier: u8) -> u32 {
    if ring == 0 {
        return 0;
    }
    let base = 30 + 35 * u32::from(ring - 1) + 3 * u32::from(tier);
    base.min(MAX_CHANCE_PERMILLE)
}

/// An encounter's budget at ring `ring` against a rig of tier `tier`.
#[must_use]
pub fn budget(ring: u8, tier: u8) -> u32 {
    (2 + u32::from(ring) + u32::from(tier) / 2).min(MAX_BUDGET)
}

/// A ship class's cost in a budget.
#[must_use]
pub const fn cost(class: ShipClass) -> u32 {
    match class {
        ShipClass::Mite => 1,
        ShipClass::Cutter | ShipClass::Drone => 2,
        ShipClass::Stinger => 3,
        ShipClass::Marauder => 4,
        ShipClass::Lancer => 5,
        ShipClass::PirateKing | ShipClass::SwarmQueen | ShipClass::Arbiter | ShipClass::Warden => {
            50
        }
    }
}

/// The ships `faction` sends with `budget` (rolled from `roll`). Never empty.
#[must_use]
pub fn compose(faction: Faction, budget: u32, roll: u64) -> Vec<ShipClass> {
    let mut left = budget.max(2);
    let mut ships = Vec::new();
    let mut roll = roll;
    let mut next = || {
        roll = hash64(roll, 0xC0_4705, 1, 2);
        roll
    };
    match faction {
        Faction::Pirates => {
            // Marauders from budget 6 up, cutters for the rest.
            while left >= 2 {
                let heavy = left >= 6 && next() % 3 == 0;
                let class = if heavy {
                    ShipClass::Marauder
                } else {
                    ShipClass::Cutter
                };
                left -= cost(class).min(left);
                ships.push(class);
            }
        }
        Faction::Swarm => {
            // A flock: mostly mites, a stinger per four budget.
            while left >= 1 {
                let class = if left >= 3 && next() % 4 == 0 {
                    ShipClass::Stinger
                } else {
                    ShipClass::Mite
                };
                left -= cost(class).min(left);
                ships.push(class);
            }
            if ships.len() < 3 {
                ships.resize(3, ShipClass::Mite);
            }
        }
        Faction::Machine => {
            // A lancer leads (from budget 5), drones fill the V.
            if left >= 5 {
                ships.push(ShipClass::Lancer);
                left -= 5;
                if left >= 10 && next() % 2 == 0 {
                    ships.push(ShipClass::Lancer);
                    left -= 5;
                }
            }
            while left >= 2 {
                ships.push(ShipClass::Drone);
                left -= 2;
            }
            if ships.is_empty() {
                ships.push(ShipClass::Drone);
            }
        }
    }
    ships
}

/// How strongly each faction claims `pos` (pirates, swarm, machines).
#[must_use]
pub fn faction_weights(space: &GeneratedSpace, pos: WorldPos) -> [u32; 3] {
    let ring = u32::from(danger_ring(pos));
    let mut pirates = 6;
    let mut swarm = 2;
    let mut machines = 1 + ring / 2;
    if space.fog_at(pos) > 0 {
        pirates += 10;
    }
    let tile = crate::space::SpaceTile::of_pos(pos);
    for summary in space.systems_near(pos, 0) {
        let Some(system) = space.star_system(summary.system) else {
            continue;
        };
        let from_star = system.centre.distance(tile);
        if system.class == StarClass::Nebula || system.nebula.is_some() {
            pirates += 4;
        }
        if system
            .belts
            .iter()
            .any(|belt| from_star >= f64::from(belt.inner) && from_star <= f64::from(belt.outer))
        {
            pirates += 8;
        }
        for body in &system.bodies {
            match body.theme {
                Theme::Jungle | Theme::Fungal | Theme::OceanArchipelago => swarm += 5,
                Theme::MachineWorld => machines += 9,
                _ => {}
            }
        }
    }
    if ring >= 5 {
        machines += 4;
    }
    [pirates, swarm, machines]
}

/// The faction a roll of `roll` picks at `pos`.
#[must_use]
pub fn choose_faction(space: &GeneratedSpace, pos: WorldPos, roll: u64) -> Faction {
    let weights = faction_weights(space, pos);
    let total: u32 = weights.iter().sum();
    let mut pick = u32::try_from(roll % u64::from(total.max(1))).unwrap_or(0);
    for (faction, weight) in Faction::ALL.into_iter().zip(weights) {
        if pick < weight {
            return faction;
        }
        pick -= weight;
    }
    Faction::Pirates
}

// --- Lairs ------------------------------------------------------------------------------------

/// Where a boss waits.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Lair {
    pub faction: Faction,
    pub system: SystemRef,
    /// The lair's point (tiles from its star toward home).
    pub pos: WorldPos,
}

/// The first ring a faction's lair may be in.
#[must_use]
pub const fn lair_ring(faction: Faction) -> i64 {
    match faction {
        Faction::Pirates => 2,
        Faction::Swarm => 3,
        Faction::Machine => 4,
    }
}

fn lair_fits(faction: Faction, system: &crate::space::StarSystem) -> bool {
    match faction {
        Faction::Pirates => system.class == StarClass::Nebula || system.nebula.is_some(),
        Faction::Swarm => system
            .bodies
            .iter()
            .any(|body| matches!(body.theme, Theme::Jungle | Theme::Fungal)),
        Faction::Machine => system
            .bodies
            .iter()
            .any(|body| body.theme == Theme::MachineWorld),
    }
}

/// Where `faction`'s boss waits in the world seeded `seed`.
///
/// The first fitting system (pirates: a nebula; the swarm: a jungle or fungal world; the
/// machines: a machine world) on the rings from [`lair_ring`] outward, else the first system on
/// that ring. Cached per seed.
#[must_use]
pub fn boss_lair(seed: crate::space::SpaceSeed, faction: Faction) -> Option<Lair> {
    use std::{
        collections::BTreeMap,
        sync::{Mutex, OnceLock},
    };
    type LairCache = Mutex<BTreeMap<(crate::space::SpaceSeed, Faction), Option<Lair>>>;
    static CACHE: OnceLock<LairCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(found) = cache
        .lock()
        .ok()
        .and_then(|map| map.get(&(seed, faction)).copied())
    {
        return found;
    }
    let found = find_lair(seed, faction);
    if let Ok(mut map) = cache.lock() {
        if map.len() > 64 {
            map.clear();
        }
        map.insert((seed, faction), found);
    }
    found
}

fn find_lair(seed: crate::space::SpaceSeed, faction: Faction) -> Option<Lair> {
    let space = GeneratedSpace::new(seed);
    let (hx, hy) = galaxy_sector_of(home_star());
    let first = lair_ring(faction);
    let mut fallback = None;
    for ring in first..first + 4 {
        for (gx, gy) in ring_sectors(hx, hy, ring) {
            let (Ok(x), Ok(y)) = (i32::try_from(gx), i32::try_from(gy)) else {
                continue;
            };
            let coords = SpaceSector::new(x, y);
            let sector = space.space().sector(coords);
            for system in &sector.systems {
                if system.home {
                    continue;
                }
                let reference = GeneratedSpace::reference(coords, system);
                let lair = Lair {
                    faction,
                    system: reference,
                    pos: toward_home(system.centre.pos(), LAIR_OFFSET_TILES),
                };
                if lair_fits(faction, system) {
                    return Some(lair);
                }
                if ring == first && fallback.is_none() {
                    fallback = Some(lair);
                }
            }
        }
    }
    fallback
}

/// The sectors on ring `ring` round `(cx, cy)`, in a fixed order.
fn ring_sectors(cx: i64, cy: i64, ring: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for y in cy - ring..=cy + ring {
        for x in cx - ring..=cx + ring {
            if (x - cx).abs().max((y - cy).abs()) == ring {
                out.push((x, y));
            }
        }
    }
    out
}

/// `from` moved `tiles` toward the home star.
fn toward_home(from: WorldPos, distance_tiles: f64) -> WorldPos {
    let (dx, dy) = from.delta_f64(home_star());
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    let step = distance_tiles * f64::from(crate::terrain::TILE_SIZE);
    crate::flight::space::advance(from, dx / length * step, dy / length * step)
}

// --- Machine patrols ---------------------------------------------------------------------------

/// A machine patrol's route: a low orbit round a world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patrol {
    pub id: u64,
    pub centre: WorldPos,
    /// The route's radius (px).
    pub radius: f32,
    /// The patrol's ships.
    pub size: u32,
}

/// The machine patrols whose routes lie near `pos`: low orbits round machine worlds, and (from
/// ring 4 out) round every system's first world.
#[must_use]
pub fn patrols_near(space: &GeneratedSpace, pos: WorldPos) -> Vec<Patrol> {
    let ring = danger_ring(pos);
    if ring == 0 {
        return Vec::new();
    }
    let mut patrols = Vec::new();
    for summary in space.systems_near(pos, 0) {
        let Some(system) = space.star_system(summary.system) else {
            continue;
        };
        for (index, body) in system.bodies.iter().enumerate() {
            let guarded = body.theme == Theme::MachineWorld || (ring >= 4 && index == 0);
            if !guarded || body.is_home() {
                continue;
            }
            #[allow(clippy::cast_precision_loss, reason = "radii of a few thousand tiles")]
            let radius = tiles(body.radius as f32 + PATROL_ALTITUDE_TILES);
            patrols.push(Patrol {
                id: hash64(summary.system.id, 0x9A_7801, index as i64, 0) >> 1,
                centre: body.centre.pos(),
                radius,
                size: 4 + u32::from(ring.min(8)),
            });
        }
    }
    patrols
}

/// Whether a rig at `pos` meets `patrol` (near its route).
#[must_use]
pub fn meets_patrol(patrol: &Patrol, pos: WorldPos) -> bool {
    let distance = pos.distance(patrol.centre);
    (distance - patrol.radius).abs() < tiles(PATROL_MEET_TILES)
}

/// The key a patrol met at `step` is remembered by (it is back after an epoch).
#[must_use]
pub const fn patrol_key(patrol: u64, step: u64) -> u64 {
    hash64(patrol, 0xE9_0C45, (step / PATROL_EPOCH_STEPS) as i64, 0)
}

// --- Bounties ---------------------------------------------------------------------------------

/// A named elite ship a station's board offers (the stations track's mission target).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BountyTarget {
    pub id: u64,
    pub name: String,
    pub faction: Faction,
    pub class: ShipClass,
    /// Escorts flying with it.
    pub escorts: u8,
    /// The system it lurks in.
    pub system: SystemRef,
    /// Where it lurks.
    pub pos: WorldPos,
    /// Credits on completion.
    pub reward: u32,
}

const BOUNTY_NAMES: [&str; 16] = [
    "Red Vask",
    "the Widow of Kell",
    "Old Brannoch",
    "Saltjaw",
    "Mother Quell",
    "the Hollow Choir",
    "Brood Seven",
    "the Pale Bloom",
    "Unit Ordinal",
    "Warden Nine",
    "the Silent Tithe",
    "Marrow Jack",
    "Ashen Lio",
    "the Lantern Thief",
    "Clade Ninety",
    "Sentence Zero",
];

/// The bounties a station in `system` offers on day `day`.
///
/// Three a day, in the systems round it (never in the home sector). Pure: the stations track's
/// board lists these and posts the one a player takes with [`crate::WorldState::post_bounty`].
#[must_use]
pub fn bounty_targets(
    seed: crate::space::SpaceSeed,
    system: SystemRef,
    day: u32,
) -> Vec<BountyTarget> {
    let space = GeneratedSpace::new(seed);
    let Some(home) = space.system(system) else {
        return Vec::new();
    };
    let mut candidates: Vec<_> = space
        .systems_near(home.star, 2)
        .into_iter()
        .filter(|found| danger_ring(found.star) > 0)
        .collect();
    candidates.sort_by_key(|found| found.system);
    if candidates.is_empty() {
        return Vec::new();
    }
    (0..3_u8)
        .map(|index| {
            let id = super::bounty_id(system, day, index);
            let roll = hash64(seed.seed, id, 0, 0);
            let at = candidates[usize::try_from(roll % candidates.len() as u64).unwrap_or(0)];
            let faction = Faction::ALL[usize::try_from((roll >> 8) % 3).unwrap_or(0)];
            let class = match faction {
                Faction::Pirates => ShipClass::Marauder,
                Faction::Swarm => ShipClass::Stinger,
                Faction::Machine => ShipClass::Lancer,
            };
            #[allow(clippy::cast_precision_loss, reason = "a hash share")]
            let angle = ((roll >> 16) & 0xFFFF) as f32 / 65_536.0 * std::f32::consts::TAU;
            let (sin, cos) = crate::field::trig::sin_cos(angle);
            let reach = 60_000.0 * f64::from(crate::terrain::TILE_SIZE);
            let pos = crate::flight::space::advance(
                at.star,
                f64::from(sin) * reach,
                -f64::from(cos) * reach,
            );
            let ring = danger_ring(at.star);
            let name_index = usize::try_from((roll >> 32) % BOUNTY_NAMES.len() as u64).unwrap_or(0);
            BountyTarget {
                id,
                name: BOUNTY_NAMES[name_index].to_string(),
                faction,
                class,
                escorts: 2 + ring.min(4),
                system: at.system,
                pos,
                reward: crate::endgame::economy::space_credits(
                    class.bounty() * (2 + u32::from(ring)) + 500,
                ),
            }
        })
        .collect()
}

/// Steps a hailing demand stands before the pirates open fire (12 s).
pub const DEMAND_STEPS: u64 = steps(12.0);
