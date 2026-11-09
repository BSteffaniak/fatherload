//! Terrain drawing: the world is cut into chunks of 32x32 tiles, each one point-filtered texture
//! at art-pixel resolution (16x16 art pixels per tile, drawn at 2 world pixels per art pixel).
//!
//! A chunk keeps its tile art as palette indices on the CPU ([`tile_art`]); a terrain change
//! repaints just the changed tile and its neighbours (their edges depend on it). Exploration only
//! changes the darkness ([`fog`]), which is recomputed and composited over the cached art. Chunks
//! are only painted once they come near a view, so a new world costs nothing up front.

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ChunkPosition {
    x: i32,
    y: i32,
}

pub(super) struct TerrainRenderer {
    chunks: BTreeMap<ChunkPosition, TerrainChunk>,
    terrain_width: i32,
    terrain_height: i32,
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
}

impl TerrainRenderer {
    pub(super) fn new(raylib: &mut RaylibHandle, thread: &RaylibThread, terrain: &Terrain) -> Self {
        let mut renderer = Self {
            chunks: BTreeMap::new(),
            terrain_width: terrain.width(),
            terrain_height: terrain.height(),
        };
        renderer.recreate_chunks(raylib, thread, terrain);
        renderer
    }

    pub(super) fn mark_all_dirty(&mut self) {
        for chunk in self.chunks.values_mut() {
            chunk.work.full_paint = true;
            chunk.work.fog = true;
        }
    }

    /// Tile `position` changed: repaint it and its neighbours (whose edges and shadows depend on
    /// it, and whose hazard tell-tales reach [`tile_art::TELLTALE_RADIUS`] tiles).
    pub(super) fn mark_tile_dirty(&mut self, position: TilePosition) {
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

    fn chunk_for_tile_mut(&mut self, position: TilePosition) -> Option<&mut TerrainChunk> {
        if position.x < 0
            || position.y < 0
            || position.x >= self.terrain_width
            || position.y >= self.terrain_height
        {
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
        if self.terrain_width != terrain.width() || self.terrain_height != terrain.height() {
            self.recreate_chunks(raylib, thread, terrain);
        }
        let near: Vec<Rectangle> = views
            .iter()
            .map(|view| {
                Rectangle::new(
                    view.x - PAINT_MARGIN_WORLD,
                    view.y - PAINT_MARGIN_WORLD,
                    PAINT_MARGIN_WORLD.mul_add(2.0, view.width),
                    PAINT_MARGIN_WORLD.mul_add(2.0, view.height),
                )
            })
            .collect();
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

    fn recreate_chunks(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        terrain: &Terrain,
    ) {
        self.chunks.clear();
        self.terrain_width = terrain.width();
        self.terrain_height = terrain.height();
        for y in 0..chunk_count(self.terrain_height) {
            for x in 0..chunk_count(self.terrain_width) {
                let position = ChunkPosition { x, y };
                self.chunks.insert(
                    position,
                    TerrainChunk::new(raylib, thread, position, terrain),
                );
            }
        }
    }
}

impl TerrainChunk {
    fn new(
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        position: ChunkPosition,
        terrain: &Terrain,
    ) -> Self {
        let tile_min_x = position.x * TERRAIN_CHUNK_SIZE_TILES;
        let tile_min_y = position.y * TERRAIN_CHUNK_SIZE_TILES;
        let tile_max_x = (tile_min_x + TERRAIN_CHUNK_SIZE_TILES).min(terrain.width());
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
        }
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
        let world_x = self.tile_min_x as f32 * TILE_SIZE;
        let world_y = self.tile_min_y as f32 * TILE_SIZE;
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
                self.tile_min_x as f32 * TILE_SIZE,
                self.tile_min_y as f32 * TILE_SIZE,
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

const fn chunk_count(tile_count: i32) -> i32 {
    (tile_count + TERRAIN_CHUNK_SIZE_TILES - 1) / TERRAIN_CHUNK_SIZE_TILES
}

const fn chunk_position_for_tile(position: TilePosition) -> ChunkPosition {
    ChunkPosition {
        x: position.x / TERRAIN_CHUNK_SIZE_TILES,
        y: position.y / TERRAIN_CHUNK_SIZE_TILES,
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
