//! A frontier world's HUD and weather (Wave 12 planets-b): a seat on a dune, volcanic or
//! crystal world sees its storm over the view (a sandstorm's blowing grit, an ash fall's
//! drifting flakes, a glitterfrost's slow sparkle; all of it toned down or off with the screen
//! effects setting, and nothing that flashes) and reads, at the foot of the view, where it is
//! (the world and its stratum), the stratum's rule, the hazards biting it, a storm coming and
//! a volcano rumbling.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "HUD layout maps small values to pixels"
)]

use fatherload_simulation::{
    frontier::{
        FrontierStyle, PlanetHazard, planet_gear_tier,
        rules::{self, EruptionPhase, WeatherPhase, Zone},
        style_of_ground,
    },
    planet::BodyGround,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, TextStyle};
use crate::{
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    screen_fx::EffectsIntensity,
};

/// `colour` at opacity `alpha` (0-1).
fn fade(colour: Color, alpha: f32) -> Color {
    with_alpha(colour, (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// What a seat reads on a frontier world.
struct Reading {
    style: FrontierStyle,
    world: &'static str,
    zone: Zone,
    place: &'static str,
    hint: Option<&'static str>,
    weather: rules::Weather,
    /// Biting hazards the rig's gear does not cover.
    bites: Vec<PlanetHazard>,
    rumbling: bool,
    /// Under open sky (the weather reaches it).
    open: bool,
}

fn reading(snapshot: &RenderSnapshot<'_>, view: &ViewSnapshot<'_>) -> Option<Reading> {
    let scene = view.place_scene.as_ref()?;
    let ground = scene.terrain.planet_ground()?;
    let style = style_of_ground(ground)?;
    let millis = snapshot.tick.saturating_mul(1_000) / u64::from(snapshot.tick_rate.hz().max(1));
    let tile = scene.frame.tile_at(view.player.x, view.player.y);
    let zone = rules::zone_at(ground, tile);
    let weather = rules::weather_at(style, ground.params().seed, millis);
    let exposure = rules::exposure_at(ground, tile, weather);
    let gear = view.player.hazard_gear;
    let bites = exposure
        .hazards()
        .into_iter()
        .filter(|hazard| {
            planet_gear_tier(&gear, *hazard) < fatherload_simulation::frontier::PLANET_GEAR_MAX_TIER
        })
        .collect();
    let rumbling = style == FrontierStyle::Volcanic
        && rules::cones_near(ground, i64::from(tile.x), 60)
            .into_iter()
            .any(|cone| {
                rules::eruption_at(ground.params().seed, cone, millis).0 != EruptionPhase::Quiet
            });
    let theme = ground.theme();
    let place = match zone {
        Zone::Surface => "the surface",
        Zone::Stratum(index) => theme.strata[usize::from(index.min(3))].name,
        Zone::CoreChamber => "the core chamber",
    };
    Some(Reading {
        style,
        world: theme.name,
        zone,
        place,
        hint: rules::rule_at(ground, style, tile)
            .map(fatherload_simulation::frontier::StratumRule::hint),
        weather,
        bites,
        rumbling,
        open: tile.y < ground.surface_row(tile.x) + 1,
    })
}

/// The storm over the view, then the seat's line of what it reads.
pub fn draw_frontier_hud(
    draw: &mut impl RaylibDraw,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let Some(read) = reading(snapshot, view) else {
        return;
    };
    let seconds = snapshot.tick as f32 / snapshot.tick_rate.hz().max(1) as f32;
    if read.open {
        draw_weather(draw, &read, snapshot.effects, rect, seconds);
    }
    let mut lines: Vec<(String, Color)> = Vec::new();
    lines.push((format!("{} - {}", read.world, read.place), MIST));
    if let Some(hint) = read.hint {
        lines.push((hint.to_owned(), FOG));
    }
    let storm = rules::storm_name(read.style);
    match read.weather.phase {
        WeatherPhase::Rising => lines.push((
            format!(
                "A {storm} is coming ({} s)",
                read.weather.remaining_millis / 1_000
            ),
            AMBER,
        )),
        WeatherPhase::Storm => {
            let mut name = storm.to_owned();
            if let Some(first) = name.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            lines.push((name, AMBER));
        }
        WeatherPhase::Calm => {}
    }
    if read.rumbling {
        lines.push((
            "The ground rumbles: a cone will erupt. Watch for shadows!".to_owned(),
            RED,
        ));
    }
    for hazard in &read.bites {
        let text = match hazard {
            PlanetHazard::ToxicAir => "TOXIC AIR: the hull corrodes (gas scrubber, planet gear)",
            PlanetHazard::Cold => "BITTER COLD: the hull frosts (de-icer, planet gear)",
            PlanetHazard::HighGravity => "HEAVY PULL: climbing costs more (planet gear)",
            PlanetHazard::Heat => "HEAT (radiator, heat shield, planet gear)",
        };
        lines.push((text.to_owned(), RED));
    }
    let line_height = 10;
    let bottom = rect.y as i32 + rect.height as i32 - 6;
    let top = bottom - line_height * lines.len() as i32;
    for (index, (text, colour)) in lines.iter().enumerate() {
        let width = ui_kit::text_width(text, TextStyle::Body);
        let x = rect.x as i32 + (rect.width as i32 - width) / 2;
        let y = top + index as i32 * line_height;
        // A dark backing so the line reads over bright sand, grey basalt or glowing magma.
        draw.draw_rectangle(x - 4, y - 1, width + 8, line_height, fade(INK, 0.8));
        ui_kit::text(
            draw,
            text,
            x,
            top + index as i32 * line_height,
            TextStyle::Body,
            *colour,
        );
    }
    let _ = read.zone;
}

/// A deterministic unit value for particle `index` and `salt`.
fn unit(index: u32, salt: u32) -> f32 {
    let mut h = index.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// The weather's veil and particles over the view (the effects setting scales them; "off"
/// leaves only a faint static tint so the storm still reads).
fn draw_weather(
    draw: &mut impl RaylibDraw,
    read: &Reading,
    effects: EffectsIntensity,
    rect: Rectangle,
    seconds: f32,
) {
    let strength = match read.weather.phase {
        WeatherPhase::Calm => return,
        WeatherPhase::Rising => {
            0.25 * (1.0 - read.weather.remaining_millis as f32 / rules::RISING_MILLIS as f32)
        }
        WeatherPhase::Storm => f32::from(read.weather.strength_permille) / 1_000.0,
    };
    let (veil, mote, count) = match read.style {
        FrontierStyle::Desert => (SAND, CREAM, 70),
        FrontierStyle::Volcanic => (SLATE, FOG, 50),
        FrontierStyle::Crystal => (SKY_LIGHT, BONE, 36),
    };
    let scale: f32 = match effects {
        EffectsIntensity::Full => 1.0,
        EffectsIntensity::Reduced => 0.5,
        EffectsIntensity::Off => 0.0,
    };
    // The veil: a steady tint (never pulsing), thicker in the storm.
    let veil_alpha = (0.42 * scale).max(0.08) * strength;
    draw.draw_rectangle_rec(rect, fade(veil, veil_alpha));
    if scale <= 0.0 {
        return;
    }
    let wind = f32::from(read.weather.wind);
    let motes = (count as f32 * scale * strength) as u32;
    for index in 0..motes {
        let (u, v) = (unit(index, 1), unit(index, 2));
        let speed = 0.4 + 0.6 * unit(index, 3);
        match read.style {
            FrontierStyle::Desert => {
                // Grit streaking along with the wind.
                let x = rect.x + ((u + wind * seconds * speed * 0.9).rem_euclid(1.0)) * rect.width;
                let y = rect.y + ((v + seconds * 0.05).rem_euclid(1.0)) * rect.height;
                let length = 6.0 + 10.0 * speed;
                draw.draw_line_v(
                    Vector2::new(x, y),
                    Vector2::new(x - wind * length, y - 1.0),
                    fade(mote, 0.5 * strength),
                );
            }
            FrontierStyle::Volcanic => {
                // Ash drifting down, swaying.
                let x = rect.x
                    + ((u + (seconds * 0.7 + v * 6.0).sin() * 0.01).rem_euclid(1.0)) * rect.width;
                let y = rect.y + ((v + seconds * 0.06 * speed).rem_euclid(1.0)) * rect.height;
                draw.draw_rectangle_rec(Rectangle::new(x, y, 2.0, 2.0), fade(mote, 0.7 * strength));
            }
            FrontierStyle::Crystal => {
                // Frost glitter: each mote brightens and fades slowly (well under 3 Hz), never
                // in step with the rest.
                let x = rect.x + u * rect.width;
                let y = rect.y + ((v + seconds * 0.01).rem_euclid(1.0)) * rect.height;
                let glow = 0.5 + 0.5 * (seconds * (0.6 + speed) + u * 40.0).sin();
                draw.draw_rectangle_rec(
                    Rectangle::new(x, y, 1.0, 1.0),
                    fade(mote, glow * 0.8 * strength),
                );
            }
        }
    }
}
