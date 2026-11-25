//! The builders' buried city in world space (Wave 10 ancients-a, Descent VII strata 1-3).
//! Presentation only: every mover is a pure function of world time and the generated city
//! (`fatherload_simulation::ancient_city::movers`), so every seat and peer draws the same tram
//! in the same place; the rest reads the replicated city state.
//!
//! * **Machinery** ([`draw_machinery`], behind the rigs): tram cars in cream and teal livery
//!   under their wire, lift cabs (brass cages on their cables), piston heads on their rams (a
//!   hiss of steam and an amber lamp before they rise), crushers (a lamp that warms through
//!   amber before the slam, red while it falls), belts' chevrons running, the Great Engine's
//!   flywheel turning on the beat and the Orrery's spheres circling.
//! * **The archivists** ([`draw_archivists`]): each player's drone bobbing beside its rig, its
//!   lens lit while it speaks; the seat's own scan marks round the ore it found; unread chart
//!   plinths throw a column of gold light.
//!
//! Every flicker is slow and low contrast and scales with the effects setting (off: steady).

use raylib::prelude::*;

use fatherload_simulation::{
    PlayerId,
    ancient_city::{
        AncientCity, CityTile, archivist,
        movers::{
            CRUSHER_TILES, CityMover, LIFT_CAB_ROWS, LIFT_CAB_TILES, MILLI, PISTON_TILES,
            StrokePhase, TRAM_CAR_ROWS, TRAM_CAR_TILES,
        },
    },
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::ancient_city::{self as sites, CityStamp, CityStampKind},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprites::ancient_city as art;

use super::{ART, Pen, draw_world_sprite, fx::disc};

/// What one view needs to draw the buried city.
pub(super) struct CityScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub city: &'a AncientCity,
    /// The drawn players (rig positions; the archivists hover by them).
    pub players: &'a [crate::render_snapshot::PlayerPresentation],
    /// World milliseconds of the last simulated tick.
    pub world_millis: u64,
    pub tick_rate_hz: u32,
    /// The presentation fraction towards the next tick.
    pub alpha: f32,
    pub seconds: f32,
    /// The effects setting's share (0 turns flicker and shimmer off).
    pub effects: f32,
    /// The seat's player (its scan marks).
    pub local: Option<PlayerId>,
}

impl CityScene<'_> {
    /// The drawn moment in world milliseconds, between the last tick and the next.
    fn millis(&self) -> u64 {
        let tick_ms = 1_000.0 / f64::from(self.tick_rate_hz.max(1));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a fraction of one tick"
        )]
        let extra = (tick_ms * f64::from(self.alpha.clamp(0.0, 1.0))) as u64;
        self.world_millis + extra
    }
}

/// The tiles (inclusive) covering `visible`, a margin round them.
fn tile_span(visible: Rectangle, margin: i32) -> TileRect {
    let frame = crate::view_frame::frame();
    TileRect {
        min: TilePosition {
            x: frame.col(visible.x) - margin,
            y: frame.row(visible.y) - margin,
        },
        max: TilePosition {
            x: frame.col(visible.x + visible.width) + margin,
            y: frame.row(visible.y + visible.height) + margin,
        },
    }
}

/// Whether the view touches the buried city's rows (most frames: no, and nothing is drawn).
fn view_touches(scene: &CityScene<'_>) -> bool {
    let settings = scene.terrain.settings();
    if !settings.generator.has_descents() {
        return false;
    }
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let span = tile_span(scene.visible, 2);
    let (top, bottom) = sites::carve_rows(&layout);
    span.max.y >= top && span.min.y <= bottom
}

/// Local pixels of absolute milli-columns.
fn milli_x(milli: i64) -> f32 {
    let frame = crate::view_frame::frame();
    let column = i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0);
    #[allow(clippy::cast_precision_loss, reason = "below a thousand")]
    let frac = milli.rem_euclid(MILLI) as f32 / 1_000.0;
    frac.mul_add(TILE_SIZE, frame.col_px(column))
}

/// Local pixels of absolute milli-rows.
fn milli_y(milli: i64) -> f32 {
    let frame = crate::view_frame::frame();
    let row = i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0);
    #[allow(clippy::cast_precision_loss, reason = "below a thousand")]
    let frac = milli.rem_euclid(MILLI) as f32 / 1_000.0;
    frac.mul_add(TILE_SIZE, frame.row_px(row))
}

/// A slow, gentle pulse (0..1) at `hz`, flattened to steady by the effects setting.
fn pulse(scene: &CityScene<'_>, hz: f32) -> f32 {
    let wave = (scene.seconds * hz * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    wave.mul_add(
        scene.effects.clamp(0.0, 1.0),
        1.0 - scene.effects.clamp(0.0, 1.0),
    )
}

/// The movers and stamps round the view.
fn gather(scene: &CityScene<'_>) -> (Vec<CityMover>, Vec<CityStamp>) {
    let settings = scene.terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let span = tile_span(scene.visible, 6);
    (
        sites::movers_with(settings, &layout, &span),
        sites::stamps_with(settings, &layout, &span),
    )
}

/// Draws the machinery behind the rigs (see the module docs).
pub(super) fn draw_machinery(draw: &mut impl RaylibDraw, scene: &CityScene<'_>) {
    if !view_touches(scene) {
        return;
    }
    let (movers, stamps) = gather(scene);
    for stamp in &stamps {
        match stamp.kind {
            CityStampKind::GreatEngine => draw_flywheel(draw, scene, stamp),
            CityStampKind::Orrery => draw_orrery(draw, scene, stamp),
            CityStampKind::Avenue if stamp.tram => draw_wire(draw, stamp),
            _ => {}
        }
    }
    draw_belts(draw, scene);
    let millis = scene.millis();
    for mover in &movers {
        match mover {
            CityMover::Tram { rail_row, .. } => {
                if let Some(state) = mover.track_at(millis) {
                    draw_tram(draw, scene, milli_x(state.pos), *rail_row, state.vel != 0);
                }
            }
            CityMover::Lift { column, track, .. } => {
                if let Some(state) = mover.track_at(millis) {
                    let top = track.stops.first().copied().unwrap_or(0);
                    draw_lift(draw, *column, milli_y(state.pos), milli_y(top));
                }
            }
            CityMover::Piston {
                column, floor_row, ..
            } => {
                if let Some((top, _, phase)) = mover.piston_top(millis) {
                    draw_piston(draw, scene, *column, milli_y(top), *floor_row, phase);
                }
            }
            CityMover::Crusher {
                column,
                housing_row,
                ..
            } => {
                if let Some((bottom, phase)) = mover.crusher_bottom(millis) {
                    draw_crusher(draw, scene, *column, *housing_row, milli_y(bottom), phase);
                }
            }
        }
    }
    draw_plinth_light(draw, scene);
}

/// The overhead wire a tram runs under: a thin line under the avenue's ceiling, hangers to it.
fn draw_wire(draw: &mut impl RaylibDraw, stamp: &CityStamp) {
    let frame = crate::view_frame::frame();
    let y = frame.row_px(stamp.floor - TRAM_CAR_ROWS - 3) + TILE_SIZE * 0.5;
    let x0 = frame.col_px(stamp.bounds.min.x + 2);
    let x1 = frame.col_px(stamp.bounds.max.x - 1);
    draw.draw_rectangle_rec(
        Rectangle::new(x0, y, x1 - x0, ART),
        with_alpha(STEEL_DARK, 230),
    );
    let hangers = ((x1 - x0) / (TILE_SIZE * 6.0)).max(0.0) as i32;
    for hanger in 0..=hangers {
        let x = (hanger as f32).mul_add(TILE_SIZE * 6.0, x0);
        draw.draw_rectangle_rec(
            Rectangle::new(x, y - TILE_SIZE * 0.5, ART, TILE_SIZE * 0.5),
            with_alpha(STEEL_DARK, 160),
        );
    }
}

/// A tram car centred on `x`, its floor on `rail_row`'s top.
fn draw_tram(
    draw: &mut impl RaylibDraw,
    scene: &CityScene<'_>,
    x: f32,
    rail_row: i32,
    moving: bool,
) {
    let frame = crate::view_frame::frame();
    let width = TILE_SIZE * f32::from(i8::try_from(TRAM_CAR_TILES).unwrap_or(5));
    let height = TILE_SIZE * f32::from(i8::try_from(TRAM_CAR_ROWS).unwrap_or(3));
    let floor = frame.row_px(rail_row);
    let pen = Pen::at(x - width * 0.5, floor - height);
    let w = (width / ART) as i32;
    let h = (height / ART) as i32;
    // The pantograph up to the wire.
    pen.rect(draw, w / 2 - 1, -12, 2, 12, STEEL_DARK);
    pen.rect(draw, w / 2 - 8, -13, 16, 2, STEEL);
    // The body: cream over teal, a dark window band, rounded ends.
    pen.rect(draw, 2, 0, w - 4, h - 4, INK);
    pen.rect(draw, 1, 2, w - 2, h - 7, INK);
    pen.rect(draw, 3, 1, w - 6, h - 6, CREAM);
    pen.rect(draw, 2, 3, w - 4, h - 9, CREAM);
    pen.rect(draw, 2, h - 18, w - 4, 10, TEAL);
    pen.rect(draw, 2, h - 18, w - 4, 1, TEAL_DARK);
    // Windows (the rig shows through the middle one: it is drawn after the car).
    for (index, wx) in [5, 20, 46, 61].into_iter().enumerate() {
        let lit = index % 2 == 0;
        pen.rect(draw, wx, 6, 12, 12, if lit { AMBER } else { SLATE_DARK });
        pen.rect(draw, wx, 6, 12, 1, INK);
    }
    pen.rect(draw, 33, 5, 12, 20, with_alpha(SLATE_DARK, 170));
    // The destination sign and headlamps.
    pen.rect(draw, w / 2 - 6, 2, 12, 3, NIGHT);
    pen.rect(draw, w / 2 - 4, 3, 8, 1, if moving { GOLD } else { MINT });
    let lamp = if moving { CREAM } else { AMBER };
    pen.rect(draw, 1, h - 12, 2, 3, lamp);
    pen.rect(draw, w - 3, h - 12, 2, 3, lamp);
    // Bogies on the rail.
    for bx in [6, w - 18] {
        pen.rect(draw, bx, h - 5, 12, 3, STEEL_DARK);
        pen.rect(draw, bx + 1, h - 3, 3, 3, INK);
        pen.rect(draw, bx + 8, h - 3, 3, 3, INK);
    }
    if moving && scene.effects > 0.0 {
        // A spark where the pantograph touches the wire, now and then.
        if (scene.seconds * 3.0).fract() < 0.15 {
            pen.rect(draw, w / 2 - 1, -14, 2, 2, ICE);
        }
    }
}

/// A lift cab with its floor top at `floor_y`, its cables up to `top_y`.
fn draw_lift(draw: &mut impl RaylibDraw, column: i32, floor_y: f32, top_y: f32) {
    let frame = crate::view_frame::frame();
    let width = TILE_SIZE * f32::from(i8::try_from(LIFT_CAB_TILES).unwrap_or(3));
    let height = TILE_SIZE * f32::from(i8::try_from(LIFT_CAB_ROWS).unwrap_or(3));
    let left = frame.col_px(column - LIFT_CAB_TILES / 2);
    let cable_top = top_y - TILE_SIZE * 4.0;
    // Cables up the shaft.
    for cx in [left + width * 0.3, left + width * 0.7] {
        draw.draw_rectangle_rec(
            Rectangle::new(cx, cable_top, ART, (floor_y - height - cable_top).max(0.0)),
            with_alpha(STEEL_DARK, 220),
        );
    }
    let pen = Pen::at(left, floor_y - height);
    let w = (width / ART) as i32;
    let h = (height / ART) as i32;
    // A brass cage: roof, floor, bars.
    pen.rect(draw, 0, 0, w, 3, INK);
    pen.rect(draw, 1, 1, w - 2, 1, GOLD);
    pen.rect(draw, 0, h - 3, w, 3, INK);
    pen.rect(draw, 1, h - 3, w - 2, 1, AMBER);
    for bx in (0..w).step_by(6) {
        pen.rect(draw, bx, 3, 1, h - 6, with_alpha(AMBER, 200));
    }
    pen.rect(draw, w - 1, 3, 1, h - 6, with_alpha(AMBER, 200));
    // A little lamp under the roof.
    pen.rect(draw, w / 2 - 1, 3, 2, 2, CREAM);
}

/// A piston head (its top at `top_y`) on its ram down to its housing on `floor_row`.
fn draw_piston(
    draw: &mut impl RaylibDraw,
    scene: &CityScene<'_>,
    column: i32,
    top_y: f32,
    floor_row: i32,
    phase: StrokePhase,
) {
    let frame = crate::view_frame::frame();
    let width = TILE_SIZE * f32::from(i8::try_from(PISTON_TILES).unwrap_or(3));
    let left = frame.col_px(column - PISTON_TILES / 2);
    let floor_y = frame.row_px(floor_row);
    let pen = Pen::at(left, top_y);
    let w = (width / ART) as i32;
    // The ram: a polished rod from under the head to the housing.
    let ram = ((floor_y - top_y) / ART) as i32;
    if ram > 4 {
        pen.rect(draw, w / 2 - 6, 4, 12, ram - 4, INK);
        pen.rect(draw, w / 2 - 5, 4, 10, ram - 4, STEEL);
        pen.rect(draw, w / 2 - 4, 4, 2, ram - 4, STEEL_LIGHT);
    }
    // The head: a steel slab with a hazard edge.
    pen.rect(draw, 0, 0, w, 5, INK);
    pen.rect(draw, 1, 1, w - 2, 3, STEEL);
    for stripe in (0..w - 2).step_by(6) {
        pen.rect(draw, 1 + stripe, 3, 3, 1, GOLD);
    }
    // The telegraph: steam and an amber lamp before it rises.
    let housing = Pen::at(left, floor_y);
    if phase == StrokePhase::Tell {
        let glow = pulse(scene, 2.0);
        disc(
            draw,
            left + width * 0.5,
            floor_y + TILE_SIZE * 0.4,
            3,
            with_alpha(AMBER, (120.0 + 120.0 * glow) as u8),
        );
        if scene.effects > 0.0 {
            for puff in 0..3 {
                let t = (scene.seconds * 1.5 + puff as f32 * 0.33).fract();
                disc(
                    draw,
                    left + width * (0.2 + 0.3 * puff as f32),
                    floor_y - t * TILE_SIZE * 1.2,
                    2 + (t * 3.0) as i32,
                    with_alpha(MIST, (90.0 * (1.0 - t)) as u8),
                );
            }
        }
    } else {
        housing.rect(
            draw,
            w / 2 - 1,
            2,
            2,
            2,
            if phase == StrokePhase::Out {
                GOLD
            } else {
                RUST
            },
        );
    }
}

/// A crusher block (its bottom at `bottom_y`) under its housing on `housing_row`.
fn draw_crusher(
    draw: &mut impl RaylibDraw,
    scene: &CityScene<'_>,
    column: i32,
    housing_row: i32,
    bottom_y: f32,
    phase: StrokePhase,
) {
    let frame = crate::view_frame::frame();
    let width = TILE_SIZE * f32::from(i8::try_from(CRUSHER_TILES.0).unwrap_or(3));
    let height = TILE_SIZE * f32::from(i8::try_from(CRUSHER_TILES.1).unwrap_or(2));
    let left = frame.col_px(column - CRUSHER_TILES.0 / 2);
    let top_of_rods = frame.row_px(housing_row + 1);
    let block_top = bottom_y - height;
    let w = (width / ART) as i32;
    // Guide rods from the housing.
    for rx in [left + width * 0.2, left + width * 0.8 - ART] {
        draw.draw_rectangle_rec(
            Rectangle::new(rx, top_of_rods, ART, (block_top - top_of_rods).max(0.0)),
            STEEL_DARK,
        );
    }
    let pen = Pen::at(left, block_top);
    let h = (height / ART) as i32;
    pen.rect(draw, 0, 0, w, h, INK);
    pen.rect(draw, 1, 1, w - 2, h - 5, STEEL_DARK);
    pen.rect(draw, 1, 1, w - 2, 2, STEEL);
    // Teeth along the bottom.
    for tooth in (1..w - 1).step_by(4) {
        pen.rect(draw, tooth, h - 4, 2, 3, STEEL_LIGHT);
    }
    // The warning lamp on the housing: it warms through amber before the slam, red as it falls.
    let lamp_x = left + width * 0.5;
    let lamp_y = frame.row_px(housing_row) + TILE_SIZE * 0.5;
    let colour = match phase {
        StrokePhase::Tell => {
            let warm = pulse(scene, 1.6);
            Some(with_alpha(AMBER, (110.0 + 140.0 * warm) as u8))
        }
        StrokePhase::Out | StrokePhase::Held => Some(with_alpha(RED, 220)),
        _ => None,
    };
    if let Some(colour) = colour {
        disc(draw, lamp_x, lamp_y, 4, with_alpha(colour, 60));
        disc(draw, lamp_x, lamp_y, 2, colour);
    }
}

/// Chevrons running along the belts in view.
fn draw_belts(draw: &mut impl RaylibDraw, scene: &CityScene<'_>) {
    let frame = crate::view_frame::frame();
    let span = tile_span(scene.visible, 1);
    let shift = (scene.seconds * fatherload_simulation::ancient_city::BELT_SPEED / ART) as i32;
    for y in span.min.y..=span.max.y {
        for x in span.min.x..=span.max.x {
            let at = TilePosition { x, y };
            let Some(TileKind::City(tile)) = scene.terrain.tile(at).map(|found| found.kind) else {
                continue;
            };
            let Some(dir) = tile.belt() else {
                continue;
            };
            let pen = Pen::at(frame.col_px(x), frame.row_px(y));
            for chevron in 0..2 {
                let offset = (chevron * 8 + shift * i32::from(dir)).rem_euclid(16);
                pen.rect(draw, offset, 0, 2, 1, with_alpha(GOLD, 200));
            }
        }
    }
}

/// Unread chart plinths throw a column of gold light.
fn draw_plinth_light(draw: &mut impl RaylibDraw, scene: &CityScene<'_>) {
    let frame = crate::view_frame::frame();
    let span = tile_span(scene.visible, 1);
    for y in span.min.y..=span.max.y {
        for x in span.min.x..=span.max.x {
            let at = TilePosition { x, y };
            if scene.terrain.tile(at).map(|found| found.kind)
                != Some(TileKind::City(CityTile::ChartPlinth))
            {
                continue;
            }
            let glow = pulse(scene, 0.5);
            let cx = frame.col_px(x) + TILE_SIZE * 0.5;
            let base = frame.row_px(y);
            for step in 0..6 {
                let alpha = (70.0 - step as f32 * 10.0) * (0.6 + 0.4 * glow);
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        cx - 5.0,
                        base - TILE_SIZE * (step as f32 + 1.0),
                        10.0,
                        TILE_SIZE,
                    ),
                    with_alpha(GOLD, alpha.max(0.0) as u8),
                );
            }
            disc(
                draw,
                cx,
                base + 6.0,
                6,
                with_alpha(CREAM, (40.0 + 40.0 * glow) as u8),
            );
        }
    }
}

/// The Great Engine's flywheel: an iron wheel turning on the beat over the hall's belts.
fn draw_flywheel(draw: &mut impl RaylibDraw, scene: &CityScene<'_>, stamp: &CityStamp) {
    let frame = crate::view_frame::frame();
    let floor = stamp.bounds.max.y - 1;
    let cx = frame.col_px(stamp.bounds.center().x) + TILE_SIZE * 0.5;
    let cy = frame.row_px(floor - 8);
    let radius = TILE_SIZE * 6.5;
    let beat =
        fatherload_simulation::ancient_city::engine_beat_millis(stamp.along) as f32 / 1_000.0;
    let turn = scene.millis() as f32 / 1_000.0 / beat * std::f32::consts::TAU / 8.0;
    // Rim, spokes and hub, iron with brass bolts.
    draw.draw_ring(
        Vector2::new(cx, cy),
        radius - 10.0,
        radius,
        0.0,
        360.0,
        48,
        with_alpha(STEEL_DARK, 235),
    );
    draw.draw_ring(
        Vector2::new(cx, cy),
        radius - 3.0,
        radius - 1.0,
        0.0,
        360.0,
        48,
        with_alpha(STEEL, 200),
    );
    for spoke in 0..8 {
        let angle = turn + spoke as f32 * std::f32::consts::TAU / 8.0;
        let (sin, cos) = angle.sin_cos();
        draw.draw_line_ex(
            Vector2::new(cx, cy),
            Vector2::new(cx + cos * (radius - 8.0), cy + sin * (radius - 8.0)),
            6.0,
            with_alpha(STEEL_DARK, 230),
        );
        disc(
            draw,
            cx + cos * (radius - 5.0),
            cy + sin * (radius - 5.0),
            1,
            GOLD,
        );
    }
    disc(draw, cx, cy, 8, STEEL);
    disc(draw, cx, cy, 4, GOLD);
}

/// The Orrery's spheres circling its hall on their arms.
fn draw_orrery(draw: &mut impl RaylibDraw, scene: &CityScene<'_>, stamp: &CityStamp) {
    let frame = crate::view_frame::frame();
    let centre = stamp.bounds.center();
    let (cx, cy) = (
        frame.col_px(centre.x) + TILE_SIZE * 0.5,
        frame.row_px(centre.y - 3),
    );
    let t = scene.millis() as f32 / 1_000.0;
    // The hanging stone at the heart, and the line of light running out from it.
    disc(draw, cx, cy - TILE_SIZE * 6.0, 5, STONE_LIGHT);
    draw.draw_line_ex(
        Vector2::new(cx, cy - TILE_SIZE * 6.0),
        Vector2::new(cx + TILE_SIZE * 24.0, cy - TILE_SIZE * 9.0),
        2.0,
        with_alpha(GOLD, 110),
    );
    for (index, (rx, ry, speed, colour)) in [
        (5.0, 2.0, 0.21, GOLD),
        (9.0, 3.2, 0.13, SKY_LIGHT),
        (13.0, 4.5, 0.08, ORANGE),
        (17.0, 6.0, 0.05, LAVENDER),
        (21.0, 7.2, 0.03, MINT),
    ]
    .into_iter()
    .enumerate()
    {
        let (rx, ry) = (rx * TILE_SIZE, ry * TILE_SIZE);
        draw.draw_ellipse_lines(cx as i32, cy as i32, rx, ry, with_alpha(SLATE, 90));
        let angle = t * speed * std::f32::consts::TAU + index as f32 * 1.3;
        let (sin, cos) = angle.sin_cos();
        let (px, py) = (cx + cos * rx, cy + sin * ry);
        draw.draw_line_ex(
            Vector2::new(cx, cy),
            Vector2::new(px, py),
            1.0,
            with_alpha(STEEL, 70),
        );
        disc(draw, px, py, 3 + (index % 2) as i32, colour);
    }
}

/// Every player's archivist (see the module docs) and the seat's scan marks.
pub(super) fn draw_archivists(draw: &mut impl RaylibDraw, scene: &CityScene<'_>) {
    if scene.city.archivists.is_empty() {
        return;
    }
    let millis = scene.millis();
    let tick_ms = 1_000 / u64::from(scene.tick_rate_hz.max(1));
    for (player_id, drone) in &scene.city.archivists {
        // Only drones near a drawn rig (the drone's own point is the simulation's).
        let Some(player) = scene
            .players
            .iter()
            .find(|player| player.player_id == *player_id)
        else {
            continue;
        };
        let _ = player;
        let bob = (scene.seconds * 2.2 + f32::from(player_id.get() as u8) * 0.9).sin()
            * 3.0
            * scene.effects.clamp(0.3, 1.0);
        let speaking = drone.line.is_some_and(|(_, tick)| {
            millis.saturating_sub(tick.get() * tick_ms) < archivist::LINE_SHOW_MILLIS
        });
        let frame_index = if speaking && (scene.seconds * 6.0) as i32 % 2 == 0 {
            2
        } else {
            art::ARCHIVIST.frame_at(scene.seconds)
        };
        let width = art::ARCHIVIST.width() as f32 * ART;
        let height = art::ARCHIVIST.height() as f32 * ART;
        let (x, y) = (drone.x, drone.y + bob);
        disc(
            draw,
            x,
            y,
            9,
            with_alpha(MINT, if speaking { 40 } else { 22 }),
        );
        draw_world_sprite(
            draw,
            &art::ARCHIVIST,
            frame_index,
            x - width * 0.5,
            y - height * 0.5,
            player.x < x,
        );
        if speaking {
            // A small speech glyph over its head (the words are in the seat's HUD).
            let pen = Pen::at(x - 6.0, y - height * 0.5 - 14.0);
            pen.rect(draw, 0, 0, 7, 5, INK);
            pen.rect(draw, 1, 1, 5, 3, CREAM);
            pen.rect(draw, 2, 5, 2, 1, INK);
            for dot in 0..3 {
                pen.rect(draw, 1 + dot * 2, 2, 1, 1, SLATE);
            }
        }
        // The seat's own scan marks: brackets round the ore it found, fading.
        if Some(*player_id) == scene.local {
            let age = millis.saturating_sub(drone.scan_tick.get() * tick_ms);
            if age < archivist::SCAN_SHOW_MILLIS {
                let fade = 1.0 - age as f32 / archivist::SCAN_SHOW_MILLIS as f32;
                draw_scan_marks(draw, &drone.scan, fade);
            }
        }
    }
}

/// Brackets round each scanned tile.
fn draw_scan_marks(draw: &mut impl RaylibDraw, tiles: &[TilePosition], fade: f32) {
    let frame = crate::view_frame::frame();
    let colour = with_alpha(MINT, (220.0 * fade) as u8);
    for tile in tiles {
        let pen = Pen::at(frame.col_px(tile.x), frame.row_px(tile.y));
        for (x, y, w, h) in [
            (-1, -1, 4, 1),
            (-1, -1, 1, 4),
            (13, -1, 4, 1),
            (16, -1, 1, 4),
            (-1, 16, 4, 1),
            (-1, 13, 1, 4),
            (13, 16, 4, 1),
            (16, 13, 1, 4),
        ] {
            pen.rect(draw, x, y, w, h, colour);
        }
    }
}
