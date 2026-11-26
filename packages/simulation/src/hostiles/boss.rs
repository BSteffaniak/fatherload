//! The three bosses: big ships with readable, telegraphed attacks (every warning lasts at
//! least [`super::MIN_TELEGRAPH_STEPS`]), two phases, and no countdowns.
//!
//! * **The Gilded Maw** (the Pirate King's flagship): turns its flank to the rig and holds
//!   there. *Broadside*: its gun ports glow, then a fan of shells comes off the flank facing
//!   the rig (get off its flank, or slip between shells). *Ram*: its engines flare along the
//!   line it faces, then it charges down it. *Escorts*: it calls cutters (at most three).
//!   Below half its hull the fan widens and the cycle quickens.
//! * **The Swarm Queen**: hovers at range facing the rig. *Spore ring*: she swells, then
//!   bursts a ring of slow spores with a gap to slip through. *Lunge*: she marks where the rig
//!   is and lunges there. *Brood*: she hatches mites (at most eight). Below 40 % she rings twice.
//! * **The Arbiter** (the Concord's capital ship): slow, heavily shielded. *Volley*: its two
//!   turrets fire aimed pulses. *Lance*: a red line locks from its prow, then the lance fires
//!   down it. *Launch*: drones (at most four). Once its shield first breaks (or below half its
//!   hull) it fires three lances in a fan.

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

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::{field::trig::wrap_angle, flight::space::hash64};

use super::{
    Attack, HostileShip, ShipClass, ShotKind, Telegraph,
    ai::RigView,
    combat::{Order, bearing, from_ship_frame, hold_range, length, match_velocity, nose},
    steps,
};

/// The Pirate King's broadside range (px).
pub const KING_RANGE: f32 = 560.0;
/// The Swarm Queen's range (px).
pub const QUEEN_RANGE: f32 = 520.0;
/// The Arbiter's range (px).
pub const ARBITER_RANGE: f32 = 660.0;
/// The Warden's range (px; Wave 13 story).
pub const WARDEN_RANGE: f32 = 720.0;

/// How a boss moves this step (when not charging).
#[must_use]
pub fn order(ship: &HostileShip, rig: &RigView) -> Order {
    let stats = ship.class.stats();
    let (dx, dy) = ship.pos.delta(rig.pos);
    let at_rig = bearing(dx, dy);
    let (range, face) = match ship.class {
        ShipClass::PirateKing => {
            // Flank to the rig: whichever side is nearer its current heading. While a ram
            // winds up it turns its prow onto the rig instead.
            let face = if matches!(
                ship.telegraph,
                Some(Telegraph {
                    attack: Attack::Ram,
                    ..
                })
            ) {
                at_rig
            } else {
                let left = wrap_angle(at_rig - FRAC_PI_2);
                let right = wrap_angle(at_rig + FRAC_PI_2);
                if wrap_angle(left - ship.angle).abs() <= wrap_angle(right - ship.angle).abs() {
                    left
                } else {
                    right
                }
            };
            (KING_RANGE, face)
        }
        ShipClass::SwarmQueen => (QUEEN_RANGE, at_rig),
        ShipClass::Warden => (WARDEN_RANGE, at_rig),
        _ => (ARBITER_RANGE, at_rig),
    };
    let (wx, wy) = hold_range(dx, dy, rig.vx, rig.vy, range, stats.top_speed);
    let (ax, ay) = match_velocity(ship.vx, ship.vy, wx, wy);
    Order {
        ax,
        ay,
        face,
        fire: false,
    }
}

/// A boss's phase for its hull and shield: 1 once it is badly hurt.
#[must_use]
pub fn phase_of(ship: &HostileShip) -> u8 {
    let share = ship.hull_share();
    let line = if ship.class == ShipClass::SwarmQueen {
        0.4
    } else {
        0.5
    };
    let broken = matches!(ship.class, ShipClass::Arbiter | ShipClass::Warden) && ship.shield <= 0.0;
    u8::from(share < line || broken)
}

/// The attack cycle of `class` in `phase`.
#[must_use]
pub const fn cycle(class: ShipClass, phase: u8) -> &'static [Attack] {
    match (class, phase) {
        (ShipClass::PirateKing, 0) => &[
            Attack::Broadside,
            Attack::Escorts,
            Attack::Broadside,
            Attack::Ram,
        ],
        (ShipClass::PirateKing, _) => &[
            Attack::Broadside,
            Attack::Ram,
            Attack::Broadside,
            Attack::Escorts,
            Attack::Broadside,
        ],
        (ShipClass::SwarmQueen, 0) => &[
            Attack::SporeRing,
            Attack::Brood,
            Attack::Lunge,
            Attack::SporeRing,
        ],
        (ShipClass::SwarmQueen, _) => &[
            Attack::SporeRing,
            Attack::Lunge,
            Attack::SporeRing,
            Attack::Brood,
            Attack::Lunge,
        ],
        // The Warden (Wave 13 story): its pods' volleys, a lance from its prow pod, a ring of
        // lamp-light with a gap to slip through, and drones; the cycle quickens below half.
        (ShipClass::Warden, 0) => &[
            Attack::Volley,
            Attack::Lance,
            Attack::SporeRing,
            Attack::Volley,
            Attack::Launch,
        ],
        (ShipClass::Warden, _) => &[
            Attack::Lance,
            Attack::SporeRing,
            Attack::Volley,
            Attack::Lance,
            Attack::Launch,
            Attack::SporeRing,
        ],
        (_, 0) => &[
            Attack::Volley,
            Attack::Lance,
            Attack::Volley,
            Attack::Launch,
        ],
        (_, _) => &[
            Attack::Lance,
            Attack::Volley,
            Attack::Lance,
            Attack::Launch,
            Attack::Volley,
        ],
    }
}

/// How long `attack`'s warning lasts (steps; never under [`super::MIN_TELEGRAPH_STEPS`]).
#[must_use]
pub const fn warning(attack: Attack) -> u64 {
    let wanted = match attack {
        Attack::Broadside => steps(1.25),
        Attack::Ram => steps(1.6),
        Attack::Escorts | Attack::Brood | Attack::Launch => steps(1.0),
        Attack::SporeRing => steps(1.1),
        Attack::Lunge => steps(1.3),
        Attack::Lance => steps(1.5),
        Attack::Volley => steps(0.9),
    };
    if wanted < super::MIN_TELEGRAPH_STEPS {
        super::MIN_TELEGRAPH_STEPS
    } else {
        wanted
    }
}

/// The pause after an attack before the next warning (steps).
#[must_use]
pub const fn recovery(class: ShipClass, phase: u8) -> u64 {
    match (class, phase) {
        (ShipClass::Arbiter | ShipClass::Warden, 0) => steps(1.8),
        (_, 0) => steps(1.6),
        _ => steps(1.1),
    }
}

/// Begins `ship`'s next attack at `step` against `rig`: the warning, locked to the rig's
/// bearing and position now.
#[must_use]
pub fn begin(ship: &HostileShip, rig: &RigView, step: u64) -> Telegraph {
    let phase = phase_of(ship);
    let attacks = cycle(ship.class, phase);
    let attack = attacks[usize::from(ship.cycle) % attacks.len()];
    let (dx, dy) = ship.pos.delta(rig.pos);
    Telegraph {
        attack,
        since: step,
        fire_at: step + warning(attack),
        angle: bearing(dx, dy),
        mark: rig.pos,
    }
}

/// A shot a boss's attack fires: from an offset of the boss's centre (world axes), along a
/// direction, of a kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volley {
    pub kind: ShotKind,
    pub from: (f32, f32),
    pub angle: f32,
}

/// What an attack does when its warning ends.
#[derive(Clone, Debug, PartialEq)]
pub enum Unleash {
    Shots(Vec<Volley>),
    /// Ships to launch (class, how many, most alive at once).
    Launch(ShipClass, u8, usize),
    /// Charge along the nose at `speed` px/s for `steps`.
    Charge {
        speed: f32,
        steps: u64,
    },
}

/// What `ship`'s `telegraph` does when it fires, with the rig at `rig` (`seed` for the spore
/// ring's gap).
#[must_use]
#[allow(clippy::too_many_lines, reason = "every attack's shots in one match")]
pub fn unleash(ship: &HostileShip, telegraph: &Telegraph, rig: &RigView, seed: u64) -> Unleash {
    let phase = phase_of(ship);
    match telegraph.attack {
        Attack::Broadside => {
            // Shells off the flank facing the rig, fanned round its bearing at the warning.
            let (dx, dy) = ship.pos.delta(rig.pos);
            let (rx, _) = super::combat::to_ship_frame(ship.angle, dx, dy);
            let side = if rx >= 0.0 { 1.0 } else { -1.0 };
            let count: u8 = if phase == 0 { 9 } else { 13 };
            let spread = if phase == 0 { 0.9 } else { 1.2 };
            let shots = (0..count)
                .map(|index| {
                    let share = f32::from(index) / f32::from(count - 1) - 0.5;
                    // The gun ports down its flank (the drawn ports: art rows 44-124).
                    let port_y = share * 160.0 + 24.0;
                    let from = from_ship_frame(ship.angle, side * 46.0, port_y);
                    Volley {
                        kind: ShotKind::Broadside,
                        from,
                        angle: telegraph.angle + share * spread,
                    }
                })
                .collect();
            Unleash::Shots(shots)
        }
        Attack::Ram => Unleash::Charge {
            speed: 1_350.0,
            steps: steps(0.9),
        },
        Attack::Escorts => Unleash::Launch(ShipClass::Cutter, 2, 3),
        Attack::SporeRing => {
            let count = 20_u8;
            let gap_at = (hash64(seed, 0x5_70E1, i64::from(ship.id), telegraph.since as i64)
                % u64::from(count)) as u8;
            let gap = if phase == 0 { 5 } else { 4 };
            let shots = (0..count)
                .filter(|index| (index + count - gap_at) % count >= gap)
                .map(|index| {
                    let angle = telegraph.angle + f32::from(index) / f32::from(count) * TAU;
                    let (nx, ny) = nose(angle);
                    // The Warden's ring leaves its rim (Wave 13 story), the Queen's her body.
                    let rim = if ship.class == ShipClass::Warden {
                        150.0
                    } else {
                        70.0
                    };
                    Volley {
                        kind: ShotKind::Spore,
                        from: (nx * rim, ny * rim),
                        angle,
                    }
                })
                .collect();
            Unleash::Shots(shots)
        }
        Attack::Lunge => Unleash::Charge {
            speed: 1_250.0,
            steps: {
                let (dx, dy) = ship.pos.delta(telegraph.mark);
                // Far enough to reach the mark (and a little past), never forever.
                let reach = length(dx, dy) + 80.0;
                ((reach / 1_250.0) * super::COMBAT_STEP_HZ as f32).clamp(30.0, 150.0) as u64
            },
        },
        Attack::Brood => Unleash::Launch(ShipClass::Mite, 4, 8),
        Attack::Launch => Unleash::Launch(ShipClass::Drone, 2, 4),
        Attack::Lance => {
            let prow_y = if ship.class == ShipClass::Warden {
                -130.0
            } else {
                -150.0
            };
            let prow = from_ship_frame(ship.angle, 0.0, prow_y);
            let mut shots = vec![Volley {
                kind: ShotKind::Lance,
                from: prow,
                angle: telegraph.angle,
            }];
            if phase == 1 && matches!(ship.class, ShipClass::Arbiter | ShipClass::Warden) {
                for side in [-0.16_f32, 0.16] {
                    shots.push(Volley {
                        kind: ShotKind::Lance,
                        from: prow,
                        angle: telegraph.angle + side,
                    });
                }
            }
            Unleash::Shots(shots)
        }
        Attack::Volley => {
            let mut shots = Vec::new();
            // The Arbiter's two turrets; the Warden's two lower lamp pods (Wave 13 story).
            let (turret_x, turret_y) = if ship.class == ShipClass::Warden {
                (95.0_f32, 81.0)
            } else {
                (62.0, -20.0)
            };
            for turret in [-turret_x, turret_x] {
                let from = from_ship_frame(ship.angle, turret, turret_y);
                let (dx, dy) = ship.pos.offset(from.0, from.1).delta(rig.pos);
                let at = bearing(dx, dy);
                for index in 0..4_u8 {
                    shots.push(Volley {
                        kind: ShotKind::Pulse,
                        from,
                        angle: at + (f32::from(index) - 1.5) * 0.07,
                    });
                }
            }
            Unleash::Shots(shots)
        }
    }
}

/// The angle a lance's warning line points (for the renderer): the locked angle.
#[must_use]
pub const fn lance_line(telegraph: &Telegraph) -> f32 {
    telegraph.angle
}

/// Whether `angle` turns more than half a turn from `other` (helper for the renderer's arcs).
#[must_use]
pub fn opposite(angle: f32, other: f32) -> bool {
    wrap_angle(angle - other).abs() > PI * 0.5
}
