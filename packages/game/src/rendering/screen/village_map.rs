//! Villages on the minimap and the mine map.
//!
//! Visited villages show in gold with their name on the map, villages a cartographer charted
//! in sky blue, and surface villages the player has not reached yet as a dim outline (you can
//! see rooftops on the horizon). Underground settlements stay hidden until visited or charted.

use std::sync::Arc;

use fatherload_simulation::{
    Player,
    terrain::Terrain,
    village::{self, Village, VillageSetting},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::super::ui_kit::{self, TextStyle};

/// How a player knows a village.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Known {
    Visited,
    Charted,
    /// A surface village on the horizon, not reached yet.
    Sighted,
}

/// The villages around columns `first..=last` the player can see on a map.
fn known_villages(
    terrain: &Terrain,
    player: &Player,
    first: i32,
    last: i32,
) -> Vec<(Arc<Village>, Known)> {
    let settings = terrain.settings();
    let sky_towns = fatherload_simulation::worldgen::sky::towns::sky_towns_between(
        settings,
        first - 60,
        last + 60,
    )
    .into_iter()
    .filter_map(|site| village::village(settings, site.id));
    village::villages_between(settings, first - 60, last + 60)
        .into_iter()
        .chain(sky_towns)
        .filter_map(|found| {
            let known = if player.villages.visited.contains(&found.id) {
                Known::Visited
            } else if player.villages.charted.contains(&found.id) {
                Known::Charted
            } else if found.setting == VillageSetting::Surface || found.setting.sky_town().is_some()
            {
                // Rooftops on the horizon, or a town hanging in the sky overhead.
                Known::Sighted
            } else {
                return None;
            };
            Some((found, known))
        })
        .collect()
}

const fn colors(known: Known) -> (Color, Color) {
    match known {
        Known::Visited => (GOLD, AMBER),
        Known::Charted => (SKY_LIGHT, OCEAN),
        Known::Sighted => (FOG, SLATE),
    }
}

/// A tiny house: roof and body, `size` pixels wide, centred on `(x, y)` (its base).
fn house_icon(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32, size: i32, known: Known) {
    let (body, roof) = colors(known);
    let half = size / 2;
    draw.draw_rectangle(x - half - 1, y - size - 1, size + 2, size + 2, INK);
    draw.draw_rectangle(x - half, y - half, size, half, body);
    for row in 0..half {
        draw.draw_rectangle(x - row, y - size + row, row * 2 + 1, 1, roof);
    }
    if known == Known::Sighted {
        draw.draw_rectangle(x - half + 1, y - half + 1, size - 2, half - 2, NIGHT);
    }
}

/// Village markers on the minimap; `to_map` maps a tile to a minimap pixel (`None` outside).
pub(super) fn draw_minimap_villages(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    player: &Player,
    columns: (i32, i32),
    to_map: impl Fn(i32, i32) -> Option<(i32, i32)>,
) {
    for (found, known) in known_villages(terrain, player, columns.0, columns.1) {
        let Some((x, y)) = to_map(found.center_column(), found.ground_row - 1) else {
            continue;
        };
        house_icon(draw, x, y, 5, known);
    }
}

/// Village markers with names on the mine map, clipped to `clip`. `(x0, y0)` is the map's
/// top-left canvas pixel showing tile `origin`; each tile is `pixel` canvas pixels.
pub(super) fn draw_map_villages(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    player: &Player,
    clip: Rectangle,
    origin: (i32, i32),
    pixel: i32,
) {
    let (x0, y0) = (clip.x as i32, clip.y as i32);
    let columns = clip.width as i32 / pixel.max(1);
    ui_kit::begin_clip(clip);
    for (found, known) in known_villages(terrain, player, origin.0, origin.0 + columns) {
        let span_x = x0 + (found.first_column - origin.0) * pixel;
        let span_w = (found.last_column - found.first_column + 1) * pixel;
        let ground_y = y0 + (found.ground_row - origin.1) * pixel;
        let (body, _) = colors(known);
        draw.draw_rectangle(span_x, ground_y, span_w, 1, with_alpha(body, 140));
        let x = x0 + (found.center_column() - origin.0) * pixel;
        house_icon(draw, x, ground_y - 1, 9, known);
        let label = match known {
            Known::Visited | Known::Charted => found.name.clone(),
            Known::Sighted => "?".to_owned(),
        };
        let width = ui_kit::text_width(&label, TextStyle::Body);
        // Above the marker, or below the ground line when the map's top would clip it.
        let label_y = if ground_y - 23 < y0 {
            ground_y + 4
        } else {
            ground_y - 23
        };
        draw.draw_rectangle(
            x - width / 2 - 2,
            label_y,
            width + 4,
            11,
            with_alpha(INK, 200),
        );
        ui_kit::text_centred(draw, &label, x, label_y + 1, TextStyle::Body, body);
    }
    ui_kit::end_clip();
}
