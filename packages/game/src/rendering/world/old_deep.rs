//! Descent II, the Old Deep, in the world (Wave 8): what the tile art cannot show because it
//! moves or depends on who looks.
//!
//! * glowing tiles' halos (crystal, glow rock, isotopes, pylons, storm vents, the core);
//! * the pressure jets' plumes (a hissing tell, then the blast) and the pistons' rams;
//! * belts' moving chevrons in an awake machine hall;
//! * the Cathedral Lift's valves being held (a filling ring);
//! * the Star Core's shield ring and the pylons' feed beams;
//! * the refracted headlamp: each seat's own beams fanning out of the prisms its lamp reaches,
//!   and the latent gems they light, drawn after the darkness pass so the beams cut through it;
//! * embers in the air while a heat storm rages.
//!
//! Every moving part follows the simulation's pure cycles (`old_deep::jet_active`,
//! `piston_extension`, `storm_phase`) at the drawn world time, so every seat and peer sees the
//! jets and pistons where the simulation has them. Glows pulse slowly (under 1 Hz) and never
//! flash: photosensitivity-safe.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "art pixels are small integers drawn at float scales"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    old_deep::{
        self, CorePhase, DeepTile, Mutation, OldDeep, StormPhase,
        refraction::{self, Refraction},
        sites::{ArenaSite, CathedralSite},
    },
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use crate::render_snapshot::PlayerPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::hash;

/// What the Old Deep layer needs from the view.
pub(super) struct DeepScene<'a> {
    pub terrain: &'a Terrain,
    pub old_deep: &'a OldDeep,
    pub visible: Rectangle,
    pub world_tick: u64,
    pub tick_rate_hz: u32,
    pub world_millis: u64,
    pub seconds: f32,
    pub local: Option<&'a PlayerPresentation>,
    pub cathedral: Option<&'a CathedralSite>,
    pub arena: Option<&'a ArenaSite>,
}

impl DeepScene<'_> {
    const fn active(&self) -> bool {
        self.terrain.settings().generator.has_descents()
    }

    fn tiles(&self) -> (TilePosition, TilePosition) {
        let frame = crate::view_frame::frame();
        (
            TilePosition {
                x: frame.col(self.visible.x) - 1,
                y: frame.row(self.visible.y) - 1,
            },
            TilePosition {
                x: frame.col_ceil(self.visible.x + self.visible.width) + 1,
                y: frame.row_ceil(self.visible.y + self.visible.height) + 1,
            },
        )
    }
}

fn centre(tile: TilePosition) -> (f32, f32) {
    let frame = crate::view_frame::frame();
    (
        (frame.rel_col(tile.x) + 0.5) * TILE_SIZE,
        (frame.rel_row(tile.y) + 0.5) * TILE_SIZE,
    )
}

const fn glow_color(tile: DeepTile) -> Option<Color> {
    Some(match tile {
        DeepTile::ResonantCrystal | DeepTile::Prism => SKY_LIGHT,
        DeepTile::GlowRock | DeepTile::IsotopeCluster => LIME,
        DeepTile::MutationRelic | DeepTile::VoidBloom => PINK,
        DeepTile::CorePylon | DeepTile::LiftValveOpen => MINT,
        DeepTile::StormVent | DeepTile::CinderRock => ORANGE,
        DeepTile::Amber | DeepTile::BuilderTablet | DeepTile::ExpeditionLog => AMBER,
        DeepTile::FirstCore | DeepTile::FirstCoreShielded => CREAM,
        _ => return None,
    })
}

/// The glow radius (tiles) a deep tile lights the cave darkness with.
#[must_use]
pub(super) const fn deep_light(tile: DeepTile) -> f32 {
    match tile {
        DeepTile::FirstCore | DeepTile::FirstCoreShielded => 5.0,
        DeepTile::GlowRock | DeepTile::IsotopeCluster | DeepTile::CorePylon => 1.8,
        DeepTile::StormVent => 2.2,
        DeepTile::ResonantCrystal
        | DeepTile::Prism
        | DeepTile::MutationRelic
        | DeepTile::VoidBloom
        | DeepTile::LiftValveOpen => 1.2,
        DeepTile::Amber | DeepTile::BuilderTablet | DeepTile::ExpeditionLog => 0.8,
        _ => 0.0,
    }
}

/// Halos round the glowing tiles, jets' plumes, pistons' rams, the belts and the set pieces
/// (over the terrain, before the rigs).
pub(super) fn draw_deep_world(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>) {
    if !scene.active() {
        return;
    }
    let (first, last) = scene.tiles();
    let millis = scene.world_millis;
    for y in first.y..=last.y {
        for x in first.x..=last.x {
            let position = TilePosition { x, y };
            let Some(TileKind::Deep(tile)) = scene.terrain.tile(position).map(|t| t.kind) else {
                continue;
            };
            let (cx, cy) = centre(position);
            if let Some(color) = glow_color(tile) {
                // A slow breathing glow, out of step tile to tile.
                let phase = (hash(x, y, 851) % 628) as f32 / 100.0;
                let pulse = (scene.seconds * 0.9 + phase).sin().mul_add(0.5, 0.5);
                let radius = TILE_SIZE * deep_light(tile).max(0.6) * 0.55;
                let alpha = 10.0 + pulse * 12.0;
                draw.draw_circle(cx as i32, cy as i32, radius, with_alpha(color, alpha as u8));
            }
            match tile {
                DeepTile::JetVent => draw_jet(draw, scene, position, millis),
                DeepTile::PistonHousing => draw_ram(draw, scene, position, millis),
                DeepTile::ConveyorLeft | DeepTile::ConveyorRight => {
                    draw_belt_motion(draw, scene, position, tile == DeepTile::ConveyorLeft);
                }
                _ => {}
            }
        }
    }
    if let Some(site) = scene.cathedral {
        draw_valve_holds(draw, scene, site);
    }
    if let Some(arena) = scene.arena {
        draw_core(draw, scene, arena);
    }
}

fn draw_jet(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>, vent: TilePosition, millis: u64) {
    let calmed = !scene.old_deep.cathedral.holds.is_empty()
        && scene
            .cathedral
            .is_some_and(|site| site.bounds.contains(vent));
    let active = old_deep::jet_active(vent, millis) && !calmed;
    let warning = old_deep::jet_warning(vent, millis) && !calmed;
    if !active && !warning {
        return;
    }
    // Rise until rock stops it.
    let mut rows = 0;
    while rows < old_deep::JET_REACH_ROWS {
        let above = TilePosition {
            x: vent.x,
            y: vent.y - rows - 1,
        };
        if scene
            .terrain
            .tile(above)
            .is_none_or(|tile| !tile.kind.is_open())
        {
            break;
        }
        rows += 1;
    }
    let (cx, cy) = centre(vent);
    let top = cy - TILE_SIZE * 0.5;
    if warning {
        // The tell: wisps of steam curling off the grate.
        for wisp in 0..4 {
            let t = (scene.seconds * 1.6 + wisp as f32 * 0.25).fract();
            let x = cx + ((wisp as f32 * 1.7 + scene.seconds * 2.0).sin()) * 6.0;
            let y = top - t * TILE_SIZE * 1.5;
            draw.draw_circle(
                x as i32,
                y as i32,
                3.0 + t * 4.0,
                with_alpha(MIST, (90.0 * (1.0 - t)) as u8),
            );
        }
        return;
    }
    let height = rows as f32 * TILE_SIZE;
    // The blast: a pale column, brighter at its core, with puffs racing up it.
    draw.draw_rectangle(
        (cx - 11.0) as i32,
        (top - height) as i32,
        22,
        height as i32,
        with_alpha(SKY_LIGHT, 40),
    );
    draw.draw_rectangle(
        (cx - 5.0) as i32,
        (top - height) as i32,
        10,
        height as i32,
        with_alpha(ICE, 70),
    );
    for puff in 0..6 {
        let t = (scene.seconds * 2.4 + puff as f32 / 6.0).fract();
        let y = top - t * height;
        let x = cx + ((puff as f32 * 2.3 + scene.seconds * 5.0).sin()) * 5.0;
        draw.draw_circle(
            x as i32,
            y as i32,
            5.0 + t * 6.0,
            with_alpha(BONE, (80.0 * (1.0 - t)) as u8),
        );
    }
}

fn draw_ram(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>, housing: TilePosition, millis: u64) {
    // Dormant halls hold their rams up: only an awake hall's pistons run.
    let reach = if hall_awake(scene, housing) {
        old_deep::piston_extension(housing, millis)
    } else {
        0.0
    };
    let (cx, cy) = centre(housing);
    let top = cy + TILE_SIZE * 0.5;
    let length = reach * TILE_SIZE;
    // The shaft, then the ram's head with hazard stripes.
    draw.draw_rectangle(
        (cx - 5.0) as i32,
        top as i32,
        10,
        (length - 10.0).max(0.0) as i32,
        STEEL,
    );
    draw.draw_rectangle(
        (cx - 3.0) as i32,
        top as i32,
        3,
        (length - 10.0).max(0.0) as i32,
        STEEL_LIGHT,
    );
    // The head is the ram's last 12 pixels (it rests in the housing's mouth when retracted).
    let head_y = top + (length - 12.0).max(-12.0);
    draw.draw_rectangle((cx - 14.0) as i32, head_y as i32, 28, 10, STEEL_DARK);
    for stripe in 0..4 {
        draw.draw_rectangle(
            (cx - 14.0 + stripe as f32 * 7.0) as i32,
            (head_y + 6.0) as i32,
            4,
            4,
            GOLD,
        );
    }
    draw.draw_rectangle((cx - 14.0) as i32, (head_y + 10.0) as i32, 28, 2, INK);
}

/// Whether the machine hall holding `tile` is awake.
fn hall_awake(scene: &DeepScene<'_>, tile: TilePosition) -> bool {
    if scene.old_deep.woken_halls.is_empty() {
        return false;
    }
    let settings = scene.terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let area = fatherload_simulation::worldgen::TileRect {
        min: tile,
        max: tile,
    };
    fatherload_simulation::old_deep::sites::machine_halls_near(settings, &layout, &area, 0)
        .iter()
        .any(|hall| scene.old_deep.woken_halls.contains(&hall.region))
}

fn draw_belt_motion(
    draw: &mut impl RaylibDraw,
    scene: &DeepScene<'_>,
    belt: TilePosition,
    left: bool,
) {
    if !hall_awake(scene, belt) {
        return;
    }
    let (cx, cy) = centre(belt);
    let top = cy - TILE_SIZE * 0.5;
    let shift = (scene.seconds * 0.9).fract() * TILE_SIZE * if left { -1.0 } else { 1.0 };
    for dash in 0..3 {
        let x = cx - TILE_SIZE * 0.5 + ((dash as f32 * 11.0 + shift).rem_euclid(TILE_SIZE));
        draw.draw_rectangle(x as i32, (top + 2.0) as i32, 5, 2, with_alpha(AMBER, 200));
    }
}

fn draw_valve_holds(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>, site: &CathedralSite) {
    let hold_ticks = u64::from(scene.tick_rate_hz) * old_deep::VALVE_HOLD_MILLIS / 1_000;
    for (&index, &(_, start)) in &scene.old_deep.cathedral.holds {
        let Some(valve) = site.valves.get(usize::from(index)) else {
            continue;
        };
        let progress = (scene.world_tick.saturating_sub(start.get()) as f32
            / hold_ticks.max(1) as f32)
            .min(1.0);
        let (cx, cy) = centre(*valve);
        draw.draw_ring(
            Vector2::new(cx, cy),
            20.0,
            24.0,
            -90.0,
            -90.0 + 360.0 * progress,
            32,
            LIME,
        );
    }
}

fn draw_core(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>, arena: &ArenaSite) {
    let (cx, cy) = centre(arena.core);
    let fight = &scene.old_deep.core;
    let pulse = (scene.seconds * 0.8).sin().mul_add(0.5, 0.5);
    match fight.phase {
        CorePhase::Beaten => {
            draw.draw_circle(cx as i32, cy as i32, TILE_SIZE * 1.2, with_alpha(SLATE, 30));
        }
        CorePhase::Exposed => {
            // The open core: a bright, swelling corona.
            draw.draw_circle(
                cx as i32,
                cy as i32,
                TILE_SIZE * (2.2 + pulse * 0.4),
                with_alpha(GOLD, 50),
            );
            draw.draw_circle(cx as i32, cy as i32, TILE_SIZE * 1.1, with_alpha(CREAM, 80));
        }
        CorePhase::Dormant | CorePhase::Shielded => {
            draw.draw_circle(
                cx as i32,
                cy as i32,
                TILE_SIZE * (1.6 + pulse * 0.2),
                with_alpha(AMBER, 36),
            );
            // The shield: a cyan ring, and a beam from each standing pylon feeding it.
            draw.draw_ring(
                Vector2::new(cx, cy),
                TILE_SIZE * 1.15,
                TILE_SIZE * 1.3,
                0.0,
                360.0,
                48,
                with_alpha(MINT, 150),
            );
            for pylon in arena.pylons {
                if scene.terrain.tile(pylon).map(|t| t.kind)
                    != Some(TileKind::Deep(DeepTile::CorePylon))
                {
                    continue;
                }
                let (px, py) = centre(pylon);
                draw.draw_line_ex(
                    Vector2::new(px, py - TILE_SIZE * 0.4),
                    Vector2::new(cx, cy),
                    3.0,
                    with_alpha(MINT, (70.0 + pulse * 60.0) as u8),
                );
                draw.draw_line_ex(
                    Vector2::new(px, py - TILE_SIZE * 0.4),
                    Vector2::new(cx, cy),
                    1.0,
                    with_alpha(ICE, 200),
                );
            }
        }
    }
}

/// The refracted headlamp of the view's rig, computed exactly as the simulation does.
#[must_use]
pub(super) fn local_refraction(scene: &DeepScene<'_>) -> Refraction {
    let Some(local) = scene.local else {
        return Refraction::default();
    };
    if !scene.active() {
        return Refraction::default();
    }
    let frame = crate::view_frame::frame();
    let eye = frame.tile_at(local.x, local.y);
    let tile_at = |at: TilePosition| scene.terrain.tile(at).map(|tile| tile.kind);
    let mut out = refraction::refract(tile_at, eye, refraction::LAMP_REACH_TILES);
    let now = fatherload_simulation::SimulationTick::new(scene.world_tick);
    if scene.old_deep.mutation(local.player_id, now) == Some(Mutation::Lumen) {
        for gem in refraction::glow_reveals(tile_at, eye, 4) {
            if !out.gems.contains(&gem) {
                out.gems.push(gem);
            }
        }
    }
    out
}

/// The refracted beams and the gems they light, over the darkness (they are light).
pub(super) fn draw_refraction(
    draw: &mut impl RaylibDraw,
    scene: &DeepScene<'_>,
    refraction: &Refraction,
) {
    let hues = [ROSE, SKY_LIGHT, LIME];
    for beam in &refraction.beams {
        let (fx, fy) = centre(beam.from);
        let (tx, ty) = centre(beam.to);
        let color = hues[usize::from(beam.hue).min(2)];
        let shimmer = (scene.seconds * 0.7 + f32::from(beam.hue))
            .sin()
            .mul_add(0.15, 0.85);
        draw.draw_line_ex(
            Vector2::new(fx, fy),
            Vector2::new(tx, ty),
            16.0,
            with_alpha(color, (45.0 * shimmer) as u8),
        );
        draw.draw_line_ex(
            Vector2::new(fx, fy),
            Vector2::new(tx, ty),
            7.0,
            with_alpha(color, (90.0 * shimmer) as u8),
        );
        draw.draw_line_ex(
            Vector2::new(fx, fy),
            Vector2::new(tx, ty),
            3.0,
            with_alpha(color, (190.0 * shimmer) as u8),
        );
        draw.draw_line_ex(
            Vector2::new(fx, fy),
            Vector2::new(tx, ty),
            1.0,
            with_alpha(BONE, 200),
        );
    }
    for prism in &refraction.prisms {
        let (cx, cy) = centre(*prism);
        draw.draw_circle(cx as i32, cy as i32, TILE_SIZE * 0.6, with_alpha(ICE, 70));
    }
    for gem in &refraction.gems {
        // The gem shows through the rock: a cut stone with a glint.
        let (cx, cy) = centre(*gem);
        let glint = (scene.seconds * 0.9 + (gem.x + gem.y) as f32)
            .sin()
            .mul_add(0.5, 0.5);
        draw.draw_circle(
            cx as i32,
            cy as i32,
            TILE_SIZE * 0.55,
            with_alpha(SKY_LIGHT, 60),
        );
        let points = [
            Vector2::new(cx, cy - 9.0),
            Vector2::new(cx - 8.0, cy - 2.0),
            Vector2::new(cx, cy + 9.0),
            Vector2::new(cx + 8.0, cy - 2.0),
        ];
        draw.draw_triangle(points[0], points[1], points[2], SKY);
        draw.draw_triangle(points[0], points[2], points[3], ICE);
        draw.draw_line_ex(points[1], points[3], 1.0, BONE);
        draw.draw_circle((cx - 3.0) as i32, (cy - 4.0) as i32, 1.5 + glint, BONE);
    }
}

/// Embers drifting up through the view while a heat storm rages in the Mantle Storm Zone.
pub(super) fn draw_storm_air(draw: &mut impl RaylibDraw, scene: &DeepScene<'_>, in_zone: bool) {
    if !in_zone {
        return;
    }
    let phase = old_deep::storm_phase(scene.world_millis);
    let count = match phase {
        StormPhase::Calm => return,
        StormPhase::Gathering => 18,
        StormPhase::Raging => 60,
    };
    let v = scene.visible;
    for ember in 0..count {
        let seed = hash(ember, 7, 861);
        let speed = 0.08 + (seed % 100) as f32 / 900.0;
        let t = (scene.seconds * speed + (seed % 1000) as f32 / 1000.0).fract();
        let x = v.x
            + ((seed / 7) % 1000) as f32 / 1000.0 * v.width
            + (scene.seconds * 0.7 + ember as f32).sin() * 10.0;
        let y = v.y + v.height * (1.0 - t);
        let color = if seed.is_multiple_of(3) { GOLD } else { ORANGE };
        draw.draw_rectangle(x as i32, y as i32, 2, 2, with_alpha(color, 170));
    }
}
