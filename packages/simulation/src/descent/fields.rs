//! The stratum registry's field-effect profiles as the field-effects track's fields (Wave 7
//! integration: F8's [`super::FieldProfile`] feeds F9's [`crate::field`]).
//!
//! [`stratum_field_source`] is registered in [`crate::field::STRATUM_FIELD_SOURCES`], so every
//! reader of [`crate::WorldState::fields_at`] (rigs, pilots, creatures, drops, charges and each
//! seat's screen-effect stack) feels the stratum it is in. Profiles blend across the layout's
//! stratum boundaries ([`BLEND_HALF_ROWS`] each side) and, in strata with
//! [`super::FieldRegions`], across region cell edges. Every older generator, the sky and the
//! surface answer `None` (neutral), and a stratum whose profile is neutral answers `None` too,
//! so the classic paths run wherever nothing is asked for.

use std::cell::Cell;

use crate::{
    field::{self, ScreenEffects},
    position::Frame,
    terrain::TILE_SIZE,
    worldgen::{
        WorldSettings,
        descent::{self as layout_rules, DescentLayout},
        noise::hash2,
    },
};

use super::{
    FIELD_REGION_FEATHER_TILES, FIELD_REGION_TILES, FieldProfile, FieldRegions, ScreenEffect,
    Stratum,
};

/// Rows over which two neighbouring strata's fields blend, each side of their layout boundary.
pub const BLEND_HALF_ROWS: f32 = 8.0;

const SALT_FIELD_REGION: u64 = 0xF1E1_D7E9;

thread_local! {
    /// The last world's layout: [`layout_rules::layout`] is a pure function of the settings
    /// but too slow to rebuild for every field sample (one per body per tick).
    static LAYOUT: Cell<Option<(WorldSettings, DescentLayout)>> = const { Cell::new(None) };
}

/// [`layout_rules::layout`] of `settings`, cached per thread (a pure function, so the cache
/// never changes an answer).
#[must_use]
pub fn cached_layout(settings: &WorldSettings) -> DescentLayout {
    LAYOUT.with(|cache| {
        if let Some((cached, layout)) = cache.get()
            && cached == *settings
        {
            return layout;
        }
        let layout = layout_rules::layout(settings);
        cache.set(Some((*settings, layout)));
        layout
    })
}

/// The field-effects track's screen effect for a registry effect (every registry effect has a
/// pass since the Wave 8 blur; `None` stays for effects appended before their pass).
#[must_use]
pub const fn screen_effect(effect: ScreenEffect) -> Option<field::ScreenEffect> {
    Some(match effect {
        ScreenEffect::Pixelate => field::ScreenEffect::Pixelate,
        ScreenEffect::PaletteReduce => field::ScreenEffect::Palette,
        ScreenEffect::Exposure => field::ScreenEffect::Exposure,
        ScreenEffect::Bloom => field::ScreenEffect::Bloom,
        ScreenEffect::Desaturate => field::ScreenEffect::Desaturate,
        ScreenEffect::ColourGrade => field::ScreenEffect::ColourGrade,
        ScreenEffect::ChromaticAberration => field::ScreenEffect::Chromatic,
        ScreenEffect::Mirror => field::ScreenEffect::Mirror,
        ScreenEffect::Wobble => field::ScreenEffect::Wobble,
        ScreenEffect::Lensing => field::ScreenEffect::Lensing,
        ScreenEffect::Scanlines => field::ScreenEffect::Scanlines,
        ScreenEffect::Vignette => field::ScreenEffect::Vignette,
        ScreenEffect::Fog => field::ScreenEffect::Fog,
        ScreenEffect::Blur => field::ScreenEffect::Blur,
        ScreenEffect::Glitch => field::ScreenEffect::Glitch,
    })
}

fn per_mille(value: u16) -> f32 {
    f32::from(value) / 1_000.0
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "per mille to a percent, clamped to 0..=100"
)]
fn percent(intensity_permille: f32) -> u8 {
    (intensity_permille / 10.0).round().clamp(0.0, 100.0) as u8
}

/// `stratum`'s registry profile as a field profile.
///
/// Its screen effects are scaled by `strength` (`0..=1`; [`FieldProfile::deepening`] strata
/// pass how deep the point is). Light past the
/// standard (the Hollow World's inner sun) cannot brighten the light level, so it reads as
/// exposure on screen; the grade leans to the stratum's accent colour and the fog to its back
/// wall.
#[must_use]
pub fn field_profile(stratum: Stratum, strength: f32) -> field::FieldProfile {
    let def = stratum.def();
    let profile: &FieldProfile = &def.field;
    #[allow(
        clippy::cast_precision_loss,
        reason = "thousandths of gravity are small integers"
    )]
    let gravity = (
        profile.gravity_milli.0 as f32 / 1_000.0,
        profile.gravity_milli.1 as f32 / 1_000.0,
    );
    let accent = def.palette.accent.unwrap_or(def.palette.rock);
    let back = def.palette.back;
    let mut screen = ScreenEffects::NONE
        .graded([accent.0, accent.1, accent.2])
        .fogged([back.0, back.1, back.2]);
    for level in profile.screen {
        if let Some(effect) = screen_effect(level.effect) {
            let wanted = f32::from(level.intensity_permille) * strength;
            screen = screen.with(effect, percent(wanted).max(screen.level(effect)));
        }
    }
    if profile.light_permille > 1_000 {
        let glare = f32::from(profile.light_permille - 1_000) / 2.0 * strength;
        let exposure = field::ScreenEffect::Exposure;
        screen = screen.with(exposure, percent(glare).max(screen.level(exposure)));
    }
    field::FieldProfile {
        gravity_x: gravity.0,
        gravity_y: gravity.1,
        time_scale: per_mille(profile.time_scale_permille),
        friction: per_mille(profile.friction_permille),
        drag: per_mille(profile.drag_permille),
        light: per_mille(profile.light_permille.min(1_000)),
        sound_occlusion: 0.0,
        mirror_controls: profile.mirror_controls,
        free_rotation: profile.free_rotation,
        screen,
    }
}

/// The smoothstep of `t` clamped to `0..=1`.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

/// Gravity turned `degrees` clockwise on screen (+y down) from straight down.
const fn turned_gravity(degrees: u16) -> (f32, f32) {
    match degrees % 360 {
        90 => (-1.0, 0.0),
        180 => (0.0, -1.0),
        270 => (1.0, 0.0),
        _ => (0.0, 1.0),
    }
}

/// `profile` with `stratum`'s region departure at fractional tile `(column, row)` blended in
/// (nothing outside the stratum's active cells).
fn with_regions(
    settings: &WorldSettings,
    stratum: Stratum,
    profile: field::FieldProfile,
    column: f32,
    row: f32,
) -> field::FieldProfile {
    let regions = stratum.def().field.regions;
    let share = match regions {
        FieldRegions::None => return profile,
        FieldRegions::Authored(authored) => {
            return crate::distortion::fields::authored(settings, authored, profile, column, row);
        }
        FieldRegions::GravityTurns { per_mille, .. } | FieldRegions::Mirrored { per_mille } => {
            per_mille
        }
    };
    let (width, height) = FIELD_REGION_TILES;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "region cell sizes are small; tile coordinates of a world fit i32"
    )]
    let (cell_x, cell_y) = (
        (column / width as f32).floor() as i32,
        (row / height as f32).floor() as i32,
    );
    let roll = hash2(settings.seed, SALT_FIELD_REGION, cell_x, cell_y);
    if roll % 1_000 >= u64::from(share) {
        return profile;
    }
    #[allow(clippy::cast_precision_loss, reason = "region cells are small")]
    let inside = {
        let left = column - (cell_x * width) as f32;
        let right = ((cell_x + 1) * width) as f32 - column;
        let top = row - (cell_y * height) as f32;
        let bottom = ((cell_y + 1) * height) as f32 - row;
        left.min(right).min(top).min(bottom)
    };
    let weight = smoothstep(inside / FIELD_REGION_FEATHER_TILES);
    let departed = match regions {
        FieldRegions::None | FieldRegions::Authored(_) => profile,
        FieldRegions::GravityTurns { turns, .. } => {
            let pick = usize::try_from((roll >> 16) % turns.len().max(1) as u64).unwrap_or(0);
            let degrees = turns.get(pick).copied().unwrap_or(0);
            let (x, y) = turned_gravity(degrees);
            profile.with_gravity(x * profile.gravity_y, y * profile.gravity_y)
        }
        FieldRegions::Mirrored { .. } => {
            let mut mirrored = profile;
            mirrored.mirror_controls = true;
            mirrored.screen = mirrored.screen.with(field::ScreenEffect::Mirror, 100);
            mirrored
        }
    };
    profile.lerp(&departed, weight)
}

/// `stratum`'s full field at fractional tile `(column, row)`: its profile (deepening by how
/// far through the stratum the row is) with its regions.
fn stratum_field(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    column: f32,
    row: f32,
) -> field::FieldProfile {
    let def = stratum.def();
    let strength = if def.field.deepening {
        let (top, bottom) = layout.rows(stratum);
        #[allow(clippy::cast_precision_loss, reason = "rows are small")]
        let along = ((row - top as f32) / ((bottom - top).max(1) as f32)).clamp(0.0, 1.0);
        0.5f32.mul_add(along, 0.5)
    } else {
        1.0
    };
    with_regions(
        settings,
        stratum,
        field_profile(stratum, strength),
        column,
        row,
    )
}

/// Whether `stratum` has nothing to say to the fields.
const fn quiet(stratum: Stratum) -> bool {
    stratum.def().field.is_neutral()
}

/// The registered stratum field source ([`crate::field::STRATUM_FIELD_SOURCES`]).
///
/// The field of the stratum at local pixel `(x, y)` of `frame` in a Deep Descent world, blended over
/// [`BLEND_HALF_ROWS`] across the layout boundaries with its neighbours; `None` in every older
/// world, above the first stratum, and wherever the strata near the point are all neutral.
#[must_use]
pub fn stratum_field_source(
    settings: &WorldSettings,
    frame: Frame,
    x: f32,
    y: f32,
) -> Option<field::FieldProfile> {
    if !settings.generator.has_descents() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let layout = cached_layout(settings);
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame origins of a 13,000-row world are small"
    )]
    let (column, row) = (
        frame.origin_col() as f32 + x / TILE_SIZE,
        frame.origin_row() as f32 + y / TILE_SIZE,
    );
    #[allow(clippy::cast_possible_truncation, reason = "tile rows fit i32")]
    let whole = row.floor() as i32;
    if whole < layout.tops[0] || whole > layout.floor {
        return None;
    }
    let stratum = layout.stratum_of_row(whole);
    let index = stratum.index();
    let (top, bottom) = layout.rows(stratum);
    #[allow(clippy::cast_precision_loss, reason = "rows are small")]
    let (top, end) = (top as f32, (bottom + 1) as f32);
    // The neighbour across the nearer boundary, if the point is inside its blend.
    let previous = (index > 0 && row < top + BLEND_HALF_ROWS).then(|| Stratum::ALL[index - 1]);
    let next = (index + 1 < Stratum::ALL.len() && row > end - BLEND_HALF_ROWS)
        .then(|| Stratum::ALL[index + 1]);
    if quiet(stratum) && previous.is_none_or(quiet) && next.is_none_or(quiet) {
        return None;
    }
    let own = stratum_field(settings, &layout, stratum, column, row);
    let blended = match (previous, next) {
        (Some(above), _) => {
            let theirs = stratum_field(settings, &layout, above, column, row);
            theirs.lerp(&own, field::boundary_blend(row, top, BLEND_HALF_ROWS))
        }
        (None, Some(below)) => {
            let theirs = stratum_field(settings, &layout, below, column, row);
            own.lerp(&theirs, field::boundary_blend(row, end, BLEND_HALF_ROWS))
        }
        (None, None) => own,
    };
    Some(blended)
}

#[cfg(test)]
mod tests;
