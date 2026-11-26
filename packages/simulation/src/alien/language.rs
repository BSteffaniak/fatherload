//! Learning the species' tongues (Wave 12 aliens track).
//!
//! Each player understands each species' tongue to a degree: [`Understanding`] per mille, saved
//! and replicated per player ([`super::state::AlienLog`]). It grows by talking (once a day per
//! resident), trading, gifts they like, lessons at a town's lexicon house, lexicon stones dug out
//! from under their towns, primers bought from their traders, and finished quests.
//!
//! Speech renders through [`speech_tokens`]: every word of a line has a difficulty (short,
//! common words come first; long ones last; names and numbers are always clear), and a word
//! reads as itself once the player's understanding reaches it, else as the species' glyphs.
//! So a first meeting is all glyphs, a few days in the little words show through, and a fluent
//! speaker reads everything. The same line, the same understanding and the same species always
//! render the same way on every peer.

use serde::{Deserialize, Serialize};

use super::AlienSpecies;

/// Understanding of a tongue, per mille (0 nothing, 1000 all of it).
pub type Understanding = u16;

/// The most understanding there is.
pub const FULL_UNDERSTANDING: Understanding = 1_000;

/// Understanding a first meeting gives (you learn their name for themselves and a greeting).
pub const FIRST_CONTACT_UNDERSTANDING: Understanding = 40;
/// Understanding a day's first chat with one resident gives.
pub const CHAT_UNDERSTANDING: Understanding = 22;
/// Understanding a trade gives (once a day per town).
pub const TRADE_UNDERSTANDING: Understanding = 12;
/// Understanding a liked gift gives.
pub const GIFT_UNDERSTANDING: Understanding = 18;
/// Understanding a lesson at the lexicon house gives (one a day per species).
pub const LESSON_UNDERSTANDING: Understanding = 70;
/// Credits a lesson costs.
pub const LESSON_PRICE: u32 = crate::endgame::economy::space_credits(60);
/// Understanding a lexicon stone dug out gives.
pub const LEXICON_STONE_UNDERSTANDING: Understanding = 110;
/// Understanding a finished quest chapter gives.
pub const QUEST_UNDERSTANDING: Understanding = 60;

/// How well a player speaks a tongue.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Fluency {
    /// Nothing but glyphs.
    #[default]
    Glyphs,
    /// A few words show through.
    Fragments,
    /// Most everyday talk reads.
    Conversant,
    /// Nearly everything.
    Fluent,
    /// Every word.
    Native,
}

impl Fluency {
    /// Every level, lowest first.
    pub const ALL: [Self; 5] = [
        Self::Glyphs,
        Self::Fragments,
        Self::Conversant,
        Self::Fluent,
        Self::Native,
    ];

    /// The level of `understanding`.
    #[must_use]
    pub const fn of(understanding: Understanding) -> Self {
        match understanding {
            0..150 => Self::Glyphs,
            150..400 => Self::Fragments,
            400..700 => Self::Conversant,
            700..950 => Self::Fluent,
            _ => Self::Native,
        }
    }

    /// The understanding the level starts at.
    #[must_use]
    pub const fn threshold(self) -> Understanding {
        match self {
            Self::Glyphs => 0,
            Self::Fragments => 150,
            Self::Conversant => 400,
            Self::Fluent => 700,
            Self::Native => 950,
        }
    }

    /// Its name on the language panel.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Glyphs => "Only glyphs",
            Self::Fragments => "Fragments",
            Self::Conversant => "Conversant",
            Self::Fluent => "Fluent",
            Self::Native => "Like a native",
        }
    }

    /// Its index (events carry it).
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Glyphs => 0,
            Self::Fragments => 1,
            Self::Conversant => 2,
            Self::Fluent => 3,
            Self::Native => 4,
        }
    }
}

/// One word of a rendered line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpeechToken {
    /// A word the player understands, as written (with its punctuation).
    Word(String),
    /// A word they do not: the species' glyphs for it (`0..glyphs` indexes into the species'
    /// writing), then the punctuation that followed it.
    Glyphs { glyphs: Vec<u8>, trailing: String },
}

impl SpeechToken {
    /// Whether the player reads it.
    #[must_use]
    pub const fn known(&self) -> bool {
        matches!(self, Self::Word(_))
    }
}

/// A splitmix step (stable hashing of words).
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A stable hash of a word's letters (case folded) for `species`.
fn word_hash(word: &str, species: AlienSpecies) -> u64 {
    let mut hash = mix(0x4C45_5849_434F_4E00 ^ u64::from(species.index()));
    for byte in word.bytes() {
        hash = mix(hash ^ u64::from(byte.to_ascii_lowercase()));
    }
    hash
}

/// How much understanding it takes to read `word` (its letters only) in `species`' tongue.
///
/// Names (capitalised words after the first of a sentence) and numbers read from the start.
#[must_use]
pub fn word_difficulty(word: &str, species: AlienSpecies, sentence_start: bool) -> Understanding {
    let letters = word.chars().filter(|c| c.is_alphanumeric()).count();
    if letters == 0 || word.chars().any(|c| c.is_ascii_digit()) {
        return 0;
    }
    if !sentence_start && word.chars().next().is_some_and(char::is_uppercase) {
        return 0;
    }
    let (low, high): (u64, u64) = match letters {
        1 | 2 => (40, 260),
        3 => (90, 420),
        4 | 5 => (180, 680),
        6 | 7 => (330, 860),
        _ => (480, 940),
    };
    let roll = word_hash(word, species) % (high - low + 1);
    #[allow(clippy::cast_possible_truncation, reason = "at most 940")]
    let difficulty = (low + roll) as Understanding;
    difficulty
}

/// The glyphs `species` writes `word` with (one glyph for every two letters, at least one).
#[must_use]
pub fn word_glyphs(word: &str, species: AlienSpecies) -> Vec<u8> {
    let letters = word.chars().filter(|c| c.is_alphanumeric()).count().max(1);
    let count = letters.div_ceil(2);
    let alphabet = u64::from(species.def().glyphs.max(1));
    let mut hash = word_hash(word, species);
    (0..count)
        .map(|_| {
            hash = mix(hash);
            #[allow(clippy::cast_possible_truncation, reason = "below the glyph count")]
            let glyph = (hash % alphabet) as u8;
            glyph
        })
        .collect()
}

/// `text` as a player with `understanding` of `species`' tongue reads it.
#[must_use]
pub fn speech_tokens(
    text: &str,
    species: AlienSpecies,
    understanding: Understanding,
) -> Vec<SpeechToken> {
    let mut tokens = Vec::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let body_end = raw
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_alphanumeric())
            .map_or(0, |(index, c)| index + c.len_utf8());
        let (body, trailing) = raw.split_at(body_end);
        let difficulty = word_difficulty(body, species, sentence_start);
        if understanding >= difficulty {
            tokens.push(SpeechToken::Word(raw.to_owned()));
        } else {
            tokens.push(SpeechToken::Glyphs {
                glyphs: word_glyphs(body, species),
                trailing: trailing.to_owned(),
            });
        }
        sentence_start = trailing.contains(['.', '!', '?', ':']);
    }
    tokens
}

/// The share (per mille) of `text`'s words a player with `understanding` reads.
#[must_use]
pub fn readable_permille(text: &str, species: AlienSpecies, understanding: Understanding) -> u32 {
    let tokens = speech_tokens(text, species, understanding);
    if tokens.is_empty() {
        return 1_000;
    }
    let known = tokens.iter().filter(|token| token.known()).count();
    u32::try_from(known * 1_000 / tokens.len()).unwrap_or(1_000)
}

/// `text` as plain text for a player with `understanding` (unknown words as `~~~`), for logs,
/// tests and the terminal.
#[must_use]
pub fn rendered_text(text: &str, species: AlienSpecies, understanding: Understanding) -> String {
    speech_tokens(text, species, understanding)
        .into_iter()
        .map(|token| match token {
            SpeechToken::Word(word) => word,
            SpeechToken::Glyphs { glyphs, trailing } => {
                let mut out = "~".repeat(glyphs.len());
                out.push_str(&trailing);
                out
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
