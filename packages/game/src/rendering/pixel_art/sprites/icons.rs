//! 9x9 icons that sit on a text line: rig stats, currency, warnings and every mineral.

use fatherload_simulation::{item::BlockKind, terrain::MineralKind};

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

/// Height above the ground (the HUD readout while flying above the surface).
pub const ALTITUDE: SpriteDef = SpriteDef {
    name: "icon_altitude",
    legend: &[('o', INK), ('m', MINT), ('d', TEAL)],
    frames: &[&[
        "....o....",
        "...odo...",
        "..ommdo..",
        ".ommmmdo.",
        "ooomdmooo",
        "...omo...",
        "...omo...",
        "...omo...",
        "...ooo...",
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

/// Heat shielding: a shield with a cool core.
pub const HEAT_SHIELD: SpriteDef = SpriteDef {
    name: "icon_heat_shield",
    legend: &[('o', INK), ('a', ORANGE), ('i', ICE), ('s', SKY_LIGHT)],
    frames: &[&[
        "ooooooooo",
        "oaaaaaaao",
        "oaissssao",
        "oaisiisao",
        "oaisiisao",
        ".oaissao.",
        ".oaaisao.",
        "..oaaao..",
        "...ooo...",
    ]],
    frame_seconds: 0.0,
};

/// Lava plating: an armour plate over a lava drip.
pub const LAVA_PLATING: SpriteDef = SpriteDef {
    name: "icon_lava_plating",
    legend: &[
        ('o', INK),
        ('k', STEEL),
        ('w', STEEL_LIGHT),
        ('r', RED),
        ('a', ORANGE),
    ],
    frames: &[&[
        ".ooooooo.",
        "owwwwwwko",
        "owkkkkkko",
        "okkkkkkko",
        ".ooooooo.",
        "..oao.o..",
        "..oro.ao.",
        "...o..ro.",
        ".......o.",
    ]],
    frame_seconds: 0.0,
};

/// Gas scrubber: a filter mask.
pub const GAS_SCRUBBER: SpriteDef = SpriteDef {
    name: "icon_gas_scrubber",
    legend: &[('o', INK), ('m', MINT), ('k', SLATE), ('l', LIME)],
    frames: &[&[
        "..ooooo..",
        ".ommmmmo.",
        "omkmmmkmo",
        "ommmmmmmo",
        ".ommkmmo.",
        "..okkko..",
        "l.okoko.l",
        ".l.ooo.l.",
        "l.......l",
    ]],
    frame_seconds: 0.0,
};

/// Pressure bracing: crossed struts in a frame.
pub const PRESSURE_BRACING: SpriteDef = SpriteDef {
    name: "icon_pressure_bracing",
    legend: &[('o', INK), ('k', STEEL), ('g', GOLD)],
    frames: &[&[
        "ooooooooo",
        "ogk...kgo",
        "o.gk.kg.o",
        "o..gkg..o",
        "o...g...o",
        "o..gkg..o",
        "o.gk.kg.o",
        "ogk...kgo",
        "ooooooooo",
    ]],
    frame_seconds: 0.0,
};

/// Altitude intake: a ram scoop gulping streaks of thin air.
pub const ALTITUDE_INTAKE: SpriteDef = SpriteDef {
    name: "icon_altitude_intake",
    legend: &[
        ('o', INK),
        ('k', STEEL),
        ('w', STEEL_LIGHT),
        ('s', SKY_LIGHT),
        ('i', ICE),
    ],
    frames: &[&[
        "s........",
        ".s..oooo.",
        "i..owwwko",
        ".s.ow..ko",
        "s..ow..ko",
        ".i.ow..ko",
        "s..owwwko",
        ".s..oooo.",
        "i........",
    ]],
    frame_seconds: 0.0,
};

/// De-icer: a snowflake melting over a warm glow.
pub const DE_ICER: SpriteDef = SpriteDef {
    name: "icon_de_icer",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('w', BONE),
        ('a', AMBER),
        ('g', GOLD),
    ],
    frames: &[&[
        "....i....",
        ".i..i..i.",
        "..i.w.i..",
        "...iwi...",
        "iiwwwwwii",
        "...iwi...",
        "..i.w.i..",
        ".aagaggaa",
        "..aagaa..",
    ]],
    frame_seconds: 0.0,
};

/// Thin air: the air gauge's icon, three wind streaks thinning out.
pub const AIR: SpriteDef = SpriteDef {
    name: "icon_air",
    legend: &[('s', SKY_LIGHT), ('i', ICE), ('f', FOG)],
    frames: &[&[
        ".........",
        "iiiiiis..",
        "......is.",
        "..sssss..",
        ".........",
        ".ffff.f..",
        ".....f...",
        "..f.f....",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// Boulder deflector: a sloped plate turning a rock aside.
pub const BOULDER_DEFLECTOR: SpriteDef = SpriteDef {
    name: "icon_boulder_deflector",
    legend: &[
        ('o', INK),
        ('s', STONE),
        ('l', STONE_LIGHT),
        ('k', STEEL),
        ('w', STEEL_LIGHT),
    ],
    frames: &[&[
        ".ooo.....",
        "olsso....",
        "ossso....",
        ".ooo.....",
        "......oo.",
        "....oowko",
        "..oowkkko",
        "oowkkkko.",
        "ooooooo..",
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

/// A building block (WP-DROPS): `w` glint, `l` top face, `m` body, `d` shade, `x` specks.
const BLOCK: &[&str] = &[
    ".ooooooo.",
    "owllllllo",
    "olllllldo",
    "ommmmmmdo",
    "omxmmmmdo",
    "ommmmxmdo",
    "ommxmmmdo",
    "odddddddo",
    ".ooooooo.",
];

macro_rules! block_icon {
    ($ident:ident, $name:literal, $d:ident, $m:ident, $l:ident, $w:ident, $x:ident) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[
                ('o', INK),
                ('d', $d),
                ('m', $m),
                ('l', $l),
                ('w', $w),
                ('x', $x),
            ],
            frames: &[BLOCK],
            frame_seconds: 0.0,
        };
    };
}

block_icon!(
    DIRT_BLOCK,
    "icon_block_dirt",
    SOIL_DARK,
    SOIL,
    LOAM,
    SAND,
    SOIL_DARK
);
block_icon!(CLAY_BLOCK, "icon_block_clay", RUST, CLAY, SAND, CREAM, RUST);
block_icon!(
    STONE_BLOCK,
    "icon_block_stone",
    STONE_DARK,
    STONE,
    STONE_LIGHT,
    MIST,
    STONE_DARK
);
block_icon!(
    HARD_ROCK_BLOCK,
    "icon_block_hard_rock",
    SLATE_DARK,
    SLATE,
    SLATE_LIGHT,
    FOG,
    NIGHT
);

/// The icon for `block`.
#[must_use]
pub const fn block(block: BlockKind) -> &'static SpriteDef {
    match block {
        BlockKind::Dirt => &DIRT_BLOCK,
        BlockKind::Clay => &CLAY_BLOCK,
        BlockKind::Stone => &STONE_BLOCK,
        BlockKind::HardRock => &HARD_ROCK_BLOCK,
    }
}

/// The pilot's helmet (pilot health, the pilot suit, "on foot").
pub const HELMET: SpriteDef = SpriteDef {
    name: "icon_helmet",
    legend: &[
        ('o', INK),
        ('w', BONE),
        ('m', MIST),
        ('f', FOG),
        ('v', OCEAN),
        ('g', ICE),
        ('a', AMBER),
    ],
    frames: &[&[
        "..ooooo..",
        ".owwwwmo.",
        "owwvvvvmo",
        "owvgvvvfo",
        "owvvvvvfo",
        "owwvvvmfo",
        ".owwmmfo.",
        "oaaaaaaao",
        ".ooooooo.",
    ]],
    frame_seconds: 0.0,
};

/// The jetpack (its burn gauge and the gear row).
pub const JETPACK: SpriteDef = SpriteDef {
    name: "icon_jetpack",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('d', STEEL),
        ('k', STEEL_DARK),
        ('y', GOLD),
        ('r', ORANGE),
    ],
    frames: &[&[
        ".oo...oo.",
        "osdo.osdo",
        "osdooosdo",
        "osdkdksdo",
        "osdkdksdo",
        "okkoookko",
        ".oko.oko.",
        ".oyo.oyo.",
        "..r...r..",
    ]],
    frame_seconds: 0.0,
};

/// Air bubbles (the pilot's oxygen).
pub const OXYGEN: SpriteDef = SpriteDef {
    name: "icon_oxygen",
    legend: &[('o', INK), ('s', SKY_LIGHT), ('i', ICE), ('b', SKY)],
    frames: &[&[
        "....ooo..",
        "...oisbo.",
        "...osbbo.",
        "oo..ooo..",
        "iso......",
        "obo.oooo.",
        "...oissbo",
        "...osbbbo",
        "....oooo.",
    ]],
    frame_seconds: 0.0,
};

/// The rig's pickup magnet (the "collect blocks" toggle).
pub const MAGNET: SpriteDef = SpriteDef {
    name: "icon_magnet",
    legend: &[('o', INK), ('r', RED), ('p', ROSE), ('w', STEEL_LIGHT)],
    frames: &[&[
        "..ooooo..",
        ".oprrrro.",
        "oprooorro",
        "opro.orro",
        "orro.orro",
        "oooo.oooo",
        "owwo.owwo",
        "owwo.owwo",
        "oooo.oooo",
    ]],
    frame_seconds: 0.0,
};

/// A crescent moon: sleeping, night and the clock's night side.
pub const MOON: SpriteDef = SpriteDef {
    name: "icon_moon",
    legend: &[('o', INK), ('c', CREAM), ('m', MIST), ('b', BONE)],
    frames: &[&[
        "...ooo...",
        "..obbo...",
        ".obco....",
        ".obco....",
        ".obcco...",
        ".obccmo..",
        "..obccmo.",
        "...oooo..",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// A sun: daytime on the clock.
pub const SUN: SpriteDef = SpriteDef {
    name: "icon_sun",
    legend: &[('o', INK), ('g', GOLD), ('c', CREAM), ('a', AMBER)],
    frames: &[&[
        "....a....",
        ".a.ooo.a.",
        "..ocggo..",
        ".ocggggo.",
        "aoggggaoa",
        ".oggggao.",
        "..ogaao..",
        ".a.ooo.a.",
        "....a....",
    ]],
    frame_seconds: 0.0,
};
