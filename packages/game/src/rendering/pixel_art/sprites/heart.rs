//! Descent VIII, the Heart's creatures (Wave 10): one animated sprite per kind. Asymmetric
//! creatures face right (the renderer mirrors them). The Heart itself is drawn by the world
//! renderer (`rendering/world/heart.rs`) at full size; its sprite here is the gallery's icon.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A null moth: a dark moth whose wings are full of stars, pale eyes (wings up / down).
pub const NULL_MOTH: SpriteDef = SpriteDef {
    name: "enemy_null_moth",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('w', BONE),
        ('p', PLUM),
    ],
    frames: &[
        &[
            "oo.........oo", //
            "ovoo.....oovo", //
            "onvlo...olvno", //
            "onwnvo.ovnwno", //
            ".onnnvovnnno.", //
            "..oonpwpnoo..", //
            "....opppo....", //
            ".....ooo.....", //
            ".............", //
        ],
        &[
            ".............", //
            ".............", //
            "....oo.oo....", //
            ".oooovovoooo.", //
            "onwnnpwpnnwno", //
            "ovnvlopolvnvo", //
            ".oooopppoooo.", //
            "....oooo.....", //
            ".............", //
        ],
    ],
    frame_seconds: 0.09,
};

/// An upfall beetle: a black beetle hanging upside down, legs to the roof, its belly glowing
/// violet with dark matter (hang A / B, dropping with legs tucked).
pub const UPFALL_BEETLE: SpriteDef = SpriteDef {
    name: "enemy_upfall_beetle",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('s', SLATE_DARK),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('w', BONE),
        ('p', PLUM),
    ],
    frames: &[
        &[
            ".o..o...o..o....", //
            "..o.o...o.o.....", //
            "..ooooooooo.....", //
            ".onssnnnssno....", //
            "onvvpvvvvpvvo...", //
            "onlvvlvvlvvlno..", //
            "onvvvvvvvvvvnoo.", //
            ".onnpvvvvpnnowwo", //
            "..onnnnnnnnnoo..", //
            "...ooooooooo....", //
            "................", //
            "................", //
        ],
        &[
            "..o..o..o..o....", //
            "...o.o..o.o.....", //
            "..ooooooooo.....", //
            ".onssnnnssno....", //
            "onvvpvvvvpvvo...", //
            "onvvlvvlvvvlno..", //
            "onvvvvvvvvvvnoo.", //
            ".onnpvvvvpnnowwo", //
            "..onnnnnnnnnoo..", //
            "...ooooooooo....", //
            "................", //
            "................", //
        ],
        &[
            "................", //
            "................", //
            "...ooooooooo....", //
            "..onnnnnnnnnoo..", //
            ".onnpvvvvpnnowwo", //
            "onvvvvvvvvvvnoo.", //
            "onlvvlvvlvvlno..", //
            "onvvpvvvvpvvo...", //
            ".onssnnnssno....", //
            "..ooooooooo.....", //
            "...o.o..o.o.....", //
            "..o.o....o.o....", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A vein gnawer: a segmented violet worm with a crown of grinding teeth, facing right
/// (crawl A / B, lunging with its maw wide).
pub const VEIN_GNAWER: SpriteDef = SpriteDef {
    name: "enemy_vein_gnawer",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('s', STEEL_LIGHT),
        ('w', BONE),
    ],
    frames: &[
        &[
            ".................", //
            "..ooo.ooo.oooo...", //
            ".opvpopvpopvvpos.", //
            "opvlvpvlvpvlvvpos", //
            "onvvvnvvvnvvvvnws", //
            "onpvpnpvpnpvvpnos", //
            ".onnnonnnonnnnos.", //
            "..ooo.ooo.oooo...", //
            ".................", //
            ".................", //
        ],
        &[
            ".................", //
            "...ooo.ooo.oooo..", //
            "..opvpopvpopvvpos", //
            ".opvlvpvlvpvlvvpo", //
            "oonvvvnvvvnvvvvnw", //
            ".onpvpnpvpnpvvpno", //
            "..onnnonnnonnnnos", //
            "...ooo.ooo.oooo..", //
            ".................", //
            ".................", //
        ],
        &[
            "..............s..", //
            "..ooo.ooo.oooos.s", //
            ".opvpopvpopvvpo..", //
            "opvlvpvlvpvlvvpo.", //
            "onvvvnvvvnvvvvnww", //
            "onpvpnpvpnpvvpo..", //
            ".onnnonnnonnnno..", //
            "..ooo.ooo.oooos.s", //
            "..............s..", //
            ".................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// An orbiter: a small dark world with a pink ring round it and a hot core, turning
/// (three frames of its ring's spin).
pub const ORBITER: SpriteDef = SpriteDef {
    name: "enemy_orbiter",
    legend: &[
        ('o', INK),
        ('d', DEEP_BLUE),
        ('s', SLATE_DARK),
        ('r', ROSE),
        ('k', PINK),
        ('w', BONE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "...............", //
            ".....ooooo.....", //
            "....odsssdo....", //
            "...odsddddso...", //
            "rkkodddgdddokkr", //
            "..rrkkdwgkkrr..", //
            "...odddgddddo..", //
            "...odsdddsdo...", //
            "....odsssdo....", //
            ".....ooooo.....", //
            "...............", //
            "...............", //
            "...............", //
        ],
        &[
            "...............", //
            ".....ooooo.....", //
            "....odsssdo....", //
            "..rkodddddokr..", //
            ".r.odddgddokkr.", //
            "....kkdwgkk....", //
            ".rkkddddgddo.r.", //
            "..rkodsdsdokr..", //
            "....odsssdo....", //
            ".....ooooo.....", //
            "...............", //
            "...............", //
            "...............", //
        ],
        &[
            "...............", //
            ".....ooooo.....", //
            "...rkosssokr...", //
            "..r.odddddo.r..", //
            "....odddgdo....", //
            "...okdwgkko....", //
            "....odddgdo....", //
            "..r.odsdsdo.r..", //
            "...rkosssokr...", //
            ".....ooooo.....", //
            "...............", //
            "...............", //
            "...............", //
        ],
    ],
    frame_seconds: 0.12,
};

/// An accretion mote: a glowing pink ember trailing a tail (two flickers).
pub const ACCRETION_MOTE: SpriteDef = SpriteDef {
    name: "enemy_accretion_mote",
    legend: &[('r', ROSE), ('k', PINK), ('w', BONE), ('p', PLUM)],
    frames: &[
        &[
            ".......", //
            "...kk..", //
            "p.kwwk.", //
            ".rkwwk.", //
            "p.rkkr.", //
            "...rr..", //
            ".......", //
        ],
        &[
            ".......", //
            "..kkk..", //
            ".kwwwk.", //
            "prkwwk.", //
            ".prkkr.", //
            "p..r...", //
            ".......", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A horizon lurker: an eye on a rooted stalk, its dark iris ringed with pink light
/// (watching, blinking, glaring before its bolts).
pub const HORIZON_LURKER: SpriteDef = SpriteDef {
    name: "enemy_horizon_lurker",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('d', DEEP_BLUE),
        ('s', SLATE_DARK),
        ('k', PINK),
        ('w', BONE),
        ('p', PLUM),
    ],
    frames: &[
        &[
            "....oooooo....", //
            "...owwwwwwo...", //
            "..owwkookwwo..", //
            "..owkoooookwo.", //
            "..owkoowoookwo", //
            "..owwkoookwwo.", //
            "...owwwwwwwo..", //
            "....ooppooo...", //
            "......pp......", //
            "......pp......", //
            ".....oppo.....", //
            ".....osso.....", //
            "....osddso....", //
            "...osddddso...", //
            "..oooooooooo..", //
        ],
        &[
            "..............", //
            "....oooooo....", //
            "...onnnnnno...", //
            "..okkkkkkkko..", //
            "..onnnnnnnno..", //
            "...oooooooo...", //
            "..............", //
            "....ooppooo...", //
            "......pp......", //
            "......pp......", //
            ".....oppo.....", //
            ".....osso.....", //
            "....osddso....", //
            "...osddddso...", //
            "..oooooooooo..", //
        ],
        &[
            "....oooooo....", //
            "...okkkkkko...", //
            "..okkwwwwkko..", //
            "..okwwoowwko..", //
            "..okwowwowko..", //
            "..okkwwwwkko..", //
            "...okkkkkko...", //
            "....ooppooo...", //
            "......pp......", //
            "......pp......", //
            ".....oppo.....", //
            ".....osso.....", //
            "....osddso....", //
            "...osddddso...", //
            "..oooooooooo..", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A mercury eel: a long silver eel with a gold eye and a fin of liquid metal, facing right
/// (swim A / B, leaping with its jaw open).
pub const MERCURY_EEL: SpriteDef = SpriteDef {
    name: "enemy_mercury_eel",
    legend: &[
        ('o', INK),
        ('d', STEEL_DARK),
        ('s', STEEL),
        ('l', STEEL_LIGHT),
        ('w', BONE),
        ('g', GOLD),
        ('a', AMBER),
    ],
    frames: &[
        &[
            ".......................", //
            "..........ooo..........", //
            "o.......oolllooooo.....", //
            "do....oolsssslllsoo....", //
            "dso.oolssssssssssslooo.", //
            ".dsolsssddssssssssslgwo", //
            ".dsossdddddsssssssssslo", //
            "do..oodddddddddsssdoo..", //
            "o.....ooooooooooooo....", //
            ".......................", //
        ],
        &[
            ".......................", //
            "...........ooo.........", //
            ".......oooolllooooo....", //
            "o....oolssssslllssooo..", //
            "dso.olssssssssssssslgwo", //
            ".dsolsssddsssssssssslo.", //
            "dso.ssdddddssssssssoo..", //
            "o....oodddddddddssdo...", //
            "......ooooooooooooo....", //
            ".......................", //
        ],
        &[
            ".......................", //
            "..........ooo........o.", //
            "o.......oolllooooo..oa.", //
            "do....oolsssslllsoooaao", //
            "dso.oolssssssssssslgw..", //
            ".dsolsssddsssssssslo...", //
            ".dsossdddddssssssslooa.", //
            "do..oodddddddddsssdoaao", //
            "o.....ooooooooooooo..o.", //
            ".......................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A rail leech: a fat brass leech with a teal sucker, humping along a rail, facing right
/// (crawl A / B).
pub const RAIL_LEECH: SpriteDef = SpriteDef {
    name: "enemy_rail_leech",
    legend: &[
        ('o', INK),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('t', TEAL),
        ('m', MINT),
        ('r', RUST),
    ],
    frames: &[
        &[
            ".............", //
            "....ooooo....", //
            "..oocgagcoo..", //
            ".ocaagaaagcoo", //
            "orcccccccccmo", //
            "orrcrrcrrctmo", //
            ".oooooooooooo", //
        ],
        &[
            ".....ooo.....", //
            "...oocagco...", //
            "..ocaagaacoo.", //
            ".ocaccccccmo.", //
            "orccccccccctm", //
            "orrcrrcrrcrto", //
            ".oooooooooooo", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A slag skimmer: a flat ray over the metal, its belly glowing orange with slag, facing
/// right (glide A / B, glaring before it spits).
pub const SLAG_SKIMMER: SpriteDef = SpriteDef {
    name: "enemy_slag_skimmer",
    legend: &[
        ('o', INK),
        ('m', MAROON),
        ('r', RUST),
        ('n', ORANGE),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "................", //
            ".....oooooo.....", //
            "...oommrrmmoo...", //
            ".oomrrrrrrrrmoo.", //
            "omrrrrrrrrrrrwoo", //
            "o.oonnaaaanno..o", //
            "....oonnnnoo....", //
            "......oooo......", //
            "................", //
            "................", //
        ],
        &[
            "................", //
            "................", //
            ".....oooooo.....", //
            "..oooommmmoooo..", //
            "oommrrrrrrrrrwoo", //
            "..ooonnaaanno...", //
            "....oonnnnoo....", //
            "......oooo......", //
            "................", //
            "................", //
        ],
        &[
            "................", //
            ".....oooooo.....", //
            "...oommrrmmoo...", //
            ".oomrrrrrrrrmoo.", //
            "omrrrrrrrrrrrwoo", //
            "o.oonngggggno..o", //
            "....ongwwwgnoo..", //
            "......onnno.....", //
            ".......aaa......", //
            "........a.......", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A corona wisp: a tongue of the drive's gold light with a white-hot heart (three flickers).
pub const CORONA_WISP: SpriteDef = SpriteDef {
    name: "enemy_corona_wisp",
    legend: &[
        ('n', ORANGE),
        ('a', AMBER),
        ('g', GOLD),
        ('c', CREAM),
        ('w', BONE),
    ],
    frames: &[
        &[
            ".....a......", //
            "....aga.....", //
            "...agga.a...", //
            "...aggaaga..", //
            "..agcccgga..", //
            ".agccwwcga..", //
            ".agcwwwwcga.", //
            ".agcwwwwcga.", //
            ".naccwwccan.", //
            "..nagcccan..", //
            "...nnaaann..", //
            "....nnnn....", //
            "............", //
        ],
        &[
            "......a.....", //
            ".a...aga....", //
            ".aa..agga...", //
            "..aaaggga...", //
            "..agcccgga..", //
            ".agccwwccga.", //
            ".agcwwwwcga.", //
            ".agcwwwwcga.", //
            ".naccwwccan.", //
            "..nagcccan..", //
            "...nnaaann..", //
            "....nnnn....", //
            "............", //
        ],
        &[
            "....a.......", //
            "....ag...a..", //
            "...agga.aa..", //
            "..aaggggga..", //
            "..agcccgga..", //
            ".agccwcccga.", //
            ".agcwwwwcga.", //
            ".agcwwwwcga.", //
            ".naccwwccan.", //
            "..nagcccan..", //
            "...nnaaann..", //
            "....nnnn....", //
            "............", //
        ],
    ],
    frame_seconds: 0.1,
};

/// A cable warden: a builder guard machine on two legs, gold-plated, its lamp eye teal
/// (red before a volley), facing right (stride A / B, the volley's glare).
pub const CABLE_WARDEN: SpriteDef = SpriteDef {
    name: "enemy_cable_warden",
    legend: &[
        ('o', INK),
        ('d', STEEL_DARK),
        ('s', STEEL),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('t', TEAL),
        ('m', MINT),
        ('r', RED),
    ],
    frames: &[
        &[
            "......oooooooo......", //
            ".....ogggggggaoo....", //
            "....ogaaaaaaaatmo...", //
            "....ogaccccccatmo...", //
            "....ogaaaaaaaaaoo...", //
            ".....oooaaaaooo.....", //
            "...oodsssssssssoo...", //
            "..odssggsssggssssooo", //
            "..odsgaagsgaagsssdso", //
            "..odssggsssggsssssdo", //
            "...odssssssssssssoo.", //
            "....oddsssssssddo...", //
            ".....oooddddddoo....", //
            "......od.....do.....", //
            "......od.....do.....", //
            ".....ods....ods.....", //
            ".....ods....ods.....", //
            "....odso...odso.....", //
            "....odso...odso.....", //
            "...oggggo.oggggo....", //
            "...ooooooooooooo....", //
        ],
        &[
            "......oooooooo......", //
            ".....ogggggggaoo....", //
            "....ogaaaaaaaatmo...", //
            "....ogaccccccatmo...", //
            "....ogaaaaaaaaaoo...", //
            ".....oooaaaaooo.....", //
            "...oodsssssssssoo...", //
            "..odssggsssggssssooo", //
            "..odsgaagsgaagsssdso", //
            "..odssggsssggsssssdo", //
            "...odssssssssssssoo.", //
            "....oddsssssssddo...", //
            ".....oooddddddoo....", //
            ".......od...do......", //
            "......od.....do.....", //
            "......ods...ods.....", //
            ".....ods.....ods....", //
            ".....odso...odso....", //
            "....odso.....odso...", //
            "...oggggo...oggggo..", //
            "...oooooo...oooooo..", //
        ],
        &[
            "......oooooooo......", //
            ".....ogggggggaoo....", //
            "....ogaaaaaaaarro...", //
            "....ogaccccccarro...", //
            "....ogaaaaaaaaaoo...", //
            ".....oooaaaaooo.....", //
            "...oodsssssssssoooo.", //
            "..odssggsssggsssrrro", //
            "..odsgaagsgaagssrrro", //
            "..odssggsssggssssooo", //
            "...odssssssssssssoo.", //
            "....oddsssssssddo...", //
            ".....oooddddddoo....", //
            "......od.....do.....", //
            "......od.....do.....", //
            ".....ods....ods.....", //
            ".....ods....ods.....", //
            "....odso...odso.....", //
            "....odso...odso.....", //
            "...oggggo.oggggo....", //
            "...ooooooooooooo....", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A drive coil: a ring of gold windings round a teal-hot core, crackling (two pulses).
pub const DRIVE_COIL: SpriteDef = SpriteDef {
    name: "enemy_drive_coil",
    legend: &[
        ('o', INK),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('t', TEAL),
        ('m', MINT),
        ('w', BONE),
        ('d', STEEL_DARK),
    ],
    frames: &[
        &[
            ".......ooooooo.......", //
            ".....oocgcgcgcoo.....", //
            "....ocgagagagagco....", //
            "...ocgaoooooooagco...", //
            "..ocgao.......oagco..", //
            ".ocgao..ttttt..oagco.", //
            ".ogao..tmmmmmt..oago.", //
            "ocgao.tmmwwwmmt.oagco", //
            "oaga.tmmwwwwwmmt.agao", //
            "ocgao.tmwwwwwmt.oagco", //
            "oaga.tmmwwwwwmmt.agao", //
            "ocgao.tmmwwwmmt.oagco", //
            ".ogao..tmmmmmt..oago.", //
            ".ocgao..ttttt..oagco.", //
            "..ocgao.......oagco..", //
            "...ocgaoooooooagco...", //
            "....ocgagagagagco....", //
            ".....oocgcgcgcoo.....", //
            ".......ooooooo.......", //
            ".........ddd.........", //
            "..........d..........", //
        ],
        &[
            ".......ooooooo.......", //
            ".....oogcgcgcgoo.....", //
            "....ocagagagagaco....", //
            "...ocgaoooooooagco...", //
            "..ocgao.......oagco..", //
            ".ocgao..mmmmm..oagco.", //
            ".ogao..mwwwwwm..oago.", //
            "ocgao.mwwwwwwwm.oagco", //
            "oaga.mwwwwwwwwwm.agao", //
            "ocgao.wwwwwwwwm.oagco", //
            "oaga.mwwwwwwwwwm.agao", //
            "ocgao.mwwwwwwwm.oagco", //
            ".ogao..mwwwwwm..oago.", //
            ".ocgao..mmmmm..oagco.", //
            "..ocgao.......oagco..", //
            "...ocgaoooooooagco...", //
            "....ocagagagagaco....", //
            ".....oogcgcgcgoo.....", //
            ".......ooooooo.......", //
            ".........ddd.........", //
            "..........d..........", //
        ],
    ],
    frame_seconds: 0.15,
};

/// A heart spark: a white-hot star of the drive's light (two twinkles).
pub const HEART_SPARK: SpriteDef = SpriteDef {
    name: "enemy_heart_spark",
    legend: &[('a', AMBER), ('g', GOLD), ('c', CREAM), ('w', BONE)],
    frames: &[
        &[
            "...g....", //
            "...g....", //
            "..gcg...", //
            "ggcwcgg.", //
            "..gcg...", //
            "...g....", //
            "...a....", //
            "........", //
        ],
        &[
            "........", //
            ".a...a..", //
            "..gcg...", //
            "..cwc...", //
            "..gcg...", //
            ".a...a..", //
            "........", //
            "........", //
        ],
    ],
    frame_seconds: 0.07,
};

/// The Heart's icon (the gallery's; the fight draws the Star Drive itself at full size): a
/// blazing core in its gold cradle.
pub const STAR_HEART: SpriteDef = SpriteDef {
    name: "enemy_star_heart",
    legend: &[
        ('o', INK),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('r', CREAM),
        ('w', BONE),
    ],
    frames: &[&[
        ".....oooooo.....", //
        "...ooccccccoo...", //
        "..occaaaaaacco..", //
        ".ocaaggggggaaco.", //
        ".ocagrrrrrrgaco.", //
        "ocagrrwwwwrrgaco", //
        "ocagrwwwwwwrgaco", //
        "ocagrwwwwwwrgaco", //
        "ocagrwwwwwwrgaco", //
        "ocagrrwwwwrrgaco", //
        ".ocagrrrrrrgaco.", //
        ".ocaaggggggaaco.", //
        "..occaaaaaacco..", //
        "...ooccccccoo...", //
        ".....oooooo.....", //
        "................", //
    ]],
    frame_seconds: 0.0,
};
