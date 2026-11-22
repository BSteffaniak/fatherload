//! The Wave 7 seams wired together in a Deep Descent world (generator V11): the generator
//! places each stratum's fluid pockets and the depth logistics sites (outposts on the
//! landmark halls, the builders' cable of ancient lifts), world creation installs the sites,
//! the depth rules read the real registry layout, and a lift ride works across frame moves.

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    base::BuiltTile,
    depth_progression::{DescentLayout, RegistryLayout, layout_for},
    descent::{Descent, Stratum},
    enemy::EnemyDifficulty,
    fluid::FluidKind,
    logistics::{LogisticsCommand, sites::LogisticsSites},
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{ChunkPosition, TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, WorldSettings,
        descent::{
            landmarks::{self, logistics_sites, outpost_column},
            layout,
        },
        streamed::generated_chunk,
    },
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// Every fluid tile generated in the chunks of `stratum`'s middle rows over 64 columns.
fn fluids_in(settings: &WorldSettings, stratum: Stratum) -> Vec<(TilePosition, FluidKind)> {
    let (top, bottom) = layout(settings).rows(stratum);
    let mut found = Vec::new();
    for cy in (top + 8).div_euclid(16)..=(bottom - 8).div_euclid(16) {
        for cx in -40..40 {
            let chunk = ChunkPosition { x: cx, y: cy };
            let origin = chunk.origin();
            for (index, tile) in generated_chunk(settings, chunk).tiles().iter().enumerate() {
                if let TileKind::Fluid(kind) = tile.kind {
                    let index = i32::try_from(index).unwrap_or(0);
                    found.push((
                        TilePosition {
                            x: origin.x + index % 16,
                            y: origin.y + index / 16,
                        },
                        kind,
                    ));
                }
            }
        }
    }
    found
}

#[test]
fn v11_strata_generate_their_fluid_pockets_and_older_worlds_none() {
    let settings = v11(0x00F1_D1D5);
    let ocean = fluids_in(&settings, Stratum::UndergroundOcean);
    assert!(ocean.len() > 200, "{} ocean cells", ocean.len());
    // (Chunks near a ragged boundary take the neighbour's rules.)
    let water = ocean
        .iter()
        .filter(|(_, kind)| *kind == FluidKind::Water)
        .count();
    assert!(water * 10 >= ocean.len() * 9, "{water} of {}", ocean.len());
    let glaciers = fluids_in(&settings, Stratum::BrineGlaciers);
    assert!(glaciers.iter().any(|(_, kind)| *kind == FluidKind::Brine));
    let core = fluids_in(&settings, Stratum::CoreOcean);
    assert!(core.iter().any(|(_, kind)| *kind == FluidKind::LiquidMetal));
    let flesh = fluids_in(&settings, Stratum::FleshCaverns);
    assert!(flesh.iter().any(|(_, kind)| *kind == FluidKind::Acid));
    // A stratum without rules stays dry.
    assert!(fluids_in(&settings, Stratum::SlateShelves).is_empty());
    // Generated fluid rests: every cell has something under it that holds it.
    let world = WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    for (at, _) in ocean.iter().chain(&core).take(400) {
        let below = world
            .terrain()
            .tile(TilePosition {
                x: at.x,
                y: at.y + 1,
            })
            .map(|tile| tile.kind);
        assert!(
            below.is_some_and(|kind| kind != TileKind::Air),
            "fluid over air at {at:?}"
        );
    }
    // V10 places no fluid at all.
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..settings
    };
    for cy in 0..40 {
        for cx in -8..8 {
            assert!(
                generated_chunk(&classic, ChunkPosition { x: cx, y: cy })
                    .tiles()
                    .iter()
                    .all(|tile| !matches!(tile.kind, TileKind::Fluid(_)))
            );
        }
    }
}

#[test]
fn v11_carves_and_installs_an_outpost_per_descent_on_the_builders_cable() {
    let settings = v11(0x0BAD_CAB1);
    let sites = logistics_sites(&settings);
    assert_eq!(sites.outposts.len(), Descent::ALL.len());
    // The builders' cable (one lift per Descent) plus the Cathedral Lift (Wave 8), last.
    assert_eq!(sites.lifts.len(), Descent::ALL.len() + 1);
    let column = outpost_column(&settings);
    let layout = layout(&settings);
    for (outpost, descent) in sites.outposts.iter().zip(Descent::ALL) {
        assert_eq!(outpost.column, column);
        let (top, bottom) = layout.descent_rows(descent);
        assert!((top..=bottom).contains(&outpost.floor_row), "{outpost:?}");
        // The landmark hall and the outpost agree on the floor.
        let hall = landmarks::outpost(&settings, &layout, descent).expect("hall");
        assert_eq!(hall.bounds.max.y, outpost.floor_row);
    }
    for pair in sites.lifts[..Descent::ALL.len()].windows(2) {
        assert_eq!(pair[0].bottom_row, pair[1].top_row, "the lifts chain");
        assert_eq!(pair[0].column, pair[1].column);
    }
    // The generator carved every site tile, and world creation registered the records.
    let world = WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    for (position, kind) in LogisticsSites::carved_tiles(&sites).iter().step_by(7) {
        assert_eq!(
            world.terrain().tile(*position).map(|tile| tile.kind),
            Some(*kind),
            "{position:?}"
        );
    }
    let lift = &sites.lifts[1];
    let rail = TilePosition {
        x: lift.column,
        y: i32::midpoint(lift.stations[0], lift.stations[1]),
    };
    assert_eq!(
        world.terrain().tile(rail).map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::RuinWall))
    );
    assert_eq!(world.logistics().outposts.len(), Descent::ALL.len());
    assert_eq!(world.logistics().lifts.len(), Descent::ALL.len() + 1);
    assert!(world.progression_rules().is_descent());
    // A V10 world has none of it.
    let classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(1),
        SimulationParams::default(),
    );
    assert!(classic.logistics().outposts.is_empty());
    assert!(logistics_sites(&WorldSettings::new(1)).lifts.is_empty());
}

#[test]
fn the_depth_rules_read_the_registry_layout_on_v11_worlds() {
    let settings = v11(0x1A70);
    let registry = layout_for(&settings);
    let rows = layout(&settings);
    assert_eq!(
        registry,
        crate::depth_progression::WorldLayout::Registry(Box::new(RegistryLayout::of(&settings)))
    );
    assert_eq!(registry.strata().len(), Stratum::ALL.len());
    for (span, descent) in registry.descents().iter().zip(Descent::ALL) {
        let (top, bottom) = rows.descent_rows(descent);
        assert_eq!((span.top, span.bottom), (top, bottom + 1));
        assert_eq!(span.name, descent.name());
    }
    for pair in registry.strata().windows(2) {
        assert_eq!(pair[0].bottom, pair[1].top, "strata are contiguous");
    }
    let world = WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    let (hollow_top, _) = rows.descent_rows(Descent::HollowWorld);
    assert_eq!(world.descent_of_row(hollow_top + 3), 4);
    // Classic worlds keep the plan table.
    assert!(matches!(
        layout_for(&WorldSettings::new(2)),
        crate::depth_progression::WorldLayout::Plan(_)
    ));
}

#[test]
fn an_ancient_lift_carries_a_rig_up_the_cable_across_frame_moves() {
    let settings = v11(0x00CA_B1E5);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.enable_test_mode();
    {
        // Every rig system and protection fitted and test mode's invulnerability: the Descent
        // gates and the placeholder strata's borrowed heat leave the rider alone.
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.credits = 1_000_000;
        rig.hazard_gear = crate::protection::HazardGear::MAXED;
    }
    let lift = world
        .logistics()
        .lifts
        .values()
        .find(|lift| {
            lift.stations.first() != Some(&landmarks::LIFT_TOP_ROW)
                && lift
                    .stations
                    .last()
                    .zip(lift.stations.first())
                    .is_some_and(|(bottom, top)| bottom - top > 1_100)
        })
        .cloned()
        .expect("a lift between two outposts more than two sectors apart");
    let (top, bottom) = (
        *lift.stations.first().expect("top"),
        *lift.stations.last().expect("bottom"),
    );
    assert!(bottom - top > 1_024, "the ride crosses two sectors");
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(lift.column), i64::from(bottom)),
    );
    let act = |world: &mut WorldState, command| {
        let inputs = TickInputs::default().act(PLAYER, PlayerCommand::Logistics(command));
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        (
            output.action_outcomes.first().expect("outcome").outcome,
            output.events,
        )
    };
    let shaft = lift.id;
    let (outcome, _) = act(&mut world, LogisticsCommand::RestoreLift { shaft });
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    let frame_before = world.frame();
    let (outcome, _) = act(
        &mut world,
        LogisticsCommand::RideLift { shaft, to_row: top },
    );
    assert_eq!(
        outcome,
        PlayerScopedCommandOutcome::Applied,
        "{:?}",
        world.check_logistics(PLAYER, &LogisticsCommand::RideLift { shaft, to_row: top })
    );
    let mut arrived = false;
    for _ in 0..20_000 {
        let output = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        if world.logistics().is_riding(PLAYER) {
            let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
            assert_eq!(tile.x, lift.column, "pinned to the rail");
        }
        if output.events.iter().any(|event| {
            matches!(event.kind, GameplayEventKind::LiftArrived { player_id, .. } if player_id == PLAYER)
        }) {
            arrived = true;
            break;
        }
    }
    assert!(
        arrived,
        "the ride arrived: riding {}, hull {}, at {:?}",
        world.logistics().is_riding(PLAYER),
        world.player(PLAYER).expect("rig").hull,
        world.player(PLAYER).expect("rig").tile_in(world.frame())
    );
    assert_ne!(
        world.frame(),
        frame_before,
        "the frame followed the rider up"
    );
    for _ in 0..30 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
    assert_eq!(
        tile,
        TilePosition {
            x: lift.column,
            y: top
        }
    );
}
