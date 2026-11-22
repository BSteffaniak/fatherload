//! The distortion strata's creatures (Wave 9, Descent VI part A): what sets them apart from the
//! generic state machine in `world/enemies.rs`.
//!
//! * **Mirror twins** ([`WorldState::advance_twin`]) are wholly their own: a twin moves as its
//!   rig moves, mirrored about its hall's axis (the rig's velocity with its sideways part
//!   reversed, plus a gentle pull back onto the rig's exact reflection), so rock on its side of
//!   the axis strands it while the rig flies on. It bites when it touches; its copies of the
//!   rig's shots are fired by `world/distortion.rs` from the tick's `WeaponFired` events.
//! * **Gazers** are fixtures: once one notices a rig its eye opens over its wind-up, then while
//!   it sees the rig the rig's controls (and its screen, through the view fields) mirror.
//! * **Tumblers** roll: they fall along the local gravity (sideways or up in a shear hall) and
//!   roll along the ground toward their prey, so a boundary changes which way they go.
//! * **Lodestars** pull every rig within [`LODESTAR_PULL_TILES`] (with a clear line) toward
//!   themselves, then the generic flyer bites.
//! * **Shear mantises** hang from the ceiling (whichever way up is there) until a rig passes
//!   under them, then drop along gravity in a strike.
//! * **Glitchlings** teleport: a ghost shows where one will land ([`EnemyState::waypoint`] over
//!   its wind-up), then it is there, snapped to the rot grid.
//! * **Bitworms** run the grid: straight lines along the axes, square turns toward their prey
//!   or away from a wall.
//! * **Glassbacks** and **dead pixels** use the generic walker and flyer; the glassback's face
//!   throws shots back ([`WorldState::glassback_reflects`]) and a rot pulse rots the view of
//!   the rig it hits (`world/distortion.rs`).
//!
//! Every body follows the body rules: swept `step_body` motion, `free_from_rock`,
//! `keep_off_bodies`, contact boxes sized to the art, line-of-sight noticing.

use crate::{
    distortion::{DistortionEvent, rot},
    enemy::{EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityId},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    physics::{self, BodyParams},
    quantize::quantize_simulation_scalar as quantize,
    terrain::TILE_SIZE,
};

use super::{ENEMY_GRAVITY, EventShape, WorldState, direction, enemy_event};

/// A lodestar pulls rigs within this many tiles.
pub const LODESTAR_PULL_TILES: f32 = 7.0;
/// How fast it drags a rig toward itself at point blank, pixels per second (two fifths of it
/// at its reach): a drift a rig's drive can beat, but only by pushing away.
pub const LODESTAR_PULL: f32 = 110.0;
/// A twin's pull back onto its rig's exact reflection, per second of the gap.
const TWIN_SYNC_RATE: f32 = 2.5;
/// ... at most this fast, pixels per second.
const TWIN_SYNC_MAX: f32 = 90.0;
/// A mantis drops when a rig passes within this many tiles across its line...
const MANTIS_TRIGGER_ACROSS: f32 = 1.6;
/// ... and this many tiles below it.
const MANTIS_TRIGGER_BELOW: f32 = 12.0;
/// How far up (tiles) a mantis looks for a ceiling to hang from.
const MANTIS_CEILING_REACH: i32 = 8;
/// A glitchling lands this many tiles from its prey.
const GLITCH_LAND_TILES: f32 = 2.6;
/// A tumbler's roll acceleration, pixels per second squared.
const TUMBLER_ROLL: f32 = 320.0;
/// A bitworm looks for a turn this often.
const BITWORM_TURN_MILLIS: u64 = 360;

/// The eight landing directions a glitchling tries, in order of preference after its own pick.
const LANDINGS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, -1.0),
    (0.707, -0.707),
    (-0.707, -0.707),
    (0.707, 0.707),
    (-0.707, 0.707),
    (0.0, 1.0),
];

impl WorldState {
    /// Runs a distortion creature's tick when its kind drives itself; `false` leaves it to the
    /// generic state machine (after any extra its kind adds, like a lodestar's pull).
    pub(super) fn advance_distortion_creature(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> bool {
        let Some((start_body, start_state)) = self.enemy_parts(id) else {
            return true;
        };
        let mut body = start_body;
        let mut state = start_state;
        let handled = match state.kind {
            EnemyKind::MirrorTwin => {
                if !self.advance_twin(id, &mut body, &mut state, now, output) {
                    return true;
                }
                true
            }
            EnemyKind::Tumbler => {
                self.advance_tumbler(id, &mut body, &mut state, now, output);
                true
            }
            EnemyKind::Glitchling => {
                self.advance_glitchling(id, &mut body, &mut state, now, output);
                true
            }
            EnemyKind::Bitworm => {
                self.advance_bitworm(id, &mut body, &mut state, now, output);
                true
            }
            EnemyKind::ShearMantis if state.mode == EnemyMode::Dormant => {
                self.hang_mantis(id, &mut body, &mut state, now, output);
                true
            }
            EnemyKind::Lodestar => {
                self.lodestar_pull(&body, &mut state, output);
                if state != start_state {
                    self.entities.update(id, |entity| {
                        entity.state = crate::entity::EntityState::Enemy(state);
                    });
                }
                return false;
            }
            EnemyKind::Gazer => {
                self.advance_gazer(id, &body, &mut state, now, output);
                true
            }
            _ => false,
        };
        if handled && (body != start_body || state != start_state) {
            self.entities.update(id, |entity| {
                entity.body = body;
                entity.state = crate::entity::EntityState::Enemy(state);
            });
        }
        handled
    }

    /// Generic perception on the think ticks (sight with a clear line).
    fn think_on_schedule(
        &self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.advance_timers(state, now);
        let think_ticks = self.timing.ticks_for_millis(super::THINK_MILLIS).max(1);
        if (now.get() + id.get()).is_multiple_of(think_ticks) {
            self.think(id, body, state, now, output);
        }
    }

    // --- Mirror twins ---------------------------------------------------------------------------

    /// A twin's tick; `false` when it faded (it is gone).
    fn advance_twin(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> bool {
        let rig = state
            .target
            .and_then(|player| self.players.get(&player).map(|rig| (player, rig)))
            .filter(|(_, rig)| rig.hull > 0.0)
            .map(|(player, rig)| (player, rig.x, rig.y, rig.velocity_x, rig.velocity_y));
        let settings = *self.terrain.settings();
        let frame = self.frame();
        let zone = crate::distortion::mirror::zone_at(
            &settings,
            &crate::descent::fields::cached_layout(&settings),
            state.home.0,
            state.home.1,
        );
        let in_zone = |x: f32, y: f32| {
            zone.is_some_and(|zone| {
                #[allow(clippy::cast_precision_loss, reason = "frame origins are small")]
                let (column, row) = (
                    frame.origin_col() as f32 + x / TILE_SIZE,
                    frame.origin_row() as f32 + y / TILE_SIZE,
                );
                zone.weight(column, row) > 0.15
            })
        };
        let keep = rig.is_some_and(|(_, x, y, ..)| {
            in_zone(x, y)
                && (x - body.x).hypot(y - body.y) <= crate::distortion::TWIN_LEASH_TILES * TILE_SIZE
        }) && !self.on_foot_rig(state.target);
        let (Some((player, rig_x, rig_y, vx, vy)), Some(zone), true) = (rig, zone, keep) else {
            self.entities.despawn(id);
            output.push_event(distortion_event(
                DistortionEvent::TwinFaded,
                state.target,
                self.frame().tile_at(body.x, body.y),
            ));
            return false;
        };
        let dt = self.timing.delta_seconds;
        // The rig's reflection (pixels): the axis line is the hall's middle.
        let axis_px = (frame.col_px(zone.bounds.min.x) + frame.col_px(zone.bounds.max.x + 1)) * 0.5;
        let reflection = (2.0f32.mul_add(axis_px, -rig_x), rig_y);
        let sync = |gap: f32| (gap * TWIN_SYNC_RATE).clamp(-TWIN_SYNC_MAX, TWIN_SYNC_MAX);
        body.velocity_x = quantize(-vx + sync(reflection.0 - body.x));
        body.velocity_y = quantize(vy + sync(reflection.1 - body.y));
        state.facing = if vx > 1.0 {
            -1
        } else if vx < -1.0 {
            1
        } else {
            state.facing
        };
        state.mode = EnemyMode::Chase;
        state.last_seen = now;
        self.free_from_rock(body, state.kind);
        let params = BodyParams {
            radius: state.kind.stats().radius,
            gravity: 0.0,
            max_speed: 900.0,
            bounce: 0.0,
            rest_speed: 0.0,
            floor_friction: 0.0,
            stop_speed: 0.0,
            gravity_x: 0.0,
            time_scale: 1.0,
        };
        let _report = physics::step_body(body, &params, &self.terrain, dt);
        self.keep_off_bodies(id, body, state);
        let _ = player;
        self.contact_attack(body, state, now, output);
        true
    }

    /// Whether `player` is out of its rig (a twin copies rigs only).
    fn on_foot_rig(&self, player: Option<PlayerId>) -> bool {
        player.is_some_and(|player| self.is_on_foot(player))
    }

    // --- Gazers ---------------------------------------------------------------------------------

    /// A gazer's tick: notice, open its eye over its wind-up, then hold the rig it sees.
    fn advance_gazer(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.think_on_schedule(id, body, state, now, output);
        let target = state
            .target
            .and_then(|player| self.players.get(&player).map(|rig| (player, rig.x, rig.y)));
        let Some((player, x, y)) = target.filter(|_| state.mode.hostile()) else {
            // The eye closes.
            state.volley = 0;
            return;
        };
        let sight = state.kind.stats().sight_tiles * TILE_SIZE;
        let seen = (x - body.x).hypot(y - body.y) <= sight
            && physics::line_clear(&self.terrain, body.x, body.y, x, y);
        if !seen {
            state.volley = 0;
            return;
        }
        if state.volley == 0 {
            // The eye begins to open: the tell.
            state.volley = 1;
            state.attack_ready = self.after(now, state.kind.stats().windup_millis);
            output.push_event(enemy_event(id, state.kind, body, EventShape::Windup));
            return;
        }
        if now < state.attack_ready {
            return;
        }
        let hold = self.after(now, crate::distortion::GAZE_HOLD_MILLIS);
        let fresh = !self.distortion.gazed_at(player, now);
        self.distortion.gazed.insert(player, hold);
        if fresh {
            output.push_event(distortion_event(
                DistortionEvent::Gazed,
                Some(player),
                self.frame().tile_at(x, y),
            ));
        }
        state.facing = if x < body.x { -1 } else { 1 };
    }

    // --- Lodestars ------------------------------------------------------------------------------

    /// A lodestar pulls every rig in reach with a clear line toward itself.
    fn lodestar_pull(&mut self, body: &Body, state: &mut EnemyState, output: &mut TickOutput) {
        if !self.enemies.difficulty.hostile() || state.mode == EnemyMode::Stunned {
            return;
        }
        let reach = LODESTAR_PULL_TILES * TILE_SIZE;
        let dt = self.timing.delta_seconds;
        let immune = self.damage_immune();
        let frame = self.frame();
        let pulled: Vec<(PlayerId, f32, f32)> = self
            .players
            .iter()
            .filter(|(_, rig)| rig.hull > 0.0)
            .filter_map(|(id, rig)| {
                let distance = (rig.x - body.x).hypot(rig.y - body.y);
                (distance <= reach
                    && distance > TILE_SIZE
                    && physics::line_clear(&self.terrain, body.x, body.y, rig.x, rig.y)
                    && !self.enemy_safe_zone(frame.tile_at(rig.x, rig.y)))
                .then_some((*id, rig.x, rig.y))
            })
            .collect();
        let was_pulling = state.volley != 0;
        state.volley = u8::from(!pulled.is_empty());
        if immune {
            return;
        }
        for (player_id, x, y) in pulled {
            let ((nx, ny), distance) = direction((x, y), (body.x, body.y));
            // A drag on the rig's position (its own drive would brake a velocity nudge away),
            // axis by axis and never into rock.
            let drift = LODESTAR_PULL * (0.6f32).mul_add(-distance / reach, 1.0).max(0.0) * dt;
            let (to_x, to_y) = (
                quantize(nx.mul_add(drift, x)),
                quantize(ny.mul_add(drift, y)),
            );
            let terrain = &self.terrain;
            let free_x = !crate::world::movement::authoritative_player_collides(terrain, to_x, y);
            let free_y = !crate::world::movement::authoritative_player_collides(
                terrain,
                if free_x { to_x } else { x },
                to_y,
            );
            if let Some(rig) = self.players.get_mut(&player_id) {
                if free_x {
                    rig.x = to_x;
                }
                if free_y {
                    rig.y = to_y;
                }
            }
            if !was_pulling {
                output.push_event(distortion_event(
                    DistortionEvent::LodestarPull,
                    Some(player_id),
                    frame.tile_at(body.x, body.y),
                ));
            }
        }
    }

    // --- Tumblers -------------------------------------------------------------------------------

    /// A tumbler's tick: fall along the local gravity, roll along the ground toward its prey.
    fn advance_tumbler(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.think_on_schedule(id, body, state, now, output);
        self.free_from_rock(body, state.kind);
        let dt = self.timing.delta_seconds;
        let field = self.fields_at(body.x, body.y);
        let (gx, gy) = (field.gravity_x, field.gravity_y);
        let strength = gx.hypot(gy).max(0.01);
        // Along the ground: perpendicular to gravity.
        let (tx, ty) = (-gy / strength, gx / strength);
        let spec = state.kind.stats();
        let target = self.rig_position(state.target, (body.x, body.y));
        let wanted = match (state.mode, target) {
            (EnemyMode::Chase | EnemyMode::Recover, Some((x, y))) => {
                let along = (x - body.x).mul_add(tx, (y - body.y) * ty);
                if along.abs() < 2.0 {
                    0.0
                } else {
                    along.signum()
                }
            }
            (EnemyMode::Return, _) => {
                let (hx, hy) = self.frame().centre_of(state.home.0, state.home.1);
                let along = (hx - body.x).mul_add(tx, (hy - body.y) * ty);
                if along.abs() < TILE_SIZE {
                    state.mode = EnemyMode::Idle;
                    0.0
                } else {
                    along.signum()
                }
            }
            _ => 0.0,
        };
        let speed_along = body.velocity_x.mul_add(tx, body.velocity_y * ty);
        let accel = if wanted == 0.0 {
            // Rolls to a stop.
            -speed_along.signum() * (TUMBLER_ROLL * 0.5).min(speed_along.abs() / dt.max(1e-3))
        } else if speed_along * wanted < spec.speed {
            wanted * TUMBLER_ROLL
        } else {
            0.0
        };
        body.velocity_x = quantize((tx * accel).mul_add(dt, body.velocity_x));
        body.velocity_y = quantize((ty * accel).mul_add(dt, body.velocity_y));
        if wanted != 0.0 {
            state.facing = if (tx * wanted) < 0.0 { -1 } else { 1 };
        }
        let params = BodyParams {
            radius: spec.radius,
            gravity: ENEMY_GRAVITY,
            max_speed: 480.0,
            bounce: 0.15,
            rest_speed: 60.0,
            floor_friction: 0.0,
            stop_speed: 0.0,
            gravity_x: 0.0,
            time_scale: 1.0,
        }
        .at_fields(&self.terrain, &self.fields, body.x, body.y);
        let before = (body.x, body.y);
        let _report = physics::step_body(body, &params, &self.terrain, dt);
        self.keep_out_of_fluid(state.kind, before, body);
        self.keep_off_bodies(id, body, state);
        self.contact_attack(body, state, now, output);
    }

    // --- Shear mantises -------------------------------------------------------------------------

    /// A dormant mantis: hang from the ceiling (up is against the local gravity), and drop
    /// along gravity onto a rig passing under.
    fn hang_mantis(
        &self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let field = self.fields_at(body.x, body.y);
        let strength = field.gravity_x.hypot(field.gravity_y).max(0.01);
        let (dx, dy) = (field.gravity_x / strength, field.gravity_y / strength);
        if state.volley == 0 {
            // Climb to the ceiling once: the first solid tile up against gravity.
            state.volley = 1;
            let radius = state.kind.stats().radius;
            for step in 1..=MANTIS_CEILING_REACH * 2 {
                #[allow(clippy::cast_precision_loss, reason = "a few half tiles")]
                let reach = step as f32 * TILE_SIZE * 0.5;
                let (x, y) = (dx.mul_add(-reach, body.x), dy.mul_add(-reach, body.y));
                if physics::collides(&self.terrain, x, y, radius) {
                    break;
                }
                body.x = quantize(x);
                body.y = quantize(y);
            }
            body.previous_x = body.x;
            body.previous_y = body.y;
        }
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        // A rig flying into a hanging mantis shoves it aside (the body rules hold).
        self.keep_off_bodies(id, body, state);
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let frame = self.frame();
        let prey = self.players.iter().find_map(|(player_id, rig)| {
            if rig.hull <= 0.0 || self.enemy_safe_zone(frame.tile_at(rig.x, rig.y)) {
                return None;
            }
            let (rx, ry) = (rig.x - body.x, rig.y - body.y);
            let below = rx.mul_add(dx, ry * dy);
            let across = rx.mul_add(-dy, ry * dx).abs();
            (below > 0.0
                && below <= MANTIS_TRIGGER_BELOW * TILE_SIZE
                && across <= MANTIS_TRIGGER_ACROSS * TILE_SIZE
                && physics::line_clear(&self.terrain, body.x, body.y, rig.x, rig.y))
            .then_some(*player_id)
        });
        let Some(player) = prey else {
            return;
        };
        let dash = state.kind.stats().dash_speed;
        body.velocity_x = quantize(dx * dash);
        body.velocity_y = quantize(dy * dash);
        state.mode = EnemyMode::Strike;
        state.target = Some(player);
        state.last_seen = now;
        state.mode_until = self.after(now, 900);
        output.push_event(distortion_event(
            DistortionEvent::MantisDropped,
            Some(player),
            frame.tile_at(body.x, body.y),
        ));
        output.push_event(enemy_event(id, state.kind, body, EventShape::Alerted));
    }

    // --- Glitchlings ----------------------------------------------------------------------------

    /// A glitchling's tick: drift, pick a landing beside its prey on the rot grid, show its
    /// ghost there over the wind-up, then be there.
    fn advance_glitchling(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.think_on_schedule(id, body, state, now, output);
        self.free_from_rock(body, state.kind);
        let dt = self.timing.delta_seconds;
        let spec = state.kind.stats();
        let target = self.rig_position(state.target, (body.x, body.y));
        let mut goal = None;
        match (state.mode, target) {
            (EnemyMode::Chase, Some((tx, ty))) => {
                if state.volley == 1 {
                    // Just landed: a moment to bite, then its cooldown.
                    if now >= state.mode_until {
                        state.volley = 0;
                        state.attack_ready = state
                            .attack_ready
                            .max(self.after(now, spec.cooldown_millis));
                    }
                    goal = Some((tx, ty));
                } else if now >= state.attack_ready {
                    if let Some(landing) = self.glitch_landing(body, state, (tx, ty)) {
                        state.waypoint = Some(landing);
                        state.mode = EnemyMode::Windup;
                        state.mode_until = self.after(now, spec.windup_millis);
                        output.push_event(enemy_event(id, state.kind, body, EventShape::Windup));
                    } else {
                        state.attack_ready = self.after(now, 500);
                    }
                } else {
                    goal = Some((tx, ty));
                }
            }
            (EnemyMode::Windup, _) => {
                if now >= state.mode_until {
                    let landing = state.waypoint.take();
                    if let Some((lx, ly)) = landing {
                        let (x, y) = self.frame().centre_of(lx, ly);
                        if !physics::collides(&self.terrain, x, y, spec.radius) {
                            body.x = quantize(x);
                            body.y = quantize(y);
                            body.previous_x = body.x;
                            body.previous_y = body.y;
                            body.velocity_x = 0.0;
                            body.velocity_y = 0.0;
                            output.push_event(distortion_event(
                                DistortionEvent::GlitchTeleport,
                                state.target,
                                self.frame().tile_at(x, y),
                            ));
                        }
                    }
                    // Back to the hunt: it bites if it landed against its prey.
                    state.mode = EnemyMode::Chase;
                    state.attack_ready = now;
                    state.volley = 1;
                    state.mode_until = self.after(now, 400);
                }
            }
            (EnemyMode::Idle, _) => {
                let (hx, hy) = self.frame().centre_of(state.home.0, state.home.1);
                goal = Some((hx, hy));
            }
            (EnemyMode::Return, _) => {
                // Leashed: it blinks back home and settles (otherwise it would hang where the
                // leash caught it, too far from home ever to take up a hunt again).
                let (hx, hy) = self.frame().centre_of(state.home.0, state.home.1);
                let near = (hx - body.x).hypot(hy - body.y) < TILE_SIZE * 1.5;
                if !near && !physics::collides(&self.terrain, hx, hy, spec.radius) {
                    body.x = quantize(hx);
                    body.y = quantize(hy);
                    body.previous_x = body.x;
                    body.previous_y = body.y;
                    body.velocity_x = 0.0;
                    body.velocity_y = 0.0;
                }
                state.mode = EnemyMode::Idle;
                state.target = None;
                goal = Some((hx, hy));
            }
            _ => {}
        }
        if state.mode == EnemyMode::Windup {
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
        } else {
            let (gx, gy) = goal.unwrap_or((body.x, body.y));
            let speed = if state.mode == EnemyMode::Chase {
                spec.speed * 0.35
            } else {
                spec.speed * 0.3
            };
            physics::steer_toward(body, gx, gy, speed, speed.mul_add(4.0, 60.0), dt);
        }
        let params = super::body_params(state.kind, false).at_fields(
            &self.terrain,
            &self.fields,
            body.x,
            body.y,
        );
        let _report = physics::step_body(body, &params, &self.terrain, dt);
        self.keep_off_bodies(id, body, state);
        self.contact_attack(body, state, now, output);
    }

    /// Where a glitchling lands beside `prey`: on the rot grid there (the stratum's block size
    /// at the prey's row), in open space, as a tile.
    fn glitch_landing(
        &self,
        body: &Body,
        state: &EnemyState,
        prey: (f32, f32),
    ) -> Option<(i32, i32)> {
        let settings = *self.terrain.settings();
        let layout = crate::descent::fields::cached_layout(&settings);
        let frame = self.frame();
        let prey_tile = frame.tile_at(prey.0, prey.1);
        let size = rot::tier_at(&layout, prey_tile.y).map_or(1, rot::block_size);
        let radius = state.kind.stats().radius;
        let start = usize::try_from(u64::from(state.seed) % 8).unwrap_or(0);
        // Prefer the side it is already on.
        let approach = if body.x < prey.0 { -1.0 } else { 1.0 };
        for index in 0..LANDINGS.len() {
            let (across, down) = LANDINGS[(start + index) % LANDINGS.len()];
            let (x, y) = (
                (across * approach * GLITCH_LAND_TILES).mul_add(TILE_SIZE, prey.0),
                (down * GLITCH_LAND_TILES).mul_add(TILE_SIZE, prey.1),
            );
            let tile = frame.tile_at(x, y);
            // Snap to the grid's block (its middle tile).
            let snapped = (
                tile.x.div_euclid(size) * size + size / 2,
                tile.y.div_euclid(size) * size + size / 2,
            );
            let (cx, cy) = frame.centre_of(snapped.0, snapped.1);
            if !physics::collides(&self.terrain, cx, cy, radius)
                && physics::line_clear(&self.terrain, cx, cy, prey.0, prey.1)
            {
                return Some(snapped);
            }
        }
        None
    }

    // --- Bitworms -------------------------------------------------------------------------------

    /// A bitworm's tick: run straight along its axis; turn square corners toward its prey or
    /// away from a wall.
    fn advance_bitworm(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.think_on_schedule(id, body, state, now, output);
        self.free_from_rock(body, state.kind);
        let dt = self.timing.delta_seconds;
        let spec = state.kind.stats();
        let heading = |pattern: u8| match pattern % 4 {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        let (hx, hy) = heading(state.pattern);
        let radius = spec.radius;
        let ahead_blocked = physics::collides(
            &self.terrain,
            (hx * TILE_SIZE).mul_add(0.6, body.x),
            (hy * TILE_SIZE).mul_add(0.6, body.y),
            radius,
        );
        let target = self.rig_position(state.target, (body.x, body.y));
        let hunting = state.mode.hostile() && target.is_some();
        let home = self.bitworm_home(body, state);
        if now >= state.mode_until || ahead_blocked {
            // Due a turn check (square corners only).
            let mut pick = None;
            if let Some((tx, ty)) = target.filter(|_| hunting).or(home) {
                let (dx, dy) = (tx - body.x, ty - body.y);
                let along = dx.mul_add(hx, dy * hy);
                let across_x = -hy;
                let across_y = hx;
                let across = dx.mul_add(across_x, dy * across_y);
                if across.abs() > TILE_SIZE * 1.2 && (along < TILE_SIZE || ahead_blocked) {
                    // Turn toward it.
                    pick = Some(if across > 0.0 {
                        (state.pattern + 1) % 4
                    } else {
                        (state.pattern + 3) % 4
                    });
                } else if along < -TILE_SIZE * 2.0 {
                    pick = Some((state.pattern + 2) % 4);
                }
            }
            if pick.is_none() && ahead_blocked {
                // A wall: try right, then left, then back.
                pick = [1, 3, 2]
                    .into_iter()
                    .map(|turn| (state.pattern + turn) % 4)
                    .find(|candidate| {
                        let (cx, cy) = heading(*candidate);
                        !physics::collides(
                            &self.terrain,
                            (cx * TILE_SIZE).mul_add(0.6, body.x),
                            (cy * TILE_SIZE).mul_add(0.6, body.y),
                            radius,
                        )
                    });
            }
            if let Some(turn) = pick
                && turn != state.pattern
            {
                state.pattern = turn;
                // Back onto the grid line across the new heading (the tile's middle).
                let tile = self.frame().tile_at(body.x, body.y);
                let (cx, cy) = self.frame().centre_of(tile.x, tile.y);
                let (nx, _) = heading(turn);
                let snapped = if nx == 0.0 {
                    (cx, body.y)
                } else {
                    (body.x, cy)
                };
                if !physics::collides(&self.terrain, snapped.0, snapped.1, radius) {
                    body.x = quantize(snapped.0);
                    body.y = quantize(snapped.1);
                }
            }
            state.mode_until = self.after(now, BITWORM_TURN_MILLIS);
        }
        let (hx, hy) = heading(state.pattern);
        let speed = if hunting || home.is_some() {
            spec.speed
        } else {
            spec.speed * 0.5
        };
        body.velocity_x = quantize(hx * speed);
        body.velocity_y = quantize(hy * speed);
        state.facing = if hx < 0.0 { -1 } else { 1 };
        let params = BodyParams {
            radius,
            gravity: 0.0,
            max_speed: 480.0,
            bounce: 0.0,
            rest_speed: 0.0,
            floor_friction: 0.0,
            stop_speed: 0.0,
            gravity_x: 0.0,
            time_scale: 1.0,
        };
        let params = params.at_fields(&self.terrain, &self.fields, body.x, body.y);
        let before = (body.x, body.y);
        let _report = physics::step_body(body, &params, &self.terrain, dt);
        self.keep_out_of_fluid(state.kind, before, body);
        self.keep_off_bodies(id, body, state);
        self.contact_attack(body, state, now, output);
    }

    /// Where a leashed bitworm heads: its home, which it runs the grid back toward and
    /// settles at (`None` unless it is returning).
    fn bitworm_home(&self, body: &Body, state: &mut EnemyState) -> Option<(f32, f32)> {
        if state.mode != EnemyMode::Return {
            return None;
        }
        let (hx, hy) = self.frame().centre_of(state.home.0, state.home.1);
        if (hx - body.x).hypot(hy - body.y) < TILE_SIZE * 2.0 {
            state.mode = EnemyMode::Idle;
        }
        Some((hx, hy))
    }

    // --- Glassbacks -----------------------------------------------------------------------------

    /// Whether a hit from `attacker`'s rig lands on a glassback's mirrored face (the side it is
    /// facing) and flies back at the rig (kinetic and piercing hits only).
    pub(super) fn glassback_reflects(
        &self,
        body: &Body,
        state: &EnemyState,
        attacker: Option<PlayerId>,
        kind: crate::enemy::DamageKind,
    ) -> Option<(f32, f32)> {
        if state.kind != EnemyKind::Glassback
            || !matches!(
                kind,
                crate::enemy::DamageKind::Kinetic | crate::enemy::DamageKind::Piercing
            )
        {
            return None;
        }
        let (x, y) = attacker
            .and_then(|player| self.players.get(&player))
            .map(|rig| (rig.x, rig.y))?;
        let front = (x - body.x) * f32::from(state.facing) > 0.0;
        // From well above (a rig hovering over it) the face cannot catch the shot.
        let above = body.y - y > (x - body.x).abs() * 1.2;
        (front && !above).then_some((x, y))
    }

    /// A glassback throws the shot back at `(x, y)`.
    pub(super) fn reflect_shot(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        at: (f32, f32),
        output: &mut TickOutput,
    ) {
        self.fire(
            id,
            body,
            state,
            crate::enemy::ShotKind::MirrorShard,
            1,
            at,
            output,
        );
        output.push_event(distortion_event(
            DistortionEvent::ShotReflected,
            state.last_attacker,
            self.frame().tile_at(body.x, body.y),
        ));
    }

    /// A twin fires its copy of its rig's shot: from the twin, along the shot's direction
    /// mirrored left to right.
    pub(in crate::world) fn twin_fires(
        &mut self,
        twin: EntityId,
        shot_direction: (f32, f32),
        output: &mut TickOutput,
    ) {
        let Some((body, state)) = self.enemy_parts(twin) else {
            return;
        };
        let aim = (
            (-shot_direction.0).mul_add(TILE_SIZE * 8.0, body.x),
            shot_direction.1.mul_add(TILE_SIZE * 8.0, body.y),
        );
        self.fire(
            twin,
            &body,
            &state,
            crate::enemy::ShotKind::MirrorShard,
            1,
            aim,
            output,
        );
        output.push_event(distortion_event(
            DistortionEvent::TwinFired,
            state.target,
            self.frame().tile_at(body.x, body.y),
        ));
    }
}

/// A distortion event at `tile`.
pub(in crate::world) const fn distortion_event(
    what: DistortionEvent,
    player_id: Option<PlayerId>,
    tile: crate::terrain::TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Distortion {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}
