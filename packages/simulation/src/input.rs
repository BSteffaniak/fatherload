//! Per-tick simulation inputs: quantized held controls plus one-shot actions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::{command::PlayerCommand, ids::InputSequence};
use crate::{command::TickAction, ids::PlayerId};

/// Held controls, quantized so every peer feeds the simulation identical values.
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

/// One player's input for one tick: held controls and one-shot actions.
///
/// A player without controls (absent from [`TickInputs`] or `controls: None`) is not moved this
/// tick.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlayerTickInput {
    pub controls: Option<ControlState>,
    pub actions: Vec<TickAction>,
}

/// Every player's input for one tick, in deterministic order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickInputs {
    pub players: BTreeMap<PlayerId, PlayerTickInput>,
}

#[cfg(test)]
impl TickInputs {
    /// Inputs holding `controls` for `player`.
    pub(crate) fn held(player: PlayerId, controls: ControlState) -> Self {
        Self::default().hold(player, controls)
    }

    /// Adds `controls` for `player`.
    pub(crate) fn hold(mut self, player: PlayerId, controls: ControlState) -> Self {
        self.players.entry(player).or_default().controls = Some(controls);
        self
    }

    /// Adds a one-shot `command` for `player`, sequenced after its previous actions.
    pub(crate) fn act(mut self, player: PlayerId, command: PlayerCommand) -> Self {
        let input = self.players.entry(player).or_default();
        let sequence = InputSequence::new(u32::try_from(input.actions.len()).unwrap_or(u32::MAX));
        input.actions.push(TickAction { sequence, command });
        self
    }
}
