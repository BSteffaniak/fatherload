//! Wave 9 progression art: the deep supplies of Descents IV-VI (ballast tanks, frost heaters,
//! flare buoys, beast bait, shade canopies, time-bubble bombs, echo charges and lamp filters),
//! as the deep counters, the workshops and the supply kit show them.

use fatherload_simulation::workshop::DeepSupply;

#[allow(
    clippy::wildcard_imports,
    reason = "sprite legends name colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::SpriteDef;

/// Ballast tank: a riveted steel cylinder, half flooded, with its valve on top.
pub const BALLAST_TANK: SpriteDef = SpriteDef {
    name: "supply_ballast_tank",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('l', CREAM),
        ('d', STEEL_DARK),
        ('b', SKY),
        ('w', OCEAN),
        ('r', RED),
    ],
    frames: &[&[
        "...oro...",
        "..ooooo..",
        ".osllsdo.",
        ".osssddo.",
        ".obbbbdo.",
        ".owbwwdo.",
        ".owwwwdo.",
        "..ooooo..",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// Frost heater: a squat canister with a glowing band and a flame, frost flakes about it.
pub const FROST_HEATER: SpriteDef = SpriteDef {
    name: "supply_frost_heater",
    legend: &[
        ('o', INK),
        ('r', ORANGE),
        ('a', AMBER),
        ('y', GOLD),
        ('s', STEEL_LIGHT),
        ('d', STEEL_DARK),
        ('i', ICE),
    ],
    frames: &[&[
        "i...r...i",
        "...rar...",
        "i..aya..i",
        "..ooooo..",
        ".ossssdo.",
        ".orrrrdo.",
        ".ossssdo.",
        ".odddddo.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

/// Flare buoy: a striped float riding the water with its flare lit.
pub const FLARE_BUOY: SpriteDef = SpriteDef {
    name: "supply_flare_buoy",
    legend: &[
        ('o', INK),
        ('y', GOLD),
        ('c', CREAM),
        ('r', RED),
        ('b', SKY),
        ('w', OCEAN),
    ],
    frames: &[&[
        "...yyy...",
        "...ycy...",
        "....o....",
        "...oco...",
        "..orrro..",
        "..occco..",
        ".borrrob.",
        "bwbwbwbwb",
        ".wwwwwww.",
    ]],
    frame_seconds: 0.0,
};

/// Beast bait: a haunch of musky meat tied up in hide.
pub const BEAST_BAIT: SpriteDef = SpriteDef {
    name: "supply_beast_bait",
    legend: &[
        ('o', INK),
        ('t', SAND),
        ('p', PINK),
        ('m', RED),
        ('k', MAROON),
        ('g', LIME),
    ],
    frames: &[&[
        "g..o.o..g",
        "....t....",
        "...oto...",
        "..oppmo..",
        ".opmppmo.",
        ".ommpmko.",
        ".opmmkko.",
        "..okkko..",
        "...ooo...",
    ]],
    frame_seconds: 0.0,
};

/// Shade canopy: a mirrored folding shade on its pole.
pub const SHADE_CANOPY: SpriteDef = SpriteDef {
    name: "supply_shade_canopy",
    legend: &[
        ('o', INK),
        ('s', STEEL_LIGHT),
        ('l', CREAM),
        ('d', STEEL),
        ('k', STEEL_DARK),
        ('y', GOLD),
    ],
    frames: &[&[
        "y..ooo...",
        ".oosssoo.",
        "osllllsdo",
        "ooookoooo",
        "....k....",
        "....k....",
        "....k....",
        "..kkk....",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// Time-bubble bomb: an hourglass charge sealed in a violet bubble of slow time.
pub const TIME_BUBBLE_BOMB: SpriteDef = SpriteDef {
    name: "supply_time_bubble_bomb",
    legend: &[
        ('v', VIOLET),
        ('l', LAVENDER),
        ('o', INK),
        ('y', GOLD),
        ('c', CREAM),
    ],
    frames: &[&[
        "..vvvvv..",
        ".vcllllv.",
        "vlooooolv",
        "vlloyollv",
        "vlllolllv",
        "vlloyollv",
        "vlooooolv",
        ".vlllllv.",
        "..vvvvv..",
    ]],
    frame_seconds: 0.0,
};

/// Echo charge: a ringing quartz shard and the sound rings it throws.
pub const ECHO_CHARGE: SpriteDef = SpriteDef {
    name: "supply_echo_charge",
    legend: &[('t', MINT), ('o', INK), ('m', TEAL), ('w', CREAM)],
    frames: &[&[
        ".t.....t.",
        "t...o...t",
        "t..omo..t",
        "t.omwmo.t",
        "t.ommmo.t",
        "t.omwmo.t",
        "t..omo..t",
        ".t.ooo.t.",
        ".........",
    ]],
    frame_seconds: 0.0,
};

/// Lamp filter: a lens of three coloured glasses (red, green, blue) for the colour-keyed walls.
pub const LAMP_FILTER: SpriteDef = SpriteDef {
    name: "supply_lamp_filter",
    legend: &[
        ('o', INK),
        ('w', CREAM),
        ('r', RED),
        ('g', LIME),
        ('b', SKY),
    ],
    frames: &[&[
        "..ooooo..",
        ".owrrrro.",
        "orrrrrrro",
        "ogggggggo",
        "ogggggggo",
        "obbbbbbbo",
        "obbbbbbbo",
        ".obbbbbo.",
        "..ooooo..",
    ]],
    frame_seconds: 0.0,
};

/// The sprite of `supply`.
#[must_use]
pub const fn supply_icon(supply: DeepSupply) -> &'static SpriteDef {
    match supply {
        DeepSupply::BallastTank => &BALLAST_TANK,
        DeepSupply::FrostHeater => &FROST_HEATER,
        DeepSupply::FlareBuoy => &FLARE_BUOY,
        DeepSupply::BeastBait => &BEAST_BAIT,
        DeepSupply::ShadeCanopy => &SHADE_CANOPY,
        DeepSupply::TimeBubbleBomb => &TIME_BUBBLE_BOMB,
        DeepSupply::EchoCharge => &ECHO_CHARGE,
        DeepSupply::LampFilter => &LAMP_FILTER,
    }
}
