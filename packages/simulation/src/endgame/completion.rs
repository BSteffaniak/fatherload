//! Completion (Wave 13 endgame): how much of the whole game a player has seen and done.
//!
//! Ten categories, each a share of its goal (capped at the goal), and the total is their mean:
//!
//! | Category | Goal |
//! |---|---|
//! | Worlds landed on | [`WORLDS_GOAL`] different bodies |
//! | Planet themes | every one of the twelve kinds of world ([`Theme::PLANET`]) |
//! | Species met | every friendly species |
//! | Tongues | every species' tongue spoken fluently |
//! | Bosses | every Descent gate boss, every faction boss and every registered theme boss |
//! | Story | the Fathers' eighty logs, the Heart woken, and the galaxy story's ten chapters |
//! | Codex | every space material catalogued (mined at least once) |
//! | Ship | every ship line at tier 5, the Starwright ship and the Heartline Drive |
//! | Megastructure and homes | the Halcyon Gate's eight parts and an estate with every wing |
//! | Achievements | every other achievement |
//!
//! The figures come from the world ([`crate::WorldState::completion`]); this module is pure.

use crate::space::Theme;

/// Worlds a full record has landed on.
pub const WORLDS_GOAL: u16 = 25;

/// One line of the completion screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Category {
    pub name: &'static str,
    pub done: u32,
    pub goal: u32,
}

impl Category {
    /// Its share, 0-1000.
    #[must_use]
    pub fn per_mille(self) -> u32 {
        if self.goal == 0 {
            return 1_000;
        }
        self.done.min(self.goal) * 1_000 / self.goal
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.done >= self.goal
    }
}

/// What the world knows of a player (gathered by [`crate::WorldState::completion`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompletionFacts {
    pub worlds: u16,
    pub themes: u8,
    pub species_met: u8,
    pub species_total: u8,
    pub tongues: u8,
    pub descent_bosses: u8,
    pub faction_bosses: u8,
    pub theme_bosses: u8,
    pub theme_bosses_total: u8,
    pub logs: u8,
    pub logs_total: u8,
    pub cable_lit: bool,
    /// The galaxy story's chapters done and in all.
    pub story_done: u8,
    pub story_total: u8,
    pub catalogued: u8,
    pub lines_maxed: u8,
    pub starwright_parts: u8,
    pub heartline: bool,
    pub gate_parts: u8,
    pub estate_full: bool,
    /// Achievements earned (not counting the completionist's own) and in all.
    pub achievements: u16,
    pub achievements_total: u16,
}

/// A player's completion.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Completion {
    pub categories: Vec<Category>,
}

impl Completion {
    /// The completion of `facts`.
    #[must_use]
    pub fn of(facts: &CompletionFacts) -> Self {
        let line = |name, done: u32, goal: u32| Category { name, done, goal };
        let descent_bosses = u32::try_from(crate::descent::DescentBoss::ALL.len()).unwrap_or(8);
        let materials = u32::try_from(super::materials::SPACE_MATERIALS.len()).unwrap_or(8);
        let lines = u32::try_from(crate::station::ShipLine::ALL.len()).unwrap_or(5);
        let planets = u32::try_from(Theme::PLANET.len()).unwrap_or(12);
        Self {
            categories: vec![
                line(
                    "Worlds landed",
                    u32::from(facts.worlds),
                    u32::from(WORLDS_GOAL),
                ),
                line("Kinds of world", u32::from(facts.themes), planets),
                line(
                    "Species met",
                    u32::from(facts.species_met),
                    u32::from(facts.species_total),
                ),
                line(
                    "Tongues spoken",
                    u32::from(facts.tongues),
                    u32::from(facts.species_total),
                ),
                line(
                    "Bosses beaten",
                    u32::from(facts.descent_bosses)
                        + u32::from(facts.faction_bosses)
                        + u32::from(facts.theme_bosses),
                    descent_bosses + 3 + u32::from(facts.theme_bosses_total),
                ),
                line(
                    "Story",
                    u32::from(facts.logs)
                        + u32::from(facts.cable_lit)
                        + u32::from(facts.story_done),
                    u32::from(facts.logs_total) + 1 + u32::from(facts.story_total),
                ),
                line("Codex", u32::from(facts.catalogued), materials),
                line(
                    "Ship",
                    u32::from(facts.lines_maxed)
                        + u32::from(facts.starwright_parts)
                        + u32::from(facts.heartline),
                    lines + 3,
                ),
                line(
                    "Gate and estate",
                    u32::from(facts.gate_parts) + u32::from(facts.estate_full),
                    u32::from(super::works::GATE_PARTS) + 1,
                ),
                line(
                    "Achievements",
                    u32::from(facts.achievements),
                    u32::from(facts.achievements_total),
                ),
            ],
        }
    }

    /// The whole, 0-1000 (the mean of the categories).
    #[must_use]
    pub fn per_mille(&self) -> u32 {
        let count = u32::try_from(self.categories.len()).unwrap_or(1).max(1);
        self.categories
            .iter()
            .map(|category| category.per_mille())
            .sum::<u32>()
            / count
    }

    /// Whether everything is done.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.categories.iter().all(|category| category.complete())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_is_the_mean_of_its_categories_and_full_only_when_all_are() {
        let empty = Completion::of(&CompletionFacts {
            species_total: 3,
            logs_total: 80,
            achievements_total: 60,
            ..CompletionFacts::default()
        });
        assert_eq!(empty.per_mille(), 0);
        assert!(!empty.complete());
        let full = CompletionFacts {
            worlds: 30,
            themes: 12,
            species_met: 3,
            species_total: 3,
            tongues: 3,
            descent_bosses: 8,
            faction_bosses: 3,
            logs: 80,
            logs_total: 80,
            cable_lit: true,
            catalogued: 8,
            lines_maxed: 5,
            starwright_parts: 2,
            heartline: true,
            gate_parts: 8,
            estate_full: true,
            achievements: 60,
            achievements_total: 60,
            ..CompletionFacts::default()
        };
        let done = Completion::of(&full);
        assert_eq!(done.per_mille(), 1_000);
        assert!(done.complete());
        // A registered theme boss not yet beaten keeps it open.
        let open = Completion::of(&CompletionFacts {
            theme_bosses_total: 1,
            ..full
        });
        assert!(!open.complete());
        assert!(open.per_mille() < 1_000);
    }
}
