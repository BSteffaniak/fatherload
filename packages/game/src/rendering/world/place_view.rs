//! The place view (Part III F11): what a seat away from the home store sees, the ground of a
//! space body or free flight, in that store's own frame.
//!
//! A stub hook: the space-render track (TODO-SEAM w11-space-render) replaces the backdrop, the
//! polar ground and the flight HUD. What it draws today is enough to play and read: a star
//! backdrop, the body's ground with the game's own tile art (one terrain cache per store, lit
//! round the seat's rigs), the rigs and pilots there. The HUD is the seat's usual one.

use std::collections::{BTreeMap, BTreeSet};

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    place::{Place, StoreKey},
    terrain::{TILE_SIZE, TilePosition},
};

use super::{RigFacings, pilot, rig};
use crate::{
    render_snapshot::{PlaceScene, RenderSnapshot, ViewSnapshot},
    rendering::{pixel_art::palette, terrain::TerrainRenderer},
};

/// Tiles lit round every rig in a place (the place's own exploration, presentation only).
const LIT_RADIUS_TILES: i32 = 14;

/// One store's terrain cache and the tiles lit so far.
struct PlaceCache {
    terrain: TerrainRenderer,
    lit: DiscoveredTiles,
}

/// The terrain caches of the stores local seats are in.
#[derive(Default)]
pub struct PlaceRenderers {
    stores: BTreeMap<StoreKey, PlaceCache>,
}

impl PlaceRenderers {
    /// Brings the caches of the stores this frame's views are in up to date (and drops the
    /// others): lights the ground round their rigs and repaints changed tiles.
    pub fn sync(
        &mut self,
        raylib: &mut RaylibHandle,
        thread: &RaylibThread,
        snapshot: &RenderSnapshot<'_>,
        changed: &[(StoreKey, TilePosition)],
    ) {
        let shown: BTreeSet<StoreKey> = snapshot
            .views
            .iter()
            .filter_map(|view| view.place_scene.as_ref().map(|scene| scene.store))
            .collect();
        self.stores.retain(|store, _| shown.contains(store));
        for view in &snapshot.views {
            let Some(scene) = &view.place_scene else {
                continue;
            };
            let cache = self
                .stores
                .entry(scene.store)
                .or_insert_with(|| PlaceCache {
                    terrain: TerrainRenderer::new_for(scene.terrain),
                    lit: DiscoveredTiles::default(),
                });
            for (store, tile) in changed {
                if *store == scene.store {
                    let open = scene
                        .terrain
                        .tile(*tile)
                        .is_some_and(|found| found.kind.is_open());
                    cache.terrain.mark_tile_dirty(*tile, open);
                }
            }
            for rig in &scene.rigs {
                let centre = scene.frame.tile_at(rig.x, rig.y);
                let mut fresh = false;
                for dy in -LIT_RADIUS_TILES..=LIT_RADIUS_TILES {
                    for dx in -LIT_RADIUS_TILES..=LIT_RADIUS_TILES {
                        if dx * dx + dy * dy <= LIT_RADIUS_TILES * LIT_RADIUS_TILES {
                            fresh |= cache.lit.insert(TilePosition {
                                x: centre.x + dx,
                                y: centre.y + dy,
                            });
                        }
                    }
                }
                if fresh {
                    for (dx, dy) in [(0, 0), (-16, -16), (16, -16), (-16, 16), (16, 16)] {
                        cache.terrain.mark_fog_dirty(TilePosition {
                            x: centre.x + dx,
                            y: centre.y + dy,
                        });
                    }
                }
            }
            let (width, height) = view.visible_world_size();
            let visible = Rectangle::new(view.camera.x, view.camera.y, width, height);
            // The cache paints in the store's frame.
            let home = crate::view_frame::frame();
            let _ = crate::view_frame::set(scene.frame);
            cache
                .terrain
                .sync(raylib, thread, scene.terrain, &cache.lit, &[visible]);
            let _ = crate::view_frame::set(home);
        }
    }

    /// Draws `view`'s place: the backdrop, the ground, the rigs and pilots (in world space, the
    /// camera already set for the store's frame).
    pub fn draw(
        &self,
        draw: &mut impl RaylibDraw,
        view: &ViewSnapshot<'_>,
        scene: &PlaceScene<'_>,
        facings: &RigFacings,
        ticks: u64,
    ) {
        let (width, height) = view.visible_world_size();
        let visible = Rectangle::new(view.camera.x, view.camera.y, width, height);
        draw_backdrop(draw, scene, visible);
        if let Some(cache) = self.stores.get(&scene.store) {
            cache.terrain.draw(draw, visible);
        }
        draw_drops(draw, scene, visible);
        pilot::draw_pilots(draw, &scene.pilots, visible, ticks);
        for rig in &scene.rigs {
            if rig.x < visible.x - TILE_SIZE * 3.0
                || rig.x > visible.x + visible.width + TILE_SIZE * 3.0
                || rig.y < visible.y - TILE_SIZE * 3.0
                || rig.y > visible.y + visible.height + TILE_SIZE * 3.0
            {
                continue;
            }
            rig::draw_rig(draw, scene.terrain, rig, facings, ticks);
        }
    }
}

/// The sky of a place: the black of space with its stars (fixed to the absolute sky, so they
/// never jump when the frame moves), a dim horizon glow over a body's ground.
fn draw_backdrop(draw: &mut impl RaylibDraw, scene: &PlaceScene<'_>, visible: Rectangle) {
    draw.draw_rectangle_rec(visible, palette::INK);
    let on_body = matches!(scene.place, Place::OnBody(_));
    if on_body {
        // The thin air over a body glows above its horizon (the ground line is row 0 for the
        // stub generator; the planet track draws its own).
        let horizon = scene.frame.row_px(0);
        let glow = TILE_SIZE * 24.0;
        let top = (horizon - glow).max(visible.y);
        let bottom = horizon.min(visible.y + visible.height);
        if bottom > top {
            draw.draw_rectangle_gradient_v(
                visible.x as i32,
                top as i32,
                visible.width as i32 + 1,
                (bottom - top) as i32 + 1,
                palette::INK,
                palette::PLUM,
            );
        }
    }
    // Stars: one per 6-tile cell, hashed from the absolute cell.
    let cell = TILE_SIZE * 6.0;
    let (origin_x, origin_y) = (scene.frame.origin_col() / 6, scene.frame.origin_row() / 6);
    let first_x = (visible.x / cell).floor() as i64;
    let first_y = (visible.y / cell).floor() as i64;
    let columns = (visible.width / cell).ceil() as i64 + 1;
    let rows = (visible.height / cell).ceil() as i64 + 1;
    for row in first_y..first_y + rows {
        for column in first_x..first_x + columns {
            let hash = star_hash(column + origin_x, row + origin_y);
            if !hash.is_multiple_of(5) {
                continue;
            }
            let x = (column as f32).mul_add(cell, ((hash >> 8) % 96) as f32 * 2.0);
            let y = (row as f32).mul_add(cell, ((hash >> 16) % 96) as f32 * 2.0);
            let size = if (hash >> 24).trailing_zeros() >= 3 { 4.0 } else { 2.0 };
            let colour = match hash >> 28 & 3 {
                0 => palette::CREAM,
                1 => palette::ICE,
                2 => palette::MIST,
                _ => palette::FOG,
            };
            draw.draw_rectangle_v(Vector2::new(x, y), Vector2::new(size, size), colour);
        }
    }
}

/// Loose drops on a body's ground: small glinting nuggets.
fn draw_drops(draw: &mut impl RaylibDraw, scene: &PlaceScene<'_>, visible: Rectangle) {
    use fatherload_simulation::entity::EntityKind;
    for entity in scene.entities.of_kind(EntityKind::Drop) {
        let (x, y) = (entity.body.x, entity.body.y);
        if x < visible.x
            || x > visible.x + visible.width
            || y < visible.y
            || y > visible.y + visible.height
        {
            continue;
        }
        draw.draw_rectangle_v(
            Vector2::new(x - 5.0, y - 5.0),
            Vector2::new(10.0, 10.0),
            palette::RUST,
        );
        draw.draw_rectangle_v(
            Vector2::new(x - 3.0, y - 3.0),
            Vector2::new(6.0, 6.0),
            palette::GOLD,
        );
        draw.draw_rectangle_v(
            Vector2::new(x - 3.0, y - 3.0),
            Vector2::new(2.0, 2.0),
            palette::CREAM,
        );
    }
}

const fn star_hash(x: i64, y: i64) -> u64 {
    let mut z = (x.cast_unsigned()).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y.cast_unsigned()).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
