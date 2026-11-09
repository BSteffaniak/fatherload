use raylib::prelude::*;

use drillgame_simulation::{
    DiscoveredTiles, Player,
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
    menus,
    pixel_art::{
        palette,
        sprite::SpriteDef,
        sprites::{icons, menu_icons, rig},
    },
    prompts::{self, hint},
    ui_kit::{self, BarColors, PanelStyle, TextStyle},
    world::player_accent,
};
use crate::{
    input::{devices::InputDevice, prompts::Prompt},
    render_snapshot::{PlayerPresentation, RenderSnapshot, ViewSnapshot},
    shell::{ModalScreen, RunMode, SeatUi, ShellState, surface_zone_label},
};

/// What screen-space UI (HUD, menus, modals) reads: shell state plus the snapshot, the view the
/// UI acts for and that view's seat menus.
pub(super) struct UiFrame<'a, 'w> {
    pub shell: &'a ShellState,
    /// The seat the view belongs to (counter panels, selection, receipts).
    pub seat: &'a SeatUi,
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
            player_accent(view.player_id),
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

    draw_wallet_panel(draw, view, rect, left + width + 4, top);
    draw_view_prompt(draw, shell, view, rect);

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

/// The view's context prompt under the rig, for its seat's device: how to enter the building
/// the rig stands at, or how a controller joins a seat nobody drives yet.
fn draw_view_prompt(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let device = shell.prompt_device(view.seat_index);
    let (prompt, label, accent) = if device == InputDevice::Unassigned {
        (
            Prompt::Join,
            format!("to join as Player {}", view.seat_index + 1),
            false,
        )
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && view.failure.is_none()
        && let Some(zone) = view.surface_zone
    {
        (
            Prompt::Use,
            format!("Enter {}", surface_zone_label(zone)),
            true,
        )
    } else {
        return;
    };
    let action = hint(prompt, "");
    let Some(glyphs) = prompts::prompt_width(action, device) else {
        return;
    };
    let label = ui_kit::truncate(&label, rect.width as i32 - glyphs - 40, TextStyle::Bold);
    let width = glyphs + 4 + ui_kit::text_width(&label, TextStyle::Bold) + 12;
    let x = (rect.x + rect.width * 0.5) as i32 - width / 2;
    // Just under the rig (views centre their rig); a waiting seat's prompt sits above it.
    let centre_y = (rect.y + rect.height * 0.5) as i32;
    let y = if accent { centre_y + 16 } else { centre_y - 38 };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, 19.0),
        if accent {
            PanelStyle::Accent
        } else {
            PanelStyle::Standard
        },
    );
    let drawn = prompts::draw_prompt(draw, x + 6, y + 4, action, device, BONE);
    ui_kit::text(
        draw,
        &label,
        x + 6 + drawn + 4,
        y + 6,
        TextStyle::Bold,
        if accent { GOLD } else { BONE },
    );
}

/// Credits and depth (top-right, clear of the depth ruler), plus the scanner and bomb chips.
fn draw_wallet_panel(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    min_x: i32,
    top: i32,
) {
    let hud = view.hud;
    let money = format!("{}", hud.credits);
    let depth = format!("{} m", hud.depth);
    let has_scanner = view.player.scanner_level > 0;
    let scan = if hud.scanner_cooldown_seconds > 0.05 {
        format!("{:.0}s", hud.scanner_cooldown_seconds.ceil())
    } else {
        "READY".to_owned()
    };
    let bombs = format!("{}", view.player.bombs);
    let chips_width = ui_kit::text_width(&bombs, TextStyle::Bold)
        + icons::ICON_SIZE
        + 3
        + if has_scanner {
            ui_kit::text_width(&scan, TextStyle::Bold) + icons::ICON_SIZE + 9
        } else {
            0
        };
    let value_width = ui_kit::text_width(&money, TextStyle::Bold)
        .max(ui_kit::text_width(&depth, TextStyle::Bold))
        + icons::ICON_SIZE
        + 6;
    let panel_width = value_width.max(chips_width) + ui_kit::PANEL_PADDING * 2;
    let panel_width = panel_width.max(64);
    let x = (rect.x + rect.width) as i32 - 16 - panel_width;
    if x <= min_x {
        return;
    }
    let height = 45;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, panel_width as f32, height as f32),
        PanelStyle::Standard,
    );
    let right = x + panel_width - ui_kit::PANEL_PADDING;
    ui_kit::icon(draw, &icons::CREDITS, x + 6, top + 6);
    ui_kit::text_right(draw, &money, right, top + 7, TextStyle::Bold, GOLD);
    ui_kit::icon(draw, &icons::DEPTH, x + 6, top + 17);
    ui_kit::text_right(draw, &depth, right, top + 18, TextStyle::Bold, SKY_LIGHT);
    ui_kit::divider(draw, x + 4, top + 28, panel_width - 8);
    // Tool chips: bombs on the left, the scanner's cooldown on the right.
    let chip_y = top + 32;
    ui_kit::icon(draw, &icons::BOMB, x + 6, chip_y);
    ui_kit::text(
        draw,
        &bombs,
        x + 6 + icons::ICON_SIZE + 2,
        chip_y + 1,
        TextStyle::Bold,
        if view.player.bombs > 0 { ORANGE } else { FOG },
    );
    if has_scanner {
        let ready = scan == "READY";
        let scan_width = ui_kit::text_width(&scan, TextStyle::Bold);
        ui_kit::text_right(
            draw,
            &scan,
            right,
            chip_y + 1,
            TextStyle::Bold,
            if ready { MINT } else { FOG },
        );
        ui_kit::icon(
            draw,
            &icons::SCANNER,
            right - scan_width - icons::ICON_SIZE - 2,
            chip_y,
        );
    }
}

/// How long a message stays on screen as a toast, in shell ticks.
const TOAST_TICKS: u64 = 240;

/// The message a view's toast shows: the newer of the global message and the seat's own, while
/// it is younger than [`TOAST_TICKS`]. Returns the text and its age in shell ticks.
pub(super) fn toast_message<'a>(shell: &'a ShellState, seat: &'a SeatUi) -> Option<(&'a str, u64)> {
    let global = (shell.message.as_str(), shell.message_tick);
    let own = (seat.message.as_str(), seat.message_tick);
    let (text, tick) = if own.1 >= global.1 && !own.0.is_empty() {
        own
    } else {
        global
    };
    let age = shell.update_ticks.saturating_sub(tick);
    (!text.is_empty() && age < TOAST_TICKS).then_some((text, age))
}

/// A message as a toast near the bottom of the current canvas (receipts, saves, refusals), for
/// a few seconds after it changes. `avoid` is a docked counter panel to keep clear of: the toast
/// sits beside it when there is room, otherwise at the top of the view (over the room's ceiling)
/// when the panel leaves the top free, otherwise over the panel's lower edge.
pub(super) fn draw_toast(
    draw: &mut RaylibDrawHandle<'_>,
    message: &str,
    age: u64,
    avoid: Option<Rectangle>,
) {
    let canvas = ui_kit::canvas();
    // The free horizontal band the toast is centred in, and the bottom it rests on.
    let (mut left, mut area, mut bottom) = (0, canvas.width, canvas.height - 28);
    let mut top_anchored = false;
    if let Some(panel) = avoid {
        let beside = (panel.x + panel.width) as i32 + 8;
        if canvas.width - beside - 24 >= 120 {
            left = beside;
            area = canvas.width - beside;
        } else if panel.y as i32 >= 60 {
            top_anchored = true;
        } else {
            bottom = canvas.height - 10;
        }
    }
    let max_width = (area - 24).min(300);
    if max_width < 80 {
        return;
    }
    let lowered = message.to_lowercase();
    let warning = [
        "unavailable",
        "not enough",
        "no cargo",
        "no contract",
        "failed",
        "could not",
        "not a",
        "refused",
        "cannot",
    ]
    .iter()
    .any(|word| lowered.contains(word));
    let icon: &SpriteDef = if warning {
        &icons::WARNING
    } else if lowered.contains("save") || lowered.contains("load") {
        &super::pixel_art::sprites::menu_icons::DISK
    } else if lowered.contains("credit") || lowered.contains(" cr") || lowered.contains("bought") {
        &icons::CREDITS
    } else {
        &super::pixel_art::sprites::menu_icons::LETTER
    };
    let lines = ui_kit::wrap(message, max_width - 26, TextStyle::Body);
    let lines = &lines[..lines.len().min(3)];
    let text_width = lines
        .iter()
        .map(|line| ui_kit::text_width(line, TextStyle::Body))
        .max()
        .unwrap_or(0);
    let width = text_width + 26;
    let height = i32::try_from(lines.len()).unwrap_or(1) * TextStyle::Body.line_height() + 10;
    // Slides up into place over its first few frames.
    let rise = i32::try_from(8_u64.saturating_sub(age)).unwrap_or(0);
    let x = left + (area - width) / 2;
    // Under the room's name plate (top-left) when anchored at the top of the view.
    let y = if top_anchored {
        32 + rise
    } else {
        bottom - height + rise
    };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        if warning {
            PanelStyle::Warning
        } else {
            PanelStyle::Accent
        },
    );
    ui_kit::icon(draw, icon, x + 6, y + (height - icons::ICON_SIZE) / 2);
    for (index, line) in lines.iter().enumerate() {
        let row = i32::try_from(index).unwrap_or(0);
        ui_kit::text(
            draw,
            line,
            x + 20,
            y + 5 + row * TextStyle::Body.line_height(),
            TextStyle::Body,
            BONE,
        );
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
    if player.cargo_used() >= player.cargo_capacity {
        warnings.push(("CARGO FULL", &icons::CARGO));
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
    explored: &DiscoveredTiles,
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
            if !sky && !explored.contains(position) {
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
        draw.draw_rectangle(map_x - 1, map_y - 1, 2, 2, player_accent(remote.player_id));
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
        ModalScreen::ExitConfirm => menus::draw_exit_confirm(draw),
        ModalScreen::UnsavedExitConfirm => menus::draw_unsaved_exit_confirm(draw, ui),
        ModalScreen::Shop => counters::draw_shop(draw, ui),
        ModalScreen::Options => menus::draw_options(draw, ui),
        ModalScreen::SaveSlots => menus::draw_slots(draw, ui, true),
        ModalScreen::LoadSlots => menus::draw_slots(draw, ui, false),
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
        ModalScreen::WorldSetup => menus::draw_world_setup(draw, ui),
    }
}

fn draw_research_log_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Research Log",
        "Collected discoveries and mining intelligence.",
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
        )])
        .prompts(&[hint(Prompt::Back, "close")]),
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
            SectionItem::icon_stat(
                &icons::CREDITS,
                "Credits",
                format!("{} cr", ui.player.credits),
                GOLD,
            ),
            SectionItem::icon_stat(&icons::BOMB, "Bombs", ui.player.bombs.to_string(), ORANGE),
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
            (
                &menu_icons::SIGNAL,
                "Signal relays",
                ui.player.signal_relay_kits,
            ),
            (
                &menu_icons::DRONE,
                "Survey drones",
                ui.player.survey_drone_kits,
            ),
            (&icons::CARGO, "Cargo lifts", ui.player.cargo_lift_kits),
            (
                &menu_icons::HAMMER,
                "Tunnel supports",
                ui.player.tunnel_support_kits,
            ),
            (
                &menu_icons::GEAR,
                "Pump stations",
                ui.player.pump_station_kits,
            ),
            (
                &menu_icons::ANVIL,
                "Ore processors",
                ui.player.ore_processor_kits,
            ),
        ]
        .into_iter()
        .map(|(icon, label, count): (&'static SpriteDef, &str, u32)| {
            SectionItem::icon_stat(
                icon,
                label,
                count.to_string(),
                if count > 0 { LIME } else { FOG },
            )
        })
        .collect(),
    ));
    sections.push(Section::new(
        "Rig",
        SKY_LIGHT,
        [
            (&rig::DRILL_DOWN, "Drill", ui.player.drill_strength),
            (&menu_icons::GEAR, "Engine", ui.player.engine_level),
            (&icons::FUEL, "Fuel Tank", ui.player.fuel_tank_level),
            (&icons::CARGO, "Cargo Bay", ui.player.cargo_bay_level),
            (&icons::HULL, "Hull", ui.player.hull_level),
            (&icons::HEAT, "Radiator", ui.player.radiator_level),
            (&icons::SCANNER, "Scanner", ui.player.scanner_level),
        ]
        .into_iter()
        .map(|(icon, label, tier): (&'static SpriteDef, &str, u8)| {
            SectionItem::icon_stat(icon, label, format!("tier {tier}"), MIST)
        })
        .collect(),
    ));
    UiLayout::screen(draw).modal_with_render_plan(
        "Inventory",
        "Cargo, artifacts, consumables and field kits.",
        &ModalContent::new(sections)
            .prompts(&[hint(Prompt::Inventory, ""), hint(Prompt::Back, "close")]),
    );
}

/// Map legend entries: colour and label.
const MAP_LEGEND: [(Color, &str); 6] = [
    (GOLD, "Ore"),
    (PINK, "Relic"),
    (LIME, "Gas"),
    (ORANGE, "Lava"),
    (RED, "Blast"),
    (SLATE_DARK, "Tunnel"),
];

/// Height of one map legend row.
const MAP_LEGEND_ROW: i32 = 11;

/// Where each map legend entry (the colours, then "You") goes in a legend `width` wide that
/// starts at `(x, y)`, wrapping onto further rows in narrow dialogs. Returns the positions and
/// the number of rows.
fn map_legend_layout(x: i32, y: i32, width: i32) -> (Vec<(i32, i32)>, i32) {
    let widths = MAP_LEGEND
        .iter()
        .map(|(_, label)| ui_kit::text_width(label, TextStyle::Body) + 10)
        .chain([ui_kit::text_width("You", TextStyle::Body) + 12]);
    let (mut lx, mut ly, mut rows) = (x, y, 1);
    let mut positions = Vec::new();
    for item in widths {
        if lx > x && lx + item > x + width {
            lx = x;
            ly += MAP_LEGEND_ROW;
            rows += 1;
        }
        positions.push((lx, ly));
        lx += item + 12;
    }
    (positions, rows)
}

/// The map legend in `width` starting at `(x, y)` (see [`map_legend_layout`]).
fn draw_map_legend(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32, width: i32) {
    let (positions, _) = map_legend_layout(x, y, width);
    for ((color, label), (lx, ly)) in MAP_LEGEND.iter().zip(&positions) {
        draw.draw_rectangle(*lx, ly + 1, 7, 7, INK);
        draw.draw_rectangle(lx + 1, ly + 2, 5, 5, *color);
        ui_kit::text(draw, label, lx + 10, *ly, TextStyle::Body, MIST);
    }
    if let Some((lx, ly)) = positions.get(MAP_LEGEND.len()) {
        ui_kit::icon(
            draw,
            &super::pixel_art::sprites::menu_icons::PERSON,
            *lx,
            *ly,
        );
        ui_kit::text(draw, "You", lx + 12, *ly, TextStyle::Body, MIST);
    }
}

/// The mine map: discovered terrain at two art pixels per tile around the rig, with depth
/// marks, the rig's blinking marker and a legend.
fn draw_map_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let depth = (ui.player.y / TILE_SIZE).max(0.0) as i32;
    let map_rect = UiLayout::screen(draw).canvas_modal(
        "Mine Map",
        "Discovered terrain only.",
        &[hint(Prompt::Map, ""), hint(Prompt::Back, "close")],
        &format!(
            "Depth {depth} m | deepest {} m | {:.0} m from the west edge",
            ui.snapshot.progression.deepest_tile_reached,
            ui.player.x / TILE_SIZE
        ),
    );
    let (_, legend_rows) = map_legend_layout(0, 0, map_rect.width as i32 - 4);
    let legend_height = legend_rows * MAP_LEGEND_ROW + 3;
    let well = Rectangle {
        height: (map_rect.height - legend_height as f32).max(0.0),
        ..map_rect
    };
    ui_kit::well(draw, well);
    // Inside the well, leaving room for the depth scale on the left.
    let scale_width = 26;
    let (x0, y0) = (well.x as i32 + 2 + scale_width, well.y as i32 + 2);
    let (width, height) = (well.width as i32 - 4 - scale_width, well.height as i32 - 4);
    if width < 8 || height < 8 {
        return;
    }
    draw.draw_rectangle(x0, y0, width, height, NIGHT);
    let terrain = ui.snapshot.terrain;
    let (terrain_width, terrain_height) = (terrain.width().max(1), terrain.height().max(1));
    let pixel = 2;
    let (columns, rows) = (width / pixel, height / pixel);
    let player = ui.player.tile_position(TILE_SIZE);
    let origin_x = (player.x - columns / 2).clamp(0, (terrain_width - columns).max(0));
    let origin_y = (player.y - rows / 2).clamp(0, (terrain_height - rows).max(0));
    for row in 0..rows.min(terrain_height - origin_y) {
        let tile_y = origin_y + row;
        for column in 0..columns.min(terrain_width - origin_x) {
            let position = TilePosition {
                x: origin_x + column,
                y: tile_y,
            };
            let sky = tile_y <= 4;
            if !sky && !ui.view.is_explored(position) {
                continue;
            }
            let color = match terrain.tile(position) {
                Some(tile) if tile.kind == TileKind::Air && sky => DEEP_BLUE,
                Some(tile) => minimap_color(tile.kind),
                None => continue,
            };
            draw.draw_rectangle(x0 + column * pixel, y0 + row * pixel, pixel, pixel, color);
        }
    }
    // Depth scale: a tick and label every 10 or 20 m.
    let step = if rows > 60 { 20 } else { 10 };
    let first = (origin_y + step - 1) / step * step;
    for mark in (first..origin_y + rows).step_by(usize::try_from(step).unwrap_or(10)) {
        let y = y0 + (mark - origin_y) * pixel;
        if y - 4 < y0 {
            continue;
        }
        draw.draw_rectangle(x0 - 4, y, 3, 1, FOG);
        ui_kit::text_right(
            draw,
            &format!("{mark}"),
            x0 - 6,
            y - 4,
            TextStyle::Body,
            FOG,
        );
    }
    // The rig: an ink-ringed marker that blinks.
    let marker_x = x0 + (player.x - origin_x) * pixel;
    let marker_y = y0 + (player.y - origin_y) * pixel;
    let blink = (ui_kit::seconds() * 2.0) as i32 % 2 == 0;
    draw.draw_rectangle(marker_x - 3, marker_y - 3, 8, 8, INK);
    draw.draw_rectangle(
        marker_x - 2,
        marker_y - 2,
        6,
        6,
        if blink { BONE } else { SKY_LIGHT },
    );
    draw.draw_rectangle(marker_x - 1, marker_y - 1, 4, 4, OCEAN);
    // Other rigs.
    for remote in ui
        .snapshot
        .players
        .iter()
        .filter(|other| other.player_id != ui.view.player_id)
    {
        let (tx, ty) = ((remote.x / TILE_SIZE) as i32, (remote.y / TILE_SIZE) as i32);
        if (origin_x..origin_x + columns).contains(&tx) && (origin_y..origin_y + rows).contains(&ty)
        {
            let (rx, ry) = (x0 + (tx - origin_x) * pixel, y0 + (ty - origin_y) * pixel);
            draw.draw_rectangle(rx - 2, ry - 2, 6, 6, INK);
            draw.draw_rectangle(rx - 1, ry - 1, 4, 4, ORANGE);
        }
    }
    draw_map_legend(
        draw,
        well.x as i32 + 2,
        (well.y + well.height) as i32 + 4,
        well.width as i32 - 4,
    );
}

/// The controls reference: every action with its keys or buttons for the menu device, in two
/// columns (actions without a binding on that device are left out).
fn draw_help_ui(draw: &mut RaylibDrawHandle<'_>) {
    type Rows = &'static [(Prompt, &'static str)];
    const GROUPS: [(&str, Color, Rows); 3] = [
        (
            "Movement",
            SKY_LIGHT,
            &[
                (Prompt::Walk, "Move"),
                (Prompt::Thrust, "Thrust"),
                (Prompt::Drill, "Drill down"),
                (Prompt::Use, "Use building"),
                (Prompt::Pause, "Pause"),
            ],
        ),
        (
            "Tools",
            GOLD,
            &[
                (Prompt::Scan, "Scanner"),
                (Prompt::Bomb, "Bomb"),
                (Prompt::Map, "Map"),
                (Prompt::Inventory, "Inventory"),
                (Prompt::Infrastructure, "Field kits"),
            ],
        ),
        (
            "Menus",
            LIME,
            &[
                (Prompt::Navigate, "Select"),
                (Prompt::Confirm, "Confirm"),
                (Prompt::Back, "Back / close"),
                (Prompt::Tab, "Shop columns"),
                (Prompt::Scroll, "Scroll"),
                (Prompt::QuickSave, "Quick save"),
                (Prompt::QuickLoad, "Quick load"),
                (Prompt::Fullscreen, "Fullscreen"),
                (Prompt::Help, "Controls"),
                (Prompt::Details, "Diagnostics"),
                (Prompt::NetOverlay, "Network overlay"),
            ],
        ),
    ];
    let device = prompts::menu_device();
    let body = UiLayout::screen(draw).canvas_modal(
        "Controls",
        "",
        &[hint(Prompt::Back, "close")],
        if device.is_gamepad() {
            "Controller layout (keyboard keys show after any key press)."
        } else {
            "Keyboard layout (controller buttons show after any button press)."
        },
    );
    let column_width = (body.width as i32 - 8) / 2;
    let row_height = prompts::PROMPT_HEIGHT + 3;
    let top = body.y as i32 + 4;
    let mut columns = [
        (body.x as i32, top),
        (body.x as i32 + column_width + 8, top),
    ];
    for (index, (title, color, rows)) in GROUPS.iter().enumerate() {
        let rows: Vec<_> = rows
            .iter()
            .filter(|(prompt, _)| !crate::input::prompts::glyphs(*prompt, device).is_empty())
            .collect();
        if rows.is_empty() {
            continue;
        }
        // Movement and tools stack on the left; menus fill the right.
        let column = usize::from(index == 2);
        let (x, y) = &mut columns[column];
        ui_kit::text(draw, &title.to_uppercase(), *x, *y, TextStyle::Bold, *color);
        *y += 12;
        for (prompt, label) in rows {
            let width = prompts::prompt_width(hint(*prompt, ""), device).unwrap_or(0);
            // Narrow (split-screen) columns shorten the label rather than run into the keys.
            let label = ui_kit::truncate(label, column_width - width - 10, TextStyle::Body);
            ui_kit::text(draw, &label, *x + 2, *y + 2, TextStyle::Body, MIST);
            prompts::draw_prompt(
                draw,
                *x + column_width - width - 4,
                *y,
                hint(*prompt, ""),
                device,
                MIST,
            );
            *y += row_height;
        }
        *y += 6;
    }
}
