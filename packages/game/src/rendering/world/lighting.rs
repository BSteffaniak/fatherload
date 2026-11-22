//! Surface light map (Part II F4): the open world darkens at night (and takes the warm wash of
//! golden hour and the plum of dusk), and lights cut the dark back.
//!
//! **How.** A low-resolution buffer of 8-pixel cells covers the view. Each cell's overlay is
//! the time of day's wash ([`DayLook::overlay`]) times a mask that follows the local ground:
//! full over the surface world, fading out a few tiles into the rock (the underground stays as
//! dark as it always is; caves keep their own darkness, `world/caves.rs`) and fading out high in
//! the sky (the sky draws its own night). Lights subtract from the darkness: rig headlights (a
//! forward beam and a pool round the rig, bigger with the scanner tier: they matter at night;
//! a rig whose pilot is in bed has its lamps off), village windows and street lamps, the town's
//! windows, base lamps and lit modules, flares and blasts, lava, magma and glowing cave growths
//! and critters. A second pass lays a faint warm haze round warm lights. A light out in the open lights the skin of the ground, not the rock
//! behind it, and a headlight beam only faintly lights the open air it crosses. Cells are
//! posterized into a few levels (stepped rings of light, like the rest of the pixel art) and
//! drawn as merged runs: a few hundred quads per view, and nothing at all by day, which keeps
//! the browser build inside its budget. Presentation only: gameplay light comes from
//! `WorldState::light_at`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation maps world pixels, tiles and light cells between small integers and floats"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    ExplosiveKind, PlacedBomb, PlayerId,
    base::{Bases, Fixture, ModuleKind, StructureId},
    clock::Sleeper,
    explosive::FLARE_LIGHT_RADIUS,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::{
    effects::Explosion,
    render_snapshot::{CritterPresentation, PlayerPresentation},
};

use super::{
    RigFacings,
    caves::{RIG_LAMP_TILES, RIG_LAMP_TILES_PER_SCANNER, critter_position, tile_light},
    daylight::{DayLook, Wash},
};

/// World pixels per light cell (a quarter tile).
const CELL: f32 = 8.0;
/// Overlay levels the darkness is posterized to.
const LEVELS: i32 = 12;
/// Warm haze levels.
const WARM_LEVELS: i32 = 5;
/// Share of a headlight beam the open air it crosses catches (the beam falls on surfaces).
const BEAM_AIR: f32 = 0.42;
/// Tiles under the ground a light from above it still reaches (it lights the surface skin,
/// not the rock behind it), and where it has gone.
const SKIN_FULL_TILES: f32 = 1.0;
const SKIN_ZERO_TILES: f32 = 2.5;
/// Tiles above the local ground the overlay stays full, and where it has faded out.
const ABOVE_FULL_TILES: f32 = 5.0;
const ABOVE_ZERO_TILES: f32 = 12.0;
/// Tiles below the local ground the overlay stays full, and where it has faded out.
const BELOW_FULL_TILES: f32 = 4.0;
const BELOW_ZERO_TILES: f32 = 14.0;
/// Radius of a pilot's helmet lamp pool, in tiles (its beam reaches 1.8 times as far).
const PILOT_LAMP_TILES: f32 = 2.6;
/// How far ahead a rig's headlight beam reaches, in rig lamp radii.
const BEAM_REACH: f32 = 2.1;
/// Cosine of the beam's half-angle.
const BEAM_SPREAD: f32 = 0.78;

/// One light: centre (world pixels), radius (pixels), strength (0..1), an optional beam
/// direction (unit vector), whether it throws a warm haze, and how much of it the open air
/// above the ground catches.
#[derive(Clone, Copy, Debug)]
struct Light {
    x: f32,
    y: f32,
    radius: f32,
    strength: f32,
    beam: Option<(f32, f32)>,
    warm: bool,
    air: f32,
}

impl Light {
    const fn pool(x: f32, y: f32, radius: f32, strength: f32, warm: bool) -> Self {
        Self {
            x,
            y,
            radius,
            strength,
            beam: None,
            warm,
            air: 1.0,
        }
    }
}

/// What one view's light pass needs.
pub(super) struct LightScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub look: &'a DayLook,
    pub players: &'a [PlayerPresentation],
    /// Pilots on foot (Part II F5): each carries a helmet lamp.
    pub pilots: &'a [crate::render_snapshot::PilotPresentation],
    pub facings: &'a RigFacings,
    pub bombs: &'a [PlacedBomb],
    pub explosions: &'a [Explosion],
    pub critters: &'a [CritterPresentation],
    pub bases: &'a Bases,
    pub powered: &'a std::collections::BTreeSet<StructureId>,
    /// Lights of the town and the villages in view (`world/buildings.rs`, `world/villages.rs`):
    /// `(x, y, radius)` in world pixels.
    pub settlement_lights: &'a [(f32, f32, f32)],
    /// Players in bed: their rigs' lamps are off for the night.
    pub asleep: &'a std::collections::BTreeMap<PlayerId, Sleeper>,
    /// Whether it is night (WP-NIGHT: shallow nocturnal ore glows).
    pub night: bool,
    /// Enemies (WP-NIGHT: will-o'-wisps are lights).
    pub enemies: &'a [fatherload_simulation::entity::Entity],
    pub seconds: f32,
}

/// The overlay mask at `depth` tiles under the local ground (negative: above it).
fn ground_mask(depth: f32) -> f32 {
    if depth >= 0.0 {
        1.0 - ((depth - BELOW_FULL_TILES) / (BELOW_ZERO_TILES - BELOW_FULL_TILES)).clamp(0.0, 1.0)
    } else {
        1.0 - ((-depth - ABOVE_FULL_TILES) / (ABOVE_ZERO_TILES - ABOVE_FULL_TILES)).clamp(0.0, 1.0)
    }
}

/// Draws the view's light map (see the module docs). Nothing to do by day.
#[allow(
    clippy::too_many_lines,
    reason = "mask, lights, quantizing and the run pass read best in one place"
)]
pub(super) fn draw_light_map(draw: &mut impl RaylibDraw, scene: &LightScene<'_>) {
    let look = scene.look;
    if !look.has_overlay() {
        return;
    }
    let visible = scene.visible;
    let first_x = (visible.x / CELL).floor() as i32;
    let first_y = (visible.y / CELL).floor() as i32;
    let cols = (visible.width / CELL).ceil() as i32 + 2;
    let rows = (visible.height / CELL).ceil() as i32 + 2;
    // The local ground per tile column, smoothed over its neighbours so cliffs don't stripe.
    let first_tile = crate::view_frame::frame().col(first_x as f32 * CELL) - 2;
    let last_tile = crate::view_frame::frame().col_ceil((first_x + cols) as f32 * CELL) + 2;
    // Ground rows relative to the presented frame (F7), indexed by absolute column.
    let frame = crate::view_frame::frame();
    let origin_col = frame.col(0.0);
    let surface: Vec<f32> = (first_tile..=last_tile)
        .map(|x| frame.rel_row(scene.terrain.surface_row(x)))
        .collect();
    let ground_at = |world_x: f32| -> f32 {
        let fx = world_x / TILE_SIZE - 0.5;
        let x0 = fx.floor() as i32 + origin_col;
        let t = fx - (x0 - origin_col) as f32;
        let at = |x: i32| {
            let index = (x - first_tile).clamp(0, surface.len() as i32 - 1) as usize;
            surface[index]
        };
        (at(x0 + 1) - at(x0)).mul_add(t, at(x0))
    };
    let mut mask = vec![0.0_f32; (cols * rows) as usize];
    let mut any = false;
    let mut column_ground = Vec::with_capacity(cols as usize);
    for col in 0..cols {
        let cx = (first_x + col) as f32 * CELL + CELL * 0.5;
        let ground = ground_at(cx);
        column_ground.push(ground);
        for row in 0..rows {
            let cy = (first_y + row) as f32 * CELL + CELL * 0.5;
            let value = ground_mask(cy / TILE_SIZE - ground);
            any |= value > 0.0;
            mask[(row * cols + col) as usize] = value;
        }
    }
    if !any {
        return;
    }
    // Lights cut the darkness back (only once it is dark enough to need them).
    let lights = if look.darkness > 0.05 {
        gather_lights(scene)
    } else {
        Vec::new()
    };
    let mut lit = vec![0.0_f32; mask.len()];
    let mut warm = vec![0.0_f32; mask.len()];
    for light in &lights {
        let min_col = (((light.x - light.radius) / CELL).floor() as i32 - first_x).max(0);
        let max_col = (((light.x + light.radius) / CELL).ceil() as i32 - first_x).min(cols - 1);
        let min_row = (((light.y - light.radius) / CELL).floor() as i32 - first_y).max(0);
        let max_row = (((light.y + light.radius) / CELL).ceil() as i32 - first_y).min(rows - 1);
        let inverse = 1.0 / (light.radius * light.radius);
        // A light out in the open lights the ground's skin, not the rock behind it; one in a
        // tunnel lights its tunnel.
        let source_above = light.y / TILE_SIZE <= ground_at(light.x) + 0.5;
        for row in min_row..=max_row {
            let cell_y = (first_y + row) as f32 * CELL + CELL * 0.5;
            let dy = cell_y - light.y;
            for col in min_col..=max_col {
                let depth = cell_y / TILE_SIZE - column_ground[col as usize];
                let (reach, haze) = if depth < -0.25 {
                    (light.air, light.air)
                } else if source_above {
                    let skin = 1.0
                        - ((depth - SKIN_FULL_TILES) / (SKIN_ZERO_TILES - SKIN_FULL_TILES))
                            .clamp(0.0, 1.0);
                    (skin, skin)
                } else {
                    (1.0, 1.0)
                };
                if reach <= 0.0 {
                    continue;
                }
                let dx = (first_x + col) as f32 * CELL + CELL * 0.5 - light.x;
                let distance_sq = dx.mul_add(dx, dy * dy);
                let mut falloff = 1.0 - distance_sq * inverse;
                if falloff <= 0.0 {
                    continue;
                }
                if let Some((bx, by)) = light.beam {
                    let distance = distance_sq.sqrt().max(1.0);
                    let along = (dx * bx + dy * by) / distance;
                    let edge = ((along - BEAM_SPREAD) / (1.0 - BEAM_SPREAD)).clamp(0.0, 1.0);
                    falloff *= edge.sqrt();
                    if falloff <= 0.0 {
                        continue;
                    }
                }
                let index = (row * cols + col) as usize;
                let amount = light.strength * falloff * falloff.mul_add(-0.35, 1.35);
                lit[index] += amount * reach;
                if light.warm {
                    warm[index] += amount * 0.6 * haze;
                }
            }
        }
    }
    // The wash crossfades between two keyframes' colours: one run pass per colour.
    let cut = (look.darkness / 0.6).clamp(0.0, 1.0);
    for wash in [look.overlay.0, look.overlay.1] {
        if wash.alpha == 0 {
            continue;
        }
        draw_runs(
            draw,
            (first_x, first_y, cols, rows),
            LEVELS,
            wash,
            |index| mask[index] * (1.0 - (lit[index] * cut).min(1.0)),
        );
    }
    if !lights.is_empty() {
        let haze = Wash {
            color: GOLD,
            alpha: (cut * 60.0) as u8,
        };
        if haze.alpha > 0 {
            draw_runs(
                draw,
                (first_x, first_y, cols, rows),
                WARM_LEVELS,
                haze,
                |index| (warm[index] * mask[index]).min(1.0),
            );
        }
        draw_headlight_shafts(draw, scene, cut, &ground_at);
    }
}

/// The visible shaft of each rig's headlight in the dark: a cone of pale light widening ahead
/// of the lamp in stepped bands, fading with distance, stopped by the ground.
fn draw_headlight_shafts(
    draw: &mut impl RaylibDraw,
    scene: &LightScene<'_>,
    strength: f32,
    ground_at: &impl Fn(f32) -> f32,
) {
    const STEP: f32 = 4.0;
    const LENGTH: i32 = 34;
    if strength < 0.15 {
        return;
    }
    for player in scene.players {
        if scene.asleep.contains_key(&player.player_id) {
            continue;
        }
        let ahead = if scene.facings.facing_left(player) {
            -1.0
        } else {
            1.0
        };
        let lamp_x = player.x + ahead * TILE_SIZE * 0.45;
        let lamp_y = player.y - TILE_SIZE * 0.05;
        if lamp_x < scene.visible.x - 200.0
            || lamp_x > scene.visible.x + scene.visible.width + 200.0
        {
            continue;
        }
        let source_above = lamp_y / TILE_SIZE <= ground_at(lamp_x) + 0.5;
        for step in 0..LENGTH {
            let along = step as f32 * STEP;
            let x = lamp_x + ahead * along;
            // Centre line angled a little down, the cone opening as it goes.
            let center = lamp_y + along * 0.28;
            let half = along.mul_add(0.5, 3.0);
            let mut top = center - half;
            let mut bottom = center + half;
            if source_above {
                bottom = bottom.min(ground_at(x) * TILE_SIZE);
            }
            top = top.max(scene.visible.y);
            if bottom <= top {
                continue;
            }
            let fade = 1.0 - step as f32 / LENGTH as f32;
            // Posterized: a bright core band and a dimmer outer band.
            let alpha = (fade * strength * 34.0).round() as u8;
            if alpha == 0 {
                continue;
            }
            let left = if ahead > 0.0 { x } else { x - STEP };
            draw.draw_rectangle_rec(
                Rectangle::new(left, top, STEP, bottom - top),
                with_alpha(CREAM, alpha),
            );
            let core_top = (center - half * 0.45).max(top);
            let core_bottom = (center + half * 0.45).min(bottom);
            if core_bottom > core_top {
                draw.draw_rectangle_rec(
                    Rectangle::new(left, core_top, STEP, core_bottom - core_top),
                    with_alpha(CREAM, alpha / 2 + 6),
                );
            }
        }
    }
}

/// Posterizes `value(cell)` (0..1, times `wash`'s alpha) into `levels` steps and draws merged
/// runs of equal cells.
fn draw_runs(
    draw: &mut impl RaylibDraw,
    (first_x, first_y, cols, rows): (i32, i32, i32, i32),
    levels: i32,
    wash: Wash,
    value: impl Fn(usize) -> f32,
) {
    let top = (levels - 1) as f32;
    for row in 0..rows {
        let y = (first_y + row) as f32 * CELL;
        let mut run: Option<(i32, u8)> = None;
        for col in 0..=cols {
            let level = if col < cols {
                (value((row * cols + col) as usize) * top)
                    .round()
                    .clamp(0.0, top) as u8
            } else {
                0
            };
            match run {
                Some((_, current)) if current == level && col < cols => {}
                Some((start, current)) => {
                    if current > 0 {
                        let alpha = f32::from(wash.alpha) * f32::from(current) / top;
                        draw.draw_rectangle_rec(
                            Rectangle::new(
                                (first_x + start) as f32 * CELL,
                                y,
                                (col - start) as f32 * CELL,
                                CELL,
                            ),
                            with_alpha(wash.color, alpha.round() as u8),
                        );
                    }
                    run = Some((col, level));
                }
                None => run = Some((col, level)),
            }
        }
    }
}

/// Every light in or near the view.
#[allow(
    clippy::too_many_lines,
    reason = "one pass per kind of light source, in one place"
)]
fn gather_lights(scene: &LightScene<'_>) -> Vec<Light> {
    let visible = scene.visible;
    let margin = 8.0 * TILE_SIZE;
    let near = |x: f32, y: f32| {
        x > visible.x - margin
            && x < visible.x + visible.width + margin
            && y > visible.y - margin
            && y < visible.y + visible.height + margin
    };
    let mut lights = Vec::new();
    for player in scene.players {
        if !near(player.x, player.y) || scene.asleep.contains_key(&player.player_id) {
            continue;
        }
        let lamp = (f32::from(player.scanner_level) * RIG_LAMP_TILES_PER_SCANNER + RIG_LAMP_TILES)
            * TILE_SIZE;
        let ahead = if scene.facings.facing_left(player) {
            -1.0
        } else {
            1.0
        };
        // A pool of light round the rig, and the headlight beam ahead, angled a little down.
        lights.push(Light::pool(player.x, player.y, lamp * 0.48, 0.95, false));
        let (bx, by) = (ahead * 0.96, 0.28);
        lights.push(Light {
            x: player.x + ahead * TILE_SIZE * 0.4,
            y: player.y,
            radius: lamp * BEAM_REACH,
            strength: 1.15,
            beam: Some((bx, by)),
            warm: true,
            air: BEAM_AIR,
        });
    }
    // Helmet lamps: a small pool round the pilot's head and a short beam where it faces.
    for pilot in scene.pilots {
        let head_y = pilot.y - TILE_SIZE * 1.3;
        if !near(pilot.x, head_y) || scene.asleep.contains_key(&pilot.player_id) {
            continue;
        }
        let ahead = if pilot.state.facing < 0 { -1.0 } else { 1.0 };
        // A bought headlamp (WP-PILOT) throws a wider pool and a longer beam.
        let lamp = if pilot.state.gear.headlamp == 0 {
            PILOT_LAMP_TILES
        } else {
            pilot.state.gear.headlamp_radius_tiles()
        };
        lights.push(Light::pool(pilot.x, head_y, lamp * TILE_SIZE, 0.85, false));
        lights.push(Light {
            x: pilot.x + ahead * TILE_SIZE * 0.3,
            y: head_y,
            radius: lamp * TILE_SIZE * 1.8,
            strength: 0.9,
            beam: Some((ahead * 0.97, 0.22)),
            warm: true,
            air: BEAM_AIR,
        });
    }
    for bomb in scene.bombs {
        if bomb.kind == ExplosiveKind::Flare && bomb.burning() {
            let flicker = (scene.seconds * 23.0 + bomb.id as f32).sin() * 0.04;
            lights.push(Light::pool(
                bomb.x,
                bomb.y,
                FLARE_LIGHT_RADIUS * (1.0 + flicker),
                1.0,
                true,
            ));
        }
    }
    for blast in scene.explosions {
        let fade = (1.0 - blast.age / 0.6).clamp(0.0, 1.0);
        if fade > 0.0 {
            lights.push(Light::pool(blast.x, blast.y, TILE_SIZE * 9.0, fade, true));
        }
    }
    for &(x, y, radius) in scene.settlement_lights {
        if near(x, y) {
            let flicker = (scene.seconds * 5.0 + x * 0.01).sin() * 0.03;
            lights.push(Light::pool(x, y, radius * (1.0 + flicker), 0.9, true));
        }
    }
    // Base lamps and lit modules.
    let min = crate::view_frame::frame().tile_at(visible.x - margin, visible.y - margin);
    let max = TilePosition {
        x: crate::view_frame::frame().col_ceil(visible.x + visible.width + margin),
        y: crate::view_frame::frame().row_ceil(visible.y + visible.height + margin),
    };
    for (tile, fixtures) in scene.bases.fixtures_in(min, max) {
        if matches!(fixtures.front, Some((Fixture::Light, _))) {
            let (x, y) = tile_centre(tile);
            lights.push(Light::pool(x, y, TILE_SIZE * 6.0, 1.0, true));
        }
    }
    for module in scene.bases.modules() {
        let (w, h) = module.kind.footprint();
        let x = (crate::view_frame::frame().rel_col(module.origin.x) + w as f32 * 0.5) * TILE_SIZE;
        let y = (crate::view_frame::frame().rel_row(module.origin.y) + h as f32 * 0.5) * TILE_SIZE;
        if !near(x, y) {
            continue;
        }
        let powered = scene.powered.contains(&module.id);
        let reach = match module.kind {
            ModuleKind::Homestead | ModuleKind::Bed => 4.5,
            ModuleKind::Beacon => 2.5,
            _ if powered => 3.0,
            _ => 0.0,
        };
        if reach > 0.0 {
            lights.push(Light::pool(x, y, reach * TILE_SIZE, 0.85, true));
        }
    }
    // Glowing tiles: lava, magma vents, crystals and glowcaps.
    let first = TilePosition {
        x: crate::view_frame::frame().col(visible.x) - 1,
        y: crate::view_frame::frame().row(visible.y) - 1,
    };
    let last = TilePosition {
        x: crate::view_frame::frame().col_ceil(visible.x + visible.width) + 1,
        y: crate::view_frame::frame().row_ceil(visible.y + visible.height) + 1,
    };
    for y in first.y..=last.y {
        for x in first.x..=last.x {
            let position = TilePosition { x, y };
            let Some(tile) = scene.terrain.tile(position) else {
                continue;
            };
            let reach = tile_light(tile.kind);
            if reach > 0.0 {
                let (cx, cy) = tile_centre(position);
                lights.push(Light::pool(cx, cy, reach * TILE_SIZE, 0.8, true));
            }
            // Nocturnal ore glows near the surface at night (WP-NIGHT).
            if scene.night
                && let fatherload_simulation::terrain::TileKind::Ore(mineral) = tile.kind
                && fatherload_simulation::night::glows_at_night(mineral)
                && (0..=fatherload_simulation::night::NOCTURNAL_GLOW_DEPTH)
                    .contains(&(y - scene.terrain.surface_row(x)))
            {
                let (cx, cy) = tile_centre(position);
                let pulse = (scene.seconds * 1.7 + (x * 7 + y * 3) as f32 * 0.3).sin() * 0.12;
                lights.push(Light::pool(cx, cy, TILE_SIZE * (1.8 + pulse), 0.7, false));
            }
        }
    }
    for entity in scene.enemies {
        if let fatherload_simulation::entity::EntityState::Enemy(state) = &entity.state
            && state.kind == fatherload_simulation::EnemyKind::WillOWisp
            && near(entity.body.x, entity.body.y)
        {
            let flicker = (scene.seconds * 6.0 + entity.id.get() as f32).sin() * 0.06;
            lights.push(Light::pool(
                entity.body.x,
                entity.body.y,
                TILE_SIZE * 3.2 * (1.0 + flicker),
                0.75,
                false,
            ));
        }
    }
    for critter in scene.critters {
        let reach = critter.state.kind.light_radius();
        if reach > 0.0 {
            let (x, y) = critter_position(critter, scene.seconds);
            lights.push(Light::pool(x, y, reach * TILE_SIZE, 0.45, false));
        }
    }
    lights
}

fn tile_centre(position: TilePosition) -> (f32, f32) {
    (
        (crate::view_frame::frame().rel_col(position.x) + 0.5) * TILE_SIZE,
        (crate::view_frame::frame().rel_row(position.y) + 0.5) * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_hugs_the_ground() {
        assert!((ground_mask(0.0) - 1.0).abs() < f32::EPSILON);
        assert!((ground_mask(-ABOVE_FULL_TILES) - 1.0).abs() < f32::EPSILON);
        assert!(ground_mask(-ABOVE_ZERO_TILES) <= 0.0);
        assert!((ground_mask(BELOW_FULL_TILES) - 1.0).abs() < f32::EPSILON);
        assert!(
            ground_mask(BELOW_ZERO_TILES + 10.0) <= 0.0,
            "deep rock is never darkened"
        );
    }
}
