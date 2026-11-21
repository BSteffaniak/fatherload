//! Side-view tomb creatures (WP-TOMBS T28): mummies, crypt bats, skeletal archers, animated
//! statues, the two guardians and their shots. Generated from hand-drawn rows (see
//! `enemies.rs` for the format); walkers face right.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// A mummy: bone-white wrappings shaded with sand, one ember eye, arms reaching right (shamble A/B, lurch).
pub const MUMMY: SpriteDef = SpriteDef {
    name: "enemy_mummy",
    legend: &[
        ('o', INK),
        ('b', BONE),
        ('c', CREAM),
        ('s', SAND),
        ('k', SOIL_DARK),
        ('e', AMBER),
    ],
    frames: &[
        &[
            ".....oooo.....", //
            "....obcbbo....", //
            "...obsbbcbo...", //
            "...obbkebeo...", //
            "...osbbbbbo...", //
            "....osbbbo....", //
            "...obbcbbboooo", //
            "..obsbbsbbbcbo", //
            "..obbcbbsbooo.", //
            "..obsbbbbbo...", //
            "..obbcsbbbo...", //
            "...obbbbcbo...", //
            "...obsbobbo...", //
            "...obbo.obo...", //
            "...obo..obbo..", //
            "...ooo...ooo..", //
        ],
        &[
            ".....oooo.....", //
            "....obcbbo....", //
            "...obsbbcbo...", //
            "...obbkebeo...", //
            "...osbbbbbo...", //
            "....osbbbo....", //
            "...obbcbbboooo", //
            "..obsbbsbbbcbo", //
            "..obbcbbsbooo.", //
            "..obsbbbbbo...", //
            "..obbcsbbbo...", //
            "...obbbbcbo...", //
            "....obsbbo....", //
            "....obobbo....", //
            "....obo.obo...", //
            "....ooo.ooo...", //
        ],
        &[
            "......oooo....", //
            ".....obcbbo...", //
            "....obsbbcbo..", //
            "....obbkebeo..", //
            "....osbbbbbo..", //
            ".....osbbboooo", //
            "...obbcbbbbcbo", //
            "..obsbbsbbbooo", //
            "..obbcbbsbbcbo", //
            "..obsbbbbbbooo", //
            "..obbcsbbbo...", //
            "...obbbbcbo...", //
            "..obsbo.obo...", //
            "..obbo...obo..", //
            ".obo......obo.", //
            ".ooo......ooo.", //
        ],
    ],
    frame_seconds: 0.22,
};

/// A crypt bat: plum leather wings, a violet body, red eyes and two bone fangs (flap up/down).
pub const CRYPT_BAT: SpriteDef = SpriteDef {
    name: "enemy_crypt_bat",
    legend: &[
        ('o', INK),
        ('w', PLUM),
        ('d', VIOLET),
        ('e', RED),
        ('f', BONE),
    ],
    frames: &[
        &[
            "o..........o", //
            "wo........ow", //
            "wwo.oooo.oww", //
            ".wwodeedoww.", //
            "..wwddddww..", //
            "....ofof....", //
            "............", //
        ],
        &[
            "............", //
            "....oooo....", //
            "..oodeedoo..", //
            ".owwddddwwo.", //
            "owww.ff.wwwo", //
            "ow........wo", //
            "o..........o", //
        ],
    ],
    frame_seconds: 0.1,
};

/// A skeletal archer: bleached bones, a red spark in one socket and a longbow held right (stand, walk, draw).
pub const SKELETAL_ARCHER: SpriteDef = SpriteDef {
    name: "enemy_skeletal_archer",
    legend: &[
        ('o', INK),
        ('b', BONE),
        ('g', FOG),
        ('k', NIGHT),
        ('e', RED),
        ('w', LOAM),
        ('s', STONE_LIGHT),
        ('a', STEEL_LIGHT),
    ],
    frames: &[
        &[
            "....oooo......", //
            "...obbbbo..w..", //
            "...okekbo..ow.", //
            "...obbbbo..os.", //
            "....okko...os.", //
            ".....oo....os.", //
            "...obbbbo..os.", //
            "..obgbgbo.obs.", //
            "..obbgbbbbbos.", //
            "...obgbgo..os.", //
            "....obbo...ow.", //
            "....obbo...w..", //
            "...obooobo....", //
            "...ob...bo....", //
            "...ob...bo....", //
            "...oo...oo....", //
        ],
        &[
            "....oooo......", //
            "...obbbbo..w..", //
            "...okekbo..ow.", //
            "...obbbbo..os.", //
            "....okko...os.", //
            ".....oo....os.", //
            "...obbbbo..os.", //
            "..obgbgbo.obs.", //
            "..obbgbbbbbos.", //
            "...obgbgo..os.", //
            "....obbo...ow.", //
            "....obbo...w..", //
            "...obooobo....", //
            "....ob..bo....", //
            "...ob....bo...", //
            "...oo....oo...", //
        ],
        &[
            "....oooo......", //
            "...obbbbo...w.", //
            "...okekbo...ow", //
            "...obbbbo...ow", //
            "....okko...s.w", //
            ".....oo...s..w", //
            "...obbbbo.s..w", //
            "..obgbgbobbbbw", //
            "..obbgbbsaaaaw", //
            "...obgbgo.s..w", //
            "....obbo...s.w", //
            "....obbo....ow", //
            "...obooobo..ow", //
            "...ob...bo..w.", //
            "...ob...bo....", //
            "...oo...oo....", //
        ],
    ],
    frame_seconds: 0.24,
};

/// An animated statue: weathered grey stone with a gold sash; its eyes are dark while it sleeps and burn amber awake (dormant, awake, strike with fists up).
pub const ANIMATED_STATUE: SpriteDef = SpriteDef {
    name: "enemy_animated_statue",
    legend: &[
        ('o', INK),
        ('d', STONE_DARK),
        ('s', STONE),
        ('l', STONE_LIGHT),
        ('g', GOLD),
        ('k', NIGHT),
        ('e', AMBER),
    ],
    frames: &[
        &[
            ".....oooo.....", //
            "....oollllo...", //
            "...olssslsso..", //
            "...oskssksdo..", //
            "...osssssdo...", //
            "....osgsdo....", //
            "..oolssssdoo..", //
            ".olslsssssdslo", //
            ".olsoslssdoslo", //
            ".olso.ssd.oslo", //
            ".odd.oggdo.ddo", //
            "..oo.osssdo.oo", //
            ".....oslsdo...", //
            "....osssssdo..", //
            "....osdo.sdo..", //
            "....osdo.sdo..", //
            "....osdo.sdo..", //
            "...oddoo.oddo.", //
            "...oooo...ooo.", //
        ],
        &[
            ".....oooo.....", //
            "....oollllo...", //
            "...olssslsso..", //
            "...osessesdo..", //
            "...osssssdo...", //
            "....osgsdo....", //
            "..oolssssdoo..", //
            ".olslsssssdslo", //
            ".olsoslssdoslo", //
            ".olso.ssd.oslo", //
            ".odd.oggdo.ddo", //
            "..oo.osssdo.oo", //
            ".....oslsdo...", //
            "....osssssdo..", //
            "....osdo.sdo..", //
            "....osdo.sdo..", //
            "....osdo.sdo..", //
            "...oddoo.oddo.", //
            "...oooo...ooo.", //
        ],
        &[
            ".....oooo.....", //
            "....oollllo...", //
            "o..olssslsso.o", //
            "lo.osessesdoo.", //
            "sloosssssdols.", //
            ".sloosgsdools.", //
            "..olssssssdo..", //
            "...osssssdo...", //
            "...oslssdoo...", //
            "....ossdo.....", //
            "....oggdo.....", //
            "....osssdo....", //
            ".....oslsdo...", //
            "....osssssdo..", //
            "...osdo..sdo..", //
            "...osdo..sdo..", //
            "..osdo....sdo.", //
            "..oddoo...oddo", //
            "..oooo.....ooo", //
        ],
    ],
    frame_seconds: 0.3,
};

/// The Sunwrought Colossus: a sandstone giant with a gold crown and a burning sun set in its chest (stand, stride, crouch to leap).
/// 21 art pixels wide, so drawn half as large again it spans its 64-pixel contact box exactly and
/// never overlaps a pilot standing beside it (Wave 6); the crouch keeps its feet on the floor.
pub const SUN_COLOSSUS: SpriteDef = SpriteDef {
    name: "enemy_sun_colossus",
    legend: &[
        ('o', INK),
        ('d', SOIL),
        ('c', CLAY),
        ('s', SAND),
        ('l', CREAM),
        ('g', GOLD),
        ('a', AMBER),
    ],
    frames: &[
        &[
            "........ooooo........", //
            ".......ogggggo.......", //
            ".......oggaggo.......", //
            ".......ossssso.......", //
            "......oscasacso......", //
            "......ossssssso......", //
            ".......ocsssco.......", //
            "...ooooocccccooooo...", //
            "..osllssscccsssllso..", //
            ".osllssooogooossllso.", //
            ".osssssogaaagossssso.", //
            "oscssosgaalaagsosscso", //
            "oscssosgaaaaagsosscso", //
            "occoossogggggossoocco", //
            "occo.ossssssssso.occo", //
            "oddo.ossssssssso.oddo", //
            "ollloocccccccccoolllo", //
            ".oooosssssosssssoooo.", //
            ".....ossso.ossso.....", //
            ".....oscso.oscso.....", //
            ".....ossso.ossso.....", //
            ".....oscso.oscso.....", //
            "....occcco.occcco....", //
            "....oddddo.oddddo....", //
            "...oddddddoddddddo...", //
            "...ooooooooooooooo...", //
        ],
        &[
            "........ooooo........", //
            ".......ogggggo.......", //
            ".......oggaggo.......", //
            ".......ossssso.......", //
            "......oscasacso......", //
            "......ossssssso......", //
            ".......ocsssco.......", //
            "...ooooocccccooooo...", //
            "..osllssscccsssllso..", //
            ".osllssooogooossllso.", //
            ".osssssogaaagossssso.", //
            "oscssosgaalaagsosscso", //
            "oscssosgaaaaagsosscso", //
            "occoossogggggossoocco", //
            "occo.ossssssssso.occo", //
            "oddo.ossssssssso.oddo", //
            "ollloocccccccccoolllo", //
            ".oooosssssosssssoooo.", //
            "....ossso...ossso....", //
            "....oscso...oscso....", //
            "...ossso.....ossso...", //
            "...oscso.....oscso...", //
            "..occcco.....occcco..", //
            "..oddddo.....oddddo..", //
            ".oddddddo...oddddddo.", //
            ".oooooooo...oooooooo.", //
        ],
        &[
            ".....................", //
            ".....................", //
            ".....................", //
            ".....................", //
            ".....................", //
            "........ooooo........", //
            ".......ogggggo.......", //
            ".......oggaggo.......", //
            ".......ossssso.......", //
            "......oscasacso......", //
            "......ossssssso......", //
            "...ooooocccccooooo...", //
            ".osllssooogooossllso.", //
            ".osssssogaaagossssso.", //
            "oscssosgaalaagsosscso", //
            "oscssosgaaaaagsosscso", //
            "occoossogggggossoocco", //
            "occo.ossssssssso.occo", //
            "oddo.ossssssssso.oddo", //
            "ollloocccccccccoolllo", //
            ".oooosssssosssssoooo.", //
            "...osssso...osssso...", //
            "..osscso.....oscsso..", //
            "..occcco.....occcco..", //
            ".oddddddo...oddddddo.", //
            ".oooooooo...oooooooo.", //
        ],
    ],
    frame_seconds: 0.4,
};

/// The Hollow Matriarch: a floating wraith queen in tattered plum robes, a bone crown and mint eyes, hex orbs in her hands (drift A/B, cast with her hands raised).
pub const HOLLOW_MATRIARCH: SpriteDef = SpriteDef {
    name: "enemy_hollow_matriarch",
    legend: &[
        ('o', INK),
        ('b', BONE),
        ('m', MINT),
        ('k', NIGHT),
        ('p', PLUM),
        ('v', VIOLET),
        ('l', LAVENDER),
    ],
    frames: &[
        &[
            ".....o...oo...o.......", //
            ".....bo.obbo.ob.......", //
            "......bobbbbob........", //
            ".......obbbbbo........", //
            "......obbmbbmbo.......", //
            "......obbbbbbbo.......", //
            ".......obkkkbo........", //
            "....oooopbbbpoooo.....", //
            "...oppvvpppppvvppo....", //
            "..opvvppvvpvvppvvpo...", //
            ".opvpo.opvvvpo.opvpo..", //
            "oppo..opvvvvvpo..oppo.", //
            "obbo..opvvpvvpo..obbo.", //
            "olllo.opvvvvvpo.olllo.", //
            ".ooo..opvvpvvpo..ooo..", //
            "......opvvvvvvpo......", //
            ".....opvvpvvpvvpo.....", //
            ".....opvvvvvvvvpo.....", //
            "....opvvpvvpvvpvpo....", //
            "....opvpvvpvvpvvpo....", //
            "...opvpo.pvvp.opvpo...", //
            "...opo..opvvpo..opo...", //
            "...oo....opo.....oo...", //
            "..........o...........", //
        ],
        &[
            ".....o...oo...o.......", //
            ".....bo.obbo.ob.......", //
            "......bobbbbob........", //
            ".......obbbbbo........", //
            "......obbmbbmbo.......", //
            "......obbbbbbbo.......", //
            ".......obkkkbo........", //
            "....oooopbbbpoooo.....", //
            "...oppvvpppppvvppo....", //
            "..opvvppvvpvvppvvpo...", //
            ".opvpo.opvvvpo.opvpo..", //
            "oppo..opvvvvvpo..oppo.", //
            "obbo..opvvpvvpo..obbo.", //
            "olllo.opvvvvvpo.olllo.", //
            ".ooo..opvvpvvpo..ooo..", //
            "......opvvvvvvpo......", //
            "......opvpvvpvvpo.....", //
            ".....opvvvvvvvvpo.....", //
            ".....opvvpvvpvvpvpo...", //
            "....opvvpvvpvvpvpo....", //
            "....opvpo.pvvp.opvo...", //
            "....opo..opvvpo.opo...", //
            "....oo....opo....oo...", //
            "...........o..........", //
        ],
        &[
            "olllo.o...oo...o.olllo", //
            "obbo..bo.obbo.ob..obbo", //
            ".opo...bobbbbob...opo.", //
            ".opo....obbbbbo...opo.", //
            "..opo..obbmbbmbo.opo..", //
            "..opo..obbbbbbbo.opo..", //
            "...opo..obkkkbo.opo...", //
            "....oooopbbbpoooo.....", //
            "...oppvvpppppvvppo....", //
            "..opvvppvvpvvppvvpo...", //
            "...opo.opvvvpo..opo...", //
            "......opvvvvvpo.......", //
            "......opvvpvvpo.......", //
            "......opvvvvvpo.......", //
            "......opvvpvvpo.......", //
            "......opvvvvvvpo......", //
            ".....opvvpvvpvvpo.....", //
            ".....opvvvvvvvvpo.....", //
            "....opvvpvvpvvpvpo....", //
            "....opvpvvpvvpvvpo....", //
            "...opvpo.pvvp.opvpo...", //
            "...opo..opvvpo..opo...", //
            "...oo....opo.....oo...", //
            "..........o...........", //
        ],
    ],
    frame_seconds: 0.3,
};

/// A skeletal archer's arrow, flying right: a steel head, a wooden shaft and bone fletching.
pub const SHOT_ARROW: SpriteDef = SpriteDef {
    name: "enemy_shot_arrow",
    legend: &[('o', INK), ('w', LOAM), ('f', BONE), ('h', STEEL_LIGHT)],
    frames: &[&[
        "f.......", //
        "fwwwwwho", //
        "f.......", //
    ]],
    frame_seconds: 0.1,
};

/// A chunk of sandstone rubble thrown by the Sunwrought Colossus (tumbling).
pub const SHOT_RUBBLE: SpriteDef = SpriteDef {
    name: "enemy_shot_rubble",
    legend: &[('o', INK), ('d', SOIL), ('c', CLAY), ('s', SAND)],
    frames: &[
        &[
            ".oooo.", //
            "oscsco", //
            "osccdo", //
            "occddo", //
            "ocddo.", //
            ".ooo..", //
        ],
        &[
            ".ooo..", //
            "ocsco.", //
            "osccso", //
            "odccso", //
            "oddcco", //
            ".oooo.", //
        ],
    ],
    frame_seconds: 0.1,
};

/// The Hollow Matriarch's hex: a violet orb with a lavender spark (pulsing).
pub const SHOT_HEX: SpriteDef = SpriteDef {
    name: "enemy_shot_hex",
    legend: &[('o', PLUM), ('v', VIOLET), ('l', LAVENDER), ('w', BONE)],
    frames: &[
        &[
            ".oooo.", //
            "ovvlvo", //
            "ovlwvo", //
            "ovvvlo", //
            "ovlvvo", //
            ".oooo.", //
        ],
        &[
            ".oooo.", //
            "olvvvo", //
            "ovwlvo", //
            "ovlvvo", //
            "ovvvlo", //
            ".oooo.", //
        ],
    ],
    frame_seconds: 0.09,
};
