//! Top-down crypt floors in the world (WP-TOMBS T29; data in [`crate::tomb::crypt`]).
//!
//! * **Floors** ([`WorldState::prepare_crypt_floor`], from `write_tomb_floor`): the run's
//!   [`CryptState`] gets the floor's trap overlays (absolute tiles) and the tick the floor was
//!   written, which every trap cycle counts from.
//! * **Traps** ([`WorldState::advance_crypt`], from the tombs pass while the party explores):
//!   spikes and flame jets hurt standing members inside them while active; a boulder chute
//!   releases a boulder (a crypt shot) as its warning ends; an arrow plate a member steps on
//!   clicks and, [`PLATE_DELAY_MILLIS`] later, looses arrows from both walls along its line,
//!   re-arming [`PLATE_REARM_MILLIS`] after; a cracked floor stepped on gives way
//!   [`CRUMBLE_MILLIS`] later and stays a pit; a member walking into a pit falls (a share of
//!   their health, none with Featherfall) and climbs out at the nearest safe floor. A rolling
//!   member sets nothing off and falls into nothing. Damage scales with the site tier
//!   ([`crate::tomb::crypt::trap_tier_scale`]), the difficulty and relics (`damage_pilot`).
//!
//! Trap timing is in milliseconds since the floor started, from the tick count, so every tick
//! rate sees the same cycle.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "world pixels, tiles and milliseconds convert freely here"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    enemy::{EnemyKind, EnemyShotState, ShotKind},
    entity::{Body, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    pilot::{PILOT_HALF_WIDTH, PilotView, STRIKE_GUARD_SECONDS, TOP_DOWN_FOOTPRINT},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TilePosition},
    tomb::{
        RunPhase, SiteStyle,
        crypt::{CryptState, CryptTrap, CryptTrapKind, TrapStage, layout, trap_tier_scale},
        floor::plan_floor,
    },
    worldgen::tomb_sites,
};

use super::WorldState;

/// An arrow plate's click comes this long before its arrows.
pub const PLATE_DELAY_MILLIS: u64 = 320;
/// A plate that fired re-arms after this long.
pub const PLATE_REARM_MILLIS: u64 = 1_600;
/// A cracked floor gives way this long after it is stepped on.
pub const CRUMBLE_MILLIS: u64 = 450;
/// Share of max health a pit fall takes.
pub const PIT_FALL_SHARE: f32 = 0.18;
/// Most crypt boulders and arrows in flight at once.
const MAX_CRYPT_SHOTS: usize = 40;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

fn tile_centre(tile: TilePosition) -> (f32, f32) {
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}

/// A standing crypt member this tick.
#[derive(Clone, Copy, Debug)]
struct Walker {
    player: PlayerId,
    pilot: crate::entity::EntityId,
    /// The footprint's tiles: left, top, right, bottom (inclusive).
    tiles: (i32, i32, i32, i32),
    /// The tile under the footprint's centre.
    centre: TilePosition,
    rolling: bool,
    max_health: f32,
    featherfall: bool,
}

impl Walker {
    const fn covers(&self, tile: TilePosition) -> bool {
        tile.x >= self.tiles.0
            && tile.x <= self.tiles.2
            && tile.y >= self.tiles.1
            && tile.y <= self.tiles.3
    }
}

impl WorldState {
    /// Milliseconds `tick` is into the current crypt floor of `state`.
    fn crypt_millis(&self, state: &CryptState, tick: SimulationTick) -> u64 {
        let hz = u64::from(self.timing.rate.hz().max(1));
        tick.get().saturating_sub(state.started.get()) * 1_000 / hz
    }

    /// Milliseconds the current crypt floor of run `id` has been going (presentation reads it
    /// too, for trap animations).
    #[must_use]
    pub fn crypt_floor_millis(&self, id: u32) -> u64 {
        self.tombs
            .runs
            .get(&id)
            .map_or(0, |run| self.crypt_millis(&run.crypt, self.simulation_tick))
    }

    /// Puts the trap overlays of run `id`'s current floor into its [`CryptState`] (side-view
    /// runs get none).
    pub(super) fn prepare_crypt_floor(&mut self, id: u32) {
        let now = self.simulation_tick;
        let Some(run) = self.tombs.runs.get_mut(&id) else {
            return;
        };
        if run.site.style != SiteStyle::TopDown {
            run.crypt = CryptState::default();
            return;
        }
        let origin = tomb_sites::slot_origin(run.slot);
        let floor = layout(&plan_floor(&run.site, run.run, run.floor));
        run.crypt = CryptState {
            traps: floor
                .traps
                .iter()
                .map(|trap| CryptTrap {
                    at: TilePosition {
                        x: trap.at.x + origin.x,
                        y: trap.at.y + origin.y,
                    },
                    ..*trap
                })
                .collect(),
            started: now,
            crumbling: BTreeMap::new(),
            plates: BTreeMap::new(),
        };
    }

    /// The standing members of run `id` on foot, with their footprints.
    fn crypt_walkers(&self, id: u32) -> Vec<Walker> {
        let Some(run) = self.tombs.runs.get(&id) else {
            return Vec::new();
        };
        let mut walkers = Vec::new();
        for member in run.members.iter().filter(|member| !member.is_downed()) {
            let Some((pilot, body, state)) = self.pilot_of(member.player) else {
                continue;
            };
            if state.view != PilotView::TopDown {
                continue;
            }
            let left = tile_at(body.x - PILOT_HALF_WIDTH + 1.0, body.y).x;
            let right = tile_at(body.x + PILOT_HALF_WIDTH - 1.0, body.y).x;
            let top = tile_at(body.x, body.y - TOP_DOWN_FOOTPRINT + 1.0).y;
            let bottom = tile_at(body.x, body.y - 1.0).y;
            walkers.push(Walker {
                player: member.player,
                pilot,
                tiles: (left, top, right, bottom),
                centre: tile_at(body.x, TOP_DOWN_FOOTPRINT.mul_add(-0.5, body.y)),
                rolling: state.roll_guarded(),
                max_health: state.max_health,
                featherfall: state.boons.featherfall,
            });
        }
        walkers
    }

    /// One tick of run `id`'s crypt traps (see the module docs).
    #[allow(clippy::too_many_lines, reason = "one trap kind after another")]
    pub(super) fn advance_crypt(
        &mut self,
        id: u32,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        if run.site.style != SiteStyle::TopDown
            || run.phase != RunPhase::Exploring
            || run.crypt.traps.is_empty()
        {
            return;
        }
        let state = &run.crypt;
        let before = self.crypt_millis(
            state,
            SimulationTick::new(next_tick.get().saturating_sub(1)),
        );
        let after = self.crypt_millis(state, next_tick);
        let scale = trap_tier_scale(run.site.tier) * self.enemy_difficulty().damage();
        let dt = self.timing.delta_seconds;
        let walkers = self.crypt_walkers(id);
        let rooms = run.rooms.clone();
        let mut hurt: Vec<(crate::entity::EntityId, f32)> = Vec::new();
        let mut boulders: Vec<(TilePosition, (i8, i8))> = Vec::new();
        // One sound per room and trap kind at a time.
        let mut cues: BTreeMap<(u8, CryptTrapKind, bool), TilePosition> = BTreeMap::new();
        for trap in &state.traps {
            let Some(_) = trap.kind.cycle() else {
                continue;
            };
            let was = trap.stage(before);
            let now = trap.stage(after);
            if now != was && now != TrapStage::Idle {
                cues.entry((trap.room, trap.kind, now == TrapStage::Active))
                    .or_insert(trap.at);
            }
            match trap.kind {
                CryptTrapKind::BoulderChute => {
                    if trap.releases(before, after) {
                        boulders.push((trap.at, trap.dir));
                    }
                }
                _ if now == TrapStage::Active => {
                    for walker in &walkers {
                        if !walker.rolling && walker.covers(trap.at) {
                            let amount = trap.kind.damage_per_second() * dt * scale;
                            match hurt.iter_mut().find(|(pilot, _)| *pilot == walker.pilot) {
                                // The worst trap a member stands in counts, not every tile.
                                Some((_, worst)) => *worst = worst.max(amount),
                                None => hurt.push((walker.pilot, amount)),
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Plates and cracked floors under the members' feet.
        let mut pressed = Vec::new();
        let mut cracked = Vec::new();
        let mut falls = Vec::new();
        for walker in walkers.iter().filter(|walker| !walker.rolling) {
            match state.trap_at(walker.centre).map(|trap| trap.kind) {
                Some(CryptTrapKind::ArrowPlate) if !state.plates.contains_key(&walker.centre) => {
                    pressed.push(walker.centre);
                }
                Some(CryptTrapKind::CrackedFloor)
                    if !state.crumbling.contains_key(&walker.centre) =>
                {
                    cracked.push(walker.centre);
                }
                Some(CryptTrapKind::Pit) => falls.push(*walker),
                _ => {}
            }
        }
        let delay = self.timing.ticks_for_millis(PLATE_DELAY_MILLIS).max(1);
        let rearm = self.timing.ticks_for_millis(PLATE_REARM_MILLIS).max(1);
        let crumble = self.timing.ticks_for_millis(CRUMBLE_MILLIS).max(1);
        let mut arrows: Vec<((f32, f32), (f32, f32))> = Vec::new();
        let mut gave_way = Vec::new();
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            let state = &mut run.crypt;
            for tile in pressed {
                state.plates.insert(
                    tile,
                    (
                        SimulationTick::new(next_tick.get().saturating_add(delay)),
                        false,
                    ),
                );
                if let Some(trap) = state.trap_at(tile) {
                    cues.entry((trap.room, trap.kind, false)).or_insert(tile);
                }
            }
            for tile in cracked {
                state.crumbling.insert(
                    tile,
                    SimulationTick::new(next_tick.get().saturating_add(crumble)),
                );
                if let Some(trap) = state.trap_at(tile) {
                    cues.entry((trap.room, trap.kind, false)).or_insert(tile);
                }
            }
            // Plates whose click is up loose their arrows; spent ones re-arm.
            let mut spent = Vec::new();
            for (tile, (at, fired)) in &mut state.plates {
                if !*fired && next_tick >= *at {
                    *fired = true;
                    let Some(trap) = state
                        .traps
                        .binary_search_by(|trap| trap.at.cmp(tile))
                        .ok()
                        .map(|index| state.traps[index])
                    else {
                        continue;
                    };
                    if let Some(room) = rooms.get(usize::from(trap.room)) {
                        let interior = room.interior;
                        let (x, y) = tile_centre(*tile);
                        if trap.dir.0 != 0 {
                            let left = (interior.min.x as f32).mul_add(TILE_SIZE, 2.0);
                            let right = ((interior.max.x + 1) as f32).mul_add(TILE_SIZE, -2.0);
                            arrows.push(((left, y), (1.0, 0.0)));
                            arrows.push(((right, y), (-1.0, 0.0)));
                        } else {
                            let top = (interior.min.y as f32).mul_add(TILE_SIZE, 2.0);
                            let bottom = ((interior.max.y + 1) as f32).mul_add(TILE_SIZE, -2.0);
                            arrows.push(((x, top), (0.0, 1.0)));
                            arrows.push(((x, bottom), (0.0, -1.0)));
                        }
                        cues.insert((trap.room, trap.kind, true), *tile);
                    }
                } else if *fired && next_tick.get() >= at.get().saturating_add(rearm) {
                    spent.push(*tile);
                }
            }
            for tile in spent {
                state.plates.remove(&tile);
            }
            // Cracked floors whose time is up become pits.
            let due: Vec<TilePosition> = state
                .crumbling
                .iter()
                .filter(|(_, at)| next_tick >= **at)
                .map(|(tile, _)| *tile)
                .collect();
            for tile in due {
                state.crumbling.remove(&tile);
                if let Ok(index) = state.traps.binary_search_by(|trap| trap.at.cmp(&tile)) {
                    state.traps[index].kind = CryptTrapKind::Pit;
                    state.traps[index].hidden = false;
                    gave_way.push((state.traps[index].room, tile));
                }
            }
        }
        for (room, tile) in gave_way {
            cues.insert((room, CryptTrapKind::CrackedFloor, true), tile);
        }
        for ((_, kind, fired), tile) in cues {
            output.push_event(GameplayEventKind::CryptTrap {
                kind,
                x: tile.x,
                y: tile.y,
                fired,
            });
        }
        for (pilot, amount) in hurt {
            let _ = self.damage_pilot(pilot, amount, output);
        }
        for (tile, dir) in boulders {
            let (x, y) = tile_centre(tile);
            let unit = (f32::from(dir.0.signum()), f32::from(dir.1.signum()));
            self.spawn_crypt_shot(ShotKind::Boulder, (x, y), unit, scale);
        }
        for (from, unit) in arrows {
            self.spawn_crypt_shot(ShotKind::CryptArrow, from, unit, scale);
        }
        for walker in falls {
            self.fall_into_pit(id, walker, scale, output);
        }
    }

    /// Spawns a trap's arrow or boulder at `from` flying along `unit`.
    fn spawn_crypt_shot(&mut self, kind: ShotKind, from: (f32, f32), unit: (f32, f32), scale: f32) {
        let in_flight = self
            .entities
            .of_kind(EntityKind::EnemyShot)
            .filter(|entity| {
                matches!(entity.state, EntityState::EnemyShot(shot) if matches!(shot.kind, ShotKind::CryptArrow | ShotKind::Boulder))
            })
            .count();
        if in_flight >= MAX_CRYPT_SHOTS {
            return;
        }
        let lifetime = self.timing.ticks_for_millis(kind.life_millis()).max(1);
        let expires = SimulationTick::new(self.simulation_tick.get().saturating_add(lifetime));
        let speed = kind.speed();
        let _shot = self.spawn_entity(
            Body::at(quantize(from.0), quantize(from.1))
                .moving(quantize(unit.0 * speed), quantize(unit.1 * speed)),
            EntityState::EnemyShot(EnemyShotState {
                kind,
                damage: quantize(kind.damage() * scale),
                // Traps strike as the crypt's dead.
                shooter: EnemyKind::CryptSkeleton,
            }),
            Some(expires),
        );
    }

    /// A member walked into a pit: hurt (unless Featherfall) and put back on the nearest safe
    /// floor of the run, briefly guarded.
    fn fall_into_pit(&mut self, id: u32, walker: Walker, scale: f32, output: &mut TickOutput) {
        let Some(safe) = self.crypt_safe_floor(id, walker.centre) else {
            return;
        };
        let (x, _) = tile_centre(safe);
        let feet = ((safe.y + 1) as f32).mul_add(TILE_SIZE, -3.5);
        let (x, feet) = (quantize(x), quantize(feet));
        self.entities.update(walker.pilot, |entity| {
            entity.body.x = x;
            entity.body.y = feet;
            entity.body.previous_x = x;
            entity.body.previous_y = feet;
            entity.body.velocity_x = 0.0;
            entity.body.velocity_y = 0.0;
        });
        output.push_event(GameplayEventKind::CryptPitFall {
            player_id: walker.player,
            x: walker.centre.x,
            y: walker.centre.y,
        });
        if !walker.featherfall {
            let _ = self.damage_pilot(
                walker.pilot,
                walker.max_health * PIT_FALL_SHARE * scale,
                output,
            );
        }
        self.edit_pilot(walker.pilot, |_, state| {
            state.hurt = state.hurt.max(STRIKE_GUARD_SECONDS);
        });
    }

    /// The nearest open floor tile to `from` with no trap overlay that could hurt or drop a
    /// pilot (breadth-first over open tiles of the run's rooms).
    fn crypt_safe_floor(&self, id: u32, from: TilePosition) -> Option<TilePosition> {
        let run = self.tombs.runs.get(&id)?;
        let inside = |tile: TilePosition| {
            run.rooms
                .iter()
                .any(|room| room.interior.expanded(1).contains(tile))
        };
        let unsafe_trap = |tile: TilePosition| {
            run.crypt.trap_at(tile).is_some_and(|trap| {
                !matches!(
                    trap.kind,
                    CryptTrapKind::ArrowPlate | CryptTrapKind::BoulderChute
                )
            })
        };
        let open = |tile: TilePosition| !crate::world::pilot::pilot_blocks(&self.terrain, tile);
        let mut seen = BTreeSet::from([from]);
        let mut queue = VecDeque::from([from]);
        while let Some(at) = queue.pop_front() {
            if at != from && open(at) && !unsafe_trap(at) {
                return Some(at);
            }
            if seen.len() > 600 {
                break;
            }
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let next = TilePosition {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                if inside(next) && open(next) && seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        None
    }

    // --- Scenarios --------------------------------------------------------------------------

    /// Puts `player`'s run straight onto floor `floor`, written over the current one with the
    /// party at its start (tests, screenshot scenes and the balance harness, both styles: the
    /// guardian's floor or the lord's hall without playing the floors above).
    pub fn scenario_tomb_floor(&mut self, player: PlayerId, floor: u8) {
        let Some(id) = self.tombs.run_id_of(player) else {
            return;
        };
        let previous = self.tombs.runs.get(&id).map(Self::tomb_floor_of);
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.floor = floor.min(run.site.floors().saturating_sub(1));
            run.phase = RunPhase::Exploring;
            run.picking_since = None;
        }
        self.write_tomb_floor(id, previous);
    }

    /// Replaces the trap overlays of `player`'s crypt floor with `traps` (absolute tiles; tests
    /// and scenes), restarting the trap clock at `started_millis_ago` before now.
    pub fn scenario_crypt_traps(
        &mut self,
        player: PlayerId,
        mut traps: Vec<CryptTrap>,
        started_millis_ago: u64,
    ) {
        let ago = self.timing.ticks_for_millis(started_millis_ago);
        let started = SimulationTick::new(self.simulation_tick.get().saturating_sub(ago));
        let Some(id) = self.tombs.run_id_of(player) else {
            return;
        };
        traps.sort();
        traps.dedup_by(|a, b| a.at == b.at);
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.crypt.traps = traps;
            run.crypt.started = started;
            run.crypt.crumbling.clear();
            run.crypt.plates.clear();
        }
    }
}
