//! The Drowned and Frozen in the world (Wave 9, Descent IV; the rules live in
//! [`crate::drowned`]).
//!
//! Two hooks in the rig's movement and one tick pass, all of them returning at once on worlds
//! without Descents (V1-V10 play bit for bit as before):
//!
//! * [`WorldState::apply_drowned_drive`] (after the movement step and the ground grip): ice
//!   under the rig keeps it sliding and gives the drive little traction; hull frost takes a
//!   share of every push.
//! * [`WorldState::apply_submarine`] (after the fluid step): a submerged rig's ballast trims
//!   (thrust blows the tanks, the drill floods them, the trim holds when both are let go) and
//!   lifts or sinks it, a neutral trim hovers; the basin's gyre (and the Leviathan's
//!   maelstrom) pushes it.
//! * [`WorldState::advance_drowned`] (after the Living Dark's pass): tunnels through the
//!   glaciers are scheduled to refreeze, clams drilled open pay or bite, hull frost freezes
//!   and thaws, the abyss's pressure presses on the seals, the geysers blow, sonar pings and
//!   ink clouds fade, the lore is found, lure anglers drag, and the eel-dragon and the
//!   Leviathan fight (`world/enemies/drowned.rs`).

use crate::{
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::Stratum,
    drowned::{
        BALLAST_ACCELERATION, BALLAST_FULL, BALLAST_RATE_PER_SECOND, CLAM_BITE_DAMAGE,
        CLAM_PEARL_CREDITS, CURRENT_DRIFT, DrownedEvent, DrownedLore, DrownedState,
        FROST_BRINE_FACTOR, FROST_MAX, FROST_THAW_HARBOUR, FROST_THAW_VENT, FROST_THAW_WARM,
        FROST_WARNING, GEYSER_LIFT_SPEED, GEYSER_MAX_ROWS, GEYSER_SCALD_DAMAGE_PER_SECOND,
        GEYSER_SCALD_ROWS, GeyserPhase, ICE_DRAG_PER_SECOND, ICE_TRACTION, INK_CLOUD_HALF,
        INK_CLOUD_MILLIS, MAELSTROM_PULL, MAX_INK_CLOUDS, MAX_SONAR_PINGS,
        PRESSURE_DAMAGE_PER_SECOND, RigWater, SONAR_PING_MILLIS, SONAR_REACH_TILES,
        SONAR_SUITE_REACH_TILES, SonarPing, VENT_WARMTH_TILES, abyss_pressure, clam_open,
        current_speed, drowned_rules, frost_rate, frost_speed_factor, geyser_phase, gyre_velocity,
        is_drowned, is_ice, pressure_capacity, refreeze_millis, sonar_light,
    },
    enemy::{EnemyKind, EnemyMode, tier_scale},
    event::{GameplayEventKind, TickOutput},
    field::{FieldPreset, FieldZone},
    fluid::{FluidKind, SUBMARINE_SHARE},
    ids::{PlayerId, SimulationTick},
    item::{ItemKind, ItemStack},
    living_dark::Regrowth,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::{
            DescentLayout,
            drowned::{self, MerrowSite},
        },
        noise::hash2,
    },
};

use super::{
    WorldState,
    movement::{AUTHORITATIVE_PLAYER_RADIUS, AUTHORITATIVE_RIG_GRAVITY},
};

/// Salt of the refreeze jitter.
const SALT_FREEZE: u64 = 0x9E15_1001;
/// A lure angler's light drags rigs this close, tiles.
const LURE_REACH_TILES: f32 = 9.0;
/// ... at this drift, pixels a second.
const LURE_PULL: f32 = 55.0;
/// A frost mite's bite frosts the hull this much, per mille.
pub(super) const MITE_FROST_PERMILLE: f32 = 70.0;

/// A Drowned and Frozen event at `tile`.
pub(super) const fn drowned_event(
    what: DrownedEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Drowned {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

/// A geyser's jet a body stands in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeyserJet {
    /// The vent's column and row.
    pub vent: TilePosition,
    /// Rows from the vent up to the body.
    pub rows_above: i32,
    /// Old Faithful's.
    pub great: bool,
    pub phase: GeyserPhase,
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The Drowned and Frozen's shared state.
    #[must_use]
    pub const fn drowned(&self) -> &DrownedState {
        &self.drowned
    }

    const fn drowned_world(&self) -> bool {
        drowned_rules(self.terrain.settings())
    }

    /// Merrow City's site (Deep Descent worlds).
    #[must_use]
    pub fn merrow_city(&self) -> Option<MerrowSite> {
        drowned::merrow_city(self.terrain.settings())
    }

    /// The Leviathan's arena (Deep Descent worlds).
    #[must_use]
    pub fn leviathan_arena(&self) -> Option<TileRect> {
        drowned::leviathan_arena(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// The eel-dragon's grotto (Deep Descent worlds).
    #[must_use]
    pub fn eel_grotto(&self) -> Option<TileRect> {
        drowned::eel_grotto(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// Old Faithful (Deep Descent worlds).
    #[must_use]
    pub fn old_faithful(&self) -> Option<TileRect> {
        drowned::old_faithful(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// `player_id`'s ballast and hull frost.
    #[must_use]
    pub fn rig_water(&self, player_id: PlayerId) -> RigWater {
        self.players
            .get(&player_id)
            .map(|player| player.water)
            .unwrap_or_default()
    }

    /// Whether `player_id`'s rig (or pilot) is inside Merrow City's bubble.
    #[must_use]
    pub fn in_merrow_city(&self, player_id: PlayerId) -> bool {
        let Some(site) = self.merrow_city() else {
            return false;
        };
        self.actor_position(player_id)
            .is_some_and(|(x, y)| site.in_city(self.frame().tile_at(x, y)))
    }

    /// The Pearl Abyss's pressure on `player_id`'s rig and what its seals hold, per mille
    /// (`None` outside the abyss). The screen darkens and greys with the pressure.
    #[must_use]
    pub fn abyss_pressure_for(&self, player_id: PlayerId) -> Option<(u16, u16)> {
        let player = self.players.get(&player_id)?;
        let tile = player.tile_in(self.frame());
        if self.deep_stratum_at(tile)? != Stratum::PearlAbyss {
            return None;
        }
        let along = self.descent_strata_layout()?.along(tile.y);
        let wet = self.submersion(player.x, player.y, AUTHORITATIVE_PLAYER_RADIUS);
        let depth = if wet.kind.is_some() {
            wet.depth_rows
        } else {
            0
        };
        let cryo = player.hazard_gear.has_system(RigSystem::CryoHull);
        Some((
            abyss_pressure(along, depth),
            pressure_capacity(player.hazard_gear.pressure_rating, cryo),
        ))
    }

    /// How far `player_id`'s sonar reaches, tiles (the Sonar Suite's echo mapping reaches
    /// much further). The Echo Caves (Descent VI) ping with the same reach.
    #[must_use]
    pub fn sonar_reach(&self, player_id: PlayerId) -> i32 {
        let suite = self
            .players
            .get(&player_id)
            .is_some_and(|player| player.hazard_gear.has_system(RigSystem::SonarSuite));
        if suite {
            SONAR_SUITE_REACH_TILES
        } else {
            SONAR_REACH_TILES
        }
    }

    /// The sonar pings still lighting the water.
    #[must_use]
    pub fn sonar_pings(&self) -> &[SonarPing] {
        &self.drowned.pings
    }

    /// How brightly the water's sonar pings light local pixel `(x, y)` now (`0..=1`, the
    /// brightest ping); [`WorldState::sonar_light_at`] adds the Echo Caves' waves.
    #[must_use]
    pub fn drowned_sonar_light_at(&self, x: f32, y: f32) -> f32 {
        let frame = self.frame();
        let now = self.millis_at(self.simulation_tick);
        self.drowned
            .pings
            .iter()
            .map(|ping| {
                let (px, py) = frame.tile_center(TilePosition {
                    x: ping.x,
                    y: ping.y,
                });
                let distance = (px - x).hypot(py - y) / TILE_SIZE;
                let elapsed = now.saturating_sub(self.millis_at(ping.tick));
                sonar_light(elapsed, distance, ping.reach)
            })
            .fold(0.0, f32::max)
    }

    /// Sends a water sonar ping from `player_id`'s rig (through [`WorldState::sonar_ping`]): it
    /// lights the water round the rig for [`SONAR_PING_MILLIS`] and maps the tiles in its reach.
    pub(super) fn drowned_sonar_ping(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> bool {
        let Some(center) = self
            .players
            .get(&player_id)
            .map(|player| player.tile_in(self.frame()))
        else {
            return false;
        };
        let reach = self.sonar_reach(player_id);
        while self.drowned.pings.len() >= MAX_SONAR_PINGS {
            self.drowned.pings.remove(0);
        }
        self.drowned.pings.push(SonarPing {
            player_id,
            x: center.x,
            y: center.y,
            tick: self.simulation_tick,
            reach,
        });
        let _revealed = self.reveal_scanner_area(player_id, center, reach);
        output.push_event(drowned_event(
            DrownedEvent::SonarPing,
            Some(player_id),
            center,
        ));
        true
    }

    /// Whether `player_id`'s rig is where the water's sonar works (Deep Descent worlds): under
    /// water, or anywhere in the Drowned and Frozen.
    pub(super) fn drowned_sonar_here(&self, player_id: PlayerId) -> bool {
        if !self.drowned_world() {
            return false;
        }
        let Some((x, y)) = self.players.get(&player_id).map(|p| (p.x, p.y)) else {
            return false;
        };
        self.submersion(x, y, AUTHORITATIVE_PLAYER_RADIUS)
            .kind
            .is_some()
            || self
                .deep_stratum_at(self.frame().tile_at(x, y))
                .is_some_and(is_drowned)
    }

    /// The scanner's water hook (through [`WorldState::sonar_scanner`]): a rig under water
    /// pings its sonar too.
    pub(super) fn drowned_scanner(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        if !self.drowned_sonar_here(player_id) {
            return;
        }
        // One ping a second at most (the scanner may be pressed on consecutive ticks).
        let now = self.simulation_tick.get();
        let gap = self.timing.ticks_for_millis(1_000);
        if self
            .drowned
            .pings
            .iter()
            .any(|ping| ping.player_id == player_id && now.saturating_sub(ping.tick.get()) < gap)
        {
            return;
        }
        let _pinged = self.drowned_sonar_ping(player_id, output);
    }

    /// Whether the clam at `tile` gapes now.
    #[must_use]
    pub fn clam_open_at(&self, tile: TilePosition) -> bool {
        clam_open(
            self.terrain.settings().seed,
            tile.x,
            tile.y,
            self.millis_at(self.simulation_tick),
        )
    }

    /// The geyser jet a body at local pixel `(x, y)` stands in, if any: the first solid tile
    /// under it (within [`GEYSER_MAX_ROWS`], through open ground) is a geyser vent.
    #[must_use]
    pub fn geyser_jet_at(&self, x: f32, y: f32) -> Option<GeyserJet> {
        let frame = self.frame();
        let start = frame.tile_at(x, y);
        let settings = self.terrain.settings();
        let great_rect = self.old_faithful();
        for rows in 0..=GEYSER_MAX_ROWS {
            let tile = TilePosition {
                x: start.x,
                y: start.y + rows,
            };
            let kind = self.terrain.tile(tile)?.kind;
            if kind.is_open() {
                continue;
            }
            if kind != TileKind::Cave(CaveTile::GeyserVent) {
                return None;
            }
            let great = great_rect.is_some_and(|rect| rect.contains(tile));
            return Some(GeyserJet {
                vent: tile,
                rows_above: rows,
                great,
                phase: geyser_phase(
                    settings.seed,
                    tile.x,
                    great,
                    self.millis_at(self.simulation_tick),
                ),
            });
        }
        None
    }

    /// The current pushing a submerged body at local pixel `(x, y)`, pixels a second: a
    /// basin's gyre in the Underground Ocean, the Leviathan's maelstrom in its arena.
    #[must_use]
    pub fn current_at(&self, x: f32, y: f32) -> (f32, f32) {
        let Some(layout) = self.descent_strata_layout() else {
            return (0.0, 0.0);
        };
        let frame = self.frame();
        let tile = frame.tile_at(x, y);
        let settings = self.terrain.settings();
        let mut current = (0.0, 0.0);
        if self.deep_stratum_at(tile) == Some(Stratum::UndergroundOcean) {
            let gyres = crate::worldgen::descent::variant(settings, Stratum::UndergroundOcean) == 1;
            let area = TileRect::around(tile, 1, 1);
            for basin in drowned::basins_in(settings, &layout, &area) {
                if !basin.bounds.contains(tile) {
                    continue;
                }
                let centre = frame.tile_center(basin.bounds.center());
                #[allow(clippy::cast_precision_loss, reason = "a basin is a few dozen tiles")]
                let radii = (
                    basin.bounds.width() as f32 * TILE_SIZE * 0.5,
                    basin.bounds.height() as f32 * TILE_SIZE * 0.5,
                );
                let (vx, vy) = gyre_velocity(
                    centre,
                    radii,
                    (x, y),
                    basin.clockwise,
                    current_speed(basin.along, gyres),
                );
                current = (current.0 + vx, current.1 + vy);
            }
        }
        if let Some(maw) = self.maelstrom_centre()
            && self
                .leviathan_arena()
                .is_some_and(|arena| arena.contains(tile))
        {
            let (dx, dy) = (maw.0 - x, maw.1 - y);
            let distance = dx.hypot(dy).max(1.0);
            // Strongest near the maw, never zero in the arena.
            let strength = MAELSTROM_PULL * (1.0 - (distance / (TILE_SIZE * 40.0)).min(0.7));
            current = (
                (dx / distance).mul_add(strength, current.0),
                (dy / distance).mul_add(strength, current.1),
            );
        }
        current
    }

    // --- Movement hooks -----------------------------------------------------------------------

    /// Ice and hull frost (after the movement step and the ground grip; see the module docs).
    pub(super) fn apply_drowned_drive(
        &mut self,
        player_id: PlayerId,
        before: (f32, f32),
        horizontal: f32,
        thrust: bool,
    ) {
        if !self.drowned_world() {
            return;
        }
        let ice = self
            .ground_under_rig(player_id)
            .is_some_and(|(_, kind)| is_ice(kind));
        let seconds = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let mut slide = 0.0;
        if ice {
            let pushing = horizontal.abs() > 0.1;
            if pushing {
                // Little traction: only part of the drive's push takes.
                let pushed = (player.velocity_x - before.0).mul_add(ICE_TRACTION, before.0);
                player.velocity_x = quantize(pushed);
            } else if player.velocity_x.abs() < before.0.abs() {
                // Almost no braking: the rig slides on (the drive braked it this tick, so it
                // also slides the distance the braking took).
                let kept = before.0 / ICE_DRAG_PER_SECOND.mul_add(seconds, 1.0);
                slide = (kept - player.velocity_x) * seconds;
                player.velocity_x = quantize(kept);
            }
        }
        let frost = player.water.frost_permille();
        if frost > 0 {
            let factor = frost_speed_factor(frost);
            if horizontal.abs() > 0.1 {
                let driven = player.velocity_x;
                let pushed = (driven - before.0).mul_add(factor, before.0);
                // A frozen drive tops out lower too.
                let top = super::movement::engine_top_speed_tiles(player.engine_level)
                    * TILE_SIZE
                    * factor;
                player.velocity_x = quantize(pushed.clamp(-top, top));
                // The tick already moved the rig at the drive's speed: give back the share the
                // frost took (along the path it just swept, which is free).
                slide = (player.velocity_x - driven).mul_add(seconds, slide);
            }
            if thrust && player.velocity_y < before.1 {
                let gained = (player.velocity_y - before.1).mul_add(factor, before.1);
                player.velocity_y = quantize(gained);
            }
        }
        if slide != 0.0 {
            self.nudge_rig(player_id, slide, 0.0);
        }
    }

    /// Moves `player_id`'s rig by `(dx, dy)` pixels where it fits (else along one axis, else
    /// not at all): currents, slides and lures carry a rig even while its drive brakes.
    fn nudge_rig(&mut self, player_id: PlayerId, dx: f32, dy: f32) {
        let terrain = &self.terrain;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for (mx, my) in [(dx, dy), (dx, 0.0), (0.0, dy)] {
            if mx == 0.0 && my == 0.0 {
                continue;
            }
            let (x, y) = (quantize(player.x + mx), quantize(player.y + my));
            // A rig resting on a floor touches it: judge a sideways nudge a pixel up.
            let blocked = super::movement::authoritative_player_collides(terrain, x, y)
                && (my != 0.0
                    || super::movement::authoritative_player_collides(terrain, x, y - 1.0));
            if !blocked {
                player.x = x;
                player.y = y;
                return;
            }
        }
    }

    /// The ground tile under a rig (the Crust's grip reads the same).
    fn ground_under_rig(&self, player_id: PlayerId) -> Option<(TilePosition, TileKind)> {
        let player = self.players.get(&player_id)?;
        let tile = self
            .frame()
            .tile_at(player.x, player.y + AUTHORITATIVE_PLAYER_RADIUS + 2.0);
        let kind = self.terrain.tile(tile)?.kind;
        (!kind.is_open()).then_some((tile, kind))
    }

    /// Ballast and currents (after the fluid step; see the module docs).
    pub(super) fn apply_submarine(&mut self, player_id: PlayerId, thrust: bool, dive: bool) {
        if !self.drowned_world() {
            return;
        }
        let Some((x, y)) = self.players.get(&player_id).map(|p| (p.x, p.y)) else {
            return;
        };
        let wet = self.submersion(x, y, AUTHORITATIVE_PLAYER_RADIUS);
        let seconds = self.timing.delta_seconds;
        let drilling = self.active_drills.contains_key(&player_id);
        let submerged = wet.share >= SUBMARINE_SHARE;
        // An emergency Ballast Tank (Wave 10 polish): full lift, and no current holds it.
        let blowing = submerged && self.ballast_blowing(player_id);
        let current = if submerged && !blowing {
            self.current_at(x, y)
        } else {
            (0.0, 0.0)
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped to the trim's range"
        )]
        let step = (BALLAST_RATE_PER_SECOND * seconds).round() as i16;
        if !submerged {
            // Out of the water the tanks vent back to neutral.
            let ballast = player.water.ballast;
            player.water.ballast = if ballast > 0 {
                (ballast - step).max(0)
            } else {
                (ballast + step).min(0)
            };
            return;
        }
        if blowing {
            player.water.ballast = BALLAST_FULL;
        } else if thrust && !dive {
            player.water.ballast = (player.water.ballast + step).min(BALLAST_FULL);
        } else if dive && !thrust {
            player.water.ballast = (player.water.ballast - step).max(-BALLAST_FULL);
        }
        if drilling {
            // The drill anchors the rig (as the fluid step does): no lift, no drift.
            return;
        }
        let kind = wet.kind.unwrap_or(FluidKind::Water);
        // A neutral trim carries the weight the water does not: the rig hovers.
        let neutral = AUTHORITATIVE_RIG_GRAVITY * (1.0 - kind.buoyancy()).max(0.0) * wet.share;
        let mut trim =
            BALLAST_ACCELERATION * f32::from(player.water.ballast) / f32::from(BALLAST_FULL);
        if blowing {
            trim += crate::workshop::BALLAST_BLOW_LIFT;
        }
        let vy = (-(neutral + trim)).mul_add(seconds, player.velocity_y);
        // A sinking sub a few pixels off the bottom settles onto it and rests there, so its
        // drill can bite. (The sweep stops a rig a step short of the floor; at a low tick rate
        // the water's sink speed can keep that step longer than the gap, tick after tick.)
        let settle = (vy > 0.0)
            .then(|| settle_distance(&self.terrain, x, y))
            .flatten();
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if let Some(down) = settle {
            player.y = quantize(y + down);
            player.velocity_y = 0.0;
        } else {
            player.velocity_y = quantize(vy);
        }
        // The current carries the submarine along (its drive brakes, the sea does not).
        let drift = CURRENT_DRIFT * wet.share * seconds;
        if current != (0.0, 0.0) {
            self.nudge_rig(player_id, current.0 * drift, current.1 * drift);
        }
    }

    // --- Hooks --------------------------------------------------------------------------------

    /// A Drowned and Frozen creature died (kill credit `by`).
    pub(super) fn drowned_kill(
        &mut self,
        kind: EnemyKind,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        match kind {
            EnemyKind::EelDragon => self.eel_dragon_fell(tile, by, output),
            EnemyKind::Leviathan => self.leviathan_fell(tile, by, output),
            _ => {}
        }
    }

    /// An ink squid inks the water round `tile`.
    pub(super) fn spawn_ink_cloud(&mut self, tile: TilePosition, output: &mut TickOutput) {
        while self.drowned.inks.len() >= MAX_INK_CLOUDS {
            let Some(oldest) = self
                .drowned
                .inks
                .iter()
                .min_by_key(|(id, until)| (**until, **id))
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.drowned.inks.remove(&oldest);
            let _ = self.fields.remove(oldest);
        }
        let id = self.fields.insert(FieldZone::of_preset(
            FieldPreset::Dark,
            (tile.x, tile.y),
            INK_CLOUD_HALF,
        ));
        let until = self.after_millis(self.simulation_tick, INK_CLOUD_MILLIS);
        self.drowned.inks.insert(id, until);
        output.push_event(drowned_event(DrownedEvent::InkCloud, None, tile));
    }

    /// A frost mite bit `player_id`'s rig: the hull frosts.
    pub(super) fn mite_bite(&mut self, player_id: PlayerId) {
        if let Some(player) = self.players.get_mut(&player_id)
            && !player.hazard_gear.has_system(RigSystem::CryoHull)
        {
            player.water.add_frost(MITE_FROST_PERMILLE, 1.0);
        }
    }

    // --- Staging (tests, scenes and dev tools) ----------------------------------------------

    /// Sets `player_id`'s ballast and hull frost before play (scenes, tests).
    pub fn set_rig_water(&mut self, player_id: PlayerId, water: RigWater) {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.water = water;
        }
    }

    /// Replaces the Drowned and Frozen's shared state (scenes, tests).
    pub fn set_drowned(&mut self, state: DrownedState) {
        self.drowned = state;
    }

    /// Opens every tile of `rect` as if it had been dug this moment: tunnels through the
    /// glaciers are scheduled to refreeze, clams pay or bite (scenes, tests). Returns the
    /// events it raised.
    pub fn dig_drowned_tiles(&mut self, rect: TileRect) -> Vec<GameplayEventKind> {
        let Some(layout) = self.descent_strata_layout() else {
            return Vec::new();
        };
        self.terrain.start_journal();
        for position in rect.positions() {
            let _ = self.terrain.set_kind(position, TileKind::Air);
        }
        let mut output = TickOutput::default();
        let now = self.simulation_tick;
        self.note_drowned_openings(&layout, now, &mut output);
        let _ = self.terrain.finish_journal();
        output.events.into_iter().map(|event| event.kind).collect()
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of the Drowned and Frozen (see the module docs).
    pub(super) fn advance_drowned(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if !self.drowned_world() {
            return;
        }
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        self.note_drowned_openings(&layout, next_tick, output);
        self.advance_frost(&layout, output);
        self.advance_pressure(output);
        self.advance_geysers(next_tick, output);
        self.advance_fades(next_tick);
        self.find_lore(output);
        self.advance_lures(output);
        self.advance_eel_dragon(next_tick, output);
        self.advance_leviathan(next_tick, output);
    }

    /// Tunnels through the glaciers refreeze; clams drilled open pay or bite.
    fn note_drowned_openings(
        &mut self,
        layout: &DescentLayout,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let (first, last) = layout.descent_rows(crate::descent::Descent::DrownedAndFrozen);
        let changes: Vec<(TilePosition, TileKind, TileKind)> = self
            .terrain
            .journal_with_before()
            .filter(|(change, before)| {
                change.position.y >= first - 24
                    && change.position.y <= last + 24
                    && change.tile.kind != *before
            })
            .map(|(change, before)| (change.position, before, change.tile.kind))
            .collect();
        if changes.is_empty() {
            return;
        }
        let settings = *self.terrain.settings();
        let city = self.merrow_city();
        for (position, before, after) in changes {
            if before == TileKind::Cave(CaveTile::PearlClam) && after == TileKind::Air {
                self.clam_opened(position, output);
                continue;
            }
            if after != TileKind::Air {
                continue;
            }
            let stratum =
                crate::worldgen::descent::stratum_in(&settings, layout, position.x, position.y);
            if stratum != Stratum::BrineGlaciers
                || self.logistics.outpost_at(position).is_some()
                || self.in_lift_column(position)
                || city.is_some_and(|site| site.bounds().contains(position))
                || self.living_schedule.scorched.contains_key(&position)
            {
                continue;
            }
            let jitter = hash2(settings.seed, SALT_FREEZE, position.x, position.y) % 400;
            let millis = refreeze_millis(layout.along(position.y)) * (800 + jitter) / 1_000;
            let due = SimulationTick::new(
                now.get()
                    .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
            );
            self.living_schedule.schedule(position, due, Regrowth::Rime);
        }
    }

    /// Whether `tile` lies on a lift's column between its stations (ice never closes a lift).
    fn in_lift_column(&self, tile: TilePosition) -> bool {
        self.logistics.lift_on_column(tile.x).is_some_and(|lift| {
            match (lift.stations.first(), lift.stations.last()) {
                (Some(top), Some(bottom)) => (top - 1..=bottom + 1).contains(&tile.y),
                _ => false,
            }
        })
    }

    /// A clam drilled open at `position`: gaping, it gives up its pearl; shut, it bites the
    /// nearest rig and closes back over the hole.
    fn clam_opened(&mut self, position: TilePosition, output: &mut TickOutput) {
        let frame = self.frame();
        let (cx, cy) = frame.tile_center(position);
        let driller = self
            .players
            .iter()
            .map(|(id, player)| ((player.x - cx).hypot(player.y - cy), *id))
            .filter(|(distance, _)| *distance <= TILE_SIZE * 2.5)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id);
        if self.clam_open_at(position) {
            let _ = self.spawn_loot(
                cx,
                cy,
                (0.0, -60.0),
                ItemStack::new(ItemKind::Layer(Stratum::PearlAbyss), 1),
                None,
            );
            if let Some(player) = driller.and_then(|id| self.players.get_mut(&id)) {
                player.credits = player.credits.saturating_add(CLAM_PEARL_CREDITS);
            }
            output.push_event(drowned_event(DrownedEvent::ClamPearl, driller, position));
        } else {
            let _ = self
                .terrain
                .set_kind(position, TileKind::Cave(CaveTile::PearlClam));
            if let Some(player_id) = driller {
                let damage = CLAM_BITE_DAMAGE * tier_scale(self.tier_at(position));
                if self.damage_player(player_id, damage) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
            }
            output.push_event(drowned_event(DrownedEvent::ClamSnapped, driller, position));
        }
    }

    /// Hull frost freezes in the glaciers (brine twice as fast) and thaws by thermal vents,
    /// outposts, the city and anywhere warmer.
    fn advance_frost(&mut self, layout: &DescentLayout, output: &mut TickOutput) {
        let seconds = self.timing.delta_seconds;
        let frame = self.frame();
        let city = self.merrow_city();
        let mut changes: Vec<(PlayerId, f32)> = Vec::new();
        for (player_id, player) in &self.players {
            if player.hazard_gear.has_system(RigSystem::CryoHull) {
                if player.water.frost_micro > 0 {
                    changes.push((*player_id, -f32::from(FROST_MAX)));
                }
                continue;
            }
            let tile = player.tile_in(frame);
            let glaciers = self.deep_stratum_at(tile) == Some(Stratum::BrineGlaciers);
            if !glaciers && player.water.frost_micro == 0 {
                // Nothing to freeze or thaw (skips the vent search for every rig elsewhere).
                continue;
            }
            let harbour = self.logistics.outpost_at(tile).is_some()
                || city.is_some_and(|site| site.in_city(tile));
            let rate = if harbour {
                -FROST_THAW_HARBOUR
            } else if self.near_vent(tile) {
                -FROST_THAW_VENT
            } else if glaciers {
                let wet = self.submersion(player.x, player.y, AUTHORITATIVE_PLAYER_RADIUS);
                let brine = if wet.kind == Some(FluidKind::Brine) {
                    FROST_BRINE_FACTOR
                } else {
                    1.0
                };
                frost_rate(layout.along(tile.y)) * brine
            } else {
                -FROST_THAW_WARM
            };
            if rate > 0.0 || player.water.frost_micro > 0 {
                changes.push((*player_id, rate));
            }
        }
        for (player_id, rate) in changes {
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            let was = player.water.frost_permille();
            player.water.add_frost(rate, seconds);
            let now = player.water.frost_permille();
            if was < FROST_WARNING && now >= FROST_WARNING {
                let tile = player.tile_in(frame);
                // A Frost Heater in the kit (Wave 9 integration, the progression's supply) burns
                // the frost off as it bites; without one the warning sounds.
                if self.take_supply(player_id, crate::workshop::DeepSupply::FrostHeater) {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.water.frost_micro = 0;
                    }
                } else {
                    output.push_event(drowned_event(
                        DrownedEvent::FrostWarning,
                        Some(player_id),
                        tile,
                    ));
                }
            }
        }
    }

    /// Whether a thermal vent warms `tile`.
    pub(super) fn near_vent(&self, tile: TilePosition) -> bool {
        let reach = VENT_WARMTH_TILES;
        (-reach..=reach).any(|dy| {
            (-reach..=reach).any(|dx| {
                self.terrain
                    .tile(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    })
                    .is_some_and(|t| t.kind == TileKind::Cave(CaveTile::ThermalVent))
            })
        })
    }

    /// The abyss's pressure past a rig's seals cracks its hull.
    fn advance_pressure(&mut self, output: &mut TickOutput) {
        let seconds = self.timing.delta_seconds;
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in ids {
            let Some((pressure, capacity)) = self.abyss_pressure_for(player_id) else {
                continue;
            };
            if pressure <= capacity {
                continue;
            }
            let over = f32::from(pressure - capacity) / 100.0;
            let tile = self
                .players
                .get(&player_id)
                .map_or(TilePosition { x: 0, y: 0 }, |p| p.tile_in(self.frame()));
            let damage =
                PRESSURE_DAMAGE_PER_SECOND * over * tier_scale(self.tier_at(tile)) * seconds;
            // Warn every three seconds while it bites (world time, tick-rate safe).
            let now = self.millis_at(self.simulation_tick);
            let before = self.millis_at(SimulationTick::new(
                self.simulation_tick.get().saturating_sub(1),
            ));
            if now / 3_000 != before / 3_000 {
                output.push_event(drowned_event(
                    DrownedEvent::PressureWarning,
                    Some(player_id),
                    tile,
                ));
            }
            if self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
    }

    /// Geysers throw rigs in their jets upward and scald rigs on their throats; an eruption
    /// near a rig is announced as it starts.
    fn advance_geysers(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let seconds = self.timing.delta_seconds;
        let frame = self.frame();
        let rigs: Vec<(PlayerId, f32, f32, TilePosition)> = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.x, player.y, player.tile_in(frame)))
            .filter(|(_, _, _, tile)| self.deep_stratum_at(*tile) == Some(Stratum::GeyserFields))
            .collect();
        if rigs.is_empty() {
            return;
        }
        let settings = *self.terrain.settings();
        let now = self.millis_at(next_tick);
        let before = self.millis_at(SimulationTick::new(next_tick.get().saturating_sub(1)));
        let great_rect = self.old_faithful();
        // Eruptions starting near any rig (each vent's middle column once).
        let mut announced: Vec<TilePosition> = Vec::new();
        for (_, _, _, tile) in &rigs {
            for dy in -6..=20 {
                for dx in -12..=12 {
                    let at = TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    };
                    if self.terrain.tile(at).map(|t| t.kind)
                        != Some(TileKind::Cave(CaveTile::GeyserVent))
                    {
                        continue;
                    }
                    // Only the throat's middle vent speaks for it.
                    let left = self.terrain.tile(TilePosition { x: at.x - 1, ..at });
                    let right = self.terrain.tile(TilePosition { x: at.x + 1, ..at });
                    let vent = Some(TileKind::Cave(CaveTile::GeyserVent));
                    if left.map(|t| t.kind) != vent || right.map(|t| t.kind) != vent {
                        continue;
                    }
                    let great = great_rect.is_some_and(|rect| rect.contains(at));
                    if geyser_phase(settings.seed, at.x, great, now) == GeyserPhase::Erupting
                        && geyser_phase(settings.seed, at.x, great, before) != GeyserPhase::Erupting
                        && !announced.contains(&at)
                    {
                        announced.push(at);
                        output.push_event(drowned_event(DrownedEvent::GeyserErupted, None, at));
                    }
                }
            }
        }
        for (player_id, x, y, tile) in rigs {
            // The rig's columns (it is narrower than a tile, but may straddle two).
            let jet = [
                AUTHORITATIVE_PLAYER_RADIUS.mul_add(-0.6, x),
                x,
                AUTHORITATIVE_PLAYER_RADIUS.mul_add(0.6, x),
            ]
            .into_iter()
            .filter_map(|column| self.geyser_jet_at(column, y))
            .find(|jet| jet.phase == GeyserPhase::Erupting);
            let Some(jet) = jet else {
                continue;
            };
            if jet.rows_above <= GEYSER_SCALD_ROWS {
                let damage =
                    GEYSER_SCALD_DAMAGE_PER_SECOND * tier_scale(self.tier_at(tile)) * seconds;
                let was_hurt = self.damage_player(player_id, damage);
                if was_hurt {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                // Once a second while it scalds.
                if now / 1_000 != before / 1_000 {
                    output.push_event(drowned_event(
                        DrownedEvent::GeyserScalded,
                        Some(player_id),
                        tile,
                    ));
                }
            }
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            let lift = if jet.great {
                GEYSER_LIFT_SPEED * 1.3
            } else {
                GEYSER_LIFT_SPEED
            };
            if player.velocity_y > -lift * 0.8 {
                output.push_event(drowned_event(
                    DrownedEvent::GeyserLaunched,
                    Some(player_id),
                    tile,
                ));
            }
            player.velocity_y = quantize(player.velocity_y.min(-lift));
        }
    }

    /// Sonar pings and ink clouds fade.
    fn advance_fades(&mut self, next_tick: SimulationTick) {
        let ping_ticks = self.timing.ticks_for_millis(SONAR_PING_MILLIS).max(1);
        self.drowned
            .pings
            .retain(|ping| next_tick.get().saturating_sub(ping.tick.get()) < ping_ticks);
        let gone: Vec<u32> = self
            .drowned
            .inks
            .iter()
            .filter(|(_, until)| **until <= next_tick)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.drowned.inks.remove(&id);
            let _ = self.fields.remove(id);
        }
    }

    /// The crew finds the Descent's story beats at their landmarks (the abyss's when the
    /// Leviathan falls).
    fn find_lore(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let settings = *self.terrain.settings();
        let frame = self.frame();
        let wreck =
            drowned::drowned_sub(&settings, &layout).map(|basin| drowned::wreck_hull(&basin));
        let figures = drowned::frozen_figures(&settings, &layout);
        let geyser = drowned::old_faithful(&settings, &layout);
        let city = drowned::merrow_city(&settings);
        let actors: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .filter_map(|id| {
                self.actor_position(*id)
                    .map(|(x, y)| (*id, frame.tile_at(x, y)))
            })
            .collect();
        for (player_id, tile) in actors {
            let found = [
                (DrownedLore::DrownedSub, wreck.map(|hull| hull.expanded(1))),
                (DrownedLore::FrozenFigures, figures),
                (DrownedLore::OldFaithful, geyser),
            ];
            for (lore, rect) in found {
                if !self.drowned.found(lore) && rect.is_some_and(|rect| rect.contains(tile)) {
                    self.drowned.lore |= lore.bit();
                    output.push_event(drowned_event(
                        DrownedEvent::LoreFound,
                        Some(player_id),
                        tile,
                    ));
                }
            }
            if let Some(site) = city
                && site.in_city(tile)
                && !self.drowned.found(DrownedLore::MerrowCity)
            {
                self.drowned.lore |= DrownedLore::MerrowCity.bit();
                output.push_event(drowned_event(
                    DrownedEvent::CityReached,
                    Some(player_id),
                    tile,
                ));
                output.push_event(drowned_event(
                    DrownedEvent::LoreFound,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    /// Lure anglers drag the rigs in their light toward their jaws.
    ///
    /// Wave 10 polish: anglers in a Flare Buoy's light are dazzled and drag nobody; a rig caught
    /// drops a flare from its kit (if it carries one), and a rig blowing ballast tears free.
    fn advance_lures(&mut self, output: &mut TickOutput) {
        let seconds = self.timing.delta_seconds;
        let lures: Vec<(f32, f32)> = self
            .enemies()
            .filter(|(entity, state)| {
                state.kind == EnemyKind::LureAngler
                    && state.mode != EnemyMode::Stunned
                    && !self.flare_dazzles(entity.body.x, entity.body.y)
            })
            .map(|(entity, _)| (entity.body.x, entity.body.y))
            .collect();
        if lures.is_empty() {
            return;
        }
        let reach = LURE_REACH_TILES * TILE_SIZE;
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in ids {
            let Some((px, py)) = self.players.get(&player_id).map(|p| (p.x, p.y)) else {
                continue;
            };
            let pull = lures
                .iter()
                .filter(|(lx, ly)| {
                    (lx - px).hypot(ly - py) <= reach
                        && crate::physics::line_clear(&self.terrain, px, py, *lx, *ly)
                })
                .fold((0.0_f32, 0.0_f32), |acc, (lx, ly)| {
                    let (dx, dy) = (lx - px, ly - py);
                    let distance = dx.hypot(dy).max(1.0);
                    (acc.0 + dx / distance, acc.1 + dy / distance)
                });
            if pull == (0.0, 0.0)
                || self.ballast_blowing(player_id)
                || (!self.is_on_foot(player_id) && self.light_flare(player_id, output))
            {
                continue;
            }
            let length = pull.0.hypot(pull.1).max(1.0);
            let drift = LURE_PULL * seconds / length;
            self.nudge_rig(player_id, pull.0 * drift, pull.1 * drift);
        }
    }

    /// The gate's rewards: the Sonar Suite and the Cryo Hull for every rig (later joiners too).
    pub(super) fn grant_leviathan_rewards(&mut self) {
        for player in self.players.values_mut() {
            for system in [RigSystem::SonarSuite, RigSystem::CryoHull] {
                if !player.hazard_gear.has_system(system) {
                    crate::economy::grant_rig_system(player, system);
                }
            }
        }
    }
}

/// How far (px) above the bottom a sinking submarine settles onto it.
const SETTLE_REACH: f32 = 4.0;

/// How far a rig at `(x, y)` can sink before it touches the bottom, when that is within
/// [`SETTLE_REACH`] (`None`: open water below). Searched in quarter pixels, so it is the same
/// at every tick rate.
fn settle_distance(terrain: &crate::terrain::Terrain, x: f32, y: f32) -> Option<f32> {
    use super::movement::authoritative_player_collides as collides;
    if !collides(terrain, x, y + SETTLE_REACH) || collides(terrain, x, y) {
        return None;
    }
    (0_u8..16)
        .rev()
        .map(|quarter| f32::from(quarter) * 0.25)
        .find(|down| !collides(terrain, x, y + down))
}
