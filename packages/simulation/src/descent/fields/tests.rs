//! The stratum registry feeding the field-effects track (Wave 7 integration).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "test rows and blend widths are small"
)]

use super::*;
use crate::{
    ControlState, PlayerId, SimulationParams, TickInputs, WorldState,
    attitude::BASE_TILT_LIMIT,
    enemy::EnemyDifficulty,
    field::{FieldSample, trig::wrap_angle},
    position::WorldPos,
    terrain::{TileKind, TilePosition},
    worldgen::GeneratorVersion,
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// The field sample at the middle of absolute tile `(column, row)`, read through `frame`.
fn sample_at(settings: &WorldSettings, frame: Frame, column: i32, row: i32) -> FieldSample {
    let (x, y) = frame.tile_center(TilePosition { x: column, y: row });
    field::sample_in(&field::FieldZones::default(), settings, frame, x, y)
}

fn middle_row(settings: &WorldSettings, stratum: Stratum) -> i32 {
    let (top, bottom) = cached_layout(settings).rows(stratum);
    i32::midpoint(top, bottom)
}

#[test]
fn older_worlds_the_sky_and_quiet_strata_stay_neutral() {
    let frame = Frame::default();
    for generator in GeneratorVersion::ALL {
        if generator.has_descents() {
            continue;
        }
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(9)
        };
        for row in [-40, 0, 100, 2_000, 9_000] {
            assert_eq!(sample_at(&settings, frame, 30, row), FieldSample::NEUTRAL);
        }
    }
    let settings = v11(9);
    // The sky, the town and the classic Crust strata.
    for row in [-60, -2, 3, middle_row(&settings, Stratum::GreyStone)] {
        assert_eq!(
            stratum_field_source(&settings, frame, 40.0, row as f32 * TILE_SIZE),
            None,
            "row {row}"
        );
    }
}

#[test]
fn placeholder_strata_show_their_effects_on_screen() {
    let settings = v11(21);
    let frame = Frame::default();
    let at = |stratum| sample_at(&settings, frame, 30, middle_row(&settings, stratum));
    // The Hollow World's inner sun reads as exposure; the Sunforge burns brighter still.
    let sky = at(Stratum::InnerSky);
    let forge = at(Stratum::TheSunforge);
    assert!(sky.screen_level(field::ScreenEffect::Exposure) > 0.1);
    assert!(
        forge.screen_level(field::ScreenEffect::Exposure)
            > sky.screen_level(field::ScreenEffect::Exposure)
    );
    assert!(forge.screen_level(field::ScreenEffect::Bloom) > 0.3);
    // The Glow Sea is dark, the Pearl Abyss desaturates, the Prism Desert splits colours.
    assert!(at(Stratum::GlowSea).light < 0.3);
    // The Pearl Abyss greys from its top, and more with the pressure on each seat (Wave 9).
    assert!(at(Stratum::PearlAbyss).screen_level(field::ScreenEffect::Desaturate) > 0.15);
    assert!(at(Stratum::PrismDesert).screen_level(field::ScreenEffect::Chromatic) > 0.4);
    // Pixel Rot deepens: half strength at its top, full at its bottom.
    let (top, bottom) = cached_layout(&settings).rows(Stratum::PixelRot);
    let high = sample_at(&settings, frame, 30, top + BLEND_HALF_ROWS as i32 + 2);
    let low = sample_at(&settings, frame, 30, bottom - BLEND_HALF_ROWS as i32 - 2);
    let pixelate = field::ScreenEffect::Pixelate;
    assert!(high.screen_level(pixelate) > 0.15 && high.screen_level(pixelate) < 0.3);
    assert!(low.screen_level(pixelate) > 0.35);
    // Physics: the sludge slows time. Since Wave 9 the ocean's water is the submarine's to
    // handle and the glaciers' slide is their ice's (`drowned::is_ice`), not field drag or
    // friction (`world::drowned_tests` holds both).
    assert!(at(Stratum::UndergroundOcean).drag >= 1.0);
    assert!(crate::drowned::is_ice(Stratum::BrineGlaciers.def().soft));
    assert!(at(Stratum::TimeSludge).time_scale < 0.9);
}

#[test]
fn strange_strata_turn_gravity_and_mirror_in_regions_and_spin_freely() {
    let settings = v11(5);
    let frame = Frame::default();
    let scan = |stratum: Stratum| {
        let row = middle_row(&settings, stratum);
        (row - 48..row + 48)
            .step_by(4)
            .flat_map(|row| {
                (-1_200..1_200)
                    .step_by(8)
                    .map(move |column| sample_at(&settings, frame, column, row))
            })
            .collect::<Vec<_>>()
    };
    let shear = scan(Stratum::GravityShear);
    assert!(shear.iter().all(FieldSample::rotation_unlimited));
    assert!(shear.iter().any(|sample| sample.gravity_x > 0.9));
    assert!(shear.iter().any(|sample| sample.gravity_x < -0.9));
    assert!(shear.iter().any(|sample| sample.gravity_y < -0.9));
    assert!(
        shear
            .iter()
            .any(|sample| (sample.gravity_y - 1.0).abs() < 1e-6)
    );
    let isles = scan(Stratum::UpsideDownIsles);
    assert!(isles.iter().all(FieldSample::rotation_unlimited));
    assert!(isles.iter().any(|sample| sample.gravity_y < -0.9));
    let mirror = scan(Stratum::MirrorStrata);
    assert!(mirror.iter().any(FieldSample::mirrored));
    assert!(mirror.iter().any(|sample| !sample.mirrored()));
    assert!(
        mirror
            .iter()
            .filter(|sample| sample.mirrored())
            .all(|sample| sample.screen_level(field::ScreenEffect::Mirror) >= 0.45)
    );
    // The home world's strata keep the lean limit.
    let topsoil = sample_at(
        &settings,
        frame,
        30,
        middle_row(&settings, Stratum::ClayBeds),
    );
    assert!(!topsoil.rotation_unlimited());
}

#[test]
fn fields_blend_smoothly_across_a_stratum_boundary() {
    let settings = v11(77);
    let frame = Frame::default();
    // The Pearl Abyss (dim, grey) gives way to the Inner Sky (bright).
    let boundary = cached_layout(&settings).rows(Stratum::InnerSky).0;
    let mut previous = sample_at(&settings, frame, 30, boundary - 20).light;
    for row in boundary - 20..boundary + 20 {
        let light = sample_at(&settings, frame, 30, row).light;
        assert!(
            (light - previous).abs() < 0.08,
            "row {row}: {previous} -> {light}"
        );
        previous = light;
    }
}

#[test]
fn the_source_reads_the_same_field_in_any_frame() {
    let settings = v11(31);
    let row = middle_row(&settings, Stratum::GravityShear);
    for column in (-400..400).step_by(37) {
        let origin = sample_at(&settings, Frame::default(), column, row);
        let floating = sample_at(
            &settings,
            Frame::floating_at(-1, i64::from(row / 512)),
            column,
            row,
        );
        let far = sample_at(
            &settings,
            Frame::floating_at(3, i64::from(row / 512 + 1)),
            column,
            row,
        );
        assert_eq!(origin, floating, "column {column}");
        assert_eq!(origin, far, "column {column}");
    }
}

/// Carves an open shaft around tile `(column, row)` and drops a stock rig into it (not a
/// test-mode one: test mode fits every tilt rating).
fn rig_in_shaft(settings: &WorldSettings, column: i32, row: i32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, settings, SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    for y in row - 6..row + 100 {
        for x in column - 20..=column + 20 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(column), i64::from(row)),
    );
    world
}

/// How far `world`'s rig turns in `ticks` holding the clockwise rotation thrusters.
fn turned(world: &mut WorldState, ticks: u32) -> f32 {
    let inputs = TickInputs::held(
        PLAYER,
        ControlState::new(0.0, false, false).with_rotation(1.0),
    );
    let mut turned = 0.0;
    let mut last = world.player(PLAYER).expect("rig").attitude.orientation;
    for _ in 0..ticks {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
        let now = world.player(PLAYER).expect("rig").attitude.orientation;
        turned += wrap_angle(now - last);
        last = now;
    }
    turned
}

#[test]
fn a_rig_spins_all_the_way_round_in_the_gravity_shear_and_leans_only_at_home() {
    let settings = v11(5);
    let frame = Frame::default();
    let shear_row = middle_row(&settings, Stratum::GravityShear);
    // A column whose region keeps gravity down, so the rig falls down the shaft.
    let column = (-4_800..4_800)
        .step_by(24)
        .find(|column| {
            (shear_row - 10..shear_row + 100).all(|row| {
                let sample = sample_at(&settings, frame, *column, row);
                (sample.gravity_y - 1.0).abs() < 1e-6 && sample.gravity_x == 0.0
            })
        })
        .expect("a column without a gravity turn");
    let mut deep = rig_in_shaft(&settings, column, shear_row);
    assert!(deep.fields_for(PLAYER).rotation_unlimited());
    let spun = turned(&mut deep, 150);
    assert!(spun > std::f32::consts::PI, "spun {spun}");
    // The same thrusters on the home world's Clay Beds lean only to the limit.
    let clay_row = middle_row(&settings, Stratum::ClayBeds);
    let mut home = rig_in_shaft(&settings, column, clay_row);
    let leaned = turned(&mut home, 150);
    assert!(leaned <= BASE_TILT_LIMIT + 0.02, "leaned {leaned}");
    assert!(leaned > BASE_TILT_LIMIT - 0.1, "leaned {leaned}");
}

#[test]
fn every_registry_screen_effect_has_a_pass() {
    // Since the Wave 8 blur pass every registry effect maps to one.
    for effect in ScreenEffect::ALL {
        assert!(screen_effect(effect).is_some(), "{effect:?}");
    }
}
