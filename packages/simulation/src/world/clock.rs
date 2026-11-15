//! The world side of the clock (Part II F4, types in [`crate::clock`]): reading the time, day
//! events, beds and skipping the night, and the gameplay light level at a tile.

use crate::{
    base::{Fixture, ModuleKind},
    clock::{
        BedSpot, ClockState, DayLength, DayPhase, MoonAnchor, SleepCommand, SleepRule, Sleeper,
        WorldClock, default_moon_anchor, start_offset_millis,
    },
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::WorldState;

/// Tiles of rock under the local surface sky light still reaches (fading out linearly).
pub const SKY_LIGHT_DEPTH_TILES: i32 = 6;
/// Reach in tiles of a rig's lamps (`light_at`).
pub const RIG_LIGHT_TILES: i32 = 7;
/// Light level (permille) right at a rig's lamps.
pub const RIG_LIGHT: u16 = 900;
/// Reach in tiles of a lamp fixture.
pub const LAMP_LIGHT_TILES: i32 = 6;
/// Light level (permille) right at a lamp fixture.
pub const LAMP_LIGHT: u16 = 850;
/// Light level (permille) of village streets and the town at night (lamps and lit windows).
pub const SETTLEMENT_LIGHT: u16 = 650;
/// Rows above the local ground a settlement's street light reaches.
const SETTLEMENT_LIGHT_ROWS: i32 = 14;
/// Tiles from a rig's bed it may drift before it counts as having got up.
const BED_DRIFT_TILES: i32 = 1;

impl WorldState {
    /// The world clock now (see [`crate::clock`]).
    #[must_use]
    pub const fn clock(&self) -> WorldClock {
        WorldClock::at(
            self.simulation_tick,
            self.timing.rate,
            &self.clock,
            self.terrain.seed(),
        )
    }

    /// The clock reading at `tick` with the current clock state.
    const fn clock_at(&self, tick: SimulationTick) -> WorldClock {
        WorldClock::at(tick, self.timing.rate, &self.clock, self.terrain.seed())
    }

    /// The clock's saved state: day length, skipped time, the sleep rule and who is in bed.
    #[must_use]
    pub const fn clock_state(&self) -> &ClockState {
        &self.clock
    }

    /// Whether the sun is down.
    #[must_use]
    pub const fn is_night(&self) -> bool {
        self.clock().is_night()
    }

    /// The world's day length.
    #[must_use]
    pub const fn day_length(&self) -> DayLength {
        self.clock.day_length
    }

    /// Milliseconds of play since tick 0 (the clock before its offset).
    const fn tick_millis(&self) -> u64 {
        self.simulation_tick.get().saturating_mul(1_000) / self.timing.rate.hz() as u64
    }

    /// Sets the day length of a world before play starts (the world setup dialog): the clock
    /// restarts on the morning of day 1 (or, for a world already past that, the next morning).
    pub const fn set_day_length(&mut self, day_length: DayLength) {
        self.clock.day_length = day_length;
        let played = self.tick_millis();
        let mut target = start_offset_millis(day_length);
        if played > target {
            let day = day_length.millis();
            target += day * ((played - target) / day + 1);
        }
        self.clock.offset_millis = target - played;
    }

    /// Scenario setup (tests, screenshot scenes) before play: makes the clock read
    /// `world_millis` now (see [`WorldClock::world_millis`]; never earlier than the play time).
    pub const fn set_world_time_millis(&mut self, world_millis: u64) {
        self.clock.offset_millis = world_millis.saturating_sub(self.tick_millis());
    }

    /// Where the Moon hangs over the world (see [`MoonAnchor`]). On a world with a sky
    /// (generator V8+) this is the real Moon landmass ([`crate::worldgen::sky::moon`]): its
    /// column and the row of its underside's lowest point. Older generators have no Moon
    /// landmass, so their backdrop Moon hangs above the seed's default column.
    #[must_use]
    pub fn moon_anchor(&self) -> MoonAnchor {
        crate::worldgen::sky::moon(self.terrain.settings()).map_or_else(
            || default_moon_anchor(self.terrain.seed()),
            |moon| MoonAnchor {
                column: moon.center_x,
                row: crate::worldgen::sky::row_of(crate::worldgen::sky::MOON_BOTTOM_ROWS),
            },
        )
    }

    /// Whether `player_id` is in bed.
    #[must_use]
    pub fn is_in_bed(&self, player_id: PlayerId) -> bool {
        self.clock.sleepers.contains_key(&player_id)
    }

    /// Seconds until the night is skipped, while everyone needed is in bed (the screens fade).
    #[must_use]
    pub fn sleep_countdown_seconds(&self) -> Option<f32> {
        self.clock.sleep_deadline.map(|deadline| {
            self.timing
                .rate
                .seconds_until(self.simulation_tick, deadline)
        })
    }

    /// Seconds since the last night was skipped (the screens fade back in after it).
    #[must_use]
    pub fn seconds_since_night_skipped(&self) -> Option<f32> {
        self.clock.last_skip.map(|tick| {
            self.timing
                .rate
                .seconds_for_ticks(self.simulation_tick.get().saturating_sub(tick.get()))
        })
    }

    /// The players still awake while someone is in bed (who the vote waits for).
    #[must_use]
    pub fn awake_players(&self) -> Vec<PlayerId> {
        self.players
            .keys()
            .filter(|player| !self.clock.sleepers.contains_key(player))
            .copied()
            .collect()
    }

    /// Gameplay light level (permille, 0 dark to 1000 full daylight) at tile `(x, y)`: the open
    /// sky's light (falling off over [`SKY_LIGHT_DEPTH_TILES`] under the local ground, so the
    /// underground stays dark whatever the time), or the brightest light source in reach: rig
    /// lamps, lamp fixtures, the streets of villages and the town, lava, magma vents and glowing
    /// cave growths. Integer math only, so night monsters and other rules may branch on it.
    #[must_use]
    pub fn light_at(&self, x: i32, y: i32) -> u16 {
        let sky = self.clock().light_level();
        let surface = self.terrain.surface_row(x);
        let depth = y - surface;
        let mut light = if depth <= 0 {
            sky
        } else if depth >= SKY_LIGHT_DEPTH_TILES {
            0
        } else {
            let share = u32::try_from(SKY_LIGHT_DEPTH_TILES - depth).unwrap_or(0);
            u16::try_from(u32::from(sky) * share / SKY_LIGHT_DEPTH_TILES.unsigned_abs())
                .unwrap_or(u16::MAX)
        };
        if light >= RIG_LIGHT {
            return light;
        }
        let at = TilePosition { x, y };
        if depth > -SETTLEMENT_LIGHT_ROWS && depth <= 1 && self.in_lit_settlement(at) {
            light = light.max(SETTLEMENT_LIGHT);
        }
        for player in self.players.values() {
            let rig = player.tile_position(TILE_SIZE);
            light = light.max(falloff(rig, at, RIG_LIGHT_TILES, RIG_LIGHT));
        }
        let reach = LAMP_LIGHT_TILES;
        for (position, fixtures) in self.bases.fixtures_in(
            TilePosition {
                x: x - reach,
                y: y - reach,
            },
            TilePosition {
                x: x + reach,
                y: y + reach,
            },
        ) {
            if matches!(fixtures.front, Some((Fixture::Light, _))) {
                light = light.max(falloff(position, at, reach, LAMP_LIGHT));
            }
        }
        for dy in -3..=3 {
            for dx in -3..=3 {
                let position = TilePosition {
                    x: x + dx,
                    y: y + dy,
                };
                let Some(tile) = self.terrain.tile(position) else {
                    continue;
                };
                let (reach, level) = match tile.kind {
                    TileKind::Lava => (3, 700),
                    TileKind::MagmaVent => (2, 600),
                    TileKind::Cave(cave) if cave.light_radius() > 0 => {
                        (i32::from(cave.light_radius()), 450)
                    }
                    _ => continue,
                };
                light = light.max(falloff(position, at, reach, level));
            }
        }
        light.min(crate::clock::DAY_LIGHT)
    }

    /// Whether `at` is in a village or the town, whose lamps and windows light the streets.
    fn in_lit_settlement(&self, at: TilePosition) -> bool {
        let (first, last) = crate::surface::town_span();
        (at.x >= first - 2 && at.x <= last + 2)
            || crate::worldgen::streamed::in_village_area(self.terrain.settings(), at, 2)
    }

    /// Puts `player_id` in `bed` (the bed's service was validated by its caller). Only at dusk
    /// or night.
    pub(crate) fn lie_down(
        &mut self,
        player_id: PlayerId,
        bed: BedSpot,
    ) -> PlayerScopedCommandOutcome {
        if !self.clock().phase().sleepable() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let rig = player.tile_position(TILE_SIZE);
        self.clock.sleepers.insert(
            player_id,
            Sleeper {
                bed,
                x: rig.x,
                y: rig.y,
            },
        );
        PlayerScopedCommandOutcome::Applied
    }

    /// Whether a bed module `kind` can be slept in (the base bed and the homestead).
    #[must_use]
    pub const fn is_bed_module(kind: ModuleKind) -> bool {
        matches!(kind, ModuleKind::Bed | ModuleKind::Homestead)
    }

    /// Resolves a [`SleepCommand`].
    pub(crate) fn apply_sleep_command(
        &mut self,
        player_id: PlayerId,
        command: SleepCommand,
    ) -> PlayerScopedCommandOutcome {
        match command {
            SleepCommand::GetUp => {
                if self.clock.sleepers.remove(&player_id).is_some() {
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                }
            }
            SleepCommand::SetRule(rule) => {
                if self.clock.sleep_rule == rule {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                self.clock.sleep_rule = rule;
                PlayerScopedCommandOutcome::Applied
            }
        }
    }

    /// The clock's part of a tick: gets up anyone whose rig left its bed (or whose bed is gone),
    /// runs the sleep countdown and skips the night, and reports dawn, dusk and each new day.
    /// `before` is the clock at the previous tick.
    pub(super) fn advance_clock(
        &mut self,
        before: WorldClock,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        self.wake_wanderers(output);
        let now = self.clock_at(next_tick);
        let in_bed = self.clock.sleepers.len();
        let needed = match self.clock.sleep_rule {
            SleepRule::Everyone => self.players.len(),
            SleepRule::Anyone => 1,
        };
        let ready = in_bed > 0 && in_bed >= needed && now.phase().sleepable();
        if ready {
            let deadline = *self.clock.sleep_deadline.get_or_insert_with(|| {
                SimulationTick::new(
                    next_tick.get().saturating_add(
                        self.timing
                            .ticks_for_millis(crate::clock::SLEEP_FADE_MILLIS),
                    ),
                )
            });
            if next_tick >= deadline {
                self.clock.offset_millis = self
                    .clock
                    .offset_millis
                    .saturating_add(now.millis_until_dawn());
                self.clock.sleepers.clear();
                self.clock.sleep_deadline = None;
                self.clock.last_skip = Some(next_tick);
                output.push_event(GameplayEventKind::NightSkipped {
                    day: self.clock_at(next_tick).day(),
                });
            }
        } else {
            self.clock.sleep_deadline = None;
            if !now.phase().sleepable() && !self.clock.sleepers.is_empty() {
                // Morning came while someone waited in bed: everyone gets up.
                let sleepers: Vec<PlayerId> = self.clock.sleepers.keys().copied().collect();
                self.clock.sleepers.clear();
                for player_id in sleepers {
                    output.push_event(GameplayEventKind::GotUp { player_id });
                }
            }
        }
        let after = self.clock_at(next_tick);
        if after.day() > before.day() {
            output.push_event(GameplayEventKind::NewDay { day: after.day() });
        }
        if after.phase() != before.phase() {
            match after.phase() {
                DayPhase::Dawn => output.push_event(GameplayEventKind::Dawn { day: after.day() }),
                DayPhase::Dusk => output.push_event(GameplayEventKind::Dusk { day: after.day() }),
                DayPhase::Day | DayPhase::Night => {}
            }
        }
    }

    /// Gets up every sleeper whose rig drifted off its bed, whose bed came down, or who left.
    fn wake_wanderers(&mut self, output: &mut TickOutput) {
        let mut woken = Vec::new();
        for (player_id, sleeper) in &self.clock.sleepers {
            let rig = self
                .players
                .get(player_id)
                .map(|player| player.tile_position(TILE_SIZE));
            let bed_stands = match sleeper.bed {
                BedSpot::Module(structure) => self.bases.module(structure).is_some(),
                BedSpot::Inn(_) => true,
            };
            let stayed = rig.is_some_and(|rig| {
                (rig.x - sleeper.x).abs() <= BED_DRIFT_TILES
                    && (rig.y - sleeper.y).abs() <= BED_DRIFT_TILES
            });
            if !(stayed && bed_stands) {
                woken.push(*player_id);
            }
        }
        for player_id in woken {
            self.clock.sleepers.remove(&player_id);
            if self.players.contains_key(&player_id) {
                output.push_event(GameplayEventKind::GotUp { player_id });
            }
        }
    }
}

/// `level` at `source`, falling off linearly to nothing `reach` tiles away (integer).
fn falloff(source: TilePosition, at: TilePosition, reach: i32, level: u16) -> u16 {
    let (dx, dy) = (
        i64::from(source.x - at.x).abs(),
        i64::from(source.y - at.y).abs(),
    );
    let reach = i64::from(reach);
    if dx > reach || dy > reach {
        return 0;
    }
    // Integer distance: the larger offset plus half the smaller (an octagon close to a circle).
    let distance = dx.max(dy) + dx.min(dy) / 2;
    if distance >= reach {
        return 0;
    }
    u16::try_from(i64::from(level) * (reach - distance) / reach).unwrap_or(0)
}
