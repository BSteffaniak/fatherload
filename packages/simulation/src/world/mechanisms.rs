//! Ruin mechanisms in the world (WP-PILOT; rules and tiles in [`crate::mechanism`]).
//!
//! * [`PilotAction::Use`](crate::pilot::PilotAction::Use) on foot works the nearest lever,
//!   closed chest or keycard door within [`USE_REACH`] ([`WorldState::pilot_usable`] tells
//!   the game what the prompt should say).
//! * Pressure plates ([`WorldState::advance_plates`], once a tick after the pilots move): a
//!   pilot's feet or a resting rig presses a plate; its channel's gates open, and close again
//!   [`crate::mechanism::PLATE_HOLD_SECONDS`] after the last weight left (never onto a body:
//!   a blocked gate waits).
//! * Scenario helper [`WorldState::stamp_tiles`] writes hand-made ruins (tests and screenshot
//!   scenes); generators stamp them through [`crate::worldgen::pilot_spaces`].

use crate::{
    base::BuiltTile,
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    item::ItemStack,
    mechanism::{
        CHANNELS, MAX_GATE_HEIGHT, MechanismEvent, PLATE_HOLD_SECONDS, REACH_COLUMNS, REACH_ROWS,
        USE_REACH, Usable, chest_loot, keycard_item,
    },
    pilot::PILOT_HALF_WIDTH,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Keycard doors this tall (tiles) or less open together.
const MAX_KEYCARD_DOOR_TILES: usize = 8;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

#[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
fn tile_centre(tile: TilePosition) -> (f32, f32) {
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}

impl WorldState {
    /// The nearest thing `player_id`'s pilot could use now: `(tile, what)`.
    #[must_use]
    pub fn pilot_usable(&self, player_id: PlayerId) -> Option<(TilePosition, Usable)> {
        let (_, body, state) = self.pilot_of(player_id)?;
        let (mx, my) = state.middle(body);
        let centre = tile_at(mx, my);
        let mut best: Option<(f32, TilePosition, Usable)> = None;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let tile = TilePosition {
                    x: centre.x + dx,
                    y: centre.y + dy,
                };
                let Some(usable) = self
                    .terrain
                    .tile(tile)
                    .and_then(|found| Usable::of(found.kind))
                else {
                    continue;
                };
                let (tx, ty) = tile_centre(tile);
                let distance = (tx - mx).hypot(ty - my);
                if distance <= USE_REACH && best.is_none_or(|(nearest, ..)| distance < nearest) {
                    best = Some((distance, tile, usable));
                }
            }
        }
        best.map(|(_, tile, usable)| (tile, usable))
    }

    /// Whether the use button should work the fixture in reach rather than board the rig
    /// that is also in reach: the nearer of the two wins (a lever beside the parked rig can
    /// still be pulled, and stepping toward the rig boards it). `false` with no fixture in
    /// reach.
    #[must_use]
    pub fn pilot_fixture_first(&self, player_id: PlayerId) -> bool {
        let Some((tile, _)) = self.pilot_usable(player_id) else {
            return false;
        };
        if !self.can_board_rig(player_id) {
            return true;
        }
        let (Some((_, body, state)), Some(player)) =
            (self.pilot_of(player_id), self.players.get(&player_id))
        else {
            return true;
        };
        let (mx, my) = state.middle(body);
        let (tx, ty) = tile_centre(tile);
        (tx - mx).hypot(ty - my) < (player.x - mx).hypot(player.y - my)
    }

    /// [`crate::pilot::PilotAction::Use`]: works the nearest lever, chest or keycard door.
    pub(super) fn pilot_use(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some((tile, usable)) = self.pilot_usable(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match usable {
            Usable::Lever { channel, on } => {
                let flipped = !on;
                if !flipped && self.gates_blocked(tile, channel) {
                    // A gate would close on someone: the lever will not budge.
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let _ = self.terrain.set_kind(
                    tile,
                    TileKind::Built(BuiltTile::Lever {
                        channel,
                        on: flipped,
                    }),
                );
                output.push_event(GameplayEventKind::MechanismUsed {
                    player_id: Some(player_id),
                    x: tile.x,
                    y: tile.y,
                    mechanism: MechanismEvent::LeverPulled { on: flipped },
                });
                self.set_gates(tile, channel, flipped, Some(player_id), output);
            }
            Usable::Chest => self.open_chest(tile, player_id, output),
            Usable::TombDoor { .. }
            | Usable::TombShrine { .. }
            | Usable::TombStairs
            | Usable::TombExit
            | Usable::TombCache => {
                return self.use_tomb_fixture(player_id, tile, usable, output);
            }
            Usable::TempleFitting(fitting) => {
                if !self.work_temple_fitting(player_id, tile, fitting, output) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
            }
            Usable::KeycardDoor { color } => {
                // The card counts in the pack or on the rig's keyring: boarding empties the
                // pack into the rig, and a pilot who hops back in between the chest and the
                // door must not be locked out of its own find.
                let carried = self.players.get(&player_id).is_some_and(|player| {
                    let card = keycard_item(color);
                    player.pilot_pack.count(card) > 0 || player.carried(card) > 0
                });
                let mechanism = if carried {
                    self.open_keycard_door(tile, color);
                    MechanismEvent::KeycardAccepted { color }
                } else {
                    MechanismEvent::Locked { color }
                };
                output.push_event(GameplayEventKind::MechanismUsed {
                    player_id: Some(player_id),
                    x: tile.x,
                    y: tile.y,
                    mechanism,
                });
            }
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// The gate lintels of `channel` in reach of mechanism tile `at`, in tile order: only those
    /// in the mechanism's own space ([`MechanismScope`]), so a lever by the wall of one pilot
    /// space (or tomb run slot) never works a gate of the next one over.
    fn lintels_near(&self, at: TilePosition, channel: u8) -> Vec<TilePosition> {
        let mut found = Vec::new();
        let scope = self.mechanism_scope(at);
        for y in at.y - REACH_ROWS..=at.y + REACH_ROWS {
            for x in at.x - REACH_COLUMNS..=at.x + REACH_COLUMNS {
                let tile = TilePosition { x, y };
                if let Some(TileKind::Built(BuiltTile::GateLintel(lintel))) =
                    self.terrain.tile(tile).map(|found| found.kind)
                    && lintel % CHANNELS == channel % CHANNELS
                    && self.mechanism_scope(tile) == scope
                {
                    found.push(tile);
                }
            }
        }
        found
    }

    /// The space mechanism tile `tile` belongs to: a generated pilot space (ruin, rig vault,
    /// crawlway cave), a tomb run slot, or the open world.
    fn mechanism_scope(&self, tile: TilePosition) -> MechanismScope {
        let settings = self.terrain.settings();
        if let Some(slot) = crate::worldgen::tomb_sites::slot_at(settings, tile) {
            return MechanismScope::RunSlot(slot);
        }
        crate::worldgen::pilot_spaces::spaces_touching(
            settings,
            &crate::worldgen::features::TileRect {
                min: tile,
                max: tile,
            },
        )
        .first()
        .map_or(MechanismScope::Open, |space| {
            MechanismScope::PilotSpace(space.origin)
        })
    }

    /// The tiles of the gate under `lintel`: its bars when closed, the air down to the floor
    /// when open (at most [`MAX_GATE_HEIGHT`]).
    fn gate_column(&self, lintel: TilePosition, open: bool) -> Vec<TilePosition> {
        let want = if open {
            TileKind::Built(BuiltTile::Gate)
        } else {
            TileKind::Air
        };
        (1..=MAX_GATE_HEIGHT)
            .map(|dy| TilePosition {
                x: lintel.x,
                y: lintel.y + dy,
            })
            .take_while(|tile| self.terrain.tile(*tile).map(|found| found.kind) == Some(want))
            .collect()
    }

    /// Whether a pilot or rig stands in any open gate of `channel` near `at` (closing it then
    /// would trap them in the bars).
    fn gates_blocked(&self, at: TilePosition, channel: u8) -> bool {
        self.lintels_near(at, channel).into_iter().any(|lintel| {
            self.gate_column(lintel, false)
                .into_iter()
                .any(|tile| self.body_in_tile(tile))
        })
    }

    /// Whether a pilot or a rig overlaps `tile`.
    fn body_in_tile(&self, tile: TilePosition) -> bool {
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let (left, top) = (tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE);
        let (right, bottom) = (left + TILE_SIZE, top + TILE_SIZE);
        let pilots = self.pilots().any(|(_, body, state)| {
            body.x + PILOT_HALF_WIDTH > left
                && body.x - PILOT_HALF_WIDTH < right
                && body.y > top
                && body.y - state.height() < bottom
        });
        pilots
            || self.players.values().any(|player| {
                player.hull > 0.0
                    && player.x + AUTHORITATIVE_PLAYER_RADIUS > left
                    && player.x - AUTHORITATIVE_PLAYER_RADIUS < right
                    && player.y + AUTHORITATIVE_PLAYER_RADIUS > top
                    && player.y - AUTHORITATIVE_PLAYER_RADIUS < bottom
            })
    }

    /// Opens (or closes) every gate of `channel` in reach of `at`. Returns how many changed.
    fn set_gates(
        &mut self,
        at: TilePosition,
        channel: u8,
        open: bool,
        player_id: Option<PlayerId>,
        output: &mut TickOutput,
    ) -> usize {
        let mut changed = 0;
        for lintel in self.lintels_near(at, channel) {
            let column = self.gate_column(lintel, open);
            if column.is_empty() {
                continue;
            }
            let kind = if open {
                TileKind::Air
            } else {
                TileKind::Built(BuiltTile::Gate)
            };
            for tile in &column {
                let _ = self.terrain.set_kind(*tile, kind);
            }
            changed += 1;
            output.push_event(GameplayEventKind::MechanismUsed {
                player_id,
                x: lintel.x,
                y: lintel.y,
                mechanism: if open {
                    MechanismEvent::GateOpened
                } else {
                    MechanismEvent::GateClosed
                },
            });
        }
        changed
    }

    fn open_chest(&mut self, tile: TilePosition, player_id: PlayerId, output: &mut TickOutput) {
        let Some(TileKind::Built(BuiltTile::Chest { open: false, card })) =
            self.terrain.tile(tile).map(|found| found.kind)
        else {
            return;
        };
        let _ = self.terrain.set_kind(
            tile,
            TileKind::Built(BuiltTile::Chest {
                open: true,
                card: None,
            }),
        );
        let (x, y) = tile_centre(tile);
        let seed = self.terrain.settings().seed;
        let mut loot = chest_loot(seed, tile);
        if let Some(color) = card {
            loot.push(ItemStack::new(keycard_item(color), 1));
        }
        // V10 (WP-TOMBS): some ruin chests hold a tomb key.
        if self.terrain.settings().generator.has_tombs()
            && super::tombs::chest_holds_tomb_key(seed, tile)
        {
            loot.push(ItemStack::new(crate::tomb::TOMB_KEY, 1));
        }
        for (index, stack) in loot.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a few stacks")]
            let spread = (index as f32 - 1.0) * 45.0;
            let _ = self.spawn_loot(x, TILE_SIZE.mul_add(-0.6, y), (spread, -160.0), stack, None);
        }
        output.push_event(GameplayEventKind::ChestOpened {
            player_id,
            x: tile.x,
            y: tile.y,
        });
    }

    /// Slides open the keycard door at `tile`: every connected door tile of its colour.
    fn open_keycard_door(&mut self, tile: TilePosition, color: u8) {
        let door = TileKind::Built(BuiltTile::KeycardDoor(color));
        let mut stack = vec![tile];
        let mut opened = Vec::new();
        while let Some(next) = stack.pop() {
            if opened.len() >= MAX_KEYCARD_DOOR_TILES
                || opened.contains(&next)
                || self.terrain.tile(next).map(|found| found.kind) != Some(door)
            {
                continue;
            }
            opened.push(next);
            for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                stack.push(TilePosition {
                    x: next.x + dx,
                    y: next.y + dy,
                });
            }
        }
        opened.sort();
        for tile in opened {
            let _ = self.terrain.set_kind(tile, TileKind::Air);
        }
    }

    /// The tiles weight rests on this tick: under each pilot's feet and each rig.
    fn plate_weights(&self) -> Vec<TilePosition> {
        let mut weights: Vec<TilePosition> = self
            .pilots()
            .flat_map(|(_, body, _)| {
                let feet = body.y + 1.5;
                [
                    tile_at(body.x - PILOT_HALF_WIDTH + 2.0, feet),
                    tile_at(body.x + PILOT_HALF_WIDTH - 2.0, feet),
                ]
            })
            .collect();
        weights.extend(
            self.players
                .values()
                .filter(|player| player.hull > 0.0)
                .map(|player| tile_at(player.x, player.y + AUTHORITATIVE_PLAYER_RADIUS + 2.0)),
        );
        weights.sort();
        weights.dedup();
        weights
    }

    /// Presses and releases pressure plates (see the module docs). Runs once a tick.
    pub(super) fn advance_plates(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        // Plates exist only in generated ruins (V9) or stamped scenarios (pressed by a pilot).
        let scan =
            self.terrain.settings().generator.has_pilot_spaces() || self.pilots().next().is_some();
        if !scan && self.bases.plates().is_empty() {
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few seconds in milliseconds"
        )]
        let hold = self
            .timing
            .ticks_for_millis((PLATE_HOLD_SECONDS * 1_000.0) as u64)
            .max(1);
        let until = SimulationTick::new(next_tick.get().saturating_add(hold));
        let weights = if scan {
            self.plate_weights()
        } else {
            Vec::new()
        };
        for tile in weights {
            let Some(TileKind::Built(BuiltTile::PressurePlate { channel, down })) =
                self.terrain.tile(tile).map(|found| found.kind)
            else {
                continue;
            };
            self.bases.plates_mut().hold(tile, until);
            if !down {
                let _ = self.terrain.set_kind(
                    tile,
                    TileKind::Built(BuiltTile::PressurePlate {
                        channel,
                        down: true,
                    }),
                );
                output.push_event(GameplayEventKind::MechanismUsed {
                    player_id: None,
                    x: tile.x,
                    y: tile.y,
                    mechanism: MechanismEvent::PlatePressed,
                });
                self.set_gates(tile, channel, true, None, output);
            }
        }
        if self.bases.plates().is_empty() {
            return;
        }
        for tile in self.bases.plates_mut().take_due(next_tick) {
            let Some(TileKind::Built(BuiltTile::PressurePlate { channel, .. })) =
                self.terrain.tile(tile).map(|found| found.kind)
            else {
                continue;
            };
            if self.gates_blocked(tile, channel) {
                // Someone stands in the gateway: try again in a moment.
                let retry = SimulationTick::new(next_tick.get().saturating_add(hold / 8 + 1));
                self.bases.plates_mut().hold(tile, retry);
                continue;
            }
            let _ = self.terrain.set_kind(
                tile,
                TileKind::Built(BuiltTile::PressurePlate {
                    channel,
                    down: false,
                }),
            );
            output.push_event(GameplayEventKind::MechanismUsed {
                player_id: None,
                x: tile.x,
                y: tile.y,
                mechanism: MechanismEvent::PlateReleased,
            });
            self.set_gates(tile, channel, false, None, output);
        }
    }

    /// Writes `cells` into the terrain as they are (tests, screenshot scenes and scripted set
    /// pieces: hand-made ruins). Returns the tiles written.
    pub fn stamp_tiles(&mut self, cells: &[(TilePosition, TileKind)]) -> Vec<TilePosition> {
        cells
            .iter()
            .filter(|(tile, kind)| self.terrain.set_kind(*tile, *kind))
            .map(|(tile, _)| *tile)
            .collect()
    }
}

/// Where a mechanism works ([`WorldState::mechanism_scope`]): levers and plates reach only the
/// gates of their own space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MechanismScope {
    /// Outside every generated space (scenes, player builds).
    Open,
    /// A generated pilot space, by its layout's origin.
    PilotSpace(TilePosition),
    /// A tomb run slot of the vault.
    RunSlot(u8),
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use crate::{
        SimulationParams,
        worldgen::{GeneratorVersion, WorldSettings, tomb_sites},
    };

    /// Writes a closed gate (lintel and three bars) of `channel` under `lintel`.
    fn gate(world: &mut WorldState, lintel: TilePosition, channel: u8) {
        let _ = world
            .terrain
            .set_kind_by_rule(lintel, TileKind::Built(BuiltTile::GateLintel(channel)));
        for dy in 1..=3 {
            let _ = world.terrain.set_kind_by_rule(
                TilePosition {
                    x: lintel.x,
                    y: lintel.y + dy,
                },
                TileKind::Built(BuiltTile::Gate),
            );
        }
    }

    fn closed(world: &WorldState, lintel: TilePosition) -> bool {
        world
            .terrain
            .tile(TilePosition {
                x: lintel.x,
                y: lintel.y + 1,
            })
            .map(|found| found.kind)
            == Some(TileKind::Built(BuiltTile::Gate))
    }

    #[test]
    fn a_lever_never_reaches_a_gate_in_the_neighbouring_space() {
        let settings = WorldSettings {
            generator: GeneratorVersion::V10,
            ..WorldSettings::new(7)
        };
        let mut world = WorldState::new_game_with_settings(
            PlayerId::new(0),
            &settings,
            SimulationParams::new(60),
        );
        // A lever in the last column of run slot 0, a gate of its channel three columns into
        // slot 1 (well inside the lever's reach), and one of its own in slot 0.
        let next = tomb_sites::slot_origin(1);
        let row = next.y + 20;
        let lever = TilePosition {
            x: next.x - 1,
            y: row,
        };
        let theirs = TilePosition {
            x: next.x + 3,
            y: row - 3,
        };
        let ours = TilePosition {
            x: next.x - 4,
            y: row - 3,
        };
        let _ = world.terrain.stream_around(&[lever]);
        assert_eq!(tomb_sites::slot_at(&settings, lever), Some(0));
        assert_eq!(tomb_sites::slot_at(&settings, theirs), Some(1));
        assert!(crate::mechanism::in_reach(lever, theirs));
        gate(&mut world, theirs, 2);
        gate(&mut world, ours, 2);
        let _ = world.terrain.set_kind_by_rule(
            lever,
            TileKind::Built(BuiltTile::Lever {
                channel: 2,
                on: false,
            }),
        );
        let mut output = TickOutput::default();
        let changed = world.set_gates(lever, 2, true, None, &mut output);
        assert_eq!(changed, 1, "only the gate of its own slot");
        assert!(!closed(&world, ours), "its own gate opened");
        assert!(closed(&world, theirs), "the next slot's gate stayed shut");
    }
}
