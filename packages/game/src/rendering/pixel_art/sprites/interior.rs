//! Building interiors: per-building room tiles (wall, wainscot, floor), the counter and the
//! shopkeeper, plus shared props.
//!
//! Every building gets the same tile shapes in its own colours through [`interior_set!`], so a
//! new building only needs one more invocation.

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::{NineSlice, SpriteDef};

/// Brick wall, 16x16: `c` brick light, `a` brick, `d` brick shade, `b` mortar.
const WALL_GRID: &[&str] = &[
    "cccccccbcccccccb",
    "aaaaaaabaaaaaaab",
    "aaaaaaabaaaaaaab",
    "aaaaaaabaaaaaaab",
    "aaaaaaabaaaaaaab",
    "aaaaaaabaaaaaaab",
    "dddddddbdddddddb",
    "bbbbbbbbbbbbbbbb",
    "cccbcccccccbcccc",
    "aaabaaaaaaabaaaa",
    "aaabaaaaaaabaaaa",
    "aaabaaaaaaabaaaa",
    "aaabaaaaaaabaaaa",
    "aaabaaaaaaabaaaa",
    "dddbdddddddbdddd",
    "bbbbbbbbbbbbbbbb",
];

/// Glazed wall tiles, 16x16 (fuel station): `b` grout, `c` light, `a` tile, `d` shade.
const TILE_WALL_GRID: &[&str] = &[
    "bbbbbbbbbbbbbbbb",
    "bccccccabcccccca",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "baddddddbadddddd",
    "bbbbbbbbbbbbbbbb",
    "bccccccabcccccca",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "baddddddbadddddd",
];
/// Wood panelling, 16x16 (HQ): `b` seam, `c` light edge, `a` board, `d` shade.
const PANEL_WALL_GRID: &[&str] = &[
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
    "bcaaaaadbcaaaaad",
];
/// Marble slabs, 16x16 (bank): `b` joint, `c` light, `a` stone, `d` vein.
const MARBLE_WALL_GRID: &[&str] = &[
    "cccccccccccccccb",
    "cadaaaaaaaaaaaab",
    "caadaaaaaaaadaab",
    "caadaaaaaaadaaab",
    "caaadaaaaaaadaab",
    "caaaadaaaaaaadab",
    "caaaadaaaaaaadab",
    "caaaaadaaaaaaaab",
    "caaaaaadaaaaaaab",
    "caaaaaaadaaaaaab",
    "caaaaaaadaaaaaab",
    "caaaaaaaadaaaaab",
    "caaaaaaaaadaaaab",
    "caaaaaaaaaadaaab",
    "caaaaaaaaaadaaab",
    "bbbbbbbbbbbbbbbb",
];
/// Dressed stone, 16x16 (depot): `b` mortar, `c` light, `a` stone, `d` shade.
const STONE_WALL_GRID: &[&str] = &[
    "bbbbbbbbbbbbbbbb",
    "bcccccbccccbcccc",
    "bcaaadbcaadbcaad",
    "bcaaadbcaadbcaad",
    "bcaaadbcaadbcaad",
    "bcddddbcdddbcddd",
    "bbbbbbbbbbbbbbbb",
    "cccbcccccbccccbc",
    "aadbcaaadbcaadbc",
    "aadbcaaadbcaadbc",
    "dddbcddddbcdddbc",
    "bbbbbbbbbbbbbbbb",
    "cbcccccbccccbccc",
    "dbcaaadbcaadbcaa",
    "dbcaaadbcaadbcaa",
    "dbcddddbcdddbcdd",
];
/// Corrugated sheet, 16x16 (salvage): `b` seam and rivets, `c` ridge, `a` sheet, `d` trough.
const METAL_WALL_GRID: &[&str] = &[
    "bbbbbbbbbbbbbbbb",
    "cabdcaadcabdcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
];
/// Horizontal boards, 16x16 (explosives): `b` gap, `c` light, `a` board, `d` shade.
const BOARD_WALL_GRID: &[&str] = &[
    "cccccbcccccccbcc",
    "aaaaabaaaaaaabaa",
    "aaaaabaaaaaaabaa",
    "bbbbbbbbbbbbbbbb",
    "cbcccccccbcccccc",
    "abaaaaaaabaaaaaa",
    "abaaaaaaabaaaaaa",
    "bbbbbbbbbbbbbbbb",
    "cccccccbcccccccb",
    "aaaaaaabaaaaaaab",
    "aaaaaaabaaaaaaab",
    "bbbbbbbbbbbbbbbb",
    "cccbcccccccbcccc",
    "aaabaaaaaaabaaaa",
    "aaabaaaaaaabaaaa",
    "bbbbbbbbbbbbbbbb",
];

/// Wainscot planks, 16x16: `e` rail, `s` rail shadow, `c` plank light, `a` plank, `d` seam.
const PLANK_GRID: &[&str] = &[
    "eeeeeeeeeeeeeeee",
    "ssssssssssssssss",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "caadcaadcaadcaad",
    "dddddddddddddddd",
    "ssssssssssssssss",
];

/// Floor boards, 16x8: `e` edge light, `a` board, `d` seam.
const FLOOR_GRID: &[&str] = &[
    "eeeeeeeeeeeeeeee",
    "aaaaaaadaaaaaaad",
    "aaaaaaadaaaaaaad",
    "aaaaaaadaaaaaaad",
    "dddddddddddddddd",
    "aaadaaaaaaadaaaa",
    "aaadaaaaaaadaaaa",
    "aaadaaaaaaadaaaa",
];

/// Counter (9-slice, border 4): `l` top light, `m` top, `f` front, `d` kick plate.
const COUNTER_GRID: &[&str] = &[
    "ooooooooo",
    "olllllllo",
    "ommmmmmmo",
    "ooooooooo",
    "offfffffo",
    "offfffffo",
    "offfffffo",
    "odddddddo",
    "ooooooooo",
];

/// Shopkeeper bust facing left, 14x16: `c` cap, `s` skin, `m` mouth, `a` shirt, `b` strap.
const KEEPER_OPEN: &[&str] = &[
    "....oooooo....",
    "...ocCcccco...",
    "..occccccccco.",
    "oooooooooooo..",
    "...ossssssso..",
    "...osossosso..",
    "...ossssssso..",
    "...osssmmsso..",
    "....ossssso...",
    "...ooaaaaaoo..",
    "..oaaaaaaaaao.",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
];
const KEEPER_BLINK: &[&str] = &[
    "....oooooo....",
    "...ocCcccco...",
    "..occccccccco.",
    "oooooooooooo..",
    "...ossssssso..",
    "...ommssmmso..",
    "...ossssssso..",
    "...osssmmsso..",
    "....ossssso...",
    "...ooaaaaaoo..",
    "..oaaaaaaaaao.",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
    ".oaaaabaaaaaao",
];

/// A room's tiles and characters, all in one building's colours.
pub struct InteriorSet {
    pub wall: &'static SpriteDef,
    pub plank: &'static SpriteDef,
    pub floor: &'static SpriteDef,
    pub counter: &'static NineSlice,
    pub keeper: &'static SpriteDef,
    /// Ceiling and deep-shadow colour.
    pub shadow: Color,
    /// Sign and highlight accent.
    pub accent: Color,
}

/// Defines a building's interior sprites in module `$module`.
macro_rules! interior_set {
    (
        $module:ident, $prefix:literal,
        wall: $wall_grid:ident ($wc:ident, $wa:ident, $wd:ident, $wb:ident),
        plank: ($pe:ident, $ps:ident, $pc:ident, $pa:ident, $pd:ident),
        floor: ($fe:ident, $fa:ident, $fd:ident),
        counter: ($cl:ident, $cm:ident, $cf:ident, $cd:ident),
        keeper: ($kc:ident, $kC:ident, $ka:ident, $kb:ident),
        shadow: $shadow:ident,
        accent: $accent:ident $(,)?
    ) => {
        pub mod $module {
            use super::*;

            pub const WALL: SpriteDef = SpriteDef {
                name: concat!($prefix, "_wall"),
                legend: &[('c', $wc), ('a', $wa), ('d', $wd), ('b', $wb)],
                frames: &[$wall_grid],
                frame_seconds: 0.0,
            };
            pub const PLANK: SpriteDef = SpriteDef {
                name: concat!($prefix, "_plank"),
                legend: &[('e', $pe), ('s', $ps), ('c', $pc), ('a', $pa), ('d', $pd)],
                frames: &[PLANK_GRID],
                frame_seconds: 0.0,
            };
            pub const FLOOR: SpriteDef = SpriteDef {
                name: concat!($prefix, "_floor"),
                legend: &[('e', $fe), ('a', $fa), ('d', $fd)],
                frames: &[FLOOR_GRID],
                frame_seconds: 0.0,
            };
            pub const COUNTER_SPRITE: SpriteDef = SpriteDef {
                name: concat!($prefix, "_counter"),
                legend: &[('o', INK), ('l', $cl), ('m', $cm), ('f', $cf), ('d', $cd)],
                frames: &[COUNTER_GRID],
                frame_seconds: 0.0,
            };
            pub const COUNTER: NineSlice = NineSlice {
                sprite: &COUNTER_SPRITE,
                border: 4,
            };
            pub const KEEPER: SpriteDef = SpriteDef {
                name: concat!($prefix, "_keeper"),
                legend: &[
                    ('o', INK),
                    ('c', $kc),
                    ('C', $kC),
                    ('s', SAND),
                    ('m', CLAY),
                    ('a', $ka),
                    ('b', $kb),
                ],
                frames: &[
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_OPEN,
                    KEEPER_BLINK,
                ],
                frame_seconds: 0.2,
            };
            pub const SET: InteriorSet = InteriorSet {
                wall: &WALL,
                plank: &PLANK,
                floor: &FLOOR,
                counter: &COUNTER,
                keeper: &KEEPER,
                shadow: $shadow,
                accent: $accent,
            };
        }
    };
}

interior_set!(
    shop, "shop",
    wall: WALL_GRID (LAVENDER, VIOLET, PLUM, NIGHT),
    plank: (CLAY, SOIL_DARK, LOAM, SOIL, SOIL_DARK),
    floor: (STONE_LIGHT, STONE, STONE_DARK),
    counter: (SAND, CLAY, LOAM, SOIL),
    keeper: (ORANGE, GOLD, OCEAN, DEEP_BLUE),
    shadow: PLUM,
    accent: GOLD,
);
interior_set!(
    fuel, "fuel",
    wall: TILE_WALL_GRID (SKY, OCEAN, DEEP_BLUE, NIGHT),
    plank: (STEEL_LIGHT, STEEL_DARK, STEEL, STEEL_DARK, SLATE),
    floor: (STONE_LIGHT, STONE, STONE_DARK),
    counter: (STEEL_LIGHT, STEEL, OCEAN, DEEP_BLUE),
    keeper: (RED, ORANGE, AMBER, RUST),
    shadow: DEEP_BLUE,
    accent: AMBER,
);
interior_set!(
    repair, "repair",
    wall: WALL_GRID (CLAY, RUST, MAROON, SOIL_DARK),
    plank: (STEEL_LIGHT, STEEL_DARK, STEEL, STEEL_DARK, SLATE),
    floor: (STONE, STONE_DARK, INK),
    counter: (STEEL_LIGHT, STEEL, STEEL_DARK, SLATE),
    keeper: (SKY, SKY_LIGHT, OCEAN, DEEP_BLUE),
    shadow: MAROON,
    accent: ORANGE,
);
interior_set!(
    depot, "depot",
    wall: STONE_WALL_GRID (SAND, CLAY, LOAM, SOIL_DARK),
    plank: (SAND, SOIL_DARK, CLAY, LOAM, SOIL),
    floor: (LOAM, SOIL, SOIL_DARK),
    counter: (SAND, CLAY, LOAM, SOIL),
    keeper: (MOSS, LEAF, CLAY, RUST),
    shadow: PINE,
    accent: LIME,
);
interior_set!(
    hq, "hq",
    wall: PANEL_WALL_GRID (CLAY, LOAM, SOIL, SOIL_DARK),
    plank: (SKY_LIGHT, DEEP_BLUE, OCEAN, DEEP_BLUE, NIGHT),
    floor: (SLATE_LIGHT, SLATE, SLATE_DARK),
    counter: (MIST, STEEL_LIGHT, SLATE, SLATE_DARK),
    keeper: (DEEP_BLUE, OCEAN, SLATE_LIGHT, SLATE),
    shadow: NIGHT,
    accent: SKY_LIGHT,
);
interior_set!(
    bank, "bank",
    wall: MARBLE_WALL_GRID (BONE, MIST, FOG, FOG),
    plank: (CLAY, SOIL_DARK, LOAM, SOIL, SOIL_DARK),
    floor: (MIST, STONE_LIGHT, STONE),
    counter: (GOLD, AMBER, TEAL_DARK, PINE),
    keeper: (PINE, MOSS, SLATE, SLATE_DARK),
    shadow: PINE,
    accent: GOLD,
);
interior_set!(
    explosives, "explosives",
    wall: BOARD_WALL_GRID (CLAY, LOAM, SOIL, SOIL_DARK),
    plank: (SAND, SOIL_DARK, LOAM, SOIL, SOIL_DARK),
    floor: (LOAM, SOIL, SOIL_DARK),
    counter: (STONE_LIGHT, STONE, STONE_DARK, INK),
    keeper: (STONE, STONE_LIGHT, AMBER, RUST),
    shadow: MAROON,
    accent: ORANGE,
);
interior_set!(
    salvage, "salvage",
    wall: METAL_WALL_GRID (STEEL, STEEL_DARK, SLATE, NIGHT),
    plank: (COPPER, SOIL_DARK, RUST, MAROON, SOIL_DARK),
    floor: (LOAM, SOIL, SOIL_DARK),
    counter: (STEEL, STEEL_DARK, RUST, MAROON),
    keeper: (LIME, MOSS, LOAM, SOIL),
    shadow: STONE_DARK,
    accent: LIME,
);

/// Door frame (9-slice, border 3) around a door panel; the panel and window are drawn inside.
pub const DOOR_FRAME_SPRITE: SpriteDef = SpriteDef {
    name: "interior_door_frame",
    legend: &[('o', INK), ('l', LOAM), ('d', SOIL), ('f', SOIL_DARK)],
    frames: &[&[
        "ooooooo", //
        "olllldo", //
        "olooodo", //
        "olofodo", //
        "olooodo", //
        "olllldo", //
        "ooooooo", //
    ]],
    frame_seconds: 0.0,
};
pub const DOOR_FRAME: NineSlice = NineSlice {
    sprite: &DOOR_FRAME_SPRITE,
    border: 3,
};

/// Exit sign over the door.
pub const EXIT_SIGN: SpriteDef = SpriteDef {
    name: "interior_exit_sign",
    legend: &[('o', INK), ('g', LEAF), ('w', BONE)],
    frames: &[&[
        "ooooooooooooooooo",
        "ogggggggggggggggo",
        "ogwwwgwgwgwgwwwgo",
        "ogwgggwgwgwggwggo",
        "ogwwgggwggwggwggo",
        "ogwgggwgwgwggwggo",
        "ogwwwgwgwgwggwggo",
        "ogggggggggggggggo",
        "ooooooooooooooooo",
    ]],
    frame_seconds: 0.0,
};

/// Hanging lamp; its light cone is drawn separately with alpha.
pub const LAMP: SpriteDef = SpriteDef {
    name: "interior_lamp",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('m', STEEL),
        ('y', CREAM),
        ('g', GOLD),
    ],
    frames: &[&[
        "...o...", //
        "...o...", //
        "...o...", //
        "..ooo..", //
        ".osmso.", //
        "osmmmso", //
        "ooooooo", //
        ".ogyyg.", //
        "..ooo..", //
    ]],
    frame_seconds: 0.0,
};

pub const GEAR: SpriteDef = SpriteDef {
    name: "prop_gear",
    legend: &[('o', INK), ('m', STEEL_LIGHT), ('d', STEEL)],
    frames: &[&[
        "..o.o..", //
        ".ommmo.", //
        "omdodmo", //
        ".mo.om.", //
        "omdodmo", //
        ".ommmo.", //
        "..o.o..", //
    ]],
    frame_seconds: 0.0,
};

pub const PART_BOX: SpriteDef = SpriteDef {
    name: "prop_part_box",
    legend: &[
        ('o', INK),
        ('l', SKY_LIGHT),
        ('m', SKY),
        ('d', OCEAN),
        ('w', BONE),
    ],
    frames: &[&[
        "oooooooooo", //
        "ollllllllo", //
        "ommwwwwmdo", //
        "ommmmmmmdo", //
        "oddddddddo", //
        "oooooooooo", //
    ]],
    frame_seconds: 0.0,
};

pub const CAN: SpriteDef = SpriteDef {
    name: "prop_can",
    legend: &[('o', INK), ('r', RED), ('w', BONE), ('m', MAROON)],
    frames: &[&[
        ".ooo.", //
        "orrmo", //
        "owwmo", //
        "orrmo", //
        "orrmo", //
        "ooooo", //
    ]],
    frame_seconds: 0.0,
};

pub const CRATE: SpriteDef = SpriteDef {
    name: "prop_crate",
    legend: &[('o', INK), ('l', SAND), ('m', CLAY), ('d', LOAM)],
    frames: &[&[
        "oooooooooooo",
        "ollllllllllo",
        "omdmmmmmmdmo",
        "ommdmmmmdmmo",
        "ommmdmmdmmmo",
        "ommmmddmmmmo",
        "ommmdmmdmmmo",
        "ommdmmmmdmmo",
        "omdmmmmmmdmo",
        "oooooooooooo",
    ]],
    frame_seconds: 0.0,
};

pub const BARREL: SpriteDef = SpriteDef {
    name: "prop_barrel",
    legend: &[
        ('o', INK),
        ('l', SKY_LIGHT),
        ('m', SKY),
        ('d', OCEAN),
        ('s', STEEL_LIGHT),
    ],
    frames: &[&[
        ".oooooooo.",
        "osssssssso",
        "olmmmmmmdo",
        "olmmmmmmdo",
        "osssssssso",
        "olmmmmmmdo",
        "olmmmmmmdo",
        "olmmmmmmdo",
        "osssssssso",
        "olmmmmmmdo",
        "olmmmmmmdo",
        ".oooooooo.",
    ]],
    frame_seconds: 0.0,
};

/// Fuel pump with a blinking display.
pub const FUEL_PUMP: SpriteDef = SpriteDef {
    name: "prop_fuel_pump",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('m', MAROON),
        ('w', BONE),
        ('s', STEEL),
        ('k', NIGHT),
        ('g', LIME),
        ('h', STEEL_DARK),
    ],
    frames: &[
        &[
            ".oooooooooo...",
            "orrrrrrrrrmo..",
            "orkkkkkkkkmo..",
            "orkgggggkkmo..",
            "orkkkkkkkkmo..",
            "orrrrrrrrrmo..",
            "orwwrwwrrrmo..",
            "orrrrrrrrrmo..",
            "orrrrrrrrrmoo.",
            "orrsssrrrrmoho",
            "orrsssrrrrmo.h",
            "orrrrrrrrrmo.h",
            "orrrrrrrrrmo.h",
            "orrrrrrrrrmo.h",
            "ommmmmmmmmmo.h",
            "oooooooooooooo",
        ],
        &[
            ".oooooooooo...",
            "orrrrrrrrrmo..",
            "orkkkkkkkkmo..",
            "orkkggggkkmo..",
            "orkkkkkkkkmo..",
            "orrrrrrrrrmo..",
            "orwwrwwrrrmo..",
            "orrrrrrrrrmo..",
            "orrrrrrrrrmoo.",
            "orrsssrrrrmoho",
            "orrsssrrrrmo.h",
            "orrrrrrrrrmo.h",
            "orrrrrrrrrmo.h",
            "orrrrrrrrrmo.h",
            "ommmmmmmmmmo.h",
            "oooooooooooooo",
        ],
    ],
    frame_seconds: 0.5,
};
