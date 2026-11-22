//! The story report (`fatherload --story-report`): samples villages, prints their residents
//! with dialogue samples across several visits, then the story seeds the village grows.

use std::{collections::BTreeMap, fmt::Write as _};

use super::{
    Character, CharacterState, MemoryFact, PlayerMemory, Population, SeedSource, TalkContext,
    VillageId, VillageProblem, WorldFact, WorldFactKind, character::AbsentFate, dialogue,
    generate_population, names::Culture, population_size, story_seeds, traits::Want,
};
use crate::{
    ids::PlayerId,
    quest::{Objective, TargetSpec},
};

/// The player the report talks as.
const PLAYER_NAME: &str = "Rook";

/// The culture of report village `village` (every culture shows up in the first six).
const fn report_culture(seed: u64, village: u32) -> Culture {
    let index = (village as usize + (seed % 6) as usize) % Culture::ALL.len();
    Culture::ALL[index]
}

/// Problems report villages get, so problem-driven arcs show up too.
const fn report_problems(village: u32) -> &'static [VillageProblem] {
    match village % 4 {
        0 => &[VillageProblem::SealedMine],
        1 => &[VillageProblem::Feud],
        2 => &[VillageProblem::MissingPerson],
        _ => &[],
    }
}

/// Stand-in facts when no world is at hand (the CLI passes a real world's).
#[must_use]
pub fn sample_facts() -> Vec<WorldFact> {
    let fact = |kind, band, row| WorldFact {
        kind,
        band,
        row,
        feature: None,
    };
    vec![
        fact(WorldFactKind::Derelict, 3, 92),
        fact(WorldFactKind::SealedVault, 5, 176),
        fact(WorldFactKind::Formation, 3, 101),
        fact(WorldFactKind::SupplyCache, 2, 48),
        fact(WorldFactKind::Cavern, 2, 61),
        fact(WorldFactKind::ArtifactBed, 2, 70),
        fact(WorldFactKind::HazardPocket, 1, 31),
        fact(WorldFactKind::Motherlode, 4, 133),
    ]
}

/// Populations for the report: villages in order until `count` residents are covered.
#[must_use]
pub fn report_villages(seed: u64, count: usize) -> Vec<Population> {
    let mut villages = Vec::new();
    let mut total = 0;
    let mut village = 0;
    while total < count {
        let id = VillageId(village);
        let size = population_size(seed, id).min(count - total);
        let size = size.max(1);
        villages.push(generate_population(
            seed,
            id,
            report_culture(seed, village),
            size,
        ));
        total += size;
        village += 1;
    }
    villages
}

/// The full report text.
#[must_use]
pub fn story_report(seed: u64, count: usize, facts: &[WorldFact]) -> String {
    let mut out = String::new();
    let villages = report_villages(seed, count);
    let _ = writeln!(
        out,
        "Fatherload story report: seed {}, {count} characters in {} villages, {} world facts\n",
        crate::worldgen::format_seed(seed),
        villages.len(),
        facts.len()
    );
    let player = PlayerId::new(1);
    let mut tones = BTreeMap::new();
    let mut roles = BTreeMap::new();
    let mut wants = BTreeMap::new();
    let mut arcs = BTreeMap::new();
    for population in &villages {
        let _ = writeln!(
            out,
            "################ {} ({} village, {} style, {} residents) ################\n",
            population.village_name,
            population.culture.name(),
            population.culture.building_style(),
            population.characters.len()
        );
        let seeds = story_seeds(
            population,
            facts,
            report_problems(population.village.0),
            &[],
            0,
        );
        for character in &population.characters {
            *tones.entry(character.tone.name()).or_insert(0) += 1;
            *roles.entry(character.role.name()).or_insert(0) += 1;
            *wants.entry(character.want.name()).or_insert(0) += 1;
            profile(&mut out, seed, character, population, facts, player);
        }
        let _ = writeln!(
            out,
            "---- Story seeds in {} ----\n",
            population.village_name
        );
        if seeds.is_empty() {
            let _ = writeln!(out, "(none)\n");
        }
        for story in &seeds {
            *arcs.entry(story.template().title).or_insert(0) += 1;
            let Some(giver) = population.get(story.giver) else {
                continue;
            };
            let template = story.template();
            let _ = writeln!(
                out,
                "* \"{}\" from {} ({}), {} tier, because: {}",
                template.title,
                giver.name.full(),
                giver.role.name(),
                template.tier.name(),
                source_text(&story.source, population)
            );
            let _ = writeln!(
                out,
                "  pitch:  \"{}\"",
                story.pitch(giver, &population.village_name, PLAYER_NAME)
            );
            for (beat, stage) in template.beats.iter().zip(template.stages) {
                let steps: Vec<String> = stage
                    .iter()
                    .map(|objective| describe_objective(*objective, template.targets))
                    .collect();
                let _ = writeln!(out, "  beat:   {beat}: {}", steps.join("; "));
            }
            for twist in template.twists {
                let _ = writeln!(out, "  rule:   {}", twist.describe());
            }
            let _ = writeln!(out, "  twist:  {}", template.twist);
            let endings: Vec<&str> = template
                .outcomes
                .iter()
                .map(|outcome| outcome.label)
                .collect();
            let _ = writeln!(out, "  ends:   {}", endings.join(" / "));
            let _ = writeln!(
                out,
                "  thanks: \"{}\"",
                story.thanks(giver, &population.village_name, PLAYER_NAME)
            );
            let _ = writeln!(out, "  reward: {}\n", template.reward.summary());
        }
    }
    let _ = writeln!(out, "================ Summary ================");
    let _ = writeln!(out, "tones: {}", tally(&tones));
    let _ = writeln!(out, "roles: {}", tally(&roles));
    let _ = writeln!(out, "wants: {}", tally(&wants));
    let _ = writeln!(out, "arcs seeded: {}", tally(&arcs));
    out
}

/// "gruff 3, dry 2".
fn tally(counts: &BTreeMap<&str, u32>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(clippy::too_many_lines, reason = "one report block, top to bottom")]
fn profile(
    out: &mut String,
    seed: u64,
    character: &Character,
    population: &Population,
    facts: &[WorldFact],
    player: PlayerId,
) {
    let p = character.personality;
    let _ = writeln!(
        out,
        "== {} ({}, {}) - {} of {}",
        character.name.full(),
        character.pronouns.label(),
        character.age,
        character.role.name(),
        population.village_name
    );
    let _ = writeln!(
        out,
        "   look: {}. Voice: {} (with {} streak). Offers {}.",
        character.appearance.describe(),
        character.tone.name(),
        super::dialogue::grammar::with_article(character.secondary_tone.name()),
        character.offer.name()
    );
    let _ = writeln!(
        out,
        "   trades (WP-PEOPLE): {}; loves a gift of {}.",
        super::offers::offers_of(character)
            .iter()
            .map(|offer| {
                let price = if offer.price == 0 {
                    String::new()
                } else {
                    format!(" {}cr", offer.price)
                };
                format!("{}{price} [{:?}]", offer.title(), offer.needs)
            })
            .collect::<Vec<_>>()
            .join(", "),
        super::offers::favourite_mineral(character)
            .name()
            .to_lowercase()
    );
    let _ = writeln!(
        out,
        "   traits: O{} C{} E{} A{} N{} ({}). Quirks: {}.",
        p.openness,
        p.conscientiousness,
        p.extraversion,
        p.agreeableness,
        p.neuroticism,
        p.adjectives().join(", "),
        character
            .quirks
            .iter()
            .map(|quirk| quirk.describe())
            .collect::<Vec<_>>()
            .join("; ")
    );
    let focus = character
        .focus_name(Some(population))
        .map(|name| format!(" ({name})"))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "   values {}; wants to {}{focus}; fears {}.",
        character
            .values
            .iter()
            .map(|value| value.name())
            .collect::<Vec<_>>()
            .join(" and "),
        character.want.name(),
        character.fear.name()
    );
    for beat in &character.history {
        let _ = writeln!(out, "   - {}", beat.text);
    }
    let people: Vec<String> = population
        .relationships_of(character.id)
        .filter_map(|edge| {
            let other = population.get(edge.to)?;
            Some(format!(
                "{} {}",
                edge.kind.word(other.pronouns),
                other.name.short()
            ))
        })
        .chain(character.absent.iter().map(|person| {
            let fate = match person.fate {
                AbsentFate::WentDeep => "went deep",
                AbsentFate::MovedAway => "moved away",
                AbsentFate::OldRival => "up the valley",
                AbsentFate::Creditor => "owed money",
                AbsentFate::Estranged => "fell out",
            };
            format!("{} {} ({fate})", person.relation, person.name.formal())
        }))
        .collect();
    if !people.is_empty() {
        let _ = writeln!(out, "   knows: {}", people.join(", "));
    }

    let mut state = CharacterState::default();
    let ctx = |day| TalkContext {
        seed,
        day,
        player,
        player_name: PLAYER_NAME,
        population: Some(population),
        facts,
    };
    for day in 1..=6 {
        let lines = dialogue::talk(character, &mut state, &ctx(day));
        let text: Vec<String> = lines
            .iter()
            .map(|line| format!("\"{}\"", line.text))
            .collect();
        let _ = writeln!(out, "   visit {day}: {}", text.join(" "));
    }
    let (event, what) = memorable_event(character, population);
    state.remember(player, 7, event);
    let lines = dialogue::talk(character, &mut state, &ctx(8));
    let text: Vec<String> = lines
        .iter()
        .map(|line| format!("\"{}\"", line.text))
        .collect();
    let _ = writeln!(out, "   after {what}: {}", text.join(" "));
    let gift = dialogue::say(
        character,
        &mut state,
        &ctx(8),
        dialogue::Topic::Gift {
            liked: !character.voice_seed.is_multiple_of(3),
        },
    );
    if let Some(gift) = gift {
        let _ = writeln!(out, "   gift: \"{}\"", gift.text);
    }
    let memory: PlayerMemory = state.memory(player);
    let _ = writeln!(
        out,
        "   now: {:?}, reputation {}, mood {}\n",
        memory.standing(),
        memory.reputation,
        super::MoodBand::of(state.mood(character, 8, Some(player))).name()
    );
}

fn source_text(source: &SeedSource, population: &Population) -> String {
    match source {
        SeedSource::Want(want) => format!("wants to {}", want.name()),
        SeedSource::Fear(fear) => format!("fears {}", fear.name()),
        SeedSource::Value(value) => format!("values {}", value.name()),
        SeedSource::Quirk(quirk) => quirk.describe().to_owned(),
        SeedSource::Relationship { kind, other } => format!(
            "{} {}",
            kind.noun(),
            population
                .get(*other)
                .map_or("a neighbour", |other| other.name.short())
        ),
        SeedSource::Absent { fate, name } => format!("{name} ({fate:?})"),
        SeedSource::Problem(problem) => format!("village problem: {problem:?}"),
        SeedSource::History(fact) => format!("player history: {fact}"),
        SeedSource::Role(role) => format!("works as a {}", role.name()),
        SeedSource::FollowUp(arc) => format!(
            "a promised follow-up to \"{}\"",
            arc.template()
                .map_or("an earlier story", |template| template.title)
        ),
    }
}

/// A player who has done everything (every arc's prerequisites met), for the arc report.
fn veteran() -> crate::player::Player {
    let mut player = crate::player::Player::new(0.0, 0.0);
    player.fuel_tank_level = crate::economy::MAX_UPGRADE_LEVEL;
    player.engine_level = crate::economy::MAX_UPGRADE_LEVEL;
    player.drill_strength = crate::economy::MAX_UPGRADE_LEVEL;
    player.quests.deepest_row = 100_000;
    player
}

/// The full-arcs report (`fatherload --story-report --arcs`).
///
/// Walks the surface villages of the world `settings` describe, east and west of town, asks
/// each resident what story they have for a seasoned player, binds it to the real world from
/// their village exactly as play would, and prints it whole: the pitch in their voice, every
/// beat's journal text, nudge and contact, the climax, and every ending's reply and epilogue.
/// Stops after `count` arcs.
#[must_use]
pub fn arc_report(settings: &crate::worldgen::WorldSettings, count: usize) -> String {
    use super::story::run::{BindContext, OfferContext, bind_arc, ending_reply, offer_for};
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Fatherload arc report: seed {}, generator {:?}, up to {count} arcs\n",
        crate::worldgen::format_seed(settings.seed),
        settings.generator
    );
    if !settings.generator.has_villages() {
        let _ = writeln!(
            out,
            "(this generator has no villages, so nobody offers stories)"
        );
        return out;
    }
    let terrain = crate::terrain::Terrain::generate(settings);
    let player = veteran();
    let journal = crate::poi::PoiJournal::default();
    let taken = std::collections::BTreeSet::new();
    let mut printed = 0;
    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut seen_arcs = std::collections::BTreeSet::new();
    'villages: for step in 1..=40_i32 {
        for site in [step, -step] {
            let Some(found) =
                crate::village::village(settings, crate::village::placement::surface_id(site))
            else {
                continue;
            };
            let settlement = super::routine::Settlement::of_village(&found);
            let facts = crate::village::village_facts(settings, &found);
            let problems: Vec<VillageProblem> = found.problem.into_iter().collect();
            let day = site.unsigned_abs() * 3;
            let ctx = OfferContext {
                settings,
                frame: crate::position::Frame::FIXED,
                population: &found.population,
                facts: &facts,
                problems: &problems,
                history: &[],
                day,
            };
            for character in &found.population.characters {
                let Some(seed) = offer_for(character, &ctx, &super::ArcLog::default(), &player)
                else {
                    continue;
                };
                // One telling per arc keeps the report varied.
                if !seen_arcs.insert(seed.arc) {
                    continue;
                }
                let bind = BindContext {
                    terrain: &terrain,
                    origin: crate::terrain::TilePosition {
                        x: settlement.center_column(),
                        y: settlement.ground_row - 1,
                    },
                    home: found.id,
                    home_name: &found.name,
                    journal: &journal,
                    taken: &taken,
                    owner: PlayerId::new(1),
                    player_name: PLAYER_NAME,
                    tick: 0,
                    day,
                    hull_tenths: 1_000,
                };
                let Some(run) = bind_arc(&seed, character, &bind) else {
                    let _ = writeln!(out, "!! {} did not bind\n", seed.template().title);
                    continue;
                };
                write_arc(&mut out, &run, character, &found.population, ending_reply);
                *kinds.entry(seed.template().kind.name()).or_default() += 1;
                printed += 1;
                if printed >= count {
                    break 'villages;
                }
            }
        }
    }
    let _ = writeln!(out, "== {printed} arcs; by kind: {kinds:?}");
    out
}

/// One arc, told whole.
fn write_arc(
    out: &mut String,
    run: &super::ArcRun,
    giver: &Character,
    population: &Population,
    reply: fn(&super::ArcCast, &super::story::ArcOutcome, &Character, u64) -> String,
) {
    let Some(template) = run.template() else {
        return;
    };
    let cast = &run.cast;
    let _ = writeln!(
        out,
        "================ \"{}\" ({}, {} tier) ================",
        template.title,
        template.kind.name(),
        template.tier.name()
    );
    let _ = writeln!(
        out,
        "{}, {} of {} ({} voice), because: {}",
        cast.giver_full,
        giver.role.name(),
        cast.home,
        giver.tone.name(),
        source_text(&cast.source, population)
    );
    for place in &cast.places {
        let _ = writeln!(
            out,
            "  place: {} = {} ({}){}",
            place.noun,
            place.dest,
            place.phrase,
            if place.stand_in { " [stand-in]" } else { "" }
        );
    }
    if let Some(contact) = &cast.contact {
        let _ = writeln!(out, "  contact: {} of {}", contact.name, contact.home);
    }
    let _ = writeln!(
        out,
        "\n{}: \"{}\"",
        cast.giver_name,
        cast.voiced(&template.pitch, Some(giver), 1)
    );
    for (index, (step, beat)) in template.steps.iter().zip(template.beats).enumerate() {
        let _ = writeln!(out, "\n  [{}] {beat}", index + 1);
        let _ = writeln!(out, "      Journal: {}", cast.render(step.journal, 3));
        let _ = writeln!(
            out,
            "      {} (if you look in): \"{}\"",
            cast.giver_name,
            cast.render(step.nudge, 3)
        );
        if !step.contact.is_empty() {
            let who = cast
                .contact
                .as_ref()
                .map_or("Whoever's about", |contact| contact.name.as_str());
            let _ = writeln!(out, "      {who}: \"{}\"", cast.render(step.contact, 5));
        }
    }
    let _ = writeln!(out, "\n  CLIMAX: {}", cast.render(template.climax, 3));
    for ending in template.outcomes {
        let when = match ending.when {
            super::story::OutcomeWhen::Completed => "ending",
            super::story::OutcomeWhen::Choice => "choice",
            super::story::OutcomeWhen::Abandoned => "if abandoned",
        };
        let _ = writeln!(out, "\n  -> {when}: {}", ending.label);
        let spoken = reply(cast, ending, giver, 2);
        if !spoken.is_empty() && ending.when != super::story::OutcomeWhen::Abandoned {
            let _ = writeln!(out, "     {}: \"{spoken}\"", cast.giver_name);
        }
        let _ = writeln!(out, "     Epilogue: {}", cast.render(ending.epilogue, 4));
    }
    let _ = writeln!(out);
}

/// Something the player could plausibly have done for this character.
fn memorable_event(character: &Character, population: &Population) -> (MemoryFact, String) {
    if let Some(person) = character
        .absent
        .iter()
        .find(|person| person.fate == AbsentFate::WentDeep)
    {
        return (
            MemoryFact::FoundPerson {
                name: person.name.short().to_owned(),
                relation: person.relation.clone(),
            },
            format!("you find {}'s rig", person.name.short()),
        );
    }
    match character.want {
        Want::UncoverDeepSecret => (
            MemoryFact::FoundArtifact {
                name: "Resonant Idol".to_owned(),
            },
            "you bring up a Resonant Idol".to_owned(),
        ),
        Want::ProtectVillage => (MemoryFact::HelpedVillage, "you help the village".to_owned()),
        Want::SeeTheWorld => (
            MemoryFact::ReachedDepth { metres: 412 },
            "you reach 412m".to_owned(),
        ),
        _ => {
            let neighbour = population
                .relationships_of(character.id)
                .find(|edge| edge.kind.is_family())
                .and_then(|edge| population.get(edge.to));
            neighbour.map_or_else(
                || {
                    (
                        MemoryFact::ArcCompleted {
                            arc: super::ArcId(7),
                            outcome: 0,
                        },
                        "you finish \"Heirloom Below\" for them".to_owned(),
                    )
                },
                |other| {
                    (
                        MemoryFact::RescuedNeighbour {
                            name: other.name.short().to_owned(),
                        },
                        format!("you rescue {}", other.name.short()),
                    )
                },
            )
        }
    }
}

/// A one-line description of an objective for the report.
#[must_use]
pub fn describe_objective(objective: Objective, targets: &[TargetSpec]) -> String {
    let noun = |index: u8| {
        targets
            .get(usize::from(index))
            .map_or("site", |target| target.noun())
    };
    match objective {
        Objective::Deliver { item, count } => format!("deliver {count} {}", item.name()),
        Objective::Reach { target } => format!("reach the {}", noun(target)),
        Objective::ReachDepth { row } => format!("reach {row}m"),
        Objective::Dig { target, count } => format!("dig {count} tiles at the {}", noun(target)),
        Objective::Scan { target } => format!("scan the {}", noun(target)),
        Objective::OpenPoi { target } => format!("open the {}", noun(target)),
        Objective::Survive { target, seconds } => {
            format!("survive {seconds}s in the {}", noun(target))
        }
        Objective::PickUp { target, parcel } => {
            format!("pick up the {parcel} at the {}", noun(target))
        }
        Objective::ReturnToSurface => "return to the surface".to_owned(),
        Objective::Build { target, kind } => {
            format!("build a {} at the {}", kind.name(), noun(target))
        }
        Objective::Detonate { target, spots } => {
            format!("detonate {spots} marks in order at the {}", noun(target))
        }
        Objective::RecoverLog { target } => format!("recover the log from the {}", noun(target)),
        Objective::Simultaneous { target, spots, .. } => {
            format!(
                "hold {spots} marks at once at the {} (co-op; solo: be quick)",
                noun(target)
            )
        }
        Objective::Relay { target, .. } => format!("relay a marker to the {}", noun(target)),
        Objective::HoldAndClimb { goal, .. } => {
            format!(
                "hold the switch while a crewmate reaches the {}",
                noun(goal)
            )
        }
        Objective::Escort { target, .. } => format!("escort a haul up from the {}", noun(target)),
        Objective::SplitDelivery { item, count, .. } => {
            format!("deliver {count} {} to two buildings together", item.name())
        }
        Objective::OnFoot { target } => format!("reach the {} on foot", noun(target)),
        Objective::HandDeliver { target, parcel } => {
            format!("carry the {parcel} by hand into the {}", noun(target))
        }
        Objective::Vigil { target, seconds } => {
            format!("keep watch at the {} for {seconds}s of night", noun(target))
        }
        Objective::Speak { target } => format!("find the contact in the {}", noun(target)),
        Objective::Settle { kind } => format!("have a {} standing in your claim", kind.name()),
        Objective::NightHunt { count } => format!("bring down {count} night monsters"),
        Objective::Dawn { target } => format!("see the night through at the {}", noun(target)),
    }
}

#[cfg(test)]
mod tests {
    use super::{report_villages, sample_facts, story_report};

    #[test]
    fn the_report_covers_the_asked_count_and_is_deterministic() {
        let villages = report_villages(3, 25);
        assert_eq!(
            villages
                .iter()
                .map(|village| village.characters.len())
                .sum::<usize>(),
            25
        );
        let report = story_report(3, 10, &sample_facts());
        assert_eq!(report, story_report(3, 10, &sample_facts()));
        assert!(report.contains("Story seeds"));
        assert!(!report.contains("???"));
        assert!(!report.contains('{'));
    }
}
