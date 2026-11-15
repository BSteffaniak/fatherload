//! The mining rig, authored facing right (flip for left), plus its drill heads and flame.
//!
//! The body is 16x13 art pixels (32x26 world pixels, one tile wide): a glass cockpit dome, a
//! beacon mast and exhaust stack, a painted hull and rolling tracks. These fixed sprites are the
//! classic rig for the title screen, the art gallery and icons; rigs in play are assembled and
//! painted by `rendering::rig_art`.

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

/// Drill head pointing up (the crown bore, drawn centred on the rig's roof): [`DRILL_DOWN`]
/// upside down.
pub const DRILL_UP: SpriteDef = SpriteDef {
    name: "rig_drill_up",
    legend: DRILL_LEGEND,
    frames: &[
        &[
            "....oo....", //
            "...owdo...", //
            "..oLmmdo..", //
            "..oLmwdo..", //
            ".owmmmmdo.", //
            "oSSSSSSSSo", //
            "oooooooooo", //
        ],
        &[
            "....oo....", //
            "...oLwo...", //
            "..owmmdo..", //
            "..oLmmdo..", //
            ".oLmwmmdo.", //
            "oSSSSSSSSo", //
            "oooooooooo", //
        ],
        &[
            "....oo....", //
            "...oLdo...", //
            "..oLmwdo..", //
            "..owmmdo..", //
            ".oLmmmwdo.", //
            "oSSSSSSSSo", //
            "oooooooooo", //
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

/// Garage row icons (9x9, like `icons`): paint, parts and attachments.
macro_rules! garage_icon {
    ($ident:ident, $name:literal, [$($legend:expr),* $(,)?], [$($row:literal),* $(,)?]) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[('o', INK), $($legend),*],
            frames: &[&[$($row),*]],
            frame_seconds: 0.0,
        };
    };
}

garage_icon!(
    ICON_SPRAY_CAN,
    "icon_spray_can",
    [
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('l', CREAM),
        ('y', GOLD),
        ('a', AMBER),
        ('m', MIST)
    ],
    [
        "...oo..m.",
        "..osso...",
        "..oddo.m.",
        ".olllyo..",
        ".olyyao.m",
        ".olyyao..",
        ".olyyao..",
        ".olaaao..",
        "..ooooo..",
    ]
);
garage_icon!(
    ICON_FLAME,
    "icon_flame",
    [('r', RED), ('a', ORANGE), ('g', GOLD), ('c', CREAM)],
    [
        "....o....",
        "...oro...",
        "..orao...",
        "..oragoo.",
        ".oragaro.",
        ".oagcgao.",
        ".oagccgo.",
        "..oacgo..",
        "...ooo...",
    ]
);
garage_icon!(
    ICON_PATTERN,
    "icon_pattern",
    [('l', BONE), ('d', SLATE)],
    [
        "ooooooooo",
        "olldldldo",
        "olldldldo",
        "odlldldlo",
        "odlldldlo",
        "oldldldlo",
        "oldldlldo",
        "odldldllo",
        "ooooooooo",
    ]
);
garage_icon!(
    ICON_STAR,
    "icon_star",
    [('g', GOLD), ('c', CREAM), ('a', AMBER)],
    [
        "....o....",
        "...oco...",
        "oooocoooo",
        "occggggao",
        ".ocgggao.",
        "..oggao..",
        ".oggoaao.",
        ".ogo.oao.",
        ".oo...oo.",
    ]
);
garage_icon!(
    ICON_ANTENNA,
    "icon_antenna",
    [('r', RED), ('s', STEEL_LIGHT), ('d', STEEL)],
    [
        "...oro...",
        "....s....",
        "..osdso..",
        "....s....",
        "....s....",
        "....s....",
        "..ooooo..",
        ".osssddo.",
        "..ooooo..",
    ]
);
garage_icon!(
    ICON_LAMP,
    "icon_lamp",
    [('s', STEEL), ('c', CREAM), ('m', MIST), ('w', BONE)],
    [
        ".........",
        ".oooo..m.",
        ".ossco...",
        ".osswcom.",
        ".osswco..",
        ".ossco.m.",
        ".oooo....",
        "..os.....",
        ".oooo....",
    ]
);
garage_icon!(
    ICON_PENNANT,
    "icon_pennant",
    [('s', STEEL_LIGHT), ('r', ROSE), ('p', PINK)],
    [
        ".so......",
        ".soooo...",
        ".soppro..",
        ".sorrrro.",
        ".sorrro..",
        ".sooo....",
        ".so......",
        ".so......",
        "ooooo....",
    ]
);
garage_icon!(
    ICON_DICE,
    "icon_dice",
    [('w', BONE), ('m', MIST), ('d', FOG)],
    [
        "ooooooooo",
        "owwwwwwmo",
        "owoowwwmo",
        "owoowwwmo",
        "owwwoowmo",
        "owwwoowmo",
        "owwwwwwmo",
        "ommmmmmdo",
        "ooooooooo",
    ]
);
garage_icon!(
    ICON_CREW,
    "icon_crew",
    [('g', GOLD), ('t', TEAL), ('r', ROSE), ('w', ICE)],
    [
        ".........",
        "...ooo...",
        "..owwo...",
        ".ooooooo.",
        ".ogggtto.",
        ".ogggtto.",
        ".orrrrro.",
        ".ooooooo.",
        ".........",
    ]
);
