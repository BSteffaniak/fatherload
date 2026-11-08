//! The mining rig, authored facing right (flip for left), plus its drill heads and flame.

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

const RIG_LEGEND: &[(char, Color)] = &[
    ('o', INK),
    ('g', SKY),
    ('w', ICE),
    ('y', GOLD),
    ('h', CREAM),
    ('a', AMBER),
    ('r', ORANGE),
    ('t', STEEL),
    ('T', STEEL_DARK),
    ('l', CREAM),
    ('k', STEEL_DARK),
];

/// Rig body, 14x11 art pixels (28x22 world pixels); two frames roll the tracks.
pub const RIG: SpriteDef = SpriteDef {
    name: "rig",
    legend: RIG_LEGEND,
    frames: &[
        &[
            ".k....oooo....",
            ".k...ogggwo...",
            ".k...ogwggo...",
            "ooooooooooooo.",
            "oyhyyyyyyyyyao",
            "oyhyyyyyyyyylo",
            "oyyyyyyyyyyyao",
            "oaaaaaaaaaarro",
            "oooooooooooooo",
            "otTtTtTtTtTtTo",
            ".oooooooooooo.",
        ],
        &[
            ".k....oooo....",
            ".k...ogggwo...",
            ".k...ogwggo...",
            "ooooooooooooo.",
            "oyhyyyyyyyyyao",
            "oyhyyyyyyyyylo",
            "oyyyyyyyyyyyao",
            "oaaaaaaaaaarro",
            "oooooooooooooo",
            "oTtTtTtTtTtTto",
            ".oooooooooooo.",
        ],
    ],
    frame_seconds: 0.08,
};

/// Rig body when the hull is badly damaged: scorched plates.
pub const RIG_DAMAGED: SpriteDef = SpriteDef {
    name: "rig_damaged",
    legend: &[
        ('o', INK),
        ('g', SKY),
        ('w', ICE),
        ('y', AMBER),
        ('h', GOLD),
        ('a', ORANGE),
        ('r', RUST),
        ('t', STEEL),
        ('T', STEEL_DARK),
        ('l', CREAM),
        ('k', STEEL_DARK),
        ('x', STONE_DARK),
    ],
    frames: &[&[
        ".k....oooo....",
        ".k...ogxgwo...",
        ".k...ogwggo...",
        "ooooooooooooo.",
        "oyhyyxyyyyyyao",
        "oyhyxxyyyxyylo",
        "oyyyyyyyxxyyao",
        "oaaraaaaaaarro",
        "oooooooooooooo",
        "otTtTtTtTtTtTo",
        ".oooooooooooo.",
    ]],
    frame_seconds: 0.0,
};

/// Drill head pointing down (drawn centred under the rig).
pub const DRILL_DOWN: SpriteDef = SpriteDef {
    name: "rig_drill_down",
    legend: &[('o', INK), ('m', STEEL_LIGHT), ('d', STEEL), ('w', BONE)],
    frames: &[
        &[
            "oooooooo", //
            "omwmmmdo", //
            ".omwmdo.", //
            ".ommddo.", //
            "..omdo..", //
            "...oo...", //
        ],
        &[
            "oooooooo", //
            "ommmwmdo", //
            ".ommwdo.", //
            ".omddmo.", //
            "..omdo..", //
            "...oo...", //
        ],
    ],
    frame_seconds: 0.06,
};

/// Drill head pointing right (flip for left), drawn at the rig's side.
pub const DRILL_SIDE: SpriteDef = SpriteDef {
    name: "rig_drill_side",
    legend: &[('o', INK), ('m', STEEL_LIGHT), ('d', STEEL), ('w', BONE)],
    frames: &[
        &[
            "oo....", //
            "owoo..", //
            "ommwoo", //
            "ommddo", //
            "omdoo.", //
            "ooo...", //
        ],
        &[
            "oo....", //
            "omoo..", //
            "omwmoo", //
            "omdwdo", //
            "odmoo.", //
            "ooo...", //
        ],
    ],
    frame_seconds: 0.06,
};

/// Thruster flame under the rig while climbing.
pub const FLAME: SpriteDef = SpriteDef {
    name: "rig_flame",
    legend: &[('r', RED), ('a', ORANGE), ('g', GOLD), ('c', CREAM)],
    frames: &[
        &[
            "rgccgr", //
            "ragcar", //
            ".raar.", //
            ".rar..", //
            "..r...", //
        ],
        &[
            "agccga", //
            "ragcgr", //
            ".rgar.", //
            "..rar.", //
            "...r..", //
        ],
    ],
    frame_seconds: 0.07,
};
