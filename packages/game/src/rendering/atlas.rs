//! The Atlas on screen (Wave 14 W14-C; the seat's state is `shell::atlas`, the destinations
//! `crate::atlas`): the categories, the search box, the results, the live preview with its
//! info card, the buttons and map mode, laid out per seat canvas ([`layout`]).
//!
//! The preview is the real thing: home and planet ground painted with the game's own tile art
//! ([`preview`]), bosses and creatures with their own sprites, and anything in space with the
//! space renderer (the planet shader from orbit, stars, belts, stations, nebulae, black holes)
//! plus a galaxy minimap.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "screen layout mixes whole pixels and float positions"
)]

pub mod layout;
pub mod preview;

use raylib::{ffi, prelude::*};

use fatherload_simulation::{
    WorldState,
    space::{Space, SpaceTile},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::sprite,
    pointer, prompts,
    screen::UiFrame,
    space::{
        SpaceRenderer,
        view::{FlightMotion, SpaceCamera},
    },
    terrain::tile_art::TILE_ART,
    ui_kit::{self, ButtonState, PanelStyle, TextStyle},
};
use crate::{
    atlas::{
        AtlasCategory, AtlasTarget, Destination,
        hit::{AtlasButton, AtlasHit},
        index,
        map::MapKind,
    },
    input::prompts::Prompt,
    shell::atlas::SeatAtlas,
};
use layout::{AtlasLayout, AtlasShape};
use preview::{AtlasPreviews, Backdrop, TilePreview, space_focus};

/// What the Atlas draws its pictures with (the renderer's caches and the space renderer).
pub struct AtlasPictures<'a> {
    pub previews: &'a AtlasPreviews,
    pub space: Option<&'a SpaceRenderer>,
    /// The seat's view is clipped (split-screen): drawing the space preview re-clips it after.
    pub view_clip: Option<crate::render_snapshot::Viewport>,
}

/// The prompt that presses `button` (shown beside its label).
const fn button_prompt(button: AtlasButton) -> Prompt {
    match button {
        AtlasButton::Go => Prompt::Confirm,
        AtlasButton::Previous => Prompt::CycleWeapon,
        AtlasButton::Next => Prompt::CycleExplosive,
        AtlasButton::Random => Prompt::Scan,
        AtlasButton::Bookmark => Prompt::Bomb,
        AtlasButton::Back => Prompt::Detonate,
        AtlasButton::Map => Prompt::Map,
        AtlasButton::Close => Prompt::Back,
    }
}

/// Draws seat `ui`'s Atlas over its view.
pub(super) fn draw_atlas(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    pictures: &AtlasPictures<'_>,
) {
    let atlas = &ui.seat.atlas;
    let world = ui.snapshot.space.world;
    let canvas = ui_kit::canvas();
    let layout = AtlasLayout::compute(
        canvas.width as f32,
        canvas.height as f32,
        atlas.map.is_some(),
    );
    // The backdrop: a deep blue sheet over the whole view (the world is not drawn under it).
    draw.draw_rectangle_gradient_v(
        0,
        0,
        layout.frame.width as i32,
        layout.frame.height as i32,
        INK,
        Color::new(18, 16, 34, 255),
    );
    pointer::record_dialog(layout.frame);
    draw_header(draw, &layout, atlas);
    draw_categories(draw, &layout, atlas);
    if atlas.map.is_some() {
        draw_map(draw, &layout, ui, pictures);
    } else {
        draw_list(draw, &layout, atlas);
    }
    let current = atlas.current();
    draw_preview(draw, &layout, ui, pictures, current);
    draw_card(draw, &layout, ui, world, current);
    draw_buttons(draw, &layout, atlas, world.space_enabled());
}

fn draw_header(draw: &mut RaylibDrawHandle<'_>, layout: &AtlasLayout, atlas: &SeatAtlas) {
    let title = layout.title;
    ui_kit::text(
        draw,
        "ATLAS",
        title.x as i32 + 2,
        title.y as i32 + 4,
        TextStyle::Bold,
        GOLD,
    );
    if layout.shape != AtlasShape::Narrow {
        let badge_x = title.x as i32 + 2 + ui_kit::text_width("ATLAS", TextStyle::Bold) + 4;
        draw.draw_rectangle(badge_x, title.y as i32 + 4, 21, 9, MAROON);
        ui_kit::text(
            draw,
            "TEST",
            badge_x + 2,
            title.y as i32 + 4,
            TextStyle::Body,
            CREAM,
        );
    }
    let search = layout.search;
    ui_kit::well(draw, search);
    pointer::record_row(search, search, AtlasHit::Search.encode());
    let x = search.x as i32 + 5;
    let y = search.y as i32 + 4;
    let width = search.width as i32 - 10;
    let device = prompts::menu_device();
    if atlas.search.is_empty() && !atlas.typing {
        let hint = match prompts::prompt_width(prompts::hint(Prompt::Paste, ""), device) {
            // A keyboard types after `/`.
            Some(_) => "/ to search: planet ice, leviathan, descent 5",
            None => "Search: planet ice, leviathan, descent 5",
        };
        ui_kit::text(
            draw,
            &ui_kit::truncate(hint, width, TextStyle::Body),
            x,
            y,
            TextStyle::Body,
            SLATE_LIGHT,
        );
    } else {
        let shown = ui_kit::truncate(&atlas.search, width - 6, TextStyle::Body);
        ui_kit::text(draw, &shown, x, y, TextStyle::Body, BONE);
        if atlas.typing && (ui_kit::seconds() * 2.0) as i32 % 2 == 0 {
            let caret = x + ui_kit::text_width(&shown, TextStyle::Body) + 1;
            draw.draw_rectangle(caret, y, 1, 9, GOLD);
        }
    }
}

/// A flat tab: gold when chosen.
fn tab(draw: &mut RaylibDrawHandle<'_>, area: Rectangle, label: &str, chosen: bool) {
    let (fill, rim, ink) = if chosen {
        (GOLD, CREAM, INK)
    } else {
        (SLATE_DARK, SLATE, BONE)
    };
    draw.draw_rectangle_rec(area, fill);
    draw.draw_rectangle_lines_ex(area, 1.0, rim);
    let shown = ui_kit::truncate(label, area.width as i32 - 4, TextStyle::Bold);
    let width = ui_kit::text_width(&shown, TextStyle::Bold);
    ui_kit::text_shadowed(
        draw,
        &shown,
        area.x as i32 + (area.width as i32 - width) / 2,
        area.y as i32 + (area.height as i32 - 9) / 2,
        TextStyle::Bold,
        ink,
        if chosen { None } else { Some(INK) },
    );
}

fn draw_categories(draw: &mut RaylibDrawHandle<'_>, layout: &AtlasLayout, atlas: &SeatAtlas) {
    let searching = !atlas.search.trim().is_empty();
    for (category, area) in AtlasCategory::ALL.iter().zip(&layout.categories) {
        let title =
            if ui_kit::text_width(category.title(), TextStyle::Bold) + 4 <= area.width as i32 {
                category.title()
            } else {
                category.short_title()
            };
        tab(
            draw,
            *area,
            title,
            *category == atlas.category && !searching,
        );
        pointer::record_row(*area, *area, AtlasHit::Category(category.index()).encode());
    }
}

/// The colour that marks a destination's kind in the list (by its category; the bosses red).
const fn kind_colour(destination: &Destination) -> Color {
    match destination.kind.category() {
        AtlasCategory::Depth => AMBER,
        AtlasCategory::Places | AtlasCategory::Bookmarks | AtlasCategory::Recent => MINT,
        AtlasCategory::Bosses => RED,
        AtlasCategory::Settlements => LIME,
        AtlasCategory::Players => PINK,
        AtlasCategory::Space => SKY_LIGHT,
    }
}

fn draw_list(draw: &mut RaylibDrawHandle<'_>, layout: &AtlasLayout, atlas: &SeatAtlas) {
    let area = layout.list;
    ui_kit::panel(draw, area, PanelStyle::Standard);
    let inner = Rectangle::new(
        area.x + 3.0,
        area.y + 3.0,
        area.width - 6.0,
        area.height - 6.0,
    );
    if atlas.shown.is_empty() {
        let text = if atlas.search.trim().is_empty() {
            atlas.category.empty_text()
        } else {
            "Nothing matches. Try fewer words."
        };
        ui_kit::paragraph(
            draw,
            text,
            inner.x as i32 + 3,
            inner.y as i32 + 3,
            inner.width as i32 - 6,
            TextStyle::Body,
            FOG,
        );
        return;
    }
    let rows = layout.visible_rows().max(1);
    let count = atlas.shown.len();
    let first = atlas
        .selected
        .saturating_sub(rows / 2)
        .min(count.saturating_sub(rows));
    ui_kit::begin_clip(inner);
    for (offset, destination) in atlas.shown.iter().enumerate().skip(first).take(rows) {
        let y = inner.y + (offset - first) as f32 * layout.row_height;
        let row = Rectangle::new(inner.x, y, inner.width, layout.row_height - 1.0);
        let selected = offset == atlas.selected;
        if selected {
            draw.draw_rectangle_rec(row, with_alpha(AMBER, 70));
            draw.draw_rectangle(row.x as i32, row.y as i32, 2, row.height as i32, GOLD);
        }
        draw.draw_rectangle(
            row.x as i32 + 4,
            row.y as i32 + 4,
            3,
            3,
            kind_colour(destination),
        );
        let text_x = row.x as i32 + 10;
        let width = row.width as i32 - 12;
        let name_colour = if selected { CREAM } else { BONE };
        ui_kit::text(
            draw,
            &ui_kit::truncate(&destination.name, width, TextStyle::Bold),
            text_x,
            row.y as i32 + 1,
            TextStyle::Bold,
            name_colour,
        );
        if layout.row_height >= 18.0 {
            ui_kit::text(
                draw,
                &ui_kit::truncate(&destination.subtitle, width, TextStyle::Body),
                text_x,
                row.y as i32 + 10,
                TextStyle::Body,
                FOG,
            );
        }
        if atlas.bookmarked(destination) {
            draw.draw_rectangle(
                row.x as i32 + row.width as i32 - 5,
                row.y as i32 + 3,
                3,
                3,
                GOLD,
            );
        }
        pointer::record_row(row, inner, AtlasHit::Row(offset).encode());
    }
    ui_kit::end_clip();
    // A scroll bar when the list runs past the panel.
    if count > rows {
        let track = Rectangle::new(area.x + area.width - 3.0, inner.y, 1.0, inner.height);
        draw.draw_rectangle_rec(track, SLATE_DARK);
        let thumb = (rows as f32 / count as f32 * track.height).max(4.0);
        let top = first as f32 / (count - rows).max(1) as f32 * (track.height - thumb);
        draw.draw_rectangle_rec(
            Rectangle::new(track.x, track.y + top, 1.0, thumb),
            SLATE_LIGHT,
        );
    }
}

/// The inside of a framed part.
fn inside(area: Rectangle) -> Rectangle {
    Rectangle::new(
        area.x + 3.0,
        area.y + 3.0,
        area.width - 6.0,
        area.height - 6.0,
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "a preview of every kind of place: ground, space, the story"
)]
fn draw_preview(
    draw: &mut RaylibDrawHandle<'_>,
    layout: &AtlasLayout,
    ui: &UiFrame<'_, '_>,
    pictures: &AtlasPictures<'_>,
    current: Option<&Destination>,
) {
    let area = layout.preview;
    ui_kit::panel(draw, area, PanelStyle::Accent);
    let inner = inside(area);
    draw.draw_rectangle_rec(inner, INK);
    let Some(destination) = current else {
        ui_kit::text_centred(
            draw,
            "Pick a destination to preview it.",
            inner.x as i32 + inner.width as i32 / 2,
            inner.y as i32 + inner.height as i32 / 2 - 4,
            TextStyle::Body,
            FOG,
        );
        return;
    };
    let world = ui.snapshot.space.world;
    let seat = ui.view.seat_index;
    if let Some((centre, reach)) = space_focus(world, &destination.target) {
        draw_space_preview(draw, ui, pictures, inner, (centre, reach));
        // A planet's landing site, in its own tile art, in the corner.
        if let Some(tile) = pictures.previews.tile(world, &destination.target) {
            // Inside the preview even when it is short (a phone held sideways).
            let inset_width = (inner.width * 0.42)
                .min(150.0)
                .min((inner.height - 6.0) / 0.55)
                .floor();
            let inset = Rectangle::new(
                inner.x + 3.0,
                inner.y + inner.height - (inset_width * 0.55).floor() - 3.0,
                inset_width,
                (inset_width * 0.55).floor(),
            );
            if inset.height >= 24.0 {
                draw.draw_rectangle_lines_ex(
                    Rectangle::new(
                        inset.x - 1.0,
                        inset.y - 1.0,
                        inset.width + 2.0,
                        inset.height + 2.0,
                    ),
                    1.0,
                    SLATE_LIGHT,
                );
                draw_tile_preview(draw, inset, tile, destination, false);
                ui_kit::text(
                    draw,
                    "Landing site",
                    inset.x as i32 + 2,
                    inset.y as i32 + 1,
                    TextStyle::Body,
                    CREAM,
                );
            }
        }
        draw_minimap(draw, inner, world, ui, destination);
    } else if let Some(tile) = pictures.previews.tile(world, &destination.target) {
        draw_tile_preview(draw, inner, tile, destination, true);
    } else if destination.target == AtlasTarget::Story {
        ui_kit::paragraph(
            draw,
            "The galaxy story's current goal: the system, the world, the relic or the gate the road asks for next.",
            inner.x as i32 + 8,
            inner.y as i32 + 8,
            inner.width as i32 - 16,
            TextStyle::Body,
            BONE,
        );
    }
    let _ = seat;
    // The caption: what this is.
    let caption = format!("LIVE - {}", destination.kind.name().to_uppercase());
    let width = ui_kit::text_width(&caption, TextStyle::Body) + 6;
    draw.draw_rectangle(
        inner.x as i32 + inner.width as i32 - width - 2,
        inner.y as i32 + 2,
        width,
        11,
        with_alpha(INK, 190),
    );
    ui_kit::text(
        draw,
        &caption,
        inner.x as i32 + inner.width as i32 - width + 1,
        inner.y as i32 + 3,
        TextStyle::Body,
        MINT,
    );
}

/// A tile preview drawn into `area`: its art at one art pixel per screen pixel where it fits,
/// the fluids over it, the arrival marker and the boss.
fn draw_tile_preview(
    draw: &mut RaylibDrawHandle<'_>,
    area: Rectangle,
    tile: &TilePreview,
    destination: &Destination,
    show_boss: bool,
) {
    let backdrop = match tile.backdrop {
        Backdrop::Sky => (SKY, SKY_LIGHT),
        Backdrop::Underground => (NIGHT, INK),
        Backdrop::Space => (INK, NIGHT),
    };
    draw.draw_rectangle_gradient_v(
        area.x as i32,
        area.y as i32,
        area.width as i32,
        area.height as i32,
        backdrop.0,
        backdrop.1,
    );
    let Some(texture) = tile.texture() else {
        return;
    };
    let canvas = ui_kit::canvas();
    let (tex_width, tex_height) = (texture.width as f32, texture.height as f32);
    // Canvas pixels per art pixel: one art pixel a screen pixel, larger when the window is
    // bigger than the picture.
    let scale = (1.0 / canvas.pixel.max(1) as f32)
        .max(area.width / tex_width)
        .max(area.height / tex_height);
    let (crop_width, crop_height) = (area.width / scale, area.height / scale);
    // Over open sky the arrival sits high, so the ground under it fills the picture.
    let lift = if tile.backdrop == Backdrop::Underground {
        0.0
    } else {
        crop_height * 0.22
    };
    let focus = (
        ((tile.target.x - tile.origin.x) * TILE_ART + TILE_ART / 2) as f32,
        ((tile.target.y - tile.origin.y) * TILE_ART + TILE_ART / 2) as f32,
    );
    let view_y = focus.1 + lift;
    let source_x = (focus.0 - crop_width / 2.0).clamp(0.0, (tex_width - crop_width).max(0.0));
    let source_y = (view_y - crop_height / 2.0).clamp(0.0, (tex_height - crop_height).max(0.0));
    draw.draw_texture_pro(
        texture,
        Rectangle::new(source_x, source_y, crop_width, crop_height),
        area,
        Vector2::zero(),
        0.0,
        Color::WHITE,
    );
    let to_screen = |art_x: f32, art_y: f32| {
        (
            area.x + (art_x - source_x) * scale,
            area.y + (art_y - source_y) * scale,
        )
    };
    let tile_size = TILE_ART as f32 * scale;
    ui_kit::begin_clip(area);
    for (at, colour) in tile.fluids() {
        let (x, y) = to_screen(
            ((at.x - tile.origin.x) * TILE_ART) as f32,
            ((at.y - tile.origin.y) * TILE_ART) as f32,
        );
        draw.draw_rectangle_rec(Rectangle::new(x, y, tile_size, tile_size), *colour);
    }
    let (x, y) = to_screen(focus.0, focus.1);
    // The boss stands beside the arrival, in its own sprite.
    if show_boss && let Some(boss) = destination.boss {
        let sprite = super::world::enemies::enemy_sprite(boss);
        let height = sprite.height() as f32;
        let fit = (area.height * 0.55 / height).clamp(0.25, 1.0);
        let width = sprite.width() as f32 * fit;
        let frame = (ui_kit::seconds() * 4.0) as usize % sprite.frames.len().max(1);
        sprite::draw_sprite(
            draw,
            sprite,
            frame,
            Vector2::new(
                (x + tile_size * 3.0).min(area.x + area.width - width - 2.0),
                y + tile_size * 0.5 - height * fit,
            ),
            fit,
            Color::WHITE,
        );
    }
    // The arrival: a beacon over the spot.
    let pulse = (ui_kit::seconds() * 3.0).sin().mul_add(1.5, 4.5);
    draw.draw_circle_lines(x as i32, y as i32, pulse, GOLD);
    draw.draw_circle_lines(x as i32, y as i32, pulse + 3.0, with_alpha(GOLD, 90));
    draw.draw_triangle(
        Vector2::new(x - 3.0, y - pulse - 8.0),
        Vector2::new(x, y - pulse - 3.0),
        Vector2::new(x + 3.0, y - pulse - 8.0),
        CREAM,
    );
    ui_kit::end_clip();
    if !tile.done() {
        ui_kit::text(
            draw,
            "painting...",
            area.x as i32 + 3,
            area.y as i32 + area.height as i32 - 11,
            TextStyle::Body,
            FOG,
        );
    }
}

/// The space preview: the space renderer's own picture of the target, in `area`.
fn draw_space_preview(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    pictures: &AtlasPictures<'_>,
    area: Rectangle,
    (centre, reach): (fatherload_simulation::position::WorldPos, f32),
) {
    let (Some(space), Some(live)) = (pictures.space, pictures.previews.space(ui.view.seat_index))
    else {
        draw.draw_rectangle_rec(area, INK);
        return;
    };
    let points = ui_kit::canvas().to_points(area);
    let camera = SpaceCamera {
        centre,
        roll: 0.0,
        px_per_tile: (points.width.min(points.height) * 0.5 / reach.max(1.0)).max(1e-6),
        drop: 0.0,
        zoom: 1.0,
    };
    let seconds = ui.shell.update_ticks as f32 / 60.0;
    let view = live.view(
        camera,
        FlightMotion::Coasting,
        None,
        None,
        seconds,
        ui.snapshot.effects,
        true,
    );
    // The space renderer draws in screen points: leave the canvas for it, clipped to the
    // preview, then come back.
    // SAFETY: balanced mode and scissor calls inside the frame's drawing pass: the canvas
    // mode is reopened below and the seat's own clip restored.
    unsafe {
        ffi::EndMode2D();
        ffi::BeginScissorMode(
            points.x.round() as i32,
            points.y.round() as i32,
            points.width.round() as i32,
            points.height.round() as i32,
        );
    }
    space.draw(draw, &view, points);
    // SAFETY: closes the scissor above; the seat's view clip comes back when it had one.
    unsafe {
        ffi::EndScissorMode();
        if let Some(viewport) = pictures.view_clip {
            ffi::BeginScissorMode(viewport.x, viewport.y, viewport.width, viewport.height);
        }
    }
    // Back on the canvas (the caller's scope closes it as before).
    std::mem::forget(ui_kit::UiCanvasScope::begin(draw));
}

/// The galaxy round a space target: its systems as dots, the target ringed, the seat marked.
fn draw_minimap(
    draw: &mut RaylibDrawHandle<'_>,
    preview: Rectangle,
    world: &WorldState,
    ui: &UiFrame<'_, '_>,
    destination: &Destination,
) {
    let Some((centre, _)) = space_focus(world, &destination.target) else {
        return;
    };
    // Clear of the caption along the top on a short preview (a phone held sideways).
    let size = (preview.width.min(preview.height) * 0.36)
        .clamp(40.0, 96.0)
        .min(preview.height - 20.0)
        .floor();
    if size < 24.0 {
        return;
    }
    let area = Rectangle::new(
        preview.x + preview.width - size - 3.0,
        preview.y + preview.height - size - 3.0,
        size,
        size,
    );
    draw.draw_rectangle_rec(area, with_alpha(INK, 225));
    draw.draw_rectangle_lines_ex(area, 1.0, SLATE_LIGHT);
    let map = crate::atlas::map::GalaxyMap {
        centre: SpaceTile::of_pos(centre),
        radius: 2 * fatherload_simulation::space::SPACE_SECTOR_TILES,
    };
    let galaxy = Space::shared(world.world_seed());
    ui_kit::begin_clip(area);
    for sector in galaxy.sectors_near(map.centre, map.radius * 3 / 2) {
        for system in &sector.systems {
            let at = map.uv_of(system.centre);
            let [red, green, blue] = system.class.colour();
            draw.draw_rectangle(
                (area.x + at.0 * area.width) as i32,
                (area.y + at.1 * area.height) as i32,
                2,
                2,
                Color::new(red, green, blue, 255),
            );
        }
    }
    // The seat.
    let (u, v) = map.uv_of(SpaceTile::of_pos(ui.seat.atlas.origin.space));
    if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
        let (x, y) = (area.x + u * area.width, area.y + v * area.height);
        draw.draw_rectangle(x as i32 - 1, y as i32 - 1, 3, 3, PINK);
    }
    // The target, ringed in the middle.
    let (x, y) = (area.x + area.width / 2.0, area.y + area.height / 2.0);
    draw.draw_circle_lines(x as i32, y as i32, 4.0, GOLD);
    ui_kit::end_clip();
    if ui_kit::text_width("Galaxy", TextStyle::Body) + 4 > size as i32 {
        return;
    }
    ui_kit::text(
        draw,
        "Galaxy",
        area.x as i32 + 2,
        area.y as i32 + 1,
        TextStyle::Body,
        FOG,
    );
}

#[allow(
    clippy::too_many_lines,
    reason = "the card's name, mark, lines and sprites"
)]
fn draw_card(
    draw: &mut RaylibDrawHandle<'_>,
    layout: &AtlasLayout,
    ui: &UiFrame<'_, '_>,
    world: &WorldState,
    current: Option<&Destination>,
) {
    let area = layout.card;
    ui_kit::panel(draw, area, PanelStyle::Standard);
    let inner = inside(area);
    let Some(destination) = current else {
        return;
    };
    let x = inner.x as i32 + 3;
    let width = inner.width as i32 - 6;
    let bottom = (inner.y + inner.height) as i32;
    let mut y = inner.y as i32 + 2;
    let name_width = if ui.seat.atlas.bookmarked(destination) {
        width - 12
    } else {
        width
    };
    ui_kit::text(
        draw,
        &ui_kit::truncate(&destination.name, name_width, TextStyle::Bold),
        x,
        y,
        TextStyle::Bold,
        GOLD,
    );
    if ui.seat.atlas.bookmarked(destination) {
        // A ribbon: the bookmark's mark.
        draw.draw_rectangle(x + width - 7, y, 6, 8, GOLD);
        draw.draw_triangle(
            Vector2::new((x + width - 7) as f32, (y + 8) as f32),
            Vector2::new((x + width - 1) as f32, (y + 8) as f32),
            Vector2::new((x + width - 4) as f32, (y + 5) as f32),
            INK,
        );
    }
    y += 10;
    if y + 9 <= bottom {
        ui_kit::text(
            draw,
            &ui_kit::truncate(&destination.subtitle, width, TextStyle::Body),
            x,
            y,
            TextStyle::Body,
            MIST,
        );
        y += 11;
    }
    let distance = index::distance_text(world, &destination.target, &ui.seat.atlas.origin);
    let mut lines: Vec<(&str, String)> = vec![("Distance", distance)];
    lines.extend(
        destination
            .facts
            .iter()
            .map(|fact| (fact.label, fact.value.clone())),
    );
    // Creature sprites take the card's last row when there is room.
    let sprites_room = if destination.creatures.is_empty() && destination.boss.is_none() {
        0
    } else {
        22
    };
    for (label, value) in lines {
        let label_width = ui_kit::text_width(label, TextStyle::Bold) + 4;
        let value_lines = ui_kit::wrap(&value, (width - label_width).max(20), TextStyle::Body);
        let needed = value_lines.len().max(1) as i32 * 10;
        if y + needed > bottom - sprites_room {
            break;
        }
        ui_kit::text(draw, label, x, y, TextStyle::Bold, SLATE_LIGHT);
        for (line_index, line) in value_lines.iter().enumerate() {
            ui_kit::text(
                draw,
                line,
                x + label_width,
                y + line_index as i32 * 10,
                TextStyle::Body,
                BONE,
            );
        }
        y += needed;
    }
    if sprites_room > 0 && y + sprites_room <= bottom + 2 {
        let mut sprite_x = x as f32;
        let row_y = (bottom - 20) as f32;
        for kind in destination
            .boss
            .into_iter()
            .chain(destination.creatures.iter().copied())
        {
            let sprite = super::world::enemies::enemy_sprite(kind);
            let fit = (18.0 / sprite.height().max(1) as f32).min(1.0);
            let sprite_width = sprite.width() as f32 * fit;
            if sprite_x + sprite_width > (x + width) as f32 {
                break;
            }
            sprite::draw_sprite(
                draw,
                sprite,
                0,
                Vector2::new(sprite_x, row_y + 18.0 - sprite.height() as f32 * fit),
                fit,
                Color::WHITE,
            );
            sprite_x += sprite_width + 4.0;
        }
    }
}

fn draw_buttons(
    draw: &mut RaylibDrawHandle<'_>,
    layout: &AtlasLayout,
    atlas: &SeatAtlas,
    world_space_open: bool,
) {
    let device = prompts::menu_device();
    let has_current = atlas.current().is_some();
    for (button, area) in &layout.buttons {
        let enabled = match button {
            AtlasButton::Go | AtlasButton::Bookmark => has_current,
            AtlasButton::Previous | AtlasButton::Next | AtlasButton::Random => {
                atlas.map.is_some() || atlas.shown.len() > 1
            }
            AtlasButton::Back => atlas.back_count > 0,
            AtlasButton::Map | AtlasButton::Close => true,
        };
        let state = if !enabled {
            ButtonState::Disabled
        } else if *button == AtlasButton::Go {
            ButtonState::Selected
        } else {
            ButtonState::Idle
        };
        sprite::draw_nine_slice(
            draw,
            match state {
                ButtonState::Selected => &super::pixel_art::sprites::ui::BUTTON_SELECTED,
                ButtonState::Disabled => &super::pixel_art::sprites::ui::BUTTON_DISABLED,
                ButtonState::Idle => &super::pixel_art::sprites::ui::BUTTON_IDLE,
            },
            *area,
            1.0,
            Color::WHITE,
        );
        let label = match button {
            AtlasButton::Map => match atlas.next_map(world_space_open) {
                Some(MapKind::Galaxy) if atlas.map.is_some() => "Galaxy",
                Some(MapKind::Depth) if atlas.map.is_some() => "Depths",
                Some(_) => "Map",
                None => "List",
            },
            AtlasButton::Bookmark
                if ui_kit::text_width("Bookmark", TextStyle::Bold) + 8 > area.width as i32 =>
            {
                "Mark"
            }
            AtlasButton::Bookmark
                if atlas
                    .current()
                    .is_some_and(|destination| atlas.bookmarked(destination)) =>
            {
                "Unmark"
            }
            _ => button.label(),
        };
        let hint = prompts::hint(button_prompt(*button), "");
        let glyph = prompts::prompt_width(hint, device).filter(|width| {
            *width + 4 + ui_kit::text_width(label, TextStyle::Bold) <= area.width as i32 - 6
        });
        let text_width = ui_kit::text_width(label, TextStyle::Bold);
        let total = text_width + glyph.map_or(0, |width| width + 3);
        let mut x = area.x as i32 + (area.width as i32 - total) / 2;
        let y = area.y as i32 + (area.height as i32 - 9) / 2;
        if glyph.is_some() {
            x += prompts::draw_prompt(draw, x, y, hint, device, BONE) + 3;
        }
        ui_kit::text_shadowed(
            draw,
            &ui_kit::truncate(label, area.width as i32 - 4, TextStyle::Bold),
            x,
            y,
            TextStyle::Bold,
            state.text_color(),
            Some(if state == ButtonState::Selected {
                CREAM
            } else {
                INK
            }),
        );
        pointer::record_row(*area, *area, AtlasHit::Button(*button).encode());
    }
}

#[allow(clippy::too_many_lines, reason = "the depth map and the galaxy map")]
fn draw_map(
    draw: &mut RaylibDrawHandle<'_>,
    layout: &AtlasLayout,
    ui: &UiFrame<'_, '_>,
    pictures: &AtlasPictures<'_>,
) {
    let atlas = &ui.seat.atlas;
    let Some(map) = &atlas.map else {
        return;
    };
    let area = layout.map;
    ui_kit::panel(draw, area, PanelStyle::Accent);
    let inner = inside(area);
    draw.draw_rectangle_rec(inner, INK);
    // The map keeps its own shape inside the panel.
    let shape = match map.kind {
        MapKind::Depth => crate::shell::atlas::DEPTH_MAP_ASPECT,
        MapKind::Galaxy => 1.0,
    };
    let (width, height) = if inner.width / inner.height > shape {
        (inner.height * shape, inner.height)
    } else {
        (inner.width, inner.width / shape)
    };
    let label_room = if map.kind == MapKind::Depth {
        (inner.width - width).clamp(0.0, 70.0)
    } else {
        0.0
    };
    // A line for the map's title over it.
    let title_room = 11.0;
    let (width, height) = if height > inner.height - title_room {
        (
            width * (inner.height - title_room) / height,
            inner.height - title_room,
        )
    } else {
        (width, height)
    };
    let picture = Rectangle::new(
        (inner.x + (inner.width - width - label_room) / 2.0).floor(),
        (inner.y + title_room + (inner.height - title_room - height) / 2.0).floor(),
        width.floor(),
        height.floor(),
    );
    pointer::record_area(picture, |u, v| AtlasHit::Map(u, v).encode());
    let seat = ui.view.seat_index;
    let to_screen = |(u, v): (f32, f32)| {
        (
            picture.x + u * picture.width,
            picture.y + v * picture.height,
        )
    };
    match map.kind {
        MapKind::Depth => {
            if let Some(found) = pictures.previews.depth_map(seat) {
                if let Some(texture) = &found.texture {
                    draw.draw_texture_pro(
                        texture,
                        Rectangle::new(0.0, 0.0, texture.width as f32, texture.height as f32),
                        picture,
                        Vector2::zero(),
                        0.0,
                        Color::WHITE,
                    );
                }
                for (index, (v, numeral, name)) in found.descents.iter().enumerate() {
                    let y = picture.y + v * picture.height;
                    draw.draw_rectangle(
                        picture.x as i32,
                        y as i32,
                        picture.width as i32,
                        1,
                        with_alpha(CREAM, 120),
                    );
                    let label_x = (picture.x + picture.width) as i32 + 4;
                    let next = found
                        .descents
                        .get(index + 1)
                        .map_or(1.0, |(next, _, _)| *next);
                    let middle = picture.y + (v + next) * 0.5 * picture.height;
                    if label_room >= 20.0 {
                        ui_kit::text(
                            draw,
                            numeral,
                            label_x,
                            middle as i32 - 4,
                            TextStyle::Bold,
                            GOLD,
                        );
                        if label_room >= 60.0 {
                            ui_kit::text(
                                draw,
                                &ui_kit::truncate(name, label_room as i32 - 22, TextStyle::Body),
                                label_x + 16,
                                middle as i32 - 4,
                                TextStyle::Body,
                                FOG,
                            );
                        }
                    }
                }
                for (uv, gate) in &found.marks {
                    if !(0.0..=1.0).contains(&uv.0) {
                        continue;
                    }
                    let (x, y) = to_screen(*uv);
                    let colour = if *gate { RED } else { MINT };
                    draw.draw_rectangle(x as i32 - 1, y as i32 - 1, 3, 3, colour);
                }
            }
            // The seat.
            if let Some(tile) = atlas.origin.tile {
                let (x, y) = to_screen(map.depth.uv_of(tile));
                draw.draw_rectangle(x as i32 - 1, y as i32 - 1, 3, 3, PINK);
            }
        }
        MapKind::Galaxy => {
            draw.draw_rectangle_rec(picture, Color::new(6, 8, 18, 255));
            ui_kit::begin_clip(picture);
            if let Some(found) = pictures.previews.galaxy(seat) {
                for ((u, v), colour, name, planets) in &found.systems {
                    let (x, y) = to_screen((*u, *v));
                    let size = 1 + (*planets as i32).min(4) / 2;
                    draw.draw_rectangle(
                        x as i32 - size / 2,
                        y as i32 - size / 2,
                        size + 1,
                        size + 1,
                        *colour,
                    );
                    if picture.width >= 160.0 && *planets >= 5 {
                        ui_kit::text(
                            draw,
                            name,
                            x as i32 + 3,
                            y as i32 - 4,
                            TextStyle::Body,
                            with_alpha(FOG, 200),
                        );
                    }
                }
            }
            let (x, y) = to_screen(map.galaxy.uv_of(SpaceTile::of_pos(atlas.origin.space)));
            draw.draw_rectangle(x as i32 - 1, y as i32 - 1, 3, 3, PINK);
            ui_kit::end_clip();
        }
    }
    // The cursor: a crosshair.
    let (x, y) = to_screen(map.cursor);
    let (x, y) = (x as i32, y as i32);
    let rect = (
        picture.x as i32,
        picture.y as i32,
        picture.width as i32,
        picture.height as i32,
    );
    draw.draw_rectangle(rect.0, y, rect.2, 1, with_alpha(GOLD, 110));
    draw.draw_rectangle(x, rect.1, 1, rect.3, with_alpha(GOLD, 110));
    draw.draw_rectangle_lines(x - 3, y - 3, 7, 7, GOLD);
    let title = match map.kind {
        MapKind::Depth => "Depth map: all eight Descents",
        MapKind::Galaxy => "Galaxy map",
    };
    ui_kit::text(
        draw,
        &ui_kit::truncate(title, inner.width as i32 - 4, TextStyle::Body),
        inner.x as i32 + 2,
        inner.y as i32 + 1,
        TextStyle::Body,
        FOG,
    );
}
