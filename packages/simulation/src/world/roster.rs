//! Who is in the world: adding, spawning and removing players.
//!
//! # Spawn placement
//!
//! A new rig spawns at the first free slot of an unbounded spawn grid around the player
//! template's position: slots run right of the spawn point (then left of it) across the whole map
//! width, one row at a time, and further rows stack upward into the sky. Slots inside solid
//! terrain are skipped. A slot is free when no existing rig is within [`SPAWN_SPACING`] of it, so
//! rigs never spawn overlapping each other
//! however many join (a 240-tile-wide map holds well over a thousand spawn slots), and slots
//! vacated by rigs that moved away are reused. Only when every slot is occupied does a rig spawn
//! on the least crowded slot. Placement is O(slots tried x players) at join time, never per tick.

use crate::{ids::PlayerId, player::Player, terrain::TILE_SIZE};

use super::{
    WorldState,
    movement::{
        AUTHORITATIVE_MIN_PLAYER_Y, AUTHORITATIVE_PLAYER_RADIUS, clamp_authoritative_player_x,
    },
};

/// Distance between neighbouring spawn slots (two tiles; more than a rig's diameter).
pub const SPAWN_SPACING: f32 = 2.0 * TILE_SIZE;

/// How far above the spawn point the slot grid stacks rows of slots, in tiles.
pub const SPAWN_SKY_ROWS_TILES: f32 = 12.0;

const _: () = assert!(SPAWN_SPACING >= 2.0 * AUTHORITATIVE_PLAYER_RADIUS);

impl WorldState {
    /// Adds a player built from the player template at the first free spawn slot (see the
    /// module docs). Does nothing if the player exists.
    pub fn add_player(&mut self, player_id: PlayerId) {
        if self.players.contains_key(&player_id) {
            return;
        }
        let mut player = self.player_template.clone();
        let (x, y) = self.free_spawn_position(player.x, player.y);
        player.x = x;
        player.x_remainder = 0.0;
        player.y = y;
        self.insert_player(player_id, player);
    }

    /// Inserts a fully built player (save migration and scenario setup), creating its
    /// per-player bookkeeping.
    pub fn insert_player(&mut self, player_id: PlayerId, player: Player) {
        self.players.insert(player_id, player);
        self.equipped_rig_parts.entry(player_id).or_default();
        self.mechanic_levels.entry(player_id).or_default();
        self.scanner_cooldowns.entry(player_id).or_insert(0.0);
        self.scanner_cooldown_deadlines
            .entry(player_id)
            .or_insert(self.simulation_tick);
        self.discovered_tiles.entry(player_id).or_default();
    }

    /// Removes a player and every piece of per-player state.
    pub fn remove_player(&mut self, player_id: PlayerId) {
        self.remove_player_with(player_id, true);
    }

    /// [`Self::remove_player`], optionally keeping the player's armed remote charges (parking
    /// keeps them for when the player returns, WP-PERSIST).
    #[allow(
        clippy::too_many_lines,
        reason = "names every world field so per-player state cannot be forgotten"
    )]
    pub(super) fn remove_player_with(&mut self, player_id: PlayerId, disarm_charges: bool) {
        // Leaving mid-run abandons it (WP-TOMBS): the unbanked loot is lost.
        let mut scratch = crate::event::TickOutput::default();
        self.abandon_tomb_run(player_id, &mut scratch);
        let Self {
            timing: _,
            player_template: _,
            simulation_tick: _,
            won_game: _,
            players,
            equipped_rig_parts,
            mechanic_levels,
            world_event_severity: _,
            world_event_days_remaining: _,
            deep_run_states,
            lost_cargo_sites,
            rescue_counts,
            progression,
            scan_markers: _,
            collapse_warnings: _,
            entities,
            terrain: _,
            infrastructure: _,
            active_drills,
            active_drill_timers,
            scanner_cooldowns,
            scanner_cooldown_deadlines,
            next_bomb_id: _,
            rng: _,
            discovered_tiles,
            service_transactions,
            test_mode: _,
            enemies,
            bases: _,
            crew,
            clock,
            people,
            // Deeds, homes and furniture stay keyed by the player id (a returning player owns
            // their house again).
            homes: _,
            // Runs were left above; unlocks stay keyed by the player id.
            tombs: _,
            // Zones belong to places, not players.
            fields: _,
            fluids: _,
            // Beacons, built lifts and charges stay keyed by the player id.
            logistics,
            crust,
            // A leaving player's dose and mutation go with them (Wave 8).
            old_deep,
            // A leaving rig's root axes go with it; the Hive and the Mind belong to the world.
            living_dark,
            living_schedule: _,
            // The expedition's trail is the crew's, not a player's.
            expedition: _,
            // A leaving rig's pings fade with it; the fights belong to the world (Wave 9).
            drowned,
            // A leaving player's exposure, anchor and ride go with them (Wave 9).
            hollow,
            // A leaving rig's gaze and rot go with it; halls and landmarks stay (Wave 9).
            distortion,
            // A leaving rig's lamp filter and amber go with it (Wave 9); the rest is the
            // world's.
            far_strange,
            far_schedule: _,
            // A leaving rig's ride and alarm go with it; its archivist waits for it (Wave 10).
            ancient_city,
            city_schedule: _,
            // A leaving rig's defences stand where they were built (Wave 10); the gate and the
            // fight are the world's.
            garrison,
            garrison_schedule,
            // A leaving rig's float and well go with it; the Star Drive and the fight are the
            // crew's (Wave 10).
            heart,
            places,
            place_pass: _,
        } = self;
        // A removed player's place goes with it; a parked one keeps it (F11).
        places.forget(player_id);
        garrison.forget(player_id);
        garrison_schedule.refused.remove(&player_id);
        heart.forget(player_id);
        hollow.forget(player_id);
        distortion.forget(player_id);
        logistics.forget_transient(player_id);
        crust.forget(player_id);
        old_deep.forget(player_id);
        living_dark.forget(player_id);
        drowned.forget(player_id);
        far_strange.forget(player_id);
        ancient_city.forget(player_id);
        players.remove(&player_id);
        equipped_rig_parts.remove(&player_id);
        mechanic_levels.remove(&player_id);
        deep_run_states.remove(&player_id);
        lost_cargo_sites.remove(&player_id);
        rescue_counts.remove(&player_id);
        progression.deep_reward_milestones.remove(&player_id);
        progression
            .crew_quests
            .active
            .retain(|quest| quest.owner != player_id);
        active_drills.remove(&player_id);
        active_drill_timers.remove(&player_id);
        scanner_cooldowns.remove(&player_id);
        scanner_cooldown_deadlines.remove(&player_id);
        discovered_tiles.remove(&player_id);
        crew.members.remove(&player_id);
        // Nobody is left to trigger a departed player's remote charges: they are disarmed.
        entities.retain(|entity| {
            !disarm_charges
                || !matches!(entity.state, crate::entity::EntityState::Charge(charge)
                if charge.owner == Some(player_id)
                    && charge.kind.remote()
                    && charge.detonation_tick == crate::explosive::REMOTE_FUSE)
        });
        // A departed player's pilot on foot leaves with them (parking keeps it first).
        entities.retain(|entity| {
            !matches!(entity.state, crate::entity::EntityState::Pilot(pilot)
                if pilot.owner == player_id)
        });
        service_transactions.retain(|transaction| transaction.player_id != player_id);
        enemies.rig_guard.remove(&player_id);
        clock.sleepers.remove(&player_id);
        // Characters remember a departed player (a parked player keeps their id and returns to
        // a world that knows them); only the open conversation ends.
        let _ = people.end_conversation(player_id);
    }

    /// The first spawn slot at least [`SPAWN_SPACING`] away from every rig, or the least crowded
    /// slot when the grid is full.
    pub(super) fn free_spawn_position(&self, base_x: f32, base_y: f32) -> (f32, f32) {
        let base_x = clamp_authoritative_player_x(&self.terrain, base_x);
        let max_x = clamp_authoritative_player_x(&self.terrain, f32::MAX);
        let right = slot_count(max_x - base_x);
        let frame = self.frame();
        let left = slot_count(frame.legacy_x(base_x)).saturating_sub(1);
        // Rows of slots stacked up into the sky above the spawn point (the sky has no ceiling,
        // so the search height is fixed).
        let rows = slot_count(SPAWN_SKY_ROWS_TILES * TILE_SIZE).min(slot_count(
            frame.legacy_y(base_y) - AUTHORITATIVE_MIN_PLAYER_Y,
        ));
        let mut best = (base_x, base_y);
        let mut best_clearance = f32::NEG_INFINITY;
        for row in 0..rows {
            let y = row_offset(row).mul_add(-SPAWN_SPACING, base_y);
            for column in 0..right + left {
                let x = if column < right {
                    row_offset(column).mul_add(SPAWN_SPACING, base_x)
                } else {
                    row_offset(column - right + 1).mul_add(-SPAWN_SPACING, base_x)
                };
                if self.spawn_slot_buried(x, y) {
                    continue;
                }
                let clearance = self.spawn_clearance(x, y);
                if clearance >= SPAWN_SPACING * SPAWN_SPACING {
                    return (x, y);
                }
                if clearance > best_clearance {
                    best_clearance = clearance;
                    best = (x, y);
                }
            }
        }
        best
    }

    /// Whether the slot at `(x, y)` lies inside a non-air tile (a rig spawned there would be
    /// stuck in the ground).
    fn spawn_slot_buried(&self, x: f32, y: f32) -> bool {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "world positions are converted to terrain tile coordinates"
        )]
        let tile = self.frame().tile_at(x, y);
        self.terrain
            .tile(tile)
            .is_some_and(|tile| !tile.kind.is_open())
    }

    /// Squared distance from `(x, y)` to the nearest rig (infinite with no rigs).
    fn spawn_clearance(&self, x: f32, y: f32) -> f32 {
        self.players
            .values()
            .map(|player| {
                let (dx, dy) = (player.x - x, player.y - y);
                dx.mul_add(dx, dy * dy)
            })
            .fold(f32::INFINITY, f32::min)
    }
}

/// Number of slots that fit in `span` world units, counting the one at its start.
fn slot_count(span: f32) -> u32 {
    if span.is_nan() || span < 0.0 {
        return 1;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative world span divided by the slot spacing is a small count"
    )]
    let slots = (span / SPAWN_SPACING).floor().min(f32::from(u16::MAX)) as u32;
    slots + 1
}

const fn row_offset(index: u32) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "slot indices are bounded by the map size"
    )]
    let offset = index as f32;
    offset
}

#[cfg(test)]
mod tests {
    use super::SPAWN_SPACING;
    use crate::{
        PlayerId, WorldState,
        terrain::{TILE_SIZE, TileKind},
    };

    fn assert_spread(world: &WorldState) {
        let rigs: Vec<_> = world.players().values().collect();
        for (index, first) in rigs.iter().enumerate() {
            let (left, right) = (
                f64::from(world.terrain().min_x()) * f64::from(TILE_SIZE),
                f64::from(world.terrain().max_x() + 1) * f64::from(TILE_SIZE),
            );
            assert!(f64::from(first.x) >= left && f64::from(first.x) <= right);
            let tile = world
                .terrain()
                .tile(first.tile_in(crate::position::Frame::FIXED));
            assert!(
                tile.is_none_or(|tile| tile.kind == TileKind::Air),
                "rig at ({}, {}) spawned inside {tile:?}",
                first.x,
                first.y
            );
            for second in &rigs[index + 1..] {
                let distance = (first.x - second.x).hypot(first.y - second.y);
                assert!(
                    distance >= SPAWN_SPACING - 0.01,
                    "rigs at ({}, {}) and ({}, {}) overlap",
                    first.x,
                    first.y,
                    second.x,
                    second.y
                );
            }
        }
    }

    #[test]
    fn forty_players_spawn_without_overlap() {
        let mut world = WorldState::new_game(PlayerId::new(0));
        for id in 1..40 {
            world.add_player(PlayerId::new(id));
        }
        assert_eq!(world.player_count(), 40);
        assert_spread(&world);
    }

    #[test]
    fn spawn_grid_wraps_into_more_rows_on_a_narrow_map() {
        let mut import = crate::WorldImport::new_game(PlayerId::new(0));
        import.terrain = crate::terrain::Terrain::new_seeded(12, 40, 3);
        import.player.x = 6.0 * TILE_SIZE;
        let mut world = WorldState::from_import(&import);
        world.set_player_template(import.player);
        for id in 1..40 {
            world.add_player(PlayerId::new(id));
        }
        assert_eq!(world.player_count(), 40);
        assert_spread(&world);
    }

    #[test]
    fn slots_inside_solid_terrain_are_skipped() {
        let mut world = WorldState::new_game(PlayerId::new(0));
        let spawn = world
            .player_template()
            .tile_in(crate::position::Frame::FIXED);
        // Bury the first two slots right of the spawn point.
        for offset in [2, 4] {
            let tile = crate::terrain::TilePosition {
                x: spawn.x + offset,
                y: spawn.y,
            };
            assert!(world.terrain_mut().set_kind(tile, TileKind::Stone));
        }
        for id in 1..12 {
            world.add_player(PlayerId::new(id));
        }
        assert_spread(&world);
    }

    #[test]
    fn vacated_slots_are_reused() {
        let mut world = WorldState::new_game(PlayerId::new(0));
        world.add_player(PlayerId::new(1));
        let slot = world.player(PlayerId::new(1)).map(|rig| (rig.x, rig.y));
        world.remove_player(PlayerId::new(1));
        world.add_player(PlayerId::new(7));
        assert_eq!(
            world.player(PlayerId::new(7)).map(|rig| (rig.x, rig.y)),
            slot
        );
    }
}
