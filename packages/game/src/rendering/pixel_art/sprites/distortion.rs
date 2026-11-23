//! The distortion strata's creatures (Wave 9, Descent VI part A): one animated sprite per kind
//! and their shots. Asymmetric creatures face right (the renderer mirrors them).

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A mirror twin: a rig's silvered reflection, glassy and cold, its cab window dark, facing
/// right (hovering, thrusting).
pub const MIRROR_TWIN: SpriteDef = SpriteDef {
    name: "enemy_mirror_twin",
    legend: &[
        ('o', SLATE_DARK),
        ('s', SLATE),
        ('m', MIST),
        ('w', SKY_LIGHT),
        ('i', ICE),
        ('k', INK),
        ('t', STEEL),
        ('l', STEEL_LIGHT),
        ('f', SKY),
    ],
    frames: &[
        &[
            "..................", //
            "......oooooo......", //
            ".....owwwwkko.....", //
            "....owmmmwkkko....", //
            "...oomimmmmmmoo...", //
            "..osmmmwmmmmmmso..", //
            "..ommiimmmmwmmmo..", //
            "..osmmmmmmmmmmso..", //
            "..oosssssssssoo...", //
            "...otltltltltlo...", //
            "...ottttttttto....", //
            ".....oollloo......", //
            "......olllo.......", //
            ".......oio........", //
            "........o.........", //
        ],
        &[
            "..................", //
            "......oooooo......", //
            ".....owwwwkko.....", //
            "....owmmmwkkko....", //
            "...oomimmmmmmoo...", //
            "..osmmmwmmmmmmso..", //
            "..ommiimmmmwmmmo..", //
            "..osmmmmmmmmmmso..", //
            "..oosssssssssoo...", //
            "...oltltltltlto...", //
            "...ottttttttto....", //
            ".....oollloo......", //
            "......oiwio.......", //
            ".....f.iwi.f......", //
            "......f.f.f.......", //
        ],
    ],
    frame_seconds: 0.18,
};

/// A glassback: a broad crab whose face is a mirror (it throws shots back), violet shell over
/// grey legs, facing right (walk A/B, rearing to charge).
pub const GLASSBACK: SpriteDef = SpriteDef {
    name: "enemy_glassback",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', SLATE),
        ('g', STONE_DARK),
        ('b', STONE),
        ('m', MIST),
        ('w', SKY_LIGHT),
        ('i', ICE),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..................", //
            ".......ooooo.oo...", //
            ".....oovvvvvowio..", //
            "....ovppvvppvwmio.", //
            "...ovvvvvvvvvwmwo.", //
            "..ovpvvvvvpvvomio.", //
            "..ovvvvvvvvvvowio.", //
            "..oggggggggggoeo..", //
            "...obbbbbbbbbbo...", //
            "..ob.ob.ob.ob.bo..", //
            "..o..o..o..o...o..", //
            "..................", //
            "..................", //
        ],
        &[
            "..................", //
            ".......ooooo.oo...", //
            ".....oovvvvvowio..", //
            "....ovppvvppvwmio.", //
            "...ovvvvvvvvvwmwo.", //
            "..ovpvvvvvpvvomio.", //
            "..ovvvvvvvvvvowio.", //
            "..oggggggggggoeo..", //
            "...obbbbbbbbbbo...", //
            "...obo.obo.obo....", //
            "...o.o.o.o.o.o....", //
            "..................", //
            "..................", //
        ],
        &[
            "............oo....", //
            "......ooooo.owio..", //
            "....oovvvvvvowmio.", //
            "...ovppvvppvvwmwo.", //
            "..ovvvvvvvvvvomio.", //
            "..ovpvvvvvpvvowio.", //
            "..ogggggggggggoo..", //
            "...obbbbbbbbbbeo..", //
            "..obbbbbbbbbbbbbo.", //
            ".obo.obo.obo..obo.", //
            ".o...o.o.o.o...o..", //
            "..................", //
            "..................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A gazer: a crystal eye on a stalk of silvered glass (closed, opening, staring).
pub const GAZER: SpriteDef = SpriteDef {
    name: "enemy_gazer",
    legend: &[
        ('o', SLATE_DARK),
        ('s', SLATE),
        ('m', MIST),
        ('w', SKY_LIGHT),
        ('i', ICE),
        ('k', INK),
        ('p', VIOLET),
        ('r', PINK),
    ],
    frames: &[
        &[
            "...........", //
            "...ooooo...", //
            "..ommmmmo..", //
            ".omwmmmmmo.", //
            ".ossssssso.", //
            ".ommmmmmmo.", //
            "..ommmmmo..", //
            "...ooooo...", //
            "....oso....", //
            "....omo....", //
            "....oso....", //
            "...osmso...", //
            "..ossmsso..", //
            ".osssssssso", //
            "...........", //
        ],
        &[
            "...........", //
            "...ooooo...", //
            "..ommmmmo..", //
            ".omwiiimmo.", //
            ".oippkppio.", //
            ".omiiiiimo.", //
            "..ommmmmo..", //
            "...ooooo...", //
            "....oso....", //
            "....omo....", //
            "....oso....", //
            "...osmso...", //
            "..ossmsso..", //
            ".osssssssso", //
            "...........", //
        ],
        &[
            "....r.r....", //
            "...ooooo...", //
            "..oiiiiio..", //
            ".oiwppppio.", //
            ".oipkkkpio.", //
            ".oippppwio.", //
            "..oiiiiio..", //
            "...ooooo...", //
            "....oso....", //
            "....omo....", //
            "....oso....", //
            "...osmso...", //
            "..ossmsso..", //
            ".osssssssso", //
            "...........", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A tumbler: a ball of banded stone with a shut eye and stubby claws, rolling (four turns).
pub const TUMBLER: SpriteDef = SpriteDef {
    name: "enemy_tumbler",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('s', STONE),
        ('t', STONE_LIGHT),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..............", //
            ".....oooo.....", //
            "...oovvllvoo..", //
            "..ovvlltlvvvo.", //
            "..ovssssssvvo.", //
            ".ovvvvvvvvvvdo", //
            ".ovvvoevvvvvdo", //
            ".ovsssssssssdo", //
            ".ovvvvvvvvvddo", //
            "..ovvvvvvvddo.", //
            "..odssssssddo.", //
            "...oodddddoo..", //
            ".....oooo.....", //
            "..............", //
        ],
        &[
            "..............", //
            ".....oooo.....", //
            "...oovvsvvoo..", //
            "..ovlvvsvvvdo.", //
            "..ovtvvsvvvdo.", //
            ".ovlvvvsvvvvdo", //
            ".ovvvvvseovvdo", //
            ".ovvvvvsvvvvdo", //
            ".ovvvvvsvvvddo", //
            "..ovvvvsvvvdo.", //
            "..odvvvsvvddo.", //
            "...ooddsddoo..", //
            ".....oooo.....", //
            "..............", //
        ],
        &[
            "..............", //
            ".....oooo.....", //
            "...ooddddvoo..", //
            "..oddssssssdo.", //
            "..oddvvvvvvvo.", //
            ".odvvvvvvvvvvo", //
            ".odssssssssslo", //
            ".odvvvvveovvvo", //
            ".odvvvvvvvvvvo", //
            "..ovssssssslo.", //
            "..ovvvvvvvlto.", //
            "...oovvvvloo..", //
            ".....oooo.....", //
            "..............", //
        ],
        &[
            "..............", //
            ".....oooo.....", //
            "...oovvsvvoo..", //
            "..odvvvsvvvlo.", //
            "..odvvvsvvvto.", //
            ".odvvvvsvvvvlo", //
            ".odvvoesvvvvvo", //
            ".odvvvvsvvvvvo", //
            ".oddvvvsvvvvvo", //
            "..odvvvsvvvvo.", //
            "..oddvvsvvvdo.", //
            "...ooddsddoo..", //
            ".....oooo.....", //
            "..............", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A lodestar: a knot of pink-violet gravity inside two thin orbiting rings (pulsing).
pub const LODESTAR: SpriteDef = SpriteDef {
    name: "enemy_lodestar",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('p', PINK),
        ('r', ROSE),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "..............", //
            "......ll......", //
            "...l..dd..l...", //
            "....ldvvdl....", //
            "...dvvrrvvd...", //
            "..dvrpppprvd..", //
            ".ldvrpwwprvdl.", //
            ".ldvrpwwprvdl.", //
            "..dvrpppprvd..", //
            "...dvvrrvvd...", //
            "....ldvvdl....", //
            "...l..dd..l...", //
            "......ll......", //
            "..............", //
        ],
        &[
            "..............", //
            "..l...ll...l..", //
            "......dd......", //
            "....odvvdo....", //
            "...dvrrrrvd...", //
            "..dvrppppprd..", //
            "l.dvppwwpprd.l", //
            "l.drppwwppvd.l", //
            "..drpppppvvd..", //
            "...dvrrrrvd...", //
            "....odvvdo....", //
            "......dd......", //
            "..l...ll...l..", //
            "..............", //
        ],
    ],
    frame_seconds: 0.3,
};

/// A shear mantis: a long pink-violet mantis with folded scythes, facing right (hanging still,
/// stalking, striking with its scythes open).
pub const SHEAR_MANTIS: SpriteDef = SpriteDef {
    name: "enemy_shear_mantis",
    legend: &[
        ('o', INK),
        ('d', PLUM),
        ('v', VIOLET),
        ('r', ROSE),
        ('p', PINK),
        ('e', LIME),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "................", //
            "...........oo...", //
            "..........ovpo..", //
            "..........opeo..", //
            "...oooooooovvo..", //
            "..ovvvvrvvvvdo..", //
            ".ovrvvvvvvvdwo..", //
            "..odddddddd.ow..", //
            "...o.o..o.o..o..", //
            "...o.o..o.o.....", //
            "................", //
            "................", //
        ],
        &[
            "................", //
            "...........oo...", //
            "..........ovpo..", //
            "..........opeo..", //
            "...oooooooovvo..", //
            "..ovvvvrvvvvdo..", //
            ".ovrvvvvvvvdwo..", //
            "..odddddddd.ow..", //
            "..o.o..o.o...o..", //
            ".o...o.o..o.....", //
            "................", //
            "................", //
        ],
        &[
            "..............w.", //
            "...........oowo.", //
            "..........ovpow.", //
            "..........opeow.", //
            "...ooooooooovvow", //
            "..ovvvvrvvvvdoww", //
            ".ovrvvvvvvvdo...", //
            "..odddddddddo...", //
            "...o.o..o.o.....", //
            "..o...o..o.o....", //
            "................", //
            "................", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A glitchling: a scrap of broken pixels in lime and magenta that never quite holds still
/// (three glitches).
pub const GLITCHLING: SpriteDef = SpriteDef {
    name: "enemy_glitchling",
    legend: &[
        ('o', INK),
        ('g', LIME),
        ('m', MINT),
        ('p', PINK),
        ('t', TEAL),
        ('w', CREAM),
    ],
    frames: &[
        &[
            "...........", //
            "..oooo.....", //
            "..oggggoo..", //
            ".ogmgwgggo.", //
            ".oggggwmgo.", //
            "..ppgggggo.", //
            ".oggoggggo.", //
            ".ogtgggpp..", //
            "..oggggo...", //
            "...oooo....", //
            "...........", //
        ],
        &[
            "...........", //
            "...oooo....", //
            ".oggggoo...", //
            "ogmgwgggo..", //
            ".oggggwmgo.", //
            "...ppgggggo", //
            ".oggoggggo.", //
            "ogtgggpp...", //
            "..oggggo...", //
            "....oooo...", //
            "...........", //
        ],
        &[
            "...........", //
            "..oooo.....", //
            "..oppppoo..", //
            ".opmpwpppo.", //
            "..oggggwmgo", //
            "..ppgggggo.", //
            "oggoggggo..", //
            ".ogtgggpp..", //
            "..oggggo...", //
            "...oooo....", //
            "...........", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A bitworm's head: a square, teal and lime, two square eyes, its jaw opening (A/B). The
/// renderer trails square segments behind it.
pub const BITWORM: SpriteDef = SpriteDef {
    name: "enemy_bitworm",
    legend: &[
        ('o', INK),
        ('t', TEAL_DARK),
        ('m', TEAL),
        ('g', LIME),
        ('w', CREAM),
        ('r', RED),
    ],
    frames: &[
        &[
            "............", //
            ".oooooooooo.", //
            ".ommmmmmmmo.", //
            ".omggmmggmo.", //
            ".omwgmmwgmo.", //
            ".ommmmmmmmo.", //
            ".ommmmmmmmo.", //
            ".otttttttto.", //
            ".ommmmmmmmo.", //
            ".otttttttto.", //
            ".oooooooooo.", //
            "............", //
        ],
        &[
            "............", //
            ".oooooooooo.", //
            ".ommmmmmmmo.", //
            ".omggmmggmo.", //
            ".omwgmmwgmo.", //
            ".ommmmmmmmo.", //
            ".oooooooooo.", //
            ".orrrrrrrro.", //
            ".oooooooooo.", //
            ".otttttttto.", //
            ".oooooooooo.", //
            "............", //
        ],
    ],
    frame_seconds: 0.25,
};

/// A dead pixel: a square of pure black with a faint magenta fringe (it pulses before a rot
/// pulse).
pub const DEAD_PIXEL: SpriteDef = SpriteDef {
    name: "enemy_dead_pixel",
    legend: &[
        ('o', INK),
        ('n', NIGHT),
        ('p', PINK),
        ('g', LIME),
        ('v', PLUM),
    ],
    frames: &[
        &[
            ".............", //
            ".vvvvvvvvvvv.", //
            ".voooooooooov", //
            ".vonnnnnnnnov", //
            ".vonoooooonov", //
            ".vonoooooonov", //
            ".vonoooooonov", //
            ".vonoooooonov", //
            ".vonoooooonov", //
            ".vonnnnnnnnov", //
            ".voooooooooov", //
            "..vvvvvvvvvvv", //
            ".............", //
        ],
        &[
            "p...........g", //
            ".ppppppppppp.", //
            ".poooooooooop", //
            ".ponnnnnnnnop", //
            ".ponoooooonop", //
            ".ponoooooonop", //
            ".ponoooooonop", //
            ".ponoooooonop", //
            ".ponoooooonop", //
            ".ponnnnnnnnop", //
            ".poooooooooop", //
            "..ppppppppppp", //
            "g...........p", //
        ],
    ],
    frame_seconds: 0.0,
};

/// A shard of mirror glass spinning through the air (a twin's copy of a shot, or a shot thrown
/// back by a glassback).
pub const SHOT_MIRROR_SHARD: SpriteDef = SpriteDef {
    name: "shot_mirror_shard",
    legend: &[('o', SLATE), ('w', SKY_LIGHT), ('i', ICE)],
    frames: &[
        &[
            "..o..", //
            ".owo.", //
            "oiwio", //
            ".oio.", //
            "..o..", //
        ],
        &[
            ".....", //
            "oowoo", //
            "oiwio", //
            "oowoo", //
            ".....", //
        ],
    ],
    frame_seconds: 0.08,
};

/// A dead pixel's pulse of rot: a hollow square of glitching pixels.
pub const SHOT_ROT_PULSE: SpriteDef = SpriteDef {
    name: "shot_rot_pulse",
    legend: &[('o', INK), ('p', PINK), ('g', LIME)],
    frames: &[
        &[
            "ppggpp", //
            "po..op", //
            "g.oo.g", //
            "g.oo.g", //
            "po..op", //
            "ppggpp", //
        ],
        &[
            "ggppgg", //
            "go..og", //
            "p.oo.p", //
            "p.oo.p", //
            "go..og", //
            "ggppgg", //
        ],
    ],
    frame_seconds: 0.12,
};
