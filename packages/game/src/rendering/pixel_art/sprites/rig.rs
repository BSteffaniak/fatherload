//! The mining rig, authored facing right (flip for left), plus its drill heads and flame.
//!
//! The body is 16x13 art pixels (32x26 world pixels, one tile wide): a glass cockpit dome, a
//! beacon mast and exhaust stack, a painted hull and rolling tracks. Remote players' rigs reuse
//! the same grids with other hull paint (`rig_body!`).

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// Body frames: the tracks roll between the two.
const BODY_FRAMES: &[&[&str]] = &[
    &[
        "..b.............", //
        "..t.....ooooo...", //
        "..t...oowwggGo..", //
        ".oSo.owwggggGGo.", //
        ".oooooooooooooo.", //
        "ohhhhhhhhhhhhhyo", //
        "ohyySyyyyyyyyyll", //
        "ohrrrrrrrrrrrrao", //
        "oyyyyyyySyyyyyao", //
        "oaaaaaaaaaaaaaao", //
        ".oooooooooooooo.", //
        "oLdLdLdLdLdLdLdo", //
        ".oooooooooooooo.", //
    ],
    &[
        "..b.............", //
        "..t.....ooooo...", //
        "..t...oowwggGo..", //
        ".oSo.owwggggGGo.", //
        ".oooooooooooooo.", //
        "ohhhhhhhhhhhhhyo", //
        "ohyySyyyyyyyyyll", //
        "ohrrrrrrrrrrrrao", //
        "oyyyyyyySyyyyyao", //
        "oaaaaaaaaaaaaaao", //
        ".oooooooooooooo.", //
        "odLdLdLdLdLdLdLo", //
        ".oooooooooooooo.", //
    ],
];

/// Damaged body: scorch marks, a cracked dome and a dead headlight.
const DAMAGED_FRAMES: &[&[&str]] = &[
    &[
        "..b.............", //
        "..t.....ooooo...", //
        "..t...oowxggGo..", //
        ".oSo.owwgxggGGo.", //
        ".oooooooooooooo.", //
        "ohhhhxhhhhhhxhyo", //
        "ohyxxyyyyyyxxyko", //
        "ohrrrxrrrrrrrrao", //
        "oyyyxyyySxyyyyao", //
        "oaaraaaaxxaaaaao", //
        ".oooooooooooooo.", //
        "oLdLdLdLdLdLdLdo", //
        ".oooooooooooooo.", //
    ],
    &[
        "..b.............", //
        "..t.....ooooo...", //
        "..t...oowxggGo..", //
        ".oSo.owwgxggGGo.", //
        ".oooooooooooooo.", //
        "ohhhhxhhhhhhxhyo", //
        "ohyxxyyyyyyxxyko", //
        "ohrrrxrrrrrrrrao", //
        "oyyyxyyySxyyyyao", //
        "oaaraaaaxxaaaaao", //
        ".oooooooooooooo.", //
        "odLdLdLdLdLdLdLo", //
        ".oooooooooooooo.", //
    ],
];

macro_rules! rig_body {
    ($ident:ident, $name:literal, $frames:ident, $light:ident, $body:ident, $shade:ident, $deep:ident) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[
                ('o', INK),
                ('b', RED),
                ('t', STEEL_LIGHT),
                ('S', STEEL_DARK),
                ('g', SKY),
                ('G', OCEAN),
                ('w', ICE),
                ('h', $light),
                ('y', $body),
                ('a', $shade),
                ('r', $deep),
                ('l', CREAM),
                ('k', STEEL_DARK),
                ('x', STONE_DARK),
                ('L', STEEL_LIGHT),
                ('d', STEEL_DARK),
            ],
            frames: $frames,
            frame_seconds: 0.08,
        };
    };
}

rig_body!(RIG, "rig", BODY_FRAMES, CREAM, GOLD, AMBER, ORANGE);
rig_body!(
    RIG_DAMAGED,
    "rig_damaged",
    DAMAGED_FRAMES,
    GOLD,
    AMBER,
    ORANGE,
    RUST
);
rig_body!(
    RIG_TEAL,
    "rig_teal",
    BODY_FRAMES,
    MINT,
    TEAL,
    TEAL_DARK,
    TEAL_DARK
);
rig_body!(RIG_ROSE, "rig_rose", BODY_FRAMES, PINK, ROSE, RED, MAROON);
rig_body!(RIG_LIME, "rig_lime", BODY_FRAMES, LIME, LEAF, MOSS, PINE);
rig_body!(
    RIG_VIOLET,
    "rig_violet",
    BODY_FRAMES,
    LAVENDER,
    VIOLET,
    PLUM,
    PLUM
);
rig_body!(RIG_SKY, "rig_sky", BODY_FRAMES, ICE, SKY_LIGHT, SKY, OCEAN);

/// Each player's hull paint, chosen by player id (player 1 is the classic yellow rig). A player
/// has the same paint in every view: their own, a split-screen partner's, a remote client's, and
/// inside buildings.
pub const PLAYER_BODIES: [&SpriteDef; 6] =
    [&RIG, &RIG_TEAL, &RIG_ROSE, &RIG_LIME, &RIG_VIOLET, &RIG_SKY];

/// The accent colour matching each entry of [`PLAYER_BODIES`] (name tags, HUD title, minimap).
pub const PLAYER_ACCENTS: [Color; 6] = [GOLD, MINT, PINK, LIME, LAVENDER, SKY_LIGHT];

/// Index into [`PLAYER_BODIES`] / [`PLAYER_ACCENTS`] for a player id (ids start at 1).
#[must_use]
pub const fn paint_index(player_id: u64) -> usize {
    (player_id.saturating_sub(1) % PLAYER_BODIES.len() as u64) as usize
}

const DRILL_LEGEND: &[(char, Color)] = &[
    ('o', INK),
    ('L', STEEL_LIGHT),
    ('m', STEEL),
    ('d', STEEL_DARK),
    ('S', SLATE),
    ('w', BONE),
];

/// Drill head pointing down (drawn centred under the rig); a bright band spirals down it.
pub const DRILL_DOWN: SpriteDef = SpriteDef {
    name: "rig_drill_down",
    legend: DRILL_LEGEND,
    frames: &[
        &[
            "oooooooooo", //
            "oSSSSSSSSo", //
            ".owmmmmdo.", //
            "..oLmwdo..", //
            "..oLmmdo..", //
            "...owdo...", //
            "....oo....", //
        ],
        &[
            "oooooooooo", //
            "oSSSSSSSSo", //
            ".oLmwmmdo.", //
            "..oLmmdo..", //
            "..owmmdo..", //
            "...oLwo...", //
            "....oo....", //
        ],
        &[
            "oooooooooo", //
            "oSSSSSSSSo", //
            ".oLmmmwdo.", //
            "..owmmdo..", //
            "..oLmwdo..", //
            "...oLdo...", //
            "....oo....", //
        ],
    ],
    frame_seconds: 0.05,
};

/// Drill head pointing right (flip for left), drawn at the rig's front.
pub const DRILL_SIDE: SpriteDef = SpriteDef {
    name: "rig_drill_side",
    legend: DRILL_LEGEND,
    frames: &[
        &[
            "oo......", //
            "oSoo....", //
            "oSwmoo..", //
            "oSLmmwoo", //
            "oSmmmddo", //
            "oSmwdoo.", //
            "oSdoo...", //
            "ooo.....", //
        ],
        &[
            "oo......", //
            "oSoo....", //
            "oSLwoo..", //
            "oSLmmmoo", //
            "oSmwmddo", //
            "oSmddoo.", //
            "oSwoo...", //
            "ooo.....", //
        ],
        &[
            "oo......", //
            "oSoo....", //
            "oSLmoo..", //
            "oSwmmmoo", //
            "oSmmmdwo", //
            "oSwddoo.", //
            "oSdoo...", //
            "ooo.....", //
        ],
    ],
    frame_seconds: 0.05,
};

/// Thruster flame under the rig.
pub const FLAME: SpriteDef = SpriteDef {
    name: "rig_flame",
    legend: &[('r', RED), ('a', ORANGE), ('g', GOLD), ('c', CREAM)],
    frames: &[
        &[
            "agccga", //
            "ragcar", //
            ".rgcr.", //
            ".rgar.", //
            "..ra..", //
            "..r...", //
        ],
        &[
            "agccga", //
            "rgccgr", //
            ".agca.", //
            "..gar.", //
            "..rr..", //
            "...r..", //
        ],
        &[
            "agccga", //
            "ragcgr", //
            ".rgcar", //
            ".rag..", //
            ".rr...", //
            "..r...", //
        ],
    ],
    frame_seconds: 0.06,
};

/// A short hover flame (thrusting without climbing).
pub const FLAME_SMALL: SpriteDef = SpriteDef {
    name: "rig_flame_small",
    legend: &[('r', RED), ('a', ORANGE), ('g', GOLD), ('c', CREAM)],
    frames: &[
        &[
            "agccga", //
            ".rgar.", //
            "..r...", //
        ],
        &[
            "agccga", //
            ".ragr.", //
            "...r..", //
        ],
    ],
    frame_seconds: 0.06,
};
