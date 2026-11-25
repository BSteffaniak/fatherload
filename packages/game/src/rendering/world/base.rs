//! Player bases in the world (WP-BASE): back walls, fixtures (doors, lift ladders, lamps,
//! decorations), modules (claim beacon, storage chest, crew quarters, generator, fuel station,
//! repair bay, cargo uplink, homestead), claim boundaries and the build-mode ghost.
//!
//! Solid pieces are terrain tiles (painted by `terrain::tile_art::built_art`); everything here is
//! drawn on top of the terrain from art-pixel rectangles, one tile = 16 art pixels, animated only
//! from the shell clock. Modules show their state: a station's reserve gauge and power light,
//! the generator's core glow, the uplink's beacon, the homestead's chimney smoke and, while a
//! local rig is inside it, its furnished interior.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "art pixels are small integers drawn at float scales; one function per module"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, PlayerId,
    base::{
        Bases, BuildRefusal, Buildable, BuiltTile, CLAIM_RADIUS, Claim, FUEL_STATION_CAPACITY,
        Facing, Fixture, ModuleKind, PlacedModule, StructureId,
    },
    item::BlockKind,
    terrain::{TILE_SIZE, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::{base_view::BuildPreview, render_snapshot::PlayerPresentation};

use super::{ART, fx::disc, snap};

/// Draws art-pixel rectangles from an origin, `scale` target units per art pixel (the world's
/// art grid, or a UI icon's).
#[derive(Clone, Copy, Debug)]
struct Pen {
    x: f32,
    y: f32,
    scale: f32,
}

impl Pen {
    /// On the world art grid at world pixel `(x, y)` (snapped).
    fn at(x: f32, y: f32) -> Self {
        Self {
            x: snap(x),
            y: snap(y),
            scale: ART,
        }
    }

    /// At `(x, y)` with `scale` units per art pixel (UI icons).
    const fn at_exact(x: f32, y: f32, scale: f32) -> Self {
        Self { x, y, scale }
    }

    fn rect(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 || color.a == 0 {
            return;
        }
        draw.draw_rectangle_rec(
            Rectangle::new(
                (x as f32).mul_add(self.scale, self.x),
                (y as f32).mul_add(self.scale, self.y),
                w as f32 * self.scale,
                h as f32 * self.scale,
            ),
            color,
        );
    }
}

/// Art pixels per tile.
const T: i32 = 16;

/// What the base layer of one view draws this frame.
pub(in crate::rendering) struct BaseScene<'a> {
    pub bases: &'a Bases,
    /// Back walls hide behind solid tiles.
    pub terrain: &'a fatherload_simulation::terrain::Terrain,
    pub powered: &'a std::collections::BTreeSet<StructureId>,
    pub players: &'a [PlayerPresentation],
    pub local: Option<&'a PlayerPresentation>,
    pub explored: Option<&'a DiscoveredTiles>,
    pub visible: Rectangle,
    pub seconds: f32,
}

impl BaseScene<'_> {
    /// The visible tile range `(min, max)`, one tile of margin.
    fn tile_range(&self) -> (TilePosition, TilePosition) {
        let v = self.visible;
        (
            TilePosition {
                x: crate::view_frame::frame().col(v.x) - 1,
                y: crate::view_frame::frame().row(v.y) - 1,
            },
            TilePosition {
                x: crate::view_frame::frame().col_ceil(v.x + v.width) + 1,
                y: crate::view_frame::frame().row_ceil(v.y + v.height) + 1,
            },
        )
    }

    fn seen(&self, tile: TilePosition) -> bool {
        self.explored.is_none_or(|explored| explored.contains(tile))
    }

    fn module_visible(&self, module: &PlacedModule) -> bool {
        let (min, max) = self.tile_range();
        let (w, h) = module.kind.footprint();
        module.origin.x + w >= min.x
            && module.origin.x <= max.x
            && module.origin.y + h >= min.y
            && module.origin.y <= max.y
            && module.tiles().any(|tile| self.seen(tile))
    }

    /// The identity colour of `player` (their paint), for banners and claim flags.
    fn accent(&self, player: PlayerId) -> Color {
        super::rig::player_accent(
            player,
            self.players
                .iter()
                .find(|rig| rig.player_id == player)
                .map(|rig| rig.cosmetics),
        )
    }

    /// Whether a rig is within `reach` tiles of tile `tile`.
    fn rig_near(&self, tile: TilePosition, reach: f32) -> bool {
        let (cx, cy) = (
            (crate::view_frame::frame().rel_col(tile.x) + 0.5) * TILE_SIZE,
            (crate::view_frame::frame().rel_row(tile.y) + 0.5) * TILE_SIZE,
        );
        self.players.iter().any(|rig| {
            (rig.x - cx).abs() <= reach * TILE_SIZE && (rig.y - cy).abs() <= reach * TILE_SIZE
        })
    }
}

fn tile_pen(tile: TilePosition) -> Pen {
    Pen::at(
        crate::view_frame::frame().rel_col(tile.x) * TILE_SIZE,
        crate::view_frame::frame().rel_row(tile.y) * TILE_SIZE,
    )
}

/// Back walls: drawn right after the terrain, behind everything else.
pub(in crate::rendering) fn draw_back_walls(draw: &mut impl RaylibDraw, scene: &BaseScene<'_>) {
    let (min, max) = scene.tile_range();
    for (tile, fixtures) in scene.bases.fixtures_in(min, max) {
        if let Some(block) = fixtures.back_wall
            && scene.seen(tile)
            && !scene.terrain.is_solid_at(tile)
        {
            back_wall(draw, tile_pen(tile), block, 255);
        }
    }
}

/// Masonry colours of a back wall: fill, mortar, highlight (darker than a solid wall: it is in
/// the shade behind).
const fn back_wall_colors(block: BlockKind) -> (Color, Color, Color) {
    match block {
        BlockKind::Dirt => (SOIL_DARK, NIGHT, SOIL),
        BlockKind::Clay => (MAROON, SOIL_DARK, RUST),
        BlockKind::Stone => (STONE_DARK, NIGHT, STONE),
        BlockKind::HardRock => (SLATE_DARK, INK, SLATE),
    }
}

fn back_wall(draw: &mut impl RaylibDraw, pen: Pen, block: BlockKind, alpha: u8) {
    let (fill, mortar, light) = back_wall_colors(block);
    let (fill, mortar, light) = (
        with_alpha(fill, alpha),
        with_alpha(mortar, alpha),
        with_alpha(light, alpha),
    );
    pen.rect(draw, 0, 0, T, T, fill);
    let (course, length) = match block {
        BlockKind::Dirt => (5, 16),
        BlockKind::Clay => (4, 8),
        BlockKind::Stone => (8, 8),
        BlockKind::HardRock => (8, 16),
    };
    let mut y = 0;
    let mut row = 0;
    while y < T {
        pen.rect(draw, 0, y, T, 1, light);
        pen.rect(draw, 0, (y + course - 1).min(T - 1), T, 1, mortar);
        if block != BlockKind::Dirt {
            let offset = if row % 2 == 0 { 0 } else { length / 2 };
            let mut x = length - 1 - offset;
            while x < T {
                if x >= 0 {
                    pen.rect(draw, x, y, 1, course, mortar);
                }
                x += length;
            }
        }
        y += course;
        row += 1;
    }
}

/// Fixtures and modules: drawn after the world's props, before rigs.
pub(in crate::rendering) fn draw_structures(draw: &mut impl RaylibDraw, scene: &BaseScene<'_>) {
    let (min, max) = scene.tile_range();
    for module in scene.bases.modules() {
        if scene.module_visible(module) {
            draw_module(draw, scene, module);
        }
    }
    for (tile, fixtures) in scene.bases.fixtures_in(min, max) {
        if let Some((fixture, facing)) = fixtures.front
            && scene.seen(tile)
        {
            let owner = scene.bases.claim_at(tile).map(|claim| claim.owner);
            let open = fixture == Fixture::Door && scene.rig_near(tile, 1.3);
            let color = owner.map_or(GOLD, |owner| scene.accent(owner));
            draw_fixture(draw, tile_pen(tile), fixture, facing, open, color, 255);
        }
    }
}

/// Lamp glows and lit windows: drawn over rigs so they light the scene.
pub(in crate::rendering) fn draw_base_lights(draw: &mut impl RaylibDraw, scene: &BaseScene<'_>) {
    let (min, max) = scene.tile_range();
    let flicker = ((scene.seconds * 7.0).sin() * 0.5 + (scene.seconds * 13.0).sin() * 0.5) * 0.08;
    for (tile, fixtures) in scene.bases.fixtures_in(min, max) {
        if let Some((Fixture::Light, facing)) = fixtures.front
            && scene.seen(tile)
        {
            let (lx, ly) = lamp_bulb(facing);
            let (x, y) = crate::view_frame::frame().tile_origin(tile);
            let (x, y) = (x + lx as f32 * ART, y + ly as f32 * ART);
            let glow = 1.0 + flicker;
            disc(draw, x, y, (34.0 * glow) as i32, with_alpha(AMBER, 14));
            disc(draw, x, y, (22.0 * glow) as i32, with_alpha(GOLD, 20));
            disc(draw, x, y, 10, with_alpha(CREAM, 34));
        }
    }
    for module in scene.bases.modules() {
        if !scene.module_visible(module) {
            continue;
        }
        let powered = scene.powered.contains(&module.id);
        let (w, h) = module.kind.footprint();
        let cx = (crate::view_frame::frame().rel_col(module.origin.x) + w as f32 * 0.5) * TILE_SIZE;
        let cy = (crate::view_frame::frame().rel_row(module.origin.y) + h as f32 * 0.5) * TILE_SIZE;
        match module.kind {
            ModuleKind::Beacon => {
                let pulse = ((scene.seconds * 2.0).sin() * 0.5 + 0.5) * 30.0;
                disc(
                    draw,
                    (crate::view_frame::frame().rel_col(module.origin.x) + 0.5) * TILE_SIZE,
                    crate::view_frame::frame().rel_row(module.origin.y) * TILE_SIZE + 4.0 * ART,
                    12 + pulse as i32 / 6,
                    with_alpha(scene.accent(module.owner), 26),
                );
            }
            ModuleKind::Homestead => {
                disc(draw, cx, cy + TILE_SIZE, 40, with_alpha(AMBER, 12));
            }
            ModuleKind::Generator | ModuleKind::FuelStation | ModuleKind::RepairBay
                if powered || module.kind == ModuleKind::Generator =>
            {
                disc(draw, cx, cy, 26, with_alpha(MINT, 10));
            }
            ModuleKind::Refinery if powered && !module.storage.is_empty() => {
                // The furnace mouth glows while ore cooks.
                let throb = ((scene.seconds * 3.0).sin() * 0.5 + 0.5) * 8.0;
                disc(
                    draw,
                    cx,
                    cy + TILE_SIZE * 0.6,
                    24 + throb as i32,
                    with_alpha(ORANGE, 22),
                );
            }
            _ => {}
        }
    }
}

/// Where a lamp's bulb sits in its tile (art pixels) for a mount facing.
const fn lamp_bulb(facing: Facing) -> (i32, i32) {
    match facing {
        // Hangs from the ceiling.
        Facing::Up => (8, 9),
        // Stands on the floor.
        Facing::Down => (8, 4),
        // Sconce on the left wall, shining right.
        Facing::Left => (4, 6),
        Facing::Right => (12, 6),
    }
}

/// One front fixture at `pen` (`alpha` < 255 for the ghost).
#[allow(
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    reason = "short art names; a fixture's whole look in one call"
)]
fn draw_fixture(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    fixture: Fixture,
    facing: Facing,
    open: bool,
    accent: Color,
    alpha: u8,
) {
    let a = |color: Color| with_alpha(color, alpha);
    let flip = facing == Facing::Left;
    let fx = |x: i32, w: i32| if flip { T - x - w } else { x };
    match fixture {
        Fixture::Door => {
            // Frame.
            pen.rect(draw, 0, 0, 2, T, a(INK));
            pen.rect(draw, T - 2, 0, 2, T, a(INK));
            pen.rect(draw, 0, 0, T, 2, a(INK));
            pen.rect(draw, 1, 1, 1, T - 1, a(STEEL));
            pen.rect(draw, T - 2, 1, 1, T - 1, a(STEEL_DARK));
            pen.rect(draw, 1, 1, T - 2, 1, a(STEEL_LIGHT));
            if open {
                // Slid into the frame: a sliver of panel and a lit threshold.
                pen.rect(draw, fx(2, 3), 2, 3, T - 2, a(STEEL_DARK));
                pen.rect(draw, 2, T - 1, T - 4, 1, a(GOLD));
            } else {
                pen.rect(draw, 2, 2, T - 4, T - 2, a(STEEL));
                pen.rect(draw, 2, 2, T - 4, 1, a(STEEL_LIGHT));
                pen.rect(draw, 7, 2, 1, T - 2, a(STEEL_DARK));
                pen.rect(draw, 3, 5, 3, 4, a(SKY));
                pen.rect(draw, 9, 5, 3, 4, a(SKY));
                pen.rect(draw, fx(10, 2), 10, 2, 1, a(GOLD));
            }
        }
        Fixture::Ladder => {
            pen.rect(draw, 2, 0, 2, T, a(STEEL_DARK));
            pen.rect(draw, 12, 0, 2, T, a(STEEL_DARK));
            pen.rect(draw, 3, 0, 1, T, a(STEEL));
            pen.rect(draw, 13, 0, 1, T, a(STEEL));
            for y in [2, 6, 10, 14] {
                pen.rect(draw, 4, y, 8, 1, a(STEEL_LIGHT));
                pen.rect(draw, 4, y + 1, 8, 1, a(SLATE_DARK));
            }
            // The lift strip.
            pen.rect(draw, 7, 0, 2, T, a(with_alpha(TEAL, 160)));
        }
        Fixture::Rope => {
            // A braided hemp rope, twists alternating light and dark, with a knot per tile.
            pen.rect(draw, 7, 0, 2, T, a(SOIL));
            for y in (0..T).step_by(2) {
                pen.rect(draw, 7, y, 1, 1, a(SAND));
                pen.rect(draw, 8, y + 1, 1, 1, a(LOAM));
            }
            pen.rect(draw, 6, 9, 4, 3, a(LOAM));
            pen.rect(draw, 6, 9, 4, 1, a(SAND));
            pen.rect(draw, 6, 11, 4, 1, a(SOIL_DARK));
        }
        Fixture::Light => match facing {
            Facing::Up => {
                pen.rect(draw, 7, 0, 2, 5, a(STEEL_DARK));
                pen.rect(draw, 4, 5, 8, 3, a(STEEL));
                pen.rect(draw, 4, 5, 8, 1, a(STEEL_LIGHT));
                pen.rect(draw, 6, 8, 4, 2, a(CREAM));
            }
            Facing::Down => {
                pen.rect(draw, 5, 14, 6, 2, a(STEEL_DARK));
                pen.rect(draw, 7, 7, 2, 7, a(STEEL));
                pen.rect(draw, 5, 2, 6, 5, a(AMBER));
                pen.rect(draw, 6, 3, 4, 3, a(CREAM));
            }
            Facing::Left | Facing::Right => {
                pen.rect(draw, fx(0, 2), 4, 2, 6, a(STEEL_DARK));
                pen.rect(draw, fx(2, 3), 6, 3, 2, a(STEEL));
                pen.rect(draw, fx(4, 3), 4, 3, 5, a(AMBER));
                pen.rect(draw, fx(5, 1), 5, 1, 3, a(CREAM));
            }
        },
        Fixture::Plant => {
            pen.rect(draw, 4, 11, 8, 5, a(RUST));
            pen.rect(draw, 3, 10, 10, 2, a(COPPER));
            pen.rect(draw, 4, 15, 8, 1, a(MAROON));
            for (x, y, w) in [
                (7, 2, 2),
                (4, 4, 3),
                (9, 4, 3),
                (2, 6, 4),
                (10, 6, 4),
                (5, 7, 6),
            ] {
                pen.rect(draw, fx(x, w), y, w, 2, a(LEAF));
            }
            pen.rect(draw, 7, 4, 2, 6, a(MOSS));
            pen.rect(draw, fx(3, 2), 6, 2, 1, a(LIME));
            pen.rect(draw, fx(11, 2), 6, 2, 1, a(LIME));
        }
        Fixture::Banner => {
            pen.rect(draw, 1, 0, T - 2, 2, a(STEEL_DARK));
            pen.rect(draw, 3, 2, 10, 11, a(accent));
            pen.rect(draw, 3, 2, 10, 1, a(BONE));
            pen.rect(draw, 3, 13, 3, 2, a(accent));
            pen.rect(draw, 10, 13, 3, 2, a(accent));
            pen.rect(draw, 6, 13, 4, 1, a(accent));
            // A pick emblem.
            pen.rect(draw, 6, 5, 4, 1, a(BONE));
            pen.rect(draw, 7, 5, 2, 6, a(BONE));
        }
        Fixture::Crates => {
            pen.rect(draw, 1, 6, 9, 10, a(SOIL));
            pen.rect(draw, 1, 6, 9, 1, a(LOAM));
            pen.rect(draw, 1, 10, 9, 1, a(SOIL_DARK));
            pen.rect(draw, fx(9, 6), 9, 6, 7, a(RUST));
            pen.rect(draw, fx(9, 6), 9, 6, 1, a(COPPER));
            pen.rect(draw, fx(3, 6), 1, 6, 5, a(LOAM));
            pen.rect(draw, fx(3, 6), 1, 6, 1, a(SAND));
            for (x, y, w, h) in [(1, 6, 1, 10), (9, 6, 1, 10), (3, 1, 1, 5), (8, 1, 1, 5)] {
                pen.rect(draw, fx(x, w), y, w, h, a(SOIL_DARK));
            }
        }
        Fixture::Table => {
            pen.rect(draw, 0, 7, T, 2, a(TIMBER_TOP));
            pen.rect(draw, 0, 9, T, 1, a(SOIL_DARK));
            pen.rect(draw, 1, 10, 2, 6, a(SOIL));
            pen.rect(draw, 13, 10, 2, 6, a(SOIL));
            // A vice and a lamp on the bench.
            pen.rect(draw, fx(2, 4), 4, 4, 3, a(STEEL));
            pen.rect(draw, fx(10, 3), 3, 3, 4, a(STEEL_DARK));
            pen.rect(draw, fx(10, 3), 2, 3, 1, a(GOLD));
        }
    }
}

const TIMBER_TOP: Color = AMBER;

fn draw_module(draw: &mut impl RaylibDraw, scene: &BaseScene<'_>, module: &PlacedModule) {
    let pen = tile_pen(module.origin);
    let powered = scene.powered.contains(&module.id);
    let ctx = ModuleArt {
        pen,
        flip: module.facing == Facing::Left,
        powered,
        seconds: scene.seconds,
        alpha: 255,
        accent: scene.accent(module.owner),
    };
    match module.kind {
        ModuleKind::Beacon => beacon(draw, &ctx),
        ModuleKind::Storage => storage(draw, &ctx, !module.storage.is_empty()),
        ModuleKind::Bed => bed(draw, &ctx),
        ModuleKind::Generator => generator(draw, &ctx),
        ModuleKind::FuelStation => {
            fuel_station(
                draw,
                &ctx,
                module.fuel as f32 / FUEL_STATION_CAPACITY as f32,
            );
        }
        ModuleKind::RepairBay => repair_bay(draw, &ctx),
        ModuleKind::CargoUplink => uplink(draw, &ctx),
        ModuleKind::LunarUplink => lunar_uplink(draw, &ctx),
        ModuleKind::Homestead => {
            let inside = scene.local.is_some_and(|rig| {
                let tile = crate::view_frame::frame().tile_at(rig.x, rig.y);
                module.covers(tile)
            });
            homestead(draw, &ctx, inside, !module.storage.is_empty());
        }
        ModuleKind::Refinery => {
            refinery(draw, &ctx, !module.storage.is_empty(), module.refined > 0);
        }
        ModuleKind::Workshop => workshop(draw, &ctx),
    }
}

/// Shared drawing state of one module.
struct ModuleArt {
    pen: Pen,
    /// Mirrored (facing left).
    flip: bool,
    powered: bool,
    seconds: f32,
    alpha: u8,
    accent: Color,
}

impl ModuleArt {
    /// A rectangle in module art pixels, mirrored inside `width` when flipped.
    #[allow(
        clippy::too_many_arguments,
        reason = "an art rectangle: where, how big, what colour"
    )]
    fn rect(
        &self,
        draw: &mut impl RaylibDraw,
        width: i32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        c: Color,
    ) {
        let x = if self.flip { width - x - w } else { x };
        self.pen
            .rect(draw, x, y, w, h, with_alpha(c, self.alpha.min(c.a)));
    }

    fn blink(&self, rate: f32, phase: f32) -> bool {
        ((self.seconds * rate + phase) as i32) % 2 == 0
    }

    /// The power light colour: green when powered, red otherwise.
    const fn power_light(&self) -> Color {
        if self.powered { LIME } else { RED }
    }
}

fn beacon(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Tripod feet.
    r(draw, 1, 28, 14, 4, INK);
    r(draw, 2, 28, 12, 2, STEEL_DARK);
    r(draw, 2, 22, 3, 7, STEEL_DARK);
    r(draw, 11, 22, 3, 7, STEEL_DARK);
    // Mast.
    r(draw, 6, 4, 4, 25, INK);
    r(draw, 7, 4, 2, 24, STEEL);
    r(draw, 7, 4, 1, 24, STEEL_LIGHT);
    for y in [10, 16, 22] {
        r(draw, 5, y, 6, 1, STEEL_DARK);
    }
    // Pennant in the owner's colour, waving.
    let wave = i32::from(art.blink(1.5, 0.0));
    r(draw, 9, 5 + wave, 6, 4, art.accent);
    r(draw, 9, 5 + wave, 6, 1, BONE);
    r(draw, 13, 9 + wave, 2, 1, art.accent);
    // Lamp.
    r(draw, 5, 0, 6, 4, INK);
    let lamp = if art.blink(1.0, 0.0) { GOLD } else { AMBER };
    r(draw, 6, 1, 4, 2, lamp);
    // Control box.
    r(draw, 3, 17, 4, 5, INK);
    r(draw, 4, 18, 2, 3, NIGHT);
    r(draw, 4, 18, 2, 1, art.power_light());
}

fn storage(draw: &mut impl RaylibDraw, art: &ModuleArt, full: bool) {
    let w = T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    r(draw, 0, 4, 16, 12, INK);
    r(draw, 1, 5, 14, 10, SOIL);
    r(draw, 1, 5, 14, 3, LOAM);
    r(draw, 1, 8, 14, 1, SOIL_DARK);
    // Metal bands and corners.
    r(draw, 3, 4, 2, 12, STEEL_DARK);
    r(draw, 11, 4, 2, 12, STEEL_DARK);
    r(draw, 3, 5, 1, 10, STEEL);
    r(draw, 11, 5, 1, 10, STEEL);
    // Latch.
    r(draw, 7, 7, 2, 3, GOLD);
    r(draw, 7, 9, 2, 1, AMBER);
    if full {
        // Treasure peeking from under the lid.
        r(draw, 5, 4, 2, 1, GOLD);
        r(draw, 9, 4, 1, 1, SKY);
    }
}

fn bed(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 2 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Frame.
    r(draw, 0, 4, 2, 12, SOIL_DARK);
    r(draw, 30, 7, 2, 9, SOIL_DARK);
    r(draw, 1, 11, 30, 3, SOIL);
    r(draw, 1, 11, 30, 1, LOAM);
    r(draw, 2, 14, 2, 2, SOIL_DARK);
    r(draw, 28, 14, 2, 2, SOIL_DARK);
    // Mattress, pillow and blanket in the owner's colour.
    r(draw, 2, 8, 28, 3, BONE);
    r(draw, 3, 6, 7, 3, CREAM);
    r(draw, 3, 6, 7, 1, BONE);
    r(draw, 10, 7, 20, 4, art.accent);
    r(draw, 10, 7, 20, 1, with_alpha(BONE, 160));
    r(draw, 10, 10, 20, 1, SLATE_DARK);
    // A footlocker.
    r(draw, 22, 13, 6, 3, STEEL_DARK);
}

fn generator(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 2 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Exhaust stack.
    r(draw, 22, 0, 6, 12, INK);
    r(draw, 23, 1, 4, 11, STEEL_DARK);
    r(draw, 23, 1, 1, 11, STEEL);
    let puff = (art.seconds * 2.0).fract();
    let py = 0 - (puff * 6.0) as i32;
    r(
        draw,
        23 - (puff * 2.0) as i32,
        py - 2,
        4,
        3,
        with_alpha(FOG, (120.0 * (1.0 - puff)) as u8),
    );
    // Body.
    r(draw, 1, 10, 30, 20, INK);
    r(draw, 2, 11, 28, 18, STEEL);
    r(draw, 2, 11, 28, 2, STEEL_LIGHT);
    r(draw, 2, 26, 28, 3, STEEL_DARK);
    // Cooling fins.
    for x in [4, 7, 10] {
        r(draw, x, 14, 2, 11, STEEL_DARK);
    }
    // Core window, glowing with the heat it draws up.
    let glow = if art.blink(3.0, 0.0) { ORANGE } else { AMBER };
    r(draw, 15, 14, 12, 9, INK);
    r(draw, 16, 15, 10, 7, MAROON);
    r(draw, 17, 16, 8, 5, glow);
    r(draw, 19, 17, 4, 3, GOLD);
    // Feet, the geothermal pipe into the floor and hazard stripes.
    r(draw, 2, 29, 28, 3, SLATE_DARK);
    for x in (2..30).step_by(4) {
        r(draw, x, 30, 2, 1, GOLD);
    }
    r(draw, 13, 29, 4, 3, RUST);
    r(draw, 4, 12, 2, 1, LIME);
}

fn fuel_station(draw: &mut impl RaylibDraw, art: &ModuleArt, reserve: f32) {
    let w = 3 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Concrete island with hazard stripes.
    r(draw, 0, 42, 48, 6, INK);
    r(draw, 1, 43, 46, 4, STONE);
    r(draw, 1, 43, 46, 1, STONE_LIGHT);
    for x in (2..46).step_by(6) {
        r(draw, x, 45, 3, 2, GOLD);
    }
    // The tank: a tall cylinder with a reserve gauge.
    r(draw, 2, 6, 18, 37, INK);
    r(draw, 3, 7, 16, 35, RED);
    r(draw, 3, 7, 4, 35, ROSE);
    r(draw, 15, 7, 4, 35, MAROON);
    r(draw, 3, 12, 16, 1, MAROON);
    r(draw, 3, 36, 16, 1, MAROON);
    r(draw, 5, 3, 12, 4, INK);
    r(draw, 6, 4, 10, 3, STEEL);
    // Gauge.
    r(draw, 8, 15, 6, 19, INK);
    r(draw, 9, 16, 4, 17, NIGHT);
    let level = (reserve.clamp(0.0, 1.0) * 17.0).round() as i32;
    let fuel_color = if reserve < 0.2 { ORANGE } else { LIME };
    if level > 0 {
        r(draw, 9, 33 - level, 4, level, fuel_color);
        r(draw, 9, 33 - level, 4, 1, with_alpha(BONE, 200));
    }
    // FUEL lettering band.
    r(draw, 4, 8, 14, 3, BONE);
    r(draw, 6, 9, 10, 1, RED);
    // Pipe from the tank to the dispenser.
    r(draw, 19, 34, 9, 3, INK);
    r(draw, 19, 35, 9, 1, STEEL);
    // Dispenser.
    r(draw, 27, 14, 16, 29, INK);
    r(draw, 28, 15, 14, 27, BONE);
    r(draw, 28, 15, 14, 3, art.accent);
    r(draw, 28, 38, 14, 4, MIST);
    // Screen.
    r(draw, 30, 20, 10, 7, INK);
    r(draw, 31, 21, 8, 5, NIGHT);
    if art.powered {
        let digits = (reserve * 9.0) as i32;
        for i in 0..4 {
            let lit = i <= digits % 5;
            r(draw, 32 + i * 2, 23, 1, 2, if lit { LIME } else { MOSS });
        }
    }
    // Power light and sign lamp.
    let light = if art.powered || art.blink(1.5, 0.0) {
        art.power_light()
    } else {
        MAROON
    };
    r(draw, 39, 29, 2, 2, light);
    // Nozzle and hose.
    r(draw, 42, 24, 3, 2, INK);
    r(draw, 44, 24, 2, 14, INK);
    r(draw, 44, 25, 1, 12, SLATE);
    r(draw, 41, 30, 4, 5, STEEL_DARK);
    r(draw, 41, 30, 4, 1, STEEL);
    // Canopy sign on a post.
    r(draw, 33, 4, 2, 10, STEEL_DARK);
    r(draw, 28, 0, 13, 6, INK);
    r(draw, 29, 1, 11, 4, RED);
    let sign = if art.powered { GOLD } else { CREAM };
    r(draw, 31, 2, 7, 2, sign);
}

fn repair_bay(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 3 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Pad.
    r(draw, 0, 43, 48, 5, INK);
    r(draw, 1, 44, 46, 3, STEEL_DARK);
    for x in (3..45).step_by(8) {
        r(draw, x, 45, 4, 1, art.accent);
    }
    // Gantry.
    r(draw, 1, 2, 4, 42, INK);
    r(draw, 2, 3, 2, 40, STEEL);
    r(draw, 43, 2, 4, 42, INK);
    r(draw, 44, 3, 2, 40, STEEL);
    r(draw, 1, 1, 46, 5, INK);
    r(draw, 2, 2, 44, 3, STEEL_LIGHT);
    for x in (6..42).step_by(6) {
        r(draw, x, 3, 3, 1, STEEL_DARK);
    }
    // Arm on a trolley, swinging slowly.
    let sway = ((art.seconds * 0.8).sin() * 8.0) as i32;
    let ax = 22 + sway;
    r(draw, ax - 3, 5, 8, 4, INK);
    r(draw, ax - 2, 6, 6, 2, GOLD);
    r(draw, ax, 9, 2, 14, STEEL_DARK);
    r(draw, ax - 3, 22, 8, 3, INK);
    r(draw, ax - 2, 23, 6, 1, STEEL);
    // Welding tip: sparks while powered.
    if art.powered && art.blink(9.0, 0.0) {
        r(draw, ax, 25, 2, 2, CREAM);
        r(draw, ax - 2, 26, 1, 1, GOLD);
        r(draw, ax + 3, 27, 1, 1, AMBER);
    } else {
        r(draw, ax, 25, 2, 1, SLATE);
    }
    // Wrench sign and power light.
    r(draw, 8, 10, 10, 8, INK);
    r(draw, 9, 11, 8, 6, BONE);
    r(draw, 11, 13, 4, 2, STEEL_DARK);
    r(draw, 10, 12, 2, 4, STEEL_DARK);
    r(draw, 6, 36, 3, 3, art.power_light());
    // Tool rack.
    r(draw, 32, 30, 10, 10, SLATE_DARK);
    for x in [33, 36, 39] {
        r(draw, x, 31, 2, 7, STEEL);
    }
}

/// The ore refinery: a hopper over a crusher feeding a furnace, molten ore running down a
/// channel into ingot moulds; a chimney. `loaded`: ore is waiting (the crusher turns and the
/// furnace roars while powered); `stocked`: refined value is waiting (ingots stacked).
fn refinery(draw: &mut impl RaylibDraw, art: &ModuleArt, loaded: bool, stocked: bool) {
    let w = 3 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    let working = loaded && art.powered;
    // Brick plinth.
    r(draw, 0, 42, 48, 6, INK);
    r(draw, 1, 43, 46, 4, MAROON);
    for x in (1..46).step_by(5) {
        r(draw, x, 45, 4, 1, RUST);
    }
    // Chimney with smoke while working.
    r(draw, 36, 0, 8, 22, INK);
    r(draw, 37, 1, 6, 21, STEEL_DARK);
    r(draw, 37, 1, 1, 21, STEEL);
    for y in (5..20).step_by(4) {
        r(draw, 37, y, 6, 1, RUST);
    }
    if working {
        for i in 0..2 {
            let puff = (art.seconds * 0.9 + i as f32 * 0.5).fract();
            let py = -(puff * 10.0) as i32;
            r(
                draw,
                38 + (puff * 3.0) as i32,
                py - 3,
                4 + i,
                3,
                with_alpha(FOG, (140.0 * (1.0 - puff)) as u8),
            );
        }
    }
    // Ore hopper on stilts.
    r(draw, 1, 4, 18, 3, INK);
    r(draw, 2, 6, 16, 8, INK);
    r(draw, 3, 7, 14, 6, STEEL);
    r(draw, 3, 7, 14, 1, STEEL_LIGHT);
    r(draw, 6, 13, 8, 4, INK);
    r(draw, 7, 13, 6, 3, STEEL_DARK);
    if loaded {
        // Ore heaped in the hopper.
        r(draw, 4, 4, 4, 3, STONE);
        r(draw, 8, 3, 4, 4, STONE_LIGHT);
        r(draw, 12, 4, 4, 3, STONE);
        r(draw, 9, 4, 1, 1, GOLD);
        r(draw, 5, 5, 1, 1, SKY);
        r(draw, 13, 5, 1, 1, ORANGE);
    }
    // Crusher drum, its teeth turning while working.
    r(draw, 3, 17, 14, 10, INK);
    r(draw, 4, 18, 12, 8, SLATE_DARK);
    let turn = if working {
        (art.seconds * 8.0) as i32 % 3
    } else {
        0
    };
    for i in 0..4 {
        r(
            draw,
            5 + i * 3 + turn % 2,
            19 + (i + turn) % 3 * 2,
            2,
            2,
            STEEL,
        );
    }
    // Furnace body.
    r(draw, 18, 14, 22, 29, INK);
    r(draw, 19, 15, 20, 27, STONE);
    r(draw, 19, 15, 20, 2, STONE_LIGHT);
    for (y, x0) in [(20, 19), (25, 23), (30, 19), (35, 23)] {
        r(draw, x0, y, 16, 1, SLATE);
    }
    // Furnace mouth.
    r(draw, 23, 26, 12, 10, INK);
    let fire = if working {
        if art.blink(6.0, 0.0) { ORANGE } else { GOLD }
    } else if art.powered {
        MAROON
    } else {
        NIGHT
    };
    r(draw, 24, 27, 10, 8, fire);
    if working {
        r(draw, 26, 30, 6, 4, CREAM);
    }
    // Feed chute from crusher to furnace.
    r(draw, 15, 22, 5, 3, INK);
    r(draw, 16, 23, 4, 1, STEEL_DARK);
    // Molten channel to the moulds.
    r(draw, 2, 36, 22, 3, INK);
    let melt = if working { ORANGE } else { RUST };
    r(draw, 3, 37, 20, 1, melt);
    if working {
        let drip = (art.seconds * 4.0).fract();
        r(draw, 3 + (drip * 18.0) as i32, 37, 2, 1, GOLD);
    }
    // Ingot moulds; ingots stacked when refined value waits.
    r(draw, 2, 39, 16, 3, SLATE_DARK);
    if stocked {
        for x in [3, 8, 13] {
            r(draw, x, 39, 4, 2, GOLD);
            r(draw, x, 39, 4, 1, CREAM);
        }
        r(draw, 5, 37, 4, 2, GOLD);
    }
    // Power light and owner's band.
    r(draw, 20, 16, 18, 2, art.accent);
    r(draw, 36, 38, 2, 2, art.power_light());
}

/// The workshop: a sturdy bench with a vice, a pegboard of tools, an anvil and a stack of
/// pressed charges.
fn workshop(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 2 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Pegboard with tools.
    r(draw, 2, 1, 26, 14, INK);
    r(draw, 3, 2, 24, 12, LOAM);
    for y in [4, 8, 12] {
        for x in (5..26).step_by(4) {
            r(draw, x, y, 1, 1, SOIL_DARK);
        }
    }
    // Hammer, wrench and saw.
    r(draw, 5, 3, 2, 9, SOIL_DARK);
    r(draw, 4, 3, 4, 2, STEEL);
    r(draw, 11, 4, 2, 8, STEEL);
    r(draw, 10, 3, 4, 2, STEEL_LIGHT);
    r(draw, 17, 4, 7, 4, STEEL_LIGHT);
    r(draw, 17, 7, 7, 1, STEEL_DARK);
    r(draw, 23, 4, 2, 5, SOIL);
    // Owner's banner strip.
    r(draw, 3, 2, 24, 1, art.accent);
    // Bench top and legs.
    r(draw, 0, 17, 32, 4, INK);
    r(draw, 1, 18, 30, 2, SOIL);
    r(draw, 1, 18, 30, 1, LOAM);
    r(draw, 2, 21, 3, 11, SOIL_DARK);
    r(draw, 27, 21, 3, 11, SOIL_DARK);
    r(draw, 5, 26, 22, 2, SOIL_DARK);
    // Vice on the bench.
    r(draw, 3, 13, 7, 5, INK);
    r(draw, 4, 14, 5, 3, STEEL_DARK);
    r(draw, 9, 15, 3, 1, STEEL);
    // A charge being packed, its fuse light blinking.
    r(draw, 14, 13, 6, 5, INK);
    r(draw, 15, 14, 4, 3, RED);
    let fuse = if art.blink(2.0, 0.0) { GOLD } else { AMBER };
    r(draw, 16, 12, 2, 1, fuse);
    // Anvil under the bench and a crate of finished charges.
    r(draw, 8, 28, 10, 4, INK);
    r(draw, 9, 28, 8, 2, STEEL);
    r(draw, 11, 30, 4, 2, STEEL_DARK);
    r(draw, 20, 25, 7, 7, INK);
    r(draw, 21, 26, 5, 5, SOIL);
    r(draw, 22, 26, 1, 1, RED);
    r(draw, 24, 26, 1, 1, RED);
}

fn uplink(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 2 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Pad and pylon.
    r(draw, 1, 42, 30, 6, INK);
    r(draw, 2, 43, 28, 4, STEEL_DARK);
    r(draw, 2, 43, 28, 1, STEEL);
    r(draw, 13, 16, 6, 27, INK);
    r(draw, 14, 17, 4, 25, STEEL);
    for y in (20..40).step_by(5) {
        r(draw, 12, y, 8, 1, STEEL_DARK);
    }
    // Dish, tilted up.
    r(draw, 4, 6, 24, 4, INK);
    r(draw, 2, 9, 28, 4, INK);
    r(draw, 5, 7, 22, 2, STEEL_LIGHT);
    r(draw, 3, 10, 26, 2, STEEL);
    r(draw, 15, 12, 2, 5, STEEL_DARK);
    // Feed horn and its beacon.
    r(draw, 15, 1, 2, 6, STEEL_DARK);
    let beam = if art.powered && art.blink(2.5, 0.0) {
        SKY_LIGHT
    } else if art.powered {
        SKY
    } else {
        MAROON
    };
    r(draw, 14, 0, 4, 2, beam);
    // Cargo hopper at the foot.
    r(draw, 3, 32, 9, 10, INK);
    r(draw, 4, 33, 7, 8, art.accent);
    r(draw, 4, 33, 7, 1, BONE);
    r(draw, 22, 36, 3, 3, art.power_light());
}

/// A lunar uplink (WP-MOON): a ring gate on a pad between two pylons, its ring humming mint
/// while powered.
fn lunar_uplink(draw: &mut impl RaylibDraw, art: &ModuleArt) {
    let w = 3 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // The pad.
    r(draw, 1, 26, 46, 6, INK);
    r(draw, 2, 27, 44, 4, STEEL_DARK);
    r(draw, 2, 27, 44, 1, STEEL_LIGHT);
    for x in (4..44).step_by(8) {
        r(draw, x, 29, 4, 1, GOLD);
    }
    // Pylons.
    for x in [3, 39] {
        r(draw, x, 6, 6, 21, INK);
        r(draw, x + 1, 7, 4, 19, STEEL);
        r(draw, x + 1, 7, 1, 19, STEEL_LIGHT);
        r(draw, x + 1, 4, 4, 3, art.power_light());
    }
    // The ring: drawn as stacked arcs.
    let glow = if art.powered {
        if art.blink(1.5, 0.0) { ICE } else { MINT }
    } else {
        SLATE
    };
    let rows: [(i32, i32); 11] = [
        (16, 16),
        (13, 22),
        (11, 26),
        (10, 28),
        (10, 28),
        (10, 28),
        (10, 28),
        (11, 26),
        (13, 22),
        (16, 16),
        (19, 10),
    ];
    for (index, (x, width)) in rows.into_iter().enumerate() {
        let y = 4 + i32::try_from(index).unwrap_or(0) * 2;
        r(draw, x, y, width, 2, INK);
        r(draw, x + 1, y, 2, 2, glow);
        r(draw, x + width - 3, y, 2, 2, glow);
    }
    if art.powered {
        // The field inside the ring.
        let shimmer = (art.seconds * 3.0).fract();
        let inner = if shimmer < 0.5 { TEAL } else { TEAL_DARK };
        r(draw, 15, 8, 18, 12, inner);
        r(draw, 18, 10, 12, 8, MINT);
    }
}

fn homestead(draw: &mut impl RaylibDraw, art: &ModuleArt, inside: bool, stocked: bool) {
    let w = 7 * T;
    let r = |d: &mut _, x, y, ww, hh, c| art.rect(d, w, x, y, ww, hh, c);
    // Chimney with smoke.
    r(draw, 82, 4, 10, 20, INK);
    r(draw, 83, 5, 8, 19, STONE);
    r(draw, 83, 5, 8, 2, STONE_LIGHT);
    for i in 0..3 {
        let phase = (art.seconds * 0.6 + i as f32 / 3.0).fract();
        let size = 3 + (phase * 4.0) as i32;
        r(
            draw,
            86 + (phase * 6.0) as i32 - size / 2,
            2 - (phase * 18.0) as i32,
            size,
            size,
            with_alpha(FOG, (150.0 * (1.0 - phase)) as u8),
        );
    }
    // Pitched roof.
    for row in 0..14 {
        let inset = 14 - row;
        r(draw, inset - 2, 10 + row, w - 2 * inset + 4, 1, INK);
        r(
            draw,
            inset - 1,
            10 + row,
            w - 2 * inset + 2,
            1,
            if row % 3 == 0 { MAROON } else { RUST },
        );
    }
    r(draw, 12, 10, w - 24, 1, COPPER);
    // Walls.
    r(draw, 6, 24, w - 12, 50, INK);
    if inside {
        // Cutaway: the room inside, lit by the lamp.
        r(draw, 7, 25, w - 14, 48, SOIL);
        for x in (9..w - 8).step_by(6) {
            r(draw, x, 25, 1, 48, SOIL_DARK);
        }
        r(draw, 7, 25, w - 14, 2, LOAM);
        // Fireplace.
        r(draw, 78, 46, 18, 27, STONE_DARK);
        r(draw, 80, 52, 14, 21, INK);
        let flame = if art.blink(6.0, 0.0) { ORANGE } else { AMBER };
        r(draw, 83, 63, 8, 8, flame);
        r(draw, 85, 60, 4, 4, GOLD);
        r(draw, 78, 44, 18, 3, STONE);
        // Bed in the owner's colour.
        r(draw, 10, 62, 30, 3, SOIL_DARK);
        r(draw, 10, 58, 30, 4, BONE);
        r(draw, 18, 57, 22, 5, art.accent);
        r(draw, 11, 56, 7, 3, CREAM);
        r(draw, 10, 52, 2, 21, SOIL_DARK);
        // Trophy shelf with relics.
        r(draw, 46, 40, 26, 2, SOIL_DARK);
        r(draw, 48, 35, 4, 5, PINK);
        r(draw, 55, 33, 3, 7, GOLD);
        r(draw, 61, 36, 5, 4, SKY);
        r(draw, 67, 34, 3, 6, LAVENDER);
        // Table and stool.
        r(draw, 48, 60, 18, 2, AMBER);
        r(draw, 49, 62, 2, 11, SOIL_DARK);
        r(draw, 63, 62, 2, 11, SOIL_DARK);
        r(draw, 54, 56, 3, 4, CREAM);
        // Hanging lamp.
        r(draw, 56, 25, 1, 8, STEEL_DARK);
        r(draw, 53, 33, 7, 3, GOLD);
        // Pantry chest.
        r(draw, 70, 66, 7, 7, SOIL);
        r(draw, 70, 66, 7, 1, LOAM);
        if stocked {
            r(draw, 72, 65, 2, 1, GOLD);
        }
        // Floorboards.
        r(draw, 7, 72, w - 14, 1, SOIL_DARK);
    } else {
        // Timber frame over plaster.
        r(draw, 7, 25, w - 14, 48, BONE);
        r(draw, 7, 25, w - 14, 2, CREAM);
        for x in [7, 36, 72, w - 10] {
            r(draw, x, 25, 3, 48, SOIL);
        }
        r(draw, 7, 46, w - 14, 2, SOIL);
        // Windows, warm inside.
        for x in [14, 82] {
            r(draw, x, 32, 14, 12, INK);
            r(draw, x + 1, 33, 12, 10, AMBER);
            r(draw, x + 1, 33, 12, 3, GOLD);
            r(draw, x + 6, 33, 1, 10, SOIL_DARK);
            r(draw, x + 1, 38, 12, 1, SOIL_DARK);
            r(draw, x - 1, 44, 16, 2, SOIL);
        }
        // Door with a lamp.
        r(draw, 48, 48, 16, 25, INK);
        r(draw, 49, 49, 14, 24, RUST);
        r(draw, 49, 49, 14, 2, COPPER);
        r(draw, 55, 49, 1, 24, MAROON);
        r(draw, 59, 61, 2, 2, GOLD);
        r(draw, 66, 50, 3, 4, GOLD);
        // Flower box and a name plate in the owner's colour.
        r(draw, 14, 45, 14, 3, SOIL_DARK);
        for x in [15, 19, 23, 26] {
            r(draw, x, 43, 2, 2, if x % 2 == 0 { PINK } else { GOLD });
        }
        r(draw, 46, 40, 20, 5, art.accent);
        r(draw, 46, 40, 20, 1, BONE);
    }
    // Stone foundation.
    r(draw, 4, 73, w - 8, 7, INK);
    r(draw, 5, 74, w - 10, 5, STONE);
    for x in (6..w - 6).step_by(9) {
        r(draw, x, 74, 1, 5, STONE_DARK);
    }
    r(draw, 5, 74, w - 10, 1, STONE_LIGHT);
}

/// Every claim square in view, as a dashed outline in the owner's colour (build mode only).
pub(in crate::rendering) fn draw_claims(
    draw: &mut impl RaylibDraw,
    scene: &BaseScene<'_>,
    highlight: Option<&Claim>,
) {
    for claim in scene.bases.claims() {
        let strong = highlight.is_some_and(|other| other.id == claim.id);
        claim_outline(
            draw,
            claim.center,
            scene.accent(claim.owner),
            strong,
            scene.seconds,
        );
    }
}

fn claim_outline(
    draw: &mut impl RaylibDraw,
    center: TilePosition,
    color: Color,
    strong: bool,
    seconds: f32,
) {
    let left = crate::view_frame::frame().rel_col(center.x - CLAIM_RADIUS) * TILE_SIZE;
    let top = crate::view_frame::frame().rel_row(center.y - CLAIM_RADIUS) * TILE_SIZE;
    let size = (2 * CLAIM_RADIUS + 1) as f32 * TILE_SIZE;
    let alpha = if strong { 200 } else { 90 };
    let color = with_alpha(color, alpha);
    let dash = 4.0 * ART;
    let shift = (seconds * 8.0).rem_euclid(2.0) * dash;
    let dashes = (size / (dash * 2.0)).ceil() as i32 + 1;
    for index in 0..dashes {
        let offset = (index as f32).mul_add(dash * 2.0, -shift);
        let start = offset.max(0.0);
        let end = (offset + dash).min(size);
        if end > start {
            for (x, y, w, h) in [
                (left + start, top, end - start, ART),
                (left + start, top + size - ART, end - start, ART),
                (left, top + start, ART, end - start),
                (left + size - ART, top + start, ART, end - start),
            ] {
                draw.draw_rectangle_rec(Rectangle::new(x, y, w, h), color);
            }
        }
    }
}

/// The build-mode ghost: the selected piece at the cursor, outlined green when it fits and red
/// when it does not, the claim it would stake, and the reach square.
pub(in crate::rendering) fn draw_build_ghost(
    draw: &mut impl RaylibDraw,
    scene: &BaseScene<'_>,
    preview: &BuildPreview,
) {
    let ok = preview.place.is_ok();
    // Reach square.
    let reach = fatherload_simulation::base::BUILD_REACH;
    let left = crate::view_frame::frame().rel_col(preview.rig.x - reach) * TILE_SIZE;
    let top = crate::view_frame::frame().rel_row(preview.rig.y - reach) * TILE_SIZE;
    let size = (2 * reach + 1) as f32 * TILE_SIZE;
    draw.draw_rectangle_lines_ex(
        Rectangle::new(left, top, size, size),
        ART,
        with_alpha(BONE, 40),
    );
    // The claim a beacon would stake.
    if preview.item == Buildable::Module(ModuleKind::Beacon) {
        let color = if ok { MINT } else { RED };
        claim_outline(draw, preview.target, color, true, scene.seconds);
    }
    let pen = tile_pen(preview.origin);
    let ghost_alpha = 200;
    match preview.item {
        Buildable::Tile(tile) => {
            let (fill, light) = built_colors(tile);
            pen.rect(draw, 0, 0, T, T, with_alpha(fill, ghost_alpha));
            pen.rect(draw, 0, 0, T, 2, with_alpha(light, ghost_alpha));
        }
        Buildable::BackWall(block) => back_wall(draw, pen, block, ghost_alpha),
        Buildable::Fixture(fixture) => {
            draw_fixture(draw, pen, fixture, preview.facing, false, GOLD, ghost_alpha);
        }
        Buildable::Module(kind) => {
            let art = ModuleArt {
                pen,
                flip: preview.facing == Facing::Left,
                powered: true,
                seconds: scene.seconds,
                alpha: ghost_alpha,
                accent: scene.local.map_or(GOLD, |rig| scene.accent(rig.player_id)),
            };
            match kind {
                ModuleKind::Beacon => beacon(draw, &art),
                ModuleKind::Storage => storage(draw, &art, false),
                ModuleKind::Bed => bed(draw, &art),
                ModuleKind::Generator => generator(draw, &art),
                ModuleKind::FuelStation => fuel_station(draw, &art, 0.3),
                ModuleKind::RepairBay => repair_bay(draw, &art),
                ModuleKind::CargoUplink => uplink(draw, &art),
                ModuleKind::LunarUplink => lunar_uplink(draw, &art),
                ModuleKind::Homestead => homestead(draw, &art, false, false),
                ModuleKind::Refinery => refinery(draw, &art, false, false),
                ModuleKind::Workshop => workshop(draw, &art),
            }
        }
        // The build kit's defences (Wave 10): the turret or trap sprite, a barricade block.
        Buildable::Defence(kind) => {
            let (x, y) = (
                crate::view_frame::frame().rel_col(preview.origin.x) * TILE_SIZE,
                crate::view_frame::frame().rel_row(preview.origin.y) * TILE_SIZE,
            );
            draw_defence_icon(draw, x, y, ART, kind, ghost_alpha);
        }
    }
    let outline = if ok {
        MINT
    } else if matches!(preview.place, Err(BuildRefusal::CannotAfford)) {
        AMBER
    } else {
        RED
    };
    let pulse = ((scene.seconds * 4.0).sin() * 0.5 + 0.5) * 80.0;
    let rect = Rectangle::new(
        crate::view_frame::frame().rel_col(preview.origin.x) * TILE_SIZE,
        crate::view_frame::frame().rel_row(preview.origin.y) * TILE_SIZE,
        preview.size.0 as f32 * TILE_SIZE,
        preview.size.1 as f32 * TILE_SIZE,
    );
    draw.draw_rectangle_lines_ex(rect, ART, with_alpha(outline, 150 + pulse as u8));
    // Corner ticks on the cursor tile itself.
    let cursor = tile_pen(preview.target);
    for (x, y, w, h) in [
        (0, 0, 4, 1),
        (0, 0, 1, 4),
        (12, 0, 4, 1),
        (15, 0, 1, 4),
        (0, 15, 4, 1),
        (0, 12, 1, 4),
        (12, 15, 4, 1),
        (15, 12, 1, 4),
    ] {
        cursor.rect(draw, x, y, w, h, BONE);
    }
}

/// Fill and highlight colours of a solid piece's ghost.
const fn built_colors(tile: BuiltTile) -> (Color, Color) {
    match tile {
        BuiltTile::Wall(BlockKind::Dirt) => (SOIL, LOAM),
        BuiltTile::Wall(BlockKind::Clay) => (RUST, CLAY),
        BuiltTile::Wall(BlockKind::Stone) => (STONE, STONE_LIGHT),
        BuiltTile::Wall(BlockKind::HardRock) => (SLATE, SLATE_LIGHT),
        BuiltTile::Plating => (STEEL, STEEL_LIGHT),
        BuiltTile::Glass => (OCEAN, ICE),
        BuiltTile::Plank | BuiltTile::PressurePlate { .. } | BuiltTile::Chest { .. } => {
            (RUST, AMBER)
        }
        BuiltTile::VentGrate => (NIGHT, STEEL_LIGHT),
        // Ruin pieces are never built; their ghosts never show.
        BuiltTile::RuinWall | BuiltTile::GateLintel(_) | BuiltTile::Lever { .. } => {
            (TEAL_DARK, TEAL)
        }
        BuiltTile::Gate | BuiltTile::KeycardDoor(_) => (STEEL_DARK, STEEL_LIGHT),
        BuiltTile::Palisade => (SOIL, AMBER),
        // Tomb pieces are never built either.
        BuiltTile::TombWall(_)
        | BuiltTile::TombPillar(_)
        | BuiltTile::TombDoor { .. }
        | BuiltTile::TombShrine { .. }
        | BuiltTile::TombStairs
        | BuiltTile::TombExit
        | BuiltTile::TombCache { .. }
        | BuiltTile::TombTrap { .. } => (SAND, AMBER),
    }
}

/// A small icon of `item` for the build bar: drawn at `pen` with `scale` screen points per art
/// pixel, fitting a 16x16 art box.
pub(in crate::rendering) fn draw_item_icon(
    draw: &mut impl RaylibDraw,
    x: f32,
    y: f32,
    scale: f32,
    item: Buildable,
    accent: Color,
) {
    let px =
        |draw: &mut dyn FnMut(Rectangle, Color), ax: f32, ay: f32, w: f32, h: f32, c: Color| {
            draw(
                Rectangle::new(x + ax * scale, y + ay * scale, w * scale, h * scale),
                c,
            );
        };
    let mut rects: Vec<(Rectangle, Color)> = Vec::new();
    {
        let mut push = |rect: Rectangle, color: Color| rects.push((rect, color));
        match item {
            Buildable::Tile(tile) => {
                let (fill, light) = built_colors(tile);
                px(&mut push, 1.0, 1.0, 14.0, 14.0, INK);
                px(&mut push, 2.0, 2.0, 12.0, 12.0, fill);
                px(&mut push, 2.0, 2.0, 12.0, 2.0, light);
                if matches!(tile, BuiltTile::Wall(_)) {
                    px(&mut push, 2.0, 7.0, 12.0, 1.0, INK);
                    px(&mut push, 7.0, 2.0, 1.0, 5.0, INK);
                    px(&mut push, 4.0, 8.0, 1.0, 6.0, INK);
                    px(&mut push, 10.0, 8.0, 1.0, 6.0, INK);
                }
                if tile == BuiltTile::Glass {
                    px(&mut push, 4.0, 5.0, 2.0, 2.0, ICE);
                    px(&mut push, 7.5, 2.0, 1.0, 12.0, STEEL);
                }
                if tile == BuiltTile::Palisade {
                    // Three sharpened logs and a lashing.
                    px(&mut push, 1.0, 1.0, 14.0, 4.0, NIGHT);
                    for log in [2.0, 7.0, 12.0] {
                        px(&mut push, log, 4.0, 3.0, 10.0, SOIL);
                        px(&mut push, log, 4.0, 1.0, 10.0, AMBER);
                        px(&mut push, log + 1.0, 2.0, 1.0, 2.0, SOIL);
                    }
                    px(&mut push, 2.0, 9.0, 13.0, 1.0, SAND);
                }
                if tile == BuiltTile::VentGrate {
                    for slat in [4.0, 8.0, 12.0] {
                        px(&mut push, 2.0, slat, 12.0, 1.0, STEEL);
                    }
                    px(&mut push, 2.0, 2.0, 12.0, 1.0, AMBER);
                }
            }
            Buildable::BackWall(block) => {
                let (fill, mortar, light) = back_wall_colors(block);
                px(&mut push, 1.0, 1.0, 14.0, 14.0, mortar);
                px(&mut push, 2.0, 2.0, 12.0, 4.0, fill);
                px(&mut push, 2.0, 7.0, 12.0, 4.0, fill);
                px(&mut push, 2.0, 12.0, 12.0, 2.0, fill);
                px(&mut push, 2.0, 2.0, 12.0, 1.0, light);
                px(&mut push, 2.0, 7.0, 12.0, 1.0, light);
            }
            Buildable::Fixture(_) | Buildable::Module(_) | Buildable::Defence(_) => {}
        }
    }
    for (rect, color) in rects {
        draw.draw_rectangle_rec(rect, color);
    }
    match item {
        Buildable::Fixture(fixture) => {
            let pen = Pen::at_exact(x, y, scale);
            draw_fixture_scaled(draw, pen, fixture, accent);
        }
        Buildable::Module(kind) => {
            let (w, h) = kind.footprint();
            let span = (w.max(h) * T) as f32;
            let fit = scale * 16.0 / span;
            let ox = x + (16.0 * scale - w as f32 * T as f32 * fit) * 0.5;
            let oy = y + (16.0 * scale - h as f32 * T as f32 * fit) * 0.5;
            let art = ModuleArt {
                pen: Pen::at_exact(ox, oy, fit),
                flip: false,
                powered: true,
                seconds: 0.0,
                alpha: 255,
                accent,
            };
            draw_module_icon(draw, &art, kind);
        }
        Buildable::Defence(kind) => {
            // Fit the defence's footprint into the icon square.
            let (_, h) = kind.footprint();
            let fit = scale / h as f32;
            let ox = x + (16.0 * scale - 16.0 * fit) * 0.5;
            draw_defence_icon(draw, ox, y, fit, kind, 255);
        }
        Buildable::Tile(_) | Buildable::BackWall(_) => {}
    }
}

/// A defence's art (Wave 10) at world (or screen) `(x, y)`, `scale` units per art pixel:
/// the turret or trap sprite, or a war-plate barricade block.
fn draw_defence_icon(
    draw: &mut impl RaylibDraw,
    x: f32,
    y: f32,
    scale: f32,
    kind: fatherload_simulation::garrison::DefenceKind,
    alpha: u8,
) {
    use crate::rendering::pixel_art::{sprite, sprites::garrison as art};
    use fatherload_simulation::garrison::DefenceKind;
    let tint = Color::new(255, 255, 255, alpha);
    match kind {
        DefenceKind::Turret => {
            sprite::draw_sprite(
                draw,
                &art::DEFENCE_TURRET,
                0,
                Vector2::new(x, y),
                scale,
                tint,
            );
        }
        DefenceKind::ShockTrap => {
            sprite::draw_sprite(
                draw,
                &art::DEFENCE_TRAP,
                0,
                Vector2::new(x, y + 11.0 * scale),
                scale,
                tint,
            );
        }
        DefenceKind::Barricade => {
            let px = |draw: &mut dyn FnMut(Rectangle, Color),
                      ax: f32,
                      ay: f32,
                      w: f32,
                      h: f32,
                      c: Color| {
                draw(
                    Rectangle::new(x + ax * scale, y + ay * scale, w * scale, h * scale),
                    c,
                );
            };
            let mut rects: Vec<(Rectangle, Color)> = Vec::new();
            {
                let mut push = |rect: Rectangle, color: Color| rects.push((rect, color));
                px(&mut push, 0.0, 0.0, 16.0, 16.0, INK);
                px(&mut push, 1.0, 1.0, 14.0, 14.0, GOLD);
                px(&mut push, 3.0, 3.0, 10.0, 10.0, MOSS);
                px(&mut push, 3.0, 3.0, 10.0, 1.0, LEAF);
                px(&mut push, 4.0, 4.0, 1.0, 1.0, STONE_LIGHT);
                px(&mut push, 11.0, 4.0, 1.0, 1.0, STONE_LIGHT);
                px(&mut push, 4.0, 11.0, 1.0, 1.0, STONE_LIGHT);
                px(&mut push, 11.0, 11.0, 1.0, 1.0, STONE_LIGHT);
            }
            for (rect, color) in rects {
                draw.draw_rectangle_rec(rect, Color::new(color.r, color.g, color.b, alpha));
            }
        }
    }
}

fn draw_fixture_scaled(draw: &mut impl RaylibDraw, pen: Pen, fixture: Fixture, accent: Color) {
    draw_fixture(draw, pen, fixture, fixture.facings()[0], false, accent, 255);
}

fn draw_module_icon(draw: &mut impl RaylibDraw, art: &ModuleArt, kind: ModuleKind) {
    match kind {
        ModuleKind::Beacon => beacon(draw, art),
        ModuleKind::Storage => storage(draw, art, true),
        ModuleKind::Bed => bed(draw, art),
        ModuleKind::Generator => generator(draw, art),
        ModuleKind::FuelStation => fuel_station(draw, art, 0.7),
        ModuleKind::RepairBay => repair_bay(draw, art),
        ModuleKind::CargoUplink => uplink(draw, art),
        ModuleKind::LunarUplink => lunar_uplink(draw, art),
        ModuleKind::Homestead => homestead(draw, art, false, false),
        ModuleKind::Refinery => refinery(draw, art, true, true),
        ModuleKind::Workshop => workshop(draw, art),
    }
}
