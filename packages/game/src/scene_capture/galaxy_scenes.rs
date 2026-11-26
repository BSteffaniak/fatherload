//! The galaxy story's scenes (Wave 13 story; a Deep Descent world after the ending, the rig
//! fitted for space): the road's journal, a chapter's card, a waymark on a world, a hold
//! against raiders, the Warden at the Harbour's gate, the Harbour itself, the choice, and the
//! ending (its lead, the credits, the epilogue, the Harbour awake after the bell).

#![allow(
    clippy::missing_const_for_fn,
    clippy::cast_precision_loss,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    galaxy_story::{
        CHAPTER_COUNT, FinaleChoice, GalaxyStory, LeadRecord, discovery::leads_of, finale, plan,
    },
    hostiles::{EncounterKind, EncounterState, Faction, ShipClass, ShipGear, ShipMode},
    space::{SpaceSector, SystemRef},
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage};
use crate::shell::{ModalScreen, galaxy::event_words};

/// Seed of every galaxy scene world (the galaxy tour's seed: its road is well walked).
const SCENE_SEED: u64 = 0x00C0_FFEE;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// A world after the ending with `player`'s rig fitted for space and the story at `chapter`'s
/// goal `step` (earlier chapters done: their logs found, a few leads picked up on the way).
fn road(world: &mut WorldState, player: PlayerId, chapter: u8, step: u8) {
    *world.terrain_mut() = Terrain::generate(&settings());
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(fatherload_simulation::SimulationTick::new(900 * hz));
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(player);
    world.set_heart(heart);
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.arms = ShipGear::MAXED;
        rig.hull = rig.max_hull() * 0.9;
        rig.fuel = rig.fuel_capacity * 0.8;
        rig.credits = 48_600;
        rig.attitude = RigAttitude::LEVEL;
    }
    let anchor = plan::anchor_of(SCENE_SEED, world.star_charts());
    let road = plan::plan_for(SCENE_SEED, anchor);
    let logs = fatherload_simulation::galaxy_story::CHAPTERS
        .iter()
        .take(usize::from(chapter))
        .flat_map(|def| def.logs.iter().copied())
        .collect();
    // The stops passed so far, visited; the leads of the systems round home, picked up (the
    // first few done).
    let mut visited: std::collections::BTreeSet<SystemRef> = road
        .stops
        .iter()
        .take(usize::from(chapter) + 1)
        .map(|stop| stop.system)
        .collect();
    let mut leads = Vec::new();
    'sectors: for y in -2..=2 {
        for x in -2..=2 {
            let coords = SpaceSector::new(x, y);
            for index in 0..3 {
                let system = SystemRef {
                    sector: coords,
                    system: index,
                };
                for lead in leads_of(SCENE_SEED, system) {
                    visited.insert(system);
                    let done = leads.len() % 3 == 0;
                    leads.push(LeadRecord {
                        system,
                        index: lead.index,
                        step: if done {
                            u8::try_from(lead.steps.len()).unwrap_or(1)
                        } else {
                            0
                        },
                        done,
                        found_millis: 0,
                        ambushed: None,
                    });
                    if leads.len() >= 9 {
                        break 'sectors;
                    }
                }
            }
        }
    }
    let story = GalaxyStory {
        begun: true,
        anchor: Some(anchor),
        chapter,
        step,
        chapter_millis: vec![0; usize::from(chapter)],
        logs,
        fragments: [1, 4, 7].into_iter().collect(),
        leads,
        visited,
        remarks: 3,
        next_check_millis: u64::MAX,
        ..GalaxyStory::default()
    };
    world.set_galaxy_story(story);
}

/// Lets the story's checks run again (after the staging) so the scene's world is live.
fn wake(world: &mut WorldState) {
    let mut story = world.galaxy_story().clone();
    story.next_check_millis = 0;
    world.set_galaxy_story(story);
}

/// The road's journal mid-way: the chapter under way with its goals ticked off, the logs, the
/// discovery log.
fn journal(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, 6, 1);
        let _ = world.stage_story_warp(seats[0].player);
    });
    stage.settle(6);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::GalaxyJournal);
    // The chapter under way (its page in the footer).
    seat.selected_menu_item = 2;
    seat.message.clear();
    seat.story_moment = None;
    stage.settle(3);
}

/// The discovery log in the journal: an open lead selected, its next step in the footer.
fn discoveries(stage: &mut SceneStage<'_>) {
    journal(stage);
    let rows = crate::shell::galaxy::galaxy_rows(stage.world());
    let lead = rows
        .iter()
        .position(|row| matches!(row, crate::shell::galaxy::GalaxyRow::Lead(_)))
        .unwrap_or(0);
    let seat = stage.seat(0);
    seat.selected_menu_item = lead;
    stage.settle(2);
}

/// A chapter's card in flight beside its stop: the Silent Foundry begins.
fn chapter_card(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, 2, 1);
        let _ = world.stage_story_warp(seats[0].player);
    });
    stage.settle(20);
    let words = event_words(
        stage.world(),
        fatherload_simulation::galaxy_story::GalaxyEvent::ChapterBegun,
        2,
    );
    if let Some((kind, title, text)) = words.card {
        stage.shell().story_moment(0, kind, title, text);
    }
    stage.settle(30);
}

/// The first waymark on its world's ground: the pylon lit, the goal on the HUD.
fn waymark(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, 0, 2);
        let _ = world.stage_story_warp(seats[0].player);
    });
    stage.settle(40);
    // A few columns off the pylon, looking at it.
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let Some((body, site, _)) = world.galaxy_relic() else {
            return;
        };
        let Some(params) = fatherload_simulation::place::planet_params_for(SCENE_SEED, body) else {
            return;
        };
        let ground = fatherload_simulation::planet::PlanetGround::new(params);
        let column = i32::try_from(site.column).unwrap_or(0) - 9;
        let surface = fatherload_simulation::planet::BodyGround::surface_row(&ground, column);
        let at = fatherload_simulation::world::strip_position(
            f64::from(column) + 0.5,
            f64::from(surface - 1),
        );
        let _ = world.move_to_place(
            player,
            fatherload_simulation::place::Place::OnBody(
                fatherload_simulation::place::BodyId::Space(body),
            ),
            at,
        );
    });
    stage.settle(20);
    stage.seat(0).message.clear();
    stage.settle(2);
}

/// The lamp-key held while the Bloom drops in: the raid round the waymark, the hold's bar.
fn hold(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, 7, 3);
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        let _ = world.stage_story_warp(seats[0].player);
    });
    stage.settle(30);
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let _ = world.stage_raid(player, Faction::Swarm);
        let mut story = world.galaxy_story().clone();
        story.raid_landed = true;
        story.held_millis = 21_000;
        world.set_galaxy_story(story);
    });
    stage.settle(70);
    stage.seat(0).message.clear();
    stage.settle(2);
}

/// The Warden at the Harbour's gate, a lance's line telegraphed.
fn warden(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, CHAPTER_COUNT - 1, 0);
        world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        let _ = world.stage_story_warp(seats[0].player);
        let _ = world.stage_encounter(
            seats[0].player,
            Faction::Machine,
            EncounterKind::Boss,
            &[ShipClass::Warden],
            8,
        );
        let mut story = world.galaxy_story().clone();
        story.warden_on = Some(seats[0].player);
        world.set_galaxy_story(story);
    });
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let Some(rig) = world.rig_flight(player).map(|flight| flight.pos) else {
            return;
        };
        let mut hostiles = world.hostiles().clone();
        for ship in &mut hostiles.ships {
            ship.pos = rig.offset(40.0, -560.0);
            let (dx, dy) = ship.pos.delta(rig);
            ship.angle = fatherload_simulation::hostiles::combat::bearing(dx, dy);
            ship.mode = ShipMode::Attacking;
            ship.next_attack = 0;
            ship.hull = ship.class.stats().hull * 0.64;
            ship.shield = 0.0;
            ship.vx = 0.0;
            ship.vy = 0.0;
        }
        for encounter in &mut hostiles.encounters {
            encounter.state = EncounterState::Fighting;
        }
        world.set_hostiles(hostiles);
        wake(world);
    });
    stage.settle(64);
    stage.seat(0).message.clear();
    stage.settle(2);
}

/// The Harbour from its approach: ten world-ships moored round the red star, the gate's lamps.
fn harbour(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, CHAPTER_COUNT - 2, 1);
        let Some(harbour) = world.galaxy_plan().map(|road| road.harbour) else {
            return;
        };
        // Out on the approach, looking in across the ring.
        let (dx, dy) = (
            harbour.gate.x - harbour.star.x,
            harbour.gate.y - harbour.star.y,
        );
        let out = harbour.star.offset(dx * 9 / 5, dy * 9 / 5);
        let _ = world.stage_story_warp(seats[0].player);
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.pos = out.pos();
        }
    });
    stage.settle(10);
    stage.seat(0).message.clear();
    stage.settle(2);
}

/// The choice at the Harbour's heart: the bell and the lamp.
fn choice(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, CHAPTER_COUNT - 1, 1);
        let mut story = world.galaxy_story().clone();
        story.warden_calmed = true;
        world.set_galaxy_story(story);
        let _ = world.stage_story_warp(seats[0].player);
    });
    stage.settle(4);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::GalaxyChoice);
    seat.selected_menu_item = 0;
    seat.message.clear();
    stage.settle(3);
}

/// The ending `elapsed` ms after the bell was rung.
fn ending(stage: &mut SceneStage<'_>, choice: FinaleChoice, elapsed: u64) {
    stage.play(1, |world, seats| {
        road(world, seats[0].player, CHAPTER_COUNT, 0);
        let mut story = world.galaxy_story().clone();
        story.warden_calmed = true;
        story.choice = Some(choice);
        story.finale_millis = Some(world.world_millis().saturating_sub(elapsed));
        world.set_galaxy_story(story);
        if let Some(harbour) = world.galaxy_plan().map(|road| road.harbour)
            && let Some(rig) = world.player_mut(seats[0].player)
        {
            rig.flight.place = fatherload_simulation::flight::FlightPlace::Space;
            rig.flight.pos = harbour.gate.offset(0, 9_000).pos();
        }
    });
    stage.settle(3);
    stage.seat(0).message.clear();
    stage.settle(1);
}

/// The Harbour awake after the bell, the ending closed: its windows lit along the ring.
fn harbour_awake(stage: &mut SceneStage<'_>) {
    ending(
        stage,
        FinaleChoice::RingTheBell,
        finale::CREDITS_END_MILLIS * 3,
    );
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        let Some(harbour) = world.galaxy_plan().map(|road| road.harbour) else {
            return;
        };
        let (dx, dy) = (
            harbour.gate.x - harbour.star.x,
            harbour.gate.y - harbour.star.y,
        );
        let out = harbour.star.offset(dx * 9 / 5, dy * 9 / 5);
        if let Some(rig) = world.player_mut(player) {
            rig.flight.pos = out.pos();
        }
    });
    stage.settle(6);
    stage.seat(0).message.clear();
    stage.settle(1);
}

pub const GALAXY_SCENES: &[Scene] = &[
    Scene {
        name: "galaxy_journal",
        summary: "The Builders' Road journal mid-way: the chapter under way with its goals ticked off, logs, discoveries",
        setup: journal,
    },
    Scene {
        name: "galaxy_discoveries",
        summary: "The discovery log: a procedural lead selected, its next step and pay in the footer",
        setup: discoveries,
    },
    Scene {
        name: "galaxy_chapter_card",
        summary: "A chapter of the road begins beside its stop: the Silent Foundry's card, the goal line",
        setup: chapter_card,
    },
    Scene {
        name: "galaxy_waymark",
        summary: "The first builders' waymark on its world: the pylon's lamp, the goal on the HUD",
        setup: waymark,
    },
    Scene {
        name: "galaxy_hold",
        summary: "The lamp-key held while the Bloom drops in round the waymark: the hold's bar",
        setup: hold,
    },
    Scene {
        name: "galaxy_warden",
        summary: "The Warden at the Harbour's gate, its lance telegraphed, its bar on the HUD",
        setup: warden,
    },
    Scene {
        name: "galaxy_harbour",
        summary: "The Harbour from its approach: ten world-ships moored round a red star, the gate",
        setup: harbour,
    },
    Scene {
        name: "galaxy_choice",
        summary: "The Harbour's heart: ring the Homecoming Bell, or keep the lamp",
        setup: choice,
    },
    Scene {
        name: "galaxy_finale_lead",
        summary: "The finale: the bell rings round the ring of ships (the ending's first card)",
        setup: |stage| ending(stage, FinaleChoice::RingTheBell, 4_000),
    },
    Scene {
        name: "galaxy_credits",
        summary: "The galaxy story's credits rolling beside the road's lit waymarks",
        setup: |stage| ending(stage, FinaleChoice::RingTheBell, 34_000),
    },
    Scene {
        name: "galaxy_epilogue_lamp",
        summary: "The lamp keepers' epilogue: Wren Varga sees a new light at the top of the cable",
        setup: |stage| {
            ending(
                stage,
                FinaleChoice::KeepTheLamp,
                finale::CREDITS_END_MILLIS + 19_000,
            );
        },
    },
    Scene {
        name: "galaxy_harbour_awake",
        summary: "After the bell: the Harbour's world-ships awake, windows lit along the ring",
        setup: harbour_awake,
    },
];
