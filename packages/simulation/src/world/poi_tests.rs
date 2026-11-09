//! Point-of-interest gameplay through the real tick: crates, vault locks, visits, scanner
//! contacts, and their replication and hashing.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SectionKey, SimulationParams,
    TILE_SIZE, TickInputs, TickOutput, WorldState,
    poi::{PoiKind, PoiKnowledge, PoiTile, PoiVariant, RELAY_WINDOW_MILLIS, VaultPuzzle},
    terrain::{TileKind, TilePosition},
    worldgen::{WorldFeature, WorldSettings},
};

const RIG: PlayerId = PlayerId::new(1);

fn world(seed: u64) -> WorldState {
    WorldState::new_game_with_settings(RIG, &WorldSettings::new(seed), SimulationParams::default())
}

/// The first world (from a few seeds) with a POI matching `wanted`, and that POI.
fn world_with(wanted: impl Fn(PoiVariant) -> bool) -> (WorldState, WorldFeature) {
    for seed in 1..40 {
        let world = world(seed);
        let found = world
            .world_features()
            .pois()
            .find(|poi| poi.poi().is_some_and(&wanted))
            .copied();
        if let Some(poi) = found {
            return (world, poi);
        }
    }
    panic!("no world with the wanted POI");
}

fn tile(world: &WorldState, position: TilePosition) -> TileKind {
    world
        .terrain()
        .tile(position)
        .map_or(TileKind::Air, |tile| tile.kind)
}

fn poi_tiles(
    world: &WorldState,
    poi: &WorldFeature,
    wanted: fn(PoiTile) -> bool,
) -> Vec<TilePosition> {
    poi.bounds
        .positions()
        .filter(|position| matches!(tile(world, *position), TileKind::Poi(found) if wanted(found)))
        .collect()
}

/// Puts the rig at rest on top of `position` (clearing the tile above it).
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn stand_on(world: &mut WorldState, position: TilePosition) {
    let above = TilePosition {
        x: position.x,
        y: position.y - 1,
    };
    if world.terrain().is_solid_at(above) {
        world.terrain_mut().set_kind(above, TileKind::Air);
    }
    let rig = world.player_mut(RIG).expect("rig");
    rig.x = (position.x as f32 + 0.5) * TILE_SIZE;
    rig.y = (above.y as f32 + 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.drill_strength = 5;
    rig.fuel = rig.fuel_capacity;
    rig.hull = 1_000.0;
}

fn step(world: &mut WorldState, inputs: &TickInputs) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, inputs)
}

/// Drills straight down into `position` from on top of it until the tile changes. Returns
/// every event on the way.
fn drill_down_into(world: &mut WorldState, position: TilePosition) -> Vec<GameplayEventKind> {
    stand_on(world, position);
    let before = tile(world, position);
    let drill = TickInputs::held(RIG, ControlState::new(0.0, false, true));
    let mut events = Vec::new();
    for _ in 0..600 {
        let output = step(world, &drill);
        events.extend(output.events.iter().map(|event| event.kind));
        let reset = events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::VaultLockReset { .. }));
        if tile(world, position) != before || reset {
            return events;
        }
    }
    panic!("{before:?} at {position:?} never changed");
}

#[test]
fn supply_crates_pay_out_once_when_drilled_open() {
    let (mut world, cache) = world_with(|variant| variant == PoiVariant::SupplyCache);
    let crate_tile = poi_tiles(&world, &cache, |tile| tile == PoiTile::SupplyCrate)[0];
    let before = world.player(RIG).cloned().expect("rig");
    let events = drill_down_into(&mut world, crate_tile);
    assert_eq!(tile(&world, crate_tile), TileKind::Air);
    let reward = events
        .iter()
        .find_map(|event| match event {
            GameplayEventKind::PoiOpened {
                player_id,
                poi: PoiKind::SupplyCache,
                reward,
            } if *player_id == RIG => Some(*reward),
            _ => None,
        })
        .expect("the crate paid out");
    let after = world.player(RIG).expect("rig");
    assert!(reward.credits > 0 || reward.fuel > 0 || reward.bombs > 0);
    assert_eq!(after.credits, before.credits + reward.credits);
    assert_eq!(after.bombs, before.bombs + u32::from(reward.bombs));
    assert_eq!(
        after.journal.knowledge(cache.id),
        Some(PoiKnowledge::Visited)
    );
}

#[test]
fn sequence_vaults_open_only_in_order_and_reset_on_a_wrong_switch() {
    let (mut world, vault) =
        world_with(|variant| variant == PoiVariant::Vault(VaultPuzzle::Sequence));
    let switch = |world: &WorldState, wanted: u8| {
        vault
            .bounds
            .positions()
            .find(|position| {
                matches!(tile(world, *position), TileKind::Poi(PoiTile::VaultSwitch { order, .. }) if order == wanted)
            })
            .expect("switch")
    };
    let doors = poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultDoor);
    assert!(!doors.is_empty());
    // Switch 1 lights, then switch 3 out of order resets the lock.
    let first = switch(&world, 1);
    let _ = drill_down_into(&mut world, first);
    assert_eq!(
        tile(&world, first),
        TileKind::Poi(PoiTile::VaultSwitch {
            order: 1,
            lit: true
        })
    );
    let third = switch(&world, 3);
    let events = drill_down_into(&mut world, third);
    assert!(events.contains(&GameplayEventKind::VaultLockReset { feature: vault.id }));
    assert!(
        poi_tiles(&world, &vault, |tile| matches!(
            tile,
            PoiTile::VaultSwitch { lit: true, .. }
        ))
        .is_empty()
    );
    // In order, the doors open.
    for order in 1..=3 {
        let position = switch(&world, order);
        let events = drill_down_into(&mut world, position);
        assert_eq!(
            events.contains(&GameplayEventKind::VaultOpened { feature: vault.id }),
            order == 3
        );
    }
    assert!(
        doors
            .iter()
            .all(|door| tile(&world, *door) == TileKind::Air)
    );
    // The walls are still there and no drill gets through them.
    let wall = poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultWall)[0];
    assert_eq!(
        world.terrain().hardness_at(wall),
        Some(crate::poi::UNDRILLABLE)
    );
}

#[test]
fn pressure_seal_vaults_open_once_every_seal_is_bombed() {
    let (mut world, vault) = world_with(|variant| variant == PoiVariant::Vault(VaultPuzzle::Seals));
    let seals = poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultSeal);
    let doors = poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultDoor);
    assert!(seals.len() >= 2);
    // A drill does nothing to a seal.
    assert_eq!(
        world.terrain().hardness_at(seals[0]),
        Some(crate::poi::UNDRILLABLE)
    );
    let mut opened = false;
    for (index, seal) in seals.iter().enumerate() {
        // Stand outside the vault next to the seal and drop a bomb.
        let outside = [(-1, 0), (1, 0), (0, -1)]
            .iter()
            .map(|(dx, dy)| TilePosition {
                x: seal.x + dx,
                y: seal.y + dy,
            })
            .find(|position| {
                !vault.bounds.expanded(-1).contains(*position)
                    && !matches!(tile(&world, *position), TileKind::Poi(_))
            })
            .expect("outside tile");
        world.terrain_mut().set_kind(outside, TileKind::Air);
        let below = TilePosition {
            x: outside.x,
            y: outside.y + 1,
        };
        stand_on(&mut world, below);
        world.player_mut(RIG).expect("rig").bombs = 1;
        let _ = step(
            &mut world,
            &TickInputs::default().act(RIG, PlayerCommand::PlaceBomb),
        );
        let mut events = Vec::new();
        for _ in 0..400 {
            let output = step(&mut world, &TickInputs::default());
            events.extend(output.events.iter().map(|event| event.kind));
            if world.bombs().is_empty() {
                break;
            }
        }
        assert_eq!(
            tile(&world, *seal),
            TileKind::Poi(PoiTile::VaultSealVented),
            "seal {index}"
        );
        opened = events.contains(&GameplayEventKind::VaultOpened { feature: vault.id });
        assert_eq!(opened, index + 1 == seals.len(), "seal {index}");
    }
    assert!(opened);
    assert!(
        doors
            .iter()
            .all(|door| tile(&world, *door) == TileKind::Air)
    );
}

#[test]
fn twin_relays_must_be_drilled_within_the_window() {
    let (mut world, vault) = world_with(|variant| variant == PoiVariant::Vault(VaultPuzzle::Relay));
    let relays = poi_tiles(&world, &vault, |tile| {
        matches!(tile, PoiTile::VaultRelay { .. })
    });
    assert_eq!(relays.len(), 2);
    let _ = drill_down_into(&mut world, relays[0]);
    assert!(
        world
            .progression()
            .vault_relay_deadlines
            .contains_key(&vault.id)
    );
    // Wait out the window: the relay cools down.
    let ticks = world.timing().ticks_for_millis(RELAY_WINDOW_MILLIS) + 2;
    let mut reset = false;
    for _ in 0..ticks {
        let output = step(&mut world, &TickInputs::default());
        reset |= output
            .events
            .iter()
            .any(|event| event.kind == GameplayEventKind::VaultLockReset { feature: vault.id });
    }
    assert!(reset);
    assert_eq!(
        tile(&world, relays[0]),
        TileKind::Poi(PoiTile::VaultRelay { lit: false })
    );
    assert!(world.progression().vault_relay_deadlines.is_empty());
    // Both in time: open.
    let _ = drill_down_into(&mut world, relays[0]);
    let events = drill_down_into(&mut world, relays[1]);
    assert!(events.contains(&GameplayEventKind::VaultOpened { feature: vault.id }));
    assert!(poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultDoor).is_empty());
}

#[test]
fn vault_caches_hand_out_a_rare_part_or_a_fitting() {
    let (mut world, vault) = world_with(|variant| matches!(variant, PoiVariant::Vault(_)));
    let cache = poi_tiles(&world, &vault, |tile| tile == PoiTile::VaultCache)[0];
    let events = drill_down_into(&mut world, cache);
    let reward = events
        .iter()
        .find_map(|event| match event {
            GameplayEventKind::PoiOpened {
                poi: PoiKind::Vault,
                reward,
                ..
            } => Some(*reward),
            _ => None,
        })
        .expect("the vault paid out");
    // A new crew owns every part already, so the vault fits the rig instead.
    assert_eq!(reward.part, None);
    let fitting = reward.fitting.expect("a fitting");
    let rig = world.player(RIG).expect("rig");
    match fitting {
        crate::poi::RigFitting::Bulkhead => assert_eq!(rig.crafted_bulkheads, 1),
        crate::poi::RigFitting::AuxiliaryTank => assert!(rig.fuel_capacity > 100.0),
        crate::poi::RigFitting::CargoRack => assert_eq!(rig.cargo_capacity, 14),
    }
}

#[test]
fn derelict_cores_add_their_log_to_the_journal() {
    let (mut world, derelict) =
        world_with(|variant| matches!(variant, PoiVariant::DerelictRig { .. }));
    let Some(PoiVariant::DerelictRig { log }) = derelict.poi() else {
        unreachable!();
    };
    let core = poi_tiles(&world, &derelict, |tile| tile == PoiTile::SalvageCore)[0];
    let _ = drill_down_into(&mut world, core);
    let rig = world.player(RIG).expect("rig");
    assert!(rig.journal.logs.contains(&log));
    assert!(rig.credits > 0);
}

#[test]
fn entering_a_poi_records_a_visit_once() {
    let (mut world, poi) = world_with(|_| true);
    stand_on(
        &mut world,
        TilePosition {
            x: poi.center.x,
            y: poi.center.y + 1,
        },
    );
    let first = step(&mut world, &TickInputs::default());
    let discovered = |output: &TickOutput| {
        output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::PoiDiscovered { feature, .. } if feature == poi.id))
            .count()
    };
    assert_eq!(discovered(&first), 1);
    assert_eq!(discovered(&step(&mut world, &TickInputs::default())), 0);
    assert_eq!(
        world.player(RIG).expect("rig").journal.knowledge(poi.id),
        Some(PoiKnowledge::Visited)
    );
}

#[test]
fn scanner_tiers_record_echoes_then_silhouettes() {
    let (mut world, poi) = world_with(|_| true);
    let near = TilePosition {
        x: poi.bounds.max.x + 6,
        y: poi.center.y,
    };
    stand_on(
        &mut world,
        TilePosition {
            x: near.x,
            y: near.y + 1,
        },
    );
    let scan = TickInputs::default().act(RIG, PlayerCommand::UseScanner);
    // No scanner: nothing.
    let _ = step(&mut world, &scan);
    assert_eq!(
        world.player(RIG).expect("rig").journal.knowledge(poi.id),
        None
    );
    world.player_mut(RIG).expect("rig").scanner_level = 1;
    for _ in 0..80 {
        let _ = step(&mut world, &TickInputs::default());
    }
    let _ = step(&mut world, &scan);
    assert_eq!(
        world.player(RIG).expect("rig").journal.knowledge(poi.id),
        Some(PoiKnowledge::Echo)
    );
    world.player_mut(RIG).expect("rig").scanner_level = crate::poi::IDENTIFY_SCANNER_LEVEL;
    for _ in 0..80 {
        let _ = step(&mut world, &TickInputs::default());
    }
    let _ = step(&mut world, &scan);
    assert_eq!(
        world.player(RIG).expect("rig").journal.knowledge(poi.id),
        Some(PoiKnowledge::Identified)
    );
}

#[test]
fn poi_state_replicates_and_is_hashed() {
    let (mut authority, vault) =
        world_with(|variant| variant == PoiVariant::Vault(VaultPuzzle::Relay));
    let mut replica = authority.clone();
    let relay = poi_tiles(&authority, &vault, |tile| {
        matches!(tile, PoiTile::VaultRelay { .. })
    })[0];
    let hash_before = authority.simulation_state_hash();
    let _ = drill_down_into(&mut authority, relay);
    assert_ne!(authority.simulation_state_hash(), hash_before);
    authority
        .player_mut(RIG)
        .expect("rig")
        .journal
        .learn(vault.id, PoiKnowledge::Identified);
    for key in [SectionKey::Timers, SectionKey::Loadout(RIG)] {
        let section = authority.section(key).expect("section");
        replica.apply_section(key, section);
        assert_eq!(replica.section_hash(key), authority.section_hash(key));
    }
    assert_eq!(
        replica.progression().vault_relay_deadlines,
        authority.progression().vault_relay_deadlines
    );
    assert_eq!(
        replica.player(RIG).expect("rig").journal,
        authority.player(RIG).expect("rig").journal
    );
    // Saves keep it too.
    let json = serde_json::to_string(&authority).expect("serialize");
    let loaded: WorldState = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        loaded.simulation_state_hash(),
        authority.simulation_state_hash()
    );
}

#[test]
fn relay_windows_last_the_same_time_at_every_tick_rate() {
    let (_, vault) = world_with(|variant| variant == PoiVariant::Vault(VaultPuzzle::Relay));
    let seed = (1..40)
        .find(|seed| {
            world(*seed)
                .world_features()
                .get(vault.id)
                .is_some_and(|feature| *feature == vault)
        })
        .expect("the vault's seed");
    let run = |hz: u32| {
        let settings = WorldSettings::new(seed);
        let mut world =
            WorldState::new_game_with_settings(RIG, &settings, SimulationParams::new(hz));
        let relay = poi_tiles(&world, &vault, |tile| {
            matches!(tile, PoiTile::VaultRelay { .. })
        })[0];
        let _ = drill_down_into(&mut world, relay);
        let lit_at = world.simulation_tick();
        while world
            .progression()
            .vault_relay_deadlines
            .contains_key(&vault.id)
        {
            let _ = step(&mut world, &TickInputs::default());
        }
        world
            .timing()
            .rate
            .seconds_for_ticks(world.simulation_tick().get() - lit_at.get())
    };
    let (at_60, at_30) = (run(60), run(30));
    assert!((at_60 - at_30).abs() < 0.1, "{at_60} vs {at_30}");
    assert!((at_60 - 9.0).abs() < 0.1, "{at_60}");
}

#[test]
fn bombs_never_destroy_loot() {
    let (mut world, derelict) =
        world_with(|variant| matches!(variant, PoiVariant::DerelictRig { .. }));
    let core = poi_tiles(&world, &derelict, |tile| tile == PoiTile::SalvageCore)[0];
    let wreck_before = poi_tiles(&world, &derelict, |tile| tile == PoiTile::Wreckage).len();
    let _ = world.detonate_bomb_at(core, 2);
    assert_eq!(tile(&world, core), TileKind::Poi(PoiTile::SalvageCore));
    // The plating around it still blows apart.
    assert!(poi_tiles(&world, &derelict, |tile| tile == PoiTile::Wreckage).len() < wreck_before);
    let events = drill_down_into(&mut world, core);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::PoiOpened {
            poi: PoiKind::DerelictRig,
            ..
        }
    )));
}
