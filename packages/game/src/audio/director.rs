//! Turns a frame of the game into mixer commands: one-shot cues placed in the stereo field, and
//! the continuous sounds read straight from the snapshot — each seat's thrust (following its
//! throttle), drilling (per material class and drill speed, with crunch transients, ore glints
//! and break-through), landings and wall bumps scaled by impact, escalating low-fuel/low-hull
//! alarms, and nearby gas, lava, boulders and bomb fuses.
//!
//! Spatialization: in split-screen each seat's own sounds pan toward its viewport; world sounds
//! attenuate (and darken) with distance from the nearest local rig and pan by which side of it
//! they are on.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "positions and timers convert between world pixels, tiles and seconds"
)]

use std::collections::{BTreeMap, BTreeSet};

use drillgame_simulation::{
    PlayerId, TILE_SIZE,
    terrain::{TileKind, TilePosition},
};

use super::{
    cues::{PlacedCue, SoundSource, cue_patch},
    dsp::Rng,
    mixer::{LoopKey, LoopRequest, MixerCommand, OPEN_CUTOFF, PlayRequest},
    patch::Patch,
};
use crate::{render_snapshot::RenderSnapshot, shell::SoundCue};

/// How far each seat's sounds pan toward its side of a split screen.
const SEAT_PAN_SPREAD: f32 = 0.6;
/// World sounds play at full level within this distance of a local rig (world pixels).
const FULL_LEVEL_DISTANCE: f32 = TILE_SIZE * 6.0;
/// World sounds are inaudible beyond this distance.
const MAX_DISTANCE: f32 = TILE_SIZE * 40.0;
/// Rig foot offset below its centre (matches the renderer's airborne test).
const FOOT_OFFSET: f32 = 12.0;
/// A fall faster than this makes a landing sound (px/s).
const LANDING_SPEED: f32 = 90.0;
/// Fall speed at and above which a landing hurts (the simulation's safe landing speed).
const HARD_LANDING_SPEED: f32 = 330.0;
/// Horizontal speed lost in one frame that counts as hitting a wall (px/s).
const BUMP_SPEED: f32 = 90.0;
/// How long a toast waits for another cue before chiming (a message that arrives with its own
/// sound, like a purchase, stays quiet).
const TOAST_DELAY_SECONDS: f32 = 0.2;

/// What the director needs to know about each local seat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatAudio {
    /// The seat holds thrust.
    pub thrust: bool,
    /// The seat is outdoors driving its rig (no menu or building open).
    pub playing: bool,
    /// When the seat's toast message last changed (`SeatUi::message_tick`).
    pub message_tick: u64,
}

/// One frame of input to the director.
#[derive(Clone, Copy, Debug)]
pub struct AudioFrame<'a, 'w> {
    pub snapshot: &'a RenderSnapshot<'w>,
    pub cues: &'a [PlacedCue],
    /// Per seat, in seat order.
    pub seats: &'a [SeatAudio],
    /// The world is ticking (not paused, not on the title screen).
    pub world_running: bool,
    /// When the global toast message last changed (`ShellState::message_tick`).
    pub message_tick: u64,
    pub delta_seconds: f32,
}

/// Drilling material classes (each has its own loop and transients).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Material {
    /// Dirt, clay, gas pockets: a soft scrape.
    Soft,
    /// Stone, hard rock, fixtures: a grind with crunch transients.
    Hard,
    /// Ore and artifacts: the grind plus crystalline glints.
    Ore,
}

impl Material {
    #[must_use]
    pub const fn of(kind: TileKind) -> Self {
        match kind {
            TileKind::Air
            | TileKind::Dirt
            | TileKind::Clay
            | TileKind::Gas
            | TileKind::Lava
            | TileKind::PressurePocket => Self::Soft,
            TileKind::Stone
            | TileKind::HardRock
            | TileKind::Foundation
            | TileKind::ExplosivePocket
            | TileKind::MagmaVent
            | TileKind::Poi(_)
            | TileKind::Bedrock => Self::Hard,
            TileKind::Ore(_) | TileKind::Artifact(_) => Self::Ore,
        }
    }

    const fn loop_patch(self) -> Patch {
        match self {
            Self::Soft => Patch::DrillSoftLoop,
            Self::Hard | Self::Ore => Patch::DrillHardLoop,
        }
    }

    const fn break_patch(self) -> Patch {
        match self {
            Self::Soft => Patch::BreakSoft,
            Self::Hard | Self::Ore => Patch::BreakHard,
        }
    }
}

/// Loop slots per seat (each continuous sound gets its own key).
#[derive(Clone, Copy)]
enum SeatLoop {
    Thrust,
    Drill,
    Lava,
    Gas,
    Boulders,
}

/// Where a local seat hears from.
#[derive(Clone, Copy, Debug)]
struct Listener {
    seat: usize,
    player_id: PlayerId,
    x: f32,
    y: f32,
    /// Pan of this seat's own sounds.
    pan: f32,
    /// Half the view's width in world pixels (panning scale for world sounds).
    half_width: f32,
}

#[derive(Clone, Debug, Default)]
struct SeatState {
    player: Option<PlayerId>,
    initialized: bool,
    velocity_x: f32,
    velocity_y: f32,
    airborne: bool,
    throttle: f32,
    thrust_held: bool,
    drill: Option<(TilePosition, Material)>,
    crunch_timer: f32,
    glint_timer: f32,
    fuel_alarm: f32,
    hull_alarm: f32,
    message_tick: u64,
}

/// See the module docs.
#[derive(Debug)]
pub struct Director {
    rng: Rng,
    seats: Vec<SeatState>,
    commands: Vec<MixerCommand>,
    /// Loops requested last frame (so dropped ones are faded explicitly).
    live_loops: BTreeSet<LoopKey>,
    /// Next fuse tick per bomb, keyed by its rounded position.
    fuses: BTreeMap<(i32, i32), f32>,
    message_tick: u64,
    /// A toast waiting to chime: (seconds left, pan).
    pending_toast: Option<(f32, f32)>,
    was_running: bool,
}

impl Director {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            seats: Vec::new(),
            commands: Vec::with_capacity(64),
            live_loops: BTreeSet::new(),
            fuses: BTreeMap::new(),
            message_tick: 0,
            pending_toast: None,
            was_running: false,
        }
    }

    /// Works out this frame's mixer commands.
    pub fn update(&mut self, frame: &AudioFrame<'_, '_>) -> &[MixerCommand] {
        self.commands.clear();
        let listeners = listeners(frame.snapshot);
        let mut cue_played = false;
        for cue in frame.cues {
            if let Some(request) = Self::place_cue(*cue, frame.snapshot, &listeners) {
                self.commands.push(MixerCommand::Play(request));
                cue_played = true;
            }
        }
        self.toasts(frame, &listeners, cue_played);
        if frame.world_running {
            let mut live = BTreeSet::new();
            for listener in &listeners {
                self.seat_sounds(frame, listener, &mut live);
            }
            self.fuse_ticks(frame, &listeners);
            for key in self.live_loops.difference(&live) {
                // Fade loops nobody asked for this frame (the patch is ignored for a fade).
                self.commands.push(MixerCommand::Loop(LoopRequest {
                    key: *key,
                    patch: Patch::ThrustLoop,
                    gain: 0.0,
                    pan: 0.0,
                    rate: 1.0,
                    cutoff: OPEN_CUTOFF,
                }));
            }
            self.live_loops = live;
        } else {
            if self.was_running || !self.live_loops.is_empty() {
                self.commands.push(MixerCommand::StopLoops);
            }
            self.live_loops.clear();
            for seat in &mut self.seats {
                seat.initialized = false;
                seat.throttle = 0.0;
                seat.drill = None;
            }
        }
        self.was_running = frame.world_running;
        &self.commands
    }

    /// Gain, pan and cutoff for a sound from `source`, or `None` when it is out of earshot.
    fn place(
        source: SoundSource,
        snapshot: &RenderSnapshot<'_>,
        listeners: &[Listener],
    ) -> Option<(f32, f32, f32)> {
        match source {
            SoundSource::Interface => Some((1.0, 0.0, 20_000.0)),
            SoundSource::Player(player) => {
                if let Some(listener) = listeners.iter().find(|l| l.player_id == player) {
                    return Some((1.0, listener.pan, 20_000.0));
                }
                let presentation = snapshot.players.iter().find(|p| p.player_id == player)?;
                spatial(presentation.x, presentation.y, listeners)
            }
            SoundSource::World { x, y } => spatial(x as f32, y as f32, listeners),
        }
    }

    fn place_cue(
        cue: PlacedCue,
        snapshot: &RenderSnapshot<'_>,
        listeners: &[Listener],
    ) -> Option<PlayRequest> {
        let (gain, pan, cutoff) = Self::place(cue.source, snapshot, listeners)?;
        let patch = cue_patch(cue.cue);
        let gain = gain
            * match cue.cue {
                SoundCue::Drill => 0.6,
                _ => 1.0,
            };
        Some(PlayRequest {
            patch,
            gain,
            pan,
            rate: 1.0,
            cutoff,
        })
    }

    /// A toast chimes shortly after its message appears, unless another sound plays first.
    fn toasts(&mut self, frame: &AudioFrame<'_, '_>, listeners: &[Listener], cue_played: bool) {
        let mut newest = (frame.message_tick > self.message_tick).then_some(0.0);
        self.message_tick = self.message_tick.max(frame.message_tick);
        for (index, seat) in frame.seats.iter().enumerate() {
            let state = self.seat_state(index);
            if seat.message_tick > state.message_tick {
                let first = state.message_tick == 0 && !state.initialized;
                state.message_tick = seat.message_tick;
                if !first {
                    let pan = listeners
                        .iter()
                        .find(|listener| listener.seat == index)
                        .map_or(0.0, |listener| listener.pan);
                    newest = Some(pan);
                }
            }
        }
        if let Some(pan) = newest {
            self.pending_toast = Some((TOAST_DELAY_SECONDS, pan));
        }
        if cue_played {
            self.pending_toast = None;
        }
        if let Some((left, pan)) = self.pending_toast {
            let left = left - frame.delta_seconds;
            if left <= 0.0 {
                self.pending_toast = None;
                self.commands.push(MixerCommand::Play(PlayRequest {
                    pan,
                    ..PlayRequest::new(Patch::Toast)
                }));
            } else {
                self.pending_toast = Some((left, pan));
            }
        }
    }

    fn seat_state(&mut self, seat: usize) -> &mut SeatState {
        if self.seats.len() <= seat {
            self.seats.resize_with(seat + 1, SeatState::default);
        }
        &mut self.seats[seat]
    }

    fn loop_command(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        slot: SeatLoop,
        patch: Patch,
        params: (f32, f32, f32, f32),
    ) {
        let (gain, pan, rate, cutoff) = params;
        let key = LoopKey::new(listener.seat as u32 * 8 + slot as u32, patch);
        if gain > 1e-3 {
            live.insert(key);
        }
        self.commands.push(MixerCommand::Loop(LoopRequest {
            key,
            patch,
            gain,
            pan,
            rate,
            cutoff,
        }));
    }

    fn play(&mut self, patch: Patch, gain: f32, pan: f32, rate: f32) {
        self.commands.push(MixerCommand::Play(PlayRequest {
            patch,
            gain,
            pan,
            rate,
            cutoff: 20_000.0,
        }));
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one pass over a seat's continuous sounds reads best top to bottom"
    )]
    fn seat_sounds(
        &mut self,
        frame: &AudioFrame<'_, '_>,
        listener: &Listener,
        live: &mut BTreeSet<LoopKey>,
    ) {
        let snapshot = frame.snapshot;
        let Some(view) = snapshot
            .views
            .iter()
            .find(|view| view.seat_index == listener.seat)
        else {
            return;
        };
        let seat_input = frame.seats.get(listener.seat).copied().unwrap_or_default();
        let dt = frame.delta_seconds.max(0.0);
        let pan = listener.pan;
        let terrain = snapshot.terrain;
        let presentation = snapshot
            .players
            .iter()
            .find(|player| player.player_id == listener.player_id);
        let (velocity_x, velocity_y) =
            presentation.map_or((0.0, 0.0), |p| (p.velocity_x, p.velocity_y));
        let foot = TilePosition {
            x: (listener.x / TILE_SIZE).floor() as i32,
            y: ((listener.y + FOOT_OFFSET + 3.0) / TILE_SIZE).floor() as i32,
        };
        let airborne = !terrain.is_solid_at(foot);
        let disabled = view.failure.is_some();
        let playing = seat_input.playing && !disabled;
        let drill = view.active_drill.filter(|_| playing);
        let fuel = view.hud.fuel;
        let fuel_fraction = fuel / view.hud.fuel_capacity.max(1.0);
        let hull_fraction = view.hud.hull / view.hud.max_hull.max(1.0);

        let mut state = std::mem::take(self.seat_state(listener.seat));
        if state.player != Some(listener.player_id) {
            state = SeatState {
                player: Some(listener.player_id),
                message_tick: state.message_tick,
                ..SeatState::default()
            };
        }

        // Thrust: follows the held throttle, climbing opens it up.
        let thrusting = seat_input.thrust && playing && fuel > 0.0;
        if thrusting && !state.thrust_held && state.throttle < 0.25 {
            let rate = self.rng.jitter(0.04);
            self.play(Patch::ThrustIgnite, 0.45, pan, rate);
        }
        state.thrust_held = thrusting;
        let target = if thrusting { 1.0 } else { 0.0 };
        let speed = if target > state.throttle { 12.0 } else { 7.0 };
        state.throttle += (target - state.throttle) * (1.0 - (-dt * speed).exp());
        let climb = (-velocity_y / 190.0).clamp(0.0, 1.0);
        let throttle = state.throttle;
        self.loop_command(
            live,
            listener,
            SeatLoop::Thrust,
            Patch::ThrustLoop,
            (
                throttle * (0.55 + 0.45 * climb),
                pan,
                0.85 + 0.15 * throttle + 0.1 * climb,
                700.0 + 9_000.0 * throttle.powf(1.5) * (0.7 + 0.3 * climb),
            ),
        );

        // Drilling: loop per material class, rate from drill speed, plus transients.
        let current = drill.map(|drill| {
            let kind = terrain
                .tile(drill.target)
                .map_or(TileKind::Air, |tile| tile.kind);
            (drill, Material::of(kind))
        });
        if let Some((previous_target, material)) = state.drill {
            let moved_on = current.is_none_or(|(drill, _)| drill.target != previous_target);
            if moved_on && state.initialized && !terrain.is_solid_at(previous_target) {
                let rate = self.rng.jitter(0.05);
                self.play(material.break_patch(), 0.8, pan, rate);
            }
        }
        for material in [Material::Soft, Material::Hard] {
            let patch = material.loop_patch();
            let params = match current {
                Some((drill, active)) if active.loop_patch() == patch => {
                    let speed = (0.15 / drill.seconds_per_chip.max(0.02)).clamp(0.3, 3.0);
                    (0.9, pan, speed.powf(0.18).clamp(0.82, 1.25), OPEN_CUTOFF)
                }
                _ => (0.0, pan, 1.0, OPEN_CUTOFF),
            };
            self.loop_command(live, listener, SeatLoop::Drill, patch, params);
        }
        if let Some((drill, material)) = current {
            state.crunch_timer -= dt;
            if state.crunch_timer <= 0.0 {
                let chip = drill.seconds_per_chip.max(0.03);
                match material {
                    Material::Soft => {
                        // Dull clods.
                        let gain = self.rng.range(0.25, 0.4);
                        let rate = self.rng.range(0.5, 0.65);
                        self.play(Patch::DrillCrunch, gain, pan, rate);
                        state.crunch_timer = chip * self.rng.range(3.0, 6.0);
                    }
                    Material::Hard | Material::Ore => {
                        let gain = self.rng.range(0.4, 0.8);
                        let rate = self.rng.range(0.9, 1.15);
                        self.play(Patch::DrillCrunch, gain, pan, rate);
                        state.crunch_timer = chip * self.rng.range(1.2, 3.0);
                    }
                }
            }
            if material == Material::Ore {
                state.glint_timer -= dt;
                if state.glint_timer <= 0.0 {
                    let gain = self.rng.range(0.35, 0.6);
                    self.play(Patch::DrillGlint, gain, pan, 1.0);
                    state.glint_timer = self.rng.range(0.25, 0.6);
                }
            }
        } else {
            state.crunch_timer = 0.0;
            state.glint_timer = 0.0;
        }
        state.drill = current.map(|(drill, material)| (drill.target, material));

        // Landings and wall bumps, scaled by impact.
        if state.initialized && playing {
            if state.airborne && !airborne && state.velocity_y > LANDING_SPEED {
                let impact = state.velocity_y;
                let patch = if impact < 200.0 {
                    Patch::LandLight
                } else if impact < HARD_LANDING_SPEED {
                    Patch::LandMedium
                } else {
                    Patch::LandHeavy
                };
                let gain = (impact / HARD_LANDING_SPEED).clamp(0.4, 1.0);
                self.play(patch, gain, pan, 1.0);
            }
            let lost = state.velocity_x.abs() - velocity_x.abs();
            if drill.is_none()
                && state.velocity_x.abs() > BUMP_SPEED
                && velocity_x.abs() < state.velocity_x.abs() * 0.2
                && lost > BUMP_SPEED
            {
                let gain = (state.velocity_x.abs() / 250.0).clamp(0.3, 0.9);
                self.play(Patch::WallBump, gain, pan, 1.0);
            }
        }

        // Alarms escalate: shorter gaps as the reserve drops, then a harsher critical patch.
        // The low warnings stay sparse (a quarter tank can last minutes of play).
        if playing {
            if fuel_fraction < 0.25 {
                let critical = fuel_fraction < 0.1;
                if critical {
                    state.fuel_alarm = state.fuel_alarm.min(CRITICAL_ALARM_GAP.0);
                }
                state.fuel_alarm -= dt;
                if state.fuel_alarm <= 0.0 {
                    let patch = if critical {
                        Patch::FuelCritical
                    } else {
                        Patch::FuelLow
                    };
                    self.play(patch, 0.8, pan, 1.0);
                    state.fuel_alarm = alarm_gap(1.0 - fuel_fraction / 0.25, critical);
                    state.hull_alarm = state.hull_alarm.max(0.6);
                }
            } else {
                state.fuel_alarm = 0.0;
            }
            if hull_fraction < 0.3 {
                let critical = hull_fraction < 0.12;
                if critical {
                    state.hull_alarm = state.hull_alarm.min(CRITICAL_ALARM_GAP.0);
                }
                state.hull_alarm -= dt;
                if state.hull_alarm <= 0.0 {
                    let patch = if critical {
                        Patch::HullCritical
                    } else {
                        Patch::HullLow
                    };
                    self.play(patch, 0.8, pan, 1.0);
                    state.hull_alarm = alarm_gap(1.0 - hull_fraction / 0.3, critical);
                    state.fuel_alarm = state.fuel_alarm.max(0.6);
                }
            } else {
                state.hull_alarm = 0.0;
            }
        }

        // Lava nearby sizzles.
        let lava = nearest_lava(terrain, listener.x, listener.y);
        let lava_gain = lava.map_or(0.0, |distance| {
            (1.0 - distance / 3.5).clamp(0.0, 1.0).powi(2)
        });
        self.loop_command(
            live,
            listener,
            SeatLoop::Lava,
            Patch::LavaSizzleLoop,
            (lava_gain * 0.8, pan, 1.0, OPEN_CUTOFF),
        );

        // The nearest gas cloud hisses.
        let gas = snapshot
            .hazards
            .iter()
            .map(|cloud| {
                let dx = cloud.x - listener.x;
                let distance = (dx.hypot(cloud.y - listener.y) - cloud.radius).max(0.0);
                let fade = cloud.life.clamp(0.0, 1.0);
                (attenuation(distance) * fade, dx)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let (gas_gain, gas_dx) = gas.unwrap_or((0.0, 0.0));
        self.loop_command(
            live,
            listener,
            SeatLoop::Gas,
            Patch::GasHissLoop,
            (gas_gain * 0.7, side_pan(listener, gas_dx), 1.0, OPEN_CUTOFF),
        );

        // Loose and falling boulders rumble.
        let mut rumble = 0.0_f32;
        let mut rumble_dx = 0.0;
        for boulder in snapshot.boulders {
            let dx = boulder.x - listener.x;
            let weight = if boulder.warning_seconds > 0.0 {
                0.45
            } else {
                1.0
            };
            let level = attenuation(dx.hypot(boulder.y - listener.y)) * weight;
            if level > rumble {
                rumble_dx = dx;
            }
            rumble = rumble.max(level);
        }
        self.loop_command(
            live,
            listener,
            SeatLoop::Boulders,
            Patch::BoulderRumbleLoop,
            (
                rumble * 0.9,
                side_pan(listener, rumble_dx),
                0.9 + 0.2 * rumble,
                600.0 + 12_000.0 * rumble,
            ),
        );

        state.initialized = true;
        state.velocity_x = velocity_x;
        state.velocity_y = velocity_y;
        state.airborne = airborne;
        *self.seat_state(listener.seat) = state;
    }

    /// Bomb fuses tick faster as they burn down.
    fn fuse_ticks(&mut self, frame: &AudioFrame<'_, '_>, listeners: &[Listener]) {
        let mut seen = BTreeSet::new();
        for bomb in frame.snapshot.bombs {
            let key = (bomb.x.round() as i32, bomb.y.round() as i32);
            seen.insert(key);
            let timer = self.fuses.entry(key).or_insert(0.0);
            *timer -= frame.delta_seconds;
            if *timer <= 0.0 {
                *timer = (bomb.timer_seconds * 0.22).clamp(0.08, 0.5);
                let rate = 1.0 + (1.0 - bomb.timer_seconds.clamp(0.0, 3.0) / 3.0) * 0.2;
                if let Some((gain, pan, cutoff)) = spatial(bomb.x, bomb.y, listeners) {
                    self.commands.push(MixerCommand::Play(PlayRequest {
                        patch: Patch::FuseTick,
                        gain: gain * 0.7,
                        pan,
                        rate,
                        cutoff,
                    }));
                }
            }
        }
        self.fuses.retain(|key, _| seen.contains(key));
    }
}

/// Seconds between low-reserve warnings: (at the threshold, at the critical line).
const LOW_ALARM_GAP: (f32, f32) = (8.0, 4.0);
/// Seconds between critical warnings: (at the critical line, empty).
const CRITICAL_ALARM_GAP: (f32, f32) = (2.0, 1.0);
/// Urgency (0 at the warning threshold, 1 empty) where the critical alarm takes over; both
/// alarms put their critical line at 40% of their warning threshold.
const CRITICAL_URGENCY: f32 = 0.6;

/// Gap before the next alarm at `urgency` (0 at the warning threshold, 1 empty).
fn alarm_gap(urgency: f32, critical: bool) -> f32 {
    let lerp = |(from, to): (f32, f32), x: f32| from + (to - from) * x.clamp(0.0, 1.0);
    if critical {
        lerp(
            CRITICAL_ALARM_GAP,
            (urgency - CRITICAL_URGENCY) / (1.0 - CRITICAL_URGENCY),
        )
    } else {
        lerp(LOW_ALARM_GAP, urgency / CRITICAL_URGENCY)
    }
}

/// Every local seat as a listener.
fn listeners(snapshot: &RenderSnapshot<'_>) -> Vec<Listener> {
    let screen_width = snapshot
        .views
        .iter()
        .map(|view| view.viewport.x + view.viewport.width)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let split = snapshot.views.len() > 1;
    snapshot
        .views
        .iter()
        .map(|view| {
            let (x, y) = snapshot
                .players
                .iter()
                .find(|player| player.player_id == view.player_id)
                .map_or((view.player.x, view.player.y), |player| {
                    (player.x, player.y)
                });
            let centre = view.viewport.x as f32 + view.viewport.width as f32 * 0.5;
            let pan = if split {
                ((centre / screen_width) * 2.0 - 1.0) * SEAT_PAN_SPREAD
            } else {
                0.0
            };
            Listener {
                seat: view.seat_index,
                player_id: view.player_id,
                x,
                y,
                pan,
                half_width: (view.visible_world_size().0 * 0.5).max(TILE_SIZE),
            }
        })
        .collect()
}

/// Level of a world sound `distance` world pixels from the listener.
fn attenuation(distance: f32) -> f32 {
    if distance <= FULL_LEVEL_DISTANCE {
        1.0
    } else if distance >= MAX_DISTANCE {
        0.0
    } else {
        let beyond = (distance - FULL_LEVEL_DISTANCE) / (TILE_SIZE * 10.0);
        let fade = 1.0 - (distance - FULL_LEVEL_DISTANCE) / (MAX_DISTANCE - FULL_LEVEL_DISTANCE);
        fade / (1.0 + beyond * beyond)
    }
}

/// Pan of a world sound `dx` world pixels to the side of `listener`, kept on the listener's side
/// of a split screen.
fn side_pan(listener: &Listener, dx: f32) -> f32 {
    let room = 1.0 - listener.pan.abs() * 0.5;
    (listener.pan + (dx / listener.half_width).clamp(-1.0, 1.0) * room * 0.8).clamp(-1.0, 1.0)
}

/// Gain, pan and low-pass cutoff of a world sound at `(x, y)`, heard by the nearest listener.
fn spatial(x: f32, y: f32, listeners: &[Listener]) -> Option<(f32, f32, f32)> {
    let (listener, distance) = listeners
        .iter()
        .map(|listener| (listener, (x - listener.x).hypot(y - listener.y)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let gain = attenuation(distance);
    if gain < 0.02 {
        return None;
    }
    let far =
        ((distance - FULL_LEVEL_DISTANCE) / (MAX_DISTANCE - FULL_LEVEL_DISTANCE)).clamp(0.0, 1.0);
    let cutoff = 20_000.0 * (1.0 - far).powi(2) + 1_200.0 * far;
    Some((gain, side_pan(listener, x - listener.x), cutoff))
}

/// Distance in tiles from `(x, y)` to the nearest lava tile within three tiles.
fn nearest_lava(terrain: &drillgame_simulation::terrain::Terrain, x: f32, y: f32) -> Option<f32> {
    let centre_x = (x / TILE_SIZE).floor() as i32;
    let centre_y = (y / TILE_SIZE).floor() as i32;
    let mut nearest: Option<f32> = None;
    for dy in -3..=3 {
        for dx in -3..=3 {
            let position = TilePosition {
                x: centre_x + dx,
                y: centre_y + dy,
            };
            if terrain.is_lava_at(position) {
                let distance = (dx as f32).hypot(dy as f32);
                nearest = Some(nearest.map_or(distance, |best| best.min(distance)));
            }
        }
    }
    nearest
}

#[cfg(test)]
mod tests {
    use drillgame_netcode::SeatId;
    use drillgame_simulation::WorldState;

    use super::*;
    use crate::runtime::LocalSeat;

    fn seat(index: u32, player: PlayerId) -> LocalSeat {
        LocalSeat {
            seat: SeatId(index),
            player,
        }
    }

    fn plays(commands: &[MixerCommand]) -> Vec<PlayRequest> {
        commands
            .iter()
            .filter_map(|command| match command {
                MixerCommand::Play(request) => Some(*request),
                _ => None,
            })
            .collect()
    }

    fn frame<'a, 'w>(
        snapshot: &'a RenderSnapshot<'w>,
        cues: &'a [PlacedCue],
        seats: &'a [SeatAudio],
    ) -> AudioFrame<'a, 'w> {
        AudioFrame {
            snapshot,
            cues,
            seats,
            world_running: true,
            message_tick: 0,
            delta_seconds: 1.0 / 60.0,
        }
    }

    #[test]
    fn split_screen_pans_each_seats_sounds_toward_its_view() {
        let one = PlayerId::new(1);
        let two = PlayerId::new(2);
        let mut world = WorldState::new_game(one);
        world.add_player(two);
        let seats = [seat(0, one), seat(1, two)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let cues = [
            PlacedCue::player(SoundCue::Damage, one),
            PlacedCue::player(SoundCue::Damage, two),
            PlacedCue::interface(SoundCue::Ui),
        ];
        let audio = [SeatAudio::default(); 2];
        let played = plays(director.update(&frame(&snapshot, &cues, &audio)));
        assert_eq!(played.len(), 3);
        assert!(
            played[0].pan < -0.2,
            "seat one pans left: {}",
            played[0].pan
        );
        assert!(
            played[1].pan > 0.2,
            "seat two pans right: {}",
            played[1].pan
        );
        assert!(played[2].pan.abs() < 1e-6);
    }

    #[test]
    fn world_sounds_fade_and_pan_with_distance() {
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let rig = &snapshot.players[0];
        let mut director = Director::new(1);
        let cues = [
            PlacedCue::world(SoundCue::Explosion, rig.x + 10.0, rig.y),
            PlacedCue::world(SoundCue::Explosion, rig.x + TILE_SIZE * 20.0, rig.y),
            PlacedCue::world(SoundCue::Explosion, rig.x - TILE_SIZE * 20.0, rig.y),
            PlacedCue::world(SoundCue::Explosion, rig.x + TILE_SIZE * 80.0, rig.y),
        ];
        let played = plays(director.update(&frame(&snapshot, &cues, &[SeatAudio::default()])));
        // The one far out of earshot is dropped.
        assert_eq!(played.len(), 3);
        let (near, right, left) = (played[0], played[1], played[2]);
        assert!((near.gain - 1.0).abs() < 1e-6);
        assert!(right.gain < 0.5 && right.gain > 0.02);
        assert!(right.pan > 0.5 && left.pan < -0.5);
        assert!(right.cutoff < near.cutoff, "distant blasts are muffled");
    }

    #[test]
    fn attenuation_falls_off_monotonically() {
        let levels: Vec<f32> = (0..50)
            .map(|tiles| attenuation(tiles as f32 * TILE_SIZE))
            .collect();
        assert!(levels.windows(2).all(|pair| pair[1] <= pair[0]));
        assert!((levels[0] - 1.0).abs() < 1e-6);
        assert!(levels[45].abs() < 1e-6);
    }

    #[test]
    fn thrust_loop_follows_the_throttle_and_ignites_once() {
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let thrust_gain = |commands: &[MixerCommand]| {
            commands
                .iter()
                .find_map(|command| match command {
                    MixerCommand::Loop(request) if request.patch == Patch::ThrustLoop => {
                        Some(request.gain)
                    }
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        let held = [SeatAudio {
            thrust: true,
            playing: true,
            message_tick: 0,
        }];
        let mut ignitions = 0;
        let mut gains = Vec::new();
        for _ in 0..30 {
            let commands = director.update(&frame(&snapshot, &[], &held));
            ignitions += plays(commands)
                .iter()
                .filter(|request| request.patch == Patch::ThrustIgnite)
                .count();
            gains.push(thrust_gain(commands));
        }
        assert_eq!(ignitions, 1);
        assert!(gains.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!(gains[29] > 0.5);
        let released = [SeatAudio {
            playing: true,
            ..SeatAudio::default()
        }];
        for _ in 0..60 {
            let commands = director.update(&frame(&snapshot, &[], &released));
            gains.push(thrust_gain(commands));
        }
        assert!(gains.last().copied().unwrap_or(1.0) < 0.01);
    }

    #[test]
    fn pausing_stops_every_loop() {
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let held = [SeatAudio {
            thrust: true,
            playing: true,
            message_tick: 0,
        }];
        for _ in 0..10 {
            let _ = director.update(&frame(&snapshot, &[], &held));
        }
        let paused = AudioFrame {
            world_running: false,
            ..frame(&snapshot, &[], &held)
        };
        assert!(director.update(&paused).contains(&MixerCommand::StopLoops));
    }

    #[test]
    fn low_fuel_alarm_escalates() {
        let one = PlayerId::new(1);
        let mut world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let playing = [SeatAudio {
            playing: true,
            ..SeatAudio::default()
        }];
        let alarms = |world: &WorldState, director: &mut Director| {
            let snapshot = RenderSnapshot::build(world, &seats, (1280, 720), (&[], &[]), &[]);
            let mut found = Vec::new();
            // 20 s of play.
            for _ in 0..1_200 {
                found.extend(
                    plays(director.update(&frame(&snapshot, &[], &playing)))
                        .into_iter()
                        .filter(|request| {
                            matches!(request.patch, Patch::FuelLow | Patch::FuelCritical)
                        })
                        .map(|request| request.patch),
                );
            }
            found
        };
        let capacity = world
            .player(one)
            .map_or(100.0, |player| player.fuel_capacity);
        assert!(alarms(&world, &mut Director::new(1)).is_empty());
        if let Some(player) = world.player_mut(one) {
            player.fuel = capacity * 0.2;
        }
        let low = alarms(&world, &mut Director::new(1));
        // Sparse: one at once, then every ~6.7 s at a fifth of a tank.
        assert!(low.iter().all(|patch| *patch == Patch::FuelLow));
        assert!((3..=4).contains(&low.len()), "{} low alarms", low.len());
        if let Some(player) = world.player_mut(one) {
            player.fuel = capacity * 0.05;
        }
        let critical = alarms(&world, &mut Director::new(1));
        assert!(critical.iter().all(|patch| *patch == Patch::FuelCritical));
        assert!(
            critical.len() > low.len() * 3,
            "critical alarms come faster"
        );
    }

    #[test]
    fn alarm_gaps_shrink_as_the_reserve_drops() {
        assert!((alarm_gap(0.0, false) - LOW_ALARM_GAP.0).abs() < 1e-6);
        assert!((alarm_gap(CRITICAL_URGENCY, false) - LOW_ALARM_GAP.1).abs() < 1e-6);
        assert!((alarm_gap(CRITICAL_URGENCY, true) - CRITICAL_ALARM_GAP.0).abs() < 1e-6);
        assert!((alarm_gap(1.0, true) - CRITICAL_ALARM_GAP.1).abs() < 1e-6);
        assert!(alarm_gap(0.3, false) > alarm_gap(0.5, false));
        assert!(alarm_gap(0.7, true) > alarm_gap(0.9, true));
    }

    #[test]
    fn toasts_wait_for_other_sounds() {
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let audio = [SeatAudio::default()];
        let toast_count = |director: &mut Director, tick: u64, cues: &[PlacedCue]| {
            let frame = AudioFrame {
                message_tick: tick,
                ..frame(&snapshot, cues, &audio)
            };
            plays(director.update(&frame))
                .iter()
                .filter(|request| request.patch == Patch::Toast)
                .count()
        };
        let mut toasts = toast_count(&mut director, 5, &[]);
        for _ in 0..30 {
            toasts += toast_count(&mut director, 5, &[]);
        }
        assert_eq!(toasts, 1);
        // A message arriving with its own sound stays quiet.
        let coins = [PlacedCue::interface(SoundCue::Sell)];
        let mut toasts = toast_count(&mut director, 6, &[]);
        toasts += toast_count(&mut director, 6, &coins);
        for _ in 0..30 {
            toasts += toast_count(&mut director, 6, &[]);
        }
        assert_eq!(toasts, 0);
    }
}
