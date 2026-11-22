//! Night-time gameplay in the world (WP-NIGHT, rules and data in [`crate::night`]).
//!
//! # Tick
//!
//! [`WorldState::advance_night`] runs once per tick right after the enemy director (so night
//! monsters share the enemy world cap, the far-away despawn and Peaceful's clean sweep):
//!
//! * **Night falls and lifts.** When monsters begin to rise (late dusk) the night under way is
//!   recorded in the director's [`NightLedger`] and `NightFell` fires; when dawn comes,
//!   `NightSurvived` fires with the night's kills and every night monster burrows or flees
//!   ([`crate::night::RETREAT_MILLIS`], staggered), leaving the world with
//!   `NightMonsterBurrowed`.
//! * **Rising** (each second): round every exposed actor (a rig, or a pilot on foot, out on
//!   the open surface and outside every safe place) a monster of the biome's roster may rise
//!   out of view, under a local cap; the Moon sets the odds and the cap. On a new-moon night
//!   the umbral alpha walks once, with its pack (`RareNightSpawn`).
//! * **Prowling** (each second): idle night monsters scent the nearest exposed actor and prowl
//!   toward it; light-shunning kinds stop at the edge of a lamp's pool or a palisade (so a lit
//!   base sees eyes in the dark beyond its fence), moths make for the nearest lamp.
//! * **Nocturnal ore** (each second, at night): shallow glowing ore near an awake actor on or
//!   near the surface reveals itself on that player's map, in safe places and on Peaceful
//!   worlds too (it is the night's gift, not its danger).
//!
//! # Safety
//!
//! [`WorldState::night_safe_zone`] is where night monsters neither go nor strike: below
//! [`crate::night::NIGHT_SURFACE_DEPTH`] rows under the ground or high in the sky, the town,
//! villages and halls, around placed infrastructure, wanderers' camps, homes (claims holding a
//! bed or a homestead), spots fenced by palisades within one claim, and fixture light of at
//! least [`crate::night::NIGHT_SAFE_LIGHT`]. [`WorldState::hostile_safe_zone`] picks the right
//! rule for an attacker: night monsters use this one, everything else
//! [`WorldState::enemy_safe_zone`].

use std::collections::BTreeSet;

use crate::{
    base::BuiltTile,
    clock::WorldClock,
    enemy::{EnemyKind, EnemyMode, hazard_density},
    entity::{EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::ExplosiveKind,
    ids::{PlayerId, SimulationTick},
    night::{
        self, CAMP_SAFE_TILES, CAMPFIRE_LIGHT, CAMPFIRE_LIGHT_TILES, LightResponse,
        MAX_NIGHT_MONSTERS, MOTH_LURE_TILES, NIGHT_LOCAL_CAP, NIGHT_LOCAL_TILES, NIGHT_SAFE_LIGHT,
        NIGHT_SKY_ROWS, NIGHT_SPAWN_COLUMNS, NIGHT_SPAWN_PER_MILLE, NIGHT_SURFACE_DEPTH,
        NOCTURNAL_GLOW_DEPTH, NOCTURNAL_ORE_LIGHT, NOCTURNAL_ORE_LIGHT_TILES,
        NOCTURNAL_REVEAL_TILES, NightLedger, PALISADE_REACH, PALISADE_ROWS, RARE_SPAWN_PER_MILLE,
        RETREAT_MILLIS, RETREAT_STAGGER_MILLIS, SCENT_TILES, WISP_LIGHT, WISP_LIGHT_TILES,
    },
    surface::town_span,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::{
    WorldState,
    clock::falloff,
    enemies::{BASE_SAFE_TILES, MAX_ENEMIES, SPAWN_CHECK_MILLIS, VILLAGE_SAFE_TILES},
};

/// Columns either side of the town that stay safe at night.
const TOWN_NIGHT_MARGIN: i32 = 12;
/// Spawns stay out of every actor's view: at least this many columns or rows away.
const VIEW_HALF_TILES: (i32, i32) = (21, 12);
/// Salts for the night's hashes.
const SALT_NIGHT_SPAWN: u64 = 0x416E_0001;
const SALT_NIGHT_SITE: u64 = 0x416E_0002;
const SALT_RARE: u64 = 0x416E_0003;
const SALT_PROWL: u64 = 0x416E_0004;

impl WorldState {
    /// The world's night facts (WP-NIGHT): the night under way, its kills, kills by kind,
    /// nights survived and the last rare night. Story arcs (T26) read their night hooks here.
    #[must_use]
    pub const fn night_ledger(&self) -> &NightLedger {
        &self.enemies.night
    }

    /// Whether night monsters are rising now (late dusk through the night; see
    /// [`night::monsters_rise`]). The night market keeps the same hours.
    #[must_use]
    pub const fn night_is_dangerous(&self) -> bool {
        night::monsters_rise(self.clock())
    }

    /// Every live night monster with its state, in id order.
    pub fn night_monsters(
        &self,
    ) -> impl Iterator<Item = (&crate::entity::Entity, &crate::enemy::EnemyState)> + '_ {
        self.enemies()
            .filter(|(_, state)| night::is_nocturnal(state.kind))
    }

    /// The safe-zone rule for an attacker of kind `attacker`: night monsters use
    /// [`Self::night_safe_zone`], everything else (and no attacker) uses
    /// [`Self::enemy_safe_zone`].
    #[must_use]
    pub fn hostile_safe_zone(&self, attacker: Option<EnemyKind>, tile: TilePosition) -> bool {
        match attacker {
            Some(kind) if night::is_nocturnal(kind) => self.night_safe_zone(tile),
            _ => self.enemy_safe_zone(tile),
        }
    }

    /// Whether night monsters leave `tile` alone (see the module docs): off the open surface,
    /// the town, villages, infrastructure, camps, bed claims, contractor-built home plots (T24),
    /// palisade-fenced claims and lit spots.
    #[must_use]
    pub fn night_safe_zone(&self, tile: TilePosition) -> bool {
        let surface = self.terrain.surface_row(tile.x);
        let depth = tile.y - surface;
        if !(-NIGHT_SKY_ROWS..=NIGHT_SURFACE_DEPTH).contains(&depth) {
            return true;
        }
        let (first, last) = town_span();
        if tile.x >= first - TOWN_NIGHT_MARGIN && tile.x <= last + TOWN_NIGHT_MARGIN {
            return true;
        }
        if self.infrastructure.iter().any(|item| {
            (item.position.x - tile.x).abs() <= BASE_SAFE_TILES
                && (item.position.y - tile.y).abs() <= BASE_SAFE_TILES
        }) {
            return true;
        }
        let settings = self.terrain.settings();
        if crate::worldgen::streamed::in_village_area(settings, tile, VILLAGE_SAFE_TILES)
            || self.near_camp(tile)
            || self.in_home_claim(tile)
            || self.homes.is_safe_zone(tile)
            || self.palisade_guarded(tile)
        {
            return true;
        }
        self.fixture_light_at(tile.x, tile.y) >= NIGHT_SAFE_LIGHT
    }

    /// Whether `tile` is by a wanderers' camp (their campfire keeps the night off).
    fn near_camp(&self, tile: TilePosition) -> bool {
        crate::people::camps::camps_between(
            self.terrain.settings(),
            tile.x - CAMP_SAFE_TILES,
            tile.x + CAMP_SAFE_TILES,
        )
        .iter()
        .any(|camp| (tile.y - camp.ground_row).abs() <= CAMP_SAFE_TILES)
    }

    /// The light of a wanderers' campfire at `at` (permille).
    pub(super) fn campfire_light_at(&self, at: TilePosition) -> u16 {
        crate::people::camps::camps_between(
            self.terrain.settings(),
            at.x - CAMPFIRE_LIGHT_TILES,
            at.x + CAMPFIRE_LIGHT_TILES,
        )
        .iter()
        .map(|camp| {
            falloff(
                TilePosition {
                    x: camp.column,
                    y: camp.ground_row - 1,
                },
                at,
                CAMPFIRE_LIGHT_TILES,
                CAMPFIRE_LIGHT,
            )
        })
        .max()
        .unwrap_or(0)
    }

    /// Whether `tile` lies in a home: a claim holding a bed or a homestead (WP-HOME's homes are
    /// claims too, so they count once they hold a bed).
    fn in_home_claim(&self, tile: TilePosition) -> bool {
        self.bases.claim_at(tile).is_some_and(|claim| {
            self.bases
                .modules_in(claim.id)
                .any(|module| Self::is_bed_module(module.kind))
        })
    }

    /// Whether `tile` is fenced: inside a claim with palisade posts within
    /// [`PALISADE_REACH`] columns on both sides (and inside the claim).
    #[must_use]
    pub fn palisade_guarded(&self, tile: TilePosition) -> bool {
        let Some(claim) = self.bases.claim_at(tile) else {
            return false;
        };
        let (min, max) = claim.bounds();
        let post = |x: i32| {
            (tile.y - PALISADE_ROWS.0..=tile.y + PALISADE_ROWS.1).any(|y| {
                self.terrain
                    .tile(TilePosition { x, y })
                    .is_some_and(|t| t.kind == TileKind::Built(BuiltTile::Palisade))
            })
        };
        let left = ((tile.x - PALISADE_REACH).max(min.x)..tile.x).any(post);
        left && (tile.x + 1..=(tile.x + PALISADE_REACH).min(max.x)).any(post)
    }

    /// Light from night things at `at` (permille): will-o'-wisps and, at night, shallow
    /// nocturnal ore.
    pub(super) fn night_glow_at(&self, at: TilePosition) -> u16 {
        let mut light = 0;
        let (cx, cy) = self.frame().centre_of(at.x, at.y);
        for id in self.entities.ids_in_radius(
            cx,
            cy,
            TILE_SIZE * (f32::from(u8::try_from(WISP_LIGHT_TILES).unwrap_or(3)) + 1.0),
        ) {
            if let Some((body, state)) = self.enemy_parts(id)
                && state.kind == EnemyKind::WillOWisp
            {
                light = light.max(falloff(
                    self.frame().tile_at(body.x, body.y),
                    at,
                    WISP_LIGHT_TILES,
                    WISP_LIGHT,
                ));
            }
        }
        if self.clock().is_night() {
            let reach = NOCTURNAL_ORE_LIGHT_TILES;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let position = TilePosition {
                        x: at.x + dx,
                        y: at.y + dy,
                    };
                    if self.nocturnal_ore_at(position) {
                        light = light.max(falloff(position, at, reach, NOCTURNAL_ORE_LIGHT));
                    }
                }
            }
        }
        light
    }

    /// Whether `tile` holds nocturnal ore shallow enough to glow at night.
    #[must_use]
    pub fn nocturnal_ore_at(&self, tile: TilePosition) -> bool {
        let depth = tile.y - self.terrain.surface_row(tile.x);
        (0..=NOCTURNAL_GLOW_DEPTH).contains(&depth)
            && self.terrain.tile(tile).is_some_and(
                |t| matches!(t.kind, TileKind::Ore(mineral) if night::glows_at_night(mineral)),
            )
    }

    /// A light that draws `kind` at `(x, y)`, if any: a lit flare for cave bats; a flare or a
    /// lamp fixture for moth swarms (they circle it instead of hunting).
    pub(super) fn lure_for(&self, kind: EnemyKind, x: f32, y: f32) -> Option<(f32, f32)> {
        match kind {
            EnemyKind::CaveBat => self.flare_near(x, y),
            EnemyKind::MothSwarm => self.flare_near(x, y).or_else(|| self.lamp_near(x, y)),
            _ => None,
        }
    }

    /// The nearest lamp fixture within [`MOTH_LURE_TILES`] of `(x, y)` (world pixels of its
    /// centre), on the open surface.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a lure radius of a dozen tiles"
    )]
    fn lamp_near(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let here = self.frame().tile_at(x, y);
        let reach = MOTH_LURE_TILES as i32;
        self.bases
            .fixtures_in(
                TilePosition {
                    x: here.x - reach,
                    y: here.y - reach,
                },
                TilePosition {
                    x: here.x + reach,
                    y: here.y + reach,
                },
            )
            .filter(|(_, fixtures)| {
                matches!(fixtures.front, Some((crate::base::Fixture::Light, _)))
            })
            .map(|(position, _)| self.frame().centre_of(position.x, position.y))
            .min_by(|a, b| {
                let da = (a.0 - x).hypot(a.1 - y);
                let db = (b.0 - x).hypot(b.1 - y);
                da.total_cmp(&db)
            })
            .filter(|(lx, ly)| (lx - x).hypot(ly - y) <= MOTH_LURE_TILES * TILE_SIZE)
    }

    /// Whether a lit flare near `(x, y)` sends a light-shunning `kind` running.
    pub(super) fn flare_scares(&self, kind: EnemyKind, x: f32, y: f32) -> bool {
        night::light_response(kind) == LightResponse::Shuns
            && self
                .entities
                .of_kind(EntityKind::Charge)
                .any(|entity| match entity.state {
                    EntityState::Charge(charge)
                        if charge.kind == ExplosiveKind::Flare && charge.stage > 0 =>
                    {
                        (entity.body.x - x).hypot(entity.body.y - y)
                            <= night::FLARE_SCARE_TILES * TILE_SIZE
                    }
                    _ => false,
                })
    }

    /// Actors out on the open surface where night monsters can reach them: `(player, world
    /// pixel)` of each live rig or pilot on foot that is awake and outside every safe place.
    fn exposed_actors(&self) -> Vec<(PlayerId, (f32, f32))> {
        self.awake_actors()
            .filter(|(_, (x, y))| !self.night_safe_zone(self.frame().tile_at(*x, *y)))
            .collect()
    }

    /// `(player, world pixel)` of each live, awake rig or pilot on foot.
    fn awake_actors(&self) -> impl Iterator<Item = (PlayerId, (f32, f32))> + '_ {
        self.players
            .iter()
            .filter(|(id, player)| player.hull > 0.0 && !self.clock.sleepers.contains_key(id))
            .filter_map(|(id, _)| self.actor_position(*id).map(|at| (*id, at)))
    }

    /// Awake actors near the open surface (within reach of nocturnal ore's glow), safe or not.
    fn surface_actors(&self) -> Vec<(PlayerId, (f32, f32))> {
        self.awake_actors()
            .filter(|(_, (x, y))| {
                let tile = self.frame().tile_at(*x, *y);
                let depth = tile.y - self.terrain.surface_row(tile.x);
                (-NIGHT_SKY_ROWS..=NOCTURNAL_GLOW_DEPTH).contains(&depth)
            })
            .collect()
    }

    /// The night's part of a tick (see the module docs).
    pub(super) fn advance_night(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if self.players.is_empty() {
            return;
        }
        let clock = self.clock_after(next_tick);
        let check = self
            .timing
            .rate
            .schedule_fires(next_tick, SPAWN_CHECK_MILLIS, 250);
        // Nocturnal ore is the night's gift, not its danger: it shows itself to anyone awake on
        // the surface, in a safe place or not, and on Peaceful worlds too.
        if check && clock.is_night() {
            let surface = self.surface_actors();
            self.reveal_nocturnal_ore(&surface);
        }
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let night = night::night_of(clock);
        self.turn_of_night(night, clock, output);
        if night.is_none() {
            self.retreat_night_monsters(next_tick, output);
            return;
        }
        if check {
            let actors = self.exposed_actors();
            if actors.is_empty() {
                return;
            }
            self.raise_night_monsters(next_tick, clock, &actors);
            if let Some(night) = night {
                self.raise_rare_spawn(next_tick, clock, night, &actors, output);
            }
            self.prowl(next_tick, &actors);
        }
    }

    /// The clock at `tick`.
    const fn clock_after(&self, tick: SimulationTick) -> WorldClock {
        WorldClock::at(tick, self.timing.rate, &self.clock, self.terrain.seed())
    }

    /// Records night falling and lifting in the ledger, with their events.
    fn turn_of_night(&mut self, night: Option<u32>, clock: WorldClock, output: &mut TickOutput) {
        let ledger = &mut self.enemies.night;
        if ledger.active_night == night {
            return;
        }
        if let Some(ended) = ledger.active_night.take() {
            ledger.nights_survived = ledger.nights_survived.saturating_add(1);
            output.push_event(GameplayEventKind::NightSurvived {
                night: ended,
                kills: ledger.night_kills,
            });
            ledger.night_kills = 0;
        }
        if let Some(begun) = night {
            ledger.active_night = Some(begun);
            ledger.night_kills = 0;
            output.push_event(GameplayEventKind::NightFell {
                night: begun,
                moon: clock.moon_phase(),
            });
        }
    }

    /// By day, every night monster burrows or flees: each runs for a staggered moment, then
    /// leaves the world.
    fn retreat_night_monsters(&mut self, now: SimulationTick, output: &mut TickOutput) {
        let monsters: Vec<(EntityId, crate::entity::Body, crate::enemy::EnemyState)> = self
            .night_monsters()
            .map(|(entity, state)| (entity.id, entity.body, *state))
            .collect();
        if monsters.is_empty() {
            return;
        }
        let mut gone = BTreeSet::new();
        for (id, body, mut state) in monsters {
            if state.mode == EnemyMode::Flee && now >= state.mode_until {
                gone.insert(id);
                #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
                output.push_event(GameplayEventKind::NightMonsterBurrowed {
                    entity: id.get(),
                    kind: state.kind,
                    x: body.x.round() as i32,
                    y: body.y.round() as i32,
                });
                continue;
            }
            if state.mode == EnemyMode::Flee {
                continue;
            }
            let stagger = (id.get().wrapping_mul(0x9E37_79B9) >> 7) % RETREAT_STAGGER_MILLIS;
            state.mode = EnemyMode::Flee;
            state.mode_until = self.after(now, RETREAT_MILLIS + stagger);
            state.waypoint = None;
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
        }
        if !gone.is_empty() {
            let _gone = self.entities.despawn_many(&gone);
        }
    }

    /// Night monsters round each exposed actor (see the module docs).
    fn raise_night_monsters(
        &mut self,
        now: SimulationTick,
        clock: WorldClock,
        actors: &[(PlayerId, (f32, f32))],
    ) {
        let moon = clock.moon_phase();
        let difficulty = self.enemies.difficulty;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a per-mille chance in 0..=1000"
        )]
        let chance = (NIGHT_SPAWN_PER_MILLE as f32
            * difficulty.density()
            * hazard_density(self.terrain.settings().hazards)
            * night::moon_density_percent(moon) as f32
            / 100.0)
            .clamp(0.0, 1000.0) as u64;
        let cap = NIGHT_LOCAL_CAP + night::moon_extra_cap(moon) + difficulty.extra_cap() / 2;
        let seed = self.terrain.seed();
        let views: Vec<TilePosition> = actors
            .iter()
            .map(|(_, (x, y))| self.frame().tile_at(*x, *y))
            .collect();
        for (index, (player_id, (x, y))) in actors.iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES
                || self.night_monsters().count() >= MAX_NIGHT_MONSTERS
            {
                return;
            }
            let local = self
                .night_monsters()
                .filter(|(entity, _)| {
                    (entity.body.x - x).hypot(entity.body.y - y) <= NIGHT_LOCAL_TILES * TILE_SIZE
                })
                .count();
            if local >= cap {
                continue;
            }
            let roll = hash2(
                seed ^ now.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_NIGHT_SPAWN,
                i32::try_from(player_id.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(index).unwrap_or(0),
            );
            if roll % 1000 >= chance {
                continue;
            }
            let here = self.frame().tile_at(*x, *y);
            if let Some((kind, site)) = self.find_night_site(here, roll, &views, None) {
                let count = night::pack_size(kind, roll >> 24);
                self.raise_pack(kind, site, count);
            }
        }
    }

    /// A spot for a night monster rising for an actor at `here`: a column on either side out of
    /// view, the biome's roster (or `forced`), and ground that fits the kind.
    fn find_night_site(
        &self,
        here: TilePosition,
        roll: u64,
        views: &[TilePosition],
        forced: Option<EnemyKind>,
    ) -> Option<(EnemyKind, TilePosition)> {
        let seed = self.terrain.seed();
        let span = NIGHT_SPAWN_COLUMNS.1 - NIGHT_SPAWN_COLUMNS.0;
        for attempt in 0..8_i32 {
            let hash = hash2(seed ^ roll, SALT_NIGHT_SITE, here.x ^ attempt, here.y);
            let side = if hash & 1 == 0 { 1 } else { -1 };
            let distance = NIGHT_SPAWN_COLUMNS.0
                + i32::try_from((hash >> 4) % u64::try_from(span).unwrap_or(1)).unwrap_or(0);
            let column = here.x + side * distance;
            if !self.terrain.contains_column(column) {
                continue;
            }
            let kind = forced.unwrap_or_else(|| self.roster_pick(column, hash));
            let Some(spot) = self.night_site_in(kind, column, hash) else {
                continue;
            };
            let out_of_view = views.iter().all(|view| {
                (spot.x - view.x).abs() > VIEW_HALF_TILES.0
                    || (spot.y - view.y).abs() > VIEW_HALF_TILES.1
            });
            if out_of_view && !self.night_safe_zone(spot) {
                return Some((kind, spot));
            }
        }
        None
    }

    /// A night monster of the roster of `column`'s biome, picked by `hash`.
    fn roster_pick(&self, column: i32, hash: u64) -> EnemyKind {
        let biome = crate::worldgen::biome::biome_at(self.terrain.settings(), column);
        let roster = night::night_roster(biome);
        let total: u32 = roster.iter().map(|(_, weight)| weight).sum();
        let mut pick = u32::try_from((hash >> 16) % u64::from(total.max(1))).unwrap_or(0);
        for (candidate, weight) in roster {
            if pick < *weight {
                return *candidate;
            }
            pick -= weight;
        }
        roster[0].0
    }

    /// Where `kind` rises in `column`: on the ground (walkers), a few tiles over it (flyers) or
    /// just under the soft crust (stalkers).
    fn night_site_in(&self, kind: EnemyKind, column: i32, hash: u64) -> Option<TilePosition> {
        let ground = self.terrain.surface_row(column);
        let air = |y: i32| {
            self.terrain
                .tile(TilePosition { x: column, y })
                .is_none_or(|t| t.kind == TileKind::Air)
        };
        match kind.stats().locomotion {
            crate::enemy::Locomotion::Walking | crate::enemy::Locomotion::Stationary => {
                let solid = self
                    .terrain
                    .tile(TilePosition {
                        x: column,
                        y: ground,
                    })
                    .is_some_and(|t| t.kind != TileKind::Air);
                (solid && air(ground - 1) && air(ground - 2)).then_some(TilePosition {
                    x: column,
                    y: ground - 1,
                })
            }
            crate::enemy::Locomotion::Flying => {
                let lift = 3 + i32::try_from((hash >> 40) % 4).unwrap_or(0);
                let y = ground - lift;
                (air(y) && air(y + 1)).then_some(TilePosition { x: column, y })
            }
            crate::enemy::Locomotion::Burrowing => {
                let tile = TilePosition {
                    x: column,
                    y: ground + 1,
                };
                self.terrain
                    .tile(tile)
                    .is_some_and(|t| night::stalker_ground(t.kind))
                    .then_some(tile)
            }
        }
    }

    /// Spawns `count` of `kind` round `site` (a pack spreads along the ground).
    fn raise_pack(&mut self, kind: EnemyKind, site: TilePosition, count: usize) -> Vec<EntityId> {
        let (town_first, town_last) = town_span();
        let tier = night::night_tier(site.x - i32::midpoint(town_first, town_last));
        let mut ids = Vec::new();
        for index in 0..count {
            let offset = [0, 2, -2, 4, -4][index % 5];
            let column = site.x + offset;
            let tile = if offset == 0 {
                Some(site)
            } else {
                self.night_site_in(kind, column, 0)
                    .filter(|tile| (tile.y - site.y).abs() <= 2)
            };
            let Some(tile) = tile else {
                continue;
            };
            let point = Self::spawn_point(self.frame(), kind, tile);
            ids.push(self.spawn_enemy_with(kind, point, tier, None, None));
        }
        ids
    }

    /// On a new-moon night, once: the umbral alpha and its hounds rise near an exposed actor.
    fn raise_rare_spawn(
        &mut self,
        now: SimulationTick,
        clock: WorldClock,
        night: u32,
        actors: &[(PlayerId, (f32, f32))],
        output: &mut TickOutput,
    ) {
        if !night::is_rare_spawn_night(clock.moon_phase())
            || self.enemies.night.rare_night == Some(night)
            || self.entities.count_of(EntityKind::Enemy) + 3 > MAX_ENEMIES
        {
            return;
        }
        let roll = hash2(
            self.terrain.seed() ^ now.get().wrapping_mul(0xD6E8_FEB8_6659_FD93),
            SALT_RARE,
            i32::try_from(night).unwrap_or(0),
            0,
        );
        if roll % 1000 >= u64::from(RARE_SPAWN_PER_MILLE) {
            return;
        }
        let Some((_, (x, y))) =
            actors.get(usize::try_from((roll >> 12) % 64).unwrap_or(0) % actors.len())
        else {
            return;
        };
        let views: Vec<TilePosition> = actors
            .iter()
            .map(|(_, (x, y))| self.frame().tile_at(*x, *y))
            .collect();
        let here = self.frame().tile_at(*x, *y);
        let Some((kind, site)) =
            self.find_night_site(here, roll, &views, Some(EnemyKind::UmbralAlpha))
        else {
            return;
        };
        let ids = self.raise_pack(kind, site, 1);
        let Some(alpha) = ids.first().copied() else {
            return;
        };
        // Its packmate runs at its heels.
        let _pack = self.raise_pack(
            EnemyKind::ShadowHound,
            TilePosition {
                x: site.x + 3,
                y: site.y,
            },
            1,
        );
        self.enemies.night.rare_night = Some(night);
        if let Some((body, _)) = self.enemy_parts(alpha) {
            #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
            output.push_event(GameplayEventKind::RareNightSpawn {
                kind,
                x: body.x.round() as i32,
                y: body.y.round() as i32,
            });
        }
    }

    /// Idle night monsters scent the nearest exposed actor and prowl toward it (see the module
    /// docs).
    fn prowl(&mut self, now: SimulationTick, actors: &[(PlayerId, (f32, f32))]) {
        let idle: Vec<(EntityId, crate::entity::Body, crate::enemy::EnemyState)> = self
            .night_monsters()
            .filter(|(_, state)| matches!(state.mode, EnemyMode::Idle | EnemyMode::Return))
            .map(|(entity, state)| (entity.id, entity.body, *state))
            .collect();
        for (id, body, mut state) in idle {
            let scent = SCENT_TILES * TILE_SIZE;
            let prey = actors
                .iter()
                .map(|(_, at)| *at)
                .filter(|(x, y)| (x - body.x).abs() <= scent && (y - body.y).abs() <= scent)
                .min_by(|a, b| {
                    let da = (a.0 - body.x).hypot(a.1 - body.y);
                    let db = (b.0 - body.x).hypot(b.1 - body.y);
                    da.total_cmp(&db)
                });
            let Some((px, py)) = prey else {
                continue;
            };
            let jitter = i32::try_from(
                hash2(
                    u64::from(state.seed),
                    SALT_PROWL,
                    // A fresh flank every ten seconds of real time, whatever the tick rate.
                    i32::try_from(
                        (now.get() / self.timing.ticks_for_millis(10_000).max(1)) & 0xFFFF,
                    )
                    .unwrap_or(0),
                    0,
                ) % 5,
            )
            .unwrap_or(0)
                - 2;
            let goal = self.frame().tile_at(px, py);
            let column = self.prowl_column(
                state.kind,
                self.frame().tile_at(body.x, body.y).x,
                goal.x + jitter,
            );
            let ground = self.terrain.surface_row(column);
            let home = match state.kind.stats().locomotion {
                crate::enemy::Locomotion::Burrowing => (column, ground + 1),
                crate::enemy::Locomotion::Flying => (column, (goal.y - 2).min(ground - 2)),
                crate::enemy::Locomotion::Walking | crate::enemy::Locomotion::Stationary => {
                    (column, ground - 1)
                }
            };
            if state.home == home && state.mode == EnemyMode::Idle {
                continue;
            }
            state.home = home;
            state.mode = EnemyMode::Idle;
            state.waypoint = None;
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
        }
    }

    /// The column a prowling `kind` heads for on its way from column `from` to `to`: right up
    /// to its prey, or (for light-shunning kinds) the last spot before a safe place begins.
    fn prowl_column(&self, kind: EnemyKind, from: i32, to: i32) -> i32 {
        if night::light_response(kind) != LightResponse::Shuns {
            return to;
        }
        let step = if to >= from { 1 } else { -1 };
        let mut column = from;
        while column != to {
            let next = column + step;
            let ground = self.terrain.surface_row(next);
            if self.night_safe_zone(TilePosition {
                x: next,
                y: ground - 1,
            }) {
                return column;
            }
            column = next;
        }
        to
    }

    /// Shallow glowing ore near each exposed actor reveals itself on that player's map.
    fn reveal_nocturnal_ore(&mut self, actors: &[(PlayerId, (f32, f32))]) {
        for (player_id, (x, y)) in actors {
            let here = self.frame().tile_at(*x, *y);
            let mut found = Vec::new();
            for column in here.x - NOCTURNAL_REVEAL_TILES..=here.x + NOCTURNAL_REVEAL_TILES {
                if !self.terrain.contains_column(column) {
                    continue;
                }
                let ground = self.terrain.surface_row(column);
                for row in ground..=ground + NOCTURNAL_GLOW_DEPTH {
                    let tile = TilePosition { x: column, y: row };
                    if (row - here.y).abs() <= NOCTURNAL_REVEAL_TILES + NOCTURNAL_GLOW_DEPTH
                        && self.nocturnal_ore_at(tile)
                    {
                        found.push(tile);
                    }
                }
            }
            if found.is_empty() {
                continue;
            }
            let discovered = self.discovered_tiles.entry(*player_id).or_default();
            for tile in found {
                discovered.reveal(tile);
            }
        }
    }
}
