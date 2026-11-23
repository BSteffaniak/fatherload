//! The distortion strata's authored fields (Wave 9): the mirror halls, shear halls, the Shear
//! Spire and the rot tiers as field departures.
//!
//! The registry gives each stratum [`crate::descent::FieldRegions::Authored`]; the stratum
//! field source ([`crate::descent::fields::stratum_field_source`]) hands the stratum's own
//! profile to [`authored`], which blends the departure in by the hall's feathered weight. Pure
//! functions of the settings and the position, so hosts and predicting clients agree.

use crate::{
    descent::{AuthoredRegions, fields::cached_layout},
    field::{self, FieldProfile},
    terrain::TilePosition,
    worldgen::{TileRect, WorldSettings},
};

use super::{mirror, rot, shear};

/// The screen's mirror level inside a hall (percent; with its controls).
pub const MIRROR_SCREEN: u8 = 100;
/// The heat-haze shimmer of a turned shear field (percent of the wobble pass).
pub const SHEAR_WOBBLE: u8 = 14;

/// `profile` with the authored departure of `regions` at fractional tile `(column, row)`.
#[must_use]
pub fn authored(
    settings: &WorldSettings,
    regions: AuthoredRegions,
    profile: FieldProfile,
    column: f32,
    row: f32,
) -> FieldProfile {
    let layout = cached_layout(settings);
    #[allow(clippy::cast_possible_truncation, reason = "tile coordinates fit i32")]
    let tile = TilePosition {
        x: column.floor() as i32,
        y: row.floor() as i32,
    };
    let point = TileRect {
        min: tile,
        max: tile,
    };
    match regions {
        AuthoredRegions::MirrorHalls => {
            let Some(zone) = mirror::zones_touching(settings, &layout, &point)
                .into_iter()
                .next()
            else {
                return profile;
            };
            let weight = zone.weight(column, row);
            let mut mirrored = profile;
            mirrored.mirror_controls = true;
            mirrored.screen = mirrored
                .screen
                .with(field::ScreenEffect::Mirror, MIRROR_SCREEN);
            profile.lerp(&mirrored, weight)
        }
        AuthoredRegions::ShearHalls => {
            if let Some(spire) = shear::spire(settings, &layout)
                && spire.contains(tile)
            {
                let (x, y) = shear::spire_down(&spire, column, row);
                let weight = shear::spire_weight(&spire, column, row);
                let turned = profile
                    .with_gravity(x * profile.gravity_y, y * profile.gravity_y)
                    .with_screen(
                        profile
                            .screen
                            .with(field::ScreenEffect::Wobble, SHEAR_WOBBLE),
                    );
                return profile.lerp(&turned, weight);
            }
            let Some(region) = shear::regions_touching(settings, &layout, &point)
                .into_iter()
                .next()
            else {
                return profile;
            };
            let (x, y) = region.down();
            #[allow(clippy::cast_precision_loss, reason = "unit directions")]
            let turned = profile
                .with_gravity(x as f32 * profile.gravity_y, y as f32 * profile.gravity_y)
                .with_screen(
                    profile
                        .screen
                        .with(field::ScreenEffect::Wobble, SHEAR_WOBBLE),
                );
            profile.lerp(&turned, region.weight(column, row))
        }
        AuthoredRegions::RotTiers => {
            let (pixelate, palette, glitch) = rot::screen_levels(&layout, row);
            let percent = |value: f32| {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a percent in 0..=100"
                )]
                let value = value.round().clamp(0.0, 100.0) as u8;
                value
            };
            let mut rotted = profile;
            rotted.screen = rotted
                .screen
                .with(field::ScreenEffect::Pixelate, percent(pixelate))
                .with(field::ScreenEffect::Palette, percent(palette))
                .with(field::ScreenEffect::Glitch, percent(glitch));
            rotted
        }
    }
}

/// The mirror hall over fractional tile `(column, row)` and its weight there (presentation
/// and the twins).
#[must_use]
pub fn mirror_zone_weight(
    settings: &WorldSettings,
    column: f32,
    row: f32,
) -> Option<(mirror::MirrorZone, f32)> {
    if !super::has_distortion(settings) {
        return None;
    }
    let layout = cached_layout(settings);
    #[allow(clippy::cast_possible_truncation, reason = "tile coordinates fit i32")]
    let tile = TilePosition {
        x: column.floor() as i32,
        y: row.floor() as i32,
    };
    let zone = mirror::zones_touching(
        settings,
        &layout,
        &TileRect {
            min: tile,
            max: tile,
        },
    )
    .into_iter()
    .next()?;
    Some((zone, zone.weight(column, row)))
}
