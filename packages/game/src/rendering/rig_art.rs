//! The modular rig: one sprite assembled from layered parts, recoloured by the player's paint
//! job.
//!
//! A rig is painted into a [`RigCanvas`] (a small grid of art pixels around the 16x13 body box,
//! authored facing right), then drawn at any scale: world space (`WORLD_PIXELS_PER_ART_PIXEL`),
//! interior rooms (one room pixel per art pixel) and UI previews (garage, rig sheet). Layers, back
//! to front:
//!
//! 1. thruster flames (belly flame plus one per pod nozzle, in the cosmetic flame colour; longer
//!    and wider with engine tier),
//! 2. the drill head (a distinct material, size and accent per drill tier; its silhouette from
//!    the cosmetic drill style): on the nose, below the rig or on the roof,
//! 3. the hull (primary paint, secondary skirt band, trim stripe, hull pattern), the glass dome,
//!    the tracks,
//! 4. decal, radiator grille (radiator tier), armour plating (hull tier), the drill mount, the
//!    weapon hardpoint (empty for now: see [`HARDPOINT`]),
//! 5. cargo pods on the rear deck (bigger with cargo tier), thruster pods on the tail (engine
//!    tier), the mast with its beacon and the roof / nose / tail attachments.
//!
//! The parts and their tier classes are in `parts`, the drill heads in `drill`, colours in
//! `paint`. Everything uses palette colours only.
//!
//! # API for other tracks
//!
//! - [`RigLook::from_player`] / [`RigLook::from_presentation`] build the look of a rig;
//!   [`paint_rig`] paints it for a [`RigPose`]; [`RigCanvas::draw`] draws it.
//! - [`HARDPOINT`]: the art pixel (body coordinates, facing right) where a hardpoint weapon
//!   mounts (WP-WEAPONS draws its weapon there).
//! - [`player_accent`]: a player's identity colour, from their primary paint.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "art pixels are small integers drawn at float scales"
)]

mod drill;
mod paint;
mod parts;
#[cfg(not(target_os = "emscripten"))]
pub mod sheet;

use fatherload_simulation::{Player, PlayerId, RigCosmetics};
use raylib::prelude::*;

pub use paint::{flame_ramp, paint_accent, paint_ramp};
#[allow(
    unused_imports,
    reason = "HARDPOINT is the weapon mount for WP-WEAPONS (wave 2)"
)]
pub use parts::HARDPOINT;
pub use parts::{BODY_HEIGHT, BODY_WIDTH};

use crate::render_snapshot::PlayerPresentation;
pub use crate::render_snapshot::RigTiers;

/// Everything that decides how a rig looks: whose it is, its cosmetics (resolved) and its tiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RigLook {
    pub player_id: PlayerId,
    pub cosmetics: RigCosmetics,
    pub tiers: RigTiers,
}

impl RigLook {
    /// The look of `player`'s rig (their garage paint job, or their default).
    #[must_use]
    pub fn from_player(player_id: PlayerId, player: &Player) -> Self {
        Self {
            player_id,
            cosmetics: RigCosmetics::resolve(player.cosmetics, player_id),
            tiers: RigTiers::of(player),
        }
    }

    /// The look of a presented rig.
    #[must_use]
    pub const fn from_presentation(player: &PlayerPresentation) -> Self {
        Self {
            player_id: player.player_id,
            cosmetics: player.cosmetics,
            tiers: player.tiers,
        }
    }

    /// The same rig wearing `cosmetics` (garage preview).
    #[must_use]
    pub const fn wearing(mut self, cosmetics: RigCosmetics) -> Self {
        self.cosmetics = cosmetics;
        self
    }

    /// Where the exhaust puffs leave the rig (body art pixel, facing right).
    #[must_use]
    pub const fn exhaust(&self) -> (i32, i32) {
        parts::exhaust_origin(self)
    }
}

/// A player's identity colour (name tags, split-screen HUD title, minimap marker): their
/// primary paint's accent, from their garage paint job or their default look.
#[must_use]
pub fn player_accent(player_id: PlayerId, cosmetics: Option<RigCosmetics>) -> Color {
    paint_accent(RigCosmetics::resolve(cosmetics, player_id).primary)
}

/// Where the drill head is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrillPose {
    /// On the nose (spinning while it drills sideways).
    Nose { spinning: bool },
    /// Below the rig, spinning.
    Down,
    /// On the roof pointing up (crown bore), spinning.
    Up,
}

/// How hard the thrusters burn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Thrust {
    Off,
    /// Holding altitude: short flames.
    Hover,
    /// Climbing: long flames.
    Climb,
}

/// One frame's pose of a rig.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RigPose {
    pub drill: DrillPose,
    pub thrust: Thrust,
    /// Track animation frame (0 or 1).
    pub track_frame: usize,
    /// One art pixel of drilling shake (0 or 1): the body moves against the drill.
    pub shake: i32,
    /// Fast animation step (drill spin, flame flicker).
    pub spin: usize,
    /// Slow animation clock (shell ticks): beacons, lamps, pennants.
    pub ticks: u64,
    /// The beacon lamp colour, if lit this frame.
    pub beacon: Option<Color>,
    /// Scorched paint and a cracked dome (hull under 30%).
    pub damaged: bool,
    /// The gimbal ring glows (drilling sideways in flight).
    pub gimbal_live: bool,
}

impl RigPose {
    /// A rig standing still on the ground, nose drill idle.
    #[must_use]
    pub const fn idle(ticks: u64) -> Self {
        Self {
            drill: DrillPose::Nose { spinning: false },
            thrust: Thrust::Off,
            track_frame: 0,
            shake: 0,
            spin: (ticks / 3) as usize,
            ticks,
            beacon: None,
            damaged: false,
            gimbal_live: false,
        }
    }
}

/// Art pixels around a rig, in body coordinates (the 16x13 body box at `0..16 x 0..13`, facing
/// right), with room for overhanging parts, flames and drills.
pub struct RigCanvas {
    cells: Vec<Option<Color>>,
}

impl Default for RigCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl RigCanvas {
    const X0: i32 = -10;
    const Y0: i32 = -14;
    const WIDTH: i32 = 44;
    const HEIGHT: i32 = 46;

    /// An empty canvas.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cells: vec![None; (Self::WIDTH * Self::HEIGHT) as usize],
        }
    }

    fn index(x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = (x - Self::X0, y - Self::Y0);
        ((0..Self::WIDTH).contains(&cx) && (0..Self::HEIGHT).contains(&cy))
            .then(|| (cy * Self::WIDTH + cx) as usize)
    }

    /// Paints one art pixel (outside the canvas: ignored).
    pub fn set(&mut self, x: i32, y: i32, color: Color) {
        if let Some(index) = Self::index(x, y) {
            self.cells[index] = Some(color);
        }
    }

    /// The colour of one art pixel.
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> Option<Color> {
        Self::index(x, y).and_then(|index| self.cells[index])
    }

    /// Paints a `w`x`h` rectangle.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        for row in y..y + h {
            for column in x..x + w {
                self.set(column, row, color);
            }
        }
    }

    /// Paints char rows with their top-left at `(x, y)`; `legend` maps a char to its colour
    /// (`None`, `.` and space are transparent).
    pub fn stamp(&mut self, rows: &[&str], x: i32, y: i32, legend: impl Fn(char) -> Option<Color>) {
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                if let Some(color) = legend(ch) {
                    self.set(x + dx as i32, y + dy as i32, color);
                }
            }
        }
    }

    /// The painted bounds `(min_x, min_y, max_x, max_y)` (inclusive), if anything is painted.
    #[must_use]
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut bounds: Option<(i32, i32, i32, i32)> = None;
        for cy in 0..Self::HEIGHT {
            for cx in 0..Self::WIDTH {
                if self.cells[(cy * Self::WIDTH + cx) as usize].is_some() {
                    let (x, y) = (cx + Self::X0, cy + Self::Y0);
                    bounds = Some(bounds.map_or((x, y, x, y), |(a, b, c, d)| {
                        (a.min(x), b.min(y), c.max(x), d.max(y))
                    }));
                }
            }
        }
        bounds
    }

    /// Draws the canvas with the body box's top-left at `origin`, `scale` target units per art
    /// pixel, mirrored around the body box when `flip` (facing left). Runs of one colour in a
    /// row become one rectangle.
    pub fn draw(&self, draw: &mut impl RaylibDraw, origin: Vector2, scale: f32, flip: bool) {
        let mirror = |x: i32| if flip { BODY_WIDTH - 1 - x } else { x };
        let (first, last) = {
            let (a, b) = (mirror(Self::X0), mirror(Self::X0 + Self::WIDTH - 1));
            (a.min(b), a.max(b))
        };
        for y in Self::Y0..Self::Y0 + Self::HEIGHT {
            let mut run: Option<(i32, Color)> = None;
            for tx in first..=last + 1 {
                let color = if tx > last {
                    None
                } else {
                    self.get(mirror(tx), y)
                };
                match (run, color) {
                    (Some((_, current)), Some(next)) if current == next => {}
                    _ => {
                        if let Some((start, current)) = run.take() {
                            draw.draw_rectangle_rec(
                                Rectangle::new(
                                    start as f32 * scale + origin.x,
                                    y as f32 * scale + origin.y,
                                    (tx - start) as f32 * scale,
                                    scale,
                                ),
                                current,
                            );
                        }
                        run = color.map(|next| (tx, next));
                    }
                }
            }
        }
    }
}

/// Paints a rig: its look in a pose.
#[must_use]
pub fn paint_rig(look: &RigLook, pose: &RigPose) -> RigCanvas {
    let mut canvas = RigCanvas::new();
    parts::paint_flames(&mut canvas, look, pose);
    drill::paint_drill(&mut canvas, look, pose);
    parts::paint_body(&mut canvas, look, pose);
    canvas
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::cosmetics::{DrillStyle, FlameColor, PaintColor};

    use super::*;

    type UpgradeFn = fn(&mut RigTiers);

    fn look(tier: u8) -> RigLook {
        RigLook {
            player_id: PlayerId::new(1),
            cosmetics: RigCosmetics::default_for(PlayerId::new(1)),
            tiers: RigTiers::all(tier),
        }
    }

    #[test]
    fn every_tier_paints_inside_the_canvas_in_every_pose() {
        for tier in 1..=15 {
            for style in DrillStyle::ALL {
                for drill in [
                    DrillPose::Nose { spinning: true },
                    DrillPose::Down,
                    DrillPose::Up,
                ] {
                    let mut rig = look(tier);
                    rig.cosmetics.drill_style = *style;
                    rig.cosmetics.flame = FlameColor::Prism;
                    let pose = RigPose {
                        drill,
                        thrust: Thrust::Climb,
                        ..RigPose::idle(7)
                    };
                    let canvas = paint_rig(&rig, &pose);
                    let (min_x, min_y, max_x, max_y) = canvas.bounds().expect("painted");
                    assert!(min_x > RigCanvas::X0 && max_x < RigCanvas::X0 + RigCanvas::WIDTH - 1);
                    assert!(min_y > RigCanvas::Y0 && max_y < RigCanvas::Y0 + RigCanvas::HEIGHT - 1);
                }
            }
        }
    }

    #[test]
    fn drill_tiers_look_different() {
        let mut seen = Vec::new();
        for tier in 1..=15 {
            let mut rig = look(1);
            rig.tiers.drill = tier;
            let canvas = paint_rig(
                &rig,
                &RigPose {
                    drill: DrillPose::Down,
                    ..RigPose::idle(0)
                },
            );
            let drill: Vec<Option<Color>> = (12..28)
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .map(|(x, y)| canvas.get(x, y))
                .collect();
            assert!(!seen.contains(&drill), "drill tier {tier} repeats");
            seen.push(drill);
        }
    }

    #[test]
    fn paint_and_tiers_change_the_rig() {
        let base = paint_rig(&look(1), &RigPose::idle(0));
        let mut repainted = look(1);
        repainted.cosmetics.primary = PaintColor::Cobalt;
        let other = paint_rig(&repainted, &RigPose::idle(0));
        assert_ne!(base.get(6, 6), other.get(6, 6));
        let changes: [(&str, UpgradeFn); 4] = [
            ("engine", |tiers| tiers.engine = 8),
            ("cargo", |tiers| tiers.cargo = 8),
            ("hull", |tiers| tiers.hull = 8),
            ("radiator", |tiers| tiers.radiator = 8),
        ];
        for (line, change) in changes {
            let mut upgraded = look(1);
            change(&mut upgraded.tiers);
            let canvas = paint_rig(&upgraded, &RigPose::idle(0));
            assert!(
                canvas.cells != base.cells,
                "{line} upgrades show on the rig"
            );
        }
    }

    #[test]
    fn accents_follow_the_chosen_paint() {
        let id = PlayerId::new(2);
        assert_eq!(player_accent(id, None), paint_accent(PaintColor::Teal));
        let chosen = RigCosmetics {
            primary: PaintColor::Crimson,
            ..RigCosmetics::default()
        };
        assert_eq!(
            player_accent(id, Some(chosen)),
            paint_accent(PaintColor::Crimson)
        );
    }
}
