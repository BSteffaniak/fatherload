//! The world's local pixel frame (expansion plan §15 F7, see [`crate::position`]).
//!
//! Every gameplay `f32` position is relative to the terrain's [`Frame`]. A floating frame
//! re-centres on its anchor rig as the last step of every tick ([`WorldState::recentre_frame`]);
//! [`WorldState::move_frame`] shifts every stored local position by the same whole number of
//! sectors, so nothing moves in absolute terms.

use crate::{
    ids::PlayerId,
    position::{Frame, FrameShift, Reframe, WorldPos},
};

use super::WorldState;

impl WorldState {
    /// The local pixel frame every position in this world is measured in.
    #[must_use]
    pub const fn frame(&self) -> Frame {
        self.terrain.frame()
    }

    /// The absolute position of local pixel `(x, y)`.
    #[must_use]
    pub fn world_pos(&self, x: f32, y: f32) -> WorldPos {
        self.frame().world(x, y)
    }

    /// The local pixel of absolute position `position`.
    #[must_use]
    pub fn local_pos(&self, position: WorldPos) -> (f32, f32) {
        self.frame().local(position)
    }

    /// The absolute position of `player_id`'s actor (the pilot on foot, else the rig).
    #[must_use]
    pub fn actor_world_pos(&self, player_id: PlayerId) -> Option<WorldPos> {
        self.actor_position(player_id)
            .map(|(x, y)| self.world_pos(x, y))
    }

    /// Switches this world to a floating frame (or back to a fixed one) before play: test
    /// and tool worlds far from the origin, and new-game options that opt in. Nothing moves;
    /// a floating frame re-centres at the end of the next tick.
    pub fn set_floating_frame(&mut self, floating: bool) {
        let frame = self.frame();
        if floating {
            self.terrain
                .set_frame(Frame::floating_at(frame.sector_x, frame.sector_y));
        } else {
            // A fixed frame lives at the origin: move everything back there.
            self.move_frame(Frame::FIXED);
        }
    }

    /// Re-expresses every position of this world in `frame`, keeping its floating flag as
    /// given (tools and tests: the screenshot `--frame-shift` check renders a scene in a frame
    /// off the origin to prove presentation is frame-relative). Nothing moves in absolute terms.
    #[doc(hidden)]
    pub fn relocate_frame(&mut self, frame: Frame) {
        self.move_frame(frame);
    }

    /// Places `player_id`'s rig (and the frame, in a floating world) at absolute position
    /// `position`, for tests and tools that start far from the origin. The frame re-centres on
    /// the rig at once, so the rig's local position is exact.
    pub fn teleport_rig_to(&mut self, player_id: PlayerId, position: WorldPos) {
        let frame = self.frame();
        if frame.floating() {
            self.move_frame(Frame::floating_at(position.sector_x, position.sector_y));
        }
        let (x, y) = self.local_pos(position);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.y = y;
            player.x_remainder = 0.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
    }

    /// The anchor a floating frame follows: the first player's actor (the pilot on foot, else
    /// the rig), in player id order. Co-op players far from the anchor keep `f32` precision
    /// relative to it (many simultaneous frames are F11's).
    fn frame_anchor(&self) -> Option<(f32, f32)> {
        let anchor = *self.players.keys().next()?;
        self.actor_position(anchor)
    }

    /// Re-centres a floating frame on its anchor once the anchor drifts past
    /// [`crate::position::RECENTRE_SECTORS`] (the last step of every tick). A fixed frame never
    /// moves.
    pub(crate) fn recentre_frame(&mut self) {
        let frame = self.frame();
        if !frame.floating() {
            return;
        }
        let Some((x, y)) = self.frame_anchor() else {
            return;
        };
        if let Some(next) = frame.recentred_on(x, y) {
            self.move_frame(next);
        }
    }

    /// Moves the local pixel space to `next`, shifting every stored local position so nothing
    /// moves in absolute terms (replicas call it when the authority's frame arrives).
    pub(crate) fn move_frame(&mut self, next: Frame) {
        let shift = self.frame().shift_to(next);
        self.terrain.set_frame(next);
        if !shift.is_zero() {
            self.reframe_positions(shift);
        }
    }

    /// Shifts every local position in the world. Every field is named so new state is placed
    /// on one side or the other: positions move, everything else (tiles, timers, counters,
    /// lengths) does not.
    fn reframe_positions(&mut self, shift: FrameShift) {
        let Self {
            timing: _,
            player_template,
            simulation_tick: _,
            won_game: _,
            players,
            equipped_rig_parts: _,
            mechanic_levels: _,
            world_event_severity: _,
            world_event_days_remaining: _,
            deep_run_states: _,
            lost_cargo_sites,
            rescue_counts: _,
            // Contracts, expeditions and settled ground are tile based.
            progression: _,
            // Tile positions.
            scan_markers: _,
            collapse_warnings: _,
            entities,
            // Tiles are absolute; the frame itself was set by the caller.
            terrain: _,
            infrastructure: _,
            // Drill targets are tiles.
            active_drills: _,
            active_drill_timers: _,
            scanner_cooldowns: _,
            scanner_cooldown_deadlines: _,
            next_bomb_id: _,
            rng: _,
            discovered_tiles: _,
            service_transactions: _,
            test_mode: _,
            // Lairs, cooldowns and the night ledger are tile and tick based.
            enemies: _,
            // Claims, modules and fixtures are tiles.
            bases: _,
            crew,
            clock: _,
            people: _,
            homes: _,
            // Runs, sites and seals are tiles and ticks (their actors are entities).
            tombs: _,
            // Field zones are tile rectangles (F9), sampled through the frame.
            fields: _,
            // The fluid schedule is woken tiles (F9 fluids live in the terrain).
            fluids: _,
            // Lifts, outposts and beacons are tiles; a ride pins its rider every tick.
            logistics: _,
            crust: _,
            // Dose meters, halls, digs and the fights are tiles and ticks (Wave 8).
            old_deep: _,
            // Living Dark state is tiles, ticks and entity ids (Wave 8).
            living_dark: _,
            living_schedule: _,
            // The expedition's trail is counts and region keys.
            expedition: _,
            // Pings are tiles; the Leviathan's moves are offsets from its arena (Wave 9).
            drowned: _,
            // The Hollow World's state is ticks, meters, entity ids and world times; the
            // serpent's lunge mark is moved with the frame below.
            hollow,
        } = self;
        hollow.serpent.reframe_mark(shift);
        player_template.reframe(shift);
        players.reframe(shift);
        lost_cargo_sites.reframe(shift);
        entities.reframe_all(shift);
        for parked in crew.parked.values_mut() {
            parked.player.reframe(shift);
            parked.lost_cargo.reframe(shift);
            if let Some(pilot) = &mut parked.pilot {
                pilot.body.reframe(shift);
                if let Some(rope) = &mut pilot.state.tether {
                    shift.apply(&mut rope.anchor_x, &mut rope.anchor_y);
                }
            }
        }
    }
}
