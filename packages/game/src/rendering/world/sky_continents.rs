//! The sky continents of a V9 world (Wave 4 T21): what makes their tiles read as places.
//!
//! * **Back walls** behind the open tiles inside a continent's body (its caves, the temple's
//!   halls and anything dug out), so a cave reads as a cave, not a hole into the sky: dark
//!   continent rock, or carved temple stone inside a temple.
//! * **Sky cities**: the buildings of each continent's city
//!   ([`fatherload_simulation::worldgen::sky::continent::SkyCity`]) standing on its street:
//!   aeronaut towers with mooring masts, domed halls, a striped cloud market, a sky monastery
//!   with its bell, airship hangars and garden-roofed homes, strung with lanterns. Presentation
//!   only, like villages: rigs and people pass in front.
//! * **Lakes**: open water in the basins ([`continent::lake_water_at`]), with ripples.
//! * **Glows**: halos around aether crystal, storm glass, the aether core, keystones and relic
//!   chests.
//!
//! Everything is a pure function of the world settings, the terrain and the clock, and is
//! skipped on worlds without sky content and away from the continents.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation math on tile coordinates and bounded animation phases"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    sky::SkyTile,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        TileRect,
        sky::continent::{self, SkyBuilding, SkyBuildingKind, SkyCity},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::{hash, sky_glow};

use super::{ART, Pen, fx::disc};

/// The tiles a view's rectangle covers (plus a margin).
fn tile_rect(visible: Rectangle) -> TileRect {
    TileRect {
        min: TilePosition {
            x: (visible.x / TILE_SIZE).floor() as i32 - 1,
            y: (visible.y / TILE_SIZE).floor() as i32 - 1,
        },
        max: TilePosition {
            x: ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1,
            y: ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1,
        },
    }
}

/// Back walls behind open tiles inside the continents' bodies (drawn before the terrain).
pub(super) fn draw_continent_backs(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
) {
    let rect = tile_rect(visible);
    for (land, plan) in continent::continents_touching(terrain.settings(), &rect) {
        let first = rect.min.x.max(land.center_x - land.half_width);
        let last = rect.max.x.min(land.center_x + land.half_width);
        for x in first..=last {
            let (Some(top), Some(bottom)) = (land.surface_row(x), land.underside_row(x)) else {
                continue;
            };
            // A temple's shrine stands above the ground: its hollows get a wall too.
            let from = plan
                .temple
                .filter(|temple| {
                    let r = temple.rect();
                    x >= r.min.x && x <= r.max.x
                })
                .map_or(top + 1, |temple| temple.rect().min.y.min(top + 1));
            for y in from.max(rect.min.y)..=bottom.min(rect.max.y) {
                let position = TilePosition { x, y };
                if terrain
                    .tile(position)
                    .is_none_or(|tile| tile.kind != TileKind::Air)
                    || plan.water(&land, x, y)
                {
                    continue;
                }
                let temple = plan
                    .temple
                    .and_then(|temple| temple.cell(x, y))
                    .is_some_and(|symbol| symbol != '.');
                if y <= top && !temple {
                    continue;
                }
                let (left, top_px) = (x as f32 * TILE_SIZE, y as f32 * TILE_SIZE);
                if temple {
                    draw.draw_rectangle_rec(
                        Rectangle::new(left, top_px, TILE_SIZE, TILE_SIZE),
                        STONE_DARK,
                    );
                    // Carved courses and the odd teal glyph.
                    draw.draw_rectangle_rec(
                        Rectangle::new(left, top_px + TILE_SIZE - ART, TILE_SIZE, ART),
                        INK,
                    );
                    if hash(x, y, 901).is_multiple_of(9) {
                        let pen = Pen::at(left, top_px);
                        pen.rect(draw, 6, 5, 4, 1, TEAL);
                        pen.rect(draw, 7, 6, 2, 4, TEAL);
                        pen.rect(draw, 5, 10, 6, 1, TEAL);
                    }
                } else {
                    draw.draw_rectangle_rec(
                        Rectangle::new(left, top_px, TILE_SIZE, TILE_SIZE),
                        SLATE_DARK,
                    );
                    if hash(x, y, 903).is_multiple_of(5) {
                        let pen = Pen::at(left, top_px);
                        let h = hash(x, y, 904);
                        pen.rect(draw, (h % 12) as i32, ((h >> 4) % 12) as i32, 3, 2, NIGHT);
                    }
                }
            }
        }
    }
}

/// Lake water and glowing tiles on the continents (drawn after the terrain).
pub(super) fn draw_continent_fronts(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    seconds: f32,
) {
    let rect = tile_rect(visible);
    for (land, plan) in continent::continents_touching(terrain.settings(), &rect) {
        let bounds = land.bounds();
        let first = rect.min.x.max(bounds.min.x);
        let last = rect.max.x.min(bounds.max.x);
        for x in first..=last {
            for y in rect.min.y.max(bounds.min.y)..=rect.max.y.min(bounds.max.y) {
                let position = TilePosition { x, y };
                let Some(tile) = terrain.tile(position) else {
                    continue;
                };
                let (cx, cy) = ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE);
                if tile.kind == TileKind::Air && plan.water(&land, x, y) {
                    let surface = !plan.water(&land, x, y - 1);
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            x as f32 * TILE_SIZE,
                            y as f32 * TILE_SIZE,
                            TILE_SIZE,
                            TILE_SIZE,
                        ),
                        with_alpha(if surface { SKY } else { OCEAN }, 170),
                    );
                    if surface {
                        let top = y as f32 * TILE_SIZE;
                        draw.draw_rectangle_rec(
                            Rectangle::new(x as f32 * TILE_SIZE, top, TILE_SIZE, ART),
                            with_alpha(ICE, 200),
                        );
                        for ripple in 0..2 {
                            let h = hash(x, ripple, 905);
                            let travel =
                                (seconds * (3.0 + (h % 4) as f32) + (h % 16) as f32) % 16.0;
                            draw.draw_rectangle_rec(
                                Rectangle::new(
                                    x as f32 * TILE_SIZE + (travel * ART).floor(),
                                    top + 3.0 * ART,
                                    ART * 3.0,
                                    ART,
                                ),
                                with_alpha(BONE, 150),
                            );
                        }
                    }
                    continue;
                }
                if let TileKind::Sky(sky) = tile.kind
                    && let Some(color) = sky_glow(sky)
                {
                    let phase = (hash(x, y, 907) % 628) as f32 / 100.0;
                    let pulse = (seconds * 1.4 + phase).sin().mul_add(0.5, 0.5);
                    let radius = match sky {
                        SkyTile::AetherCore => 10,
                        SkyTile::TempleCache | SkyTile::TempleKeystone => 9,
                        _ => 12,
                    };
                    let alpha = 14.0 + pulse * 14.0;
                    disc(draw, cx, cy, radius, with_alpha(color, (alpha * 0.6) as u8));
                    disc(draw, cx, cy, radius * 2 / 3, with_alpha(color, alpha as u8));
                }
            }
        }
    }
}

/// The sky cities in view, standing on their streets (drawn before the terrain, so the
/// street's paving and anything in front overlap their feet).
pub(super) fn draw_sky_cities(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    seconds: f32,
) {
    let rect = tile_rect(visible);
    for (land, _) in continent::continents_touching(terrain.settings(), &rect) {
        let Some(city) = continent::city_of(&land) else {
            continue;
        };
        let street = city.street_row as f32 * TILE_SIZE;
        if street < visible.y - 2.0 || street - 26.0 * TILE_SIZE > visible.y + visible.height {
            continue;
        }
        draw_lantern_strings(draw, &city, seconds);
        for building in &city.buildings {
            let left = building.first_column as f32 * TILE_SIZE;
            let right = left + building.width as f32 * TILE_SIZE;
            if right < visible.x - TILE_SIZE || left > visible.x + visible.width + TILE_SIZE {
                continue;
            }
            draw_building(draw, building, street, seconds);
        }
        draw_plaza(draw, &city, street, seconds);
    }
}

/// Lanterns strung between the rooftops along the street.
fn draw_lantern_strings(draw: &mut impl RaylibDraw, city: &SkyCity, seconds: f32) {
    let street = city.street_row as f32 * TILE_SIZE;
    for pair in city.buildings.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let from = (
            (a.first_column + a.width) as f32 * TILE_SIZE,
            street - (a.height.min(b.height) as f32 * 0.6).max(3.0) * TILE_SIZE,
        );
        let to = (b.first_column as f32 * TILE_SIZE, from.1);
        let span = to.0 - from.0;
        let steps = (span / (ART * 2.0)).max(1.0) as i32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let sag = (t * std::f32::consts::PI).sin() * TILE_SIZE * 0.6;
            let (x, y) = (span.mul_add(t, from.0), from.1 + sag);
            draw.draw_rectangle_rec(Rectangle::new(x.floor(), y.floor(), ART, ART), SOIL_DARK);
            if step % 5 == 2 {
                let flicker = (seconds * 3.0 + step as f32).sin().mul_add(0.15, 0.85);
                let color = if step % 10 == 2 { AMBER } else { ROSE };
                disc(
                    draw,
                    x,
                    y + ART * 2.0,
                    2,
                    with_alpha(color, (230.0 * flicker) as u8),
                );
            }
        }
    }
}

/// The open plaza: a stone fountain with a little aether flame.
fn draw_plaza(draw: &mut impl RaylibDraw, city: &SkyCity, street: f32, seconds: f32) {
    let middle = (city.plaza.0 + city.plaza.1) as f32 * 0.5 * TILE_SIZE;
    let pen = Pen::at(middle - 12.0 * ART, street - 14.0 * ART);
    pen.boxed(draw, 2, 8, 20, 6, MIST);
    pen.rect(draw, 3, 9, 18, 1, BONE);
    pen.boxed(draw, 9, 2, 6, 7, FOG);
    let flame = ((seconds * 8.0) as i32) % 3;
    pen.rect(draw, 11, -1 - flame, 2, 3 + flame, LAVENDER);
    pen.rect(draw, 11, 0, 2, 2, PINK);
    disc(
        draw,
        middle,
        street - 13.0 * ART,
        6,
        with_alpha(LAVENDER, 40),
    );
}

/// One building of a sky city, standing on `street` (world y of the street's top).
#[allow(
    clippy::too_many_lines,
    clippy::suspicious_operation_groupings,
    reason = "one arm per building kind; the dome is a circle's chord"
)]
fn draw_building(draw: &mut impl RaylibDraw, building: &SkyBuilding, street: f32, seconds: f32) {
    let w = building.width * 16;
    let h = building.height * 16;
    let pen = Pen::at(
        building.first_column as f32 * TILE_SIZE,
        street - building.height as f32 * TILE_SIZE,
    );
    let seed = hash(building.first_column, building.height, 911);
    match building.kind {
        SkyBuildingKind::Tower => {
            // A slender white tower, gold bands, a teal cone and a mooring mast with a pennant.
            pen.boxed(draw, 4, 10, w - 8, h - 10, BONE);
            pen.rect(draw, w - 7, 11, 2, h - 12, MIST);
            for band in (24..h).step_by(28) {
                pen.rect(draw, 5, band, w - 10, 2, GOLD);
            }
            windows(draw, pen, (8, 18), (w - 16, h - 30), 10, seed, seconds);
            let mid = w / 2;
            for row in 0..10 {
                pen.rect(
                    draw,
                    mid - row - 1,
                    row,
                    row * 2 + 2,
                    1,
                    if row == 0 { INK } else { TEAL },
                );
            }
            pen.rect(draw, mid, -14, 1, 14, STEEL_DARK);
            let wave = ((seconds * 4.0) as i32 + (seed % 3) as i32) % 3;
            pen.rect(draw, mid + 1, -14, 6 - wave, 3, RED);
        }
        SkyBuildingKind::Hall => {
            // White walls under a great sky-blue dome with a gold finial.
            let body = h * 3 / 5;
            pen.boxed(draw, 0, h - body, w, body, MIST);
            for pillar in (4..w - 4).step_by(10) {
                pen.rect(draw, pillar, h - body + 4, 3, body - 4, BONE);
            }
            let radius = w / 2 - 2;
            for row in 0..radius {
                let half =
                    ((radius * radius - (radius - row) * (radius - row)) as f32).sqrt() as i32;
                pen.rect(
                    draw,
                    w / 2 - half,
                    h - body - radius + row,
                    half * 2,
                    1,
                    if row < 2 {
                        INK
                    } else if row < radius / 3 {
                        SKY_LIGHT
                    } else {
                        SKY
                    },
                );
            }
            pen.rect(draw, w / 2 - 1, h - body - radius - 6, 2, 6, GOLD);
            pen.boxed(draw, w / 2 - 5, h - 16, 10, 16, OCEAN);
        }
        SkyBuildingKind::Market => {
            // Striped awnings over stalls of crates and fruit.
            let stalls = (w / 24).max(1);
            for stall in 0..stalls {
                let x = stall * 24 + 2;
                let stripe = if stall % 2 == 0 { RED } else { TEAL };
                for s in 0..10 {
                    pen.rect(
                        draw,
                        x + s * 2,
                        h - 40,
                        2,
                        8,
                        if s % 2 == 0 { stripe } else { CREAM },
                    );
                }
                pen.rect(draw, x, h - 41, 20, 1, INK);
                pen.rect(draw, x + 1, h - 32, 1, 32, SOIL);
                pen.rect(draw, x + 18, h - 32, 1, 32, SOIL);
                pen.boxed(draw, x + 3, h - 12, 14, 12, LOAM);
                for fruit in 0..4 {
                    let color =
                        [ORANGE, LIME, PINK, GOLD][((seed as i32 + fruit + stall) % 4) as usize];
                    pen.rect(draw, x + 4 + fruit * 3, h - 14, 2, 2, color);
                }
            }
        }
        SkyBuildingKind::Monastery => {
            // A tall cream facade with arched windows, a lavender roof and a bell tower.
            let tower_w = 12;
            pen.boxed(draw, 0, 24, w - tower_w, h - 24, CREAM);
            for row in 0..12 {
                pen.rect(
                    draw,
                    row,
                    24 - row - 1,
                    w - tower_w - row * 2,
                    1,
                    if row == 0 { INK } else { LAVENDER },
                );
            }
            for arch in (6..w - tower_w - 6).step_by(12) {
                pen.rect(draw, arch, 40, 6, 14, SLATE);
                pen.rect(draw, arch + 1, 39, 4, 1, SLATE);
                pen.rect(draw, arch + 1, 42, 4, 11, with_alpha(GOLD, 200));
            }
            let tx = w - tower_w;
            pen.boxed(draw, tx, 0, tower_w, h, BONE);
            pen.rect(draw, tx + 3, 6, tower_w - 6, 8, INK);
            let swing = ((seconds * 2.0).sin() * 2.0) as i32;
            pen.rect(draw, tx + 4 + swing, 8, 4, 5, GOLD);
            pen.rect(draw, tx - 1, -4, tower_w + 2, 4, VIOLET);
            pen.boxed(draw, (w - tower_w) / 2 - 5, h - 18, 10, 18, PLUM);
        }
        SkyBuildingKind::Hangar => {
            // A steel barrel vault with a great door, and a small airship moored above.
            let body = h - 16;
            for row in 0..16 {
                let inset = (16 - row) * (16 - row) / 12;
                pen.rect(
                    draw,
                    inset,
                    h - body - 16 + row,
                    w - inset * 2,
                    1,
                    if row == 0 { INK } else { STEEL },
                );
            }
            pen.boxed(draw, 0, h - body, w, body, STEEL_DARK);
            for rib in (4..w).step_by(8) {
                pen.rect(draw, rib, h - body - 4, 1, body + 4, STEEL);
            }
            pen.boxed(draw, w / 2 - 14, h - body + 8, 28, body - 8, NIGHT);
            pen.rect(draw, w / 2 - 13, h - body + 9, 26, 2, AMBER);
            // The moored blimp.
            let bob = ((seconds * 1.3).sin() * 2.0) as i32;
            pen.boxed(draw, w / 2 - 16, -30 + bob, 32, 12, ROSE);
            pen.rect(draw, w / 2 - 14, -26 + bob, 28, 2, PINK);
            pen.boxed(draw, w / 2 - 5, -18 + bob, 10, 5, LOAM);
            pen.rect(
                draw,
                w / 2,
                -13 + bob,
                1,
                h - body - 3 - (-13 + bob),
                SOIL_DARK,
            );
        }
        SkyBuildingKind::Home => {
            // A cosy white home with a garden growing on its flat roof.
            pen.boxed(draw, 2, 14, w - 4, h - 14, MIST);
            pen.rect(draw, 1, 12, w - 2, 3, SOIL);
            for tuft in (2..w - 2).step_by(3) {
                let tall = (hash(building.first_column, tuft, 913) % 4) as i32;
                pen.rect(
                    draw,
                    tuft,
                    9 - tall,
                    2,
                    3 + tall,
                    if tuft % 2 == 0 { LEAF } else { MOSS },
                );
                if hash(building.first_column, tuft, 915).is_multiple_of(5) {
                    pen.px(draw, tuft, 8 - tall, PINK);
                }
            }
            windows(draw, pen, (6, 20), (w - 12, h - 36), 9, seed, seconds);
            pen.boxed(draw, w / 2 - 4, h - 14, 8, 14, TEAL_DARK);
        }
    }
}

/// A grid of windows in `area` (`origin` and size in art pixels), every `step` art pixels; a
/// few are lit (warm at night, a soft sky glint by day).
fn windows(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    origin: (i32, i32),
    area: (i32, i32),
    step: i32,
    seed: u32,
    seconds: f32,
) {
    let mut row = 0;
    while row + 5 <= area.1 {
        let mut column = 0;
        while column + 4 <= area.0 {
            let h = hash(origin.0 + column, origin.1 + row, seed);
            let lit = !h.is_multiple_of(3) || ((seconds * 0.1) as u32 + h).is_multiple_of(7);
            pen.rect(draw, origin.0 + column, origin.1 + row, 4, 5, INK);
            pen.rect(
                draw,
                origin.0 + column + 1,
                origin.1 + row + 1,
                2,
                3,
                if lit { AMBER } else { SKY_LIGHT },
            );
            column += step;
        }
        row += step + 2;
    }
}
