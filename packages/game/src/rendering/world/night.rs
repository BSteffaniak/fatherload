//! Night touches in world space (WP-NIGHT) drawn over the light map: shallow nocturnal ore
//! glowing in its own colour through the turf (silver icy, emerald green, diamond sky-blue,
//! platinum pale), with a slow pulse and a twinkle, once the view's player has seen it.
//! Presentation only: which ore glows is `fatherload_simulation::night::glows_at_night`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "presentation maps tiles and pixels between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    clock::WorldClock,
    night::{NOCTURNAL_GLOW_DEPTH, glows_at_night},
    terrain::{MineralKind, TILE_SIZE, Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, fx::disc, snap};

/// The glow of a nocturnal ore.
const fn glow_colour(mineral: MineralKind) -> Color {
    match mineral {
        MineralKind::Emerald => LIME,
        MineralKind::Diamond => SKY_LIGHT,
        MineralKind::Platinum => MIST,
        _ => ICE,
    }
}

/// Draws the glow of every discovered nocturnal ore tile in `visible` at night.
pub(super) fn draw_ore_glow(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    clock: WorldClock,
    seconds: f32,
) {
    if !clock.is_night() {
        return;
    }
    let first_x = (visible.x / TILE_SIZE).floor() as i32 - 1;
    let last_x = ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1;
    let first_y = (visible.y / TILE_SIZE).floor() as i32 - 1;
    let last_y = ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1;
    for x in first_x..=last_x {
        let ground = terrain.surface_row(x);
        let top = first_y.max(ground);
        let bottom = last_y.min(ground + NOCTURNAL_GLOW_DEPTH);
        for y in top..=bottom {
            let position = TilePosition { x, y };
            let Some(tile) = terrain.tile(position) else {
                continue;
            };
            let TileKind::Ore(mineral) = tile.kind else {
                continue;
            };
            if !glows_at_night(mineral) || explored.is_some_and(|tiles| !tiles.contains(position)) {
                continue;
            }
            let colour = glow_colour(mineral);
            let (cx, cy) = ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE);
            let pulse = (seconds * 1.4 + (x * 7 + y * 13) as f32 * 0.21).sin();
            disc(
                draw,
                cx,
                cy,
                7 + (pulse * 1.5) as i32,
                with_alpha(colour, 26),
            );
            disc(draw, cx, cy, 4, with_alpha(colour, 46));
            // A twinkle that hops between a few spots on the tile.
            let spot = ((seconds * 2.2) as i32 + x * 3 + y * 5).rem_euclid(4);
            let (sx, sy): (f32, f32) =
                [(-3.0, -2.0), (2.0, -3.0), (3.0, 2.0), (-2.0, 3.0)][spot as usize];
            let twinkle = with_alpha(CREAM, (150.0 + pulse * 80.0) as u8);
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(sx.mul_add(ART, cx)),
                    snap(sy.mul_add(ART, cy)),
                    ART,
                    ART,
                ),
                twinkle,
            );
        }
    }
}
