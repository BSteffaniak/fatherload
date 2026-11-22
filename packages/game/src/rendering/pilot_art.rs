//! The pilot's sprite sheet (Part II F5), painted procedurally like the rig: a helmeted
//! prospector in a suit wearing the player's paint job (suit = primary colour, pack, boots and
//! gloves = secondary, belt and pads = trim), with a frame set for every pose: idle (breathing),
//! walk and run cycles, jump, fall, climb (seen from behind), crouch, crawl, swim, hurt,
//! jetpack and ledge hang.
//!
//! Wave 6 art pass: six-frame walk and run cycles with knee lift and arm swing, a knocked-back
//! hurt flinch, low-gravity variants (a floating star-jump in the air and a bounding lope on
//! the Moon) and a downed pilot lying flat on its back.
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
    RigCosmetics, WeaponKind,
    pilot::{PilotGear, PilotPose},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::rig_art::{Ramp, paint_accent, paint_ramp};

/// Canvas bounds in art pixels around the feet (wide enough for a rifle held out and a
/// glider overhead).
const X0: i32 = -17;
const Y0: i32 = -34;
const WIDTH: i32 = 34;
const HEIGHT: i32 = 35;

/// A pilot's colours and gear (from its owner's paint job and the pilot's gear).
#[derive(Clone, Copy, Debug)]
pub struct PilotLook {
    pub suit: Ramp,
    pub gear: Ramp,
    pub trim: Color,
    pub jetpack: bool,
    pub suit_tier: u8,
    /// Armour plates fitted (WP-PILOT): shoulder pads.
    pub armor: bool,
    /// The headlamp tier: a bigger lamp on the helmet.
    pub headlamp: u8,
    /// Climbing boots: cleats.
    pub boots: bool,
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
            armor: gear.armor > 0,
            headlamp: gear.headlamp,
            boots: gear.boots > 0,
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
    /// What the hands hold and where they point (WP-PILOT).
    pub arms: PilotArms,
    /// Under low gravity (the Moon): airborne poses float limbs-out and walking lopes along in
    /// bounds (Wave 6).
    pub low_gravity: bool,
    /// Down and waiting for a revive (a tomb run): lying flat on the floor (Wave 6).
    pub downed: bool,
}

impl PilotFrame {
    /// A plain frame of `pose`: empty hands, normal gravity, standing.
    /// (Only the desktop screenshot scenes and tests build frames this way, so the browser
    /// build does not use it.)
    #[must_use]
    #[cfg_attr(target_os = "emscripten", allow(dead_code))]
    pub const fn new(pose: PilotPose, seconds: f32) -> Self {
        Self {
            pose,
            seconds,
            swinging: false,
            arms: PilotArms::NONE,
            low_gravity: false,
            downed: false,
        }
    }
}

/// The weapon in a pilot's hands for one frame (WP-PILOT).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PilotArms {
    /// The held personal arm (`None`: the hand pick, drawn only while it swings).
    pub weapon: Option<WeaponKind>,
    /// The aim in the pilot's facing space (x forward, y down), a unit vector.
    pub aim: (f32, f32),
    /// Share of a melee swing or a shot's recoil left (0..1).
    pub swing: f32,
}

impl PilotArms {
    /// The hand pick, nothing raised.
    pub const NONE: Self = Self {
        weapon: None,
        aim: (1.0, 0.0),
        swing: 0.0,
    };
}

/// A small art-pixel grid around the pilot's feet.
#[derive(Clone, Debug)]
pub struct PilotCanvas {
    cells: Vec<Option<Color>>,
    /// Where the front glove ended up (rope and weapon effects attach there).
    pub hand: (i32, i32),
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
            hand: (2, -10),
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
    /// Legs folded and the body squashed down (crouch, crawl).
    folded: bool,
    /// Legs tucked up under a full-height body (a jump).
    tucked: bool,
    /// Extra drop of the upper body while folded.
    fold_drop: i32,
}

/// Paints one frame of a pilot.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one pose table")]
pub fn paint_pilot(look: &PilotLook, frame: PilotFrame) -> PilotCanvas {
    let mut canvas = PilotCanvas::new();
    let t = frame.seconds;
    let step = |rate: f32, frames: usize| ((t.max(0.0) * rate) as usize) % frames;
    if frame.downed {
        paint_downed(&mut canvas, look);
        canvas.outline();
        return canvas;
    }
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
        PilotPose::Walk if frame.low_gravity => {
            // The Moon's bounding lope: push off, sail with both boots up, land.
            let phase = step(6.0, 4);
            let front = [(2, 0), (1, 2), (-1, 2), (-2, 0)][phase];
            rig.front_leg = front;
            rig.back_leg = (-front.0, front.1.min(1));
            rig.front_arm = (-front.0, -2 * i32::from(front.1 > 0));
            rig.back_arm = (front.0, -2 * i32::from(front.1 > 0));
            rig.upper_dy = -i32::from(front.1 > 0);
        }
        PilotPose::Walk => {
            // Six frames: contact, passing (a knee lifts), contact on the other foot...
            let phase = step(10.0, 6);
            let cycle = [(2, 0), (1, 0), (0, 0), (-1, 0), (-1, 1), (1, 1)];
            let front = cycle[phase];
            let back = cycle[(phase + 3) % 6];
            rig.front_leg = front;
            rig.back_leg = back;
            // Arms swing against the legs.
            rig.front_arm = (-front.0.clamp(-1, 1), 0);
            rig.back_arm = (-back.0.clamp(-1, 1), 0);
            rig.upper_dy = i32::from(phase == 0 || phase == 3);
        }
        PilotPose::Run => {
            // Six frames, leaning in: long strides, the trailing boot kicked high, arms pumping.
            let phase = step(14.0, 6);
            let cycle = [(3, 0), (1, 0), (-1, 0), (-3, 1), (-1, 3), (2, 2)];
            let front = cycle[phase];
            let back = cycle[(phase + 3) % 6];
            rig.front_leg = front;
            rig.back_leg = back;
            rig.upper_dx = 1;
            rig.upper_dy = i32::from(phase == 0 || phase == 3);
            let pump = |leg: (i32, i32)| if leg.0 > 0 { (-1, -1) } else { (2, -3) };
            rig.front_arm = pump(front);
            rig.back_arm = pump(back);
        }
        PilotPose::Jump | PilotPose::Fall if frame.low_gravity => {
            // Floating under a sixth of a g: a slow star-jump, limbs out, drifting.
            let drift = step(2.0, 2) as i32;
            rig.front_leg = (2, 1 + drift);
            rig.back_leg = (-2, 2 - drift);
            rig.front_arm = (3, -7 + drift);
            rig.back_arm = (-3, -6 - drift);
            rig.upper_dy = -1;
        }
        PilotPose::Jump => {
            rig.tucked = true;
            rig.front_leg = (1, 1);
            rig.back_leg = (-1, 0);
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
            rig.fold_drop = 3;
        }
        PilotPose::Crawl => {
            rig.folded = true;
            rig.fold_drop = 3;
            rig.upper_dx = 3;
            let phase = step(6.0, 2) as i32;
            rig.front_arm = (2 + phase, 0);
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
            // Knocked back: thrown off balance, arms flung up, the front boot kicked off the
            // ground, shuddering.
            rig.upper_dx = -1 - step(16.0, 2) as i32;
            rig.front_arm = (3, -6);
            rig.back_arm = (-3, -5);
            rig.front_leg = (2, 2);
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
        PilotPose::Hover => {
            // Legs together, arms out to the sides for balance.
            rig.front_leg = (0, 0);
            rig.back_leg = (0, 1);
            rig.front_arm = (3, -4);
            rig.back_arm = (-2, -4);
        }
        PilotPose::Glide => {
            // Both hands up on the glider's bar, body trailing behind.
            rig.front_arm = (0, -13);
            rig.back_arm = (2, -13);
            rig.front_leg = (-1, 1);
            rig.back_leg = (-3, 2);
            rig.upper_dx = 1;
        }
        PilotPose::Rope => {
            // One hand up the rope, the other on it lower down, boots dangling.
            rig.front_arm = (-1, -14);
            rig.back_arm = (0, -9);
            rig.front_leg = (0, 1);
            rig.back_leg = (-1, 0);
        }
        PilotPose::WallSlide => {
            // A palm flat on the wall ahead, knees bent, boots braking.
            rig.front_arm = (3, -9);
            rig.back_arm = (2, -3);
            rig.front_leg = (1, 2);
            rig.back_leg = (0, 0);
            rig.upper_dx = 1;
        }
        PilotPose::Climb => {}
    }
    let holds = frame.arms.weapon.is_some()
        && !matches!(
            frame.pose,
            PilotPose::Glide | PilotPose::Rope | PilotPose::Hang | PilotPose::Swim
        );
    if holds {
        rig.front_arm = held_arm(frame.arms);
    }
    if frame.swinging && !holds {
        // The pick goes up and comes down in a steady rhythm.
        rig.front_arm = if step(6.6, 2) == 0 { (1, -6) } else { (3, -1) };
    }
    paint_body(&mut canvas, look, &rig, frame);
    if holds && let Some(weapon) = frame.arms.weapon {
        paint_held(&mut canvas, weapon, frame.arms);
    }
    if frame.pose == PilotPose::Glide {
        paint_glider(&mut canvas, look);
    }
    canvas.outline();
    canvas
}

/// The glove offset (from the hanging position) for a weapon held along `arms.aim`, or for a
/// melee arm partway through its swing.
fn held_arm(arms: PilotArms) -> (i32, i32) {
    let (x, y) = held_direction(arms);
    // The hanging glove sits 1 forward and 5 down from the shoulder; reach 5 along the aim.
    ((x * 5.0).round() as i32 - 1, (y * 5.0).round() as i32 - 5)
}

/// The direction the held thing points: the aim for a gun; for a melee arm, an arc from
/// raised behind the head to struck forward as the swing plays out.
fn held_direction(arms: PilotArms) -> (f32, f32) {
    if let Some(WeaponKind::Wrench | WeaponKind::HandPick) = arms.weapon {
        if arms.swing > 0.0 {
            // Swing time runs out from 1 to 0: raised (-110 degrees) to struck (+35).
            let angle = 145.0f32.mul_add(-arms.swing, 35.0).to_radians();
            (angle.cos(), angle.sin())
        } else {
            // Carried low, head forward.
            (0.55, 0.83)
        }
    } else {
        {
            let (x, y) = arms.aim;
            let length = x.hypot(y);
            if length < 0.01 {
                (1.0, 0.0)
            } else {
                // Recoil kicks the muzzle up a touch.
                let kick = arms.swing * 0.25;
                let (x, y) = (x / length, y / length - kick);
                let length = x.hypot(y);
                (x / length, y / length)
            }
        }
    }
}

/// Paints the held weapon's sprite into the canvas from the glove, rotated along its
/// direction (sampled back through the rotation, so no pixel gaps at diagonals).
fn paint_held(canvas: &mut PilotCanvas, weapon: WeaponKind, arms: PilotArms) {
    let Some(sprite) = super::pixel_art::sprites::weapons::in_hand(weapon) else {
        return;
    };
    let rows = sprite.frames[0];
    let height = rows.len() as i32;
    let width = rows.iter().map(|row| row.len()).max().unwrap_or(0) as i32;
    let (dx, dy) = held_direction(arms);
    // The grip pivot: second column, the row under the barrel.
    let pivot = (1.0_f32, 1.5_f32);
    let (hx, hy) = canvas.hand;
    let reach = width.max(height) + 2;
    for oy in -reach..=reach {
        for ox in -reach..=reach {
            // Inverse-rotate the canvas offset into sprite space.
            let (fx, fy) = (ox as f32, oy as f32);
            let sx = fx.mul_add(dx, fy * dy) + pivot.0;
            let sy = fy.mul_add(dx, -fx * dy) + pivot.1;
            let (column, row) = (sx.floor() as i32, sy.floor() as i32);
            if column < 0 || row < 0 || column >= width || row >= height {
                continue;
            }
            let Some(cell) = rows[row as usize].chars().nth(column as usize) else {
                continue;
            };
            if cell == '.' || cell == ' ' {
                continue;
            }
            if let Some((_, color)) = sprite.legend.iter().find(|(key, _)| *key == cell) {
                canvas.set(hx + ox, hy + oy, *color);
            }
        }
    }
    // The glove stays on top of the grip.
    let _ = (hx, hy);
}

/// The glider: a bright canvas wing over the pilot's raised hands, ribs in the suit's trim.
fn paint_glider(canvas: &mut PilotCanvas, look: &PilotLook) {
    let (hx, hy) = canvas.hand;
    let top = hy - 4;
    for x in -13..=13 {
        // A shallow arch: highest in the middle.
        let sag = (x * x) / 40;
        let y = top + sag;
        canvas.set(hx + x, y, if x % 4 == 0 { look.trim } else { ROSE });
        canvas.set(hx + x, y + 1, if x.abs() > 10 { MAROON } else { PINK });
        if x.abs() <= 9 {
            canvas.set(hx + x, y - 1, PINK);
        }
    }
    // The control bar down to the hands.
    for y in top + 2..hy {
        canvas.set(hx, y, STEEL_LIGHT);
    }
}

/// The side view: back arm, legs, pack, torso, helmet, front arm (and pick).
#[allow(clippy::too_many_lines, reason = "the body's parts in draw order")]
fn paint_body(canvas: &mut PilotCanvas, look: &PilotLook, rig: &Rig, frame: PilotFrame) {
    let suit = look.suit;
    let gear = look.gear;
    // Legs and boots.
    let (leg_top, leg_rows) = if rig.folded || rig.tucked {
        (-4, 3)
    } else {
        (-7, 5)
    };
    let leg = |canvas: &mut PilotCanvas, x: i32, (dx, lift): (i32, i32), front: bool| {
        let color = if front { suit.body } else { suit.shade };
        let boot = if front { gear.shade } else { gear.deep };
        // A tucked leg is a folded one drawn up under the belt.
        let lift = lift + if rig.tucked { 3 } else { 0 };
        if rig.folded || rig.tucked {
            // Thigh forward, shin back down: a bent knee.
            canvas.rect(x + dx, leg_top - lift, 3, 2, color);
            canvas.rect(x + dx + 1, leg_top + 2 - lift, 2, 1, color);
            canvas.rect(x + dx, -1 - lift, 3, 1, boot);
        } else {
            canvas.rect(x + dx, leg_top - lift, 2, leg_rows, color);
            canvas.rect(x + dx, -2 - lift, 3, 2, boot);
            if look.boots {
                canvas.set(x + dx + 2, -1 - lift, STEEL_LIGHT);
            }
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
    // Folded (crouch, crawl, tucked jump) the torso squashes and the helmet tucks down, so a
    // crouched pilot fits a 1-tile crawlway.
    let squash = if rig.folded { 4 } else { 0 };
    let top = -15 + up_y + squash;
    let rows = 7 - squash;
    // Back arm (behind the pack and torso).
    let shoulder = (-1 + up_x, top + 1);
    paint_arm(canvas, shoulder, rig.back_arm, suit.deep, gear.deep);
    // Pack or jetpack.
    let pack_x = -6 + up_x;
    if look.jetpack {
        canvas.rect(pack_x, top, 3, rows, STEEL);
        canvas.rect(pack_x, top, 1, rows, STEEL_LIGHT);
        canvas.rect(pack_x + 2, top, 1, rows, STEEL_DARK);
        canvas.rect(pack_x, top - 1, 3, 1, look.trim);
        canvas.rect(pack_x, belt, 1, 2, STEEL_DARK);
        canvas.rect(pack_x + 2, belt, 1, 2, STEEL_DARK);
    } else {
        canvas.rect(pack_x, top, 3, rows - 1, gear.body);
        canvas.rect(pack_x, top, 3, 1, gear.light);
        canvas.rect(pack_x, top + rows - 2, 3, 1, gear.shade);
        canvas.set(pack_x + 1, top + 2, look.trim);
    }
    // Belt and torso.
    canvas.rect(-3 + up_x, belt, 5, 1, look.trim);
    canvas.set(up_x, belt, GOLD);
    canvas.rect(-3 + up_x, top, 5, rows, suit.body);
    canvas.rect(-3 + up_x, top, 1, rows, suit.shade);
    canvas.rect(1 + up_x, top + 1, 1, rows - 2, suit.light);
    canvas.rect(-2 + up_x, top, 4, 1, suit.light);
    // A pack strap across the chest and a chest badge.
    canvas.set(-2 + up_x, top + 1, gear.shade);
    canvas.set(-1 + up_x, top + 2, gear.shade);
    canvas.set(up_x, top + (rows - 1).min(3), look.trim);
    if look.suit_tier >= 2 {
        canvas.rect(-3 + up_x, top, 2, 1, look.trim);
        canvas.set(1 + up_x, top, look.trim);
    }
    if look.suit_tier >= 3 && rows >= 6 {
        canvas.set(-1 + up_x, top + 4, MINT);
    }
    // Helmet with its visor and lamp.
    let hx = up_x;
    let hy = up_y + squash + i32::from(rig.folded);
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
    if look.suit_tier >= 3 && !rig.folded {
        // The top suit's antenna (folded down out of the way in a crouch).
        canvas.rect(-2 + hx, -25 + hy, 1, 2, STEEL);
        canvas.set(-2 + hx, -26 + hy, look.trim);
    }
    // Front arm, then the pick in its glove.
    let front_shoulder = (1 + up_x, top + 1);
    let glove = paint_arm(canvas, front_shoulder, rig.front_arm, suit.body, gear.body);
    canvas.hand = glove;
    if frame.swinging && frame.arms.weapon.is_none() {
        paint_pick(canvas, glove, rig.front_arm.1 < -3);
    }
    if look.armor && !rig.folded {
        // Shoulder pads over both shoulders.
        canvas.rect(-3 + up_x, top - 1, 2, 2, STEEL);
        canvas.set(-3 + up_x, top - 1, STEEL_LIGHT);
        canvas.rect(up_x, top - 1, 2, 2, STEEL);
        canvas.set(1 + up_x, top - 1, STEEL_LIGHT);
    }
    if look.headlamp > 0 {
        // A bigger lamp on the brow: brighter with the tier.
        let hx = up_x;
        let hy = up_y + squash + i32::from(rig.folded);
        canvas.rect(2 + hx, -23 + hy, 2, 2, STEEL);
        canvas.set(
            3 + hx,
            -22 + hy,
            if look.headlamp >= 2 { CREAM } else { GOLD },
        );
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

/// Down (a tomb run, waiting for a revive): flat on its back on the floor, head forward, the
/// pack under it, boots toes-up, one arm across the chest and the other flopped on the ground.
fn paint_downed(canvas: &mut PilotCanvas, look: &PilotLook) {
    let suit = look.suit;
    let gear = look.gear;
    // The pack, squashed flat under the back.
    canvas.rect(
        -6,
        -2,
        9,
        2,
        if look.jetpack { STEEL_DARK } else { gear.shade },
    );
    // Legs flat, boots toes-up at the far end.
    canvas.rect(-12, -5, 7, 2, suit.shade);
    canvas.rect(-12, -4, 7, 1, suit.body);
    canvas.rect(-14, -7, 2, 4, gear.deep);
    canvas.rect(-14, -7, 2, 1, gear.shade);
    if look.boots {
        canvas.set(-14, -4, STEEL_LIGHT);
    }
    // The torso, belt and chest badge.
    canvas.rect(-5, -7, 8, 4, suit.body);
    canvas.rect(-5, -7, 8, 1, suit.light);
    canvas.rect(-5, -3, 8, 1, suit.shade);
    canvas.rect(-5, -7, 1, 4, look.trim);
    canvas.set(0, -6, look.trim);
    // The helmet on its back, visor up.
    canvas.rect(3, -8, 6, 6, MIST);
    canvas.rect(3, -3, 6, 1, FOG);
    canvas.rect(4, -8, 4, 1, BONE);
    canvas.rect(5, -7, 3, 2, OCEAN);
    canvas.set(6, -7, ICE);
    canvas.set(7, -5, DEEP_BLUE);
    // One arm across the chest, the other flopped out on the floor past the helmet.
    canvas.rect(-3, -8, 4, 1, suit.body);
    canvas.set(1, -8, gear.body);
    canvas.rect(9, -2, 4, 1, suit.shade);
    canvas.set(13, -2, gear.deep);
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
                ..PilotGear::default()
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
            PilotPose::Hover,
            PilotPose::Rope,
            PilotPose::WallSlide,
        ] {
            for seconds in [0.0, 0.13, 0.26, 0.4] {
                let canvas = paint_pilot(&look(), PilotFrame::new(pose, seconds));
                let (min_x, min_y, max_x, max_y) = canvas.bounds().expect("painted");
                assert!(max_y <= -1, "{pose:?}: nothing below the feet");
                assert!(min_x >= X0 && max_x < X0 + WIDTH, "{pose:?}");
                let height = max_y - min_y + 1;
                if matches!(pose, PilotPose::Crouch | PilotPose::Crawl) {
                    assert!(height <= 16, "{pose:?} fits a crawlway: {height}");
                } else if pose == PilotPose::Jump {
                    assert!((18..=30).contains(&height), "{pose:?}: {height}");
                } else {
                    assert!((22..=30).contains(&height), "{pose:?}: {height}");
                }
            }
        }
    }

    #[test]
    fn walking_animates() {
        let frame = |seconds| paint_pilot(&look(), PilotFrame::new(PilotPose::Walk, seconds)).cells;
        assert_ne!(frame(0.0), frame(0.13));
    }

    #[test]
    fn walk_and_run_cycles_have_six_distinct_frames() {
        for (pose, rate) in [(PilotPose::Walk, 10.0), (PilotPose::Run, 14.0)] {
            let frames: Vec<_> = (0..6)
                .map(|index| {
                    paint_pilot(&look(), PilotFrame::new(pose, (index as f32 + 0.5) / rate)).cells
                })
                .collect();
            for (index, a) in frames.iter().enumerate() {
                for b in &frames[index + 1..] {
                    assert_ne!(a, b, "{pose:?}");
                }
            }
        }
    }

    #[test]
    fn low_gravity_floats_and_lopes_on_its_feet() {
        for pose in [PilotPose::Jump, PilotPose::Fall, PilotPose::Walk] {
            for seconds in [0.0, 0.2, 0.4, 0.6] {
                let frame = PilotFrame {
                    low_gravity: true,
                    ..PilotFrame::new(pose, seconds)
                };
                let canvas = paint_pilot(&look(), frame);
                let (_, min_y, _, max_y) = canvas.bounds().expect("painted");
                assert!(max_y <= -1, "{pose:?}: nothing below the feet");
                assert!((20..=30).contains(&(max_y - min_y + 1)), "{pose:?}");
            }
        }
        let floating = paint_pilot(
            &look(),
            PilotFrame {
                low_gravity: true,
                ..PilotFrame::new(PilotPose::Fall, 0.0)
            },
        );
        assert_ne!(
            floating.cells,
            paint_pilot(&look(), PilotFrame::new(PilotPose::Fall, 0.0)).cells
        );
    }

    #[test]
    fn a_downed_pilot_lies_flat() {
        let canvas = paint_pilot(
            &look(),
            PilotFrame {
                downed: true,
                ..PilotFrame::new(PilotPose::Idle, 0.0)
            },
        );
        let (min_x, min_y, max_x, max_y) = canvas.bounds().expect("painted");
        assert!(max_y <= -1);
        assert!(max_y - min_y < 10, "lying, not standing");
        assert!(max_x - min_x + 1 >= 20, "stretched out");
    }
}
