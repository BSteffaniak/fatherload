//! The garage panel (the Repair Garage's paint bay): one row per cosmetic slot showing the
//! draft's choice (paint names in their own colour, locked items tagged), the finishing rows, and
//! a preview card that shows the draft rig large, cycling through parked, climbing and drilling
//! poses. The rig standing in the room wears the draft too.

use fatherload_simulation::{CosmeticItem, RigCosmetics};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{self, ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::{sprite::SpriteDef, sprites::rig},
    prompts::hint,
    rig_art::{self, DrillPose, RigLook, RigPose, Thrust},
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    shell::{GarageRow, item_available},
};

/// The icon of a garage row.
const fn row_icon(row: GarageRow) -> &'static SpriteDef {
    match row {
        GarageRow::Primary | GarageRow::Secondary | GarageRow::Trim | GarageRow::Apply => {
            &rig::ICON_SPRAY_CAN
        }
        GarageRow::Flame => &rig::ICON_FLAME,
        GarageRow::Drill => &rig::DRILL_SIDE,
        GarageRow::Pattern => &rig::ICON_PATTERN,
        GarageRow::Decal => &rig::ICON_STAR,
        GarageRow::Roof => &rig::ICON_ANTENNA,
        GarageRow::Front => &rig::ICON_LAMP,
        GarageRow::Rear => &rig::ICON_PENNANT,
        GarageRow::Shuffle => &rig::ICON_DICE,
        GarageRow::Default => &rig::ICON_CREW,
    }
}

/// What the details footer says about a row.
fn row_about(row: GarageRow, locked: bool) -> String {
    if locked {
        return "Locked. This one has to be earned out in the world; you can still try it on."
            .to_owned();
    }
    match row {
        GarageRow::Primary => {
            "The hull's main colour, and your colour on name tags, the HUD and the minimap."
        }
        GarageRow::Secondary => "The chassis band, cargo pods and thruster pods.",
        GarageRow::Trim => "Stripes, rims, the drill collar and decals.",
        GarageRow::Flame => "The colour of every thruster flame and the pods' intake glow.",
        GarageRow::Drill => "The drill's shape. Your drill tier decides its metal and size.",
        GarageRow::Pattern => "A pattern painted across the hull.",
        GarageRow::Decal => "A decal on the flank.",
        GarageRow::Roof => "What rides on the mast.",
        GarageRow::Front => "What sits on the nose, above the drill.",
        GarageRow::Rear => "What sits on the tail.",
        GarageRow::Shuffle => "A random look from everything you own.",
        GarageRow::Default => "Back to your crew colours.",
        GarageRow::Apply => "Paint it for real. Free of charge; Iona likes an excuse.",
    }
    .to_owned()
}

/// The colour a row's choice is written in.
const fn item_color(item: CosmeticItem) -> Color {
    match item {
        CosmeticItem::Paint(paint) => rig_art::paint_accent(paint),
        CosmeticItem::Flame(flame) => rig_art::flame_ramp(flame, 0).bright,
        _ => BONE,
    }
}

pub(super) fn draw_garage(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(look) = ui.seat.garage_preview() else {
        return;
    };
    let selected = ui.seat.selected_menu_item;
    let current = RigCosmetics::resolve(ui.player.cosmetics, ui.view.player_id);
    let mut about = None;
    let mut sections: Vec<(&str, Color, Vec<SectionItem>)> = vec![
        ("Paint", GOLD, Vec::new()),
        ("Parts", SKY_LIGHT, Vec::new()),
        ("Attachments", MINT, Vec::new()),
        ("Finish", PINK, Vec::new()),
    ];
    for (index, row) in GarageRow::ALL.into_iter().enumerate() {
        let item = row.item(&look);
        let locked = item
            .is_some_and(|item| !item_available(ui.snapshot.test_mode.is_some(), ui.player, item));
        let changed = look != current;
        if index == selected {
            about = Some(row_about(row, locked));
        }
        let (value, value_color) = match (row, item) {
            (_, Some(item)) if locked => (format!("{} LOCKED", item.label()), RED),
            (_, Some(item)) => (item.label().to_owned(), item_color(item)),
            (GarageRow::Apply, None) if !changed => ("FITTED".to_owned(), LIME),
            (GarageRow::Apply, None) => ("FREE".to_owned(), LIME),
            _ => (String::new(), BONE),
        };
        let blocked = row == GarageRow::Apply
            && look
                .items()
                .into_iter()
                .any(|item| !item_available(ui.snapshot.test_mode.is_some(), ui.player, item));
        let section = match index {
            0..=2 => 0,
            3..=6 => 1,
            7..=9 => 2,
            _ => 3,
        };
        sections[section].2.push(SectionItem::item(ItemEntry {
            icon: Some(row_icon(row)),
            label: row.label().to_owned(),
            detail: String::new(),
            value,
            value_color,
            coin: false,
            selected: index == selected,
            disabled: blocked || (row == GarageRow::Apply && !changed),
        }));
    }
    let sections = sections
        .into_iter()
        .map(|(title, color, items)| Section::new(title, color, items))
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "Paint Bay",
        "Iona's spray booth: try anything on, pay nothing.",
        &ModalContent::new(sections)
            .footer(about)
            .prompts(&garage_prompts(super::prompts::menu_device().is_gamepad())),
    );
    draw_preview_card(draw, ui, look);
}

/// The Paint Bay's footer prompts. A controller also changes options with LB/RB (the shell
/// reads the tab buttons there); on a keyboard those are the same keys as "change".
fn garage_prompts(gamepad: bool) -> Vec<super::prompts::Hint> {
    let mut prompts = vec![hint(Prompt::Navigate, "select")];
    if gamepad {
        prompts.extend([hint(Prompt::Adjust, ""), hint(Prompt::Tab, "change")]);
    } else {
        prompts.push(hint(Prompt::Adjust, "change"));
    }
    prompts.extend([hint(Prompt::Confirm, ""), hint(Prompt::Back, "counter")]);
    prompts
}

/// The pose the preview card shows at `seconds` (it cycles every two seconds).
fn preview_pose(seconds: f32) -> (RigPose, &'static str) {
    let ticks = (seconds * 60.0) as u64;
    let idle = RigPose::idle(ticks);
    match (seconds / 2.0) as u32 % 3 {
        0 => (
            RigPose {
                beacon: (!(ticks / 40).is_multiple_of(2)).then_some(GOLD),
                ..idle
            },
            "Parked",
        ),
        1 => (
            RigPose {
                thrust: Thrust::Climb,
                drill: DrillPose::Nose { spinning: true },
                ..idle
            },
            "Climbing",
        ),
        _ => (
            RigPose {
                drill: DrillPose::Down,
                thrust: Thrust::Hover,
                ..idle
            },
            "Drilling",
        ),
    }
}

/// A card beside the panel with the draft rig drawn large, when the view has room for it.
fn draw_preview_card(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>, look: RigCosmetics) {
    let canvas = ui_kit::canvas();
    let Some(panel) = layout::peek_last_dialog_rect() else {
        return;
    };
    let scale = if canvas.height >= 300 { 4 } else { 2 };
    // The rig with every part and a drill stays inside this many art pixels.
    let (art_w, art_h) = (34, 30);
    let card_w = art_w * scale + 16;
    let card_h = art_h * scale + 30;
    let free_left = (panel.x + panel.width) as i32 + 8;
    let x = canvas.width - card_w - 8;
    if x < free_left || card_h + 16 > canvas.height {
        return;
    }
    let y = 8;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, card_w as f32, card_h as f32),
        PanelStyle::Accent,
    );
    let (pose, caption) = preview_pose(ui_kit::seconds());
    ui_kit::text(draw, "Preview", x + 8, y + 6, TextStyle::Bold, GOLD);
    let caption_width = ui_kit::text_width(caption, TextStyle::Body);
    ui_kit::text(
        draw,
        caption,
        x + card_w - 8 - caption_width,
        y + 6,
        TextStyle::Body,
        MIST,
    );
    let well = Rectangle::new(
        (x + 6) as f32,
        (y + 18) as f32,
        (card_w - 12) as f32,
        (card_h - 24) as f32,
    );
    ui_kit::well(draw, well);
    let rig_look = RigLook::from_player(ui.view.player_id, ui.player).wearing(look);
    let painted = rig_art::paint_rig(&rig_look, &pose);
    // Centre what is painted (pods, drill and flames included) in the well.
    let Some((min_x, min_y, max_x, max_y)) = painted.bounds() else {
        return;
    };
    let centre_x = well.x as i32 + well.width as i32 / 2;
    let centre_y = well.y as i32 + well.height as i32 / 2;
    let origin_x = centre_x - (min_x + max_x + 1) * scale / 2;
    let origin_y = centre_y - (min_y + max_y + 1) * scale / 2;
    painted.draw(
        draw,
        Vector2::new(origin_x as f32, origin_y as f32),
        scale as f32,
        false,
    );
}
