//! The Hollow World's creatures (Wave 9, Descent V): one animated sprite per kind,
//! sized so their contact boxes match. Asymmetric creatures face right (the renderer
//! mirrors them). Generated from a padded ASCII source, so every row of a sprite has
//! the same width.
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;
/// A sky manta: a great pale-violet ray of the Inner Sky with cream belly spots, a long
/// whip tail and glowing gill slits, facing right (wings up, wings level, wings down).
pub const SKY_MANTA: SpriteDef = SpriteDef {
    name: "enemy_sky_manta",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('m', VIOLET),
        ('l', LAVENDER),
        ('s', CREAM),
        ('w', BONE),
        ('g', MINT),
        ('t', NIGHT),
    ],
    frames: &[
        &[
            "..................................", //
            "...........ooooo..................", //
            ".........oollllloo................", //
            ".......oollllllllloo..............", //
            "......olllllllllllllo.........oo..", //
            "t....olllsllllllsllllo......oolo..", //
            ".tt.ollllllllllllllllloooooollwo..", //
            "...tommmmmmmlllllllmmmmmmmmmmgmo..", //
            "....ooddddmmmmmmmmmmmmddddddmmmo..", //
            "......oooodddddddddddddoooooooo...", //
            "..........oooooooooooo............", //
        ],
        &[
            "..................................", //
            "..................................", //
            "..................................", //
            "..........oooooooo................", //
            ".......oollllllllloo..........oo..", //
            "t...oollllsllllllslllooo....oolo..", //
            ".ttolllllllllllllllllllloooollwo..", //
            "...tommmmmmmmllllllmmmmmmmmmmgmo..", //
            "...oddddddmmmmmmmmmmmmmdddddmmmo..", //
            "....oooooodddddddddddddooooooooo..", //
            "..........oooooooooooo............", //
        ],
        &[
            "..................................", //
            "..................................", //
            "..................................", //
            "..............................oo..", //
            "t..........oooooooooooo.....oolo..", //
            ".tt....oooollllllllllllooooollwo..", //
            "...toolllllllsllllllsllllmmmmgmo..", //
            "....ommmmmmmmmmmmmmmmmmmmdddmmmo..", //
            ".....oddddddmmmmmmmmmmddddoooooo..", //
            ".......oooooddddddddddoooo........", //
            "...........ooooooooooo............", //
        ],
    ],
    frame_seconds: 0.32,
};

/// A stoop roc: a russet raptor of the floating stones with a gold beak and a cream
/// breast, facing right (gliding, a wingbeat, stooping with its wings folded).
pub const STOOP_ROC: SpriteDef = SpriteDef {
    name: "enemy_stoop_roc",
    legend: &[
        ('o', INK),
        ('r', RUST),
        ('c', COPPER),
        ('a', AMBER),
        ('s', CREAM),
        ('y', GOLD),
        ('e', BONE),
        ('d', SOIL_DARK),
    ],
    frames: &[
        &[
            "...................", //
            "...................", //
            "..ooo.........ooo..", //
            ".orrroo.....oorco..", //
            "..orrrroooooorcco..", //
            "...ooccrrrrrcccoooo", //
            ".....occsssccceoyyo", //
            "....odccssssccoooo.", //
            "...odr.oossoo......", //
            "...oo...oddo.......", //
            ".........oo........", //
        ],
        &[
            "...................", //
            ".......ooo.........", //
            "......orrro........", //
            ".....orrcco........", //
            ".....orccco.ooo....", //
            "...ooccrrrcccooooo.", //
            "..odccsssssccceoyyo", //
            ".odr.occssscccoooo.", //
            ".oo...ooosoo.......", //
            "........oddo.......", //
            ".........oo........", //
        ],
        &[
            "..........ooo......", //
            ".........orrco.....", //
            "........orrcco.....", //
            ".......orrccco.....", //
            "......occsssco.....", //
            "......ocsssso......", //
            ".......ocsso.......", //
            ".......oceeo.......", //
            "........oyyo.......", //
            ".........yo........", //
            ".........o.........", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A gust bladder: a translucent sky-blue gas sac with pale veins and dangling feelers,
/// swelling and easing as it drifts.
pub const GUST_BLADDER: SpriteDef = SpriteDef {
    name: "enemy_gust_bladder",
    legend: &[
        ('o', INK),
        ('b', OCEAN),
        ('l', SKY),
        ('w', SKY_LIGHT),
        ('i', ICE),
        ('v', LAVENDER),
    ],
    frames: &[
        &[
            "....oooooo....", //
            "..oolllllloo..", //
            ".olwwllllllbo.", //
            "olwiilllvlllbo", //
            "olwilllvllllbo", //
            "ollllllvllllbo", //
            "olllllvlllllbo", //
            ".ollllvllllbo.", //
            "..oobbbbbboo..", //
            "....o.o.o.....", //
            "....v..v..v...", //
            "...v..v....v..", //
        ],
        &[
            "..............", //
            "...oooooooo...", //
            ".oolllllllloo.", //
            "olwwlllllllllo", //
            "lwiilllvlllllb", //
            "lwillllvlllllb", //
            "olllllvllllllo", //
            ".ollllvlllllo.", //
            "..oobbbbbboo..", //
            "....o.o.o.....", //
            ".....v.v..v...", //
            "....v...v..v..", //
        ],
    ],
    frame_seconds: 0.45,
};

/// A crestback: a gentle olive grazer of the jungle with a coral crest along its back, a
/// cream belly and a saddle-wide back the pilot rides, facing right (walk A/B, asleep).
pub const CRESTBACK: SpriteDef = SpriteDef {
    name: "enemy_crestback",
    legend: &[
        ('o', INK),
        ('d', PINE),
        ('m', MOSS),
        ('l', LEAF),
        ('s', CREAM),
        ('c', ROSE),
        ('p', PINK),
        ('e', BONE),
        ('k', SOIL_DARK),
    ],
    frames: &[
        &[
            ".........o.o.o.o.o..........", //
            "........ocpcpcpcpco.........", //
            "......ooccccccccccooo.......", //
            "....oolllllllllllllloo..ooo.", //
            "..oollllllllllllllllllooloo.", //
            ".olmllllllllllllllllllllleo.", //
            "olmmmllllllllllllllllllllllo", //
            "odmmmmmmsssssssssssmmmmmmko.", //
            ".oddmmmssssssssssssssmmooo..", //
            "..ooddmmmmmmmmmmmmmmmddo....", //
            "...odmo.odmo...odmo.odmo....", //
            "...odo...odo...odo...odo....", //
            "...oko...oko...oko...oko....", //
        ],
        &[
            ".........o.o.o.o.o..........", //
            "........ocpcpcpcpco.........", //
            "......ooccccccccccooo.......", //
            "....oolllllllllllllloo..ooo.", //
            "..oollllllllllllllllllooloo.", //
            ".olmllllllllllllllllllllleo.", //
            "olmmmllllllllllllllllllllllo", //
            "odmmmmmmsssssssssssmmmmmmko.", //
            ".oddmmmssssssssssssssmmooo..", //
            "..ooddmmmmmmmmmmmmmmmddo....", //
            "....odmoodmo.....odmoodmo...", //
            "....odo.odo.......odoodo....", //
            "....oko.oko.......okooko....", //
        ],
        &[
            "............................", //
            "............................", //
            "............................", //
            "............................", //
            ".........o.o.o.o.o..........", //
            "........ocpcpcpcpco.........", //
            "......ooccccccccccooo.......", //
            "....oolllllllllllllloooooo..", //
            "..oollllllllllllllllllllkko.", //
            ".olmmllllllllllllllllllllleo", //
            "odmmmmmmmssssssssssmmmmmmmko", //
            ".oddddmmmmmmmmmmmmmmmmddooo.", //
            "..oooooooooooooooooooooo....", //
        ],
    ],
    frame_seconds: 0.28,
};

/// A skyneck: a towering longneck of the jungle halls, slate-green with a pale belly and
/// ochre stripes, its small head high on a long neck, facing right (walk A/B).
pub const SKYNECK: SpriteDef = SpriteDef {
    name: "enemy_skyneck",
    legend: &[
        ('o', INK),
        ('d', SLATE_DARK),
        ('m', SLATE),
        ('l', TEAL),
        ('s', MIST),
        ('y', AMBER),
        ('e', BONE),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "............................oooo....", //
            "...........................ollllo...", //
            "..........................ollleolo..", //
            "..........................olllllloo.", //
            "...........................oolllo...", //
            "............................olllo...", //
            "............................ollmo...", //
            "...........................olllmo...", //
            "...........................ollmo....", //
            "..........................olllmo....", //
            "..........................ollmo.....", //
            ".........................olllmo.....", //
            ".........................ollmmo.....", //
            "........................olllmo......", //
            "........................ollmmo......", //
            ".......................olllmo.......", //
            ".......................ollmmo.......", //
            "......................olllmmo.......", //
            ".........oooooooooooooollmmo........", //
            ".......oollyllllyllllylllmmo........", //
            ".....oollllyllllyllllyllllmo........", //
            "....ollllllyllllyllllylllllo........", //
            "...ollllllllllllllllllllllmo........", //
            "..ollllllllllllllllllllllmmo........", //
            "..olmmmllllllllllllllllmmmo.........", //
            ".oolmmmmssssssssssssmmmmmmo.........", //
            "olmmo.mmmsssssssssmmmmo.oo..........", //
            "olo...ommmmmmmmmmmmmmo..............", //
            "oo....odmmo.odmo.odmmo.odmo.........", //
            "......odmo..odmo..odmo..odmo........", //
            "......odmo..odmo..odmo..odmo........", //
            "......odmo..odmo..odmo..odmo........", //
            "......okko..okko..okko..okko........", //
        ],
        &[
            "............................oooo....", //
            "...........................ollllo...", //
            "..........................ollleolo..", //
            "..........................olllllloo.", //
            "...........................oolllo...", //
            "............................olllo...", //
            "............................ollmo...", //
            "...........................olllmo...", //
            "...........................ollmo....", //
            "..........................olllmo....", //
            "..........................ollmo.....", //
            ".........................olllmo.....", //
            ".........................ollmmo.....", //
            "........................olllmo......", //
            "........................ollmmo......", //
            ".......................olllmo.......", //
            ".......................ollmmo.......", //
            "......................olllmmo.......", //
            ".........oooooooooooooollmmo........", //
            ".......oollyllllyllllylllmmo........", //
            ".....oollllyllllyllllyllllmo........", //
            "....ollllllyllllyllllylllllo........", //
            "...ollllllllllllllllllllllmo........", //
            "..ollllllllllllllllllllllmmo........", //
            "..olmmmllllllllllllllllmmmo.........", //
            ".oolmmmmssssssssssssmmmmmmo.........", //
            "olmmo.mmmsssssssssmmmmo.oo..........", //
            "olo...ommmmmmmmmmmmmmo..............", //
            "oo.....odmo.odmmo..odmo.odmmo.......", //
            ".......odmo..odmo..odmo..odmo.......", //
            "......odmo..odmo..odmo..odmo........", //
            "......odmo..odmo..odmo..odmo........", //
            "......okko..okko..okko..okko........", //
        ],
    ],
    frame_seconds: 0.5,
};

/// A fangrunner: a lean striped pack hunter of the jungle, ochre with dark stripes, a
/// pale throat and a red eye, facing right (run A/B, leaping).
pub const FANGRUNNER: SpriteDef = SpriteDef {
    name: "enemy_fangrunner",
    legend: &[
        ('o', INK),
        ('d', SOIL_DARK),
        ('m', RUST),
        ('l', AMBER),
        ('s', CREAM),
        ('r', RED),
        ('e', BONE),
    ],
    frames: &[
        &[
            ".............oooo...", //
            "............ollllo..", //
            "...........olllrlloo", //
            "o..........ollllleeo", //
            ".o....ooooolllloooo.", //
            "..ooolldlldllloo....", //
            "...olldlldlldllo....", //
            "....omddmddmssso....", //
            "....omo.oo..omo.....", //
            "...omo.......omo....", //
            "...oo.........oo....", //
        ],
        &[
            ".............oooo...", //
            "............ollllo..", //
            "...........olllrlloo", //
            "...........ollllleeo", //
            "oo....ooooolllloooo.", //
            "..ooolldlldllloo....", //
            "...olldlldlldllo....", //
            "....omddmddmssso....", //
            ".....omoo..omo......", //
            "......omo.omo.......", //
            ".......oo.oo........", //
        ],
        &[
            "....................", //
            "....................", //
            "...............oooo.", //
            "..............ollllo", //
            "oo..ooooooo..olllrlo", //
            "..ooolldlldlolllllee", //
            "...olldlldlldllloooo", //
            "....omddmddmssssoo..", //
            "...omo.......omo....", //
            "..omo.........omo...", //
            "..oo...........oo...", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A hangclaw: a sand-coloured clawed thing that hangs from an isle's underside by its
/// hooked feet, blue eyes in a hard shell (hanging still, awake, dropping claws first).
pub const HANGCLAW: SpriteDef = SpriteDef {
    name: "enemy_hangclaw",
    legend: &[
        ('o', INK),
        ('d', SOIL),
        ('m', CLAY),
        ('l', SAND),
        ('s', CREAM),
        ('b', SKY),
        ('w', ICE),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            "...............", //
            "...............", //
            ".k...k...k...k.", //
            ".ko.oko.oko.ok.", //
            "..oooooooooooo.", //
            ".olllllllllllo.", //
            "olllmlllllmlllo", //
            "ollmmmmlmmmmllo", //
            ".olmmmmmmmmmlo.", //
            "..ommmmmmmmmo..", //
            "...odddddddo...", //
            "....ooooooo....", //
        ],
        &[
            "...............", //
            ".k...k...k...k.", //
            ".ko.oko.oko.ok.", //
            "..oooooooooooo.", //
            ".olllllllllllo.", //
            "olllmlllllmlllo", //
            "ollmbwlllbwmllo", //
            ".olmbblmlbbmlo.", //
            "..ommmmmmmmmo..", //
            "...odsdsdsdo...", //
            "...ok.ooo.ko...", //
            "...k.......k...", //
        ],
        &[
            "...k.......k...", //
            "...ko.....ok...", //
            "....oooooooo...", //
            "..ollllllllllo.", //
            ".olmbwlllbwmllo", //
            "olmmbbllmbbmmlo", //
            "olmmmmmmmmmmmlo", //
            ".ommmmmmmmmmmo.", //
            "..odsdsdsdsdo..", //
            "..ok..ooo..ko..", //
            ".ok.........ko.", //
            ".k...........k.", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A lode mite: a tiny deep-blue magnetic mite with ice-bright feelers, buzzing.
pub const LODE_MITE: SpriteDef = SpriteDef {
    name: "enemy_lode_mite",
    legend: &[
        ('o', INK),
        ('d', DEEP_BLUE),
        ('m', OCEAN),
        ('l', SKY),
        ('w', ICE),
    ],
    frames: &[
        &[
            ".w....w.", //
            "..o..o..", //
            ".oommoo.", //
            "omllmmdo", //
            "omlmmmdo", //
            ".oommoo.", //
            ".o.oo.o.", //
        ],
        &[
            "..w..w..", //
            "..o..o..", //
            ".oommoo.", //
            "omllmmdo", //
            "omlmmmdo", //
            ".oommoo.", //
            "o..oo..o", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A cinder drake: a sun-basking salamander of gold and ember, a glowing back ridge and a
/// white-hot throat, facing right (walk A/B, breathing fire).
pub const CINDER_DRAKE: SpriteDef = SpriteDef {
    name: "enemy_cinder_drake",
    legend: &[
        ('o', INK),
        ('d', MAROON),
        ('m', RUST),
        ('l', ORANGE),
        ('y', GOLD),
        ('s', CREAM),
        ('w', BONE),
    ],
    frames: &[
        &[
            ".......y.y.y.y...........", //
            "......oyoyoyoyo..ooo.....", //
            ".....ollllllllloollyo....", //
            "..oolllllllllllllllso....", //
            "oollllllllllllllooooo....", //
            ".ommmmllllllllmmo........", //
            "..oomdmmmmmmmmdmo........", //
            "...odo.odo..odo.odo......", //
            "...oo..oo...oo..oo.......", //
        ],
        &[
            ".......y.y.y.y...........", //
            "......oyoyoyoyo..ooo.....", //
            ".....ollllllllloollyo....", //
            "..oolllllllllllllllso....", //
            "oollllllllllllllooooo....", //
            ".ommmmllllllllmmo........", //
            "..oomdmmmmmmmmdmo........", //
            "....odoodo...odoodo......", //
            "....oo.oo....oo.oo.......", //
        ],
        &[
            ".......y.y.y.y...........", //
            "......oyoyoyoyo..ooo.....", //
            ".....ollllllllloollyo.yy.", //
            "..oolllllllllllllllswyssy", //
            "oollllllllllllllooowsyyw.", //
            ".ommmmllllllllmmo...yy...", //
            "..oomdmmmmmmmmdmo........", //
            "...odo.odo..odo.odo......", //
            "...oo..oo...oo..oo.......", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A shade stalker: a ragged shape of the Sunforge's shadows, violet-black with wisping
/// edges and two pale eyes (drifting A/B, lunging).
pub const SHADE_STALKER: SpriteDef = SpriteDef {
    name: "enemy_shade_stalker",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('p', PLUM),
        ('v', VIOLET),
        ('w', BONE),
        ('l', LAVENDER),
    ],
    frames: &[
        &[
            ".....ooooo.....", //
            "...oonnnnnoo...", //
            "..onnnnnnnnno..", //
            ".onnwnnnnnwnno.", //
            ".onnwnnnnnwnno.", //
            "onnnnnnnnnnnnno", //
            "onnnppnnnppnnno", //
            ".onnpvpnpvpnno.", //
            ".onpvvpppvvpno.", //
            "..onp.pvp.pno..", //
            "..op..p.p..po..", //
            "...p..v...p....", //
            "......p........", //
        ],
        &[
            ".....ooooo.....", //
            "...oonnnnnoo...", //
            "..onnnnnnnnno..", //
            ".onnwnnnnnwnno.", //
            ".onnwnnnnnwnno.", //
            "onnnnnnnnnnnnno", //
            "onnnppnnnppnnno", //
            ".onnpvpnpvpnno.", //
            ".onpvvpppvvpno.", //
            "..onpp.v.ppno..", //
            "..o.p..p..p.o..", //
            "....v...p..v...", //
            "........p......", //
        ],
        &[
            "...............", //
            "....ooooooo....", //
            "..oonnnnnnnoo..", //
            ".onnnnnnnnnnno.", //
            "onnlwnnnnnlwnno", //
            "onnwwnnnnnwwnno", //
            "onnnnnoooonnnno", //
            "onnnnowwwwonnno", //
            ".onnnoooooonno.", //
            ".onpvvpppvvpno.", //
            "..onpvvpvvpno..", //
            "...oop.p.poo...", //
            "....p..v..p....", //
        ],
    ],
    frame_seconds: 0.35,
};

/// A flarefly: a tiny white-gold sun with flickering ember wings.
pub const FLAREFLY: SpriteDef = SpriteDef {
    name: "enemy_flarefly",
    legend: &[
        ('o', RUST),
        ('y', GOLD),
        ('s', CREAM),
        ('w', BONE),
        ('r', ORANGE),
    ],
    frames: &[
        &[
            "r......r", //
            ".r.oo.r.", //
            "..oyyo..", //
            ".oywsyo.", //
            ".oysyyo.", //
            "..oyyo..", //
            ".r.oo.r.", //
            "r......r", //
        ],
        &[
            "........", //
            "r..oo..r", //
            ".roysor.", //
            ".oywsyo.", //
            ".oysyyo.", //
            ".royyor.", //
            "r..oo..r", //
            "........", //
        ],
    ],
    frame_seconds: 0.07,
};

/// The Sun Serpent's head: a great golden serpent head with a ruby crest, white-hot eyes
/// and solar frills, facing right (gliding, the lunge's tell with its jaws agape, dazed
/// when it falls from its orbit).
pub const SUN_SERPENT: SpriteDef = SpriteDef {
    name: "enemy_sun_serpent",
    legend: &[
        ('o', INK),
        ('d', RUST),
        ('m', ORANGE),
        ('l', AMBER),
        ('y', GOLD),
        ('s', CREAM),
        ('w', BONE),
        ('r', RED),
        ('k', MAROON),
    ],
    frames: &[
        &[
            "..........r.r.r...........", //
            ".........orororo..........", //
            ".......oorrrrrrroo........", //
            ".....ooyyyyyyyyyyyoo......", //
            "...ooyyllllllllllyyyoo....", //
            "..oyyllllllllllllllyyyo...", //
            ".oyllllllllllllllwwolyyo..", //
            "oyllmmmlllllllllwwwollyoo.", //
            "oylmmmmmmllllllllooolllyyo", //
            "oylmmssmmmmllllllllllllsso", //
            "oyllmmssmmmmmmmmmmmmmmmsso", //
            ".oyllmmmmmmmmmmmmmmmmmooo.", //
            "..oyyllmmmdddddddddooo....", //
            "...ooyyyllddkkkkkoo.......", //
            ".....oooyyyddooooo........", //
            "........ooooo.............", //
        ],
        &[
            "..........r.r.r...........", //
            ".........orororo..........", //
            ".......oorrrrrrroo........", //
            ".....ooyyyyyyyyyyyoo......", //
            "...ooyyllllllllllyyyoo....", //
            "..oyyllllllllllllllyyyoo..", //
            ".oyllllllllllllllwwolyyyo.", //
            "oyllmmmlllllllllwwwolllyyo", //
            "oylmmmmmmlllllllloooooooo.", //
            "oylmmssmmmmllllo.w.w.w.w..", //
            "oyllmmssmmmmmmo...........", //
            ".oyllmmmmmmmmmo.w.w.w.w...", //
            "..oyyllmmmdddddooooooooo..", //
            "...ooyyyllddkkkkkkkkkkoo..", //
            ".....oooyyyddooooooooo....", //
            "........ooooo.............", //
        ],
        &[
            "..........k.k.k...........", //
            ".........okokoko..........", //
            ".......ookkkkkkkoo........", //
            ".....ooddddddddddddoo.....", //
            "...ooddmmmmmmmmmmmdddoo...", //
            "..oddmmmmmmmmmmmmmmmmddo..", //
            ".odmmmmmmmmmmmmmmmoomddo..", //
            "odmmllmmmmmmmmmmmo.ommdoo.", //
            "odmlllllmmmmmmmmmmoommmddo", //
            "odmllsslllmmmmmmmmmmmmmsso", //
            "odmmllsslllllllllllllllsso", //
            ".odmmlllllllllllllllllooo.", //
            "..oddmmllldddddddddooo....", //
            "...ooddmmmddkkkkkoo.......", //
            ".....ooodddddooooo........", //
            "........ooooo.............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// One of the Sun Serpent's coils: a golden armoured segment; a sun scale blazes white
/// at its heart until it is cracked (sun scale, armour, cracked scale).
pub const SERPENT_COIL: SpriteDef = SpriteDef {
    name: "enemy_serpent_coil",
    legend: &[
        ('o', INK),
        ('d', RUST),
        ('m', ORANGE),
        ('l', AMBER),
        ('y', GOLD),
        ('s', CREAM),
        ('w', BONE),
        ('k', MAROON),
        ('n', NIGHT),
    ],
    frames: &[
        &[
            ".....oooooo.....", //
            "...ooyyyyyyoo...", //
            "..oyyllllllyyo..", //
            ".oyllssssssllyo.", //
            ".oylsswwwwssllo.", //
            "oyllswwwwwwslldo", //
            "oylsswwwwwwssldo", //
            "oylsswwwwwwssldo", //
            "oyllswwwwwwslldo", //
            "oyllsswwwwsslldo", //
            ".oylllssssllldo.", //
            ".oyylllllllldmo.", //
            "..oddmmmmmmddo..", //
            "...oommmmmmoo...", //
            ".....oooooo.....", //
        ],
        &[
            ".....oooooo.....", //
            "...ooyyyyyyoo...", //
            "..oyyllllllyyo..", //
            ".oylllllllllllo.", //
            ".oyllyyyllyylldo", //
            "oyllyllyyllylldo", //
            "oyllllllllllldmo", //
            "oylllyyllyyllldo", //
            "oylllllllllllldo", //
            "oyllyyllyyllldmo", //
            ".oyllllllllldmo.", //
            ".oyylllllllldmo.", //
            "..oddmmmmmmddo..", //
            "...oommmmmmoo...", //
            ".....oooooo.....", //
        ],
        &[
            ".....oooooo.....", //
            "...ooyyyyyyoo...", //
            "..oyyllllllyyo..", //
            ".oyllkkkkkkllyo.", //
            ".oylknnonnnklldo", //
            "oyllknonnnnklldo", //
            "oylknnnonnnnkldo", //
            "oylknnnnonnnkldo", //
            "oyllknnnnonklldo", //
            "oyllknnnononklmo", //
            ".oylllkkkkklldo.", //
            ".oyylllllllldmo.", //
            "..oddmmmmmmddo..", //
            "...oommmmmmoo...", //
            ".....oooooo.....", //
        ],
    ],
    frame_seconds: 0.0,
};
