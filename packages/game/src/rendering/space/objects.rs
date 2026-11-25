#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "screen geometry and pixel art"
)]

//! Everything in space that is not a round body or the sky: stars by class, nebulae, belts
//! and rocks, comets, derelicts and stations (placeholder pixel art built from rectangles,
//! readable at any zoom; Wave 12 replaces it), black holes and wormholes, and the craft.
//! Every animation is slow (under half a hertz) and small: nothing flashes.

use std::f32::consts::TAU;

use raylib::{ffi, prelude::*};

use fatherload_simulation::{PlayerId, RigCosmetics, star_chart::StarClass, terrain::TILE_SIZE};

use super::{
    Projection, SpaceArt, additive, draw_centred, fade, hash64, unit,
    view::{
        BeltView, BlackHoleView, CometView, CraftView, SpaceRenderView, StructureKind,
        StructureView,
    },
};
use crate::rendering::{pixel_art::palette, rig_art};

/// A star class's colours: corona, surface, core.
#[must_use]
pub const fn star_colours(class: StarClass) -> [Color; 3] {
    match class {
        StarClass::Yellow => [palette::AMBER, palette::GOLD, palette::CREAM],
        StarClass::Red => [palette::RED, palette::ORANGE, palette::AMBER],
        StarClass::Blue => [palette::SKY, palette::SKY_LIGHT, palette::ICE],
        StarClass::White => [palette::MIST, palette::BONE, palette::BONE],
        StarClass::Binary => [palette::ORANGE, palette::GOLD, palette::CREAM],
        StarClass::Nebula => [palette::LAVENDER, palette::PINK, palette::BONE],
    }
}

/// A point `distance` points from `centre` toward screen angle `angle`.
fn toward(centre: (f32, f32), angle: f32, distance: f32) -> (f32, f32) {
    (
        centre.0 + angle.cos() * distance,
        centre.1 + angle.sin() * distance,
    )
}

fn v(point: (f32, f32)) -> Vector2 {
    Vector2::new(point.0, point.1)
}

/// Draws the suns.
pub fn draw_stars(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for star in view.stars {
        let point = projection.project(star.centre);
        let radius = projection.tiles(star.radius_tiles);
        let reach = (radius * 7.0).max(40.0);
        if !projection.visible(point, reach) {
            continue;
        }
        let colours = star_colours(star.class);
        if star.class == StarClass::Binary {
            // Two suns circling each other, slowly.
            let turn = view.seconds * 0.04 + unit(u64::from(star.seed), 0) * TAU;
            let gap = (radius * 1.6).max(5.0);
            let a = toward(point, turn - projection.roll, gap);
            let b = toward(
                point,
                turn - projection.roll + std::f32::consts::PI,
                gap * 1.3,
            );
            draw_star(
                draw,
                art,
                b,
                radius * 0.6,
                star_colours(StarClass::Red),
                star.seed ^ 7,
                view.seconds,
            );
            draw_star(draw, art, a, radius, colours, star.seed, view.seconds);
        } else {
            if star.class == StarClass::Nebula
                && let Some(cloud) = art.clouds[(star.seed % 3) as usize].as_ref()
            {
                let size = (radius * 16.0).max(120.0);
                draw_centred(
                    cloud,
                    point,
                    (size, size),
                    star.seed as f32 % 360.0,
                    fade(palette::VIOLET, 0.35),
                );
            }
            draw_star(draw, art, point, radius, colours, star.seed, view.seconds);
        }
    }
}

fn draw_star(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    point: (f32, f32),
    radius: f32,
    [corona, surface, core]: [Color; 3],
    seed: u32,
    seconds: f32,
) {
    let h = hash64(&[i64::from(seed), 5]);
    additive(|| {
        if let Some(glow) = art.glow.as_ref() {
            let outer = (radius * 9.0).max(46.0);
            draw_centred(glow, point, (outer, outer), 0.0, fade(corona, 0.28));
            let inner = (radius * 3.4).max(18.0);
            draw_centred(glow, point, (inner, inner), 0.0, fade(surface, 0.55));
        }
        // Diffraction spikes stay square to the screen (they are the eye's, not the star's).
        let spike = (radius * 4.5).max(16.0);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            draw.draw_line_ex(
                v(point),
                Vector2::new(point.0 + dx * spike, point.1 + dy * spike),
                if radius > 8.0 { 2.0 } else { 1.0 },
                fade(surface, 0.5),
            );
        }
    });
    if radius < 1.5 {
        draw.draw_rectangle_rec(
            Rectangle::new(point.0.round() - 1.0, point.1.round() - 1.0, 2.0, 2.0),
            core,
        );
        return;
    }
    let centre = v(point);
    draw.draw_circle_v(centre, radius * 1.06, fade(corona, 0.9));
    draw.draw_circle_v(centre, radius, surface);
    draw.draw_circle_v(centre, radius * 0.78, fade(core, 0.85));
    draw.draw_circle_v(centre, radius * 0.5, core);
    if radius >= 6.0 {
        // Spots drift across the face.
        for index in 0..3 {
            let hs = hash64(&[h as i64, index]);
            let angle = unit(hs, 0) * TAU + seconds * 0.01;
            let distance = radius * (0.2 + 0.5 * unit(hs, 16));
            let spot = toward(point, angle, distance);
            draw.draw_circle_v(
                v(spot),
                radius * (0.05 + 0.05 * unit(hs, 32)),
                fade(corona, 0.7),
            );
        }
        // Prominences: slow loops on the rim, rising and falling over many seconds.
        additive(|| {
            for index in 0..4 {
                let hs = hash64(&[h as i64, 10 + index]);
                let angle = unit(hs, 0) * TAU + seconds * 0.006;
                let rise = 0.55 + 0.45 * (seconds * 0.12 + unit(hs, 16) * TAU).sin();
                let foot = toward(point, angle, radius * 0.98);
                let size = radius * (0.12 + 0.1 * unit(hs, 32)) * rise;
                let facing = angle.to_degrees();
                draw.draw_ring(
                    v(foot),
                    size * 0.7,
                    size,
                    facing - 90.0,
                    facing + 90.0,
                    10,
                    fade(corona, 0.75),
                );
            }
        });
    }
}

/// Draws the nebulae: overlapping cloud quads in their two colours, with young stars inside.
pub fn draw_nebulae(art: &SpaceArt, view: &SpaceRenderView<'_>, projection: &Projection) {
    for nebula in view.nebulae {
        let point = projection.project(nebula.centre);
        let radius = projection.tiles(nebula.radius_tiles);
        if !projection.visible(point, radius * 1.4) || radius < 2.0 {
            continue;
        }
        // Eight overlapping clouds: enough depth, little overdraw (software WebGL).
        for index in 0..8 {
            let h = hash64(&[i64::from(nebula.seed), index]);
            let angle = unit(h, 0) * TAU;
            let distance = radius * 0.6 * unit(h, 16).sqrt();
            let (dx, dy) = projection.turn((angle.cos() * distance, angle.sin() * distance));
            let size = radius * (0.7 + 0.7 * unit(h, 32));
            let colour = if index % 3 == 0 {
                nebula.colours[0]
            } else {
                nebula.colours[1]
            };
            if let Some(cloud) = art.clouds[(h >> 50) as usize % 3].as_ref() {
                draw_centred(
                    cloud,
                    (point.0 + dx, point.1 + dy),
                    (size, size * (0.7 + 0.3 * unit(h, 40))),
                    unit(h, 8) * 360.0 - projection.roll.to_degrees(),
                    fade(colour, 0.34),
                );
            }
        }
        additive(|| {
            for index in 0..6 {
                let h = hash64(&[i64::from(nebula.seed), 100 + index]);
                let angle = unit(h, 0) * TAU;
                let distance = radius * 0.5 * unit(h, 16);
                let (dx, dy) = projection.turn((angle.cos() * distance, angle.sin() * distance));
                if let Some(glow) = art.glow.as_ref() {
                    let size = 10.0 + 14.0 * unit(h, 32);
                    draw_centred(
                        glow,
                        (point.0 + dx, point.1 + dy),
                        (size, size),
                        0.0,
                        fade(palette::BONE, 0.7),
                    );
                    draw_centred(
                        glow,
                        (point.0 + dx, point.1 + dy),
                        (size * 3.0, size * 3.0),
                        0.0,
                        fade(nebula.colours[0], 0.3),
                    );
                }
            }
        });
    }
}

/// Inside a nebula the view takes on its colour (its sensor fog, as the eye sees it).
pub fn draw_nebula_fog(
    draw: &mut impl RaylibDraw,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for nebula in view.nebulae {
        let distance = projection.camera.distance(nebula.centre) / TILE_SIZE;
        let inside = 1.0 - distance / nebula.radius_tiles.max(1.0);
        if inside > 0.0 {
            draw.draw_rectangle_rec(
                projection.bounds,
                fade(nebula.colours[1], 0.22 * (inside * 2.0).min(1.0)),
            );
        }
    }
}

/// Draws the wormholes: a dark mouth showing the far side's stars, swirling arms, a bright rim.
pub fn draw_wormholes(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for hole in view.wormholes {
        let point = projection.project(hole.centre);
        let radius = projection.tiles(hole.radius_tiles).max(5.0);
        if !projection.visible(point, radius * 3.0) {
            continue;
        }
        let spin = view.seconds * 0.3;
        if let Some(glow) = art.glow.as_ref() {
            additive(|| {
                draw_centred(
                    glow,
                    point,
                    (radius * 6.0, radius * 6.0),
                    0.0,
                    fade(hole.tint, 0.35),
                );
            });
        }
        let dark = Color::new(hole.tint.r / 8, hole.tint.g / 8, hole.tint.b / 6 + 8, 255);
        draw.draw_circle_v(v(point), radius, dark);
        // The far side's stars, turning slowly the other way.
        for index in 0..28 {
            let h = hash64(&[i64::from(hole.seed), index]);
            let angle = unit(h, 0) * TAU - spin * 0.4;
            let distance = radius * 0.85 * unit(h, 16).sqrt();
            let star = toward(point, angle, distance);
            let colour = if index % 4 == 0 {
                hole.tint
            } else {
                palette::BONE
            };
            draw.draw_rectangle_rec(
                Rectangle::new(star.0.round(), star.1.round(), 1.0, 1.0),
                fade(colour, 0.5 + 0.5 * unit(h, 32)),
            );
        }
        additive(|| {
            for arm in 0..4 {
                for step in 0..64 {
                    let k = step as f32;
                    let angle = arm as f32 * TAU / 4.0 + k * 0.09 + spin / (1.0 + k * 0.04);
                    let distance = radius * (1.0 + k * 0.022);
                    let dot = toward(point, angle, distance);
                    let size = (radius * 0.11 * (1.0 - k / 70.0)).max(1.5).round();
                    let colour = if step < 14 { palette::BONE } else { hole.tint };
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            (dot.0 - size * 0.5).round(),
                            (dot.1 - size * 0.5).round(),
                            size,
                            size,
                        ),
                        fade(colour, 0.75 * (1.0 - k / 64.0)),
                    );
                }
            }
            draw.draw_ring(
                v(point),
                radius * 0.94,
                radius * 1.05,
                0.0,
                360.0,
                48,
                fade(palette::BONE, 0.7),
            );
        });
    }
}

/// The accretion disc's colour from its inner edge (0) to its outer edge (1).
fn disc_colour(t: f32) -> Color {
    match (t * 5.0) as i32 {
        0 => palette::CREAM,
        1 => palette::GOLD,
        2 => palette::AMBER,
        3 => palette::ORANGE,
        _ => palette::RED,
    }
}

/// One half (or all) of a black hole's disc: ellipse rings, brighter on the side turning
/// toward the viewer, streaked, turning slowly (faster inside). `front` picks the near half
/// (drawn again over the shadow), `None` the whole disc.
fn draw_disc(
    hole: &BlackHoleView,
    point: (f32, f32),
    projection: &Projection,
    seconds: f32,
    front: Option<bool>,
) {
    const RINGS: i32 = 16;
    const SEGMENTS: i32 = 36;
    let horizon = projection.tiles(hole.horizon_tiles).max(3.0);
    let outer = projection.tiles(hole.disc_tiles).max(horizon * 3.0);
    let inner = horizon * 1.6;
    // SAFETY: a matrix push balanced by the pop below.
    unsafe {
        ffi::rlPushMatrix();
        ffi::rlTranslatef(point.0, point.1, 0.0);
        ffi::rlRotatef((hole.tilt - projection.roll).to_degrees(), 0.0, 0.0, 1.0);
        ffi::rlScalef(1.0, 0.24, 1.0);
    }
    for ring in 0..RINGS {
        let t = ring as f32 / RINGS as f32;
        let r0 = inner + (outer - inner) * t;
        let r1 = inner + (outer - inner) * (ring + 1) as f32 / RINGS as f32;
        let colour = disc_colour(t);
        let turn = seconds * 0.25 / (1.0 + t * 3.0);
        for segment in 0..SEGMENTS {
            let a0 = segment as f32 / SEGMENTS as f32 * 360.0;
            let a1 = a0 + 360.0 / SEGMENTS as f32;
            let middle = (a0 + a1) * 0.5;
            // Lower half (y down in the disc's frame) passes in front.
            let near = middle < 180.0;
            if front.is_some_and(|want| want != near) {
                continue;
            }
            let h = hash64(&[
                i64::from(ring),
                ((middle / 360.0 + turn).rem_euclid(1.0) * 24.0) as i64,
            ]);
            let streak = 0.55 + 0.45 * unit(h, 0);
            // The side coming toward the viewer is brighter (beaming).
            let beam = 0.55 + 0.45 * (middle.to_radians()).cos();
            let alpha = (streak * beam * (1.0 - t * 0.55)).clamp(0.08, 1.0);
            ffiless_ring(r0, r1, a0, a1, fade(colour, alpha));
        }
    }
    // SAFETY: closes the push above.
    unsafe { ffi::rlPopMatrix() };
}

/// `DrawRing` round the current origin.
fn ffiless_ring(inner: f32, outer: f32, start: f32, end: f32, colour: Color) {
    // SAFETY: an immediate-mode ring under the caller's matrix.
    unsafe {
        ffi::DrawRing(
            ffi::Vector2 { x: 0.0, y: 0.0 },
            inner,
            outer,
            start,
            end,
            3,
            colour,
        );
    }
}

/// Draws every black hole's glow and disc (before the lens bends them).
pub fn draw_black_hole_discs(art: &SpaceArt, view: &SpaceRenderView<'_>, projection: &Projection) {
    for hole in view.black_holes {
        let point = projection.project(hole.centre);
        let outer = projection.tiles(hole.disc_tiles);
        if !projection.visible(point, outer * 2.5) {
            continue;
        }
        if let Some(glow) = art.glow.as_ref() {
            additive(|| {
                draw_centred(
                    glow,
                    point,
                    (outer * 4.0, outer * 1.6),
                    (hole.tilt - projection.roll).to_degrees(),
                    fade(palette::ORANGE, 0.3),
                );
            });
        }
        additive(|| draw_disc(hole, point, projection, view.seconds, None));
    }
}

/// After the lens (or without it): the near half of the disc over the shadow.
pub fn draw_black_hole_front(view: &SpaceRenderView<'_>, projection: &Projection) {
    for hole in view.black_holes {
        let point = projection.project(hole.centre);
        if projection.visible(point, projection.tiles(hole.disc_tiles) * 2.5) {
            additive(|| draw_disc(hole, point, projection, view.seconds, Some(true)));
        }
    }
}

/// Without the lens shader: the horizon's shadow and a photon ring.
pub fn draw_black_hole_shadow(
    draw: &mut impl RaylibDraw,
    hole: &BlackHoleView,
    point: (f32, f32),
    projection: &Projection,
) {
    let horizon = projection.tiles(hole.horizon_tiles).max(3.0);
    draw.draw_circle_v(v(point), horizon * 1.25, fade(palette::AMBER, 0.35));
    draw.draw_circle_v(v(point), horizon * 1.08, fade(palette::CREAM, 0.6));
    draw.draw_circle_v(v(point), horizon, Color::BLACK);
}

/// Draws the asteroid belts: single rocks near the camera, a speckled band further out.
pub fn draw_belts(draw: &mut impl RaylibDraw, view: &SpaceRenderView<'_>, projection: &Projection) {
    for belt in view.belts {
        draw_belt(draw, belt, view, projection);
    }
}

fn draw_belt(
    draw: &mut impl RaylibDraw,
    belt: &BeltView,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    let tile = f64::from(TILE_SIZE);
    // The camera relative to the belt's centre (tiles), and the belt's centre relative to the
    // camera (world pixels).
    let (cx, cy) = belt.centre.delta_f64(projection.camera);
    let (cx, cy) = (cx / tile, cy / tile);
    let centre = projection.camera.delta_f64(belt.centre);
    let distance = cx.hypot(cy);
    let reach = f64::from(projection.reach() / projection.px_per_tile);
    let radius = f64::from(belt.radius_tiles);
    let half_width = f64::from(belt.width_tiles) * 0.5;
    if (distance - radius).abs() > half_width + reach {
        return;
    }
    // Cells about square, a few hundred on screen at any zoom (rocks smaller than a point
    // become dust specks).
    let cell = (reach / 14.0).max(36.0);
    let inner = (distance - reach).max(radius - half_width);
    let outer = (distance + reach).min(radius + half_width);
    let camera_angle = cy.atan2(cx);
    let half_arc = if distance > reach {
        (reach / distance).asin() * 1.1
    } else {
        std::f64::consts::PI
    };
    let level = cell.log2().floor() as i64;
    let cell = 2_f64.powi(level as i32);
    let ring_cells = |r: f64| (std::f64::consts::TAU * r / cell).round().max(1.0) as i64;
    let scale = f64::from(projection.scale);
    let light = projection.turn(view.light_toward(projection.camera));
    let reach_points = f64::from(projection.reach()) * 1.1;
    let first_ring = ((inner - (radius - half_width)) / cell).floor() as i64;
    let last_ring = ((outer - (radius - half_width)) / cell).ceil() as i64;
    for ring in first_ring..=last_ring {
        let r0 = radius - half_width + ring as f64 * cell;
        let count = ring_cells(r0 + cell * 0.5);
        let step = std::f64::consts::TAU / count as f64;
        let first = ((camera_angle - half_arc) / step).floor() as i64;
        let last = ((camera_angle + half_arc) / step).ceil() as i64;
        for index in first..=last {
            let wrapped = index.rem_euclid(count);
            let h = hash64(&[i64::from(belt.seed), level, ring, wrapped]);
            // In clumps; seen from far off, a cell stands for many rocks: more specks (capped),
            // so the belt still reads as a dusty ring.
            let clump = 0.4 + 0.9 * f64::from(unit(h, 0));
            let far = (cell / 36.0).clamp(1.0, 120.0);
            let rocks = (f64::from(belt.density) * clump * 0.8 * far + f64::from(unit(h, 16)))
                .floor() as i64;
            for rock in 0..rocks.max(0) {
                let hr = hash64(&[h as i64, rock]);
                let angle = (index as f64 + f64::from(unit(hr, 0))) * step;
                let r = r0 + f64::from(unit(hr, 16)) * cell;
                // Denser along the belt's middle, none outside it.
                let across = (r - radius) / half_width;
                if f64::from(unit(hr, 32)) > 1.0 - across * across {
                    continue;
                }
                let r = r * tile;
                let (x, y) = (
                    (centre.0 + angle.cos() * r) * scale,
                    (centre.1 + angle.sin() * r) * scale,
                );
                if x.abs() > reach_points || y.abs() > reach_points {
                    continue;
                }
                let (sx, sy) = projection.turn((x as f32, y as f32));
                let point = (projection.pivot.0 + sx, projection.pivot.1 + sy);
                // Mostly gravel, a few boulders, now and then a big one (radius, tiles).
                let size_tiles = 0.3 + 9.0 * unit(hr, 48).powi(7);
                let size = size_tiles * projection.px_per_tile;
                if !projection.visible(point, size + 2.0) {
                    continue;
                }
                if size < 1.4 {
                    let tone = [palette::STONE, palette::STONE_LIGHT, palette::SLATE_LIGHT]
                        [(hr >> 60) as usize % 3];
                    let speck = if size > 0.7 { 2.0 } else { 1.0 };
                    draw.draw_rectangle_rec(
                        Rectangle::new(point.0.round(), point.1.round(), speck, speck),
                        fade(tone, 0.55 + 0.4 * unit(hr, 32)),
                    );
                } else {
                    draw_rock(draw, point, size, hr, light);
                }
            }
        }
    }
}

/// Draws the loose asteroids.
pub fn draw_asteroids(
    draw: &mut impl RaylibDraw,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for rock in view.asteroids {
        let point = projection.project(rock.centre);
        let size = projection.tiles(rock.radius_tiles);
        if !projection.visible(point, size * 1.3) {
            continue;
        }
        let light = projection.turn(view.light_toward(rock.centre));
        if size < 1.4 {
            draw.draw_rectangle_rec(
                Rectangle::new(point.0.round(), point.1.round(), 2.0, 2.0),
                palette::STONE_LIGHT,
            );
        } else {
            draw_rock(draw, point, size, u64::from(rock.seed), light);
        }
    }
}

/// One rock: a lumpy polygon in three light steps toward `light`, a crater or two and, on
/// some, glints of ore.
pub fn draw_rock(
    draw: &mut impl RaylibDraw,
    point: (f32, f32),
    radius: f32,
    h: u64,
    light: (f32, f32),
) {
    const SIDES: usize = 10;
    let tones = match (h >> 56) % 5 {
        0 => [palette::RUST, palette::COPPER, palette::SAND],
        1 => [palette::SLATE_DARK, palette::SLATE, palette::SLATE_LIGHT],
        _ => [palette::STONE_DARK, palette::STONE, palette::STONE_LIGHT],
    };
    let turn = unit(h, 4) * TAU;
    let mut outline = [(0.0_f32, 0.0_f32); SIDES];
    for (index, slot) in outline.iter_mut().enumerate() {
        let angle = turn + index as f32 / SIDES as f32 * TAU;
        let lump = 0.72 + 0.28 * unit(hash64(&[h as i64, index as i64]), 0);
        *slot = (angle.cos() * radius * lump, angle.sin() * radius * lump);
    }
    let fan = |draw: &mut dyn FnMut(Vector2, Vector2, Vector2), scale: f32, shift: (f32, f32)| {
        let centre = Vector2::new(point.0 + shift.0, point.1 + shift.1);
        for index in 0..SIDES {
            let (a, b) = (outline[index], outline[(index + 1) % SIDES]);
            let pa = Vector2::new(centre.x + a.0 * scale, centre.y + a.1 * scale);
            let pb = Vector2::new(centre.x + b.0 * scale, centre.y + b.1 * scale);
            // raylib wants counter-clockwise triangles; the outline turns clockwise on screen.
            draw(centre, pb, pa);
        }
    };
    let mut layer = |colour: Color, scale: f32, shift: (f32, f32)| {
        fan(
            &mut |a, b, c| draw.draw_triangle(a, b, c, colour),
            scale,
            shift,
        );
    };
    layer(tones[0], 1.0, (0.0, 0.0));
    layer(
        tones[1],
        0.78,
        (light.0 * radius * 0.16, light.1 * radius * 0.16),
    );
    layer(
        tones[2],
        0.38,
        (light.0 * radius * 0.4, light.1 * radius * 0.4),
    );
    if radius > 4.0 {
        for crater in 0..2 {
            let hc = hash64(&[h as i64, 50 + crater]);
            let angle = unit(hc, 0) * TAU;
            let at = toward(point, angle, radius * 0.4 * unit(hc, 16));
            let size = radius * (0.12 + 0.1 * unit(hc, 32));
            draw.draw_circle_v(v(at), size, fade(tones[0], 0.9));
            draw.draw_circle_v(
                v((at.0 + light.0 * size * 0.3, at.1 + light.1 * size * 0.3)),
                size * 0.6,
                fade(tones[1], 0.9),
            );
        }
    }
    if (h >> 40).is_multiple_of(3) {
        for glint in 0..3 {
            let hg = hash64(&[h as i64, 80 + glint]);
            let at = toward(point, unit(hg, 0) * TAU, radius * 0.55 * unit(hg, 16));
            let colour = if glint == 0 {
                palette::GOLD
            } else {
                palette::MINT
            };
            let size = (radius * 0.12).clamp(1.0, 3.0);
            draw.draw_rectangle_rec(
                Rectangle::new(at.0.round(), at.1.round(), size, size),
                colour,
            );
        }
    }
}

/// Draws the comets: a nucleus in its glowing coma, a curved dust tail and a straight ion
/// tail, both streaming slowly away from the star.
pub fn draw_comets(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for comet in view.comets {
        draw_comet(draw, art, comet, view, projection);
    }
}

fn draw_comet(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    comet: &CometView,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    let point = projection.project(comet.centre);
    let length = projection.tiles(comet.tail_tiles).max(40.0);
    if !projection.visible(point, length) {
        return;
    }
    let radius = projection.tiles(comet.radius_tiles).max(2.0);
    let angle = projection.screen_angle(comet.tail_angle);
    let (dx, dy) = (angle.cos(), angle.sin());
    let (nx, ny) = (-dy, dx);
    additive(|| {
        // The tails' glow: soft blobs along each path, widening and fading.
        if let Some(glow) = art.glow.as_ref() {
            for index in 0..32 {
                let t = index as f32 / 32.0;
                let along = length * t;
                let bend = length * 0.14 * t * t;
                let dust = (
                    point.0 + dx * along + nx * bend,
                    point.1 + dy * along + ny * bend,
                );
                let size = radius * 4.0 + length * 0.2 * t;
                let colour = if t < 0.3 {
                    palette::CREAM
                } else {
                    palette::AMBER
                };
                draw_centred(
                    glow,
                    dust,
                    (size, size),
                    0.0,
                    fade(colour, 0.32 * (1.0 - t).powf(1.4)),
                );
                let ion_along = length * 1.25 * t;
                let ion = (point.0 + dx * ion_along, point.1 + dy * ion_along);
                let width = radius * 2.5 + length * 0.04 * t;
                draw_centred(
                    glow,
                    ion,
                    (width, width),
                    0.0,
                    fade(palette::SKY, 0.3 * (1.0 - t)),
                );
            }
        }
        // Dust tail: broad, curving, cream to amber.
        for index in 0..260 {
            let h = hash64(&[i64::from(comet.seed), index]);
            let t = (unit(h, 0) + view.seconds * 0.02).fract();
            let width = radius * 1.5 + length * 0.1 * t;
            let bend = length * 0.14 * t * t;
            let side = (unit(h, 16) * 2.0 - 1.0) * width;
            let along = length * t;
            let x = point.0 + dx * along + nx * (bend + side);
            let y = point.1 + dy * along + ny * (bend + side);
            let colour = if t < 0.35 {
                palette::CREAM
            } else {
                palette::AMBER
            };
            let speck = if t < 0.2 { 3.0 } else { 2.0 };
            draw.draw_rectangle_rec(
                Rectangle::new(x.round(), y.round(), speck, speck),
                fade(colour, (1.0 - t).powf(1.6) * 0.75),
            );
        }
        // Ion tail: straight, thin, blue, dashes streaming outward.
        for index in 0..60 {
            let h = hash64(&[i64::from(comet.seed), 1_000 + index]);
            let t = (unit(h, 0) + view.seconds * 0.05).fract();
            let along = length * 1.25 * t;
            let side = (unit(h, 16) * 2.0 - 1.0) * radius * 0.6;
            let start = Vector2::new(
                point.0 + dx * along + nx * side,
                point.1 + dy * along + ny * side,
            );
            let dash = length * 0.05;
            draw.draw_line_ex(
                start,
                Vector2::new(start.x + dx * dash, start.y + dy * dash),
                1.0,
                fade(palette::SKY_LIGHT, (1.0 - t) * 0.6),
            );
        }
        if let Some(glow) = art.glow.as_ref() {
            let coma = (radius * 9.0).max(22.0);
            draw_centred(glow, point, (coma, coma), 0.0, fade(palette::MINT, 0.5));
            draw_centred(
                glow,
                point,
                (coma * 0.4, coma * 0.4),
                0.0,
                fade(palette::BONE, 0.7),
            );
        }
    });
    draw_rock(draw, point, radius * 0.6, u64::from(comet.seed), (-dx, -dy));
}

/// Draws the derelicts and stations.
pub fn draw_structures(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    view: &SpaceRenderView<'_>,
    projection: &Projection,
) {
    for structure in view.structures {
        let point = projection.project(structure.centre);
        let size = projection.tiles(structure.size_tiles);
        if !projection.visible(point, size) {
            continue;
        }
        // One art pixel: the structure is 64 across.
        let px = size / 64.0;
        if px < 0.2 {
            draw.draw_rectangle_rec(
                Rectangle::new(point.0.round(), point.1.round(), 2.0, 2.0),
                palette::STEEL_LIGHT,
            );
            continue;
        }
        // SAFETY: a matrix push balanced by the pop below.
        unsafe {
            ffi::rlPushMatrix();
            ffi::rlTranslatef(point.0, point.1, 0.0);
            ffi::rlRotatef(
                projection.screen_angle(structure.angle).to_degrees(),
                0.0,
                0.0,
                1.0,
            );
            ffi::rlScalef(px, px, 1.0);
        }
        match structure.kind {
            StructureKind::Station => draw_station(draw, art, structure, view.seconds),
            StructureKind::Derelict => draw_derelict(draw, structure, view.seconds),
        }
        // SAFETY: closes the push above.
        unsafe { ffi::rlPopMatrix() };
    }
}

/// Art-pixel rectangle (the structure's own grid, centred on 0, 0).
fn block(draw: &mut impl RaylibDraw, x: f32, y: f32, w: f32, h: f32, colour: Color) {
    draw.draw_rectangle_rec(Rectangle::new(x, y, w, h), colour);
}

/// A station hub: a ring with modules, a truss, two solar wings, docking arms with slow
/// lights and an antenna. 64 art pixels across, centred.
#[allow(
    clippy::too_many_lines,
    reason = "one placeholder's parts, top to bottom"
)]
fn draw_station(
    draw: &mut impl RaylibDraw,
    art: &SpaceArt,
    structure: &StructureView,
    seconds: f32,
) {
    use palette::{
        DEEP_BLUE, GOLD, INK, MINT, OCEAN, RED, SKY, STEEL, STEEL_DARK, STEEL_LIGHT, TEAL,
    };
    let _ = structure;
    // Truss.
    block(draw, -30.0, -1.0, 60.0, 2.0, STEEL_DARK);
    for x in (-28..28).step_by(4) {
        block(draw, x as f32, -2.0, 1.0, 4.0, STEEL);
    }
    // Solar wings.
    for side in [-1.0_f32, 1.0] {
        let x0 = if side < 0.0 { -31.0 } else { 17.0 };
        block(draw, x0, -9.0, 14.0, 18.0, STEEL_DARK);
        for row in 0..4 {
            for column in 0..3 {
                let colour = if (row + column) % 2 == 0 {
                    OCEAN
                } else {
                    DEEP_BLUE
                };
                block(
                    draw,
                    x0 + 1.0 + column as f32 * 4.3,
                    -8.0 + row as f32 * 4.2,
                    3.6,
                    3.6,
                    colour,
                );
                block(
                    draw,
                    x0 + 1.0 + column as f32 * 4.3,
                    -8.0 + row as f32 * 4.2,
                    1.0,
                    1.0,
                    SKY,
                );
            }
        }
    }
    // Hub ring and core.
    // SAFETY: rings under the caller's matrix.
    unsafe {
        ffi::DrawRing(
            ffi::Vector2 { x: 0.0, y: 0.0 },
            9.0,
            13.0,
            0.0,
            360.0,
            28,
            STEEL,
        );
        ffi::DrawRing(
            ffi::Vector2 { x: 0.0, y: 0.0 },
            12.0,
            13.0,
            200.0,
            340.0,
            12,
            STEEL_LIGHT,
        );
        ffi::DrawCircle(0, 0, 6.5, STEEL_DARK);
        ffi::DrawCircle(0, 0, 4.5, TEAL);
        ffi::DrawCircle(-1, -1, 2.0, MINT);
    }
    // Windows round the ring.
    for index in 0..8 {
        let angle = index as f32 / 8.0 * TAU;
        let (x, y) = (angle.cos() * 11.0, angle.sin() * 11.0);
        block(
            draw,
            x - 0.5,
            y - 0.5,
            1.0,
            1.0,
            if index % 3 == 0 { INK } else { GOLD },
        );
    }
    // Docking arms, their lights blinking once a second.
    let lit = (seconds as i64) % 2 == 0;
    for side in [-1.0_f32, 1.0] {
        let y0 = if side < 0.0 { -24.0 } else { 13.0 };
        block(draw, -2.0, y0, 4.0, 11.0, STEEL_DARK);
        block(
            draw,
            -4.0,
            if side < 0.0 { y0 } else { y0 + 9.0 },
            8.0,
            2.0,
            STEEL_LIGHT,
        );
        let light = if lit == (side < 0.0) { MINT } else { TEAL };
        block(
            draw,
            -4.0,
            if side < 0.0 { y0 - 1.0 } else { y0 + 11.0 },
            1.0,
            1.0,
            light,
        );
        block(
            draw,
            3.0,
            if side < 0.0 { y0 - 1.0 } else { y0 + 11.0 },
            1.0,
            1.0,
            light,
        );
    }
    // Antenna.
    block(draw, 9.0, -20.0, 1.0, 9.0, STEEL_LIGHT);
    block(draw, 7.0, -21.0, 5.0, 1.0, STEEL);
    block(draw, 9.0, -22.0, 1.0, 1.0, RED);
    if let Some(glow) = art.glow.as_ref() {
        additive(|| draw_centred(glow, (0.0, 0.0), (30.0, 30.0), 0.0, fade(TEAL, 0.35)));
    }
}

/// A derelict: a broken hull in two drifting pieces, ribs in the gap, scorch and rust, dead
/// windows, one dim light, and debris. 64 art pixels across, centred.
fn draw_derelict(draw: &mut impl RaylibDraw, structure: &StructureView, seconds: f32) {
    use palette::{AMBER, INK, RUST, SLATE, STEEL, STEEL_DARK, STEEL_LIGHT, STONE_DARK};
    let h = u64::from(structure.seed);
    // Front piece: a long hull with a blunt nose.
    block(draw, -30.0, -6.0, 26.0, 12.0, STEEL_DARK);
    block(draw, -30.0, -6.0, 26.0, 2.0, STEEL);
    block(draw, -32.0, -4.0, 2.0, 8.0, STEEL_DARK);
    block(draw, -26.0, -2.0, 6.0, 3.0, SLATE);
    for x in 0..4 {
        block(draw, -18.0 + x as f32 * 3.5, -2.0, 2.0, 2.0, INK);
    }
    block(draw, -22.0, 2.0, 9.0, 3.0, RUST);
    // The broken edge: jagged steps.
    for step in 0..6 {
        let jag = (unit(hash64(&[h as i64, step]), 0) * 3.0).floor();
        block(
            draw,
            -4.0,
            -6.0 + step as f32 * 2.0,
            jag + 1.0,
            2.0,
            STEEL_DARK,
        );
    }
    // Ribs across the gap.
    for rib in 0..3 {
        block(draw, -1.0 + rib as f32 * 3.0, -5.0, 1.0, 10.0, STONE_DARK);
    }
    // Rear piece, drifted and turned a little.
    // SAFETY: a matrix push balanced by the pop below.
    unsafe {
        ffi::rlPushMatrix();
        ffi::rlTranslatef(18.0 + (seconds * 0.05).sin() * 0.6, 2.0, 0.0);
        ffi::rlRotatef(9.0 + (seconds * 0.04).sin() * 1.5, 0.0, 0.0, 1.0);
    }
    block(draw, -9.0, -7.0, 20.0, 14.0, STEEL_DARK);
    block(draw, -9.0, -7.0, 20.0, 2.0, STEEL);
    block(draw, 9.0, -9.0, 4.0, 18.0, STEEL);
    block(draw, 12.0, -6.0, 3.0, 4.0, STEEL_LIGHT);
    block(draw, 12.0, 2.0, 3.0, 4.0, STEEL_LIGHT);
    block(draw, -6.0, 1.0, 8.0, 4.0, RUST);
    block(draw, -2.0, -3.0, 2.0, 2.0, INK);
    // One light still on, breathing slowly.
    let breath = 0.45 + 0.35 * (seconds * 0.8).sin();
    block(draw, 3.0, -3.0, 2.0, 2.0, fade(AMBER, breath));
    // SAFETY: closes the push above.
    unsafe { ffi::rlPopMatrix() };
    // Debris drifting round it.
    for bit in 0..7 {
        let hb = hash64(&[h as i64, 200 + bit]);
        let angle = unit(hb, 0) * TAU + seconds * 0.02 * (unit(hb, 8) - 0.5);
        let distance = 20.0 + 16.0 * unit(hb, 16);
        let (x, y) = (angle.cos() * distance, angle.sin() * distance * 0.6);
        let size = 1.0 + (unit(hb, 32) * 2.0).floor();
        block(
            draw,
            x,
            y,
            size,
            size,
            if bit % 2 == 0 { STEEL } else { RUST },
        );
    }
}

/// The craft at the pivot, upright (the camera is locked to it).
pub fn draw_craft(
    draw: &mut impl RaylibDraw,
    craft: CraftView,
    projection: &Projection,
    seconds: f32,
) {
    let ticks = (seconds * 60.0) as u64;
    let look = craft.look.unwrap_or_else(|| rig_art::RigLook {
        player_id: PlayerId::new(1),
        cosmetics: RigCosmetics::default_for(PlayerId::new(1)),
        tiers: rig_art::RigTiers::all(6),
    });
    let mut pose = rig_art::RigPose::idle(ticks);
    if craft.thrusting {
        pose.thrust = rig_art::Thrust::Climb;
    }
    let canvas = rig_art::paint_rig(&look, &pose);
    // Two points per art pixel, as on the ground: the craft stays readable at any zoom.
    let scale = 2.0;
    let (width, height) = (
        rig_art::BODY_WIDTH as f32 * scale,
        rig_art::BODY_HEIGHT as f32 * scale,
    );
    canvas.draw(
        draw,
        Vector2::new(
            (projection.pivot.0 - width * 0.5).round(),
            (projection.pivot.1 - height * 0.5).round(),
        ),
        scale,
        false,
    );
}
