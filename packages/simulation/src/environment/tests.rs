use super::*;
use crate::worldgen::{GeneratorVersion, sky};

fn v8() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(42)
    }
}

/// World pixel y of `altitude_rows` rows above row 0.
#[allow(clippy::cast_precision_loss, reason = "small altitudes")]
fn y_of(altitude_rows: i32) -> f32 {
    -(altitude_rows as f32) * TILE_SIZE
}

/// A column far from the Moon and with no updraft at `altitude_rows`.
fn calm_column(settings: &WorldSettings, altitude_rows: i32) -> f32 {
    let moon = sky::moon_column(settings).expect("a moon");
    let away = if moon > 0 { -1 } else { 1 };
    (0..2_000)
        .map(|step| away * (2_000 + step * 5))
        .find(|x| sky::updraft_at(settings, *x, sky::row_of(altitude_rows)) == 0)
        .map(|x| {
            #[allow(clippy::cast_precision_loss, reason = "small columns")]
            let x = x as f32;
            (x + 0.5) * TILE_SIZE
        })
        .expect("a calm column")
}

#[test]
fn the_ground_and_old_worlds_are_standard() {
    let settings = v8();
    for altitude in [-800, -5, 0, 60, STANDARD_BELOW_ROWS - 1] {
        assert!(environment_at(&settings, 400.0, y_of(altitude)).is_standard());
    }
    let v7 = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(42)
    };
    for altitude in [0, 500, 5_000, 12_000] {
        assert!(environment_at(&v7, 400.0, y_of(altitude)).is_standard());
    }
}

#[test]
fn air_thins_with_altitude_and_is_gone_in_orbit() {
    let mut previous = 1.0;
    for altitude in (0..sky::ORBIT_BOTTOM_ROWS + 400).step_by(50) {
        #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
        let density = air_density(altitude as f32);
        assert!(density <= previous, "at {altitude}");
        assert!((0.0..=1.0).contains(&density));
        previous = density;
    }
    assert!((air_density(150.0) - 1.0).abs() < f32::EPSILON);
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let orbit = sky::ORBIT_BOTTOM_ROWS as f32;
    assert!(
        air_density(orbit + VACUUM_FADE_ROWS).abs() < f32::EPSILON,
        "vacuum"
    );
    assert!(air_density(800.0) < 0.75 && air_density(800.0) > 0.4);
}

#[test]
fn intakes_recover_thrust_in_thin_air() {
    let thin = Environment {
        air_density: 0.2,
        ..Environment::STANDARD
    };
    let mut previous = 0.0;
    for level in 0..=ALTITUDE_INTAKE_MAX_LEVEL {
        let efficiency = thin.thrust_efficiency(level);
        assert!(efficiency > previous, "tier {level}");
        previous = efficiency;
    }
    assert!((thin.thrust_efficiency(0) - 0.2).abs() < 1e-6);
    assert!((Environment::STANDARD.thrust_efficiency(0) - 1.0).abs() < f32::EPSILON);
    // Thin air burns more fuel; intakes bring it back down.
    assert!(
        Environment::burn_factor(thin.thrust_efficiency(0))
            > Environment::burn_factor(thin.thrust_efficiency(ALTITUDE_INTAKE_MAX_LEVEL))
    );
    assert!((Environment::burn_factor(1.0) - 1.0).abs() < f32::EPSILON);
    // Vacuum drags nothing.
    let vacuum = Environment {
        air_density: 0.0,
        ..Environment::STANDARD
    };
    assert!((vacuum.drag_retention(0.98) - 1.0).abs() < f32::EPSILON);
    assert!((Environment::STANDARD.drag_retention(0.98) - 0.98).abs() < 1e-6);
}

#[test]
fn cold_builds_up_and_the_de_icer_covers_it() {
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let at = |rows: i32| cold_level(rows as f32);
    assert_eq!(
        at(sky::LOW_ISLES_BOTTOM_ROWS + 50),
        0,
        "the low isles are mild"
    );
    let mut previous = 0;
    for altitude in (0..sky::MOON_BOTTOM_ROWS + 500).step_by(100) {
        let cold = at(altitude);
        assert!(cold >= previous, "at {altitude}");
        previous = cold;
    }
    assert!(at(sky::DEAD_AIR_BOTTOM_ROWS + 100) >= 4);
    assert_eq!(at(sky::MOON_BOTTOM_ROWS + 500), MAX_COLD);
    assert_eq!(cold_excess(MAX_COLD, 0), MAX_COLD);
    assert_eq!(
        cold_excess(MAX_COLD, DE_ICER_MAX_LEVEL),
        0,
        "top tier: no cold"
    );
    assert_eq!(cold_excess(5, 2), 1);
}

#[test]
fn gravity_fields_follow_the_bands_and_the_moon() {
    let settings = v8();
    let x = calm_column(&settings, 300);
    #[allow(clippy::cast_possible_truncation, reason = "pixel to tile")]
    let column = (x / TILE_SIZE).floor() as i32;
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let g = |rows: i32| gravity_scale(&settings, column, rows as f32);
    assert!(
        (g(300) - 1.0).abs() < f32::EPSILON,
        "islands: normal gravity"
    );
    assert!((g(sky::DEAD_AIR_BOTTOM_ROWS + 50) - 1.0).abs() < f32::EPSILON);
    assert!((g(sky::CONTINENTS_BOTTOM_ROWS + 300) - CONTINENT_GRAVITY).abs() < 1e-6);
    assert!(
        (g(sky::ORBIT_BOTTOM_ROWS + 300) - ORBIT_GRAVITY).abs() < 1e-6,
        "orbit"
    );
    // On the Moon (above its column) gravity is low but real.
    let moon = sky::moon(&settings).expect("a moon");
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let on_moon = gravity_scale(
        &settings,
        moon.center_x,
        (sky::MOON_BOTTOM_ROWS + sky::MOON_DEPTH_ROWS + 4) as f32,
    );
    assert!((on_moon - MOON_GRAVITY).abs() < 1e-6, "{on_moon}");
    // Far beside it, orbit's near-zero gravity.
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let beside = gravity_scale(
        &settings,
        moon.center_x + moon.half_width + MOON_HALO_COLUMNS + 50,
        (sky::MOON_BOTTOM_ROWS + 100) as f32,
    );
    assert!((beside - ORBIT_GRAVITY).abs() < 1e-6, "{beside}");
    // Smooth: no jump bigger than a few percent between neighbouring rows.
    let mut previous = g(0);
    for rows in 0..sky::ORBIT_BOTTOM_ROWS + 300 {
        let now = g(rows);
        assert!((now - previous).abs() < 0.02, "a jump at {rows}");
        previous = now;
    }
}

#[test]
fn wind_blows_in_layers_harder_in_the_gap_and_never_on_the_ground() {
    let settings = v8();
    assert!(wind_at(&settings, 50, 100.0).abs() < f32::EPSILON);
    let peak = |from: i32, to: i32| {
        (from..to)
            .step_by(8)
            .map(|rows| {
                #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
                let rows = rows as f32;
                wind_at(&settings, 50, rows).abs()
            })
            .fold(0.0_f32, f32::max)
    };
    let islands = peak(sky::LOW_ISLES_BOTTOM_ROWS, sky::ARCHIPELAGO_BOTTOM_ROWS);
    let gap = peak(
        sky::DEAD_AIR_BOTTOM_ROWS + 64,
        sky::CONTINENTS_BOTTOM_ROWS - 64,
    );
    assert!(islands > 10.0, "a breeze among the isles: {islands}");
    assert!(
        gap > islands * 2.0,
        "crosswinds in the gap: {gap} vs {islands}"
    );
    #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
    let orbit = sky::ORBIT_BOTTOM_ROWS as f32 + 400.0;
    assert!(
        wind_at(&settings, 50, orbit).abs() < f32::EPSILON,
        "no wind in vacuum"
    );
    // Layers blow both ways somewhere in the archipelago.
    let winds: Vec<f32> = (sky::ARCHIPELAGO_BOTTOM_ROWS..sky::DEAD_AIR_BOTTOM_ROWS)
        .step_by(WIND_LAYER_ROWS as usize)
        .map(|rows| {
            #[allow(clippy::cast_precision_loss, reason = "small altitudes")]
            let rows = rows as f32 + 32.0;
            wind_at(&settings, 50, rows)
        })
        .collect();
    assert!(winds.iter().any(|w| *w > 5.0) && winds.iter().any(|w| *w < -5.0));
    // Deterministic.
    assert_eq!(
        wind_at(&settings, 777, 1_234.5).to_bits(),
        wind_at(&settings, 777, 1_234.5).to_bits()
    );
}

#[test]
fn environment_at_combines_every_field() {
    let settings = v8();
    let x = calm_column(&settings, sky::DEAD_AIR_BOTTOM_ROWS + 300);
    let gap = environment_at(&settings, x, y_of(sky::DEAD_AIR_BOTTOM_ROWS + 300));
    assert!(!gap.is_standard());
    assert!(gap.air_density < 0.2);
    assert!(gap.cold >= 4);
    assert!(gap.updraft.abs() < f32::EPSILON);
    assert!((gap.gravity - 1.0).abs() < f32::EPSILON);
    let body = gap.body_gravity(560.0);
    assert!((body - 560.0).abs() < 1e-3);
    // In still air (the climb harness) there is no wind, updraft or vacuum.
    let still = with_still_air(|| environment_at(&settings, x, y_of(sky::ORBIT_BOTTOM_ROWS + 500)));
    assert!(still.wind_x.abs() < f32::EPSILON);
    assert!((still.gravity - 1.0).abs() < f32::EPSILON);
    assert!(still.air_density > 0.0);
}
