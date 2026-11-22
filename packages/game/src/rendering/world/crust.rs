//! The authored Crust in world space (Wave 8, `fatherload_simulation::crust`): the glow of the
//! Sinkhole's core and of lava-tube glass. Presentation only.

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    terrain::{TILE_SIZE, Terrain, TileKind},
};

use crate::render_snapshot::PlayerPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::sprites::world::MINE_CART,
    terrain::tile_art::{crust_glow, hash},
};

use super::{
    ART, draw_world_sprite,
    fx::{disc, visible_tiles},
    snap,
};

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

/// The mine cart under a riding rig: the rig sits in it, the wheels on the rail, sparks when it
/// rolls fast.
pub(super) fn draw_cart(draw: &mut impl RaylibDraw, player: &PlayerPresentation, ticks: u64) {
    let width = 20.0 * ART;
    let x = player.x - width / 2.0;
    // The rig rests on the rail's top: the cart's wheels sit on it, its tub around the rig.
    let y = player.y + 10.5 - 11.0 * ART + 0.5;
    let rolling = player.velocity_x.abs() > 20.0;
    #[allow(clippy::cast_possible_truncation, reason = "a small frame index")]
    let frame = if rolling {
        ((ticks / 4) % 2) as usize
    } else {
        0
    };
    draw_world_sprite(draw, &MINE_CART, frame, x, y, player.velocity_x < 0.0);
    if player.velocity_x.abs() > 180.0 && (ticks / 3).is_multiple_of(2) {
        let behind = if player.velocity_x > 0.0 { -1.0 } else { 1.0 };
        for (dx, dy, color) in [(3.0, 0.0, GOLD), (5.0, -1.0, AMBER), (6.0, 1.0, CREAM)] {
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(player.x + behind * (width / 2.0 + dx * ART)),
                    snap(y + 10.0 * ART + dy * ART),
                    ART,
                    ART,
                ),
                color,
            );
        }
    }
}

/// The Sinkhole's collapse front while it climbs: a boiling band of dust across the zone at
/// the front, with chunks tumbling in it.
pub(super) fn draw_sinkhole_front(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    crust: &fatherload_simulation::crust::CrustState,
    visible: Rectangle,
    seconds: f32,
) {
    let Some(run) = &crust.sinkhole else {
        return;
    };
    let Some(site) = fatherload_simulation::crust::sinkhole_site(terrain.settings()) else {
        return;
    };
    let frame = crate::view_frame::frame();
    let front_y = frame.rel_row(run.front_row) * TILE_SIZE;
    if front_y < visible.y - 4.0 * TILE_SIZE || front_y > visible.y + visible.height + TILE_SIZE {
        return;
    }
    let left = frame.rel_col(site.zone.min.x) * TILE_SIZE;
    let right = frame.rel_col(site.zone.max.x + 1) * TILE_SIZE;
    let first = left.max(visible.x - TILE_SIZE);
    let end = right.min(visible.x + visible.width + TILE_SIZE);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "pixel columns of the visible span"
    )]
    let steps = ((end - first) / (ART * 2.0)).max(0.0) as u32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "pixel columns"
    )]
    for index in 0..steps {
        let x = (index as f32).mul_add(ART * 2.0, first);
        let column = (x / ART) as i32;
        let h = hash(column, 3, 41);
        let boil = (seconds * 3.0 + (h % 100) as f32 / 17.0)
            .sin()
            .mul_add(0.5, 0.5);
        let height = (2.0 + boil * 3.0) * TILE_SIZE / 4.0;
        for band in 0..3_u8 {
            let alpha = [70_u8, 45, 25][usize::from(band)];
            let color = [STONE_LIGHT, STONE, STONE_DARK][usize::from(band)];
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(x),
                    snap(front_y - height * f32::from(band + 1) / 2.0),
                    ART * 2.0,
                    snap(height / 2.0),
                ),
                with_alpha(color, alpha),
            );
        }
    }
}
