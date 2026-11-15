//! What changes about a character during play: mood and their memory of each player.
//!
//! Time here is the world day (WP-PEOPLE derives it from the tick and tick rate), so mood
//! behaves the same at any tick rate. Every field has a serde default so saves stay loadable as
//! fields are added.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{PeopleRng, character::Character, salt, story::ArcId};
use crate::ids::PlayerId;

/// How many remembered events a character keeps per player (oldest go first).
pub const MEMORY_EVENTS: usize = 16;
/// How many recently spoken lines are avoided (the no-repeat window).
pub const NO_REPEAT_WINDOW: usize = 12;
/// Days a mood shift takes to fade completely.
pub const MOOD_FADE_DAYS: u32 = 4;

/// Something a character remembers about a player.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MemoryFact {
    /// They first met.
    Met,
    /// The player gave them a gift.
    Gift { liked: bool },
    /// They traded.
    Traded,
    /// The player finished one of their story arcs (`outcome` indexes the arc's outcomes).
    ArcCompleted { arc: ArcId, outcome: u8 },
    /// The player took on one of their arcs and walked away.
    ArcAbandoned { arc: ArcId },
    /// The player found someone (or what became of them) they cared about.
    FoundPerson { name: String, relation: String },
    /// The player rescued a neighbour of theirs.
    RescuedNeighbour { name: String },
    /// The player brought back an artifact they heard about.
    FoundArtifact { name: String },
    /// The player went deeper than anyone they know.
    ReachedDepth { metres: i32 },
    /// The player helped their village.
    HelpedVillage,
    /// The player was rude, broke a promise or wrecked something of theirs.
    Slighted,
}

impl MemoryFact {
    /// Reputation, favours, slights and mood a fact applies when it happens.
    #[must_use]
    pub const fn effect(&self) -> FactEffect {
        match self {
            Self::Met => FactEffect::new(1, 0, 0, 2),
            Self::Gift { liked: true } => FactEffect::new(4, 0, 0, 12),
            Self::Gift { liked: false } => FactEffect::new(0, 0, 0, -4),
            Self::Traded => FactEffect::new(1, 0, 0, 2),
            Self::ArcCompleted { .. } => FactEffect::new(15, 1, 0, 25),
            Self::ArcAbandoned { .. } => FactEffect::new(-8, 0, 1, -15),
            Self::FoundPerson { .. } => FactEffect::new(20, 1, 0, 30),
            Self::RescuedNeighbour { .. } | Self::HelpedVillage => FactEffect::new(10, 1, 0, 15),
            Self::FoundArtifact { .. } => FactEffect::new(5, 0, 0, 10),
            Self::ReachedDepth { .. } => FactEffect::new(2, 0, 0, 4),
            Self::Slighted => FactEffect::new(-12, 0, 1, -20),
        }
    }

    /// Whether dialogue can bring it up later.
    #[must_use]
    pub const fn is_memorable(&self) -> bool {
        !matches!(self, Self::Met | Self::Traded)
    }
}

/// See [`MemoryFact::effect`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactEffect {
    pub reputation: i16,
    pub favours: u16,
    pub slights: u16,
    pub mood: i16,
}

impl FactEffect {
    const fn new(reputation: i16, favours: u16, slights: u16, mood: i16) -> Self {
        Self {
            reputation,
            favours,
            slights,
            mood,
        }
    }
}

/// One remembered event.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct MemoryEvent {
    pub day: u32,
    pub fact: MemoryFact,
    /// Whether dialogue has brought it up already.
    #[serde(default)]
    pub mentioned: bool,
}

/// A character's memory of one player.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlayerMemory {
    /// `-100..=100`.
    #[serde(default)]
    pub reputation: i16,
    #[serde(default)]
    pub favours: u16,
    #[serde(default)]
    pub slights: u16,
    /// Conversations had.
    #[serde(default)]
    pub talks: u32,
    #[serde(default)]
    pub first_met_day: Option<u32>,
    #[serde(default)]
    pub last_talk_day: u32,
    /// Most recent last; at most [`MEMORY_EVENTS`].
    #[serde(default)]
    pub events: Vec<MemoryEvent>,
    /// Keys of recently spoken lines, most recent last; at most [`NO_REPEAT_WINDOW`].
    #[serde(default)]
    pub recent_lines: Vec<u64>,
}

/// How well a character knows and likes a player.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Standing {
    /// Never met.
    Stranger,
    /// Holds a grudge.
    Cold,
    /// Knows them a little.
    Acquainted,
    /// Likes them.
    Friendly,
    /// Would lend them their rig.
    Close,
}

impl PlayerMemory {
    #[must_use]
    pub const fn standing(&self) -> Standing {
        if self.talks == 0 && self.first_met_day.is_none() {
            Standing::Stranger
        } else if self.reputation <= -15 {
            Standing::Cold
        } else if self.reputation >= 50 {
            Standing::Close
        } else if self.reputation >= 15 {
            Standing::Friendly
        } else {
            Standing::Acquainted
        }
    }

    /// Records a spoken line in the no-repeat window.
    pub fn note_line(&mut self, key: u64) {
        self.recent_lines.retain(|recent| *recent != key);
        self.recent_lines.push(key);
        if self.recent_lines.len() > NO_REPEAT_WINDOW {
            self.recent_lines.remove(0);
        }
    }

    /// The most recent memorable event dialogue has not brought up yet.
    #[must_use]
    pub fn unmentioned(&self) -> Option<usize> {
        self.events
            .iter()
            .rposition(|event| event.fact.is_memorable() && !event.mentioned)
    }
}

/// Mood bands lines are chosen by.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MoodBand {
    Glum,
    Low,
    Content,
    Cheery,
    Elated,
}

impl MoodBand {
    /// The band a mood value (`-100..=100`) falls in.
    #[must_use]
    pub const fn of(mood: i16) -> Self {
        match mood {
            i16::MIN..=-35 => Self::Glum,
            -34..=-12 => Self::Low,
            -11..=11 => Self::Content,
            12..=34 => Self::Cheery,
            _ => Self::Elated,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Glum => "glum",
            Self::Low => "low",
            Self::Content => "content",
            Self::Cheery => "cheery",
            Self::Elated => "elated",
        }
    }
}

/// The mutable, saved state of one character.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CharacterState {
    /// A temporary mood shift from recent events, fading over [`MOOD_FADE_DAYS`].
    #[serde(default)]
    pub mood_shift: i16,
    /// The day `mood_shift` was last set.
    #[serde(default)]
    pub mood_day: u32,
    #[serde(default)]
    pub players: BTreeMap<PlayerId, PlayerMemory>,
}

impl CharacterState {
    /// What they remember of `player` (empty if they never met).
    #[must_use]
    pub fn memory(&self, player: PlayerId) -> PlayerMemory {
        self.players.get(&player).cloned().unwrap_or_default()
    }

    /// Records `fact` about `player` on `day`, applying its reputation and mood effect.
    pub fn remember(&mut self, player: PlayerId, day: u32, fact: MemoryFact) {
        let effect = fact.effect();
        let shift = self.current_shift(day);
        self.mood_shift = (shift + effect.mood).clamp(-60, 60);
        self.mood_day = day;
        let memory = self.players.entry(player).or_default();
        memory.reputation = (memory.reputation + effect.reputation).clamp(-100, 100);
        memory.favours = memory.favours.saturating_add(effect.favours);
        memory.slights = memory.slights.saturating_add(effect.slights);
        if fact == MemoryFact::Met {
            memory.first_met_day.get_or_insert(day);
        }
        memory.events.push(MemoryEvent {
            day,
            fact,
            mentioned: false,
        });
        if memory.events.len() > MEMORY_EVENTS {
            // Forget the oldest event, but never the first meeting while others remain.
            let oldest =
                usize::from(memory.events.len() > 1 && memory.events[0].fact == MemoryFact::Met);
            memory.events.remove(oldest);
        }
    }

    /// Adds a reputation change heard through the grapevine (see
    /// [`super::Population::reputation_ripple`]).
    pub fn hear_about(&mut self, player: PlayerId, delta: i16) {
        let memory = self.players.entry(player).or_default();
        memory.reputation = (memory.reputation + delta).clamp(-100, 100);
    }

    /// The mood shift left on `day`.
    #[must_use]
    pub fn current_shift(&self, day: u32) -> i16 {
        let elapsed = day.saturating_sub(self.mood_day);
        if elapsed >= MOOD_FADE_DAYS {
            return 0;
        }
        let left = i32::try_from(MOOD_FADE_DAYS - elapsed).unwrap_or(0);
        let fade = i32::try_from(MOOD_FADE_DAYS).unwrap_or(1);
        i16::try_from(i32::from(self.mood_shift) * left / fade).unwrap_or(0)
    }

    /// `character`'s mood (`-100..=100`) on `day` towards `player`: their resting mood, a daily
    /// wobble that swings wider for anxious people, recent events, and how they feel about the
    /// player.
    #[must_use]
    pub fn mood(&self, character: &Character, day: u32, player: Option<PlayerId>) -> i16 {
        let mut rng = PeopleRng::new(character.voice_seed, salt::MOOD, u64::from(day), 0);
        let swing = i32::from(character.personality.neuroticism) / 4 + 6;
        let wobble = rng.range(-swing, swing);
        let reputation = player
            .and_then(|player| self.players.get(&player))
            .map_or(0, |memory| i32::from(memory.reputation) / 5);
        let mood = i32::from(character.base_mood)
            + wobble
            + i32::from(self.current_shift(day))
            + reputation;
        i16::try_from(mood.clamp(-100, 100)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{CharacterState, MEMORY_EVENTS, MemoryFact, MoodBand, Standing};
    use crate::{
        ids::PlayerId,
        people::{CharacterId, generate_character, names::Culture},
    };

    #[test]
    fn memories_change_standing_and_mood_then_fade() {
        let character = generate_character(1, CharacterId::new(0, 0), Culture::Frontier);
        let player = PlayerId::new(1);
        let mut state = CharacterState::default();
        assert_eq!(state.memory(player).standing(), Standing::Stranger);
        let before = state.mood(&character, 10, Some(player));
        state.remember(player, 10, MemoryFact::Met);
        state.remember(
            player,
            10,
            MemoryFact::FoundPerson {
                name: "Tobin".into(),
                relation: "brother".into(),
            },
        );
        assert_eq!(state.memory(player).standing(), Standing::Friendly);
        assert!(state.mood(&character, 10, Some(player)) > before);
        assert_eq!(state.current_shift(10 + super::MOOD_FADE_DAYS), 0);
        state.remember(player, 20, MemoryFact::Slighted);
        state.remember(player, 20, MemoryFact::Slighted);
        state.remember(player, 20, MemoryFact::Slighted);
        assert_eq!(state.memory(player).standing(), Standing::Cold);
        assert_eq!(state.memory(player).slights, 3);
        for day in 0..40 {
            state.remember(player, 30 + day, MemoryFact::Traded);
        }
        let memory = state.memory(player);
        assert_eq!(memory.events.len(), MEMORY_EVENTS);
        assert_eq!(memory.events[0].fact, MemoryFact::Met);
    }

    #[test]
    fn mood_bands_cover_the_range() {
        assert_eq!(MoodBand::of(-100), MoodBand::Glum);
        assert_eq!(MoodBand::of(0), MoodBand::Content);
        assert_eq!(MoodBand::of(100), MoodBand::Elated);
    }
}
