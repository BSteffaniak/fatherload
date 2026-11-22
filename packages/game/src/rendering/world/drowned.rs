//! The Drowned and Frozen in the world view (Wave 9, Descent IV).
//!
//! * **Geysers** ([`draw_drowned_world`]): a vent's throat glows; in its tell, wisps of steam
//!   curl up and the throat brightens; while it erupts, a roaring white column of steam rises
//!   up the chimney (to its ceiling or [`GEYSER_MAX_ROWS`]), billowing. Old Faithful's is wider.
//! * **Clams** gape and close on their beat, a pearl gleaming in the open shell.
//! * **Thermal vents** shimmer with warm air; **frozen figures** catch a cold glint.
//! * **Currents**: motes drift along each basin's gyre (and swirl into the Leviathan's maw in
//!   its maelstrom), so the water's flow can be read.
//! * **The Leviathan's tells**: before a charge, its eyes flare and a dashed red line runs
//!   along the charge's path; its open maw breathes a ring of bubbles; the eel-dragon crackles
//!   with sparks before it pulses.
//! * **Lights** ([`draw_drowned_lights`], over the darkness): sonar rings and the creatures
//!   they echo off, and the frost riming each rig's hull.
//!
//! Every effect is slow, low-contrast and scaled by the effects setting where it moves the
//! picture (nothing strobes).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation maps world pixels and tiles between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    cave::CaveTile,
    descent::Stratum,
    drowned::{
        DrownedState, GEYSER_MAX_ROWS, GeyserPhase, LeviathanMove, SONAR_PING_MILLIS, clam_open,
        geyser_phase, sonar_light,
    },
    enemy::EnemyKind,
    entity::{Entity, EntityState},
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::descent::drowned,
};

use crate::render_snapshot::{DrownedSites, PlayerPresentation};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, fx::disc};

/// What one view needs to draw the Drowned and Frozen.
pub(super) struct DrownedScene<'a> {
    pub terrain: &'a Terrain,
    pub drowned: &'a DrownedState,
    pub sites: &'a DrownedSites,
    pub visible: Rectangle,
    pub world_millis: u64,
    pub tick_rate_hz: u32,
    pub seconds: f32,
    pub players: &'a [PlayerPresentation],
    pub enemies: &'a [Entity],
    pub effects_strength: f32,
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

fn centre(tile: TilePosition) -> (f32, f32) {
    let frame = crate::view_frame::frame();
    (
        (frame.rel_col(tile.x) + 0.5) * TILE_SIZE,
        (frame.rel_row(tile.y) + 0.5) * TILE_SIZE,
    )
}

/// A cheap, stable hash for presentation jitter.
const fn jitter(x: i32, y: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Whether the world has the Drowned and Frozen and the view reaches Descent IV.
fn in_reach(scene: &DrownedScene<'_>) -> bool {
    let settings = scene.terrain.settings();
    if !settings.generator.has_descents() {
        return false;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let (first, last) =
        layout.descent_rows(fatherload_simulation::descent::Descent::DrownedAndFrozen);
    let (low, high) = tile_span(scene.visible);
    high.y >= first - 4 && low.y <= last + 4
}

/// Draws the Drowned and Frozen's world effects in view (see the module docs).
pub(super) fn draw_drowned_world(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    if !in_reach(scene) {
        return;
    }
    let (low, high) = tile_span(scene.visible);
    let seed = scene.terrain.settings().seed;
    for y in low.y..=high.y {
        for x in low.x..=high.x {
            let position = TilePosition { x, y };
            let Some(tile) = scene.terrain.tile(position) else {
                continue;
            };
            match tile.kind {
                TileKind::Cave(CaveTile::GeyserVent) => {
                    draw_geyser(draw, scene, position, seed);
                }
                TileKind::Cave(CaveTile::PearlClam) => {
                    let open = clam_open(seed, x, y, scene.world_millis);
                    draw_clam(draw, position, open, scene.seconds);
                }
                TileKind::Cave(CaveTile::ThermalVent) => {
                    draw_vent_shimmer(draw, position, scene.seconds, scene.effects_strength);
                }
                TileKind::Cave(CaveTile::FrozenFigure) => {
                    let (cx, cy) = centre(position);
                    let glint = (scene.seconds * 0.7 + (jitter(x, y, 3) % 100) as f32 * 0.06)
                        .sin()
                        .max(0.0);
                    disc(
                        draw,
                        cx - 4.0,
                        cy - 6.0,
                        1,
                        with_alpha(BONE, (glint * 200.0) as u8),
                    );
                }
                _ => {}
            }
        }
    }
    draw_currents(draw, scene);
    draw_city_sea(draw, scene);
}

/// Merrow City's sea round the bubble: little fish schooling past the glass, the light
/// rippling on it; and a faint electric glow from the eel-dragon's grotto.
fn draw_city_sea(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    let frame = crate::view_frame::frame();
    if let Some(city) = scene.sites.city {
        let (low, high) = tile_span(scene.visible);
        let view = fatherload_simulation::worldgen::TileRect {
            min: low,
            max: high,
        };
        if city.bounds().intersects(&view) {
            let dome = city.dome;
            let (left, _) = centre(dome.min);
            let (right, top) = centre(TilePosition {
                x: dome.max.x,
                y: dome.min.y - 3,
            });
            for fish in 0..18 {
                let lane = (jitter(fish, 1, 11) % 5) as f32;
                let speed = 18.0 + (jitter(fish, 2, 11) % 20) as f32;
                let span = (right - left).max(1.0);
                let x = left + (scene.seconds * speed + fish as f32 * 97.0).rem_euclid(span);
                let y = top - lane * TILE_SIZE * 0.6 + (scene.seconds + fish as f32).sin() * 3.0;
                let tile = frame.tile_at(x, y);
                if scene.terrain.fluid_at(tile).is_none() {
                    continue;
                }
                let color = [GOLD, MINT, PINK, SKY_LIGHT][(fish % 4) as usize];
                draw.draw_rectangle_rec(Rectangle::new(x, y, ART * 3.0, ART * 1.5), color);
                draw.draw_rectangle_rec(
                    Rectangle::new(x - ART, y, ART, ART * 1.5),
                    with_alpha(color, 160),
                );
            }
        }
    }
    if let Some(grotto) = scene.sites.grotto
        && !scene.drowned.eel.beaten
    {
        let (cx, cy) = centre(grotto.center());
        let pulse = (scene.seconds * 0.8).sin().mul_add(15.0, 30.0) as u8;
        disc(draw, cx, cy, 14, with_alpha(MINT, pulse / 2));
    }
}

/// A geyser vent: its glowing throat, its tell and its eruption.
fn draw_geyser(
    draw: &mut impl RaylibDraw,
    scene: &DrownedScene<'_>,
    vent: TilePosition,
    seed: u64,
) {
    let great = scene
        .sites
        .old_faithful
        .is_some_and(|rect| rect.contains(vent));
    let phase = geyser_phase(seed, vent.x, great, scene.world_millis);
    let (cx, cy) = centre(vent);
    let top = cy - TILE_SIZE * 0.5;
    let throat = match phase {
        GeyserPhase::Quiet => 40,
        GeyserPhase::Gurgling => 110,
        GeyserPhase::Erupting => 160,
    };
    disc(draw, cx, top + 2.0, 3, with_alpha(ORANGE, throat));
    // How far the chimney runs up from the vent.
    let mut rows = 0;
    while rows < GEYSER_MAX_ROWS {
        let above = TilePosition {
            x: vent.x,
            y: vent.y - rows - 1,
        };
        if !scene
            .terrain
            .tile(above)
            .is_some_and(|tile| tile.kind.is_open())
        {
            break;
        }
        rows += 1;
    }
    let height = rows as f32 * TILE_SIZE;
    match phase {
        GeyserPhase::Quiet => {}
        GeyserPhase::Gurgling => {
            // Wisps curling up out of the throat: the tell.
            for wisp in 0..5 {
                let rise = (scene.seconds * 1.6 + wisp as f32 * 0.37).fract();
                let sway = (scene.seconds * 2.0 + wisp as f32).sin() * 6.0;
                let alpha = ((1.0 - rise) * 150.0) as u8;
                disc(
                    draw,
                    cx + sway,
                    top - rise * TILE_SIZE * 2.5,
                    2 + (wisp % 2),
                    with_alpha(MIST, alpha),
                );
            }
        }
        GeyserPhase::Erupting => {
            // The column: a white core, billows along its sides, spray at its head.
            let half = if great {
                TILE_SIZE * 1.1
            } else {
                TILE_SIZE * 0.6
            };
            draw.draw_rectangle_rec(
                Rectangle::new(cx - half * 0.5, top - height, half, height),
                with_alpha(BONE, 150),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(cx - half, top - height, half * 2.0, height),
                with_alpha(MIST, 70),
            );
            let puffs = (rows * 2).max(2);
            for puff in 0..puffs {
                let along = (puff as f32 / puffs as f32 + scene.seconds * 1.5).fract();
                let side = if puff % 2 == 0 { -1.0 } else { 1.0 };
                let swell = (scene.seconds * 4.0 + puff as f32).sin() * 3.0;
                disc(
                    draw,
                    cx + side * (half + swell),
                    top - along * height,
                    3 + (puff % 3),
                    with_alpha(MIST, 110),
                );
            }
            for drop in 0..8 {
                let spread = (drop as f32 * 2.399).sin() * half * 2.0;
                let fall = (scene.seconds * 2.2 + drop as f32 * 0.21).fract();
                disc(
                    draw,
                    cx + spread,
                    top - height + fall * TILE_SIZE * 1.5,
                    1,
                    with_alpha(SKY_LIGHT, 180),
                );
            }
        }
    }
}

/// A clam: closed, a dark line where its lips meet; open, its pink flesh and a gleaming pearl.
fn draw_clam(draw: &mut impl RaylibDraw, position: TilePosition, open: bool, seconds: f32) {
    let (cx, cy) = centre(position);
    let lip = cy + 4.0;
    if open {
        draw.draw_rectangle_rec(
            Rectangle::new(cx - 10.0, lip - 8.0, 20.0, 6.0),
            with_alpha(PINK, 220),
        );
        draw.draw_rectangle_rec(Rectangle::new(cx - 12.0, lip - 12.0, 24.0, 3.0), MIST);
        let gleam = (seconds * 2.0).sin().mul_add(30.0, 200.0) as u8;
        disc(draw, cx, lip - 5.0, 2, with_alpha(BONE, gleam));
        disc(draw, cx - 1.0, lip - 6.0, 0, BONE);
    } else {
        draw.draw_rectangle_rec(Rectangle::new(cx - 10.0, lip - 1.0, 20.0, 2.0), INK);
    }
}

/// Warm air shimmering up from a thermal vent, and its glow.
fn draw_vent_shimmer(
    draw: &mut impl RaylibDraw,
    position: TilePosition,
    seconds: f32,
    strength: f32,
) {
    let (cx, cy) = centre(position);
    disc(draw, cx, cy - 4.0, 6, with_alpha(AMBER, 40));
    for ripple in 0..4 {
        let rise = (seconds * 0.8 + ripple as f32 * 0.25).fract();
        let sway = (seconds * 3.0 + ripple as f32 * 1.7).sin() * 4.0 * strength;
        disc(
            draw,
            cx + sway,
            cy - TILE_SIZE * 0.5 - rise * TILE_SIZE * 2.0,
            1,
            with_alpha(GOLD, ((1.0 - rise) * 90.0) as u8),
        );
    }
}

/// Motes drifting along the basins' gyres and into the Leviathan's maw.
fn draw_currents(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    let settings = scene.terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let (low, high) = tile_span(scene.visible);
    let area = fatherload_simulation::worldgen::TileRect {
        min: low,
        max: high,
    };
    let frame = crate::view_frame::frame();
    for basin in drowned::basins_in(settings, &layout, &area) {
        let in_ocean = scene.terrain.stratum_at(basin.bounds.center()) == Stratum::UndergroundOcean;
        if !in_ocean {
            continue;
        }
        let (cx, cy) = centre(basin.bounds.center());
        let rx = basin.bounds.width() as f32 * TILE_SIZE * 0.5;
        let ry = basin.bounds.height() as f32 * TILE_SIZE * 0.5;
        let speed = 0.05 + f32::from(basin.along) / 1_000.0 * 0.12;
        let direction = if basin.clockwise { 1.0 } else { -1.0 };
        for mote in 0..36 {
            let ring = 0.35 + (jitter(mote, basin.bounds.min.x, 7) % 50) as f32 / 100.0;
            let angle = (mote as f32 * 0.6213 + scene.seconds * speed * direction / ring)
                * std::f32::consts::TAU
                / 3.7;
            let (x, y) = (
                cx + angle.cos() * rx * ring * 0.9,
                cy + angle.sin() * ry * ring * 0.9,
            );
            let tile = frame.tile_at(x, y);
            if scene.terrain.fluid_at(tile).is_none() {
                continue;
            }
            disc(draw, x, y, 0, with_alpha(MINT, 120));
        }
    }
    // The maelstrom: motes spiralling into the maw.
    if let (Some(arena), LeviathanMove::Maw) = (scene.sites.arena, scene.drowned.leviathan.action)
        && scene.drowned.leviathan.awake()
    {
        let (ax, ay) = centre(arena.center());
        let floor = drowned::arena_floor_row(&arena);
        let maw = (
            ax,
            ay + (floor - arena.center().y) as f32 * TILE_SIZE - TILE_SIZE * 1.6,
        );
        for mote in 0..60 {
            let t = (mote as f32 / 60.0 + scene.seconds * 0.35).fract();
            let radius = (1.0 - t) * TILE_SIZE * 14.0;
            let angle = mote as f32 * 2.399 + t * 9.0;
            disc(
                draw,
                maw.0 + angle.cos() * radius,
                maw.1 + angle.sin() * radius * 0.5,
                (1.0 - t).mul_add(1.5, 0.0) as i32,
                with_alpha(SKY_LIGHT, (t * 180.0) as u8 + 40),
            );
        }
    }
}

/// The Leviathan's charge telegraph: flaring eyes and the charge's path dashed in red.
fn draw_leviathan_tells(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    let lev = &scene.drowned.leviathan;
    let (Some(arena), Some(entity)) = (scene.sites.arena, lev.entity) else {
        return;
    };
    let Some(body) = scene
        .enemies
        .iter()
        .find(|enemy| enemy.id.get() == entity)
        .map(|enemy| enemy.body)
    else {
        return;
    };
    let (ax, ay) = centre(arena.center());
    if let LeviathanMove::ChargeTell { target, .. } = lev.action {
        let (tx, ty) = (ax + target.0 as f32, ay + target.1 as f32);
        let (dx, dy) = (tx - body.x, ty - body.y);
        let length = dx.hypot(dy).max(1.0);
        let steps = (length / 18.0) as i32 + 6;
        let pulse = (scene.seconds * 6.0).sin().mul_add(25.0, 225.0) as u8;
        for step in 0..steps {
            if step % 2 == 1 {
                continue;
            }
            let t = step as f32 * 18.0 / length;
            disc(
                draw,
                body.x + dx * t,
                body.y + dy * t,
                3,
                with_alpha(ORANGE, pulse),
            );
        }
        // Its eyes flare.
        let facing = if tx < body.x { -1.0 } else { 1.0 };
        disc(
            draw,
            body.x + facing * 70.0,
            body.y - 18.0,
            6,
            with_alpha(GOLD, 200),
        );
        disc(
            draw,
            body.x + facing * 70.0,
            body.y - 18.0,
            10,
            with_alpha(AMBER, 70),
        );
    }
    if lev.maw_open() {
        // Bubbles breathing out of the open maw.
        let facing = if body.velocity_x < 0.0 { -1.0 } else { 1.0 };
        for bubble in 0..10 {
            let rise = (scene.seconds * 0.9 + bubble as f32 * 0.1).fract();
            disc(
                draw,
                body.x + facing * 90.0 + (bubble as f32 * 1.7).sin() * 10.0,
                body.y - rise * TILE_SIZE * 4.0,
                1 + (bubble % 2),
                with_alpha(BONE, ((1.0 - rise) * 160.0) as u8),
            );
        }
    }
}

/// The eel-dragon's crackle before its electric pulse.
fn draw_eel_tell(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    let eel = &scene.drowned.eel;
    let (Some(entity), Some(_)) = (eel.entity, eel.pulse_at) else {
        return;
    };
    let Some(body) = scene
        .enemies
        .iter()
        .find(|enemy| enemy.id.get() == entity)
        .map(|enemy| enemy.body)
    else {
        return;
    };
    let reach = fatherload_simulation::drowned::EEL_PULSE_TILES * TILE_SIZE;
    for spark in 0..14 {
        let angle = spark as f32 * 0.449 + scene.seconds * 3.0;
        let radius = reach * (0.3 + ((scene.seconds * 2.0 + spark as f32).sin() * 0.5 + 0.5) * 0.7);
        disc(
            draw,
            body.x + angle.cos() * radius,
            body.y + angle.sin() * radius,
            1,
            with_alpha(MINT, 170),
        );
    }
    // The pulse's reach, faintly ringed.
    let steps = 48;
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        draw.draw_rectangle_rec(
            Rectangle::new(
                body.x + angle.cos() * reach,
                body.y + angle.sin() * reach,
                ART,
                ART,
            ),
            with_alpha(MINT, 90),
        );
    }
}

/// Sonar rings and their echoes, and frost on the rigs, drawn over the darkness.
#[allow(
    clippy::too_many_lines,
    reason = "one block per light: the bosses' tells, creature lights, sonar and frost"
)]
pub(super) fn draw_drowned_lights(draw: &mut impl RaylibDraw, scene: &DrownedScene<'_>) {
    if !scene.terrain.settings().generator.has_descents() {
        return;
    }
    // The bosses' tells read through any dark.
    draw_leviathan_tells(draw, scene);
    draw_eel_tell(draw, scene);
    // Living lights of the deep: the angler's lure, the eel-dragon's crackle, the
    // Leviathan's eyes.
    for enemy in scene.enemies {
        let EntityState::Enemy(state) = &enemy.state else {
            continue;
        };
        let (dx, dy, color) = match state.kind {
            EnemyKind::LureAngler => (14.0, -24.0, GOLD),
            EnemyKind::EelDragon => (30.0, -4.0, MINT),
            EnemyKind::Leviathan => (68.0, -16.0, AMBER),
            _ => continue,
        };
        let facing = f32::from(state.facing.signum());
        let pulse = (scene.seconds * 2.0).sin().mul_add(12.0, 50.0) as u8;
        disc(
            draw,
            enemy.body.x + dx * facing,
            enemy.body.y + dy,
            9,
            with_alpha(color, pulse),
        );
    }
    let frame = crate::view_frame::frame();
    for ping in &scene.drowned.pings {
        let ping_millis =
            ping.tick.get().saturating_mul(1_000) / u64::from(scene.tick_rate_hz.max(1));
        let elapsed = scene.world_millis.saturating_sub(ping_millis);
        if elapsed >= SONAR_PING_MILLIS {
            continue;
        }
        let (px, py) = centre(TilePosition {
            x: ping.x,
            y: ping.y,
        });
        let seconds = elapsed as f32 / 1_000.0;
        let ring = (seconds * fatherload_simulation::drowned::SONAR_RING_TILES_PER_SECOND)
            .min(ping.reach as f32)
            * TILE_SIZE;
        let fade = 1.0 - elapsed as f32 / SONAR_PING_MILLIS as f32;
        // A soft wake inside the front, then the bright front itself.
        draw.draw_ring(
            Vector2::new(px, py),
            (ring - TILE_SIZE * 1.5).max(0.0),
            ring,
            0.0,
            360.0,
            96,
            with_alpha(MINT, (fade * 50.0) as u8),
        );
        draw.draw_ring(
            Vector2::new(px, py),
            (ring - ART * 2.0).max(0.0),
            ring,
            0.0,
            360.0,
            96,
            with_alpha(MINT, (fade * 210.0) as u8),
        );
        // The ring breaks on rock: the walls it touches light up.
        let steps = 120;
        for step in 0..steps {
            let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
            let (x, y) = (px + angle.cos() * ring, py + angle.sin() * ring);
            if scene.terrain.is_solid_at(frame.tile_at(x, y)) {
                draw.draw_rectangle_rec(
                    Rectangle::new(x - ART, y - ART, ART * 2.0, ART * 2.0),
                    with_alpha(ICE, (fade * 230.0) as u8),
                );
            }
        }
        // The creatures it echoes off show as bright outlines.
        for enemy in scene.enemies {
            let EntityState::Enemy(state) = &enemy.state else {
                continue;
            };
            let distance = (enemy.body.x - px).hypot(enemy.body.y - py) / TILE_SIZE;
            let light = sonar_light(elapsed, distance, ping.reach);
            if light <= 0.0 {
                continue;
            }
            let (hw, hh) = state.kind.contact_half();
            let color = if state.kind == EnemyKind::Leviathan {
                GOLD
            } else {
                MINT
            };
            draw.draw_rectangle_lines_ex(
                Rectangle::new(enemy.body.x - hw, enemy.body.y - hh, hw * 2.0, hh * 2.0),
                ART,
                with_alpha(color, (light.mul_add(155.0, 100.0)) as u8),
            );
        }
    }
    // Frost riming each rig: pale crystals creeping in from its edges.
    for player in scene.players {
        let frost = f32::from(player.water.frost_permille()) / 1_000.0;
        if frost <= 0.05 {
            continue;
        }
        let crystals = (frost * 14.0) as i32;
        for crystal in 0..crystals {
            let angle = crystal as f32 * 2.399;
            let r = 11.0 + (crystal % 3) as f32;
            disc(
                draw,
                player.x + angle.cos() * r,
                player.y + angle.sin() * r * 0.8,
                0,
                with_alpha(ICE, (120.0 + frost * 120.0) as u8),
            );
        }
    }
}
