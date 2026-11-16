//! Story arc lints (WP-STORY): shape, text, variety, no dead ends, rewards in bounds, and that
//! every arc is offered by someone, binds and renders on every generator from V5 to V8, with
//! and without sky, deterministically.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ARC_TEMPLATES, ARC_VARS, ArcEffect, ArcKind, ArcTemplate, OutcomeWhen, Slot, WorldFactKind,
    run::{ArcRun, BindContext, OfferContext, bind_arc, offer_for},
};
use crate::{
    ids::PlayerId,
    people::{
        ArcLog, Character, MemoryFact, PlayerMemory, Population, VillageId, VillageProblem,
        dialogue::grammar,
        generate_population,
        names::Culture,
        report::sample_facts,
        routine::{self, Settlement},
        traits::Tone,
    },
    player::Player,
    poi::PoiJournal,
    quest::{Objective, Prerequisite, StoryPlace, TargetSpec},
    terrain::{Terrain, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings},
};

fn ascii_and_punctuated(what: &str, text: &str) {
    assert!(text.is_ascii(), "{what}: non-ASCII text {text:?}");
    assert!(
        text.ends_with(['.', '!', '?', '"', ')', '\'']),
        "{what}: unpunctuated {text:?}"
    );
}

/// Every line an arc speaks or writes, with what it is.
fn arc_lines(arc: &'static ArcTemplate) -> Vec<(String, &'static str)> {
    let mut lines = Vec::new();
    for tone in Tone::ALL {
        for line in arc.pitch.get(tone) {
            lines.push((format!("pitch ({tone:?})"), *line));
        }
        for line in arc.thanks.get(tone) {
            lines.push((format!("thanks ({tone:?})"), *line));
        }
    }
    for (index, step) in arc.steps.iter().enumerate() {
        lines.push((format!("step {index} journal"), step.journal));
        lines.push((format!("step {index} nudge"), step.nudge));
        if !step.contact.is_empty() {
            lines.push((format!("step {index} contact"), step.contact));
        }
    }
    lines.push(("climax".to_owned(), arc.climax));
    for outcome in arc.outcomes {
        if !outcome.line.is_empty() {
            lines.push((format!("ending {:?} line", outcome.label), outcome.line));
        }
        lines.push((
            format!("ending {:?} epilogue", outcome.label),
            outcome.epilogue,
        ));
    }
    lines
}

fn uses_speak(stage: &[Objective]) -> bool {
    stage
        .iter()
        .any(|objective| matches!(objective, Objective::Speak { .. }))
}

#[allow(clippy::too_many_lines, reason = "one pass over every rule of an arc")]
fn check_arc(arc: &'static ArcTemplate) {
    let name = arc.title;
    assert!(
        !arc.hook.is_empty() && !arc.twist.is_empty() && !arc.consequence.is_empty(),
        "{name}: every arc has a hook, a twist and a consequence"
    );
    assert!(!arc.triggers.is_empty(), "{name}: no triggers");
    assert!(!arc.stages.is_empty(), "{name}: no stages");
    assert_eq!(
        arc.beats.len(),
        arc.stages.len(),
        "{name}: a beat per stage"
    );
    assert_eq!(
        arc.steps.len(),
        arc.stages.len(),
        "{name}: a step per stage"
    );
    for beat in arc.beats {
        assert!(
            !beat.contains(['{', '[']),
            "{name}: beat titles take no variables"
        );
    }
    // Endings: one default, at least one other way it can go, no dead references.
    let defaults = arc
        .outcomes
        .iter()
        .filter(|outcome| outcome.when == OutcomeWhen::Completed)
        .count();
    assert_eq!(defaults, 1, "{name}: exactly one default ending");
    assert!(arc.outcomes.len() >= 2, "{name}: arcs branch on outcome");
    assert!(
        arc.outcomes.len() <= 4,
        "{name}: at most four endings (the talk panel's rows)"
    );
    for outcome in arc.outcomes {
        assert!(!outcome.label.is_empty(), "{name}: empty ending label");
        assert!(
            !outcome.label.contains(['{', '[']),
            "{name}: ending labels take no variables"
        );
        assert!(
            !outcome.epilogue.is_empty(),
            "{name}: every ending has an epilogue"
        );
        if outcome.when == OutcomeWhen::Choice {
            assert!(
                !outcome.line.is_empty(),
                "{name}: a chosen ending needs a reply"
            );
        }
        for effect in outcome.effects {
            if let ArcEffect::FollowUp(next) = effect {
                assert!(next.template().is_some(), "{name}: follow-up {next:?}");
                assert_ne!(*next, arc.id, "{name}: an arc can't follow itself");
            }
            if matches!(effect, ArcEffect::Relationship(_)) {
                assert!(
                    arc.slots
                        .iter()
                        .any(|slot| matches!(slot, Slot::Neighbour(_))),
                    "{name}: a relationship effect needs a neighbour slot"
                );
            }
        }
    }
    // Text: valid templates using only variables the arc provides, never reused.
    let provided = arc.provided_vars();
    let story_targets = arc
        .targets
        .iter()
        .any(|target| matches!(target, TargetSpec::Story(_)));
    let has_place_slot = arc
        .slots
        .iter()
        .any(|slot| matches!(slot, Slot::Place { .. }));
    for (what, line) in arc_lines(arc) {
        if let Err(error) = grammar::check(line, ARC_VARS) {
            panic!("{name} {what}: {error} in {line:?}");
        }
        for var in grammar::variables(line).unwrap_or_default() {
            assert!(
                provided.contains(&var.as_str()),
                "{name} {what}: {{{var}}} has no slot in {line:?}"
            );
            if story_targets && !has_place_slot {
                assert!(
                    !["band", "depth", "place"].contains(&var.as_str()),
                    "{name} {what}: story places are named with {{dest}} and {{where}}"
                );
            }
        }
        ascii_and_punctuated(&format!("{name} {what}"), line);
        // {place} names the first place slot's target and {where} target 0: together they
        // must describe the same place.
        let vars = grammar::variables(line).unwrap_or_default();
        if vars.iter().any(|var| var == "place") && vars.iter().any(|var| var == "where") {
            let first = arc.slots.iter().find_map(|slot| match slot {
                Slot::Place { target, .. } => Some(*target),
                _ => None,
            });
            assert_eq!(
                first,
                Some(0),
                "{name} {what}: {{place}} and {{where}} disagree"
            );
        }
    }
    // Voices: every tone has a pitch; at least three distinct voices.
    let mut pitches = BTreeSet::new();
    for tone in Tone::ALL {
        assert!(!arc.pitch.get(tone).is_empty(), "{name}: no {tone:?} pitch");
        pitches.extend(arc.pitch.get(tone).iter().copied());
    }
    assert!(pitches.len() >= 3, "{name}: pitches need three voices");
    let thanked: Vec<bool> = Tone::ALL
        .iter()
        .map(|tone| !arc.thanks.get(*tone).is_empty())
        .collect();
    assert!(
        thanked.iter().all(|has| *has) || thanked.iter().all(|has| !*has),
        "{name}: thanks for every tone or none"
    );
    if !thanked[0] {
        let default = &arc.outcomes[arc.completed_outcome()];
        assert!(
            !default.line.is_empty(),
            "{name}: without authored thanks the default ending needs a line"
        );
    }
    // Contacts: Speak beats have a contact line, others none.
    for (stage, step) in arc.stages.iter().zip(arc.steps) {
        assert_eq!(
            uses_speak(stage),
            !step.contact.is_empty(),
            "{name}: contact lines belong to Speak beats ({:?})",
            step.journal
        );
    }
    if arc.contact_other {
        assert!(
            arc.slots
                .iter()
                .any(|slot| matches!(slot, Slot::Neighbour(_))),
            "{name}: a contact who is the neighbour needs a neighbour slot"
        );
    }
    // Places.
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
    // The quest engine's rules.
    let def = arc
        .id
        .quest()
        .def()
        .expect("every arc has a quest definition");
    assert_eq!(def.title, arc.title);
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
    // No dead ends: prerequisites a player can meet.
    for prerequisite in arc.prerequisites {
        match prerequisite {
            Prerequisite::Upgrade(_, level) => assert!(
                *level <= crate::economy::MAX_UPGRADE_LEVEL,
                "{name}: unreachable upgrade"
            ),
            Prerequisite::Depth(row) => assert!(*row <= 200, "{name}: unreachable depth"),
            Prerequisite::Quest(quest) => {
                assert!(quest.def().is_some(), "{name}: missing quest");
            }
            _ => {}
        }
    }
    // The kind is honest.
    let objectives: Vec<Objective> = def.objectives().collect();
    let targets_place = |wanted: &[StoryPlace]| {
        arc.targets
            .iter()
            .any(|target| matches!(target, TargetSpec::Story(place) if wanted.contains(place)))
    };
    match arc.kind {
        ArcKind::Pilot => assert!(arc.needs_pilot(), "{name}: pilot arcs need the pilot"),
        ArcKind::Night => assert!(
            objectives
                .iter()
                .any(|objective| matches!(objective, Objective::Vigil { .. })),
            "{name}: night arcs keep a watch"
        ),
        ArcKind::Home => assert!(
            objectives
                .iter()
                .any(|objective| matches!(objective, Objective::Settle { .. })),
            "{name}: home arcs settle somewhere"
        ),
        ArcKind::Sky => assert!(
            targets_place(&[StoryPlace::SkyIsland, StoryPlace::SkyContinent]),
            "{name}: sky arcs go up"
        ),
        ArcKind::Mystery => assert!(
            targets_place(&[StoryPlace::Moon, StoryPlace::SkyIsland, StoryPlace::Hall])
                || arc.targets.iter().any(|target| target.bands().1 >= 4),
            "{name}: mystery arcs go deep or to the Moon"
        ),
        ArcKind::Crew => assert!(
            objectives.iter().any(|objective| objective.is_coop()),
            "{name}: crew arcs use a co-op primitive"
        ),
        ArcKind::Village => assert!(
            objectives
                .iter()
                .any(|objective| matches!(objective, Objective::Speak { .. }))
                || arc
                    .slots
                    .iter()
                    .any(|slot| matches!(slot, Slot::Neighbour(_))),
            "{name}: village arcs are about people"
        ),
        ArcKind::Frontier => {}
    }
    // Sky places need the tank to get there.
    if targets_place(&[
        StoryPlace::SkyIsland,
        StoryPlace::SkyContinent,
        StoryPlace::Moon,
    ]) && arc.kind != ArcKind::Mystery
    {
        assert!(
            arc.prerequisites.iter().any(|prerequisite| matches!(
                prerequisite,
                Prerequisite::Upgrade(crate::quest::UpgradeStat::FuelTank, _)
            )),
            "{name}: a sky arc needs a fuel tank prerequisite"
        );
    }
}

#[test]
fn arcs_are_well_formed_and_varied() {
    let mut ids = BTreeSet::new();
    let mut titles = BTreeSet::new();
    let mut seen_lines: BTreeMap<&'static str, &'static str> = BTreeMap::new();
    let mut kinds: BTreeMap<ArcKind, usize> = BTreeMap::new();
    let mut shapes: BTreeMap<Vec<String>, Vec<&'static str>> = BTreeMap::new();
    let mut primitives = BTreeSet::new();
    for arc in ARC_TEMPLATES.iter() {
        assert!(ids.insert(arc.id), "duplicate arc id {:?}", arc.id);
        assert!(
            titles.insert(arc.title),
            "duplicate arc title {}",
            arc.title
        );
        check_arc(arc);
        *kinds.entry(arc.kind).or_default() += 1;
        let own: BTreeSet<&'static str> =
            arc_lines(arc).into_iter().map(|(_, line)| line).collect();
        for line in own {
            if let Some(other) = seen_lines.insert(line, arc.title) {
                panic!("{}: line reused from {other}: {line:?}", arc.title);
            }
        }
        let shape: Vec<String> = arc
            .stages
            .iter()
            .map(|stage| {
                stage
                    .iter()
                    .map(|objective| {
                        let text = format!("{objective:?}");
                        let kind = text.split([' ', '{']).next().unwrap_or("").to_owned();
                        primitives.insert(kind.clone());
                        kind
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            })
            .collect();
        shapes.entry(shape).or_default().push(arc.title);
    }
    let count = ARC_TEMPLATES.len();
    assert!((60..=100).contains(&count), "{count} arcs (want 60-100)");
    for kind in ArcKind::ALL {
        let have = kinds.get(&kind).copied().unwrap_or(0);
        assert!(have >= 3, "only {have} {kind:?} arcs");
    }
    // Variety: no beat sequence shared by more than three arcs, most primitives in use.
    for (shape, arcs) in &shapes {
        assert!(arcs.len() <= 3, "{shape:?} is shared by {arcs:?}");
    }
    assert!(
        primitives.len() >= 16,
        "only {} primitives in use: {primitives:?}",
        primitives.len()
    );
    // Follow-ups chain onward from the first arcs into the mystery.
    let mystery = ARC_TEMPLATES
        .iter()
        .filter(|arc| {
            arc.outcomes.iter().any(|outcome| {
                outcome.effects.iter().any(|effect| {
                    matches!(effect, ArcEffect::FollowUp(next) if next.template().is_some_and(|next| next.kind == ArcKind::Mystery))
                })
            })
        })
        .count();
    assert!(mystery >= 5, "the mystery needs threads leading into it");
}

/// A player who has done everything (every prerequisite met).
fn veteran() -> Player {
    let mut player = Player::new(0.0, 0.0);
    player.fuel_tank_level = crate::economy::MAX_UPGRADE_LEVEL;
    player.engine_level = crate::economy::MAX_UPGRADE_LEVEL;
    player.drill_strength = crate::economy::MAX_UPGRADE_LEVEL;
    player.hazard_gear.altitude_intake = crate::climb::INTAKE_TIERS[3];
    player.quests.deepest_row = 100_000;
    player
}

#[test]
fn every_arc_is_offered_by_someone() {
    let facts = sample_facts();
    let player = veteran();
    let history = [
        MemoryFact::FoundArtifact {
            name: "Resonant Idol".into(),
        },
        MemoryFact::ReachedDepth { metres: 200 },
    ];
    let mut offered: BTreeMap<u16, u32> = BTreeMap::new();
    let mut givers = 0;
    for village in 0..240_u32 {
        let culture = Culture::ALL[village as usize % Culture::ALL.len()];
        let population = generate_population(11, VillageId(village), culture, 9);
        let problems = [VillageProblem::ALL[village as usize % VillageProblem::ALL.len()]];
        let ctx = OfferContext {
            settings: &WorldSettings::new(11),
            population: &population,
            facts: &facts,
            problems: &problems,
            history: &history,
            day: village % 9,
        };
        for character in &population.characters {
            let log = ArcLog::default();
            let seed = offer_for(character, &ctx, &log, &player);
            assert_eq!(
                seed,
                offer_for(character, &ctx, &log, &player),
                "deterministic"
            );
            if let Some(seed) = seed {
                givers += 1;
                *offered.entry(seed.arc.0).or_default() += 1;
            }
        }
    }
    assert!(
        givers >= 240 * 9 * 3 / 4,
        "most people have a story ({givers})"
    );
    let missing: Vec<&str> = ARC_TEMPLATES
        .iter()
        .filter(|arc| offered.get(&arc.id.0).copied().unwrap_or(0) == 0)
        .map(|arc| arc.title)
        .collect();
    assert!(missing.is_empty(), "never offered: {missing:?}");
    // No single arc crowds out the rest.
    let most = offered.values().copied().max().unwrap_or(0);
    assert!(
        most * 10 <= givers,
        "one arc is offered {most} of {givers} times"
    );
}

#[test]
fn offers_move_on_after_finishing_and_follow_ups_come_first() {
    let facts = sample_facts();
    let player = veteran();
    let population = generate_population(5, VillageId(7), Culture::Frontier, 10);
    let ctx = OfferContext {
        settings: &WorldSettings::new(5),
        population: &population,
        facts: &facts,
        problems: &[],
        history: &[],
        day: 4,
    };
    let character = population
        .characters
        .iter()
        .find(|character| offer_for(character, &ctx, &ArcLog::default(), &player).is_some())
        .expect("someone has a story");
    let first = offer_for(character, &ctx, &ArcLog::default(), &player).expect("offer");
    let mut log = ArcLog::default();
    log.finished.insert((character.id, first.arc));
    let next = offer_for(character, &ctx, &log, &player);
    assert_ne!(
        next.map(|seed| seed.arc),
        Some(first.arc),
        "a finished arc isn't offered again"
    );
    log.follow_ups.insert((character.id, super::ArcId(402)));
    let follow = offer_for(character, &ctx, &log, &player).expect("follow-up");
    assert_eq!(follow.arc, super::ArcId(402));
    // A fresh player isn't offered arcs they can't reach.
    let newcomer = Player::new(0.0, 0.0);
    for character in &population.characters {
        if let Some(seed) = offer_for(character, &ctx, &ArcLog::default(), &newcomer) {
            assert!(
                seed.template()
                    .prerequisites
                    .iter()
                    .all(|prerequisite| prerequisite.met(&newcomer, ctx.settings)),
                "{} offered too early",
                seed.template().title
            );
        }
    }
}

/// Worlds every arc must bind on: each generator from V5 to V8 on a few seeds.
fn binding_worlds() -> Vec<WorldSettings> {
    let seeds: u64 = std::env::var("FATHERLOAD_STORY_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let mut worlds = Vec::new();
    for generator in [
        GeneratorVersion::V5,
        GeneratorVersion::V6,
        GeneratorVersion::V7,
        GeneratorVersion::V8,
    ] {
        for seed in 0..seeds {
            worlds.push(WorldSettings {
                generator,
                ..WorldSettings::new(0x5707_0000 + seed * 977)
            });
        }
    }
    worlds
}

/// The settlement stories are bound from: the first village (or the town where there are no
/// villages, with a stand-in population).
fn giver_home(settings: &WorldSettings) -> (TilePosition, VillageId, String, Population) {
    if let Some(found) = crate::village::village(settings, crate::village::placement::surface_id(1))
    {
        let settlement = Settlement::of_village(&found);
        return (
            TilePosition {
                x: settlement.center_column(),
                y: settlement.ground_row - 1,
            },
            found.id,
            found.name.clone(),
            found.population.clone(),
        );
    }
    let population = routine::town_population(settings.seed);
    (
        TilePosition {
            x: crate::people::routine::TOWN_VILLAGE.0.cast_signed() + 115,
            y: 4,
        },
        routine::TOWN_VILLAGE,
        routine::TOWN_NAME.to_owned(),
        population,
    )
}

fn assert_rendered(run: &ArcRun, what: &str) {
    let template = run.template().expect("template");
    let mut texts = vec![run.journal_text()];
    texts.extend(run.story_so_far());
    for step in template.steps {
        texts.push(run.cast.render(step.journal, 1));
        texts.push(run.cast.render(step.nudge, 1));
        texts.push(run.cast.render(step.contact, 1));
    }
    texts.push(run.cast.render(template.climax, 1));
    for outcome in template.outcomes {
        texts.push(run.cast.render(outcome.line, 1));
        texts.push(run.cast.render(outcome.epilogue, 1));
    }
    for tone in Tone::ALL {
        for line in template.pitch.get(tone) {
            texts.push(run.cast.render(line, 1));
        }
    }
    for text in texts {
        assert!(
            !text.contains(['{', '}', '[', ']']) && !text.contains("someone someone"),
            "{what} {}: {text}",
            template.title
        );
    }
}

/// A stand-in giver for `arc` (the binding only needs a name and a voice).
fn stand_in_giver(population: &Population) -> &Character {
    &population.characters[0]
}

#[test]
fn every_arc_binds_and_renders_on_v5_to_v8_with_and_without_sky() {
    for settings in binding_worlds() {
        let terrain = Terrain::generate(&settings);
        let (origin, home, home_name, population) = giver_home(&settings);
        let giver = stand_in_giver(&population);
        let journal = PoiJournal::default();
        let taken = BTreeSet::new();
        let ctx = BindContext {
            terrain: &terrain,
            origin,
            home,
            home_name: &home_name,
            journal: &journal,
            taken: &taken,
            owner: PlayerId::new(1),
            player_name: "Rook",
            tick: 0,
            day: 1,
            hull_tenths: 1_000,
        };
        for arc in ARC_TEMPLATES.iter() {
            let seed = stand_in_seed(arc, giver);
            let run = bind_arc(&seed, giver, &ctx).unwrap_or_else(|| {
                panic!(
                    "{} does not bind on {:?} seed {}",
                    arc.title, settings.generator, settings.seed
                )
            });
            assert_eq!(
                run,
                bind_arc(&seed, giver, &ctx).expect("again"),
                "{}: binding is deterministic",
                arc.title
            );
            assert_eq!(run.cast.places.len(), arc.targets.len());
            // Sky places stand in on worlds without a sky.
            for (place, spec) in run.cast.places.iter().zip(arc.targets) {
                if let TargetSpec::Story(story) = spec
                    && story.is_sky()
                    && !settings.generator.has_sky()
                {
                    assert!(
                        place.stand_in || *story == StoryPlace::Moon,
                        "{}: {story:?} should stand in on {:?}",
                        arc.title,
                        settings.generator
                    );
                }
            }
            assert_rendered(&run, &format!("{:?}", settings.generator));
        }
    }
}

#[test]
fn the_memory_of_an_arc_is_remembered_by_dialogue() {
    // Completed arcs become memories later dialogue brings up by title.
    let population = generate_population(3, VillageId(9), Culture::Frontier, 6);
    let character = &population.characters[0];
    let player = PlayerId::new(1);
    let mut state = crate::people::CharacterState::default();
    state.remember(player, 1, MemoryFact::Met);
    let arc = ARC_TEMPLATES[0].id;
    state.remember(player, 2, MemoryFact::ArcCompleted { arc, outcome: 0 });
    let ctx = crate::people::TalkContext {
        seed: 3,
        day: 3,
        player,
        player_name: "Rook",
        population: Some(&population),
        facts: &[],
    };
    let line = crate::people::say(character, &mut state, &ctx, crate::people::Topic::Memory)
        .expect("a memory to bring up");
    assert!(line.text.contains(ARC_TEMPLATES[0].title), "{}", line.text);
    let memory: PlayerMemory = state.memory(player);
    assert!(memory.reputation > 0);
}

#[test]
fn sky_arcs_wait_until_the_rig_can_climb_there() {
    // On a world with a sky, an arc that sends the rig up is only offered once the measured
    // climb table says it gets there: the isles, then the dead-air gate for the great lands
    // and the Moon. Without a sky the stand-ins are on the ground, so only the authored
    // prerequisites count.
    let sky = WorldSettings::new(11);
    assert!(sky.generator.has_sky());
    let ground = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(11)
    };
    // A stock rig doesn't reach the low isles.
    let mut grounded = veteran();
    grounded.fuel_tank_level = 1;
    grounded.engine_level = 1;
    grounded.hazard_gear.altitude_intake = 0;
    assert!(!crate::milestone::island_capable(&grounded));
    // A big tank without altitude intakes meets the great lands' authored tank prerequisites
    // but stalls in the dead air.
    let mut island_hopper = veteran();
    island_hopper.fuel_tank_level = 12;
    island_hopper.hazard_gear.altitude_intake = 0;
    assert!(crate::milestone::island_capable(&island_hopper));
    assert!(!crate::milestone::crosses_dead_air(&island_hopper));
    let veteran = veteran();
    assert!(crate::milestone::crosses_dead_air(&veteran));
    let goes_up = |arc: &ArcTemplate| {
        arc.targets
            .iter()
            .any(|target| matches!(target, TargetSpec::Story(place) if place.is_sky()))
    };
    let mut gated = 0;
    for arc in ARC_TEMPLATES.iter() {
        assert!(super::run::unlocked(arc, &veteran, &sky), "{}", arc.title);
        if goes_up(arc) {
            gated += 1;
            assert!(
                !super::run::unlocked(arc, &grounded, &sky),
                "{} is offered to a rig that can't climb there",
                arc.title
            );
        }
        let past_dead_air = arc.targets.iter().any(|target| {
            matches!(
                target,
                TargetSpec::Story(StoryPlace::SkyContinent | StoryPlace::Moon)
            )
        });
        if past_dead_air {
            assert!(
                !super::run::unlocked(arc, &island_hopper, &sky),
                "{} is offered before the rig crosses the dead air",
                arc.title
            );
        }
        let authored = arc
            .prerequisites
            .iter()
            .all(|prerequisite| prerequisite.met(&grounded, &ground));
        assert_eq!(
            super::run::unlocked(arc, &grounded, &ground),
            authored,
            "{}: without a sky only the authored prerequisites count",
            arc.title
        );
    }
    assert!(gated >= 8, "{gated} sky-bound arcs");
}

#[test]
fn arcs_bind_from_settlements_anywhere_in_the_endless_world() {
    // Bound from villages far to the west and east (regional queries from the giver's own
    // settlement), on every generator with people, every arc still finds its places close to
    // the giver rather than back at the spawn.
    for generator in [
        GeneratorVersion::V6,
        GeneratorVersion::V7,
        GeneratorVersion::V8,
    ] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(0x00E1_D1E5)
        };
        let terrain = Terrain::generate(&settings);
        let homes: Vec<_> = [-60, -17, 9, 41, 85]
            .into_iter()
            .filter_map(|site| {
                (site..site + 12).find_map(|site| {
                    crate::village::village(&settings, crate::village::placement::surface_id(site))
                })
            })
            .collect();
        assert!(homes.len() >= 4, "{generator:?}: villages far out");
        for found in homes {
            let settlement = Settlement::of_village(&found);
            let origin = TilePosition {
                x: settlement.center_column(),
                y: settlement.ground_row - 1,
            };
            let giver = &found.population.characters[0];
            let journal = PoiJournal::default();
            let taken = BTreeSet::new();
            let ctx = BindContext {
                terrain: &terrain,
                origin,
                home: found.id,
                home_name: &found.name,
                journal: &journal,
                taken: &taken,
                owner: PlayerId::new(1),
                player_name: "Rook",
                tick: 0,
                day: 1,
                hull_tenths: 1_000,
            };
            for arc in ARC_TEMPLATES.iter() {
                let run = bind_arc(&stand_in_seed(arc, giver), giver, &ctx).unwrap_or_else(|| {
                    panic!(
                        "{} does not bind from {} ({generator:?}, column {})",
                        arc.title, found.name, origin.x
                    )
                });
                // The Moon hangs over one fixed column and the town is at the spawn (the last
                // fallback); everything else is near the giver.
                for (target, place) in run.quest.targets.iter().zip(&run.cast.places) {
                    let fixed = (place.noun == "Moon" && !place.stand_in) || place.noun == "town";
                    assert!(
                        fixed || (target.center.x - origin.x).abs() < 6_000,
                        "{}: bound {} columns from {}",
                        arc.title,
                        target.center.x - origin.x,
                        found.name
                    );
                }
                assert_rendered(&run, &format!("{generator:?} {}", found.name));
            }
        }
    }
}

/// A seed for `arc` from `giver` with stand-in slot bindings (binding only needs the shape).
fn stand_in_seed(arc: &ArcTemplate, giver: &Character) -> super::StorySeed {
    super::StorySeed {
        arc: arc.id,
        giver: giver.id,
        source: super::SeedSource::Role(giver.role),
        bindings: arc
            .slots
            .iter()
            .map(|slot| match slot {
                Slot::Absent(_) => super::SlotBinding::Absent {
                    name: "Tobin".into(),
                    relation: "brother".into(),
                },
                Slot::Neighbour(_) => super::SlotBinding::Neighbour {
                    id: None,
                    name: "Marla".into(),
                    relation: "rival".into(),
                },
                Slot::Place { kind, .. } => super::SlotBinding::Place(super::WorldFact {
                    kind: *kind,
                    band: 2,
                    row: 60,
                    feature: None,
                }),
            })
            .collect(),
        score: 0,
    }
}
