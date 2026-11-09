//! Point-of-interest art: 16x16 stamps the terrain rasterizer paints into POI tiles (crates,
//! salvage cores, vault doors, seals, switches, relays, caches, outpost lockers), small fossil
//! imprints for fossil shale, and the 9x9 POI icons used on the maps and in the collection log.
//!
//! Tile stamps keep `.` transparent where the tile's procedural base (vault wall, back wall)
//! shows through. Like the ore stamps they are packed into the atlas for the gallery and tests,
//! but the terrain reads them CPU-side via [`SpriteDef::color_of`].

use drillgame_simulation::poi::{PoiKind, PoiKnowledge};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A planked supply crate with steel corners and a stencil plate.
pub const CRATE: SpriteDef = SpriteDef {
    name: "poi_crate",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', LOAM),
        ('d', SOIL),
        ('k', SOIL_DARK),
        ('s', STEEL),
        ('w', STEEL_LIGHT),
        ('g', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        "oooooooooooooooo", //
        "osssllllllllssso", //
        "oswsllllllllswso", //
        "ossskkkkkkkkssso", //
        "ollkmmmmmmmmlkdo", //
        "ollkmmmmmmmlkkdo", //
        "ollkddddddlkddko", //
        "ollkmmmcglkmmkdo", //
        "ollkmmmglkgmmkdo", //
        "ollkddlkddddddko", //
        "ollkmlkmmmmmmkdo", //
        "ollklkmmmmmmmkdo", //
        "ossskkkkkkkkssso", //
        "oswsddddddddswso", //
        "osssddddddddssso", //
        "oooooooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A derelict rig's cockpit console: a glowing screen, buttons and a beacon.
pub const SALVAGE_CORE: SpriteDef = SpriteDef {
    name: "poi_salvage_core",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('k', STEEL_DARK),
        ('t', TEAL_DARK),
        ('n', MINT),
        ('i', ICE),
        ('g', GOLD),
        ('r', RED),
        ('e', ORANGE),
        ('v', LEAF),
        ('a', AMBER),
    ],
    frames: &[&[
        "oooooooooooooooo", //
        "ossssssssssssrro", //
        "oskkkkkkkkkkkkeo", //
        "osooooooooooooko", //
        "osottttttttiioko", //
        "osotntntntnttoko", //
        "osottttttttttoko", //
        "osotnntnntntnoko", //
        "osottttttttttoko", //
        "osooooooooooooko", //
        "oskkkkkkkkkkkkko", //
        "osggggggggggggko", //
        "oskkkkkkkkkkkkko", //
        "oskrrkkvvkkaakko", //
        "oskkkkkkkkkkkkko", //
        "oooooooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A banded vault door with a brass ring lock and a red "locked" lamp.
pub const VAULT_DOOR: SpriteDef = SpriteDef {
    name: "poi_vault_door",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('l', STEEL_LIGHT),
        ('m', STEEL),
        ('d', STEEL_DARK),
        ('k', NIGHT),
        ('c', CREAM),
        ('r', RED),
        ('e', ROSE),
    ],
    frames: &[&[
        "oooooooooooooooo", //
        "oggggggggggggggo", //
        "oglmdmmermmdmdao", //
        "oglmdmmrrmmdmdao", //
        "oglmdmmmmmmdmdao", //
        "oglmdmmggmmdmdao", //
        "oglmdggggggdmdao", //
        "oglmdgckkkgdmdao", //
        "oglmggkookggmdao", //
        "oglmggkookggmdao", //
        "oglmdgkkkkadmdao", //
        "oglmdgaaaaadmdao", //
        "oglmdmmaammdmdao", //
        "oglmdmmmmmmdmdao", //
        "ogaaaaaaaaaaaaao", //
        "oooooooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A pressure seal: a red valve wheel with a gauge, set into the vault wall.
pub const VAULT_SEAL: SpriteDef = SpriteDef {
    name: "poi_vault_seal",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('e', ROSE),
        ('k', MAROON),
        ('d', STEEL_DARK),
        ('g', GOLD),
        ('c', CREAM),
        ('b', BONE),
        ('n', RED),
    ],
    frames: &[&[
        "...........ooooo", //
        "...........obnbo", //
        "......oooo.obbbo", //
        "....ooerrroooooo", //
        "...oerkddkrro...", //
        "...erkkddkkrro..", //
        "..oekkkddkkkro..", //
        "..orkkcgggkkro..", //
        "..orddggggddro..", //
        "..orkkggggkkro..", //
        "..orkkkddkkkro..", //
        "...orkkddkkro...", //
        "...orrkddkrro...", //
        "....oorrrroo....", //
        "......oooo......", //
        "................", //
    ]],
    frame_seconds: 0.0,
};

/// A blasted pressure seal: the wheel cracked open and scorched.
pub const VAULT_SEAL_VENTED: SpriteDef = SpriteDef {
    name: "poi_vault_seal_vented",
    legend: &[
        ('o', INK),
        ('r', MAROON),
        ('k', NIGHT),
        ('d', STEEL_DARK),
        ('x', INK),
        ('b', FOG),
        ('s', STONE_DARK),
    ],
    frames: &[&[
        "...........ooooo", //
        "...........obxbo", //
        "......ooxo.obbbo", //
        "....ooxr.rxooooo", //
        "...orxkddkrxo...", //
        "...xs.kddk.rx...", //
        "..orks.dd.skxo..", //
        "..x.kkddddk.xr..", //
        "..rdddssssdddx..", //
        "..or.kddddk.ro..", //
        "..xrks.dd.skro..", //
        "...xs.kddk.xr...", //
        "...orxkddkrrx...", //
        "....oorrxroo....", //
        "......ooox......", //
        "................", //
    ]],
    frame_seconds: 0.0,
};

/// One sequence-lock switch plate: `pips` lights show its order, with a chevron under it
/// pointing down (drill it from above).
macro_rules! switch_plate {
    ($ident:ident, $name:literal, $pips:expr, $pip:ident, $glint:ident, $face:ident) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[
                ('o', INK),
                ('g', GOLD),
                ('l', CREAM),
                ('k', $face),
                ('p', $pip),
                ('q', $glint),
                ('a', AMBER),
            ],
            frames: &[$pips],
            frame_seconds: 0.0,
        };
    };
}

const PIPS_1: &[&str] = &[
    "oooooooooooooooo", //
    "oggggggggggggggo", //
    "oglkkkkkkkkkkkgo", //
    "ogkkkkkqpkkkkkgo", //
    "ogkkkkkppkkkkkgo", //
    "ogkkkkkkkkkkkkgo", //
    "oggggggggggggggo", //
    "oooooooooooooooo", //
    "................", //
    ".....aaaaaa.....", //
    "......aaaa......", //
    ".......aa.......", //
    "................", //
    "................", //
    "................", //
    "................", //
];

const PIPS_2: &[&str] = &[
    "oooooooooooooooo", //
    "oggggggggggggggo", //
    "oglkkkkkkkkkkkgo", //
    "ogkkkqpkkqpkkkgo", //
    "ogkkkppkkppkkkgo", //
    "ogkkkkkkkkkkkkgo", //
    "oggggggggggggggo", //
    "oooooooooooooooo", //
    "................", //
    ".....aaaaaa.....", //
    "......aaaa......", //
    ".......aa.......", //
    "................", //
    "................", //
    "................", //
    "................", //
];

const PIPS_3: &[&str] = &[
    "oooooooooooooooo", //
    "oggggggggggggggo", //
    "oglkkkkkkkkkkkgo", //
    "ogkqpkkqpkkqpkgo", //
    "ogkppkkppkkppkgo", //
    "ogkkkkkkkkkkkkgo", //
    "oggggggggggggggo", //
    "oooooooooooooooo", //
    "................", //
    ".....aaaaaa.....", //
    "......aaaa......", //
    ".......aa.......", //
    "................", //
    "................", //
    "................", //
    "................", //
];

switch_plate!(SWITCH_1, "poi_switch_1", PIPS_1, STEEL_DARK, STEEL, NIGHT);
switch_plate!(SWITCH_2, "poi_switch_2", PIPS_2, STEEL_DARK, STEEL, NIGHT);
switch_plate!(SWITCH_3, "poi_switch_3", PIPS_3, STEEL_DARK, STEEL, NIGHT);
switch_plate!(
    SWITCH_1_LIT,
    "poi_switch_1_lit",
    PIPS_1,
    LIME,
    CREAM,
    TEAL_DARK
);
switch_plate!(
    SWITCH_2_LIT,
    "poi_switch_2_lit",
    PIPS_2,
    LIME,
    CREAM,
    TEAL_DARK
);
switch_plate!(
    SWITCH_3_LIT,
    "poi_switch_3_lit",
    PIPS_3,
    LIME,
    CREAM,
    TEAL_DARK
);

/// A twin-relay pylon: a copper coil between steel caps, cold.
pub const RELAY: SpriteDef = SpriteDef {
    name: "poi_relay",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('w', STEEL_LIGHT),
        ('l', COPPER),
        ('c', RUST),
        ('u', MAROON),
    ],
    frames: &[&[
        "...oooooooooo...", //
        "...osswwwwsso...", //
        "...osssssssso...", //
        "...oooooooooo...", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "...osssssssso...", //
        "...oooooooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// A lit relay: the coil glows and arcs crackle off it.
pub const RELAY_LIT: SpriteDef = SpriteDef {
    name: "poi_relay_lit",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('w', STEEL_LIGHT),
        ('l', CREAM),
        ('c', GOLD),
        ('u', AMBER),
        ('z', GOLD),
        ('y', CREAM),
    ],
    frames: &[&[
        "...oooooooooo...", //
        "...osswzywsso...", //
        "...osssssssso...", //
        "...oooooooooo...", //
        "....olccccco....", //
        "..z.ocuuuuuo....", //
        ".y..olccccco.z..", //
        "....ocuuuuuo..y.", //
        "..z.olccccco....", //
        ".y..ocuuuuuo.z..", //
        "....olccccco..y.", //
        "....ocuuuuuo....", //
        "....olccccco....", //
        "....ocuuuuuo....", //
        "...osssssssso...", //
        "...oooooooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// A vault's prize: an ornate reliquary with a glowing gem.
pub const VAULT_CACHE: SpriteDef = SpriteDef {
    name: "poi_vault_cache",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('c', CREAM),
        ('v', VIOLET),
        ('k', PLUM),
        ('p', LAVENDER),
        ('l', PINK),
        ('w', BONE),
    ],
    frames: &[&[
        ".......oo.......", //
        "...oooooooooo...", //
        "...ocgggggggo...", //
        "...ogaaaaaaao...", //
        ".oooooooooooooo.", //
        ".ocgggggggggggo.", //
        ".ogaaaaaaaaaaao.", //
        ".ogkvvvppvvvkao.", //
        ".ogkvvwllpvvkao.", //
        ".ogkvplwllpvkao.", //
        ".ogkvpllllpvkao.", //
        ".ogkvvpllpvvkao.", //
        ".ogkkvvppvvkkao.", //
        ".ogaaaaaaaaaaao.", //
        ".oaaaaaaaaaaaao.", //
        ".oooooooooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A rival outpost's locker: teal steel with vents, a handle and a logo plate.
pub const LOCKER: SpriteDef = SpriteDef {
    name: "poi_locker",
    legend: &[
        ('o', INK),
        ('k', STEEL_DARK),
        ('m', MINT),
        ('t', TEAL),
        ('d', TEAL_DARK),
        ('g', GOLD),
        ('c', CREAM),
        ('b', BONE),
        ('r', RED),
    ],
    frames: &[&[
        "oooooooooooooooo", //
        "okkkkkkkkkkkkkko", //
        "okmmmmmmmmmmmmko", //
        "okmtttttttttttko", //
        "okmtddddddddttko", //
        "okmtttttttttttko", //
        "okmtddddddddttko", //
        "okmtttttttttttko", //
        "okmtddddddddttko", //
        "okmtttttttttttko", //
        "okmbbbbttttcgtko", //
        "okmbrrbttttggtko", //
        "okmbbbbttttggtko", //
        "okmtttttttttdtko", //
        "okkkkkkkkkkkkkko", //
        "oooooooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A small ammonite imprint in fossil shale.
pub const SHELL_IMPRINT: SpriteDef = SpriteDef {
    name: "poi_shell_imprint",
    legend: &[('o', INK), ('b', BONE), ('s', SAND), ('f', FOG)],
    frames: &[&[
        ".ooo..", //
        "obbbo.", //
        "obsobo", //
        "obobso", //
        "osbbfo", //
        ".oooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A small bone imprint in fossil shale.
pub const BONE_IMPRINT: SpriteDef = SpriteDef {
    name: "poi_bone_imprint",
    legend: &[('o', INK), ('b', BONE), ('f', FOG)],
    frames: &[&[
        "oo....", //
        "obo...", //
        ".obo..", //
        "..obo.", //
        "...obo", //
        "....oo", //
    ]],
    frame_seconds: 0.0,
};

/// Map and collection-log icon of a supply cache.
pub const ICON_CACHE: SpriteDef = SpriteDef {
    name: "icon_poi_cache",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', LOAM),
        ('d', SOIL),
        ('s', STEEL_LIGHT),
        ('g', GOLD),
    ],
    frames: &[&[
        ".........", //
        "ooooooooo", //
        "oslllllso", //
        "olmmdmmdo", //
        "olmgggmdo", //
        "olmmdmmdo", //
        "osddddddo", //
        "ooooooooo", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

/// Map and collection-log icon of a derelict rig (a crashed, tilted rig).
pub const ICON_DERELICT: SpriteDef = SpriteDef {
    name: "icon_poi_derelict",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('s', STEEL),
        ('t', MINT),
        ('r', RUST),
    ],
    frames: &[&[
        "....oo...", //
        "...ogto..", //
        "..ogggao.", //
        ".ogggaao.", //
        "ogrgaaso.", //
        "osgaasso.", //
        ".ossrso..", //
        "..ooooso.", //
        "......oo.", //
    ]],
    frame_seconds: 0.0,
};

/// Map and collection-log icon of a vault (a brass-framed door with its ring lock).
pub const ICON_VAULT: SpriteDef = SpriteDef {
    name: "icon_poi_vault",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('m', STEEL),
        ('d', STEEL_DARK),
        ('k', NIGHT),
    ],
    frames: &[&[
        "ooooooooo", //
        "ogggggggo", //
        "ogmdddmao", //
        "ogdgggdao", //
        "ogdgkgdao", //
        "ogdgggdao", //
        "ogmdddmao", //
        "oaaaaaaao", //
        "ooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// Map and collection-log icon of a formation (a cracked geode).
pub const ICON_FORMATION: SpriteDef = SpriteDef {
    name: "icon_poi_formation",
    legend: &[
        ('o', INK),
        ('s', STONE_LIGHT),
        ('d', STONE),
        ('v', VIOLET),
        ('p', LAVENDER),
        ('w', PINK),
    ],
    frames: &[&[
        "..ooooo..", //
        ".osssddo.", //
        "osspwvddo", //
        "ospwpvvdo", //
        "osvpwpvdo", //
        "osdvpvddo", //
        "osddvdddo", //
        ".oddddo..", //
        "..oooo...", //
    ]],
    frame_seconds: 0.0,
};

/// Map and collection-log icon of a rival claim (an outpost mast).
pub const ICON_OUTPOST: SpriteDef = SpriteDef {
    name: "icon_poi_outpost",
    legend: &[
        ('o', INK),
        ('t', TEAL),
        ('m', MINT),
        ('s', STEEL),
        ('r', RED),
    ],
    frames: &[&[
        "....r....", //
        "...ooo...", //
        "...oso...", //
        "..osmso..", //
        "..ostso..", //
        ".ossssso.", //
        ".otmtmto.", //
        "ottttttto", //
        "ooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A scanner's "strange echo": something is there, but the scanner cannot tell what.
pub const ICON_ECHO: SpriteDef = SpriteDef {
    name: "icon_poi_echo",
    legend: &[('o', INK), ('m', MINT), ('t', TEAL), ('k', TEAL_DARK)],
    frames: &[
        &[
            "..ooooo..", //
            ".okmmmko.", //
            "okmkkkmko", //
            "okkkkmmko", //
            "okkkmmkko", //
            "okkkkkkko", //
            ".okkmkko.", //
            "..ooooo..", //
            ".........", //
        ],
        &[
            "..ooooo..", //
            ".oktttko.", //
            "oktkkktko", //
            "okkkkttko", //
            "okkkttkko", //
            "okkkkkkko", //
            ".okktkko.", //
            "..ooooo..", //
            ".........", //
        ],
    ],
    frame_seconds: 0.5,
};

/// The colour that marks a POI kind on maps, previews and scanner brackets.
#[must_use]
pub const fn poi_color(kind: PoiKind) -> raylib::prelude::Color {
    match kind {
        PoiKind::SupplyCache => SAND,
        PoiKind::DerelictRig => GOLD,
        PoiKind::Vault => AMBER,
        PoiKind::Formation => LAVENDER,
        PoiKind::RivalClaim => MINT,
    }
}

/// The icon of a POI kind.
#[must_use]
pub const fn poi_icon(kind: PoiKind) -> &'static SpriteDef {
    match kind {
        PoiKind::SupplyCache => &ICON_CACHE,
        PoiKind::DerelictRig => &ICON_DERELICT,
        PoiKind::Vault => &ICON_VAULT,
        PoiKind::Formation => &ICON_FORMATION,
        PoiKind::RivalClaim => &ICON_OUTPOST,
    }
}

/// The map icon for a POI the player knows `knowledge` about.
#[must_use]
pub const fn known_poi_icon(kind: PoiKind, knowledge: PoiKnowledge) -> &'static SpriteDef {
    match knowledge {
        PoiKnowledge::Echo => &ICON_ECHO,
        PoiKnowledge::Identified | PoiKnowledge::Visited => poi_icon(kind),
    }
}
