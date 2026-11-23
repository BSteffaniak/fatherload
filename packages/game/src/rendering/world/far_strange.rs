//! The far Strange Layers in world space (Wave 9 strange-b, Descent VI strata 4-6 and its gate).
//! Presentation only: everything derives from the replicated far state, the terrain (a pure
//! function of the seed for the time pockets) and the clocks.
//!
//! * **Time Sludge**: motes hanging still in a slow pool, streaks racing through a fast lane;
//!   time bubbles as amber spheres with a ticking rim; an ambered rig glazed in amber.
//! * **Echo Caves**: total darkness drawn in the world, cut back only by each sonar ping's
//!   wave (a cyan ring running out, the cave lit as it passes and fading), echo quartz's
//!   faint glimmer and the rig's pinprick lamp.
//! * **Prism Desert**: keyed walls glow their colour; a filtered lamp throws a pool of its
//!   colour on the rock round the rig.
//! * **The Rewrite Hall**: its glyphs light with the rule being written; mid-rewrite the hall
//!   flickers and The Glitch tears into colour channels; before a blink its ghost shows where
//!   it will be.
//!
//! Every flicker is slow and low contrast and scales with the effects setting (off: none).

use raylib::prelude::*;

use fatherload_simulation::{
    ExplosiveKind, PlacedBomb,
    cave::CaveTile,
    descent::Stratum,
    entity::{Entity, EntityState},
    far_strange::{
        FarStrange, GlitchRule, LampFilter, PRISM_KEY_REACH_TILES, TimePocketKind, sonar_reveal,
        time_bubble_radius,
    },
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::far_strange::{self as sites, FarStampKind},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::bayer4;
use crate::{effects::Explosion, render_snapshot::PlayerPresentation};

use super::fx::disc;

/// World pixels per light cell (a quarter tile).
const CELL: f32 = 8.0;
/// Darkness levels the overlay is quantized to.
const LEVELS: i32 = 10;
/// How dark the Echo Caves are with no light near (0..1).
const ECHO_DARK: f32 = 0.97;
/// The rig's pinprick lamp in the Echo Caves, tiles.
const ECHO_LAMP_TILES: f32 = 1.6;

/// What one view needs to draw the far Strange Layers.
pub(super) struct FarScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub far: &'a FarStrange,
    pub players: &'a [PlayerPresentation],
    pub bombs: &'a [PlacedBomb],
    pub explosions: &'a [Explosion],
    pub enemies: &'a [Entity],
    pub arena: Option<TileRect>,
    /// World milliseconds of the drawn moment.
    pub world_millis: u64,
    pub tick_rate_hz: u32,
    pub world_tick: u64,
    pub seconds: f32,
    /// The effects setting's share (0 turns flicker and shimmer off).
    pub effects: f32,
}

impl FarScene<'_> {
    fn millis_of(&self, tick: u64) -> u64 {
        tick.saturating_mul(1_000) / u64::from(self.tick_rate_hz.max(1))
    }
}

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

/// The far stratum at `tile`, if it is one.
fn far_stratum(terrain: &Terrain, tile: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let stratum =
        fatherload_simulation::worldgen::descent::stratum_in(settings, &layout, tile.x, tile.y);
    sites::is_far(stratum).then_some(stratum)
}

/// Whether the view touches a far stratum at all (most frames: no, and nothing is drawn).
fn view_touches(scene: &FarScene<'_>, stratum: Stratum) -> bool {
    let settings = scene.terrain.settings();
    if !settings.generator.has_descents() {
        return false;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let (min, max) = tile_span(scene.visible);
    let (top, bottom) = layout.rows(stratum);
    // The ragged boundaries reach a score of rows past the layout rows.
    max.y >= top - 40 && min.y <= bottom + 40
}

/// A colour per lamp filter.
const fn filter_color(filter: LampFilter) -> Color {
    match filter {
        LampFilter::White => CREAM,
        LampFilter::Red => RED,
        LampFilter::Green => LIME,
        LampFilter::Blue => SKY,
    }
}

// --- The Time Sludge --------------------------------------------------------------------------

/// Motes in slow pools, streaks in fast lanes, the clock hall's ticking stripes.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_time_air(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    if !view_touches(scene, Stratum::TimeSludge) {
        return;
    }
    let settings = scene.terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let frame = crate::view_frame::frame();
    let area = {
        let (min, max) = tile_span(scene.visible);
        TileRect { min, max }
    };
    let strength = scene.effects.max(0.35);
    for stamp in sites::stamps_with(settings, &layout, &area) {
        let kind = match stamp.kind {
            FarStampKind::TimePocket(kind) => kind,
            FarStampKind::ClockHall => {
                draw_clock_face(draw, &stamp.bounds, scene);
                continue;
            }
            _ => continue,
        };
        let b = stamp.bounds;
        let count = (b.width() * b.height() / 6).clamp(6, 90);
        for index in 0..count {
            let seed =
                ((stamp.seed >> (index % 48)) as u32) ^ (index as u32).wrapping_mul(2_654_435_761);
            let fx = (seed % 997) as f32 / 997.0;
            let fy = ((seed / 997) % 991) as f32 / 991.0;
            let tx = b.min.x as f32 + fx * b.width() as f32;
            let ty = b.min.y as f32 + fy * b.height() as f32;
            let tile = TilePosition {
                x: tx.floor() as i32,
                y: ty.floor() as i32,
            };
            if !sites::pocket_open(&stamp, tile.x, tile.y)
                || !scene
                    .terrain
                    .tile(tile)
                    .is_some_and(|found| found.kind.is_open())
            {
                continue;
            }
            let (x, y) = (
                frame.rel_col(tile.x) * TILE_SIZE + (tx.fract()) * TILE_SIZE,
                frame.rel_row(tile.y) * TILE_SIZE + (ty.fract()) * TILE_SIZE,
            );
            match kind {
                TimePocketKind::Slow => {
                    // Hanging still, barely drifting: a mote and its faint halo.
                    let drift = (scene.seconds * 0.15 + fx * 6.0).sin() * 2.0;
                    disc(
                        draw,
                        x,
                        y + drift,
                        3,
                        with_alpha(GOLD, (40.0 * strength) as u8),
                    );
                    disc(
                        draw,
                        x,
                        y + drift,
                        1,
                        with_alpha(CREAM, (170.0 * strength) as u8),
                    );
                }
                TimePocketKind::Fast => {
                    // Racing streaks along the lane.
                    let run = (scene.seconds * 1.8 + fx * 3.0).fract();
                    let sx = b.min.x as f32 * TILE_SIZE - frame.origin_col() as f32 * TILE_SIZE
                        + run * b.width() as f32 * TILE_SIZE;
                    let length = 10.0 + fy * 18.0;
                    draw.draw_line_ex(
                        Vector2::new(sx - length, y),
                        Vector2::new(sx, y),
                        1.5,
                        with_alpha(ICE, (120.0 * strength) as u8),
                    );
                    let _ = x;
                }
            }
        }
    }
}

/// The Stopped Clock's face over its hall: a handless dial, its ticks lit slow and fast.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
fn draw_clock_face(draw: &mut impl RaylibDraw, hall: &TileRect, scene: &FarScene<'_>) {
    let frame = crate::view_frame::frame();
    let centre = hall.center();
    let (cx, cy) = frame.tile_center(TilePosition {
        x: centre.x,
        y: hall.min.y + 4,
    });
    let radius = TILE_SIZE * 3.2;
    draw.draw_circle_v(Vector2::new(cx, cy), radius + 4.0, with_alpha(INK, 160));
    draw.draw_circle_v(Vector2::new(cx, cy), radius, with_alpha(SOIL_DARK, 220));
    draw.draw_circle_lines(cx as i32, cy as i32, radius, with_alpha(AMBER, 200));
    for tick in 0..12 {
        let angle = tick as f32 * std::f32::consts::TAU / 12.0;
        let (sin, cos) = angle.sin_cos();
        let inner = radius - 7.0;
        // The ticks light in turn, slowly (no hands: time here is stopped).
        let lit = ((scene.seconds * 0.5) as i32).rem_euclid(12) == tick;
        draw.draw_line_ex(
            Vector2::new(cx + sin * inner, cy - cos * inner),
            Vector2::new(cx + sin * (radius - 2.0), cy - cos * (radius - 2.0)),
            2.0,
            if lit { GOLD } else { with_alpha(AMBER, 150) },
        );
    }
    disc(draw, cx, cy, 3, AMBER);
}

/// Time bubbles: amber spheres with a ticking rim (slow inside).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_time_bubbles(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    for bomb in scene.bombs {
        if bomb.kind != ExplosiveKind::TimeBubble {
            continue;
        }
        let radius = time_bubble_radius(bomb.stage);
        if radius <= 0.0 {
            continue;
        }
        let centre = Vector2::new(bomb.x, bomb.y);
        draw.draw_circle_v(centre, radius, with_alpha(GOLD, 34));
        draw.draw_circle_v(centre, radius * 0.7, with_alpha(AMBER, 22));
        draw.draw_circle_lines(bomb.x as i32, bomb.y as i32, radius, with_alpha(GOLD, 170));
        // A slow ticking hand round the rim.
        let angle = scene.seconds * 0.7 * std::f32::consts::TAU / 4.0;
        for tick in 0..16 {
            let a = tick as f32 * std::f32::consts::TAU / 16.0;
            let (sin, cos) = a.sin_cos();
            let lit = ((a - angle).rem_euclid(std::f32::consts::TAU)) < 0.4;
            disc(
                draw,
                bomb.x + sin * (radius - 3.0),
                bomb.y - cos * (radius - 3.0),
                if lit { 2 } else { 1 },
                if lit { CREAM } else { with_alpha(AMBER, 160) },
            );
        }
    }
}

/// An ambered rig glazed in amber, drips running off it.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_amber(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    if scene.far.ambered.is_empty() {
        return;
    }
    for player in scene.players {
        let Some(until) = scene.far.ambered.get(&player.player_id) else {
            continue;
        };
        if until.get() <= scene.world_tick {
            continue;
        }
        let left = scene
            .millis_of(until.get())
            .saturating_sub(scene.world_millis);
        let fade = (left as f32 / 600.0).clamp(0.0, 1.0);
        let alpha = (110.0 * fade) as u8;
        draw.draw_rectangle_rounded(
            Rectangle::new(player.x - 20.0, player.y - 18.0, 40.0, 34.0),
            0.4,
            6,
            with_alpha(AMBER, alpha),
        );
        for drip in 0..3 {
            let t = (scene.seconds * 0.6 + drip as f32 * 0.33).fract();
            disc(
                draw,
                player.x - 12.0 + drip as f32 * 12.0,
                player.y + 16.0 + t * 10.0,
                2,
                with_alpha(GOLD, (f32::from(alpha) * (1.0 - t)) as u8),
            );
        }
    }
}

// --- The Echo Caves --------------------------------------------------------------------------

/// The Echo Caves' darkness: everything black but what a ping's wave lights for a moment, the
/// quartz's glimmer and a pinprick lamp; each ping's wave front drawn as a cyan ring.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "presentation math on small bounded values; one pass over the view"
)]
pub(super) fn draw_echo_darkness(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    if !view_touches(scene, Stratum::EchoCaves) {
        return;
    }
    let (min, max) = tile_span(scene.visible);
    let (tw, th) = (max.x - min.x + 1, max.y - min.y + 1);
    // Which tiles are in the Echo Caves (its boundaries are ragged) and how lit they are.
    let mut echo = vec![false; (tw * th) as usize];
    let mut lit_tiles = vec![0.0_f32; (tw * th) as usize];
    let mut any = false;
    let pings: Vec<(fatherload_simulation::far_strange::SonarPing, u64)> = scene
        .far
        .pings
        .iter()
        .map(|ping| {
            (
                *ping,
                scene
                    .world_millis
                    .saturating_sub(scene.millis_of(ping.tick.get())),
            )
        })
        .collect();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let tile = TilePosition { x, y };
            if far_stratum(scene.terrain, tile) != Some(Stratum::EchoCaves) {
                continue;
            }
            any = true;
            let index = ((y - min.y) * tw + (x - min.x)) as usize;
            echo[index] = true;
            let mut light = pings
                .iter()
                .map(|(ping, since)| sonar_reveal(ping, *since, x, y))
                .fold(0.0_f32, f32::max);
            if scene.terrain.tile(tile).map(|found| found.kind)
                == Some(TileKind::Cave(CaveTile::EchoQuartz))
            {
                light = light.max(0.12);
            }
            lit_tiles[index] = light;
        }
    }
    if !any {
        return;
    }
    let frame = crate::view_frame::frame();
    let first_x = (scene.visible.x / CELL).floor() as i32;
    let first_y = (scene.visible.y / CELL).floor() as i32;
    let cols = (scene.visible.width / CELL).ceil() as i32 + 2;
    let rows = (scene.visible.height / CELL).ceil() as i32 + 2;
    // Small lamps: each rig's pinprick, flares, blasts.
    let mut lamps: Vec<(f32, f32, f32, f32)> = scene
        .players
        .iter()
        .map(|player| (player.x, player.y, ECHO_LAMP_TILES * TILE_SIZE, 0.9))
        .collect();
    for bomb in scene.bombs {
        if bomb.kind == ExplosiveKind::Flare && bomb.burning() {
            lamps.push((
                bomb.x,
                bomb.y,
                fatherload_simulation::explosive::FLARE_LIGHT_RADIUS,
                1.0,
            ));
        }
    }
    for blast in scene.explosions {
        let fade = (1.0 - blast.age / 0.6).clamp(0.0, 1.0);
        if fade > 0.0 {
            lamps.push((blast.x, blast.y, TILE_SIZE * 8.0, fade));
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
                let tile = frame.tile_at(cx, cy);
                let (lx, ly) = (tile.x - min.x, tile.y - min.y);
                if lx >= 0 && ly >= 0 && lx < tw && ly < th && echo[(ly * tw + lx) as usize] {
                    let mut light = lit_tiles[(ly * tw + lx) as usize];
                    for (lamp_x, lamp_y, radius, strength) in &lamps {
                        let (dx, dy) = (cx - lamp_x, cy - lamp_y);
                        let falloff = 1.0 - dx.mul_add(dx, dy * dy) / (radius * radius).max(1.0);
                        if falloff > 0.0 {
                            light += strength * falloff;
                        }
                    }
                    let value = 255.0 * ECHO_DARK * (1.0 - light.min(1.0));
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
                Some((_, current)) if current == alpha && col < cols => {}
                Some((start, current)) => {
                    if current > 0 {
                        let x = (first_x + start) as f32 * CELL;
                        draw.draw_rectangle_rec(
                            Rectangle::new(x, y, (col - start) as f32 * CELL, CELL),
                            with_alpha(INK, current),
                        );
                    }
                    run = Some((col, alpha));
                }
                None => run = Some((col, alpha)),
            }
        }
    }
    // The wave fronts: a cyan ring running out from each ping, fading as it goes.
    for (ping, since) in &pings {
        let seconds = *since as f32 / 1_000.0;
        let reach = f32::from(ping.reach);
        let travelled = seconds * fatherload_simulation::far_strange::SONAR_WAVE_TILES_PER_SECOND;
        if travelled > reach {
            continue;
        }
        let (cx, cy) = frame.tile_center(TilePosition {
            x: ping.x,
            y: ping.y,
        });
        let fade = 1.0 - travelled / reach.max(1.0);
        let radius = travelled * TILE_SIZE;
        for (offset, alpha) in [(0.0, 210.0), (-5.0, 90.0), (-10.0, 40.0)] {
            let r = (radius + offset).max(1.0);
            draw.draw_circle_lines(
                cx as i32,
                cy as i32,
                r,
                with_alpha(SKY_LIGHT, (alpha * fade) as u8),
            );
        }
    }
}

// --- The Prism Desert ------------------------------------------------------------------------

/// Keyed walls glow their colour; a filtered lamp throws a pool of its colour round the rig.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_prism_light(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    if !view_touches(scene, Stratum::PrismDesert) {
        return;
    }
    let frame = crate::view_frame::frame();
    let (min, max) = tile_span(scene.visible);
    let shimmer = 0.5 + 0.5 * (scene.seconds * 1.4).sin() * scene.effects;
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let tile = TilePosition { x, y };
            let Some(TileKind::Cave(cave)) = scene.terrain.tile(tile).map(|found| found.kind)
            else {
                continue;
            };
            let Some(filter) = LampFilter::of_wall(cave) else {
                continue;
            };
            let (cx, cy) = frame.tile_center(tile);
            disc(
                draw,
                cx,
                cy,
                13,
                with_alpha(filter_color(filter), (22.0 + 14.0 * shimmer) as u8),
            );
        }
    }
    for player in scene.players {
        let filter = scene.far.filter_of(player.player_id);
        if filter == LampFilter::White {
            continue;
        }
        let reach = PRISM_KEY_REACH_TILES as f32 * TILE_SIZE;
        let color = filter_color(filter);
        draw.draw_circle_v(
            Vector2::new(player.x, player.y),
            reach,
            with_alpha(color, 26),
        );
        draw.draw_circle_v(
            Vector2::new(player.x, player.y),
            reach * 0.55,
            with_alpha(color, 24),
        );
        draw.draw_circle_lines(
            player.x as i32,
            player.y as i32,
            reach,
            with_alpha(color, 90),
        );
    }
}

// --- The Rewrite Hall ------------------------------------------------------------------------

/// The colour a rule writes in (the hall's glyphs and its warning).
#[must_use]
pub const fn rule_color(rule: GlitchRule) -> Color {
    match rule {
        GlitchRule::Baseline => MINT,
        GlitchRule::GravityUp => PINK,
        GlitchRule::Mirrored => SKY_LIGHT,
        GlitchRule::TimeWarp => GOLD,
        GlitchRule::Sideways => LIME,
    }
}

/// The Rewrite Hall's glyphs, the rewrite's flicker and The Glitch's blink ghost.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_rewrite_hall(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    let Some(arena) = scene.arena else {
        return;
    };
    let frame = crate::view_frame::frame();
    let (left, top) = (
        frame.rel_col(arena.min.x) * TILE_SIZE,
        frame.rel_row(arena.min.y) * TILE_SIZE,
    );
    let rect = Rectangle::new(
        left,
        top,
        arena.width() as f32 * TILE_SIZE,
        arena.height() as f32 * TILE_SIZE,
    );
    if rect.x > scene.visible.x + scene.visible.width
        || rect.x + rect.width < scene.visible.x
        || rect.y > scene.visible.y + scene.visible.height
        || rect.y + rect.height < scene.visible.y
    {
        return;
    }
    let glitch = &scene.far.glitch;
    if glitch.beaten {
        return;
    }
    let (rule, writing) = match glitch.rewriting {
        Some((next, _)) => (next, true),
        None => (glitch.rule, false),
    };
    let color = rule_color(rule);
    // The glyphs in its walls light with the rule (they flicker while it is rewritten).
    let flicker = if writing {
        0.55 + 0.45 * ((scene.seconds * 3.0).sin() * scene.effects)
    } else {
        1.0
    };
    for y in arena.min.y..=arena.max.y {
        for x in arena.min.x..=arena.max.x {
            let tile = TilePosition { x, y };
            if scene.terrain.tile(tile).map(|found| found.kind)
                != Some(TileKind::Cave(CaveTile::RuinGlyph))
            {
                continue;
            }
            let (cx, cy) = frame.tile_center(tile);
            disc(draw, cx, cy, 9, with_alpha(color, (60.0 * flicker) as u8));
        }
    }
    if writing {
        // A slow, low-contrast tint over the hall (no strobing: under two swings a second).
        let wash = (0.5 + 0.5 * (scene.seconds * 2.2).sin()) * scene.effects;
        draw.draw_rectangle_rec(rect, with_alpha(color, (22.0 * wash) as u8));
        // Torn scanlines across the hall.
        for band in 0..6 {
            let phase = (scene.seconds * 0.8 + band as f32 * 0.17).fract();
            let y = rect.y + phase * rect.height;
            draw.draw_rectangle_rec(
                Rectangle::new(rect.x, y, rect.width, 3.0),
                with_alpha(color, (40.0 * scene.effects.max(0.3)) as u8),
            );
        }
    }
    // The blink ghost: where The Glitch is about to be.
    if let Some((tx, ty)) = glitch.blink_to {
        let (gx, gy) = frame.tile_center(TilePosition { x: tx, y: ty });
        let pulse = 0.6 + 0.4 * (scene.seconds * 6.0).sin().abs();
        draw.draw_rectangle_lines_ex(
            Rectangle::new(gx - 40.0, gy - 40.0, 80.0, 80.0),
            2.0,
            with_alpha(color, (170.0 * pulse) as u8),
        );
        draw.draw_rectangle_rec(
            Rectangle::new(gx - 36.0, gy - 36.0, 72.0, 72.0),
            with_alpha(color, (30.0 * pulse) as u8),
        );
    }
}

/// The Glitch's torn colour channels (red and blue ghosts beside its body), stronger mid-
/// rewrite; drawn behind its sprite.
#[allow(
    clippy::cast_precision_loss,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_glitch_channels(draw: &mut impl RaylibDraw, scene: &FarScene<'_>) {
    let Some(id) = scene.far.glitch.entity else {
        return;
    };
    let Some(entity) = scene.enemies.iter().find(|entity| entity.id.get() == id) else {
        return;
    };
    if !matches!(entity.state, EntityState::Enemy(_)) {
        return;
    }
    let spread = if scene.far.glitch.rewriting() {
        9.0
    } else {
        4.0
    } * scene.effects.max(0.25);
    let (x, y) = (entity.body.x, entity.body.y);
    let wobble = (scene.seconds * 1.7).sin() * spread * 0.4;
    draw.draw_rectangle_rec(
        Rectangle::new(x - 44.0 - spread + wobble, y - 44.0, 88.0, 88.0),
        with_alpha(RED, 30),
    );
    draw.draw_rectangle_rec(
        Rectangle::new(x - 44.0 + spread - wobble, y - 44.0, 88.0, 88.0),
        with_alpha(SKY, 30),
    );
}
