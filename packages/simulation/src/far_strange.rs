//! The far Strange Layers (expansion plan §14, Descent VI strata 4-6 and its gate; Wave 9
//! track strange-b): time, sound and light come apart near the builders' machine.
//!
//! Pure data and math; the tick lives in `world/far_strange.rs`, the creatures' behaviour and
//! The Glitch's fight in `world/enemies/far_strange.rs`, the generated places in
//! [`crate::worldgen::descent::far_strange`].
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | Time Sludge | **slow pools** and **fast lanes** (the Wave 7 time-scale field, generated pockets that grow commoner and stronger deeper; [`time_pocket_scale`]); creatures that live at their own speed ([`creature_tempo`]): tick mites live fast and die young, amber sloths crawl and amber a rig they touch, rewind newts rewind when hurt | **time-bubble bombs** ([`crate::explosive::ExplosiveKind::TimeBubble`]) slow every creature in their bubble and stop a newt rewinding; ride the fast lanes, skirt the slow pools |
//! | Echo Caves | **total darkness**: a scanner press is a **sonar ping** whose wave reveals the cave for a moment ([`sonar_reveal`]; the Sonar Suite reaches much further); **blind hunters** hunt by the noise a rig makes ([`rig_noise_tiles`]): thrust, drilling and above all pings | coast quietly, ping sparingly; kill shriekers before they scream; a hush moth hides your noise but swallows your pings |
//! | Prism Desert | **colours split** (chromatic aberration, stronger deeper); walls are **colour-keyed** ([`LampFilter`]) and open only in light of the matching colour, closing again once the light leaves | swap the lamp filter (per rig, the use button beside a keyed wall); prism scarabs only crack under the matching light; spires' bolts of your colour pass through you |
//! | Gate | **The Glitch** rewrites its arena's rules every phase ([`GlitchRule`]: gravity up, mirroring, time warp, sideways gravity with pixel rot), each change telegraphed for [`GLITCH_REWRITE_MILLIS`] while it cannot attack | learn each rule, shoot it between rewrites; rewards the Phase Drill (drills builder alloy, which seals Descent VII) |
//!
//! State: [`FarStrange`] (shared, saved, replicated as the appended `FarStrange` section and
//! hashed once anything happened: lamp filters, sonar pings, amber on rigs, landmarks found,
//! The Glitch's fight) and [`FarSchedule`] (host bookkeeping: open keyed walls waiting to close,
//! rewind newts' histories; saved, never hashed or replicated).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    cave::CaveTile,
    descent::Stratum,
    enemy::EnemyKind,
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TilePosition},
};

// --- Time Sludge ----------------------------------------------------------------------------

/// What a time pocket does.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum TimePocketKind {
    /// Time crawls: everything inside moves (and burns fuel) slower.
    Slow,
    /// Time races: everything inside moves faster.
    Fast,
}

/// A slow pool's time scale at the Time Sludge's top and bottom (gently, then pushed).
pub const SLOW_POOL_SCALE: (f32, f32) = (0.62, 0.32);
/// A fast lane's time scale at the Time Sludge's top and bottom.
pub const FAST_LANE_SCALE: (f32, f32) = (1.3, 1.85);

/// The time scale of a pocket of `kind` at `along` per mille through the Time Sludge.
#[must_use]
pub fn time_pocket_scale(kind: TimePocketKind, along: u16) -> f32 {
    let t = f32::from(along.min(1_000)) / 1_000.0;
    let (top, bottom) = match kind {
        TimePocketKind::Slow => SLOW_POOL_SCALE,
        TimePocketKind::Fast => FAST_LANE_SCALE,
    };
    (bottom - top).mul_add(t, top)
}

/// How long a time bubble lasts (pulses of [`TIME_BUBBLE_PULSE_MILLIS`]).
pub const TIME_BUBBLE_PULSES: u8 = 20;
/// Time between a time bubble's pulses (it swells, holds, then thins).
pub const TIME_BUBBLE_PULSE_MILLIS: u64 = 500;
/// Widest a time bubble gets, world pixels.
pub const TIME_BUBBLE_MAX_RADIUS: f32 = TILE_SIZE * 5.5;
/// How fast a creature inside a full time bubble lives (a quarter speed).
pub const TIME_BUBBLE_TEMPO: f32 = 0.22;
/// Time bubbles in a pack and the pack's base price (scaled by the Descent's price percent).
pub const TIME_BUBBLE_PACK: (u32, u32) = (3, 900);

/// Radius of a time bubble at pulse `stage` (1 right after it bursts; 0 before and after).
#[must_use]
pub fn time_bubble_radius(stage: u8) -> f32 {
    if stage == 0 || stage > TIME_BUBBLE_PULSES {
        return 0.0;
    }
    let grow = (f32::from(stage) / 2.0).min(1.0);
    let fade = (f32::from(TIME_BUBBLE_PULSES + 1 - stage) / 3.0).min(1.0);
    TIME_BUBBLE_MAX_RADIUS * grow.sqrt() * fade.sqrt()
}

/// How long an amber sloth's touch holds a rig in amber.
pub const AMBER_MILLIS: u64 = 2_200;
/// How fast an ambered rig lives (its motion and fuel burn).
pub const AMBER_TEMPO: f32 = 0.3;

/// How long a tick mite lives, in world seconds of its own fast life (it crumbles to dust).
pub const TICK_MITE_LIFE_SECONDS: u8 = 24;
/// A rewind newt's memory: how far back it rewinds.
pub const REWIND_MILLIS: u64 = 3_000;
/// How often a newt remembers where it was.
pub const REWIND_SAMPLE_MILLIS: u64 = 500;
/// How many rewinds a newt has in it.
pub const REWIND_CHARGES: u8 = 2;
/// A newt rewinds once its health falls under this share of its most.
pub const REWIND_BELOW: f32 = 0.45;

/// How fast a creature of `kind` lives (1 for every creature that keeps the world's time).
#[must_use]
pub const fn creature_tempo(kind: EnemyKind) -> f32 {
    match kind {
        EnemyKind::TickMite => 2.1,
        EnemyKind::AmberSloth => 0.5,
        EnemyKind::RewindNewt => 1.2,
        _ => 1.0,
    }
}

// --- Echo Caves -----------------------------------------------------------------------------

/// The Echo Caves' light (per one): next to nothing, the lamp a pinprick.
pub const ECHO_AMBIENT: f32 = 0.0;
/// A rig lamp's reach in the Echo Caves, as a multiple of its usual reach (the dark swallows
/// it: only sonar shows the cave).
pub const ECHO_LAMP_SCALE: f32 = 0.18;
/// A sonar ping's reach without the Sonar Suite, tiles.
pub const SONAR_PING_TILES: u8 = 14;
/// ... with the Sonar Suite (echo mapping).
pub const SONAR_SUITE_PING_TILES: u8 = 32;
/// How fast a ping's wave runs out, tiles a second.
pub const SONAR_WAVE_TILES_PER_SECOND: f32 = 36.0;
/// How long a tile stays lit after the wave passes it (it fades out over this).
pub const SONAR_REVEAL_MILLIS: u64 = 3_600;
/// Most pings remembered (the oldest fades first).
pub const MAX_SONAR_PINGS: usize = 16;
/// A ping inside a hush moth's hush reaches only this share as far.
pub const HUSHED_PING_SHARE: f32 = 0.3;
/// A hush moth's hush, tiles: rigs inside make no noise and their pings are muffled.
pub const HUSH_RADIUS_TILES: f32 = 5.0;
/// An echo quartz pillar rings (a free ping round it) when a rig strikes it this hard (px/s).
pub const QUARTZ_RING_SPEED: f32 = 70.0;
/// A quartz pillar's ping reach, tiles.
pub const QUARTZ_PING_TILES: u8 = 22;
/// A rig rings quartz at most this often.
pub const QUARTZ_RING_MILLIS: u64 = 2_500;

/// A sonar ping's wave: where it started, when, and how far it reaches.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct SonarPing {
    /// Who pinged (`None`: an echo quartz pillar or a shrieker's scream).
    pub player: Option<PlayerId>,
    /// The tile it started at.
    pub x: i32,
    pub y: i32,
    /// When it started.
    pub tick: SimulationTick,
    /// How far it reaches, tiles.
    pub reach: u8,
}

/// How lit tile `(x, y)` is by `ping`, `millis_since` after it started (`0..=1`): nothing until
/// its wave arrives, fully lit as it passes, fading out over [`SONAR_REVEAL_MILLIS`]; nothing
/// past its reach.
#[must_use]
pub fn sonar_reveal(ping: &SonarPing, millis_since: u64, x: i32, y: i32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "tile distances are small")]
    let distance = ((x - ping.x) as f32).hypot((y - ping.y) as f32);
    let reach = f32::from(ping.reach);
    if distance > reach {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a few seconds")]
    let seconds = millis_since as f32 / 1_000.0;
    let arrival = distance / SONAR_WAVE_TILES_PER_SECOND;
    if seconds < arrival {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a few seconds")]
    let fade = (seconds - arrival) / (SONAR_REVEAL_MILLIS as f32 / 1_000.0);
    // The far rim of a ping is fainter: an echo, not a lamp.
    let rim = 0.45f32.mul_add(-(distance / reach.max(1.0)), 1.0);
    ((1.0 - fade).clamp(0.0, 1.0) * rim).max(0.0)
}

/// Whether `ping` still lights anything `millis_since` after it started.
#[must_use]
pub fn sonar_alive(ping: &SonarPing, millis_since: u64) -> bool {
    let crossing = f32::from(ping.reach) / SONAR_WAVE_TILES_PER_SECOND;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few seconds"
    )]
    let crossing_millis = (crossing * 1_000.0) as u64;
    millis_since <= crossing_millis + SONAR_REVEAL_MILLIS
}

/// Noise a rig makes: the distance (tiles) a blind hunter hears it from.
///
/// Nothing while it rests or coasts, a little while it rolls, more under thrust, most while
/// drilling, and a sonar ping carries far.
#[must_use]
pub fn rig_noise_tiles(speed: f32, thrusting: bool, drilling: bool, pinged: bool) -> f32 {
    let mut noise: f32 = if speed > 40.0 { 3.0 } else { 0.0 };
    if thrusting {
        noise = noise.max(9.0);
    }
    if drilling {
        noise = noise.max(12.0);
    }
    if pinged {
        noise = noise.max(26.0);
    }
    noise
}

/// How long after a ping a rig still rings in blind hunters' ears.
pub const PING_NOISE_MILLIS: u64 = 2_500;
/// A shrieker's scream calls every blind hunter this close to its prey (tiles).
pub const SHRIEK_CALL_TILES: f32 = 28.0;
/// A shrieker screams at most this often.
pub const SHRIEK_MILLIS: u64 = 6_000;

// --- Prism Desert ---------------------------------------------------------------------------

/// A rig lamp's colour filter (per rig; the Prism Desert's keyed walls open only in light of
/// their own colour).
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum LampFilter {
    /// No filter: plain white light opens no keyed wall.
    #[default]
    White,
    Red,
    Green,
    Blue,
}

impl LampFilter {
    /// Every filter, in the swap cycle's order.
    pub const ALL: [Self; 4] = [Self::White, Self::Red, Self::Green, Self::Blue];
    /// The keyed colours.
    pub const KEYED: [Self; 3] = [Self::Red, Self::Green, Self::Blue];

    /// The next filter in the cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::White => Self::Red,
            Self::Red => Self::Green,
            Self::Green => Self::Blue,
            Self::Blue => Self::White,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
        }
    }

    /// The light's colour (presentation: the lamp's tint, the HUD's swatch).
    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::White => [255, 248, 230],
            Self::Red => [255, 92, 84],
            Self::Green => [96, 255, 128],
            Self::Blue => [92, 156, 255],
        }
    }

    /// The keyed wall this light opens, if any.
    #[must_use]
    pub const fn wall(self) -> Option<CaveTile> {
        match self {
            Self::White => None,
            Self::Red => Some(CaveTile::PrismRed),
            Self::Green => Some(CaveTile::PrismGreen),
            Self::Blue => Some(CaveTile::PrismBlue),
        }
    }

    /// The filter whose light opens keyed wall `tile`, if it is one.
    #[must_use]
    pub const fn of_wall(tile: CaveTile) -> Option<Self> {
        match tile {
            CaveTile::PrismRed => Some(Self::Red),
            CaveTile::PrismGreen => Some(Self::Green),
            CaveTile::PrismBlue => Some(Self::Blue),
            _ => None,
        }
    }

    /// The colour of a prism scarab's shell or a splitter spire's bolt numbered `index`.
    #[must_use]
    pub const fn keyed(index: u64) -> Self {
        match index % 3 {
            0 => Self::Red,
            1 => Self::Green,
            _ => Self::Blue,
        }
    }
}

/// How far a filtered lamp opens keyed walls of its colour, tiles.
pub const PRISM_KEY_REACH_TILES: i32 = 5;
/// An opened keyed wall closes this long after the last matching light leaves it.
pub const PRISM_CLOSE_MILLIS: u64 = 2_500;
/// A keyed wall that cannot close (a body in it) tries again this much later.
pub const PRISM_RETRY_MILLIS: u64 = 800;
/// Most keyed wall tiles open at once (the oldest close first past it).
pub const MAX_OPEN_KEYED: usize = 4_096;
/// How long a prism scarab's shell keeps one colour.
pub const SCARAB_SHELL_MILLIS: u64 = 4_000;
/// Damage a scarab takes in the matching light, and outside it.
pub const SCARAB_MATCHED: f32 = 1.6;
pub const SCARAB_UNMATCHED: f32 = 0.2;
/// The use button beside a keyed wall (this close, tiles) swaps the lamp filter.
pub const FILTER_SWAP_REACH_TILES: i32 = 8;

/// The shell colour of the prism scarab `id` at `millis` of world time (it turns every
/// [`SCARAB_SHELL_MILLIS`]; a pure function of time, so every client agrees).
#[must_use]
pub const fn scarab_shell(id: u64, millis: u64) -> LampFilter {
    LampFilter::keyed(millis / SCARAB_SHELL_MILLIS + id)
}

// --- The Glitch -----------------------------------------------------------------------------

/// The rule The Glitch writes over its arena.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum GlitchRule {
    /// The world's own rules (its first phase: learn its attacks).
    #[default]
    Baseline,
    /// Gravity points up: fall to the ceiling, turn the rig with its thrusters.
    GravityUp,
    /// Screen and controls mirrored.
    Mirrored,
    /// Time races for everything in the arena.
    TimeWarp,
    /// Gravity points sideways and the picture rots into big pixels.
    Sideways,
}

impl GlitchRule {
    /// The rules in the order The Glitch writes them, one per phase.
    pub const PHASES: [Self; 5] = [
        Self::Baseline,
        Self::GravityUp,
        Self::Mirrored,
        Self::TimeWarp,
        Self::Sideways,
    ];

    /// The words the arena's warning shows (and the news line).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Baseline => "BASELINE",
            Self::GravityUp => "GRAVITY: UP",
            Self::Mirrored => "MIRROR: ON",
            Self::TimeWarp => "TIME: x1.6",
            Self::Sideways => "GRAVITY: SIDEWAYS / RES: LOW",
        }
    }

    /// The phase index (0-based).
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }
}

/// The Glitch's health thresholds (per mille of its most) at which it rewrites the next rule.
pub const GLITCH_THRESHOLDS: [u16; 4] = [800, 600, 400, 200];
/// How long a rewrite is telegraphed (The Glitch flickers and cannot attack or be hurt).
pub const GLITCH_REWRITE_MILLIS: u64 = 3_000;
/// The Glitch blinks (teleports) this often; its ghost shows where for [`GLITCH_BLINK_TELL_MILLIS`].
pub const GLITCH_BLINK_MILLIS: u64 = 9_000;
pub const GLITCH_BLINK_TELL_MILLIS: u64 = 1_100;
/// The Glitch sleeps (and heals) once no rig has been in its arena for this long.
pub const GLITCH_SLEEP_MILLIS: u64 = 30_000;
/// Most glitch shards about at once.
pub const MAX_GLITCH_SHARDS: usize = 2;
/// How often The Glitch sheds shards (from its third phase).
pub const GLITCH_SHARD_MILLIS: u64 = 11_000;
/// The time scale of the time-warp rule.
pub const GLITCH_TIME_WARP: f32 = 1.6;

/// The Glitch's fight.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct GlitchState {
    /// Beaten: the Phase Drill handed out.
    #[serde(default)]
    pub beaten: bool,
    /// Its entity while it is awake.
    #[serde(default)]
    pub entity: Option<u64>,
    /// The rule over the arena now.
    #[serde(default)]
    pub rule: GlitchRule,
    /// The rule being written, and when the rewrite lands.
    #[serde(default)]
    pub rewriting: Option<(GlitchRule, SimulationTick)>,
    /// The arena's field zone while a rule other than the baseline holds.
    #[serde(default)]
    pub zone: Option<u32>,
    /// The next blink, and where to (a tile in the arena) once its ghost shows.
    #[serde(default)]
    pub next_blink: SimulationTick,
    #[serde(default)]
    pub blink_to: Option<(i32, i32)>,
    #[serde(default)]
    pub next_shards: SimulationTick,
    /// Its shards alive.
    #[serde(default)]
    pub shards: Vec<u64>,
    /// The last tick a rig was in its arena.
    #[serde(default)]
    pub last_visited: SimulationTick,
}

impl GlitchState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether it is awake.
    #[must_use]
    pub const fn awake(&self) -> bool {
        self.entity.is_some()
    }

    /// Whether a rewrite is under way (it cannot be hurt).
    #[must_use]
    pub const fn rewriting(&self) -> bool {
        self.rewriting.is_some()
    }
}

/// The rule after `rule`, if The Glitch has one left.
#[must_use]
pub const fn next_rule(rule: GlitchRule) -> Option<GlitchRule> {
    match rule {
        GlitchRule::Baseline => Some(GlitchRule::GravityUp),
        GlitchRule::GravityUp => Some(GlitchRule::Mirrored),
        GlitchRule::Mirrored => Some(GlitchRule::TimeWarp),
        GlitchRule::TimeWarp => Some(GlitchRule::Sideways),
        GlitchRule::Sideways => None,
    }
}

// --- Landmarks and their story ------------------------------------------------------------

/// The far Strange Layers' landmarks a crew can find (each found once, with its story beat).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FarLandmark {
    /// Time Sludge: the builders' clock hall, its floor striped slow and fast.
    ClockHall,
    /// Echo Caves: the Choir, a hall of echo quartz pillars that ring.
    Choir,
    /// Prism Desert: a spectrum vault behind walls of three colours.
    SpectrumVault,
    /// The Glitch's arena (Descent VI's gate).
    GlitchArena,
}

impl FarLandmark {
    pub const ALL: [Self; 4] = [
        Self::ClockHall,
        Self::Choir,
        Self::SpectrumVault,
        Self::GlitchArena,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ClockHall => "The Stopped Clock",
            Self::Choir => "The Choir",
            Self::SpectrumVault => "A Spectrum Vault",
            Self::GlitchArena => "The Rewrite Hall",
        }
    }

    /// The stratum it stands in.
    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::ClockHall => Stratum::TimeSludge,
            Self::Choir => Stratum::EchoCaves,
            Self::SpectrumVault | Self::GlitchArena => Stratum::PrismDesert,
        }
    }

    /// Its story beat: what the crew reads there (the builders' marks, the Fathers' traces).
    #[must_use]
    pub const fn beat(self) -> &'static str {
        match self {
            Self::ClockHall => {
                "A builders' clock with no hands, its floor striped slow and fast. On the slow \
                 side a kettle the Fathers left still steams; on the fast side their bedroll has \
                 rotted to thread. Time here is not a river. It is a material, and someone cut it."
            }
            Self::Choir => {
                "Quartz pillars that sing back whatever strikes them. Scratched into the biggest, \
                 in Amos's hand: 'Dov says the hum has words in it down here. Ping and listen.' \
                 Under it, a row of tally marks, one for every ping that came back changed."
            }
            Self::SpectrumVault => {
                "Walls of red, green and blue glass that let only their own light through. The \
                 builders kept their workings behind colours, like a child's lock. Ines's chalk \
                 on the floor: R, then G, then B. 'They wanted us to learn to see.'"
            }
            Self::GlitchArena => {
                "A hall where the rules are written on the walls and keep being rewritten. The \
                 builders' machine is close: reality is thin enough here to edit. Whatever \
                 guards it does not want to be read."
            }
        }
    }
}

// --- The world state ------------------------------------------------------------------------

/// The far Strange Layers' shared state (see the module docs).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FarStrange {
    /// Each rig's lamp filter (absent: white).
    #[serde(default)]
    pub filters: BTreeMap<PlayerId, LampFilter>,
    /// Sonar pings whose waves still light the cave.
    #[serde(default)]
    pub pings: Vec<SonarPing>,
    /// Rigs held in amber, until when.
    #[serde(default)]
    pub ambered: BTreeMap<PlayerId, SimulationTick>,
    /// Landmarks the crew has found.
    #[serde(default)]
    pub found: BTreeSet<FarLandmark>,
    #[serde(default)]
    pub glitch: GlitchState,
}

impl FarStrange {
    /// Whether nothing has happened here (left out of the state hash then, so every older
    /// world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
            && self.pings.is_empty()
            && self.ambered.is_empty()
            && self.found.is_empty()
            && self.glitch.is_default()
    }

    /// `player`'s lamp filter.
    #[must_use]
    pub fn filter_of(&self, player: PlayerId) -> LampFilter {
        self.filters.get(&player).copied().unwrap_or_default()
    }

    /// Whether `player`'s rig is held in amber at `now`.
    #[must_use]
    pub fn is_ambered(&self, player: PlayerId, now: SimulationTick) -> bool {
        self.ambered.get(&player).is_some_and(|until| now < *until)
    }

    /// Forgets a leaving player.
    pub fn forget(&mut self, player: PlayerId) {
        self.filters.remove(&player);
        self.ambered.remove(&player);
    }
}

/// One remembered moment of a rewind newt.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct NewtMoment {
    pub tick: SimulationTick,
    pub x: f32,
    pub y: f32,
    pub health: f32,
}

impl crate::position::Reframe for NewtMoment {
    fn reframe(&mut self, shift: crate::position::FrameShift) {
        let (x, y) = shift.point((self.x, self.y));
        self.x = x;
        self.y = y;
    }
}

/// Host bookkeeping of the far Strange Layers: saved, never hashed or replicated (clients see
/// its effects as terrain writes and entity updates).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct FarSchedule {
    /// Keyed wall tiles standing open, their colour, and when they close (pushed back while a
    /// matching light is on them).
    #[serde(default, with = "crate::pair_list")]
    pub open_keyed: BTreeMap<TilePosition, (LampFilter, SimulationTick)>,
    /// Each rewind newt's recent moments (oldest first).
    #[serde(default)]
    pub newts: BTreeMap<u64, Vec<NewtMoment>>,
    /// When each rig last rang an echo quartz pillar.
    #[serde(default)]
    pub rung: BTreeMap<PlayerId, SimulationTick>,
    /// When each rig last pinged (blind hunters hear it for [`PING_NOISE_MILLIS`]).
    #[serde(default)]
    pub pinged: BTreeMap<PlayerId, SimulationTick>,
    /// The time pocket each rig is in (-1 slow, 1 fast; absent: neither), so crossing into one
    /// is announced once.
    #[serde(default)]
    pub pockets: BTreeMap<PlayerId, i8>,
    /// When each shrieker last screamed.
    #[serde(default)]
    pub shrieked: BTreeMap<u64, SimulationTick>,
    /// Each rig's speed last tick (a rig striking quartz stops dead: the blow is last tick's
    /// speed).
    #[serde(default)]
    pub speeds: BTreeMap<PlayerId, f32>,
    /// The rigs whose thrusters fire this tick (rebuilt from the held controls at the start of
    /// every tick): a hovering rig is still loud to a blind hunter.
    #[serde(default)]
    pub thrusting: BTreeSet<PlayerId>,
}

impl FarSchedule {
    /// Whether nothing waits.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.open_keyed.is_empty() && self.newts.is_empty()
    }
}

// --- Events ---------------------------------------------------------------------------------

/// What a [`crate::event::GameplayEventKind::FarStrange`] event reports (netcode codes
/// `580 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FarStrangeEvent {
    /// A rig entered a slow pool or a fast lane.
    TimeShift,
    /// A time bubble burst.
    TimeBubble,
    /// An amber sloth held a rig in amber.
    Ambered,
    /// A rewind newt rewound itself.
    Rewound,
    /// A tick mite crumbled at the end of its life.
    MiteCrumbled,
    /// A sonar ping went out.
    SonarPing,
    /// An echo quartz pillar rang.
    QuartzRang,
    /// A shrieker screamed: the blind hunters near it come.
    Shriek,
    /// A rig swapped its lamp filter.
    FilterSwapped,
    /// Keyed walls opened to a matching light.
    KeyedOpened,
    /// A landmark was found (its story beat).
    LandmarkFound,
    /// The Glitch woke.
    GlitchAwoke,
    /// The Glitch began rewriting its arena's rule (the telegraph).
    GlitchRewriting,
    /// The new rule landed.
    GlitchRewrote,
    /// The Glitch blinked across its arena.
    GlitchBlinked,
    /// The Glitch fell.
    GlitchDefeated,
    /// The gate's reward (the Phase Drill) was fitted to a rig.
    GateRewards,
    /// Time bubbles were bought.
    Bought,
}

impl FarStrangeEvent {
    /// Offset of the event's netcode code from 580.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

// --- Commands -------------------------------------------------------------------------------

/// A far Strange Layers command (sent as `LogisticsCommand::FarStrange`, so the depth logistics
/// dialog and the use button serve it with every input already wired).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FarStrangeAction {
    /// Swap to the next lamp filter.
    CycleFilter,
    /// Fit a particular lamp filter.
    SetFilter(LampFilter),
    /// Buy a pack of time bubbles (at outposts and in town).
    BuyTimeBubbles,
}

/// Why a far Strange Layers command is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FarStrangeRefusal {
    /// Lamp filters come with the Strange Layers' kit: only on Deep Descent worlds.
    NoFilters,
    /// That filter is already fitted.
    SameFilter,
    /// Time bubbles sell at outposts and in town.
    NoBubbleSeller,
    /// On foot there is no lamp to filter.
    OnFoot,
}

impl FarStrangeRefusal {
    /// The refusal in words.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::NoFilters => "Lamp filters belong to the Strange Layers' kit.",
            Self::SameFilter => "That filter is already on the lamp.",
            Self::NoBubbleSeller => "Time bubbles sell at outposts and in town.",
            Self::OnFoot => "Board the rig to change its lamp.",
        }
        .to_owned()
    }
}

/// The far Strange Layers' authored content in `stratum` for the descent probe: `(mechanics,
/// set pieces)` beyond what the registry already says, and the new things' names.
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8, &'static [&'static str]) {
    match stratum {
        Stratum::TimeSludge => (
            2,
            1,
            &[
                "slow pools",
                "fast lanes",
                "time-bubble bombs",
                "the Stopped Clock",
            ],
        ),
        Stratum::EchoCaves => (
            2,
            1,
            &[
                "sonar pings",
                "noise and blind hunters",
                "echo quartz",
                "the Choir",
            ],
        ),
        Stratum::PrismDesert => (
            2,
            2,
            &[
                "lamp filters",
                "colour-keyed walls",
                "spectrum vaults",
                "The Glitch",
            ],
        ),
        _ => (0, 0, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pockets_deepen_and_bubbles_swell_then_thin() {
        assert!(
            time_pocket_scale(TimePocketKind::Slow, 0)
                > time_pocket_scale(TimePocketKind::Slow, 1_000)
        );
        assert!(
            time_pocket_scale(TimePocketKind::Fast, 1_000)
                > time_pocket_scale(TimePocketKind::Fast, 0)
        );
        assert!(time_pocket_scale(TimePocketKind::Slow, 1_000) < 0.5);
        assert!(time_bubble_radius(0) <= 0.0);
        assert!(time_bubble_radius(1) > 0.0);
        let full = time_bubble_radius(TIME_BUBBLE_PULSES / 2);
        assert!((full - TIME_BUBBLE_MAX_RADIUS).abs() < 1e-3);
        assert!(time_bubble_radius(TIME_BUBBLE_PULSES) < full);
        assert!(time_bubble_radius(TIME_BUBBLE_PULSES + 1) <= 0.0);
    }

    #[test]
    fn a_ping_lights_tiles_as_its_wave_passes_then_fades() {
        let ping = SonarPing {
            player: None,
            x: 0,
            y: 0,
            tick: SimulationTick::new(0),
            reach: 20,
        };
        // Ten tiles out: dark before the wave, lit as it passes, gone after the fade.
        assert!(sonar_reveal(&ping, 100, 10, 0) <= 0.0);
        assert!(sonar_reveal(&ping, 300, 10, 0) > 0.5);
        assert!(sonar_reveal(&ping, 300 + SONAR_REVEAL_MILLIS, 10, 0) <= 0.0);
        // Past its reach: never.
        assert!(sonar_reveal(&ping, 2_000, 25, 0) <= 0.0);
        assert!(sonar_alive(&ping, 1_000));
        assert!(!sonar_alive(&ping, 10_000));
    }

    #[test]
    fn noise_grows_with_what_a_rig_does() {
        assert!(rig_noise_tiles(0.0, false, false, false) <= 0.0);
        let rolling = rig_noise_tiles(100.0, false, false, false);
        let thrust = rig_noise_tiles(100.0, true, false, false);
        let drill = rig_noise_tiles(0.0, false, true, false);
        let ping = rig_noise_tiles(0.0, false, false, true);
        assert!(rolling < thrust && thrust < drill && drill < ping);
    }

    #[test]
    fn filters_key_their_walls_and_scarabs_turn_colour() {
        for filter in LampFilter::KEYED {
            let wall = filter.wall().expect("keyed");
            assert_eq!(LampFilter::of_wall(wall), Some(filter));
        }
        assert!(LampFilter::White.wall().is_none());
        assert_eq!(LampFilter::Blue.next(), LampFilter::White);
        let first = scarab_shell(7, 0);
        assert_eq!(scarab_shell(7, SCARAB_SHELL_MILLIS - 1), first);
        assert_ne!(scarab_shell(7, SCARAB_SHELL_MILLIS), first);
    }

    #[test]
    fn the_glitch_writes_every_rule_once_in_order() {
        let mut rule = GlitchRule::Baseline;
        let mut seen = vec![rule];
        while let Some(next) = next_rule(rule) {
            rule = next;
            seen.push(rule);
        }
        assert_eq!(seen, GlitchRule::PHASES.to_vec());
        assert_eq!(GLITCH_THRESHOLDS.len() + 1, GlitchRule::PHASES.len());
    }
}
