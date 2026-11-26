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
    pen.glow(draw, -10, base - 20, 21, 22, alpha(glass, 26));
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
    let (scale, x, y) = match species {
        AlienSpecies::Vessi => (1.6, 4.0, 72.0),
        AlienSpecies::Tokkari => (2.4, 14.0, 52.0),
        AlienSpecies::Siphra => (1.5, 16.0, 60.0),
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
            draw_vessi(
                draw,
                art,
                seed,
                character.role,
                character.age,
                pose,
                seconds,
            );
        }
        AlienSpecies::Tokkari => draw_tokkari(draw, art, seed, character.role, pose, seconds),
        AlienSpecies::Siphra => draw_siphra(draw, art, seed, character.role, pose, seconds),
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
    let ice = Color::new(0xcc, 0xe6, 0xf2, 255);
    let ice_shade = Color::new(0x98, 0xbe, 0xd6, 255);
    let line = Color::new(0x7a, 0xa2, 0xbe, 255);
    let warm = Color::new(0xff, 0xb8, 0x5c, 255);
    if kind == AlienBuildingKind::Landmark {
        // The song spire: a slim shaft of frozen glass with rings and a bell lantern on top.
        let top = -h;
        pen.rect(draw, -6, -6, 12, 6, ice_shade);
        pen.rect(draw, -3, top + 14, 6, h - 20, alpha(ICE, 230));
        pen.rect(draw, -1, top + 14, 1, h - 20, alpha(CREAM, 200));
        for ring in 0..4 {
            let y = top + 22 + ring * (h - 32) / 4;
            pen.rect(draw, -5, y, 10, 2, line);
        }
        pen.dome(draw, 0, top + 14, 6, 8, alpha(ICE, 220));
        let pulse = 0.6 + 0.4 * (seconds * 1.7).sin();
        pen.glow(draw, -2, top + 6, 4, 5, alpha(warm, (pulse * 255.0) as u8));
        pen.glow(draw, -6, top + 2, 12, 13, alpha(warm, (pulse * 60.0) as u8));
        // Frost pennants singing in the wind.
        for i in 0..3 {
            let sway = ((seconds * 2.0 + i as f32).sin() * 2.0).round() as i32;
            pen.rect(draw, 3, top + 24 + i * 10, 5 + sway, 1, alpha(ROSE, 200));
        }
        return;
    }
    // A spiral snow-vault: a dome of packed ice blocks with a spiral seam, lit from inside.
    let rx = w / 2 - 1;
    let ry = h - 2;
    pen.dome(draw, 0, -1, rx, ry, ice);
    // Shade on the right side.
    for dy in 0..ry {
        let t = 1.0 - (dy as f32 / ry as f32).powi(2);
        let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
        pen.rect(draw, half / 2, -1 - dy, half - half / 2 + 1, 1, ice_shade);
    }
    // Block courses and the spiral seam.
    for course in (4..ry).step_by(5) {
        let t = 1.0 - (course as f32 / ry as f32).powi(2);
        let half = (rx as f32 * t.max(0.0).sqrt()).round() as i32;
        pen.rect(draw, -half, -1 - course, 2 * half + 1, 1, line);
        let stagger = if (course / 5) % 2 == 0 { 0 } else { 4 };
        let mut x = -half + stagger;
        while x < half {
            pen.px(draw, x, -course, line);
            x += 8;
        }
    }
    for i in 0..ry {
        let angle = i as f32 * 0.35;
        let x = (angle.cos() * rx as f32 * (1.0 - i as f32 / ry as f32)).round() as i32;
        pen.px(draw, x, -1 - i, alpha(CREAM, 220));
    }
    // A round door glowing warm, an arch of blocks over it.
    let door = (w / 6).clamp(5, 9);
    pen.dome(draw, 0, -1, door / 2 + 1, door + 1, line);
    let flicker = 0.85 + 0.15 * (seconds * 5.0 + f32::from(look.index)).sin();
    pen.glow(
        draw,
        -door / 2,
        -door,
        door,
        door,
        alpha(warm, (flicker * 255.0) as u8),
    );
    pen.dome(
        draw,
        0,
        -door,
        door / 2,
        door / 2,
        alpha(warm, (flicker * 255.0) as u8),
    );
    // Round windows.
    for side in [-1, 1] {
        pen.ellipse(draw, side * rx / 2, -ry / 2, 2, 2, line);
        pen.glow(draw, side * rx / 2 - 1, -ry / 2 - 1, 3, 3, alpha(warm, 220));
    }
    // What the building is.
    match kind {
        AlienBuildingKind::TradeHall => {
            // Frost-silk banners and a basket of glow pearls.
            for i in -2..=2 {
                let sway = ((seconds * 1.5 + i as f32).sin()).round() as i32;
                pen.rect(
                    draw,
                    i * 8 - 1,
                    -ry - 3,
                    3,
                    6 + sway,
                    if i % 2 == 0 { ROSE } else { SKY },
                );
            }
            pen.rect(draw, rx - 6, -5, 6, 4, CLAY);
            pen.glow(draw, rx - 5, -7, 4, 2, alpha(PINK, 230));
        }
        AlienBuildingKind::ElderHouse => {
            // A taller crown of frozen glass.
            pen.rect(draw, -2, -ry - 8, 4, 8, alpha(ICE, 230));
            pen.glow(draw, -1, -ry - 11, 2, 3, alpha(warm, 220));
        }
        AlienBuildingKind::LexiconHouse => {
            // A frieze of the Vessi's ring glyphs.
            for i in 0..5 {
                draw_glyph_art(
                    draw,
                    pen,
                    -12 + i * 6,
                    -ry / 2 - 10,
                    AlienSpecies::Vessi,
                    i as u8 * 2,
                    alpha(line, 255),
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // A kiln vent breathing warm.
            pen.rect(draw, rx - 8, -ry + 2, 4, 6, line);
            let puff = (seconds * 1.3).fract();
            pen.glow(
                draw,
                rx - 8,
                -ry - 2 - (puff * 8.0) as i32,
                4,
                3,
                alpha(CREAM, ((1.0 - puff) * 160.0) as u8),
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
    let stone = Color::new(0x9a, 0x7c, 0x62, 255);
    let stone_shade = Color::new(0x6e, 0x54, 0x40, 255);
    let seam = Color::new(0x4c, 0x38, 0x2a, 255);
    let forge = Color::new(0xff, 0x8a, 0x2c, 255);
    if kind == AlienBuildingKind::Landmark {
        // The forge chimney: a tall brick stack with a glowing throat and smoke.
        pen.rect(draw, -8, -10, 16, 10, stone_shade);
        pen.rect(draw, -5, -h, 10, h - 8, stone);
        for row in (-h..-10).step_by(4) {
            pen.rect(draw, -5, row, 10, 1, seam);
            let offset = if (row / 4) % 2 == 0 { -2 } else { 2 };
            pen.px(draw, offset, row + 2, seam);
        }
        let pulse = 0.7 + 0.3 * (seconds * 3.0).sin();
        pen.glow(draw, -3, -h - 2, 6, 3, alpha(forge, (pulse * 255.0) as u8));
        pen.glow(draw, -6, -h - 5, 12, 6, alpha(forge, (pulse * 70.0) as u8));
        for i in 0..3 {
            let t = (seconds * 0.4 + i as f32 / 3.0).fract();
            let x = (t * 10.0).round() as i32 - 2;
            pen.glow(
                draw,
                x,
                -h - 6 - (t * 30.0) as i32,
                4,
                3,
                alpha(STONE_LIGHT, ((1.0 - t) * 150.0) as u8),
            );
        }
        pen.glow(draw, -2, -6, 4, 4, alpha(forge, 230));
        return;
    }
    // A terraced mound-hall of fused stone, half sunk into the ground.
    let tiers = (h / 10).clamp(2, 4);
    let tier_h = h / tiers;
    for tier in 0..tiers {
        let half = w / 2 - tier * (w / (2 * tiers + 2)) - 1;
        let y = -(tier + 1) * tier_h;
        pen.rect(
            draw,
            -half,
            y,
            2 * half,
            tier_h,
            if tier % 2 == 0 { stone } else { stone_shade },
        );
        pen.rect(draw, -half, y, 2 * half, 1, seam);
        // A rounded shoulder.
        pen.px(draw, -half, y, alpha(stone, 0));
        // Fused-stone seams.
        let mut x = -half + 3 + tier;
        while x < half - 2 {
            pen.px(draw, x, y + tier_h / 2, seam);
            x += 7;
        }
    }
    // A round door with a copper ring.
    let door = (w / 6).clamp(4, 7);
    pen.ellipse(draw, -w / 5, -door, door / 2 + 1, door / 2 + 1, COPPER);
    pen.ellipse(draw, -w / 5, -door, door / 2, door / 2, seam);
    pen.glow(draw, -w / 5 - 1, -door - 1, 2, 2, alpha(forge, 140));
    // A small forge chimney on the top terrace.
    let top = -tiers * tier_h;
    pen.rect(draw, w / 6, top - 5, 3, 5, stone_shade);
    let pulse = 0.6 + 0.4 * (seconds * 4.0 + f32::from(look.index)).sin();
    pen.glow(
        draw,
        w / 6,
        top - 6,
        3,
        1,
        alpha(forge, (pulse * 255.0) as u8),
    );
    match kind {
        AlienBuildingKind::TradeHall => {
            // A balance-scale sign: honest weight.
            pen.rect(draw, w / 4, top - 10, 1, 8, STEEL);
            pen.rect(draw, w / 4 - 4, top - 10, 9, 1, STEEL);
            pen.rect(draw, w / 4 - 5, top - 8, 3, 1, GOLD);
            pen.rect(draw, w / 4 + 3, top - 8, 3, 1, GOLD);
        }
        AlienBuildingKind::ElderHouse => {
            // Knotted cords of the debts hanging from the eaves.
            for i in 0..6 {
                let x = -w / 2 + 6 + i * 4;
                pen.rect(draw, x, -tier_h - 1, 1, 6 + i % 3, CREAM);
                pen.px(draw, x, -tier_h + 2 + i % 3, MAROON);
            }
        }
        AlienBuildingKind::LexiconHouse => {
            for i in 0..4 {
                draw_glyph_art(
                    draw,
                    pen,
                    -8 + i * 6,
                    top + 4,
                    AlienSpecies::Tokkari,
                    i as u8 * 3,
                    seam,
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // An anvil by the door and sparks.
            pen.rect(draw, w / 2 - 2, -3, 5, 3, STEEL_DARK);
            pen.rect(draw, w / 2 - 3, -4, 7, 1, STEEL);
            let spark = (seconds * 2.0).fract();
            if spark < 0.4 {
                pen.glow(
                    draw,
                    w / 2 + (spark * 10.0) as i32,
                    -6 - (spark * 8.0) as i32,
                    1,
                    1,
                    GOLD,
                );
                pen.glow(
                    draw,
                    w / 2 - (spark * 8.0) as i32,
                    -5 - (spark * 6.0) as i32,
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
    let crystal = Color::new(0xb8, 0xc8, 0xf0, 255);
    let crystal_shade = Color::new(0x7a, 0x88, 0xc0, 255);
    let (glass, inner) = SIPHRA_GLASS[usize::from(look.index) % 3];
    if kind == AlienBuildingKind::Landmark {
        // The memory pylon: a tall crystal with motes of memory circling it.
        pen.rect(draw, -6, -4, 12, 4, crystal_shade);
        for dy in 0..h {
            let half = (3 - dy * 3 / h).max(0) + 1;
            pen.rect(
                draw,
                -half,
                -dy - 4,
                2 * half,
                1,
                if dy % 9 == 0 { CREAM } else { crystal },
            );
        }
        let pulse = 0.5 + 0.5 * (seconds * 1.4).sin();
        pen.glow(
            draw,
            -2,
            -h / 2,
            4,
            h / 3,
            alpha(inner, (80.0 + pulse * 120.0) as u8),
        );
        for i in 0..7 {
            let angle = seconds * 0.9 + i as f32 * 0.9;
            let x = (angle.cos() * 9.0).round() as i32;
            let y = -h / 2 - 10 + (angle.sin() * 3.0).round() as i32 + i * 4;
            pen.glow(draw, x, y, 1, 1, if i % 2 == 0 { CREAM } else { GOLD });
        }
        return;
    }
    // A lantern-pod hung from a crystal pylon, tethered over the ground.
    let pylon_x = -w / 2 + 2;
    pen.rect(draw, pylon_x - 1, -h, 3, h, crystal);
    pen.rect(draw, pylon_x, -h, 1, h, crystal_shade);
    pen.rect(draw, pylon_x - 1, -h - 3, 3, 3, CREAM);
    // The arm and the tether.
    pen.rect(draw, pylon_x, -h + 2, w / 2, 1, crystal_shade);
    let sway = ((seconds * 0.8 + f32::from(look.index)).sin() * 1.5).round() as i32;
    let pod_y = -h / 2 - 2;
    pen.rect(draw, sway / 2, -h + 2, 1, h / 2 - 8, alpha(CREAM, 160));
    // The pod: a glowing glass bulb.
    let rx = w / 2 - 3;
    let ry = h / 3;
    pen.glow(
        draw,
        sway - rx - 2,
        pod_y - ry - 2,
        2 * rx + 5,
        2 * ry + 5,
        alpha(glass, 30),
    );
    pen.ellipse(draw, sway, pod_y, rx, ry, alpha(glass, 170));
    let pulse = 0.5 + 0.5 * (seconds * 1.2 + f32::from(look.index)).sin();
    pen.ellipse(
        draw,
        sway,
        pod_y,
        rx - 3,
        ry - 3,
        alpha(inner, (100.0 + 100.0 * pulse) as u8),
    );
    pen.rect(
        draw,
        sway - rx + 2,
        pod_y - 1,
        2 * rx - 3,
        1,
        alpha(CREAM, 120),
    );
    // A slit door on its underside and the ground tether.
    pen.rect(draw, sway - 1, pod_y + ry - 2, 3, 2, alpha(NIGHT, 200));
    pen.rect(draw, sway, pod_y + ry, 1, -pod_y - ry, alpha(crystal, 150));
    match kind {
        AlienBuildingKind::TradeHall => {
            // Three small pods of goods hanging alongside.
            for i in 0..3 {
                let x = sway - rx + 3 + i * rx;
                pen.ellipse(draw, x, pod_y + ry + 4, 2, 2, alpha(GOLD, 220));
            }
        }
        AlienBuildingKind::ElderHouse => {
            pen.glow(draw, sway - 1, pod_y - ry - 5, 3, 3, alpha(GOLD, 230));
        }
        AlienBuildingKind::LexiconHouse => {
            for i in 0..3 {
                draw_glyph_art(
                    draw,
                    pen,
                    sway - 9 + i * 6,
                    pod_y - 2,
                    AlienSpecies::Siphra,
                    i as u8 * 4,
                    alpha(CREAM, 230),
                );
            }
        }
        AlienBuildingKind::Workshop => {
            // A half-grown pod on a second pylon.
            pen.rect(draw, w / 2 - 3, -h / 2, 2, h / 2, crystal);
            pen.ellipse(draw, w / 2 - 2, -h / 2 - 3, 3, 3, alpha(glass, 120));
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
        let sway = ((seconds * 2.0).sin() * 1.5).round() as i32;
        pen.rect(
            draw,
            w / 2 - 4,
            -height * 16 - 6,
            1,
            height * 16 + 6,
            STEEL_LIGHT,
        );
        pen.rect(draw, w / 2 - 3, -height * 16 - 6, 6 + sway, 4, GOLD);
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
            // A ring (a sung note) with a tick at one of eight places, sometimes a dot inside.
            let mut bits = [0b01110, 0b10001, 0b10001, 0b10001, 0b01110, 0];
            match index % 8 {
                0 => bits[5] = 0b00100,
                1 => bits[0] |= 0b10000,
                2 => bits[0] |= 0b00001,
                3 => bits[5] = 0b10001,
                4 => {
                    bits[2] = 0b10101;
                }
                5 => bits[5] = 0b01100,
                6 => bits[5] = 0b00110,
                _ => bits[0] = 0b11111,
            }
            if index >= 8 || roll.is_multiple_of(3) {
                bits[2] |= 0b00100;
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
