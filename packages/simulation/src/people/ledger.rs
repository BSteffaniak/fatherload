//! What play changes about people (WP-PEOPLE): the world's [`PeopleLedger`].
//!
//! Characters are rebuilt from the seed; only what happened is kept:
//!
//! - **Character state** ([`CharacterState`] per [`CharacterId`]): mood shifts and each
//!   character's memory of each player (reputation, favours, slights, remembered events, the
//!   no-repeat window of lines). Reputation is the affinity a relationship is measured in
//!   (`-100..=100`, banded into a [`super::Standing`]).
//! - **Player journals** ([`PersonJournal`] per [`PlayerId`]): the player's open conversation
//!   (what was just said, so every peer shows the same lines), one-of-a-kind offers already
//!   bought, apprenticeship perks learned, and when each person last shared a free rumour.
//!
//! Player ids survive parking and rejoining (WP-PERSIST), so a returning player is remembered
//! by everyone. The ledger saves with the world (serde defaults keep older saves loading) and
//! replicates as the public `People` section whenever it changes.
//!
//! Players act on people with [`PeopleCommand`]s (`PlayerCommand::People`), each naming the NPC
//! entity they address; `world/people.rs` validates reach and applies them inside the tick.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{CharacterId, CharacterState, offers::Perk};
use crate::{entity::EntityId, ids::PlayerId, item::ItemKind};

/// Lines kept in an open conversation (the newest last).
pub const CONVERSATION_LINES: usize = 6;

/// What a player can say to someone (the talk panel's choices).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum TalkTopic {
    /// Open a conversation, or carry on chatting (greeting and the next topic in rotation).
    Chat,
    /// Ask about rumours: a rumour line and, once a day per person, a site marked on the map.
    Rumours,
    /// Ask about their work and what they want out of life.
    Life,
    /// Ask a contractor about building work.
    BuildingWork,
}

/// A player's action toward a person (`PlayerCommand::People`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum PeopleCommand {
    /// Talk to the NPC entity `npc` about `topic`.
    Talk { npc: EntityId, topic: TalkTopic },
    /// Buy row `offer` of `npc`'s trade list ([`super::offers::offers_of`]).
    Trade { npc: EntityId, offer: u8 },
    /// Hand one `item` from the cargo bay to `npc`.
    Gift { npc: EntityId, item: ItemKind },
    /// End the open conversation.
    Goodbye,
}

/// Who said a conversation line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Speaker {
    /// The person.
    Them,
    /// The player (their choice, echoed).
    You,
    /// A note from the game ("Marked on your map: ...").
    Note,
}

/// One line of a conversation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConversationLine {
    pub speaker: Speaker,
    pub text: String,
}

/// A player's open conversation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Conversation {
    /// The NPC entity they are talking to.
    pub npc: EntityId,
    pub character: CharacterId,
    /// The newest last, at most [`CONVERSATION_LINES`].
    pub lines: Vec<ConversationLine>,
    /// Counts every exchange (presentation notices new lines by it).
    pub exchange: u32,
}

impl Conversation {
    /// Appends lines, keeping the newest [`CONVERSATION_LINES`].
    pub fn push(&mut self, lines: impl IntoIterator<Item = ConversationLine>) {
        self.lines.extend(lines);
        let excess = self.lines.len().saturating_sub(CONVERSATION_LINES);
        if excess > 0 {
            self.lines.drain(..excess);
        }
        self.exchange = self.exchange.wrapping_add(1);
    }
}

/// What one player has done with people.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PersonJournal {
    #[serde(default)]
    pub conversation: Option<Conversation>,
    /// One-of-a-kind offers bought: `(person, offer row)`.
    #[serde(default)]
    pub bought: BTreeSet<(CharacterId, u8)>,
    #[serde(default)]
    pub perks: BTreeSet<Perk>,
    /// The world day each person last shared a free rumour with this player.
    #[serde(default, with = "pairs")]
    pub rumour_days: BTreeMap<CharacterId, u32>,
    /// The world day each person last counted a chat as goodwill.
    #[serde(default, with = "pairs")]
    pub chat_days: BTreeMap<CharacterId, u32>,
}

impl PersonJournal {
    /// Whether the player learned `perk`.
    #[must_use]
    pub fn has(&self, perk: Perk) -> bool {
        self.perks.contains(&perk)
    }
}

/// Everything play changed about people.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PeopleLedger {
    #[serde(default, with = "pairs")]
    pub characters: BTreeMap<CharacterId, CharacterState>,
    #[serde(default)]
    pub players: BTreeMap<PlayerId, PersonJournal>,
}

impl PeopleLedger {
    /// Whether nothing has happened yet (worlds without people hash as before).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty() && self.players.is_empty()
    }

    /// `character`'s state (empty if nothing happened with them).
    #[must_use]
    pub fn character(&self, character: CharacterId) -> CharacterState {
        self.characters.get(&character).cloned().unwrap_or_default()
    }

    /// `player`'s journal (empty if they never met anyone).
    #[must_use]
    pub fn journal(&self, player: PlayerId) -> Option<&PersonJournal> {
        self.players.get(&player)
    }

    /// `player`'s open conversation.
    #[must_use]
    pub fn conversation(&self, player: PlayerId) -> Option<&Conversation> {
        self.players.get(&player)?.conversation.as_ref()
    }

    /// How `character` regards `player`: their reputation, `-100..=100`.
    #[must_use]
    pub fn affinity(&self, character: CharacterId, player: PlayerId) -> i16 {
        self.characters
            .get(&character)
            .and_then(|state| state.players.get(&player))
            .map_or(0, |memory| memory.reputation)
    }

    /// How `character` regards `player` as a [`super::Standing`].
    #[must_use]
    pub fn standing(&self, character: CharacterId, player: PlayerId) -> super::Standing {
        self.characters
            .get(&character)
            .map_or(super::Standing::Stranger, |state| {
                state.memory(player).standing()
            })
    }

    /// Whether `player` has learned `perk`.
    #[must_use]
    pub fn has_perk(&self, player: PlayerId, perk: Perk) -> bool {
        self.players
            .get(&player)
            .is_some_and(|journal| journal.has(perk))
    }

    /// Ends `player`'s open conversation (returns the NPC they were talking to).
    pub fn end_conversation(&mut self, player: PlayerId) -> Option<EntityId> {
        self.players
            .get_mut(&player)?
            .conversation
            .take()
            .map(|conversation| conversation.npc)
    }
}

/// Serializes a map with struct keys as a list of pairs (JSON object keys must be strings).
mod pairs {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize,
        V: Serialize,
        S: Serializer,
    {
        serializer.collect_seq(map.iter())
    }

    pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K, V)>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::{PeopleLedger, PersonJournal};
    use crate::{
        ids::PlayerId,
        people::{CharacterId, MemoryFact},
    };

    #[test]
    fn the_ledger_round_trips_through_json() {
        let mut ledger = PeopleLedger::default();
        let who = CharacterId::new(5, 2);
        ledger
            .characters
            .entry(who)
            .or_default()
            .remember(PlayerId::new(1), 3, MemoryFact::Met);
        let mut journal = PersonJournal::default();
        journal.rumour_days.insert(who, 3);
        journal.bought.insert((who, 0));
        ledger.players.insert(PlayerId::new(1), journal);
        let json = serde_json::to_string(&ledger).expect("json");
        let back: PeopleLedger = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, ledger);
        assert_eq!(back.affinity(who, PlayerId::new(1)), 1);
    }
}
