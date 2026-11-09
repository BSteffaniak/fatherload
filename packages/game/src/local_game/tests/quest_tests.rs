//! Quests through `LocalGame::frame`: the quest tab at a keeper's counter, taking, finishing and
//! turning in a solo job, a two-seat crew job, the quest log's tracked quest, and a save made
//! mid-quest.

use drillgame_simulation::quest::{QuestGiver, QuestId};

use super::*;
use crate::shell::{DeskRow, FanfareKind, desk_rows};

const SCALES: QuestId = QuestId(101);
const TWO_COUNTERS: QuestId = QuestId(104);

fn right() -> PlayerInput {
    PlayerInput {
        menu_right: true,
        ..PlayerInput::default()
    }
}

/// Seat 0 moves its selection to desk row `row` (from the top).
fn select_row(game: &mut LocalGame, row: usize) {
    for _ in 0..12 {
        let _report = press(
            game,
            PlayerInput {
                menu_up: true,
                ..PlayerInput::default()
            },
        );
    }
    for _ in 0..row {
        let _report = press(game, down());
    }
    assert_eq!(game.shell.seat(0).selected_menu_item, row);
}

/// The index of `quest`'s row on `giver`'s desk for `player`.
fn row_of(game: &LocalGame, player: PlayerId, giver: QuestGiver, quest: QuestId) -> usize {
    desk_rows(game.world(), player, giver)
        .iter()
        .position(|row| row.quest() == quest)
        .expect("the quest is on the desk")
}

#[test]
fn a_solo_pilot_takes_finishes_and_turns_in_a_job_at_the_counter_tab() {
    let world = rig_at_building(SurfaceZone::Depot, |rig| {
        rig.cargo.insert(MineralKind::Iron, 6);
    });
    let mut game = playing(world, 1, "quest-solo");
    idle(&mut game, 5);
    walk_to_counter(&mut game, SurfaceZone::Depot);
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Depot));
    let _report = press(&mut game, right());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::QuestDesk));
    let row = row_of(&game, PLAYER, QuestGiver::Kade, SCALES);
    select_row(&mut game, row);
    confirm(&mut game);
    let quests = game.world().visible_quests(PLAYER);
    let quest = quests
        .iter()
        .find(|quest| quest.quest == SCALES)
        .expect("accepted");
    assert!(quest.ready, "the iron is already aboard");
    assert_eq!(game.shell.seat(0).tracked_quest, Some(SCALES));
    assert!(
        game.shell.seat(0).message.contains("Scales Don't Lie"),
        "{}",
        game.shell.seat(0).message
    );
    // The finished job tops the desk.
    assert_eq!(
        desk_rows(game.world(), PLAYER, QuestGiver::Kade).first(),
        Some(&DeskRow::TurnIn(SCALES))
    );
    let credits = player(&game, PLAYER).credits;
    select_row(&mut game, 0);
    confirm(&mut game);
    let rig = player(&game, PLAYER);
    assert_eq!(rig.credits, credits + 170);
    assert!(rig.quests.completed.contains(&SCALES));
    let fanfare = game.shell.seat(0).fanfare.clone().expect("fanfare");
    assert_eq!(fanfare.kind, FanfareKind::Complete);
    assert_eq!(fanfare.title, "Scales Don't Lie");
    // Back to the depot's services with the other direction.
    let _report = press(
        &mut game,
        PlayerInput {
            menu_left: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Depot));
}

#[test]
fn a_two_seat_crew_splits_a_delivery_and_both_get_paid() {
    let second = PlayerId::new(2);
    let mut world = split_world(&[Some(SurfaceZone::Depot), Some(SurfaceZone::Salvage)]);
    {
        let lead = world.player_mut(PLAYER).expect("lead");
        lead.quests.reputation.insert(QuestGiver::Kade, 10);
        lead.quests.deepest_row = drillgame_simulation::worldgen::strata::scaled_row(
            &drillgame_simulation::worldgen::WorldSettings::default(),
            30,
        );
        lead.cargo.insert(MineralKind::Silver, 2);
    }
    world
        .player_mut(second)
        .expect("wing")
        .cargo
        .insert(MineralKind::Silver, 2);
    let mut game = playing(world, 2, "quest-crew");
    idle(&mut game, 5);
    press_seat(&mut game, 0, interact());
    walk_seats_to_counters(&mut game, &[0]);
    let _report = press(&mut game, right());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::QuestDesk));
    let row = row_of(&game, PLAYER, QuestGiver::Kade, TWO_COUNTERS);
    select_row(&mut game, row);
    confirm(&mut game);
    // Accepted as a crew quest: shared, and both rigs already stand at the two counters.
    let crew = &game.world().crew_quests().active;
    assert_eq!(crew.len(), 1, "a crew quest");
    assert!(crew[0].ready, "both halves dropped together");
    assert!(
        game.world()
            .visible_quests(second)
            .iter()
            .any(|quest| quest.quest == TWO_COUNTERS)
    );
    let (lead_credits, wing_credits) =
        (player(&game, PLAYER).credits, player(&game, second).credits);
    select_row(&mut game, 0);
    confirm(&mut game);
    assert_eq!(player(&game, PLAYER).credits, lead_credits + 460);
    assert_eq!(player(&game, second).credits, wing_credits + 460 / 4);
    assert!(game.world().crew_quests().active.is_empty());
}

#[test]
fn a_crew_job_can_also_be_taken_solo_while_others_play() {
    let second = PlayerId::new(2);
    let mut world = split_world(&[Some(SurfaceZone::Depot), Some(SurfaceZone::Salvage)]);
    {
        let lead = world.player_mut(PLAYER).expect("lead");
        lead.quests.reputation.insert(QuestGiver::Kade, 10);
        lead.quests.deepest_row = drillgame_simulation::worldgen::strata::scaled_row(
            &drillgame_simulation::worldgen::WorldSettings::default(),
            30,
        );
    }
    let mut game = playing(world, 2, "quest-crew-solo");
    idle(&mut game, 5);
    press_seat(&mut game, 0, interact());
    walk_seats_to_counters(&mut game, &[0]);
    let _report = press(&mut game, right());
    let rows = desk_rows(game.world(), PLAYER, QuestGiver::Kade);
    let crew_row = row_of(&game, PLAYER, QuestGiver::Kade, TWO_COUNTERS);
    assert!(matches!(rows[crew_row], DeskRow::Offer(_)));
    assert!(
        matches!(&rows[crew_row + 1], DeskRow::SoloOffer(offer) if offer.quest == TWO_COUNTERS),
        "the solo variant sits right under the crew row"
    );
    // Solo-only quests get one row.
    assert_eq!(
        rows.iter().filter(|row| row.quest() == SCALES).count(),
        1,
        "{rows:?}"
    );
    select_row(&mut game, crew_row + 1);
    confirm(&mut game);
    assert!(game.world().crew_quests().active.is_empty());
    assert!(
        player(&game, PLAYER)
            .quests
            .active
            .iter()
            .any(|quest| quest.quest == TWO_COUNTERS && !quest.crew),
        "taken as the lead's own solo job"
    );
    assert!(
        !game
            .world()
            .visible_quests(second)
            .iter()
            .any(|quest| quest.quest == TWO_COUNTERS)
    );
}

/// Accepts `quest` for the world's first rig straight through the simulation (standing it at
/// the giver's door for one tick).
fn accept_in_world(world: &mut WorldState, quest: QuestId) {
    use drillgame_simulation::{
        InputSequence, PlayerTickInput, TickAction, TickInputs, service::SessionServiceRequest,
    };
    let giver = quest.def().expect("quest").giver;
    let door = drillgame_simulation::quest::binding::giver_door(giver);
    let rig = world.player_mut(PLAYER).expect("rig");
    let home = rig.x;
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
    {
        rig.x = (door.x as f32 + 0.5) * TILE_SIZE;
    }
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        PLAYER,
        PlayerTickInput {
            controls: None,
            actions: vec![TickAction {
                sequence: InputSequence::new(0),
                command: PlayerCommand::Service(SessionServiceRequest::AcceptQuest {
                    quest,
                    crew: false,
                }),
            }],
        },
    );
    let tick = world.simulation_tick();
    let _output = world.simulate_tick(tick, &inputs);
    world.player_mut(PLAYER).expect("rig").x = home;
}

#[test]
fn the_quest_log_tracks_the_selected_job_and_flips_with_the_map() {
    let mut world = rig_at_building(SurfaceZone::Fuel, |rig| {
        rig.quests.deepest_row = drillgame_simulation::worldgen::strata::scaled_row(
            &drillgame_simulation::worldgen::WorldSettings::default(),
            30,
        );
    });
    accept_in_world(&mut world, QuestId(301));
    let mut game = playing(world, 1, "quest-log");
    idle(&mut game, 5);
    // And a second job through Pip's counter tab.
    walk_to_counter(&mut game, SurfaceZone::Fuel);
    let _report = press(&mut game, right());
    let row = row_of(&game, PLAYER, QuestGiver::Pip, QuestId(501));
    select_row(&mut game, row);
    confirm(&mut game);
    let _report = press(&mut game, back_input());
    let _report = press(&mut game, back_input());
    idle(&mut game, 2);
    assert_eq!(game.shell.seat(0).interior_zone, None);
    assert_eq!(game.world().visible_quests(PLAYER).len(), 2);
    let _report = press(
        &mut game,
        PlayerInput {
            map: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
    let _report = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::ResearchLog));
    let _report = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::QuestLog));
    // Solo quests come first in accept order: Nix's demolition, then Pip's run.
    let _report = press(&mut game, down());
    assert_eq!(game.shell.seat(0).tracked_quest, Some(QuestId(501)));
    let _report = press(
        &mut game,
        PlayerInput {
            menu_up: true,
            ..PlayerInput::default()
        },
    );
    assert_eq!(game.shell.seat(0).tracked_quest, Some(QuestId(301)));
    let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
    let view = snapshot.primary_view().expect("view");
    assert_eq!(view.quests.len(), 2);
    assert!(
        !view.quest_markers.is_empty(),
        "the demolition marks are on the map"
    );
    let _report = press(&mut game, confirm_input());
    assert_eq!(game.shell.seat(0).modal, Some(ModalScreen::Map));
}

#[test]
fn a_save_made_mid_quest_loads_and_the_job_carries_on() {
    let world = rig_at_building(SurfaceZone::Depot, |_| {});
    let mut game = playing(world, 1, "quest-save");
    idle(&mut game, 5);
    walk_to_counter(&mut game, SurfaceZone::Depot);
    let _report = press(&mut game, right());
    let row = row_of(&game, PLAYER, QuestGiver::Kade, SCALES);
    select_row(&mut game, row);
    confirm(&mut game);
    let save = save::WorldSave {
        world: game.world().clone(),
        play_seconds: 1.0,
    };
    let path =
        std::env::temp_dir().join(format!("drillgame-quest-save-{}.json", std::process::id()));
    save::save_world_to_path(&save, &path).expect("write");
    let loaded = save::load_world_from_path(&path).expect("read");
    let _ignored = std::fs::remove_file(&path);
    assert_eq!(
        loaded.world.simulation_state_hash(),
        save.world.simulation_state_hash()
    );
    let quest = loaded
        .world
        .visible_quests(PLAYER)
        .into_iter()
        .find(|quest| quest.quest == SCALES)
        .cloned()
        .expect("the job survived the save");
    assert!(!quest.ready);
    assert!(quest.deadline.is_some(), "its clock too");
    let mut world = loaded.world;
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo
        .insert(MineralKind::Iron, 5);
    let mut game = playing(world, 1, "quest-save-loaded");
    idle(&mut game, 5);
    walk_to_counter(&mut game, SurfaceZone::Depot);
    let _report = press(&mut game, right());
    select_row(&mut game, 0);
    confirm(&mut game);
    assert!(player(&game, PLAYER).quests.completed.contains(&SCALES));
}
