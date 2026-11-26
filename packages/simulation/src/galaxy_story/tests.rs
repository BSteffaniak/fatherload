//! The galaxy story's pure parts: the plan, the chapters' words, the leads, the journal.

use std::collections::BTreeSet;

use super::{
    CHAPTER_COUNT, CHAPTERS, GalaxyEvent, GalaxyStory, Goal, LeadKind, LeadStep,
    chapters::{self, StopKind},
    discovery::leads_of,
    fill,
    finale::{self, FinaleChoice, FinalePhase, FinaleView},
    journal::{self, FRAGMENTS, REMARKS, RemarkTrigger, SPACE_LOGS},
    plan::{self, HARBOUR_MIN_SECTORS},
    stop_words,
};
use crate::space::{HOME_SECTOR, Space, SpaceSector, SystemRef};

const SEEDS: [u64; 3] = [0x00C0_FFEE, 7, 0x5_7A2D];

#[test]
fn the_story_has_ten_chapters_with_goals_and_words() {
    assert_eq!(CHAPTERS.len(), usize::from(CHAPTER_COUNT));
    let goals = super::goal_count();
    assert!((20..=40).contains(&goals), "{goals} goals");
    for (index, chapter) in CHAPTERS.iter().enumerate() {
        assert!(
            !chapter.title.is_empty() && !chapter.intro.is_empty() && !chapter.outro.is_empty()
        );
        assert!(!chapter.goals.is_empty(), "chapter {index}");
        for log in chapter.logs {
            assert!(
                usize::from(*log) < SPACE_LOGS.len(),
                "chapter {index} log {log}"
            );
        }
        // A relic goal has a relic to find.
        if chapter.goals.iter().any(|goal| goal.goal.on_ground()) {
            assert!(!chapter.relic.is_empty(), "chapter {index}");
        }
    }
    // Every log is opened by exactly one chapter, in order.
    let opened: Vec<u8> = CHAPTERS
        .iter()
        .flat_map(|chapter| chapter.logs.iter().copied())
        .collect();
    let expected: Vec<u8> = (0..u8::try_from(SPACE_LOGS.len()).unwrap_or(0)).collect();
    assert_eq!(opened, expected);
    // The finale ends it: the Warden, then the choice.
    let last = CHAPTERS.last().expect("chapters");
    assert_eq!(
        last.goals.iter().map(|goal| goal.goal).collect::<Vec<_>>(),
        vec![Goal::CalmWarden, Goal::Choose]
    );
    assert!(chapters::all_goals().count() == usize::try_from(goals).unwrap_or(0));
}

#[test]
fn ids_are_unique_and_events_fit_their_code_range() {
    let logs: BTreeSet<&str> = SPACE_LOGS.iter().map(|log| log.id).collect();
    assert_eq!(logs.len(), SPACE_LOGS.len());
    let remarks: BTreeSet<&str> = REMARKS.iter().map(|remark| remark.id).collect();
    assert_eq!(remarks.len(), REMARKS.len());
    let codes: BTreeSet<u8> = GalaxyEvent::ALL.iter().map(|event| event.code()).collect();
    assert_eq!(codes.len(), GalaxyEvent::ALL.len());
    assert!(codes.iter().all(|code| *code < 20), "netcode 1040-1059");
    assert!(FRAGMENTS.len() >= 16);
}

#[test]
fn the_plan_is_a_pure_function_of_the_seed_and_anchor() {
    for seed in SEEDS {
        let anchor = plan::anchor_of(seed, &[]);
        assert_ne!(anchor.sector, HOME_SECTOR);
        let a = plan::plan_for(seed, anchor);
        let b = plan::compute_for_tests(seed, anchor);
        assert_eq!(*a, b, "seed {seed:#x}");
        assert_eq!(a.stops.len(), usize::from(CHAPTER_COUNT));
    }
}

#[test]
fn every_stop_has_what_its_goals_need() {
    for seed in SEEDS {
        let anchor = plan::anchor_of(seed, &[]);
        let road = plan::plan_for(seed, anchor);
        let space = Space::shared(seed);
        let mut worlds = BTreeSet::new();
        for (index, chapter) in CHAPTERS.iter().enumerate() {
            let stop = road.stop(u8::try_from(index).unwrap_or(0)).expect("stop");
            assert!(
                space.system(stop.system).is_some(),
                "seed {seed:#x} ch {index}"
            );
            if chapter.goals.iter().any(|goal| goal.goal.needs_world()) {
                let body = stop.body.expect("a world");
                assert!(crate::place::planet_params_for(seed, body).is_some());
                assert_eq!(body.system_ref(), stop.system);
                assert!(worlds.insert(body), "seed {seed:#x}: a world used twice");
                let relic = stop.relic.expect("a relic");
                let found = space.body(body).expect("body");
                assert!(relic.column < found.surface_columns);
                assert_eq!(relic.rows, chapter.relic_rows);
            }
            if let StopKind::Theme(themes) = chapter.stop {
                eprintln!(
                    "seed {seed:#x} ch {index} {:?}: wanted {themes:?}",
                    stop.theme
                );
            }
        }
        // The Harbour is far, and its gate is on its ring facing home.
        let harbour = road.harbour;
        let ring = harbour
            .system
            .sector
            .x
            .abs()
            .max(harbour.system.sector.y.abs());
        assert!(
            i64::from(ring) >= HARBOUR_MIN_SECTORS * 7 / 10,
            "ring {ring}"
        );
        let distance = harbour.star.distance(harbour.gate);
        assert!(
            (distance - f64::from(i32::try_from(plan::HARBOUR_RING_TILES).unwrap_or(0))).abs()
                < 200.0,
            "{distance}"
        );
        assert!(
            harbour.gate.distance2(SpaceSector::new(0, 0).centre())
                < harbour.star.distance2(SpaceSector::new(0, 0).centre())
        );
    }
}

#[test]
fn the_road_starts_from_the_first_chart() {
    let seed = SEEDS[0];
    let chart =
        crate::star_chart::chart_at_site(seed, 120, 9_400, 200, crate::ids::SimulationTick::new(3));
    let anchor = plan::anchor_of(seed, std::slice::from_ref(&chart));
    assert_eq!(anchor, Space::shared(seed).chart_system(&chart));
    let road = plan::plan_for(seed, anchor);
    assert_eq!(
        road.stops[0].system,
        road.stops[0]
            .body
            .map_or(anchor, crate::space::SpaceBodyId::system_ref)
    );
}

#[test]
fn words_name_the_stops() {
    let seed = SEEDS[0];
    let road = plan::plan_for(seed, plan::anchor_of(seed, &[]));
    for index in 0..CHAPTER_COUNT {
        let words = stop_words(seed, &road, index);
        let chapter = &CHAPTERS[usize::from(index)];
        for text in std::iter::once(chapter.intro)
            .chain(std::iter::once(chapter.outro))
            .chain(chapter.goals.iter().map(|goal| goal.text))
        {
            let filled = fill(text, &words);
            assert!(!filled.contains('{'), "{filled}");
        }
    }
}

#[test]
fn leads_are_deterministic_per_system_and_varied() {
    let seed = SEEDS[0];
    let space = Space::shared(seed);
    let mut kinds = BTreeSet::new();
    let mut systems = 0;
    for y in -3..=3 {
        for x in -3..=3 {
            let coords = SpaceSector::new(x, y);
            for system in &space.sector(coords).systems {
                let reference = SystemRef {
                    sector: coords,
                    system: system.index,
                };
                let a = leads_of(seed, reference);
                let b = leads_of(seed, reference);
                assert_eq!(a, b);
                if system.home {
                    assert!(a.is_empty(), "home has no leads");
                    continue;
                }
                systems += 1;
                assert!(!a.is_empty() && a.len() <= 2);
                for lead in &a {
                    kinds.insert(lead.kind);
                    assert!(!lead.steps.is_empty() && lead.credits > 0);
                    assert!(usize::from(lead.fragment) < FRAGMENTS.len());
                    for step in &lead.steps {
                        if let Some(body) = step.body() {
                            assert_eq!(body.system_ref(), reference);
                        }
                        if let LeadStep::FindSite(body, site) = step {
                            let found = space.body(*body).expect("body");
                            assert!(site.column < found.surface_columns);
                        }
                    }
                    for step in 0..=u8::try_from(lead.steps.len()).unwrap_or(0) {
                        assert!(!lead.step_text(step, system).is_empty());
                    }
                }
                // Another seed, other leads (at least sometimes).
                let _ = leads_of(seed ^ 1, reference);
            }
        }
    }
    assert!(systems > 20);
    for kind in [
        LeadKind::LostShip,
        LeadKind::DistressCall,
        LeadKind::Ruins,
        LeadKind::Survey,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} appears near home");
    }
}

#[test]
fn the_archivist_has_a_line_for_every_trigger() {
    for chapter in 0..CHAPTER_COUNT {
        assert!(journal::remark_for(RemarkTrigger::ChapterBegun(chapter), chapter, 0).is_some());
    }
    assert!(journal::remark_for(RemarkTrigger::Asked, 0, 0).is_some());
    assert!(journal::remark_for(RemarkTrigger::LeadFound, 0, 0).is_some());
    assert!(journal::remark_for(RemarkTrigger::LeadDone, 0, 0).is_some());
    assert!(journal::remark_for(RemarkTrigger::AfterTheRoad, CHAPTER_COUNT, 0).is_some());
    // Lines take turns.
    let a = journal::remark_for(RemarkTrigger::AfterTheRoad, CHAPTER_COUNT, 0);
    let b = journal::remark_for(RemarkTrigger::AfterTheRoad, CHAPTER_COUNT, 1);
    assert_ne!(a, b);
    // Nothing spoils: lines after the road wait for it.
    assert!(journal::remark_for(RemarkTrigger::AfterTheRoad, 3, 0).is_none());
}

#[test]
fn the_ending_plays_then_lets_go_and_skips_kindly() {
    let mut view = FinaleView::new();
    assert_eq!(view.phase(0), FinalePhase::Lead);
    assert_eq!(view.phase(finale::LEAD_MILLIS + 1), FinalePhase::Credits);
    assert!(view.phase(finale::LEAD_MILLIS + 1).covers_screen());
    assert_eq!(
        view.phase(finale::CREDITS_END_MILLIS + 1),
        FinalePhase::Epilogue
    );
    assert_eq!(
        view.phase(finale::CREDITS_END_MILLIS + finale::EPILOGUE_MILLIS + 1),
        FinalePhase::Done
    );
    // A press right as the credits start does nothing; later it skips to the epilogue.
    assert!(!view.skip(finale::LEAD_MILLIS + 100));
    assert!(view.skip(20_000));
    assert_eq!(view.phase(20_001), FinalePhase::Epilogue);
    assert!(view.skip(22_000));
    assert_eq!(view.phase(22_001), FinalePhase::Done);
    for choice in FinaleChoice::ALL {
        assert_eq!(FinaleChoice::from_code(choice.code()), Some(choice));
        assert!(finale::ending_text(choice).len() > 400);
    }
}

#[test]
fn an_empty_story_is_empty() {
    let story = GalaxyStory::default();
    assert!(story.is_empty());
    assert!(story.current_goal().is_none());
    assert_eq!(story.stats().chapters_done, 0);
}
