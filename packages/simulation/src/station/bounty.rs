//! The bounty board and the mission framework.
//!
//! A station with a board offers three missions at a time, one per slot. A slot's mission is a
//! pure function of the world seed, the station, the slot and the slot's serial (how many times
//! it was taken), so every peer shows the same board; taking a mission rolls the slot's next
//! one ([`super::StationLedger::boards`]). Kinds ([`MissionKind`], append-only):
//!
//! * **Deliver**: the station loads `units` of a good into the hold (they take hold room and
//!   cannot be sold); dock at the other station and turn in to deliver and be paid.
//! * **Scan a body**: ping the scanner near the body (within its approach, twice as far with
//!   the xeno lens) or use it on the body's ground.
//! * **Mine a quota**: collect `units` of ore on the body's ground.
//!
//! Scans and quotas, once done, are paid at any station's board ([`super::StationAction::
//! TurnIn`]). Missions never expire (no unwinnable countdowns); a player holds at most
//! [`MAX_ACTIVE`] and may abandon one at no cost but the consignment.
//!
//! Rewards grow with distance (another system pays more), the quota and the body's gravity;
//! outposts pay a fifth more. Halcyon Dock's first board is a fixed starter set (a scan, a
//! quota and a delivery inside or next to the home system) so a first trip out has clear,
//! near goals.
//!
//! * **Bounty** (Wave 12 integration with the hostiles track): a named elite ship of a hostile
//!   faction ([`crate::hostiles::bounty_targets`]: a pirate captain, a swarm brood-mother, a
//!   machine warden) with its escort, lurking in a system a little farther out. Taking it posts
//!   the bounty for the player (the ships appear when the rig comes near) and points the nav
//!   waypoint at its system; the kill pays the reward on the spot and closes the mission.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use serde::{Deserialize, Serialize};

use super::{
    StationId, StationKind,
    market::Good,
    profile::{StationProfile, profile_of},
    station_hash,
};
use crate::space::{BodyKind, Space, SpaceBodyId, SpaceTile, StarSystem, SystemRef};

/// Missions a player may hold at once.
pub const MAX_ACTIVE: usize = 3;
/// Slots on a board.
pub const BOARD_SLOTS: u8 = 3;
/// Systems within this many tiles of a station's system are near enough for its missions.
pub const NEAR_TILES: i64 = 6_000_000;

const SALT_MISSION: u64 = 0x57A7_0201;

/// A mission's id: the station, slot and serial that rolled it.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct MissionId {
    pub station: StationId,
    pub slot: u8,
    pub serial: u16,
}

/// What a mission asks (append-only).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MissionKind {
    /// Carry `units` of `good` (loaded at the giver) to station `to`.
    Deliver {
        good: Good,
        units: u16,
        to: StationId,
    },
    /// Scan `body`.
    Scan { body: SpaceBodyId },
    /// Collect `units` of ore on `body`'s ground.
    Mine { body: SpaceBodyId, units: u16 },
    /// Hunt down a hostile faction's named ship: target `index` of the board of system
    /// `posted` on day `day` ([`bounty_target_of`]; `target` is its id). Wave 12 hostiles.
    Bounty {
        target: u64,
        posted: crate::flight::SystemRef,
        day: u32,
        index: u8,
    },
}

impl MissionKind {
    /// Units of work (deliveries count the consignment).
    #[must_use]
    pub const fn goal(self) -> u16 {
        match self {
            Self::Deliver { units, .. } | Self::Mine { units, .. } => units,
            Self::Scan { .. } | Self::Bounty { .. } => 1,
        }
    }

    /// The body it is about, if any.
    #[must_use]
    pub const fn body(self) -> Option<SpaceBodyId> {
        match self {
            Self::Scan { body } | Self::Mine { body, .. } => Some(body),
            Self::Deliver { .. } | Self::Bounty { .. } => None,
        }
    }

    /// Hold room it takes while under way.
    #[must_use]
    pub const fn hold_units(self) -> u32 {
        match self {
            Self::Deliver { units, .. } => units as u32,
            _ => 0,
        }
    }
}

/// A mission on a board or under way.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Mission {
    pub id: MissionId,
    pub kind: MissionKind,
    /// Credits paid on turning in.
    pub reward: u32,
    /// Units done.
    #[serde(default)]
    pub progress: u16,
}

impl Mission {
    /// Whether its work is done (a delivery is done when handed over at turn-in).
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.progress >= self.kind.goal()
    }
}

/// What a board needs to know about the space round a station.
#[derive(Clone, Debug, Default)]
pub struct BoardContext {
    pub station: Option<StationProfile>,
    /// The station's own position.
    pub at: SpaceTile,
    /// Landable bodies near (not the home world or the Moon): id, distance in tiles, whether
    /// in another system, gravity (milli-g), kind.
    pub bodies: Vec<NearBody>,
    /// Crewed stations near (not this one): id, distance, whether in another system.
    pub stations: Vec<(StationId, f64, bool)>,
}

/// A landable body near a station.
#[derive(Clone, Copy, Debug)]
pub struct NearBody {
    pub id: SpaceBodyId,
    pub distance: f64,
    pub elsewhere: bool,
    pub gravity_milli_g: u16,
    pub radius: u32,
}

/// The board context of station `id` (`None`: no such station).
#[must_use]
pub fn board_context(space: &Space, id: StationId) -> Option<BoardContext> {
    let home = SystemRef {
        sector: id.sector,
        system: id.system,
    };
    let system = space.system(home)?;
    let station = *system.stations.get(usize::from(id.index))?;
    let profile = profile_of(space.seed(), &system, &station);
    let at = station.centre;
    let mut bodies = Vec::new();
    let mut stations = Vec::new();
    let mut visit = |reference: SystemRef, found: &StarSystem| {
        let elsewhere = reference != home;
        for body in &found.bodies {
            if body.is_home()
                || !matches!(
                    body.kind,
                    BodyKind::Planet | BodyKind::Moon | BodyKind::CraterRock
                )
            {
                continue;
            }
            bodies.push(NearBody {
                id: SpaceBodyId {
                    sector: reference.sector,
                    system: reference.system,
                    body: body.index,
                },
                distance: body.centre.distance(at),
                elsewhere,
                gravity_milli_g: body.gravity_milli_g,
                radius: body.radius,
            });
        }
        for other in &found.stations {
            if other.id == id {
                continue;
            }
            if profile_of(space.seed(), found, other).owner.crewed() {
                stations.push((other.id, other.centre.distance(at), elsewhere));
            }
        }
    };
    visit(home, &system);
    for (reference, _) in space.systems_near(at, NEAR_TILES) {
        if reference == home {
            continue;
        }
        if let Some(found) = space.system(reference) {
            visit(reference, &found);
        }
    }
    bodies.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id)));
    stations.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    Some(BoardContext {
        station: Some(profile),
        at,
        bodies,
        stations,
    })
}

/// Distance pay: credits for `distance` tiles (one per 600 tiles, at most 1,200), plus 900
/// for another system.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a bounded distance in tiles"
)]
fn distance_pay(distance: f64, elsewhere: bool) -> u32 {
    let near = ((distance / 600.0) as u32).min(1_200);
    near + if elsewhere { 900 } else { 0 }
}

/// The reward for `kind` offered at `station`.
#[must_use]
pub fn reward_for(kind: MissionKind, station: &StationProfile, context: &BoardContext) -> u32 {
    let base = match kind {
        MissionKind::Scan { body } => {
            let near = context.bodies.iter().find(|found| found.id == body);
            600 + near.map_or(0, |found| distance_pay(found.distance, found.elsewhere))
        }
        MissionKind::Mine { body, units } => {
            let near = context.bodies.iter().find(|found| found.id == body);
            700 + 45 * u32::from(units)
                + near.map_or(0, |found| {
                    distance_pay(found.distance, found.elsewhere)
                        + u32::from(found.gravity_milli_g) / 2
                })
        }
        MissionKind::Deliver { good, units, to } => {
            let near = context.stations.iter().find(|found| found.0 == to);
            500 + u32::from(units) * good.base_price() / 2
                + near.map_or(0, |found| distance_pay(found.1, found.2))
        }
        // The kill pays the bounty's own reward (the hostiles track pays it on the spot);
        // the board shows that figure ([`offer`]).
        MissionKind::Bounty { .. } => return 0,
    };
    // Wave 13: on the space credit scale (the delivery's goods already are; their share is
    // taken back out so it is not scaled twice).
    let base = match kind {
        MissionKind::Deliver { good, units, .. } => {
            let goods = u32::from(units) * good.base_price() / 2;
            crate::endgame::economy::space_credits(base - goods) + goods
        }
        _ => crate::endgame::economy::space_credits(base),
    };
    // Wave 13 endgame: far boards pay more (a quarter more a ring of galactic sectors from
    // home, up to eight rings), so the long curve keeps paying as the rig reaches farther.
    let ring = u32::try_from(
        station
            .id
            .sector
            .chebyshev(crate::space::HOME_SECTOR)
            .clamp(0, 8),
    )
    .unwrap_or(0);
    let base = base * (100 + 25 * ring) / 100;
    if station.kind == StationKind::Outpost {
        base + base / 5
    } else {
        base
    }
}

/// The mission in `slot` of `context`'s board at serial `serial` (`None`: nothing to offer).
#[must_use]
pub fn offer(
    seed: crate::space::SpaceSeed,
    context: &BoardContext,
    slot: u8,
    serial: u16,
) -> Option<Mission> {
    let station = context.station.as_ref()?;
    if !station.bounties {
        return None;
    }
    let id = MissionId {
        station: station.id,
        slot,
        serial,
    };
    let roll = station_hash(
        seed.seed,
        station.id,
        SALT_MISSION ^ (u64::from(slot) << 16) ^ (u64::from(serial) << 24),
    );
    let starter = station.halcyon && serial == 0;
    // A starter board keeps its fixed three; any other rolls a bounty one time in four
    // (Wave 12 integration: the hostile factions' targets).
    let kinds: u64 = if starter { 3 } else { 4 };
    let first_kind = if starter {
        u64::from(slot)
    } else {
        roll % kinds
    };
    // Try the rolled kind, then the others.
    for step in 0..kinds {
        let kind = match (first_kind + step) % kinds {
            0 => scan_offer(context, roll, starter),
            1 => mine_offer(context, roll, starter),
            2 => deliver_offer(context, roll, starter),
            _ => bounty_offer(seed, station, serial, roll),
        };
        if let Some(kind) = kind {
            let reward = match kind {
                MissionKind::Bounty { .. } => {
                    bounty_target_of(seed, kind).map_or(0, |target| target.reward)
                }
                _ => reward_for(kind, station, context),
            };
            return Some(Mission {
                id,
                kind,
                reward,
                progress: 0,
            });
        }
    }
    None
}

/// A bounty on a board: one of the hostile factions' targets posted round the station's
/// system (the targets change with the slot's serial, so a taken bounty is replaced).
fn bounty_offer(
    seed: crate::space::SpaceSeed,
    station: &StationProfile,
    serial: u16,
    roll: u64,
) -> Option<MissionKind> {
    let space = Space::shared(seed);
    let system = space.system(SystemRef {
        sector: station.id.sector,
        system: station.id.system,
    })?;
    let posted = crate::flight::space::GeneratedSpace::reference(station.id.sector, &system);
    let day = u32::from(serial);
    let targets = crate::hostiles::bounty_targets(seed, posted, day);
    if targets.is_empty() {
        return None;
    }
    let index = ((roll >> 13) % targets.len() as u64) as usize;
    let target = &targets[index];
    Some(MissionKind::Bounty {
        target: target.id,
        posted,
        day,
        index: u8::try_from(index).unwrap_or(0),
    })
}

/// The hostile target a bounty mission names (`None` for other kinds).
#[must_use]
pub fn bounty_target_of(
    seed: crate::space::SpaceSeed,
    kind: MissionKind,
) -> Option<crate::hostiles::BountyTarget> {
    let MissionKind::Bounty {
        posted, day, index, ..
    } = kind
    else {
        return None;
    };
    crate::hostiles::bounty_targets(seed, posted, day)
        .into_iter()
        .nth(usize::from(index))
}

/// Picks one of `n` candidates (the nearest for a starter).
const fn choose(n: usize, roll: u64, starter: bool) -> usize {
    if starter {
        0
    } else {
        ((roll >> 11) % n as u64) as usize
    }
}

fn scan_offer(context: &BoardContext, roll: u64, starter: bool) -> Option<MissionKind> {
    let candidates: Vec<&NearBody> = context.bodies.iter().take(8).collect();
    if candidates.is_empty() {
        return None;
    }
    let body = candidates[choose(candidates.len(), roll, starter)];
    Some(MissionKind::Scan { body: body.id })
}

fn mine_offer(context: &BoardContext, roll: u64, starter: bool) -> Option<MissionKind> {
    // Light worlds only for a starter (an easy first landing).
    let mut candidates: Vec<&NearBody> = context
        .bodies
        .iter()
        .filter(|body| body.gravity_milli_g <= 1_400)
        .take(8)
        .collect();
    if starter {
        candidates.sort_by(|a, b| {
            a.gravity_milli_g
                .cmp(&b.gravity_milli_g)
                .then(a.distance.total_cmp(&b.distance))
                .then(a.id.cmp(&b.id))
        });
    }
    if candidates.is_empty() {
        return None;
    }
    let body = candidates[choose(candidates.len(), roll, starter)];
    let units = if starter {
        12
    } else {
        12 + ((roll >> 23) % 19) as u16
    };
    Some(MissionKind::Mine {
        body: body.id,
        units,
    })
}

fn deliver_offer(context: &BoardContext, roll: u64, starter: bool) -> Option<MissionKind> {
    let candidates: Vec<&(StationId, f64, bool)> = context.stations.iter().take(6).collect();
    if candidates.is_empty() {
        return None;
    }
    let to = candidates[choose(candidates.len(), roll, starter)].0;
    let goods = [
        Good::WaterIce,
        Good::Hydroponics,
        Good::MachineParts,
        Good::MedKits,
        Good::ReactorCells,
    ];
    let good = goods[((roll >> 31) % goods.len() as u64) as usize];
    let units = if starter {
        4
    } else {
        4 + ((roll >> 37) % 5) as u16
    };
    Some(MissionKind::Deliver { good, units, to })
}

/// A body's name (`Kessa III`, `Kessa IIb`).
#[must_use]
pub fn body_name(space: &Space, body: SpaceBodyId) -> String {
    space
        .system(SystemRef {
            sector: body.sector,
            system: body.system,
        })
        .map_or_else(
            || "an unknown world".to_owned(),
            |system| system.body_name(body.body),
        )
}

/// A station's name.
#[must_use]
pub fn station_name(space: &Space, station: StationId) -> String {
    space
        .system(SystemRef {
            sector: station.sector,
            system: station.system,
        })
        .and_then(|system| {
            let found = *system.stations.get(usize::from(station.index))?;
            Some(profile_of(space.seed(), &system, &found).name)
        })
        .unwrap_or_else(|| "a station".to_owned())
}

/// A mission's title for the board, the log and the HUD (`Scan Kessa III`, `Mine 12 ore on
/// Kessa IIb`, `Deliver 4 Med Kits to Tycho Exchange`).
#[must_use]
pub fn describe(space: &Space, kind: MissionKind) -> String {
    match kind {
        MissionKind::Scan { body } => format!("Scan {}", body_name(space, body)),
        MissionKind::Mine { body, units } => {
            format!("Mine {units} ore on {}", body_name(space, body))
        }
        MissionKind::Deliver { good, units, to } => format!(
            "Deliver {units} {} to {}",
            good.name(),
            station_name(space, to)
        ),
        MissionKind::Bounty { .. } => bounty_target_of(space.space_seed(), kind).map_or_else(
            || "Bounty".to_owned(),
            |target| format!("Bounty: {} ({})", target.name, target.faction.name()),
        ),
    }
}
