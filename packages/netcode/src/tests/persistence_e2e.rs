//! WP-PERSIST over the `LinkSim` harness on the real world: friends play, the host autosaves
//! (a JSON round trip, as the game's save files are), quits, reloads and the friends rejoin with
//! every bit of progress; a brand-new player gets a fresh rig beside them; a player from a new
//! install claims their saved rig with the host's approval.

use std::time::Duration;

use fatherload_simulation::{Player, WorldState, terrain::MineralKind};

use super::support::assert_converged;
use crate::harness::{Harness, HarnessConfig};
use crate::world::to_sim_player;
use crate::{CrewNotice, IdentityNotice, LinkConditions, PlayerId, ProfileId, SessionParams};

type WorldHarness = Harness<WorldState>;

const ALEX: ProfileId = ProfileId(0xa1e8_0000_0000_0001);
const SAM: ProfileId = ProfileId(0x5a3a_0000_0000_0002);
const NEWBIE: ProfileId = ProfileId(0x0e3b_0000_0000_0003);
const SAM_NEW_LAPTOP: ProfileId = ProfileId(0x5a3a_0000_0000_00ff);

fn harness() -> WorldHarness {
    let link = LinkConditions {
        latency: Duration::from_millis(40),
        jitter: Duration::from_millis(10),
        loss: 0.02,
        ..LinkConditions::default()
    };
    WorldHarness::new(HarnessConfig::new(SessionParams::new(60), 0, link))
}

/// The save file: the world through JSON, exactly what the game writes and reads.
fn autosave(harness: &WorldHarness) -> WorldState {
    let json = serde_json::to_string(harness.world()).expect("world serializes");
    serde_json::from_str(&json).expect("world deserializes")
}

fn wait_in_game(harness: &mut WorldHarness) {
    let mut guard = 0;
    while !harness.all_in_game() {
        harness.step();
        guard += 1;
        assert!(guard < 5_000, "clients never got in game");
    }
}

/// Steps in 1 ms slices until the host has bound the client at `index` (no tick runs in
/// between, so the world still holds exactly what the rejoin restored).
fn wait_joined(harness: &mut WorldHarness, index: usize) -> PlayerId {
    let id = harness.clients[index].id;
    let mut guard = 0;
    loop {
        if let Some(player) = harness.host.client_player(id) {
            return player;
        }
        harness.step_dt(Duration::from_millis(1));
        guard += 1;
        assert!(guard < 20_000, "host never bound the client");
    }
}

/// Everything a rig carries between sessions. A rig parked in flight may fall a pixel on a
/// tick that runs before the comparison, so its height is compared to the nearest tile.
fn progress(player: &Player) -> String {
    let mut player = player.clone();
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.y = (player.y / fatherload_simulation::TILE_SIZE).round();
    format!("{player:?}")
}

fn rig(harness: &WorldHarness, player: PlayerId) -> Player {
    harness
        .world()
        .player(to_sim_player(player))
        .cloned()
        .expect("rig in the live world")
}

/// Alex and Sam play a session; returns their players.
fn play_first_session(harness: &mut WorldHarness) -> (PlayerId, PlayerId) {
    let alex = harness.add_client_as(None, Some(ALEX), "Alex");
    let sam = harness.add_client_as(None, Some(SAM), "Sam");
    wait_in_game(harness);
    harness.run_for(Duration::from_secs(4));
    harness.settle(Duration::from_secs(1));
    let alex = harness.clients[alex].session.player().expect("alex");
    let sam = harness.clients[sam].session.player().expect("sam");
    assert_ne!(alex, sam);
    (alex, sam)
}

#[test]
fn friends_rejoin_a_reloaded_save_with_all_their_progress() {
    let mut harness = harness();
    let (alex, sam) = play_first_session(&mut harness);
    let mut save = autosave(&harness);
    // Give both rigs distinct progress that survives only if the roster does.
    for (player, credits) in [(alex, 12_345), (sam, 54_321)] {
        let rig = save.player_mut(to_sim_player(player)).expect("saved rig");
        rig.credits = credits;
        rig.cargo_bay.insert(MineralKind::Ruby, 2);
        rig.engine_level = 3;
    }
    let saved_alex = save.player(to_sim_player(alex)).cloned().expect("alex");
    let saved_sam = save.player(to_sim_player(sam)).cloned().expect("sam");

    // Quit and reload: nobody is connected, so both rigs are parked, not deleted.
    harness.restart_host(save);
    assert!(harness.world().player(to_sim_player(alex)).is_none());
    let roster = harness.host.roster();
    let parked: Vec<(PlayerId, String)> = roster
        .iter()
        .filter(|entry| entry.parked)
        .map(|entry| (entry.player, entry.name.clone()))
        .collect();
    assert_eq!(
        parked,
        [(alex, "Alex".to_owned()), (sam, "Sam".to_owned())],
        "the lobby lists both saved players as offline"
    );

    // Both rejoin from their installs: recognized automatically, same rigs, same progress.
    let alex_again = harness.add_client_as(None, Some(ALEX), "Alex");
    assert_eq!(wait_joined(&mut harness, alex_again), alex);
    assert_eq!(progress(&rig(&harness, alex)), progress(&saved_alex));
    let sam_again = harness.add_client_as(None, Some(SAM), "Sam");
    assert_eq!(wait_joined(&mut harness, sam_again), sam);
    assert_eq!(progress(&rig(&harness, sam)), progress(&saved_sam));
    assert!(harness.host.roster().iter().all(|entry| !entry.parked));
    let joins: Vec<_> = harness
        .host
        .take_crew_notices()
        .into_iter()
        .filter_map(|notice| match notice {
            CrewNotice::Joined {
                name, returning, ..
            } => Some((name, returning)),
            _ => None,
        })
        .collect();
    assert!(joins.contains(&("Alex".to_owned(), true)));
    assert!(joins.contains(&("Sam".to_owned(), true)));

    wait_in_game(&mut harness);
    for index in [alex_again, sam_again] {
        assert!(
            harness.clients[index]
                .session
                .take_identity_notices()
                .contains(&IdentityNotice::WelcomeBack),
            "the client gets its welcome-back toast"
        );
    }
    harness.reset_measurements();
    harness.run_for(Duration::from_secs(3));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    harness.assert_actions_exactly_once();

    // Sam leaves for good: parked again (saved), Alex keeps playing.
    harness.disconnect(sam_again);
    harness.run_for(Duration::from_secs(40));
    assert!(
        harness.world().is_parked(to_sim_player(sam)),
        "an expired reservation parks the rig instead of deleting it"
    );
}

#[test]
fn a_new_player_gets_a_fresh_rig_beside_a_returning_one() {
    let mut harness = harness();
    let (alex, sam) = play_first_session(&mut harness);
    let save = autosave(&harness);
    let template_credits = save.player_template().credits;
    harness.restart_host(save);

    let alex_again = harness.add_client_as(None, Some(ALEX), "Alex");
    let newbie = harness.add_client_as(None, Some(NEWBIE), "Robin");
    assert_eq!(wait_joined(&mut harness, alex_again), alex);
    let robin = wait_joined(&mut harness, newbie);
    assert!(robin != alex && robin != sam, "never a saved player's id");
    assert_eq!(rig(&harness, robin).credits, template_credits);
    // Sam is still offline and waiting.
    assert!(
        harness
            .host
            .roster()
            .iter()
            .any(|entry| entry.player == sam && entry.parked && entry.name == "Sam")
    );
    wait_in_game(&mut harness);
    assert!(
        !harness.clients[newbie]
            .session
            .take_identity_notices()
            .contains(&IdentityNotice::WelcomeBack)
    );
    harness.reset_measurements();
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
}

#[test]
fn a_player_on_a_new_install_claims_their_saved_rig_with_the_hosts_approval() {
    let mut harness = harness();
    let (_alex, sam) = play_first_session(&mut harness);
    let mut save = autosave(&harness);
    save.player_mut(to_sim_player(sam)).expect("sam").credits = 9_999;
    harness.restart_host(save);

    // Sam's laptop is new: the host does not recognize the profile, so Sam gets a fresh rig.
    let laptop = harness.add_client_as(None, Some(SAM_NEW_LAPTOP), "Sam");
    let fresh = wait_joined(&mut harness, laptop);
    assert_ne!(fresh, sam);
    wait_in_game(&mut harness);

    // A denied claim changes nothing.
    harness.clients[laptop].session.claim_rig(sam);
    let client = harness.clients[laptop].id;
    let mut guard = 0;
    while harness.host.claim_requests().is_empty() {
        harness.step();
        guard += 1;
        assert!(guard < 5_000, "claim never reached the host");
    }
    assert_eq!(harness.host.claim_requests()[0].target_name, "Sam");
    assert!(!harness.host.resolve_claim(client, false));
    harness.run_for(Duration::from_millis(500));
    assert!(
        harness.clients[laptop]
            .session
            .take_identity_notices()
            .contains(&IdentityNotice::ClaimDenied { player: sam })
    );
    assert_eq!(harness.clients[laptop].session.player(), Some(fresh));

    // Asked again and approved: the client now plays Sam's saved rig.
    harness.clients[laptop].session.claim_rig(sam);
    let mut guard = 0;
    while harness.host.claim_requests().is_empty() {
        harness.step();
        guard += 1;
        assert!(guard < 5_000, "claim never reached the host");
    }
    assert!(harness.host.resolve_claim(client, true));
    assert_eq!(harness.host.client_player(client), Some(sam));
    assert!(
        harness.world().player(to_sim_player(fresh)).is_none(),
        "the stand-in rig is gone"
    );
    assert_eq!(rig(&harness, sam).credits, 9_999);
    harness.run_for(Duration::from_millis(500));
    assert_eq!(harness.clients[laptop].session.player(), Some(sam));
    assert!(
        harness.clients[laptop]
            .session
            .take_identity_notices()
            .contains(&IdentityNotice::ClaimApproved)
    );
    harness.reset_measurements();
    harness.run_for(Duration::from_secs(2));
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);

    // The rig now belongs to the laptop's profile: after another reload it comes back
    // automatically.
    let save = autosave(&harness);
    harness.restart_host(save);
    let laptop_again = harness.add_client_as(None, Some(SAM_NEW_LAPTOP), "Sam");
    assert_eq!(wait_joined(&mut harness, laptop_again), sam);
}
