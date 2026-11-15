use super::*;
use crate::input::gamepad::{DETONATE_HOLD_SECONDS, PAD_BINDINGS};

const DT: f32 = 1.0 / 60.0;
/// A phone held sideways, with a notch and a home indicator.
const PHONE: TouchScreen = TouchScreen {
    width: 844.0,
    height: 390.0,
    insets: [47.0, 0.0, 47.0, 21.0],
    points_per_ui_pixel: 1.6,
};

const PLAY: TouchContext = TouchContext {
    mode: TouchMode::Play,
    armed: false,
    charges_out: false,
    prompt: false,
    kits: [false; KIT_COUNT],
};

const ARMED: TouchContext = TouchContext {
    armed: true,
    ..PLAY
};

/// A touch screen driven one frame at a time.
struct Rig {
    input: TouchInput,
    context: TouchContext,
    settings: TouchSettings,
}

impl Rig {
    fn new(context: TouchContext) -> Self {
        Self {
            input: TouchInput::default(),
            context,
            settings: TouchSettings::default(),
        }
    }

    fn layout(&self) -> TouchLayout {
        TouchLayout::compute(PHONE, self.settings, self.context, self.input.tools_open())
    }

    fn frame(&mut self, points: &[(i32, (f32, f32))]) -> TouchFrame {
        let layout = self.layout();
        let points: Vec<TouchPoint> = points
            .iter()
            .map(|(id, (x, y))| TouchPoint {
                id: *id,
                x: *x,
                y: *y,
            })
            .collect();
        self.input.update(&points, &layout, PHONE, self.context, DT)
    }

    fn centre(&self, control: TouchControl) -> (f32, f32) {
        self.layout()
            .rect(control)
            .unwrap_or_else(|| panic!("{control:?} is not on screen"))
            .centre()
    }

    /// A point in the stick zone clear of every control.
    fn stick_point(&self) -> (f32, f32) {
        let layout = self.layout();
        let zone = layout.stick_zone;
        let point = (zone.x + zone.w * 0.4, zone.y + zone.h * 0.6);
        assert_eq!(layout.hit(point.0, point.1), None);
        point
    }

    /// Presses and lifts one finger on `control`, returning both frames' input.
    fn tap(&mut self, control: TouchControl) -> (PlayerInput, PlayerInput) {
        let at = self.centre(control);
        let down = self.frame(&[(1, at)]).input;
        let up = self.frame(&[]).input;
        (down, up)
    }
}

fn close(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.001
}

#[test]
fn the_stick_drives_thrusts_and_drills_like_a_controller_stick() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    let planted = rig.frame(&[(1, base)]);
    assert!(planted.used && planted.engaged);
    assert!(close(planted.input.horizontal, 0.0) && !planted.input.thrust);

    let right = rig.frame(&[(1, (base.0 + radius, base.1))]).input;
    assert!(close(right.horizontal, 1.0), "{}", right.horizontal);
    assert!(!right.thrust && !right.drill_down);

    let up = rig.frame(&[(1, (base.0, base.1 - radius))]).input;
    assert!(up.thrust && !up.drill_down && close(up.horizontal, 0.0));

    let down = rig.frame(&[(1, (base.0, base.1 + radius))]).input;
    assert!(down.drill_down && !down.thrust);

    let lifted = rig.frame(&[]);
    assert!(!lifted.engaged && !lifted.input.drill_down);
    assert!(lifted.view.stick.is_none());
}

#[test]
fn the_stick_matches_the_controller_mapping_for_the_same_deflection() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    rig.frame(&[(1, base)]);
    let touch = rig
        .frame(&[(1, (base.0 - radius * 0.8, base.1 - radius * 0.6))])
        .input;

    let mut mapper = PadMapper::default();
    mapper.map(&PadState::default(), DT);
    let pad = mapper.map(
        &PadState {
            left_x: -0.8,
            left_y: -0.6,
            ..PadState::default()
        },
        DT,
    );
    assert!(
        close(touch.horizontal, pad.horizontal),
        "full travel is 1:1"
    );
    assert_eq!(touch.thrust, pad.thrust);
    assert_eq!(touch.drill_down, pad.drill_down);
}

#[test]
fn a_light_push_already_moves_and_jitter_does_not() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    rig.frame(&[(1, base)]);
    let jitter = rig.frame(&[(1, (base.0 + radius * 0.08, base.1))]).input;
    assert!(close(jitter.horizontal, 0.0));
    let light = rig.frame(&[(1, (base.0 + radius * 0.2, base.1))]).input;
    assert!(
        light.horizontal > STICK_DEADZONE && light.horizontal < 0.5,
        "{}",
        light.horizontal
    );
}

#[test]
fn drilling_down_does_not_drift_sideways() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    rig.frame(&[(1, base)]);
    let down = rig
        .frame(&[(1, (base.0 + radius * 0.3, base.1 + radius * 0.9))])
        .input;
    assert!(down.drill_down && close(down.horizontal, 0.0));
    // Pushed properly sideways as well, it drives while it drills.
    let diagonal = rig
        .frame(&[(1, (base.0 + radius * 0.7, base.1 + radius * 0.7))])
        .input;
    assert!(diagonal.drill_down && diagonal.horizontal > 0.5);
}

#[test]
fn the_stick_base_follows_a_thumb_that_outruns_it() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    rig.frame(&[(1, base)]);
    rig.frame(&[(1, (base.0 + radius * 3.0, base.1))]);
    // Coming back by one radius is already a full reverse, not a return to centre.
    let back = rig.frame(&[(1, (base.0 + radius * 1.0, base.1))]).input;
    assert!(close(back.horizontal, -1.0), "{}", back.horizontal);
}

#[test]
fn a_finger_keeps_what_it_first_touched() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let bomb = rig.centre(TouchControl::Bomb);
    rig.frame(&[(1, base)]);
    let over_button = rig.frame(&[(1, bomb)]);
    assert!(over_button.view.held.is_empty());
    let lifted = rig.frame(&[]).input;
    assert!(
        !lifted.bomb,
        "a stick thumb sliding over a button presses nothing"
    );

    // And a second finger still works the buttons while the first steers.
    rig.frame(&[(1, base)]);
    rig.frame(&[(1, base), (2, bomb)]);
    let dropped = rig.frame(&[(1, base)]).input;
    assert!(dropped.bomb);
}

#[test]
fn only_one_thumb_owns_the_stick() {
    let mut rig = Rig::new(PLAY);
    let base = rig.stick_point();
    let radius = rig.layout().stick_radius();
    rig.frame(&[(1, base)]);
    let second = (base.0 + 20.0, base.1 - 30.0);
    let frame = rig.frame(&[(1, (base.0 + radius, base.1)), (2, second)]);
    assert!(close(frame.input.horizontal, 1.0));
    assert!(frame.events.contains(&PointerEvent::Press {
        x: second.0,
        y: second.1
    }));
}

#[test]
fn tapping_the_bomb_drops_it_on_release() {
    let mut rig = Rig::new(PLAY);
    let (down, up) = rig.tap(TouchControl::Bomb);
    assert!(!down.bomb);
    assert!(up.bomb && close(up.aim_x, 0.0) && close(up.aim_y, 0.0));
    assert!(!rig.frame(&[]).input.bomb, "once");
}

#[test]
fn dragging_the_bomb_aims_and_releasing_throws() {
    let mut rig = Rig::new(PLAY);
    let at = rig.centre(TouchControl::Bomb);
    let unit = rig.layout().button;
    rig.frame(&[(1, at)]);
    let aiming = rig.frame(&[(1, (at.0 - unit, at.1 - unit))]);
    assert!(!aiming.input.bomb);
    assert!(aiming.input.aim_x < -0.5 && aiming.input.aim_y < -0.5);
    assert!(matches!(aiming.view.aim, Some((TouchControl::Bomb, _))));
    let thrown = rig.frame(&[]).input;
    assert!(thrown.bomb && thrown.aim_x < -0.5 && thrown.aim_y < -0.5);
}

#[test]
fn pulling_an_aim_back_onto_the_bomb_cancels_the_throw() {
    let mut rig = Rig::new(PLAY);
    let at = rig.centre(TouchControl::Bomb);
    let unit = rig.layout().button;
    rig.frame(&[(1, at)]);
    rig.frame(&[(1, (at.0 - unit, at.1))]);
    rig.frame(&[(1, (at.0 - unit * 0.1, at.1))]);
    assert!(!rig.frame(&[]).input.bomb);
}

#[test]
fn a_wobbly_tap_on_the_bomb_still_drops_it() {
    let mut rig = Rig::new(PLAY);
    let at = rig.centre(TouchControl::Bomb);
    let unit = rig.layout().button;
    rig.frame(&[(1, at)]);
    rig.frame(&[(1, (at.0 + unit * 0.2, at.1 + unit * 0.1))]);
    assert!(rig.frame(&[]).input.bomb);
}

#[test]
fn holding_fire_auto_aims_and_dragging_it_aims_freely() {
    let mut rig = Rig::new(ARMED);
    let at = rig.centre(TouchControl::Fire);
    let unit = rig.layout().button;
    let held = rig.frame(&[(1, at)]).input;
    assert!(held.fire && !held.fire_thrusts_unarmed);
    assert!(
        close(held.aim_x, 0.0) && close(held.aim_y, 0.0),
        "no aim: the weapon auto-aims"
    );
    let aimed = rig.frame(&[(1, (at.0 - unit * 0.5, at.1))]).input;
    assert!(aimed.fire && aimed.aim_x < -STICK_DEADZONE && close(aimed.aim_y, 0.0));
    let released = rig.frame(&[]).input;
    assert!(!released.fire && close(released.aim_x, 0.0));
}

#[test]
fn the_fire_button_is_only_there_with_a_weapon() {
    assert!(Rig::new(PLAY).layout().rect(TouchControl::Fire).is_none());
    let armed = Rig::new(ARMED).layout();
    assert!(armed.rect(TouchControl::Fire).is_some());
    assert!(armed.rect(TouchControl::CycleWeapon).is_some());
}

#[test]
fn detonator_taps_fire_everything_and_holds_fire_in_sequence() {
    let context = TouchContext {
        charges_out: true,
        ..PLAY
    };
    assert!(
        Rig::new(PLAY)
            .layout()
            .rect(TouchControl::Detonate)
            .is_none()
    );
    let mut rig = Rig::new(context);
    let (down, up) = rig.tap(TouchControl::Detonate);
    assert!(!down.detonate && up.detonate && !up.detonate_sequence);

    let at = rig.centre(TouchControl::Detonate);
    let mut sequenced = 0;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a short positive frame count"
    )]
    let frames = (DETONATE_HOLD_SECONDS / DT).ceil() as usize + 3;
    for _ in 0..frames {
        let input = rig.frame(&[(1, at)]).input;
        assert!(!input.detonate);
        sequenced += usize::from(input.detonate_sequence);
    }
    assert_eq!(sequenced, 1);
    assert!(!rig.frame(&[]).input.detonate, "the hold already fired");
}

#[test]
fn one_shot_buttons_act_once_on_the_way_down() {
    let mut rig = Rig::new(ARMED);
    for (control, check) in [
        (
            TouchControl::Scan,
            (|input| input.scan) as fn(&PlayerInput) -> bool,
        ),
        (TouchControl::CycleWeapon, |input| input.cycle_weapon),
        (TouchControl::CycleExplosive, |input| input.cycle_explosive),
        (TouchControl::Pause, |input| input.pause),
        (TouchControl::Inventory, |input| input.inventory),
        (TouchControl::Map, |input| input.map),
    ] {
        let at = rig.centre(control);
        assert!(check(&rig.frame(&[(1, at)]).input), "{control:?}");
        assert!(!check(&rig.frame(&[(1, at)]).input), "{control:?} repeats");
        rig.frame(&[]);
    }
}

#[test]
fn the_prompt_under_the_rig_is_a_button() {
    assert!(Rig::new(PLAY).layout().rect(TouchControl::Prompt).is_none());
    let mut rig = Rig::new(TouchContext {
        prompt: true,
        ..PLAY
    });
    let (down, _) = rig.tap(TouchControl::Prompt);
    assert!(down.interact && down.confirm);
}

#[test]
fn the_tools_fan_offers_carried_kits_and_build_mode() {
    let mut kits = [false; KIT_COUNT];
    kits[2] = true;
    kits[4] = true;
    let mut rig = Rig::new(TouchContext { kits, ..PLAY });
    assert!(rig.layout().rect(TouchControl::Kit(2)).is_none());
    rig.tap(TouchControl::Tools);
    let open = rig.layout();
    assert!(open.rect(TouchControl::Kit(2)).is_some());
    assert!(open.rect(TouchControl::Kit(4)).is_some());
    assert!(open.rect(TouchControl::Kit(0)).is_none(), "not carried");
    assert!(open.rect(TouchControl::Build).is_some());

    let (placed, _) = rig.tap(TouchControl::Kit(4));
    assert!(placed.place_pump && !placed.place_lift);
    assert!(!rig.input.tools_open(), "placing puts the fan away");

    rig.tap(TouchControl::Tools);
    let (build, _) = rig.tap(TouchControl::Build);
    assert!(build.build_mode);
}

#[test]
fn touching_anything_else_puts_the_tools_fan_away() {
    let mut rig = Rig::new(PLAY);
    rig.tap(TouchControl::Tools);
    assert!(rig.input.tools_open());
    rig.tap(TouchControl::Tools);
    assert!(!rig.input.tools_open(), "the tools button toggles");

    rig.tap(TouchControl::Tools);
    assert!(
        rig.layout().rect(TouchControl::Scan).is_none(),
        "the fan takes over the thumb corner"
    );
    let base = rig.stick_point();
    rig.frame(&[(1, base)]);
    assert!(!rig.input.tools_open(), "steering puts it away");
    rig.frame(&[]);

    rig.tap(TouchControl::Tools);
    rig.context.mode = TouchMode::Menu;
    rig.frame(&[]);
    assert!(!rig.input.tools_open(), "a menu opening closes it");
}

#[test]
fn every_kit_slot_places_its_own_kit() {
    let mut rig = Rig::new(TouchContext {
        kits: [true; KIT_COUNT],
        ..PLAY
    });
    let placed: Vec<[bool; KIT_COUNT]> = (0u8..)
        .take(KIT_COUNT)
        .map(|slot| {
            rig.tap(TouchControl::Tools);
            let (input, _) = rig.tap(TouchControl::Kit(slot));
            [
                input.place_relay,
                input.place_drone,
                input.place_lift,
                input.place_support,
                input.place_pump,
                input.place_processor,
            ]
        })
        .collect();
    for (slot, flags) in placed.iter().enumerate() {
        let expected: Vec<bool> = (0..KIT_COUNT).map(|index| index == slot).collect();
        assert_eq!(flags.to_vec(), expected, "slot {slot}");
    }
}

#[test]
fn build_mode_points_with_taps_and_has_no_stick() {
    let mut rig = Rig::new(TouchContext {
        mode: TouchMode::Build,
        ..TouchContext::default()
    });
    let layout = rig.layout();
    assert!(close(layout.stick_zone.w, 0.0));
    let world = (200.0, 250.0);
    assert_eq!(layout.hit(world.0, world.1), None);
    assert!(rig.frame(&[(1, world)]).input.build_pointer.is_none());
    let lifted = rig.frame(&[]);
    assert_eq!(
        lifted.events,
        vec![PointerEvent::Tap {
            x: world.0,
            y: world.1
        }]
    );

    let (place, _) = rig.tap(TouchControl::BuildPlace);
    assert!(place.confirm);
    let (remove, _) = rig.tap(TouchControl::BuildRemove);
    assert!(remove.build_remove && !remove.confirm);
    let (rotate, _) = rig.tap(TouchControl::BuildRotate);
    assert!(rotate.build_rotate);
    let (next, _) = rig.tap(TouchControl::BuildNext);
    assert!(next.tab_next);
    let (previous, _) = rig.tap(TouchControl::BuildPrevious);
    assert!(previous.build_previous);
    let (exit, _) = rig.tap(TouchControl::BuildExit);
    assert!(exit.build_mode);
}

#[test]
fn tapping_a_pointed_tile_again_places_there() {
    let mut taps = BuildTap::default();
    assert!(!taps.tap((3, 4)), "the first tap only points");
    assert!(taps.tap((3, 4)));
    assert!(!taps.tap((5, 4)), "another tile points again");
    taps.reset();
    assert!(!taps.tap((5, 4)));
}

#[test]
fn dragging_a_menu_scrolls_it_and_a_still_finger_taps() {
    let mut rig = Rig::new(TouchContext::default());
    let layout = rig.layout();
    // Right of the stick zone, above the buttons.
    let start = (600.0, 120.0);
    assert_eq!(layout.hit(start.0, start.1), None);
    assert!(!layout.stick_zone.contains(start.0, start.1));
    rig.frame(&[(1, start)]);
    let small = rig.frame(&[(1, (start.0, start.1 + 4.0))]);
    assert!(close(small.input.ui_scroll, 0.0), "inside the tap slop");
    let dragged = rig.frame(&[(1, (start.0, start.1 + 4.0 + 76.8))]);
    assert!(
        close(dragged.input.ui_scroll, 1.0),
        "48 art pixels down is one notch up: {}",
        dragged.input.ui_scroll
    );
    assert_eq!(
        rig.frame(&[]).events,
        vec![PointerEvent::DragEnd {
            x: start.0,
            y: start.1 + 80.8
        }]
    );

    rig.frame(&[(2, start)]);
    assert_eq!(
        rig.frame(&[]).events,
        vec![PointerEvent::Tap {
            x: start.0,
            y: start.1
        }]
    );
}

#[test]
fn menus_and_interiors_have_their_own_buttons() {
    let mut menu = Rig::new(TouchContext::default());
    assert!(menu.tap(TouchControl::Confirm).0.confirm);
    let back = menu.tap(TouchControl::Back).0;
    assert!(back.back && !back.cancel && !back.pause);
    assert!(menu.tap(TouchControl::TabNext).0.tab_next);
    assert!(menu.tap(TouchControl::TabPrevious).0.tab_previous);
    // The stick steps the selection.
    let base = menu.stick_point();
    let radius = menu.layout().stick_radius();
    menu.frame(&[(1, base)]);
    assert!(
        menu.frame(&[(1, (base.0, base.1 + radius))])
            .input
            .menu_down
    );

    let mut interior = Rig::new(TouchContext {
        mode: TouchMode::Interior,
        ..TouchContext::default()
    });
    assert!(interior.tap(TouchControl::Use).0.interact);
    assert!(interior.tap(TouchControl::Back).0.back);
}

/// Every combination of what can be on screen.
fn contexts() -> Vec<TouchContext> {
    let mut contexts = Vec::new();
    for mode in [
        TouchMode::Play,
        TouchMode::Build,
        TouchMode::Interior,
        TouchMode::Menu,
    ] {
        for bits in 0u8..16 {
            contexts.push(TouchContext {
                mode,
                armed: bits & 1 != 0,
                charges_out: bits & 2 != 0,
                prompt: bits & 4 != 0,
                kits: if bits & 8 == 0 {
                    [false; KIT_COUNT]
                } else {
                    [true; KIT_COUNT]
                },
            });
        }
    }
    contexts
}

fn layouts(screen: TouchScreen) -> Vec<(String, TouchLayout)> {
    let mut layouts = Vec::new();
    for context in contexts() {
        for left_handed in [false, true] {
            for (thrust_button, scale) in [(false, 1.0), (true, 1.0), (true, 1.4), (false, 0.7)] {
                for tools_open in [false, true] {
                    let settings = TouchSettings {
                        scale,
                        left_handed,
                        thrust_button,
                    };
                    layouts.push((
                        format!("{context:?} {settings:?} tools {tools_open}"),
                        TouchLayout::compute(screen, settings, context, tools_open),
                    ));
                }
            }
        }
    }
    layouts
}

const SCREENS: &[TouchScreen] = &[
    PHONE,
    // The same phone upright.
    TouchScreen {
        width: 390.0,
        height: 844.0,
        insets: [0.0, 47.0, 0.0, 34.0],
        points_per_ui_pixel: 1.6,
    },
    // A small phone, both ways.
    TouchScreen::new(667.0, 375.0),
    TouchScreen::new(375.0, 667.0),
    // A tablet, both ways.
    TouchScreen::new(1180.0, 820.0),
    TouchScreen::new(820.0, 1180.0),
    // A desktop window (`--touch`).
    TouchScreen::new(1280.0, 720.0),
];

#[test]
fn controls_stay_inside_the_safe_area_and_never_overlap() {
    for screen in SCREENS {
        for (name, layout) in layouts(*screen) {
            let safe = screen.safe();
            for (index, (control, rect)) in layout.controls.iter().enumerate() {
                assert!(
                    rect.inside(safe),
                    "{control:?} leaves the safe area on {screen:?}: {name}"
                );
                // The prompt is a wide strip under the rig that buttons may sit on top of:
                // they are hit first.
                if *control == TouchControl::Prompt {
                    assert_eq!(index, layout.controls.len() - 1, "{name}");
                    continue;
                }
                for (other, other_rect) in &layout.controls[index + 1..] {
                    assert!(
                        *other == TouchControl::Prompt || !rect.overlaps(*other_rect),
                        "{control:?} overlaps {other:?} on {screen:?}: {name}"
                    );
                }
            }
        }
    }
}

#[test]
fn buttons_are_big_enough_to_hit() {
    for screen in SCREENS {
        for (name, layout) in layouts(*screen) {
            for (control, rect) in &layout.controls {
                assert!(
                    rect.w >= 18.0 && rect.h >= 18.0,
                    "{control:?} is {}x{} on {screen:?}: {name}",
                    rect.w,
                    rect.h
                );
            }
        }
    }
}

#[test]
fn left_handed_mirrors_the_stick_and_the_buttons() {
    let right = TouchLayout::compute(PHONE, TouchSettings::default(), PLAY, false);
    let left = TouchLayout::compute(
        PHONE,
        TouchSettings {
            left_handed: true,
            ..TouchSettings::default()
        },
        PLAY,
        false,
    );
    let middle = PHONE.width * 0.5;
    let bomb = |layout: &TouchLayout| layout.rect(TouchControl::Bomb).map(TouchRect::centre);
    assert!(bomb(&right).is_some_and(|(x, _)| x > middle));
    assert!(bomb(&left).is_some_and(|(x, _)| x < middle));
    assert!(right.stick_zone.centre().0 < middle && left.stick_zone.centre().0 > middle);
}

#[test]
fn every_controller_action_has_a_touch_control_somewhere() {
    let layouts = layouts(PHONE);
    for (action, _) in PAD_BINDINGS {
        match pad_action_control(*action) {
            TouchReach::Stick => {}
            TouchReach::Control(control) => assert!(
                layouts
                    .iter()
                    .any(|(_, layout)| layout.rect(control).is_some()),
                "{action:?} maps to {control:?}, which no layout shows"
            ),
        }
    }
}

#[test]
fn prompts_shown_for_touch_are_short_words() {
    for prompt in [
        Prompt::Use,
        Prompt::Confirm,
        Prompt::Back,
        Prompt::Scroll,
        Prompt::BuildCursor,
    ] {
        assert!(!touch_labels(prompt).is_empty(), "{prompt:?}");
    }
    assert!(touch_labels(Prompt::Bomb).is_empty(), "it has a button");
    assert!(touch_labels(Prompt::Fullscreen).is_empty(), "not on touch");
}

/// Every [`PlayerInput`] field, with how touch reaches it. Exhaustive on purpose: a new input
/// field does not compile here until someone decides what touch does about it.
#[test]
fn every_input_field_has_a_touch_answer() {
    let PlayerInput {
        // The stick (through the controller mapping).
        horizontal: _,
        thrust: _,
        drill_down: _,
        menu_up: _,
        menu_down: _,
        menu_left: _,
        menu_right: _,
        // On-screen controls.
        interact: _,
        confirm: _,
        back: _,
        pause: _,
        tab_previous: _,
        tab_next: _,
        inventory: _,
        map: _,
        bomb: _,
        cycle_explosive: _,
        detonate: _,
        detonate_sequence: _,
        aim_x: _,
        aim_y: _,
        fire: _,
        cycle_weapon: _,
        scan: _,
        place_relay: _,
        place_drone: _,
        place_lift: _,
        place_support: _,
        place_pump: _,
        place_processor: _,
        build_mode: _,
        build_remove: _,
        build_rotate: _,
        build_previous: _,
        // Taps on the world (the app projects them) and drags on menus.
        build_pointer: _,
        pointer_primary: _,
        ui_scroll: _,
        // Never set by touch: a controller's trigger quirk, the mouse's screen-space aim (the
        // fire button's drag aims instead), and seats that only auto-aim.
        fire_thrusts_unarmed: _,
        pointer: _,
        auto_aim: _,
        pointer_secondary: _,
        // Not on touch: Esc-style cancel (Back never quits), desktop hotkeys, and the keyboard's
        // text entry and paging keys.
        cancel: _,
        details: _,
        save: _,
        load: _,
        selected_upgrade: _,
        help: _,
        volume_up: _,
        volume_down: _,
        fullscreen: _,
        local_multiplayer_toggle: _,
        exit_requested: _,
        text_input: _,
        text_backspace: _,
        net_debug: _,
        tune_next: _,
        page_up: _,
        page_down: _,
        scroll_home: _,
        scroll_end: _,
        paste: _,
    } = PlayerInput::default();
}
