//! The Void Geode Fields' low-gravity pockets as a field source.
//!
//! Inside a geode's hollow the pull is weak ([`GEODE_GRAVITY`]) and points out from the
//! geode's heart to its shell, so a rig drifts to the nearest wall; air drag drops, and the
//! rig may turn all the way round to land feet-first on any wall. The pull fades in over the
//! last tiles of the hollow, so crossing the shell is smooth. Registered first in
//! [`crate::field::STRATUM_FIELD_SOURCES`]; everywhere else it answers `None` and the
//! stratum source decides.

use crate::{
    field::{self, FieldProfile, ScreenEffect},
    position::Frame,
    terrain::TILE_SIZE,
    worldgen::WorldSettings,
};

use super::sites::{self, Geode};

/// The pull inside a geode's hollow, standard gravities.
pub const GEODE_GRAVITY: f32 = 0.32;
/// The air drag inside a geode's hollow (floaty).
pub const GEODE_DRAG: f32 = 0.35;

/// The field inside `geode` at fractional tile `(column, row)`, blended over the base
/// profile `base` by how deep in the hollow the point is.
#[must_use]
pub fn geode_profile(geode: &Geode, base: FieldProfile, column: f32, row: f32) -> FieldProfile {
    #[allow(clippy::cast_precision_loss, reason = "tile coordinates of a geode")]
    let (dx, dy) = (
        column - (geode.centre.x as f32 + 0.5),
        row - (geode.centre.y as f32 + 0.5),
    );
    let distance = dx.hypot(dy);
    #[allow(clippy::cast_precision_loss, reason = "small radii")]
    let hollow = geode.hollow() as f32;
    // Full strength a tile and a half inside the shell, nothing at the shell's inner face.
    let weight = ((hollow - distance) / 1.5).clamp(0.0, 1.0);
    if weight <= 0.0 {
        return base;
    }
    // Out toward the shell, fading to nothing at the very heart (no singularity).
    let reach = (distance / 3.0).min(1.0);
    let (nx, ny) = if distance > 0.01 {
        (dx / distance, dy / distance)
    } else {
        (0.0, 0.0)
    };
    let mut inside = base.with_gravity(nx * GEODE_GRAVITY * reach, ny * GEODE_GRAVITY * reach);
    inside.drag = GEODE_DRAG;
    inside.free_rotation = true;
    inside.screen = inside
        .screen
        .with(ScreenEffect::Wobble, 12)
        .with(ScreenEffect::ColourGrade, 22)
        .graded([214, 140, 255]);
    base.lerp(&inside, weight)
}

/// The registered geode field source: `None` outside every geode hollow.
#[must_use]
pub fn geode_field_source(
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
    let geode = sites::geode_at(settings, &layout, tx, ty)?;
    let d2 = (tx - geode.centre.x).pow(2) + (ty - geode.centre.y).pow(2);
    if d2 >= geode.hollow() * geode.hollow() {
        return None;
    }
    let base = crate::descent::fields::stratum_field_source(settings, frame, x, y)
        .unwrap_or(field::FieldProfile::NEUTRAL);
    Some(geode_profile(&geode, base, column, row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TilePosition;

    #[test]
    fn a_geode_pulls_out_to_its_shell_and_frees_rotation() {
        let geode = Geode {
            centre: TilePosition { x: 0, y: 0 },
            radius: 10,
        };
        // Five tiles right of the heart: pulled right, weakly.
        let field = geode_profile(&geode, FieldProfile::NEUTRAL, 5.5, 0.5);
        assert!(field.gravity_x > 0.2 && field.gravity_x <= GEODE_GRAVITY + 1e-4);
        assert!(field.gravity_y.abs() < 0.01);
        assert!(field.free_rotation);
        assert!(field.drag < 0.5);
        // Above the heart: pulled up.
        let field = geode_profile(&geode, FieldProfile::NEUTRAL, 0.5, -5.5);
        assert!(field.gravity_y < -0.2);
        // In the shell: the base profile.
        let field = geode_profile(&geode, FieldProfile::NEUTRAL, 9.0, 0.5);
        assert!((field.gravity_y - 1.0).abs() < 1e-6);
    }
}
