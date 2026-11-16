//! The quest content suite: every reference resolves, the unlock graph is acyclic and fully
//! reachable from a fresh game, co-op quests have solo paths, rewards stay inside their tier's
//! bounds, no quest is filler, and every quest binds on many seeds.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    poi::PoiJournal,
    terrain::{Terrain, TilePosition},
    worldgen::{SPAWN_TILE_X, WORLD_HEIGHT_TILES, WorldSettings},
};

use super::{
    BOARD_TEMPLATES, Objective, Prerequisite, QUESTS, QuestDef, QuestGiver, QuestId,
    binding::{bind_quest, board_offers, diggable_spot},
};

fn all() -> impl Iterator<Item = &'static QuestDef> {
    QUESTS.iter().chain(BOARD_TEMPLATES.iter())
}

/// Seeds the binding test covers (`FATHERLOAD_QUEST_SEEDS` overrides).
fn seed_count() -> u64 {
    std::env::var("FATHERLOAD_QUEST_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50)
}

#[test]
fn ids_are_unique_and_every_reference_resolves() {
    let mut ids = BTreeSet::new();
    for def in all() {
        assert!(ids.insert(def.id), "duplicate quest id {:?}", def.id);
        assert_eq!(def.id.def().map(|found| found.title), Some(def.title));
        assert!(
            !def.title.is_empty() && !def.hook.is_empty(),
            "{}: title and hook",
            def.title
        );
        assert!(
            !def.text.offer.is_empty()
                && !def.text.progress.is_empty()
                && !def.text.complete.is_empty(),
            "{}: authored text",
            def.title
        );
        assert!(!def.stages.is_empty(), "{}: no stages", def.title);
        for prerequisite in def.prerequisites {
            if let Prerequisite::Quest(quest) = prerequisite {
                assert!(
                    quest.def().is_some(),
                    "{}: unknown prerequisite {quest:?}",
                    def.title
                );
                assert!(
                    !quest.def().is_some_and(|other| other.repeatable),
                    "{}: board templates cannot gate quests",
                    def.title
                );
            }
        }
        let last = def.stages.len() - 1;
        for (stage_index, stage) in def.stages.iter().enumerate() {
            assert!(!stage.is_empty(), "{}: empty stage", def.title);
            for objective in *stage {
                for target in objective.targets() {
                    assert!(
                        usize::from(target) < def.targets.len(),
                        "{}: objective names target {target} of {}",
                        def.title,
                        def.targets.len()
                    );
                }
                if matches!(objective, Objective::Deliver { .. }) {
                    assert_eq!(
                        stage_index, last,
                        "{}: deliveries belong in the last stage",
                        def.title
                    );
                }
            }
        }
        for (index, _) in def.targets.iter().enumerate() {
            assert!(
                def.objectives().any(|objective| objective
                    .targets()
                    .contains(&u8::try_from(index).unwrap_or(0))),
                "{}: target {index} is never used",
                def.title
            );
        }
        if def.repeatable {
            assert!(
                !def.twists.is_empty(),
                "{}: board templates pick a twist",
                def.title
            );
            assert_eq!(
                def.giver,
                QuestGiver::Vale,
                "{}: board templates are Vale's",
                def.title
            );
        } else {
            assert_ne!(
                def.giver,
                QuestGiver::Vale,
                "{}: Vale only posts on the board",
                def.title
            );
        }
    }
}

/// Upgrade levels a player can buy.
const MAX_UPGRADE_LEVEL: u8 = 5;
/// Derelict logs in the collection.
const LOG_COUNT: u8 = 12;

/// Whether a prerequisite can be met, given the quests already reachable.
fn satisfiable(prerequisite: Prerequisite, reached: &BTreeSet<QuestId>) -> bool {
    match prerequisite {
        Prerequisite::Depth(row) => row > 0 && row < WORLD_HEIGHT_TILES,
        Prerequisite::Visited(_) => true,
        Prerequisite::Logs(count) => count <= LOG_COUNT,
        Prerequisite::Upgrade(_, level) => level <= MAX_UPGRADE_LEVEL,
        Prerequisite::Quest(quest) => reached.contains(&quest),
        Prerequisite::Reputation(giver, points) => {
            let earned: u32 = reached
                .iter()
                .filter_map(|quest| quest.def())
                .filter(|def| def.giver == giver && !def.repeatable)
                .map(|def| def.reward.reputation)
                .sum();
            earned >= points
        }
    }
}

/// Depth-first walk of the quest-to-quest unlock edges, panicking on a cycle.
fn visit(
    quest: QuestId,
    edges: &BTreeMap<QuestId, Vec<QuestId>>,
    stack: &mut Vec<QuestId>,
    done: &mut BTreeSet<QuestId>,
) {
    if done.contains(&quest) {
        return;
    }
    assert!(
        !stack.contains(&quest),
        "prerequisite cycle through {stack:?} -> {quest:?}"
    );
    stack.push(quest);
    for next in edges.get(&quest).into_iter().flatten() {
        visit(*next, edges, stack, done);
    }
    stack.pop();
    done.insert(quest);
}

#[test]
fn the_unlock_graph_is_acyclic_and_every_quest_is_reachable_from_a_fresh_game() {
    // Quest-to-quest edges form a DAG.
    let edges: BTreeMap<QuestId, Vec<QuestId>> = all()
        .map(|def| {
            (
                def.id,
                def.prerequisites
                    .iter()
                    .filter_map(|prerequisite| match prerequisite {
                        Prerequisite::Quest(quest) => Some(*quest),
                        _ => None,
                    })
                    .collect(),
            )
        })
        .collect();
    let mut done = BTreeSet::new();
    for quest in edges.keys() {
        visit(*quest, &edges, &mut Vec::new(), &mut done);
    }

    // From a fresh game, finishing whatever is unlocked eventually unlocks everything.
    let mut reached = BTreeSet::new();
    loop {
        let next: Vec<QuestId> = all()
            .filter(|def| !reached.contains(&def.id))
            .filter(|def| {
                def.prerequisites
                    .iter()
                    .all(|prerequisite| satisfiable(*prerequisite, &reached))
            })
            .map(|def| def.id)
            .collect();
        if next.is_empty() {
            break;
        }
        reached.extend(next);
    }
    let unreachable: Vec<&str> = all()
        .filter(|def| !reached.contains(&def.id))
        .map(|def| def.title)
        .collect();
    assert!(
        unreachable.is_empty(),
        "unreachable quests: {unreachable:?}"
    );
    let starters = QUESTS
        .iter()
        .filter(|def| def.prerequisites.is_empty())
        .count();
    assert!(starters >= 4, "a fresh game needs several starting quests");
}

#[test]
fn co_op_quests_always_have_a_solo_path() {
    for def in all().filter(|def| def.is_coop()) {
        for objective in def.objectives().filter(|objective| objective.is_coop()) {
            assert!(
                objective.has_solo_path(),
                "{}: {objective:?} has no solo variant",
                def.title
            );
        }
    }
}

#[test]
fn rewards_stay_inside_their_tier_bounds() {
    for def in all() {
        let bounds = def.tier.reward_bounds();
        let reward = def.reward;
        assert!(
            reward.credits > 0 && reward.credits <= bounds.credits,
            "{}: credits",
            def.title
        );
        assert!(
            reward.material_count() <= bounds.materials,
            "{}: materials",
            def.title
        );
        assert!(
            reward
                .fragment
                .is_none_or(|(_, count)| count <= bounds.fragments),
            "{}: fragments",
            def.title
        );
        assert!(
            reward.fitting.is_none() || bounds.fitting,
            "{}: fitting",
            def.title
        );
        assert!(reward.bombs <= bounds.bombs, "{}: bombs", def.title);
        assert!(
            reward.reputation > 0 && reward.reputation <= bounds.reputation,
            "{}: reputation",
            def.title
        );
    }
    // Deeper tiers pay more on average.
    let average = |tier| {
        let paid: Vec<u32> = QUESTS
            .iter()
            .filter(|def| def.tier == tier)
            .map(|def| def.reward.credits)
            .collect();
        paid.iter().sum::<u32>() / u32::try_from(paid.len().max(1)).unwrap_or(1)
    };
    assert!(average(super::QuestTier::Mid) > 2 * average(super::QuestTier::Shallow));
}

#[test]
fn no_quest_is_filler() {
    for def in all() {
        let interesting = def.objectives().any(|objective| !objective.is_trivial());
        assert!(
            interesting || !def.twists.is_empty() || def.is_coop(),
            "{}: only basic primitives and no twist",
            def.title
        );
    }
}

#[test]
fn the_first_batch_has_the_promised_shape() {
    let authored: Vec<&QuestDef> = QUESTS.iter().collect();
    assert!(
        (25..=35).contains(&authored.len()),
        "{} quests",
        authored.len()
    );
    let givers: BTreeSet<QuestGiver> = authored.iter().map(|def| def.giver).collect();
    assert!(givers.len() >= 4, "{givers:?}");
    let co_op = authored.iter().filter(|def| def.is_coop()).count();
    assert!(co_op >= 8, "{co_op} co-op quests");
    let tiers: BTreeSet<_> = authored.iter().map(|def| def.tier).collect();
    assert!(tiers.contains(&super::QuestTier::Shallow) && tiers.contains(&super::QuestTier::Mid));
    // Every co-op primitive appears somewhere.
    let mut kinds = Vec::new();
    for objective in authored.iter().flat_map(|def| def.objectives()) {
        let kind = std::mem::discriminant(&objective);
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    assert!(kinds.len() >= 15, "{} distinct primitives", kinds.len());
}

/// Worlds the binding test covers: [`seed_count`] default worlds, every world option on a few
/// seeds, and seeds that once failed to bind (no motherlode in a lean world, a cavern too close
/// to the edge for a row of marks).
fn binding_worlds() -> Vec<WorldSettings> {
    use crate::worldgen::{HazardIntensity, ResourceRichness, WorldSize};
    let mut worlds: Vec<WorldSettings> = (0..seed_count()).map(WorldSettings::new).collect();
    let variant = |seed, size, hazards, richness| {
        let mut settings = WorldSettings::new(seed);
        settings.size = size;
        settings.hazards = hazards;
        settings.richness = richness;
        settings
    };
    for seed in [162] {
        for size in WorldSize::ALL {
            for hazards in HazardIntensity::ALL {
                for richness in ResourceRichness::ALL {
                    worlds.push(variant(seed, size, hazards, richness));
                }
            }
        }
    }
    worlds.extend([59, 118, 128, 163, 208, 283].map(WorldSettings::new));
    worlds.push(variant(
        198,
        WorldSize::Standard,
        HazardIntensity::Brutal,
        ResourceRichness::Standard,
    ));
    worlds.push(variant(
        297,
        WorldSize::Standard,
        HazardIntensity::Standard,
        ResourceRichness::Lean,
    ));
    worlds
}

/// Prints how long accepting each quest takes on a fresh endless world (binding runs inside the
/// simulation tick that accepts it, so a slow first binding is a hitch). Run in release:
/// `cargo test --release -p fatherload_simulation quest_binding_timings -- --ignored --nocapture`.
#[test]
#[ignore = "timing helper"]
fn print_quest_binding_timings() {
    let origin = TilePosition {
        x: SPAWN_TILE_X,
        y: 4,
    };
    let journal = PoiJournal::default();
    for seed in [11_u64, 12, 13] {
        let terrain = Terrain::generate(&WorldSettings::new(seed));
        let mut worst = (std::time::Duration::ZERO, "");
        let mut total = std::time::Duration::ZERO;
        for def in all() {
            let start = std::time::Instant::now();
            let _ = bind_quest(def, &terrain, origin, &journal, 0, &BTreeSet::new());
            let elapsed = start.elapsed();
            total += elapsed;
            if elapsed > worst.0 {
                worst = (elapsed, def.title);
            }
        }
        println!(
            "seed {seed}: every quest bound in {total:?}; slowest first binding {:?} ({})",
            worst.0, worst.1
        );
    }
}

#[test]
fn every_quest_binds_on_many_seeds() {
    let origin = TilePosition {
        x: SPAWN_TILE_X,
        y: 4,
    };
    let journal = PoiJournal::default();
    for settings in binding_worlds() {
        let seed = settings.seed;
        let terrain = Terrain::generate(&settings);
        let mut salts = vec![0_u64];
        salts.extend(
            (0..4)
                .flat_map(|day| board_offers(seed, day))
                .map(|offer| offer.salt),
        );
        for def in all() {
            for salt in if def.repeatable {
                &salts[..]
            } else {
                &salts[..1]
            } {
                let Some((targets, objectives)) =
                    bind_quest(def, &terrain, origin, &journal, *salt, &BTreeSet::new())
                else {
                    panic!(
                        "{} does not bind on seed {seed} {:?}/{:?}/{:?} (salt {salt})",
                        def.title, settings.size, settings.hazards, settings.richness
                    );
                };
                assert_eq!(targets.len(), def.targets.len());
                assert_eq!(objectives.len(), def.objectives().count());
                for (objective, progress) in def.objectives().zip(&objectives) {
                    if matches!(
                        objective,
                        Objective::Simultaneous { .. } | Objective::Detonate { .. }
                    ) {
                        for spot in &progress.spots {
                            assert!(
                                diggable_spot(&terrain, *spot),
                                "{}: spot {spot:?} on seed {seed} cannot be reached",
                                def.title
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Features in `terrain` a target spec bound from `origin` could pick: every one of a bounded
/// world, or those in the nearest window a streamed world's binding searches first.
fn candidate_count(spec: super::TargetSpec, terrain: &Terrain, origin: TilePosition) -> usize {
    use crate::worldgen::{FeatureKind, TileRect};
    let (first, last) = spec.bands();
    let in_bands = |feature: &crate::worldgen::WorldFeature| {
        (usize::from(first)..=usize::from(last)).contains(&feature.band.index())
    };
    let (min_x, max_x) = if terrain.is_streamed() {
        let columns = super::binding::STREAMED_BIND_COLUMNS[0];
        (origin.x - columns, origin.x + columns)
    } else {
        (terrain.min_x(), terrain.max_x())
    };
    let rect = TileRect {
        min: TilePosition { x: min_x, y: 0 },
        max: TilePosition {
            x: max_x,
            y: terrain.height() - 1,
        },
    };
    match spec {
        super::TargetSpec::Poi { kind, unopened, .. } => terrain
            .features_in(&rect, Some(&[FeatureKind::PointOfInterest]))
            .iter()
            .filter(|feature| feature.poi_kind() == Some(kind) && in_bands(feature))
            .filter(|feature| !unopened || super::binding::poi_unopened(terrain, feature))
            .count(),
        super::TargetSpec::Feature { kind, .. } => terrain
            .features_in(&rect, Some(&[kind]))
            .iter()
            .filter(|feature| in_bands(feature))
            .count(),
        // Story places always bind (each has a fallback).
        super::TargetSpec::Story(_) => 1,
    }
}

#[test]
fn binding_is_deterministic_and_prefers_unclaimed_places() {
    let terrain = Terrain::generate(&WorldSettings::new(7));
    let origin = TilePosition {
        x: SPAWN_TILE_X,
        y: 4,
    };
    let journal = PoiJournal::default();
    let mut moved = 0;
    for def in all() {
        let first = bind_quest(def, &terrain, origin, &journal, 0, &BTreeSet::new());
        let again = bind_quest(def, &terrain, origin, &journal, 0, &BTreeSet::new());
        assert_eq!(first, again, "{}", def.title);
        let Some((targets, _)) = first else {
            continue;
        };
        let taken: BTreeSet<_> = targets.iter().filter_map(|target| target.feature).collect();
        if taken.is_empty() {
            continue;
        }
        if let Some((others, _)) = bind_quest(def, &terrain, origin, &journal, 0, &taken) {
            // With alternatives available, a second binding moves to different places.
            let other_features: BTreeSet<_> =
                others.iter().filter_map(|target| target.feature).collect();
            let alternatives = def
                .targets
                .iter()
                .all(|spec| candidate_count(*spec, &terrain, origin) >= 2);
            if def.targets.len() == 1 && alternatives {
                assert!(other_features.is_disjoint(&taken), "{}", def.title);
                moved += 1;
            }
        }
    }
    // The check above must actually run (an empty candidate count would skip it silently).
    assert!(moved > 0, "no quest had alternatives to move to");
}

#[test]
fn the_board_posts_distinct_templates_each_day() {
    for day in 0..20 {
        let offers = board_offers(42, day);
        assert_eq!(offers, board_offers(42, day));
        assert_eq!(offers.len(), usize::from(super::binding::BOARD_SLOTS));
        let templates: BTreeSet<QuestId> = offers.iter().map(|offer| offer.quest).collect();
        assert_eq!(templates.len(), offers.len());
        for offer in &offers {
            let def = offer.quest.def().expect("template");
            assert!(usize::from(offer.variant) < def.twists.len());
        }
    }
    let days: BTreeSet<Vec<QuestId>> = (0..20)
        .map(|day| {
            board_offers(42, day)
                .iter()
                .map(|offer| offer.quest)
                .collect()
        })
        .collect();
    assert!(days.len() > 3, "the board rotates");
}

/// Caves bind as quest targets (WP-CAVES acceptance): a quest naming
/// [`crate::worldgen::FeatureKind::Cave`] binds to a cave ecosystem's open centre on V6 worlds,
/// and to its large-cavern stand-in on worlds without caves.
#[test]
fn cave_targets_bind_on_cave_worlds_and_fall_back_elsewhere() {
    use crate::worldgen::{FeatureKind, GeneratorVersion};
    const CAVE_SURVEY: QuestDef = QuestDef {
        id: QuestId(9_999),
        title: "Cave Survey",
        giver: QuestGiver::Vale,
        tier: super::QuestTier::Shallow,
        hook: "test",
        prerequisites: &[],
        targets: &[super::TargetSpec::Feature {
            kind: FeatureKind::Cave,
            bands: (1, 4),
        }],
        stages: &[&[Objective::Reach { target: 0 }]],
        twists: &[],
        reward: super::QuestReward::credits(100, 1),
        text: super::QuestText {
            offer: "o",
            progress: "p",
            complete: "c",
        },
        repeatable: false,
    };
    let origin = TilePosition {
        x: SPAWN_TILE_X,
        y: 4,
    };
    for seed in 0..6 {
        for (generator, wanted) in [
            (GeneratorVersion::V6, FeatureKind::Cave),
            (GeneratorVersion::V5, FeatureKind::LargeCavern),
            (GeneratorVersion::V4, FeatureKind::LargeCavern),
        ] {
            let terrain = Terrain::generate(&WorldSettings {
                generator,
                ..WorldSettings::new(seed)
            });
            let (targets, _) = bind_quest(
                &CAVE_SURVEY,
                &terrain,
                origin,
                &PoiJournal::default(),
                0,
                &BTreeSet::new(),
            )
            .unwrap_or_else(|| panic!("seed {seed} {generator:?}: cave quest did not bind"));
            let feature = targets[0]
                .feature
                .and_then(|id| terrain.feature(id))
                .expect("bound feature");
            assert_eq!(feature.kind, wanted, "seed {seed} {generator:?}");
            if wanted == FeatureKind::Cave {
                assert!(
                    terrain
                        .cave_at(feature.center)
                        .is_some_and(|sample| sample.interior),
                    "seed {seed}: a cave target's centre lies inside the cave"
                );
            }
        }
    }
}
