//! Stamps the terrain rasterizer paints into tiles (ore clusters, buried artifacts) and the crack
//! overlay drawn over the tile being drilled.
//!
//! Ore stamps share a few shapes recoloured per mineral (`o` outline, `d`/`m`/`l` dark/mid/light,
//! `w` glint). Artifact stamps are one-offs. They are registered in the atlas too, so the gallery
//! and tests see them, but the terrain uses them CPU-side via [`SpriteDef::color_of`].

use drillgame_simulation::{
    terrain::{ArtifactKind, DeepStratum, MineralKind},
    worldgen::Band,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

const NUGGET: &[&str] = &[
    "..oo..", //
    ".olmo.", //
    "olwmdo", //
    "omddo.", //
    ".ooo..", //
];

const GEM: &[&str] = &[
    "..o..", //
    ".olo.", //
    "olwmo", //
    "olmdo", //
    "omddo", //
    ".odo.", //
    "..o..", //
];

const CHUNK: &[&str] = &[
    "oooo..", //
    "olwmoo", //
    "olmmdo", //
    "ommddo", //
    "oooooo", //
];

const FLAKE: &[&str] = &[
    ".ooo.", //
    "olwmo", //
    "omddo", //
    ".ooo.", //
];

macro_rules! ore_stamp {
    ($ident:ident, $name:literal, $shape:ident, $dark:ident, $mid:ident, $light:ident, $glint:ident) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[
                ('o', INK),
                ('d', $dark),
                ('m', $mid),
                ('l', $light),
                ('w', $glint),
            ],
            frames: &[$shape],
            frame_seconds: 0.0,
        };
    };
}

ore_stamp!(ORE_COPPER, "ore_copper", NUGGET, RUST, COPPER, SAND, CREAM);
ore_stamp!(
    ORE_IRON,
    "ore_iron",
    CHUNK,
    RUST,
    STEEL_DARK,
    STEEL,
    STEEL_LIGHT
);
ore_stamp!(
    ORE_SILVER,
    "ore_silver",
    FLAKE,
    STEEL,
    STEEL_LIGHT,
    MIST,
    BONE
);
ore_stamp!(ORE_GOLD, "ore_gold", NUGGET, ORANGE, AMBER, GOLD, CREAM);
ore_stamp!(ORE_EMERALD, "ore_emerald", GEM, MOSS, LEAF, LIME, BONE);
ore_stamp!(ORE_RUBY, "ore_ruby", GEM, MAROON, RED, ROSE, PINK);
ore_stamp!(ORE_DIAMOND, "ore_diamond", GEM, SKY, SKY_LIGHT, ICE, BONE);
ore_stamp!(
    ORE_PLATINUM,
    "ore_platinum",
    CHUNK,
    SLATE_LIGHT,
    MIST,
    ICE,
    BONE
);
ore_stamp!(ORE_URANIUM, "ore_uranium", GEM, MOSS, LIME, MINT, CREAM);
ore_stamp!(
    ORE_MYTHRIL,
    "ore_mythril",
    GEM,
    PLUM,
    VIOLET,
    LAVENDER,
    PINK
);
// Mythril in the Void Geode Fields, whose rock shares its violet: a pale ice rim and a brighter
// body keep the veins readable against the geode rock.
pub const ORE_MYTHRIL_GEODE: SpriteDef = SpriteDef {
    name: "ore_mythril_geode",
    legend: &[
        ('o', ICE),
        ('d', VIOLET),
        ('m', LAVENDER),
        ('l', PINK),
        ('w', BONE),
    ],
    frames: &[GEM],
    frame_seconds: 0.0,
};
// Volatile crystals in explosive pockets.
ore_stamp!(
    VOLATILE_CRYSTAL,
    "volatile_crystal",
    GEM,
    MAROON,
    RED,
    ORANGE,
    GOLD
);

/// The cluster stamp for `mineral`, and how many clusters a tile carries.
#[must_use]
pub const fn ore_stamp(mineral: MineralKind) -> (&'static SpriteDef, u32) {
    match mineral {
        MineralKind::Copper => (&ORE_COPPER, 4),
        MineralKind::Iron => (&ORE_IRON, 3),
        MineralKind::Silver => (&ORE_SILVER, 4),
        MineralKind::Gold => (&ORE_GOLD, 3),
        MineralKind::Emerald => (&ORE_EMERALD, 3),
        MineralKind::Ruby => (&ORE_RUBY, 3),
        MineralKind::Diamond => (&ORE_DIAMOND, 3),
        MineralKind::Platinum => (&ORE_PLATINUM, 3),
        MineralKind::Uranium => (&ORE_URANIUM, 3),
        MineralKind::Mythril => (&ORE_MYTHRIL, 3),
    }
}

/// The cluster stamp for `mineral` embedded in `band`'s rock: [`ore_stamp`], with a high-contrast
/// variant where the band's rock shares the ore's colours.
#[must_use]
pub const fn ore_stamp_in(mineral: MineralKind, band: Band) -> (&'static SpriteDef, u32) {
    match (mineral, band) {
        (MineralKind::Mythril, Band::Deep(DeepStratum::VoidGeodeFields)) => (&ORE_MYTHRIL_GEODE, 3),
        _ => ore_stamp(mineral),
    }
}

pub const FOSSIL: SpriteDef = SpriteDef {
    name: "artifact_fossil",
    legend: &[('o', INK), ('b', BONE), ('m', MIST), ('f', FOG)],
    frames: &[&[
        "..oooooo..", //
        ".obbbbbbo.", //
        "obmmmmmbbo", //
        "obmoooombo", //
        "obmobbfmbo", //
        "obmobofmbo", //
        "obmfoombbo", //
        "obbmmmmbo.", //
        ".obbbbbo..", //
        "..ooooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const OLD_CIRCUIT: SpriteDef = SpriteDef {
    name: "artifact_old_circuit",
    legend: &[
        ('o', INK),
        ('t', TEAL_DARK),
        ('T', TEAL),
        ('m', MINT),
        ('g', GOLD),
        ('s', SLATE),
    ],
    frames: &[&[
        "oooooooooo", //
        "oTmTTTTmTo", //
        "oTggoooTTo", //
        "oTTossoTgo", //
        "ogTossoTTo", //
        "oTTooooggo", //
        "oTmTTgTTmo", //
        "otttttttto", //
        "oooooooooo", //
    ]],
    frame_seconds: 0.0,
};

pub const BURIED_IDOL: SpriteDef = SpriteDef {
    name: "artifact_buried_idol",
    legend: &[
        ('o', INK),
        ('d', AMBER),
        ('g', GOLD),
        ('c', CREAM),
        ('r', RED),
    ],
    frames: &[&[
        "...oooo...", //
        "..ogggco..", //
        "..orggro..", //
        "..oggggo..", //
        ".oogddgoo.", //
        "ogdggggdgo", //
        "oo.oggo.oo", //
        "...oggo...", //
        "..ogddgo..", //
        "..oooooo..", //
    ]],
    frame_seconds: 0.0,
};

pub const STAR_CORE: SpriteDef = SpriteDef {
    name: "artifact_star_core",
    legend: &[('o', INK), ('l', SKY_LIGHT), ('w', ICE), ('c', BONE)],
    frames: &[&[
        "....o....", //
        "...olo...", //
        "..olwlo..", //
        "oolwcwloo", //
        "olwcccwlo", //
        "oolwcwloo", //
        "..olwlo..", //
        "...olo...", //
        "....o....", //
    ]],
    frame_seconds: 0.0,
};

pub const CRYSTAL_PRISM: SpriteDef = SpriteDef {
    name: "artifact_crystal_prism",
    legend: &[('o', INK), ('w', ICE), ('l', LAVENDER), ('s', VIOLET)],
    frames: &[&[
        "....o.....", //
        "...owo....", //
        "...owlo.o.", //
        ".o.owloowo", //
        "owoowlowlo", //
        "owlowlowlo", //
        "owlowlowso", //
        "oslosloslo", //
        "oooooooooo", //
    ]],
    frame_seconds: 0.0,
};

pub const RADIANT_SHELL: SpriteDef = SpriteDef {
    name: "artifact_radiant_shell",
    legend: &[('o', INK), ('g', GOLD), ('c', CREAM), ('a', AMBER)],
    frames: &[&[
        "...oooo...", //
        ".oocgcgoo.", //
        "ocgcgcgcgo", //
        "ocgcgcgcgo", //
        "oagcgcgcao", //
        ".oagcgcao.", //
        "..oagcao..", //
        "...oooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const PRESSURE_GLYPH: SpriteDef = SpriteDef {
    name: "artifact_pressure_glyph",
    legend: &[('o', INK), ('d', DEEP_BLUE), ('b', OCEAN), ('w', ICE)],
    frames: &[&[
        ".oooooooo.", //
        "obbbbbbbbo", //
        "obwbbbbwbo", //
        "obbwwwwbbo", //
        "obbbwwbbbo", //
        "obbwbbwbbo", //
        "obwbbbbwbo", //
        "obbbbbbbbo", //
        "oddddddddo", //
        ".oooooooo.", //
    ]],
    frame_seconds: 0.0,
};

pub const REACTOR_BLOOM: SpriteDef = SpriteDef {
    name: "artifact_reactor_bloom",
    legend: &[
        ('o', INK),
        ('g', LIME),
        ('m', MINT),
        ('l', LEAF),
        ('d', MOSS),
        ('c', CREAM),
    ],
    frames: &[&[
        "...ooo....", //
        "..ommmo...", //
        ".ommcmmoo.", //
        "ogmcccmmgo", //
        "ogmmcmmggo", //
        ".oggmggoo.", //
        "..oolloo..", //
        "...olo....", //
        "..oldlo...", //
        "..ooooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const MACHINE_SIGIL: SpriteDef = SpriteDef {
    name: "artifact_machine_sigil",
    legend: &[
        ('o', INK),
        ('l', STEEL_LIGHT),
        ('s', STEEL),
        ('d', STEEL_DARK),
        ('r', ORANGE),
        ('g', GOLD),
    ],
    frames: &[&[
        "...oooo...", //
        ".o.olso.o.", //
        "oloolssoso", //
        ".ollsssso.", //
        "olllogosso", //
        "olsoggrsdo", //
        ".olssssdo.", //
        "osoosddodo", //
        ".o.osdo.o.", //
        "...oooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const VOID_PEARL: SpriteDef = SpriteDef {
    name: "artifact_void_pearl",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('w', BONE),
    ],
    frames: &[&[
        "..oooooo..", //
        ".ovvvvvvo.", //
        "ovllvvvvpo", //
        "ovlwlvvvpo", //
        "ovvlvvvvpo", //
        "ovvvvvvvpo", //
        "ovvvvvvppo", //
        ".opvvvppo.", //
        "..oooooo..", //
    ]],
    frame_seconds: 0.0,
};

pub const MANTLE_HEART: SpriteDef = SpriteDef {
    name: "artifact_mantle_heart",
    legend: &[
        ('o', INK),
        ('m', MAROON),
        ('r', RED),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[&[
        ".ooo..ooo.", //
        "oarroorrro", //
        "oagrrrrrro", //
        "oarrrrrrmo", //
        ".orrrrrmo.", //
        "..orrrmo..", //
        "...ormo...", //
        "....oo....", //
    ]],
    frame_seconds: 0.0,
};

/// The stamp for a buried `artifact`.
#[must_use]
pub const fn artifact_stamp(artifact: ArtifactKind) -> &'static SpriteDef {
    match artifact {
        ArtifactKind::Fossil => &FOSSIL,
        ArtifactKind::OldCircuit => &OLD_CIRCUIT,
        ArtifactKind::BuriedIdol => &BURIED_IDOL,
        ArtifactKind::StarCore => &STAR_CORE,
        ArtifactKind::CrystalPrism => &CRYSTAL_PRISM,
        ArtifactKind::RadiantShell => &RADIANT_SHELL,
        ArtifactKind::PressureGlyph => &PRESSURE_GLYPH,
        ArtifactKind::ReactorBloom => &REACTOR_BLOOM,
        ArtifactKind::MachineSigil => &MACHINE_SIGIL,
        ArtifactKind::VoidPearl => &VOID_PEARL,
        ArtifactKind::MantleHeart => &MANTLE_HEART,
    }
}

/// Cracks spreading over the tile being drilled: frame `n` of 4 shows progress `n / 4`.
pub const CRACKS: SpriteDef = SpriteDef {
    name: "terrain_cracks",
    legend: &[('o', INK)],
    frames: CRACK_FRAMES,
    frame_seconds: 0.0,
};

/// The lit lower edge of [`CRACKS`] (drawn one art pixel down-right, under the dark lines).
pub const CRACKS_LIT: SpriteDef = SpriteDef {
    name: "terrain_cracks_lit",
    legend: &[('o', MIST)],
    frames: CRACK_FRAMES,
    frame_seconds: 0.0,
};

const CRACK_FRAMES: &[&[&str]] = &[
    &[
        "................", //
        "................", //
        "................", //
        "................", //
        "................", //
        "................", //
        "......oo........", //
        ".......o.o......", //
        "........oo......", //
        ".........o......", //
        "................", //
        "................", //
        "................", //
        "................", //
        "................", //
        "................", //
    ],
    &[
        "................", //
        "................", //
        "................", //
        "....o...........", //
        ".....o...o......", //
        ".....oo.o.......", //
        "......ooo.......", //
        ".......o.o......", //
        "........oo......", //
        ".........o......", //
        "..........o.....", //
        "................", //
        "................", //
        "................", //
        "................", //
        "................", //
    ],
    &[
        "................", //
        "...o............", //
        "...o........o...", //
        "....o......o....", //
        ".....o...oo.....", //
        ".....oo.o.......", //
        "o.....ooo.....o.", //
        ".oo....o.o..oo..", //
        "...ooo..oooo....", //
        ".........o......", //
        "..........o.....", //
        "...........o....", //
        "................", //
        "................", //
        "................", //
        "................", //
    ],
    &[
        "..o.........o...", //
        "...o.......o....", //
        "...o.......o....", //
        "....o.....o.....", //
        ".....o...oo.....", //
        ".....oo.o.....o.", //
        "o.....ooo....o..", //
        ".oo....o.o..oo..", //
        "...ooo..oooo....", //
        "......o..o......", //
        ".....o....o.....", //
        "....o......oo...", //
        "...o.........o..", //
        "..oo..........o.", //
        ".o.............o", //
        "................", //
    ],
];
