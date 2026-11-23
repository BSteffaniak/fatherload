//! The far Strange Layers' creatures (Wave 9 strange-b, Descent VI strata 4-6 and its gate):
//! one animated sprite per kind and their shots. Asymmetric creatures face right (the renderer
//! mirrors them).

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A tick mite: a tiny brass-bodied mite with a ticking gear on its back and a blur of legs,
/// facing right (scuttle A/B).
pub const TICK_MITE: SpriteDef = SpriteDef {
    name: "enemy_tick_mite",
    legend: &[
        ('o', INK),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('r', RED),
        ('s', STONE_LIGHT),
    ],
    frames: &[
        &[
            "...ogo...", //
            "..oagao..", //
            ".occaccro", //
            "occccccwo", //
            ".s.s.s.s.", //
            "s.s.s.s..", //
        ],
        &[
            "...oao...", //
            "..ogago..", //
            ".occaccro", //
            "occccccwo", //
            "s.s.s.s..", //
            ".s.s.s.s.", //
        ],
    ],
    frame_seconds: 0.08,
};

/// An amber sloth: a heavy, hunched beast crusted with amber, honey dripping from its long
/// claws, facing right (crawl A/B, reaching out).
pub const AMBER_SLOTH: SpriteDef = SpriteDef {
    name: "enemy_amber_sloth",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('b', SOIL),
        ('l', LOAM),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('e', ICE),
    ],
    frames: &[
        &[
            "......oooooo......", //
            "....ooaagaaaoo....", //
            "...oagwaaaagaao...", //
            "..oaaaaaaaggaaaoo.", //
            ".obbbaaaabbbbllleo", //
            "obbbbbbbbbbbbbllgo", //
            "obllbbbbbbbbbbbbo.", //
            "obbbbbbbbbbbbbbo..", //
            ".obbddbbbbbddbo...", //
            "..odd.obo.odd.o...", //
            "..oaa..o..oaa.....", //
            "...g.......g......", //
            "..................", //
        ],
        &[
            "......oooooo......", //
            "....ooaagaaaoo....", //
            "...oagwaaaagaao...", //
            "..oaaaaaaaggaaaoo.", //
            ".obbbaaaabbbbllleo", //
            "obbbbbbbbbbbbbllgo", //
            "obllbbbbbbbbbbbbo.", //
            "obbbbbbbbbbbbbbo..", //
            ".obbddbbbbbddbo...", //
            "...oddo.obo.oddo..", //
            "...oaa...o...oaa..", //
            "......g......g....", //
            "..................", //
        ],
        &[
            "......oooooo......", //
            "....ooaagaaaoo....", //
            "...oagwaaaagaaoooo", //
            "..oaaaaaaaggaaablo", //
            ".obbbaaaabbbbbblao", //
            "obbbbbbbbbbbbbooga", //
            "obllbbbbbbbbbbo..g", //
            "obbbbbbbbbbbbbo...", //
            ".obbddbbbbbddbo...", //
            "..odd.obo.odd.o...", //
            "..oaa..o..oaa.....", //
            "...g.......g......", //
            "..................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A rewind newt: a slim teal newt with a clock-face spot on its back and a ghost of itself
/// trailing behind, facing right (walk A/B, lunging).
pub const REWIND_NEWT: SpriteDef = SpriteDef {
    name: "enemy_rewind_newt",
    legend: &[
        ('o', INK),
        ('t', TEAL_DARK),
        ('m', TEAL),
        ('n', MINT),
        ('w', CREAM),
        ('g', GOLD),
        ('h', SLATE_LIGHT),
        ('e', RED),
    ],
    frames: &[
        &[
            "..............", //
            "h......oooo...", //
            ".hh..oommmmoo.", //
            "..hhommwgmmmeo", //
            "...ommmggmmmno", //
            "oooommmmmmmmo.", //
            ".otto.oto.ot..", //
        ],
        &[
            "..............", //
            ".h.....oooo...", //
            "..hh.oommmmoo.", //
            "...homwgmmmmeo", //
            "...ommmggmmmno", //
            "..ooommmmmmmo.", //
            "..ot.ottoo.oto", //
        ],
        &[
            "...........oo.", //
            "hh.....ooommmo", //
            "..hhooommmwgeo", //
            "....ommmmmggno", //
            "...ommmmmmmmo.", //
            "..ommmmmooo...", //
            ".oto.ot.......", //
        ],
    ],
    frame_seconds: 0.0,
};

/// An echo stalker: an eyeless, long-limbed hunter in grey hide, huge cupped ears and a
/// gaping mouth, facing right (prowl A/B, lunging).
pub const ECHO_STALKER: SpriteDef = SpriteDef {
    name: "enemy_echo_stalker",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('s', SLATE),
        ('l', SLATE_LIGHT),
        ('f', FOG),
        ('p', PINK),
        ('w', BONE),
    ],
    frames: &[
        &[
            "...........oo.oo..", //
            "..........oflofl..", //
            "......ooooolslsoo.", //
            "...ooosssssssssllo", //
            ".oossssssssssspwwo", //
            "osssdssssssdsoppo.", //
            "osdssssssssssswwo.", //
            ".oddsssdsssssdoo..", //
            "..odo.ods..odo....", //
            "..os..os...os.....", //
            ".oo..oo...oo......", //
        ],
        &[
            "...........oo.oo..", //
            "..........oflofl..", //
            "......ooooolslsoo.", //
            "...ooosssssssssllo", //
            ".oossssssssssspwwo", //
            "osssdssssssdsoppo.", //
            "osdssssssssssswwo.", //
            ".oddsssdsssssdoo..", //
            "...ods.odo..ods...", //
            "....os..os...os...", //
            "...oo..oo...oo....", //
        ],
        &[
            "............oo.oo.", //
            ".....oooo..oflofl.", //
            "..oooossssooolslso", //
            ".osssssssssssssslo", //
            "osssdsssssssssppwo", //
            "osdsssssdsssso.pwo", //
            ".oddsssssssssowwo.", //
            "..oddoooodsdooo...", //
            ".odo......odo.....", //
            "oo.........oo.....", //
            "..................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A shrieker: a bat-winged screamer with a huge round mouth and pale ribbed wings (flap
/// A/B; the mouth gapes when it screams).
pub const SHRIEKER: SpriteDef = SpriteDef {
    name: "enemy_shrieker",
    legend: &[
        ('o', INK),
        ('d', NIGHT),
        ('v', PLUM),
        ('l', LAVENDER),
        ('p', PINK),
        ('r', MAROON),
        ('w', BONE),
    ],
    frames: &[
        &[
            "o.........o.", //
            "ol.......lo.", //
            "ovl.ooo.lvo.", //
            "ovvlovvolvvo", //
            ".ovvvprvvvo.", //
            "..ovprrpvo..", //
            "...ovwwvo...", //
            "....oddo....", //
            ".....oo.....", //
        ],
        &[
            "............", //
            "....ooo.....", //
            "...ovvvo....", //
            ".oolvprvloo.", //
            "olvvprrpvvlo", //
            "ovvvvwwvvvvo", //
            "o.ovvddvvo.o", //
            "...ovoovo...", //
            "....o..o....", //
        ],
    ],
    frame_seconds: 0.14,
};

/// A hush moth: a soft grey moth cloud with velvet wings and muffled, dusty edges (flutter).
pub const HUSH_MOTH: SpriteDef = SpriteDef {
    name: "enemy_hush_moth",
    legend: &[
        ('o', SLATE_DARK),
        ('s', SLATE),
        ('l', SLATE_LIGHT),
        ('f', FOG),
        ('m', MIST),
        ('e', LAVENDER),
    ],
    frames: &[
        &[
            "..m........m..", //
            ".mfm......mfm.", //
            "mflfo....oflfm", //
            "flllso..oslllf", //
            "fllsssooossllf", //
            ".flssseesssfl.", //
            "..mfsoooosfm..", //
            "..mfss..ssfm..", //
            ".mff......ffm.", //
            "..m........m..", //
            "..............", //
        ],
        &[
            "..............", //
            "..m........m..", //
            ".mfm......mfm.", //
            "mflfoo..ooflfm", //
            "fllssoooosllf.", //
            ".flssseesssf..", //
            "..fsoooooosf..", //
            "..mfss..ssfm..", //
            "..mf......fm..", //
            "...m......m...", //
            "..............", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A prism scarab: a beetle under a faceted crystal shell (the renderer tints the shell to its
/// colour), legs of black glass, facing right (walk A/B, charging).
pub const PRISM_SCARAB: SpriteDef = SpriteDef {
    name: "enemy_prism_scarab",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('s', STEEL),
        ('w', ICE),
        ('l', SKY_LIGHT),
        ('m', SKY),
        ('e', GOLD),
    ],
    frames: &[
        &[
            ".....ooooo.....", //
            "...oolwlmmoo...", //
            "..olwlmmmlmmo..", //
            ".olmmlwmmlmmmoo", //
            "olmmmmlmmmmmmeo", //
            "oddddddddddddoo", //
            ".odo.odo.odo...", //
            ".o...o...o.....", //
            "...............", //
            "...............", //
        ],
        &[
            "...............", //
            ".....ooooo.....", //
            "...oolwlmmoo...", //
            "..olwlmmmlmmo..", //
            ".olmmlwmmlmmmoo", //
            "olmmmmlmmmmmmeo", //
            "oddddddddddddoo", //
            "..odo.odo.odo..", //
            "...o...o...o...", //
            "...............", //
        ],
        &[
            "...............", //
            ".......ooooo...", //
            ".....oolwlmmoo.", //
            "...oolwlmmmlmmo", //
            "..olmmlwmmlmmeo", //
            ".olmmmmlmmmmmoo", //
            "oddddddddddddo.", //
            "odo.odo.odo....", //
            "o...o...o......", //
            "...............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A splitter spire: a rooted crystal spire with three facets (red, green, blue) and a white
/// heart that brightens before it fires (idle A/B, charging).
pub const SPLITTER_SPIRE: SpriteDef = SpriteDef {
    name: "enemy_splitter_spire",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('g', LIME),
        ('b', SKY),
        ('w', ICE),
        ('c', CREAM),
        ('s', STONE_DARK),
        ('t', STONE),
    ],
    frames: &[
        &[
            "....o....", //
            "...owo...", //
            "...orbo..", //
            "..orwbo..", //
            "..orgbbo.", //
            ".orrgwbo.", //
            ".orggbbo.", //
            ".orgwgbo.", //
            "orrggbbbo", //
            "orrgggbbo", //
            "orrgwgbbo", //
            ".orggbbo.", //
            "ostttttso", //
            "osssssso.", //
            ".osssso..", //
            "..oooo...", //
        ],
        &[
            "....o....", //
            "...oco...", //
            "...orbo..", //
            "..orwbo..", //
            "..orgbbo.", //
            ".orrgcbo.", //
            ".orggbbo.", //
            ".orgcgbo.", //
            "orrggbbbo", //
            "orrgggbbo", //
            "orrgcgbbo", //
            ".orggbbo.", //
            "ostttttso", //
            "osssssso.", //
            ".osssso..", //
            "..oooo...", //
        ],
        &[
            "...ooo...", //
            "...cwc...", //
            "..owcwo..", //
            "..ocwco..", //
            "..orwbbo.", //
            ".orrwcbo.", //
            ".orgwcbo.", //
            ".orgcgbo.", //
            "orrgwcbbo", //
            "orrgcgbbo", //
            "orrgcgbbo", //
            ".orggbbo.", //
            "ostttttso", //
            "osssssso.", //
            ".osssso..", //
            "..oooo...", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A lens wraith: a drifting shape of bent light, a lens for a face (the renderer tints it to
/// its colour and hides it from any other light), facing right (drift A/B, lunging).
pub const LENS_WRAITH: SpriteDef = SpriteDef {
    name: "enemy_lens_wraith",
    legend: &[
        ('o', SLATE_DARK),
        ('w', ICE),
        ('l', SKY_LIGHT),
        ('m', MIST),
        ('c', CREAM),
        ('k', INK),
    ],
    frames: &[
        &[
            "....oooo....", //
            "..oommllooo.", //
            ".omlwwwwlmo.", //
            ".omwckkcwmo.", //
            "omlwkwwkwlmo", //
            "omlwckkcwlmo", //
            ".omlwwwwlmo.", //
            ".ommllllmmo.", //
            "..omllllmo..", //
            "..oml..lmo..", //
            "...om..mo...", //
            "....o..o....", //
            "............", //
        ],
        &[
            "....oooo....", //
            "..oommllooo.", //
            ".omlwwwwlmo.", //
            ".omwckkcwmo.", //
            "omlwkwwkwlmo", //
            "omlwckkcwlmo", //
            ".omlwwwwlmo.", //
            ".ommllllmmo.", //
            "..omllllmo..", //
            "..omlo.lmo..", //
            "..om...om...", //
            "..o.....o...", //
            "............", //
        ],
        &[
            "......oooo..", //
            "...oommllooo", //
            "..omlwwwwlmo", //
            "..omwckkcwmo", //
            ".omlwkwwkwlo", //
            "omlwwckkcwlo", //
            "omllwwwwlmo.", //
            "ommmllllmo..", //
            "omllllmmo...", //
            "oml..oo.....", //
            "om..........", //
            "o...........", //
            "............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// The Glitch: a builder construct coming apart into its own data, a cracked monitor-face
/// with a single eye, torn panels, loose pixels orbiting and stuck colour channels. It is
/// drawn half as large again, towering over a rig (idle A/B, firing, rewriting).
pub const THE_GLITCH: SpriteDef = SpriteDef {
    name: "enemy_the_glitch",
    legend: &[
        ('o', INK),
        ('d', NIGHT),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('l', STEEL_LIGHT),
        ('m', MINT),
        ('c', TEAL),
        ('k', TEAL_DARK),
        ('p', PINK),
        ('r', RED),
        ('b', SKY),
        ('g', LIME),
        ('w', ICE),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "..p.......oooooooooo.......b..", //
            "........oosssssssssssoo.......", //
            "...g..oosttllllllllttsso......", //
            "......ostlooooooooooootso..r..", //
            ".....ostlodkkkkkkkkkkdotso....", //
            ".....otlodkcccccccccckdotlo...", //
            "....ostodkcmmmmmmmmmmckdots...", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "...ostlokcmwwwrrrrwwwmckoltso.", //
            "...otlodkcmwwrryyrrwwmckdolto.", //
            "...otlodkcmwwrryyrrwwmckdolto.", //
            "...ostlokcmwwwrrrrwwwmckoltso.", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "b...ostodkcmmmmmmmmmmckdots...", //
            ".....otlodkcccccccccckdotlo...", //
            ".....ostlodkkkkkkkkkkdotso..g.", //
            "......ostlooooooooooootso.....", //
            ".......oosttllllllllttsso.....", //
            ".....oo.oosssssssssssoo.oo....", //
            "....otso..oosttttsoo..osto....", //
            "...otlso....ossssso...oslto...", //
            "..otlso....oslttlso....ostlo..", //
            "..oso.....osltppltso....oso...", //
            ".oso.....osltpppplts.....oso..", //
            ".oo.....ostlttpptltso.....oo..", //
            "r......osttttttttttso.........", //
            "......ossssssssssssssso....p..", //
            ".....oooooooooooooooooo.......", //
            "..g...........................", //
            "..............................", //
        ],
        &[
            ".b........oooooooooo........p.", //
            "........oosssssssssssoo.......", //
            "......oosttllllllllttsso..g...", //
            "..r...ostlooooooooooootso.....", //
            ".....ostlodkkkkkkkkkkdotso....", //
            ".....otlodkcccccccccckdotlo...", //
            "....ostodkcmmmmmmmmmmckdots...", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "...ostlokcmwwwwrrrrwwmckoltso.", //
            "...otlodkcmwwwrryyrrwmckdolto.", //
            "...otlodkcmwwwrryyrrwmckdolto.", //
            "...ostlokcmwwwwrrrrwwmckoltso.", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "....ostodkcmmmmmmmmmmckdots..b", //
            ".....otlodkcccccccccckdotlo...", //
            ".g...ostlodkkkkkkkkkkdotso....", //
            "......ostlooooooooooootso.....", //
            ".......oosttllllllllttsso.....", //
            "......o.oosssssssssssoo.o.....", //
            ".....otso.oosttttsoo.osto.....", //
            "....otlso...ossssso..oslto....", //
            "...otlso...oslttlso...ostlo...", //
            "...oso....osltppltso...oso....", //
            "..oso....osltpppplts....oso...", //
            "..oo....ostlttpptltso....oo...", //
            "......osttttttttttso.......r..", //
            "......ossssssssssssssso.......", //
            ".....oooooooooooooooooo.......", //
            "......................y.......", //
            "..............................", //
        ],
        &[
            "..p.......oooooooooo.......b..", //
            "........oosssssssssssoo.......", //
            "...g..oosttllllllllttsso......", //
            "......ostlooooooooooootso..r..", //
            ".....ostlodkkkkkkkkkkdotso....", //
            ".....otlodkcccccccccckdotlo...", //
            "....ostodkcmmmmmmmmmmckdots...", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "...ostlokcmwwyyyyyywwmckoltso.", //
            "...otlodkcmwyyyyyyyywmckdolto.", //
            "...otlodkcmwyyyyyyyywmckdolto.", //
            "...ostlokcmwwyyyyyywwmckoltso.", //
            "....otlokcmwwwwwwwwwwmckolto..", //
            "b...ostodkcmmmmmmmmmmckdots...", //
            ".....otlodkcccccccccckdotlo...", //
            ".....ostlodkkkkkkkkkkdotso..g.", //
            "......ostlooooooooooootso.....", //
            ".......oosttllllllllttsso.....", //
            "...oooo.oosssssssssssoo.oooo..", //
            "..otllso..oosttttsoo..osllto..", //
            ".otlso......ossssso.....oslto.", //
            "otlso......oslttlso......ostlo", //
            "oso.......osltppltso.......oso", //
            "oo.......osltpppplts........oo", //
            ".........ostlttpptltso........", //
            "r......osttttttttttso.........", //
            "......ossssssssssssssso....p..", //
            ".....oooooooooooooooooo.......", //
            "..g...........................", //
            "..............................", //
        ],
        &[
            "p.b.g.r...oooooooooo...b.g.r.p", //
            "...g....oosssssssssssoo...p...", //
            ".b....oosttllllllllttsso....g.", //
            "...r..ostlooooooooooootso.b...", //
            ".....ostlodggggggggggdotso....", //
            ".p...otlodgpppppppppgdotlo..r.", //
            "....ostodgpbbbbbbbbpgdots.....", //
            "..g.otlogpbwwwwwwwwbpgolto..b.", //
            "...ostlogpbwrgbrgbwbpgoltso...", //
            "...otlodgpbwgbrgbrwbpgdolto...", //
            ".b.otlodgpbwbrgbrgwbpgdolto.g.", //
            "...ostlogpbwrgbrgbwbpgoltso...", //
            "....otlogpbwwwwwwwwbpgolto....", //
            "..r.ostodgpbbbbbbbbpgdots..p..", //
            ".....otlodgpppppppppgdotlo....", //
            ".g...ostlodggggggggggdotso..b.", //
            "......ostlooooooooooootso.....", //
            "...b...oosttllllllllttsso..r..", //
            ".....oo.oosssssssssssoo.oo....", //
            "....otso..oosttttsoo..osto....", //
            "..g.otlso...ossssso...oslto.p.", //
            "..otlso....oslttlso....ostlo..", //
            "..oso.....osltppltso....oso...", //
            ".oso..b..osltpppplts..g..oso..", //
            ".oo.....ostlttpptltso.....oo..", //
            "r......osttttttttttso......b..", //
            "......ossssssssssssssso....p..", //
            ".....oooooooooooooooooo.......", //
            "..g.....p.......b.......r.....", //
            "..............................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A glitch shard: a jittering square of corrupted pixels (it flickers through colours).
pub const GLITCH_SHARD: SpriteDef = SpriteDef {
    name: "enemy_glitch_shard",
    legend: &[('o', INK), ('m', MINT), ('p', PINK), ('b', SKY), ('w', ICE)],
    frames: &[
        &[
            ".oooooo.", //
            "ompmbpmo", //
            "obwwwwpo", //
            "omwbpwmo", //
            "opwpbwbo", //
            "obwwwwmo", //
            "ompbmpbo", //
            ".oooooo.", //
        ],
        &[
            "o.oooo.o", //
            ".obmpmbo", //
            "ompwwbpo", //
            "obwpmwmo", //
            "omwbmwbo", //
            "opbwwwpo", //
            ".obmpbo.", //
            "o.oooo.o", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A splitter spire's red bolt.
pub const SHOT_PRISM_RED: SpriteDef = SpriteDef {
    name: "shot_prism_red",
    legend: &[('o', MAROON), ('r', RED), ('w', PINK)],
    frames: &[&[".oo.", "orwo", "owro", ".oo."]],
    frame_seconds: 0.0,
};

/// A splitter spire's green bolt.
pub const SHOT_PRISM_GREEN: SpriteDef = SpriteDef {
    name: "shot_prism_green",
    legend: &[('o', PINE), ('g', LIME), ('w', CREAM)],
    frames: &[&[".oo.", "ogwo", "owgo", ".oo."]],
    frame_seconds: 0.0,
};

/// A splitter spire's blue bolt.
pub const SHOT_PRISM_BLUE: SpriteDef = SpriteDef {
    name: "shot_prism_blue",
    legend: &[('o', DEEP_BLUE), ('b', SKY), ('w', ICE)],
    frames: &[&[".oo.", "obwo", "owbo", ".oo."]],
    frame_seconds: 0.0,
};

/// The Glitch's bolt: a corrupted square of colour channels.
pub const SHOT_GLITCH_BOLT: SpriteDef = SpriteDef {
    name: "shot_glitch_bolt",
    legend: &[('o', INK), ('m', MINT), ('p', PINK), ('b', SKY), ('w', ICE)],
    frames: &[
        &["oooooo", "ompbmo", "obwwpo", "opwwbo", "ombpmo", "oooooo"],
        &["o.oo.o", ".obpm.", "opwwbo", "obwwpo", ".ompb.", "o.oo.o"],
    ],
    frame_seconds: 0.06,
};
