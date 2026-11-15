//! Multiplayer persistence scenes (WP-PERSIST): the host's lobby with saved friends offline and
//! waiting, a rig claim to approve, a client's "claim a saved rig" entry, the "host this world
//! online" offer after loading a save, and the "Welcome back" banner.

use fatherload_netcode::{ClaimRequest, ClientId, LobbyEntry, PlayerId as NetPlayerId};
use fatherload_simulation::{economy::SurfaceZone, terrain::MineralKind};

use super::{Scene, SceneStage, park_at};
use crate::{
    runtime::{LobbyInfo, OnlineRole},
    shell::{FanfareKind, OnlineScreen, RunMode},
};

/// `(id, name, state)` roster rows: `L` local host, `R` ready, `O` online, `P` parked (saved,
/// offline), `D` disconnected (reconnecting).
fn roster(rows: &[(u32, &str, char)]) -> Vec<LobbyEntry> {
    rows.iter()
        .map(|&(id, name, state)| LobbyEntry {
            player: NetPlayerId(id),
            name: name.to_owned(),
            ready: state == 'R',
            connected: matches!(state, 'L' | 'R' | 'O'),
            local: state == 'L',
            parked: state == 'P',
        })
        .collect()
}

fn lobby(stage: &mut SceneStage<'_>, role: OnlineRole, info: LobbyInfo, message: &str) {
    let shell = stage.shell();
    shell.run_mode = RunMode::Lobby;
    shell.online.screen = OnlineScreen::Lobby;
    shell.online.selected = 0;
    shell.online.role = Some(role);
    message.clone_into(&mut shell.message);
    shell.online.lobby = Some(info);
}

pub const CREW_SCENES: &[Scene] = &[
    Scene {
        name: "lobby_saved_crew",
        summary: "Hosting a resumed save: friends listed by name, two back online, two offline and waiting",
        setup: |stage| {
            lobby(
                stage,
                OnlineRole::Host,
                LobbyInfo {
                    roster: roster(&[
                        (0, "Hana", 'L'),
                        (1, "Alex", 'R'),
                        (4, "Robin", 'O'),
                        (2, "Sam", 'P'),
                        (3, "Jo", 'P'),
                    ]),
                    addr: "192.168.1.20:47777".parse().ok(),
                    descriptor: Some("{}".to_owned()),
                    lan_published: true,
                    status: "Hosting on 192.168.1.20:47777 (2 remote player(s) connected)"
                        .to_owned(),
                    local_player: Some(NetPlayerId(0)),
                    ..LobbyInfo::default()
                },
                "Alex is back: their rig is right where they left it.",
            );
        },
    },
    Scene {
        name: "lobby_rig_claim_host",
        summary: "A friend on a new laptop asks for their saved rig; the host approves or denies",
        setup: |stage| {
            lobby(
                stage,
                OnlineRole::Host,
                LobbyInfo {
                    roster: roster(&[(0, "Hana", 'L'), (5, "Sam", 'O'), (2, "Sam", 'P')]),
                    addr: "192.168.1.20:47777".parse().ok(),
                    descriptor: Some("{}".to_owned()),
                    lan_published: true,
                    status: "Hosting on 192.168.1.20:47777 (1 remote player(s) connected)"
                        .to_owned(),
                    local_player: Some(NetPlayerId(0)),
                    claims: vec![ClaimRequest {
                        client: ClientId(7),
                        name: "Sam".to_owned(),
                        target: NetPlayerId(2),
                        target_name: "Sam".to_owned(),
                    }],
                    ..LobbyInfo::default()
                },
                "Sam (new device) asks for Sam's saved rig: answer in Pause > Online Session.",
            );
            stage.shell().online.selected = 1;
        },
    },
    Scene {
        name: "lobby_claim_saved_rig",
        summary: "Joined from a new device: claim your saved rig from the host's save",
        setup: |stage| {
            lobby(
                stage,
                OnlineRole::Client,
                LobbyInfo {
                    roster: roster(&[
                        (0, "Hana", 'L'),
                        (5, "Sam", 'O'),
                        (2, "Sam", 'P'),
                        (3, "Jo", 'P'),
                    ]),
                    status: "In the host's lobby. Waiting for the host to start.".to_owned(),
                    local_player: Some(NetPlayerId(5)),
                    ..LobbyInfo::default()
                },
                "",
            );
            stage.shell().online.selected = 1;
        },
    },
    Scene {
        name: "resume_host_offer",
        summary: "Loading a save that holds friends offers to host it online straight away",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Depot);
            });
            stage.settle(3);
            stage.shell().offer_resume_hosting(vec![
                "Alex".to_owned(),
                "Sam".to_owned(),
                "Jo".to_owned(),
            ]);
        },
    },
    Scene {
        name: "welcome_back_toast",
        summary: "A friend rejoins a resumed world and lands in their own rig: the welcome-back banner",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                park_at(world, player, SurfaceZone::Depot);
                // The saved progress they get back: credits, a loaded bay and upgraded parts.
                if let Some(rig) = world.player_mut(player) {
                    rig.credits = 31_337;
                    rig.cargo_bay.insert(MineralKind::Gold, 4);
                    rig.cargo_bay.insert(MineralKind::Ruby, 3);
                    rig.drill_strength = 6;
                    rig.engine_level = 5;
                    rig.cargo_bay_level = 4;
                    rig.hull_level = 4;
                    rig.bombs = 6;
                }
            });
            stage.settle(5);
            stage.shell().quest_fanfare(
                0,
                FanfareKind::WelcomeBack,
                "Welcome back, Alex".to_owned(),
                "Your rig, cargo, credits and upgrades are right where you left them.".to_owned(),
            );
            "Joined the session.".clone_into(&mut stage.shell().message);
            stage.settle(20);
        },
    },
];
