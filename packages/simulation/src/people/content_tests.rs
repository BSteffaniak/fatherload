//! Content lints for people, dialogue and story arcs: every template parses and only uses the
//! variables it is given, every line set has variants for every tone bucket, nothing is
//! duplicated, characters are varied and distinct, arcs have a hook, a twist and a consequence,
//! and every arc runs on the quest engine (rewards in bounds, solo paths, binding on many
//! seeds).

use std::collections::{BTreeMap, BTreeSet};

use super::{
    CharacterState, MemoryFact, TalkContext, VillageId, VillageProblem, WorldFact,
    character::{BEAT_VARS, BEATS, LifeStage, Role},
    dialogue::{
        self, Topic,
        grammar::{self, Vars},
        lines::{COMMON_VARS, LINE_SETS, LineTopic, ToneLines},
    },
    generate_population,
    names::Culture,
    report::{report_villages, sample_facts},
    story::{ARC_TEMPLATES, ARC_VARS, ArcTemplate, OutcomeWhen, Slot, WorldFactKind, story_seeds},
    traits::{Fear, Quirk, Tone, Value, Want},
};
use crate::{
    ids::PlayerId,
    poi::PoiJournal,
    quest::{Objective, QuestGiver, QuestId, binding::bind_quest},
    terrain::{Terrain, TilePosition},
    worldgen::{SPAWN_TILE_X, WorldSettings},
};

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

fn arc_world_settings() -> Vec<WorldSettings> {
    use crate::worldgen::{HazardIntensity, ResourceRichness, WorldSize};
    let count = std::env::var("FATHERLOAD_QUEST_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_u64);
    let mut worlds: Vec<WorldSettings> = (0..count).map(WorldSettings::new).collect();
    for size in WorldSize::ALL {
        for richness in ResourceRichness::ALL {
            let mut settings = WorldSettings::new(162);
            settings.size = size;
            settings.richness = richness;
            settings.hazards = HazardIntensity::Brutal;
            worlds.push(settings);
        }
    }
    worlds
}

fn check_arc_shape(arc: &'static ArcTemplate) {
    let name = arc.title;
    assert!(
        !arc.hook.is_empty() && !arc.twist.is_empty() && !arc.consequence.is_empty(),
        "{name}"
    );
    assert_eq!(
        arc.beats.len(),
        arc.stages.len(),
        "{name}: one beat title per stage"
    );
    assert!(!arc.triggers.is_empty(), "{name}: no triggers");
    assert!(arc.outcomes.len() >= 2, "{name}: arcs branch on outcome");
    assert!(
        arc.outcomes
            .iter()
            .any(|outcome| outcome.when == OutcomeWhen::Completed),
        "{name}: no default ending"
    );
    for slot in arc.slots {
        if let Slot::Place { target, kind } = slot {
            let spec = arc
                .targets
                .get(usize::from(*target))
                .unwrap_or_else(|| panic!("{name}: place slot for missing target {target}"));
            let bound = WorldFactKind::of_target(*spec);
            let compatible = bound == Some(*kind)
                || (*kind == WorldFactKind::Cavern && bound == Some(WorldFactKind::DeepChamber));
            assert!(compatible, "{name}: slot {kind:?} on target {spec:?}");
        }
    }
    let provided = arc.provided_vars();
    let mut seen = BTreeSet::new();
    for lines in [&arc.pitch, &arc.thanks] {
        check_tone_lines(name, lines, ARC_VARS, 1, &mut seen);
        for tone in Tone::ALL {
            for line in lines.get(tone) {
                for var in grammar::variables(line).unwrap() {
                    assert!(
                        provided.contains(&var.as_str()),
                        "{name}: {{{var}}} has no slot in {line:?}"
                    );
                }
            }
        }
    }
    for text in [arc.log.offer, arc.log.progress, arc.log.complete] {
        assert!(
            !text.contains(['{', '[']),
            "{name}: log lines take no variables"
        );
    }
    let def = arc.quest_def(QuestId(9_000 + arc.id.0), QuestGiver::Vale);
    let bounds = arc.tier.reward_bounds();
    assert!(def.reward.credits <= bounds.credits, "{name}: credits");
    assert!(
        def.reward.reputation <= bounds.reputation,
        "{name}: reputation"
    );
    assert!(def.reward.bombs <= bounds.bombs, "{name}: bombs");
    assert!(
        def.reward.material_count() <= bounds.materials,
        "{name}: materials"
    );
    assert!(
        def.objectives().all(Objective::has_solo_path),
        "{name}: solo path"
    );
    assert!(
        def.objectives().any(|objective| !objective.is_trivial()) || !def.twists.is_empty(),
        "{name}: filler"
    );
    for (stage_index, stage) in def.stages.iter().enumerate() {
        for objective in *stage {
            if matches!(objective, Objective::Deliver { .. }) {
                assert_eq!(
                    stage_index + 1,
                    def.stages.len(),
                    "{name}: deliver only last"
                );
            }
            for target in objective.targets() {
                assert!(
                    usize::from(target) < def.targets.len(),
                    "{name}: bad target"
                );
            }
        }
    }
}

#[test]
fn arcs_are_well_formed() {
    let mut ids = BTreeSet::new();
    let mut titles = BTreeSet::new();
    for arc in ARC_TEMPLATES {
        assert!(ids.insert(arc.id), "duplicate arc id {:?}", arc.id);
        assert!(
            titles.insert(arc.title),
            "duplicate arc title {}",
            arc.title
        );
        check_arc_shape(arc);
        for outcome in arc.outcomes {
            for effect in outcome.effects {
                if let super::story::ArcEffect::FollowUp(next) = effect {
                    assert!(
                        next.template().is_some(),
                        "{}: follow-up {next:?}",
                        arc.title
                    );
                }
            }
        }
    }
    assert!(ARC_TEMPLATES.len() >= 8);
}

#[test]
fn every_arc_binds_on_many_seeds() {
    let origin = TilePosition {
        x: SPAWN_TILE_X,
        y: 4,
    };
    let journal = PoiJournal::default();
    for settings in arc_world_settings() {
        let terrain = Terrain::generate(&settings);
        for arc in ARC_TEMPLATES {
            let def = arc.quest_def(QuestId(9_000 + arc.id.0), QuestGiver::Vale);
            let bound = bind_quest(&def, &terrain, origin, &journal, 0, &BTreeSet::new());
            assert!(
                bound.is_some(),
                "{} does not bind on seed {} {:?}/{:?}",
                arc.title,
                settings.seed,
                settings.size,
                settings.richness
            );
        }
    }
}

#[test]
fn every_arc_gets_seeded_and_seeds_render() {
    let facts = sample_facts();
    let problems = VillageProblem::ALL;
    let history = [MemoryFact::FoundArtifact {
        name: "Resonant Idol".into(),
    }];
    let mut seeded: BTreeMap<u16, u32> = BTreeMap::new();
    for village in 0..60 {
        let culture = Culture::ALL[village as usize % Culture::ALL.len()];
        let population = generate_population(4, VillageId(village), culture, 8);
        let problems = &problems[village as usize % problems.len()..][..1];
        let seeds = story_seeds(&population, &facts, problems, &history, 0);
        assert_eq!(
            seeds,
            story_seeds(&population, &facts, problems, &history, 0)
        );
        let givers: BTreeSet<_> = seeds.iter().map(|seed| seed.giver).collect();
        assert_eq!(givers.len(), seeds.len(), "one seed per resident");
        assert!(
            seeds.len() >= 3,
            "village {village} only seeds {}",
            seeds.len()
        );
        for seed in &seeds {
            *seeded.entry(seed.arc.0).or_default() += 1;
            assert_eq!(seed.bindings.len(), seed.template().slots.len());
            let giver = population.get(seed.giver).unwrap();
            // A want about a person seeds an arc about that person.
            if let (super::SeedSource::Want(_), Some(focus)) =
                (&seed.source, giver.focus_name(Some(&population)))
            {
                let people: Vec<&String> = seed
                    .bindings
                    .iter()
                    .filter_map(|binding| match binding {
                        super::SlotBinding::Absent { name, .. }
                        | super::SlotBinding::Neighbour { name, .. } => Some(name),
                        super::SlotBinding::Place(_) => None,
                    })
                    .collect();
                assert!(
                    people.is_empty() || people.contains(&&focus),
                    "{} wants something about {focus} but \"{}\" binds {people:?}",
                    giver.name.full(),
                    seed.template().title
                );
            }
            for text in [
                seed.pitch(giver, &population.village_name, "Rook"),
                seed.thanks(giver, &population.village_name, "Rook"),
            ] {
                assert!(
                    !text.contains(['{', '}', '[', ']']) && !text.contains("???"),
                    "{text}"
                );
            }
        }
    }
    for arc in ARC_TEMPLATES {
        let count = seeded.get(&arc.id.0).copied().unwrap_or(0);
        assert!(
            count >= 5,
            "{} seeded only {count} times in 60 villages",
            arc.title
        );
    }
}

#[test]
fn seeds_use_real_world_facts() {
    let terrain = Terrain::generate(&WorldSettings::new(9));
    let facts = WorldFact::from_features(terrain.features().all());
    assert!(facts.len() >= 10);
    let population = generate_population(9, VillageId(0), Culture::Frontier, 12);
    let seeds = story_seeds(&population, &facts, &[VillageProblem::SealedMine], &[], 0);
    assert!(!seeds.is_empty());
    for seed in seeds {
        for binding in seed.bindings {
            if let super::SlotBinding::Place(fact) = binding {
                assert!(
                    fact.feature.is_some(),
                    "{fact:?} should come from the world"
                );
            }
        }
    }
    let _ = Vars::new();
}
