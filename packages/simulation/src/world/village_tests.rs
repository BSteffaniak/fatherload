//! Villages in play: first-visit discovery, counter deals validated at the door, blessings,
//! charting and the journal riding in the player's private loadout.

use std::sync::Arc;

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SessionServiceRequest, SimulationParams, TickInputs, TickOutput, WorldState,
    terrain::{MineralKind, TILE_SIZE},
    village::{
        self, BlessingKind, BuildingKind, Specialty, Village, VillageDeal, VillageService,
        placement,
    },
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);

fn world(seed: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V6,
        ..WorldSettings::new(seed)
    };
    WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(60))
}

/// Parks the rig on the ground at `column` of `found`.
fn park(world: &mut WorldState, found: &Village, column: i32) {
    let rig = world.player_mut(PLAYER).expect("rig");
    #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
    {
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (found.ground_row as f32 - 0.5) * TILE_SIZE;
    }
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

fn idle() -> TickInputs {
    TickInputs::held(PLAYER, ControlState::new(0.0, false, false))
}

/// The first village (from site 1 outward) with a building of `kind`.
fn village_with(world: &WorldState, kind: BuildingKind) -> (Arc<Village>, usize) {
    let settings = *world.world_settings();
    for site in (1..30).flat_map(|site| [site, -site]) {
        let Some(found) = village::village(&settings, placement::surface_id(site)) else {
            continue;
        };
        if let Some(index) = found.buildings.iter().position(|b| b.kind == kind) {
            return (found, index);
        }
    }
    panic!("no {kind:?} in thirty sites");
}

fn deal(found: &Village, building: usize, offer: usize) -> PlayerCommand {
    PlayerCommand::Service(SessionServiceRequest::Village {
        village: found.id,
        building: u8::try_from(building).expect("index"),
        offer: u8::try_from(offer).expect("index"),
        service: VillageService::of(found.buildings[building].kind).expect("service"),
    })
}

fn outcome(output: &TickOutput) -> PlayerScopedCommandOutcome {
    output.action_outcomes.first().expect("one action").outcome
}

#[test]
fn entering_a_village_records_it_once_with_an_event() {
    let mut world = world(0x7111);
    let settings = *world.world_settings();
    let found = village::village(&settings, placement::surface_id(1)).expect("village");
    park(&mut world, &found, found.center_column());
    let first = step(&mut world, &idle());
    assert!(first.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::VillageDiscovered { player_id: PLAYER, village } if village == found.id
    )));
    let again = step(&mut world, &idle());
    assert!(
        !again
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::VillageDiscovered { .. }))
    );
    let player = world.player(PLAYER).expect("rig");
    assert!(player.villages.visited.contains(&found.id));
    assert_eq!(world.player_village(PLAYER).map(|v| v.id), Some(found.id));
}

#[test]
fn trading_post_sells_cargo_and_pumps_fuel_only_at_its_door() {
    let mut world = world(0x7112);
    let (found, market) = village_with(&world, BuildingKind::Market);
    let lot = found.buildings[market].clone();
    park(&mut world, &found, lot.first_column + 2);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.cargo_bay.add(MineralKind::Gold, 3);
        rig.fuel = 10.0;
        rig.credits = 500;
    }
    assert_eq!(
        world
            .player_village_building(PLAYER)
            .map(|(v, b)| (v.id, b)),
        Some((found.id, market))
    );
    let sold = step(&mut world, &idle().act(PLAYER, deal(&found, market, 0)));
    assert_eq!(outcome(&sold), PlayerScopedCommandOutcome::Applied);
    assert!(sold.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::VillageServiceUsed {
            service: VillageService::TradingPost,
            ..
        }
    )));
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.cargo_bay.mineral(MineralKind::Gold), 0);
    assert!(rig.credits > 500);
    let credits = rig.credits;
    let fuelled = step(&mut world, &idle().act(PLAYER, deal(&found, market, 1)));
    assert_eq!(outcome(&fuelled), PlayerScopedCommandOutcome::Applied);
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.fuel > 10.0);
    assert!(rig.credits < credits);

    // A step off the lot and the counter refuses.
    park(&mut world, &found, lot.first_column - 1);
    world.player_mut(PLAYER).expect("rig").fuel = 5.0;
    let refused = step(&mut world, &idle().act(PLAYER, deal(&found, market, 1)));
    assert_eq!(
        outcome(&refused),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
    // A forged building index or service is refused too.
    park(&mut world, &found, lot.first_column + 2);
    let forged = PlayerCommand::Service(SessionServiceRequest::Village {
        village: found.id,
        building: u8::try_from(market).expect("index"),
        offer: 1,
        service: VillageService::Shrine,
    });
    let refused = step(&mut world, &idle().act(PLAYER, forged));
    assert_eq!(
        outcome(&refused),
        PlayerScopedCommandOutcome::IgnoredUnavailable
    );
}

#[test]
fn an_assay_office_pays_well_for_its_mineral_only() {
    let mut world = world(0x7113);
    let (found, assay) = village_with(&world, BuildingKind::Assay);
    let Specialty::MineralBuyer(mineral) = found.specialty else {
        panic!("assay without a mineral");
    };
    let other = if mineral == MineralKind::Copper {
        MineralKind::Iron
    } else {
        MineralKind::Copper
    };
    park(&mut world, &found, found.buildings[assay].first_column + 1);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.cargo_bay.add(mineral, 4);
        rig.cargo_bay.add(other, 2);
        rig.credits = 0;
    }
    let town_value = world.mineral_market_value(mineral) * 4;
    let sold = step(&mut world, &idle().act(PLAYER, deal(&found, assay, 0)));
    assert_eq!(outcome(&sold), PlayerScopedCommandOutcome::Applied);
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.cargo_bay.mineral(mineral), 0);
    assert_eq!(rig.cargo_bay.mineral(other), 2, "only its mineral");
    assert!(
        rig.credits >= town_value * 3 / 2,
        "{} vs {town_value}",
        rig.credits
    );
}

#[test]
fn a_shrine_blessing_wards_the_hull_until_it_fades() {
    let mut world = world(0x7114);
    let (found, shrine) = village_with(&world, BuildingKind::Shrine);
    let offers = found.offers(shrine);
    let ward = offers
        .iter()
        .position(|offer| offer.deal == VillageDeal::Blessing(BlessingKind::Ward))
        .expect("ward");
    park(&mut world, &found, found.buildings[shrine].first_column + 1);
    world.player_mut(PLAYER).expect("rig").credits = 1_000;
    let blessed = step(&mut world, &idle().act(PLAYER, deal(&found, shrine, ward)));
    assert_eq!(outcome(&blessed), PlayerScopedCommandOutcome::Applied);
    assert_eq!(world.player_blessing(PLAYER), Some(BlessingKind::Ward));
    let before = world.player(PLAYER).expect("rig").hull;
    assert!(world.damage_player(PLAYER, 10.0));
    let after = world.player(PLAYER).expect("rig").hull;
    assert!((before - after - 7.0).abs() < 0.01, "{before} -> {after}");
    let until = world
        .player(PLAYER)
        .expect("rig")
        .villages
        .blessing
        .expect("blessing")
        .until;
    world.set_simulation_tick(until);
    assert_eq!(world.player_blessing(PLAYER), None);
}

#[test]
fn a_cartographer_charts_sites_and_neighbouring_villages() {
    let mut world = world(0x7115);
    let (found, room) = village_with(&world, BuildingKind::MapRoom);
    park(&mut world, &found, found.buildings[room].first_column + 1);
    world.player_mut(PLAYER).expect("rig").credits = 1_000;
    let charted = step(&mut world, &idle().act(PLAYER, deal(&found, room, 0)));
    assert_eq!(outcome(&charted), PlayerScopedCommandOutcome::Applied);
    let rig = world.player(PLAYER).expect("rig");
    assert!(!rig.villages.charted.is_empty());
    assert!(rig.villages.knows(placement::surface_id(found.site + 1)));
    assert!(rig.credits < 1_000);
}

#[test]
fn the_village_journal_saves_and_stays_private() {
    let mut world = world(0x7116);
    let settings = *world.world_settings();
    let found = village::village(&settings, placement::surface_id(-1)).expect("village");
    park(&mut world, &found, found.center_column());
    let _ = step(&mut world, &idle());
    let json = serde_json::to_string(world.player(PLAYER).expect("rig")).expect("save");
    let loaded: crate::Player = serde_json::from_str(&json).expect("load");
    assert!(loaded.villages.visited.contains(&found.id));
    let other = PlayerId::new(2);
    let redacted = world.redacted_for(Some(other));
    assert!(
        redacted
            .player(PLAYER)
            .expect("rig")
            .villages
            .visited
            .is_empty()
    );
}
