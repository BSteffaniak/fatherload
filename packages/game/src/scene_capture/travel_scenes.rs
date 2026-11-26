//! The destination index's travel (Wave 14 W14-B, test mode): where a rig lands when it
//! travels by the index ("Descent V", a boss arena's door and its floor, a village, the next
//! planet, a station, a wormhole) and split-screen seats sent to two different places. The
//! scenes go through `WorldState::stage_travel`, the same resolve and safe arrival as the
//! `TravelTo` action.

use fatherload_simulation::{
    PlayerId, WorldState,
    descent::Descent,
    destination::{ArrivalOptions, DestinationKind, Selector, TravelRequest},
    space::Theme,
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage};

/// Seed of every travel scene world.
const SCENE_SEED: u64 = 0x00C0_FFEE;

/// A Deep Descent test-mode world with space open and `players` fitted for it.
fn god_world(world: &mut WorldState, players: &[PlayerId]) {
    *world.terrain_mut() = Terrain::generate(&WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    });
    world.enable_test_mode();
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    for player in players {
        heart.star_drive.insert(*player);
    }
    world.set_heart(heart);
    for player in players {
        if let Some(rig) = world.player_mut(*player) {
            fatherload_simulation::economy::max_out_upgrades(rig);
            rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        }
    }
}

/// Stages a one-seat world and sends its rig by `request`.
fn travel_scene(stage: &mut SceneStage<'_>, request: TravelRequest) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        god_world(world, &[player]);
        let arrived = world.stage_travel(player, &request);
        assert!(arrived.is_some(), "the scene's travel {request:?} arrives");
    });
    stage.settle(45);
}

pub(super) const TRAVEL_SCENES: &[Scene] = &[
    Scene {
        name: "travel_descent_v",
        summary: "Travelled to Descent V (the Hollow World): the rig in its freshly carved \
                  arrival pocket, level, nothing hostile near",
        setup: |stage| travel_scene(stage, TravelRequest::descent(Descent::HollowWorld)),
    },
    Scene {
        name: "travel_depth_4200",
        summary: "Travelled to exactly 4,200 m under the spawn: the arrival pocket and its floor",
        setup: |stage| travel_scene(stage, TravelRequest::depth_metres(4_200)),
    },
    Scene {
        name: "travel_boss_door",
        summary: "Travelled to the nearest boss arena: the pocket eight rows over its roof",
        setup: |stage| {
            travel_scene(stage, TravelRequest::nearest(DestinationKind::BossArena));
        },
    },
    Scene {
        name: "travel_boss_inside",
        summary: "Travelled into the nearest boss arena (\"in the arena\"): standing on its floor",
        setup: |stage| {
            travel_scene(
                stage,
                TravelRequest::nearest(DestinationKind::BossArena).arriving(ArrivalOptions {
                    in_water: false,
                    in_arena: true,
                }),
            );
        },
    },
    Scene {
        name: "travel_village",
        summary: "Travelled to the nearest village: set down on its ground clear of the houses",
        setup: |stage| travel_scene(stage, TravelRequest::nearest(DestinationKind::Village)),
    },
    Scene {
        name: "travel_cave",
        summary: "Travelled to the nearest cave: its arrival pocket with the cave round it",
        setup: |stage| travel_scene(stage, TravelRequest::nearest(DestinationKind::Cave)),
    },
    Scene {
        name: "travel_planet_ice",
        summary: "Travelled to the nearest ice world: landed at its arrival point, level",
        setup: |stage| {
            travel_scene(
                stage,
                TravelRequest::nearest(DestinationKind::Planet).filtered(
                    fatherload_simulation::destination::DestinationFilter::Theme(Theme::Ice),
                ),
            );
        },
    },
    Scene {
        name: "travel_station",
        summary: "Travelled to the nearest station: stopped in free flight a safe distance off",
        setup: |stage| travel_scene(stage, TravelRequest::nearest(DestinationKind::Station)),
    },
    Scene {
        name: "travel_split_two_places",
        summary: "Split-screen: seat 1 travelled to Descent V, seat 2 to the next planet",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let (one, two) = (seats[0].player, seats[1].player);
                god_world(world, &[one, two]);
                assert!(
                    world
                        .stage_travel(one, &TravelRequest::descent(Descent::HollowWorld))
                        .is_some()
                );
                assert!(
                    world
                        .stage_travel(
                            two,
                            &TravelRequest::new(DestinationKind::Planet, Selector::Next),
                        )
                        .is_some()
                );
            });
            stage.settle(45);
        },
    },
];
