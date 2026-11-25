//! The buried city's moving machinery: trams, lifts, pistons and crushers.
//!
//! Every mover is a **pure function of world time** and the generated place it runs in: a
//! tram or a lift cab runs a [`Track`] (stops along an axis, a top speed, a dwell at every
//! stop, a phase), a piston or a crusher a [`Cycle`] (one stroke of the Great Engine's beat).
//! Positions are integer milli-tiles computed from world milliseconds, so every peer and
//! every tick rate sees the same machine in the same place, and nothing about a mover is
//! saved or replicated except who rides it ([`CityRide`]).

use serde::{Deserialize, Serialize};

use crate::ids::SimulationTick;

/// Milli-tiles a tile.
pub const MILLI: i64 = 1_000;

/// A tram car's width (tiles).
pub const TRAM_CAR_TILES: i32 = 5;
/// A tram car's height, roof included (tiles).
pub const TRAM_CAR_ROWS: i32 = 3;
/// A tram's cruising speed (milli-tiles a second; it peaks half again faster mid-leg).
pub const TRAM_SPEED: i64 = 9_000;
/// How long a tram waits at a station.
pub const TRAM_DWELL_MILLIS: u64 = 3_600;
/// A lift cab's width (tiles).
pub const LIFT_CAB_TILES: i32 = 3;
/// A lift cab's height (tiles).
pub const LIFT_CAB_ROWS: i32 = 3;
/// A lift's cruising speed (milli-tiles a second).
pub const LIFT_SPEED: i64 = 6_000;
/// How long a lift waits at a floor.
pub const LIFT_DWELL_MILLIS: u64 = 2_800;
/// The easing added to every leg (start and stop), milliseconds.
pub const LEG_RAMP_MILLIS: u64 = 600;
/// A piston head's width (tiles).
pub const PISTON_TILES: i32 = 3;
/// A crusher block's width and height (tiles).
pub const CRUSHER_TILES: (i32, i32) = (3, 2);
/// How fast a rig drives along a car, cab or head it rides (px/s).
pub const RIDE_WALK_SPEED: f32 = 110.0;
/// How long after riding off a piston a rig cannot settle onto it again.
pub const PISTON_REBOARD_MILLIS: u64 = 450;

/// What kind of mover.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MoverKind {
    /// A tram car on an avenue's rail.
    Tram,
    /// A lift cab in a tower's shaft.
    Lift,
    /// A piston head rising from an engine hall's floor.
    Piston,
    /// A crusher block slamming down from an engine hall's ceiling.
    Crusher,
}

/// Which mover: its kind and anchor tile (the generator's identity for it).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct MoverRef {
    pub kind: MoverKind,
    pub x: i32,
    pub y: i32,
}

/// A rig riding a mover: which one, where along it, since when.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct CityRide {
    pub mover: MoverRef,
    /// The rig's offset from the mover's centre along the free axis (px).
    pub offset: f32,
    pub since: SimulationTick,
}

/// A line a car runs along (see the module docs).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    /// Stops along the axis (milli-tiles, ascending; at least one).
    pub stops: Vec<i64>,
    /// Cruising speed (milli-tiles a second).
    pub speed: i64,
    /// Dwell at every stop.
    pub dwell_millis: u64,
    /// Where in its timetable the line is at world time zero.
    pub phase_millis: u64,
}

/// Where a car on a [`Track`] is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrackState {
    /// Its centre along the axis (milli-tiles).
    pub pos: i64,
    /// Its speed along the axis (milli-tiles a second, signed).
    pub vel: i64,
    /// The stop it waits at, if it is dwelling.
    pub dwelling: Option<usize>,
    /// Milliseconds until it next leaves (dwelling) or arrives (moving).
    pub until_millis: u64,
}

/// How long a leg of `distance` milli-tiles takes at `speed`.
#[must_use]
pub const fn leg_millis(distance: i64, speed: i64) -> u64 {
    let speed = if speed < 1 { 1 } else { speed };
    let distance = distance.unsigned_abs();
    distance * 1_000 / speed.unsigned_abs() + LEG_RAMP_MILLIS
}

/// Fixed-point smoothstep of `u` in `0..=1<<16`.
const fn smooth(u: i64) -> i64 {
    let one = 1_i64 << 16;
    let u = if u < 0 {
        0
    } else if u > one {
        one
    } else {
        u
    };
    // 3u^2 - 2u^3, in 16-bit fixed point.
    let u2 = (u * u) >> 16;
    let u3 = (u2 * u) >> 16;
    3 * u2 - 2 * u3
}

/// The smoothstep's slope at `u` (in 16-bit fixed point, per unit of `u`): `6u(1-u)`.
const fn smooth_slope(u: i64) -> i64 {
    let one = 1_i64 << 16;
    if u <= 0 || u >= one {
        return 0;
    }
    6 * ((u * (one - u)) >> 16)
}

/// The legs of a track's round trip, `(from, to)` in order (out to the last stop and back).
fn legs(stops: &[i64]) -> Vec<(i64, i64)> {
    let mut legs = Vec::new();
    for pair in stops.windows(2) {
        legs.push((pair[0], pair[1]));
    }
    for pair in stops.windows(2).rev() {
        legs.push((pair[1], pair[0]));
    }
    legs
}

/// One round trip of `track` (milliseconds; its dwell alone for a single stop).
#[must_use]
pub fn track_period(track: &Track) -> u64 {
    legs(&track.stops)
        .iter()
        .map(|(from, to)| track.dwell_millis + leg_millis(to - from, track.speed))
        .sum::<u64>()
        .max(track.dwell_millis.max(1))
}

/// The index of the stop at milli-tile `pos` on `track`, if there is one.
fn stop_index(track: &Track, pos: i64) -> usize {
    track
        .stops
        .iter()
        .position(|stop| *stop == pos)
        .unwrap_or(0)
}

/// Where `track`'s car is at world time `millis`.
#[must_use]
pub fn track_state(track: &Track, millis: u64) -> TrackState {
    let first = track.stops.first().copied().unwrap_or(0);
    if track.stops.len() < 2 {
        return TrackState {
            pos: first,
            vel: 0,
            dwelling: Some(0),
            until_millis: track.dwell_millis,
        };
    }
    let period = track_period(track);
    let mut t = (millis + track.phase_millis) % period;
    for (from, to) in legs(&track.stops) {
        if t < track.dwell_millis {
            return TrackState {
                pos: from,
                vel: 0,
                dwelling: Some(stop_index(track, from)),
                until_millis: track.dwell_millis - t,
            };
        }
        t -= track.dwell_millis;
        let span = leg_millis(to - from, track.speed);
        if t < span {
            #[allow(clippy::cast_possible_wrap, reason = "a leg is seconds long")]
            let u = ((t as i64) << 16) / (span as i64).max(1);
            let delta = to - from;
            #[allow(clippy::cast_possible_truncation, reason = "within the leg's own span")]
            let pos = from + ((i128::from(delta) * i128::from(smooth(u))) >> 16) as i64;
            #[allow(
                clippy::cast_possible_wrap,
                clippy::cast_possible_truncation,
                reason = "a leg is seconds long; a speed of tiles a second"
            )]
            let vel = ((i128::from(delta) * i128::from(smooth_slope(u)) * 1_000)
                / (i128::from(span as i64) << 16)) as i64;
            return TrackState {
                pos,
                vel,
                dwelling: None,
                until_millis: span - t,
            };
        }
        t -= span;
    }
    TrackState {
        pos: first,
        vel: 0,
        dwelling: Some(0),
        until_millis: 0,
    }
}

/// One stroke of a piston or a crusher: its period (the hall's beat), phase and travel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cycle {
    pub period_millis: u64,
    pub phase_millis: u64,
    /// How far it travels (tiles).
    pub travel: i32,
}

/// Where a piston or a crusher is in its stroke.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokePhase {
    /// At rest (retracted piston, raised crusher).
    Rest,
    /// At rest, about to move (the telegraph: a piston hisses, a crusher's lamp glows).
    Tell,
    /// Moving out (a piston rising, a crusher slamming down).
    Out,
    /// Held out.
    Held,
    /// Moving back.
    Back,
}

/// A stroke at one moment: how far out (per mille of its travel), how fast (per mille a
/// second, signed outward) and in which phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stroke {
    pub extension: i64,
    pub rate: i64,
    pub phase: StrokePhase,
}

/// The phases of a piston's stroke (per mille of its period): rest, rise, held high, fall.
pub const PISTON_PHASES: (i64, i64, i64) = (400, 560, 800);
/// The phases of a crusher's stroke (per mille of its period): rest, slam, held down, rise.
pub const CRUSHER_PHASES: (i64, i64, i64) = (500, 545, 650);

/// The stroke of `cycle` at world time `millis`.
///
/// `phases` are its boundaries (out starts, held starts, back starts; per mille of the
/// period), `tell_millis` the telegraph before it moves out; a slam (`slam`) accelerates
/// instead of easing.
#[must_use]
pub fn stroke(
    cycle: &Cycle,
    millis: u64,
    phases: (i64, i64, i64),
    tell_millis: u64,
    slam: bool,
) -> Stroke {
    let period = cycle.period_millis.max(1);
    #[allow(clippy::cast_possible_wrap, reason = "seconds")]
    let (t, period) = (
        ((millis + cycle.phase_millis) % period) as i64,
        period as i64,
    );
    let at = |share: i64| period * share / 1_000;
    let (out, held, back) = (at(phases.0), at(phases.1), at(phases.2));
    #[allow(clippy::cast_possible_wrap, reason = "seconds")]
    let tell = tell_millis as i64;
    if t < out {
        let phase = if t >= out - tell {
            StrokePhase::Tell
        } else {
            StrokePhase::Rest
        };
        return Stroke {
            extension: 0,
            rate: 0,
            phase,
        };
    }
    if t < held {
        let span = (held - out).max(1);
        let u = ((t - out) << 16) / span;
        let (extension, rate) = if slam {
            // A slam: u^2, accelerating to the floor.
            let e = (((u * u) >> 16) * 1_000) >> 16;
            let r = ((2 * u * 1_000) >> 16) * 1_000 / span;
            (e, r)
        } else {
            (
                (smooth(u) * 1_000) >> 16,
                ((smooth_slope(u) * 1_000) >> 16) * 1_000 / span,
            )
        };
        return Stroke {
            extension,
            rate,
            phase: StrokePhase::Out,
        };
    }
    if t < back {
        return Stroke {
            extension: 1_000,
            rate: 0,
            phase: StrokePhase::Held,
        };
    }
    let span = (period - back).max(1);
    let u = ((t - back) << 16) / span;
    Stroke {
        extension: 1_000 - ((smooth(u) * 1_000) >> 16),
        rate: -(((smooth_slope(u) * 1_000) >> 16) * 1_000 / span),
        phase: StrokePhase::Back,
    }
}

/// A piston's stroke at `millis`.
#[must_use]
pub fn piston_stroke(cycle: &Cycle, millis: u64) -> Stroke {
    stroke(
        cycle,
        millis,
        PISTON_PHASES,
        super::PISTON_TELL_MILLIS,
        false,
    )
}

/// A crusher's stroke at `millis`.
#[must_use]
pub fn crusher_stroke(cycle: &Cycle, millis: u64) -> Stroke {
    stroke(
        cycle,
        millis,
        CRUSHER_PHASES,
        super::CRUSHER_TELL_MILLIS,
        true,
    )
}

/// A generated mover, with everything needed to place it at any moment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CityMover {
    /// A tram on an avenue: its car rests on the rail row (the car's floor is the rail's top)
    /// and runs `track` along the columns (milli-tiles of the car's centre).
    Tram {
        anchor: MoverRef,
        rail_row: i32,
        track: Track,
    },
    /// A lift in a tower: its cab is centred on `column` and runs `track` along the rows
    /// (milli-tiles of the cab floor's top edge; a stop is the floor row a rig stands on).
    Lift {
        anchor: MoverRef,
        column: i32,
        track: Track,
    },
    /// A piston: a head [`PISTON_TILES`] wide centred on `column`, flush with `floor_row`'s
    /// top at rest and `cycle.travel` rows higher at full stroke.
    Piston {
        anchor: MoverRef,
        column: i32,
        floor_row: i32,
        cycle: Cycle,
    },
    /// A crusher: a block [`CRUSHER_TILES`] hanging under its housing on `housing_row`,
    /// centred on `column`; at full stroke its bottom meets the top of `housing_row + 3 +
    /// cycle.travel`.
    Crusher {
        anchor: MoverRef,
        column: i32,
        housing_row: i32,
        cycle: Cycle,
    },
}

impl CityMover {
    /// Its identity.
    #[must_use]
    pub const fn anchor(&self) -> MoverRef {
        match self {
            Self::Tram { anchor, .. }
            | Self::Lift { anchor, .. }
            | Self::Piston { anchor, .. }
            | Self::Crusher { anchor, .. } => *anchor,
        }
    }

    /// The car or cab's track state at `millis` (trams and lifts).
    #[must_use]
    pub fn track_at(&self, millis: u64) -> Option<TrackState> {
        match self {
            Self::Tram { track, .. } | Self::Lift { track, .. } => Some(track_state(track, millis)),
            _ => None,
        }
    }

    /// The top edge of a piston head at `millis` (milli-rows), and its speed (milli-rows a
    /// second, negative rising).
    #[must_use]
    pub fn piston_top(&self, millis: u64) -> Option<(i64, i64, StrokePhase)> {
        let Self::Piston {
            floor_row, cycle, ..
        } = self
        else {
            return None;
        };
        let stroke = piston_stroke(cycle, millis);
        let travel = i64::from(cycle.travel) * MILLI;
        let top = i64::from(*floor_row) * MILLI - travel * stroke.extension / 1_000;
        let vel = -travel * stroke.rate / 1_000;
        Some((top, vel, stroke.phase))
    }

    /// The bottom edge of a crusher block at `millis` (milli-rows), and its phase.
    #[must_use]
    pub fn crusher_bottom(&self, millis: u64) -> Option<(i64, StrokePhase)> {
        let Self::Crusher {
            housing_row, cycle, ..
        } = self
        else {
            return None;
        };
        let stroke = crusher_stroke(cycle, millis);
        let rest = i64::from(*housing_row + 1 + CRUSHER_TILES.1) * MILLI;
        let travel = i64::from(cycle.travel) * MILLI;
        Some((rest + travel * stroke.extension / 1_000, stroke.phase))
    }

    /// The tiles it sweeps over its whole cycle (for keeping things clear of it).
    #[must_use]
    pub fn swept(&self) -> crate::worldgen::TileRect {
        use crate::{terrain::TilePosition, worldgen::TileRect};
        match self {
            Self::Tram {
                rail_row, track, ..
            } => {
                let first = track.stops.first().copied().unwrap_or(0) / MILLI;
                let last = track.stops.last().copied().unwrap_or(0) / MILLI;
                #[allow(clippy::cast_possible_truncation, reason = "tile columns")]
                TileRect {
                    min: TilePosition {
                        x: first as i32 - TRAM_CAR_TILES / 2,
                        y: rail_row - TRAM_CAR_ROWS,
                    },
                    max: TilePosition {
                        x: last as i32 + TRAM_CAR_TILES / 2,
                        y: rail_row - 1,
                    },
                }
            }
            Self::Lift { column, track, .. } => {
                let top = track.stops.first().copied().unwrap_or(0) / MILLI;
                let bottom = track.stops.last().copied().unwrap_or(0) / MILLI;
                #[allow(clippy::cast_possible_truncation, reason = "tile rows")]
                TileRect {
                    min: TilePosition {
                        x: column - LIFT_CAB_TILES / 2,
                        y: top as i32 - LIFT_CAB_ROWS,
                    },
                    max: TilePosition {
                        x: column + LIFT_CAB_TILES / 2,
                        y: bottom as i32 - 1,
                    },
                }
            }
            Self::Piston {
                column,
                floor_row,
                cycle,
                ..
            } => TileRect {
                min: TilePosition {
                    x: column - PISTON_TILES / 2,
                    y: floor_row - cycle.travel,
                },
                max: TilePosition {
                    x: column + PISTON_TILES / 2,
                    y: *floor_row,
                },
            },
            Self::Crusher {
                column,
                housing_row,
                cycle,
                ..
            } => TileRect {
                min: TilePosition {
                    x: column - CRUSHER_TILES.0 / 2,
                    y: *housing_row,
                },
                max: TilePosition {
                    x: column + CRUSHER_TILES.0 / 2,
                    y: housing_row + CRUSHER_TILES.1 + cycle.travel,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Track {
        Track {
            stops: vec![10_500, 40_500, 80_500],
            speed: TRAM_SPEED,
            dwell_millis: TRAM_DWELL_MILLIS,
            phase_millis: 0,
        }
    }

    #[test]
    fn a_tram_dwells_runs_and_comes_back() {
        let track = line();
        let start = track_state(&track, 0);
        assert_eq!(start.pos, 10_500);
        assert_eq!(start.dwelling, Some(0));
        // Half way along its first leg it moves right.
        let leg = leg_millis(30_000, TRAM_SPEED);
        let mid = track_state(&track, TRAM_DWELL_MILLIS + leg / 2);
        assert!(mid.dwelling.is_none() && mid.vel > 0);
        assert!((mid.pos - 25_500).abs() < 300, "{}", mid.pos);
        // Then it waits at the second stop.
        let second = track_state(&track, TRAM_DWELL_MILLIS + leg + 10);
        assert_eq!(second.dwelling, Some(1));
        assert_eq!(second.pos, 40_500);
        // A full round trip later it is back where it began.
        let period = track_period(&track);
        assert_eq!(track_state(&track, period), start);
        // On the way back it moves left.
        let back = track_state(&track, period - leg / 2);
        assert!(back.vel < 0);
    }

    #[test]
    fn a_track_moves_continuously_and_its_speed_matches_its_motion() {
        let track = line();
        let period = track_period(&track);
        let mut last = track_state(&track, 0);
        for millis in (10..period).step_by(10) {
            let now = track_state(&track, millis);
            // No jumps: at most a tram's peak speed for 10 ms (with rounding).
            assert!((now.pos - last.pos).abs() <= TRAM_SPEED * 2 / 100 + 2);
            // The reported speed agrees with the motion's direction.
            if now.vel > 200 {
                assert!(now.pos >= last.pos);
            }
            last = now;
        }
    }

    #[test]
    fn a_piston_rests_tells_rises_holds_and_falls() {
        let cycle = Cycle {
            period_millis: 4_000,
            phase_millis: 0,
            travel: 6,
        };
        assert_eq!(piston_stroke(&cycle, 0).phase, StrokePhase::Rest);
        assert_eq!(piston_stroke(&cycle, 1_500).phase, StrokePhase::Tell);
        let rising = piston_stroke(&cycle, 1_920);
        assert_eq!(rising.phase, StrokePhase::Out);
        assert!(rising.rate > 0 && rising.extension > 0 && rising.extension < 1_000);
        assert_eq!(piston_stroke(&cycle, 2_500).extension, 1_000);
        let falling = piston_stroke(&cycle, 3_600);
        assert_eq!(falling.phase, StrokePhase::Back);
        assert!(falling.rate < 0);
        // The same stroke a period later.
        assert_eq!(piston_stroke(&cycle, 1_920 + 4_000), rising);
    }

    #[test]
    fn a_crusher_warns_before_it_slams() {
        let cycle = Cycle {
            period_millis: 4_000,
            phase_millis: 0,
            travel: 4,
        };
        let slam_starts = 2_000;
        assert_eq!(
            crusher_stroke(&cycle, slam_starts - super::super::CRUSHER_TELL_MILLIS).phase,
            StrokePhase::Tell
        );
        assert_eq!(crusher_stroke(&cycle, slam_starts - 1).extension, 0);
        assert_eq!(
            crusher_stroke(&cycle, slam_starts + 90).phase,
            StrokePhase::Out
        );
        assert_eq!(crusher_stroke(&cycle, 2_400).extension, 1_000);
    }
}
