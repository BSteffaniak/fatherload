//! WP-PERSIST through `LocalGame::frame`: resuming a save that holds friends offers to host it
//! online, the lobby lists them as saved and waiting, and a returning player's "Welcome back"
//! banner waits for the gameplay view.

use fatherload_simulation::CrewMember;

use super::*;
use crate::{
    profile::PlayerProfile,
    runtime::LocalIdentity,
    shell::{FanfareKind, OnlineScreen},
};

const ALEX: PlayerId = PlayerId::new(7);

/// A save from an earlier online session: Kit's own rig (recorded for Kit's profile) and Alex's,
/// written while Alex was still connected.
fn save_with_a_friend(path: &std::path::Path, kit: &PlayerProfile) {
    let identity = LocalIdentity {
        profile: Some(kit.id),
        name: kit.name.clone(),
    };
    let mut world = Runtime::local_as(new_world(), 1, identity).world().clone();
    world.add_player(ALEX);
    world.set_crew_member(
        ALEX,
        CrewMember {
            profile: Some(fatherload_simulation::ProfileId::new(0xa1e8)),
            name: "Alex".to_owned(),
            seat: None,
        },
    );
    world.player_mut(ALEX).expect("alex").credits = 5_150;
    save::save_world_to_path(
        &WorldSave {
            world,
            play_seconds: 600.0,
        },
        path,
    )
    .expect("save written");
}

fn resumed_game(name: &str) -> (LocalGame, std::path::PathBuf) {
    let path = temp_save(name);
    let kit = PlayerProfile::fixed(0xbeef, "Kit");
    save_with_a_friend(&path, &kit);
    let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
    game.set_profile(kit);
    game.set_host_bind("127.0.0.1:0".parse().expect("addr"));
    // The title's first entry resumes the save.
    let _report = press(&mut game, confirm_input());
    (game, path)
}

#[test]
fn resuming_a_save_with_friends_offers_to_host_it_online() {
    let (mut game, path) = resumed_game("resume-crew-host");
    assert_eq!(game.shell.run_mode, RunMode::Lobby);
    assert_eq!(game.shell.online.screen, OnlineScreen::ResumeHost);
    assert_eq!(game.shell.online.resume_crew, ["Alex"]);
    // No ghost rig: Alex waits parked, Kit plays their own rig.
    assert!(game.world().is_parked(ALEX));
    assert_eq!(game.seats().len(), 1);
    assert_ne!(game.seats()[0].player, ALEX);

    // "Host this world online": the lobby lists Alex as saved and waiting.
    let _report = press(&mut game, confirm_input());
    let _ignored = std::fs::remove_file(&path);
    assert_eq!(game.runtime().online_role(), Some(OnlineRole::Host));
    assert_eq!(game.shell.online.screen, OnlineScreen::Lobby);
    idle(&mut game, 3);
    let lobby = game.shell.online.lobby.clone().expect("hosting");
    let alex = lobby
        .roster
        .iter()
        .find(|entry| entry.name == "Alex")
        .expect("Alex is listed");
    assert!(alex.parked && !alex.connected);
    assert!(
        lobby
            .roster
            .iter()
            .any(|entry| entry.name == "Kit" && entry.local)
    );
    assert_eq!(
        game.world()
            .parked_player(ALEX)
            .map(|parked| parked.player.credits),
        Some(5_150)
    );
    // Starting the session keeps the save's play time.
    let _report = press(&mut game, confirm_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert!((game.shell.play_seconds - 600.0).abs() < 1.0);
    game.shutdown();
}

#[test]
fn resuming_offline_keeps_friends_parked_for_later() {
    let (mut game, path) = resumed_game("resume-crew-offline");
    assert_eq!(game.shell.online.screen, OnlineScreen::ResumeHost);
    let _report = press(&mut game, down());
    let _report = press(&mut game, confirm_input());
    assert_eq!(game.shell.run_mode, RunMode::Playing);
    assert!(game.shell.message.contains("1 saved crewmate"));
    assert!(game.runtime().online_role().is_none());
    // Saving again keeps Alex in the roster.
    let _report = press(
        &mut game,
        PlayerInput {
            save: true,
            ..PlayerInput::default()
        },
    );
    let saved = save::load_world_from_path(&path).expect("saved");
    let _ignored = std::fs::remove_file(&path);
    assert!(saved.world.is_parked(ALEX));
}

#[test]
fn the_welcome_back_banner_waits_for_the_gameplay_view() {
    let mut game = title_game("welcome-banner");
    game.shell.show_online(OnlineScreen::Lobby);
    game.pending_welcome = Some(("Welcome back, Alex".to_owned(), "detail".to_owned()));
    idle(&mut game, 2);
    assert!(game.shell.seat(0).fanfare.is_none(), "not over the lobby");
    game.shell.enter_world("Joined the session.", 0.0);
    let report = step(&mut game, PlayerInput::default());
    let fanfare = game.shell.seat(0).fanfare.clone().expect("banner up");
    assert_eq!(fanfare.kind, FanfareKind::WelcomeBack);
    assert_eq!(fanfare.title, "Welcome back, Alex");
    assert!(
        report
            .sound_cues
            .iter()
            .any(|cue| cue.cue == SoundCue::CrewReturn)
    );
    assert!(game.pending_welcome.is_none());
}
