//! Side-view tomb guardians (WP-TOMBS T28): the Sunwrought Colossus and the Hollow Matriarch.
//!
//! Both are ordinary [`EnemyKind`]s (damage, drops, saves, replication and the body rules are
//! the shared enemy machinery); this module gives them their attack patterns and phases. The
//! phase is a pure function of health ([`guardian_phase`]); which move an attack is comes from
//! [`guardian_move`], a pure function of the kind, the phase and the wind-up's deadline, so the
//! wind-up (and the renderer's telegraph) and the release always agree.
//!
//! * **Sunwrought Colossus** (walks): phase 1 crouches and leaps at its prey, and the landing
//!   throws rubble both ways ([`GuardianMove::LeapSlam`]). Phase 2 adds a thrown volley of
//!   rubble ([`GuardianMove::RubbleToss`]) and raises up to [`MAX_RAISED`] mummies
//!   ([`GuardianMove::RaiseDead`]). Phase 3 enrages: faster, shorter cooldowns.
//! * **Hollow Matriarch** (floats): phase 1 hangs above to one side and throws fans of hexes
//!   ([`GuardianMove::HexFan`]). Phase 2 adds rings of hexes ([`GuardianMove::HexRing`]) and
//!   crypt bats ([`GuardianMove::SummonBats`]). Phase 3 adds a dive through its prey
//!   ([`GuardianMove::Dive`], hurts on contact).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "world pixels and small counts"
)]

use crate::{
    enemy::{EnemyKind, EnemyMode, EnemyShotState, EnemyState, ShotKind, tier_scale},
    entity::{Body, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::SimulationTick,
    physics,
    quantize::quantize_simulation_scalar as quantize,
    terrain::TILE_SIZE,
    tomb::{side_view::guardian_phase, traps::world_millis},
};

use super::{Intent, MAX_ENEMIES, MAX_SHOTS, RECOVER_MILLIS, WorldState, direction};

/// Most mummies a colossus keeps raised.
pub const MAX_RAISED: usize = 2;
/// Most crypt bats a matriarch keeps about her.
pub const MAX_BATS: usize = 3;
/// The colossus' leap: upward speed (px/s) and how long it flies before it slams.
const LEAP_SPEED: f32 = 430.0;
const LEAP_MILLIS: u64 = 720;
/// The matriarch's dive lasts this long.
const DIVE_MILLIS: u64 = 650;
/// Where the matriarch hovers: columns to the side, rows above her prey.
const HOVER: (f32, f32) = (TILE_SIZE * 4.0, TILE_SIZE * 3.5);
/// Eight directions of a hex ring (unit vectors, no trigonometry).
const RING8: [(f32, f32); 8] = {
    use std::f32::consts::FRAC_1_SQRT_2 as D;
    [
        (1.0, 0.0),
        (D, D),
        (0.0, 1.0),
        (-D, D),
        (-1.0, 0.0),
        (-D, -D),
        (0.0, -1.0),
        (D, -D),
    ]
};

/// One of a guardian's moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardianMove {
    LeapSlam,
    RubbleToss,
    RaiseDead,
    HexFan,
    HexRing,
    SummonBats,
    Dive,
}

/// Whether `kind` is a side-view tomb guardian with patterns of its own.
#[must_use]
pub const fn is_tomb_guardian(kind: EnemyKind) -> bool {
    matches!(kind, EnemyKind::SunColossus | EnemyKind::HollowMatriarch)
}

/// The move a guardian of `kind` at health `fraction` makes with the wind-up ending at world
/// millisecond `deadline` (a rotation by the second, so it is the same at any tick rate).
#[must_use]
pub fn guardian_move(kind: EnemyKind, fraction: f32, deadline: u64) -> GuardianMove {
    let phase = guardian_phase(fraction);
    let turn = usize::try_from(deadline / 1_000).unwrap_or(0);
    match (kind, phase) {
        (EnemyKind::HollowMatriarch, 1) => GuardianMove::HexFan,
        (EnemyKind::HollowMatriarch, 2) => [
            GuardianMove::HexFan,
            GuardianMove::HexRing,
            GuardianMove::SummonBats,
        ][turn % 3],
        (EnemyKind::HollowMatriarch, _) => [
            GuardianMove::Dive,
            GuardianMove::HexRing,
            GuardianMove::HexFan,
            GuardianMove::SummonBats,
        ][turn % 4],
        (_, 1) => GuardianMove::LeapSlam,
        _ => [
            GuardianMove::LeapSlam,
            GuardianMove::RubbleToss,
            GuardianMove::RaiseDead,
        ][turn % 3],
    }
}

impl WorldState {
    const fn world_millis_at(&self, tick: SimulationTick) -> u64 {
        world_millis(tick.get(), self.timing.rate.hz())
    }

    /// The guardian's move for the wind-up it is in (or would start now).
    fn current_move(&self, state: &EnemyState) -> GuardianMove {
        guardian_move(
            state.kind,
            state.health_fraction(),
            self.world_millis_at(state.mode_until),
        )
    }

    /// How many living creatures `parent` raised.
    fn raised_by(&self, parent: EntityId) -> usize {
        self.enemies()
            .filter(|(_, state)| state.parent == Some(parent.get()))
            .count()
    }

    /// A guardian's chase: where it goes, and when it starts a wind-up.
    #[allow(
        clippy::too_many_arguments,
        reason = "the chase arm's own arguments, like the sky chase"
    )]
    pub(super) fn guardian_chase(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        (tx, ty): (f32, f32),
        distance: f32,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let phase = guardian_phase(state.health_fraction());
        let ready = now >= state.attack_ready;
        let clear = physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
        let range = spec.attack_range_tiles * TILE_SIZE;
        if ready && distance <= range && clear {
            self.start_windup(id, body, state, now, output);
            return Intent::default();
        }
        match state.kind {
            EnemyKind::HollowMatriarch => {
                // Hang above and to one side, switching sides every few seconds.
                let side = if (self.world_millis_at(now) / 3_500).is_multiple_of(2) {
                    -1.0
                } else {
                    1.0
                };
                Intent {
                    goal: Some((side * HOVER.0 + tx, ty - HOVER.1)),
                    speed: spec.speed * if phase >= 3 { 1.3 } else { 1.0 },
                }
            }
            _ => Intent {
                goal: Some((tx, ty)),
                speed: spec.speed * if phase >= 3 { 1.35 } else { 1.0 },
            },
        }
    }

    /// The end of a guardian's wind-up: its move.
    pub(super) fn guardian_release(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        aim: (f32, f32),
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = state.kind.stats();
        let phase = guardian_phase(state.health_fraction());
        let mut cooldown = self.attack_cooldown_millis(state.kind);
        if phase >= 3 {
            cooldown = cooldown * 3 / 5;
        }
        let action = self.current_move(state);
        state.attack_ready = self.after(now, cooldown);
        state.mode = EnemyMode::Recover;
        state.mode_until = self.after(now, RECOVER_MILLIS);
        let ((nx, ny), distance) = direction((body.x, body.y), aim);
        match action {
            GuardianMove::LeapSlam => {
                // Leap to land on the prey: cover the distance over the flight.
                let flight = LEAP_MILLIS as f32 / 1_000.0;
                let run = ((aim.0 - body.x) / flight).clamp(-spec.dash_speed, spec.dash_speed);
                body.velocity_x = quantize(run);
                body.velocity_y = quantize(-LEAP_SPEED);
                state.mode = EnemyMode::Strike;
                state.mode_until = self.after(now, LEAP_MILLIS);
            }
            GuardianMove::RubbleToss => {
                self.fire(id, body, state, ShotKind::Rubble, 3, aim, output);
            }
            GuardianMove::RaiseDead => {
                let raised = self.raised_by(id);
                for side in [-1.0_f32, 1.0] {
                    if raised >= MAX_RAISED
                        || self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES
                    {
                        break;
                    }
                    let at = (side.mul_add(TILE_SIZE * 2.5, body.x), body.y);
                    let _ = self.spawn_enemy_with(EnemyKind::Mummy, at, state.tier, None, Some(id));
                }
            }
            GuardianMove::HexFan => {
                self.fire(id, body, state, ShotKind::Hex, 3, aim, output);
            }
            GuardianMove::HexRing => {
                self.shot_ring(id, body, state, ShotKind::Hex, output);
            }
            GuardianMove::SummonBats => {
                let raised = self.raised_by(id);
                for index in 0..2 {
                    if raised + index >= MAX_BATS
                        || self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES
                    {
                        break;
                    }
                    let side = if index == 0 { -1.0 } else { 1.0 };
                    let at = ((side * TILE_SIZE).mul_add(1.5, body.x), body.y - TILE_SIZE);
                    let _ =
                        self.spawn_enemy_with(EnemyKind::CryptBat, at, state.tier, None, Some(id));
                }
            }
            GuardianMove::Dive => {
                let dash = spec.dash_speed;
                #[allow(clippy::cast_sign_loss, reason = "a positive fraction of a second")]
                let reach = (distance / dash.max(1.0) * 1_000.0).max(0.0) as u64 + 150;
                body.velocity_x = quantize(nx * dash);
                body.velocity_y = quantize(ny * dash);
                state.mode = EnemyMode::Strike;
                state.mode_until = self.after(now, DIVE_MILLIS.max(reach.min(1_100)));
            }
        }
    }

    /// Before its timers: a colossus whose leap ends this tick slams down and throws rubble
    /// both ways.
    pub(super) fn guardian_tick(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if state.kind != EnemyKind::SunColossus
            || state.mode != EnemyMode::Strike
            || now < state.mode_until
        {
            return;
        }
        let damage = ShotKind::Rubble.damage() * tier_scale(state.tier);
        let shots: &[(f32, f32)] = if guardian_phase(state.health_fraction()) >= 2 {
            &[
                (150.0, -260.0),
                (260.0, -190.0),
                (-150.0, -260.0),
                (-260.0, -190.0),
            ]
        } else {
            &[(200.0, -230.0), (-200.0, -230.0)]
        };
        self.spawn_shots(id, body, state, ShotKind::Rubble, damage, shots, output);
    }

    /// A ring of eight shots around the shooter.
    fn shot_ring(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        shot: ShotKind,
        output: &mut TickOutput,
    ) {
        let damage = shot.damage() * tier_scale(state.tier);
        let speed = shot.speed();
        let velocities: Vec<(f32, f32)> = RING8
            .iter()
            .map(|(dx, dy)| (dx * speed, dy * speed))
            .collect();
        self.spawn_shots(id, body, state, shot, damage, &velocities, output);
    }

    /// Shots of `shot` leaving the shooter's edge at the given velocities.
    #[allow(clippy::too_many_arguments, reason = "one volley's whole description")]
    fn spawn_shots(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        shot: ShotKind,
        damage: f32,
        velocities: &[(f32, f32)],
        output: &mut TickOutput,
    ) {
        if self.entities.count_of(EntityKind::EnemyShot) >= MAX_SHOTS {
            return;
        }
        let lifetime = self.timing.ticks_for_millis(shot.life_millis()).max(1);
        let expires = SimulationTick::new(self.simulation_tick.get().saturating_add(lifetime));
        let muzzle = state.kind.stats().radius + shot.radius() + 1.0;
        for (vx, vy) in velocities {
            let length = vx.hypot(*vy).max(1.0);
            let (x, y) = (
                quantize((vx / length).mul_add(muzzle, body.x)),
                quantize((vy / length).mul_add(muzzle, body.y)),
            );
            let _ = self.spawn_entity(
                Body::at(x, y).moving(quantize(*vx), quantize(*vy)),
                EntityState::EnemyShot(EnemyShotState {
                    kind: shot,
                    damage,
                    shooter: state.kind,
                }),
                Some(expires),
            );
        }
        output.push_event(GameplayEventKind::EnemyFired {
            entity: id.get(),
            shot,
            x: body.x.round() as i32,
            y: body.y.round() as i32,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_guardian_has_distinct_patterns_per_phase() {
        let moves = |kind, fraction| -> std::collections::BTreeSet<String> {
            (0..12)
                .map(|second| format!("{:?}", guardian_move(kind, fraction, second * 1_000)))
                .collect()
        };
        assert_eq!(moves(EnemyKind::SunColossus, 1.0).len(), 1);
        assert_eq!(moves(EnemyKind::SunColossus, 0.5).len(), 3);
        assert_eq!(moves(EnemyKind::HollowMatriarch, 1.0).len(), 1);
        assert_eq!(moves(EnemyKind::HollowMatriarch, 0.5).len(), 3);
        assert!(moves(EnemyKind::HollowMatriarch, 0.2).contains("Dive"));
        assert!(
            moves(EnemyKind::SunColossus, 0.5).is_disjoint(&moves(EnemyKind::HollowMatriarch, 0.5))
        );
    }
}
