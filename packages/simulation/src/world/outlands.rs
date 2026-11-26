//! The outland worlds in play (Wave 13 planets-c; the data lives in
//! [`crate::planet::outlands`]).
//!
//! One pass a tick over the ground of a fungal, machine or dead planet (a body's ground pass,
//! or a world that is one planet), returning at once anywhere else. It keeps no state of its
//! own: every rhythm (the gear works, the spore vents, the ghost lights, the storms) is a pure
//! function of the body and world time, every edit is a terrain write, and every cadence steps
//! on fixed millisecond periods, so every tick rate plays alike and nothing new saves.
//!
//! * **Hazards**: the Sporeheart Deep's spore air (and the spore drift over the open surface)
//!   fouls the hull, the Reactor Shell's heat burns it, a ghost light's touch chills it. Each
//!   is countered by the stations' planet gear or the Descent's rig systems
//!   ([`WorldState::planet_gear_counters`]); only rigs feel them (the pilot's suit has the
//!   weather's own bite).
//! * **Signature rules**: caps throw a falling rig back up; mycelium regrows behind the rigs;
//!   spore vents swell then blast a rig up their chimneys; belts carry a resting rig; turning
//!   gears grind; loose ash pours into open space; ghost lights chill. Drilled tiles answer in
//!   [`WorldState::outland_tile_mined`]: puffballs burst, glowcaps flash and stun, relays flip
//!   (a vault opens when its three are lit), tomb seals wake their wardens, relics are told.
//! * **Weather**: the theme's storm rising is told to the rigs near the open surface (its
//!   effects are the bodylife weather rules').
//! * **Creatures and the theme boss** (`world/enemies/outlands.rs`).

use crate::{
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode},
    entity::{EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    hazard::HazardCloud,
    ids::{PlayerId, SimulationTick},
    planet::{
        BodyGround, PlanetGround,
        outlands::{
            OutlandTile, OutlandsEvent, boss_lair, dead, fungal,
            life::{self, GearPhase, OutlandRule, OutlandTheme, VentPhase},
            machine,
        },
        wilds::{PlanetHazard, wrap_delta},
    },
    quantize::quantize_simulation_scalar as quantize,
    rig::RigPartKind,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    WorldState,
    movement::{AUTHORITATIVE_PLAYER_RADIUS, authoritative_player_collides},
};

/// A cap throws back a rig falling faster than this (px/s), at this share of its speed and at
/// least this fast.
pub const BOUNCE_MIN_SPEED: f32 = 90.0;
pub const BOUNCE_KEEP: f32 = 0.92;
pub const BOUNCE_LEAST: f32 = 320.0;
/// A belt's speed (px/s).
pub const CONVEYOR_SPEED: f32 = 120.0;
/// A spore vent's blast: the lift it gives a rig in its chimney (px/s up), and how far over
/// the chimney's mouth it still reaches (rows).
pub const VENT_LIFT: f32 = 430.0;
const VENT_REACH_ROWS: i32 = 6;
/// A turning gear's grind, hull a second at tier 0.
pub const GEAR_DAMAGE_PER_SECOND: f32 = 5.0;
/// Spore-laden air and the reactor's heat, hull a second at tier 0.
pub const SPORE_AIR_DAMAGE_PER_SECOND: f32 = 0.6;
pub const REACTOR_HEAT_PER_SECOND: f32 = 1.2;
/// The Thermal Hull's share of the reactor's heat that still gets through.
pub const THERMAL_HULL_HEAT: f32 = 0.25;
/// A ghost light's chill: a hit of this much hull at tier 0, at most this often.
pub const WISP_CHILL_DAMAGE: f32 = 6.0;
pub const WISP_CHILL_MILLIS: u64 = 1_200;
/// How close a wisp's centre must come to a rig's to touch it (tiles).
pub const WISP_TOUCH_TILES: f32 = 1.1;
/// A glowcap's flash: its reach (tiles) and how long the hunters stay stunned.
pub const GLOWFLASH_TILES: f32 = 8.0;
pub const GLOWFLASH_STUN_MILLIS: u64 = 3_000;
/// A burst puffball's cloud: radius (pixels) and life (seconds).
pub const PUFF_CLOUD_RADIUS: f32 = 48.0;
pub const PUFF_CLOUD_LIFE: f32 = 6.0;
/// The regrowth sweep: how often, how far round each rig (tiles), the chance of each open
/// tile (one in `n`), and how close to a body nothing grows (tiles).
pub const REGROW_SWEEP_MILLIS: u64 = 2_000;
pub const REGROW_REACH: i32 = 9;
const REGROW_ONE_IN: u64 = 5;
const REGROW_CLEARANCE: i32 = 3;
/// The ashfall sweep: how often, how far round each rig, at most how many moves a sweep.
pub const ASH_SWEEP_MILLIS: u64 = 250;
pub const ASH_REACH: i32 = 10;
const ASH_MOVES: usize = 24;
/// A hazard event repeats at most this often while it lasts.
const HAZARD_EVENT_MILLIS: u64 = 3_000;
/// Rows over and under the surface a rig hears a storm rise from.
const STORM_HEARING_ROWS: i32 = 30;
/// The enemy tier of an outland world's surface; each stratum down adds one, up to the max.
/// The outland worlds lie farther out than the wild and frontier worlds' first systems, and a
/// body two sectors or more from home adds one more (as the wild worlds do).
pub const OUTLAND_BASE_TIER: u8 = 8;
pub const OUTLAND_MAX_TIER: u8 = 11;
/// Columns round the settlement site that creatures leave alone.
pub const OUTLAND_SITE_MARGIN: i32 = 6;

impl WorldState {
    /// The outland world this pass's ground is, with its life (`None` anywhere else).
    #[must_use]
    pub fn outland_world(&self) -> Option<(&PlanetGround, &'static OutlandTheme)> {
        let planet = self.planet()?;
        let life = life::outland_theme(planet.params().theme)?;
        Some((planet, life))
    }

    /// Whether it is night at strip column `column` of this outland world now (the body's
    /// own clock; a lone planet world's day otherwise).
    #[must_use]
    pub fn outland_night_at(&self, column: i32) -> bool {
        let Some((planet, _)) = self.outland_world() else {
            return false;
        };
        let millis = self.body_millis();
        self.body_life_here().map_or_else(
            || crate::planet::wilds::life::is_night(planet.params(), millis),
            |body| body.clock.elevation(millis, i64::from(column)) < 0.0,
        )
    }

    /// The enemy tier at `tile` on an outland world (`None` elsewhere).
    #[must_use]
    pub fn outland_tier(&self, tile: TilePosition) -> Option<u8> {
        let (planet, _) = self.outland_world()?;
        let stratum = if tile.y < planet.surface_row(tile.x) {
            0
        } else {
            planet.stratum_index(tile.x, tile.y).min(2) + 1
        };
        let far = self
            .body_life_here()
            .and_then(|life| life.id)
            .map_or(0, |id| {
                u8::from(crate::bodylife::sectors_from_home(id.sector) >= 2)
            });
        Some(
            OUTLAND_BASE_TIER
                .saturating_add(u8::try_from(stratum).unwrap_or(0))
                .saturating_add(far)
                .min(OUTLAND_MAX_TIER),
        )
    }

    /// Whether creatures leave `tile` alone on an outland world (`None` elsewhere): the
    /// settlement site only (the surface is the creatures' too).
    #[must_use]
    pub fn outlands_safe_zone(&self, tile: TilePosition) -> Option<bool> {
        let (planet, _) = self.outland_world()?;
        let site = crate::planet::world_of(planet.params().theme)?.settlement_site(planet)?;
        let width = i64::from(planet.strip_width());
        let delta = wrap_delta(i64::from(tile.x), i64::from(site.centre()), width);
        Some(
            delta.abs() <= i64::from(site.width / 2 + OUTLAND_SITE_MARGIN)
                && (site.floor - 30..site.floor + 6).contains(&tile.y),
        )
    }

    /// The signature rule at the rig of `player_id` (`None` off an outland world, above
    /// ground or in the core chamber): the HUD's line.
    #[must_use]
    pub fn outland_rule_of(&self, player_id: PlayerId) -> Option<OutlandRule> {
        let (planet, _) = self.outland_world()?;
        let rig = self.players.get(&player_id)?;
        let tile = self.frame().tile_at(rig.x, rig.y);
        life::rule_at(planet, tile.x, tile.y)
    }

    /// Whether the outland boss of this body is beaten (its lair's seal spent): `None` off an
    /// outland planet or before the lair's ground is loaded.
    ///
    /// The crew-wide record, for bodies not loaded too, is the endgame's hunt list
    /// ([`crate::endgame::hunt`]), written when the boss falls.
    #[must_use]
    pub fn outland_boss_beaten_here(&self) -> Option<bool> {
        let (planet, _) = self.outland_world()?;
        let lair = boss_lair(planet)?;
        let seal = self.terrain.tile(TilePosition {
            x: lair.seal.0,
            y: lair.seal.1,
        })?;
        Some(seal.kind == TileKind::Outland(OutlandTile::BossSealSpent))
    }

    /// Whether a timed event for `player_id` falls due this tick.
    fn outland_due(&self, player_id: PlayerId, next_tick: SimulationTick, millis: u64) -> bool {
        let every = self.timing.ticks_for_millis(millis).max(1);
        (next_tick.get() + player_id.get()).is_multiple_of(every)
    }

    /// The outland world's pass (see the module docs).
    pub(super) fn advance_outlands(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some((planet, life)) = self.outland_world().map(|(planet, life)| (*planet, life))
        else {
            return;
        };
        let players: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|id| !self.in_space(*id))
            .collect();
        for player_id in &players {
            self.apply_outland_hazards(&planet, *player_id, next_tick, output);
            self.apply_outland_rules(&planet, *player_id, next_tick, output);
        }
        self.outland_weather_turns(&planet, life, &players, next_tick, output);
        self.gear_clank(&planet, &players, next_tick, output);
        self.regrowth_sweep(&planet, &players, next_tick, output);
        self.ash_sweep(&planet, &players, next_tick, output);
        self.advance_outland_creatures(&planet, life, &players, next_tick);
        self.advance_outland_boss(&planet, &players, next_tick, output);
    }

    /// The hazards `player_id`'s rig feels now, each with its strength.
    #[must_use]
    pub fn outland_hazards_for(&self, player_id: PlayerId) -> Vec<(PlanetHazard, f32)> {
        let Some((planet, life)) = self.outland_world() else {
            return Vec::new();
        };
        let Some(rig) = self.players.get(&player_id) else {
            return Vec::new();
        };
        let tile = self.frame().tile_at(rig.x, rig.y);
        let mut out: Vec<(PlanetHazard, f32)> = life::hazards_at(planet, tile.x, tile.y)
            .iter()
            // The ghost lights' cold is their touch (below), not the air.
            .filter(|hazard| **hazard != PlanetHazard::Cold)
            .map(|hazard| (*hazard, 1.0))
            .collect();
        // The spore drift fouls the open surface's air.
        if life.storm == crate::planet::outlands::hooks::SPORE_DRIFT
            && tile.y < planet.surface_row(tile.x)
            && self.open_sky_at(tile)
            && let Some(body) = self.body_life_here()
        {
            let weather = body.weather(self.body_millis(), i64::from(tile.x));
            if weather.kind == life.storm && weather.intensity_permille > 0 {
                out.push((
                    PlanetHazard::ToxicAir,
                    f32::from(weather.intensity_permille) / 1_000.0,
                ));
            }
        }
        out
    }

    fn apply_outland_hazards(
        &mut self,
        planet: &PlanetGround,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let Some((x, y)) = self.players.get(&player_id).map(|rig| (rig.x, rig.y)) else {
            return;
        };
        let tile = self.frame().tile_at(x, y);
        let tier = crate::enemy::tier_scale(self.outland_tier(tile).unwrap_or(0));
        let due = self.outland_due(player_id, next_tick, HAZARD_EVENT_MILLIS);
        for (hazard, scale) in self.outland_hazards_for(player_id) {
            if self.planet_gear_counters(player_id, hazard) {
                continue;
            }
            let per_second = match hazard {
                PlanetHazard::Heat => {
                    let thermal = self
                        .equipped_rig_parts
                        .get(&player_id)
                        .is_some_and(|parts| parts.contains(&RigPartKind::ThermalHull));
                    REACTOR_HEAT_PER_SECOND * if thermal { THERMAL_HULL_HEAT } else { 1.0 }
                }
                PlanetHazard::ToxicAir => SPORE_AIR_DAMAGE_PER_SECOND,
                _ => continue,
            };
            if self.damage_player(player_id, per_second * scale * tier * dt) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
            if due {
                output.push_event(outland_event(
                    OutlandsEvent::HazardBite,
                    Some(player_id),
                    tile,
                ));
            }
        }
        // The ghost lights: a touch chills the hull (the cold gear keeps it out).
        if life::rule_at(planet, tile.x, tile.y) == Some(OutlandRule::GhostLights)
            && self.outland_due(player_id, next_tick, WISP_CHILL_MILLIS)
            && !self.planet_gear_counters(player_id, PlanetHazard::Cold)
        {
            let millis = self.body_millis();
            let (cx, cy) = self.frame().centre_of(tile.x, tile.y);
            #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
            let (fx, fy) = (
                tile.x as f32 + 0.5 + (x - cx) / TILE_SIZE,
                tile.y as f32 + 0.5 + (y - cy) / TILE_SIZE,
            );
            let touched = life::wisps_near(planet, i64::from(tile.x), tile.y, millis)
                .iter()
                .any(|wisp| {
                    let (wx, wy) = wisp.tiles();
                    let (dx, dy) = (wx - fx, wy - fy);
                    dx.mul_add(dx, dy * dy) <= WISP_TOUCH_TILES * WISP_TOUCH_TILES
                });
            if touched {
                if self.damage_player(player_id, WISP_CHILL_DAMAGE * tier) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                output.push_event(outland_event(
                    OutlandsEvent::WispChill,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    #[allow(clippy::too_many_lines, reason = "one block per rule")]
    fn apply_outland_rules(
        &mut self,
        planet: &PlanetGround,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((x, y, vy)) = self
            .players
            .get(&player_id)
            .map(|rig| (rig.x, rig.y, rig.velocity_y))
        else {
            return;
        };
        let frame = self.frame();
        let tile = frame.tile_at(x, y);
        let dt = self.timing.delta_seconds;
        let drilling = self.active_drills.contains_key(&player_id);
        let outland_at = |world: &Self, at: TilePosition| {
            world.terrain.tile(at).and_then(|found| match found.kind {
                TileKind::Outland(kind) => Some(kind),
                _ => None,
            })
        };
        // A cap throws a falling rig back up.
        if vy > BOUNCE_MIN_SPEED {
            let probe = frame.tile_at(x, vy.mul_add(dt, y + AUTHORITATIVE_PLAYER_RADIUS) + 2.0);
            if outland_at(self, probe).is_some_and(OutlandTile::is_springy) {
                if let Some(rig) = self.players.get_mut(&player_id) {
                    rig.velocity_y = quantize(-(vy * BOUNCE_KEEP).max(BOUNCE_LEAST));
                }
                output.push_event(outland_event(
                    OutlandsEvent::CapBounce,
                    Some(player_id),
                    probe,
                ));
            }
        }
        // A belt carries a resting rig along (never into rock): a walkway its own motion rides
        // on. A rig drilling braces against it.
        let under = frame.tile_at(x, y + AUTHORITATIVE_PLAYER_RADIUS + 2.0);
        if !drilling
            && let Some(direction) = outland_at(self, under).and_then(OutlandTile::conveyor)
        {
            let carried = quantize((f32::from(direction) * CONVEYOR_SPEED).mul_add(dt, x));
            if !authoritative_player_collides(&self.terrain, carried, y)
                && let Some(rig) = self.players.get_mut(&player_id)
            {
                rig.x = carried;
            }
        }
        let rule = life::rule_at(planet, tile.x, tile.y);
        let millis = self.millis_at(self.simulation_tick);
        let next_millis = self.millis_at(next_tick);
        // Turning gears grind what touches them.
        if rule == Some(OutlandRule::GrindingGears)
            && life::gear_phase(millis) == GearPhase::Turning
        {
            // Within a few pixels of a gear's face.
            let reach = AUTHORITATIVE_PLAYER_RADIUS + 6.0;
            let touching = [
                (-reach, 0.0),
                (reach, 0.0),
                (0.0, -reach),
                (0.0, reach),
                (-reach * 0.7, -reach * 0.7),
                (reach * 0.7, -reach * 0.7),
                (-reach * 0.7, reach * 0.7),
                (reach * 0.7, reach * 0.7),
            ]
            .iter()
            .find(|(dx, dy)| {
                outland_at(self, frame.tile_at(x + dx, y + dy)) == Some(OutlandTile::Gear)
            })
            .copied();
            if let Some((dx, dy)) = touching {
                let tier = crate::enemy::tier_scale(self.outland_tier(tile).unwrap_or(0));
                if self.damage_player(player_id, GEAR_DAMAGE_PER_SECOND * tier * dt) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                // The teeth shove the rig off.
                if let Some(rig) = self.players.get_mut(&player_id) {
                    rig.velocity_x =
                        quantize((-dx.signum()).mul_add(40.0 * dt * 10.0, rig.velocity_x));
                    rig.velocity_y =
                        quantize((-dy.signum()).mul_add(40.0 * dt * 10.0, rig.velocity_y));
                }
                if self.outland_due(player_id, next_tick, 1_000) {
                    output.push_event(outland_event(
                        OutlandsEvent::GearGrind,
                        Some(player_id),
                        tile,
                    ));
                }
            }
        }
        // A spore vent under the rig: it swells, then blasts it up its chimney.
        if let Some(vent) = self.vent_below(planet, tile) {
            {
                let at = TilePosition {
                    x: i32::try_from(vent.column).unwrap_or(0),
                    y: vent.row,
                };
                let before = life::vent_phase(planet, vent.column, millis);
                let after = life::vent_phase(planet, vent.column, next_millis);
                if before != VentPhase::Swelling && after == VentPhase::Swelling {
                    output.push_event(outland_event(
                        OutlandsEvent::VentSwelling,
                        Some(player_id),
                        at,
                    ));
                }
                if after == VentPhase::Blasting {
                    if before != VentPhase::Blasting {
                        output.push_event(outland_event(
                            OutlandsEvent::VentBlast,
                            Some(player_id),
                            at,
                        ));
                    }
                    if let Some(rig) = self.players.get_mut(&player_id) {
                        rig.velocity_y = quantize(rig.velocity_y.min(-VENT_LIFT));
                    }
                    if !self.planet_gear_counters(player_id, PlanetHazard::ToxicAir) {
                        let tier = crate::enemy::tier_scale(self.outland_tier(tile).unwrap_or(0));
                        if self
                            .damage_player(player_id, SPORE_AIR_DAMAGE_PER_SECOND * 2.0 * tier * dt)
                        {
                            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                        }
                    }
                }
            }
        }
    }

    /// The live spore vent whose chimney `tile` is in (its mouth and a few rows over it).
    fn vent_below(&self, planet: &PlanetGround, tile: TilePosition) -> Option<fungal::SporeVent> {
        if planet.params().theme != crate::planet::PlanetThemeId::FUNGAL {
            return None;
        }
        let width = i64::from(planet.strip_width());
        let column = i64::from(tile.x);
        for probe in [0, fungal::VENT_CELL.1 / 2, fungal::VENT_CELL.1] {
            let Some(vent) = fungal::vent_in_cell(planet, column, tile.y + probe) else {
                continue;
            };
            if wrap_delta(column, vent.column, width).abs() > 1 {
                continue;
            }
            if tile.y >= vent.row || tile.y < vent.row - vent.chimney - VENT_REACH_ROWS {
                continue;
            }
            let at = TilePosition {
                x: i32::try_from(vent.column).unwrap_or(0),
                y: vent.row,
            };
            if self
                .terrain
                .tile(at)
                .is_some_and(|t| t.kind == TileKind::Outland(OutlandTile::SporeVent))
            {
                return Some(vent);
            }
        }
        None
    }

    /// The gear works' clank, told to each rig in the Gear Works as the cycle turns to it.
    fn gear_clank(
        &self,
        planet: &PlanetGround,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let before = life::gear_phase(self.millis_at(self.simulation_tick));
        let after = life::gear_phase(self.millis_at(next_tick));
        if before == GearPhase::Clank || after != GearPhase::Clank {
            return;
        }
        let frame = self.frame();
        for player_id in players {
            let Some(tile) = self
                .players
                .get(player_id)
                .map(|rig| frame.tile_at(rig.x, rig.y))
            else {
                continue;
            };
            if life::rule_at(planet, tile.x, tile.y) == Some(OutlandRule::GrindingGears) {
                output.push_event(outland_event(
                    OutlandsEvent::GearsClank,
                    Some(*player_id),
                    tile,
                ));
            }
        }
    }

    /// The theme's storm rising, told to each rig on or near the open surface.
    fn outland_weather_turns(
        &self,
        planet: &PlanetGround,
        life: &OutlandTheme,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(body) = self.body_life_here() else {
            return;
        };
        let frame = self.frame();
        for player_id in players {
            let Some(tile) = self
                .players
                .get(player_id)
                .map(|rig| frame.tile_at(rig.x, rig.y))
            else {
                continue;
            };
            if tile.y >= planet.surface_row(tile.x) + STORM_HEARING_ROWS {
                continue;
            }
            let column = i64::from(tile.x);
            let before = body.weather(self.millis_at(self.simulation_tick), column);
            let after = body.weather(self.millis_at(next_tick), column);
            if after.kind == life.storm
                && after.intensity_permille > 0
                && (before.kind != life.storm || before.intensity_permille == 0)
            {
                output.push_event(outland_event(
                    OutlandsEvent::StormRising,
                    Some(*player_id),
                    tile,
                ));
            }
        }
    }

    /// Tiles round which nothing regrows or pours: rigs and pilots (and creatures, for the
    /// regrowth).
    fn bodies_near(&self) -> Vec<TilePosition> {
        let frame = self.frame();
        let mut out: Vec<TilePosition> = self
            .players
            .values()
            .map(|rig| frame.tile_at(rig.x, rig.y))
            .collect();
        out.extend(
            self.pilots()
                .map(|(_, body, _)| frame.tile_at(body.x, body.y)),
        );
        out.extend(
            self.entities
                .of_kind(EntityKind::Enemy)
                .map(|entity| frame.tile_at(entity.body.x, entity.body.y)),
        );
        out
    }

    /// The Mycelium Mat grows back into the open tunnels round each rig in it.
    fn regrowth_sweep(
        &mut self,
        planet: &PlanetGround,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if planet.params().theme != crate::planet::PlanetThemeId::FUNGAL {
            return;
        }
        let every = self.timing.ticks_for_millis(REGROW_SWEEP_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(every) {
            return;
        }
        let sweep = next_tick.get() / every;
        let frame = self.frame();
        let bodies = self.bodies_near();
        for player_id in players {
            let Some(centre) = self
                .players
                .get(player_id)
                .map(|rig| frame.tile_at(rig.x, rig.y))
            else {
                continue;
            };
            let mut grew = false;
            for dy in -REGROW_REACH..=REGROW_REACH {
                for dx in -REGROW_REACH..=REGROW_REACH {
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    if self
                        .terrain
                        .tile(at)
                        .is_none_or(|t| t.kind != TileKind::Air)
                    {
                        continue;
                    }
                    if planet.generated_tile(at.x, at.y).kind
                        != TileKind::Outland(OutlandTile::MyceliumMat)
                        || life::rule_at(planet, at.x, at.y) != Some(OutlandRule::Regrowth)
                    {
                        continue;
                    }
                    let clear = bodies.iter().all(|body| {
                        (body.x - at.x).abs() > REGROW_CLEARANCE
                            || (body.y - at.y).abs() > REGROW_CLEARANCE
                    });
                    let hash = life::life_roll(
                        planet,
                        8,
                        i64::from(at.x) ^ (i64::try_from(sweep).unwrap_or(0) << 20),
                        i64::from(at.y),
                    );
                    if clear && hash.is_multiple_of(REGROW_ONE_IN) {
                        grew |= self
                            .terrain
                            .set_kind(at, TileKind::Outland(OutlandTile::MyceliumMat));
                    }
                }
            }
            if grew && self.outland_due(*player_id, next_tick, HAZARD_EVENT_MILLIS) {
                output.push_event(outland_event(
                    OutlandsEvent::MyceliumRegrew,
                    Some(*player_id),
                    centre,
                ));
            }
        }
    }

    /// Loose ash pours down into the open round each rig (never onto a body).
    fn ash_sweep(
        &mut self,
        planet: &PlanetGround,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if planet.params().theme != crate::planet::PlanetThemeId::DEAD {
            return;
        }
        let every = self.timing.ticks_for_millis(ASH_SWEEP_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(every) {
            return;
        }
        let frame = self.frame();
        let bodies = self.bodies_near();
        for player_id in players {
            let Some(centre) = self
                .players
                .get(player_id)
                .map(|rig| frame.tile_at(rig.x, rig.y))
            else {
                continue;
            };
            let mut moves = 0;
            // Bottom up, so a column falls one step a sweep.
            for dy in (-ASH_REACH..=ASH_REACH).rev() {
                for dx in -ASH_REACH..=ASH_REACH {
                    if moves >= ASH_MOVES {
                        break;
                    }
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    let below = TilePosition {
                        x: at.x,
                        y: at.y + 1,
                    };
                    let loose = self
                        .terrain
                        .tile(at)
                        .is_some_and(|t| t.kind == TileKind::Outland(OutlandTile::AshDrift));
                    if !loose
                        || self
                            .terrain
                            .tile(below)
                            .is_none_or(|t| t.kind != TileKind::Air)
                        || life::rule_at(planet, at.x, at.y) != Some(OutlandRule::Ashfall)
                    {
                        continue;
                    }
                    let clear = bodies
                        .iter()
                        .all(|body| (body.x - below.x).abs() > 1 || (body.y - below.y).abs() > 1);
                    if clear
                        && self
                            .terrain
                            .set_kind(below, TileKind::Outland(OutlandTile::AshDrift))
                    {
                        let _ = self.terrain.set_kind(at, TileKind::Air);
                        moves += 1;
                    }
                }
            }
            // Told at most twice a second while it pours.
            if moves > 0 && (next_tick.get() / every).is_multiple_of(2) {
                output.push_event(outland_event(
                    OutlandsEvent::AshPoured,
                    Some(*player_id),
                    centre,
                ));
            }
        }
    }

    /// An outland tile `player_id` drilled out at `target`.
    pub(super) fn outland_tile_mined(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: OutlandTile,
        output: &mut TickOutput,
    ) {
        let Some((planet, _)) = self.outland_world().map(|(planet, life)| (*planet, life)) else {
            return;
        };
        let (x, y) = self.frame().centre_of(target.x, target.y);
        match tile {
            OutlandTile::Puffball => {
                let _ = self.spawn_gas_cloud(HazardCloud {
                    x,
                    y,
                    life: PUFF_CLOUD_LIFE,
                    radius: PUFF_CLOUD_RADIUS,
                });
                output.push_event(outland_event(
                    OutlandsEvent::PuffballBurst,
                    Some(player_id),
                    target,
                ));
            }
            OutlandTile::Glowcap => {
                self.glowflash(x, y);
                output.push_event(outland_event(
                    OutlandsEvent::GlowFlash,
                    Some(player_id),
                    target,
                ));
            }
            OutlandTile::RelayOff | OutlandTile::RelayOn => {
                self.flip_relays(
                    &planet,
                    player_id,
                    target,
                    tile == OutlandTile::RelayOn,
                    output,
                );
            }
            OutlandTile::TombSeal => {
                self.open_tomb(&planet, player_id, target, output);
            }
            OutlandTile::SporeCrown
            | OutlandTile::ReactorHeart
            | OutlandTile::ThroneRelic
            | OutlandTile::BuilderCache
            | OutlandTile::Truffle
            | OutlandTile::Sarcophagus => {
                output.push_event(outland_event(
                    OutlandsEvent::RelicFound,
                    Some(player_id),
                    target,
                ));
            }
            OutlandTile::Cache => {
                output.push_event(outland_event(
                    OutlandsEvent::SecretCache,
                    Some(player_id),
                    target,
                ));
            }
            _ => {}
        }
    }

    /// A glowcap's flash at `(x, y)`: every hunter within reach is stunned (the bosses too,
    /// for a moment), the neutral fauna start.
    fn glowflash(&mut self, x: f32, y: f32) {
        let reach = GLOWFLASH_TILES * TILE_SIZE;
        let until = SimulationTick::new(
            self.simulation_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(GLOWFLASH_STUN_MILLIS)),
        );
        let ids: Vec<_> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| {
                let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                dx.mul_add(dx, dy * dy) <= reach * reach
            })
            .map(|entity| entity.id)
            .collect();
        for id in ids {
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state
                    && !state.kind.is_outland_fauna()
                {
                    state.mode = EnemyMode::Stunned;
                    state.mode_until = until;
                }
            });
        }
    }

    /// A relay of a power vault was drilled at `target`: it and its neighbours flip (lights
    /// out); with all three lit, the vault's seals open.
    fn flip_relays(
        &mut self,
        planet: &PlanetGround,
        player_id: PlayerId,
        target: TilePosition,
        was_lit: bool,
        output: &mut TickOutput,
    ) {
        let width = i64::from(planet.strip_width());
        let Some(vault) = machine::vault_at(planet, i64::from(target.x), target.y) else {
            return;
        };
        let relays = vault.relays(width);
        let ring_dx = vault.ring_dx(i64::from(target.x), width);
        let Some(index) = machine::RELAY_OFFSETS
            .iter()
            .position(|offset| *offset == ring_dx)
            .filter(|_| target.y == vault.row)
        else {
            return;
        };
        let lit = |world: &Self, (column, row): (i32, i32)| {
            world
                .terrain
                .tile(TilePosition { x: column, y: row })
                .is_some_and(|t| t.kind == TileKind::Outland(OutlandTile::RelayOn))
        };
        // The drilled relay is open air now (it is put back flipped below); a relay blown
        // away reads dark and is put back too.
        let mut states: [bool; 3] = [
            lit(self, relays[0]),
            lit(self, relays[1]),
            lit(self, relays[2]),
        ];
        states[index] = was_lit;
        for (relay, state) in states.iter_mut().enumerate() {
            if relay.abs_diff(index) <= 1 {
                *state = !*state;
            }
        }
        for (state, (column, row)) in states.iter().zip(relays) {
            let kind = if *state {
                OutlandTile::RelayOn
            } else {
                OutlandTile::RelayOff
            };
            let _ = self
                .terrain
                .set_kind(TilePosition { x: column, y: row }, TileKind::Outland(kind));
        }
        output.push_event(outland_event(
            OutlandsEvent::RelayFlipped,
            Some(player_id),
            target,
        ));
        if states.iter().all(|state| *state) {
            // The seals open: every power seal in the vault's roof.
            for dx in 0..machine::VAULT_BOX.0 {
                let at = TilePosition {
                    x: i32::try_from((vault.column + dx * vault.span).rem_euclid(width))
                        .unwrap_or(0),
                    y: vault.row,
                };
                if self
                    .terrain
                    .tile(at)
                    .is_some_and(|t| t.kind == TileKind::Outland(OutlandTile::PowerSeal))
                {
                    // Power opens what no drill can.
                    let _ = self.terrain.set_kind_by_rule(at, TileKind::Air);
                }
            }
            output.push_event(outland_event(
                OutlandsEvent::VaultOpened,
                Some(player_id),
                target,
            ));
        }
    }

    /// A tomb's seal was broken at `target`: the rest of the seal falls and its two wardens
    /// wake inside (none on Peaceful).
    fn open_tomb(
        &mut self,
        planet: &PlanetGround,
        player_id: PlayerId,
        target: TilePosition,
        output: &mut TickOutput,
    ) {
        let width = i64::from(planet.strip_width());
        let Some(tomb) = dead::tomb_at(planet, i64::from(target.x), target.y) else {
            return;
        };
        for (column, row) in tomb.seal(width) {
            let at = TilePosition { x: column, y: row };
            if self
                .terrain
                .tile(at)
                .is_some_and(|t| t.kind == TileKind::Outland(OutlandTile::TombSeal))
            {
                let _ = self.terrain.set_kind(at, TileKind::Air);
            }
        }
        output.push_event(outland_event(
            OutlandsEvent::TombOpened,
            Some(player_id),
            target,
        ));
        if self.enemies.difficulty == EnemyDifficulty::Peaceful {
            return;
        }
        let (middle_x, middle_y) = tomb.middle();
        let frame = self.frame();
        for offset in [-3_i64, 3] {
            let tile = TilePosition {
                x: i32::try_from(middle_x + offset).unwrap_or(0),
                y: middle_y,
            };
            let point = Self::spawn_point(frame, EnemyKind::Gravewarden, tile);
            let tier = self.tier_at(tile);
            let id = self.spawn_enemy_with(EnemyKind::Gravewarden, point, tier, None, None);
            // Woken, they come for the one who broke in.
            let now = self.simulation_tick;
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.target = Some(player_id);
                    state.last_seen = now;
                    state.mode = EnemyMode::Chase;
                }
            });
        }
    }
}

/// An outland world's event at `tile`.
pub(super) const fn outland_event(
    what: OutlandsEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Outlands {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}
