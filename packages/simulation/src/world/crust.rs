//! The authored Crust in the tick (Wave 8, see [`crate::crust`]): ground grip on mud and
//! slate, mine-cart rides, timber sets that fail, slate shelves that slide, strongboxes, the
//! lava tubes' heat, the Sinkhole set piece and the Deep Claim gate.
//!
//! Every rule runs only on a Deep Descent world ([`crust_rules`]); older worlds never enter
//! this module's state, so their hashes and replays are unchanged.

use std::collections::BTreeSet;

use crate::{
    crust::{
        self, CART_BOARD_SPEED, CartRide, CartStep, CartStick, CrustState, CrustTile,
        DRILL_II_LEVEL, GroundGrip, MUD_LIFT_SHARE, MUD_TOP_SPEED_SHARE,
        SINKHOLE_CAUGHT_HULL_SHARE, SINKHOLE_FRONT_ROWS_PER_SECOND, SINKHOLE_RUMBLE_MILLIS,
        SLICK_DRAG_PER_SECOND, SLIDE_MAX_TILES, SLIDE_WARNING_MILLIS, STRONGBOX_CREDITS,
        SUPPORT_CREAK_MILLIS, SUPPORT_ROOF_ROWS, SUPPORT_SPAN, SinkholeRun, crust_rules,
    },
    depth_progression::{GATE_DAMAGE_PER_SECOND, missing_gate},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    quantize::quantize_simulation_scalar,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    WorldState,
    hazards::BOULDER_RADIUS,
    movement::{AUTHORITATIVE_PLAYER_RADIUS, authoritative_player_collides},
};

/// Life of a boulder shaken loose by the Crust's collapses.
const COLLAPSE_BOULDER_LIFE_MILLIS: u64 = 3_600;
/// Millis between the boulders the Sinkhole's walls shed while the collapse climbs.
const SINKHOLE_BOULDER_MILLIS: u64 = 450;

/// The y (local pixels of `frame`) a rig rests at whose floor's top is at row `top` (tile
/// rows, fractional).
fn resting_y_on(frame: crate::position::Frame, top: f64) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame rows are small near the rig"
    )]
    let y = (top - frame.origin_row() as f64).mul_add(
        f64::from(TILE_SIZE),
        -f64::from(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
    );
    #[allow(clippy::cast_possible_truncation, reason = "world pixels fit f32")]
    quantize_simulation_scalar(y as f32)
}

/// The x (local pixels of `frame`) of a point `column` tiles from the world's origin.
fn x_at(frame: crate::position::Frame, column: f64) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "frame columns are small near the rig"
    )]
    let x = (column - frame.origin_col() as f64) * f64::from(TILE_SIZE);
    #[allow(clippy::cast_possible_truncation, reason = "world pixels fit f32")]
    quantize_simulation_scalar(x as f32)
}

impl WorldState {
    /// The Crust's state (Deep Descent worlds; empty elsewhere).
    #[must_use]
    pub const fn crust(&self) -> &CrustState {
        &self.crust
    }

    /// Grants the crew the Deep Claim (the Sinkhole's gate) without the set piece (tests,
    /// tools and scenes that start below Descent I). The rigs already here keep their drills
    /// (no Drill II); rigs that join later get it like any crew joining after the Sinkhole.
    pub fn grant_deep_claim(&mut self) {
        self.crust.deep_claim = true;
        self.crust.drill_ii.extend(self.players.keys().copied());
    }

    /// Heat units at `tile`: the stratum's own heat (plus a lava tube's) on a Deep Descent
    /// world, the classic band heat elsewhere.
    pub(super) fn heat_units_at(&self, tile: TilePosition) -> u8 {
        crust::heat_units(&self.terrain, tile)
    }

    /// Whether `player_id`'s rig rides a mine cart.
    #[must_use]
    pub fn riding_cart(&self, player_id: PlayerId) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.cart.is_some())
    }

    /// Whether `at` is a rail a cart can roll onto: a rail bed with open ground over it (or the
    /// main line's own rail, where a branch dives under it at a junction).
    fn rail_open(&self, at: TilePosition) -> bool {
        let rail = |at: TilePosition| {
            self.terrain
                .tile(at)
                .is_some_and(|tile| tile.kind == TileKind::Crust(CrustTile::Rail))
        };
        let above = TilePosition {
            x: at.x,
            y: at.y - 1,
        };
        rail(at) && (!self.terrain.is_solid_at(above) || rail(above))
    }

    /// The tile a resting rig stands on (under its middle), if it is on the ground.
    fn ground_under(&self, player_id: PlayerId) -> Option<(TilePosition, TileKind)> {
        let player = self.players.get(&player_id)?;
        if !authoritative_player_collides(&self.terrain, player.x, player.y + 2.0) {
            return None;
        }
        let tile = self
            .frame()
            .tile_at(player.x, player.y + AUTHORITATIVE_PLAYER_RADIUS + 2.0);
        Some((tile, self.terrain.tile(tile)?.kind))
    }

    /// Mud and slate (after the movement step): mud bogs the drive down and fights the
    /// thrusters, slick slate keeps a rig sliding where the stick does not push.
    pub(super) fn apply_ground_grip(
        &mut self,
        player_id: PlayerId,
        before: (f32, f32),
        horizontal: f32,
    ) {
        if !crust_rules(self.terrain.settings()) {
            return;
        }
        let Some((_, kind)) = self.ground_under(player_id) else {
            return;
        };
        let grip = crust::ground_grip(kind);
        if grip == GroundGrip::Firm {
            return;
        }
        let seconds = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        match grip {
            GroundGrip::Mud => {
                let top = super::movement::engine_top_speed_tiles(player.engine_level)
                    * TILE_SIZE
                    * MUD_TOP_SPEED_SHARE;
                // The drive bogs down: only part of its push takes, up to a low top speed. The
                // tick already moved the rig at the drive's speed, so it gives the difference
                // back (along the path it just swept, which is free).
                let driven = player.velocity_x;
                let pushed = (driven - before.0).mul_add(crust::MUD_DRIVE_SHARE, before.0);
                player.velocity_x = quantize_simulation_scalar(pushed.clamp(-top, top));
                let give_back = quantize_simulation_scalar((player.velocity_x - driven) * seconds);
                if give_back.abs() > 0.0 && driven.signum() == before.0.signum() {
                    player.x = quantize_simulation_scalar(player.x + give_back);
                }
                if player.velocity_y < before.1 {
                    // Pulling out of the suction: only part of the lift takes.
                    let gained = player.velocity_y - before.1;
                    player.velocity_y =
                        quantize_simulation_scalar(gained.mul_add(MUD_LIFT_SHARE, before.1));
                }
            }
            GroundGrip::Slick => {
                let pushing = horizontal.abs() > 0.1
                    && horizontal.signum() == before.0.signum()
                    && before.0 != 0.0;
                if !pushing && player.velocity_x.abs() < before.0.abs() {
                    // The shelf barely brakes: the rig slides on.
                    let kept = before.0 / SLICK_DRAG_PER_SECOND.mul_add(seconds, 1.0);
                    player.velocity_x = quantize_simulation_scalar(kept);
                }
            }
            GroundGrip::Firm => {}
        }
    }

    /// Boards a mine cart: a rig resting on a rail with the stick held along the line (not
    /// thrusting or drilling) hooks onto it.
    pub(super) fn try_board_cart(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        thrust: bool,
        output: &mut TickOutput,
    ) {
        if !crust_rules(self.terrain.settings())
            || thrust
            || horizontal.abs() < 0.5
            || self.riding_cart(player_id)
            || self.active_drills.contains_key(&player_id)
            || self.logistics.is_riding(player_id)
        {
            return;
        }
        let Some((rail, kind)) = self.ground_under(player_id) else {
            return;
        };
        if kind != TileKind::Crust(CrustTile::Rail) {
            return;
        }
        let dir: i8 = if horizontal > 0.0 { 1 } else { -1 };
        let Some(next) = crust::next_rail(|at| self.rail_open(at), rail, dir, false) else {
            return;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "rig speeds are a few hundred pixels a second"
        )]
        let speed = ((player.velocity_x.abs() / TILE_SIZE) * 1_000.0) as i32;
        player.cart = Some(CartRide {
            rail,
            next,
            dir,
            progress: 0,
            speed: speed.max(CART_BOARD_SPEED),
        });
        output.push_event(GameplayEventKind::CartBoarded { player_id });
    }

    /// One tick of a cart ride: the fixed cart steps due by `next_tick`, then the rig is set on
    /// the cart. Thrust jumps off with the cart's speed; the end of the line (or rubble on it)
    /// ends the ride.
    pub(super) fn step_cart_ride(
        &mut self,
        player_id: PlayerId,
        stick: (f32, bool, bool),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let (horizontal, thrust, down) = stick;
        let Some(mut ride) = self.players.get(&player_id).and_then(|player| player.cart) else {
            return;
        };
        let hz = self.timing.rate.hz();
        let now = next_tick.get();
        let due = crust::cart_steps_by(now, hz)
            .saturating_sub(crust::cart_steps_by(now.saturating_sub(1), hz));
        let stick = CartStick {
            horizontal: if horizontal > 0.3 {
                1
            } else if horizontal < -0.3 {
                -1
            } else {
                0
            },
            down,
        };
        let mut ended = None;
        let mut switched = false;
        if thrust {
            ended = Some(ride.speed * i32::from(ride.dir));
        } else {
            for _ in 0..due {
                match crust::step_cart(&mut ride, stick, |at| self.rail_open(at)) {
                    CartStep::Rolled => {}
                    CartStep::Advanced { switched: took } => switched |= took,
                    CartStep::Ended { velocity } => {
                        ended = Some(velocity);
                        break;
                    }
                }
            }
        }
        let frame = self.frame();
        let (x_milli, y_milli) = ride.point_milli();
        #[allow(
            clippy::cast_precision_loss,
            reason = "milli-tile positions near the rig"
        )]
        let (column, top) = (x_milli as f64 / 1_000.0, y_milli as f64 / 1_000.0);
        let (x, y) = (x_at(frame, column), resting_y_on(frame, top));
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.x = x;
        player.x_remainder = 0.0;
        player.y = y;
        #[allow(clippy::cast_precision_loss, reason = "cart speeds are small")]
        let speed_px = ride.speed as f32 / 1_000.0 * TILE_SIZE * f32::from(ride.dir);
        player.velocity_x = quantize_simulation_scalar(speed_px);
        player.velocity_y = 0.0;
        if switched {
            output.push_event(GameplayEventKind::CartSwitched { player_id });
        }
        match ended {
            Some(velocity) => {
                player.cart = None;
                #[allow(clippy::cast_precision_loss, reason = "cart speeds are small")]
                let leaving = velocity as f32 / 1_000.0 * TILE_SIZE;
                player.velocity_x = quantize_simulation_scalar(leaving);
                if thrust {
                    // A hop off the cart.
                    player.velocity_y = -140.0;
                }
                let tiles_per_second =
                    u8::try_from((velocity.unsigned_abs() / 1_000).min(255)).unwrap_or(u8::MAX);
                output.push_event(GameplayEventKind::CartLeft {
                    player_id,
                    speed: tiles_per_second,
                });
                self.free_rig_from_rock(player_id);
            }
            None => player.cart = Some(ride),
        }
    }

    /// Lifts a rig left inside rock (a cart ride ended at a fork) up to the first free spot.
    fn free_rig_from_rock(&mut self, player_id: PlayerId) {
        let terrain = &self.terrain;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for step in 0..=16_i16 {
            let y = f32::from(step).mul_add(-TILE_SIZE / 8.0, player.y);
            if !super::movement::rig_collides(terrain, player.x, y, player.attitude.orientation) {
                player.y = quantize_simulation_scalar(y);
                return;
            }
        }
    }

    /// Ends `player_id`'s cart ride where it is (the rig was wrecked, rescued or left on foot).
    pub(super) fn drop_cart(&mut self, player_id: PlayerId) {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.cart = None;
        }
    }

    /// A strongbox drilled open: credits and an expedition log.
    pub(super) fn open_strongbox(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        output: &mut TickOutput,
    ) {
        let log = crust::log_for(self.terrain.settings(), target);
        let first = self.crust.logs.insert(log);
        if let Some(player) = self.players.get_mut(&player_id) {
            let credits = if first {
                STRONGBOX_CREDITS * 2
            } else {
                STRONGBOX_CREDITS
            };
            player.credits = player.credits.saturating_add(credits);
        }
        output.push_event(GameplayEventKind::StrongboxOpened {
            player_id,
            log,
            first,
        });
    }

    /// The Sinkhole's core came out: the rumble starts, and every rig in the collapse zone
    /// has to get clear.
    pub(super) fn start_sinkhole(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let Some(site) = crust::sinkhole_site(self.terrain.settings()) else {
            return;
        };
        if self.crust.sinkhole.is_some() || self.crust.deep_claim {
            return;
        }
        let frame = self.frame();
        let inside: BTreeSet<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| site.zone.expanded(2).contains(player.tile_in(frame)))
            .map(|(id, _)| *id)
            .chain(std::iter::once(player_id))
            .collect();
        self.crust.sinkhole = Some(SinkholeRun {
            started_by: player_id,
            started: self.simulation_tick,
            front_row: site.chamber.max.y,
            inside,
            escaped: BTreeSet::new(),
            caught: BTreeSet::new(),
        });
        output.push_event(GameplayEventKind::SinkholeRumble { player_id });
    }

    /// One tick of the Crust: timber sets and slate shelves that lost their support, the
    /// Sinkhole, the lava tubes' cue and the Deep Claim gate.
    pub(super) fn advance_crust(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if !crust_rules(self.terrain.settings()) {
            return;
        }
        self.schedule_failures(next_tick, output);
        self.fire_failures(next_tick, output);
        self.advance_sinkhole(next_tick, output);
        self.track_tubes_and_gate(output);
    }

    /// Reads this tick's terrain writes: a broken beam, or a tile opened under a set's posts,
    /// dooms the set; a tile opened under a slate shelf sets it sliding.
    fn schedule_failures(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let opened: Vec<TilePosition> = self
            .terrain
            .journal()
            .iter()
            .filter(|change| change.tile.kind.is_open())
            .map(|change| change.position)
            .collect();
        if opened.is_empty() {
            return;
        }
        let creak = SimulationTick::new(
            next_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(SUPPORT_CREAK_MILLIS)),
        );
        let slide = SimulationTick::new(
            next_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(SLIDE_WARNING_MILLIS)),
        );
        let kind_at = |terrain: &crate::terrain::Terrain, x: i32, y: i32| {
            terrain.tile(TilePosition { x, y }).map(|tile| tile.kind)
        };
        let timber = Some(TileKind::Crust(CrustTile::Timber));
        let slate = Some(TileKind::Crust(CrustTile::Slate));
        for at in opened {
            // A beam cut through: its neighbours in the set are left hanging.
            if (kind_at(&self.terrain, at.x - 1, at.y) == timber
                || kind_at(&self.terrain, at.x + 1, at.y) == timber)
                && let std::collections::btree_map::Entry::Vacant(entry) =
                    self.crust.collapses.entry(at)
            {
                entry.insert(creak);
                output.push_event(GameplayEventKind::SupportCreaked { x: at.x, y: at.y });
            }
            // Undercut: looking straight up through open ground to a beam.
            for rise in 1..=crate::worldgen::descent::crust::STOPE_AIR + 2 {
                let kind = kind_at(&self.terrain, at.x, at.y - rise);
                if kind == timber {
                    let beam = TilePosition {
                        x: at.x,
                        y: at.y - rise,
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        self.crust.collapses.entry(beam)
                    {
                        entry.insert(creak);
                        output.push_event(GameplayEventKind::SupportCreaked {
                            x: beam.x,
                            y: beam.y,
                        });
                    }
                    break;
                }
                if !kind.is_some_and(TileKind::is_open) {
                    break;
                }
            }
            // A shelf over the opened tile slides.
            let above = TilePosition {
                x: at.x,
                y: at.y - 1,
            };
            if kind_at(&self.terrain, above.x, above.y) == slate
                && let std::collections::btree_map::Entry::Vacant(entry) =
                    self.crust.slides.entry(above)
            {
                entry.insert(slide);
                output.push_event(GameplayEventKind::SlateShudder {
                    x: above.x,
                    y: above.y,
                });
            }
        }
    }

    /// Brings down the sets and shelves whose warning ran out.
    fn fire_failures(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let due: Vec<TilePosition> = self
            .crust
            .collapses
            .iter()
            .filter(|(_, tick)| **tick <= next_tick)
            .map(|(at, _)| *at)
            .collect();
        for beam in due {
            self.crust.collapses.remove(&beam);
            self.collapse_set(beam);
            output.push_event(GameplayEventKind::SupportCollapsed {
                x: beam.x,
                y: beam.y,
            });
        }
        let due: Vec<TilePosition> = self
            .crust
            .slides
            .iter()
            .filter(|(_, tick)| **tick <= next_tick)
            .map(|(at, _)| *at)
            .collect();
        for start in due {
            self.crust.slides.remove(&start);
            if self.slide_shelf(start) {
                output.push_event(GameplayEventKind::SlateSlid {
                    x: start.x,
                    y: start.y,
                });
            }
        }
    }

    /// The set around beam `beam` comes down: its timber and the roof over it fall as boulders,
    /// and the fallen roof lies as rubble on the gallery floor.
    fn collapse_set(&mut self, beam: TilePosition) {
        let frame = self.frame();
        for dx in -SUPPORT_SPAN..=SUPPORT_SPAN {
            let column = beam.x + dx;
            let at = TilePosition {
                x: column,
                y: beam.y,
            };
            let kind = self.terrain.tile(at).map(|tile| tile.kind);
            if kind != Some(TileKind::Crust(CrustTile::Timber)) && dx != 0 {
                continue;
            }
            let _ = self.terrain.set_kind(at, TileKind::Air);
            for rise in 1..=SUPPORT_ROOF_ROWS {
                let roof = TilePosition {
                    x: column,
                    y: beam.y - rise,
                };
                if self.terrain.tile(roof).is_some_and(|tile| {
                    matches!(
                        tile.kind,
                        TileKind::Dirt
                            | TileKind::Clay
                            | TileKind::Stone
                            | TileKind::HardRock
                            | TileKind::Ore(_)
                            | TileKind::Crust(CrustTile::Rubble | CrustTile::Timber)
                    )
                }) {
                    let _ = self.terrain.set_kind(roof, TileKind::Air);
                }
            }
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates near the rig")]
            let x = (frame.rel_col(column) + 0.5) * TILE_SIZE;
            let y = frame.rel_row(beam.y).mul_add(TILE_SIZE, BOULDER_RADIUS);
            self.spawn_falling_boulder(x, y, 0, COLLAPSE_BOULDER_LIFE_MILLIS);
            // The fallen roof settles on the floor (never over a rig).
            if let Some(floor) =
                (1..=crate::worldgen::descent::crust::STOPE_AIR + 2).find_map(|drop| {
                    let below = TilePosition {
                        x: column,
                        y: beam.y + drop,
                    };
                    self.terrain.is_solid_at(below).then_some(TilePosition {
                        x: column,
                        y: below.y - 1,
                    })
                })
                && floor.y > beam.y
                && !self.rig_in_tile(floor)
            {
                let _ = self
                    .terrain
                    .set_kind(floor, TileKind::Crust(CrustTile::Rubble));
            }
        }
    }

    /// Whether any rig's box overlaps tile `tile`.
    fn rig_in_tile(&self, tile: TilePosition) -> bool {
        let frame = self.frame();
        self.players.values().any(|player| {
            let reach = AUTHORITATIVE_PLAYER_RADIUS + 2.0;
            [
                (-reach, -reach),
                (reach, -reach),
                (-reach, reach),
                (reach, reach),
            ]
            .iter()
            .any(|(dx, dy)| frame.tile_at(player.x + dx, player.y + dy) == tile)
        })
    }

    /// The shelf through `start` slides: every slate tile of it (up to [`SLIDE_MAX_TILES`])
    /// left over open ground comes down. Returns whether any did.
    fn slide_shelf(&mut self, start: TilePosition) -> bool {
        let slate = TileKind::Crust(CrustTile::Slate);
        let mut seen = BTreeSet::from([start]);
        let mut queue = vec![start];
        let mut sheet = Vec::new();
        while let Some(at) = queue.pop() {
            if sheet.len() >= SLIDE_MAX_TILES {
                break;
            }
            if self.terrain.tile(at).map(|tile| tile.kind) != Some(slate) {
                continue;
            }
            sheet.push(at);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let next = TilePosition {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                if seen.insert(next) {
                    queue.push(next);
                }
            }
        }
        let frame = self.frame();
        let mut slid = false;
        for at in sheet {
            let below = TilePosition {
                x: at.x,
                y: at.y + 1,
            };
            if self.terrain.is_solid_at(below) {
                continue;
            }
            let _ = self.terrain.set_kind(at, TileKind::Air);
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates near the rig")]
            let x = (frame.rel_col(at.x) + 0.5) * TILE_SIZE;
            let y = frame.rel_row(at.y).mul_add(TILE_SIZE, BOULDER_RADIUS);
            self.spawn_falling_boulder(x, y, 0, COLLAPSE_BOULDER_LIFE_MILLIS);
            slid = true;
        }
        slid
    }

    /// The Sinkhole collapse: the front climbs after the rumble, filling the zone with rubble
    /// and shaking boulders from the walls; rigs it reaches are dug out onto the lip; once it
    /// tops out the crew either has the Deep Claim (someone got clear) or the site resets.
    #[allow(clippy::too_many_lines, reason = "the set piece's tick in order")]
    fn advance_sinkhole(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(run) = self.crust.sinkhole.clone() else {
            return;
        };
        let settings = *self.terrain.settings();
        let Some(site) = crust::sinkhole_site(&settings) else {
            self.crust.sinkhole = None;
            return;
        };
        let rate = self.timing.rate;
        let elapsed_ticks = next_tick.get().saturating_sub(run.started.get());
        let rumble_ticks = rate.ticks_for_millis(SINKHOLE_RUMBLE_MILLIS);
        let frame = self.frame();
        let mut run = run;
        // Rigs that got clear.
        for player_id in run.inside.clone() {
            if run.escaped.contains(&player_id) || run.caught.contains(&player_id) {
                continue;
            }
            let Some(player) = self.players.get(&player_id) else {
                continue;
            };
            if site.clear_of(player.tile_in(frame)) {
                run.escaped.insert(player_id);
                output.push_event(GameplayEventKind::SinkholeEscaped { player_id });
            }
        }
        if elapsed_ticks >= rumble_ticks {
            let climbing = elapsed_ticks - rumble_ticks;
            let risen = i32::try_from(
                climbing.saturating_mul(u64::from(SINKHOLE_FRONT_ROWS_PER_SECOND))
                    / u64::from(rate.hz().max(1)),
            )
            .unwrap_or(i32::MAX);
            let front = (site.chamber.max.y - risen).max(site.lip_row);
            // Rigs the front reaches are dug out and set on the lip.
            let caught: Vec<PlayerId> = self
                .players
                .iter()
                .filter(|(_, player)| {
                    let tile = player.tile_in(frame);
                    site.collapses(tile) && tile.y >= front - 1
                })
                .map(|(id, _)| *id)
                .collect();
            for player_id in caught {
                self.dig_out_onto_lip(player_id, &site);
                if !run.escaped.contains(&player_id) {
                    run.caught.insert(player_id);
                }
                output.push_event(GameplayEventKind::SinkholeCaught { player_id });
            }
            if front < run.front_row {
                for row in front..run.front_row {
                    for x in site.zone.min.x..=site.zone.max.x {
                        let at = TilePosition { x, y: row };
                        if site.collapses(at)
                            && self
                                .terrain
                                .tile(at)
                                .is_some_and(|tile| tile.kind.is_open())
                        {
                            let _ = self
                                .terrain
                                .set_kind(at, TileKind::Crust(CrustTile::Rubble));
                        }
                    }
                }
                run.front_row = front;
            }
            // The walls shed boulders a few rows over the front.
            let period = rate.ticks_for_millis(SINKHOLE_BOULDER_MILLIS).max(1);
            if climbing.is_multiple_of(period) && front > site.lip_row + 8 {
                let seed = settings.seed;
                let rows_up = site.chamber.min.y - (front - 7);
                let half = crust::funnel_half_width(rows_up.max(0));
                let roll = crate::worldgen::noise::hash2(
                    seed,
                    0xC705_5111,
                    i32::try_from(climbing / period).unwrap_or(0),
                    0,
                );
                let span = u64::try_from(2 * half + 1).unwrap_or(1);
                let dx = i32::try_from(roll % span).unwrap_or(0) - half;
                let at = TilePosition {
                    x: site.core.x + dx,
                    y: front - 7,
                };
                if self
                    .terrain
                    .tile(at)
                    .is_some_and(|tile| tile.kind.is_open())
                {
                    #[allow(clippy::cast_precision_loss, reason = "tile coordinates")]
                    let x = (frame.rel_col(at.x) + 0.5) * TILE_SIZE;
                    let y = frame.rel_row(at.y).mul_add(TILE_SIZE, BOULDER_RADIUS);
                    self.spawn_falling_boulder(x, y, 600, COLLAPSE_BOULDER_LIFE_MILLIS);
                    output.push_event(GameplayEventKind::BouldersChanged);
                }
            }
            if front <= site.lip_row {
                // It topped out.
                self.crust.sinkhole = None;
                if run.escaped.is_empty() {
                    self.reset_sinkhole(&site);
                    output.push_event(GameplayEventKind::SinkholeReset {
                        player_id: run.started_by,
                    });
                } else {
                    self.crust.deep_claim = true;
                    self.grant_drill_ii();
                    output.push_event(GameplayEventKind::DeepClaimGranted {
                        player_id: run.started_by,
                    });
                }
                return;
            }
        }
        self.crust.sinkhole = Some(run);
    }

    /// Every rig of a crew holding the Deep Claim that has not had its Drill II yet gets it
    /// (the crew at the Sinkhole, and anyone who joins the world afterwards).
    fn grant_drill_ii(&mut self) {
        let crew: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|player_id| !self.crust.drill_ii.contains(player_id))
            .collect();
        for player_id in crew {
            self.crust.drill_ii.insert(player_id);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.drill_strength = player.drill_strength.max(DRILL_II_LEVEL);
            }
        }
    }

    /// Sets a rig caught by the Sinkhole down in the expedition's camp at the adit's end,
    /// dented (the crew digs it out).
    fn dig_out_onto_lip(&mut self, player_id: PlayerId, site: &crust::SinkholeSite) {
        let landing = site.landing();
        let frame = self.frame();
        let x = (frame.rel_col(landing.x) + 0.5) * TILE_SIZE;
        let y = resting_y_on(frame, f64::from(landing.y + 1));
        self.drop_cart(player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = quantize_simulation_scalar(x);
            player.x_remainder = 0.0;
            player.y = y;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
            let dent = player.max_hull() * SINKHOLE_CAUGHT_HULL_SHARE;
            player.hull = quantize_simulation_scalar((player.hull - dent).max(1.0));
        }
        self.active_drills.remove(&player_id);
        self.active_drill_timers.remove(&player_id);
    }

    /// Puts the Sinkhole back as it was generated (nobody got clear): the rubble goes and the
    /// core sits in the floor again.
    fn reset_sinkhole(&mut self, site: &crust::SinkholeSite) {
        let settings = *self.terrain.settings();
        let rubble = TileKind::Crust(CrustTile::Rubble);
        for y in site.zone.min.y..=site.chamber.max.y {
            for x in site.zone.min.x..=site.zone.max.x {
                let at = TilePosition { x, y };
                if self.terrain.tile(at).map(|tile| tile.kind) == Some(rubble) {
                    let original = crate::worldgen::streamed::generated_tile(&settings, at);
                    let _ = self.terrain.set_kind(at, original.kind);
                }
            }
        }
        let _ = self
            .terrain
            .set_kind(site.core, TileKind::Crust(CrustTile::SinkholeCore));
    }

    /// The lava tubes' "into the heat" cue, and the Deep Claim gate: on a Descent world a rig
    /// below Descent I without the crew's Deep Claim is warned once and crushed like any other
    /// missing gate.
    fn track_tubes_and_gate(&mut self, output: &mut TickOutput) {
        let settings = *self.terrain.settings();
        let frame = self.frame();
        let rigs: Vec<(PlayerId, TilePosition, u8)> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.tile_in(frame), player.hazard_gear.rig_systems))
            .collect();
        if self.crust.deep_claim {
            // Rigs that joined after the Sinkhole fell get their Drill II too.
            self.grant_drill_ii();
        }
        let delta = self.timing.delta_seconds;
        for (player_id, tile, systems) in rigs {
            if crust::lava_tube_at(&settings, tile.x, tile.y) {
                if self.crust.in_tube.insert(player_id) {
                    output.push_event(GameplayEventKind::LavaTubeEntered { player_id });
                }
            } else {
                self.crust.in_tube.remove(&player_id);
            }
            if !self.logistics.rules.is_descent() || self.crust.deep_claim {
                continue;
            }
            let descent = self.descent_of_row(tile.y);
            if descent == 0 {
                self.crust.claim_warned.remove(&player_id);
                continue;
            }
            if missing_gate(systems, descent).is_some() {
                // The deeper gate's own crush applies.
                continue;
            }
            if self.crust.claim_warned.insert(player_id) {
                output.push_event(GameplayEventKind::DeepClaimMissing { player_id });
            }
            if self.damage_player(player_id, GATE_DAMAGE_PER_SECOND * delta) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
    }
}
