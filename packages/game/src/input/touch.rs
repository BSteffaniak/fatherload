#![allow(
    clippy::suboptimal_flops,
    reason = "layout arithmetic reads as the geometry it describes"
)]

//! Touch controls: fingers on a screen to a [`PlayerInput`].
//!
//! Nothing here touches raylib. The app samples the fingers that are down ([`TouchPoint`]),
//! [`TouchLayout::compute`] places the on-screen controls for the current [`TouchContext`], and
//! [`TouchInput::update`] turns both into one frame of input plus what the overlay should draw
//! ([`TouchView`]). Rendering reads the same layout, so a control is always hit where it is
//! drawn.
//!
//! The left thumb gets a floating stick that is fed through the controller mapping
//! ([`PadMapper`]), so driving, thrusting, drilling and stepping menus feel exactly like the
//! left stick of a gamepad. The right thumb gets a cluster that only holds what can be used
//! right now. Two of its buttons aim by dragging: the weapon fires while held and aims where
//! the thumb is dragged (auto-aim when it is not), and the explosive is thrown on release in
//! the dragged direction (a plain tap drops it).
//!
//! All positions are logical screen points, the same space as the mouse.
//!
//! # Keeping up with new controls
//!
//! [`pad_action_control`] and [`touch_labels`] match exhaustively on [`PadAction`] and
//! [`Prompt`], and a test destructures every [`PlayerInput`] field: adding a control anywhere
//! else in the game stops this module compiling until touch has an answer for it.

use std::collections::BTreeMap;

use super::{
    PlayerInput,
    gamepad::{PadAction, PadMapper, PadState, STICK_DEADZONE, primary_button},
    prompts::Prompt,
};

/// Infrastructure kit slots (relay, drone, lift, support, pump, processor).
pub const KIT_COUNT: usize = 6;

/// Stick travel ignored as thumb jitter, as a fraction of the stick radius. Smaller than a
/// gamepad's deadzone: a thumb on glass has no spring to fight.
const TOUCH_DEADZONE: f32 = 0.12;
/// When the thumb points this steeply down the sideways part is dropped, so drilling straight
/// down does not drift into the shaft wall (horizontal share of the vertical travel).
const DRILL_LOCK_RATIO: f32 = 0.45;
/// Drag travel (button widths) before an aim button counts as aiming rather than tapping.
const AIM_DRAG_START: f32 = 0.3;
/// Drag travel (button widths) for a fully deflected aim.
const AIM_DRAG_FULL: f32 = 1.0;
/// Finger travel (points) before a touch on the world or a menu is a drag instead of a tap.
const TAP_SLOP: f32 = 10.0;
/// Art pixels a dialog scrolls per wheel notch (see `PadState::scroll_notches`).
const SCROLL_ART_PIXELS_PER_NOTCH: f32 = 48.0;

/// One finger that is down this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPoint {
    /// Stable for as long as the finger stays down.
    pub id: i32,
    pub x: f32,
    pub y: f32,
}

/// An axis-aligned area in screen points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl TouchRect {
    /// A square of half-size `radius` around `(x, y)`.
    #[must_use]
    pub fn around(x: f32, y: f32, radius: f32) -> Self {
        Self {
            x: x - radius,
            y: y - radius,
            w: radius * 2.0,
            h: radius * 2.0,
        }
    }

    #[must_use]
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    #[must_use]
    pub fn centre(self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    #[cfg(test)]
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    /// Whether `self` lies entirely inside `outer` (a small tolerance absorbs rounding).
    #[cfg(test)]
    #[must_use]
    pub fn inside(self, outer: Self) -> bool {
        const SLACK: f32 = 0.01;
        self.x >= outer.x - SLACK
            && self.y >= outer.y - SLACK
            && self.x + self.w <= outer.x + outer.w + SLACK
            && self.y + self.h <= outer.y + outer.h + SLACK
    }
}

/// The screen the controls are laid out on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchScreen {
    pub width: f32,
    pub height: f32,
    /// Notch / rounded-corner / home-indicator margins: left, top, right, bottom.
    pub insets: [f32; 4],
    /// Screen points per UI art pixel (turns a drag into dialog scroll notches).
    pub points_per_ui_pixel: f32,
    /// Screen units per point (a browser canvas is measured in device pixels, see
    /// `pixel_scale::display_scale`): finger-sized lengths are multiplied by it.
    pub unit: f32,
}

impl TouchScreen {
    /// A screen without insets.
    #[cfg(test)]
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            insets: [0.0; 4],
            points_per_ui_pixel: 2.0,
            unit: 1.0,
        }
    }

    /// The area clear of notches and system gestures.
    #[must_use]
    pub fn safe(self) -> TouchRect {
        let [left, top, right, bottom] = self.insets;
        TouchRect {
            x: left,
            y: top,
            w: (self.width - left - right).max(1.0),
            h: (self.height - top - bottom).max(1.0),
        }
    }

    #[must_use]
    pub fn portrait(self) -> bool {
        self.height > self.width
    }
}

/// What the seat is doing, which decides the controls on screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TouchMode {
    /// Driving the rig outdoors.
    Play,
    /// Build mode: the rig is parked and taps point at tiles.
    Build,
    /// Walking around inside a building.
    Interior,
    /// A menu, dialog, counter or the title screen.
    #[default]
    Menu,
}

/// What the controls need to know about the game this frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts about what is on screen"
)]
pub struct TouchContext {
    pub mode: TouchMode,
    /// A weapon is mounted (shows the fire button and weapon chip).
    pub armed: bool,
    /// Remote charges wait for the detonator.
    pub charges_out: bool,
    /// A context prompt is showing under the rig (enter a building, use a module, call the
    /// rescue crew): tapping it acts on it.
    pub prompt: bool,
    /// Which infrastructure kits the rig carries, by slot.
    pub kits: [bool; KIT_COUNT],
    /// Where the pilot is (Part II F5): in the rig or on foot.
    pub pilot: TouchPilot,
    /// On foot (WP-PILOT): the pilot owns a personal weapon (the swap button shows).
    pub pilot_armed: bool,
    /// On foot: a rope can be thrown or let go (grapple owned, beside the rig, or roped).
    pub rope: bool,
    /// On foot: a lever, chest or keycard door is in reach, or a rig link can call the rig
    /// (the USE button shows).
    pub usable: bool,
    /// On foot in a top-down crypt (WP-TOMBS T29): the stick walks eight ways (no drill lock),
    /// the right thumb's FIRE aims anywhere while held, and ROLL replaces JUMP.
    pub crypt: bool,
}

/// Where the first seat's pilot is, as the touch controls see it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TouchPilot {
    /// In the rig and unable to climb out (moving, or nowhere to stand).
    #[default]
    Seated,
    /// In the rig and able to climb out: the small OUT button shows in the top row.
    CanExit,
    /// On foot: the thumb cluster turns into jump, pick and throw.
    OnFoot,
    /// On foot beside the rig: the IN button shows too.
    CanBoard,
}

impl TouchPilot {
    /// From the view's flags.
    #[must_use]
    pub const fn new(on_foot: bool, can_exit: bool, can_board: bool) -> Self {
        match (on_foot, can_exit, can_board) {
            (true, _, true) => Self::CanBoard,
            (true, _, false) => Self::OnFoot,
            (false, true, _) => Self::CanExit,
            (false, false, _) => Self::Seated,
        }
    }

    /// The pilot is out of the rig.
    #[must_use]
    pub const fn on_foot(self) -> bool {
        matches!(self, Self::OnFoot | Self::CanBoard)
    }
}

impl TouchContext {
    /// Whether the tools button has anything behind it (it always offers build mode).
    #[must_use]
    pub const fn has_tools(self) -> bool {
        matches!(self.mode, TouchMode::Play)
    }
}

/// Player preferences for the touch controls (saved with the settings).
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TouchSettings {
    /// Button size multiplier.
    pub scale: f32,
    /// Stick on the right, buttons on the left.
    pub left_handed: bool,
    /// Show a hold-to-thrust button beside the stick's push-up.
    pub thrust_button: bool,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            left_handed: false,
            thrust_button: false,
        }
    }
}

/// An on-screen control.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TouchControl {
    /// Hold to fire the weapon; drag to aim it.
    Fire,
    CycleWeapon,
    /// Tap to drop the selected explosive; drag and release to throw it.
    Bomb,
    CycleExplosive,
    /// Tap: every waiting charge. Hold: one after another.
    Detonate,
    Scan,
    /// Opens the kit and build fan.
    Tools,
    /// Place the infrastructure kit in this slot.
    Kit(u8),
    /// Enter build mode.
    Build,
    /// Hold to thrust (optional; pushing the stick up thrusts too).
    Thrust,
    /// On foot: hold to jump, and in the air to burn the jetpack.
    Jump,
    /// In a top-down crypt: the dodge roll (the jump button's crypt meaning).
    Roll,
    /// Climb out of the rig (a small top-row button, away from the thumbs).
    ExitRig,
    /// On foot beside the rig: climb back in.
    BoardRig,
    /// On foot (WP-PILOT): the rope. Tap fires the grapple up and forward (or clips onto the
    /// winch beside the rig, or lets go); drag and release aims the grapple.
    Rope,
    /// The context prompt under the rig.
    Prompt,
    /// Use the counter or door the rig stands at inside a building.
    Use,
    Pause,
    Inventory,
    Map,
    Back,
    TabPrevious,
    TabNext,
    BuildPlace,
    BuildRemove,
    BuildRotate,
    BuildPrevious,
    BuildNext,
    BuildExit,
}

/// Where the controls are this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchLayout {
    /// Width of a standard button, in points (everything scales from it).
    pub button: f32,
    /// A touch starting here (and on no control) plants the stick. Empty when the mode has no
    /// stick.
    pub stick_zone: TouchRect,
    /// Controls in hit-test order.
    pub controls: Vec<(TouchControl, TouchRect)>,
}

/// A cluster slot: distance from the thumb corner along x and y, and the control's radius, all
/// in button widths.
type Slot = (f32, f32, f32);

const PRIMARY: Slot = (0.8, 0.8, 0.7);
const SIDE: Slot = (2.15, 0.6, 0.5);
const UPPER: Slot = (0.6, 2.15, 0.45);
const DIAGONAL: Slot = (2.0, 1.95, 0.45);
const FAR_SIDE: Slot = (3.25, 0.5, 0.36);
const FAR_UPPER: Slot = (0.5, 3.2, 0.36);
const FAR_DIAGONAL: Slot = (3.1, 1.6, 0.36);
const OUTER: Slot = (1.75, 3.0, 0.36);
const MENU_BACK: Slot = (0.55, 0.5, 0.42);
const MENU_NEXT: Slot = (1.45, 0.42, 0.34);
const MENU_PREVIOUS: Slot = (2.2, 0.42, 0.34);

/// The HUD's top corner panels in UI art pixels: the stats panel's width (left), the wallet's
/// (right), and how far down the taller of them reaches. The top-row buttons go between them
/// or below them.
const HUD_CORNER_PANELS: (f32, f32, f32) = (184.0, 128.0, 78.0);

/// Radius of the small buttons along the top edge, in button widths.
const TOP_BUTTON_RADIUS: f32 = 0.3;
/// Radius of a tools-fan item, in button widths.
const FAN_RADIUS: f32 = 0.42;
/// Tools-fan items per row.
const FAN_COLUMNS: usize = 4;
/// Radius of the stick's travel, in button widths.
const STICK_RADIUS: f32 = 0.9;

impl TouchLayout {
    /// Lays the controls out for `context` on `screen`. `tools_open` adds the kit and build fan.
    #[must_use]
    pub fn compute(
        screen: TouchScreen,
        settings: TouchSettings,
        context: TouchContext,
        tools_open: bool,
    ) -> Self {
        let safe = screen.safe();
        let share = if screen.portrait() { 0.15 } else { 0.17 };
        // Never so large that the cluster, the top row and the stick stop fitting.
        let unit = screen.unit.max(0.25);
        let button = ((safe.w.min(safe.h) * share).clamp(44.0 * unit, 84.0 * unit)
            * settings.scale.clamp(0.7, 1.4))
        .min(safe.h / 4.6)
        .min(safe.w / 6.0);
        let mut layout = Self {
            button,
            stick_zone: TouchRect::default(),
            controls: Vec::new(),
        };
        let cluster = Cluster {
            safe,
            button,
            mirrored: settings.left_handed,
        };
        // Build mode points with taps and menus are tapped directly: no stick for either.
        let has_stick = matches!(context.mode, TouchMode::Play | TouchMode::Interior);
        if has_stick {
            let top = if screen.portrait() { 0.5 } else { 0.25 };
            let width = safe.w * 0.45;
            layout.stick_zone = TouchRect {
                x: if settings.left_handed {
                    safe.x + safe.w - width
                } else {
                    safe.x
                },
                y: safe.y + safe.h * top,
                w: width,
                h: safe.h * (1.0 - top),
            };
        }
        match context.mode {
            TouchMode::Play => {
                layout.push_top(screen, settings.left_handed, play_top_row(context));
                if context.pilot.on_foot() && context.crypt {
                    layout.push_crypt(cluster, context);
                } else if context.pilot.on_foot() {
                    layout.push_foot(cluster, context);
                } else if tools_open {
                    // The fan takes the thumb corner over; only its own button stays.
                    layout
                        .controls
                        .push((TouchControl::Tools, cluster.place(tools_slot(context))));
                    layout.push_fan(cluster, context);
                } else {
                    layout.push_play(cluster, settings, context);
                }
                if context.prompt {
                    let (x, y) = safe.centre();
                    layout.controls.push((
                        TouchControl::Prompt,
                        TouchRect {
                            x: x - button * 2.5,
                            y,
                            w: button * 5.0,
                            h: button * 1.6,
                        },
                    ));
                }
            }
            TouchMode::Build => {
                layout.push_top(screen, settings.left_handed, &[TouchControl::BuildExit]);
                for (control, slot) in [
                    (TouchControl::BuildPlace, PRIMARY),
                    (TouchControl::BuildRemove, SIDE),
                    (TouchControl::BuildRotate, UPPER),
                    (TouchControl::BuildNext, DIAGONAL),
                    (TouchControl::BuildPrevious, FAR_DIAGONAL),
                ] {
                    layout.controls.push((control, cluster.place(slot)));
                }
            }
            TouchMode::Interior => {
                layout.push_top(screen, settings.left_handed, &[TouchControl::Pause]);
                layout
                    .controls
                    .push((TouchControl::Use, cluster.place(PRIMARY)));
                layout
                    .controls
                    .push((TouchControl::Back, cluster.place(SIDE)));
            }
            TouchMode::Menu => {
                // Rows are tapped where they are; what is left is backing out and stepping
                // a row's value or tab left and right. Kept small: they sit over the dialog.
                for (control, slot) in [
                    (TouchControl::Back, MENU_BACK),
                    (TouchControl::TabNext, MENU_NEXT),
                    (TouchControl::TabPrevious, MENU_PREVIOUS),
                ] {
                    layout.controls.push((control, cluster.place(slot)));
                }
            }
        }
        layout
    }

    /// The control under `(x, y)`.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<TouchControl> {
        self.controls
            .iter()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(control, _)| *control)
    }

    /// Where `control` is, if it is on screen.
    #[cfg(test)]
    #[must_use]
    pub fn rect(&self, control: TouchControl) -> Option<TouchRect> {
        self.controls
            .iter()
            .find(|(candidate, _)| *candidate == control)
            .map(|(_, rect)| *rect)
    }

    /// Radius of the stick's travel in points.
    #[must_use]
    pub fn stick_radius(&self) -> f32 {
        self.button * STICK_RADIUS
    }

    /// On foot: the pilot jumps where the rig would fire, swings the pick (or fires the held
    /// weapon) beside it, and throws explosives above. The rope, the weapon swap and the use
    /// button join when they mean something (WP-PILOT).
    fn push_foot(&mut self, cluster: Cluster, context: TouchContext) {
        let mut push = |control, slot| self.controls.push((control, cluster.place(slot)));
        push(TouchControl::Jump, PRIMARY);
        push(TouchControl::Fire, SIDE);
        push(TouchControl::Bomb, UPPER);
        push(TouchControl::CycleExplosive, FAR_UPPER);
        if context.pilot == TouchPilot::CanBoard {
            push(TouchControl::BoardRig, FAR_SIDE);
        } else if context.usable {
            push(TouchControl::Use, FAR_SIDE);
        }
        if context.charges_out {
            push(TouchControl::Detonate, DIAGONAL);
        } else if context.rope {
            push(TouchControl::Rope, DIAGONAL);
        }
        if context.charges_out && context.rope {
            push(TouchControl::Rope, FAR_DIAGONAL);
        }
        if context.pilot_armed {
            push(TouchControl::CycleWeapon, OUTER);
        }
    }

    /// In a top-down crypt (WP-TOMBS T29): the big FIRE button under the right thumb is the
    /// aim stick (held fires, dragged aims anywhere), ROLL beside it, USE above when a lever,
    /// cache, shrine or the stairs are in reach, the explosive and the weapon swap further out.
    fn push_crypt(&mut self, cluster: Cluster, context: TouchContext) {
        let mut push = |control, slot| self.controls.push((control, cluster.place(slot)));
        push(TouchControl::Fire, PRIMARY);
        push(TouchControl::Roll, SIDE);
        push(TouchControl::Bomb, DIAGONAL);
        push(TouchControl::CycleExplosive, FAR_DIAGONAL);
        if context.usable {
            push(TouchControl::Use, UPPER);
        }
        if context.charges_out {
            push(TouchControl::Detonate, FAR_UPPER);
        }
        if context.pilot_armed {
            push(TouchControl::CycleWeapon, FAR_SIDE);
        }
    }

    fn push_play(&mut self, cluster: Cluster, settings: TouchSettings, context: TouchContext) {
        let mut push = |control, slot| self.controls.push((control, cluster.place(slot)));
        if context.armed {
            push(TouchControl::Fire, PRIMARY);
            push(TouchControl::CycleWeapon, UPPER);
            push(TouchControl::Bomb, SIDE);
            push(TouchControl::CycleExplosive, FAR_SIDE);
            push(TouchControl::Scan, FAR_UPPER);
        } else {
            push(TouchControl::Bomb, PRIMARY);
            push(TouchControl::CycleExplosive, SIDE);
            push(TouchControl::Scan, UPPER);
        }
        push(TouchControl::Tools, tools_slot(context));
        if context.charges_out {
            push(TouchControl::Detonate, DIAGONAL);
        }
        if settings.thrust_button {
            push(TouchControl::Thrust, FAR_DIAGONAL);
        }
    }

    /// A row of small buttons centred on the top edge. When the gap between the HUD's corner
    /// panels is too narrow for it, the row drops below them and moves to the stick's side,
    /// clear of the thumb cluster.
    fn push_top(&mut self, screen: TouchScreen, left_handed: bool, controls: &[TouchControl]) {
        let safe = screen.safe();
        let radius = self.button * TOP_BUTTON_RADIUS;
        let pitch = self.button * 0.8;
        #[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
        let row_width = pitch * (controls.len() as f32 - 1.0) + radius * 2.0;
        let panels = HUD_CORNER_PANELS;
        let gap = safe.w - (panels.0 + panels.1) * screen.points_per_ui_pixel;
        let dropped = gap < row_width + self.button * 0.2;
        let drop = if dropped {
            panels.2 * screen.points_per_ui_pixel
        } else {
            0.0
        };
        let margin = self.button * 0.2 + radius;
        #[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
        let span = pitch * (controls.len() as f32 - 1.0);
        let first = match (dropped, left_handed) {
            // Centred in the gap between the two panels (they are not the same width).
            (false, _) => safe.x + panels.0 * screen.points_per_ui_pixel + gap * 0.5 - span * 0.5,
            (true, false) => safe.x + margin,
            (true, true) => safe.x + safe.w - margin - span,
        };
        for (index, control) in controls.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
            let x = pitch.mul_add(index as f32, first);
            self.controls.push((
                *control,
                TouchRect::around(x, safe.y + drop + radius + self.button * 0.1, radius),
            ));
        }
    }

    /// The tools fan: every carried kit and build mode, in rows growing out of the thumb
    /// corner (where the buttons it replaces were).
    fn push_fan(&mut self, cluster: Cluster, context: TouchContext) {
        let items = (0u8..)
            .zip(context.kits)
            .filter(|(_, carried)| *carried)
            .map(|(slot, _)| TouchControl::Kit(slot))
            .chain([TouchControl::Build]);
        for (index, control) in items.enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
            let (column, row) = ((index % FAN_COLUMNS) as f32, (index / FAN_COLUMNS) as f32);
            self.controls.push((
                control,
                cluster.place((0.6 + column, 0.6 + row, FAN_RADIUS)),
            ));
        }
    }
}

/// Where the tools button sits (the weapon's buttons push it outwards).
const fn tools_slot(context: TouchContext) -> Slot {
    if context.armed { OUTER } else { FAR_UPPER }
}

/// The thumb corner the cluster hangs off.
#[derive(Clone, Copy)]
struct Cluster {
    safe: TouchRect,
    button: f32,
    mirrored: bool,
}

impl Cluster {
    fn place(self, (along, up, radius): Slot) -> TouchRect {
        let margin = self.button * 0.2;
        let x = if self.mirrored {
            self.safe.x + margin + along * self.button
        } else {
            self.safe.x + self.safe.w - margin - along * self.button
        };
        let y = self.safe.y + self.safe.h - margin - up * self.button;
        TouchRect::around(x, y, radius * self.button)
    }
}

/// A touch on something other than a control: the world, or a menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    /// The finger went down.
    Press { x: f32, y: f32 },
    /// The finger moved by `(dx, dy)` since the last frame, past the tap slop.
    Drag { x: f32, y: f32, dx: f32, dy: f32 },
    /// The finger lifted without dragging.
    Tap { x: f32, y: f32 },
    /// The finger lifted after dragging.
    DragEnd { x: f32, y: f32 },
}

/// What the overlay draws beyond the layout itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchView {
    /// The stick's base and thumb position while a thumb is on it.
    pub stick: Option<((f32, f32), (f32, f32))>,
    /// Controls with a finger on them.
    pub held: Vec<TouchControl>,
    /// The aim being dragged: the button, and the aim vector (-1..=1, +y down).
    pub aim: Option<(TouchControl, (f32, f32))>,
    pub tools_open: bool,
}

/// One frame of touch input.
#[derive(Clone, Debug, Default)]
pub struct TouchFrame {
    pub input: PlayerInput,
    /// A finger went down this frame (touch is the device in use).
    pub used: bool,
    /// Any finger is down.
    pub engaged: bool,
    /// Touches on the world or a menu, in order.
    pub events: Vec<PointerEvent>,
    pub view: TouchView,
}

#[derive(Clone, Copy, Debug)]
enum Grab {
    Stick {
        base: (f32, f32),
    },
    Control {
        control: TouchControl,
        start: (f32, f32),
        /// The finger has been dragged out far enough to aim at some point.
        aimed: bool,
    },
    Pointer {
        start: (f32, f32),
        dragging: bool,
    },
    /// Swallowed (it closed the tools fan).
    Ignored,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    grab: Grab,
    x: f32,
    y: f32,
}

/// Finger tracking across frames. Keep one for the touch screen.
#[derive(Clone, Debug, Default)]
pub struct TouchInput {
    fingers: BTreeMap<i32, Finger>,
    mapper: PadMapper,
    tools_open: bool,
    /// The last frame's context was a crypt: the stick has no drill lock.
    crypt: bool,
}

impl TouchInput {
    /// Whether the tools fan is open (the layout for the next frame needs it).
    #[must_use]
    pub const fn tools_open(&self) -> bool {
        self.tools_open
    }

    /// Maps this frame's `points` against `layout` (`delta_seconds` since the last frame).
    pub fn update(
        &mut self,
        points: &[TouchPoint],
        layout: &TouchLayout,
        screen: TouchScreen,
        context: TouchContext,
        delta_seconds: f32,
    ) -> TouchFrame {
        let mut frame = TouchFrame::default();
        let mut pressed = Vec::new();
        let mut released = Vec::new();
        self.crypt = context.crypt;
        if !context.has_tools() {
            self.tools_open = false;
        }

        let lifted: Vec<i32> = self
            .fingers
            .keys()
            .filter(|id| !points.iter().any(|point| point.id == **id))
            .copied()
            .collect();
        for id in lifted {
            let Some(finger) = self.fingers.remove(&id) else {
                continue;
            };
            match finger.grab {
                Grab::Control { control, .. } => released.push((control, finger)),
                Grab::Pointer { dragging, .. } => frame.events.push(if dragging {
                    PointerEvent::DragEnd {
                        x: finger.x,
                        y: finger.y,
                    }
                } else {
                    PointerEvent::Tap {
                        x: finger.x,
                        y: finger.y,
                    }
                }),
                Grab::Stick { .. } | Grab::Ignored => {}
            }
        }

        for point in points {
            if let Some(finger) = self.fingers.get_mut(&point.id) {
                let (dx, dy) = (point.x - finger.x, point.y - finger.y);
                finger.x = point.x;
                finger.y = point.y;
                let aiming = finger.aim(layout).is_some();
                if let Grab::Control { aimed, .. } = &mut finger.grab {
                    *aimed |= aiming;
                }
                if let Grab::Pointer { start, dragging } = &mut finger.grab {
                    if !*dragging
                        && (point.x - start.0).hypot(point.y - start.1)
                            > TAP_SLOP * screen.unit.max(0.25)
                    {
                        *dragging = true;
                    }
                    if *dragging && (dx.abs() > 0.0 || dy.abs() > 0.0) {
                        frame.events.push(PointerEvent::Drag {
                            x: point.x,
                            y: point.y,
                            dx,
                            dy,
                        });
                    }
                }
                continue;
            }
            frame.used = true;
            let grab = self.grab_for(*point, layout);
            match grab {
                Grab::Control { control, .. } => pressed.push(control),
                Grab::Pointer { .. } => frame.events.push(PointerEvent::Press {
                    x: point.x,
                    y: point.y,
                }),
                Grab::Stick { .. } | Grab::Ignored => {}
            }
            self.fingers.insert(
                point.id,
                Finger {
                    grab,
                    x: point.x,
                    y: point.y,
                },
            );
        }
        frame.engaged = !self.fingers.is_empty();

        let pad = self.pad_state(layout, &mut frame.view);
        let mut input = self.mapper.map(&pad, delta_seconds);
        // The mapper speaks for a controller, whose trigger thrusts an unarmed rig.
        input.fire_thrusts_unarmed = false;
        for control in pressed {
            self.press(control, &mut input);
        }
        self.hold(layout, &mut input, &mut frame.view);
        for (control, finger) in released {
            release(control, finger, layout, &mut input);
        }
        if matches!(context.mode, TouchMode::Menu) {
            let notch = SCROLL_ART_PIXELS_PER_NOTCH * screen.points_per_ui_pixel.max(0.1);
            for event in &frame.events {
                if let PointerEvent::Drag { dy, .. } = event {
                    input.ui_scroll += dy / notch;
                }
            }
        }
        frame.view.tools_open = self.tools_open;
        frame.input = input;
        frame
    }

    /// What a new finger at `point` takes hold of.
    fn grab_for(&mut self, point: TouchPoint, layout: &TouchLayout) -> Grab {
        let hit = layout.hit(point.x, point.y);
        if self.tools_open && !matches!(hit, Some(TouchControl::Kit(_) | TouchControl::Build)) {
            // A touch anywhere else puts the fan away; on the tools button that is all it does.
            self.tools_open = false;
            if hit == Some(TouchControl::Tools) {
                return Grab::Ignored;
            }
        }
        if let Some(control) = hit {
            return Grab::Control {
                control,
                start: (point.x, point.y),
                aimed: false,
            };
        }
        let stick_taken = self
            .fingers
            .values()
            .any(|finger| matches!(finger.grab, Grab::Stick { .. }));
        if !stick_taken && layout.stick_zone.contains(point.x, point.y) {
            Grab::Stick {
                base: (point.x, point.y),
            }
        } else {
            Grab::Pointer {
                start: (point.x, point.y),
                dragging: false,
            }
        }
    }

    /// The virtual controller: the stick, and the detonator (whose tap / hold timing the
    /// controller mapping owns).
    fn pad_state(&mut self, layout: &TouchLayout, view: &mut TouchView) -> PadState {
        let mut pad = PadState::default();
        let radius = layout.stick_radius().max(1.0);
        for finger in self.fingers.values_mut() {
            match &mut finger.grab {
                Grab::Stick { base } => {
                    let (mut dx, mut dy) = (finger.x - base.0, finger.y - base.1);
                    let travel = dx.hypot(dy);
                    if travel > radius {
                        // The base follows a thumb that outruns it, so reversing is immediate.
                        let scale = radius / travel;
                        dx *= scale;
                        dy *= scale;
                        *base = (finger.x - dx, finger.y - dy);
                    }
                    let (x, y) = stick_axes(dx / radius, dy / radius, !self.crypt);
                    pad.left_x = x;
                    pad.left_y = y;
                    view.stick = Some((*base, (finger.x, finger.y)));
                }
                Grab::Control {
                    control: TouchControl::Detonate,
                    ..
                } => {
                    if let Some(button) = primary_button(PadAction::Detonate) {
                        pad.buttons.insert(button);
                    }
                }
                Grab::Control { .. } | Grab::Pointer { .. } | Grab::Ignored => {}
            }
        }
        pad
    }

    /// A control was pressed: everything that acts on the way down.
    const fn press(&mut self, control: TouchControl, input: &mut PlayerInput) {
        match control {
            TouchControl::CycleWeapon => input.cycle_weapon = true,
            TouchControl::CycleExplosive => input.cycle_explosive = true,
            TouchControl::Scan => input.scan = true,
            TouchControl::Tools => self.tools_open = !self.tools_open,
            TouchControl::Kit(slot) => {
                self.tools_open = false;
                match slot {
                    0 => input.place_relay = true,
                    1 => input.place_drone = true,
                    2 => input.place_lift = true,
                    3 => input.place_support = true,
                    4 => input.place_pump = true,
                    _ => input.place_processor = true,
                }
            }
            TouchControl::Build => {
                self.tools_open = false;
                input.build_mode = true;
            }
            TouchControl::BuildExit => input.build_mode = true,
            TouchControl::Prompt => {
                input.interact = true;
                input.confirm = true;
            }
            TouchControl::Use | TouchControl::ExitRig | TouchControl::BoardRig => {
                input.interact = true;
            }
            TouchControl::Pause => input.pause = true,
            TouchControl::Inventory => input.inventory = true,
            TouchControl::Map => input.map = true,
            TouchControl::BuildPlace => input.confirm = true,
            TouchControl::Back => input.back = true,
            // Left and right both switch tabs and adjust the selected row, on every screen.
            TouchControl::TabPrevious => input.menu_left = true,
            TouchControl::TabNext => input.menu_right = true,
            TouchControl::BuildNext => input.tab_next = true,
            TouchControl::BuildRemove => input.build_remove = true,
            TouchControl::BuildRotate => input.build_rotate = true,
            TouchControl::BuildPrevious => input.build_previous = true,
            // Held (fire, thrust), timed by the pad mapping (detonate) or acting on release
            // (bomb, rope).
            TouchControl::Fire
            | TouchControl::Thrust
            | TouchControl::Jump
            | TouchControl::Roll
            | TouchControl::Detonate
            | TouchControl::Bomb
            | TouchControl::Rope => {}
        }
    }

    /// Controls that act for as long as a finger rests on them.
    fn hold(&self, layout: &TouchLayout, input: &mut PlayerInput, view: &mut TouchView) {
        for finger in self.fingers.values() {
            let Grab::Control { control, .. } = finger.grab else {
                continue;
            };
            view.held.push(control);
            match control {
                TouchControl::Fire => {
                    input.fire = true;
                    if let Some(aim) = finger.aim(layout) {
                        input.aim_x = aim.0;
                        input.aim_y = aim.1;
                        view.aim = Some((control, aim));
                    }
                }
                TouchControl::Bomb | TouchControl::Rope => {
                    // Aiming shows the throw before the release commits it.
                    if let Some(aim) = finger.aim(layout) {
                        input.aim_x = aim.0;
                        input.aim_y = aim.1;
                        view.aim = Some((control, aim));
                    }
                }
                TouchControl::Thrust => input.thrust = true,
                TouchControl::Jump | TouchControl::Roll => input.jump = true,
                _ => {}
            }
        }
    }
}

/// A control was let go: the explosive is dropped (a tap) or thrown (a drag still held out).
/// A drag pulled back onto the button cancels.
fn release(control: TouchControl, finger: Finger, layout: &TouchLayout, input: &mut PlayerInput) {
    if control != TouchControl::Bomb && control != TouchControl::Rope {
        return;
    }
    let aim = finger.aim(layout);
    let cancelled = aim.is_none() && matches!(finger.grab, Grab::Control { aimed: true, .. });
    if cancelled {
        return;
    }
    if control == TouchControl::Rope {
        input.rope = true;
    } else {
        input.bomb = true;
    }
    if let Some((x, y)) = aim {
        input.aim_x = x;
        input.aim_y = y;
    }
}

impl Finger {
    /// The aim this finger describes by its drag away from where it pressed a control, once it
    /// is far enough out to mean it.
    fn aim(self, layout: &TouchLayout) -> Option<(f32, f32)> {
        let Grab::Control { start, .. } = self.grab else {
            return None;
        };
        let unit = layout.button.max(1.0);
        let (dx, dy) = ((self.x - start.0) / unit, (self.y - start.1) / unit);
        let travel = dx.hypot(dy);
        if travel < AIM_DRAG_START {
            return None;
        }
        // Always past the controller deadzone, so a short drag aims as surely as a long one.
        let strength = (travel / AIM_DRAG_FULL).clamp(STICK_DEADZONE + 0.05, 1.0);
        Some((dx / travel * strength, dy / travel * strength))
    }
}

/// Shapes the thumb's offset (fractions of the stick radius) into controller stick axes: a
/// small touch deadzone rescaled to start just past the controller's own, and (with
/// `drill_lock`, everywhere but a crypt) no sideways drift while pushing steeply down.
fn stick_axes(x: f32, y: f32, drill_lock: bool) -> (f32, f32) {
    let travel = x.hypot(y).min(1.0);
    if travel < TOUCH_DEADZONE {
        return (0.0, 0.0);
    }
    let x = if drill_lock && y > 0.0 && x.abs() < y * DRILL_LOCK_RATIO {
        0.0
    } else {
        x
    };
    let length = x.hypot(y).max(f32::EPSILON);
    let live = (travel - TOUCH_DEADZONE) / (1.0 - TOUCH_DEADZONE);
    let strength = (1.0 - STICK_DEADZONE)
        .mul_add(live, STICK_DEADZONE)
        .min(1.0);
    (x / length * strength, y / length * strength)
}

/// The small buttons along the top edge in play: the OUT button joins them while the rig can
/// let its pilot out.
const fn play_top_row(context: TouchContext) -> &'static [TouchControl] {
    if matches!(context.pilot, TouchPilot::CanExit) {
        &[
            TouchControl::Inventory,
            TouchControl::Pause,
            TouchControl::Map,
            TouchControl::ExitRig,
        ]
    } else {
        &[
            TouchControl::Inventory,
            TouchControl::Pause,
            TouchControl::Map,
        ]
    }
}

/// How a controller action is reached by touch.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchReach {
    /// An on-screen control.
    Control(TouchControl),
    /// The floating stick.
    Stick,
    /// Touching the thing itself (a menu row).
    Tap,
}

/// The touch answer to every controller action. Exhaustive on purpose: a new [`PadAction`]
/// does not compile until it is given one here (checked by the tests).
#[cfg(test)]
#[must_use]
pub const fn pad_action_control(action: PadAction) -> TouchReach {
    use TouchControl as C;
    use TouchReach::{Control, Stick, Tap};
    match action {
        PadAction::Thrust => Stick,
        PadAction::Confirm => Tap,
        PadAction::Interact => Control(C::Prompt),
        PadAction::Back => Control(C::Back),
        PadAction::Inventory => Control(C::Inventory),
        PadAction::Map => Control(C::Map),
        PadAction::Pause => Control(C::Pause),
        PadAction::Scan => Control(C::Scan),
        PadAction::Bomb => Control(C::Bomb),
        PadAction::CycleExplosive => Control(C::CycleExplosive),
        PadAction::Detonate => Control(C::Detonate),
        PadAction::TabPrevious => Control(C::TabPrevious),
        PadAction::TabNext => Control(C::TabNext),
        // On foot the fire button swings the pick.
        PadAction::FireWeapon | PadAction::SwingPick => Control(C::Fire),
        PadAction::CycleWeapon => Control(C::CycleWeapon),
        PadAction::BuildMode => Control(C::Build),
        PadAction::BuildRotate => Control(C::BuildRotate),
        PadAction::Jump => Control(C::Jump),
        PadAction::BoardRig => Control(C::BoardRig),
        PadAction::Rope => Control(C::Rope),
    }
}

/// What a prompt shows on a touch screen. Most are empty: the control is already on screen
/// with its own label, so the prompt is left out. Exhaustive on purpose: a new [`Prompt`] does
/// not compile until touch decides what to show for it.
#[must_use]
pub const fn touch_labels(prompt: Prompt) -> &'static [&'static str] {
    match prompt {
        Prompt::Use | Prompt::Confirm | Prompt::BuildCursor => &["Tap"],
        Prompt::Back => &["Back"],
        Prompt::Scroll => &["Drag"],
        // On-screen controls that label themselves.
        Prompt::Navigate
        | Prompt::Adjust
        | Prompt::Walk
        | Prompt::Thrust
        | Prompt::Drill
        | Prompt::Scan
        | Prompt::Bomb
        | Prompt::CycleExplosive
        | Prompt::Detonate
        | Prompt::DetonateSequence
        | Prompt::Aim
        | Prompt::FireWeapon
        | Prompt::CycleWeapon
        | Prompt::AimWeapon
        | Prompt::Tab
        | Prompt::Pause
        | Prompt::Map
        | Prompt::Inventory
        | Prompt::Infrastructure
        | Prompt::BuildMode
        | Prompt::BuildPlace
        | Prompt::BuildRemove
        | Prompt::BuildRotate
        | Prompt::BuildCycle
        | Prompt::Jump
        | Prompt::Climb
        | Prompt::Crouch
        | Prompt::Pick
        | Prompt::ExitRig
        | Prompt::BoardRig
        | Prompt::Rope
        // Not on touch: desktop hotkeys, text editing keys and controller seat claims.
        | Prompt::Help
        | Prompt::Fullscreen
        | Prompt::Volume
        | Prompt::QuickSave
        | Prompt::QuickLoad
        | Prompt::Paste
        | Prompt::Delete
        | Prompt::Details
        | Prompt::NetOverlay
        | Prompt::Join => &[],
    }
}

/// Build mode's tap-to-point, tap-again-to-place: remembers the tile last pointed at.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildTap {
    pointed: Option<(i32, i32)>,
}

impl BuildTap {
    /// A tap landed on `tile`. Returns whether it places (the tile was already pointed at).
    pub fn tap(&mut self, tile: (i32, i32)) -> bool {
        let place = self.pointed == Some(tile);
        self.pointed = Some(tile);
        place
    }

    /// Forget the pointed tile (build mode was left).
    pub const fn reset(&mut self) {
        self.pointed = None;
    }
}

#[cfg(test)]
mod tests;
