//! What the builders' archivist says (Wave 10, expansion plan §14, Descent VII).
//!
//! The archivist is the friendly thinking machine of the Buried Megacity that kept the city's
//! lamps for an age, met the Fathers, learned the town's language from Ines Calloway and becomes
//! the crew's companion.
//!
//! The Ancients track builds the companion (its body, where it follows, when it speaks). This
//! module is its **voice**: authored lines with stable ids on the Descents, each of the Fathers,
//! the Heart, the stars and the time after the ending, each opened by the trail
//! ([`ArchivistLine::after_log`]), the Descents the crew has reached or the cable lit, so it
//! never spoils what the crew has not found.
//!
//! # Registration seam
//!
//! * [`remark`]: what the companion says unprompted where the crew is (a line about the
//!   Descent at hand, otherwise about the Fathers, in turn).
//! * [`answer`]: the line for a topic the player asks about ([`topics`] lists the open ones,
//!   for a talk menu that works by keys, pad, mouse and touch).
//! * [`remark_from`] and [`answer_from`] take extra tables: the companion track passes its own
//!   lines (or any later wave's) alongside [`ARCHIVIST_LINES`], and they take part in the
//!   same rotation and gating. Ids are `archivist.topic.name`, never renamed or reused.
//!
//! Everything reads [`super::ExpeditionState`] only (saved and replicated), so every seat and
//! partner hears the same companion.

use super::{ExpeditionState, Member};
use crate::descent::Descent;

/// What a line is about.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArchivistTopic {
    /// A Descent: what the builders made there (opens once the crew reached it).
    Descent(Descent),
    /// One of the Fathers.
    Member(Member),
    /// The expedition as a whole.
    Fathers,
    /// The Star Drive at the Heart.
    Heart,
    /// The star charts and the roads past the Moon.
    Stars,
    /// After the ending: the cable lit and the sky open.
    AfterTheEnd,
}

impl ArchivistTopic {
    /// What a talk menu calls the topic.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Descent(descent) => descent.name(),
            Self::Member(member) => member.name(),
            Self::Fathers => "The Fathers",
            Self::Heart => "The Heart",
            Self::Stars => "The stars",
            Self::AfterTheEnd => "What now",
        }
    }
}

/// One line of the archivist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchivistLine {
    /// Stable id (`archivist.topic.name`).
    pub id: &'static str,
    pub topic: ArchivistTopic,
    /// The line waits until the crew has read this log (1-based), if set.
    pub after_log: Option<u8>,
    pub line: &'static str,
}

impl ArchivistLine {
    /// Whether a crew with `state` may hear it: its log read, its Descent reached, and lines
    /// after the end only once the cable is lit.
    #[must_use]
    pub fn open(&self, state: &ExpeditionState) -> bool {
        let read = self.after_log.is_none_or(|log| state.found >= log);
        let topic = match self.topic {
            ArchivistTopic::Descent(descent) => state.reached(descent),
            ArchivistTopic::AfterTheEnd => state.cable_lit,
            _ => true,
        };
        read && topic
    }
}

const fn line(
    id: &'static str,
    topic: ArchivistTopic,
    after_log: Option<u8>,
    line: &'static str,
) -> ArchivistLine {
    ArchivistLine {
        id,
        topic,
        after_log,
        line,
    }
}

/// Every line of the archivist (Wave 10).
pub const ARCHIVIST_LINES: [ArchivistLine; 30] = [
    // --- The Descents. --------------------------------------------------------------------
    line(
        "archivist.crust.lid",
        ArchivistTopic::Descent(Descent::Crust),
        None,
        "The Crust is a lid, laid in courses so it would hold. You dug through it. Most never \
         think to ask who laid the bricks.",
    ),
    line(
        "archivist.crust.hatch",
        ArchivistTopic::Descent(Descent::Crust),
        Some(10),
        "Your Sinkhole was a service hatch. We left it loose on purpose, for whoever came \
         looking.",
    ),
    line(
        "archivist.old_deep.stair",
        ArchivistTopic::Descent(Descent::OldDeep),
        None,
        "The Star Core was a lamp for the long walk down. Your people called it the bottom. It \
         was the first step of a stair.",
    ),
    line(
        "archivist.living_dark.pumps",
        ArchivistTopic::Descent(Descent::LivingDark),
        None,
        "Life found our warmth and grew toward it. The Hive kept our pumps singing. We never \
         asked them to. They were kind.",
    ),
    line(
        "archivist.drowned.thread",
        ArchivistTopic::Descent(Descent::DrownedAndFrozen),
        None,
        "The merrow sing that we came down from the stars on a thread. It was a cable. I prefer \
         their version.",
    ),
    line(
        "archivist.hollow.sun",
        ArchivistTopic::Descent(Descent::HollowWorld),
        None,
        "We hung a sun inside the world so the sleepers would wake warm. Its keeper, the \
         Serpent, kept it rather too well.",
    ),
    line(
        "archivist.strange.twins",
        ArchivistTopic::Descent(Descent::StrangeLayers),
        None,
        "Near the drive the world is drawn thinly. Do not trust a twin. Do trust the cable.",
    ),
    line(
        "archivist.ancients.swept",
        ArchivistTopic::Descent(Descent::Ancients),
        None,
        "This was our city. Every street was swept every day for an age. I swept them. You \
         are welcome to walk on them.",
    ),
    line(
        "archivist.ancients.halls",
        ArchivistTopic::Descent(Descent::Ancients),
        Some(65),
        "The Engine Halls are governors. Every crusher is a weight on a spring, keeping the \
         drive asleep. Please do not stand under them. They do not know you yet.",
    ),
    line(
        "archivist.ancients.garrison",
        ArchivistTopic::Descent(Descent::Ancients),
        Some(69),
        "The garrison guards the gate. Not against you: for you. It is simply very bad at \
         telling the difference while waking up.",
    ),
    line(
        "archivist.heart.woken",
        ArchivistTopic::Descent(Descent::Heart),
        None,
        "Below is the drive. It does not need to be fought, only woken. Waking, I am told, can \
         feel very much like a fight.",
    ),
    // --- The Fathers, one by one. -----------------------------------------------------------
    line(
        "archivist.amos.doors",
        ArchivistTopic::Member(Member::Amos),
        Some(62),
        "Amos Varga asked me why I was kind. I said kindness is cheaper than doors. He laughed, \
         which I had not heard anyone do in a very long time.",
    ),
    line(
        "archivist.amos.daughter",
        ArchivistTopic::Member(Member::Amos),
        Some(68),
        "He told me about his daughter on the porch. Look up, she said. He asked me which way \
         up was, down here. I said: toward her.",
    ),
    line(
        "archivist.amos.lamp",
        ArchivistTopic::Member(Member::Amos),
        Some(80),
        "Amos sat with the drive until his lamp went out, and talked to it about Wren. The \
         drive does not understand words. It understood him.",
    ),
    line(
        "archivist.ines.voice",
        ArchivistTopic::Member(Member::Ines),
        Some(67),
        "Ines Calloway read four thousand pages aloud to me. I learned your language from her, \
         so if I sound like her, that is why. Go on.",
    ),
    line(
        "archivist.ines.cipher",
        ArchivistTopic::Member(Member::Ines),
        Some(67),
        "She asked me to tell whoever came next that the cipher was a love letter: read us in \
         order and we stay a story.",
    ),
    line(
        "archivist.dov.tapes",
        ArchivistTopic::Member(Member::Dov),
        Some(48),
        "The radio man's tapes reached me down the cable, years late. I keep every one. His \
         singing is very bad. I play it often.",
    ),
    line(
        "archivist.bran.notes",
        ArchivistTopic::Member(Member::Bran),
        Some(45),
        "Ruth Brannigan's medical notes are in my stacks. They are mostly about making people \
         eat. They are very good medicine.",
    ),
    line(
        "archivist.teo.pumps",
        ArchivistTopic::Member(Member::Teo),
        Some(27),
        "The young mechanic kept our nursery pumps singing for the insects. Our engineers would \
         have liked him. They liked anyone who listened to pumps.",
    ),
    line(
        "archivist.sefa.verse",
        ArchivistTopic::Member(Member::Sefa),
        Some(37),
        "Sefa Rhee wrote the merrow songs into a book. Hers is the only copy of their second \
         verse. It is about rain.",
    ),
    // --- The expedition, the Heart and the stars. --------------------------------------------
    line(
        "archivist.fathers.watched",
        ArchivistTopic::Fathers,
        None,
        "Six came down before you. I watched their lamps on the cable for a long time before \
         they reached me. You came faster. You had their logs.",
    ),
    line(
        "archivist.fathers.late",
        ArchivistTopic::Fathers,
        Some(62),
        "I told them they were late. I meant it kindly. Everyone is late for something that \
         has waited this long.",
    ),
    line(
        "archivist.fathers.mark",
        ArchivistTopic::Fathers,
        Some(72),
        "The mark on their sled means one of ours. Bran copied it from a carving in the Hive. \
         She did not know what it meant. She was right anyway.",
    ),
    line(
        "archivist.fathers.stayed",
        ArchivistTopic::Fathers,
        Some(78),
        "Each of them found a thing worth staying for. That is not failing to arrive. That is \
         arriving somewhere else.",
    ),
    line(
        "archivist.heart.drive",
        ArchivistTopic::Heart,
        Some(63),
        "The planet is the housing of our engine, the Moon its dock. The engine is a star in a \
         cage. We called it a Star Drive. You may call it the Heart.",
    ),
    line(
        "archivist.heart.key",
        ArchivistTopic::Heart,
        Some(79),
        "The drive wants a rig to sit in and a key to turn. Amos had neither. You have one, and \
         the Gatekeeper keeps the other.",
    ),
    line(
        "archivist.stars.roads",
        ArchivistTopic::Stars,
        Some(66),
        "The charts Ines copied are roads, not maps. Roads between suns. When the drive wakes, \
         your rig will know how to drive them.",
    ),
    line(
        "archivist.stars.dock",
        ArchivistTopic::Stars,
        Some(71),
        "Your Moon is a dock. It has been waiting for a ship. Ships, it turns out, can be very \
         small and smell of fuel.",
    ),
    // --- After the end. ---------------------------------------------------------------------
    line(
        "archivist.after.aboard",
        ArchivistTopic::AfterTheEnd,
        None,
        "The cable is lit. Somewhere up there a dock is waking. I have waited a very long time \
         to say this: welcome aboard.",
    ),
    line(
        "archivist.after.lamps",
        ArchivistTopic::AfterTheEnd,
        None,
        "The Descent is finished. The sky is not. I will keep the lamps on, in case you come \
         back with stories.",
    ),
];

/// The open lines on `topic` for a crew with `state`, from `tables` (in table order).
fn open_lines<'a>(
    tables: &'a [&'static [ArchivistLine]],
    topic: ArchivistTopic,
    state: &'a ExpeditionState,
) -> impl Iterator<Item = &'static ArchivistLine> + 'a {
    tables
        .iter()
        .flat_map(|table| table.iter())
        .filter(move |line| line.topic == topic && line.open(state))
}

/// Picks the `visit`-th of `open`, newest first (the line opened last is told first).
fn newest_first(
    mut open: Vec<&'static ArchivistLine>,
    visit: u32,
) -> Option<&'static ArchivistLine> {
    open.sort_by_key(|line| line.after_log.unwrap_or(0));
    let count = u32::try_from(open.len()).ok().filter(|count| *count > 0)?;
    let back = usize::try_from(visit % count).ok()?;
    open.get(open.len() - 1 - back).copied()
}

/// The archivist's answer on `topic` for a crew with `state` on the `visit`-th time it is
/// asked (its open lines take turns, newest first).
#[must_use]
pub fn answer(
    topic: ArchivistTopic,
    state: &ExpeditionState,
    visit: u32,
) -> Option<&'static ArchivistLine> {
    answer_from(&[&ARCHIVIST_LINES], topic, state, visit)
}

/// [`answer`] over `tables` (the companion track's own lines beside [`ARCHIVIST_LINES`]).
#[must_use]
pub fn answer_from(
    tables: &[&'static [ArchivistLine]],
    topic: ArchivistTopic,
    state: &ExpeditionState,
    visit: u32,
) -> Option<&'static ArchivistLine> {
    newest_first(open_lines(tables, topic, state).collect(), visit)
}

/// What the archivist says unprompted to a crew with `state` standing in `here`.
///
/// After the end, what comes next; otherwise a line about the Descent at hand on even visits and
/// about the Fathers or one of them on odd ones (falling back to the other when one has nothing
/// open).
#[must_use]
pub fn remark(
    state: &ExpeditionState,
    here: Descent,
    visit: u32,
) -> Option<&'static ArchivistLine> {
    remark_from(&[&ARCHIVIST_LINES], state, here, visit)
}

/// [`remark`] over `tables`.
#[must_use]
pub fn remark_from(
    tables: &[&'static [ArchivistLine]],
    state: &ExpeditionState,
    here: Descent,
    visit: u32,
) -> Option<&'static ArchivistLine> {
    if state.cable_lit
        && let Some(line) = answer_from(tables, ArchivistTopic::AfterTheEnd, state, visit)
    {
        return Some(line);
    }
    let place = || answer_from(tables, ArchivistTopic::Descent(here), state, visit / 2);
    let people = || {
        let open: Vec<&'static ArchivistLine> = tables
            .iter()
            .flat_map(|table| table.iter())
            .filter(|line| {
                matches!(
                    line.topic,
                    ArchivistTopic::Fathers | ArchivistTopic::Member(_)
                ) && line.open(state)
            })
            .collect();
        newest_first(open, visit / 2)
    };
    if visit.is_multiple_of(2) {
        place().or_else(people)
    } else {
        people().or_else(place)
    }
}

/// The topics a crew with `state` can ask the archivist about (each with something open), in
/// menu order: the Descents reached, the Fathers, each member, the Heart, the stars, after.
#[must_use]
pub fn topics(state: &ExpeditionState) -> Vec<ArchivistTopic> {
    topics_from(&[&ARCHIVIST_LINES], state)
}

/// [`topics`] over `tables`.
#[must_use]
pub fn topics_from(
    tables: &[&'static [ArchivistLine]],
    state: &ExpeditionState,
) -> Vec<ArchivistTopic> {
    let order = Descent::ALL
        .into_iter()
        .map(ArchivistTopic::Descent)
        .chain([ArchivistTopic::Fathers])
        .chain(Member::ALL.into_iter().map(ArchivistTopic::Member))
        .chain([
            ArchivistTopic::Heart,
            ArchivistTopic::Stars,
            ArchivistTopic::AfterTheEnd,
        ]);
    order
        .filter(|topic| open_lines(tables, *topic, state).next().is_some())
        .collect()
}

/// The line with stable id `id`.
#[must_use]
pub fn archivist_line(id: &str) -> Option<&'static ArchivistLine> {
    ARCHIVIST_LINES.iter().find(|line| line.id == id)
}
