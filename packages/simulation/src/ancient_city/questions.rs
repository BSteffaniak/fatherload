//! Asking the archivist (Wave 11 carryover): the questions a player may put to its companion
//! and the answers' line ids.
//!
//! Wave 10 gave the companion a voice ([`crate::expedition::archivist`]) but no ears: it only
//! spoke unprompted. Now a player asks it ([`crate::logistics::LogisticsCommand::AskArchivist`],
//! from the game's "Ask Index" page by keys, pad, mouse and touch, per seat) about:
//!
//! * **nearby ore** ([`ArchivistQuestion::NearbyOre`]): it scans wider than its own sweep
//!   ([`ASK_SCAN_REACH_TILES`]), marks the best and says what and which way;
//! * **the crew's finds** ([`ArchivistQuestion::Finds`]): the last of the Fathers' logs read and
//!   how many stars the crew has charted;
//! * **lore** ([`ArchivistQuestion::Lore`]): any open topic of the story's
//!   [`crate::expedition::archivist::topics`] (the Descents reached, the Fathers and each of
//!   them, the Heart, the stars, after the end), answered with the story's own lines in turn.
//!
//! An answer is a line id like every other line of the archivist (it travels in the
//! `ArchivistSpoke` event and in [`super::archivist::Archivist::line`]), so the speech box shows
//! it on every seat and peer alike. Story lines keep their ids past
//! [`super::archivist::STORY_LINE_BASE`]; the ore and finds answers are ids whose words are a
//! pure function of the id ([`answer_words`]), so nothing new needs to be replicated.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{
    expedition::{
        ExpeditionState,
        archivist::{ArchivistTopic, topics},
        logs::LOGS,
    },
    star_chart::MAX_CHARTS,
    terrain::{MineralKind, TilePosition},
};

/// How far it scans when asked about ore (tiles): wider than its own sweep.
pub const ASK_SCAN_REACH_TILES: i32 = 16;
/// The answer when nothing worth the drill lies within reach.
pub const NO_ORE_ANSWER: u16 = 1_999;
/// Ore answers: `ORE_ANSWER_BASE + kind * 9 + direction` (see [`ore_answer`]).
pub const ORE_ANSWER_BASE: u16 = 2_000;
/// Finds answers: `FINDS_ANSWER_BASE + logs read * (MAX_CHARTS + 1) + charts`.
pub const FINDS_ANSWER_BASE: u16 = 3_000;

/// A question for the archivist.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ArchivistQuestion {
    /// "Anything worth digging near here?"
    NearbyOre,
    /// "What have we found so far?"
    Finds,
    /// A topic of the story's lore.
    Lore(ArchivistTopic),
}

impl ArchivistQuestion {
    /// What the "Ask Index" page calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NearbyOre => "Nearby ore",
            Self::Finds => "Our finds",
            Self::Lore(topic) => topic.label(),
        }
    }

    /// The question as the player puts it (the page's footer).
    #[must_use]
    pub fn asked(self) -> String {
        match self {
            Self::NearbyOre => "Anything worth digging near here?".to_owned(),
            Self::Finds => "What have we found so far?".to_owned(),
            Self::Lore(ArchivistTopic::Descent(descent)) => {
                format!("What did the builders make in {}?", descent.name())
            }
            Self::Lore(ArchivistTopic::Member(member)) => {
                format!("Tell me about {}.", member.name())
            }
            Self::Lore(ArchivistTopic::Fathers) => "Tell me about the Fathers.".to_owned(),
            Self::Lore(ArchivistTopic::Heart) => "What is the Heart?".to_owned(),
            Self::Lore(ArchivistTopic::Stars) => "What are the star charts for?".to_owned(),
            Self::Lore(ArchivistTopic::AfterTheEnd) => "What now?".to_owned(),
        }
    }
}

/// The questions a crew with `state` can ask, in the page's order: ore, finds, then every open
/// lore topic.
#[must_use]
pub fn questions(state: &ExpeditionState) -> Vec<ArchivistQuestion> {
    [ArchivistQuestion::NearbyOre, ArchivistQuestion::Finds]
        .into_iter()
        .chain(topics(state).into_iter().map(ArchivistQuestion::Lore))
        .collect()
}

/// What the ore answer names (an ore, a builder ore or a cache), by its code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OreFind {
    Ore(MineralKind),
    AlloyOre,
    EngineCore,
    StarGlass,
    Cache,
}

/// The minerals in code order.
const MINERALS: [MineralKind; 12] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
    MineralKind::Skystone,
    MineralKind::CloudSilver,
];

impl OreFind {
    const fn code(self) -> u16 {
        match self {
            Self::Ore(mineral) => {
                let mut index = 0;
                while index < MINERALS.len() {
                    if MINERALS[index] as u8 == mineral as u8 {
                        #[allow(clippy::cast_possible_truncation, reason = "12 minerals")]
                        return index as u16;
                    }
                    index += 1;
                }
                0
            }
            Self::AlloyOre => 12,
            Self::EngineCore => 13,
            Self::StarGlass => 14,
            Self::Cache => 15,
        }
    }

    fn of_code(code: u16) -> Option<Self> {
        Some(match code {
            12 => Self::AlloyOre,
            13 => Self::EngineCore,
            14 => Self::StarGlass,
            15 => Self::Cache,
            _ => Self::Ore(*MINERALS.get(usize::from(code))?),
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Ore(mineral) => mineral.name(),
            Self::AlloyOre => "Ancient alloy",
            Self::EngineCore => "Engine core ore",
            Self::StarGlass => "Star glass",
            Self::Cache => "A builders' cache",
        }
    }
}

/// Which way from the asker a find lies: 0 right here, then up, up-right, right, down-right,
/// down, down-left, left, up-left.
#[must_use]
pub const fn direction(from: TilePosition, to: TilePosition) -> u16 {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx.abs() <= 1 && dy.abs() <= 1 {
        return 0;
    }
    // Mostly one way when the other is under half of it.
    let horizontal = if dx.abs() * 2 < dy.abs() {
        0
    } else {
        dx.signum()
    };
    let vertical = if dy.abs() * 2 < dx.abs() {
        0
    } else {
        dy.signum()
    };
    match (horizontal, vertical) {
        (0, -1) => 1,
        (1, -1) => 2,
        (1, 0) => 3,
        (1, 1) => 4,
        (0, 1) => 5,
        (-1, 1) => 6,
        (-1, 0) => 7,
        _ => 8,
    }
}

const fn direction_words(direction: u16) -> &'static str {
    match direction {
        0 => "right under your treads",
        1 => "straight above you",
        2 => "up and to the right",
        3 => "off to the right",
        4 => "down and to the right",
        5 => "straight below you",
        6 => "down and to the left",
        7 => "off to the left",
        _ => "up and to the left",
    }
}

/// The ore answer's id for `find` lying `direction` ([`direction`]) from the asker.
#[must_use]
pub const fn ore_answer(find: OreFind, direction: u16) -> u16 {
    ORE_ANSWER_BASE + find.code() * 9 + if direction > 8 { 8 } else { direction }
}

/// The finds answer's id for a crew that has read `logs` of the Fathers' logs and charted
/// `charts` stars.
#[must_use]
pub fn finds_answer(logs: u8, charts: usize) -> u16 {
    let logs = u16::from(logs.min(80));
    let charts = u16::try_from(charts.min(MAX_CHARTS)).unwrap_or(0);
    let per_log = u16::try_from(MAX_CHARTS + 1).unwrap_or(u16::MAX);
    FINDS_ANSWER_BASE + logs * per_log + charts
}

/// The words of an answer id ([`NO_ORE_ANSWER`], an ore answer or a finds answer), else `None`.
#[must_use]
pub fn answer_words(id: u16) -> Option<Cow<'static, str>> {
    if id == NO_ORE_ANSWER {
        return Some(Cow::Borrowed(
            "Nothing worth the drill within sixteen tiles. The good seams are further on. They \
             usually are.",
        ));
    }
    let per_log = u16::try_from(MAX_CHARTS + 1).ok()?;
    if id >= FINDS_ANSWER_BASE {
        let offset = id - FINDS_ANSWER_BASE;
        let (logs, charts) = (offset / per_log, offset % per_log);
        if logs > 80 {
            return None;
        }
        let trail = usize::from(logs)
            .checked_sub(1)
            .and_then(|index| LOGS.get(index))
            .map_or_else(
                || "None of the Fathers' logs yet.".to_owned(),
                |log| {
                    format!(
                        "{logs} of the Fathers' eighty logs. The last you read was \"{}\", in \
                         {}'s hand.",
                        log.title,
                        log.author.name()
                    )
                },
            );
        let sky = match charts {
            0 => " No stars charted. The Archive of Stars keeps them under lock.".to_owned(),
            1 => " One star charted. A beginning.".to_owned(),
            n => format!(" {n} stars charted. The roads past the Moon are filling in."),
        };
        return Some(Cow::Owned(format!("We have found {trail}{sky}")));
    }
    if id >= ORE_ANSWER_BASE {
        let offset = id - ORE_ANSWER_BASE;
        let find = OreFind::of_code(offset / 9)?;
        let way = direction_words(offset % 9);
        return Some(Cow::Owned(format!(
            "{}, {way}. I have marked it for you.",
            find.name()
        )));
    }
    None
}

/// Whether `id` is one of the asked answers (the ore and finds ids).
#[must_use]
pub const fn is_answer(id: u16) -> bool {
    id >= NO_ORE_ANSWER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_answer_id_has_words_and_ids_do_not_overlap() {
        assert!(answer_words(NO_ORE_ANSWER).is_some());
        for code in 0..16 {
            let find = OreFind::of_code(code).expect("a find");
            assert_eq!(find.code(), code);
            for way in 0..9 {
                let id = ore_answer(find, way);
                assert!(id < FINDS_ANSWER_BASE);
                let words = answer_words(id).expect("words");
                assert!(words.starts_with(find.name()), "{words}");
            }
        }
        // Fits a u16 with every log and the whole chart list.
        let last = finds_answer(80, MAX_CHARTS);
        assert!(last > ORE_ANSWER_BASE + 16 * 9);
        let words = answer_words(last).expect("words");
        assert!(
            words.contains("Wake It") && words.contains("512 stars"),
            "{words}"
        );
        let none = answer_words(finds_answer(0, 0)).expect("words");
        assert!(none.contains("None of the Fathers' logs"), "{none}");
        assert!(
            answer_words(finds_answer(3, 1))
                .expect("words")
                .contains("One star")
        );
        // The story's lines and the archivist's own are not answers.
        assert!(!is_answer(super::super::archivist::STORY_LINE_BASE + 29));
    }

    #[test]
    fn directions_read_the_way_a_player_would_say_them() {
        let here = TilePosition { x: 0, y: 0 };
        let at = |x, y| direction(here, TilePosition { x, y });
        assert_eq!(at(1, 1), 0);
        assert_eq!(at(0, -9), 1);
        assert_eq!(at(6, -7), 2);
        assert_eq!(at(9, 1), 3);
        assert_eq!(at(0, 9), 5);
        assert_eq!(at(-9, 2), 7);
        assert_eq!(at(-5, -6), 8);
    }

    #[test]
    fn questions_start_with_ore_and_finds_and_open_lore_with_the_trail() {
        let state = ExpeditionState::default();
        let early = questions(&state);
        assert_eq!(
            &early[..2],
            &[ArchivistQuestion::NearbyOre, ArchivistQuestion::Finds]
        );
        let late = questions(&ExpeditionState {
            found: 80,
            descents: 0xFF,
            cable_lit: true,
            ..ExpeditionState::default()
        });
        assert!(late.len() > early.len());
        assert!(late.contains(&ArchivistQuestion::Lore(ArchivistTopic::AfterTheEnd)));
        for question in late {
            assert!(!question.label().is_empty() && question.asked().ends_with(['?', '.']));
        }
    }
}
