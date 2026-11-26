//! Test mode on the world: setting it up, the per-tick refills, and the warp action (see
//! [`crate::test_mode`]).

use crate::{
    ids::PlayerId,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    test_mode::{TestModeAction, TestModeState},
    worldgen::{Band, strata::band_solid_top_row},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Half-width in tiles of the pocket a warp carves (3 tiles wide).
const POCKET_HALF_WIDTH: i32 = 1;
/// Height in tiles of the pocket a warp carves.
const POCKET_HEIGHT: i32 = 2;
/// How far either side of the rig a warp looks for a column clear of fixtures.
const WARP_SEARCH_COLUMNS: i32 = 48;

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
    /// every rig that joins later, starts fully stocked (Wave 14: every upgrade maxed, every
    /// consumable at its stack, every part, gear, cosmetic, gate reward, space line and star
    /// chart; see `world/test_stock.rs`) with a large balance.
    pub fn enable_test_mode(&mut self) {
        // Test mode is for exploring: start invulnerable with infinite fuel and supplies (each
        // can be switched off in the pause menu to test real survival).
        self.test_mode.get_or_insert(TestModeState::new_world());
        self.stock_everything();
    }

    pub(super) fn apply_test_mode_action(
        &mut self,
        player_id: PlayerId,
        action: TestModeAction,
    ) -> PlayerScopedCommandOutcome {
        if self.test_mode.is_none() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        // The god-mode toolkit (Wave 14, `world/test_stock.rs`).
        if let Some(outcome) = self.apply_toolkit_action(player_id, action) {
            return outcome;
        }
        let Some(state) = self.test_mode.as_mut() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match action {
            TestModeAction::SetInvulnerable(on) => state.invulnerable = on,
            TestModeAction::SetInfiniteFuel(on) => state.infinite_fuel = on,
            TestModeAction::WarpToBand(band) => return self.warp_to_band(player_id, band),
            TestModeAction::WarpToPlace(place) => return self.warp_to_place(player_id, place),
            TestModeAction::PlaceFieldZone(preset) => {
                return self.place_dev_field_zone(player_id, preset);
            }
            TestModeAction::ClearFieldZones => self.fields.clear(),
            TestModeAction::GrantStarDrive => return self.grant_star_drive(player_id),
            // Space combat (Wave 12 hostiles, `world/hostiles.rs`).
            TestModeAction::SpawnHostiles(faction) => {
                return self.test_spawn_hostiles(player_id, faction, false);
            }
            TestModeAction::SpawnBoss(faction) => {
                return self.test_spawn_hostiles(player_id, faction, true);
            }
            TestModeAction::SpawnRaid(faction) => return self.test_spawn_raid(player_id, faction),
            // The galaxy story (Wave 13 story, `world/galaxy_story.rs`).
            TestModeAction::WarpToStory => return self.warp_to_story(player_id),
            // The destination index (Wave 14, `world/destinations.rs`).
            TestModeAction::TravelTo(request) => {
                return self.order_travel(player_id, super::TravelOrder::To(request));
            }
            TestModeAction::TravelBack => {
                return self.order_travel(player_id, super::TravelOrder::Back);
            }
            TestModeAction::BookmarkHere => {
                return self.order_travel(player_id, super::TravelOrder::BookmarkHere);
            }
            TestModeAction::Bookmark(id) => {
                return self.order_travel(player_id, super::TravelOrder::Bookmark(id));
            }
            TestModeAction::RemoveBookmark(index) => {
                return self.order_travel(player_id, super::TravelOrder::RemoveBookmark(index));
            }
            // The Wave 14 console's travel and stocking (`world/travel.rs`).
            TestModeAction::PaletteTravel(destination) => {
                return self.palette_travel_to(player_id, destination);
            }
            TestModeAction::PaletteStock(request) => {
                return self.apply_stock_request(player_id, request);
            }
            TestModeAction::PourTestPools => {
                let Some(at) = self
                    .players
                    .get(&player_id)
                    .map(|player| player.tile_in(self.terrain.frame()))
                else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                // The basins' open top rows sit level with the rig, just to its right.
                let origin = TilePosition {
                    x: at.x + 1,
                    y: at.y - 4,
                };
                return if self.pour_demo_basins(origin) {
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                };
            }
            TestModeAction::SetNoClip(on) => {
                let was_on = state.no_clip;
                state.no_clip = on;
                if was_on && !on {
                    self.free_rigs_from_rock();
                }
                if on {
                    // Flight takes over: any drill in progress stops.
                    self.active_drills.clear();
                    self.active_drill_timers.clear();
                }
            }
            // Handled by `apply_toolkit_action` above.
            TestModeAction::SetInfiniteSupplies(_)
            | TestModeAction::Restock
            | TestModeAction::StockAll
            | TestModeAction::Give(..)
            | TestModeAction::SetStoryUnlocked(_)
            | TestModeAction::SetGateCleared { .. }
            | TestModeAction::SetEndingDone(_)
            | TestModeAction::SetTimeOfDay { .. }
            | TestModeAction::SetDayLock(_)
            | TestModeAction::SetWeather(_)
            | TestModeAction::SpawnCreature(_)
            | TestModeAction::ClearNearbyEnemies
            | TestModeAction::SetAiFrozen(_) => {}
        }
        self.apply_test_mode_refills();
        PlayerScopedCommandOutcome::Applied
    }

    /// Refills hulls, fuel tanks and consumables when the test-mode toggles say so (end of
    /// every tick, so a rig never shows damage, an empty tank or a spent stack between ticks).
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
                player.weapon.energy = player.battery_capacity();
            }
        }
        if state.infinite_supplies {
            let players: Vec<PlayerId> = self.players.keys().copied().collect();
            for player_id in players {
                self.top_up_supplies(player_id);
            }
        }
        // The story unlock reaches a rig that joined, or was away when it was switched on.
        if state.story_unlocked {
            for player in self.players.values_mut() {
                super::test_stock::unlock_rig_logs(player);
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
            let column = player.tile_in(self.frame()).x;
            let Some((column, top)) = self.warp_pocket_site(band, column) else {
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            };
            self.carve_warp_pocket(band, column, top);
            #[allow(
                clippy::cast_precision_loss,
                reason = "small tile coordinates convert to pixels"
            )]
            let position = (
                TILE_SIZE.mul_add(self.frame().rel_col(column), TILE_SIZE * 0.5),
                // Resting on the pocket floor (frame-local like the column: a floating
                // frame away from the origin row must not push the rig a sector deeper).
                TILE_SIZE.mul_add(
                    self.frame().rel_row(top + POCKET_HEIGHT),
                    -(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
                ),
            );
            position
        };
        self.set_active_drill(player_id, None);
        self.deep_run_states.remove(&player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.x_remainder = 0.0;
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
        let first_column = self.terrain.min_x() + margin;
        let last_column = self.terrain.max_x() - margin;
        if last_column < first_column {
            return None;
        }
        let start = column.clamp(first_column, last_column);
        (0..=WARP_SEARCH_COLUMNS)
            .flat_map(|offset| [start - offset, start + offset])
            .filter(|candidate| (first_column..=last_column).contains(candidate))
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
                        Some(
                            TileKind::Poi(_)
                                | TileKind::Foundation
                                | TileKind::Bedrock
                                | TileKind::Built(_)
                        )
                    )
                });
                clear.then_some((candidate, top))
            })
    }

    /// The pocket at `(column, top)` with a one-tile margin all round.
    pub(super) fn pocket_area(column: i32, top: i32) -> impl Iterator<Item = TilePosition> {
        let columns = column - POCKET_HALF_WIDTH - 1..=column + POCKET_HALF_WIDTH + 1;
        let rows = top - 1..=top + POCKET_HEIGHT + 1;
        rows.flat_map(move |y| columns.clone().map(move |x| TilePosition { x, y }))
    }

    /// Carves the warp pocket: open air inside, the band's rock for the floor, and every hazard
    /// in the margin made safe.
    pub(super) fn carve_warp_pocket(&mut self, band: Band, column: i32, top: i32) {
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
