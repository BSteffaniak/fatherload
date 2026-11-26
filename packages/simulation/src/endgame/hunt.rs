//! The hunt list (Wave 13 endgame): every boss of the galaxy in one list.
//!
//! The three faction bosses (Wave 12 hostiles: the Gilded Maw, the Swarm Queen, the Arbiter,
//! each in a lair the nav chart can point at) and the theme worlds' bosses, which the theme
//! tracks register in [`THEME_BOSSES`] (TODO-SEAM(w13-planets-c/d, w12 theme worlds): append
//! an entry per theme boss and call [`crate::WorldState::record_theme_boss`] when it falls).
//! The list, the completion screen and the achievements read the same registry, so a theme
//! boss counts the moment it is registered.

use serde::{Deserialize, Serialize};

use crate::{hostiles::Faction, space::Theme};

/// A theme world's boss (registered by its theme track).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeBoss {
    /// Its planet theme's id ([`crate::planet::PlanetThemeId`]'s number).
    pub theme: u16,
    /// The space theme of the worlds it rules (for the list's where-to line).
    pub world: Theme,
    pub name: &'static str,
}

/// The theme bosses (TODO-SEAM: empty until the theme tracks register theirs).
pub const THEME_BOSSES: &[ThemeBoss] = &[];

/// One entry of the hunt list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Quarry {
    Faction(Faction),
    /// A theme boss, by planet theme id.
    Theme(u16),
}

impl Quarry {
    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Faction(faction) => faction_boss_name(faction),
            Self::Theme(theme) => THEME_BOSSES
                .iter()
                .find(|boss| boss.theme == theme)
                .map_or("Unknown", |boss| boss.name),
        }
    }

    /// Where to look (a line for the list).
    #[must_use]
    pub fn whereabouts(self) -> String {
        match self {
            Self::Faction(faction) => format!("Lair of {}", faction.name()),
            Self::Theme(theme) => THEME_BOSSES
                .iter()
                .find(|boss| boss.theme == theme)
                .map_or_else(String::new, |boss| {
                    format!("On a {} world", boss.world.name())
                }),
        }
    }

    /// Wire code (events): factions 0-2, theme bosses 100 + theme id.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Faction(faction) => faction.code(),
            Self::Theme(theme) => 100 + theme as u32,
        }
    }
}

/// A faction boss's name.
#[must_use]
pub const fn faction_boss_name(faction: Faction) -> &'static str {
    match faction {
        Faction::Pirates => "The Gilded Maw",
        Faction::Swarm => "The Swarm Queen",
        Faction::Machine => "The Arbiter",
    }
}

/// The whole hunt list, in order: the faction bosses, then the theme bosses.
#[must_use]
pub fn hunt_list() -> Vec<Quarry> {
    Faction::ALL
        .into_iter()
        .map(Quarry::Faction)
        .chain(THEME_BOSSES.iter().map(|boss| Quarry::Theme(boss.theme)))
        .collect()
}
