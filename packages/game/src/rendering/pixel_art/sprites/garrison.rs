//! Descent VII part B's sprites (Wave 10 ancients-b): the Sleeping Garrison's machines, the
//! Seal Gate's creatures, the Gatekeeper Colossus's torso, head, fists and legs (each part
//! drawn where its hit area is), their shots and the build kit's defences. Generated from
//! ASCII art; asymmetric creatures face right (the renderer mirrors them).

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A war sentry: an olive-armoured walker with a red visor slit and a rifle arm, facing right
/// (stride A/B, aiming with its visor and muzzle bright).
pub const WAR_SENTRY: SpriteDef = SpriteDef {
    name: "enemy_war_sentry",
    legend: &[
        ('o', INK),
        ('d', PINE),
        ('m', MOSS),
        ('l', LEAF),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('r', RED),
        ('p', PINK),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "....oooooo......", //
            "...ommllmmo.....", //
            "..ommllllmmo....", //
            "..omoooooomo....", //
            "..omorrrrpmo....", //
            "..omoooooomo....", //
            "...ommmmmmo.....", //
            "..ossmmmmssooo..", //
            ".osdmmlgmmdsttwo", //
            ".osdmmmmmmdsooo.", //
            ".osddmmmmdds....", //
            "..ossddddsso....", //
            "...osmmmmso.....", //
            "...odmooomdo....", //
            "..odmo..odmo....", //
            "..odmo...odmo...", //
            ".osso....osso...", //
            ".oooo....oooo...", //
        ],
        &[
            "....oooooo......", //
            "...ommllmmo.....", //
            "..ommllllmmo....", //
            "..omoooooomo....", //
            "..omorrrrpmo....", //
            "..omoooooomo....", //
            "...ommmmmmo.....", //
            "..ossmmmmssooo..", //
            ".osdmmlgmmdsttwo", //
            ".osdmmmmmmdsooo.", //
            ".osddmmmmdds....", //
            "..ossddddsso....", //
            "...osmmmmso.....", //
            "....odmoodmo....", //
            "....odmoodmo....", //
            "...odmo.odmo....", //
            "..osso..osso....", //
            "..oooo..oooo....", //
        ],
        &[
            "....oooooo......", //
            "...ommllmmo.....", //
            "..ommllllmmo....", //
            "..omoooooomo....", //
            "..omppppppmo....", //
            "..omoooooomo...g", //
            "...ommmmmmo...gp", //
            "..ossmmmmssoooop", //
            ".osdmmlgmmdsttwp", //
            ".osdmmmmmmdsoooo", //
            ".osddmmmmdds....", //
            "..ossddddsso....", //
            "...osmmmmso.....", //
            "...odmooomdo....", //
            "..odmo..odmo....", //
            "..odmo...odmo...", //
            ".osso....osso...", //
            ".oooo....oooo...", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A siege walker: a heavy four-legged olive war machine with a mortar on its back and a red
/// eye, facing right (stride A/B, crouched with the mortar glowing before a shell).
pub const SIEGE_WALKER: SpriteDef = SpriteDef {
    name: "enemy_siege_walker",
    legend: &[
        ('o', INK),
        ('d', PINE),
        ('m', MOSS),
        ('l', LEAF),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('r', RED),
        ('a', AMBER),
        ('g', GOLD),
        ('c', CREAM),
    ],
    frames: &[
        &[
            "..........oooooooo............", //
            ".........osssssstoo...........", //
            "........ossttttttsso..oooo....", //
            "........ostwwwwttsso.osstto...", //
            ".......oooooooooooooossstwo...", //
            ".....oommmmllllmmmmmmssooo....", //
            "....ommmllllllllmmmmmmmo......", //
            "...ommmmmmmmmmmmmmmmmmmmo.....", //
            "...omdmmmrrmmmmmmmmmmmdmmo....", //
            "...omddmmrrmmmgmmmmmmddmmo....", //
            "...ommdddddddddddddddddmo.....", //
            "....oommmmmmmmmmmmmmmmoo......", //
            ".....ooosssooooooossso........", //
            "......osdso.......osdso.......", //
            ".....osddso......osddso.......", //
            "....osdmmdo.....osdmmdo.......", //
            "...osdmo.oso...osdmo.oso......", //
            "..osdmo...oso.osdmo...oso.....", //
            ".osdmo.....ososdmo.....oso....", //
            "osdmo.......osdmo.......oso...", //
            "ossso......ossso........ossso.", //
            "ooooo......ooooo........ooooo.", //
        ],
        &[
            "..........oooooooo............", //
            ".........osssssstoo...........", //
            "........ossttttttsso..oooo....", //
            "........ostwwwwttsso.osstto...", //
            ".......oooooooooooooossstwo...", //
            ".....oommmmllllmmmmmmssooo....", //
            "....ommmllllllllmmmmmmmo......", //
            "...ommmmmmmmmmmmmmmmmmmmo.....", //
            "...omdmmmrrmmmmmmmmmmmdmmo....", //
            "...omddmmrrmmmgmmmmmmddmmo....", //
            "...ommdddddddddddddddddmo.....", //
            "....oommmmmmmmmmmmmmmmoo......", //
            ".....ooosssooooooossso........", //
            "......osdso.......osdso.......", //
            "......osddso......osddso......", //
            ".....osdmmdo......osdmmdo.....", //
            ".....osdmo.oso.....osdmo.oso..", //
            "....osdmo...oso....osdmo..oso.", //
            "....osdmo....oso..osdmo....oso", //
            "...osdmo......oso.osdmo....oso", //
            "..ossso......ossso.ossso.ossso", //
            "..ooooo......ooooo.ooooo.ooooo", //
        ],
        &[
            "..............................", //
            "..............................", //
            "..........oooooooo..ooooo.....", //
            ".........osssssstoooosstao....", //
            "........ossttttttssoossttgo...", //
            "........ostwwwwttssoostwwco...", //
            ".......oooooooooooooooooooo...", //
            ".....oommmmllllmmmmmmssooo....", //
            "....ommmllllllllmmmmmmmo......", //
            "...ommmmmmmmmmmmmmmmmmmmo.....", //
            "...omdmmmrrmmmmmmmmmmmdmmo....", //
            "...omddmmrrmmmgmmmmmmddmmo....", //
            "...ommdddddddddddddddddmo.....", //
            "....oommmmmmmmmmmmmmmmoo......", //
            ".....ooosssooooooossso........", //
            "......osdso.......osdso.......", //
            ".....osddsoo.....osddsoo......", //
            "....osdmmdmso...osdmmdmso.....", //
            "..oosdmoo.osoooosdmoo.osoo....", //
            ".osdmoo....osdmoo......osdo...", //
            "ossso......ossso.......ossso..", //
            "ooooo......ooooo.......ooooo..", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A lancer drone: twin rotors over an olive pod and a long steel lance, facing right (hover
/// A/B, diving with its lance white-hot).
pub const LANCER_DRONE: SpriteDef = SpriteDef {
    name: "enemy_lancer_drone",
    legend: &[
        ('o', INK),
        ('d', PINE),
        ('m', MOSS),
        ('l', LEAF),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('r', RED),
        ('p', PINK),
        ('c', CREAM),
        ('k', SKY_LIGHT),
    ],
    frames: &[
        &[
            ".oooo....oooo...", //
            "otttwo..otttwo..", //
            ".oooo.oo.oooo...", //
            "...oommmmoo.....", //
            "..ommllllmmoooo.", //
            "..omdmrpmmdtwwwo", //
            "..ommmmmmmmoooo.", //
            "...oodddddoo....", //
            ".....okkko......", //
        ],
        &[
            "..oooo..oooo....", //
            ".otttwo.otttwo..", //
            "..oooo.oo.oooo..", //
            "...oommmmoo.....", //
            "..ommllllmmoooo.", //
            "..omdmrpmmdtwwwo", //
            "..ommmmmmmmoooo.", //
            "...oodddddoo....", //
            "......oko.......", //
        ],
        &[
            ".oooo....oooo...", //
            "otttwo..otttwo..", //
            ".oooo.oo.oooo...", //
            "...oommmmooooooo", //
            "..ommllllmmcpppc", //
            "..omdmppmmdtwwwc", //
            "..ommmmmmmmooooo", //
            "...oodddddoo....", //
            ".....okkko......", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A seal warden: a hovering gilded sentinel with a teal eye inside a golden ring (drift A/B,
/// the ring flaring white before an orb).
pub const SEAL_WARDEN: SpriteDef = SpriteDef {
    name: "enemy_seal_warden",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('a', AMBER),
        ('g', GOLD),
        ('c', CREAM),
        ('t', TEAL),
        ('m', MINT),
        ('i', ICE),
    ],
    frames: &[
        &[
            ".......oooo.......", //
            ".....oogggaoo.....", //
            "....ogaaaaaago....", //
            "...ogaoooooaago...", //
            "...oaotmmmtoaro...", //
            "..ogaotmiimtoago..", //
            "..oaaotmmmmtoaao..", //
            "..ograoootoooarg..", //
            ".oo.oraaaaaarao.oo", //
            "og...orrggrro...go", //
            "oa....ooaaoo....ao", //
            ".og....orro....go.", //
            "..og....oo....go..", //
            "...ogg..oo..ggo...", //
            ".....oogggggoo....", //
            ".......ooooo......", //
        ],
        &[
            ".......oooo.......", //
            ".....oogggaoo.....", //
            "....ogaaaaaago....", //
            "...ogaoooooaago...", //
            "...oaotmmmtoaro...", //
            "..ogaotmiimtoago..", //
            "..oaaotmmmmtoaao..", //
            "..ograoootoooarg..", //
            "....oraaaaaarao...", //
            "oo...orrggrro...oo", //
            "go....ooaaoo....og", //
            "ag.....orro.....ga", //
            ".ag.....oo.....ga.", //
            "..ogg...oo...ggo..", //
            "....oogggggggoo...", //
            "......ooooooo.....", //
        ],
        &[
            ".......oooo.......", //
            ".....oogggaoo.....", //
            "....ogaaaaaago....", //
            "...ogaoooooaago...", //
            "...oaocccccoaro...", //
            "..ogaociiicoago...", //
            "..oaaocccccoaao...", //
            "..ograoootoooarg..", //
            ".oc.oraaaaaarao.co", //
            "oc...orrggrro...co", //
            "cg....ooaaoo....gc", //
            ".cg....orro....gc.", //
            "..cg....oo....gc..", //
            "...ccg..oo..gcc...", //
            ".....ccgggggcc....", //
            ".......ccccc......", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A gilded crawler: a gold-plated many-legged crawler with violet mandibles, facing right
/// (scuttle A/B, lunging with its back flaring cream).
pub const GILDED_CRAWLER: SpriteDef = SpriteDef {
    name: "enemy_gilded_crawler",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('a', AMBER),
        ('g', GOLD),
        ('c', CREAM),
        ('p', PLUM),
        ('v', VIOLET),
    ],
    frames: &[
        &[
            ".....oooooooo.....", //
            "...oogggaggaroo...", //
            "..ogacgaagaagarooo", //
            ".ogaaaaaaaaaaarvco", //
            ".orraaraaraarrppoo", //
            ".orrrrrrrrrrrrooo.", //
            "..oobbbbbbbbboo...", //
            "..ob.ob.ob.ob.....", //
            ".ob..ob..ob..ob...", //
            ".o...o...o...o....", //
        ],
        &[
            ".....oooooooo.....", //
            "...oogggaggaroo...", //
            "..ogacgaagaagarooo", //
            ".ogaaaaaaaaaaarvco", //
            ".orraaraaraarrppoo", //
            ".orrrrrrrrrrrrooo.", //
            "..oobbbbbbbbboo...", //
            "...ob.ob.ob.ob....", //
            "...ob..ob..ob.ob..", //
            "....o...o...o..o..", //
        ],
        &[
            "....oooooooo......", //
            "..oogggaggaroooo..", //
            ".ogacgaagaagarvcoo", //
            "ogcaaaaaaaaaarvcco", //
            "orraaraaraarrppooo", //
            "orrrrrrrrrrrroooo.", //
            ".oobbbbbbbbboo....", //
            ".ob.ob.ob.ob......", //
            "ob..ob..ob..ob....", //
            "o...o...o...o.....", //
        ],
    ],
    frame_seconds: 0.00,
};

/// The Gatekeeper Colossus's torso: bronze-gold war armour with a teal core in its chest
/// (on guard, roaring as the hall collapses with its core flaring, unbound with its chest
/// plates gone and the core blazing).
pub const COLOSSUS_CORE: SpriteDef = SpriteDef {
    name: "enemy_colossus_core",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('t', TEAL_DARK),
        ('e', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('p', PINK),
        ('d', RED),
        ('s', STONE_DARK),
    ],
    frames: &[
        &[
            ".......oooo..........oooo.......", //
            ".....ooggaaoooooooooogaagoo.....", //
            "....oggaaaaccooooooccaaaaggo....", //
            "...ogaaaaaacccaaaacccaaaaaago...", //
            "..ogaaaaaaccrraaaarrccaaaaaago..", //
            "..oaaaaccrrrooooooorrrccaaaao...", //
            ".ogaaacrrroogaaaagoorrrcaaago...", //
            ".oaacrrrrogaeeeeaagorrrrcaao....", //
            ".oacrrbbrgaeemmmeaagrbbrrcao....", //
            ".ocrrbbbogaemmiimeaagobbbrco....", //
            ".ocrbbbbogaemiiimeaagobbbbco....", //
            ".ocrbbbbogaemmiimeaagobbbbco....", //
            ".ocrrbbbrgaeemmmeaagrbbbrrco....", //
            "..ocrrbbrrgaeeeeaagrrbbrrco.....", //
            "..ocrrrbbrrggaaggrrbbrrrco......", //
            "...ocrrrbbrrrooorrrbbrrrco......", //
            "...ocaarrrbbbtttbbbrrraaco......", //
            "...ocaaaarrrbtetbrrraaaaco......", //
            "...ocgaaaaarrtetrraaaaagco......", //
            "....ocgaaaaaarttraaaaagco.......", //
            "....ocggaaaaaaaaaaaaaggco.......", //
            ".....occggaaaaaaaaaggcco........", //
            ".....orcccggggggggggcccro.......", //
            "......orrcccccccccccccro........", //
            ".......orrrrrrrrrrrrrro.........", //
            "........obbbbbbbbbbbbo..........", //
            ".........oooooooooooo...........", //
            "................................", //
        ],
        &[
            ".......oooo..........oooo.......", //
            ".....ooggaaoooooooooogaagoo.....", //
            "....oggaaaaccooooooccaaaaggo....", //
            "...ogaaaaaacccaaaacccaaaaaago...", //
            "..ogaaaaaaccrraaaarrccaaaaaago..", //
            "..oaaaaccrrrooooooorrrccaaaao...", //
            ".ogaaacrrroogaaaagoorrrcaaago...", //
            ".oaacrrrrogappppaagorrrrcaao....", //
            ".oacrrbbrgappwwwpaagrbbrrcao....", //
            ".ocrrbbbogapwwwwwpaagobbbrco....", //
            ".ocrbbbbogapwwwwwpaagobbbbco....", //
            ".ocrbbbbogapwwwwwpaagobbbbco....", //
            ".ocrrbbbrgappwwwpaagrbbbrrco....", //
            "..ocrrbbrrgappppaagrrbbrrco.....", //
            "..ocrrrbbrrggaaggrrbbrrrco......", //
            "...ocrrrbbrrrooorrrbbrrrco......", //
            "...ocaarrrbbbdddbbbrrraaco......", //
            "...ocaaaarrrbdpdbrrraaaaco......", //
            "...ocgaaaaarrdpdrraaaaagco......", //
            "....ocgaaaaaarddraaaaagco.......", //
            "....ocggaaaaaaaaaaaaaggco.......", //
            ".....occggaaaaaaaaaggcco........", //
            ".....orcccggggggggggcccro.......", //
            "......orrcccccccccccccro........", //
            ".......orrrrrrrrrrrrrro.........", //
            "........obbbbbbbbbbbbo..........", //
            ".........oooooooooooo...........", //
            "................................", //
        ],
        &[
            ".......oooo..........oooo.......", //
            ".....oossaaoo......oogaassoo....", //
            "....osssaacco..o...occaaasso....", //
            "...ossaaacco..mm....occaaasso...", //
            "..osaaaacco..mmmm....occaaaaso..", //
            "..oaaaacco..mmiimm...ooccaaao...", //
            ".osaaacco..mmiiiimm....occaao...", //
            ".oaaccoo..mmiiwwiimm....occao...", //
            ".oacco...mmiiwwwwiimm....ocao...", //
            ".ocro...mmiiwwwwwwiimm...orco...", //
            ".ocro..emmiiwwwwwwiimme..orco...", //
            ".ocro...mmiiwwwwwwiimm...orco...", //
            ".ocro....mmiiwwwwiimm....orco...", //
            "..ocro....mmiiwwiimm....orco....", //
            "..ocrro....mmiiiimm....orrco....", //
            "...ocrro....mmiimm....orrco.....", //
            "...ocaaro....mmmm....oraaco.....", //
            "...ocaaaro....mm....oraaaco.....", //
            "...ocgaaaro........oraaagco.....", //
            "....ocgaaaroo....ooraaagco......", //
            "....ocggaaaaroooooraaaggco......", //
            ".....occggaaaaaaaaaaggcco.......", //
            ".....orcccggggggggggcccro.......", //
            "......orrcccccccccccccro........", //
            ".......orrrrrrrrrrrrrro.........", //
            "........obbbbbbbbbbbbo..........", //
            ".........oooooooooooo...........", //
            "................................", //
        ],
    ],
    frame_seconds: 0.00,
};

/// The Colossus's crested helm with its single eye (calm teal, glaring red-white as it
/// strikes).
pub const COLOSSUS_HEAD: SpriteDef = SpriteDef {
    name: "enemy_colossus_head",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('t', TEAL_DARK),
        ('e', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('p', PINK),
        ('d', RED),
        ('s', STONE_DARK),
    ],
    frames: &[
        &[
            "........oooooo........", //
            "......oogggggaoo......", //
            ".....ogggaaaaaago.....", //
            "....ogaaaaaaaaaago....", //
            "...ogaaccccccccaago...", //
            "...oaacoooooooocaao...", //
            "..ogacotttttttttocago.", //
            "..oaacotemmmmmetocaao.", //
            "..oaacotemiiimmetcaao.", //
            "..oaacotemmmmmetocaao.", //
            "..oaacotttttttttocaao.", //
            "..ogaacoooooooocaago..", //
            "...ogaaccccccccaago...", //
            "...ocaaaarrrraaaaco...", //
            "....ocaarrbbrraaco....", //
            "....ocaarbbbbraaco....", //
            ".....ocrrbbbbrrco.....", //
            "......ocrrrrrrco......", //
            ".......ooooooooo......", //
            "......................", //
        ],
        &[
            "........oooooo........", //
            "......oogggggaoo......", //
            ".....ogggaaaaaago.....", //
            "....ogaaaaaaaaaago....", //
            "...ogaaccccccccaago...", //
            "...oaacoooooooocaao...", //
            "..ogacodddddddddocago.", //
            "..oaacoddpppppddocaao.", //
            "..oaacoddpwwwppddcaao.", //
            "..oaacoddpppppddocaao.", //
            "..oaacodddddddddocaao.", //
            "..ogaacoooooooocaago..", //
            "...ogaaccccccccaago...", //
            "...ocaaaarrrraaaaco...", //
            "....ocaarrbbrraaco....", //
            "....ocaarbbbbraaco....", //
            ".....ocrrbbbbrrco.....", //
            "......ocrrrrrrco......", //
            ".......ooooooooo......", //
            "......................", //
        ],
    ],
    frame_seconds: 0.00,
};

/// One of the Colossus's gauntleted fists, knuckle plates and a teal seam (clenched, glowing
/// red-hot over its mark before it strikes).
pub const COLOSSUS_FIST: SpriteDef = SpriteDef {
    name: "enemy_colossus_fist",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('t', TEAL_DARK),
        ('e', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('p', PINK),
        ('d', RED),
        ('s', STONE_DARK),
    ],
    frames: &[
        &[
            "......oooooooo......", //
            "....oogaggaggaoo....", //
            "...ogaacgaacgaacgo..", //
            "..ogaacoaacoaacoago.", //
            "..oaaccoaccoaccoaao.", //
            ".ogacrrorrrorrrocago", //
            ".oacrrbbrbbrrbbrcao.", //
            ".oacrrrrrrrrrrrrcao.", //
            ".ogaccrrrrrrrrrccago", //
            "..oaaacccccccccaaao.", //
            "..ogaaaaaaaaaaaaago.", //
            "..oaatttttttttttaao.", //
            "..oaatemmmmmmmetaao.", //
            "..oaatttttttttttaao.", //
            "..ogaaaaaaaaaaaaago.", //
            "...ogaaaaaaaaaaago..", //
            "....ocgaaaaaaagco...", //
            ".....occcccccccco...", //
            "......oooooooooo....", //
            "....................", //
        ],
        &[
            "......oooooooo......", //
            "....oogaggaggaoo....", //
            "...ogaacgaacgaacgo..", //
            "..ogaacoaacoaacoago.", //
            "..oaaccoaccoaccoaao.", //
            ".ogacrrorrrorrrocago", //
            ".oacrrbbrbbrrbbrcao.", //
            ".oacrrrrrrrrrrrrcao.", //
            ".ogaccrrrrrrrrrccago", //
            "..oaaacccccccccaaao.", //
            "..ogaaaaaaaaaaaaago.", //
            "..oaadddddddddddaao.", //
            "..oaadpwwwwwwwpdaao.", //
            "..oaadddddddddddaao.", //
            "..ogaaaaaaaaaaaaago.", //
            "...ogaaaaaaaaaaago..", //
            "....ocgaaaaaaagco...", //
            ".....occcccccccco...", //
            "......oooooooooo....", //
            "....................", //
        ],
    ],
    frame_seconds: 0.00,
};

/// The Colossus's hips and legs: bronze greaves and great flat feet with a teal seam at the
/// hips.
pub const COLOSSUS_LEGS: SpriteDef = SpriteDef {
    name: "enemy_colossus_legs",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('g', GOLD),
        ('w', CREAM),
        ('t', TEAL_DARK),
        ('e', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('p', PINK),
        ('d', RED),
        ('s', STONE_DARK),
    ],
    frames: &[&[
        "....oooooooooooooooooooooo....", //
        "...ogaaaaaaccrrrrccaaaaaago...", //
        "..ogaaaaccrrbbbbbbrrccaaaago..", //
        "..oaaaccrrbbttttttbbrrccaaao..", //
        "..oacccrrbbtemmmmetbbrrrccao..", //
        "..ocrrrrrbbttttttttbbrrrrrco..", //
        "...oorrrrbbbbbbbbbbbbrrrroo...", //
        ".....ooooooo......ooooooo.....", //
        "....ogaaaacco....occaaaago....", //
        "....oaaaaccro....orccaaaao....", //
        "....oaacrrrro....orrrrcaao....", //
        "....oacrrbbro....orbbrrcao....", //
        "....ocrrbbbro....orbbbrrco....", //
        "...ogaaaaaaago..ogaaaaaaago...", //
        "...oaaccccaaao..oaaaccccaao...", //
        "...ogacttttcao..oactttttago...", //
        "....oaaccccaao..oaacccccao....", //
        "....ocrrrrrrco..ocrrrrrrco....", //
        "....ocrrbbrrco..ocrrbbrrco....", //
        "....ocrrbbrrco..ocrrbbrrco....", //
        "....ocrrbbrrco..ocrrbbrrco....", //
        "...ocrrrrrrrrco.ocrrrrrrrrco..", //
        "..ogaaaaaaaaaaoogaaaaaaaaaaago", //
        "..ogaaaaaaaaaaaoogaaaaaaaaaago", //
        "..occcccccccccco.occccccccccco", //
        "..obbbbbbbbbbbbo.obbbbbbbbbbbo", //
        "..oooooooooooooo.ooooooooooooo", //
        "..............................", //
    ]],
    frame_seconds: 0.00,
};

/// A war sentry's red bolt.
pub const SHOT_SENTRY_BOLT: SpriteDef = SpriteDef {
    name: "shot_sentry_bolt",
    legend: &[('o', INK), ('r', RED), ('p', PINK), ('w', CREAM)],
    frames: &[
        &[
            "ooooooo", //
            "oprrwwo", //
            "ooooooo", //
        ],
        &[
            "ooooooo", //
            "orrpwwo", //
            "ooooooo", //
        ],
    ],
    frame_seconds: 0.06,
};

/// A siege walker's lobbed shell, its fuse sparking.
pub const SHOT_SIEGE_SHELL: SpriteDef = SpriteDef {
    name: "shot_siege_shell",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('a', AMBER),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "..og...", //
            ".ooaoo.", //
            "osttwso", //
            "osttsso", //
            "ossssso", //
            ".osssso", //
            "..ooo..", //
        ],
        &[
            "...ga..", //
            ".ooaoo.", //
            "osttwso", //
            "osttsso", //
            "ossssso", //
            ".osssso", //
            "..ooo..", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A seal warden's slow golden orb.
pub const SHOT_WARDEN_ORB: SpriteDef = SpriteDef {
    name: "shot_warden_orb",
    legend: &[('o', INK), ('a', AMBER), ('g', GOLD), ('w', CREAM)],
    frames: &[
        &[
            "..ooo..", //
            ".ogago.", //
            "oggwggo", //
            "oawwwao", //
            "oggwggo", //
            ".ogago.", //
            "..ooo..", //
        ],
        &[
            "..ooo..", //
            ".oagao.", //
            "oaggwao", //
            "ogwwwgo", //
            "oawggao", //
            ".oagao.", //
            "..ooo..", //
        ],
    ],
    frame_seconds: 0.10,
};

/// The Colossus's teal eye bolt.
pub const SHOT_COLOSSUS_BOLT: SpriteDef = SpriteDef {
    name: "shot_colossus_bolt",
    legend: &[
        ('o', INK),
        ('t', TEAL),
        ('m', MINT),
        ('i', ICE),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "..oooo..", //
            ".ottmmo.", //
            "otmiimto", //
            "omiwwimo", //
            "omiwwimo", //
            "otmiimto", //
            ".ottmmo.", //
            "..oooo..", //
        ],
        &[
            "..oooo..", //
            ".ommtto.", //
            "omiimmto", //
            "oiwwimmo", //
            "omiwwiio", //
            "otmmiimo", //
            ".ottmmo.", //
            "..oooo..", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A stone falling from the ceiling.
pub const SHOT_FALLING_STONE: SpriteDef = SpriteDef {
    name: "shot_falling_stone",
    legend: &[
        ('o', INK),
        ('b', SOIL_DARK),
        ('r', RUST),
        ('s', STONE),
        ('l', STONE_LIGHT),
        ('g', GOLD),
    ],
    frames: &[&[
        "..ooooo..", //
        ".osllsro.", //
        "oslgssrro", //
        "osssssrbo", //
        "orssrrrbo", //
        "orrrrrbbo", //
        ".obbbbbo.", //
        "..ooooo..", //
    ]],
    frame_seconds: 0.00,
};

/// The build kit's auto-turret: an olive gun head on a steel tripod with a sky-blue sensor,
/// facing right (idle, firing with a muzzle flash). Two tiles tall.
pub const DEFENCE_TURRET: SpriteDef = SpriteDef {
    name: "defence_turret",
    legend: &[
        ('o', INK),
        ('d', PINE),
        ('m', MOSS),
        ('l', LEAF),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('k', SKY_LIGHT),
        ('g', GOLD),
        ('c', CREAM),
        ('a', AMBER),
    ],
    frames: &[
        &[
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            ".....oooooo.....", //
            "....ommllmmo....", //
            "...ommllllmmo...", //
            "...omookkoomoooo", //
            "...omokkkkomsttt", //
            "...omookkoomoooo", //
            "...ommmmmmmmo...", //
            "....oddddddo....", //
            ".....osstso.....", //
            "......osso......", //
            "......osso......", //
            ".....ossso......", //
            "....osgaso......", //
            "....oooooo......", //
            "....osttso......", //
            "...ost..tso.....", //
            "...ot....to.....", //
            "..ost....tso....", //
            "..ot......to....", //
            ".ost......tso...", //
            ".oo........oo...", //
            "oo..........oo..", //
        ],
        &[
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            "................", //
            ".....oooooo.....", //
            "....ommllmmo....", //
            "...ommllllmmo...", //
            "...omookkoomoooc", //
            "...omokkkkomsttc", //
            "...omookkoomoooc", //
            "...ommmmmmmmo...", //
            "....oddddddo....", //
            ".....osstso.....", //
            "......osso......", //
            "......osso......", //
            ".....ossso......", //
            "....osgaso......", //
            "....oooooo......", //
            "....osttso......", //
            "...ost..tso.....", //
            "...ot....to.....", //
            "..ost....tso....", //
            "..ot......to....", //
            ".ost......tso...", //
            ".oo........oo...", //
            "oo..........oo..", //
        ],
    ],
    frame_seconds: 0.00,
};

/// The build kit's shock trap: a steel floor plate with coils (armed: arcing blue; spent:
/// dark while it re-arms).
pub const DEFENCE_TRAP: SpriteDef = SpriteDef {
    name: "defence_trap",
    legend: &[
        ('o', INK),
        ('s', STEEL_DARK),
        ('t', STEEL),
        ('w', STEEL_LIGHT),
        ('k', SKY),
        ('i', ICE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "..i....k....i...", //
            ".oooooooooooooo.", //
            "oskktskkktskkkso", //
            "ogtwtgtwtgtwtgto", //
            "oooooooooooooooo", //
        ],
        &[
            "................", //
            ".oooooooooooooo.", //
            "osssssssssssssso", //
            "ogtwtgtwtgtwtgto", //
            "oooooooooooooooo", //
        ],
    ],
    frame_seconds: 0.00,
};
