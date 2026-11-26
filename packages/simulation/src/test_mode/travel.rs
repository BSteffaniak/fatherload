//! Test-mode travel targets (Wave 14 console track).
//!
//! **TODO-SEAM(w14-index):** this is the console track's thin stand-in for the destination
//! index (expansion plan §19 W14-B). The command palette, `--start-at` and the bots build a
//! [`Destination`] and submit [`crate::TestModeAction::PaletteTravel`]; the world answers it through
//! [`crate::WorldState::resolve_destination`] (the palette's `tp … ?` preview reads the same
//! answer). The integrator maps these kinds onto w14-index's destination model and selectors and
//! deletes the stub resolver in `world/travel.rs`: every caller already goes through the one
//! action and the one resolve call, so nothing else changes.
//!
//! What the stub resolves: a Descent, a depth (rows below the ground), a stratum, a gate boss's
//! Descent floor, home, the Moon, a planet by theme (nearest, n-th outward or random), another
//! player, and an exact spot (the palette's own back-stack and bookmarks). Stations resolve to
//! nothing until the index lands.

use serde::{Deserialize, Serialize};

use crate::{
    descent::{Descent, DescentBoss, Stratum},
    ids::PlayerId,
    place::Place,
    space::Theme,
};

/// Which of several candidates of one kind (ordered outward from the traveller).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Pick {
    /// The `n`-th candidate outward (0 is the nearest); wraps both ways, so "previous" from
    /// the nearest is the farthest.
    Nth(i32),
    /// A candidate chosen by `seed` (the caller rolls it, so every peer picks the same one).
    Random(u32),
}

impl Pick {
    /// The nearest candidate.
    pub const NEAREST: Self = Self::Nth(0);

    /// This pick's index into `count` candidates (`None` when there are none).
    #[must_use]
    pub fn index(self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let count_i64 = i64::try_from(count).ok()?;
        let at = match self {
            Self::Nth(n) => i64::from(n).rem_euclid(count_i64),
            Self::Random(seed) => i64::from(seed).rem_euclid(count_i64),
        };
        usize::try_from(at).ok()
    }
}

/// A place a test-mode rig can travel to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Destination {
    /// The top of a Descent (a fresh pocket just under its first stratum's top).
    Descent(Descent),
    /// `rows` rows (metres) below the ground at the rig's column.
    Depth(i32),
    /// The top of a stratum.
    Stratum(Stratum),
    /// The floor of the Descent a gate boss guards (its arena's neighbourhood).
    Boss(DescentBoss),
    /// The home world's spawn point.
    Home,
    /// The Moon's top.
    Moon,
    /// A planet with the theme (any planet theme when `None`), picked outward from the rig's
    /// sector.
    Planet { theme: Option<Theme>, pick: Pick },
    /// A station (TODO-SEAM(w14-index): resolves to nothing until the index lands).
    Station { pick: Pick },
    /// Beside another player's rig.
    Player(PlayerId),
    /// An exact tile of a place (the palette's back-stack and bookmarks).
    Spot { place: Place, column: i64, row: i64 },
}

/// Where a [`Destination`] lands, as the world resolved it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrivalTarget {
    /// A pocket carved at the top `row` of the home world (column: the rig's, or the nearest
    /// clear one).
    HomeRow { row: i32 },
    /// A place's arrival point (the existing place warp).
    Place(Place),
    /// An exact tile of a place.
    Spot { place: Place, column: i64, row: i64 },
}

/// A resolved destination: where it lands and what to call it (the palette's preview card).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Arrival {
    /// Short name ("The Living Dark", "ice planet Kessa IV").
    pub name: String,
    /// One line of facts: depth or coordinates, Descent/stratum or theme.
    pub detail: String,
    pub target: ArrivalTarget,
}

/// Every travel kind's word, for help text and completion (the palette's `tp <kind>`).
pub const DESTINATION_WORDS: [&str; 10] = [
    "planet", "descent", "depth", "stratum", "boss", "station", "player", "home", "moon", "back",
];
