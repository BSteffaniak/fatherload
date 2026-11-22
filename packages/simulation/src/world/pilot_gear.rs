//! The pilot's gear in the world (WP-PILOT; tiers and the body's rope physics live in
//! [`crate::pilot`]).
//!
//! * **Rope** ([`PilotAction::Rope`]): standing beside its rig, the pilot clips onto the
//!   rig's **winch** cable (every rig has one, [`crate::pilot::WINCH_LENGTH`]): walk off the
//!   edge of a shaft and hold down to be lowered, up to be hauled back. Elsewhere the
//!   **grapple** ([`crate::pilot::PilotGear::grapple`]) fires a hook along the aim; it bites
//!   the first solid tile in reach. Pressing the button again (or jump) lets go. A grapple
//!   whose rock is mined away lets go by itself, and so does a winch whose rig is wrecked.
//! * **Recall** ([`PilotAction::RecallRig`], rig link tier 1): the parked rig flies itself to
//!   the pilot along an open tile path ([`WorldState::rig_recall_waypoint`]: a budgeted
//!   breadth-first search, recomputed each tick, so a path dug or blocked meanwhile is
//!   respected); it burns a little fuel and stops beside the pilot, or gives up when no path
//!   is left.
//! * **Sentry** (rig link tier [`crate::pilot::RIG_SENTRY_TIER`]): a parked rig's mounted gun
//!   fires on creatures near its pilot ([`WorldState::rig_sentry_control`]), from the rig's
//!   own capacitor: the rig covers you while you walk.
//! * **Exposure**: the high cold (V8 skies, [`crate::environment::cold_level`]) and the deep
//!   heat ([`crate::protection::band_heat_level`]) hurt a pilot whose thermal lining or heat
//!   weave does not cover them.
//! * **Armour** soaks strikes and blasts (`strike_pilot`, `pilots_feel_blasts`).

use std::collections::VecDeque;

use crate::{
    entity::Body,
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    input::WeaponControl,
    physics::line_clear,
    pilot::{
        BOARD_REACH, PilotAction, PilotMode, PilotState, ROPE_MIN_LENGTH, RecallOutcome, RopeEvent,
        Tether, TetherKind, WINCH_LENGTH,
    },
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TilePosition},
    weapon::{AUTO_AIM_RANGE, MUZZLE_RISE, aim_vector, quantize_aim},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS, pilot::pilot_blocks};

/// How fast a recalled rig flies (px/s).
pub const RECALL_SPEED: f32 = 260.0;
/// Fuel a recalled rig burns per second of flight.
pub const RECALL_FUEL_PER_SECOND: f32 = 0.6;
/// A recall searches this many tiles around the rig and the pilot (and gives up beyond).
pub const RECALL_SEARCH_MARGIN: i32 = 6;
/// Farthest a rig can be recalled from (tiles, either axis).
pub const RECALL_MAX_TILES: i32 = 64;
/// A recalled rig has arrived once its centre is this close to the pilot's middle (px).
pub const RECALL_ARRIVE_DISTANCE: f32 = BOARD_REACH * 0.8;
/// The sentry only fires on creatures this close to its pilot (px).
pub const SENTRY_COVER: f32 = TILE_SIZE * 9.0;
/// Grapple ray step (px).
const GRAPPLE_STEP: f32 = 4.0;
/// The recall search's four steps (up first, so a rig rises out of a hole before it slides).
const PATH_STEPS: [(i32, i32); 4] = [(0, -1), (1, 0), (-1, 0), (0, 1)];
/// A search-grid tile no step has reached.
const PATH_UNSEEN: u8 = u8::MAX;
/// The search-grid mark of the start tile.
const PATH_START: u8 = 4;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn px(value: f32) -> i32 {
    value.round() as i32
}

impl WorldState {
    /// Where `player_id`'s rig winch cable hangs from: the rig's belly. `None` for a wrecked
    /// or missing rig.
    #[must_use]
    pub fn winch_anchor(&self, player_id: PlayerId) -> Option<(f32, f32)> {
        let player = self.players.get(&player_id)?;
        (player.hull > 0.0).then_some((player.x, player.y + AUTHORITATIVE_PLAYER_RADIUS))
    }

    /// Where a grapple fired by a pilot at `from` along `aim` would bite, within `range`: just
    /// short of the first tile that stops a pilot. `None` if only air is in reach.
    #[must_use]
    pub fn grapple_hit(&self, from: (f32, f32), aim: (f32, f32), range: f32) -> Option<(f32, f32)> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few dozen steps"
        )]
        let steps = (range / GRAPPLE_STEP).ceil() as u32;
        let mut last = from;
        for step in 1..=steps {
            #[allow(clippy::cast_precision_loss, reason = "a few dozen steps")]
            let travelled = step as f32 * GRAPPLE_STEP;
            let point = (
                aim.0.mul_add(travelled, from.0),
                aim.1.mul_add(travelled, from.1),
            );
            let tile = self.frame().tile_at(point.0, point.1);
            if pilot_blocks(&self.terrain, tile) && self.terrain.tile(tile).is_some() {
                return Some((quantize(last.0), quantize(last.1)));
            }
            last = point;
        }
        None
    }

    /// [`PilotAction::Rope`]: let go, take the rig's winch cable, or fire the grapple.
    pub(super) fn pilot_rope(
        &mut self,
        player_id: PlayerId,
        aim: (i8, i8),
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some((id, body, state)) = self.pilot_of(player_id).map(|(id, b, s)| (id, *b, *s))
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let (mx, my) = state.middle(&body);
        if let Some(rope) = state.tether {
            self.edit_pilot(id, |_, state| {
                state.tether = None;
                state.mode = PilotMode::Free;
            });
            output.push_event(GameplayEventKind::PilotRope {
                player_id,
                action: RopeEvent::Released,
                x: px(rope.anchor_x),
                y: px(rope.anchor_y),
            });
            return PlayerScopedCommandOutcome::Applied;
        }
        if matches!(state.mode, PilotMode::Swim | PilotMode::Ledge) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let tether = if self.can_board_rig(player_id) {
            let Some((ax, ay)) = self.winch_anchor(player_id) else {
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            };
            Tether {
                kind: TetherKind::Winch,
                anchor_x: ax,
                anchor_y: ay,
                length: quantize((mx - ax).hypot(my - ay).max(ROPE_MIN_LENGTH)),
                max_length: WINCH_LENGTH,
            }
        } else {
            let range = state.gear.grapple_range();
            if range <= 0.0 {
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            }
            let facing = f32::from(state.facing);
            let aim = aim_vector(aim.0, aim.1)
                .filter(|(_, y)| *y < 0.35)
                .unwrap_or_else(|| {
                    aim_vector(
                        quantize_aim(facing * 0.55, -0.83).map_or(0, |(x, _)| x),
                        quantize_aim(facing * 0.55, -0.83).map_or(-127, |(_, y)| y),
                    )
                    .unwrap_or((0.0, -1.0))
                });
            let hand = (body.x, state.height().mul_add(-0.85, body.y));
            let Some((ax, ay)) = self.grapple_hit(hand, aim, range) else {
                output.push_event(GameplayEventKind::PilotRope {
                    player_id,
                    action: RopeEvent::Missed,
                    x: px(aim.0.mul_add(range, hand.0)),
                    y: px(aim.1.mul_add(range, hand.1)),
                });
                return PlayerScopedCommandOutcome::Applied;
            };
            Tether {
                kind: TetherKind::Grapple,
                anchor_x: ax,
                anchor_y: ay,
                length: quantize((mx - ax).hypot(my - ay).max(ROPE_MIN_LENGTH)),
                max_length: quantize(range * 1.25),
            }
        };
        self.edit_pilot(id, |_, state| {
            state.tether = Some(tether);
            state.mode = PilotMode::Tether;
            state.jetting = false;
            state.hovering = false;
            state.gliding = false;
            state.crouching = false;
        });
        output.push_event(GameplayEventKind::PilotRope {
            player_id,
            action: if tether.kind == TetherKind::Winch {
                RopeEvent::Winched
            } else {
                RopeEvent::Hooked
            },
            x: px(tether.anchor_x),
            y: px(tether.anchor_y),
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// The winch anchor to feed a roped pilot's step this tick, after letting go of ropes
    /// whose anchor is gone (rock mined away, rig wrecked). Returns the step's anchor.
    pub(super) fn refresh_rope(
        &self,
        state: &mut PilotState,
        owner: PlayerId,
        output: &mut TickOutput,
    ) -> Option<(f32, f32)> {
        let rope = state.tether?;
        let anchor = match rope.kind {
            TetherKind::Winch => self.winch_anchor(owner),
            TetherKind::Grapple => {
                // The hook holds while the rock it bit (just beyond the anchor) stands.
                let held = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        let tile = self.frame().tile_at(rope.anchor_x, rope.anchor_y);
                        self.terrain.is_solid_at(TilePosition {
                            x: tile.x + dx,
                            y: tile.y + dy,
                        })
                    })
                });
                held.then_some((rope.anchor_x, rope.anchor_y))
            }
        };
        if anchor.is_none() {
            state.tether = None;
            if state.mode == PilotMode::Tether {
                state.mode = PilotMode::Free;
            }
            output.push_event(GameplayEventKind::PilotRope {
                player_id: owner,
                action: RopeEvent::Released,
                x: px(rope.anchor_x),
                y: px(rope.anchor_y),
            });
        }
        anchor
    }

    // --- Recall -----------------------------------------------------------------------------

    /// The next point a recalled rig of `player_id` flies to: along the shortest open tile
    /// path to its pilot (4-connected, searched in a box around both, see
    /// [`RECALL_SEARCH_MARGIN`]). `None` when there is no path, the pilot is out of range, or
    /// either is missing.
    #[must_use]
    pub fn rig_recall_waypoint(&self, player_id: PlayerId) -> Option<(f32, f32)> {
        let (_, body, state) = self.pilot_of(player_id)?;
        let player = self.players.get(&player_id)?;
        let (mx, my) = state.middle(body);
        let start = self.frame().tile_at(player.x, player.y);
        let goal = self.frame().tile_at(mx, my);
        if (goal.x - start.x).abs() > RECALL_MAX_TILES
            || (goal.y - start.y).abs() > RECALL_MAX_TILES
        {
            return None;
        }
        let open = |tile: TilePosition| {
            self.terrain.contains_column(tile.x)
                && tile.y >= self.terrain.min_y()
                && tile.y < self.terrain.height()
                && !self.terrain.is_solid_at(tile)
        };
        let (left, right) = (
            start.x.min(goal.x) - RECALL_SEARCH_MARGIN,
            start.x.max(goal.x) + RECALL_SEARCH_MARGIN,
        );
        let (top, bottom) = (
            start.y.min(goal.y) - RECALL_SEARCH_MARGIN,
            start.y.max(goal.y) + RECALL_SEARCH_MARGIN,
        );
        // The goal: the pilot's tile or any open tile touching it.
        let is_goal = |tile: TilePosition| (tile.x - goal.x).abs() + (tile.y - goal.y).abs() <= 1;
        // A flat grid over the search box (it runs every tick of a recall, so no map
        // lookups): the step that reached each tile, `PATH_START` for the start, `PATH_UNSEEN`
        // if none.
        let width = right - left + 1;
        let index = |tile: TilePosition| {
            usize::try_from((tile.y - top) * width + (tile.x - left)).unwrap_or(usize::MAX)
        };
        let mut came_by = vec![PATH_UNSEEN; usize::try_from(width * (bottom - top + 1)).ok()?];
        came_by[index(start)] = PATH_START;
        let mut queue = VecDeque::from([start]);
        let mut reached = None;
        while let Some(tile) = queue.pop_front() {
            if is_goal(tile) {
                reached = Some(tile);
                break;
            }
            for (step, (dx, dy)) in (0_u8..).zip(PATH_STEPS) {
                let next = TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                };
                if next.x < left
                    || next.x > right
                    || next.y < top
                    || next.y > bottom
                    || came_by[index(next)] != PATH_UNSEEN
                    || !open(next)
                {
                    continue;
                }
                came_by[index(next)] = step;
                queue.push_back(next);
            }
        }
        let mut tile = reached?;
        // Walk back to the first step after the start.
        let mut path = vec![tile];
        loop {
            let step = came_by[index(tile)];
            if step == PATH_START || step == PATH_UNSEEN {
                break;
            }
            let (dx, dy) = PATH_STEPS[usize::from(step)];
            tile = TilePosition {
                x: tile.x - dx,
                y: tile.y - dy,
            };
            path.push(tile);
        }
        path.reverse();
        if path.len() < 2 {
            return Some((mx, my));
        }
        let here = self.frame().tile_center(start);
        let next = path[1];
        // Line up across the direction of travel first (centred in the corridor), so the rig
        // never clips a corner; along it, head straight for the next tile.
        let off_line = if next.y == start.y {
            (player.y - here.1).abs()
        } else {
            (player.x - here.0).abs()
        };
        if off_line > 2.0 {
            return Some(if next.y == start.y {
                (player.x, here.1)
            } else {
                (here.0, player.y)
            });
        }
        Some(self.frame().tile_center(next))
    }

    /// [`PilotAction::RecallRig`]: the rig flies itself over (or the recall is called off).
    pub(super) fn recall_rig(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some((id, _, state)) = self.pilot_of(player_id).map(|(id, b, s)| (id, *b, *s)) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if state.gear.rig_link == 0
            || self
                .players
                .get(&player_id)
                .is_none_or(|player| player.hull <= 0.0 || player.fuel <= 0.0)
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let outcome = if state.recalling {
            RecallOutcome::NoPath
        } else if self.can_board_rig(player_id) {
            RecallOutcome::Arrived
        } else if self.rig_recall_waypoint(player_id).is_some() {
            RecallOutcome::Started
        } else {
            RecallOutcome::NoPath
        };
        let recalling = outcome == RecallOutcome::Started;
        self.edit_pilot(id, |_, state| state.recalling = recalling);
        output.push_event(GameplayEventKind::RigRecall { player_id, outcome });
        PlayerScopedCommandOutcome::Applied
    }

    /// Whether `player_id`'s rig is flying itself to its pilot.
    #[must_use]
    pub fn rig_recalling(&self, player_id: PlayerId) -> bool {
        self.pilot_of(player_id)
            .is_some_and(|(_, _, state)| state.recalling)
    }

    /// One tick of a recalled rig's autopilot (instead of its movement step). Returns whether
    /// it flew this tick.
    pub(super) fn step_rig_recall(&mut self, player_id: PlayerId, output: &mut TickOutput) -> bool {
        let Some((id, body, state)) = self.pilot_of(player_id).map(|(id, b, s)| (id, *b, *s))
        else {
            return false;
        };
        if !state.recalling {
            return false;
        }
        let (mx, my) = state.middle(&body);
        let stop = |world: &mut Self, outcome: RecallOutcome, output: &mut TickOutput| {
            world.edit_pilot(id, |_, state| state.recalling = false);
            if let Some(player) = world.players.get_mut(&player_id) {
                player.velocity_x = 0.0;
                player.velocity_y = 0.0;
            }
            output.push_event(GameplayEventKind::RigRecall { player_id, outcome });
        };
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        if player.hull <= 0.0 || player.fuel <= 0.0 {
            stop(self, RecallOutcome::NoPath, output);
            return false;
        }
        if (player.x - mx).hypot(player.y - my) <= RECALL_ARRIVE_DISTANCE {
            stop(self, RecallOutcome::Arrived, output);
            return true;
        }
        let Some((tx, ty)) = self.rig_recall_waypoint(player_id) else {
            stop(self, RecallOutcome::NoPath, output);
            return false;
        };
        let dt = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        let (dx, dy) = (tx - player.x, ty - player.y);
        let distance = dx.hypot(dy);
        let advance = (RECALL_SPEED * dt).min(distance);
        if distance > f32::EPSILON {
            player.x = quantize(dx / distance).mul_add(advance, player.x);
            player.y = quantize(dy / distance).mul_add(advance, player.y);
            player.x = quantize(player.x);
            player.y = quantize(player.y);
            player.velocity_x = quantize(dx / distance * RECALL_SPEED);
            player.velocity_y = quantize(dy / distance * RECALL_SPEED);
        }
        player.fuel = quantize(RECALL_FUEL_PER_SECOND.mul_add(-dt, player.fuel).max(0.0));
        true
    }

    // --- Sentry -----------------------------------------------------------------------------

    /// The weapon control a parked rig's sentry sends this tick (rig link tier
    /// [`crate::pilot::RIG_SENTRY_TIER`], a mounted gun, its pilot on foot): fire at the
    /// nearest creature within [`SENTRY_COVER`] of the pilot that the gun can see. `None`:
    /// hold fire.
    #[must_use]
    pub fn rig_sentry_control(&self, player_id: PlayerId) -> Option<WeaponControl> {
        let (_, body, state) = self.pilot_of(player_id)?;
        if state.gear.rig_link < crate::pilot::RIG_SENTRY_TIER || state.recalling {
            return None;
        }
        let player = self.players.get(&player_id)?;
        player.mounted_weapon?;
        if player.hull <= 0.0 {
            return None;
        }
        let (mx, my) = state.middle(body);
        let origin = (player.x, player.y - MUZZLE_RISE);
        let mut best: Option<(f32, f32, f32)> = None;
        for id in self
            .entities
            .ids_in_radius(origin.0, origin.1, AUTO_AIM_RANGE)
        {
            let Some(entity) = self.entities.get(id) else {
                continue;
            };
            if entity.state.health().is_none() {
                continue;
            }
            let (tx, ty) = (entity.body.x, entity.body.y);
            if (tx - mx).hypot(ty - my) > SENTRY_COVER
                || !line_clear(&self.terrain, origin.0, origin.1, tx, ty)
            {
                continue;
            }
            let distance = (tx - origin.0).hypot(ty - origin.1);
            if best.is_none_or(|(nearest, ..)| distance < nearest) {
                best = Some((distance, tx, ty));
            }
        }
        let (_, tx, ty) = best?;
        let (aim_x, aim_y) = quantize_aim(tx - origin.0, ty - origin.1)?;
        Some(WeaponControl {
            fire: true,
            auto_aim: false,
            aim_x,
            aim_y,
        })
    }

    // --- Exposure ---------------------------------------------------------------------------

    /// Whether world pixel `(x, y)` keeps a pilot out of the high sky's cold (wave 4
    /// integration): the Moon colony's and facility's heated rooms and finished lunar habitats
    /// (T22, T24), the streets of sky towns and sky cities (T20, T21: braziers and windbreaks,
    /// like the ground's settlements are safe) and the halls of sky temples (T21, enclosed
    /// stone). Out in the open sky the pilot needs a thermal lining.
    #[must_use]
    pub fn pilot_sheltered(&self, x: f32, y: f32) -> bool {
        let settings = self.terrain.settings();
        let tile = self.frame().tile_at(x, y);
        if crate::moon::sheltered_in(settings, self.frame(), x, y) || self.in_lunar_habitat(tile) {
            return true;
        }
        if tile.y >= 0 {
            return false;
        }
        if settings.generator.has_sky_isles()
            && crate::worldgen::streamed::in_village_area(settings, tile, 4)
        {
            return true;
        }
        settings.generator.has_sky_content()
            && (crate::worldgen::sky::continent::sky_city_at(settings, tile, 4).is_some()
                || crate::worldgen::sky::continent::temples_touching(
                    settings,
                    &crate::worldgen::TileRect {
                        min: tile,
                        max: tile,
                    },
                )
                .iter()
                .any(|(_, temple)| temple.rect().contains(tile)))
    }

    /// The cold and heat a pilot's suit fails to keep out this tick: health lost (none in god
    /// mode), and a [`GameplayEventKind::PilotExposed`] when it starts.
    pub(super) fn pilot_exposure(
        &self,
        body: &Body,
        state: &mut PilotState,
        dt: f32,
        owner: PlayerId,
        output: &mut TickOutput,
    ) -> f32 {
        let settings = self.terrain.settings();
        let (mx, my) = state.middle(body);
        let sheltered = settings.generator.has_sky() && self.pilot_sheltered(mx, my);
        let cold = if settings.generator.has_sky() && !sheltered {
            crate::environment::environment_in(settings, self.frame(), mx, my).cold
        } else {
            0
        };
        let cold_excess = cold.saturating_sub(state.gear.cold_cover());
        let tile = self.frame().tile_at(mx, my);
        // Run spaces (WP-TOMBS) keep their own climate: no band heat in the vault. The heat is
        // the rig's (`heat_units_at`): band heat bit for bit on older worlds, each stratum's own
        // on a Deep Descent world (Wave 8), in whole levels.
        let heat = if tile.y > 0 && !self.in_run_space(tile) {
            self.heat_units_at(tile) / crate::protection::HEAT_UNITS_PER_LEVEL
        } else {
            0
        };
        let heat_excess = heat.saturating_sub(state.gear.heat_cover());
        let exposure = if cold_excess > 0 {
            1
        } else if heat_excess > 0 {
            2
        } else {
            0
        };
        if exposure != 0 && exposure != state.exposure {
            output.push_event(GameplayEventKind::PilotExposed {
                player_id: owner,
                heat: exposure == 2,
            });
        }
        state.exposure = exposure;
        if self.damage_immune() {
            return 0.0;
        }
        (f32::from(cold_excess) * crate::pilot::COLD_DAMAGE_PER_LEVEL
            + f32::from(heat_excess) * crate::pilot::HEAT_DAMAGE_PER_LEVEL)
            * dt
    }

    /// The pilot actions that are not about hands (rope, recall); `None`: not one of them.
    pub(super) fn pilot_gear_action(
        &mut self,
        player_id: PlayerId,
        action: PilotAction,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        Some(match action {
            PilotAction::Rope { aim_x, aim_y } => {
                self.pilot_rope(player_id, (aim_x, aim_y), output)
            }
            PilotAction::RecallRig => self.recall_rig(player_id, output),
            PilotAction::Use | PilotAction::Equip(_) | PilotAction::CycleWeapon => return None,
        })
    }
}
