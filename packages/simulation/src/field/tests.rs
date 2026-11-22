use super::*;

const T: f32 = TILE_SIZE;

fn zone(preset: FieldPreset) -> FieldZone {
    FieldZone::of_preset(preset, (100, 40), (6, 4))
}

#[test]
fn the_world_is_neutral_everywhere_without_zones_or_strata() {
    let settings = WorldSettings::default();
    let zones = FieldZones::default();
    for (x, y) in [(0.0, 0.0), (3_200.0, 9_000.0), (-50_000.0, -40_000.0)] {
        let sample = sample(&zones, &settings, x, y);
        assert_eq!(sample, FieldSample::NEUTRAL);
        assert!(sample.is_neutral_physics());
        assert!(!sample.rotation_unlimited());
        assert_eq!(sample.down_angle(), Some(0.0));
    }
    assert!(stratum_profile_at(&settings, crate::position::Frame::default(), 0.0, 0.0).is_none());
}

#[test]
fn a_zone_applies_fully_inside_and_not_at_all_outside() {
    let settings = WorldSettings::default();
    let mut zones = FieldZones::default();
    zones.insert(zone(FieldPreset::GravityFlip));
    let inside = sample(&zones, &settings, 100.5 * T, 40.5 * T);
    assert!((inside.gravity_y + 1.0).abs() < 1e-6, "{inside:?}");
    assert!(inside.rotation_unlimited());
    let outside = sample(&zones, &settings, 120.5 * T, 40.5 * T);
    assert_eq!(outside, FieldSample::NEUTRAL);
}

#[test]
fn zone_edges_blend_smoothly_and_monotonically() {
    let settings = WorldSettings::default();
    let mut zones = FieldZones::default();
    zones.insert(zone(FieldPreset::SlowTime));
    // Walk in from outside the left edge (tile 94) to the middle (tile 100), a pixel at a
    // time: time slows steadily, never jumps by more than a sliver per pixel.
    let mut previous = 1.0_f32;
    let mut x = 92.0 * T;
    while x <= 100.0 * T {
        let scale = sample(&zones, &settings, x, 40.5 * T).time_scale;
        assert!(scale <= previous + 1e-6, "time scale rises at {x}");
        assert!(
            previous - scale < 0.01,
            "jump at {x}: {previous} -> {scale}"
        );
        previous = scale;
        x += 1.0;
    }
    assert!((previous - 0.5).abs() < 1e-6);
    // The screen effect fades in with it.
    let edge = sample(&zones, &settings, 95.0 * T, 40.5 * T);
    let deep = sample(&zones, &settings, 100.0 * T, 40.5 * T);
    let desaturate = ScreenEffect::Desaturate;
    assert!(edge.screen_level(desaturate) < deep.screen_level(desaturate));
    assert!((deep.screen_level(desaturate) - 0.55).abs() < 1e-6);
}

#[test]
fn overlapping_zones_blend_in_id_order() {
    let settings = WorldSettings::default();
    let mut zones = FieldZones::default();
    zones.insert(zone(FieldPreset::SlowTime));
    zones.insert(zone(FieldPreset::Mirror));
    let both = sample(&zones, &settings, 100.5 * T, 40.5 * T);
    assert!((both.time_scale - 0.5).abs() < 1e-6);
    assert!(both.mirrored());
    assert!((both.screen_level(ScreenEffect::Mirror) - 1.0).abs() < 1e-6);
}

#[test]
fn boundary_blend_and_profile_lerp_for_stratum_sources() {
    assert!(boundary_blend(0.0, 100.0, 8.0).abs() < f32::EPSILON);
    assert!((boundary_blend(100.0, 100.0, 8.0) - 0.5).abs() < 1e-6);
    assert!((boundary_blend(200.0, 100.0, 8.0) - 1.0).abs() < f32::EPSILON);
    let a = FieldProfile::NEUTRAL;
    let b = FieldPreset::PixelRot.profile();
    let half = a.lerp(&b, 0.5);
    assert_eq!(half.screen.level(ScreenEffect::Pixelate), 35);
    assert_eq!(a.lerp(&b, 0.0), a);
    assert_eq!(a.lerp(&b, 1.0), b);
}

#[test]
fn sideways_and_weightless_fields_report_their_frames() {
    let shear = FieldPreset::GravityShear.profile().sample();
    let angle = shear.down_angle().unwrap_or_default();
    // Gravity to the right: down = (-sin a, cos a) = (1, 0), so a = -pi/2.
    assert!(
        (angle + std::f32::consts::FRAC_PI_2).abs() < 1e-4,
        "{angle}"
    );
    assert!(shear.rotation_unlimited());
    let weightless = FieldPreset::Weightless.profile().sample();
    assert!(weightless.down_angle().is_none());
    assert!(weightless.rotation_unlimited());
}

#[test]
fn zones_are_capped_and_cleared() {
    let mut zones = FieldZones::default();
    for _ in 0..(MAX_FIELD_ZONES + 5) {
        zones.insert(zone(FieldPreset::Dark));
    }
    assert_eq!(zones.iter().count(), MAX_FIELD_ZONES);
    zones.clear();
    assert!(zones.is_empty());
}
