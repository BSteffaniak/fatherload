//! The depth logistics dialog and its inventory row (Part III F10): the seat's rows come from
//! [`crate::shell::logistics::logistics_rows`] (built by the shell from the world), so this
//! file only lays them out.

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
        sprites::{icons, menu_icons, rig},
    },
    prompts::hint,
    screen::UiFrame,
};
use crate::{
    input::prompts::Prompt,
    shell::logistics::{LogisticsGroup, LogisticsRow},
};
use fatherload_simulation::{
    living_dark::{CourtService, LivingDarkAction},
    logistics::{LogisticsCommand, OutpostService},
};

/// The icon of a logistics row.
const fn row_icon(row: &LogisticsRow) -> &'static SpriteDef {
    match row.command {
        LogisticsCommand::RideLift { .. } | LogisticsCommand::RestoreLift { .. } => &icons::CARGO,
        LogisticsCommand::PlaceLiftStation | LogisticsCommand::RemoveLiftStation { .. } => {
            &menu_icons::HAMMER
        }
        LogisticsCommand::PlaceBeacon | LogisticsCommand::RemoveBeacon { .. } => {
            &menu_icons::SIGNAL
        }
        LogisticsCommand::Travel { .. } => &menu_icons::DRONE,
        LogisticsCommand::BuyDiveCharges | LogisticsCommand::FireDiveCharge => &icons::BOMB,
        LogisticsCommand::Outpost { service, .. } => match service {
            OutpostService::Refuel => &icons::FUEL,
            OutpostService::Repair => &icons::HULL,
            OutpostService::SellCargo => &icons::CREDITS,
            OutpostService::BuyDiveCharges => &icons::BOMB,
            OutpostService::Claim => &menu_icons::SIGNAL,
            OutpostService::SetHome => &menu_icons::HOUSE,
        },
        LogisticsCommand::OutpostUpgrade { offer, .. } => {
            let lines = &fatherload_simulation::economy::UpgradeKind::ALL;
            if (offer as usize) < lines.len() {
                super::counters::upgrade_icon(lines[offer as usize])
            } else {
                &menu_icons::GEAR
            }
        }
        LogisticsCommand::Craft { recipe, .. } => {
            let recipes = &fatherload_simulation::workshop::RECIPES;
            if (recipe as usize) < recipes.len() {
                super::pixel_art::sprites::deep_gear::layer_resource(
                    recipes[recipe as usize].inputs[0].0,
                )
            } else {
                &super::pixel_art::sprites::deep_gear::WORKSHOP
            }
        }
        // The Hollow World (Wave 9): the Gravity Anchor.
        LogisticsCommand::Hollow(_) => &menu_icons::SIGNAL,
        // The far Strange Layers (Wave 9): lamp filters and time bubbles.
        LogisticsCommand::FarStrange(action) => match action {
            fatherload_simulation::far_strange::FarStrangeAction::BuyTimeBubbles => &icons::BOMB,
            _ => &icons::HEADLAMP,
        },
        // The Living Dark (Wave 8): root axes and the Amber Court's counter.
        LogisticsCommand::LivingDark(action) => match action {
            LivingDarkAction::BuyRootAxes | LivingDarkAction::FireRootAxe => &menu_icons::HAMMER,
            LivingDarkAction::Court(service) => match service {
                CourtService::RoyalJelly => &icons::HULL,
                CourtService::TakeErrand | CourtService::TurnIn => &menu_icons::SIGNAL,
                CourtService::BrokerTruce | CourtService::MakeAmends => &icons::CREDITS,
            },
        },
    }
}

/// The depth logistics dialog ([`crate::shell::ModalScreen::Logistics`]).
pub(super) fn draw_logistics(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let menu = &ui.seat.logistics;
    let selected = ui.seat.selected_menu_item;
    let player = ui.player;
    let status = vec![
        SectionItem::icon_stat(
            &icons::CREDITS,
            "Credits",
            format!("{} cr", player.credits),
            GOLD,
        ),
        SectionItem::meter("Fuel", player.fuel, player.fuel_capacity, AMBER, RED),
        SectionItem::icon_stat(
            &icons::BOMB,
            "Dive charges",
            ui.snapshot
                .logistics
                .dive_charges_of(ui.view.player_id)
                .to_string(),
            ORANGE,
        ),
    ];
    let mut about = None;
    let mut sections = vec![Section::new("Rig", SKY_LIGHT, status)];
    // At a workshop: what the bay holds for it.
    if menu
        .rows
        .iter()
        .any(|row| row.group == LogisticsGroup::Workshop)
    {
        let held = layer_resource_items(ui.player);
        sections.push(Section::new(
            "Layer resources",
            MINT,
            if held.is_empty() {
                vec![SectionItem::text(
                    "None aboard yet: the drill digs them out of each stratum's rock now and then.",
                )]
            } else {
                held
            },
        ));
    }
    for (group, title, color) in [
        (LogisticsGroup::Here, "Here", MINT),
        (LogisticsGroup::Shop, "Deep shop", GOLD),
        (LogisticsGroup::Workshop, "Workshop", ORANGE),
        (LogisticsGroup::Travel, "Travel network", SKY_LIGHT),
        (LogisticsGroup::FieldKit, "Field kit", LIME),
    ] {
        let items: Vec<SectionItem> = menu
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.group == group)
            .map(|(index, row)| {
                if index == selected {
                    about = Some(
                        row.refusal
                            .clone()
                            .or_else(|| row.about.clone())
                            .unwrap_or_else(|| row.detail.clone()),
                    );
                }
                SectionItem::item(ItemEntry {
                    icon: Some(row_icon(row)),
                    label: row.label.clone(),
                    detail: row.detail.clone(),
                    value: row.value.clone(),
                    value_color: if row.refusal.is_some() { FOG } else { GOLD },
                    coin: false,
                    selected: index == selected,
                    disabled: row.refusal.is_some(),
                })
            })
            .collect();
        if !items.is_empty() {
            sections.push(Section::new(title, color, items));
        }
    }
    UiLayout::screen(draw).modal_with_render_plan(
        &menu.title,
        &menu.subtitle,
        &ModalContent::new(sections).footer(about).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, ""),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The inventory's depth logistics section: the row that opens the dialog (selected at
/// `row`), and what the rig carries for it.
pub(super) fn inventory_section(ui: &UiFrame<'_, '_>, row: usize) -> Section {
    let logistics = ui.snapshot.logistics;
    let player = ui.view.player_id;
    let mut items = vec![
        SectionItem::item(ItemEntry {
            icon: Some(&rig::DRILL_DOWN),
            label: "Depth logistics".to_owned(),
            detail: "Beacons, lift stations, dive charges and travel".to_owned(),
            value: String::new(),
            value_color: GOLD,
            coin: false,
            selected: ui.seat.selected_menu_item == row,
            disabled: false,
        }),
        SectionItem::stat(
            "Dive charges",
            logistics.dive_charges_of(player).to_string(),
            ORANGE,
        ),
        SectionItem::stat(
            "Beacons placed",
            logistics.beacons_of(player).count().to_string(),
            SKY_LIGHT,
        ),
    ];
    // Wave 8: layer resources aboard, for the outposts' workshops.
    items.extend(layer_resource_items(ui.player));
    Section::new("Depth Logistics", SKY_LIGHT, items)
}

/// One stat per layer resource `player` carries (Wave 8), with its material's icon.
fn layer_resource_items(player: &fatherload_simulation::Player) -> Vec<SectionItem> {
    player
        .cargo_bay
        .iter()
        .filter_map(|(kind, count)| match kind {
            fatherload_simulation::item::ItemKind::Layer(stratum) => Some(SectionItem::icon_stat(
                super::pixel_art::sprites::deep_gear::layer_resource(stratum),
                stratum.def().layer_resource.name,
                format!("x{count}"),
                MINT,
            )),
            _ => None,
        })
        .collect()
}
