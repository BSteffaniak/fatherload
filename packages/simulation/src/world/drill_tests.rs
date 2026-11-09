//! Drilling rules: the stock rig drills only while resting on the ground; the drill mount
//! upgrade adds sideways drilling in flight (gimbal) and drilling up into a ceiling (crown bore).

use crate::{
    ControlState, DrillDirection, GameplayEventKind, PlayerCommand, PlayerId,
    PlayerScopedCommandOutcome, SimulationParams, TickInputs, WorldState,
    economy::{UPGRADE_OFFER_COUNT, UpgradeKind, upgrade_offers},
    terrain::{DeepStratum, TILE_SIZE, TileKind, TilePosition},
    worldgen::{Band, strata::band_shallowest_row},
};

use super::{DrillContact, DrillIntent, authoritative_mine_target};

const PLAYER: PlayerId = PlayerId::new(1);
/// The shaft's column.
const SHAFT_X: i32 = 30;
/// The shaft's ceiling row (solid) and floor row (solid); air in between.
const CEILING_Y: i32 = 8;
const FLOOR_Y: i32 = 13;
/// Index of the drill mount in the shop's offers.
const DRILL_MOUNT_OFFER: usize = UPGRADE_OFFER_COUNT - 1;

/// A world at `hz` with a one-tile-wide vertical shaft of air (rows `CEILING_Y + 1 ..
/// FLOOR_Y`) in solid dirt, and a strong drill.
fn shaft_world(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    for x in SHAFT_X - 3..=SHAFT_X + 3 {
        for y in CEILING_Y - 3..=FLOOR_Y + 3 {
            let open = x == SHAFT_X && y > CEILING_Y && y < FLOOR_Y;
            let kind = if open { TileKind::Air } else { TileKind::Dirt };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let player = world.player_mut(PLAYER).expect("player");
    player.drill_strength = 5;
    player.fuel = player.fuel_capacity;
    world
}

#[allow(
    clippy::cast_precision_loss,
    reason = "small tile coordinates convert to pixels"
)]
const fn tile_px(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE
}

/// Puts the rig in the shaft with its centre at pixel row `y`, at rest.
fn place(world: &mut WorldState, y: f32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = TILE_SIZE.mul_add(0.5, tile_px(SHAFT_X));
    player.y = y;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

/// Resting on the shaft floor.
fn on_floor(world: &mut WorldState) {
    place(world, tile_px(FLOOR_Y) - 11.0);
}

/// Hanging in the middle of the shaft, touching nothing.
fn mid_air(world: &mut WorldState) {
    place(world, TILE_SIZE.mul_add(0.5, tile_px(FLOOR_Y - 2)));
}

/// Pressed against the shaft ceiling.
fn under_ceiling(world: &mut WorldState) {
    place(world, tile_px(CEILING_Y + 1) + 11.0);
}

fn set_mount(world: &mut WorldState, level: u8) {
    world.player_mut(PLAYER).expect("player").drill_mount_level = level;
}

fn target(
    world: &WorldState,
    horizontal: f32,
    drill_down: bool,
    thrust: bool,
) -> Option<(TilePosition, DrillDirection)> {
    let player = world.player(PLAYER).expect("player");
    authoritative_mine_target(
        player,
        DrillIntent {
            horizontal,
            drill_down,
            thrust,
        },
        DrillContact::of(world.terrain(), player),
    )
}

#[test]
fn contacts_follow_the_rig_position() {
    let mut world = shaft_world(60);
    on_floor(&mut world);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(
        DrillContact::of(world.terrain(), player),
        DrillContact {
            grounded: true,
            ceiling: false
        }
    );
    mid_air(&mut world);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(
        DrillContact::of(world.terrain(), player),
        DrillContact::default()
    );
    under_ceiling(&mut world);
    let player = world.player(PLAYER).expect("player");
    assert!(DrillContact::of(world.terrain(), player).ceiling);
}

#[test]
fn grounded_rig_drills_down_and_sideways() {
    let mut world = shaft_world(60);
    on_floor(&mut world);
    let row = FLOOR_Y - 1;
    assert_eq!(
        target(&world, 0.0, true, false),
        Some((
            TilePosition {
                x: SHAFT_X,
                y: FLOOR_Y
            },
            DrillDirection::Down
        ))
    );
    assert_eq!(
        target(&world, -1.0, false, false),
        Some((
            TilePosition {
                x: SHAFT_X - 1,
                y: row
            },
            DrillDirection::Left
        ))
    );
    assert_eq!(
        target(&world, 1.0, false, false),
        Some((
            TilePosition {
                x: SHAFT_X + 1,
                y: row
            },
            DrillDirection::Right
        ))
    );
}

#[test]
fn airborne_stock_rig_cannot_drill() {
    let mut world = shaft_world(60);
    mid_air(&mut world);
    assert_eq!(target(&world, -1.0, false, false), None);
    assert_eq!(target(&world, 1.0, false, true), None);
    assert_eq!(
        target(&world, 0.0, true, false),
        None,
        "down needs ground contact"
    );
    under_ceiling(&mut world);
    assert_eq!(
        target(&world, 0.0, false, true),
        None,
        "up needs the crown bore"
    );
}

#[test]
fn gimbal_mount_drills_sideways_in_flight_but_not_up() {
    let mut world = shaft_world(60);
    set_mount(&mut world, 1);
    mid_air(&mut world);
    let row = FLOOR_Y - 2;
    assert_eq!(
        target(&world, -1.0, false, true),
        Some((
            TilePosition {
                x: SHAFT_X - 1,
                y: row
            },
            DrillDirection::Left
        ))
    );
    assert_eq!(
        target(&world, 0.0, true, false),
        None,
        "down still needs ground"
    );
    under_ceiling(&mut world);
    assert_eq!(target(&world, 0.0, false, true), None);
}

#[test]
fn crown_bore_drills_up_into_a_ceiling_it_thrusts_against() {
    let mut world = shaft_world(60);
    set_mount(&mut world, 2);
    under_ceiling(&mut world);
    assert_eq!(
        target(&world, 0.0, false, true),
        Some((
            TilePosition {
                x: SHAFT_X,
                y: CEILING_Y
            },
            DrillDirection::Up
        ))
    );
    assert_eq!(
        target(&world, 0.0, false, false),
        None,
        "only while thrusting"
    );
    // Thrusting in open air drills nothing.
    mid_air(&mut world);
    assert_eq!(target(&world, 0.0, false, true), None);
}

/// Holds `controls` for up to `seconds`, returning how many drills started and whether `tile`
/// was dug out.
fn hold(
    world: &mut WorldState,
    controls: ControlState,
    seconds: u32,
    tile: TilePosition,
) -> (usize, bool) {
    let hz = world.tick_rate_hz();
    let inputs = TickInputs::held(PLAYER, controls);
    let mut started = 0;
    let mut tick = world.simulation_tick();
    for _ in 0..seconds * hz {
        let output = world.simulate_tick(tick, &inputs);
        tick = tick.next();
        started += output
            .events
            .iter()
            .filter(|event| event.kind == GameplayEventKind::DrillStarted { player_id: PLAYER })
            .count();
        if world
            .terrain()
            .tile(tile)
            .is_some_and(|tile| tile.kind == TileKind::Air)
        {
            return (started, true);
        }
    }
    (started, false)
}

#[test]
fn thrusting_into_the_ceiling_bores_upward_with_the_crown_bore_at_any_tick_rate() {
    let ceiling = TilePosition {
        x: SHAFT_X,
        y: CEILING_Y,
    };
    for hz in [30, 60, 120] {
        let mut stock = shaft_world(hz);
        under_ceiling(&mut stock);
        let (started, dug) = hold(&mut stock, ControlState::new(0.0, true, false), 4, ceiling);
        assert_eq!(
            (started, dug),
            (0, false),
            "{hz} Hz: stock rig cannot bore up"
        );

        let mut crown = shaft_world(hz);
        set_mount(&mut crown, 2);
        under_ceiling(&mut crown);
        let (started, dug) = hold(&mut crown, ControlState::new(0.0, true, false), 4, ceiling);
        assert!(started >= 1 && dug, "{hz} Hz: crown bore digs the ceiling");
        assert!(
            crown
                .active_drill(PLAYER)
                .is_none_or(|drill| drill.target != ceiling)
        );
    }
}

#[test]
fn falling_past_a_wall_only_drills_it_with_the_gimbal() {
    let mut stock = shaft_world(60);
    mid_air(&mut stock);
    let inputs = TickInputs::held(PLAYER, ControlState::new(1.0, false, false));
    let mut drilled = false;
    for _ in 0..60 {
        let tick = stock.simulation_tick();
        let _ = stock.simulate_tick(tick, &inputs);
        if let Some(drill) = stock.active_drill(PLAYER) {
            // The stock rig drops to the floor first, and only then drills the side wall.
            let row = stock
                .player(PLAYER)
                .expect("player")
                .tile_position(TILE_SIZE)
                .y;
            assert_eq!(row, FLOOR_Y - 1);
            assert_eq!(drill.target.y, FLOOR_Y - 1);
            drilled = true;
        }
    }
    assert!(drilled, "the landed rig drills the wall");

    let mut gimbal = shaft_world(60);
    set_mount(&mut gimbal, 1);
    mid_air(&mut gimbal);
    let inputs = TickInputs::held(PLAYER, ControlState::new(1.0, false, false));
    let tick = gimbal.simulation_tick();
    let _ = gimbal.simulate_tick(tick, &inputs);
    let drill = gimbal
        .active_drill(PLAYER)
        .expect("gimbal drills in flight");
    assert_eq!(drill.direction, DrillDirection::Right);
    assert!(
        drill.target.y < FLOOR_Y - 1,
        "drilling started above the floor"
    );
}

#[test]
fn the_drill_mount_is_late_game_shop_gear() {
    let mut world = shaft_world(60);
    world.player_mut(PLAYER).expect("player").credits = 100_000;
    let offer = upgrade_offers(world.player(PLAYER).expect("player"))[DRILL_MOUNT_OFFER];
    assert_eq!(offer.kind, UpgradeKind::DrillMount);
    let buy = PlayerCommand::BuyUpgrade {
        index: DRILL_MOUNT_OFFER,
    };
    assert_eq!(
        world.apply_player_command(PLAYER, &buy),
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "locked until someone reaches the deep strata"
    );
    let settings = *world.terrain().settings();
    world.progression.deepest_tile_reached =
        band_shallowest_row(&settings, Band::Deep(DeepStratum::CrystalFaults));
    assert_eq!(
        world.apply_player_command(PLAYER, &buy),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.player(PLAYER).expect("player").drill_mount_level, 1);
    assert_eq!(
        world.apply_player_command(PLAYER, &buy),
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "the crown bore needs the pressure cathedrals"
    );
    world.progression.deepest_tile_reached =
        band_shallowest_row(&settings, Band::Deep(DeepStratum::PressureCathedrals));
    assert_eq!(
        world.apply_player_command(PLAYER, &buy),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.player(PLAYER).expect("player").drill_mount_level, 2);
    assert_eq!(
        world.apply_player_command(PLAYER, &buy),
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "maxed"
    );
    assert_eq!(
        world.player(PLAYER).expect("player").credits,
        100_000 - 6_000 - 24_000
    );
}

#[test]
fn drill_mount_replicates_with_the_public_rig_build() {
    let mut authority = shaft_world(60);
    set_mount(&mut authority, 2);
    let mut replica = shaft_world(60);
    let key = crate::SectionKey::RigBuild(PLAYER);
    let payload = authority.section(key).expect("rig build");
    replica.apply_section(key, payload);
    assert_eq!(replica.player(PLAYER).expect("player").drill_mount_level, 2);
}
