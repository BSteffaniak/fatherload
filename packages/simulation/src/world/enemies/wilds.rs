//! The wild worlds' creatures (Wave 12 planets-a): what sets them apart from the generic state
//! machine in `world/enemies.rs`, and their spawning round the rigs on a wild planet.
//!
//! Everything runs in the planet's local frame (its strip, where "down" is toward the core),
//! with the generic enemy body rules (`step_body`, `free_from_rock`, `keep_off_bodies`,
//! `site_fits`, contact boxes, line-of-sight noticing):
//!
//! * **The neutral fauna** (trundlehorns, woolhorns, shellstriders) never attack first; struck,
//!   they fight back with a slow, telegraphed charge. The herds bed down at night. **Harmless
//!   fauna** (glimmerfins, rimeskippers) scatter from rigs.
//! * **Ambushers**: a vinestalker hangs under the canopy and sees only what passes below it,
//!   drops, then climbs back to hang still; a mossprowler lies flat in the undergrowth (dormant)
//!   until a rig comes close, pounces, and lies down again where it ends up.
//! * **Rimewolf packs** circle their prey and leap together on one shared rhythm of world time
//!   ([`WOLF_CIRCLE_MILLIS`]); in a blizzard they see farther.
//! * **Sawjaws** circle while their dash recharges, then dash in; the sea's swimmers never
//!   leave the water (`wilds_after_move`).
//! * **Crevassewyrms** wait in the ice walls (they tunnel through ice and permafrost, nothing
//!   else) and lunge out.

use crate::{
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityKind, EntityState},
    ids::{PlayerId, SimulationTick},
    physics,
    planet::{
        BodyGround, PlanetGround,
        wilds::{WildTheme, life, life::Habitat},
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{Intent, WorldState, direction};

/// A rimewolf pack circles this long, then leaps for as long (a world-time rhythm).
pub const WOLF_CIRCLE_MILLIS: u64 = 2_400;
/// How often the spawner looks round each rig.
pub const WILD_SPAWN_MILLIS: u64 = 2_000;
/// The most wild creatures round one rig, and how far "round" is (tiles).
pub const WILD_NEAR_CAP: usize = 6;
pub const WILD_NEAR_TILES: f32 = 44.0;
/// Creatures farther than this from every rig leave (tiles).
pub const WILD_DESPAWN_TILES: f32 = 80.0;
/// Spawns happen at least this far from a rig (out of its view), and at most this far.
const SPAWN_MIN_TILES: i32 = 16;
const SPAWN_SPREAD_TILES: i32 = 18;
/// Harmless fauna scatter from a rig this close (tiles).
const SCATTER_TILES: f32 = 5.0;

impl WorldState {
    /// Whether `state`'s creature is at peace with the crew (auto-aim spares it): the wild
    /// worlds' neutral fauna until struck.
    pub(in crate::world) const fn wilds_peaceful(state: &EnemyState) -> bool {
        matches!(
            state.kind,
            EnemyKind::Trundlehorn
                | EnemyKind::Shellstrider
                | EnemyKind::Woolhorn
                | EnemyKind::Glimmerfin
                | EnemyKind::Rimeskipper
        ) && state.last_attacker.is_none()
    }

    /// The wild worlds' say on who `state`'s creature notices: `Some(answer)` overrides the
    /// generic perception, `None` leaves it.
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn wilds_notice(
        &self,
        body: &Body,
        state: &EnemyState,
        generic: impl FnOnce() -> Option<PlayerId>,
    ) -> Option<Option<PlayerId>> {
        if !state.kind.is_wilds() {
            return None;
        }
        let below = |player: PlayerId| {
            self.players
                .get(&player)
                .is_some_and(|rig| rig.y > TILE_SIZE.mul_add(1.5, body.y))
        };
        Some(match state.kind {
            // The peaceful and the harmless only ever fight back (a hit sets their target).
            EnemyKind::Trundlehorn
            | EnemyKind::Shellstrider
            | EnemyKind::Woolhorn
            | EnemyKind::Glimmerfin
            | EnemyKind::Rimeskipper => None,
            EnemyKind::Vinestalker => generic().filter(|player| below(*player)),
            EnemyKind::Rimewolf
                if self
                    .wild_weather()
                    .is_some_and(|weather| weather.intensity > 300) =>
            {
                // Blizzard hunters: farther sight, still needing a clear line.
                let reach = state.kind.stats().sight_tiles * TILE_SIZE * 1.6;
                self.players
                    .iter()
                    .filter(|(_, rig)| rig.hull > 0.0)
                    .map(|(id, rig)| {
                        let (dx, dy) = (rig.x - body.x, rig.y - body.y);
                        (dx.mul_add(dx, dy * dy).sqrt(), *id, rig.x, rig.y)
                    })
                    .filter(|(distance, ..)| *distance <= reach)
                    .filter(|(_, _, x, y)| {
                        physics::line_clear(&self.terrain, body.x, body.y, *x, *y)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                    .map(|(_, id, ..)| id)
            }
            _ => generic(),
        })
    }

    /// Whether a wild world lets `kind` appear at `tile` (`None`: not a wild creature).
    pub(in crate::world) fn wilds_habitat(
        &self,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> Option<bool> {
        if !kind.is_wilds() {
            return None;
        }
        let Some((planet, wild)) = self.wild_world() else {
            // Nowhere but on its own world.
            return Some(false);
        };
        let Some(creature) = wild.creatures.iter().find(|creature| creature.kind == kind) else {
            return Some(false);
        };
        Some(self.habitat_holds(planet, creature.habitat, tile))
    }

    fn open_air(&self, tile: TilePosition) -> bool {
        self.terrain
            .tile(tile)
            .is_some_and(|t| t.kind == TileKind::Air)
    }

    fn deep_water(&self, tile: TilePosition) -> bool {
        self.terrain
            .fluid_at(tile)
            .is_some_and(|cell| cell.level >= crate::fluid::FLUID_FULL / 2)
    }

    fn solid(&self, tile: TilePosition) -> bool {
        self.terrain.tile(tile).is_some_and(|t| !t.kind.is_open())
    }

    fn habitat_holds(&self, planet: &PlanetGround, habitat: Habitat, tile: TilePosition) -> bool {
        let up = TilePosition {
            x: tile.x,
            y: tile.y - 1,
        };
        let down = TilePosition {
            x: tile.x,
            y: tile.y + 1,
        };
        let surface = planet.surface_row(tile.x);
        let stratum_is = |n: u8| {
            tile.y > surface + 2
                && tile.y < planet.core_chamber_top()
                && planet.stratum_index(tile.x, tile.y) == usize::from(n)
        };
        match habitat {
            Habitat::Surface => {
                self.open_air(tile) && self.open_air(up) && self.solid(down) && tile.y < surface + 2
            }
            Habitat::Hanging => {
                self.open_air(tile)
                    && self.solid(up)
                    && tile.y < surface
                    && (1..=3).all(|below| {
                        self.open_air(TilePosition {
                            x: tile.x,
                            y: tile.y + below,
                        })
                    })
            }
            Habitat::Water => {
                self.deep_water(tile)
                    && (-2..=2)
                        .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
                        .filter(|(dx, dy)| {
                            self.deep_water(TilePosition {
                                x: tile.x + dx,
                                y: tile.y + dy,
                            })
                        })
                        .count()
                        >= 16
            }
            Habitat::Caves(n) => stratum_is(n) && self.open_air(tile) && self.solid(down),
            Habitat::CaveAir(n) => {
                stratum_is(n) && self.open_air(tile) && self.open_air(up) && self.open_air(down)
            }
        }
    }

    /// The wild worlds' movement for `state`'s creature this tick, or `None` for the generic
    /// behaviour (see the module docs).
    pub(super) fn wilds_behave(
        &self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
    ) -> Option<Intent> {
        let spec = state.kind.stats();
        let frame = self.frame();
        let (home_x, home_y) = frame.centre_of(state.home.0, state.home.1);
        let millis = self.millis_at(now);
        match state.kind {
            EnemyKind::Trundlehorn | EnemyKind::Woolhorn => {
                // The herds bed down at night (unless struck).
                let night = self.wild_night_at(frame.tile_at(body.x, body.y).x);
                match (night, state.mode) {
                    (true, EnemyMode::Idle) if state.last_attacker.is_none() => {
                        state.mode = EnemyMode::Dormant;
                        Some(Intent::default())
                    }
                    (false, EnemyMode::Dormant) => {
                        state.mode = EnemyMode::Idle;
                        None
                    }
                    _ => None,
                }
            }
            EnemyKind::Glimmerfin | EnemyKind::Rimeskipper if state.last_attacker.is_none() => {
                // Scatter from a rig close by, else drift about home.
                let threat = self
                    .players
                    .values()
                    .map(|rig| (rig.x, rig.y))
                    .find(|(x, y)| {
                        let (dx, dy) = (x - body.x, y - body.y);
                        dx.mul_add(dx, dy * dy) < (SCATTER_TILES * TILE_SIZE).powi(2)
                    });
                let (tx, ty) = threat?;
                let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                let away = if nx.abs() < 0.1 { 1.0 } else { nx.signum() };
                let lift = if state.kind == EnemyKind::Glimmerfin {
                    ny * TILE_SIZE * 2.0
                } else {
                    0.0
                };
                Some(Intent {
                    goal: Some((away.mul_add(TILE_SIZE * 5.0, body.x), body.y + lift)),
                    speed: spec.speed * 1.6,
                })
            }
            EnemyKind::Rimewolf if state.mode == EnemyMode::Chase => {
                // The pack circles until the shared rhythm turns, then leaps together.
                if (millis / WOLF_CIRCLE_MILLIS).is_multiple_of(2) {
                    return None;
                }
                let (tx, ty) = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, rig.y))?;
                let side: f32 = if (state.seed & 1) == 0 { 1.0 } else { -1.0 };
                #[allow(clippy::cast_precision_loss, reason = "a step of three")]
                let swing = ((millis / 600 + u64::from(state.seed)) % 3) as f32 - 1.0;
                Some(Intent {
                    goal: Some((side.mul_add(TILE_SIZE * 4.5, tx) + swing * TILE_SIZE, ty)),
                    speed: spec.speed,
                })
            }
            EnemyKind::Sawjaw if state.mode == EnemyMode::Chase && state.attack_ready > now => {
                // Circle the prey while the dash recharges.
                let (tx, ty) = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, rig.y))?;
                Some(Intent {
                    goal: Some(self.orbit((tx, ty), state, now, TILE_SIZE * 4.0)),
                    speed: spec.speed * 1.15,
                })
            }
            EnemyKind::Vinestalker | EnemyKind::Mossprowler if state.mode == EnemyMode::Return => {
                // Back home, the ambusher hangs (or lies) still again.
                let (dx, dy) = (body.x - home_x, body.y - home_y);
                if dx.mul_add(dx, dy * dy) < (TILE_SIZE * 0.8).powi(2) {
                    state.mode = EnemyMode::Dormant;
                    return Some(Intent::default());
                }
                None
            }
            EnemyKind::Vinestalker | EnemyKind::Mossprowler if state.mode == EnemyMode::Idle => {
                // The stalker hangs still where it is, the prowler lies flat: ambushers never
                // wander about in plain sight.
                state.mode = EnemyMode::Dormant;
                Some(Intent::default())
            }
            _ => None,
        }
    }

    /// After a wild creature moved: the sea's swimmers stay in the water.
    pub(super) fn wilds_after_move(&self, body: &mut Body, state: &EnemyState) {
        if !state.kind.can_swim() {
            return;
        }
        let tile = self.frame().tile_at(body.x, body.y);
        if !self.deep_water(tile) {
            // Out of the water: back down into it, never up into the air.
            body.velocity_y = body.velocity_y.max(80.0);
            let below = TilePosition {
                x: tile.x,
                y: tile.y + 1,
            };
            if self.deep_water(below) {
                body.y = crate::quantize::quantize_simulation_scalar(body.y + 2.0);
            }
        }
    }

    /// Spawns and despawns the wild world's creatures round its rigs (see the module docs).
    pub(in crate::world) fn advance_wild_creatures(
        &mut self,
        planet: &PlanetGround,
        wild: &'static WildTheme,
        players: &[PlayerId],
        next_tick: SimulationTick,
    ) {
        let every = self.timing.ticks_for_millis(WILD_SPAWN_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(every) {
            return;
        }
        let frame = self.frame();
        let rigs: Vec<(PlayerId, f32, f32)> = players
            .iter()
            .filter_map(|id| self.players.get(id).map(|rig| (*id, rig.x, rig.y)))
            .collect();
        // The far ones leave.
        let far = WILD_DESPAWN_TILES * TILE_SIZE;
        let gone: std::collections::BTreeSet<_> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| match &entity.state {
                EntityState::Enemy(state) => state.kind.is_wilds(),
                _ => false,
            })
            .filter(|entity| {
                rigs.iter().all(|(_, x, y)| {
                    let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                    dx.mul_add(dx, dy * dy) > far * far
                })
            })
            .map(|entity| entity.id)
            .collect();
        if !gone.is_empty() {
            let _ = self.entities.despawn_many(&gone);
        }
        let peaceful = self.enemies.difficulty == EnemyDifficulty::Peaceful;
        let sweep = next_tick.get() / every;
        for (player_id, x, y) in rigs {
            if self.wild_creature_count_near(x, y, WILD_NEAR_TILES * TILE_SIZE) >= WILD_NEAR_CAP {
                continue;
            }
            let rig = frame.tile_at(x, y);
            // Nothing comes into a theme boss's lair but its brood (Wave 14 finish).
            if self.in_theme_lair(rig) {
                continue;
            }
            let roll = life::life_roll(
                planet,
                1,
                i64::try_from(sweep).unwrap_or(0),
                i64::try_from(player_id.get()).unwrap_or(0),
            );
            let Some((kind, site, count)) = self.wild_spawn_site(planet, wild, rig, roll, peaceful)
            else {
                continue;
            };
            let tier = self.tier_at(site);
            for index in 0..count {
                let tile = TilePosition {
                    x: site.x + i32::from(index) * if roll & 1 == 0 { 1 } else { -1 },
                    y: site.y,
                };
                let tile = if self.site_fits(kind, tile) {
                    tile
                } else {
                    site
                };
                let point = Self::spawn_point(frame, kind, tile);
                let _ = self.spawn_enemy_with(kind, point, tier, None, None);
            }
        }
    }

    /// A creature, a site out of view of the rig at `rig` and a pack size, if the dice find one.
    fn wild_spawn_site(
        &self,
        planet: &PlanetGround,
        wild: &'static WildTheme,
        rig: TilePosition,
        roll: u64,
        peaceful: bool,
    ) -> Option<(EnemyKind, TilePosition, u8)> {
        let surface = planet.surface_row(rig.x);
        let underground = rig.y > surface + 6;
        for attempt in 0..8_u64 {
            let hash = life::life_roll(
                planet,
                2,
                i64::try_from(roll).unwrap_or(0),
                i64::try_from(attempt).unwrap_or(0),
            );
            let side = if hash & 1 == 0 { 1 } else { -1 };
            let dx = SPAWN_MIN_TILES
                + i32::try_from((hash >> 4) % u64::from(SPAWN_SPREAD_TILES.unsigned_abs()))
                    .unwrap_or(0);
            let column = rig.x + side * dx;
            // The creatures whose habitat this rig is near.
            let candidates: Vec<_> = wild
                .creatures
                .iter()
                .filter(|creature| !peaceful || creature.neutral)
                .filter(|creature| match creature.habitat {
                    Habitat::Surface | Habitat::Hanging => !underground,
                    // The turtles and the schools keep to the open sea; only hunters
                    // follow a rig down into flooded caves and shafts.
                    Habitat::Water => !underground || !creature.neutral,
                    Habitat::Caves(n) | Habitat::CaveAir(n) => {
                        underground && planet.stratum_index(rig.x, rig.y) == usize::from(n)
                    }
                })
                .collect();
            let total: u32 = candidates.iter().map(|creature| creature.weight).sum();
            if total == 0 {
                return None;
            }
            let mut pick = u32::try_from((hash >> 12) % u64::from(total)).unwrap_or(0);
            let Some(creature) = candidates.iter().find(|creature| {
                if pick < creature.weight {
                    true
                } else {
                    pick -= creature.weight;
                    false
                }
            }) else {
                continue;
            };
            let floor = planet.surface_row(column);
            let rows: Vec<i32> = match creature.habitat {
                Habitat::Surface => (floor - 3..=floor + 1).collect(),
                Habitat::Hanging => (floor - 40..floor - 3).collect(),
                Habitat::Water => (rig.y.min(floor) - 12..=rig.y.min(floor) + 12).collect(),
                Habitat::Caves(_) | Habitat::CaveAir(_) => (rig.y - 12..=rig.y + 12).collect(),
            };
            let found = rows
                .into_iter()
                .map(|y| TilePosition { x: column, y })
                .find(|tile| {
                    self.habitat_holds(planet, creature.habitat, *tile)
                        && self.site_fits(creature.kind, *tile)
                });
            if let Some(site) = found {
                let (low, high) = creature.pack;
                let count =
                    low + u8::try_from((hash >> 24) % u64::from(high - low + 1)).unwrap_or(0);
                return Some((creature.kind, site, count.max(1)));
            }
        }
        None
    }
}
