//! The top-down crypt view (WP-TOMBS T29): what a seat whose player is in a top-down crypt run
//! sees instead of the side-view world.
//!
//! The switch is **per view** ([`crypt_run_for_view`]): in split screen one seat can crawl a
//! crypt from above while the other plays the side view outside. The view draws, back to
//! front:
//!
//! * the floor of every revealed room in the site's tile set ([`CryptTileset`]: catacomb,
//!   sandstone, sky vault, lunar vault), with the trap overlays on it (spike holes that glint
//!   and rise, flame grates that glow and burst, raised arrow plates, boulder chutes, cracks
//!   and pits) and the creatures' telegraphs (a golem's slam ring, the crypt lord's charge
//!   line, fan and ring);
//! * walls in 3/4 view, painter-sorted row by row with the fixtures and every body standing
//!   in the row (pilots seen from above, crypt creatures, boulders and bolts), so a body walks
//!   behind the wall tops north of it and in front of the faces south of it;
//! * flames, projectiles, muzzle flashes, explosions, sparks and drops on top;
//! * torchlight: darkness everywhere but round the party, lit shrines and burning vents.
//!
//! Unrevealed rooms stay black (rooms reveal for the whole party as a member walks in).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "world drawing mixes tile indices and float pixels"
)]

use fatherload_simulation::{
    EnemyKind,
    base::BuiltTile,
    enemy::{EnemyMode, EnemyState, ShotKind},
    entity::{Entity, EntityState},
    pilot::{PilotState, TOP_DOWN_FOOTPRINT, top_down},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        SiteStyle, TombRun,
        crypt::{CryptAttack, CryptTileset, CryptTrap, CryptTrapKind, TrapStage, lord_patterns},
    },
    worldgen::noise::hash2,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::{render_snapshot::PilotPresentation, rendering::pixel_art::sprite};

use super::{ART, WorldScene, enemies, player_accent};

/// How tall a wall stands in the 3/4 view (px): its top is drawn this far up.
const WALL_HEIGHT: f32 = 16.0;
/// Size of a light cell (px).
const LIGHT_CELL: f32 = 16.0;
/// A golem's slam reach (the simulation's, for the telegraph ring).
const SLAM_REACH: f32 = fatherload_simulation::world::SLAM_REACH;

/// The colours of one tile set.
#[derive(Clone, Copy)]
struct Look {
    floor: Color,
    floor_alt: Color,
    grout: Color,
    cap: Color,
    cap_edge: Color,
    face: Color,
    face_dark: Color,
    accent: Color,
    background: Color,
}

const fn look(set: CryptTileset) -> Look {
    match set {
        CryptTileset::Catacomb => Look {
            floor: Color::new(70, 64, 72, 255),
            floor_alt: Color::new(62, 56, 64, 255),
            grout: Color::new(34, 30, 38, 255),
            cap: Color::new(132, 122, 116, 255),
            cap_edge: Color::new(164, 154, 144, 255),
            face: Color::new(104, 92, 86, 255),
            face_dark: Color::new(52, 44, 44, 255),
            accent: BONE,
            background: Color::new(10, 8, 12, 255),
        },
        CryptTileset::Sandstone => Look {
            floor: Color::new(170, 126, 82, 255),
            floor_alt: Color::new(158, 116, 74, 255),
            grout: Color::new(112, 78, 50, 255),
            cap: Color::new(222, 176, 118, 255),
            cap_edge: Color::new(240, 204, 150, 255),
            face: Color::new(180, 130, 80, 255),
            face_dark: Color::new(126, 86, 54, 255),
            accent: RUST,
            background: Color::new(18, 12, 8, 255),
        },
        CryptTileset::SkyVault => Look {
            floor: Color::new(192, 196, 210, 255),
            floor_alt: Color::new(178, 184, 200, 255),
            grout: Color::new(132, 138, 164, 255),
            cap: Color::new(236, 238, 246, 255),
            cap_edge: Color::new(252, 252, 255, 255),
            face: Color::new(196, 202, 220, 255),
            face_dark: Color::new(142, 148, 174, 255),
            accent: GOLD,
            background: Color::new(14, 16, 26, 255),
        },
        CryptTileset::LunarVault => Look {
            floor: Color::new(42, 42, 60, 255),
            floor_alt: Color::new(36, 36, 52, 255),
            grout: Color::new(20, 20, 30, 255),
            cap: Color::new(86, 88, 114, 255),
            cap_edge: Color::new(112, 116, 146, 255),
            face: Color::new(56, 58, 78, 255),
            face_dark: Color::new(30, 30, 44, 255),
            accent: MINT,
            background: Color::new(4, 4, 10, 255),
        },
    }
}

/// The top-down crypt run the view's player is in, if any.
pub(super) fn crypt_run_for_view<'a>(scene: &WorldScene<'a>) -> Option<&'a TombRun> {
    let player = scene.local?.player_id;
    scene
        .tombs
        .run_of(player)
        .filter(|run| run.site.style == SiteStyle::TopDown)
}

fn tile_of(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

/// What the crypt view knows this frame.
struct Crypt<'s, 'a> {
    scene: &'s WorldScene<'a>,
    run: &'a TombRun,
    look: Look,
    /// Milliseconds into the floor (trap cycles).
    millis: u64,
    seconds: f32,
    trap_sight: bool,
    /// Visible tiles: left, top, right, bottom (exclusive).
    tiles: (i32, i32, i32, i32),
}

impl Crypt<'_, '_> {
    fn kind(&self, tile: TilePosition) -> Option<TileKind> {
        self.scene.terrain.tile(tile).map(|found| found.kind)
    }

    fn revealed(&self, tile: TilePosition) -> bool {
        self.scene
            .explored
            .is_none_or(|explored| explored.contains(tile))
    }

    /// Masonry in the 3/4 view (walls and the lintels and gates hung in them).
    fn wall(&self, tile: TilePosition) -> bool {
        matches!(
            self.kind(tile),
            Some(TileKind::Built(
                BuiltTile::TombWall(_) | BuiltTile::TombPillar(_) | BuiltTile::GateLintel(_)
            )) | Some(TileKind::Bedrock)
        )
    }

    /// Floor seen from above: open tiles and the tiles fixtures stand on.
    fn floor(&self, tile: TilePosition) -> bool {
        matches!(
            self.kind(tile),
            Some(
                TileKind::Air
                    | TileKind::Built(
                        BuiltTile::TombShrine { .. }
                            | BuiltTile::TombCache { .. }
                            | BuiltTile::TombStairs
                            | BuiltTile::TombExit
                            | BuiltTile::Lever { .. }
                            | BuiltTile::Gate
                    )
            )
        )
    }

    fn trap_shown(&self, trap: &CryptTrap) -> bool {
        !trap.hidden || self.trap_sight
    }
}

/// Draws the crypt view (see the module docs).
pub(super) fn draw_crypt_scene(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>, run: &TombRun) {
    let hz = u64::from(scene.tick_rate_hz.max(1));
    let millis = scene.world_tick.saturating_sub(run.crypt.started.get()) * 1_000 / hz;
    let visible = scene.visible;
    let tiles = (
        (visible.x / TILE_SIZE).floor() as i32 - 1,
        (visible.y / TILE_SIZE).floor() as i32 - 1,
        ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1,
        ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 2,
    );
    let local = scene.local.map(|local| local.player_id);
    let crypt = Crypt {
        scene,
        run,
        look: look(CryptTileset::of(run.site.setting)),
        millis,
        seconds: scene.ticks as f32 / 60.0,
        trap_sight: scene
            .pilots
            .iter()
            .any(|pilot| Some(pilot.player_id) == local && pilot.state.boons.trap_sight),
        tiles,
    };
    draw.draw_rectangle_rec(visible, crypt.look.background);
    draw_floors(draw, &crypt);
    draw_traps(draw, &crypt);
    draw_tells(draw, &crypt);
    draw_rows(draw, &crypt);
    draw_flames(draw, &crypt);
    super::drops::draw_drops(draw, scene);
    super::weapons::draw_projectiles(
        draw,
        scene.projectiles,
        scene.players,
        scene.bomb_alpha,
        crypt.seconds,
    );
    super::weapons::draw_beams(draw, scene.beams, crypt.seconds);
    super::weapons::draw_shots(draw, scene.shots);
    super::fx::draw_sparks(draw, scene.sparks);
    for explosion in scene.explosions {
        if explosion.kind == fatherload_simulation::ExplosiveKind::Bomb {
            super::fx::draw_explosion(draw, explosion);
        } else {
            super::explosives::draw_blast(draw, explosion);
        }
    }
    draw_light(draw, &crypt);
    super::tombs::draw_downed(draw, scene.tombs, scene.pilots, crypt.seconds);
    if let (Some(reticle), Some(local)) = (scene.reticle, scene.local) {
        let accent = player_accent(local.player_id, Some(local.cosmetics));
        super::weapons::draw_reticle(draw, &reticle, accent, crypt.seconds);
    }
}

fn draw_floors(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    let look = crypt.look;
    let (left, top, right, bottom) = crypt.tiles;
    let seed = crypt.run.site.seed;
    for y in top..bottom {
        for x in left..right {
            let tile = TilePosition { x, y };
            if !crypt.floor(tile) || !crypt.revealed(tile) {
                continue;
            }
            let (px, py) = (x as f32 * TILE_SIZE, y as f32 * TILE_SIZE);
            let roll = hash2(seed, 0xC4F7, x, y);
            // Two flagstones per tile, offset row by row like laid stone.
            let base = if (x + y).rem_euclid(2) == 0 {
                look.floor
            } else {
                look.floor_alt
            };
            let shade = (roll % 9) as i32 - 4;
            let tint = |color: Color, by: i32| {
                Color::new(
                    (i32::from(color.r) + by).clamp(0, 255) as u8,
                    (i32::from(color.g) + by).clamp(0, 255) as u8,
                    (i32::from(color.b) + by).clamp(0, 255) as u8,
                    255,
                )
            };
            draw.draw_rectangle_v(
                Vector2::new(px, py),
                Vector2::new(TILE_SIZE, TILE_SIZE),
                tint(base, shade),
            );
            let split = if y.rem_euclid(2) == 0 { 12.0 } else { 20.0 };
            draw.draw_rectangle_v(
                Vector2::new(px, py),
                Vector2::new(TILE_SIZE, 2.0),
                look.grout,
            );
            draw.draw_rectangle_v(
                Vector2::new(px + split, py),
                Vector2::new(2.0, TILE_SIZE / 2.0),
                look.grout,
            );
            draw.draw_rectangle_v(
                Vector2::new(px, py + TILE_SIZE / 2.0),
                Vector2::new(TILE_SIZE, 1.0),
                with_alpha(look.grout, 140),
            );
            draw.draw_rectangle_v(
                Vector2::new(px + (32.0 - split), py + TILE_SIZE / 2.0),
                Vector2::new(2.0, TILE_SIZE / 2.0),
                look.grout,
            );
            if roll % 11 == 0 {
                // A worn chip.
                draw.draw_rectangle_v(
                    Vector2::new(px + 6.0 + (roll % 17) as f32, py + 8.0),
                    Vector2::new(4.0, 2.0),
                    with_alpha(look.grout, 160),
                );
            }
            if crypt.wall(TilePosition { x, y: y - 1 }) {
                // The shadow under the wall face to the north.
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(TILE_SIZE, 7.0),
                    Color::new(0, 0, 0, 90),
                );
            }
        }
    }
}

fn draw_traps(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    let (left, top, right, bottom) = crypt.tiles;
    let look = crypt.look;
    for trap in &crypt.run.crypt.traps {
        let tile = trap.at;
        if tile.x < left || tile.x >= right || tile.y < top || tile.y >= bottom {
            continue;
        }
        if !crypt.revealed(tile) {
            continue;
        }
        let (px, py) = (tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE);
        let (cx, cy) = (px + 16.0, py + 16.0);
        if !crypt.trap_shown(trap) {
            continue;
        }
        let stage = trap.stage(crypt.millis);
        let progress = trap.progress(crypt.millis);
        match trap.kind {
            CryptTrapKind::Spikes => {
                for (dx, dy) in [
                    (8.0, 8.0),
                    (24.0, 8.0),
                    (16.0, 16.0),
                    (8.0, 24.0),
                    (24.0, 24.0),
                ] {
                    let (hx, hy) = (px + dx, py + dy);
                    draw.draw_circle_v(Vector2::new(hx, hy), 3.5, Color::new(0, 0, 0, 150));
                    match stage {
                        TrapStage::Idle => {}
                        TrapStage::Warning => {
                            // Tips rattling up out of the holes.
                            let rise = progress * 3.0;
                            let jitter = ((crypt.seconds * 40.0).sin()) * 0.6;
                            draw.draw_triangle(
                                Vector2::new(hx + jitter, hy - 1.5 - rise),
                                Vector2::new(hx - 2.0 + jitter, hy + 1.5),
                                Vector2::new(hx + 2.0 + jitter, hy + 1.5),
                                STEEL,
                            );
                        }
                        TrapStage::Active => {
                            draw.draw_triangle(
                                Vector2::new(hx, hy - 9.0),
                                Vector2::new(hx - 3.5, hy + 2.0),
                                Vector2::new(hx + 3.5, hy + 2.0),
                                STEEL_LIGHT,
                            );
                            draw.draw_line_v(
                                Vector2::new(hx, hy - 9.0),
                                Vector2::new(hx - 3.5, hy + 2.0),
                                STEEL_DARK,
                            );
                        }
                    }
                }
            }
            CryptTrapKind::FlameJet => {
                draw.draw_rectangle_v(
                    Vector2::new(px + 6.0, py + 6.0),
                    Vector2::new(20.0, 20.0),
                    Color::new(20, 14, 12, 255),
                );
                for bar in 0..4 {
                    draw.draw_rectangle_v(
                        Vector2::new(px + 8.0 + bar as f32 * 5.0, py + 6.0),
                        Vector2::new(2.0, 20.0),
                        STEEL_DARK,
                    );
                }
                let glow = match stage {
                    TrapStage::Idle => 0.0,
                    TrapStage::Warning => 0.3 + 0.5 * progress,
                    TrapStage::Active => 1.0,
                };
                if glow > 0.0 {
                    let pulse = 0.8 + 0.2 * (crypt.seconds * 18.0).sin();
                    draw.draw_rectangle_v(
                        Vector2::new(px + 6.0, py + 6.0),
                        Vector2::new(20.0, 20.0),
                        with_alpha(ORANGE, (glow * pulse * 190.0) as u8),
                    );
                }
            }
            CryptTrapKind::ArrowPlate => {
                let pressed = crypt.run.crypt.plates.contains_key(&tile);
                let (inset, color) = if pressed {
                    (6.0, Color::new(0, 0, 0, 90))
                } else {
                    (4.0, with_alpha(look.cap, 255))
                };
                draw.draw_rectangle_v(
                    Vector2::new(px + inset, py + inset),
                    Vector2::new(32.0 - inset * 2.0, 32.0 - inset * 2.0),
                    color,
                );
                draw.draw_rectangle_lines_ex(
                    Rectangle::new(
                        px + inset,
                        py + inset,
                        32.0 - inset * 2.0,
                        32.0 - inset * 2.0,
                    ),
                    1.0,
                    look.face_dark,
                );
                // A glyph pointing along the line the arrows fly.
                let (dx, dy) = (f32::from(trap.dir.0), f32::from(trap.dir.1));
                draw.draw_line_ex(
                    Vector2::new(cx - dx * 7.0, cy - dy * 7.0),
                    Vector2::new(cx + dx * 7.0, cy + dy * 7.0),
                    2.0,
                    look.face_dark,
                );
                draw_arrow_slots(draw, crypt, trap);
            }
            CryptTrapKind::BoulderChute => {
                draw.draw_circle_v(Vector2::new(cx, cy), 12.0, Color::new(0, 0, 0, 200));
                draw.draw_circle_lines(cx as i32, cy as i32, 12.0, look.face_dark);
                let (dx, dy) = (f32::from(trap.dir.0), f32::from(trap.dir.1));
                let (nx, ny) = (-dy, dx);
                let tip = Vector2::new(cx + dx * 14.0, cy + dy * 14.0);
                let warn = stage == TrapStage::Warning;
                let color = if warn {
                    with_alpha(AMBER, (150.0 + 100.0 * (crypt.seconds * 20.0).sin()) as u8)
                } else {
                    with_alpha(look.accent, 120)
                };
                draw.draw_triangle(
                    tip,
                    Vector2::new(cx + dx * 6.0 - nx * 6.0, cy + dy * 6.0 - ny * 6.0),
                    Vector2::new(cx + dx * 6.0 + nx * 6.0, cy + dy * 6.0 + ny * 6.0),
                    color,
                );
                if warn {
                    // Dust shaken loose.
                    for k in 0..4 {
                        let t = (crypt.seconds * 3.0 + k as f32 * 0.25).fract();
                        draw.draw_circle_v(
                            Vector2::new(cx + (k as f32 - 1.5) * 6.0, cy - 12.0 + t * 20.0),
                            1.5,
                            with_alpha(look.cap_edge, ((1.0 - t) * 200.0) as u8),
                        );
                    }
                }
            }
            CryptTrapKind::CrackedFloor => {
                let crumbling = crypt.run.crypt.crumbling.contains_key(&tile);
                let shake = if crumbling {
                    (crypt.seconds * 50.0).sin() * 1.5
                } else {
                    0.0
                };
                let color = if crumbling {
                    Color::new(0, 0, 0, 220)
                } else {
                    Color::new(0, 0, 0, 130)
                };
                let r = hash2(crypt.run.site.seed, 0xC4AC, tile.x, tile.y);
                let a = Vector2::new(px + 4.0 + (r % 8) as f32 + shake, py + 6.0);
                let b = Vector2::new(cx + shake, cy);
                let c = Vector2::new(px + 26.0 + shake, py + 22.0 + ((r >> 4) % 6) as f32);
                let d = Vector2::new(px + 10.0 + shake, py + 28.0);
                draw.draw_line_ex(a, b, 1.5, color);
                draw.draw_line_ex(b, c, 1.5, color);
                draw.draw_line_ex(b, d, 1.0, color);
            }
            CryptTrapKind::Pit => {
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(32.0, 32.0),
                    Color::new(4, 2, 6, 255),
                );
                // The rim catches the light on its far side.
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(32.0, 5.0),
                    with_alpha(look.face_dark, 255),
                );
                draw.draw_rectangle_v(
                    Vector2::new(px + 2.0, py + 5.0),
                    Vector2::new(28.0, 6.0),
                    Color::new(20, 16, 22, 255),
                );
            }
        }
        if trap.hidden && crypt.trap_sight {
            let pulse = 0.6 + 0.4 * (crypt.seconds * 4.0).sin();
            draw.draw_rectangle_lines_ex(
                Rectangle::new(px + 2.0, py + 2.0, 28.0, 28.0),
                2.0,
                Color::new(255, 110, 90, (200.0 * pulse) as u8),
            );
        }
    }
}

/// The arrow slots in the walls at both ends of a plate's line.
fn draw_arrow_slots(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>, trap: &CryptTrap) {
    let Some(room) = crypt.run.rooms.get(usize::from(trap.room)) else {
        return;
    };
    let interior = room.interior;
    let ends = if trap.dir.0 != 0 {
        [
            TilePosition {
                x: interior.min.x - 1,
                y: trap.at.y,
            },
            TilePosition {
                x: interior.max.x + 1,
                y: trap.at.y,
            },
        ]
    } else {
        [
            TilePosition {
                x: trap.at.x,
                y: interior.min.y - 1,
            },
            TilePosition {
                x: trap.at.x,
                y: interior.max.y + 1,
            },
        ]
    };
    for end in ends {
        let (px, py) = (end.x as f32 * TILE_SIZE, end.y as f32 * TILE_SIZE);
        // On the face (south wall: its top edge; others: the face band).
        let y = py + TILE_SIZE - WALL_HEIGHT * 0.5;
        draw.draw_rectangle_v(
            Vector2::new(px + 12.0, y - 3.0),
            Vector2::new(8.0, 4.0),
            Color::new(0, 0, 0, 230),
        );
    }
}

/// Telegraphs drawn on the floor: a golem's slam ring and the crypt lord's tells.
fn draw_tells(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    let scene = crypt.scene;
    for entity in scene.enemies {
        let EntityState::Enemy(state) = &entity.state else {
            continue;
        };
        if state.mode != EnemyMode::Windup {
            continue;
        }
        let (x, y) = entity.body.lerp_position(scene.bomb_alpha);
        if !crypt.revealed(tile_of(x, y)) {
            continue;
        }
        let left = state.mode_until.get().saturating_sub(scene.world_tick) as f32;
        let total = (state.kind.stats().windup_millis as f32 * scene.tick_rate_hz as f32 / 1_000.0)
            .max(1.0);
        let charge = (1.0 - left / total).clamp(0.0, 1.0);
        match state.kind {
            EnemyKind::CryptGolem => {
                draw.draw_circle_v(
                    Vector2::new(x, y),
                    SLAM_REACH,
                    with_alpha(RED, (30.0 + 60.0 * charge) as u8),
                );
                draw.draw_ring(
                    Vector2::new(x, y),
                    SLAM_REACH - 2.0,
                    SLAM_REACH,
                    0.0,
                    360.0,
                    40,
                    with_alpha(RED, 220),
                );
                draw.draw_ring(
                    Vector2::new(x, y),
                    (SLAM_REACH * charge - 2.0).max(0.0),
                    SLAM_REACH * charge,
                    0.0,
                    360.0,
                    40,
                    with_alpha(ORANGE, 200),
                );
            }
            EnemyKind::CryptLord => {
                let patterns = lord_patterns(state.phase.max(1));
                let attack = patterns[usize::from(state.pattern) % patterns.len()];
                let target = nearest_pilot(scene.pilots, (x, y));
                match attack {
                    CryptAttack::Charge => {
                        if let Some((tx, ty)) = target {
                            let (dx, dy) = (tx - x, ty - y);
                            let length = dx.hypot(dy).max(1.0);
                            let end =
                                Vector2::new(x + dx / length * 420.0, y + dy / length * 420.0);
                            draw.draw_line_ex(
                                Vector2::new(x, y),
                                end,
                                40.0,
                                with_alpha(RED, (40.0 + 80.0 * charge) as u8),
                            );
                            draw.draw_line_ex(Vector2::new(x, y), end, 3.0, with_alpha(RED, 220));
                        }
                    }
                    CryptAttack::Fan => {
                        if let Some((tx, ty)) = target {
                            let (dx, dy) = (tx - x, ty - y);
                            let length = dx.hypot(dy).max(1.0);
                            let (ux, uy) = (dx / length, dy / length);
                            for step in -2..=2 {
                                let angle = step as f32 * 0.2618;
                                let (c, s) = (angle.cos(), angle.sin());
                                let (vx, vy) = (ux * c - uy * s, ux * s + uy * c);
                                draw.draw_line_ex(
                                    Vector2::new(x, y),
                                    Vector2::new(x + vx * 200.0 * charge, y + vy * 200.0 * charge),
                                    2.0,
                                    with_alpha(LAVENDER, 160),
                                );
                            }
                        }
                    }
                    CryptAttack::Ring | CryptAttack::Spiral | CryptAttack::Slam => {
                        let radius = 24.0 + 40.0 * charge;
                        draw.draw_ring(
                            Vector2::new(x, y),
                            radius - 3.0,
                            radius,
                            0.0,
                            360.0,
                            48,
                            with_alpha(LAVENDER, 200),
                        );
                    }
                }
            }
            EnemyKind::Cultist => {
                draw.draw_circle_v(
                    Vector2::new(x, y),
                    8.0 + 8.0 * charge,
                    with_alpha(VIOLET, 90),
                );
            }
            _ => {}
        }
    }
}

fn nearest_pilot(pilots: &[PilotPresentation], from: (f32, f32)) -> Option<(f32, f32)> {
    pilots.iter().map(PilotPresentation::middle).min_by(|a, b| {
        (a.0 - from.0)
            .hypot(a.1 - from.1)
            .total_cmp(&(b.0 - from.0).hypot(b.1 - from.1))
    })
}

/// Something standing in a row: drawn after the row's walls, in x order.
enum Body<'e> {
    Pilot(&'e PilotPresentation),
    Enemy(&'e Entity, &'e EnemyState, (f32, f32)),
    Shot(&'e Entity, ShotKind, (f32, f32)),
}

/// Walls, fixtures and bodies, painter-sorted by row.
fn draw_rows(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    let scene = crypt.scene;
    let (left, top, right, bottom) = crypt.tiles;
    let mut rows: std::collections::BTreeMap<i32, Vec<(f32, Body<'_>)>> =
        std::collections::BTreeMap::new();
    for pilot in scene.pilots {
        if pilot.state.view != fatherload_simulation::pilot::PilotView::TopDown {
            continue;
        }
        let row = tile_of(pilot.x, pilot.y - 0.5).y;
        rows.entry(row)
            .or_default()
            .push((pilot.x, Body::Pilot(pilot)));
    }
    for entity in scene.enemies {
        let (x, y) = entity.body.lerp_position(scene.bomb_alpha);
        if !crypt.revealed(tile_of(x, y)) {
            continue;
        }
        match &entity.state {
            EntityState::Enemy(state) => {
                let feet = y + state.kind.stats().radius;
                rows.entry(tile_of(x, feet - 0.5).y)
                    .or_default()
                    .push((x, Body::Enemy(entity, state, (x, y))));
            }
            EntityState::EnemyShot(shot) => {
                rows.entry(tile_of(x, y).y)
                    .or_default()
                    .push((x, Body::Shot(entity, shot.kind, (x, y))));
            }
            _ => {}
        }
    }
    for y in top..bottom {
        for x in left..right {
            let tile = TilePosition { x, y };
            if !crypt.revealed(tile) && !crypt.revealed(TilePosition { x, y: y + 1 }) {
                continue;
            }
            if crypt.wall(tile) {
                draw_wall(draw, crypt, tile);
            } else if let Some(TileKind::Built(built)) = crypt.kind(tile) {
                if crypt.revealed(tile) {
                    draw_fixture(draw, crypt, tile, built);
                }
            }
        }
        if let Some(bodies) = rows.get_mut(&y) {
            bodies.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, body) in bodies.iter() {
                match body {
                    Body::Pilot(pilot) => draw_pilot(draw, crypt, pilot),
                    Body::Enemy(entity, state, at) => draw_enemy(draw, crypt, entity, state, *at),
                    Body::Shot(entity, kind, at) => draw_shot(draw, crypt, entity, *kind, *at),
                }
            }
        }
    }
}

fn draw_wall(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>, tile: TilePosition) {
    let look = crypt.look;
    let (px, py) = (tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE);
    let south = TilePosition {
        x: tile.x,
        y: tile.y + 1,
    };
    let open_south = !crypt.wall(south) && crypt.revealed(south);
    // Bedrock beyond the floor: only the masonry next to the floor is drawn.
    if matches!(crypt.kind(tile), Some(TileKind::Bedrock)) {
        return;
    }
    let seed = crypt.run.site.seed;
    let roll = hash2(seed, 0xC4A1, tile.x, tile.y);
    if open_south {
        let face_top = py + TILE_SIZE - WALL_HEIGHT;
        draw.draw_rectangle_v(
            Vector2::new(px, face_top),
            Vector2::new(TILE_SIZE, WALL_HEIGHT),
            look.face,
        );
        // Courses of stone.
        draw.draw_rectangle_v(
            Vector2::new(px, face_top + 7.0),
            Vector2::new(TILE_SIZE, 1.0),
            look.face_dark,
        );
        let joint = if tile.x.rem_euclid(2) == 0 {
            10.0
        } else {
            22.0
        };
        draw.draw_rectangle_v(
            Vector2::new(px + joint, face_top),
            Vector2::new(1.0, 7.0),
            look.face_dark,
        );
        draw.draw_rectangle_v(
            Vector2::new(px + 32.0 - joint, face_top + 8.0),
            Vector2::new(1.0, 8.0),
            look.face_dark,
        );
        draw.draw_rectangle_v(
            Vector2::new(px, py + TILE_SIZE - 2.0),
            Vector2::new(TILE_SIZE, 2.0),
            look.face_dark,
        );
        // The set's mark on a few faces.
        if roll % 4 == 0 {
            match CryptTileset::of(crypt.run.site.setting) {
                CryptTileset::Catacomb => {
                    // A skull in a niche.
                    draw.draw_rectangle_v(
                        Vector2::new(px + 11.0, face_top + 2.0),
                        Vector2::new(10.0, 11.0),
                        look.face_dark,
                    );
                    draw.draw_circle_v(Vector2::new(px + 16.0, face_top + 7.0), 3.5, look.accent);
                    draw.draw_rectangle_v(
                        Vector2::new(px + 14.0, face_top + 6.0),
                        Vector2::new(1.0, 2.0),
                        INK,
                    );
                    draw.draw_rectangle_v(
                        Vector2::new(px + 17.0, face_top + 6.0),
                        Vector2::new(1.0, 2.0),
                        INK,
                    );
                }
                CryptTileset::Sandstone => {
                    draw.draw_rectangle_v(
                        Vector2::new(px + 6.0, face_top + 3.0),
                        Vector2::new(20.0, 1.0),
                        look.accent,
                    );
                    draw.draw_circle_v(Vector2::new(px + 16.0, face_top + 10.0), 2.5, look.accent);
                }
                CryptTileset::SkyVault => {
                    draw.draw_rectangle_v(
                        Vector2::new(px, face_top + 2.0),
                        Vector2::new(TILE_SIZE, 2.0),
                        look.accent,
                    );
                }
                CryptTileset::LunarVault => {
                    let glow = 0.6 + 0.4 * (crypt.seconds * 2.0 + roll as f32).sin();
                    draw.draw_rectangle_v(
                        Vector2::new(px + 4.0, face_top + 10.0),
                        Vector2::new(24.0, 1.0),
                        with_alpha(look.accent, (glow * 220.0) as u8),
                    );
                }
            }
        }
    }
    // The top, raised by the wall's height.
    let cap_top = py - WALL_HEIGHT;
    draw.draw_rectangle_v(
        Vector2::new(px, cap_top),
        Vector2::new(TILE_SIZE, TILE_SIZE),
        look.cap,
    );
    let north = TilePosition {
        x: tile.x,
        y: tile.y - 1,
    };
    if !crypt.wall(north) {
        draw.draw_rectangle_v(
            Vector2::new(px, cap_top),
            Vector2::new(TILE_SIZE, 2.0),
            look.cap_edge,
        );
    }
    if !crypt.wall(TilePosition {
        x: tile.x - 1,
        y: tile.y,
    }) {
        draw.draw_rectangle_v(
            Vector2::new(px, cap_top),
            Vector2::new(2.0, TILE_SIZE),
            look.cap_edge,
        );
    }
    if !crypt.wall(TilePosition {
        x: tile.x + 1,
        y: tile.y,
    }) {
        draw.draw_rectangle_v(
            Vector2::new(px + 30.0, cap_top),
            Vector2::new(2.0, TILE_SIZE),
            look.face_dark,
        );
    }
    if open_south {
        draw.draw_rectangle_v(
            Vector2::new(px, cap_top + TILE_SIZE - 2.0),
            Vector2::new(TILE_SIZE, 2.0),
            look.face_dark,
        );
    }
    if roll % 7 == 0 {
        draw.draw_rectangle_v(
            Vector2::new(px + 8.0 + (roll % 13) as f32, cap_top + 10.0),
            Vector2::new(5.0, 1.0),
            look.face_dark,
        );
    }
    if matches!(
        crypt.kind(tile),
        Some(TileKind::Built(BuiltTile::GateLintel(_)))
    ) {
        // The lintel's keystone over a gate.
        draw.draw_rectangle_v(
            Vector2::new(px + 10.0, cap_top + 8.0),
            Vector2::new(12.0, 14.0),
            look.accent,
        );
    }
}

fn draw_fixture(
    draw: &mut impl RaylibDraw,
    crypt: &Crypt<'_, '_>,
    tile: TilePosition,
    built: BuiltTile,
) {
    let look = crypt.look;
    let (px, py) = (tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE);
    let (cx, cy) = (px + 16.0, py + 16.0);
    let seconds = crypt.seconds;
    match built {
        BuiltTile::TombShrine { lit } => {
            draw.draw_ellipse(
                cx as i32,
                (cy + 8.0) as i32,
                14.0,
                6.0,
                Color::new(0, 0, 0, 90),
            );
            draw.draw_rectangle_v(
                Vector2::new(cx - 10.0, cy - 6.0),
                Vector2::new(20.0, 16.0),
                look.face,
            );
            draw.draw_rectangle_v(
                Vector2::new(cx - 12.0, cy - 12.0),
                Vector2::new(24.0, 8.0),
                look.cap,
            );
            draw.draw_circle_v(Vector2::new(cx, cy - 8.0), 6.0, look.face_dark);
            if lit {
                let flicker = (seconds * 11.0).sin() * 1.5;
                draw.draw_circle_v(
                    Vector2::new(cx, cy - 12.0),
                    24.0,
                    Color::new(255, 190, 90, 40),
                );
                draw.draw_circle_v(
                    Vector2::new(cx, cy - 12.0 + flicker * 0.3),
                    6.0 + flicker,
                    ORANGE,
                );
                draw.draw_circle_v(Vector2::new(cx, cy - 13.0), 3.5, GOLD);
                draw.draw_circle_v(Vector2::new(cx, cy - 14.0), 1.5, CREAM);
            } else {
                draw.draw_circle_v(Vector2::new(cx, cy - 9.0), 3.0, Color::new(90, 70, 50, 255));
            }
        }
        BuiltTile::TombCache { open } => {
            draw.draw_ellipse(
                cx as i32,
                (cy + 9.0) as i32,
                13.0,
                5.0,
                Color::new(0, 0, 0, 90),
            );
            draw.draw_rectangle_v(
                Vector2::new(cx - 11.0, cy - 4.0),
                Vector2::new(22.0, 13.0),
                Color::new(110, 64, 34, 255),
            );
            draw.draw_rectangle_v(
                Vector2::new(cx - 11.0, cy + 2.0),
                Vector2::new(22.0, 2.0),
                GOLD,
            );
            if open {
                draw.draw_rectangle_v(
                    Vector2::new(cx - 11.0, cy - 12.0),
                    Vector2::new(22.0, 8.0),
                    Color::new(86, 50, 28, 255),
                );
                draw.draw_rectangle_v(
                    Vector2::new(cx - 9.0, cy - 4.0),
                    Vector2::new(18.0, 4.0),
                    Color::new(30, 20, 14, 255),
                );
            } else {
                draw.draw_rectangle_v(
                    Vector2::new(cx - 12.0, cy - 10.0),
                    Vector2::new(24.0, 8.0),
                    Color::new(140, 84, 44, 255),
                );
                draw.draw_rectangle_v(
                    Vector2::new(cx - 12.0, cy - 7.0),
                    Vector2::new(24.0, 2.0),
                    GOLD,
                );
                draw.draw_rectangle_v(
                    Vector2::new(cx - 2.0, cy - 6.0),
                    Vector2::new(4.0, 5.0),
                    AMBER,
                );
                let glint = ((seconds * 2.0 + tile.x as f32).sin() + 1.0) * 0.5;
                draw.draw_circle_v(
                    Vector2::new(cx + 7.0, cy - 9.0),
                    1.5,
                    with_alpha(CREAM, (glint * 255.0) as u8),
                );
            }
        }
        BuiltTile::TombStairs => {
            for step in 0..5 {
                let shade = 120 - step * 22;
                draw.draw_rectangle_v(
                    Vector2::new(px + 2.0, py + 2.0 + step as f32 * 6.0),
                    Vector2::new(28.0, 6.0),
                    Color::new(
                        shade as u8,
                        (shade * 9 / 10) as u8,
                        (shade * 8 / 10) as u8,
                        255,
                    ),
                );
            }
            draw.draw_rectangle_lines_ex(
                Rectangle::new(px + 1.0, py + 1.0, 30.0, 30.0),
                2.0,
                look.cap_edge,
            );
        }
        BuiltTile::TombExit => {
            draw.draw_rectangle_v(
                Vector2::new(px + 3.0, py + 3.0),
                Vector2::new(26.0, 26.0),
                Color::new(220, 210, 170, 255),
            );
            draw.draw_rectangle_v(
                Vector2::new(px + 7.0, py + 7.0),
                Vector2::new(18.0, 18.0),
                Color::new(255, 244, 200, 255),
            );
            draw.draw_rectangle_lines_ex(
                Rectangle::new(px + 2.0, py + 2.0, 28.0, 28.0),
                2.0,
                look.face_dark,
            );
        }
        BuiltTile::Lever { on, .. } => {
            draw.draw_rectangle_v(
                Vector2::new(cx - 8.0, cy - 2.0),
                Vector2::new(16.0, 10.0),
                look.face_dark,
            );
            draw.draw_rectangle_v(
                Vector2::new(cx - 9.0, cy - 6.0),
                Vector2::new(18.0, 6.0),
                STEEL_DARK,
            );
            let tip = if on {
                Vector2::new(cx + 9.0, cy - 14.0)
            } else {
                Vector2::new(cx - 9.0, cy - 14.0)
            };
            draw.draw_line_ex(Vector2::new(cx, cy - 4.0), tip, 3.0, STEEL_LIGHT);
            draw.draw_circle_v(tip, 3.0, if on { MINT } else { RED });
        }
        BuiltTile::Gate => {
            // Bars across the doorway, standing as tall as the wall.
            let top = py - WALL_HEIGHT;
            draw.draw_rectangle_v(
                Vector2::new(px, top),
                Vector2::new(TILE_SIZE, 3.0),
                STEEL_DARK,
            );
            for bar in 0..4 {
                draw.draw_rectangle_v(
                    Vector2::new(px + 3.0 + bar as f32 * 8.0, top),
                    Vector2::new(3.0, TILE_SIZE + WALL_HEIGHT - 2.0),
                    STEEL,
                );
            }
            draw.draw_rectangle_v(
                Vector2::new(px, py + 12.0),
                Vector2::new(TILE_SIZE, 3.0),
                STEEL_DARK,
            );
        }
        _ => {}
    }
}

/// A pilot seen from above: a shadow, the pack, the suit, the helmet with its visor toward
/// where it walks, the arm and weapon toward its aim; a tumbling ball mid-roll; lying still
/// when downed.
fn draw_pilot(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>, pilot: &PilotPresentation) {
    let state: &PilotState = &pilot.state;
    let suit = player_accent(pilot.player_id, Some(pilot.cosmetics));
    let (x, feet) = (pilot.x, pilot.y);
    let middle = feet - TOP_DOWN_FOOTPRINT * 0.5;
    draw.draw_ellipse(
        x as i32,
        (feet - 3.0) as i32,
        12.0,
        5.0,
        Color::new(0, 0, 0, 100),
    );
    let downed = crypt
        .run
        .member(pilot.player_id)
        .is_some_and(fatherload_simulation::tomb::RunMember::is_downed);
    if downed {
        draw.draw_ellipse(x as i32, (middle + 2.0) as i32, 14.0, 7.0, darken(suit));
        draw.draw_circle_v(Vector2::new(x + 11.0, middle), 6.0, STEEL_LIGHT);
        draw.draw_circle_v(
            Vector2::new(x + 12.0, middle),
            3.0,
            Color::new(40, 60, 80, 255),
        );
        return;
    }
    if state.roll > 0.0 {
        let (ux, uy) = top_down::unit_of(state.roll_x, state.roll_y);
        for trail in 1..=3 {
            let back = trail as f32 * 7.0;
            draw.draw_circle_v(
                Vector2::new(x - ux * back, middle - 4.0 - uy * back),
                10.0 - trail as f32,
                Color::new(255, 245, 220, (150 - trail * 40) as u8),
            );
        }
        draw.draw_circle_v(Vector2::new(x, middle - 4.0), 10.0, suit);
        let spin = state.anim * 25.0;
        for k in 0..2 {
            let angle = spin + k as f32 * std::f32::consts::PI;
            draw.draw_line_ex(
                Vector2::new(x + angle.cos() * 9.0, middle - 4.0 + angle.sin() * 9.0),
                Vector2::new(x - angle.cos() * 9.0, middle - 4.0 - angle.sin() * 9.0),
                2.0,
                darken(suit),
            );
        }
        return;
    }
    let (hx, hy) = top_down::heading_of(state);
    let bob = if state.run_time > 0.0 {
        (state.anim * 14.0).sin() * 1.2
    } else {
        0.0
    };
    // Boots stepping.
    let step = if state.run_time > 0.0 {
        (state.anim * 14.0).sin() * 3.0
    } else {
        0.0
    };
    draw.draw_circle_v(Vector2::new(x - 5.0, feet - 4.0 + step), 3.0, INK);
    draw.draw_circle_v(Vector2::new(x + 5.0, feet - 4.0 - step), 3.0, INK);
    // The pack, behind.
    draw.draw_rectangle_v(
        Vector2::new(x - hx * 6.0 - 6.0, middle - 10.0 - hy * 5.0 + bob),
        Vector2::new(12.0, 10.0),
        STEEL_DARK,
    );
    // The suit.
    draw.draw_circle_v(Vector2::new(x, middle - 4.0 + bob), 9.5, darken(suit));
    draw.draw_circle_v(Vector2::new(x, middle - 5.0 + bob), 8.0, suit);
    // The arm and what it holds, toward the aim.
    let (ax, ay) = {
        let (ax, ay) = (f32::from(state.aim_x), f32::from(state.aim_y));
        let length = ax.hypot(ay);
        if length < 1.0 {
            (hx, hy)
        } else {
            (ax / length, ay / length)
        }
    };
    let shoulder = Vector2::new(x + ax * 4.0, middle - 6.0 + bob + ay * 3.0);
    let reach = if state.swing > 0.0 { 18.0 } else { 14.0 };
    let hand = Vector2::new(shoulder.x + ax * reach, shoulder.y + ay * reach);
    let held = if state.weapon.is_some() {
        STEEL
    } else {
        Color::new(140, 110, 80, 255)
    };
    draw.draw_line_ex(shoulder, hand, 4.0, held);
    draw.draw_circle_v(hand, 2.0, STEEL_LIGHT);
    // The helmet and visor.
    let head = Vector2::new(x + hx * 1.5, middle - 14.0 + bob + hy * 1.5);
    draw.draw_circle_v(head, 6.5, STEEL_LIGHT);
    draw.draw_circle_v(
        Vector2::new(head.x + hx * 3.0, head.y + hy * 2.0 + 0.5),
        3.5,
        Color::new(60, 110, 150, 255),
    );
    draw.draw_circle_v(
        Vector2::new(head.x + hx * 3.5 - 1.0, head.y + hy * 2.0 - 0.5),
        1.0,
        ICE,
    );
    if state.hurt > 0.0 && ((state.hurt * 20.0) as i32) % 2 == 0 {
        draw.draw_circle_v(
            Vector2::new(x, middle - 8.0),
            12.0,
            Color::new(255, 255, 255, 120),
        );
    }
}

fn darken(color: Color) -> Color {
    Color::new(
        color.r / 2 + 10,
        color.g / 2 + 10,
        color.b / 2 + 10,
        color.a,
    )
}

fn draw_enemy(
    draw: &mut impl RaylibDraw,
    crypt: &Crypt<'_, '_>,
    entity: &Entity,
    state: &EnemyState,
    (x, y): (f32, f32),
) {
    let scene = crypt.scene;
    let sprite = enemies::enemy_sprite(state.kind);
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let radius = state.kind.stats().radius;
    let in_wall = state.kind.passes_walls() && crypt.wall(tile_of(x, y));
    if !in_wall {
        draw.draw_ellipse(
            x as i32,
            (y + radius * 0.7) as i32,
            radius + 3.0,
            radius * 0.45 + 2.0,
            Color::new(0, 0, 0, 90),
        );
    }
    if state.kind == EnemyKind::CryptLord && state.phase >= 3 {
        let pulse = 0.5 + 0.5 * (crypt.seconds * 6.0).sin();
        draw.draw_circle_v(
            Vector2::new(x, y),
            30.0 + pulse * 4.0,
            with_alpha(ROSE, (40.0 + pulse * 30.0) as u8),
        );
    }
    let hurt = state.hurt_until.get() > scene.world_tick;
    let windup = state.mode == EnemyMode::Windup && ((crypt.seconds * 14.0) as i32) % 2 == 0;
    let alpha: u8 = match (state.kind.passes_walls(), in_wall) {
        (true, true) => 70,
        (true, false) => 200,
        _ => 255,
    };
    let tint = if hurt {
        Color::new(255, 120, 120, alpha)
    } else if windup {
        Color::new(255, 230, 200, alpha)
    } else {
        Color::new(255, 255, 255, alpha)
    };
    let phase = crypt.seconds + (state.seed % 97) as f32 * 0.013;
    let moving = entity.body.velocity_x.abs() + entity.body.velocity_y.abs() > 6.0;
    let frame = if moving || state.kind.passes_walls() {
        sprite.frame_at(phase)
    } else {
        0
    };
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        frame,
        Vector2::new(super::snap(x - width * 0.5), super::snap(y - height * 0.6)),
        ART,
        state.facing < 0,
        tint,
    );
    if windup && state.kind == EnemyKind::CryptSkeleton {
        draw.draw_rectangle_v(
            Vector2::new(x - 1.5, y - height * 0.6 - 10.0),
            Vector2::new(3.0, 6.0),
            RED,
        );
        draw.draw_rectangle_v(
            Vector2::new(x - 1.5, y - height * 0.6 - 3.0),
            Vector2::new(3.0, 2.0),
            RED,
        );
    }
    if state.kind != EnemyKind::CryptLord && state.health < state.max_health {
        let share = state.health_fraction();
        let top = y - height * 0.6 - 4.0;
        draw.draw_rectangle_v(Vector2::new(x - 10.0, top), Vector2::new(20.0, 3.0), INK);
        draw.draw_rectangle_v(
            Vector2::new(x - 10.0, top),
            Vector2::new(20.0 * share, 3.0),
            RED,
        );
    }
}

fn draw_shot(
    draw: &mut impl RaylibDraw,
    crypt: &Crypt<'_, '_>,
    entity: &Entity,
    kind: ShotKind,
    (x, y): (f32, f32),
) {
    let sprite = enemies::shot_sprite(kind);
    match kind {
        ShotKind::CryptArrow => {
            let degrees = entity
                .body
                .velocity_y
                .atan2(entity.body.velocity_x)
                .to_degrees();
            sprite::draw_sprite_rotated(draw, sprite, 0, Vector2::new(x, y), ART, degrees);
        }
        ShotKind::Boulder => {
            draw.draw_ellipse(
                x as i32,
                (y + 10.0) as i32,
                13.0,
                5.0,
                Color::new(0, 0, 0, 110),
            );
            let frame = ((crypt.seconds * 8.0) as usize) % 2;
            let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
            sprite::draw_sprite(
                draw,
                sprite,
                frame,
                Vector2::new(super::snap(x - width * 0.5), super::snap(y - height * 0.5)),
                ART,
                Color::WHITE,
            );
        }
        _ => {
            draw.draw_circle_v(Vector2::new(x, y), 9.0, with_alpha(LAVENDER, 60));
            let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
            sprite::draw_sprite(
                draw,
                sprite,
                sprite.frame_at(crypt.seconds),
                Vector2::new(super::snap(x - width * 0.5), super::snap(y - height * 0.5)),
                ART,
                Color::WHITE,
            );
        }
    }
}

/// Flames bursting out of active vents (over the bodies standing in them).
fn draw_flames(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    for trap in &crypt.run.crypt.traps {
        if trap.kind != CryptTrapKind::FlameJet
            || trap.stage(crypt.millis) != TrapStage::Active
            || !crypt.revealed(trap.at)
        {
            continue;
        }
        let (cx, cy) = (
            (trap.at.x as f32 + 0.5) * TILE_SIZE,
            (trap.at.y as f32 + 0.5) * TILE_SIZE,
        );
        let t = crypt.seconds * 9.0 + trap.at.x as f32;
        for k in 0..4 {
            let lift = k as f32 * 7.0 + (t + k as f32).sin() * 2.0;
            let size = 11.0 - k as f32 * 2.0;
            draw.draw_circle_v(
                Vector2::new(cx + (t * 1.3 + k as f32).sin() * 2.0, cy - lift),
                size,
                with_alpha(ORANGE, 200),
            );
            draw.draw_circle_v(
                Vector2::new(cx, cy - lift + 1.0),
                size * 0.55,
                with_alpha(GOLD, 220),
            );
        }
    }
}

/// Darkness but for the party's lamps, lit shrines, burning vents and hex light.
fn draw_light(draw: &mut impl RaylibDraw, crypt: &Crypt<'_, '_>) {
    let scene = crypt.scene;
    let visible = scene.visible;
    let mut lights: Vec<(f32, f32, f32)> = Vec::new();
    for pilot in scene.pilots {
        let (x, y) = pilot.middle();
        let reach = if pilot.state.boons.trap_sight {
            11.0
        } else {
            9.0
        };
        lights.push((x, y, reach * TILE_SIZE));
    }
    let (left, top, right, bottom) = crypt.tiles;
    for y in top..bottom {
        for x in left..right {
            if let Some(TileKind::Built(BuiltTile::TombShrine { lit: true })) =
                crypt.kind(TilePosition { x, y })
            {
                lights.push((
                    (x as f32 + 0.5) * TILE_SIZE,
                    (y as f32 + 0.5) * TILE_SIZE,
                    5.0 * TILE_SIZE,
                ));
            }
        }
    }
    for trap in &crypt.run.crypt.traps {
        if trap.kind == CryptTrapKind::FlameJet && trap.stage(crypt.millis) != TrapStage::Idle {
            lights.push((
                (trap.at.x as f32 + 0.5) * TILE_SIZE,
                (trap.at.y as f32 + 0.5) * TILE_SIZE,
                2.2 * TILE_SIZE,
            ));
        }
    }
    for entity in scene.enemies {
        if let EntityState::EnemyShot(shot) = &entity.state
            && shot.kind == ShotKind::CryptHex
        {
            lights.push((entity.body.x, entity.body.y, 1.5 * TILE_SIZE));
        }
    }
    let lights: Vec<(f32, f32, f32)> = lights
        .into_iter()
        .filter(|(x, y, reach)| {
            *x > visible.x - reach
                && *x < visible.x + visible.width + reach
                && *y > visible.y - reach
                && *y < visible.y + visible.height + reach
        })
        .collect();
    let x0 = (visible.x / LIGHT_CELL).floor() * LIGHT_CELL;
    let y0 = (visible.y / LIGHT_CELL).floor() * LIGHT_CELL;
    let mut y = y0;
    while y < visible.y + visible.height {
        let mut x = x0;
        while x < visible.x + visible.width {
            let (cx, cy) = (x + LIGHT_CELL * 0.5, y + LIGHT_CELL * 0.5);
            let light = lights
                .iter()
                .map(|(lx, ly, reach)| {
                    let d = (cx - lx).hypot(cy - ly) / reach;
                    (1.0 - d * d).max(0.0)
                })
                .fold(0.0_f32, f32::max);
            let dark = (1.0 - light).clamp(0.0, 1.0) * 0.74;
            if dark > 0.02 {
                draw.draw_rectangle_v(
                    Vector2::new(x, y),
                    Vector2::new(LIGHT_CELL, LIGHT_CELL),
                    Color::new(0, 0, 0, (dark * 255.0) as u8),
                );
            }
            x += LIGHT_CELL;
        }
        y += LIGHT_CELL;
    }
}
