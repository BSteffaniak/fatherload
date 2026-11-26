//! WP-TOMBS, world side: seals and entering, the run slot, a scripted bot clearing a run of
//! each style on the placeholder realizers, banking and death, co-op revives and wipes, relic
//! effects, saves mid-run, replication and tick-rate safety.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    TickOutput, WorldState,
    base::BuiltTile,
    enemy::EnemyDifficulty,
    mechanism::Usable,
    pilot::{PilotAction, PilotView},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        Relic, RunOutcome, RunPhase, SiteOpening, SiteStyle, TOMB_KEY, TombCommand, TombSite,
        realize::SpawnRole,
    },
    worldgen::{GeneratorVersion, WorldSettings, features::TileRect, tomb_sites},
};

use super::{SectionKey, tombs::REVIVE_MILLIS};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::default()
    }
}

fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world
}

fn world() -> WorldState {
    world_at(60)
}

/// The first site of `style` near town (ground settings).
fn site_of(style: SiteStyle) -> TombSite {
    let rect = TileRect {
        min: TilePosition { x: -6_000, y: -100 },
        max: TilePosition { x: 6_000, y: 900 },
    };
    tomb_sites::sites_touching(&settings(), &rect)
        .into_iter()
        .find(|site| site.style == style)
        .expect("a site of the style")
}

fn run_ticks(world: &mut WorldState, inputs: &TickInputs, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let output: TickOutput = world.simulate_tick(tick, inputs);
        events.extend(output.events.into_iter().map(|event| event.kind));
    }
    events
}

fn act(world: &mut WorldState, player: PlayerId, command: PlayerCommand) -> Vec<GameplayEventKind> {
    run_ticks(world, &TickInputs::default().act(player, command), 1)
}

fn use_button(world: &mut WorldState, player: PlayerId) -> Vec<GameplayEventKind> {
    act(world, player, PlayerCommand::Pilot(PilotAction::Use))
}

/// Puts `player`'s pilot out in front of `site`'s door.
fn at_the_door(world: &mut WorldState, player: PlayerId, site: &TombSite, index: usize) {
    let (x, feet) = WorldState::outside_spot(world.frame(), site, index);
    if world.pilot_of(player).is_some() {
        let id = world.pilot_of(player).expect("pilot").0;
        world.edit_pilot(id, |body, _| {
            body.x = x;
            body.y = feet;
            body.previous_x = x;
            body.previous_y = feet;
        });
    } else {
        let _ = world.place_pilot(player, x, feet);
    }
}

fn pilot_middle(world: &WorldState, player: PlayerId) -> (f32, f32) {
    let (_, body, state) = world.pilot_of(player).expect("pilot");
    state.middle(body)
}

fn enter(world: &mut WorldState, site: &TombSite, party: &[PlayerId]) -> u32 {
    for player in party {
        if world.player_mut(*player).is_none() {
            world.add_player(*player);
        }
    }
    world
        .scenario_enter_tomb(site, party)
        .expect("the run starts")
}

// --- Seals and entering ---------------------------------------------------------------------

#[test]
fn a_key_breaks_a_key_seal_and_the_door_lets_the_party_in() {
    let mut world = world();
    let site = tomb_sites::sites_touching(
        &settings(),
        &TileRect {
            min: TilePosition { x: -9_000, y: -100 },
            max: TilePosition { x: 9_000, y: 900 },
        },
    )
    .into_iter()
    .find(|site| site.opening == SiteOpening::Key)
    .expect("a key site");
    assert!(matches!(
        world.terrain().tile(site.door).map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::TombDoor { sealed: true, .. }))
    ));
    world.add_player(OTHER);
    at_the_door(&mut world, PLAYER, &site, 0);
    at_the_door(&mut world, OTHER, &site, 1);
    assert!(matches!(
        world.pilot_usable(PLAYER),
        Some((_, Usable::TombDoor { sealed: true, .. }))
    ));
    // Without a key the seal holds.
    let events = use_button(&mut world, PLAYER);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombSeal { opened: false, .. }))
    );
    // With one it breaks (the key is used up), and the next press goes in with the crewmate.
    world
        .player_mut(PLAYER)
        .expect("rig")
        .pilot_pack
        .add(TOMB_KEY, 1);
    let events = use_button(&mut world, PLAYER);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombSeal { opened: true, .. }))
    );
    assert_eq!(world.players()[&PLAYER].pilot_pack.count(TOMB_KEY), 0);
    assert!(world.tombs().is_unsealed(site.id));
    let events = use_button(&mut world, PLAYER);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombEntered { party: 2, .. }))
    );
    let run = world.tomb_run_of(PLAYER).expect("in a run");
    assert_eq!(run.members.len(), 2);
    assert_eq!(world.tomb_run_of(OTHER).map(|run| run.id), Some(run.id));
    // The party stands in the run slot; the rig waits outside.
    let slot = run.slot;
    let (x, y) = pilot_middle(&world, PLAYER);
    let tile = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    assert_eq!(
        tomb_sites::slot_at(world.terrain().settings(), tile),
        Some(slot)
    );
    assert!(world.in_run_space(tile));
    let rig = &world.players()[&PLAYER];
    assert!(!world.in_run_space(rig.tile_position(TILE_SIZE)));
    assert_eq!(world.tombs().site_runs.get(&site.id), Some(&1));
}

#[test]
fn a_blast_cracks_an_explosives_seal_and_story_seals_answer_finished_stories() {
    let mut world = world();
    let sites = tomb_sites::sites_touching(
        &settings(),
        &TileRect {
            min: TilePosition { x: -9_000, y: -100 },
            max: TilePosition { x: 9_000, y: 900 },
        },
    );
    let blast_site = sites
        .iter()
        .find(|site| site.opening == SiteOpening::Explosives)
        .copied()
        .expect("an explosives site");
    let mut output = TickOutput::default();
    let (x, y) = (
        (blast_site.door.x as f32 + 0.5) * TILE_SIZE,
        (blast_site.door.y as f32 + 0.5) * TILE_SIZE,
    );
    world.blast_tomb_seals(
        &[super::bombs::Blast {
            kind: crate::ExplosiveKind::Bomb,
            x: x - TILE_SIZE * 1.5,
            y,
            owner: Some(PLAYER),
        }],
        &mut output,
    );
    assert!(world.tombs().is_unsealed(blast_site.id));
    // A story seal holds for a player who has seen no story through.
    let story = sites
        .iter()
        .find(|site| site.opening == SiteOpening::Story)
        .copied()
        .expect("a story site");
    at_the_door(&mut world, PLAYER, &story, 0);
    let events = use_button(&mut world, PLAYER);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::TombSeal {
            opened: false,
            opening: SiteOpening::Story,
            ..
        }
    )));
    // The arc binding's entry point opens it.
    assert!(world.unseal_tomb_site(&story, None, &mut TickOutput::default()));
    assert!(world.tombs().is_unsealed(story.id));
}

#[test]
fn every_run_of_a_site_is_fresh_but_identical_for_every_peer() {
    let site = site_of(SiteStyle::SideView);
    let mut first = world();
    let mut second = world();
    enter(&mut first, &site, &[PLAYER]);
    enter(&mut second, &site, &[PLAYER]);
    assert_eq!(first.tombs(), second.tombs());
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    // Out and back in: the next run has the next counter and another floor.
    let run = first.tomb_run_of(PLAYER).expect("run").clone();
    let floor_a = WorldState::tomb_floor_of(&run);
    first.end_tomb_run(run.id, RunOutcome::Retreated, &mut TickOutput::default());
    enter(&mut first, &site, &[PLAYER]);
    let again = first.tomb_run_of(PLAYER).expect("run").clone();
    assert_eq!(again.run, run.run + 1);
    assert_ne!(WorldState::tomb_floor_of(&again), floor_a);
}

// --- The scripted bot -----------------------------------------------------------------------

/// Plays a whole run of `site` with the bot; returns the run's events.
fn bot_plays(world: &mut WorldState, site: &TombSite) -> Vec<GameplayEventKind> {
    enter(world, site, &[PLAYER]);
    let mut bot = crate::balance::tombs::TombBot::new(PLAYER, false);
    let mut events = Vec::new();
    let rate = world.tick_rate_hz();
    let mut floor_ticks = 0;
    let mut floor = 0;
    for _ in 0..rate * 60 * 12 {
        let Some(run) = world.tomb_run_of(PLAYER) else {
            return events;
        };
        if run.floor != floor {
            floor = run.floor;
            floor_ticks = 0;
        }
        floor_ticks += 1;
        let inputs = bot.inputs(world);
        assert!(
            floor_ticks < rate * 150,
            "stuck on floor {floor} of {:?} at {:?} with {:?}\n{}",
            site.style,
            world
                .pilot_of(PLAYER)
                .map(|(_, body, _)| (body.x / TILE_SIZE, body.y / TILE_SIZE)),
            inputs.players.get(&PLAYER),
            floor_text(world, run)
        );
        events.extend(run_ticks(world, &inputs, 1));
    }
    panic!("the run never ended");
}

#[test]
fn a_scripted_bot_clears_a_side_view_tomb() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    let events = bot_plays(&mut world, &site);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::TombRunEnded {
                outcome: RunOutcome::Finished,
                ..
            }
        )),
        "{:?}",
        world.tombs().summaries.get(&PLAYER)
    );
    let summary = world.tombs().summaries.get(&PLAYER).expect("a summary");
    assert_eq!(summary.floors, site.floors());
    assert_eq!(summary.style, SiteStyle::SideView);
    assert!(
        world
            .tombs()
            .unlocks_of(PLAYER)
            .expect("unlocks")
            .cleared
            .contains(&site.id)
    );
    // Out again, in front of the door, the vault slot back to bedrock.
    assert!(world.tomb_run_of(PLAYER).is_none());
    let (x, _) = pilot_middle(&world, PLAYER);
    assert!((x / TILE_SIZE - site.door.x as f32).abs() < 6.0);
    let origin = tomb_sites::slot_origin(0);
    for dx in 0..tomb_sites::SLOT_SIZE.0 {
        assert_eq!(
            world
                .terrain()
                .tile(TilePosition {
                    x: origin.x + dx,
                    y: origin.y + 27
                })
                .map(|tile| tile.kind),
            Some(TileKind::Bedrock)
        );
    }
}

#[test]
fn a_scripted_bot_clears_a_top_down_crypt() {
    let site = site_of(SiteStyle::TopDown);
    let mut world = world();
    let events = bot_plays(&mut world, &site);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::TombRunEnded {
                outcome: RunOutcome::Finished,
                ..
            }
        )),
        "{:?}",
        world.tombs().summaries.get(&PLAYER)
    );
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::TombRelicPicked { relic: Some(_), .. }
    )));
    // The pilot walks in side view again outside.
    assert_eq!(
        world.pilot_of(PLAYER).map(|(_, _, state)| state.view),
        Some(PilotView::Side)
    );
}

#[test]
fn a_crypt_run_walks_the_pilot_top_down() {
    let site = site_of(SiteStyle::TopDown);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    assert_eq!(
        world.pilot_of(PLAYER).map(|(_, _, state)| state.view),
        Some(PilotView::TopDown)
    );
    let before = pilot_middle(&world, PLAYER);
    run_ticks(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, true)),
        20,
    );
    let after = pilot_middle(&world, PLAYER);
    assert!(
        after.1 > before.1 + 20.0,
        "down walks south: {before:?} -> {after:?}"
    );
    assert!((after.0 - before.0).abs() < 1.0);
}

// --- Banking and death ----------------------------------------------------------------------

fn loot(world: &mut WorldState, player: PlayerId, value: u32) {
    world.scenario_edit_tomb_member(player, |member| member.unbanked += value);
}

#[test]
fn a_shrine_banks_and_the_way_out_keeps_everything() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    let credits = world.players()[&PLAYER].credits;
    loot(&mut world, PLAYER, 500);
    let run = world.tomb_run_of(PLAYER).expect("run").clone();
    let shrine = run
        .rooms
        .iter()
        .find(|room| room.kind == crate::tomb::RoomKind::Shrine)
        .expect("shrine room");
    // Stand by the shrine and rest.
    let shrine_tile = (shrine.interior.min.x..=shrine.interior.max.x)
        .flat_map(|x| {
            (shrine.interior.min.y..=shrine.interior.max.y).map(move |y| TilePosition { x, y })
        })
        .find(|tile| {
            matches!(
                world.terrain().tile(*tile).map(|t| t.kind),
                Some(TileKind::Built(BuiltTile::TombShrine { .. }))
            )
        })
        .expect("the shrine");
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| {
        body.x = (shrine_tile.x as f32 - 0.5) * TILE_SIZE;
        body.y = (shrine_tile.y + 1) as f32 * TILE_SIZE - 0.25;
    });
    let events = use_button(&mut world, PLAYER);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::TombShrineRested { banked: 500, .. }
    )));
    assert_eq!(world.players()[&PLAYER].credits, credits + 500);
    let member = world.tombs().member_of(PLAYER).expect("member");
    assert_eq!((member.unbanked, member.banked), (0, 500));
    // More loot, then the first floor's way out: kept.
    loot(&mut world, PLAYER, 120);
    let start = world.tomb_run_of(PLAYER).expect("run").start;
    world.edit_pilot(id, |body, _| {
        body.x = (start.x as f32 - 2.0) * TILE_SIZE;
        body.y = (start.y + 1) as f32 * TILE_SIZE - 0.25;
    });
    assert!(matches!(
        world.pilot_usable(PLAYER),
        Some((_, Usable::TombExit))
    ));
    let mut events = use_button(&mut world, PLAYER);
    events.extend(run_ticks(&mut world, &TickInputs::default(), 1));
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::TombRunEnded {
                outcome: RunOutcome::Retreated,
                banked: 620,
                lost: 0,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(world.players()[&PLAYER].credits, credits + 620);
}

#[test]
fn dying_alone_downs_the_member_and_the_wipe_loses_unbanked_loot() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    let credits = world.players()[&PLAYER].credits;
    loot(&mut world, PLAYER, 300);
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let mut output = TickOutput::default();
    assert!(world.damage_pilot(id, 10_000.0, &mut output));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::TombMemberDowned { revived: false, .. }
    )));
    // The pilot is still there (downed, not dead), then the wipe ends the run.
    let events = run_ticks(&mut world, &TickInputs::default(), 2);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::TombRunEnded {
                outcome: RunOutcome::Wiped,
                lost: 300,
                banked: 0,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(world.players()[&PLAYER].credits, credits);
    let summary = world.tombs().summaries.get(&PLAYER).expect("summary");
    assert_eq!(summary.outcome, RunOutcome::Wiped);
    // Back outside on foot with some health, in side view.
    let (_, _, state) = world.pilot_of(PLAYER).expect("pilot");
    assert!(state.health > 0.0);
    assert_eq!(state.view, PilotView::Side);
}

#[test]
fn the_rig_waits_outside_untouched() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    let hull = world.players()[&PLAYER].hull;
    let _ = world.damage_player(PLAYER, 50.0);
    assert_eq!(world.players()[&PLAYER].hull, hull);
    // Radioing for a pickup or boarding does nothing inside.
    let events = act(&mut world, PLAYER, PlayerCommand::Rescue);
    assert!(world.tomb_run_of(PLAYER).is_some(), "{events:?}");
}

// --- Co-op ----------------------------------------------------------------------------------

#[test]
fn a_crewmate_revives_a_downed_partner_and_both_down_is_a_wipe() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER, OTHER]);
    let downed = world.pilot_of(OTHER).expect("pilot").0;
    let mut output = TickOutput::default();
    assert!(world.damage_pilot(downed, 10_000.0, &mut output));
    assert!(world.tomb_downed(OTHER));
    // Standing beside them long enough stands them up.
    let (x, y) = {
        let (_, body, _) = world.pilot_of(OTHER).expect("pilot");
        (body.x, body.y)
    };
    let helper = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(helper, |body, _| {
        body.x = x + 10.0;
        body.y = y;
    });
    let rate = world.tick_rate_hz();
    let need = (REVIVE_MILLIS as u32 * rate).div_ceil(1000);
    let events = run_ticks(&mut world, &TickInputs::default(), need + 2);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::TombMemberDowned { revived: true, player_id } if *player_id == OTHER
        )),
        "{events:?}"
    );
    assert!(!world.tomb_downed(OTHER));
    // Both down: the run ends for both.
    let mut output = TickOutput::default();
    let a = world.pilot_of(PLAYER).expect("pilot").0;
    let b = world.pilot_of(OTHER).expect("pilot").0;
    world.edit_pilot(b, |_, state| state.hurt = 0.0);
    assert!(world.damage_pilot(a, 10_000.0, &mut output));
    assert!(world.damage_pilot(b, 10_000.0, &mut output));
    let events = run_ticks(&mut world, &TickInputs::default(), 2);
    let ended = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameplayEventKind::TombRunEnded {
                    outcome: RunOutcome::Wiped,
                    ..
                }
            )
        })
        .count();
    assert_eq!(ended, 2);
}

#[test]
fn members_pick_their_own_relics_and_the_party_descends_together() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER, OTHER]);
    world.scenario_tomb_stairs(PLAYER);
    let run = world.tomb_run_of(PLAYER).expect("run").clone();
    assert_eq!(run.phase, RunPhase::Picking);
    let first = run.member(PLAYER).expect("member").offers.clone();
    let second = run.member(OTHER).expect("member").offers.clone();
    assert_eq!(first.len(), 4);
    assert_ne!(first, second, "individual picks");
    act(
        &mut world,
        PLAYER,
        PlayerCommand::Tomb(TombCommand::PickRelic(Some(1))),
    );
    assert_eq!(
        world.tomb_run_of(PLAYER).expect("run").floor,
        0,
        "waits for everyone"
    );
    act(
        &mut world,
        OTHER,
        PlayerCommand::Tomb(TombCommand::PickRelic(None)),
    );
    run_ticks(&mut world, &TickInputs::default(), 1);
    let run = world.tomb_run_of(PLAYER).expect("run");
    assert_eq!(run.floor, 1);
    assert_eq!(run.phase, RunPhase::Exploring);
    assert_eq!(run.member(PLAYER).expect("member").relics, vec![first[1]]);
    assert_eq!(run.member(OTHER).expect("member").relics, []);
    assert!(
        world
            .tombs()
            .unlocks_of(PLAYER)
            .expect("unlocks")
            .codex
            .contains(&first[1])
    );
    // Both stand at the new floor's start.
    let (a, _) = pilot_middle(&world, PLAYER);
    let (b, _) = pilot_middle(&world, OTHER);
    assert!((a - b).abs() < TILE_SIZE * 2.0);
}

#[test]
fn relics_change_the_pilots_body_and_hits() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    let max = world.pilot_of(PLAYER).expect("pilot").2.max_health;
    world.scenario_edit_tomb_member(PLAYER, |member| {
        member.relics = vec![Relic::SpringHeels, Relic::IronLungs, Relic::GliderBoost];
    });
    let (_, _, state) = world.pilot_of(PLAYER).expect("pilot");
    assert_eq!(
        state.boons.extra_jumps, 2,
        "spring heels plus the Skywalker synergy"
    );
    assert!((state.max_health - max * 1.3).abs() < 0.5);
    // A mid-air jump.
    run_ticks(&mut world, &TickInputs::default(), 30);
    let jump = TickInputs::held(PLAYER, ControlState::new(0.0, false, false).with_jump(true));
    run_ticks(&mut world, &jump, 1);
    run_ticks(&mut world, &TickInputs::default(), 18);
    let before = world.pilot_of(PLAYER).expect("pilot").1.velocity_y;
    let events = run_ticks(&mut world, &jump, 1);
    let after = world.pilot_of(PLAYER).expect("pilot").1.velocity_y;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotJumped { .. }))
    );
    assert!(after < before - 200.0, "{before} -> {after}");
    // Out of the run the boons are gone.
    let id = world.tomb_run_of(PLAYER).expect("run").id;
    world.end_tomb_run(id, RunOutcome::Retreated, &mut TickOutput::default());
    let (_, _, state) = world.pilot_of(PLAYER).expect("pilot");
    assert_eq!(state.boons, crate::pilot::PilotBoons::default());
    assert!((state.max_health - max).abs() < 0.5);
}

#[test]
fn the_guardian_waits_in_its_room_and_its_fall_opens_the_way_out() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    enter(&mut world, &site, &[PLAYER]);
    // Skip to the last floor.
    for _ in 1..site.floors() {
        world.scenario_tomb_stairs(PLAYER);
        act(
            &mut world,
            PLAYER,
            PlayerCommand::Tomb(TombCommand::PickRelic(None)),
        );
        run_ticks(&mut world, &TickInputs::default(), 1);
    }
    let run = world.tomb_run_of(PLAYER).expect("run").clone();
    assert!(run.on_last_floor());
    assert!(!run.boss_defeated);
    let boss_room = run
        .spawns
        .iter()
        .find(|spawn| spawn.role == SpawnRole::Boss)
        .map(|spawn| spawn.room)
        .expect("the guardian's room");
    let room = run.rooms[boss_room];
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| {
        body.x = (room.interior.min.x as f32 + 1.5) * TILE_SIZE;
        body.y = (room.interior.max.y + 1) as f32 * TILE_SIZE - 0.25;
    });
    let events = run_ticks(&mut world, &TickInputs::default(), 2);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombAmbush { boss: true, .. }))
    );
    let boss = world
        .tomb_run_of(PLAYER)
        .expect("run")
        .boss
        .expect("the guardian");
    // The stairs refuse while it stands.
    world.scenario_tomb_stairs(PLAYER);
    assert!(world.tomb_run_of(PLAYER).is_some());
    let _ = world.entities.despawn(crate::entity::EntityId(boss));
    let events = run_ticks(&mut world, &TickInputs::default(), 1);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::TombGuardianDefeated { .. }))
    );
    assert_eq!(
        world.tombs().unlocks_of(PLAYER).expect("unlocks").trophies,
        1
    );
    // Wave 6 trophies for the home: the guardian's mask comes with its fall, the relic plinth
    // with the first run seen through.
    let count = |world: &WorldState, piece| world.homes().furniture_count(PLAYER, piece);
    assert_eq!(count(&world, crate::home::Furniture::GuardianMask), 1);
    assert_eq!(count(&world, crate::home::Furniture::RelicPlinth), 0);
    world.scenario_tomb_stairs(PLAYER);
    assert!(world.tomb_run_of(PLAYER).is_none());
    assert_eq!(
        world
            .tombs()
            .summaries
            .get(&PLAYER)
            .map(|summary| summary.outcome),
        Some(RunOutcome::Finished)
    );
    assert_eq!(count(&world, crate::home::Furniture::RelicPlinth), 1);
}

#[test]
fn nothing_hurts_the_party_while_it_picks_relics() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER]);
    world.scenario_tomb_stairs(PLAYER);
    assert_eq!(
        world.tomb_run_of(PLAYER).map(|run| run.phase),
        Some(RunPhase::Picking)
    );
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let mut output = TickOutput::default();
    assert!(!world.damage_pilot(id, 10_000.0, &mut output));
    assert!(!world.tomb_downed(PLAYER));
    act(
        &mut world,
        PLAYER,
        PlayerCommand::Tomb(TombCommand::PickRelic(None)),
    );
    run_ticks(&mut world, &TickInputs::default(), 1);
    assert_eq!(
        world.tomb_run_of(PLAYER).map(|run| run.phase),
        Some(RunPhase::Exploring)
    );
    assert!(
        world.damage_pilot(id, 10.0, &mut output),
        "the next floor bites"
    );
}

#[test]
fn the_floor_keeps_its_creatures_and_its_guardian_however_far_the_party_walks() {
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    enter(&mut world, &site, &[PLAYER]);
    for _ in 1..site.floors() {
        world.scenario_tomb_stairs(PLAYER);
        act(
            &mut world,
            PLAYER,
            PlayerCommand::Tomb(TombCommand::PickRelic(None)),
        );
        run_ticks(&mut world, &TickInputs::default(), 1);
    }
    let run = world.tomb_run_of(PLAYER).expect("run").clone();
    let room = run.rooms[run
        .spawns
        .iter()
        .find(|spawn| spawn.role == SpawnRole::Boss)
        .map(|spawn| spawn.room)
        .expect("the guardian's room")];
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let stand = |world: &mut WorldState, x: f32| {
        world.edit_pilot(id, |body, _| {
            body.x = x;
            body.previous_x = x;
            body.y = (room.interior.max.y + 1) as f32 * TILE_SIZE - 0.25;
            body.previous_y = body.y;
        });
    };
    stand(&mut world, (room.interior.min.x as f32 + 1.5) * TILE_SIZE);
    run_ticks(&mut world, &TickInputs::default(), 2);
    let boss = world.tomb_run_of(PLAYER).and_then(|run| run.boss);
    assert!(boss.is_some(), "the guardian woke");
    let creatures = world.enemies().count();
    // Back to the start, far past the director's despawn reach, for several of its passes.
    let start = (run.start.x as f32 + 0.5) * TILE_SIZE;
    assert!((room.interior.min.x - run.start.x) as f32 > super::enemies::DESPAWN_TILES);
    for _ in 0..4 {
        stand(&mut world, start);
        run_ticks(&mut world, &TickInputs::default(), 60);
    }
    let run = world.tomb_run_of(PLAYER).expect("still in the run");
    assert!(!run.boss_defeated, "walking away fells no guardian");
    assert!(
        world
            .entities
            .contains(crate::entity::EntityId(boss.expect("boss")))
    );
    assert_eq!(world.enemies().count(), creatures, "nothing despawned");
}

// --- Saves, replication, tick rates ---------------------------------------------------------

#[test]
fn a_run_saved_mid_floor_continues_identically() {
    let site = site_of(SiteStyle::TopDown);
    let mut world = world();
    enter(&mut world, &site, &[PLAYER, OTHER]);
    loot(&mut world, PLAYER, 77);
    world.scenario_edit_tomb_member(OTHER, |member| member.relics.push(Relic::Quickstep));
    let walk = TickInputs::held(PLAYER, ControlState::new(1.0, false, true))
        .hold(OTHER, ControlState::new(1.0, true, false));
    run_ticks(&mut world, &walk, 30);
    let json = serde_json::to_string(&world).expect("save");
    let mut restored: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(restored.tombs(), world.tombs());
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let mut from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard load");
    run_ticks(&mut world, &walk, 90);
    run_ticks(&mut restored, &walk, 90);
    run_ticks(&mut from_bytes, &walk, 90);
    assert_eq!(
        world.simulation_state_hash(),
        restored.simulation_state_hash()
    );
    assert_eq!(
        world.simulation_state_hash(),
        from_bytes.simulation_state_hash()
    );
}

#[test]
fn the_tombs_section_replicates_and_is_hashed_once_used() {
    let fresh = world();
    let baseline = fresh.simulation_state_hash();
    assert!(fresh.tombs().is_empty());
    let site = site_of(SiteStyle::SideView);
    let mut world = world();
    assert_eq!(world.simulation_state_hash(), baseline);
    enter(&mut world, &site, &[PLAYER]);
    loot(&mut world, PLAYER, 40);
    let section = world.section(SectionKey::Tombs).expect("tombs section");
    let mut replica = self::world();
    replica.apply_section(SectionKey::Tombs, section);
    assert_eq!(replica.tombs(), world.tombs());
    assert!(SectionKey::SHARED.contains(&SectionKey::Tombs));
    assert!(SectionKey::Tombs.visible_to(Some(OTHER)));
}

#[test]
fn runs_are_tick_rate_safe() {
    // Revives and the walk of the top-down pilot take the same world time at every rate.
    let site = site_of(SiteStyle::TopDown);
    let mut results = Vec::new();
    for hz in [30, 60, 120] {
        let mut world = world_at(hz);
        enter(&mut world, &site, &[PLAYER]);
        let before = pilot_middle(&world, PLAYER);
        run_ticks(
            &mut world,
            &TickInputs::held(PLAYER, ControlState::new(1.0, false, false)),
            hz / 2,
        );
        let after = pilot_middle(&world, PLAYER);
        results.push(after.0 - before.0);
    }
    for walked in &results {
        assert!((walked - results[0]).abs() < 2.5, "{results:?}");
    }
}

/// A floor as text (`#` masonry, `^` traps, `|` gates, `=` lintels, `L` levers, `P` plates,
/// `S` shrine, `>` stairs, `<` way out, `$` caches), for debugging realizers.
fn floor_text(world: &WorldState, run: &crate::tomb::TombRun) -> String {
    let realized = WorldState::tomb_floor_of(run);
    let origin = tomb_sites::slot_origin(run.slot);
    let mut text = format!("{:?} start {:?}\n", run.site.style, realized.start);
    for y in 0..realized.size.1 {
        for x in 0..realized.size.0 {
            let tile = TilePosition {
                x: origin.x + x,
                y: origin.y + y,
            };
            text.push(match world.terrain().tile(tile).map(|found| found.kind) {
                Some(TileKind::Air) => '.',
                Some(TileKind::Bedrock) => ' ',
                Some(TileKind::Built(BuiltTile::TombWall(_))) => '#',
                Some(TileKind::Built(BuiltTile::TombTrap { .. })) => '^',
                Some(TileKind::Built(BuiltTile::Gate)) => '|',
                Some(TileKind::Built(BuiltTile::GateLintel(_))) => '=',
                Some(TileKind::Built(BuiltTile::Lever { .. })) => 'L',
                Some(TileKind::Built(BuiltTile::PressurePlate { .. })) => 'P',
                Some(TileKind::Built(BuiltTile::TombShrine { .. })) => 'S',
                Some(TileKind::Built(BuiltTile::TombStairs)) => '>',
                Some(TileKind::Built(BuiltTile::TombExit)) => '<',
                Some(TileKind::Built(BuiltTile::TombCache { .. })) => '$',
                _ => '?',
            });
        }
        text.push('\n');
    }
    text
}

#[test]
#[ignore = "debug: prints the first floor of each style"]
fn print_floors() {
    for style in SiteStyle::ALL {
        let site = site_of(style);
        let mut world = world();
        enter(&mut world, &site, &[PLAYER]);
        let run = world.tomb_run_of(PLAYER).expect("run").clone();
        println!("{}", floor_text(&world, &run));
    }
}
