//! Sprites of the Moon (WP-MOON): its creatures and machines, the Warden, the Moon gear icons,
//! lunar relics and the Moon weapons.

#[allow(
    clippy::wildcard_imports,
    reason = "sprite legends name colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A regolith crawler: a pale, many-legged leaper with a dark-glassed head, facing right
/// (scuttle A/B; it crouches on frame B before a leap).
pub const REGOLITH_CRAWLER: SpriteDef = SpriteDef {
    name: "enemy_regolith_crawler",
    legend: &[
        ('o', INK),
        ('f', FOG),
        ('m', MIST),
        ('b', BONE),
        ('s', SLATE),
        ('e', TEAL),
        ('c', MINT),
    ],
    frames: &[
        &[
            "......oooooo........", //
            "....oommmmmmoo......", //
            "...ommmbbmmmmmooo...", //
            "..ommfmmmmmfmmmeco..", //
            ".ommfffmmmffmmmeeco.", //
            "ommmmmmmmmmmmmmmmo..", //
            "ossssssssssssssso...", //
            ".oo.o.o.oo.o.o.oo...", //
            ".o..o..o..o..o..o...", //
            "o..o..o....o..o..o..", //
        ],
        &[
            "....................", //
            "......oooooo........", //
            "....oommmmmmoo......", //
            "...ommmbbmmmmmooo...", //
            "..ommfmmmmmfmmmeco..", //
            ".ommfffmmmffmmmeeco.", //
            "ommmmmmmmmmmmmmmmo..", //
            "ossssssssssssssso...", //
            "oo.oo.oo..oo.oo.oo..", //
            "o.o..o..oo..o..o.o..", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A crystal mite: a tiny violet crystal shell on buzzing wings (flap A/B).
pub const CRYSTAL_MITE: SpriteDef = SpriteDef {
    name: "enemy_crystal_mite",
    legend: &[
        ('o', INK),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('p', PINK),
        ('w', ICE),
    ],
    frames: &[
        &[
            "w..oo..w.", //
            ".wovpvow.", //
            "..ovlpo..", //
            ".ovllpvo.", //
            "..ovvvo..", //
            "...ooo...", //
        ],
        &[
            ".........", //
            "ww.oo.ww.", //
            "..ovpvo..", //
            ".ovllpvo.", //
            "..ovvvo..", //
            "...ooo...", //
        ],
    ],
    frame_seconds: 0.05,
};

/// A derelict automaton: a rusted colony security robot, one red eye, facing right (step A/B).
pub const DERELICT_AUTOMATON: SpriteDef = SpriteDef {
    name: "enemy_derelict_automaton",
    legend: &[
        ('o', INK),
        ('d', STEEL_DARK),
        ('s', STEEL),
        ('S', STEEL_LIGHT),
        ('r', RUST),
        ('R', RED),
        ('a', ORANGE),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "......oooooo......", //
            ".....oSSSSSSo.....", //
            ".....osssRRao.....", //
            ".....osssssso.....", //
            "......oooooo......", //
            "...oooossssoooo...", //
            "..osSSssssssrrso..", //
            ".osSsyyssssssrsso.", //
            ".osssyyssssrrsso..", //
            ".oso.osssssssoso..", //
            ".oso.osssrsssoso..", //
            ".odo.oddddddoodo..", //
            "..o..osso.osso.o..", //
            ".....osso.osso....", //
            ".....odo...odo....", //
            "....oddo...oddo...", //
            "....oooo...oooo...", //
        ],
        &[
            "......oooooo......", //
            ".....oSSSSSSo.....", //
            ".....osssaRRo.....", //
            ".....osssssso.....", //
            "......oooooo......", //
            "...oooossssoooo...", //
            "..osSSssssssrrso..", //
            ".osSsyyssssssrsso.", //
            ".osssyyssssrrsso..", //
            ".oso.osssssssoso..", //
            ".oso.osssrsssoso..", //
            ".odo.oddddddoodo..", //
            "..o..osso..osso...", //
            ".....osso...osso..", //
            "....odo......odo..", //
            "...oddo.....oddo..", //
            "...oooo.....oooo..", //
        ],
    ],
    frame_seconds: 0.25,
};

/// The Warden: the alien facility's guardian, a ring of dark teal stone plates around a
/// burning mint eye, floating (pulse A/B).
pub const WARDEN: SpriteDef = SpriteDef {
    name: "enemy_warden",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('t', TEAL_DARK),
        ('T', TEAL),
        ('c', MINT),
        ('w', ICE),
        ('W', BONE),
        ('v', VIOLET),
    ],
    frames: &[
        &[
            "............oooooo............", //
            "..........oottttttoo..........", //
            "........ootnnnnnnnntoo........", //
            ".......otnnTTTTTTTTnnto.......", //
            "..oo..otnTTttttttttTTnto..oo..", //
            ".otto.otnTtoooooooottTnto.otto", //
            ".otTto.oTtoccccccccotTo.otTto.", //
            "..otTtootoccwwwwwwccotootTto..", //
            "...otnntocwwWWWWWWwwcotnnto...", //
            "....otnoccwWWvvvvWWwccontto...", //
            "....otnocwWWvvvvvvWWwcontto...", //
            "....otnocwWWvvvvvvWWwconto....", //
            "....otnoccwWWvvvvWWwccontto...", //
            "...otnntocwwWWWWWWwwcotnnto...", //
            "..otTtootoccwwwwwwccotootTto..", //
            ".otTto.oTtoccccccccotTo.otTto.", //
            ".otto.otnTtoooooooottTnto.otto", //
            "..oo..otnTTttttttttTTnto..oo..", //
            ".......otnnTTTTTTTTnnto.......", //
            "........ootnnnnnnnntoo........", //
            "..........oottttttoo..........", //
            "............oooooo............", //
        ],
        &[
            "............oooooo............", //
            "..........oottttttoo..........", //
            "........ootnnnnnnnntoo........", //
            ".......otnnTTTTTTTTnnto.......", //
            "......otnTTttttttttTTnto......", //
            ".oo...otnTtoooooooottTnto..oo.", //
            "otto..oTtoccccccccccotTo..otto", //
            "otTto.otoccwwwwwwwwccoto.otTto", //
            ".otTtootcwwWWWWWWWWwwctootTto.", //
            "..otnnocwWWvvvvvvvvWWwconnto..", //
            "...otnocwWvvvvvvvvvvWwcontto..", //
            "...otnocwWvvvvvvvvvvWwconto...", //
            "..otnnocwWWvvvvvvvvWWwconnto..", //
            ".otTtootcwwWWWWWWWWwwctootTto.", //
            "otTto.otoccwwwwwwwwccoto.otTto", //
            "otto..oTtoccccccccccotTo..otto", //
            ".oo...otnTtoooooooottTnto..oo.", //
            "......otnTTttttttttTTnto......", //
            ".......otnnTTTTTTTTnnto.......", //
            "........ootnnnnnnnntoo........", //
            "..........oottttttoo..........", //
            "............oooooo............", //
        ],
    ],
    frame_seconds: 0.35,
};

/// Pressurized hull: a riveted plate sealed with a gasket ring.
pub const PRESSURIZED_HULL: SpriteDef = SpriteDef {
    name: "icon_pressurized_hull",
    legend: &[
        ('o', INK),
        ('k', STEEL),
        ('w', STEEL_LIGHT),
        ('a', ORANGE),
        ('s', SKY_LIGHT),
    ],
    frames: &[&[
        ".ooooooo.", //
        "owwwwwwko", //
        "owaaaaako", //
        "owakskako", //
        "owassskao", //
        "owakskako", //
        "owaaaaako", //
        "okkkkkkko", //
        ".ooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// Life support: an air tank with a green bubble.
pub const LIFE_SUPPORT: SpriteDef = SpriteDef {
    name: "icon_life_support",
    legend: &[
        ('o', INK),
        ('k', STEEL),
        ('w', STEEL_LIGHT),
        ('l', LIME),
        ('g', LEAF),
        ('i', ICE),
    ],
    frames: &[&[
        "...ooo...", //
        "..okwko..", //
        ".owwwwko.", //
        ".owiilko.", //
        ".owilgko.", //
        ".owwlgko.", //
        ".owwggko.", //
        ".okkkkko.", //
        "..ooooo..", //
    ]],
    frame_seconds: 0.0,
};

/// A lunar relic: a small tablet of pale stone carved with a mint glyph.
pub const LUNAR_RELIC: SpriteDef = SpriteDef {
    name: "artifact_lunar_relic",
    legend: &[
        ('o', INK),
        ('f', FOG),
        ('m', MIST),
        ('c', MINT),
        ('t', TEAL),
    ],
    frames: &[&[
        ".oooooooo.", //
        "ommmmmmmfo", //
        "omcctccmfo", //
        "omctmmcmfo", //
        "omcmcctmfo", //
        "omctccmmfo", //
        "offfffffo.", //
        ".ooooooo..", //
    ]],
    frame_seconds: 0.0,
};

/// The Warden's eye: a burning violet orb in a mint ring.
pub const WARDEN_EYE: SpriteDef = SpriteDef {
    name: "artifact_warden_eye",
    legend: &[
        ('o', INK),
        ('c', MINT),
        ('w', ICE),
        ('v', VIOLET),
        ('p', PINK),
    ],
    frames: &[&[
        "...oooo...", //
        ".oocccco..", //
        ".ocwwwwco.", //
        "ocwvvpvwco", //
        "ocwvppvwco", //
        ".ocwwwwco.", //
        ".oocccco..", //
        "...oooo...", //
    ]],
    frame_seconds: 0.0,
};

/// The Lunar Cutter: the plasma cutter with an alloy focusing ring.
pub const LUNAR_CUTTER: SpriteDef = SpriteDef {
    name: "weapon_lunar_cutter",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('S', STEEL),
        ('l', LAVENDER),
        ('c', MINT),
        ('w', ICE),
    ],
    frames: &[&[
        ".ooooooo....", //
        "oSsSsSslloo.", //
        "osSsSsSllcwo", //
        "oSsSsSsllcco", //
        ".oooooooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// The Low-G Mortar: a stubby wide tube canted up.
pub const LOW_G_MORTAR: SpriteDef = SpriteDef {
    name: "weapon_low_g_mortar",
    legend: &[
        ('o', INK),
        ('d', STEEL_DARK),
        ('s', STEEL),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[&[
        "......oooo", //
        "....oossso", //
        "..oossssdo", //
        "oossaasdo.", //
        "osssaadoo.", //
        "oddddoo...", //
        ".oooo.....", //
    ]],
    frame_seconds: 0.0,
};

/// The Void Lance: a long pale-teal alien spear with a violet heart.
pub const VOID_LANCE: SpriteDef = SpriteDef {
    name: "weapon_void_lance",
    legend: &[
        ('o', INK),
        ('t', TEAL_DARK),
        ('T', TEAL),
        ('c', MINT),
        ('v', VIOLET),
        ('p', PINK),
    ],
    frames: &[&[
        ".ooooo..........", //
        "otTTTtooooooo...", //
        "otvpvTTTcccccooo", //
        "otTTTtooooooo...", //
        ".ooooo..........", //
    ]],
    frame_seconds: 0.0,
};

/// The Lunar Cutter's icon.
pub const ICON_LUNAR_CUTTER: SpriteDef = SpriteDef {
    name: "icon_lunar_cutter",
    legend: &[
        ('o', INK),
        ('S', STEEL),
        ('s', STEEL_DARK),
        ('l', LAVENDER),
        ('c', MINT),
    ],
    frames: &[&[
        ".........", //
        ".........", //
        ".oooooo..", //
        "oSsSslco.", //
        "osSsSlccc", //
        "oSsSslco.", //
        ".oooooo..", //
        ".........", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

/// The Low-G Mortar's icon.
pub const ICON_LOW_G_MORTAR: SpriteDef = SpriteDef {
    name: "icon_low_g_mortar",
    legend: &[('o', INK), ('s', STEEL), ('d', STEEL_DARK), ('a', ORANGE)],
    frames: &[&[
        "......oo.", //
        ".....osso", //
        "....ossdo", //
        "...osado.", //
        "..osado..", //
        ".osado...", //
        "oddoo....", //
        "ooo......", //
        ".........", //
    ]],
    frame_seconds: 0.0,
};

/// The Void Lance's icon.
pub const ICON_VOID_LANCE: SpriteDef = SpriteDef {
    name: "icon_void_lance",
    legend: &[('o', INK), ('T', TEAL), ('c', MINT), ('v', VIOLET)],
    frames: &[&[
        "........o", //
        ".......oc", //
        "......ocT", //
        ".....ocTo", //
        "....ovTo.", //
        "...ovvo..", //
        "..oTTo...", //
        ".oTTo....", //
        "oTo......", //
    ]],
    frame_seconds: 0.0,
};

/// A low-g mortar shell.
pub const MORTAR_SHELL: SpriteDef = SpriteDef {
    name: "weapon_mortar_shell",
    legend: &[('o', INK), ('s', STEEL), ('a', ORANGE)],
    frames: &[&[
        ".oo.", //
        "osao", //
        "osao", //
        ".oo.", //
    ]],
    frame_seconds: 0.0,
};

/// A moon icon (the HUD's lunar checklist and the colony's sign).
pub const MOON_ICON: SpriteDef = SpriteDef {
    name: "icon_moon_checklist",
    legend: &[
        ('o', INK),
        ('m', MIST),
        ('f', FOG),
        ('b', BONE),
        ('s', SLATE),
    ],
    frames: &[&[
        "..ooooo..", //
        ".obbmmmo.", //
        "obbmmsmfo", //
        "obmmmmmfo", //
        "ombsmmffo", //
        "ommmmmffo", //
        "ommmsfffo", //
        ".offfffo.", //
        "..ooooo..", //
    ]],
    frame_seconds: 0.0,
};
