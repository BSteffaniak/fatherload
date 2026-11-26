//! The exotic worlds' rules at work (Wave 13 planets-d, [`crate::exotic`]): each stratum's
//! signature rule, the planet hazards, the weather, the bosses' arenas and the creatures.
//!
//! [`WorldState::advance_exotic`] runs on whichever pass holds an exotic planet's ground (a
//! body's store, or a world that *is* a planet). Everything is a function of the ground, the
//! rigs, the terrain and world time (the schedules are [`crate::exotic::rules`]'), so nothing
//! is saved and every tick rate plays alike:
//!
//! | Rule | What it does |
//! |---|---|
//! | buoyancy (gas giant) | the thick gas holds an airborne rig up (a share of its pull) and caps its fall |
//! | jet streams | lanes of wind carry rigs and pilots east or west (drilling braces) |
//! | lightning | a flickering column strikes after its tell: rigs in it take a bolt |
//! | crushing pressure | warned on the way down, then hull a second growing with depth (high-gravity gear braces) |
//! | diamond rain, meteor showers | a shadow grows, then a hailstone or meteorite lands: a blow round it, diamond or meteorite left |
//! | sun and shadow | the day side burns, the night side freezes (heat or cold gear; half under rock) |
//! | geysers | a vent hisses, then a plume throws rigs and pilots up (and bites without gear) |
//! | tidal quakes | a rumble, then fault rubble over rigs falls as boulders |
//! | lodestone drift, the Skyring's drift, the terminator gale | a sideways carry |
//! | microgravity | the Skyring holds an airborne rig up by two thirds of its pull |
//! | venting ice, live conduits | answered when mined and touched (see [`WorldState::exotic_mined`]) |
//! | dense core | an extra pull through the planet gear |

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use std::collections::BTreeSet;

use crate::{
    event::{GameplayEventKind, TickOutput},
    exotic::{
        ExoticEvent, ExoticRule, ExoticStyle, ExoticTile, boss,
        locked::{self, Side},
        rules::{
            self, FALL_DAMAGE, FALL_RADIUS_TILES, FALL_TELL_MILLIS, GAS_TERMINAL_FALL, GEYSER_BITE,
            GEYSER_LIFT, GEYSER_REACH_ROWS, GeyserPhase, Zone,
        },
        style_of_ground,
    },
    frontier::{PlanetHazard, planet_gear_tier},
    hazard::FallingBoulder,
    ids::{PlayerId, SimulationTick},
    planet::{BodyGround, PlanetGround},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{
    WorldState,
    movement::{AUTHORITATIVE_RIG_GRAVITY, authoritative_player_collides},
};

/// A hazard's bite repeats its event at most this often.
const HAZARD_EVENT_MILLIS: u64 = 3_000;
/// Rows above the Pressure Deep within which a diving rig is warned.
pub const PRESSURE_WARNING_ROWS: i32 = 60;
/// A pressure warning repeats this often while it applies.
const PRESSURE_WARNING_MILLIS: u64 = 4_000;
/// Hull a second a live conduit takes from a rig touching it (before the tier's scale).
pub const CONDUIT_SHOCK: f32 = 6.0;
/// How hard a vent of ring ice throws the drilling rig back (pixels a second).
pub const VENT_KICK: f32 = 340.0;
/// The most conduit tiles one cut junction darkens.
const CONDUIT_RUN_MAX: usize = 160;
/// The most rubble one quake drops round one rig.
const QUAKE_RUBBLE_MAX: usize = 6;
/// The rubble's warning before it falls, and how long a boulder lasts.
const RUBBLE_WARNING_MILLIS: u64 = 450;
const RUBBLE_LIFE_MILLIS: u64 = 4_000;
/// Rows over and under the surface a rig hears a storm rise from.
const STORM_HEARING_ROWS: i32 = 40;
/// Columns either side of the settlement creatures leave alone, and rows under it.
const SETTLEMENT_MARGIN: i32 = 12;
const SETTLEMENT_SAFE_ROWS: i32 = 12;

impl WorldState {
    /// The exotic world this pass's ground is (`None` anywhere else).
    #[must_use]
    pub fn exotic_world(&self) -> Option<(PlanetGround, ExoticStyle)> {
        let ground = *self.terrain.planet_ground()?;
        let style = style_of_ground(&ground)?;
        Some((ground, style))
    }

    /// The enemy tier at `tile` on an exotic world (`None` elsewhere): like the wild worlds',
    /// eight on the surface and one more per stratum down (a body two sectors or more from
    /// home adds one more).
    #[must_use]
    pub fn exotic_tier(&self, tile: TilePosition) -> Option<u8> {
        let (ground, style) = self.exotic_world()?;
        let far = self
            .body_life_here()
            .and_then(|life| life.id)
            .map_or(0, |id| {
                u8::from(crate::bodylife::sectors_from_home(id.sector) >= 2)
            });
        Some(
            rules::tier_at(&ground, style, tile)
                .saturating_add(far)
                .min(rules::EXOTIC_MAX_TIER),
        )
    }

    /// Whether creatures leave `tile` alone on an exotic world (`None` elsewhere): only the
    /// settlement (a sky city's deck, the twilight meadow, the plains town), claimed bases and
    /// placed infrastructure are safe.
    #[must_use]
    pub fn exotic_safe_zone(&self, tile: TilePosition) -> Option<bool> {
        let (ground, _) = self.exotic_world()?;
        let settlement = crate::exotic::settlement_site(&ground).is_some_and(|site| {
            let width = i64::from(ground.strip_width());
            let dx = crate::exotic::wrap_delta(i64::from(tile.x), i64::from(site.centre()), width);
            dx.abs() <= i64::from(site.width / 2 + SETTLEMENT_MARGIN)
                && tile.y < site.floor + SETTLEMENT_SAFE_ROWS
                && tile.y > site.floor - 40
        });
        let near_infrastructure = self.infrastructure.iter().any(|item| {
            (item.position.x - tile.x).abs() <= super::enemies::BASE_SAFE_TILES
                && (item.position.y - tile.y).abs() <= super::enemies::BASE_SAFE_TILES
        });
        Some(
            settlement
                || near_infrastructure
                || self.bases.is_safe_zone(tile)
                || self.homes.is_safe_zone(tile),
        )
    }

    /// Whether the theme boss of this exotic world has been beaten (its seal opened into its
    /// trophy, or the trophy dug out). `None` off an exotic world. TODO-SEAM w13-endgame: the
    /// hunt list reads this per body ([`crate::exotic::boss::THEME_BOSSES`]).
    #[must_use]
    pub fn exotic_boss_beaten(&self) -> Option<bool> {
        let (ground, _) = self.exotic_world()?;
        let arena = boss::arena(&ground)?;
        let tile = self.terrain.tile(arena.seal)?;
        Some(tile.kind != TileKind::Exotic(ExoticTile::BossSeal))
    }

    /// The live rigs here (not away in space) as `(player, tile)`, in id order.
    fn exotic_rigs(&self) -> Vec<(PlayerId, TilePosition)> {
        let frame = self.frame();
        self.players
            .iter()
            .filter(|(id, rig)| rig.hull > 0.0 && !self.in_space(**id))
            .map(|(id, rig)| (*id, frame.tile_at(rig.x, rig.y)))
            .collect()
    }

    /// Whether an event for `player_id` falls due this tick on a `millis` cadence.
    fn exotic_due(&self, player_id: PlayerId, next_tick: SimulationTick, millis: u64) -> bool {
        let every = self.timing.ticks_for_millis(millis).max(1);
        (next_tick.get() + player_id.get()).is_multiple_of(every)
    }

    /// The exotic worlds' pass (see the module docs). Does nothing on any other ground.
    pub(super) fn advance_exotic(
        &mut self,
        next_tick: SimulationTick,
        inputs: &crate::TickInputs,
        output: &mut TickOutput,
    ) {
        let Some((ground, style)) = self.exotic_world() else {
            return;
        };
        let now = self.millis_at(next_tick);
        let before = self.millis_at(SimulationTick::new(next_tick.get().saturating_sub(1)));
        let seed = ground.params().seed;
        let storm = rules::storm_strength(style, seed, now);
        let rigs = self.exotic_rigs();
        // The storm's rise, told to the rigs near the open sky.
        if rules::storm_rising(style, seed, before, now) {
            for (player_id, tile) in &rigs {
                if tile.y < ground.surface_row(tile.x) + STORM_HEARING_ROWS {
                    output.push_event(exotic_event(
                        ExoticEvent::StormRising,
                        Some(*player_id),
                        *tile,
                    ));
                }
            }
        }
        let bracing: BTreeSet<PlayerId> = inputs
            .players
            .iter()
            .filter_map(|(id, input)| {
                let controls = input.controls?;
                controls.drill_down.then_some(*id)
            })
            .chain(self.active_drills.keys().copied())
            .collect();
        for (player_id, tile) in &rigs {
            self.exotic_rig_rules(
                &ground, style, *player_id, *tile, storm, &bracing, next_tick, output,
            );
        }
        self.exotic_pilot_rules(&ground, storm);
        self.exotic_docking(&ground, output);
        match style {
            ExoticStyle::GasGiant => {
                self.exotic_bolts(&ground, &rigs, before, now, storm, output);
                self.exotic_fallers(&ground, style, &rigs, before, now, storm, output);
            }
            ExoticStyle::Ring => {
                self.exotic_fallers(&ground, style, &rigs, before, now, storm, output);
            }
            ExoticStyle::Locked => {
                self.exotic_geysers(&ground, &rigs, before, now, output);
                self.exotic_quakes(&ground, &rigs, before, now, output);
            }
        }
        self.advance_exotic_boss_arena(&ground, style, output);
        self.advance_exotic_creatures(&ground, style, next_tick);
    }

    /// One rig's hazards, lift, carry and touch rules.
    #[allow(clippy::too_many_arguments, reason = "one rig's share of the pass")]
    fn exotic_rig_rules(
        &mut self,
        ground: &PlanetGround,
        style: ExoticStyle,
        player_id: PlayerId,
        tile: TilePosition,
        storm: f32,
        bracing: &BTreeSet<PlayerId>,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let Some((x, y, gear)) = self
            .players
            .get(&player_id)
            .map(|rig| (rig.x, rig.y, rig.hazard_gear))
        else {
            return;
        };
        let tier =
            crate::enemy::tier_scale(self.exotic_tier(tile).unwrap_or(rules::EXOTIC_BASE_TIER));
        let unprotected = |hazard: PlanetHazard| {
            crate::frontier::rules::unprotected_share(planet_gear_tier(&gear, hazard))
        };
        // Hazards: heat, cold, the crushing pressure.
        let exposure = rules::exposure_at(ground, tile);
        let bite = (exposure.heat * unprotected(PlanetHazard::Heat)
            + exposure.cold * unprotected(PlanetHazard::Cold)
            + exposure.pressure * unprotected(PlanetHazard::HighGravity))
            * tier
            * dt;
        if bite > 0.0 && self.damage_player(player_id, bite) {
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            if self.exotic_due(player_id, next_tick, HAZARD_EVENT_MILLIS) {
                output.push_event(exotic_event(ExoticEvent::HazardBite, Some(player_id), tile));
            }
        }
        // The pressure's early warning: on the way down to the Pressure Deep, and while in it
        // without the full brace.
        if style == ExoticStyle::GasGiant {
            let top = crate::exotic::gas_giant::cloud_top(ground, i64::from(tile.x));
            let deep_top = crate::exotic::gas_giant::band_top_row(ground, 2);
            let depth = tile.y - top;
            let approaching = depth >= deep_top - PRESSURE_WARNING_ROWS && depth < deep_top;
            let crushing = exposure.pressure > 0.0 && unprotected(PlanetHazard::HighGravity) > 0.0;
            if (approaching || crushing)
                && self.exotic_due(player_id, next_tick, PRESSURE_WARNING_MILLIS)
            {
                output.push_event(exotic_event(
                    ExoticEvent::PressureWarning,
                    Some(player_id),
                    tile,
                ));
            }
        }
        let grounded = authoritative_player_collides(&self.terrain, x, y + 2.0);
        let pull = AUTHORITATIVE_RIG_GRAVITY * ground.gravity();
        let lift = rules::lift_share(ground, tile);
        let heavy = exposure.heavy * unprotected(PlanetHazard::HighGravity);
        let carry = rules::carry_at(ground, tile, storm).filter(|_| !bracing.contains(&player_id));
        let terrain = &self.terrain;
        if let Some(rig) = self.players.get_mut(&player_id) {
            if heavy > 0.0 {
                rig.velocity_y = quantize((heavy * pull).mul_add(dt, rig.velocity_y));
            }
            if lift > 0.0 && !grounded {
                rig.velocity_y = quantize((-lift * pull).mul_add(dt, rig.velocity_y));
            }
            if style == ExoticStyle::GasGiant && rig.velocity_y > GAS_TERMINAL_FALL {
                rig.velocity_y = quantize(GAS_TERMINAL_FALL);
            }
            if let Some(speed) = carry {
                let moved = quantize(speed.mul_add(dt, rig.x));
                if !authoritative_player_collides(terrain, moved, rig.y) {
                    rig.x = moved;
                }
            }
        }
        // A live conduit shocks a rig touching it.
        if style == ExoticStyle::Ring && self.touches_live_conduit(x, y) {
            let shock = CONDUIT_SHOCK * tier * dt;
            if self.damage_player(player_id, shock) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
            if self.exotic_due(player_id, next_tick, 1_000) {
                output.push_event(exotic_event(
                    ExoticEvent::ConduitShock,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    fn touches_live_conduit(&self, x: f32, y: f32) -> bool {
        let reach = super::movement::AUTHORITATIVE_PLAYER_RADIUS + 3.0;
        let frame = self.frame();
        [(-reach, 0.0), (reach, 0.0), (0.0, -reach), (0.0, reach)]
            .iter()
            .any(|(dx, dy)| {
                self.terrain
                    .tile(frame.tile_at(x + dx, y + dy))
                    .is_some_and(|t| t.kind == TileKind::Exotic(ExoticTile::ConduitLive))
            })
    }

    /// Pilots on foot ride the jet streams and the drifts, and the thick gas holds them up.
    fn exotic_pilot_rules(&mut self, ground: &PlanetGround, storm: f32) {
        let dt = self.timing.delta_seconds;
        let frame = self.frame();
        let pilots: Vec<(crate::entity::EntityId, TilePosition)> = self
            .pilots()
            .map(|(id, body, pilot)| {
                let (x, y) = pilot.middle(body);
                (id, frame.tile_at(x, y))
            })
            .collect();
        for (id, tile) in pilots {
            let carry = rules::carry_at(ground, tile, storm);
            let lift = rules::lift_share(ground, tile);
            if carry.is_none() && lift <= 0.0 {
                continue;
            }
            let pull = AUTHORITATIVE_RIG_GRAVITY * ground.gravity();
            self.entities.update(id, |entity| {
                if let Some(speed) = carry {
                    let top = speed;
                    let velocity = entity.body.velocity_x;
                    entity.body.velocity_x = quantize(if top > 0.0 && velocity < top {
                        (speed * 4.0).mul_add(dt, velocity).min(top)
                    } else if top < 0.0 && velocity > top {
                        (speed * 4.0).mul_add(dt, velocity).max(top)
                    } else {
                        velocity
                    });
                }
                if lift > 0.0 && entity.body.velocity_y > 0.0 {
                    entity.body.velocity_y =
                        quantize((-lift * pull * 0.5).mul_add(dt, entity.body.velocity_y));
                }
            });
        }
    }

    /// A rig that set down on a sky city's deck or the Tether's anchor is docked: the city's
    /// welcome (a touchdown this tick on the deck plating).
    fn exotic_docking(&self, _ground: &PlanetGround, output: &mut TickOutput) {
        let landed: Vec<(PlayerId, TilePosition)> = output
            .events
            .iter()
            .filter_map(|event| match event.kind {
                GameplayEventKind::Planet {
                    player_id: Some(player_id),
                    what:
                        crate::planet::landing::PlanetEvent::Landed
                        | crate::planet::landing::PlanetEvent::Slid,
                    ..
                } => Some(player_id),
                _ => None,
            })
            .filter_map(|player_id| {
                let rig = self.players.get(&player_id)?;
                let under = self.frame().tile_at(rig.x, rig.y + TILE_SIZE);
                let kind = self.terrain.tile(under)?.kind;
                matches!(
                    kind,
                    TileKind::Exotic(ExoticTile::SkyDeck | ExoticTile::TetherPlate)
                        | TileKind::Alien(_)
                )
                .then_some((player_id, under))
            })
            .collect();
        for (player_id, tile) in landed {
            output.push_event(exotic_event(ExoticEvent::CityDocked, Some(player_id), tile));
        }
    }

    /// The Storm Belt's bolts round the rigs: told, then striking.
    fn exotic_bolts(
        &mut self,
        ground: &PlanetGround,
        rigs: &[(PlayerId, TilePosition)],
        before: u64,
        now: u64,
        storm: f32,
        output: &mut TickOutput,
    ) {
        let width = ground.strip_width();
        let mut bolts: BTreeSet<(u64, i32, i32)> = BTreeSet::new();
        for (_, tile) in rigs {
            if !matches!(
                rules::zone_at(ground, ExoticStyle::GasGiant, *tile),
                Zone::Stratum(0..=2)
            ) {
                continue;
            }
            for bolt in rules::bolts_near(ground, *tile, 2, before, now, storm) {
                bolts.insert((bolt.tell_millis, bolt.column, bolt.top));
            }
        }
        for (tell, column, top) in bolts {
            let bolt = rules::Bolt {
                column,
                top,
                bottom: top + rules::BOLT_CELL.1 - 1,
                tell_millis: tell,
            };
            let at = TilePosition {
                x: column,
                y: i32::midpoint(bolt.top, bolt.bottom),
            };
            if bolt.tell_millis > before && bolt.tell_millis <= now {
                output.push_event(exotic_event(ExoticEvent::LightningTell, None, at));
            }
            if bolt.strike_millis() > before && bolt.strike_millis() <= now {
                output.push_event(exotic_event(ExoticEvent::LightningStrike, None, at));
                for (player_id, tile) in rigs {
                    if bolt.covers(*tile, width) {
                        let damage = rules::BOLT_DAMAGE
                            * crate::enemy::tier_scale(self.exotic_tier(*tile).unwrap_or(8));
                        if self.damage_player(*player_id, damage) {
                            output.push_event(GameplayEventKind::PlayerDamaged {
                                player_id: *player_id,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Diamond hail (the gas giant's Diamond Rain) and meteors (the ring world's plains):
    /// shadows told, then blows that leave their stone.
    #[allow(
        clippy::too_many_arguments,
        reason = "the schedule's window and the storm"
    )]
    fn exotic_fallers(
        &mut self,
        ground: &PlanetGround,
        style: ExoticStyle,
        rigs: &[(PlayerId, TilePosition)],
        before: u64,
        now: u64,
        storm: f32,
        output: &mut TickOutput,
    ) {
        let mut fallers: BTreeSet<(u64, i32, i32, i32)> = BTreeSet::new();
        let mut tells: BTreeSet<(u64, i32)> = BTreeSet::new();
        for (_, tile) in rigs {
            let zone = rules::zone_at(ground, style, *tile);
            let near = match style {
                ExoticStyle::GasGiant => matches!(zone, Zone::Stratum(2..) | Zone::CoreChamber),
                _ => matches!(zone, Zone::Surface | Zone::Stratum(0)),
            };
            if !near {
                continue;
            }
            for faller in rules::fallers_near(ground, style, *tile, 2, before, now, storm) {
                fallers.insert((
                    faller.land_millis,
                    faller.column,
                    faller.from_row,
                    faller.to_row,
                ));
            }
            for faller in rules::fallers_near(
                ground,
                style,
                *tile,
                2,
                before + FALL_TELL_MILLIS,
                now + FALL_TELL_MILLIS,
                storm,
            ) {
                if faller.land_millis - FALL_TELL_MILLIS > before {
                    tells.insert((faller.land_millis, faller.column));
                }
            }
        }
        if style == ExoticStyle::Ring {
            for (_, column) in tells {
                let at = TilePosition {
                    x: column,
                    y: ground.surface_row(column),
                };
                output.push_event(exotic_event(ExoticEvent::MeteorIncoming, None, at));
            }
        }
        for (_, column, from, to) in fallers {
            self.land_faller(style, column, from, to, output);
        }
    }

    /// A hailstone or meteorite lands on the first solid ground of `column` between `from`
    /// and `to`: a blow round it, its stone left on top.
    fn land_faller(
        &mut self,
        style: ExoticStyle,
        column: i32,
        from: i32,
        to: i32,
        output: &mut TickOutput,
    ) {
        let Some(floor) = (from..=to).find(|row| {
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
        let (cx, cy) = self.frame().centre_of(at.x, at.y);
        let reach = FALL_RADIUS_TILES * TILE_SIZE;
        let struck: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(id, rig)| !self.in_space(**id) && (rig.x - cx).hypot(rig.y - cy) <= reach)
            .map(|(id, _)| *id)
            .collect();
        let tier = crate::enemy::tier_scale(self.exotic_tier(at).unwrap_or(8));
        for player_id in &struck {
            if self.damage_player(*player_id, FALL_DAMAGE * tier) {
                output.push_event(GameplayEventKind::PlayerDamaged {
                    player_id: *player_id,
                });
            }
        }
        // Its stone stays where it fell (never on a body).
        let stone = match style {
            ExoticStyle::GasGiant => ExoticTile::DiamondHail,
            _ => ExoticTile::Meteorite,
        };
        let clear = struck.is_empty()
            && self.pilots().all(|(_, body, pilot)| {
                let (px, py) = pilot.middle(body);
                (px - cx).abs() > TILE_SIZE || (py - cy).abs() > TILE_SIZE * 1.5
            });
        if clear
            && self
                .terrain
                .tile(at)
                .is_some_and(|tile| tile.kind == TileKind::Air)
        {
            let _ = self.terrain.set_kind(at, TileKind::Exotic(stone));
        }
        let what = match style {
            ExoticStyle::GasGiant => ExoticEvent::DiamondHail,
            _ => ExoticEvent::MeteorImpact,
        };
        output.push_event(exotic_event(what, struck.first().copied(), at));
    }

    /// The Geyser Fields' vents round the rigs: hissing, then blowing.
    fn exotic_geysers(
        &mut self,
        ground: &PlanetGround,
        rigs: &[(PlayerId, TilePosition)],
        before: u64,
        now: u64,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let mut vents: BTreeSet<TilePosition> = BTreeSet::new();
        for (_, tile) in rigs {
            if rules::rule_at(ground, ExoticStyle::Locked, *tile) == Some(ExoticRule::Geysers) {
                vents.extend(rules::vents_near(ground, *tile, 30));
            }
        }
        for vent in vents {
            if self
                .terrain
                .tile(vent)
                .is_none_or(|tile| tile.kind != TileKind::Exotic(ExoticTile::GeyserVent))
            {
                continue;
            }
            let was = rules::geyser_phase(ground, vent, before);
            let is = rules::geyser_phase(ground, vent, now);
            if was != is {
                match is {
                    GeyserPhase::Hissing => {
                        output.push_event(exotic_event(ExoticEvent::GeyserHiss, None, vent));
                    }
                    GeyserPhase::Blowing => {
                        output.push_event(exotic_event(ExoticEvent::GeyserErupt, None, vent));
                    }
                    GeyserPhase::Quiet => {}
                }
            }
            if is != GeyserPhase::Blowing {
                continue;
            }
            // The plume: up from the vent through open ground.
            let top = (1..=GEYSER_REACH_ROWS)
                .find(|up| {
                    self.terrain
                        .tile(TilePosition {
                            x: vent.x,
                            y: vent.y - up,
                        })
                        .is_none_or(|tile| !tile.kind.is_open())
                })
                .map_or(vent.y - GEYSER_REACH_ROWS, |up| vent.y - up + 1);
            let night = locked::side_of(ground, i64::from(vent.x)) == Side::Night;
            let hazard = if night {
                PlanetHazard::Cold
            } else {
                PlanetHazard::Heat
            };
            let frame = self.frame();
            let in_plume = |x: f32, y: f32| {
                let tile = frame.tile_at(x, y);
                (tile.x - vent.x).abs() <= 1 && tile.y >= top - 1 && tile.y < vent.y
            };
            let riders: Vec<PlayerId> = self
                .players
                .iter()
                .filter(|(id, rig)| !self.in_space(**id) && in_plume(rig.x, rig.y))
                .map(|(id, _)| *id)
                .collect();
            for player_id in riders {
                let gear = self.players.get(&player_id).map(|rig| rig.hazard_gear);
                if let Some(rig) = self.players.get_mut(&player_id) {
                    rig.velocity_y = quantize(rig.velocity_y.min(-GEYSER_LIFT));
                }
                let share = gear.map_or(1.0, |gear| {
                    crate::frontier::rules::unprotected_share(planet_gear_tier(&gear, hazard))
                });
                let bite = GEYSER_BITE
                    * share
                    * crate::enemy::tier_scale(self.exotic_tier(vent).unwrap_or(9))
                    * dt;
                if bite > 0.0 && self.damage_player(player_id, bite) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
            }
            let pilots: Vec<crate::entity::EntityId> = self
                .pilots()
                .filter(|(_, body, pilot)| {
                    let (x, y) = pilot.middle(body);
                    in_plume(x, y)
                })
                .map(|(id, _, _)| id)
                .collect();
            for id in pilots {
                self.entities.update(id, |entity| {
                    entity.body.velocity_y =
                        quantize(entity.body.velocity_y.min(-GEYSER_LIFT * 0.8));
                });
            }
        }
    }

    /// The Stress Fractures' tidal quakes: the rumble, then rubble over the rigs falls.
    fn exotic_quakes(
        &mut self,
        ground: &PlanetGround,
        rigs: &[(PlayerId, TilePosition)],
        before: u64,
        now: u64,
        output: &mut TickOutput,
    ) {
        let shaken: Vec<(PlayerId, TilePosition)> = rigs
            .iter()
            .filter(|(_, tile)| {
                rules::rule_at(ground, ExoticStyle::Locked, *tile) == Some(ExoticRule::TidalQuakes)
            })
            .copied()
            .collect();
        if shaken.is_empty() {
            return;
        }
        if rules::quake_rumble_starts(ground, before, now) {
            for (player_id, tile) in &shaken {
                output.push_event(exotic_event(
                    ExoticEvent::QuakeRumble,
                    Some(*player_id),
                    *tile,
                ));
            }
        }
        if !rules::quake_shock(ground, before, now) {
            return;
        }
        let rate = self.timing.rate;
        let warning = rate.ticks_for_millis(RUBBLE_WARNING_MILLIS);
        let life = rate.ticks_for_millis(RUBBLE_LIFE_MILLIS);
        let tick = self.simulation_tick.get();
        for (player_id, centre) in shaken {
            // Rubble with open ground under it, nearest first.
            let mut loose: Vec<(i32, TilePosition)> = Vec::new();
            for dy in -10..=2 {
                for dx in -12..=12 {
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    let rubble = self.terrain.tile(at).is_some_and(
                        |tile| matches!(tile.kind, TileKind::Exotic(found) if found.is_loose()),
                    );
                    let open_under = self
                        .terrain
                        .tile(TilePosition {
                            x: at.x,
                            y: at.y + 1,
                        })
                        .is_some_and(|tile| tile.kind.is_open());
                    if rubble && open_under {
                        loose.push((dx.abs() + dy.abs(), at));
                    }
                }
            }
            loose.sort();
            let mut fell = 0;
            for (_, at) in loose.into_iter().take(QUAKE_RUBBLE_MAX) {
                if !self.terrain.set_kind(at, TileKind::Air) {
                    continue;
                }
                let (x, y) = self.frame().centre_of(at.x, at.y);
                self.spawn_boulder(
                    FallingBoulder {
                        x,
                        y,
                        velocity_y: 60.0,
                        warning_seconds: rate.seconds_for_ticks(warning),
                        life: rate.seconds_for_ticks(life),
                    },
                    SimulationTick::new(tick.saturating_add(warning)),
                    SimulationTick::new(tick.saturating_add(life)),
                );
                fell += 1;
            }
            output.push_event(exotic_event(
                ExoticEvent::QuakeShock,
                Some(player_id),
                centre,
            ));
            if fell > 0 {
                output.push_event(GameplayEventKind::BouldersChanged);
            }
        }
    }

    /// An exotic tile `player_id` drilled out at `target`: a junction darkens its conduit, ring
    /// ice vents, relics and trophies are announced.
    pub(super) fn exotic_mined(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: ExoticTile,
        output: &mut TickOutput,
    ) {
        match tile {
            ExoticTile::ConduitJunction => {
                self.cut_conduit(target);
                output.push_event(exotic_event(
                    ExoticEvent::ConduitCut,
                    Some(player_id),
                    target,
                ));
            }
            ExoticTile::RingIce => {
                // Only the buried lenses vent (the Skyring's ice is old and dry).
                let Some((ground, style)) = self.exotic_world() else {
                    return;
                };
                if rules::rule_at(&ground, style, target) != Some(ExoticRule::VentingIce) {
                    return;
                }
                let (tx, ty) = self.frame().centre_of(target.x, target.y);
                if let Some(rig) = self.players.get_mut(&player_id) {
                    let (dx, dy) = (rig.x - tx, rig.y - ty);
                    let length = dx.hypot(dy).max(1.0);
                    rig.velocity_x = quantize(dx / length * VENT_KICK);
                    rig.velocity_y = quantize(dy / length * VENT_KICK - VENT_KICK * 0.3);
                }
                output.push_event(exotic_event(
                    ExoticEvent::IceVented,
                    Some(player_id),
                    target,
                ));
            }
            ExoticTile::StormTrophy
            | ExoticTile::GloamTrophy
            | ExoticTile::AnchorTrophy
            | ExoticTile::RelicCache
            | ExoticTile::LandmarkRelic => {
                output.push_event(exotic_event(
                    ExoticEvent::RelicFound,
                    Some(player_id),
                    target,
                ));
            }
            _ => {}
        }
    }

    /// Darkens the live conduit joined to `start` (a cut junction), up to
    /// [`CONDUIT_RUN_MAX`] tiles.
    fn cut_conduit(&mut self, start: TilePosition) {
        let mut queue = vec![start];
        let mut seen: BTreeSet<TilePosition> = BTreeSet::new();
        let mut darkened = 0;
        while let Some(at) = queue.pop() {
            if darkened >= CONDUIT_RUN_MAX || !seen.insert(at) {
                continue;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = TilePosition {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                if (next.x - start.x).abs() > 80 || seen.contains(&next) {
                    continue;
                }
                let live = self.terrain.tile(next).is_some_and(|tile| {
                    matches!(
                        tile.kind,
                        TileKind::Exotic(ExoticTile::ConduitLive | ExoticTile::ConduitJunction)
                    )
                });
                if live
                    && self
                        .terrain
                        .set_kind(next, TileKind::Exotic(ExoticTile::ConduitDead))
                {
                    darkened += 1;
                    queue.push(next);
                }
            }
        }
    }
}

/// An exotic world's event at `tile`.
pub(super) const fn exotic_event(
    what: ExoticEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Exotic {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}
