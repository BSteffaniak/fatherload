//! Cave ecosystems in the world view (WP-CAVES): glowing tiles, rippling water, critters and
//! cave darkness lit by rig lamps, flares, blasts and glowing things.
//!
//! **Darkness.** Inside a cave's footprint the view darkens to the cave's ambient darkness
//! (`CaveKind::ambient_darkness`: pitch black in chasms and burrows, dim in geodes and fungal
//! grottos). Lights cut it back: every rig carries a lamp (brighter forward of the rig, wider
//! with the scanner tier), lit flares and blasts flood their surroundings, and crystals,
//! glowcaps, glyphs, algae, lava and glowing critters each light a little pool. Darkness is
//! computed per 8-pixel cell (a quarter tile), eased smoothly across the cave's edge, quantized
//! to a few levels with an ordered dither (so it reads as pixel art) and drawn as merged runs of
//! cells. Presentation only: the simulation never reads it. A view with no cave in it pays one
//! region lookup.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation maps world pixels, tiles and light cells between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, ExplosiveKind, PlacedBomb,
    cave::{CaveTile, CritterKind},
    explosive::FLARE_LIGHT_RADIUS,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        TileRect,
        streamed::{BiomeCell, CaveBiome},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::shade::bayer4,
    terrain::tile_art::{cave_glow, hash},
};
use crate::{
    effects::Explosion,
    render_snapshot::{CritterPresentation, PlayerPresentation},
};

use super::{ART, RigFacings, snap};

/// World pixels per light cell (a quarter tile).
const CELL: f32 = 8.0;
/// Darkness levels the overlay is quantized to.
const LEVELS: i32 = 9;
/// A rig lamp's radius, in tiles, before scanner upgrades.
pub const RIG_LAMP_TILES: f32 = 5.5;
/// Lamp radius each scanner tier adds, in tiles.
pub const RIG_LAMP_TILES_PER_SCANNER: f32 = 0.3;
/// Radius of a pilot's helmet lamp, in tiles (smaller than a rig's floodlights).
pub const PILOT_LAMP_TILES: f32 = 3.5;
/// The colour of cave darkness.
const DARK: Color = Color::new(6, 4, 14, 255);

/// One light: centre (world pixels), radius (pixels) and strength (0..1).
#[derive(Clone, Copy, Debug)]
struct Light {
    x: f32,
    y: f32,
    radius: f32,
    strength: f32,
}

/// The tiles (inclusive) covering `visible`, one tile of margin.
fn tile_rect(visible: Rectangle) -> TileRect {
    TileRect {
        min: TilePosition {
            x: crate::view_frame::frame().col(visible.x) - 1,
            y: crate::view_frame::frame().row(visible.y) - 1,
        },
        max: TilePosition {
            x: crate::view_frame::frame().col_ceil(visible.x + visible.width) + 1,
            y: crate::view_frame::frame().row_ceil(visible.y + visible.height) + 1,
        },
    }
}

fn tile_centre(position: TilePosition) -> (f32, f32) {
    (
        (crate::view_frame::frame().rel_col(position.x) + 0.5) * TILE_SIZE,
        (crate::view_frame::frame().rel_row(position.y) + 0.5) * TILE_SIZE,
    )
}

/// How far a glowing tile lights, in tiles (0: it doesn't).
pub(super) const fn tile_light(kind: TileKind) -> f32 {
    match kind {
        TileKind::Lava => 3.2,
        TileKind::MagmaVent => 2.2,
        // Molten metal glows (Part III F9).
        TileKind::Fluid(fatherload_simulation::fluid::FluidKind::LiquidMetal) => 1.8,
        TileKind::Cave(tile) => tile.light_radius() as f32 * 1.1,
        // The Sinkhole's core and the glow in a lava tube's glass (Wave 8).
        TileKind::Crust(tile) => tile.light_radius() as f32 * 1.1,
        // The Moon's blooms, glyphs, veils and core (WP-MOON).
        TileKind::Lunar(tile) => super::lunar::lunar_light(tile),
        // The Old Deep's glowing rock, crystal, vents and the core (Wave 8).
        TileKind::Deep(tile) => super::old_deep::deep_light(tile),
        // The Hollow World's sun glass, amber, slag and the inner sun (Wave 9).
        TileKind::Hollow(tile) => tile.glow() as f32 / 255.0 * 3.2,
        // The distortion strata's glass, ore and lit pixels (Wave 9).
        TileKind::Warped(tile) => tile.glow() as f32 / 64.0,
        // The buried city's lamps, cores, plinths and star glass (Wave 10).
        TileKind::City(tile) => tile.glow() as f32 / 255.0 * 3.4,
        _ => 0.0,
    }
}

/// Where a critter is now (its home plus its pure-function flight path).
#[must_use]
pub fn critter_position(critter: &CritterPresentation, seconds: f32) -> (f32, f32) {
    let (dx, dy) = critter.state.offset_at(seconds);
    (critter.home_x + dx, critter.home_y + dy)
}

/// Glowing cave tiles throw soft pulsing halos (crystal glints twinkle), and cave water
/// shimmers along its surface. Only explored tiles glow, so a halo never gives a cave away.
pub(super) fn draw_cave_glows(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    seconds: f32,
) {
    let rect = tile_rect(visible);
    if terrain.caves_touching(&rect).is_empty() {
        return;
    }
    for position in rect.positions() {
        if !explored.is_some_and(|tiles| tiles.contains(position)) {
            continue;
        }
        let Some(tile) = terrain.tile(position) else {
            continue;
        };
        let (cx, cy) = tile_centre(position);
        let phase = (hash(position.x, position.y, 7) % 628) as f32 / 100.0;
        let pulse = (seconds * 1.6 + phase).sin().mul_add(0.5, 0.5);
        match tile.kind {
            TileKind::Cave(cave) => {
                let Some(color) = cave_glow(cave, terrain.band_at(position)) else {
                    continue;
                };
                let radius = match cave {
                    CaveTile::Crystal | CaveTile::GlowCap => 14,
                    CaveTile::TreasureCache | CaveTile::RelicCache => 9,
                    _ => 11,
                };
                halo(draw, cx, cy, radius, color, 10.0 + pulse * 12.0);
                if cave == CaveTile::Crystal {
                    // A glint that winks on and off.
                    let h = hash(position.x, position.y, 11);
                    if (seconds * 0.7 + (h % 97) as f32 / 31.0).fract() < 0.12 {
                        let gx = snap(cx + ((h >> 4) % 10) as f32 * ART - 5.0 * ART);
                        let gy = snap(cy + ((h >> 9) % 10) as f32 * ART - 5.0 * ART);
                        draw.draw_rectangle_rec(
                            Rectangle::new(gx - ART, gy, ART * 3.0, ART),
                            CREAM,
                        );
                        draw.draw_rectangle_rec(
                            Rectangle::new(gx, gy - ART, ART, ART * 3.0),
                            CREAM,
                        );
                    }
                }
            }
            TileKind::Air if terrain.water_at(position) => {
                let above = TilePosition {
                    x: position.x,
                    y: position.y - 1,
                };
                if terrain.water_at(above) {
                    continue;
                }
                // The surface: bright ripples sliding along the waterline.
                let surface =
                    crate::view_frame::frame().rel_row(position.y) * TILE_SIZE + 3.0 * ART;
                for ripple in 0..3 {
                    let h = hash(position.x, ripple, 13);
                    let speed = 3.0 + (h % 4) as f32;
                    let travel = (seconds * speed + (h % 16) as f32) % 16.0;
                    let x = crate::view_frame::frame().rel_col(position.x) * TILE_SIZE
                        + snap(travel * ART);
                    draw.draw_rectangle_rec(
                        Rectangle::new(x, surface, ART * 3.0, ART),
                        with_alpha(ICE, 170),
                    );
                }
            }
            _ => {}
        }
    }
}

/// A soft disc of light: a few stacked translucent pixel discs.
fn halo(draw: &mut impl RaylibDraw, x: f32, y: f32, radius: i32, color: Color, alpha: f32) {
    super::fx::disc(draw, x, y, radius, with_alpha(color, (alpha * 0.6) as u8));
    super::fx::disc(draw, x, y, radius * 2 / 3, with_alpha(color, alpha as u8));
}

/// Draws the cave critters in view.
pub(super) fn draw_critters(
    draw: &mut impl RaylibDraw,
    critters: &[CritterPresentation],
    players: &[PlayerPresentation],
    visible: Rectangle,
    seconds: f32,
) {
    for critter in critters {
        let (mut x, mut y) = critter_position(critter, seconds);
        if x < visible.x - TILE_SIZE
            || y < visible.y - TILE_SIZE
            || x > visible.x + visible.width + TILE_SIZE
            || y > visible.y + visible.height + TILE_SIZE
        {
            continue;
        }
        // Bats and moths shy away from a rig that comes close.
        if matches!(critter.state.kind, CritterKind::Bat | CritterKind::GlowMoth) {
            for player in players {
                let (dx, dy) = (x - player.x, y - player.y);
                let distance = dx.hypot(dy).max(1.0);
                let reach = TILE_SIZE * 2.5;
                if distance < reach {
                    let push = (reach - distance) * 0.8;
                    x += dx / distance * push;
                    y += dy / distance * push;
                }
            }
        }
        let (x, y) = (snap(x), snap(y));
        let flap = ((seconds * 14.0 + f32::from(critter.state.phase) / 4_000.0) as i32) % 2 == 0;
        let mut plot = |dx: i32, dy: i32, color: Color| {
            draw.draw_rectangle_rec(
                Rectangle::new(x + dx as f32 * ART, y + dy as f32 * ART, ART, ART),
                color,
            );
        };
        match critter.state.kind {
            CritterKind::Firefly => {
                let blink = (seconds * 1.3 + f32::from(critter.state.phase) / 9_000.0).sin();
                let glow = if blink > -0.3 { LIME } else { MOSS };
                plot(0, 0, glow);
                if blink > 0.2 {
                    plot(-1, 0, with_alpha(LIME, 110));
                    plot(1, 0, with_alpha(LIME, 110));
                    plot(0, -1, with_alpha(LIME, 110));
                    plot(0, 1, with_alpha(CREAM, 180));
                }
            }
            CritterKind::GlowMoth | CritterKind::EmberMoth => {
                let (body, wing) = if critter.state.kind == CritterKind::GlowMoth {
                    (CREAM, with_alpha(ICE, 200))
                } else {
                    (GOLD, with_alpha(ORANGE, 210))
                };
                plot(0, 0, body);
                plot(0, 1, body);
                let rise = i32::from(flap);
                plot(-1, -rise, wing);
                plot(1, -rise, wing);
                if !flap {
                    plot(-2, 0, wing);
                    plot(2, 0, wing);
                }
            }
            CritterKind::Bat => {
                plot(0, 0, NIGHT);
                plot(0, 1, INK);
                plot(-1, 0, ROSE);
                if flap {
                    for dx in [-3, -2, -1, 1, 2, 3] {
                        plot(dx, -1 + i32::from(dx.abs() == 3), INK);
                    }
                } else {
                    for dx in [-2, -1, 1, 2] {
                        plot(dx, 1, INK);
                    }
                    plot(-3, 2, INK);
                    plot(3, 2, INK);
                }
            }
            CritterKind::CaveFish => {
                // Facing the way it swims.
                let (dx, _) = critter.state.offset_at(seconds + 0.05);
                let (now, _) = critter.state.offset_at(seconds);
                let dir = if dx >= now { 1 } else { -1 };
                for step in -1..=1 {
                    plot(step * dir, 0, BONE);
                }
                plot(2 * dir, 0, CREAM);
                plot(-2 * dir, -1, MIST);
                plot(-2 * dir, 1, MIST);
                plot(0, -1, with_alpha(MIST, 160));
            }
        }
    }
}

/// Pilots' helmet lamps (Part II F5): a small pool around each pilot and a beam where it looks.
fn pilot_lamps(pilots: &[crate::render_snapshot::PilotPresentation], lights: &mut Vec<Light>) {
    for pilot in pilots {
        let (x, y) = pilot.middle();
        let ahead = f32::from(pilot.state.facing.signum());
        lights.push(Light {
            x,
            y,
            radius: PILOT_LAMP_TILES * TILE_SIZE,
            strength: 0.95,
        });
        lights.push(Light {
            x: ahead.mul_add(PILOT_LAMP_TILES * TILE_SIZE * 0.6, x),
            y: TILE_SIZE.mul_add(-0.3, y),
            radius: PILOT_LAMP_TILES * TILE_SIZE * 0.7,
            strength: 0.8,
        });
    }
}

/// The lights in view: rig lamps (with a forward beam), pilots' lamps, flares, blasts,
/// glowing tiles and glowing critters.
#[allow(clippy::too_many_lines, reason = "one block per kind of light")]
fn gather_lights(scene: &DarknessScene<'_>, rect: &TileRect) -> Vec<Light> {
    let DarknessScene {
        terrain,
        players,
        pilots,
        facings,
        bombs,
        explosions,
        critters,
        seconds,
        beam_lights,
        ..
    } = *scene;
    let mut lights = Vec::new();
    for &(x, y, radius) in beam_lights {
        lights.push(Light {
            x,
            y,
            radius,
            strength: 0.8,
        });
    }
    for player in players {
        let lamp = (f32::from(player.scanner_level) * RIG_LAMP_TILES_PER_SCANNER + RIG_LAMP_TILES)
            * TILE_SIZE
            * player.lamp_scale;
        lights.push(Light {
            x: player.x,
            y: player.y,
            radius: lamp,
            strength: 1.0,
        });
        let ahead = if facings.facing_left(player) {
            -1.0
        } else {
            1.0
        };
        lights.push(Light {
            x: player.x + ahead * lamp * 0.55,
            y: player.y + TILE_SIZE * 0.2,
            radius: lamp * 0.75,
            strength: 0.9,
        });
    }
    pilot_lamps(pilots, &mut lights);
    for bomb in bombs {
        if bomb.kind == ExplosiveKind::Flare && bomb.burning() {
            let flicker = (seconds * 23.0 + bomb.id as f32).sin() * 0.04;
            lights.push(Light {
                x: bomb.x,
                y: bomb.y,
                radius: FLARE_LIGHT_RADIUS * (1.0 + flicker),
                strength: 1.0,
            });
        } else {
            // A fuse glows a little.
            lights.push(Light {
                x: bomb.x,
                y: bomb.y,
                radius: TILE_SIZE * 1.2,
                strength: 0.5,
            });
        }
    }
    for blast in explosions {
        let fade = (1.0 - blast.age / 0.6).clamp(0.0, 1.0);
        if fade > 0.0 {
            lights.push(Light {
                x: blast.x,
                y: blast.y,
                radius: TILE_SIZE * 9.0,
                strength: fade,
            });
        }
    }
    for position in rect.positions() {
        let Some(tile) = terrain.tile(position) else {
            continue;
        };
        let reach = tile_light(tile.kind);
        if reach > 0.0 {
            let (x, y) = tile_centre(position);
            lights.push(Light {
                x,
                y,
                radius: reach * TILE_SIZE,
                strength: 0.75,
            });
        }
    }
    for critter in critters {
        let reach = critter.state.kind.light_radius();
        if reach > 0.0 {
            let (x, y) = critter_position(critter, seconds);
            lights.push(Light {
                x,
                y,
                radius: reach * TILE_SIZE,
                strength: 0.4,
            });
        }
    }
    lights
}

/// The ambient darkness of tile `(x, y)` of `biome` at full `level`: the open interior and the
/// wall right against it are fully dark, the next ring of wall two-thirds, and rock farther out
/// stays as it is, so the darkness traces the cave's own shape instead of smudging its whole
/// footprint.
fn interior_shade(biome: &CaveBiome, x: i32, y: i32, level: u8) -> u8 {
    if biome.cell(x, y) == BiomeCell::Outside {
        return 0;
    }
    let open_within = |reach: i32| {
        (-reach..=reach).any(|dy| {
            (-reach..=reach).any(|dx| {
                matches!(
                    biome.cell(x + dx, y + dy),
                    BiomeCell::Open | BiomeCell::Water
                )
            })
        })
    };
    if open_within(1) {
        level
    } else if open_within(2) {
        (u16::from(level) * 2 / 3) as u8
    } else {
        0
    }
}

/// What one view's darkness pass needs.
pub(super) struct DarknessScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub players: &'a [PlayerPresentation],
    /// Pilots on foot: their headlamps light the dark too (Part II F5).
    pub pilots: &'a [crate::render_snapshot::PilotPresentation],
    pub facings: &'a RigFacings,
    pub bombs: &'a [PlacedBomb],
    pub explosions: &'a [Explosion],
    pub critters: &'a [CritterPresentation],
    pub seconds: f32,
    /// Extra lights `(x, y, radius)` in local pixels: the refracted headlamp's beams (Wave 8).
    pub beam_lights: &'a [(f32, f32, f32)],
}

/// Darkens the cave parts of the view, cut back by every light (see the module docs).
#[allow(
    clippy::too_many_lines,
    reason = "ambient, lights and the dithered run pass read best in one place"
)]
pub(super) fn draw_cave_darkness(draw: &mut impl RaylibDraw, scene: &DarknessScene<'_>) {
    let rect = tile_rect(scene.visible);
    let caves = scene.terrain.caves_touching(&rect);
    if caves.is_empty() {
        return;
    }
    // Ambient darkness per tile over the view (plus margin).
    let (tw, th) = (rect.width(), rect.height());
    let mut ambient = vec![0_u8; (tw * th) as usize];
    for region in &caves {
        let Some(biome) = region.biome.as_ref() else {
            continue;
        };
        let level = biome.kind.ambient_darkness();
        let bounds = biome.bounds();
        for y in bounds.min.y.max(rect.min.y)..=bounds.max.y.min(rect.max.y) {
            for x in bounds.min.x.max(rect.min.x)..=bounds.max.x.min(rect.max.x) {
                let shade = interior_shade(biome, x, y, level);
                if shade > 0 {
                    let index = ((y - rect.min.y) * tw + (x - rect.min.x)) as usize;
                    ambient[index] = ambient[index].max(shade);
                }
            }
        }
    }
    if ambient.iter().all(|level| *level == 0) {
        return;
    }
    let ambient_at = |x: i32, y: i32| -> f32 {
        let (lx, ly) = (
            (x - rect.min.x).clamp(0, tw - 1),
            (y - rect.min.y).clamp(0, th - 1),
        );
        f32::from(ambient[(ly * tw + lx) as usize])
    };
    // Cells over the visible area, snapped to the cell grid.
    let first_x = (scene.visible.x / CELL).floor() as i32;
    let first_y = (scene.visible.y / CELL).floor() as i32;
    let cols = (scene.visible.width / CELL).ceil() as i32 + 2;
    let rows = (scene.visible.height / CELL).ceil() as i32 + 2;
    let mut dark = vec![0.0_f32; (cols * rows) as usize];
    for row in 0..rows {
        for col in 0..cols {
            let cx = (first_x + col) as f32 * CELL + CELL * 0.5;
            let cy = (first_y + row) as f32 * CELL + CELL * 0.5;
            // Bilinear between tile centres: a smooth edge across the cave's rim.
            let fx = cx / TILE_SIZE - 0.5;
            let fy = cy / TILE_SIZE - 0.5;
            let (lx0, ly0) = (fx.floor() as i32, fy.floor() as i32);
            let (sx, sy) = (fx - lx0 as f32, fy - ly0 as f32);
            // Absolute tiles of the presented frame (F7).
            let (x0, y0) = (
                crate::view_frame::frame().col(0.0) + lx0,
                crate::view_frame::frame().row(0.0) + ly0,
            );
            let top = (ambient_at(x0 + 1, y0) - ambient_at(x0, y0)).mul_add(sx, ambient_at(x0, y0));
            let bottom = (ambient_at(x0 + 1, y0 + 1) - ambient_at(x0, y0 + 1))
                .mul_add(sx, ambient_at(x0, y0 + 1));
            dark[(row * cols + col) as usize] = (bottom - top).mul_add(sy, top);
        }
    }
    // Lights cut the darkness back.
    let lights = gather_lights(scene, &rect);
    let mut lit = vec![0.0_f32; dark.len()];
    for light in &lights {
        let min_col = (((light.x - light.radius) / CELL).floor() as i32 - first_x).max(0);
        let max_col = (((light.x + light.radius) / CELL).ceil() as i32 - first_x).min(cols - 1);
        let min_row = (((light.y - light.radius) / CELL).floor() as i32 - first_y).max(0);
        let max_row = (((light.y + light.radius) / CELL).ceil() as i32 - first_y).min(rows - 1);
        let inverse = 1.0 / (light.radius * light.radius);
        for row in min_row..=max_row {
            let cy = (first_y + row) as f32 * CELL + CELL * 0.5 - light.y;
            for col in min_col..=max_col {
                let cx = (first_x + col) as f32 * CELL + CELL * 0.5 - light.x;
                let falloff = 1.0 - cx.mul_add(cx, cy * cy) * inverse;
                if falloff > 0.0 {
                    let slot = &mut lit[(row * cols + col) as usize];
                    *slot += light.strength * falloff * falloff.mul_add(-0.35, 1.35);
                }
            }
        }
    }
    // Quantize with an ordered dither and draw merged runs.
    let step = 255.0 / (LEVELS - 1) as f32;
    for row in 0..rows {
        let y = (first_y + row) as f32 * CELL;
        let mut run: Option<(i32, u8)> = None;
        for col in 0..=cols {
            let alpha = if col < cols {
                let index = (row * cols + col) as usize;
                let value = dark[index] * (1.0 - lit[index].min(1.0));
                let threshold = f32::from(bayer4(first_x + col, first_y + row)) / 16.0;
                let level = (value / step + threshold - 0.5)
                    .round()
                    .clamp(0.0, (LEVELS - 1) as f32);
                (level * step) as u8
            } else {
                0
            };
            match run {
                Some((start, current)) if current == alpha && col < cols => {
                    let _ = start;
                }
                Some((start, current)) => {
                    if current > 0 {
                        let x = (first_x + start) as f32 * CELL;
                        draw.draw_rectangle_rec(
                            Rectangle::new(x, y, (col - start) as f32 * CELL, CELL),
                            with_alpha(DARK, current),
                        );
                    }
                    run = Some((col, alpha));
                }
                None => run = Some((col, alpha)),
            }
        }
    }
}
