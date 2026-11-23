//! The Living Dark in the world view (Wave 8, Descent III): the Flesh Caverns' walls pulse with
//! the heartbeat, and the Glow Sea's near-total darkness is cut back only by living lights.
//!
//! * **The pulse** ([`draw_flesh_pulse`]): every visible flesh tile takes a dark-red wash that
//!   swells on each heartbeat (a "lub" and a softer "dub"), at the stratum's heartbeat
//!   (`living_dark::heartbeat_bpm`: quicker deeper and while the acid tide rises). Low
//!   contrast and under two beats a second, so it never strobes; off with the effects setting.
//! * **The Glow Sea's dark** ([`draw_glow_sea_darkness`]): every tile of the Glow Sea in view,
//!   rock and open alike, sinks into near-black; light-giving creatures (lure fish, glow
//!   jellies, an angler's bait, the Mycelial Mind and its nodes), rig lamps (a candle there
//!   until Lamp II: `PlayerPresentation::lamp_scale`), pilots' lamps, flares and blasts cut it
//!   back; a faint mint halo marks each living light. Darkness is computed per 8-pixel cell,
//!   quantized with an ordered dither and drawn as merged runs, like the cave darkness. The
//!   seat's post-process light is lifted to full there (`render_snapshot`), so the screen is
//!   not darkened twice. Presentation only: the simulation's own reading is
//!   `WorldState::glow_light_at`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation maps world pixels, tiles and light cells between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    ExplosiveKind, PlacedBomb,
    cave::CaveTile,
    descent::Stratum,
    enemy::EnemyKind,
    entity::{Entity, EntityState},
    explosive::FLARE_LIGHT_RADIUS,
    living_dark::{heartbeat_bpm, tide_at},
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::bayer4;
use crate::{effects::Explosion, render_snapshot::PlayerPresentation};

use super::{
    ART, RigFacings,
    caves::{PILOT_LAMP_TILES, RIG_LAMP_TILES, RIG_LAMP_TILES_PER_SCANNER},
    fx::disc,
};

/// World pixels per light cell (a quarter tile).
const CELL: f32 = 8.0;
/// Darkness levels the overlay is quantized to.
const LEVELS: i32 = 10;
/// How dark the Glow Sea is with no light near (0..1). Wave 10 polish lifted it from 0.95: the
/// rock's silhouettes now read faintly through the deep-sea blue, so a player can find the
/// way, and the living lights still own the scene.
const GLOW_SEA_DARK: f32 = 0.8;
/// The Glow Sea's dark: a deep-sea blue-black rather than plain ink (Wave 10 polish), so the
/// lifted dark keeps the sea's mood.
const GLOW_SEA_INK: Color = Color::new(0x05, 0x0b, 0x18, 255);
/// Strongest heartbeat wash over flesh (alpha 0..255).
const PULSE_ALPHA: f32 = 46.0;

/// The tiles (inclusive) covering `visible`, one tile of margin.
fn tile_span(visible: Rectangle) -> (TilePosition, TilePosition) {
    let frame = crate::view_frame::frame();
    (
        TilePosition {
            x: frame.col(visible.x) - 1,
            y: frame.row(visible.y) - 1,
        },
        TilePosition {
            x: frame.col(visible.x + visible.width) + 1,
            y: frame.row(visible.y + visible.height) + 1,
        },
    )
}

/// The stratum at `tile` on a Deep Descent world (`None` on older worlds).
fn stratum_of(terrain: &Terrain, tile: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    Some(fatherload_simulation::worldgen::descent::stratum_in(
        settings, &layout, tile.x, tile.y,
    ))
}

/// How far through a heartbeat `seconds` is at `bpm`: the wash's strength (0..1), a sharp
/// "lub" and a softer "dub" a quarter of a beat later.
fn beat(seconds: f32, bpm: f32) -> f32 {
    let phase = (seconds * bpm / 60.0).fract();
    let lub = (-((phase / 0.07).powi(2))).exp();
    let dub = (-(((phase - 0.26) / 0.07).powi(2))).exp() * 0.6;
    (lub + dub).min(1.0)
}

/// The Flesh Caverns' walls swelling with the heartbeat (see the module docs). `strength` is
/// the effects setting's share (0 turns it off).
pub(super) fn draw_flesh_pulse(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    world_millis: u64,
    strength: f32,
) {
    if strength <= 0.0 {
        return;
    }
    let (min, max) = tile_span(visible);
    let middle = TilePosition {
        x: i32::midpoint(min.x, max.x),
        y: i32::midpoint(min.y, max.y),
    };
    if stratum_of(terrain, middle) != Some(Stratum::FleshCaverns) {
        return;
    }
    let settings = terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let bpm = heartbeat_bpm(layout.along(middle.y), tide_at(world_millis).0) as f32;
    let seconds = world_millis as f32 / 1_000.0;
    let frame = crate::view_frame::frame();
    for y in min.y..=max.y {
        let mut run: Option<(i32, u8)> = None;
        for x in min.x..=max.x + 1 {
            let alpha = if x <= max.x
                && terrain.tile(TilePosition { x, y }).map(|tile| tile.kind)
                    == Some(TileKind::Cave(CaveTile::Flesh))
            {
                // Waves of the pulse roll through the wall, so it breathes rather than blinks.
                let lag = ((x * 7 + y * 3).rem_euclid(23)) as f32 * 0.004;
                (beat(seconds - lag, bpm) * PULSE_ALPHA * strength) as u8
            } else {
                0
            };
            match run {
                Some((start, current)) if current == alpha && x <= max.x => {
                    let _ = start;
                }
                Some((start, current)) => {
                    if current > 2 {
                        draw.draw_rectangle_rec(
                            Rectangle::new(
                                frame.col_px(start),
                                frame.row_px(y),
                                (x - start) as f32 * TILE_SIZE,
                                TILE_SIZE,
                            ),
                            with_alpha(MAROON, current),
                        );
                    }
                    run = Some((x, alpha));
                }
                None => run = Some((x, alpha)),
            }
        }
    }
}

/// One light: centre (world pixels), radius (pixels), strength (0..1) and whether it is a
/// living light (a mint halo marks it).
#[derive(Clone, Copy, Debug)]
struct Light {
    x: f32,
    y: f32,
    radius: f32,
    strength: f32,
    living: bool,
}

/// What the Glow Sea's darkness needs.
pub(super) struct GlowSeaScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub players: &'a [PlayerPresentation],
    pub pilots: &'a [crate::render_snapshot::PilotPresentation],
    pub facings: &'a RigFacings,
    pub bombs: &'a [PlacedBomb],
    pub explosions: &'a [Explosion],
    pub enemies: &'a [Entity],
    pub seconds: f32,
}

/// Where an angler's bait hangs (the renderer draws its glow there too).
fn angler_bait(x: f32, y: f32, facing: i8, seconds: f32) -> (f32, f32) {
    let side = f32::from(facing.signum());
    let bob = (seconds * 2.0).sin() * ART;
    (side.mul_add(ART * 3.0, x), y - ART * 6.5 + bob)
}

/// Every light in the Glow Sea scene.
fn gather_lights(scene: &GlowSeaScene<'_>) -> Vec<Light> {
    let mut lights = Vec::new();
    for player in scene.players {
        let lamp = (f32::from(player.scanner_level) * RIG_LAMP_TILES_PER_SCANNER + RIG_LAMP_TILES)
            * TILE_SIZE
            * player.lamp_scale;
        lights.push(Light {
            x: player.x,
            y: player.y,
            radius: lamp,
            strength: 1.0,
            living: false,
        });
        let ahead = if scene.facings.facing_left(player) {
            -1.0
        } else {
            1.0
        };
        lights.push(Light {
            x: ahead * lamp * 0.55 + player.x,
            y: player.y + TILE_SIZE * 0.2,
            radius: lamp * 0.75,
            strength: 0.85,
            living: false,
        });
    }
    for pilot in scene.pilots {
        let (x, y) = pilot.middle();
        lights.push(Light {
            x,
            y,
            radius: PILOT_LAMP_TILES * TILE_SIZE * 0.6,
            strength: 0.9,
            living: false,
        });
    }
    for bomb in scene.bombs {
        if bomb.kind == ExplosiveKind::Flare && bomb.burning() {
            lights.push(Light {
                x: bomb.x,
                y: bomb.y,
                radius: FLARE_LIGHT_RADIUS,
                strength: 1.0,
                living: false,
            });
        }
    }
    for blast in scene.explosions {
        let fade = (1.0 - blast.age / 0.6).clamp(0.0, 1.0);
        if fade > 0.0 {
            lights.push(Light {
                x: blast.x,
                y: blast.y,
                radius: TILE_SIZE * 9.0,
                strength: fade,
                living: false,
            });
        }
    }
    for entity in scene.enemies {
        let EntityState::Enemy(state) = &entity.state else {
            continue;
        };
        let reach = state.kind.light_tiles();
        if reach <= 0.0 {
            continue;
        }
        let (x, y) = (entity.body.x, entity.body.y);
        let pulse = 1.0 + 0.1 * (scene.seconds * 2.2 + state.seed as f32 * 0.01).sin();
        let (lx, ly) = if state.kind == EnemyKind::Angler {
            angler_bait(x, y, state.facing, scene.seconds)
        } else {
            (x, y)
        };
        lights.push(Light {
            x: lx,
            y: ly,
            radius: reach * TILE_SIZE * pulse,
            strength: 0.95,
            living: true,
        });
    }
    lights
}

/// The Glow Sea's darkness cut back by living lights (see the module docs).
#[allow(
    clippy::too_many_lines,
    reason = "ambient, lights, the dithered runs and the halos read best in one place"
)]
pub(super) fn draw_glow_sea_darkness(draw: &mut impl RaylibDraw, scene: &GlowSeaScene<'_>) {
    let (min, max) = tile_span(scene.visible);
    let (tw, th) = (max.x - min.x + 1, max.y - min.y + 1);
    // Which tiles are in the Glow Sea (its boundary is ragged).
    let mut sea = vec![false; (tw * th) as usize];
    let mut any = false;
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            if stratum_of(scene.terrain, TilePosition { x, y }) == Some(Stratum::GlowSea) {
                sea[((y - min.y) * tw + (x - min.x)) as usize] = true;
                any = true;
            }
        }
    }
    if !any {
        return;
    }
    let frame = crate::view_frame::frame();
    let in_sea = |px: f32, py: f32| {
        let tile = frame.tile_at(px, py);
        let (lx, ly) = (tile.x - min.x, tile.y - min.y);
        lx >= 0 && ly >= 0 && lx < tw && ly < th && sea[(ly * tw + lx) as usize]
    };
    let first_x = (scene.visible.x / CELL).floor() as i32;
    let first_y = (scene.visible.y / CELL).floor() as i32;
    let cols = (scene.visible.width / CELL).ceil() as i32 + 2;
    let rows = (scene.visible.height / CELL).ceil() as i32 + 2;
    let lights = gather_lights(scene);
    let mut lit = vec![0.0_f32; (cols * rows) as usize];
    for light in &lights {
        let min_col = (((light.x - light.radius) / CELL).floor() as i32 - first_x).max(0);
        let max_col = (((light.x + light.radius) / CELL).ceil() as i32 - first_x).min(cols - 1);
        let min_row = (((light.y - light.radius) / CELL).floor() as i32 - first_y).max(0);
        let max_row = (((light.y + light.radius) / CELL).ceil() as i32 - first_y).min(rows - 1);
        let inverse = 1.0 / (light.radius * light.radius).max(1.0);
        for row in min_row..=max_row {
            let cy = (first_y + row) as f32 * CELL + CELL * 0.5 - light.y;
            for col in min_col..=max_col {
                let cx = (first_x + col) as f32 * CELL + CELL * 0.5 - light.x;
                let falloff = 1.0 - cx.mul_add(cx, cy * cy) * inverse;
                if falloff > 0.0 {
                    let slot = &mut lit[(row * cols + col) as usize];
                    *slot += light.strength * falloff * falloff.mul_add(-0.3, 1.3);
                }
            }
        }
    }
    let step = 255.0 / (LEVELS - 1) as f32;
    for row in 0..rows {
        let y = (first_y + row) as f32 * CELL;
        let mut run: Option<(i32, u8)> = None;
        for col in 0..=cols {
            let alpha = if col < cols {
                let cx = (first_x + col) as f32 * CELL + CELL * 0.5;
                let cy = y + CELL * 0.5;
                if in_sea(cx, cy) {
                    let index = (row * cols + col) as usize;
                    let value = 255.0 * GLOW_SEA_DARK * (1.0 - lit[index].min(1.0));
                    let threshold = f32::from(bayer4(first_x + col, first_y + row)) / 16.0;
                    let level = (value / step + threshold - 0.5)
                        .round()
                        .clamp(0.0, (LEVELS - 1) as f32);
                    (level * step) as u8
                } else {
                    0
                }
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
                            with_alpha(GLOW_SEA_INK, current),
                        );
                    }
                    run = Some((col, alpha));
                }
                None => run = Some((col, alpha)),
            }
        }
    }
    // A faint mint halo round every living light, so the dark glows the creatures' colour.
    for light in lights.iter().filter(|light| light.living) {
        if in_sea(light.x, light.y) {
            // A soft falloff of a few faint discs, brightest at the heart.
            for (share, color, alpha) in [(0.34, TEAL_DARK, 18), (0.2, TEAL, 16), (0.1, MINT, 30)] {
                disc(
                    draw,
                    light.x,
                    light.y,
                    (light.radius * share).max(2.0) as i32,
                    with_alpha(color, alpha),
                );
            }
        }
    }
}

/// How far from the Mycelial Mind its nerve nodes can hang and still be wired to it (pixels).
const MIND_TENDRIL_REACH: f32 = TILE_SIZE * 30.0;

/// The Mycelial Mind's state as the screen shows it: glowing nerve tendrils from each living
/// node to the Mind while they shield it, and while none lives (its exposure) a raw, throbbing
/// rose core and torn tendril stubs, so a player reads at a glance whether hits will land.
/// Drawn over the Glow Sea's dark (the nerves are living light). Slow pulses only (well under
/// three a second); the effects setting's `strength` scales the core's throb.
pub(super) fn draw_mind_nerves(
    draw: &mut impl RaylibDraw,
    enemies: &[Entity],
    seconds: f32,
    strength: f32,
) {
    let kind_of = |entity: &Entity| match &entity.state {
        EntityState::Enemy(state) => Some(state.kind),
        _ => None,
    };
    for mind in enemies
        .iter()
        .filter(|entity| kind_of(entity) == Some(EnemyKind::MycelialMind))
    {
        let (mx, my) = (mind.body.x, mind.body.y);
        let nodes: Vec<(f32, f32)> = enemies
            .iter()
            .filter(|entity| kind_of(entity) == Some(EnemyKind::MindNode))
            .map(|entity| (entity.body.x, entity.body.y))
            .filter(|(x, y)| (x - mx).hypot(y - my) < MIND_TENDRIL_REACH)
            .collect();
        if nodes.is_empty() {
            draw_exposed_core(draw, mx, my, seconds, strength);
            continue;
        }
        for (index, (nx, ny)) in nodes.into_iter().enumerate() {
            draw_tendril(draw, (nx, ny), (mx, my), seconds + index as f32 * 0.37);
        }
    }
}

/// One nerve tendril from a node to the Mind: a wavering chain of art pixels, a bright pulse
/// running along it toward the Mind (the node feeding its shield).
fn draw_tendril(draw: &mut impl RaylibDraw, from: (f32, f32), to: (f32, f32), seconds: f32) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy).max(1.0);
    let (nx, ny) = (-dy / length, dx / length);
    let steps = (length / ART).ceil() as i32;
    let pulse = (seconds * 0.8).fract();
    for step in 0..=steps {
        let t = step as f32 / steps.max(1) as f32;
        // Sway most in the middle, pinned at both ends.
        let sway =
            (t * std::f32::consts::PI).sin() * (t * 9.0 + seconds * 1.7).sin() * TILE_SIZE * 0.6;
        let x = snap_art(nx.mul_add(sway, dx.mul_add(t, from.0)));
        let y = snap_art(ny.mul_add(sway, dy.mul_add(t, from.1)));
        let color = if (t - pulse).abs() < 0.06 {
            with_alpha(LIME, 230)
        } else if step % 3 == 0 {
            with_alpha(MOSS, 200)
        } else {
            with_alpha(LEAF, 150)
        };
        draw.draw_rectangle_rec(Rectangle::new(x, y, ART, ART), color);
    }
}

/// The exposed Mind: a throbbing rose core and three torn stubs where its nerves were.
fn draw_exposed_core(draw: &mut impl RaylibDraw, x: f32, y: f32, seconds: f32, strength: f32) {
    let throb = (seconds * 2.4).sin().mul_add(0.5, 0.5) * strength;
    disc(draw, x, y, 30 + (throb * 6.0) as i32, with_alpha(RED, 40));
    disc(draw, x, y, 18, with_alpha(ROSE, 60 + (throb * 50.0) as u8));
    disc(draw, x, y, 8, with_alpha(PINK, 120));
    for (index, angle) in [0.6_f32, 2.5, 4.3].into_iter().enumerate() {
        let droop = (seconds * 1.3 + index as f32).sin() * 0.15;
        for step in 0..8_u8 {
            let reach = f32::from(step).mul_add(ART, 22.0);
            let px = snap_art((angle + droop).cos().mul_add(reach, x));
            let py = snap_art((angle + droop).sin().mul_add(reach, y) + f32::from(step) * 0.8);
            let alpha = 200_u8.saturating_sub(step * 24);
            draw.draw_rectangle_rec(Rectangle::new(px, py, ART, ART), with_alpha(ROSE, alpha));
        }
    }
}

/// Snaps a world coordinate to the art-pixel grid.
fn snap_art(value: f32) -> f32 {
    crate::pixel_scale::round_half_up(value / ART) * ART
}
