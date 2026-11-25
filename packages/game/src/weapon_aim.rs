//! Seat-side weapon aiming (presentation and input, no gameplay): turns a seat's sampled input
//! into the simulation's [`WeaponControl`], tracks where each seat's view sits on screen so the
//! mouse can aim in world space, and works out the aim reticle each view draws.
//!
//! Aim sources, by seat:
//!
//! * the keyboard-and-mouse seat (solo, or split-screen WASD) aims at the mouse pointer while
//!   the mouse is in use ([`crate::input::PlayerInput::pointer`]);
//! * a controller aims with its right stick;
//! * otherwise (the split-screen arrow-keys seat always, any seat not actively aiming) the
//!   simulation auto-aims: the nearest target in a cone around the rig's facing
//!   ([`fatherload_simulation::WorldState::auto_aim_target`]).

use fatherload_simulation::{
    Player, PlayerCommand, WeaponControl, WeaponKind, WorldState,
    terrain::TILE_SIZE,
    weapon::{MUZZLE_RISE, quantize_aim},
};

use crate::{
    input::PlayerInput,
    render_snapshot::{CameraPosition, Viewport},
};

/// Where one seat's view sat on screen when it was last drawn: enough to turn a screen point
/// into a world point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewAnchor {
    pub viewport: Viewport,
    pub camera: CameraPosition,
    pub world_scale: f32,
    /// The camera's roll (F9: radians, clockwise; the seat's rig orientation while it rolls,
    /// 0 otherwise). The world is drawn turned the other way about [`Self::pivot`].
    pub roll: f32,
    /// The world point the rolled view turns about (the seat's rig as drawn).
    pub pivot: (f32, f32),
}

impl ViewAnchor {
    /// The world point under screen point `(x, y)` (through the rolled camera, if it rolls).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "viewport coordinates are small")]
    pub fn world_point(&self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.world_scale.max(f32::EPSILON);
        let flat = (
            (x - self.viewport.x as f32) / scale + self.camera.x,
            (y - self.viewport.y as f32) / scale + self.camera.y,
        );
        if self.roll == 0.0 {
            return flat;
        }
        let (dx, dy) = self.world_direction(flat.0 - self.pivot.0, flat.1 - self.pivot.1);
        (self.pivot.0 + dx, self.pivot.1 + dy)
    }

    /// A direction on screen (`(x, y)`, +y down: a controller's aim stick) as a world
    /// direction: turned by the camera's roll, since the world is drawn turned the other way.
    #[must_use]
    pub fn world_direction(&self, x: f32, y: f32) -> (f32, f32) {
        if self.roll == 0.0 {
            return (x, y);
        }
        let (sin, cos) = self.roll.sin_cos();
        (cos.mul_add(x, -(sin * y)), sin.mul_add(x, cos * y))
    }

    /// Whether screen point `(x, y)` is inside this view.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "viewport coordinates are small")]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let viewport = self.viewport;
        x >= viewport.x as f32
            && y >= viewport.y as f32
            && x < (viewport.x + viewport.width) as f32
            && y < (viewport.y + viewport.height) as f32
    }
}

/// How a seat aimed this frame (kept for the reticle).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeatAim {
    pub control: WeaponControl,
    /// The world point the mouse aims at, for the mouse seat.
    pub pointer: Option<(f32, f32)>,
}

/// The control a seat sends this frame: the trigger (only with a weapon mounted) and the aim
/// from the pointer, the right stick, or auto-aim around `facing` (-1 left, 1 right).
#[must_use]
pub fn seat_aim(
    input: &PlayerInput,
    player: Option<&Player>,
    facing: i8,
    anchor: Option<&ViewAnchor>,
) -> SeatAim {
    let armed = player.is_some_and(|player| player.mounted_weapon.is_some());
    let fire = input.fire && armed;
    let facing_aim = (if facing < 0 { -127 } else { 127 }, 0);
    let pointer = input
        .pointer
        .zip(anchor)
        .filter(|((x, y), anchor)| anchor.contains(*x, *y))
        .map(|((x, y), anchor)| anchor.world_point(x, y));
    if !input.auto_aim
        && let Some(player) = player
    {
        if let Some((wx, wy)) = pointer
            && let Some((aim_x, aim_y)) = quantize_aim(wx - player.x, wy - (player.y - MUZZLE_RISE))
        {
            return SeatAim {
                control: WeaponControl {
                    fire,
                    auto_aim: false,
                    aim_x,
                    aim_y,
                },
                pointer: Some((wx, wy)),
            };
        }
        // The stick points on screen: through a rolled camera that is a turned world
        // direction (F9).
        let (stick_x, stick_y) = anchor.map_or((input.aim_x, input.aim_y), |anchor| {
            anchor.world_direction(input.aim_x, input.aim_y)
        });
        if let Some((aim_x, aim_y)) = quantize_aim(stick_x, stick_y) {
            return SeatAim {
                control: WeaponControl {
                    fire,
                    auto_aim: false,
                    aim_x,
                    aim_y,
                },
                pointer: None,
            };
        }
    }
    SeatAim {
        control: WeaponControl {
            fire,
            auto_aim: true,
            aim_x: facing_aim.0,
            aim_y: facing_aim.1,
        },
        pointer: None,
    }
}

/// The control a seat on foot sends (Part II F5): the trigger swings the hand pick, aimed from
/// the pilot's chest at the pointer, along the right stick, or (auto) where the pilot faces.
/// `None` when `player_id` is in its rig.
#[must_use]
pub fn pilot_aim(
    input: &PlayerInput,
    world: &WorldState,
    player_id: fatherload_simulation::PlayerId,
    anchor: Option<&ViewAnchor>,
) -> Option<SeatAim> {
    let (_, body, state) = world.pilot_of(player_id)?;
    let (x, y) = state.middle(body);
    let top_down = state.view == fatherload_simulation::pilot::PilotView::TopDown;
    // Seen from above (a crypt, WP-TOMBS T29) the pilot faces where it last walked.
    let facing_aim = if top_down {
        let (hx, hy) = fatherload_simulation::pilot::top_down::heading_of(state);
        quantize_aim(hx, hy).unwrap_or((127, 0))
    } else {
        (if state.facing < 0 { -127 } else { 127 }, 0)
    };
    let pointer = input
        .pointer
        .zip(anchor)
        .filter(|((px, py), anchor)| anchor.contains(*px, *py))
        .map(|((px, py), anchor)| anchor.world_point(px, py));
    let aim = if input.auto_aim {
        None
    } else {
        pointer
            .and_then(|(wx, wy)| quantize_aim(wx - x, wy - y))
            .or_else(|| quantize_aim(input.aim_x, input.aim_y))
    };
    let (aim_x, aim_y) = aim.unwrap_or(facing_aim);
    // Twin-stick in a crypt: pushing the right stick well out fires along it.
    let stick_fires = top_down
        && !input.auto_aim
        && pointer.is_none()
        && input.aim_x.hypot(input.aim_y) >= TWIN_STICK_FIRE;
    Some(SeatAim {
        control: WeaponControl {
            fire: input.fire || stick_fires,
            auto_aim: aim.is_none(),
            aim_x,
            aim_y,
        },
        pointer: if aim.is_some() { pointer } else { None },
    })
}

/// The control a seat sends in space (Wave 12 hostiles): the trigger always fires the ship's
/// guns (the cannon down the nose, which the player aims by turning the craft), and the
/// turret's aim is where the seat points on screen from the craft at the view's middle (the
/// pointer, else the right stick). The camera is locked to the craft, so a screen direction is
/// craft-relative (the screen-relative scheme keeps the chart up, and the simulation reads the
/// aim against the chart then). Never auto-aim; no aim fires the turret forward.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "viewport coordinates are small")]
pub fn space_aim(input: &PlayerInput, anchor: Option<&ViewAnchor>) -> WeaponControl {
    let aim = input
        .pointer
        .zip(anchor)
        .filter(|((x, y), anchor)| anchor.contains(*x, *y))
        .and_then(|((x, y), anchor)| {
            let viewport = anchor.viewport;
            quantize_aim(
                x - (viewport.width as f32).mul_add(0.5, viewport.x as f32),
                y - (viewport.height as f32).mul_add(0.5, viewport.y as f32),
            )
        })
        .or_else(|| quantize_aim(input.aim_x, input.aim_y))
        .unwrap_or((0, 0));
    WeaponControl {
        fire: input.fire,
        auto_aim: false,
        aim_x: aim.0,
        aim_y: aim.1,
    }
}

/// How far out the right stick fires on its own in a top-down crypt (twin-stick).
pub const TWIN_STICK_FIRE: f32 = 0.6;

/// Whether a controller trigger meant as "fire" should thrust instead: the rig has no weapon
/// mounted (the trigger's meaning before weapons).
#[must_use]
pub fn trigger_thrusts(input: &PlayerInput, player: Option<&Player>) -> bool {
    input.fire
        && input.fire_thrusts_unarmed
        && player.is_none_or(|player| player.mounted_weapon.is_none())
}

/// The command the next-weapon button sends: mount the next owned weapon (`None` when the rig
/// owns nothing else to swap to). On foot (`on_foot`, WP-PILOT) it swaps what the pilot holds.
#[must_use]
pub fn cycle_command(
    input: &PlayerInput,
    player: Option<&Player>,
    on_foot: bool,
) -> Option<PlayerCommand> {
    if !input.cycle_weapon {
        return None;
    }
    let player = player?;
    if on_foot {
        return (!player.pilot_arsenal.owned.is_empty()).then_some(PlayerCommand::Pilot(
            fatherload_simulation::pilot::PilotAction::CycleWeapon,
        ));
    }
    let current = player.mounted_weapon.map(|mounted| mounted.kind);
    let next = player.weapon_rack.next_after(current)?;
    (Some(next) != current).then_some(PlayerCommand::MountWeapon { weapon: Some(next) })
}

/// The command the rope button sends on foot (WP-PILOT): fire the grapple along the seat's
/// free aim (zero when auto-aiming: up and forward), clip onto the winch or let go; the
/// simulation picks which.
#[must_use]
pub const fn rope_command(
    input: &PlayerInput,
    aim: &SeatAim,
    on_foot: bool,
) -> Option<PlayerCommand> {
    if !input.rope || !on_foot {
        return None;
    }
    let (aim_x, aim_y) = if aim.control.auto_aim {
        (0, 0)
    } else {
        (aim.control.aim_x, aim.control.aim_y)
    };
    Some(PlayerCommand::Pilot(
        fatherload_simulation::pilot::PilotAction::Rope { aim_x, aim_y },
    ))
}

/// What a view's aim reticle shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reticle {
    /// World point the reticle sits on.
    pub x: f32,
    pub y: f32,
    /// Auto-aim has a target locked (the reticle closes in on it).
    pub locked: bool,
    /// The aim is automatic (no pointer or stick).
    pub auto: bool,
    pub weapon: WeaponKind,
}

/// Distance ahead of the muzzle a stick or facing reticle floats, in pixels.
const RETICLE_REACH: f32 = TILE_SIZE * 4.0;

/// The reticle for `player_id` aiming with `aim` (`None` without a weapon).
#[must_use]
pub fn reticle(
    world: &WorldState,
    player_id: fatherload_simulation::PlayerId,
    aim: &SeatAim,
) -> Option<Reticle> {
    let player = world.player(player_id)?;
    if let Some((_, body, state)) = world.pilot_of(player_id) {
        // On foot a held gun shows its reticle from the pilot's hand (WP-PILOT); the pick
        // shows its cracks on the tile instead, and melee arms need none.
        let weapon = state
            .weapon
            .filter(|weapon| !fatherload_simulation::pilot::arsenal::is_melee(*weapon))?;
        let (hx, hy) = state.middle(body);
        if let Some((x, y)) = aim.pointer {
            return Some(Reticle {
                x,
                y,
                locked: false,
                auto: false,
                weapon,
            });
        }
        let control = aim.control;
        let direction = fatherload_simulation::weapon::aim_vector(control.aim_x, control.aim_y)
            .unwrap_or_else(|| (f32::from(state.facing), 0.0));
        if control.auto_aim
            && let Some((_, x, y)) = world.pilot_auto_aim_target((hx, hy), direction)
        {
            return Some(Reticle {
                x,
                y,
                locked: true,
                auto: true,
                weapon,
            });
        }
        return Some(Reticle {
            x: direction.0.mul_add(RETICLE_REACH, hx),
            y: direction.1.mul_add(RETICLE_REACH, hy),
            locked: false,
            auto: control.auto_aim,
            weapon,
        });
    }
    let weapon = player.mounted_weapon?.kind;
    let base = (player.x, player.y - MUZZLE_RISE);
    // Mirrored controls (a mirror field or hall, a gazer, Wave 9) reverse the aim's sideways
    // part in the simulation, so the reticle is reflected about the muzzle too: it sits where
    // the shot really goes (on a mirrored screen, under the cursor).
    let mirrored = world.rig_controls_mirrored(player_id);
    if let Some((x, y)) = aim.pointer {
        return Some(Reticle {
            x: if mirrored {
                2.0f32.mul_add(base.0, -x)
            } else {
                x
            },
            y,
            locked: false,
            auto: false,
            weapon,
        });
    }
    let mut control = aim.control;
    if mirrored {
        control.aim_x = control.aim_x.saturating_neg();
    }
    let control = control;
    let direction = fatherload_simulation::weapon::aim_vector(control.aim_x, control.aim_y)
        .unwrap_or((1.0, 0.0));
    if control.auto_aim {
        if let Some((_, x, y)) = world.auto_aim_target(player_id, direction) {
            return Some(Reticle {
                x,
                y,
                locked: true,
                auto: true,
                weapon,
            });
        }
        return Some(Reticle {
            x: direction.0.mul_add(RETICLE_REACH, base.0),
            y: direction.1.mul_add(RETICLE_REACH, base.1),
            locked: false,
            auto: true,
            weapon,
        });
    }
    Some(Reticle {
        x: direction.0.mul_add(RETICLE_REACH, base.0),
        y: direction.1.mul_add(RETICLE_REACH, base.1),
        locked: false,
        auto: false,
        weapon,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::WeaponKind;

    fn armed() -> Player {
        let mut player = Player::new(320.0, 320.0);
        let _ = player.receive_weapon(WeaponKind::RivetGun);
        player
    }

    fn anchor() -> ViewAnchor {
        ViewAnchor {
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 640,
                height: 360,
            },
            camera: CameraPosition { x: 0.0, y: 0.0 },
            world_scale: 1.0,
            roll: 0.0,
            pivot: (0.0, 0.0),
        }
    }

    #[test]
    fn a_rolled_camera_turns_the_mouse_and_the_stick_aim_with_the_world() {
        // The rig at the view's centre, leaning a quarter turn clockwise: the world is drawn a
        // quarter turn anticlockwise around it, so the screen's right is the world's down.
        let player = armed();
        let rolled = ViewAnchor {
            camera: CameraPosition {
                x: player.x - 320.0,
                y: player.y - 180.0,
            },
            roll: std::f32::consts::FRAC_PI_2,
            pivot: (player.x, player.y),
            ..anchor()
        };
        let (wx, wy) = rolled.world_point(420.0, 180.0);
        assert!((wx - player.x).abs() < 1e-3, "{wx}");
        assert!((wy - (player.y + 100.0)).abs() < 1e-3, "{wy}");
        let (dx, dy) = rolled.world_direction(1.0, 0.0);
        assert!(dx.abs() < 1e-6 && (dy - 1.0).abs() < 1e-6, "{dx} {dy}");
        let stick = PlayerInput {
            fire: true,
            aim_x: 1.0,
            ..PlayerInput::default()
        };
        let aim = seat_aim(&stick, Some(&player), 1, Some(&rolled));
        assert!(!aim.control.auto_aim);
        assert!(
            aim.control.aim_y > 100 && aim.control.aim_x.abs() < 5,
            "{aim:?}"
        );
        // Level: unchanged.
        assert_eq!(anchor().world_point(10.0, 20.0), (10.0, 20.0));
    }

    #[test]
    fn the_mouse_aims_at_the_world_point_under_it() {
        let player = armed();
        let input = PlayerInput {
            fire: true,
            pointer: Some((320.0, 100.0)),
            ..PlayerInput::default()
        };
        let aim = seat_aim(&input, Some(&player), 1, Some(&anchor()));
        assert!(aim.control.fire && !aim.control.auto_aim);
        assert_eq!(aim.control.aim_x, 0);
        assert!(aim.control.aim_y < -120, "straight up");
        assert_eq!(aim.pointer, Some((320.0, 100.0)));
    }

    #[test]
    fn sticks_aim_freely_and_idle_seats_auto_aim_along_their_facing() {
        let player = armed();
        let stick = PlayerInput {
            aim_x: -0.9,
            aim_y: 0.0,
            ..PlayerInput::default()
        };
        let aim = seat_aim(&stick, Some(&player), 1, None);
        assert!(!aim.control.auto_aim && aim.control.aim_x < -120);
        let idle = seat_aim(&PlayerInput::default(), Some(&player), -1, None);
        assert!(idle.control.auto_aim && idle.control.aim_x == -127);
        // The arrow-keys seat auto-aims even with a pointer around.
        let arrows = PlayerInput {
            auto_aim: true,
            pointer: Some((10.0, 10.0)),
            ..PlayerInput::default()
        };
        assert!(
            seat_aim(&arrows, Some(&player), 1, Some(&anchor()))
                .control
                .auto_aim
        );
    }

    #[test]
    fn unarmed_rigs_never_fire_and_the_pad_trigger_thrusts_for_them() {
        let unarmed = Player::new(0.0, 0.0);
        let trigger = PlayerInput {
            fire: true,
            fire_thrusts_unarmed: true,
            ..PlayerInput::default()
        };
        assert!(!seat_aim(&trigger, Some(&unarmed), 1, None).control.fire);
        assert!(trigger_thrusts(&trigger, Some(&unarmed)));
        assert!(!trigger_thrusts(&trigger, Some(&armed())));
        let mouse = PlayerInput {
            fire: true,
            ..PlayerInput::default()
        };
        assert!(
            !trigger_thrusts(&mouse, Some(&unarmed)),
            "a mouse click never thrusts"
        );
    }

    #[test]
    fn the_next_weapon_button_cycles_owned_weapons() {
        let mut player = armed();
        let press = PlayerInput {
            cycle_weapon: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            cycle_command(&press, Some(&player), false),
            None,
            "only one weapon"
        );
        let _ = player.receive_weapon(WeaponKind::Railgun);
        assert_eq!(
            cycle_command(&press, Some(&player), false),
            Some(PlayerCommand::MountWeapon {
                weapon: Some(WeaponKind::Railgun)
            })
        );
    }

    #[test]
    fn on_foot_the_buttons_swap_personal_arms_and_throw_the_rope() {
        use fatherload_simulation::pilot::PilotAction;
        let mut player = Player::new(320.0, 320.0);
        let press = PlayerInput {
            cycle_weapon: true,
            rope: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            cycle_command(&press, Some(&player), true),
            None,
            "no personal arms yet"
        );
        let _ = player.pilot_arsenal.receive(WeaponKind::Sidearm);
        assert_eq!(
            cycle_command(&press, Some(&player), true),
            Some(PlayerCommand::Pilot(PilotAction::CycleWeapon))
        );
        let aimed = SeatAim {
            control: WeaponControl {
                fire: false,
                auto_aim: false,
                aim_x: 40,
                aim_y: -120,
            },
            pointer: None,
        };
        assert_eq!(
            rope_command(&press, &aimed, true),
            Some(PlayerCommand::Pilot(PilotAction::Rope {
                aim_x: 40,
                aim_y: -120
            }))
        );
        let auto = SeatAim {
            control: WeaponControl {
                auto_aim: true,
                ..aimed.control
            },
            pointer: None,
        };
        assert_eq!(
            rope_command(&press, &auto, true),
            Some(PlayerCommand::Pilot(PilotAction::Rope {
                aim_x: 0,
                aim_y: 0
            })),
            "auto-aiming seats throw up and forward"
        );
        assert_eq!(
            rope_command(&press, &aimed, false),
            None,
            "rig-only key in the rig"
        );
    }

    #[test]
    fn a_mirrored_rigs_reticle_shows_where_the_reversed_aim_really_goes() {
        let player_id = fatherload_simulation::PlayerId::new(0);
        let mut world = fatherload_simulation::WorldState::new_game(player_id);
        let _ = world
            .player_mut(player_id)
            .expect("rig")
            .receive_weapon(WeaponKind::RivetGun);
        let rig = world.player(player_id).expect("rig").clone();
        let base = (rig.x, rig.y - MUZZLE_RISE);
        let pointed = SeatAim {
            control: WeaponControl::default(),
            pointer: Some((base.0 + 100.0, base.1 - 20.0)),
        };
        let stick = SeatAim {
            control: WeaponControl {
                aim_x: 127,
                ..WeaponControl::default()
            },
            pointer: None,
        };
        let plain = reticle(&world, player_id, &pointed).expect("reticle");
        assert!((plain.x - (base.0 + 100.0)).abs() < 0.01);
        let plain_stick = reticle(&world, player_id, &stick).expect("reticle");
        assert!(plain_stick.x > base.0);
        // A mirror room around the rig.
        let frame = world.frame();
        let tile = (frame.col(rig.x), frame.row(rig.y));
        let _ = world.place_field_zone(fatherload_simulation::field::FieldZone::of_preset(
            fatherload_simulation::field::FieldPreset::Mirror,
            tile,
            (12, 12),
        ));
        assert!(world.rig_controls_mirrored(player_id));
        let mirrored = reticle(&world, player_id, &pointed).expect("reticle");
        assert!(
            (mirrored.x - (base.0 - 100.0)).abs() < 0.01,
            "reflected about the muzzle: {}",
            mirrored.x
        );
        assert!((mirrored.y - plain.y).abs() < 0.01);
        let mirrored_stick = reticle(&world, player_id, &stick).expect("reticle");
        assert!(mirrored_stick.x < base.0, "the stick's aim reversed too");
    }
}
