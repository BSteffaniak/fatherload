//! Villager sprites (WP-PEOPLE): a small procedural figure assembled from a character's
//! [`Appearance`] seed, culture and trade, so every person looks like themselves on the street,
//! in their rooms and in the talk panel's portrait.
//!
//! Layers, back to front: shadow; legs (trousers, a skirt or kilt, or the hem of a robe) with a
//! four-frame walk; boots; the torso in the culture's cloth (frontier plaid, highland tartan,
//! mesa poncho bands, riverside linen, scrapper patches, deepfolk hoods) with the trade's
//! garment over it (a smith's apron, a miner's braces, a trader's waistcoat, a scholar's long
//! coat, a healer's robe, a courier's satchel, a warden's badge); swinging arms and hands; the
//! head with skin, eyes that blink, a mouth that moves while they talk, the hair style and
//! colour; and the accessory (goggles, hats, a neckerchief, a lamp helmet, spectacles, a tool
//! belt, a scarf, earrings, a feathered cap, a leather apron). Elders lean on a stick.
//!
//! Everything is a pure function of the character and the pose, at any pixel scale: the world
//! draws people at two screen pixels per art pixel, the talk panel draws the portrait at eight.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use fatherload_simulation::people::{Appearance, Character, Culture, Role};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;

const SKIN: [Color; 6] = [
    Color::new(0xf3, 0xcf, 0xa8, 255),
    Color::new(0xe0, 0xac, 0x7c, 255),
    Color::new(0xc4, 0x8a, 0x5c, 255),
    Color::new(0x9c, 0x66, 0x40, 255),
    Color::new(0x74, 0x4a, 0x2e, 255),
    Color::new(0x56, 0x36, 0x22, 255),
];

/// Deepfolk live under lamplight: paler, cooler skin.
const DEEP_SKIN: [Color; 3] = [
    Color::new(0xe6, 0xd8, 0xd0, 255),
    Color::new(0xcf, 0xbd, 0xb6, 255),
    Color::new(0xa8, 0x96, 0x94, 255),
];

/// A culture's cloth: `(main, shade)` pairs picked by the outfit index.
const fn cloth(culture: Culture) -> &'static [(Color, Color)] {
    match culture {
        Culture::Frontier => &[
            (OCEAN, DEEP_BLUE),
            (RED, MAROON),
            (LOAM, SOIL),
            (SAND, CLAY),
            (TEAL, TEAL_DARK),
            (SLATE_LIGHT, SLATE),
        ],
        Culture::Highland => &[
            (MOSS, PINE),
            (SLATE_LIGHT, SLATE),
            (MAROON, PLUM),
            (STONE_LIGHT, STONE),
            (TEAL_DARK, DEEP_BLUE),
        ],
        Culture::Mesa => &[
            (ORANGE, RUST),
            (AMBER, COPPER),
            (CLAY, LOAM),
            (ROSE, MAROON),
            (CREAM, SAND),
        ],
        Culture::Riverside => &[
            (LEAF, MOSS),
            (CREAM, SAND),
            (TEAL, TEAL_DARK),
            (MINT, TEAL),
            (SKY, OCEAN),
        ],
        Culture::Scrapper => &[
            (STEEL, STEEL_DARK),
            (RUST, SOIL),
            (SLATE, SLATE_DARK),
            (COPPER, RUST),
            (STONE, STONE_DARK),
        ],
        Culture::Deepfolk => &[
            (VIOLET, PLUM),
            (SLATE, NIGHT),
            (LAVENDER, VIOLET),
            (TEAL_DARK, NIGHT),
            (STEEL_DARK, NIGHT),
        ],
        // Sky cultures (WP-SKY): flight leathers and brass for the aeronauts, pale robes and
        // sky blues for the Cloudborn.
        Culture::Aeronaut => &[
            (LOAM, SOIL),
            (CLAY, LOAM),
            (OCEAN, DEEP_BLUE),
            (CREAM, SAND),
            (RED, MAROON),
        ],
        Culture::Cloudborn => &[
            (BONE, MIST),
            (SKY_LIGHT, SKY),
            (MIST, FOG),
            (ICE, SKY_LIGHT),
            (CREAM, SAND),
        ],
        // The Moon's colonists (Wave 6): station jumpsuits in white, orange and slate.
        Culture::Lunar => &[
            (BONE, STONE_LIGHT),
            (ORANGE, RUST),
            (SLATE_LIGHT, SLATE),
            (STEEL_LIGHT, STEEL),
            (TEAL, TEAL_DARK),
        ],
    }
}

/// A culture's pattern accent (plaid lines, tartan, poncho bands, patches).
const fn accent(culture: Culture, seed: u64) -> Color {
    let pick = (seed >> 13) % 3;
    match culture {
        Culture::Frontier => [RED, CREAM, GOLD][pick as usize],
        Culture::Highland => [RED, GOLD, SKY_LIGHT][pick as usize],
        Culture::Mesa => [TEAL, GOLD, CREAM][pick as usize],
        Culture::Riverside => [GOLD, ROSE, CREAM][pick as usize],
        Culture::Scrapper => [ORANGE, AMBER, STEEL_LIGHT][pick as usize],
        Culture::Deepfolk => [MINT, GOLD, LAVENDER][pick as usize],
        Culture::Aeronaut => [COPPER, GOLD, AMBER][pick as usize],
        Culture::Cloudborn => [SKY, GOLD, TEAL][pick as usize],
        Culture::Lunar => [ORANGE, SKY_LIGHT, GOLD][pick as usize],
    }
}

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

    fn px(self, draw: &mut impl RaylibDraw, x: i32, y: i32, color: Color) {
        self.rect(draw, x, y, 1, 1, color);
    }
}

/// How a person is posed this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    /// -1 facing left, 1 facing right.
    pub facing: i32,
    /// Walking: the distance walked so far (world pixels) picks the step frame.
    pub stride: Option<f32>,
    /// Talking: the mouth moves.
    pub talking: bool,
}

/// What the figure's lower half looks like.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Legs {
    Trousers,
    /// A skirt or kilt to the knee.
    Skirt,
    /// A robe to the ankle.
    Robe,
}

/// Draws `character` at `pen` (feet centre) in `pose` at `seconds`.
pub fn draw_person_posed(
    draw: &mut impl RaylibDraw,
    pen: PersonPen,
    character: &Character,
    pose: Pose,
    seconds: f32,
) {
    let look: Appearance = character.appearance;
    let seed = look.seed;
    let culture = character.culture;
    let role = character.role;
    let facing = if pose.facing < 0 { -1 } else { 1 };
    let (wide, tall) = match look.build {
        2 | 3 => (7, 0),
        5 => (7, -1),
        4 => (5, 2),
        1 => (5, 1),
        _ => (5, 0),
    };
    let elder = character.age >= 62;
    let young = character.age < 18;
    let shrink = i32::from(young) * 2;
    let palette = cloth(culture);
    let (shirt, shirt_shade) = palette[usize::from(look.outfit) % palette.len()];
    let (trousers, trousers_shade) =
        palette[(usize::from(look.outfit) + 2 + seed as usize % 3) % palette.len()];
    let trim = accent(culture, seed);
    let skin = if culture == Culture::Deepfolk {
        DEEP_SKIN[(seed % DEEP_SKIN.len() as u64) as usize]
    } else {
        SKIN[(seed % SKIN.len() as u64) as usize]
    };
    let hair = hair_color(look.hair);
    let legs_kind = match role {
        Role::ShrineKeeper | Role::Healer | Role::Hermit => Legs::Robe,
        _ if culture == Culture::Highland && seed.is_multiple_of(3) => Legs::Skirt,
        _ if matches!(culture, Culture::Mesa | Culture::Riverside)
            && (seed >> 5).is_multiple_of(3) =>
        {
            Legs::Skirt
        }
        _ => Legs::Trousers,
    };
    // Motion: a four-frame step while walking, else breathing.
    let phase = (seed >> 8) % 100;
    let step = pose
        .stride
        .map(|walked| ((walked.abs() / 7.0) as i32).rem_euclid(4));
    let bob = match step {
        Some(1 | 3) => -1,
        Some(_) => 0,
        None => -i32::from(((seconds * 0.9 + phase as f32 / 37.0) as i32) % 3 == 0),
    };
    let blink = ((seconds * 1.3 + phase as f32 / 13.0) % 4.0) < 0.12;
    let half = wide / 2;
    let legs = 6 - shrink / 2;
    let torso = 7 + tall - shrink / 2;
    let head = 5;
    let torso_top = -legs - torso + bob;
    let head_top = torso_top - head;
    let right = wide - half;

    // Shadow.
    pen.rect(draw, -half - 1, 0, wide + 2, 1, with_alpha(INK, 80));

    // Legs and boots.
    let (left_leg, right_leg) = match step {
        Some(0) => (-1, 1),
        Some(2) => (1, -1),
        _ => (0, 0),
    };
    let boot = if culture == Culture::Mesa {
        LOAM
    } else {
        SOIL_DARK
    };
    match legs_kind {
        Legs::Trousers => {
            pen.rect(draw, -half + left_leg, -legs, half, legs, INK);
            pen.rect(draw, right_leg + 1, -legs, right - 1, legs, INK);
            pen.rect(
                draw,
                -half + 1 + left_leg,
                -legs,
                half - 1,
                legs - 1,
                trousers,
            );
            pen.rect(
                draw,
                1 + right_leg,
                -legs,
                right - 2,
                legs - 1,
                trousers_shade,
            );
            pen.rect(draw, -half + left_leg, -1, half, 1, boot);
            pen.rect(draw, 1 + right_leg, -1, right - 1, 1, boot);
        }
        Legs::Skirt => {
            pen.rect(draw, -half - 1, -legs, wide + 2, legs - 2, INK);
            pen.rect(draw, -half, -legs, wide, legs - 3, trousers);
            if culture == Culture::Highland {
                // Tartan: a cross-hatch of the accent.
                for y in 0..legs - 3 {
                    pen.px(draw, -half + (y * 2) % wide, -legs + y, trim);
                }
                pen.rect(draw, -half, -legs + 2, wide, 1, trousers_shade);
            } else {
                pen.rect(draw, -half, -4, wide, 1, trim);
            }
            pen.rect(draw, -half + 1 + left_leg, -2, 1, 2, skin);
            pen.rect(draw, right - 2 + right_leg, -2, 1, 2, skin);
            pen.px(draw, -half + 1 + left_leg, -1, boot);
            pen.px(draw, right - 2 + right_leg, -1, boot);
        }
        Legs::Robe => {
            pen.rect(draw, -half - 1, -legs, wide + 2, legs, INK);
            pen.rect(draw, -half, -legs, wide, legs - 1, shirt);
            pen.rect(draw, -half, -2, wide, 1, shirt_shade);
            pen.rect(draw, -1 + left_leg, -1, 1, 1, boot);
            pen.rect(draw, 1 + right_leg, -1, 1, 1, boot);
        }
    }

    // Torso with an outline.
    let coat = matches!(role, Role::Scholar | Role::Cartographer | Role::Warden);
    let body_bottom = if coat { torso + 3 } else { torso + 1 };
    pen.rect(draw, -half - 1, torso_top, wide + 2, body_bottom, INK);
    pen.rect(draw, -half, torso_top + 1, wide, body_bottom - 2, shirt);
    pen.rect(draw, -half, torso_top + 1, wide, 1, with_alpha(BONE, 60));
    // Culture cloth.
    match culture {
        Culture::Frontier => {
            // Plaid: a check of shade and accent.
            for y in (torso_top + 2..torso_top + torso).step_by(2) {
                pen.rect(draw, -half, y, wide, 1, with_alpha(shirt_shade, 200));
            }
            for x in (-half + 1..-half + wide).step_by(3) {
                pen.rect(draw, x, torso_top + 1, 1, torso - 1, with_alpha(trim, 120));
            }
        }
        Culture::Highland => {
            // A wool scarf.
            pen.rect(draw, -half, torso_top + 1, wide, 2, trim);
            pen.rect(draw, facing * (half - 1), torso_top + 1, 1, 4, trim);
        }
        Culture::Mesa => {
            // Poncho bands with a zigzag.
            pen.rect(draw, -half, torso_top + 3, wide, 1, trim);
            for x in -half..-half + wide {
                pen.px(
                    draw,
                    x,
                    torso_top + 4 + (x.rem_euclid(2)),
                    with_alpha(trim, 180),
                );
            }
            pen.rect(
                draw,
                -half - 1,
                torso_top + torso - 2,
                wide + 2,
                1,
                shirt_shade,
            );
        }
        Culture::Riverside => {
            // A sash.
            for y in 0..torso - 1 {
                pen.px(
                    draw,
                    -half + (y * wide / torso.max(1)),
                    torso_top + 1 + y,
                    trim,
                );
            }
        }
        Culture::Scrapper => {
            // Patches and rivets.
            let patch_x = -half + (seed % wide.max(1) as u64) as i32;
            pen.rect(
                draw,
                patch_x.min(half - 2),
                torso_top + 3,
                2,
                2,
                STEEL_LIGHT,
            );
            pen.px(draw, -half + 1, torso_top + torso - 2, trim);
            pen.px(draw, half - 1, torso_top + 2, trim);
        }
        Culture::Deepfolk => {
            // A hood falling over the shoulders.
            pen.rect(draw, -half, torso_top + 1, wide, 1, shirt_shade);
            pen.rect(draw, -half - 1, torso_top, 1, 3, shirt_shade);
            pen.rect(draw, right, torso_top, 1, 3, shirt_shade);
        }
        Culture::Aeronaut => {
            // A flight jacket: a fleece collar and a brass buckle.
            pen.rect(draw, -half, torso_top + 1, wide, 1, CREAM);
            pen.px(draw, 0, torso_top + torso - 2, trim);
        }
        Culture::Cloudborn => {
            // A robe's sash, low on the waist.
            pen.rect(draw, -half, torso_top + torso - 3, wide, 1, trim);
            pen.px(draw, facing * (half - 1), torso_top + torso - 2, trim);
        }
        Culture::Lunar => {
            // A jumpsuit: a zip down the front, a mission patch and a ring collar.
            pen.rect(draw, -half, torso_top + 1, wide, 1, STEEL_LIGHT);
            pen.rect(draw, 0, torso_top + 2, 1, torso - 2, with_alpha(INK, 90));
            pen.px(draw, -facing * (half - 1), torso_top + 3, trim);
        }
    }
    // The trade's garment.
    match role {
        Role::Smith | Role::Baker | Role::Brewer => {
            let apron = if role == Role::Smith { SOIL } else { CREAM };
            pen.rect(draw, -half + 1, torso_top + 3, wide - 2, torso - 2, apron);
            pen.rect(
                draw,
                -half + 1,
                torso_top + 3,
                wide - 2,
                1,
                with_alpha(INK, 60),
            );
        }
        Role::Prospector | Role::RetiredMiner | Role::Mechanic | Role::Farmer => {
            // Braces.
            pen.rect(draw, -half + 1, torso_top + 1, 1, torso - 1, trousers_shade);
            pen.rect(draw, half - 1, torso_top + 1, 1, torso - 1, trousers_shade);
            pen.rect(draw, -half, torso_top + torso - 2, wide, 2, trousers);
        }
        Role::Trader | Role::Innkeeper => {
            // A waistcoat with buttons.
            pen.rect(draw, -half, torso_top + 2, 2, torso - 2, shirt_shade);
            pen.rect(draw, half - 1, torso_top + 2, 2, torso - 2, shirt_shade);
            pen.px(draw, 0, torso_top + 3, GOLD);
            pen.px(draw, 0, torso_top + 5, GOLD);
        }
        Role::Scholar | Role::Cartographer => {
            pen.rect(draw, -half, torso_top + 1, 1, body_bottom - 2, shirt_shade);
            pen.rect(draw, half, torso_top + 1, 1, body_bottom - 2, shirt_shade);
            if role == Role::Cartographer {
                // A rolled map under the arm.
                pen.rect(draw, -facing * (half + 1), torso_top + 3, 2, 4, CREAM);
            }
        }
        Role::Warden => {
            pen.px(draw, facing * (half - 1), torso_top + 3, GOLD);
            pen.rect(draw, -half, torso_top + torso - 1, wide, 1, SOIL_DARK);
        }
        Role::Courier | Role::Storyteller => {
            // A satchel strap across the chest and the bag at the hip.
            for y in 0..torso - 1 {
                pen.px(
                    draw,
                    half - (y * wide / torso.max(1)),
                    torso_top + 1 + y,
                    LOAM,
                );
            }
            pen.rect(draw, -facing * half - 1, torso_top + torso - 2, 3, 3, SOIL);
        }
        Role::Tinkerer | Role::Apprentice => {
            pen.rect(draw, -half, torso_top + torso - 2, wide, 1, LOAM);
            pen.px(draw, -half + 1, torso_top + torso - 1, STEEL_LIGHT);
            pen.px(draw, half - 1, torso_top + torso - 1, COPPER);
        }
        Role::ShrineKeeper | Role::Healer | Role::Hermit | Role::Monk => {
            pen.rect(draw, -1, torso_top + 2, 2, torso - 1, shirt_shade);
            if role == Role::Healer {
                pen.rect(draw, -1, torso_top + 3, 2, 1, CREAM);
            }
        }
        Role::Aeronaut | Role::StormChaser => {
            // Goggles pushed up on the brow... and a harness strap.
            pen.rect(draw, -half + 1, torso_top + 1, 1, torso - 1, SOIL);
            pen.px(draw, -half + 1, torso_top + 3, COPPER);
        }
        Role::Smuggler => {
            // A long coat's lapels.
            pen.rect(draw, -half, torso_top + 1, 1, body_bottom - 2, shirt_shade);
            pen.rect(
                draw,
                half - 1,
                torso_top + 1,
                1,
                body_bottom - 2,
                shirt_shade,
            );
        }
        Role::Hydroponist => {
            // A green apron with a seed pocket.
            pen.rect(draw, -half + 1, torso_top + 3, wide - 2, torso - 2, LEAF);
            pen.px(draw, 0, torso_top + 4, MOSS);
        }
        Role::DomeTech | Role::Quartermaster => {
            // A tool belt (the tech) or a clipboard strap (the stores).
            pen.rect(draw, -half, torso_top + torso - 2, wide, 1, SOIL);
            let tag = if role == Role::DomeTech { AMBER } else { CREAM };
            pen.px(draw, facing * (half - 1), torso_top + torso - 2, tag);
        }
        Role::Selenographer => {
            // A chart case slung at the hip.
            pen.rect(draw, -facing * (half + 1), torso_top + 3, 2, 4, SLATE_LIGHT);
        }
    }

    // Arms swing opposite the legs; hands.
    let swing = match step {
        Some(0) => 1,
        Some(2) => -1,
        _ => 0,
    };
    let arm_top = torso_top + 2;
    pen.rect(draw, -half - 1, arm_top - swing, 1, torso - 3, shirt_shade);
    pen.rect(draw, right, arm_top + swing, 1, torso - 3, shirt_shade);
    pen.rect(draw, -half - 1, arm_top + torso - 3 - swing, 1, 2, skin);
    pen.rect(draw, right, arm_top + torso - 3 + swing, 1, 2, skin);
    if elder {
        // A walking stick.
        pen.rect(
            draw,
            facing * (half + 2),
            torso_top + 3,
            1,
            -torso_top - 3,
            LOAM,
        );
        pen.px(draw, facing * (half + 2), torso_top + 2, SOIL);
    }

    // Head.
    let head_x = -2;
    pen.rect(draw, head_x - 1, head_top - 1, 6, head + 1, INK);
    pen.rect(draw, head_x, head_top, 4, head, skin);
    let eye_x = if facing < 0 { head_x } else { head_x + 2 };
    if blink {
        pen.rect(draw, eye_x, head_top + 2, 2, 1, with_alpha(INK, 120));
    } else {
        pen.px(draw, eye_x, head_top + 2, INK);
        pen.px(draw, eye_x + 1, head_top + 2, INK);
    }
    // Mouth: a line, opening and closing while they talk.
    let open = pose.talking && ((seconds * 9.0 + phase as f32) as i32) % 3 != 0;
    let mouth_x = if facing < 0 { head_x } else { head_x + 2 };
    if open {
        pen.rect(draw, mouth_x, head_top + 4, 2, 1, MAROON);
    } else {
        pen.px(
            draw,
            mouth_x + i32::from(facing > 0),
            head_top + 4,
            with_alpha(INK, 110),
        );
    }
    if let Some(hair) = hair {
        let style = (seed >> 3) % 5;
        pen.rect(draw, head_x, head_top - 1, 4, 2, hair);
        let back = if facing < 0 { head_x + 3 } else { head_x };
        match style {
            // Long hair down the back.
            0 => pen.rect(draw, back, head_top, 1, 5, hair),
            // A bun.
            1 => pen.rect(draw, back - facing, head_top - 1, 2, 2, hair),
            // Short and tidy.
            2 => pen.rect(draw, back, head_top, 1, 2, hair),
            // A fringe.
            3 => {
                pen.rect(draw, back, head_top, 1, 3, hair);
                pen.px(draw, eye_x, head_top, hair);
            }
            // Wild.
            _ => {
                pen.rect(draw, head_x - 1, head_top - 2, 6, 2, hair);
                pen.rect(draw, back, head_top, 1, 3, hair);
            }
        }
        if character.age > 24 && seed.is_multiple_of(4) && !young {
            // A beard.
            pen.rect(draw, head_x, head_top + 4, 4, 1, hair);
            pen.px(
                draw,
                mouth_x + i32::from(facing > 0),
                head_top + 4,
                with_alpha(INK, 110),
            );
        }
    }
    if culture == Culture::Deepfolk && look.accessory != 3 {
        // The hood up over the head.
        pen.rect(draw, head_x - 1, head_top - 2, 6, 2, shirt_shade);
        let back = if facing < 0 { head_x + 4 } else { head_x - 1 };
        pen.rect(draw, back, head_top - 1, 1, 4, shirt_shade);
    }
    if matches!(role, Role::Farmer) && look.accessory != 1 {
        // A straw hat.
        pen.rect(draw, head_x - 2, head_top - 1, 8, 1, SAND);
        pen.rect(draw, head_x, head_top - 3, 4, 2, GOLD);
    }

    // Accessory.
    match look.accessory {
        0 => {
            pen.rect(draw, head_x, head_top + 1, 4, 1, GOLD);
            pen.rect(draw, eye_x, head_top + 1, 2, 2, AMBER);
        }
        1 => {
            let brim = if culture == Culture::Mesa { SAND } else { SOIL };
            pen.rect(draw, head_x - 2, head_top - 1, 8, 1, brim);
            pen.rect(draw, head_x - 1, head_top - 3, 6, 2, LOAM);
            pen.rect(draw, head_x - 1, head_top - 2, 6, 1, trim);
        }
        2 => pen.rect(draw, -half, torso_top + 1, wide, 2, RED),
        3 => {
            pen.rect(draw, head_x - 1, head_top - 2, 6, 2, GOLD);
            pen.rect(draw, head_x + 1 + facing, head_top - 3, 2, 1, CREAM);
            // The lamp's glow.
            pen.rect(
                draw,
                head_x + 1 + facing * 3,
                head_top - 4,
                3,
                3,
                with_alpha(GOLD, 70),
            );
        }
        4 => pen.rect(draw, eye_x - 1, head_top + 2, 3, 1, STEEL_LIGHT),
        5 => pen.rect(draw, -half, torso_top + torso - 2, wide, 1, LOAM),
        6 => pen.rect(draw, -half - 1, torso_top + 1, wide + 2, 2, LAVENDER),
        7 => pen.px(
            draw,
            head_x + 3 * i32::from(facing < 0),
            head_top + 3,
            STEEL_LIGHT,
        ),
        8 => {
            pen.rect(draw, head_x - 1, head_top - 2, 5, 2, MOSS);
            pen.rect(
                draw,
                head_x + 4 - 5 * i32::from(facing < 0),
                head_top - 4,
                1,
                3,
                RED,
            );
        }
        _ => pen.rect(draw, -half, torso_top + 3, wide, torso - 3, SOIL),
    }
}

/// The figure's height in art pixels (for name tags and the portrait frame).
#[must_use]
pub const fn person_height(character: &Character) -> i32 {
    let tall = match character.appearance.build {
        4 => 2,
        1 => 1,
        5 => -1,
        _ => 0,
    };
    let shrink = if character.age < 18 { 2 } else { 0 };
    6 + 7 + tall - shrink + 5
}

/// Art pixels a side of a talk-panel portrait ([`draw_portrait`]).
pub const PORTRAIT_SIZE: i32 = 32;

/// Draws `character`'s head-and-shoulders portrait for the talk panel: a [`PORTRAIT_SIZE`]
/// square of art pixels with its top-left at `pen` (`pen.pixel` draw units each), the same
/// skin, hair, cloth, trade and accessory as their street figure, in more detail: eyes that
/// blink, brows, a nose, a mouth that moves while they talk, ears, beards, wrinkles for elders.
pub fn draw_portrait(
    draw: &mut impl RaylibDraw,
    pen: PersonPen,
    character: &Character,
    talking: bool,
    seconds: f32,
) {
    let look = character.appearance;
    let seed = look.seed;
    let culture = character.culture;
    let role = character.role;
    let palette = cloth(culture);
    let (shirt, shirt_shade) = palette[usize::from(look.outfit) % palette.len()];
    let trim = accent(culture, seed);
    let skin = if culture == Culture::Deepfolk {
        DEEP_SKIN[(seed % DEEP_SKIN.len() as u64) as usize]
    } else {
        SKIN[(seed % SKIN.len() as u64) as usize]
    };
    let skin_shade = Color::new(
        (u16::from(skin.r) * 4 / 5) as u8,
        (u16::from(skin.g) * 4 / 5) as u8,
        (u16::from(skin.b) * 4 / 5) as u8,
        255,
    );
    let hair = hair_color(look.hair);
    let elder = character.age >= 62;
    let phase = (seed >> 8) % 100;
    let blink = ((seconds * 1.3 + phase as f32 / 13.0) % 4.0) < 0.12;
    let size = PORTRAIT_SIZE;

    // Backdrop: the culture's cloth, darkened, with a soft glow behind the head.
    pen.rect(draw, 0, 0, size, size, with_alpha(shirt_shade, 120));
    pen.rect(draw, 4, 2, size - 8, size - 6, with_alpha(BONE, 26));

    // Shoulders and chest.
    let wide = matches!(look.build, 2 | 3 | 5);
    let (sx, sw) = if wide { (2, 28) } else { (4, 24) };
    pen.rect(draw, sx - 1, 22, sw + 2, 10, INK);
    pen.rect(draw, sx, 23, sw, 9, shirt);
    pen.rect(draw, sx, 23, sw, 1, with_alpha(BONE, 70));
    // Collar.
    pen.rect(draw, 12, 23, 8, 2, shirt_shade);
    match culture {
        Culture::Frontier => {
            for x in (sx + 2..sx + sw).step_by(4) {
                pen.rect(draw, x, 24, 1, 8, with_alpha(trim, 140));
            }
            pen.rect(draw, sx, 28, sw, 1, with_alpha(shirt_shade, 220));
        }
        Culture::Highland => {
            pen.rect(draw, 9, 22, 14, 3, trim);
            pen.rect(draw, 20, 24, 3, 6, trim);
        }
        Culture::Mesa => {
            pen.rect(draw, sx, 27, sw, 2, trim);
            for x in (sx..sx + sw).step_by(2) {
                pen.px(draw, x, 29, with_alpha(trim, 200));
            }
        }
        Culture::Riverside => {
            for step in 0..8 {
                pen.rect(draw, sx + 3 + step * 2, 24 + step, 2, 1, trim);
            }
        }
        Culture::Scrapper => {
            pen.rect(draw, sx + 3, 26, 4, 3, STEEL_LIGHT);
            pen.px(draw, sx + 3, 26, INK);
            pen.px(draw, sx + sw - 4, 25, trim);
            pen.px(draw, sx + sw - 6, 29, trim);
        }
        Culture::Deepfolk => {
            pen.rect(draw, sx, 23, sw, 2, shirt_shade);
        }
        Culture::Aeronaut => {
            pen.rect(draw, sx, 23, sw, 2, CREAM);
            pen.rect(draw, 15, 29, 3, 2, trim);
        }
        Culture::Cloudborn => {
            pen.rect(draw, sx, 29, sw, 2, trim);
        }
        Culture::Lunar => {
            pen.rect(draw, sx, 23, sw, 2, STEEL_LIGHT);
            pen.rect(draw, sx + sw - 7, 26, 3, 3, trim);
            pen.rect(draw, 15, 25, 1, 7, with_alpha(INK, 90));
        }
    }
    match role {
        Role::Smith | Role::Baker | Role::Brewer => {
            let apron = if role == Role::Smith { SOIL } else { CREAM };
            pen.rect(draw, 10, 26, 12, 6, apron);
            pen.rect(draw, 11, 23, 1, 3, apron);
            pen.rect(draw, 20, 23, 1, 3, apron);
        }
        Role::Prospector | Role::RetiredMiner | Role::Mechanic | Role::Farmer => {
            pen.rect(draw, 9, 23, 2, 9, LOAM);
            pen.rect(draw, 21, 23, 2, 9, LOAM);
            pen.px(draw, 9, 28, GOLD);
            pen.px(draw, 22, 28, GOLD);
        }
        Role::Trader | Role::Innkeeper => {
            pen.rect(draw, sx + 2, 24, 6, 8, shirt_shade);
            pen.rect(draw, sx + sw - 8, 24, 6, 8, shirt_shade);
            pen.px(draw, 16, 27, GOLD);
            pen.px(draw, 16, 30, GOLD);
        }
        Role::Scholar | Role::Cartographer => {
            pen.rect(draw, 13, 24, 6, 8, CREAM);
            pen.rect(draw, 15, 25, 2, 7, MAROON);
        }
        Role::Warden => {
            pen.rect(draw, sx + sw - 7, 26, 3, 3, GOLD);
            pen.px(draw, sx + sw - 6, 27, AMBER);
        }
        Role::Courier | Role::Storyteller => {
            for step in 0..9 {
                pen.rect(draw, sx + sw - 4 - step * 2, 23 + step, 2, 1, LOAM);
            }
        }
        Role::Tinkerer | Role::Apprentice => {
            pen.rect(draw, sx + 3, 27, 2, 4, STEEL_LIGHT);
            pen.rect(draw, sx + sw - 5, 26, 2, 5, COPPER);
        }
        Role::Aeronaut | Role::StormChaser => {
            pen.rect(draw, 9, 23, 2, 9, SOIL);
            pen.rect(draw, 9, 27, 2, 2, COPPER);
        }
        Role::Smuggler => {
            pen.rect(draw, sx + 1, 24, 3, 8, shirt_shade);
            pen.rect(draw, sx + sw - 4, 24, 3, 8, shirt_shade);
        }
        Role::ShrineKeeper | Role::Healer | Role::Hermit | Role::Monk => {
            pen.rect(draw, 14, 24, 4, 8, shirt_shade);
            if role == Role::Healer {
                pen.rect(draw, 15, 26, 2, 4, CREAM);
                pen.rect(draw, 14, 27, 4, 2, CREAM);
            }
        }
        Role::Hydroponist => {
            pen.rect(draw, 10, 26, 12, 6, LEAF);
            pen.rect(draw, 11, 23, 1, 3, LEAF);
            pen.rect(draw, 20, 23, 1, 3, LEAF);
        }
        Role::DomeTech | Role::Quartermaster => {
            pen.rect(draw, sx, 30, sw, 2, SOIL);
            let tag = if role == Role::DomeTech { AMBER } else { CREAM };
            pen.rect(draw, sx + 3, 26, 3, 3, tag);
        }
        Role::Selenographer => {
            pen.rect(draw, 13, 24, 6, 8, SLATE_LIGHT);
            pen.rect(draw, 15, 25, 2, 7, SKY_LIGHT);
        }
    }

    // Neck.
    pen.rect(draw, 13, 19, 6, 5, skin_shade);
    // Head: an outlined rounded block.
    let (hx, hy, hw, hh) = (9, 5, 14, 16);
    pen.rect(draw, hx - 1, hy, hw + 2, hh, INK);
    pen.rect(draw, hx, hy - 1, hw, hh + 2, INK);
    pen.rect(draw, hx, hy, hw, hh, skin);
    pen.rect(draw, hx, hy + hh - 3, hw, 3, skin_shade);
    pen.rect(draw, hx + 2, hy + hh - 2, hw - 4, 2, skin);
    // Ears.
    pen.rect(draw, hx - 2, 12, 2, 4, INK);
    pen.rect(draw, hx - 1, 13, 1, 2, skin_shade);
    pen.rect(draw, hx + hw, 12, 2, 4, INK);
    pen.rect(draw, hx + hw, 13, 1, 2, skin_shade);
    if look.accessory == 7 {
        pen.px(draw, hx - 1, 16, STEEL_LIGHT);
        pen.px(draw, hx + hw, 16, STEEL_LIGHT);
    }
    // Eyes and brows.
    let brow = hair.unwrap_or(skin_shade);
    if blink {
        pen.rect(draw, 11, 13, 3, 1, INK);
        pen.rect(draw, 18, 13, 3, 1, INK);
    } else {
        pen.rect(draw, 11, 12, 3, 2, CREAM);
        pen.rect(draw, 18, 12, 3, 2, CREAM);
        let look_x = i32::from((seed >> 17) % 2 == 1);
        pen.rect(draw, 12 + look_x, 12, 1, 2, INK);
        pen.rect(draw, 19 + look_x, 12, 1, 2, INK);
    }
    pen.rect(draw, 11, 10, 3, 1, brow);
    pen.rect(draw, 18, 10, 3, 1, brow);
    if elder {
        pen.px(draw, 10, 14, skin_shade);
        pen.px(draw, 21, 14, skin_shade);
        pen.rect(draw, 13, 8, 6, 1, skin_shade);
    }
    // Nose and mouth.
    pen.rect(draw, 15, 13, 2, 3, skin_shade);
    pen.px(draw, 15, 16, INK);
    let open = talking && ((seconds * 9.0 + phase as f32) as i32) % 3 != 0;
    if open {
        pen.rect(draw, 14, 18, 4, 2, MAROON);
        pen.rect(draw, 14, 18, 4, 1, INK);
    } else {
        pen.rect(draw, 14, 18, 4, 1, with_alpha(INK, 170));
    }

    // Hair.
    if let Some(hair) = hair {
        let style = (seed >> 3) % 5;
        pen.rect(draw, hx - 1, hy - 2, hw + 2, 4, hair);
        pen.rect(draw, hx - 1, hy + 2, 2, 4, hair);
        pen.rect(draw, hx + hw - 1, hy + 2, 2, 4, hair);
        match style {
            0 => {
                pen.rect(draw, hx - 2, hy + 2, 2, 14, hair);
                pen.rect(draw, hx + hw, hy + 2, 2, 14, hair);
            }
            1 => pen.rect(draw, 13, 0, 6, 3, hair),
            2 => {}
            3 => pen.rect(draw, hx + 1, hy + 2, 7, 2, hair),
            _ => {
                pen.rect(draw, hx - 2, hy - 3, hw + 4, 3, hair);
                pen.px(draw, hx - 3, hy, hair);
                pen.px(draw, hx + hw + 2, hy, hair);
            }
        }
        if character.age > 24 && seed.is_multiple_of(4) {
            pen.rect(draw, hx, 16, hw, 5, hair);
            pen.rect(draw, hx + 2, 21, hw - 4, 1, hair);
            let mouth = if open { MAROON } else { with_alpha(INK, 170) };
            pen.rect(draw, 14, 18, 4, 1, mouth);
        }
    } else {
        // A bald head catches the light.
        pen.rect(draw, hx + 3, hy + 1, 3, 1, with_alpha(BONE, 110));
    }
    if culture == Culture::Deepfolk && look.accessory != 3 {
        pen.rect(draw, hx - 3, hy - 3, hw + 6, 3, shirt_shade);
        pen.rect(draw, hx - 3, hy, 2, hh + 3, shirt_shade);
        pen.rect(draw, hx + hw + 1, hy, 2, hh + 3, shirt_shade);
    }
    if role == Role::Farmer && look.accessory != 1 {
        pen.rect(draw, hx - 5, hy - 1, hw + 10, 2, SAND);
        pen.rect(draw, hx, hy - 6, hw, 5, GOLD);
        pen.rect(draw, hx, hy - 2, hw, 1, RED);
    }

    // Accessory.
    match look.accessory {
        0 => {
            pen.rect(draw, hx - 1, 8, hw + 2, 1, SOIL);
            pen.rect(draw, 11, 6, 4, 3, AMBER);
            pen.rect(draw, 17, 6, 4, 3, AMBER);
            pen.px(draw, 12, 7, CREAM);
            pen.px(draw, 18, 7, CREAM);
        }
        1 => {
            let brim = if culture == Culture::Mesa { SAND } else { SOIL };
            pen.rect(draw, hx - 5, hy - 1, hw + 10, 2, brim);
            pen.rect(draw, hx, hy - 7, hw, 6, LOAM);
            pen.rect(draw, hx, hy - 3, hw, 1, trim);
        }
        2 => {
            pen.rect(draw, 10, 21, 12, 3, RED);
            pen.rect(draw, 14, 24, 4, 2, RED);
        }
        3 => {
            pen.rect(draw, hx - 1, hy - 4, hw + 2, 6, GOLD);
            pen.rect(draw, hx - 1, hy + 1, hw + 2, 1, AMBER);
            pen.rect(draw, 14, hy - 3, 4, 3, CREAM);
            pen.rect(draw, 12, hy - 6, 8, 2, with_alpha(GOLD, 80));
        }
        4 => {
            pen.rect(draw, 10, 11, 5, 4, STEEL_LIGHT);
            pen.rect(draw, 17, 11, 5, 4, STEEL_LIGHT);
            pen.rect(draw, 11, 12, 3, 2, CREAM);
            pen.rect(draw, 18, 12, 3, 2, CREAM);
            if !blink {
                pen.px(draw, 12, 12, INK);
                pen.px(draw, 19, 12, INK);
            }
            pen.rect(draw, 15, 12, 2, 1, STEEL_LIGHT);
        }
        5 => pen.rect(draw, sx, 30, sw, 2, LOAM),
        6 => pen.rect(draw, 8, 21, 16, 4, LAVENDER),
        8 => {
            pen.rect(draw, hx - 1, hy - 4, hw + 2, 5, MOSS);
            pen.rect(draw, hx + hw - 2, hy - 9, 2, 6, RED);
            pen.px(draw, hx + hw - 1, hy - 10, ORANGE);
        }
        9 => {
            pen.rect(draw, 10, 25, 12, 7, SOIL);
            pen.rect(draw, 11, 23, 1, 2, SOIL);
            pen.rect(draw, 20, 23, 1, 2, SOIL);
        }
        _ => {}
    }
    // A frame.
    pen.rect(draw, 0, 0, size, 1, with_alpha(INK, 160));
    pen.rect(draw, 0, size - 1, size, 1, with_alpha(INK, 160));
}
