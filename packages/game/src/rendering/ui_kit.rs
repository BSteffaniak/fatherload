//! The pixel UI kit: a virtual canvas in art pixels plus the widgets every screen draws with.
//!
//! Screen-space UI is drawn inside a [`UiCanvasScope`]: a camera that scales the canvas by the
//! frame's integer UI pixel size (see [`crate::pixel_scale::ui_pixel_size`]), so every rectangle,
//! sprite and glyph lands on whole art pixels. Coordinates in this module are canvas art pixels;
//! use whole numbers.
//!
//! Widgets: framed panels ([`panel`], three styles), inset wells, buttons with idle / selected /
//! disabled states, stat bars, item rows (icon + label + detail + price), tabs, tooltips / speech
//! bubbles, warning badges, icons and crisp shadowed text.

use std::{cell::Cell, ffi::CString};

use raylib::{ffi, prelude::*};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::pixel_art::{
    font::GLYPH_HEIGHT,
    palette,
    sprite::{self, NineSlice, SpriteDef},
    sprites::{icons::ICON_SIZE, ui},
};
use crate::{pixel_scale, render_snapshot::Viewport};

/// The frame's UI canvas: its size in art pixels and how it maps to the framebuffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiCanvas {
    /// Physical framebuffer pixels per art pixel.
    pub pixel: i32,
    /// Physical pixels per logical screen point.
    pub density: f32,
    /// Canvas width in art pixels.
    pub width: i32,
    /// Canvas height in art pixels.
    pub height: i32,
}

impl UiCanvas {
    /// The canvas for a `render_width` x `render_height` framebuffer at `density`.
    #[must_use]
    pub fn for_framebuffer(render_width: i32, render_height: i32, density: f32) -> Self {
        let pixel = pixel_scale::ui_pixel_size(render_width, render_height);
        Self {
            pixel,
            density: density.max(1.0),
            width: render_width.max(1) / pixel,
            height: render_height.max(1) / pixel,
        }
    }

    /// The canvas for the frame being drawn.
    #[must_use]
    pub fn from_draw(draw: &RaylibDrawHandle<'_>) -> Self {
        let (render_width, render_height) = (draw.get_render_width(), draw.get_render_height());
        let density = super::pixel_density(
            (render_width, render_height),
            (draw.get_screen_width(), draw.get_screen_height()),
        );
        Self::for_framebuffer(render_width, render_height, density)
    }

    /// Logical screen points per art pixel.
    #[must_use]
    pub fn points_per_pixel(self) -> f32 {
        self.pixel as f32 / self.density
    }

    /// The whole canvas.
    #[must_use]
    pub fn rect(self) -> Rectangle {
        Rectangle::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    /// A viewport (logical points) in canvas art pixels, shrunk to whole art pixels inside it.
    #[must_use]
    pub fn viewport_rect(self, viewport: Viewport) -> Rectangle {
        let scale = self.points_per_pixel();
        let x0 = (viewport.x as f32 / scale).ceil();
        let y0 = (viewport.y as f32 / scale).ceil();
        let x1 = ((viewport.x + viewport.width) as f32 / scale).floor();
        let y1 = ((viewport.y + viewport.height) as f32 / scale).floor();
        Rectangle::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// A canvas rect in logical screen points (for scissor clipping).
    #[must_use]
    pub fn to_points(self, rect: Rectangle) -> Rectangle {
        let scale = self.points_per_pixel();
        Rectangle::new(
            rect.x * scale,
            rect.y * scale,
            rect.width * scale,
            rect.height * scale,
        )
    }

    fn camera(self) -> Camera2D {
        Camera2D {
            offset: Vector2::zero(),
            target: Vector2::zero(),
            rotation: 0.0,
            zoom: self.pixel as f32,
        }
    }
}

/// While alive, drawing happens in canvas art pixels (see [`UiCanvas`]).
pub struct UiCanvasScope(());

impl UiCanvasScope {
    /// Starts drawing on the frame's canvas. The draw handle proves a frame is being drawn.
    #[must_use]
    pub fn begin(_draw: &mut RaylibDrawHandle<'_>) -> Self {
        // SAFETY: balanced by EndMode2D in `drop`, inside the same BeginDrawing/EndDrawing pass.
        unsafe { ffi::BeginMode2D(canvas().camera().into()) };
        Self(())
    }
}

impl Drop for UiCanvasScope {
    fn drop(&mut self) {
        // SAFETY: closes the BeginMode2D opened by `begin`.
        unsafe { ffi::EndMode2D() };
    }
}

/// Clips drawing to `rect` (canvas pixels) until [`end_clip`].
pub fn begin_clip(rect: Rectangle) {
    let points = canvas().to_points(rect);
    // SAFETY: scissor calls only set GPU state; callers balance them with `end_clip`.
    unsafe {
        ffi::BeginScissorMode(
            points.x.round() as i32,
            points.y.round() as i32,
            points.width.round() as i32,
            points.height.round() as i32,
        );
    }
}

/// Ends [`begin_clip`].
pub fn end_clip() {
    // SAFETY: closes the scissor region opened by `begin_clip`.
    unsafe { ffi::EndScissorMode() };
}

/// The pixel fonts the kit draws text with.
#[derive(Clone, Copy, Debug)]
pub struct UiFontFaces {
    pub regular: ffi::Font,
    pub bold: ffi::Font,
}

#[derive(Clone, Copy)]
struct FrameState {
    canvas: UiCanvas,
    fonts: Option<UiFontFaces>,
    seconds: f32,
}

thread_local! {
    static FRAME: Cell<FrameState> = const {
        Cell::new(FrameState {
            canvas: UiCanvas {
                pixel: 2,
                density: 1.0,
                width: 640,
                height: 360,
            },
            fonts: None,
            seconds: 0.0,
        })
    };
}

/// Sets this frame's canvas, fonts and animation clock (call once at the start of a frame).
pub fn begin_frame(canvas: UiCanvas, fonts: Option<UiFontFaces>, seconds: f32) {
    FRAME.with(|frame| {
        frame.set(FrameState {
            canvas,
            fonts,
            seconds,
        });
    });
}

/// The frame's canvas.
#[must_use]
pub fn canvas() -> UiCanvas {
    FRAME.with(|frame| frame.get().canvas)
}

/// Seconds since start, for UI animation (blinking cursors, warning pulses).
#[must_use]
pub fn seconds() -> f32 {
    FRAME.with(|frame| frame.get().seconds)
}

fn fonts() -> Option<UiFontFaces> {
    FRAME.with(|frame| frame.get().fonts)
}

/// Text faces and sizes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TextStyle {
    /// Regular face, 1x (cap height 7).
    Body,
    /// Bold face, 1x: labels, headings, buttons.
    Bold,
    /// Bold face, 2x: screen titles.
    Title,
}

impl TextStyle {
    /// Art pixels per glyph pixel.
    #[must_use]
    pub const fn scale(self) -> i32 {
        match self {
            Self::Body | Self::Bold => 1,
            Self::Title => 2,
        }
    }

    /// Glyph cell height (including descenders).
    #[must_use]
    pub const fn height(self) -> i32 {
        GLYPH_HEIGHT * self.scale()
    }

    /// Distance between wrapped lines.
    #[must_use]
    pub const fn line_height(self) -> i32 {
        self.height() + 2 * self.scale()
    }

    fn face(self) -> Option<ffi::Font> {
        let faces = fonts()?;
        Some(match self {
            Self::Body => faces.regular,
            Self::Bold | Self::Title => faces.bold,
        })
    }
}

/// Width of `text` in art pixels.
#[must_use]
pub fn text_width(text: &str, style: TextStyle) -> i32 {
    let (Some(font), Ok(cstring)) = (style.face(), CString::new(text)) else {
        return i32::try_from(text.chars().count()).unwrap_or(0) * 6 * style.scale();
    };
    let scale = style.scale() as f32;
    // SAFETY: measuring reads the font and a valid NUL-terminated string.
    let size =
        unsafe { ffi::MeasureTextEx(font, cstring.as_ptr(), GLYPH_HEIGHT as f32 * scale, scale) };
    size.x.round() as i32
}

/// Draws `text` with its top-left at `(x, y)` and a one-pixel ink drop shadow.
pub fn text(
    draw: &mut impl RaylibDraw,
    text: &str,
    x: i32,
    y: i32,
    style: TextStyle,
    color: Color,
) {
    text_shadowed(draw, text, x, y, style, color, Some(INK));
}

/// Draws `text` with an optional drop shadow colour.
pub fn text_shadowed(
    _draw: &mut impl RaylibDraw,
    text: &str,
    x: i32,
    y: i32,
    style: TextStyle,
    color: Color,
    shadow: Option<Color>,
) {
    let (Some(font), Ok(cstring)) = (style.face(), CString::new(text)) else {
        return;
    };
    let scale = style.scale() as f32;
    let size = GLYPH_HEIGHT as f32 * scale;
    // SAFETY: drawing reads the font and a valid NUL-terminated string during a frame.
    unsafe {
        if let Some(shadow) = shadow {
            let offset = scale.max(1.0);
            ffi::DrawTextEx(
                font,
                cstring.as_ptr(),
                Vector2::new(x as f32 + offset, y as f32 + offset),
                size,
                scale,
                palette::with_alpha(shadow, color.a.min(shadow.a)),
            );
        }
        ffi::DrawTextEx(
            font,
            cstring.as_ptr(),
            Vector2::new(x as f32, y as f32),
            size,
            scale,
            color,
        );
    }
}

/// Draws `text` right-aligned so it ends at `right`.
pub fn text_right(
    draw: &mut impl RaylibDraw,
    value: &str,
    right: i32,
    y: i32,
    style: TextStyle,
    color: Color,
) {
    text(
        draw,
        value,
        right - text_width(value, style),
        y,
        style,
        color,
    );
}

/// Draws `text` centred horizontally on `centre_x`.
pub fn text_centred(
    draw: &mut impl RaylibDraw,
    value: &str,
    centre_x: i32,
    y: i32,
    style: TextStyle,
    color: Color,
) {
    text(
        draw,
        value,
        centre_x - text_width(value, style) / 2,
        y,
        style,
        color,
    );
}

/// Splits `text` into lines no wider than `width` (words longer than a line are split).
#[must_use]
pub fn wrap(text: &str, width: i32, style: TextStyle) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if text_width(&candidate, style) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                word.clone_into(&mut line);
            }
        }
        lines.push(line);
    }
    lines
}

/// `text` cut to fit `width`, ending in "..." when shortened.
#[must_use]
pub fn truncate(text: &str, width: i32, style: TextStyle) -> String {
    if text_width(text, style) <= width {
        return text.to_owned();
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() {
        cut.pop();
        let candidate = format!("{}...", cut.trim_end());
        if text_width(&candidate, style) <= width {
            return candidate;
        }
    }
    String::new()
}

/// Draws wrapped `text` inside `width` from `(x, y)`; returns the height used.
pub fn paragraph(
    draw: &mut impl RaylibDraw,
    value: &str,
    x: i32,
    y: i32,
    width: i32,
    style: TextStyle,
    color: Color,
) -> i32 {
    let lines = wrap(value, width, style);
    for (index, line) in lines.iter().enumerate() {
        let row = i32::try_from(index).unwrap_or(0);
        text(draw, line, x, y + row * style.line_height(), style, color);
    }
    i32::try_from(lines.len()).unwrap_or(0) * style.line_height()
}

/// Panel frame styles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PanelStyle {
    /// Slate rim: HUD, lists, secondary panels.
    Standard,
    /// Amber rim: dialogs, focused or primary panels.
    Accent,
    /// Red rim on dark: warnings and failures.
    Warning,
}

impl PanelStyle {
    const fn slice(self) -> &'static NineSlice {
        match self {
            Self::Standard => &ui::PANEL_STANDARD,
            Self::Accent => &ui::PANEL_ACCENT,
            Self::Warning => &ui::PANEL_WARNING,
        }
    }
}

/// Inner padding of a panel's frame (content starts this far in).
pub const PANEL_PADDING: i32 = 6;

/// Draws a framed panel over `rect`.
pub fn panel(draw: &mut impl RaylibDraw, rect: Rectangle, style: PanelStyle) {
    sprite::draw_nine_slice(draw, style.slice(), rect, 1.0, Color::WHITE);
}

/// Draws a recessed well (a sunken area inside a panel).
pub fn well(draw: &mut impl RaylibDraw, rect: Rectangle) {
    sprite::draw_nine_slice(draw, &ui::WELL, rect, 1.0, Color::WHITE);
}

/// A horizontal divider line with a highlight below it.
pub fn divider(draw: &mut impl RaylibDraw, x: i32, y: i32, width: i32) {
    draw.draw_rectangle(x, y, width, 1, INK);
    draw.draw_rectangle(x, y + 1, width, 1, SLATE);
}

/// Dims everything behind a dialog.
pub fn dim(draw: &mut impl RaylibDraw, rect: Rectangle) {
    draw.draw_rectangle_rec(rect, palette::with_alpha(INK, 150));
}

/// Button / menu item states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonState {
    Idle,
    Selected,
    Disabled,
}

impl ButtonState {
    const fn slice(self) -> &'static NineSlice {
        match self {
            Self::Idle => &ui::BUTTON_IDLE,
            Self::Selected => &ui::BUTTON_SELECTED,
            Self::Disabled => &ui::BUTTON_DISABLED,
        }
    }

    /// Label colour on this state's face.
    #[must_use]
    pub const fn text_color(self) -> Color {
        match self {
            Self::Idle => BONE,
            Self::Selected => INK,
            Self::Disabled => FOG,
        }
    }

    const fn text_shadow(self) -> Color {
        match self {
            Self::Selected => CREAM,
            Self::Idle | Self::Disabled => INK,
        }
    }
}

/// Standard button height (one text line plus bevels).
pub const BUTTON_HEIGHT: i32 = 17;

/// Draws a button over `rect` with `label` left-aligned; the selected state adds the cursor.
pub fn button(draw: &mut impl RaylibDraw, rect: Rectangle, label: &str, state: ButtonState) {
    button_face(draw, rect, state);
    let y = rect.y as i32 + (rect.height as i32 - GLYPH_HEIGHT) / 2;
    text_shadowed(
        draw,
        label,
        rect.x as i32 + 8,
        y,
        TextStyle::Bold,
        state.text_color(),
        Some(state.text_shadow()),
    );
}

/// Draws a button face and, when selected, the cursor beside it.
pub fn button_face(draw: &mut impl RaylibDraw, rect: Rectangle, state: ButtonState) {
    sprite::draw_nine_slice(draw, state.slice(), rect, 1.0, Color::WHITE);
    if state == ButtonState::Selected {
        cursor(
            draw,
            rect.x as i32 - 7,
            rect.y as i32 + (rect.height as i32 - 7) / 2,
        );
    }
}

/// The animated selection chevron, top-left at `(x, y)` (5x7).
pub fn cursor(draw: &mut impl RaylibDraw, x: i32, y: i32) {
    let frame = ui::CURSOR.frame_at(seconds());
    icon_frame(draw, &ui::CURSOR, frame, x, y);
}

/// Draws `sprite` (frame 0) at canvas pixel `(x, y)`.
pub fn icon(draw: &mut impl RaylibDraw, sprite: &SpriteDef, x: i32, y: i32) {
    icon_frame(draw, sprite, 0, x, y);
}

/// Draws one frame of `sprite` at canvas pixel `(x, y)`.
pub fn icon_frame(draw: &mut impl RaylibDraw, sprite: &SpriteDef, frame: usize, x: i32, y: i32) {
    sprite::draw_sprite(
        draw,
        sprite,
        frame,
        Vector2::new(x as f32, y as f32),
        1.0,
        Color::WHITE,
    );
}

/// Three-tone colours of a bar fill: highlight row, body, shade row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarColors {
    pub light: Color,
    pub body: Color,
    pub shade: Color,
}

impl BarColors {
    pub const HULL: Self = Self {
        light: LIME,
        body: LEAF,
        shade: MOSS,
    };
    pub const FUEL: Self = Self {
        light: CREAM,
        body: GOLD,
        shade: AMBER,
    };
    pub const CARGO: Self = Self {
        light: SAND,
        body: CLAY,
        shade: LOAM,
    };
    pub const COOL: Self = Self {
        light: ICE,
        body: SKY_LIGHT,
        shade: SKY,
    };
    pub const DANGER: Self = Self {
        light: ORANGE,
        body: RED,
        shade: MAROON,
    };
}

/// Draws a framed bar over `rect` filled to `ratio` (0..=1), with quarter tick marks. Below
/// `danger_below` the fill switches to [`BarColors::DANGER`] and pulses.
pub fn bar(
    draw: &mut impl RaylibDraw,
    rect: Rectangle,
    ratio: f32,
    colors: BarColors,
    danger_below: f32,
) {
    let (x, y) = (rect.x as i32, rect.y as i32);
    let (width, height) = (rect.width as i32, rect.height as i32);
    if width < 4 || height < 3 {
        return;
    }
    draw.draw_rectangle(x, y, width, height, INK);
    let (inner_x, inner_y) = (x + 1, y + 1);
    let (inner_width, inner_height) = (width - 2, height - 2);
    draw.draw_rectangle(inner_x, inner_y, inner_width, inner_height, NIGHT);
    let ratio = ratio.clamp(0.0, 1.0);
    let danger = ratio < danger_below;
    let colors = if danger { BarColors::DANGER } else { colors };
    let filled = (inner_width as f32 * ratio).round() as i32;
    let filled = if ratio > 0.0 { filled.max(1) } else { 0 };
    if filled > 0 {
        let pulse_off = danger && (seconds() * 4.0) as i32 % 2 == 1;
        let body = if pulse_off { colors.shade } else { colors.body };
        draw.draw_rectangle(inner_x, inner_y, filled, inner_height, body);
        draw.draw_rectangle(inner_x, inner_y, filled, 1, colors.light);
        if inner_height > 2 {
            draw.draw_rectangle(inner_x, inner_y + inner_height - 1, filled, 1, colors.shade);
        }
    }
    for quarter in 1..4 {
        let tick_x = inner_x + inner_width * quarter / 4;
        draw.draw_rectangle(
            tick_x,
            inner_y + inner_height - 2,
            1,
            2,
            palette::with_alpha(INK, 160),
        );
    }
}

/// An icon, a label and a bar with `value/max` on one row `width` wide; returns the row height.
#[allow(
    clippy::too_many_arguments,
    reason = "one call draws one labelled stat row"
)]
pub fn stat_bar(
    draw: &mut impl RaylibDraw,
    x: i32,
    y: i32,
    width: i32,
    sprite: &SpriteDef,
    label: &str,
    value: f32,
    max: f32,
    colors: BarColors,
    danger_below: f32,
) -> i32 {
    icon(draw, sprite, x, y);
    let label_x = x + ICON_SIZE + 3;
    text(draw, label, label_x, y + 1, TextStyle::Bold, MIST);
    let numbers = format!("{value:.0}/{max:.0}");
    text_right(draw, &numbers, x + width, y + 1, TextStyle::Body, BONE);
    let bar_x = label_x + text_width("CARGO", TextStyle::Bold) + 4;
    let bar_width = (x + width - text_width(&numbers, TextStyle::Body) - 4 - bar_x).max(8);
    let ratio = if max <= f32::EPSILON {
        0.0
    } else {
        value / max
    };
    bar(
        draw,
        Rectangle::new(bar_x as f32, y as f32 + 1.0, bar_width as f32, 8.0),
        ratio,
        colors,
        danger_below,
    );
    ICON_SIZE + 3
}

/// One row of a shop / inventory list.
#[derive(Clone, Copy, Debug)]
pub struct ItemRow<'a> {
    pub icon: Option<&'static SpriteDef>,
    pub label: &'a str,
    /// Second line under the label (may be empty).
    pub detail: &'a str,
    /// Right-aligned price or value (with a coin when `coin`).
    pub price: &'a str,
    pub price_color: Color,
    pub coin: bool,
    pub state: ButtonState,
}

/// Height of an [`item_row`] with a detail line.
pub const ITEM_ROW_HEIGHT: i32 = 26;

/// Draws an item row over `rect`: button face, icon in a well, label and detail, price at the
/// right edge.
pub fn item_row(draw: &mut impl RaylibDraw, rect: Rectangle, row: &ItemRow<'_>) {
    button_face(draw, rect, row.state);
    let (x, y) = (rect.x as i32, rect.y as i32);
    let (width, height) = (rect.width as i32, rect.height as i32);
    let slot = (height - 8).clamp(ICON_SIZE + 4, 18);
    if let Some(sprite) = row.icon {
        let slot_y = y + (height - slot) / 2;
        well(
            draw,
            Rectangle::new((x + 4) as f32, slot_y as f32, slot as f32, slot as f32),
        );
        let icon_x = x + 4 + (slot - sprite.width()) / 2;
        let icon_y = slot_y + (slot - sprite.height()) / 2;
        icon(draw, sprite, icon_x, icon_y);
    }
    let text_x = if row.icon.is_some() {
        x + 4 + slot + 5
    } else {
        x + 8
    };
    let price_space = if row.price.is_empty() {
        0
    } else {
        text_width(row.price, TextStyle::Bold) + if row.coin { ICON_SIZE + 4 } else { 0 } + 8
    };
    let text_room = (x + width - 6 - price_space - text_x).max(0);
    let label = truncate(row.label, text_room, TextStyle::Bold);
    let detail = truncate(row.detail, text_room, TextStyle::Body);
    let color = row.state.text_color();
    let shadow = Some(row.state.text_shadow());
    let has_detail = !row.detail.is_empty();
    let label_y = if has_detail {
        y + (height - GLYPH_HEIGHT * 2 - 1) / 2
    } else {
        y + (height - GLYPH_HEIGHT) / 2
    };
    text_shadowed(
        draw,
        &label,
        text_x,
        label_y,
        TextStyle::Bold,
        color,
        shadow,
    );
    if has_detail {
        let detail_color = match row.state {
            ButtonState::Selected => SOIL,
            ButtonState::Idle => FOG,
            ButtonState::Disabled => SLATE_LIGHT,
        };
        text_shadowed(
            draw,
            &detail,
            text_x,
            label_y + GLYPH_HEIGHT + 1,
            TextStyle::Body,
            detail_color,
            None,
        );
    }
    if !row.price.is_empty() {
        let right = x + width - 6;
        let price_y = y + (height - GLYPH_HEIGHT) / 2;
        // Role colours read on the dark faces; the amber selected face takes ink, and a disabled
        // row keeps red (why it is unavailable) but greys every other tag.
        let price_color = match row.state {
            ButtonState::Selected => INK,
            ButtonState::Disabled if row.price_color != RED => FOG,
            _ => row.price_color,
        };
        text_shadowed(
            draw,
            row.price,
            right - text_width(row.price, TextStyle::Bold),
            price_y,
            TextStyle::Bold,
            price_color,
            shadow,
        );
        if row.coin {
            let coin_x = right - text_width(row.price, TextStyle::Bold) - ICON_SIZE - 2;
            icon(
                draw,
                &super::pixel_art::sprites::icons::CREDITS,
                coin_x,
                price_y,
            );
        }
    }
}

/// Draws a row of tabs across the top of `rect`; returns the tab strip height.
pub fn tabs(draw: &mut impl RaylibDraw, rect: Rectangle, labels: &[&str], selected: usize) -> i32 {
    const TAB_HEIGHT: i32 = 17;
    let mut x = rect.x as i32 + 4;
    let y = rect.y as i32;
    for (index, label) in labels.iter().enumerate() {
        let active = index == selected;
        let width = text_width(label, TextStyle::Bold) + 14;
        let slice = if active {
            &ui::TAB_ACTIVE
        } else {
            &ui::TAB_IDLE
        };
        let top = if active { y } else { y + 2 };
        sprite::draw_nine_slice(
            draw,
            slice,
            Rectangle::new(
                x as f32,
                top as f32,
                width as f32,
                (y + TAB_HEIGHT - top) as f32,
            ),
            1.0,
            Color::WHITE,
        );
        text(
            draw,
            label,
            x + 7,
            top + 3,
            TextStyle::Bold,
            if active { GOLD } else { FOG },
        );
        x += width + 1;
    }
    TAB_HEIGHT
}

/// Draws a speech bubble / tooltip whose tail points down at `(anchor_x, anchor_y)`, wrapping
/// `text` to `max_width`. The box is kept inside `bounds`.
pub fn tooltip(
    draw: &mut impl RaylibDraw,
    value: &str,
    anchor_x: i32,
    anchor_y: i32,
    max_width: i32,
    bounds: Rectangle,
) {
    let lines = wrap(value, max_width - 10, TextStyle::Body);
    let line_count = i32::try_from(lines.len()).unwrap_or(1);
    let text_wide = lines
        .iter()
        .map(|line| text_width(line, TextStyle::Body))
        .max()
        .unwrap_or(0);
    let width = text_wide + 10;
    let height = line_count * TextStyle::Body.line_height() + 6;
    let tail_height = ui::TOOLTIP_TAIL.height();
    let min_x = bounds.x as i32 + 2;
    let max_x = (bounds.x + bounds.width) as i32 - width - 2;
    let x = (anchor_x - width / 2).clamp(min_x, max_x.max(min_x));
    let y = (anchor_y - tail_height - height + 1).max(bounds.y as i32 + 2);
    sprite::draw_nine_slice(
        draw,
        &ui::TOOLTIP,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        1.0,
        Color::WHITE,
    );
    let tail_x = (anchor_x - ui::TOOLTIP_TAIL.width() / 2).clamp(x + 2, x + width - 8);
    icon(draw, &ui::TOOLTIP_TAIL, tail_x, y + height - 1);
    for (index, line) in lines.iter().enumerate() {
        let row = i32::try_from(index).unwrap_or(0);
        text_shadowed(
            draw,
            line,
            x + 5,
            y + 4 + row * TextStyle::Body.line_height(),
            TextStyle::Body,
            INK,
            None,
        );
    }
}

/// A compact badge: icon plus label in a warning panel. Returns its width.
pub fn badge(draw: &mut impl RaylibDraw, x: i32, y: i32, sprite: &SpriteDef, label: &str) -> i32 {
    let width = ICON_SIZE + text_width(label, TextStyle::Bold) + 14;
    panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, 17.0),
        PanelStyle::Warning,
    );
    let frame = sprite.frame_at(seconds());
    icon_frame(draw, sprite, frame, x + 4, y + 4);
    let blink = (seconds() * 3.0) as i32 % 2 == 0;
    text(
        draw,
        label,
        x + 4 + ICON_SIZE + 3,
        y + 4,
        TextStyle::Bold,
        if blink { GOLD } else { ORANGE },
    );
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_is_reference_sized_in_whole_pixels() {
        let canvas = UiCanvas::for_framebuffer(1280, 720, 1.0);
        assert_eq!((canvas.pixel, canvas.width, canvas.height), (2, 640, 360));
        let retina = UiCanvas::for_framebuffer(2560, 1440, 2.0);
        assert_eq!((retina.pixel, retina.width, retina.height), (4, 640, 360));
        assert!((retina.points_per_pixel() - 2.0).abs() < f32::EPSILON);
        let full_hd = UiCanvas::for_framebuffer(1920, 1080, 1.0);
        assert_eq!((full_hd.width, full_hd.height), (640, 360));
        let small = UiCanvas::for_framebuffer(960, 540, 1.0);
        assert_eq!((small.width, small.height), (480, 270));
    }

    #[test]
    fn viewports_map_to_whole_canvas_pixels_inside_them() {
        let canvas = UiCanvas::for_framebuffer(1920, 1080, 1.0);
        let right_half = canvas.viewport_rect(Viewport::new(960, 0, 960, 1080));
        assert_eq!(right_half, Rectangle::new(320.0, 0.0, 320.0, 360.0));
        let odd = canvas.viewport_rect(Viewport::new(641, 1, 640, 1079));
        assert!(odd.x * 3.0 >= 641.0 && (odd.x + odd.width) * 3.0 <= 1281.0);
        assert!(odd.x.fract().abs() < f32::EPSILON);
        let points = canvas.to_points(right_half);
        assert_eq!(points, Rectangle::new(960.0, 0.0, 960.0, 1080.0));
    }

    #[test]
    fn truncation_fits_and_marks_the_cut() {
        assert_eq!(truncate("short", 100, TextStyle::Body), "short");
        let cut = truncate("a rather long line of text", 60, TextStyle::Body);
        assert!(cut.ends_with("...") && text_width(&cut, TextStyle::Body) <= 60);
    }

    #[test]
    fn wrapping_keeps_every_word() {
        let lines = wrap("one two three four five", 30, TextStyle::Body);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "one two three four five");
        assert_eq!(wrap("", 30, TextStyle::Body), vec![String::new()]);
    }
}
