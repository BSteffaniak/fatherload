//! The builders' buried city in the world (Wave 10 ancients-a, Descent VII strata 1-3; the
//! rules live in [`crate::ancient_city`]).
//!
//! [`WorldState::advance_ancient_city`] runs once per tick after the far Strange Layers' pass
//! and before the fluid pass. On worlds without Descents it returns at once. In order:
//!
//! 1. **Rides**: every rig on a tram, lift or piston is carried to where its mover is now
//!    (movers are pure functions of world time, [`crate::ancient_city::movers`]); the stick
//!    walks it along the car, cab or head, off it at a stop or off a piston's edge, and thrust
//!    jumps off (a rising piston throws the rig).
//! 2. **Boarding** and the **machinery's hazards**: a rig resting on a tram's floor, in a lift
//!    cab or on a piston head boards it; a crusher slamming onto a rig strikes it once a slam
//!    and shoves it out; a piston's ram shoves a rig out of its way; belts carry resting rigs.
//! 3. **The alarm**: city fabric opened this tick (the terrain journal) raises the alarm of the
//!    rig nearest it; a full alarm starts a hunt.
//! 4. **Plinths and vaults**: a rig or pilot beside a star chart's plinth reads its chart; one
//!    near a vault's unread plinth has opened the vault.
//! 5. **Landmarks found** and **archivists**: cores wake an archivist for each player who comes
//!    near; every archivist follows its rig or pilot, scans for ore and speaks
//!    ([`crate::ancient_city::archivist`]).
//!
//! Hooks elsewhere: the input loop skips drilling and flying for a rig on a ride and records
//! its stick ([`WorldState::note_city_stick`]); creatures' perception, theft and jolts
//! (`world/enemies/ancient_city.rs`).

use crate::{
    ancient_city::{
        ALARM_FULL, ALARM_PER_TILE, ALARM_WATCH, AncientCity, BELT_SPEED, CRUSH_DAMAGE, CityEvent,
        CityLandmark, CitySchedule, CityTile,
        archivist::{
            self, Archivist, LINE_GAP_MILLIS, MUSE_AFTER_MILLIS, SCAN_MARKS, SCAN_MILLIS,
            SCAN_REACH_TILES, Trigger, URGENT_GAP_MILLIS, WAKE_REACH_TILES,
        },
        movers::{
            CityMover, CityRide, LIFT_CAB_TILES, MILLI, MoverKind, PISTON_REBOARD_MILLIS,
            PISTON_TILES, RIDE_WALK_SPEED, StrokePhase, TRAM_CAR_TILES,
        },
        scan_worthy,
    },
    descent::{Descent, Stratum},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    position::Frame,
    quantize::quantize_simulation_scalar as quantize,
    star_chart::{MAX_CHARTS, StarChart, chart_at_site},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::{ancient_city as sites, landmarks},
    },
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// A rising piston head's speed is multiplied this much when a rig thrusts off it ...
const PISTON_LAUNCH_SPRING: f32 = 1.6;
/// ... and this much more upward speed is added (px/s).
const PISTON_LAUNCH_KICK: f32 = 180.0;
/// How far round a rig the machinery is looked for (tiles).
const MOVER_REACH: i32 = 12;
/// A rig this close (tiles) to an unread vault plinth has opened its vault.
const VAULT_OPEN_REACH: f32 = 6.0;
/// ... looked for this many tiles round it.
const VAULT_OPEN_REACH_TILES: i32 = 6;
/// A rig or pilot this close (tiles, centre to centre) to a plinth reads its chart.
const PLINTH_READ_REACH: f32 = 1.75;
/// The archivist moves and looks around this often.
const ARCHIVIST_STEP_MILLIS: u64 = archivist::STEP_MILLIS;
/// Creatures an archivist remarks on are this close (tiles).
const CREATURE_REMARK_TILES: f32 = 9.0;
/// Machinery an archivist hints at is this close (tiles).
const MACHINE_HINT_TILES: i32 = 10;
/// A vault it hints at is this close (tiles).
const VAULT_HINT_TILES: i32 = 14;

/// A buried city event at `tile`.
pub(super) const fn city_event(
    what: CityEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
    detail: u16,
) -> GameplayEventKind {
    GameplayEventKind::AncientCity {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
        detail,
    }
}

/// Local pixels of absolute milli-tile `milli` along columns.
fn milli_x(frame: Frame, milli: i64) -> f32 {
    let column = i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0);
    #[allow(clippy::cast_precision_loss, reason = "below a thousand")]
    let frac = milli.rem_euclid(MILLI) as f32 / 1_000.0;
    frac.mul_add(TILE_SIZE, frame.col_px(column))
}

/// Local pixels of absolute milli-tile `milli` along rows.
fn milli_y(frame: Frame, milli: i64) -> f32 {
    let row = i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0);
    #[allow(clippy::cast_precision_loss, reason = "below a thousand")]
    let frac = milli.rem_euclid(MILLI) as f32 / 1_000.0;
    frac.mul_add(TILE_SIZE, frame.row_px(row))
}

/// Pixels a second of milli-tiles a second.
#[allow(
    clippy::cast_precision_loss,
    reason = "speeds of a few dozen tiles a second"
)]
fn milli_speed(milli_per_second: i64) -> f32 {
    milli_per_second as f32 / 1_000.0 * TILE_SIZE
}

/// Where a rider of `mover` stands at world time `millis`: `(centre x of its free axis, y a rig
/// rests at, velocity x, velocity y, half the span it may walk, whether it waits at a stop)`.
fn ride_frame(mover: &CityMover, frame: Frame, millis: u64) -> Option<RideFrame> {
    let radius = AUTHORITATIVE_PLAYER_RADIUS;
    match mover {
        CityMover::Tram { rail_row, .. } => {
            let state = mover.track_at(millis)?;
            Some(RideFrame {
                x: milli_x(frame, state.pos),
                y: frame.row_px(*rail_row) - radius - 0.5,
                vx: milli_speed(state.vel),
                vy: 0.0,
                half_span: TILE_SIZE.mul_add(
                    f32::from(i8::try_from(TRAM_CAR_TILES).unwrap_or(5)) * 0.5,
                    -radius,
                ) - 2.0,
                at_stop: state.dwelling.is_some(),
                stop: state.dwelling,
            })
        }
        CityMover::Lift { column, .. } => {
            let state = mover.track_at(millis)?;
            Some(RideFrame {
                x: TILE_SIZE.mul_add(0.5, frame.col_px(*column)),
                y: milli_y(frame, state.pos) - radius - 0.5,
                vx: 0.0,
                vy: milli_speed(state.vel),
                half_span: TILE_SIZE.mul_add(
                    f32::from(i8::try_from(LIFT_CAB_TILES).unwrap_or(3)) * 0.5,
                    -radius,
                ) - 1.0,
                at_stop: state.dwelling.is_some(),
                stop: state.dwelling,
            })
        }
        CityMover::Piston { column, .. } => {
            let (top, vel, phase) = mover.piston_top(millis)?;
            Some(RideFrame {
                x: TILE_SIZE.mul_add(0.5, frame.col_px(*column)),
                y: milli_y(frame, top) - radius - 0.5,
                vx: 0.0,
                vy: milli_speed(vel),
                half_span: TILE_SIZE * f32::from(i8::try_from(PISTON_TILES).unwrap_or(3)) * 0.5,
                at_stop: matches!(
                    phase,
                    StrokePhase::Rest | StrokePhase::Tell | StrokePhase::Held
                ),
                stop: None,
            })
        }
        CityMover::Crusher { .. } => None,
    }
}

/// See [`ride_frame`].
#[derive(Clone, Copy, Debug)]
struct RideFrame {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    half_span: f32,
    at_stop: bool,
    stop: Option<usize>,
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The buried city's shared state.
    #[must_use]
    pub const fn ancient_city(&self) -> &AncientCity {
        &self.ancient_city
    }

    /// The buried city's host bookkeeping (tests, tools).
    #[must_use]
    pub const fn city_schedule(&self) -> &CitySchedule {
        &self.city_schedule
    }

    /// The star charts the crew has read, in the order found (the seam Wave 11's space
    /// generator reads; see [`crate::star_chart`]).
    #[must_use]
    pub fn star_charts(&self) -> &[StarChart] {
        &self.ancient_city.charts
    }

    /// Whether the crew has charted star system `system`.
    #[must_use]
    pub fn star_chart_known(&self, system: u64) -> bool {
        self.ancient_city
            .charts
            .iter()
            .any(|chart| chart.system == system)
    }

    /// `player_id`'s archivist, once it woke for them.
    #[must_use]
    pub fn archivist_of(&self, player_id: PlayerId) -> Option<&Archivist> {
        self.ancient_city.archivists.get(&player_id)
    }

    /// `player_id`'s alarm with the Megacity's watch now (per mille).
    #[must_use]
    pub fn city_alarm_of(&self, player_id: PlayerId) -> u32 {
        self.ancient_city
            .alarm_of(player_id, self.millis_at(self.simulation_tick))
    }

    /// Whether the city hunts `player_id` now.
    #[must_use]
    pub fn city_hunts(&self, player_id: PlayerId) -> bool {
        self.ancient_city
            .hunted(player_id, self.millis_at(self.simulation_tick))
    }

    /// The tram, lift or piston `player_id` rides, if any.
    #[must_use]
    pub fn city_ride_of(&self, player_id: PlayerId) -> Option<CityRide> {
        self.ancient_city.rides.get(&player_id).copied()
    }

    /// The movers whose sweep touches the area `reach` tiles round `tile` (the game draws
    /// them; empty on older worlds and away from the buried city).
    #[must_use]
    pub fn city_movers_near(&self, tile: TilePosition, reach: i32) -> Vec<CityMover> {
        let Some(layout) = self.descent_strata_layout() else {
            return Vec::new();
        };
        let (top, bottom) = sites::carve_rows(&layout);
        if tile.y + reach < top || tile.y - reach > bottom {
            return Vec::new();
        }
        sites::movers_with(
            self.terrain.settings(),
            &layout,
            &TileRect::around(tile, reach, reach),
        )
    }

    /// The city stamps overlapping the area `reach` tiles round `tile` (the game dresses them).
    #[must_use]
    pub fn city_stamps_near(&self, tile: TilePosition, reach: i32) -> Vec<sites::CityStamp> {
        let Some(layout) = self.descent_strata_layout() else {
            return Vec::new();
        };
        sites::stamps_with(
            self.terrain.settings(),
            &layout,
            &TileRect::around(tile, reach, reach),
        )
    }

    /// The Great Engine's beat where `player_id` is (ms; the music's tempo), `None` outside the
    /// Engine Halls.
    #[must_use]
    pub fn engine_beat_for(&self, player_id: PlayerId) -> Option<u64> {
        let (x, y) = self.actor_position(player_id)?;
        let tile = self.frame().tile_at(x, y);
        if self.deep_stratum_at(tile)? != Stratum::EngineHalls {
            return None;
        }
        let along = self.descent_strata_layout()?.along(tile.y);
        Some(crate::ancient_city::engine_beat_millis(along))
    }

    /// The Lantern Library (Deep Descent worlds).
    #[must_use]
    pub fn lantern_library(&self) -> Option<TileRect> {
        sites::lantern_library(self.terrain.settings(), &self.descent_strata_layout()?)
            .map(|stamp| stamp.bounds)
    }

    /// The archivists' cores (the Lantern Library's and the Archive Post's).
    #[must_use]
    pub fn archivist_cores(&self) -> Vec<TilePosition> {
        self.descent_strata_layout()
            .map(|layout| sites::archivist_cores(self.terrain.settings(), &layout))
            .unwrap_or_default()
    }

    /// The bounds of `landmark` (Deep Descent worlds).
    #[must_use]
    pub fn city_landmark_bounds(&self, landmark: CityLandmark) -> Option<TileRect> {
        let settings = self.terrain.settings();
        let layout = self.descent_strata_layout()?;
        match landmark {
            CityLandmark::LanternLibrary => {
                sites::lantern_library(settings, &layout).map(|stamp| stamp.bounds)
            }
            CityLandmark::ArchivePost => {
                landmarks::outpost(settings, &layout, Descent::Ancients).map(|hall| hall.bounds)
            }
            CityLandmark::GreatEngine => {
                sites::great_engine(settings, &layout).map(|stamp| stamp.bounds)
            }
            CityLandmark::Orrery => sites::orrery(settings, &layout).map(|stamp| stamp.bounds),
            CityLandmark::FirstVault => None,
        }
    }

    /// Whether `player_id` rides a tram, lift or piston (it neither drills nor flies).
    #[must_use]
    pub fn riding_city_mover(&self, player_id: PlayerId) -> bool {
        self.ancient_city.is_riding(player_id)
    }

    // --- Hooks -------------------------------------------------------------------------------

    /// Records `player_id`'s stick for its ride this tick (the input loop; the ride is stepped
    /// in [`Self::advance_ancient_city`], with or without input).
    pub(super) fn note_city_stick(&mut self, player_id: PlayerId, horizontal: f32, thrust: bool) {
        if self.ancient_city.is_riding(player_id) {
            self.city_schedule
                .sticks
                .insert(player_id, (horizontal, thrust));
        }
    }

    // --- The tick ----------------------------------------------------------------------------

    /// One tick of the buried city (see the module docs).
    pub(super) fn advance_ancient_city(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if !self.terrain.settings().generator.has_descents() {
            return;
        }
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let (top, bottom) = sites::carve_rows(&layout);
        let millis = self.millis_at(next_tick);
        // Who is in the city's rows at all (the rest is skipped cheaply).
        let frame = self.frame();
        let near: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|id| {
                self.actor_position(*id).is_some_and(|(x, y)| {
                    let row = frame.row(y);
                    let _ = x;
                    row >= top - 40 && row <= bottom + 40
                })
            })
            .collect();
        self.step_city_rides(next_tick, millis, output);
        for player_id in &near {
            self.city_machinery(*player_id, next_tick, millis, output);
        }
        self.city_sticks_spent();
        self.raise_city_alarms(millis, output);
        for player_id in &near {
            self.read_plinths(*player_id, next_tick, output);
            self.find_city_landmarks(*player_id, output);
        }
        self.wake_archivists(next_tick, output);
        self.advance_archivists(next_tick, millis, output);
    }

    fn city_sticks_spent(&mut self) {
        if !self.city_schedule.sticks.is_empty() {
            self.city_schedule.sticks.clear();
        }
    }

    /// Carries every rider (see the module docs, step 1).
    #[allow(clippy::too_many_lines, reason = "one pass over the riders")]
    fn step_city_rides(&mut self, next_tick: SimulationTick, millis: u64, output: &mut TickOutput) {
        let riders: Vec<(PlayerId, CityRide)> = self
            .ancient_city
            .rides
            .iter()
            .map(|(id, ride)| (*id, *ride))
            .collect();
        if riders.is_empty() {
            return;
        }
        let settings = *self.terrain.settings();
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let frame = self.frame();
        let dt = self.timing.delta_seconds;
        for (player_id, ride) in riders {
            let wrecked = self
                .players
                .get(&player_id)
                .is_none_or(|player| player.hull <= 0.0);
            let mover = sites::mover_at(&settings, &layout, ride.mover);
            let Some((mover, place)) = mover.and_then(|mover| {
                let place = ride_frame(&mover, frame, millis)?;
                Some((mover, place))
            }) else {
                self.ancient_city.rides.remove(&player_id);
                continue;
            };
            if wrecked || self.is_on_foot(player_id) {
                self.ancient_city.rides.remove(&player_id);
                continue;
            }
            let (stick, thrust) = self
                .city_schedule
                .sticks
                .get(&player_id)
                .copied()
                .unwrap_or((0.0, false));
            let mut offset = (stick.clamp(-1.0, 1.0) * RIDE_WALK_SPEED).mul_add(dt, ride.offset);
            let kind = ride.mover.kind;
            let tile = frame.tile_at(place.x + offset, place.y);
            // Off the edge: a piston any time, a car or cab only while it waits at a stop.
            let leaving =
                offset.abs() > place.half_span && (kind == MoverKind::Piston || place.at_stop);
            if thrust || leaving {
                self.ancient_city.rides.remove(&player_id);
                self.city_schedule.launched.insert(player_id, next_tick);
                let launch = kind == MoverKind::Piston && place.vy < -150.0;
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.x = quantize(
                        place.x + offset.clamp(-place.half_span - 4.0, place.half_span + 4.0),
                    );
                    player.y = quantize(place.y);
                    player.velocity_x =
                        quantize(stick.clamp(-1.0, 1.0).mul_add(RIDE_WALK_SPEED, place.vx));
                    // Thrusting off a rising head springs the rig: the stroke throws it
                    // well past where the head stops (riding the pistons, Wave 10).
                    let vy = if launch && thrust {
                        place.vy.mul_add(PISTON_LAUNCH_SPRING, -PISTON_LAUNCH_KICK)
                    } else {
                        place.vy.min(0.0) + if thrust { -60.0 } else { 0.0 }
                    };
                    player.velocity_y = quantize(vy);
                }
                let what = match kind {
                    MoverKind::Tram => CityEvent::TramAlighted,
                    MoverKind::Lift => CityEvent::LiftAlighted,
                    _ => CityEvent::PistonLaunched,
                };
                if kind != MoverKind::Piston || launch {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a speed of a few hundred px/s"
                    )]
                    let detail = (-place.vy).max(0.0) as u16;
                    output.push_event(city_event(what, Some(player_id), tile, detail));
                }
                continue;
            }
            offset = offset.clamp(-place.half_span, place.half_span);
            // Arriving at a station carrying a rig.
            if kind == MoverKind::Tram {
                let before =
                    ride_frame(&mover, frame, millis.saturating_sub(self.millis_per_tick()));
                if place.at_stop && before.is_some_and(|before| !before.at_stop) {
                    #[allow(clippy::cast_possible_truncation, reason = "a handful of stops")]
                    output.push_event(city_event(
                        CityEvent::TramArrived,
                        Some(player_id),
                        tile,
                        place.stop.unwrap_or(0) as u16,
                    ));
                }
            }
            if let Some(entry) = self.ancient_city.rides.get_mut(&player_id) {
                entry.offset = quantize(offset);
            }
            if let Some(player) = self.players.get_mut(&player_id) {
                player.x = quantize(place.x + offset);
                player.y = quantize(place.y);
                player.velocity_x = quantize(place.vx);
                player.velocity_y = quantize(place.vy);
            }
        }
    }

    /// World milliseconds a tick lasts (at least one).
    fn millis_per_tick(&self) -> u64 {
        (1_000 / u64::from(self.tick_rate_hz().max(1))).max(1)
    }

    /// Boarding, crushers, rams and belts round `player_id`'s rig (see the module docs, step 2).
    #[allow(
        clippy::too_many_lines,
        reason = "one pass over the machinery near a rig"
    )]
    fn city_machinery(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        millis: u64,
        output: &mut TickOutput,
    ) {
        if self.is_on_foot(player_id) {
            return;
        }
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        if player.hull <= 0.0 {
            return;
        }
        let frame = self.frame();
        let (x, y, vx, vy) = (player.x, player.y, player.velocity_x, player.velocity_y);
        let radius = AUTHORITATIVE_PLAYER_RADIUS;
        let tile = frame.tile_at(x, y);
        let riding = self.ancient_city.is_riding(player_id)
            || self.logistics.is_riding(player_id)
            || self.riding_cart(player_id)
            || self.no_clip_active();
        let movers = self.city_movers_near(tile, MOVER_REACH);
        let guard = self
            .city_schedule
            .launched
            .get(&player_id)
            .is_some_and(|tick| {
                self.millis_at(next_tick)
                    .saturating_sub(self.millis_at(*tick))
                    < PISTON_REBOARD_MILLIS
            });
        for mover in &movers {
            match mover {
                CityMover::Crusher { column, .. } => {
                    let Some((bottom, phase)) = mover.crusher_bottom(millis) else {
                        continue;
                    };
                    let half = TILE_SIZE * 1.5;
                    let cx = TILE_SIZE.mul_add(0.5, frame.col_px(*column));
                    let block_bottom = milli_y(frame, bottom);
                    let block_top = TILE_SIZE.mul_add(-2.0, block_bottom);
                    let overlaps = (x - cx).abs() < half + radius
                        && y + radius > block_top
                        && y - radius < block_bottom;
                    if !overlaps {
                        continue;
                    }
                    // Shoved out to the nearer side.
                    let side: f32 = if x < cx { -1.0 } else { 1.0 };
                    let slam = phase == StrokePhase::Out;
                    let struck_recently =
                        self.city_schedule
                            .crushed
                            .get(&player_id)
                            .is_some_and(|tick| {
                                self.millis_at(next_tick)
                                    .saturating_sub(self.millis_at(*tick))
                                    < 900
                            });
                    if slam && !struck_recently {
                        self.city_schedule.crushed.insert(player_id, next_tick);
                        let scale = crate::enemy::tier_scale(self.tier_at(tile));
                        if !self.damage_immune()
                            && self.damage_player(player_id, CRUSH_DAMAGE * scale)
                        {
                            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                        }
                        output.push_event(city_event(CityEvent::Crushed, Some(player_id), tile, 0));
                    }
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.x = quantize(side.mul_add(half + radius + 1.0, cx));
                        player.velocity_x = quantize(side * 220.0);
                    }
                }
                CityMover::Piston {
                    column, floor_row, ..
                } if !riding => {
                    let Some(place) = ride_frame(mover, frame, millis) else {
                        continue;
                    };
                    let head_top = place.y + radius + 0.5;
                    let cx = TILE_SIZE.mul_add(0.5, frame.col_px(*column));
                    let dx = x - cx;
                    let over = dx.abs() <= place.half_span;
                    let bottom = y + radius;
                    // Resting on (or sinking onto) the head: aboard.
                    if over
                        && !guard
                        && bottom >= head_top - 3.0
                        && bottom <= head_top + 9.0
                        && vy >= -60.0
                    {
                        self.board_city(player_id, mover.anchor(), dx, next_tick);
                        let (_, _, phase) =
                            mover
                                .piston_top(millis)
                                .unwrap_or((0, 0, StrokePhase::Rest));
                        if !matches!(phase, StrokePhase::Rest) {
                            output.push_event(city_event(
                                CityEvent::PistonBoarded,
                                Some(player_id),
                                tile,
                                0,
                            ));
                        }
                        continue;
                    }
                    // In the ram's way under a raised head: shoved aside.
                    let floor_px = frame.row_px(*floor_row);
                    let in_ram = dx.abs() < place.half_span + radius
                        && y - radius < floor_px
                        && y + radius > head_top + 2.0;
                    if in_ram && head_top < floor_px - 1.0 {
                        let side: f32 = if dx < 0.0 { -1.0 } else { 1.0 };
                        if let Some(player) = self.players.get_mut(&player_id) {
                            player.x = quantize(side.mul_add(place.half_span + radius + 1.0, cx));
                            player.velocity_x = quantize(side * 160.0);
                        }
                    }
                }
                CityMover::Tram { .. } | CityMover::Lift { .. } if !riding && !guard => {
                    let Some(place) = ride_frame(mover, frame, millis) else {
                        continue;
                    };
                    let dx = x - place.x;
                    let resting = (y - place.y).abs() <= 7.0 && vy.abs() < 260.0;
                    // Aboard only while the car or cab waits at a stop (the same moment at
                    // every tick rate: the rig steps in, it is not scooped up).
                    if dx.abs() <= place.half_span && resting && place.at_stop {
                        let what = if matches!(mover, CityMover::Tram { .. }) {
                            CityEvent::TramBoarded
                        } else {
                            CityEvent::LiftBoarded
                        };
                        self.board_city(player_id, mover.anchor(), dx, next_tick);
                        #[allow(clippy::cast_possible_truncation, reason = "a handful of stops")]
                        output.push_event(city_event(
                            what,
                            Some(player_id),
                            tile,
                            place.stop.unwrap_or(0) as u16,
                        ));
                    }
                }
                _ => {}
            }
        }
        // Belts carry a resting rig.
        if !self.ancient_city.is_riding(player_id) && !riding {
            let under = frame.tile_at(x, y + radius + 1.0);
            let belt = match self.terrain.tile(under).map(|found| found.kind) {
                Some(TileKind::City(city)) => city.belt(),
                _ => None,
            };
            // The belt moves the rig itself, a fixed distance a second (so every tick rate
            // agrees), as far as the way is clear.
            let step = f32::from(belt.unwrap_or(0)) * BELT_SPEED * self.timing.delta_seconds;
            if belt.is_some()
                && vy.abs() < 40.0
                && !super::movement::authoritative_player_collides(&self.terrain, x + step, y)
                && let Some(player) = self.players.get_mut(&player_id)
            {
                let (next, rest) = super::movement::integrate_x(x, player.x_remainder, step);
                player.x = next;
                player.x_remainder = rest;
                let _ = (vx, radius);
            }
        }
    }

    /// Puts `player_id` aboard `mover` at `offset` px from its centre.
    fn board_city(
        &mut self,
        player_id: PlayerId,
        mover: crate::ancient_city::MoverRef,
        offset: f32,
        tick: SimulationTick,
    ) {
        self.drop_active_drill(player_id);
        self.ancient_city.rides.insert(
            player_id,
            CityRide {
                mover,
                offset: quantize(offset),
                since: tick,
            },
        );
    }

    /// Stops `player_id`'s drill (boarding a mover).
    fn drop_active_drill(&mut self, player_id: PlayerId) {
        self.active_drills.remove(&player_id);
        self.active_drill_timers.remove(&player_id);
    }

    /// Raises the alarm for city fabric opened this tick (see the module docs, step 3).
    fn raise_city_alarms(&mut self, millis: u64, output: &mut TickOutput) {
        let opened: Vec<TilePosition> = self
            .terrain
            .journal_with_before()
            .filter_map(|(change, before)| match before {
                TileKind::City(tile) if tile.alarms() && change.tile.kind.is_open() => {
                    Some(change.position)
                }
                _ => None,
            })
            .collect();
        for position in opened {
            if self.deep_stratum_at(position) != Some(Stratum::BuriedMegacity) {
                continue;
            }
            let Some(culprit) = self.city_rig_near(position) else {
                continue;
            };
            let alarm = self.ancient_city.alarms.entry(culprit).or_default();
            let before = alarm.level_at(millis);
            let hunt = alarm.raise(ALARM_PER_TILE, millis);
            let after = alarm.level_at(millis);
            if hunt {
                output.push_event(city_event(
                    CityEvent::AlarmHunt,
                    Some(culprit),
                    position,
                    1_000,
                ));
                self.archivist_say(culprit, Trigger::AlarmHunt, output);
            } else if before < ALARM_WATCH && after >= ALARM_WATCH {
                #[allow(clippy::cast_possible_truncation, reason = "per mille")]
                output.push_event(city_event(
                    CityEvent::AlarmRising,
                    Some(culprit),
                    position,
                    after.min(ALARM_FULL) as u16,
                ));
            }
        }
        // Spent alarms are forgotten.
        self.ancient_city
            .alarms
            .retain(|_, alarm| !alarm.spent(millis));
    }

    /// The rig nearest `tile` (within a dozen tiles), if any.
    fn city_rig_near(&self, tile: TilePosition) -> Option<PlayerId> {
        let (x, y) = self.frame().tile_center(tile);
        self.players
            .iter()
            .map(|(id, rig)| ((rig.x - x).hypot(rig.y - y), *id))
            .filter(|(distance, _)| *distance <= TILE_SIZE * 12.0)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id)
    }

    /// A rig or pilot beside a plinth reads its chart; near a vault's unread plinth it has
    /// opened the vault (see the module docs, step 4).
    fn read_plinths(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((x, y)) = self.actor_position(player_id) else {
            return;
        };
        let frame = self.frame();
        let centre = frame.tile_at(x, y);
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let (archive_top, archive_bottom) = layout.rows(Stratum::ArchiveOfStars);
        if centre.y < archive_top - 30 || centre.y > archive_bottom + 30 {
            return;
        }
        let settings = *self.terrain.settings();
        // Vaults opened: near an unread plinth inside one.
        let reach = VAULT_OPEN_REACH_TILES;
        if let Some((stamp, _kind)) = sites::vault_near(&settings, &layout, centre, reach)
            && let Some(plinth) = sites::vault_plinth(&stamp)
        {
            let (px, py) = frame.tile_center(plinth);
            let close = (px - x).hypot(py - y) <= VAULT_OPEN_REACH * TILE_SIZE;
            let unread = self.terrain.tile(plinth).map(|tile| tile.kind)
                == Some(TileKind::City(CityTile::ChartPlinth));
            if close && unread && self.ancient_city.vaults.insert(stamp.bounds.min) {
                output.push_event(city_event(
                    CityEvent::VaultOpened,
                    Some(player_id),
                    plinth,
                    0,
                ));
                self.archivist_say(player_id, Trigger::VaultOpened, output);
                if self.ancient_city.found.insert(CityLandmark::FirstVault) {
                    self.announce_city_landmark(
                        player_id,
                        CityLandmark::FirstVault,
                        plinth,
                        output,
                    );
                }
            }
        }
        // Plinths in reach.
        for dy in -2..=2 {
            for dx in -2..=2 {
                let at = TilePosition {
                    x: centre.x + dx,
                    y: centre.y + dy,
                };
                if self.terrain.tile(at).map(|tile| tile.kind)
                    != Some(TileKind::City(CityTile::ChartPlinth))
                {
                    continue;
                }
                let (px, py) = frame.tile_center(at);
                if (px - x).hypot(py - y) > PLINTH_READ_REACH * TILE_SIZE {
                    continue;
                }
                let _ = self
                    .terrain
                    .set_kind_by_rule(at, TileKind::City(CityTile::ChartPlinthRead));
                let along = layout.along(at.y);
                let chart = chart_at_site(settings.seed, at.x, at.y, along, next_tick);
                if !self.star_chart_known(chart.system)
                    && self.ancient_city.charts.len() < MAX_CHARTS
                {
                    self.ancient_city.charts.push(chart);
                }
                let count = self.ancient_city.charts.len();
                #[allow(clippy::cast_possible_truncation, reason = "below MAX_CHARTS")]
                output.push_event(city_event(
                    CityEvent::ChartRead,
                    Some(player_id),
                    at,
                    count.saturating_sub(1) as u16,
                ));
                self.archivist_say(player_id, Trigger::Chart(count), output);
            }
        }
    }

    /// Landmarks found (see the module docs, step 5).
    fn find_city_landmarks(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let Some(tile) = self.actor_tile(player_id) else {
            return;
        };
        for landmark in [
            CityLandmark::LanternLibrary,
            CityLandmark::ArchivePost,
            CityLandmark::GreatEngine,
            CityLandmark::Orrery,
        ] {
            if self.ancient_city.found.contains(&landmark) {
                continue;
            }
            let Some(bounds) = self.city_landmark_bounds(landmark) else {
                continue;
            };
            if bounds.contains(tile) {
                self.ancient_city.found.insert(landmark);
                self.announce_city_landmark(player_id, landmark, tile, output);
            }
        }
    }

    fn announce_city_landmark(
        &mut self,
        player_id: PlayerId,
        landmark: CityLandmark,
        tile: TilePosition,
        output: &mut TickOutput,
    ) {
        let index = CityLandmark::ALL
            .iter()
            .position(|known| *known == landmark)
            .unwrap_or(0);
        #[allow(clippy::cast_possible_truncation, reason = "a handful of landmarks")]
        output.push_event(city_event(
            CityEvent::LandmarkFound,
            Some(player_id),
            tile,
            index as u16,
        ));
        self.archivist_say(player_id, Trigger::Landmark(landmark), output);
    }

    /// Wakes an archivist for every player near a core who has none yet.
    fn wake_archivists(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let cores = self.archivist_cores();
        if cores.is_empty() {
            return;
        }
        let frame = self.frame();
        let sleepers: Vec<(PlayerId, f32, f32)> = self
            .players
            .keys()
            .filter(|id| !self.ancient_city.archivists.contains_key(id))
            .filter_map(|id| self.actor_position(*id).map(|(x, y)| (*id, x, y)))
            .collect();
        for (player_id, x, y) in sleepers {
            let Some(core) = cores.iter().copied().find(|core| {
                let (cx, cy) = frame.tile_center(*core);
                (cx - x).hypot(cy - y) <= WAKE_REACH_TILES * TILE_SIZE
            }) else {
                continue;
            };
            let (cx, cy) = frame.tile_center(core);
            self.ancient_city
                .archivists
                .insert(player_id, Archivist::woken(next_tick, cx, cy));
            output.push_event(city_event(
                CityEvent::ArchivistWoke,
                Some(player_id),
                core,
                0,
            ));
            self.archivist_say(player_id, Trigger::Woke, output);
        }
    }

    /// Every archivist follows, scans and speaks (see [`crate::ancient_city::archivist`]).
    fn advance_archivists(
        &mut self,
        next_tick: SimulationTick,
        millis: u64,
        output: &mut TickOutput,
    ) {
        if self.ancient_city.archivists.is_empty() {
            return;
        }
        let previous = self.millis_at(self.simulation_tick);
        // The drones move (and look round) on whole quarter seconds of world time: the same
        // steps at every tick rate, and the shared section changes four times a second, not
        // every tick (the game draws each drone gliding between its last two points).
        let look = millis / ARCHIVIST_STEP_MILLIS != previous / ARCHIVIST_STEP_MILLIS;
        if !look {
            return;
        }
        #[allow(clippy::cast_precision_loss, reason = "a quarter second")]
        let step = ARCHIVIST_STEP_MILLIS as f32 / 1_000.0;
        let owners: Vec<PlayerId> = self.ancient_city.archivists.keys().copied().collect();
        for player_id in owners {
            if !self.players.contains_key(&player_id) {
                continue;
            }
            let Some((x, y)) = self.actor_position(player_id) else {
                continue;
            };
            let facing = self.actor_facing(player_id);
            let target = archivist::hover_point(x, y, facing);
            if let Some(archivist) = self.ancient_city.archivists.get_mut(&player_id) {
                let (nx, ny) = archivist::follow_step((archivist.x, archivist.y), target, step);
                archivist.from = (archivist.x, archivist.y);
                archivist.x = quantize(nx);
                archivist.y = quantize(ny);
            }
            self.archivist_scan(player_id, next_tick, millis, output);
            self.archivist_look_around(player_id, millis, output);
        }
    }

    /// Which way `player_id`'s rig or pilot faces (-1 left, 1 right).
    fn actor_facing(&self, player_id: PlayerId) -> f32 {
        if let Some((_, _, pilot)) = self.pilot_of(player_id) {
            return f32::from(pilot.facing);
        }
        let Some(player) = self.players.get(&player_id) else {
            return 1.0;
        };
        if player.velocity_x.abs() > 25.0 {
            return player.velocity_x.signum();
        }
        // Keep the side it hovers on: it stays behind until the rig turns for real.
        self.ancient_city
            .archivists
            .get(&player_id)
            .map_or(
                1.0,
                |archivist| if archivist.x > player.x { -1.0 } else { 1.0 },
            )
    }

    /// Marks the best ore round `player_id` every [`SCAN_MILLIS`].
    fn archivist_scan(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        millis: u64,
        output: &mut TickOutput,
    ) {
        let Some(archivist) = self.ancient_city.archivists.get(&player_id) else {
            return;
        };
        if millis.saturating_sub(self.millis_at(archivist.scan_tick)) < SCAN_MILLIS {
            return;
        }
        let Some(centre) = self.actor_tile(player_id) else {
            return;
        };
        let reach = SCAN_REACH_TILES;
        let mut found: Vec<(u32, i32, TilePosition)> = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let at = TilePosition {
                    x: centre.x + dx,
                    y: centre.y + dy,
                };
                let worth = match self.terrain.tile(at).map(|tile| tile.kind) {
                    Some(TileKind::Ore(mineral)) if scan_worthy(mineral) => mineral.value(),
                    Some(TileKind::City(
                        CityTile::AlloyOre | CityTile::EngineCoreOre | CityTile::StarGlass,
                    )) => 400,
                    Some(TileKind::Cave(cave)) if cave.is_cache() => 900,
                    _ => continue,
                };
                found.push((worth, dx * dx + dy * dy, at));
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let marks: Vec<TilePosition> = found
            .iter()
            .take(SCAN_MARKS)
            .map(|(_, _, at)| *at)
            .collect();
        let first = self
            .ancient_city
            .archivists
            .get(&player_id)
            .is_some_and(|archivist| !archivist.has_said(80));
        let changed = self
            .ancient_city
            .archivists
            .get(&player_id)
            .is_some_and(|archivist| archivist.scan != marks);
        if let Some(archivist) = self.ancient_city.archivists.get_mut(&player_id) {
            archivist.scan_tick = next_tick;
            archivist.scan.clone_from(&marks);
        }
        if !marks.is_empty() && changed {
            #[allow(clippy::cast_possible_truncation, reason = "a few marks")]
            output.push_event(city_event(
                CityEvent::ArchivistScanned,
                Some(player_id),
                marks[0],
                marks.len() as u16,
            ));
            if first {
                self.archivist_say(player_id, Trigger::Scanned, output);
            }
        }
    }

    /// What `player_id`'s archivist sees round its rig, in the order it would rather talk
    /// about it (see [`crate::ancient_city::archivist`]).
    fn archivist_look_around(&mut self, player_id: PlayerId, millis: u64, output: &mut TickOutput) {
        let Some((x, y)) = self.actor_position(player_id) else {
            return;
        };
        let frame = self.frame();
        let tile = frame.tile_at(x, y);
        let mut triggers: Vec<Trigger> = Vec::new();
        if let Some(player) = self.players.get(&player_id) {
            if player.hull > 0.0 && player.hull < player.max_hull() * 0.25 {
                triggers.push(Trigger::LowHull);
            }
            if player.fuel < player.fuel_capacity * 0.2 {
                triggers.push(Trigger::LowFuel);
            }
        }
        if let Some(stratum) = self.deep_stratum_at(tile) {
            triggers.push(Trigger::Stratum(stratum));
        }
        if self.city_hunts(player_id) {
            triggers.push(Trigger::AlarmHunt);
        } else if self.city_alarm_of(player_id) >= ALARM_WATCH {
            triggers.push(Trigger::AlarmWatch);
        }
        // Creatures in sight.
        triggers.extend(
            self.enemies()
                .filter(|(entity, state)| {
                    state.kind.is_ancient_city()
                        && (entity.body.x - x).hypot(entity.body.y - y)
                            <= CREATURE_REMARK_TILES * TILE_SIZE
                })
                .map(|(_, state)| Trigger::Creature(state.kind)),
        );
        if let Some(ride) = self.ancient_city.rides.get(&player_id) {
            triggers.push(Trigger::Riding(ride.mover.kind));
        }
        for mover in self.city_movers_near(tile, MACHINE_HINT_TILES) {
            triggers.push(match mover {
                CityMover::Tram { .. } => Trigger::NearTram,
                CityMover::Lift { .. } => Trigger::NearLift,
                CityMover::Piston { .. } => Trigger::NearPiston,
                CityMover::Crusher { .. } => Trigger::NearCrusher,
            });
        }
        if let Some(layout) = self.descent_strata_layout()
            && let Some((stamp, kind)) =
                sites::vault_near(self.terrain.settings(), &layout, tile, VAULT_HINT_TILES)
            && !self.ancient_city.vaults.contains(&stamp.bounds.min)
        {
            triggers.push(Trigger::NearVault(kind));
        }
        // The Fathers' logs read since it last remarked.
        let found = self.expedition.found;
        let seen = self
            .city_schedule
            .logs_seen
            .get(&player_id)
            .copied()
            .unwrap_or(0);
        if found > seen {
            self.city_schedule.logs_seen.insert(player_id, found);
            if seen > 0 {
                triggers.push(Trigger::Log(found));
            }
        }
        // After a long silence in the builders' world, a musing.
        let in_builders_world = self
            .deep_stratum_at(tile)
            .is_some_and(|stratum| stratum.descent().index() >= Descent::Ancients.index());
        let quiet = self
            .ancient_city
            .archivists
            .get(&player_id)
            .and_then(|archivist| archivist.line)
            .is_none_or(|(_, when)| {
                millis.saturating_sub(self.millis_at(when)) >= MUSE_AFTER_MILLIS
            });
        if in_builders_world && quiet {
            triggers.push(Trigger::Muse);
        }
        for trigger in triggers {
            if self.archivist_say(player_id, trigger, output) {
                break;
            }
        }
    }

    /// `player_id`'s archivist says the line answering `trigger` if it has one left and the gap
    /// since its last line allows. Returns whether it spoke.
    pub(super) fn archivist_say(
        &mut self,
        player_id: PlayerId,
        trigger: Trigger,
        output: &mut TickOutput,
    ) -> bool {
        let now = self.simulation_tick.next();
        let now_millis = self.millis_at(now);
        let Some(archivist) = self.ancient_city.archivists.get(&player_id) else {
            return false;
        };
        let gap = if trigger.urgent() {
            URGENT_GAP_MILLIS
        } else {
            LINE_GAP_MILLIS
        };
        if let Some((_, when)) = archivist.line
            && now_millis.saturating_sub(self.millis_at(when)) < gap
            && trigger != Trigger::Woke
        {
            return false;
        }
        let Some(line) = archivist::line_for(archivist, trigger) else {
            return false;
        };
        let id = line.id;
        let frame = self.frame();
        let tile = frame.tile_at(archivist.x, archivist.y);
        if let Some(archivist) = self.ancient_city.archivists.get_mut(&player_id) {
            archivist.said.insert(id);
            archivist.line = Some((id, now));
            if trigger == Trigger::Muse {
                archivist.musing = archivist.musing.saturating_add(1);
            }
        }
        output.push_event(city_event(
            CityEvent::ArchivistSpoke,
            Some(player_id),
            tile,
            id,
        ));
        true
    }

    // --- Dev and scene seams ----------------------------------------------------------------

    /// Wakes `player_id`'s archivist where its rig is (tests, scenes, tools).
    pub fn wake_archivist_for(&mut self, player_id: PlayerId) {
        if self.ancient_city.archivists.contains_key(&player_id) {
            return;
        }
        if let Some((x, y)) = self.actor_position(player_id) {
            let at = self.simulation_tick;
            self.ancient_city
                .archivists
                .insert(player_id, Archivist::woken(at, x, y - 30.0));
        }
    }

    /// Makes `player_id`'s archivist say line `id` now (scenes).
    pub fn stage_archivist_line(&mut self, player_id: PlayerId, id: u16) {
        let now = self.simulation_tick;
        if let Some(archivist) = self.ancient_city.archivists.get_mut(&player_id) {
            archivist.said.insert(id);
            archivist.line = Some((id, now));
        }
    }

    /// Adds the chart of the plinth at `site` to the crew's list (tests, scenes).
    pub fn stage_star_chart(&mut self, site: TilePosition) {
        let settings = *self.terrain.settings();
        let along = self
            .descent_strata_layout()
            .map_or(500, |layout| layout.along(site.y));
        let chart = chart_at_site(settings.seed, site.x, site.y, along, self.simulation_tick);
        if !self.star_chart_known(chart.system) {
            self.ancient_city.charts.push(chart);
        }
    }

    /// Replaces the buried city's shared state (tests, scenes).
    pub fn set_ancient_city(&mut self, city: AncientCity) {
        self.ancient_city = city;
    }

    /// Raises `player_id`'s alarm to `level` per mille now (tests, scenes).
    pub fn stage_city_alarm(&mut self, player_id: PlayerId, level: u32) {
        let millis = self.millis_at(self.simulation_tick);
        let alarm = self.ancient_city.alarms.entry(player_id).or_default();
        let _ = alarm.raise(level.min(ALARM_FULL), millis);
    }
}
