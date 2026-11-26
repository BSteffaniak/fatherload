//! The exotic worlds' creatures and theme bosses (Wave 13 planets-d): their spawner and what
//! sets them apart from the generic state machine in `world/enemies.rs`.
//!
//! An exotic planet's ground runs in its own store's ground pass, where the home world's
//! director never runs, so its creatures have a spawner of their own
//! ([`WorldState::advance_exotic_creatures`]): every second, round each rig, one of its zone's
//! roster may appear out of sight (on the locked world, the roster of the side the rig is on).
//! Everything moves in the planet's local frame through the ordinary enemy body rules
//! (`step_body`, `free_from_rock`, `keep_off_bodies`, `site_fits`, contact boxes,
//! line-of-sight noticing).
//!
//! * **Neutral fauna** never hunt: drift rays glide in wide loops and veer from rigs, gasbags
//!   drift about their patch and rise away once struck, sunbaskers and ringhoppers bolt from a
//!   rig that comes close, dusk grazers and shardbacks graze.
//! * **The frost stalker** hunts by motion: a rig that stands still (not drilling, barely
//!   moving) is lost to it.
//! * Storm eels (they crackle, then lunge), the swarms (vapour stingers, scorch mites, void
//!   mites) and tether drones (their lens glows before they fire) are generic hunters with
//!   their kind's stats.
//! * **The bosses** ([`crate::exotic::boss`]) wake at their arena, run their told patterns
//!   (`exotic_boss_fight`), and sleep again, healed, when every rig leaves.

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

use crate::{
    enemy::{EnemyKind, EnemyMode, EnemyState, Locomotion},
    entity::{Body, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    exotic::{
        ExoticEvent, ExoticStyle, ExoticTile,
        boss::{self, Arena, BossAttack},
        locked::{self, Side},
        rules::{Zone, zone_at},
    },
    ids::{PlayerId, SimulationTick},
    planet::{BodyGround, PlanetGround},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{Intent, MAX_ENEMIES, SPAWN_CHECK_MILLIS, WorldState, direction};

/// Tiles from a rig within which shy fauna bolt.
pub const SHY_TILES: f32 = 6.0;
/// The speed (pixels a second) under which a frost stalker loses a rig.
pub const STALKER_STILL_SPEED: f32 = 25.0;
/// Chance per mille each second that a creature appears round a rig (below its cap).
const SPAWN_PER_MILLE: u16 = 220;
/// Tiles from a rig a creature appears (out of its view).
const SPAWN_RING: (i32, i32) = (22, 30);
/// Tiles round a rig counted against its cap.
const LOCAL_TILES: f32 = 32.0;
/// Rows up and down a column the spawner looks for room.
const SPAWN_ROWS: i32 = 14;
const SALT_EXOTIC_SPAWN: u64 = 0xD13E_5A11;
/// Tiles round a boss's arena a rig must stay within to keep it awake.
const ARENA_LEASH_TILES: i32 = 30;

/// The creatures of `style`'s `zone` (on the locked world, `side`'s), with spawn weights.
#[must_use]
pub const fn roster(style: ExoticStyle, zone: Zone, side: Side) -> &'static [(EnemyKind, u32)] {
    use EnemyKind::{
        DriftRay, DuskGrazer, FrostStalker, Gasbag, Ringhopper, ScorchMite, Shardback, StormEel,
        Sunbasker, TetherDrone, VaporStinger, VoidMite,
    };
    match (style, zone) {
        (ExoticStyle::GasGiant, Zone::Surface) => &[(DriftRay, 5), (Gasbag, 2), (VaporStinger, 2)],
        (ExoticStyle::GasGiant, Zone::Stratum(0)) => {
            &[(DriftRay, 3), (VaporStinger, 3), (Gasbag, 2)]
        }
        (ExoticStyle::GasGiant, Zone::Stratum(1)) => &[(StormEel, 4), (Gasbag, 1)],
        (ExoticStyle::GasGiant, _) => &[(StormEel, 3)],
        (ExoticStyle::Locked, Zone::Surface) => match side {
            Side::Day => &[(Sunbasker, 4), (ScorchMite, 3)],
            Side::Twilight => &[(DuskGrazer, 5), (Sunbasker, 1)],
            Side::Night => &[(FrostStalker, 3)],
        },
        (ExoticStyle::Locked, Zone::Stratum(0 | 1)) => match side {
            Side::Day => &[(ScorchMite, 3)],
            Side::Twilight => &[(ScorchMite, 1), (FrostStalker, 1)],
            Side::Night => &[(FrostStalker, 3)],
        },
        (ExoticStyle::Locked, _) => &[(FrostStalker, 2), (ScorchMite, 2)],
        (ExoticStyle::Ring, Zone::Surface) => &[(Ringhopper, 4), (Shardback, 3), (VoidMite, 1)],
        (ExoticStyle::Ring, Zone::Skyring) => &[(Ringhopper, 3), (VoidMite, 3)],
        (ExoticStyle::Ring, Zone::Stratum(0)) => &[(VoidMite, 3)],
        (ExoticStyle::Ring, Zone::Stratum(1)) => &[(VoidMite, 3), (TetherDrone, 1)],
        (ExoticStyle::Ring, Zone::Stratum(2)) => &[(TetherDrone, 4)],
        (ExoticStyle::Ring, _) => &[(TetherDrone, 2), (VoidMite, 2)],
    }
}

/// How many of `kind` appear together.
const fn pack_size(kind: EnemyKind, roll: u64) -> usize {
    match kind {
        EnemyKind::VaporStinger | EnemyKind::ScorchMite | EnemyKind::VoidMite => {
            3 + (roll % 2) as usize
        }
        EnemyKind::DriftRay | EnemyKind::DuskGrazer => 2 + (roll % 2) as usize,
        _ => 1,
    }
}

impl WorldState {
    /// Whether `state`'s creature is at peace with the crew (auto-aim spares it): the exotic
    /// worlds' neutral fauna.
    pub(in crate::world) const fn exotic_peaceful(state: &EnemyState) -> bool {
        state.kind.is_exotic_fauna()
    }

    /// The exotic creatures' say on what they notice: `Some(None)` for the fauna and the
    /// bosses (they never hunt by the generic rules), the frost stalker's eye for motion,
    /// `None` for the generic sight.
    #[allow(
        clippy::option_option,
        reason = "the notice hooks' shape: decided (and whom), or left to sight"
    )]
    pub(in crate::world) fn exotic_notice(
        &self,
        body: &Body,
        state: &EnemyState,
        perceive: impl FnOnce() -> Option<PlayerId>,
    ) -> Option<Option<PlayerId>> {
        if !state.kind.is_exotic() {
            return None;
        }
        if state.kind.is_exotic_fauna() || state.kind.is_exotic_boss() {
            return Some(None);
        }
        if state.kind != EnemyKind::FrostStalker {
            return None;
        }
        let _ = body;
        let seen = perceive()?;
        let moving = self.players.get(&seen).is_some_and(|rig| {
            self.active_drills.contains_key(&seen)
                || rig.velocity_x.hypot(rig.velocity_y) > STALKER_STILL_SPEED
        });
        // Already on the hunt, it keeps what it has (a still rig breaks only a fresh sighting).
        Some((moving || state.mode != EnemyMode::Idle).then_some(seen))
    }

    /// The damage multiplier on `state`'s creature: a boss's open weak point.
    pub(in crate::world) fn exotic_damage_factor(state: &EnemyState) -> f32 {
        if state.kind.is_exotic_boss() && state.volley == STEP_EXPOSED {
            boss::EXPOSED_DAMAGE_FACTOR
        } else {
            1.0
        }
    }

    /// The exotic creatures' own ways (`None` leaves the generic behaviour).
    pub(super) fn exotic_behave(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Option<Intent> {
        if state.kind.is_exotic_boss() {
            return self.exotic_boss_fight(id, body, state, now, output);
        }
        let spec = state.kind.stats();
        let (home_x, home_y) = self.frame().centre_of(state.home.0, state.home.1);
        let nearest = self
            .players
            .values()
            .filter(|rig| rig.hull > 0.0)
            .map(|rig| ((rig.x, rig.y), (rig.x - body.x).hypot(rig.y - body.y)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let shy = SHY_TILES * TILE_SIZE;
        match state.kind {
            EnemyKind::Sunbasker | EnemyKind::Ringhopper => {
                let ((rx, ry), _) = nearest.filter(|(_, d)| *d < shy)?;
                let ((nx, _), _) = direction((rx, ry), (body.x, body.y));
                state.facing = if nx < 0.0 { -1 } else { 1 };
                state.mode = EnemyMode::Idle;
                Some(Intent {
                    goal: Some((nx.signum().mul_add(TILE_SIZE * 8.0, body.x), body.y)),
                    speed: spec.speed * 1.6,
                })
            }
            EnemyKind::DriftRay => {
                // Wide loops round its patch; it veers away from a rig.
                if let Some(((rx, ry), _)) = nearest.filter(|(_, d)| *d < shy) {
                    let ((nx, ny), _) = direction((rx, ry), (body.x, body.y));
                    return Some(Intent {
                        goal: Some((
                            nx.mul_add(TILE_SIZE * 6.0, body.x),
                            ny.mul_add(TILE_SIZE * 4.0, body.y),
                        )),
                        speed: spec.speed * 1.4,
                    });
                }
                Some(Intent {
                    goal: Some(self.exotic_orbit((home_x, home_y), state, now, TILE_SIZE * 6.0)),
                    speed: spec.speed,
                })
            }
            EnemyKind::Gasbag => {
                // Struck, it rises away; else it drifts about its patch.
                if state.health < state.max_health {
                    return Some(Intent {
                        goal: Some((body.x, body.y - TILE_SIZE * 6.0)),
                        speed: spec.speed * 2.0,
                    });
                }
                Some(Intent {
                    goal: Some(self.exotic_orbit((home_x, home_y), state, now, TILE_SIZE * 3.0)),
                    speed: spec.speed,
                })
            }
            _ => None,
        }
    }

    /// A point circling `center` at `radius` (eight stations, one every 700 ms).
    fn exotic_orbit(
        &self,
        center: (f32, f32),
        state: &EnemyState,
        now: SimulationTick,
        radius: f32,
    ) -> (f32, f32) {
        const STATIONS: [(f32, f32); 8] = [
            (1.0, 0.0),
            (0.7, 0.5),
            (0.0, 0.7),
            (-0.7, 0.5),
            (-1.0, 0.0),
            (-0.7, -0.5),
            (0.0, -0.7),
            (0.7, -0.5),
        ];
        let step = self.timing.ticks_for_millis(700).max(1);
        let station = (now.get() / step + u64::from(state.seed)) % 8;
        let (dx, dy) = STATIONS[usize::try_from(station).unwrap_or(0)];
        (dx.mul_add(radius, center.0), dy.mul_add(radius, center.1))
    }

    /// A creature of an exotic world died: a boss's seal opens into its trophy.
    pub(super) fn exotic_kill(
        &mut self,
        kind: EnemyKind,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        if !kind.is_exotic_boss() {
            return;
        }
        let Some((ground, style)) = self.exotic_world() else {
            return;
        };
        if style.boss() != kind {
            return;
        }
        self.open_exotic_seal(&ground, style, by, output);
    }

    /// The seal of `style`'s boss turns into its trophy (the win, kept by the terrain).
    fn open_exotic_seal(
        &mut self,
        ground: &PlanetGround,
        style: ExoticStyle,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let Some(arena) = boss::arena(ground) else {
            return;
        };
        if self.terrain.tile(arena.seal).map(|tile| tile.kind)
            != Some(TileKind::Exotic(ExoticTile::BossSeal))
        {
            return;
        }
        let _ = self
            .terrain
            .set_kind_by_rule(arena.seal, TileKind::Exotic(boss::trophy(style)));
        output.push_event(super::super::exotic::exotic_event(
            ExoticEvent::BossDefeated,
            by,
            arena.seal,
        ));
    }

    /// The boss's arena: it wakes when a rig steps in while the seal stands, sleeps (healed)
    /// when every rig has gone, and on a Peaceful world the seal opens to a rig that reaches it.
    pub(in crate::world) fn advance_exotic_boss_arena(
        &mut self,
        ground: &PlanetGround,
        style: ExoticStyle,
        output: &mut TickOutput,
    ) {
        let Some(arena) = boss::arena(ground) else {
            return;
        };
        let width = ground.strip_width();
        let sealed = self.terrain.tile(arena.seal).map(|tile| tile.kind)
            == Some(TileKind::Exotic(ExoticTile::BossSeal));
        let frame = self.frame();
        let rigs: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .filter(|(id, rig)| rig.hull > 0.0 && !self.in_space(**id))
            .map(|(id, rig)| (*id, frame.tile_at(rig.x, rig.y)))
            .collect();
        let inside = rigs
            .iter()
            .find(|(_, tile)| arena.contains(*tile, width, 0))
            .copied();
        let near = rigs
            .iter()
            .any(|(_, tile)| arena.contains(*tile, width, ARENA_LEASH_TILES));
        let kind = style.boss();
        let existing: Vec<EntityId> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(
                |entity| matches!(&entity.state, EntityState::Enemy(state) if state.kind == kind),
            )
            .map(|entity| entity.id)
            .collect();
        if !sealed || !near {
            for id in existing {
                let _ = self.entities.despawn(id);
            }
            return;
        }
        if !self.enemies.difficulty.hostile() {
            if let Some((player_id, _)) = inside {
                self.open_exotic_seal(ground, style, Some(player_id), output);
            }
            return;
        }
        if !existing.is_empty() {
            return;
        }
        let Some((player_id, _)) = inside else {
            return;
        };
        let (x, y) = Self::spawn_point(frame, kind, arena.home);
        let id = self.spawn_enemy_with(kind, (x, y), boss::BOSS_TIER, None, None);
        let now = self.simulation_tick;
        let rest = self.timing.ticks_for_millis(boss::REST_MILLIS);
        self.entities.update(id, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.phase = 1;
                state.volley = STEP_REST;
                state.pattern = 0;
                state.mode = EnemyMode::Idle;
                state.mode_until = SimulationTick::new(now.get().saturating_add(rest));
            }
        });
        output.push_event(super::super::exotic::exotic_event(
            ExoticEvent::BossAwoke,
            Some(player_id),
            arena.home,
        ));
    }

    /// One tick of a boss's fight: rest, tell an attack (its marks shown), strike, and open
    /// its weak point after its signature attack. Returns how it moves. The step lives in
    /// [`EnemyState::volley`] and the generic mode stays idle (the generic timers never turn
    /// a boss's tell into a strike of their own).
    fn exotic_boss_fight(
        &mut self,
        _id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Option<Intent> {
        let (ground, style) = self.exotic_world()?;
        let arena = boss::arena(&ground)?;
        let spec = state.kind.stats();
        let width = ground.strip_width();
        // It rages below half health: shorter tells, more marks.
        if state.phase < 2 && state.health <= state.max_health * 0.5 {
            state.phase = 2;
        }
        let rage = state.phase >= 2;
        let attack_now = boss::pattern(style)
            .get(usize::from(state.pattern) % boss::pattern(style).len())
            .copied()
            .unwrap_or(BossAttack::BoltSalvo);
        let target = self.boss_target(&arena, width, body);
        if now >= state.mode_until {
            let ticks = |world: &Self, millis: u64| {
                SimulationTick::new(
                    now.get()
                        .saturating_add(world.timing.ticks_for_millis(millis)),
                )
            };
            match state.volley {
                STEP_REST => {
                    let Some(target) = target else {
                        state.mode_until = ticks(self, 500);
                        return Some(self.boss_idle_intent(&arena, body, state, now, spec.speed));
                    };
                    state.waypoint = Some((target.x, target.y));
                    state.volley = STEP_TELL;
                    let tell = if rage {
                        boss::RAGE_TELL_MILLIS
                    } else {
                        boss::TELL_MILLIS
                    };
                    state.mode_until = ticks(self, tell);
                    output.push_event(super::super::exotic::exotic_event(
                        ExoticEvent::BossTell,
                        None,
                        target,
                    ));
                }
                STEP_TELL => {
                    let told = state
                        .waypoint
                        .map_or(arena.home, |(x, y)| TilePosition { x, y });
                    self.boss_strike(&arena, attack_now, told, rage, body, output);
                    state.volley = STEP_STRIKE;
                    let length = if attack_now == BossAttack::Gale {
                        boss::GALE_MILLIS
                    } else {
                        boss::STRIKE_MILLIS
                    };
                    state.mode_until = ticks(self, length);
                }
                STEP_STRIKE => {
                    if attack_now.opens() {
                        state.volley = STEP_EXPOSED;
                        state.mode_until = ticks(self, boss::EXPOSED_MILLIS);
                    } else {
                        state.volley = STEP_REST;
                        state.mode = EnemyMode::Idle;
                        state.pattern = state.pattern.wrapping_add(1);
                        state.mode_until = ticks(self, boss::REST_MILLIS);
                    }
                }
                _ => {
                    state.volley = STEP_REST;
                    state.mode = EnemyMode::Idle;
                    state.pattern = state.pattern.wrapping_add(1);
                    state.mode_until = ticks(self, boss::REST_MILLIS);
                }
            }
        }
        // The gale blows while it strikes.
        if state.volley == STEP_STRIKE && attack_now == BossAttack::Gale {
            self.boss_gale(body);
        }
        // How it moves.
        let told = state.waypoint.map(|(x, y)| self.frame().centre_of(x, y));
        Some(match (state.volley, attack_now) {
            (STEP_STRIKE, BossAttack::Dive) => Intent {
                goal: told,
                speed: spec.dash_speed,
            },
            (STEP_TELL | STEP_STRIKE | STEP_EXPOSED, _) => Intent::default(),
            _ => self.boss_idle_intent(&arena, body, state, now, spec.speed),
        })
    }

    /// Where a resting boss goes: the Thunderhead and the Warden circle their home, the
    /// Gloamwarden paces toward the nearest rig along its arena.
    fn boss_idle_intent(
        &self,
        arena: &Arena,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        speed: f32,
    ) -> Intent {
        let (hx, hy) = self.frame().centre_of(arena.home.x, arena.home.y);
        if state.kind == EnemyKind::Gloamwarden {
            let nearest = self
                .players
                .values()
                .filter(|rig| rig.hull > 0.0)
                .map(|rig| rig.x)
                .min_by(|a, b| (a - body.x).abs().total_cmp(&(b - body.x).abs()));
            let reach = (arena.half_width - 6) as f32 * TILE_SIZE;
            let (cx, _) = self.frame().centre_of(arena.centre, arena.floor);
            let goal_x = nearest.map_or(hx, |x| x.clamp(cx - reach, cx + reach));
            state.facing = if goal_x < body.x { -1 } else { 1 };
            return Intent {
                goal: Some((goal_x, body.y)),
                speed,
            };
        }
        Intent {
            goal: Some(self.exotic_orbit((hx, hy), state, now, TILE_SIZE * 4.0)),
            speed,
        }
    }

    /// The rig a boss aims at: the nearest live rig in its arena.
    fn boss_target(&self, arena: &Arena, width: i32, body: &Body) -> Option<TilePosition> {
        let frame = self.frame();
        self.players
            .iter()
            .filter(|(id, rig)| rig.hull > 0.0 && !self.in_space(**id))
            .filter(|(_, rig)| arena.contains(frame.tile_at(rig.x, rig.y), width, 4))
            .map(|(_, rig)| (rig, (rig.x - body.x).hypot(rig.y - body.y)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(rig, _)| frame.tile_at(rig.x, rig.y))
    }

    /// The told attack lands: rigs under its marks are struck; a drone call brings drones.
    fn boss_strike(
        &mut self,
        arena: &Arena,
        attack: BossAttack,
        told: TilePosition,
        rage: bool,
        body: &Body,
        output: &mut TickOutput,
    ) {
        let width = self
            .terrain
            .planet_ground()
            .map_or(1, crate::planet::PlanetGround::strip_width);
        if attack == BossAttack::DroneCall {
            let drones = self
                .entities
                .of_kind(EntityKind::Enemy)
                .filter(|entity| {
                    matches!(&entity.state, EntityState::Enemy(state) if state.kind == EnemyKind::TetherDrone)
                })
                .filter(|entity| {
                    arena.contains(
                        self.frame().tile_at(entity.body.x, entity.body.y),
                        width,
                        8,
                    )
                })
                .count();
            for side in [-1.0_f32, 1.0] {
                if drones + usize::from(side > 0.0) >= boss::MAX_DRONES {
                    break;
                }
                let at = (
                    side.mul_add(TILE_SIZE * 3.0, body.x),
                    body.y - TILE_SIZE * 2.0,
                );
                let _ =
                    self.spawn_enemy_with(EnemyKind::TetherDrone, at, boss::BOSS_TIER, None, None);
            }
            return;
        }
        let damage = match attack {
            BossAttack::BoltSalvo => boss::BOLT_DAMAGE,
            BossAttack::SunSlam => boss::SLAM_DAMAGE,
            BossAttack::FrostShards => boss::SHARD_DAMAGE,
            BossAttack::BeamSweep => boss::BEAM_DAMAGE,
            _ => return,
        } * crate::enemy::tier_scale(boss::BOSS_TIER);
        let marks = boss::marks(arena, attack, told, rage);
        let frame = self.frame();
        let struck: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(id, _)| !self.in_space(**id))
            .filter(|(_, rig)| {
                let tile = frame.tile_at(rig.x, rig.y);
                marks.iter().any(|mark| boss::mark_hits(mark, tile, width))
            })
            .map(|(id, _)| *id)
            .collect();
        for player_id in struck {
            if self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
    }

    /// The Thunderhead's gale: rigs in the arena are blown away from it.
    fn boss_gale(&mut self, body: &Body) {
        let dt = self.timing.delta_seconds;
        let ids: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(id, rig)| {
                !self.in_space(**id) && (rig.x - body.x).hypot(rig.y - body.y) < TILE_SIZE * 30.0
            })
            .map(|(id, _)| *id)
            .collect();
        for player_id in ids {
            if let Some(rig) = self.players.get_mut(&player_id) {
                let away = if rig.x < body.x { -1.0 } else { 1.0 };
                let target = away * boss::GALE_PUSH;
                if (target > 0.0 && rig.velocity_x < target)
                    || (target < 0.0 && rig.velocity_x > target)
                {
                    rig.velocity_x = quantize((target * 3.0).mul_add(dt, rig.velocity_x));
                }
            }
        }
    }

    /// The exotic worlds' spawner (see the module docs): every [`SPAWN_CHECK_MILLIS`], a
    /// creature of each rig's zone may appear out of its sight. Nothing on a Peaceful world.
    pub(in crate::world) fn advance_exotic_creatures(
        &mut self,
        ground: &PlanetGround,
        style: ExoticStyle,
        next_tick: SimulationTick,
    ) {
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let period = self.timing.ticks_for_millis(SPAWN_CHECK_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(period) {
            return;
        }
        if self.in_ground_pass() {
            self.despawn_far_enemies();
        }
        let rigs = self.rig_tiles();
        let seed = ground.params().seed;
        let width = ground.strip_width();
        let arena = boss::arena(ground);
        for (index, (player_id, tile)) in rigs.iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                break;
            }
            if self.in_space(*player_id) {
                continue;
            }
            // A boss's arena holds only its fight.
            if arena.is_some_and(|arena| arena.contains(*tile, width, 12)) {
                continue;
            }
            let zone = zone_at(ground, style, *tile);
            let side = locked::side_of(ground, i64::from(tile.x));
            let (cx, cy) = self.frame().centre_of(tile.x, tile.y);
            let local = self.enemies_near(cx, cy, LOCAL_TILES * TILE_SIZE).len();
            let cap = if matches!(zone, Zone::Surface | Zone::Skyring) {
                3
            } else {
                4
            } + self.enemies.difficulty.extra_cap();
            if local >= cap {
                continue;
            }
            let roll = hash2(
                seed ^ next_tick.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_EXOTIC_SPAWN,
                i32::try_from(player_id.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(index).unwrap_or(0),
            );
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a per-mille chance"
            )]
            let chance = (f32::from(SPAWN_PER_MILLE) * self.enemies.difficulty.density()) as u64;
            if roll % 1_000 >= chance {
                continue;
            }
            let list = roster(style, zone, side);
            let total: u32 = list.iter().map(|(_, weight)| *weight).sum();
            if total == 0 {
                continue;
            }
            let mut pick = u32::try_from((roll >> 12) % u64::from(total)).unwrap_or(0);
            let mut kind = list[0].0;
            for (candidate, weight) in list {
                if pick < *weight {
                    kind = *candidate;
                    break;
                }
                pick -= weight;
            }
            let Some(site) = self.exotic_site(ground, kind, zone, *tile, roll, &rigs) else {
                continue;
            };
            if !kind.is_exotic_fauna() && self.enemy_safe_zone(site) {
                continue;
            }
            let tier = self.tier_at(site);
            for offset in 0..pack_size(kind, roll >> 28) {
                let at = TilePosition {
                    x: site.x + i32::try_from(offset).unwrap_or(0) * 2,
                    y: site.y,
                };
                if offset > 0 && !self.site_fits(kind, at) {
                    continue;
                }
                let point = Self::spawn_point(self.frame(), kind, at);
                let _ = self.spawn_enemy_with(kind, point, tier, None, None);
            }
        }
    }

    /// A spot for a `kind` round the rig at `tile`, out of every rig's sight: walkers on the
    /// first ground under the spot, fliers in the open air round its row.
    fn exotic_site(
        &self,
        ground: &PlanetGround,
        kind: EnemyKind,
        zone: Zone,
        tile: TilePosition,
        roll: u64,
        rigs: &[(PlayerId, TilePosition)],
    ) -> Option<TilePosition> {
        let flying = matches!(kind.stats().locomotion, Locomotion::Flying);
        for attempt in 0..8_u64 {
            let hash = hash2(
                roll,
                SALT_EXOTIC_SPAWN + 1,
                i32::try_from(attempt).unwrap_or(0),
                0,
            );
            let side = if hash & 1 == 0 { 1 } else { -1 };
            let span = SPAWN_RING.1 - SPAWN_RING.0;
            let distance = SPAWN_RING.0
                + i32::try_from((hash >> 4) % u64::try_from(span).unwrap_or(1)).unwrap_or(0);
            let x = tile.x + side * distance;
            let candidate = if !flying && zone == Zone::Surface {
                // Down from the sky to the first ground.
                let top = ground.surface_row(x) - 24;
                let Some(floor) = (top..top + 60).find(|row| {
                    self.terrain
                        .tile(TilePosition { x, y: *row })
                        .is_some_and(|t| !t.kind.is_open())
                }) else {
                    continue;
                };
                TilePosition { x, y: floor - 1 }
            } else {
                let found = (0..=SPAWN_ROWS).flat_map(|dy| [dy, -dy]).find_map(|dy| {
                    let at = TilePosition { x, y: tile.y + dy };
                    (Self::out_of_view(at, rigs) && self.site_fits(kind, at)).then_some(at)
                });
                match found {
                    Some(at) => at,
                    None => continue,
                }
            };
            if !Self::out_of_view(candidate, rigs) {
                continue;
            }
            if self.site_fits(kind, candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

/// A boss's fight steps (kept in [`EnemyState::volley`]).
pub const STEP_REST: u8 = 0;
pub const STEP_TELL: u8 = 1;
pub const STEP_STRIKE: u8 = 2;
pub const STEP_EXPOSED: u8 = 3;

/// What the renderer and the HUD show of a boss's fight.
#[derive(Clone, Debug, PartialEq)]
pub struct BossView {
    pub kind: EnemyKind,
    pub health_fraction: f32,
    /// The attack it is telling or striking with, and where it lands.
    pub attack: Option<BossAttack>,
    pub telling: bool,
    pub marks: Vec<boss::BossMark>,
    /// Its weak point is open.
    pub exposed: bool,
    pub raging: bool,
}

impl WorldState {
    /// The fight of this exotic world's boss, if it is awake.
    #[must_use]
    pub fn exotic_boss_view(&self) -> Option<BossView> {
        let (ground, style) = self.exotic_world()?;
        let arena = boss::arena(&ground)?;
        let state = self
            .entities
            .of_kind(EntityKind::Enemy)
            .find_map(|entity| match &entity.state {
                EntityState::Enemy(state) if state.kind == style.boss() => Some(*state),
                _ => None,
            })?;
        let pattern = boss::pattern(style);
        let attack = pattern
            .get(usize::from(state.pattern) % pattern.len())
            .copied();
        let active = matches!(state.volley, STEP_TELL | STEP_STRIKE);
        let marks = match (active, attack, state.waypoint) {
            (true, Some(attack), Some((x, y))) => {
                boss::marks(&arena, attack, TilePosition { x, y }, state.phase >= 2)
            }
            _ => Vec::new(),
        };
        Some(BossView {
            kind: state.kind,
            health_fraction: state.health_fraction(),
            attack: active.then_some(attack).flatten(),
            telling: state.volley == STEP_TELL,
            marks,
            exposed: state.volley == STEP_EXPOSED,
            raging: state.phase >= 2,
        })
    }
}
