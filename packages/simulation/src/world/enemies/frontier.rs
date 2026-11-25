//! The frontier worlds' creatures (Wave 12 planets-b): their spawner and what sets them apart
//! from the generic state machine in `world/enemies.rs`.
//!
//! A frontier planet's ground runs in its own store's ground pass (F11), where the home
//! world's enemy director never runs, so these creatures have a spawner of their own
//! ([`WorldState::advance_frontier_creatures`]): every second, round each rig, one of its zone's
//! roster ([`crate::frontier::roster`]) may appear out of sight, on the surface or in a
//! stratum. Everything moves in the planet's local frame (the strip: down is toward the core)
//! through the ordinary enemy body rules (`step_body`, `free_from_rock`, `keep_off_bodies`,
//! `site_fits`, contact boxes, line-of-sight noticing).
//!
//! * **Neutral fauna** (dune skippers, ember striders, basalt tortoises, mirage and glint
//!   moths, chime drifters) never hunt: skippers and striders bolt from a rig that comes
//!   close, a tortoise shells up where it stands, mirage moths circle their oasis, glint
//!   moths circle a rig's lamps, and a **chime drifter** struck by a shot rings: every
//!   quarter of its health lost shatters the crystal round it ([`DRIFTER_RING_TILES`]), a
//!   living tool for the crystal world's diggers.
//! * **The dune wyrm** swims the dune world's sand ([`wyrm_ground`]) and hears rather than
//!   sees: a rig drilling or driving within its hearing, three times as far in the Worm
//!   Galleries ([`crate::frontier::StratumRule::EchoingGalleries`]); it ripples the sand for
//!   its wind-up and erupts (the generic burrower's lunge).
//! * The rest (glass scorpions, shard crabs, magma slugs, cinder bats, geode mimics) are
//!   generic hunters with the stats and resistances of their kind.

use crate::{
    enemy::{EnemyKind, EnemyMode, EnemyState, Locomotion},
    entity::{Body, EntityId, EntityKind},
    event::{GameplayEventKind, TickOutput},
    frontier::{
        FrontierEvent, FrontierStyle, FrontierTile, StratumRule, roster,
        rules::{Zone, zone_at},
        settlement_site, style_of_ground,
    },
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{Intent, MAX_ENEMIES, SPAWN_CHECK_MILLIS, WorldState, direction};

/// Tiles from a rig within which neutral fauna run (or shell up).
pub const FAUNA_SHY_TILES: f32 = 6.0;
/// Tiles round a struck chime drifter whose crystal its ring shatters.
pub const DRIFTER_RING_TILES: i32 = 3;
/// How much farther a dune wyrm hears in the Worm Galleries.
pub const GALLERY_HEARING: f32 = 3.0;
/// Speed (pixels per second) at which a rig's tracks on the sand reach a wyrm.
const WYRM_FEELS_SPEED: f32 = 40.0;
/// Rows up and down a column the spawner looks for an underground spot.
const SPAWN_ROWS: i32 = 12;
/// Columns either side of a settlement site its safety reaches.
const SETTLEMENT_MARGIN: i32 = 16;
/// Rows under a settlement site's ground its safety reaches.
const SETTLEMENT_SAFE_ROWS: i32 = 12;
/// Chance per mille each second that a creature appears round a rig (below its cap).
const SPAWN_PER_MILLE: u16 = 400;
/// Tiles from a rig a creature appears (out of its view).
const SPAWN_RING: (i32, i32) = (22, 30);
/// Tiles round a rig counted against its cap.
const LOCAL_TILES: f32 = 32.0;
const SALT_FRONTIER_SPAWN: u64 = 0xF207_0001;

/// Ground a dune wyrm swims through: the dune world's sands, sandstone, wormstone and salt
/// (and the plain soft rock any burrower digs).
#[must_use]
pub const fn wyrm_ground(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Frontier(
            FrontierTile::DuneSand
                | FrontierTile::PackedSand
                | FrontierTile::BandedSandstone
                | FrontierTile::Wormstone
                | FrontierTile::SaltBed
        ) | TileKind::Dirt
            | TileKind::Clay
            | TileKind::Stone
    )
}

impl WorldState {
    /// The frontier creatures' say on what they notice: `Some(None)` for the fauna (they never
    /// hunt), the dune wyrm's hearing, `None` for the generic sight.
    #[allow(
        clippy::option_option,
        reason = "the notice hooks' shape: decided (and whom), or left to sight"
    )]
    pub(in crate::world) fn frontier_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        if !state.kind.is_frontier() {
            return None;
        }
        if state.kind.is_frontier_fauna() {
            return Some(None);
        }
        if state.kind != EnemyKind::DuneWyrm {
            return None;
        }
        let ground = self.terrain.planet_ground();
        let spec = state.kind.stats();
        let frame = self.frame();
        let heard = self
            .players
            .iter()
            .filter(|(_, rig)| rig.hull > 0.0)
            .filter(|(id, rig)| {
                // A drill's bite or tracks moving over the sand.
                self.active_drills.contains_key(id)
                    || rig.velocity_x.hypot(rig.velocity_y) > WYRM_FEELS_SPEED
            })
            .filter_map(|(id, rig)| {
                let echoing = ground.is_some_and(|ground| {
                    style_of_ground(ground) == Some(FrontierStyle::Desert)
                        && crate::frontier::rules::rule_at(
                            ground,
                            FrontierStyle::Desert,
                            frame.tile_at(rig.x, rig.y),
                        ) == Some(StratumRule::EchoingGalleries)
                });
                let reach =
                    spec.hearing_tiles * TILE_SIZE * if echoing { GALLERY_HEARING } else { 1.0 };
                let distance = (rig.x - body.x).hypot(rig.y - body.y);
                (distance <= reach).then_some((distance, *id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id);
        Some(heard)
    }

    /// Whether `tile` of a frontier world is safe from its hunters (`None` on any other
    /// ground): its surface is no refuge, only its settlement (the aliens' town, TODO-SEAM
    /// w12-aliens), claimed bases and placed infrastructure are.
    pub(in crate::world) fn frontier_safe_zone(&self, tile: TilePosition) -> Option<bool> {
        let ground = self.terrain.planet_ground()?;
        style_of_ground(ground)?;
        let settlement = settlement_site(ground).is_some_and(|site| {
            let width = ground.strip_width();
            let dx = (tile.x - site.column).rem_euclid(width);
            (dx < site.width + SETTLEMENT_MARGIN || dx > width - SETTLEMENT_MARGIN)
                && tile.y < site.surface_row + SETTLEMENT_SAFE_ROWS
        });
        let near_infrastructure = self.infrastructure.iter().any(|item| {
            (item.position.x - tile.x).abs() <= super::BASE_SAFE_TILES
                && (item.position.y - tile.y).abs() <= super::BASE_SAFE_TILES
        });
        Some(
            settlement
                || near_infrastructure
                || self.bases.is_safe_zone(tile)
                || self.homes.is_safe_zone(tile),
        )
    }

    /// The frontier creatures' say on where `kind` can appear: `Some(fits)` decides, `None`
    /// leaves it to the generic habitat.
    pub(in crate::world) fn frontier_habitat(
        &self,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> Option<bool> {
        if !kind.is_frontier() {
            return None;
        }
        let at = |dx: i32, dy: i32| {
            self.terrain
                .tile(TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                })
                .map(|t| t.kind)
        };
        let air = |dx: i32, dy: i32| at(dx, dy) == Some(TileKind::Air);
        match kind {
            // In the sand, or in an open pocket against it.
            EnemyKind::DuneWyrm => Some(
                at(0, 0).is_some_and(wyrm_ground)
                    || (air(0, 0)
                        && [(0, -1), (1, 0), (0, 1), (-1, 0)]
                            .into_iter()
                            .any(|(dx, dy)| at(dx, dy).is_some_and(wyrm_ground))),
            ),
            // On a lava shore: it walks the lava itself.
            EnemyKind::EmberStrider => Some(
                air(0, 0)
                    && air(0, -1)
                    && at(0, 1).is_some_and(|below| !below.is_open())
                    && (-6..=6).any(|dx| (1..=3).any(|dy| at(dx, dy) == Some(TileKind::Lava))),
            ),
            // Over an oasis's water.
            EnemyKind::MirageMoth => {
                Some(air(0, 0) && (1..=6).any(|dy| matches!(at(0, dy), Some(TileKind::Fluid(_)))))
            }
            _ => None,
        }
    }

    /// The frontier creatures' own ways (`None` leaves the generic behaviour).
    pub(super) fn frontier_behave(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Option<Intent> {
        let spec = state.kind.stats();
        let (home_x, home_y) = self.frame().centre_of(state.home.0, state.home.1);
        let nearest = self
            .players
            .values()
            .filter(|rig| rig.hull > 0.0)
            .map(|rig| ((rig.x, rig.y), (rig.x - body.x).hypot(rig.y - body.y)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let shy = FAUNA_SHY_TILES * TILE_SIZE;
        match state.kind {
            EnemyKind::DuneSkipper | EnemyKind::EmberStrider => {
                let ((rx, ry), distance) = nearest.filter(|(_, d)| *d < shy)?;
                // Bolt away from it, along the ground.
                let ((nx, _), _) = direction((rx, ry), (body.x, body.y));
                state.facing = if nx < 0.0 { -1 } else { 1 };
                state.mode = EnemyMode::Idle;
                let _ = distance;
                Some(Intent {
                    goal: Some((nx.signum().mul_add(TILE_SIZE * 8.0, body.x), body.y)),
                    speed: spec.speed * 1.5,
                })
            }
            EnemyKind::BasaltTortoise => {
                // Shells up where it stands while a rig is close.
                nearest
                    .filter(|(_, d)| *d < TILE_SIZE * 3.0)
                    .map(|_| Intent::default())
            }
            EnemyKind::MirageMoth => Some(Intent {
                goal: Some(self.orbit((home_x, home_y), state, now, TILE_SIZE * 3.0)),
                speed: spec.speed * 0.6,
            }),
            EnemyKind::GlintMoth => {
                // Drawn to lamps: circles the nearest rig's light, harmless.
                let ((rx, ry), _) = nearest.filter(|(_, d)| *d < spec.sight_tiles * TILE_SIZE)?;
                Some(Intent {
                    goal: Some(self.orbit((rx, ry), state, now, TILE_SIZE * 2.5)),
                    speed: spec.speed,
                })
            }
            EnemyKind::ChimeDrifter => {
                // Every quarter of its health lost, it rings.
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "0..=4"
                )]
                let rings_due = ((1.0 - state.health_fraction()) * 4.0)
                    .floor()
                    .clamp(0.0, 4.0) as u8;
                if rings_due > state.phase {
                    state.phase = rings_due;
                    self.drifter_rings(id, body, output);
                }
                // A slow bell drifting about its home.
                Some(Intent {
                    goal: Some(self.orbit((home_x, home_y), state, now, TILE_SIZE * 2.0)),
                    speed: spec.speed,
                })
            }
            _ => None,
        }
    }

    /// A struck chime drifter's ring: the crystal within [`DRIFTER_RING_TILES`] shatters.
    fn drifter_rings(&mut self, id: EntityId, body: &Body, output: &mut TickOutput) {
        let centre = self.frame().tile_at(body.x, body.y);
        let mut shattered = 0;
        for dy in -DRIFTER_RING_TILES..=DRIFTER_RING_TILES {
            for dx in -DRIFTER_RING_TILES..=DRIFTER_RING_TILES {
                if dx * dx + dy * dy > DRIFTER_RING_TILES * DRIFTER_RING_TILES {
                    continue;
                }
                let at = TilePosition {
                    x: centre.x + dx,
                    y: centre.y + dy,
                };
                if self.terrain.tile(at).is_some_and(
                    |tile| matches!(tile.kind, TileKind::Frontier(found) if found.shatters()),
                ) && self.terrain.set_kind(at, TileKind::Air)
                {
                    shattered += 1;
                }
            }
        }
        let _ = (id, shattered);
        output.push_event(GameplayEventKind::Frontier {
            what: FrontierEvent::DrifterChimed,
            player_id: None,
            x: centre.x,
            y: centre.y,
        });
    }

    /// The frontier worlds' spawner (see the module docs): every [`SPAWN_CHECK_MILLIS`], a
    /// creature of each rig's zone may appear out of its sight. Does nothing off a frontier
    /// world and on a Peaceful one.
    pub(in crate::world) fn advance_frontier_creatures(&mut self, next_tick: SimulationTick) {
        let Some(ground) = self.terrain.planet_ground().copied() else {
            return;
        };
        let Some(style) = style_of_ground(&ground) else {
            return;
        };
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let period = self.timing.ticks_for_millis(SPAWN_CHECK_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(period) {
            return;
        }
        // A body's ground pass has no director to tidy up after it.
        if self.in_ground_pass() {
            self.despawn_far_enemies();
        }
        let rigs = self.rig_tiles();
        let seed = ground.params().seed;
        for (index, (player_id, tile)) in rigs.iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                break;
            }
            if self.in_space(*player_id) {
                continue;
            }
            let zone = zone_at(&ground, *tile);
            let (cx, cy) = self.frame().centre_of(tile.x, tile.y);
            let local = self.enemies_near(cx, cy, LOCAL_TILES * TILE_SIZE).len();
            let cap =
                if zone == Zone::Surface { 3 } else { 4 } + self.enemies.difficulty.extra_cap();
            if local >= cap {
                continue;
            }
            let roll = hash2(
                seed ^ next_tick.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_FRONTIER_SPAWN,
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
            let list = roster(style, zone);
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
            let Some(site) = self.frontier_site(&ground, kind, zone, *tile, roll, &rigs) else {
                continue;
            };
            // No hunter appears where it may not hunt (the settlement, bases).
            if !kind.is_frontier_fauna() && self.enemy_safe_zone(site) {
                continue;
            }
            let tier = self.tier_at(site);
            let count = if kind == EnemyKind::CinderBat {
                3 + usize::try_from((roll >> 28) % 2).unwrap_or(0)
            } else {
                1
            };
            for offset in 0..count {
                let at = TilePosition {
                    x: site.x + i32::try_from(offset).unwrap_or(0),
                    y: site.y,
                };
                let point = Self::spawn_point(self.frame(), kind, at);
                let _ = self.spawn_enemy_with(kind, point, tier, None, None);
            }
        }
    }

    /// A spot for a `kind` round the rig at `tile`, out of every rig's sight: on (or over)
    /// the ground for a surface zone, in the rock or open ground round it below.
    fn frontier_site(
        &self,
        ground: &crate::planet::PlanetGround,
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
                SALT_FRONTIER_SPAWN + 1,
                i32::try_from(attempt).unwrap_or(0),
                0,
            );
            let side = if hash & 1 == 0 { 1 } else { -1 };
            let span = SPAWN_RING.1 - SPAWN_RING.0;
            let distance = SPAWN_RING.0
                + i32::try_from((hash >> 4) % u64::try_from(span).unwrap_or(1)).unwrap_or(0);
            let x = tile.x + side * distance;
            let candidate = if zone == Zone::Surface && kind != EnemyKind::DuneWyrm {
                // Down from the sky to the first ground (the generated surface, or whatever
                // has been dug or built there since).
                let top = crate::planet::BodyGround::surface_row(ground, x) - 24;
                let floor = (top..top + 60).find(|row| {
                    self.terrain
                        .tile(TilePosition { x, y: *row })
                        .is_some_and(|t| !t.kind.is_open())
                })?;
                TilePosition {
                    x,
                    y: floor - if flying { 4 } else { 1 },
                }
            } else {
                // The nearest open spot up or down the column that suits it (a cave, a
                // tunnel, a tube; a burrower's own sand).
                let _ = hash >> 16;
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
