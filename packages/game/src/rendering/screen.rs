mod cave_map;
mod clock_hud;
mod poi_map;
mod village_map;

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, ExplosiveKind, Player,
    economy::{UpgradeKind, current_level},
    terrain::{SurfaceMaterial, TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::Band,
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
        sprites::{explosives as explosive_art, icons, menu_icons, rig},
    },
    prompts::{self, hint},
    quests,
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
    let tile = view.player.tile_position(TILE_SIZE);
    let depth_heat = fatherload_simulation::protection::heat_excess(
        terrain.band_at(tile),
        view.player.radiator_level,
        view.player.hazard_gear.heat_shield,
    ) > 0;
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
    let rect = view_rect(view);
    let (left, top) = (rect.x as i32 + 4, rect.y as i32 + 4);
    let split = snapshot.views.len() > 1;

    // Stats: hull, fuel, cargo, block hopper.
    let width = stats_panel_width(rect.width);
    let header = if split { 11 } else { 0 };
    let panel_height = 6 + header + 4 * 12 + 3;
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
            player_accent(view.player_id, view.player.cosmetics),
        );
        y += header;
    }
    if let Some(pilot) = &view.pilot {
        // On foot (Part II F5): the pilot's own gauges replace the rig's.
        draw_pilot_bars(draw, view, pilot, (inner_x, y, inner_width));
    } else {
        draw_rig_bars(draw, view, (inner_x, y, inner_width));
    }

    let armed = snapshot
        .bombs
        .iter()
        .filter(|bomb| bomb.owner == Some(view.player_id) && bomb.awaiting_remote())
        .count();
    let wallet_bottom = draw_wallet_panel(
        draw,
        view,
        (&shell.seat(view.seat_index).explosives, armed),
        rect,
        left + width + 4,
        top,
    );
    if let Some(bottom) = wallet_bottom {
        clock_hud::draw_clock_badge(draw, snapshot, rect, bottom + 3);
    }
    // Altitude air, cold and wind while flying a V8 sky (F6).
    super::sky_hud::draw_sky_gauge(draw, snapshot, view, rect, top + 74);
    // The Moon checklist, the vacuum warning and the landing's sunrise (WP-MOON).
    super::moon_hud::draw_moon_hud(draw, shell, snapshot, view, rect, top + 124);
    draw_view_prompt(draw, shell, view, rect);
    draw_view_hud_rest(
        draw,
        shell,
        snapshot,
        view,
        (left, top, width, panel_height),
        fps,
    );
}

/// The rig's four gauges: hull, fuel, cargo and the block hopper.
fn draw_rig_bars(draw: &mut RaylibDrawHandle<'_>, view: &ViewSnapshot<'_>, at: (i32, i32, i32)) {
    let hud = view.hud;
    let (inner_x, mut y, inner_width) = at;
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
    y += ui_kit::stat_bar(
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
    // WP-DROPS: the block hopper fills separately from the bay ("OFF" when the rig's collect
    // toggle is off).
    ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::STONE_BLOCK,
        if view.player.collect_blocks {
            "BLOCK"
        } else {
            "OFF"
        },
        view.player.block_hopper.total() as f32,
        view.player.block_hopper_capacity as f32,
        BarColors::HOPPER,
        0.0,
    );
}

/// The pilot's gauges on foot: health, air, the jetpack's burn (or the rig's hull when there
/// is no jetpack) and the personal pack.
fn draw_pilot_bars(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    pilot: &crate::render_snapshot::PilotPresentation,
    at: (i32, i32, i32),
) {
    let state = &pilot.state;
    let (inner_x, mut y, inner_width) = at;
    y += ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::HELMET,
        "PILOT",
        state.health,
        state.max_health,
        BarColors::PILOT,
        0.3,
    );
    y += ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::OXYGEN,
        "AIR",
        state.oxygen,
        state.max_oxygen,
        BarColors::AIR,
        0.25,
    );
    y += if state.jetpack_capacity > 0.0 {
        ui_kit::stat_bar(
            draw,
            inner_x,
            y,
            inner_width,
            &icons::JETPACK,
            "JET",
            state.jetpack_fuel,
            state.jetpack_capacity,
            BarColors::FUEL,
            0.0,
        )
    } else {
        ui_kit::stat_bar(
            draw,
            inner_x,
            y,
            inner_width,
            &icons::HULL,
            "RIG",
            view.hud.hull,
            view.hud.max_hull,
            BarColors::HULL,
            0.35,
        )
    };
    ui_kit::stat_bar(
        draw,
        inner_x,
        y,
        inner_width,
        &icons::CARGO,
        "PACK",
        view.player.pilot_pack.total() as f32,
        view.player.pilot_gear.pack_capacity() as f32,
        BarColors::CARGO,
        0.0,
    );
}

/// The rest of a view's HUD: warning badges, the quest tracker and the details overlay.
fn draw_view_hud_rest(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    (left, top, width, panel_height): (i32, i32, i32, i32),
    fps: u32,
) {
    let rect = view_rect(view);
    // Warning badges.
    let mut badge_x = left;
    let badge_y = top + panel_height + 3;
    let mut warned = false;
    for (warning, sprite) in hud_warnings(snapshot, view) {
        badge_x += ui_kit::badge(draw, badge_x, badge_y, sprite, warning) + 2;
        warned = true;
    }
    quests::draw_quest_tracker(
        draw,
        shell.seat(view.seat_index),
        snapshot,
        view,
        (left, badge_y + if warned { 16 } else { 0 }, width),
    );

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
    clock_hud::draw_sleep_overlay(draw, snapshot, view, rect);
}

/// The view's context prompt under the rig, for its seat's device: how to enter the building
/// the rig stands at, or how a controller joins a seat nobody drives yet.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per prompt, in priority order"
)]
fn draw_view_prompt(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let device = shell.prompt_device(view.seat_index);
    let idle = shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && shell.seat(view.seat_index).build.is_none();
    let exit_ready = idle
        && view.can_exit
        && view.failure.is_none()
        && view.active_drill.is_none()
        && view.player.velocity_x.abs() < 1.0
        && view.player.velocity_y.abs() < 1.0;
    // A drained buoy (or a tank too full for it) does not stand in the way of climbing out:
    // use falls through to the exit then (`SeatUi::try_buoy`).
    let buoy_usable = view.buoy.is_some_and(|(_, used)| {
        !used
            && fatherload_simulation::worldgen::sky::isles::buoy_refill(
                view.player.fuel_capacity,
                view.player.fuel,
            ) >= 1.0
    });
    let (prompt, label, accent) = if device == InputDevice::Unassigned {
        (
            Prompt::Join,
            format!("to join as Player {}", view.seat_index + 1),
            false,
        )
    } else if idle && view.can_board {
        // On foot beside the rig (Part II F5): boarding comes first.
        (Prompt::BoardRig, "Board the rig".to_owned(), true)
    } else if idle && view.pilot.is_some() && view.surface_zone.is_some() {
        let zone = view
            .surface_zone
            .unwrap_or(fatherload_simulation::economy::SurfaceZone::Fuel);
        (
            Prompt::Climb,
            format!("Walk into {}", surface_zone_label(zone)),
            true,
        )
    } else if idle && view.pilot.is_some() && view.village_door.is_some() {
        let door = view.village_door.clone().unwrap_or_default();
        (Prompt::Climb, door.replacen("Enter", "Walk into", 1), true)
    } else if idle && view.pilot.is_some() && view.failure.is_some() {
        // On foot with the rig wrecked or dry: use (twice) radios for a pickup.
        (Prompt::Use, "Radio for a pickup".to_owned(), true)
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
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && view.failure.is_none()
        && view.in_colony
    {
        (Prompt::Use, "Trade at Tranquility Station".to_owned(), true)
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && view.failure.is_none()
        && let Some((_, name)) = &view.talk_target
    {
        (Prompt::Use, format!("Talk to {name}"), true)
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && view.failure.is_none()
        && let Some(door) = &view.village_door
    {
        (Prompt::Use, door.clone(), true)
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && let Some((_, used)) = view.buoy
        && (buoy_usable || !exit_ready)
    {
        // A sky isle fuel buoy (WP-SKY): one pricey refill each, into a low tank only.
        let adds = fatherload_simulation::worldgen::sky::isles::buoy_refill(
            view.player.fuel_capacity,
            view.player.fuel,
        );
        if used {
            (Prompt::Use, "Buoy drained".to_owned(), false)
        } else if adds < 1.0 {
            (Prompt::Use, "Tank too full for the buoy".to_owned(), false)
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a few hundred fuel"
            )]
            let adds = adds.round() as u32;
            (
                Prompt::Use,
                format!(
                    "Buoy refill: +{adds} fuel for {} cr",
                    fatherload_simulation::worldgen::sky::isles::BUOY_PRICE
                ),
                true,
            )
        }
    } else if shell.run_mode == RunMode::Playing
        && shell.modal.is_none()
        && shell.seat(view.seat_index).modal.is_none()
        && shell.seat(view.seat_index).build.is_none()
        && (view.failure.is_none()
            || view.nearby_module == Some(fatherload_simulation::base::ModuleKind::FuelStation))
        && let Some(module) = view.nearby_module
    {
        (Prompt::Use, format!("Use {}", module.name()), true)
    } else if exit_ready {
        (Prompt::ExitRig, "Climb out".to_owned(), false)
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
/// Returns the panel's bottom (no panel when the view is too narrow for it), where the clock
/// badge goes.
#[allow(clippy::too_many_lines, reason = "one panel, row by row")]
fn draw_wallet_panel(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    (explosives, armed): (&crate::input_mapping::ExplosiveSelection, usize),
    rect: Rectangle,
    min_x: i32,
    top: i32,
) -> Option<i32> {
    let hud = view.hud;
    let money = format!("{}", hud.credits);
    // Underground (or on the ground): depth below the local surface; in the air: altitude
    // above it.
    let (depth, depth_icon, depth_color) = hud.altitude.map_or_else(
        || (format!("{} m", hud.depth), &icons::DEPTH, SKY_LIGHT),
        |altitude| (format!("{altitude} m"), &icons::ALTITUDE, MINT),
    );
    let has_scanner = view.player.scanner_level > 0;
    let scan = if hud.scanner_cooldown_seconds > 0.05 {
        format!("{:.0}s", hud.scanner_cooldown_seconds.ceil())
    } else {
        "READY".to_owned()
    };
    let selected = explosives.selected;
    let carried = view.player.explosive_count(selected);
    let bombs = format!("{carried}");
    // A third row names a special charge and counts C4 waiting for the detonator.
    let charge_line = (selected != ExplosiveKind::Bomb || armed > 0).then(|| {
        let name = selected.name().to_uppercase();
        if armed > 0 && selected == ExplosiveKind::Bomb {
            // "BOMB  1 ARMED" reads as if a bomb were armed: name the waiting charges instead.
            format!("{armed} C4 ARMED")
        } else if armed > 0 {
            format!("{name}  {armed} ARMED")
        } else {
            name
        }
    });
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
    let line_width = charge_line
        .as_ref()
        .map_or(0, |line| ui_kit::text_width(line, TextStyle::Body));
    // A weapon row: the mounted gun, its tier and its capacitor charge or ammo.
    let weapon_line = weapon_row(view);
    let weapon_width = weapon_line.as_ref().map_or(0, |WeaponRow { label, .. }| {
        ui_kit::text_width(label, TextStyle::Body) + icons::ICON_SIZE + 40
    });
    let panel_width = value_width
        .max(chips_width)
        .max(line_width)
        .max(weapon_width)
        + ui_kit::PANEL_PADDING * 2;
    let panel_width = panel_width.max(64);
    let x = (rect.x + rect.width) as i32 - 16 - panel_width;
    if x <= min_x {
        return None;
    }
    let weapon_top = if charge_line.is_some() { 56 } else { 45 };
    let height = weapon_top + if weapon_line.is_some() { 13 } else { 0 };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, panel_width as f32, height as f32),
        PanelStyle::Standard,
    );
    let right = x + panel_width - ui_kit::PANEL_PADDING;
    ui_kit::icon(draw, &icons::CREDITS, x + 6, top + 6);
    ui_kit::text_right(draw, &money, right, top + 7, TextStyle::Bold, GOLD);
    ui_kit::icon(draw, depth_icon, x + 6, top + 17);
    ui_kit::text_right(draw, &depth, right, top + 18, TextStyle::Bold, depth_color);
    ui_kit::divider(draw, x + 4, top + 28, panel_width - 8);
    // Tool chips: bombs on the left, the scanner's cooldown on the right.
    let chip_y = top + 32;
    ui_kit::icon(draw, explosive_art::icon(selected), x + 6, chip_y);
    ui_kit::text(
        draw,
        &bombs,
        x + 6 + icons::ICON_SIZE + 2,
        chip_y + 1,
        TextStyle::Bold,
        if carried > 0 { ORANGE } else { FOG },
    );
    if let Some(line) = &charge_line {
        ui_kit::text(
            draw,
            line,
            x + 6,
            chip_y + 12,
            TextStyle::Body,
            if armed > 0 { ROSE } else { MIST },
        );
    }
    if let Some(row) = &weapon_line {
        draw_weapon_row(draw, row, x + 6, right, top + weapon_top - 2);
    }
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
    Some(top + height)
}

/// The HUD's weapon row: the mounted gun, its label, its ammo (ammo weapons) or capacitor
/// charge (0..=1), and whether it can fire now.
struct WeaponRow {
    weapon: fatherload_simulation::WeaponKind,
    label: String,
    ammo: Option<u32>,
    energy: f32,
    ready: bool,
}

fn weapon_row(view: &ViewSnapshot<'_>) -> Option<WeaponRow> {
    let mounted = view.player.mounted_weapon?;
    let stats = mounted.kind.stats(mounted.mk);
    let ammo = stats.ammo.map(|ammo| view.player.ammo(ammo));
    Some(WeaponRow {
        weapon: mounted.kind,
        label: format!(
            "{} {}",
            mounted.kind.name().to_uppercase(),
            crate::shell::arms::mk_name(mounted.mk).trim_start_matches("Mk ")
        ),
        ammo,
        energy: (view.player.weapon.energy / view.player.battery_capacity().max(1.0))
            .clamp(0.0, 1.0),
        ready: ammo.map_or(view.player.weapon.energy >= stats.energy, |count| count > 0),
    })
}

fn draw_weapon_row(draw: &mut RaylibDrawHandle<'_>, row: &WeaponRow, x: i32, right: i32, y: i32) {
    ui_kit::icon(
        draw,
        crate::rendering::pixel_art::sprites::weapons::icon(row.weapon),
        x,
        y,
    );
    ui_kit::text(
        draw,
        &row.label,
        x + icons::ICON_SIZE + 2,
        y + 1,
        TextStyle::Body,
        if row.ready { MINT } else { FOG },
    );
    if let Some(count) = row.ammo {
        ui_kit::text_right(
            draw,
            &format!("x{count}"),
            right,
            y + 1,
            TextStyle::Bold,
            if count > 0 { ORANGE } else { ROSE },
        );
    } else {
        // The capacitor gauge.
        let (bar_width, bar_x) = (28, right - 28);
        draw.draw_rectangle(bar_x, y + 3, bar_width, 5, NIGHT);
        let fill = ((bar_width - 2) as f32 * row.energy).round() as i32;
        draw.draw_rectangle(
            bar_x + 1,
            y + 4,
            fill.max(0),
            3,
            if row.ready { MINT } else { ROSE },
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
    } else if let Some(controls_top) = super::touch_controls::upright_controls_top() {
        // A phone held upright has the stick and buttons along the bottom: sit above them.
        bottom = bottom.min(controls_top - 4);
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
    if snapshot.test_mode.is_some() {
        // Always first: this world's records do not count.
        warnings.push(("TEST MODE", &menu_icons::ROCKET));
    }
    if player.fuel <= player.fuel_capacity * 0.2 {
        warnings.push(("LOW FUEL", &icons::FUEL));
    }
    if player.hull <= player.max_hull() * 0.35 {
        warnings.push(("HULL", &icons::WARNING));
    }
    if player.cargo_used() >= player.cargo_capacity {
        warnings.push(("CARGO FULL", &icons::CARGO));
    }
    if player.collect_blocks && player.block_hopper.total() >= player.block_hopper_capacity {
        warnings.push(("HOPPER FULL", &icons::STONE_BLOCK));
    }
    // Warning depths are classic nominal rows (deep worlds stretch them with their bands).
    let depth = snapshot.terrain.nominal_row(view.hud.row.max(0));
    if depth >= 60 {
        warnings.push(("HEAT", &icons::HEAT));
    }
    if view.deep_instability >= 60.0 {
        warnings.push(("UNSTABLE", &icons::WARNING));
    }
    if depth >= 80 && snapshot.signal_relay_count == 0 {
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
        TileKind::Surface(material) => match material {
            SurfaceMaterial::Sand | SurfaceMaterial::Sandstone => SAND,
            SurfaceMaterial::RedRock => RUST,
            SurfaceMaterial::Snow => MIST,
            SurfaceMaterial::Granite => SLATE_LIGHT,
            SurfaceMaterial::Mud => SOIL,
        },
        TileKind::Poi(tile) => super::terrain::tile_art::poi_color(tile, Band::Stone),
        TileKind::Cave(tile) => super::terrain::tile_art::cave_color(tile, Band::Stone),
        TileKind::Bedrock => PLUM,
        TileKind::Built(_) => CREAM,
        TileKind::Sky(tile) => super::terrain::tile_art::sky_color(tile),
        TileKind::Lunar(tile) => super::terrain::tile_art::lunar_color(tile),
    }
}

/// Bottom-right minimap of the explored mine around the rig (left of the depth ruler).
#[allow(
    clippy::too_many_lines,
    reason = "terrain, then every kind of marker, then the rig, in draw order"
)]
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

    let terrain_height = terrain.height().max(1);
    let player_tile = view.player.tile_position(TILE_SIZE);
    // A bounded world fits across the minimap; an endless one scrolls with the rig. Rows
    // follow the rig between the world's first row (mountain tops) and its floor; high in the
    // sky the minimap keeps the ground below the rig in view (see `map_focus_row`).
    let (origin_x, visible_width) = minimap_columns(terrain, player_tile.x);
    let top_row = terrain.min_y();
    let world_rows = terrain_height - top_row;
    let visible_height = world_rows.min(80);
    let origin_y = if world_rows > visible_height {
        (map_focus_row(terrain, player_tile, visible_height) - visible_height / 2)
            .clamp(top_row, terrain_height - visible_height)
    } else {
        top_row
    };
    for py in 0..height {
        let tile_y = origin_y + py * visible_height / height;
        for px in 0..width {
            let position = TilePosition {
                x: origin_x + px * visible_width / width,
                y: tile_y,
            };
            if let Some(color) = map_tile_color(terrain, position, explored.contains(position)) {
                draw.draw_rectangle(x + px, y + py, 1, 1, color);
            }
        }
    }
    let to_map = |tile_x: i32, tile_y: i32| {
        (
            x + (tile_x - origin_x) * width / visible_width,
            y + (tile_y - origin_y) * height / visible_height,
        )
    };
    let on_map = |tile_x: i32, tile_y: i32| {
        tile_y >= origin_y
            && tile_y < origin_y + visible_height
            && tile_x >= origin_x
            && tile_x < origin_x + visible_width
    };
    poi_map::draw_minimap_markers(
        draw,
        &poi_map::markers(terrain, view.player),
        |tile_x, tile_y| on_map(tile_x, tile_y).then(|| to_map(tile_x, tile_y)),
    );
    cave_map::draw_minimap_markers(
        draw,
        &cave_map::markers(terrain, view.player),
        |tile_x, tile_y| on_map(tile_x, tile_y).then(|| to_map(tile_x, tile_y)),
    );
    village_map::draw_minimap_villages(
        draw,
        terrain,
        view.player,
        (origin_x, origin_x + visible_width),
        |tile_x, tile_y| on_map(tile_x, tile_y).then(|| to_map(tile_x, tile_y)),
    );
    quests::draw_minimap_quest_markers(draw, &view.quest_markers, |tile_x, tile_y| {
        on_map(tile_x, tile_y).then(|| to_map(tile_x, tile_y))
    });
    for remote in players
        .iter()
        .filter(|other| other.player_id != view.player_id)
    {
        let tile_x = (remote.x / TILE_SIZE).floor() as i32;
        let tile_y = (remote.y / TILE_SIZE).floor() as i32;
        if !on_map(tile_x, tile_y) {
            continue;
        }
        let (map_x, map_y) = to_map(tile_x, tile_y);
        draw.draw_rectangle(
            map_x - 1,
            map_y - 1,
            2,
            2,
            player_accent(remote.player_id, Some(remote.cosmetics)),
        );
    }
    if terrain.is_streamed() {
        // Navigation in an endless world: which way, and how far, home is.
        let heading = town_heading(view.player.x / TILE_SIZE);
        if heading != "in town" {
            let label_width = ui_kit::text_width(&heading, TextStyle::Body);
            draw.draw_rectangle(
                x + width + 4 - label_width - 4,
                y - 20,
                label_width + 8,
                14,
                INK,
            );
            ui_kit::text_right(draw, &heading, x + width + 4, y - 18, TextStyle::Body, BONE);
        }
    }
    let (map_x, map_y) = to_map(player_tile.x, player_tile.y.max(origin_y));
    let blink = (ui_kit::seconds() * 2.0) as i32 % 2 == 0;
    draw.draw_rectangle(map_x - 1, map_y - 1, 3, 3, INK);
    draw.draw_rectangle(map_x, map_y, 1, 1, if blink { BONE } else { SKY_LIGHT });
}

/// The row a map `rows` tall centres on for a rig at `tile`: the rig itself, except high in
/// the sky (the sky has no ceiling), where the map keeps the ground under the rig a quarter
/// of the map above its bottom edge instead of showing nothing but empty sky.
fn map_focus_row(terrain: &Terrain, tile: TilePosition, rows: i32) -> i32 {
    // Among a V8 sky's islands the map follows the rig, so the isles around it show.
    if terrain.settings().generator.has_sky()
        && tile.y
            <= fatherload_simulation::worldgen::sky::row_of(
                fatherload_simulation::worldgen::sky::LOW_ISLES_BOTTOM_ROWS - 40,
            )
    {
        return tile.y;
    }
    tile.y.max(terrain.surface_row(tile.x) - rows / 4)
}

/// The map colour of `position`: open sky above the column's ground always shows, the rest
/// only once explored (`None`: leave the map's background).
fn map_tile_color(terrain: &Terrain, position: TilePosition, explored: bool) -> Option<Color> {
    let biomes = terrain.settings().generator.has_biomes();
    let surface = terrain.surface_row(position.x);
    let sky = position.y < surface || (!biomes && position.y <= 4);
    if !sky && !explored {
        return None;
    }
    match terrain.tile(position) {
        Some(tile) if tile.kind == TileKind::Air && sky => Some(DEEP_BLUE),
        Some(tile) => Some(minimap_color(tile.kind)),
        None if sky => Some(DEEP_BLUE),
        None => None,
    }
}

/// Columns the minimap spans in an endless world (about the width of a classic bounded world).
const MINIMAP_STREAMED_COLUMNS: i32 = 240;

/// The minimap's first column and how many columns it spans: the whole of a bounded world, or a
/// window centred on the rig's column `player_x` in an endless (streamed) one.
fn minimap_columns(terrain: &Terrain, player_x: i32) -> (i32, i32) {
    if terrain.is_streamed() {
        (
            player_x - MINIMAP_STREAMED_COLUMNS / 2,
            MINIMAP_STREAMED_COLUMNS,
        )
    } else {
        (terrain.min_x(), terrain.width().max(1))
    }
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
    draw_ruler_strata(draw, terrain, view, (x, top, height), max_depth);
    let step = match max_depth {
        0..=180 => 20,
        181..=500 => 50,
        _ => 100,
    };
    for marker in (0..=max_depth).step_by(usize::try_from(step).unwrap_or(20)) {
        let y = top + marker * (height - 1) / max_depth;
        draw.draw_rectangle(x - 3, y, 3, 1, FOG);
    }
    let depth = (view.player.y / TILE_SIZE).max(0.0);
    let marker_y = top + ((depth / max_depth as f32).clamp(0.0, 1.0) * (height - 1) as f32) as i32;
    // A left-pointing marker: ink outline around a gold arrow (mint while flying above the
    // ground, pinned to the top: the sky has no end to show).
    let (body, shade) = if view.hud.altitude.is_some() {
        (MINT, TEAL)
    } else {
        (GOLD, AMBER)
    };
    draw.draw_rectangle(x - 4, marker_y - 3, 11, 7, INK);
    draw.draw_rectangle(x - 3, marker_y - 2, 9, 5, body);
    draw.draw_rectangle(x - 3, marker_y + 1, 9, 1, shade);
}

/// Tints the depth ruler's well with the band colours of the rig's own column, so the wavy
/// layer boundaries of this world show where the next layer starts.
fn draw_ruler_strata(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
    (x, top, height): (i32, i32, i32),
    max_depth: i32,
) {
    let column = view.player.tile_position(TILE_SIZE).x;
    let band_at_row = |row: i32| {
        let tile_y = row * max_depth / (height - 1).max(1);
        (tile_y > super::terrain::tile_art::SKY_MAX_TILE_Y).then(|| {
            terrain.band_at(TilePosition {
                x: column,
                y: tile_y,
            })
        })
    };
    let mut start = 0;
    let mut current = band_at_row(0);
    for row in 1..=height {
        let band = if row < height { band_at_row(row) } else { None };
        if band == current && row < height {
            continue;
        }
        if let Some(band) = current {
            let color = super::terrain::tile_art::material_color(band.soft_material(), band);
            draw.draw_rectangle(x + 1, top + start, 4, row - start, color.alpha(0.75));
        }
        start = row;
        current = band;
    }
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
        ModalScreen::WarpToLayer => menus::draw_warp_layers(draw, ui),
        ModalScreen::QuestDesk => quests::draw_quest_desk(draw, ui),
        ModalScreen::QuestLog => quests::draw_quest_log(draw, ui),
        ModalScreen::Garage => super::garage::draw_garage(draw, ui),
        ModalScreen::VillageCounter => counters::draw_village_counter(draw, ui),
        ModalScreen::VillageTalk => counters::draw_village_talk(draw, ui),
        ModalScreen::PersonTrade => counters::draw_person_trade(draw, ui),
        ModalScreen::PersonGift => counters::draw_person_gift(draw, ui),
        ModalScreen::BaseModule => super::build::draw_base_module(draw, ui),
        ModalScreen::Achievements => super::rewards::draw_achievements(draw, ui),
        ModalScreen::Colony => super::moon_hud::draw_colony(draw, ui),
    }
}

/// The collection log: sites visited, scanner contacts, recovered logs and mining intelligence.
fn draw_research_log_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut sections = poi_map::collection_sections(ui.snapshot.terrain, ui.player);
    sections.extend(cave_map::collection_section(ui.snapshot.terrain, ui.player));
    sections.push(Section::new(
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
        ],
    ));
    UiLayout::screen(draw).modal_with_render_plan(
        "Collection Log",
        "Sites you have found, scanner contacts and recovered logs.",
        &ModalContent::new(sections)
            .prompts(&[hint(Prompt::Confirm, "map"), hint(Prompt::Back, "close")]),
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "inventory content model builds several dynamic sections"
)]
fn draw_inventory_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let mut sections = Vec::new();
    let (hopper, hopper_about) = counters::hopper_section(ui);
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
        ],
    ));
    // Every explosive type, carried or not, so players can see what the shack stocks.
    sections.push(Section::new(
        "Explosives",
        ORANGE,
        fatherload_simulation::ExplosiveKind::SELECTABLE
            .into_iter()
            .map(|kind| {
                let count = ui.player.explosive_count(kind);
                let icon = if kind == fatherload_simulation::ExplosiveKind::Bomb {
                    &icons::BOMB
                } else {
                    explosive_art::icon(kind)
                };
                SectionItem::icon_stat(
                    icon,
                    kind.name(),
                    if count == 0 {
                        "none".to_owned()
                    } else {
                        format!("x{count}")
                    },
                    if count == 0 { MIST } else { ORANGE },
                )
            })
            .collect(),
    ));
    // Hardpoint weapons, the capacitor and ammo (once the rig owns any).
    let rack = &ui.player.weapon_rack;
    let rockets = ui.player.ammo(fatherload_simulation::AmmoKind::Rocket);
    if !rack.owned.is_empty() || rockets > 0 {
        let mounted = ui.player.mounted_weapon.map(|mounted| mounted.kind);
        let mut items: Vec<SectionItem> = rack
            .owned
            .iter()
            .map(|(weapon, mk)| {
                SectionItem::icon_stat(
                    crate::rendering::pixel_art::sprites::weapons::icon(*weapon),
                    weapon.name(),
                    if mounted == Some(*weapon) {
                        format!("{} | mounted", crate::shell::arms::mk_name(*mk))
                    } else {
                        crate::shell::arms::mk_name(*mk).to_owned()
                    },
                    if mounted == Some(*weapon) { MINT } else { MIST },
                )
            })
            .collect();
        items.push(SectionItem::icon_stat(
            &crate::rendering::pixel_art::sprites::weapons::ICON_BATTERY,
            "Capacitor",
            format!(
                "{:.0}/{:.0}",
                ui.player.weapon.energy,
                ui.player.battery_capacity()
            ),
            MINT,
        ));
        items.push(SectionItem::icon_stat(
            &crate::rendering::pixel_art::sprites::weapons::ICON_ROCKET_AMMO,
            "Rockets",
            format!("x{rockets}"),
            if rockets > 0 { ORANGE } else { MIST },
        ));
        sections.push(Section::new("Arms", MINT, items));
    }
    let mineral_items = if ui.player.cargo_bay.has_minerals() {
        ui.player
            .cargo_bay
            .minerals()
            .map(|(mineral, quantity)| {
                let value = ui.snapshot.market_value(mineral).saturating_mul(quantity);
                SectionItem::icon_stat(
                    icons::mineral(mineral),
                    format!("{} x{quantity}", mineral.name()),
                    format!("est. {value} cr"),
                    GOLD,
                )
            })
            .collect()
    } else {
        vec![SectionItem::text("No minerals onboard.")]
    };
    sections.push(Section::new("Minerals", BONE, mineral_items));
    let artifact_items = if ui.player.cargo_bay.has_artifacts() {
        ui.player
            .cargo_bay
            .artifacts()
            .map(|(artifact, quantity)| {
                SectionItem::stat(
                    artifact.name(),
                    format!(
                        "x{quantity} | base {} cr",
                        artifact.value().saturating_mul(quantity)
                    ),
                    PINK,
                )
            })
            .collect()
    } else {
        vec![SectionItem::text("No artifacts onboard.")]
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
            (&rig::DRILL_DOWN, "Drill", UpgradeKind::Drill),
            (&menu_icons::GEAR, "Engine", UpgradeKind::Engine),
            (&icons::FUEL, "Fuel Tank", UpgradeKind::FuelTank),
            (&icons::CARGO, "Cargo Bay", UpgradeKind::CargoBay),
            (&icons::HULL, "Hull", UpgradeKind::Hull),
            (&icons::HEAT, "Radiator", UpgradeKind::Radiator),
            (&icons::SCANNER, "Scanner", UpgradeKind::Scanner),
            (&rig::DRILL_UP, "Drill Mount", UpgradeKind::DrillMount),
            (
                &icons::HEAT_SHIELD,
                "Heat Shielding",
                UpgradeKind::HeatShield,
            ),
            (
                &icons::LAVA_PLATING,
                "Lava Plating",
                UpgradeKind::LavaPlating,
            ),
            (
                &icons::GAS_SCRUBBER,
                "Gas Scrubber",
                UpgradeKind::GasScrubber,
            ),
            (
                &icons::PRESSURE_BRACING,
                "Pressure Bracing",
                UpgradeKind::PressureBracing,
            ),
            (
                &icons::BOULDER_DEFLECTOR,
                "Boulder Deflector",
                UpgradeKind::BoulderDeflector,
            ),
            (
                &icons::ALTITUDE_INTAKE,
                "Altitude Intake",
                UpgradeKind::AltitudeIntake,
            ),
            (&icons::DE_ICER, "De-Icer", UpgradeKind::DeIcer),
            (
                &crate::rendering::pixel_art::sprites::moon::PRESSURIZED_HULL,
                "Pressurized Hull",
                UpgradeKind::PressurizedHull,
            ),
            (
                &crate::rendering::pixel_art::sprites::moon::LIFE_SUPPORT,
                "Life Support",
                UpgradeKind::LifeSupport,
            ),
        ]
        .into_iter()
        .map(
            |(icon, label, kind): (&'static SpriteDef, &str, UpgradeKind)| {
                let tier = current_level(ui.player, kind);
                let max = kind.max_level();
                SectionItem::icon_stat(
                    icon,
                    label,
                    format!("tier {tier}/{max}"),
                    if tier >= max { LIME } else { MIST },
                )
            },
        )
        .collect(),
    ));
    // The hopper's rows, then the pilot pack's, are the dialog's selectable ones: right after
    // the cargo.
    let (pack, pack_about) = counters::pack_section(ui);
    sections.insert(1, hopper);
    sections.insert(2, pack);
    UiLayout::screen(draw).modal_with_render_plan(
        "Inventory",
        "Cargo, blocks, the pilot's pack, artifacts, consumables and field kits.",
        &ModalContent::new(sections)
            .footer(hopper_about.or(pack_about))
            .prompts(&[
                hint(Prompt::Confirm, "toggle / dump / move"),
                hint(Prompt::Inventory, ""),
                hint(Prompt::Back, "close"),
            ]),
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

/// Map legend entries drawn with an icon: a known site and a scanner echo.
const MAP_LEGEND_ICONS: [(&SpriteDef, &str); 2] = [
    (&super::pixel_art::sprites::poi::ICON_VAULT, "Site"),
    (&super::pixel_art::sprites::poi::ICON_ECHO, "Echo"),
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
        .chain(
            MAP_LEGEND_ICONS
                .iter()
                .map(|(_, label)| ui_kit::text_width(label, TextStyle::Body) + 12),
        )
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
    for ((icon, label), (lx, ly)) in MAP_LEGEND_ICONS
        .iter()
        .zip(positions.iter().skip(MAP_LEGEND.len()))
    {
        ui_kit::icon(draw, icon, *lx, *ly);
        ui_kit::text(draw, label, lx + 12, *ly, TextStyle::Body, MIST);
    }
    if let Some((lx, ly)) = positions.get(MAP_LEGEND.len() + MAP_LEGEND_ICONS.len()) {
        ui_kit::icon(
            draw,
            &super::pixel_art::sprites::menu_icons::PERSON,
            *lx,
            *ly,
        );
        ui_kit::text(draw, "You", lx + 12, *ly, TextStyle::Body, MIST);
    }
}

/// The map footer: depth and the layer the rig is in, deepest depth and distance from the west
/// edge.
fn map_footer(ui: &UiFrame<'_, '_>) -> String {
    let hud = ui.view.hud;
    let height = hud.altitude.map_or_else(
        || format!("Depth {} m", hud.depth),
        |altitude| format!("Altitude {altitude} m"),
    );
    let layer = ui
        .snapshot
        .terrain
        .band_at(ui.player.tile_position(TILE_SIZE));
    let terrain = ui.snapshot.terrain;
    let column = ui.player.x / TILE_SIZE;
    let across = if terrain.is_streamed() {
        // An endless world has no edge: say how far the town is, and which way.
        town_heading(column)
    } else {
        format!("{column:.0} m from the west edge")
    };
    format!(
        "{height} ({}) | deepest {} m | {across}",
        layer.name(),
        ui.snapshot.progression.deepest_tile_reached,
    )
}

/// How far, and which way, the town lies from a rig at column `column` (a fractional tile).
fn town_heading(column: f32) -> String {
    let (first, last) = fatherload_simulation::surface::town_span();
    #[allow(clippy::cast_precision_loss, reason = "town columns are small")]
    let (first, last) = (first as f32, last as f32 + 1.0);
    if column < first {
        format!("town {:.0} m east", first - column)
    } else if column > last {
        format!("town {:.0} m west", column - last)
    } else {
        "in town".to_owned()
    }
}

/// The other rigs on the mine map, each in its owner's paint (as on the minimap and name tags).
fn draw_map_other_rigs(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    (x0, y0): (i32, i32),
    (origin_x, origin_y): (i32, i32),
    (columns, rows): (i32, i32),
    pixel: i32,
) {
    for remote in ui
        .snapshot
        .players
        .iter()
        .filter(|other| other.player_id != ui.view.player_id)
    {
        // Floor, not truncation: endless worlds have negative columns.
        let (tx, ty) = (
            (remote.x / TILE_SIZE).floor() as i32,
            (remote.y / TILE_SIZE).floor() as i32,
        );
        if (origin_x..origin_x + columns).contains(&tx) && (origin_y..origin_y + rows).contains(&ty)
        {
            let (rx, ry) = (x0 + (tx - origin_x) * pixel, y0 + (ty - origin_y) * pixel);
            draw.draw_rectangle(rx - 2, ry - 2, 6, 6, INK);
            draw.draw_rectangle(
                rx - 1,
                ry - 1,
                4,
                4,
                player_accent(remote.player_id, Some(remote.cosmetics)),
            );
        }
    }
}

/// The mine map: discovered terrain at two art pixels per tile around the rig, with depth
/// marks, the rig's blinking marker and a legend.
fn draw_map_ui(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let map_rect = UiLayout::screen(draw).canvas_modal(
        "Mine Map",
        "Discovered terrain and scanner contacts.",
        &[
            hint(Prompt::Confirm, "log"),
            hint(Prompt::Map, ""),
            hint(Prompt::Back, "close"),
        ],
        &map_footer(ui),
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
    let terrain_height = terrain.height().max(1);
    let pixel = 2;
    let (columns, rows) = (width / pixel, height / pixel);
    let player = ui.player.tile_position(TILE_SIZE);
    // Centred on the rig, kept inside a bounded world's columns (an endless world scrolls).
    let origin_x = (player.x - columns / 2).clamp(
        terrain.min_x(),
        (terrain.max_x() + 1 - columns).max(terrain.min_x()),
    );
    // Centred on the rig, kept between the world's first row (a biome world's mountain tops)
    // and its floor; a rig high in the sky sees the ground below it, its marker pinned to the
    // map's top edge.
    let top_row = terrain.min_y();
    let origin_y = (map_focus_row(terrain, player, rows) - rows / 2)
        .clamp(top_row, (terrain_height - rows).max(top_row));
    for row in 0..rows.min(terrain_height - origin_y) {
        let tile_y = origin_y + row;
        for column in 0..columns.min(terrain.max_x() + 1 - origin_x) {
            let position = TilePosition {
                x: origin_x + column,
                y: tile_y,
            };
            let Some(color) = map_tile_color(terrain, position, ui.view.is_explored(position))
            else {
                continue;
            };
            draw.draw_rectangle(x0 + column * pixel, y0 + row * pixel, pixel, pixel, color);
        }
    }
    draw_map_depth_scale(draw, (x0, y0), origin_y, rows, pixel);
    let clip = Rectangle::new(x0 as f32, y0 as f32, width as f32, height as f32);
    let markers = poi_map::markers(terrain, ui.player);
    poi_map::draw_map_markers(draw, &markers, clip, (origin_x, origin_y), pixel);
    cave_map::draw_map_markers(
        draw,
        &cave_map::markers(terrain, ui.player),
        clip,
        (origin_x, origin_y),
        pixel,
    );
    village_map::draw_map_villages(draw, terrain, ui.player, clip, (origin_x, origin_y), pixel);
    quests::draw_map_quest_markers(
        draw,
        &ui.view.quest_markers,
        clip,
        (origin_x, origin_y),
        pixel,
    );
    // The rig: an ink-ringed marker that blinks.
    let marker_x = x0 + (player.x - origin_x) * pixel;
    let marker_y = y0 + (player.y - origin_y).max(1) * pixel;
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
    draw_map_other_rigs(
        draw,
        ui,
        (x0, y0),
        (origin_x, origin_y),
        (columns, rows),
        pixel,
    );
    draw_map_legend(
        draw,
        well.x as i32 + 2,
        (well.y + well.height) as i32 + 4,
        well.width as i32 - 4,
    );
}

/// The map's depth scale: a tick and label every 10 or 20 m down its left edge.
fn draw_map_depth_scale(
    draw: &mut RaylibDrawHandle<'_>,
    (x0, y0): (i32, i32),
    origin_y: i32,
    rows: i32,
    pixel: i32,
) {
    let step = if rows > 60 { 20 } else { 10 };
    let first = (origin_y + step - 1).div_euclid(step) * step;
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
}

/// The controls reference: every action with its keys or buttons for the menu device, in two
/// columns (actions without a binding on that device are left out).
#[allow(clippy::too_many_lines, reason = "the controls table is data")]
fn draw_help_ui(draw: &mut RaylibDrawHandle<'_>) {
    type Rows = &'static [(Prompt, &'static str)];
    const GROUPS: [(&str, Color, Rows); 3] = [
        (
            "Movement",
            SKY_LIGHT,
            &[
                (Prompt::Walk, "Move / dig sideways"),
                (Prompt::Thrust, "Thrust / crown bore"),
                (Prompt::Drill, "Drill down"),
                (Prompt::Use, "Use building"),
                (Prompt::ExitRig, "Climb out / board"),
                (Prompt::Jump, "Jump / jetpack (on foot)"),
                (Prompt::Climb, "Climb ladders (on foot)"),
                (Prompt::Crouch, "Crouch / crawl (on foot)"),
                (Prompt::Pick, "Hand pick (on foot)"),
                (Prompt::Pause, "Pause"),
            ],
        ),
        (
            "Tools",
            GOLD,
            &[
                (Prompt::Scan, "Scanner"),
                (Prompt::Bomb, "Explosive"),
                (Prompt::CycleExplosive, "Next explosive"),
                (Prompt::Aim, "Aim throws"),
                (Prompt::FireWeapon, "Fire weapon"),
                (Prompt::AimWeapon, "Aim weapon"),
                (Prompt::CycleWeapon, "Next weapon"),
                (Prompt::Detonate, "Detonate C4"),
                (Prompt::DetonateSequence, "Detonate in sequence"),
                (Prompt::Map, "Map, logs, milestones"),
                (Prompt::Inventory, "Inventory"),
                (Prompt::Infrastructure, "Field kits"),
                (Prompt::BuildMode, "Build mode"),
            ],
        ),
        (
            "Menus",
            LIME,
            &[
                (Prompt::Navigate, "Select"),
                (Prompt::Confirm, "Confirm"),
                (Prompt::Back, "Back / close"),
                (Prompt::Tab, "Shop columns, garage options"),
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
