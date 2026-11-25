//! The wild worlds in play (Wave 12 planets-a; the data lives in [`crate::planet::wilds`]).
//!
//! One pass a tick over the ground of a jungle, ocean archipelago or glacier planet (a body's
//! ground pass, or a world that is one planet), returning at once anywhere else:
//!
//! * **Hazards** ([`PlanetHazard`]): cold drinks fuel (twice as fast in a blizzard, three
//!   times in the Under-Ice Sea's water); a black smoker's heat burns the hull; the Trench's
//!   water crushes it; spore-laden air fouls it. Each is countered by the Descent's rig system
//!   ([`PlanetHazard::descent_counter`]) or by planet gear ([`WorldState::planet_gear_counters`],
//!   TODO-SEAM w12-stations), and only rigs feel them (the pilot's suit is its own).
//! * **Signature rules**: grasping roots hold back a rig rubbing against them; a rig resting on
//!   rime breaks through (snow bridges over crevasses); at night tunnels through the glacier
//!   refreeze as rime and by day the rime thaws; an icicle over a passing rig cracks and falls
//!   (a falling boulder with a short warning). Puffcaps, lifesap and relics answer the drill
//!   ([`WorldState::wild_tile_mined`]). Slick ice and the sea's submarine mode are Descent IV's
//!   own movement hooks, switched on for wild worlds ([`WorldState::wild_water_rules`]).
//! * **Weather** ([`crate::planet::wilds::life::weather_at`]): a squall or a blizzard pushes
//!   craft over the open surface; the turn of a storm is announced.
//! * **Creatures** (`world/enemies/wilds.rs`): spawned round each rig from the theme's roster,
//!   despawned when far.

use crate::{
    entity::EntityKind,
    event::{GameplayEventKind, TickOutput},
    fluid::SUBMARINE_SHARE,
    hazard::{FallingBoulder, HazardCloud},
    ids::{PlayerId, SimulationTick},
    planet::{
        BodyGround, PlanetGround,
        wilds::{
            PlanetHazard, SignatureRule, WildTheme, WildTile, WildsEvent,
            life::{self, Weather},
        },
    },
    quantize::quantize_simulation_scalar as quantize,
    rig::RigPartKind,
    terrain::{TileKind, TilePosition},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// How hard living roots hold a rig back (share of its speed lost per second).
pub const ROOT_GRIP_PER_SECOND: f32 = 3.5;
/// Fuel the heaters burn in the cold, share of the tank per second.
pub const COLD_FUEL_SHARE_PER_SECOND: f32 = 0.004;
/// The cold's multipliers: a blizzard on the open surface, the Under-Ice Sea's water.
pub const BLIZZARD_COLD: f32 = 2.0;
pub const COLD_WATER: f32 = 3.0;
/// A smoker's heat: reach (tiles) and hull per second.
pub const SMOKER_REACH: i32 = 2;
pub const SMOKER_DAMAGE_PER_SECOND: f32 = 6.0;
/// The Thermal Hull's share of a smoker's heat that still gets through.
pub const THERMAL_HULL_HEAT: f32 = 0.25;
/// Hull per second the Trench's water takes without the Pressure Hull.
pub const PRESSURE_DAMAGE_PER_SECOND: f32 = 1.5;
/// Hull per second spore-laden air takes without the Bio-Filter.
pub const SPORE_AIR_DAMAGE_PER_SECOND: f32 = 0.5;
/// Share of the hull a drilled lifesap node mends.
pub const LIFESAP_MEND: f32 = 0.12;
/// A burst puffcap's cloud: radius (pixels) and life (seconds).
pub const SPORE_CLOUD_RADIUS: f32 = 52.0;
pub const SPORE_CLOUD_LIFE: f32 = 7.0;
/// The refreeze and thaw sweep: how often, how far round each rig (tiles), and the chance of
/// each open tile (one in `n`).
pub const FREEZE_SWEEP_MILLIS: u64 = 1_000;
pub const FREEZE_REACH: i32 = 6;
const REFREEZE_ONE_IN: u64 = 9;
const THAW_ONE_IN: u64 = 5;
/// Rows over a rig an icicle notices it, and the warning before it falls.
pub const ICICLE_REACH: i32 = 8;
pub const ICICLE_WARNING_MILLIS: u64 = 350;
const ICICLE_FALL_SPEED: f32 = 140.0;
const ICICLE_LIFE_MILLIS: u64 = 4_000;
/// A hazard event repeats at most this often while it lasts.
const HAZARD_EVENT_MILLIS: u64 = 3_000;

impl WorldState {
    /// The wild world this pass's ground is, with its life (`None` anywhere else).
    #[must_use]
    pub fn wild_world(&self) -> Option<(&PlanetGround, &'static WildTheme)> {
        let planet = self.planet()?;
        let wild = life::wild_theme(planet.params().theme)?;
        Some((planet, wild))
    }

    /// Whether Descent IV's water and ice rules (submarine mode, slick ice) run here: on a
    /// wild world's ground.
    #[must_use]
    pub fn wild_water_rules(&self) -> bool {
        self.wild_world().is_some()
    }

    /// The weather on this wild world now.
    #[must_use]
    pub fn wild_weather(&self) -> Option<Weather> {
        let (planet, _) = self.wild_world()?;
        life::weather_at(planet.params(), self.millis_at(self.simulation_tick))
    }

    /// Whether it is night on this wild world now.
    #[must_use]
    pub fn wild_night(&self) -> bool {
        self.wild_world().is_some_and(|(planet, _)| {
            life::is_night(planet.params(), self.millis_at(self.simulation_tick))
        })
    }

    /// Whether `player_id`'s planet gear counters `hazard`. TODO-SEAM w12-stations: the
    /// stations track's planet gear line (heat, cold, toxic air, high gravity) answers here;
    /// until it lands only the Descent's rig systems counter ([`PlanetHazard::descent_counter`]).
    #[must_use]
    pub fn planet_gear_counters(&self, player_id: PlayerId, hazard: PlanetHazard) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        hazard
            .descent_counter()
            .is_some_and(|system| player.hazard_gear.has_system(system))
    }

    /// The hazards `player_id`'s rig feels now, with the cold's multiplier.
    #[must_use]
    pub fn wild_hazards_for(&self, player_id: PlayerId) -> Vec<(PlanetHazard, f32)> {
        let Some((planet, _)) = self.wild_world() else {
            return Vec::new();
        };
        let Some(player) = self.players.get(&player_id) else {
            return Vec::new();
        };
        let tile = self.frame().tile_at(player.x, player.y);
        let mut hazards: Vec<(PlanetHazard, f32)> = life::hazards_at(planet, tile.x, tile.y)
            .iter()
            .map(|hazard| (*hazard, 1.0))
            .collect();
        let wet = self
            .submersion(player.x, player.y, AUTHORITATIVE_PLAYER_RADIUS)
            .share
            >= SUBMARINE_SHARE;
        let surface = planet.surface_row(tile.x);
        let open_air = tile.y < surface && !wet;
        for (hazard, scale) in &mut hazards {
            match hazard {
                PlanetHazard::Cold => {
                    if open_air
                        && let Some(weather) = self.wild_weather()
                        && weather.kind == life::WeatherKind::Blizzard
                    {
                        *scale = (BLIZZARD_COLD - 1.0).mul_add(weather.strength(), 1.0);
                    }
                    if wet {
                        *scale = COLD_WATER;
                    }
                }
                // The Trench's pressure is the water's: dry halls and shafts are safe.
                PlanetHazard::Pressure if !wet => *scale = 0.0,
                // The heat is the smokers': only close to one.
                PlanetHazard::Heat if !self.smoker_near(tile) => *scale = 0.0,
                _ => {}
            }
        }
        hazards.retain(|(_, scale)| *scale > 0.0);
        hazards
    }

    fn smoker_near(&self, tile: TilePosition) -> bool {
        (-SMOKER_REACH..=SMOKER_REACH).any(|dy| {
            (-SMOKER_REACH..=SMOKER_REACH).any(|dx| {
                self.terrain
                    .tile(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    })
                    .is_some_and(|t| t.kind == TileKind::Wild(WildTile::Smoker))
            })
        })
    }

    /// The wild world's pass (see the module docs).
    pub(super) fn advance_wilds(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some((planet, wild)) = self.wild_world().map(|(planet, wild)| (*planet, wild)) else {
            return;
        };
        let players: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|id| !self.in_space(*id))
            .collect();
        for player_id in &players {
            self.apply_wild_hazards(*player_id, next_tick, output);
            self.apply_wild_rules(&planet, *player_id, next_tick, output);
        }
        self.wild_weather_turns(next_tick, output);
        self.freeze_sweep(&planet, &players, next_tick, output);
        self.advance_wild_creatures(&planet, wild, &players, next_tick);
    }

    /// Whether a timed hazard event for `player_id` falls due this tick.
    fn wild_event_due(&self, player_id: PlayerId, next_tick: SimulationTick, millis: u64) -> bool {
        let every = self.timing.ticks_for_millis(millis).max(1);
        (next_tick.get() + player_id.get()).is_multiple_of(every)
    }

    fn apply_wild_hazards(
        &mut self,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let hazards = self.wild_hazards_for(player_id);
        if hazards.is_empty() {
            return;
        }
        let dt = self.timing.delta_seconds;
        let due = self.wild_event_due(player_id, next_tick, HAZARD_EVENT_MILLIS);
        let Some((x, y)) = self.players.get(&player_id).map(|rig| (rig.x, rig.y)) else {
            return;
        };
        let tile = self.frame().tile_at(x, y);
        for (hazard, scale) in hazards {
            if self.planet_gear_counters(player_id, hazard) {
                continue;
            }
            let event = match hazard {
                PlanetHazard::Cold => {
                    if let Some(rig) = self.players.get_mut(&player_id) {
                        let burn = rig.fuel_capacity * COLD_FUEL_SHARE_PER_SECOND * scale * dt;
                        rig.fuel = quantize((rig.fuel - burn).max(0.0));
                    }
                    WildsEvent::ColdBite
                }
                PlanetHazard::Heat | PlanetHazard::Pressure | PlanetHazard::ToxicAir => {
                    // The Thermal Hull takes most of a smoker's heat.
                    let thermal = hazard == PlanetHazard::Heat
                        && self
                            .equipped_rig_parts
                            .get(&player_id)
                            .is_some_and(|parts| parts.contains(&RigPartKind::ThermalHull));
                    let scale = if thermal {
                        scale * THERMAL_HULL_HEAT
                    } else {
                        scale
                    };
                    let per_second = match hazard {
                        PlanetHazard::Heat => SMOKER_DAMAGE_PER_SECOND,
                        PlanetHazard::Pressure => PRESSURE_DAMAGE_PER_SECOND,
                        _ => SPORE_AIR_DAMAGE_PER_SECOND,
                    };
                    if self.damage_player(player_id, per_second * scale * dt) {
                        output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    }
                    match hazard {
                        PlanetHazard::Heat => WildsEvent::Scalded,
                        PlanetHazard::Pressure => WildsEvent::PressureCreak,
                        _ => continue,
                    }
                }
                // The gravity is the body's own (its field zone); gear that lightens it is the
                // stations track's.
                PlanetHazard::HighGravity => continue,
            };
            if due {
                output.push_event(wild_event(event, Some(player_id), tile));
            }
        }
    }

    fn apply_wild_rules(
        &mut self,
        planet: &PlanetGround,
        player_id: PlayerId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((x, y)) = self.players.get(&player_id).map(|rig| (rig.x, rig.y)) else {
            return;
        };
        let frame = self.frame();
        let tile = frame.tile_at(x, y);
        let dt = self.timing.delta_seconds;
        let drilling = self.active_drills.contains_key(&player_id);
        // Grasping roots: a rig rubbing against living root is held back.
        let reach = AUTHORITATIVE_PLAYER_RADIUS + 3.0;
        let touching_root = [(-reach, 0.0), (reach, 0.0), (0.0, -reach), (0.0, reach)]
            .iter()
            .any(|(dx, dy)| {
                self.terrain
                    .tile(frame.tile_at(x + dx, y + dy))
                    .is_some_and(|t| t.kind == TileKind::Wild(WildTile::LivingRoot))
            });
        if touching_root && !drilling {
            if let Some(rig) = self.players.get_mut(&player_id) {
                let keep = ROOT_GRIP_PER_SECOND.mul_add(-dt, 1.0).max(0.0);
                rig.velocity_x = quantize(rig.velocity_x * keep);
                if rig.velocity_y < 0.0 {
                    rig.velocity_y = quantize(rig.velocity_y * keep);
                }
            }
            if self.wild_event_due(player_id, next_tick, HAZARD_EVENT_MILLIS) {
                output.push_event(wild_event(WildsEvent::RootsSnag, Some(player_id), tile));
            }
        }
        // Rime gives way under a resting rig (snow bridges over crevasses, refrozen floors).
        let under = frame.tile_at(x, y + AUTHORITATIVE_PLAYER_RADIUS + 2.0);
        if self
            .terrain
            .tile(under)
            .is_some_and(|t| t.kind == TileKind::Wild(WildTile::Rime))
        {
            let _ = self.terrain.set_kind(under, TileKind::Air);
            output.push_event(wild_event(WildsEvent::RimeBroke, Some(player_id), under));
        }
        // Weather: wind over the open surface.
        if let Some(weather) = self.wild_weather()
            && weather.intensity > 0
            && weather.kind.wind() > 0.0
            && tile.y < planet.surface_row(tile.x)
            && self.submersion(x, y, AUTHORITATIVE_PLAYER_RADIUS).share < SUBMARINE_SHARE
        {
            let direction = self.wind_direction(planet);
            let push = weather.kind.wind() * weather.strength() * direction * dt;
            if let Some(rig) = self.players.get_mut(&player_id) {
                rig.velocity_x = quantize(rig.velocity_x + push);
            }
        }
        // Icicles over a rig in the Permafrost Caves crack and fall.
        if life::signature_at(planet, tile.x, tile.y) == Some(SignatureRule::Icicles)
            && self.wild_event_due(player_id, next_tick, 250)
        {
            self.drop_icicle_over(tile, output);
        }
    }

    /// The wind's direction this weather cycle (+1 east, -1 west).
    fn wind_direction(&self, planet: &PlanetGround) -> f32 {
        let cycle = self.millis_at(self.simulation_tick) / (life::WEATHER_CYCLE_SECONDS * 1_000);
        if life::life_roll(planet, 7, i64::try_from(cycle).unwrap_or(0), 0).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        }
    }

    /// The first icicle over `tile` within [`ICICLE_REACH`] rows (open air between), cracked
    /// loose: it falls as a boulder after a short warning.
    fn drop_icicle_over(&mut self, tile: TilePosition, output: &mut TickOutput) {
        for dx in [0, -1, 1] {
            for up in 1..=ICICLE_REACH {
                let at = TilePosition {
                    x: tile.x + dx,
                    y: tile.y - up,
                };
                let Some(found) = self.terrain.tile(at) else {
                    break;
                };
                if found.kind.is_open() {
                    continue;
                }
                if found.kind == TileKind::Wild(WildTile::Icicle) {
                    let _ = self.terrain.set_kind(at, TileKind::Air);
                    let (x, y) = self.frame().centre_of(at.x, at.y);
                    let rate = self.timing.rate;
                    let warning = rate.ticks_for_millis(ICICLE_WARNING_MILLIS);
                    let life = rate.ticks_for_millis(ICICLE_LIFE_MILLIS);
                    let now = self.simulation_tick.get();
                    self.spawn_boulder(
                        FallingBoulder {
                            x,
                            y,
                            velocity_y: ICICLE_FALL_SPEED,
                            warning_seconds: rate.seconds_for_ticks(warning),
                            life: rate.seconds_for_ticks(life),
                        },
                        SimulationTick::new(now.saturating_add(warning)),
                        SimulationTick::new(now.saturating_add(life)),
                    );
                    output.push_event(wild_event(WildsEvent::IcicleCrack, None, at));
                    output.push_event(GameplayEventKind::BouldersChanged);
                    return;
                }
                break;
            }
        }
    }

    /// A storm's first moment is announced.
    fn wild_weather_turns(&self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some((planet, _)) = self.wild_world() else {
            return;
        };
        let before = life::weather_at(planet.params(), self.millis_at(self.simulation_tick));
        let after = life::weather_at(planet.params(), self.millis_at(next_tick));
        if let (Some(before), Some(after)) = (before, after)
            && before.intensity == 0
            && after.intensity > 0
        {
            let spot = self
                .players
                .values()
                .next()
                .map_or(TilePosition { x: 0, y: 0 }, |rig| {
                    self.frame().tile_at(rig.x, rig.y)
                });
            output.push_event(wild_event(WildsEvent::StormRising, None, spot));
        }
    }

    /// The glacier's refreeze (night) and thaw (day) round each rig.
    fn freeze_sweep(
        &mut self,
        planet: &PlanetGround,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let every = self.timing.ticks_for_millis(FREEZE_SWEEP_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(every) {
            return;
        }
        let night = life::is_night(planet.params(), self.millis_at(next_tick));
        let sweep = next_tick.get() / every;
        let frame = self.frame();
        // Bodies the ice must not close round: rigs and pilots.
        let mut keep_clear: Vec<TilePosition> = self
            .players
            .values()
            .map(|rig| frame.tile_at(rig.x, rig.y))
            .collect();
        keep_clear.extend(
            self.pilots()
                .map(|(_, body, _)| frame.tile_at(body.x, body.y)),
        );
        for player_id in players {
            let Some(centre) = self
                .players
                .get(player_id)
                .map(|rig| frame.tile_at(rig.x, rig.y))
            else {
                continue;
            };
            if life::signature_at(planet, centre.x, centre.y) != Some(SignatureRule::SlickIce) {
                continue;
            }
            let mut changed = false;
            for dy in -FREEZE_REACH..=FREEZE_REACH {
                for dx in -FREEZE_REACH..=FREEZE_REACH {
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    let Some(now) = self.terrain.tile(at) else {
                        continue;
                    };
                    let hash = life::life_roll(
                        planet,
                        8,
                        i64::from(at.x) ^ i64::try_from(sweep).unwrap_or(0) << 20,
                        i64::from(at.y),
                    );
                    if night && now.kind == TileKind::Air {
                        let ice = matches!(
                            planet.generated_tile(at.x, at.y).kind,
                            TileKind::Wild(WildTile::GlacierIce | WildTile::BlueIce)
                        );
                        let clear = keep_clear
                            .iter()
                            .all(|body| (body.x - at.x).abs() > 2 || (body.y - at.y).abs() > 2);
                        if ice
                            && clear
                            && hash.is_multiple_of(REFREEZE_ONE_IN)
                            && life::signature_at(planet, at.x, at.y)
                                == Some(SignatureRule::SlickIce)
                        {
                            changed |= self.terrain.set_kind(at, TileKind::Wild(WildTile::Rime));
                        }
                    } else if !night
                        && now.kind == TileKind::Wild(WildTile::Rime)
                        && hash.is_multiple_of(THAW_ONE_IN)
                    {
                        let _ = self.terrain.set_kind(at, TileKind::Air);
                    }
                }
            }
            if changed {
                output.push_event(wild_event(WildsEvent::Refroze, Some(*player_id), centre));
            }
        }
    }

    /// A wild tile `player_id` drilled out at `target`: puffcaps burst, lifesap mends, relics
    /// and caches are announced.
    pub(super) fn wild_tile_mined(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: WildTile,
        output: &mut TickOutput,
    ) {
        let (x, y) = self.frame().centre_of(target.x, target.y);
        match tile {
            WildTile::Puffcap => {
                let _ = self.spawn_gas_cloud(HazardCloud {
                    x,
                    y,
                    life: SPORE_CLOUD_LIFE,
                    radius: SPORE_CLOUD_RADIUS,
                });
                output.push_event(wild_event(WildsEvent::SporeBurst, Some(player_id), target));
            }
            WildTile::Lifesap => {
                if let Some(rig) = self.players.get_mut(&player_id) {
                    let max = rig.max_hull();
                    rig.hull = quantize(LIFESAP_MEND.mul_add(max, rig.hull).min(max));
                }
                output.push_event(wild_event(
                    WildsEvent::LifesapMended,
                    Some(player_id),
                    target,
                ));
            }
            WildTile::ElderSeed | WildTile::RuinRelic | WildTile::TitanHeart => {
                output.push_event(wild_event(
                    WildsEvent::LandmarkRelic,
                    Some(player_id),
                    target,
                ));
            }
            WildTile::Cache => {
                output.push_event(wild_event(WildsEvent::SecretCache, Some(player_id), target));
            }
            _ => {}
        }
    }

    /// Whether `tile` holds a climbable vine (pilots on foot climb vines like ladders).
    #[must_use]
    pub fn wild_vine_at(&self, tile: TilePosition) -> bool {
        self.planet().is_some_and(|planet| {
            crate::planet::wilds::jungle::vine_at(planet, i64::from(tile.x), tile.y)
                && self
                    .terrain
                    .tile(tile)
                    .is_some_and(|t| t.kind == TileKind::Air)
        })
    }

    /// The wild creatures in this pass's ground.
    pub(super) fn wild_creature_count_near(&self, x: f32, y: f32, reach: f32) -> usize {
        self.entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| match &entity.state {
                crate::entity::EntityState::Enemy(state) => state.kind.is_wilds(),
                _ => false,
            })
            .filter(|entity| {
                let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                dx.mul_add(dx, dy * dy) <= reach * reach
            })
            .count()
    }
}

/// A wild world's event at `tile`.
pub(super) const fn wild_event(
    what: WildsEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Wilds {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}
