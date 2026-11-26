#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "screen geometry and UI art pixels from bounded readings"
)]

//! A body's life on screen (Wave 12 bodylife): its weather over the seat's view (rain, snow,
//! grit, ash, spores, the aurora's ribbons, the storm's gloom and fog), oriented to the body's
//! local down whatever way the craft is turned, and the body badge in the clock's place on the
//! HUD (the body's name, its local hour and day side, temperature, air, gravity and weather).
//!
//! Everything is drawn from the seat's [`BodyLifeView`] (a pure reading of the simulation), so
//! each seat on each body sees its own sky. Particles are a pure function of time and the
//! camera, scaled by the screen-effects setting (fewer at Reduced, none at Off), and nothing
//! flashes. Plain raylib shapes only (no shader), so the browser build draws them cheaply.

use raylib::prelude::*;

use fatherload_simulation::{
    bodylife::weather::{WeatherParticles, weather_kind},
    space::Atmosphere,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::{render_snapshot::BodyLifeView, screen_fx::EffectsIntensity};

/// Most particles a full-strength spell draws on a seat at full effects.
const MAX_PARTICLES: usize = 420;

/// A splitmix step for particle seeds.
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A unit value from `seed`.
fn unit(seed: u64) -> f32 {
    (mix(seed) >> 40) as f32 / (1_u64 << 24) as f32
}

/// How much of the weather's motion the setting lets through.
const fn particle_share(effects: EffectsIntensity) -> f32 {
    match effects {
        EffectsIntensity::Full => 1.0,
        EffectsIntensity::Reduced => 0.45,
        EffectsIntensity::Off => 0.0,
    }
}

fn with_alpha(colour: [u8; 3], alpha: f32) -> Color {
    Color::new(
        colour[0],
        colour[1],
        colour[2],
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// The seat's weather over its view `rect` (physical pixels). `roll` is the craft's turn from
/// the local up (radians, clockwise: the ground's down is turned the other way on screen);
/// `camera` the seat's strip position in tiles (particles stay put in the world as the craft
/// moves); `px_per_tile` the zoom.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "one seat's weather: each particle kind in turn"
)]
pub fn draw_weather(
    draw: &mut RaylibDrawHandle<'_>,
    life: &BodyLifeView,
    rect: Rectangle,
    roll: f32,
    camera: (f64, f64),
    px_per_tile: f32,
    seconds: f32,
    effects: EffectsIntensity,
) {
    let weather = life.reading.weather;
    if weather.is_clear() {
        return;
    }
    let kind = weather_kind(weather.kind);
    let strength = weather.intensity();
    let mut scissor = draw.begin_scissor_mode(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
    );
    let draw = &mut scissor;
    // The local frame on screen: down and along (rising columns) turned by the craft's roll.
    let (sin, cos) = roll.sin_cos();
    let down = (sin, cos);
    let along = (cos, -sin);
    let centre = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
    let to_screen = |x: f32, y: f32| -> Vector2 {
        Vector2::new(
            centre.0 + along.0 * x + down.0 * y,
            centre.1 + along.1 * x + down.1 * y,
        )
    };
    // The storm's gloom, before the particles.
    if weather.light_permille < 0 {
        let gloom = f32::from(-weather.light_permille) / 1_000.0 * 0.55;
        draw.draw_rectangle_rec(rect, Color::new(8, 10, 22, (gloom * 255.0) as u8));
    }
    let share = particle_share(effects);
    let span = rect.width.hypot(rect.height) * 0.5 + 24.0;
    let wind_px = weather.wind / fatherload_simulation::TILE_SIZE * px_per_tile;
    // The camera's place in the world, so the weather does not ride along with the craft.
    let scroll = (
        (camera.0 * f64::from(px_per_tile)).rem_euclid(f64::from(span * 2.0)) as f32,
        (camera.1 * f64::from(px_per_tile)).rem_euclid(f64::from(span * 2.0)) as f32,
    );
    let count = (MAX_PARTICLES as f32 * strength * share) as usize;
    let wrap = |value: f32| (value + span).rem_euclid(span * 2.0) - span;
    match kind.look.particles {
        WeatherParticles::None => {}
        WeatherParticles::Rain => {
            for index in 0..count {
                let seed = index as u64 * 31 + 7;
                let fall = 22.0 * px_per_tile * (0.8 + unit(seed + 2) * 0.4);
                let x = wrap(unit(seed) * span * 2.0 + wind_px * seconds - scroll.0);
                let y = wrap(unit(seed + 1) * span * 2.0 + fall * seconds - scroll.1);
                let length = (0.45 + unit(seed + 3) * 0.35) * px_per_tile;
                let tilt = wind_px / fall.max(1.0);
                let start = to_screen(x, y);
                let end = to_screen(x + tilt * length, y + length);
                draw.draw_line_ex(start, end, 1.4, with_alpha(kind.look.colour, 0.62));
            }
        }
        WeatherParticles::Snow | WeatherParticles::Ash | WeatherParticles::Spores => {
            let (fall, wobble, size) = match kind.look.particles {
                WeatherParticles::Snow => (2.2, 0.6, 0.12),
                WeatherParticles::Ash => (1.4, 0.35, 0.1),
                _ => (0.25, 0.9, 0.09),
            };
            for index in 0..count {
                let seed = index as u64 * 53 + 11;
                let phase = unit(seed + 4) * std::f32::consts::TAU;
                let drift = (seconds * 0.9 + phase).sin() * wobble * px_per_tile;
                let x = wrap(unit(seed) * span * 2.0 + wind_px * 0.8 * seconds + drift - scroll.0);
                let y = wrap(
                    unit(seed + 1) * span * 2.0
                        + fall * px_per_tile * seconds * (0.7 + unit(seed + 2) * 0.6)
                        - scroll.1,
                );
                let point = to_screen(x, y);
                let radius = (size * px_per_tile * (0.6 + unit(seed + 3) * 0.8)).max(1.0);
                draw.draw_circle_v(point, radius, with_alpha(kind.look.colour, 0.85));
            }
        }
        WeatherParticles::Dust => {
            for index in 0..count {
                let seed = index as u64 * 71 + 5;
                let x = wrap(unit(seed) * span * 2.0 + wind_px * 1.4 * seconds - scroll.0);
                let y = wrap(unit(seed + 1) * span * 2.0 + px_per_tile * 0.4 * seconds - scroll.1);
                let streak = 0.3 * px_per_tile * wind_px.signum();
                draw.draw_line_ex(
                    to_screen(x, y),
                    to_screen(x + streak, y),
                    1.5,
                    with_alpha(kind.look.colour, 0.55),
                );
            }
        }
        WeatherParticles::Aurora => {
            draw_aurora(
                draw,
                kind.look.sky_glow.unwrap_or(kind.look.colour),
                strength,
                share,
                &to_screen,
                (span, rect.height, px_per_tile, seconds),
                camera,
            );
        }
    }
    // Fog: the view closes in toward the fog's colour.
    let fog = f32::from(1_000 - weather.visibility_permille.min(1_000)) / 1_000.0;
    if fog > 0.0 {
        let veil = fog * if share > 0.0 { 0.55 } else { 0.35 };
        draw.draw_rectangle_rec(rect, with_alpha(kind.look.fog, veil * 0.6));
        // Thicker toward the edges: four soft bands.
        let band = rect.width.min(rect.height) * 0.18;
        let edge = with_alpha(kind.look.fog, veil * 0.5);
        let clear = with_alpha(kind.look.fog, 0.0);
        let (left, top) = (rect.x as i32, rect.y as i32);
        let (width, height) = (rect.width as i32, rect.height as i32);
        let band = band as i32;
        draw.draw_rectangle_gradient_v(left, top, width, band, edge, clear);
        draw.draw_rectangle_gradient_v(left, top + height - band, width, band, clear, edge);
        draw.draw_rectangle_gradient_h(left, top, band, height, edge, clear);
        draw.draw_rectangle_gradient_h(left + width - band, top, band, height, clear, edge);
    }
}

/// The aurora: curtains of light hanging over the horizon, swaying slowly (no flicker), drawn
/// additively so they glow over the night sky. Heights are shares of the view, so the
/// curtains hang in the upper sky at any zoom.
#[allow(clippy::too_many_arguments, reason = "one ribbon set's inputs")]
fn draw_aurora(
    draw: &mut impl RaylibDraw,
    glow: [u8; 3],
    strength: f32,
    share: f32,
    to_screen: &impl Fn(f32, f32) -> Vector2,
    (span, height, px_per_tile, seconds): (f32, f32, f32, f32),
    camera: (f64, f64),
) {
    let sway = if share > 0.0 { 1.0 } else { 0.0 };
    let violet = [
        glow[0].saturating_add(90),
        glow[1].saturating_sub(120),
        glow[2].saturating_add(80),
    ];
    let ribbons: [([u8; 3], f32, f32, f32); 3] = [
        (glow, 0.16, 0.0, 1.0),
        (violet, 0.27, 1.7, 0.7),
        ([90, 200, 255], 0.36, 3.1, 0.55),
    ];
    let columns = 96;
    let width = span * 2.0 / columns as f32 + 1.5;
    super::space::additive(|| {
        for (colour, lift, phase, weight) in ribbons {
            let hem = -lift * height;
            for step in 0..columns {
                let t = step as f32 / columns as f32;
                let x = (t * 2.0 - 1.0) * span;
                // Fixed to the ground (a little parallax), swaying slowly.
                let along = x / px_per_tile.max(1.0) + camera.0 as f32 * 0.4;
                let wave = (along * 0.09 + phase + seconds * 0.1 * sway).sin() * 0.035
                    + (along * 0.031 + phase * 2.0).sin() * 0.05;
                let bottom = hem + wave * height;
                let fold = (along * 0.23 + phase).sin() * 0.5 + 0.5;
                let curtain = (0.1 + fold * 0.16) * height;
                let alpha = strength * weight * (0.2 + 0.25 * fold);
                // Bright at its lower hem, fading upward.
                let segments = 7;
                for segment in 0..segments {
                    let fade = 1.0 - segment as f32 / segments as f32;
                    let from = to_screen(x, bottom - curtain * segment as f32 / segments as f32);
                    let to =
                        to_screen(x, bottom - curtain * (segment + 1) as f32 / segments as f32);
                    draw.draw_line_ex(from, to, width, with_alpha(colour, alpha * fade * fade));
                }
            }
        }
    });
}

/// The air's word for the badge.
const fn air_word(life: &BodyLifeView) -> (&'static str, Color) {
    if life.reading.climate.breathable {
        return ("AIR OK", MINT);
    }
    match life.atmosphere {
        Atmosphere::Vacuum => ("NO AIR", ORANGE),
        Atmosphere::Thin => ("THIN AIR", ORANGE),
        Atmosphere::Toxic => ("TOXIC AIR", RED),
        Atmosphere::Spores => ("SPORE AIR", LIME),
        Atmosphere::Scalding => ("SCALDING", RED),
        Atmosphere::Breathable | Atmosphere::Thick => ("FOUL AIR", ORANGE),
    }
}

/// The body badge, in the clock badge's place (right-aligned at `top` in `rect`): the body's
/// name, its local hour (or its side, when tidally locked), temperature, air, gravity and the
/// weather. Returns where it was drawn.
pub fn draw_body_badge(
    draw: &mut RaylibDrawHandle<'_>,
    life: &BodyLifeView,
    rect: Rectangle,
    top: i32,
    avoid: &[Rectangle],
) -> Option<Rectangle> {
    let reading = &life.reading;
    let (hours, minutes) = reading.hours_minutes;
    let day_side = reading.elevation > 0.0;
    let time = if life.locked {
        if day_side {
            "DAY SIDE".to_owned()
        } else if reading.elevation > -0.12 {
            "TWILIGHT".to_owned()
        } else {
            "NIGHT SIDE".to_owned()
        }
    } else {
        format!("{hours:02}:{minutes:02} LOCAL")
    };
    let climate = format!(
        "{} C  {:.2} g",
        reading.climate.temperature_c,
        f32::from(life.gravity_milli) / 1_000.0
    );
    let (air, air_colour) = air_word(life);
    let weather = if reading.weather.is_clear() {
        None
    } else {
        let kind = weather_kind(reading.weather.kind);
        let wind = (reading.weather.wind / fatherload_simulation::TILE_SIZE).abs();
        Some(if wind >= 0.5 {
            let arrow = if reading.weather.wind > 0.0 { ">" } else { "<" };
            format!("{} {arrow}{wind:.0} t/s", kind.name.to_uppercase())
        } else {
            kind.name.to_uppercase()
        })
    };
    let name = life.name.to_uppercase();
    let lines = [
        ui_kit::text_width(&name, TextStyle::Bold),
        ui_kit::text_width(&time, TextStyle::Body),
        ui_kit::text_width(&climate, TextStyle::Body),
        ui_kit::text_width(air, TextStyle::Body),
        weather
            .as_ref()
            .map_or(0, |line| ui_kit::text_width(line, TextStyle::Body)),
    ];
    let dial = 16;
    let width = ui_kit::PANEL_PADDING * 2 + dial + 5 + lines.iter().copied().max().unwrap_or(0);
    let height = if weather.is_some() { 56 } else { 46 };
    if top + height > (rect.y + rect.height) as i32 - 8 {
        return None;
    }
    let badge = super::screen::badge_rect(rect, top, (width, height), avoid);
    ui_kit::panel(draw, badge, PanelStyle::Standard);
    let x = badge.x as i32 + ui_kit::PANEL_PADDING;
    // A little disc: the body, lit on the star's side.
    let cx = x + dial / 2;
    let cy = top + 12;
    draw.draw_circle(cx, cy, 6.0, SLATE_DARK);
    let lit = if day_side { GOLD } else { LAVENDER };
    draw.draw_circle_sector(
        Vector2::new(cx as f32, cy as f32),
        6.0,
        if day_side { -90.0 } else { 90.0 },
        if day_side { 90.0 } else { 270.0 },
        8,
        lit,
    );
    draw.draw_circle_lines(cx, cy, 6.0, INK);
    let text_x = x + dial + 5;
    let mut y = top + 5;
    ui_kit::text(draw, &name, text_x, y, TextStyle::Bold, GOLD);
    y += 10;
    let time_colour = if day_side { MIST } else { LAVENDER };
    ui_kit::text(draw, &time, text_x, y, TextStyle::Body, time_colour);
    y += 10;
    let heat = reading.climate.heat_units > 0;
    let cold = reading.climate.cold > 0;
    let climate_colour = if heat {
        ORANGE
    } else if cold {
        ICE
    } else {
        MIST
    };
    ui_kit::text(draw, &climate, text_x, y, TextStyle::Body, climate_colour);
    y += 10;
    ui_kit::text(draw, air, text_x, y, TextStyle::Body, air_colour);
    y += 10;
    if let Some(line) = &weather {
        ui_kit::text(draw, line, text_x, y, TextStyle::Body, SKY_LIGHT);
    }
    Some(badge)
}
