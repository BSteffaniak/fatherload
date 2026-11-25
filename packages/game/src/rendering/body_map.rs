//! A round body's own map (Wave 12 systems): the M key on a body shows the body's strip as
//! the seat's player discovered it, not the home world's map. The strip wraps, so the map
//! scrolls round the lap without an edge; the seam (column 0, the body's top) is marked, and
//! every rig in the store is drawn where it stands.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map layout in whole pixels"
)]

use fatherload_simulation::terrain::TilePosition;
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::UiLayout,
    prompts::hint,
    screen::{UiFrame, map_tile_color},
    ui_kit::{self, TextStyle},
};
use crate::{input::prompts::Prompt, render_snapshot::PlaceScene};

/// Draws `scene`'s body map for the seat (see the module docs).
pub(super) fn draw_body_map(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    scene: &PlaceScene<'_>,
) {
    let terrain = scene.terrain;
    let Some(planet) = terrain.planet_ground() else {
        return;
    };
    let lap = i32::try_from(planet.grid().surface_columns())
        .unwrap_or(i32::MAX)
        .max(1);
    let player = ui.player.tile_in(scene.frame);
    let home = terrain.lap_tile(player);
    let theme = scene
        .body
        .as_ref()
        .map_or("round", |body| body.body.theme.name());
    let degrees = i64::from(home.x) * 360 / i64::from(lap);
    let summary = format!(
        "{} columns round; you are {degrees} degrees round from its top, {} rows down.",
        lap,
        player.y - terrain.surface_row(player.x)
    );
    let rect = UiLayout::screen(draw).canvas_modal(
        &format!("Body Map: a {theme} world"),
        "This body's own map: the ground you discovered here.",
        &[hint(Prompt::Map, ""), hint(Prompt::Back, "close")],
        &summary,
    );
    ui_kit::well(draw, rect);
    let (x0, y0) = (rect.x as i32 + 2, rect.y as i32 + 2);
    let (width, height) = (rect.width as i32 - 4, rect.height as i32 - 4);
    if width < 8 || height < 8 {
        return;
    }
    draw.draw_rectangle(x0, y0, width, height, NIGHT);
    let pixel = 2;
    let (columns, rows) = (width / pixel, height / pixel);
    // Centred on the rig across the lap (no edge: the strip wraps) and between the sky over
    // the hills and the core.
    let origin_x = player.x - columns / 2;
    let top = terrain.min_y();
    let bottom = terrain.height();
    let origin_y = (player.y - rows / 3).clamp(top, (bottom - rows).max(top));
    let explored = |tile: TilePosition| {
        scene
            .discovered
            .is_some_and(|tiles| tiles.contains(terrain.lap_tile(tile)))
    };
    for row in 0..rows.min(bottom - origin_y) {
        let y = origin_y + row;
        for column in 0..columns {
            let tile = TilePosition {
                x: origin_x + column,
                y,
            };
            if let Some(colour) = map_tile_color(terrain, tile, explored(tile)) {
                draw.draw_rectangle(x0 + column * pixel, y0 + row * pixel, pixel, pixel, colour);
            }
        }
    }
    // The seam: the body's top, column 0 of the lap.
    for column in 0..columns {
        if terrain
            .lap_tile(TilePosition {
                x: origin_x + column,
                y: 0,
            })
            .x
            == 0
        {
            let x = x0 + column * pixel;
            for dash in (0..height).step_by(6) {
                draw.draw_rectangle(x, y0 + dash, 1, 3, with_alpha(GOLD, 160));
            }
            ui_kit::text(draw, "TOP", x + 3, y0 + 2, TextStyle::Body, GOLD);
        }
    }
    // Every rig in the store: the seat's own blinks, the others are mint.
    for rig in &scene.rigs {
        let tile = scene.frame.tile_at(rig.x, rig.y);
        let offset = (tile.x - origin_x).rem_euclid(lap);
        if offset >= columns {
            continue;
        }
        let (x, y) = (
            x0 + offset * pixel,
            y0 + (tile.y - origin_y).clamp(0, rows - 1) * pixel,
        );
        let own = rig.player_id == ui.view.player_id;
        let blink = (ui_kit::seconds() * 2.0) as i32 % 2 == 0;
        draw.draw_rectangle(x - 3, y - 3, 8, 8, INK);
        let colour = if !own {
            MINT
        } else if blink {
            BONE
        } else {
            SKY_LIGHT
        };
        draw.draw_rectangle(x - 2, y - 2, 6, 6, colour);
    }
}
