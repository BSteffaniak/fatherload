//! The Moon on screen (WP-MOON): the approach checklist under the sky gauge, the vacuum
//! warning, the sunrise set piece after the first landing, and the lunar colony's counter.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI layout mixes small integer art pixels and float rectangles"
)]

use fatherload_simulation::{
    moon::{self, ColonyOffer, MoonRequirement},
    sky::SkyBand,
    terrain::{StrategicResourceKind as Material, TILE_SIZE},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "HUD code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::{
        sprite::SpriteDef,
        sprites::{icons, moon as moon_art, weapons as weapon_art},
    },
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    shell::ShellState,
};

/// How long the landing's sunrise plays, in shell ticks (60 per second).
pub const SUNRISE_TICKS: u64 = 720;

/// Whether the checklist shows for a rig in `band` (from the dead-air gap up).
const fn checklist_band(band: SkyBand) -> bool {
    matches!(
        band,
        SkyBand::DeadAir | SkyBand::Continents | SkyBand::Orbit | SkyBand::Lunar
    )
}

/// Draws the Moon's HUD pieces for `view`: the approach checklist (below the sky gauge at
/// `top`) and the landing's sunrise.
pub(super) fn draw_moon_hud(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    if let Some(landed) = shell.seat(view.seat_index).moon_landing {
        let age = shell.update_ticks.saturating_sub(landed);
        if age < SUNRISE_TICKS {
            draw_sunrise(draw, rect, age);
        }
    }
    let settings = snapshot.terrain.settings();
    if !settings.generator.has_sky() {
        return;
    }
    let player = view.player;
    let row = (player.y / TILE_SIZE).floor() as i32;
    let band = SkyBand::at_row_of(settings, row);
    let missing = moon::missing_requirements(player);
    let vacuum = view.pilot.is_none() && moon::vacuum_at(settings, player.x, player.y);
    let breach = vacuum.then(|| moon::vacuum_breach(player)).flatten();
    if (!checklist_band(band) || missing.is_empty()) && breach.is_none() {
        return;
    }
    let title = if breach.is_some() {
        "HULL BREACH: VACUUM"
    } else {
        "MOON CHECKLIST"
    };
    let lines: Vec<(MoonRequirement, bool)> = moon::approach_checklist(player).to_vec();
    let width = 150;
    let height = 18 + lines.len() as i32 * 11;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
    let style = if breach.is_some() {
        PanelStyle::Warning
    } else {
        PanelStyle::Standard
    };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        style,
    );
    let inner = x + ui_kit::PANEL_PADDING;
    ui_kit::icon(draw, &moon_art::MOON_ICON, inner, top + 4);
    let blink = (shell.update_ticks / 20).is_multiple_of(2);
    ui_kit::text(
        draw,
        title,
        inner + icons::ICON_SIZE + 3,
        top + 5,
        TextStyle::Bold,
        if breach.is_some() && blink { RED } else { MINT },
    );
    for (index, (requirement, met)) in lines.into_iter().enumerate() {
        let y = top + 17 + index as i32 * 11;
        let (mark, color) = if met { ("+", LIME) } else { ("-", ROSE) };
        ui_kit::text(draw, mark, inner + 2, y, TextStyle::Bold, color);
        ui_kit::text(
            draw,
            requirement.name(),
            inner + 12,
            y,
            TextStyle::Body,
            if met { MIST } else { color },
        );
    }
}

/// The sunrise over the Moon's rim: a warm band of light swelling up from the bottom of the
/// view, a white-gold sun cresting at its right with long rays, fading after a few seconds.
fn draw_sunrise(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, age: u64) {
    let t = age as f32 / SUNRISE_TICKS as f32;
    // Swell in fast, hold, fade out.
    let strength = if t < 0.15 {
        t / 0.15
    } else if t < 0.6 {
        1.0
    } else {
        ((1.0 - t) / 0.4).max(0.0)
    };
    let alpha = |base: f32| (base * strength).clamp(0.0, 255.0) as u8;
    // The rig rests on the rim at the view's middle: the horizon runs just under it.
    let horizon = rect.y + rect.height * 0.535;
    let band = rect.height * 0.45;
    // A warm wash rising from the horizon into the black, and a faint glow on the dust.
    draw.draw_rectangle_gradient_v(
        rect.x as i32,
        (horizon - band) as i32,
        rect.width as i32,
        band as i32,
        Color::new(255, 140, 60, 0),
        Color::new(255, 160, 80, alpha(95.0)),
    );
    draw.draw_rectangle_gradient_v(
        rect.x as i32,
        horizon as i32,
        rect.width as i32,
        (rect.height * 0.12) as i32,
        Color::new(255, 190, 120, alpha(70.0)),
        Color::new(255, 150, 80, 0),
    );
    // The sun cresting the rim (only its upper half shows over the ground), rising as it plays.
    let sun_x = rect.x + rect.width * 0.82;
    let sun_y = horizon + rect.height * 0.02 - t * rect.height * 0.05;
    let radius = rect.height * 0.055;
    for (scale, color) in [
        (6.0, Color::new(255, 190, 110, alpha(22.0))),
        (3.5, Color::new(255, 205, 135, alpha(40.0))),
        (2.0, Color::new(255, 225, 165, alpha(80.0))),
        (1.0, Color::new(255, 250, 225, alpha(245.0))),
    ] {
        draw.draw_circle_sector(
            Vector2::new(sun_x, sun_y),
            radius * scale,
            180.0,
            360.0,
            48,
            color,
        );
    }
    // Long rays across the rim.
    for ray in 0..9 {
        let angle = std::f32::consts::PI * (0.92 + 0.02 * ray as f32 + t * 0.03);
        let length = rect.width * (0.5 + 0.05 * (ray % 3) as f32);
        let end = Vector2::new(
            angle.cos().mul_add(length, sun_x),
            angle.sin().mul_add(length * 0.25, sun_y),
        );
        draw.draw_line_ex(
            Vector2::new(sun_x, sun_y),
            end,
            2.0,
            Color::new(255, 220, 160, alpha(50.0)),
        );
    }
}

/// The icon of a colony row.
const fn offer_icon(offer: ColonyOffer) -> &'static SpriteDef {
    match offer {
        ColonyOffer::Refuel | ColonyOffer::MeltIce => &icons::FUEL,
        ColonyOffer::Repair => &icons::HULL,
        ColonyOffer::SellHaul => &icons::CREDITS,
        ColonyOffer::Reactor => &moon_art::LIFE_SUPPORT,
        ColonyOffer::AlloyDrill => &super::pixel_art::sprites::rig::DRILL_DOWN,
        ColonyOffer::BuyWeapon(weapon) | ColonyOffer::UpgradeWeapon(weapon) => {
            weapon_art::icon(weapon)
        }
    }
}

/// The lunar colony's counter ([`crate::shell::ModalScreen::Colony`]).
pub(super) fn draw_colony(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let player = ui.player;
    let selected = ui.seat.selected_menu_item;
    let material = |kind| player.cargo_bay.material(kind);
    let status = vec![
        SectionItem::stat("Credits", format!("{} c", player.credits), GOLD),
        SectionItem::stat(
            "Aboard",
            format!(
                "{} He-3  {} ice  {} alloy",
                material(Material::Helium3),
                material(Material::MoonIce),
                material(Material::AlienAlloy)
            ),
            SKY_LIGHT,
        ),
        SectionItem::meter("Fuel", player.fuel, player.fuel_capacity, AMBER, RED),
    ];
    let mut about = None;
    let rows: Vec<SectionItem> = ColonyOffer::ALL
        .iter()
        .enumerate()
        .map(|(index, offer)| {
            let price = offer.price(player);
            let affordable = price.is_some_and(|price| match offer {
                ColonyOffer::Refuel => player.credits >= moon::COLONY_FUEL_UNIT_COST,
                ColonyOffer::Repair => player.credits >= moon::COLONY_REPAIR_UNIT_COST,
                _ => price.affordable(player),
            });
            let value = match (offer, price) {
                (_, None) => match offer {
                    ColonyOffer::Reactor | ColonyOffer::AlloyDrill | ColonyOffer::BuyWeapon(_) => {
                        "Owned".to_owned()
                    }
                    _ => "-".to_owned(),
                },
                (ColonyOffer::Refuel, Some(_)) => {
                    format!("{} c / fuel", moon::COLONY_FUEL_UNIT_COST)
                }
                (ColonyOffer::Repair, Some(_)) => {
                    format!("{} c / point", moon::COLONY_REPAIR_UNIT_COST)
                }
                (ColonyOffer::MeltIce, Some(_)) => format!("+{:.0} fuel each", moon::MOON_ICE_FUEL),
                (ColonyOffer::SellHaul, Some(_)) => format!(
                    "{} c",
                    moon::haul_value(&player.cargo_bay, player.fuel >= player.fuel_capacity)
                ),
                (_, Some(price)) => price.label(),
            };
            if index == selected {
                about = Some(offer.description().to_owned());
            }
            SectionItem::item(ItemEntry {
                icon: Some(offer_icon(*offer)),
                label: offer.name(),
                detail: String::new(),
                value,
                value_color: if affordable { GOLD } else { FOG },
                coin: false,
                selected: index == selected,
                disabled: !affordable,
            })
        })
        .collect();
    let sections = vec![
        Section::new("Status", SKY_LIGHT, status),
        Section::new("Counter", MINT, rows),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "Tranquility Station",
        "The lunar research colony: air, fuel, a fair price for helium-3, and gear you will not find below.",
        &ModalContent::new(sections).footer(about).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, ""),
            hint(Prompt::Back, "close"),
        ]),
    );
}
