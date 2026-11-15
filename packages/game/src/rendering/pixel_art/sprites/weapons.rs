//! Hardpoint weapons: one world sprite per [`fatherload_simulation::WeaponKind`] (drawn on the
//! rig's hardpoint, pointing right, rotated toward the aim; the pivot is the left-middle art
//! pixel), a 9x9 HUD/shop icon per weapon, projectiles and the practice target.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;
use fatherload_simulation::WeaponKind;

/// A rivet gun: a short steel barrel with a magazine drum.
pub const RIVET_GUN: SpriteDef = SpriteDef {
    name: "weapon_rivet_gun",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('w', STEEL_LIGHT),
        ('g', GOLD),
    ],
    frames: &[&[
        ".ooo......", //
        "oSwSoooooo", //
        "osSSSwwwSo", //
        "oSgSoooooo", //
        ".ooo......", //
    ]],
    frame_seconds: 0.0,
};

/// A scatter blaster: a stubby flared muzzle over a pump grip.
pub const SCATTER_BLASTER: SpriteDef = SpriteDef {
    name: "weapon_scatter_blaster",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
    ],
    frames: &[&[
        "........oo", //
        ".ooooooocS", //
        "osSSSSSSSo", //
        "orrcSSSSSo", //
        ".ooooooocS", //
        "........oo", //
    ]],
    frame_seconds: 0.0,
};

/// A plasma cutter: a ribbed emitter with a glowing violet lens.
pub const PLASMA_CUTTER: SpriteDef = SpriteDef {
    name: "weapon_plasma_cutter",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('p', PINK),
    ],
    frames: &[&[
        ".oooooo...", //
        "oSsSsSsoo.", //
        "osSsSsSvlo", //
        "oSsSsSsvpo", //
        ".oooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A rocket launcher: a fat olive tube with a warhead peeking out and a hazard band.
pub const ROCKET_LAUNCHER: SpriteDef = SpriteDef {
    name: "weapon_rocket_launcher",
    legend: &[
        ('o', INK),
        ('m', MOSS),
        ('p', PINE),
        ('l', LEAF),
        ('y', GOLD),
        ('r', RED),
    ],
    frames: &[&[
        ".oooooooooo.", //
        "omllmmmmmmlo", //
        "omyymmmmmmrr", //
        "opyypppppprr", //
        "oppppppppppo", //
        ".oooooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A harpoon gun: a crossbow-like frame with a barbed spear loaded.
pub const HARPOON: SpriteDef = SpriteDef {
    name: "weapon_harpoon",
    legend: &[
        ('o', INK),
        ('k', SOIL_DARK),
        ('b', LOAM),
        ('S', STEEL),
        ('w', STEEL_LIGHT),
    ],
    frames: &[&[
        "...o......", //
        "..obo...o.", //
        "obbbSSSwwo", //
        "..obo...o.", //
        "...o......", //
    ]],
    frame_seconds: 0.0,
};

/// A railgun: twin long rails with teal coils.
pub const RAILGUN: SpriteDef = SpriteDef {
    name: "weapon_railgun",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('t', TEAL),
        ('m', MINT),
    ],
    frames: &[&[
        ".ooooooooooooo", //
        "osSSSSSSSSSSSo", //
        "otmtmtm.......", //
        "osSSSSSSSSSSSo", //
        ".ooooooooooooo", //
    ]],
    frame_seconds: 0.0,
};

/// A rivet in flight.
pub const RIVET: SpriteDef = SpriteDef {
    name: "projectile_rivet",
    legend: &[('o', INK), ('w', STEEL_LIGHT), ('g', GOLD)],
    frames: &[&["owwg", "owwg"]],
    frame_seconds: 0.0,
};

/// A scatter pellet.
pub const PELLET: SpriteDef = SpriteDef {
    name: "projectile_pellet",
    legend: &[('a', AMBER), ('c', CREAM)],
    frames: &[&["ac", "aa"]],
    frame_seconds: 0.0,
};

/// A rocket with its fins.
pub const ROCKET: SpriteDef = SpriteDef {
    name: "projectile_rocket",
    legend: &[
        ('o', INK),
        ('m', MOSS),
        ('l', LEAF),
        ('r', RED),
        ('w', STEEL_LIGHT),
    ],
    frames: &[&[
        "o.....", //
        "omllwr", //
        "ommmwr", //
        "o.....", //
    ]],
    frame_seconds: 0.0,
};

/// A harpoon spear.
pub const SPEAR: SpriteDef = SpriteDef {
    name: "projectile_spear",
    legend: &[('o', INK), ('b', LOAM), ('S', STEEL), ('w', STEEL_LIGHT)],
    frames: &[&[
        ".......o.", //
        "bbbbbSSwo", //
        ".......o.", //
    ]],
    frame_seconds: 0.0,
};

/// A practice target: a red-and-white bullseye on a post with a little base.
pub const TARGET: SpriteDef = SpriteDef {
    name: "world_practice_target",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('M', MAROON),
        ('b', BONE),
        ('m', MIST),
        ('k', SOIL_DARK),
        ('l', LOAM),
    ],
    frames: &[&[
        "...ooooo...", //
        "..orrrrro..", //
        ".orbbbbbro.", //
        "orbrrrrrbro", //
        "orbrbbbrbro", //
        "orbrbMbrbro", //
        "orbrbbbrbro", //
        "orbrrrrrbro", //
        ".orbbbbbro.", //
        "..orrrrro..", //
        "...ooooo...", //
        ".....l.....", //
        ".....k.....", //
        ".....l.....", //
        "...okkko...", //
        "..ollllo...", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_RIVET_GUN: SpriteDef = SpriteDef {
    name: "icon_rivet_gun",
    legend: &[('o', INK), ('S', STEEL), ('w', STEEL_LIGHT), ('g', GOLD)],
    frames: &[&[
        ".........", //
        ".ooo.....", //
        "oSwSooooo", //
        "oSSSwwwSo", //
        "oSgSooooo", //
        ".oSo.....", //
        ".oSo.....", //
        ".ooo.....", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_SCATTER_BLASTER: SpriteDef = SpriteDef {
    name: "icon_scatter_blaster",
    legend: &[
        ('o', INK),
        ('S', STEEL),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
    ],
    frames: &[&[
        "......a.a", //
        ".......oo", //
        "ooooooocS", //
        "oSSSSSSSo", //
        "orrcSSSSo", //
        "ooooooocS", //
        ".......oo", //
        "......a.a", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_PLASMA_CUTTER: SpriteDef = SpriteDef {
    name: "icon_plasma_cutter",
    legend: &[
        ('o', INK),
        ('S', STEEL),
        ('s', STEEL_DARK),
        ('v', VIOLET),
        ('p', PINK),
    ],
    frames: &[&[
        ".........", //
        ".........", //
        ".oooooo..", //
        "oSsSsSvo.", //
        "osSsSsvpp", //
        "oSsSsSvo.", //
        ".oooooo..", //
        ".........", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_ROCKET_LAUNCHER: SpriteDef = SpriteDef {
    name: "icon_rocket_launcher",
    legend: &[
        ('o', INK),
        ('m', MOSS),
        ('p', PINE),
        ('y', GOLD),
        ('r', RED),
    ],
    frames: &[&[
        ".........", //
        ".oooooooo", //
        "ommmmmmrr", //
        "oyymmmmrr", //
        "oyypppppo", //
        ".oooooooo", //
        "..o......", //
        "..o......", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_HARPOON: SpriteDef = SpriteDef {
    name: "icon_harpoon",
    legend: &[('o', INK), ('b', LOAM), ('S', STEEL), ('w', STEEL_LIGHT)],
    frames: &[&[
        ".........", //
        "..o......", //
        ".obo...o.", //
        "obbbSSSwo", //
        ".obo...o.", //
        "..o......", //
        ".........", //
        ".........", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

pub const ICON_RAILGUN: SpriteDef = SpriteDef {
    name: "icon_railgun",
    legend: &[('o', INK), ('S', STEEL), ('t', TEAL), ('m', MINT)],
    frames: &[&[
        ".........", //
        ".........", //
        "ooooooooo", //
        "oSSSSSSSo", //
        "otmtm....", //
        "oSSSSSSSo", //
        "ooooooooo", //
        ".........", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

/// A capacitor cell (HUD energy and the shack's battery row).
pub const ICON_BATTERY: SpriteDef = SpriteDef {
    name: "icon_battery",
    legend: &[('o', INK), ('S', STEEL), ('m', MINT), ('t', TEAL)],
    frames: &[&[
        "...ooo...", //
        "..oSSSo..", //
        ".ooooooo.", //
        ".ommmmmo.", //
        ".otttmmo.", //
        ".ommmmmo.", //
        ".otttmmo.", //
        ".ommmmmo.", //
        ".ooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// A rocket round (ammo rows).
pub const ICON_ROCKET_AMMO: SpriteDef = SpriteDef {
    name: "icon_rocket_ammo",
    legend: &[
        ('o', INK),
        ('m', MOSS),
        ('l', LEAF),
        ('r', RED),
        ('a', AMBER),
    ],
    frames: &[&[
        "....o....", //
        "...ooo...", //
        "...oro...", //
        "...olo...", //
        "...omo...", //
        "...olo...", //
        "..oomoo..", //
        "..o.a.o..", //
        "....a....", //
    ]],
    frame_seconds: 0.0,
};

/// The world sprite of a mounted weapon.
#[must_use]
pub const fn mounted(weapon: WeaponKind) -> &'static SpriteDef {
    match weapon {
        WeaponKind::RivetGun => &RIVET_GUN,
        WeaponKind::ScatterBlaster => &SCATTER_BLASTER,
        WeaponKind::PlasmaCutter => &PLASMA_CUTTER,
        WeaponKind::RocketLauncher => &ROCKET_LAUNCHER,
        WeaponKind::Harpoon => &HARPOON,
        WeaponKind::Railgun => &RAILGUN,
    }
}

/// The HUD/shop icon of a weapon.
#[must_use]
pub const fn icon(weapon: WeaponKind) -> &'static SpriteDef {
    match weapon {
        WeaponKind::RivetGun => &ICON_RIVET_GUN,
        WeaponKind::ScatterBlaster => &ICON_SCATTER_BLASTER,
        WeaponKind::PlasmaCutter => &ICON_PLASMA_CUTTER,
        WeaponKind::RocketLauncher => &ICON_ROCKET_LAUNCHER,
        WeaponKind::Harpoon => &ICON_HARPOON,
        WeaponKind::Railgun => &ICON_RAILGUN,
    }
}

/// The in-flight sprite of a weapon's projectile (`None`: hitscan weapons have none).
#[must_use]
pub const fn projectile(weapon: WeaponKind) -> Option<&'static SpriteDef> {
    match weapon {
        WeaponKind::RivetGun => Some(&RIVET),
        WeaponKind::ScatterBlaster => Some(&PELLET),
        WeaponKind::RocketLauncher => Some(&ROCKET),
        WeaponKind::Harpoon => Some(&SPEAR),
        WeaponKind::PlasmaCutter | WeaponKind::Railgun => None,
    }
}
