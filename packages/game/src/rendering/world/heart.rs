//! Descent VIII in the world (Wave 10, the Heart):
//!
//! * **Gravity wells**: each singularity drawn as a black disc ringed with light (its horizon),
//!   an accretion disc of pink and violet motes turning slowly round it, and faint infall
//!   streaks; the screen's lensing (the post-process) bends the view round it too.
//! * **The Heart**: the Star Drive in its chamber, as big as the fight needs: a gold cradle of
//!   rings round a breathing core. Each phase shows its tell: the gravity pulse's ring of light
//!   contracting toward the core, then infall streaks while it pulls; the ignition's two beams
//!   fading in and turning; the collapse's dark core with its bright rim, brightening before it
//!   exhales a slow ring of light. Awake, it burns steadily and its rings turn.
//! * **The builders' cable**: a dark braided line through the open places (the chamber, the
//!   wells, the metal sea); once the Heart wakes, its light climbs from the Heart up the cable,
//!   out of the ground at the cable's column and across the sky to the Moon's underside, and
//!   stays lit: from the Heart, from the surface and from the Moon alike.
//!
//! Nothing flashes: every change is a fade or a steady movement.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "presentation maths on bounded values"
)]

use fatherload_simulation::{
    heart::{
        BEAM_HALF_WIDTH_TILES, BreathState, CHAMBER_RADIUS, Heart, HeartPhase, PULSE_TELL_MILLIS,
        PulseState, breath_at, ending::beam_climb, pulse_at,
    },
    terrain::{TILE_SIZE, Terrain, TilePosition},
    world::CablePath,
    worldgen::{TileRect, descent::heart::Well},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::fx::disc;

/// What the Heart's pass draws from.
pub(super) struct HeartScene<'a> {
    pub terrain: &'a Terrain,
    pub heart: &'a Heart,
    pub visible: Rectangle,
    /// World milliseconds of the drawn moment (the fight's rhythms and the cable's climb).
    pub world_millis: u64,
    pub seconds: f32,
    pub effects_strength: f32,
    pub wells: &'a [Well],
    pub centre: Option<TilePosition>,
    pub cable: Option<CablePath>,
    pub cable_rows: Option<(i32, i32)>,
}

fn near(scene: &HeartScene<'_>, x: f32, y: f32, reach: f32) -> bool {
    x > scene.visible.x - reach
        && x < scene.visible.x + scene.visible.width + reach
        && y > scene.visible.y - reach
        && y < scene.visible.y + scene.visible.height + reach
}

/// The wells' horizons and accretion discs.
pub(super) fn draw_wells(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    let frame = crate::view_frame::frame();
    for well in scene.wells {
        let (cx, cy) = frame.tile_center(well.centre);
        let reach = well.reach() as f32 * TILE_SIZE;
        if !near(scene, cx, cy, reach) {
            continue;
        }
        let radius = well.radius as f32 * TILE_SIZE;
        // A faint glow of infalling light filling the cavern.
        disc(draw, cx, cy, (radius * 0.9) as i32, with_alpha(PLUM, 26));
        disc(draw, cx, cy, (radius * 0.55) as i32, with_alpha(VIOLET, 30));
        // The accretion disc: motes on tilted ellipses, turning slowly.
        let turn = scene.seconds * 0.35;
        for index in 0..48 {
            let lane = (index % 4) as f32;
            let r = radius * (0.32 + lane * 0.1);
            let a = turn * (1.0 + lane * 0.25) + index as f32 * 0.7;
            let (s, c) = a.sin_cos();
            let (x, y) = (cx + c * r, cy + s * r * 0.36);
            let colour = if index % 3 == 0 {
                PINK
            } else if index % 3 == 1 {
                ROSE
            } else {
                LAVENDER
            };
            let size = if index % 5 == 0 { 3 } else { 2 };
            disc(draw, x, y, size, with_alpha(colour, 200));
        }
        // The horizon: black, ringed with a thin bright edge, a soft halo outside.
        // (The lensing pass magnifies it into the ring's shadow, so it is drawn small.)
        let horizon = fatherload_simulation::heart::HORIZON_TILES * TILE_SIZE * 0.4;
        disc(draw, cx, cy, (horizon * 2.4) as i32, with_alpha(PINK, 50));
        disc(draw, cx, cy, (horizon + 3.0) as i32, with_alpha(BONE, 220));
        disc(draw, cx, cy, horizon as i32, INK);
    }
}

/// The builders' dark cable line through the open places of the deep.
pub(super) fn draw_cable(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    let Some(path) = scene.cable else {
        return;
    };
    let frame = crate::view_frame::frame();
    let column_x = frame
        .tile_center(TilePosition {
            x: path.column,
            y: 0,
        })
        .0;
    let heart_y = frame.tile_center(path.heart).1;
    // The dark cable line in the deep (behind the light): from the Heart up to Heart's Edge.
    if let Some((top, _)) = scene.cable_rows {
        let top_y = frame
            .tile_center(TilePosition {
                x: path.column,
                y: top,
            })
            .1;
        if column_x > scene.visible.x - 20.0
            && column_x < scene.visible.x + scene.visible.width + 20.0
        {
            let y0 = top_y.max(scene.visible.y - 10.0);
            let y1 = heart_y.min(scene.visible.y + scene.visible.height + 10.0);
            if y1 > y0 {
                draw.draw_line_ex(
                    Vector2::new(column_x, y0),
                    Vector2::new(column_x, y1),
                    8.0,
                    with_alpha(STEEL_DARK, 220),
                );
                draw.draw_line_ex(
                    Vector2::new(column_x - 1.0, y0),
                    Vector2::new(column_x - 1.0, y1),
                    2.0,
                    with_alpha(STEEL, 200),
                );
            }
        }
    }
}

/// The cable's light once the Heart woke (drawn over the dark: it is a light).
pub(super) fn draw_cable_light(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    if !scene.terrain.settings().generator.has_descents() {
        return;
    }
    let Some(path) = scene.cable else {
        return;
    };
    let frame = crate::view_frame::frame();
    let column_x = frame
        .tile_center(TilePosition {
            x: path.column,
            y: 0,
        })
        .0;
    let heart_y = frame.tile_center(path.heart).1;
    let Some(lit) = scene.heart.cable_lit_millis else {
        return;
    };
    let elapsed = scene.world_millis.saturating_sub(lit);
    let climb = beam_climb(elapsed);
    // The path: the Heart up to the ground line, then across the sky to the Moon's underside.
    let ground_y = frame
        .tile_center(TilePosition {
            x: path.column,
            y: path.surface_row,
        })
        .1;
    let heart = Vector2::new(column_x, heart_y);
    let ground = Vector2::new(column_x, ground_y);
    let moon = path.moon.map(|tile| {
        let (x, y) = frame.tile_center(tile);
        Vector2::new(x, y)
    });
    let deep = heart_y - ground_y;
    let sky = moon.map_or(0.0, |m| (m.x - ground.x).hypot(m.y - ground.y));
    let total = (deep + sky).max(1.0);
    let reach = climb * total;
    // The tip (while the light climbs) and the lit ends.
    let underground_tip = if reach < deep {
        Vector2::new(column_x, heart_y - reach)
    } else {
        ground
    };
    let glow = |draw: &mut dyn FnMut(Vector2, Vector2, f32, Color), a: Vector2, b: Vector2| {
        draw(a, b, 26.0, with_alpha(GOLD, 34));
        draw(a, b, 14.0, with_alpha(GOLD, 80));
        draw(a, b, 6.0, with_alpha(CREAM, 200));
        draw(a, b, 2.0, with_alpha(BONE, 255));
    };
    let mut line = |a: Vector2, b: Vector2, width: f32, colour: Color| {
        draw.draw_line_ex(a, b, width, colour);
    };
    glow(&mut line, heart, underground_tip);
    if reach > deep
        && let Some(moon) = moon
    {
        let f = ((reach - deep) / sky.max(1.0)).clamp(0.0, 1.0);
        let tip = Vector2::new(
            f.mul_add(moon.x - ground.x, ground.x),
            f.mul_add(moon.y - ground.y, ground.y),
        );
        glow(&mut line, ground, tip);
    }
    // The climbing tip blazes.
    if climb < 1.0 {
        let tip = if reach < deep {
            underground_tip
        } else {
            moon.map_or(ground, |m| {
                let f = ((reach - deep) / sky.max(1.0)).clamp(0.0, 1.0);
                Vector2::new(
                    f.mul_add(m.x - ground.x, ground.x),
                    f.mul_add(m.y - ground.y, ground.y),
                )
            })
        };
        disc(draw, tip.x, tip.y, 26, with_alpha(GOLD, 70));
        disc(draw, tip.x, tip.y, 12, with_alpha(CREAM, 200));
    } else if let Some(moon) = moon {
        // The Moon's underside, lit where the cable docks.
        disc(draw, moon.x, moon.y, 90, with_alpha(GOLD, 30));
        disc(draw, moon.x, moon.y, 40, with_alpha(CREAM, 60));
    }
}

/// The Star Drive in its chamber (see the module docs).
pub(super) fn draw_heart(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    let Some(centre) = scene.centre else {
        return;
    };
    let frame = crate::view_frame::frame();
    let (cx, cy) = frame.tile_center(centre);
    let chamber = CHAMBER_RADIUS as f32 * TILE_SIZE;
    if !near(scene, cx, cy, chamber + 64.0) {
        return;
    }
    let fight = &scene.heart.fight;
    let elapsed = scene.world_millis.saturating_sub(fight.phase_millis);
    let breath = (scene.seconds * 0.9).sin().mul_add(0.5, 0.5);
    let awake = fight.beaten;
    let collapsing = fight.phase.breathing();
    // The chamber's warm light.
    disc(
        draw,
        cx,
        cy,
        (chamber * 0.95) as i32,
        with_alpha(AMBER, if awake { 34 } else { 18 }),
    );
    // The cradle: gold rings round the core (turning once it is awake or fighting).
    let spin = if awake || fight.phase.fighting() {
        scene.seconds * 0.25
    } else {
        0.0
    };
    for (index, ring) in [5.2_f32, 7.0, 8.6].into_iter().enumerate() {
        let r = ring * TILE_SIZE;
        let segments = 24;
        for step in 0..segments {
            if (step + index * 3) % 4 == 3 {
                continue;
            }
            let a0 = step as f32 / segments as f32 * std::f32::consts::TAU
                + spin * (1.0 + index as f32 * 0.4);
            let a1 = a0 + std::f32::consts::TAU / segments as f32 * 0.8;
            let (s0, c0) = a0.sin_cos();
            let (s1, c1) = a1.sin_cos();
            draw.draw_line_ex(
                Vector2::new(cx + c0 * r, cy + s0 * r),
                Vector2::new(cx + c1 * r, cy + s1 * r),
                4.0,
                with_alpha(if index == 1 { GOLD } else { COPPER }, 230),
            );
        }
    }
    // The gravity pulse's tell: a ring of light contracting toward the core, then infall (the
    // reforge's too, Wave 11 carryover).
    if fight.phase.sealed() {
        match pulse_at(elapsed) {
            PulseState::Tell => {
                let t = (elapsed.saturating_sub(fatherload_simulation::heart::PULSE_FIRST_MILLIS)
                    % fatherload_simulation::heart::PULSE_PERIOD_MILLIS)
                    as f32
                    / PULSE_TELL_MILLIS as f32;
                let r = chamber * (1.0 - t * 0.75);
                let centre = Vector2::new(cx, cy);
                draw.draw_ring(
                    centre,
                    r - 10.0,
                    r + 10.0,
                    0.0,
                    360.0,
                    96,
                    with_alpha(ORANGE, 60),
                );
                draw.draw_ring(
                    centre,
                    r - 4.0,
                    r + 4.0,
                    0.0,
                    360.0,
                    96,
                    with_alpha(GOLD, 170),
                );
                draw.draw_ring(
                    centre,
                    r - 1.5,
                    r + 1.5,
                    0.0,
                    360.0,
                    96,
                    with_alpha(CREAM, 230),
                );
            }
            PulseState::Pull => {
                for index in 0..20 {
                    let a = index as f32 * 0.314 + scene.seconds * 0.4;
                    let along = (scene.seconds * 1.6 + index as f32 * 0.37).fract();
                    let r0 = chamber * (1.0 - along);
                    let r1 = r0 - TILE_SIZE * 2.0;
                    let (s, c) = a.sin_cos();
                    draw.draw_line_ex(
                        Vector2::new(cx + c * r0, cy + s * r0),
                        Vector2::new(cx + c * r1.max(0.0), cy + s * r1.max(0.0)),
                        2.0,
                        with_alpha(CREAM, (180.0 * (1.0 - along)) as u8),
                    );
                }
            }
            PulseState::Calm => {}
        }
    }
    // The reforge's inner coils (Wave 11 carryover): their orbit, and each one shielded by a
    // copper cage while the drive is calm, glowing gold (open to harm) while it pulls.
    if fight.phase == HeartPhase::Reforge {
        let orbit = fatherload_simulation::heart::INNER_ORBIT_TILES * TILE_SIZE;
        draw.draw_circle_lines(cx as i32, cy as i32, orbit, with_alpha(COPPER, 90));
        let open = pulse_at(elapsed) != PulseState::Calm;
        for (index, slot) in fight.inner.iter().enumerate() {
            if slot.is_none() {
                continue;
            }
            let angle = fatherload_simulation::heart::inner_coil_angle(index, elapsed);
            let (s, c) = angle.sin_cos();
            let (x, y) = (cx + c * orbit, cy + s * orbit);
            if open {
                disc(draw, x, y, 34, with_alpha(GOLD, 70));
                disc(draw, x, y, 24, with_alpha(CREAM, 60));
            } else {
                for spoke in 0..6 {
                    let a0 = spoke as f32 / 6.0 * std::f32::consts::TAU + scene.seconds;
                    let a1 = a0 + std::f32::consts::TAU / 6.0;
                    draw.draw_line_ex(
                        Vector2::new(x + a0.cos() * 28.0, y + a0.sin() * 28.0),
                        Vector2::new(x + a1.cos() * 28.0, y + a1.sin() * 28.0),
                        3.0,
                        with_alpha(COPPER, 230),
                    );
                }
            }
        }
    }
    // The beams: fading in, then burning and turning (the ignition's two, back the other way
    // once rekindled, and the eclipse's one while the core holds its breath; Wave 11 carryover).
    for (angle, strength) in
        fatherload_simulation::heart::fight_beams(fight.phase, fight.rekindled, elapsed)
    {
        if strength <= 0.0 {
            continue;
        }
        let (s, c) = angle.sin_cos();
        let half = BEAM_HALF_WIDTH_TILES * TILE_SIZE;
        {
            let end = Vector2::new(cx + c * chamber, cy + s * chamber);
            let start = Vector2::new(cx, cy);
            let alpha = if strength < 1.0 { strength * 0.5 } else { 1.0 };
            draw.draw_line_ex(
                start,
                end,
                half * 2.6,
                with_alpha(ORANGE, (alpha * 60.0) as u8),
            );
            draw.draw_line_ex(
                start,
                end,
                half * 1.6,
                with_alpha(GOLD, (alpha * 150.0) as u8),
            );
            draw.draw_line_ex(
                start,
                end,
                half * 0.6,
                with_alpha(CREAM, (alpha * 240.0) as u8),
            );
        }
    }
    // The core.
    let core = 72.0;
    if collapsing {
        let breath_state = breath_at(elapsed);
        match breath_state {
            BreathState::Held | BreathState::Tell => {
                // A dark core with a bright rim; brightening inside before it exhales.
                let tell = if breath_state == BreathState::Tell {
                    0.5 + 0.3 * breath
                } else {
                    0.0
                };
                disc(draw, cx, cy, (core * 1.8) as i32, with_alpha(VIOLET, 60));
                disc(draw, cx, cy, (core + 6.0) as i32, with_alpha(CREAM, 230));
                disc(draw, cx, cy, core as i32, INK);
                if tell > 0.0 {
                    disc(
                        draw,
                        cx,
                        cy,
                        (core * 0.7) as i32,
                        with_alpha(GOLD, (tell * 200.0) as u8),
                    );
                    disc(
                        draw,
                        cx,
                        cy,
                        (core * 0.4) as i32,
                        with_alpha(CREAM, (tell * 220.0) as u8),
                    );
                }
            }
            BreathState::Exhale => {
                // Open and blazing: strike now.
                disc(draw, cx, cy, (core * 2.0) as i32, with_alpha(GOLD, 70));
                disc(draw, cx, cy, (core * 1.3) as i32, with_alpha(AMBER, 170));
                disc(draw, cx, cy, core as i32, CREAM);
                disc(draw, cx, cy, (core * 0.6) as i32, BONE);
            }
        }
        if breath_state == BreathState::Exhale {
            // A slow ring of light breathing out.
            let t = ((elapsed % fatherload_simulation::heart::BREATH_PERIOD_MILLIS)
                .saturating_sub(fatherload_simulation::heart::EXHALE_AT_MILLIS))
                as f32
                / 3_900.0;
            let r = core + chamber * t.clamp(0.0, 1.0);
            draw.draw_circle_lines(
                cx as i32,
                cy as i32,
                r,
                with_alpha(CREAM, (200.0 * (1.0 - t)).max(0.0) as u8),
            );
        }
    } else {
        let glow = if awake {
            1.0
        } else if fight.phase.fighting() {
            0.85
        } else {
            0.45 + 0.15 * breath
        };
        disc(
            draw,
            cx,
            cy,
            (core * 2.0) as i32,
            with_alpha(GOLD, (40.0 * glow) as u8),
        );
        disc(
            draw,
            cx,
            cy,
            (core * 1.35) as i32,
            with_alpha(AMBER, (130.0 * glow) as u8),
        );
        disc(
            draw,
            cx,
            cy,
            core as i32,
            with_alpha(CREAM, (200.0 * glow + 40.0) as u8),
        );
        disc(
            draw,
            cx,
            cy,
            (core * 0.6) as i32,
            with_alpha(BONE, (220.0 * glow) as u8),
        );
        // A sealed core shows its plates while the coils turn.
        if fight.phase.sealed() || fight.phase == HeartPhase::Asleep {
            for index in 0..6 {
                let a = index as f32 / 6.0 * std::f32::consts::TAU + spin;
                let (s, c) = a.sin_cos();
                draw.draw_line_ex(
                    Vector2::new(cx + c * core * 0.3, cy + s * core * 0.3),
                    Vector2::new(cx + c * core, cy + s * core),
                    5.0,
                    with_alpha(COPPER, 220),
                );
            }
            draw.draw_circle_lines(cx as i32, cy as i32, core, with_alpha(COPPER, 255));
        }
    }
    let _ = scene.effects_strength;
}

/// The Heart's world pieces in draw order for a view (wells and cable behind, the Star Drive).
pub(super) fn draw_heart_world(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    if !scene.terrain.settings().generator.has_descents() {
        return;
    }
    draw_core_glows(draw, scene);
    draw_wells(draw, scene);
    draw_cable(draw, scene);
    draw_heart(draw, scene);
}

/// Soft halos round Descent VIII's glowing tiles: dark matter, event glass, the rails' magnet
/// strips, star shards and the cable.
fn draw_core_glows(draw: &mut impl RaylibDraw, scene: &HeartScene<'_>) {
    let frame = crate::view_frame::frame();
    let min = frame.tile_at(scene.visible.x, scene.visible.y);
    let max = frame.tile_at(
        scene.visible.x + scene.visible.width,
        scene.visible.y + scene.visible.height,
    );
    let (top, bottom) = {
        let layout =
            fatherload_simulation::descent::fields::cached_layout(scene.terrain.settings());
        layout.descent_rows(fatherload_simulation::descent::Descent::Heart)
    };
    if max.y < top - 30 || min.y > bottom + 30 {
        return;
    }
    for position in (TileRect { min, max }).positions() {
        let Some(tile) = scene.terrain.tile(position) else {
            continue;
        };
        let fatherload_simulation::terrain::TileKind::Core(core) = tile.kind else {
            continue;
        };
        let Some(colour) = crate::rendering::terrain::tile_art::core_glow(core) else {
            continue;
        };
        let (cx, cy) = frame.tile_center(position);
        let phase = ((position.x * 31 + position.y * 17).rem_euclid(628)) as f32 / 100.0;
        let pulse = (scene.seconds * 1.2 + phase).sin().mul_add(0.5, 0.5);
        disc(
            draw,
            cx,
            cy,
            15,
            with_alpha(colour, (14.0 + pulse * 16.0) as u8),
        );
    }
}

/// The wells touching `visible` (the scene's input).
#[must_use]
pub fn wells_in_view(terrain: &Terrain, visible: Rectangle) -> Vec<Well> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let frame = crate::view_frame::frame();
    let min = frame.tile_at(visible.x, visible.y);
    let max = frame.tile_at(visible.x + visible.width, visible.y + visible.height);
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    fatherload_simulation::worldgen::descent::heart::wells_touching(
        settings,
        &layout,
        &TileRect { min, max },
    )
}
