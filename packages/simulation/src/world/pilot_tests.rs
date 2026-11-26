//! Part II F5, world side: exit and board, controls routed to the pilot, the pack, the hand
//! pick, throws, oxygen, damage, death and respawn, crawlways, gear, parking, saves,
//! replication and tick-rate independence.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::while_float,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use super::{WorldImport, pilot::PilotAir};
use crate::{
    ControlState, ExplosiveKind, GameplayEventKind, HazardCloud, PlayerCommand, PlayerId,
    SessionServiceRequest, SimulationParams, SimulationTick, TickInputs, TickOutput, WeaponControl,
    WorldState,
    entity::{EntityKind, EntityState},
    item::{BlockKind, ItemKind},
    pilot::{
        BOARD_REACH, PICK_SECONDS_SOFT, PILOT_STAND_HEIGHT, PilotGearKind, PilotMode, THIN_AIR_ROW,
    },
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
/// The arena's floor row (bedrock).
const FLOOR: i32 = 32;

/// An open air box (tiles 8..=40 x 8..=31) over a bedrock floor at row 32, the rig resting on
/// the floor at column 20.
fn arena_with(params: SimulationParams) -> WorldState {
    let mut world = WorldState::from_import_with_params(&WorldImport::new_game(PLAYER), params);
    fill(&mut world, 8..=40, 8..=31, TileKind::Air);
    fill(&mut world, 8..=40, FLOOR..=FLOOR, TileKind::Bedrock);
    rest(&mut world, PLAYER, 20.5);
    world
}

fn arena() -> WorldState {
    arena_with(SimulationParams::default())
}

fn fill(
    world: &mut WorldState,
    xs: std::ops::RangeInclusive<i32>,
    ys: std::ops::RangeInclusive<i32>,
    kind: TileKind,
) {
    for y in ys {
        for x in xs.clone() {
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

fn rest(world: &mut WorldState, player_id: PlayerId, tile_x: f32) {
    let player = world.player_mut(player_id).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = FLOOR as f32 * TILE_SIZE - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn tick_with(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn run(world: &mut WorldState, inputs: &TickInputs, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(tick_with(world, inputs).events.into_iter().map(|e| e.kind));
    }
    events
}

fn toggle(world: &mut WorldState) -> Vec<GameplayEventKind> {
    run(
        world,
        &TickInputs::default().act(PLAYER, PlayerCommand::TogglePilot),
        1,
    )
}

fn walk(horizontal: f32) -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(horizontal, false, false))
}

fn pilot_pos(world: &WorldState) -> (f32, f32) {
    let (_, body, _) = world.pilot_of(PLAYER).expect("on foot");
    (body.x, body.y)
}

/// Settles the rig, then climbs out.
fn out_of_the_rig(world: &mut WorldState) {
    run(world, &TickInputs::default(), 10);
    let events = toggle(world);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotExited { .. })),
        "{events:?}"
    );
    run(world, &TickInputs::default(), 20);
}

#[test]
fn climbing_out_leaves_the_rig_parked_and_controls_drive_the_pilot() {
    let mut world = arena();
    assert!(world.can_exit_rig(PLAYER));
    out_of_the_rig(&mut world);
    assert!(world.is_on_foot(PLAYER));
    let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
    assert!(state.grounded, "standing on the floor");
    assert!((body.y - FLOOR as f32 * TILE_SIZE).abs() < 1.0);
    let rig_before = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    // Thrust and drill go to the pilot now: the rig stays put.
    let start = pilot_pos(&world);
    run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(1.0, true, true)),
        60,
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!((rig.x - rig_before.0).abs() < 0.01 && (rig.y - rig_before.1).abs() < 0.5);
    assert!(
        world.active_drill(PLAYER).is_none(),
        "the parked rig never drills"
    );
    run(&mut world, &walk(1.0), 30);
    assert!(
        pilot_pos(&world).0 > start.0 + TILE_SIZE,
        "the pilot walked"
    );
    // The actor (counters, camera) is the pilot.
    let (ax, _) = world.actor_position(PLAYER).expect("actor");
    assert!((ax - pilot_pos(&world).0).abs() < 0.01);
}

#[test]
fn boarding_needs_the_pilot_beside_the_rig_and_empties_the_pack() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 90);
    assert!(!world.can_board_rig(PLAYER), "too far away");
    let events = toggle(&mut world);
    assert!(world.is_on_foot(PLAYER), "board refused: {events:?}");
    run(&mut world, &walk(-1.0), 200);
    // Back at the rig (the rig is solid: the pilot walks into it... and stands beside it).
    let rig_x = world.player(PLAYER).expect("rig").x;
    let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
    let (mx, my) = state.middle(body);
    let near = (mx - rig_x).abs() <= BOARD_REACH;
    assert!(
        near || body.x < rig_x,
        "walked back past the rig: {mx} vs {rig_x} ({my})"
    );
    // Put the pilot right beside the rig with some loot in the pack.
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| body.x = rig_x - 20.0);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .pilot_pack
        .add(ItemKind::Mineral(MineralKind::Gold), 3);
    assert!(world.can_board_rig(PLAYER));
    let events = toggle(&mut world);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotBoarded { transferred: 3, .. }
    )));
    assert!(!world.is_on_foot(PLAYER));
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.pilot_pack.is_empty());
    assert_eq!(rig.cargo_bay.count(ItemKind::Mineral(MineralKind::Gold)), 3);
}

#[test]
fn a_moving_rig_cannot_be_left() {
    let mut world = arena();
    world.player_mut(PLAYER).expect("rig").velocity_x = 200.0;
    world.player_mut(PLAYER).expect("rig").y -= TILE_SIZE * 4.0;
    assert!(!world.can_exit_rig(PLAYER));
    toggle(&mut world);
    assert!(!world.is_on_foot(PLAYER));
}

#[test]
fn the_pack_swaps_with_the_rig_by_hand() {
    let mut world = arena();
    world
        .player_mut(PLAYER)
        .expect("rig")
        .block_hopper
        .add(ItemKind::Block(BlockKind::Dirt), 20);
    let take = PlayerCommand::PilotPack {
        item: ItemKind::Block(BlockKind::Dirt),
        count: 50,
        to_rig: false,
    };
    let events = run(&mut world, &TickInputs::default().act(PLAYER, take), 1);
    let rig = world.player(PLAYER).expect("rig");
    let capacity = rig.pilot_gear.pack_capacity();
    assert_eq!(rig.pilot_pack.total(), capacity, "the pack fills up");
    assert_eq!(
        rig.block_hopper.count(ItemKind::Block(BlockKind::Dirt)),
        20 - capacity
    );
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PackTransferred { to_rig: false, .. }
    )));
    let back = PlayerCommand::PilotPack {
        item: ItemKind::Block(BlockKind::Dirt),
        count: 2,
        to_rig: true,
    };
    run(&mut world, &TickInputs::default().act(PLAYER, back), 1);
    assert_eq!(
        world.player(PLAYER).expect("rig").pilot_pack.total(),
        capacity - 2
    );
}

#[test]
fn the_hand_pick_chips_soft_sky_soil_but_not_sky_rock() {
    use crate::sky::{SkyBiome, SkyTile};
    let mut world = arena();
    let soil = TileKind::Sky(SkyTile::Soil);
    fill(&mut world, 24..=25, 28..=31, soil);
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 120);
    let swing = TickInputs::held(
        PLAYER,
        ControlState::new(0.0, false, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: false,
            aim_x: 100,
            aim_y: 0,
        }),
    );
    let target = {
        let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
        world
            .pick_target(body, state, swing.players[&PLAYER].controls.expect("held"))
            .expect("the wall is in reach")
    };
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(soil)
    );
    let ticks = (PICK_SECONDS_SOFT * 60.0) as u32 + 4;
    let events = run(&mut world, &swing, ticks);
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(TileKind::Air),
        "the pick broke the sky soil"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PickStruck { broke: true, .. }))
    );
    // Island rock only clinks.
    let rock = TileKind::Sky(SkyTile::Rock(SkyBiome::Meadow));
    let _ = world.terrain_mut().set_kind(target, rock);
    let events = run(&mut world, &swing, 90);
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(rock)
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PickClinked { .. }))
    );
}

#[test]
fn the_hand_pick_chips_soft_dirt_but_not_stone() {
    let mut world = arena();
    // A dirt wall right of the rig, a stone block above it.
    fill(&mut world, 24..=25, 28..=31, TileKind::Dirt);
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: 24, y: 27 }, TileKind::Stone);
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 120);
    let swing = TickInputs::held(
        PLAYER,
        ControlState::new(0.0, false, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: false,
            aim_x: 100,
            aim_y: 0,
        }),
    );
    let target = {
        let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
        world
            .pick_target(body, state, swing.players[&PLAYER].controls.expect("held"))
            .expect("the wall is in reach")
    };
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(TileKind::Dirt)
    );
    let ticks = (PICK_SECONDS_SOFT * 60.0) as u32 + 4;
    let events = run(&mut world, &swing, ticks);
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(TileKind::Air),
        "the pick broke the dirt"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PickStruck { broke: true, .. }))
    );
    // Every broken tile drops: the dirt block is loose or already in the pack.
    run(&mut world, &TickInputs::default(), 30);
    let in_pack = world
        .player(PLAYER)
        .expect("rig")
        .pilot_pack
        .count(ItemKind::Block(BlockKind::Dirt));
    let loose = world
        .drops()
        .filter(|(_, _, drop, _)| drop.stack.kind == ItemKind::Block(BlockKind::Dirt))
        .count();
    assert!(in_pack + u32::try_from(loose).unwrap_or(0) >= 1);
    // Stone only clinks.
    let _ = world.terrain_mut().set_kind(target, TileKind::Stone);
    let events = run(&mut world, &swing, 90);
    assert_eq!(
        world.terrain().tile(target).map(|tile| tile.kind),
        Some(TileKind::Stone)
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PickClinked { .. }))
    );
}

#[test]
fn bombs_are_thrown_from_the_pilots_hands() {
    let mut world = arena();
    world.player_mut(PLAYER).expect("rig").bombs = 2;
    out_of_the_rig(&mut world);
    let (px, py) = pilot_pos(&world);
    run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::PlaceBomb),
        1,
    );
    assert_eq!(world.player(PLAYER).expect("rig").bombs, 1);
    let bomb = world.bombs().into_iter().next().expect("a live bomb");
    assert!(
        (bomb.x - px).abs() < TILE_SIZE && bomb.y < py,
        "from the hands"
    );
    assert!(bomb.velocity_y < 0.0, "lobbed up and out");
    // The throw cooldown stops a second bomb on the very next tick.
    run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::PlaceBomb),
        1,
    );
    assert_eq!(world.player(PLAYER).expect("rig").bombs, 1);
    // Standing on top of it hurts (blasts reach pilots): follow the bomb until it goes off.
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let mut events = Vec::new();
    for _ in 0..240 {
        if let Some(bomb) = world.bombs().into_iter().next() {
            world.edit_pilot(id, |body, _| body.x = bomb.x);
        }
        events.extend(
            tick_with(&mut world, &TickInputs::default())
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotHurt { .. })),
        "{events:?}"
    );
}

#[test]
fn oxygen_runs_out_in_gas_and_comes_back_in_air() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    let (x, y) = pilot_pos(&world);
    assert_eq!(world.pilot_air_at(x, y - 40.0), PilotAir::Breathable);
    // The default (V8) sky decides by air density: still breathable just over the peaks.
    assert_eq!(
        world.pilot_air_at(x, (THIN_AIR_ROW - 4) as f32 * TILE_SIZE),
        PilotAir::Breathable,
        "V8 air is full up to 200 rows"
    );
    assert_eq!(
        world.pilot_air_at(x, -2_000.0 * TILE_SIZE),
        PilotAir::Thin,
        "too high to breathe"
    );
    let _ = world.spawn_gas_cloud(HazardCloud {
        x,
        y: y - 40.0,
        life: 5.0,
        radius: 200.0,
    });
    let max = world.pilot_of(PLAYER).expect("pilot").2.max_oxygen;
    let ticks = (max * 60.0) as u32 + 120;
    let mut events = Vec::new();
    for _ in 0..ticks {
        // Keep the cloud alive.
        let clouds: Vec<_> = world.entities().ids_of(EntityKind::GasCloud).collect();
        for id in clouds {
            world.entities.update(id, |entity| {
                if let EntityState::GasCloud(cloud) = &mut entity.state {
                    cloud.life = 5.0;
                    cloud.radius = 200.0;
                }
            });
        }
        events.extend(
            tick_with(&mut world, &TickInputs::default())
                .events
                .into_iter()
                .map(|event| event.kind),
        );
    }
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotOxygenLow { empty: false, .. }
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotOxygenLow { empty: true, .. }))
    );
    let state = *world.pilot_of(PLAYER).expect("pilot").2;
    assert!(state.oxygen <= 0.0);
    assert!(state.health < state.max_health, "suffocating hurts");
    // Clear air: oxygen refills.
    let ids: Vec<_> = world.entities().ids_of(EntityKind::GasCloud).collect();
    for id in ids {
        let _ = world.entities.despawn(id);
    }
    run(&mut world, &TickInputs::default(), 60);
    assert!(world.pilot_of(PLAYER).expect("pilot").2.oxygen > 1.0);
}

#[test]
fn a_dead_pilot_drops_part_of_its_pack_and_respawns_in_the_rig() {
    let mut world = arena();
    world
        .player_mut(PLAYER)
        .expect("rig")
        .pilot_pack
        .add(ItemKind::Mineral(MineralKind::Silver), 4);
    out_of_the_rig(&mut world);
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let mut output = TickOutput::default();
    assert!(world.damage_pilot(id, 1_000.0, &mut output));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::PilotDied { .. }))
    );
    assert!(!world.is_on_foot(PLAYER), "back in the rig");
    let rig = world.player(PLAYER).expect("rig");
    // What the pack kept goes aboard with the respawned pilot, as when boarding.
    assert!(rig.pilot_pack.is_empty());
    assert_eq!(
        rig.cargo_bay.count(ItemKind::Mineral(MineralKind::Silver)),
        2
    );
    let dropped: u32 = world
        .drops()
        .filter(|(_, _, drop, _)| drop.stack.kind == ItemKind::Mineral(MineralKind::Silver))
        .map(|(_, _, drop, _)| drop.stack.count)
        .sum();
    assert_eq!(dropped, 2, "half the stack is left where it died");
}

#[test]
fn falls_and_enemies_hurt_but_god_mode_does_not() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    // Drop it from ten tiles up.
    world.edit_pilot(id, |body, state| {
        body.y -= 10.0 * TILE_SIZE;
        state.grounded = false;
    });
    let events = run(&mut world, &TickInputs::default(), 120);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotLanded { hard: true, .. }))
    );
    let after_fall = world.pilot_of(PLAYER).expect("pilot").2.health;
    assert!(after_fall < world.pilot_of(PLAYER).expect("pilot").2.max_health);
    // An enemy strike knocks it back and starts the guard.
    let (x, y) = pilot_pos(&world);
    let mut output = TickOutput::default();
    world.set_enemy_difficulty(crate::EnemyDifficulty::Normal);
    let before = world.pilot_of(PLAYER).expect("pilot").2.health;
    assert!(world.strike_pilot(id, 10.0, (x - 30.0, y - 20.0), 200.0, &mut output));
    let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
    assert!(state.health < before && state.hurt > 0.0);
    assert!(body.velocity_x > 0.0, "knocked away from the hit");
    assert!(
        !world.strike_pilot(id, 10.0, (x - 30.0, y - 20.0), 200.0, &mut output),
        "the strike guard"
    );
    world.test_mode = Some(crate::TestModeState {
        invulnerable: true,
        infinite_fuel: false,
        no_clip: false,
        ..crate::TestModeState::default()
    });
    let mut output = TickOutput::default();
    assert!(!world.damage_pilot(id, 50.0, &mut output), "god mode");
}

#[test]
fn a_crawlway_lets_pilots_through_but_stops_rigs() {
    let mut world = arena();
    assert!(
        crate::poi::CRAWLWAY_ACCESS.admits_pilot() && !crate::poi::CRAWLWAY_ACCESS.admits_rig()
    );
    assert_eq!(
        crate::base::BuiltTile::VentGrate.access(),
        Some(crate::poi::SpaceAccess::RequiresPilot)
    );
    assert!(
        crate::poi::PoiKind::ALL
            .iter()
            .all(|kind| kind.access().admits_rig())
    );
    // A wall of stone with a 1-tile-high crawlway through it at the floor.
    fill(&mut world, 26..=30, 8..=31, TileKind::Stone);
    let written = world.carve_crawlway(TilePosition { x: 26, y: 31 }, 5);
    assert_eq!(written.len(), 5);
    assert!(
        world.terrain().is_solid_at(TilePosition { x: 26, y: 31 }),
        "the grate is solid terrain"
    );
    out_of_the_rig(&mut world);
    let crawl = TickInputs::held(PLAYER, ControlState::new(1.0, false, true));
    run(&mut world, &crawl, 60 * 8);
    assert!(
        pilot_pos(&world).0 > 31.0 * TILE_SIZE,
        "crawled through: {}",
        pilot_pos(&world).0 / TILE_SIZE
    );
    // Board again isn't possible from here; the rig can't follow either.
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    let rig_x = world.player(PLAYER).expect("rig").x;
    world.edit_pilot(id, |body, _| body.x = rig_x - 20.0);
    toggle(&mut world);
    run(&mut world, &walk(1.0), 60 * 6);
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.x < 26.0 * TILE_SIZE,
        "the grate stops the rig: {}",
        rig.x / TILE_SIZE
    );
}

#[test]
fn pilot_gear_is_sold_at_the_explosives_shack() {
    let mut world = WorldState::new_game(PLAYER);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    let buy = SessionServiceRequest::BuyPilotGear {
        gear: PilotGearKind::Jetpack,
    };
    // Bought at the shack counter.
    let shop = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Explosives)
        .copied()
        .expect("shack");
    let player = world.player_mut(PLAYER).expect("rig");
    player.x = (shop.tile_x as f32 + 0.5 * shop.tile_width as f32) * TILE_SIZE;
    run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::Service(buy)),
        1,
    );
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.pilot_gear.jetpack, 1);
    assert!(rig.credits < 100_000);
}

#[test]
fn parking_a_player_on_foot_parks_the_pilot_too() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 40);
    let before = pilot_pos(&world);
    assert!(world.park_player(PLAYER));
    assert_eq!(world.entities().count_of(EntityKind::Pilot), 0);
    let json = serde_json::to_string(&world).expect("save");
    let mut loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert!(loaded.unpark_player(PLAYER));
    assert!(loaded.is_on_foot(PLAYER));
    let after = pilot_pos(&loaded);
    assert!((before.0 - after.0).abs() < 0.01 && (before.1 - after.1).abs() < 0.01);
}

#[test]
fn a_world_with_a_pilot_out_saves_and_keeps_playing_identically() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 30);
    let json = serde_json::to_string(&world).expect("json");
    let mut restored: WorldState = serde_json::from_str(&json).expect("restore");
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(
        from_bytes.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let script = TickInputs::held(
        PLAYER,
        ControlState::new(-1.0, false, false).with_jump(true),
    );
    for _ in 0..120 {
        let tick = world.simulation_tick();
        let a = world.simulate_tick(tick, &script);
        let b = restored.simulate_tick(SimulationTick::new(tick.get()), &script);
        assert_eq!(a, b);
    }
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash()
    );
}

#[test]
fn the_pilot_replicates_to_other_clients_and_predicts_identically() {
    let mut host = arena();
    host.add_player(OTHER);
    out_of_the_rig(&mut host);
    // A client keyframe (redacted for the other player) carries the pilot entity.
    let mut client = host.redacted_for(Some(OTHER));
    assert!(client.is_on_foot(PLAYER), "everyone sees the pilot");
    assert_eq!(
        client.visible_state_hash(Some(OTHER)),
        host.visible_state_hash(Some(OTHER))
    );
    // The host moves the pilot; entity deltas bring the client along.
    let script = walk(1.0);
    let before: Vec<(crate::entity::EntityId, u64)> = client.entity_revisions(None, None);
    run(&mut host, &script, 20);
    let changed: Vec<crate::entity::Entity> = host
        .entity_revisions(None, None)
        .into_iter()
        .filter(|record| !before.contains(record))
        .filter_map(|(id, _)| host.entity_record(id))
        .collect();
    assert!(
        changed
            .iter()
            .any(|entity| entity.kind() == EntityKind::Pilot),
        "the moving pilot is in the delta"
    );
    client.apply_entity_delta(crate::entity::EntityDelta {
        updates: changed,
        despawned: Vec::new(),
    });
    assert_eq!(pilot_pos(&client), pilot_pos(&host));
    // Prediction: the owner's copy running the same inputs lands on the same spot.
    let mut predicted = host.clone();
    run(&mut host, &script, 30);
    run(&mut predicted, &script, 30);
    assert_eq!(pilot_pos(&predicted), pilot_pos(&host));
    assert_eq!(
        predicted.simulation_state_hash(),
        host.simulation_state_hash()
    );
}

#[test]
fn walking_and_jumping_on_foot_is_tick_rate_independent() {
    let mut results = Vec::new();
    for hz in [30, 60, 120] {
        let mut world = arena_with(SimulationParams::new(hz));
        // Whole numbers of 30 Hz ticks, so every rate runs exactly the same time.
        let ticks = |seconds: f32| (seconds * hz as f32).round() as u32;
        run(&mut world, &TickInputs::default(), ticks(0.3));
        toggle(&mut world);
        run(&mut world, &TickInputs::default(), ticks(0.5) - 1);
        run(&mut world, &walk(1.0), ticks(0.7));
        run(
            &mut world,
            &TickInputs::held(PLAYER, ControlState::new(1.0, false, false).with_jump(true)),
            ticks(0.5),
        );
        run(&mut world, &walk(-0.5), ticks(0.5));
        run(&mut world, &TickInputs::default(), ticks(0.5));
        let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
        results.push((hz, body.x, body.y, state.mode));
    }
    let (_, x, y, mode) = results[0];
    for (hz, other_x, other_y, other_mode) in &results[1..] {
        assert!(
            (x - other_x).abs() < 0.05 && (y - other_y).abs() < 0.05,
            "{hz} Hz: ({other_x}, {other_y}) vs ({x}, {y})"
        );
        assert_eq!(mode, *other_mode);
    }
    assert_eq!(mode, PilotMode::Free);
    let _ = (ExplosiveKind::Bomb, PILOT_STAND_HEIGHT);
}

#[test]
fn radioing_for_a_pickup_hauls_the_pilot_back_aboard() {
    let mut world = arena();
    world.player_mut(PLAYER).expect("rig").credits = 100;
    out_of_the_rig(&mut world);
    run(&mut world, &walk(1.0), 90);
    assert!(!world.can_board_rig(PLAYER), "walked away");
    world
        .player_mut(PLAYER)
        .expect("rig")
        .pilot_pack
        .add(ItemKind::Mineral(MineralKind::Gold), 2);
    let events = run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::Rescue),
        1,
    );
    assert!(!world.is_on_foot(PLAYER), "hauled aboard: {events:?}");
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotBoarded { transferred: 2, .. }
    )));
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.credits, 100 - crate::pilot::PILOT_RECALL_FEE);
    assert_eq!(rig.cargo_bay.count(ItemKind::Mineral(MineralKind::Gold)), 2);
    // A wrecked rig still takes its pilot back (its own rescue follows from the rig).
    out_of_the_rig(&mut world);
    world.player_mut(PLAYER).expect("rig").hull = 0.0;
    assert!(!world.can_board_rig(PLAYER));
    run(
        &mut world,
        &TickInputs::default().act(PLAYER, PlayerCommand::Rescue),
        1,
    );
    assert!(!world.is_on_foot(PLAYER));
    assert!(
        world.failure_state(PLAYER).is_some(),
        "the rig's rescue is next"
    );
}

#[test]
fn rig_services_on_foot_need_the_rig_parked_outside() {
    let mut world = WorldState::new_game(PLAYER);
    let depot = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Depot)
        .copied()
        .expect("depot");
    let door_x = (depot.tile_x as f32 + 0.5 * depot.tile_width as f32) * TILE_SIZE;
    let rig_y = world.player(PLAYER).expect("rig").y;
    world.player_mut(PLAYER).expect("rig").x = door_x;
    world
        .place_pilot(PLAYER, door_x, rig_y + 11.0)
        .expect("pilot");
    let sell = PlayerCommand::SellCargo;
    let contract = PlayerCommand::Service(SessionServiceRequest::CompleteDepotWork);
    assert!(world.command_allowed_here(PLAYER, &sell), "rig outside");
    assert!(world.command_allowed_here(PLAYER, &contract));
    // The rig parked far away: the pilot can still do business, but not sell the rig's cargo.
    world.player_mut(PLAYER).expect("rig").x = door_x + TILE_SIZE * 40.0;
    assert!(!world.rig_within_service_reach(PLAYER));
    assert!(!world.command_allowed_here(PLAYER, &sell));
    assert!(!world.command_allowed_here(
        PLAYER,
        &PlayerCommand::Service(SessionServiceRequest::SellCargo)
    ));
    assert!(world.command_allowed_here(PLAYER, &contract));
}

/// A V8 world with the rig parked at the arena's floor and its pilot out, then lifted into open
/// sky `altitude_rows` up.
fn pilot_in_the_sky(settings: &crate::worldgen::WorldSettings, altitude_rows: f32) -> WorldState {
    let mut world = WorldState::new_game_with_settings(PLAYER, settings, SimulationParams::new(60));
    fill(&mut world, 8..=40, 8..=31, TileKind::Air);
    fill(&mut world, 8..=40, FLOOR..=FLOOR, TileKind::Bedrock);
    rest(&mut world, PLAYER, 20.5);
    out_of_the_rig(&mut world);
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| {
        body.y = -altitude_rows * TILE_SIZE;
        body.previous_y = body.y;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
    });
    world
}

#[test]
fn the_sky_thins_the_pilots_air_and_lightens_its_fall() {
    use crate::worldgen::{GeneratorVersion, WorldSettings, sky};
    let settings = WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(0x0E00_0015)
    };
    // Low isles breathe; the high archipelago, orbit and the Moon do not.
    let world = pilot_in_the_sky(&settings, 300.0);
    let x = 20.5 * TILE_SIZE;
    assert_eq!(
        world.pilot_air_at(x, -300.0 * TILE_SIZE),
        PilotAir::Breathable
    );
    assert_eq!(world.pilot_air_at(x, -1_200.0 * TILE_SIZE), PilotAir::Thin);
    let moon = sky::moon(&settings).expect("a V8 Moon");
    let moon_x = (moon.center_x as f32 + 0.5) * TILE_SIZE;
    let moon_top =
        moon.surface_row(moon.center_x).expect("the Moon has a top") as f32 * TILE_SIZE - 40.0;
    assert_eq!(
        world.pilot_air_at(moon_x, moon_top),
        PilotAir::Thin,
        "no air on the Moon"
    );

    // Free fall for half a second: gravity at 300 rows is the ground's; orbit's is a sliver.
    let fall = |altitude: f32| {
        let mut world = pilot_in_the_sky(&settings, altitude);
        let start = pilot_pos(&world).1;
        run(&mut world, &TickInputs::default(), 30);
        pilot_pos(&world).1 - start
    };
    let low = fall(300.0);
    let orbit = fall(sky::ORBIT_BOTTOM_ROWS as f32 + 300.0);
    assert!(low > 40.0, "falls at full gravity low down: {low}");
    assert!(
        orbit > 0.0 && orbit < low * 0.1,
        "floats in orbit: {orbit} vs {low}"
    );
}
