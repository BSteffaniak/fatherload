//! What a tap or click on the Atlas lands on, carried through the menu pointer's row index
//! (`rendering::pointer` records each part as a "row"; the shell decodes the pick).

use super::AtlasCategory;

/// The Atlas's buttons.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AtlasButton {
    Go,
    Previous,
    Next,
    Random,
    Bookmark,
    /// Travel back to where the seat was before its last trip.
    Back,
    /// Map mode on or off.
    Map,
    Close,
}

impl AtlasButton {
    pub const ALL: [Self; 8] = [
        Self::Go,
        Self::Previous,
        Self::Next,
        Self::Random,
        Self::Bookmark,
        Self::Back,
        Self::Map,
        Self::Close,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Go => "Go",
            Self::Previous => "Prev",
            Self::Next => "Next",
            Self::Random => "Random",
            Self::Bookmark => "Bookmark",
            Self::Back => "Back",
            Self::Map => "Map",
            Self::Close => "Close",
        }
    }
}

/// A part of the Atlas under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AtlasHit {
    Category(usize),
    Button(AtlasButton),
    Search,
    /// A result row, by its index in the shown list.
    Row(usize),
    /// A point on the open map, `(u, v)` in 0..1.
    Map(f32, f32),
}

const BUTTON_BASE: usize = 100;
const SEARCH: usize = 200;
const ROW_BASE: usize = 1_000;
const MAP_BASE: usize = 1_000_000;
/// Map points are carried in steps of 1/`MAP_STEPS` each way.
pub const MAP_STEPS: usize = 4_096;

impl AtlasHit {
    /// The pointer row index standing for this hit.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "map steps are small whole numbers"
    )]
    pub fn encode(self) -> usize {
        match self {
            Self::Category(index) => index.min(BUTTON_BASE - 1),
            Self::Button(button) => {
                BUTTON_BASE
                    + AtlasButton::ALL
                        .iter()
                        .position(|candidate| *candidate == button)
                        .unwrap_or_default()
            }
            Self::Search => SEARCH,
            Self::Row(index) => ROW_BASE + index.min(MAP_BASE - ROW_BASE - 1),
            Self::Map(u, v) => {
                let step = |value: f32| {
                    ((value.clamp(0.0, 1.0) * (MAP_STEPS - 1) as f32).round() as usize)
                        .min(MAP_STEPS - 1)
                };
                MAP_BASE + step(v) * MAP_STEPS + step(u)
            }
        }
    }

    /// The hit a pointer row index stands for.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "map steps are small whole numbers"
    )]
    pub fn decode(row: usize) -> Option<Self> {
        if row >= MAP_BASE {
            let cell = row - MAP_BASE;
            let (v, u) = (cell / MAP_STEPS, cell % MAP_STEPS);
            if v >= MAP_STEPS {
                return None;
            }
            let scale = (MAP_STEPS - 1) as f32;
            return Some(Self::Map(u as f32 / scale, v as f32 / scale));
        }
        if row >= ROW_BASE {
            return Some(Self::Row(row - ROW_BASE));
        }
        if row == SEARCH {
            return Some(Self::Search);
        }
        if row >= BUTTON_BASE {
            return AtlasButton::ALL
                .get(row - BUTTON_BASE)
                .map(|button| Self::Button(*button));
        }
        (row < AtlasCategory::ALL.len()).then_some(Self::Category(row))
    }

    /// Whether `row` is a point on the map (hovering it moves the cursor).
    #[must_use]
    pub const fn is_map(row: usize) -> bool {
        row >= MAP_BASE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_survive_the_pointer_row_index() {
        for hit in [
            AtlasHit::Category(0),
            AtlasHit::Category(7),
            AtlasHit::Search,
            AtlasHit::Row(0),
            AtlasHit::Row(312),
        ]
        .into_iter()
        .chain(AtlasButton::ALL.map(AtlasHit::Button))
        {
            assert_eq!(AtlasHit::decode(hit.encode()), Some(hit));
        }
        let Some(AtlasHit::Map(u, v)) = AtlasHit::decode(AtlasHit::Map(0.25, 0.8).encode()) else {
            panic!("a map point");
        };
        assert!((u - 0.25).abs() < 1e-3 && (v - 0.8).abs() < 1e-3);
        assert!(AtlasHit::is_map(AtlasHit::Map(0.0, 0.0).encode()));
        assert_eq!(AtlasHit::decode(50), None, "between categories and buttons");
    }
}
