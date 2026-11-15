//! Base building UI (WP-BASE): the build-mode bar along the bottom of a view (the piece strip,
//! the selected piece's name and cost, whether it fits here, the controls) and the base module
//! menu.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "UI layout mixes small integer art pixels and float rectangles"
)]

use fatherload_simulation::base::{BuildGroup, BuildRefusal, Buildable, ModuleKind, ModuleService};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::sprites::icons,
    prompts::{self, hint},
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
    world::base::draw_item_icon,
};
use crate::{
    base_view::BuildPreview,
    input::{devices::InputDevice, prompts::Prompt},
    render_snapshot::ViewSnapshot,
};

/// What `preview`'s piece costs here, as one line.
fn cost_line(preview: &BuildPreview) -> (String, Color) {
    let base = preview.item.base_cost();
    preview.place.as_ref().map_or_else(
        |_| {
            let text = match (base.block, base.any_blocks, base.credits) {
                (Some(block), _, _) => format!(
                    "1 {} or {} c",
                    block.name(),
                    fatherload_simulation::base::block_credit_price(block)
                ),
                (None, 0, credits) => format!("{credits} c"),
                (None, blocks, credits) => format!("{credits} c + {blocks} blocks"),
            };
            (text, FOG)
        },
        |quote| {
            let blocks = quote.cost.blocks.total();
            let credits = quote.cost.credits;
            let text = match (blocks, credits) {
                (0, 0) => "Free".to_owned(),
                (0, credits) => format!("{credits} c"),
                (blocks, 0) => format!("{blocks} block{}", if blocks == 1 { "" } else { "s" }),
                (blocks, credits) => format!("{blocks} blocks + {credits} c"),
            };
            (text, GOLD)
        },
    )
}

/// The status line: ready, or why not.
fn status_line(preview: &BuildPreview) -> (String, Color) {
    match &preview.place {
        Ok(_) => match preview.item {
            Buildable::Module(ModuleKind::Beacon) => ("Stake a claim here".to_owned(), MINT),
            _ => ("Fits here".to_owned(), MINT),
        },
        Err(BuildRefusal::CannotAfford) => (BuildRefusal::CannotAfford.message(), AMBER),
        Err(refusal) => (refusal.message(), RED),
    }
}

/// What the remove button would take down at the cursor.
fn remove_line(preview: &BuildPreview) -> Option<String> {
    let quote = preview.remove.as_ref().ok()?;
    let blocks = quote.refund.blocks.total();
    let credits = quote.refund.credits;
    Some(match (blocks, credits) {
        (0, 0) => "Take down".to_owned(),
        (0, credits) => format!("Take down: +{credits} c"),
        (blocks, 0) => format!(
            "Take down: +{blocks} block{}",
            if blocks == 1 { "" } else { "s" }
        ),
        (blocks, credits) => format!("Take down: +{blocks} blocks, +{credits} c"),
    })
}

/// The build bar along the bottom of `view`.
#[allow(
    clippy::too_many_lines,
    reason = "the bar's rows are laid out top to bottom in one place"
)]
pub(super) fn draw_build_bar(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    preview: &BuildPreview,
    device: InputDevice,
) {
    let rect = ui_kit::canvas().viewport_rect(view.viewport);
    // Left of the minimap (bottom-right, `screen::draw_minimap_for_view`).
    let minimap = (rect.width / 6.0).clamp(48.0, 80.0) + 30.0;
    let room = rect.width - 12.0 - minimap;
    let width = room.min(420.0);
    let hints = [
        hint(Prompt::BuildPlace, "place"),
        hint(Prompt::BuildRemove, "take down"),
        hint(Prompt::BuildCursor, "aim"),
        hint(Prompt::BuildCycle, "piece"),
        hint(Prompt::BuildRotate, "rotate"),
        hint(Prompt::BuildMode, "done"),
    ];
    let rows = prompts::wrap_bar(&hints, device, width as i32 - 16);
    let prompt_height = prompts::wrapped_height(rows.len());
    let height = 74.0 + prompt_height as f32;
    let x = rect.x + 8.0 + (room - width) * 0.5;
    let y = rect.y + rect.height - height - 6.0;
    ui_kit::panel(
        draw,
        Rectangle::new(x, y, width, height),
        PanelStyle::Accent,
    );
    let left = x as i32 + 8;
    let mut row_y = y as i32 + 6;

    // The piece strip: the selection in the middle, its neighbours either side.
    let slots = ((width as i32 - 16) / 22).clamp(3, 15) | 1;
    let half = slots / 2;
    let count = Buildable::PALETTE.len() as i32;
    let strip_left = x as i32 + (width as i32 - slots * 22) / 2;
    let accent = GOLD;
    for slot in 0..slots {
        let index = (preview.selected as i32 + slot - half).rem_euclid(count);
        let item = Buildable::PALETTE[index as usize];
        let sx = strip_left + slot * 22;
        let selected = slot == half;
        let frame = Rectangle::new(sx as f32, row_y as f32, 20.0, 20.0);
        if selected {
            draw.draw_rectangle_rec(frame, with_alpha(GOLD, 255));
            draw.draw_rectangle_rec(
                Rectangle::new(frame.x + 1.0, frame.y + 1.0, 18.0, 18.0),
                NIGHT,
            );
        } else {
            draw.draw_rectangle_rec(frame, with_alpha(INK, 160));
        }
        draw_item_icon(draw, sx as f32 + 2.0, row_y as f32 + 2.0, 1.0, item, accent);
        // A tick between groups.
        let previous = Buildable::PALETTE[(index - 1).rem_euclid(count) as usize];
        if previous.group() != item.group() && slot > 0 {
            draw.draw_rectangle_rec(
                Rectangle::new(sx as f32 - 2.0, row_y as f32 + 2.0, 1.0, 16.0),
                with_alpha(FOG, 160),
            );
        }
    }
    row_y += 24;

    // Name, group and cost.
    let group = match preview.item.group() {
        BuildGroup::Structure => SKY_LIGHT,
        BuildGroup::Interior => MINT,
        BuildGroup::Modules => PINK,
    };
    ui_kit::text(
        draw,
        preview.item.name(),
        left,
        row_y,
        TextStyle::Bold,
        BONE,
    );
    let name_width = ui_kit::text_width(preview.item.name(), TextStyle::Bold);
    ui_kit::text(
        draw,
        preview.item.group().name(),
        left + name_width + 6,
        row_y,
        TextStyle::Body,
        group,
    );
    let (cost, cost_color) = cost_line(preview);
    let cost_width = ui_kit::text_width(&cost, TextStyle::Bold);
    if name_width + cost_width + 24 > width as i32 {
        // Narrow views: the cost goes on the detail line's place.
        row_y += 12;
        ui_kit::text(draw, &cost, left, row_y, TextStyle::Bold, cost_color);
        row_y += 12;
        let (status, status_color) = status_line(preview);
        ui_kit::text(draw, &status, left, row_y, TextStyle::Bold, status_color);
        row_y += 14;
        for row in rows {
            prompts::draw_bar(draw, left, row_y, &row, device, BONE);
            row_y += prompts::wrapped_height(1) + 2;
        }
        return;
    }
    ui_kit::text_right(
        draw,
        &cost,
        x as i32 + width as i32 - 8,
        row_y,
        TextStyle::Bold,
        cost_color,
    );
    row_y += 12;

    // What the module does, or the hopper.
    let detail = match preview.item {
        Buildable::Module(kind) => kind.blurb().to_owned(),
        _ => format!(
            "Hopper {}/{} blocks{}",
            preview.hopper_total,
            preview.hopper_capacity,
            if preview.item.base_cost().block.is_some() {
                format!(", {} of this kind", preview.blocks_on_hand)
            } else {
                String::new()
            }
        ),
    };
    let detail = ui_kit::truncate(&detail, width as i32 - 16, TextStyle::Body);
    ui_kit::text(draw, &detail, left, row_y, TextStyle::Body, MIST);
    row_y += 12;

    // Whether it fits, and what taking down the cursor tile gives back.
    let (status, status_color) = status_line(preview);
    ui_kit::text(draw, &status, left, row_y, TextStyle::Bold, status_color);
    if let Some(remove) = remove_line(preview).filter(|_| width >= 300.0) {
        ui_kit::text_right(
            draw,
            &remove,
            x as i32 + width as i32 - 8,
            row_y,
            TextStyle::Body,
            FOG,
        );
    }
    row_y += 14;
    for row in rows {
        prompts::draw_bar(draw, left, row_y, &row, device, BONE);
        row_y += prompts::wrapped_height(1) + 2;
    }
}

/// The base module menu: one row per service, the module's state and power.
pub(super) fn draw_base_module(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(menu) = &ui.view.base_module else {
        return;
    };
    let selected = ui.seat.selected_menu_item;
    let mut status = vec![SectionItem::stat(
        "Claim",
        if menu.owned {
            "Yours".to_owned()
        } else {
            format!("Player {}", menu.claim.owner.get() + 1)
        },
        if menu.owned { MINT } else { SKY_LIGHT },
    )];
    status.push(SectionItem::stat(
        "Power",
        format!(
            "{} / {}{}",
            menu.power.demand.min(menu.power.supply),
            menu.power.supply,
            if menu.needs_power() {
                if menu.powered { "  ON" } else { "  OFF" }
            } else {
                ""
            }
        ),
        if !menu.needs_power() || menu.powered {
            LIME
        } else {
            RED
        },
    ));
    match menu.kind() {
        ModuleKind::FuelStation => status.push(SectionItem::meter(
            "Reserve",
            menu.module.fuel as f32,
            fatherload_simulation::base::FUEL_STATION_CAPACITY as f32,
            AMBER,
            RED,
        )),
        ModuleKind::Storage | ModuleKind::Homestead => status.push(SectionItem::meter(
            "Stored",
            menu.module.storage.slots_used() as f32,
            menu.kind().storage_capacity() as f32,
            SKY,
            GOLD,
        )),
        _ => {}
    }
    if menu.is_home {
        status.push(SectionItem::stat(
            "Home",
            "The rescue crew brings you here",
            GOLD,
        ));
    }
    let mut about = None;
    let rows: Vec<SectionItem> = menu
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            if index == selected {
                about = Some(row.blocked.map_or_else(
                    || service_about(row.service).to_owned(),
                    BuildRefusal::message,
                ));
            }
            SectionItem::item(ItemEntry {
                icon: Some(service_icon(row.service)),
                label: row.label.clone(),
                detail: String::new(),
                value: row.value.clone(),
                value_color: if row.blocked.is_some() { FOG } else { GOLD },
                coin: false,
                selected: index == selected,
                disabled: row.blocked.is_some(),
            })
        })
        .collect();
    let mut sections = vec![Section::new("Status", SKY_LIGHT, status)];
    if rows.is_empty() {
        about = Some(menu.kind().blurb().to_owned());
    } else {
        sections.push(Section::new("Services", GOLD, rows));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        menu.kind().name(),
        menu.kind().blurb(),
        &ModalContent::new(sections).footer(about).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, ""),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The icon of a service row.
const fn service_icon(service: ModuleService) -> &'static super::pixel_art::sprite::SpriteDef {
    match service {
        ModuleService::Refuel | ModuleService::OrderSupply => &icons::FUEL,
        ModuleService::Repair => &icons::HULL,
        ModuleService::DepositCargo | ModuleService::DepositBlocks | ModuleService::WithdrawAll => {
            &icons::CARGO
        }
        ModuleService::SellCargo | ModuleService::CollectRefined => &icons::CREDITS,
        ModuleService::SetHome => &icons::DEPTH,
        ModuleService::CycleAccess => &icons::SCANNER,
        ModuleService::RefineOre => &icons::HEAT,
        ModuleService::Craft(_) => &icons::BOMB,
    }
}

/// What a service row does (the footer line).
const fn service_about(service: ModuleService) -> &'static str {
    match service {
        ModuleService::Refuel => "Fill the tank from the reserve. Your own fuel is free.",
        ModuleService::OrderSupply => "A drone drops a canister: fills the reserve for credits.",
        ModuleService::Repair => "Patch the hull at town prices.",
        ModuleService::DepositCargo => "Store ore, relics and materials from the bay.",
        ModuleService::DepositBlocks => "Store the hopper's building blocks.",
        ModuleService::WithdrawAll => "Load everything that fits back aboard.",
        ModuleService::SellCargo => "Sell the bay's ore and relics at today's prices, less 15%.",
        ModuleService::SetHome => "The rescue crew will drop you here instead of in town.",
        ModuleService::CycleAccess => "Who may build here and use the claim's modules.",
        ModuleService::RefineOre => {
            "Leave the bay's ore here. Powered, it refines a lump every 3 s, worth 40% more."
        }
        ModuleService::CollectRefined => "Take what the refined ore is worth, in credits.",
        ModuleService::Craft(_) => {
            "Press blocks from the hopper (plus a little credit) into a charge."
        }
    }
}
