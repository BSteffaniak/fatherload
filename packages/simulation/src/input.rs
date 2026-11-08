//! Per-tick simulation inputs: quantized held controls plus one-shot actions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    command::{PlayerCommand, SequencedPlayerCommand},
    ids::{InputSequence, PlayerId},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ControlState {
    horizontal_milli: i16,
    pub thrust: bool,
    pub drill_down: bool,
}

impl ControlState {
    const HORIZONTAL_SCALE: f32 = 1_000.0;

    #[must_use]
    pub fn new(horizontal: f32, thrust: bool, drill_down: bool) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped control input is deliberately quantized for deterministic simulation"
        )]
        let horizontal_milli =
            (horizontal.clamp(-1.0, 1.0) * Self::HORIZONTAL_SCALE).round() as i16;
        Self {
            horizontal_milli,
            thrust,
            drill_down,
        }
    }

    #[must_use]
    pub fn horizontal(self) -> f32 {
        f32::from(self.horizontal_milli) / Self::HORIZONTAL_SCALE
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerTickInput {
    pub controls: Option<(InputSequence, ControlState)>,
    pub actions: Vec<SequencedPlayerCommand>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickInputs {
    pub players: BTreeMap<PlayerId, PlayerTickInput>,
}

impl TickInputs {
    #[must_use]
    pub fn from_commands(commands: &[SequencedPlayerCommand]) -> Self {
        let mut inputs = Self::default();
        for command in commands {
            let player = inputs.players.entry(command.player_id).or_default();
            if let PlayerCommand::Movement {
                horizontal,
                thrust,
                drill_down,
            } = command.command
            {
                player.controls = Some((
                    command.sequence,
                    ControlState::new(horizontal, thrust, drill_down),
                ));
            } else {
                player.actions.push(command.clone());
            }
        }
        inputs
    }

    #[must_use]
    pub fn command_count(&self) -> usize {
        self.players
            .values()
            .map(|input| usize::from(input.controls.is_some()) + input.actions.len())
            .sum()
    }
}
