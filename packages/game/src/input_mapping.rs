//! Maps a seat's sampled [`PlayerInput`] to gameplay intent: held controls plus one-shot actions.

use fatherload_simulation::{
    ControlState, ExplosiveKind, Player, PlayerCommand, ThrowAim, explosive::Deployment,
};

use crate::{input::PlayerInput, runtime::SeatInput};

/// Held movement controls for a seat.
#[must_use]
pub fn gameplay_controls(input: &PlayerInput) -> ControlState {
    ControlState::new(
        input.horizontal.clamp(-1.0, 1.0),
        input.thrust,
        input.drill_down,
    )
}

/// A seat's explosive selection and aim: which carried kind the bomb button deploys and which
/// way the rig last faced (throws go that way when no direction is held).
///
/// Presentation-side seat state: the chosen kind and aim travel inside each
/// [`PlayerCommand::DeployExplosive`], so the simulation never needs to know the selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplosiveSelection {
    pub selected: ExplosiveKind,
    /// -1 facing left, 1 facing right.
    pub facing: i8,
    /// The aim the latest frame's input gave the selected kind (drawn as the aim preview).
    pub last_aim: ThrowAim,
}

impl Default for ExplosiveSelection {
    fn default() -> Self {
        Self::new()
    }
}

impl ExplosiveSelection {
    /// Plain bombs selected, facing right.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selected: ExplosiveKind::Bomb,
            facing: 1,
            last_aim: ThrowAim::from_digital(0, 0, 1),
        }
    }

    /// Follows one frame of gameplay input: the facing tracks the stick, and the cycle button
    /// moves to the next kind `player` carries (plain bombs are always in the cycle). Returns
    /// whether the selection changed.
    pub fn observe(&mut self, input: &PlayerInput, player: Option<&Player>) -> bool {
        if input.horizontal.abs() > 0.2 {
            self.facing = if input.horizontal < 0.0 { -1 } else { 1 };
        }
        let changed = if input.cycle_explosive {
            let next = next_carried(self.selected, player);
            let changed = next != self.selected;
            self.selected = next;
            changed
        } else {
            false
        };
        // Out of the selected kind (the last one just went): fall forward to the next kind
        // carried, so the bomb button never silently does nothing.
        if let Some(player) = player
            && player.explosive_count(self.selected) == 0
            && self.selected != ExplosiveKind::Bomb
        {
            self.selected = next_carried(self.selected, Some(player));
        }
        self.last_aim = self.aim(input);
        changed
    }

    /// The aim for a throw or placement of the selected kind this frame: the free-aim stick
    /// when pushed, otherwise the held directions and facing. Placed charges go straight at
    /// the nearest surface: down when drilling down, up when thrusting, else ahead.
    #[must_use]
    pub fn aim(self, input: &PlayerInput) -> ThrowAim {
        let stick = ThrowAim::from_vector(input.aim_x, input.aim_y);
        if self.selected.deployment() == Deployment::Place {
            if let Some(stick) = stick {
                let (x, y) = stick.vector();
                return cardinal(x, y);
            }
            return if input.drill_down {
                ThrowAim::DOWN
            } else if input.thrust && input.horizontal.abs() <= 0.2 {
                ThrowAim::UP
            } else if self.facing(input) < 0 {
                ThrowAim::LEFT
            } else {
                ThrowAim::RIGHT
            };
        }
        stick.unwrap_or_else(|| {
            let horizontal = if input.horizontal.abs() > 0.2 {
                if input.horizontal < 0.0 { -1 } else { 1 }
            } else {
                0
            };
            let vertical = if input.drill_down {
                1
            } else {
                -i8::from(input.thrust)
            };
            ThrowAim::from_digital(horizontal, vertical, self.facing)
        })
    }

    fn facing(self, input: &PlayerInput) -> i8 {
        if input.horizontal.abs() > 0.2 {
            if input.horizontal < 0.0 { -1 } else { 1 }
        } else {
            self.facing
        }
    }

    /// The command the bomb button sends for the selected kind.
    #[must_use]
    pub fn deploy_command(self, input: &PlayerInput) -> PlayerCommand {
        if self.selected == ExplosiveKind::Bomb {
            PlayerCommand::PlaceBomb
        } else {
            PlayerCommand::DeployExplosive {
                kind: self.selected,
                aim: self.aim(input),
            }
        }
    }
}

/// The selectable kind after `current` that `player` carries (plain bombs always count).
#[must_use]
pub fn next_carried(current: ExplosiveKind, player: Option<&Player>) -> ExplosiveKind {
    let kinds = ExplosiveKind::SELECTABLE;
    let start = kinds.iter().position(|kind| *kind == current).unwrap_or(0);
    (1..=kinds.len())
        .map(|offset| kinds[(start + offset) % kinds.len()])
        .find(|kind| {
            *kind == ExplosiveKind::Bomb
                || player.is_some_and(|player| player.explosive_count(*kind) > 0)
        })
        .unwrap_or(ExplosiveKind::Bomb)
}

fn cardinal(x: f32, y: f32) -> ThrowAim {
    if x.abs() > y.abs() {
        if x < 0.0 {
            ThrowAim::LEFT
        } else {
            ThrowAim::RIGHT
        }
    } else if y < 0.0 {
        ThrowAim::UP
    } else {
        ThrowAim::DOWN
    }
}

/// One-shot gameplay actions pressed this frame (scanner, explosives, the remote detonator,
/// infrastructure kits).
#[must_use]
pub fn gameplay_actions(input: &PlayerInput, explosives: ExplosiveSelection) -> Vec<PlayerCommand> {
    let mut actions = Vec::new();
    if input.scan {
        actions.push(PlayerCommand::UseScanner);
    }
    if input.bomb {
        actions.push(explosives.deploy_command(input));
    }
    if input.detonate || input.detonate_sequence {
        actions.push(PlayerCommand::RemoteDetonate {
            sequenced: input.detonate_sequence,
        });
    }
    for (pressed, slot) in [
        (input.place_relay, 0),
        (input.place_drone, 1),
        (input.place_lift, 2),
        (input.place_support, 3),
        (input.place_pump, 4),
        (input.place_processor, 5),
    ] {
        if pressed {
            actions.push(PlayerCommand::PlaceInfrastructure { slot });
        }
    }
    actions
}

/// A seat's full gameplay input with plain bombs selected.
#[must_use]
#[cfg(test)]
pub fn seat_input(input: &PlayerInput) -> SeatInput {
    seat_input_with(input, ExplosiveSelection::new())
}

/// A seat's full gameplay input, deploying the seat's selected explosive.
#[must_use]
pub fn seat_input_with(input: &PlayerInput, explosives: ExplosiveSelection) -> SeatInput {
    SeatInput {
        controls: gameplay_controls(input),
        actions: gameplay_actions(input, explosives),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_movement_to_held_controls() {
        let input = PlayerInput {
            horizontal: 2.0,
            thrust: true,
            drill_down: true,
            ..PlayerInput::default()
        };
        let controls = gameplay_controls(&input);
        assert_eq!(controls, ControlState::new(1.0, true, true));
        assert!(gameplay_actions(&input, ExplosiveSelection::new()).is_empty());
    }

    #[test]
    fn idle_input_is_neutral() {
        assert_eq!(
            seat_input(&PlayerInput::default()),
            SeatInput {
                controls: ControlState::default(),
                actions: Vec::new(),
            }
        );
    }

    #[test]
    fn maps_one_shot_tools_to_actions() {
        let input = PlayerInput {
            bomb: true,
            scan: true,
            place_pump: true,
            pause: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            gameplay_actions(&input, ExplosiveSelection::new()),
            vec![
                PlayerCommand::UseScanner,
                PlayerCommand::PlaceBomb,
                PlayerCommand::PlaceInfrastructure { slot: 4 },
            ]
        );
    }

    #[test]
    fn cycling_visits_only_carried_kinds_and_always_offers_bombs() {
        let mut player = Player::new(0.0, 0.0);
        let mut selection = ExplosiveSelection::new();
        let cycle = PlayerInput {
            cycle_explosive: true,
            ..PlayerInput::default()
        };
        assert!(
            !selection.observe(&cycle, Some(&player)),
            "only bombs carried"
        );
        player.add_explosives(ExplosiveKind::C4, 2);
        player.add_explosives(ExplosiveKind::Flare, 1);
        assert!(selection.observe(&cycle, Some(&player)));
        assert_eq!(selection.selected, ExplosiveKind::C4);
        selection.observe(&cycle, Some(&player));
        assert_eq!(selection.selected, ExplosiveKind::Flare);
        selection.observe(&cycle, Some(&player));
        assert_eq!(selection.selected, ExplosiveKind::Bomb);
    }

    #[test]
    fn running_out_of_the_selected_kind_moves_to_the_next_one_carried() {
        let mut player = Player::new(0.0, 0.0);
        player.add_explosives(ExplosiveKind::Sticky, 1);
        player.add_explosives(ExplosiveKind::Flare, 2);
        let mut selection = ExplosiveSelection {
            selected: ExplosiveKind::Sticky,
            ..ExplosiveSelection::new()
        };
        assert!(player.take_explosive(ExplosiveKind::Sticky));
        selection.observe(&PlayerInput::default(), Some(&player));
        assert_eq!(selection.selected, ExplosiveKind::Flare);
        // Plain bombs stay selected even with none carried (the HUD shows zero).
        let mut bombs = ExplosiveSelection::new();
        bombs.observe(&PlayerInput::default(), Some(&Player::new(0.0, 0.0)));
        assert_eq!(bombs.selected, ExplosiveKind::Bomb);
    }

    #[test]
    fn the_bomb_button_deploys_the_selection_along_the_aim_and_the_detonator_fires() {
        let selection = ExplosiveSelection {
            selected: ExplosiveKind::Sticky,
            facing: -1,
            ..ExplosiveSelection::new()
        };
        let throw = PlayerInput {
            bomb: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            gameplay_actions(&throw, selection),
            vec![PlayerCommand::DeployExplosive {
                kind: ExplosiveKind::Sticky,
                aim: ThrowAim::from_digital(0, 0, -1),
            }]
        );
        let stick = PlayerInput {
            bomb: true,
            aim_x: 0.0,
            aim_y: 0.9,
            ..PlayerInput::default()
        };
        assert_eq!(selection.aim(&stick), ThrowAim::DOWN);
        let c4 = ExplosiveSelection {
            selected: ExplosiveKind::C4,
            ..ExplosiveSelection::new()
        };
        let drill = PlayerInput {
            drill_down: true,
            ..PlayerInput::default()
        };
        assert_eq!(c4.aim(&drill), ThrowAim::DOWN);
        assert_eq!(c4.aim(&PlayerInput::default()), ThrowAim::RIGHT);
        let detonate = PlayerInput {
            detonate: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            gameplay_actions(&detonate, selection),
            vec![PlayerCommand::RemoteDetonate { sequenced: false }]
        );
        let sequence = PlayerInput {
            detonate_sequence: true,
            ..PlayerInput::default()
        };
        assert_eq!(
            gameplay_actions(&sequence, selection),
            vec![PlayerCommand::RemoteDetonate { sequenced: true }]
        );
    }

    #[test]
    fn facing_follows_the_stick() {
        let mut selection = ExplosiveSelection::new();
        selection.observe(
            &PlayerInput {
                horizontal: -1.0,
                ..PlayerInput::default()
            },
            None,
        );
        assert_eq!(selection.facing, -1);
        selection.observe(&PlayerInput::default(), None);
        assert_eq!(selection.facing, -1, "kept while idle");
    }
}
