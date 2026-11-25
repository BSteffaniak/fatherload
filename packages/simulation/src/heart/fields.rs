//! The Singularity Fields' gravity wells as a field source.
//!
//! Round each well ([`crate::worldgen::descent::heart::Well`]) gravity turns toward its point
//! and grows the closer a body gets: at the well's reach it is the stratum's own pull, at the
//! point [`WELL_PULL_G`] standard gravities straight in. Rigs, pilots, creatures, drops and
//! charges all read it (they read every field), so everything loose falls into a well. Rotation
//! is free once the pull leans hard ([`FREE_TURN_WEIGHT`]); the player turns the rig with its
//! thrusters. The light bends: the lensing screen effect grows toward the point. Registered
//! before the stratum source in [`crate::field::STRATUM_FIELD_SOURCES`]; everywhere else it
//! answers `None`.

use crate::{
    descent::Stratum,
    field::{self, FieldProfile, ScreenEffect},
    position::Frame,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        descent::heart::{Well, wells_touching},
    },
};

use super::WELL_PULL_G;

/// The pull's weight past which rotation is free.
pub const FREE_TURN_WEIGHT: f32 = 0.25;

/// How far into `well`'s pull fractional tile `(column, row)` is, `0..=1` (0 at its reach and
/// outside, 1 at the point).
#[must_use]
pub fn well_weight(well: &Well, column: f32, row: f32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "tile coordinates of a well")]
    let (cx, cy, reach) = (
        well.centre.x as f32 + 0.5,
        well.centre.y as f32 + 0.5,
        well.reach() as f32,
    );
    let d = (column - cx).hypot(row - cy);
    if d >= reach {
        return 0.0;
    }
    1.0 - d / reach
}

/// The field round `well` at fractional tile `(column, row)`, over `base`.
#[must_use]
pub fn well_profile(well: &Well, base: FieldProfile, column: f32, row: f32) -> FieldProfile {
    let weight = well_weight(well, column, row);
    if weight <= 0.0 {
        return base;
    }
    #[allow(clippy::cast_precision_loss, reason = "tile coordinates of a well")]
    let (dx, dy) = (
        well.centre.x as f32 + 0.5 - column,
        well.centre.y as f32 + 0.5 - row,
    );
    let d = dx.hypot(dy).max(0.05);
    // Grows steeply toward the point (weight^1.5), the stratum's own pull fading out.
    let pull = WELL_PULL_G * weight * weight.sqrt();
    let keep = 1.0 - weight;
    let mut profile = base.with_gravity(
        (dx / d).mul_add(pull, base.gravity_x * keep),
        (dy / d).mul_add(pull, base.gravity_y * keep),
    );
    profile.free_rotation = base.free_rotation || weight >= FREE_TURN_WEIGHT;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a weight in 0..=1 as a percentage"
    )]
    let lens = (weight * 100.0).round().clamp(0.0, 100.0) as u8;
    let lens = lens.max(base.screen.levels[ScreenEffect::Lensing.index()]);
    profile.screen = profile.screen.with(ScreenEffect::Lensing, lens);
    profile
}

/// The well (of those given) pulling hardest at fractional tile `(column, row)`.
#[must_use]
pub fn strongest(wells: &[Well], column: f32, row: f32) -> Option<(Well, f32)> {
    wells
        .iter()
        .map(|well| (*well, well_weight(well, column, row)))
        .filter(|(_, weight)| *weight > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// The registered well field source: `None` outside every well's reach.
#[must_use]
pub fn well_field_source(
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> Option<FieldProfile> {
    if !settings.generator.has_descents() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame origins of a 13,000-row world are small"
    )]
    let (column, row) = (
        frame.origin_col() as f32 + x / TILE_SIZE,
        frame.origin_row() as f32 + y / TILE_SIZE,
    );
    #[allow(clippy::cast_possible_truncation, reason = "tile coordinates fit i32")]
    let (tx, ty) = (column.floor() as i32, row.floor() as i32);
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    if ty < top - 4 || ty > bottom + 4 {
        return None;
    }
    let here = TileRect {
        min: TilePosition { x: tx, y: ty },
        max: TilePosition { x: tx, y: ty },
    };
    let wells = wells_touching(settings, &layout, &here);
    let (well, _) = strongest(&wells, column, row)?;
    let base = crate::descent::fields::stratum_field_source(settings, frame, x, y)
        .unwrap_or(field::FieldProfile::NEUTRAL);
    Some(well_profile(&well, base, column, row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well() -> Well {
        Well {
            centre: TilePosition { x: 0, y: 0 },
            radius: 8,
        }
    }

    #[test]
    fn gravity_turns_toward_the_point_and_grows_inward() {
        let well = well();
        // East of the point: pulled west.
        let near = well_profile(&well, FieldProfile::NEUTRAL, 4.5, 0.5);
        assert!(near.gravity_x < -0.5, "{}", near.gravity_x);
        let far = well_profile(&well, FieldProfile::NEUTRAL, 14.5, 0.5);
        assert!(far.gravity_x < 0.0 && far.gravity_x > near.gravity_x);
        // Above the point: pulled down harder than standard near it.
        let above = well_profile(&well, FieldProfile::NEUTRAL, 0.5, -1.5);
        assert!(above.gravity_y > 1.2, "{}", above.gravity_y);
        assert!(near.free_rotation);
        assert!(near.screen.levels[ScreenEffect::Lensing.index()] > 50);
        // Out of reach: untouched.
        let out = well_profile(&well, FieldProfile::NEUTRAL, 40.0, 0.5);
        assert!((out.gravity_y - 1.0).abs() < 1e-6 && out.gravity_x.abs() < 1e-6);
    }
}
