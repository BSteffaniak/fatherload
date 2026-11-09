//! Everything drawn in world space: the sky and town behind the terrain, the terrain itself, and
//! the rigs, props and effects in front of it. All of it is pixel art on the world art grid (one
//! art pixel = [`WORLD_PIXELS_PER_ART_PIXEL`] world pixels).

#![allow(
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "procedural art mixes small hashed integers and short coordinate names"
)]

use raylib::prelude::*;

use drillgame_simulation::{
    DiscoveredTiles, DrillState, FallingBoulder, HazardCloud, PlacedBomb, PlacedInfrastructure,
    terrain::{TILE_SIZE, Terrain},
};

use super::{
    pixel_art::{
        palette,
        sprite::{self, SpriteDef},
        sprites::world as props,
    },
    terrain::TerrainRenderer,
    ui_kit::{self, TextStyle},
};
use crate::{
    effects::{Explosion, SparkParticle},
    pixel_scale::{WORLD_PIXELS_PER_ART_PIXEL, snap_to_pixel_grid},
    render_snapshot::PlayerPresentation,
};

mod buildings;
mod fx;
mod rig;
mod sky;

pub use rig::{RigFacings, player_accent};
pub use sky::SkyArt;

/// World pixels per art pixel.
const ART: f32 = WORLD_PIXELS_PER_ART_PIXEL;
/// World y of the ground line (top of the first ground row).
const GROUND_Y: f32 = 5.0 * TILE_SIZE;

/// The camera that draws world pixel `camera` at the origin, `zoom` physical pixels per world
/// pixel. The target is snapped to the physical pixel grid so pixel art never shimmers while the
/// camera scrolls (see [`crate::pixel_scale`]).
pub(super) fn world_camera(camera: Vector2, zoom: f32) -> Camera2D {
    let snapped_camera = Vector2::new(
        snap_to_pixel_grid(camera.x, zoom),
        snap_to_pixel_grid(camera.y, zoom),
    );
    Camera2D {
        offset: Vector2::zero(),
        target: snapped_camera,
        rotation: 0.0,
        zoom,
    }
}

/// Everything one view draws in world space this frame.
pub(super) struct WorldScene<'a> {
    /// The world rectangle the view shows (world pixels).
    pub visible: Rectangle,
    pub terrain_renderer: &'a TerrainRenderer,
    pub terrain: &'a Terrain,
    pub sky: &'a SkyArt,
    pub infrastructure: &'a [PlacedInfrastructure],
    pub hazards: &'a [HazardCloud],
    pub bombs: &'a [PlacedBomb],
    pub boulders: &'a [FallingBoulder],
    pub sparks: &'a [SparkParticle],
    pub explosions: &'a [Explosion],
    pub players: &'a [PlayerPresentation],
    /// Each rig's last facing.
    pub facings: &'a RigFacings,
    /// The rig this view follows.
    pub local: Option<&'a PlayerPresentation>,
    pub active_drill: Option<DrillState>,
    pub lost_cargo: Option<(f32, f32)>,
    /// Tiles this view's player has discovered (glows never give away hidden lava).
    pub explored: Option<&'a DiscoveredTiles>,
    /// Shell ticks (60 per second); drives every animation deterministically.
    pub ticks: u64,
}

impl WorldScene<'_> {
    fn seconds(&self) -> f32 {
        self.ticks as f32 / 60.0
    }
}

/// Draws one view's world: backdrop, town, terrain, props, rigs and effects, back to front.
pub(super) fn draw_world_scene(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>) {
    let seconds = scene.seconds();
    sky::draw_sky(draw, scene.sky, scene.visible, seconds);
    buildings::draw_town(draw, scene.visible, scene.local, seconds);
    draw_world_edges(draw, scene.terrain, scene.visible);
    scene.terrain_renderer.draw(draw, scene.visible);
    fx::draw_lava_glow(draw, scene.terrain, scene.explored, scene.visible, seconds);
    fx::draw_infrastructure(draw, scene.infrastructure, seconds);
    if let Some(drill) = scene.active_drill {
        fx::draw_drill_damage(draw, scene.terrain, drill, scene.ticks);
    }
    if let Some((x, y)) = scene.lost_cargo {
        fx::draw_lost_cargo(draw, x, y, seconds);
    }
    for bomb in scene.bombs {
        fx::draw_bomb(draw, bomb, seconds);
    }
    for boulder in scene.boulders {
        fx::draw_boulder(draw, boulder, seconds);
    }
    if let Some(local) = scene.local {
        fx::draw_scanner_marks(draw, scene.terrain, local, seconds);
        fx::draw_poi_scanner_marks(draw, scene.terrain, local, seconds);
    }
    for player in scene.players {
        rig::draw_rig(draw, scene.terrain, player, scene.facings, scene.ticks);
    }
    if let Some(drill) = scene.active_drill {
        fx::draw_drill_debris(draw, scene.terrain, drill, scene.ticks);
    }
    for cloud in scene.hazards {
        fx::draw_gas_cloud(draw, cloud, seconds);
    }
    fx::draw_sparks(draw, scene.sparks);
    for explosion in scene.explosions {
        fx::draw_explosion(draw, explosion);
    }
    for player in scene.players {
        let is_local = scene
            .local
            .is_some_and(|local| local.player_id == player.player_id);
        if !is_local {
            rig::draw_remote_tag(draw, player, scene.ticks);
        }
    }
}

/// Solid bedrock beyond the left and right edges of the world, below the ground line.
fn draw_world_edges(draw: &mut impl RaylibDraw, terrain: &Terrain, visible: Rectangle) {
    let world_right = terrain.width() as f32 * TILE_SIZE;
    let top = GROUND_Y.max(visible.y);
    let bottom = visible.y + visible.height + ART;
    if bottom <= top {
        return;
    }
    let tile = 8.0 * ART;
    let fill = |draw: &mut dyn FnMut(Rectangle), from: f32, to: f32| {
        // Anchor the pattern to the world grid so it does not swim while scrolling.
        let start = (from / tile).floor() * tile;
        let rows_top = ((top - GROUND_Y) / tile).floor().mul_add(tile, GROUND_Y);
        draw(Rectangle::new(
            start,
            rows_top,
            to - start,
            bottom - rows_top,
        ));
    };
    if visible.x < 0.0 {
        fill(
            &mut |rect| {
                sprite::draw_tiled(draw, &props::BEDROCK, 0, rect, ART, Color::WHITE);
            },
            visible.x - ART,
            0.0,
        );
        draw.draw_rectangle_rec(Rectangle::new(-ART, top, ART, bottom - top), palette::INK);
    }
    let right = visible.x + visible.width + ART;
    if right > world_right {
        fill(
            &mut |rect| {
                sprite::draw_tiled(draw, &props::BEDROCK, 0, rect, ART, Color::WHITE);
            },
            world_right,
            right,
        );
        draw.draw_rectangle_rec(
            Rectangle::new(world_right, top, ART, bottom - top),
            palette::INK,
        );
    }
}

/// Snaps a world coordinate to the art-pixel grid (whole physical pixels at any snapped zoom).
fn snap(value: f32) -> f32 {
    (value / ART).round() * ART
}

/// Draws `sprite` frame `frame` with its top-left at world `(x, y)`, one art pixel per
/// [`WORLD_PIXELS_PER_ART_PIXEL`] world pixels, snapped to the art-pixel grid.
fn draw_world_sprite(
    draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    x: f32,
    y: f32,
    flip: bool,
) {
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        frame,
        Vector2::new(snap(x), snap(y)),
        ART,
        flip,
        Color::WHITE,
    );
}

/// Draws in art pixels relative to an origin snapped to the art grid.
#[derive(Clone, Copy, Debug)]
struct Pen {
    x: f32,
    y: f32,
}

impl Pen {
    fn at(x: f32, y: f32) -> Self {
        Self {
            x: snap(x),
            y: snap(y),
        }
    }

    /// A `w`x`h` art-pixel rectangle at art `(x, y)`.
    fn rect(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        draw.draw_rectangle_rec(
            Rectangle::new(
                (x as f32).mul_add(ART, self.x),
                (y as f32).mul_add(ART, self.y),
                w as f32 * ART,
                h as f32 * ART,
            ),
            color,
        );
    }

    fn px(self, draw: &mut impl RaylibDraw, x: i32, y: i32, color: Color) {
        self.rect(draw, x, y, 1, 1, color);
    }

    /// A filled box with a one-pixel `INK` outline.
    fn boxed(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, fill: Color) {
        self.rect(draw, x, y, w, h, palette::INK);
        self.rect(draw, x + 1, y + 1, w - 2, h - 2, fill);
    }

    fn sprite(self, draw: &mut impl RaylibDraw, sprite: &SpriteDef, frame: usize, x: i32, y: i32) {
        draw_world_sprite(
            draw,
            sprite,
            frame,
            (x as f32).mul_add(ART, self.x),
            (y as f32).mul_add(ART, self.y),
            false,
        );
    }

    /// Bold pixel text (one font pixel per art pixel) with its top-left at art `(x, y)`.
    fn text(self, draw: &mut impl RaylibDraw, text: &str, x: i32, y: i32, color: Color) {
        ui_kit::text_shadowed(
            draw,
            text,
            (x as f32).mul_add(ART, self.x) as i32,
            (y as f32).mul_add(ART, self.y) as i32,
            TextStyle::Title,
            color,
            None,
        );
    }
}

/// Width of `text` drawn by [`Pen::text`], in art pixels.
fn text_art_width(text: &str) -> i32 {
    (ui_kit::text_width(text, TextStyle::Title) as f32 / ART).round() as i32
}
