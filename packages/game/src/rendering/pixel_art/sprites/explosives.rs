//! Explosive charges: one world sprite per [`fatherload_simulation::ExplosiveKind`] (the plain
//! bomb lives in `world::BOMB`) and a 9x9 HUD/shop icon per carried kind (`icons::BOMB` for
//! plain bombs).

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A bomb smeared in green adhesive gel.
pub const STICKY: SpriteDef = SpriteDef {
    name: "world_sticky_bomb",
    legend: &[
        ('o', INK),
        ('n', SLATE_DARK),
        ('S', STEEL),
        ('L', LIME),
        ('l', LEAF),
        ('m', MOSS),
        ('g', GOLD),
    ],
    frames: &[&[
        ".....g....", //
        ".....o....", //
        "....oSo...", //
        "..oooSooo.", //
        ".oLLLnnnno", //
        "oLllLLnnno", //
        "olnnllLnno", //
        "onnnnnlLlo", //
        "omnnnnnmlo", //
        ".omlnnnmo.", //
        "..oLoomoo.", //
        "...m..L...", //
    ]],
    frame_seconds: 0.0,
};

/// A cluster bomb: an orange-banded shell scored into bomblet panels.
pub const CLUSTER: SpriteDef = SpriteDef {
    name: "world_cluster_bomb",
    legend: &[
        ('o', INK),
        ('n', SLATE_DARK),
        ('w', SLATE_LIGHT),
        ('r', ORANGE),
        ('a', AMBER),
        ('S', STEEL),
        ('g', GOLD),
    ],
    frames: &[&[
        "....g.....", //
        "....o.....", //
        "...oSo....", //
        ".oooSoooo.", //
        "onwnonnnno", //
        "orrrarrrro", //
        "onnnonnnno", //
        "onnnonnnno", //
        "orrrarrrro", //
        ".onnonnno.", //
        "..oooooo..", //
    ]],
    frame_seconds: 0.0,
};

/// One cluster bomblet.
pub const BOMBLET: SpriteDef = SpriteDef {
    name: "world_bomblet",
    legend: &[
        ('o', INK),
        ('n', SLATE_DARK),
        ('r', ORANGE),
        ('w', SLATE_LIGHT),
    ],
    frames: &[&[
        ".ooo.", //
        "ownno", //
        "orrro", //
        "onnno", //
        ".ooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A C4 brick: putty block, black tape and a detonator box (its LED blinks separately).
pub const C4: SpriteDef = SpriteDef {
    name: "world_c4",
    legend: &[
        ('o', INK),
        ('l', CREAM),
        ('m', SAND),
        ('d', CLAY),
        ('t', SLATE_DARK),
        ('k', STEEL_DARK),
        ('s', STEEL),
        ('R', RED),
        ('B', SKY),
    ],
    frames: &[&[
        "...R....B...", //
        "...o....o...", //
        "oooooooooooo", //
        "olmmmmmmmmdo", //
        "omkkkkkkmmdo", //
        "ttksskkkttto", //
        "omkkkkkkmmdo", //
        "ommmmmmmmddo", //
        "oddddddddddo", //
        "oooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A shaped charge: a steel cone whose point faces the way it cuts (drawn pointing down; the
/// renderer turns it to face its blast direction).
pub const SHAPED: SpriteDef = SpriteDef {
    name: "world_shaped_charge",
    legend: &[
        ('o', INK),
        ('W', STEEL_LIGHT),
        ('S', STEEL),
        ('d', STEEL_DARK),
        ('R', RED),
        ('g', GOLD),
    ],
    frames: &[&[
        "..oooooo..", //
        ".oWWWWWSo.", //
        ".oRRRRRRo.", //
        ".oWSSSSdo.", //
        "..oSSSdo..", //
        "..oWSSdo..", //
        "...oSdo...", //
        "...oSdo...", //
        "....og....", //
        "....oo....", //
    ]],
    frame_seconds: 0.0,
};

/// A mining charge: a squat hazard-striped barrel.
pub const MINING: SpriteDef = SpriteDef {
    name: "world_mining_charge",
    legend: &[
        ('o', INK),
        ('a', AMBER),
        ('g', GOLD),
        ('k', SLATE_DARK),
        ('S', STEEL),
        ('W', STEEL_LIGHT),
        ('R', RED),
    ],
    frames: &[&[
        "....oRo.....", //
        "..ooooooooo.", //
        ".oWSSSSSSSSo", //
        ".oggakkaakko", //
        ".oakkaakkaao", //
        ".okkaakkaako", //
        ".ogaaaaaaaao", //
        ".oWSSSSSSSSo", //
        ".oggakkaakko", //
        ".oakkaakkaao", //
        "..ooooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// An incendiary canister with a flame stencil.
pub const INCENDIARY: SpriteDef = SpriteDef {
    name: "world_incendiary",
    legend: &[
        ('o', INK),
        ('S', STEEL),
        ('W', STEEL_LIGHT),
        ('M', MAROON),
        ('R', RED),
        ('r', ORANGE),
        ('g', GOLD),
    ],
    frames: &[&[
        "...oo...", //
        "..oWSo..", //
        ".oooooo.", //
        "oRRRRRMo", //
        "oRRgRRMo", //
        "oRgrgRMo", //
        "oRrgrRMo", //
        "oRRrRRMo", //
        "oRRRRRMo", //
        "oWSSSSSo", //
        ".oooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// An EMP charge: a teal core wound with copper coil.
pub const EMP: SpriteDef = SpriteDef {
    name: "world_emp",
    legend: &[
        ('o', INK),
        ('m', MINT),
        ('t', TEAL),
        ('d', TEAL_DARK),
        ('c', COPPER),
        ('i', ICE),
    ],
    frames: &[&[
        "...oooo...", //
        "..otmmto..", //
        ".occcccco.", //
        "otmimmmtdo", //
        "occcccccco", //
        "otmmmmttdo", //
        ".occcccco.", //
        "..odttdo..", //
        "...oooo...", //
    ]],
    frame_seconds: 0.0,
};

/// A road flare: a red stick with a striker cap (the flame is drawn as light).
pub const FLARE: SpriteDef = SpriteDef {
    name: "world_flare",
    legend: &[
        ('o', INK),
        ('R', RED),
        ('M', MAROON),
        ('c', CREAM),
        ('k', SLATE_DARK),
    ],
    frames: &[&[
        ".oo.", //
        "okko", //
        "oRMo", //
        "ocMo", //
        "oRMo", //
        "oRMo", //
        "ocMo", //
        "oRMo", //
        "oRMo", //
        ".oo.", //
    ]],
    frame_seconds: 0.0,
};

/// A smoke canister: a squat grey can with vent holes and a pull ring.
pub const SMOKE: SpriteDef = SpriteDef {
    name: "world_smoke",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('w', STEEL_LIGHT),
        ('f', FOG),
        ('m', MIST),
        ('g', GOLD),
    ],
    frames: &[&[
        "...gg...", //
        "..g..g..", //
        ".oooooo.", //
        "oSwwwSso", //
        "osfofoso", //
        "oSSSSSso", //
        "osfofoso", //
        "oSmmmSso", //
        "osssssso", //
        ".oooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A scare charge: a fat red banger in yellow hazard stripes with a short wick.
pub const SCARE: SpriteDef = SpriteDef {
    name: "world_scare",
    legend: &[
        ('o', INK),
        ('R', RED),
        ('M', MAROON),
        ('y', GOLD),
        ('a', AMBER),
        ('c', CREAM),
        ('k', SLATE_DARK),
    ],
    frames: &[&[
        "....c...", //
        "....k...", //
        "..oooo..", //
        ".oRRRMo.", //
        ".oyyyao.", //
        ".oRRRMo.", //
        ".oyyyao.", //
        ".oRRRMo.", //
        ".oyyyao.", //
        ".oRRRMo.", //
        "..oooo..", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_SMOKE: SpriteDef = SpriteDef {
    name: "icon_smoke",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('f', FOG),
        ('m', MIST),
        ('b', BONE),
    ],
    frames: &[&[
        "...mbm...", //
        "..mbbfm..", //
        "...mfm...", //
        "..ooooo..", //
        ".oSSSSso.", //
        ".osfofso.", //
        ".oSSSSso.", //
        ".osfofso.", //
        "..ooooo..", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_SCARE: SpriteDef = SpriteDef {
    name: "icon_scare",
    legend: &[
        ('o', INK),
        ('R', RED),
        ('M', MAROON),
        ('y', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        "c...c...c", //
        ".c..c..c.", //
        "...ooo...", //
        "..oRRMo..", //
        "..oyyyo..", //
        "..oRRMo..", //
        "..oyyyo..", //
        "..oRRMo..", //
        "...ooo...", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_STICKY: SpriteDef = SpriteDef {
    name: "icon_sticky_bomb",
    legend: &[
        ('o', INK),
        ('k', SLATE),
        ('L', LIME),
        ('l', LEAF),
        ('g', GOLD),
    ],
    frames: &[&[
        ".....o.g.", //
        "....o....", //
        "..oooo...", //
        ".oLLkko..", //
        "oLlLLkko.", //
        "okklLLlo.", //
        "okkkklLo.", //
        ".oLkkLo..", //
        "..oolo...", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_CLUSTER: SpriteDef = SpriteDef {
    name: "icon_cluster_bomb",
    legend: &[('o', INK), ('k', SLATE), ('r', ORANGE), ('g', GOLD)],
    frames: &[&[
        "....o.g..", //
        "...o.....", //
        ".ooooo...", //
        "okkokko..", //
        "orrorro.o", //
        "okkokkoor", //
        "orrorro.o", //
        ".ooooo.o.", //
        ".......or", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_C4: SpriteDef = SpriteDef {
    name: "icon_c4",
    legend: &[
        ('o', INK),
        ('m', SAND),
        ('d', CLAY),
        ('k', STEEL_DARK),
        ('R', RED),
    ],
    frames: &[&[
        "..R......", //
        "..o......", //
        "ooooooooo", //
        "ommmmmmdo", //
        "okkkkmmdo", //
        "okRkkmmdo", //
        "ommmmmmdo", //
        "odddddddo", //
        "ooooooooo", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_SHAPED: SpriteDef = SpriteDef {
    name: "icon_shaped_charge",
    legend: &[
        ('o', INK),
        ('W', STEEL_LIGHT),
        ('S', STEEL),
        ('R', RED),
        ('g', GOLD),
    ],
    frames: &[&[
        "ooooooooo", //
        "oWWWWWWSo", //
        "oRRRRRRRo", //
        ".oWSSSSo.", //
        "..oWSSo..", //
        "...oSo...", //
        "....o....", //
        "....g....", //
        "...g.g...", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_MINING: SpriteDef = SpriteDef {
    name: "icon_mining_charge",
    legend: &[
        ('o', INK),
        ('a', AMBER),
        ('k', SLATE_DARK),
        ('S', STEEL),
        ('R', RED),
    ],
    frames: &[&[
        "...oRo...", //
        ".ooooooo.", //
        "oSSSSSSSo", //
        "oakkaakko", //
        "okkaakkao", //
        "oaaaaaaao", //
        "oSSSSSSSo", //
        "oakkaakko", //
        ".ooooooo.", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_INCENDIARY: SpriteDef = SpriteDef {
    name: "icon_incendiary",
    legend: &[
        ('o', INK),
        ('S', STEEL),
        ('R', RED),
        ('M', MAROON),
        ('r', ORANGE),
        ('g', GOLD),
    ],
    frames: &[&[
        "...ooo...", //
        "...oSo...", //
        ".ooooooo.", //
        ".oRRgRMo.", //
        ".oRgrgMo.", //
        ".ogrrrgo.", //
        ".oRgrRMo.", //
        ".oRRRRMo.", //
        ".ooooooo.", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_EMP: SpriteDef = SpriteDef {
    name: "icon_emp",
    legend: &[
        ('o', INK),
        ('m', MINT),
        ('t', TEAL),
        ('c', COPPER),
        ('i', ICE),
    ],
    frames: &[&[
        "..ooooo..", //
        ".otmimto.", //
        "occccccco", //
        "otmmmmmto", //
        "occccccco", //
        "otmmmmmto", //
        ".occccco.", //
        "..ooooo..", //
        "i.......i", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_FLARE: SpriteDef = SpriteDef {
    name: "icon_flare",
    legend: &[
        ('o', INK),
        ('R', RED),
        ('M', MAROON),
        ('c', CREAM),
        ('g', GOLD),
        ('a', AMBER),
    ],
    frames: &[&[
        "......gc.", //
        ".....gac.", //
        "....oRa..", //
        "...oRMo..", //
        "..oRMo...", //
        ".oRMo....", //
        "oRMo.....", //
        "oMo......", //
        ".o.......", //
    ]],
    frame_seconds: 0.0,
};

/// The HUD/shop icon of a carried explosive kind.
#[must_use]
pub const fn icon(kind: fatherload_simulation::ExplosiveKind) -> &'static SpriteDef {
    use fatherload_simulation::ExplosiveKind;
    match kind {
        ExplosiveKind::Bomb | ExplosiveKind::Bomblet | ExplosiveKind::Rocket => &super::icons::BOMB,
        ExplosiveKind::Sticky => &ICON_STICKY,
        ExplosiveKind::Cluster => &ICON_CLUSTER,
        ExplosiveKind::C4 => &ICON_C4,
        ExplosiveKind::Shaped => &ICON_SHAPED,
        ExplosiveKind::Mining => &ICON_MINING,
        ExplosiveKind::Incendiary => &ICON_INCENDIARY,
        ExplosiveKind::Emp => &ICON_EMP,
        ExplosiveKind::Flare => &ICON_FLARE,
        ExplosiveKind::Smoke => &ICON_SMOKE,
        ExplosiveKind::Scare => &ICON_SCARE,
    }
}
