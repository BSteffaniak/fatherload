//! Placeholder villager sprites: a small procedural figure assembled from a character's
//! [`Appearance`] (build, hair, accessory, outfit) and seed, so every resident looks like
//! themselves outdoors and in their rooms. WP-PEOPLE replaces this with full sprite assembly
//! and animation; until then villagers stand still, breathe and blink.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use fatherload_simulation::people::{Appearance, Character};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;

const OUTFITS: [(Color, Color); 10] = [
    (TEAL, TEAL_DARK),
    (RED, MAROON),
    (OCEAN, DEEP_BLUE),
    (MOSS, PINE),
    (AMBER, COPPER),
    (VIOLET, PLUM),
    (LOAM, SOIL),
    (SLATE_LIGHT, SLATE),
    (ROSE, MAROON),
    (STEEL, STEEL_DARK),
];

const SKIN: [Color; 5] = [
    Color::new(0xf1, 0xc8, 0xa0, 255),
    Color::new(0xd9, 0xa0, 0x6c, 255),
    Color::new(0xb3, 0x77, 0x4a, 255),
    Color::new(0x8a, 0x55, 0x34, 255),
    Color::new(0x5e, 0x3a, 0x24, 255),
];

const fn hair_color(index: u8) -> Option<Color> {
    Some(match index {
        0 => INK,
        1 => COPPER,
        2 => SAND,
        3 => MIST,
        4 => LOAM,
        5 => BONE,
        6 => RUST,
        7 => SOIL_DARK,
        _ => return None,
    })
}

/// Where and how big to draw: art pixel `(0, 0)` is the figure's feet centre.
#[derive(Clone, Copy, Debug)]
pub struct PersonPen {
    pub x: f32,
    pub y: f32,
    /// Draw-space units per art pixel.
    pub pixel: f32,
}

impl PersonPen {
    fn rect(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        draw.draw_rectangle_rec(
            Rectangle::new(
                (x as f32).mul_add(self.pixel, self.x),
                (y as f32).mul_add(self.pixel, self.y),
                w as f32 * self.pixel,
                h as f32 * self.pixel,
            ),
            color,
        );
    }
}

/// Draws `character` standing at `pen` (feet centre), facing `facing` (-1 left, 1 right), at
/// `seconds` (breathing and blinking).
pub fn draw_person(
    draw: &mut impl RaylibDraw,
    pen: PersonPen,
    character: &Character,
    facing: i32,
    seconds: f32,
) {
    let look: Appearance = character.appearance;
    let seed = look.seed;
    let (wide, tall) = match look.build {
        2 | 3 | 5 => (7, 0),
        4 => (5, 2),
        _ => (5, 0),
    };
    let elder = character.age >= 60;
    let child = character.age < 16;
    let shrink = if child { 4 } else { 0 };
    let (shirt, trousers) = OUTFITS[usize::from(look.outfit) % OUTFITS.len()];
    let skin = SKIN[(seed % SKIN.len() as u64) as usize];
    let hair = hair_color(look.hair);
    // Breathing: the torso lifts a pixel now and then, each villager on their own beat.
    let phase = (seed >> 8) % 100;
    let breathe = i32::from(((seconds * 0.9 + phase as f32 / 37.0) as i32) % 3 == 0);
    let blink = ((seconds * 1.3 + phase as f32 / 13.0) % 4.0) < 0.12;
    let half = wide / 2;
    let legs = 5 - shrink / 2;
    let torso = 6 + tall - shrink / 2;
    let head = 5;
    let torso_top = -legs - torso + breathe;
    let head_top = torso_top - head;
    // Shadow.
    pen.rect(draw, -half - 1, 0, wide + 2, 1, with_alpha(INK, 90));
    // Legs and boots.
    pen.rect(draw, -half, -legs, wide, legs, INK);
    pen.rect(draw, -half + 1, -legs, half, legs - 1, trousers);
    pen.rect(draw, 1, -legs, wide - half - 2, legs - 1, trousers);
    pen.rect(draw, -half, -1, half, 1, SOIL_DARK);
    pen.rect(draw, 1, -1, wide - half - 1, 1, SOIL_DARK);
    // Torso with an outline and arms.
    pen.rect(draw, -half - 1, torso_top, wide + 2, torso + 1, INK);
    pen.rect(draw, -half, torso_top + 1, wide, torso - 1, shirt);
    pen.rect(draw, -half, torso_top + 1, wide, 1, with_alpha(BONE, 70));
    if elder {
        // A walking stick.
        pen.rect(
            draw,
            facing * (half + 2),
            torso_top + 2,
            1,
            torso + legs - 2,
            LOAM,
        );
    }
    // Hands.
    pen.rect(draw, -half - 1, torso_top + torso - 2, 1, 2, skin);
    pen.rect(
        draw,
        half + 1 - (1 - wide % 2),
        torso_top + torso - 2,
        1,
        2,
        skin,
    );
    // Head.
    let head_x = -2;
    pen.rect(draw, head_x - 1, head_top - 1, 6, head + 1, INK);
    pen.rect(draw, head_x, head_top, 4, head, skin);
    let eye_x = if facing < 0 { head_x } else { head_x + 2 };
    if !blink {
        pen.rect(draw, eye_x, head_top + 2, 1, 1, INK);
        pen.rect(draw, eye_x + 1, head_top + 2, 1, 1, INK);
    }
    if let Some(hair) = hair {
        pen.rect(draw, head_x, head_top - 1, 4, 2, hair);
        let back = if facing < 0 { head_x + 3 } else { head_x };
        pen.rect(draw, back, head_top, 1, 3, hair);
        if character.age > 20 && seed.is_multiple_of(3) && !elder {
            // A beard or long hair.
            pen.rect(draw, head_x, head_top + 4, 4, 1, hair);
        }
    }
    // Accessory.
    match look.accessory {
        0 => {
            pen.rect(draw, head_x, head_top + 1, 4, 1, GOLD);
            pen.rect(draw, eye_x, head_top + 1, 2, 2, AMBER);
        }
        1 => {
            pen.rect(draw, head_x - 2, head_top - 1, 8, 1, SOIL);
            pen.rect(draw, head_x - 1, head_top - 3, 6, 2, LOAM);
        }
        2 => pen.rect(draw, -half, torso_top + 1, wide, 2, RED),
        3 => {
            pen.rect(draw, head_x - 1, head_top - 2, 6, 2, GOLD);
            pen.rect(draw, head_x + 1, head_top - 3, 2, 1, CREAM);
        }
        4 => pen.rect(draw, eye_x - 1, head_top + 2, 3, 1, STEEL_LIGHT),
        5 => pen.rect(draw, -half, torso_top + torso - 2, wide, 1, LOAM),
        6 => pen.rect(draw, -half - 1, torso_top + 1, wide + 2, 2, LAVENDER),
        7 => pen.rect(
            draw,
            head_x + 3 * i32::from(facing < 0),
            head_top + 3,
            1,
            1,
            STEEL_LIGHT,
        ),
        8 => {
            pen.rect(draw, head_x - 1, head_top - 2, 5, 2, MOSS);
            pen.rect(draw, head_x + 4, head_top - 4, 1, 3, RED);
        }
        _ => pen.rect(draw, -half, torso_top + 2, wide, torso - 2, SOIL),
    }
}
