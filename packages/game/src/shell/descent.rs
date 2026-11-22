//! The "entering <stratum>" banner of a Deep Descent world (generator V11): each seat follows
//! the stratum its rig is in and announces every new one (a new Descent's first stratum with
//! the chapter's number and name).
//!
//! The banner follows the strata's layout rows (not the ragged per-tile boundary), so a rig
//! working a transition zone does not flicker between two names; a stratum announced in the
//! last few seconds is not announced again.

use fatherload_simulation::{
    descent::{Descent, Stratum},
    terrain::TILE_SIZE,
    worldgen::descent,
};

use super::{SeatContext, SeatUi};

/// Frames the stratum banner shows for.
pub const STRATUM_BANNER_TICKS: u64 = 240;
/// Frames after a banner before the same stratum is announced again.
const REPEAT_TICKS: u64 = 600;
/// Rows below the classic surface the rig must be before strata are announced.
const ANNOUNCE_BELOW_ROWS: i32 = 8;

/// An "entering <stratum>" banner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StratumBanner {
    pub stratum: Stratum,
    /// The stratum opens a new Descent (the banner names the chapter).
    pub new_descent: bool,
    /// `ShellState::update_ticks` when it went up.
    pub tick: u64,
}

impl StratumBanner {
    /// Whether the banner is still showing at `now`.
    #[must_use]
    pub const fn showing(&self, now: u64) -> bool {
        now.saturating_sub(self.tick) < STRATUM_BANNER_TICKS
    }

    /// The small line above the name: "DESCENT IV" for a new chapter, else "ENTERING".
    #[must_use]
    pub fn heading(&self) -> String {
        if self.new_descent {
            format!("DESCENT {}", self.stratum.descent().numeral())
        } else {
            "ENTERING".to_owned()
        }
    }

    /// The line under the name: the Descent it belongs to.
    #[must_use]
    pub const fn subtitle(&self) -> &'static str {
        self.stratum.descent().name()
    }
}

/// A seat's stratum tracking.
#[derive(Clone, Debug, Default)]
pub struct SeatDescent {
    pub banner: Option<StratumBanner>,
    /// The stratum (by layout rows) the rig was in last frame.
    last: Option<Stratum>,
    /// The strata announced lately, with the frame each banner went up.
    recent: Vec<(Stratum, u64)>,
}

impl SeatDescent {
    /// No banner yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            banner: None,
            last: None,
            recent: Vec::new(),
        }
    }

    /// Follows the rig at tile row `row` of a world laid out as `layout`; raises a banner on a
    /// new stratum.
    pub fn track(&mut self, layout: &descent::DescentLayout, row: Option<i32>, tick: u64) {
        let Some(row) = row.filter(|row| *row >= layout.tops[0] + ANNOUNCE_BELOW_ROWS) else {
            self.last = None;
            return;
        };
        let stratum = layout.stratum_of_row(row);
        if self.last == Some(stratum) {
            return;
        }
        self.last = Some(stratum);
        self.recent
            .retain(|(_, shown)| tick.saturating_sub(*shown) < REPEAT_TICKS);
        let recently = self.recent.iter().any(|(shown, _)| *shown == stratum);
        if !recently {
            self.recent.push((stratum, tick));
            self.banner = Some(StratumBanner {
                stratum,
                new_descent: stratum == stratum.descent().first_stratum()
                    && stratum.descent() != Descent::Crust,
                tick,
            });
        }
    }
}

impl SeatUi {
    /// Shows the "entering <stratum>" banner as the seat's rig (or pilot) crosses into a new
    /// stratum of a Deep Descent world.
    pub(super) fn track_stratum(&mut self, ctx: SeatContext<'_>, tick: u64) {
        let settings = ctx.world.world_settings();
        if !settings.generator.has_descents() {
            self.descent = SeatDescent::default();
            return;
        }
        let row = ctx.player().map(|player| {
            #[allow(clippy::cast_possible_truncation, reason = "tile rows are small")]
            let row = (player.y / TILE_SIZE).floor() as i32;
            row
        });
        self.descent.track(&descent::layout(settings), row, tick);
    }
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::worldgen::{GeneratorVersion, WorldSettings};

    use super::*;

    #[test]
    fn crossing_into_a_stratum_raises_one_banner() {
        let settings = WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(5)
        };
        let layout = descent::layout(&settings);
        let mut seat = SeatDescent::default();
        // On the surface: nothing.
        seat.track(&layout, Some(2), 1);
        assert!(seat.banner.is_none());
        // Going down into the Topsoil, then the Clay Beds.
        seat.track(&layout, Some(20), 2);
        assert_eq!(
            seat.banner.map(|banner| banner.stratum),
            Some(Stratum::Topsoil)
        );
        let clay = layout.rows(Stratum::ClayBeds).0 + 3;
        seat.track(&layout, Some(clay), 500);
        let banner = seat.banner.expect("a banner");
        assert_eq!(banner.stratum, Stratum::ClayBeds);
        assert_eq!(banner.heading(), "ENTERING");
        assert!(banner.showing(500 + STRATUM_BANNER_TICKS - 1));
        assert!(!banner.showing(500 + STRATUM_BANNER_TICKS));
        // Bobbing back over the line does not re-announce at once.
        seat.track(&layout, Some(clay - 6), 520);
        seat.track(&layout, Some(clay), 540);
        assert_eq!(seat.banner.map(|banner| banner.tick), Some(500));
        // A new Descent names the chapter.
        let fungal = layout.rows(Stratum::FungalForest).0 + 1;
        seat.track(&layout, Some(fungal), 2_000);
        let banner = seat.banner.expect("a banner");
        assert!(banner.new_descent);
        assert_eq!(banner.heading(), "DESCENT III");
        assert_eq!(banner.subtitle(), "The Living Dark");
    }
}
