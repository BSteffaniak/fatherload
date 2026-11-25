//! Terrain drawing: the world is cut into chunks of 32x32 tiles, each one point-filtered texture
//! at art-pixel resolution (16x16 art pixels per tile, drawn at 2 world pixels per art pixel).
//!
//! A chunk keeps its tile art as palette indices on the CPU ([`tile_art`]); a terrain change
//! repaints just the changed tile and its neighbours (their edges depend on it). Exploration only
//! changes the darkness ([`fog`]), which is recomputed and composited over the cached art. Chunks
//! are only created and painted once they come near a view, and dropped (textures freed) once
//! every view is far away, so a new world costs nothing up front and an endless (streamed) world
//! keeps a bounded number of textures.

use std::collections::{BTreeMap, BTreeSet};

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

use super::pixel_art::{
    shade::{PaletteIndex, TRANSPARENT, color_at},
    sprite,
};

pub mod fog;
pub mod tile_art;

use fog::DarknessGrid;
use tile_art::TILE_ART;

pub(super) const TERRAIN_CHUNK_SIZE_TILES: i32 = 32;
/// Chunks within this many world pixels of a view are painted ahead of time (one per frame).
const PAINT_MARGIN_WORLD: f32 = 512.0;
/// Chunks further than this many world pixels from every view are dropped.
const DROP_MARGIN_WORLD: f32 = 2_048.0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ChunkPosition {
    x: i32,
    y: i32,
}

pub(super) struct TerrainRenderer {
    chunks: BTreeMap<ChunkPosition, TerrainChunk>,
    /// The terrain's columns (first, last), first row and height the chunks were cut for.
    bounds: (i32, i32, i32, i32),
}

/// What a chunk must redo before it is drawn.
#[derive(Clone, Debug, Default)]
struct ChunkWork {
    /// Repaint every tile.
    full_paint: bool,
    /// Repaint these tiles.
    tiles: BTreeSet<TilePosition>,
    /// Recompute the darkness grid.
    fog: bool,
}

impl ChunkWork {
    fn is_empty(&self) -> bool {
        !self.full_paint && self.tiles.is_empty() && !self.fog
    }
}

struct TerrainChunk {
    texture: Option<Texture2D>,
    tile_min_x: i32,
    tile_min_y: i32,
    tile_max_x: i32,
    tile_max_y: i32,
    /// Size in art pixels.
    art_width: i32,
    art_height: i32,
    /// Painted tile art, row-major palette indices.
    art: Vec<PaletteIndex>,
    darkness: DarknessGrid,
    work: ChunkWork,
    /// Whether the texture shows anything yet.
    painted: bool,
    /// Per tile (row-major), whether it was last painted as open ground (air or fluid).
    open: Vec<bool>,
}

impl TerrainRenderer {
    pub(super) const fn new(
        _raylib: &mut RaylibHandle,
        _thread: &RaylibThread,
        terrain: &Terrain,
    ) -> Self {
        Self {
            chunks: BTreeMap::new(),
            bounds: terrain_bounds(terrain),
        }
    }

    /// A cache for `terrain` (a place's ground, F11; no GPU work until it is synced).
    pub(super) const fn new_for(terrain: &Terrain) -> Self {
        Self {
            chunks: BTreeMap::new(),
            bounds: terrain_bounds(terrain),
        }
    }

    pub(super) fn mark_all_dirty(&mut self) {
        for chunk in self.chunks.values_mut() {
            chunk.work.full_paint = true;
            chunk.work.fog = true;
        }
    }

    /// Tile `position` changed: repaint it and its neighbours (whose edges and shadows depend on
    /// it, and whose hazard tell-tales reach [`tile_art::TELLTALE_RADIUS`] tiles).
    ///
    /// A tile painted open that is still open (`now_open`: air or fluid, which paint the same
    /// back wall; the fluid itself is drawn live over the terrain) needs nothing: a flood
    /// changes thousands of fluid levels without repainting a tile.
    pub(super) fn mark_tile_dirty(&mut self, position: TilePosition, now_open: bool) {
        if now_open
            && self
                .chunk_for_tile_mut(position)
                .is_some_and(|chunk| chunk.painted_open(position))
        {
            return;
        }
        let reach = tile_art::TELLTALE_RADIUS.max(1);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let tile = TilePosition {
                    x: position.x + dx,
                    y: position.y + dy,
                };
                if let Some(chunk) = self.chunk_for_tile_mut(tile) {
                    chunk.work.tiles.insert(tile);
                }
            }
        }
    }

    /// Exploration near tile `position` changed: recompute darkness of its chunk.
    pub(super) fn mark_fog_dirty(&mut self, position: TilePosition) {
        if let Some(chunk) = self.chunk_for_tile_mut(position) {
            chunk.work.fog = true;
        }
    }

    /// The chunk holding `position`, if it exists (chunks not yet created are painted whole
    /// when they are).
    fn chunk_for_tile_mut(&mut self, position: TilePosition) -> Option<&mut TerrainChunk> {
        let (first, last, top, height) = self.bounds;
        if position.x < first || position.y < top || position.x > last || position.y >= height {
            return None;
        }
        self.chunks.get_mut(&chunk_position_for_tile(position))
    }

    /// Brings the chunks near `views` (world-pixel rectangles) up to date.
    pub(super) fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        terrain: &Terrain,
        explored: &DiscoveredTiles,
        views: &[Rectangle],
    ) {
        if self.bounds != terrain_bounds(terrain) {
            self.chunks.clear();
            self.bounds = terrain_bounds(terrain);
        }
        let grow = |view: &Rectangle, margin: f32| {
            Rectangle::new(
                view.x - margin,
                view.y - margin,
                margin.mul_add(2.0, view.width),
                margin.mul_add(2.0, view.height),
            )
        };
        let near: Vec<Rectangle> = views
            .iter()
            .map(|view| grow(view, PAINT_MARGIN_WORLD))
            .collect();
        let keep: Vec<Rectangle> = views
            .iter()
            .map(|view| grow(view, DROP_MARGIN_WORLD))
            .collect();
        self.chunks
            .retain(|_, chunk| keep.iter().any(|view| chunk.overlaps(*view)));
        for view in &near {
            self.create_chunks_in(raylib, thread, terrain, *view);
        }
        // Chunks on screen are always brought up to date; a full paint of an off-screen chunk
        // near a view costs a few milliseconds, so only one is painted ahead per frame.
        let mut ahead_budget = 1;
        for chunk in self.chunks.values_mut() {
            if chunk.work.is_empty() {
                continue;
            }
            let on_screen = views.iter().any(|view| chunk.overlaps(*view));
            let cheap = chunk.painted && !chunk.work.full_paint;
            if !on_screen {
                if !near.iter().any(|view| chunk.overlaps(*view)) {
                    continue;
                }
                if !cheap {
                    if ahead_budget == 0 {
                        continue;
                    }
                    ahead_budget -= 1;
                }
            }
            chunk.update(raylib, thread, terrain, explored);
        }
    }

    /// Draws the chunks overlapping `visible_world` (world pixels).
    pub(super) fn draw(&self, draw: &mut impl RaylibDraw, visible_world: Rectangle) {
        for chunk in self.chunks.values() {
            if chunk.painted && chunk.overlaps(visible_world) {
                chunk.draw(draw);
            }
        }
    }

    /// Creates (unpainted) every missing chunk overlapping `view` (world pixels).
    fn create_chunks_in(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        terrain: &Terrain,
        view: Rectangle,
    ) {
        let (first, last, top, height) = self.bounds;
        let frame = crate::view_frame::frame();
        let (top_row, bottom_row) = (
            frame.row(view.y).max(top),
            frame.row(view.y + view.height).min(height - 1),
        );
        if bottom_row < top_row {
            // All sky (or all below the floor): no terrain to cut.
            return;
        }
        let min = chunk_position_for_tile(TilePosition {
            x: frame.col(view.x).max(first),
            y: top_row,
        });
        let max = chunk_position_for_tile(TilePosition {
            x: frame.col(view.x + view.width).min(last),
            y: bottom_row,
        });
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let position = ChunkPosition { x, y };
                self.chunks
                    .entry(position)
                    .or_insert_with(|| TerrainChunk::new(raylib, thread, position, terrain));
            }
        }
    }
}

/// The columns (first, last), first row and height of `terrain`.
const fn terrain_bounds(terrain: &Terrain) -> (i32, i32, i32, i32) {
    (
        terrain.min_x(),
        terrain.max_x(),
        terrain.min_y(),
        terrain.height(),
    )
}

impl TerrainChunk {
    fn new(
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        position: ChunkPosition,
        terrain: &Terrain,
    ) -> Self {
        let tile_min_x = (position.x * TERRAIN_CHUNK_SIZE_TILES).max(terrain.min_x());
        let tile_min_y = (position.y * TERRAIN_CHUNK_SIZE_TILES).max(terrain.min_y());
        let tile_max_x = (position.x * TERRAIN_CHUNK_SIZE_TILES + TERRAIN_CHUNK_SIZE_TILES)
            .min(terrain.max_x() + 1);
        let tile_max_y = (tile_min_y + TERRAIN_CHUNK_SIZE_TILES).min(terrain.height());
        let art_width = (tile_max_x - tile_min_x) * TILE_ART;
        let art_height = (tile_max_y - tile_min_y) * TILE_ART;
        let texture = raylib
            .load_texture_from_image(thread, &sprite::blank_image(art_width, art_height))
            .ok();
        if let Some(texture) = &texture {
            texture.set_texture_filter(thread, TextureFilter::TEXTURE_FILTER_POINT);
        }
        Self {
            texture,
            tile_min_x,
            tile_min_y,
            tile_max_x,
            tile_max_y,
            art_width,
            art_height,
            art: vec![TRANSPARENT; (art_width * art_height) as usize],
            darkness: DarknessGrid::default(),
            work: ChunkWork {
                full_paint: true,
                tiles: BTreeSet::new(),
                fog: true,
            },
            painted: false,
            open: vec![
                false;
                ((tile_max_x - tile_min_x) * (tile_max_y - tile_min_y)).max(0) as usize
            ],
        }
    }

    /// Index of `tile` in [`Self::open`], if it is inside the chunk.
    fn tile_index(&self, tile: TilePosition) -> Option<usize> {
        if tile.x < self.tile_min_x
            || tile.x >= self.tile_max_x
            || tile.y < self.tile_min_y
            || tile.y >= self.tile_max_y
        {
            return None;
        }
        let columns = self.tile_max_x - self.tile_min_x;
        let (row, column) = (tile.y - self.tile_min_y, tile.x - self.tile_min_x);
        usize::try_from(row * columns + column).ok()
    }

    /// Whether `tile` was painted as open ground and no repaint is pending for it.
    fn painted_open(&self, tile: TilePosition) -> bool {
        self.painted
            && !self.work.full_paint
            && !self.work.tiles.contains(&tile)
            && self
                .tile_index(tile)
                .and_then(|index| self.open.get(index))
                .copied()
                .unwrap_or(false)
    }

    fn update(
        &mut self,
        _raylib: &mut RaylibHandle,
        _thread: &RaylibThread,
        terrain: &Terrain,
        explored: &DiscoveredTiles,
    ) {
        let work = std::mem::take(&mut self.work);
        let stride = self.art_width as usize;
        if work.full_paint {
            for tile_y in self.tile_min_y..self.tile_max_y {
                for tile_x in self.tile_min_x..self.tile_max_x {
                    self.paint(
                        terrain,
                        TilePosition {
                            x: tile_x,
                            y: tile_y,
                        },
                        stride,
                    );
                }
            }
        } else {
            for tile in &work.tiles {
                self.paint(terrain, *tile, stride);
            }
        }
        if work.fog || self.darkness == DarknessGrid::default() {
            self.darkness = DarknessGrid::compute(
                explored,
                TilePosition {
                    x: self.tile_min_x,
                    y: self.tile_min_y,
                },
                (
                    self.tile_max_x - self.tile_min_x,
                    self.tile_max_y - self.tile_min_y,
                ),
                terrain.height(),
                |x| terrain.surface_row(x),
            );
        }
        let pixels = self.composite();
        if let Some(texture) = &mut self.texture
            && texture.update_texture(&pixels).is_ok()
        {
            self.painted = true;
        }
    }

    fn paint(&mut self, terrain: &Terrain, tile: TilePosition, stride: usize) {
        if tile.x < self.tile_min_x
            || tile.x >= self.tile_max_x
            || tile.y < self.tile_min_y
            || tile.y >= self.tile_max_y
        {
            return;
        }
        let origin = (
            ((tile.x - self.tile_min_x) * TILE_ART) as usize,
            ((tile.y - self.tile_min_y) * TILE_ART) as usize,
        );
        tile_art::paint_tile(terrain, tile, &mut self.art, stride, origin);
        let open = terrain.tile(tile).is_some_and(|found| found.kind.is_open());
        if let Some(slot) = self
            .tile_index(tile)
            .and_then(|index| self.open.get_mut(index))
        {
            *slot = open;
        }
    }

    /// The art seen through the darkness, as RGBA bytes.
    fn composite(&self) -> Vec<u8> {
        let mut pixels = vec![0; self.art.len() * 4];
        let (origin_x, origin_y) = (self.tile_min_x * TILE_ART, self.tile_min_y * TILE_ART);
        for y in 0..self.art_height {
            for x in 0..self.art_width {
                let offset = (y * self.art_width + x) as usize;
                let index = self.art[offset];
                if index == TRANSPARENT {
                    continue;
                }
                let (gx, gy) = (origin_x + x, origin_y + y);
                let darkness = self.darkness.at_art_pixel(gx, gy);
                let color = color_at(fog::apply(index, darkness, gx, gy));
                pixels[offset * 4..offset * 4 + 4]
                    .copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
        }
        pixels
    }

    fn draw(&self, draw: &mut impl RaylibDraw) {
        let Some(texture) = &self.texture else {
            return;
        };
        let world_x = crate::view_frame::frame().rel_col(self.tile_min_x) * TILE_SIZE;
        let world_y = crate::view_frame::frame().rel_row(self.tile_min_y) * TILE_SIZE;
        let scale = TILE_SIZE / TILE_ART as f32;
        draw.draw_texture_pro(
            texture,
            Rectangle::new(0.0, 0.0, self.art_width as f32, self.art_height as f32),
            Rectangle::new(
                world_x,
                world_y,
                self.art_width as f32 * scale,
                self.art_height as f32 * scale,
            ),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
    }

    fn overlaps(&self, world: Rectangle) -> bool {
        rects_overlap(
            Rectangle::new(
                crate::view_frame::frame().rel_col(self.tile_min_x) * TILE_SIZE,
                crate::view_frame::frame().rel_row(self.tile_min_y) * TILE_SIZE,
                (self.tile_max_x - self.tile_min_x) as f32 * TILE_SIZE,
                (self.tile_max_y - self.tile_min_y) as f32 * TILE_SIZE,
            ),
            world,
        )
    }
}

/// Whether two rectangles touch or overlap (edges count, so a chunk on the border still draws).
fn rects_overlap(a: Rectangle, b: Rectangle) -> bool {
    a.x + a.width >= b.x && a.x <= b.x + b.width && a.y + a.height >= b.y && a.y <= b.y + b.height
}

const fn chunk_position_for_tile(position: TilePosition) -> ChunkPosition {
    ChunkPosition {
        x: position.x.div_euclid(TERRAIN_CHUNK_SIZE_TILES),
        y: position.y.div_euclid(TERRAIN_CHUNK_SIZE_TILES),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_culling_uses_the_visible_world_rectangle() {
        let chunk = Rectangle::new(1_000.0, 500.0, 256.0, 256.0);
        // 1920x1080 at 1.5x shows 1280x720 world pixels; a 2560x1080 ultrawide shows ~1707.
        assert!(!rects_overlap(
            chunk,
            Rectangle::new(-400.0, 0.0, 1280.0, 720.0)
        ));
        assert!(rects_overlap(
            chunk,
            Rectangle::new(-400.0, 0.0, 1706.7, 720.0)
        ));
        assert!(rects_overlap(
            chunk,
            Rectangle::new(1_256.0, 756.0, 10.0, 10.0)
        ));
        assert!(!rects_overlap(
            chunk,
            Rectangle::new(1_000.0, 757.0, 10.0, 10.0)
        ));
    }
}
