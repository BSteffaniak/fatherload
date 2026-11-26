//! An exotic world's HUD (Wave 13 planets-d): a seat on a gas giant, a tidally locked world or
//! a ring world reads, at the foot of the view, where it is (the world, its band or stratum
//! and, on the locked world, the side), the rule there, the hazards biting it without the
//! gear, the gas giant's pressure gauge on the way down, and a quake rumbling; and, while a
//! theme boss is awake, its name, health and what it is winding up at the top of the view.
//! Nothing flashes: the boss's tell line is steady, its weak point's "OPEN" a fixed colour.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "HUD layout maps small values to pixels"
)]

use fatherload_simulation::{
    exotic::{
        ExoticStyle,
        boss::BossAttack,
        gas_giant,
        locked::{self, Side},
        rules::{self, Zone},
        style_of_ground,
    },
    frontier::{PLANET_GEAR_MAX_TIER, PlanetHazard, planet_gear_tier},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// `colour` at opacity `alpha` (0-1).
fn fade(colour: Color, alpha: f32) -> Color {
    with_alpha(colour, (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// The seat's lines (text and colour).
type Lines = Vec<(String, Color)>;

/// The seat's lines and the gauge (0-1) of the gas giant's pressure.
fn lines(snapshot: &RenderSnapshot<'_>, view: &ViewSnapshot<'_>) -> Option<(Lines, Option<f32>)> {
    let scene = view.place_scene.as_ref()?;
    let ground = scene.terrain.planet_ground()?;
    let style = style_of_ground(ground)?;
    let millis = snapshot.tick.saturating_mul(1_000) / u64::from(snapshot.tick_rate.hz().max(1));
    let tile = scene.frame.tile_at(view.player.x, view.player.y);
    let zone = rules::zone_at(ground, style, tile);
    let theme = ground.theme();
    let place = match zone {
        Zone::Surface => match style {
            ExoticStyle::GasGiant => "the cloud tops",
            _ => "the surface",
        },
        Zone::Skyring => "the Skyring",
        Zone::Stratum(index) => theme.strata[usize::from(index.min(3))].name,
        Zone::CoreChamber => "the core chamber",
    };
    let side = match (style, locked::side_of(ground, i64::from(tile.x))) {
        (ExoticStyle::Locked, Side::Day) => " (day side)",
        (ExoticStyle::Locked, Side::Twilight) => " (twilight)",
        (ExoticStyle::Locked, Side::Night) => " (night side)",
        _ => "",
    };
    let mut out = vec![(format!("{} - {place}{side}", theme.name), MIST)];
    if let Some(rule) = rules::rule_at(ground, style, tile) {
        out.push((rule.hint().to_owned(), FOG));
    }
    let gear = view.player.hazard_gear;
    let exposure = rules::exposure_at(ground, tile);
    for hazard in exposure.hazards() {
        if planet_gear_tier(&gear, hazard) >= PLANET_GEAR_MAX_TIER {
            continue;
        }
        let text = match (hazard, exposure.pressure > 0.0) {
            (PlanetHazard::HighGravity, true) => {
                "CRUSHING PRESSURE: the hull buckles (gravity struts, planet gear)"
            }
            (PlanetHazard::HighGravity, false) => "HEAVY PULL: climbing costs more (planet gear)",
            (PlanetHazard::Heat, _) => "SCORCHING SUN: the hull burns (heat shielding, or shade)",
            (PlanetHazard::Cold, _) => "DEEP COLD: the hull freezes (cold insulation)",
            (PlanetHazard::ToxicAir, _) => "TOXIC AIR (planet gear)",
        };
        out.push((text.to_owned(), RED));
    }
    let mut gauge = None;
    if style == ExoticStyle::GasGiant {
        let deep = rules::pressure_depth(ground, tile);
        let top = gas_giant::cloud_top(ground, i64::from(tile.x));
        let warn_from = gas_giant::band_top_row(ground, 2) - 120;
        if tile.y - top >= warn_from {
            gauge = Some(deep);
            if deep <= 0.0 {
                out.push((
                    "The pressure rises below: the Pressure Deep crushes bare hulls".to_owned(),
                    AMBER,
                ));
            }
        }
    }
    if style == ExoticStyle::Locked
        && zone == Zone::Stratum(2)
        && rules::quake_at(ground, millis) == Some(false)
    {
        out.push((
            "The ground rumbles: a tidal quake! Get out from under the rubble".to_owned(),
            RED,
        ));
    }
    Some((out, gauge))
}

/// The seat's lines at the foot of the view, the pressure gauge, and the boss's bar.
pub fn draw_exotic_hud(
    draw: &mut impl RaylibDraw,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let Some((lines, gauge)) = lines(snapshot, view) else {
        return;
    };
    let line_height = 10;
    let bottom = rect.y as i32 + rect.height as i32 - 6;
    let top = bottom - line_height * lines.len() as i32;
    for (index, (text, colour)) in lines.iter().enumerate() {
        let width = ui_kit::text_width(text, TextStyle::Body);
        let x = rect.x as i32 + (rect.width as i32 - width) / 2;
        let y = top + index as i32 * line_height;
        draw.draw_rectangle(x - 4, y - 1, width + 8, line_height, fade(INK, 0.55));
        ui_kit::text(draw, text, x, y, TextStyle::Body, *colour);
    }
    if let Some(level) = gauge {
        draw_gauge(draw, rect, level);
    }
    if let Some(scene) = view.place_scene.as_ref() {
        draw_boss_bar(draw, scene, rect);
    }
}

/// The pressure gauge down the view's right edge: green to amber to red as the crush grows.
fn draw_gauge(draw: &mut impl RaylibDraw, rect: Rectangle, level: f32) {
    let height = (rect.height * 0.4).max(40.0);
    let x = rect.x + rect.width - 14.0;
    let y = rect.y + (rect.height - height) * 0.5;
    draw.draw_rectangle_rec(
        Rectangle::new(x - 2.0, y - 2.0, 10.0, height + 4.0),
        fade(INK, 0.7),
    );
    let filled = height * level.clamp(0.0, 1.0);
    let colour = if level < 0.34 {
        LEAF
    } else if level < 0.67 {
        AMBER
    } else {
        RED
    };
    draw.draw_rectangle_rec(Rectangle::new(x, y + height - filled, 6.0, filled), colour);
    draw.draw_rectangle_lines_ex(
        Rectangle::new(x - 1.0, y - 1.0, 8.0, height + 2.0),
        1.0,
        FOG,
    );
    ui_kit::text(
        draw,
        "P",
        x as i32,
        (y - 11.0) as i32,
        TextStyle::Body,
        MIST,
    );
}

/// An awake boss's name, health and what it winds up, along the top of the view.
fn draw_boss_bar(
    draw: &mut impl RaylibDraw,
    scene: &crate::render_snapshot::PlaceScene<'_>,
    rect: Rectangle,
) {
    let Some(ground) = scene.terrain.planet_ground() else {
        return;
    };
    let Some(style) = style_of_ground(ground) else {
        return;
    };
    let Some(state) = scene
        .entities
        .iter()
        .find_map(|entity| match &entity.state {
            fatherload_simulation::entity::EntityState::Enemy(state)
                if state.kind == style.boss() =>
            {
                Some(*state)
            }
            _ => None,
        })
    else {
        return;
    };
    let name = state.kind.name();
    let width = (rect.width * 0.5).clamp(120.0, 320.0);
    let x = rect.x + (rect.width - width) * 0.5;
    let y = rect.y + 18.0;
    draw.draw_rectangle_rec(
        Rectangle::new(x - 4.0, y - 14.0, width + 8.0, 30.0),
        fade(INK, 0.6),
    );
    ui_kit::text(
        draw,
        name,
        x as i32,
        (y - 12.0) as i32,
        TextStyle::Body,
        GOLD,
    );
    draw.draw_rectangle_rec(Rectangle::new(x, y, width, 6.0), NIGHT);
    let health = state.health_fraction().clamp(0.0, 1.0);
    let exposed = state.volley == fatherload_simulation::world::BOSS_STEP_EXPOSED;
    draw.draw_rectangle_rec(
        Rectangle::new(x, y, width * health, 6.0),
        if exposed { GOLD } else { RED },
    );
    let pattern = fatherload_simulation::exotic::boss::pattern(style);
    let attack = pattern[usize::from(state.pattern) % pattern.len()];
    let telling = state.volley == fatherload_simulation::world::BOSS_STEP_TELL;
    let line = if exposed {
        Some(("Its weak point is OPEN: strike now!", GOLD))
    } else if telling {
        Some((
            match attack {
                BossAttack::BoltSalvo => "It gathers lightning: leave the flickering columns!",
                BossAttack::Gale => "It draws breath: brace for the gale!",
                BossAttack::Dive => "It fixes on you: get out of its path!",
                BossAttack::SunSlam => "Its sun arm rises: get off the ground!",
                BossAttack::FrostShards => "Frost gathers overhead: leave the marked columns!",
                BossAttack::BeamSweep => "A row glows across the arena: change height!",
                BossAttack::DroneCall => "It calls its drones!",
            },
            AMBER,
        ))
    } else {
        None
    };
    if let Some((text, colour)) = line {
        let tw = ui_kit::text_width(text, TextStyle::Body);
        ui_kit::text(
            draw,
            text,
            (rect.x + (rect.width - tw as f32) * 0.5) as i32,
            (y + 8.0) as i32,
            TextStyle::Body,
            colour,
        );
    }
}
