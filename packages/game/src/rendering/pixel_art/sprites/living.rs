//! The Living Dark's creatures (Wave 8, Descent III): one animated sprite per kind and
//! their shots. Asymmetric creatures face right (the renderer mirrors them).

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A spore puffer: a waddling violet puffball with pale spots and glowing lime vents, facing
/// right (walk A/B, inflated before it lobs).
pub const SPORE_PUFFER: SpriteDef = SpriteDef {
    name: "enemy_spore_puffer",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('m', VIOLET),
        ('l', LAVENDER),
        ('w', PINK),
        ('s', CREAM),
        ('g', LIME),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..............", //
            "......gg......", //
            "....ogllgo....", //
            "...ollllllo...", //
            "..ollwllsllo..", //
            ".olsllllllmmo.", //
            ".ollllllleomo.", //
            "olllllllllmmdo", //
            "olmmllllmmmmdo", //
            ".ommmmmmmmmdo.", //
            "..oddddddddo..", //
            "...o.o..o.o...", //
            "..............", //
        ],
        &[
            "..............", //
            "......gg......", //
            "....ogllgo....", //
            "...ollllllo...", //
            "..ollwllsllo..", //
            ".olsllllllmmo.", //
            ".ollllllleomo.", //
            "olllllllllmmdo", //
            "olmmllllmmmmdo", //
            ".ommmmmmmmmdo.", //
            "..oddddddddo..", //
            "..o.o..o.o....", //
            "..............", //
        ],
        &[
            ".....g..g.....", //
            "...ogllllgo...", //
            "..olllwllllo..", //
            ".ollsllllsllo.", //
            "olllllllllllmo", //
            "olsllllllleomo", //
            "olllllllllllmo", //
            "olllllllllmmdo", //
            "olmmllllmmmmdo", //
            ".ommmmmmmmmdo.", //
            "..oddddddddo..", //
            "...o.o..o.o...", //
            "..............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A cap crawler: a crab scuttling under a springy rose mushroom cap with cream spots, facing
/// right (walk A/B, rearing to charge).
pub const CAP_CRAWLER: SpriteDef = SpriteDef {
    name: "enemy_cap_crawler",
    legend: &[
        ('o', INK),
        ('c', MAROON),
        ('r', ROSE),
        ('p', PINK),
        ('s', CREAM),
        ('g', STONE_DARK),
        ('b', SAND),
        ('B', LOAM),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..................", //
            "......oooooo......", //
            "....oopprrrroo....", //
            "...oprsrrrrsrro...", //
            "..oprrrrrsrrrrro..", //
            ".orrsrrrrrrrrsrro.", //
            "occcccccccccccccco", //
            ".ogggggggggggggo..", //
            "...obbbbbbbbbeo...", //
            "..oBbbbbbbbbbbbo..", //
            ".oB.oBo..oBo..Bo..", //
            ".o..o.o..o.o...o..", //
            "..................", //
        ],
        &[
            "..................", //
            "......oooooo......", //
            "....oopprrrroo....", //
            "...oprsrrrrsrro...", //
            "..oprrrrrsrrrrro..", //
            ".orrsrrrrrrrrsrro.", //
            "occcccccccccccccco", //
            ".ogggggggggggggo..", //
            "...obbbbbbbbbeo...", //
            "..oBbbbbbbbbbbbo..", //
            "..oBo..oBo..oBo...", //
            "..o.o..o.o..o.o...", //
            "..................", //
        ],
        &[
            ".......oooooo.....", //
            ".....oopprrrroo...", //
            "....oprsrrrrsrro..", //
            "...oprrrrrsrrrrro.", //
            "..orrsrrrrrrrrsrro", //
            ".occcccccccccccco.", //
            "..ogggggggggggoo..", //
            "...obbbbbbbbbbeoo.", //
            "..oBbbbbbbbbbbbbBo", //
            ".oBo.oBo..oBo..oo.", //
            ".o...o.o..o.o.....", //
            "..................", //
            "..................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// Glow gnats: a little cloud of lime-glowing gnats with whirring wings (they shimmer).
pub const GLOW_GNAT: SpriteDef = SpriteDef {
    name: "enemy_glow_gnat",
    legend: &[
        ('o', INK),
        ('g', LIME),
        ('G', CREAM),
        ('w', MINT),
        ('m', LEAF),
    ],
    frames: &[
        &[
            "...........", //
            ".w.....w...", //
            "wgw...wgw..", //
            ".m.....m...", //
            "....w......", //
            "...wGw...w.", //
            "....m...wgw", //
            ".w.......m.", //
            "wgw........", //
        ],
        &[
            "...........", //
            "..w.....w..", //
            ".wgw...wgw.", //
            "..m.....m..", //
            "...w.......", //
            "..wGw.....w", //
            "...m.....wg", //
            "w.........m", //
            "gw.........", //
        ],
    ],
    frame_seconds: 0.09,
};

/// A root snapper: a knot of roots lying still in a tunnel; awake, it splits into a toothed
/// maw with a lime eye (dormant, awake, snapping).
pub const ROOT_SNAPPER: SpriteDef = SpriteDef {
    name: "enemy_root_snapper",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('m', SOIL),
        ('l', LOAM),
        ('L', CLAY),
        ('g', MOSS),
        ('e', LIME),
        ('t', BONE),
        ('p', MAROON),
    ],
    frames: &[
        &[
            "................", //
            "......o..o......", //
            "....oolo.olo....", //
            "...olmlmolmmo...", //
            "..olmmlmmlmmmo..", //
            ".oLlmmlmmmlmmdo.", //
            ".olmglmmmmlmgdo.", //
            "olmmmlmmgmmmlmdo", //
            "olmmmmlmmmmlmmdo", //
            "odmmlmmmlmmmmddo", //
            ".oddmmmmmmmdddo.", //
            "o.oddddddddddo.o", //
            "................", //
        ],
        &[
            "................", //
            "......o..o......", //
            "....oolo.olo....", //
            "...olmlmolmmo...", //
            "..olmeemmlmmmo..", //
            ".oLlmeemmmlmmdo.", //
            ".olmglmmmmlmgdo.", //
            "otmtmtmtmtmtmtdo", //
            "otototototototdo", //
            "odmmlmmmlmmmmddo", //
            ".oddmmmmmmmdddo.", //
            "o.oddddddddddo.o", //
            "................", //
        ],
        &[
            "......o..o......", //
            "....oolo.olo....", //
            "...olmlmolmmo...", //
            "..olmeemmlmmmo..", //
            ".oLlmeemmmlmmdo.", //
            "otmtmtmtmtmtmtdo", //
            "opppppppppppppdo", //
            "oppppppppppppdo.", //
            "otmtmtmtmtmtmtdo", //
            "odmmlmmmlmmmmddo", //
            ".oddmmmmmmmdddo.", //
            "o.oddddddddddo.o", //
            "................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A bark beetle: a glossy copper-backed beetle with mandibles that bore root and rock,
/// facing right (jaws shut / open).
pub const BARK_BEETLE: SpriteDef = SpriteDef {
    name: "enemy_bark_beetle",
    legend: &[
        ('o', INK),
        ('d', RUST),
        ('m', COPPER),
        ('l', AMBER),
        ('k', SOIL_DARK),
        ('e', LIME),
        ('t', BONE),
    ],
    frames: &[
        &[
            "..............", //
            "...ooooooo....", //
            "..olllmmmdo...", //
            ".ollmmmmmmdoo.", //
            "olmmmmmmmmdeot", //
            "omdmmdmmmmdkko", //
            "odmmdmmdmmdkot", //
            ".oddddddddoo..", //
            "..o.o..o.o....", //
            "..............", //
        ],
        &[
            "..............", //
            "...ooooooo....", //
            "..olllmmmdo..t", //
            ".ollmmmmmmdoot", //
            "olmmmmmmmmdeo.", //
            "omdmmdmmmmdkko", //
            "odmmdmmdmmdko.", //
            ".oddddddddoot.", //
            ".o.o..o.o....t", //
            "..............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A thorn wisp: a drifting seed head of pale thorns round a glowing green heart (it turns).
pub const THORN_WISP: SpriteDef = SpriteDef {
    name: "enemy_thorn_wisp",
    legend: &[
        ('o', INK),
        ('t', SAND),
        ('T', BONE),
        ('g', LEAF),
        ('G', LIME),
        ('w', CREAM),
    ],
    frames: &[
        &[
            ".....t.....", //
            "..t..T..t..", //
            "...ToooT...", //
            "...oggGo...", //
            "tTTogGwoTTt", //
            "...oGggo...", //
            "...ToooT...", //
            "..t..T..t..", //
            ".....t.....", //
            "...........", //
            "...........", //
        ],
        &[
            "...........", //
            ".t.......t.", //
            "..T..t..T..", //
            "...oooo....", //
            "..TogGgoT..", //
            ".t.oGwgo.t.", //
            "..ToggGoT..", //
            "...oooo....", //
            "..T..t..T..", //
            ".t.......t.", //
            "...........", //
        ],
    ],
    frame_seconds: 0.25,
};

/// A Swarm soldier: a lean red-and-black wasp with a stinger and buzzing wings, facing right.
pub const HIVE_SOLDIER: SpriteDef = SpriteDef {
    name: "enemy_hive_soldier",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('m', MAROON),
        ('y', ORANGE),
        ('w', MIST),
        ('W', BONE),
        ('e', GOLD),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "....wWw.....", //
            "...wWWw.....", //
            "....oo.ooo..", //
            ".ooryrommmeo", //
            "orkykykrommo", //
            "omrykyrro.o.", //
            "o.oooooo....", //
            "..o..o......", //
            "............", //
        ],
        &[
            "............", //
            "....w.......", //
            "...wWwoooo..", //
            ".ooryrommmeo", //
            "orkykykrommo", //
            "omrykyrro.o.", //
            "o.oooooo....", //
            "...o..o.....", //
            "............", //
        ],
    ],
    frame_seconds: 0.05,
};

/// A Court drone: a plump amber worker with a gold-striped abdomen and soft wings, carrying
/// a drop of jelly, facing right.
pub const HIVE_DRONE: SpriteDef = SpriteDef {
    name: "enemy_hive_drone",
    legend: &[
        ('o', INK),
        ('a', AMBER),
        ('g', GOLD),
        ('c', CREAM),
        ('d', SOIL),
        ('w', SKY_LIGHT),
        ('W', ICE),
        ('e', NIGHT),
    ],
    frames: &[
        &[
            ".....wWw.....", //
            "....wWWWw....", //
            ".....ooo.ooo.", //
            "..oooagaoagao", //
            ".oadadadoaeao", //
            "oagdgdgaoaaao", //
            "oadadadao.oo.", //
            ".oaaaaaocco..", //
            "..ooooo.oo...", //
            ".............", //
        ],
        &[
            ".............", //
            ".....w.......", //
            "....wWWwooo..", //
            "..oooagaoagao", //
            ".oadadadoaeao", //
            "oagdgdgaoaaao", //
            "oadadadao.oo.", //
            ".oaaaaaocco..", //
            "..ooooo.oo...", //
            ".............", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A Swarm lancer: an armoured ant on its hind legs, its swollen green acid gland ready,
/// facing right (stance A/B, rearing to spit).
pub const ACID_LANCER: SpriteDef = SpriteDef {
    name: "enemy_acid_lancer",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('m', MAROON),
        ('k', NIGHT),
        ('g', LIME),
        ('G', LEAF),
        ('e', GOLD),
        ('t', BONE),
    ],
    frames: &[
        &[
            "..............", //
            "........ooo...", //
            ".......ommeo..", //
            "......omrrrot.", //
            "......omrrro..", //
            ".......oroo...", //
            "..oooo.omo....", //
            ".ogggGomrro...", //
            "oggGGGomrmo...", //
            "ogGGGGomrro...", //
            ".oGGGomrmo....", //
            "..oooomrro....", //
            "....o.o.o.o...", //
            "...o..o...o...", //
        ],
        &[
            "..............", //
            "........ooo...", //
            ".......ommeo..", //
            "......omrrrot.", //
            "......omrrro..", //
            ".......oroo...", //
            "..oooo.omo....", //
            ".ogggGomrro...", //
            "oggGGGomrmo...", //
            "ogGGGGomrro...", //
            ".oGGGomrmo....", //
            "..oooomrro....", //
            "...o.o.o.o....", //
            "...o..o..o....", //
        ],
        &[
            ".........ooo..", //
            "........ommeo.", //
            ".......omrrrot", //
            ".......omrrro.", //
            "..oooo..oroo..", //
            ".ogggGo.omo...", //
            "oggGGGGomrro..", //
            "ogGGGGGomrmo..", //
            "ogGGGGGomrro..", //
            ".oGGGGomrmo...", //
            "..ooooomrro...", //
            "......omro....", //
            "....o.o.o.o...", //
            "...o..o...o...", //
        ],
    ],
    frame_seconds: 0.0,
};

/// The brood matriarch: a vast bloated queen of the Swarm, her egg sac pulsing amber under
/// red chitin, a crown of horns and four gold eyes, facing right (breathing A/B, laying).
pub const SWARM_MATRIARCH: SpriteDef = SpriteDef {
    name: "enemy_swarm_matriarch",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('m', MAROON),
        ('k', NIGHT),
        ('a', AMBER),
        ('y', GOLD),
        ('c', CREAM),
        ('s', SAND),
        ('e', GOLD),
        ('t', BONE),
        ('p', PLUM),
    ],
    frames: &[
        &[
            "..............................", //
            "...................t..t.......", //
            "..................oto.oto.....", //
            ".................ommrmmrmo....", //
            "....ooooooooo...omrrrrrrrmo...", //
            "..ooaaaacaaaaoo.omreeerrrro...", //
            ".oaacaaaaaaacaaoomrrrrrreemo..", //
            "oaaaaayaaacaaaaaomrrrrrrrrrot.", //
            "oacaaaaaaaaaaayaommmrrrrrrot..", //
            "oaaaaacaaayaaaaaokommmrrrmo...", //
            "oayaaaaaaaaaacaaomkkommmmo....", //
            "oaaaaaaacaaaaaaaommkkoooo.....", //
            ".oaacaaaaaaayaaaommmmrro......", //
            ".oaaaaayaaaaaaaaommrrmro......", //
            "..ooaaaaaacaaaaommrrmmro......", //
            "....oossssssssooommmmmmo......", //
            "......oooooooo..oo.oo.oo......", //
            ".................o.o..o.o.....", //
            "................o..o..o..o....", //
            "..............................", //
            "..............................", //
        ],
        &[
            "..............................", //
            "...................t..t.......", //
            "..................oto.oto.....", //
            ".................ommrmmrmo....", //
            "...ooooooooooo..omrrrrrrrmo...", //
            ".ooaaaacaaaaaaoo.omreeerrrro..", //
            "oaacaaaaaaacaaaaoomrrrrreemo..", //
            "oaaaaayaaacaaaaaaomrrrrrrrrrot", //
            "oacaaaaaaaaaaayaaommmrrrrrrot.", //
            "oaaaaacaaayaaaaaaokommmrrrmo..", //
            "oayaaaaaaaaaacaaaomkkommmmo...", //
            "oaaaaaaacaaaaaaaaommkkoooo....", //
            "oaaacaaaaaaayaaaaommmmrro.....", //
            ".oaaaaayaaaaaaaaaommrrmro.....", //
            "..ooaaaaaacaaaaaommrrmmro.....", //
            "....oossssssssssoommmmmmo.....", //
            "......oooooooooo.oo.oo.oo.....", //
            ".................o.o..o.o.....", //
            "................o..o..o..o....", //
            "..............................", //
            "..............................", //
        ],
        &[
            "..............................", //
            "...................t..t.......", //
            "..................oto.oto.....", //
            ".................ommrmmrmo....", //
            "....ooooooooo...omrrrrrrrmo...", //
            "..ooaaaacaaaaoo.omreeerrrro...", //
            ".oaacaaaaaaacaaoomrrrrrreemo..", //
            "oaaaaayaaacaaaaaomrrrrrrrrrot.", //
            "oacaaaaaaaaaaayaommmrrrrrrot..", //
            "oaaaaacaaayaaaaaokommmrrrmo...", //
            "oayaaaaaaaaaacaaomkkommmmo....", //
            "oaaaaaaacaaaaaaaommkkoooo.....", //
            ".oaacaaaaaaayaaaommmmrro......", //
            ".oaaaaayaaaaaaaaommrrmro......", //
            "..ooaaaaaacaaaaommrrmmro......", //
            "..ccoossssssssooommmmmmo......", //
            ".ccccooooooooo..oo.oo.oo......", //
            "..cc.............o.o..o.o.....", //
            "................o..o..o..o....", //
            "..............................", //
            "..............................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A maw worm: a pale pink segmented worm whose head opens into a ring of teeth, facing right
/// (closed / gaping).
pub const MAW_WORM: SpriteDef = SpriteDef {
    name: "enemy_maw_worm",
    legend: &[
        ('o', INK),
        ('p', PINK),
        ('r', ROSE),
        ('m', MAROON),
        ('c', CREAM),
        ('t', BONE),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "................", //
            "..ooo.ooo.oooo..", //
            ".oppoopproprrro.", //
            "opcpopcppocprrro", //
            "oppprppprpppmrto", //
            "orrprrrprrrrmmko", //
            ".ormoormmomrmto.", //
            "..ooo.ooo.oooo..", //
            "................", //
            "................", //
            "................", //
        ],
        &[
            "................", //
            "..ooo.ooo.ooooo.", //
            ".oppoopproprrot.", //
            "opcpopcppocprmk.", //
            "oppprppprpppmmk.", //
            "orrprrrprrrrmmk.", //
            ".ormoormmomrmot.", //
            "..ooo.ooo.ooooo.", //
            "................", //
            "................", //
            "................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A bile polyp: a swollen fleshy sac rooted to the cavern, veined maroon, its sphincter
/// leaking bile (budding pulses).
pub const BILE_POLYP: SpriteDef = SpriteDef {
    name: "enemy_bile_polyp",
    legend: &[
        ('o', INK),
        ('p', PINK),
        ('r', ROSE),
        ('m', MAROON),
        ('c', CREAM),
        ('g', LIME),
        ('G', LEAF),
    ],
    frames: &[
        &[
            "................", //
            "......oooo......", //
            ".....oggGGo.....", //
            "....orrooorro...", //
            "...orprrrrprro..", //
            "..orppcprrrprro.", //
            "..orpmprrmrrpro.", //
            ".orrppprrrmprrro", //
            ".orprrmppprrrmro", //
            ".orrmrrrrrmprrro", //
            ".ormrrprrrrrmrro", //
            "..ormrrrmrrrmro.", //
            "..ommrrrrrrrmmo.", //
            ".ommmmmmmmmmmmmo", //
            "oooooooooooooooo", //
        ],
        &[
            "................", //
            ".....oooooo.....", //
            "....oggGGGgo....", //
            "...orrooooorro..", //
            "..orpprrrrrprro.", //
            ".orppcprrrrrprro", //
            ".orpmprrrmrrrpro", //
            ".orrppprrrrmprro", //
            ".orprrmppprrrmro", //
            ".orrmrrrrrrmprro", //
            ".ormrrprrrrrrmro", //
            "..ormrrrmrrrrmo.", //
            "..ommrrrrrrrrmo.", //
            ".ommmmmmmmmmmmmo", //
            "oooooooooooooooo", //
        ],
    ],
    frame_seconds: 0.5,
};

/// A leechling: a small black-red leech with a sucker mouth, wriggling, facing right.
pub const LEECHLING: SpriteDef = SpriteDef {
    name: "enemy_leechling",
    legend: &[
        ('o', INK),
        ('m', MAROON),
        ('r', RED),
        ('p', ROSE),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "..........", //
            "..oooooo..", //
            ".ormrmrmo.", //
            "ormrmrmrko", //
            ".oooooooo.", //
            "..........", //
        ],
        &[
            "..........", //
            ".ooooooo..", //
            "ormrmrmroo", //
            ".ormrmrmko", //
            "..ooooooo.", //
            "..........", //
        ],
    ],
    frame_seconds: 0.15,
};

/// A lure fish: a slim deep-blue fish with a glowing mint stripe and tail, facing right.
pub const LURE_FISH: SpriteDef = SpriteDef {
    name: "enemy_lure_fish",
    legend: &[
        ('o', INK),
        ('b', DEEP_BLUE),
        ('B', OCEAN),
        ('g', MINT),
        ('G', ICE),
        ('e', CREAM),
    ],
    frames: &[
        &[
            "............", //
            "o....ooooo..", //
            "goo.oBBBBBo.", //
            "ogggggggggeo", //
            "goo.obbbbbo.", //
            "o....ooooo..", //
            "............", //
            "............", //
        ],
        &[
            "............", //
            ".o...ooooo..", //
            ".goooBBBBBo.", //
            ".ogggggggGeo", //
            ".goooobbbbo.", //
            ".o...ooooo..", //
            "............", //
            "............", //
        ],
    ],
    frame_seconds: 0.2,
};

/// A glow jelly: a translucent teal bell lit from within, trailing glowing tendrils (pulse).
pub const GLOW_JELLY: SpriteDef = SpriteDef {
    name: "enemy_glow_jelly",
    legend: &[
        ('o', TEAL_DARK),
        ('t', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('w', BONE),
    ],
    frames: &[
        &[
            "....oooo....", //
            "..oottttoo..", //
            ".otmmiimmto.", //
            "otmiwwwwimto", //
            "otmmiiiimmto", //
            "otttmmmmttto", //
            ".ooooooooo..", //
            "..m.t..t.m..", //
            "..t.m..m.t..", //
            "...m.t.t.m..", //
            "...t.m.m.t..", //
            "....m...m...", //
            "............", //
        ],
        &[
            "............", //
            "....oooo....", //
            "..oottttoo..", //
            ".otmmiimmto.", //
            "otmiwwwwimto", //
            "otmmiiiimmto", //
            ".otttmmmtto.", //
            "..ooooooo...", //
            "..t.m..m.t..", //
            "..m.t..t.m..", //
            "..t..m.m.t..", //
            "...m.t.t.m..", //
            "....t...t...", //
        ],
    ],
    frame_seconds: 0.45,
};

/// An angler: a black, needle-toothed fish of the deep with a glowing lure on a rod over its
/// head, facing right (lurking, gaping lunge).
pub const ANGLER: SpriteDef = SpriteDef {
    name: "enemy_angler",
    legend: &[
        ('o', INK),
        ('k', NIGHT),
        ('d', SLATE_DARK),
        ('s', SLATE),
        ('t', BONE),
        ('g', MINT),
        ('G', ICE),
        ('e', CREAM),
        ('r', MAROON),
    ],
    frames: &[
        &[
            "...........ooo......", //
            "...........oGgo.....", //
            "............ooo.....", //
            "........ooooo.o.....", //
            "......oodddddoo.....", //
            "....oodsssssddooo...", //
            "..ooddssssssssddeo..", //
            "oodsdsssssssssssdoo.", //
            "ookdsssssssstststto.", //
            "oodsdssssssstototoo.", //
            "..oodsssssssssssddo.", //
            "....oodddddddddoo...", //
            "......oooooooooo....", //
            "....................", //
        ],
        &[
            "..........ooo.......", //
            "..........oGgo......", //
            "...........ooo......", //
            "........ooooo.o.....", //
            "......oodddddoo.....", //
            "....oodsssssddooo...", //
            "..ooddssssssssddeoo.", //
            "oodsdssssssssstttt..", //
            "ookdsssssssssrrrrr..", //
            "oodsdssssssssrrrrr..", //
            "..oodsssssssstttt...", //
            "....oodddddddddoo...", //
            "......oooooooooo....", //
            "....................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// The Mycelial Mind: a vast brain of knotted fungus, veined with glowing spore light, a
/// single great eye in its folds and roots trailing below (pulse, staring).
pub const MYCELIAL_MIND: SpriteDef = SpriteDef {
    name: "enemy_mycelial_mind",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('m', VIOLET),
        ('l', LAVENDER),
        ('p', PINK),
        ('g', LIME),
        ('G', CREAM),
        ('e', GOLD),
        ('r', RED),
        ('b', BONE),
        ('s', SAND),
    ],
    frames: &[
        &[
            "...........ooooooo.........", //
            "........oooldlmlldooo......", //
            "......oollmlddllmmllloo....", //
            ".....olmmlgldmmllgdmmllo...", //
            "....olmdllmmllddmmllgmmlo..", //
            "...olllgmmdldmllmddlmmdllo.", //
            "..olmdllmmlllooooolllgmmdlo", //
            "..olmmdlgmlloobbbbbolldmmlo", //
            ".olldmmllmlobbbeeebbolmmdlo", //
            ".olmmllgdmlobbeerrebbollmlo", //
            ".oldlmmmdllobbbeeebbolgmldo", //
            ".olmgmlldmmloobbbbbollmmldo", //
            "..olmmdllgmmlloooooldmmldo.", //
            "..oldmmlmmdlmmllgmmllmmlo..", //
            "...oldmmllgmmdlmmdllgmdlo..", //
            "....odddmmdddmmddddmmddo...", //
            ".....ooddoddooddoddoodo....", //
            "......osoo.oso.oso.osoo....", //
            ".....os...os..os..os.......", //
            "....os...os...os...os......", //
            "....s....s.....s....s......", //
            "...........................", //
        ],
        &[
            "...........ooooooo.........", //
            "........oooldlmlldooo......", //
            "......oollmlddllmmllloo....", //
            ".....olmmlGldmmllGdmmllo...", //
            "....olmdllmmllddmmllGmmlo..", //
            "...olllGmmdldmllmddlmmdllo.", //
            "..olmdllmmlllooooolllGmmdlo", //
            "..olmmdlGmlloobbbbbolldmmlo", //
            ".olldmmllmlobbbeeebbolmmdlo", //
            ".olmmllGdmlobbeerrebbollmlo", //
            ".oldlmmmdllobbbeeebbolGmldo", //
            ".olmGmlldmmloobbbbbollmmldo", //
            "..olmmdllGmmlloooooldmmldo.", //
            "..oldmmlmmdlmmllGmmllmmlo..", //
            "...oldmmllGmmdlmmdllGmdlo..", //
            "....odddmmdddmmddddmmddo...", //
            ".....ooddoddooddoddoodo....", //
            "......osoo.oso.oso.osoo....", //
            "......os...os..os...os.....", //
            ".....os...os....os...os....", //
            ".....s....s......s....s....", //
            "...........................", //
        ],
    ],
    frame_seconds: 0.6,
};

/// A nerve node of the Mycelial Mind: a pulsing violet sac hanging from a mycelium strand,
/// its core glowing lime (pulse).
pub const MIND_NODE: SpriteDef = SpriteDef {
    name: "enemy_mind_node",
    legend: &[
        ('o', INK),
        ('s', SAND),
        ('d', PLUM),
        ('m', VIOLET),
        ('l', LAVENDER),
        ('g', LIME),
        ('G', CREAM),
    ],
    frames: &[
        &[
            ".....s......", //
            ".....s......", //
            "....oso.....", //
            "...ollmo....", //
            "..ollmmmo...", //
            ".olmgggmmo..", //
            ".olmgGgmdo..", //
            ".olmgggmdo..", //
            ".ommmmmddo..", //
            "..odmmmdo...", //
            "...odddo....", //
            "....ooo.....", //
            "............", //
        ],
        &[
            ".....s......", //
            ".....s......", //
            "....oso.....", //
            "...ollmmo...", //
            "..ollmmmmo..", //
            ".olmggggmmo.", //
            ".olmgGGgmdo.", //
            ".olmggggmdo.", //
            ".ommmmmmddo.", //
            "..odmmmmdo..", //
            "...oddddo...", //
            "....oooo....", //
            "............", //
        ],
    ],
    frame_seconds: 0.5,
};

/// A spore ball: a fuzzy lavender puff trailing green motes.
pub const SHOT_SPORE: SpriteDef = SpriteDef {
    name: "shot_spore",
    legend: &[('o', PLUM), ('l', LAVENDER), ('g', LIME), ('w', PINK)],
    frames: &[
        &[
            "..oo..", //
            ".ollo.", //
            "olwllo", //
            "ollglo", //
            ".ollo.", //
            "..oo..", //
        ],
        &[
            ".g....", //
            "..oo..", //
            ".olwo.", //
            "olllgo", //
            ".ollo.", //
            "..oo.g", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A thorn: a pale barbed spike, pointing right.
pub const SHOT_THORN: SpriteDef = SpriteDef {
    name: "shot_thorn",
    legend: &[('o', INK), ('t', SAND), ('T', BONE)],
    frames: &[&[
        ".o.....", //
        "ottTTTo", //
        ".o.....", //
    ]],
    frame_seconds: 0.0,
};

/// The Mind's spore orb: a slow violet orb wreathed in glowing spores.
pub const SHOT_MIND_SPORE: SpriteDef = SpriteDef {
    name: "shot_mind_spore",
    legend: &[
        ('o', PLUM),
        ('m', VIOLET),
        ('l', LAVENDER),
        ('g', LIME),
        ('G', CREAM),
    ],
    frames: &[
        &[
            "g..oo..g", //
            "..ommo..", //
            ".omllmo.", //
            "omlGGlmo", //
            "omlGGlmo", //
            ".omllmo.", //
            "..ommo..", //
            "g..oo..g", //
        ],
        &[
            "...oo...", //
            ".gommog.", //
            ".omllmo.", //
            "omlGGlmo", //
            "omlGGlmo", //
            ".omllmo.", //
            ".gommog.", //
            "...oo...", //
        ],
    ],
    frame_seconds: 0.18,
};
