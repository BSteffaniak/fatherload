//! Descent VII part B (expansion plan §14, strata 4-5 and the gate; Wave 10 track
//! ancients-b): the **Sleeping Garrison**, **the Seal Gate** and the **Gatekeeper Colossus**.
//!
//! Pure data and math; the tick lives in `world/garrison.rs`, the creatures' behaviour and the
//! Colossus's fight in `world/enemies/garrison.rs`, the generated places in
//! [`crate::worldgen::descent::garrison`].
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | Sleeping Garrison | **the garrison wakes in waves**: a rig that walks into a builders' fort stirs it; after a build window, war machines climb out of the fort's bays wave after wave ([`wave_roster`]), each wave telegraphed by the bays it will come from ([`WAVE_TELL_MILLIS`]). Hold the fort (stay in it) until the garrison sleeps again and its armoury pays out. Deeper forts and the Grand Bastion field more and bigger waves | **base defence**: the build kit's defences ([`DefenceKind`]: turrets that shoot, war-plate barricades that hold machines, shock traps that stun them) go up in any claim and, without one, inside a fort. Walking away lets the garrison stand down (no penalty, the waves start over) |
//! | The Seal Gate | **tremors** ([`tremor_period`]): the Colossus stirs behind the gate, the stratum shakes, and stones fall from the ceilings over rigs (dust shows where for [`TREMOR_TELL_MILLIS`]); more often deeper | keep moving when the dust falls; low tunnels shed less |
//! | Gate | **The Seal Gate** opens with the keys of every Descent ([`SealKey`]; sockets on the door light for the keys a rig carries), then the **Gatekeeper Colossus**: three stages across a **collapsing hall** ([`ColossusStage`]); between stages the hall gives way, telegraphed for [`COLLAPSE_TELL_MILLIS`] while the Colossus cannot attack or be hurt, and never onto a body | read the fists' tells, strike the head, climb the rubble; rewards the Heart Key (opens Descent VIII through the Heart Seal) |
//!
//! State: [`Garrison`] (shared, saved, replicated as the appended `Garrison` section and
//! hashed once anything happened: forts and their waves, defences, the Seal Gate, the
//! Colossus's fight, tremor stones, landmarks found) and [`GarrisonSchedule`] (host
//! bookkeeping: whom the gate refused; saved, never hashed or replicated).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::EnemyKind,
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TilePosition},
    worldgen::TileRect,
};

// --- Forts and waves (Sleeping Garrison) ------------------------------------------------------

/// What kind of fort a garrison holds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FortKind {
    /// A barracks fort: three waves.
    Barracks,
    /// The Grand Bastion (one per world): five waves and the biggest armoury.
    Bastion,
}

impl FortKind {
    /// How many waves it fields before it sleeps.
    #[must_use]
    pub const fn waves(self) -> u8 {
        match self {
            Self::Barracks => 3,
            Self::Bastion => 5,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Barracks => "a garrison fort",
            Self::Bastion => "the Grand Bastion",
        }
    }

    /// The armoury's pay-out per rig that held the fort: `(credits before the Descent's price
    /// percent, war plate, ancient alloy)`.
    #[must_use]
    pub const fn armoury(self) -> (u32, u32, u32) {
        match self {
            Self::Barracks => (9_000, 6, 2),
            Self::Bastion => (26_000, 14, 6),
        }
    }
}

/// The build window between a fort stirring and its first wave.
pub const STIR_MILLIS: u64 = 18_000;
/// How long a wave's bays glow before its machines climb out.
pub const WAVE_TELL_MILLIS: u64 = 5_000;
/// The breather between a wave cleared and the next wave's tell.
pub const BREATHER_MILLIS: u64 = 9_000;
/// How long a stirred fort waits for a rig to come back before it stands down.
pub const ABANDON_MILLIS: u64 = 15_000;
/// Most machines a fort has out at once.
pub const MAX_FORT_MACHINES: usize = 12;
/// How far round a fort's walls a rig still holds it, tiles.
pub const FORT_REACH_TILES: i32 = 6;

/// The machines of wave `wave` (1-based) of a `kind` fort held by `rigs` rigs, `along` per
/// mille down the Sleeping Garrison (deeper forts field bigger waves).
#[must_use]
pub fn wave_roster(kind: FortKind, wave: u8, rigs: usize, along: u16) -> Vec<EnemyKind> {
    let wave = u32::from(wave.max(1));
    let deep = u32::from(along.min(1_000)) / 400;
    let mut sentries = 1 + wave + deep;
    let mut drones = wave / 2 + u32::from(wave >= 2);
    let mut walkers = u32::from(wave >= 3) + u32::from(wave >= 5) + u32::from(deep >= 2);
    if kind == FortKind::Bastion {
        sentries += 1;
        drones += wave / 3;
    }
    // Co-op: every extra rig half as many again, at most twice the solo wave.
    let rigs = u32::try_from(rigs.clamp(1, 4)).unwrap_or(1);
    let scale = |count: u32| (count * (rigs + 1)).div_ceil(2).min(count * 2);
    sentries = scale(sentries);
    drones = scale(drones);
    walkers = scale(walkers).min(3);
    let mut roster = Vec::new();
    // Interleaved so a capped wave still mixes its kinds.
    let most = sentries.max(drones).max(walkers);
    for index in 0..most {
        if index < walkers {
            roster.push(EnemyKind::SiegeWalker);
        }
        if index < sentries {
            roster.push(EnemyKind::WarSentry);
        }
        if index < drones {
            roster.push(EnemyKind::LancerDrone);
        }
    }
    roster.truncate(MAX_FORT_MACHINES);
    roster
}

/// Where a fort is in its waking and sleeping.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum FortPhase {
    /// Asleep, waiting for a rig to walk in.
    #[default]
    Dormant,
    /// Stirring: the build window before the first wave (until `due`).
    Stirring,
    /// A wave's bays glow (until `due`, when its machines climb out).
    Telegraph,
    /// A wave is out.
    Wave,
    /// Between waves (until `due`, when the next wave's tell starts).
    Breather,
    /// Cleared: the garrison sleeps for good and its armoury paid out.
    Asleep,
}

/// One fort's state.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FortState {
    pub kind: FortKind,
    /// The fort's walls (every tile it holds).
    pub bounds: TileRect,
    #[serde(default)]
    pub phase: FortPhase,
    /// The wave out, telegraphed or last cleared (0 before the first).
    #[serde(default)]
    pub wave: u8,
    /// When the current phase ends (stirring, telegraph, breather).
    #[serde(default)]
    pub due: SimulationTick,
    /// The machines of the wave out.
    #[serde(default)]
    pub machines: Vec<u64>,
    /// The bays the next (or current) wave climbs out of: they glow.
    #[serde(default)]
    pub lit_bays: Vec<TilePosition>,
    /// The last tick a rig held the fort.
    #[serde(default)]
    pub last_held: SimulationTick,
}

impl FortState {
    /// A dormant fort of `kind` within `bounds`.
    #[must_use]
    pub const fn new(kind: FortKind, bounds: TileRect) -> Self {
        Self {
            kind,
            bounds,
            phase: FortPhase::Dormant,
            wave: 0,
            due: SimulationTick::new(0),
            machines: Vec::new(),
            lit_bays: Vec::new(),
            last_held: SimulationTick::new(0),
        }
    }

    /// Whether the garrison is up (stirring or fighting).
    #[must_use]
    pub const fn awake(&self) -> bool {
        matches!(
            self.phase,
            FortPhase::Stirring | FortPhase::Telegraph | FortPhase::Wave | FortPhase::Breather
        )
    }
}

// --- Defences (the build kit's base defence) --------------------------------------------------

/// A defence the build kit places (`Buildable::Defence`): in any claim, and inside a garrison
/// fort without one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DefenceKind {
    /// An auto-turret on a tripod: shoots the nearest hostile it sees.
    Turret,
    /// A war-plate barricade: a solid block machines must batter down.
    Barricade,
    /// A shock trap on the floor: zaps and stuns what walks over it, then re-arms.
    ShockTrap,
}

impl DefenceKind {
    pub const ALL: [Self; 3] = [Self::Turret, Self::Barricade, Self::ShockTrap];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Turret => "Auto-Turret",
            Self::Barricade => "War-Plate Barricade",
            Self::ShockTrap => "Shock Trap",
        }
    }

    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Turret => "Shoots the nearest hostile in sight. Machines batter it.",
            Self::Barricade => "A solid block machines must batter down.",
            Self::ShockTrap => "Zaps and stuns what walks over it, then re-arms.",
        }
    }

    /// Footprint `(width, height)` in tiles; placed by its bottom-left tile.
    #[must_use]
    pub const fn footprint(self) -> (i32, i32) {
        match self {
            Self::Turret => (1, 2),
            Self::Barricade | Self::ShockTrap => (1, 1),
        }
    }

    /// Whether it needs solid ground under it.
    #[must_use]
    pub const fn needs_floor(self) -> bool {
        matches!(self, Self::Turret | Self::ShockTrap)
    }

    /// Hit points when built.
    #[must_use]
    pub const fn health(self) -> u16 {
        match self {
            Self::Turret => 160,
            Self::Barricade => 240,
            Self::ShockTrap => 90,
        }
    }

    /// Credits before the Descent's price percent ([`crate::logistics::scaled_price`]).
    #[must_use]
    pub const fn base_credits(self) -> u32 {
        match self {
            Self::Turret => 600,
            Self::Barricade => 40,
            Self::ShockTrap => 180,
        }
    }

    /// Building blocks of any kind it takes (missing ones are bought).
    #[must_use]
    pub const fn blocks(self) -> u32 {
        match self {
            Self::Turret => 4,
            Self::Barricade => 2,
            Self::ShockTrap => 1,
        }
    }
}

/// How far a turret sees and shoots, tiles.
pub const TURRET_RANGE_TILES: f32 = 11.0;
/// Time between a turret's shots.
pub const TURRET_PERIOD_MILLIS: u64 = 600;
/// A turret shot's damage (kinetic).
pub const TURRET_DAMAGE: f32 = 14.0;
/// How long a turret's tracer shows (presentation).
pub const TRACER_MILLIS: u64 = 140;
/// A shock trap's zap (EMP: machines are also stunned).
pub const TRAP_DAMAGE: f32 = 26.0;
/// Time a shock trap takes to re-arm.
pub const TRAP_REARM_MILLIS: u64 = 2_500;
/// How long a shock trap stuns a machine.
pub const TRAP_STUN_MILLIS: u64 = 1_400;
/// How often machines batter the defences they reach.
pub const BATTER_MILLIS: u64 = 1_000;
/// How close a machine must be to batter a defence (from its body's edge, tiles).
pub const BATTER_REACH_TILES: f32 = 0.9;
/// A siege shell's blast on the defences it lands among: `(radius tiles, damage)`.
pub const SHELL_BLAST: (f32, u16) = (1.6, 45);

/// What a garrison machine's blow does to a defence, per [`BATTER_MILLIS`] (0: it does not).
#[must_use]
pub const fn batter_damage(kind: EnemyKind) -> u16 {
    match kind {
        EnemyKind::WarSentry => 14,
        EnemyKind::SiegeWalker => 34,
        EnemyKind::LancerDrone => 8,
        _ => 0,
    }
}

/// A turret shot's trace (presentation: a tracer for [`TRACER_MILLIS`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ShotTrace {
    pub tick: SimulationTick,
    /// The tile it hit.
    pub to: TilePosition,
}

/// A built defence.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Defence {
    pub kind: DefenceKind,
    /// Its bottom-left tile (where it was placed).
    pub anchor: TilePosition,
    /// Who built it.
    pub owner: PlayerId,
    pub health: u16,
    /// A turret's next shot, a trap's re-arm.
    #[serde(default)]
    pub ready: SimulationTick,
    /// A turret's last shot.
    #[serde(default)]
    pub fired: Option<ShotTrace>,
    /// Credits paid (half come back when it is taken down).
    #[serde(default)]
    pub paid: u32,
}

impl Defence {
    /// Whether it covers `tile`.
    #[must_use]
    pub const fn covers(&self, tile: TilePosition) -> bool {
        let (width, height) = self.kind.footprint();
        tile.x >= self.anchor.x
            && tile.x < self.anchor.x + width
            && tile.y <= self.anchor.y
            && tile.y > self.anchor.y - height
    }

    /// Its top tile (a turret's head).
    #[must_use]
    pub const fn top(&self) -> TilePosition {
        let (_, height) = self.kind.footprint();
        TilePosition {
            x: self.anchor.x,
            y: self.anchor.y - height + 1,
        }
    }

    /// Whether a trap is armed at `now`.
    #[must_use]
    pub fn armed(&self, now: SimulationTick) -> bool {
        self.ready <= now
    }
}

// --- The Seal Gate ---------------------------------------------------------------------------

/// A key of an earlier Descent the Seal Gate asks for (one socket each on its door).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SealKey {
    /// The Crust's Deep Claim (the Sinkhole).
    DeepClaim,
    /// The Old Deep's Pressure Hull (the Star Core).
    PressureHull,
    /// The Living Dark's Bio-Filter (the Mycelial Mind).
    BioFilter,
    /// The Drowned and Frozen's Sonar Suite and Cryo Hull (the Leviathan).
    SonarCryo,
    /// The Hollow World's Gravity Anchor (the Sun Serpent).
    GravityAnchor,
    /// The Strange Layers' Phase Drill (The Glitch).
    PhaseDrill,
}

impl SealKey {
    /// Every key, in socket order (top left to bottom right).
    pub const ALL: [Self; 6] = [
        Self::DeepClaim,
        Self::PressureHull,
        Self::BioFilter,
        Self::SonarCryo,
        Self::GravityAnchor,
        Self::PhaseDrill,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DeepClaim => "Deep Claim",
            Self::PressureHull => "Pressure Hull",
            Self::BioFilter => "Bio-Filter",
            Self::SonarCryo => "Sonar Suite and Cryo Hull",
            Self::GravityAnchor => "Gravity Anchor",
            Self::PhaseDrill => "Phase Drill",
        }
    }

    /// The Descent whose gate gave it (0-based).
    #[must_use]
    pub const fn descent(self) -> u8 {
        self as u8
    }

    /// The rig systems it stands for (none for the Deep Claim, which the world holds).
    #[must_use]
    pub const fn systems(self) -> &'static [RigSystem] {
        match self {
            Self::DeepClaim => &[],
            Self::PressureHull => &[RigSystem::PressureHull],
            Self::BioFilter => &[RigSystem::BioFilter],
            Self::SonarCryo => &[RigSystem::SonarSuite, RigSystem::CryoHull],
            Self::GravityAnchor => &[RigSystem::GravityAnchor],
            Self::PhaseDrill => &[RigSystem::PhaseDrill],
        }
    }

    /// Its socket's glow colour (presentation).
    #[must_use]
    pub const fn colour(self) -> [u8; 3] {
        match self {
            Self::DeepClaim => [214, 168, 96],
            Self::PressureHull => [120, 200, 255],
            Self::BioFilter => [140, 240, 120],
            Self::SonarCryo => [80, 230, 230],
            Self::GravityAnchor => [255, 196, 70],
            Self::PhaseDrill => [220, 120, 255],
        }
    }
}

/// How far from the door's face a rig must come for the gate to read its keys, tiles.
pub const SEAL_READ_TILES: i32 = 6;
/// Time between the sockets lighting one by one as the gate opens.
pub const SOCKET_MILLIS: u64 = 600;
/// Time per row of the door sinking into the floor once every socket is lit.
pub const DOOR_ROW_MILLIS: u64 = 320;

/// Sockets lit `millis` after the gate began to open.
#[must_use]
pub fn sockets_lit(millis: u64) -> usize {
    usize::try_from(millis / SOCKET_MILLIS + 1)
        .unwrap_or(usize::MAX)
        .min(SealKey::ALL.len())
}

/// Rows of a `rows`-tall door sunk `millis` after the gate began to open (from the top).
#[must_use]
pub fn door_rows_open(millis: u64, rows: i32) -> i32 {
    let lit = SOCKET_MILLIS * SealKey::ALL.len() as u64;
    if millis < lit {
        return 0;
    }
    i32::try_from((millis - lit) / DOOR_ROW_MILLIS + 1)
        .unwrap_or(i32::MAX)
        .min(rows)
}

/// The Seal Gate's state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct SealGateState {
    /// When it began to open (sockets lighting, then the door sinking), and who opened it.
    #[serde(default)]
    pub opening: Option<(SimulationTick, PlayerId)>,
    /// Whether the door is all the way open.
    #[serde(default)]
    pub open: bool,
}

// --- The Gatekeeper Colossus -----------------------------------------------------------------

/// The Colossus's stage (and the hall's shape: the hall changes between them).
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum ColossusStage {
    /// Stage one: it guards the hall's far end, slamming its fists.
    #[default]
    Guard,
    /// Stage two: the floor gave way; it stands in the crypt below, sweeping its fists while
    /// the ceiling sheds stones.
    Fallen,
    /// Stage three: the vault came down and its armour with it; the core hovers, unbound.
    Unbound,
}

impl ColossusStage {
    pub const ALL: [Self; 3] = [Self::Guard, Self::Fallen, Self::Unbound];

    /// The stage after this one, if any.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Guard => Some(Self::Fallen),
            Self::Fallen => Some(Self::Unbound),
            Self::Unbound => None,
        }
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Guard => "The Guard",
            Self::Fallen => "The Fall",
            Self::Unbound => "Unbound",
        }
    }

    /// Health share (per mille) at which the stage ends and the hall collapses into the next.
    #[must_use]
    pub const fn ends_below(self) -> u16 {
        match self {
            Self::Guard => 660,
            Self::Fallen => 330,
            Self::Unbound => 0,
        }
    }

    /// Health share (per mille) the stage starts at.
    #[must_use]
    pub const fn starts_at(self) -> u16 {
        match self {
            Self::Guard => 1_000,
            Self::Fallen => 660,
            Self::Unbound => 330,
        }
    }
}

/// How long the hall's collapse is telegraphed (the Colossus cannot attack or be hurt).
pub const COLLAPSE_TELL_MILLIS: u64 = 4_000;
/// How long the Colossus waits with nobody in its hall before it sleeps (and heals to its
/// stage's top).
pub const COLOSSUS_SLEEP_MILLIS: u64 = 30_000;
/// A fist's tell before it strikes (stage three's is quicker).
pub const FIST_TELL_MILLIS: [u64; 3] = [1_300, 1_400, 1_000];
/// A slam's fall, a sweep's run.
pub const FIST_STRIKE_MILLIS: [u64; 3] = [260, 1_200, 240];
/// How long a fist rests after striking (it takes full damage while it rests).
pub const FIST_REST_MILLIS: u64 = 1_400;
/// Time between fist attacks per stage.
pub const FIST_PERIOD_MILLIS: [u64; 3] = [4_200, 4_600, 3_200];
/// A slam's shockwave: `(half width tiles, height tiles, damage)`.
pub const SLAM_WAVE: (f32, f32, f32) = (2.4, 2.0, 24.0);
/// A sweeping fist's damage to a rig it touches.
pub const SWEEP_DAMAGE: f32 = 22.0;
/// Stones the hall's ceiling sheds per volley in stage two, and how often.
pub const RUBBLE_VOLLEY: (usize, u64) = (3, 3_600);
/// How long falling stones are telegraphed by dust.
pub const RUBBLE_TELL_MILLIS: u64 = 1_300;
/// A falling stone's damage.
pub const RUBBLE_DAMAGE: f32 = 14.0;
/// Stage three's adds: at most this many war sentries, one pair every so often.
pub const COLOSSUS_ADDS: (usize, u64) = (2, 14_000);
/// How far a fist reaches from the Colossus's centre, tiles.
pub const FIST_REACH_TILES: i32 = 16;

/// One of the Colossus's parts (the hit areas round its core, each drawn where it is).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ColossusPart {
    /// The head with its single eye: the weak point.
    Head,
    /// The left fist.
    LeftFist,
    /// The right fist.
    RightFist,
    /// The hips and legs (gone once it is unbound).
    Legs,
}

impl ColossusPart {
    pub const ALL: [Self; 4] = [Self::Head, Self::LeftFist, Self::RightFist, Self::Legs];

    /// Its index (an `EnemyState::pattern` on its entity).
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Head),
            1 => Some(Self::LeftFist),
            2 => Some(Self::RightFist),
            3 => Some(Self::Legs),
            _ => None,
        }
    }

    /// Its hit circle's radius, pixels (its drawn half size; `MAX_HIT_RADIUS` bounds it).
    #[must_use]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Head => 33.0,
            Self::LeftFist | Self::RightFist => 30.0,
            Self::Legs => 45.0,
        }
    }

    /// Where it sits at rest from the core's centre, pixels.
    #[must_use]
    pub const fn rest_offset(self) -> (f32, f32) {
        match self {
            Self::Head => (0.0, -78.0),
            Self::LeftFist => (-87.0, 21.0),
            Self::RightFist => (87.0, 21.0),
            Self::Legs => (0.0, 87.0),
        }
    }

    /// The share of a hit on it the core takes (the head is the weak point; fists are armour,
    /// except while they rest after a strike).
    #[must_use]
    pub const fn damage_share(self, resting: bool) -> f32 {
        match self {
            Self::Head => 1.6,
            Self::LeftFist | Self::RightFist => {
                if resting {
                    1.0
                } else {
                    0.4
                }
            }
            Self::Legs => 0.7,
        }
    }

    /// Whether it is a fist (`Some(hand)`, 0 left, 1 right).
    #[must_use]
    pub const fn hand(self) -> Option<u8> {
        match self {
            Self::LeftFist => Some(0),
            Self::RightFist => Some(1),
            _ => None,
        }
    }
}

/// The core's hit circle, pixels (the torso as drawn).
pub const COLOSSUS_CORE_RADIUS: f32 = 48.0;
/// The share of a hit the unbound core takes (its armour is gone).
pub const UNBOUND_CORE_SHARE: f32 = 1.3;
/// Pixels from the core's centre down to its feet (the legs' bottom) while it stands.
pub const COLOSSUS_STANDING_DROP: f32 = 130.0;

/// What a fist is doing.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FistMoveKind {
    /// Raised over a column, then slammed down onto the floor there.
    Slam,
    /// Lowered to one end of the crypt's floor, then swept along it.
    Sweep,
}

/// A fist attack under way.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FistMove {
    pub kind: FistMoveKind,
    /// 0 left fist, 1 right fist.
    pub hand: u8,
    /// The tile it strikes (a slam), or the tile a sweep starts from.
    pub from: TilePosition,
    /// Where a sweep ends (a slam: the same as `from`).
    pub to: TilePosition,
    /// The tell ends (it strikes).
    pub strikes: SimulationTick,
    /// The strike lands (a slam's impact, a sweep's end).
    pub lands: SimulationTick,
    /// The rest ends (the fist goes back).
    pub rests: SimulationTick,
    /// Whether the slam's impact was dealt.
    #[serde(default)]
    pub landed: bool,
}

/// A stone falling from a ceiling (tremors, the Colossus's crypt): dust shows where until it
/// drops.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct RubbleDrop {
    pub lands: SimulationTick,
    /// The open tile under the ceiling it falls from.
    pub x: i32,
    pub y: i32,
}

/// The Gatekeeper Colossus's fight.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ColossusState {
    /// Whether it has fallen (the Heart Key is the crew's).
    #[serde(default)]
    pub beaten: bool,
    /// Its core while it is awake.
    #[serde(default)]
    pub entity: Option<u64>,
    /// Its parts while it is awake, by [`ColossusPart`] order.
    #[serde(default)]
    pub parts: Vec<(ColossusPart, u64)>,
    /// The stage it fights in.
    #[serde(default)]
    pub stage: ColossusStage,
    /// The shape of the hall (it keeps its collapses through a sleep).
    #[serde(default)]
    pub arena: ColossusStage,
    /// A collapse under way: the stage it opens, and when the hall gives way.
    #[serde(default)]
    pub collapsing: Option<(ColossusStage, SimulationTick)>,
    #[serde(default)]
    pub fist: Option<FistMove>,
    #[serde(default)]
    pub next_fist: SimulationTick,
    /// The hand the last attack used.
    #[serde(default)]
    pub last_hand: u8,
    #[serde(default)]
    pub next_rubble: SimulationTick,
    /// Its war sentries alive (stage three).
    #[serde(default)]
    pub adds: Vec<u64>,
    #[serde(default)]
    pub next_adds: SimulationTick,
    /// The last tick a rig was in its hall.
    #[serde(default)]
    pub last_visited: SimulationTick,
}

impl ColossusState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub const fn awake(&self) -> bool {
        self.entity.is_some()
    }

    /// Whether the hall is collapsing (it cannot be hurt).
    #[must_use]
    pub const fn collapsing(&self) -> bool {
        self.collapsing.is_some()
    }
}

// --- Tremors (the Seal Gate) -----------------------------------------------------------------

/// Time between tremors at the Seal Gate's top and bottom.
pub const TREMOR_MILLIS: (u64, u64) = (42_000, 20_000);
/// How long a tremor's dust shows where its stones will fall.
pub const TREMOR_TELL_MILLIS: u64 = 1_400;
/// Stones a tremor drops round each rig at the top and the bottom.
pub const TREMOR_STONES: (usize, usize) = (2, 4);
/// How far above a rig a ceiling sheds stones, tiles.
pub const TREMOR_CEILING_TILES: i32 = 9;
/// How long a tremor shakes (presentation).
pub const TREMOR_SHAKE_MILLIS: u64 = 1_800;
/// A tremor stone's damage.
pub const TREMOR_STONE_DAMAGE: f32 = 10.0;

/// Time between tremors `along` per mille down the Seal Gate (gently, then pushed).
#[must_use]
pub fn tremor_period(along: u16) -> u64 {
    let (top, bottom) = TREMOR_MILLIS;
    let along = u64::from(along.min(1_000));
    top - (top - bottom) * along / 1_000
}

/// Stones a tremor drops round a rig `along` per mille down the Seal Gate.
#[must_use]
pub fn tremor_stones(along: u16) -> usize {
    let (top, bottom) = TREMOR_STONES;
    top + (bottom - top) * usize::from(along.min(1_000)) / 1_000
}

// --- Landmarks and their story ---------------------------------------------------------------

/// The places of Descent VII part B a crew can find (each found once, with its story beat).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum GarrisonLandmark {
    /// Sleeping Garrison: the Grand Bastion.
    GrandBastion,
    /// The Seal Gate: the court before the door, its six key altars.
    SealCourt,
    /// The Gatekeeper's hall behind the door.
    ColossusHall,
}

impl GarrisonLandmark {
    pub const ALL: [Self; 3] = [Self::GrandBastion, Self::SealCourt, Self::ColossusHall];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GrandBastion => "The Grand Bastion",
            Self::SealCourt => "The Seal Court",
            Self::ColossusHall => "The Gatekeeper's Hall",
        }
    }

    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::GrandBastion => Stratum::SleepingGarrison,
            Self::SealCourt | Self::ColossusHall => Stratum::TheSealGate,
        }
    }

    /// Its story beat: what the crew reads there.
    #[must_use]
    pub const fn beat(self) -> &'static str {
        match self {
            Self::GrandBastion => {
                "Racks of war machines asleep on their feet, rows on rows, each with a lamp that \
                 never went out. On the pylon, scratched in Amos's hand: 'They wake when you \
                 stand still. I did not stand still. Whoever comes down after us: keep moving, \
                 and look up.' Under it someone, later, has drawn a turret and a wall."
            }
            Self::SealCourt => {
                "Six altars, six sockets on a door three storeys high: one for every Descent \
                 above. The builders did not lock the Heart with a key. They locked it with the \
                 whole road down. Amos came here with none of them, and his sled tracks go \
                 straight to the door anyway."
            }
            Self::ColossusHall => {
                "The Gatekeeper was built to let its makers' own through. The Fathers' sled \
                 carried the cable mark, and it stood aside. You carry the keys of every \
                 Descent, and a claim must be tested. The floor is cracked from wall to wall: \
                 this hall was built to fall."
            }
        }
    }
}

// --- The world state -------------------------------------------------------------------------

/// Descent VII part B's shared state (see the module docs).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Garrison {
    /// Forts that have stirred, keyed by their hold pylon's tile.
    #[serde(default, with = "crate::pair_list")]
    pub forts: BTreeMap<TilePosition, FortState>,
    /// Built defences by id.
    #[serde(default)]
    pub defences: BTreeMap<u32, Defence>,
    #[serde(default)]
    pub next_defence: u32,
    #[serde(default)]
    pub seal: SealGateState,
    #[serde(default)]
    pub colossus: ColossusState,
    /// Stones about to fall (tremors and the Colossus's crypt).
    #[serde(default)]
    pub drops: Vec<RubbleDrop>,
    /// The last tremor (presentation: the shake) and the next.
    #[serde(default)]
    pub last_tremor: Option<SimulationTick>,
    #[serde(default)]
    pub next_tremor: SimulationTick,
    /// Landmarks the crew has found.
    #[serde(default)]
    pub found: BTreeSet<GarrisonLandmark>,
}

impl Garrison {
    /// Whether nothing has happened here (left out of the state hash then, so every older
    /// world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forts.is_empty()
            && self.defences.is_empty()
            && self.next_defence == 0
            && self.seal == SealGateState::default()
            && self.colossus.is_default()
            && self.drops.is_empty()
            && self.last_tremor.is_none()
            && self.next_tremor == SimulationTick::new(0)
            && self.found.is_empty()
    }

    /// The defence covering `tile`, if any.
    #[must_use]
    pub fn defence_at(&self, tile: TilePosition) -> Option<(u32, &Defence)> {
        self.defences
            .iter()
            .find(|(_, defence)| defence.covers(tile))
            .map(|(id, defence)| (*id, defence))
    }

    /// Forgets a leaving player (its defences stay: they stand where they were built).
    pub const fn forget(&mut self, _player: PlayerId) {}
}

/// Host bookkeeping of Descent VII part B: saved, never hashed or replicated.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct GarrisonSchedule {
    /// Rigs the Seal Gate refused, until they leave its reach (so it says why once).
    #[serde(default)]
    pub refused: BTreeSet<PlayerId>,
}

// --- Events ----------------------------------------------------------------------------------

/// What a [`crate::event::GameplayEventKind::Garrison`] event reports (netcode codes
/// `650 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum GarrisonEvent {
    /// A rig walked into a fort: it stirs (the build window starts).
    FortStirred,
    /// A wave's bays glow.
    WaveTelegraphed,
    /// A wave climbed out of its bays.
    WaveWoke,
    /// A wave was beaten.
    WaveCleared,
    /// The last wave fell: the garrison sleeps and its armoury pays out (per rig).
    GarrisonSlept,
    /// Nobody held the fort: it stood down (its waves start over).
    StoodDown,
    /// A turret fired.
    TurretFired,
    /// A shock trap zapped something.
    TrapSprung,
    /// A defence was battered down.
    DefenceDestroyed,
    /// A tremor shook the Seal Gate.
    Tremor,
    /// A socket on the Seal Gate lit.
    SocketLit,
    /// The Seal Gate began to open.
    SealOpening,
    /// The Seal Gate refused a rig (a key is missing).
    SealRefused,
    /// The Gatekeeper Colossus woke.
    ColossusAwoke,
    /// The hall began to collapse (the telegraph).
    HallCollapsing,
    /// The hall gave way: a new stage.
    HallCollapsed,
    /// A fist slammed down.
    FistSlam,
    /// The Colossus fell.
    ColossusDefeated,
    /// The gate's reward (the Heart Key) was the rig's.
    GateRewards,
    /// A landmark was found (its story beat).
    LandmarkFound,
}

impl GarrisonEvent {
    /// Offset of the event's netcode code from 650.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// Descent VII part B's authored content in `stratum` for the descent probe: `(mechanics, set
/// pieces, the new things' names)`.
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8, &'static [&'static str]) {
    match stratum {
        Stratum::SleepingGarrison => (
            2,
            2,
            &[
                "waking garrisons",
                "base defence: turrets, barricades, traps",
                "garrison forts",
                "the Grand Bastion",
            ],
        ),
        Stratum::TheSealGate => (
            2,
            2,
            &[
                "tremors",
                "the Seal Gate's sockets",
                "the Seal Court",
                "the Gatekeeper Colossus",
            ],
        ),
        _ => (0, 0, &[]),
    }
}

/// Pixels a tile's centre lies from its top-left (helpers' shorthand).
pub const HALF_TILE: f32 = TILE_SIZE * 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_grow_and_mix_and_crews_get_more() {
        let first = wave_roster(FortKind::Barracks, 1, 1, 0);
        let last = wave_roster(FortKind::Barracks, 3, 1, 0);
        assert!(last.len() > first.len());
        assert!(last.contains(&EnemyKind::SiegeWalker));
        assert!(!first.contains(&EnemyKind::SiegeWalker));
        let crew = wave_roster(FortKind::Barracks, 3, 3, 0);
        assert!(crew.len() > last.len());
        assert!(crew.len() <= MAX_FORT_MACHINES);
        let deep = wave_roster(FortKind::Barracks, 2, 1, 1_000);
        assert!(deep.len() > wave_roster(FortKind::Barracks, 2, 1, 0).len());
        assert!(
            wave_roster(FortKind::Bastion, 5, 1, 0).len()
                >= wave_roster(FortKind::Barracks, 3, 1, 0).len()
        );
    }

    #[test]
    fn the_gate_lights_its_sockets_then_sinks_its_door() {
        assert_eq!(sockets_lit(0), 1);
        assert_eq!(sockets_lit(SOCKET_MILLIS * 10), SealKey::ALL.len());
        assert_eq!(door_rows_open(0, 8), 0);
        let lit = SOCKET_MILLIS * 6;
        assert_eq!(door_rows_open(lit, 8), 1);
        assert_eq!(door_rows_open(lit + DOOR_ROW_MILLIS * 20, 8), 8);
    }

    #[test]
    fn tremors_come_more_often_and_harder_deeper() {
        assert!(tremor_period(0) > tremor_period(1_000));
        assert!(tremor_stones(1_000) > tremor_stones(0));
    }

    #[test]
    fn the_stages_run_in_order_and_split_its_health() {
        let mut stage = ColossusStage::Guard;
        let mut seen = vec![stage];
        while let Some(next) = stage.next() {
            assert_eq!(next.starts_at(), stage.ends_below());
            stage = next;
            seen.push(stage);
        }
        assert_eq!(seen, ColossusStage::ALL.to_vec());
        for part in ColossusPart::ALL {
            assert_eq!(ColossusPart::from_index(part.index()), Some(part));
            assert!(part.radius() <= TILE_SIZE * 1.5);
        }
    }
}
