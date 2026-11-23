//! The Drowned and Frozen (expansion plan §14, Descent IV; Wave 9): water and ice everywhere,
//! and new movement rules for each of them.
//!
//! Pure data and math; the tick lives in `world/drowned.rs`, the creatures' behaviour in
//! `world/enemies/drowned.rs`, the generated places in [`crate::worldgen::descent::drowned`].
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | The Underground Ocean | the rig's **submarine mode**: ballast trims it to rise, sink or hover ([`RigWater::ballast`]); **currents** circle every basin, weak near the top and strong at the bottom ([`current_at`]); the scanner becomes a **sonar ping** that lights the water for a moment | trim, then ride the gyres; ping before you dive |
//! | Brine Glaciers | **ice physics** (rigs slide on brine ice, pack ice and rime: little traction, almost no braking), **frozen-over tunnels** (a tunnel dug through the ice refreezes as thin rime, faster deeper) and **hull frost** ([`RigWater::frost_micro`]: a cold meter that slows the rig until it is warmed) | thermal vents are warm safe spots; the Cryo Hull shrugs the frost off |
//! | Geyser Fields | **geysers**: each vent erupts on its own beat ([`geyser_phase`]), steam telegraphed by a gurgle; a rig in a chimney is thrown upward (an elevator), one sitting on the throat is scalded; the great geyser, Old Faithful, throws a rig up a 60-row shaft | time the beat, ride the column, never park on a vent |
//! | Merrow City | a **friendly city** of fish-folk in an air bubble: residents with routines, dialogue and trades, a shop, the Descent IV outpost and an aquarium furniture line | (a safe harbour) |
//! | Pearl Abyss | **crushing pressure** ([`abyss_pressure`]) against the Pressure Rating's seals ([`pressure_capacity`]): past them the hull groans and cracks; the deeper, the darker and greyer the screen; pearl clams gape and bite on a beat ([`clam_open`]) | the Abyssal Seals; drill clams while they gape |
//! | Gate | the **Leviathan**: a giant sea beast fought from the submarine, three phases (circling charges, the maelstrom, thrashing) | dodge the telegraphed charges, hit the open maw; rewards the Sonar Suite and the Cryo Hull |
//!
//! State: [`RigWater`] rides each rig's per-tick state (predicted and reconciled like the
//! pose), and [`DrownedState`] is shared (saved, replicated as the appended `Drowned` section
//! and hashed once anything happened: sonar pings, ink clouds, the eel-dragon, the Leviathan's
//! fight and the lore the crew found). Frozen-over tunnels reuse the Living Dark's regrowth
//! schedule ([`crate::living_dark::Regrowth::Rime`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    descent::Stratum,
    ids::{PlayerId, SimulationTick},
    worldgen::{WorldSettings, noise::hash2},
};

/// Whether `settings` has the Drowned and Frozen (a Deep Descent world).
#[must_use]
pub const fn drowned_rules(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// The name of Merrow City and its settlement.
pub const MERROW_CITY_NAME: &str = "Merrow City";
/// Merrow City's village site index: one past the last site a world places villages at, so
/// no other settlement shares its id.
pub const MERROW_SITE: i32 = crate::village::placement::MAX_SITE + 1;

/// Whether `stratum` is one of the Drowned and Frozen's.
#[must_use]
pub const fn is_drowned(stratum: Stratum) -> bool {
    matches!(
        stratum,
        Stratum::UndergroundOcean
            | Stratum::BrineGlaciers
            | Stratum::GeyserFields
            | Stratum::MerrowCity
            | Stratum::PearlAbyss
    )
}

// --- The rig in the water (per tick) --------------------------------------------------------

/// A rig's water state: ballast trim and hull frost (Wave 9). Part of the rig's per-tick state,
/// so it is predicted and reconciled like the pose.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct RigWater {
    /// Ballast trim, per mille: `+1000` tanks blown (rises), `-1000` flooded (sinks), `0`
    /// neutral (hovers). Thrust blows the tanks, the drill held floods them; let go and the
    /// trim holds.
    #[serde(default)]
    pub ballast: i16,
    /// Hull frost in millionths of a fully frozen hull (fine enough that a slow freeze still
    /// moves every tick at any tick rate; [`Self::frost_permille`] reads it): the colder, the
    /// slower the rig.
    #[serde(default)]
    pub frost_micro: u32,
}

/// Millionths of a full frost meter per per-mille step.
pub const FROST_MICRO_PER_PERMILLE: u32 = 1_000;

impl RigWater {
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.ballast == 0 && self.frost_micro == 0
    }

    /// Hull frost, per mille ([`FROST_MAX`]).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "at most 1000")]
    pub const fn frost_permille(&self) -> u16 {
        let permille = self.frost_micro / FROST_MICRO_PER_PERMILLE;
        if permille > FROST_MAX as u32 {
            FROST_MAX
        } else {
            permille as u16
        }
    }

    /// Changes the frost by `per_second` per mille a second over `seconds`, clamped.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "clamped to the meter's range, under 2^24"
    )]
    pub fn add_frost(&mut self, per_second: f32, seconds: f32) {
        let delta = (per_second * seconds * FROST_MICRO_PER_PERMILLE as f32).round();
        let max = f32::from(FROST_MAX) * FROST_MICRO_PER_PERMILLE as f32;
        #[allow(clippy::cast_precision_loss, reason = "under 2^24")]
        let next = (self.frost_micro as f32 + delta).clamp(0.0, max);
        self.frost_micro = next as u32;
    }
}

/// Full ballast trim either way.
pub const BALLAST_FULL: i16 = 1_000;
/// How fast the trim moves while thrust or the drill is held, per mille a second.
pub const BALLAST_RATE_PER_SECOND: f32 = 1_400.0;
/// Upward push of fully blown tanks (and downward of flooded ones), pixels per second squared.
pub const BALLAST_ACCELERATION: f32 = 300.0;

// --- Hull frost -----------------------------------------------------------------------------

/// A fully frozen hull.
pub const FROST_MAX: u16 = 1_000;
/// Frost a second in the Brine Glaciers' open air at its top, and at its bottom.
pub const FROST_RATE_TOP: f32 = 12.0;
pub const FROST_RATE_BOTTOM: f32 = 34.0;
/// Brine on the hull freezes it this many times faster.
pub const FROST_BRINE_FACTOR: f32 = 2.0;
/// Frost a thermal vent thaws a second (a rig within [`VENT_WARMTH_TILES`] of one).
pub const FROST_THAW_VENT: f32 = 260.0;
/// Frost thawing a second anywhere outside the glaciers.
pub const FROST_THAW_WARM: f32 = 70.0;
/// Frost thawing a second at an outpost or in a settlement.
pub const FROST_THAW_HARBOUR: f32 = 400.0;
/// A thermal vent warms rigs this close, tiles.
pub const VENT_WARMTH_TILES: i32 = 4;
/// Share of its speed a fully frozen rig loses.
pub const FROST_SLOW_MAX: f32 = 0.55;
/// Frost at which the crew is warned.
pub const FROST_WARNING: u16 = 600;

/// How much of a push still takes on a hull with `frost` (`1` unfrozen).
#[must_use]
pub fn frost_speed_factor(frost: u16) -> f32 {
    f32::from(frost.min(FROST_MAX)).mul_add(-FROST_SLOW_MAX / f32::from(FROST_MAX), 1.0)
}

/// Frost a second at `along` per mille down the Brine Glaciers.
#[must_use]
pub fn frost_rate(along: u16) -> f32 {
    (FROST_RATE_BOTTOM - FROST_RATE_TOP)
        .mul_add(f32::from(along.min(1_000)) / 1_000.0, FROST_RATE_TOP)
}

// --- Ice ------------------------------------------------------------------------------------

/// Share of the drive's push that takes on ice.
pub const ICE_TRACTION: f32 = 0.3;
/// How hard ice brakes a sliding rig, per second (slate brakes far harder).
pub const ICE_DRAG_PER_SECOND: f32 = 0.18;
/// How long a tunnel dug through the glaciers takes to refreeze at the top (halved by the
/// bottom).
pub const REFREEZE_MILLIS: u64 = 70_000;

/// Whether rigs slide on `kind`.
#[must_use]
pub const fn is_ice(kind: crate::terrain::TileKind) -> bool {
    use crate::{cave::CaveTile, terrain::TileKind};
    matches!(
        kind,
        TileKind::Cave(CaveTile::BrineIce | CaveTile::PackIce | CaveTile::Rime)
    )
}

/// How long a tunnel tile dug at `along` per mille down the Brine Glaciers takes to refreeze.
#[must_use]
pub const fn refreeze_millis(along: u16) -> u64 {
    REFREEZE_MILLIS - REFREEZE_MILLIS * (along as u64) / 2_000
}

// --- Currents -------------------------------------------------------------------------------

/// A basin's current at its rim at the Underground Ocean's top, and at its bottom, pixels a
/// second (a "Current Gyres" world runs half again faster).
pub const CURRENT_SPEED_TOP: f32 = 34.0;
pub const CURRENT_SPEED_BOTTOM: f32 = 118.0;
/// Share of the current's speed a fully submerged rig drifts at (the sea carries it even while
/// its drive brakes).
pub const CURRENT_DRIFT: f32 = 0.8;

/// The gyre's speed at `along` per mille down the ocean.
#[must_use]
pub fn current_speed(along: u16, gyres: bool) -> f32 {
    let base = (CURRENT_SPEED_BOTTOM - CURRENT_SPEED_TOP)
        .mul_add(f32::from(along.min(1_000)) / 1_000.0, CURRENT_SPEED_TOP);
    if gyres { base * 1.5 } else { base }
}

/// The current of a gyre at `point`, pixels a second.
///
/// The gyre circles `centre` (pixels) with radii `radii`: tangent to the basin's ellipse,
/// strongest two thirds of the way out, `clockwise` or not, `speed` at its strongest.
#[must_use]
pub fn gyre_velocity(
    centre: (f32, f32),
    radii: (f32, f32),
    point: (f32, f32),
    clockwise: bool,
    speed: f32,
) -> (f32, f32) {
    let dx = (point.0 - centre.0) / radii.0.max(1.0);
    let dy = (point.1 - centre.1) / radii.1.max(1.0);
    let r = dx.hypot(dy);
    if !(0.05..1.0).contains(&r) {
        return (0.0, 0.0);
    }
    // Rises to its peak at 2/3 of the radius, falls to nothing at the rim.
    let profile = if r < 0.66 { r / 0.66 } else { (1.0 - r) / 0.34 };
    let (tx, ty) = if clockwise { (-dy, dx) } else { (dy, -dx) };
    let length = tx.hypot(ty).max(1e-3);
    (
        tx / length * speed * profile * radii.0 / radii.0.max(radii.1),
        ty / length * speed * profile * radii.1 / radii.0.max(radii.1),
    )
}

// --- Geysers --------------------------------------------------------------------------------

/// A vent's beat: its period is this plus up to [`GEYSER_PERIOD_JITTER_MILLIS`] (by column).
pub const GEYSER_PERIOD_MILLIS: u64 = 7_000;
pub const GEYSER_PERIOD_JITTER_MILLIS: u64 = 3_000;
/// Old Faithful's slower, mightier beat.
pub const OLD_FAITHFUL_PERIOD_MILLIS: u64 = 16_000;
/// The gurgle (steam wisps, a rumble) before each eruption.
pub const GEYSER_TELL_MILLIS: u64 = 1_500;
/// How long an eruption blows.
pub const GEYSER_BLAST_MILLIS: u64 = 1_800;
/// The jet's upward speed: a rig in the column rises at least this fast, pixels a second.
pub const GEYSER_LIFT_SPEED: f32 = 560.0;
/// The tallest a jet reaches, rows (a chimney's ceiling stops it sooner).
pub const GEYSER_MAX_ROWS: i32 = 80;
/// Rows over the throat where the steam scalds instead of lifting.
pub const GEYSER_SCALD_ROWS: i32 = 2;
/// Hull a second the throat scalds (before the depth's creature scale).
pub const GEYSER_SCALD_DAMAGE_PER_SECOND: f32 = 4.0;

/// A geyser's phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum GeyserPhase {
    Quiet,
    /// The tell: steam wisps and a rumble before it blows.
    Gurgling,
    /// Erupting.
    Erupting,
}

const SALT_GEYSER: u64 = 0x9E15_0001;

/// A vent at `column`'s period (`great`: Old Faithful's).
#[must_use]
pub const fn geyser_period(seed: u64, column: i32, great: bool) -> u64 {
    if great {
        OLD_FAITHFUL_PERIOD_MILLIS
    } else {
        GEYSER_PERIOD_MILLIS + hash2(seed, SALT_GEYSER, column, 0) % GEYSER_PERIOD_JITTER_MILLIS
    }
}

/// The phase of the vent at `column` at `millis` of world time (tick-rate safe: a pure
/// function of world time).
#[must_use]
pub const fn geyser_phase(seed: u64, column: i32, great: bool, millis: u64) -> GeyserPhase {
    let period = geyser_period(seed, column, great);
    let offset = hash2(seed, SALT_GEYSER, column, 1) % period;
    let t = (millis + offset) % period;
    let blast_start = period - GEYSER_BLAST_MILLIS;
    if t >= blast_start {
        GeyserPhase::Erupting
    } else if t >= blast_start - GEYSER_TELL_MILLIS {
        GeyserPhase::Gurgling
    } else {
        GeyserPhase::Quiet
    }
}

// --- Pressure -------------------------------------------------------------------------------

/// The Pearl Abyss's pressure at `along` per mille down it, `depth_rows` under a water surface
/// (0 out of water), per mille: it grows with the trench and with the water over the hull.
#[must_use]
pub const fn abyss_pressure(along: u16, depth_rows: i32) -> u16 {
    let along = if along > 1_000 { 1_000 } else { along };
    let depth = if depth_rows < 0 {
        0
    } else if depth_rows > 120 {
        120
    } else {
        depth_rows
    };
    let raw = 280 + along as i32 * 6 / 10 + depth * 3;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "between 280 and 1000"
    )]
    if raw > 1_000 { 1_000 } else { raw as u16 }
}

/// The pressure a rig's seals hold, per mille: its Pressure Rating (tier 3, the Abyssal Seals,
/// is rated for the Drowned and Frozen) and the Cryo Hull.
#[must_use]
pub const fn pressure_capacity(rating: u8, cryo: bool) -> u16 {
    if cryo {
        return 1_000;
    }
    match rating {
        0 | 1 => 420,
        2 => 560,
        3 => 760,
        _ => 1_000,
    }
}

/// Hull a second per hundred per mille of pressure past the seals (before the depth's creature
/// scale; felt on a dip, ruinous to stay in).
pub const PRESSURE_DAMAGE_PER_SECOND: f32 = 1.6;
/// Pressure past the seals at which the crew is warned, per mille.
pub const PRESSURE_WARNING_OVER: u16 = 1;

// --- Pearl clams ----------------------------------------------------------------------------

/// A clam's beat: it gapes for [`CLAM_OPEN_MILLIS`] of every period.
pub const CLAM_PERIOD_MILLIS: u64 = 6_000;
pub const CLAM_OPEN_MILLIS: u64 = 2_600;
/// Hull a shut clam bites off a rig that drills it (before the creature scale).
pub const CLAM_BITE_DAMAGE: f32 = 5.0;
/// Credits an Abyss Pearl's clam pays besides the pearl itself.
pub const CLAM_PEARL_CREDITS: u32 = 900;

const SALT_CLAM: u64 = 0x9E15_0002;

/// Whether the clam at `(x, y)` gapes at `millis` of world time.
#[must_use]
pub const fn clam_open(seed: u64, x: i32, y: i32, millis: u64) -> bool {
    let offset = hash2(seed, SALT_CLAM, x, y) % CLAM_PERIOD_MILLIS;
    (millis + offset) % CLAM_PERIOD_MILLIS < CLAM_OPEN_MILLIS
}

// --- Sonar ----------------------------------------------------------------------------------

/// How long a sonar ping lights the water.
pub const SONAR_PING_MILLIS: u64 = 2_600;
/// A ping's reach, tiles (the Sonar Suite's echo mapping reaches [`SONAR_SUITE_REACH_TILES`]).
pub const SONAR_REACH_TILES: i32 = 12;
pub const SONAR_SUITE_REACH_TILES: i32 = 26;
/// How fast the ping's ring spreads, tiles a second.
pub const SONAR_RING_TILES_PER_SECOND: f32 = 16.0;
/// Most pings remembered at once (the oldest fades first).
pub const MAX_SONAR_PINGS: usize = 8;

/// One sonar ping: who sent it, from which tile, when, and how far it reaches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SonarPing {
    pub player_id: PlayerId,
    pub x: i32,
    pub y: i32,
    pub tick: SimulationTick,
    pub reach: i32,
}

/// How brightly a ping sent `elapsed_millis` ago lights a point `distance_tiles` from it.
///
/// `0..=1`: its ring passes over the point and leaves an afterglow that fades by
/// [`SONAR_PING_MILLIS`]. The Echo Caves (Descent VI) read the same light.
#[must_use]
pub fn sonar_light(elapsed_millis: u64, distance_tiles: f32, reach: i32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a few thousand milliseconds")]
    let seconds = elapsed_millis as f32 / 1_000.0;
    #[allow(clippy::cast_precision_loss, reason = "a few dozen tiles")]
    let reach = reach as f32;
    if distance_tiles > reach || elapsed_millis >= SONAR_PING_MILLIS {
        return 0.0;
    }
    let ring = seconds * SONAR_RING_TILES_PER_SECOND;
    if distance_tiles > ring {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a few thousand milliseconds")]
    let fade = 1.0 - elapsed_millis as f32 / SONAR_PING_MILLIS as f32;
    let edge = (1.0 - (ring - distance_tiles) / 3.0).clamp(0.0, 1.0);
    let falloff = (distance_tiles / reach.max(1.0)).mul_add(-0.5, 1.0);
    (fade * falloff * edge.mul_add(0.6, 0.4)).clamp(0.0, 1.0)
}

// --- Ink ------------------------------------------------------------------------------------

/// How long an ink squid's cloud hangs in the water.
pub const INK_CLOUD_MILLIS: u64 = 6_000;
/// Half size of an ink cloud, `(columns, rows)`.
pub const INK_CLOUD_HALF: (i32, i32) = (3, 3);
/// Most ink clouds at once.
pub const MAX_INK_CLOUDS: usize = 6;

// --- The eel-dragon (the ocean's mini-boss) -------------------------------------------------

/// Time between the eel-dragon's electric pulses.
pub const EEL_PULSE_MILLIS: u64 = 7_000;
/// The crackle before a pulse.
pub const EEL_PULSE_TELL_MILLIS: u64 = 1_300;
/// How far a pulse reaches through water, tiles.
pub const EEL_PULSE_TILES: f32 = 5.0;
/// Hull a pulse takes (before the creature scale).
pub const EEL_PULSE_DAMAGE: f32 = 6.0;
/// The bounty on the eel-dragon.
pub const EEL_BOUNTY: u32 = 12_000;
/// A rig this close to the grotto wakes the eel-dragon, tiles.
pub const EEL_WAKE_TILES: i32 = 18;

/// The eel-dragon's state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EelDragonState {
    #[serde(default)]
    pub beaten: bool,
    #[serde(default)]
    pub entity: Option<u64>,
    /// The next pulse's tell starts at this tick.
    #[serde(default)]
    pub next_pulse: SimulationTick,
    /// A pulse being telegraphed lands at this tick.
    #[serde(default)]
    pub pulse_at: Option<SimulationTick>,
}

// --- The Leviathan --------------------------------------------------------------------------

/// The Leviathan's three phases, by its health.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum LeviathanPhase {
    /// Above two thirds of its health: it circles the arena and charges across it.
    #[default]
    Circling,
    /// It sinks to the floor and opens its maw: a whirlpool pulls rigs in, its open maw takes
    /// double damage, then it spits pressure bubbles.
    Maelstrom,
    /// Under a third: it thrashes, shaking the ceiling loose, charging faster, calling sharks.
    Thrashing,
}

impl LeviathanPhase {
    /// The phase at `health_fraction` of its health.
    #[must_use]
    pub fn of(health_fraction: f32) -> Self {
        if health_fraction > 2.0 / 3.0 {
            Self::Circling
        } else if health_fraction > 1.0 / 3.0 {
            Self::Maelstrom
        } else {
            Self::Thrashing
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Circling => "Circling",
            Self::Maelstrom => "The Maelstrom",
            Self::Thrashing => "Thrashing",
        }
    }
}

/// What the Leviathan is doing (its position is a pure function of its move and the world time
/// since the move began, so it swims the same at every tick rate).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum LeviathanMove {
    /// Swimming the arena's ellipse from [`LeviathanState::angle_milli`] (thousandths of a
    /// radian), [`LeviathanState::clockwise`] or not.
    #[default]
    Circle,
    /// The telegraph: it stops at `at`, its eyes flare, it roars, aiming at `target` (pixels
    /// from the arena's centre).
    ChargeTell { at: (i32, i32), target: (i32, i32) },
    /// The charge: a straight dash from `from` to `to` (pixels from the arena's centre).
    Charge { from: (i32, i32), to: (i32, i32) },
    /// Sinking from `from` to the arena's floor, maw opening.
    Sink { from: (i32, i32) },
    /// On the floor with its maw open: the whirlpool pulls.
    Maw,
    /// Spitting pressure bubbles from the floor.
    Spit,
    /// Rising from the floor back to its circle.
    Rise,
}

/// Charge telegraph and dash.
pub const LEVIATHAN_TELL_MILLIS: u64 = 1_400;
pub const LEVIATHAN_CHARGE_MILLIS: u64 = 2_000;
/// One lap of its circle.
pub const LEVIATHAN_LAP_MILLIS: u64 = 18_000;
/// Time circling between attacks (shorter while it thrashes).
pub const LEVIATHAN_ATTACK_GAP_MILLIS: u64 = 5_500;
/// Sinking, the open maw, spitting, rising.
pub const LEVIATHAN_SINK_MILLIS: u64 = 1_600;
pub const LEVIATHAN_MAW_MILLIS: u64 = 5_000;
pub const LEVIATHAN_SPIT_MILLIS: u64 = 1_400;
pub const LEVIATHAN_RISE_MILLIS: u64 = 1_600;
/// The maelstrom's pull toward the maw, pixels a second at full strength.
pub const MAELSTROM_PULL: f32 = 95.0;
/// Pressure bubbles it spits at each rig.
pub const LEVIATHAN_BUBBLES: u8 = 3;
/// Its open maw takes this many times the damage.
pub const MAW_DAMAGE_FACTOR: f32 = 2.0;
/// Sharks it calls once it thrashes.
pub const LEVIATHAN_SHARKS: usize = 2;
/// Boulders it shakes off the ceiling with each thrash.
pub const LEVIATHAN_ROCKFALL: i32 = 3;
/// With no rig in its arena for this long it sinks back to sleep and heals.
pub const LEVIATHAN_SLEEP_MILLIS: u64 = 30_000;

/// The Leviathan's fight.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LeviathanState {
    /// Beaten: the gate is open and its rewards handed out.
    #[serde(default)]
    pub beaten: bool,
    /// Its entity while awake.
    #[serde(default)]
    pub entity: Option<u64>,
    #[serde(default)]
    pub phase: LeviathanPhase,
    #[serde(default)]
    pub action: LeviathanMove,
    /// When the current move began.
    #[serde(default)]
    pub since: SimulationTick,
    /// Its angle on the circle when it last began circling (thousandths of a radian) and which
    /// way it swims.
    #[serde(default)]
    pub angle_milli: i32,
    #[serde(default)]
    pub clockwise: bool,
    /// Whether it called its sharks yet.
    #[serde(default)]
    pub sharks_called: bool,
    /// Attacks made since it woke (the maelstrom alternates charges and its maw).
    #[serde(default)]
    pub attacks: u8,
    /// The last tick a rig was in its arena.
    #[serde(default)]
    pub last_visited: SimulationTick,
}

impl LeviathanState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    #[must_use]
    pub const fn awake(&self) -> bool {
        self.entity.is_some()
    }

    /// Whether its maw is open (it takes [`MAW_DAMAGE_FACTOR`] times the damage).
    #[must_use]
    pub const fn maw_open(&self) -> bool {
        matches!(self.action, LeviathanMove::Maw | LeviathanMove::Spit)
    }
}

// --- Lore -----------------------------------------------------------------------------------

/// The Drowned and Frozen's story beats, one per stratum, each found at its landmark.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DrownedLore {
    /// The Underground Ocean: the drowned builder sub.
    DrownedSub,
    /// Brine Glaciers: the builder figures frozen in the ice.
    FrozenFigures,
    /// Geyser Fields: the Fathers' soaked tape at Old Faithful.
    OldFaithful,
    /// Merrow City: the merrow remember the Fathers, and Sefa.
    MerrowCity,
    /// Pearl Abyss: the Leviathan's fall; the floor is a ceiling.
    AbyssFloor,
}

impl DrownedLore {
    pub const ALL: [Self; 5] = [
        Self::DrownedSub,
        Self::FrozenFigures,
        Self::OldFaithful,
        Self::MerrowCity,
        Self::AbyssFloor,
    ];

    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::DrownedSub => Stratum::UndergroundOcean,
            Self::FrozenFigures => Stratum::BrineGlaciers,
            Self::OldFaithful => Stratum::GeyserFields,
            Self::MerrowCity => Stratum::MerrowCity,
            Self::AbyssFloor => Stratum::PearlAbyss,
        }
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::DrownedSub => "The drowned builder sub",
            Self::FrozenFigures => "Figures in the ice",
            Self::OldFaithful => "A soaked tape",
            Self::MerrowCity => "The merrow remember",
            Self::AbyssFloor => "The floor is a ceiling",
        }
    }

    /// What the crew finds.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::DrownedSub => {
                "A hull in the silt, longer than three rigs, its hatch facing the cable. Inside, \
                 seats for longer legs and a chart of this very sea, every basin drawn and \
                 every current arrowed. Scratched into the bulkhead, newer: F.L., and a raft."
            }
            Self::FrozenFigures => {
                "Tall shapes frozen in clear blue ice, all around the cable, all with a hand \
                 raised to it. Not statues: the builders themselves, or their suits, waiting. \
                 Someone has chipped a little hollow in the ice beside one and left a mitten."
            }
            Self::OldFaithful => {
                "Wedged in the sinter by the great geyser: a tape in a tin, soaked and dried a \
                 hundred times. Dov's voice, laughing: \"We rode it up! Bran says never again. \
                 Amos says again tomorrow.\""
            }
            Self::MerrowCity => {
                "The merrow knew you were coming: 'the cable-folk always come down in sixes.' \
                 One of yours stayed, they say, a woman who drew every fish and sang their \
                 songs back wrong until she sang them right. Sefa. Her drawings hang in the \
                 archive still."
            }
            Self::AbyssFloor => {
                "The Leviathan's fall cracks the trench floor, and through the crack comes \
                 light. Warm light, from below. The floor of the sea is the roof of something \
                 else: a sky, and a sun inside the world."
            }
        }
    }
}

// --- Events ---------------------------------------------------------------------------------

/// What a [`crate::event::GameplayEventKind::Drowned`] event reports (netcode codes
/// `520 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DrownedEvent {
    /// A rig's sonar pinged.
    SonarPing,
    /// A rig's hull frost crossed the warning line.
    FrostWarning,
    /// A tunnel through the glaciers froze over.
    TunnelRefroze,
    /// A geyser near a rig erupted.
    GeyserErupted,
    /// A geyser threw a rig upward.
    GeyserLaunched,
    /// A geyser's throat scalded a rig.
    GeyserScalded,
    /// A gaping clam gave up its pearl.
    ClamPearl,
    /// A shut clam bit the drill.
    ClamSnapped,
    /// The pressure passed a rig's seals.
    PressureWarning,
    /// An ink squid inked the water.
    InkCloud,
    /// The eel-dragon woke in its grotto.
    EelDragonAwoke,
    /// The eel-dragon's electric pulse began to crackle (it lands a moment later).
    EelDragonPulse,
    /// The eel-dragon fell.
    EelDragonDefeated,
    /// The Leviathan rose.
    LeviathanAwoke,
    /// The Leviathan entered a new phase.
    LeviathanPhase,
    /// The Leviathan roared: a charge is coming.
    LeviathanCharge,
    /// The Leviathan fell.
    LeviathanDefeated,
    /// The gate's rewards (the Sonar Suite and the Cryo Hull) were fitted to a rig.
    GateRewards,
    /// The crew found one of the Descent's story beats.
    LoreFound,
    /// A rig came into Merrow City.
    CityReached,
}

impl DrownedEvent {
    /// Offset of the event's netcode code from 520.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

// --- The world state ------------------------------------------------------------------------

/// The Drowned and Frozen's shared state (see the module docs).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DrownedState {
    /// Recent sonar pings (oldest first).
    #[serde(default)]
    pub pings: Vec<SonarPing>,
    /// Ink clouds in the water: their field zone ids and when each clears.
    #[serde(default)]
    pub inks: BTreeMap<u32, SimulationTick>,
    #[serde(default)]
    pub eel: EelDragonState,
    #[serde(default)]
    pub leviathan: LeviathanState,
    /// The story beats found ([`DrownedLore::bit`]).
    #[serde(default)]
    pub lore: u8,
}

impl DrownedState {
    /// Whether nothing has happened in the Drowned and Frozen (left out of the state hash then,
    /// so every older world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pings.is_empty()
            && self.inks.is_empty()
            && self.eel == EelDragonState::default()
            && self.leviathan.is_default()
            && self.lore == 0
    }

    /// Whether the crew found `lore`.
    #[must_use]
    pub const fn found(&self, lore: DrownedLore) -> bool {
        self.lore & lore.bit() != 0
    }

    /// Forgets a leaving player's pings.
    pub fn forget(&mut self, player: PlayerId) {
        self.pings.retain(|ping| ping.player_id != player);
    }
}

/// How many authored mechanics and set pieces `stratum` adds for the descent probe's count of
/// new things (Wave 9).
#[must_use]
#[allow(clippy::match_same_arms, reason = "one row per stratum")]
pub const fn authored_content(stratum: Stratum) -> (u8, u8) {
    match stratum {
        // Ballast, currents, sonar; the basins, the drowned sub and the eel-dragon.
        Stratum::UndergroundOcean => (3, 3),
        // Ice, refreezing tunnels, hull frost; thermal vents and the frozen figures.
        Stratum::BrineGlaciers => (3, 2),
        // Geysers; the chimneys and Old Faithful.
        Stratum::GeyserFields => (2, 2),
        // The city, its people, its shop and furniture.
        Stratum::MerrowCity => (1, 3),
        // Pressure, the darkening screen, pearl clams; the pearl beds and the Leviathan.
        Stratum::PearlAbyss => (3, 2),
        _ => (0, 0),
    }
}

/// Share of a route through `stratum` crossed by submarine in its authored basins, per mille
/// (Wave 9 integration: the descent probe's water share beyond the registry's pockets).
///
/// The Underground Ocean's basins and currents carry most of a crossing.
#[must_use]
pub const fn submarine_share_permille(stratum: Stratum) -> u16 {
    match stratum {
        Stratum::UndergroundOcean => 700,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_codes_fit_the_drowned_range() {
        assert!(DrownedEvent::CityReached.code() < 20);
    }

    #[test]
    fn geysers_keep_their_beat_in_world_time() {
        let seed = 7;
        let mut erupting = 0;
        let mut gurgling = 0;
        let period = geyser_period(seed, 12, false);
        for millis in 0..period {
            match geyser_phase(seed, 12, false, millis) {
                GeyserPhase::Erupting => erupting += 1,
                GeyserPhase::Gurgling => gurgling += 1,
                GeyserPhase::Quiet => {}
            }
        }
        assert_eq!(erupting, GEYSER_BLAST_MILLIS);
        assert_eq!(gurgling, GEYSER_TELL_MILLIS);
        // Old Faithful is slower.
        assert!(geyser_period(seed, 12, true) > period);
        // Every eruption is telegraphed: the moment before blowing is a gurgle.
        for millis in 1..period * 2 {
            if geyser_phase(seed, 12, false, millis) == GeyserPhase::Erupting
                && geyser_phase(seed, 12, false, millis - 1) != GeyserPhase::Erupting
            {
                assert_eq!(
                    geyser_phase(seed, 12, false, millis - 1),
                    GeyserPhase::Gurgling
                );
            }
        }
    }

    #[test]
    fn pressure_grows_down_the_trench_and_the_seals_hold_it() {
        assert!(abyss_pressure(0, 0) < abyss_pressure(1_000, 0));
        assert!(abyss_pressure(500, 0) < abyss_pressure(500, 30));
        assert!(abyss_pressure(1_000, 200) <= 1_000);
        // The Abyssal Seals hold the trench's top but not its deep water; the Cryo Hull all.
        assert!(pressure_capacity(3, false) >= abyss_pressure(200, 0));
        assert!(pressure_capacity(3, false) < abyss_pressure(1_000, 40));
        assert_eq!(pressure_capacity(0, true), 1_000);
    }

    #[test]
    fn frost_slows_but_never_stops_a_rig() {
        assert!((frost_speed_factor(0) - 1.0).abs() < 1e-6);
        assert!(frost_speed_factor(FROST_MAX) > 0.4);
        assert!(frost_rate(1_000) > frost_rate(0));
    }

    #[test]
    fn clams_gape_on_a_beat() {
        let open = (0..CLAM_PERIOD_MILLIS)
            .filter(|millis| clam_open(3, 10, 20, *millis))
            .count();
        assert_eq!(open as u64, CLAM_OPEN_MILLIS);
    }

    #[test]
    fn a_sonar_ring_spreads_then_fades() {
        assert!(
            sonar_light(100, 10.0, 12) <= 0.0,
            "the ring has not reached it yet"
        );
        assert!(sonar_light(800, 10.0, 12) > 0.0);
        assert!(sonar_light(SONAR_PING_MILLIS, 2.0, 12) <= 0.0);
        assert!(sonar_light(800, 13.0, 12) <= 0.0, "past its reach");
    }

    #[test]
    fn gyres_circle_and_vanish_at_the_rim_and_centre() {
        let (vx, vy) = gyre_velocity((0.0, 0.0), (100.0, 50.0), (0.0, -33.0), true, 60.0);
        assert!(vx.abs() > 10.0 && vy.abs() < 1.0, "{vx} {vy}");
        assert_eq!(
            gyre_velocity((0.0, 0.0), (100.0, 50.0), (0.0, -60.0), true, 60.0),
            (0.0, 0.0)
        );
        assert_eq!(
            gyre_velocity((0.0, 0.0), (100.0, 50.0), (0.0, 0.0), true, 60.0),
            (0.0, 0.0)
        );
    }

    #[test]
    fn the_leviathan_changes_phase_with_its_health() {
        assert_eq!(LeviathanPhase::of(1.0), LeviathanPhase::Circling);
        assert_eq!(LeviathanPhase::of(0.5), LeviathanPhase::Maelstrom);
        assert_eq!(LeviathanPhase::of(0.2), LeviathanPhase::Thrashing);
    }
}
