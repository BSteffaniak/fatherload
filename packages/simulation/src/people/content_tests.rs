//! Content lints for people, dialogue and story arcs: every template parses and only uses the
//! variables it is given, every line set has variants for every tone bucket, nothing is
//! duplicated and characters are varied and distinct. The story arcs' own lints live in
//! `story/arc_tests.rs`.

use std::collections::BTreeSet;

use super::{
    CharacterState, TalkContext, VillageId,
    character::{BEAT_VARS, BEATS, LifeStage, Role},
    dialogue::{
        self, Topic, grammar,
        lines::{COMMON_VARS, LINE_SETS, LineTopic, ToneLines},
    },
    generate_population,
    names::Culture,
    report::{report_villages, sample_facts},
    story::WorldFactKind,
    traits::{Fear, Quirk, Tone, Value, Want},
};
use crate::ids::PlayerId;

/// Minimum variants per tone in every line set.
const MIN_VARIANTS_PER_TONE: usize = 2;

fn ascii(text: &str) -> bool {
    text.is_ascii()
}

fn ends_well(text: &str) -> bool {
    text.ends_with(['.', '!', '?', '*', '"', ')'])
}

#[test]
fn every_topic_has_one_line_set_with_unique_ids() {
    let mut ids = BTreeSet::new();
    for set in LINE_SETS {
        assert!(ids.insert(set.id), "duplicate line set id {}", set.id);
    }
    for topic in LineTopic::ALL {
        let count = LINE_SETS.iter().filter(|set| set.topic == topic).count();
        assert_eq!(count, 1, "{topic:?} has {count} line sets");
    }
}

fn check_tone_lines(
    what: &str,
    lines: &ToneLines,
    allowed: &[&str],
    min: usize,
    seen: &mut BTreeSet<&'static str>,
) {
    for tone in Tone::ALL {
        let variants = lines.get(tone);
        assert!(
            variants.len() >= min,
            "{what}: {tone:?} has {} variants (want {min})",
            variants.len()
        );
        let mut local = BTreeSet::new();
        for line in variants {
            if let Err(error) = grammar::check(line, allowed) {
                panic!("{what} ({tone:?}): {error} in {line:?}");
            }
            // Long phrase variables read badly twice in one line.
            let vars = grammar::variables(line).unwrap_or_default();
            for long in ["want", "fear", "value", "place", "work"] {
                let uses = vars.iter().filter(|name| *name == long).count();
                assert!(
                    uses <= 1,
                    "{what}: {{{long}}} used {uses} times in {line:?}"
                );
            }
            assert!(ascii(line), "{what}: non-ASCII line {line:?}");
            assert!(ends_well(line), "{what}: unpunctuated line {line:?}");
            assert!(local.insert(*line), "{what}: duplicate line {line:?}");
            assert!(seen.insert(line), "{what}: line reused elsewhere: {line:?}");
        }
    }
    // Tones must actually sound different: no line set where two tones share a line.
    let all: Vec<&str> = Tone::ALL
        .iter()
        .flat_map(|tone| lines.get(*tone))
        .copied()
        .collect();
    let distinct: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(all.len(), distinct.len(), "{what}: tones share lines");
}

#[test]
fn every_line_set_has_variants_for_every_tone_and_valid_templates() {
    let mut seen = BTreeSet::new();
    for set in LINE_SETS {
        let mut allowed: Vec<&str> = COMMON_VARS.to_vec();
        allowed.extend(set.topic.vars());
        check_tone_lines(
            set.id,
            &set.lines,
            &allowed,
            MIN_VARIANTS_PER_TONE,
            &mut seen,
        );
    }
    let total: usize = LINE_SETS
        .iter()
        .map(|set| {
            Tone::ALL
                .iter()
                .map(|tone| set.lines.get(*tone).len())
                .sum::<usize>()
        })
        .sum();
    assert!(total >= 400, "only {total} authored lines");
}

#[test]
fn topic_specific_variables_are_used() {
    // A topic that provides a variable should mostly use it (or it's probably the wrong topic).
    for set in LINE_SETS {
        for var in set.topic.vars() {
            if matches!(
                *var,
                "days" | "rel_word" | "band" | "depth" | "fact_relation"
            ) {
                continue;
            }
            let lines: Vec<&str> = Tone::ALL
                .iter()
                .flat_map(|tone| set.lines.get(*tone))
                .copied()
                .collect();
            let using = lines.iter().filter(|line| {
                grammar::variables(line).is_ok_and(|vars| vars.iter().any(|name| name == var))
            });
            assert!(
                using.count() * 2 >= lines.len(),
                "{}: fewer than half its lines use {{{var}}}",
                set.id
            );
        }
    }
}

#[test]
fn trait_phrase_tables_are_full_and_valid() {
    let mut seen = BTreeSet::new();
    for quirk in Quirk::ALL {
        assert!(quirk.asides().len() >= 3, "{quirk:?} needs 3 asides");
        for aside in quirk.asides() {
            grammar::check(aside, COMMON_VARS).unwrap();
            assert!(ascii(aside) && ends_well(aside), "{aside:?}");
            assert!(seen.insert(*aside), "duplicate aside {aside:?}");
        }
    }
    for want in Want::ALL {
        assert!(want.phrases().len() >= 2);
        let needs_focus = matches!(
            want,
            Want::FindRelative | Want::BeatRival | Want::PayOffDebt | Want::MakeAmends
        );
        for phrase in want.phrases() {
            grammar::check(phrase, &["focus", "focus_relation"]).unwrap();
            assert_eq!(phrase.contains("{focus"), needs_focus, "{want:?}: {phrase}");
            assert!(seen.insert(*phrase));
        }
    }
    for fear in Fear::ALL {
        assert!(fear.phrases().len() >= 2);
        seen.extend(fear.phrases());
    }
    for value in Value::ALL {
        assert!(value.phrases().len() >= 2);
        seen.extend(value.phrases());
    }
    for role in Role::ALL {
        assert!(role.work().len() >= 3, "{role:?} needs 3 work phrases");
        for work in role.work() {
            assert!(seen.insert(*work), "duplicate work phrase {work:?}");
            assert!(ascii(work));
        }
    }
    for kind in WorldFactKind::ALL {
        assert!(kind.nouns().len() >= 2, "{kind:?}");
    }
}

#[test]
fn history_beats_are_valid_and_plentiful() {
    let mut seen = BTreeSet::new();
    for beat in BEATS {
        grammar::check(beat.text, BEAT_VARS).unwrap();
        assert!(ascii(beat.text) && ends_well(beat.text), "{}", beat.text);
        assert!(seen.insert(beat.text));
    }
    for stage in [
        LifeStage::Origin,
        LifeStage::TurningPoint,
        LifeStage::Recently,
    ] {
        let generic = BEATS
            .iter()
            .filter(|beat| beat.stage == stage && beat.cultures.is_empty() && beat.roles.is_empty())
            .filter(|beat| !beat.text.contains("{absent"))
            .count();
        assert!(generic >= 6, "{stage:?} has only {generic} generic beats");
    }
}

#[test]
fn names_and_village_names_are_ascii() {
    for culture in Culture::ALL {
        let table = culture.table();
        for name in table
            .given
            .iter()
            .chain(table.surnames)
            .chain(table.nicknames)
            .chain(table.place_heads)
            .chain(table.place_tails)
        {
            assert!(ascii(name), "{culture:?}: {name}");
        }
    }
}

/// Fifty sampled characters' signature lines (first greeting, work, want, fear, value).
fn signature(character: &super::Character, population: &super::Population) -> Vec<String> {
    let mut state = CharacterState::default();
    let ctx = TalkContext {
        seed: 1,
        day: 1,
        player: PlayerId::new(1),
        player_name: "Rook",
        population: Some(population),
        facts: &[],
    };
    [
        Topic::Greeting,
        Topic::Work,
        Topic::Want,
        Topic::Fear,
        Topic::Value,
    ]
    .into_iter()
    .filter_map(|topic| dialogue::say(character, &mut state, &ctx, topic))
    .map(|line| {
        assert!(
            !line.text.contains(['{', '}', '[', ']']) && !line.text.contains("???"),
            "{}",
            line.text
        );
        line.text
    })
    .collect()
}

#[test]
fn fifty_sampled_characters_are_distinct_and_varied() {
    for seed in [1, 2, 77] {
        let villages = report_villages(seed, 50);
        let characters: Vec<(&super::Character, &super::Population)> = villages
            .iter()
            .flat_map(|village| {
                village
                    .characters
                    .iter()
                    .map(move |character| (character, village))
            })
            .collect();
        assert_eq!(characters.len(), 50);
        let names: BTreeSet<String> = characters.iter().map(|(c, _)| c.name.full()).collect();
        assert!(names.len() >= 49, "seed {seed}: duplicate names");
        let signatures: BTreeSet<Vec<String>> =
            characters.iter().map(|(c, p)| signature(c, p)).collect();
        assert_eq!(signatures.len(), 50, "seed {seed}: duplicated line sets");
        let tones: BTreeSet<Tone> = characters.iter().map(|(c, _)| c.tone).collect();
        let roles: BTreeSet<Role> = characters.iter().map(|(c, _)| c.role).collect();
        let wants: BTreeSet<Want> = characters.iter().map(|(c, _)| c.want).collect();
        let quirks: BTreeSet<Quirk> = characters
            .iter()
            .flat_map(|(c, _)| c.quirks.clone())
            .collect();
        assert_eq!(tones.len(), Tone::ALL.len(), "seed {seed}: tones {tones:?}");
        assert!(roles.len() >= 14, "seed {seed}: roles {roles:?}");
        assert!(wants.len() >= 9, "seed {seed}: wants {wants:?}");
        assert!(quirks.len() >= 15, "seed {seed}: quirks {quirks:?}");
        let personalities: BTreeSet<_> = characters.iter().map(|(c, _)| c.personality).collect();
        assert_eq!(personalities.len(), 50);
    }
}

#[test]
fn talking_repeatedly_does_not_repeat_within_the_window() {
    let population = generate_population(21, VillageId(3), Culture::Scrapper, 9);
    let facts = sample_facts();
    for character in &population.characters {
        let mut state = CharacterState::default();
        let mut topic_lines: Vec<String> = Vec::new();
        for day in 0..10 {
            let ctx = TalkContext {
                seed: 21,
                day,
                player: PlayerId::new(2),
                player_name: "Juno",
                population: Some(&population),
                facts: &facts,
            };
            let lines = dialogue::talk(character, &mut state, &ctx);
            // Skip the greeting; the rest should not repeat within six visits.
            topic_lines.extend(lines.into_iter().skip(1).map(|line| line.text));
        }
        for window in topic_lines.windows(6) {
            let distinct: BTreeSet<&String> = window.iter().collect();
            assert_eq!(
                distinct.len(),
                window.len(),
                "{}: {window:#?}",
                character.name.full()
            );
        }
    }
}
