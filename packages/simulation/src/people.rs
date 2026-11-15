//! People and narrative framework (expansion plan §3 F3): procedural characters, the dialogue
//! system and story seeds.
//!
//! This is a pure, deterministic data + logic layer; villages place the people (WP-VILLAGE) and
//! `world/people.rs` walks them as NPC entities (WP-PEOPLE). The APIs:
//!
//! - **Characters** ([`character`]): [`generate_character`] builds a [`Character`] from
//!   `(world seed, CharacterId, Culture)` alone: a culture-flavoured name ([`names`]), an
//!   appearance seed, a [`Role`], Big-Five-style [`Personality`] traits plus [`Quirk`]s,
//!   [`Value`]s, a [`Want`] and a [`Fear`], history beats, a home [`VillageId`] and absent
//!   people they talk about (a missing brother, an old rival up the valley).
//! - **Populations** ([`population`]): [`generate_population`] generates a village's residents
//!   together, adding families (shared surnames, sensible ages) and a relationship graph
//!   (family, friends, rivals, debts, sweethearts).
//! - **Mutable state** ([`memory`]): a [`CharacterState`] per character holds what changes
//!   during play (mood shifts and a [`PlayerMemory`] per player: reputation, favours, slights,
//!   remembered events and recently spoken lines). It is the only part that needs saving; the
//!   generated character is rebuilt from its seed. It serializes with serde defaults so later
//!   fields keep old saves loading.
//! - **Dialogue** ([`dialogue`]): authored line sets ([`dialogue::lines`]) with a variant list per
//!   [`Tone`] bucket, a tiny template grammar (`{var}`, `{a var}`, `[one|two]`), selection by
//!   personality, mood, standing and memory, per-trait tone filters, and a no-repeat window.
//! - **Story seeds** ([`story`]): character needs, relationships, village problems, world facts
//!   and player history become [`StorySeed`]s that pick an [`ArcTemplate`] and bind its slots.
//!   Arc beats are made of the quest engine's own [`crate::quest::Objective`] primitives and
//!   [`crate::quest::TargetSpec`]s, so [`StorySeed::quest_def`] yields a
//!   [`crate::quest::QuestDef`] the existing binding and objective rules run unchanged.
//! - **Report** ([`report`]): [`report::story_report`] prints sampled characters, dialogue
//!   samples and story seeds for review (`fatherload --story-report`).
//!
//! People in the world (WP-PEOPLE, wave 3) build on these:
//!
//! - **Clock** ([`clock`]): [`clock::ClockSource`], implemented by T14's world clock (what
//!   `WorldState::people_clock` reads) and by the tick-derived [`clock::TickClock`] (48-minute
//!   days, for tests and scenes).
//! - **Camps** ([`camps`]): wanderers (traders, prospectors, surveyors, smugglers) camped on
//!   the open surface between settlements, deterministic per slot of the surface.
//! - **Routines** ([`routine`]): a [`routine::Settlement`] (a village, the town or a camp)
//!   knows each resident's home, workplace and habits; [`routine::Settlement::stop_at`] says where they
//!   should be at any time of day.
//! - **Offers** ([`offers`]): each person's unique trade list, apprenticeship [`offers::Perk`]s,
//!   prices by standing and gift tastes.
//! - **Contractors** ([`contractor`]): the building trade as a person role, with quotes and a
//!   hire record for WP-HOME.
//! - **Ledger** ([`ledger`]): the world's saved, replicated memory of what players did with
//!   people, and the [`ledger::PeopleCommand`]s players act with.
//!
//! Everything is integer-only and hash-seeded ([`PeopleRng`]), so the same seed yields the same
//! people on every platform, host and client alike.

pub mod camps;
pub mod character;
pub mod clock;
#[cfg(test)]
mod content_tests;
pub mod contractor;
pub mod dialogue;
pub mod ledger;
pub mod memory;
pub mod names;
pub mod offers;
pub mod population;
pub mod report;
pub mod routine;
pub mod story;
pub mod traits;

pub use character::{
    AbsentPerson, Appearance, Character, CharacterId, HistoryBeat, OfferKind, Role, VillageId,
    WantFocus, generate_character,
};
pub use dialogue::{TalkContext, Topic, say, talk};
pub use memory::{CharacterState, MemoryEvent, MemoryFact, MoodBand, PlayerMemory, Standing};
pub use names::{Culture, PersonName};
pub use population::{
    Population, Relationship, RelationshipKind, generate_population, population_size,
};
pub use story::{
    ArcId, ArcTemplate, SeedSource, SlotBinding, StorySeed, VillageProblem, WorldFact,
    WorldFactKind, story_seeds,
};
pub use traits::{Fear, Personality, Quirk, Tone, Value, Want};

pub use clock::{ClockSource, DayTime, TickClock};
pub use ledger::{PeopleCommand, PeopleLedger, TalkTopic};

use crate::worldgen::noise::hash2;

/// Salts that keep each kind of roll independent.
pub(crate) mod salt {
    pub const CHARACTER: u64 = 0x5045_4F50_4C45_0001;
    pub const POPULATION: u64 = 0x5045_4F50_4C45_0002;
    pub const DIALOGUE: u64 = 0x5045_4F50_4C45_0003;
    pub const MOOD: u64 = 0x5045_4F50_4C45_0004;
    pub const STORY: u64 = 0x5045_4F50_4C45_0005;
    pub const VILLAGE_NAME: u64 = 0x5045_4F50_4C45_0006;
}

/// Small deterministic generator for people content (`SplitMix64` over a hashed start).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeopleRng(u64);

impl PeopleRng {
    /// A generator for one purpose (`salt`) of one subject (`a`, `b`) under `seed`.
    #[must_use]
    pub const fn new(seed: u64, salt: u64, a: u64, b: u64) -> Self {
        let mixed = hash2(seed ^ a.rotate_left(17), salt ^ b.rotate_left(41), 0, 0);
        Self(mixed)
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % (n as u64)).unwrap_or(0)
    }

    /// Whether a `percent` chance passes.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.next_u64() % 100 < u64::from(percent)
    }

    /// A value in `low..=high`.
    pub fn range(&mut self, low: i32, high: i32) -> i32 {
        if high <= low {
            return low;
        }
        let span = u64::from(high.abs_diff(low)) + 1;
        low + i32::try_from(self.next_u64() % span).unwrap_or(0)
    }

    /// One item of a non-empty slice.
    ///
    /// # Panics
    ///
    /// Panics on an empty slice (content tables are never empty; the lint tests check it).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// One item of a weighted, non-empty table.
    ///
    /// # Panics
    ///
    /// Panics on an empty table.
    pub fn weighted<'a, T>(&mut self, items: &'a [(T, u32)]) -> &'a T {
        let total: u64 = items.iter().map(|(_, weight)| u64::from(*weight)).sum();
        let mut roll = self.next_u64() % total.max(1);
        for (item, weight) in items {
            let weight = u64::from(*weight);
            if roll < weight {
                return item;
            }
            roll -= weight;
        }
        &items[items.len() - 1].0
    }
}

#[cfg(test)]
mod rng_tests {
    use super::PeopleRng;

    #[test]
    fn rng_is_deterministic_and_spreads() {
        let mut a = PeopleRng::new(7, 1, 2, 3);
        let mut b = PeopleRng::new(7, 1, 2, 3);
        let mut c = PeopleRng::new(7, 1, 2, 4);
        let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let again: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        let other: Vec<u64> = (0..8).map(|_| c.next_u64()).collect();
        assert_eq!(first, again);
        assert_ne!(first, other);
        let mut rng = PeopleRng::new(1, 2, 3, 4);
        let mut seen = [0_u32; 6];
        for _ in 0..6000 {
            seen[rng.below(6)] += 1;
        }
        assert!(seen.iter().all(|count| (800..1200).contains(count)));
        for _ in 0..200 {
            assert!((-3..=3).contains(&rng.range(-3, 3)));
        }
    }
}
