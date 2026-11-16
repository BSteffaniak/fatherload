//! WP-PILOT, world side: personal arms (shots, ammo, melee, swaps, the gunsmith), gear
//! (hover, glider, boots, grapple, winch, armour, cold), ruin mechanisms (levers, plates,
//! keycard doors, chests), the rig-and-pilot combos (winch, the rig on a plate, recall, the
//! sentry) and saves / replication of all of it.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    reason = "test fixtures convert small tile, tick and pixel numbers freely"
)]

use super::WorldImport;
use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SessionServiceRequest,
    SimulationParams, SimulationTick, TickInputs, TickOutput, WeaponControl, WeaponKind,
    WorldState,
    base::BuiltTile,
    entity::{EntityKind, EntityState},
    mechanism::{MechanismEvent, keycard_item},
    pilot::{
        PilotAction, PilotGear, PilotMode, RecallOutcome, RopeEvent, TetherKind,
        arsenal::PilotArmsPurchase,
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
    weapon::AmmoKind,
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
/// The arena's floor row (bedrock).
const FLOOR: i32 = 32;

fn arena() -> WorldState {
    let mut world = WorldState::from_import_with_params(
        &WorldImport::new_game(PLAYER),
        SimulationParams::default(),
    );
    fill(&mut world, 8..=60, 8..=31, TileKind::Air);
    // Stone, not bedrock: ruin plates and shafts are cut into it.
    fill(&mut world, 8..=60, FLOOR..=FLOOR + 14, TileKind::Stone);
    rest(&mut world, 20.5);
    world
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

fn rest(world: &mut WorldState, tile_x: f32) {
    let player = world.player_mut(PLAYER).expect("player");
    player.x = tile_x * TILE_SIZE;
    player.y = FLOOR as f32 * TILE_SIZE - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn run(world: &mut WorldState, inputs: &TickInputs, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let output: TickOutput = world.simulate_tick(tick, inputs);
        events.extend(output.events.into_iter().map(|event| event.kind));
    }
    events
}

fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    run(world, &TickInputs::default().act(PLAYER, command), 1)
}

fn pilot_action(world: &mut WorldState, action: PilotAction) -> Vec<GameplayEventKind> {
    act(world, PlayerCommand::Pilot(action))
}

fn held(horizontal: f32, up: bool, down: bool) -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(horizontal, up, down))
}

fn firing(aim: (i8, i8)) -> TickInputs {
    TickInputs::held(
        PLAYER,
        ControlState::new(0.0, false, false).with_weapon(WeaponControl {
            fire: true,
            auto_aim: false,
            aim_x: aim.0,
            aim_y: aim.1,
        }),
    )
}

fn out_of_the_rig(world: &mut WorldState) {
    run(world, &TickInputs::default(), 10);
    let events = act(world, PlayerCommand::TogglePilot);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotExited { .. })),
        "{events:?}"
    );
    run(world, &TickInputs::default(), 20);
}

fn pilot_pos(world: &WorldState) -> (f32, f32) {
    let (_, body, _) = world.pilot_of(PLAYER).expect("on foot");
    (body.x, body.y)
}

fn pilot_state(world: &WorldState) -> crate::pilot::PilotState {
    *world.pilot_of(PLAYER).expect("on foot").2
}

/// Puts the pilot (already out) standing at tile column `tile_x` on the arena floor.
fn stand_at(world: &mut WorldState, tile_x: f32) {
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, state| {
        body.x = tile_x * TILE_SIZE;
        body.y = FLOOR as f32 * TILE_SIZE - 0.25;
        body.previous_x = body.x;
        body.previous_y = body.y;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        state.mode = PilotMode::Free;
        state.grounded = true;
    });
    run(world, &TickInputs::default(), 2);
}

fn gear(world: &mut WorldState, change: impl FnOnce(&mut PilotGear)) {
    let player = world.player_mut(PLAYER).expect("rig");
    change(&mut player.pilot_gear);
    let gear = player.pilot_gear;
    if let Some((id, _, _)) = world.pilot_of(PLAYER) {
        world.edit_pilot(id, |_, state| {
            *state = crate::pilot::PilotState {
                gear,
                jetpack_capacity: gear.jetpack_seconds(),
                jetpack_fuel: gear.jetpack_seconds(),
                ..*state
            };
        });
    }
}

fn target_health(world: &WorldState, id: crate::entity::EntityId) -> Option<f32> {
    world
        .entities()
        .get(id)
        .and_then(|entity| entity.state.health())
        .map(|health| health.current)
}

fn tile(world: &WorldState, x: i32, y: i32) -> TileKind {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .expect("tile")
        .kind
}

// --- Personal arms -------------------------------------------------------------------------

#[test]
fn personal_guns_fire_from_the_hand_spend_their_ammo_and_hurt_creatures() {
    let mut world = arena();
    {
        let player = world.player_mut(PLAYER).expect("rig");
        assert!(player.pilot_arsenal.receive(WeaponKind::Sidearm));
        assert!(player.pilot_arsenal.receive(WeaponKind::Rifle));
        let _ = player.pilot_arsenal.add_ammo(AmmoKind::Cartridge, 3);
    }
    out_of_the_rig(&mut world);
    assert_eq!(
        pilot_state(&world).weapon,
        Some(WeaponKind::Sidearm),
        "the first gun bought is in hand when the pilot climbs out"
    );
    stand_at(&mut world, 14.0);
    let (x, y) = pilot_pos(&world);
    let target = world.spawn_target_dummy(x + 6.0 * TILE_SIZE, y - 32.0, 500.0, 0.0);
    let events = run(&mut world, &firing((127, 0)), 90);
    let shots = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameplayEventKind::PilotFired {
                    weapon: WeaponKind::Sidearm,
                    ..
                }
            )
        })
        .count();
    assert!(
        (3..=6).contains(&shots),
        "the sidearm repeats at its cooldown: {shots}"
    );
    let after_sidearm = target_health(&world, target).expect("target");
    assert!(after_sidearm < 500.0, "sidearm rounds landed");
    // The rifle spends cartridges, then clicks dry.
    let events = pilot_action(&mut world, PilotAction::CycleWeapon);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotWeaponSwapped {
            weapon: Some(WeaponKind::Rifle),
            ..
        }
    )));
    assert_eq!(pilot_state(&world).weapon, Some(WeaponKind::Rifle));
    let events = run(&mut world, &firing((127, 0)), 150);
    let rifle_shots = events
        .iter()
        .filter(|event| matches!(event, GameplayEventKind::PilotFired { .. }))
        .count();
    assert_eq!(rifle_shots, 3, "one shot per cartridge");
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .pilot_arsenal
            .ammo(AmmoKind::Cartridge),
        0
    );
    // A fresh pull with no cartridges clicks.
    run(&mut world, &TickInputs::default(), 2);
    let events = run(&mut world, &firing((127, 0)), 2);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotWeaponDry { .. }))
    );
    assert!(target_health(&world, target).is_none_or(|health| health < after_sidearm));
    // The quick swap comes round to the hand pick.
    pilot_action(&mut world, PilotAction::CycleWeapon);
    assert_eq!(pilot_state(&world).weapon, None);
}

#[test]
fn the_wrench_and_the_pick_swing_at_creatures_in_reach() {
    let mut world = arena();
    let _ = world
        .player_mut(PLAYER)
        .expect("rig")
        .pilot_arsenal
        .receive(WeaponKind::Wrench);
    out_of_the_rig(&mut world);
    stand_at(&mut world, 14.0);
    let (x, y) = pilot_pos(&world);
    let target = world.spawn_target_dummy(x + TILE_SIZE, y - 26.0, 200.0, 0.0);
    let events = run(&mut world, &firing((127, 0)), 30);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotFired {
            weapon: WeaponKind::Wrench,
            ..
        }
    )));
    let after_wrench = target_health(&world, target).expect("target");
    assert!(after_wrench < 200.0, "the wrench connects");
    // The bare pick swings at the creature instead of the floor.
    pilot_action(&mut world, PilotAction::Equip(None));
    run(&mut world, &TickInputs::default(), 30);
    let events = run(&mut world, &firing((127, 0)), 30);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotFired {
            weapon: WeaponKind::HandPick,
            ..
        }
    )));
    assert!(target_health(&world, target).expect("target") < after_wrench);
    // An unowned weapon cannot be equipped.
    let events = pilot_action(&mut world, PilotAction::Equip(Some(WeaponKind::Shotgun)));
    assert!(events.is_empty() || pilot_state(&world).weapon.is_none());
}

#[test]
fn the_pilot_is_weaker_than_the_rig_in_raw_firepower() {
    let rig = WeaponKind::RivetGun.stats(0);
    let rig_dps = rig.damage * 1_000.0 / rig.cooldown_millis as f32;
    for weapon in crate::pilot::arsenal::PILOT_WEAPONS {
        let stats = weapon.stats(0);
        let dps =
            stats.damage * f32::from(stats.pellets.max(1)) * 1_000.0 / stats.cooldown_millis as f32;
        assert!(dps < rig_dps, "{weapon:?}: {dps} vs the rig's {rig_dps}");
    }
    assert!(PilotGear::default().max_health() < 200.0);
}

#[test]
fn the_town_gunsmith_and_village_armories_sell_personal_arms_and_ammo() {
    let mut world = WorldState::new_game(PLAYER);
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    let shop = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Explosives)
        .copied()
        .expect("shack");
    world.player_mut(PLAYER).expect("rig").x =
        (shop.tile_x as f32 + 0.5 * shop.tile_width as f32) * TILE_SIZE;
    for purchase in PilotArmsPurchase::TOWN {
        act(
            &mut world,
            PlayerCommand::Service(SessionServiceRequest::PilotArms(purchase)),
        );
    }
    let player = world.player(PLAYER).expect("rig");
    assert_eq!(player.pilot_arsenal.owned.len(), 4);
    assert_eq!(player.pilot_arsenal.equipped, Some(WeaponKind::Wrench));
    assert_eq!(
        player.pilot_arsenal.ammo(AmmoKind::Shell),
        AmmoKind::Shell.pack().0
    );
    assert!(player.credits < 100_000);
    // Buying a weapon twice is refused (and free).
    let credits = player.credits;
    act(
        &mut world,
        PlayerCommand::Service(SessionServiceRequest::PilotArms(PilotArmsPurchase::Weapon(
            WeaponKind::Rifle,
        ))),
    );
    assert_eq!(world.player(PLAYER).expect("rig").credits, credits);
    // Every gear kind's tiers are for sale up to the top tier.
    for kind in crate::pilot::PilotGearKind::ALL {
        for tier in 0..kind.max_tier() {
            assert!(kind.price(tier).is_some(), "{kind:?} {tier}");
        }
        assert!(kind.price(kind.max_tier()).is_none());
    }
}

#[test]
fn the_thermal_lining_is_not_sold_on_a_world_without_a_sky() {
    use crate::{
        pilot::PilotGearKind,
        worldgen::{GeneratorVersion, WorldSettings},
    };
    let buy = |generator: GeneratorVersion, gear: PilotGearKind| {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(0x0E00_0023)
        };
        let mut world =
            WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(60));
        let shop = crate::surface::SURFACE_BUILDINGS
            .iter()
            .find(|building| building.zone == crate::economy::SurfaceZone::Explosives)
            .copied()
            .expect("shack");
        let player = world.player_mut(PLAYER).expect("rig");
        player.credits = 100_000;
        player.x = (shop.tile_x as f32 + 0.5 * shop.tile_width as f32) * TILE_SIZE;
        act(
            &mut world,
            PlayerCommand::Service(SessionServiceRequest::BuyPilotGear { gear }),
        );
        world.player(PLAYER).expect("rig").pilot_gear
    };
    assert_eq!(buy(GeneratorVersion::V7, PilotGearKind::Thermal).thermal, 0);
    assert_eq!(buy(GeneratorVersion::V7, PilotGearKind::Armor).armor, 1);
    assert_eq!(buy(GeneratorVersion::V9, PilotGearKind::Thermal).thermal, 1);
}

// --- Gear ----------------------------------------------------------------------------------

#[test]
fn a_hover_jetpack_holds_altitude_and_a_glider_slows_the_fall() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    gear(&mut world, |gear| {
        gear.jetpack = 2;
        gear.glider = 1;
    });
    stand_at(&mut world, 30.0);
    // Jump, then hold jump + down: hover.
    run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, false).with_jump(true)),
        12,
    );
    let hover = TickInputs::held(PLAYER, ControlState::new(0.0, false, true).with_jump(true));
    let events = run(&mut world, &hover, 10);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotGlide { hover: true, .. }))
    );
    let start = pilot_pos(&world).1;
    run(&mut world, &hover, 30);
    let state = pilot_state(&world);
    assert!(state.hovering, "still hovering");
    assert!(
        (pilot_pos(&world).1 - start).abs() < 12.0,
        "held its height"
    );
    // Glide from high up: the fall stays slow, and faster without the glider.
    let fall_with = |glider: u8| {
        let mut world = arena();
        out_of_the_rig(&mut world);
        gear(&mut world, |gear| gear.glider = glider);
        let id = world.pilot_of(PLAYER).expect("pilot").0;
        world.edit_pilot(id, |body, state| {
            body.y = 12.0 * TILE_SIZE;
            body.previous_y = body.y;
            state.grounded = false;
        });
        run(
            &mut world,
            &TickInputs::held(PLAYER, ControlState::new(1.0, false, false).with_jump(true)),
            40,
        );
        (pilot_pos(&world).1, pilot_state(&world).gliding)
    };
    let (gliding_y, gliding) = fall_with(1);
    let (falling_y, _) = fall_with(0);
    assert!(gliding, "the glider opened");
    assert!(
        gliding_y < falling_y - 2.0 * TILE_SIZE,
        "{gliding_y} vs {falling_y}"
    );
}

#[test]
fn climbing_boots_slide_down_walls_and_kick_off_them() {
    let mut world = arena();
    fill(&mut world, 34..=35, 10..=31, TileKind::Stone);
    out_of_the_rig(&mut world);
    gear(&mut world, |gear| gear.boots = 1);
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, state| {
        body.x = 34.0 * TILE_SIZE - crate::pilot::PILOT_HALF_WIDTH - 0.3;
        body.y = 14.0 * TILE_SIZE;
        body.previous_x = body.x;
        body.previous_y = body.y;
        state.grounded = false;
        state.ledge_cooldown = 10.0;
    });
    run(&mut world, &held(1.0, false, false), 30);
    let (_, body, state) = world.pilot_of(PLAYER).expect("pilot");
    assert_eq!(state.wall, 1, "pressed against the wall");
    assert!(
        body.velocity_y <= crate::pilot::WALL_SLIDE_SPEED + 1.0,
        "sliding: {}",
        body.velocity_y
    );
    let events = run(
        &mut world,
        &TickInputs::held(PLAYER, ControlState::new(1.0, false, false).with_jump(true)),
        2,
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotWallJumped { .. })),
        "{events:?}"
    );
    let (_, body, _) = world.pilot_of(PLAYER).expect("pilot");
    assert!(
        body.velocity_x < 0.0 && body.velocity_y < 0.0,
        "kicked up and away"
    );
    // Boots also soak longer falls.
    assert!(
        crate::pilot::fall_damage_in(
            900.0,
            PilotGear {
                boots: 1,
                ..PilotGear::default()
            }
        ) < crate::pilot::fall_damage(900.0)
    );
}

#[test]
fn the_grapple_bites_rock_reels_the_pilot_up_and_lets_go() {
    let mut world = arena();
    fill(&mut world, 8..=60, 25..=25, TileKind::Stone);
    out_of_the_rig(&mut world);
    gear(&mut world, |gear| gear.grapple = 1);
    stand_at(&mut world, 30.0);
    // The ceiling is seven rows up: in reach.
    let events = pilot_action(
        &mut world,
        PilotAction::Rope {
            aim_x: 0,
            aim_y: -127,
        },
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::PilotRope {
                action: RopeEvent::Hooked,
                ..
            }
        )),
        "{events:?}"
    );
    let state = pilot_state(&world);
    assert_eq!(state.mode, PilotMode::Tether);
    assert_eq!(state.tether.expect("rope").kind, TetherKind::Grapple);
    let start = pilot_pos(&world).1;
    run(&mut world, &held(0.0, true, false), 90);
    let risen = start - pilot_pos(&world).1;
    assert!(risen > 4.0 * TILE_SIZE, "reeled up {risen}");
    // The rope button lets go.
    let events = pilot_action(&mut world, PilotAction::Rope { aim_x: 0, aim_y: 0 });
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotRope {
            action: RopeEvent::Released,
            ..
        }
    )));
    assert!(pilot_state(&world).tether.is_none());
    // Without a grapple the button does nothing away from the rig.
    let mut bare = arena();
    out_of_the_rig(&mut bare);
    stand_at(&mut bare, 40.0);
    let events = pilot_action(
        &mut bare,
        PilotAction::Rope {
            aim_x: 0,
            aim_y: -127,
        },
    );
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn the_rig_winch_lowers_its_pilot_down_a_shaft_and_hauls_it_back() {
    let mut world = arena();
    // A shaft through the floor right of the rig, ten tiles deep.
    fill(&mut world, 22..=22, FLOOR..=FLOOR + 10, TileKind::Air);
    fill(
        &mut world,
        21..=21,
        FLOOR + 1..=FLOOR + 11,
        TileKind::Bedrock,
    );
    fill(
        &mut world,
        23..=23,
        FLOOR + 1..=FLOOR + 11,
        TileKind::Bedrock,
    );
    fill(
        &mut world,
        22..=22,
        FLOOR + 11..=FLOOR + 11,
        TileKind::Bedrock,
    );
    out_of_the_rig(&mut world);
    assert!(world.can_board_rig(PLAYER));
    let events = pilot_action(&mut world, PilotAction::Rope { aim_x: 0, aim_y: 0 });
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PilotRope {
            action: RopeEvent::Winched,
            ..
        }
    )));
    // Over the shaft, then pay out.
    let id = world.pilot_of(PLAYER).expect("pilot").0;
    world.edit_pilot(id, |body, _| {
        body.x = 22.5 * TILE_SIZE;
        body.previous_x = body.x;
    });
    let events = run(&mut world, &held(0.0, false, true), 150);
    let (x, y) = pilot_pos(&world);
    assert!(
        y > (FLOOR + 5) as f32 * TILE_SIZE,
        "lowered down the shaft: {y}"
    );
    assert!((x - 22.5 * TILE_SIZE).abs() < TILE_SIZE);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotHurt { .. })),
        "a winched descent never hurts"
    );
    let bottom = y;
    run(&mut world, &held(0.0, true, false), 120);
    assert!(
        pilot_pos(&world).1 < bottom - 3.0 * TILE_SIZE,
        "hauled back up"
    );
    // The rope stays clipped to the rig the whole way.
    assert_eq!(
        pilot_state(&world).tether.map(|rope| rope.kind),
        Some(TetherKind::Winch)
    );
}

#[test]
fn armour_soaks_strikes() {
    let damage_with = |armor: u8| {
        let mut world = arena();
        out_of_the_rig(&mut world);
        gear(&mut world, |gear| gear.armor = armor);
        world.set_enemy_difficulty(crate::EnemyDifficulty::Normal);
        stand_at(&mut world, 40.0);
        let id = world.pilot_of(PLAYER).expect("pilot").0;
        let (x, y) = pilot_pos(&world);
        let before = pilot_state(&world).health;
        let mut output = TickOutput::default();
        assert!(world.strike_pilot(id, 20.0, (x - 20.0, y - 20.0), 0.0, &mut output));
        before - pilot_state(&world).health
    };
    let bare = damage_with(0);
    let armoured = damage_with(3);
    assert!(armoured < bare * 0.6, "{armoured} vs {bare}");
}

#[test]
fn the_high_cold_hurts_a_pilot_without_thermal_lining() {
    use crate::worldgen::{GeneratorVersion, WorldSettings, sky};
    let settings = WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(0x0E00_0015)
    };
    let altitude = (sky::DEAD_AIR_BOTTOM_ROWS - 20) as f32;
    let hurt = |thermal: u8| {
        let mut world =
            WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(60));
        fill(&mut world, 8..=60, 8..=31, TileKind::Air);
        fill(&mut world, 8..=60, FLOOR..=FLOOR, TileKind::Bedrock);
        rest(&mut world, 20.5);
        out_of_the_rig(&mut world);
        gear(&mut world, |gear| gear.thermal = thermal);
        let id = world.pilot_of(PLAYER).expect("pilot").0;
        let mut events = Vec::new();
        for _ in 0..30 {
            world.edit_pilot(id, |body, _| {
                body.y = -altitude * TILE_SIZE;
                body.previous_y = body.y;
                body.velocity_y = 0.0;
            });
            events.extend(run(&mut world, &TickInputs::default(), 1));
        }
        let state = pilot_state(&world);
        (state.max_health - state.health, events)
    };
    let (bare, events) = hurt(0);
    assert!(bare > 0.0, "the cold bites");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::PilotExposed { heat: false, .. }))
    );
    let (lined, _) = hurt(crate::pilot::MAX_THERMAL_TIER);
    assert!(lined < bare, "{lined} vs {bare}");
}

// --- Mechanisms ----------------------------------------------------------------------------

/// A gate column at x = 36 (lintel on row 28, bars rows 29..=31), a lever at (33, 30) set in a
/// wall stub, a plate at (44, 32) on channel 1 next to a second gate at x = 48.
fn stamp_mini_ruin(world: &mut WorldState) {
    let built = TileKind::Built;
    let mut cells = vec![
        (TilePosition { x: 36, y: 27 }, built(BuiltTile::RuinWall)),
        (
            TilePosition { x: 36, y: 28 },
            built(BuiltTile::GateLintel(0)),
        ),
        (
            TilePosition { x: 33, y: 30 },
            built(BuiltTile::Lever {
                channel: 0,
                on: false,
            }),
        ),
        (
            TilePosition { x: 44, y: FLOOR },
            built(BuiltTile::PressurePlate {
                channel: 1,
                down: false,
            }),
        ),
        (
            TilePosition { x: 48, y: 28 },
            built(BuiltTile::GateLintel(1)),
        ),
    ];
    for y in 29..=31 {
        cells.push((TilePosition { x: 36, y }, built(BuiltTile::Gate)));
        cells.push((TilePosition { x: 48, y }, built(BuiltTile::Gate)));
    }
    assert_eq!(world.stamp_tiles(&cells).len(), cells.len());
}

#[test]
fn the_use_button_works_a_fixture_nearer_than_the_rig() {
    let mut world = arena();
    world.stamp_tiles(&[(
        TilePosition {
            x: 23,
            y: FLOOR - 1,
        },
        TileKind::Built(BuiltTile::Lever {
            channel: 0,
            on: false,
        }),
    )]);
    out_of_the_rig(&mut world);
    rest(&mut world, 21.3);
    // Right up against the lever, with the rig a step behind: the lever takes the press.
    stand_at(&mut world, 22.55);
    assert!(world.can_board_rig(PLAYER));
    assert!(world.pilot_usable(PLAYER).is_some());
    assert!(world.pilot_fixture_first(PLAYER));
    // Beside the rig with the lever at arm's length: the press boards.
    stand_at(&mut world, 21.9);
    assert!(world.can_board_rig(PLAYER));
    assert!(world.pilot_usable(PLAYER).is_some());
    assert!(!world.pilot_fixture_first(PLAYER));
}

#[test]
fn a_lever_opens_its_gate_and_closes_it_again() {
    let mut world = arena();
    stamp_mini_ruin(&mut world);
    out_of_the_rig(&mut world);
    stand_at(&mut world, 34.5);
    let usable = world.pilot_usable(PLAYER).expect("the lever is in reach");
    assert_eq!(usable.0, TilePosition { x: 33, y: 30 });
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MechanismUsed {
            mechanism: MechanismEvent::GateOpened,
            ..
        }
    )));
    for y in 29..=31 {
        assert_eq!(tile(&world, 36, y), TileKind::Air, "row {y} open");
    }
    // The plate's gate (another channel) stays shut.
    assert_eq!(tile(&world, 48, 30), TileKind::Built(BuiltTile::Gate));
    // Walk through the gateway.
    run(&mut world, &held(1.0, false, false), 50);
    assert!(pilot_pos(&world).0 > 37.0 * TILE_SIZE, "through the gate");
    // Back at the lever: thrown again, the gate closes.
    stand_at(&mut world, 34.5);
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MechanismUsed {
            mechanism: MechanismEvent::GateClosed,
            ..
        }
    )));
    assert_eq!(tile(&world, 36, 31), TileKind::Built(BuiltTile::Gate));
    // A rig cannot drill a gate.
    assert!(crate::terrain::is_indestructible(TileKind::Built(
        BuiltTile::Gate
    )));
}

#[test]
fn a_pressure_plate_holds_its_gate_while_weighted_and_never_shuts_on_anyone() {
    let mut world = arena();
    stamp_mini_ruin(&mut world);
    out_of_the_rig(&mut world);
    stand_at(&mut world, 44.5);
    run(&mut world, &TickInputs::default(), 2);
    assert_eq!(
        tile(&world, 44, FLOOR),
        TileKind::Built(BuiltTile::PressurePlate {
            channel: 1,
            down: true
        })
    );
    assert_eq!(
        tile(&world, 48, 30),
        TileKind::Air,
        "the plate opened its gate"
    );
    // Standing in the gateway keeps it open past the hold time.
    stand_at(&mut world, 48.5);
    let hold_ticks = (crate::mechanism::PLATE_HOLD_SECONDS * 60.0) as u32;
    run(&mut world, &TickInputs::default(), hold_ticks + 30);
    assert_eq!(
        tile(&world, 48, 30),
        TileKind::Air,
        "it waits for the doorway to clear"
    );
    // Clear of it, the plate rises and the gate drops.
    stand_at(&mut world, 54.5);
    let events = run(&mut world, &TickInputs::default(), hold_ticks + 30);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MechanismUsed {
            mechanism: MechanismEvent::PlateReleased,
            ..
        }
    )));
    assert_eq!(tile(&world, 48, 30), TileKind::Built(BuiltTile::Gate));
}

#[test]
fn a_rig_parked_on_a_plate_holds_the_gate_for_its_pilot() {
    let mut world = arena();
    stamp_mini_ruin(&mut world);
    rest(&mut world, 44.5);
    run(&mut world, &TickInputs::default(), 20);
    assert_eq!(
        tile(&world, 48, 30),
        TileKind::Air,
        "the rig's weight holds the gate open"
    );
    out_of_the_rig(&mut world);
    run(&mut world, &TickInputs::default(), 300);
    assert_eq!(
        tile(&world, 48, 30),
        TileKind::Air,
        "still open while the rig sits there"
    );
}

#[test]
fn chests_open_by_hand_and_keycards_open_their_doors() {
    let mut world = arena();
    let built = TileKind::Built;
    let mut cells = vec![(
        TilePosition { x: 30, y: 31 },
        built(BuiltTile::Chest {
            open: false,
            card: Some(1),
        }),
    )];
    for y in 29..=31 {
        cells.push((TilePosition { x: 40, y }, built(BuiltTile::KeycardDoor(1))));
    }
    world.stamp_tiles(&cells);
    out_of_the_rig(&mut world);
    // The keycard door refuses a pilot without its card.
    stand_at(&mut world, 39.0);
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MechanismUsed {
            mechanism: MechanismEvent::Locked { color: 1 },
            ..
        }
    )));
    assert_eq!(
        tile(&world, 40, 30),
        TileKind::Built(BuiltTile::KeycardDoor(1))
    );
    // The chest opens by hand and spills its loot and the blue card.
    stand_at(&mut world, 28.8);
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::ChestOpened { .. })),
        "{events:?}"
    );
    assert_eq!(
        tile(&world, 30, 31),
        TileKind::Built(BuiltTile::Chest {
            open: true,
            card: None
        })
    );
    let drops = world.entities().count_of(EntityKind::Drop);
    assert!(drops >= 3, "loot and a keycard spilled: {drops}");
    // Let the spill settle, then walk over every stack (the ore first, so the pack fills up
    // before the card is reached: the card rides on the key ring and is never left behind).
    run(&mut world, &TickInputs::default(), 60);
    let mut spill: Vec<(bool, f32)> = world
        .entities()
        .iter()
        .filter_map(|entity| match entity.state {
            EntityState::Drop(drop) => Some((
                crate::mechanism::keycard_of(drop.stack.kind).is_some(),
                entity.body.x / TILE_SIZE,
            )),
            _ => None,
        })
        .collect();
    spill.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (_, column) in spill {
        stand_at(&mut world, column);
        run(&mut world, &TickInputs::default(), 10);
    }
    let player = world.player(PLAYER).expect("rig");
    let card = keycard_item(1);
    assert_eq!(
        player.pilot_pack.count(card),
        1,
        "the pilot picked up the card"
    );
    assert!(
        player.pilot_pack.total() - 1 <= player.pilot_gear.pack_capacity(),
        "ore still respects the pack's room: {:?}",
        player.pilot_pack
    );
    stand_at(&mut world, 39.0);
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MechanismUsed {
            mechanism: MechanismEvent::KeycardAccepted { color: 1 },
            ..
        }
    )));
    for y in 29..=31 {
        assert_eq!(tile(&world, 40, y), TileKind::Air);
    }
    // Nothing to use any more: an open chest is not usable.
    stand_at(&mut world, 28.8);
    assert!(world.pilot_usable(PLAYER).is_none());
}

#[test]
fn a_keycard_stowed_in_the_rig_still_opens_its_door() {
    let mut world = arena();
    for y in 29..=31 {
        world.stamp_tiles(&[(
            TilePosition { x: 40, y },
            TileKind::Built(BuiltTile::KeycardDoor(2)),
        )]);
    }
    out_of_the_rig(&mut world);
    // Boarding emptied the pack into the rig: the card rides in the cargo bay.
    let stowed = world
        .player_mut(PLAYER)
        .expect("rig")
        .stow(keycard_item(2), 1);
    assert_eq!(stowed, 1);
    stand_at(&mut world, 39.0);
    let events = pilot_action(&mut world, PilotAction::Use);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::MechanismUsed {
                mechanism: MechanismEvent::KeycardAccepted { color: 2 },
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(tile(&world, 40, 30), TileKind::Air);
}

// --- Rig and pilot together ----------------------------------------------------------------

#[test]
fn a_rig_link_recalls_the_rig_to_its_pilot_along_an_open_path() {
    let mut world = arena();
    out_of_the_rig(&mut world);
    gear(&mut world, |gear| gear.rig_link = 1);
    stand_at(&mut world, 50.0);
    let events = pilot_action(&mut world, PilotAction::RecallRig);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::RigRecall {
            outcome: RecallOutcome::Started,
            ..
        }
    )));
    let events = run(&mut world, &held(0.0, false, false), 600);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::RigRecall {
                outcome: RecallOutcome::Arrived,
                ..
            }
        )),
        "the rig arrived"
    );
    assert!(world.can_board_rig(PLAYER), "parked beside its pilot");
    assert!(!pilot_state(&world).recalling);
    // Sealed off from the rig: no path.
    let mut sealed = arena();
    out_of_the_rig(&mut sealed);
    gear(&mut sealed, |gear| gear.rig_link = 1);
    stand_at(&mut sealed, 50.0);
    fill(&mut sealed, 45..=46, 8..=31, TileKind::Bedrock);
    let events = pilot_action(&mut sealed, PilotAction::RecallRig);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::RigRecall {
            outcome: RecallOutcome::NoPath,
            ..
        }
    )));
    // Without the link the command is refused.
    let mut unlinked = arena();
    out_of_the_rig(&mut unlinked);
    stand_at(&mut unlinked, 50.0);
    assert!(pilot_action(&mut unlinked, PilotAction::RecallRig).is_empty());
}

#[test]
fn a_rig_link_sentry_fires_on_creatures_near_its_pilot() {
    let mut world = arena();
    let _ = world
        .player_mut(PLAYER)
        .expect("rig")
        .receive_weapon(WeaponKind::RivetGun);
    out_of_the_rig(&mut world);
    stand_at(&mut world, 26.0);
    let (x, y) = pilot_pos(&world);
    let target = world.spawn_target_dummy(x + 2.0 * TILE_SIZE, y - 30.0, 400.0, 0.0);
    // Tier 1: no sentry.
    gear(&mut world, |gear| gear.rig_link = 1);
    let events = run(&mut world, &held(0.0, false, false), 30);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::WeaponFired { .. }))
    );
    gear(&mut world, |gear| gear.rig_link = 2);
    let events = run(&mut world, &held(0.0, false, false), 60);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::WeaponFired { .. })),
        "the parked rig covers its pilot"
    );
    assert!(target_health(&world, target).is_none_or(|health| health < 400.0));
}

// --- Saves and replication -----------------------------------------------------------------

#[test]
fn pilot_content_saves_replicates_and_keeps_playing_identically() {
    let mut world = arena();
    world.add_player(OTHER);
    stamp_mini_ruin(&mut world);
    {
        let player = world.player_mut(PLAYER).expect("rig");
        let _ = player.pilot_arsenal.receive(WeaponKind::Shotgun);
        let _ = player.pilot_arsenal.add_ammo(AmmoKind::Shell, 9);
        player.pilot_gear.grapple = 2;
    }
    out_of_the_rig(&mut world);
    fill(&mut world, 8..=60, 25..=25, TileKind::Stone);
    stand_at(&mut world, 44.5);
    pilot_action(
        &mut world,
        PilotAction::Rope {
            aim_x: 0,
            aim_y: -127,
        },
    );
    run(&mut world, &held(0.0, true, false), 10);
    assert!(pilot_state(&world).tether.is_some());
    assert!(!world.bases().plates().is_empty(), "a plate is held");

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
        ControlState::new(1.0, false, true).with_weapon(WeaponControl {
            fire: true,
            auto_aim: true,
            aim_x: 127,
            aim_y: 0,
        }),
    );
    for _ in 0..240 {
        let tick = world.simulation_tick();
        let a = world.simulate_tick(tick, &script);
        let b = restored.simulate_tick(SimulationTick::new(tick.get()), &script);
        assert_eq!(a, b);
    }
    assert_eq!(
        restored.simulation_state_hash(),
        world.simulation_state_hash()
    );

    // Another client sees the pilot's weapon and rope, but not its arsenal.
    let client = world.redacted_for(Some(OTHER));
    let (_, _, seen) = client.pilot_of(PLAYER).expect("everyone sees the pilot");
    assert_eq!(seen.weapon, Some(WeaponKind::Shotgun));
    assert!(
        client
            .player(PLAYER)
            .is_none_or(|player| player.pilot_arsenal.owned.is_empty()),
        "the arsenal is private"
    );
    assert_eq!(
        client.visible_state_hash(Some(OTHER)),
        world.visible_state_hash(Some(OTHER))
    );
    // The pilot entity carries the new state through entity records.
    let record = world
        .entities()
        .iter()
        .find(|entity| matches!(entity.state, EntityState::Pilot(_)))
        .expect("pilot record")
        .clone();
    let round: crate::entity::Entity =
        postcard::from_bytes(&postcard::to_allocvec(&record).expect("encode")).expect("decode");
    assert_eq!(round, record);
}
