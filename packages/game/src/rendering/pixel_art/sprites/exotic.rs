//! The exotic worlds' creatures and theme bosses (Wave 13 planets-d): one animated sprite
//! per kind, drawn facing right (the renderer mirrors them). Names carry the track's `pd_`
//! prefix. Generated from shape sketches (ellipses, polygons and strokes, shaded light from
//! the upper left and outlined in ink), then kept here as plain rows.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A drift ray: a pale glider of the cloud tops, wings up and down, a whip of a tail.
pub const DRIFT_RAY: SpriteDef = SpriteDef {
    name: "pd_enemy_drift_ray",
    legend: &[
        ('o', INK),
        ('l', ICE),
        ('w', BONE),
        ('d', SKY_LIGHT),
        ('k', OCEAN),
    ],
    frames: &[
        &[
            ".........o........", //
            "........owo.......", //
            ".......owlwooooo..", //
            "ooooooowlllwwwwko.", //
            "ddddddwlllllllllwo", //
            "ooooooodddddddddo.", //
            ".......ooooooooo..", //
            "..................", //
        ],
        &[
            "..................", //
            "..................", //
            ".......ooooooooo..", //
            "ooooooowwwwwwwwko.", //
            "ddddddddddddddllwo", //
            "ooooooodddddddddo.", //
            ".......odddooooo..", //
            "........odo.......", //
        ],
    ],
    frame_seconds: 0.32,
};

/// A gasbag: a great mint bladder glowing from inside, a small lilac body slung under it,
/// tendrils trailing (the sac breathes).
pub const GASBAG: SpriteDef = SpriteDef {
    name: "pd_enemy_gasbag",
    legend: &[
        ('o', INK),
        ('m', MINT),
        ('g', ICE),
        ('t', TEAL),
        ('w', BONE),
        ('c', LAVENDER),
        ('e', GOLD),
    ],
    frames: &[
        &[
            ".....ooooggggoooo.....", //
            "....oggggmmmmggggo....", //
            "..oogmwmmmmmmmmmmgoo..", //
            ".oggmwwwmmmmmmmmmmggo.", //
            ".ogmmmwmmmmmmmmmmmmto.", //
            "ogmmmmmmmmmmmmmmmmmmgo", //
            "ogmmmmmmmmmmmmmmmmmmto", //
            ".ogmmmmmmmmmmmmmmmmto.", //
            ".ogtmmmmmmmmmmmmmmtto.", //
            "..oogmmmmmmmmmmmmtoo..", //
            "..otttttttttttttttto..", //
            "...ooooocccccccccceo..", //
            "......ooccccccccccco..", //
            ".....otoootoootoooto..", //
            "....otto.ottotto.otto.", //
            "....oto...ototo...oto.", //
        ],
        &[
            ".....ooooggggoooo.....", //
            "....oggggmmmmggggo....", //
            "..oogmmwmmmmmmmmmgoo..", //
            ".oggmmwwwmmmmmmmmmggo.", //
            ".ogmmmmwmmmmmmmmmmmto.", //
            "ogmmmmmmmmmmmmmmmmmmgo", //
            "ogmmmmmmmmmmmmmmmmmmto", //
            ".ogmmmmmmmmmmmmmmmmto.", //
            ".ogtmmmmmmmmmmmmmmtto.", //
            "..oogmmmmmmmmmmmmtoo..", //
            "..otttttttttttttttto..", //
            "...ooooocccccccccceo..", //
            "......ooccccccccccco..", //
            ".....otoootoootoooto..", //
            ".....ottotto.ottotto..", //
            "......ototo...ototo...", //
        ],
    ],
    frame_seconds: 0.60,
};

/// A storm eel: a violet ribbon with gold sparks down its back (swim A / B; crackling before it
/// lunges).
pub const STORM_EEL: SpriteDef = SpriteDef {
    name: "pd_enemy_storm_eel",
    legend: &[
        ('o', INK),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('p', PLUM),
        ('y', GOLD),
        ('w', BONE),
    ],
    frames: &[
        &[
            "........................", //
            "........oooo.......ooo..", //
            ".......olylloo....olllo.", //
            ".o....olvppvlloooolvvvwo", //
            "oloooolppoolyvllllyvvvvl", //
            ".olyllyoo..olvvvvvvppppo", //
            "..olppo.....olpyvppoooo.", //
            "...ooo.......oolpoo.....", //
        ],
        &[
            ".................ooo....", //
            "......ooo.......ollloo..", //
            "....oolllo.....olvyvllo.", //
            "...ollypploo.oolvvvvvvwo", //
            ".oolpooooyllollyvppvvvvl", //
            "ollyo....olvlvvppoolpppo", //
            "olpo......olyvpoo..oooo.", //
            ".oo........olpo.........", //
        ],
        &[
            "...oyo.......oyo...o...o", //
            "....o..oooo...o..ooloooy", //
            "......ollllo....ollvlloo", //
            ".....oyppyvloooolvyvvvwo", //
            ".oooolpooolvllllvvpvvvvl", //
            "ollllpo...olyvvyvpolpppo", //
            ".olyoo...o.olpvppo.oooo.", //
            "..oo....oyo.ooloo.oyo...", //
        ],
    ],
    frame_seconds: 0.20,
};

/// A vapour stinger: an amber wasp with pale wings and a black sting (wings up / down).
pub const VAPOR_STINGER: SpriteDef = SpriteDef {
    name: "pd_enemy_vapor_stinger",
    legend: &[
        ('o', INK),
        ('a', AMBER),
        ('y', CREAM),
        ('r', RUST),
        ('n', INK),
        ('w', ICE),
        ('k', RED),
    ],
    frames: &[
        &[
            "...oooo...", //
            "..owwwwo..", //
            "..owwwwoo.", //
            "..oooooyyo", //
            ".oyyyyyako", //
            ".onnanrrro", //
            "onooyrooo.", //
            "no..oo....", //
        ],
        &[
            "..........", //
            "..oooo....", //
            ".owwwwooo.", //
            "..owwwoyyo", //
            ".oyyyyyako", //
            ".onnanrrro", //
            "onooyrooo.", //
            "no..oo....", //
        ],
    ],
    frame_seconds: 0.08,
};

/// The Thunderhead: a towering storm cloud with a slate underbelly, slit eyes and a lightning
/// heart (idle A / B with rain; telling: eyes white, heart blazing, bolts forking; open: the
/// cloud parted round a gold heart).
pub const THUNDERHEAD: SpriteDef = SpriteDef {
    name: "pd_enemy_thunderhead",
    legend: &[
        ('o', INK),
        ('c', MIST),
        ('w', BONE),
        ('f', FOG),
        ('s', SLATE),
        ('d', SLATE_DARK),
        ('b', SKY_LIGHT),
        ('n', INK),
        ('v', VIOLET),
        ('l', LAVENDER),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "............oooooowwwwwwwwwo............", //
            "..........oowwwwwwcccccccccwoooooo......", //
            ".........owwccccccccccccccccwwwwwwoo....", //
            "........owccccccccccccccccccccccccwwo...", //
            ".....ooowcccccccccccccccccccccccccccwo..", //
            "...oowwwcccccccccccccccccccccccccccccwo.", //
            "..owwccccccccccccccccccccccccccccccccfo.", //
            ".owccccccccccccccccccccccccccccccccccfo.", //
            "owccccccccccccccccccccccccccccccccccccwo", //
            "owccccccccccccccccccccccccccccccccccccfo", //
            "owcccccccccccccccccccccccccccccccccccccw", //
            "wcccccccccccccccsssssssssccccccccccccccf", //
            "owccccccccsssnbbsssssssssbbnsssccccccccf", //
            "owcccccssssssbbbsssssssssbbbsssssscccccf", //
            "owcccssssssssssssssssssssssssssssssscccf", //
            ".owcssssssssssssssssdsssssssssssssssscfo", //
            "..ossssssdddddddddvvvvvdddddddddsssssso.", //
            "...ossdddddddddddvvvlvvvdddddddddddsso..", //
            "....odddddddddddddvvvvvdddddddddddddo...", //
            ".....oodddddddddddddddddddddddddddoo....", //
            ".......ooodddddddddddddddddddddooo......", //
            "......o...oooooossssdssssoooooo.........", //
            ".....obo..o..oboooooooboo.o..obo..o.....", //
            "....obbo.oboobbo.oboobbo.oboobbo.obo....", //
            "....obo.obboobo.obboobo.obboobo.obbo....", //
            ".....o..obo..o..obo..o..obo..o..obo.....", //
        ],
        &[
            "............oooooowwwwwwwwwo............", //
            "..........oowwwwwwcccccccccwoooooo......", //
            ".........owwccccccccccccccccwwwwwwoo....", //
            "........owccccccccccccccccccccccccwwo...", //
            ".....ooowcccccccccccccccccccccccccccwo..", //
            "...oowwwcccccccccccccccccccccccccccccwo.", //
            "..owwccccccccccccccccccccccccccccccccfo.", //
            ".owccccccccccccccccccccccccccccccccccfo.", //
            "owccccccccccccccccccccccccccccccccccccwo", //
            "owccccccccccccccccccccccccccccccccccccfo", //
            "owcccccccccccccccccccccccccccccccccccccw", //
            "wcccccccccccccccsssssssssccccccccccccccf", //
            "owccccccccsssnwwssssssssswwnsssccccccccf", //
            "owcccccsssssswwwssssssssswwwsssssscccccf", //
            "owcccssssssssssssssssssssssssssssssscccf", //
            ".owcssssssssssssssssdsssssssssssssssscfo", //
            "..ossssssdddddddddvvvvvdddddddddsssssso.", //
            "...ossdddddddddddvvvlvvvdddddddddddsso..", //
            "....odddddddddddddvvvvvdddddddddddddo...", //
            ".....oodddddddddddddddddddddddddddoo....", //
            ".......ooodddddddddddddddddddddooo......", //
            "..........oooooossssdssssoooooo...o.....", //
            "......o..obo..o.oobooooooobo..o..obo....", //
            ".....oboobbo.oboobbo.oboobbo.oboobbo....", //
            "....obboobo.obboobo.obboobo.obboobo.....", //
            "....obo..o..obo..o..obo..o..obo..o......", //
        ],
        &[
            "............oooooowwwwwwwwwo............", //
            "..........oowwwwwwcccccccccwoooooo......", //
            ".........owwccccccccccccccccwwwwwwoo....", //
            "........owccccccccccccccccccccccccwwo...", //
            ".....ooowcccccccccccccccccccccccccccwo..", //
            "...oowwwcccccccccccccccccccccccccccccwo.", //
            "..owwccccccccccccccccccccccccccccccccfo.", //
            ".owccccccccccccccccccccccccccccccccccfo.", //
            "owccccccccccccccccccccccccccccccccccccwo", //
            "owccccccccccccccccccccccccccccccccccccfo", //
            "owcccccccccccccccccccccccccccccccccccccw", //
            "wcccccccccccccccsssssssssccccccccccccccf", //
            "owccccccccsssnwwssssssssswwnsssccccccccf", //
            "owcccccsssssswwwssssssssswwwsssssscccccf", //
            "owcccssssssssssssssssssssssssssssssscccf", //
            ".owcsssssssssssssssyyyssssssssssssssscfo", //
            "..ossssssddddddddyyywyyyddddddddsssssso.", //
            "...ossdddddddddddyywwwyydddddddddddsso..", //
            "....oddddddddddddyyywyyyddddddddddddo...", //
            ".....ooddddddddddddyyyddddddddddddoo....", //
            ".......ooodddddddddddddddddddddooo......", //
            "........oyoooooossssyssssooooooyo.......", //
            ".......oyyo.....oooyyoooo....oyyo.......", //
            "........oyyo.......oyyo.......oyyo......", //
            "........oyo........oyo........oyo.......", //
            ".........oyo........oyo........oyo......", //
        ],
        &[
            "............oooooowwwwwwwwwo............", //
            "..........oowwwwwwcccccccccwoooooo......", //
            ".........owwccccccccccccccccwwwwwwoo....", //
            "........owccccccccccccccccccccccccwwo...", //
            ".....ooowcccccccccccccccccccccccccccwo..", //
            "...oowwwcccccccccccccccccccccccccccccwo.", //
            "..owwccccccccccccccccccccccccccccccccfo.", //
            ".owccccccccccccccccccccccccccccccccccfo.", //
            "owccccccccccccccccccccccccccccccccccccwo", //
            "owccccccccccccccccccccccccccccccccccccfo", //
            "owcccccccccccccccccccccccccccccccccccccw", //
            "wcccccccccccccccsssssssssccccccccccccccf", //
            "owccccccccsssnwwssssssssswwnsssccccccccf", //
            "owcccccsssssswwwsyyyyyyyswwwsssssscccccf", //
            "owcccsssssssssssyyyyyyyyyssssssssssscccf", //
            ".owcsssssssssssyyywwwwwyyyssssssssssscfo", //
            "..ossssssddddddyyywwwwwyyyddddddsssssso.", //
            "...ossdddddddddyyywwwwwyyydddddddddsso..", //
            "....odddddddddddyyyyyyyyydddddddddddo...", //
            ".....ooddddddddddyyyyyyyddddddddddoo....", //
            ".......ooodddddddddddddddddddddooo......", //
            "......o...oooooossssdssssoooooo.........", //
            ".....obo..o..oboooooooboo.o..obo..o.....", //
            "....obbo.oboobbo.oboobbo.oboobbo.obo....", //
            "....obo.obboobo.obboobo.obboobo.obbo....", //
            ".....o..obo..o..obo..o..obo..o..obo.....", //
        ],
    ],
    frame_seconds: 0.45,
};

/// A dusk grazer: a shaggy blue-grey herd beast with pale horns and glowing mint spots on its
/// back (stride A / B).
pub const DUSK_GRAZER: SpriteDef = SpriteDef {
    name: "pd_enemy_dusk_grazer",
    legend: &[
        ('o', INK),
        ('u', SLATE_LIGHT),
        ('h', FOG),
        ('U', SLATE),
        ('w', BONE),
        ('m', MINT),
        ('e', GOLD),
    ],
    frames: &[
        &[
            "..............owo.ow", //
            ".......ooooo..owooww", //
            "....ooohmhhhooowoowo", //
            "...ohmhuuuumhhhwwwo.", //
            ".oohuuuuuuuuuuuuuueo", //
            "oUUuuuumuuuuuuuuuuuh", //
            "oUhuuuuuuuuuuuuuuuUo", //
            "oUhuuuuuuuuuuuuuUUo.", //
            "UUohuuuuuuuuuuuUoo..", //
            "oooUuUuuuuuuuUuUo...", //
            "..oUUoUuuUUuUoUUUo..", //
            "..oUUoUUUooUUoUUUo..", //
            "..oUUooUUooUUooUUo..", //
            "..oUUooUUooUUooUUo..", //
        ],
        &[
            "..............owo.ow", //
            ".......ooooo..owooww", //
            "....ooohmhhhooowoowo", //
            "...ohmhuuuumhhhwwwo.", //
            ".oohuuuuuuuuuuuuuueo", //
            "oUUuuuumuuuuuuuuuuuh", //
            "oUhuuuuuuuuuuuuuuuUo", //
            "oUhuuuuuuuuuuuuuUUo.", //
            "UUohuuuuuuuuuuuUoo..", //
            "oooUuuuuuuuuuuuUo...", //
            "..oUUUUuUUUuUUUUo...", //
            "..oUUUUUoooUUUUUo...", //
            "...oUUUUo..oUUUUo...", //
            "...oUUUUo..oUUUUo...", //
        ],
    ],
    frame_seconds: 0.30,
};

/// A sunbasker: a gold lizard of mirror scales (scurry A / B).
pub const SUNBASKER: SpriteDef = SpriteDef {
    name: "pd_enemy_sunbasker",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('w', CREAM),
        ('r', ORANGE),
        ('k', INK),
    ],
    frames: &[
        &[
            "..............", //
            "..............", //
            "....oooooooooo", //
            "..oowwwwwwwwko", //
            ".owwgwgwgggggw", //
            "orrgrrrrggrrro", //
            "rrrroooorrooo.", //
            "ooro....oro...", //
        ],
        &[
            "..............", //
            "..............", //
            "....oooooooooo", //
            "..oowwwwwwwwko", //
            ".owwgwgwgggggw", //
            "orrggrrggrrrro", //
            "rrorroorroooo.", //
            "oo.orooro.....", //
        ],
    ],
    frame_seconds: 0.12,
};

/// A scorch mite: a glowing ember of a bug on buzzing wings.
pub const SCORCH_MITE: SpriteDef = SpriteDef {
    name: "pd_enemy_scorch_mite",
    legend: &[
        ('o', INK),
        ('r', ORANGE),
        ('y', GOLD),
        ('w', CREAM),
        ('k', MAROON),
    ],
    frames: &[
        &[
            "..ooo...", //
            ".owwwo..", //
            "..orroo.", //
            ".orryrko", //
            ".orrrroo", //
            "okooookk", //
        ],
        &[
            "........", //
            ".oooo...", //
            "owwwroo.", //
            ".orryrko", //
            ".orrrroo", //
            "okooookk", //
        ],
    ],
    frame_seconds: 0.07,
};

/// A frost stalker: a pale night cat with pricked ears and an ice-blue eye (stride A / B;
/// crouched to pounce).
pub const FROST_STALKER: SpriteDef = SpriteDef {
    name: "pd_enemy_frost_stalker",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('w', BONE),
        ('s', SKY),
        ('e', SKY_LIGHT),
        ('k', NIGHT),
    ],
    frames: &[
        &[
            ".............o.oo.", //
            "oo..........ososso", //
            "iio.........ososo.", //
            "oio..oooooooowwwo.", //
            "oiioowwwwwwwwiiieo", //
            ".oiwwiiiiiiiiiiisk", //
            ".owiiiiiiiiiiissoo", //
            "..owiiiiiiiiisoo..", //
            "...ossisssisso....", //
            "...ososooososo....", //
            "..oso.ososo.oso...", //
            "..oso.ososo.oso...", //
        ],
        &[
            ".............o.oo.", //
            "oo..........ososso", //
            "iio.........ososo.", //
            "oio..oooooooowwwo.", //
            "oiioowwwwwwwwiiieo", //
            ".oiwwiiiiiiiiiiisk", //
            ".owiiiiiiiiiiissoo", //
            "..owiiiiiiiiisoo..", //
            "...ossisssisso....", //
            "...ososooososo....", //
            "....oso...oso.....", //
            "....oso...oso.....", //
        ],
        &[
            "..................", //
            "..................", //
            ".............o.oo.", //
            "oo..........ososso", //
            "iio.........ososo.", //
            "oio..oooooooowwwo.", //
            "oiioowwwwwwwwiiieo", //
            ".oiwwiiiiiiiiiiisk", //
            ".owiiiiiiiiiiissoo", //
            "..owiiiiiiiiisoo..", //
            "..ossisssisiso....", //
            "..ososooososo.....", //
        ],
    ],
    frame_seconds: 0.16,
};

/// The Gloamwarden: a colossus half frozen (ice, left) and half scorched (gold, right), a stone
/// head with a blue eye and a gold one (idle; the sun arm raised to slam; the frost arm raised;
/// its chest cracked open).
pub const GLOAMWARDEN: SpriteDef = SpriteDef {
    name: "pd_enemy_gloamwarden",
    legend: &[
        ('o', INK),
        ('i', ICE),
        ('w', BONE),
        ('s', SKY),
        ('S', OCEAN),
        ('g', AMBER),
        ('y', GOLD),
        ('r', ORANGE),
        ('R', RUST),
        ('k', STONE_DARK),
        ('b', SKY_LIGHT),
        ('v', VIOLET),
        ('W', CREAM),
    ],
    frames: &[
        &[
            "..............................", //
            "..............ooo.............", //
            "............ookkkoo...........", //
            "...........okkkkkkko..........", //
            "..........okkkkkkkkko.........", //
            "..........okkkkkkkkko.........", //
            "..........obbbbkyyyyo.........", //
            "..........okkkkkkkkko.........", //
            "..........okkkkkkkkko.........", //
            "..........owkkkkkkkyo.........", //
            "......oooowiiikkkgggyoooo.....", //
            ".....ossswiiiiiggggggyrrro....", //
            ".....osssiiiiiigggggggrrro....", //
            "....ossssiiiiiigggggggrrrro...", //
            "....ossssiiiiiigggggggrrrro...", //
            "....osssiiiiiiiggggggggrrro...", //
            "...ossssiiiiiiivgggggggrrrro..", //
            "...ossssiiiiiivvvggggggrrrro..", //
            "...ossssiiiiiivvvggggggrrrro..", //
            "..ossssiiiiiiivvvgggggggrrrro.", //
            "..ossssiiiiiiiivggggggggrrrro.", //
            "..ossssiiiiiiiigggggggggrrrro.", //
            "..osssowiiiiiiiggggggggrorrro.", //
            ".ossssowiiiiiiiggggggggrorrrro", //
            ".ossssoowiiiiiigggggggroorrrro", //
            "oiiiio..owiiiiiggggggro..ogggg", //
            "oiiiio...owiiiigggggro...ogggg", //
            "oiiiio..oSSiiisgrgggRRo..ogggg", //
            ".oooo...oSSSSSoyoRRRRRo...oooo", //
            "........oSSSSSoooRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            ".......okkkkkko.okkkkkko......", //
            ".......okkkkkko.okkkkkko......", //
        ],
        &[
            ".........................ooyoo", //
            "..............ooo.......oyyyyy", //
            "............ookkkoo.....oyyyyy", //
            "...........okkkkkkko....oyyyyy", //
            "..........okkkkkkkkko...orryro", //
            "..........okkkkkkkkko..orrrro.", //
            "..........obbbbkyyyyo..orrrro.", //
            "..........okkkkkkkkko..orrro..", //
            "..........okkkkkkkkko.orrrro..", //
            "..........owkkkkkkkyo.orrrro..", //
            "......oooowiiikkkgggyorrrro...", //
            ".....ossswiiiiiggggggyrrrro...", //
            ".....osssiiiiiigggggggrrrro...", //
            "....ossssiiiiiigggggggrrro....", //
            "....ossssiiiiiiggggggggro.....", //
            "....osssiiiiiiigggggggggyo....", //
            "...ossssiiiiiiivggggggggro....", //
            "...ossssiiiiiivvvgggggggro....", //
            "...ossssiiiiiivvvgggggggro....", //
            "..ossssiiiiiiivvvgggggggro....", //
            "..ossssiiiiiiiivggggggggro....", //
            "..ossssiiiiiiiigggggggggro....", //
            "..osssowiiiiiiiggggggggro.....", //
            ".ossssowiiiiiiiggggggggro.....", //
            ".ossssoowiiiiiigggggggro......", //
            "oiiiio..owiiiiiggggggro.......", //
            "oiiiio...owiiiigggggro........", //
            "oiiiio..oSSiiisgrgggRRo.......", //
            ".oooo...oSSSSSoyoRRRRRo.......", //
            "........oSSSSSoooRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            ".......okkkkkko.okkkkkko......", //
            ".......okkkkkko.okkkkkko......", //
        ],
        &[
            "ooowoowo......................", //
            "wwwwwwo.......ooo.............", //
            "owwwwwo.....ookkkoo...........", //
            "owwwwwo....okkkkkkko..........", //
            ".oswsso...okkkkkkkkko.........", //
            "owosssso..okkkkkkkkko.........", //
            ".oosssso..obbbbkyyyyo.........", //
            "...ossso..okkkkkkkkko.........", //
            "...osssso.okkkkkkkkko.........", //
            "...osssso.owkkkkkkkyo.........", //
            "....ossssowiiikkkgggyoooo.....", //
            "....osssswiiiiiggggggyrrro....", //
            "....ossssiiiiiigggggggrrro....", //
            ".....osssiiiiiigggggggrrrro...", //
            "......owiiiiiiigggggggrrrro...", //
            ".....owiiiiiiiiggggggggrrro...", //
            ".....owiiiiiiiivgggggggrrrro..", //
            ".....owiiiiiiivvvggggggrrrro..", //
            ".....owiiiiiiivvvggggggrrrro..", //
            ".....owiiiiiiivvvgggggggrrrro.", //
            ".....owiiiiiiiivggggggggrrrro.", //
            ".....owiiiiiiiigggggggggrrrro.", //
            "......owiiiiiiiggggggggrorrro.", //
            "......owiiiiiiiggggggggrorrrro", //
            ".......owiiiiiigggggggroorrrro", //
            "........owiiiiiggggggro..ogggg", //
            ".........owiiiigggggro...ogggg", //
            "........oSSiiisgrgggRRo..ogggg", //
            "........oSSSSSoyoRRRRRo...oooo", //
            "........oSSSSSoooRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            ".......okkkkkko.okkkkkko......", //
            ".......okkkkkko.okkkkkko......", //
        ],
        &[
            "..............................", //
            "..............ooo.............", //
            "............ookkkoo...........", //
            "...........okkkkkkko..........", //
            "..........okkkkkkkkko.........", //
            "..........okkkkkkkkko.........", //
            "..........obbbbkyyyyo.........", //
            "..........okkkkkkkkko.........", //
            "..........okkkkkkkkko.........", //
            "..........owkkkkkkkyo.........", //
            "......oooowiiikkkgggyoooo.....", //
            ".....ossswiiiiiggggggyrrro....", //
            ".....osssiiiiiigggggggrrro....", //
            "....ossssiiiiiikggggggrrrro...", //
            "....ossssiiiiiykygggggrrrro...", //
            "....osssiiiiiyykyygggggrrro...", //
            "...ossssiiiiyyykyyyggggrrrro..", //
            "...ossssiiiiyyWkWyyggggrrrro..", //
            "...ossssiiiiyyWkWyyggggrrrro..", //
            "..ossssiiiiiyyWkWyygggggrrrro.", //
            "..ossssiiiiiyyykyyygggggrrrro.", //
            "..ossssiiiiiiyykyyggggggrrrro.", //
            "..osssowiiiiiiykyggggggrorrro.", //
            ".ossssowiiiiiiikgggggggrorrrro", //
            ".ossssoowiiiiiigggggggroorrrro", //
            "oiiiio..owiiiiiggggggro..ogggg", //
            "oiiiio...owiiiigggggro...ogggg", //
            "oiiiio..oSSiiisgrgggRRo..ogggg", //
            ".oooo...oSSSSSoyoRRRRRo...oooo", //
            "........oSSSSSoooRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            "........oSSSSSo.oRRRRRo.......", //
            ".......okkkkkko.okkkkkko......", //
            ".......okkkkkko.okkkkkko......", //
        ],
    ],
    frame_seconds: 0.50,
};

/// A ringhopper: a lilac puff on long legs with one long ear (standing / mid-hop).
pub const RINGHOPPER: SpriteDef = SpriteDef {
    name: "pd_enemy_ringhopper",
    legend: &[
        ('o', INK),
        ('l', LAVENDER),
        ('w', PINK),
        ('p', PLUM),
        ('k', INK),
    ],
    frames: &[
        &[
            ".......opo", //
            ".......opo", //
            "...ooooopo", //
            "..owwwwwkw", //
            ".owlllllpp", //
            ".owllllpoo", //
            "..oppppo..", //
            "..opoopo..", //
            ".oppoopo..", //
            ".opo.opo..", //
        ],
        &[
            ".......opo", //
            "...oooowwo", //
            "..owwwwlkw", //
            ".owlllllpo", //
            ".owpllppo.", //
            "..opwppo..", //
            ".oppoopo..", //
            ".opo.opo..", //
            "oppo.opo..", //
            ".oo..opo..", //
        ],
    ],
    frame_seconds: 0.22,
};

/// A shardback: a stone-grey grazer under a ridge of ring-ice crystals (stride A / B).
pub const SHARDBACK: SpriteDef = SpriteDef {
    name: "pd_enemy_shardback",
    legend: &[
        ('o', INK),
        ('b', STONE_LIGHT),
        ('f', FOG),
        ('B', STONE),
        ('c', SKY_LIGHT),
        ('i', ICE),
        ('C', SKY),
        ('k', INK),
    ],
    frames: &[
        &[
            "....o...o...o...o.....", //
            "...oio.oio.oio.oio....", //
            "...oio.oio.oio.oio....", //
            "...oiiooiiooiiooiio...", //
            "..oicCoicCoicCoicCo...", //
            "..oiccfcccfcccfccCo...", //
            "..offbbbbbbbbbbbffoo..", //
            ".ofbbbbbbbbbbbbbbbffoo", //
            ".ofbbbbbbbbbbbbbbbbbkf", //
            ".ofbbbbbbbbbbbbbbbbbbB", //
            "..ofbbbbbbbbbbbbbbBbBB", //
            "..oBBBBBbbBBbbBBBBofoo", //
            "..oBBoooBBooBBooBBoo..", //
            "..oBBo.oBBooBBooBBo...", //
        ],
        &[
            "....o...o...o...o.....", //
            "...oio.oio.oio.oio....", //
            "...oio.oio.oio.oio....", //
            "...oiiooiiooiiooiio...", //
            "..oicCoicCoicCoicCo...", //
            "..oiccfcccfcccfccCo...", //
            "..offbbbbbbbbbbbffoo..", //
            ".ofbbbbbbbbbbbbbbbffoo", //
            ".ofbbbbbbbbbbbbbbbbbkf", //
            ".ofbbbbbbbbbbbbbbbbbbB", //
            "..ofbbbbbbbbbbbbbBBbBB", //
            "..oBBbBbbBBBBbbbBoofoo", //
            "...oBBoBBooooBBBBo.o..", //
            "...oBBoBBo..oBBBBo....", //
        ],
    ],
    frame_seconds: 0.40,
};

/// A tether drone: a hexagonal builder machine with a mint lens (dim / glowing to fire).
pub const TETHER_DRONE: SpriteDef = SpriteDef {
    name: "pd_enemy_tether_drone",
    legend: &[
        ('o', INK),
        ('m', STEEL),
        ('L', STEEL_LIGHT),
        ('M', STEEL_DARK),
        ('g', MINT),
        ('G', TEAL),
        ('w', BONE),
        ('y', SKY_LIGHT),
    ],
    frames: &[
        &[
            "...oMMMo....", //
            "...ooooooo..", //
            "..oLLLLLLLo.", //
            "..oLmmmmmmLo", //
            ".oLmmmmGGGMo", //
            ".oLmmmmGgGmL", //
            ".oLmmmmGGGMo", //
            "..oLmmmmmmMo", //
            "..oLMMMMMMo.", //
            "...oMMMooo..", //
        ],
        &[
            "...oMMMo....", //
            "...ooooooo..", //
            "..oLLLLLLLo.", //
            "..oLmmmmmmLo", //
            "ooLmmmmgggMo", //
            "yoLmmmmgwgmL", //
            "ooLmmmmgggMo", //
            "..oLmmmmmmMo", //
            "..oLMMMMMMo.", //
            "...oMMMooo..", //
        ],
    ],
    frame_seconds: 0.30,
};

/// A void mite: a spiny dark burr with a violet eye (its spines turn).
pub const VOID_MITE: SpriteDef = SpriteDef {
    name: "pd_enemy_void_mite",
    legend: &[('o', INK), ('p', PLUM), ('v', VIOLET), ('l', LAVENDER)],
    frames: &[
        &[
            "po....op", //
            "opoooopo", //
            ".oppppo.", //
            ".oppppo.", //
            ".opplpo.", //
            ".oppppo.", //
            "opoooopo", //
            "po....op", //
        ],
        &[
            "...opo..", //
            "..oopo..", //
            ".oppppo.", //
            "oopvppoo", //
            "pppplppp", //
            "ooppppoo", //
            "..oopo..", //
            "...opo..", //
        ],
    ],
    frame_seconds: 0.14,
};

/// The Ring Warden: a builder ring of steel with mint lights round a teal core on four struts
/// (idle; telling: lights and core white-hot; venting: segments open, the core bare and gold).
pub const RING_WARDEN: SpriteDef = SpriteDef {
    name: "pd_enemy_ring_warden",
    legend: &[
        ('o', INK),
        ('m', STEEL),
        ('L', STEEL_LIGHT),
        ('M', STEEL_DARK),
        ('g', MINT),
        ('w', BONE),
        ('c', TEAL),
        ('T', TEAL_DARK),
        ('y', GOLD),
    ],
    frames: &[
        &[
            "............oooooooooooo............", //
            "..........ooLLLLLLLLLLLLoo..........", //
            "........ooLLmmmmmmmmmmmmLLoo........", //
            ".......oLLmmmmmmmgmmmmmmmmLLo.......", //
            "......oLmmmmmmmmmmmmmmmmmmmmLo......", //
            ".....oLmmmgmmmmMMMMMMmmmmgmmmLo.....", //
            "....oLmmmmmmMMMoooMooLMMmmmmmmLo....", //
            "...oLmmmmmmMooo..oMo.oooLmmmmmmLo...", //
            "..oLmmmmmmMo.....oMo....oLmmmmmmLo..", //
            "..oLmmmmmMo......oMo.....oLmmmmmMo..", //
            ".oLmmgmmMo.......oMo......oLmmgmmLo.", //
            ".oLmmmmMo.......ooMo.......oLmmmmMo.", //
            ".oLmmmmMo.....ooggggoo.....oLmmmmMo.", //
            "oLmmmmMo.....oggccccggo.....oLmmmmLo", //
            "oLmmmmMo.....ogccccccTo.....oLmmmmMo", //
            "oLmmmmMoooooogcccggcccgooooooLmmmmMo", //
            "oLmgmmMMMMMMMgccggggccTMMMMMMLmmgmMo", //
            "oLmmmmMoooooogccggggccTooooooLmmmmMo", //
            "oLmmmmMo....ogcccggcccTo....oLmmmmMo", //
            "oLmmmmMo.....ogccccccTo.....oLmmmmMo", //
            "oLmmmmMo.....ogTccccTTo.....oLmmmmMo", //
            ".oLmmmmLo.....oogTTToo.....oLmmmmMo.", //
            ".oLmmmmMo.......ooMo.......oLmmmmMo.", //
            ".oLmmgmmLo.......oMo......oLmmgmmMo.", //
            "..oLmmmmmLo......oMo.....oLmmmmmMo..", //
            "..oLmmmmmmLo.....oMo....oLmmmmmmMo..", //
            "...oLmmmmmmLooo..oMo.oooLmmmmmmMo...", //
            "....oLmmmmmmLLLoooMooLLLmmmmmmMo....", //
            ".....oLmmmgmmmmLLLMLLmmmmgmmmMo.....", //
            "......oLmmmmmmmmmmmmmmmmmmmmMo......", //
            ".......oLMmmmmmmmmgmmmmmmmMMo.......", //
            "........ooLMmmmmmmmmmmmmMMoo........", //
            "..........ooLMMMMMMMMMMMoo..........", //
            "............oooooooooooo............", //
        ],
        &[
            "............oooooooooooo............", //
            "..........ooLLLLLLLLLLLLoo..........", //
            "........ooLLmmmmmmmmmmmmLLoo........", //
            ".......oLLmmmmmmmwmmmmmmmmLLo.......", //
            "......oLmmmmmmmmmmmmmmmmmmmmLo......", //
            ".....oLmmmwmmmmMMMMMMmmmmwmmmLo.....", //
            "....oLmmmmmmMMMoooMooLMMmmmmmmLo....", //
            "...oLmmmmmmMooo..oMo.oooLmmmmmmLo...", //
            "..oLmmmmmmMo.....oMo....oLmmmmmmLo..", //
            "..oLmmmmmMo......oMo.....oLmmmmmMo..", //
            ".oLmmwmmMo.......oMo......oLmmwmmLo.", //
            ".oLmmmmMo.......ooMo.......oLmmmmMo.", //
            ".oLmmmmMo.....ooggggoo.....oLmmmmMo.", //
            "oLmmmmMo.....oggccccggo.....oLmmmmLo", //
            "oLmmmmMo.....ogccwwccTo.....oLmmmmMo", //
            "oLmmmmMoooooogccwwwwccgooooooLmmmmMo", //
            "oLmwmmMMMMMMMgcwwwwwwcTMMMMMMLmmwmMo", //
            "oLmmmmMoooooogcwwwwwwcTooooooLmmmmMo", //
            "oLmmmmMo....ogccwwwwccTo....oLmmmmMo", //
            "oLmmmmMo.....ogccwwccTo.....oLmmmmMo", //
            "oLmmmmMo.....ogTccccTTo.....oLmmmmMo", //
            ".oLmmmmLo.....oogTTToo.....oLmmmmMo.", //
            ".oLmmmmMo.......ooMo.......oLmmmmMo.", //
            ".oLmmwmmLo.......oMo......oLmmwmmMo.", //
            "..oLmmmmmLo......oMo.....oLmmmmmMo..", //
            "..oLmmmmmmLo.....oMo....oLmmmmmmMo..", //
            "...oLmmmmmmLooo..oMo.oooLmmmmmmMo...", //
            "....oLmmmmmmLLLoooMooLLLmmmmmmMo....", //
            ".....oLmmmwmmmmLLLMLLmmmmwmmmMo.....", //
            "......oLmmmmmmmmmmmmmmmmmmmmMo......", //
            ".......oLMmmmmmmmmwmmmmmmmMMo.......", //
            "........ooLMmmmmmmmmmmmmMMoo........", //
            "..........ooLMMMMMMMMMMMoo..........", //
            "............oooooooooooo............", //
        ],
        &[
            "............oooooooooooo............", //
            "..........ooLLLLLLLLLLLLoo..........", //
            "........ooLLmmmmmoommmmmLLoo........", //
            ".......oLLmmmmmmo..ommmmmmLLo.......", //
            "......oLmmmmmmmmmoommmmmmmmmLo......", //
            ".....oLmmmgmmmmMMMMMMmmmmgmmmLo.....", //
            "....oLmmmmmmMMMoooMooLMMmmmmmmLo....", //
            "...oLmmmmmmMooo..oMo.oooLmmmmmmLo...", //
            "..oLmmmmmmMo.....oMo....oLmmmmmmLo..", //
            "..oLmmmmmMo......oMo.....oLmmmmmMo..", //
            ".oLmmgmmMo.....oooMoo.....oLmmgmmLo.", //
            ".oLmmmmMo.....ogggMggo.....oLmmmmMo.", //
            ".oLmmmmMo....ogccccccgo....oLmmmmMo.", //
            "oLmmmmMo....ogccccccccgo....oLmmmmLo", //
            "oLmmmmMo...ogcccyyyycccgo...oLmmmmMo", //
            "oLmommMooooogccyyyyyyccToooooLmmomMo", //
            "oLo.omMMMMMMMccyywwyyccMMMMMMLmo.oMo", //
            "oLo.omMooooogccyywwyyccToooooLmo.oMo", //
            "oLmommMo...ogccyyyyyyccTo...oLmmomMo", //
            "oLmmmmMo...ogcccyyyycccTo...oLmmmmMo", //
            "oLmmmmMo....ogccccccccTo....oLmmmmMo", //
            ".oLmmmmLo....ogccccccTo....oLmmmmMo.", //
            ".oLmmmmMo.....ogTTMTTo.....oLmmmmMo.", //
            ".oLmmgmmLo.....oooMoo.....oLmmgmmMo.", //
            "..oLmmmmmLo......oMo.....oLmmmmmMo..", //
            "..oLmmmmmmLo.....oMo....oLmmmmmmMo..", //
            "...oLmmmmmmLooo..oMo.oooLmmmmmmMo...", //
            "....oLmmmmmmLLLoooMooLLLmmmmmmMo....", //
            ".....oLmmmgmmmmLLLMLLmmmmgmmmMo.....", //
            "......oLmmmmmmmmmoommmmmmmmmMo......", //
            ".......oLMmmmmmmo..ommmmmmMMo.......", //
            "........ooLMmmmmmoommmmmMMoo........", //
            "..........ooLMMMMMMMMMMMoo..........", //
            "............oooooooooooo............", //
        ],
    ],
    frame_seconds: 0.40,
};
