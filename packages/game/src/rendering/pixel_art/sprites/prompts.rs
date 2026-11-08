//! Button-prompt glyphs: keyboard keycaps and controller buttons (see `rendering/prompts.rs`).
//!
//! Every glyph is 11 art pixels tall so prompts line up with body text. Face buttons are one
//! circle grid per printed letter/symbol recoloured per controller family; bumpers and triggers
//! are blank shoulder shapes the prompt code labels with the pixel font (LB, L1, ZL...). Grids
//! are generated from simple shapes (circle, rounded rectangle, cross) and checked in as data.

use super::super::{
    palette::{
        AMBER, BONE, CREAM, FOG, GOLD, INK, LEAF, LIME, MAROON, MINT, MIST, MOSS, NIGHT, OCEAN,
        PINK, RED, ROSE, SKY, SKY_LIGHT, SLATE, SLATE_DARK, SLATE_LIGHT, STEEL, STEEL_DARK,
        STEEL_LIGHT,
    },
    sprite::{NineSlice, SpriteDef},
};

/// Height of every prompt glyph in art pixels.
pub const GLYPH_HEIGHT: i32 = 11;

/// A keyboard key: stretched to its label by [`KEYCAP`].
pub const KEYCAP_SPRITE: SpriteDef = SpriteDef {
    name: "prompt_keycap",
    legend: &[('o', INK), ('h', BONE), ('m', MIST), ('s', FOG)],
    frames: &[&[
        ".ooooo.", //
        "ohhhhho", //
        "ommmmmo", //
        "ommmmmo", //
        "ommmmmo", //
        "ommmmmo", //
        "ommmmmo", //
        "ommmmmo", //
        "ommmmmo", //
        "ossssso", //
        ".ooooo.", //
    ]],
    frame_seconds: 0.0,
};

/// The keycap frame (3-pixel corners; the label is drawn on top in `INK`).
pub const KEYCAP: NineSlice = NineSlice {
    sprite: &KEYCAP_SPRITE,
    border: 3,
};

/// A round face button printed "A".
const FACE_A: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohhwwwffo.", //
    "ohhwfffwffo", //
    "ohfwfffwfso", //
    "ohfwwwwwfso", //
    "ohfwfffwfso", //
    "offwfffwsso", //
    ".ofwfffwso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed "B".
const FACE_B: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohwwwwffo.", //
    "ohhwfffwffo", //
    "ohfwfffwfso", //
    "ohfwwwwffso", //
    "ohfwfffwfso", //
    "offwfffwsso", //
    ".ofwwwwsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed "X".
const FACE_X: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohwfffwfo.", //
    "ohhwfffwffo", //
    "ohffwfwffso", //
    "ohfffwfffso", //
    "ohffwfwffso", //
    "offwfffwsso", //
    ".ofwfffwso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed "Y".
const FACE_Y: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohwfffwfo.", //
    "ohhwfffwffo", //
    "ohffwfwffso", //
    "ohfffwfffso", //
    "ohfffwfffso", //
    "offffwffsso", //
    ".offfwfsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed with a cross.
const FACE_CROSS: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohhfffffo.", //
    "ohhwfffwffo", //
    "ohffwfwffso", //
    "ohfffwfffso", //
    "ohffwfwffso", //
    "offwfffwsso", //
    ".offfffsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed with a circle.
const FACE_CIRCLE: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohhfffffo.", //
    "ohhfwwwfffo", //
    "ohfwfffwfso", //
    "ohfwfffwfso", //
    "ohfwfffwfso", //
    "offfwwwfsso", //
    ".offfffsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed with a square.
const FACE_SQUARE: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohhfffffo.", //
    "ohhwwwwwffo", //
    "ohfwfffwfso", //
    "ohfwfffwfso", //
    "ohfwfffwfso", //
    "offwwwwwsso", //
    ".offfffsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A round face button printed with a triangle.
const FACE_TRIANGLE: &[&str] = &[
    "...ooooo...", //
    "..ohhhhfo..", //
    ".ohhfffffo.", //
    "ohhffwffffo", //
    "ohffwfwffso", //
    "ohffwfwffso", //
    "ohfwfffwfso", //
    "offwwwwwsso", //
    ".offfffsso.", //
    "..ofsssso..", //
    "...ooooo...", //
];

/// A bumper: a flat pill (labelled by the prompt code).
const SHOULDER_BUMPER: &[&str] = &[
    "...............", //
    ".ooooooooooooo.", //
    "ohhhhhhhhhhhhho", //
    "offfffffffffffo", //
    "offfffffffffffo", //
    "offfffffffffffo", //
    "offfffffffffffo", //
    "offfffffffffffo", //
    "ossssssssssssso", //
    ".ooooooooooooo.", //
    "...............", //
];

/// A trigger: rounded top, flat bottom (labelled by the prompt code).
const SHOULDER_TRIGGER: &[&str] = &[
    "..ooooooooo..", //
    ".ohhhhhhhhho.", //
    "ohfffffffffho", //
    "offfffffffffo", //
    "offfffffffffo", //
    "offfffffffffo", //
    "offfffffffffo", //
    "offfffffffffo", //
    "offfffffffffo", //
    "offfffffffffo", //
    "ooooooooooooo", //
];

/// The d-pad cross; arms recolour per variant.
const DPAD: &[&str] = &[
    "...ooooo...", //
    "...ouuuo...", //
    "...ouuuo...", //
    "oooouuuoooo", //
    "olllcccrrro", //
    "olllcccrrro", //
    "olllcccrrro", //
    "oooodddoooo", //
    "...odddo...", //
    "...odddo...", //
    "...ooooo...", //
];

/// Xbox A.
pub const XBOX_A: SpriteDef = SpriteDef {
    name: "pad_xbox_a",
    legend: &[
        ('o', INK),
        ('f', LEAF),
        ('h', LIME),
        ('s', MOSS),
        ('w', BONE),
    ],
    frames: &[FACE_A],
    frame_seconds: 0.0,
};

/// Xbox B.
pub const XBOX_B: SpriteDef = SpriteDef {
    name: "pad_xbox_b",
    legend: &[
        ('o', INK),
        ('f', RED),
        ('h', ROSE),
        ('s', MAROON),
        ('w', BONE),
    ],
    frames: &[FACE_B],
    frame_seconds: 0.0,
};

/// Xbox X.
pub const XBOX_X: SpriteDef = SpriteDef {
    name: "pad_xbox_x",
    legend: &[
        ('o', INK),
        ('f', SKY),
        ('h', SKY_LIGHT),
        ('s', OCEAN),
        ('w', BONE),
    ],
    frames: &[FACE_X],
    frame_seconds: 0.0,
};

/// Xbox Y.
pub const XBOX_Y: SpriteDef = SpriteDef {
    name: "pad_xbox_y",
    legend: &[
        ('o', INK),
        ('f', GOLD),
        ('h', CREAM),
        ('s', AMBER),
        ('w', INK),
    ],
    frames: &[FACE_Y],
    frame_seconds: 0.0,
};

/// Nintendo A (dark button, white letter).
pub const NINTENDO_A: SpriteDef = SpriteDef {
    name: "pad_nintendo_a",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[FACE_A],
    frame_seconds: 0.0,
};

/// Nintendo B (dark button, white letter).
pub const NINTENDO_B: SpriteDef = SpriteDef {
    name: "pad_nintendo_b",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[FACE_B],
    frame_seconds: 0.0,
};

/// Nintendo X (dark button, white letter).
pub const NINTENDO_X: SpriteDef = SpriteDef {
    name: "pad_nintendo_x",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[FACE_X],
    frame_seconds: 0.0,
};

/// Nintendo Y (dark button, white letter).
pub const NINTENDO_Y: SpriteDef = SpriteDef {
    name: "pad_nintendo_y",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[FACE_Y],
    frame_seconds: 0.0,
};

/// `PlayStation` cross (dark button, coloured symbol).
pub const PS_CROSS: SpriteDef = SpriteDef {
    name: "pad_ps_cross",
    legend: &[
        ('o', INK),
        ('f', SLATE_DARK),
        ('h', SLATE),
        ('s', NIGHT),
        ('w', SKY_LIGHT),
    ],
    frames: &[FACE_CROSS],
    frame_seconds: 0.0,
};

/// `PlayStation` circle (dark button, coloured symbol).
pub const PS_CIRCLE: SpriteDef = SpriteDef {
    name: "pad_ps_circle",
    legend: &[
        ('o', INK),
        ('f', SLATE_DARK),
        ('h', SLATE),
        ('s', NIGHT),
        ('w', ROSE),
    ],
    frames: &[FACE_CIRCLE],
    frame_seconds: 0.0,
};

/// `PlayStation` square (dark button, coloured symbol).
pub const PS_SQUARE: SpriteDef = SpriteDef {
    name: "pad_ps_square",
    legend: &[
        ('o', INK),
        ('f', SLATE_DARK),
        ('h', SLATE),
        ('s', NIGHT),
        ('w', PINK),
    ],
    frames: &[FACE_SQUARE],
    frame_seconds: 0.0,
};

/// `PlayStation` triangle (dark button, coloured symbol).
pub const PS_TRIANGLE: SpriteDef = SpriteDef {
    name: "pad_ps_triangle",
    legend: &[
        ('o', INK),
        ('f', SLATE_DARK),
        ('h', SLATE),
        ('s', NIGHT),
        ('w', MINT),
    ],
    frames: &[FACE_TRIANGLE],
    frame_seconds: 0.0,
};

/// Bumper (LB/RB, L1/R1, L/R).
pub const BUMPER: SpriteDef = SpriteDef {
    name: "pad_bumper",
    legend: &[
        ('o', INK),
        ('f', STEEL_DARK),
        ('h', STEEL),
        ('s', SLATE_DARK),
    ],
    frames: &[SHOULDER_BUMPER],
    frame_seconds: 0.0,
};

/// Trigger (LT/RT, L2/R2, ZL/ZR).
pub const TRIGGER: SpriteDef = SpriteDef {
    name: "pad_trigger",
    legend: &[
        ('o', INK),
        ('f', STEEL_DARK),
        ('h', STEEL),
        ('s', SLATE_DARK),
    ],
    frames: &[SHOULDER_TRIGGER],
    frame_seconds: 0.0,
};

/// Menu / Start / Options / Plus (three lines).
pub const MENU: SpriteDef = SpriteDef {
    name: "pad_menu",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[&[
        "...ooooo...", //
        "..ohhhhfo..", //
        ".ohhfffffo.", //
        "ohhwwwwwffo", //
        "ohfffffffso", //
        "ohfwwwwwfso", //
        "ohfffffffso", //
        "offwwwwwsso", //
        ".offfffsso.", //
        "..ofsssso..", //
        "...ooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// View / Back / Share / Minus (two windows).
pub const VIEW: SpriteDef = SpriteDef {
    name: "pad_view",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('h', SLATE_LIGHT),
        ('s', SLATE_DARK),
        ('w', BONE),
    ],
    frames: &[&[
        "...ooooo...", //
        "..ohhhhfo..", //
        ".ohhwwwwwo.", //
        "ohhfwfffwfo", //
        "ohwwwwwfwso", //
        "ohwfffwwwso", //
        "ohwfffwffso", //
        "ofwwwwwfsso", //
        ".offfffsso.", //
        "..ofsssso..", //
        "...ooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// D-pad with the vertical arm(s) lit.
pub const DPAD_VERTICAL: SpriteDef = SpriteDef {
    name: "pad_dpad_vertical",
    legend: &[
        ('o', INK),
        ('c', STEEL_DARK),
        ('u', BONE),
        ('d', BONE),
        ('l', STEEL),
        ('r', STEEL),
    ],
    frames: &[DPAD],
    frame_seconds: 0.0,
};

/// D-pad with the horizontal arm(s) lit.
pub const DPAD_HORIZONTAL: SpriteDef = SpriteDef {
    name: "pad_dpad_horizontal",
    legend: &[
        ('o', INK),
        ('c', STEEL_DARK),
        ('u', STEEL),
        ('d', STEEL),
        ('l', BONE),
        ('r', BONE),
    ],
    frames: &[DPAD],
    frame_seconds: 0.0,
};

/// D-pad with the down arm(s) lit.
pub const DPAD_DOWN: SpriteDef = SpriteDef {
    name: "pad_dpad_down",
    legend: &[
        ('o', INK),
        ('c', STEEL_DARK),
        ('u', STEEL),
        ('d', BONE),
        ('l', STEEL),
        ('r', STEEL),
    ],
    frames: &[DPAD],
    frame_seconds: 0.0,
};

/// The left stick seen from above.
pub const LEFT_STICK: SpriteDef = SpriteDef {
    name: "pad_left_stick",
    legend: &[
        ('o', INK),
        ('f', SLATE),
        ('k', INK),
        ('m', STEEL_LIGHT),
        ('h', BONE),
    ],
    frames: &[&[
        "...ooooo...", //
        "..ofkkffo..", //
        ".ofkhhkkfo.", //
        "ofkhhhmmkfo", //
        "ofkhhmmmkfo", //
        "ofkhmmmmkfo", //
        "ofkmmmmkffo", //
        "offkmmkkffo", //
        ".offfffffo.", //
        "..offfffo..", //
        "...ooooo...", //
    ]],
    frame_seconds: 0.0,
};

/// Every prompt sprite (each must be registered in `sprites::ALL`).
#[cfg(test)]
pub const ALL: [&SpriteDef; 21] = [
    &KEYCAP_SPRITE,
    &XBOX_A,
    &XBOX_B,
    &XBOX_X,
    &XBOX_Y,
    &NINTENDO_A,
    &NINTENDO_B,
    &NINTENDO_X,
    &NINTENDO_Y,
    &PS_CROSS,
    &PS_CIRCLE,
    &PS_SQUARE,
    &PS_TRIANGLE,
    &BUMPER,
    &TRIGGER,
    &MENU,
    &VIEW,
    &DPAD_VERTICAL,
    &DPAD_HORIZONTAL,
    &DPAD_DOWN,
    &LEFT_STICK,
];
