//! The friendly aliens' pixel art (Wave 12 aliens track): the three species' figures and
//! portraits, their towns' buildings in their own styles, and their writing.
//!
//! * **Vessi** (ice worlds): tall as two rigs on four thin stilt legs, a bell of white
//!   frost-fur, a long neck and a throat lantern that glows when they sing (brighter while
//!   they talk). Their towns are spiral snow-vaults of packed ice lit warm from inside, round a
//!   song spire of frozen glass. Their script is rings with ticks (sung notation).
//! * **Tokkari** (rocky worlds): low and broad, four short scuttling legs and two big digging
//!   claws under a domed carapace studded with grown-in gems; eyes on stalks that bob. Their
//!   towns are terraced mound-halls of fused stone half sunk into the ground, with round doors
//!   and forge chimneys glowing orange. Their script is notched runes.
//! * **Siphra** (airless moons): drifting bells of living glass trailing tendrils of light,
//!   hovering above the ground. Their towns are lantern-pods of grown glass hung from crystal
//!   pylons. Their script is constellations: dots joined by lines.
//!
//! Everything is a pure function of the character (or building) and the time, drawn with its
//! feet at the pen's origin in art pixels (sixteen to a tile), so the same art serves the polar
//! ground, the talk panel's portrait and the scenes.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::match_same_arms,
    clippy::similar_names,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use fatherload_simulation::{
    alien::{AlienBuildingKind, AlienSpecies},
    people::{Character, Role},
};
use raylib::prelude::*;

use super::people_art::{PersonPen, Pose};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;

/// `colour` scaled by `light` (0..1), alpha kept.
#[must_use]
pub fn lit(colour: Color, light: f32) -> Color {
    let light = light.clamp(0.0, 1.0);
    Color::new(
        (f32::from(colour.r) * light) as u8,
        (f32::from(colour.g) * light) as u8,
        (f32::from(colour.b) * light) as u8,
        colour.a,
    )
}

/// `colour` with alpha `alpha`.
const fn alpha(colour: Color, alpha: u8) -> Color {
    Color::new(colour.r, colour.g, colour.b, alpha)
}

/// A pen that dims what it draws by a light level (night on a body's far side).
#[derive(Clone, Copy, Debug)]
pub struct ArtPen {
    pub pen: PersonPen,
    pub light: f32,
    /// Nothing is drawn outside this rectangle (draw units), if set (a portrait's frame).
    pub clip: Option<Rectangle>,
}

impl ArtPen {
    #[must_use]
    pub const fn new(x: f32, y: f32, pixel: f32, light: f32) -> Self {
        Self {
            pen: PersonPen { x, y, pixel },
            light,
            clip: None,
        }
    }

    /// The rectangle of art pixels `(x, y, w, h)` in draw units, cut to the clip.
    fn place(self, x: i32, y: i32, w: i32, h: i32) -> Option<Rectangle> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let pen = self.pen;
        let rect = Rectangle::new(
            (x as f32).mul_add(pen.pixel, pen.x),
            (y as f32).mul_add(pen.pixel, pen.y),
            w as f32 * pen.pixel,
            h as f32 * pen.pixel,
        );
        let Some(clip) = self.clip else {
            return Some(rect);
        };
        let left = rect.x.max(clip.x);
        let top = rect.y.max(clip.y);
        let right = (rect.x + rect.width).min(clip.x + clip.width);
        let bottom = (rect.y + rect.height).min(clip.y + clip.height);
        (right > left && bottom > top)
            .then(|| Rectangle::new(left, top, right - left, bottom - top))
    }

    fn rect(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, colour: Color) {
        if let Some(rect) = self.place(x, y, w, h) {
            draw.draw_rectangle_rec(rect, lit(colour, self.light));
        }
    }

    /// A glow (not dimmed by the dark: it is its own light).
    fn glow(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, colour: Color) {
        if let Some(rect) = self.place(x, y, w, h) {
            draw.draw_rectangle_rec(rect, colour);
        }
    }

    fn px(self, draw: &mut impl RaylibDraw, x: i32, y: i32, colour: Color) {
        self.rect(draw, x, y, 1, 1, colour);
    }

    /// A filled ellipse of half-sizes `(rx, ry)` centred on `(cx, cy)`, row by row.
    fn ellipse(
        self,
        draw: &mut impl RaylibDraw,
        cx: i32,
        cy: i32,
        rx: i32,
        ry: i32,
        colour: Color,
    ) {
        for dy in -ry..=ry {
            let t = 1.0 - (dy as f32 / ry.max(1) as f32).powi(2);
            let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
            self.rect(draw, cx - half, cy + dy, 2 * half + 1, 1, colour);
        }
    }

    /// A filled ellipse that is its own light (not dimmed by the dark).
    fn glow_ellipse(
        self,
        draw: &mut impl RaylibDraw,
        cx: i32,
        cy: i32,
        rx: i32,
        ry: i32,
        colour: Color,
    ) {
        for dy in -ry..=ry {
            let t = 1.0 - (dy as f32 / ry.max(1) as f32).powi(2);
            let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
            self.glow(draw, cx - half, cy + dy, 2 * half + 1, 1, colour);
        }
    }

    /// The top half of an ellipse (a dome) standing on row `base`.
    fn dome(self, draw: &mut impl RaylibDraw, cx: i32, base: i32, rx: i32, ry: i32, colour: Color) {
        for dy in 0..=ry {
            let t = 1.0 - (dy as f32 / ry.max(1) as f32).powi(2);
            let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
            self.rect(draw, cx - half, base - dy, 2 * half + 1, 1, colour);
        }
    }
}

const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// --- Palettes --------------------------------------------------------------------------------

/// Vessi fur tints: white, pale blue, pale lilac.
const VESSI_FUR: [(Color, Color); 3] = [
    (
        Color::new(0xf2, 0xf4, 0xf6, 255),
        Color::new(0xc8, 0xd2, 0xdc, 255),
    ),
    (
        Color::new(0xdc, 0xec, 0xf8, 255),
        Color::new(0xa8, 0xc4, 0xdc, 255),
    ),
    (
        Color::new(0xec, 0xe2, 0xf4, 255),
        Color::new(0xc4, 0xb4, 0xd4, 255),
    ),
];
/// Vessi lantern colours by seed: amber, rose, gold.
const VESSI_LANTERN: [Color; 3] = [AMBER, ROSE, GOLD];
/// Tokkari carapace (shell, shade, rim).
const TOKKARI_SHELL: [(Color, Color, Color); 3] = [
    (
        Color::new(0x8a, 0x6a, 0x52, 255),
        Color::new(0x5e, 0x46, 0x36, 255),
        CLAY,
    ),
    (
        Color::new(0x6e, 0x76, 0x7e, 255),
        Color::new(0x4a, 0x50, 0x58, 255),
        STONE_LIGHT,
    ),
    (
        Color::new(0x9a, 0x5a, 0x3a, 255),
        Color::new(0x6a, 0x3a, 0x26, 255),
        COPPER,
    ),
];
/// Gems grown into a Tokkari's shell.
const TOKKARI_GEMS: [Color; 4] = [RED, MINT, AMBER, VIOLET];
/// Siphra glass (bell, inner glow).
const SIPHRA_GLASS: [(Color, Color); 3] = [(TEAL, MINT), (LAVENDER, PINK), (SKY, SKY_LIGHT)];

// --- Figures ---------------------------------------------------------------------------------

/// Art pixels tall a figure of `species` stands (name tags and contact boxes).
#[must_use]
pub const fn figure_height(species: AlienSpecies) -> i32 {
    match species {
        AlienSpecies::Vessi => 40,
        AlienSpecies::Tokkari => 20,
        AlienSpecies::Siphra => 34,
    }
}

/// Draws `character` (of `species`) with its feet at the pen, posed and lit.
pub fn draw_alien(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    character: &Character,
    species: AlienSpecies,
    pose: Pose,
    seconds: f32,
) {
    let seed = character.appearance.seed ^ u64::from(character.id.index);
    match species {
        AlienSpecies::Vessi => draw_vessi(
            draw,
            pen,
            seed,
            character.role,
            character.age,
            pose,
            seconds,
        ),
        AlienSpecies::Tokkari => draw_tokkari(draw, pen, seed, character.role, pose, seconds),
        AlienSpecies::Siphra => draw_siphra(draw, pen, seed, character.role, pose, seconds),
    }
}

fn draw_vessi(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    seed: u64,
    role: Role,
    age: u8,
    pose: Pose,
    seconds: f32,
) {
    let (fur, shade) = VESSI_FUR[(seed % 3) as usize];
    let lantern = VESSI_LANTERN[((seed >> 8) % 3) as usize];
    let facing = if pose.facing < 0 { -1 } else { 1 };
    let step = pose
        .stride
        .map_or(0, |stride| ((stride / 10.0) as i32).rem_euclid(4));
    // Shadow.
    pen.rect(draw, -9, 0, 18, 1, alpha(INK, 90));
    // Four stilt legs: back pair darker, front pair lighter, striding in turn.
    let lift = |phase: i32| -> i32 { i32::from((step + phase) % 4 == 1) };
    for (offset, phase, colour) in [(-6, 0, shade), (4, 2, shade), (-3, 1, fur), (7, 3, fur)] {
        let x = offset * facing;
        let up = lift(phase);
        pen.rect(draw, x, -20, 1, 20 - up, colour);
        pen.rect(draw, x - 1, -up - 1, 3, 1, STONE_LIGHT);
        // The knee.
        pen.px(draw, x + facing, -11, shade);
    }
    // The bell body under its fur.
    let bob = ((seconds * 1.6 + (seed % 7) as f32).sin() * 0.8).round() as i32;
    pen.dome(draw, 0, -18 + bob, 11, 9, fur);
    pen.rect(draw, -11, -18 + bob, 23, 2, shade);
    // Fur tufts along the hem.
    for tuft in (-10..=10).step_by(3) {
        pen.px(draw, tuft, -16 + bob, fur);
    }
    for (x, y) in [(-6, -24), (3, -25), (-2, -21), (7, -21)] {
        pen.px(draw, x, y + bob, shade);
    }
    // Little arms under the body.
    pen.rect(draw, 5 * facing, -17 + bob, 1, 3, shade);
    pen.rect(draw, 7 * facing, -17 + bob, 1, 2, shade);
    // The neck curves up and forward.
    let neck_x = 7 * facing;
    for (i, (dx, dy)) in [(0, -27), (1, -29), (2, -31), (3, -33), (4, -34)]
        .iter()
        .enumerate()
    {
        let x = neck_x + dx * facing;
        pen.rect(
            draw,
            x.min(x + facing),
            dy + bob,
            3,
            3,
            if i % 2 == 0 { fur } else { shade },
        );
    }
    // The head.
    let head_x = neck_x + 5 * facing;
    pen.ellipse(draw, head_x, -37 + bob, 3, 2, fur);
    pen.px(draw, head_x + facing, -38 + bob, INK);
    pen.px(draw, head_x + 3 * facing, -37 + bob, shade);
    // Two short antler-frost spikes.
    pen.px(draw, head_x - facing, -40 + bob, ICE);
    pen.px(draw, head_x, -41 + bob, ICE);
    // The throat lantern: it pulses slowly, brighter while they talk.
    let pulse = if pose.talking {
        0.75 + 0.25 * (seconds * 9.0).sin()
    } else {
        0.55 + 0.15 * (seconds * 1.3 + (seed % 5) as f32).sin()
    };
    let glow = alpha(lantern, (pulse * 255.0) as u8);
    pen.glow(draw, neck_x + 2 * facing - 1, -30 + bob, 3, 3, glow);
    pen.glow(
        draw,
        neck_x + 2 * facing - 2,
        -31 + bob,
        5,
        5,
        alpha(lantern, (pulse * 70.0) as u8),
    );
    // A trade sign: a scholar's glyph-slate, an elder's frost crown, a smith's hammer.
    match role {
        Role::Scholar => pen.rect(draw, -3, -26 + bob, 4, 3, SLATE),
        Role::Storyteller => {
            pen.px(draw, head_x + facing, -41 + bob, GOLD);
            pen.px(draw, head_x - 2 * facing, -40 + bob, GOLD);
        }
        Role::Smith => pen.rect(draw, -8 * facing, -16 + bob, 2, 3, STEEL),
        Role::Trader => pen.rect(draw, -9, -22 + bob, 3, 4, ROSE),
        _ => {}
    }
    if age >= 70 {
        // An elder's long frost beard.
        pen.rect(draw, head_x - facing, -35 + bob, 2, 3, BONE);
    }
}

fn draw_tokkari(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    seed: u64,
    role: Role,
    pose: Pose,
    seconds: f32,
) {
    let (shell, shade, rim) = TOKKARI_SHELL[(seed % 3) as usize];
    let facing = if pose.facing < 0 { -1 } else { 1 };
    let step = pose
        .stride
        .map_or(0, |stride| ((stride / 6.0) as i32).rem_euclid(2));
    pen.rect(draw, -10, 0, 20, 1, alpha(INK, 90));
    // Four short legs scuttling.
    for (i, x) in [-7, -3, 2, 6].iter().enumerate() {
        let up = i32::from((i as i32 + step) % 2 == 0 && pose.stride.is_some());
        pen.rect(draw, *x, -4 - up, 2, 4 - up.min(1) + up, shade);
        pen.rect(draw, *x - 1, -1 - up, 3, 1, STONE_DARK);
    }
    // The domed carapace.
    pen.dome(draw, 0, -4, 10, 9, shell);
    pen.rect(draw, -10, -5, 21, 2, rim);
    // Plates.
    for x in [-6, -1, 4] {
        pen.rect(draw, x, -12, 1, 7, shade);
    }
    // Grown-in gems.
    for i in 0..3 {
        let roll = mix(seed ^ i);
        let gem = TOKKARI_GEMS[(roll % 4) as usize];
        let x = -6 + (roll >> 8) as i32 % 12;
        let y = -11 + (roll >> 16) as i32 % 4;
        pen.px(draw, x, y, gem);
        pen.px(draw, x, y - 1, alpha(CREAM, 180));
    }
    // The face under the shell's front edge, and two digging claws.
    let front = 10 * facing;
    pen.rect(
        draw,
        front - if facing > 0 { 2 } else { 0 },
        -7,
        3,
        3,
        shade,
    );
    let dig = if pose.talking {
        ((seconds * 8.0).sin() * 1.2).round() as i32
    } else {
        0
    };
    for (dy, size) in [(-4, 3), (-1, 2)] {
        let x = front + 2 * facing;
        pen.rect(
            draw,
            x.min(x + facing * (size - 1)),
            dy + dig,
            size,
            2,
            STEEL,
        );
        pen.px(draw, x + size * facing, dy + 1 + dig, STEEL_LIGHT);
    }
    // Eye stalks that bob.
    let bob = (seconds * 3.0 + (seed % 9) as f32).sin();
    for (i, dx) in [3, 6].iter().enumerate() {
        let sway = (bob * if i == 0 { 1.0 } else { -1.0 }).round() as i32;
        let x = dx * facing;
        pen.rect(draw, x, -17, 1, 5, shade);
        pen.rect(draw, x - 1 + sway, -19, 3, 2, CREAM);
        pen.px(draw, x + facing.max(0) + sway, -19, INK);
    }
    // A trade sign.
    match role {
        Role::Smith => pen.rect(draw, -9 * facing, -9, 2, 4, MAROON),
        Role::Trader => {
            pen.rect(draw, -2, -15, 5, 1, GOLD);
            pen.px(draw, -2, -14, GOLD);
            pen.px(draw, 2, -14, GOLD);
        }
        Role::Scholar => pen.rect(draw, -8, -10, 1, 6, CREAM),
        Role::RetiredMiner => pen.rect(draw, -4, -14, 7, 1, BONE),
        _ => {}
    }
}

fn draw_siphra(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    seed: u64,
    role: Role,
    pose: Pose,
    seconds: f32,
) {
    let (glass, inner) = SIPHRA_GLASS[(seed % 3) as usize];
    let phase = (seed % 11) as f32;
    let hover = 7 + ((seconds * 1.2 + phase).sin() * 2.0).round() as i32;
    // A soft shadow on the ground under the drifting bell.
    pen.rect(draw, -6, 0, 12, 1, alpha(INK, 60));
    let base = -hover;
    // Tendrils of light trailing down, waving.
    for i in 0..5 {
        let x0 = -6 + i * 3;
        for j in 0..(5 + i % 3) {
            let wave =
                ((seconds * 2.4 + phase + j as f32 * 0.7 + i as f32).sin() * 1.3).round() as i32;
            let fade = 200 - j * 26;
            pen.glow(
                draw,
                x0 + wave,
                base + j * 2,
                1,
                2,
                alpha(inner, fade.max(40) as u8),
            );
        }
    }
    // The bell: translucent glass with a bright rim and an inner glow.
    let breathe = if pose.talking {
        (seconds * 6.0).sin() * 0.5 + 0.5
    } else {
        (seconds * 1.1 + phase).sin() * 0.25 + 0.5
    };
    pen.glow_ellipse(draw, 0, base - 8, 12, 11, alpha(glass, 26));
    pen.dome(draw, 0, base, 9, 15, alpha(glass, 150));
    pen.dome(
        draw,
        0,
        base - 2,
        6,
        10,
        alpha(inner, (90.0 + breathe * 120.0) as u8),
    );
    pen.rect(draw, -9, base, 19, 1, alpha(CREAM, 200));
    // The rim lights blink in turn.
    for i in 0..5 {
        let on = ((seconds * 2.0 + i as f32) as i32 + i).rem_euclid(3) == 0;
        pen.glow(
            draw,
            -8 + i * 4,
            base + 1,
            1,
            1,
            if on { CREAM } else { alpha(inner, 160) },
        );
    }
    // Two eye-spots, turned the way it faces.
    let facing = if pose.facing < 0 { -1 } else { 1 };
    pen.glow(draw, 2 * facing - 1, base - 8, 1, 1, NIGHT);
    pen.glow(draw, 5 * facing - 1, base - 8, 1, 1, NIGHT);
    // A trade sign: a teacher's glyph motes, an elder's halo.
    match role {
        Role::Storyteller => {
            for i in 0..6 {
                let angle = seconds * 0.8 + i as f32;
                let x = (angle.cos() * 11.0).round() as i32;
                let y = base - 9 + (angle.sin() * 4.0).round() as i32;
                pen.glow(draw, x, y, 1, 1, GOLD);
            }
        }
        Role::Scholar => pen.glow(draw, -1, base - 18, 3, 1, alpha(GOLD, 220)),
        _ => {}
    }
}

/// Draws `character`'s portrait (a [`super::people_art::PORTRAIT_SIZE`] square with its
/// top-left at the pen): the species' head and shoulders up close.
pub fn draw_alien_portrait(
    draw: &mut impl RaylibDraw,
    pen: PersonPen,
    character: &Character,
    species: AlienSpecies,
    talking: bool,
    seconds: f32,
) {
    let size = super::people_art::PORTRAIT_SIZE;
    let seed = character.appearance.seed ^ u64::from(character.id.index);
    // The figure, scaled up so its head fills the frame, cut to the frame.
    let pose = Pose {
        facing: 1,
        stride: None,
        talking,
    };
    // Feet placed so the face fills the frame: the Vessi's head on its long neck, the
    // Tokkari whole (they are small), the Siphra's bell.
    let (scale, x, y) = match species {
        AlienSpecies::Vessi => (1.15, 2.0, 52.0),
        AlienSpecies::Tokkari => (1.3, 15.0, 30.0),
        AlienSpecies::Siphra => (1.0, 16.0, 36.0),
    };
    let mut art = ArtPen::new(
        pen.x + x * pen.pixel,
        pen.y + y * pen.pixel,
        pen.pixel * scale,
        1.0,
    );
    let backdrop = match species {
        AlienSpecies::Vessi => Color::new(0x1c, 0x2c, 0x44, 255),
        AlienSpecies::Tokkari => Color::new(0x2c, 0x1e, 0x18, 255),
        AlienSpecies::Siphra => Color::new(0x10, 0x14, 0x2a, 255),
    };
    let frame = Rectangle::new(
        pen.x,
        pen.y,
        size as f32 * pen.pixel,
        size as f32 * pen.pixel,
    );
    draw.draw_rectangle_rec(frame, backdrop);
    // Everything cut to the frame.
    art.clip = Some(frame);
    match species {
        AlienSpecies::Vessi => {
            // The standing figure's head is a few pixels on a long neck: up close it gets its
            // own head-and-shoulders painting at the frame's own scale.
            let mut close = ArtPen::new(pen.x, pen.y, pen.pixel, 1.0);
            close.clip = Some(frame);
            draw_vessi_portrait(draw, close, seed, character, talking, seconds);
        }
        AlienSpecies::Tokkari => draw_tokkari(draw, art, seed, character.role, pose, seconds),
        AlienSpecies::Siphra => draw_siphra(draw, art, seed, character.role, pose, seconds),
    }
}

/// A Vessi up close in a 32 x 32 portrait (the pen at the frame's top-left): the frost-fur
/// bell of the shoulders, the long neck with its throat lantern, the head turned to the viewer
/// with its frost spikes.
fn draw_vessi_portrait(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    seed: u64,
    character: &Character,
    talking: bool,
    seconds: f32,
) {
    let (fur, shade) = VESSI_FUR[(seed % 3) as usize];
    let lantern = VESSI_LANTERN[((seed >> 8) % 3) as usize];
    let bob = ((seconds * 1.6 + (seed % 7) as f32).sin() * 0.6).round() as i32;
    // The shoulders: the top of the fur bell, tufted along its rim.
    pen.dome(draw, 11, 33, 15, 9, fur);
    pen.rect(draw, -4, 30, 30, 2, shade);
    for tuft in (-2..26).step_by(3) {
        pen.px(draw, tuft, 25 + (tuft % 2), shade);
    }
    // The neck curves up and to the right, banded fur.
    for (i, (x, y)) in [(13, 22), (14, 19), (15, 16), (17, 13), (18, 11)]
        .iter()
        .enumerate()
    {
        pen.rect(
            draw,
            *x,
            *y + bob,
            5,
            4,
            if i % 2 == 0 { fur } else { shade },
        );
    }
    // The throat lantern, brighter and quicker while they speak.
    let pulse = if talking {
        0.75 + 0.25 * (seconds * 9.0).sin()
    } else {
        0.55 + 0.15 * (seconds * 1.3 + (seed % 5) as f32).sin()
    };
    pen.glow(
        draw,
        12,
        14 + bob,
        8,
        8,
        alpha(lantern, (pulse * 70.0) as u8),
    );
    pen.glow(
        draw,
        14,
        16 + bob,
        4,
        4,
        alpha(lantern, (pulse * 255.0) as u8),
    );
    pen.glow(draw, 15, 17 + bob, 1, 1, alpha(CREAM, 230));
    // The head: long and soft, a dark eye, a muzzle with a breathing slit.
    pen.ellipse(draw, 21, 8 + bob, 6, 4, fur);
    pen.rect(draw, 24, 9 + bob, 4, 2, fur);
    pen.rect(draw, 16, 10 + bob, 10, 1, shade);
    pen.rect(draw, 22, 6 + bob, 2, 2, INK);
    pen.px(draw, 22, 6 + bob, CREAM);
    pen.px(draw, 27, 9 + bob, shade);
    if talking && (seconds * 6.0) as i32 % 2 == 0 {
        pen.rect(draw, 24, 11 + bob, 3, 1, INK);
    }
    // Frost spikes, a storyteller's gold among them.
    for (x, y) in [(17, 3), (18, 2), (19, 3), (20, 1)] {
        pen.px(draw, x, y + bob, ICE);
    }
    if character.role == Role::Storyteller {
        pen.px(draw, 18, 1 + bob, GOLD);
        pen.px(draw, 21, 2 + bob, GOLD);
    }
    if character.age >= 70 {
        // An elder's long frost beard.
        pen.rect(draw, 23, 12 + bob, 3, 5, BONE);
        pen.px(draw, 24, 17 + bob, BONE);
    }
}

// --- Buildings -------------------------------------------------------------------------------

/// What a building shows besides its kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildingLook {
    /// A monument plinth whose monument a quest raised.
    pub raised: bool,
    /// A home plot: sold (scaffolding), and built (a home with the owner's flag).
    pub sold: bool,
    pub built: bool,
    /// The building's index in its town (varies the details).
    pub index: u8,
    /// A built home's estate wings (Wave 13 endgame, in
    /// [`fatherload_simulation::endgame::works::ESTATE_WINGS`] order).
    pub wings: u8,
}

/// Draws a town building `width` x `height` tiles of `kind` in `species`' style, its ground
/// centre at the pen (`pen.pixel` per art pixel; sixteen art pixels to a tile).
pub fn draw_building(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    species: AlienSpecies,
    kind: AlienBuildingKind,
    width: i32,
    height: i32,
    look: BuildingLook,
    seconds: f32,
) {
    let w = width * 16;
    let h = height * 16;
    match kind {
        AlienBuildingKind::HomePlot => draw_plot(draw, pen, species, w, look, seconds),
        AlienBuildingKind::Monument => draw_monument(draw, pen, species, w, look, seconds),
        _ => match species {
            AlienSpecies::Vessi => draw_vessi_building(draw, pen, kind, w, h, look, seconds),
            AlienSpecies::Tokkari => draw_tokkari_building(draw, pen, kind, w, h, look, seconds),
            AlienSpecies::Siphra => draw_siphra_building(draw, pen, kind, w, h, look, seconds),
        },
    }
}

fn draw_vessi_building(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    kind: AlienBuildingKind,
    w: i32,
    h: i32,
    look: BuildingLook,
    seconds: f32,
) {
    let ice = Color::new(0xdc, 0xf0, 0xfb, 255);
    let ice_shade = Color::new(0xa4, 0xcc, 0xe6, 255);
    let seam = Color::new(0x7c, 0xa8, 0xcc, 255);
    let snow = Color::new(0xf8, 0xfc, 0xff, 255);
    let warm = Color::new(0xff, 0xbe, 0x62, 255);
    let flicker = 0.85 + 0.15 * (seconds * 4.0 + f32::from(look.index)).sin();
    if kind == AlienBuildingKind::Landmark {
        // The song spire: a slim shaft of frozen glass with rings, a bell lantern on top and
        // frost pennants singing in the wind.
        let top = -h;
        pen.dome(draw, 0, -1, 9, 6, ice_shade);
        pen.rect(draw, -3, top + 14, 7, h - 18, alpha(ICE, 235));
        pen.rect(draw, -1, top + 14, 2, h - 18, alpha(snow, 220));
        pen.rect(draw, 2, top + 14, 1, h - 18, alpha(seam, 200));
        for ring in 0..5 {
            let y = top + 22 + ring * (h - 30) / 5;
            pen.rect(draw, -5, y, 11, 2, seam);
            pen.rect(draw, -5, y, 11, 1, snow);
        }
        pen.dome(draw, 0, top + 14, 7, 9, alpha(ICE, 230));
        let pulse = 0.6 + 0.4 * (seconds * 1.7).sin();
        pen.glow(draw, -2, top + 7, 5, 6, alpha(warm, (pulse * 255.0) as u8));
        pen.glow(draw, -7, top + 2, 15, 15, alpha(warm, (pulse * 50.0) as u8));
        pen.rect(draw, 0, top, 1, 6, snow);
        for i in 0..3 {
            let sway = ((seconds * 2.0 + i as f32).sin() * 2.0).round() as i32;
            let y = top + 26 + i * 14;
            pen.rect(draw, 4, y, 6 + sway, 2, if i % 2 == 0 { ROSE } else { SKY });
        }
        return;
    }
    // A spiral snow-vault: a low dome of packed ice blocks, a snow cap, a tunnel door glowing
    // warm, round windows and icicles at the eaves.
    let rx = (w * 2 / 5).max(18);
    let ry = h.min(rx * 9 / 10).max(14);
    pen.dome(draw, 0, -1, rx, ry, ice);
    // Shade toward the right.
    for dy in 0..ry {
        let t = 1.0 - (dy as f32 / ry as f32).powi(2);
        let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
        pen.rect(draw, half / 3, -1 - dy, half - half / 3 + 1, 1, ice_shade);
    }
    // Block courses, staggered joints.
    for course in (6..ry - 2).step_by(7) {
        let t = 1.0 - (course as f32 / ry as f32).powi(2);
        let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
        pen.rect(draw, -half + 1, -1 - course, 2 * half - 1, 1, seam);
        let stagger = if (course / 7) % 2 == 0 { 0 } else { 5 };
        let mut x = -half + 3 + stagger;
        while x < half - 2 {
            pen.rect(draw, x, -course + 1, 1, 6, seam);
            x += 10;
        }
    }
    // The spiral seam rising round the vault.
    for i in 0..ry {
        let angle = i as f32 * 0.32;
        let x = (angle.cos() * rx as f32 * (1.0 - i as f32 / ry as f32)).round() as i32;
        pen.px(draw, x, -1 - i, snow);
    }
    // The snow cap.
    pen.dome(draw, 0, -ry + 4, rx / 3, 5, snow);
    // Icicles along the lower eaves.
    for x in (-rx + 4..rx - 3).step_by(5) {
        let length = 2 + (x.rem_euclid(3));
        pen.rect(draw, x, -3 - length, 1, length, alpha(ICE, 220));
    }
    // A short ice tunnel and its round door, glowing warm.
    let door = (rx / 3).clamp(8, 14);
    pen.rect(
        draw,
        -door / 2 - 3,
        -door - 2,
        door + 6,
        door + 2,
        ice_shade,
    );
    pen.dome(draw, 0, -door - 2, door / 2 + 3, 4, ice);
    pen.dome(
        draw,
        0,
        -1,
        door / 2,
        door,
        alpha(warm, (flicker * 255.0) as u8),
    );
    pen.glow(
        draw,
        -door / 2 - 2,
        -door - 3,
        door + 4,
        door + 3,
        alpha(warm, 40),
    );
    // Round windows.
    for side in [-1, 1] {
        let (x, y) = (side * rx * 3 / 5, -ry / 2);
        pen.ellipse(draw, x, y, 3, 3, seam);
        pen.ellipse(draw, x, y, 2, 2, alpha(warm, (flicker * 230.0) as u8));
    }
    match kind {
        AlienBuildingKind::TradeHall => {
            // Frost-silk banners and a basket of glow pearls.
            for i in -2..=2 {
                let sway = (seconds * 1.5 + i as f32).sin().round() as i32;
                let x = i * (rx / 3);
                let t = 1.0 - ((x as f32) / rx as f32).powi(2);
                let top = -(ry as f32 * t.max(0.0).sqrt()) as i32;
                pen.rect(
                    draw,
                    x,
                    top,
                    3,
                    8 + sway,
                    if i % 2 == 0 { ROSE } else { SKY },
                );
            }
            pen.rect(draw, rx - 4, -6, 8, 5, CLAY);
            pen.glow(draw, rx - 3, -8, 6, 2, alpha(PINK, 230));
        }
        AlienBuildingKind::ElderHouse => {
            // A crown of frozen glass and a lantern.
            pen.rect(draw, -2, -ry - 10, 5, 10, alpha(ICE, 235));
            pen.rect(draw, -4, -ry - 4, 9, 2, seam);
            pen.glow(draw, -1, -ry - 14, 3, 4, alpha(warm, 230));
        }
        AlienBuildingKind::LexiconHouse => {
            // A frieze of the Vessi's ring glyphs round the vault.
            for i in 0..5 {
                draw_glyph_art(
                    draw,
                    pen,
                    -16 + i * 7,
                    -ry * 2 / 3 - 3,
                    AlienSpecies::Vessi,
                    i as u8 * 2,
                    seam,
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // A kiln vent breathing warm.
            pen.rect(draw, rx / 2, -ry + 2, 5, 7, seam);
            pen.glow(draw, rx / 2 + 1, -ry + 3, 3, 2, alpha(warm, 230));
            let puff = (seconds * 1.3).fract();
            pen.glow(
                draw,
                rx / 2,
                -ry - 2 - (puff * 10.0) as i32,
                5,
                4,
                alpha(CREAM, ((1.0 - puff) * 150.0) as u8),
            );
        }
        _ => {}
    }
}

fn draw_tokkari_building(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    kind: AlienBuildingKind,
    w: i32,
    h: i32,
    look: BuildingLook,
    seconds: f32,
) {
    let stone = Color::new(0xb6, 0x90, 0x6a, 255);
    let stone_light = Color::new(0xd2, 0xae, 0x84, 255);
    let stone_shade = Color::new(0x86, 0x64, 0x48, 255);
    let seam = Color::new(0x56, 0x3e, 0x2c, 255);
    let forge = Color::new(0xff, 0x8e, 0x30, 255);
    let pulse = 0.65 + 0.35 * (seconds * 3.5 + f32::from(look.index)).sin();
    if kind == AlienBuildingKind::Landmark {
        // The forge chimney: a tall stack of fused brick on a mound, its throat glowing, smoke
        // rising.
        pen.dome(draw, 0, -1, 12, 9, stone_shade);
        pen.rect(draw, -5, -h, 11, h - 6, stone);
        pen.rect(draw, 3, -h, 3, h - 6, stone_shade);
        for row in (-h + 2..-8).step_by(5) {
            pen.rect(draw, -5, row, 11, 1, seam);
            let offset = if (row / 5) % 2 == 0 { -2 } else { 2 };
            pen.rect(draw, offset, row + 1, 1, 4, seam);
        }
        pen.rect(draw, -7, -h - 2, 15, 3, stone_light);
        pen.glow(draw, -4, -h - 4, 9, 3, alpha(forge, (pulse * 255.0) as u8));
        pen.glow(draw, -8, -h - 9, 17, 8, alpha(forge, (pulse * 50.0) as u8));
        for i in 0..3 {
            let t = (seconds * 0.35 + i as f32 / 3.0).fract();
            let x = (t * 12.0).round() as i32 - 3;
            let size = 3 + (t * 5.0) as i32;
            pen.glow(
                draw,
                x,
                -h - 8 - (t * 34.0) as i32,
                size,
                size,
                alpha(STONE_LIGHT, ((1.0 - t) * 140.0) as u8),
            );
        }
        // The forge mouth at its foot.
        pen.dome(draw, 0, -1, 4, 6, seam);
        pen.dome(draw, 0, -1, 3, 4, alpha(forge, (pulse * 255.0) as u8));
        return;
    }
    // A terraced mound-hall of fused stone, half sunk into the ground: rounded tiers, a round
    // door with a copper ring, glowing round windows and a forge chimney on top.
    let tiers = (h / 14).clamp(2, 3);
    let tier_h = (h / tiers).clamp(11, 16);
    let base_half = (w * 2 / 5).max(16);
    for tier in 0..tiers {
        let half = base_half - tier * (base_half / (tiers + 1));
        let y = -(tier + 1) * tier_h;
        // Rounded shoulders: the tier's top rows step in.
        pen.rect(draw, -half, y + 2, 2 * half + 1, tier_h - 2, stone);
        pen.rect(draw, -half + 2, y, 2 * half - 3, 2, stone);
        pen.rect(draw, -half + 1, y + 1, 2 * half - 1, 1, stone);
        pen.rect(draw, -half + 2, y, 2 * half - 3, 1, stone_light);
        pen.rect(
            draw,
            half - half / 3,
            y + 2,
            half / 3 + 1,
            tier_h - 2,
            stone_shade,
        );
        // Fused seams.
        let mut x = -half + 4 + tier * 2;
        while x < half - 4 {
            pen.rect(draw, x, y + tier_h / 2, 3, 1, seam);
            x += 9;
        }
        // Round windows glowing forge-orange.
        if tier > 0 {
            for side in [-1, 1] {
                let wx = side * half / 2;
                pen.ellipse(draw, wx, y + tier_h / 2, 2, 2, seam);
                pen.glow(
                    draw,
                    wx - 1,
                    y + tier_h / 2 - 1,
                    3,
                    3,
                    alpha(forge, (pulse * 220.0) as u8),
                );
            }
        }
    }
    // The round door on the ground tier, a copper ring round it.
    let door = (base_half / 3).clamp(6, 9);
    let door_x = -base_half / 3;
    pen.ellipse(draw, door_x, -door, door + 1, door + 1, COPPER);
    pen.ellipse(draw, door_x, -door, door, door, seam);
    pen.ellipse(
        draw,
        door_x,
        -door + 1,
        door - 2,
        door - 2,
        alpha(forge, (pulse * 120.0) as u8),
    );
    pen.rect(draw, door_x - door - 1, -1, 2 * door + 3, 1, stone_shade);
    // The forge chimney on the top tier.
    let top = -tiers * tier_h;
    let chimney_x = base_half / 4;
    pen.rect(draw, chimney_x, top - 7, 4, 7, stone_shade);
    pen.rect(draw, chimney_x - 1, top - 8, 6, 1, stone_light);
    pen.glow(
        draw,
        chimney_x,
        top - 9,
        4,
        1,
        alpha(forge, (pulse * 255.0) as u8),
    );
    let puff = (seconds * 0.6 + f32::from(look.index) * 0.3).fract();
    pen.glow(
        draw,
        chimney_x + (puff * 4.0) as i32,
        top - 12 - (puff * 14.0) as i32,
        3,
        3,
        alpha(STONE_LIGHT, ((1.0 - puff) * 130.0) as u8),
    );
    match kind {
        AlienBuildingKind::TradeHall => {
            // A balance-scale sign over the door: honest weight.
            let x = door_x;
            let y = -2 * door - 8;
            pen.rect(draw, x, y, 1, 6, STEEL);
            pen.rect(draw, x - 5, y, 11, 1, STEEL);
            pen.rect(draw, x - 6, y + 2, 4, 1, GOLD);
            pen.rect(draw, x + 3, y + 2, 4, 1, GOLD);
        }
        AlienBuildingKind::ElderHouse => {
            // The knotted cords of the hold's debts hanging from the eaves.
            for i in 0..7 {
                let x = -base_half + 5 + i * 4;
                pen.rect(draw, x, -tier_h + 2, 1, 6 + i % 3, CREAM);
                pen.px(draw, x, -tier_h + 4 + i % 3, MAROON);
                pen.px(draw, x, -tier_h + 6 + i % 2, MAROON);
            }
        }
        AlienBuildingKind::LexiconHouse => {
            for i in 0..4 {
                draw_glyph_art(
                    draw,
                    pen,
                    -12 + i * 7,
                    top + 4,
                    AlienSpecies::Tokkari,
                    i as u8 * 3,
                    seam,
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // An anvil by the door, and sparks.
            let x = base_half - 6;
            pen.rect(draw, x, -4, 6, 4, STEEL_DARK);
            pen.rect(draw, x - 1, -5, 8, 1, STEEL);
            let spark = (seconds * 2.0).fract();
            if spark < 0.4 {
                pen.glow(
                    draw,
                    x + 3 + (spark * 10.0) as i32,
                    -7 - (spark * 8.0) as i32,
                    1,
                    1,
                    GOLD,
                );
                pen.glow(
                    draw,
                    x + 2 - (spark * 8.0) as i32,
                    -6 - (spark * 6.0) as i32,
                    1,
                    1,
                    forge,
                );
            }
        }
        _ => {}
    }
}

fn draw_siphra_building(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    kind: AlienBuildingKind,
    w: i32,
    h: i32,
    look: BuildingLook,
    seconds: f32,
) {
    let crystal = Color::new(0xc4, 0xd2, 0xf6, 255);
    let crystal_shade = Color::new(0x84, 0x92, 0xcc, 255);
    let (glass, inner) = SIPHRA_GLASS[usize::from(look.index) % 3];
    if kind == AlienBuildingKind::Landmark {
        // The memory pylon: a tall faceted crystal, light rising inside it, memories circling.
        pen.dome(draw, 0, -1, 8, 4, crystal_shade);
        for dy in 0..h {
            let half = 4 - dy * 4 / h.max(1);
            pen.rect(
                draw,
                -half,
                -dy - 3,
                2 * half + 1,
                1,
                if dy % 11 == 0 { CREAM } else { crystal },
            );
            pen.rect(draw, 1, -dy - 3, half.max(0), 1, crystal_shade);
        }
        let pulse = 0.5 + 0.5 * (seconds * 1.4).sin();
        let rise = ((seconds * 0.5).fract() * h as f32) as i32;
        pen.glow(
            draw,
            -1,
            -rise - 6,
            3,
            6,
            alpha(inner, (120.0 + pulse * 120.0) as u8),
        );
        for i in 0..7 {
            let angle = seconds * 0.9 + i as f32 * 0.9;
            let x = (angle.cos() * 10.0).round() as i32;
            let y = -h / 2 - 12 + (angle.sin() * 3.0).round() as i32 + i * 4;
            pen.glow(draw, x, y, 1, 1, if i % 2 == 0 { CREAM } else { GOLD });
        }
        return;
    }
    // A lantern-pod of grown glass hung from a crystal pylon and tethered over the ground.
    let pod_rx = (w / 3).clamp(10, 22);
    let pod_ry = pod_rx * 4 / 5;
    let pylon_h = h.clamp(48, 110);
    let pylon_x = -pod_rx - 5;
    for dy in 0..pylon_h {
        let half = if dy > pylon_h - 6 {
            (pylon_h - dy) / 3
        } else {
            1
        };
        pen.rect(draw, pylon_x - half, -dy - 1, 2 * half + 2, 1, crystal);
        pen.px(draw, pylon_x + 1, -dy - 1, crystal_shade);
    }
    pen.glow(draw, pylon_x - 1, -pylon_h - 2, 4, 3, alpha(CREAM, 230));
    let sway = ((seconds * 0.8 + f32::from(look.index)).sin() * 1.5).round() as i32;
    let pod_y = -pylon_h + pod_ry + 8;
    // The arm and the hanger.
    pen.rect(
        draw,
        pylon_x,
        -pylon_h + 3,
        -pylon_x + sway + 1,
        1,
        crystal_shade,
    );
    pen.rect(
        draw,
        sway,
        -pylon_h + 3,
        1,
        pod_y - pod_ry + pylon_h - 3,
        alpha(CREAM, 180),
    );
    // The pod: an outer glow, the glass, its inner light and a highlight.
    let pulse = 0.5 + 0.5 * (seconds * 1.2 + f32::from(look.index)).sin();
    pen.glow_ellipse(draw, sway, pod_y, pod_rx + 4, pod_ry + 4, alpha(glass, 40));
    pen.ellipse(draw, sway, pod_y, pod_rx, pod_ry, alpha(glass, 200));
    pen.glow_ellipse(
        draw,
        sway,
        pod_y + 1,
        pod_rx - 3,
        pod_ry - 3,
        alpha(inner, (120.0 + 110.0 * pulse) as u8),
    );
    pen.rect(
        draw,
        sway - pod_rx / 2,
        pod_y - pod_ry + 3,
        pod_rx / 2,
        2,
        alpha(CREAM, 170),
    );
    // Window-dots round its waist.
    for i in -2..=2 {
        pen.glow(draw, sway + i * pod_rx / 3, pod_y + 1, 1, 1, CREAM);
    }
    // A slit door underneath and a ladder of light down to the ground.
    pen.rect(draw, sway - 2, pod_y + pod_ry - 2, 5, 2, alpha(NIGHT, 210));
    let ladder_top = pod_y + pod_ry;
    let mut y = ladder_top;
    while y < -1 {
        pen.glow(draw, sway, y, 1, 2, alpha(inner, 170));
        y += 4;
    }
    match kind {
        AlienBuildingKind::TradeHall => {
            // Small pods of goods hanging alongside.
            for i in 0..3 {
                let x = sway + pod_rx + 4 + i * 6;
                pen.rect(draw, x, pod_y - pod_ry, 1, 6 + i * 3, alpha(CREAM, 160));
                pen.ellipse(draw, x, pod_y - pod_ry + 8 + i * 3, 2, 2, alpha(GOLD, 230));
            }
        }
        AlienBuildingKind::ElderHouse => {
            pen.glow(draw, sway - 1, pod_y - pod_ry - 6, 3, 3, alpha(GOLD, 230));
            pen.glow(draw, sway - 4, pod_y - pod_ry - 9, 9, 9, alpha(GOLD, 40));
        }
        AlienBuildingKind::LexiconHouse => {
            for i in 0..3 {
                draw_glyph_art(
                    draw,
                    pen,
                    sway - 10 + i * 7,
                    pod_y - 3,
                    AlienSpecies::Siphra,
                    i as u8 * 4,
                    alpha(CREAM, 235),
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // A half-grown pod on a second, shorter pylon.
            let x = pod_rx + 8;
            pen.rect(draw, x, -pylon_h / 2, 2, pylon_h / 2, crystal);
            pen.ellipse(draw, x + 1, -pylon_h / 2 - 4, 5, 4, alpha(glass, 140));
        }
        _ => {}
    }
}

fn draw_monument(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    species: AlienSpecies,
    w: i32,
    look: BuildingLook,
    seconds: f32,
) {
    let (stone, accent) = match species {
        AlienSpecies::Vessi => (Color::new(0xa8, 0xc8, 0xdc, 255), AMBER),
        AlienSpecies::Tokkari => (Color::new(0x7c, 0x60, 0x4a, 255), ORANGE),
        AlienSpecies::Siphra => (Color::new(0x9a, 0xa8, 0xd8, 255), MINT),
    };
    // The plinth.
    pen.rect(draw, -w / 2 + 2, -6, w - 4, 6, stone);
    pen.rect(draw, -w / 2 + 1, -7, w - 2, 1, lit(stone, 0.75));
    if !look.raised {
        // Waiting: an empty plinth with a marker stake.
        pen.rect(draw, -1, -16, 2, 10, lit(stone, 0.6));
        return;
    }
    let pulse = 0.6 + 0.4 * (seconds * 2.2).sin();
    match species {
        AlienSpecies::Vessi => {
            // The spire-bell: a frame of frozen glass with a bell that swings and glows.
            pen.rect(draw, -8, -40, 2, 33, ICE);
            pen.rect(draw, 6, -40, 2, 33, ICE);
            pen.rect(draw, -8, -42, 16, 2, ICE);
            let swing = ((seconds * 1.5).sin() * 2.0).round() as i32;
            pen.dome(draw, swing, -22, 5, 9, GOLD);
            pen.glow(
                draw,
                swing - 1,
                -24,
                3,
                3,
                alpha(accent, (pulse * 255.0) as u8),
            );
        }
        AlienSpecies::Tokkari => {
            // The new forge: an anvil-shaped hearth burning bright.
            pen.rect(draw, -10, -18, 20, 11, lit(stone, 0.8));
            pen.rect(draw, -7, -16, 14, 6, NIGHT);
            pen.glow(draw, -6, -15, 12, 5, alpha(accent, (pulse * 255.0) as u8));
            pen.glow(draw, -10, -24, 20, 6, alpha(accent, (pulse * 60.0) as u8));
        }
        AlienSpecies::Siphra => {
            // The memory pylon's heart: a diamond crystal, turning light.
            for dy in 0..24 {
                let half = 6 - (dy - 12_i32).abs() / 2;
                pen.rect(draw, -half, -8 - dy, 2 * half, 1, alpha(CREAM, 220));
            }
            pen.glow(draw, -3, -24, 6, 8, alpha(accent, (pulse * 255.0) as u8));
        }
    }
}

/// An estate's wings round a home on an alien world (Wave 13 endgame), in the order they are
/// raised: a bedroom annex right, a kitchen with its chimney left, a garage right, a glass
/// greenhouse left, an upper storey over the bedroom annex, a gallery's tall windows right and
/// an observatory's dome on the gallery. A paved terrace joins them to the house. Walls take
/// the species' stone, roofs its accent.
#[allow(clippy::too_many_lines, reason = "one block per wing, in order")]
fn draw_estate_wings(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    species: AlienSpecies,
    w: i32,
    _h: i32,
    wings: u8,
    seconds: f32,
) {
    if wings == 0 {
        return;
    }
    let (wall, roof) = match species {
        AlienSpecies::Vessi => (Color::new(0x9c, 0xb8, 0xc8, 255), ROSE),
        AlienSpecies::Tokkari => (Color::new(0xb6, 0x90, 0x6a, 255), ORANGE),
        AlienSpecies::Siphra => (Color::new(0x8c, 0x84, 0xb0, 255), MINT),
    };
    let window = lit(AMBER, 0.9);
    // A Siphra home is a slender pylon: its wings gather close round its foot.
    let half = match species {
        AlienSpecies::Siphra => 18,
        AlienSpecies::Vessi | AlienSpecies::Tokkari => w / 2 - 4,
    };
    let has = |wing: u8| wings > wing;
    let right_end = half
        + if has(5) {
            58
        } else if has(2) {
            42
        } else {
            18
        };
    let left_end = -half - if has(3) { 40 } else { 18 };
    // The terrace under it all.
    pen.rect(
        draw,
        left_end - 2,
        -2,
        right_end - left_end + 4,
        2,
        lit(wall, 0.6),
    );
    pen.rect(
        draw,
        left_end - 2,
        -3,
        right_end - left_end + 4,
        1,
        lit(wall, 0.9),
    );
    // 0: a bedroom annex on the right.
    if has(0) {
        pen.rect(draw, half, -32, 18, 30, wall);
        pen.rect(draw, half, -32, 18, 2, lit(wall, 1.2));
        pen.rect(draw, half + 17, -32, 1, 30, lit(wall, 0.7));
        pen.rect(draw, half + 6, -24, 6, 6, window);
        pen.rect(draw, half + 6, -21, 6, 1, lit(wall, 0.8));
        if !has(4) {
            pen.rect(draw, half - 1, -36, 20, 4, roof);
        }
    }
    // 1: a kitchen on the left, its chimney breathing smoke.
    if has(1) {
        pen.rect(draw, -half - 18, -28, 18, 26, wall);
        pen.rect(draw, -half - 18, -28, 1, 26, lit(wall, 0.7));
        pen.rect(draw, -half - 19, -32, 20, 4, roof);
        pen.rect(draw, -half - 14, -42, 4, 10, lit(wall, 0.7));
        let rise = ((seconds * 0.6) % 6.0) as i32;
        pen.rect(
            draw,
            -half - 14 + rise / 3,
            -46 - rise,
            3,
            3,
            alpha(FOG, 140),
        );
        pen.rect(draw, -half - 11, -20, 6, 5, window);
    }
    // 2: a garage bay on the right, past the annex.
    if has(2) {
        pen.rect(draw, half + 18, -22, 24, 20, lit(wall, 0.85));
        pen.rect(draw, half + 17, -25, 26, 3, roof);
        pen.rect(draw, half + 22, -18, 16, 16, STEEL_DARK);
        for slat in (-17..-2).step_by(3) {
            pen.rect(draw, half + 22, slat, 16, 1, STEEL);
        }
    }
    // 3: a glass greenhouse on the left, past the kitchen.
    if has(3) {
        pen.rect(draw, -half - 40, -24, 22, 22, alpha(MINT, 110));
        pen.rect(draw, -half - 40, -24, 22, 1, ICE);
        for mullion in (-half - 40..-half - 18).step_by(5) {
            pen.rect(draw, mullion, -24, 1, 22, alpha(ICE, 180));
        }
        for plant in 0..4 {
            pen.rect(draw, -half - 37 + plant * 5, -10, 2, 8, LEAF);
            pen.rect(draw, -half - 38 + plant * 5, -12, 4, 3, MOSS);
        }
    }
    // 4: an upper storey over the bedroom annex.
    if has(4) && has(0) {
        pen.rect(draw, half, -52, 18, 20, wall);
        pen.rect(draw, half + 17, -52, 1, 20, lit(wall, 0.7));
        pen.rect(draw, half - 1, -56, 20, 4, roof);
        pen.rect(draw, half + 6, -46, 6, 6, window);
    }
    // 5: a gallery tower on the far right, tall windows.
    if has(5) {
        let x = half + if has(2) { 42 } else { 18 };
        pen.rect(draw, x, -44, 16, 42, lit(wall, 1.05));
        pen.rect(draw, x + 15, -44, 1, 42, lit(wall, 0.7));
        pen.rect(draw, x + 3, -36, 3, 20, alpha(SKY_LIGHT, 200));
        pen.rect(draw, x + 10, -36, 3, 20, alpha(SKY_LIGHT, 200));
        // 6: an observatory dome on the gallery, its slit to the sky.
        if has(6) {
            let cx = x + 8;
            for dy in 0..10 {
                let r = 10 - dy * dy / 10;
                pen.rect(
                    draw,
                    cx - r,
                    -45 - dy,
                    r * 2,
                    1,
                    lit(STEEL_LIGHT, 1.0 - dy as f32 * 0.03),
                );
            }
            pen.rect(draw, cx - 1, -55, 2, 8, NIGHT);
            pen.rect(draw, cx + 3, -54, 7, 2, STEEL_DARK);
        } else {
            pen.rect(draw, x - 1, -48, 18, 4, roof);
        }
    }
}

fn draw_plot(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    species: AlienSpecies,
    w: i32,
    look: BuildingLook,
    seconds: f32,
) {
    if look.built {
        // A home in the species' style, with the owner's pennant.
        let height = match species {
            AlienSpecies::Vessi => 6,
            AlienSpecies::Tokkari => 4,
            AlienSpecies::Siphra => 7,
        };
        let inner = BuildingLook {
            index: look.index.wrapping_add(1),
            ..look
        };
        match species {
            AlienSpecies::Vessi => draw_vessi_building(
                draw,
                pen,
                AlienBuildingKind::Dwelling,
                w - 8,
                height * 16,
                inner,
                seconds,
            ),
            AlienSpecies::Tokkari => draw_tokkari_building(
                draw,
                pen,
                AlienBuildingKind::Dwelling,
                w - 8,
                height * 16,
                inner,
                seconds,
            ),
            AlienSpecies::Siphra => draw_siphra_building(
                draw,
                pen,
                AlienBuildingKind::Dwelling,
                w - 8,
                height * 16,
                inner,
                seconds,
            ),
        }
        draw_estate_wings(draw, pen, species, w, height * 16, look.wings, seconds);
        // The owner's pennant on a short staff at the plot's corner, beside the door (a staff
        // as tall as the home stood apart from it and read as a lone pole).
        let sway = ((seconds * 2.0).sin() * 1.5).round() as i32;
        let staff = 26;
        pen.rect(draw, w / 2 - 6, -2, 5, 2, STONE_LIGHT);
        pen.rect(draw, w / 2 - 4, -staff, 1, staff, STEEL_LIGHT);
        pen.rect(draw, w / 2 - 3, -staff, 7 + sway, 3, GOLD);
        pen.rect(draw, w / 2 - 3, -staff + 3, 4 + sway, 2, lit(GOLD, 0.75));
        return;
    }
    if look.sold {
        // Scaffolding going up.
        for x in (-w / 2 + 4..w / 2 - 3).step_by(8) {
            pen.rect(draw, x, -40, 1, 40, CLAY);
        }
        for y in (-40..0).step_by(10) {
            pen.rect(draw, -w / 2 + 4, y, w - 8, 1, CLAY);
        }
        let blink = (seconds * 1.5) as i32 % 2 == 0;
        pen.glow(
            draw,
            w / 2 - 6,
            -42,
            2,
            2,
            if blink { AMBER } else { alpha(AMBER, 80) },
        );
        return;
    }
    // For sale: corner stakes and a sign in the species' writing.
    let rope = match species {
        AlienSpecies::Vessi => ROSE,
        AlienSpecies::Tokkari => CREAM,
        AlienSpecies::Siphra => MINT,
    };
    for x in [-w / 2 + 2, w / 2 - 3] {
        pen.rect(draw, x, -8, 1, 8, CLAY);
    }
    pen.rect(draw, -w / 2 + 2, -7, w - 4, 1, alpha(rope, 200));
    pen.rect(draw, -1, -20, 2, 20, CLAY);
    pen.rect(draw, -8, -24, 16, 8, CREAM);
    draw_glyph_art(draw, pen, -6, -23, species, 1, INK);
    draw_glyph_art(draw, pen, 1, -23, species, 5, INK);
}

// --- Writing ---------------------------------------------------------------------------------

/// Art pixels a glyph is wide and tall.
pub const GLYPH_SIZE: (i32, i32) = (5, 6);

/// The pixels of `species`' glyph `index` in a [`GLYPH_SIZE`] cell (a 5 x 6 bitmap, row by
/// row, bit 4 the left column).
#[must_use]
pub fn glyph_bits(species: AlienSpecies, index: u8) -> [u8; 6] {
    let roll = mix(u64::from(index) ^ (u64::from(species.index()) << 8) ^ 0x474C_5950_4800);
    match species {
        AlienSpecies::Vessi => {
            // A sung note: a small ring (the note's head) high or low, left or right, with a
            // stem and a flag or a held dot. (Full-cell rings with ticks all read as the same
            // "o" at text size; the heads and stems give each glyph its own outline.)
            let shape = index % 12;
            let low = shape & 1 == 1;
            let right = shape & 2 == 2;
            let deco = shape / 4;
            let mut bits = [0_u8; 6];
            let head_row = if low { 3 } else { 0 };
            let shift = if right { 0 } else { 2 };
            bits[head_row] |= 0b111 << shift;
            bits[head_row + 1] |= (if deco == 2 { 0b111 } else { 0b101 }) << shift;
            bits[head_row + 2] |= 0b111 << shift;
            // The stem runs up (or down) the head's outer edge across the rest of the cell.
            let stem_col: u8 = if right { 0b00001 } else { 0b10000 };
            let stem_rows = if low { 0..3 } else { 3..6 };
            for row in stem_rows {
                bits[row] |= stem_col;
            }
            if deco >= 1 {
                // A flag off the stem's free end, pointing inward.
                let row = if low { 0 } else { 5 };
                bits[row] |= if right { 0b00010 } else { 0b01000 };
            }
            bits
        }
        AlienSpecies::Tokkari => {
            // A stem with notches cut either side (a knot count).
            let mut bits = [0b00100; 6];
            for (row, bits_row) in bits.iter_mut().enumerate() {
                match (roll >> (row * 2)) & 3 {
                    1 => *bits_row |= 0b01000,
                    2 => *bits_row |= 0b00010,
                    3 => *bits_row |= 0b11011,
                    _ => {}
                }
            }
            bits[0] |= if index.is_multiple_of(2) { 0b01110 } else { 0 };
            bits
        }
        AlienSpecies::Siphra => {
            // Three or four stars joined by lines (a constellation).
            let mut bits = [0_u8; 6];
            let stars = 3 + (roll % 2) as usize;
            let mut last: Option<(i32, i32)> = None;
            for star in 0..stars {
                let x = ((roll >> (8 + star * 6)) % 5) as i32;
                let y = ((roll >> (11 + star * 6)) % 6) as i32;
                if let Some((lx, ly)) = last {
                    let steps = (x - lx).abs().max((y - ly).abs()).max(1);
                    for s in 0..=steps {
                        let px = lx + (x - lx) * s / steps;
                        let py = ly + (y - ly) * s / steps;
                        bits[py as usize] |= 1 << (4 - px);
                    }
                }
                bits[y as usize] |= 1 << (4 - x);
                last = Some((x, y));
            }
            bits
        }
    }
}

/// Draws glyph `index` of `species` with its top-left at art pixel `(x, y)` of the pen.
pub fn draw_glyph_art(
    draw: &mut impl RaylibDraw,
    pen: ArtPen,
    x: i32,
    y: i32,
    species: AlienSpecies,
    index: u8,
    colour: Color,
) {
    let bits = glyph_bits(species, index);
    for (row, bits) in bits.iter().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) != 0 {
                pen.rect(draw, x + column, y + row as i32, 1, 1, colour);
            }
        }
    }
}

/// Draws glyph `index` of `species` with its top-left at screen point `(x, y)`, `scale` points
/// per glyph pixel (the talk panel's writing).
pub fn draw_glyph(
    draw: &mut impl RaylibDraw,
    x: f32,
    y: f32,
    scale: f32,
    species: AlienSpecies,
    index: u8,
    colour: Color,
) {
    let pen = ArtPen::new(x, y, scale, 1.0);
    draw_glyph_art(draw, pen, 0, 0, species, index, colour);
}

/// Screen points a glyph run of `count` glyphs takes at `scale`.
#[must_use]
pub fn glyphs_width(count: usize, scale: f32) -> f32 {
    count as f32 * (GLYPH_SIZE.0 as f32 + 1.0) * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_drawn_and_the_scripts_differ() {
        for species in AlienSpecies::ALL {
            let mut seen = std::collections::BTreeSet::new();
            for index in 0..species.def().glyphs {
                let bits = glyph_bits(species, index);
                assert!(
                    bits.iter().any(|row| *row != 0),
                    "{species:?} {index} is blank"
                );
                assert!(bits.iter().all(|row| *row < 32));
                seen.insert(bits);
            }
            assert!(
                seen.len() * 4 >= usize::from(species.def().glyphs) * 3,
                "{species:?}: most glyphs differ"
            );
        }
        assert_ne!(
            glyph_bits(AlienSpecies::Vessi, 3),
            glyph_bits(AlienSpecies::Tokkari, 3)
        );
    }
}
