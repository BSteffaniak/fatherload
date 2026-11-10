//! UI frames: 9-slice panels, buttons, tabs and the tooltip box.
//!
//! Each frame shares one grid between several legends (standard / accent / warning, or
//! idle / selected / disabled), so the shapes stay identical and only the colours change.

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::sprite::{NineSlice, SpriteDef};

/// Panel: ink outline, coloured rim (`r`), bevel highlight (`h`), fill (`f`), inner shadow (`s`).
const PANEL_GRID: &[&str] = &[
    ".ooooooo.",
    "orrrrrrro",
    "orhhhhhro",
    "orhfffsro",
    "orhfffsro",
    "orhfffsro",
    "orsssssro",
    "orrrrrrro",
    ".ooooooo.",
];

pub const PANEL_STANDARD_SPRITE: SpriteDef = SpriteDef {
    name: "ui_panel_standard",
    legend: &[
        ('o', INK),
        ('r', SLATE_LIGHT),
        ('h', SLATE),
        ('f', SLATE_DARK),
        ('s', NIGHT),
    ],
    frames: &[PANEL_GRID],
    frame_seconds: 0.0,
};

pub const PANEL_ACCENT_SPRITE: SpriteDef = SpriteDef {
    name: "ui_panel_accent",
    legend: &[
        ('o', INK),
        ('r', AMBER),
        ('h', SLATE),
        ('f', SLATE_DARK),
        ('s', NIGHT),
    ],
    frames: &[PANEL_GRID],
    frame_seconds: 0.0,
};

pub const PANEL_WARNING_SPRITE: SpriteDef = SpriteDef {
    name: "ui_panel_warning",
    legend: &[
        ('o', INK),
        ('r', RED),
        ('h', MAROON),
        ('f', NIGHT),
        ('s', INK),
    ],
    frames: &[PANEL_GRID],
    frame_seconds: 0.0,
};

/// Inset well: a recessed area inside a panel (lists, map, text fields).
pub const WELL_SPRITE: SpriteDef = SpriteDef {
    name: "ui_well",
    legend: &[('o', INK), ('s', INK), ('f', NIGHT), ('h', SLATE)],
    frames: &[&[
        "ooooo", //
        "osssh", //
        "osffh", //
        "osffh", //
        "ohhhh", //
    ]],
    frame_seconds: 0.0,
};

/// Button: ink outline, top-left light (`h`), face (`f`), bottom-right shade (`s`).
const BUTTON_GRID: &[&str] = &[
    ".ooooo.", //
    "ohhhhho", //
    "ohfffso", //
    "ohfffso", //
    "ohfffso", //
    "ossssso", //
    ".ooooo.", //
];

pub const BUTTON_IDLE_SPRITE: SpriteDef = SpriteDef {
    name: "ui_button_idle",
    legend: &[
        ('o', INK),
        ('h', SLATE_LIGHT),
        ('f', SLATE),
        ('s', SLATE_DARK),
    ],
    frames: &[BUTTON_GRID],
    frame_seconds: 0.0,
};

pub const BUTTON_SELECTED_SPRITE: SpriteDef = SpriteDef {
    name: "ui_button_selected",
    legend: &[('o', INK), ('h', CREAM), ('f', AMBER), ('s', ORANGE)],
    frames: &[BUTTON_GRID],
    frame_seconds: 0.0,
};

pub const BUTTON_DISABLED_SPRITE: SpriteDef = SpriteDef {
    name: "ui_button_disabled",
    legend: &[('o', INK), ('h', SLATE), ('f', SLATE_DARK), ('s', NIGHT)],
    frames: &[BUTTON_GRID],
    frame_seconds: 0.0,
};

/// Tab: like a panel top with an open bottom, so the active tab merges into its panel.
const TAB_GRID: &[&str] = &[
    ".ooooo.", //
    "orrrrro", //
    "orhhhro", //
    "orhfsro", //
    "orhfsro", //
    "orhfsro", //
    "orhfsro", //
];

pub const TAB_ACTIVE_SPRITE: SpriteDef = SpriteDef {
    name: "ui_tab_active",
    legend: &[
        ('o', INK),
        ('r', AMBER),
        ('h', SLATE),
        ('f', SLATE_DARK),
        ('s', SLATE_DARK),
    ],
    frames: &[TAB_GRID],
    frame_seconds: 0.0,
};

pub const TAB_IDLE_SPRITE: SpriteDef = SpriteDef {
    name: "ui_tab_idle",
    legend: &[
        ('o', INK),
        ('r', SLATE),
        ('h', SLATE_DARK),
        ('f', NIGHT),
        ('s', NIGHT),
    ],
    frames: &[TAB_GRID],
    frame_seconds: 0.0,
};

/// Tooltip / speech box: light paper with an ink outline.
pub const TOOLTIP_SPRITE: SpriteDef = SpriteDef {
    name: "ui_tooltip",
    legend: &[('o', INK), ('c', CREAM), ('s', SAND)],
    frames: &[&[
        ".ooo.", //
        "occco", //
        "occco", //
        "ossso", //
        ".ooo.", //
    ]],
    frame_seconds: 0.0,
};

/// Speech-bubble tail, drawn under a tooltip box pointing down at the speaker.
pub const TOOLTIP_TAIL: SpriteDef = SpriteDef {
    name: "ui_tooltip_tail",
    legend: &[('o', INK), ('s', SAND)],
    frames: &[&[
        "osssso", //
        ".osso.", //
        "..oo..", //
    ]],
    frame_seconds: 0.0,
};

/// Selection cursor (a right-pointing chevron) shown left of the focused menu item.
pub const CURSOR: SpriteDef = SpriteDef {
    name: "ui_cursor",
    legend: &[('o', INK), ('g', GOLD), ('a', AMBER)],
    frames: &[
        &[
            "oo...", //
            "ogo..", //
            "oggo.", //
            "ogaao", //
            "oaao.", //
            "oao..", //
            "oo...", //
        ],
        &[
            ".oo..", //
            ".ogo.", //
            ".oggo", //
            ".ogao", //
            ".oao.", //
            ".oo..", //
            ".....", //
        ],
    ],
    frame_seconds: 0.4,
};

pub const PANEL_STANDARD: NineSlice = NineSlice {
    sprite: &PANEL_STANDARD_SPRITE,
    border: 3,
};
pub const PANEL_ACCENT: NineSlice = NineSlice {
    sprite: &PANEL_ACCENT_SPRITE,
    border: 3,
};
pub const PANEL_WARNING: NineSlice = NineSlice {
    sprite: &PANEL_WARNING_SPRITE,
    border: 3,
};
pub const WELL: NineSlice = NineSlice {
    sprite: &WELL_SPRITE,
    border: 2,
};
pub const BUTTON_IDLE: NineSlice = NineSlice {
    sprite: &BUTTON_IDLE_SPRITE,
    border: 3,
};
pub const BUTTON_SELECTED: NineSlice = NineSlice {
    sprite: &BUTTON_SELECTED_SPRITE,
    border: 3,
};
pub const BUTTON_DISABLED: NineSlice = NineSlice {
    sprite: &BUTTON_DISABLED_SPRITE,
    border: 3,
};
#[cfg(any(test, not(target_os = "emscripten")))]
pub const TAB_ACTIVE: NineSlice = NineSlice {
    sprite: &TAB_ACTIVE_SPRITE,
    border: 3,
};
#[cfg(any(test, not(target_os = "emscripten")))]
pub const TAB_IDLE: NineSlice = NineSlice {
    sprite: &TAB_IDLE_SPRITE,
    border: 3,
};
pub const TOOLTIP: NineSlice = NineSlice {
    sprite: &TOOLTIP_SPRITE,
    border: 2,
};
