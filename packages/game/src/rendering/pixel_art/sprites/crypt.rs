//! Top-down crypt creatures and trap shots (WP-TOMBS T29), seen from above with a little of
//! their front showing (the crypt's 3/4 view).

use super::super::{palette::*, sprite::SpriteDef};

pub const CRYPT_SKELETON: SpriteDef = SpriteDef {
    name: "crypt_skeleton",
    legend: &[
        ('o', INK),
        ('w', BONE),
        ('g', FOG),
        ('r', RED),
        ('s', STEEL_LIGHT),
    ],
    frames: &[
        &[
            "....oooooo....", //
            "...owwwwwwo...", //
            "...owrwwrwo...", //
            "...owwwwwwo...", //
            "....owggwo....", //
            "..ooowwwwooo..", //
            ".owgowgwgowgo.", //
            "sowwowwwwowwo.", //
            "sowgowgwgowgo.", //
            "s.oo.owwo.oo..", //
            "s....owwo.....", //
            ".....oggo.....", //
            "....owoowo....", //
            "....oo..oo....", //
        ],
        &[
            "....oooooo....", //
            "...owwwwwwo...", //
            "...owrwwrwo...", //
            "...owwwwwwo...", //
            "....owggwo....", //
            "..ooowwwwooo..", //
            ".owgowgwgowgo.", //
            ".owwowwwwowwos", //
            ".owgowgwgowgos", //
            "..oo.owwo.oo.s", //
            ".....owwo....s", //
            ".....oggo.....", //
            ".....owoowo...", //
            ".....oo..oo...", //
        ],
    ],
    frame_seconds: 0.22,
};

pub const SPECTRE: SpriteDef = SpriteDef {
    name: "crypt_spectre",
    legend: &[
        ('o', SLATE),
        ('m', MIST),
        ('f', FOG),
        ('l', LAVENDER),
        ('e', INK),
        ('c', MINT),
    ],
    frames: &[
        &[
            ".....oooo.....", //
            "...oommmmoo...", //
            "..ommmmmmmmo..", //
            "..omeemmeemo..", //
            "..omecmmcemo..", //
            "..ommmmmmmmo..", //
            "..omffeeffmo..", //
            ".olmmmmmmmmlo.", //
            "olmmmmmmmmmmlo", //
            "olfmmmmmmmmflo", //
            ".ofmmmmmmmmfo.", //
            "..offmmmmffo..", //
            "..of.offo.fo..", //
            "...o..oo..o...", //
            "..............", //
            "..............", //
        ],
        &[
            "..............", //
            ".....oooo.....", //
            "...oommmmoo...", //
            "..ommmmmmmmo..", //
            "..omeemmeemo..", //
            "..omecmmcemo..", //
            "..ommmmmmmmo..", //
            "..omffeeffmo..", //
            "olmmmmmmmmmmlo", //
            "olmmmmmmmmmmlo", //
            ".ofmmmmmmmmfo.", //
            "..offmmmmffo..", //
            "...ofoffofo...", //
            "...o.o..o.o...", //
            "..............", //
            "..............", //
        ],
    ],
    frame_seconds: 0.3,
};

pub const CULTIST: SpriteDef = SpriteDef {
    name: "crypt_cultist",
    legend: &[
        ('o', INK),
        ('p', PLUM),
        ('v', VIOLET),
        ('s', SOIL_DARK),
        ('y', AMBER),
        ('g', GOLD),
        ('e', ROSE),
    ],
    frames: &[
        &[
            ".....oooo.....", //
            "....ovvvvo....", //
            "...ovppppvo...", //
            "...ovpeepvo...", //
            "...ovpsspvo...", //
            "..oovvppvvoo..", //
            ".oyovvvvvvoyo.", //
            ".ogovpvvpvogo.", //
            "..oovpvvpvoo..", //
            "...ovpvvpvo...", //
            "...ovpvvpvo...", //
            "..ovvpvvpvvo..", //
            "..ovvvvvvvvo..", //
            "...oooooooo...", //
        ],
        &[
            ".....oooo.....", //
            "....ovvvvo....", //
            "...ovppppvo...", //
            "...ovpeepvo...", //
            "...ovpsspvo...", //
            "..oovvppvvoo..", //
            ".ogovvvvvvogo.", //
            ".oyovpvvpvoyo.", //
            "..oovpvvpvoo..", //
            "...ovpvvpvo...", //
            "..ovvpvvpvvo..", //
            "..ovpvvvvpvo..", //
            "...ovvvvvvo...", //
            "...oooooooo...", //
        ],
    ],
    frame_seconds: 0.25,
};

pub const CRYPT_GOLEM: SpriteDef = SpriteDef {
    name: "crypt_golem",
    legend: &[
        ('o', INK),
        ('d', STONE_DARK),
        ('s', STONE),
        ('l', STONE_LIGHT),
        ('c', MINT),
        ('m', MOSS),
    ],
    frames: &[
        &[
            "......oooooooooo......", //
            ".....ollllllllllo.....", //
            ".....olsssssssslo.....", //
            ".....olsccsssccso.....", //
            ".....olsssssssssd.....", //
            "..oooodssddddssdooooo.", //
            ".olllloossssssoollllo.", //
            ".olsssoslllllssossslo.", //
            ".olsssoslmllllsosssso.", //
            ".olssdoslllllmsodsslo.", //
            ".odssdosllllllsodssdo.", //
            ".odddooslllllssooddo..", //
            "..ooo.osssssssso.ooo..", //
            "......odssssssdo......", //
            "......osssooosso......", //
            ".....olsso..osslo.....", //
            ".....olsdo..odslo.....", //
            ".....oddo....oddo.....", //
            ".....oooo....oooo.....", //
            "......................", //
        ],
        &[
            "......oooooooooo......", //
            ".....ollllllllllo.....", //
            ".....olsssssssslo.....", //
            ".....olsccsssccso.....", //
            ".....olsssssssssd.....", //
            ".oooooossddddssdooooo.", //
            ".olllloossssssoollllo.", //
            ".olsssoslllllssossslo.", //
            ".olsssoslmllllsosssso.", //
            ".olssdoslllllmsodsslo.", //
            ".odssdosllllllsodssdo.", //
            "..oddooslllllssooddo..", //
            "...ooo.osssssssooo....", //
            "......odssssssdo......", //
            "......osssooosso......", //
            "......olsso.osslo.....", //
            "......olsdo.odslo.....", //
            "......oddo..oddo......", //
            "......oooo..oooo......", //
            "......................", //
        ],
    ],
    frame_seconds: 0.4,
};

pub const CRYPT_LORD: SpriteDef = SpriteDef {
    name: "crypt_lord",
    legend: &[
        ('o', INK),
        ('g', GOLD),
        ('a', AMBER),
        ('w', BONE),
        ('f', FOG),
        ('r', RED),
        ('m', MAROON),
        ('p', PLUM),
        ('v', VIOLET),
        ('t', TEAL),
        ('c', MINT),
    ],
    frames: &[
        &[
            "..........o..o..o.........", //
            ".........ogoogoogo........", //
            ".........ogggggggo........", //
            "........owwwwwwwwwo.......", //
            "........owrrwwwrrwo.......", //
            "........owwwwfwwwwo.......", //
            "........oowfwfwfwoo.......", //
            "......oooowwfwfwwoooo.....", //
            "....oommmmoowwwwoommmmoo..", //
            "...ommmpppmmooooommpppmmo.", //
            "..ommpppppmmmmmmmmmpppppmo", //
            "..omppvvpppmmggmmmppvvppmo", //
            ".comppvvppmmmgamgmmppvvpmo", //
            "ocwomppppmmmmmgmmmmmppppmo", //
            "ocwomppppmmmmmmmmmmmppppmo", //
            "ocwoommppmmmmmmmmmmmppmmo.", //
            ".otoo.ommmpmmmmmmmpmmmo...", //
            "..to..ommmpmmmmmmmpmmmo...", //
            "..to..ommmpmmmmmmmpmmmo...", //
            "..to.ommmmpmmmmmmmpmmmmo..", //
            "..to.ommmppmmmmmmmppmmmo..", //
            "..to.ommppmmmmmmmmmppmmo..", //
            "..to.oommmmmmmmmmmmmmmoo..", //
            "..to..ooooooooooooooooo...", //
            "..oo......................", //
            "..........................", //
        ],
        &[
            "..........o..o..o.........", //
            ".........ogoogoogo........", //
            ".........ogggggggo........", //
            "........owwwwwwwwwo.......", //
            "........owrrwwwrrwo.......", //
            "........owwwwfwwwwo.......", //
            "........oowfwfwfwoo.......", //
            "......oooowwfwfwwoooo.....", //
            "....oommmmoowwwwoommmmoo..", //
            "...ommmpppmmooooommpppmmo.", //
            "..ommpppppmmmmmmmmmpppppmo", //
            "..omppvvpppmmggmmmppvvppmo", //
            ".comppvvppmmmgamgmmppvvpmo", //
            "occwmppppmmmmmgmmmmmppppmo", //
            "ocwcmppppmmmmmmmmmmmppppmo", //
            "occwommppmmmmmmmmmmmppmmo.", //
            ".otoo.ommmpmmmmmmmpmmmo...", //
            "..to..ommmpmmmmmmmpmmmo...", //
            "..to.ommmmpmmmmmmmpmmmmo..", //
            "..to.ommmmpmmmmmmmpmmmmo..", //
            "..to.ommmppmmmmmmmppmmmo..", //
            "..to.ommppmmmmmmmmmppmmmo.", //
            "..to.oommmmmmmmmmmmmmmmoo.", //
            "..to..oooooooooooooooooo..", //
            "..oo......................", //
            "..........................", //
        ],
    ],
    frame_seconds: 0.35,
};

pub const SHOT_HEX: SpriteDef = SpriteDef {
    name: "crypt_shot_hex",
    legend: &[('o', PLUM), ('v', LAVENDER), ('w', PINK)],
    frames: &[
        &[
            ".oo.", //
            "ovvo", //
            "ovwo", //
            ".oo.", //
        ],
        &[
            ".oo.", //
            "owvo", //
            "ovvo", //
            ".oo.", //
        ],
    ],
    frame_seconds: 0.08,
};

pub const SHOT_ARROW: SpriteDef = SpriteDef {
    name: "crypt_shot_arrow",
    legend: &[('w', BONE), ('s', STEEL_LIGHT), ('r', RED)],
    frames: &[&[
        "r.......", //
        "rrwwwwws", //
        "r.......", //
    ]],
    frame_seconds: 0.1,
};

pub const SHOT_ARROW_UP: SpriteDef = SpriteDef {
    name: "crypt_shot_arrow_up",
    legend: &[('w', BONE), ('s', STEEL_LIGHT), ('r', RED)],
    frames: &[&[
        ".s.", //
        ".w.", //
        ".w.", //
        ".w.", //
        ".w.", //
        ".w.", //
        ".r.", //
        "rrr", //
    ]],
    frame_seconds: 0.1,
};

pub const SHOT_BOULDER: SpriteDef = SpriteDef {
    name: "crypt_shot_boulder",
    legend: &[
        ('o', INK),
        ('d', STONE_DARK),
        ('s', STONE),
        ('l', STONE_LIGHT),
    ],
    frames: &[
        &[
            "....oooooo....", //
            "..oollllssoo..", //
            ".ollllsssssdo.", //
            ".olllssssssdo.", //
            "olllsssdssssdo", //
            "ollsssdsssssdo", //
            "olssssssssssdo", //
            "olsssssssdssdo", //
            "osssdsssssssdo", //
            "osssssssssddo.", //
            ".osssssssdddo.", //
            ".odsssssdddo..", //
            "..ooddddoo....", //
            "....oooo......", //
        ],
        &[
            "....oooooo....", //
            "..oossllllo...", //
            ".odssssslllo..", //
            ".odsdssssslo..", //
            "odssssssssllo.", //
            "odsssssdsssloo", //
            "odssssssssssso", //
            "odssdsssssssso", //
            "odsssssssdssso", //
            ".oddssssssssdo", //
            ".odddssssssdo.", //
            "..odddsssddo..", //
            "...ooddddoo...", //
            ".....oooo.....", //
        ],
    ],
    frame_seconds: 0.12,
};
