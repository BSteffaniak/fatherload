//! The builders' buried city's creatures (Wave 10 ancients-a, Descent VII strata 1-3): one
//! animated sprite per kind, their shots, and the archivist's drone. Asymmetric creatures face
//! right (the renderer mirrors them).
//!
//! Generated rows are checked for equal widths (see the atlas test); edit with care.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A maintenance drone: a squat steel body on two rotors, a mint sensor band and two tool arms
/// (rotors A/B, hunting with its band red).
pub const MAINTENANCE_DRONE: SpriteDef = SpriteDef {
    name: "enemy_maintenance_drone",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('l', STEEL_LIGHT),
        ('y', GOLD),
        ('m', MINT),
        ('r', RED),
    ],
    frames: &[
        &[
            ".lll.....lll.", //
            "...o.....o...", //
            "..oooooooooo.", //
            ".ottttttttto.", //
            "otyommmmmoyto", //
            ".otsssssssto.", //
            "..ooooooooo..", //
            "...o.y.y.o...", //
            "...l.....l...", //
        ],
        &[
            "..ll.....ll..", //
            "...o.....o...", //
            "..oooooooooo.", //
            ".ottttttttto.", //
            "otyommmmmoyto", //
            ".otsssssssto.", //
            "..ooooooooo..", //
            "...o.y.y.o...", //
            "...l.....l...", //
        ],
        &[
            ".lll.....lll.", //
            "...o.....o...", //
            "..oooooooooo.", //
            ".ottttttttto.", //
            "otyorrrrroyto", //
            ".otsssssssto.", //
            "..ooooooooo..", //
            "...o.y.y.o...", //
            "...l.....l...", //
        ],
    ],
    frame_seconds: 0.06,
};

/// A scrap crawler: a rusty crab of salvage with a heap of scrap on its back, a red eye and a
/// pincer (scuttle A/B, lunging).
pub const SCRAP_CRAWLER: SpriteDef = SpriteDef {
    name: "enemy_scrap_crawler",
    legend: &[
        ('o', INK),
        ('r', RUST),
        ('c', COPPER),
        ('s', STEEL_DARK),
        ('t', STEEL_LIGHT),
        ('e', RED),
    ],
    frames: &[
        &[
            "....rrcc........", //
            "...rcsstcr......", //
            "..orrrrrrrro....", //
            ".orcccccccrroo..", //
            "orccccccccccreo.", //
            "orrrrrrrrrrrrcco", //
            ".oooooooooooo.t.", //
            ".s.s..s.s..s....", //
            "s...s....s...s..", //
        ],
        &[
            "....rrcc........", //
            "...rcsstcr......", //
            "..orrrrrrrro....", //
            ".orcccccccrroo..", //
            "orccccccccccreo.", //
            "orrrrrrrrrrrrcco", //
            ".oooooooooooo.t.", //
            "s...s....s...s..", //
            ".s.s..s.s..s....", //
        ],
        &[
            "....rrcc........", //
            "...rcsstcr......", //
            "..orrrrrrrro....", //
            ".orcccccccrroo..", //
            "orccccccccccreo.", //
            "orrrrrrrrrrrrcct", //
            ".oooooooooooo.tt", //
            ".s.s..s.s..s....", //
            "s...s....s...s..", //
        ],
    ],
    frame_seconds: 0.1,
};

/// A street sentinel: a tall steel walker whose head is a searchlight (cream; red when it
/// fires), a hazard band round its chest (stride A/B, firing).
pub const STREET_SENTINEL: SpriteDef = SpriteDef {
    name: "enemy_street_sentinel",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('l', STEEL_LIGHT),
        ('y', GOLD),
        ('w', CREAM),
        ('r', RED),
    ],
    frames: &[
        &[
            "....oooooo....", //
            "...ossssssoo..", //
            "...osttttswwo.", //
            "...osttttswwwo", //
            "...ossssssoo..", //
            "....oottoo....", //
            "..oossssssoo..", //
            ".ostttttttttso", //
            ".ostllllllltso", //
            ".ostyyyyyyytso", //
            ".ostttttttttso", //
            "..ossssssssoo.", //
            "...oso..oso...", //
            "...oto..oto...", //
            "...oto..oto...", //
            "...oto..oto...", //
            "..ottto.ottto.", //
            "..ooooo.ooooo.", //
        ],
        &[
            "....oooooo....", //
            "...ossssssoo..", //
            "...osttttswwo.", //
            "...osttttswwwo", //
            "...ossssssoo..", //
            "....oottoo....", //
            "..oossssssoo..", //
            ".ostttttttttso", //
            ".ostllllllltso", //
            ".ostyyyyyyytso", //
            ".ostttttttttso", //
            "..ossssssssoo.", //
            "...oso..oso...", //
            "...oto...oto..", //
            "...oto...oto..", //
            "...oto...oto..", //
            "..ottto..ottto", //
            "..ooooo..ooooo", //
        ],
        &[
            "....oooooo....", //
            "...ossssssoo..", //
            "...osttttsrro.", //
            "...osttttsrrro", //
            "...ossssssoo..", //
            "....oottoo....", //
            "..oossssssoo..", //
            ".ostttttttttso", //
            ".ostllllllltso", //
            ".ostyyyyyyytso", //
            ".ostttttttttso", //
            "..ossssssssoo.", //
            "...oso..oso...", //
            "...oto..oto...", //
            "...oto..oto...", //
            "...oto..oto...", //
            "..ottto.ottto.", //
            "..ooooo.ooooo.", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A cog roller: a toothed steel wheel with a gold hub and a red eye at its axle (rolling A/B,
/// its eye blazing as it winds up).
pub const COG_ROLLER: SpriteDef = SpriteDef {
    name: "enemy_cog_roller",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL_LIGHT),
        ('y', GOLD),
        ('e', AMBER),
        ('r', RED),
    ],
    frames: &[
        &[
            ".....oooo.....", //
            "..oo.ottto.oo.", //
            "..ottoottoots.", //
            "..otsssssssto.", //
            ".ootsoooosstoo", //
            "ottsoyyyyosstt", //
            "otssoyeeyosso.", //
            ".ossoyeeyosstt", //
            "ottsoyyyyosstt", //
            ".ootsoooosstoo", //
            "..otsssssssto.", //
            "..ottoottoots.", //
            "..oo.ottto.oo.", //
            ".....oooo.....", //
        ],
        &[
            "....oo..oo....", //
            "...otoooot....", //
            ".oottsssstto..", //
            ".otsssssssso..", //
            "ootssoooossto.", //
            "otssoyyyyossoo", //
            ".tssoyeeyosst.", //
            "otssoyeeyossto", //
            "ootssoyyyossoo", //
            ".otssoooosst..", //
            ".otsssssssso..", //
            ".oottsssstto..", //
            "...otoooot....", //
            "....oo..oo....", //
        ],
        &[
            ".....oooo.....", //
            "..oo.ottto.oo.", //
            "..ottoottoots.", //
            "..otsssssssto.", //
            ".ootsoooosstoo", //
            "ottsoyyyyosstt", //
            "otssoyrryosso.", //
            ".ossoyrryosstt", //
            "ottsoyyyyosstt", //
            ".ootsoooosstoo", //
            "..otsssssssto.", //
            "..ottoottoots.", //
            "..oo.ottto.oo.", //
            ".....oooo.....", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A spark wisp: a crackling knot of gold and white light throwing sparks (flicker A/B).
pub const SPARK_WISP: SpriteDef = SpriteDef {
    name: "enemy_spark_wisp",
    legend: &[('w', CREAM), ('y', GOLD), ('s', SKY_LIGHT)],
    frames: &[
        &[
            "....w....", //
            "..s.y.s..", //
            "...yyy...", //
            ".syywyys.", //
            "wyywwwyyw", //
            ".syywyys.", //
            "...yyy...", //
            "..s.y.s..", //
            "....w....", //
        ],
        &[
            "w.......w", //
            ".s..y..s.", //
            "...yyy...", //
            "..yywyy..", //
            ".yywwwyy.", //
            "..yywyy..", //
            "...yyy...", //
            ".s..y..s.", //
            "w.......w", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A rivet hound: a lean steel watchdog with a gold collar plate and an orange eye (run A/B,
/// crouched to pounce).
pub const RIVET_HOUND: SpriteDef = SpriteDef {
    name: "enemy_rivet_hound",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('y', GOLD),
        ('e', ORANGE),
    ],
    frames: &[
        &[
            "..............oo..", //
            ".............oteo.", //
            "..o.........ottttt", //
            ".otooooooooottsso.", //
            "otttttttttttttso..", //
            "osstttttttttsso...", //
            ".osssyyyysssso....", //
            "..ooosssssoooo....", //
            "..os.os..os.os....", //
            "..ot.ot..ot.ot....", //
            ".oo.oo..oo.oo.....", //
        ],
        &[
            "..............oo..", //
            ".............oteo.", //
            "..o.........ottttt", //
            ".otooooooooottsso.", //
            "otttttttttttttso..", //
            "osstttttttttsso...", //
            ".osssyyyysssso....", //
            "..ooosssssoooo....", //
            "..os..os.os..os...", //
            "..ot..ot.ot..ot...", //
            ".oo..oo.oo..oo....", //
        ],
        &[
            "..............oo..", //
            ".............oteo.", //
            "..o.........ottttt", //
            ".otooooooooottssoo", //
            "otttttttttttttso..", //
            "osstttttttttsso...", //
            ".osssyyyysssso....", //
            "..ooosssssoooo....", //
            ".os.......os......", //
            "oot.......oot.....", //
            "oo.........oo.....", //
        ],
    ],
    frame_seconds: 0.1,
};

/// An archive warden: a broad deep-blue guardian, gold seams down its chest, a reading eye of
/// light in its hood (step A/B, reading with its beam).
pub const ARCHIVE_WARDEN: SpriteDef = SpriteDef {
    name: "enemy_archive_warden",
    legend: &[
        ('o', INK),
        ('d', DEEP_BLUE),
        ('s', OCEAN),
        ('y', GOLD),
        ('w', CREAM),
        ('i', SKY_LIGHT),
        ('b', ICE),
    ],
    frames: &[
        &[
            "......oooooo......", //
            ".....oddddddo.....", //
            "....oddssssddo....", //
            "....odsiiiisdo....", //
            "....odswwwwsdo....", //
            "....oddssssddo....", //
            ".....oddddddo.....", //
            "...oooddddddooo...", //
            "..oddddyyyyddddo..", //
            ".odddsdyddydsdddo.", //
            ".oddssdyddydssddo.", //
            "oddo.sdddddds.oddo", //
            "odd.osdddddds.oddo", //
            "oso.osddyyddso.oso", //
            "oyo.osddddddso.oyo", //
            "....osddooddso....", //
            "....oddo..oddo....", //
            "...odddo..odddo...", //
            "...ooooo..ooooo...", //
        ],
        &[
            "......oooooo......", //
            ".....oddddddo.....", //
            "....oddssssddo....", //
            "....odsiiiisdo....", //
            "....odswwwwsdo....", //
            "....oddssssddo....", //
            ".....oddddddo.....", //
            "...oooddddddooo...", //
            "..oddddyyyyddddo..", //
            ".odddsdyddydsdddo.", //
            ".oddssdyddydssddo.", //
            "oddo.sdddddds.oddo", //
            "odd.osdddddds.oddo", //
            "oso.osddyyddso.oso", //
            "oyo.osddddddso.oyo", //
            "....osddooddso....", //
            "....oddo...oddo...", //
            "...odddo...odddo..", //
            "...ooooo...ooooo..", //
        ],
        &[
            "......oooooo......", //
            ".....oddddddo.....", //
            "....oddssssddo....", //
            "....odsbbbbsdo....", //
            "....odswwwwsdoyyy.", //
            "....oddssssddo....", //
            ".....oddddddo.....", //
            "...oooddddddooo...", //
            "..oddddyyyyddddo..", //
            ".odddsdyddydsdddo.", //
            ".oddssdyddydssddo.", //
            "oddo.sdddddds.oddo", //
            "odd.osdddddds.oddo", //
            "oso.osddyyddso.oso", //
            "oyo.osddddddso.oyo", //
            "....osddooddso....", //
            "....oddo..oddo....", //
            "...odddo..odddo...", //
            "...ooooo..ooooo...", //
        ],
    ],
    frame_seconds: 0.22,
};

/// A page moth: wings of old paper, cream with brown script, a dark body (flap A/B).
pub const PAGE_MOTH: SpriteDef = SpriteDef {
    name: "enemy_page_moth",
    legend: &[('w', CREAM), ('c', LOAM), ('o', INK)],
    frames: &[
        &[
            "ww.....ww", //
            "wcw...wcw", //
            "wccwowccw", //
            ".wcwowcw.", //
            "..w.o.w..", //
            ".....o...", //
            "....o....", //
        ],
        &[
            ".........", //
            "..w...w..", //
            ".wcwowcw.", //
            "wccwowccw", //
            "wcw.o.wcw", //
            "ww...o.ww", //
            "....o....", //
        ],
    ],
    frame_seconds: 0.09,
};

/// An index spider: a plum-backed spider with a ledger-card pattern and a gold eye, long
/// jointed legs (creep A/B, rearing to drop).
pub const INDEX_SPIDER: SpriteDef = SpriteDef {
    name: "enemy_index_spider",
    legend: &[('o', INK), ('p', PLUM), ('e', GOLD)],
    frames: &[
        &[
            "o.....o........", //
            ".o...o..ooo....", //
            "..o.o..oppppo..", //
            "...oooppppppeo.", //
            "..oppppppppppoo", //
            ".o.oppppppppo..", //
            "o..o.o.o.o.o...", //
            "..o..o...o..o..", //
            ".o..o.....o..o.", //
        ],
        &[
            "o.....o........", //
            ".o...o..ooo....", //
            "..o.o..oppppo..", //
            "...oooppppppeo.", //
            "..oppppppppppoo", //
            ".o.oppppppppo..", //
            "o..o.o.o.o.o...", //
            ".o..o...o...o..", //
            "o..o.....o...o.", //
        ],
        &[
            "o.....o......oo", //
            ".o...o..ooo.oo.", //
            "..o.o..oppppo..", //
            "...oooppppppeo.", //
            "..oppppppppppoo", //
            ".o.oppppppppo..", //
            "o..o.o.o.o.o...", //
            "..o..o...o..o..", //
            ".o..o.....o..o.", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A street sentinel's bolt: a hot white-gold dart.
pub const SHOT_SENTINEL_BOLT: SpriteDef = SpriteDef {
    name: "shot_sentinel_bolt",
    legend: &[('o', INK), ('y', GOLD), ('w', CREAM)],
    frames: &[
        &[
            ".oooo.", //
            "oywwyo", //
            ".oooo.", //
        ],
        &[
            ".oooo.", //
            "owyywo", //
            ".oooo.", //
        ],
    ],
    frame_seconds: 0.06,
};

/// An archive warden's reading beam: a slow, wide pulse of pale light.
pub const SHOT_READING_BEAM: SpriteDef = SpriteDef {
    name: "shot_reading_beam",
    legend: &[('s', SKY_LIGHT), ('w', ICE), ('b', BONE)],
    frames: &[
        &[
            "..sss..", //
            ".swwws.", //
            "swwbwws", //
            ".swwws.", //
            "..sss..", //
        ],
        &[
            "...s...", //
            ".sswss.", //
            "swbbbws", //
            ".sswss.", //
            "...s...", //
        ],
    ],
    frame_seconds: 0.12,
};

/// The archivist: a little brass-and-steel drone with a mint lens for an eye and three gold
/// hover jets (hover A/B, speaking).
pub const ARCHIVIST: SpriteDef = SpriteDef {
    name: "archivist_drone",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('l', STEEL_LIGHT),
        ('m', TEAL),
        ('i', MINT),
        ('w', ICE),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "....ooo....", //
            "...ottto...", //
            ".ooolllooo.", //
            "otllmmmllto", //
            "otlmiwimlto", //
            "otllmmmllto", //
            ".ooosssooo.", //
            "..y..o..y..", //
            ".y...y...y.", //
        ],
        &[
            "....ooo....", //
            "...ottto...", //
            ".ooolllooo.", //
            "otllmmmllto", //
            "otlmiwimlto", //
            "otllmmmllto", //
            ".ooosssooo.", //
            "...y.o.y...", //
            "...y.y.y...", //
        ],
        &[
            "....ooo....", //
            "...ottto...", //
            ".ooolllooo.", //
            "otllmmmllto", //
            "otlmwiwmlto", //
            "otllmmmllto", //
            ".ooosssooo.", //
            "..y..o..y..", //
            ".y...y...y.", //
        ],
    ],
    frame_seconds: 0.12,
};
