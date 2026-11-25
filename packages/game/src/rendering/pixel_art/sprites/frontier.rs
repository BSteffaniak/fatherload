//! The frontier worlds' creatures (Wave 12 planets-b): one animated sprite per kind, drawn
//! facing right (the renderer mirrors them). Names carry the track's `pb_` prefix.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A dune skipper: a sand-coloured hopper with long ears, a cream belly and a tufted tail
/// (sitting / mid-hop).
pub const DUNE_SKIPPER: SpriteDef = SpriteDef {
    name: "pb_enemy_dune_skipper",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', CLAY),
        ('d', LOAM),
        ('c', CREAM),
        ('w', BONE),
    ],
    frames: &[
        &[
            "........oo..", //
            ".......omo..", //
            "......omlo..", //
            ".....olllo..", //
            "....ollollo.", //
            "o...olllllco", //
            "do.olllllco.", //
            ".dddllllco..", //
            "...oddddo...", //
            "...oo.oo....", //
        ],
        &[
            ".........oo.", //
            "........omo.", //
            ".......omlo.", //
            "......olllo.", //
            ".....ollollo", //
            "o...ollllllc", //
            ".dooolllllco", //
            "..ddddlllco.", //
            "...ooddddo..", //
            ".oo.....o...", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A dune wyrm: a huge ringed sand-swimmer, tail to the left, its round head to the right; its
/// maw closed in plates, then gaping with rings of bone teeth over a red throat.
pub const DUNE_WYRM: SpriteDef = SpriteDef {
    name: "pb_enemy_dune_wyrm",
    legend: &[
        ('o', INK),
        ('l', SAND),
        ('m', CLAY),
        ('d', LOAM),
        ('s', SOIL),
        ('c', CREAM),
        ('w', BONE),
        ('r', RED),
        ('x', MAROON),
    ],
    frames: &[
        &[
            "....................oooo....", //
            "................ooooommmoo..", //
            "..........oooooommmmlllllmo.", //
            "....oooooommmmdmmmlllllllllo", //
            "..oommmmmmdmmmmdmmllmmmmlllo", //
            ".omlllmmmmdlllmdmllmddddmllo", //
            "omlllllmmmdlllmdmllmdoodmllo", //
            "omdmmmmmmmdmmmmdmllmddddmllo", //
            ".omdddddddddddddmllmmmmllmo.", //
            "..oooooooosdddddmmlllllllmo.", //
            "..........ooooooodmlllllmo..", //
            "..................odmmmmmo..", //
            "...................ooooo....", //
            "............................", //
        ],
        &[
            "....................oooo....", //
            "................ooooommmoo..", //
            "..........oooooommmmcccccmo.", //
            "....oooooommmmdmmmcwwwwwwcmo", //
            "..oommmmmmdmmmmdmcwxxxxxxwco", //
            ".omlllmmmmdlllmdmcwxrrrrxwco", //
            "omlllllmmmdlllmdmcwxrrrrxwco", //
            "omdmmmmmmmdmmmmdmcwxrrrrxwco", //
            ".omdddddddddddddmcwxxxxxxwco", //
            "..oooooooosdddddmmcwwwwwwcmo", //
            "..........oooooooodmcccccmo.", //
            "..................odmmmmmo..", //
            "...................ooooo....", //
            "............................", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A glass scorpion: a body of green glass, pincers forward, its tail curled over (walk A / B,
/// the tail raised to sting).
pub const GLASS_SCORPION: SpriteDef = SpriteDef {
    name: "pb_enemy_glass_scorpion",
    legend: &[
        ('o', INK),
        ('t', TEAL_DARK),
        ('g', TEAL),
        ('n', MINT),
        ('i', ICE),
        ('k', INK),
        ('r', RED),
    ],
    frames: &[
        &[
            "..oooo..........", //
            ".ogniooo........", //
            ".ogo..ogo.......", //
            "..o....ogo......", //
            "........ogoooo..", //
            "..ooooooggnngoo.", //
            ".ogggnnggggggono", //
            "ognigggggrggggo.", //
            ".oo.o.o.o.ooooo.", //
            ".o..o.o..o..o...", //
        ],
        &[
            "..oooo..........", //
            ".ogniooo........", //
            ".ogo..ogo.......", //
            "..o....ogo......", //
            "........ogoooo..", //
            "..ooooooggnngoo.", //
            ".ogggnnggggggono", //
            "ognigggggrggggo.", //
            ".oo.o.o.o.ooooo.", //
            "..o..o.o..o..o..", //
        ],
        &[
            "......ooo.......", //
            ".....onio.......", //
            ".....ogo........", //
            ".....ogo........", //
            "......ogoooooo..", //
            "..ooooooggnngoo.", //
            ".ogggnnggggggono", //
            "ognigggggrggggo.", //
            ".oo.o.o.o.ooooo.", //
            ".o..o.o..o..o...", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A mirage moth: pale shimmering wings with a sky-blue eye spot (wings up / down).
pub const MIRAGE_MOTH: SpriteDef = SpriteDef {
    name: "pb_enemy_mirage_moth",
    legend: &[
        ('o', INK),
        ('w', BONE),
        ('s', SKY_LIGHT),
        ('c', CREAM),
        ('l', SAND),
    ],
    frames: &[
        &[
            "oo......oo", //
            "owoo..oowo", //
            "oswco.ocwo", //
            ".owwcocwo.", //
            "..oolllo..", //
            "....olo...", //
            ".....o....", //
            "..........", //
        ],
        &[
            "..........", //
            "..........", //
            "...o..o...", //
            "ooowcocwoo", //
            "oswwlllwso", //
            "oowwoloowo", //
            "....olo...", //
            ".....o....", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A magma slug: a crust of black rock over a glowing orange body, eye stalks up (ooze A / B).
pub const MAGMA_SLUG: SpriteDef = SpriteDef {
    name: "pb_enemy_magma_slug",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('x', MAROON),
        ('r', RED),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "............o.o.", //
            "............o.o.", //
            ".....oooooooraro", //
            "...oonnnxnnnrago", //
            "..onnxnnnxnnaago", //
            ".onxrrarrrxrraao", //
            "oaarraggarrraaao", //
            "oaaggaaaagaaaaao", //
            ".ooooooooooooooo", //
        ],
        &[
            "...........o.o..", //
            "...........o.o..", //
            "......ooooooraro", //
            "...ooonnxnnnrago", //
            "..onnnxnnnxnaago", //
            ".onrxrrarrrxraao", //
            "oarraggarrraaaao", //
            "oaagaaaaggaaaaao", //
            ".ooooooooooooooo", //
        ],
    ],
    frame_seconds: 0.30,
};

/// A cinder bat: soot-black wings edged with embers, glowing eyes (wings up / down).
pub const CINDER_BAT: SpriteDef = SpriteDef {
    name: "pb_enemy_cinder_bat",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('s', SLATE_DARK),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "oa.......ao", //
            "osao...oaso", //
            "onssoooossn", //
            ".onsngnsno.", //
            "..osnnnso..", //
            "...ooaoo...", //
            "...........", //
        ],
        &[
            "...........", //
            "....ooo....", //
            "...ongno...", //
            "oaonnnnnoao", //
            "osssnnnsssn", //
            "onoaosoaono", //
            "a.........a", //
        ],
    ],
    frame_seconds: 0.08,
};

/// An ember strider: a basalt-backed grazer on four long stilt legs, a glowing belly (stride A
/// / B).
pub const EMBER_STRIDER: SpriteDef = SpriteDef {
    name: "pb_enemy_ember_strider",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('s', STONE_DARK),
        ('t', STONE),
        ('a', ORANGE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "...........oo.", //
            "..oooooooooto.", //
            ".ostsstsstsso.", //
            "osstsstsstssgo", //
            "onnaaaaaannoo.", //
            ".oooooooooo...", //
            "..o.o...o.o...", //
            "..o.o...o.o...", //
            ".o..o...o..o..", //
            ".o...o.o...o..", //
            "o....o.o....o.", //
            "o....o.o....o.", //
        ],
        &[
            "...........oo.", //
            "..oooooooooto.", //
            ".ostsstsstsso.", //
            "osstsstsstssgo", //
            "onnaaaaaannoo.", //
            ".oooooooooo...", //
            "...oo....oo...", //
            "...oo....oo...", //
            "..o..o..o..o..", //
            "..o..o..o..o..", //
            ".o....oo....o.", //
            ".o....oo....o.", //
        ],
    ],
    frame_seconds: 0.22,
};

/// A basalt tortoise: a shell of hexagonal basalt plates with ember seams (walk A / B, shelled
/// up).
pub const BASALT_TORTOISE: SpriteDef = SpriteDef {
    name: "pb_enemy_basalt_tortoise",
    legend: &[
        ('o', INK),
        ('s', STONE_DARK),
        ('t', STONE),
        ('l', STONE_LIGHT),
        ('a', ORANGE),
        ('d', LOAM),
    ],
    frames: &[
        &[
            ".....oooooooo......", //
            "...ootlttltttoo....", //
            "..otlttatttattto...", //
            ".otttatttattttato..", //
            ".ostsssstsssstsso..", //
            "ossssassssassssso..", //
            "ooooooooooooooooddo", //
            "..od..od....od.odoo", //
            "..od..od....od.oddo", //
            "..oo..oo....oo..oo.", //
            "...................", //
        ],
        &[
            ".....oooooooo......", //
            "...ootlttltttoo....", //
            "..otlttatttattto...", //
            ".otttatttattttato..", //
            ".ostsssstsssstsso..", //
            "ossssassssassssso..", //
            "ooooooooooooooooddo", //
            "...od..od..od..odoo", //
            "...od..od..od..oddo", //
            "...oo..oo..oo...oo.", //
            "...................", //
        ],
        &[
            "...................", //
            ".....oooooooo......", //
            "...ootlttltttoo....", //
            "..otlttatttattto...", //
            ".otttatttattttato..", //
            ".ostsssstsssstsso..", //
            "ossssassssassssso..", //
            "ooooooooooooooooo..", //
            "...................", //
            "...................", //
            "...................", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A shard crab: a slate shell bristling with amethyst shards, crystal claws (scuttle A / B,
/// claws raised).
pub const SHARD_CRAB: SpriteDef = SpriteDef {
    name: "pb_enemy_shard_crab",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('w', BONE),
        ('k', INK),
        ('s', SLATE),
    ],
    frames: &[
        &[
            "....o....o......", //
            "...olo..olo.....", //
            "..ooooooooooo...", //
            ".oswssvssvsso...", //
            "olvsslssslssvoo.", //
            "ovvsssssssssvvlo", //
            ".oossssssssooolo", //
            "..o.o.o.o.o..oo.", //
            ".o..o.o.o..o....", //
            "o...o...o...o...", //
        ],
        &[
            "....o....o......", //
            "...olo..olo.....", //
            "..ooooooooooo...", //
            ".oswssvssvsso...", //
            "olvsslssslssvoo.", //
            "ovvsssssssssvvlo", //
            ".oossssssssooolo", //
            "...o.o.o.o.o.oo.", //
            "..o..o.o.o..o...", //
            ".o...o...o...o..", //
        ],
        &[
            "....o....o...ol.", //
            "...olo..olo.olv.", //
            "..ooooooooooovo.", //
            ".oswssvssvssvo..", //
            "olvsslssslssso..", //
            "ovvsssssssssso..", //
            ".oossssssssoo...", //
            "..o.o.o.o.o.....", //
            ".o..o.o.o..o....", //
            "o...o...o...o...", //
        ],
    ],
    frame_seconds: 0.00,
};

/// A glint moth: a tiny crystal moth with faceted wings that catch the light (wings up / down).
pub const GLINT_MOTH: SpriteDef = SpriteDef {
    name: "pb_enemy_glint_moth",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('s', SKY_LIGHT),
        ('w', BONE),
        ('v', VIOLET),
    ],
    frames: &[
        &[
            "oo.....oo", //
            "oiso.osio", //
            "oswivwiso", //
            ".ooivioo.", //
            "...ovo...", //
            "....o....", //
            ".........", //
            ".........", //
        ],
        &[
            ".........", //
            ".........", //
            "...ovo...", //
            "ooovwvooo", //
            "oisiviiso", //
            "oswoooiwo", //
            ".oo...oo.", //
            ".........", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A chime drifter: a translucent crystal bell drifting with a ringing clapper and trailing
/// threads (sway A / B).
pub const CHIME_DRIFTER: SpriteDef = SpriteDef {
    name: "pb_enemy_chime_drifter",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('s', SKY_LIGHT),
        ('l', LAVENDER),
        ('v', VIOLET),
        ('w', BONE),
        ('g', GOLD),
    ],
    frames: &[
        &[
            "....oooo....", //
            "...oiwiio...", //
            "..oisssiio..", //
            ".oissllssio.", //
            ".oslvvvvlso.", //
            "oislvvvvlsio", //
            "osslvvvvlsso", //
            "oooooggoooo.", //
            "....ogwo....", //
            "....o..o....", //
            "...o....o...", //
            "...o....o...", //
            "..o......o..", //
            "..o......o..", //
        ],
        &[
            "....oooo....", //
            "...oiwiio...", //
            "..oisssiio..", //
            ".oissllssio.", //
            ".oslvvvvlso.", //
            "oislvvvvlsio", //
            "osslvvvvlsso", //
            ".oooogggoooo", //
            ".....ogwo...", //
            ".....o..o...", //
            "....o....o..", //
            "....o....o..", //
            ".....o....o.", //
            ".....o....o.", //
        ],
    ],
    frame_seconds: 0.45,
};

/// A geode mimic: a grey rock lump that splits into crystal-lined jaws (closed, open, biting).
pub const GEODE_MIMIC: SpriteDef = SpriteDef {
    name: "pb_enemy_geode_mimic",
    legend: &[
        ('o', INK),
        ('n', STONE_DARK),
        ('t', STONE),
        ('l', STONE_LIGHT),
        ('v', VIOLET),
        ('p', LAVENDER),
        ('w', BONE),
        ('r', ROSE),
    ],
    frames: &[
        &[
            "................", //
            "................", //
            "................", //
            ".....oooooo.....", //
            "...ootlttltoo...", //
            "..otlttttttlto..", //
            ".otlttntttnttto.", //
            ".ottnttttttnnto.", //
            "otttttnttttttnto", //
            "ottntttttntttnno", //
            "onttttttttnttnno", //
            ".onnttnnttttnno.", //
            "..oonnnnnnnnoo..", //
            "....oooooooo....", //
        ],
        &[
            "......oooo......", //
            "....ootlltoo....", //
            "...otltttttlo...", //
            "..otnvpvvpvnto..", //
            ".otvwvwvvwvwvto.", //
            ".ovrrrrrrrrrrvo.", //
            "..orrrrrrrrrro..", //
            ".ovrrrrrrrrrrvo.", //
            ".otvwvwvvwvwvto.", //
            "otnnvpvvvvpvnnto", //
            "ottntttttntttnno", //
            "onttttttttnttnno", //
            ".onnttnnttttnno.", //
            "..oooooooooooo..", //
        ],
        &[
            "....oooooo......", //
            "..ootltttloo....", //
            ".otlttttttlto...", //
            ".otnvpvvpvnto...", //
            "otvwvwvvwvwvto..", //
            "ovrrrrrrrrrrvoo.", //
            "orrrrrrrrrrrrrro", //
            "ovrrrrrrrrrrvoo.", //
            "otvwvwvvwvwvto..", //
            "otnnvpvvvvpvnnto", //
            "ottntttttntttnno", //
            "onttttttttnttnno", //
            ".onnttnnttttnno.", //
            "..oooooooooooo..", //
        ],
    ],
    frame_seconds: 0.00,
};
