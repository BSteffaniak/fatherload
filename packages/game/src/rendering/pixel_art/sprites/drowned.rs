//! The Drowned and Frozen's creatures (Wave 9, Descent IV): one animated sprite per kind and
//! the Leviathan's pressure bubble. Asymmetric creatures face right (the renderer mirrors
//! them). The Leviathan is drawn at twice the usual scale.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A reef shark: a slate-blue torpedo with a pale belly, a tall fin and a toothy grin, facing
/// right (swim A/B, jaws open for the dash).
pub const REEF_SHARK: SpriteDef = SpriteDef {
    name: "enemy_reef_shark",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('s', SLATE),
        ('l', SLATE_LIGHT),
        ('b', MIST),
        ('w', BONE),
        ('e', GOLD),
        ('r', RED),
    ],
    frames: &[
        &[
            ".........oo...........", //
            "........odso..........", //
            ".o.....odsso..........", //
            "odo..ooddsssooooo.....", //
            "osdooddssssssssslooo..", //
            ".osddsssssssssssslleoo", //
            ".odsdssslllllllllllwwo", //
            "odo.oolbbbbbbbbbbbowo.", //
            ".o.....ooooolooooooo..", //
        ],
        &[
            ".........oo...........", //
            "........odso..........", //
            ".......odsso..........", //
            "..o..ooddsssooooo.....", //
            ".odooddssssssssslooo..", //
            "osdddsssssssssssslleoo", //
            "odsdssslllllllllllllwo", //
            "osoo.oolbbbbbbbbbbbbwo", //
            ".o.....ooooolooooooo..", //
        ],
        &[
            ".........oo...........", //
            "........odso..........", //
            ".o.....odsso.......o..", //
            "odo..ooddsssoooooooso.", //
            "osdooddssssssssslooleo", //
            ".osddssssssssssslloooo", //
            ".odsdssslllllllllowwwo", //
            "odo.oolbbbbbbbbbbbrrro", //
            ".o.....ooooolooooowwwo", //
        ],
    ],
    frame_seconds: 0.22,
};

/// An ink squid: a rose-and-plum mantle with big eyes and curling arms (swim pulse A/B, the
/// ink burst).
pub const INK_SQUID: SpriteDef = SpriteDef {
    name: "enemy_ink_squid",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('m', ROSE),
        ('p', PINK),
        ('w', BONE),
        ('n', NIGHT),
    ],
    frames: &[
        &[
            "....oooo....", //
            "...ompmmo...", //
            "..ompmmmmo..", //
            "..ommmpmmo..", //
            "..ommmmmmo..", //
            ".omwnmmwnmo.", //
            ".ommmmmmmmo.", //
            "..odmdmdmo..", //
            ".odo.dd.odo.", //
            ".od.od.do.do", //
            "od..od..d..o", //
            "o...o...o...", //
            "............", //
        ],
        &[
            "....oooo....", //
            "...ompmmo...", //
            "..ompmmmmo..", //
            "..ommmpmmo..", //
            "..ommmmmmo..", //
            ".omwnmmwnmo.", //
            ".ommmmmmmmo.", //
            "..odmdmdmo..", //
            "..odddddo...", //
            "..ododdod...", //
            "..od.dd.do..", //
            "...o.oo.o...", //
            "............", //
        ],
        &[
            "....oooo....", //
            "...ompmmo...", //
            "..ompmmmmo..", //
            "..ommmpmmo..", //
            "..ommmmmmo..", //
            ".omwnmmwnmo.", //
            ".ommmmmmmmo.", //
            "..odmdmdmo..", //
            ".nnodddonn..", //
            "nnnnnddnnnn.", //
            ".nnnnnnnnnnn", //
            "..nnn.nnnn..", //
            "....n..n....", //
        ],
    ],
    frame_seconds: 0.3,
};

/// The eel-dragon: a long teal serpent with a frilled, horned head, gold-lit gills and a
/// crackling mint stripe down its back, facing right (swim A/B, jaws open and sparking).
pub const EEL_DRAGON: SpriteDef = SpriteDef {
    name: "enemy_eel_dragon",
    legend: &[
        ('o', INK),
        ('d', TEAL_DARK),
        ('t', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('e', GOLD),
        ('a', AMBER),
        ('w', BONE),
        ('r', RED),
        ('p', PINE),
    ],
    frames: &[
        &[
            "................................oo.o....", //
            "...............................odoodo...", //
            "......................ooooo...odtttdoo..", //
            "........oooooo.......odtttdo.odtttttdoo.", //
            "......oodttttdoo....odtmmmtdoddtttttteo.", //
            ".oo..odtmmmmmmtdooooodmmmmmdttdmmtttttoo", //
            "odto.odtttttttttdttdddtttttttdtttttttwwo", //
            "otmtoodpppppppptttttpppppptttttapppaawo.", //
            ".otmmddpppppppddpppddpppppdddddaaaaoooo.", //
            "..odddoooooooooddooooooooooooooooooo....", //
            "...ooo.........oo.......................", //
            "........................................", //
        ],
        &[
            "................................oo.o....", //
            "......oooooo...................odoodo...", //
            "....oodttttdoo................odtttdoo..", //
            "...odtmmmmmmtdoo.....ooooooo.odtttttdoo.", //
            "..odttttttttttdoo..oodtmmmtdoddtttttteo.", //
            "..odpppppppppptdooodtmmmmmmdttdmmtttttoo", //
            ".oodpppppppppddtttttttttttttdtttttttwwo.", //
            "odtooooooooooodpppppppppptttttapppaawo..", //
            "otmo..........oddpppppppdddddaaaaoooo...", //
            ".otmo..........oooooooooooooooooo.......", //
            "..oo....................................", //
            "........................................", //
        ],
        &[
            "...........................m....oo.o....", //
            "..........m...................odoodoo...", //
            "......................ooooo..odtttdteoo.", //
            "........oooooo....m..odtttdo.odttttttdo.", //
            "......oodttttdoo....odtmmmtdoddtttttttoo", //
            ".oo..odtmmmmmmtdooooodmmmmmdttdmmttttwoo", //
            "odto.odtttttttttdttdddtttttttdttttttwrro", //
            "otmtoodpppppppptttttpppppptttttapppawrro", //
            ".otmmddpppppppddpppddpppppdddddaaaaaowwo", //
            "..odddoooooooooddooooooooooooooooooo.oo.", //
            "...ooo.....m...oo..........m............", //
            "........................................", //
        ],
    ],
    frame_seconds: 0.28,
};

/// Frost mites: a little swarm of pale ice mites with frosty legs (they skitter).
pub const FROST_MITE: SpriteDef = SpriteDef {
    name: "enemy_frost_mite",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('s', SKY_LIGHT),
        ('b', SKY),
        ('w', BONE),
    ],
    frames: &[
        &[
            "............", //
            "..o.o.......", //
            ".oiiio...o.o", //
            ".osiwo..oiio", //
            "..obo...oswo", //
            ".o.o.o...oo.", //
            "....o.o.o...", //
            "....oiiio...", //
            "....osiwo...", //
        ],
        &[
            "......o.o...", //
            ".o.o.oiiio..", //
            "oiiio.osiwo.", //
            "osiwo..obo..", //
            ".obo..o.o.o.", //
            "o.o.o.......", //
            ".......o.o..", //
            "......oiiio.", //
            "......osiwo.", //
        ],
    ],
    frame_seconds: 0.16,
};

/// A tusk seal: a fat, mottled grey seal with long ivory tusks and whiskers, facing right
/// (shuffle A/B, flat on its belly for the slide).
pub const TUSK_SEAL: SpriteDef = SpriteDef {
    name: "enemy_tusk_seal",
    legend: &[
        ('o', INK),
        ('d', STONE_DARK),
        ('s', STONE),
        ('l', STONE_LIGHT),
        ('f', FOG),
        ('w', BONE),
        ('n', NIGHT),
    ],
    frames: &[
        &[
            "....................", //
            "..............oooo..", //
            ".............olllso.", //
            "........ooooolnllllo", //
            "......oossslllllllso", //
            "....oosslssllllflwfo", //
            "..oossslsssslllfowfo", //
            ".osssssssssssffo.wo.", //
            "osdssslsssdsffo..wo.", //
            "odddddddddddddo..o..", //
            ".odo.....odo.oo.....", //
            "..o.......o.........", //
        ],
        &[
            "....................", //
            "..............oooo..", //
            ".............olllso.", //
            "........ooooolnllllo", //
            "......oossslllllllso", //
            "....oosslssllllflwfo", //
            "..oossslsssslllfowfo", //
            ".osssssssssssffo.wo.", //
            "osdssslsssdsffo..wo.", //
            "odddddddddddddo..o..", //
            "..odo...odo..oo.....", //
            "...o.....o..........", //
        ],
        &[
            "....................", //
            "....................", //
            "....................", //
            "....................", //
            "..............oooo..", //
            "......oooooooolllso.", //
            "...ooosssslllnllllo.", //
            ".oossslsssslllflwwo.", //
            "osdssslsssdsffffowo.", //
            "oddddddddddddddo.w..", //
            ".ooooooooooooooo....", //
            "....................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A scald beetle: a squat beetle with a sinter-crusted amber shell, steam rising from its
/// vents, facing right (walk A/B, the pounce).
pub const SCALD_BEETLE: SpriteDef = SpriteDef {
    name: "enemy_scald_beetle",
    legend: &[
        ('o', INK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('s', SAND),
        ('m', MIST),
        ('e', GOLD),
        ('d', SOIL_DARK),
    ],
    frames: &[
        &[
            "...m...m.....", //
            "....m.m......", //
            "...ooooooo...", //
            "..osacasacoo.", //
            ".oacccacccaeo", //
            ".orrrrrrrrrro", //
            "..odddddddoo.", //
            "..od.od.od...", //
            "..o..o..o....", //
        ],
        &[
            "....m...m....", //
            "...m...m.....", //
            "...ooooooo...", //
            "..osacasacoo.", //
            ".oacccacccaeo", //
            ".orrrrrrrrrro", //
            "..odddddddoo.", //
            "...do.do.do..", //
            "...o..o..o...", //
        ],
        &[
            "..m.m.m.m....", //
            "...ooooooo...", //
            "..osacasacoo.", //
            ".oacccacccaeo", //
            ".orrrrrrrrrro", //
            ".odddddddddo.", //
            "od.od..od.do.", //
            "o..o....o..o.", //
            ".............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A sinter snail: a slow snail under a heavy, banded shell of sinter, facing right (creep
/// A/B).
pub const SINTER_SNAIL: SpriteDef = SpriteDef {
    name: "enemy_sinter_snail",
    legend: &[
        ('o', INK),
        ('s', SAND),
        ('c', CREAM),
        ('l', LOAM),
        ('k', CLAY),
        ('g', MOSS),
        ('m', LIME),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..............", //
            "....oooooo....", //
            "...osccssso...", //
            "..oscllllsko..", //
            ".osclsccslsko.", //
            ".osklcsslksko.", //
            ".oskllllkksko.", //
            "..oskkkkksko.o", //
            "...oosssoo.oeo", //
            ".ogggmgggggmmo", //
            "oggggggggggggo", //
            ".oooooooooooo.", //
        ],
        &[
            "..............", //
            "....oooooo....", //
            "...osccssso...", //
            "..oscllllsko..", //
            ".osclsccslsko.", //
            ".osklcsslksko.", //
            ".oskllllkksko.", //
            "..oskkkkksko..", //
            "...oosssoo..oo", //
            ".ogggmgggggoeo", //
            "ogggggggggmmmo", //
            ".oooooooooooo.", //
        ],
    ],
    frame_seconds: 0.5,
};

/// A spinepuff: a small, round reef fish that swells into a ball of spines when something
/// comes close (drifting, swollen).
pub const SPINEPUFF: SpriteDef = SpriteDef {
    name: "enemy_spinepuff",
    legend: &[
        ('o', INK),
        ('y', GOLD),
        ('a', AMBER),
        ('c', CREAM),
        ('w', BONE),
        ('n', NIGHT),
        ('r', RUST),
    ],
    frames: &[
        &[
            "..............", //
            "..............", //
            "..............", //
            "..............", //
            ".....oooo.....", //
            "..o.oyyayo....", //
            "..oooywnyao...", //
            "..oaayyyyyao..", //
            "..o.occcccao..", //
            ".....ooooo....", //
            "..............", //
            "..............", //
            "..............", //
            "..............", //
        ],
        &[
            "..r...r...r...", //
            "...r.oooo.r...", //
            "r..ooyyyyoo..r", //
            ".rooyyyyyyyoor", //
            "..oyywnyyyyyo.", //
            "roayyyyyyyyyao", //
            ".oaayyyyyyyaao", //
            "roaacyyyyycaao", //
            ".oaccccccccaao", //
            "r.oaccccccaao.", //
            "..ooaaccaaoo.r", //
            ".r..oooooo..r.", //
            "...r..r...r...", //
            "..............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A lure angler: a black, lumpy anglerfish with needle teeth and a glowing lure dangling on a
/// rod over its head, facing right (hovering with the lure swaying A/B, jaws agape).
pub const LURE_ANGLER: SpriteDef = SpriteDef {
    name: "enemy_lure_angler",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('s', SLATE_DARK),
        ('l', SLATE),
        ('w', BONE),
        ('c', CREAM),
        ('y', GOLD),
        ('m', MINT),
        ('r', MAROON),
    ],
    frames: &[
        &[
            "...............oo...", //
            "..............ocyo..", //
            ".........oooooyccyo.", //
            "........o.....oyyo..", //
            ".......o.......oo...", //
            "....oooooooo........", //
            "..oonssssssnoo......", //
            ".onssslssssssno.....", //
            "onsssssssssmwsno....", //
            "onssslssssssrrwo....", //
            "osssssssssssrrwo....", //
            "onssslsssssswwwo....", //
            ".onsssssssssssno....", //
            "..oonssssssnoo......", //
            "....oooooooo........", //
        ],
        &[
            "....................", //
            ".............oo.....", //
            ".........ooo.ocyo...", //
            "........o...ocyyco..", //
            ".......o.....oyyo...", //
            "....oooooooo..oo....", //
            "..oonssssssnoo......", //
            ".onssslssssssno.....", //
            "onsssssssssmwsno....", //
            "onssslssssssrrwo....", //
            "osssssssssssrrwo....", //
            "onssslsssssswwwo....", //
            ".onsssssssssssno....", //
            "..oonssssssnoo......", //
            "....oooooooo........", //
        ],
        &[
            "...............oo...", //
            "..............ocyo..", //
            ".........oooooyccyo.", //
            "........o.....oyyo..", //
            ".......o.......oo...", //
            "....ooooooooo.......", //
            "..oonssssssssoo.....", //
            ".onssslsssmwwwwo....", //
            "onsssssssswoooowo...", //
            "onssslssssrrrrrr....", //
            "osssssssssrrrrrr....", //
            "onssslssssoooowo....", //
            ".onsssssssswwwwo....", //
            "..oonssssssssoo.....", //
            "....ooooooooo.......", //
        ],
    ],
    frame_seconds: 0.45,
};

/// A gulper eel: a slow violet eel that is mostly mouth, a pink gullet and a tiny eye, facing
/// right (swim A/B, the long gape that warns of a gulp).
pub const GULPER_EEL: SpriteDef = SpriteDef {
    name: "enemy_gulper_eel",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('p', PINK),
        ('r', ROSE),
        ('w', BONE),
        ('m', MINT),
    ],
    frames: &[
        &[
            "..........................", //
            "..................ooooo...", //
            "................oovvvvvoo.", //
            "...............ovvvmvvvvvo", //
            "....oooo......ovvvvvvvvvvo", //
            "..oodvvdoooooodvvvvvvwwwwo", //
            "oodvvlvvvvvvvvvvvvvvo.....", //
            "odvvvvdddddddddvvvvvo.....", //
            ".odddooooooooooodvvvvwwwwo", //
            "..ooo..........odddddddddo", //
            "................oooooooo..", //
            "..........................", //
        ],
        &[
            "..........................", //
            "..................ooooo...", //
            "................oovvvvvoo.", //
            "...............ovvvmvvvvvo", //
            "..oooo........ovvvvvvvvvvo", //
            ".odvvdoo.....odvvvvvvwwwwo", //
            "odvvlvvvdooooovvvvvvo.....", //
            ".odvvvddddddddvvvvvvo.....", //
            "..oddoooooooooodvvvvwwwwo.", //
            "...oo..........oddddddddo.", //
            "................oooooooo..", //
            "..........................", //
        ],
        &[
            "..................ooooo...", //
            "................oovvvvvoo.", //
            "...............ovvvmvvvvvo", //
            "..............ovvvvvvwwwwo", //
            "....oooo......ovvvvvo.....", //
            "..oodvvdoooooodvvvvrppppp.", //
            "oodvvlvvvvvvvvvvvvvrppppp.", //
            "odvvvvdddddddddvvvvrppppp.", //
            ".odddooooooooooodvvvo.....", //
            "..ooo...........ovvvvwwwwo", //
            "................oddddddddo", //
            ".................oooooooo.", //
        ],
    ],
    frame_seconds: 0.35,
};

/// The Leviathan: a vast, scarred sea beast, deep ocean blue with a pale mottled belly, a
/// barnacled brow over two glowing amber eyes, a jaw of white teeth, gill slits and a long
/// fluked tail, facing right. Drawn at twice the usual scale (swim A/B, the maw agape, the
/// roar's flared eyes).
pub const LEVIATHAN: SpriteDef = SpriteDef {
    name: "enemy_leviathan",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('d', DEEP_BLUE),
        ('b', OCEAN),
        ('s', SKY),
        ('m', MIST),
        ('f', FOG),
        ('w', BONE),
        ('e', AMBER),
        ('y', GOLD),
        ('c', CREAM),
        ('r', MAROON),
        ('p', ROSE),
        ('t', TEAL_DARK),
        ('g', STONE_LIGHT),
    ],
    frames: &[
        &[
            "...........................................ooooo....", //
            "...............................oooooo....oogggggoo..", //
            ".oo.......................ooodddddddoooooggdgdggdoo.", //
            "odbo...................oodddbbbbbbbbbddddddddddddddo", //
            "odbbo...............ooddbbbbbsbbbbbbbbbbbbbbbbbbeeyo", //
            ".odbbo...........oodddbbbbbbbbbbbbbsbbbbbbbbbbbbeyyo", //
            "..odbbooo.....oodddbbbbsbbbbbbbbbbbbbbbbtbtbtbbbbbbo", //
            "..odbbbbdoooooddbbbbbbbbbbbbbbbbsbbbbbbbtbtbtbbbbbdo", //
            "..oddbbbbddddbbbbbbbsbbbbbbbbbbbbbbbbbbbtbtbtbbbbbbo", //
            "..odbbbbbbbbbbbbbbbbbbbbbbbbbbfmmmmmmmmmmmmmmbwwwwwo", //
            ".odbbbdddddbbbbbbbbbbbbbffmmmmmmmmfmmmmmmmmmmmrrrrro", //
            "odbbo.oooooddddbbbbffmmmmmmfmmmmmmmmmmmmfmmmmwwwwwwo", //
            "odbo.......oooodddfmmmmfmmmmmmmmmfmmmmmmmmmmmmmmmmo.", //
            ".oo...........ooooddfffmmmmmmmmmmmmmmmmmmmmmmmmmmo..", //
            "..................ooodddfffffffffffffffffffffffoo...", //
            ".....................oooodddddddddddddddddddoooo....", //
            ".........................ooooooooooooooooooo........", //
            "....................................................", //
        ],
        &[
            "...........................................ooooo....", //
            "...............................oooooo....oogggggoo..", //
            "..........................ooodddddddoooooggdgdggdoo.", //
            ".......................oodddbbbbbbbbbddddddddddddddo", //
            ".oooo...............ooddbbbbbsbbbbbbbbbbbbbbbbbbeeyo", //
            "oddbbooo.........oodddbbbbbbbbbbbbbsbbbbbbbbbbbbeyyo", //
            ".oodbbbboooo..oodddbbbbsbbbbbbbbbbbbbbbbtbtbtbbbbbbo", //
            "...oddbbbbbdoddbbbbbbbbbbbbbbbbsbbbbbbbtbtbtbbbbbdo.", //
            "....oddbbbbbbbbbbbbbsbbbbbbbbbbbbbbbbbbbtbtbtbbbbbbo", //
            "...odbbbbbbbbbbbbbbbbbbbbbbbbbbfmmmmmmmmmmmmmbwwwwwo", //
            "...odbbbddddbbbbbbbbbbbbffmmmmmmmmfmmmmmmmmmmmrrrrro", //
            ".oodbbooooodddbbbbbffmmmmmmfmmmmmmmmmmmmfmmmmwwwwwwo", //
            "oddbbo.....oooodddfmmmmfmmmmmmmmmfmmmmmmmmmmmmmmmmo.", //
            ".oooo.........ooooddfffmmmmmmmmmmmmmmmmmmmmmmmmmmo..", //
            "..................ooodddfffffffffffffffffffffffoo...", //
            ".....................oooodddddddddddddddddddoooo....", //
            ".........................ooooooooooooooooooo........", //
            "....................................................", //
        ],
        &[
            "...........................................ooooo....", //
            "...............................oooooo....oogggggoo..", //
            ".oo.......................ooodddddddoooooggdgdggdoo.", //
            "odbo...................oodddbbbbbbbbbddddddddddddddo", //
            "odbbo...............ooddbbbbbsbbbbbbbbbbbbbbbbbbeccy", //
            "odbbo...........oodddbbbbbbbbbbbbbsbbbbbbbbbbbbecyyo", //
            ".odbbooo.....oodddbbbbsbbbbbbbbbbbbbbbbtbtbtbbbbbbbo", //
            ".odbbbbdoooooddbbbbbbbbbbbbbbbbsbbbbbbbtbtbtbbbbbbbo", //
            ".oddbbbbddddbbbbbbbsbbbbbbbbbbbbbbbbbbbtbtbtbbwwwwwo", //
            ".odbbbbbbbbbbbbbbbbbbbbbbbbbbfmmmmmmmmmmmmmmrppppppo", //
            "odbbbdddddbbbbbbbbbbbbbffmmmmmmmmfmmmmmmmmmrrppppppo", //
            "odbbo.oooooddddbbbbffmmmmmmfmmmmmmmmmmmmfmmmrrpppppp", //
            "odbo.......oooodddfmmmmfmmmmmmmmmfmmmmmmmmmmrppppppo", //
            ".oo...........ooooddfffmmmmmmmmmmmmmmmmmmmmmmwwwwwwo", //
            "..................ooodddffffffffffffffffffffffooooo.", //
            ".....................oooodddddddddddddddddddoooo....", //
            ".........................ooooooooooooooooooo........", //
            "....................................................", //
        ],
    ],
    frame_seconds: 0.5,
};

/// The Leviathan's pressure bubble: a wobbling sphere of crushed water with a bright rim.
pub const SHOT_PRESSURE_BUBBLE: SpriteDef = SpriteDef {
    name: "enemy_shot_pressure_bubble",
    legend: &[('o', OCEAN), ('s', SKY), ('l', SKY_LIGHT), ('w', BONE)],
    frames: &[
        &[
            "..oooo..", //
            ".osssso.", //
            "oslwsslo", //
            "oswssslo", //
            "osssssso", //
            "osssssso", //
            ".osslso.", //
            "..oooo..", //
        ],
        &[
            "........", //
            "..oooo..", //
            ".oslwso.", //
            "osswssso", //
            "osssssso", //
            ".osssso.", //
            "..oooo..", //
            "........", //
        ],
    ],
    frame_seconds: 0.18,
};
