//! 9x9 icons for menu rows, building counters and lobby rosters (the rig-stat and mineral icons
//! live in [`super::icons`]). Every icon sits on one text line like the stat icons.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// Defines a single-frame 9x9 icon.
macro_rules! menu_icon {
    ($ident:ident, $name:literal, [$(($symbol:literal, $color:ident)),* $(,)?], [$($row:literal),* $(,)?]) => {
        pub const $ident: SpriteDef = SpriteDef {
            name: $name,
            legend: &[('o', INK), $(($symbol, $color)),*],
            frames: &[&[$($row),*]],
            frame_seconds: 0.0,
        };
    };
}

menu_icon!(
    HAMMER,
    "menu_hammer",
    [
        ('l', STEEL_LIGHT),
        ('m', STEEL),
        ('d', STEEL_DARK),
        ('s', SAND),
        ('a', LOAM)
    ],
    [
        ".ooooooo.",
        "olllllldo",
        "ommmmmmdo",
        ".ooosooo.",
        "...oso...",
        "...oso...",
        "...oao...",
        "...oao...",
        "...ooo...",
    ]
);

menu_icon!(
    CONTRACT,
    "menu_contract",
    [('w', BONE), ('m', MIST), ('k', SLATE_LIGHT), ('r', RED)],
    [
        ".ooooo...",
        ".owwwoo..",
        ".owwwwmo.",
        ".okkkkwo.",
        ".owwwwwo.",
        ".okkkwwo.",
        ".owwwrro.",
        ".okkwrro.",
        ".ooooooo.",
    ]
);

menu_icon!(
    RECEIPT,
    "menu_receipt",
    [('w', BONE), ('m', MIST), ('k', SLATE_LIGHT)],
    [
        ".ooooooo.",
        ".owwwwmo.",
        ".okkkwmo.",
        ".owwwwmo.",
        ".okkkkmo.",
        ".owwwwmo.",
        ".okkwwmo.",
        ".owmwmwo.",
        ".o.o.o.o.",
    ]
);

menu_icon!(
    SORT,
    "menu_sort",
    [('l', SKY_LIGHT), ('m', SKY), ('d', OCEAN)],
    [
        ".........",
        "ooooooooo",
        "olllllldo",
        ".ommmmdo.",
        "..ommdo..",
        "...omo...",
        "...omo...",
        "...odo...",
        "...ooo...",
    ]
);

menu_icon!(
    BOOK,
    "menu_book",
    [('r', VIOLET), ('R', LAVENDER), ('d', PLUM), ('w', BONE)],
    [
        ".ooooooo.",
        "odrrrrrro",
        "odrRRRrro",
        "odrrrrrro",
        "odrrrrrro",
        "odrrrrrro",
        "odrrrrrro",
        "odwwwwwwo",
        ".ooooooo.",
    ]
);

menu_icon!(
    LETTER,
    "menu_letter",
    [('w', BONE), ('m', MIST), ('r', RED)],
    [
        ".........",
        "ooooooooo",
        "oowwwwwoo",
        "owowwwowo",
        "owwormowo",
        "owwwmwwwo",
        "owwwwwwwo",
        "ommmmmmmo",
        "ooooooooo",
    ]
);

menu_icon!(
    BANK,
    "menu_bank",
    [('g', GOLD), ('a', AMBER), ('m', MIST)],
    [
        "....o....",
        "..oogoo..",
        ".oggggao.",
        "ooooooooo",
        ".omomomo.",
        ".omomomo.",
        ".omomomo.",
        "oggggggao",
        "ooooooooo",
    ]
);

menu_icon!(
    INSURANCE,
    "menu_insurance",
    [('l', LEAF), ('m', MOSS), ('w', BONE)],
    [
        ".ooooooo.",
        "ollllllmo",
        "olllllwmo",
        "ollllwlmo",
        "olwlwllmo",
        ".olwllmo.",
        ".ollllmo.",
        "..ommmo..",
        "...ooo...",
    ]
);

menu_icon!(
    ROCKET,
    "menu_rocket",
    [
        ('r', RED),
        ('w', BONE),
        ('m', MIST),
        ('b', SKY),
        ('g', GOLD),
        ('a', ORANGE)
    ],
    [
        "....o....",
        "...oro...",
        "..owwmo..",
        "..owbmo..",
        "..owwmo..",
        ".oowwmoo.",
        ".orowmoro",
        ".oo.g.oo.",
        "....a....",
    ]
);

menu_icon!(
    GIFT,
    "menu_gift",
    [('r', RED), ('m', MAROON), ('g', GOLD)],
    [
        "..o...o..",
        ".ogo.ogo.",
        "..ogogo..",
        "ooooooooo",
        "orrrgrrmo",
        "ooooooooo",
        ".orrgrmo.",
        ".orrgrmo.",
        ".ooooooo.",
    ]
);

menu_icon!(
    DRONE,
    "menu_drone",
    [('m', STEEL_LIGHT), ('d', STEEL), ('k', NIGHT), ('g', MINT)],
    [
        "ooo...ooo",
        "..o...o..",
        ".ooooooo.",
        "ommmmmmdo",
        "omkkgkkdo",
        ".odddddo.",
        "..o...o..",
        ".oo...oo.",
        ".........",
    ]
);

menu_icon!(
    SCRAP,
    "menu_scrap",
    [('r', RUST), ('c', COPPER), ('s', STEEL_LIGHT), ('d', STEEL)],
    [
        ".........",
        "...ooo...",
        "..ocrro..",
        ".ocrrrooo",
        ".orrrosso",
        "oorrossdo",
        "occoossdo",
        "orrrodddo",
        "ooooooooo",
    ]
);

menu_icon!(
    COLLAPSE,
    "menu_collapse",
    [('l', MIST), ('m', STONE_LIGHT), ('d', STONE),],
    [
        ".....ooo.",
        "....olmdo",
        "....ommdo",
        ".ooo.ooo.",
        "olmdo....",
        "ommdooooo",
        ".oooolmdo",
        "olmmommdo",
        "ooooooooo",
    ]
);

menu_icon!(
    HOUSE,
    "menu_house",
    [
        ('r', RED),
        ('m', MAROON),
        ('s', SAND),
        ('b', SKY),
        ('l', LOAM)
    ],
    [
        "....o....",
        "...oro...",
        "..orrmo..",
        ".orrrrmo.",
        "ooooooooo",
        ".osbssso.",
        ".ossslso.",
        ".ossslso.",
        ".ooooooo.",
    ]
);

menu_icon!(
    FLAG,
    "menu_flag",
    [('l', STEEL_LIGHT), ('r', RED), ('m', MAROON), ('w', ORANGE)],
    [
        "oo.......",
        "oloooooo.",
        "olwrrrrmo",
        "olrrrrrmo",
        "olrrrmooo",
        "olooo....",
        "olo......",
        "olo......",
        "ooo......",
    ]
);

menu_icon!(
    ANVIL,
    "menu_anvil",
    [('l', STEEL_LIGHT), ('m', STEEL), ('d', STEEL_DARK)],
    [
        ".........",
        "ooooooo..",
        "olllllmoo",
        ".ommmmmdo",
        "..odddoo.",
        "...omo...",
        "..ommdo..",
        ".ommmmdo.",
        ".ooooooo.",
    ]
);

menu_icon!(
    DISK,
    "menu_disk",
    [
        ('b', OCEAN),
        ('l', SKY),
        ('w', BONE),
        ('s', STEEL_LIGHT),
        ('k', NIGHT)
    ],
    [
        "oooooooo.",
        "olwwwwwlo",
        "obwwwwwbo",
        "obbbbbbbo",
        "obbbbbbbo",
        "obsskssbo",
        "obsskssbo",
        "obsssssbo",
        "ooooooooo",
    ]
);

menu_icon!(
    FOLDER,
    "menu_folder",
    [('a', AMBER), ('g', GOLD), ('c', CREAM), ('r', ORANGE)],
    [
        ".........",
        "oooo.....",
        "oaaaooooo",
        "oaaaaaaao",
        "occccccco",
        "ogggggggo",
        "ogggggggo",
        "orrrrrrro",
        "ooooooooo",
    ]
);

menu_icon!(
    PERSON,
    "menu_person",
    [('s', SAND), ('c', CLAY), ('a', SKY), ('d', OCEAN)],
    [
        "...ooo...",
        "..ossso..",
        "..ossco..",
        "..oscco..",
        "...ooo...",
        ".ooaaaoo.",
        "oaaaaaado",
        "oaaaaaddo",
        "ooooooooo",
    ]
);

menu_icon!(
    READY,
    "menu_ready",
    [('l', LEAF), ('m', MOSS), ('w', BONE)],
    [
        "..ooooo..",
        ".olllllo.",
        "olllllwmo",
        "ollllwlmo",
        "olwlwllmo",
        "ollwllmmo",
        ".olllmmo.",
        "..ooooo..",
        ".........",
    ]
);

menu_icon!(
    WAITING,
    "menu_waiting",
    [('m', MIST), ('d', FOG), ('k', SLATE_DARK)],
    [
        "..ooooo..",
        ".ommmmmo.",
        "ommmkmmdo",
        "ommmkmmdo",
        "ommmkkmdo",
        "ommmmmmdo",
        ".odddddo.",
        "..ooooo..",
        ".........",
    ]
);

menu_icon!(
    OFFLINE,
    "menu_offline",
    [('r', RED), ('a', ORANGE)],
    [
        "oo.....oo",
        "oao...oro",
        ".oao.oro.",
        "..oarro..",
        "...oro...",
        "..orrro..",
        ".oro.oro.",
        "oro...oro",
        "oo.....oo",
    ]
);

menu_icon!(
    CROWN,
    "menu_crown",
    [('g', GOLD), ('a', AMBER), ('r', RED), ('c', CREAM)],
    [
        ".........",
        ".o..o..o.",
        "ogoogoogo",
        "ocgggggao",
        "ogrgcgrao",
        "ogggggggo",
        "oaaaaaaao",
        "ooooooooo",
        ".........",
    ]
);

menu_icon!(
    SIGNAL,
    "menu_signal",
    [('l', SKY_LIGHT), ('m', SKY), ('c', ICE)],
    [
        "..lllll..",
        ".l.....l.",
        "l..mmm..l",
        "..m...m..",
        ".........",
        "...ooo...",
        "...oco...",
        "...ooo...",
        ".........",
    ]
);

menu_icon!(
    KEY,
    "menu_key",
    [('g', GOLD), ('a', AMBER), ('c', CREAM)],
    [
        ".........",
        ".ooo.....",
        "ogcao....",
        "og.goooo.",
        "ogaagggao",
        ".ooooooao",
        "......oo.",
        ".........",
        ".........",
    ]
);

menu_icon!(
    SPEAKER,
    "menu_speaker",
    [('m', STEEL_LIGHT), ('d', STEEL), ('l', SKY_LIGHT)],
    [
        "....o....",
        "...omo...",
        "ooommo.l.",
        "ommmmo..l",
        "ommddo..l",
        "ooodmo.l.",
        "...odo...",
        "....o....",
        ".........",
    ]
);

menu_icon!(
    SCREEN,
    "menu_screen",
    [('b', SKY), ('l', SKY_LIGHT), ('d', OCEAN), ('m', STEEL)],
    [
        "ooooooooo",
        "ollbbbbbo",
        "olbbbbbdo",
        "obbbbbddo",
        "obbbbdddo",
        "ooooooooo",
        "...omo...",
        "..ooooo..",
        ".........",
    ]
);

menu_icon!(
    PLAY,
    "menu_play",
    [('l', LIME), ('d', LEAF)],
    [
        "..oo.....",
        "..olo....",
        "..ollo...",
        "..olllo..",
        "..olllldo",
        "..oldddo.",
        "..oddo...",
        "..odo....",
        "..oo.....",
    ]
);

menu_icon!(
    DOOR,
    "menu_door",
    [('l', CLAY), ('m', LOAM), ('d', SOIL), ('g', GOLD)],
    [
        ".ooooooo.",
        ".ollllmo.",
        ".olmmmdo.",
        ".olmmmdo.",
        ".olmmgdo.",
        ".olmmmdo.",
        ".olmmmdo.",
        ".olmmmdo.",
        "ooooooooo",
    ]
);

menu_icon!(
    BACK,
    "menu_back",
    [('l', MIST), ('d', FOG)],
    [
        ".........",
        "...oo....",
        "..olo....",
        ".olloooo.",
        "ollllllo.",
        ".oddoooo.",
        "..odo....",
        "...oo....",
        ".........",
    ]
);

menu_icon!(
    GLOBE,
    "menu_globe",
    [('b', OCEAN), ('l', SKY), ('g', LEAF), ('m', MOSS)],
    [
        "..ooooo..",
        ".olbggbo.",
        "olggbbbbo",
        "obgggbbgo",
        "obbggbgmo",
        "obbbgbbbo",
        "obbbbbbmo",
        ".obbbbbo.",
        "..ooooo..",
    ]
);

menu_icon!(
    SPLIT,
    "menu_split",
    [('b', SKY), ('d', OCEAN), ('g', LEAF), ('m', MOSS)],
    [
        "ooooooooo",
        "obbbogggo",
        "obbbogggo",
        "obbdoggmo",
        "obddogmmo",
        "ooooooooo",
        "...ooo...",
        "..ooooo..",
        ".........",
    ]
);

menu_icon!(
    DICE,
    "menu_dice",
    [('w', BONE), ('m', MIST), ('p', RED)],
    [
        ".ooooooo.",
        "owwwwwwmo",
        "owpwwwpmo",
        "owwwwwwmo",
        "owwwpwwmo",
        "owwwwwwmo",
        "owpwwwpmo",
        "ommmmmmmo",
        ".ooooooo.",
    ]
);

menu_icon!(
    GEAR,
    "menu_gear",
    [('m', STEEL_LIGHT), ('d', STEEL), ('k', STEEL_DARK)],
    [
        "...ooo...",
        ".oomdmoo.",
        ".omdddko.",
        "omddoddko",
        "omdo.odko",
        "omddoddko",
        ".odddkko.",
        ".ookkkoo.",
        "...ooo...",
    ]
);
