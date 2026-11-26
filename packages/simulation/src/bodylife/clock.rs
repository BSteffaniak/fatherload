//! A body's own clock (Wave 12 bodylife): its day length, the time of day at every longitude,
//! the sun's place in its sky and the light it gives.
//!
//! The home world keeps its [`crate::clock::WorldClock`]. A body's time of day is a pure
//! function of the same world time (`WorldClock::world_millis`, so a night slept away at home
//! passes on every body too), the body's day length and phase, and the strip column: the
//! body turns under its star, so noon sweeps round it once a day and two players at
//! different columns, or on different bodies, see their own hour.
//!
//! A **tidally locked** body (day length 0) always shows its star the same face: column 0
//! (the top of the body) has the star overhead forever, the far side (half a lap round) is in
//! eternal night, and the terminators lie a quarter of a lap either way.
//!
//! Local time runs `0..DAY_PERMILLE` from local midnight (`500` is noon), so the home clock's
//! slices and [`DayPhase`] apply unchanged ([`BodyClock::slice`]).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "bounded clock arithmetic (columns, permille, milliseconds); plain IEEE arithmetic \
              (no fused multiply-add), bit-identical on every platform"
)]

use crate::{
    clock::{DAY_SLICES, DayPhase},
    polar::trig,
};

/// Local time's resolution: a day is this many parts from midnight.
pub const DAY_PERMILLE: u32 = 1_000;
/// Light (per mille) of the night side of an airless body: starlight on bare rock.
pub const AIRLESS_NIGHT_LIGHT: u16 = 140;
/// Light (per mille) of the night side of a body with air: its sky scatters a little.
pub const AIR_NIGHT_LIGHT: u16 = 110;
/// The sun's elevation (sine) below which it is full night...
pub const NIGHT_ELEVATION: f64 = -0.12;
/// ...and above which it is full day (between them, twilight).
pub const DAY_ELEVATION: f64 = 0.16;

/// A body's clock (see the module docs).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodyClock {
    /// Length of a day (milliseconds); `0` for a tidally locked body.
    pub day_millis: u64,
    /// Where the day stands at world time zero (milliseconds into the day at column 0).
    pub phase_millis: u64,
    /// Columns round the surface (one lap of the strip).
    pub columns: u32,
}

impl BodyClock {
    /// The clock of a body whose day lasts `day_seconds` (0: tidally locked) with `columns`
    /// round its surface; `seed` sets where its day stood at world time zero.
    #[must_use]
    pub const fn new(day_seconds: u32, columns: u32, seed: u64) -> Self {
        let day_millis = day_seconds as u64 * 1_000;
        Self {
            day_millis,
            phase_millis: if day_millis == 0 {
                0
            } else {
                (seed >> 7) % day_millis
            },
            columns: if columns == 0 { 1 } else { columns },
        }
    }

    /// Whether the body is tidally locked (eternal day and night sides).
    #[must_use]
    pub const fn tidally_locked(&self) -> bool {
        self.day_millis == 0
    }

    /// Local time at strip `column` at world time `millis`, in [`DAY_PERMILLE`]ths of a day
    /// from local midnight (columns wrap).
    #[must_use]
    pub const fn local_permille(&self, millis: u64, column: i64) -> u32 {
        let columns = self.columns as i64;
        let column = column.rem_euclid(columns) as u64;
        if self.day_millis == 0 {
            // Column 0 faces the star: noon there, midnight half a lap round.
            let along = column * DAY_PERMILLE as u64 / self.columns as u64;
            return ((DAY_PERMILLE as u64 / 2 + along) % DAY_PERMILLE as u64) as u32;
        }
        let day = self.day_millis;
        let offset = (column as u128 * day as u128 / self.columns as u128) as u64;
        let into = (millis % day + self.phase_millis + offset) % day;
        (into as u128 * DAY_PERMILLE as u128 / day as u128) as u32
    }

    /// Local time at strip `column` as the home clock's day slice (`0..DAY_SLICES`).
    #[must_use]
    pub const fn slice(&self, millis: u64, column: i64) -> u32 {
        self.local_permille(millis, column) * DAY_SLICES / DAY_PERMILLE
    }

    /// The part of the day at strip `column`.
    #[must_use]
    pub const fn phase(&self, millis: u64, column: i64) -> DayPhase {
        DayPhase::of_slice(self.slice(millis, column))
    }

    /// Local hours and minutes at strip `column` (a 24-hour face whatever the day's length).
    #[must_use]
    pub const fn hours_minutes(&self, millis: u64, column: i64) -> (u8, u8) {
        let minutes = self.local_permille(millis, column) * 24 * 60 / DAY_PERMILLE;
        ((minutes / 60) as u8, (minutes % 60) as u8)
    }

    /// Days since world time zero at column 0 (the local day counter; always 0 on a locked
    /// body).
    #[must_use]
    pub const fn day(&self, millis: u64) -> u64 {
        match (millis + self.phase_millis).checked_div(self.day_millis) {
            Some(day) => day,
            None => 0,
        }
    }

    /// The local day at strip `column` (markets turn over at local midnight).
    #[must_use]
    pub const fn local_day(&self, millis: u64, column: i64) -> u64 {
        if self.day_millis == 0 {
            return 0;
        }
        let columns = self.columns as i64;
        let column = column.rem_euclid(columns) as u64;
        let offset = (column as u128 * self.day_millis as u128 / self.columns as u128) as u64;
        (millis + self.phase_millis + offset) / self.day_millis
    }

    /// The sun's elevation at strip `column`: the sine of its height over the horizon (1 at
    /// noon, -1 at midnight). Deterministic trigonometry ([`trig`]).
    #[must_use]
    pub fn elevation(&self, millis: u64, column: i64) -> f64 {
        let hour = f64::from(self.local_permille(millis, column)) / f64::from(DAY_PERMILLE);
        // Noon (0.5) is straight up: elevation = -cos(2 pi hour).
        let (_, cos) = trig::sin_cos(hour * std::f64::consts::TAU);
        -cos
    }

    /// The star's angle from the local up at strip `column` (radians, clockwise like the
    /// strip's columns): 0 at noon, a quarter turn at the evening terminator.
    #[must_use]
    pub fn sun_angle(&self, millis: u64, column: i64) -> f64 {
        let hour = f64::from(self.local_permille(millis, column)) / f64::from(DAY_PERMILLE);
        (0.5 - hour) * std::f64::consts::TAU
    }

    /// How far the body has turned since its column 0 was at local midnight (turns,
    /// `0.0..1.0`): the renderer turns the body's day side with it.
    #[must_use]
    pub fn turn(&self, millis: u64) -> f64 {
        if self.day_millis == 0 {
            return 0.5;
        }
        let into = (millis % self.day_millis + self.phase_millis) % self.day_millis;
        into as f64 / self.day_millis as f64
    }

    /// Daylight at strip `column` (per mille), from `night` on the night side to full day.
    #[must_use]
    pub fn daylight(&self, millis: u64, column: i64, night: u16) -> u16 {
        let elevation = self.elevation(millis, column);
        let t = ((elevation - NIGHT_ELEVATION) / (DAY_ELEVATION - NIGHT_ELEVATION)).clamp(0.0, 1.0);
        // Smoothstep in plain arithmetic.
        let t = t * t * (3.0 - 2.0 * t);
        let night = f64::from(night);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "between `night` and 1,000"
        )]
        let light = (night + (1_000.0 - night) * t).round() as u16;
        light
    }

    /// The first world time at or after `after` when the local time at strip `column` is
    /// `permille` (`after` itself on a tidally locked body, whose hours stand still; scenes,
    /// tests and the HUD's "dawn in ...").
    #[must_use]
    pub const fn millis_when(&self, after: u64, column: i64, permille: u32) -> u64 {
        if self.day_millis == 0 {
            return after;
        }
        let day = self.day_millis;
        let columns = self.columns as i64;
        let column = column.rem_euclid(columns) as u64;
        let offset = (column as u128 * day as u128 / self.columns as u128) as u64;
        // The day position wanted, rounded up so the reading is `permille`.
        let want =
            ((permille % DAY_PERMILLE) as u128 * day as u128).div_ceil(DAY_PERMILLE as u128) as u64;
        let now = (after % day + self.phase_millis + offset) % day;
        let ahead = (want + day - now) % day;
        after + ahead
    }

    /// The strip column whose local time is `permille` at world time `millis` (the nearest
    /// one; scenes and tests).
    #[must_use]
    pub const fn column_at(&self, millis: u64, permille: u32) -> i64 {
        let now = self.local_permille(millis, 0) as i64;
        let want = (permille % DAY_PERMILLE) as i64;
        let ahead = (want - now).rem_euclid(DAY_PERMILLE as i64);
        ahead * self.columns as i64 / DAY_PERMILLE as i64
    }
}
