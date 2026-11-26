//! The frontier worlds' rules at work (Wave 12 planets-b, [`crate::frontier`]): each stratum's
//! signature rule, the planet hazards, the weather and the volcanoes.
//!
//! [`WorldState::advance_frontier`] runs on whichever pass holds a frontier planet's ground:
//! the ground pass of a body's store (F11) or the world pass of a world that *is* a planet.
//! Everything is a function of the ground, the rigs and world time (the weather and the
//! eruptions are pure schedules, [`crate::frontier::rules`]), so nothing is saved and every
//! tick rate plays alike. The cadenced rules step on fixed millisecond periods.
//!
//! | Rule | When | What it does |
//! |---|---|---|
//! | loose sand and ash | every [`LOOSE_MILLIS`] | a loose tile with open air under it drops a row (not into a rig or pilot) |
//! | crumbling floors | every [`FLOOR_MILLIS`] | a cracked ruin floor under a resting rig or pilot loses a point; at none, its run gives way |
//! | brittle glass, resonance | when mined | the joined shattering tiles within [`SHATTER_TILES`] break (glass nicks the hull) |
//! | splinters | when mined | obsidian throws splinters at the driller's hull (the boulder deflector turns them) |
//! | light beams | every [`BEAM_MILLIS`] | a light well near a rig traces its beam; lit, its receptor lights and its vault's seals open |
//! | regrowth | every [`REGROWTH_MILLIS`] | open air touching two crystals round a rig in the Growing Geodes grows crystal (never into a body) |
//! | toxic air, cold | each tick | hull damage through the planet gear's share |
//! | heavy lattice | each tick | an extra pull through the planet gear's share |
//! | tube draughts | each tick | rigs and pilots in a lava tube are carried along it |
//! | sandstorms | each tick | rigs and pilots in the open on the surface are carried with the wind |
//! | eruptions | each tick | bombs land where the schedule says (shadows shown first), hurting what is under them and leaving cinders |
//!
//! Heat is the classic heat model's: a frontier zone's heat units come from
//! `crust::heat_units` (asked first), so the radiator, heat shielding and the HEAT warning
//! work unchanged.

use crate::{
    event::{GameplayEventKind, TickOutput},
    frontier::{
        FrontierEvent, FrontierStyle, FrontierTile, PlanetHazard, StratumRule,
        generate::{LightWell, light_wells, tube_way},
        planet_gear_tier,
        rules::{
            BOMB_DAMAGE, BOMB_RADIUS_TILES, EruptionPhase, SANDSTORM_SPEED, WeatherPhase, Zone,
            bombs_landing, cones_near, eruption_at, exposure_at, rule_at, trace_beam,
            unprotected_share, weather_at, zone_at,
        },
        style_of_ground,
    },
    ids::{PlayerId, SimulationTick},
    planet::{BodyGround, PlanetGround},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    WorldState,
    enemies::RIG_CONTACT_HALF,
    movement::{AUTHORITATIVE_RIG_GRAVITY, authoritative_player_collides},
};

/// Milliseconds between loose sand's steps.
pub const LOOSE_MILLIS: u64 = 150;
/// Milliseconds between a cracked floor's losses under a resting rig.
pub const FLOOR_MILLIS: u64 = 250;
/// Milliseconds between light-beam traces.
pub const BEAM_MILLIS: u64 = 500;
/// Milliseconds between crystal regrowth steps.
pub const REGROWTH_MILLIS: u64 = 2_000;
/// Tiles round a broken shattering tile its joined neighbours break within.
pub const SHATTER_TILES: i32 = 3;
/// Most tiles one shatter breaks.
pub const SHATTER_MAX: usize = 24;
/// Hull a shard of desert glass nicks off the driller.
pub const GLASS_SHARD_DAMAGE: f32 = 0.6;
/// Hull obsidian's splinters take off a driller with no boulder deflector.
pub const SPLINTER_DAMAGE: f32 = 1.5;
/// How fast hot draughts carry a rig or a pilot along a lava tube, pixels per second (on top
/// of its own motion: thrust against it to go upwind).
pub const DRAUGHT_SPEED: f32 = 150.0;
/// Columns and rows round a rig the loose-sand and regrowth rules look at.
const LOOSE_REACH: (i32, i32) = (14, 10);
const REGROWTH_REACH: i32 = 6;
/// Most loose tiles moved in one step.
const LOOSE_MAX: usize = 64;
/// Columns round a rig in which volcanoes erupt for it and light wells shine.
const VOLCANO_REACH: i64 = 160;
const WELL_REACH: i32 = 64;

/// A body the rules keep clear of falling sand and growing crystal: `(centre, half size)`.
type BodyBox = ((f32, f32), (f32, f32));

impl WorldState {
    /// The frontier worlds' rules for this pass (see the module docs). Does nothing on any
    /// other ground.
    pub(super) fn advance_frontier(
        &mut self,
        next_tick: SimulationTick,
        inputs: &crate::TickInputs,
        output: &mut TickOutput,
    ) {
        let Some(ground) = self.terrain.planet_ground().copied() else {
            return;
        };
        let Some(style) = style_of_ground(&ground) else {
            return;
        };
        let now = self.millis_at(next_tick);
        let before = self.millis_at(SimulationTick::new(next_tick.get().saturating_sub(1)));
        let every = |world: &Self, millis: u64| {
            next_tick
                .get()
                .is_multiple_of(world.timing.ticks_for_millis(millis).max(1))
        };
        self.frontier_weather_events(&ground, style, before, now, output);
        // A rig drilling braces against the draughts and the wind; one steering braces against
        // a tube's draught only (a sandstorm's wind is a head- or tailwind to it).
        let bracing: Vec<(PlayerId, bool)> = inputs
            .players
            .iter()
            .filter_map(|(id, input)| {
                let controls = input.controls?;
                let steering = controls.horizontal().abs() > 0.1;
                (controls.drill_down || steering).then_some((*id, controls.drill_down))
            })
            .collect();
        self.frontier_exposure(&ground, style, now, &bracing, output);
        if every(self, LOOSE_MILLIS) {
            self.frontier_loose_ground(output);
        }
        if every(self, FLOOR_MILLIS) {
            self.frontier_floors(output);
        }
        if style == FrontierStyle::Crystal {
            if every(self, BEAM_MILLIS) {
                self.frontier_beams(&ground, output);
            }
            if every(self, REGROWTH_MILLIS) {
                self.frontier_regrowth(&ground, output);
            }
        }
        if style == FrontierStyle::Volcanic {
            self.frontier_eruptions(&ground, before, now, output);
        }
        self.advance_frontier_creatures(next_tick);
    }

    /// The live rigs here (not away in space) as `(player, centre)`, in id order.
    fn frontier_rigs(&self) -> Vec<(PlayerId, (f32, f32))> {
        self.players
            .iter()
            .filter(|(id, rig)| rig.hull > 0.0 && !self.in_space(**id))
            .map(|(id, rig)| (*id, (rig.x, rig.y)))
            .collect()
    }

    /// Every rig's and pilot's box (falling sand and growing crystal keep out of them).
    fn frontier_bodies(&self) -> Vec<BodyBox> {
        let mut boxes: Vec<BodyBox> = self
            .players
            .values()
            .map(|rig| ((rig.x, rig.y), RIG_CONTACT_HALF))
            .collect();
        boxes.extend(self.pilots().map(|(_, body, pilot)| {
            (
                pilot.middle(body),
                (crate::pilot::PILOT_HALF_WIDTH, pilot.height() * 0.5),
            )
        }));
        boxes
    }

    /// Whether tile `at` overlaps any of `bodies`.
    fn tile_holds_body(&self, at: TilePosition, bodies: &[BodyBox]) -> bool {
        let (cx, cy) = self.frame().centre_of(at.x, at.y);
        let half = TILE_SIZE * 0.5;
        bodies
            .iter()
            .any(|((x, y), (w, h))| (cx - x).abs() < half + w && (cy - y).abs() < half + h)
    }

    // --- Mining ----------------------------------------------------------------------------

    /// `player_id` mined frontier tile `tile` at `at`: glass and crystal shatter their joined
    /// neighbours, obsidian splinters.
    pub(super) fn frontier_mined(
        &mut self,
        player_id: PlayerId,
        at: TilePosition,
        tile: FrontierTile,
        output: &mut TickOutput,
    ) {
        if tile == FrontierTile::Obsidian {
            let deflector = self
                .players
                .get(&player_id)
                .map_or(0, |rig| rig.hazard_gear.boulder_deflector);
            let share = unprotected_share(deflector);
            if share > 0.0 && self.damage_player(player_id, SPLINTER_DAMAGE * share) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::Splintered,
                player_id: Some(player_id),
                x: at.x,
                y: at.y,
            });
            return;
        }
        if !tile.shatters() {
            return;
        }
        let glass = tile == FrontierTile::DesertGlass;
        let broken = self.shatter_from(at, glass);
        if broken == 0 && !glass {
            // A lone crystal still rings.
        }
        if glass && broken > 0 {
            #[allow(clippy::cast_precision_loss, reason = "at most a few dozen shards")]
            let damage = GLASS_SHARD_DAMAGE * broken as f32;
            if self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
        output.push_event(GameplayEventKind::Frontier {
            what: if glass {
                FrontierEvent::GlassShattered
            } else {
                FrontierEvent::CrystalRang
            },
            player_id: Some(player_id),
            x: at.x,
            y: at.y,
        });
    }

    /// Breaks the shattering tiles joined to `from` (glass to glass, crystal to crystal)
    /// within [`SHATTER_TILES`], at most [`SHATTER_MAX`]; how many broke.
    fn shatter_from(&mut self, from: TilePosition, glass: bool) -> usize {
        let same = |kind: TileKind| match kind {
            TileKind::Frontier(FrontierTile::DesertGlass) => glass,
            TileKind::Frontier(FrontierTile::Crystal | FrontierTile::SpireCrystal) => !glass,
            _ => false,
        };
        let mut queue = std::collections::VecDeque::from([from]);
        let mut seen = std::collections::BTreeSet::from([from]);
        let mut broken = 0;
        while let Some(at) = queue.pop_front() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = TilePosition {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                if (next.x - from.x).abs() > SHATTER_TILES
                    || (next.y - from.y).abs() > SHATTER_TILES
                    || !seen.insert(next)
                {
                    continue;
                }
                if broken >= SHATTER_MAX {
                    return broken;
                }
                if self.terrain.tile(next).is_some_and(|tile| same(tile.kind))
                    && self.terrain.set_kind(next, TileKind::Air)
                {
                    broken += 1;
                    queue.push_back(next);
                }
            }
        }
        broken
    }

    // --- Loose sand and crumbling floors ---------------------------------------------------

    /// Loose sand and ash with open air under it drop a row (bottom first, so a column falls
    /// together), never into a rig or a pilot.
    fn frontier_loose_ground(&mut self, output: &mut TickOutput) {
        let bodies = self.frontier_bodies();
        let frame = self.frame();
        let mut spots = std::collections::BTreeSet::new();
        for ((x, y), _) in &bodies {
            let centre = frame.tile_at(*x, *y);
            for dy in -LOOSE_REACH.1..=LOOSE_REACH.1 {
                for dx in -LOOSE_REACH.0..=LOOSE_REACH.0 {
                    spots.insert((centre.y + dy, centre.x + dx));
                }
            }
        }
        let mut moved = 0;
        let mut first = None;
        // Bottom rows first.
        for (row, column) in spots.into_iter().rev() {
            if moved >= LOOSE_MAX {
                break;
            }
            let at = TilePosition { x: column, y: row };
            let below = TilePosition {
                x: column,
                y: row + 1,
            };
            let Some(tile) = self.terrain.tile(at) else {
                continue;
            };
            let TileKind::Frontier(found) = tile.kind else {
                continue;
            };
            if !found.is_loose()
                || self.terrain.tile(below).map(|t| t.kind) != Some(TileKind::Air)
                || self.tile_holds_body(below, &bodies)
            {
                continue;
            }
            if self.terrain.set_kind(below, tile.kind) && self.terrain.set_kind(at, TileKind::Air) {
                moved += 1;
                first.get_or_insert(below);
            }
        }
        if let Some(at) = first {
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::SandSlid,
                player_id: None,
                x: at.x,
                y: at.y,
            });
        }
    }

    /// A cracked ruin floor under a resting rig or pilot loses a point; at none, it gives way
    /// with the cracked floor joined to it in its row.
    fn frontier_floors(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let mut feet: Vec<(Option<PlayerId>, TilePosition)> = self
            .players
            .iter()
            .filter(|(_, rig)| rig.hull > 0.0 && rig.velocity_y.abs() < 30.0)
            .map(|(id, rig)| {
                (
                    Some(*id),
                    frame.tile_at(rig.x, rig.y + RIG_CONTACT_HALF.1 + 4.0),
                )
            })
            .collect();
        feet.extend(self.pilots().filter(|(_, _, pilot)| pilot.grounded).map(
            |(_, body, pilot)| {
                let (x, y) = pilot.middle(body);
                (
                    Some(pilot.owner),
                    frame.tile_at(x, pilot.height().mul_add(0.5, y) + 4.0),
                )
            },
        ));
        for (player_id, at) in feet {
            let Some(tile) = self.terrain.tile(at) else {
                continue;
            };
            if tile.kind != TileKind::Frontier(FrontierTile::RuinFloor) {
                continue;
            }
            if tile.durability > 1 {
                let _ = self
                    .terrain
                    .set_tile_from_network(at, tile.kind, tile.durability - 1);
                continue;
            }
            // It gives way, and the cracked run with it.
            let _ = self.terrain.set_kind(at, TileKind::Air);
            for step in [1, -1] {
                for reach in 1..=6 {
                    let next = TilePosition {
                        x: at.x + step * reach,
                        y: at.y,
                    };
                    if self.terrain.tile(next).map(|t| t.kind)
                        != Some(TileKind::Frontier(FrontierTile::RuinFloor))
                    {
                        break;
                    }
                    let _ = self.terrain.set_kind(next, TileKind::Air);
                }
            }
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::FloorCrumbled,
                player_id,
                x: at.x,
                y: at.y,
            });
        }
    }

    // --- Light wells and regrowth ----------------------------------------------------------

    /// The light wells near rigs trace their beams; a beam that reaches its receptor lights
    /// it and opens its vault.
    fn frontier_beams(&mut self, ground: &PlanetGround, output: &mut TickOutput) {
        let width = ground.strip_width();
        let rigs: Vec<TilePosition> = self
            .frontier_rigs()
            .into_iter()
            .map(|(_, (x, y))| self.frame().tile_at(x, y))
            .collect();
        let wells: Vec<LightWell> = light_wells(ground)
            .into_iter()
            .filter(|well| {
                rigs.iter().any(|rig| {
                    let dx = (rig.x - well.column).rem_euclid(width);
                    dx.min(width - dx) <= WELL_REACH
                })
            })
            .collect();
        for well in wells {
            let receptor = self.terrain.tile(well.receptor).map(|tile| tile.kind);
            if receptor != Some(TileKind::Frontier(FrontierTile::Receptor)) {
                continue;
            }
            let beam = trace_beam(&well, |at| self.terrain.tile(at).map(|tile| tile.kind));
            if !beam.lit {
                continue;
            }
            let _ = self
                .terrain
                .set_kind_by_rule(well.receptor, TileKind::Frontier(FrontierTile::ReceptorLit));
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::BeamLit,
                player_id: None,
                x: well.receptor.x,
                y: well.receptor.y,
            });
            let (x0, x1, y0, y1) = well.vault;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let at = TilePosition { x, y };
                    if well.is_seal(at)
                        && self.terrain.tile(at).map(|tile| tile.kind)
                            == Some(TileKind::Frontier(FrontierTile::VaultSeal))
                    {
                        let _ = self.terrain.set_kind_by_rule(at, TileKind::Air);
                    }
                }
            }
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::VaultOpened,
                player_id: None,
                x: i32::midpoint(x0, x1),
                y: y1,
            });
        }
    }

    /// Crystal grows back into open air touching two crystals round rigs in the Growing
    /// Geodes (two tiles a step at most, never into a body).
    fn frontier_regrowth(&mut self, ground: &PlanetGround, output: &mut TickOutput) {
        let bodies = self.frontier_bodies();
        let crystal = TileKind::Frontier(FrontierTile::Crystal);
        let mut grown = None;
        for (_, (x, y)) in self.frontier_rigs() {
            let centre = self.frame().tile_at(x, y);
            if rule_at(ground, FrontierStyle::Crystal, centre) != Some(StratumRule::Regrowth)
                || self.in_theme_lair(centre)
            {
                continue;
            }
            let mut budget = 2;
            for dy in -REGROWTH_REACH..=REGROWTH_REACH {
                for dx in -REGROWTH_REACH..=REGROWTH_REACH {
                    if budget == 0 {
                        break;
                    }
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    if self.terrain.tile(at).map(|t| t.kind) != Some(TileKind::Air)
                        || self.tile_holds_body(at, &bodies)
                        || rule_at(ground, FrontierStyle::Crystal, at)
                            != Some(StratumRule::Regrowth)
                    {
                        continue;
                    }
                    let touching = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .filter(|(nx, ny)| {
                            self.terrain
                                .tile(TilePosition {
                                    x: at.x + nx,
                                    y: at.y + ny,
                                })
                                .is_some_and(|t| t.kind == crystal)
                        })
                        .count();
                    if touching >= 2 && self.terrain.set_kind(at, crystal) {
                        budget -= 1;
                        grown.get_or_insert(at);
                    }
                }
            }
        }
        if let Some(at) = grown {
            output.push_event(GameplayEventKind::Frontier {
                what: FrontierEvent::CrystalRegrew,
                player_id: None,
                x: at.x,
                y: at.y,
            });
        }
    }

    // --- Hazards, draughts and weather -----------------------------------------------------

    /// Toxic air and cold bite through the planet gear's share; the heavy lattice pulls; the
    /// lava tubes' draughts and a sandstorm's wind push rigs and pilots.
    fn frontier_exposure(
        &mut self,
        ground: &PlanetGround,
        style: FrontierStyle,
        now: u64,
        bracing: &[(PlayerId, bool)],
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let weather = weather_at(style, ground.params().seed, now);
        let frame = self.frame();
        let seconds_mark = {
            let second = self.timing.ticks_for_millis(1_000).max(1);
            self.simulation_tick.get().is_multiple_of(second)
        };
        for (player_id, (x, y)) in self.frontier_rigs() {
            let tile = frame.tile_at(x, y);
            let zone = zone_at(ground, tile);
            let exposure = exposure_at(ground, tile, weather);
            let Some(gear) = self.players.get(&player_id).map(|rig| rig.hazard_gear) else {
                continue;
            };
            let toxic =
                exposure.toxic * unprotected_share(planet_gear_tier(&gear, PlanetHazard::ToxicAir));
            let cold =
                exposure.cold * unprotected_share(planet_gear_tier(&gear, PlanetHazard::Cold));
            let bite = (toxic + cold) * dt;
            if bite > 0.0 && self.damage_player(player_id, bite) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                if seconds_mark {
                    output.push_event(GameplayEventKind::Frontier {
                        what: FrontierEvent::HazardBite,
                        player_id: Some(player_id),
                        x: tile.x,
                        y: tile.y,
                    });
                }
            }
            let heavy = exposure.heavy
                * unprotected_share(planet_gear_tier(&gear, PlanetHazard::HighGravity));
            let carry = self.frontier_carry(ground, style, zone, tile, weather);
            // Drilling braces against both; steering only against a tube's draught.
            let braced = self.active_drills.contains_key(&player_id)
                || bracing
                    .iter()
                    .any(|(id, drilling)| *id == player_id && (*drilling || zone != Zone::Surface));
            let terrain = &self.terrain;
            if let Some(rig) = self.players.get_mut(&player_id) {
                if heavy > 0.0 {
                    rig.velocity_y = quantize(
                        (heavy * AUTHORITATIVE_RIG_GRAVITY * ground.gravity())
                            .mul_add(dt, rig.velocity_y),
                    );
                }
                // Carried along (never into rock): a conveyor the rig's own motion rides on. A
                // rig drilling braces against it (steering, against a draught).
                if let Some(speed) = carry.filter(|_| !braced) {
                    let x = quantize(speed.mul_add(dt, rig.x));
                    if !authoritative_player_collides(terrain, x, rig.y) {
                        rig.x = x;
                    }
                }
            }
        }
        // Pilots on foot feel the wind and the draughts too.
        let pilots: Vec<(crate::entity::EntityId, TilePosition)> = self
            .pilots()
            .map(|(id, body, pilot)| {
                let (x, y) = pilot.middle(body);
                (id, frame.tile_at(x, y))
            })
            .collect();
        for (id, tile) in pilots {
            let zone = zone_at(ground, tile);
            if let Some(speed) = self.frontier_carry(ground, style, zone, tile, weather) {
                self.entities.update(id, |entity| {
                    entity.body.velocity_x = quantize(push_velocity(
                        entity.body.velocity_x,
                        speed * 4.0,
                        speed,
                        dt,
                    ));
                });
            }
        }
    }

    /// The sideways carry at `tile` (signed pixels per second): a lava tube's draught, a
    /// sandstorm's wind in the open on the surface.
    fn frontier_carry(
        &self,
        ground: &PlanetGround,
        style: FrontierStyle,
        zone: Zone,
        tile: TilePosition,
        weather: crate::frontier::rules::Weather,
    ) -> Option<f32> {
        match (style, zone) {
            (FrontierStyle::Volcanic, Zone::Stratum(1)) => {
                let way = tube_way(ground, i64::from(tile.x), tile.y)?;
                #[allow(clippy::cast_precision_loss, reason = "a sign")]
                let way = way as f32;
                Some(way * DRAUGHT_SPEED)
            }
            (FrontierStyle::Desert, Zone::Surface)
                if weather.phase == WeatherPhase::Storm
                    && tile.y < ground.surface_row(tile.x)
                    && !self.terrain.is_solid_at(TilePosition {
                        x: tile.x,
                        y: tile.y - 1,
                    }) =>
            {
                let strength = f32::from(weather.strength_permille) / 1_000.0;
                let way = f32::from(weather.wind);
                Some(way * SANDSTORM_SPEED * strength)
            }
            _ => None,
        }
    }

    /// Weather changes as events (a storm rising, breaking, passing).
    fn frontier_weather_events(
        &self,
        ground: &PlanetGround,
        style: FrontierStyle,
        before: u64,
        now: u64,
        output: &mut TickOutput,
    ) {
        let seed = ground.params().seed;
        let (was, is) = (
            weather_at(style, seed, before).phase,
            weather_at(style, seed, now).phase,
        );
        if was == is || self.frontier_rigs().is_empty() {
            return;
        }
        let what = match is {
            WeatherPhase::Rising => FrontierEvent::StormRising,
            WeatherPhase::Storm => FrontierEvent::StormStarted,
            WeatherPhase::Calm => FrontierEvent::StormEnded,
        };
        output.push_event(GameplayEventKind::Frontier {
            what,
            player_id: None,
            x: 0,
            y: 0,
        });
    }

    // --- Eruptions -------------------------------------------------------------------------

    /// The volcanoes near rigs rumble, erupt and throw bombs on their schedules.
    fn frontier_eruptions(
        &mut self,
        ground: &PlanetGround,
        before: u64,
        now: u64,
        output: &mut TickOutput,
    ) {
        let seed = ground.params().seed;
        let frame = self.frame();
        let mut cones = std::collections::BTreeSet::new();
        for (_, (x, _)) in self.frontier_rigs() {
            let column = i64::from(frame.tile_at(x, 0.0).x);
            cones.extend(cones_near(ground, column, VOLCANO_REACH));
        }
        for cone in cones {
            let found = crate::frontier::generate::cone_at(ground, cone);
            let (was, _) = eruption_at(seed, cone, before);
            let (is, _) = eruption_at(seed, cone, now);
            let centre = i32::try_from(found.centre).unwrap_or(0);
            if was != is && is != EruptionPhase::Quiet {
                output.push_event(GameplayEventKind::Frontier {
                    what: if is == EruptionPhase::Rumbling {
                        FrontierEvent::EruptionRumble
                    } else {
                        FrontierEvent::EruptionStarted
                    },
                    player_id: None,
                    x: centre,
                    y: -found.peak,
                });
            }
            for bomb in bombs_landing(ground, cone, before, now) {
                self.land_bomb(ground, i32::try_from(bomb.column).unwrap_or(0), output);
            }
        }
    }

    /// A lava bomb lands on `column`: it hurts rigs and pilots within
    /// [`BOMB_RADIUS_TILES`] of where it hits and leaves a cinder there.
    fn land_bomb(&mut self, ground: &PlanetGround, column: i32, output: &mut TickOutput) {
        // The first ground from the sky down (whatever has been dug or built since).
        let top = ground.surface_row(column) - 48;
        let Some(floor) = (top..top + 96).find(|row| {
            self.terrain
                .tile(TilePosition { x: column, y: *row })
                .is_some_and(|tile| !tile.kind.is_open())
        }) else {
            return;
        };
        let at = TilePosition {
            x: column,
            y: floor - 1,
        };
        let frame = self.frame();
        // The column nearest the rigs (the strip wraps).
        let (hit_x, hit_y) = frame.centre_of(at.x, at.y);
        let reach = BOMB_RADIUS_TILES * TILE_SIZE;
        #[allow(
            clippy::cast_precision_loss,
            reason = "a lap is a few hundred thousand columns"
        )]
        let lap = ground.strip_width() as f32 * TILE_SIZE;
        let victims: Vec<PlayerId> = self
            .frontier_rigs()
            .into_iter()
            .filter(|(_, (x, y))| {
                let mut dx = (x - hit_x).rem_euclid(lap);
                if dx > lap * 0.5 {
                    dx -= lap;
                }
                dx.hypot(y - hit_y) <= reach + RIG_CONTACT_HALF.0
            })
            .map(|(id, _)| id)
            .collect();
        for player_id in victims {
            if self.damage_player(player_id, BOMB_DAMAGE) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
            if let Some(rig) = self.players.get_mut(&player_id) {
                rig.velocity_y = quantize(rig.velocity_y.min(-220.0));
            }
        }
        let pilots: Vec<crate::entity::EntityId> = self
            .pilots()
            .filter(|(_, body, pilot)| {
                let (x, y) = pilot.middle(body);
                (x - hit_x).hypot(y - hit_y) <= reach
            })
            .map(|(id, _, _)| id)
            .collect();
        for id in pilots {
            let _ = self.damage_pilot(id, BOMB_DAMAGE, output);
        }
        let bodies = self.frontier_bodies();
        if self.terrain.tile(at).map(|tile| tile.kind) == Some(TileKind::Air)
            && !self.tile_holds_body(at, &bodies)
        {
            let _ = self
                .terrain
                .set_kind(at, TileKind::Frontier(FrontierTile::Cinder));
        }
        output.push_event(GameplayEventKind::Frontier {
            what: FrontierEvent::LavaBombLanded,
            player_id: None,
            x: at.x,
            y: at.y,
        });
    }
}

/// `velocity` pushed by `accel` toward `top` (signed) over `dt`, never past `top`.
fn push_velocity(velocity: f32, accel: f32, top: f32, dt: f32) -> f32 {
    if top > 0.0 && velocity < top {
        accel.mul_add(dt, velocity).min(top)
    } else if top < 0.0 && velocity > top {
        accel.mul_add(dt, velocity).max(top)
    } else {
        velocity
    }
}
