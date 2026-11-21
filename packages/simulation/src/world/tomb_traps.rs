//! Side-view tomb runs in the world (WP-TOMBS, T28; trap shapes in [`crate::tomb::traps`]).
//!
//! One pass per side-view run per tick ([`WorldState::advance_side_view_run`], called from the
//! tombs pass after the spike check):
//!
//! * **Moving traps**: every blade, crusher, dart thrower and cursed vent near a standing
//!   member is evaluated at the world clock; a pilot touching a hazard shape takes its damage
//!   rate (scaled by the member's trap relics and the enemy difficulty); a crusher's block
//!   shoves a pilot out from under it. A pilot in a sluice room whose feet are under the sand
//!   takes the sand's rate.
//! * **Crumbling floors**: a slab a grounded member stands on starts to crumble
//!   ([`crate::tomb::TombRun::crumbling`]), falls [`CRUMBLE_MILLIS`] later and sets again
//!   [`REFORM_MILLIS`] after that once nothing stands in its place.
//! * **Cues**: a trap that goes off near the party reports [`GameplayEventKind::TombTrapSprung`]
//!   (the slam, the dart, the burst, the pour, the fall).
//! * **The guardian's phases**: as the guardian's health falls past
//!   [`crate::tomb::side_view::guardian_phase`]'s thresholds the run records the phase and
//!   reports [`GameplayEventKind::TombGuardianPhase`].

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "world pixels and small tile offsets convert freely here"
)]

use std::collections::BTreeSet;

use crate::{
    base::BuiltTile,
    entity::{EntityId, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    pilot::PILOT_HALF_WIDTH,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        RunPhase, SiteStyle,
        relic::RelicEffects,
        side_view::{dart_facing, guardian_phase},
        traps::{
            BLADE_DPS, BLADE_RADIUS, Bounds, CRUMBLE_MILLIS, CRUSHER_DPS, CRUSHER_MAX_REACH,
            CrumblingSlab, CrusherStage, DART_DPS, DART_MAX_REACH, FOG_DPS, FogStage,
            REFORM_MILLIS, SAND_DPS, SAND_WADE_PIXELS, SandStage, TrapKind, blade_tip,
            crusher_block, crusher_state, dart_bounds, dart_position, dart_state, fog_centre,
            fog_hurts, fog_state, sand_state, sand_surface, world_millis,
        },
    },
    worldgen::features::TileRect,
};

use super::{WorldState, pilot::pilot_blocks};

/// Columns either side of a pilot searched for traps (a dart's reach).
const SCAN_COLUMNS: i32 = DART_MAX_REACH;
/// Rows above a pilot's head searched (a blade's or crusher's ceiling).
const SCAN_ROWS_ABOVE: i32 = 7;
/// Rows below a pilot's feet searched (vents, sluices).
const SCAN_ROWS_BELOW: i32 = 3;

/// A standing member's pilot this tick.
struct Walker {
    player: PlayerId,
    pilot: EntityId,
    bounds: Bounds,
    feet: (f32, f32),
    grounded: bool,
}

fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

impl WorldState {
    /// Side-view run `id`'s tick (see the module docs).
    pub(super) fn advance_side_view_run(
        &mut self,
        id: u32,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        if run.site.style != SiteStyle::SideView || run.phase != RunPhase::Exploring {
            return;
        }
        let hz = self.timing.rate.hz();
        let now = world_millis(next_tick.get(), hz);
        let before = world_millis(next_tick.get().saturating_sub(1), hz);
        let walkers = self.side_walkers(id);
        self.advance_crumbling(id, &walkers, now, output);
        self.hurt_by_side_traps(id, &walkers, now, before, output);
        self.track_guardian_phase(id, output);
    }

    fn side_walkers(&self, id: u32) -> Vec<Walker> {
        let Some(run) = self.tombs.runs.get(&id) else {
            return Vec::new();
        };
        run.members
            .iter()
            .filter(|member| !member.is_downed())
            .filter_map(|member| {
                let (pilot, body, state) = self.pilot_of(member.player)?;
                Some(Walker {
                    player: member.player,
                    pilot,
                    bounds: Bounds::new(
                        body.x - PILOT_HALF_WIDTH,
                        body.y - state.height(),
                        body.x + PILOT_HALF_WIDTH,
                        body.y,
                    ),
                    feet: (body.x, body.y),
                    grounded: state.grounded,
                })
            })
            .collect()
    }

    fn trap_tile(&self, tile: TilePosition) -> Option<(TrapKind, bool)> {
        match self.terrain.tile(tile).map(|found| found.kind) {
            Some(TileKind::Built(BuiltTile::TombTrap { kind, hidden })) => {
                Some((TrapKind::from_index(kind), hidden))
            }
            _ => None,
        }
    }

    /// Open rows under a crusher's housing (its block's reach).
    fn crusher_reach(&self, tile: TilePosition) -> i32 {
        (1..=CRUSHER_MAX_REACH)
            .take_while(|dy| {
                !pilot_blocks(
                    &self.terrain,
                    TilePosition {
                        x: tile.x,
                        y: tile.y + dy,
                    },
                )
            })
            .count() as i32
    }

    /// Open tiles in front of a dart thrower (its darts' reach).
    fn dart_reach(&self, tile: TilePosition, facing: i8) -> i32 {
        (1..=DART_MAX_REACH)
            .take_while(|dx| {
                !pilot_blocks(
                    &self.terrain,
                    TilePosition {
                        x: tile.x + dx * i32::from(facing),
                        y: tile.y,
                    },
                )
            })
            .count() as i32
    }

    /// Whether the room `room` (absolute interior) has sluice grates under its floor.
    fn sluice_room(&self, room: TileRect) -> bool {
        (room.min.x..=room.max.x).any(|x| {
            matches!(
                self.trap_tile(TilePosition {
                    x,
                    y: room.max.y + 1,
                }),
                Some((TrapKind::SandSluice, _))
            )
        })
    }

    #[allow(clippy::too_many_lines, reason = "one arm per trap kind")]
    fn hurt_by_side_traps(
        &mut self,
        id: u32,
        walkers: &[Walker],
        now: u64,
        before: u64,
        output: &mut TickOutput,
    ) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let rooms: Vec<TileRect> = run.rooms.iter().map(|room| room.interior).collect();
        let dt = self.timing.delta_seconds;
        let difficulty = self.enemy_difficulty().damage();
        let mut sprung: BTreeSet<TilePosition> = BTreeSet::new();
        let mut events: Vec<(TrapKind, (f32, f32))> = Vec::new();
        let mut hurt: Vec<(EntityId, f32)> = Vec::new();
        let mut shoves: Vec<(EntityId, f32)> = Vec::new();
        for walker in walkers {
            let (mut blade, mut crusher, mut dart, mut fog, mut sand) = (0.0, 0.0, 0.0, 0.0, 0.0);
            let head = tile_at(walker.feet.0, walker.bounds.top);
            let foot = tile_at(walker.feet.0, walker.feet.1 - 1.0);
            for y in head.y - SCAN_ROWS_ABOVE..=foot.y + SCAN_ROWS_BELOW {
                for x in foot.x - SCAN_COLUMNS..=foot.x + SCAN_COLUMNS {
                    let tile = TilePosition { x, y };
                    let Some((kind, hidden)) = self.trap_tile(tile) else {
                        continue;
                    };
                    match kind {
                        TrapKind::Blade => {
                            if walker
                                .bounds
                                .touches_disc(blade_tip(tile, now), BLADE_RADIUS)
                            {
                                blade = BLADE_DPS;
                            }
                        }
                        TrapKind::Crusher => {
                            let reach = self.crusher_reach(tile);
                            let (stage, extension) = crusher_state(tile, now);
                            if let Some(block) = crusher_block(tile, reach, extension)
                                && block.overlaps(&walker.bounds)
                            {
                                crusher = CRUSHER_DPS;
                                let middle = (block.left + block.right) * 0.5;
                                let to = if walker.feet.0 < middle {
                                    block.left - PILOT_HALF_WIDTH - 0.5
                                } else {
                                    block.right + PILOT_HALF_WIDTH + 0.5
                                };
                                shoves.push((walker.pilot, to));
                            }
                            let (was, _) = crusher_state(tile, before);
                            if stage == CrusherStage::Slamming
                                && was == CrusherStage::Shaking
                                && sprung.insert(tile)
                            {
                                events.push((
                                    kind,
                                    (
                                        (tile.x as f32 + 0.5) * TILE_SIZE,
                                        (tile.y + 1 + reach) as f32 * TILE_SIZE,
                                    ),
                                ));
                            }
                        }
                        TrapKind::DartWall => {
                            let facing = dart_facing(hidden);
                            let reach = self.dart_reach(tile, facing);
                            if let Some(at) = dart_position(tile, facing, reach, now)
                                && dart_bounds(at).overlaps(&walker.bounds)
                            {
                                dart = DART_DPS;
                            }
                            if dart_state(tile, before).1.is_none()
                                && dart_state(tile, now).1.is_some()
                                && sprung.insert(tile)
                            {
                                events.push((
                                    kind,
                                    (
                                        (tile.x as f32 + 0.5) * TILE_SIZE,
                                        (tile.y as f32 + 0.5) * TILE_SIZE,
                                    ),
                                ));
                            }
                        }
                        TrapKind::FogVent => {
                            if let Some(radius) = fog_hurts(tile, now)
                                && walker.bounds.touches_disc(fog_centre(tile), radius)
                            {
                                fog = FOG_DPS;
                            }
                            if fog_state(tile, now).0 == FogStage::Burst
                                && fog_state(tile, before).0 != FogStage::Burst
                                && sprung.insert(tile)
                            {
                                events.push((kind, fog_centre(tile)));
                            }
                        }
                        TrapKind::Spikes
                        | TrapKind::Crumble
                        | TrapKind::SandSluice
                        | TrapKind::FalseWall => {}
                    }
                }
            }
            // Rising sand: the room the pilot's feet are in.
            if let Some(room) = rooms.iter().find(|room| room.contains(foot))
                && self.sluice_room(*room)
            {
                let (stage, level) = sand_state(*room, now);
                if level > 0.0 && walker.feet.1 > sand_surface(*room, level) + SAND_WADE_PIXELS {
                    sand = SAND_DPS;
                }
                if stage == SandStage::Rising
                    && sand_state(*room, before).0 != SandStage::Rising
                    && sprung.insert(room.min)
                {
                    events.push((
                        TrapKind::SandSluice,
                        (
                            (room.min.x + room.max.x + 1) as f32 * 0.5 * TILE_SIZE,
                            (room.max.y + 1) as f32 * TILE_SIZE,
                        ),
                    ));
                }
            }
            let dps: f32 = blade + crusher + dart + fog + sand;
            if dps > 0.0 {
                let scale = self.tombs.effects_of(walker.player).trap_pct;
                hurt.push((
                    walker.pilot,
                    dps * dt * difficulty * RelicEffects::scale(scale),
                ));
            }
        }
        for (pilot, to) in shoves {
            let clear = self.pilot_of_entity_fits(pilot, to);
            if clear {
                self.edit_pilot(pilot, |body, _| {
                    body.x = crate::quantize::quantize_simulation_scalar(to);
                    body.previous_x = body.x;
                    body.velocity_x = 0.0;
                });
            }
        }
        for (pilot, amount) in hurt {
            let _ = self.damage_pilot(pilot, amount, output);
        }
        for (kind, (x, y)) in events {
            output.push_event(GameplayEventKind::TombTrapSprung {
                kind,
                x: x as i32,
                y: y as i32,
            });
        }
    }

    /// Whether pilot `pilot` would fit with its middle at column `x` (its own rows).
    fn pilot_of_entity_fits(&self, pilot: EntityId, x: f32) -> bool {
        let Some((body, height)) = self
            .entities
            .get(pilot)
            .and_then(|entity| match entity.state {
                EntityState::Pilot(state) => Some((entity.body, state.height())),
                _ => None,
            })
        else {
            return false;
        };
        let rows = tile_at(x, body.y - height + 1.0).y..=tile_at(x, body.y - 1.0).y;
        let columns = [
            tile_at(x - PILOT_HALF_WIDTH + 1.0, body.y).x,
            tile_at(x + PILOT_HALF_WIDTH - 1.0, body.y).x,
        ];
        rows.into_iter().all(|y| {
            columns
                .iter()
                .all(|column| !pilot_blocks(&self.terrain, TilePosition { x: *column, y }))
        })
    }

    fn advance_crumbling(
        &mut self,
        id: u32,
        walkers: &[Walker],
        now: u64,
        output: &mut TickOutput,
    ) {
        // Slabs under grounded feet start to crumble.
        let mut stepped: Vec<(TilePosition, bool)> = Vec::new();
        for walker in walkers.iter().filter(|walker| walker.grounded) {
            let row = tile_at(walker.feet.0, walker.feet.1 + 1.0).y;
            for x in [
                tile_at(walker.bounds.left + 1.0, 0.0).x,
                tile_at(walker.bounds.right - 1.0, 0.0).x,
            ] {
                let tile = TilePosition { x, y: row };
                if let Some((TrapKind::Crumble, hidden)) = self.trap_tile(tile)
                    && !stepped.iter().any(|(at, _)| *at == tile)
                {
                    stepped.push((tile, hidden));
                }
            }
        }
        let Some(run) = self.tombs.runs.get_mut(&id) else {
            return;
        };
        for (tile, hidden) in stepped {
            if !run.crumbling.iter().any(|slab| slab.tile == tile) {
                run.crumbling.push(CrumblingSlab {
                    tile,
                    stepped: now,
                    fell: None,
                    hidden,
                });
            }
        }
        if run.crumbling.is_empty() {
            return;
        }
        let mut falls = Vec::new();
        let mut reforms = Vec::new();
        for slab in &mut run.crumbling {
            match slab.fell {
                None if now >= slab.stepped + CRUMBLE_MILLIS => {
                    slab.fell = Some(now);
                    falls.push(slab.tile);
                }
                Some(fell) if now >= fell + REFORM_MILLIS => reforms.push(*slab),
                _ => {}
            }
        }
        for tile in falls {
            let _ = self.terrain.set_kind_by_rule(tile, TileKind::Air);
            output.push_event(GameplayEventKind::TombTrapSprung {
                kind: TrapKind::Crumble,
                x: ((tile.x as f32 + 0.5) * TILE_SIZE) as i32,
                y: ((tile.y as f32 + 0.5) * TILE_SIZE) as i32,
            });
        }
        for slab in reforms {
            // Never set a slab inside someone standing in the hole.
            let left = slab.tile.x as f32 * TILE_SIZE;
            let top = slab.tile.y as f32 * TILE_SIZE;
            let cell = Bounds::new(left, top, left + TILE_SIZE, top + TILE_SIZE);
            let occupied = self.pilots().any(|(_, body, state)| {
                cell.overlaps(&Bounds::new(
                    body.x - PILOT_HALF_WIDTH,
                    body.y - state.height(),
                    body.x + PILOT_HALF_WIDTH,
                    body.y,
                ))
            }) || !self
                .enemies_near(left + TILE_SIZE * 0.5, top + TILE_SIZE * 0.5, TILE_SIZE)
                .is_empty();
            if occupied {
                continue;
            }
            let _ = self.terrain.set_kind_by_rule(
                slab.tile,
                TileKind::Built(BuiltTile::TombTrap {
                    kind: TrapKind::Crumble.index(),
                    hidden: slab.hidden,
                }),
            );
            if let Some(run) = self.tombs.runs.get_mut(&id) {
                run.crumbling.retain(|kept| kept.tile != slab.tile);
            }
        }
    }

    /// Records the guardian's phase as its health falls, and reports each new one.
    fn track_guardian_phase(&mut self, id: u32, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let Some(boss) = run.boss else {
            return;
        };
        if run.boss_defeated {
            return;
        }
        let Some((body, state)) = self
            .entities
            .get(EntityId(boss))
            .and_then(|entity| match entity.state {
                EntityState::Enemy(state) => Some((entity.body, state)),
                _ => None,
            })
        else {
            return;
        };
        let phase = guardian_phase(state.health_fraction());
        if phase <= run.boss_phase.max(1) {
            return;
        }
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.boss_phase = phase;
        }
        output.push_event(GameplayEventKind::TombGuardianPhase {
            kind: state.kind,
            phase,
            x: body.x as i32,
            y: body.y as i32,
        });
    }
}
