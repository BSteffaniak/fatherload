//! The world clock (Part II F4): days, the sun, the Moon's phase and sleeping through the night.
//!
//! Time of day is a pure function of the world tick, the session tick rate and a small saved
//! [`ClockState`] (the per-world [`DayLength`] setting and the time added by sleeping), so it is
//! deterministic, tick-rate safe, replicated almost for free and saved with the world:
//!
//! ```text
//! world_millis = tick * 1000 / tick_rate_hz + ClockState::offset_millis
//! ```
//!
//! A day is split into [`DAY_SLICES`] equal slices counted from midnight. With the default
//! 48-minute day one slice is 30 real seconds: night runs from slice [`NIGHT_START_SLICE`] (78)
//! round midnight to [`DAWN_START_SLICE`] (18), about 18 minutes; dawn lasts five slices
//! (2.5 minutes), full day 50 (25 minutes) and dusk five, so the sun is up for about 30 minutes.
//! The day counter ([`WorldClock::day`]) rolls over at midnight; new worlds start a little after
//! dawn on day 1 ([`NEW_WORLD_START_SLICE`]).
//!
//! Gameplay decisions use the integer readings ([`WorldClock::phase`], [`WorldClock::slice`],
//! [`WorldClock::light_level`], [`WorldClock::moon_phase`]); the float readings
//! ([`WorldClock::day_fraction`], [`WorldClock::sun_angle`], ...) are for presentation.
//!
//! **Sleeping.** A bed (the base bed module or a homestead, see [`crate::base`]) or a room at a
//! village inn puts a player in bed while it is dusk or night ([`DayPhase::sleepable`]). Once
//! every player in the world is in bed (or anyone is, under [`SleepRule::Anyone`], the host's
//! choice), a short [`SLEEP_FADE_MILLIS`] countdown runs and the clock jumps to the next dawn:
//! the jump is added to [`ClockState::offset_millis`], so it saves and replicates like
//! everything else. Only the world clock jumps; tick-based timers (fuel synthesis, blessings, the
//! market day) do not. **Systems whose progress should continue through a skipped night (for
//! example contractor construction, plan decision 3) must measure time with
//! [`WorldClock::world_millis`], not ticks.**

#![allow(
    clippy::cast_possible_truncation,
    reason = "clock readings are bounded: slices, permille, hours, a day count, and presentation \
              floats from exact integer times"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ids::{PlayerId, SimulationTick},
    tick::TickRate,
};

/// Equal slices a day is divided into, counted from midnight.
pub const DAY_SLICES: u32 = 96;
/// First slice of dawn (the sun touches the horizon).
pub const DAWN_START_SLICE: u32 = 18;
/// First slice of full day.
pub const DAY_START_SLICE: u32 = 23;
/// First slice of dusk.
pub const DUSK_START_SLICE: u32 = 73;
/// First slice of night (the sun is down).
pub const NIGHT_START_SLICE: u32 = 78;
/// Noon: the sun at the top of its arc.
pub const NOON_SLICE: u32 = 48;
/// Where a brand-new world's clock starts: 8 in the morning of day 1, in full daylight.
pub const NEW_WORLD_START_SLICE: u32 = 32;
/// Days from one new moon to the next.
pub const LUNAR_CYCLE_DAYS: u32 = 8;
/// Light level (permille) of a moonless night on the open surface.
pub const NIGHT_LIGHT: u16 = 110;
/// Extra night light (permille) a full moon adds.
pub const FULL_MOON_LIGHT: u16 = 90;
/// Full daylight (permille).
pub const DAY_LIGHT: u16 = 1_000;
/// How long everyone has to be in bed before the night is skipped (the screens fade out).
pub const SLEEP_FADE_MILLIS: u64 = 2_500;
/// Credits a room at a village inn costs for the night.
pub const INN_ROOM_PRICE: u32 = 30;

/// How long a full in-game day lasts in real time (a world setting chosen at creation).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum DayLength {
    /// 24 real minutes.
    Short,
    /// 48 real minutes (about 30 of daylight and 18 of night). The default, and what saves from
    /// before the clock load with.
    #[default]
    Standard,
    /// 72 real minutes.
    Long,
}

impl DayLength {
    pub const ALL: [Self; 3] = [Self::Short, Self::Standard, Self::Long];

    /// Real minutes per full day.
    #[must_use]
    pub const fn minutes(self) -> u64 {
        match self {
            Self::Short => 24,
            Self::Standard => 48,
            Self::Long => 72,
        }
    }

    /// Real milliseconds per full day.
    #[must_use]
    pub const fn millis(self) -> u64 {
        self.minutes() * 60 * 1_000
    }

    /// Milliseconds per slice ([`DAY_SLICES`] per day).
    #[must_use]
    pub const fn slice_millis(self) -> u64 {
        self.millis() / DAY_SLICES as u64
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Short => "24 min",
            Self::Standard => "48 min",
            Self::Long => "72 min",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Short => "Quick days: about 15 minutes of daylight, 9 of night",
            Self::Standard => "About 30 minutes of daylight and 18 of night",
            Self::Long => "Long days: about 45 minutes of daylight, 27 of night",
        }
    }
}

/// The part of the day.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum DayPhase {
    #[default]
    Night,
    Dawn,
    Day,
    Dusk,
}

impl DayPhase {
    /// The phase of day slice `slice` (`0..DAY_SLICES`, from midnight).
    #[must_use]
    pub const fn of_slice(slice: u32) -> Self {
        if slice < DAWN_START_SLICE || slice >= NIGHT_START_SLICE {
            Self::Night
        } else if slice < DAY_START_SLICE {
            Self::Dawn
        } else if slice < DUSK_START_SLICE {
            Self::Day
        } else {
            Self::Dusk
        }
    }

    /// Whether a bed lets you sleep now (dusk or night).
    #[must_use]
    pub const fn sleepable(self) -> bool {
        matches!(self, Self::Dusk | Self::Night)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Night => "Night",
            Self::Dawn => "Dawn",
            Self::Day => "Day",
            Self::Dusk => "Dusk",
        }
    }
}

/// The Moon's phase: how much of the face the sun lights, seen from the ground.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum MoonPhase {
    #[default]
    New,
    WaxingCrescent,
    FirstQuarter,
    WaxingGibbous,
    Full,
    WaningGibbous,
    LastQuarter,
    WaningCrescent,
}

impl MoonPhase {
    pub const ALL: [Self; 8] = [
        Self::New,
        Self::WaxingCrescent,
        Self::FirstQuarter,
        Self::WaxingGibbous,
        Self::Full,
        Self::WaningGibbous,
        Self::LastQuarter,
        Self::WaningCrescent,
    ];

    /// Lit share of the face in permille: `(1 - cos(phase angle)) / 2`.
    #[must_use]
    pub const fn illumination(self) -> u16 {
        match self {
            Self::New => 0,
            Self::WaxingCrescent | Self::WaningCrescent => 146,
            Self::FirstQuarter | Self::LastQuarter => 500,
            Self::WaxingGibbous | Self::WaningGibbous => 854,
            Self::Full => 1_000,
        }
    }

    /// Whether the lit part is growing (the right-hand side is lit, toward the sunset).
    #[must_use]
    pub const fn waxing(self) -> bool {
        matches!(
            self,
            Self::WaxingCrescent | Self::FirstQuarter | Self::WaxingGibbous
        )
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::New => "New moon",
            Self::WaxingCrescent => "Waxing crescent",
            Self::FirstQuarter => "First quarter",
            Self::WaxingGibbous => "Waxing gibbous",
            Self::Full => "Full moon",
            Self::WaningGibbous => "Waning gibbous",
            Self::LastQuarter => "Last quarter",
            Self::WaningCrescent => "Waning crescent",
        }
    }
}

/// Who has to be in bed before the night is skipped.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum SleepRule {
    /// Every player in the world (a vote: the last one to lie down ends the night).
    #[default]
    Everyone,
    /// Any one player in bed is enough (the host allows it).
    Anyone,
}

impl SleepRule {
    pub const ALL: [Self; 2] = [Self::Everyone, Self::Anyone];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Everyone => "Everyone in bed",
            Self::Anyone => "Anyone can sleep",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Everyone => Self::Anyone,
            Self::Anyone => Self::Everyone,
        }
    }
}

/// Where a player lies down.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BedSpot {
    /// A base bed module or homestead.
    Module(crate::base::StructureId),
    /// A rented room at a village inn.
    Inn(crate::village::VillageId),
    /// A bed in a contractor-built home (WP-HOME).
    Home(crate::home::PlotId),
}

/// A player in bed: where, and the rig's tile when they lay down (moving away gets them up).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Sleeper {
    pub bed: BedSpot,
    pub x: i32,
    pub y: i32,
}

/// Bed actions besides lying down (lying down is a bed's service: `ModuleService::Sleep` on a
/// bed or homestead, or `VillageDeal::Room` at an inn).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SleepCommand {
    /// Get out of bed (driving the rig away does it too).
    GetUp,
    /// Changes who must be in bed to skip the night (the game offers it to the host).
    SetRule(SleepRule),
}

/// The world clock's saved and replicated state (the shared `SectionKey::Clock` section).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClockState {
    /// The world's day length.
    #[serde(default)]
    pub day_length: DayLength,
    /// World time at tick 0 plus every skipped night, in milliseconds.
    #[serde(default = "default_offset_millis")]
    pub offset_millis: u64,
    /// Who must be in bed to skip the night.
    #[serde(default)]
    pub sleep_rule: SleepRule,
    /// Players in bed.
    #[serde(default)]
    pub sleepers: BTreeMap<PlayerId, Sleeper>,
    /// When everyone is in bed: the tick the night is skipped on.
    #[serde(default)]
    pub sleep_deadline: Option<SimulationTick>,
    /// The tick the last night was skipped on (the screens fade back in from it).
    #[serde(default)]
    pub last_skip: Option<SimulationTick>,
}

const fn default_offset_millis() -> u64 {
    start_offset_millis(DayLength::Standard)
}

/// World time at tick 0 for a new world with `day_length`: [`NEW_WORLD_START_SLICE`] of day 1.
#[must_use]
pub const fn start_offset_millis(day_length: DayLength) -> u64 {
    day_length.slice_millis() * NEW_WORLD_START_SLICE as u64
}

impl Default for ClockState {
    fn default() -> Self {
        Self::new(DayLength::Standard)
    }
}

impl ClockState {
    /// A new world's clock: early morning of day 1.
    #[must_use]
    pub const fn new(day_length: DayLength) -> Self {
        Self {
            day_length,
            offset_millis: start_offset_millis(day_length),
            sleep_rule: SleepRule::Everyone,
            sleepers: BTreeMap::new(),
            sleep_deadline: None,
            last_skip: None,
        }
    }

    /// Whether this is an untouched standard clock (worlds from before the clock hash as before).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A reading of the world clock at one tick (cheap, `Copy`; see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldClock {
    world_millis: u64,
    day_millis: u64,
    lunar_offset: u32,
}

impl WorldClock {
    /// The clock at `tick` of a session running at `rate`, with saved `state`; `seed` (the
    /// world seed) sets where in the lunar cycle day 1 falls.
    #[must_use]
    pub const fn at(tick: SimulationTick, rate: TickRate, state: &ClockState, seed: u64) -> Self {
        let tick_millis = tick.get().saturating_mul(1_000) / rate.hz() as u64;
        Self::from_world_millis(
            tick_millis.saturating_add(state.offset_millis),
            state.day_length,
            seed,
        )
    }

    /// The clock at an absolute world time (tests, scenes, previews).
    #[must_use]
    pub const fn from_world_millis(world_millis: u64, day_length: DayLength, seed: u64) -> Self {
        Self {
            world_millis,
            day_millis: day_length.millis(),
            lunar_offset: lunar_offset(seed),
        }
    }

    /// Milliseconds of world time since day 1 began (includes skipped nights). The time base for
    /// anything that should progress while the crew sleeps.
    #[must_use]
    pub const fn world_millis(self) -> u64 {
        self.world_millis
    }

    /// Real milliseconds per full day.
    #[must_use]
    pub const fn day_millis(self) -> u64 {
        self.day_millis
    }

    /// The day number, from 1. Rolls over at midnight.
    #[must_use]
    pub const fn day(self) -> u32 {
        let days = self.world_millis / self.day_millis;
        if days >= u32::MAX as u64 {
            u32::MAX
        } else {
            days as u32 + 1
        }
    }

    /// Milliseconds since midnight.
    #[must_use]
    pub const fn millis_into_day(self) -> u64 {
        self.world_millis % self.day_millis
    }

    /// The day slice (`0..DAY_SLICES`, from midnight).
    #[must_use]
    pub const fn slice(self) -> u32 {
        (self.millis_into_day() * DAY_SLICES as u64 / self.day_millis) as u32
    }

    /// Time of day in permille since midnight (`0..1000`; 500 is noon).
    #[must_use]
    pub const fn time_permille(self) -> u32 {
        (self.millis_into_day() * 1_000 / self.day_millis) as u32
    }

    /// Hours and minutes on a 24-hour face (midnight 00:00, noon 12:00), for display.
    #[must_use]
    pub const fn hours_minutes(self) -> (u8, u8) {
        let minutes = self.millis_into_day() * 24 * 60 / self.day_millis;
        ((minutes / 60) as u8, (minutes % 60) as u8)
    }

    #[must_use]
    pub const fn phase(self) -> DayPhase {
        DayPhase::of_slice(self.slice())
    }

    /// Whether it is night (the sun is down).
    #[must_use]
    pub const fn is_night(self) -> bool {
        matches!(self.phase(), DayPhase::Night)
    }

    /// How far through the current phase the clock is, in permille.
    #[must_use]
    pub const fn phase_progress(self) -> u32 {
        let slice_millis = self.day_millis / DAY_SLICES as u64;
        let now = self.millis_into_day();
        let (start, length) = match self.phase() {
            DayPhase::Dawn => (DAWN_START_SLICE, DAY_START_SLICE - DAWN_START_SLICE),
            DayPhase::Day => (DAY_START_SLICE, DUSK_START_SLICE - DAY_START_SLICE),
            DayPhase::Dusk => (DUSK_START_SLICE, NIGHT_START_SLICE - DUSK_START_SLICE),
            DayPhase::Night => (
                NIGHT_START_SLICE,
                DAY_SLICES - NIGHT_START_SLICE + DAWN_START_SLICE,
            ),
        };
        let start_millis = start as u64 * slice_millis;
        let into = if now >= start_millis {
            now - start_millis
        } else {
            now + self.day_millis - start_millis
        };
        let total = length as u64 * slice_millis;
        let permille = into * 1_000 / total;
        if permille > 1_000 {
            1_000
        } else {
            permille as u32
        }
    }

    /// Milliseconds from now until the next dawn begins (zero exactly at dawn's first instant).
    #[must_use]
    pub const fn millis_until_dawn(self) -> u64 {
        let dawn = DAWN_START_SLICE as u64 * self.day_millis / DAY_SLICES as u64;
        let now = self.millis_into_day();
        if now <= dawn {
            dawn - now
        } else {
            self.day_millis - now + dawn
        }
    }

    /// The Moon's phase today.
    #[must_use]
    pub const fn moon_phase(self) -> MoonPhase {
        let index = (self.day().wrapping_sub(1).wrapping_add(self.lunar_offset)) % LUNAR_CYCLE_DAYS;
        MoonPhase::ALL[index as usize]
    }

    /// Open-sky light level in permille: [`DAY_LIGHT`] by day, a moonlit [`NIGHT_LIGHT`] (plus up
    /// to [`FULL_MOON_LIGHT`]) at night, ramping linearly through dawn and dusk. Integer, so
    /// gameplay may branch on it.
    #[must_use]
    pub const fn light_level(self) -> u16 {
        let night = NIGHT_LIGHT
            + (FULL_MOON_LIGHT as u32 * self.moon_phase().illumination() as u32 / 1_000) as u16;
        let span = (DAY_LIGHT - night) as u32;
        match self.phase() {
            DayPhase::Day => DAY_LIGHT,
            DayPhase::Night => night,
            DayPhase::Dawn => night + (span * self.phase_progress() / 1_000) as u16,
            DayPhase::Dusk => DAY_LIGHT - (span * self.phase_progress() / 1_000) as u16,
        }
    }

    /// Time of day as a fraction (`0..1`, from midnight; presentation).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "presentation reading of an exact integer time"
    )]
    pub fn day_fraction(self) -> f32 {
        (self.millis_into_day() as f64 / self.day_millis as f64) as f32
    }

    /// Where the sun is on its circle, in radians (presentation): 0 rising on the eastern
    /// (left-hand) horizon at dawn's first instant, `PI / 2` at the top of the sky at noon, `PI`
    /// setting on the western (right-hand) horizon as night falls, and on round under the world
    /// through the night back to `TAU`.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "presentation reading of an exact integer time"
    )]
    pub fn sun_angle(self) -> f32 {
        let slices = self.millis_into_day() as f64 * f64::from(DAY_SLICES) / self.day_millis as f64;
        let up = f64::from(NIGHT_START_SLICE - DAWN_START_SLICE);
        let since_dawn = (slices - f64::from(DAWN_START_SLICE)).rem_euclid(f64::from(DAY_SLICES));
        let angle = if since_dawn <= up {
            since_dawn / up * std::f64::consts::PI
        } else {
            let down = f64::from(DAY_SLICES) - up;
            ((since_dawn - up) / down).mul_add(std::f64::consts::PI, std::f64::consts::PI)
        };
        angle as f32
    }

    /// The sun's height over the horizon (presentation): `sin(sun_angle)`, 1 at noon, negative at
    /// night.
    #[must_use]
    pub fn sun_height(self) -> f32 {
        self.sun_angle().sin()
    }

    /// Position in the lunar cycle (`0..1`: 0 new, 0.5 full; presentation, eases through the
    /// day so the terminator creeps smoothly).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "presentation reading of an exact integer time"
    )]
    pub fn moon_cycle(self) -> f32 {
        let day = f64::from(self.day().wrapping_sub(1).wrapping_add(self.lunar_offset));
        let millis = self.millis_into_day() as f64 / self.day_millis as f64;
        (((day + millis) / f64::from(LUNAR_CYCLE_DAYS)).fract()) as f32
    }
}

/// Days into the lunar cycle that day 1 of a world falls on.
const fn lunar_offset(seed: u64) -> u32 {
    ((seed ^ (seed >> 29) ^ 0x5DEE_CE66) % LUNAR_CYCLE_DAYS as u64) as u32
}

/// Where the Moon hangs: the column it sits above and the row of its underside.
///
/// The row is negative (rows above the ground). Plan WP-MOON places a real landmass there (track
/// T16/T22, world generator V8); until that generator supplies its own placement,
/// [`default_moon_anchor`] derives one from the seed, and the sky draws the Moon above it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MoonAnchor {
    /// Tile column under the Moon's centre.
    pub column: i32,
    /// Tile row of the Moon's underside (far above the ground: a negative row).
    pub row: i32,
}

/// Rows above the ground the default Moon's underside hangs at (about the highest band of the
/// plan's sky; T8 measured altitude precision there).
pub const DEFAULT_MOON_ROW: i32 = -100_000;

/// The seam for the Moon's placement (T16/T22): a deterministic column a few thousand tiles from
/// town, east or west by the seed, always the same Moon for a given seed.
#[must_use]
pub const fn default_moon_anchor(seed: u64) -> MoonAnchor {
    let mixed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17) ^ 0xA076_1D64_78BD_642F;
    let distance = 2_400 + (mixed % 2_400) as i32;
    let side = if (mixed >> 40) & 1 == 0 { 1 } else { -1 };
    MoonAnchor {
        column: crate::worldgen::SPAWN_TILE_X + side * distance,
        row: DEFAULT_MOON_ROW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at_slice(slice: u32) -> WorldClock {
        let day = DayLength::Standard;
        WorldClock::from_world_millis(u64::from(slice) * day.slice_millis(), day, 0)
    }

    #[test]
    fn phases_follow_the_slices() {
        assert_eq!(clock_at_slice(0).phase(), DayPhase::Night);
        assert_eq!(clock_at_slice(17).phase(), DayPhase::Night);
        assert_eq!(clock_at_slice(18).phase(), DayPhase::Dawn);
        assert_eq!(clock_at_slice(22).phase(), DayPhase::Dawn);
        assert_eq!(clock_at_slice(23).phase(), DayPhase::Day);
        assert_eq!(clock_at_slice(72).phase(), DayPhase::Day);
        assert_eq!(clock_at_slice(73).phase(), DayPhase::Dusk);
        assert_eq!(clock_at_slice(77).phase(), DayPhase::Dusk);
        assert_eq!(clock_at_slice(78).phase(), DayPhase::Night);
        assert_eq!(clock_at_slice(95).phase(), DayPhase::Night);
    }

    #[test]
    fn standard_day_is_thirty_minutes_light_and_eighteen_night() {
        let day = DayLength::Standard;
        let night_slices = DAY_SLICES - (NIGHT_START_SLICE - DAWN_START_SLICE);
        assert_eq!(u64::from(night_slices) * day.slice_millis(), 18 * 60_000);
        assert_eq!(
            u64::from(NIGHT_START_SLICE - DAWN_START_SLICE) * day.slice_millis(),
            30 * 60_000
        );
        assert_eq!(DayLength::Short.millis() * 3, DayLength::Long.millis());
    }

    #[test]
    fn days_roll_over_at_midnight_and_the_moon_cycles() {
        let day = DayLength::Short;
        let clock = WorldClock::from_world_millis(day.millis() - 1, day, 0);
        assert_eq!(clock.day(), 1);
        let next = WorldClock::from_world_millis(day.millis(), day, 0);
        assert_eq!(next.day(), 2);
        assert_eq!(next.hours_minutes(), (0, 0));
        let phases: Vec<MoonPhase> = (0..u64::from(LUNAR_CYCLE_DAYS))
            .map(|n| WorldClock::from_world_millis(n * day.millis(), day, 0).moon_phase())
            .collect();
        let mut sorted = phases;
        sorted.sort();
        assert_eq!(sorted, MoonPhase::ALL.to_vec(), "every phase once a cycle");
    }

    #[test]
    fn light_is_full_by_day_dim_at_night_and_ramps_between() {
        assert_eq!(clock_at_slice(48).light_level(), DAY_LIGHT);
        let night = clock_at_slice(0).light_level();
        assert!((NIGHT_LIGHT..=NIGHT_LIGHT + FULL_MOON_LIGHT).contains(&night));
        let mut last = 0;
        let day = DayLength::Standard;
        for step in 0..=10 {
            let millis = u64::from(DAWN_START_SLICE) * day.slice_millis()
                + step * u64::from(DAY_START_SLICE - DAWN_START_SLICE) * day.slice_millis() / 10;
            let level = WorldClock::from_world_millis(millis, day, 0).light_level();
            assert!(level >= last, "dawn brightens");
            last = level;
        }
    }

    #[test]
    fn sun_rises_left_peaks_at_noon_and_sets_right() {
        let rise = clock_at_slice(DAWN_START_SLICE).sun_angle();
        let noon = clock_at_slice(NOON_SLICE).sun_angle();
        let set = clock_at_slice(NIGHT_START_SLICE).sun_angle();
        assert!(rise.abs() < 1e-4);
        assert!((noon - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
        assert!((set - std::f32::consts::PI).abs() < 1e-3);
        assert!(clock_at_slice(0).sun_height() < 0.0);
    }

    #[test]
    fn dawn_is_this_morning_or_tomorrow() {
        let day = DayLength::Standard;
        let dawn = u64::from(DAWN_START_SLICE) * day.slice_millis();
        assert_eq!(clock_at_slice(0).millis_until_dawn(), dawn);
        assert_eq!(
            clock_at_slice(NIGHT_START_SLICE).millis_until_dawn(),
            day.millis() - u64::from(NIGHT_START_SLICE) * day.slice_millis() + dawn
        );
    }

    #[test]
    fn a_new_clock_starts_in_the_morning_of_day_one() {
        for length in DayLength::ALL {
            let state = ClockState::new(length);
            let clock = WorldClock::at(SimulationTick::new(0), TickRate::new(60), &state, 7);
            assert_eq!(clock.day(), 1);
            assert_eq!(clock.phase(), DayPhase::Day);
            assert_eq!(clock.slice(), NEW_WORLD_START_SLICE);
        }
    }

    #[test]
    fn the_default_moon_hangs_a_few_thousand_tiles_from_town() {
        for seed in [0, 1, 2, 0xD111_6A4E, u64::MAX] {
            let anchor = default_moon_anchor(seed);
            let distance = (anchor.column - crate::worldgen::SPAWN_TILE_X).abs();
            assert!((2_400..4_800).contains(&distance), "{seed}: {distance}");
            assert_eq!(anchor, default_moon_anchor(seed));
        }
    }
}
