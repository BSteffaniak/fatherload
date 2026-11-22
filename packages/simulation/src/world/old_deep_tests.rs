//! Descent II, the Old Deep, through the tick (Wave 8): every stratum's signature rule, the
//! Cathedral Lift, the dose meter, the Star Core fought by a bot with Descent II kit, and the
//! gate that holds without the Pressure Hull.

use crate::{
    ControlState, GameplayEventKind, PlayerId, SimulationParams, TickInputs, WorldState,
    depth_progression::RigSystem,
    descent::{Descent, Stratum},
    enemy::EnemyDifficulty,
    entity::EntityState,
    old_deep::{self, CorePhase, DeepTile, Mutation, sites},
    position::WorldPos,
    protection::HazardGear,
    rig::RigPartKind,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings, descent::layout},
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// A V11 world at `hz`, creatures off, with a rig fitted with Descent II's kit (deep tiers,
/// every hazard counter) but none of the Descent rig systems.
fn deep_world(seed: u64, hz: u32) -> WorldState {
    let settings = v11(seed);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.drill_strength = 8;
    rig.engine_level = 10;
    rig.hull_level = 10;
    rig.radiator_level = 12;
    rig.hazard_gear = HazardGear {
        rig_systems: 0,
        ..HazardGear::MAXED
    };
    rig.hull = rig.max_hull();
    rig.fuel_capacity = 5_000.0;
    rig.fuel = 5_000.0;
    world
}

/// A column far from the spawn, the outposts and the set pieces.
const FAR: i32 = -380;

/// A row in the middle of `stratum`.
fn middle_row(world: &WorldState, stratum: Stratum) -> i32 {
    let (top, bottom) = layout(world.terrain().settings()).rows(stratum);
    i32::midpoint(top, bottom)
}

/// Carves a closed room: solid `wall` around `(x0..=x1, y0..=y1)` of air.
fn room(world: &mut WorldState, (x0, y0): (i32, i32), (x1, y1): (i32, i32), wall: TileKind) {
    for y in y0 - 2..=y1 + 2 {
        for x in x0 - 2..=x1 + 2 {
            let inside = x >= x0 && x <= x1 && y >= y0 && y <= y1;
            let kind = if inside { TileKind::Air } else { wall };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
}

fn set(world: &mut WorldState, x: i32, y: i32, kind: TileKind) {
    let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
}

fn kind(world: &WorldState, x: i32, y: i32) -> Option<TileKind> {
    world
        .terrain()
        .tile(TilePosition { x, y })
        .map(|tile| tile.kind)
}

/// Puts the rig resting on the floor under tile `(x, y)`.
fn stand(world: &mut WorldState, x: i32, y: i32) {
    world.teleport_rig_to(PLAYER, WorldPos::tile_center(i64::from(x), i64::from(y)));
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let output = world.simulate_tick(world.simulation_tick(), &TickInputs::held(PLAYER, controls));
    output.events.into_iter().map(|event| event.kind).collect()
}

fn idle(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(step(world, ControlState::default()));
    }
    events
}

fn deep(tile: DeepTile) -> TileKind {
    TileKind::Deep(tile)
}

#[test]
fn classic_worlds_never_touch_the_old_deep() {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(60));
    let _ = idle(&mut world, 120);
    assert!(world.old_deep().is_empty());
    assert!(world.cathedral_site().is_none());
    assert!(world.star_core_arena().is_none());
}

#[test]
fn drilling_resonant_crystal_rings_the_fault_as_far_as_the_drill_is_fast() {
    for (strength, harvest, expect_min) in [(3u8, false, 1usize), (8, false, 4), (8, true, 4)] {
        let mut world = deep_world(0x0A11_C0DE, 60);
        let row = middle_row(&world, Stratum::CrystalFaults);
        room(
            &mut world,
            (FAR - 6, row - 4),
            (FAR + 6, row),
            TileKind::Stone,
        );
        // A fault: crystal under the rig and a line of it running off through the rock.
        for dx in 0..8 {
            set(
                &mut world,
                FAR + dx,
                row + 1,
                deep(DeepTile::ResonantCrystal),
            );
        }
        world.player_mut(PLAYER).expect("rig").drill_strength = strength;
        if harvest {
            world
                .equipped_rig_parts
                .entry(PLAYER)
                .or_default()
                .insert(RigPartKind::ResonanceDrill);
        }
        stand(&mut world, FAR, row);
        let mut chain = None;
        for _ in 0..600 {
            for event in step(&mut world, ControlState::new(0.0, false, true)) {
                if let GameplayEventKind::ResonanceChain {
                    shattered,
                    harvested,
                    ..
                } = event
                {
                    chain = Some((shattered, harvested));
                }
            }
            if chain.is_some() {
                break;
            }
        }
        let (shattered, harvested) = chain.expect("the fault rang");
        assert!(
            usize::from(shattered) >= expect_min,
            "drill {strength}: {shattered}"
        );
        if strength == 3 {
            assert!(shattered <= 1, "a slow drill barely rings it: {shattered}");
        }
        assert_eq!(harvested, harvest);
        let left = (1..8)
            .filter(|dx| kind(&world, FAR + dx, row + 1) == Some(deep(DeepTile::ResonantCrystal)))
            .count();
        assert!(left < 7);
    }
}

#[test]
fn a_prism_splits_the_headlamp_and_lights_a_latent_gem() {
    let mut world = deep_world(0x0BEA_4500, 60);
    let row = middle_row(&world, Stratum::CrystalFaults);
    room(
        &mut world,
        (FAR - 6, row - 3),
        (FAR + 6, row),
        TileKind::Stone,
    );
    set(&mut world, FAR + 4, row, deep(DeepTile::Prism));
    // A latent gem down-right of the prism, behind a crystal window.
    set(
        &mut world,
        FAR + 5,
        row + 1,
        deep(DeepTile::ResonantCrystal),
    );
    set(&mut world, FAR + 6, row + 2, deep(DeepTile::LatentGem));
    stand(&mut world, FAR, row - 1);
    let refraction = world.refraction_for(PLAYER);
    assert_eq!(refraction.prisms.len(), 1, "{refraction:?}");
    assert_eq!(refraction.beams.len(), 3);
    assert!(refraction.gems.contains(&TilePosition {
        x: FAR + 6,
        y: row + 2
    }));
    // A Lumen mutation lights gems near the rig without a prism.
    set(&mut world, FAR + 4, row, TileKind::Stone);
    set(&mut world, FAR - 2, row + 1, deep(DeepTile::LatentGem));
    assert!(world.refraction_for(PLAYER).gems.is_empty());
    world.set_mutation_for_tests(PLAYER, Mutation::Lumen);
    assert!(!world.refraction_for(PLAYER).gems.is_empty());
}

#[test]
fn fossil_bones_come_out_whole_only_when_dug_free_and_a_whole_skeleton_pays() {
    let mut world = deep_world(0x0F05_5115, 60);
    let row = middle_row(&world, Stratum::FossilOceans);
    // A buried bone, taken straight down: rock on three sides, it crumbles.
    room(
        &mut world,
        (FAR - 4, row - 3),
        (FAR + 4, row),
        TileKind::Stone,
    );
    set(&mut world, FAR, row + 1, deep(DeepTile::FossilBone));
    stand(&mut world, FAR, row);
    let mut freed = Vec::new();
    for _ in 0..600 {
        for event in step(&mut world, ControlState::new(0.0, false, true)) {
            if let GameplayEventKind::FossilBoneFreed { intact, .. } = event {
                freed.push(intact);
            }
        }
        if !freed.is_empty() {
            break;
        }
    }
    assert_eq!(freed, vec![false], "a buried bone crumbles");
    // Dug free on both sides first: it lifts out whole.
    let mut world = deep_world(0x0F05_5115, 60);
    room(
        &mut world,
        (FAR - 4, row - 3),
        (FAR + 4, row),
        TileKind::Stone,
    );
    set(&mut world, FAR, row + 1, deep(DeepTile::FossilBone));
    set(&mut world, FAR - 1, row + 1, TileKind::Air);
    set(&mut world, FAR + 1, row + 1, TileKind::Air);
    set(&mut world, FAR - 1, row + 2, TileKind::Stone);
    set(&mut world, FAR + 1, row + 2, TileKind::Stone);
    stand(&mut world, FAR, row);
    let mut freed = Vec::new();
    for _ in 0..600 {
        for event in step(&mut world, ControlState::new(0.0, false, true)) {
            if let GameplayEventKind::FossilBoneFreed { intact, .. } = event {
                freed.push(intact);
            }
        }
        if !freed.is_empty() {
            break;
        }
    }
    assert_eq!(freed, vec![true], "a bone dug free comes out whole");

    // A generated skeleton excavated bone by bone, every bone freed first: the bounty.
    let settings = *world.terrain().settings();
    let lay = layout(&settings);
    let (w, h) = sites::region_size(Stratum::FossilOceans);
    let (top, bottom) = lay.rows(Stratum::FossilOceans);
    let skeleton = (top / h..=bottom / h)
        .flat_map(|ry| (-20..20).map(move |rx| (rx, ry)))
        .find_map(|(rx, ry)| sites::skeleton(&settings, &lay, rx, ry))
        .expect("a skeleton in the Fossil Oceans");
    let _ = w;
    // Stream the skeleton's chunks in by standing next to it.
    let first = skeleton.bones[0];
    stand(&mut world, first.x, first.y - 3);
    let _ = idle(&mut world, 2);
    for &bone in &skeleton.bones {
        assert_eq!(
            kind(&world, bone.x, bone.y),
            Some(deep(DeepTile::FossilBone))
        );
    }
    let credits = world.player(PLAYER).expect("rig").credits;
    let mut output = crate::event::TickOutput::default();
    for &bone in &skeleton.bones {
        // Clear everything around it but its fellow bones, then take it.
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let at = TilePosition {
                x: bone.x + dx,
                y: bone.y + dy,
            };
            if kind(&world, at.x, at.y) != Some(deep(DeepTile::FossilBone)) {
                set(&mut world, at.x, at.y, TileKind::Air);
            }
        }
        set(&mut world, bone.x, bone.y, TileKind::Air);
        world.mined_deep_tile(PLAYER, bone, DeepTile::FossilBone, &mut output);
    }
    let completed = output.events.iter().any(|event| {
        matches!(event.kind, GameplayEventKind::SkeletonCompleted { bones, .. } if usize::from(bones) == skeleton.bones.len())
    });
    assert!(completed, "the skeleton completed");
    assert!(world.player(PLAYER).expect("rig").credits > credits);
}

#[test]
fn amber_holds_a_rig_fast() {
    let mut world = deep_world(0x0A3B_E500, 60);
    let row = middle_row(&world, Stratum::FossilOceans);
    room(
        &mut world,
        (FAR - 8, row - 4),
        (FAR + 8, row),
        TileKind::Stone,
    );
    for dy in -4..=0 {
        set(&mut world, FAR + 1, row + dy, deep(DeepTile::Amber));
    }
    stand(&mut world, FAR, row);
    world.player_mut(PLAYER).expect("rig").velocity_x = 300.0;
    let events = idle(&mut world, 2);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::AmberStuck { .. }))
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.velocity_x.abs() <= old_deep::AMBER_SPEED_CAP + 0.01,
        "{}",
        rig.velocity_x
    );
}

#[test]
fn the_gas_tide_breathes_gas_out_of_the_pockets_at_high_tide() {
    let mut world = deep_world(0x06A5_71DE, 20);
    let row = middle_row(&world, Stratum::FossilOceans);
    room(
        &mut world,
        (FAR - 8, row - 5),
        (FAR + 8, row),
        TileKind::Stone,
    );
    for dx in 3..6 {
        set(&mut world, FAR + dx, row + 1, TileKind::Gas);
        set(&mut world, FAR + dx, row, TileKind::Air);
    }
    stand(&mut world, FAR - 4, row);
    // Just before the tide turns high.
    let hz = world.tick_rate_hz();
    let start = (old_deep::GAS_TIDE_HIGH.0 - 1_000) * u64::from(hz) / 1_000;
    world.set_simulation_tick(crate::ids::SimulationTick::new(start));
    let events = idle(&mut world, hz * 4);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::GasTideTurned { high: true }))
    );
    let clouds = world
        .entities()
        .iter()
        .filter(|entity| matches!(entity.state, EntityState::GasCloud(_)))
        .count();
    assert!(clouds > 0, "the pockets breathed");
}

#[test]
fn a_pressure_jet_throws_a_rig_up_on_its_cycle() {
    for hz in [30, 60] {
        let mut world = deep_world(0x0001_E7E7, hz);
        let row = middle_row(&world, Stratum::PressureCathedrals);
        room(
            &mut world,
            (FAR - 6, row - 16),
            (FAR + 6, row),
            TileKind::Stone,
        );
        set(&mut world, FAR, row + 1, deep(DeepTile::JetVent));
        stand(&mut world, FAR, row - 1);
        let vent = TilePosition { x: FAR, y: row + 1 };
        let mut thrown = false;
        for _ in 0..hz * 5 {
            let millis = old_deep::world_millis(world.simulation_tick(), hz);
            let _ = step(&mut world, ControlState::default());
            let rig = world.player(PLAYER).expect("rig");
            if old_deep::jet_active(vent, millis) && rig.velocity_y < -200.0 {
                thrown = true;
                break;
            }
        }
        assert!(thrown, "{hz} Hz: the jet threw the rig");
    }
}

#[test]
fn three_held_valves_restore_the_cathedral_lift() {
    let mut world = deep_world(0x0CA7_ED2A, 60);
    let site = world.cathedral_site().expect("a cathedral");
    let installed = world
        .logistics()
        .lifts
        .values()
        .find(|lift| lift.column == site.lift_column)
        .cloned()
        .expect("the Cathedral Lift is installed");
    assert!(!installed.restored);
    let mut opened = 0;
    let mut restored = false;
    for (index, valve) in site.valves.iter().enumerate() {
        // Stand beside the valve on its floor or ledge, on the side away from the walls.
        let offset = if index == 1 { -1 } else { 1 };
        stand(&mut world, valve.x + offset, valve.y);
        let _ = idle(&mut world, 2);
        let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
        assert!(
            (tile.x - valve.x).abs() <= 1 && (tile.y - valve.y).abs() <= 1,
            "valve {index}: the rig stands at {tile:?}, the valve at {valve:?}"
        );
        for event in idle(&mut world, 60 * 4) {
            match event {
                GameplayEventKind::CathedralValveOpened { .. } => opened += 1,
                GameplayEventKind::CathedralLiftRestored { .. } => restored = true,
                _ => {}
            }
        }
        assert_eq!(
            kind(&world, valve.x, valve.y),
            Some(deep(DeepTile::LiftValveOpen)),
            "valve {index}"
        );
    }
    assert_eq!(opened, 3);
    assert!(restored);
    assert!(world.old_deep().cathedral.restored);
    let shaft = world
        .logistics()
        .lifts
        .values()
        .find(|lift| lift.column == site.lift_column)
        .expect("lift");
    assert!(shaft.restored, "the lift runs");
}

#[test]
fn the_dose_meter_fills_near_glow_rock_warns_and_drains_away_from_it() {
    let mut world = deep_world(0x0D05_E000, 20);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .gas_scrubber = 0;
    let row = middle_row(&world, Stratum::RadioactiveHollow);
    room(
        &mut world,
        (FAR - 6, row - 3),
        (FAR + 6, row),
        deep(DeepTile::GlowRock),
    );
    stand(&mut world, FAR, row);
    let events = idle(&mut world, 20 * 30);
    let dose = world.old_deep().dose(PLAYER);
    assert!(dose.level >= 400, "{dose:?}");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::DoseWarning { level: 1, .. }))
    );
    // Gear keeps most of it out.
    let mut shielded = deep_world(0x0D05_E000, 20);
    shielded
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .gas_scrubber = 4;
    shielded
        .equipped_rig_parts
        .entry(PLAYER)
        .or_default()
        .insert(RigPartKind::PressureHull);
    room(
        &mut shielded,
        (FAR - 6, row - 3),
        (FAR + 6, row),
        deep(DeepTile::GlowRock),
    );
    stand(&mut shielded, FAR, row);
    let _ = idle(&mut shielded, 20 * 30);
    assert!(shielded.old_deep().dose(PLAYER).level * 2 < dose.level);
    // Out of the hollow it drains.
    let up = middle_row(&world, Stratum::CrystalFaults);
    room(
        &mut world,
        (FAR - 3, up - 3),
        (FAR + 3, up),
        TileKind::Stone,
    );
    stand(&mut world, FAR, up);
    let _ = idle(&mut world, 20 * 20);
    assert!(world.old_deep().dose(PLAYER).level < dose.level);
}

#[test]
fn a_mutation_relic_changes_the_rig_for_a_while() {
    let mut world = deep_world(0x0E1A_0001, 60);
    let row = middle_row(&world, Stratum::RadioactiveHollow);
    room(
        &mut world,
        (FAR - 4, row - 3),
        (FAR + 4, row),
        TileKind::Stone,
    );
    set(&mut world, FAR, row + 1, deep(DeepTile::MutationRelic));
    stand(&mut world, FAR, row);
    let mut gained = None;
    for _ in 0..600 {
        for event in step(&mut world, ControlState::new(0.0, false, true)) {
            if let GameplayEventKind::MutationGained { mutation, .. } = event {
                gained = Some(mutation);
            }
        }
        if gained.is_some() {
            break;
        }
    }
    let mutation = gained.expect("a mutation");
    assert_eq!(
        world.old_deep().mutation(PLAYER, world.simulation_tick()),
        Some(mutation)
    );
}

#[test]
fn a_machine_hall_wakes_and_its_belts_carry_a_rig() {
    let mut world = deep_world(0x0AC4_1E00, 60);
    let settings = *world.terrain().settings();
    let lay = layout(&settings);
    let (top, bottom) = lay.rows(Stratum::AncientMachineLayer);
    let (_, h) = sites::region_size(Stratum::AncientMachineLayer);
    let hall = (top / h..=bottom / h)
        .flat_map(|ry| (-20..20).map(move |rx| (rx, ry)))
        .find_map(|(rx, ry)| sites::machine_hall(&settings, &lay, rx, ry))
        .expect("a machine hall");
    // Stand on the first belt, clear of the doors.
    let x = hall.bounds.min.x + 2;
    stand(&mut world, x, hall.floor() - 1);
    let events = idle(&mut world, 3);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::MachineryWoke { .. }))
    );
    let belt = kind(&world, x, hall.floor());
    let way = match belt {
        Some(TileKind::Deep(DeepTile::ConveyorLeft)) => -1.0,
        Some(TileKind::Deep(DeepTile::ConveyorRight)) => 1.0,
        other => panic!("no belt under the rig: {other:?}"),
    };
    let start = world.player(PLAYER).expect("rig").x;
    let _ = idle(&mut world, 30);
    let moved = world.player(PLAYER).expect("rig").x - start;
    assert!(moved * way > 8.0, "the belt carried the rig {moved} px");
}

#[test]
fn a_heat_storm_scorches_open_caves_but_not_a_rig_dug_in() {
    let hz = 20;
    let damage = |open: bool| {
        let mut world = deep_world(0x0057_0A11, hz);
        let row = middle_row(&world, Stratum::MantleStormZone);
        if open {
            room(
                &mut world,
                (FAR - 6, row - 6),
                (FAR + 6, row),
                TileKind::Stone,
            );
        } else {
            room(&mut world, (FAR, row), (FAR, row), TileKind::Stone);
        }
        stand(&mut world, FAR, row);
        let start = (old_deep::STORM_BREAKS_AT + 1_000) * u64::from(hz) / 1_000;
        world.set_simulation_tick(crate::ids::SimulationTick::new(start));
        let hull = world.player(PLAYER).expect("rig").hull;
        let _ = idle(&mut world, hz * 5);
        hull - world.player(PLAYER).expect("rig").hull
    };
    let exposed = damage(true);
    let sheltered = damage(false);
    assert!(exposed > 5.0, "the storm scorched the open cave: {exposed}");
    assert!(
        sheltered < exposed * 0.25,
        "dug in: {sheltered} vs {exposed}"
    );
}

/// A simple pilot for the boss test: flies the rig to stand on tile `(x, y)` through a lane
/// above the arena's floor clutter, then drills `drill` (horizontal, down) once there.
struct Pilot {
    lane_row: i32,
}

impl Pilot {
    fn controls(
        &self,
        world: &WorldState,
        stand_at: TilePosition,
        drill: (f32, bool),
    ) -> ControlState {
        let frame = world.frame();
        let rig = world.player(PLAYER).expect("rig");
        let (gx, gy) = frame.tile_center(stand_at);
        let lane_y = frame
            .tile_center(TilePosition {
                x: stand_at.x,
                y: self.lane_row,
            })
            .1;
        let tile = rig.tile_in(frame);
        let dx = gx - rig.x;
        let at_column = dx.abs() < 6.0;
        if tile == stand_at && rig.velocity_y.abs() < 1.0 {
            return ControlState::new(drill.0, false, drill.1);
        }
        let damp = rig.velocity_x / 220.0;
        let horizontal = (dx / 40.0 - damp).clamp(-1.0, 1.0);
        if !at_column {
            // Climb to the lane, then cross along it.
            let thrust = rig.y > lane_y + 4.0 || rig.velocity_y > 40.0;
            let horizontal = if rig.y > lane_y + 24.0 && tile.y > self.lane_row + 1 {
                horizontal * 0.2
            } else {
                horizontal
            };
            return ControlState::new(horizontal, thrust, false);
        }
        // Over the stand: drop onto it, braking a fast fall.
        let thrust = rig.velocity_y > 220.0 || (gy - rig.y) < -8.0;
        ControlState::new(horizontal, thrust, false)
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "the gate, the whole fight and the gate again"
)]
fn a_bot_with_descent_two_kit_beats_the_star_core_and_wins_the_pressure_hull() {
    let hz = 30;
    let mut world = deep_world(0x057A_4C04, hz);
    let arena = world.star_core_arena().expect("an arena");
    let floor = arena.floor();
    let b = arena.bounds;
    // The gate holds before the fight: Descent III crushes a rig without the Pressure Hull.
    let lay = layout(world.terrain().settings());
    let below = lay.rows(Descent::LivingDark.first_stratum()).0 + 30;
    room(
        &mut world,
        (FAR - 3, below - 3),
        (FAR + 3, below),
        TileKind::Stone,
    );
    stand(&mut world, FAR, below);
    let hull = world.player(PLAYER).expect("rig").hull;
    let events = idle(&mut world, hz * 3);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::DescentGateBlocked { .. }))
    );
    assert!(world.player(PLAYER).expect("rig").hull < hull);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull();
    }

    // Into the arena, on the floor left of the core.
    stand(&mut world, arena.core.x - 6, floor - 1);
    let pilot = Pilot {
        lane_row: b.min.y + 4,
    };
    let stands = [
        (
            TilePosition {
                x: arena.pylons[0].x + 1,
                y: floor - 1,
            },
            (-1.0, false),
            arena.pylons[0],
        ),
        (
            TilePosition {
                x: arena.pylons[2].x + 1,
                y: arena.pylons[2].y,
            },
            (-1.0, false),
            arena.pylons[2],
        ),
        (
            TilePosition {
                x: arena.pylons[3].x - 1,
                y: arena.pylons[3].y,
            },
            (1.0, false),
            arena.pylons[3],
        ),
        (
            TilePosition {
                x: arena.pylons[1].x - 1,
                y: floor - 1,
            },
            (1.0, false),
            arena.pylons[1],
        ),
    ];
    let core_stand = TilePosition {
        x: arena.core.x,
        y: arena.core.y - 1,
    };
    let mut beaten = false;
    let mut phases = Vec::new();
    let mut ticks = 0;
    while !beaten && ticks < hz * 600 {
        ticks += 1;
        let pylon_target = stands.iter().find(|(_, _, pylon)| {
            kind(&world, pylon.x, pylon.y) == Some(deep(DeepTile::CorePylon))
        });
        let exposed = kind(&world, arena.core.x, arena.core.y) == Some(deep(DeepTile::FirstCore));
        let controls = match (exposed, pylon_target) {
            (true, _) => pilot.controls(&world, core_stand, (0.0, true)),
            (false, Some((stand_at, drill, _))) => pilot.controls(&world, *stand_at, *drill),
            (false, None) => ControlState::default(),
        };
        for event in step(&mut world, controls) {
            match event {
                GameplayEventKind::StarCorePhase { phase, .. } => phases.push(phase),
                GameplayEventKind::StarCoreBeaten { .. } => beaten = true,
                _ => {}
            }
        }
        // Keep the rig alive and fuelled like a player with repair kits would.
        let rig = world.player_mut(PLAYER).expect("rig");
        assert!(
            rig.hull > 0.0,
            "the bot's rig was wrecked after {ticks} ticks: {phases:?}"
        );
        rig.fuel = rig.fuel_capacity;
    }
    assert!(
        beaten,
        "the bot beat the core within 10 minutes: phases {phases:?}, rig at {:?}",
        world.player(PLAYER).expect("rig").tile_in(world.frame())
    );
    assert!(phases.contains(&CorePhase::Exposed));
    assert_eq!(world.old_deep().core.phase, CorePhase::Beaten);
    assert_eq!(
        kind(&world, arena.core.x, arena.core.y),
        Some(deep(DeepTile::FirstCoreSpent))
    );
    assert!(
        world
            .old_deep()
            .logs_read
            .contains(&old_deep::LOG_CORE_REVEAL)
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.hazard_gear.has_system(RigSystem::PressureHull));

    // And now the gate lets it pass.
    let mut rig_world = world;
    room(
        &mut rig_world,
        (FAR - 3, below - 3),
        (FAR + 3, below),
        TileKind::Stone,
    );
    stand(&mut rig_world, FAR, below);
    let hull = rig_world.player(PLAYER).expect("rig").hull;
    let events = idle(&mut rig_world, hz * 3);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::DescentGateBlocked { .. }))
    );
    assert!(rig_world.player(PLAYER).expect("rig").hull >= hull - 1.0);
}

#[test]
fn the_star_core_sleeps_again_when_the_arena_empties() {
    let hz = 20;
    let mut world = deep_world(0x057A_4C05, hz);
    let arena = world.star_core_arena().expect("an arena");
    stand(&mut world, arena.core.x - 6, arena.floor() - 1);
    let events = idle(&mut world, 4);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::StarCoreAwoke))
    );
    // Knock a pylon out, then leave.
    set(
        &mut world,
        arena.pylons[0].x,
        arena.pylons[0].y,
        TileKind::Air,
    );
    let away = middle_row(&world, Stratum::CrystalFaults);
    room(
        &mut world,
        (FAR - 3, away - 3),
        (FAR + 3, away),
        TileKind::Stone,
    );
    stand(&mut world, FAR, away);
    let _ = idle(&mut world, hz * 12);
    assert_eq!(world.old_deep().core.phase, CorePhase::Dormant);
}

#[test]
fn the_geode_field_pulls_out_to_the_shell_in_the_world() {
    let world = deep_world(0x0060_DE00, 60);
    let settings = *world.terrain().settings();
    let lay = layout(&settings);
    let (top, bottom) = lay.rows(Stratum::VoidGeodeFields);
    let (_, h) = sites::region_size(Stratum::VoidGeodeFields);
    let geode = (top / h..=bottom / h)
        .flat_map(|ry| (-20..20).map(move |rx| (rx, ry)))
        .find_map(|(rx, ry)| sites::geode(&settings, &lay, rx, ry))
        .expect("a geode");
    let frame = world.frame();
    let (cx, cy) = frame.tile_center(geode.centre);
    let right = world.fields_at(TILE_SIZE.mul_add(4.0, cx), cy);
    assert!(right.gravity_x > 0.1, "{right:?}");
    assert!(right.rotation_unlimited());
    #[allow(clippy::cast_precision_loss, reason = "a small radius")]
    let outside = world.fields_at(TILE_SIZE.mul_add(geode.radius as f32 + 3.0, cx), cy);
    assert!(outside.gravity_x.abs() < 0.01);
}
