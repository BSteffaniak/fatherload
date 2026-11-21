//! Crypt creatures on the room grid (WP-TOMBS T29; kinds in [`crate::enemy`], spawn tables in
//! [`crate::tomb::crypt`]).
//!
//! Seen from above there is no gravity: every crypt creature steers toward its goal through
//! budgeted tile paths (`route`, the same search the cave creatures use) and moves with the
//! shared swept body step, so the body rules hold exactly as elsewhere: never inside a wall
//! (`free_from_rock` at the start of each tick), never overlapping a pilot (`keep_off_bodies`
//! after moving), contact reaching only `CONTACT_MARGIN` past touching. **Spectres are the one
//! exception by design**: they drift straight through crypt masonry (no terrain collision, no
//! line-of-sight test), stay inside their floor, cannot start an attack from inside a wall,
//! and are still pushed off pilots.
//!
//! * **Skeletons** hunt in packs: each holds a flank of its target (by its seed) until its
//!   attack is ready, then rattles (the wind-up) and lunges.
//! * **Spectres** drift through walls toward their prey and swoop the same way.
//! * **Cultists** keep five tiles off, circle in sight, and cast a hex bolt after glowing
//!   hands (a fan of three every third cast).
//! * **Golems** plod in, raise their fists (a ring marks the reach) and slam everything
//!   around them.
//! * **The crypt lord** wakes when the party walks into its hall and fights in three phases
//!   ([`lord_phase`]: its health share), each cycling its own pattern ([`lord_patterns`]: a fan
//!   of five bolts, a ring of twelve, a charge across the hall, a spiral of three rotating
//!   waves). Every attack is telegraphed ([`GameplayEventKind::CryptTell`] with its aim), and
//!   each new phase roars, pauses and calls creatures up out of the floor (skeletons, then
//!   spectres).
//!
//! Directions come from a fixed table (no trigonometry), so every platform agrees bit for bit.

use super::{
    BURN_DAMAGE_PER_SECOND, Body, CONTACT_MARGIN, EntityId, EntityKind, EntityState, EventShape,
    GameplayEventKind, Intent, LOSE_TARGET_MILLIS, MAX_ENEMIES, PILOT_HALF_WIDTH, PlayerId,
    RECOVER_MILLIS, SimulationTick, THINK_MILLIS, TILE_SIZE, TickOutput, WorldState, direction,
    enemy_event, physics, quantize, tier_scale,
};
use crate::{
    enemy::{EnemyKind, EnemyMode, EnemyShotState, EnemyState, ShotKind},
    physics::BodyParams,
    tomb::crypt::{CryptAttack, lord_patterns, lord_phase},
};

/// Unit vectors every 15 degrees, clockwise from east (+y down).
#[allow(
    clippy::approx_constant,
    reason = "a literal table of cos and sin values"
)]
const DIRS: [(f32, f32); 24] = [
    (1.0, 0.0),
    (0.965_926, 0.258_819),
    (0.866_025, 0.5),
    (0.707_107, 0.707_107),
    (0.5, 0.866_025),
    (0.258_819, 0.965_926),
    (0.0, 1.0),
    (-0.258_819, 0.965_926),
    (-0.5, 0.866_025),
    (-0.707_107, 0.707_107),
    (-0.866_025, 0.5),
    (-0.965_926, 0.258_819),
    (-1.0, 0.0),
    (-0.965_926, -0.258_819),
    (-0.866_025, -0.5),
    (-0.707_107, -0.707_107),
    (-0.5, -0.866_025),
    (-0.258_819, -0.965_926),
    (0.0, -1.0),
    (0.258_819, -0.965_926),
    (0.5, -0.866_025),
    (0.707_107, -0.707_107),
    (0.866_025, -0.5),
    (0.965_926, -0.258_819),
];

/// A golem's slam reaches this far from its centre (px).
pub const SLAM_REACH: f32 = TILE_SIZE * 2.2;
/// A cultist keeps about this far from its prey (px).
const CASTER_RANGE: f32 = TILE_SIZE * 5.0;
/// The lord's pause after an attack.
const LORD_RECOVER_MILLIS: u64 = 550;
/// The lord's roar between phases.
const LORD_ROAR_MILLIS: u64 = 1_000;
/// Gap between the spiral's waves.
const SPIRAL_WAVE_MILLIS: u64 = 240;
/// Waves of the lord's spiral.
const SPIRAL_WAVES: u8 = 3;
/// Longest charge.
const CHARGE_MILLIS: u64 = 1_100;
/// How long a golem stands after a slam.
const SLAM_RECOVER_MILLIS: u64 = 700;

/// `v` turned clockwise by table step `step` (15 degrees each, negative turns back).
fn turn(v: (f32, f32), step: i32) -> (f32, f32) {
    let (c, s) = DIRS[usize::try_from(step.rem_euclid(24)).unwrap_or(0)];
    (v.0.mul_add(c, -v.1 * s), v.0.mul_add(s, v.1 * c))
}

/// The wind-up of the lord's `attack` in `phase` (faster in the last phase).
const fn lord_windup_millis(attack: CryptAttack, phase: u8) -> u64 {
    let base = match attack {
        CryptAttack::Fan => 700,
        CryptAttack::Ring | CryptAttack::Slam => 900,
        CryptAttack::Charge => 800,
        CryptAttack::Spiral => 1_000,
    };
    if phase >= 3 { base * 85 / 100 } else { base }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels become event pixels"
)]
const fn px(value: f32) -> i32 {
    value as i32
}

impl WorldState {
    /// One crypt creature's tick (see the module docs).
    pub(super) fn advance_crypt_enemy(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((start_body, start_state)) = self.enemy_parts(id) else {
            return;
        };
        let mut body = start_body;
        let mut state = start_state;
        let dt = self.timing.delta_seconds;
        if state.burning_until > now {
            state.health = BURN_DAMAGE_PER_SECOND.mul_add(-dt, state.health);
            if state.health <= 0.0 {
                self.kill_enemy(id, body, state, output);
                return;
            }
        }
        self.advance_timers(&mut state, now);
        let think_ticks = self.timing.ticks_for_millis(THINK_MILLIS).max(1);
        if (now.get() + id.get()).is_multiple_of(think_ticks) {
            self.crypt_think(id, &body, &mut state, now, output);
        }
        let ghost = state.kind.passes_walls();
        if !ghost {
            self.free_from_rock(&mut body, state.kind);
        }
        let intent = match state.kind {
            EnemyKind::CryptLord => self.lord_behave(id, &mut body, &mut state, now, output),
            EnemyKind::CryptGolem => self.golem_behave(id, &mut body, &mut state, now, output),
            EnemyKind::Cultist => self.caster_behave(id, &body, &mut state, now, output),
            _ => self.melee_behave(id, &mut body, &mut state, now, output),
        };
        self.crypt_move(id, &mut body, &mut state, intent, now);
        if ghost {
            self.keep_ghost_off(id, &mut body, &state);
        } else {
            self.keep_off_bodies(id, &mut body, &state);
        }
        match state.kind {
            EnemyKind::CryptLord => self.lord_contact(&mut body, &mut state, now, output),
            // The slam is the golem's attack.
            EnemyKind::CryptGolem => {}
            _ => self.contact_attack(&mut body, &mut state, now, output),
        }
        if body != start_body || state != start_state {
            self.entities.update(id, |entity| {
                entity.body = body;
                entity.state = EntityState::Enemy(state);
            });
        }
    }

    /// The nearest standing crypt pilot `body` notices: in sight range with a clear line (a
    /// spectre feels them through walls).
    fn crypt_prey(&self, body: &Body, state: &EnemyState) -> Option<PlayerId> {
        let sight = state.kind.stats().sight_tiles * TILE_SIZE;
        let ghost = state.kind.passes_walls();
        let mut best: Option<(f32, PlayerId)> = None;
        for (_, pilot_body, pilot) in self.pilots() {
            if pilot.view != crate::pilot::PilotView::TopDown || self.tomb_downed(pilot.owner) {
                continue;
            }
            let (x, y) = pilot.middle(pilot_body);
            let distance = (x - body.x).hypot(y - body.y);
            if distance > sight || best.is_some_and(|(nearest, _)| nearest <= distance) {
                continue;
            }
            if ghost || physics::line_clear(&self.terrain, body.x, body.y, x, y) {
                best = Some((distance, pilot.owner));
            }
        }
        best.map(|(_, player)| player)
    }

    /// Where a crypt creature's target stands (its pilot's middle), while it is up.
    fn crypt_target(&self, state: &EnemyState) -> Option<(f32, f32)> {
        let player = state.target?;
        if self.tomb_downed(player) {
            return None;
        }
        self.pilot_of(player)
            .map(|(_, body, pilot)| pilot.middle(body))
    }

    fn crypt_think(
        &self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if matches!(state.mode, EnemyMode::Stunned | EnemyMode::Flee) {
            return;
        }
        let noticed = self.crypt_prey(body, state);
        match (state.mode, noticed) {
            (EnemyMode::Idle | EnemyMode::Dormant | EnemyMode::Return, Some(player)) => {
                state.target = Some(player);
                state.last_seen = now;
                state.mode = EnemyMode::Chase;
                output.push_event(enemy_event(id, state.kind, body, EventShape::Alerted));
                if state.kind == EnemyKind::CryptLord && state.phase == 0 {
                    state.phase = 1;
                    output.push_event(GameplayEventKind::CryptLordPhase {
                        phase: 1,
                        x: px(body.x),
                        y: px(body.y),
                    });
                }
            }
            (EnemyMode::Chase | EnemyMode::Windup | EnemyMode::Strike | EnemyMode::Recover, _) => {
                if let Some(player) = noticed {
                    state.target = Some(player);
                    state.last_seen = now;
                }
                let gone = self.crypt_target(state).is_none();
                let forgotten = now.get().saturating_sub(state.last_seen.get())
                    > self.timing.ticks_for_millis(LOSE_TARGET_MILLIS);
                if (gone || forgotten) && state.mode == EnemyMode::Chase {
                    state.target = noticed;
                    if noticed.is_none() {
                        state.mode = EnemyMode::Idle;
                        state.waypoint = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Starts a telegraphed attack (the generic wind-up event).
    fn crypt_windup(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        millis: u64,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        state.mode = EnemyMode::Windup;
        state.mode_until = self.after(now, millis);
        body.velocity_x = quantize(body.velocity_x * 0.3);
        body.velocity_y = quantize(body.velocity_y * 0.3);
        output.push_event(enemy_event(id, state.kind, body, EventShape::Windup));
    }

    /// Idle drifting near home, fleeing and backing off: what every crypt creature shares.
    fn crypt_common(
        &self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        target: Option<(f32, f32)>,
    ) -> Option<Intent> {
        let speed = state.kind.stats().speed;
        match state.mode {
            EnemyMode::Idle => Some(self.wander(body, state, now)),
            EnemyMode::Return => {
                state.mode = EnemyMode::Idle;
                Some(Intent::default())
            }
            EnemyMode::Dormant | EnemyMode::Stunned | EnemyMode::Strike => Some(Intent::default()),
            EnemyMode::Flee | EnemyMode::Recover => {
                let goal = target.map(|(tx, ty)| {
                    let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                    (
                        nx.mul_add(TILE_SIZE * 3.0, body.x),
                        ny.mul_add(TILE_SIZE * 3.0, body.y),
                    )
                });
                Some(Intent {
                    goal,
                    speed: speed * 0.6,
                })
            }
            EnemyMode::Chase | EnemyMode::Windup => None,
        }
    }

    /// Skeletons and spectres: flank, rattle, lunge.
    fn melee_behave(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let target = self.crypt_target(state);
        if let Some(intent) = self.crypt_common(body, state, now, target) {
            return intent;
        }
        let Some((tx, ty)) = target else {
            state.mode = EnemyMode::Idle;
            return Intent::default();
        };
        state.facing = if tx < body.x { -1 } else { 1 };
        let ((nx, ny), distance) = direction((body.x, body.y), (tx, ty));
        if state.mode == EnemyMode::Windup {
            if now < state.mode_until {
                return Intent::default();
            }
            state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
            body.velocity_x = quantize(nx * spec.dash_speed);
            body.velocity_y = quantize(ny * spec.dash_speed);
            state.mode = EnemyMode::Strike;
            let gap = (distance - state.kind.contact_half().0 - PILOT_HALF_WIDTH).max(0.0);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a dash lasts a fraction of a second"
            )]
            let reach = (gap / spec.dash_speed.max(1.0) * 1_000.0) as u64 + 120;
            state.mode_until = self.after(now, reach.clamp(250, 900));
            return Intent::default();
        }
        let ready = now >= state.attack_ready;
        let ghost = state.kind.passes_walls();
        let in_wall = ghost && physics::collides(&self.terrain, body.x, body.y, spec.radius);
        let clear = ghost || physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
        if ready && distance <= spec.attack_range_tiles * TILE_SIZE && clear && !in_wall {
            self.crypt_windup(id, body, state, spec.windup_millis, now, output);
            return Intent::default();
        }
        // Between attacks a pack holds its flanks round the prey (each its own side).
        let goal = if ready {
            (tx, ty)
        } else {
            let side = DIRS[usize::try_from((state.seed % 8) * 3).unwrap_or(0)];
            let standoff = TILE_SIZE.mul_add(1.6, state.kind.contact_half().0 + PILOT_HALF_WIDTH);
            (side.0.mul_add(standoff, tx), side.1.mul_add(standoff, ty))
        };
        Intent {
            goal: Some(goal),
            speed: spec.speed,
        }
    }

    /// Cultists: keep their distance, circle in sight, cast hex bolts.
    fn caster_behave(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let target = self.crypt_target(state);
        if let Some(intent) = self.crypt_common(body, state, now, target) {
            return intent;
        }
        let Some((tx, ty)) = target else {
            state.mode = EnemyMode::Idle;
            return Intent::default();
        };
        state.facing = if tx < body.x { -1 } else { 1 };
        let (_, distance) = direction((body.x, body.y), (tx, ty));
        let clear = physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
        if state.mode == EnemyMode::Windup {
            if now < state.mode_until {
                return Intent::default();
            }
            state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
            state.volley = state.volley.wrapping_add(1);
            let count = if state.volley.is_multiple_of(3) { 3 } else { 1 };
            self.fire(id, body, state, ShotKind::CryptHex, count, (tx, ty), output);
            state.mode = EnemyMode::Recover;
            state.mode_until = self.after(now, RECOVER_MILLIS);
            return Intent::default();
        }
        if now >= state.attack_ready && distance <= spec.attack_range_tiles * TILE_SIZE && clear {
            let mut moving = *body;
            self.crypt_windup(id, &mut moving, state, spec.windup_millis, now, output);
            return Intent::default();
        }
        let goal = if distance < TILE_SIZE * 3.0 && clear {
            let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
            (
                nx.mul_add(TILE_SIZE * 3.0, body.x),
                ny.mul_add(TILE_SIZE * 3.0, body.y),
            )
        } else if clear && distance <= spec.attack_range_tiles * TILE_SIZE {
            self.orbit((tx, ty), state, now, CASTER_RANGE)
        } else {
            (tx, ty)
        };
        Intent {
            goal: Some(goal),
            speed: spec.speed,
        }
    }

    /// Golems: plod in, raise their fists, slam.
    fn golem_behave(
        &mut self,
        _id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let target = self.crypt_target(state);
        if let Some(intent) = self.crypt_common(body, state, now, target) {
            return intent;
        }
        if state.mode == EnemyMode::Windup {
            if now < state.mode_until {
                return Intent::default();
            }
            self.golem_slam(body, state, now, output);
            return Intent::default();
        }
        let Some((tx, ty)) = target else {
            state.mode = EnemyMode::Idle;
            return Intent::default();
        };
        state.facing = if tx < body.x { -1 } else { 1 };
        let (_, distance) = direction((body.x, body.y), (tx, ty));
        if now >= state.attack_ready && distance <= spec.attack_range_tiles * TILE_SIZE {
            state.mode = EnemyMode::Windup;
            state.mode_until = self.after(now, spec.windup_millis);
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
            output.push_event(GameplayEventKind::CryptTell {
                kind: state.kind,
                attack: CryptAttack::Slam,
                x: px(body.x),
                y: px(body.y),
                aim_x: px(tx),
                aim_y: px(ty),
            });
            return Intent::default();
        }
        Intent {
            goal: Some((tx, ty)),
            speed: spec.speed,
        }
    }

    /// A golem's slam: everyone standing within [`SLAM_REACH`] is struck.
    fn golem_slam(
        &mut self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = state.kind.stats();
        let damage = spec.contact_damage * tier_scale(state.tier);
        for (pilot, _) in self.pilots_near(body.x, body.y, SLAM_REACH) {
            let _ = self.strike_pilot_as(
                Some(state.kind),
                pilot,
                damage,
                (body.x, body.y),
                spec.knockback,
                output,
            );
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few tiles in pixels"
        )]
        output.push_event(GameplayEventKind::CryptSlam {
            x: px(body.x),
            y: px(body.y),
            radius: SLAM_REACH as u16,
        });
        state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
        state.mode = EnemyMode::Recover;
        state.mode_until = self.after(now, SLAM_RECOVER_MILLIS);
    }

    /// The crypt lord (see the module docs).
    #[allow(clippy::too_many_lines, reason = "one arm per pattern")]
    fn lord_behave(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        // A new phase: roar, pause, call up the dead.
        if state.phase > 0 && state.mode != EnemyMode::Strike {
            let phase = lord_phase(state.health_fraction()).max(state.phase);
            if phase > state.phase {
                state.phase = phase;
                state.pattern = 0;
                state.volley = 0;
                state.mode = EnemyMode::Recover;
                state.mode_until = self.after(now, LORD_ROAR_MILLIS);
                body.velocity_x = 0.0;
                body.velocity_y = 0.0;
                output.push_event(GameplayEventKind::CryptLordPhase {
                    phase,
                    x: px(body.x),
                    y: px(body.y),
                });
                let (kind, count) = if phase == 2 {
                    (EnemyKind::CryptSkeleton, 3)
                } else {
                    (EnemyKind::Spectre, 2)
                };
                self.lord_summon(id, body, state, kind, count, output);
                return Intent::default();
            }
        }
        let target = self.crypt_target(state);
        match state.mode {
            EnemyMode::Strike => {
                // The charge ends at a wall.
                if body.velocity_x.abs() + body.velocity_y.abs() < 40.0 {
                    state.mode = EnemyMode::Recover;
                    state.mode_until = self.after(now, LORD_RECOVER_MILLIS);
                }
                return Intent::default();
            }
            EnemyMode::Recover | EnemyMode::Stunned | EnemyMode::Dormant => {
                return Intent::default();
            }
            EnemyMode::Idle | EnemyMode::Return | EnemyMode::Flee => {
                if state.mode != EnemyMode::Idle {
                    state.mode = EnemyMode::Idle;
                }
                return Intent::default();
            }
            EnemyMode::Chase | EnemyMode::Windup => {}
        }
        let Some((tx, ty)) = target else {
            state.mode = EnemyMode::Idle;
            return Intent::default();
        };
        state.facing = if tx < body.x { -1 } else { 1 };
        let ((nx, ny), distance) = direction((body.x, body.y), (tx, ty));
        let patterns = lord_patterns(state.phase.max(1));
        let attack = patterns[usize::from(state.pattern) % patterns.len()];
        if state.mode == EnemyMode::Windup {
            if now < state.mode_until {
                return Intent::default();
            }
            match attack {
                CryptAttack::Fan => {
                    let dirs: Vec<(f32, f32)> = (-2..=2).map(|step| turn((nx, ny), step)).collect();
                    self.lord_volley(id, body, state, &dirs, output);
                }
                CryptAttack::Ring | CryptAttack::Slam => {
                    let offset = usize::from(state.pattern / 2 % 2);
                    let dirs: Vec<(f32, f32)> = (0..12).map(|k| DIRS[k * 2 + offset]).collect();
                    self.lord_volley(id, body, state, &dirs, output);
                }
                CryptAttack::Spiral => {
                    let wave = usize::from(SPIRAL_WAVES.saturating_sub(state.volley));
                    let dirs: Vec<(f32, f32)> = (0..8).map(|k| DIRS[(k * 3 + wave) % 24]).collect();
                    self.lord_volley(id, body, state, &dirs, output);
                    state.volley = state.volley.saturating_sub(1);
                    if state.volley > 0 {
                        state.mode_until = self.after(now, SPIRAL_WAVE_MILLIS);
                        return Intent::default();
                    }
                }
                CryptAttack::Charge => {
                    body.velocity_x = quantize(nx * spec.dash_speed);
                    body.velocity_y = quantize(ny * spec.dash_speed);
                    state.mode = EnemyMode::Strike;
                    state.mode_until = self.after(now, CHARGE_MILLIS);
                    state.pattern = state.pattern.wrapping_add(1);
                    state.attack_ready = self.after(now, self.lord_cooldown(state));
                    return Intent::default();
                }
            }
            state.pattern = state.pattern.wrapping_add(1);
            state.attack_ready = self.after(now, self.lord_cooldown(state));
            state.mode = EnemyMode::Recover;
            state.mode_until = self.after(now, LORD_RECOVER_MILLIS);
            return Intent::default();
        }
        let clear = physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
        if now >= state.attack_ready && clear && distance <= spec.attack_range_tiles * TILE_SIZE {
            state.mode = EnemyMode::Windup;
            state.mode_until = self.after(now, lord_windup_millis(attack, state.phase));
            state.volley = if attack == CryptAttack::Spiral {
                SPIRAL_WAVES
            } else {
                0
            };
            body.velocity_x = quantize(body.velocity_x * 0.2);
            body.velocity_y = quantize(body.velocity_y * 0.2);
            output.push_event(GameplayEventKind::CryptTell {
                kind: state.kind,
                attack,
                x: px(body.x),
                y: px(body.y),
                aim_x: px(tx),
                aim_y: px(ty),
            });
            return Intent::default();
        }
        // Keep a fighting distance: close in from afar, back off when crowded, else circle.
        let goal = if distance > TILE_SIZE * 7.0 || !clear {
            (tx, ty)
        } else if distance < TILE_SIZE * 3.5 {
            (
                (-nx).mul_add(TILE_SIZE * 3.0, body.x),
                (-ny).mul_add(TILE_SIZE * 3.0, body.y),
            )
        } else {
            self.orbit((tx, ty), state, now, TILE_SIZE * 5.5)
        };
        Intent {
            goal: Some(goal),
            speed: spec.speed * if state.phase >= 3 { 1.3 } else { 1.0 },
        }
    }

    /// Milliseconds between the lord's attacks (shorter in its last phase).
    fn lord_cooldown(&self, state: &EnemyState) -> u64 {
        let base = self.attack_cooldown_millis(state.kind);
        if state.phase >= 3 {
            base * 7 / 10
        } else {
            base
        }
    }

    /// Hex bolts out of the lord along `dirs`.
    fn lord_volley(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        dirs: &[(f32, f32)],
        output: &mut TickOutput,
    ) {
        let shot = ShotKind::CryptHex;
        let lifetime = self.timing.ticks_for_millis(shot.life_millis()).max(1);
        let expires = SimulationTick::new(self.simulation_tick.get().saturating_add(lifetime));
        let damage = quantize(shot.damage() * tier_scale(state.tier));
        let muzzle = state.kind.stats().radius + shot.radius() + 2.0;
        for (dx, dy) in dirs {
            let x = quantize(dx.mul_add(muzzle, body.x));
            let y = quantize(dy.mul_add(muzzle, body.y));
            let _shot = self.spawn_entity(
                Body::at(x, y).moving(quantize(dx * shot.speed()), quantize(dy * shot.speed())),
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
            x: px(body.x),
            y: px(body.y),
        });
    }

    /// The lord calls `count` creatures of `kind` up around it (they join its hunt).
    fn lord_summon(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        kind: EnemyKind,
        count: u8,
        output: &mut TickOutput,
    ) {
        let mut called = 0;
        for k in 0..usize::from(count) {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                break;
            }
            let (dx, dy) = DIRS[(k * 24 / usize::from(count.max(1)) + 3) % 24];
            let at = (
                dx.mul_add(TILE_SIZE * 2.5, body.x),
                dy.mul_add(TILE_SIZE * 2.5, body.y),
            );
            let summoned = self.spawn_enemy_with(kind, at, state.tier, None, Some(id));
            let target = state.target;
            let now = self.simulation_tick;
            self.entities.update(summoned, |entity| {
                if let EntityState::Enemy(enemy) = &mut entity.state {
                    enemy.target = target;
                    enemy.last_seen = now;
                    enemy.mode = EnemyMode::Chase;
                }
            });
            called += 1;
        }
        output.push_event(GameplayEventKind::CryptSummon {
            x: px(body.x),
            y: px(body.y),
            count: called,
        });
    }

    /// The lord's charge hurts whoever it runs into.
    fn lord_contact(
        &mut self,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if state.mode != EnemyMode::Strike {
            return;
        }
        let spec = state.kind.stats();
        let (own_w, own_h) = state.kind.contact_half();
        let hit = self.pilots().find_map(|(pilot, pilot_body, pilot_state)| {
            let (mx, my) = pilot_state.middle(pilot_body);
            ((body.x - mx).abs() <= own_w + PILOT_HALF_WIDTH + CONTACT_MARGIN
                && (body.y - my).abs() <= pilot_state.height().mul_add(0.5, own_h) + CONTACT_MARGIN)
                .then_some(pilot)
        });
        let Some(pilot) = hit else {
            return;
        };
        let damage = spec.contact_damage * tier_scale(state.tier) * 1.5;
        if self.strike_pilot_as(
            Some(state.kind),
            pilot,
            damage,
            (body.x, body.y),
            spec.knockback,
            output,
        ) {
            body.velocity_x = quantize(body.velocity_x * 0.2);
            body.velocity_y = quantize(body.velocity_y * 0.2);
            state.mode = EnemyMode::Recover;
            state.mode_until = self.after(now, LORD_RECOVER_MILLIS);
        }
    }

    /// Steering and the body step of a crypt creature (spectres drift through walls).
    fn crypt_move(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        intent: Intent,
        now: SimulationTick,
    ) {
        let spec = state.kind.stats();
        let dt = self.timing.delta_seconds;
        let ghost = state.kind.passes_walls();
        match state.mode {
            EnemyMode::Strike => {}
            EnemyMode::Windup | EnemyMode::Stunned | EnemyMode::Dormant => {
                let keep = 8.0f32.mul_add(-dt, 1.0).max(0.0);
                body.velocity_x = quantize(body.velocity_x * keep);
                body.velocity_y = quantize(body.velocity_y * keep);
            }
            _ => {
                let (gx, gy) = intent.goal.map_or((body.x, body.y), |goal| {
                    if ghost {
                        goal
                    } else {
                        self.route(id, body, state, goal, now)
                    }
                });
                physics::steer_toward(
                    body,
                    gx,
                    gy,
                    intent.speed,
                    intent.speed.mul_add(5.0, 80.0),
                    dt,
                );
            }
        }
        if ghost {
            body.x = body.velocity_x.mul_add(dt, body.x);
            body.y = body.velocity_y.mul_add(dt, body.y);
            if let Some((left, top, right, bottom)) = self.crypt_floor_box(state.home) {
                body.x = body.x.clamp(left, right);
                body.y = body.y.clamp(top, bottom);
            }
            body.x = quantize(body.x);
            body.y = quantize(body.y);
        } else {
            let params = BodyParams {
                radius: spec.radius,
                gravity: 0.0,
                max_speed: 480.0,
                bounce: 0.0,
                rest_speed: 0.0,
                floor_friction: 0.0,
                stop_speed: 0.0,
            };
            let _report = physics::step_body(body, &params, &self.terrain, dt);
        }
        if let Some((gx, _)) = intent.goal
            && (gx - body.x).abs() > 2.0
            && state.mode != EnemyMode::Windup
        {
            state.facing = if gx < body.x { -1 } else { 1 };
        }
    }

    /// The pixel box of the crypt floor holding home tile `home` (its rooms and their walls).
    fn crypt_floor_box(&self, home: (i32, i32)) -> Option<(f32, f32, f32, f32)> {
        let tile = crate::terrain::TilePosition {
            x: home.0,
            y: home.1,
        };
        let run = self.tombs.runs.values().find(|run| {
            run.rooms
                .iter()
                .any(|room| room.interior.expanded(1).contains(tile))
        })?;
        let mut rooms = run.rooms.iter();
        let first = rooms.next()?.interior;
        let (mut min, mut max) = (first.min, first.max);
        for room in rooms {
            min.x = min.x.min(room.interior.min.x);
            min.y = min.y.min(room.interior.min.y);
            max.x = max.x.max(room.interior.max.x);
            max.y = max.y.max(room.interior.max.y);
        }
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        Some((
            min.x as f32 * TILE_SIZE,
            min.y as f32 * TILE_SIZE,
            (max.x + 1) as f32 * TILE_SIZE,
            (max.y + 1) as f32 * TILE_SIZE,
        ))
    }

    /// A spectre touches pilots but never overlaps them: pushed out along the shallower axis
    /// to exactly touching (walls do not stop it).
    fn keep_ghost_off(&self, id: EntityId, body: &mut Body, state: &EnemyState) {
        let (own_w, own_h) = state.kind.contact_half();
        let tie = if id.get().is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        for ((cx, cy), (half_w, half_h)) in self.prey_boxes() {
            let (dx, dy) = (body.x - cx, body.y - cy);
            let (reach_x, reach_y) = (own_w + half_w, own_h + half_h);
            let (depth_x, depth_y) = (reach_x - dx.abs(), reach_y - dy.abs());
            if depth_x <= 0.0 || depth_y <= 0.0 {
                continue;
            }
            if depth_x <= depth_y {
                let side = if dx.abs() < 0.01 { tie } else { dx.signum() };
                body.x = quantize(side.mul_add(reach_x, cx));
                if body.velocity_x * side < 0.0 {
                    body.velocity_x = 0.0;
                }
            } else {
                let side = if dy.abs() < 0.01 { -1.0 } else { dy.signum() };
                body.y = quantize(side.mul_add(reach_y, cy));
                if body.velocity_y * side < 0.0 {
                    body.velocity_y = 0.0;
                }
            }
        }
    }

    /// One crypt shot's tick: hex bolts and arrows fly straight and break on masonry, gates
    /// and fixtures or on the first pilot they meet; boulders roll through pilots (one strike
    /// each, their strike guard does the rest) until a wall stops them. A rolling pilot is not
    /// there to hit.
    pub(super) fn advance_crypt_shot(
        &mut self,
        id: EntityId,
        _now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((mut body, shot)) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::EnemyShot(shot) => Some((entity.body, shot)),
            _ => None,
        }) else {
            return;
        };
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        let travel = (body.velocity_x.abs().max(body.velocity_y.abs()) * dt).max(1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bounded handful of substeps"
        )]
        let steps = ((travel / 4.0).ceil() as u32).clamp(1, 32);
        #[allow(clippy::cast_precision_loss, reason = "a handful of substeps")]
        let step = dt / steps as f32;
        let reach = shot.kind.radius() + PILOT_HALF_WIDTH + 2.0;
        let knock = if shot.kind == ShotKind::Boulder {
            260.0
        } else {
            110.0
        };
        for _ in 0..steps {
            body.x = body.velocity_x.mul_add(step, body.x);
            body.y = body.velocity_y.mul_add(step, body.y);
            if physics::solid_at(&self.terrain, body.x, body.y) {
                self.entities.despawn(id);
                return;
            }
            let victims: Vec<EntityId> = self
                .pilots_near(body.x, body.y, reach)
                .into_iter()
                .map(|(pilot, _)| pilot)
                .filter(|pilot| {
                    !matches!(
                        self.entities.get(*pilot).map(|entity| entity.state),
                        Some(EntityState::Pilot(state)) if state.roll_guarded()
                    )
                })
                .collect();
            for pilot in victims {
                let _landed = self.strike_pilot_as(
                    Some(shot.shooter),
                    pilot,
                    shot.damage,
                    (body.x, body.y),
                    knock,
                    output,
                );
                if shot.kind != ShotKind::Boulder && !self.damage_immune() {
                    self.entities.despawn(id);
                    return;
                }
            }
        }
        body.x = quantize(body.x);
        body.y = quantize(body.y);
        self.entities.update(id, |entity| entity.body = body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_direction_table_is_unit_length_and_turns_add_up() {
        for (x, y) in DIRS {
            assert!((x.hypot(y) - 1.0).abs() < 1e-5);
        }
        let east = (1.0, 0.0);
        let south = turn(east, 6);
        assert!(south.0.abs() < 1e-5 && (south.1 - 1.0).abs() < 1e-5);
        let back = turn(turn(east, 5), -5);
        assert!((back.0 - 1.0).abs() < 1e-5 && back.1.abs() < 1e-5);
    }
}
