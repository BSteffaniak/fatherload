//! Side-view tomb traps in motion (WP-TOMBS T28): every moving part is drawn from the same pure
//! functions of the world clock the simulation hurts with (`fatherload_simulation::tomb::traps`),
//! so what you see is exactly what hits.
//!
//! * Blades swing on their chains with a faint motion trail.
//! * Crushers shake and shed dust (the tell), slam their toothed block down and wind it back.
//! * Dart throwers' eyes glow before each dart; darts streak across the room.
//! * Sluice rooms trickle (the tell), then fill with sand (water in sky and lunar vaults),
//!   hold and drain.
//! * Cursed vents gather wisps (the tell), then burst into a cloud of grave fog.
//! * Crumbling slabs shake and crack, then fall in a shower of grit.
//! * With the Trap-Sight Lantern, each trap's reach is outlined: a blade's sweep, a crusher's
//!   column and a dart's flight line.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "world drawing mixes tile indices and float pixels"
)]

use fatherload_simulation::{
    DiscoveredTiles,
    base::BuiltTile,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    tomb::{
        SiteStyle, Tombs,
        culture::TombCulture,
        side_view::dart_facing,
        traps::{
            BLADE_RADIUS, CRUMBLE_MILLIS, CRUSHER_MAX_REACH, CrusherStage, DART_MAX_REACH,
            FOG_RADIUS, FogStage, SandStage, TrapKind, blade_angle, blade_pivot, blade_tip,
            crusher_state, dart_position, dart_state, fog_centre, fog_state, sand_state,
            sand_surface,
        },
    },
    world::pilot::pilot_blocks,
    worldgen::tomb_sites,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::{culture_colors, masonry_near};

use super::{ART, fx::disc, snap};

/// What one view needs to draw the moving traps.
pub(super) struct TrapScene<'a> {
    pub terrain: &'a Terrain,
    pub tombs: &'a Tombs,
    pub explored: Option<&'a DiscoveredTiles>,
    pub visible: Rectangle,
    /// World milliseconds of the drawn moment (interpolated between ticks).
    pub millis: u64,
    /// A pilot in view carries the Trap-Sight Lantern.
    pub trap_sight: bool,
}

fn culture_at(terrain: &Terrain, tile: TilePosition) -> TombCulture {
    masonry_near(terrain, tile)
        .and_then(TombCulture::of_masonry)
        .unwrap_or_default()
}

fn open(terrain: &Terrain, tile: TilePosition) -> bool {
    !pilot_blocks(terrain, tile)
}

fn line(draw: &mut impl RaylibDraw, from: (f32, f32), to: (f32, f32), width: f32, color: Color) {
    draw.draw_line_ex(
        Vector2::new(from.0, from.1),
        Vector2::new(to.0, to.1),
        width,
        color,
    );
}

/// Draws every moving trap of the side-view runs in view.
pub(super) fn draw_side_traps(draw: &mut impl RaylibDraw, scene: &TrapScene<'_>) {
    for run in scene.tombs.runs.values() {
        if run.site.style != SiteStyle::SideView {
            continue;
        }
        let origin = tomb_sites::slot_origin(run.slot);
        let left = (crate::view_frame::frame().col(scene.visible.x) - 6).max(origin.x);
        let top = (crate::view_frame::frame().row(scene.visible.y) - 8).max(origin.y);
        let right = (crate::view_frame::frame().col_ceil(scene.visible.x + scene.visible.width)
            + 6)
        .min(origin.x + tomb_sites::SLOT_SIZE.0);
        let bottom = (crate::view_frame::frame().row_ceil(scene.visible.y + scene.visible.height)
            + 2)
        .min(origin.y + tomb_sites::SLOT_SIZE.1);
        if left >= right || top >= bottom {
            continue;
        }
        // Sluice rooms first (the sand is behind everything else).
        for room in &run.rooms {
            let has_sluice = (room.interior.min.x..=room.interior.max.x).any(|x| {
                matches!(
                    scene.terrain.tile(TilePosition { x, y: room.interior.max.y + 1 }).map(|found| found.kind),
                    Some(TileKind::Built(BuiltTile::TombTrap { kind, .. }))
                        if TrapKind::from_index(kind) == TrapKind::SandSluice
                )
            });
            if has_sluice
                && scene
                    .explored
                    .is_none_or(|explored| explored.contains(room.interior.min))
            {
                draw_sand(draw, scene, room.interior);
            }
        }
        for y in top..bottom {
            for x in left..right {
                let tile = TilePosition { x, y };
                let Some(TileKind::Built(BuiltTile::TombTrap { kind, hidden })) =
                    scene.terrain.tile(tile).map(|found| found.kind)
                else {
                    continue;
                };
                if scene
                    .explored
                    .is_some_and(|explored| !explored.contains(tile))
                {
                    continue;
                }
                match TrapKind::from_index(kind) {
                    TrapKind::Blade => draw_blade(draw, scene, tile),
                    TrapKind::Crusher => draw_crusher(draw, scene, tile),
                    TrapKind::DartWall => draw_dart(draw, scene, tile, dart_facing(hidden)),
                    TrapKind::FogVent => draw_fog(draw, scene, tile),
                    _ => {}
                }
            }
        }
        for slab in &run.crumbling {
            draw_crumbling(draw, scene, slab);
        }
    }
}

fn draw_blade(draw: &mut impl RaylibDraw, scene: &TrapScene<'_>, tile: TilePosition) {
    let culture = culture_at(scene.terrain, tile);
    let [_, chain_dark, chain, shine] = culture_colors(if culture == TombCulture::LunarVault {
        TombCulture::LunarVault
    } else {
        TombCulture::Catacomb
    });
    let pivot = blade_pivot(crate::view_frame::frame(), tile);
    let blade_half = BLADE_RADIUS;
    if scene.trap_sight {
        // The blade's sweep, faint.
        for step in 0..=20 {
            let millis = scene.millis + step * 65;
            let (x, y) = blade_tip(crate::view_frame::frame(), tile, millis);
            disc(draw, x, y, 1, with_alpha(ROSE, 70));
        }
    }
    // A short trail behind the blade.
    for (back, alpha) in [(70, 40), (35, 80)] {
        let (x, y) = blade_tip(
            crate::view_frame::frame(),
            tile,
            scene.millis.saturating_sub(back),
        );
        draw.draw_circle_v(
            Vector2::new(x, y),
            BLADE_RADIUS,
            with_alpha(STEEL_LIGHT, alpha),
        );
    }
    let tip = blade_tip(crate::view_frame::frame(), tile, scene.millis);
    line(draw, pivot, tip, ART * 1.5, chain_dark);
    // Chain links.
    for link in 1..8 {
        let t = link as f32 / 8.0;
        let at = (
            (tip.0 - pivot.0).mul_add(t, pivot.0),
            (tip.1 - pivot.1).mul_add(t, pivot.1),
        );
        disc(draw, at.0, at.1, 1, chain);
    }
    // The blade: a broad steel crescent hung across the arm, its edge facing the swing.
    let angle = blade_angle(tile, scene.millis);
    // Raylib angles are degrees clockwise from +x (y down): straight down is 90.
    let down = 90.0 - angle.to_degrees();
    let centre = Vector2::new(tip.0, tip.1);
    draw.draw_circle_sector(centre, blade_half + 6.0, down - 80.0, down + 80.0, 16, INK);
    draw.draw_circle_sector(
        centre,
        blade_half + 4.0,
        down - 76.0,
        down + 76.0,
        16,
        STEEL,
    );
    draw.draw_circle_sector(
        centre,
        blade_half - 2.0,
        down - 76.0,
        down + 76.0,
        16,
        STEEL_DARK,
    );
    draw.draw_ring(
        centre,
        blade_half + 2.0,
        blade_half + 4.0,
        down - 70.0,
        down + 70.0,
        16,
        shine,
    );
    disc(draw, tip.0, tip.1, 2, chain_dark);
    disc(draw, pivot.0, pivot.1, 2, COPPER);
}

fn crusher_reach(terrain: &Terrain, tile: TilePosition) -> i32 {
    (1..=CRUSHER_MAX_REACH)
        .take_while(|dy| {
            open(
                terrain,
                TilePosition {
                    x: tile.x,
                    y: tile.y + dy,
                },
            )
        })
        .count() as i32
}

fn draw_crusher(draw: &mut impl RaylibDraw, scene: &TrapScene<'_>, tile: TilePosition) {
    let reach = crusher_reach(scene.terrain, tile);
    let (stage, extension) = crusher_state(tile, scene.millis);
    let left = crate::view_frame::frame().rel_col(tile.x) * TILE_SIZE + 2.0;
    let top = crate::view_frame::frame().rel_row(tile.y + 1) * TILE_SIZE;
    let width = TILE_SIZE - 4.0;
    let shake = if stage == CrusherStage::Shaking {
        if (scene.millis / 40).is_multiple_of(2) {
            ART
        } else {
            -ART
        }
    } else {
        0.0
    };
    if scene.trap_sight {
        draw.draw_rectangle_lines_ex(
            Rectangle::new(left, top, width, reach as f32 * TILE_SIZE),
            2.0,
            with_alpha(ROSE, 90),
        );
    }
    if stage == CrusherStage::Shaking {
        // Dust sifting down from the housing: the tell.
        for grain in 0..6_u64 {
            let fall = ((scene.millis / 20 + grain * 7) % 24) as f32;
            let x = left + 3.0 + (grain * 5 % 26) as f32;
            disc(draw, x, top + fall * ART, 0, with_alpha(SAND, 200));
        }
    }
    let length = (reach as f32 * TILE_SIZE * extension).max(ART * 3.0);
    let block = Rectangle::new(left + shake, top - ART * 3.0, width, length);
    // Chains up into the housing.
    line(
        draw,
        (left + 6.0 + shake, top - 4.0),
        (left + 6.0 + shake, block.y + block.height - 10.0),
        2.0,
        STEEL_DARK,
    );
    line(
        draw,
        (left + width - 6.0 + shake, top - 4.0),
        (left + width - 6.0 + shake, block.y + block.height - 10.0),
        2.0,
        STEEL_DARK,
    );
    // The iron weight: a whole tile tall once it has room, riveted, teeth underneath.
    let face_height = (TILE_SIZE - 4.0).min(block.height);
    let face = Rectangle::new(
        block.x,
        block.y + block.height - face_height,
        block.width,
        face_height,
    );
    draw.draw_rectangle_rec(face, STEEL_DARK);
    draw.draw_rectangle_lines_ex(face, 2.0, INK);
    draw.draw_rectangle_rec(
        Rectangle::new(face.x + 2.0, face.y + 2.0, face.width - 4.0, 3.0),
        STEEL_LIGHT,
    );
    // Teeth.
    for tooth in 0..4 {
        let x = face.x + 2.0 + tooth as f32 * (face.width - 4.0) / 4.0;
        let base = face.y + face.height;
        draw.draw_triangle(
            Vector2::new(x, base),
            Vector2::new(x + (face.width - 4.0) / 8.0, base + 5.0),
            Vector2::new(x + (face.width - 4.0) / 4.0, base),
            STEEL_LIGHT,
        );
    }
    if stage == CrusherStage::Down {
        // The slam's dust puff on the floor.
        let floor = top + reach as f32 * TILE_SIZE;
        for side in [-1.0_f32, 1.0] {
            disc(
                draw,
                side.mul_add(width * 0.7, left + width * 0.5),
                floor - 4.0,
                3,
                with_alpha(SAND, 90),
            );
        }
    }
}

fn draw_dart(draw: &mut impl RaylibDraw, scene: &TrapScene<'_>, tile: TilePosition, facing: i8) {
    let reach = (1..=DART_MAX_REACH)
        .take_while(|dx| {
            open(
                scene.terrain,
                TilePosition {
                    x: tile.x + dx * i32::from(facing),
                    y: tile.y,
                },
            )
        })
        .count() as i32;
    let (charge, _) = dart_state(tile, scene.millis);
    let mouth = (
        if facing < 0 {
            crate::view_frame::frame().rel_col(tile.x) * TILE_SIZE + 2.0
        } else {
            crate::view_frame::frame().rel_col(tile.x + 1) * TILE_SIZE - 2.0
        },
        (crate::view_frame::frame().rel_row(tile.y) + 0.6) * TILE_SIZE,
    );
    if scene.trap_sight {
        let end = mouth.0 + f32::from(facing) * reach as f32 * TILE_SIZE;
        line(draw, mouth, (end, mouth.1), 1.0, with_alpha(ROSE, 90));
    }
    if charge > 0.0 {
        // The eye charging: a growing ember.
        let radius = 1 + (charge * 3.0) as i32;
        disc(draw, mouth.0, mouth.1, radius + 1, with_alpha(RED, 70));
        disc(draw, mouth.0, mouth.1, radius, with_alpha(AMBER, 160));
    }
    if let Some((x, y)) = dart_position(
        crate::view_frame::frame(),
        tile,
        facing,
        reach,
        scene.millis,
    ) {
        let back = x - f32::from(facing) * 12.0;
        line(draw, (back, y), (x, y), 2.0, LOAM);
        line(
            draw,
            (back, y - 2.0),
            (back - f32::from(facing) * 3.0, y - 3.0),
            1.0,
            BONE,
        );
        line(
            draw,
            (back, y + 2.0),
            (back - f32::from(facing) * 3.0, y + 3.0),
            1.0,
            BONE,
        );
        draw.draw_triangle(
            Vector2::new(x + f32::from(facing) * 5.0, y),
            Vector2::new(x, y - 3.0 * f32::from(facing)),
            Vector2::new(x, y + 3.0 * f32::from(facing)),
            STEEL_LIGHT,
        );
        // A faint streak behind.
        line(
            draw,
            (back - f32::from(facing) * 20.0, y),
            (back, y),
            1.0,
            with_alpha(STEEL_LIGHT, 60),
        );
    }
}

fn draw_fog(draw: &mut impl RaylibDraw, scene: &TrapScene<'_>, tile: TilePosition) {
    let (stage, size) = fog_state(tile, scene.millis);
    let (cx, cy) = fog_centre(crate::view_frame::frame(), tile);
    let vent = (
        (crate::view_frame::frame().rel_col(tile.x) + 0.5) * TILE_SIZE,
        crate::view_frame::frame().rel_row(tile.y) * TILE_SIZE,
    );
    match stage {
        FogStage::Calm => {
            disc(draw, vent.0, vent.1 - 2.0, 3, with_alpha(LIME, 26));
        }
        FogStage::Gathering => {
            // Wisps curling up out of the vent: the tell.
            for wisp in 0..5_u64 {
                let rise = ((scene.millis / 12 + wisp * 37) % 60) as f32 / 60.0;
                let sway = ((wisp as f32).mul_add(1.7, rise * 6.0)).sin() * 6.0;
                disc(
                    draw,
                    vent.0 + sway,
                    vent.1 - rise * TILE_SIZE * 1.6,
                    1 + (rise * 2.0) as i32,
                    with_alpha(LIME, (150.0 * (1.0 - rise)) as u8),
                );
            }
            disc(
                draw,
                cx,
                cy,
                (FOG_RADIUS * size / ART) as i32,
                with_alpha(MOSS, 50),
            );
        }
        FogStage::Burst | FogStage::Fading => {
            let alpha = if stage == FogStage::Burst { 1.0 } else { size };
            let radius = FOG_RADIUS * if stage == FogStage::Burst { size } else { 1.0 };
            for puff in 0..7_u64 {
                let angle = puff as f32 * 0.9 + scene.millis as f32 / 900.0;
                let (px, py) = (
                    cx + angle.cos() * radius * 0.45,
                    cy + angle.sin() * radius * 0.35,
                );
                draw.draw_circle_v(
                    Vector2::new(px, py),
                    radius * 0.6,
                    with_alpha(MOSS, (70.0 * alpha) as u8),
                );
            }
            draw.draw_circle_v(
                Vector2::new(cx, cy),
                radius * 0.7,
                with_alpha(LIME, (45.0 * alpha) as u8),
            );
            // A ghostly face in the cloud.
            if stage == FogStage::Burst && size >= 1.0 {
                disc(draw, cx - 6.0, cy - 4.0, 1, with_alpha(BONE, 160));
                disc(draw, cx + 6.0, cy - 4.0, 1, with_alpha(BONE, 160));
                disc(draw, cx, cy + 5.0, 2, with_alpha(INK, 120));
            }
        }
    }
}

fn draw_sand(
    draw: &mut impl RaylibDraw,
    scene: &TrapScene<'_>,
    room: fatherload_simulation::worldgen::features::TileRect,
) {
    let (stage, level) = sand_state(room, scene.millis);
    let culture = culture_at(scene.terrain, room.min);
    let water = culture.floods_with_water();
    let (body, top) = if water {
        (with_alpha(OCEAN, 170), with_alpha(SKY_LIGHT, 200))
    } else {
        (with_alpha(SAND, 235), CREAM)
    };
    let bottom = crate::view_frame::frame().rel_row(room.max.y + 1) * TILE_SIZE;
    // The trickle (the tell) and the pour, from every grate.
    if matches!(stage, SandStage::Trickling | SandStage::Rising) {
        for x in room.min.x..=room.max.x {
            let grate = TilePosition {
                x,
                y: room.max.y + 1,
            };
            if !matches!(
                scene.terrain.tile(grate).map(|found| found.kind),
                Some(TileKind::Built(BuiltTile::TombTrap { kind, .. }))
                    if TrapKind::from_index(kind) == TrapKind::SandSluice
            ) {
                continue;
            }
            let cx = (crate::view_frame::frame().rel_col(x) + 0.5) * TILE_SIZE;
            for grain in 0..8_u64 {
                let up = ((scene.millis / 15 + grain * 11) % 40) as f32;
                disc(
                    draw,
                    cx + ((grain % 3) as f32 - 1.0) * 3.0,
                    bottom - up * ART,
                    0,
                    top,
                );
            }
        }
    }
    if level <= 0.0 {
        return;
    }
    let surface = sand_surface(crate::view_frame::frame(), room, level);
    for x in room.min.x..=room.max.x {
        let mut y = room.max.y;
        while y >= room.min.y && (crate::view_frame::frame().rel_row(y) + 1.0) * TILE_SIZE > surface
        {
            let tile = TilePosition { x, y };
            if open(scene.terrain, tile) {
                let cell_top = (crate::view_frame::frame().rel_row(y) * TILE_SIZE).max(surface);
                let cell_bottom = (crate::view_frame::frame().rel_row(y) + 1.0) * TILE_SIZE;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        crate::view_frame::frame().rel_col(x) * TILE_SIZE,
                        cell_top,
                        TILE_SIZE,
                        cell_bottom - cell_top,
                    ),
                    body,
                );
                if cell_top <= surface + 0.5 {
                    let ripple = ((x as f32).mul_add(0.9, scene.millis as f32 / 300.0)).sin() * 1.5;
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            crate::view_frame::frame().rel_col(x) * TILE_SIZE,
                            snap(surface + ripple),
                            TILE_SIZE,
                            ART,
                        ),
                        top,
                    );
                }
            }
            y -= 1;
        }
    }
}

fn draw_crumbling(
    draw: &mut impl RaylibDraw,
    scene: &TrapScene<'_>,
    slab: &fatherload_simulation::tomb::traps::CrumblingSlab,
) {
    let (x, y) = (
        crate::view_frame::frame().rel_col(slab.tile.x) * TILE_SIZE,
        crate::view_frame::frame().rel_row(slab.tile.y) * TILE_SIZE,
    );
    match slab.fell {
        None => {
            let shake = slab.shake(scene.millis);
            let jitter = if (scene.millis / 30).is_multiple_of(2) {
                ART
            } else {
                -ART
            } * shake;
            // Cracks spreading and grit sifting down.
            for crack in 0..3 {
                let cx = x + 6.0 + crack as f32 * 9.0 + jitter;
                line(draw, (cx, y), (cx + 4.0, y + TILE_SIZE * shake), 2.0, INK);
            }
            for grain in 0..4_u64 {
                let fall = ((scene.millis / 18 + grain * 9) % 20) as f32;
                disc(
                    draw,
                    x + 4.0 + grain as f32 * 7.0,
                    y + TILE_SIZE + fall * ART,
                    0,
                    with_alpha(SAND, 200),
                );
            }
        }
        Some(fell) => {
            let since = scene.millis.saturating_sub(fell);
            if since < CRUMBLE_MILLIS {
                // Chunks tumbling into the pit.
                let t = since as f32 / 1_000.0;
                for chunk in 0..5 {
                    let drift = (chunk as f32 - 2.0) * 4.0;
                    disc(
                        draw,
                        x + 16.0 + drift * (1.0 + t * 4.0),
                        y + 8.0 + 900.0 * t * t,
                        2,
                        with_alpha(CLAY, (220.0 * (1.0 - t * 2.0).max(0.0)) as u8),
                    );
                }
            }
        }
    }
}
