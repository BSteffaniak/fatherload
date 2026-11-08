//! [`NetSimulation`] for the real gameplay world, [`drillgame_simulation::WorldState`].
//!
//! * Controls are the quantized held movement controls ([`ControlState`]: horizontal axis,
//!   thrust, drill).
//! * Actions are the one-shot [`PlayerCommand`]s: bombs, scanner, infrastructure, and the
//!   service/economy transactions. Economy transactions are not predicted (plan §8).
//! * Events carry every [`GameplayEvent`] unchanged, plus one [`WorldEvent::ActionResolved`]
//!   per submitted action so every action's fate is observable exactly once (an ignored action,
//!   e.g. a purchase without credits, produces no gameplay event).
//! * Replication maps one-to-one onto the world's replication sections: per-tick
//!   [`PlayerTickState`] / [`LiveEntities`], the op log ([`WorldOp`]: terrain [`TileChange`]s
//!   and per-player [`Discovery`] reveals) and on-change [`SectionKey`] / [`SectionPayload`]
//!   sections. The keyframe is the world itself.
//! * A player's loadout section and discovery ops go only to that player's client; other
//!   clients get the redacted keyframe and compare the client-visible state hash
//!   ([`WorldState::visible_state_hash`]).
//! * Poses are in tiles, so the netcode's distance tunables (snap threshold, correction epsilon,
//!   relevance radius) are expressed in tiles.

use drillgame_simulation::{
    ControlState, Discovery, GameplayEvent, GameplayEventKind, InputSequence, LiveEntities,
    PlayerCommand, PlayerScopedCommandOutcome, PlayerTickState, SectionKey, SectionPayload,
    SimulationTick, TILE_SIZE, TickAction, TileChange, WorldState, input,
};
use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, PlayerId, Tick};
use crate::simulation::{NetSimulation, Pose, Relevance, TickEvent, TickInputs, TickOutput};

/// Event kind discriminants for [`TickEvent::kind`].
pub mod event_kind {
    /// Offset added to a gameplay event's variant index.
    pub const GAMEPLAY_BASE: u32 = 1;
    /// [`super::WorldEvent::ActionResolved`].
    pub const ACTION_RESOLVED: u32 = 100;
}

/// How a submitted action was resolved by the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionResolution {
    /// The command changed the world.
    Applied,
    /// The command was valid but had no effect (e.g. not enough credits, no bombs left).
    Ignored,
}

/// Events produced by the real world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldEvent {
    /// A gameplay event (audio/vfx cue, notification).
    Gameplay(GameplayEvent),
    /// One submitted action and its outcome.
    ActionResolved {
        /// The action's identity.
        id: ActionId,
        /// The command.
        command: PlayerCommand,
        /// What happened.
        resolution: ActionResolution,
    },
}

/// One op-log entry of the real world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldOp {
    /// A terrain tile write (shared).
    Tile(TileChange),
    /// Tiles one player newly discovered (only that player's map).
    Discovery(Discovery),
}

/// Converts a netcode player id to the simulation's.
#[must_use]
pub fn to_sim_player(id: PlayerId) -> drillgame_simulation::PlayerId {
    drillgame_simulation::PlayerId::new(u64::from(id.0))
}

/// Converts a simulation player id to netcode's (`None` if it does not fit).
#[must_use]
pub fn from_sim_player(id: drillgame_simulation::PlayerId) -> Option<PlayerId> {
    u32::try_from(id.get()).ok().map(PlayerId)
}

/// Builds held controls from analog input.
#[must_use]
pub fn controls(horizontal: f32, thrust: bool, drill_down: bool) -> ControlState {
    ControlState::new(horizontal, thrust, drill_down)
}

const fn gameplay_kind(kind: &GameplayEventKind) -> (u32, Option<drillgame_simulation::PlayerId>) {
    match *kind {
        GameplayEventKind::DrillStarted { player_id } => (0, Some(player_id)),
        GameplayEventKind::PurchaseCompleted { player_id } => (1, Some(player_id)),
        GameplayEventKind::CargoChanged { player_id } => (2, Some(player_id)),
        GameplayEventKind::BombPlaced { player_id } => (3, Some(player_id)),
        GameplayEventKind::InfrastructurePlaced { player_id } => (4, Some(player_id)),
        GameplayEventKind::ScannerUsed { player_id } => (5, Some(player_id)),
        GameplayEventKind::EscapeStarted { player_id } => (6, Some(player_id)),
        GameplayEventKind::EscapeTunnelChanged => (7, None),
        GameplayEventKind::DeepRewardGranted { player_id } => (8, Some(player_id)),
        GameplayEventKind::RescueTriggered { player_id } => (9, Some(player_id)),
        GameplayEventKind::PlayerDamaged { player_id } => (10, Some(player_id)),
        GameplayEventKind::HazardChanged => (11, None),
        GameplayEventKind::BouldersChanged => (12, None),
        GameplayEventKind::BombDetonated => (13, None),
    }
}

const fn drill_mode(target: drillgame_simulation::terrain::TilePosition) -> u32 {
    // Distinct non-zero code per target tile (zero means "not drilling").
    ((target.y.cast_unsigned() << 16) ^ (target.x.cast_unsigned() & 0xFFFF)).wrapping_add(1)
}

impl NetSimulation for WorldState {
    type Controls = ControlState;
    type Action = PlayerCommand;
    type Event = WorldEvent;
    type PlayerState = PlayerTickState;
    type Entities = LiveEntities;
    type Op = WorldOp;
    type SectionKey = SectionKey;
    type Section = SectionPayload;
    type Keyframe = Self;

    fn tick(&self) -> Tick {
        Tick(self.simulation_tick().get())
    }

    fn set_tick(&mut self, tick: Tick) {
        self.set_simulation_tick(SimulationTick::new(tick.0));
    }

    fn tick_rate_hz(&self) -> Option<u32> {
        Some(Self::tick_rate_hz(self))
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        Self::player_ids(self).filter_map(from_sim_player).collect()
    }

    fn add_player(&mut self, id: PlayerId) {
        Self::add_player(self, to_sim_player(id));
    }

    fn remove_player(&mut self, id: PlayerId) {
        Self::remove_player(self, to_sim_player(id));
    }

    fn simulate_tick(
        &mut self,
        inputs: &TickInputs<ControlState, PlayerCommand>,
    ) -> TickOutput<WorldEvent, WorldOp> {
        debug_assert_eq!(
            self.simulation_tick().get().saturating_add(1),
            inputs.tick.0,
            "inputs target the next tick"
        );
        let current = self.simulation_tick();
        let mut sim_inputs = drillgame_simulation::TickInputs::default();
        // Per player: the ids of the actions handed to the world, indexed by sequence.
        let mut submitted: Vec<(drillgame_simulation::PlayerId, Vec<ActionId>)> = Vec::new();
        for player in Self::player_ids(self) {
            let Some(net_player) = from_sim_player(player) else {
                continue;
            };
            let input = inputs.players.get(&net_player);
            let held = input.map(|input| input.controls).unwrap_or_default();
            let mut ids = Vec::new();
            let actions = input
                .map(|input| {
                    input
                        .actions
                        .iter()
                        .map(|envelope| {
                            let sequence =
                                InputSequence::new(u32::try_from(ids.len()).unwrap_or(u32::MAX));
                            ids.push(envelope.id);
                            TickAction {
                                sequence,
                                command: envelope.action.clone(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            sim_inputs.players.insert(
                player,
                input::PlayerTickInput {
                    controls: Some(held),
                    actions,
                },
            );
            submitted.push((player, ids));
        }

        let output = Self::simulate_tick(self, current, &sim_inputs);

        let mut events = Vec::with_capacity(output.events.len() + output.action_outcomes.len());
        for outcome in &output.action_outcomes {
            let Some(id) = submitted
                .iter()
                .find(|(player, _)| *player == outcome.player_id)
                .and_then(|(_, ids)| {
                    usize::try_from(outcome.sequence.get())
                        .ok()
                        .and_then(|index| ids.get(index))
                })
                .copied()
            else {
                continue;
            };
            let command = sim_inputs
                .players
                .get(&outcome.player_id)
                .and_then(|input| {
                    input
                        .actions
                        .iter()
                        .find(|action| action.sequence == outcome.sequence)
                })
                .map(|action| action.command.clone());
            let Some(command) = command else {
                continue;
            };
            events.push(TickEvent {
                player: from_sim_player(outcome.player_id),
                kind: event_kind::ACTION_RESOLVED,
                payload: WorldEvent::ActionResolved {
                    id,
                    command,
                    resolution: if outcome.outcome == PlayerScopedCommandOutcome::Applied {
                        ActionResolution::Applied
                    } else {
                        ActionResolution::Ignored
                    },
                },
            });
        }
        for event in output.events {
            let (index, player) = gameplay_kind(&event.kind);
            events.push(TickEvent {
                player: player.and_then(from_sim_player),
                kind: event_kind::GAMEPLAY_BASE + index,
                payload: WorldEvent::Gameplay(event),
            });
        }
        let mut ops: Vec<WorldOp> = output.terrain_ops.into_iter().map(WorldOp::Tile).collect();
        ops.extend(output.discoveries.into_iter().map(WorldOp::Discovery));
        TickOutput { events, ops }
    }

    fn predicts_action(action: &PlayerCommand) -> bool {
        matches!(
            action,
            PlayerCommand::PlaceBomb
                | PlayerCommand::UseScanner
                | PlayerCommand::PlaceInfrastructure { .. }
        )
    }

    fn player_pose(&self, id: PlayerId) -> Option<Pose> {
        let sim_id = to_sim_player(id);
        let player = self.player(sim_id)?;
        let mode = self
            .active_drill(sim_id)
            .map_or(0, |drill| drill_mode(drill.target));
        Some(Pose::new(player.x / TILE_SIZE, player.y / TILE_SIZE, mode))
    }

    fn player_state(&self, id: PlayerId) -> Option<PlayerTickState> {
        self.player_tick_state(to_sim_player(id))
    }

    fn apply_player_state(&mut self, id: PlayerId, state: PlayerTickState) {
        self.apply_player_tick_state(to_sim_player(id), state);
    }

    fn op_relevant(&self, op: &WorldOp, relevance: &Relevance) -> bool {
        match op {
            WorldOp::Tile(_) => true,
            // Discovery is private to its player.
            WorldOp::Discovery(discovery) => relevance
                .viewer
                .is_some_and(|viewer| to_sim_player(viewer) == discovery.player_id),
        }
    }

    fn entities(&self, relevance: &Relevance) -> LiveEntities {
        if relevance.is_infinite() {
            return self.live_entities();
        }
        self.live_entities_where(|x, y| relevance.contains(x / TILE_SIZE, y / TILE_SIZE))
    }

    fn apply_entities(&mut self, entities: LiveEntities) {
        self.apply_live_entities(entities);
    }

    fn apply_op(&mut self, op: &WorldOp) {
        match op {
            WorldOp::Tile(change) => {
                let _applied = self.apply_tile_change(change);
            }
            WorldOp::Discovery(discovery) => self.apply_discovery(discovery),
        }
    }

    fn section_keys(&self) -> Vec<SectionKey> {
        Self::section_keys(self)
    }

    fn section_hash(&self, key: &SectionKey) -> Option<u64> {
        Self::section_hash(self, *key)
    }

    fn section(&self, key: &SectionKey) -> Option<SectionPayload> {
        Self::section(self, *key)
    }

    fn apply_section(&mut self, key: &SectionKey, section: SectionPayload) {
        Self::apply_section(self, *key, section);
    }

    fn remove_section(&mut self, key: &SectionKey) {
        Self::remove_section(self, *key);
    }

    fn section_visible(&self, key: &SectionKey, viewer: Option<PlayerId>) -> bool {
        key.visible_to(viewer.map(to_sim_player))
    }

    fn keyframe(&self) -> Self {
        self.clone()
    }

    fn keyframe_for(&self, viewer: Option<PlayerId>) -> Self {
        self.redacted_for(viewer.map(to_sim_player))
    }

    fn from_keyframe(keyframe: Self) -> Self {
        keyframe
    }

    fn state_hash(&self) -> u64 {
        self.simulation_state_hash().get()
    }

    fn visible_state_hash(&self, viewer: Option<PlayerId>) -> u64 {
        Self::visible_state_hash(self, viewer.map(to_sim_player)).get()
    }
}
