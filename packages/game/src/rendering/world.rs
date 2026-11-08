use raylib::prelude::*;

use drillgame_simulation::{
    DrillDirection, DrillState, FallingBoulder, InfrastructureKind, PlacedBomb,
    PlacedInfrastructure,
    economy::SurfaceZone,
    surface::SURFACE_BUILDINGS,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::terrain::TerrainRenderer;
use crate::{
    effects::SparkParticle,
    render_snapshot::{DangerLevel, PlayerPresentation},
};

pub(super) fn world_camera(camera: Vector2, zoom: f32) -> Camera2D {
    let snapped_camera = Vector2::new(camera.x.round(), camera.y.round());
    Camera2D {
        offset: Vector2::zero(),
        target: snapped_camera,
        rotation: 0.0,
        zoom,
    }
}

pub(super) fn draw_world(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    camera: Vector2,
    terrain_renderer: &TerrainRenderer,
    terrain: &Terrain,
    infrastructure: &[PlacedInfrastructure],
    active_drill: Option<DrillState>,
    update_ticks: u64,
) {
    draw_surface_buildings(draw);
    terrain_renderer.draw(draw, camera);
    draw_infrastructure(draw, infrastructure, update_ticks);

    if let Some(drill) = active_drill {
        let x = (drill.target.x as f32 * TILE_SIZE) as i32;
        let y = (drill.target.y as f32 * TILE_SIZE) as i32;
        let current_durability = terrain
            .tile(drill.target)
            .map_or(drill.initial_durability, |tile| tile.durability);
        let chipped = drill.initial_durability.saturating_sub(current_durability);
        let progress = ((f32::from(chipped) + drill.progress.clamp(0.0, 1.0))
            / f32::from(drill.initial_durability.max(1)))
        .clamp(0.0, 1.0);
        draw.draw_rectangle_lines(
            x - 2,
            y - 2,
            TILE_SIZE as i32 + 4,
            TILE_SIZE as i32 + 4,
            Color::YELLOW,
        );
        if drill.direction == DrillDirection::Down {
            let pulse = (update_ticks as f32 * 0.4).sin().abs();
            draw.draw_circle(
                x + TILE_SIZE as i32 / 2,
                y + TILE_SIZE as i32 / 2,
                10.0 + pulse * 8.0,
                Color::new(255, 180, 70, 85),
            );
        }
        if matches!(terrain.tile(drill.target), Some(tile) if tile.kind == TileKind::Gas) {
            let pulse = (update_ticks as f32 * 0.22).sin().abs();
            draw.draw_circle(
                x + TILE_SIZE as i32 / 2,
                y + TILE_SIZE as i32 / 2,
                10.0 + pulse * 8.0,
                Color::new(95, 230, 90, 80),
            );
        }
        let bar_width = (TILE_SIZE * progress) as i32;
        draw.draw_rectangle(
            x + 4,
            y + TILE_SIZE as i32 - 7,
            TILE_SIZE as i32 - 8,
            3,
            Color::new(0, 0, 0, 120),
        );
        draw.draw_rectangle(
            x + 4,
            y + TILE_SIZE as i32 - 7,
            bar_width.min(TILE_SIZE as i32 - 8),
            3,
            Color::new(255, 215, 90, 220),
        );
        draw.draw_circle(x + 10, y + 11, 2.0, Color::new(255, 235, 150, 170));
        if progress > 0.45 {
            draw.draw_circle(x + 21, y + 18, 2.0, Color::new(255, 235, 150, 150));
        }
    }
}

fn draw_infrastructure(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    infrastructure: &[PlacedInfrastructure],
    update_ticks: u64,
) {
    for item in infrastructure {
        let x = (item.position.x as f32 * TILE_SIZE + TILE_SIZE * 0.5) as i32;
        let y = (item.position.y as f32 * TILE_SIZE + TILE_SIZE * 0.5) as i32;
        let pulse = (update_ticks as f32 * 0.12).sin().abs();
        let color = match item.kind {
            InfrastructureKind::SignalRelay => Color::SKYBLUE,
            InfrastructureKind::SurveyDrone => Color::GREEN,
            InfrastructureKind::CargoLift => Color::GOLD,
            InfrastructureKind::TunnelSupport => Color::ORANGE,
            InfrastructureKind::PumpStation => Color::BLUE,
            InfrastructureKind::OreProcessor => Color::PURPLE,
        };
        draw.draw_circle_lines(x, y, 11.0 + pulse * 3.0, color);
        draw.draw_rectangle(x - 4, y - 10, 8, 20, Color::DARKBLUE);
        draw.draw_line(x, y - 12, x, y - 22, Color::RAYWHITE);
    }
}

fn draw_surface_buildings(draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>) {
    for building in SURFACE_BUILDINGS {
        draw_building(
            draw,
            building.tile_x as f32,
            building.tile_width as f32,
            building_color(building.zone),
            building.label,
        );
    }
}

const fn building_color(zone: SurfaceZone) -> Color {
    match zone {
        SurfaceZone::Fuel => Color::DARKBLUE,
        SurfaceZone::Repair => Color::MAROON,
        SurfaceZone::Depot => Color::DARKGREEN,
        SurfaceZone::Headquarters => Color::DARKPURPLE,
        SurfaceZone::Shop => Color::PURPLE,
        SurfaceZone::Bank => Color::new(25, 110, 70, 255),
        SurfaceZone::Explosives => Color::RED,
        SurfaceZone::Salvage => Color::BROWN,
    }
}

fn draw_building(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    tile_x: f32,
    tile_width: f32,
    color: Color,
    kind: &str,
) {
    let x = tile_x * TILE_SIZE;
    let y = 3.0 * TILE_SIZE;
    let width = tile_width * TILE_SIZE;

    draw.draw_rectangle(x as i32, y as i32, width as i32, 64, color);
    draw.draw_triangle(
        Vector2::new(x, y),
        Vector2::new(x + width * 0.5, y - 24.0),
        Vector2::new(x + width, y),
        Color::new(35, 35, 45, 255),
    );
    draw.draw_rectangle(
        (x + width - 22.0) as i32,
        (y + 18.0) as i32,
        14,
        46,
        Color::new(35, 25, 18, 255),
    );
    draw.draw_rectangle((x + 10.0) as i32, (y + 12.0) as i32, 22, 18, Color::SKYBLUE);
    match kind {
        "FUEL" => draw.draw_circle(
            (x + width - 42.0) as i32,
            (y + 28.0) as i32,
            10.0,
            Color::GOLD,
        ),
        "REPAIR" => {
            draw.draw_rectangle(
                (x + width - 50.0) as i32,
                (y + 23.0) as i32,
                24,
                8,
                Color::RAYWHITE,
            );
            draw.draw_rectangle(
                (x + width - 42.0) as i32,
                (y + 15.0) as i32,
                8,
                24,
                Color::RAYWHITE,
            );
        }
        "DEPOT" => draw.draw_rectangle(
            (x + width - 54.0) as i32,
            (y + 23.0) as i32,
            28,
            20,
            Color::BROWN,
        ),
        "HQ" => draw.draw_triangle(
            Vector2::new(x + width - 52.0, y + 42.0),
            Vector2::new(x + width - 38.0, y + 14.0),
            Vector2::new(x + width - 24.0, y + 42.0),
            Color::SKYBLUE,
        ),
        _ => draw.draw_circle_lines(
            (x + width - 40.0) as i32,
            (y + 30.0) as i32,
            13.0,
            Color::MAGENTA,
        ),
    }
}

pub(super) fn draw_particles(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    lost_cargo: Option<(f32, f32)>,
    boulders: &[FallingBoulder],
    sparks: &[SparkParticle],
) {
    if let Some((x, y)) = lost_cargo {
        draw.draw_rectangle((x - 8.0) as i32, (y - 7.0) as i32, 16, 14, Color::GOLD);
        draw.draw_rectangle_lines((x - 8.0) as i32, (y - 7.0) as i32, 16, 14, Color::BROWN);
        draw.draw_line(
            (x - 8.0) as i32,
            y as i32,
            (x + 8.0) as i32,
            y as i32,
            Color::BROWN,
        );
    }

    for boulder in boulders {
        let wobble = if boulder.warning_seconds > 0.0 {
            (boulder.warning_seconds * 60.0).sin() * 3.0
        } else {
            0.0
        };
        let color = if boulder.warning_seconds > 0.0 {
            Color::new(180, 80, 55, 255)
        } else {
            Color::new(95, 80, 70, 255)
        };
        draw.draw_circle_v(Vector2::new(boulder.x + wobble, boulder.y), 8.0, color);
        draw.draw_circle_lines(boulder.x as i32, boulder.y as i32, 8.0, Color::DARKGRAY);
    }
    for spark in sparks {
        let alpha = (spark.life / 0.45).clamp(0.0, 1.0);
        draw.draw_circle_v(
            Vector2::new(spark.x, spark.y),
            2.5,
            Color::new(255, 190, 60, (220.0 * alpha) as u8),
        );
    }
}

fn drill_visual_offset(active_drill: Option<DrillState>) -> (f32, f32) {
    let Some(drill) = active_drill else {
        return (0.0, 0.0);
    };
    let pulse = (drill.progress * std::f32::consts::TAU * 3.0).sin() * 1.8;
    match drill.direction {
        DrillDirection::Down => (0.0, pulse.abs()),
        DrillDirection::Left => (-pulse.abs(), 0.0),
        DrillDirection::Right => (pulse.abs(), 0.0),
    }
}

pub(super) fn draw_placed_bombs(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    bombs: &[PlacedBomb],
) {
    for bomb in bombs {
        let x = bomb.x as i32;
        let y = bomb.y as i32;
        draw.draw_circle(x, y, 7.0, Color::BLACK);
        draw.draw_circle_lines(x, y, 8.0, Color::RED);
    }
}

pub(super) fn draw_scanner_marks(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    terrain: &Terrain,
    player: &PlayerPresentation,
) {
    if player.scanner_level == 0 {
        return;
    }
    let center = TilePosition {
        x: (player.x / TILE_SIZE).floor() as i32,
        y: (player.y / TILE_SIZE).floor() as i32,
    };
    let radius = 3 + i32::from(player.scanner_level) * 2;
    for y in center.y - radius..=center.y + radius {
        for x in center.x - radius..=center.x + radius {
            if (x - center.x).abs() + (y - center.y).abs() > radius {
                continue;
            }
            let Some(tile) = terrain.tile(TilePosition { x, y }) else {
                continue;
            };
            let color = match tile.kind {
                TileKind::Ore(_) => Color::GOLD,
                TileKind::Artifact(_) if player.scanner_level >= 3 => Color::MAGENTA,
                TileKind::Gas
                | TileKind::Lava
                | TileKind::MagmaVent
                | TileKind::ExplosivePocket
                | TileKind::PressurePocket
                    if player.scanner_level >= 2 =>
                {
                    Color::RED
                }
                _ => continue,
            };
            draw.draw_circle_v(
                Vector2::new(
                    x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
                    y as f32 * TILE_SIZE + TILE_SIZE * 0.5,
                ),
                3.0,
                color,
            );
        }
    }
}

pub(super) fn draw_remote_player(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    player: &PlayerPresentation,
) {
    draw_remote_player_at(draw, player.x, player.y, player.danger_level());
    draw_remote_player_motion_indicator(
        draw,
        player.x,
        player.y,
        player.danger_level(),
        player.velocity_x,
        player.velocity_y,
    );
    draw_remote_player_status(draw, player);
}

#[allow(
    clippy::too_many_lines,
    reason = "remote player rendering includes label, readiness, and survival bars"
)]
pub(super) fn draw_remote_player_at(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    player_x: f32,
    player_y: f32,
    danger_level: DangerLevel,
) {
    let body_color = match danger_level {
        DangerLevel::Nominal => Color::new(80, 170, 245, 180),
        DangerLevel::Warning => Color::new(230, 185, 40, 210),
        DangerLevel::Critical => Color::new(230, 70, 55, 220),
    };
    let outline_color = match danger_level {
        DangerLevel::Nominal => Color::BLUE,
        DangerLevel::Warning => Color::ORANGE,
        DangerLevel::Critical => Color::RED,
    };
    draw.draw_rectangle(
        (player_x - 12.0) as i32,
        (player_y - 9.0) as i32,
        24,
        20,
        body_color,
    );
    draw.draw_rectangle_lines(
        (player_x - 12.0) as i32,
        (player_y - 9.0) as i32,
        24,
        20,
        outline_color,
    );
    draw.draw_rectangle(
        (player_x - 6.0) as i32,
        (player_y - 16.0) as i32,
        12,
        7,
        Color::new(155, 220, 255, 220),
    );
    draw.draw_circle(
        (player_x - 8.0) as i32,
        (player_y + 12.0) as i32,
        3.0,
        Color::DARKBLUE,
    );
    draw.draw_circle(
        (player_x + 8.0) as i32,
        (player_y + 12.0) as i32,
        3.0,
        Color::DARKBLUE,
    );
    draw_remote_player_motion_indicator(draw, player_x, player_y, danger_level, 0.0, 0.0);
}

pub(super) fn draw_remote_player_motion_indicator(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    player_x: f32,
    player_y: f32,
    danger_level: DangerLevel,
    velocity_x: f32,
    velocity_y: f32,
) {
    let speed = (velocity_x.mul_add(velocity_x, velocity_y * velocity_y)).sqrt();
    if speed <= f32::EPSILON {
        return;
    }
    let direction_x = velocity_x / speed;
    let direction_y = velocity_y / speed;
    let trail_color = match danger_level {
        DangerLevel::Nominal => Color::new(120, 220, 255, 150),
        DangerLevel::Warning => Color::new(255, 190, 40, 170),
        DangerLevel::Critical => Color::new(255, 70, 70, 190),
    };
    let trail_length = (speed * 0.12).clamp(12.0, 34.0);
    let start = Vector2::new(player_x - direction_x * 18.0, player_y - direction_y * 18.0);
    let end = Vector2::new(
        player_x - direction_x * (18.0 + trail_length),
        player_y - direction_y * (18.0 + trail_length),
    );
    draw.draw_line_ex(start, end, 4.0, trail_color);
    draw.draw_circle_v(end, 3.0, trail_color);
}

fn draw_remote_player_status(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    player: &PlayerPresentation,
) {
    let label_x = (player.x - 42.0) as i32;
    let label_y = (player.y - 48.0) as i32;
    draw.draw_rectangle(label_x - 4, label_y - 4, 90, 38, Color::new(0, 0, 25, 170));
    draw.draw_rectangle_lines(label_x - 4, label_y - 4, 90, 38, Color::SKYBLUE);
    draw_remote_player_bar(
        draw,
        label_x,
        label_y + 14,
        "Fuel",
        player.fuel,
        100.0,
        Color::LIME,
        Color::ORANGE,
    );
    draw_remote_player_bar(
        draw,
        label_x,
        label_y + 25,
        "Hull",
        player.hull,
        100.0,
        Color::SKYBLUE,
        Color::RED,
    );
}

#[allow(
    clippy::too_many_arguments,
    reason = "tiny immediate-mode bar helper keeps remote player status drawing local"
)]
fn draw_remote_player_bar(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    x: i32,
    y: i32,
    _label: &str,
    value: f32,
    max_value: f32,
    good_color: Color,
    danger_color: Color,
) {
    let ratio = (value / max_value).clamp(0.0, 1.0);
    let color = if ratio < 0.3 {
        danger_color
    } else {
        good_color
    };
    draw.draw_rectangle(x + 28, y, 50, 6, Color::new(35, 35, 45, 220));
    draw.draw_rectangle(x + 28, y, (50.0 * ratio) as i32, 6, color);
    draw.draw_rectangle_lines(x + 28, y, 50, 6, Color::DARKGRAY);
}

#[allow(
    clippy::too_many_lines,
    reason = "player rendering includes upgrade visual variants"
)]
pub(super) fn draw_player_at(
    draw: &mut RaylibMode2D<'_, RaylibDrawHandle<'_>>,
    player: &PlayerPresentation,
) {
    let active_drill = player.active_drill;
    let (offset_x, offset_y) = drill_visual_offset(active_drill);
    let screen_x = player.x + offset_x;
    let screen_y = player.y + offset_y;

    let hull_color = if player.hull < player.max_hull * 0.3 {
        Color::new(210, 95, 60, 255)
    } else {
        Color::new(235, 190, 45, 255)
    };
    draw.draw_rectangle(
        (screen_x - 14.0) as i32,
        (screen_y - 10.0) as i32,
        28,
        22,
        hull_color,
    );
    draw.draw_rectangle_lines(
        (screen_x - 14.0) as i32,
        (screen_y - 10.0) as i32,
        28,
        22,
        Color::BROWN,
    );
    draw.draw_rectangle(
        (screen_x - 7.0) as i32,
        (screen_y - 17.0) as i32,
        14,
        8,
        Color::SKYBLUE,
    );
    draw.draw_circle(
        (screen_x - 10.0) as i32,
        (screen_y + 13.0) as i32,
        4.0,
        Color::DARKGRAY,
    );
    draw.draw_circle(
        (screen_x + 10.0) as i32,
        (screen_y + 13.0) as i32,
        4.0,
        Color::DARKGRAY,
    );

    let direction = active_drill.map_or(DrillDirection::Down, |drill| drill.direction);
    match direction {
        DrillDirection::Down => draw.draw_triangle(
            Vector2::new(screen_x - 6.0, screen_y + 13.0),
            Vector2::new(screen_x + 6.0, screen_y + 13.0),
            Vector2::new(screen_x, screen_y + 28.0),
            Color::ORANGE,
        ),
        DrillDirection::Left => draw.draw_triangle(
            Vector2::new(screen_x - 15.0, screen_y - 4.0),
            Vector2::new(screen_x - 15.0, screen_y + 8.0),
            Vector2::new(screen_x - 29.0, screen_y + 2.0),
            Color::ORANGE,
        ),
        DrillDirection::Right => draw.draw_triangle(
            Vector2::new(screen_x + 15.0, screen_y - 4.0),
            Vector2::new(screen_x + 15.0, screen_y + 8.0),
            Vector2::new(screen_x + 29.0, screen_y + 2.0),
            Color::ORANGE,
        ),
    }
    if player.velocity_y < -40.0 {
        draw.draw_triangle(
            Vector2::new(screen_x - 8.0, screen_y + 16.0),
            Vector2::new(screen_x + 8.0, screen_y + 16.0),
            Vector2::new(screen_x, screen_y + 34.0),
            Color::new(255, 95, 20, 220),
        );
    }
    if player.hull < player.max_hull * 0.35 {
        let smoke_alpha = 90 + ((player.hull as i32).rem_euclid(40) as u8);
        draw.draw_circle(
            (screen_x - 16.0) as i32,
            (screen_y - 18.0) as i32,
            5.0,
            Color::new(60, 60, 60, smoke_alpha),
        );
        draw.draw_circle(
            (screen_x - 22.0) as i32,
            (screen_y - 25.0) as i32,
            7.0,
            Color::new(45, 45, 45, smoke_alpha.saturating_sub(25)),
        );
    }
    if player.scanner_level > 0 {
        draw.draw_circle_lines(
            screen_x as i32,
            (screen_y - 24.0) as i32,
            8.0 + f32::from(player.scanner_level) * 2.0,
            Color::SKYBLUE,
        );
        draw.draw_line(
            screen_x as i32,
            (screen_y - 16.0) as i32,
            screen_x as i32,
            (screen_y - 30.0) as i32,
            Color::SKYBLUE,
        );
    }
    if player.radiator_level > 1 {
        for fin in 0..player.radiator_level {
            draw.draw_rectangle(
                (screen_x + 16.0) as i32,
                (screen_y - 14.0 + f32::from(fin) * 5.0) as i32,
                10,
                2,
                Color::ORANGE,
            );
        }
    }
    if player.hull_level > 2 {
        draw.draw_rectangle_lines(
            (screen_x - 19.0) as i32,
            (screen_y - 18.0) as i32,
            38,
            36,
            Color::GOLD,
        );
    }
}
