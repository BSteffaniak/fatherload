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
}

impl ViewAnchor {
    /// The world point under screen point `(x, y)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "viewport coordinates are small")]
    pub fn world_point(&self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.world_scale.max(f32::EPSILON);
        (
            (x - self.viewport.x as f32) / scale + self.camera.x,
            (y - self.viewport.y as f32) / scale + self.camera.y,
        )
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
        if let Some((aim_x, aim_y)) = quantize_aim(input.aim_x, input.aim_y) {
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

/// Whether a controller trigger meant as "fire" should thrust instead: the rig has no weapon
/// mounted (the trigger's meaning before weapons).
#[must_use]
pub fn trigger_thrusts(input: &PlayerInput, player: Option<&Player>) -> bool {
    input.fire
        && input.fire_thrusts_unarmed
        && player.is_none_or(|player| player.mounted_weapon.is_none())
}

/// The command the next-weapon button sends: mount the next owned weapon (`None` when the rig
/// owns nothing else to swap to).
#[must_use]
pub fn cycle_command(input: &PlayerInput, player: Option<&Player>) -> Option<PlayerCommand> {
    if !input.cycle_weapon {
        return None;
    }
    let player = player?;
    let current = player.mounted_weapon.map(|mounted| mounted.kind);
    let next = player.weapon_rack.next_after(current)?;
    (Some(next) != current).then_some(PlayerCommand::MountWeapon { weapon: Some(next) })
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
    let weapon = player.mounted_weapon?.kind;
    let base = (player.x, player.y - MUZZLE_RISE);
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
        }
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
            cycle_command(&press, Some(&player)),
            None,
            "only one weapon"
        );
        let _ = player.receive_weapon(WeaponKind::Railgun);
        assert_eq!(
            cycle_command(&press, Some(&player)),
            Some(PlayerCommand::MountWeapon {
                weapon: Some(WeaponKind::Railgun)
            })
        );
    }
}
