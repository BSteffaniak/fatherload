//! World props and effects drawn over the terrain: lava glow, infrastructure, the crack overlay
//! and debris of the tile being drilled, bombs, boulders, gas clouds, sparks, explosions, scanner
//! markers and the lost-cargo beacon. Particle motion that has no simulation state (dust, lava
//! glints) is derived from the tick clock so it is deterministic.

use raylib::prelude::*;

use drillgame_simulation::{
    DiscoveredTiles, DrillDirection, DrillState, FallingBoulder, HazardCloud, InfrastructureKind,
    PlacedBomb, PlacedInfrastructure,
    poi::{PoiKnowledge, PoiTile, in_scanner_range, scanner_poi_knowledge, scanner_poi_range},
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::sprites::{
        poi as poi_art,
        terrain::{CRACKS, CRACKS_LIT},
        world as props,
    },
    terrain::tile_art::{TILE_ART, hash, material_color},
};
use crate::{
    effects::{EXPLOSION_SECONDS, Explosion, SparkParticle},
    render_snapshot::{PlayerPresentation, RelayCountdown},
};

use super::{ART, Pen, draw_world_sprite, snap};

/// A filled pixel disc of radius `radius` art pixels centred on world `(x, y)`.
fn disc(draw: &mut impl RaylibDraw, x: f32, y: f32, radius: i32, color: Color) {
    let (cx, cy) = (snap(x), snap(y));
    for row in -radius..=radius {
        let half = (((radius * radius - row * row) as f32).sqrt() + 0.3).floor() as i32;
        draw.draw_rectangle_rec(
            Rectangle::new(
                (-half as f32).mul_add(ART, cx),
                (row as f32).mul_add(ART, cy),
                (half * 2 + 1) as f32 * ART,
                ART,
            ),
            color,
        );
    }
}

fn tile_origin(position: TilePosition) -> (f32, f32) {
    (position.x as f32 * TILE_SIZE, position.y as f32 * TILE_SIZE)
}

/// The tiles overlapping `visible`.
fn visible_tiles(visible: Rectangle) -> impl Iterator<Item = TilePosition> {
    let min_x = (visible.x / TILE_SIZE).floor() as i32 - 1;
    let min_y = (visible.y / TILE_SIZE).floor() as i32 - 1;
    let max_x = ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1;
    let max_y = ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1;
    (min_y..max_y).flat_map(move |y| (min_x..max_x).map(move |x| TilePosition { x, y }))
}

/// Molten tiles throw a warm pulsing halo and shimmer with drifting bright glints.
pub fn draw_lava_glow(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    seconds: f32,
) {
    if visible.y + visible.height < 48.0 * TILE_SIZE {
        return;
    }
    for position in visible_tiles(visible) {
        let Some(tile) = terrain.tile(position) else {
            continue;
        };
        if !matches!(tile.kind, TileKind::Lava | TileKind::MagmaVent)
            || !explored.is_some_and(|tiles| tiles.contains(position))
        {
            continue;
        }
        let (x, y) = tile_origin(position);
        let (cx, cy) = (x + TILE_SIZE * 0.5, y + TILE_SIZE * 0.5);
        let phase = (hash(position.x, position.y, 5) % 628) as f32 / 100.0;
        let pulse = (seconds * 2.2 + phase).sin().mul_add(0.5, 0.5);
        match tile.kind {
            TileKind::Lava => {
                disc(
                    draw,
                    cx,
                    cy,
                    13,
                    with_alpha(ORANGE, (14.0 + pulse * 14.0) as u8),
                );
                // Glints sliding across the surface.
                for glint in 0..3 {
                    let h = hash(position.x, position.y * 7 + glint, 9);
                    let speed = 2.0 + (h % 5) as f32;
                    let travel = (seconds * speed + (h % 16) as f32) % TILE_ART as f32;
                    let gx = x + snap(travel * ART);
                    let gy = y + ((h >> 8) % (TILE_ART as u32 - 2)) as f32 * ART + ART;
                    let color = if glint == 0 { CREAM } else { GOLD };
                    draw.draw_rectangle_rec(Rectangle::new(gx, gy, ART * 2.0, ART), color);
                }
            }
            TileKind::MagmaVent => {
                disc(
                    draw,
                    cx,
                    cy,
                    10,
                    with_alpha(RED, (18.0 + pulse * 22.0) as u8),
                );
                let ember_age = (seconds * 1.5 + phase) % 1.0;
                let ex = snap(cx + (phase - 3.0) * 2.0);
                let ey = snap(y - ember_age * 20.0);
                if terrain
                    .tile(TilePosition {
                        x: position.x,
                        y: position.y - 1,
                    })
                    .is_some_and(|above| above.kind == TileKind::Air)
                {
                    draw.draw_rectangle_rec(
                        Rectangle::new(ex, ey, ART, ART),
                        with_alpha(GOLD, (255.0 * (1.0 - ember_age)) as u8),
                    );
                }
            }
            _ => {}
        }
    }
}

/// Placed infrastructure: relays, drones, lifts, supports, pumps and processors.
pub fn draw_infrastructure(
    draw: &mut impl RaylibDraw,
    infrastructure: &[PlacedInfrastructure],
    seconds: f32,
) {
    for item in infrastructure {
        let (x, y) = tile_origin(item.position);
        let sprite = match item.kind {
            InfrastructureKind::SignalRelay => &props::SIGNAL_RELAY,
            InfrastructureKind::SurveyDrone => &props::SURVEY_DRONE,
            InfrastructureKind::CargoLift => &props::CARGO_LIFT,
            InfrastructureKind::TunnelSupport => &props::TUNNEL_SUPPORT,
            InfrastructureKind::PumpStation => &props::PUMP_STATION,
            InfrastructureKind::OreProcessor => &props::ORE_PROCESSOR,
        };
        let frame = sprite.frame_at(seconds);
        let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
        let left = x + (TILE_SIZE - width) * 0.5;
        let top = if item.kind == InfrastructureKind::SurveyDrone {
            // Hovers and bobs in the middle of its tile.
            let bob = if (seconds * 2.0) as i32 % 2 == 0 {
                0.0
            } else {
                ART
            };
            y + (TILE_SIZE - height) * 0.5 + bob
        } else {
            y + TILE_SIZE - height
        };
        draw_world_sprite(draw, sprite, frame, left, top, false);
        if item.kind == InfrastructureKind::SurveyDrone {
            // Scanning beam.
            let sweep = ((seconds * 3.0).sin() * 3.0).round();
            let pen = Pen::at(left, top);
            for row in 0..4 {
                pen.px(
                    draw,
                    6 + sweep as i32 + row / 2,
                    8 + row,
                    with_alpha(MINT, 140 - row as u8 * 30),
                );
            }
        }
    }
}

/// Cracks spreading over the tile being drilled, by overall progress (chips so far plus the
/// current chip).
pub fn draw_drill_damage(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    drill: DrillState,
    ticks: u64,
) {
    let current = terrain
        .tile(drill.target)
        .map_or(drill.initial_durability, |tile| tile.durability);
    let chipped = drill.initial_durability.saturating_sub(current);
    let progress = ((f32::from(chipped) + drill.progress.clamp(0.0, 1.0))
        / f32::from(drill.initial_durability.max(1)))
    .clamp(0.0, 1.0);
    let frames = CRACKS.frames.len();
    let frame = ((progress * frames as f32) as usize).min(frames - 1);
    let (x, y) = tile_origin(drill.target);
    draw_world_sprite(draw, &CRACKS_LIT, frame, x + ART, y + ART, false);
    draw_world_sprite(draw, &CRACKS, frame, x, y, false);
    // Heat glow at the bit when drilling into gas: a warning pulse.
    if terrain
        .tile(drill.target)
        .is_some_and(|tile| tile.kind == TileKind::Gas)
    {
        let pulse = (ticks / 8).is_multiple_of(2);
        disc(
            draw,
            x + TILE_SIZE * 0.5,
            y + TILE_SIZE * 0.5,
            6,
            with_alpha(LIME, if pulse { 90 } else { 40 }),
        );
    }
}

/// Chips of the drilled material and sparks flying from the bit.
pub fn draw_drill_debris(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    drill: DrillState,
    ticks: u64,
) {
    let Some(tile) = terrain.tile(drill.target) else {
        return;
    };
    let (x, y) = tile_origin(drill.target);
    let (contact_x, contact_y, out_x, out_y) = match drill.direction {
        DrillDirection::Down => (x + TILE_SIZE * 0.5, y + ART, 0.0, -1.0),
        DrillDirection::Left => (x + TILE_SIZE - ART, y + TILE_SIZE * 0.6, 1.0, -0.4),
        DrillDirection::Right => (x + ART, y + TILE_SIZE * 0.6, -1.0, -0.4),
    };
    let dust = material_color(tile.kind, terrain.band_at(drill.target));
    let hard = matches!(
        tile.kind,
        TileKind::Stone | TileKind::HardRock | TileKind::Ore(_) | TileKind::Artifact(_)
    );
    for grain in 0..9_u64 {
        let life = 14;
        let age = (ticks + grain * 3) % life;
        let h = hash(grain as i32, ((ticks + grain * 3) / life) as i32, 3);
        let spread = (h % 9) as f32 - 4.0;
        let t = age as f32;
        let (dx, dy) = if out_x == 0.0 {
            (spread * t * 0.45, out_y * t * 1.6 + t * t * 0.09)
        } else {
            (out_x * t * 1.4, spread * t * 0.3 + out_y * t + t * t * 0.1)
        };
        let px = snap(contact_x + dx * ART * 0.5);
        let py = snap(contact_y + dy * ART * 0.5);
        let spark = hard && grain % 3 == 0;
        let color = if spark {
            if age < 5 { CREAM } else { GOLD }
        } else {
            dust
        };
        let alpha = (255 - age * 15) as u8;
        let size = if spark || age > 8 { ART } else { ART * 2.0 };
        draw.draw_rectangle_rec(Rectangle::new(px, py, size, size), with_alpha(color, alpha));
    }
}

/// The crate a stranded rig left behind, with a bobbing arrow and a pulsing beacon ring.
pub fn draw_lost_cargo(draw: &mut impl RaylibDraw, x: f32, y: f32, seconds: f32) {
    let crate_sprite = &props::CARGO_CRATE;
    let (width, height) = (
        crate_sprite.width() as f32 * ART,
        crate_sprite.height() as f32 * ART,
    );
    let ring = (seconds * 1.2).fract();
    let radius = 6 + (ring * 12.0) as i32;
    let alpha = (180.0 * (1.0 - ring)) as u8;
    let (cx, cy) = (snap(x), snap(y));
    for step in 0..24 {
        let angle = step as f32 / 24.0 * std::f32::consts::TAU;
        let px = snap(cx + angle.cos() * radius as f32 * ART);
        let py = snap(cy + angle.sin() * radius as f32 * ART);
        draw.draw_rectangle_rec(Rectangle::new(px, py, ART, ART), with_alpha(GOLD, alpha));
    }
    draw_world_sprite(
        draw,
        crate_sprite,
        0,
        x - width * 0.5,
        y - height * 0.5,
        false,
    );
    let arrow = &props::MARKER_ARROW;
    let bob = if (seconds * 3.0) as i32 % 2 == 0 {
        0.0
    } else {
        ART
    };
    draw_world_sprite(
        draw,
        arrow,
        0,
        x - arrow.width() as f32 * ART * 0.5,
        y - height * 0.5 - arrow.height() as f32 * ART - ART * 2.0 + bob,
        false,
    );
}

/// A placed bomb: the fuse fizzes faster and a warning light flashes as it runs down.
pub fn draw_bomb(draw: &mut impl RaylibDraw, bomb: &PlacedBomb, seconds: f32) {
    let sprite = &props::BOMB;
    let rate = if bomb.timer_seconds < 0.8 { 16.0 } else { 7.0 };
    let frame = (seconds * rate) as usize % sprite.frames.len();
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let left = bomb.x - width * 0.5;
    let top = bomb.y + 8.0 - height;
    draw_world_sprite(draw, sprite, frame, left, top, false);
    let flash =
        ((bomb.timer_seconds * if bomb.timer_seconds < 0.8 { 12.0 } else { 4.0 }) as i32) % 2 == 0;
    if flash {
        disc(
            draw,
            left + width * 0.5,
            top + 7.0 * ART,
            6,
            with_alpha(RED, 70),
        );
        Pen::at(left, top).rect(draw, 2, 6, 2, 2, ROSE);
    }
}

/// A boulder: it trembles with a warning mark before it drops, then falls with dust.
pub fn draw_boulder(draw: &mut impl RaylibDraw, boulder: &FallingBoulder, seconds: f32) {
    let sprite = &props::BOULDER;
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let warning = boulder.warning_seconds > 0.0;
    let shake = if warning && (boulder.warning_seconds * 30.0) as i32 % 2 == 0 {
        ART
    } else {
        0.0
    };
    let left = boulder.x - width * 0.5 + shake;
    let top = boulder.y - height * 0.5;
    draw_world_sprite(draw, sprite, 0, left, top, false);
    let pen = Pen::at(left, top);
    if warning {
        // Blinking "!" above, and grit trickling down.
        if (seconds * 6.0) as i32 % 2 == 0 {
            pen.rect(draw, 5, -9, 3, 6, INK);
            pen.rect(draw, 6, -8, 1, 4, GOLD);
            pen.rect(draw, 5, -2, 3, 2, INK);
            pen.px(draw, 6, -2, GOLD);
        }
        for grit in 0..3 {
            let fall = ((seconds * 20.0) as i32 + grit * 5) % 12;
            pen.px(draw, 3 + grit * 3, 11 + fall, with_alpha(STONE_LIGHT, 200));
        }
    } else {
        for puff in 0..3 {
            pen.rect(
                draw,
                1 + puff * 4,
                -2 - puff % 2 * 2,
                2,
                2,
                with_alpha(STONE_LIGHT, 120),
            );
        }
    }
}

/// A drifting cloud of toxic gas: dithered puffs that thin out as it dissipates.
pub fn draw_gas_cloud(draw: &mut impl RaylibDraw, cloud: &HazardCloud, seconds: f32) {
    let strength = cloud.life.clamp(0.0, 1.0);
    let radius_art = (cloud.radius / ART).max(4.0);
    for puff in 0..7 {
        let h = hash(puff, 0, 91);
        let angle =
            (h % 628) as f32 / 100.0 + seconds * 0.6 * if puff % 2 == 0 { 1.0 } else { -1.0 };
        let reach = radius_art * 0.45 * ((h >> 10) % 100) as f32 / 100.0;
        let px = cloud.x + angle.cos() * reach * ART;
        let py = cloud.y + angle.sin() * reach * ART * 0.7;
        let size = (radius_art * 0.55) as i32 + (h >> 20) as i32 % 3;
        let color = if puff % 3 == 0 { LIME } else { LEAF };
        disc(
            draw,
            px,
            py,
            size,
            with_alpha(color, (60.0 * strength) as u8),
        );
    }
    disc(
        draw,
        cloud.x,
        cloud.y,
        (radius_art * 0.4) as i32,
        with_alpha(MOSS, (70.0 * strength) as u8),
    );
    // Bubbles rising through the cloud.
    for bubble in 0..4 {
        let h = hash(bubble, 1, 93);
        let rise = (seconds * 0.8 + (h % 100) as f32 / 100.0).fract();
        let bx = cloud.x + ((h >> 8) % 20) as f32 * ART - 20.0;
        let by = cloud.y + radius_art * ART * 0.5 - rise * radius_art * ART;
        draw.draw_rectangle_rec(
            Rectangle::new(snap(bx), snap(by), ART, ART),
            with_alpha(LIME, (200.0 * strength * (1.0 - rise)) as u8),
        );
    }
}

/// Sparks: bright square embers cooling from cream to red.
pub fn draw_sparks(draw: &mut impl RaylibDraw, sparks: &[SparkParticle]) {
    for spark in sparks {
        let life = (spark.life / 0.45).clamp(0.0, 1.0);
        let color = if life > 0.7 {
            CREAM
        } else if life > 0.4 {
            GOLD
        } else if life > 0.2 {
            ORANGE
        } else {
            RED
        };
        let size = if life > 0.6 { ART * 2.0 } else { ART };
        draw.draw_rectangle_rec(
            Rectangle::new(snap(spark.x), snap(spark.y), size, size),
            color,
        );
    }
}

/// A bomb blast: a white flash, a chunky fireball and rolling smoke.
pub fn draw_explosion(draw: &mut impl RaylibDraw, explosion: &Explosion) {
    let t = (explosion.age / EXPLOSION_SECONDS).clamp(0.0, 1.0);
    let (x, y) = (explosion.x, explosion.y);
    if t < 0.12 {
        disc(draw, x, y, 30, with_alpha(CREAM, 120));
        disc(draw, x, y, 18, CREAM);
        return;
    }
    // Smoke puffs drift up and fade.
    let smoke_alpha = (200.0 * (1.0 - t)) as u8;
    for puff in 0..6 {
        let angle = puff as f32 / 6.0 * std::f32::consts::TAU + 0.4;
        let reach = 10.0 + t * 26.0;
        let px = x + angle.cos() * reach * ART * 0.8;
        let py = y + angle.sin() * reach * ART * 0.6 - t * 30.0;
        let color = if puff % 2 == 0 { STONE } else { STONE_DARK };
        disc(
            draw,
            px,
            py,
            6 + (t * 6.0) as i32,
            with_alpha(color, smoke_alpha),
        );
    }
    if t < 0.7 {
        let heat = 1.0 - t / 0.7;
        let radius = 8.0 + 20.0 * (1.0 - heat);
        disc(
            draw,
            x,
            y,
            radius as i32,
            with_alpha(RED, (220.0 * heat) as u8),
        );
        disc(
            draw,
            x,
            y,
            (radius * 0.75) as i32,
            with_alpha(ORANGE, (240.0 * heat) as u8),
        );
        disc(
            draw,
            x,
            y,
            (radius * 0.45) as i32,
            with_alpha(GOLD, (255.0 * heat) as u8),
        );
        if heat > 0.6 {
            disc(draw, x, y, (radius * 0.2) as i32, CREAM);
        }
    }
}

/// Twin-relay locks counting down: a draining timer bar over both relays of the vault (gold,
/// then a blinking red in the last third), so the pilot can see how long is left to reach the
/// other relay.
pub fn draw_relay_countdowns(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    countdowns: &[RelayCountdown],
    seconds: f32,
) {
    let blink = (seconds * 6.0) as i32 % 2 == 0;
    for countdown in countdowns {
        let urgent = countdown.remaining < 0.34;
        let fill = if urgent && blink {
            ROSE
        } else if urgent {
            RED
        } else {
            GOLD
        };
        let inner = TILE_ART;
        let filled = ((inner as f32) * countdown.remaining.clamp(0.0, 1.0)).ceil() as i32;
        for position in countdown.bounds.positions() {
            if !matches!(
                terrain.tile(position).map(|tile| tile.kind),
                Some(TileKind::Poi(PoiTile::VaultRelay { .. }))
            ) {
                continue;
            }
            let pen = Pen::at(position.x as f32 * TILE_SIZE, position.y as f32 * TILE_SIZE);
            pen.rect(draw, -1, -8, TILE_ART + 2, 6, with_alpha(INK, 230));
            pen.rect(draw, 0, -7, inner, 4, with_alpha(STONE_DARK, 240));
            pen.rect(draw, 0, -7, filled, 4, fill);
            pen.rect(draw, 0, -7, filled, 1, with_alpha(BONE, 120));
        }
    }
}

/// Scanner contacts with points of interest in range: big corner brackets around the site's
/// outline (its silhouette) in the POI's colour from the identifying tier, or a blinking
/// "strange echo" glyph near it below that.
pub fn draw_poi_scanner_marks(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    player: &PlayerPresentation,
    seconds: f32,
) {
    if !terrain.settings().generator.has_points_of_interest() {
        return;
    }
    let Some(knowledge) = scanner_poi_knowledge(player.scanner_level, false) else {
        return;
    };
    let center = TilePosition {
        x: (player.x / TILE_SIZE).floor() as i32,
        y: (player.y / TILE_SIZE).floor() as i32,
    };
    let range = scanner_poi_range(player.scanner_level, false);
    let pulse = (seconds * 2.0) as i32 % 2 == 0;
    for poi in terrain.features().pois() {
        let Some(kind) = poi.poi_kind() else {
            continue;
        };
        if !in_scanner_range(&poi.bounds, center, range) {
            continue;
        }
        if knowledge == PoiKnowledge::Echo {
            let h = hash(poi.center.x, poi.center.y, 131);
            let (x, y) = (
                poi.center.x + (h % 5).cast_signed() - 2,
                poi.center.y + ((h >> 4) % 5).cast_signed() - 2,
            );
            let pen = Pen::at(x as f32 * TILE_SIZE, y as f32 * TILE_SIZE);
            pen.sprite(
                draw,
                &poi_art::ICON_ECHO,
                poi_art::ICON_ECHO.frame_at(seconds),
                3,
                3,
            );
            continue;
        }
        let color = with_alpha(poi_art::poi_color(kind), if pulse { 230 } else { 150 });
        let pen = Pen::at(
            poi.bounds.min.x as f32 * TILE_SIZE,
            poi.bounds.min.y as f32 * TILE_SIZE,
        );
        let (width, height) = (
            poi.bounds.width() * TILE_ART,
            poi.bounds.height() * TILE_ART,
        );
        let arm = 10;
        for (x, y, dx, dy) in [
            (-3, -3, 1, 1),
            (width + 1, -3, -1, 1),
            (-3, height + 1, 1, -1),
            (width + 1, height + 1, -1, -1),
        ] {
            let hx = if dx > 0 { x } else { x - arm + 2 };
            let vy = if dy > 0 { y } else { y - arm + 2 };
            pen.rect(draw, hx - 1, y - 1, arm + 2, 4, with_alpha(INK, 160));
            pen.rect(draw, x - 1, vy - 1, 4, arm + 2, with_alpha(INK, 160));
            pen.rect(draw, hx, y, arm, 2, color);
            pen.rect(draw, x, vy, 2, arm, color);
        }
        pen.sprite(draw, poi_art::poi_icon(kind), 0, width / 2 - 4, -12);
    }
}

/// Scanner pings: corner brackets around nearby ores (gold), artifacts (pink) and hazards (red)
/// within the scanner's range, gently pulsing.
pub fn draw_scanner_marks(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    player: &PlayerPresentation,
    seconds: f32,
) {
    if player.scanner_level == 0 {
        return;
    }
    let center = TilePosition {
        x: (player.x / TILE_SIZE).floor() as i32,
        y: (player.y / TILE_SIZE).floor() as i32,
    };
    let radius = 3 + i32::from(player.scanner_level) * 2;
    let inset = if (seconds * 2.5) as i32 % 2 == 0 {
        1
    } else {
        2
    };
    for y in center.y - radius..=center.y + radius {
        for x in center.x - radius..=center.x + radius {
            if (x - center.x).abs() + (y - center.y).abs() > radius {
                continue;
            }
            let Some(tile) = terrain.tile(TilePosition { x, y }) else {
                continue;
            };
            let color = match tile.kind {
                TileKind::Ore(_) => GOLD,
                TileKind::Artifact(_) if player.scanner_level >= 3 => PINK,
                TileKind::Gas
                | TileKind::Lava
                | TileKind::MagmaVent
                | TileKind::ExplosivePocket
                | TileKind::PressurePocket
                    if player.scanner_level >= 2 =>
                {
                    RED
                }
                _ => continue,
            };
            let pen = Pen::at(x as f32 * TILE_SIZE, y as f32 * TILE_SIZE);
            let (near, far) = (inset, TILE_ART - inset - 3);
            for (cx, cy, dx, dy) in [
                (near, near, 1, 1),
                (far + 2, near, -1, 1),
                (near, far + 2, 1, -1),
                (far + 2, far + 2, -1, -1),
            ] {
                pen.rect(draw, cx.min(cx + dx * 2), cy, 3, 1, with_alpha(color, 220));
                pen.rect(draw, cx, cy.min(cy + dy * 2), 1, 3, with_alpha(color, 220));
            }
            pen.px(draw, TILE_ART / 2, TILE_ART / 2, with_alpha(color, 160));
        }
    }
}
