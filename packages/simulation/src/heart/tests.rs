//! Descent VIII's pure rules: tiles, dark matter's weight, the fight's rhythms.

use super::*;

#[test]
fn core_tiles_round_trip_their_codes_and_the_hard_ones_never_break() {
    for (index, tile) in CoreTile::ALL.iter().enumerate() {
        assert_eq!(tile.code(), index as u64);
        assert_eq!(CoreTile::from_code(tile.code()), Some(*tile));
        let kind = crate::terrain::TileKind::Core(*tile);
        assert_eq!(
            crate::terrain::is_indestructible(kind),
            tile.indestructible(),
            "{tile:?}"
        );
    }
    assert_eq!(CoreTile::from_code(CoreTile::ALL.len() as u64), None);
    // The layer resources come from their own tiles.
    assert_eq!(
        CoreTile::DarkMatter.item(),
        Some(ItemKind::Layer(Stratum::DarkMatterVeins))
    );
    assert_eq!(
        CoreTile::StarShard.item(),
        Some(ItemKind::Layer(Stratum::TheHeart))
    );
}

#[test]
fn dark_matter_lightens_a_rig_and_ore_weighs_it_back_down() {
    // No dark matter: the classic weight exactly, however full the bay.
    assert_eq!(cargo_mass_permille(0, 0), 1_000);
    assert_eq!(cargo_mass_permille(0, 80), 1_000);
    // Every lump lightens it.
    assert!(cargo_mass_permille(2, 0) < cargo_mass_permille(1, 0));
    assert_eq!(cargo_mass_permille(WEIGHTLESS_LUMPS, 0), 0);
    // Too much and it falls up.
    assert!(cargo_mass_permille(WEIGHTLESS_LUMPS + 2, 0) < 0);
    // Ordinary ore weighs it back down: plan the load.
    assert!(cargo_mass_permille(WEIGHTLESS_LUMPS + 2, 20) > 0);
    // Bounded both ways.
    assert_eq!(cargo_mass_permille(64, 0), MASS_MIN_PERMILLE);
    assert_eq!(cargo_mass_permille(1, 200), MASS_MAX_PERMILLE);
}

#[test]
fn the_fights_rhythms_tell_before_they_bite() {
    assert_eq!(pulse_at(0), PulseState::Calm);
    assert_eq!(pulse_at(PULSE_FIRST_MILLIS), PulseState::Tell);
    assert_eq!(
        pulse_at(PULSE_FIRST_MILLIS + PULSE_TELL_MILLIS),
        PulseState::Pull
    );
    assert_eq!(
        pulse_at(PULSE_FIRST_MILLIS + PULSE_TELL_MILLIS + PULSE_PULL_MILLIS),
        PulseState::Calm
    );
    // Every pull comes after a full tell.
    let mut last = PulseState::Calm;
    for t in (0..120_000).step_by(100) {
        let now = pulse_at(t);
        if now == PulseState::Pull {
            assert_ne!(last, PulseState::Calm, "no pull without a tell at {t}");
        }
        last = now;
    }
    assert_eq!(breath_at(0), BreathState::Held);
    assert_eq!(breath_at(EXHALE_TELL_AT_MILLIS), BreathState::Tell);
    assert_eq!(breath_at(EXHALE_AT_MILLIS), BreathState::Exhale);
    assert_eq!(breath_at(BREATH_PERIOD_MILLIS), BreathState::Held);
    // The beams fade in before they burn.
    assert!(beam_strength(0) < 0.01);
    assert!((beam_strength(BEAM_TELL_MILLIS) - 1.0).abs() < 1e-6);
    // Six coils evenly round the ring.
    let a0 = coil_angle(0, 0);
    let a1 = coil_angle(1, 0);
    let step = crate::field::trig::wrap_angle(a1 - a0).abs();
    assert!((step - std::f32::consts::TAU / 6.0).abs() < 0.01, "{step}");
}

#[test]
fn the_crew_state_reports_space_and_the_ending_shield() {
    let mut heart = Heart::default();
    assert!(heart.is_empty());
    let player = PlayerId::new(3);
    assert!(!heart.space_access(player).may_leave());
    heart.space_unlocked = true;
    heart.star_drive.insert(player);
    heart.cable_lit_millis = Some(10_000);
    assert!(heart.space_access(player).may_leave());
    assert!(!heart.space_access(PlayerId::new(4)).may_leave());
    assert!(!heart.ending_shields(9_000));
    assert!(heart.ending_shields(10_000));
    assert!(heart.ending_shields(10_000 + ENDING_SHIELD_MILLIS - 1));
    assert!(!heart.ending_shields(10_000 + ENDING_SHIELD_MILLIS));
    // The Star Drive is the crew's: it stays when the player leaves.
    heart.forget(player);
    assert!(heart.has_star_drive(player));
    assert!(!heart.is_empty());
    assert!(heart.find(HeartLandmark::Lens));
    assert!(!heart.find(HeartLandmark::Lens));
}
