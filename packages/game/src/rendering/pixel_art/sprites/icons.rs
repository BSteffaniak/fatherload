//! 9x9 icons that sit on a text line: rig stats, currency, warnings and every mineral.

use drillgame_simulation::terrain::MineralKind;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// Icon edge length in art pixels (one text line).
pub const ICON_SIZE: i32 = 9;

pub const FUEL: SpriteDef = SpriteDef {
    name: "icon_fuel",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('c', CREAM),
        ('r', ORANGE),
    ],
    frames: &[&[
        "....o....",
        "...ogo...",
        "...ogo...",
        "..ogcgo..",
        ".ogcggao.",
        ".oggggao.",
        ".ogggaro.",
        "..oarro..",
        "...ooo...",
    ]],
    frame_seconds: 0.0,
};

pub const HULL: SpriteDef = SpriteDef {
    name: "icon_hull",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('w', BONE),
        ('k', STEEL_DARK),
    ],
    frames: &[&[
        ".ooooooo.",
        "oswsssdko",
        "owsssddko",
        "osssdddko",
        "ossdddkko",
        ".osddkko.",
        ".oddkkko.",
        "..odkko..",
        "...ooo...",
    ]],
    frame_seconds: 0.0,
};

pub const CARGO: SpriteDef = SpriteDef {
    name: "icon_cargo",
    legend: &[('o', INK), ('l', LOAM), ('s', SAND), ('d', SOIL)],
    frames: &[&[
        "ooooooooo",
        "osssssslo",
        "osldllslo",
        "oslldlllo",
        "osllddllo",
        "osldlldlo",
        "osdlllldo",
        "olllllldo",
        "ooooooooo",
    ]],
    frame_seconds: 0.0,
};

pub const CREDITS: SpriteDef = SpriteDef {
    name: "icon_credits",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('c', CREAM),
        ('r', ORANGE),
    ],
    frames: &[&[
        "..ooooo..",
        ".occggao.",
        "ocggaagao",
        "ocgaggaro",
        "oggaggaro",
        "oggaggaro",
        "ogggaaaro",
        ".oaarrro.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

pub const DEPTH: SpriteDef = SpriteDef {
    name: "icon_depth",
    legend: &[('o', INK), ('m', SKY_LIGHT), ('d', SKY)],
    frames: &[&[
        "...ooo...",
        "...omo...",
        "...omo...",
        "...omo...",
        "ooomdmooo",
        ".ommmmdo.",
        "..ommdo..",
        "...odo...",
        "....o....",
    ]],
    frame_seconds: 0.0,
};

pub const WARNING: SpriteDef = SpriteDef {
    name: "icon_warning",
    legend: &[('o', INK), ('g', GOLD), ('a', AMBER), ('r', ORANGE)],
    frames: &[
        &[
            "....o....",
            "...ogo...",
            "...ogo...",
            "..ogoao..",
            "..ogoao..",
            ".oggoaao.",
            ".oggggao.",
            "oggaoaaao",
            "ooooooooo",
        ],
        &[
            "....o....",
            "...oao...",
            "...oao...",
            "..oaoro..",
            "..oaoro..",
            ".oaaorro.",
            ".oaaaaro.",
            "oaaroarro",
            "ooooooooo",
        ],
    ],
    frame_seconds: 0.35,
};

pub const HEAT: SpriteDef = SpriteDef {
    name: "icon_heat",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('a', ORANGE),
        ('g', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        "....o....",
        "...oro...",
        "..orao...",
        "..oraao..",
        ".oraagao.",
        ".oragcao.",
        "oraggcgao",
        ".oagccgo.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

pub const BOMB: SpriteDef = SpriteDef {
    name: "icon_bomb",
    legend: &[
        ('o', INK),
        ('k', SLATE),
        ('w', FOG),
        ('g', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        ".....o.g.",
        "....o.gcg",
        "..oooo.g.",
        ".okkkko..",
        "okwkkkko.",
        "okkkkkko.",
        "okkkkkko.",
        ".okkkko..",
        "..oooo...",
    ]],
    frame_seconds: 0.0,
};

pub const SCANNER: SpriteDef = SpriteDef {
    name: "icon_scanner",
    legend: &[('o', INK), ('m', MINT), ('d', TEAL)],
    frames: &[&[
        "..mmmmm..",
        ".m.....m.",
        "m..mmm..m",
        "..m...m..",
        "....d....",
        "...ooo...",
        "...odo...",
        "..ooooo..",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// Rounded ore nugget (metals): `d` shade, `m` body, `l` light, `w` glint.
const NUGGET: &[&str] = &[
    ".........",
    "...ooo...",
    "..owlmo..",
    ".owllmmo.",
    ".olllmmdo",
    "olllmmmdo",
    "olmmmmddo",
    ".oddddoo.",
    "..oooo...",
];

/// Faceted gem: `d` shade, `m` body, `l` light, `w` glint.
const GEM: &[&str] = &[
    "..ooooo..",
    ".owlmmdo.",
    "owllmmmdo",
    "olllmmddo",
    ".olmmmdo.",
    "..olmdo..",
    "...omo...",
    "....o....",
    ".........",
];

macro_rules! mineral_icon {
    ($ident:ident, $name:literal, $shape:ident, $d:ident, $m:ident, $l:ident, $w:ident) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[('o', INK), ('d', $d), ('m', $m), ('l', $l), ('w', $w)],
            frames: &[$shape],
            frame_seconds: 0.0,
        };
    };
}

mineral_icon!(COPPER_ORE, "icon_copper", NUGGET, RUST, COPPER, SAND, CREAM);
mineral_icon!(
    IRON_ORE,
    "icon_iron",
    NUGGET,
    STONE,
    STONE_LIGHT,
    MIST,
    BONE
);
mineral_icon!(
    SILVER_ORE,
    "icon_silver",
    NUGGET,
    STEEL_DARK,
    STEEL_LIGHT,
    ICE,
    BONE
);
mineral_icon!(GOLD_ORE, "icon_gold", NUGGET, AMBER, GOLD, CREAM, BONE);
mineral_icon!(EMERALD_ORE, "icon_emerald", GEM, MOSS, LEAF, LIME, BONE);
mineral_icon!(RUBY_ORE, "icon_ruby", GEM, MAROON, RED, ROSE, PINK);
mineral_icon!(DIAMOND_ORE, "icon_diamond", GEM, SKY, SKY_LIGHT, ICE, BONE);
mineral_icon!(
    PLATINUM_ORE,
    "icon_platinum",
    NUGGET,
    SLATE_LIGHT,
    MIST,
    ICE,
    BONE
);
mineral_icon!(URANIUM_ORE, "icon_uranium", NUGGET, MOSS, LIME, CREAM, BONE);
mineral_icon!(
    MYTHRIL_ORE,
    "icon_mythril",
    GEM,
    PLUM,
    VIOLET,
    LAVENDER,
    PINK
);

/// The icon for `mineral`.
#[must_use]
pub const fn mineral(mineral: MineralKind) -> &'static SpriteDef {
    match mineral {
        MineralKind::Copper => &COPPER_ORE,
        MineralKind::Iron => &IRON_ORE,
        MineralKind::Silver => &SILVER_ORE,
        MineralKind::Gold => &GOLD_ORE,
        MineralKind::Emerald => &EMERALD_ORE,
        MineralKind::Ruby => &RUBY_ORE,
        MineralKind::Diamond => &DIAMOND_ORE,
        MineralKind::Platinum => &PLATINUM_ORE,
        MineralKind::Uranium => &URANIUM_ORE,
        MineralKind::Mythril => &MYTHRIL_ORE,
    }
}
