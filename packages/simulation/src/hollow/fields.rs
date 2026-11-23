//! The Upside-Down Isles' inverted gravity as a field source.
//!
//! Under each hanging isle (its body and [`ISLE_BUBBLE_ROWS`] rows of air below it, a few
//! columns wider) gravity points **up**, so a rig flying in under an isle falls up onto its
//! underside. Rotation is free there; the player turns the rig with its thrusters to land feet
//! first (nothing turns it for them). The pull fades over [`FEATHER_TILES`] at the bubble's
//! sides and bottom, so crossing in or out is smooth. Registered before the stratum source in
//! [`crate::field::STRATUM_FIELD_SOURCES`]; everywhere else it answers `None`.

use crate::{
    descent::Stratum,
    field::{self, FieldProfile, ScreenEffect},
    position::Frame,
    terrain::TILE_SIZE,
    worldgen::{
        TileRect, WorldSettings,
        descent::hollow::{ISLE_BUBBLE_ROWS, Isle, isles_touching},
    },
};

/// Tiles over which the inverted pull fades in at a bubble's edges.
pub const FEATHER_TILES: f32 = 3.0;

/// How far inside `isle`'s bubble fractional tile `(column, row)` is, `0..=1` (0 outside).
#[must_use]
pub fn bubble_weight(isle: &Isle, column: f32, row: f32) -> f32 {
    let bubble = isle.bubble();
    #[allow(clippy::cast_precision_loss, reason = "tile coordinates of an isle")]
    let (left, right, bottom, top) = (
        bubble.min.x as f32,
        (bubble.max.x + 1) as f32,
        (bubble.max.y + 1) as f32,
        bubble.min.y as f32,
    );
    if column < left || column > right || row < top || row > bottom {
        return 0.0;
    }
    let inside = (column - left).min(right - column).min(bottom - row);
    let t = (inside / FEATHER_TILES).clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

/// The field under `isle` at fractional tile `(column, row)`, over `base`.
#[must_use]
pub fn isle_profile(isle: &Isle, base: FieldProfile, column: f32, row: f32) -> FieldProfile {
    let weight = bubble_weight(isle, column, row);
    if weight <= 0.0 {
        return base;
    }
    let strength = base.gravity_y.abs().max(0.6);
    let mut inverted = base.with_gravity(0.0, -strength);
    inverted.free_rotation = true;
    inverted.screen = inverted
        .screen
        .with(ScreenEffect::ColourGrade, 18)
        .graded([170, 214, 255]);
    base.lerp(&inverted, weight)
}

/// The registered isle field source: `None` outside every isle's bubble.
#[must_use]
pub fn isle_field_source(
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
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    if ty < top - 4 || ty > bottom + ISLE_BUBBLE_ROWS {
        return None;
    }
    let here = TileRect {
        min: crate::terrain::TilePosition { x: tx, y: ty },
        max: crate::terrain::TilePosition { x: tx, y: ty },
    };
    let isle = isles_touching(settings, &layout, &here)
        .into_iter()
        .find(|isle| bubble_weight(isle, column, row) > 0.0)?;
    let base = crate::descent::fields::stratum_field_source(settings, frame, x, y)
        .unwrap_or(field::FieldProfile::NEUTRAL);
    Some(isle_profile(&isle, base, column, row))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TilePosition;

    fn isle() -> Isle {
        Isle {
            bounds: TileRect {
                min: TilePosition { x: 0, y: 0 },
                max: TilePosition { x: 19, y: 8 },
            },
            stalk: None,
            seed: 0,
        }
    }

    #[test]
    fn gravity_points_up_under_an_isle_and_fades_at_its_edges() {
        let isle = isle();
        // Right under the middle of its underside: full upward pull, free rotation.
        let field = isle_profile(&isle, FieldProfile::NEUTRAL, 10.0, 12.0);
        assert!((field.gravity_y + 1.0).abs() < 1e-5, "{}", field.gravity_y);
        assert!(field.free_rotation);
        // At the bottom edge of the bubble: fading back to down.
        let edge = isle_profile(&isle, FieldProfile::NEUTRAL, 10.0, 9.0 + 17.5);
        assert!(edge.gravity_y > -1.0 && edge.gravity_y < 1.0);
        // Far below: untouched.
        let below = isle_profile(&isle, FieldProfile::NEUTRAL, 10.0, 40.0);
        assert!((below.gravity_y - 1.0).abs() < 1e-6);
    }
}
