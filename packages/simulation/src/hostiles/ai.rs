//! How each faction flies and fights (the regular ships; bosses are [`super::boss`]).
//!
//! * **Pirates** close in and hold off at hailing range while their demand stands; fighting,
//!   cutters circle the rig (each its own way round) and strafe it, marauders hang back and
//!   fire bursts; the wounded run.
//! * **The swarm flocks**: separation from close mates, alignment with the flock's motion and
//!   cohesion toward its middle, plus the pull of its prey. Mites take turns diving at the rig
//!   and peel off after a bite; stingers hold off and spit.
//! * **The machines fly in formation**: drones keep their V slots off their lancer (which flies
//!   the patrol route until it sees a rig, then turns on it); drones fire their volleys on one
//!   shared beat, lancers charge telegraphed lances. A formation that loses every lancer breaks
//!   and its drones fight alone.
//!
//! Ships aim by turning their own noses at a limited rate (no instant aim), so a rig that
//! moves across their bows is hard to hit: fair, and readable.

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

use crate::{flight::space::hash64, ids::PlayerId, position::WorldPos};

use super::{
    Encounter, EncounterState, HostileShip, ShipClass, ShipMode, ShotKind,
    combat::{Order, bearing, from_ship_frame, hold_range, lead_point, length, match_velocity},
    steps,
};

/// A rig as the AI sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigView {
    pub player: PlayerId,
    pub pos: WorldPos,
    pub vx: f32,
    pub vy: f32,
}

/// Pirates hail from this far (px).
pub const HAIL_RANGE: f32 = 430.0;
/// Pirates start hailing once this close (px).
pub const HAIL_START: f32 = 560.0;
/// A cutter's strafing orbit (px).
pub const CUTTER_ORBIT: f32 = 300.0;
/// A marauder's stand-off range (px).
pub const MARAUDER_RANGE: f32 = 520.0;
/// A stinger's stand-off range (px).
pub const STINGER_RANGE: f32 = 430.0;
/// A mite's circling range between dives (px).
pub const MITE_ORBIT: f32 = 240.0;
/// The swarm's separation distance (px).
pub const FLOCK_SEPARATION: f32 = 52.0;
/// How far the flock's alignment and cohesion reach (px).
pub const FLOCK_REACH: f32 = 260.0;
/// A machine formation's attack range (px).
pub const FORMATION_RANGE: f32 = 600.0;
/// How far a patrol sees a rig (px; halved in a nebula's fog).
pub const PATROL_SIGHT: f32 = 1_500.0;
/// Machine slot spacing (px): rank behind and spread to the side.
pub const SLOT_BACK: f32 = 64.0;
pub const SLOT_SIDE: f32 = 70.0;
/// A wounded cutter runs at this share of its hull.
pub const CUTTER_FLEE_SHARE: f32 = 0.25;
/// A mite's dive lasts this long (steps).
pub const DIVE_STEPS: u64 = 150;

/// A formation slot's offset from its leader in the leader's frame (x right, y toward the
/// tail): a V behind it, alternating sides.
#[must_use]
pub fn slot_offset(slot: u8) -> (f32, f32) {
    let rank = f32::from(slot / 2 + 1);
    let side = if slot.is_multiple_of(2) { -1.0 } else { 1.0 };
    (side * SLOT_SIDE * rank, SLOT_BACK * rank)
}

/// The ship's order for one step.
///
/// `target` is its rig (if any), `mates` every ship of its encounter (itself included), `leader`
/// the formation's lancer (machines), `step` the combat step, `seed` the world's.
#[must_use]
pub fn order(
    ship: &HostileShip,
    encounter: &Encounter,
    target: Option<&RigView>,
    mates: &[HostileShip],
    leader: Option<&HostileShip>,
    step: u64,
    seed: u64,
) -> Order {
    let stats = ship.class.stats();
    let Some(rig) = target else {
        return drift(ship);
    };
    let (dx, dy) = ship.pos.delta(rig.pos);
    let distance = length(dx, dy);
    if ship.mode == ShipMode::Leaving {
        // Away from the rig, nose first.
        let away = if distance > 0.0 {
            (-dx / distance, -dy / distance)
        } else {
            (0.0, 1.0)
        };
        let (ax, ay) = match_velocity(
            ship.vx,
            ship.vy,
            away.0 * stats.top_speed,
            away.1 * stats.top_speed,
        );
        return Order {
            ax,
            ay,
            face: bearing(ship.vx + away.0, ship.vy + away.1),
            fire: false,
        };
    }
    match ship.class {
        ShipClass::Cutter | ShipClass::Marauder => pirate(ship, encounter, rig, (dx, dy)),
        ShipClass::Mite | ShipClass::Stinger => swarm(ship, rig, (dx, dy), mates, step, seed),
        ShipClass::Drone | ShipClass::Lancer => machine(ship, rig, (dx, dy), leader, step),
        // Bosses fly their own patterns (`boss::order`).
        ShipClass::PirateKing | ShipClass::SwarmQueen | ShipClass::Arbiter => drift(ship),
    }
}

/// No target: coast to a stop.
fn drift(ship: &HostileShip) -> Order {
    let (ax, ay) = match_velocity(ship.vx, ship.vy, 0.0, 0.0);
    Order {
        ax,
        ay,
        face: ship.angle,
        fire: false,
    }
}

/// Faces a target `(dx, dy)` away moving `(rvx, rvy)` relative to the ship, leading by `lead`;
/// fires when the nose is within `cone` of the aim and the target within range.
fn aim(
    ship: &HostileShip,
    rel: (f32, f32),
    rel_v: (f32, f32),
    gun: ShotKind,
    lead: f32,
    cone: f32,
) -> (f32, bool) {
    let stats = ship.class.stats();
    let (lx, ly) = lead_point(rel.0, rel.1, rel_v.0, rel_v.1, gun.speed(), lead);
    let face = bearing(lx, ly);
    let off = crate::field::trig::wrap_angle(face - ship.angle).abs();
    let distance = length(rel.0, rel.1);
    (face, off <= cone && distance <= stats.range)
}

fn pirate(ship: &HostileShip, encounter: &Encounter, rig: &RigView, rel: (f32, f32)) -> Order {
    let stats = ship.class.stats();
    let (dx, dy) = rel;
    let distance = length(dx, dy).max(1.0);
    let rel_v = (rig.vx - ship.vx, rig.vy - ship.vy);
    let hailing = matches!(
        encounter.state,
        EncounterState::Closing | EncounterState::Demand { .. }
    );
    if hailing {
        // Close in and hold off at hailing range, nose on the rig, guns quiet.
        let (wx, wy) = hold_range(dx, dy, rig.vx, rig.vy, HAIL_RANGE, stats.top_speed);
        let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
        return Order {
            ax,
            ay,
            face: bearing(dx, dy),
            fire: false,
        };
    }
    let gun = stats.gun.unwrap_or(ShotKind::Bolt);
    if ship.class == ShipClass::Cutter {
        // Circle the rig (each cutter its own way round) and strafe it.
        let way = if ship.slot.is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let (tx, ty) = (-dy / distance * way, dx / distance * way);
        let radial = ((distance - CUTTER_ORBIT) * 2.5).clamp(-stats.top_speed, stats.top_speed);
        let tangential = stats.top_speed * 0.65;
        let wx = rig.vx + dx / distance * radial + tx * tangential;
        let wy = rig.vy + dy / distance * radial + ty * tangential;
        let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
        let (face, fire) = aim(ship, rel, rel_v, gun, 0.5, 0.14);
        return Order { ax, ay, face, fire };
    }
    // The marauder hangs back and fires bursts.
    let (wx, wy) = hold_range(dx, dy, rig.vx, rig.vy, MARAUDER_RANGE, stats.top_speed);
    let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
    let (face, fire) = aim(ship, rel, rel_v, gun, 0.6, 0.12);
    Order { ax, ay, face, fire }
}

/// The flock's steering for `ship` among `mates`: separation, alignment and cohesion.
fn flock(ship: &HostileShip, mates: &[HostileShip]) -> (f32, f32) {
    let mut separate = (0.0_f32, 0.0_f32);
    let mut align = (0.0_f32, 0.0_f32);
    let mut centre = (0.0_f32, 0.0_f32);
    let mut count = 0.0_f32;
    for mate in mates {
        if mate.id == ship.id || !matches!(mate.class, ShipClass::Mite | ShipClass::Stinger) {
            continue;
        }
        let (mx, my) = ship.pos.delta(mate.pos);
        let distance = length(mx, my);
        if distance > FLOCK_REACH {
            continue;
        }
        if distance < FLOCK_SEPARATION && distance > 0.0 {
            let push = (FLOCK_SEPARATION - distance) / FLOCK_SEPARATION;
            separate.0 -= mx / distance * push;
            separate.1 -= my / distance * push;
        }
        align.0 += mate.vx;
        align.1 += mate.vy;
        centre.0 += mx;
        centre.1 += my;
        count += 1.0;
    }
    if count == 0.0 {
        return (separate.0 * 1_400.0, separate.1 * 1_400.0);
    }
    let align = (align.0 / count - ship.vx, align.1 / count - ship.vy);
    let centre = (centre.0 / count, centre.1 / count);
    (
        separate.0 * 1_400.0 + align.0 * 0.6 + centre.0 * 0.9,
        separate.1 * 1_400.0 + align.1 * 0.6 + centre.1 * 0.9,
    )
}

fn swarm(
    ship: &HostileShip,
    rig: &RigView,
    rel: (f32, f32),
    mates: &[HostileShip],
    step: u64,
    seed: u64,
) -> Order {
    let stats = ship.class.stats();
    let (dx, dy) = rel;
    let distance = length(dx, dy).max(1.0);
    let (fx, fy) = flock(ship, mates);
    if ship.class == ShipClass::Mite {
        if ship.phase == 1 {
            // Diving: straight at the rig's lead point, top speed.
            let (lx, ly) = lead_point(dx, dy, rig.vx - ship.vx, rig.vy - ship.vy, 1_100.0, 0.4);
            let len = length(lx, ly).max(1.0);
            let wx = rig.vx * 0.5 + lx / len * stats.top_speed;
            let wy = rig.vy * 0.5 + ly / len * stats.top_speed;
            let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
            return Order {
                ax: ax * 1.5,
                ay: ay * 1.5,
                face: bearing(ship.vx, ship.vy),
                fire: false,
            };
        }
        // Circling between dives, a little unsettled.
        let wobble = hash64(seed, 0x5_3A1D, i64::from(ship.id), (step / 30) as i64);
        let way = if wobble & 1 == 0 { 1.0 } else { -1.0 };
        let (tx, ty) = (-dy / distance * way, dx / distance * way);
        let radial = ((distance - MITE_ORBIT) * 2.0).clamp(-stats.top_speed, stats.top_speed);
        let wx = rig.vx + dx / distance * radial + tx * stats.top_speed * 0.5;
        let wy = rig.vy + dy / distance * radial + ty * stats.top_speed * 0.5;
        let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
        return Order {
            ax: ax + fx,
            ay: ay + fy,
            face: bearing(ship.vx, ship.vy),
            fire: false,
        };
    }
    // Stinger: hold off and spit.
    let (wx, wy) = hold_range(dx, dy, rig.vx, rig.vy, STINGER_RANGE, stats.top_speed);
    let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
    let (face, fire) = aim(
        ship,
        rel,
        (rig.vx - ship.vx, rig.vy - ship.vy),
        ShotKind::Spit,
        0.3,
        0.2,
    );
    Order {
        ax: ax + fx,
        ay: ay + fy,
        face,
        fire,
    }
}

fn machine(
    ship: &HostileShip,
    rig: &RigView,
    rel: (f32, f32),
    leader: Option<&HostileShip>,
    step: u64,
) -> Order {
    let stats = ship.class.stats();
    let (dx, dy) = rel;
    let rel_v = (rig.vx - ship.vx, rig.vy - ship.vy);
    let gun = stats.gun.unwrap_or(ShotKind::Pulse);
    let alerted = ship.mode != ShipMode::Patrolling;
    if ship.class == ShipClass::Drone
        && let Some(leader) = leader
        && leader.id != ship.id
    {
        // Keep the slot off the leader (in its frame), matching its motion.
        let (sx, sy) = slot_offset(ship.slot);
        let (ox, oy) = from_ship_frame(leader.angle, sx, sy);
        let (lx, ly) = ship.pos.delta(leader.pos);
        let (gx, gy) = (lx + ox, ly + oy);
        let wx = leader.vx + gx * 3.0;
        let wy = leader.vy + gy * 3.0;
        let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
        if !alerted {
            return Order {
                ax,
                ay,
                face: leader.angle,
                fire: false,
            };
        }
        // Every drone of the formation fires on one beat: the volley.
        let (face, aligned) = aim(ship, rel, rel_v, gun, 1.0, 0.3);
        let beat = (step + u64::from(leader.id)).is_multiple_of(stats.fire_steps);
        return Order {
            ax,
            ay,
            face,
            fire: aligned && beat,
        };
    }
    if !alerted {
        // A leader on patrol (`world/hostiles.rs` steers it along its route).
        return drift(ship);
    }
    // The lancer (or a lone drone) approaches to formation range and turns on the rig.
    let range = if ship.class == ShipClass::Lancer {
        FORMATION_RANGE
    } else {
        CUTTER_ORBIT + 80.0
    };
    let (wx, wy) = hold_range(dx, dy, rig.vx, rig.vy, range, stats.top_speed);
    let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
    let (face, fire) = aim(ship, rel, rel_v, gun, 1.0, 0.25);
    Order { ax, ay, face, fire }
}

/// How long a lancer's lance is telegraphed (steps; 1.2 s).
#[must_use]
pub const fn lance_warning() -> u64 {
    steps(1.2)
}
