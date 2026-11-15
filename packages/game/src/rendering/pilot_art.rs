//! The pilot's sprite sheet (Part II F5), painted procedurally like the rig: a helmeted
//! prospector in a suit wearing the player's paint job (suit = primary colour, pack, boots and
//! gloves = secondary, belt and pads = trim), with a frame set for every pose: idle (breathing),
//! walk and run cycles, jump, fall, climb (seen from behind), crouch, crawl, swim, hurt,
//! jetpack and ledge hang.
//!
//! A [`PilotCanvas`] is a small grid of art pixels around the pilot's feet (x right, y up
//! negative, the feet on row 0's top edge). Parts are painted flat, then
//! [`PilotCanvas::outline`] rings everything in ink, so every pose reads cleanly against any
//! background. Facing left mirrors the canvas around the feet.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "art coordinates are small integers"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    RigCosmetics,
    pilot::{PilotGear, PilotPose},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::rig_art::{Ramp, paint_accent, paint_ramp};

/// Canvas bounds in art pixels around the feet.
const X0: i32 = -10;
const Y0: i32 = -30;
const WIDTH: i32 = 21;
const HEIGHT: i32 = 31;

/// A pilot's colours and gear (from its owner's paint job and the pilot's gear).
#[derive(Clone, Copy, Debug)]
pub struct PilotLook {
    pub suit: Ramp,
    pub gear: Ramp,
    pub trim: Color,
    pub jetpack: bool,
    pub suit_tier: u8,
}

impl PilotLook {
    /// The look of a pilot wearing `cosmetics` with `gear`.
    #[must_use]
    pub const fn new(cosmetics: RigCosmetics, gear: PilotGear) -> Self {
        Self {
            suit: paint_ramp(cosmetics.primary),
            gear: paint_ramp(cosmetics.secondary),
            trim: paint_accent(cosmetics.trim),
            jetpack: gear.jetpack > 0,
            suit_tier: gear.suit,
        }
    }
}

/// One frame of the pilot: a pose and where in its animation it is.
#[derive(Clone, Copy, Debug)]
pub struct PilotFrame {
    pub pose: PilotPose,
    /// Animation clock (seconds in the pose family; any running clock works).
    pub seconds: f32,
    /// The hand pick is out (swinging at a tile).
    pub swinging: bool,
}

/// A small art-pixel grid around the pilot's feet.
#[derive(Clone, Debug)]
pub struct PilotCanvas {
    cells: Vec<Option<Color>>,
}

impl Default for PilotCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl PilotCanvas {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cells: vec![None; (WIDTH * HEIGHT) as usize],
        }
    }

    fn index(x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = (x - X0, y - Y0);
        ((0..WIDTH).contains(&cx) && (0..HEIGHT).contains(&cy)).then(|| (cy * WIDTH + cx) as usize)
    }

    /// One art pixel.
    pub fn set(&mut self, x: i32, y: i32, color: Color) {
        if let Some(index) = Self::index(x, y) {
            self.cells[index] = Some(color);
        }
    }

    /// The colour at an art pixel.
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> Option<Color> {
        Self::index(x, y).and_then(|index| self.cells[index])
    }

    /// A `w`x`h` rectangle with its top-left at `(x, y)`.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        for row in y..y + h {
            for column in x..x + w {
                self.set(column, row, color);
            }
        }
    }

    /// Rings every painted pixel with ink (never below the feet).
    pub fn outline(&mut self) {
        let mut ring = Vec::new();
        for y in Y0..Y0 + HEIGHT {
            for x in X0..X0 + WIDTH {
                if self.get(x, y).is_some() || y >= 0 {
                    continue;
                }
                let touches = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .any(|(dx, dy)| self.get(x + dx, y + dy).is_some_and(|c| c != INK));
                if touches {
                    ring.push((x, y));
                }
            }
        }
        for (x, y) in ring {
            self.set(x, y, INK);
        }
    }

    /// The painted bounds `(min_x, min_y, max_x, max_y)`.
    #[must_use]
    #[cfg(test)]
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let mut bounds: Option<(i32, i32, i32, i32)> = None;
        for y in Y0..Y0 + HEIGHT {
            for x in X0..X0 + WIDTH {
                if self.get(x, y).is_some() {
                    bounds = Some(bounds.map_or((x, y, x, y), |(a, b, c, d)| {
                        (a.min(x), b.min(y), c.max(x), d.max(y))
                    }));
                }
            }
        }
        bounds
    }

    /// Draws the canvas with the feet at `feet` (target units), `scale` units per art pixel,
    /// mirrored when `flip` (facing left); `tint` replaces every colour but ink (hurt flash).
    pub fn draw(
        &self,
        draw: &mut impl RaylibDraw,
        feet: Vector2,
        scale: f32,
        flip: bool,
        tint: Option<Color>,
    ) {
        for y in Y0..Y0 + HEIGHT {
            let mut run: Option<(i32, Color)> = None;
            for screen_x in X0..=X0 + WIDTH {
                let source_x = if flip { -screen_x - 1 } else { screen_x };
                let color = (screen_x < X0 + WIDTH)
                    .then(|| self.get(source_x, y))
                    .flatten()
                    .map(|color| match tint {
                        Some(tint) if color != INK => tint,
                        _ => color,
                    });
                match (run, color) {
                    (Some((_, current)), Some(next)) if current == next => {}
                    _ => {
                        if let Some((start, current)) = run.take() {
                            draw.draw_rectangle_rec(
                                Rectangle::new(
                                    (start as f32).mul_add(scale, feet.x),
                                    (y as f32).mul_add(scale, feet.y),
                                    (screen_x - start) as f32 * scale,
                                    scale,
                                ),
                                current,
                            );
                        }
                        run = color.map(|next| (screen_x, next));
                    }
                }
            }
        }
    }
}

/// Where the limbs and body sit in one frame (art pixels from the feet, facing right).
#[derive(Clone, Copy, Debug, Default)]
struct Rig {
    /// Upper body offset (bob and lean).
    upper_dx: i32,
    upper_dy: i32,
    /// Front and back leg: horizontal offset of the boot and how far it is lifted.
    front_leg: (i32, i32),
    back_leg: (i32, i32),
    /// Front arm: the glove's offset from the hanging position (x forward, y up negative).
    front_arm: (i32, i32),
    back_arm: (i32, i32),
    /// Legs folded (crouch, crawl, tucked jump).
    folded: bool,
    /// Extra drop of the upper body while folded.
    fold_drop: i32,
}

/// Paints one frame of a pilot.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one pose table")]
pub fn paint_pilot(look: &PilotLook, frame: PilotFrame) -> PilotCanvas {
    let mut canvas = PilotCanvas::new();
    let t = frame.seconds;
    let step = |rate: f32, frames: usize| ((t * rate) as usize) % frames;
    if frame.pose == PilotPose::Climb {
        paint_climb(&mut canvas, look, step(6.0, 2));
        canvas.outline();
        return canvas;
    }
    let mut rig = Rig::default();
    match frame.pose {
        PilotPose::Idle => {
            rig.upper_dy = i32::from(step(1.5, 2) == 1);
        }
        PilotPose::Walk => {
            let phase = step(8.0, 4);
            let stride = [1, 0, -1, 0][phase];
            rig.front_leg = (stride, i32::from(phase == 3));
            rig.back_leg = (-stride, i32::from(phase == 1));
            rig.front_arm = (-stride, 0);
            rig.upper_dy = i32::from(phase % 2 == 1);
        }
        PilotPose::Run => {
            let phase = step(12.0, 4);
            let stride = [2, 0, -2, 0][phase];
            rig.front_leg = (stride, if phase == 3 { 2 } else { 0 });
            rig.back_leg = (-stride, if phase == 1 { 2 } else { 0 });
            rig.upper_dx = 1;
            rig.upper_dy = i32::from(phase % 2 == 1);
            rig.front_arm = (if stride > 0 { -1 } else { 2 }, -2);
            rig.back_arm = (if stride > 0 { 2 } else { -1 }, -2);
        }
        PilotPose::Jump => {
            rig.folded = true;
            rig.fold_drop = 0;
            rig.front_leg = (1, 2);
            rig.back_leg = (-1, 1);
            rig.front_arm = (1, -6);
        }
        PilotPose::Fall => {
            rig.front_leg = (1, 0);
            rig.back_leg = (-1, 1);
            rig.front_arm = (2, -5);
            rig.back_arm = (-1, -4);
        }
        PilotPose::Jetpack => {
            let kick = step(10.0, 2) as i32;
            rig.front_leg = (kick, 1);
            rig.back_leg = (-1, 0);
            rig.front_arm = (1, -2);
        }
        PilotPose::Crouch => {
            rig.folded = true;
            rig.fold_drop = 8;
        }
        PilotPose::Crawl => {
            rig.folded = true;
            rig.fold_drop = 9;
            rig.upper_dx = 3;
            let phase = step(6.0, 2) as i32;
            rig.front_arm = (2 + phase, 3);
            rig.front_leg = (-phase, 0);
            rig.back_leg = (phase - 1, 0);
        }
        PilotPose::Swim => {
            let phase = step(4.0, 4);
            let kick = [1, 0, -1, 0][phase];
            rig.front_leg = (kick, 1);
            rig.back_leg = (-kick, 0);
            rig.front_arm = ([3, 1, -1, 1][phase], [-5, -2, 0, -2][phase]);
            rig.upper_dx = 1;
        }
        PilotPose::Hurt => {
            rig.upper_dx = -1;
            rig.front_arm = (2, -3);
            rig.back_arm = (-2, -3);
            rig.back_leg = (-1, 0);
        }
        PilotPose::Hang => {
            // Both hands up over the helmet, gripping the ledge.
            rig.front_arm = (1, -17);
            rig.back_arm = (2, -17);
            rig.front_leg = (0, 0);
            rig.back_leg = (0, 0);
            rig.upper_dy = 0;
        }
        PilotPose::Climb => {}
    }
    if frame.swinging {
        // The pick goes up and comes down in a steady rhythm.
        rig.front_arm = if step(6.6, 2) == 0 { (1, -6) } else { (3, -1) };
    }
    paint_body(&mut canvas, look, &rig, frame);
    canvas.outline();
    canvas
}

/// The side view: back arm, legs, pack, torso, helmet, front arm (and pick).
#[allow(clippy::too_many_lines, reason = "the body's parts in draw order")]
fn paint_body(canvas: &mut PilotCanvas, look: &PilotLook, rig: &Rig, frame: PilotFrame) {
    let suit = look.suit;
    let gear = look.gear;
    // Legs and boots.
    let (leg_top, leg_rows) = if rig.folded { (-4, 3) } else { (-7, 5) };
    let leg = |canvas: &mut PilotCanvas, x: i32, (dx, lift): (i32, i32), front: bool| {
        let color = if front { suit.body } else { suit.shade };
        let boot = if front { gear.shade } else { gear.deep };
        if rig.folded {
            // Thigh forward, shin back down: a bent knee.
            canvas.rect(x + dx, leg_top - lift, 3, 2, color);
            canvas.rect(x + dx + 1, leg_top + 2 - lift, 2, 1, color);
            canvas.rect(x + dx, -1 - lift, 3, 1, boot);
        } else {
            canvas.rect(x + dx, leg_top - lift, 2, leg_rows, color);
            canvas.rect(x + dx, -2 - lift, 3, 2, boot);
            if front && look.suit_tier >= 1 {
                canvas.set(x + dx + 1, -5 - lift, look.trim);
            }
        }
    };
    leg(canvas, -3, rig.back_leg, false);
    leg(canvas, 0, rig.front_leg, true);
    let up_x = rig.upper_dx;
    let up_y = rig.upper_dy + rig.fold_drop;
    let belt = -8 + up_y;
    // Back arm (behind the pack and torso).
    let shoulder = (-1 + up_x, -14 + up_y);
    paint_arm(canvas, shoulder, rig.back_arm, suit.deep, gear.deep);
    // Pack or jetpack.
    let pack_x = -6 + up_x;
    if look.jetpack {
        canvas.rect(pack_x, -15 + up_y, 3, 7, STEEL);
        canvas.rect(pack_x, -15 + up_y, 1, 7, STEEL_LIGHT);
        canvas.rect(pack_x + 2, -15 + up_y, 1, 7, STEEL_DARK);
        canvas.rect(pack_x, -16 + up_y, 3, 1, look.trim);
        canvas.rect(pack_x, -8 + up_y, 1, 2, STEEL_DARK);
        canvas.rect(pack_x + 2, -8 + up_y, 1, 2, STEEL_DARK);
    } else {
        canvas.rect(pack_x, -15 + up_y, 3, 6, gear.body);
        canvas.rect(pack_x, -15 + up_y, 3, 1, gear.light);
        canvas.rect(pack_x, -10 + up_y, 3, 1, gear.shade);
        canvas.set(pack_x + 1, -13 + up_y, look.trim);
    }
    // Belt and torso.
    canvas.rect(-3 + up_x, belt, 5, 1, look.trim);
    canvas.set(up_x, belt, GOLD);
    canvas.rect(-3 + up_x, -15 + up_y, 5, 7, suit.body);
    canvas.rect(-3 + up_x, -15 + up_y, 1, 7, suit.shade);
    canvas.rect(1 + up_x, -14 + up_y, 1, 5, suit.light);
    canvas.rect(-2 + up_x, -15 + up_y, 4, 1, suit.light);
    // A pack strap across the chest and a chest badge.
    canvas.set(-2 + up_x, -14 + up_y, gear.shade);
    canvas.set(-1 + up_x, -13 + up_y, gear.shade);
    canvas.set(up_x, -12 + up_y, look.trim);
    if look.suit_tier >= 2 {
        canvas.rect(-3 + up_x, -15 + up_y, 2, 1, look.trim);
        canvas.set(1 + up_x, -15 + up_y, look.trim);
    }
    if look.suit_tier >= 3 {
        canvas.set(-1 + up_x, -11 + up_y, MINT);
    }
    // Helmet with its visor and lamp.
    let hx = up_x;
    let hy = up_y;
    canvas.rect(-2 + hx, -16 + hy, 4, 1, STONE_DARK);
    canvas.rect(-2 + hx, -23 + hy, 4, 1, BONE);
    canvas.rect(-3 + hx, -22 + hy, 6, 6, MIST);
    canvas.rect(-3 + hx, -22 + hy, 1, 6, FOG);
    canvas.rect(-2 + hx, -22 + hy, 4, 1, BONE);
    canvas.rect(-2 + hx, -18 + hy, 4, 1, FOG);
    canvas.rect(hx, -21 + hy, 3, 3, OCEAN);
    canvas.set(1 + hx, -21 + hy, ICE);
    canvas.set(hx, -21 + hy, SKY);
    canvas.set(2 + hx, -19 + hy, DEEP_BLUE);
    canvas.set(2 + hx, -22 + hy, CREAM);
    if look.suit_tier >= 1 {
        canvas.rect(-3 + hx, -19 + hy, 1, 1, look.trim);
    }
    if look.suit_tier >= 3 {
        canvas.rect(-2 + hx, -25 + hy, 1, 2, STEEL);
        canvas.set(-2 + hx, -26 + hy, look.trim);
    }
    // Front arm, then the pick in its glove.
    let front_shoulder = (1 + up_x, -14 + up_y);
    let glove = paint_arm(canvas, front_shoulder, rig.front_arm, suit.body, gear.body);
    if frame.swinging {
        paint_pick(canvas, glove, rig.front_arm.1 < -3);
    }
}

/// An arm from `shoulder` to a glove offset `reach` from the hanging position (2 px right of the
/// shoulder line, 5 down). Returns where the glove is.
fn paint_arm(
    canvas: &mut PilotCanvas,
    shoulder: (i32, i32),
    reach: (i32, i32),
    sleeve: Color,
    glove: Color,
) -> (i32, i32) {
    let (sx, sy) = shoulder;
    let target = (sx + 1 + reach.0, sy + 5 + reach.1);
    // Walk the arm from the shoulder to the glove in single-pixel steps.
    let (mut x, mut y) = (sx, sy);
    let steps = (target.0 - sx).abs().max((target.1 - sy).abs()).max(1);
    for index in 0..=steps {
        x = sx + (target.0 - sx) * index / steps;
        y = sy + (target.1 - sy) * index / steps;
        canvas.set(x, y, sleeve);
        canvas.set(x, y + i32::from(target.1 >= sy), sleeve);
    }
    canvas.set(x, y, glove);
    (x, y)
}

/// The hand pick held at `glove`: raised (head up behind) or striking (head forward, down).
fn paint_pick(canvas: &mut PilotCanvas, (gx, gy): (i32, i32), raised: bool) {
    if raised {
        canvas.rect(gx, gy - 4, 1, 4, SOIL);
        canvas.rect(gx - 2, gy - 5, 4, 1, STEEL_LIGHT);
        canvas.set(gx - 2, gy - 4, STEEL);
    } else {
        canvas.rect(gx + 1, gy, 3, 1, SOIL);
        canvas.rect(gx + 4, gy - 2, 1, 4, STEEL_LIGHT);
        canvas.set(gx + 4, gy + 2, STEEL);
        canvas.set(gx + 5, gy + 1, CREAM);
    }
}

/// Climbing, seen from behind: helmet, pack, alternate arms and legs reaching up the ladder.
fn paint_climb(canvas: &mut PilotCanvas, look: &PilotLook, phase: usize) {
    let suit = look.suit;
    let gear = look.gear;
    let shift = if phase == 0 { 1 } else { -1 };
    // Legs.
    canvas.rect(-3, -7 - shift.max(0) * 2, 2, 5, suit.shade);
    canvas.rect(1, -7 + shift.min(0) * 2, 2, 5, suit.shade);
    canvas.rect(-3, -2 - shift.max(0) * 2, 2, 2, gear.deep);
    canvas.rect(1, -2 + shift.min(0) * 2, 2, 2, gear.deep);
    // Torso and pack from behind.
    canvas.rect(-3, -15, 6, 7, suit.body);
    canvas.rect(-3, -8, 6, 1, look.trim);
    if look.jetpack {
        canvas.rect(-2, -15, 4, 7, STEEL);
        canvas.rect(-2, -15, 1, 7, STEEL_LIGHT);
        canvas.rect(-2, -8, 1, 2, STEEL_DARK);
        canvas.rect(1, -8, 1, 2, STEEL_DARK);
    } else {
        canvas.rect(-2, -15, 4, 6, gear.body);
        canvas.rect(-2, -15, 4, 1, gear.light);
        canvas.set(0, -13, look.trim);
    }
    // Arms up the rungs, alternating.
    let left_top = if phase == 0 { -21 } else { -18 };
    let right_top = if phase == 0 { -18 } else { -21 };
    canvas.rect(-5, left_top, 2, -14 - left_top, suit.body);
    canvas.rect(3, right_top, 2, -14 - right_top, suit.body);
    canvas.rect(-5, left_top - 1, 2, 1, gear.body);
    canvas.rect(3, right_top - 1, 2, 1, gear.body);
    // Helmet from behind (no visor; the lamp strap).
    canvas.rect(-2, -23, 4, 1, BONE);
    canvas.rect(-3, -22, 6, 6, MIST);
    canvas.rect(-3, -22, 1, 6, FOG);
    canvas.rect(-3, -19, 6, 1, look.trim);
    canvas.rect(-2, -16, 4, 1, STONE_DARK);
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::PlayerId;

    use super::*;

    fn look() -> PilotLook {
        PilotLook::new(
            RigCosmetics::default_for(PlayerId::new(1)),
            PilotGear {
                jetpack: 1,
                suit: 3,
            },
        )
    }

    #[test]
    fn every_pose_paints_a_figure_standing_on_its_feet() {
        for pose in [
            PilotPose::Idle,
            PilotPose::Walk,
            PilotPose::Run,
            PilotPose::Jump,
            PilotPose::Fall,
            PilotPose::Climb,
            PilotPose::Crouch,
            PilotPose::Crawl,
            PilotPose::Swim,
            PilotPose::Hurt,
            PilotPose::Jetpack,
            PilotPose::Hang,
        ] {
            for seconds in [0.0, 0.13, 0.26, 0.4] {
                let canvas = paint_pilot(
                    &look(),
                    PilotFrame {
                        pose,
                        seconds,
                        swinging: false,
                    },
                );
                let (min_x, min_y, max_x, max_y) = canvas.bounds().expect("painted");
                assert!(max_y <= -1, "{pose:?}: nothing below the feet");
                assert!(min_x >= X0 && max_x < X0 + WIDTH, "{pose:?}");
                let height = max_y - min_y + 1;
                if matches!(pose, PilotPose::Crouch | PilotPose::Crawl) {
                    assert!(height <= 16, "{pose:?} fits a crawlway: {height}");
                } else {
                    assert!((22..=30).contains(&height), "{pose:?}: {height}");
                }
            }
        }
    }

    #[test]
    fn walking_animates() {
        let frame = |seconds| {
            paint_pilot(
                &look(),
                PilotFrame {
                    pose: PilotPose::Walk,
                    seconds,
                    swinging: false,
                },
            )
            .cells
        };
        assert_ne!(frame(0.0), frame(0.13));
    }
}
