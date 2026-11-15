//! Enemy sprites (WP-ENEMIES): one animated sprite per `fatherload_simulation::EnemyKind`
//! and the enemy shots. Asymmetric creatures and machines face right (the renderer mirrors
//! them); symmetric ones are authored as mirrored halves. Generated grids, hand-tuned.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A cave bat: leathery violet wings, red eyes and a fang (wing flap).
pub const CAVE_BAT: SpriteDef = SpriteDef {
    name: "enemy_cave_bat",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('m', VIOLET),
        ('e', RED),
        ('w', BONE),
    ],
    frames: &[
        &[
            "o...........o", //
            "oo.........oo", //
            "omo.......omo", //
            "ommo.ooo.ommo", //
            "ommmodddommmo", //
            ".ommdededmmo.", //
            "..oomdddmoo..", //
            "....odwdo....", //
            ".....ooo.....", //
            ".............", //
        ],
        &[
            ".............", //
            ".............", //
            ".............", //
            "o....ooo....o", //
            "oo..odddo..oo", //
            "ommoodddoommo", //
            "ommmdededmmmo", //
            ".oommdddmmoo.", //
            "....odwdo....", //
            ".....ooo.....", //
        ],
        &[
            ".............", //
            ".....ooo.....", //
            "....odddo....", //
            "...odededo...", //
            "..oomdddmoo..", //
            ".ommodwdommo.", //
            "ommo.ooo.ommo", //
            "omo.......omo", //
            "oo.........oo", //
            "o...........o", //
        ],
        &[
            ".............", //
            ".............", //
            ".............", //
            "o....ooo....o", //
            "oo..odddo..oo", //
            "ommoodddoommo", //
            "ommmdededmmmo", //
            ".oommdddmmoo.", //
            "....odwdo....", //
            ".....ooo.....", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A burrowing worm's head and first segment, facing right (jaws shut / open).
pub const BURROWER: SpriteDef = SpriteDef {
    name: "enemy_burrower",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', COPPER),
        ('d', RUST),
        ('k', SOIL_DARK),
        ('e', GOLD),
        ('t', CREAM),
        ('p', MAROON),
    ],
    frames: &[
        &[
            "...oooo.oooo..", //
            "..ommmmommmmo.", //
            ".omllmmollmmmo", //
            "omllmdmoomeemo", //
            "omldddmdmmmmto", //
            "omddkddmddmtto", //
            ".odkkdooddmmo.", //
            "..oooo..oooo..", //
            "..............", //
        ],
        &[
            "...oooo.oooo..", //
            "..ommmmommmmo.", //
            ".omllmmollmmmo", //
            "omllmdmoomeeo.", //
            "omldddmdmmmot.", //
            "omddkddmddmpp.", //
            ".odkkdooddmtto", //
            "..oooo..ooooo.", //
            "..............", //
        ],
    ],
    frame_seconds: 0.18,
};

/// An acid spitter: a squat slug under a bulging acid sac, facing right (walk A/B, puffed).
pub const ACID_SPITTER: SpriteDef = SpriteDef {
    name: "enemy_acid_spitter",
    legend: &[
        ('o', INK),
        ('l', LIME),
        ('W', CREAM),
        ('m', LEAF),
        ('M', MOSS),
        ('e', GOLD),
        ('k', INK),
        ('w', BONE),
        ('p', MAROON),
    ],
    frames: &[
        &[
            "................", //
            "....oooooo......", //
            "..oollWWlloo....", //
            ".ollWWllllllo...", //
            ".olWllllllllmo..", //
            "ollllllllmmmmoo.", //
            "olllllmmmmmmmeko", //
            "ommmmmmmmmmmmmwo", //
            "oMMmmmmmmmmmmmpo", //
            ".oMMMMMMMMMMMMo.", //
            "..oo.oo..oo.oo..", //
            "................", //
        ],
        &[
            "................", //
            "....oooooo......", //
            "..oollWWlloo....", //
            ".ollWWllllllo...", //
            ".olWllllllllmo..", //
            "ollllllllmmmmoo.", //
            "olllllmmmmmmmeko", //
            "ommmmmmmmmmmmmwo", //
            "oMMmmmmmmmmmmmpo", //
            ".oMMMMMMMMMMMMo.", //
            "...oo.oo..oo.oo.", //
            "................", //
        ],
        &[
            "...ooooooo......", //
            ".oollWWWlloo....", //
            "ollWWWllllllo...", //
            "olWWllllllllloo.", //
            "olWllllllllllmo.", //
            "olllllllllmmmmoo", //
            "olllllmmmmmmmeko", //
            "ommmmmmmmmmmmmwo", //
            "oMMmmmmmmmmmmmpo", //
            ".oMMMMMMMMMMMMo.", //
            "..oo.oo..oo.oo..", //
            "................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A rock mimic: a boulder until it wakes, then eyes, fangs and a red maw.
pub const ROCK_MIMIC: SpriteDef = SpriteDef {
    name: "enemy_rock_mimic",
    legend: &[
        ('o', INK),
        ('L', MIST),
        ('l', STONE_LIGHT),
        ('m', STONE),
        ('d', STONE_DARK),
        ('e', RED),
        ('E', GOLD),
        ('t', BONE),
        ('p', MAROON),
        ('r', RED),
    ],
    frames: &[
        &[
            "......oooooo......", //
            "....ooLLllmmoo....", //
            "..ooLllmmmmmmmoo..", //
            ".oLllmmmmmmmdmmmo.", //
            ".ollmmmmmmmmmmmddo", //
            "olmmmmmdmmmmmmmmdo", //
            "ommmmmmmmmmmmdmmdo", //
            "ommmmmmmmmmmmmmddo", //
            "ommdmmmmmmmmmmmddo", //
            "odmmmmmmmmmmmmdddo", //
            ".oddmmmmmmmmmdddo.", //
            "..ooddddddddddoo..", //
            "....oooooooooo....", //
            "..................", //
        ],
        &[
            "......oooooo......", //
            "....ooLLllmmoo....", //
            "..ooLllmmmmmmmoo..", //
            ".oLllmmmmmmmdmmmo.", //
            ".olleEmmmmmmeEmddo", //
            "olmmmmmdmmmmmmmmdo", //
            "ommmmmmmmmmmmdmmdo", //
            "otmtmtmtmtmtmtmtdo", //
            "otototototototoddo", //
            "odmmmmmmmmmmmmdddo", //
            ".oddmmmmmmmmmdddo.", //
            "..ooddddddddddoo..", //
            "....oooooooooo....", //
            ".o.o..........o.o.", //
        ],
        &[
            "......oooooo......", //
            "....ooLLllmmoo....", //
            "..ooLllmmmmmmmoo..", //
            ".oLllmmmmmmmdmmmo.", //
            ".olleEmmmmmmeEmddo", //
            "olmmmmmdmmmmmmmmdo", //
            "otmtmtmtmtmtmtmtdo", //
            "oppppppppppppppddo", //
            "opppprpppprppppddo", //
            "otmtmtmtmtmtmtmddo", //
            ".oddmmmmmmmmmdddo.", //
            "..ooddddddddddoo..", //
            "....oooooooooo....", //
            ".o.o..........o.o.", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A shellback: a slow crawler under heavy banded plates, facing right (walk A/B).
pub const SHELLBACK: SpriteDef = SpriteDef {
    name: "enemy_shellback",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', CLAY),
        ('d', RUST),
        ('S', SOIL),
        ('h', LOAM),
        ('e', GOLD),
        ('t', CREAM),
        ('k', SOIL_DARK),
    ],
    frames: &[
        &[
            "......oooooooo........", //
            "....oollllllmmoo......", //
            "...ollllmmmmmmmdo.....", //
            "..ollmmoddmmmoddmo....", //
            ".ollmmoddmmmoddmmdo...", //
            ".olmmmoddmmmoddmmdooo.", //
            "olmmmmoddmmmoddmmdhhho", //
            "olmmmmoddmmmoddmmdhheo", //
            "odddddddddddddddddohho", //
            ".oSSSSSSSSSSSSSSSoohto", //
            "..okk.okk..okk.okk.oo.", //
            "..ok..ok...ok..ok.....", //
            ".ok..ok...ok..ok......", //
            "......................", //
        ],
        &[
            "......oooooooo........", //
            "....oollllllmmoo......", //
            "...ollllmmmmmmmdo.....", //
            "..ollmmoddmmmoddmo....", //
            ".ollmmoddmmmoddmmdo...", //
            ".olmmmoddmmmoddmmdooo.", //
            "olmmmmoddmmmoddmmdhhho", //
            "olmmmmoddmmmoddmmdhheo", //
            "odddddddddddddddddohho", //
            ".oSSSSSSSSSSSSSSSoohto", //
            "..okk.okk..okk.okk.oo.", //
            "...ok..ok...ok..ok....", //
            "....ok..ok...ok..ok...", //
            "......................", //
        ],
    ],
    frame_seconds: 0.25,
};

/// A magma wisp: a living flame with a white-hot core and two dark eyes (flicker).
pub const MAGMA_WISP: SpriteDef = SpriteDef {
    name: "enemy_magma_wisp",
    legend: &[
        ('o', MAROON),
        ('r', RED),
        ('a', ORANGE),
        ('g', AMBER),
        ('c', CREAM),
        ('k', INK),
    ],
    frames: &[
        &[
            "...o..o...", //
            "..oro.oro.", //
            ".oraroorao", //
            ".oraaaraao", //
            "oraagggaro", //
            "oragkggkro", //
            "oragcccgro", //
            ".oraggggao", //
            ".orraaaaro", //
            "..orrrrro.", //
            "...oooooo.", //
            "..........", //
        ],
        &[
            "..o...o...", //
            ".oro.oro..", //
            ".oraroorao", //
            ".oraaaraao", //
            "oraagggaro", //
            "oragkggkro", //
            "oragcccgro", //
            ".oraggggao", //
            ".orraaaaro", //
            "..orrrrro.", //
            "...oooooo.", //
            "..........", //
        ],
        &[
            "....o..o..", //
            "...oro.oro", //
            ".oraroorao", //
            ".oraaaraao", //
            "oraagggaro", //
            "oragkggkro", //
            "oragcccgro", //
            ".oraggggao", //
            ".orraaaaro", //
            "..orrrrro.", //
            "...oooooo.", //
            "..........", //
        ],
    ],
    frame_seconds: 0.1,
};

/// A brood nest: a fleshy mound riddled with burrows and glowing eggs (pulse).
pub const BROOD_NEST: SpriteDef = SpriteDef {
    name: "enemy_brood_nest",
    legend: &[
        ('o', INK),
        ('W', CREAM),
        ('l', PINK),
        ('m', ROSE),
        ('d', PLUM),
        ('h', NIGHT),
        ('g', LIME),
        ('G', LEAF),
        ('L', CREAM),
    ],
    frames: &[
        &[
            "....................", //
            "......oooooooo......", //
            "....ooWllllllWoo....", //
            "...oWlllmmmmlllWo...", //
            "..oWllmoooooomllWo..", //
            "..olmmohhhhhhommlo..", //
            ".olllmohhhhhhomlllo.", //
            ".olmmmmoooooommmmlo.", //
            "olmgGmmmmmmmmmmGgmlo", //
            "olmGGmmohhhhommGGmlo", //
            "ommmmmmohhhhommmmmmo", //
            "omdmmmmmoooommmmmdmo", //
            "oddmmmmmmmmmmmmmmddo", //
            "oddddmmdmmmmdmmddddo", //
            ".oddddddddddddddddo.", //
            "..oooooooooooooooo..", //
        ],
        &[
            "....................", //
            "......oooooooo......", //
            "....ooWllllllWoo....", //
            "...oWlllmmmmlllWo...", //
            "..oWllmoooooomllWo..", //
            "..olmmohhhhhhommlo..", //
            ".olllmohhhhhhomlllo.", //
            ".olmmmmoooooommmmlo.", //
            "olmLgmmmmmmmmmmgLmlo", //
            "olmggmmohhhhommggmlo", //
            "ommmmmmohhhhommmmmmo", //
            "omdmmmmmoooommmmmdmo", //
            "oddmmmmmmmmmmmmmmddo", //
            "oddddmmdmmmmdmmddddo", //
            ".oddddddddddddddddo.", //
            "..oooooooooooooooo..", //
        ],
    ],
    frame_seconds: 0.5,
};

/// The brood lord: a horned, four-eyed elite with a fanged maw (idle, roar).
pub const BROOD_LORD: SpriteDef = SpriteDef {
    name: "enemy_brood_lord",
    legend: &[
        ('o', INK),
        ('w', BONE),
        ('l', LAVENDER),
        ('m', VIOLET),
        ('d', PLUM),
        ('e', RED),
        ('E', ORANGE),
        ('t', BONE),
        ('p', MAROON),
        ('r', ORANGE),
    ],
    frames: &[
        &[
            "ow..........................wo", //
            "oww........................wwo", //
            ".oww......................wwo.", //
            "..oww....ooooo..ooooo....wwo..", //
            "...owoooommmmmmmmmmmmoooowo...", //
            "...oommmllllllllllllllmmmoo...", //
            "...omllllllllllllllllllllmo...", //
            "..omllllllllllllllllllllllmo..", //
            "..omlllEelllllllllllleElllmo..", //
            "..omlllleelllllllllleellllmo..", //
            ".omlllllllllmmmmmmlllllllllmo.", //
            ".omllmmmmmmmmmmmmmmmmmmmmllmo.", //
            "omllmmoooooooooooooooooommllmo", //
            "omllmmotttottttttttotttommllmo", //
            "omlmmmoppppppppppppppppommmlmo", //
            "omlmmmoppprpppppppprpppommmlmo", //
            "ommmmmotttottttttttotttommmmmo", //
            "odmmmmmoooooooooooooooommmmmdo", //
            "oddmmmmmmmmmmmmmmmmmmmmmmmmddo", //
            ".odddmmmmmmmmddddmmmmmmmmdddo.", //
            "..oddddddddddooooddddddddddo..", //
            "...ooddoooddo....oddoooddoo...", //
            "....oddo.oddo....oddo.oddo....", //
            "....oooo.oooo....oooo.oooo....", //
        ],
        &[
            "ow..........................wo", //
            "oww........................wwo", //
            ".oww......................wwo.", //
            "..oww....ooooo..ooooo....wwo..", //
            "...owoooommmmmmmmmmmmoooowo...", //
            "...oommmllllllllllllllmmmoo...", //
            "...omllllllllllllllllllllmo...", //
            "..omllllllllllllllllllllllmo..", //
            "..omlllEelllllllllllleElllmo..", //
            "..omlllleelllllllllleellllmo..", //
            ".omlllllllllmmmmmmlllllllllmo.", //
            ".omllmmmmmmmmmmmmmmmmmmmmllmo.", //
            "omllmmotttottttttttotttommllmo", //
            "omllmmoppppppppppppppppommllmo", //
            "omlmmmoppprpppppppprpppommmlmo", //
            "omlmmmopprrrrpppprrrrppommmlmo", //
            "ommmmmoppppppppppppppppommmmmo", //
            "odmmmmmotttottttttotttommmmmdo", //
            "oddmmmmmmmmmmmmmmmmmmmmmmmmddo", //
            ".odddmmmmmmmmddddmmmmmmmmdddo.", //
            "..oddddddddddooooddddddddddo..", //
            "...ooddoooddo....oddoooddoo...", //
            "....oddo.oddo....oddo.oddo....", //
            "....oooo.oooo....oooo.oooo....", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A rogue mining drone: a steel pod on a rotor with a red lens (rotor A/B).
pub const MINING_DRONE: SpriteDef = SpriteDef {
    name: "enemy_mining_drone",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('S', STEEL_LIGHT),
        ('d', STEEL_DARK),
        ('k', INK),
        ('e', RED),
        ('y', AMBER),
        ('r', MIST),
    ],
    frames: &[
        &[
            ".rrrr..rrrr.", //
            "....oooo....", //
            "..oossssoo..", //
            ".osSSSSSSso.", //
            "osSkeekSSsdo", //
            "ossssssssddo", //
            ".oddddddddo.", //
            "...oyoyoo...", //
            "....oddo....", //
        ],
        &[
            "..rrrrrrrr..", //
            "....oooo....", //
            "..oossssoo..", //
            ".osSSSSSSso.", //
            "osSkeekSSsdo", //
            "ossssssssddo", //
            ".oddddddddo.", //
            "...oyoyoo...", //
            "....oddo....", //
        ],
    ],
    frame_seconds: 0.05,
};

/// An ancient sentry turret on its plinth, barrel to the right (idle, lens lit).
pub const SENTRY_TURRET: SpriteDef = SpriteDef {
    name: "enemy_sentry_turret",
    legend: &[
        ('o', INK),
        ('s', STEEL),
        ('S', STEEL_LIGHT),
        ('d', STEEL_DARK),
        ('b', SLATE),
        ('e', RED),
        ('G', GOLD),
        ('y', AMBER),
    ],
    frames: &[
        &[
            "..............", //
            ".....oooo.....", //
            "....oSSSSo....", //
            "...oSSssssoooo", //
            "...oSseesbbbbo", //
            "...osssssooooo", //
            "....osssso....", //
            ".....oddo.....", //
            "...oyoyoyoyo..", //
            "..oddddddddo..", //
            ".oddddddddddo.", //
            "oddsddddddsddo", //
            "oooooooooooooo", //
        ],
        &[
            "..............", //
            ".....oooo.....", //
            "....oSSSSo....", //
            "...oSSssssoooo", //
            "...oSsGGsbbbbo", //
            "...osssssooooo", //
            "....osssso....", //
            ".....oddo.....", //
            "...oyoyoyoyo..", //
            "..oddddddddo..", //
            ".oddddddddddo.", //
            "oddsddddddsddo", //
            "oooooooooooooo", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A rogue excavator: a rusted tracked digger with a spinning drill, facing right (spin A/B).
pub const ROGUE_EXCAVATOR: SpriteDef = SpriteDef {
    name: "enemy_rogue_excavator",
    legend: &[
        ('o', INK),
        ('y', AMBER),
        ('a', GOLD),
        ('r', RUST),
        ('w', ICE),
        ('k', SLATE_DARK),
        ('d', STEEL_DARK),
        ('s', STEEL),
        ('S', STEEL_LIGHT),
    ],
    frames: &[
        &[
            "...oooooooo.............", //
            "..oyyyyyyyyo............", //
            "..oywwwwkyyo............", //
            "..oywwwwkyyo............", //
            "..oyyyyyyyyo.....ooo....", //
            ".ooyyyyyyyyyoo..oSSSo...", //
            "oaayyyyyyyyyyyoosSSSSo..", //
            "oyyyyrryyyyyyyossSSSSSo.", //
            "oyyyyrryyyyyyyosssSSSSSo", //
            "oyyyyyyyyyyyyyossSSSSSo.", //
            "oddddddddddddooosSSSSo..", //
            "okokokokokokoko.oSSSo...", //
            "okkkkkkkkkkkkko..ooo....", //
            "okokokokokokoko.........", //
            ".ooooooooooooo..........", //
            "........................", //
        ],
        &[
            "...oooooooo.............", //
            "..oyyyyyyyyo............", //
            "..oywwwwkyyo............", //
            "..oywwwwkyyo............", //
            "..oyyyyyyyyo.....ooo....", //
            ".ooyyyyyyyyyoo..ossso...", //
            "oaayyyyyyyyyyyooSsssso..", //
            "oyyyyrryyyyyyyoSSssssso.", //
            "oyyyyrryyyyyyyoSSSssssso", //
            "oyyyyyyyyyyyyyoSSssssso.", //
            "oddddddddddddoooSsssso..", //
            "ookokokokokokoo.oSSSo...", //
            "okkkkkkkkkkkkko..ooo....", //
            "ookokokokokokoo.........", //
            ".ooooooooooooo..........", //
            "........................", //
        ],
    ],
    frame_seconds: 0.08,
};

/// An ancient guardian: a floating gold-and-stone construct with a glowing eye (pulse).
pub const ANCIENT_GUARDIAN: SpriteDef = SpriteDef {
    name: "enemy_ancient_guardian",
    legend: &[
        ('o', INK),
        ('G', GOLD),
        ('g', AMBER),
        ('s', STONE_LIGHT),
        ('m', STONE),
        ('c', MINT),
        ('C', TEAL),
    ],
    frames: &[
        &[
            "..........oooooo..........", //
            "........ooGGGGGGoo........", //
            ".......oGGggggggGGo.......", //
            "......oGgssssssssgGo......", //
            ".....oGgsmmmmmmmmsgGo.....", //
            "....oogsmmoooooommsgoo....", //
            "...ossgsmoccccccomsgsso...", //
            "..ossssgsmocCCcomsgsssso..", //
            ".osssssgsmocCCcomsgssssso.", //
            "osssooosgsmoccomsgsooossso", //
            "osso..osgsmmoommsgso..osso", //
            ".oo...osgssmmmmssgso...oo.", //
            "......ossgssssssgsso......", //
            "......osGGssssssGGso......", //
            ".....osGoosGssGsooGso.....", //
            ".....osoGGossssoGGoso.....", //
            "......o.oGGossoGGo.o......", //
            "........osGGooGGso........", //
            "........oGGo..oGGo........", //
            ".........oGo..oGo.........", //
            "..........o....o..........", //
            "..........................", //
        ],
        &[
            "..........oooooo..........", //
            "........ooGGGGGGoo........", //
            ".......oGGggggggGGo.......", //
            "......oGgssssssssgGo......", //
            ".....oGgsmmmmmmmmsgGo.....", //
            "....oogsmmoooooommsgoo....", //
            "...ossgsmoCCCCCComsgsso...", //
            "..ossssgsmoCccComsgsssso..", //
            ".osssssgsmoCccComsgssssso.", //
            "osssooosgsmoCComsgsooossso", //
            "osso..osgsmmoommsgso..osso", //
            ".oo...osgssmmmmssgso...oo.", //
            "......ossgssssssgsso......", //
            "......osGGssssssGGso......", //
            ".....osGoosGssGsooGso.....", //
            ".....osoGGossssoGGoso.....", //
            "......o.oGGossoGGo.o......", //
            "........osGGooGGso........", //
            "........oGGo..oGGo........", //
            ".........oGo..oGo.........", //
            "..........o....o..........", //
            "..........................", //
        ],
    ],
    frame_seconds: 0.4,
};

/// A glob of acid.
pub const SHOT_ACID: SpriteDef = SpriteDef {
    name: "enemy_shot_acid",
    legend: &[('o', MOSS), ('l', LIME), ('w', CREAM)],
    frames: &[&[
        ".oo.", //
        "olwo", //
        "ollo", //
        ".oo.", //
    ]],
    frame_seconds: 0.0,
};

/// A fireball (flicker).
pub const SHOT_FIREBALL: SpriteDef = SpriteDef {
    name: "enemy_shot_fireball",
    legend: &[('o', RED), ('a', ORANGE), ('g', AMBER), ('c', CREAM)],
    frames: &[
        &[
            ".oo..", //
            "oagao", //
            "agcga", //
            "oagao", //
            ".oo..", //
        ],
        &[
            "..oo.", //
            "oagao", //
            "agcga", //
            "oagao", //
            "..oo.", //
        ],
    ],
    frame_seconds: 0.08,
};

/// An energy bolt.
pub const SHOT_BOLT: SpriteDef = SpriteDef {
    name: "enemy_shot_bolt",
    legend: &[('o', TEAL), ('m', MINT), ('w', ICE)],
    frames: &[&[
        ".oo.", //
        "omwm", //
        "mwmo", //
        ".oo.", //
    ]],
    frame_seconds: 0.0,
};
