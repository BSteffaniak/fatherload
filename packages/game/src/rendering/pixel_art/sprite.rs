//! Char-grid sprites: definitions, the startup atlas texture and integer-scale drawing.
//!
//! A [`SpriteDef`] is plain `const` data: a legend mapping characters to palette colours and one
//! or more frames of equal-sized rows (`.` and space are transparent). At startup every sprite in
//! [`super::sprites::ALL`] is rasterized into one atlas texture with point filtering. Drawing
//! looks the sprite up by name and blits it at a whole-number scale, so art pixels stay square
//! and crisp.

use std::{cell::Cell, collections::BTreeMap, sync::OnceLock};

use raylib::{ffi, prelude::*};

use super::palette::CLEAR;

/// Atlas texture width; sprites are shelf-packed into rows this wide.
const ATLAS_WIDTH: i32 = 512;
/// Transparent gap around every packed frame.
const ATLAS_PADDING: i32 = 1;

/// A sprite authored as character grids.
#[derive(Debug, PartialEq)]
pub struct SpriteDef {
    /// Unique name (atlas key).
    pub name: &'static str,
    /// Character -> colour. `.` and ` ` are always transparent and need no entry.
    pub legend: &'static [(char, Color)],
    /// Animation frames; every frame has the same rows count and row width.
    pub frames: &'static [&'static [&'static str]],
    /// Seconds each frame shows when animated (ignored for single-frame sprites).
    pub frame_seconds: f32,
}

impl SpriteDef {
    /// Width in art pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        self.frames
            .first()
            .and_then(|rows| rows.first())
            .map_or(0, |row| i32::try_from(row.chars().count()).unwrap_or(0))
    }

    /// Height in art pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        self.frames
            .first()
            .map_or(0, |rows| i32::try_from(rows.len()).unwrap_or(0))
    }

    /// The frame to show `seconds` into a looping animation.
    #[must_use]
    pub fn frame_at(&self, seconds: f32) -> usize {
        let count = self.frames.len().max(1);
        if count == 1 || self.frame_seconds <= f32::EPSILON {
            return 0;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "animation step counts are small and non-negative"
        )]
        let step = (seconds.max(0.0) / self.frame_seconds) as usize;
        step % count
    }

    /// The colour of grid character `symbol` (`None` for an unknown character).
    #[must_use]
    pub fn color_of(&self, symbol: char) -> Option<Color> {
        if symbol == '.' || symbol == ' ' {
            return Some(CLEAR);
        }
        self.legend
            .iter()
            .find(|(key, _)| *key == symbol)
            .map(|(_, color)| *color)
    }
}

/// A frame that stretches to any size: fixed `border`-pixel corners, edges stretched along one
/// axis and the centre stretched both ways. Author edges and centre so they tile when stretched
/// (constant along the stretch axis).
#[derive(Debug)]
pub struct NineSlice {
    pub sprite: &'static SpriteDef,
    /// Corner size in art pixels.
    pub border: i32,
}

/// Where one sprite frame lives in the atlas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl AtlasRect {
    fn source(self) -> Rectangle {
        Rectangle::new(
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
        )
    }
}

/// The packed position of every sprite frame.
#[derive(Debug)]
pub struct AtlasLayout {
    pub width: i32,
    pub height: i32,
    frames: BTreeMap<&'static str, Vec<AtlasRect>>,
}

impl AtlasLayout {
    /// Shelf-packs `sprites` in order.
    #[must_use]
    pub fn pack(sprites: &[&'static SpriteDef]) -> Self {
        let mut frames = BTreeMap::new();
        let (mut x, mut y, mut shelf_height) = (ATLAS_PADDING, ATLAS_PADDING, 0);
        for sprite in sprites {
            let (width, height) = (sprite.width(), sprite.height());
            let rects = (0..sprite.frames.len())
                .map(|_| {
                    if x + width + ATLAS_PADDING > ATLAS_WIDTH {
                        x = ATLAS_PADDING;
                        y += shelf_height + ATLAS_PADDING;
                        shelf_height = 0;
                    }
                    let rect = AtlasRect {
                        x,
                        y,
                        width,
                        height,
                    };
                    x += width + ATLAS_PADDING;
                    shelf_height = shelf_height.max(height);
                    rect
                })
                .collect();
            frames.insert(sprite.name, rects);
        }
        Self {
            width: ATLAS_WIDTH,
            height: i32::try_from(
                u32::try_from(y + shelf_height + ATLAS_PADDING)
                    .unwrap_or(1)
                    .next_power_of_two(),
            )
            .unwrap_or(i32::MAX),
            frames,
        }
    }

    /// Frame `frame` of the sprite called `name` (wrapping around the frame count).
    #[must_use]
    pub fn frame(&self, name: &str, frame: usize) -> Option<AtlasRect> {
        let rects = self.frames.get(name)?;
        rects.get(frame % rects.len().max(1)).copied()
    }
}

/// The atlas layout of [`super::sprites::ALL`], computed once.
pub fn layout() -> &'static AtlasLayout {
    static LAYOUT: OnceLock<AtlasLayout> = OnceLock::new();
    LAYOUT.get_or_init(|| AtlasLayout::pack(super::sprites::ALL))
}

/// Rasterizes every sprite into one point-filtered texture.
pub fn load_atlas(raylib: &mut RaylibHandle, thread: &RaylibThread) -> Option<Texture2D> {
    let layout = layout();
    let mut image = blank_image(layout.width, layout.height);
    for sprite in super::sprites::ALL {
        for (index, rows) in sprite.frames.iter().enumerate() {
            let Some(rect) = layout.frame(sprite.name, index) else {
                continue;
            };
            for (row_index, row) in rows.iter().enumerate() {
                for (column, symbol) in row.chars().enumerate() {
                    let color = sprite.color_of(symbol).unwrap_or(CLEAR);
                    if color.a == 0 {
                        continue;
                    }
                    let px = rect.x + i32::try_from(column).unwrap_or(0);
                    let py = rect.y + i32::try_from(row_index).unwrap_or(0);
                    image.draw_pixel(px, py, color);
                }
            }
        }
    }
    match raylib.load_texture_from_image(thread, &image) {
        Ok(texture) => {
            texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
            Some(texture)
        }
        Err(error) => {
            eprintln!("Failed to build the sprite atlas: {error}");
            None
        }
    }
}

/// A fully transparent CPU image to rasterize art into.
pub fn blank_image(width: i32, height: i32) -> Image {
    // SAFETY: GenImageColor allocates a fresh image that the returned wrapper owns and frees.
    unsafe { Image::from_raw(ffi::GenImageColor(width, height, CLEAR)) }
}

thread_local! {
    static CURRENT_ATLAS: Cell<Option<ffi::Texture2D>> = const { Cell::new(None) };
}

/// Makes `texture` the atlas the draw helpers sample this frame.
pub fn set_current_atlas(texture: Option<&Texture2D>) {
    CURRENT_ATLAS.with(|current| current.set(texture.map(|texture| **texture)));
}

fn current_atlas() -> Option<ffi::Texture2D> {
    CURRENT_ATLAS.with(Cell::get)
}

/// Blits atlas `source` into `dest`, mirrored horizontally when `flip_x`.
fn blit(source: Rectangle, dest: Rectangle, flip_x: bool, tint: Color) {
    let Some(texture) = current_atlas() else {
        return;
    };
    let source = if flip_x {
        Rectangle::new(source.x, source.y, -source.width, source.height)
    } else {
        source
    };
    // SAFETY: only called between BeginDrawing/EndDrawing by the draw helpers below, with the
    // atlas texture owned by the renderer for the whole frame.
    unsafe {
        ffi::DrawTexturePro(texture, source, dest, Vector2::zero(), 0.0, tint);
    }
}

/// Draws frame `frame` of `sprite` with its top-left at `(x, y)`, each art pixel `scale` units
/// wide. Use whole-number scales (and positions on the pixel grid) to keep pixels crisp.
pub fn draw_sprite(
    draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    position: Vector2,
    scale: f32,
    tint: Color,
) {
    draw_sprite_flipped(draw, sprite, frame, position, scale, false, tint);
}

/// [`draw_sprite`], optionally mirrored horizontally (e.g. a rig facing left).
pub fn draw_sprite_flipped(
    _draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    position: Vector2,
    scale: f32,
    flip_x: bool,
    tint: Color,
) {
    let Some(rect) = layout().frame(sprite.name, frame) else {
        return;
    };
    let dest = Rectangle::new(
        position.x,
        position.y,
        rect.width as f32 * scale,
        rect.height as f32 * scale,
    );
    blit(rect.source(), dest, flip_x, tint);
}

/// Draws `slice` stretched over `dest`; corners are `border * scale` units.
pub fn draw_nine_slice(
    _draw: &mut impl RaylibDraw,
    slice: &NineSlice,
    dest: Rectangle,
    scale: f32,
    tint: Color,
) {
    let Some(rect) = layout().frame(slice.sprite.name, 0) else {
        return;
    };
    let border = slice.border as f32;
    let corner = border * scale;
    let (sx, sy) = (rect.x as f32, rect.y as f32);
    let (sw, sh) = (rect.width as f32, rect.height as f32);
    let source_columns = [
        (sx, border),
        (sx + border, sw - border * 2.0),
        (sx + sw - border, border),
    ];
    let source_rows = [
        (sy, border),
        (sy + border, sh - border * 2.0),
        (sy + sh - border, border),
    ];
    let inner_width = (dest.width - corner * 2.0).max(0.0);
    let inner_height = (dest.height - corner * 2.0).max(0.0);
    let dest_columns = [
        (dest.x, corner),
        (dest.x + corner, inner_width),
        (dest.x + corner + inner_width, corner),
    ];
    let dest_rows = [
        (dest.y, corner),
        (dest.y + corner, inner_height),
        (dest.y + corner + inner_height, corner),
    ];
    for (row, (source_y, source_height)) in source_rows.iter().enumerate() {
        for (column, (source_x, source_width)) in source_columns.iter().enumerate() {
            let (dest_x, dest_width) = dest_columns[column];
            let (dest_y, dest_height) = dest_rows[row];
            if dest_width <= 0.0 || dest_height <= 0.0 {
                continue;
            }
            blit(
                Rectangle::new(*source_x, *source_y, *source_width, *source_height),
                Rectangle::new(dest_x, dest_y, dest_width, dest_height),
                false,
                tint,
            );
        }
    }
}

/// Tiles frame `frame` of `sprite` over `dest` (whole tiles from the top-left, clipped at the
/// right and bottom edges), each art pixel `scale` units.
pub fn draw_tiled(
    _draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    dest: Rectangle,
    scale: f32,
    tint: Color,
) {
    let Some(rect) = layout().frame(sprite.name, frame) else {
        return;
    };
    let tile_width = rect.width as f32 * scale;
    let tile_height = rect.height as f32 * scale;
    if tile_width <= 0.0 || tile_height <= 0.0 {
        return;
    }
    let rows = (dest.height / tile_height).ceil() as i32;
    let columns = (dest.width / tile_width).ceil() as i32;
    for row in 0..rows {
        let y = (row as f32).mul_add(tile_height, dest.y);
        let visible_height = (dest.y + dest.height - y).min(tile_height);
        for column in 0..columns {
            let x = (column as f32).mul_add(tile_width, dest.x);
            let visible_width = (dest.x + dest.width - x).min(tile_width);
            let source = Rectangle::new(
                rect.x as f32,
                rect.y as f32,
                visible_width / scale,
                visible_height / scale,
            );
            blit(
                source,
                Rectangle::new(x, y, visible_width, visible_height),
                false,
                tint,
            );
        }
    }
}
