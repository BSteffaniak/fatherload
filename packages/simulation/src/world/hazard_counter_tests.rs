//! Progression and hazard counters: upgrade curves, drill pace, lava, boulders, the hazard
//! counter lines and test mode's god-mode drilling.

use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    economy::{
        MAX_UPGRADE_LEVEL, UpgradeKind, buy_upgrade, current_level, max_out_upgrades, upgrade_cost,
        upgrade_offers,
    },
    player::{
        ENGINE_CLIMB_CAP, Player, cargo_capacity_for_level, drill_bite, drill_speed, engine_power,
        fuel_capacity_for_level, hull_capacity_for_level,
    },
    poi::PoiTile,
    protection::{HazardGear, LAVA_PLATING_MAX_LEVEL},
    terrain::{ArtifactKind, TILE_SIZE, TileKind, TilePosition, tile_durability},
    tick::TickRate,
};

use super::{
    commands::{
        LAVA_DRILL_DAMAGE, MIN_DRILL_SECONDS_PER_CHIP, authoritative_drill_seconds_per_chip,
    },
    hazards::BOULDER_HIT_DAMAGE,
    movement::{
        AUTHORITATIVE_BOULDER_SPAWN_CHANCE, AUTHORITATIVE_BOULDER_WARNING_MILLIS,
        AUTHORITATIVE_CAVE_IN_WARNING_MILLIS, AUTHORITATIVE_RIG_GRAVITY,
        AUTHORITATIVE_THRUST_ACCELERATION,
    },
};

const PLAYER: PlayerId = PlayerId::new(1);
const SHAFT_X: i32 = 30;
const FLOOR_Y: i32 = 13;

/// A shallow pocket of air (rows 9..13) in dirt around column [`SHAFT_X`], with the rig resting
/// on its floor and `floor` as the tile under the rig.
fn pocket_world(floor: TileKind) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(60));
    for x in SHAFT_X - 4..=SHAFT_X + 4 {
        for y in 5..=FLOOR_Y + 3 {
            let open = (SHAFT_X - 1..=SHAFT_X + 1).contains(&x) && (9..FLOOR_Y).contains(&y);
            let kind = if open { TileKind::Air } else { TileKind::Dirt };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let _ = world.terrain_mut().set_kind(below(), floor);
    let player = world.player_mut(PLAYER).expect("player");
    player.x = TILE_SIZE.mul_add(0.5, tile_px(SHAFT_X));
    player.y = tile_px(FLOOR_Y) - 11.0;
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
    player.fuel = player.fuel_capacity;
    world
}

const fn below() -> TilePosition {
    TilePosition {
        x: SHAFT_X,
        y: FLOOR_Y,
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "small tile coordinates convert to pixels"
)]
const fn tile_px(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE
}

/// Holds drill-down for up to `ticks` ticks, returning the ticks it took to clear the tile
/// below the rig (or `None`).
fn drill_down(world: &mut WorldState, ticks: u32) -> Option<u32> {
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, false, true));
    let mut tick = world.simulation_tick();
    for elapsed in 1..=ticks {
        let _ = world.simulate_tick(tick, &inputs);
        tick = tick.next();
        if world
            .terrain()
            .tile(below())
            .is_some_and(|tile| tile.kind == TileKind::Air)
        {
            return Some(elapsed);
        }
    }
    None
}

fn hull(world: &WorldState) -> f32 {
    world.player(PLAYER).expect("player").hull
}

#[test]
fn every_upgrade_line_has_many_tiers_with_rising_prices() {
    let mut player = Player::new(0.0, 0.0);
    for kind in UpgradeKind::ALL {
        if matches!(kind, UpgradeKind::BombPack | UpgradeKind::DrillMount) {
            continue;
        }
        let tiers = kind.max_level() - kind.starting_level();
        assert!(tiers >= 6, "{kind:?} has {tiers} tiers");
        let first = upgrade_cost(kind, kind.starting_level());
        assert!(first <= 1_000, "{kind:?} starts cheap ({first} cr)");
        let mut previous = 0;
        for level in kind.starting_level()..kind.max_level() {
            let cost = upgrade_cost(kind, level);
            assert!(cost > previous, "{kind:?} tier {} costs more", level + 1);
            previous = cost;
        }
    }
    for kind in [
        UpgradeKind::Drill,
        UpgradeKind::FuelTank,
        UpgradeKind::CargoBay,
        UpgradeKind::Engine,
        UpgradeKind::Hull,
        UpgradeKind::Radiator,
    ] {
        assert_eq!(kind.max_level(), MAX_UPGRADE_LEVEL);
        let top = upgrade_cost(kind, MAX_UPGRADE_LEVEL - 1);
        assert!(
            (50_000..=500_000).contains(&top),
            "{kind:?} top tier is a big goal ({top} cr)"
        );
    }
    // Buying walks the whole line and stops at the top.
    player.credits = u32::MAX / 2;
    let index = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::CargoBay)
        .expect("cargo line");
    while buy_upgrade(&mut player, index).is_ok() {}
    assert_eq!(player.cargo_bay_level, MAX_UPGRADE_LEVEL);
    assert_eq!(
        player.cargo_capacity,
        cargo_capacity_for_level(MAX_UPGRADE_LEVEL)
    );
}

#[test]
fn endgame_stats_are_huge() {
    let mut player = Player::new(0.0, 0.0);
    max_out_upgrades(&mut player);
    assert!(
        player.cargo_capacity >= 200,
        "cargo {}",
        player.cargo_capacity
    );
    assert!(
        player.fuel_capacity >= 2_000.0,
        "fuel {}",
        player.fuel_capacity
    );
    assert!(player.max_hull() >= 2_000.0, "hull {}", player.max_hull());
    // A maxed engine with a full bay still out-thrusts gravity several times over.
    let full_bay_thrust =
        AUTHORITATIVE_THRUST_ACCELERATION * engine_power(MAX_UPGRADE_LEVEL) * 0.82;
    assert!(full_bay_thrust > 4.0 * AUTHORITATIVE_RIG_GRAVITY);
    assert!(engine_power(MAX_UPGRADE_LEVEL) > ENGINE_CLIMB_CAP);
    // Every stat curve rises with every tier.
    for level in 1..MAX_UPGRADE_LEVEL {
        assert!(cargo_capacity_for_level(level + 1) > cargo_capacity_for_level(level));
        assert!(fuel_capacity_for_level(level + 1) > fuel_capacity_for_level(level));
        assert!(hull_capacity_for_level(level + 1) > hull_capacity_for_level(level));
        assert!(engine_power(level + 1) > engine_power(level));
        assert!(drill_speed(level + 1) > drill_speed(level));
        assert!(drill_bite(level + 1) >= drill_bite(level));
    }
    // The stock rig is unchanged.
    let stock = Player::new(0.0, 0.0);
    assert_eq!(stock.cargo_capacity, cargo_capacity_for_level(1));
    assert!((stock.fuel_capacity - fuel_capacity_for_level(1)).abs() < f32::EPSILON);
    assert!((stock.max_hull() - hull_capacity_for_level(1)).abs() < f32::EPSILON);
}

/// Seconds a rig with a drill of `level` takes to dig `kind` straight down at `hz`.
fn dig_seconds(world: &WorldState, kind: TileKind, level: u8, hz: u32) -> f32 {
    let seconds_per_chip = authoritative_drill_seconds_per_chip(
        kind,
        level,
        crate::DrillDirection::Down,
        &std::collections::BTreeSet::new(),
        world.terrain(),
        below(),
    );
    let chips = tile_durability(kind).div_ceil(drill_bite(level));
    let rate = TickRate::new(hz);
    #[allow(clippy::cast_precision_loss, reason = "a handful of ticks")]
    let seconds =
        (u64::from(chips) * rate.ticks_for_seconds(seconds_per_chip)) as f32 / rate.hz_f32();
    seconds
}

#[test]
fn a_maxed_drill_plows_through_the_hardest_rock() {
    let world = pocket_world(TileKind::Dirt);
    let hardest = TileKind::Artifact(ArtifactKind::MantleHeart);
    for kind in [TileKind::HardRock, hardest] {
        let mut previous = f32::MAX;
        for level in 5..=MAX_UPGRADE_LEVEL {
            let seconds = dig_seconds(&world, kind, level, 60);
            assert!(
                seconds <= previous,
                "{kind:?} digs no slower at tier {level}"
            );
            previous = seconds;
        }
        let tier5 = dig_seconds(&world, kind, 5, 60);
        let top = dig_seconds(&world, kind, MAX_UPGRADE_LEVEL, 60);
        assert!(top <= 0.25, "{kind:?} takes {top}s with a maxed drill");
        assert!(top * 3.0 < tier5, "{kind:?}: tier 5 {tier5}s vs top {top}s");
        // The pace holds at other tick rates (chips never go below a few ticks).
        for hz in [30, 120] {
            let other = dig_seconds(&world, kind, MAX_UPGRADE_LEVEL, hz);
            assert!((other - top).abs() <= 0.06, "{hz} Hz: {other}s vs {top}s");
        }
    }
    let stock = dig_seconds(&world, TileKind::Stone, 3, 60);
    assert!(stock > 0.5, "early rigs still feel the rock ({stock}s)");
}

#[test]
fn a_maxed_rig_digs_hard_rock_in_the_world_quickly() {
    let mut world = pocket_world(TileKind::HardRock);
    max_out_upgrades(world.player_mut(PLAYER).expect("player"));
    let ticks = drill_down(&mut world, 60).expect("hard rock gives way");
    assert!(ticks <= 15, "took {ticks} ticks");
}

#[test]
fn lava_is_drillable_and_burns_less_with_plating() {
    let mut burns = Vec::new();
    for plating in 0..=LAVA_PLATING_MAX_LEVEL {
        let mut world = pocket_world(TileKind::Lava);
        let player = world.player_mut(PLAYER).expect("player");
        player.hazard_gear.lava_plating = plating;
        let before = hull(&world);
        drill_down(&mut world, 60).expect("lava no longer blocks the drill");
        burns.push(before - hull(&world));
    }
    assert!(
        burns[0] >= LAVA_DRILL_DAMAGE,
        "stock rigs get burnt: {burns:?}"
    );
    for pair in burns.windows(2) {
        assert!(pair[1] <= pair[0], "each tier burns less: {burns:?}");
    }
    assert!(burns.last().expect("top tier").abs() < f32::EPSILON);
}

#[test]
fn magma_vents_and_pressure_pockets_are_braced_against() {
    for floor in [
        TileKind::MagmaVent,
        TileKind::PressurePocket,
        TileKind::ExplosivePocket,
    ] {
        let mut stock = pocket_world(floor);
        let before = hull(&stock);
        drill_down(&mut stock, 60).expect("pocket gives way");
        assert!(hull(&stock) < before, "{floor:?} hurts a stock rig");

        let mut braced = pocket_world(floor);
        braced.player_mut(PLAYER).expect("player").hazard_gear = HazardGear::MAXED;
        let before = hull(&braced);
        drill_down(&mut braced, 60).expect("pocket gives way");
        assert!(
            (hull(&braced) - before).abs() < f32::EPSILON,
            "{floor:?} cannot hurt a braced rig"
        );
    }
}

/// Drops a boulder onto the rig and returns the hull it cost.
fn boulder_hit(gear: HazardGear) -> f32 {
    let mut world = pocket_world(TileKind::Dirt);
    world.player_mut(PLAYER).expect("player").hazard_gear = gear;
    let (x, y) = {
        let player = world.player(PLAYER).expect("player");
        (player.x, TILE_SIZE.mul_add(-1.5, player.y))
    };
    world.spawn_falling_boulder(x, y, 0, 4_000);
    let before = hull(&world);
    let inputs = TickInputs::default();
    let mut tick = world.simulation_tick();
    for _ in 0..120 {
        let _ = world.simulate_tick(tick, &inputs);
        tick = tick.next();
    }
    assert!(world.falling_boulders().is_empty(), "the boulder landed");
    before - hull(&world)
}

#[test]
fn the_boulder_deflector_turns_boulders_aside() {
    let stock = boulder_hit(HazardGear::default());
    assert!(
        (stock - BOULDER_HIT_DAMAGE).abs() < 0.01,
        "stock hit {stock}"
    );
    let half = boulder_hit(HazardGear {
        boulder_deflector: 3,
        ..HazardGear::default()
    });
    assert!(half < stock && half > 0.0, "tier 3 hit {half}");
    assert!(boulder_hit(HazardGear::MAXED).abs() < f32::EPSILON);
}

#[test]
fn boulders_fall_rarely_and_with_a_long_warning() {
    // One in 16 mined stone tiles used to loosen a boulder with under a second of warning.
    const { assert!(AUTHORITATIVE_BOULDER_SPAWN_CHANCE >= 64) };
    const { assert!(AUTHORITATIVE_BOULDER_WARNING_MILLIS >= 1_500) };
    const { assert!(AUTHORITATIVE_CAVE_IN_WARNING_MILLIS >= 900) };

    // Mining a long stone corridor loosens few boulders (the same per-tile roll the tick uses).
    let world = pocket_world(TileKind::Dirt);
    let mut boulders = 0;
    for x in 0..400 {
        let position = TilePosition {
            x: x % 200 + 20,
            y: 40 + x / 200,
        };
        let value = world.terrain().seed()
            ^ (u64::try_from(position.x)
                .expect("positive")
                .wrapping_mul(0x9E37))
            ^ (u64::try_from(position.y)
                .expect("positive")
                .wrapping_mul(0x85EB));
        if value.is_multiple_of(AUTHORITATIVE_BOULDER_SPAWN_CHANCE) {
            boulders += 1;
        }
    }
    assert!(boulders <= 20, "{boulders} boulders in 400 stone tiles");
}

#[test]
fn fast_drills_loosen_no_more_boulders_per_minute() {
    // The spawn odds fall with the drill's bite, so plowing through rock with a top drill does
    // not rain boulders any faster than digging with a mid-game one.
    let world = pocket_world(TileKind::Dirt);
    let per_second = |level: u8| {
        let tiles_per_second = 1.0 / dig_seconds(&world, TileKind::HardRock, level, 60);
        #[allow(clippy::cast_precision_loss, reason = "small constants")]
        let chance = 1.0 / super::movement::boulder_spawn_chance(level) as f32;
        tiles_per_second * chance
    };
    let mid_game = per_second(5);
    assert_eq!(
        super::movement::boulder_spawn_chance(1),
        AUTHORITATIVE_BOULDER_SPAWN_CHANCE
    );
    for level in 6..=MAX_UPGRADE_LEVEL {
        assert!(
            per_second(level) <= mid_game,
            "tier {level}: {} boulders/s vs {mid_game} at tier 5",
            per_second(level)
        );
    }
}

#[test]
fn test_mode_drills_anything_near_instantly_without_harm() {
    for floor in [
        TileKind::Foundation,
        TileKind::Lava,
        TileKind::MagmaVent,
        TileKind::ExplosivePocket,
        TileKind::Artifact(ArtifactKind::MantleHeart),
        TileKind::Poi(PoiTile::VaultWall),
    ] {
        let mut world = pocket_world(floor);
        world.enable_test_mode();
        world.player_mut(PLAYER).expect("player").hull = 50.0;
        let ticks = drill_down(&mut world, 30).unwrap_or_else(|| panic!("{floor:?} gives way"));
        assert!(
            u64::from(ticks) <= TickRate::new(60).ticks_for_seconds(MIN_DRILL_SECONDS_PER_CHIP) + 2,
            "{floor:?} took {ticks} ticks"
        );
    }
}

#[test]
fn invulnerable_test_mode_takes_no_damage_from_anything() {
    let mut world = pocket_world(TileKind::Lava);
    world.enable_test_mode();
    // Strip the counters: invulnerability alone must hold.
    world.player_mut(PLAYER).expect("player").hazard_gear = HazardGear::default();
    let full = world.player(PLAYER).expect("player").max_hull();
    let (x, y) = {
        let player = world.player(PLAYER).expect("player");
        (player.x, TILE_SIZE.mul_add(-1.5, player.y))
    };
    world.spawn_falling_boulder(x, y, 0, 4_000);
    let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, false, true));
    let mut tick = world.simulation_tick();
    for _ in 0..120 {
        let output = world.simulate_tick(tick, &inputs);
        tick = tick.next();
        assert!((hull(&world) - full).abs() < f32::EPSILON);
        assert!(
            !output
                .events
                .iter()
                .any(|event| matches!(event.kind, crate::GameplayEventKind::PlayerDamaged { .. })),
            "no damage events"
        );
    }
}

#[test]
fn old_saves_past_the_old_top_tier_keep_upgrading() {
    // A rig maxed under the old five-tier curves.
    let mut player = Player::new(0.0, 0.0);
    player.drill_strength = 5;
    player.cargo_bay_level = 5;
    player.cargo_capacity = 44;
    player.fuel_tank_level = 5;
    player.fuel_capacity = 300.0;
    player.hull_level = 5;
    player.radiator_level = 5;
    let mut json = serde_json::to_value(&player).expect("serialize");
    json.as_object_mut()
        .expect("player object")
        .remove("hazard_gear");
    let mut loaded: Player = serde_json::from_value(json).expect("old saves load");
    assert_eq!(loaded.hazard_gear, HazardGear::default());
    // Nothing is maxed: every line offers its next tier.
    for offer in upgrade_offers(&loaded) {
        assert!(
            !crate::economy::upgrade_maxed(&offer),
            "{:?} still upgrades",
            offer.kind
        );
    }
    loaded.credits = 1_000_000;
    let cargo = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::CargoBay)
        .expect("cargo line");
    let fuel = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::FuelTank)
        .expect("fuel line");
    buy_upgrade(&mut loaded, cargo).expect("tier 6 cargo");
    buy_upgrade(&mut loaded, fuel).expect("tier 6 fuel");
    assert_eq!(current_level(&loaded, UpgradeKind::CargoBay), 6);
    assert!(loaded.cargo_capacity >= cargo_capacity_for_level(6));
    assert!(loaded.fuel_capacity >= fuel_capacity_for_level(6));
    // Loading catches an old rig up to the current curves at once, keeping earned bonuses.
    let mut old = Player::new(0.0, 0.0);
    old.cargo_bay_level = 5;
    old.cargo_capacity = 44 + 4;
    old.fuel_tank_level = 5;
    old.fuel_capacity = 300.0 + 20.0;
    old.catch_up_capacity_curves();
    assert_eq!(old.cargo_capacity, cargo_capacity_for_level(5) + 4);
    assert!((old.fuel_capacity - (fuel_capacity_for_level(5) + 20.0)).abs() < f32::EPSILON);
    let caught_up = old.clone();
    old.catch_up_capacity_curves();
    assert_eq!(old, caught_up, "idempotent");
    let mut stock = Player::new(0.0, 0.0);
    stock.catch_up_capacity_curves();
    assert_eq!(stock, Player::new(0.0, 0.0), "current rigs are untouched");
    // Levels beyond the new top load fine too (clamped by the shop, not by loading).
    let mut beyond = Player::new(0.0, 0.0);
    beyond.drill_strength = 20;
    let json = serde_json::to_string(&beyond).expect("serialize");
    let beyond: Player = serde_json::from_str(&json).expect("load");
    assert!(crate::economy::upgrade_maxed(&upgrade_offers(&beyond)[0]));
}
