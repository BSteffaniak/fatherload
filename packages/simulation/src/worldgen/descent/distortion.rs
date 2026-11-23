//! The distortion strata's generated places (Wave 9, Descent VI part A of V11).
//!
//! [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11): the Mirror Strata's
//! mirror halls and the Looking-Glass Hall, the Gravity Shear's shear halls and the Shear Spire,
//! Pixel Rot's quantized rock and the Dead Screen.
//!
//! One pass, a pure function of the settings and the tile (chunks carve the same in any
//! order), run by `streamed::assemble` after the Living Dark's and before the Old Deep's, only
//! over chunks near the distortion strata's rows. The shapes live in [`crate::distortion`]
//! (`mirror`, `shear`, `rot`); this module gathers what touches a chunk ([`plan_for`]) and asks
//! it per tile ([`distortion_tile`]). The Descent landmarks (the outpost hall, anomaly rooms)
//! stay as stamped.

use crate::{
    descent::Stratum,
    distortion::{self, mirror, rot, shear},
    terrain::{TileKind, TilePosition},
    worldgen::{TileRect, WorldSettings},
};

use super::{DescentLayout, landmarks, stratum_in};

/// Everything of the distortion strata touching one chunk.
#[derive(Clone, Debug, Default)]
pub struct DistortionPlan {
    pub mirrors: Vec<mirror::MirrorZone>,
    pub shears: Vec<shear::ShearRegion>,
    pub spire: Option<TileRect>,
    pub screen: Option<TileRect>,
    /// Descent landmarks (outposts, anomaly rooms) the pass leaves alone.
    pub keep: Vec<TileRect>,
}

/// Whether `area` is near the distortion strata's rows (the pass runs there only).
#[must_use]
pub const fn touches_rows(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    if !distortion::has_distortion(settings) {
        return false;
    }
    let (first, last) = distortion::distortion_rows(layout);
    area.max.y >= first - 24 && area.min.y <= last + 24
}

/// What of the distortion strata touches `area` (a chunk).
#[must_use]
pub fn plan_for(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> DistortionPlan {
    if !touches_rows(settings, layout, area) {
        return DistortionPlan::default();
    }
    let grown = area.expanded(1);
    DistortionPlan {
        mirrors: mirror::zones_touching(settings, layout, &grown),
        shears: shear::regions_touching(settings, layout, &grown),
        spire: shear::spire(settings, layout).filter(|spire| spire.intersects(area)),
        screen: rot::dead_screen(settings, layout).filter(|screen| screen.intersects(area)),
        keep: landmarks::landmarks_with(settings, layout, area)
            .iter()
            .map(|landmark| landmark.bounds.expanded(1))
            .collect(),
    }
}

/// The distortion strata's tile at `(x, y)` over `base`, or `None` to leave it.
#[must_use]
pub fn distortion_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    plan: &DistortionPlan,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if plan.keep.iter().any(|keep| keep.contains(at)) {
        return None;
    }
    if let Some(spire) = &plan.spire
        && let Some(kind) = shear::spire_tile(spire, x, y, base)
    {
        return Some(kind);
    }
    if let Some(screen) = &plan.screen
        && let Some(kind) = rot::dead_screen_tile(screen, x, y, base)
    {
        return Some(kind);
    }
    // Halls first (their frames reach a tile outside them), inside ones winning.
    for zone in &plan.mirrors {
        if zone.bounds.contains(at)
            && let Some(kind) = mirror::zone_tile(settings, zone, x, y, base)
        {
            return Some(kind);
        }
    }
    for region in &plan.shears {
        if region.bounds.contains(at)
            && let Some(kind) = shear::region_tile(region, x, y, base)
        {
            return Some(kind);
        }
    }
    for zone in &plan.mirrors {
        if let Some(kind) = mirror::zone_tile(settings, zone, x, y, base) {
            return Some(kind);
        }
    }
    for region in &plan.shears {
        if let Some(kind) = shear::region_tile(region, x, y, base) {
            return Some(kind);
        }
    }
    if stratum_in(settings, layout, x, y) == Stratum::PixelRot {
        return rot::rot_tile(settings, layout, x, y, base);
    }
    None
}

#[cfg(test)]
mod tests;
