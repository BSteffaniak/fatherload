//! The world clock as people see it: a tiny seam for villager routines.
//!
//! Wave 3 builds the real world clock in parallel (T14, expansion plan §9 F4). Until it lands,
//! routines read time through [`ClockSource`], whose default implementation [`TickClock`] derives
//! the day and the time of day from the simulation tick, the tick rate and a day length
//! ([`DEFAULT_DAY_LENGTH_MINUTES`], the plan's 48 real minutes). Everything is integer
//! arithmetic on the tick, so it is deterministic, tick-rate safe and needs no saving or
//! replication.
//!
//! **Integration:** `WorldState::people_clock` (in `world/people.rs`) is the single place the
//! simulation asks for the time; it reads T14's world clock (`crate::clock::WorldClock`
//! implements [`ClockSource`]), so routines follow the world's day length and skipped nights.
//! [`TickClock`] stays for tests and for tools that only know a tick.

/// Real minutes in one in-game day by default (plan §11 decision 2: 24 / 48 / 72).
pub const DEFAULT_DAY_LENGTH_MINUTES: u32 = 48;
/// Clock minutes in a day (the routine schedule is written in a familiar 24-hour clock).
pub const CLOCK_MINUTES_PER_DAY: u32 = 24 * 60;
/// The time of day at tick 0 (08:00), so a new world starts in the morning.
pub const START_MINUTE: u32 = 8 * 60;
/// Night starts at 19:30, with the world clock's night (`crate::clock::NIGHT_START_SLICE`;
/// WP-NIGHT aligned people's night with the sky's) ...
pub const NIGHT_START_MINUTE: u32 =
    crate::clock::NIGHT_START_SLICE * CLOCK_MINUTES_PER_DAY / crate::clock::DAY_SLICES;
/// ... and ends at 04:30, when dawn begins (`crate::clock::DAWN_START_SLICE`).
pub const NIGHT_END_MINUTE: u32 =
    crate::clock::DAWN_START_SLICE * CLOCK_MINUTES_PER_DAY / crate::clock::DAY_SLICES;

/// A moment on the clock: the day number (0 is the first day) and the minute of that day
/// (`0..CLOCK_MINUTES_PER_DAY`, 0 is midnight).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DayTime {
    pub day: u32,
    pub minute: u16,
}

impl DayTime {
    /// A time on `day` at `hour:minute`.
    #[must_use]
    pub const fn at(day: u32, hour: u16, minute: u16) -> Self {
        Self {
            day,
            minute: (hour % 24) * 60 + minute % 60,
        }
    }

    /// The hour (`0..24`).
    #[must_use]
    pub const fn hour(self) -> u16 {
        self.minute / 60
    }

    /// Whether it is night (19:30 to 04:30, the world clock's night).
    #[must_use]
    pub const fn is_night(self) -> bool {
        let minute = self.minute as u32;
        minute >= NIGHT_START_MINUTE || minute < NIGHT_END_MINUTE
    }

    /// "07:45".
    #[must_use]
    pub fn label(self) -> String {
        format!("{:02}:{:02}", self.minute / 60, self.minute % 60)
    }
}

/// Anything that can tell people the time (T14's world clock implements this at integration).
pub trait ClockSource {
    /// The current day and minute of day.
    fn day_time(&self) -> DayTime;

    /// Whether it is night (villagers are home, the open surface is dangerous).
    fn is_night(&self) -> bool {
        self.day_time().is_night()
    }
}

/// The default clock: the tick, the tick rate and a day length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TickClock {
    pub tick: u64,
    pub tick_rate_hz: u32,
    pub day_length_minutes: u32,
}

impl TickClock {
    #[must_use]
    pub const fn new(tick: u64, tick_rate_hz: u32, day_length_minutes: u32) -> Self {
        Self {
            tick,
            tick_rate_hz,
            day_length_minutes,
        }
    }

    /// The first tick at which the clock reads `time` (the inverse of [`ClockSource::day_time`],
    /// rounded up; scene capture and tests use it to jump to a time of day).
    #[must_use]
    pub const fn tick_at(time: DayTime, tick_rate_hz: u32, day_length_minutes: u32) -> u64 {
        let day_millis = day_length_minutes as u64 * 60_000;
        let minutes = time.day as u64 * CLOCK_MINUTES_PER_DAY as u64 + time.minute as u64;
        let since_start = minutes.saturating_sub(START_MINUTE as u64);
        let millis = (since_start * day_millis).div_ceil(CLOCK_MINUTES_PER_DAY as u64);
        (millis * tick_rate_hz as u64).div_ceil(1_000)
    }
}

impl ClockSource for TickClock {
    fn day_time(&self) -> DayTime {
        let hz = u64::from(self.tick_rate_hz.max(1));
        let millis = self.tick.saturating_mul(1_000) / hz;
        let day_millis = u64::from(self.day_length_minutes.max(1)) * 60_000;
        let start = u64::from(START_MINUTE) * day_millis / u64::from(CLOCK_MINUTES_PER_DAY);
        let elapsed = millis + start;
        let day = u32::try_from(elapsed / day_millis).unwrap_or(u32::MAX);
        let minute = (elapsed % day_millis) * u64::from(CLOCK_MINUTES_PER_DAY) / day_millis;
        DayTime {
            day,
            minute: u16::try_from(minute).unwrap_or(0),
        }
    }
}

/// T14's world clock (Part II F4): the day from 0 and the minute of the day on its 24-hour
/// face. A fresh standard world reads exactly what [`TickClock`] would (both start at 08:00 of
/// a 48-minute day), and skipped nights and the world's day length carry through.
impl ClockSource for crate::clock::WorldClock {
    fn day_time(&self) -> DayTime {
        let minute = self.millis_into_day() * u64::from(CLOCK_MINUTES_PER_DAY) / self.day_millis();
        DayTime {
            day: self.day().saturating_sub(1),
            minute: u16::try_from(minute).unwrap_or(0),
        }
    }

    fn is_night(&self) -> bool {
        Self::is_night(*self)
    }
}

impl ClockSource for DayTime {
    fn day_time(&self) -> DayTime {
        *self
    }
}

#[cfg(test)]
mod tests {
    use super::{ClockSource, DEFAULT_DAY_LENGTH_MINUTES, DayTime, TickClock};

    #[test]
    fn the_clock_starts_in_the_morning_and_is_tick_rate_safe() {
        for hz in [30, 60, 120] {
            let clock = TickClock::new(0, hz, DEFAULT_DAY_LENGTH_MINUTES);
            assert_eq!(clock.day_time(), DayTime::at(0, 8, 0));
            // A full 48-minute day later it is 08:00 on day 1.
            let later = TickClock::new(48 * 60 * u64::from(hz), hz, DEFAULT_DAY_LENGTH_MINUTES);
            assert_eq!(later.day_time(), DayTime::at(1, 8, 0));
            // Half a day later it is 20:00 and night.
            let evening = TickClock::new(24 * 60 * u64::from(hz), hz, DEFAULT_DAY_LENGTH_MINUTES);
            assert_eq!(evening.day_time(), DayTime::at(0, 20, 0));
            assert!(evening.is_night());
        }
    }

    #[test]
    fn peoples_night_is_the_skys_night() {
        // WP-NIGHT: villagers' night and the world clock's night start and end together.
        let length = crate::clock::DayLength::Standard;
        for slice in 0..crate::clock::DAY_SLICES {
            let clock = crate::clock::WorldClock::from_world_millis(
                u64::from(slice) * length.slice_millis() + 1,
                length,
                3,
            );
            assert_eq!(
                ClockSource::is_night(&clock.day_time()),
                clock.is_night(),
                "slice {slice}"
            );
        }
    }

    #[test]
    fn tick_at_inverts_the_clock() {
        for hz in [30, 60] {
            for (day, hour, minute) in [(0, 8, 0), (0, 12, 30), (0, 21, 15), (3, 2, 5)] {
                let time = DayTime::at(day, hour, minute);
                let tick = TickClock::tick_at(time, hz, DEFAULT_DAY_LENGTH_MINUTES);
                let read = TickClock::new(tick, hz, DEFAULT_DAY_LENGTH_MINUTES).day_time();
                assert_eq!(read, time, "{hz} Hz {}", time.label());
            }
        }
    }
}
