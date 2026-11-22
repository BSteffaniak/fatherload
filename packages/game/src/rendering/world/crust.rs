//! The authored Crust in world space (Wave 8, `fatherload_simulation::crust`): the glow of the
//! Sinkhole's core and of lava-tube glass. Presentation only.

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    terrain::{TILE_SIZE, Terrain, TileKind},
};

use crate::rendering::{
    pixel_art::palette::with_alpha,
    terrain::tile_art::{crust_glow, hash},
};

use super::fx::{disc, visible_tiles};

/// Glowing Crust tiles throw soft pulsing halos (the Sinkhole's core hums brighter). Only
/// explored tiles glow.
pub(super) fn draw_crust_glows(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    seconds: f32,
) {
    if !fatherload_simulation::crust::crust_rules(terrain.settings()) {
        return;
    }
    for position in visible_tiles(visible) {
        let Some(tile) = terrain.tile(position) else {
            continue;
        };
        let TileKind::Crust(crust) = tile.kind else {
            continue;
        };
        let Some(color) = crust_glow(crust) else {
            continue;
        };
        if !explored.is_some_and(|tiles| tiles.contains(position)) {
            continue;
        }
        let frame = crate::view_frame::frame();
        let (cx, cy) = (
            (frame.rel_col(position.x) + 0.5) * TILE_SIZE,
            (frame.rel_row(position.y) + 0.5) * TILE_SIZE,
        );
        #[allow(clippy::cast_precision_loss, reason = "an animation phase")]
        let phase = (hash(position.x, position.y, 13) % 628) as f32 / 100.0;
        let core = crust == fatherload_simulation::crust::CrustTile::SinkholeCore;
        // The core's hum is slow and deep; tube glass only smoulders.
        let pulse = (seconds * if core { 1.1 } else { 0.7 } + phase)
            .sin()
            .mul_add(0.5, 0.5);
        let (radius, alpha) = if core {
            (20, 24.0 + pulse * 22.0)
        } else {
            (7, 4.0 + pulse * 6.0)
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "an alpha in 0..255"
        )]
        {
            disc(draw, cx, cy, radius, with_alpha(color, (alpha * 0.6) as u8));
            disc(draw, cx, cy, radius * 2 / 3, with_alpha(color, alpha as u8));
        }
    }
}
