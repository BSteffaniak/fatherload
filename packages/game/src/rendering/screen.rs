use raylib::prelude::*;

use drillgame_simulation::{
    Player,
    economy::{upgrade_offers, upgrade_tier_name},
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::layout::{HudCard, ModalContent, Section, SectionItem, StatItem, UiLayout};
use crate::{
    render_snapshot::{PlayerPresentation, RenderSnapshot, ViewSnapshot},
    save::{latest_save_summary, save_slot_count, save_slot_metadata, saves_exist},
    shell::{ModalScreen, PauseOption, ShellState, TitleOption},
};

/// What screen-space UI (HUD, menus, modals) reads: shell state plus the snapshot and the view
/// the UI acts for.
pub(super) struct UiFrame<'a, 'w> {
    pub shell: &'a ShellState,
    pub snapshot: &'a RenderSnapshot<'w>,
    pub view: &'a ViewSnapshot<'w>,
    pub player: &'w Player,
}

struct MinimapProjection {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    terrain_width: i32,
    origin_y: i32,
    visible_height: i32,
}

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
    if depth_heat || near_lava {
        let alpha = if near_lava { 70 } else { 35 };
        draw.draw_rectangle(
            view.viewport.x,
            view.viewport.y,
            view.viewport.width,
            view.viewport.height,
            Color::new(255, 70, 20, alpha),
        );
    }
}

pub(super) fn draw_view_hud(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
) {
    let fps = draw.get_fps();
    let hud = view.hud;
    let viewport = Rectangle {
        x: view.viewport.x as f32,
        y: view.viewport.y as f32,
        width: view.viewport.width as f32,
        height: view.viewport.height as f32,
    };
    let cards = vec![
        HudCard::meter(
            format!("P{} Hull", view.seat_index + 1),
            hud.hull,
            hud.max_hull,
            Color::SKYBLUE,
            Color::RED,
        ),
        HudCard::meter(
            "Fuel",
            hud.fuel,
            hud.fuel_capacity,
            Color::LIME,
            Color::ORANGE,
        ),
        HudCard::meter(
            "Cargo",
            hud.cargo_used as f32,
            hud.cargo_capacity as f32,
            Color::GOLD,
            Color::ORANGE,
        ),
        HudCard::text(
            "Run",
            format!("{} cr | {}m", hud.credits, hud.depth),
            Color::GOLD,
        ),
    ];
    let details = shell.show_details.then(|| {
        vec![
            StatItem::new("FPS", fps.to_string(), Color::LIME),
            StatItem::new(
                "Tick",
                format!("{} ({} new events)", snapshot.tick, snapshot.events.len()),
                Color::LIGHTGRAY,
            ),
            StatItem::new(
                "Camera",
                format!(
                    "{:.0},{:.0}",
                    view.camera.x / TILE_SIZE,
                    view.camera.y / TILE_SIZE
                ),
                Color::LIGHTGRAY,
            ),
            StatItem::new("Message", shell.message.clone(), Color::LIGHTGRAY),
        ]
    });
    let mut layout = UiLayout::new(draw, viewport);
    layout.top_hud(&cards, details.as_deref());
}

pub(super) fn draw_minimap_for_view(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
    players: &[PlayerPresentation],
) {
    let width = (view.viewport.width / 6).clamp(86, 130);
    let height = (view.viewport.height / 8).clamp(64, 96);
    let x = view.viewport.x + view.viewport.width - width - 18;
    let y = view.viewport.y + view.viewport.height - height - 38;
    draw.draw_rectangle(
        x - 8,
        y - 8,
        width + 16,
        height + 16,
        Color::new(0, 0, 0, 150),
    );

    let terrain_width = terrain.width().max(1);
    let terrain_height = terrain.height().max(1);
    let player_tile = view.player.tile_position(TILE_SIZE);
    let visible_height = terrain_height.min(80);
    let origin_y = if terrain_height > visible_height {
        (player_tile.y - visible_height / 2).clamp(0, terrain_height - visible_height)
    } else {
        0
    };
    let projection = MinimapProjection {
        x,
        y,
        width,
        height,
        terrain_width,
        origin_y,
        visible_height,
    };
    for ty in origin_y..origin_y + visible_height {
        for tx in 0..terrain_width {
            let Some(tile) = terrain.tile(TilePosition { x: tx, y: ty }) else {
                continue;
            };
            let color = marker_color(tile.kind);
            if color.a > 0 {
                draw_map_marker(draw, &projection, tx, ty, color);
            }
        }
    }
    let player_x = x + player_tile.x * width / terrain_width;
    let player_y = y + (player_tile.y - origin_y) * height / visible_height;
    draw.draw_circle(player_x, player_y, 3.0, Color::SKYBLUE);
    for remote in players
        .iter()
        .filter(|other| other.player_id != view.player_id)
    {
        let remote_tile_x = (remote.x / TILE_SIZE) as i32;
        let remote_tile_y = (remote.y / TILE_SIZE) as i32;
        if remote_tile_y < origin_y || remote_tile_y >= origin_y + visible_height {
            continue;
        }
        let remote_x = x + remote_tile_x * width / terrain_width;
        let remote_y = y + (remote_tile_y - origin_y) * height / visible_height;
        draw.draw_circle(remote_x, remote_y, 3.0, Color::ORANGE);
        if remote.velocity_x.abs() > f32::EPSILON || remote.velocity_y.abs() > f32::EPSILON {
            let speed = (remote
                .velocity_x
                .mul_add(remote.velocity_x, remote.velocity_y * remote.velocity_y))
            .sqrt();
            let direction_x = remote.velocity_x / speed;
            let direction_y = remote.velocity_y / speed;
            draw.draw_line(
                remote_x,
                remote_y,
                remote_x + (direction_x * 8.0) as i32,
                remote_y + (direction_y * 8.0) as i32,
                Color::GOLD,
            );
        }
    }
}

pub(super) fn draw_depth_ruler_for_view(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
) {
    let x = view.viewport.x + view.viewport.width - 20;
    let top = view.viewport.y + 72;
    let height = (view.viewport.height - 130).max(80);
    draw.draw_rectangle(x, top, 10, height, Color::new(0, 0, 0, 120));
    draw.draw_rectangle_lines(x, top, 10, height, Color::new(255, 255, 255, 120));

    let max_depth = terrain.height().max(80);
    let step = if max_depth > 180 { 50 } else { 20 };
    for marker in (0..=max_depth).step_by(step as usize) {
        let y = top + (marker * height / max_depth);
        draw.draw_line(x - 8, y, x + 18, y, Color::LIGHTGRAY);
    }

    let depth = (view.player.y / TILE_SIZE).max(0.0);
    let marker_y = top + ((depth / max_depth as f32).clamp(0.0, 1.0) * height as f32) as i32;
    draw.draw_circle(x + 5, marker_y, 6.0, Color::GOLD);
}

const fn marker_color(kind: TileKind) -> Color {
    match kind {
        TileKind::Ore(_) => Color::GOLD,
        TileKind::Artifact(_) => Color::MAGENTA,
        TileKind::Gas
        | TileKind::Lava
        | TileKind::MagmaVent
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket => Color::RED,
        _ => Color::WHITE,
    }
}

fn draw_map_marker(
    draw: &mut RaylibDrawHandle<'_>,
    projection: &MinimapProjection,
    tile_x: i32,
    tile_y: i32,
    color: Color,
) {
    if tile_y < projection.origin_y || tile_y >= projection.origin_y + projection.visible_height {
        return;
    }
    let x = projection.x + tile_x * projection.width / projection.terrain_width;
    let y = projection.y
        + (tile_y - projection.origin_y) * projection.height / projection.visible_height;
    draw.draw_rectangle(x - 1, y - 1, 3, 3, color);
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
        ModalScreen::Depot => draw_modal_depot_ui(draw, ui),
        ModalScreen::DepotReceiptHistory => draw_depot_receipt_history_ui(draw, ui),
        ModalScreen::ShopConfirm => draw_confirm_modal_ui(
            draw,
            "Confirm Upgrade Purchase",
            "Enter/E buys the selected upgrade. Esc returns to the shop.",
        ),
        ModalScreen::Fuel => draw_service_modal_ui(draw, ui, "Fuel Station", "fuel"),
        ModalScreen::Repair => draw_service_modal_ui(draw, ui, "Repair Garage", "hull"),
        ModalScreen::FuelConfirm => draw_confirm_modal_ui(
            draw,
            "Confirm Fuel Purchase",
            "Enter/E confirms the selected refuel amount. Esc cancels.",
        ),
        ModalScreen::RepairConfirm => draw_confirm_modal_ui(
            draw,
            "Confirm Repair",
            "Enter/E confirms the selected hull repair. Esc cancels.",
        ),
        ModalScreen::ExitConfirm => draw_confirm_modal_ui(
            draw,
            "Exit to Desktop?",
            "Enter/E exits. Esc returns to the game.",
        ),
        ModalScreen::UnsavedExitConfirm => draw_unsaved_exit_confirm_ui(draw, ui),
        ModalScreen::Shop => draw_shop_ui(draw, ui),
        ModalScreen::Options => draw_options_ui(draw, ui),
        ModalScreen::SaveSlots => draw_slots_ui(draw, ui, true),
        ModalScreen::LoadSlots => draw_slots_ui(draw, ui, false),
        ModalScreen::Bank => draw_generic_options_ui(
            draw,
            ui,
            "Bank",
            "Manage deposits and debt. Esc closes.",
            &["Deposit credits", "Withdraw credits", "Pay debt"],
        ),
        ModalScreen::Explosives => draw_generic_options_ui(
            draw,
            ui,
            "Explosives Shack",
            "Purchase mining explosives and blast tools. Esc closes.",
            &[
                "Small bomb pack",
                "Standard bomb pack",
                "Heavy blast charge",
                "Safety permit",
            ],
        ),
        ModalScreen::Salvage => draw_generic_options_ui(
            draw,
            ui,
            "Salvage Yard",
            "Buy and deploy field infrastructure. Esc closes.",
            &[
                "Signal relay kit",
                "Survey drone kit",
                "Cargo lift kit",
                "Rock support kit",
                "Pump kit",
                "Processor kit",
            ],
        ),
        ModalScreen::Headquarters => draw_generic_options_ui(
            draw,
            ui,
            "Headquarters",
            "Contracts, story briefings, finance, and expedition planning.",
            &[
                "Complete depot work",
                "Read briefing",
                "Request finance",
                "Open expedition board",
                "Research log",
                "Town development",
                "Deep claim status",
            ],
        ),
        ModalScreen::Crafting => draw_generic_options_ui(
            draw,
            ui,
            "Crafting Bench",
            "Turn recovered materials into support equipment.",
            &[
                "Relay parts",
                "Drone parts",
                "Lift frame",
                "Support brace",
                "Pump assembly",
                "Processor assembly",
            ],
        ),
        ModalScreen::TownDevelopment => draw_generic_options_ui(
            draw,
            ui,
            "Town Development",
            "Invest in settlement upgrades and local services.",
            &[
                "Depot expansion",
                "Fuel cooperative",
                "Repair garage",
                "Research hall",
                "Residential blocks",
                "Trade office",
            ],
        ),
        ModalScreen::ExpeditionBoard => draw_generic_options_ui(
            draw,
            ui,
            "Expedition Board",
            "Review active expeditions and available claims.",
            &[
                "Accept first offer",
                "Accept second offer",
                "Review active expedition",
                "Collect completed expedition",
            ],
        ),
        ModalScreen::ResearchLog => draw_research_log_ui(draw, ui),
        ModalScreen::Inventory => draw_inventory_ui(draw, ui),
        ModalScreen::Map => draw_map_ui(draw, ui),
        ModalScreen::Help => draw_help_ui(draw),
    }
}

fn draw_service_modal_ui(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    title: &str,
    service: &str,
) {
    let mut items = Vec::new();
    let hud = ui.view.hud;
    if service == "fuel" {
        let (fuel, capacity, credits) = (hud.fuel, hud.fuel_capacity, hud.credits);
        let missing = (capacity - fuel).ceil().max(0.0) as u32;
        items.push(SectionItem::text(format!(
            "Tank: {fuel:.0}/{capacity:.0}. Fill cost: {missing} cr. Credits: {credits}."
        )));
    } else {
        let (hull, capacity, credits) = (hud.hull, hud.max_hull, hud.credits);
        items.push(SectionItem::text(format!(
            "Hull: {hull:.0}/{capacity:.0}. Credits available: {credits}."
        )));
    }
    for (index, option) in ["Small 25%", "Half 50%", "Full 100%"].iter().enumerate() {
        items.push(SectionItem::stat(
            selected_label(index == ui.shell.selected_menu_item, option),
            "",
            if index == ui.shell.selected_menu_item {
                Color::GOLD
            } else {
                Color::RAYWHITE
            },
        ));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        "Up/Down: small/half/full | Enter/E buy selected | Esc close",
        &ModalContent::new(vec![Section::new("Service", Color::SKYBLUE, items)]),
    );
}

fn draw_confirm_modal_ui(draw: &mut RaylibDrawHandle<'_>, title: &str, body: &str) {
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        "Enter/E confirms | Esc closes",
        &ModalContent::new(vec![Section::new(
            "Confirm",
            Color::GOLD,
            vec![SectionItem::text(body)],
        )]),
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

fn draw_shop_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let items = upgrade_offers(ui.player)
        .iter()
        .enumerate()
        .map(|(index, offer)| {
            SectionItem::stat(
                selected_label(
                    index == ui.shell.selected_menu_item,
                    &format!("{} — {} cr", offer.name, offer.cost),
                ),
                format!(
                    "Tier {} {}",
                    offer.level + 1,
                    upgrade_tier_name(offer.kind, offer.level)
                ),
                if index == ui.shell.selected_menu_item {
                    Color::GOLD
                } else {
                    Color::RAYWHITE
                },
            )
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "Upgrade Shop",
        "Up/Down select | Enter/E buy | Esc close",
        &ModalContent::new(vec![Section::new(
            "Available Upgrades",
            Color::SKYBLUE,
            items,
        )]),
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

fn draw_generic_options_ui(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    title: &str,
    help: &str,
    options: &[&str],
) {
    draw_option_modal(draw, ui, title, help, options);
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
                    Color::GOLD
                } else {
                    Color::RAYWHITE
                },
            )
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        title,
        help,
        &ModalContent::new(vec![Section::new("Options", Color::SKYBLUE, items)]),
    );
}

fn selected_label(selected: bool, label: &str) -> String {
    if selected {
        format!("> {label}")
    } else {
        format!("  {label}")
    }
}

fn draw_depot_receipt_history_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let items = if ui.shell.depot_receipts.is_empty() {
        vec![SectionItem::text("No depot receipts yet.")]
    } else {
        ui.shell
            .depot_receipts
            .iter()
            .rev()
            .take(10)
            .enumerate()
            .flat_map(|(index, receipt)| {
                let mut items = vec![SectionItem::stat(
                    format!("Receipt {}", index + 1),
                    "",
                    Color::GOLD,
                )];
                items.extend(
                    receipt
                        .lines()
                        .take(5)
                        .map(|line| SectionItem::text(line.to_owned())),
                );
                items
            })
            .collect()
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Depot Receipts",
        "Recent sales and contract payouts. Esc closes.",
        &ModalContent::new(vec![Section::new("History", Color::GOLD, items)]),
    );
}

fn draw_research_log_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Research Log",
        "Collected discoveries and mining intelligence. Esc closes.",
        &ModalContent::new(vec![Section::new(
            "Discoveries",
            Color::MAGENTA,
            vec![
                SectionItem::stat(
                    "Warnings",
                    format!(
                        "{} | World events: {}",
                        ui.snapshot.warning_summary(ui.view),
                        ui.snapshot.world_event_summary()
                    ),
                    Color::LIGHTGRAY,
                ),
                SectionItem::stat(
                    "Deep instability",
                    format!(
                        "{:.0}% | deepest reached: {}m",
                        ui.view.deep_instability.min(100.0),
                        ui.snapshot.progression.deepest_tile_reached
                    ),
                    Color::LIGHTGRAY,
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
        Color::GOLD,
        vec![
            SectionItem::meter(
                "Hold",
                ui.player.cargo_used() as f32,
                ui.player.cargo_capacity as f32,
                Color::GOLD,
                Color::ORANGE,
            ),
            SectionItem::stat("Credits", format!("{} cr", ui.player.credits), Color::GOLD),
            SectionItem::stat("Bombs", ui.player.bombs.to_string(), Color::ORANGE),
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
                SectionItem::stat(
                    mineral.name(),
                    format!("x{quantity} | est. {value} cr"),
                    Color::RAYWHITE,
                )
            })
            .collect()
    };
    sections.push(Section::new("Minerals", Color::RAYWHITE, mineral_items));
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
                    Color::MAGENTA,
                )
            })
            .collect()
    };
    sections.push(Section::new("Artifacts", Color::MAGENTA, artifact_items));
    sections.push(Section::new(
        "Field Kits",
        Color::LIME,
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
            SectionItem::stat(
                label,
                count.to_string(),
                if count > 0 { Color::LIME } else { Color::GRAY },
            )
        })
        .collect(),
    ));
    sections.push(Section::new(
        "Rig",
        Color::SKYBLUE,
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
        .map(|(label, tier)| SectionItem::stat(label, format!("tier {tier}"), Color::LIGHTGRAY))
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
                TileKind::Lava | TileKind::MagmaVent => Color::RED,
                TileKind::Gas => Color::GREEN,
                TileKind::ExplosivePocket => Color::ORANGE,
                TileKind::PressurePocket => Color::SKYBLUE,
                TileKind::Ore(_) => Color::GOLD,
                TileKind::Artifact(_) => Color::MAGENTA,
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
    draw.draw_circle(player_map_x, player_map_y, 5.0, Color::SKYBLUE);
    draw.draw_circle_lines(player_map_x, player_map_y, 8.0, Color::RAYWHITE);
}

fn draw_help_ui(draw: &mut RaylibDrawHandle<'_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Controls",
        "Esc/Backspace closes. Controls are context-sensitive.",
        &ModalContent::new(vec![
            Section::new(
                "Movement",
                Color::SKYBLUE,
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
                Color::GOLD,
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

#[allow(
    clippy::too_many_lines,
    reason = "depot modal builds several dynamic semantic sections"
)]
fn draw_modal_depot_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let option_data = [
        (
            "Complete active contract".to_owned(),
            format!(
                "{}: {}/{} {} for {} credits",
                ui.snapshot.progression.contracts.active.title,
                ui.snapshot.progression.contracts.active.progress(ui.player),
                ui.snapshot.progression.contracts.active.required,
                ui.snapshot.progression.contracts.active.target.name(),
                ui.snapshot.progression.contracts.active.reward
            ),
        ),
        (
            "Sell loose cargo".to_owned(),
            format!(
                "Cargo manifest: {}/{} slots",
                ui.player.cargo_used(),
                ui.player.cargo_capacity
            ),
        ),
        (
            "Auto-sort low-grade cargo".to_owned(),
            "Keep valuable ores while clearing cheap cargo from the hold.".to_owned(),
        ),
        (
            "Sell scan data".to_owned(),
            format!("Market: {}", ui.snapshot.world_event_summary()),
        ),
        (
            "Receipt history".to_owned(),
            format!("{} saved receipts", ui.shell.depot_receipts.len()),
        ),
    ];
    let options = option_data
        .iter()
        .enumerate()
        .map(|(index, (label, detail))| {
            SectionItem::stat(
                selected_label(index == ui.shell.selected_menu_item, label),
                detail,
                if index == ui.shell.selected_menu_item {
                    Color::GOLD
                } else {
                    Color::RAYWHITE
                },
            )
        })
        .collect();
    let cargo_items = if ui.player.cargo_used() == 0 {
        vec![SectionItem::text("Cargo hold empty")]
    } else {
        ui.player
            .cargo
            .iter()
            .map(|(mineral, count)| {
                SectionItem::stat(
                    mineral.name(),
                    format!("x{count} = {} cr", mineral.value() * count),
                    Color::RAYWHITE,
                )
            })
            .chain(ui.player.artifacts.iter().map(|(artifact, count)| {
                SectionItem::stat(
                    artifact.name(),
                    format!("x{count} = {} cr", artifact.value() * count),
                    Color::MAGENTA,
                )
            }))
            .collect()
    };
    let minerals = crate::render_snapshot::MINERALS;
    let market_items = minerals
        .into_iter()
        .map(|mineral| {
            let quote = ui
                .snapshot
                .market
                .iter()
                .find(|quote| quote.mineral == mineral)
                .copied();
            let current = quote.map_or(100, |quote| quote.factor);
            let previous = quote
                .and_then(|quote| quote.previous_factor)
                .unwrap_or(current);
            let trend = match current.cmp(&previous) {
                std::cmp::Ordering::Greater => "↑",
                std::cmp::Ordering::Less => "↓",
                std::cmp::Ordering::Equal => "→",
            };
            let label = if current >= 120 {
                "high"
            } else if current <= 90 {
                "low"
            } else {
                "avg"
            };
            SectionItem::stat(
                mineral.name(),
                format!("{} cr {trend} {label}", ui.snapshot.market_value(mineral)),
                Color::LIGHTGRAY,
            )
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "Ore Depot",
        "Up/Down select | Enter/E confirm | Esc/Backspace close",
        &ModalContent::new(vec![
            Section::new("Depot Actions", Color::SKYBLUE, options),
            Section::new("Current Cargo", Color::GOLD, cargo_items),
            Section::new("Market Snapshot", Color::LIME, market_items),
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
                    Color::GOLD
                } else {
                    Color::RAYWHITE
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
        &ModalContent::new(vec![Section::new("Menu", Color::SKYBLUE, items)]),
    );
}

pub(super) fn draw_pause(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut items = PauseOption::ALL
        .iter()
        .enumerate()
        .map(|(index, option)| {
            SectionItem::stat(
                selected_label(index == ui.shell.selected_pause_item, option.label()),
                "",
                if index == ui.shell.selected_pause_item {
                    Color::GOLD
                } else {
                    Color::RAYWHITE
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
        &ModalContent::new(vec![Section::new("Menu", Color::SKYBLUE, items)]),
    );
}

pub(super) fn draw_game_over(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Emergency",
        "Press E to pay the rescue fee and return to base.",
        &ModalContent::new(vec![Section::new(
            "Status",
            Color::RED,
            vec![
                SectionItem::text(match ui.view.failure {
                    Some(failure) if failure.disabled => {
                        "Your rig's hull gave out. Call the rescue crew.".to_owned()
                    }
                    Some(_) => "Your rig is out of fuel. Call the rescue crew.".to_owned(),
                    None => ui.shell.message.clone(),
                }),
                SectionItem::stat(
                    "Rescues called",
                    ui.view.rescue_count.to_string(),
                    Color::LIGHTGRAY,
                ),
            ],
        )]),
    );
}
