//! WP-PERSIST over REAL QUIC on localhost, through the app's own runtime (`LocalHost` with its
//! seat binding and parking, `OnlineClient`, the save files): host -> friends play -> autosave
//! -> quit -> reload -> one friend rejoins with every bit of progress, a new player gets a fresh
//! rig beside them, and the other friend waits parked in the lobby.

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use fatherload_netcode::{ClientPhase, SeatId, world::to_sim_player};
use fatherload_quic_transport::JoinTarget;
use fatherload_simulation::{Player, PlayerId, ProfileId, WorldState, terrain::MineralKind};

use super::{client::OnlineClient, headless::headless_world};
use crate::{
    runtime::{CrewToast, LocalHost, LocalIdentity, Runtime, SeatInput},
    save::{self, WorldSave},
};

const FRAME: Duration = Duration::from_millis(5);
/// Upper bound for one wait: each ends as soon as its condition holds, so this only has to
/// outlast a debug build on a heavily loaded machine (it never weakens a check).
const WAIT: Duration = Duration::from_secs(60);

fn host_identity() -> LocalIdentity {
    LocalIdentity {
        profile: Some(ProfileId::new(0x4057)),
        name: "Hana".to_owned(),
    }
}

fn host(world: WorldState) -> Runtime {
    let mut host = LocalHost::online(
        world,
        1,
        host_identity(),
        "127.0.0.1:0".parse().expect("addr"),
        false,
    )
    .expect("network thread starts");
    host.set_host_name("Hana");
    Runtime::Host(Box::new(host))
}

fn join(addr: SocketAddr, name: &str, profile: u64) -> Runtime {
    Runtime::Client(Box::new(
        OnlineClient::join(
            JoinTarget::Addr(addr),
            name,
            None,
            Some(fatherload_netcode::ProfileId(profile)),
            headless_world(),
        )
        .expect("client network thread starts"),
    ))
}

/// One real-time frame for every runtime: clients drill while in game.
fn step(runtimes: &mut [&mut Runtime], dt: Duration) {
    for runtime in runtimes.iter_mut() {
        runtime.pump_network(dt.as_secs_f32());
        if let Some(seat) = runtime.local_seats().first().copied()
            && runtime.online_role() == Some(crate::runtime::OnlineRole::Client)
        {
            runtime.submit(
                seat.seat,
                SeatInput {
                    controls: fatherload_simulation::ControlState::new(0.0, false, true),
                    actions: Vec::new(),
                },
            );
        }
        let _frame = runtime.advance(dt);
    }
}

fn run_until(
    runtimes: &mut [&mut Runtime],
    what: &str,
    mut done: impl FnMut(&[&mut Runtime]) -> bool,
) {
    let deadline = Instant::now() + WAIT;
    let mut last = Instant::now();
    while !done(runtimes) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(FRAME);
        let now = Instant::now();
        step(runtimes, now - last);
        last = now;
    }
}

fn run_for(runtimes: &mut [&mut Runtime], duration: Duration) {
    let end = Instant::now() + duration;
    run_until(runtimes, "the play window", |_| Instant::now() >= end);
}

fn addr(host: &mut Runtime) -> SocketAddr {
    let mut found = None;
    run_until(&mut [host], "the host endpoint", |runtimes| {
        found = runtimes[0].lobby_info().and_then(|lobby| lobby.addr);
        found.is_some()
    });
    found.expect("bound")
}

fn in_game(runtime: &Runtime) -> bool {
    matches!(runtime, Runtime::Client(client) if client.session().phase() == ClientPhase::InGame)
}

fn player(runtime: &Runtime) -> PlayerId {
    runtime.local_seats().first().expect("joined").player
}

fn leave(runtime: Runtime) {
    if let Runtime::Client(client) = runtime {
        client.leave_blocking();
    }
}

fn quit(runtime: Runtime) {
    if let Runtime::Host(mut host) = runtime {
        host.end_session();
        if let Some(net) = host.take_net() {
            net.shutdown_blocking();
        }
    }
}

/// What a rig carries between sessions (its height may settle by a pixel).
fn progress(rig: &Player) -> String {
    let mut rig = rig.clone();
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.y = (rig.y / fatherload_simulation::TILE_SIZE).round();
    format!("{rig:?}")
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one end-to-end story: two sessions around a save"
)]
fn friends_rejoin_a_reloaded_save_over_real_quic() {
    // Session one: Alex and Sam join Hana's world and dig.
    let mut hana = host(headless_world());
    let hana_addr = addr(&mut hana);
    let mut alex = join(hana_addr, "Alex", 0xa1e8);
    let mut sam = join(hana_addr, "Sam", 0x5a3a);
    run_until(
        &mut [&mut hana, &mut alex, &mut sam],
        "both friends in the lobby",
        |runtimes| {
            runtimes[0]
                .lobby_info()
                .is_some_and(|lobby| lobby.roster.iter().filter(|e| e.connected).count() == 3)
        },
    );
    hana.start_online_game();
    run_until(
        &mut [&mut hana, &mut alex, &mut sam],
        "in game",
        |runtimes| in_game(runtimes[1]) && in_game(runtimes[2]),
    );
    run_for(
        &mut [&mut hana, &mut alex, &mut sam],
        Duration::from_millis(1_500),
    );
    let (alex_id, sam_id) = (player(&alex), player(&sam));

    // The autosave, with progress no fresh rig has.
    let mut world = hana.world().clone();
    for (id, credits) in [(alex_id, 31_337), (sam_id, 4_242)] {
        let rig = world.player_mut(id).expect("friend's rig");
        rig.credits = credits;
        rig.cargo_bay.insert(MineralKind::Diamond, 3);
        rig.drill_strength = 5;
    }
    let path = std::env::temp_dir().join(format!(
        "fatherload-persist-quic-{}.json",
        std::process::id()
    ));
    save::save_world_to_path(
        &WorldSave {
            world,
            play_seconds: 99.0,
        },
        &path,
    )
    .expect("autosave");

    // Everyone quits.
    leave(alex);
    leave(sam);
    quit(hana);

    // Reload: both friends are listed as saved and waiting; no ghost rigs in the world.
    let loaded = save::load_world_from_path(&path).expect("save loads");
    let _ = std::fs::remove_file(&path);
    let saved_alex = loaded.world.player(alex_id).cloned().expect("alex saved");
    let mut hana = host(loaded.world);
    assert_eq!(
        hana.world().player_ids().count(),
        1,
        "only Hana's rig is live"
    );
    let lobby = hana.lobby_info().expect("hosting");
    let mut waiting: Vec<(String, bool)> = lobby
        .roster
        .iter()
        .filter(|entry| entry.parked)
        .map(|entry| (entry.name.clone(), entry.connected))
        .collect();
    waiting.sort();
    assert_eq!(
        waiting,
        [("Alex".to_owned(), false), ("Sam".to_owned(), false)]
    );

    // Alex comes back; Robin is new.
    let hana_addr = addr(&mut hana);
    let mut alex = join(hana_addr, "Alex", 0xa1e8);
    let mut robin = join(hana_addr, "Robin", 0x0b1e);
    run_until(
        &mut [&mut hana, &mut alex, &mut robin],
        "Alex and Robin in the lobby",
        |runtimes| {
            runtimes[0]
                .lobby_info()
                .is_some_and(|lobby| lobby.roster.iter().filter(|e| e.connected).count() == 3)
                && !runtimes[1].local_seats().is_empty()
                && !runtimes[2].local_seats().is_empty()
        },
    );
    assert_eq!(player(&alex), alex_id, "recognized by profile");
    assert_eq!(
        progress(hana.world().player(alex_id).expect("alex is live")),
        progress(&saved_alex)
    );
    let robin_id = player(&robin);
    assert!(robin_id != alex_id && robin_id != sam_id);
    assert_eq!(
        hana.world().player(robin_id).map(|rig| rig.credits),
        Some(hana.world().player_template().credits),
        "a fresh rig"
    );
    let lobby = hana.lobby_info().expect("hosting");
    assert!(
        lobby
            .roster
            .iter()
            .any(|entry| entry.name == "Sam" && entry.parked),
        "Sam still waits"
    );
    assert!(
        lobby
            .roster
            .iter()
            .any(|entry| entry.name == "Alex" && entry.connected && !entry.parked)
    );
    let host_toasts = hana.take_crew_toasts();
    assert!(host_toasts.contains(&CrewToast::CrewmateReturned {
        name: "Alex".to_owned()
    }));
    assert!(host_toasts.contains(&CrewToast::CrewmateJoined {
        name: "Robin".to_owned()
    }));
    assert!(alex.take_crew_toasts().contains(&CrewToast::WelcomeBack {
        name: "Alex".to_owned()
    }));
    assert!(
        !robin
            .take_crew_toasts()
            .iter()
            .any(|toast| matches!(toast, CrewToast::WelcomeBack { .. }))
    );

    hana.start_online_game();
    run_until(
        &mut [&mut hana, &mut alex, &mut robin],
        "in game",
        |runtimes| in_game(runtimes[1]) && in_game(runtimes[2]),
    );
    // At least 1.2 s of play, and until each client applied a few server frames (a loaded
    // machine runs fewer frames in a fixed window).
    let window = Instant::now() + Duration::from_millis(1_200);
    run_until(
        &mut [&mut hana, &mut alex, &mut robin],
        "a few server frames on each client",
        |runtimes| {
            Instant::now() >= window
                && runtimes[1..].iter().all(|runtime| {
                    matches!(&**runtime, Runtime::Client(client)
                        if client.session().stats().frames_received >= 30)
                })
        },
    );
    for runtime in [&alex, &robin] {
        let Runtime::Client(client) = runtime else {
            unreachable!("joined clients")
        };
        assert_eq!(client.session().stats().hash_mismatches, 0);
        assert!(
            client
                .world()
                .player(to_sim_player(fatherload_netcode::PlayerId(
                    u32::try_from(alex_id.get()).expect("small")
                )))
                .is_some()
        );
    }
    assert_eq!(hana.local_seats()[0].seat, SeatId(0));
    leave(alex);
    leave(robin);
    quit(hana);
}
