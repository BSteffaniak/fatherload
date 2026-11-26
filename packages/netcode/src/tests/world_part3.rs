//! Part III online, end to end (Wave 13 integration): a client walks the galaxy story's first
//! chapter while the other client and the host are elsewhere, on the Wave 13 worlds; then the
//! crew spreads over all six new kinds of world at once. Every client matches the host, the
//! story's road is the crew's one road on every peer, and nothing grows without bound.

use std::time::Duration;

use fatherload_simulation::{
    ControlState, PlayerCommand, TestModeAction, place::Place, space::Theme,
};

use super::support::{assert_bounded, assert_converged};
use super::world_soak::{body_of_theme, harness, seat_host, wait_in_game, warp};
use crate::ProfileId;
use crate::world::to_sim_player;

const ALEX: ProfileId = ProfileId(0xa1e8_0000_0000_1300);
const SAM: ProfileId = ProfileId(0x5a3a_0000_0000_1300);

#[test]
fn world_part3_story_online_with_the_crew_spread_over_the_new_worlds() {
    let mut harness = harness();
    let host_player = seat_host(&mut harness, None);
    let alex = harness.add_client_as(None, Some(ALEX), "Alex");
    let sam = harness.add_client_as(None, Some(SAM), "Sam");
    wait_in_game(&mut harness);
    let alex_player = harness.clients[alex].session.player().expect("alex");
    // The road is open for the crew.
    harness.run_for(Duration::from_secs(1));
    assert!(harness.world().galaxy_story().begun, "the road opened");
    // Sam and the host go elsewhere: a fungal world and a gas giant.
    let fungal = body_of_theme(Theme::Fungal);
    let giant = body_of_theme(Theme::GasGiant);
    harness.clients[sam].queued_actions.push(warp(fungal));
    harness
        .host
        .submit_local_input(host_player, ControlState::default(), vec![warp(giant)]);
    harness.run_for(Duration::from_secs(3));
    // Alex walks the first chapter: to the system, a scan, down onto the world's waymark.
    let story_warp = PlayerCommand::TestMode(TestModeAction::WarpToStory);
    let chapter_before = harness.world().galaxy_story().chapter;
    for command in [story_warp.clone(), PlayerCommand::UseScanner, story_warp] {
        harness.clients[alex].queued_actions.push(command);
        harness.run_for(Duration::from_secs(3));
    }
    harness.run_for(Duration::from_secs(3));
    let world = harness.world();
    assert!(
        world.galaxy_story().chapter > chapter_before,
        "Alex closed the first chapter (goal {:?})",
        world.galaxy_story().current_goal().map(|goal| goal.goal)
    );
    assert_eq!(
        world.place_of(to_sim_player(sam_player(&harness, sam))),
        fungal
    );
    assert_eq!(world.place_of(to_sim_player(host_player)), giant);
    assert_ne!(
        world.place_of(to_sim_player(alex_player)),
        fungal,
        "Alex is elsewhere"
    );
    // Then the crew spreads over the six new kinds of world, a few seconds each.
    let legs = [
        [Theme::MachineWorld, Theme::DeadWorld, Theme::TidallyLocked],
        [Theme::RingWorld, Theme::Fungal, Theme::GasGiant],
    ];
    for leg in legs {
        let [host_world, alex_world, sam_world] = leg.map(body_of_theme);
        harness.host.submit_local_input(
            host_player,
            ControlState::default(),
            vec![warp(host_world)],
        );
        harness.clients[alex].queued_actions.push(warp(alex_world));
        harness.clients[sam].queued_actions.push(warp(sam_world));
        for second in 0..6_u64 {
            // The host digs and drives a little on its world.
            let controls = ControlState::new(
                if second % 2 == 0 { 0.6 } else { -0.6 },
                false,
                second % 3 == 0,
            );
            harness
                .host
                .submit_local_input(host_player, controls, Vec::new());
            harness.run_for(Duration::from_secs(1));
        }
        let world = harness.world();
        assert_eq!(world.place_of(to_sim_player(host_player)), host_world);
        assert_eq!(world.place_of(to_sim_player(alex_player)), alex_world);
    }
    harness.settle(Duration::from_secs(2));
    assert_converged(&harness);
    assert_bounded(&harness);
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let seen = client.session.authority().expect("in game");
        assert_eq!(
            seen.galaxy_story(),
            harness.world().galaxy_story(),
            "one road on every peer"
        );
    }
    // Back to free flight for all (the stores unload as the crew leaves).
    harness.host.submit_local_input(
        host_player,
        ControlState::default(),
        vec![warp(Place::Space)],
    );
    harness.run_for(Duration::from_secs(2));
    assert_converged(&harness);
}

fn sam_player(harness: &super::world_soak::WorldHarness, sam: usize) -> crate::PlayerId {
    harness.clients[sam].session.player().expect("sam")
}
