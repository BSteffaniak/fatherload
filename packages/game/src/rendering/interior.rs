//! Walk-in building interiors, drawn as pixel-art rooms.
//!
//! A room is a fixed [`ROOM_WIDTH`] x [`ROOM_HEIGHT`] art-pixel canvas shown at the largest whole
//! pixel size that fits the window (see [`crate::pixel_scale::fit_pixel_size`]) and centred; the
//! walls and floor continue past its edges on wider screens. The rig walks along the floor in the
//! shell's room units (`interior_x`, 0..640), which map to art pixels at half scale, so the door,
//! counter and walking rules stay exactly those of [`crate::shell`].
//!
//! Every building shares the room structure (ceiling, wall, wainscot, floor, exit door, sign,
//! lamps, counter and keeper) in its own wall pattern and colours; [`props`] dresses each room
//! with its own furniture, and the Upgrade Shop keeps its parts shelves and tool board here.

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{
        palette,
        sprite::{self, SpriteDef},
        sprites::{
            interior::{self as art, InteriorSet},
            rig,
        },
    },
    prompts::{self, Hint, hint},
    ui_kit::{self, PanelStyle, TextStyle, UiCanvasScope},
};
use drillgame_simulation::economy::SurfaceZone;

mod props;

use crate::{
    input::prompts::Prompt,
    pixel_scale,
    render_snapshot::{MarketQuote, ViewSnapshot, Viewport},
    shell::{SeatUi, ServiceAnimation, interior_service_x, surface_zone_label},
};

/// Room canvas size in art pixels.
pub const ROOM_WIDTH: i32 = 320;
pub const ROOM_HEIGHT: i32 = 180;
/// Bottom of the ceiling band.
const CEILING_Y: i32 = 16;
/// Top of the wainscot (lower wall).
const WAINSCOT_Y: i32 = 104;
/// Top of the floor; the rig and counter stand on it.
const FLOOR_Y: i32 = 140;
/// Shell room units per art pixel.
const ROOM_UNITS_PER_PIXEL: f32 = 2.0;
/// How close (room units) the rig must be to the counter for `E` to work (mirrors the shell).
const COUNTER_REACH: f32 = 70.0;
/// Room units left of which `E` uses the door (mirrors the shell).
const DOOR_REACH: f32 = 74.0;
/// Counter width in art pixels.
const COUNTER_WIDTH: i32 = 76;
const COUNTER_HEIGHT: i32 = 24;
/// Leftmost room x for the sign board. Counter dialogs dock on the left and end around room
/// x 169 at 16:9 sizes, so a long sign shifts right instead of disappearing under the dialog.
const SIGN_MIN_X: i32 = 174;

/// The part of the framebuffer a room is drawn into: the whole window for one player, or a
/// split-screen seat's view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RoomArea {
    /// Framebuffer rect in physical pixels.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// The seat's view (logical points) in split-screen; `None` fills the window.
    pub viewport: Option<Viewport>,
    /// Lift the room to the top of the area (a counter panel docked along the bottom of a tall
    /// split-screen view would otherwise cover the floor).
    pub align_top: bool,
}

impl RoomArea {
    /// The whole `render_width` x `render_height` framebuffer.
    pub(super) const fn window(render_width: i32, render_height: i32) -> Self {
        Self {
            x: 0,
            y: 0,
            width: render_width,
            height: render_height,
            viewport: None,
            align_top: false,
        }
    }
}

/// Where the room canvas sits in the framebuffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoomPlacement {
    /// Physical pixels per room art pixel.
    pixel: i32,
    /// Framebuffer position of the area the room is drawn into.
    origin_x: i32,
    origin_y: i32,
    /// Position of the room's top-left corner inside that area.
    offset_x: i32,
    offset_y: i32,
    render_width: i32,
    render_height: i32,
}

impl RoomPlacement {
    #[cfg(test)]
    fn fit(render_width: i32, render_height: i32) -> Self {
        Self::fit_area(RoomArea::window(render_width, render_height))
    }

    fn fit_area(area: RoomArea) -> Self {
        let pixel =
            pixel_scale::fit_pixel_size((area.width, area.height), (ROOM_WIDTH, ROOM_HEIGHT));
        let centred_y = (area.height - ROOM_HEIGHT * pixel) / 2;
        Self {
            pixel,
            origin_x: area.x,
            origin_y: area.y,
            offset_x: (area.width - ROOM_WIDTH * pixel) / 2,
            offset_y: if area.align_top {
                centred_y.min(0)
            } else {
                centred_y
            },
            render_width: area.width,
            render_height: area.height,
        }
    }

    fn camera(self) -> Camera2D {
        Camera2D {
            offset: Vector2::new(
                (self.origin_x + self.offset_x) as f32,
                (self.origin_y + self.offset_y) as f32,
            ),
            target: Vector2::zero(),
            rotation: 0.0,
            zoom: self.pixel as f32,
        }
    }

    /// Room x range visible on screen (wider than the room on wide windows).
    const fn visible_x(self) -> (i32, i32) {
        let left = -(self.offset_x + self.pixel - 1) / self.pixel;
        let right = (self.render_width - self.offset_x + self.pixel - 1) / self.pixel;
        (left, right)
    }

    /// Room y range visible on screen.
    const fn visible_y(self) -> (i32, i32) {
        let top = -(self.offset_y + self.pixel - 1) / self.pixel;
        let bottom = (self.render_height - self.offset_y + self.pixel - 1) / self.pixel;
        (top, bottom)
    }

    /// A room point in UI canvas art pixels.
    const fn to_canvas(self, canvas: ui_kit::UiCanvas, x: i32, y: i32) -> (i32, i32) {
        (
            (self.origin_x - canvas.origin_x + self.offset_x + x * self.pixel) / canvas.pixel,
            (self.origin_y - canvas.origin_y + self.offset_y + y * self.pixel) / canvas.pixel,
        )
    }
}

/// Room art x of a shell room-unit position.
fn room_pixel_x(room_units: f32) -> i32 {
    (room_units / ROOM_UNITS_PER_PIXEL).round() as i32
}

const fn interior_set(zone: SurfaceZone) -> &'static InteriorSet {
    match zone {
        SurfaceZone::Fuel => &art::fuel::SET,
        SurfaceZone::Repair => &art::repair::SET,
        SurfaceZone::Depot => &art::depot::SET,
        SurfaceZone::Headquarters => &art::hq::SET,
        SurfaceZone::Shop => &art::shop::SET,
        SurfaceZone::Bank => &art::bank::SET,
        SurfaceZone::Explosives => &art::explosives::SET,
        SurfaceZone::Salvage => &art::salvage::SET,
    }
}

const fn sign_text(zone: SurfaceZone) -> &'static str {
    match zone {
        SurfaceZone::Fuel => "FUEL",
        SurfaceZone::Repair => "REPAIRS",
        SurfaceZone::Depot => "ORE DEPOT",
        SurfaceZone::Headquarters => "BOREALIS HQ",
        SurfaceZone::Shop => "UPGRADES",
        SurfaceZone::Bank => "IRON LEDGER",
        SurfaceZone::Explosives => "BOOM!",
        SurfaceZone::Salvage => "SALVAGE",
    }
}

/// Draws `seat`'s building interior into `area`, then (on the current UI canvas, which covers
/// the same area) its name plate, controls, keeper's line and door/counter prompts. In
/// split-screen the plate names the seat's player (`player`).
pub(super) fn draw_interior(
    draw: &mut RaylibDrawHandle<'_>,
    seat: &SeatUi,
    view: &ViewSnapshot<'_>,
    market: &[MarketQuote],
    area: RoomArea,
    player: Option<usize>,
) {
    let zone = seat.interior_zone.unwrap_or(SurfaceZone::Depot);
    let room_set = interior_set(zone);
    let placement = RoomPlacement::fit_area(area);
    let seconds = ui_kit::seconds();
    match area.viewport {
        None => draw.clear_background(room_set.shadow),
        Some(viewport) => draw.draw_rectangle(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            room_set.shadow,
        ),
    }
    let counter_x = room_pixel_x(interior_service_x(zone)) - COUNTER_WIDTH / 2;
    {
        let mut room = draw.begin_mode2D(placement.camera());
        draw_room_shell(&mut room, room_set, placement);
        draw_door(&mut room);
        let context = props::RoomContext {
            zone,
            counter_x,
            seconds,
            market,
        };
        if zone == SurfaceZone::Shop {
            draw_shop_props(&mut room, counter_x, seconds);
        } else {
            props::draw_room_props(&mut room, &context);
        }
        draw_lamps(&mut room, zone, counter_x);
        draw_sign(&mut room, zone, room_set, counter_x);
        draw_counter(&mut room, room_set, counter_x, seconds);
        props::draw_counter_props(&mut room, &context);
        draw_service_animation(&mut room, seat, zone, counter_x);
        draw_rig(&mut room, seat, seconds);
    }

    let _canvas = UiCanvasScope::begin(draw);
    let canvas = ui_kit::canvas();
    if seat.modal.is_some() {
        return;
    }
    draw_name_plate(draw, surface_zone_label(zone), player);
    draw_controls_hint(draw, canvas);
    let near_counter = (seat.interior_x - interior_service_x(zone)).abs() <= COUNTER_REACH;
    let at_door = seat.interior_x < DOOR_REACH;
    let keeper_x = counter_x + COUNTER_WIDTH / 2;
    let (bubble_x, bubble_y) = placement.to_canvas(canvas, keeper_x, FLOOR_Y - COUNTER_HEIGHT - 17);
    ui_kit::tooltip(
        draw,
        npc_line(zone, view),
        bubble_x,
        bubble_y,
        180,
        canvas.rect(),
    );
    if near_counter || at_door {
        let target_x = if at_door { 28 } else { keeper_x };
        let (prompt_x, prompt_y) = placement.to_canvas(canvas, target_x, FLOOR_Y - 4);
        let label = if at_door { "Leave" } else { "Use counter" };
        draw_key_prompt(draw, prompt_x, prompt_y, hint(Prompt::Use, label));
    }
}

/// The building's name plate (top-left), led by the player's tag in split-screen.
fn draw_name_plate(draw: &mut RaylibDrawHandle<'_>, title: &str, player: Option<usize>) {
    let tag = player.map(|seat| format!("P{}", seat + 1));
    let tag_width = tag
        .as_deref()
        .map_or(0, |tag| ui_kit::text_width(tag, TextStyle::Bold) + 7);
    let plate_width = tag_width + ui_kit::text_width(title, TextStyle::Bold) + 16;
    ui_kit::panel(
        draw,
        Rectangle::new(8.0, 8.0, plate_width as f32, 19.0),
        PanelStyle::Accent,
    );
    if let Some(tag) = &tag {
        ui_kit::text(draw, tag, 16, 13, TextStyle::Bold, BONE);
        draw.draw_rectangle(16 + tag_width - 5, 12, 1, 11, palette::with_alpha(BONE, 90));
    }
    ui_kit::text(draw, title, 16 + tag_width, 13, TextStyle::Bold, GOLD);
}

/// A small "button + action" prompt centred on `x`, bobbing above `y`.
fn draw_key_prompt(draw: &mut RaylibDrawHandle<'_>, x: i32, y: i32, action: Hint) {
    let device = prompts::menu_device();
    let Some(content) = prompts::prompt_width(action, device) else {
        return;
    };
    let bob = (ui_kit::seconds() * 2.5) as i32 % 2;
    let width = content + 8;
    let left = x - width / 2;
    let top = y - 17 - bob;
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, 17.0),
        PanelStyle::Accent,
    );
    prompts::draw_prompt(draw, left + 4, top + 3, action, device, BONE);
}

/// The room's controls, bottom-right.
fn draw_controls_hint(draw: &mut RaylibDrawHandle<'_>, canvas: ui_kit::UiCanvas) {
    const HINTS: [Hint; 3] = [
        hint(Prompt::Walk, "walk"),
        hint(Prompt::Use, "use"),
        hint(Prompt::Back, "leave"),
    ];
    let device = prompts::menu_device();
    let width = prompts::bar_width(&HINTS, device) + 12;
    let x = canvas.width - width - 8;
    let y = canvas.height - 23;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, 17.0),
        PanelStyle::Standard,
    );
    prompts::draw_bar(draw, x + 6, y + 3, &HINTS, device, MIST);
}

/// Ceiling, brick wall, wainscot and floor across everything visible.
fn draw_room_shell(draw: &mut impl RaylibDraw, set: &InteriorSet, placement: RoomPlacement) {
    let (left, right) = placement.visible_x();
    let (top, bottom) = placement.visible_y();
    // Tile from a 16-pixel grid line left of the screen so tiles line up with the room.
    let tile_left = left.div_euclid(16) * 16;
    let width = right - tile_left;
    draw.draw_rectangle(tile_left, top, width, CEILING_Y - top, set.shadow);
    // Ceiling beams.
    for beam_x in (tile_left..right).step_by(48) {
        draw.draw_rectangle(beam_x, top, 6, CEILING_Y - top, INK);
    }
    draw.draw_rectangle(tile_left, CEILING_Y - 2, width, 2, INK);
    sprite::draw_tiled(
        draw,
        set.wall,
        0,
        Rectangle::new(
            tile_left as f32,
            CEILING_Y as f32,
            width as f32,
            (WAINSCOT_Y - CEILING_Y) as f32,
        ),
        1.0,
        Color::WHITE,
    );
    // Shadow under the ceiling.
    draw.draw_rectangle(
        tile_left,
        CEILING_Y,
        width,
        3,
        palette::with_alpha(INK, 110),
    );
    sprite::draw_tiled(
        draw,
        set.plank,
        0,
        Rectangle::new(
            tile_left as f32,
            WAINSCOT_Y as f32,
            width as f32,
            (FLOOR_Y - WAINSCOT_Y) as f32,
        ),
        1.0,
        Color::WHITE,
    );
    sprite::draw_tiled(
        draw,
        set.floor,
        0,
        Rectangle::new(
            tile_left as f32,
            FLOOR_Y as f32,
            width as f32,
            (bottom - FLOOR_Y).max(0) as f32,
        ),
        1.0,
        Color::WHITE,
    );
    draw.draw_rectangle(tile_left, FLOOR_Y, width, 1, INK);
    // Fade the floor towards the bottom of the screen.
    for (index, alpha) in [40_u8, 80, 120].into_iter().enumerate() {
        let band = i32::try_from(index).unwrap_or(0);
        draw.draw_rectangle(
            tile_left,
            FLOOR_Y + 16 + band * 8,
            width,
            8,
            palette::with_alpha(INK, alpha),
        );
    }
    draw.draw_rectangle(
        tile_left,
        FLOOR_Y + 40,
        width,
        (bottom - FLOOR_Y - 40).max(0),
        palette::with_alpha(INK, 160),
    );
}

/// The exit door on the left wall with its sign.
fn draw_door(draw: &mut impl RaylibDraw) {
    let (x, y, width, height) = (14, FLOOR_Y - 46, 28, 46);
    sprite::draw_nine_slice(
        draw,
        &art::DOOR_FRAME,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        1.0,
        Color::WHITE,
    );
    // Door panel, window and handle.
    draw.draw_rectangle(x + 3, y + 3, width - 6, height - 3, LOAM);
    draw.draw_rectangle(x + 3, y + 3, 1, height - 3, CLAY);
    draw.draw_rectangle(x + width - 4, y + 3, 1, height - 3, SOIL);
    draw.draw_rectangle(x + 7, y + 8, width - 14, 12, INK);
    draw.draw_rectangle(x + 8, y + 9, width - 16, 10, SKY);
    draw.draw_rectangle(x + 8, y + 9, 3, 10, SKY_LIGHT);
    draw.draw_rectangle(x + 8, y + 13, width - 16, 1, INK);
    draw.draw_rectangle(x + width - 9, y + 26, 3, 3, INK);
    draw.draw_rectangle(x + width - 9, y + 26, 2, 2, GOLD);
    let sign = &art::EXIT_SIGN;
    ui_icon(
        draw,
        sign,
        x + (width - sign.width()) / 2,
        y - sign.height() - 3,
    );
}

/// Hanging lamps with soft light cones.
fn draw_lamps(draw: &mut impl RaylibDraw, zone: SurfaceZone, counter_x: i32) {
    let lamp = &art::LAMP;
    let mut positions = vec![counter_x + COUNTER_WIDTH / 2 - lamp.width() / 2];
    // A second lamp over the showroom where its ceiling is free.
    match zone {
        SurfaceZone::Shop | SurfaceZone::Depot | SurfaceZone::Headquarters => positions.push(96),
        SurfaceZone::Fuel => positions.push(128),
        SurfaceZone::Explosives => positions.push(106),
        SurfaceZone::Repair | SurfaceZone::Bank | SurfaceZone::Salvage => {}
    }
    for x in positions {
        // Cord from the ceiling.
        draw.draw_rectangle(x + 3, CEILING_Y, 1, 8, INK);
        ui_icon(draw, lamp, x, CEILING_Y + 6);
        let tip = Vector2::new(x as f32 + 3.5, (CEILING_Y + 15) as f32);
        draw.draw_triangle(
            tip,
            Vector2::new(tip.x - 34.0, FLOOR_Y as f32),
            Vector2::new(tip.x + 34.0, FLOOR_Y as f32),
            palette::with_alpha(CREAM, 22),
        );
        draw.draw_triangle(
            tip,
            Vector2::new(tip.x - 18.0, FLOOR_Y as f32),
            Vector2::new(tip.x + 18.0, FLOOR_Y as f32),
            palette::with_alpha(CREAM, 18),
        );
    }
}

/// Draws `sprite` frame 0 at room pixel `(x, y)`.
fn ui_icon(draw: &mut impl RaylibDraw, sprite: &SpriteDef, x: i32, y: i32) {
    room_sprite(draw, sprite, 0, x, y, false);
}

fn room_sprite(
    draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    x: i32,
    y: i32,
    flip: bool,
) {
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        frame,
        Vector2::new(x as f32, y as f32),
        1.0,
        flip,
        Color::WHITE,
    );
}

/// The building's sign board on the wall above the counter.
fn draw_sign(draw: &mut impl RaylibDraw, zone: SurfaceZone, set: &InteriorSet, counter_x: i32) {
    let label = sign_text(zone);
    let width = ui_kit::text_width(label, TextStyle::Bold) + 14;
    let x = (counter_x + COUNTER_WIDTH / 2 - width / 2)
        .max(SIGN_MIN_X)
        .min(ROOM_WIDTH - width - 4);
    let y = CEILING_Y + 32;
    // Hanging chains.
    draw.draw_rectangle(x + 4, CEILING_Y, 1, y - CEILING_Y, INK);
    draw.draw_rectangle(x + width - 5, CEILING_Y, 1, y - CEILING_Y, INK);
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, 17.0),
        PanelStyle::Standard,
    );
    ui_kit::text(draw, label, x + 7, y + 4, TextStyle::Bold, set.accent);
}

/// The counter with the keeper standing behind it.
fn draw_counter(draw: &mut impl RaylibDraw, set: &InteriorSet, counter_x: i32, seconds: f32) {
    let keeper = set.keeper;
    let keeper_x = counter_x + COUNTER_WIDTH / 2 - keeper.width() / 2;
    let keeper_y = FLOOR_Y - COUNTER_HEIGHT - keeper.height() + 4;
    room_sprite(
        draw,
        keeper,
        keeper.frame_at(seconds),
        keeper_x,
        keeper_y,
        false,
    );
    // Shadow on the wall behind the counter.
    draw.draw_rectangle(
        counter_x + 2,
        FLOOR_Y - COUNTER_HEIGHT - 2,
        COUNTER_WIDTH,
        2,
        palette::with_alpha(INK, 90),
    );
    sprite::draw_nine_slice(
        draw,
        set.counter,
        Rectangle::new(
            counter_x as f32,
            (FLOOR_Y - COUNTER_HEIGHT) as f32,
            COUNTER_WIDTH as f32,
            COUNTER_HEIGHT as f32,
        ),
        1.0,
        Color::WHITE,
    );
    // Floor shadow in front of the counter.
    draw.draw_rectangle(
        counter_x + 2,
        FLOOR_Y,
        COUNTER_WIDTH - 2,
        2,
        palette::with_alpha(INK, 100),
    );
}

/// The Upgrade Shop's dressing: parts shelves, a tool board, crates and the cash register.
fn draw_shop_props(draw: &mut impl RaylibDraw, counter_x: i32, seconds: f32) {
    // Parts shelves on the left wall.
    let shelf_x = 54;
    for (row, shelf_y) in [58, 80, 102].into_iter().enumerate() {
        draw.draw_rectangle(shelf_x, shelf_y, 84, 3, LOAM);
        draw.draw_rectangle(shelf_x, shelf_y, 84, 1, SAND);
        draw.draw_rectangle(shelf_x, shelf_y + 3, 84, 1, INK);
        for bracket in [shelf_x + 4, shelf_x + 76] {
            draw.draw_rectangle(bracket, shelf_y + 3, 2, 4, INK);
        }
        let items: [(&SpriteDef, i32); 4] = match row {
            0 => [
                (&art::PART_BOX, 2),
                (&art::GEAR, 18),
                (&art::GEAR, 27),
                (&art::PART_BOX, 40),
            ],
            1 => [
                (&art::CAN, 4),
                (&art::CAN, 11),
                (&rig::DRILL_DOWN, 24),
                (&art::PART_BOX, 52),
            ],
            _ => [
                (&art::GEAR, 6),
                (&art::CAN, 20),
                (&art::PART_BOX, 34),
                (&art::GEAR, 62),
            ],
        };
        for (sprite, offset) in items {
            ui_icon(draw, sprite, shelf_x + offset, shelf_y - sprite.height());
        }
        // A spare drill head spinning on its display stand.
        if row == 1 {
            let drill = &rig::DRILL_DOWN;
            room_sprite(
                draw,
                drill,
                drill.frame_at(seconds * 0.5),
                shelf_x + 64,
                shelf_y - drill.height(),
                false,
            );
        }
    }
    // Tool board right of the counter.
    let board_x = counter_x + COUNTER_WIDTH + 14;
    ui_kit::well(draw, Rectangle::new(board_x as f32, 50.0, 44.0, 40.0));
    for (index, (dx, dy)) in [(6, 6), (18, 8), (30, 6), (8, 22), (24, 24)]
        .into_iter()
        .enumerate()
    {
        let (tool_x, tool_y) = (board_x + dx, 50 + dy);
        draw.draw_rectangle(tool_x + 2, tool_y - 2, 1, 2, FOG);
        if index % 2 == 0 {
            // Wrench.
            draw.draw_rectangle(tool_x + 1, tool_y, 3, 10, STEEL_LIGHT);
            draw.draw_rectangle(tool_x, tool_y, 5, 3, STEEL);
            draw.draw_rectangle(tool_x + 2, tool_y, 1, 2, INK);
        } else {
            // Hammer.
            draw.draw_rectangle(tool_x + 2, tool_y + 2, 2, 9, LOAM);
            draw.draw_rectangle(tool_x, tool_y, 6, 3, STEEL_DARK);
        }
    }
    // Crates stacked by the right wall and a barrel.
    let crate_sprite = &art::CRATE;
    let crates_x = board_x + 2;
    ui_icon(
        draw,
        crate_sprite,
        crates_x,
        FLOOR_Y - crate_sprite.height(),
    );
    ui_icon(
        draw,
        crate_sprite,
        crates_x + crate_sprite.width() - 1,
        FLOOR_Y - crate_sprite.height(),
    );
    ui_icon(
        draw,
        crate_sprite,
        crates_x + 6,
        FLOOR_Y - crate_sprite.height() * 2 + 1,
    );
    ui_icon(
        draw,
        &art::BARREL,
        crates_x + 30,
        FLOOR_Y - art::BARREL.height(),
    );
    // Cash register on the counter.
    let register_x = counter_x + COUNTER_WIDTH - 22;
    let register_y = FLOOR_Y - COUNTER_HEIGHT - 9;
    draw.draw_rectangle(register_x, register_y, 16, 10, INK);
    draw.draw_rectangle(register_x + 1, register_y + 1, 14, 8, STEEL);
    draw.draw_rectangle(register_x + 1, register_y + 1, 14, 1, STEEL_LIGHT);
    draw.draw_rectangle(register_x + 3, register_y + 3, 8, 3, INK);
    draw.draw_rectangle(register_x + 4, register_y + 4, 6, 1, LIME);
}

fn draw_service_animation(
    draw: &mut impl RaylibDraw,
    seat: &SeatUi,
    zone: SurfaceZone,
    counter_x: i32,
) {
    let Some(animation) = seat.service_animation else {
        return;
    };
    let step = (seat.service_animation_seconds * 18.0) as i32;
    let rig_x = room_pixel_x(seat.interior_x);
    match animation {
        ServiceAnimation::Fuel if zone == SurfaceZone::Fuel => {
            // Hose from the counter to the rig, with fuel pulses running along it.
            let start = counter_x;
            let y = FLOOR_Y - 10;
            let (from, to) = if rig_x < start {
                (rig_x + 7, start)
            } else {
                (start + COUNTER_WIDTH, rig_x - 7)
            };
            draw.draw_rectangle(from.min(to), y, (to - from).abs(), 2, INK);
            for pulse in (0..(to - from).abs()).step_by(6) {
                if (pulse / 6 + step) % 3 == 0 {
                    draw.draw_rectangle(from.min(to) + pulse, y, 2, 2, GOLD);
                }
            }
        }
        ServiceAnimation::Repair if zone == SurfaceZone::Repair => {
            for spark in 0..4 {
                let dx = (step * 7 + spark * 5).rem_euclid(14) - 7;
                let dy = (step * 3 + spark * 4).rem_euclid(10);
                draw.draw_rectangle(rig_x + dx, FLOOR_Y - 14 - dy, 1, 1, GOLD);
                draw.draw_rectangle(rig_x - dx, FLOOR_Y - 10 - dy / 2, 1, 1, ORANGE);
            }
        }
        _ => {}
    }
}

/// The player's rig standing on the floor at the seat's `interior_x`.
fn draw_rig(draw: &mut impl RaylibDraw, seat: &SeatUi, seconds: f32) {
    let body = &rig::RIG;
    let x = room_pixel_x(seat.interior_x) - body.width() / 2;
    let y = FLOOR_Y - body.height();
    draw.draw_rectangle(
        x + 1,
        FLOOR_Y,
        body.width() - 2,
        1,
        palette::with_alpha(INK, 120),
    );
    let flip = seat.interior_facing < 0.0;
    // Tracks roll only while the rig is walking (approximated by the animation clock).
    let frame = body.frame_at(seconds);
    room_sprite(draw, body, frame, x, y, flip);
}

fn npc_line(zone: SurfaceZone, view: &ViewSnapshot<'_>) -> &'static str {
    match zone {
        SurfaceZone::Fuel if view.player.fuel < view.player.fuel_capacity * 0.25 => {
            "Pip: You came in on fumes. Try leaving with more than courage."
        }
        SurfaceZone::Fuel => "Pip: Fuel sale rumors start as soon as miners stop exploding.",
        SurfaceZone::Repair if view.rescue_count > 0 => {
            "Iona: I patched your rescue dents. The drill keeps receipts."
        }
        SurfaceZone::Repair => "Iona: Slow landings are cheaper than heroic ones.",
        SurfaceZone::Depot => "Kade: Market's twitchy. Sell jackpots before the buyers blink.",
        SurfaceZone::Headquarters => "Director Vale: Bring contracts, not ghost stories.",
        SurfaceZone::Shop => "Bolt: If it still rattles, upgrade the part making the noise.",
        SurfaceZone::Bank if view.player.loan_debt > 0 => {
            "Ledger: Debt compounds faster than tunnels collapse."
        }
        SurfaceZone::Bank => "Ledger: Credit is just fuel with paperwork.",
        SurfaceZone::Explosives => "Nix: Set it, run, then brag from a safer zip code.",
        SurfaceZone::Salvage => "Mara: Everything lost underground becomes inventory eventually.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_fits_and_centres_in_whole_pixels() {
        let reference = RoomPlacement::fit(1280, 720);
        assert_eq!(
            (reference.pixel, reference.offset_x, reference.offset_y),
            (4, 0, 0)
        );
        let ultrawide = RoomPlacement::fit(2560, 1080);
        assert_eq!(ultrawide.pixel, 6);
        assert_eq!(ultrawide.offset_x, (2560 - 1920) / 2);
        let small = RoomPlacement::fit(960, 540);
        assert_eq!((small.pixel, small.offset_x, small.offset_y), (3, 0, 0));
        let tall = RoomPlacement::fit(1000, 1000);
        assert_eq!(tall.pixel, 3);
        assert_eq!(tall.offset_y, (1000 - 540) / 2);
    }

    #[test]
    fn visible_range_covers_the_screen_beyond_the_room() {
        let ultrawide = RoomPlacement::fit(2560, 1080);
        let (left, right) = ultrawide.visible_x();
        assert!(left * ultrawide.pixel + ultrawide.offset_x <= 0);
        assert!(right * ultrawide.pixel + ultrawide.offset_x >= 2560);
        let (top, bottom) = RoomPlacement::fit(1000, 1000).visible_y();
        assert!(top < 0 && bottom > ROOM_HEIGHT);
    }

    #[test]
    fn walkable_room_and_counters_fit_inside_the_room() {
        // The shell clamps the rig to 42..598 room units.
        assert!(room_pixel_x(42.0) - rig::RIG.width() / 2 >= 0);
        assert!(room_pixel_x(598.0) + rig::RIG.width() / 2 <= ROOM_WIDTH);
        for zone in [
            SurfaceZone::Fuel,
            SurfaceZone::Repair,
            SurfaceZone::Depot,
            SurfaceZone::Headquarters,
            SurfaceZone::Shop,
            SurfaceZone::Bank,
            SurfaceZone::Explosives,
            SurfaceZone::Salvage,
        ] {
            let counter_x = room_pixel_x(interior_service_x(zone)) - COUNTER_WIDTH / 2;
            assert!(
                counter_x > 60 && counter_x + COUNTER_WIDTH < ROOM_WIDTH,
                "{zone:?}"
            );
        }
    }

    #[test]
    fn room_points_map_onto_the_ui_canvas() {
        let placement = RoomPlacement::fit(1280, 720);
        let canvas = ui_kit::UiCanvas::for_framebuffer(1280, 720, 1.0);
        assert_eq!(placement.to_canvas(canvas, 160, 90), (320, 180));
    }
}
