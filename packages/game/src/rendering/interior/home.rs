//! Contractor-built home interiors (WP-HOME): the room the seat walks in, in its own
//! wallpaper and floor, with windows that show the real time of day (the sky's own colour
//! keyframes, stars at night, space on the Moon and clouds on a sky island), the front door,
//! stairs, the basement's hatch and lift, the garage bay with the rig parked in it, and every
//! piece of furniture where the decor grid puts it (trophy cases show their relic or gem).
//!
//! Grid: [`ROOM_COLUMNS`] columns of 32 art pixels from x 32; the floor row stands on the
//! floor, the wall row hangs at eye height and the ceiling row hangs from the ceiling.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::many_single_char_names,
    reason = "procedural art mixes small integers and pixel coordinates"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    clock::WorldClock,
    home::{
        Flooring, Furniture, Home, Plot, PlotSetting, Room, RoomKind, Wallpaper,
        decor::{DecorRow, PlacedDecor, ROOM_COLUMNS},
    },
    item::ItemKind,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::{
    CEILING_Y, FLOOR_Y, RoomArea, RoomPlacement, WAINSCOT_Y, draw_controls_hint, draw_door,
    draw_key_prompt, draw_name_plate, room_pixel_x,
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    rendering::{
        pixel_art::{
            sprite,
            sprites::{icons, terrain as terrain_art},
        },
        prompts::hint,
        rig_art,
        ui_kit::{self, UiCanvasScope},
        world::daylight::DayLook,
    },
    shell::{
        SeatUi,
        homes::{self, HOME_DOOR_X, HOME_REACH, STAIRS_DOWN_X, STAIRS_UP_X},
    },
};

/// Art x of the grid's first column.
const GRID_X: i32 = 32;
/// Art width of a grid column.
const CELL_W: i32 = 32;
/// Middle of the wall row.
const WALL_ROW_Y: i32 = 80;

/// Draws the room `seat` stands in, then its name plate and prompts.
pub(in crate::rendering) fn draw_home_interior(
    draw: &mut RaylibDrawHandle<'_>,
    seat: &SeatUi,
    view: &ViewSnapshot<'_>,
    snapshot: &RenderSnapshot<'_>,
    area: RoomArea,
    player: Option<usize>,
) {
    let Some(inside) = seat.home.interior else {
        return;
    };
    let Some(plot) = snapshot.homes.plot(inside.plot) else {
        return;
    };
    let Some(home) = plot.home.as_ref() else {
        return;
    };
    let Some(room) = home.rooms.get(inside.room) else {
        return;
    };
    let placement = RoomPlacement::fit_area(area);
    let seconds = ui_kit::seconds();
    let palette = wall_palette(room.decor.wallpaper);
    match area.viewport {
        None => draw.clear_background(palette.shadow),
        Some(viewport) => draw.draw_rectangle(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            palette.shadow,
        ),
    }
    let floor_rooms: Vec<usize> = home.rooms_on(room.floor).map(|(index, _)| index).collect();
    let first_on_floor = floor_rooms.first() == Some(&inside.room);
    let (low, high) = home.floor_span();
    let look = DayLook::at(snapshot.clock);
    let lit = room.decor.lit();
    {
        let mut d = draw.begin_mode2D(placement.camera());
        draw_shell(&mut d, room, placement);
        if room.kind.has_windows() {
            for x in window_columns(room) {
                draw_window(&mut d, x, plot, &look, snapshot.clock, seconds);
            }
        }
        match room.kind {
            RoomKind::GarageBay => draw_garage(&mut d, snapshot, view, plot, seconds),
            RoomKind::Basement => draw_basement(&mut d, home, seconds),
            RoomKind::Airlock => draw_porthole(&mut d, 150, seconds),
            _ => {}
        }
        if room.floor == 0 && first_on_floor {
            draw_door(&mut d);
        }
        if first_on_floor {
            if room.floor < high {
                draw_stairs_up(&mut d, room_pixel_x(STAIRS_UP_X));
            }
            if room.floor > low {
                draw_stairs_down(&mut d, room_pixel_x(STAIRS_DOWN_X));
            }
        }
        for (cell, piece) in &room.decor.rugs {
            draw_piece(&mut d, *cell, *piece, seconds, lit);
        }
        // Back to front: ceiling, wall, then floor pieces.
        for row in [DecorRow::Ceiling, DecorRow::Wall, DecorRow::Floor] {
            for (cell, piece) in room
                .decor
                .pieces
                .iter()
                .filter(|(cell, _)| DecorRow::of_cell(**cell) == row)
            {
                draw_piece(&mut d, *cell, *piece, seconds, lit);
            }
        }
        if let Some((cell, furniture)) = seat.home.preview(seat.selected_menu_item) {
            draw_ghost(&mut d, cell, furniture, seconds);
        }
        draw_room_light(&mut d, room, &look, seconds);
        draw_pilot(
            &mut d,
            view,
            inside.x,
            inside.facing,
            inside.stride,
            seconds,
        );
        // An unlit room at night is dim; lamps and a fire keep it warm.
        if look.darkness > 0.05 && !lit {
            let (left, right) = placement.visible_x();
            let (top, bottom) = placement.visible_y();
            d.draw_rectangle(
                left,
                top,
                right - left,
                bottom - top,
                with_alpha(NIGHT, (look.darkness * 150.0) as u8),
            );
        }
    }

    let _canvas = UiCanvasScope::begin(draw);
    let canvas = ui_kit::canvas();
    let accent = rig_art::player_accent(view.player_id, view.player.cosmetics);
    let title = format!("{}: {}", home.blueprint.name(), room.kind.name());
    draw_name_plate(draw, &title, player, accent);
    if seat.modal.is_some() {
        return;
    }
    draw_controls_hint(draw, canvas);
    let x = inside.x;
    let prompt = if room.floor == 0 && first_on_floor && (x - HOME_DOOR_X).abs() <= HOME_REACH {
        Some((room_pixel_x(HOME_DOOR_X), "Leave"))
    } else if first_on_floor && room.floor < high && (x - STAIRS_UP_X).abs() <= HOME_REACH {
        Some((room_pixel_x(STAIRS_UP_X), "Upstairs"))
    } else if first_on_floor && room.floor > low && (x - STAIRS_DOWN_X).abs() <= HOME_REACH {
        Some((room_pixel_x(STAIRS_DOWN_X), "Downstairs"))
    } else if room.kind == RoomKind::Basement && (x - 120.0).abs() <= HOME_REACH {
        Some((60, "Down the hatch"))
    } else if room.kind == RoomKind::Basement && home.lift && (x - 440.0).abs() <= HOME_REACH {
        Some((220, "Ride the lift"))
    } else {
        homes::cell_column_at(x)
            .and_then(|column| room.decor.piece_at(column))
            .and_then(|(cell, piece)| {
                piece.furniture.service().map(|service| {
                    let label = match service {
                        fatherload_simulation::home::DecorService::Sleep => "Sleep",
                        fatherload_simulation::home::DecorService::Cook => "Cook",
                        fatherload_simulation::home::DecorService::Store => "Storage",
                        fatherload_simulation::home::DecorService::Display => "Trophy case",
                        fatherload_simulation::home::DecorService::Craft => "Workbench",
                        fatherload_simulation::home::DecorService::Stargaze => "Look",
                    };
                    let width = i32::from(piece.furniture.width()) * CELL_W;
                    (
                        GRID_X + i32::from(cell % ROOM_COLUMNS) * CELL_W + width / 2,
                        label,
                    )
                })
            })
            .or_else(|| Some((room_pixel_x(x), "Home menu")))
    };
    if let Some((target_x, label)) = prompt {
        let (prompt_x, prompt_y) = placement.to_canvas(canvas, target_x, FLOOR_Y - 46);
        draw_key_prompt(draw, prompt_x, prompt_y, hint(Prompt::Use, label));
    }
}

/// A wallpaper's colours.
#[derive(Clone, Copy)]
struct WallLook {
    base: Color,
    pattern: Color,
    wainscot: Color,
    wainscot_dark: Color,
    shadow: Color,
}

const fn wall_palette(paper: Wallpaper) -> WallLook {
    match paper {
        Wallpaper::Plaster => WallLook {
            base: CREAM,
            pattern: SAND,
            wainscot: LOAM,
            wainscot_dark: SOIL,
            shadow: SOIL_DARK,
        },
        Wallpaper::Timber => WallLook {
            base: CLAY,
            pattern: LOAM,
            wainscot: SOIL,
            wainscot_dark: SOIL_DARK,
            shadow: SOIL_DARK,
        },
        Wallpaper::Floral => WallLook {
            base: ROSE,
            pattern: CREAM,
            wainscot: MAROON,
            wainscot_dark: SOIL_DARK,
            shadow: PLUM,
        },
        Wallpaper::Stripes => WallLook {
            base: MINT,
            pattern: TEAL,
            wainscot: TEAL_DARK,
            wainscot_dark: DEEP_BLUE,
            shadow: DEEP_BLUE,
        },
        Wallpaper::Brick => WallLook {
            base: RUST,
            pattern: MAROON,
            wainscot: STONE_DARK,
            wainscot_dark: INK,
            shadow: INK,
        },
        Wallpaper::Midnight => WallLook {
            base: DEEP_BLUE,
            pattern: VIOLET,
            wainscot: SLATE_DARK,
            wainscot_dark: INK,
            shadow: NIGHT,
        },
    }
}

/// Ceiling, papered wall, wainscot and floor across everything visible.
fn draw_shell(d: &mut impl RaylibDraw, room: &Room, placement: RoomPlacement) {
    let (left, right) = placement.visible_x();
    let (top, bottom) = placement.visible_y();
    let look = wall_palette(room.decor.wallpaper);
    let width = right - left;
    // Ceiling with beams.
    d.draw_rectangle(left, top, width, CEILING_Y - top, look.shadow);
    let first = left.div_euclid(40) * 40;
    for x in (first..right).step_by(40) {
        d.draw_rectangle(x, top, 5, CEILING_Y - top, INK);
    }
    d.draw_rectangle(left, CEILING_Y - 2, width, 2, INK);
    // The wall.
    d.draw_rectangle(left, CEILING_Y, width, WAINSCOT_Y - CEILING_Y, look.base);
    let first = left.div_euclid(16) * 16;
    match room.decor.wallpaper {
        Wallpaper::Plaster => {
            for x in (first..right).step_by(16) {
                for y in (CEILING_Y + 6..WAINSCOT_Y).step_by(12) {
                    let jitter = ((x * 7 + y * 3).rem_euclid(5)) - 2;
                    d.draw_rectangle(x + 6 + jitter, y, 2, 1, look.pattern);
                }
            }
        }
        Wallpaper::Timber => {
            for x in (first..right).step_by(16) {
                d.draw_rectangle(x, CEILING_Y, 1, WAINSCOT_Y - CEILING_Y, look.pattern);
                d.draw_rectangle(x + 8, CEILING_Y + 20, 1, 3, look.pattern);
                d.draw_rectangle(x + 4, CEILING_Y + 52, 1, 4, look.pattern);
            }
        }
        Wallpaper::Floral => {
            for x in (first..right).step_by(16) {
                for (row, y) in (CEILING_Y + 8..WAINSCOT_Y - 4).step_by(14).enumerate() {
                    let shift = if row % 2 == 0 { 0 } else { 8 };
                    let cx = x + 4 + shift;
                    d.draw_rectangle(cx, y, 3, 3, look.pattern);
                    d.draw_rectangle(cx + 1, y + 1, 1, 1, GOLD);
                    d.draw_rectangle(cx + 1, y + 3, 1, 2, MOSS);
                }
            }
        }
        Wallpaper::Stripes => {
            for x in (first..right).step_by(16) {
                d.draw_rectangle(x, CEILING_Y, 5, WAINSCOT_Y - CEILING_Y, look.pattern);
                d.draw_rectangle(x + 9, CEILING_Y, 1, WAINSCOT_Y - CEILING_Y, look.pattern);
            }
        }
        Wallpaper::Brick => {
            for (row, y) in (CEILING_Y..WAINSCOT_Y).step_by(8).enumerate() {
                let shift = if row % 2 == 0 { 0 } else { 8 };
                d.draw_rectangle(left, y, width, 1, look.pattern);
                for x in (first - 16..right).step_by(16) {
                    d.draw_rectangle(x + shift, y, 1, 8, look.pattern);
                }
            }
        }
        Wallpaper::Midnight => {
            for x in (first..right).step_by(16) {
                for y in (CEILING_Y + 5..WAINSCOT_Y).step_by(11) {
                    if (x / 16 + y / 11) % 3 == 0 {
                        d.draw_rectangle(x + 7, y, 1, 1, GOLD);
                    } else {
                        d.draw_rectangle(x + 3, y + 4, 1, 1, look.pattern);
                    }
                }
            }
        }
    }
    d.draw_rectangle(left, CEILING_Y, width, 3, with_alpha(INK, 90));
    // Wainscot with a rail.
    d.draw_rectangle(left, WAINSCOT_Y, width, FLOOR_Y - WAINSCOT_Y, look.wainscot);
    d.draw_rectangle(left, WAINSCOT_Y, width, 2, look.wainscot_dark);
    d.draw_rectangle(left, WAINSCOT_Y + 2, width, 1, with_alpha(BONE, 60));
    for x in (left.div_euclid(24) * 24..right).step_by(24) {
        d.draw_rectangle(
            x,
            WAINSCOT_Y + 6,
            1,
            FLOOR_Y - WAINSCOT_Y - 8,
            look.wainscot_dark,
        );
    }
    // The floor.
    let (floor, line, light) = match room.decor.flooring {
        Flooring::Planks => (LOAM, SOIL, CLAY),
        Flooring::Stone => (STONE, STONE_DARK, STONE_LIGHT),
        Flooring::Tiles => (BONE, SLATE, CREAM),
        Flooring::Carpet => (MAROON, PLUM, RED),
        Flooring::Parquet => (CLAY, LOAM, SAND),
    };
    d.draw_rectangle(left, FLOOR_Y, width, bottom - FLOOR_Y, floor);
    d.draw_rectangle(left, FLOOR_Y, width, 1, INK);
    d.draw_rectangle(left, FLOOR_Y + 1, width, 1, light);
    match room.decor.flooring {
        Flooring::Planks | Flooring::Parquet => {
            for (row, y) in (FLOOR_Y + 4..FLOOR_Y + 40).step_by(6).enumerate() {
                d.draw_rectangle(left, y, width, 1, line);
                let step = if room.decor.flooring == Flooring::Parquet {
                    12
                } else {
                    28
                };
                let shift = (row as i32 * 11) % step;
                for x in (left.div_euclid(step) * step..right).step_by(step as usize) {
                    d.draw_rectangle(x + shift, y - 5, 1, 5, line);
                }
            }
        }
        Flooring::Stone => {
            for (row, y) in (FLOOR_Y + 2..FLOOR_Y + 40).step_by(9).enumerate() {
                d.draw_rectangle(left, y, width, 1, line);
                let shift = if row % 2 == 0 { 0 } else { 10 };
                for x in (left.div_euclid(20) * 20..right).step_by(20) {
                    d.draw_rectangle(x + shift, y, 1, 9, line);
                }
            }
        }
        Flooring::Tiles => {
            for (row, y) in (FLOOR_Y + 2..FLOOR_Y + 40).step_by(8).enumerate() {
                for (column, x) in (left.div_euclid(8) * 8..right).step_by(8).enumerate() {
                    if (row + column) % 2 == 0 {
                        d.draw_rectangle(x, y, 8, 8, line);
                    }
                }
            }
        }
        Flooring::Carpet => {
            for x in (left.div_euclid(6) * 6..right).step_by(6) {
                d.draw_rectangle(x, FLOOR_Y + 6 + (x / 6) % 3, 1, 1, line);
            }
            d.draw_rectangle(left, FLOOR_Y + 3, width, 1, GOLD);
        }
    }
    for (index, alpha) in [40_u8, 80, 120].into_iter().enumerate() {
        d.draw_rectangle(
            left,
            FLOOR_Y + 16 + index as i32 * 8,
            width,
            8,
            with_alpha(INK, alpha),
        );
    }
    d.draw_rectangle(
        left,
        FLOOR_Y + 40,
        width,
        (bottom - FLOOR_Y - 40).max(0),
        with_alpha(INK, 160),
    );
}

/// Window x positions (art) for a room: two windows clear of the door and the stairs.
fn window_columns(room: &Room) -> Vec<i32> {
    match room.kind {
        RoomKind::Observatory => vec![110, 170],
        _ => vec![86, 206],
    }
}

/// A window at art `x`: the sky outside at this time of day.
fn draw_window(
    d: &mut impl RaylibDraw,
    x: i32,
    plot: &Plot,
    look: &DayLook,
    clock: WorldClock,
    seconds: f32,
) {
    let (y, w, h) = (26, 30, 30);
    d.draw_rectangle(x - 2, y - 2, w + 4, h + 4, INK);
    d.draw_rectangle(x - 1, y - 1, w + 2, h + 2, SOIL);
    let band_h = h / 5;
    let space = plot.site.setting == PlotSetting::Moon;
    for (index, band) in look.bands.iter().enumerate() {
        let rect = Rectangle::new(
            x as f32,
            (y + index as i32 * band_h) as f32,
            w as f32,
            if index == 4 {
                (h - 4 * band_h) as f32
            } else {
                band_h as f32
            },
        );
        if space {
            d.draw_rectangle_rec(rect, NIGHT);
        } else {
            band.fill(d, rect);
        }
    }
    // Stars when they are out (always on the Moon).
    let stars = if space { 1.0 } else { look.stars };
    if stars > 0.1 {
        for index in 0..7 {
            let sx = x + 2 + (index * 13 + plot.id.0 as i32 * 5) % (w - 4);
            let sy = y + 2 + (index * 7) % (h / 2);
            let twinkle = ((seconds * 1.7 + index as f32).sin() * 0.5 + 0.5) * stars;
            d.draw_rectangle(sx, sy, 1, 1, with_alpha(BONE, (twinkle * 255.0) as u8));
        }
    }
    // The sun or Moon passing.
    let height = clock.sun_height();
    if !space && height > 0.0 {
        let sx = x + (clock.day_fraction() * 3.0 * w as f32) as i32 % w;
        let sy = y + h - 4 - (height * (h - 8) as f32) as i32;
        d.draw_rectangle(sx, sy, 4, 4, with_alpha(CREAM, 220));
    } else if clock.is_night() || space {
        let mx = x + w - 10;
        d.draw_rectangle(mx, y + 5, 5, 5, with_alpha(BONE, 230));
        d.draw_rectangle(mx + 3, y + 5, 2, 5, with_alpha(SLATE, 200));
    }
    // What lies beyond: hills, a cloud sea up high, regolith on the Moon.
    match plot.site.setting {
        PlotSetting::SkyIsland => {
            for index in 0..4 {
                let cx = x + (index * 9 + (seconds * 2.0) as i32) % w;
                d.draw_rectangle(cx, y + h - 6, 8, 3, with_alpha(BONE, 200));
            }
            d.draw_rectangle(x, y + h - 3, w, 3, with_alpha(MIST, 220));
        }
        PlotSetting::Moon => {
            d.draw_rectangle(x, y + h - 5, w, 5, STONE);
            d.draw_rectangle(x + 6, y + h - 5, 6, 1, STONE_LIGHT);
            // The world, far below and blue.
            d.draw_rectangle(x + 4, y + 8, 6, 6, OCEAN);
            d.draw_rectangle(x + 5, y + 9, 2, 2, LEAF);
        }
        PlotSetting::Lakeside => {
            d.draw_rectangle(x, y + h - 6, w, 6, look.hills.base);
            d.draw_rectangle(x, y + h - 3, w, 3, with_alpha(OCEAN, 200));
        }
        _ => {
            let hills = look.hills;
            for index in 0..w {
                let ridge = ((index as f32 * 0.35 + plot.id.0 as f32).sin() * 2.0 + 5.0) as i32;
                hills.fill(
                    d,
                    Rectangle::new(
                        (x + index) as f32,
                        (y + h - ridge) as f32,
                        1.0,
                        ridge as f32,
                    ),
                );
            }
        }
    }
    // Frame cross and sill.
    d.draw_rectangle(x + w / 2, y, 1, h, SOIL);
    d.draw_rectangle(x, y + h / 2, w, 1, SOIL);
    d.draw_rectangle(x - 3, y + h + 1, w + 6, 3, CLAY);
    d.draw_rectangle(x - 3, y + h + 4, w + 6, 1, INK);
    // Night glass reflects the room a little.
    if look.darkness > 0.05 {
        d.draw_rectangle(x + 2, y + 2, 3, 8, with_alpha(BONE, 40));
    }
}

/// A round porthole (airlocks): stars and the world far below.
fn draw_porthole(d: &mut impl RaylibDraw, x: i32, seconds: f32) {
    let center = Vector2::new(x as f32, 50.0);
    d.draw_circle_v(center, 17.0, INK);
    d.draw_circle_v(center, 15.0, STEEL);
    d.draw_circle_v(center, 12.0, NIGHT);
    for index in 0..6 {
        let angle = index as f32 * 1.1 + seconds * 0.05;
        d.draw_rectangle(
            x + (angle.cos() * 8.0) as i32,
            50 + (angle.sin() * 7.0) as i32,
            1,
            1,
            BONE,
        );
    }
    for bolt in 0..8 {
        let angle = bolt as f32 * std::f32::consts::FRAC_PI_4;
        d.draw_rectangle(
            x + (angle.cos() * 14.0) as i32,
            50 + (angle.sin() * 14.0) as i32,
            1,
            1,
            STEEL_LIGHT,
        );
    }
}

/// The garage bay: a roll-up door on the left wall, tools, an oil stain and (when it is parked
/// in the bay outside) the seat's rig.
fn draw_garage(
    d: &mut impl RaylibDraw,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    plot: &Plot,
    seconds: f32,
) {
    // Roll-up door.
    d.draw_rectangle(48, FLOOR_Y - 70, 76, 70, INK);
    for y in (FLOOR_Y - 68..FLOOR_Y).step_by(6) {
        d.draw_rectangle(50, y, 72, 5, STEEL);
        d.draw_rectangle(50, y, 72, 1, STEEL_LIGHT);
    }
    d.draw_rectangle(82, FLOOR_Y - 8, 8, 2, STEEL_DARK);
    // Tool board.
    d.draw_rectangle(206, 40, 70, 40, LOAM);
    d.draw_rectangle(206, 40, 70, 1, SOIL);
    for (index, color) in [STEEL_LIGHT, RED, STEEL, AMBER, STEEL_LIGHT]
        .into_iter()
        .enumerate()
    {
        let tx = 212 + index as i32 * 13;
        d.draw_rectangle(tx, 46, 2, 18, color);
        d.draw_rectangle(tx - 2, 46, 6, 3, color);
    }
    // Oil stain.
    d.draw_ellipse(170, FLOOR_Y + 6, 26.0, 3.0, with_alpha(INK, 90));
    // The rig, if it stands in the bay outside.
    let parked = snapshot.players.iter().find(|player| {
        player.player_id == view.player_id
            && plot.garage_columns().is_some_and(|(first, last)| {
                let column = (player.x / fatherload_simulation::TILE_SIZE).floor() as i32;
                column >= first && column < last
            })
    });
    if parked.is_some() {
        let look = rig_art::RigLook::from_player(view.player_id, view.player);
        let ticks = (seconds * 60.0) as u64;
        let pose = rig_art::RigPose::idle(ticks);
        let painted = rig_art::paint_rig(&look, &pose);
        let x = 170 - rig_art::BODY_WIDTH / 2;
        painted.draw(
            d,
            Vector2::new(x as f32, (FLOOR_Y - rig_art::BODY_HEIGHT) as f32),
            1.0,
            false,
        );
        // Repair sparks.
        if (seconds * 3.0) as i32 % 2 == 0 {
            d.draw_rectangle(x + 4, FLOOR_Y - 14, 1, 1, GOLD);
            d.draw_rectangle(x + 6, FLOOR_Y - 16, 1, 1, CREAM);
        }
    }
}

/// The basement: a hatch in the floor down into the cellar and, with one, the lift cage.
fn draw_basement(d: &mut impl RaylibDraw, home: &Home, seconds: f32) {
    // Hatch grate.
    d.draw_rectangle(46, FLOOR_Y, 28, 6, INK);
    for x in (48..72).step_by(4) {
        d.draw_rectangle(x, FLOOR_Y + 1, 2, 4, STEEL_DARK);
    }
    d.draw_rectangle(46, FLOOR_Y - 1, 28, 1, STEEL);
    // A ladder rising out of it.
    d.draw_rectangle(52, FLOOR_Y - 22, 1, 22, LOAM);
    d.draw_rectangle(67, FLOOR_Y - 22, 1, 22, LOAM);
    for y in (FLOOR_Y - 20..FLOOR_Y).step_by(5) {
        d.draw_rectangle(52, y, 16, 1, CLAY);
    }
    // Pipes along the ceiling.
    d.draw_rectangle(0, CEILING_Y + 4, 320, 3, COPPER);
    d.draw_rectangle(0, CEILING_Y + 4, 320, 1, AMBER);
    if home.lift {
        // The lift cage with its call light.
        d.draw_rectangle(200, FLOOR_Y - 64, 40, 64, INK);
        d.draw_rectangle(202, FLOOR_Y - 62, 36, 62, STEEL_DARK);
        for x in (204..238).step_by(5) {
            d.draw_rectangle(x, FLOOR_Y - 60, 1, 58, STEEL);
        }
        let blink = if (seconds * 2.0) as i32 % 2 == 0 {
            LIME
        } else {
            MOSS
        };
        d.draw_rectangle(244, FLOOR_Y - 40, 4, 4, INK);
        d.draw_rectangle(245, FLOOR_Y - 39, 2, 2, blink);
        d.draw_rectangle(208, FLOOR_Y - 72, 24, 7, INK);
        d.draw_rectangle(209, FLOOR_Y - 71, 22, 5, AMBER);
    }
}

/// A staircase climbing up to the right at art `x`.
fn draw_stairs_up(d: &mut impl RaylibDraw, x: i32) {
    for step in 0..8 {
        let sx = x - 14 + step * 4;
        let sy = FLOOR_Y - (step + 1) * 8;
        d.draw_rectangle(sx, sy, 10, (step + 1) * 8, LOAM);
        d.draw_rectangle(sx, sy, 10, 1, CLAY);
        d.draw_rectangle(sx, sy, 1, (step + 1) * 8, SOIL);
    }
    d.draw_rectangle(x - 14, FLOOR_Y - 76, 1, 12, SOIL_DARK);
    d.draw_rectangle(x - 14, FLOOR_Y - 76, 40, 1, SOIL_DARK);
}

/// Steps going down through the floor at art `x`.
fn draw_stairs_down(d: &mut impl RaylibDraw, x: i32) {
    d.draw_rectangle(x - 14, FLOOR_Y, 28, 30, INK);
    for step in 0..4 {
        d.draw_rectangle(
            x - 12 + step * 3,
            FLOOR_Y + 3 + step * 6,
            22 - step * 3,
            2,
            LOAM,
        );
    }
    d.draw_rectangle(x - 16, FLOOR_Y - 20, 2, 20, SOIL);
    d.draw_rectangle(x - 16, FLOOR_Y - 20, 10, 2, SOIL);
}

/// The art rectangle of a piece's cells: `(x, width)`.
fn cell_span(cell: u8, furniture: Furniture) -> (i32, i32) {
    let column = i32::from(cell % ROOM_COLUMNS);
    (
        GRID_X + column * CELL_W,
        i32::from(furniture.width()) * CELL_W,
    )
}

/// A translucent preview of `furniture` at `cell` (the decorating spot picker).
fn draw_ghost(d: &mut impl RaylibDraw, cell: u8, furniture: Furniture, seconds: f32) {
    let (x, w) = cell_span(cell, furniture);
    let pulse = ((seconds * 4.0).sin() * 0.5 + 0.5) * 80.0;
    let (y, h) = match furniture.row() {
        DecorRow::Floor => (FLOOR_Y - 40, 40),
        DecorRow::Wall => (WALL_ROW_Y - 18, 36),
        DecorRow::Ceiling => (CEILING_Y, 30),
    };
    d.draw_rectangle(x + 1, y, w - 2, h, with_alpha(LIME, 50 + pulse as u8));
    d.draw_rectangle_lines(x + 1, y, w - 2, h, LIME);
    draw_piece(
        d,
        cell,
        PlacedDecor {
            furniture,
            display: None,
        },
        seconds,
        false,
    );
}

/// Draws one placed piece.
fn draw_piece(d: &mut impl RaylibDraw, cell: u8, piece: PlacedDecor, seconds: f32, lit: bool) {
    let (x, w) = cell_span(cell, piece.furniture);
    let mid = x + w / 2;
    let f = FLOOR_Y;
    match piece.furniture {
        Furniture::Bed => {
            d.draw_rectangle(x + 2, f - 24, 4, 24, SOIL);
            d.draw_rectangle(x + w - 6, f - 16, 4, 16, SOIL);
            d.draw_rectangle(x + 4, f - 14, w - 8, 8, BONE);
            d.draw_rectangle(x + 16, f - 15, w - 20, 9, RED);
            d.draw_rectangle(x + 16, f - 15, w - 20, 2, ROSE);
            d.draw_rectangle(x + 6, f - 18, 10, 5, CREAM);
            d.draw_rectangle(x + 4, f - 6, w - 8, 3, LOAM);
        }
        Furniture::Sofa => {
            d.draw_rectangle(x + 3, f - 22, w - 6, 10, TEAL_DARK);
            d.draw_rectangle(x + 2, f - 14, w - 4, 10, TEAL);
            d.draw_rectangle(x + 2, f - 14, w - 4, 2, MINT);
            d.draw_rectangle(x, f - 16, 5, 12, TEAL_DARK);
            d.draw_rectangle(x + w - 5, f - 16, 5, 12, TEAL_DARK);
            d.draw_rectangle(x + 4, f - 4, 2, 4, SOIL);
            d.draw_rectangle(x + w - 6, f - 4, 2, 4, SOIL);
            d.draw_rectangle(x + w / 2 - 4, f - 19, 8, 5, GOLD);
        }
        Furniture::Table => {
            d.draw_rectangle(x + 4, f - 16, w - 8, 3, CLAY);
            d.draw_rectangle(x + 4, f - 13, w - 8, 1, SOIL);
            d.draw_rectangle(x + 7, f - 13, 2, 13, LOAM);
            d.draw_rectangle(x + w - 9, f - 13, 2, 13, LOAM);
            d.draw_rectangle(mid - 3, f - 21, 6, 5, BONE);
            d.draw_rectangle(mid - 1, f - 24, 2, 3, LEAF);
        }
        Furniture::Chair => {
            d.draw_rectangle(mid - 5, f - 24, 2, 24, LOAM);
            d.draw_rectangle(mid - 5, f - 12, 11, 3, CLAY);
            d.draw_rectangle(mid + 4, f - 9, 2, 9, LOAM);
            d.draw_rectangle(mid - 5, f - 24, 4, 2, CLAY);
        }
        Furniture::Armchair => {
            d.draw_rectangle(mid - 10, f - 26, 20, 12, PLUM);
            d.draw_rectangle(mid - 12, f - 15, 24, 11, VIOLET);
            d.draw_rectangle(mid - 12, f - 15, 24, 1, LAVENDER);
            d.draw_rectangle(mid - 12, f - 18, 4, 14, PLUM);
            d.draw_rectangle(mid + 8, f - 18, 4, 14, PLUM);
            d.draw_rectangle(mid - 10, f - 4, 2, 4, SOIL);
            d.draw_rectangle(mid + 8, f - 4, 2, 4, SOIL);
        }
        Furniture::Bookshelf => {
            d.draw_rectangle(mid - 13, f - 54, 26, 54, SOIL);
            d.draw_rectangle(mid - 11, f - 52, 22, 50, SOIL_DARK);
            let colors = [RED, OCEAN, MOSS, GOLD, PLUM, TEAL, ROSE];
            for shelf in 0..4 {
                let sy = f - 50 + shelf * 12;
                d.draw_rectangle(mid - 11, sy + 10, 22, 2, LOAM);
                for book in 0..6 {
                    let color = colors[((shelf * 3 + book) % 7) as usize];
                    let height = 7 + (shelf + book) % 3;
                    d.draw_rectangle(mid - 10 + book * 3, sy + 10 - height, 2, height, color);
                }
            }
        }
        Furniture::Wardrobe => {
            d.draw_rectangle(mid - 13, f - 50, 26, 50, LOAM);
            d.draw_rectangle(mid - 13, f - 50, 26, 2, CLAY);
            d.draw_rectangle(mid, f - 46, 1, 44, SOIL);
            d.draw_rectangle(mid - 3, f - 26, 2, 4, GOLD);
            d.draw_rectangle(mid + 2, f - 26, 2, 4, GOLD);
            d.draw_rectangle(mid - 13, f - 2, 26, 2, SOIL);
        }
        Furniture::Stove => {
            d.draw_rectangle(mid - 12, f - 26, 24, 26, STEEL_DARK);
            d.draw_rectangle(mid - 12, f - 26, 24, 3, STEEL);
            d.draw_rectangle(mid - 8, f - 18, 16, 12, INK);
            let flame = if (seconds * 6.0) as i32 % 2 == 0 {
                ORANGE
            } else {
                AMBER
            };
            d.draw_rectangle(mid - 5, f - 12, 10, 5, flame);
            d.draw_rectangle(mid - 3, f - 14, 6, 3, GOLD);
            d.draw_rectangle(mid + 4, f - 54, 4, 28, STEEL_DARK);
            d.draw_rectangle(mid - 8, f - 31, 8, 5, STEEL);
            // Steam from the pot.
            let rise = (seconds * 10.0) as i32 % 10;
            d.draw_rectangle(mid - 5, f - 34 - rise, 1, 2, with_alpha(BONE, 150));
        }
        Furniture::Fireplace => {
            d.draw_rectangle(x + 4, f - 40, w - 8, 40, STONE);
            d.draw_rectangle(x + 2, f - 42, w - 4, 4, STONE_LIGHT);
            d.draw_rectangle(x + 14, f - 26, w - 28, 26, INK);
            let flicker = (seconds * 7.0).sin();
            let flame = if flicker > 0.0 { ORANGE } else { AMBER };
            d.draw_rectangle(x + 20, f - 12, w - 40, 10, flame);
            d.draw_rectangle(x + 24, f - 18 - (flicker * 2.0) as i32, w - 48, 8, GOLD);
            d.draw_rectangle(x + 16, f - 3, w - 32, 3, SOIL);
            for (index, color) in [BONE, GOLD, OCEAN].into_iter().enumerate() {
                d.draw_rectangle(x + 10 + index as i32 * 14, f - 47, 4, 5, color);
            }
            if lit {
                d.draw_rectangle(x - 6, f - 30, w + 12, 30, with_alpha(ORANGE, 25));
            }
        }
        Furniture::Workbench => {
            d.draw_rectangle(x + 2, f - 22, w - 4, 4, CLAY);
            d.draw_rectangle(x + 4, f - 18, 3, 18, LOAM);
            d.draw_rectangle(x + w - 7, f - 18, 3, 18, LOAM);
            d.draw_rectangle(x + 4, f - 8, w - 8, 2, LOAM);
            d.draw_rectangle(x + 10, f - 28, 10, 6, STEEL);
            d.draw_rectangle(x + 30, f - 26, 2, 4, RED);
            d.draw_rectangle(x + 40, f - 27, 12, 2, STEEL_LIGHT);
        }
        Furniture::Chest => {
            d.draw_rectangle(mid - 12, f - 16, 24, 16, LOAM);
            d.draw_rectangle(mid - 12, f - 18, 24, 5, CLAY);
            d.draw_rectangle(mid - 12, f - 13, 24, 1, SOIL);
            d.draw_rectangle(mid - 12, f - 16, 2, 16, STEEL_DARK);
            d.draw_rectangle(mid + 10, f - 16, 2, 16, STEEL_DARK);
            d.draw_rectangle(mid - 2, f - 13, 4, 4, GOLD);
        }
        Furniture::TrophyCase => {
            d.draw_rectangle(mid - 12, f - 12, 24, 12, SOIL);
            d.draw_rectangle(mid - 12, f - 12, 24, 2, CLAY);
            d.draw_rectangle(mid - 10, f - 44, 20, 32, with_alpha(ICE, 70));
            d.draw_rectangle_lines(mid - 10, f - 44, 20, 32, STEEL_LIGHT);
            d.draw_rectangle(mid - 11, f - 46, 22, 3, SOIL);
            match piece.display {
                Some(item) => {
                    let sprite = match item {
                        ItemKind::Artifact(artifact) => terrain_art::artifact_stamp(artifact),
                        ItemKind::Mineral(mineral) => icons::mineral(mineral),
                        _ => &icons::CARGO,
                    };
                    let bob = ((seconds * 1.5).sin() * 1.5) as i32;
                    let sx = mid - sprite.width() / 2;
                    let sy = f - 30 - sprite.height() / 2 + bob;
                    sprite::draw_sprite(
                        d,
                        sprite,
                        0,
                        Vector2::new(sx as f32, sy as f32),
                        1.0,
                        Color::WHITE,
                    );
                    // A glint and a little spotlight.
                    let glint = ((seconds * 2.0).sin() * 0.5 + 0.5) * 200.0;
                    d.draw_rectangle(mid + 4, f - 38, 1, 3, with_alpha(BONE, glint as u8));
                    d.draw_rectangle(mid - 9, f - 43, 18, 2, with_alpha(CREAM, 60));
                }
                None => {
                    d.draw_rectangle(mid - 4, f - 18, 8, 6, with_alpha(SOIL, 160));
                }
            }
        }
        Furniture::Plant => {
            d.draw_rectangle(mid - 5, f - 10, 10, 10, CLAY);
            d.draw_rectangle(mid - 6, f - 11, 12, 2, RUST);
            let sway = ((seconds * 1.2 + f32::from(cell)).sin() * 1.5) as i32;
            for (dx, h) in [(-5, 14), (-2, 20), (1, 17), (4, 13), (-7, 9), (6, 10)] {
                d.draw_rectangle(mid + dx + sway * (h / 10), f - 10 - h, 2, h, MOSS);
                d.draw_rectangle(mid + dx + sway * (h / 10), f - 10 - h, 2, 3, LEAF);
            }
        }
        Furniture::Planter => {
            d.draw_rectangle(x + 3, f - 9, w - 6, 9, LOAM);
            d.draw_rectangle(x + 3, f - 9, w - 6, 2, CLAY);
            for index in 0..4 {
                let px = x + 6 + index * 6;
                d.draw_rectangle(px, f - 18, 2, 9, LEAF);
                d.draw_rectangle(
                    px - 1,
                    f - 19,
                    4,
                    3,
                    if index % 2 == 0 { RED } else { GOLD },
                );
            }
        }
        Furniture::Rug => {
            d.draw_rectangle(x + 2, f + 2, w - 4, 6, MAROON);
            d.draw_rectangle(x + 4, f + 3, w - 8, 4, RED);
            for tx in (x + 6..x + w - 6).step_by(6) {
                d.draw_rectangle(tx, f + 4, 2, 2, GOLD);
            }
            d.draw_rectangle(x + 1, f + 2, 1, 6, BONE);
            d.draw_rectangle(x + w - 2, f + 2, 1, 6, BONE);
        }
        Furniture::Telescope => {
            d.draw_rectangle(mid - 1, f - 22, 2, 22, SOIL);
            d.draw_rectangle(mid - 8, f - 2, 16, 2, SOIL);
            d.draw_rectangle(mid - 10, f - 34, 22, 6, GOLD);
            d.draw_rectangle(mid - 10, f - 34, 22, 1, CREAM);
            d.draw_rectangle(mid + 10, f - 36, 4, 10, AMBER);
            d.draw_rectangle(mid - 12, f - 33, 3, 4, COPPER);
        }
        Furniture::Piano => {
            d.draw_rectangle(x + 4, f - 34, w - 8, 34, INK);
            d.draw_rectangle(x + 5, f - 33, w - 10, 32, SOIL_DARK);
            d.draw_rectangle(x + 6, f - 20, w - 12, 4, BONE);
            for key in (x + 8..x + w - 8).step_by(4) {
                d.draw_rectangle(key, f - 20, 2, 2, INK);
            }
            d.draw_rectangle(x + 8, f - 4, 3, 4, INK);
            d.draw_rectangle(x + w - 11, f - 4, 3, 4, INK);
            d.draw_rectangle(x + w / 2 - 6, f - 30, 12, 7, CREAM);
            if (seconds * 0.7).sin() > 0.6 {
                let rise = (seconds * 12.0) as i32 % 12;
                d.draw_rectangle(x + w / 2, f - 40 - rise, 2, 3, GOLD);
            }
        }
        Furniture::Painting => {
            let y = WALL_ROW_Y - 12;
            d.draw_rectangle(mid - 12, y, 24, 20, GOLD);
            d.draw_rectangle(mid - 10, y + 2, 20, 16, SKY_LIGHT);
            d.draw_rectangle(mid - 10, y + 11, 20, 7, MOSS);
            d.draw_rectangle(mid - 6, y + 8, 8, 4, LEAF);
            d.draw_rectangle(mid + 3, y + 4, 3, 3, CREAM);
            d.draw_rectangle(mid - 12, y + 20, 24, 1, with_alpha(INK, 120));
        }
        Furniture::Shelf => {
            let y = WALL_ROW_Y;
            d.draw_rectangle(mid - 13, y, 26, 3, CLAY);
            d.draw_rectangle(mid - 11, y + 3, 2, 4, SOIL);
            d.draw_rectangle(mid + 9, y + 3, 2, 4, SOIL);
            d.draw_rectangle(mid - 10, y - 8, 5, 8, OCEAN);
            d.draw_rectangle(mid - 3, y - 6, 6, 6, BONE);
            d.draw_rectangle(mid + 5, y - 10, 4, 10, MOSS);
        }
        Furniture::WallLamp => {
            let y = WALL_ROW_Y - 12;
            d.draw_rectangle(mid - 1, y + 6, 2, 8, STEEL_DARK);
            d.draw_rectangle(mid - 5, y, 10, 7, AMBER);
            d.draw_rectangle(mid - 4, y + 1, 8, 5, GOLD);
            d.draw_circle(mid, y + 4, 14.0, with_alpha(CREAM, 30));
        }
        Furniture::Clock => {
            let y = WALL_ROW_Y - 14;
            d.draw_rectangle(mid - 6, y, 12, 30, SOIL);
            d.draw_circle(mid, y + 7, 5.0, BONE);
            let hand = seconds * 0.5;
            d.draw_line(
                mid,
                y + 7,
                mid + (hand.cos() * 4.0) as i32,
                y + 7 + (hand.sin() * 4.0) as i32,
                INK,
            );
            let swing = ((seconds * 3.0).sin() * 3.0) as i32;
            d.draw_rectangle(mid + swing - 1, y + 16, 2, 9, GOLD);
            d.draw_rectangle(mid + swing - 2, y + 24, 4, 3, GOLD);
        }
        Furniture::Mirror => {
            let y = WALL_ROW_Y - 18;
            d.draw_ellipse(mid, y + 14, 9.0, 14.0, GOLD);
            d.draw_ellipse(mid, y + 14, 7.0, 12.0, ICE);
            d.draw_rectangle(mid - 3, y + 6, 2, 10, with_alpha(BONE, 160));
        }
        Furniture::Banner => {
            let y = WALL_ROW_Y - 20;
            d.draw_rectangle(x + 6, y, w - 12, 2, SOIL);
            d.draw_rectangle(x + 10, y + 2, w - 20, 30, RED);
            d.draw_rectangle(x + 10, y + 2, w - 20, 3, MAROON);
            d.draw_rectangle(x + w / 2 - 5, y + 12, 10, 10, GOLD);
            d.draw_triangle(
                Vector2::new((x + 10) as f32, (y + 32) as f32),
                Vector2::new((x + w / 2) as f32, (y + 26) as f32),
                Vector2::new((x + w - 10) as f32, (y + 32) as f32),
                with_alpha(INK, 0),
            );
        }
        Furniture::MountedPick => {
            let y = WALL_ROW_Y - 14;
            d.draw_rectangle(mid - 10, y + 8, 20, 12, SOIL);
            d.draw_rectangle(mid - 1, y - 2, 2, 24, LOAM);
            d.draw_rectangle(mid - 9, y - 4, 18, 4, STEEL);
            d.draw_rectangle(mid - 9, y - 4, 18, 1, STEEL_LIGHT);
        }
        Furniture::Chandelier => {
            d.draw_rectangle(mid - 1, CEILING_Y, 2, 10, GOLD);
            d.draw_rectangle(mid - 14, CEILING_Y + 10, 28, 3, GOLD);
            for (index, dx) in [-13, -6, 0, 6, 12].into_iter().enumerate() {
                d.draw_rectangle(mid + dx, CEILING_Y + 6, 2, 4, CREAM);
                let flicker = ((seconds * 5.0 + index as f32).sin() * 0.5 + 0.5) > 0.3;
                d.draw_rectangle(
                    mid + dx,
                    CEILING_Y + 4,
                    2,
                    2,
                    if flicker { GOLD } else { AMBER },
                );
            }
            d.draw_circle(mid, CEILING_Y + 12, 30.0, with_alpha(CREAM, 22));
        }
        Furniture::Lantern => {
            d.draw_rectangle(mid, CEILING_Y, 1, 12, INK);
            d.draw_rectangle(mid - 4, CEILING_Y + 12, 9, 11, STEEL_DARK);
            d.draw_rectangle(mid - 3, CEILING_Y + 14, 7, 7, AMBER);
            d.draw_rectangle(mid - 1, CEILING_Y + 16, 3, 3, GOLD);
            d.draw_circle(mid, CEILING_Y + 18, 16.0, with_alpha(CREAM, 26));
        }
        Furniture::HangingPlant => {
            d.draw_rectangle(mid, CEILING_Y, 1, 8, SOIL);
            d.draw_rectangle(mid - 5, CEILING_Y + 8, 11, 6, CLAY);
            for (dx, h) in [(-5, 12), (-1, 18), (3, 10), (5, 15)] {
                d.draw_rectangle(mid + dx, CEILING_Y + 12, 1, h, MOSS);
                d.draw_rectangle(mid + dx - 1, CEILING_Y + 12 + h, 3, 2, LEAF);
            }
        }
        Furniture::StarMobile => {
            d.draw_rectangle(mid, CEILING_Y, 1, 8, STEEL);
            d.draw_rectangle(mid - 12, CEILING_Y + 8, 24, 1, STEEL);
            for (index, dx) in [-12, -4, 4, 12].into_iter().enumerate() {
                let spin = (seconds * 0.8 + index as f32).sin();
                let drop = 6 + index as i32 * 3;
                d.draw_rectangle(mid + dx, CEILING_Y + 8, 1, drop, STEEL);
                let size = if spin > 0.0 { 3 } else { 2 };
                d.draw_rectangle(
                    mid + dx - 1,
                    CEILING_Y + 8 + drop,
                    size,
                    size,
                    if index % 2 == 0 { GOLD } else { ICE },
                );
            }
        }
    }
}

/// Warm light pooling under lamps (and a cool wash from the windows by day).
fn draw_room_light(d: &mut impl RaylibDraw, room: &Room, look: &DayLook, seconds: f32) {
    if room.kind.has_windows() && look.darkness < 0.3 {
        for x in window_columns(room) {
            d.draw_triangle(
                Vector2::new(x as f32, 56.0),
                Vector2::new((x - 18) as f32, FLOOR_Y as f32),
                Vector2::new((x + 48) as f32, FLOOR_Y as f32),
                with_alpha(CREAM, 14),
            );
        }
    }
    if look.darkness > 0.05 {
        let flicker = (seconds * 3.0).sin() * 2.0;
        for (cell, piece) in &room.decor.pieces {
            if !piece.furniture.lights() {
                continue;
            }
            let (x, w) = cell_span(*cell, piece.furniture);
            let mid = (x + w / 2) as f32;
            let top = match piece.furniture.row() {
                DecorRow::Ceiling => (CEILING_Y + 14) as f32,
                DecorRow::Wall => (WALL_ROW_Y - 6) as f32,
                DecorRow::Floor => (FLOOR_Y - 20) as f32,
            };
            d.draw_triangle(
                Vector2::new(mid, top),
                Vector2::new(mid - 40.0 - flicker, FLOOR_Y as f32),
                Vector2::new(mid + 40.0 + flicker, FLOOR_Y as f32),
                with_alpha(GOLD, 24),
            );
        }
    }
}

/// The pilot standing (or walking) on the floor at room units `x`.
fn draw_pilot(
    d: &mut impl RaylibDraw,
    view: &ViewSnapshot<'_>,
    x: f32,
    facing: f32,
    stride: f32,
    seconds: f32,
) {
    use fatherload_simulation::pilot::PilotPose;
    let Some(pilot) = &view.pilot else {
        return;
    };
    let look = crate::rendering::pilot_art::PilotLook::new(pilot.cosmetics, pilot.state.gear);
    let walking = stride > 0.0;
    let frame = crate::rendering::pilot_art::PilotFrame {
        pose: if walking {
            PilotPose::Walk
        } else {
            PilotPose::Idle
        },
        seconds: if walking { stride } else { seconds },
        swinging: false,
        arms: crate::rendering::pilot_art::PilotArms::NONE,
    };
    let px = room_pixel_x(x);
    d.draw_rectangle(px - 5, FLOOR_Y, 10, 1, with_alpha(INK, 120));
    crate::rendering::pilot_art::paint_pilot(&look, frame).draw(
        d,
        Vector2::new(px as f32, FLOOR_Y as f32),
        1.0,
        facing < 0.0,
        None,
    );
}
