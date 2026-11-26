//! The outland worlds' creatures and theme bosses (Wave 13 planets-c): what sets them apart
//! from the generic state machine in `world/enemies.rs`, their spawning round the rigs, and
//! each theme boss's lair.
//!
//! Everything runs in the planet's local frame (its strip, "down" toward the core) with the
//! generic enemy body rules (`step_body`, `free_from_rock`, `keep_off_bodies`, `site_fits`,
//! contact boxes fitted to the art, line-of-sight noticing):
//!
//! * **Neutral fauna** never attack first; struck, they fight back (the capgrazer) or flee.
//!   The capgrazer herds bed down at night; ashrunners bolt from a rig that comes close; a
//!   **lanternfly** (the fungal world's symbiont) drifts to the nearest rig and hovers by it,
//!   lighting its way, until struck.
//! * **Bonepickers** circle their prey while their dart recharges, then dart in.
//! * **The theme bosses** wait in the lair under their landmark while its seal stands
//!   ([`crate::planet::outlands::boss_lair`]): a rig entering the lair wakes the boss (an
//!   event), the fight is the generic telegraphed attack of the kind (a wind-up, then a volley)
//!   plus a brood called on a rhythm of world time with a warning first (more often below half
//!   health, never more than [`BROOD_CAP`] at once). Leave the lair and the boss rests (it
//!   comes back whole: no countdown, no lost run). Its fall spends the seal and opens the
//!   reward vault beside the lair. On Peaceful there is no fight: the seal is spent and the
//!   vault opens when a rig first enters.

use crate::{
    enemy::{EnemyDifficulty, EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityKind, EntityState},
    event::TickOutput,
    ids::{PlayerId, SimulationTick},
    planet::{
        BodyGround, PlanetGround,
        outlands::{
            BossLair, LAIR_HALF_WIDTH, OutlandTile, OutlandsEvent, boss_lair,
            life::{self, OutlandTheme},
        },
        wilds::{life::Habitat, wrap_delta},
    },
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{Intent, WorldState, direction};
use crate::world::outlands::outland_event;

/// How often the spawner looks round each rig.
pub const OUTLAND_SPAWN_MILLIS: u64 = 2_000;
/// The most outland creatures round one rig, and how far "round" is (tiles).
pub const OUTLAND_NEAR_CAP: usize = 6;
pub const OUTLAND_NEAR_TILES: f32 = 44.0;
/// Creatures farther than this from every rig leave (tiles).
pub const OUTLAND_DESPAWN_TILES: f32 = 80.0;
const SPAWN_MIN_TILES: i32 = 16;
const SPAWN_SPREAD_TILES: i32 = 18;
/// Ashrunners bolt from a rig this close (tiles); a lanternfly keeps this far from its rig.
const BOLT_TILES: f32 = 6.0;
const LANTERN_TILES: f32 = 2.5;
/// A lanternfly finds a rig this far away (tiles).
const LANTERN_SEEK_TILES: f32 = 12.0;
/// The boss's brood: called this often (and this often below half health), the warning
/// before, at most this many at once.
pub const BROOD_MILLIS: u64 = 14_000;
pub const BROOD_HURT_MILLIS: u64 = 9_000;
pub const BROOD_WARNING_MILLIS: u64 = 1_500;
pub const BROOD_CAP: usize = 4;
/// The boss looks after its lair this often.
const LAIR_MILLIS: u64 = 500;
/// A boss rests when no rig is this far (tiles) round its lair.
const BOSS_REST_TILES: i64 = 34;

impl WorldState {
    /// Whether `state`'s creature is at peace with the crew (auto-aim spares it): the outland
    /// worlds' neutral fauna until struck.
    pub(in crate::world) const fn outlands_peaceful(state: &EnemyState) -> bool {
        state.kind.is_outland_fauna() && state.last_attacker.is_none()
    }

    /// The outland worlds' say on who `state`'s creature notices (`None` leaves the generic
    /// perception): the fauna notice nobody (a hit sets their target).
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) const fn outlands_notice(state: &EnemyState) -> Option<Option<PlayerId>> {
        if state.kind.is_outland_fauna() {
            Some(None)
        } else {
            None
        }
    }

    /// Whether an outland world lets `kind` appear at `tile` (`None`: not an outland creature).
    pub(in crate::world) fn outlands_habitat(
        &self,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> Option<bool> {
        if !kind.is_outlands() {
            return None;
        }
        let Some((planet, life)) = self.outland_world() else {
            return Some(false);
        };
        // The bosses and the tombs' wardens are placed by their lairs and tombs too.
        if kind.is_outland_boss() {
            return Some(true);
        }
        let Some(creature) = life.creatures.iter().find(|creature| creature.kind == kind) else {
            return Some(false);
        };
        Some(self.outland_habitat_holds(planet, creature.habitat, tile))
    }

    fn outland_open(&self, tile: TilePosition) -> bool {
        self.terrain
            .tile(tile)
            .is_some_and(|t| t.kind == TileKind::Air)
    }

    fn outland_solid(&self, tile: TilePosition) -> bool {
        self.terrain.tile(tile).is_some_and(|t| !t.kind.is_open())
    }

    fn outland_habitat_holds(
        &self,
        planet: &PlanetGround,
        habitat: Habitat,
        tile: TilePosition,
    ) -> bool {
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
                self.outland_open(tile)
                    && self.outland_open(up)
                    && self.outland_solid(down)
                    && tile.y < surface + 2
            }
            Habitat::Hanging | Habitat::Water => false,
            Habitat::Caves(n) => {
                stratum_is(n) && self.outland_open(tile) && self.outland_solid(down)
            }
            Habitat::CaveAir(n) => {
                stratum_is(n)
                    && self.outland_open(tile)
                    && self.outland_open(up)
                    && self.outland_open(down)
            }
        }
    }

    /// The outland worlds' movement for `state`'s creature this tick, or `None` for the
    /// generic behaviour (see the module docs).
    pub(super) fn outlands_behave(
        &self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
    ) -> Option<Intent> {
        let spec = state.kind.stats();
        let frame = self.frame();
        match state.kind {
            EnemyKind::Capgrazer => {
                // The herds bed down at night (unless struck).
                let night = self.outland_night_at(frame.tile_at(body.x, body.y).x);
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
            EnemyKind::Ashrunner if state.last_attacker.is_none() => {
                let (tx, ty) = self.nearest_rig_within(body, BOLT_TILES)?;
                let ((nx, _), _) = direction((tx, ty), (body.x, body.y));
                let away = if nx.abs() < 0.1 { 1.0 } else { nx.signum() };
                Some(Intent {
                    goal: Some((away.mul_add(TILE_SIZE * 6.0, body.x), body.y)),
                    speed: spec.speed * 1.5,
                })
            }
            EnemyKind::Ashrunner | EnemyKind::Lanternfly if state.last_attacker.is_some() => {
                // Struck, the harmless flee from whoever struck them.
                let (tx, ty) = state
                    .last_attacker
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, rig.y))?;
                let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                Some(Intent {
                    goal: Some((
                        nx.mul_add(TILE_SIZE * 8.0, body.x),
                        ny.mul_add(TILE_SIZE * 8.0, body.y),
                    )),
                    speed: spec.speed * 1.8,
                })
            }
            EnemyKind::Lanternfly => {
                // The symbiont drifts to the nearest rig and hovers by it, lighting its way.
                let (tx, ty) = self.nearest_rig_within(body, LANTERN_SEEK_TILES)?;
                let side: f32 = if (state.seed & 1) == 0 { 1.0 } else { -1.0 };
                #[allow(clippy::cast_precision_loss, reason = "a slow bob")]
                let bob = ((self.millis_at(now) / 400 + u64::from(state.seed)) % 4) as f32 - 1.5;
                Some(Intent {
                    goal: Some((
                        side.mul_add(TILE_SIZE * LANTERN_TILES, tx),
                        TILE_SIZE.mul_add(-1.5, ty) + bob * 3.0,
                    )),
                    speed: spec.speed,
                })
            }
            EnemyKind::Bonepicker if state.mode == EnemyMode::Chase && state.attack_ready > now => {
                // Circle the prey while the dart recharges.
                let (tx, ty) = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, rig.y))?;
                Some(Intent {
                    goal: Some(self.orbit((tx, ty), state, now, TILE_SIZE * 4.0)),
                    speed: spec.speed,
                })
            }
            _ => None,
        }
    }

    /// The nearest live rig within `tiles` of `body`.
    fn nearest_rig_within(&self, body: &Body, tiles: f32) -> Option<(f32, f32)> {
        let reach = tiles * TILE_SIZE;
        self.players
            .values()
            .filter(|rig| rig.hull > 0.0)
            .map(|rig| {
                let (dx, dy) = (rig.x - body.x, rig.y - body.y);
                (dx.mul_add(dx, dy * dy), rig.x, rig.y)
            })
            .filter(|(distance2, ..)| *distance2 <= reach * reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, x, y)| (x, y))
    }

    /// The outland creatures round `(x, y)` (bosses aside).
    fn outland_creatures_near(&self, x: f32, y: f32, reach: f32) -> usize {
        self.entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| match &entity.state {
                EntityState::Enemy(state) => {
                    state.kind.is_outlands() && !state.kind.is_outland_boss()
                }
                _ => false,
            })
            .filter(|entity| {
                let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                dx.mul_add(dx, dy * dy) <= reach * reach
            })
            .count()
    }

    /// Spawns and despawns the outland world's creatures round its rigs.
    pub(in crate::world) fn advance_outland_creatures(
        &mut self,
        planet: &PlanetGround,
        life: &'static OutlandTheme,
        players: &[PlayerId],
        next_tick: SimulationTick,
    ) {
        let every = self.timing.ticks_for_millis(OUTLAND_SPAWN_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(every) {
            return;
        }
        let frame = self.frame();
        let rigs: Vec<(PlayerId, f32, f32)> = players
            .iter()
            .filter_map(|id| self.players.get(id).map(|rig| (*id, rig.x, rig.y)))
            .collect();
        // The far ones leave (the bosses keep their own lair).
        let far = OUTLAND_DESPAWN_TILES * TILE_SIZE;
        let gone: std::collections::BTreeSet<_> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| match &entity.state {
                EntityState::Enemy(state) => {
                    state.kind.is_outlands() && !state.kind.is_outland_boss()
                }
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
            if self.outland_creatures_near(x, y, OUTLAND_NEAR_TILES * TILE_SIZE) >= OUTLAND_NEAR_CAP
            {
                continue;
            }
            let rig = frame.tile_at(x, y);
            // Nothing comes into a boss's lair but its brood.
            if boss_lair(planet).is_some_and(|lair| {
                lair.contains(i64::from(rig.x), rig.y, i64::from(planet.strip_width()))
            }) {
                continue;
            }
            let roll = life::life_roll(
                planet,
                1,
                i64::try_from(sweep).unwrap_or(0),
                i64::try_from(player_id.get()).unwrap_or(0),
            );
            let Some((kind, site, count)) =
                self.outland_spawn_site(planet, life, rig, roll, peaceful)
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
    fn outland_spawn_site(
        &self,
        planet: &PlanetGround,
        life: &'static OutlandTheme,
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
            let candidates: Vec<_> = life
                .creatures
                .iter()
                .filter(|creature| !peaceful || creature.neutral)
                .filter(|creature| match creature.habitat {
                    Habitat::Surface => !underground,
                    Habitat::Caves(n) | Habitat::CaveAir(n) => {
                        underground && planet.stratum_index(rig.x, rig.y) == usize::from(n)
                    }
                    Habitat::Hanging | Habitat::Water => false,
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
                _ => (rig.y - 12..=rig.y + 12).collect(),
            };
            let found = rows
                .into_iter()
                .map(|y| TilePosition { x: column, y })
                .find(|tile| {
                    self.outland_habitat_holds(planet, creature.habitat, *tile)
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

    // --- The theme boss ------------------------------------------------------------------------

    /// The boss of the lair, if it is awake (its entity, body and state).
    fn lair_boss(&self, lair: &BossLair) -> Option<(crate::entity::EntityId, Body, EnemyState)> {
        self.entities
            .of_kind(EntityKind::Enemy)
            .find_map(|entity| match &entity.state {
                EntityState::Enemy(state) if state.kind == lair.boss.kind => {
                    Some((entity.id, entity.body, *state))
                }
                _ => None,
            })
    }

    /// Wakes, rests and feeds the brood of this body's theme boss (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the lair's beats in order")]
    pub(in crate::world) fn advance_outland_boss(
        &mut self,
        planet: &PlanetGround,
        players: &[PlayerId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(lair) = boss_lair(planet) else {
            return;
        };
        let seal = TilePosition {
            x: lair.seal.0,
            y: lair.seal.1,
        };
        if self
            .terrain
            .tile(seal)
            .is_none_or(|t| t.kind != TileKind::Outland(OutlandTile::BossSeal))
        {
            return;
        }
        let width = i64::from(planet.strip_width());
        let frame = self.frame();
        let rig_in_lair = players.iter().copied().find(|id| {
            self.players.get(id).is_some_and(|rig| {
                let tile = frame.tile_at(rig.x, rig.y);
                lair.contains(i64::from(tile.x), tile.y, width)
            })
        });
        let (middle_x, middle_y) = lair.middle();
        let boss = self.lair_boss(&lair);
        let every = self.timing.ticks_for_millis(LAIR_MILLIS).max(1);
        if next_tick.get().is_multiple_of(every) {
            match (boss, rig_in_lair) {
                (None, Some(player_id)) => {
                    if self.enemies.difficulty == EnemyDifficulty::Peaceful {
                        // No fight on Peaceful: the lair yields its vault.
                        self.boss_lair_won(planet, &lair, Some(player_id), output);
                        return;
                    }
                    let kind = lair.boss.kind;
                    let tile = TilePosition {
                        x: i32::try_from(middle_x.rem_euclid(width)).unwrap_or(0),
                        y: lair.floor - 2,
                    };
                    let point = Self::spawn_point(frame, kind, tile);
                    let tier = self.tier_at(tile);
                    let id = self.spawn_enemy_with(kind, point, tier, None, None);
                    let now = self.simulation_tick;
                    self.entities.update(id, |entity| {
                        if let EntityState::Enemy(state) = &mut entity.state {
                            state.target = Some(player_id);
                            state.last_seen = now;
                            state.mode = EnemyMode::Chase;
                        }
                    });
                    output.push_event(outland_event(
                        OutlandsEvent::BossWoke,
                        Some(player_id),
                        tile,
                    ));
                }
                (Some((id, _, _)), None) => {
                    // Nobody near the lair: the boss rests (and comes back whole).
                    let near = players.iter().any(|player| {
                        self.players.get(player).is_some_and(|rig| {
                            let tile = frame.tile_at(rig.x, rig.y);
                            wrap_delta(i64::from(tile.x), middle_x, width).abs()
                                <= LAIR_HALF_WIDTH + BOSS_REST_TILES
                                && (i64::from(tile.y) - i64::from(middle_y)).abs()
                                    <= BOSS_REST_TILES
                        })
                    });
                    if !near {
                        self.entities.despawn(id);
                    }
                }
                _ => {}
            }
        }
        // The brood: a warning, then the call, on a rhythm of world time.
        let Some((_, body, state)) = self.lair_boss(&lair) else {
            return;
        };
        if !state.mode.hostile() {
            return;
        }
        let period = if state.health_fraction() < 0.5 {
            BROOD_HURT_MILLIS
        } else {
            BROOD_MILLIS
        };
        let (before, after) = (
            self.millis_at(self.simulation_tick),
            self.millis_at(next_tick),
        );
        let boss_tile = frame.tile_at(body.x, body.y);
        let warned =
            (before + BROOD_WARNING_MILLIS) / period != (after + BROOD_WARNING_MILLIS) / period;
        if warned {
            output.push_event(outland_event(
                OutlandsEvent::BossSummons,
                state.target,
                boss_tile,
            ));
        }
        if before / period == after / period {
            return;
        }
        let brood = brood_of(lair.boss.kind);
        let count = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(
                |entity| matches!(&entity.state, EntityState::Enemy(other) if other.kind == brood),
            )
            .count();
        for side in [-1_i64, 1] {
            if count + usize::from(side == 1) >= BROOD_CAP {
                break;
            }
            let tile = TilePosition {
                x: i32::try_from((middle_x + side * (LAIR_HALF_WIDTH - 6)).rem_euclid(width))
                    .unwrap_or(0),
                y: lair.floor - 2,
            };
            let point = Self::spawn_point(frame, brood, tile);
            let tier = self.tier_at(tile);
            let id = self.spawn_enemy_with(brood, point, tier, None, None);
            let now = self.simulation_tick;
            let target = state.target;
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(added) = &mut entity.state {
                    added.target = target;
                    added.last_seen = now;
                    added.mode = EnemyMode::Chase;
                }
            });
        }
    }

    /// A theme boss fell (or a Peaceful lair yielded): its seal is spent, its vault opens.
    fn boss_lair_won(
        &mut self,
        planet: &PlanetGround,
        lair: &BossLair,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let seal = TilePosition {
            x: lair.seal.0,
            y: lair.seal.1,
        };
        // The world's own rule writes over the unbreakable seal and door.
        let _ = self
            .terrain
            .set_kind_by_rule(seal, TileKind::Outland(OutlandTile::BossSealSpent));
        for (column, row) in lair.door {
            let _ = self
                .terrain
                .set_kind_by_rule(TilePosition { x: column, y: row }, TileKind::Air);
        }
        let _ = planet;
        output.push_event(outland_event(OutlandsEvent::BossDefeated, by, seal));
        // The endgame's hunt list (Wave 13 integration).
        self.record_theme_boss(lair.boss.theme.0, by, output);
    }

    /// An outland creature died at `tile`: a theme boss's fall wins its lair.
    pub(super) fn outlands_kill(
        &mut self,
        kind: EnemyKind,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        if !kind.is_outland_boss() {
            return;
        }
        let Some((planet, _)) = self.outland_world().map(|(planet, life)| (*planet, life)) else {
            return;
        };
        let Some(lair) = boss_lair(&planet) else {
            return;
        };
        if lair.boss.kind == kind {
            self.boss_lair_won(&planet, &lair, by, output);
        }
    }
}

/// The brood a theme boss calls.
#[must_use]
pub const fn brood_of(boss: EnemyKind) -> EnemyKind {
    match boss {
        EnemyKind::ReactorWarden => EnemyKind::WardDrone,
        EnemyKind::AshenSovereign => EnemyKind::Shade,
        _ => EnemyKind::Sporeling,
    }
}
