//! The dialogue system: picks and fills authored lines for a character.
//!
//! [`say`] answers one [`Topic`]; [`talk`] runs a short exchange (greeting, one topic, maybe a
//! quirk aside) the way a talk button would, rotating topics across visits.
//!
//! Selection, in order:
//! 1. The topic resolves to a line set using state: greetings by [`Standing`], relationship
//!    talk by a relationship the character has, memories by the most recent unmentioned
//!    [`MemoryFact`], mood talk only when the mood is clearly up or down.
//! 2. The voice is the character's primary [`Tone`] most of the time and their secondary tone
//!    otherwise, so two gruff people still sound different.
//! 3. Variants spoken to this player within the no-repeat window are skipped while
//!    alternatives remain.
//! 4. The template is filled (grammar in [`grammar`]) and a per-trait tone filter adds a
//!    stammer, a grunt, an exclamation or a wry tag, scaled by how strong the trait is.
//!
//! All rolls are seeded by the character's voice seed, the conversation count and the topic,
//! so a conversation is reproducible (and replays identically on every peer).

use super::{
    PeopleRng,
    character::{AbsentFate, Character},
    memory::{CharacterState, MemoryFact, MoodBand, Standing},
    population::{Population, RelationshipKind},
    salt,
    story::{ArcId, WorldFact},
    traits::Tone,
};
use crate::ids::PlayerId;

pub mod grammar;
pub mod lines;

use grammar::Vars;
use lines::{LineTopic, line_set};

/// Percent of lines spoken in the secondary tone.
const SECONDARY_TONE_PERCENT: u32 = 25;
/// Deepest band (by index) most rumours point into.
const RUMOUR_NEAR_BAND: u8 = 5;
/// Percent of exchanges that end with a quirk aside.
const ASIDE_PERCENT: u32 = 30;

/// What a player can ask about (or what the game prompts a line for).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Topic {
    Greeting,
    Farewell,
    Work,
    Want,
    Fear,
    Value,
    /// Someone they know (a random relationship, or an absent person).
    People,
    Rumour,
    Mood,
    /// The most recent thing they remember about the player and have not brought up.
    Memory,
    Gift {
        liked: bool,
    },
    Thanks,
    Quirk,
}

/// What a conversation needs from the world.
#[derive(Clone, Copy, Debug)]
pub struct TalkContext<'a> {
    /// The world seed (names the home village of a character without a population).
    pub seed: u64,
    /// The world day (mood and memory timing).
    pub day: u32,
    pub player: PlayerId,
    pub player_name: &'a str,
    /// The character's village, for neighbours' names.
    pub population: Option<&'a Population>,
    /// Places rumours can point at.
    pub facts: &'a [WorldFact],
}

/// One spoken line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Line {
    pub text: String,
    /// The line set it came from (`None` for quirk asides).
    pub topic: Option<LineTopic>,
    pub tone: Tone,
}

fn common_vars(character: &Character, ctx: &TalkContext<'_>, rng: &mut PeopleRng) -> Vars {
    let mut vars = Vars::new();
    vars.set("name", character.name.short());
    vars.set("role", character.role.name());
    vars.set("work", *rng.pick(character.role.work()));
    vars.set(
        "village",
        ctx.population.map_or_else(
            || super::names::village_name(ctx.seed, character.home.0, character.culture),
            |population| population.village_name.clone(),
        ),
    );
    vars.set("player", ctx.player_name);
    vars
}

/// "yesterday", "a few days back", ...
fn days_ago(today: u32, then: u32) -> &'static str {
    match today.saturating_sub(then) {
        0 => "earlier today",
        1 => "yesterday",
        2..=6 => "a few days back",
        7..=20 => "a while back",
        _ => "ages ago",
    }
}

/// Resolves a topic into a line set and its variables, or `None` when the character has
/// nothing to say about it.
fn resolve(
    character: &Character,
    state: &CharacterState,
    ctx: &TalkContext<'_>,
    topic: Topic,
    rng: &mut PeopleRng,
) -> Option<(LineTopic, Vars, Option<usize>)> {
    let mut vars = common_vars(character, ctx, rng);
    let memory = state.memory(ctx.player);
    let mut mentioned = None;
    let line_topic = match topic {
        Topic::Greeting => match memory.standing() {
            Standing::Stranger => LineTopic::GreetFirst,
            Standing::Cold => LineTopic::GreetCold,
            Standing::Acquainted => LineTopic::GreetAgain,
            Standing::Friendly | Standing::Close => LineTopic::GreetFriend,
        },
        Topic::Farewell => LineTopic::Farewell,
        Topic::Work => LineTopic::Work,
        Topic::Want => {
            let mut want_vars = vars.clone();
            if let Some(focus) = character.focus_name(ctx.population) {
                want_vars.set("focus", focus);
            }
            if let Some(relation) = character.focus_relation(ctx.population) {
                want_vars.set("focus_relation", relation);
            }
            let phrase = grammar::expand(rng.pick(character.want.phrases()), &want_vars, rng);
            vars.set("want", lower_first(&phrase));
            LineTopic::Want
        }
        Topic::Fear => {
            vars.set("fear", *rng.pick(character.fear.phrases()));
            LineTopic::Fear
        }
        Topic::Value => {
            let value = rng.pick(&character.values);
            vars.set("value", *rng.pick(value.phrases()));
            LineTopic::Value
        }
        Topic::People => people_topic(character, ctx, &mut vars, rng)?,
        Topic::Rumour => {
            if ctx.facts.is_empty() {
                LineTopic::NoRumour
            } else {
                // Village gossip mostly reaches as far as the first deep stratum; the
                // really deep places are rarer, stranger rumours.
                let near: Vec<&WorldFact> = ctx
                    .facts
                    .iter()
                    .filter(|fact| fact.band <= RUMOUR_NEAR_BAND)
                    .collect();
                let fact = if !near.is_empty() && rng.chance(80) {
                    *rng.pick(&near)
                } else {
                    rng.pick(ctx.facts)
                };
                vars.set("place", *rng.pick(fact.kind.nouns()));
                vars.set("band", fact.band_name());
                vars.set("depth", fact.depth().to_string());
                LineTopic::Rumour
            }
        }
        Topic::Mood => match MoodBand::of(state.mood(character, ctx.day, Some(ctx.player))) {
            MoodBand::Glum | MoodBand::Low => LineTopic::MoodLow,
            MoodBand::Cheery | MoodBand::Elated => LineTopic::MoodHigh,
            MoodBand::Content => return None,
        },
        Topic::Memory => {
            let index = memory.unmentioned()?;
            let event = &memory.events[index];
            vars.set("days", days_ago(ctx.day, event.day));
            mentioned = Some(index);
            match &event.fact {
                MemoryFact::FoundPerson { name, relation } => {
                    vars.set("fact_name", name.clone());
                    vars.set("fact_relation", relation.clone());
                    LineTopic::RememberFoundPerson
                }
                MemoryFact::ArcCompleted { arc, .. } => {
                    vars.set("arc", arc_title(*arc));
                    LineTopic::RememberArc
                }
                MemoryFact::ArcAbandoned { arc } => {
                    vars.set("arc", arc_title(*arc));
                    LineTopic::RememberAbandoned
                }
                MemoryFact::Gift { .. } => LineTopic::RememberGift,
                MemoryFact::Slighted => LineTopic::RememberSlight,
                MemoryFact::ReachedDepth { metres } => {
                    vars.set("depth", metres.to_string());
                    LineTopic::RememberDepth
                }
                MemoryFact::FoundArtifact { name } => {
                    vars.set("fact_name", name.clone());
                    LineTopic::RememberArtifact
                }
                MemoryFact::RescuedNeighbour { name } => {
                    vars.set("fact_name", name.clone());
                    LineTopic::RememberRescue
                }
                MemoryFact::HelpedVillage => LineTopic::RememberVillage,
                MemoryFact::Met | MemoryFact::Traded => return None,
            }
        }
        Topic::Gift { liked } => {
            if liked {
                LineTopic::GiftLiked
            } else {
                LineTopic::GiftDisliked
            }
        }
        Topic::Thanks => LineTopic::Thanks,
        Topic::Quirk => return None,
    };
    Some((line_topic, vars, mentioned))
}

fn arc_title(arc: ArcId) -> String {
    arc.template().map_or_else(
        || "that job".to_owned(),
        |template| template.title.to_owned(),
    )
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

/// Picks someone to talk about: a neighbour they have a relationship with, or an absent
/// person.
fn people_topic(
    character: &Character,
    ctx: &TalkContext<'_>,
    vars: &mut Vars,
    rng: &mut PeopleRng,
) -> Option<LineTopic> {
    let neighbours: Vec<(String, &'static str, RelationshipKind)> = ctx
        .population
        .map(|population| {
            population
                .relationships_of(character.id)
                .filter_map(|edge| {
                    let other = population.get(edge.to)?;
                    Some((
                        other.name.short().to_owned(),
                        edge.kind.word(other.pronouns),
                        edge.kind,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let absent = &character.absent;
    let total = neighbours.len() + absent.len();
    if total == 0 {
        return None;
    }
    let pick = rng.below(total);
    let (name, word, topic) = if let Some((name, word, kind)) = neighbours.get(pick) {
        let topic = match kind {
            RelationshipKind::Parent
            | RelationshipKind::Child
            | RelationshipKind::Sibling
            | RelationshipKind::Spouse => LineTopic::AboutFamily,
            RelationshipKind::Friend => LineTopic::AboutFriend,
            RelationshipKind::Rival => LineTopic::AboutRival,
            RelationshipKind::Owes => LineTopic::AboutOwes,
            RelationshipKind::Lender => LineTopic::AboutLender,
            RelationshipKind::Sweetheart => LineTopic::AboutSweetheart,
            RelationshipKind::Estranged => LineTopic::AboutEstranged,
        };
        (name.clone(), (*word).to_owned(), topic)
    } else {
        let person = &absent[pick - neighbours.len()];
        let topic = match person.fate {
            AbsentFate::WentDeep => LineTopic::AboutWentDeep,
            AbsentFate::MovedAway => LineTopic::AboutMovedAway,
            AbsentFate::OldRival => LineTopic::AboutRival,
            AbsentFate::Creditor => LineTopic::AboutOwes,
            AbsentFate::Estranged => LineTopic::AboutEstranged,
        };
        (
            person.name.short().to_owned(),
            person.relation.clone(),
            topic,
        )
    };
    vars.set("rel", name);
    vars.set("rel_word", word);
    Some(topic)
}

/// A stable key for a line variant (for the no-repeat window).
fn line_key(id: &str, tone: Tone, index: usize) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in id.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3);
    }
    hash ^ ((tone as u64) << 56) ^ (index as u64).wrapping_mul(0x9E37_79B9)
}

/// Picks the voice for one line.
fn voice(character: &Character, rng: &mut PeopleRng) -> Tone {
    if rng.chance(SECONDARY_TONE_PERCENT) {
        character.secondary_tone
    } else {
        character.tone
    }
}

fn topic_salt(topic: Topic) -> u64 {
    match topic {
        Topic::Greeting => 1,
        Topic::Farewell => 2,
        Topic::Work => 3,
        Topic::Want => 4,
        Topic::Fear => 5,
        Topic::Value => 6,
        Topic::People => 7,
        Topic::Rumour => 8,
        Topic::Mood => 9,
        Topic::Memory => 10,
        Topic::Gift { liked } => 11 + u64::from(liked),
        Topic::Thanks => 13,
        Topic::Quirk => 14,
    }
}

/// One line about `topic`, recorded in the no-repeat window (and memories it brings up are
/// marked mentioned). `None` when the character has nothing to say about it.
pub fn say(
    character: &Character,
    state: &mut CharacterState,
    ctx: &TalkContext<'_>,
    topic: Topic,
) -> Option<Line> {
    let memory = state.memory(ctx.player);
    let mut rng = PeopleRng::new(
        character.voice_seed,
        salt::DIALOGUE ^ topic_salt(topic),
        u64::from(memory.talks),
        u64::from(ctx.day),
    );
    if topic == Topic::Quirk {
        let quirk = rng.pick(&character.quirks);
        let vars = common_vars(character, ctx, &mut rng);
        let id = quirk.describe();
        let index = pick_fresh(
            &memory.recent_lines,
            quirk.asides().len(),
            &mut rng,
            |index| line_key(id, character.tone, index),
        );
        state
            .players
            .entry(ctx.player)
            .or_default()
            .note_line(line_key(id, character.tone, index));
        return Some(Line {
            text: grammar::expand(quirk.asides()[index], &vars, &mut rng),
            topic: None,
            tone: character.tone,
        });
    }
    let (line_topic, vars, mentioned) = resolve(character, state, ctx, topic, &mut rng)?;
    let set = line_set(line_topic);
    let tone = voice(character, &mut rng);
    let variants = set.lines.get(tone);
    let index = pick_fresh(&memory.recent_lines, variants.len(), &mut rng, |index| {
        line_key(set.id, tone, index)
    });
    let text = grammar::expand(variants[index], &vars, &mut rng);
    let text = tone_filter(character, tone, &text, &mut rng);
    let entry = state.players.entry(ctx.player).or_default();
    entry.note_line(line_key(set.id, tone, index));
    if let Some(mentioned) = mentioned
        && let Some(event) = entry.events.get_mut(mentioned)
    {
        event.mentioned = true;
    }
    Some(Line {
        text,
        topic: Some(line_topic),
        tone,
    })
}

/// A variant not in the no-repeat window, or (when all were said recently) the one said
/// longest ago.
fn pick_fresh(
    recent: &[u64],
    count: usize,
    rng: &mut PeopleRng,
    key: impl Fn(usize) -> u64,
) -> usize {
    let fresh: Vec<usize> = (0..count)
        .filter(|index| !recent.contains(&key(*index)))
        .collect();
    if fresh.is_empty() {
        (0..count)
            .min_by_key(|index| recent.iter().position(|recent| *recent == key(*index)))
            .unwrap_or(0)
    } else {
        *rng.pick(&fresh)
    }
}

/// Topics a visit rotates through after the greeting.
const ROTATION: [Topic; 7] = [
    Topic::Work,
    Topic::People,
    Topic::Want,
    Topic::Rumour,
    Topic::Mood,
    Topic::Fear,
    Topic::Value,
];

/// A short exchange: a greeting, one topic (an unmentioned memory first, otherwise the next
/// topic in a per-character rotation) and sometimes a quirk aside. Counts the conversation and
/// records the first meeting.
pub fn talk(character: &Character, state: &mut CharacterState, ctx: &TalkContext<'_>) -> Vec<Line> {
    let mut lines = Vec::new();
    lines.extend(say(character, state, ctx, Topic::Greeting));
    let memory = state.memory(ctx.player);
    let talks = memory.talks;
    if memory.first_met_day.is_none() {
        state.remember(ctx.player, ctx.day, MemoryFact::Met);
    }
    let mut rng = PeopleRng::new(
        character.voice_seed,
        salt::DIALOGUE,
        u64::from(talks),
        0x7A1C,
    );
    let mut topic_line = None;
    if talks > 0 && memory.unmentioned().is_some() {
        topic_line = say(character, state, ctx, Topic::Memory);
    }
    if topic_line.is_none() {
        // The first visit is introductions: what they do.
        let offset = usize::try_from(character.voice_seed % 7).unwrap_or(0);
        let start = if talks == 0 {
            0
        } else {
            usize::try_from(talks).unwrap_or(0) + offset
        };
        for step in 0..ROTATION.len() {
            let topic = ROTATION[(start + step) % ROTATION.len()];
            if let Some(line) = say(character, state, ctx, topic) {
                topic_line = Some(line);
                break;
            }
        }
    }
    lines.extend(topic_line);
    if rng.chance(ASIDE_PERCENT) {
        lines.extend(say(character, state, ctx, Topic::Quirk));
    }
    let entry = state.players.entry(ctx.player).or_default();
    entry.talks += 1;
    entry.last_talk_day = ctx.day;
    lines
}

const POETIC_TAGS: &[&str] = &[
    " The stone keeps its own time.",
    " Like water finding its way down.",
    " The lamps know.",
    " Such is the seam.",
];
const DRY_TAGS: &[&str] = &[" Probably.", " Allegedly.", " Or so I'm told.", " Anyway."];
const EARNEST_TAGS: &[&str] = &[" I mean that.", " Honestly.", " Truly."];
const GRUFF_LEADS: &[&str] = &["Hmph. ", "Mm. ", "Tch. "];

/// Per-trait colouring on top of the authored line: the stronger the trait behind the tone,
/// the likelier the filter.
fn tone_filter(character: &Character, tone: Tone, text: &str, rng: &mut PeopleRng) -> String {
    let p = character.personality;
    let strength = |value: u8| u32::from(value.saturating_sub(55)) / 2;
    let ends_plain = text.ends_with('.') && !text.ends_with("..");
    match tone {
        Tone::Nervous if rng.chance(strength(p.neuroticism) + 5) => stammer(text),
        Tone::Gruff if rng.chance(strength(100 - p.agreeableness)) => {
            format!("{}{text}", rng.pick(GRUFF_LEADS))
        }
        Tone::Cheerful if ends_plain && rng.chance(strength(p.extraversion) + 10) => {
            format!("{}!", &text[..text.len() - 1])
        }
        Tone::Poetic if ends_plain && rng.chance(strength(p.openness)) => {
            format!("{text}{}", rng.pick(POETIC_TAGS))
        }
        Tone::Dry if ends_plain && rng.chance(strength(p.conscientiousness) / 2 + 4) => {
            format!("{text}{}", rng.pick(DRY_TAGS))
        }
        Tone::Earnest if ends_plain && rng.chance(strength(p.agreeableness) / 2 + 4) => {
            format!("{text}{}", rng.pick(EARNEST_TAGS))
        }
        _ => text.to_owned(),
    }
}

/// "Well..." becomes "W-well...".
fn stammer(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() && !text.starts_with("H-") => {
            format!("{first}-{}{}", first.to_ascii_lowercase(), chars.as_str())
        }
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{TalkContext, Topic, say, stammer, talk};
    use crate::{
        ids::PlayerId,
        people::{
            CharacterState, MemoryFact, VillageId, generate_population,
            names::Culture,
            story::{WorldFact, WorldFactKind},
        },
    };

    fn facts() -> Vec<WorldFact> {
        vec![WorldFact {
            kind: WorldFactKind::Derelict,
            band: 3,
            row: 140,
            feature: None,
        }]
    }

    #[test]
    fn repeated_talks_vary_and_never_leave_placeholders() {
        let population = generate_population(5, VillageId(1), Culture::Highland, 8);
        let facts = facts();
        for character in &population.characters {
            let mut state = CharacterState::default();
            let mut seen = Vec::new();
            for day in 0..8 {
                let ctx = TalkContext {
                    seed: 5,
                    day,
                    player: PlayerId::new(1),
                    player_name: "Rook",
                    population: Some(&population),
                    facts: &facts,
                };
                for line in talk(character, &mut state, &ctx).into_iter().skip(1) {
                    assert!(
                        !line.text.contains(['{', '}', '[', ']']) && !line.text.contains("???"),
                        "{}",
                        line.text
                    );
                    seen.push(line.text);
                }
            }
            let distinct: std::collections::BTreeSet<&String> = seen.iter().collect();
            assert!(
                distinct.len() * 10 >= seen.len() * 8,
                "{} repeats itself: {seen:#?}",
                character.name.full()
            );
        }
    }

    #[test]
    fn memories_are_brought_up_once() {
        let population = generate_population(5, VillageId(2), Culture::Frontier, 6);
        let character = &population.characters[0];
        let player = PlayerId::new(3);
        let mut state = CharacterState::default();
        let ctx = |day| TalkContext {
            seed: 5,
            day,
            player,
            player_name: "Wren",
            population: Some(&population),
            facts: &[],
        };
        let _ = talk(character, &mut state, &ctx(1));
        state.remember(
            player,
            2,
            MemoryFact::FoundPerson {
                name: "Tobin".into(),
                relation: "brother".into(),
            },
        );
        let lines = talk(character, &mut state, &ctx(3));
        assert!(
            lines
                .iter()
                .any(|line| line.text.contains("Tobin") || line.text.contains("brother")),
            "{lines:#?}"
        );
        assert!(say(character, &mut state, &ctx(3), Topic::Memory).is_none());
    }

    #[test]
    fn talks_are_deterministic() {
        let population = generate_population(8, VillageId(4), Culture::Mesa, 5);
        let run = || {
            let mut state = CharacterState::default();
            let mut out = Vec::new();
            for day in 0..4 {
                let ctx = TalkContext {
                    seed: 5,
                    day,
                    player: PlayerId::new(1),
                    player_name: "Kit",
                    population: Some(&population),
                    facts: &[],
                };
                out.extend(talk(&population.characters[2], &mut state, &ctx));
            }
            (out, state)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn stammer_doubles_the_first_letter() {
        assert_eq!(stammer("Well, hello."), "W-well, hello.");
        assert_eq!(stammer("*hums*"), "*hums*");
    }
}
