//! Maps a seat's sampled [`PlayerInput`] to gameplay intent: held controls plus one-shot actions.

use drillgame_simulation::{ControlState, PlayerCommand};

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

/// One-shot gameplay actions pressed this frame (scanner, bomb, infrastructure kits).
#[must_use]
pub fn gameplay_actions(input: &PlayerInput) -> Vec<PlayerCommand> {
    let mut actions = Vec::new();
    if input.scan {
        actions.push(PlayerCommand::UseScanner);
    }
    if input.bomb {
        actions.push(PlayerCommand::PlaceBomb);
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

/// A seat's full gameplay input.
#[must_use]
pub fn seat_input(input: &PlayerInput) -> SeatInput {
    SeatInput {
        controls: gameplay_controls(input),
        actions: gameplay_actions(input),
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
        assert!(gameplay_actions(&input).is_empty());
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
            gameplay_actions(&input),
            vec![
                PlayerCommand::UseScanner,
                PlayerCommand::PlaceBomb,
                PlayerCommand::PlaceInfrastructure { slot: 4 },
            ]
        );
    }
}
