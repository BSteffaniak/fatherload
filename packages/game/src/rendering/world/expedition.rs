//! The Fathers' expedition in the world (Wave 8 story): their camps, abandoned rigs and cairns
//! with the cipher canister of each log, and the anomaly rooms' tells, drawn over the terrain
//! from the world's own placement (a pure function of its settings) and the shared journal.
//!
//! A canister still to read glows and glints; one reached early shows its red seal; a read one
//! stands open. The lights (the camp lantern, the canister's glow and each anomaly's own light)
//! go on after the darkness pass, so the trail reads from a distance in the dark.
//!
//! One tile is 16 art pixels; everything is art-pixel rectangles animated from the shell clock.
//! Anomaly tells move slowly and never flash (photosensitivity-safe).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "art pixels are small integers drawn at float scales"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    expedition::{
        ExpeditionState, SiteKind,
        anomaly::{AnomalyKind, AnomalyRoom, rooms_in},
        sites::{ExpeditionSite, sites_in},
    },
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{TileRect, WorldSettings},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, Pen, fx::disc};

/// Art pixels per tile.
const T: i32 = 16;

/// What the expedition layer needs from the view.
pub(super) struct ExpeditionScene<'a> {
    pub settings: &'a WorldSettings,
    pub expedition: &'a ExpeditionState,
    pub visible: Rectangle,
    pub seconds: f32,
}

impl ExpeditionScene<'_> {
    /// The visible tiles, with a margin.
    fn area(&self) -> TileRect {
        let frame = crate::view_frame::frame();
        TileRect {
            min: TilePosition {
                x: frame.col(self.visible.x) - 3,
                y: frame.row(self.visible.y) - 3,
            },
            max: TilePosition {
                x: frame.col_ceil(self.visible.x + self.visible.width) + 3,
                y: frame.row_ceil(self.visible.y + self.visible.height) + 3,
            },
        }
    }

    /// How the canister of `site` stands.
    fn canister_state(&self, site: &ExpeditionSite) -> Canister {
        let number = site.log + 1;
        if number <= self.expedition.found {
            Canister::Read
        } else if self.expedition.sealed.contains(&number) {
            Canister::Sealed
        } else {
            Canister::Waiting
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Canister {
    Waiting,
    Sealed,
    Read,
}

/// The local pixel `x` of absolute column `column`'s left edge (the view's frame, F7).
fn col_px(column: i32) -> f32 {
    crate::view_frame::frame().rel_col(column) * TILE_SIZE
}

/// The local pixel `y` of absolute row `row`'s top edge.
fn row_px(row: i32) -> f32 {
    crate::view_frame::frame().rel_row(row) * TILE_SIZE
}

/// A pen at the top-left of tile `tile`.
fn pen_at(tile: TilePosition) -> Pen {
    Pen::at(col_px(tile.x), row_px(tile.y))
}

/// Draws every visible expedition site and anomaly room (before the rigs).
pub(super) fn draw_expedition(draw: &mut impl RaylibDraw, scene: &ExpeditionScene<'_>) {
    if !scene.settings.generator.has_descents() {
        return;
    }
    let area = scene.area();
    for room in rooms_in(scene.settings, &area) {
        draw_room(draw, scene, &room, false);
    }
    for site in sites_in(scene.settings, &area) {
        let canister = scene.canister_state(&site);
        match site.kind {
            SiteKind::Camp => {
                draw_camp(draw, scene, &site);
                dress_camp(draw, scene, &site);
            }
            SiteKind::Rig => draw_wreck(draw, &site),
            SiteKind::Marker => draw_cairn(draw, &site),
            SiteKind::Grave => draw_grave(draw, scene, &site),
        }
        draw_canister(draw, scene, site.canister(), canister);
    }
}

/// How the camps of the deeper Descents look (Wave 9): what the crew had left by then, and
/// what the place did to their things.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CampDress {
    /// Descents I-III: the camp as they set out.
    Plain,
    /// Descent IV: the sled lashed on fuel drums as a raft, frost on the canvas, a paddle.
    Raft,
    /// Descent V: a thatch of giant leaves over the tent, a hammock, a beast's tusk.
    Jungle,
    /// Descent VI: the camp's mirror twin standing faintly beside it, corners rotted to blocks.
    Thin,
}

const fn camp_dress(site: &ExpeditionSite) -> CampDress {
    use fatherload_simulation::descent::Descent;
    match site.stratum.descent() {
        Descent::DrownedAndFrozen => CampDress::Raft,
        Descent::HollowWorld => CampDress::Jungle,
        Descent::StrangeLayers | Descent::Ancients | Descent::Heart => CampDress::Thin,
        Descent::Crust | Descent::OldDeep | Descent::LivingDark => CampDress::Plain,
    }
}

/// The deeper Descents' dressing over a camp ([`CampDress`]).
#[allow(clippy::too_many_lines, reason = "one dressing per Descent")]
fn dress_camp(draw: &mut impl RaylibDraw, scene: &ExpeditionScene<'_>, site: &ExpeditionSite) {
    let inner = site.interior();
    let pen = pen_at(inner.min);
    let floor = floor_y(site);
    let width = (inner.max.x - inner.min.x + 1) * T;
    // The tent's place and the sled's, as `draw_camp` lays them out.
    let (tent_x, tent_w, tent_h) = (8, 52, 40);
    let sled_x = tent_x + tent_w + 4 + 46;
    match camp_dress(site) {
        CampDress::Plain => {}
        CampDress::Raft => {
            // Frost along the canvas's ridge and slopes.
            for row in (0..tent_h).step_by(3) {
                let half = row * tent_w / (2 * tent_h);
                let cx = tent_x + tent_w / 2;
                pen.px(draw, cx - half, floor - tent_h + row, ICE);
                pen.px(draw, cx + half - 1, floor - tent_h + row, MIST);
            }
            pen.rect(draw, tent_x + tent_w / 2 - 3, floor - tent_h, 6, 1, ICE);
            // Two fuel drums lashed under the sled, rope wound round them.
            for drum in 0..2 {
                let x = sled_x + 2 + drum * 24;
                pen.boxed(draw, x, floor - 9, 18, 9, OCEAN);
                pen.rect(draw, x + 1, floor - 8, 16, 2, SKY);
                pen.rect(draw, x + 1, floor - 4, 16, 1, DEEP_BLUE);
                pen.rect(draw, x + 8, floor - 9, 2, 9, SAND);
            }
            // A paddle against the far prop instead of the pick, and a puddle of sea water.
            pen.rect(draw, width - 26, floor - 28, 2, 24, LOAM);
            pen.rect(draw, width - 28, floor - 8, 6, 8, CLAY);
            pen.rect(draw, width - 27, floor - 7, 4, 6, LOAM);
            let shine = ((scene.seconds * 0.8).sin() * 3.0) as i32;
            pen.rect(draw, 20, floor - 1, 36, 1, with_alpha(SKY, 150));
            pen.rect(draw, 30 + shine, floor - 1, 6, 1, with_alpha(ICE, 200));
        }
        CampDress::Jungle => {
            // Broad leaves laid over the ridge, each with a midrib.
            for (index, lean) in [-1, 1, -1, 1, 0].into_iter().enumerate() {
                let x = tent_x + 4 + index as i32 * 10;
                let y = floor - tent_h - 4 + (index as i32 % 2) * 3;
                pen.rect(draw, x, y, 16, 5, MOSS);
                pen.rect(draw, x + 1, y + 1, 14, 3, LEAF);
                pen.rect(draw, x + 2, y + 2 + lean.min(0), 12, 1, PINE);
            }
            // A hammock slung between the near prop and the tent pole.
            for x in 0..30 {
                let sag = (x - 15) * (x - 15) / 40;
                pen.px(draw, 4 + x, floor - 34 + 6 - sag.min(6), CREAM);
                pen.px(draw, 4 + x, floor - 33 + 6 - sag.min(6), SAND);
            }
            // A beast's tusk propped by the stove, and a ring of bright flowers.
            let tusk_x = width - 40;
            for step in 0..18 {
                pen.rect(
                    draw,
                    tusk_x + step,
                    floor - 4 - step * step / 18,
                    3,
                    3,
                    BONE,
                );
            }
            for (index, colour) in [ROSE, GOLD, PINK, ORANGE].into_iter().enumerate() {
                let x = sled_x - 30 + index as i32 * 5;
                pen.rect(draw, x, floor - 3, 1, 3, LEAF);
                pen.rect(draw, x - 1, floor - 5, 3, 2, colour);
            }
        }
        CampDress::Thin => {
            // The camp's mirror twin: the tent drawn again on the far side, flipped, in
            // scanlines of lavender with a pale outline, its sled a ghost of the real one.
            let twin_x = width - tent_x - tent_w - 2;
            let cx = twin_x + tent_w / 2;
            for row in 0..tent_h {
                let half = row * tent_w / (2 * tent_h);
                let y = floor - tent_h + row;
                if row % 2 == 0 {
                    pen.rect(draw, cx - half, y, half, 1, with_alpha(VIOLET, 120));
                    pen.rect(draw, cx, y, half, 1, with_alpha(LAVENDER, 100));
                }
                pen.px(draw, cx - half, y, with_alpha(BONE, 170));
                pen.px(draw, cx + half - 1, y, with_alpha(BONE, 170));
            }
            pen.rect(
                draw,
                cx - 1,
                floor - tent_h - 3,
                2,
                4,
                with_alpha(BONE, 150),
            );
            pen.rect(
                draw,
                twin_x - 44,
                floor - 12,
                40,
                9,
                with_alpha(LAVENDER, 70),
            );
            pen.text(draw, ".L.F", twin_x - 34, floor - 11, with_alpha(BONE, 160));
            // Corners rotted to blocks of the wrong colour (still: no flashing).
            let colours = [LIME, PINK, SKY, GOLD];
            for index in 0..14 {
                let x = (index * 37 + i32::from(site.log) * 11) % (width - 8) / 4 * 4;
                let y = (index * 23 + 5) % (floor - 8) / 4 * 4;
                pen.rect(
                    draw,
                    x,
                    y,
                    4,
                    4,
                    with_alpha(colours[index as usize % 4], 130),
                );
            }
        }
    }
}

/// A grave (Wave 9): Dov Halloran's, under the inner sun. A long mound of sun-baked earth, a
/// headstone of sunstone cut with his initials, his radio headset hung over it, and a ring of
/// flowers that grow in the light.
fn draw_grave(draw: &mut impl RaylibDraw, scene: &ExpeditionScene<'_>, site: &ExpeditionSite) {
    let inner = site.interior();
    let pen = pen_at(inner.min);
    let floor = floor_y(site);
    let width = (inner.max.x - inner.min.x + 1) * T;
    // The mound: a long dome of dark turned earth, outlined, with pebbles on its crest.
    let (mound_x, mound_w) = (24, 66);
    for column in 0..mound_w {
        let t = column as f32 / (mound_w - 1) as f32 * 2.0 - 1.0;
        let height = ((1.0 - t * t).max(0.0).sqrt() * 13.0) as i32 + 2;
        pen.rect(draw, mound_x + column, floor - height, 1, height, SOIL_DARK);
        pen.rect(
            draw,
            mound_x + column,
            floor - height + 1,
            1,
            height - 1,
            LOAM,
        );
        pen.px(draw, mound_x + column, floor - height + 1, CLAY);
        if column % 11 == 5 {
            pen.rect(draw, mound_x + column, floor - height + 4, 2, 2, SOIL);
        }
    }
    for (x, rise) in [(36, 12), (50, 15), (63, 13), (76, 9)] {
        pen.boxed(draw, x, floor - rise - 1, 4, 3, STONE);
        pen.px(draw, x + 1, floor - rise - 1, STONE_LIGHT);
    }
    // The headstone at the head of the mound: a pale stone slab with a rounded top and an
    // ink outline, so it stands out of the sun-washed rock, the initials and his radio's wave
    // cut into it.
    let stone_x = 4;
    let top = floor - 34;
    pen.rect(draw, stone_x + 2, top - 1, 16, 1, INK);
    pen.rect(draw, stone_x, top + 1, 1, 33, INK);
    pen.rect(draw, stone_x + 19, top + 1, 1, 33, INK);
    pen.px(draw, stone_x + 1, top, INK);
    pen.px(draw, stone_x + 18, top, INK);
    pen.rect(draw, stone_x + 1, top + 1, 18, 33, STONE_LIGHT);
    pen.rect(draw, stone_x + 2, top, 16, 1, STONE_LIGHT);
    pen.rect(draw, stone_x + 15, top + 2, 3, 31, STONE);
    pen.rect(draw, stone_x + 2, top + 1, 2, 31, BONE);
    pen.text(draw, "D.H.", stone_x + 3, top + 6, SLATE_DARK);
    for step in 0..13 {
        let y = top + 20 + ((step as f32 * 1.1).sin() * 2.0) as i32;
        pen.px(draw, stone_x + 3 + step, y, SLATE_DARK);
    }
    pen.rect(draw, stone_x - 1, floor - 2, 22, 2, STONE_DARK);
    // His headset hung over the stone: the band, two padded ear cups, the cord trailing to
    // the ground.
    pen.rect(draw, stone_x + 3, top - 4, 14, 2, INK);
    pen.rect(draw, stone_x + 4, top - 4, 12, 1, SLATE_LIGHT);
    for x in [stone_x - 1, stone_x + 16] {
        pen.boxed(draw, x, top - 2, 5, 7, SLATE_DARK);
        pen.rect(draw, x + 1, top - 1, 3, 5, STEEL);
        pen.px(draw, x + 1, top - 1, STEEL_LIGHT);
    }
    for step in 0..16 {
        pen.px(draw, stone_x + 20 + step / 5, top + 5 + step * 2, INK);
    }
    // Flowers that grow in the sun's light, nodding slowly (no flashing).
    let nod = ((scene.seconds * 0.9).sin() * 1.2) as i32;
    for (index, colour) in [GOLD, ROSE, CREAM, ORANGE, GOLD, PINK, CREAM]
        .into_iter()
        .enumerate()
    {
        let x = mound_x + 4 + index as i32 * 10;
        let stem = 5 + (index as i32 % 3);
        pen.rect(draw, x, floor - stem, 1, stem, LEAF);
        pen.rect(
            draw,
            x - 1 + nod * (index as i32 % 2),
            floor - stem - 2,
            3,
            2,
            colour,
        );
    }
    // Two stones at the foot, where the canister rests between them.
    for x in [width - 24, width - 8] {
        pen.boxed(draw, x, floor - 5, 6, 5, STONE);
        pen.px(draw, x + 1, floor - 5, STONE_LIGHT);
    }
}

/// The floor line of `site` in art pixels from the interior's top-left: the interior's bottom.
const fn floor_y(site: &ExpeditionSite) -> i32 {
    (site.interior().max.y - site.interior().min.y + 1) * T
}

fn draw_camp(draw: &mut impl RaylibDraw, scene: &ExpeditionScene<'_>, site: &ExpeditionSite) {
    let inner = site.interior();
    let pen = pen_at(inner.min);
    let floor = floor_y(site);
    let width = (inner.max.x - inner.min.x + 1) * T;
    // Timber props holding the ceiling at both ends, a cross beam.
    for x in [2, width - 6] {
        pen.rect(draw, x, 0, 4, floor, SOIL_DARK);
        pen.rect(draw, x + 1, 0, 2, floor, LOAM);
        pen.rect(draw, x + 1, 0, 1, floor, CLAY);
    }
    pen.rect(draw, 0, 0, width, 4, SOIL_DARK);
    pen.rect(draw, 0, 1, width, 2, LOAM);
    // A rope strung across with a hanging lamp on it.
    for x in (6..width - 6).step_by(2) {
        let sag = ((x - width / 2) * (x - width / 2)) / (width * 3);
        pen.px(draw, x, 6 + 6 - sag.min(6), SAND);
    }
    // The tent: canvas A-frame, shaded side, the door flap, guy ropes and pegs.
    let tent_x = 8;
    let tent_w = 52;
    let tent_h = 40;
    for row in 0..tent_h {
        let half = row * tent_w / (2 * tent_h);
        let y = floor - tent_h + row;
        let cx = tent_x + tent_w / 2;
        pen.rect(draw, cx - half, y, half, 1, SAND);
        pen.rect(draw, cx, y, half, 1, CLAY);
        pen.px(draw, cx - half, y, LOAM);
        pen.px(draw, cx + half - 1, y, SOIL);
        if row > tent_h / 2 {
            let door = (row - tent_h / 2) * 8 / (tent_h / 2);
            pen.rect(draw, cx - door, y, door * 2, 1, SOIL_DARK);
        }
    }
    let cx = tent_x + tent_w / 2;
    pen.rect(draw, cx - 1, floor - tent_h - 3, 2, 4, LOAM);
    pen.rect(draw, cx + 6, floor - 26, 6, 5, CREAM);
    pen.rect(draw, cx + 7, floor - 25, 4, 1, LOAM);
    for (from, to) in [
        (tent_x - 6, tent_x + 4),
        (tent_x + tent_w + 6, tent_x + tent_w - 4),
    ] {
        let steps = (to - from).abs();
        for step in 0..steps {
            let x = from + (to - from).signum() * step;
            pen.px(draw, x, floor - 1 - step * 14 / steps.max(1), STONE_LIGHT);
        }
        pen.rect(draw, from - 1, floor - 3, 2, 3, SOIL_DARK);
    }
    // A bedroll and the sled with its painted name.
    let roll_x = tent_x + tent_w + 4;
    pen.rect(draw, roll_x, floor - 7, 20, 7, MAROON);
    pen.rect(draw, roll_x, floor - 7, 20, 2, RUST);
    pen.rect(draw, roll_x + 6, floor - 7, 2, 7, BONE);
    pen.rect(draw, roll_x + 14, floor - 7, 2, 7, BONE);
    pen.rect(draw, roll_x + 19, floor - 6, 3, 5, PLUM);
    let sled_x = roll_x + 46;
    pen.rect(draw, sled_x, floor - 12, 44, 9, SOIL);
    pen.rect(draw, sled_x, floor - 12, 44, 2, CLAY);
    pen.rect(draw, sled_x - 3, floor - 3, 50, 2, STEEL_DARK);
    pen.rect(draw, sled_x - 5, floor - 6, 3, 4, STEEL_DARK);
    pen.text(draw, "F.L.", sled_x + 10, floor - 11, BONE);
    // A cold stove with its pipe and the last embers.
    let stove_x = roll_x + 28;
    pen.boxed(draw, stove_x, floor - 12, 12, 12, SLATE_DARK);
    pen.rect(
        draw,
        stove_x + 3,
        floor - 7,
        6,
        3,
        if ember(scene.seconds) { RUST } else { MAROON },
    );
    pen.rect(draw, stove_x + 8, floor - 24, 3, 12, SLATE);
    // The crate the canister rests on (the canister sits a tile in from its own tile).
    let crate_tile = site.canister();
    let crate_pen = pen_at(crate_tile);
    crate_pen.boxed(draw, 2, T - 10, 12, 10, LOAM);
    crate_pen.rect(draw, 3, T - 6, 10, 1, SOIL_DARK);
    crate_pen.rect(draw, 7, T - 9, 1, 8, SOIL_DARK);
    // A pickaxe leaning at the far prop.
    pen.rect(draw, width - 12, floor - 22, 2, 22, LOAM);
    pen.rect(draw, width - 17, floor - 23, 12, 3, STEEL);
    pen.px(draw, width - 18, floor - 22, STEEL_LIGHT);
}

/// Embers glow on and off slowly (twice a second at most: never a flash).
fn ember(seconds: f32) -> bool {
    (seconds * 1.7).sin() > -0.2
}

fn draw_wreck(draw: &mut impl RaylibDraw, site: &ExpeditionSite) {
    let inner = site.interior();
    let pen = pen_at(inner.min);
    let floor = floor_y(site);
    let width = (inner.max.x - inner.min.x + 1) * T;
    // A one-piece hull, older than any yard model, lying tilted on its side: rows of a long
    // rounded body that leans down to the right.
    let hull_x = 18;
    let hull_w = 88;
    for column in 0..hull_w {
        let lean = column / 11;
        let top = floor - 30 + lean;
        let bottom = floor - 4 + lean / 3;
        let edge = column < 3 || column >= hull_w - 3;
        let (top, bottom) = if edge {
            (top + 4, bottom - 3)
        } else {
            (top, bottom)
        };
        let x = hull_x + column;
        pen.rect(draw, x, top, 1, bottom - top, STEEL_DARK);
        pen.rect(draw, x, top + 1, 1, (bottom - top) / 2, STEEL);
        pen.px(draw, x, top, STEEL_LIGHT);
        if (column * 7 + lean * 3) % 23 < 4 {
            pen.rect(draw, x, top + 6, 1, 5, RUST);
        }
    }
    // The cracked cab window (where the canister was left) and a seam line.
    pen.rect(draw, hull_x + 8, floor - 25, 22, 10, INK);
    pen.rect(draw, hull_x + 9, floor - 24, 20, 8, DEEP_BLUE);
    pen.rect(draw, hull_x + 10, floor - 23, 6, 2, OCEAN);
    for step in 0..8 {
        pen.px(draw, hull_x + 18 + step, floor - 24 + step, ICE);
    }
    pen.rect(draw, hull_x + 34, floor - 20, hull_w - 40, 1, SLATE_DARK);
    // The drill cone, snapped off and pointing down at the floor.
    let cone_x = hull_x + hull_w - 2;
    for row in 0..14 {
        let half = 7 - row / 2;
        pen.rect(
            draw,
            cone_x + row / 2,
            floor - 20 + row,
            half.max(1),
            1,
            STEEL_LIGHT,
        );
        pen.px(draw, cone_x + row / 2, floor - 20 + row, STEEL_DARK);
    }
    // A loose track in the dust and scattered scrap.
    pen.rect(draw, 4, floor - 4, 30, 4, SLATE_DARK);
    for link in 0..7 {
        pen.rect(draw, 5 + link * 4, floor - 4, 2, 1, STEEL);
    }
    for (x, w) in [(width - 14, 6), (width - 24, 3), (hull_x + hull_w / 2, 4)] {
        pen.rect(draw, x, floor - 2, w, 2, RUST);
        pen.px(draw, x, floor - 3, COPPER);
    }
}

fn draw_cairn(draw: &mut impl RaylibDraw, site: &ExpeditionSite) {
    let inner = site.interior();
    let pen = pen_at(inner.min);
    let floor = floor_y(site);
    let width = (inner.max.x - inner.min.x + 1) * T;
    let cx = width / 2;
    // Stacked stones, wide to narrow, each with a lit top and a shaded side.
    let stones = [(14, 6), (12, 5), (10, 5), (7, 4)];
    let mut y = floor;
    for (index, (half_width, height)) in stones.into_iter().enumerate() {
        y -= height;
        let shift = i32::from(index % 2 != 0);
        pen.rect(
            draw,
            cx - half_width + shift,
            y,
            half_width * 2,
            height,
            STONE_DARK,
        );
        pen.rect(
            draw,
            cx - half_width + 1 + shift,
            y,
            half_width * 2 - 2,
            height - 1,
            STONE,
        );
        pen.rect(
            draw,
            cx - half_width + 2 + shift,
            y,
            half_width,
            1,
            STONE_LIGHT,
        );
    }
    // A flag on a stick, its cloth hanging still in the deep's dead air.
    let pole_x = cx + 10;
    pen.rect(draw, pole_x, floor - 46, 2, 46, LOAM);
    pen.rect(draw, pole_x + 2, floor - 46, 14, 9, RED);
    pen.rect(draw, pole_x + 2, floor - 38, 12, 1, MAROON);
    pen.text(draw, "F", pole_x + 5, floor - 46, BONE);
    // A chalk arrow on the back wall pointing down the trail.
    let arrow_x = cx - 30;
    pen.rect(draw, arrow_x, floor - 50, 2, 24, BONE);
    for step in 0..6 {
        pen.px(draw, arrow_x - step, floor - 28 - step, BONE);
        pen.px(draw, arrow_x + 1 + step, floor - 28 - step, BONE);
    }
}

fn draw_canister(
    draw: &mut impl RaylibDraw,
    scene: &ExpeditionScene<'_>,
    tile: TilePosition,
    state: Canister,
) {
    let pen = pen_at(tile);
    // A brass cipher canister with its dial rings, on the floor of its tile.
    let base = T - 10;
    let (body, ring) = match state {
        Canister::Waiting => (GOLD, AMBER),
        Canister::Sealed => (AMBER, MAROON),
        Canister::Read => (COPPER, RUST),
    };
    pen.boxed(draw, 5, base - 9, 7, 10, body);
    pen.rect(draw, 6, base - 6, 5, 1, ring);
    pen.rect(draw, 6, base - 3, 5, 1, ring);
    pen.px(draw, 6, base - 8, CREAM);
    match state {
        Canister::Waiting => {
            // A slow glint travels down the brass.
            let glint = ((scene.seconds * 1.3).rem_euclid(3.0) * 4.0) as i32;
            if glint < 9 {
                pen.px(draw, 10, base - 8 + glint, CREAM);
            }
        }
        Canister::Sealed => {
            // The red wax seal over the cap.
            pen.rect(draw, 7, base - 11, 3, 2, RED);
            pen.px(draw, 8, base - 12, ROSE);
        }
        Canister::Read => {
            // The lid lies beside it, the canister stands open.
            pen.rect(draw, 6, base - 10, 5, 1, INK);
            pen.rect(draw, 12, base - 1, 5, 2, body);
        }
    }
}

#[allow(clippy::too_many_lines, reason = "one tell per kind of room")]
/// One anomaly room's tells: its props (`glow` false, before the darkness pass) or its
/// luminous parts (`glow` true, after it, so they read in the dark).
fn draw_room(
    draw: &mut impl RaylibDraw,
    scene: &ExpeditionScene<'_>,
    room: &AnomalyRoom,
    glow: bool,
) {
    let inner = room.interior();
    let pen = pen_at(inner.min);
    let width = (inner.max.x - inner.min.x + 1) * T;
    let height = (inner.max.y - inner.min.y + 1) * T;
    let seconds = scene.seconds;
    let seed = (room.region.0.wrapping_mul(73_856_093) ^ room.region.1.wrapping_mul(19_349_663))
        .unsigned_abs();
    let spot = |index: u32, span: i32| -> i32 {
        let mixed = seed.wrapping_add(index.wrapping_mul(2_654_435_761)) >> 7;
        (mixed % span.max(1) as u32) as i32
    };
    match room.kind {
        AnomalyKind::GravityFlip => {
            // Rubble and dust gather on the ceiling; motes drift up toward it.
            for index in (0..16).filter(|_| !glow) {
                let x = spot(index, width - 8);
                let w = 3 + spot(index + 40, 6);
                let h = 2 + spot(index + 80, 3);
                pen.rect(draw, x, 0, w, h, STONE_DARK);
                pen.rect(draw, x + 1, 0, w - 2, h - 1, STONE);
                pen.rect(draw, x + 1, h - 2, w - 3, 1, STONE_LIGHT);
            }
            if !glow {
                return;
            }
            // Pebbles and drops falling up, each with a short streak below it.
            for index in 0..30 {
                let x = spot(index + 200, width - 2);
                let rise = (seconds * 7.0 + spot(index + 300, height) as f32)
                    .rem_euclid(height as f32) as i32;
                let y = height - rise;
                let tint = if index % 3 == 0 {
                    STONE_LIGHT
                } else {
                    LAVENDER
                };
                pen.rect(draw, x, y + 3, 1, 6, with_alpha(tint, 60));
                pen.rect(draw, x - 1, y, 3, 3, with_alpha(tint, 150));
                pen.px(draw, x, y + 1, with_alpha(BONE, 230));
            }
            // An upturned glyph on the back wall: a ball on a cable, hanging the wrong way.
            let (cx, cy) = (width / 2, height / 2);
            ring(draw, pen, cx, cy, 14, with_alpha(VIOLET, 150));
            ring(draw, pen, cx, cy + 4, 4, with_alpha(LAVENDER, 190));
            pen.rect(draw, cx, cy - 14, 1, 14, with_alpha(LAVENDER, 170));
        }
        AnomalyKind::PixelRot if glow => {
            // Blocks of the wrong colour, re-dealt about once a second (no flashing: the
            // blocks are dim and change one at a time).
            let tick = (seconds * 1.1) as u32;
            let colours = [LIME, PINK, SKY, GOLD, MINT, VIOLET];
            for index in 0..16 {
                let deal = index + tick.wrapping_mul(16) * u32::from(index % 4 == (tick % 4));
                let x = spot(deal, width - 8) / 4 * 4;
                let y = spot(deal + 500, height - 8) / 4 * 4;
                let colour = colours[spot(deal + 900, colours.len() as i32) as usize];
                pen.rect(
                    draw,
                    x,
                    y,
                    4 + 4 * spot(deal + 70, 2),
                    4,
                    with_alpha(colour, 90),
                );
            }
            // A torn scanline row.
            let row = (seconds * 5.0).rem_euclid(height as f32) as i32;
            pen.rect(draw, 0, row, width, 1, with_alpha(BONE, 40));
        }
        AnomalyKind::TimeFrozen => {
            let sx = width / 3;
            let sy = height / 3;
            if !glow {
                // A stone stopped mid-fall, its streak behind it.
                for step in 0..8 {
                    pen.px(
                        draw,
                        sx + 2,
                        sy - 10 + step,
                        with_alpha(FOG, 30 + step as u8 * 15),
                    );
                }
                pen.boxed(draw, sx, sy, 6, 5, STONE);
                return;
            }
            // Drops and dust hung in the air mid-fall.
            for index in 0..34 {
                let x = spot(index, width);
                let y = spot(index + 100, height - 6);
                if index % 3 == 0 {
                    pen.rect(draw, x, y, 2, 3, with_alpha(SKY_LIGHT, 210));
                    pen.rect(draw, x, y + 3, 2, 1, with_alpha(ICE, 240));
                    pen.rect(draw, x, y - 3, 1, 3, with_alpha(SKY, 60));
                } else {
                    pen.rect(draw, x, y, 2, 2, with_alpha(MIST, 170));
                }
            }
            // A stopped clock face on the back wall.
            let (cx, cy) = (width * 2 / 3, height / 2);
            ring(draw, pen, cx, cy, 10, with_alpha(ICE, 120));
            pen.rect(draw, cx, cy - 7, 1, 7, with_alpha(ICE, 160));
            pen.rect(draw, cx, cy, 5, 1, with_alpha(ICE, 160));
        }
        AnomalyKind::Mirror if glow => {
            // The seam down the middle and a slow sheen sweeping across the panes.
            let mid = width / 2;
            pen.rect(draw, mid, 0, 1, height, with_alpha(LAVENDER, 120));
            let sweep = (seconds * 18.0).rem_euclid((width + 40) as f32) as i32 - 20;
            for row in 0..height {
                let x = sweep + row / 3;
                if (0..width).contains(&x) {
                    pen.px(draw, x, row, with_alpha(BONE, 60));
                }
            }
            // Twin glyphs, one the mirror of the other.
            for side in [-1, 1] {
                let gx = mid + side * width / 4;
                pen.rect(
                    draw,
                    gx - 3 * side.max(0),
                    height / 2 - 6,
                    3,
                    12,
                    with_alpha(VIOLET, 130),
                );
                pen.rect(draw, gx - 3, height / 2 - 6, 6, 2, with_alpha(VIOLET, 130));
            }
        }
        AnomalyKind::PixelRot | AnomalyKind::Mirror => {}
        AnomalyKind::Sideways => {
            if !glow {
                // Rubble heaped against the right wall: the floor, once down swings round.
                for index in 0..14 {
                    let y = spot(index, height - height / 4 - 8);
                    let h = 3 + spot(index + 40, 6);
                    let w = 2 + spot(index + 80, 3);
                    pen.rect(draw, width - w, y, w, h, STONE_DARK);
                    pen.rect(draw, width - w, y + 1, w - 1, h - 2, STONE);
                    pen.rect(draw, width - w, y + 1, 1, h - 3, STONE_LIGHT);
                }
                return;
            }
            // Dust streaming sideways toward the right wall, a short streak behind each mote,
            // above the floor strip where down is still down (`SIDEWAYS_FLOOR_ROWS`).
            let air = height - height / 4;
            for index in 0..26 {
                let y = spot(index + 200, air - 2);
                let run = (seconds * 8.0 + spot(index + 300, width) as f32).rem_euclid(width as f32)
                    as i32;
                pen.rect(draw, run - 6, y, 6, 1, with_alpha(ORANGE, 50));
                pen.rect(draw, run, y - 1, 3, 3, with_alpha(SAND, 160));
            }
            // The glyph turned on its side: the cable runs left to right, its ball at the wall.
            let (cx, cy) = (width / 2, height / 2);
            pen.rect(draw, cx - 16, cy, 16, 1, with_alpha(ORANGE, 170));
            ring(draw, pen, cx + 4, cy, 4, with_alpha(SAND, 190));
            ring(draw, pen, cx, cy, 14, with_alpha(RUST, 140));
        }
        AnomalyKind::Sunlit => {
            if !glow {
                // A small brass sun-disc set in the back wall, with its builder rings.
                let (cx, cy) = (width / 2, height / 3);
                pen.boxed(draw, cx - 5, cy - 5, 10, 10, COPPER);
                pen.rect(draw, cx - 3, cy - 3, 6, 6, GOLD);
                return;
            }
            // Shafts of warm light leaning across the room, swaying very slowly.
            let sway = (seconds * 0.4).sin() * 6.0;
            for shaft in 0..4 {
                let top = 10 + shaft * width / 4 + sway as i32;
                for row in 0..height {
                    let x = top + row / 2;
                    if (0..width - 6).contains(&x) {
                        pen.rect(draw, x, row, 6, 1, with_alpha(GOLD, 22));
                    }
                }
            }
            // Golden motes hanging in the light, rising a little.
            for index in 0..24 {
                let x = spot(index + 500, width);
                let rise = (seconds * 2.0 + spot(index + 600, height) as f32)
                    .rem_euclid(height as f32) as i32;
                pen.rect(draw, x, height - rise, 2, 2, with_alpha(CREAM, 150));
            }
            let (cx, cy) = (width / 2, height / 3);
            ring(draw, pen, cx, cy, 9, with_alpha(GOLD, 170));
            ring(draw, pen, cx, cy, 13, with_alpha(AMBER, 110));
        }
        AnomalyKind::Weightless => {
            // Stones adrift, each bobbing on its own slow beat (props and glow both: they are
            // lit by the room's violet).
            let bob = |index: u32| ((seconds * 0.7 + index as f32 * 1.3).sin() * 3.0) as i32;
            if !glow {
                for index in 0..9 {
                    let x = spot(index, width - 10);
                    let y = 12 + spot(index + 30, height - 30) + bob(index);
                    let size = 3 + spot(index + 60, 4);
                    pen.boxed(draw, x, y, size + 2, size, STONE);
                    pen.px(draw, x + 1, y, STONE_LIGHT);
                }
                return;
            }
            for index in 0..20 {
                let x = spot(index + 700, width);
                let y = spot(index + 800, height) + bob(index + 3);
                pen.rect(draw, x, y, 2, 2, with_alpha(LAVENDER, 140));
                pen.px(draw, x, y + 3, with_alpha(VIOLET, 80));
            }
        }
        AnomalyKind::Lensing => {
            if !glow {
                // A dark knot in the middle of the floor, the weight the room bends round.
                let cx = width / 2;
                pen.rect(draw, cx - 4, height - 6, 8, 6, NIGHT);
                pen.rect(draw, cx - 2, height - 8, 4, 2, INK);
                return;
            }
            // Rings of bent light round the middle, breathing in and out slowly.
            let (cx, cy) = (width / 2, height / 2);
            for band in 0..4 {
                let breath = ((seconds * 0.6 + band as f32).sin() * 2.0) as i32;
                let radius = 8 + band * 9 + breath;
                ring(
                    draw,
                    pen,
                    cx,
                    cy,
                    radius,
                    with_alpha(SKY_LIGHT, 130 - band as u8 * 25),
                );
            }
            // Stars seen through the fold: points that hold still in the bent light.
            for index in 0..12 {
                let x = spot(index + 900, width);
                let y = spot(index + 950, height);
                pen.px(draw, x, y, with_alpha(ICE, 200));
            }
        }
    }
}

/// A one-pixel ring of art radius `radius` centred at art `(cx, cy)`.
fn ring(draw: &mut impl RaylibDraw, pen: Pen, cx: i32, cy: i32, radius: i32, color: Color) {
    let steps = radius * 6;
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let x = cx + (angle.cos() * radius as f32).round() as i32;
        let y = cy + (angle.sin() * radius as f32).round() as i32;
        pen.px(draw, x, y, color);
    }
}

/// The trail's lights, after the darkness pass: the camp lantern, each canister still to
/// read, and every anomaly room's own light.
pub(super) fn draw_expedition_lights(draw: &mut impl RaylibDraw, scene: &ExpeditionScene<'_>) {
    if !scene.settings.generator.has_descents() {
        return;
    }
    let area = scene.area();
    let breathe = 1.0 + (scene.seconds * 1.5).sin() * 0.06;
    for site in sites_in(scene.settings, &area) {
        let inner = site.interior();
        if site.kind == SiteKind::Camp {
            // The hanging lamp on the rope.
            let x = col_px(inner.min.x) + (inner.max.x - inner.min.x + 1) as f32 * TILE_SIZE * 0.5;
            let y = row_px(inner.min.y) + 12.0 * ART;
            disc(draw, x, y, (46.0 * breathe) as i32, with_alpha(AMBER, 14));
            disc(draw, x, y, (22.0 * breathe) as i32, with_alpha(GOLD, 26));
            let pen = Pen::at(x, y);
            pen.rect(draw, -2, -3, 4, 5, GOLD);
            pen.rect(draw, -1, -2, 2, 3, CREAM);
            pen.rect(draw, -2, -4, 4, 1, STEEL_DARK);
        }
        if site.kind == SiteKind::Grave {
            // The headstone holds the sun's warmth and gives it back softly.
            let x = col_px(inner.min.x) + 14.0 * ART;
            let y = row_px(inner.max.y) + (T as f32 - 16.0) * ART;
            disc(draw, x, y, (40.0 * breathe) as i32, with_alpha(GOLD, 16));
            disc(draw, x, y, (18.0 * breathe) as i32, with_alpha(AMBER, 24));
        }
        if scene.canister_state(&site) == Canister::Waiting {
            let tile = site.canister();
            let x = col_px(tile.x) + 8.5 * ART;
            let y = row_px(tile.y) + (T as f32 - 14.0) * ART;
            disc(draw, x, y, (26.0 * breathe) as i32, with_alpha(GOLD, 18));
        }
    }
    for room in rooms_in(scene.settings, &area) {
        draw_room(draw, scene, &room, true);
        let inner = room.interior();
        let colour = match room.kind {
            AnomalyKind::GravityFlip => VIOLET,
            AnomalyKind::PixelRot => LIME,
            AnomalyKind::TimeFrozen => ICE,
            AnomalyKind::Mirror => LAVENDER,
            AnomalyKind::Sideways => ORANGE,
            AnomalyKind::Sunlit => GOLD,
            AnomalyKind::Weightless => ROSE,
            AnomalyKind::Lensing => SKY_LIGHT,
        };
        let centre = inner.center();
        let x = col_px(centre.x) + 0.5 * TILE_SIZE;
        let y = row_px(centre.y) + 0.5 * TILE_SIZE;
        disc(draw, x, y, (90.0 * breathe) as i32, with_alpha(colour, 10));
        disc(draw, x, y, (50.0 * breathe) as i32, with_alpha(colour, 14));
    }
}
