use raylib::prelude::*;

use drillgame_simulation::{
    Player,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    counters,
    layout::{ModalContent, Section, SectionItem, UiLayout},
    pixel_art::{palette, sprite::SpriteDef, sprites::icons},
    ui_kit::{self, BarColors, PanelStyle, TextStyle},
};
use crate::{
    render_snapshot::{PlayerPresentation, RenderSnapshot, ViewSnapshot},
    runtime::OnlineRole,
    save::{latest_save_summary, save_slot_count, save_slot_metadata, saves_exist},
    shell::{JOIN_MENU, ModalScreen, OnlineScreen, PauseOption, ShellState, TitleOption},
};

/// What screen-space UI (HUD, menus, modals) reads: shell state plus the snapshot and the view
/// the UI acts for.
pub(super) struct UiFrame<'a, 'w> {
    pub shell: &'a ShellState,
    pub snapshot: &'a RenderSnapshot<'w>,
    pub view: &'a ViewSnapshot<'w>,
    pub player: &'w Player,
}

/// A view's rect in UI canvas art pixels.
fn view_rect(view: &ViewSnapshot<'_>) -> Rectangle {
    ui_kit::canvas().viewport_rect(view.viewport)
}

/// Tints the view orange near lava or deep down (heat), pulsing when lava is close.
pub(super) fn draw_heat_warning_for_view(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
) {
    let depth_heat = view.player.y > 18.0 * TILE_SIZE;
    let tile = view.player.tile_position(TILE_SIZE);
    let near_lava = (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            matches!(
                terrain.tile(TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                }),
                Some(tile) if tile.kind == TileKind::Lava
            )
        })
    });
    if !(depth_heat || near_lava) {
        return;
    }
    let rect = view_rect(view);
    let alpha = if near_lava { 60 } else { 28 };
    draw.draw_rectangle_rec(rect, palette::with_alpha(ORANGE, alpha));
    if near_lava {
        // A pulsing heat border inside the view edge.
        let on = (ui_kit::seconds() * 3.0) as i32 % 2 == 0;
        let color = palette::with_alpha(if on { RED } else { ORANGE }, 150);
        let (x, y, width, height) = (
            rect.x as i32,
            rect.y as i32,
            rect.width as i32,
            rect.height as i32,
        );
        draw.draw_rectangle(x, y, width, 2, color);
        draw.draw_rectangle(x, y + height - 2, width, 2, color);
        draw.draw_rectangle(x, y, 2, height, color);
        draw.draw_rectangle(x + width - 2, y, 2, height, color);
    }
}

/// Width of the HUD stats panel for a view `view_width` art pixels wide.
fn stats_panel_width(view_width: f32) -> i32 {
    ((view_width * 0.42) as i32).clamp(132, 176)
}

/// The view's HUD: hull / fuel / cargo bars (top-left), credits and depth (top-right), warning
/// badges under the bars, and the diagnostics panel when held.
#[allow(
    clippy::too_many_lines,
    reason = "the HUD draws its panels in reading order in one place"
)]
pub(super) fn draw_view_hud(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
) {
    let fps = draw.get_fps();
    let hud = view.hud;
    let rect = view_rect(view);
    let (left, top) = (rect.x as i32 + 4, rect.y as i32 + 4);
    let split = snapshot.views.len() > 1;

    // Stats: hull, fuel, cargo.
    let width = stats_panel_width(rect.width);
    let header = if split { 11 } else { 0 };
    let panel_height = 6 + header + 3 * 12 + 3;
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, panel_height as f32),
        PanelStyle::Standard,
    );
    let inner_x = left + ui_kit::PANEL_PADDING;
    let inner_width = width - ui_kit::PANEL_PADDING * 2;
    let mut y = top + 6;
    if split {
        ui_kit::text(
            draw,
            &format!("PLAYER {}", view.seat_index + 1),
            inner_x,
            y,
            TextStyle::Bold,
            GOLD,
        );
        y += header;
    }
    y += ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::HULL,
        "HULL",
        hud.hull,
        hud.max_hull,
        BarColors::HULL,
        0.35,
    );
    y += ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::FUEL,
        "FUEL",
        hud.fuel,
        hud.fuel_capacity,
        BarColors::FUEL,
        0.2,
    );
    ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::CARGO,
        "CARGO",
        hud.cargo_used as f32,
        hud.cargo_capacity as f32,
        BarColors::CARGO,
        0.0,
    );

    // Credits and depth, clear of the depth ruler on the right edge.
    let money = format!("{}", hud.credits);
    let depth = format!("{} m", hud.depth);
    let value_width = ui_kit::text_width(&money, TextStyle::Bold)
        .max(ui_kit::text_width(&depth, TextStyle::Bold));
    let money_width = (value_width + icons::ICON_SIZE + 18).max(64);
    let money_x = (rect.x + rect.width) as i32 - 16 - money_width;
    if money_x > left + width + 4 {
        ui_kit::panel(
            draw,
            Rectangle::new(money_x as f32, top as f32, money_width as f32, 32.0),
            PanelStyle::Standard,
        );
        let right = money_x + money_width - ui_kit::PANEL_PADDING;
        ui_kit::icon(draw, &icons::CREDITS, money_x + 6, top + 6);
        ui_kit::text_right(draw, &money, right, top + 7, TextStyle::Bold, GOLD);
        ui_kit::icon(draw, &icons::DEPTH, money_x + 6, top + 17);
        ui_kit::text_right(draw, &depth, right, top + 18, TextStyle::Bold, SKY_LIGHT);
    }

    // Warning badges.
    let mut badge_x = left;
    let badge_y = top + panel_height + 3;
    for (warning, sprite) in hud_warnings(snapshot, view) {
        badge_x += ui_kit::badge(draw, badge_x, badge_y, sprite, warning) + 2;
    }

    if shell.show_details {
        let lines = [
            format!("FPS {fps}"),
            format!(
                "Tick {} ({} new events)",
                snapshot.tick,
                snapshot.events.len()
            ),
            format!(
                "Camera {:.0},{:.0}",
                view.camera.x / TILE_SIZE,
                view.camera.y / TILE_SIZE
            ),
            shell.message.clone(),
        ];
        let details_y = badge_y + 20;
        let details_width = (rect.width as i32 - 8).min(280);
        let wrapped: Vec<String> = lines
            .iter()
            .flat_map(|line| ui_kit::wrap(line, details_width - 12, TextStyle::Body))
            .collect();
        let height = i32::try_from(wrapped.len()).unwrap_or(0) * TextStyle::Body.line_height() + 22;
        ui_kit::panel(
            draw,
            Rectangle::new(
                left as f32,
                details_y as f32,
                details_width as f32,
                height as f32,
            ),
            PanelStyle::Standard,
        );
        ui_kit::text(
            draw,
            "DIAGNOSTICS",
            left + 6,
            details_y + 6,
            TextStyle::Bold,
            LIME,
        );
        for (index, line) in wrapped.iter().enumerate() {
            let row = i32::try_from(index).unwrap_or(0);
            ui_kit::text(
                draw,
                line,
                left + 6,
                details_y + 17 + row * TextStyle::Body.line_height(),
                TextStyle::Body,
                MIST,
            );
        }
    }
}

/// Active rig warnings for the HUD badges (same thresholds as the research log's summary).
fn hud_warnings(
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
) -> Vec<(&'static str, &'static SpriteDef)> {
    let player = view.player;
    let mut warnings = Vec::new();
    if player.fuel <= player.fuel_capacity * 0.2 {
        warnings.push(("LOW FUEL", &icons::FUEL));
    }
    if player.hull <= player.max_hull() * 0.35 {
        warnings.push(("HULL", &icons::WARNING));
    }
    if view.hud.depth >= 60 {
        warnings.push(("HEAT", &icons::HEAT));
    }
    if view.deep_instability >= 60.0 {
        warnings.push(("UNSTABLE", &icons::WARNING));
    }
    if view.hud.depth >= 80 && snapshot.signal_relay_count == 0 {
        warnings.push(("NO SIGNAL", &icons::WARNING));
    }
    warnings
}

/// Colour of an explored tile on the minimap.
const fn minimap_color(kind: TileKind) -> Color {
    match kind {
        TileKind::Air => SLATE_DARK,
        TileKind::Ore(_) => GOLD,
        TileKind::Artifact(_) => PINK,
        TileKind::Gas => LIME,
        TileKind::Lava | TileKind::MagmaVent => ORANGE,
        TileKind::ExplosivePocket | TileKind::PressurePocket => RED,
        TileKind::Stone | TileKind::HardRock | TileKind::Foundation => STONE,
        TileKind::Dirt | TileKind::Clay => LOAM,
    }
}

/// Bottom-right minimap of the explored mine around the rig (left of the depth ruler).
pub(super) fn draw_minimap_for_view(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
    players: &[PlayerPresentation],
) {
    let rect = view_rect(view);
    let width = ((rect.width / 6.0) as i32).clamp(48, 80);
    let height = ((rect.height / 5.0) as i32).clamp(36, 56);
    let x = (rect.x + rect.width) as i32 - width - 22;
    let y = (rect.y + rect.height) as i32 - height - 10;
    ui_kit::panel(
        draw,
        Rectangle::new(
            (x - 4) as f32,
            (y - 4) as f32,
            (width + 8) as f32,
            (height + 8) as f32,
        ),
        PanelStyle::Standard,
    );
    draw.draw_rectangle(x, y, width, height, NIGHT);

    let terrain_width = terrain.width().max(1);
    let terrain_height = terrain.height().max(1);
    let player_tile = view.player.tile_position(TILE_SIZE);
    let visible_height = terrain_height.min(80);
    let origin_y = if terrain_height > visible_height {
        (player_tile.y - visible_height / 2).clamp(0, terrain_height - visible_height)
    } else {
        0
    };
    for py in 0..height {
        let tile_y = origin_y + py * visible_height / height;
        for px in 0..width {
            let position = TilePosition {
                x: px * terrain_width / width,
                y: tile_y,
            };
            let sky = tile_y <= 4;
            if !sky && !view.is_explored(position) {
                continue;
            }
            let color = match terrain.tile(position) {
                Some(tile) if tile.kind == TileKind::Air && sky => DEEP_BLUE,
                Some(tile) => minimap_color(tile.kind),
                None => continue,
            };
            draw.draw_rectangle(x + px, y + py, 1, 1, color);
        }
    }
    let to_map = |tile_x: i32, tile_y: i32| {
        (
            x + tile_x * width / terrain_width,
            y + (tile_y - origin_y) * height / visible_height,
        )
    };
    for remote in players
        .iter()
        .filter(|other| other.player_id != view.player_id)
    {
        let tile_x = (remote.x / TILE_SIZE) as i32;
        let tile_y = (remote.y / TILE_SIZE) as i32;
        if tile_y < origin_y || tile_y >= origin_y + visible_height {
            continue;
        }
        let (map_x, map_y) = to_map(tile_x, tile_y);
        draw.draw_rectangle(map_x - 1, map_y - 1, 2, 2, ORANGE);
    }
    let (map_x, map_y) = to_map(player_tile.x, player_tile.y);
    let blink = (ui_kit::seconds() * 2.0) as i32 % 2 == 0;
    draw.draw_rectangle(map_x - 1, map_y - 1, 3, 3, INK);
    draw.draw_rectangle(map_x, map_y, 1, 1, if blink { BONE } else { SKY_LIGHT });
}

/// Depth gauge down the view's right edge with a marker at the rig's depth.
pub(super) fn draw_depth_ruler_for_view(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
) {
    let rect = view_rect(view);
    let x = (rect.x + rect.width) as i32 - 12;
    let top = rect.y as i32 + 42;
    let height = ((rect.y + rect.height) as i32 - 16 - top).max(40);
    ui_kit::well(
        draw,
        Rectangle::new(x as f32, top as f32, 6.0, height as f32),
    );
    let max_depth = terrain.height().max(80);
    let step = if max_depth > 180 { 50 } else { 20 };
    for marker in (0..=max_depth).step_by(usize::try_from(step).unwrap_or(20)) {
        let y = top + marker * (height - 1) / max_depth;
        draw.draw_rectangle(x - 3, y, 3, 1, FOG);
    }
    let depth = (view.player.y / TILE_SIZE).max(0.0);
    let marker_y = top + ((depth / max_depth as f32).clamp(0.0, 1.0) * (height - 1) as f32) as i32;
    // A left-pointing marker: ink outline around a gold arrow.
    draw.draw_rectangle(x - 4, marker_y - 3, 11, 7, INK);
    draw.draw_rectangle(x - 3, marker_y - 2, 9, 5, GOLD);
    draw.draw_rectangle(x - 3, marker_y + 1, 9, 1, AMBER);
}

pub(super) fn draw_modal(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    modal: ModalScreen,
) {
    draw_modal_ui(draw, ui, modal);
}

#[allow(
    clippy::too_many_lines,
    reason = "exhaustive modal dispatch is intentionally centralized during UI migration"
)]
fn draw_modal_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>, modal: ModalScreen) {
    match modal {
        ModalScreen::Depot => counters::draw_depot(draw, ui),
        ModalScreen::DepotReceiptHistory => counters::draw_receipts(draw, ui),
        ModalScreen::ShopConfirm => counters::draw_shop_confirm(draw, ui),
        ModalScreen::Fuel => counters::draw_fuel(draw, ui),
        ModalScreen::Repair => counters::draw_repair(draw, ui),
        ModalScreen::FuelConfirm => counters::draw_fuel_confirm(draw, ui),
        ModalScreen::RepairConfirm => counters::draw_repair_confirm(draw, ui),
        ModalScreen::ExitConfirm => draw_confirm_modal_ui(
            draw,
            "Exit to Desktop?",
            "Enter/E exits. Esc returns to the game.",
        ),
        ModalScreen::UnsavedExitConfirm => draw_unsaved_exit_confirm_ui(draw, ui),
        ModalScreen::Shop => counters::draw_shop(draw, ui),
        ModalScreen::Options => draw_options_ui(draw, ui),
        ModalScreen::SaveSlots => draw_slots_ui(draw, ui, true),
        ModalScreen::LoadSlots => draw_slots_ui(draw, ui, false),
        ModalScreen::Bank => counters::draw_bank(draw, ui),
        ModalScreen::Explosives => counters::draw_explosives(draw, ui),
        ModalScreen::Salvage => counters::draw_salvage(draw, ui),
        ModalScreen::Headquarters => counters::draw_headquarters(draw, ui),
        ModalScreen::Crafting => counters::draw_crafting(draw, ui),
        ModalScreen::TownDevelopment => counters::draw_town_development(draw, ui),
        ModalScreen::ExpeditionBoard => counters::draw_expedition_board(draw, ui),
        ModalScreen::ResearchLog => draw_research_log_ui(draw, ui),
        ModalScreen::Inventory => draw_inventory_ui(draw, ui),
        ModalScreen::Map => draw_map_ui(draw, ui),
        ModalScreen::Help => draw_help_ui(draw),
    }
}

fn draw_confirm_modal_ui(draw: &mut RaylibDrawHandle<'_>, title: &str, body: &str) {
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        "Enter/E confirms | Esc closes",
        &ModalContent::new(vec![Section::new(
            "Confirm",
            GOLD,
            vec![SectionItem::text(body)],
        )])
        .compact(),
    );
}

fn draw_unsaved_exit_confirm_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    draw_option_modal(
        draw,
        ui,
        "Unsaved Progress",
        "Choose what to do before leaving the game.",
        &["Save and exit", "Exit without saving", "Cancel"],
    );
}

fn draw_options_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let options = [
        format!("Master volume: {:.0}%", ui.shell.master_volume * 100.0),
        format!(
            "Fullscreen: {}",
            if ui.shell.fullscreen { "on" } else { "off" }
        ),
        "Back".to_owned(),
    ];
    draw_owned_option_modal(
        draw,
        ui,
        "Options",
        "Up/Down select | Left/Right adjust | Esc close",
        &options,
    );
}

fn draw_slots_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>, saving: bool) {
    let options = (0..save_slot_count())
        .map(|slot| {
            save_slot_metadata(slot).map_or_else(
                || format!("Slot {} — empty", slot + 1),
                |metadata| {
                    format!(
                        "Slot {} — depth {}m, {} credits",
                        slot + 1,
                        metadata.depth,
                        metadata.credits
                    )
                },
            )
        })
        .collect::<Vec<_>>();
    draw_owned_option_modal(
        draw,
        ui,
        if saving { "Save Game" } else { "Load Game" },
        "Up/Down select | Enter/E confirm | Esc close",
        &options,
    );
}

fn draw_option_modal(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    title: &str,
    help: &str,
    options: &[&str],
) {
    let owned = options
        .iter()
        .map(|option| (*option).to_owned())
        .collect::<Vec<_>>();
    draw_owned_option_modal(draw, ui, title, help, &owned);
}

fn draw_owned_option_modal(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    title: &str,
    help: &str,
    options: &[String],
) {
    let items = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            SectionItem::stat(
                selected_label(index == ui.shell.selected_menu_item, option),
                "",
                if index == ui.shell.selected_menu_item {
                    GOLD
                } else {
                    BONE
                },
            )
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        help,
        &ModalContent::new(vec![Section::new("Options", SKY_LIGHT, items)]).compact(),
    );
}

fn selected_label(selected: bool, label: &str) -> String {
    if selected {
        format!("> {label}")
    } else {
        format!("  {label}")
    }
}

fn draw_research_log_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Research Log",
        "Collected discoveries and mining intelligence. Esc closes.",
        &ModalContent::new(vec![Section::new(
            "Discoveries",
            PINK,
            vec![
                SectionItem::stat(
                    "Warnings",
                    format!(
                        "{} | World events: {}",
                        ui.snapshot.warning_summary(ui.view),
                        ui.snapshot.world_event_summary()
                    ),
                    MIST,
                ),
                SectionItem::stat(
                    "Deep instability",
                    format!(
                        "{:.0}% | deepest reached: {}m",
                        ui.view.deep_instability.min(100.0),
                        ui.snapshot.progression.deepest_tile_reached
                    ),
                    MIST,
                ),
                SectionItem::text("Use the collection log and expedition board for detailed objectives while the UI migration continues to consolidate data models."),
            ],
        )]),
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "inventory content model builds several dynamic sections"
)]
fn draw_inventory_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut sections = Vec::new();
    sections.push(Section::new(
        "Cargo",
        GOLD,
        vec![
            SectionItem::meter(
                "Hold",
                ui.player.cargo_used() as f32,
                ui.player.cargo_capacity as f32,
                GOLD,
                ORANGE,
            ),
            SectionItem::stat("Credits", format!("{} cr", ui.player.credits), GOLD),
            SectionItem::stat("Bombs", ui.player.bombs.to_string(), ORANGE),
        ],
    ));
    let mineral_items = if ui.player.cargo.is_empty() {
        vec![SectionItem::text("No minerals onboard.")]
    } else {
        ui.player
            .cargo
            .iter()
            .map(|(mineral, quantity)| {
                let value = ui.snapshot.market_value(*mineral).saturating_mul(*quantity);
                SectionItem::icon_stat(
                    icons::mineral(*mineral),
                    format!("{} x{quantity}", mineral.name()),
                    format!("est. {value} cr"),
                    GOLD,
                )
            })
            .collect()
    };
    sections.push(Section::new("Minerals", BONE, mineral_items));
    let artifact_items = if ui.player.artifacts.is_empty() {
        vec![SectionItem::text("No artifacts onboard.")]
    } else {
        ui.player
            .artifacts
            .iter()
            .map(|(artifact, quantity)| {
                SectionItem::stat(
                    artifact.name(),
                    format!(
                        "x{quantity} | base {} cr",
                        artifact.value().saturating_mul(*quantity)
                    ),
                    PINK,
                )
            })
            .collect()
    };
    sections.push(Section::new("Artifacts", PINK, artifact_items));
    sections.push(Section::new(
        "Field Kits",
        LIME,
        [
            ("Signal relays", ui.player.signal_relay_kits),
            ("Survey drones", ui.player.survey_drone_kits),
            ("Cargo lifts", ui.player.cargo_lift_kits),
            ("Tunnel supports", ui.player.tunnel_support_kits),
            ("Pump stations", ui.player.pump_station_kits),
            ("Ore processors", ui.player.ore_processor_kits),
        ]
        .into_iter()
        .map(|(label, count)| {
            SectionItem::stat(label, count.to_string(), if count > 0 { LIME } else { FOG })
        })
        .collect(),
    ));
    sections.push(Section::new(
        "Rig",
        SKY_LIGHT,
        [
            ("Drill", ui.player.drill_strength),
            ("Engine", ui.player.engine_level),
            ("Fuel Tank", ui.player.fuel_tank_level),
            ("Cargo Bay", ui.player.cargo_bay_level),
            ("Hull", ui.player.hull_level),
            ("Radiator", ui.player.radiator_level),
            ("Scanner", ui.player.scanner_level),
        ]
        .into_iter()
        .map(|(label, tier)| SectionItem::stat(label, format!("tier {tier}"), MIST))
        .collect(),
    ));
    UiLayout::screen(draw).modal_with_render_plan(
        "Inventory",
        "Tab/Esc/Backspace closes | cargo, artifacts, consumables, and field kits",
        &ModalContent::new(sections),
    );
}

fn draw_map_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let map_rect = UiLayout::screen(draw).canvas_modal(
        "Mine Map",
        "M/Esc/Backspace closes | discovered terrain only",
        &format!(
            "Position: {:.0}m lateral, depth {}m | Deepest reached: {}m",
            ui.player.x / TILE_SIZE,
            (ui.player.y / TILE_SIZE).max(0.0) as i32,
            ui.snapshot.progression.deepest_tile_reached
        ),
    );
    let map_x = map_rect.x as i32;
    let map_y = map_rect.y as i32;
    let map_w = map_rect.width as i32;
    let map_h = map_rect.height as i32;
    draw.draw_rectangle(map_x, map_y, map_w, map_h, Color::new(12, 10, 14, 255));
    draw.draw_rectangle_lines(map_x, map_y, map_w, map_h, Color::new(190, 205, 220, 230));
    let terrain_width = ui.snapshot.terrain.width().max(1);
    let terrain_height = ui.snapshot.terrain.height().max(1);
    for ty in 0..terrain_height {
        for tx in 0..terrain_width {
            let position = TilePosition { x: tx, y: ty };
            if !ui.view.is_explored(position) {
                continue;
            }
            let Some(tile) = ui.snapshot.terrain.tile(position) else {
                continue;
            };
            let color = match tile.kind {
                TileKind::Air => Color::new(40, 42, 55, 255),
                TileKind::Foundation => Color::new(135, 125, 105, 255),
                TileKind::Lava | TileKind::MagmaVent => RED,
                TileKind::Gas => Color::GREEN,
                TileKind::ExplosivePocket => ORANGE,
                TileKind::PressurePocket => SKY_LIGHT,
                TileKind::Ore(_) => GOLD,
                TileKind::Artifact(_) => PINK,
                _ => Color::new(115, 82, 58, 255),
            };
            let px = map_x + tx * map_w / terrain_width;
            let py = map_y + ty * map_h / terrain_height;
            let pw = ((tx + 1) * map_w / terrain_width - tx * map_w / terrain_width).max(1);
            let ph = ((ty + 1) * map_h / terrain_height - ty * map_h / terrain_height).max(1);
            draw.draw_rectangle(px, py, pw, ph, color);
        }
    }
    let player_map_x = map_x + ((ui.player.x / TILE_SIZE) as i32) * map_w / terrain_width;
    let player_map_y = map_y + ((ui.player.y / TILE_SIZE) as i32) * map_h / terrain_height;
    draw.draw_circle(player_map_x, player_map_y, 5.0, SKY_LIGHT);
    draw.draw_circle_lines(player_map_x, player_map_y, 8.0, BONE);
}

fn draw_help_ui(draw: &mut RaylibDrawHandle<'_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Controls",
        "Esc/Backspace closes. Controls are context-sensitive.",
        &ModalContent::new(vec![
            Section::new(
                "Movement",
                SKY_LIGHT,
                [
                    "Move: A/D or Left/Right",
                    "Thrust: W or Up",
                    "Drill down: S or Down",
                    "Interact: E / Enter",
                    "Pause: P / Esc",
                ]
                .into_iter()
                .map(SectionItem::text)
                .collect(),
            ),
            Section::new(
                "Tools",
                GOLD,
                [
                    "Map: M",
                    "Inventory: Tab",
                    "Diagnostics: Ctrl+Tab",
                    "Scanner: C",
                    "Bomb: B",
                    "Infrastructure: R/T/L/U/O/P",
                    "Save/Load: F5/F9",
                ]
                .into_iter()
                .map(SectionItem::text)
                .collect(),
            ),
        ]),
    );
}

pub(super) fn draw_title(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut items = TitleOption::all(saves_exist())
        .iter()
        .enumerate()
        .map(|(index, option)| {
            SectionItem::stat(
                selected_label(index == ui.shell.selected_title_item, option.label()),
                "",
                if index == ui.shell.selected_title_item {
                    GOLD
                } else {
                    BONE
                },
            )
        })
        .collect::<Vec<_>>();
    if TitleOption::all(saves_exist()).contains(&TitleOption::Resume)
        && let Some(meta) = latest_save_summary()
    {
        items.push(SectionItem::text(format!(
            "Last save: depth {}m | {} cr | {:.0} min",
            meta.depth,
            meta.credits,
            (meta.play_seconds / 60.0).floor()
        )));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        "DRILLGAME",
        "A frontier mining run awaits below. Up/Down select | Enter/E confirm | Esc exits | F11 fullscreen",
        &ModalContent::new(vec![Section::new("Menu", SKY_LIGHT, items)]).compact(),
    );
}

pub(super) fn draw_pause(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut items = PauseOption::for_role(ui.shell.online.role)
        .iter()
        .enumerate()
        .map(|(index, option)| {
            SectionItem::stat(
                selected_label(index == ui.shell.selected_pause_item, option.label()),
                "",
                if index == ui.shell.selected_pause_item {
                    GOLD
                } else {
                    BONE
                },
            )
        })
        .collect::<Vec<_>>();
    items.push(SectionItem::text(format!(
        "Depth {}m | {} credits | hull {:.0}/{:.0} | fuel {:.0}/{:.0}",
        (ui.player.y / TILE_SIZE).max(0.0) as i32,
        ui.player.credits,
        ui.player.hull,
        ui.player.max_hull(),
        ui.player.fuel,
        ui.player.fuel_capacity
    )));
    if ui.snapshot.won_game {
        items.push(SectionItem::text(
            "Star Core secured. Deep Claim operations are open.",
        ));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        "Paused",
        "Up/Down select | Enter/E confirm | Esc resume",
        &ModalContent::new(vec![Section::new("Menu", SKY_LIGHT, items)]).compact(),
    );
}

pub(super) fn draw_game_over(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Emergency",
        "Press E to pay the rescue fee and return to base.",
        &ModalContent::new(vec![Section::new(
            "Status",
            RED,
            vec![
                SectionItem::text(match ui.view.failure {
                    Some(failure) if failure.disabled => {
                        "Your rig's hull gave out. Call the rescue crew.".to_owned()
                    }
                    Some(_) => "Your rig is out of fuel. Call the rescue crew.".to_owned(),
                    None => ui.shell.message.clone(),
                }),
                SectionItem::stat("Rescues called", ui.view.rescue_count.to_string(), MIST),
            ],
        )])
        .warning()
        .compact(),
    );
}

fn option_items(options: &[String], selected: usize) -> Vec<SectionItem> {
    options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            SectionItem::stat(
                selected_label(index == selected, option),
                "",
                if index == selected { GOLD } else { BONE },
            )
        })
        .collect()
}

/// Lobby roster rows shown before the rest condense into a count, so any player count fits
/// (plan §15).
const MAX_ROSTER_ROWS: usize = 10;

/// Online screens: join forms, LAN list, lobby roster and session notices. Drawn without a
/// world view (a joining client has no rig until the host's welcome arrives).
#[allow(
    clippy::too_many_lines,
    reason = "one screen per online state keeps the online UI in one place"
)]
pub(super) fn draw_online(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState) {
    let online = &shell.online;
    let host = online.role == Some(OnlineRole::Host);
    let (title, help, sections) = match online.screen {
        OnlineScreen::JoinMenu => (
            "Join Online Game".to_owned(),
            "Up/Down select | Enter/E confirm | Esc back",
            vec![Section::new(
                "Join",
                SKY_LIGHT,
                option_items(
                    &JOIN_MENU
                        .iter()
                        .map(|label| (*label).to_owned())
                        .collect::<Vec<_>>(),
                    online.selected,
                ),
            )],
        ),
        OnlineScreen::JoinAddress => (
            "Join by Address".to_owned(),
            "Type the host's ip:port | Enter join | Backspace delete | Esc back",
            vec![Section::new(
                "Host address",
                SKY_LIGHT,
                vec![
                    SectionItem::stat(format!("{}_", online.text), "", GOLD),
                    SectionItem::text(
                        "Direct-IP joins are encrypted but do not authenticate the host; use a descriptor to pin its certificate.",
                    ),
                ],
            )],
        ),
        OnlineScreen::JoinDescriptor => {
            let preview: String = online.text.chars().take(64).collect();
            (
                "Join by Descriptor".to_owned(),
                "Ctrl+V paste the host's descriptor | Enter join | Backspace delete | Esc back",
                vec![Section::new(
                    "Descriptor",
                    SKY_LIGHT,
                    vec![if online.text.is_empty() {
                        SectionItem::text(
                            "The host copies it from its lobby (\"Copy descriptor to clipboard\").",
                        )
                    } else {
                        SectionItem::stat(
                            format!("{preview}..."),
                            format!("{} characters", online.text.len()),
                            GOLD,
                        )
                    }],
                )],
            )
        }
        OnlineScreen::LanBrowser => {
            let mut items = vec![SectionItem::text(online.lan_status.clone())];
            items.extend(option_items(&online.lan_options(), online.selected));
            (
                "LAN Games".to_owned(),
                "Up/Down select | Enter/E join | Esc back",
                vec![Section::new("Discovered sessions", SKY_LIGHT, items)],
            )
        }
        OnlineScreen::Lobby => {
            let lobby = online.lobby.clone().unwrap_or_default();
            let mut session = vec![SectionItem::text(lobby.status.clone())];
            if let Some(addr) = lobby.addr {
                session.push(SectionItem::stat("Address", addr.to_string(), GOLD));
            }
            if host {
                session.push(SectionItem::stat(
                    "LAN discovery",
                    if lobby.lan_published {
                        "advertised"
                    } else {
                        "not advertised"
                    },
                    MIST,
                ));
                session.push(SectionItem::stat(
                    "Descriptor",
                    if lobby.descriptor.is_some() {
                        "ready to copy"
                    } else {
                        "pending"
                    },
                    MIST,
                ));
            }
            if !shell.message.is_empty() {
                session.push(SectionItem::text(shell.message.clone()));
            }
            let players = if lobby.roster.is_empty() {
                vec![SectionItem::text("Waiting for the roster...")]
            } else {
                let hidden = lobby.roster.len().saturating_sub(MAX_ROSTER_ROWS);
                let mut rows: Vec<SectionItem> = lobby
                    .roster
                    .iter()
                    .take(MAX_ROSTER_ROWS)
                    .map(|entry| {
                        let mut name = entry.name.clone();
                        if lobby.local_player == Some(entry.player) {
                            name.push_str(" (you)");
                        } else if entry.local {
                            name.push_str(" (host)");
                        }
                        let (state, color) = if !entry.connected {
                            ("disconnected", ORANGE)
                        } else if entry.local || entry.ready {
                            ("ready", LIME)
                        } else {
                            ("not ready", MIST)
                        };
                        SectionItem::stat(name, state, color)
                    })
                    .collect();
                if hidden > 0 {
                    rows.push(SectionItem::text(format!("... and {hidden} more")));
                }
                rows
            };
            let options: Vec<String> = online
                .lobby_options()
                .iter()
                .map(|action| action.label(host).to_owned())
                .collect();
            (
                if host {
                    "Hosting Online Game".to_owned()
                } else {
                    "Online Lobby".to_owned()
                },
                if lobby.started {
                    "Up/Down select | Enter/E confirm | Esc back to the game"
                } else {
                    "Up/Down select | Enter/E confirm"
                },
                vec![
                    Section::new("Session", SKY_LIGHT, session),
                    Section::new(format!("Players ({})", lobby.roster.len()), GOLD, players),
                    Section::new("Menu", SKY_LIGHT, option_items(&options, online.selected)),
                ],
            )
        }
        OnlineScreen::Notice => {
            let options: Vec<String> = online
                .notice_options()
                .iter()
                .map(|action| action.label().to_owned())
                .collect();
            let mut items = vec![SectionItem::text(online.notice.clone())];
            items.extend(option_items(&options, online.selected));
            (
                "Online Session".to_owned(),
                "Up/Down select | Enter/E confirm | Esc title",
                vec![Section::new("Session over", ORANGE, items)],
            )
        }
    };
    UiLayout::screen(draw).modal_with_render_plan(&title, help, &ModalContent::new(sections));
}
