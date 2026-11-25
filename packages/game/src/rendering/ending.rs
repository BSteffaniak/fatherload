//! The ending sequence's screens (Wave 10, the Heart; the state machine lives in
//! `fatherload_simulation::heart::ending`). Each seat plays its own, over its own view:
//!
//! * **The awakening** (in the world): a title over the waking Star Drive while the camera
//!   pulls back and the light leaves the Heart up the cable (the world renderer draws the
//!   beam, `rendering/world/heart.rs`).
//! * **The ascent**: the planet in cutaway, its eight Descents as rings round the burning
//!   core; the light climbs the builders' cable ring by ring (each Descent named as it passes),
//!   leaves the ground and crosses the sky to the Moon, which lights up.
//! * **The credits**: they roll over the stars, the lit cable rising to the Moon beside them.
//! * **The epilogue**: a card, then back to play.
//!
//! Nothing strobes: every change is a slow fade or a steady climb, and the stars twinkle gently
//! in brightness only.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "presentation maths on small bounded values; each screen reads top to bottom"
)]

use fatherload_simulation::{
    descent::Descent,
    heart::ending::{
        ASCENT_END_MILLIS, AWAKENING_MILLIS, CREDITS, CreditLine, EPILOGUE, EPILOGUE_TITLE,
        EndingPhase, beam_climb,
    },
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::ViewSnapshot;

/// The fraction of the cable's climb where it leaves the ground (the rest is the sky).
const GROUND_SHARE: f32 = 0.82;

const fn colour(rgb: fatherload_simulation::descent::Rgb) -> Color {
    Color::new(rgb.0, rgb.1, rgb.2, 255)
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color::new(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
}

/// A deterministic starfield over `rect`, twinkling gently (brightness only, slow).
fn stars(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, seconds: f32, count: u32) {
    for index in 0..count {
        let hash = index.wrapping_mul(2_654_435_761) ^ 0x9E37;
        let fx = (hash % 997) as f32 / 997.0;
        let fy = ((hash / 997) % 991) as f32 / 991.0;
        let x = rect.x + fx * rect.width;
        let y = rect.y + fy * rect.height;
        let twinkle = (seconds * 0.6 + index as f32 * 1.7)
            .sin()
            .mul_add(0.25, 0.75);
        let bright = if index % 7 == 0 { BONE } else { SKY_LIGHT };
        let size = if index % 13 == 0 { 2 } else { 1 };
        draw.draw_rectangle(
            x as i32,
            y as i32,
            size,
            size,
            with_alpha(bright, (twinkle * 220.0) as u8),
        );
    }
}

/// A glowing line from `a` to `b` (layered widths).
fn beam(draw: &mut RaylibDrawHandle<'_>, a: Vector2, b: Vector2, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    draw.draw_line_ex(a, b, 9.0, with_alpha(GOLD, (alpha * 50.0) as u8));
    draw.draw_line_ex(a, b, 5.0, with_alpha(GOLD, (alpha * 120.0) as u8));
    draw.draw_line_ex(a, b, 2.0, with_alpha(CREAM, (alpha * 255.0) as u8));
}

/// The point `t` (`0..=1`) along the polyline `points`.
fn along(points: &[Vector2], t: f32) -> (Vector2, usize) {
    let lengths: Vec<f32> = points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .collect();
    let total: f32 = lengths.iter().sum();
    let mut left = t.clamp(0.0, 1.0) * total;
    for (index, length) in lengths.iter().enumerate() {
        if left <= *length || index + 1 == lengths.len() {
            let f = if *length > 0.0 { left / length } else { 0.0 };
            let (a, b) = (points[index], points[index + 1]);
            return (
                Vector2::new(f.mul_add(b.x - a.x, a.x), f.mul_add(b.y - a.y, a.y)),
                index,
            );
        }
        left -= length;
    }
    (points[points.len() - 1], points.len() - 2)
}

/// The skip hint in the corner.
fn hint(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, text: &str) {
    let width = ui_kit::text_width(text, TextStyle::Body);
    ui_kit::text(
        draw,
        text,
        (rect.x + rect.width) as i32 - width - 10,
        (rect.y + rect.height) as i32 - 16,
        TextStyle::Body,
        with_alpha(FOG, 200),
    );
}

/// The awakening's title over the world.
fn draw_awakening(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, elapsed: u64) {
    let seconds = elapsed as f32 / 1_000.0;
    let total = AWAKENING_MILLIS as f32 / 1_000.0;
    let alpha = ((seconds - 0.8) / 1.0).clamp(0.0, 1.0) * ((total - seconds) / 0.8).clamp(0.0, 1.0);
    let a = (alpha * 255.0) as u8;
    let centre = (rect.x + rect.width * 0.5) as i32;
    let y = (rect.y + rect.height * 0.56) as i32;
    // A soft dark band behind the words, so the burning chamber never swallows them.
    draw.draw_rectangle(
        rect.x as i32,
        y - 12,
        rect.width as i32,
        58,
        with_alpha(INK, (alpha * 150.0) as u8),
    );
    ui_kit::text_centred(
        draw,
        "THE HEART WAKES",
        centre,
        y,
        TextStyle::Title,
        with_alpha(CREAM, a),
    );
    ui_kit::text_centred(
        draw,
        "The Star Drive turns over. The cable fills with light.",
        centre,
        y + 24,
        TextStyle::Body,
        with_alpha(GOLD, a),
    );
}

/// The ascent: the planet in cutaway and the light climbing the cable to the Moon.
fn draw_ascent(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, elapsed: u64, seconds: f32) {
    // Space at the top, the core's warmth rising from the bottom.
    draw.draw_rectangle_gradient_v(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
        Color::new(4, 4, 14, 255),
        Color::new(28, 14, 30, 255),
    );
    stars(
        draw,
        Rectangle::new(rect.x, rect.y, rect.width, rect.height * 0.55),
        seconds,
        120,
    );
    let radius = rect.height * 0.62;
    let centre = Vector2::new(
        rect.x + rect.width * 0.38,
        rect.y + rect.height + radius * 0.18,
    );
    // The planet's eight Descents as rings, outermost (the Crust) first.
    let surface = radius;
    let ring = surface / 9.0;
    for (index, descent) in Descent::ALL.iter().enumerate() {
        let outer = surface - ring * index as f32;
        let rock = colour(descent.first_stratum().def().palette.rock);
        draw.draw_circle_v(centre, outer, rock);
        draw.draw_circle_lines(
            centre.x as i32,
            centre.y as i32,
            outer,
            with_alpha(INK, 160),
        );
    }
    // The grass of the surface and the town on it.
    draw.draw_circle_lines(centre.x as i32, centre.y as i32, surface, LEAF);
    draw.draw_circle_lines(centre.x as i32, centre.y as i32, surface + 1.0, MOSS);
    // The Heart: a burning core breathing slowly.
    let breath = (seconds * 1.2).sin().mul_add(0.08, 1.0);
    let core = ring * 0.9 * breath;
    draw.draw_circle_v(centre, core * 2.2, with_alpha(GOLD, 50));
    draw.draw_circle_v(centre, core * 1.4, with_alpha(AMBER, 160));
    draw.draw_circle_v(centre, core, CREAM);
    // The Moon, high over the far side.
    let moon = Vector2::new(rect.x + rect.width * 0.8, rect.y + rect.height * 0.16);
    let moon_radius = rect.height * 0.07;
    let ground = Vector2::new(centre.x, centre.y - surface);
    let dock = Vector2::new(moon.x - moon_radius * 0.3, moon.y + moon_radius * 0.9);
    let path = [centre, ground, dock];
    // The climb: through the ground first, then the sky.
    let climb = beam_climb(elapsed);
    let lit = if climb < GROUND_SHARE {
        climb / GROUND_SHARE * 0.5
    } else {
        ((climb - GROUND_SHARE) / (1.0 - GROUND_SHARE)).mul_add(0.5, 0.5)
    };
    // The whole cable faint, the lit part blazing.
    draw.draw_line_ex(centre, ground, 1.5, with_alpha(STEEL, 160));
    draw.draw_line_ex(ground, dock, 1.0, with_alpha(STEEL, 90));
    let (tip, segment) = along(&path, lit);
    if segment == 0 {
        beam(draw, centre, tip, 1.0);
    } else {
        beam(draw, centre, ground, 1.0);
        beam(draw, ground, tip, 1.0);
    }
    draw.draw_circle_v(tip, 5.0, with_alpha(CREAM, 220));
    draw.draw_circle_v(tip, 10.0, with_alpha(GOLD, 70));
    // The Moon lights up when the light arrives.
    let arrived = ((climb - 0.97) / 0.03).clamp(0.0, 1.0);
    draw.draw_circle_v(
        moon,
        moon_radius * (1.0 + arrived * 0.8),
        with_alpha(GOLD, (arrived * 60.0) as u8),
    );
    draw.draw_circle_v(moon, moon_radius, lerp(SLATE, BONE, arrived));
    draw.draw_circle_v(
        Vector2::new(moon.x - moon_radius * 0.3, moon.y - moon_radius * 0.2),
        moon_radius * 0.22,
        lerp(SLATE_DARK, MIST, arrived),
    );
    // The Descent the light is passing, named (depth 0 at the surface, 1 at the core).
    let depth = if climb < GROUND_SHARE {
        1.0 - climb / GROUND_SHARE
    } else {
        0.0
    };
    let passing = Descent::ALL[((depth * 8.0) as usize).min(7)];
    let label = if climb >= 0.97 {
        "THE MOON"
    } else if climb >= GROUND_SHARE {
        "UP INTO THE SKY"
    } else if depth > 0.94 {
        "THE STAR DRIVE"
    } else {
        passing.name()
    };
    let numeral = if climb < GROUND_SHARE && depth <= 0.94 {
        passing.numeral()
    } else {
        ""
    };
    let x = (rect.x + rect.width * 0.62) as i32;
    let y = (rect.y + rect.height * 0.62) as i32;
    if !numeral.is_empty() {
        ui_kit::text(
            draw,
            &format!("DESCENT {numeral}"),
            x,
            y,
            TextStyle::Bold,
            GOLD,
        );
    } else if climb >= 0.97 {
        ui_kit::text(draw, "THE BUILDERS' DOCK", x, y, TextStyle::Bold, GOLD);
    }
    ui_kit::text(draw, label, x, y + 12, TextStyle::Title, CREAM);
    ui_kit::text(
        draw,
        "The cable to the Moon lights up",
        (rect.x + 12.0) as i32,
        (rect.y + 10.0) as i32,
        TextStyle::Bold,
        with_alpha(SKY_LIGHT, 220),
    );
    let _ = ASCENT_END_MILLIS;
}

/// The credits rolling over the stars.
fn draw_credits(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, progress: f32, seconds: f32) {
    draw.draw_rectangle(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
        Color::new(4, 4, 14, 255),
    );
    stars(draw, rect, seconds, 220);
    // The lit cable rising to the Moon on the left.
    let moon = Vector2::new(rect.x + rect.width * 0.14, rect.y + rect.height * 0.14);
    let moon_radius = rect.height * 0.06;
    draw.draw_circle_v(moon, moon_radius * 1.6, with_alpha(GOLD, 40));
    draw.draw_circle_v(moon, moon_radius, BONE);
    draw.draw_circle_v(
        Vector2::new(moon.x - moon_radius * 0.3, moon.y - moon_radius * 0.2),
        moon_radius * 0.22,
        MIST,
    );
    beam(
        draw,
        Vector2::new(rect.x + rect.width * 0.08, rect.y + rect.height),
        Vector2::new(moon.x, moon.y + moon_radius),
        0.9,
    );
    // The roll: every line in order, rising from the bottom to past the top.
    let column = (rect.x + rect.width * 0.58) as i32;
    let gap = 16;
    let lines = CREDITS.len() as i32;
    let total = lines * gap + rect.height as i32;
    let offset = (progress * total as f32) as i32;
    let mut y = (rect.y + rect.height) as i32 - offset;
    for line in CREDITS {
        let fade = |y: i32| {
            let top = (y as f32 - rect.y) / 30.0;
            let bottom = (rect.y + rect.height - y as f32) / 30.0;
            (top.min(bottom).clamp(0.0, 1.0) * 255.0) as u8
        };
        match line {
            CreditLine::Title(text) => {
                ui_kit::text_centred(
                    draw,
                    text,
                    column,
                    y,
                    TextStyle::Title,
                    with_alpha(GOLD, fade(y)),
                );
                y += gap * 2;
            }
            CreditLine::Heading(text) => {
                ui_kit::text_centred(
                    draw,
                    text,
                    column,
                    y,
                    TextStyle::Bold,
                    with_alpha(AMBER, fade(y)),
                );
                y += gap;
            }
            CreditLine::Name(text) => {
                ui_kit::text_centred(
                    draw,
                    text,
                    column,
                    y,
                    TextStyle::Body,
                    with_alpha(CREAM, fade(y)),
                );
                y += gap;
            }
            CreditLine::Gap => y += gap,
        }
    }
}

/// The epilogue's card.
fn draw_epilogue(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, since: f32, seconds: f32) {
    draw.draw_rectangle(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
        Color::new(4, 4, 14, 230),
    );
    stars(draw, rect, seconds, 90);
    let alpha = (since / 1.2).clamp(0.0, 1.0);
    let a = (alpha * 255.0) as u8;
    let width = (rect.width as i32 - 40).min(380);
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let text_width = width - 2 * ui_kit::PANEL_PADDING - 8;
    let height = 44
        + ui_kit::wrap(EPILOGUE, text_width, TextStyle::Body).len() as i32
            * TextStyle::Body.line_height();
    let y = rect.y as i32 + (rect.height as i32 - height) / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    ui_kit::text_centred(
        draw,
        EPILOGUE_TITLE,
        x + width / 2,
        y + 8,
        TextStyle::Title,
        with_alpha(GOLD, a),
    );
    ui_kit::paragraph(
        draw,
        EPILOGUE,
        x + ui_kit::PANEL_PADDING + 4,
        y + 36,
        text_width,
        TextStyle::Body,
        with_alpha(CREAM, a),
    );
}

/// The seat's ending screen over its view (nothing once it is done). `skip` names the seat's
/// skip button.
pub(super) fn draw_ending(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    skip: &str,
) {
    let Some(ending) = view.ending else {
        return;
    };
    let seconds = ui_kit::seconds();
    match ending.phase {
        EndingPhase::Awakening => {
            draw_awakening(draw, rect, ending.elapsed);
            hint(draw, rect, &format!("{skip}: skip"));
        }
        EndingPhase::Ascent => {
            draw_ascent(draw, rect, ending.elapsed, seconds);
            hint(draw, rect, &format!("{skip}: skip"));
        }
        EndingPhase::Credits => {
            draw_credits(
                draw,
                rect,
                ending.view.credits_progress(ending.elapsed),
                seconds,
            );
            hint(draw, rect, &format!("{skip}: skip"));
        }
        EndingPhase::Epilogue => {
            let since = ending
                .elapsed
                .saturating_sub(ending.view.phase_start(ending.elapsed))
                as f32
                / 1_000.0;
            draw_epilogue(draw, rect, since, seconds);
            hint(draw, rect, &format!("{skip}: continue"));
        }
        EndingPhase::Done => {}
    }
}
