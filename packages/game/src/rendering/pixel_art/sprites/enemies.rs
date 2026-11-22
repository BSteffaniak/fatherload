//! Enemy sprites (WP-ENEMIES): one animated sprite per `fatherload_simulation::EnemyKind`
//! and the enemy shots. Asymmetric creatures and machines face right (the renderer mirrors
//! them); symmetric ones are authored as mirrored halves. Generated grids, hand-tuned.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A cave bat: leathery lavender wings on a violet body, glowing gold eyes and a fang (wing
/// flap). Light wings so a swarm reads against dark cave walls.
pub const CAVE_BAT: SpriteDef = SpriteDef {
    name: "enemy_cave_bat",
    legend: &[
        ('o', INK),
        ('d', VIOLET),
        ('m', LAVENDER),
        ('e', GOLD),
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

// --- Sky families (Wave 4 T21) ---------------------------------------------------------------

/// A sky ray: a blue manta gliding on wide wings, pale belly, gold eyes and a whip tail (wings up, level, down).
pub const SKY_RAY: SpriteDef = SpriteDef {
    name: "enemy_sky_ray",
    legend: &[
        ('o', INK),
        ('b', SKY),
        ('l', ICE),
        ('w', BONE),
        ('e', GOLD),
        ('t', OCEAN),
    ],
    frames: &[
        &[
            "o...................o", //
            "oo.................oo", //
            "obo...............obo", //
            "obbo.............obbo", //
            ".obbo...ooooo...obbo.", //
            ".obbbo.obbbbbo.obbbo.", //
            "..obbbobbbebbbobbbo..", //
            "..obbbbbbbbbbbbbbbo..", //
            "...oolllllwllllloo...", //
            ".....oolll.llloo.....", //
            ".......oo.t.oo.......", //
            "..........t..........", //
        ],
        &[
            ".....................", //
            ".....................", //
            ".....................", //
            "........ooooo........", //
            "oo.....obbbbbo.....oo", //
            "obboo.obbbebbbo.oobbo", //
            ".obbbbbbbbbbbbbbbbbo.", //
            "..obbbbbbbbbbbbbbbo..", //
            "...oolllllwllllloo...", //
            ".....oolll.llloo.....", //
            ".......oo.t.oo.......", //
            "..........t..........", //
        ],
        &[
            ".....................", //
            ".....................", //
            ".....................", //
            "........ooooo........", //
            ".......obbbbbo.......", //
            "......obbbebbbo......", //
            "....oobbbbbbbbboo....", //
            "..oobbbbbbbbbbbbboo..", //
            ".obbbolllllllllobbbo.", //
            "obbo..oollllloo..obbo", //
            "obo....oo.t.oo....obo", //
            "oo........t........oo", //
        ],
        &[
            ".....................", //
            ".....................", //
            ".....................", //
            "........ooooo........", //
            "oo.....obbbbbo.....oo", //
            "obboo.obbbebbbo.oobbo", //
            ".obbbbbbbbbbbbbbbbbo.", //
            "..obbbbbbbbbbbbbbbo..", //
            "...oolllllwllllloo...", //
            ".....oolll.llloo.....", //
            ".......oo.t.oo.......", //
            "..........t..........", //
        ],
    ],
    frame_seconds: 0.16,
};

/// A wyvern: a rust-red sky drake with leathery green wings and a gold belly, facing right (wings up / down).
pub const WYVERN: SpriteDef = SpriteDef {
    name: "enemy_wyvern",
    legend: &[
        ('o', INK),
        ('g', MOSS),
        ('l', LEAF),
        ('r', RUST),
        ('y', AMBER),
        ('e', GOLD),
        ('w', BONE),
    ],
    frames: &[
        &[
            "....oo..........oo......", //
            "...ogo.........ogo......", //
            "..oggo........oggo......", //
            ".ogggo.......ogggo..ooo.", //
            ".oggggo.....oggggo.orrro", //
            "oggllggo...ogglggoorrere", //
            "ogglllggoooggllgorrrrrrw", //
            ".oglllggrrrrrrrrrrrrooo.", //
            "..oolggrrrrrrrrrrro.....", //
            "....orrryyyyyrrrro......", //
            "...orro.oyyyyoorro......", //
            "..orro...oooo.orro......", //
            ".oro.........oro.o......", //
            "oo...........oo.........", //
        ],
        &[
            "........................", //
            "........................", //
            "....................ooo.", //
            "...................orrro", //
            "..................orrere", //
            "........oooooo...orrrrrw", //
            "...ooooorrrrrrrrrrrrooo.", //
            ".oogggggrrrrrrrrrrro....", //
            "ogglllgrrryyyyyrrro.....", //
            "ogglllggoryyyyoorro.....", //
            ".ogglggo..oooo.orro.....", //
            "..ogggo.......oro.o.....", //
            "...ogo........oo........", //
            "....o...................", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A storm elemental: a knot of thundercloud around a crackling gold core, sparks falling (flicker).
pub const STORM_ELEMENTAL: SpriteDef = SpriteDef {
    name: "enemy_storm_elemental",
    legend: &[
        ('o', NIGHT),
        ('f', FOG),
        ('m', SLATE_LIGHT),
        ('s', SLATE),
        ('y', GOLD),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "....ooooooooo....", //
            "..ooffffmffffoo..", //
            ".offfmmmmmmmfffo.", //
            "offmmmmmmmmmmmffo", //
            "ofmmssmmmmmssmmfo", //
            "ofmssssyyyssssmfo", //
            "omsssyywwwyysssmo", //
            "omssyywwywwyyssmo", //
            ".omsssyyyyysssmo.", //
            "..oomsssssssmoo..", //
            "....ooo.s.ooo....", //
            "......y...y......", //
            ".....y.....y.....", //
            "......y...y......", //
        ],
        &[
            "....ooooooooo....", //
            "..oommffmffmmoo..", //
            ".ofmmmfffffmmmfo.", //
            "ofmmmmmmmmmmmmmfo", //
            "ofmmssmmmmmssmmfo", //
            "ommssyysssyyssmmo", //
            "omssyywwywwyyssmo", //
            "omsssyywwwyysssmo", //
            ".omsssssysssssmo.", //
            "..oomsssssssmoo..", //
            "....ooo...ooo....", //
            ".....y..s..y.....", //
            "......y...y......", //
            ".....y.....y.....", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A sky pirate airship: a patched crimson gas bag over a plank gondola with gun ports and a skull flag; the stern prop spins.
pub const PIRATE_AIRSHIP: SpriteDef = SpriteDef {
    name: "enemy_pirate_airship",
    legend: &[
        ('o', INK),
        ('c', MAROON),
        ('C', RED),
        ('p', AMBER),
        ('s', ROSE),
        ('r', SOIL),
        ('w', BONE),
        ('d', LOAM),
        ('y', GOLD),
        ('k', SOIL_DARK),
    ],
    frames: &[
        &[
            "..............oooooooooooooo..............", //
            "..........oooocccccccccccccccoooo.........", //
            ".......ooocccCCCCCCCCCCCCCCCCcccccoo......", //
            ".....oocccCCCCCCCCppppCCCCCCCCCCccccoo....", //
            "....occcCCCCCCCCCCpppppCCCCCCCCCCCcccco...", //
            "...occCCCCCCCCCCCCCppCCCCCCCCCCCCCCCccco..", //
            "..occCCCCssssssssssssssssssssssCCCCCCcco..", //
            "..occCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCccco.", //
            "..occcCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCcco..", //
            "...occcccCCCCCCCCCCCCCCCCCCCCCCCCCCccco...", //
            "....oocccccccccccccccccccccccccccccco.....", //
            "......ooocccccccccccccccccccccccoooo......", //
            ".........oooooooooooooooooooooooo.........", //
            "..........r.....r.........r.....r.........", //
            "..........r.....r.........r.....r..owo....", //
            "..........r.....r.........r.....r.owkwo...", //
            "....oooooooooooooooooooooooooooooowwwwo...", //
            "...owwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwowo....", //
            "..owddyddddyddddyddddyddddyddddyddo.o.....", //
            "..oddddddddddddddddddddddddddddddddo......", //
            "o.oddkkddddkkddddkkddddkkddddkkdddo.......", //
            "ooodddddddddddddddddddddddddddddddo.......", //
            "o..oddddddddddddddddddddddddddddo.........", //
            "....oooooooooooooooooooooooooooo..........", //
        ],
        &[
            "..............oooooooooooooo..............", //
            "..........oooocccccccccccccccoooo.........", //
            ".......ooocccCCCCCCCCCCCCCCCCcccccoo......", //
            ".....oocccCCCCCCCCppppCCCCCCCCCCccccoo....", //
            "....occcCCCCCCCCCCpppppCCCCCCCCCCCcccco...", //
            "...occCCCCCCCCCCCCCppCCCCCCCCCCCCCCCccco..", //
            "..occCCCCssssssssssssssssssssssCCCCCCcco..", //
            "..occCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCccco.", //
            "..occcCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCcco..", //
            "...occcccCCCCCCCCCCCCCCCCCCCCCCCCCCccco...", //
            "....oocccccccccccccccccccccccccccccco.....", //
            "......ooocccccccccccccccccccccccoooo......", //
            ".........oooooooooooooooooooooooo.........", //
            "..........r.....r.........r.....r.........", //
            "..........r.....r.........r.....r..owo....", //
            "..........r.....r.........r.....r.owkwo...", //
            "....oooooooooooooooooooooooooooooowwwwo...", //
            "...owwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwowo....", //
            "..owddyddddyddddyddddyddddyddddyddo.o.....", //
            "o.oddddddddddddddddddddddddddddddo........", //
            "..oddkkddddkkddddkkddddkkddddkkdddo.......", //
            "oooddddddddddddddddddddddddddddddo........", //
            "...oddddddddddddddddddddddddddddo.........", //
            "....oooooooooooooooooooooooooooo..........", //
        ],
    ],
    frame_seconds: 0.1,
};

/// An airship's brass swivel cannon, barrel to the right (idle / charge glow).
pub const AIRSHIP_TURRET: SpriteDef = SpriteDef {
    name: "enemy_airship_turret",
    legend: &[
        ('o', INK),
        ('y', AMBER),
        ('a', GOLD),
        ('d', STEEL_DARK),
        ('k', NIGHT),
        ('G', CREAM),
    ],
    frames: &[
        &[
            "....oooo....", //
            "...oyyyyo...", //
            "..oyyaayoooo", //
            "..oyaaaaddko", //
            "..oyyaayoooo", //
            "...oyyyyo...", //
            "....oddo....", //
            "...oddddo...", //
            "...oooooo...", //
        ],
        &[
            "....oooo....", //
            "...oyyyyo...", //
            "..oyyaayoooo", //
            "..oyaGGaddko", //
            "..oyyaayoooo", //
            "...oyyyyo...", //
            "....oddo....", //
            "...oddddo...", //
            "...oooooo...", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A sky pirate boarder: red bandana, brass goggles, a green coat, a cutlass and a jetpack sputtering flame, facing right (flame flicker).
pub const SKY_PIRATE: SpriteDef = SpriteDef {
    name: "enemy_sky_pirate",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('s', SAND),
        ('w', BONE),
        ('g', MOSS),
        ('k', STEEL_DARK),
        ('d', SOIL),
        ('y', ORANGE),
        ('Y', GOLD),
    ],
    frames: &[
        &[
            "....oooo....", //
            "...orrrro...", //
            "..orrrrrro..", //
            "..ossswwso..", //
            "..osssssso..", //
            "...ossso....", //
            "..oggggggo..", //
            ".kogggggg.oo", //
            ".kogggggoowo", //
            ".kogggggo.ow", //
            "..oddddo...w", //
            "..oddoddo...", //
            "..odo.odo...", //
            "..oo...oo...", //
            "..y.....y...", //
        ],
        &[
            "....oooo....", //
            "...orrrro...", //
            "..orrrrrro..", //
            "..ossswwso..", //
            "..osssssso..", //
            "...ossso....", //
            "..oggggggo..", //
            ".kogggggg.oo", //
            ".kogggggoowo", //
            ".kogggggo.ow", //
            "..oddddo...w", //
            "..oddoddo...", //
            "..odo.odo...", //
            "..oo...oo...", //
            "..Y.....Y...", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A harpy nester: tawny wings, a pale feathered breast, a sharp face and gold talons (wings up / down).
pub const HARPY_NESTER: SpriteDef = SpriteDef {
    name: "enemy_harpy_nester",
    legend: &[
        ('o', INK),
        ('b', COPPER),
        ('f', BONE),
        ('l', CREAM),
        ('s', SAND),
        ('e', RED),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "o.................o", //
            "oo...............oo", //
            "obo.............obo", //
            "obbo...ooooo...obbo", //
            ".obbo.ossfsso.obbo.", //
            ".obbboosefesoobbbo.", //
            "..obbbossfssobbbo..", //
            "..obbbbbofobbbbbo..", //
            "...oobbbbfbbbboo...", //
            ".....obbblbbbo.....", //
            "......oblllbo......", //
            "......oyoooyo......", //
            ".....oy.y.y.yo.....", //
            ".....o...o...o.....", //
        ],
        &[
            "...................", //
            "...................", //
            "...................", //
            ".......ooooo.......", //
            "......ossfsso......", //
            ".....oosefesoo.....", //
            "oooooobssfssboooooo", //
            "obbbbbbbofobbbbbbbo", //
            ".obbbbbbbfbbbbbbbo.", //
            "..oobbbbblbbbbboo..", //
            "....oobblllbboo....", //
            "......oyoooyo......", //
            ".....oy.y.y.yo.....", //
            ".....o...o...o.....", //
        ],
    ],
    frame_seconds: 0.14,
};

/// An iron cannonball with a glint.
pub const SHOT_CANNONBALL: SpriteDef = SpriteDef {
    name: "enemy_shot_cannonball",
    legend: &[('o', INK), ('d', SLATE_DARK), ('s', SLATE), ('w', FOG)],
    frames: &[&[
        ".oooo.", //
        "odwsdo", //
        "odssdo", //
        "oddddo", //
        "odddo.", //
        ".ooo..", //
    ]],
    frame_seconds: 0.0,
};

// --- Night monsters (WP-NIGHT) ----------------------------------------------------------

/// A shadow hound: a smoke-dark hound with a pale rim, violet ears and glowing mint eyes, facing
/// right (trot A/B, pounce).
pub const SHADOW_HOUND: SpriteDef = SpriteDef {
    name: "enemy_shadow_hound",
    legend: &[
        ('o', INK),
        ('d', SLATE),
        ('s', FOG),
        ('m', SLATE_DARK),
        ('e', MINT),
        ('t', BONE),
        ('k', VIOLET),
    ],
    frames: &[
        &[
            ".................", //
            "..........oo.oo..", //
            ".........osdosdo.", //
            "o.......osdddddo.", //
            "do.....osddddedo.", //
            ".do.oooddddddddtt", //
            "..oddddddddddkoo.", //
            "..osssddddddddo..", //
            "..odddddddddddo..", //
            "...odmmmmmmmdo...", //
            "...odo.odo.odo...", //
            "...od..od..od....", //
            "...oo..oo..oo....", //
        ],
        &[
            ".................", //
            "..........oo.oo..", //
            ".........osdosdo.", //
            "o.......osdddddo.", //
            "do.....osddddedo.", //
            ".do.oooddddddddtt", //
            "..oddddddddddkoo.", //
            "..osssddddddddo..", //
            "..odddddddddddo..", //
            "...odmmmmmmmdo...", //
            "..odo.odo..odo...", //
            ".od...od....od...", //
            ".oo...oo....oo...", //
        ],
        &[
            ".........oo.oo...", //
            "........osdosdo..", //
            "o......osdddddo..", //
            "do....osddddedoo.", //
            ".dooooddddddddttt", //
            ".oddddddddddddkoo", //
            ".ossssddddddddo..", //
            "..odddddddddddo..", //
            "..odmmmmmmmmdo...", //
            ".odo........odo..", //
            "od............od.", //
            "oo............oo.", //
            ".................", //
        ],
    ],
    frame_seconds: 0.16,
};

/// A dune stalker breaking out of the sand: a sand-hided burrower with red eyes and glassy
/// fangs, facing right (jaws shut / open).
pub const DUNE_STALKER: SpriteDef = SpriteDef {
    name: "enemy_dune_stalker",
    legend: &[
        ('o', INK),
        ('d', LOAM),
        ('s', SAND),
        ('m', SOIL),
        ('e', RED),
        ('t', BONE),
    ],
    frames: &[
        &[
            ".....oooooo.......", //
            "...ooddddddoo.....", //
            "..odsssdddddddoo..", //
            ".odssdddddddddddo.", //
            "odddddddeeoddddtto", //
            "odmmddddoooddddtto", //
            ".odmmmdddddddddoo.", //
            "..oommmmmmmddoo...", //
            "...o.oo.oo.oo.....", //
        ],
        &[
            ".....oooooo.......", //
            "...ooddddddoo.....", //
            "..odsssdddddddoo..", //
            ".odssdddddddddddoo", //
            "odddddddeeoddddt.t", //
            "odmmddddoooddddtt.", //
            ".odmmmdddddddddttt", //
            "..oommmmmmmddooo..", //
            "...oo.oo.oo.o.....", //
        ],
    ],
    frame_seconds: 0.2,
};

/// A will-o'-wisp: a teardrop of cold mint light with a bone-white heart and two dark eyes
/// (flicker).
pub const WILL_O_WISP: SpriteDef = SpriteDef {
    name: "enemy_will_o_wisp",
    legend: &[('o', TEAL), ('c', MINT), ('w', BONE), ('k', TEAL_DARK)],
    frames: &[
        &[
            "....oo....", //
            "...occo...", //
            "..occcco..", //
            ".occwwcco.", //
            ".ocwwwwco.", //
            ".ocwkwkco.", //
            ".ocwwwwco.", //
            "..ocwwco..", //
            "..occcco..", //
            "...occo...", //
            "....oo....", //
            ".....o....", //
        ],
        &[
            "....oo....", //
            "...occo...", //
            "..occcco..", //
            ".occwwcco.", //
            ".ocwwwwco.", //
            ".ocwkwkco.", //
            ".ocwwwwco.", //
            "..ocwwco..", //
            "...occo...", //
            "...oo.oo..", //
            "...o...o..", //
            "..........", //
        ],
    ],
    frame_seconds: 0.14,
};

/// One moth of a moth swarm: dusty pale wings on a dark body (flap). The swarm is drawn as a
/// whirl of them.
pub const NIGHT_MOTH: SpriteDef = SpriteDef {
    name: "enemy_night_moth",
    legend: &[('o', INK), ('w', MIST), ('d', SOIL)],
    frames: &[
        &[
            "o.....o", //
            "oo.o.oo", //
            "owoooww", //
            "owwdwwo", //
            ".oodoo.", //
            "..ooo..", //
        ],
        &[
            ".......", //
            ".......", //
            "oooooo.", //
            "owwdwwo", //
            "oowdwoo", //
            "..ooo..", //
        ],
    ],
    frame_seconds: 0.06,
};

/// The umbral alpha: a huge night-black hound with a violet mane rim and red eyes, facing right
/// (stalk A/B).
pub const UMBRAL_ALPHA: SpriteDef = SpriteDef {
    name: "enemy_umbral_alpha",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('s', LAVENDER),
        ('m', PLUM),
        ('e', RED),
        ('t', BONE),
        ('k', LAVENDER),
    ],
    frames: &[
        &[
            "...................oo.oo..", //
            "..................osdosdo.", //
            ".................osdddddoo", //
            "o...............osddddeedo", //
            "do.............osdddddddtt", //
            ".do...ooooooooodddddddddtt", //
            "..oooddddddddddddddddddkoo", //
            "...osssssdddddddddddddddo.", //
            "...odddddddddddddddddddo..", //
            "...oddddddddddddddddddo...", //
            "....odmmmmmmmmmmmmmmdo....", //
            "....oddo..oddo..oddo.ddo..", //
            "....odo...odo...odo...odo.", //
            "....odo...odo...odo...odo.", //
            "....ooo...ooo...ooo...ooo.", //
        ],
        &[
            "...................oo.oo..", //
            "..................osdosdo.", //
            ".................osdddddoo", //
            "o...............osddddeedo", //
            "do.............osdddddddtt", //
            ".do...ooooooooodddddddddtt", //
            "..oooddddddddddddddddddkoo", //
            "...osssssdddddddddddddddo.", //
            "...odddddddddddddddddddo..", //
            "...oddddddddddddddddddo...", //
            "....odmmmmmmmmmmmmmmdo....", //
            "...oddo...oddo..oddo..ddo.", //
            "..odo....odo.....odo...odo", //
            "..odo....odo.....odo...odo", //
            "..ooo....ooo.....ooo...ooo", //
        ],
    ],
    frame_seconds: 0.2,
};

/// A wispfire bolt: a cold mint flame.
pub const SHOT_WISPFIRE: SpriteDef = SpriteDef {
    name: "enemy_shot_wispfire",
    legend: &[('o', TEAL), ('c', MINT), ('w', BONE)],
    frames: &[
        &[
            ".oo.", //
            "occo", //
            "ocwo", //
            ".oo.", //
        ],
        &[
            ".oo.", //
            "owco", //
            "occo", //
            ".oo.", //
        ],
    ],
    frame_seconds: 0.08,
};

// --- The Crust's creatures (Wave 8) --------------------------------------------------------

/// A burrow mole: a velvet-dark tunneller with a pink star nose and big digging claws, facing
/// right (jaws shut, then open for the bite).
pub const BURROW_MOLE: SpriteDef = SpriteDef {
    name: "enemy_burrow_mole",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('m', SOIL),
        ('l', LOAM),
        ('p', PINK),
        ('r', ROSE),
        ('w', BONE),
        ('e', CREAM),
    ],
    frames: &[
        &[
            "......oooooo......", //
            "....oommmmmmoo....", //
            "...ommlllllmmmo...", //
            "..omllllllllmmmo..", //
            ".omlllllllllmmmoo.", //
            ".omllllllllemmmrpo", //
            "odmmllllllmmmmorpp", //
            "odmmmmmmmmmmmmoppo", //
            ".oddmmmmmmmmddoo..", //
            "..opopo..opopo....", //
        ],
        &[
            "......oooooo......", //
            "....oommmmmmoo....", //
            "...ommlllllmmmo...", //
            "..omllllllllmmmoo.", //
            ".omlllllllllmmmorp", //
            ".omllllllllemmowwp", //
            "odmmllllllmmmo...o", //
            "odmmmmmmmmmmmowwpp", //
            ".oddmmmmmmmmddoopp", //
            "..opopo...opopop..", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A mud leech: a fat, glistening ringed slug the colour of wet mud, a sucker mouth at its
/// front, facing right (it humps along: two frames).
pub const MUD_LEECH: SpriteDef = SpriteDef {
    name: "enemy_mud_leech",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('m', LOAM),
        ('l', CLAY),
        ('h', SAND),
        ('r', MAROON),
        ('p', ROSE),
    ],
    frames: &[
        &[
            "................", //
            "......oooooo....", //
            "....oommmmmmoo..", //
            "...omlhhlmlmmmo.", //
            "..omllmllmlmmrpo", //
            ".omdmmdmmdmmdrpo", //
            ".oddddddddddddo.", //
            "..oooooooooooo..", //
        ],
        &[
            "......oooo......", //
            "....oommmmoo....", //
            "...omlhhlmmmo...", //
            "..omllmllmlmmoo.", //
            ".omlmmdmmdmmmrpo", //
            ".omdddmddmdddrpo", //
            "..odddddddddddo.", //
            "...oooooooooooo.", //
        ],
    ],
    frame_seconds: 0.3,
};

/// A potter wasp: a clay-red striped abdomen, a black waist and glassy wings that blur
/// (wings up, wings down).
pub const POTTER_WASP: SpriteDef = SpriteDef {
    name: "enemy_potter_wasp",
    legend: &[
        ('o', INK),
        ('c', CLAY),
        ('a', AMBER),
        ('s', SKY_LIGHT),
        ('i', ICE),
        ('w', BONE),
    ],
    frames: &[
        &[
            "...oo..oo...", //
            "..osio.oiso.", //
            "..oisioisio.", //
            "...ooioioo..", //
            ".ooaco.oocwo", //
            "ocacacoooaoo", //
            ".ooacaoooo..", //
            "...ooo..o.o.", //
        ],
        &[
            "............", //
            "............", //
            "..oooo.oooo.", //
            ".osiiiooiiso", //
            ".ooaco.oocwo", //
            "ocacacoooaoo", //
            ".ooacaoooo..", //
            "...ooo..o.o.", //
        ],
    ],
    frame_seconds: 0.05,
};

/// A mine bat: a dusty grey colony bat with a lamp-pale belly, big ears and a squashed snout
/// (wings spread, folded, and down).
pub const MINE_BAT: SpriteDef = SpriteDef {
    name: "enemy_mine_bat",
    legend: &[
        ('o', INK),
        ('d', STONE_DARK),
        ('m', STONE),
        ('l', STONE_LIGHT),
        ('e', AMBER),
        ('w', BONE),
    ],
    frames: &[
        &[
            "oo.o......o.oo", //
            "omoo.o..o.oomo", //
            "ommmodooodommo", //
            "ommmdelleddmmo", //
            ".omdlllllldmo.", //
            "..oodlwwldoo..", //
            "....oddddo....", //
            ".....o..o.....", //
        ],
        &[
            "..............", //
            "....o....o....", //
            "...odooooodo..", //
            "..odelleldo...", //
            ".omdlllllldmo.", //
            "ommodlwwldommo", //
            "omo.oddddo.omo", //
            "oo...o..o...oo", //
        ],
        &[
            "....o....o....", //
            "...odooooodo..", //
            "..odelleldo...", //
            "..omlllllldo..", //
            ".ommdlwwldmmo.", //
            "ommo.oddo.ommo", //
            "omo........omo", //
            "oo..........oo", //
        ],
    ],
    frame_seconds: 0.09,
};

/// A timber beetle: a glossy chestnut shell with a gold seam, a horned head and strong
/// mandibles, facing right (walk A/B, then jaws wide for the bite).
pub const TIMBER_BEETLE: SpriteDef = SpriteDef {
    name: "enemy_timber_beetle",
    legend: &[
        ('o', INK),
        ('r', RUST),
        ('c', COPPER),
        ('g', GOLD),
        ('d', SOIL_DARK),
        ('k', MAROON),
        ('w', BONE),
    ],
    frames: &[
        &[
            "....oooooooo......", //
            "..oocccccgccoo....", //
            ".occcgccccgcccoo..", //
            ".ocrrrrrrgrrrrkoo.", //
            "ocrrrrrrrgrrrrkdoo", //
            "okkkkkkkkgkkkkdddw", //
            ".oodddddddddddoowo", //
            "..o.o.o.o.o.o.....", //
            "..o..o..o..o......", //
        ],
        &[
            "....oooooooo......", //
            "..oocccccgccoo....", //
            ".occcgccccgcccoo..", //
            ".ocrrrrrrgrrrrkoo.", //
            "ocrrrrrrrgrrrrkdoo", //
            "okkkkkkkkgkkkkdddw", //
            ".oodddddddddddoowo", //
            "...o.o.o.o.o.o....", //
            "....o..o..o..o....", //
        ],
        &[
            "....oooooooo......", //
            "..oocccccgccoo.ow.", //
            ".occcgccccgcccoodo", //
            ".ocrrrrrrgrrrrkdoo", //
            "ocrrrrrrrgrrrrkdd.", //
            "okkkkkkkkgkkkkdoow", //
            ".oodddddddddddo.wo", //
            "..o.o.o.o.o.o.....", //
            "..o..o..o..o......", //
        ],
    ],
    frame_seconds: 0.2,
};

/// A slate skink: a long, low lizard banded slate-blue and pale, a whip of a tail and bright
/// gold eyes, facing right (running A/B, then rearing to lunge).
pub const SLATE_SKINK: SpriteDef = SpriteDef {
    name: "enemy_slate_skink",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('m', SLATE),
        ('l', SLATE_LIGHT),
        ('s', SKY_LIGHT),
        ('e', GOLD),
        ('w', BONE),
    ],
    frames: &[
        &[
            "......................", //
            "...........ooooooooo..", //
            "oo......oooslmslmsleo.", //
            "ommoooooddmlmmlmmlmmwo", //
            ".oddmmmdddddddddddddo.", //
            "..ooooooooddodoodo.o..", //
            "..........o.o..o.o....", //
        ],
        &[
            "......................", //
            "...........ooooooooo..", //
            ".oo.....oooslmslmsleo.", //
            "..ommooooddmlmmlmmlmwo", //
            "...ooddmddddddddddddo.", //
            ".....oooooddodoodo.o..", //
            "...........o.o..o.o...", //
        ],
        &[
            "...................oo.", //
            "..............ooooeso.", //
            "............oolmslmwo.", //
            "oo.......oooslmmlmoo..", //
            "ommoooooddmlmmddoo....", //
            ".oddmmmdddddddoo......", //
            "..oooooooooddo.o......", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A shelf crab: a squat crab carrying a slab of slate for a shell, one claw bigger than the
/// other, eyes on stalks (scuttle A/B, then the big claw raised).
pub const SHELF_CRAB: SpriteDef = SpriteDef {
    name: "enemy_shelf_crab",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('m', SLATE),
        ('l', SLATE_LIGHT),
        ('s', SKY_LIGHT),
        ('r', RUST),
        ('c', COPPER),
        ('e', CREAM),
    ],
    frames: &[
        &[
            "......oooooooooo........", //
            "....oollslmmmmllmoo.....", //
            "...ollmmmmlmmmmmmmdo.o.o", //
            "...odddddddddddddddoeoeo", //
            "..orcccccrrrrrccccroroo.", //
            ".orccrrrrrrrrrrrrrrrcco.", //
            "orrooorrrrrrrrrrrroocrro", //
            ".o..o.o..o..o..o.o..ooo.", //
            "...o.o..o..o..o..o......", //
        ],
        &[
            "......oooooooooo........", //
            "....oollslmmmmllmoo.....", //
            "...ollmmmmlmmmmmmmdo.o.o", //
            "...odddddddddddddddoeoeo", //
            "..orcccccrrrrrccccroroo.", //
            ".orccrrrrrrrrrrrrrrrcco.", //
            "orrooorrrrrrrrrrrroocrro", //
            ".o.o..o..o..o..o..o.ooo.", //
            "..o..o..o..o..o..o......", //
        ],
        &[
            "......oooooooooo....ooo.", //
            "....oollslmmmmllmoooccro", //
            "...ollmmmmlmmmmmmmdorrro", //
            "...odddddddddddddddoeoo.", //
            "..orcccccrrrrrccccrorr..", //
            ".orccrrrrrrrrrrrrrrrro..", //
            "orrooorrrrrrrrrrrrooo...", //
            ".o..o.o..o..o..o.o......", //
            "...o.o..o..o..o..o......", //
        ],
    ],
    frame_seconds: 0.22,
};

/// A tube crawler: a many-legged basalt-black crawler with glowing seams of lava between its
/// plates, facing right (crawl A/B, then its throat glows to spit).
pub const TUBE_CRAWLER: SpriteDef = SpriteDef {
    name: "enemy_tube_crawler",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('d', STONE_DARK),
        ('m', MAROON),
        ('r', RED),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "......oooooooooo......", //
            "....oondndndndndoo....", //
            "...ondadndadndadndoo..", //
            "..onddmddmddmddmddddo.", //
            ".ondmmrmmrmmrmmrmmdgo.", //
            ".onnnnnnnnnnnnnnnnnoo.", //
            "..o.o.o.o.o.o.o.o.o...", //
            ".o.o.o.o.o.o.o.o.o....", //
        ],
        &[
            "......oooooooooo......", //
            "....oondndndndndoo....", //
            "...ondadndadndadndoo..", //
            "..onddmddmddmddmddddo.", //
            ".ondmmrmmrmmrmmrmmdgo.", //
            ".onnnnnnnnnnnnnnnnnoo.", //
            "...o.o.o.o.o.o.o.o.o..", //
            "..o.o.o.o.o.o.o.o.o...", //
        ],
        &[
            "......oooooooooo......", //
            "....oondndndndndoo....", //
            "...ondadndadndadndooo.", //
            "..onddmddmddmddmddaago", //
            ".ondmmrmmrmmrmmrmmagao", //
            ".onnnnnnnnnnnnnnnnoaao", //
            "..o.o.o.o.o.o.o.o.o...", //
            ".o.o.o.o.o.o.o.o.o....", //
        ],
    ],
    frame_seconds: 0.15,
};

/// A cinder moth: broad charcoal wings edged with glowing embers, a furry amber body and
/// feathered feelers (wings up, out and down).
pub const CINDER_MOTH: SpriteDef = SpriteDef {
    name: "enemy_cinder_moth",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('d', STONE_DARK),
        ('r', RED),
        ('a', ORANGE),
        ('g', GOLD),
        ('c', CREAM),
    ],
    frames: &[
        &[
            ".o.........o.", //
            "..o.......o..", //
            "aoo.......ooa", //
            "ardo.ooo.odra", //
            "anddogcgoddna", //
            ".anddgagddna.", //
            "..onodadonoo.", //
            "....oo.oo....", //
        ],
        &[
            ".o.........o.", //
            "..o.......o..", //
            "...o.ooo.o...", //
            "aaoodgcgdooaa", //
            "arddnoagoddra", //
            ".anddgagddna.", //
            "..aanodonaa..", //
            "....oo.oo....", //
        ],
        &[
            ".o.........o.", //
            "..o.......o..", //
            "...o.ooo.o...", //
            "....ogcgo....", //
            "..oodgagdoo..", //
            ".onddoaodddn.", //
            "anddno.ondnna", //
            "arra.....arra", //
        ],
    ],
    frame_seconds: 0.07,
};
