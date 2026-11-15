//! World-event hazards, deep-run pressure, and falling boulders.

use crate::{
    event::{GameplayEventKind, TickOutput},
    hazard::{FallingBoulder, HazardCloud},
    ids::{PlayerId, SimulationTick},
    progression::WorldEventKind,
    quantize::quantize_simulation_scalar,
    terrain::{DeepStratum, MineralKind, TILE_SIZE, TilePosition},
    worldgen::Band,
};

use super::{
    WorldState,
    movement::{
        AUTHORITATIVE_BOULDER_LIFE_MILLIS, AUTHORITATIVE_BOULDER_WARNING_MILLIS,
        AUTHORITATIVE_CAVE_IN_LIFE_MILLIS, AUTHORITATIVE_CAVE_IN_WARNING_MILLIS,
        AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS,
    },
};

/// Period of the collapse-surge cave-in roll near an underground rig. Was 3 s, which rained a
/// cave-in on the rig every 3 s for the whole multi-day event; each one also has to find
/// unsettled ground now ([`SETTLED_GROUND_MILLIS`]).
pub const COLLAPSE_SURGE_PERIOD_MILLIS: u64 = 45_000;
/// Period of gas-bloom clouds near a deep rig.
const GAS_BLOOM_PERIOD_MILLIS: u64 = 4_000;
/// Period of gas clouds released by rare cargo on a deep run.
const RARE_CARGO_GAS_PERIOD_MILLIS: u64 = 4_000;
/// Scanner lockout refreshed every tick while below the pressure depth.
const DEEP_PRESSURE_SCANNER_LOCKOUT_MILLIS: u64 = 1_200;
/// Instability per second added for each band below the slate shelves (a full meter
/// triggers a cave-in).
const DEEP_PRESSURE_PER_BAND: f32 = 0.05;
/// Hull damage of a falling boulder hitting a rig (before the boulder deflector).
pub const BOULDER_HIT_DAMAGE: f32 = 8.0;
/// Radius of a falling boulder (px, the sprite's half height): it hangs this far under the top
/// of its tile and shatters when its lower edge meets solid ground.
pub const BOULDER_RADIUS: f32 = 11.0;
/// How long ground that came down holds: no random collapse (mining boulder, collapse surge,
/// deep-run cave-in, earthquake, stalactite) starts in or next to its cell for this long.
pub const SETTLED_GROUND_MILLIS: u64 = 180_000;
/// Rows above the rig a cave-in boulder may hang (under the ceiling when it is that close; the
/// old spawn was always two rows up, ceiling or not).
const CAVE_IN_RISE_ROWS: i32 = 3;
/// Value of a full deep-run instability meter.
const DEEP_INSTABILITY_FULL: f32 = 100.0;

impl WorldState {
    pub(super) fn advance_world_event_hazards(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        // World-event hazards strike underground only: within three rows of the local surface
        // (a mountain slope, a lake bed) a rig is still out in the open.
        let tile = player.tile_position(TILE_SIZE);
        if tile.y < self.terrain.surface_row(tile.x) + 3 {
            return;
        }
        let (player_x, player_y) = (player.x, player.y);
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::CollapseSurge)
            && self
                .timing
                .rate
                .schedule_fires(next_tick, COLLAPSE_SURGE_PERIOD_MILLIS, 0)
            && self.try_cave_in(player_x, player_y)
        {
            output.push_event(GameplayEventKind::BouldersChanged);
        }
        if self
            .world_event_severity
            .contains_key(&WorldEventKind::GasBloom)
            && self
                .timing
                .rate
                .schedule_fires(next_tick, GAS_BLOOM_PERIOD_MILLIS, 0)
        {
            self.spawn_gas_cloud(HazardCloud {
                x: player_x,
                y: player_y + TILE_SIZE,
                life: 6.0,
                radius: 12.0,
            });
            output.push_event(GameplayEventKind::HazardChanged);
        }
    }

    pub(super) fn advance_deep_run_pressure(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let band = self.terrain.band_at(player.tile_position(TILE_SIZE));
        let rare_cargo = player
            .cargo_bay
            .minerals()
            .filter(|(mineral, _)| {
                matches!(
                    mineral,
                    MineralKind::Diamond
                        | MineralKind::Platinum
                        | MineralKind::Uranium
                        | MineralKind::Mythril
                )
            })
            .map(|(_, count)| count)
            .sum::<u32>()
            .saturating_add(player.cargo_bay.artifact_total());
        let timing = self.timing;
        let delta_seconds = timing.delta_seconds;
        let state = self.deep_run_states.entry(player_id).or_default();
        // Instability builds from the basalt deeps down, wherever this world's strata put them.
        let bands_deep = band.index().saturating_sub(Band::Slate.index());
        if bands_deep == 0 {
            state.trip_ticks = 0;
            state.instability = 2.0f32.mul_add(-delta_seconds, state.instability).max(0.0);
            return;
        }
        state.trip_ticks = state.trip_ticks.saturating_add(1);
        let trip_seconds = timing.rate.seconds_for_ticks(state.trip_ticks);
        #[allow(
            clippy::cast_precision_loss,
            reason = "bounded band and cargo counts intentionally feed gameplay pressure"
        )]
        let pressure = (bands_deep as f32).mul_add(
            DEEP_PRESSURE_PER_BAND,
            trip_seconds.mul_add(0.0003, rare_cargo as f32 * 0.01),
        );
        state.instability = delta_seconds.mul_add(pressure, state.instability);
        if band.index() >= Band::Deep(DeepStratum::FossilOceans).index() {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.fuel =
                    quantize_simulation_scalar(delta_seconds.mul_add(-0.25, player.fuel).max(0.0));
            }
            let deadline = SimulationTick::new(
                next_tick
                    .get()
                    .saturating_add(timing.ticks_for_millis(DEEP_PRESSURE_SCANNER_LOCKOUT_MILLIS)),
            );
            self.scanner_cooldown_deadlines
                .entry(player_id)
                .and_modify(|current| *current = (*current).max(deadline))
                .or_insert(deadline);
        }
        if rare_cargo > 0
            && timing
                .rate
                .schedule_fires(next_tick, RARE_CARGO_GAS_PERIOD_MILLIS, 0)
        {
            let player = self.players.get(&player_id).expect("player remains valid");
            let cloud = HazardCloud {
                x: player.x,
                y: player.y + TILE_SIZE,
                life: 5.0,
                radius: 10.0 + f32::from(u16::try_from(rare_cargo).unwrap_or(u16::MAX)).min(4.0),
            };
            super::entities::spawn_gas_cloud_into(&mut self.entities, self.simulation_tick, cloud);
            output.push_event(GameplayEventKind::HazardChanged);
        }
        if state.instability >= DEEP_INSTABILITY_FULL {
            // A full meter empties (it used to drop to 45 and refill within a couple of
            // minutes); the cave-in itself still needs unsettled ground.
            state.instability = 0.0;
            let player = self.players.get(&player_id).expect("player remains valid");
            let (player_x, player_y) = (player.x, player.y);
            let damage = player.hazard_gear.boulder_damage(4.0);
            if self.try_cave_in(player_x, player_y) {
                if self.damage_player(player_id, damage) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                output.push_event(GameplayEventKind::BouldersChanged);
            }
        }
    }

    /// Whether random ground may come down at `tile` now: at least three rows under the local
    /// surface (out in the open nothing hangs overhead) and no recent collapse in or next to its
    /// cell. When it may, its cell settles for [`SETTLED_GROUND_MILLIS`].
    pub(super) fn loosen_ground(&mut self, tile: TilePosition) -> bool {
        if tile.y < self.terrain.surface_row(tile.x) + 3
            || self
                .progression
                .settled_ground
                .holds(tile, self.simulation_tick)
        {
            return false;
        }
        self.settle_ground(tile);
        true
    }

    /// Settles `tile`'s cell for [`SETTLED_GROUND_MILLIS`].
    pub(super) fn settle_ground(&mut self, tile: TilePosition) {
        let until = SimulationTick::new(
            self.simulation_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(SETTLED_GROUND_MILLIS)),
        );
        self.progression.settled_ground.settle(tile, until);
    }

    /// Where a boulder loosened at `tile` hangs: the highest open tile of the open run from
    /// `tile` up to `max_rise` rows higher (right under the ceiling, if one is that close).
    /// `None` when `tile` itself is solid (a boulder never starts inside rock) or, with
    /// `needs_drop`, when the tile under the hanging spot is solid (no room to fall).
    pub(super) fn boulder_hang_tile(
        &self,
        tile: TilePosition,
        max_rise: i32,
        needs_drop: bool,
    ) -> Option<TilePosition> {
        if self.terrain.is_solid_at(tile) {
            return None;
        }
        let mut hang = tile;
        for _ in 0..max_rise {
            let above = TilePosition {
                x: hang.x,
                y: hang.y - 1,
            };
            if self.terrain.is_solid_at(above) {
                break;
            }
            hang = above;
        }
        let below = TilePosition {
            x: hang.x,
            y: hang.y + 1,
        };
        (!needs_drop || !self.terrain.is_solid_at(below)).then_some(hang)
    }

    /// A random cave-in over `(x, y)` (collapse surges, deep-run pressure, earthquakes): only
    /// underground on unsettled ground. Returns whether any boulder came loose.
    pub(super) fn try_cave_in(&mut self, x: f32, y: f32) -> bool {
        if !self.loosen_ground(tile_at(x, y)) {
            return false;
        }
        self.spawn_cave_in(x, y)
    }

    /// Spawns up to three boulders over `(x, y)`, the centre one first and the outer ones a
    /// little later: each hangs in open ground in its column, up to three rows above `y` (under
    /// the ceiling if that is closer), with an open tile under it to fall through; a column
    /// whose tile at `y` is solid, or with no such room (a one-tile-high tunnel), gets none.
    /// Settles the ground there. Returns whether any boulder came loose.
    pub(super) fn spawn_cave_in(&mut self, x: f32, y: f32) -> bool {
        self.settle_ground(tile_at(x, y));
        let mut spawned = false;
        for offset in -1_i16..=1 {
            let column_x = f32::from(offset).mul_add(TILE_SIZE, x);
            let Some(hang) = self.boulder_hang_tile(tile_at(column_x, y), CAVE_IN_RISE_ROWS, true)
            else {
                continue;
            };
            self.spawn_falling_boulder(
                column_x,
                hanging_y(hang),
                AUTHORITATIVE_CAVE_IN_WARNING_MILLIS
                    + u64::from(offset.unsigned_abs()) * AUTHORITATIVE_CAVE_IN_WARNING_STEP_MILLIS,
                AUTHORITATIVE_CAVE_IN_LIFE_MILLIS,
            );
            spawned = true;
        }
        spawned
    }

    /// The boulder mining `target` (just broken stone or hard rock) may loosen: one in
    /// `chance` tiles by a fixed per-tile roll; it hangs in the open tile over the cut (the
    /// cut itself under a ceiling, else the tile above it) when there is room under it to fall,
    /// underground, on unsettled ground. Returns whether one came loose.
    pub(super) fn loosen_mining_boulder(&mut self, target: TilePosition, chance: u64) -> bool {
        if !mining_boulder_roll(self.terrain.seed(), target, chance) {
            return false;
        }
        let Some(hang) = self.boulder_hang_tile(target, 1, true) else {
            return false;
        };
        if !self.loosen_ground(target) {
            return false;
        }
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let x = (hang.x as f32 + 0.5) * TILE_SIZE;
        self.spawn_falling_boulder(
            x,
            hanging_y(hang),
            AUTHORITATIVE_BOULDER_WARNING_MILLIS,
            AUTHORITATIVE_BOULDER_LIFE_MILLIS,
        );
        true
    }

    pub(super) fn spawn_falling_boulder(
        &mut self,
        x: f32,
        y: f32,
        warning_millis: u64,
        life_millis: u64,
    ) {
        let rate = self.timing.rate;
        let warning_ticks = rate.ticks_for_millis(warning_millis);
        let life_ticks = rate.ticks_for_millis(life_millis);
        self.spawn_boulder(
            FallingBoulder {
                x,
                y,
                velocity_y: 0.0,
                warning_seconds: rate.seconds_for_ticks(warning_ticks),
                life: rate.seconds_for_ticks(life_ticks),
            },
            SimulationTick::new(self.simulation_tick.get().saturating_add(warning_ticks)),
            SimulationTick::new(self.simulation_tick.get().saturating_add(life_ticks)),
        );
    }
}

/// The fixed per-tile roll for a mining boulder: whether breaking `target` on `seed`'s world
/// loosens one at odds of one in `chance` (the legacy hash, so each tile always rolls the same).
#[must_use]
pub(super) fn mining_boulder_roll(seed: u64, target: TilePosition, chance: u64) -> bool {
    #[allow(
        clippy::cast_sign_loss,
        reason = "terrain coordinates deterministically reproduce the legacy hazard schedule"
    )]
    let value =
        seed ^ ((target.x as u64).wrapping_mul(0x9E37)) ^ ((target.y as u64).wrapping_mul(0x85EB));
    value.is_multiple_of(chance.max(1))
}

/// The tile holding the point `(x, y)`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world coordinates intentionally map to terrain tiles"
)]
fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

/// The centre height of a boulder hanging under the top of `tile`.
#[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
const fn hanging_y(tile: TilePosition) -> f32 {
    (tile.y as f32).mul_add(TILE_SIZE, BOULDER_RADIUS)
}
