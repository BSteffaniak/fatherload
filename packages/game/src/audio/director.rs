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

use fatherload_simulation::{
    PlayerId, TILE_SIZE,
    terrain::{SurfaceMaterial, TileKind, TilePosition},
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
            | TileKind::Fluid(_)
            | TileKind::Dirt
            | TileKind::Clay
            | TileKind::Gas
            | TileKind::Lava
            | TileKind::PressurePocket
            | TileKind::Surface(
                SurfaceMaterial::Sand
                | SurfaceMaterial::Snow
                | SurfaceMaterial::Mud
                | SurfaceMaterial::Sandstone,
            ) => Self::Soft,
            TileKind::Stone
            | TileKind::Surface(SurfaceMaterial::Granite | SurfaceMaterial::RedRock)
            | TileKind::HardRock
            | TileKind::Foundation
            | TileKind::ExplosivePocket
            | TileKind::MagmaVent
            | TileKind::Poi(_)
            | TileKind::Bedrock
            | TileKind::Built(_) => Self::Hard,
            TileKind::Ore(_) | TileKind::Artifact(_) => Self::Ore,
            TileKind::Sky(tile) => match tile {
                fatherload_simulation::sky::SkyTile::Crystal
                | fatherload_simulation::sky::SkyTile::Cache
                | fatherload_simulation::sky::SkyTile::Keystone
                | fatherload_simulation::sky::SkyTile::AetherCrystal
                | fatherload_simulation::sky::SkyTile::StormGlass
                | fatherload_simulation::sky::SkyTile::HarpyEgg => Self::Ore,
                fatherload_simulation::sky::SkyTile::Turf(_)
                | fatherload_simulation::sky::SkyTile::Soil
                | fatherload_simulation::sky::SkyTile::Roots
                | fatherload_simulation::sky::SkyTile::Vines
                | fatherload_simulation::sky::SkyTile::Nest
                | fatherload_simulation::sky::SkyTile::Spring
                | fatherload_simulation::sky::SkyTile::Regolith
                | fatherload_simulation::sky::SkyTile::HarpyNest => Self::Soft,
                fatherload_simulation::sky::SkyTile::Rock(_)
                | fatherload_simulation::sky::SkyTile::ContinentRock
                | fatherload_simulation::sky::SkyTile::RuinBrick
                | fatherload_simulation::sky::SkyTile::RuinPillar
                | fatherload_simulation::sky::SkyTile::Vent
                | fatherload_simulation::sky::SkyTile::BuoyPad
                | fatherload_simulation::sky::SkyTile::MoonRock
                | fatherload_simulation::sky::SkyTile::AetherCore
                | fatherload_simulation::sky::SkyTile::TempleBrick
                | fatherload_simulation::sky::SkyTile::TempleDoor
                | fatherload_simulation::sky::SkyTile::KeystoneDoor
                | fatherload_simulation::sky::SkyTile::TempleLever
                | fatherload_simulation::sky::SkyTile::TempleKeystone
                | fatherload_simulation::sky::SkyTile::TempleCache => Self::Hard,
            },
            TileKind::Lunar(tile) => match tile {
                fatherload_simulation::moon::LunarTile::Regolith
                | fatherload_simulation::moon::LunarTile::MareDust
                | fatherload_simulation::moon::LunarTile::Helium3 => Self::Soft,
                fatherload_simulation::moon::LunarTile::Ice
                | fatherload_simulation::moon::LunarTile::AlloyVein
                | fatherload_simulation::moon::LunarTile::RelicCache
                | fatherload_simulation::moon::LunarTile::CrystalBloom
                | fatherload_simulation::moon::LunarTile::CrystalStalk => Self::Ore,
                _ => Self::Hard,
            },
            TileKind::Cave(tile) => match tile {
                fatherload_simulation::cave::CaveTile::Crystal
                | fatherload_simulation::cave::CaveTile::TreasureCache
                | fatherload_simulation::cave::CaveTile::RelicCache => Self::Ore,
                fatherload_simulation::cave::CaveTile::MushroomStem
                | fatherload_simulation::cave::CaveTile::MushroomCap
                | fatherload_simulation::cave::CaveTile::GlowCap
                | fatherload_simulation::cave::CaveTile::Mycelium
                | fatherload_simulation::cave::CaveTile::GlowAlgae
                | fatherload_simulation::cave::CaveTile::Silt => Self::Soft,
                _ => Self::Hard,
            },
            TileKind::Crust(tile) => match tile {
                fatherload_simulation::crust::CrustTile::RootMat
                | fatherload_simulation::crust::CrustTile::Timber
                | fatherload_simulation::crust::CrustTile::CabinLog
                | fatherload_simulation::crust::CrustTile::Rubble => Self::Soft,
                fatherload_simulation::crust::CrustTile::Strongbox
                | fatherload_simulation::crust::CrustTile::SinkholeCore => Self::Ore,
                _ => Self::Hard,
            },
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
    Wind,
    Rain,
    /// Night crickets on the surface (world clock).
    Night,
    /// The high sky's airy pad and wind chimes among the islands (WP-SKY).
    SkyAir,
    /// A waterfall pouring off a sky isle nearby.
    Waterfall,
    /// The uneasy drone and far howls while night monsters prowl near (WP-NIGHT).
    Prowl,
    /// A side-view tomb's run music while the seat explores one (WP-TOMBS T28).
    TombRun,
    /// The guardian's theme while a side-view tomb's guardian is awake.
    TombBoss,
    /// A crypt run's music (WP-TOMBS T29).
    Crypt,
    /// The crypt lord's theme while it fights.
    CryptBoss,
    /// The thin whistle of the air high up, taking over as the surface wind fades (Wave 6).
    HighWind,
    /// The Moon's (and orbit's) near-silence: the suit's hum and breath (Wave 6).
    Moon,
    /// A village's or sky town's bustle (Wave 6).
    Town,
    /// The lunar colony's air recyclers and consoles (Wave 6).
    Colony,
    /// Bubbles and the muffled pumps while the seat is under fluid (Part III F9).
    Underwater,
    /// The music bed of the Crust stratum the seat digs in (Wave 8).
    CrustBed,
    /// The Sinkhole chase while its collapse climbs (Wave 8).
    Sinkhole,
}

/// Loop key sources reserved per seat (one per [`SeatLoop`], with room to grow).
const SEAT_LOOP_SLOTS: u32 = 32;

/// Seconds for an environment's ambience to crossfade in or out (Wave 6), so walking into a
/// town, a tomb or the colony blends the beds instead of cutting them.
const AMBIENCE_FADE_SECONDS: f32 = 1.2;
/// Seconds between the (cheap but not free) village and colony lookups behind the ambience.
const AMBIENCE_LOOKUP_SECONDS: f32 = 0.25;
/// Seconds a clock theme waits before playing, so a music cue of the same moment (whose event
/// may land a frame or two apart from the clock's turn online) can claim the moment instead.
const CLOCK_THEME_DELAY_SECONDS: f32 = 0.6;
/// Seconds after another piece of music (the dawn relief, a skipped night, nightfall) in which
/// the clock themes stay quiet.
const CLOCK_THEME_HOLD_SECONDS: f32 = 4.0;
/// Seconds between a pilot's footfalls on the Moon while walking and while running.
const MOON_STEP_SECONDS: (f32, f32) = (0.56, 0.4);

/// How much of each environment a seat is in (0..1, smoothed; Wave 6). The outdoor beds (wind,
/// rain, crickets, the sky, the prowl) are scaled down by the others, so ambiences crossfade by
/// environment and never stack.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Ambience {
    /// In a tomb or crypt run (its music takes over).
    indoor: f32,
    /// In vacuum: on the Moon or in orbit.
    vacuum: f32,
    /// Inside the lunar colony's rooms.
    colony: f32,
    /// In a village, sky town or the home town.
    town: f32,
}

impl Ambience {
    /// Eases every share toward `target` over [`AMBIENCE_FADE_SECONDS`].
    fn approach(&mut self, target: Self, dt: f32) {
        let k = 1.0 - (-dt / (AMBIENCE_FADE_SECONDS / 3.0)).exp();
        self.indoor += (target.indoor - self.indoor) * k;
        self.vacuum += (target.vacuum - self.vacuum) * k;
        self.colony += (target.colony - self.colony) * k;
        self.town += (target.town - self.town) * k;
    }

    /// How much of the open-air beds (wind, rain, crickets, the sky) are heard.
    fn open_air(self) -> f32 {
        (1.0 - self.indoor) * (1.0 - self.vacuum) * (1.0 - self.colony)
    }
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
    /// The listener is in vacuum (orbit, the Moon): sound only reaches it through the hull or
    /// the suit, muffled (WP-MOON).
    vacuum: bool,
}

/// Low-pass cutoff of world sounds heard in vacuum (through the ground and the hull).
const VACUUM_WORLD_CUTOFF: f32 = 700.0;
/// Low-pass cutoff of a listener's own sounds in vacuum (felt through the frame).
const VACUUM_OWN_CUTOFF: f32 = 2_600.0;

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
    /// Smoothed environment shares (Wave 6).
    ambience: Ambience,
    /// The last looked-up town and colony shares and the time until the next lookup.
    ambience_lookup: (f32, f32, f32),
    /// Time until the next footfall of a pilot walking on the Moon.
    step_timer: f32,
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
    /// The world clock's phase last frame (dawn and dusk themes play on the change; Wave 6).
    clock_phase: Option<fatherload_simulation::clock::DayPhase>,
    /// A clock theme waiting to play: (seconds left, patch). Held back briefly so the music
    /// event cues of the same moment (the dawn relief after a night with monsters, a slept-through
    /// night) can claim it instead of two pieces stacking.
    pending_theme: Option<(f32, Patch)>,
    /// Seconds left in which another piece of music owns the moment and clock themes stay quiet.
    music_hold: f32,
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
            clock_phase: None,
            pending_theme: None,
            music_hold: 0.0,
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
            self.clock_themes(frame, &listeners);
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
            self.clock_phase = None;
            self.pending_theme = None;
            self.music_hold = 0.0;
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
                    let cutoff = if listener.vacuum {
                        VACUUM_OWN_CUTOFF
                    } else {
                        20_000.0
                    };
                    return Some((1.0, listener.pan, cutoff));
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
        let cutoff = if listener.vacuum {
            cutoff.min(VACUUM_OWN_CUTOFF)
        } else {
            cutoff
        };
        let key = LoopKey::new(listener.seat as u32 * SEAT_LOOP_SLOTS + slot as u32, patch);
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
        let foot = crate::view_frame::frame().tile_at(listener.x, listener.y + FOOT_OFFSET + 3.0);
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

        // Thrust: follows the held throttle, climbing opens it up. On foot (Part II F5) the
        // same loop is the jetpack's roar while it burns (its ignition is an event cue).
        let thrusting = view.pilot.as_ref().map_or_else(
            || seat_input.thrust && playing && fuel > 0.0,
            |pilot| pilot.state.jetting && playing,
        );
        if thrusting && !state.thrust_held && state.throttle < 0.25 && view.pilot.is_none() {
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
            // A giant mushroom cap springing the rig back up (fungal grottos): the fall flips
            // into a climb right over a cap.
            if state.velocity_y > LANDING_SPEED
                && velocity_y < -LANDING_SPEED * 2.0
                && [0, 1].iter().any(|below| {
                    terrain
                        .tile(TilePosition {
                            x: foot.x,
                            y: foot.y + below,
                        })
                        .is_some_and(|tile| {
                            tile.kind
                                == TileKind::Cave(
                                    fatherload_simulation::cave::CaveTile::MushroomCap,
                                )
                        })
                })
            {
                let gain = (state.velocity_y / HARD_LANDING_SPEED).clamp(0.45, 1.0);
                let rate = self.rng.range(0.92, 1.08);
                self.play(Patch::MushroomBounce, gain, pan, rate);
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

        // Under fluid: bubbles stream past and the pumps rumble (Part III F9).
        let underwater = terrain
            .fluid_at(crate::view_frame::frame().tile_at(listener.x, listener.y))
            .is_some_and(|cell| cell.level >= fatherload_simulation::fluid::FLUID_FULL / 2);
        self.loop_command(
            live,
            listener,
            SeatLoop::Underwater,
            Patch::BubbleLoop,
            (if underwater { 0.7 } else { 0.0 }, pan, 1.0, OPEN_CUTOFF),
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
        for boulder in &snapshot.boulders {
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

        state.ambience.approach(
            Self::ambience_target(&mut state.ambience_lookup, listener, snapshot, dt),
            dt,
        );
        let ambience = state.ambience;
        self.surface_ambience(live, listener, terrain, snapshot.clock, ambience);
        self.sky_ambience(live, listener, terrain, ambience);
        self.prowl_ambience(live, listener, &snapshot.enemies, ambience);
        self.place_ambience(live, listener, ambience);
        self.moon_steps(&mut state, listener, snapshot, dt);
        self.tomb_music(live, listener, snapshot.tombs);
        self.crypt_music(live, listener, snapshot.tombs);
        self.crust_music(live, listener, snapshot, ambience);

        state.initialized = true;
        state.velocity_x = velocity_x;
        state.velocity_y = velocity_y;
        state.airborne = airborne;
        *self.seat_state(listener.seat) = state;
    }

    /// Biome ambience out on the surface (WP-SURFACE): wind by biome that strengthens and
    /// rises in pitch with altitude (and thins out to silence in space), rain over wetlands,
    /// and crickets at night near the ground (world clock), loudest in grass and marsh.
    /// Underground all of them fade within a few rows.
    fn surface_ambience(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        terrain: &fatherload_simulation::terrain::Terrain,
        clock: fatherload_simulation::clock::WorldClock,
        ambience: Ambience,
    ) {
        use fatherload_simulation::{
            terrain::TILE_SIZE,
            worldgen::biome::{self, Biome},
        };
        let open_air = ambience.open_air();
        #[allow(clippy::cast_possible_truncation, reason = "world pixels to tiles")]
        let column = crate::view_frame::frame().col(listener.x);
        #[allow(clippy::cast_precision_loss, reason = "surface rows are small")]
        let ground = crate::view_frame::frame().rel_row(terrain.surface_row(column)) * TILE_SIZE;
        // Tiles above (positive) or below (negative) the column's ground.
        let height = (ground - listener.y) / TILE_SIZE;
        let open = ((height + 4.0) / 4.0).clamp(0.0, 1.0);
        let settings = terrain.settings();
        let biome = biome::biome_at(settings, column);
        let base = match biome {
            Biome::Tundra => 0.45,
            Biome::Mountains => 0.35,
            Biome::Desert | Biome::Badlands => 0.3,
            Biome::Wetlands => 0.12,
            Biome::Plains => 0.1,
            Biome::Forest => 0.06,
        };
        // Stronger wind higher up, peaking a couple of hundred tiles up, then thinning air.
        let climb = (height / 200.0).clamp(0.0, 1.0);
        let thin = (1.0 - (height - 300.0) / 400.0).clamp(0.0, 1.0);
        // A town's walls break the wind a little (Wave 6).
        let wind =
            (base + climb * 0.5).min(1.0) * open * thin * open_air * (1.0 - 0.4 * ambience.town);
        self.loop_command(
            live,
            listener,
            SeatLoop::Wind,
            Patch::WindLoop,
            (
                wind * 0.55,
                0.0,
                0.9 + climb * 0.35,
                1_800.0 + 10_000.0 * climb,
            ),
        );
        let rain = if biome == Biome::Wetlands && settings.generator.has_biomes() {
            open * (1.0 - height / 40.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.loop_command(
            live,
            listener,
            SeatLoop::Rain,
            Patch::RainLoop,
            (rain * 0.45 * open_air, 0.0, 1.0, OPEN_CUTOFF),
        );
        // The thin air high up (Wave 6): a whistle taking over as the surface wind thins out
        // (from ~200 tiles up), itself quieter and thinner the higher the seat climbs, gone by
        // the edge of space.
        let high = ((height - 150.0) / 150.0).clamp(0.0, 1.0)
            * (1.0 - (height - 350.0) / 450.0).clamp(0.0, 1.0);
        self.loop_command(
            live,
            listener,
            SeatLoop::HighWind,
            Patch::HighWindLoop,
            (
                high * 0.4 * open_air,
                0.0,
                1.0 + 0.15 * ((height - 350.0) / 450.0).clamp(0.0, 1.0),
                6_000.0 + 8_000.0 * (1.0 - ((height - 350.0) / 450.0).clamp(0.0, 1.0)),
            ),
        );
        let night = Self::night_share(clock);
        let chorus = match biome {
            Biome::Plains | Biome::Wetlands => 1.0,
            Biome::Forest => 0.85,
            Biome::Desert | Biome::Badlands => 0.5,
            Biome::Tundra | Biome::Mountains => 0.25,
        };
        let near_ground = (1.0 - height / 24.0).clamp(0.0, 1.0);
        self.loop_command(
            live,
            listener,
            SeatLoop::Night,
            Patch::NightLoop,
            (
                night * chorus * open * near_ground * 0.4 * open_air * (1.0 - 0.6 * ambience.town),
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
    }

    /// The sky isles' ambience (WP-SKY, V9): the music shifts up high into an airy pad with
    /// wind chimes that swells in among the islands and thins out toward the dead-air gap, and
    /// the roar of the nearest waterfall, panned to its side.
    fn sky_ambience(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        terrain: &fatherload_simulation::terrain::Terrain,
        ambience: Ambience,
    ) {
        use fatherload_simulation::{
            sky::SkyTile,
            terrain::{TILE_SIZE, TileKind},
            worldgen::{TileRect, sky},
        };
        let settings = terrain.settings();
        let altitude = -crate::view_frame::frame().legacy_y(listener.y) / TILE_SIZE;
        let bands = settings.generator.sky_bands().unwrap_or(sky::V8_SKY_BANDS);
        #[allow(clippy::cast_precision_loss, reason = "band rows are small integers")]
        let (low, gap) = (bands.low_isles as f32, bands.dead_air as f32);
        let share = if settings.generator.has_sky_isles() {
            ((altitude - (low - 80.0)) / 120.0).clamp(0.0, 1.0)
                * (1.0 - (altitude - gap) / 300.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.loop_command(
            live,
            listener,
            SeatLoop::SkyAir,
            Patch::SkyAirLoop,
            (share * 0.5 * ambience.open_air(), 0.0, 1.0, OPEN_CUTOFF),
        );
        let mut roar = (0.0_f32, 0.0_f32);
        if share > 0.0 {
            #[allow(clippy::cast_possible_truncation, reason = "world pixels to tiles")]
            let here = crate::view_frame::frame().tile_at(listener.x, listener.y);
            for land in sky::lands_touching(settings, &TileRect::around(here, 40, 70)) {
                let Some(spring) = land.content().and_then(|content| content.spring) else {
                    continue;
                };
                if terrain
                    .tile(spring)
                    .is_none_or(|tile| tile.kind != TileKind::Sky(SkyTile::Spring))
                {
                    continue;
                }
                // The water falls below the spring: hear it from the nearest point of the fall.
                let fall_y = here.y.clamp(spring.y, spring.y + 50);
                #[allow(clippy::cast_precision_loss, reason = "tile distances")]
                let (dx, dy) = ((spring.x - here.x) as f32, (fall_y - here.y) as f32);
                let distance = dx.hypot(dy);
                let gain = (1.0 - distance / 36.0).clamp(0.0, 1.0);
                if gain > roar.0 {
                    roar = (gain, (dx / 24.0).clamp(-0.8, 0.8));
                }
            }
        }
        self.loop_command(
            live,
            listener,
            SeatLoop::Waterfall,
            Patch::WaterfallLoop,
            (
                roar.0 * roar.0 * 0.55 * ambience.open_air(),
                roar.1,
                1.0,
                OPEN_CUTOFF,
            ),
        );
    }

    /// The night's menace (WP-NIGHT): a low drone with far howls that swells as night monsters
    /// gather near the seat's rig (silent with none about, so Peaceful worlds and towns stay
    /// calm).
    fn prowl_ambience(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        enemies: &[fatherload_simulation::entity::Entity],
        ambience: Ambience,
    ) {
        use fatherload_simulation::{entity::EntityState, night, terrain::TILE_SIZE};
        let reach = 40.0 * TILE_SIZE;
        let near = enemies
            .iter()
            .filter(|entity| {
                matches!(&entity.state, EntityState::Enemy(state) if night::is_nocturnal(state.kind))
                    && (entity.body.x - listener.x).abs() < reach
                    && (entity.body.y - listener.y).abs() < reach
            })
            .count();
        #[allow(clippy::cast_precision_loss, reason = "a handful of monsters")]
        let threat = (near as f32 / 3.0).min(1.0);
        self.loop_command(
            live,
            listener,
            SeatLoop::Prowl,
            Patch::NightProwlLoop,
            (
                threat * 0.45 * (1.0 - ambience.indoor) * (1.0 - ambience.vacuum),
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
    }

    /// Where a seat is, as environment shares to crossfade toward (Wave 6): a tomb or crypt run,
    /// vacuum, the lunar colony's rooms, a village, sky town or the home town. The village and
    /// colony lookups refresh a few times a second.
    fn ambience_target(
        lookup: &mut (f32, f32, f32),
        listener: &Listener,
        snapshot: &RenderSnapshot<'_>,
        dt: f32,
    ) -> Ambience {
        use fatherload_simulation::{moon, surface, village, worldgen};
        let settings = snapshot.terrain.settings();
        lookup.2 -= dt;
        if lookup.2 <= 0.0 {
            lookup.2 = AMBIENCE_LOOKUP_SECONDS;
            let tile = crate::view_frame::frame().tile_at(listener.x, listener.y);
            let (first, last) = surface::town_span();
            let surface = snapshot.terrain.surface_row(tile.x);
            let home_town =
                (first..=last).contains(&tile.x) && (surface - 14..=surface + 2).contains(&tile.y);
            // Villages on the surface and sky towns above it; an underground village's hall is
            // a cave (no birds or carts down there), so it keeps the cave's beds.
            let town = home_town
                || (tile.y <= surface + 2 && village::village_at(settings, tile).is_some());
            let colony =
                moon::sheltered_in(settings, crate::view_frame::frame(), listener.x, listener.y)
                    && worldgen::moon::layout(settings)
                        .is_some_and(|layout| layout.colony.covers(tile.x));
            lookup.0 = if town { 1.0 } else { 0.0 };
            lookup.1 = if colony { 1.0 } else { 0.0 };
        }
        Ambience {
            indoor: if snapshot.tombs.run_of(listener.player_id).is_some() {
                1.0
            } else {
                0.0
            },
            vacuum: if listener.vacuum { 1.0 } else { 0.0 },
            colony: lookup.1,
            town: lookup.0,
        }
    }

    /// The beds that belong to a place (Wave 6): the Moon's near-silence in vacuum, the colony's
    /// machinery in its rooms, and a town's bustle in villages, sky towns and the home town.
    fn place_ambience(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        ambience: Ambience,
    ) {
        let outside = 1.0 - ambience.indoor;
        self.loop_command(
            live,
            listener,
            SeatLoop::Moon,
            Patch::MoonLoop,
            (
                ambience.vacuum * (1.0 - ambience.colony) * outside * 0.22,
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
        self.loop_command(
            live,
            listener,
            SeatLoop::Colony,
            Patch::ColonyLoop,
            (ambience.colony * outside * 0.32, 0.0, 1.0, OPEN_CUTOFF),
        );
        self.loop_command(
            live,
            listener,
            SeatLoop::Town,
            Patch::TownLoop,
            (
                ambience.town * outside * (1.0 - ambience.vacuum) * 0.3,
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
    }

    /// Soft footfalls of a pilot walking or running on the Moon (Wave 6), felt through the suit.
    fn moon_steps(
        &mut self,
        state: &mut SeatState,
        listener: &Listener,
        snapshot: &RenderSnapshot<'_>,
        dt: f32,
    ) {
        use fatherload_simulation::pilot::PilotPose;
        let striding = snapshot
            .pilots
            .iter()
            .find(|pilot| pilot.player_id == listener.player_id)
            .filter(|pilot| pilot.low_gravity && pilot.state.grounded)
            .and_then(|pilot| match pilot.pose {
                PilotPose::Walk => Some(MOON_STEP_SECONDS.0),
                PilotPose::Run => Some(MOON_STEP_SECONDS.1),
                _ => None,
            });
        let Some(gap) = striding else {
            state.step_timer = 0.0;
            return;
        };
        state.step_timer -= dt;
        if state.step_timer <= 0.0 {
            state.step_timer = gap;
            let rate = self.rng.jitter(0.05);
            self.commands.push(MixerCommand::Play(PlayRequest {
                patch: Patch::MoonStep,
                gain: 0.5,
                pan: listener.pan,
                rate,
                cutoff: if listener.vacuum {
                    VACUUM_OWN_CUTOFF
                } else {
                    OPEN_CUTOFF
                },
            }));
        }
    }

    /// The world clock's dawn and dusk themes (Wave 6): a short piece of music when the day
    /// turns, heard by seats out in the open (not in a tomb or out in vacuum). The dawn after a
    /// night with monsters already has its own music (the relief), as does a night slept
    /// through: those take the moment and the theme stays quiet rather than stacking on them.
    fn clock_themes(&mut self, frame: &AudioFrame<'_, '_>, listeners: &[Listener]) {
        use fatherload_simulation::clock::DayPhase;
        let snapshot = frame.snapshot;
        let dt = frame.delta_seconds.max(0.0);
        if frame.cues.iter().any(|cue| {
            matches!(
                cue.cue,
                SoundCue::DawnRelief | SoundCue::NightSkipped | SoundCue::NightFalls
            )
        }) {
            self.music_hold = CLOCK_THEME_HOLD_SECONDS;
            self.pending_theme = None;
        }
        self.music_hold = (self.music_hold - dt).max(0.0);
        let phase = snapshot.clock.phase();
        let previous = self.clock_phase.replace(phase);
        if let Some(previous) = previous
            && previous != phase
        {
            self.pending_theme = match phase {
                DayPhase::Dawn => Some((CLOCK_THEME_DELAY_SECONDS, Patch::DawnTheme)),
                DayPhase::Dusk => Some((CLOCK_THEME_DELAY_SECONDS, Patch::DuskTheme)),
                DayPhase::Day | DayPhase::Night => None,
            };
        }
        if self.music_hold > 0.0 {
            self.pending_theme = None;
        }
        let Some((left, patch)) = self.pending_theme else {
            return;
        };
        if left > dt {
            self.pending_theme = Some((left - dt, patch));
            return;
        }
        self.pending_theme = None;
        let outdoors = listeners.iter().any(|listener| {
            !listener.vacuum && snapshot.tombs.run_of(listener.player_id).is_none()
        });
        if outdoors {
            self.play(patch, 0.55, 0.0, 1.0);
        }
    }

    /// The authored Crust's beds (Wave 8, Deep Descent worlds): underground, each Crust stratum
    /// plays its own piece (the Topsoil's folk line, the Clay Beds' marimba, the works' song,
    /// the shelves' glass, the basalt's drone); while the Sinkhole's collapse climbs, the
    /// chase takes over for every seat near it. Quiet in tomb runs.
    fn crust_music(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        snapshot: &RenderSnapshot<'_>,
        ambience: Ambience,
    ) {
        use fatherload_simulation::{crust, descent::Stratum};
        let settings = snapshot.terrain.settings();
        if !crust::crust_rules(settings) {
            return;
        }
        let tile = crate::view_frame::frame().tile_at(listener.x, listener.y);
        let quiet = 1.0 - ambience.indoor;
        let chase = snapshot.crust.sinkhole.is_some()
            && crust::sinkhole_site(settings)
                .is_some_and(|site| site.zone.expanded(40).contains(tile));
        self.loop_command(
            live,
            listener,
            SeatLoop::Sinkhole,
            Patch::SinkholeLoop,
            (
                if chase { 0.75 * quiet } else { 0.0 },
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
        let underground = tile.y > snapshot.terrain.surface_row(tile.x) + 3;
        let bed = match snapshot.terrain.stratum_at(tile) {
            Stratum::Topsoil => Some(Patch::CrustTopsoilLoop),
            Stratum::ClayBeds => Some(Patch::CrustClayLoop),
            Stratum::GreyStone => Some(Patch::CrustWorksLoop),
            Stratum::SlateShelves => Some(Patch::CrustSlateLoop),
            Stratum::BasaltColumns => Some(Patch::CrustBasaltLoop),
            _ => None,
        };
        if let Some(patch) = bed {
            let gain = if underground && !chase {
                0.45 * quiet
            } else {
                0.0
            };
            self.loop_command(
                live,
                listener,
                SeatLoop::CrustBed,
                patch,
                (gain, 0.0, 1.0, OPEN_CUTOFF),
            );
        }
    }

    /// A side-view tomb's music (WP-TOMBS T28): the run loop while the seat's player explores
    /// one, the guardian's theme instead while its guardian is awake. Crypts (T29) bring their
    /// own.
    fn tomb_music(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        tombs: &fatherload_simulation::tomb::Tombs,
    ) {
        use fatherload_simulation::tomb::{RunPhase, SiteStyle};
        let run = tombs
            .run_of(listener.player_id)
            .filter(|run| run.site.style == SiteStyle::SideView);
        let boss = run.is_some_and(|run| run.boss.is_some() && !run.boss_defeated);
        let exploring = run.is_some_and(|run| run.phase == RunPhase::Exploring);
        self.loop_command(
            live,
            listener,
            SeatLoop::TombRun,
            Patch::TombRunLoop,
            (
                if exploring && !boss { 0.42 } else { 0.0 },
                0.0,
                1.0,
                OPEN_CUTOFF,
            ),
        );
        self.loop_command(
            live,
            listener,
            SeatLoop::TombBoss,
            Patch::TombBossLoop,
            (if boss { 0.5 } else { 0.0 }, 0.0, 1.0, OPEN_CUTOFF),
        );
    }

    /// A crypt run's music (WP-TOMBS T29): the crypt's drone and bells while the seat's player
    /// is in a top-down crypt, the crypt lord's war drums instead while it fights.
    fn crypt_music(
        &mut self,
        live: &mut BTreeSet<LoopKey>,
        listener: &Listener,
        tombs: &fatherload_simulation::tomb::Tombs,
    ) {
        use fatherload_simulation::tomb::SiteStyle;
        let run = tombs
            .run_of(listener.player_id)
            .filter(|run| run.site.style == SiteStyle::TopDown);
        let boss = run.is_some_and(|run| run.boss.is_some() && !run.boss_defeated);
        let (crypt, lord) = match (run.is_some(), boss) {
            (true, true) => (0.0, 0.55),
            (true, false) => (0.42, 0.0),
            _ => (0.0, 0.0),
        };
        self.loop_command(
            live,
            listener,
            SeatLoop::Crypt,
            Patch::CryptLoop,
            (crypt, 0.0, 1.0, OPEN_CUTOFF),
        );
        self.loop_command(
            live,
            listener,
            SeatLoop::CryptBoss,
            Patch::CryptBossLoop,
            (lord, 0.0, 1.0, OPEN_CUTOFF),
        );
    }

    /// How much of the night chorus is out (0 by day, 1 at night, easing through dusk and dawn).
    fn night_share(clock: fatherload_simulation::clock::WorldClock) -> f32 {
        use fatherload_simulation::clock::DayPhase;
        #[allow(clippy::cast_precision_loss, reason = "permille")]
        let progress = clock.phase_progress() as f32 / 1_000.0;
        match clock.phase() {
            DayPhase::Night => 1.0,
            DayPhase::Day => 0.0,
            DayPhase::Dusk => progress,
            DayPhase::Dawn => 1.0 - progress,
        }
    }

    /// Bomb fuses tick faster as they burn down (C4 only once its detonator has fired).
    fn fuse_ticks(&mut self, frame: &AudioFrame<'_, '_>, listeners: &[Listener]) {
        let mut seen = BTreeSet::new();
        for bomb in &frame.snapshot.bombs {
            // Remote charges waiting for the detonator and burning flares or fires have no fuse.
            if !bomb.kind.has_audible_fuse() || bomb.awaiting_remote() || bomb.burning() {
                continue;
            }
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
            // On foot the seat hears from its pilot (Part II F5).
            let (x, y) = view.pilot.as_ref().map_or_else(
                || {
                    snapshot
                        .players
                        .iter()
                        .find(|player| player.player_id == view.player_id)
                        .map_or((view.player.x, view.player.y), |player| {
                            (player.x, player.y)
                        })
                },
                crate::render_snapshot::PilotPresentation::middle,
            );
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
                vacuum: fatherload_simulation::moon::vacuum_in(
                    snapshot.terrain.settings(),
                    crate::view_frame::frame(),
                    x,
                    y,
                ),
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
    if listener.vacuum {
        // No air to carry it: a dull thud through the ground (WP-MOON).
        return Some((
            gain * 0.6,
            side_pan(listener, x - listener.x),
            cutoff.min(VACUUM_WORLD_CUTOFF),
        ));
    }
    Some((gain, side_pan(listener, x - listener.x), cutoff))
}

/// Distance in tiles from `(x, y)` to the nearest lava tile within three tiles.
fn nearest_lava(terrain: &fatherload_simulation::terrain::Terrain, x: f32, y: f32) -> Option<f32> {
    let centre_x = crate::view_frame::frame().col(x);
    let centre_y = crate::view_frame::frame().row(y);
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
    use fatherload_netcode::SeatId;
    use fatherload_simulation::WorldState;

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
    fn a_mushroom_cap_bounce_boings() {
        let one = PlayerId::new(1);
        let mut world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let (x, y) = {
            let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
            (snapshot.players[0].x, snapshot.players[0].y)
        };
        let foot = crate::view_frame::frame().tile_at(x, y + FOOT_OFFSET + 3.0);
        let cap = TileKind::Cave(fatherload_simulation::cave::CaveTile::MushroomCap);
        assert!(world.terrain_mut().set_kind(foot, cap));
        let mut director = Director::new(1);
        let audio = [SeatAudio {
            playing: true,
            ..SeatAudio::default()
        }];
        let mut boings = 0;
        for velocity_y in [320.0, -300.0, -280.0] {
            let mut snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
            snapshot.players[0].velocity_y = velocity_y;
            boings += plays(director.update(&frame(&snapshot, &[], &audio)))
                .iter()
                .filter(|request| request.patch == Patch::MushroomBounce)
                .count();
        }
        assert_eq!(boings, 1, "one boing per bounce");
    }

    /// The first clock (scanning one standard day) whose phase is `phase`.
    fn clock_in(
        phase: fatherload_simulation::clock::DayPhase,
    ) -> fatherload_simulation::clock::WorldClock {
        use fatherload_simulation::clock::{DayLength, WorldClock};
        (0..2_880)
            .map(|step| WorldClock::from_world_millis(step * 1_000, DayLength::Standard, 7))
            .find(|clock| clock.phase() == phase)
            .expect("the phase comes round in a day")
    }

    /// The clock themes `director` plays while the snapshot's clock walks through `phases`,
    /// one second of frames per phase, with `cues` arriving on the first frame of each phase.
    fn clock_themes_through(
        director: &mut Director,
        snapshot: &mut RenderSnapshot<'_>,
        phases: &[(fatherload_simulation::clock::DayPhase, &[PlacedCue])],
    ) -> Vec<Patch> {
        let audio = [SeatAudio::default()];
        let mut themes = Vec::new();
        for &(phase, cues) in phases {
            snapshot.clock = clock_in(phase);
            for step in 0..60 {
                let cues = if step == 0 { cues } else { &[] };
                themes.extend(
                    plays(director.update(&frame(snapshot, cues, &audio)))
                        .into_iter()
                        .map(|request| request.patch)
                        .filter(|patch| matches!(patch, Patch::DawnTheme | Patch::DuskTheme)),
                );
            }
        }
        themes
    }

    #[test]
    fn dusk_and_dawn_play_their_themes_once_when_the_day_turns() {
        use fatherload_simulation::clock::DayPhase;
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let mut snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let themes = clock_themes_through(
            &mut director,
            &mut snapshot,
            &[
                (DayPhase::Day, &[]),
                (DayPhase::Day, &[]),
                (DayPhase::Dusk, &[]),
                (DayPhase::Dusk, &[]),
                (DayPhase::Night, &[]),
                (DayPhase::Dawn, &[]),
            ],
        );
        assert_eq!(themes, vec![Patch::DuskTheme, Patch::DawnTheme]);
    }

    #[test]
    fn the_dawn_theme_yields_to_the_relief_after_a_night_with_monsters() {
        use fatherload_simulation::clock::DayPhase;
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let mut snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let relief = [PlacedCue::interface(SoundCue::DawnRelief)];
        let skipped = [PlacedCue::interface(SoundCue::NightSkipped)];
        let mut director = Director::new(1);
        let themes = clock_themes_through(
            &mut director,
            &mut snapshot,
            &[
                (DayPhase::Night, &[]),
                (DayPhase::Dawn, &relief),
                (DayPhase::Day, &[]),
                (DayPhase::Night, &[]),
                (DayPhase::Dawn, &skipped),
            ],
        );
        assert!(
            themes.is_empty(),
            "stacked on the night's own music: {themes:?}"
        );
    }

    #[test]
    fn the_home_town_bustle_fades_in_and_the_open_air_beds_give_way_indoors() {
        let one = PlayerId::new(1);
        let world = WorldState::new_game(one);
        let seats = [seat(0, one)];
        let snapshot = RenderSnapshot::build(&world, &seats, (1280, 720), (&[], &[]), &[]);
        let mut director = Director::new(1);
        let audio = [SeatAudio::default()];
        let town_gain = |commands: &[MixerCommand]| {
            commands.iter().find_map(|command| match command {
                MixerCommand::Loop(request) if request.patch == Patch::TownLoop => {
                    Some(request.gain)
                }
                _ => None,
            })
        };
        let first = town_gain(director.update(&frame(&snapshot, &[], &audio))).unwrap_or(0.0);
        let mut later = first;
        for _ in 0..180 {
            later = town_gain(director.update(&frame(&snapshot, &[], &audio))).unwrap_or(0.0);
        }
        assert!(first < 0.05, "crossfades in, not cut in: {first}");
        assert!(later > 0.2, "the new game starts in the home town: {later}");
        let tomb = Ambience {
            indoor: 1.0,
            ..Ambience::default()
        };
        assert!(
            tomb.open_air() < 1e-6,
            "a tomb's music replaces the wind and crickets"
        );
        assert!((Ambience::default().open_air() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_seat_loop_slot_has_its_own_key_range() {
        assert!((SeatLoop::Colony as u32) < SEAT_LOOP_SLOTS);
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
