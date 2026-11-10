//! Draws button prompts: an action ([`Prompt`]) as keyboard keycaps or controller glyphs for the
//! relevant seat's active device, followed by a short label. All coordinates are UI canvas art
//! pixels; every glyph is [`PROMPT_HEIGHT`] tall so a prompt row lines up with body text.
//!
//! Menus are driven by seat 0, so modal headers use the *menu device* published once per frame
//! ([`set_menu_device`]); per-view HUD prompts pass their seat's device explicitly.

use std::cell::Cell;

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::{
        sprite::{SpriteDef, draw_nine_slice},
        sprites::prompts as art,
    },
    ui_kit::{self, TextStyle},
};
use crate::input::{
    devices::{InputDevice, PadFamily},
    gamepad::PadButton,
    prompts::{Glyph, PadGlyph, Prompt, glyphs, pad_button_name},
};

/// Height of a prompt row (every glyph) in art pixels.
pub const PROMPT_HEIGHT: i32 = art::GLYPH_HEIGHT;
/// Gap between the glyphs of one prompt.
const GLYPH_GAP: i32 = 1;
/// Gap between a prompt's glyphs and its label.
const LABEL_GAP: i32 = 3;
/// Gap between prompts in a row.
const ITEM_GAP: i32 = 9;
/// Text offset that centres the 7-pixel cap height in a glyph row.
const TEXT_DY: i32 = 2;

/// One prompt in a row: an action and what it does here ("select", "buy", "leave").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hint {
    pub prompt: Prompt,
    pub label: &'static str,
}

/// A [`Hint`].
#[must_use]
pub const fn hint(prompt: Prompt, label: &'static str) -> Hint {
    Hint { prompt, label }
}

thread_local! {
    static MENU_DEVICE: Cell<InputDevice> = const { Cell::new(InputDevice::KEYBOARD) };
}

/// Publishes the device menus and dialogs draw their prompts for this frame (seat 0's).
pub fn set_menu_device(device: InputDevice) {
    MENU_DEVICE.with(|current| current.set(device));
}

/// The device menus and dialogs draw their prompts for.
#[must_use]
pub fn menu_device() -> InputDevice {
    MENU_DEVICE.with(Cell::get)
}

fn keycap_width(label: &str) -> i32 {
    (ui_kit::text_width(label, TextStyle::Body) + 6).max(PROMPT_HEIGHT)
}

/// The sprite of a controller glyph, plus the label printed on it (shoulder buttons).
const fn pad_sprite(
    glyph: PadGlyph,
    family: PadFamily,
) -> (&'static SpriteDef, Option<&'static str>) {
    use PadFamily::{Nintendo, PlayStation, Xbox};
    let sprite = match (glyph, family) {
        (PadGlyph::Button(PadButton::A), Xbox) => &art::XBOX_A,
        (PadGlyph::Button(PadButton::B), Xbox) => &art::XBOX_B,
        (PadGlyph::Button(PadButton::X), Xbox) => &art::XBOX_X,
        (PadGlyph::Button(PadButton::Y), Xbox) => &art::XBOX_Y,
        // Nintendo prints its letters rotated: B at the bottom, A on the right...
        (PadGlyph::Button(PadButton::A), Nintendo) => &art::NINTENDO_B,
        (PadGlyph::Button(PadButton::B), Nintendo) => &art::NINTENDO_A,
        (PadGlyph::Button(PadButton::X), Nintendo) => &art::NINTENDO_Y,
        (PadGlyph::Button(PadButton::Y), Nintendo) => &art::NINTENDO_X,
        (PadGlyph::Button(PadButton::A), PlayStation) => &art::PS_CROSS,
        (PadGlyph::Button(PadButton::B), PlayStation) => &art::PS_CIRCLE,
        (PadGlyph::Button(PadButton::X), PlayStation) => &art::PS_SQUARE,
        (PadGlyph::Button(PadButton::Y), PlayStation) => &art::PS_TRIANGLE,
        (PadGlyph::Button(button @ (PadButton::LeftBumper | PadButton::RightBumper)), _) => {
            return (&art::BUMPER, Some(pad_button_name(button, family)));
        }
        (PadGlyph::Button(button @ (PadButton::LeftTrigger | PadButton::RightTrigger)), _) => {
            return (&art::TRIGGER, Some(pad_button_name(button, family)));
        }
        (PadGlyph::Button(PadButton::View), _) => &art::VIEW,
        (PadGlyph::Button(PadButton::Menu | PadButton::Guide), _) => &art::MENU,
        (PadGlyph::Button(PadButton::DPadUp | PadButton::DPadDown) | PadGlyph::DPadVertical, _) => {
            &art::DPAD_VERTICAL
        }
        (
            PadGlyph::Button(PadButton::DPadLeft | PadButton::DPadRight) | PadGlyph::DPadHorizontal,
            _,
        ) => &art::DPAD_HORIZONTAL,
        (PadGlyph::DPadDown, _) => &art::DPAD_DOWN,
        (PadGlyph::Button(PadButton::LeftStick) | PadGlyph::LeftStick, _) => &art::LEFT_STICK,
        (PadGlyph::Button(PadButton::RightStick) | PadGlyph::RightStick, _) => &art::RIGHT_STICK,
    };
    (sprite, None)
}

/// Width of one glyph.
#[must_use]
pub fn glyph_width(glyph: Glyph) -> i32 {
    match glyph {
        Glyph::Key(label) => keycap_width(label),
        Glyph::Pad(glyph, family) => pad_sprite(glyph, family).0.width(),
        Glyph::MouseWheel => art::MOUSE_WHEEL.width(),
    }
}

/// Draws one glyph with its top-left at `(x, y)`; returns its width.
pub fn draw_glyph(draw: &mut impl RaylibDraw, glyph: Glyph, x: i32, y: i32) -> i32 {
    match glyph {
        Glyph::Key(label) => {
            let width = keycap_width(label);
            draw_nine_slice(
                draw,
                &art::KEYCAP,
                Rectangle::new(x as f32, y as f32, width as f32, PROMPT_HEIGHT as f32),
                1.0,
                Color::WHITE,
            );
            let text_x = x + (width - ui_kit::text_width(label, TextStyle::Body)) / 2;
            ui_kit::text_shadowed(draw, label, text_x, y + TEXT_DY, TextStyle::Body, INK, None);
            width
        }
        Glyph::Pad(glyph, family) => {
            let (sprite, label) = pad_sprite(glyph, family);
            ui_kit::icon(draw, sprite, x, y);
            if let Some(label) = label {
                let text_x = x + (sprite.width() - ui_kit::text_width(label, TextStyle::Body)) / 2;
                ui_kit::text_shadowed(
                    draw,
                    label,
                    text_x,
                    y + TEXT_DY,
                    TextStyle::Body,
                    BONE,
                    None,
                );
            }
            sprite.width()
        }
        Glyph::MouseWheel => {
            ui_kit::icon(draw, &art::MOUSE_WHEEL, x, y);
            art::MOUSE_WHEEL.width()
        }
    }
}

/// Width of the glyphs alone (0 when the device has no binding).
fn glyphs_width(glyphs: &[Glyph]) -> i32 {
    let count = i32::try_from(glyphs.len()).unwrap_or(0);
    glyphs.iter().map(|glyph| glyph_width(*glyph)).sum::<i32>() + GLYPH_GAP * (count - 1).max(0)
}

/// Width of one prompt (glyphs + label), or `None` when `device` has no binding for it.
#[must_use]
pub fn prompt_width(hint: Hint, device: InputDevice) -> Option<i32> {
    let glyphs = glyphs(hint.prompt, device);
    if glyphs.is_empty() {
        return None;
    }
    let label = if hint.label.is_empty() {
        0
    } else {
        LABEL_GAP + ui_kit::text_width(hint.label, TextStyle::Body)
    };
    Some(glyphs_width(&glyphs) + label)
}

/// Draws one prompt at `(x, y)` with its label in `color`; returns its width (0 when the device
/// has no binding for it, in which case nothing is drawn).
pub fn draw_prompt(
    draw: &mut impl RaylibDraw,
    x: i32,
    y: i32,
    hint: Hint,
    device: InputDevice,
    color: Color,
) -> i32 {
    let glyphs = glyphs(hint.prompt, device);
    if glyphs.is_empty() {
        return 0;
    }
    let mut cursor = x;
    for (index, glyph) in glyphs.iter().enumerate() {
        if index > 0 {
            cursor += GLYPH_GAP;
        }
        cursor += draw_glyph(draw, *glyph, cursor, y);
    }
    if !hint.label.is_empty() {
        cursor += LABEL_GAP;
        ui_kit::text(
            draw,
            hint.label,
            cursor,
            y + TEXT_DY,
            TextStyle::Body,
            color,
        );
        cursor += ui_kit::text_width(hint.label, TextStyle::Body);
    }
    cursor - x
}

/// Space after a prompt in a row: a label-less prompt groups with the next one
/// ("[Tab] [Esc] close").
const fn gap_after(hint: Hint) -> i32 {
    if hint.label.is_empty() {
        GLYPH_GAP + 2
    } else {
        ITEM_GAP
    }
}

/// Width of a row of prompts (hidden prompts take no room).
#[must_use]
pub fn bar_width(hints: &[Hint], device: InputDevice) -> i32 {
    let mut width = 0;
    let mut gap = 0;
    for hint in hints {
        if let Some(item) = prompt_width(*hint, device) {
            width += gap + item;
            gap = gap_after(*hint);
        }
    }
    width
}

/// Draws a row of prompts from `(x, y)`; returns its width.
pub fn draw_bar(
    draw: &mut impl RaylibDraw,
    x: i32,
    y: i32,
    hints: &[Hint],
    device: InputDevice,
    color: Color,
) -> i32 {
    let mut cursor = x;
    let mut gap = 0;
    for hint in hints {
        if prompt_width(*hint, device).is_some() {
            cursor += gap;
            cursor += draw_prompt(draw, cursor, y, *hint, device, color);
            gap = gap_after(*hint);
        }
    }
    cursor - x
}

/// Splits `hints` into rows no wider than `max_width` (a prompt wider than that gets a row of
/// its own); hidden prompts are dropped.
#[must_use]
pub fn wrap_bar(hints: &[Hint], device: InputDevice, max_width: i32) -> Vec<Vec<Hint>> {
    let mut rows: Vec<Vec<Hint>> = Vec::new();
    let mut width = 0;
    let mut gap = 0;
    for hint in hints {
        let Some(item) = prompt_width(*hint, device) else {
            continue;
        };
        match rows.last_mut() {
            Some(row) if width + gap + item <= max_width => {
                row.push(*hint);
                width += gap + item;
            }
            _ => {
                rows.push(vec![*hint]);
                width = item;
            }
        }
        gap = gap_after(*hint);
    }
    rows
}

/// Height of `rows` wrapped prompt rows.
#[must_use]
pub fn wrapped_height(rows: usize) -> i32 {
    let rows = i32::try_from(rows).unwrap_or(0);
    (rows * (PROMPT_HEIGHT + 2) - 2).max(0)
}

/// Every prompt on every device family, plus the controller glyphs at 3x: the reference sheet
/// captured as the `prompt_sheet` scene (call inside a [`ui_kit::UiCanvasScope`]).
#[allow(
    clippy::too_many_lines,
    reason = "a flat reference sheet: device rows, then the glyphs enlarged"
)]
#[cfg(any(test, not(target_os = "emscripten")))]
pub fn draw_prompt_sheet(draw: &mut impl RaylibDraw) {
    use super::pixel_art::sprite::draw_sprite;
    use crate::input::devices::KeyboardLayout;
    const ALL: [Prompt; 22] = [
        Prompt::Navigate,
        Prompt::Adjust,
        Prompt::Confirm,
        Prompt::Back,
        Prompt::Use,
        Prompt::Walk,
        Prompt::Thrust,
        Prompt::Drill,
        Prompt::Scan,
        Prompt::Bomb,
        Prompt::Tab,
        Prompt::Pause,
        Prompt::Map,
        Prompt::Inventory,
        Prompt::Help,
        Prompt::Fullscreen,
        Prompt::Volume,
        Prompt::QuickSave,
        Prompt::Paste,
        Prompt::Delete,
        Prompt::Scroll,
        Prompt::Join,
    ];
    let canvas = ui_kit::canvas();
    draw.draw_rectangle_rec(canvas.rect(), NIGHT);
    ui_kit::text_centred(
        draw,
        "Button Prompts",
        canvas.width / 2,
        6,
        TextStyle::Title,
        GOLD,
    );
    ui_kit::text_centred(
        draw,
        "Each action on each device (see rendering/ART_GUIDE.md)",
        canvas.width / 2,
        26,
        TextStyle::Body,
        FOG,
    );
    let devices = [
        ("Keyboard", InputDevice::KEYBOARD),
        ("Arrow keys", InputDevice::Keyboard(KeyboardLayout::Arrows)),
        (
            "Xbox",
            InputDevice::Gamepad {
                index: 0,
                family: PadFamily::Xbox,
            },
        ),
        (
            "PlayStation",
            InputDevice::Gamepad {
                index: 1,
                family: PadFamily::PlayStation,
            },
        ),
        (
            "Nintendo",
            InputDevice::Gamepad {
                index: 2,
                family: PadFamily::Nintendo,
            },
        ),
    ];
    let label_width = 70;
    let mut y = 42;
    for (name, device) in devices {
        ui_kit::text(draw, name, 8, y + TEXT_DY, TextStyle::Bold, SKY_LIGHT);
        let mut x = 8 + label_width;
        let mut row_y = y;
        for prompt in ALL {
            let item = hint(prompt, prompt_name(prompt));
            let Some(width) = prompt_width(item, device) else {
                continue;
            };
            if x + width > canvas.width - 8 {
                x = 8 + label_width;
                row_y += PROMPT_HEIGHT + 3;
            }
            draw_prompt(draw, x, row_y, item, device, MIST);
            x += width + ITEM_GAP;
        }
        y = row_y + PROMPT_HEIGHT + 9;
    }
    ui_kit::text(
        draw,
        "Controller glyphs at 3x",
        8,
        y,
        TextStyle::Bold,
        SKY_LIGHT,
    );
    y += 12;
    let mut x = 8;
    for sprite in [
        &art::XBOX_A,
        &art::XBOX_B,
        &art::XBOX_X,
        &art::XBOX_Y,
        &art::PS_CROSS,
        &art::PS_CIRCLE,
        &art::PS_SQUARE,
        &art::PS_TRIANGLE,
        &art::NINTENDO_B,
        &art::NINTENDO_A,
        &art::MENU,
        &art::VIEW,
        &art::DPAD_VERTICAL,
        &art::LEFT_STICK,
        &art::RIGHT_STICK,
        &art::MOUSE_WHEEL,
        &art::BUMPER,
        &art::TRIGGER,
    ] {
        if x + sprite.width() * 3 > canvas.width - 8 {
            break;
        }
        draw_sprite(
            draw,
            sprite,
            0,
            Vector2::new(x as f32, y as f32),
            3.0,
            Color::WHITE,
        );
        x += sprite.width() * 3 + 4;
    }
}

/// A prompt's name on the reference sheet.
#[cfg(any(test, not(target_os = "emscripten")))]
const fn prompt_name(prompt: Prompt) -> &'static str {
    match prompt {
        Prompt::Navigate => "navigate",
        Prompt::Adjust => "adjust",
        Prompt::Confirm => "confirm",
        Prompt::Back => "back",
        Prompt::Use => "use",
        Prompt::Walk => "walk",
        Prompt::Thrust => "thrust",
        Prompt::Drill => "drill",
        Prompt::Scan => "scan",
        Prompt::Bomb => "bomb",
        Prompt::Tab => "tab",
        Prompt::Pause => "pause",
        Prompt::Map => "map",
        Prompt::Inventory => "inventory",
        Prompt::Help => "help",
        Prompt::Fullscreen => "fullscreen",
        Prompt::Volume => "volume",
        Prompt::QuickSave => "save",
        Prompt::QuickLoad => "load",
        Prompt::Paste => "paste",
        Prompt::Delete => "delete",
        Prompt::Infrastructure => "kits",
        Prompt::Details => "diagnostics",
        Prompt::NetOverlay => "network",
        Prompt::Join => "join",
        Prompt::Scroll => "scroll",
    }
}
