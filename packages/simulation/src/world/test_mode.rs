//! Test mode on the world: setting it up, the per-tick refills, and the warp action (see
//! [`crate::test_mode`]).

use crate::{
    economy::max_out_upgrades,
    ids::PlayerId,
    player::Player,
    rig::RigPartKind,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    test_mode::{TEST_MODE_CREDITS, TestModeAction, TestModeState},
    worldgen::{Band, strata::band_solid_top_row},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Half-width in tiles of the pocket a warp carves (3 tiles wide).
const POCKET_HALF_WIDTH: i32 = 1;
/// Height in tiles of the pocket a warp carves.
const POCKET_HEIGHT: i32 = 2;
/// How far either side of the rig a warp looks for a column clear of fixtures.
const WARP_SEARCH_COLUMNS: i32 = 48;

/// Gives `player` the test-mode loadout: every upgrade maxed and a large balance.
fn equip_for_testing(player: &mut Player) {
    max_out_upgrades(player);
    player.credits = player.credits.max(TEST_MODE_CREDITS);
}

impl WorldState {
    /// The world's test-mode toggles, or `None` outside test mode.
    #[must_use]
    pub const fn test_mode(&self) -> Option<TestModeState> {
        self.test_mode
    }

    /// Whether this is a test-mode world.
    #[must_use]
    pub const fn is_test_mode(&self) -> bool {
        self.test_mode.is_some()
    }

    /// Turns a world being built (before play starts) into a test-mode world: every rig, and
    /// every rig that joins later, starts with maxed upgrades and a large balance, and the parts
    /// locker holds every rig part.
    pub fn enable_test_mode(&mut self) {
        // Test mode is for exploring: start invulnerable with infinite fuel (both can be
        // switched off in the pause menu to test real survival).
        self.test_mode.get_or_insert(TestModeState {
            invulnerable: true,
            infinite_fuel: true,
        });
        equip_for_testing(&mut self.player_template);
        for player in self.players.values_mut() {
            equip_for_testing(player);
        }
        self.progression.rig_part_inventory.extend(RigPartKind::ALL);
    }

    pub(super) fn apply_test_mode_action(
        &mut self,
        player_id: PlayerId,
        action: TestModeAction,
    ) -> PlayerScopedCommandOutcome {
        let Some(state) = self.test_mode.as_mut() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match action {
            TestModeAction::SetInvulnerable(on) => state.invulnerable = on,
            TestModeAction::SetInfiniteFuel(on) => state.infinite_fuel = on,
            TestModeAction::WarpToBand(band) => return self.warp_to_band(player_id, band),
        }
        self.apply_test_mode_refills();
        PlayerScopedCommandOutcome::Applied
    }

    /// Refills hulls and fuel tanks when the test-mode toggles say so (end of every tick, so a
    /// rig never shows damage or an empty tank between ticks).
    pub(super) fn apply_test_mode_refills(&mut self) {
        let Some(state) = self.test_mode else {
            return;
        };
        for player in self.players.values_mut() {
            if state.invulnerable {
                player.hull = player.max_hull();
            }
            if state.infinite_fuel {
                player.fuel = player.fuel_capacity;
            }
        }
    }

    /// Moves `player_id`'s rig into a safe pocket carved at the top of `band`, near its column.
    fn warp_to_band(&mut self, player_id: PlayerId, band: Band) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let (x, y) = if band == Band::Topsoil {
            // The top of the topsoil is the surface: back to the spawn point.
            (self.player_template.x, self.player_template.y)
        } else {
            let column = player.tile_position(TILE_SIZE).x;
            let Some((column, top)) = self.warp_pocket_site(band, column) else {
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            };
            self.carve_warp_pocket(band, column, top);
            #[allow(
                clippy::cast_precision_loss,
                reason = "small tile coordinates convert to pixels"
            )]
            let position = (
                TILE_SIZE.mul_add(column as f32, TILE_SIZE * 0.5),
                // Resting on the pocket floor.
                TILE_SIZE.mul_add(
                    (top + POCKET_HEIGHT) as f32,
                    -(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
                ),
            );
            position
        };
        self.set_active_drill(player_id, None);
        self.deep_run_states.remove(&player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.y = y;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// The column nearest `column` whose pocket area at the top of `band` holds no fixtures
    /// (points of interest, foundations), and the pocket's top row there.
    fn warp_pocket_site(&self, band: Band, column: i32) -> Option<(i32, i32)> {
        let settings = *self.terrain.settings();
        let margin = POCKET_HALF_WIDTH + 2;
        let last_column = self.terrain.width() - 1 - margin;
        if last_column < margin {
            return None;
        }
        let start = column.clamp(margin, last_column);
        (0..=WARP_SEARCH_COLUMNS)
            .flat_map(|offset| [start - offset, start + offset])
            .filter(|candidate| (margin..=last_column).contains(candidate))
            .find_map(|candidate| {
                let top = (candidate - POCKET_HALF_WIDTH..=candidate + POCKET_HALF_WIDTH)
                    .map(|x| band_solid_top_row(&settings, band, x))
                    .max()?;
                let floor = top + POCKET_HEIGHT;
                if floor + 1 >= self.terrain.height() {
                    return None;
                }
                let clear = Self::pocket_area(candidate, top).all(|position| {
                    !matches!(
                        self.terrain.tile(position).map(|tile| tile.kind),
                        Some(TileKind::Poi(_) | TileKind::Foundation)
                    )
                });
                clear.then_some((candidate, top))
            })
    }

    /// The pocket at `(column, top)` with a one-tile margin all round.
    fn pocket_area(column: i32, top: i32) -> impl Iterator<Item = TilePosition> {
        let columns = column - POCKET_HALF_WIDTH - 1..=column + POCKET_HALF_WIDTH + 1;
        let rows = top - 1..=top + POCKET_HEIGHT + 1;
        rows.flat_map(move |y| columns.clone().map(move |x| TilePosition { x, y }))
    }

    /// Carves the warp pocket: open air inside, the band's rock for the floor, and every hazard
    /// in the margin made safe.
    fn carve_warp_pocket(&mut self, band: Band, column: i32, top: i32) {
        let floor = top + POCKET_HEIGHT;
        let rock = band.soft_material();
        for position in Self::pocket_area(column, top) {
            let inside = (position.x - column).abs() <= POCKET_HALF_WIDTH
                && (top..floor).contains(&position.y);
            let kind = self.terrain.tile(position).map(|tile| tile.kind);
            let replacement = if inside {
                (kind != Some(TileKind::Air)).then_some(TileKind::Air)
            } else if position.y == floor && (position.x - column).abs() <= POCKET_HALF_WIDTH {
                (!kind.is_some_and(|kind| {
                    matches!(
                        kind,
                        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
                    )
                }))
                .then_some(rock)
            } else {
                kind.is_some_and(|kind| {
                    matches!(
                        kind,
                        TileKind::Lava
                            | TileKind::Gas
                            | TileKind::ExplosivePocket
                            | TileKind::PressurePocket
                            | TileKind::MagmaVent
                    )
                })
                .then_some(rock)
            };
            if let Some(replacement) = replacement {
                let _written = self.terrain.set_kind(position, replacement);
            }
        }
    }
}
