//! Sky enemies (Wave 4 T21, expansion plan WP-SKY "sky enemies"): spawning in the hostile sky
//! of V9 worlds, swoops, lightning, airships with turrets and boarders, harpy nests and temple
//! guards. The kinds are ordinary [`EnemyKind`]s, so damage (`damage_entity` ->
//! `damage_enemy`), drops (`spawn_loot`), safe zones, difficulty, saves and replication are the
//! shared enemy machinery; this module adds what is particular to the sky.
//!
//! * **Where.** The hostile sky starts at the cloud archipelago on V9 worlds
//!   ([`WorldState::hostile_sky`]); the low isles stay calm so early hopping is safe. Sky
//!   cities ([`crate::worldgen::sky::continent::sky_city_at`]), bases and infrastructure are
//!   safe zones ([`WorldState::sky_safe_zone`], folded into `enemy_safe_zone`).
//! * **Spawning** ([`WorldState::spawn_sky_enemies`], from the director each second): around
//!   each player's actor (the pilot on foot, else the rig) in the archipelago, the dead-air gap
//!   or the continents, under a local cap, the band's roster ([`sky_roster`]) spawns in open
//!   air on a ring out of view; an airship arrives with its two turrets. Nests and temples
//!   ([`WorldState::fill_sky_lairs`], every two seconds) keep a harpy nester on each nest and a
//!   sentry and a storm elemental at each temple's guard posts until they are killed (cleared
//!   like lairs).
//! * **Swoops** ([`AttackStyle::Swoop`]): climb to a perch above the prey, hang (the wind-up,
//!   [`SkyEnemyAction::Swoop`]), dive through it (contact hurts) and climb away.
//! * **Lightning** ([`AttackStyle::Lightning`]): a crackling wind-up, then an instant arc; a rig
//!   it strikes is jolted like a machine hit by an EMP: its capacitor drains and its weapon
//!   jams for [`JOLT_MILLIS`]. Pilots just take the hit.
//! * **Airships** ([`AttackStyle::Boarding`]): hold station above and to the side of their
//!   prey, drop pairs of jetpack boarders ([`EnemyKind::SkyPirate`]) every cooldown while the
//!   prey is near, and carry two [`EnemyKind::AirshipTurret`]s that ride the hull (their
//!   `home` field holds their mount offset in pixels) and lob cannonballs. A downed hull takes
//!   its turrets with it ([`SkyEnemyAction::AirshipDowned`]).
//! * **Harpy nesters** ([`EnemyKind::HarpyNester`]) only notice prey within
//!   [`NEST_GUARD_TILES`] of their nest, until an egg is taken: then they hunt anyone in sight
//!   ([`SkyEnemyAction::NesterEnraged`]).
//!
//! Every spawn decision is hashed from the seed, the tick and positions (no RNG state);
//! gravity fields apply to stunned flyers through `BodyParams::at`.

use crate::{
    enemy::{
        AttackStyle, EnemyKind, EnemyMode, EnemyState, SkyEnemyAction, hazard_density, sky_roster,
        sky_tier, tier_scale, tile_centre,
    },
    entity::{Body, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    physics,
    quantize::quantize_simulation_scalar as quantize,
    sky::{SkyBand, SkyTile},
    terrain::{ChunkPosition, TILE_SIZE, TileKind, TilePosition},
    worldgen::{TileRect, noise::hash2, sky::continent},
};

use super::{BASE_SAFE_TILES, Intent, MAX_ENEMIES, RING, WorldState, direction, tile_of};

/// Tiles around a sky city's area that stay safe.
pub const SKY_CITY_SAFE_TILES: i32 = 12;
/// A rig struck by lightning has its weapon jammed (and capacitor drained) this long.
pub const JOLT_MILLIS: u64 = 1_500;
/// A harpy nester only notices prey this close to its nest (until its eggs are taken).
pub const NEST_GUARD_TILES: f32 = 9.0;
/// Rows above its nest a nester spawns and idles.
const NESTER_PERCH_ROWS: i32 = 3;
/// Most boarders an airship keeps alive.
const MAX_BOARDERS: usize = 3;
/// Sky enemies counted toward an actor's local cap within this many tiles.
const SKY_LOCAL_TILES: f32 = 40.0;
/// Sky spawns land this far from the actor, in tiles.
const SKY_SPAWN_TILES: (i32, i32) = (24, 32);
/// A sky chunk that spawned may not spawn again for this long.
const SKY_COOLDOWN_MILLIS: u64 = 30_000;
/// The perch a swooper climbs to before diving: columns to the side, rows above its prey.
const PERCH: (f32, f32) = (TILE_SIZE * 4.0, TILE_SIZE * 5.0);
/// Where an airship holds station: columns to the side, rows above its prey.
const STATION: (f32, f32) = (TILE_SIZE * 8.0, TILE_SIZE * 7.0);
/// Mount offsets of an airship's two turrets from the hull's centre, in pixels.
pub const TURRET_MOUNTS: [(i32, i32); 2] = [(-30, 22), (30, 22)];
/// Base damage of a lightning arc (scaled by tier and difficulty).
const LIGHTNING_DAMAGE: f32 = 10.0;

const SALT_SKY_SPAWN: u64 = 0xE7E1_5C01;
const SALT_SKY_SITE: u64 = 0xE7E1_5C02;

impl WorldState {
    /// Whether `tile` lies in the hostile sky: the archipelago band and up on a V9 world
    /// (sky enemies roam there; the low isles stay calm).
    #[must_use]
    pub fn hostile_sky(&self, tile: TilePosition) -> bool {
        self.terrain.settings().generator.has_sky_content()
            && SkyBand::at_row_of(self.terrain.settings(), tile.y) >= SkyBand::Archipelago
    }

    /// Whether `tile` in the hostile sky is safe anyway: inside a sky city's area
    /// ([`SKY_CITY_SAFE_TILES`] around it), in or near a sky town of the archipelago (T20,
    /// [`super::VILLAGE_SAFE_TILES`] around it, the village rule), around placed
    /// infrastructure, in a claimed base, or on a home plot (sky villas, T24).
    #[must_use]
    pub fn sky_safe_zone(&self, tile: TilePosition) -> bool {
        continent::sky_city_at(self.terrain.settings(), tile, SKY_CITY_SAFE_TILES).is_some()
            || crate::worldgen::streamed::in_village_area(
                self.terrain.settings(),
                tile,
                super::VILLAGE_SAFE_TILES,
            )
            || self.infrastructure.iter().any(|item| {
                (item.position.x - tile.x).abs() <= BASE_SAFE_TILES
                    && (item.position.y - tile.y).abs() <= BASE_SAFE_TILES
            })
            || self.bases.is_safe_zone(tile)
            || self.homes.is_safe_zone(tile)
    }

    /// The director's sky pass (see the module docs): roaming sky enemies around actors in the
    /// hostile sky.
    pub(super) fn spawn_sky_enemies(&mut self, now: SimulationTick) {
        if !self.terrain.settings().generator.has_sky_content() {
            return;
        }
        let difficulty = self.enemies.difficulty;
        let density = difficulty.density() * hazard_density(self.terrain.settings().hazards);
        let actors: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .filter_map(|id| self.actor_tile(*id).map(|tile| (*id, tile)))
            .collect();
        let rigs = self.rig_tiles();
        let seed = self.terrain.seed();
        for (index, (player_id, tile)) in actors.iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                break;
            }
            if !self.hostile_sky(*tile) || self.enemy_safe_zone(*tile) {
                continue;
            }
            let band = SkyBand::at_row_of(self.terrain.settings(), tile.y);
            let roster = sky_roster(band);
            if roster.is_empty() {
                continue;
            }
            let (cx, cy) = tile_centre(tile.x, tile.y);
            let local = self.enemies_near(cx, cy, SKY_LOCAL_TILES * TILE_SIZE).len();
            let cap = 2 + usize::from(band.code()) + difficulty.extra_cap();
            if local >= cap {
                continue;
            }
            let roll = hash2(
                seed ^ now.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_SKY_SPAWN,
                i32::try_from(player_id.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(index).unwrap_or(0),
            );
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a per-mille chance in 0..=1000"
            )]
            let chance = (150.0 * density).clamp(0.0, 1000.0) as u64;
            if roll % 1000 >= chance {
                continue;
            }
            let (dx, dy) = RING[usize::try_from((roll >> 12) % 16).unwrap_or(0)];
            let span = SKY_SPAWN_TILES.1 - SKY_SPAWN_TILES.0;
            let distance = SKY_SPAWN_TILES.0
                + i32::try_from((roll >> 20) % u64::try_from(span).unwrap_or(1)).unwrap_or(0);
            let target = TilePosition {
                x: tile.x + dx * distance / 16,
                y: tile.y + dy * distance * 10 / (16 * 16),
            };
            let chunk = ChunkPosition::of(target);
            if self.enemies.chunk_cooling(chunk, now) {
                continue;
            }
            let Some((kind, site)) = self.find_sky_site(target, roster, roll, &rigs) else {
                continue;
            };
            let tier = sky_tier(band);
            self.spawn_sky_group(kind, site, tier);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "bounded milliseconds"
            )]
            let cooldown = (SKY_COOLDOWN_MILLIS as f32 / density.max(0.25)) as u64;
            let until = self.after(now, cooldown);
            self.enemies.cool_chunk(chunk, until);
        }
    }

    /// An open-air spot near `around` for one of `roster`'s kinds: out of every rig's view,
    /// outside safe zones, in the hostile sky.
    fn find_sky_site(
        &self,
        around: TilePosition,
        roster: &[(EnemyKind, u32)],
        roll: u64,
        rigs: &[(PlayerId, TilePosition)],
    ) -> Option<(EnemyKind, TilePosition)> {
        let total: u32 = roster.iter().map(|(_, weight)| weight).sum();
        let mut pick = u32::try_from((roll >> 32) % u64::from(total.max(1))).unwrap_or(0);
        let mut kind = roster.first()?.0;
        for (candidate, weight) in roster {
            if pick < *weight {
                kind = *candidate;
                break;
            }
            pick -= weight;
        }
        let seed = self.terrain.seed();
        (0..12_i32).find_map(|attempt| {
            let hash = hash2(seed ^ roll, SALT_SKY_SITE, around.x ^ attempt, around.y);
            let tile = TilePosition {
                x: around.x + i32::try_from(hash % 9).unwrap_or(0) - 4,
                y: around.y + i32::try_from((hash >> 8) % 7).unwrap_or(0) - 3,
            };
            let room = if kind == EnemyKind::PirateAirship {
                24
            } else {
                16
            };
            (self.hostile_sky(tile)
                && !self.enemy_safe_zone(tile)
                && Self::out_of_view(tile, rigs)
                && self.air(tile)
                && self.open_area(tile) >= room)
                .then_some((kind, tile))
        })
    }

    /// Spawns `kind` at `site` (an airship with its turrets).
    fn spawn_sky_group(&mut self, kind: EnemyKind, site: TilePosition, tier: u8) -> EntityId {
        let point = tile_centre(site.x, site.y);
        let id = self.spawn_enemy_with(kind, point, tier, None, None);
        if kind == EnemyKind::PirateAirship {
            self.mount_turrets(id, point, tier);
        }
        id
    }

    /// Mounts an airship's two turrets on hull `hull` at `point`.
    fn mount_turrets(&mut self, hull: EntityId, point: (f32, f32), tier: u8) {
        for (ox, oy) in TURRET_MOUNTS {
            #[allow(clippy::cast_precision_loss, reason = "small pixel offsets")]
            let at = (point.0 + ox as f32, point.1 + oy as f32);
            let turret =
                self.spawn_enemy_with(EnemyKind::AirshipTurret, at, tier, None, Some(hull));
            // A turret's home holds its mount offset (it never goes home: it rides the hull).
            self.entities.update(turret, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.home = (ox, oy);
                }
            });
        }
    }

    /// Spawns a pirate airship (with its turrets) centred at world pixel `(x, y)` at the tier
    /// of that sky band (scenes and tests; gameplay spawns through the director). Returns the
    /// hull.
    pub fn spawn_airship(&mut self, x: f32, y: f32) -> EntityId {
        let tier = sky_tier(SkyBand::at_row_of(self.terrain.settings(), tile_of(x, y).y));
        let id = self.spawn_enemy_with(EnemyKind::PirateAirship, (x, y), tier, None, None);
        self.mount_turrets(id, (x, y), tier);
        id
    }

    /// Keeps a harpy nester on every nest and the guards at every temple near an actor, until
    /// they are killed (then the spot is cleared, like a lair).
    pub(super) fn fill_sky_lairs(&mut self) {
        let settings = *self.terrain.settings();
        if !settings.generator.has_sky_content() {
            return;
        }
        let actors: Vec<TilePosition> = self
            .players
            .keys()
            .filter_map(|id| self.actor_tile(*id))
            .filter(|tile| self.hostile_sky(*tile))
            .collect();
        let rigs = self.rig_tiles();
        for actor in actors {
            let around = TileRect {
                min: TilePosition {
                    x: actor.x - 48,
                    y: actor.y - 32,
                },
                max: TilePosition {
                    x: actor.x + 48,
                    y: actor.y + 32,
                },
            };
            let mut posts: Vec<(EnemyKind, TilePosition)> =
                continent::nests_touching(&settings, &around)
                    .into_iter()
                    .map(|nest| {
                        (
                            EnemyKind::HarpyNester,
                            TilePosition {
                                x: nest.x,
                                y: nest.y - NESTER_PERCH_ROWS,
                            },
                        )
                    })
                    .collect();
            for (_, temple) in continent::temples_touching(&settings, &around) {
                let [sentry, storm] = temple.guard_posts();
                posts.push((EnemyKind::SentryTurret, sentry));
                posts.push((EnemyKind::StormElemental, storm));
            }
            for (kind, post) in posts {
                if self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
                    return;
                }
                // The post's tile names the lair (one occupant per post; posts are tiles, so
                // two posts never share a key).
                let lair = ChunkPosition {
                    x: post.x,
                    y: post.y,
                };
                if self.enemies.cleared_lairs.contains(&lair)
                    || self.enemies().any(|(_, state)| state.lair == Some(lair))
                    || !Self::out_of_view(post, &rigs)
                {
                    continue;
                }
                let tier = sky_tier(SkyBand::at_row_of(self.terrain.settings(), post.y));
                let point = Self::spawn_point(kind, post);
                let _id = self.spawn_enemy_with(kind, point, tier, Some(lair), None);
            }
        }
    }

    // --- Behaviour ------------------------------------------------------------------------

    /// Chase behaviour of the sky attack styles (swoop, lightning, boarding).
    #[allow(clippy::too_many_arguments, reason = "the chase's whole context")]
    pub(super) fn sky_chase(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        (tx, ty): (f32, f32),
        distance: f32,
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> Intent {
        let spec = state.kind.stats();
        let range = spec.attack_range_tiles * TILE_SIZE;
        let ready = now >= state.attack_ready;
        let side: f32 = if body.x < tx { -1.0 } else { 1.0 };
        match spec.attack {
            AttackStyle::Swoop => {
                let perch = (side.mul_add(PERCH.0, tx), ty - PERCH.1);
                let (_, to_perch) = direction((body.x, body.y), perch);
                if ready && distance <= range && to_perch < TILE_SIZE * 1.5 {
                    self.start_windup(id, body, state, now, output);
                    output.push_event(sky_event(id, state.kind, body, SkyEnemyAction::Swoop));
                    return Intent::default();
                }
                // A wyvern spits fire from afar while it closes in.
                if state.kind == EnemyKind::Wyvern
                    && ready
                    && distance > range
                    && distance < range * 1.8
                    && physics::line_clear(&self.terrain, body.x, body.y, tx, ty)
                {
                    self.fire(
                        id,
                        body,
                        state,
                        crate::enemy::ShotKind::Fireball,
                        1,
                        (tx, ty),
                        output,
                    );
                    state.attack_ready = self.after(now, spec.cooldown_millis / 2);
                }
                Intent {
                    goal: Some(perch),
                    speed: spec.speed,
                }
            }
            AttackStyle::Lightning => {
                let clear = physics::line_clear(&self.terrain, body.x, body.y, tx, ty);
                if ready && distance <= range && clear {
                    self.start_windup(id, body, state, now, output);
                    return Intent::default();
                }
                // Hang back at about two thirds of its reach, a little above.
                let goal = if distance < range * 0.5 {
                    let ((nx, ny), _) = direction((tx, ty), (body.x, body.y));
                    (
                        nx.mul_add(TILE_SIZE * 3.0, body.x),
                        ny.mul_add(TILE_SIZE, body.y) - TILE_SIZE,
                    )
                } else {
                    (tx, TILE_SIZE.mul_add(-3.0, ty))
                };
                Intent {
                    goal: Some(goal),
                    speed: spec.speed,
                }
            }
            AttackStyle::Boarding => {
                if ready && distance <= range {
                    state.attack_ready = self.after(now, self.attack_cooldown_millis(state.kind));
                    self.drop_boarders(id, body, state.tier, output);
                }
                Intent {
                    goal: Some((side.mul_add(STATION.0, tx), ty - STATION.1)),
                    speed: spec.speed,
                }
            }
            _ => Intent::default(),
        }
    }

    /// The end of a sky wind-up: a swoop's dive or a lightning arc. Returns whether it
    /// handled the style.
    pub(super) fn sky_release(
        &mut self,
        body: &mut Body,
        state: &mut EnemyState,
        target: (f32, f32),
        now: SimulationTick,
        output: &mut TickOutput,
    ) -> bool {
        let spec = state.kind.stats();
        match spec.attack {
            AttackStyle::Swoop => {
                // Dive through the prey, aimed a little below it so the path arcs through.
                let aim = (target.0, TILE_SIZE.mul_add(0.5, target.1));
                let ((nx, ny), _) = direction((body.x, body.y), aim);
                body.velocity_x = quantize(nx * spec.dash_speed);
                body.velocity_y = quantize(ny * spec.dash_speed);
                state.mode = EnemyMode::Strike;
                state.mode_until = self.after(now, 650);
                true
            }
            AttackStyle::Lightning => {
                self.lightning_arc(body, state, target, output);
                state.mode = EnemyMode::Recover;
                state.mode_until = self.after(now, 600);
                true
            }
            _ => false,
        }
    }

    /// A storm elemental's arc at `target`: it hits the prey if it is still in reach and in
    /// line, jolting a rig (capacitor drained, weapon jammed).
    fn lightning_arc(
        &mut self,
        body: &Body,
        state: &EnemyState,
        target: (f32, f32),
        output: &mut TickOutput,
    ) {
        let spec = state.kind.stats();
        let reach = spec.attack_range_tiles * TILE_SIZE * 1.25;
        let damage = LIGHTNING_DAMAGE * tier_scale(state.tier);
        let now = self.simulation_tick.next();
        let player = state.target;
        let mut struck = None;
        let mut end = target;
        if let Some(player) = player {
            let (_, distance) = direction((body.x, body.y), target);
            let clear = physics::line_clear(&self.terrain, body.x, body.y, target.0, target.1);
            if distance <= reach && clear {
                let rig_hit = self
                    .players
                    .get(&player)
                    .is_some_and(|rig| (rig.x - target.0).hypot(rig.y - target.1) < 2.0);
                if rig_hit {
                    if self.strike_rig(
                        player,
                        damage,
                        (body.x, body.y),
                        spec.knockback,
                        state.kind,
                        now,
                        output,
                    ) {
                        self.jolt_rig(player);
                        struck = Some(player);
                    }
                } else if let Some((pilot, _, _)) = self.pilot_of(player)
                    && self.strike_pilot_as(
                        Some(state.kind),
                        pilot,
                        damage,
                        (body.x, body.y),
                        spec.knockback,
                        output,
                    )
                {
                    struck = Some(player);
                }
            } else {
                // Out of reach: the arc earths short of its target.
                let ((nx, ny), _) = direction((body.x, body.y), target);
                end = (nx.mul_add(reach, body.x), ny.mul_add(reach, body.y));
            }
        }
        #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
        output.push_event(GameplayEventKind::LightningArc {
            x: body.x.round() as i32,
            y: body.y.round() as i32,
            end_x: end.0.round() as i32,
            end_y: end.1.round() as i32,
            struck,
        });
    }

    /// A lightning jolt on `player`'s rig: the capacitor drains and the weapon jams for
    /// [`JOLT_MILLIS`] (the EMP-like effect of storm elementals on machines).
    pub(super) fn jolt_rig(&mut self, player: PlayerId) {
        let jam = self
            .timing
            .ticks_for_millis(JOLT_MILLIS)
            .max(1)
            .saturating_add(self.simulation_tick.get());
        if let Some(rig) = self.players.get_mut(&player) {
            rig.weapon.energy = 0.0;
            rig.weapon.ready_tick = rig.weapon.ready_tick.max(SimulationTick::new(jam));
        }
    }

    /// An airship drops a pair of boarders under its hull (while it has fewer than
    /// [`MAX_BOARDERS`]).
    fn drop_boarders(&mut self, id: EntityId, body: &Body, tier: u8, output: &mut TickOutput) {
        let alive = self
            .enemies()
            .filter(|(_, state)| {
                state.parent == Some(id.get()) && state.kind == EnemyKind::SkyPirate
            })
            .count();
        if alive >= MAX_BOARDERS || self.entities.count_of(EntityKind::Enemy) >= MAX_ENEMIES {
            return;
        }
        let radius = EnemyKind::PirateAirship.stats().radius;
        for side in [-1.0_f32, 1.0] {
            let at = (side.mul_add(10.0, body.x), body.y + radius + 12.0);
            if physics::solid_at(&self.terrain, at.0, at.1) {
                continue;
            }
            let pirate = self.spawn_enemy_with(EnemyKind::SkyPirate, at, tier, None, Some(id));
            self.entities.update(pirate, |entity| {
                entity.body.velocity_x = quantize(side * 60.0);
                entity.body.velocity_y = 140.0;
            });
        }
        output.push_event(sky_event(
            id,
            EnemyKind::PirateAirship,
            body,
            SkyEnemyAction::BoardersDropped,
        ));
    }

    /// Moves turret `state` onto its hull (its mount offset in `home`). Returns `false` when
    /// the hull is gone (the turret falls with it).
    ///
    /// The mount is a teleport, so it keeps the body rules: where the hull hugs rock and the
    /// mount point would sit in it, the turret takes the nearest open spot instead (at most
    /// [`super::UNSTICK_PIXELS`] off), or stays put until there is room.
    pub(super) fn ride_airship(&self, body: &mut Body, state: &EnemyState) -> bool {
        let Some(hull) = state
            .parent
            .and_then(|parent| self.entities.get(EntityId(parent)))
            .filter(|entity| entity.kind() == EntityKind::Enemy)
        else {
            return false;
        };
        #[allow(clippy::cast_precision_loss, reason = "small pixel offsets")]
        let (x, y) = (
            hull.body.x + state.home.0 as f32,
            hull.body.y + state.home.1 as f32,
        );
        let radius = state.kind.stats().radius;
        let (x, y) = if physics::collides(&self.terrain, x, y, radius) {
            physics::free_spot_near(&self.terrain, x, y, radius, super::UNSTICK_PIXELS)
                .unwrap_or((body.x, body.y))
        } else {
            (x, y)
        };
        body.previous_x = body.x;
        body.previous_y = body.y;
        body.x = quantize(x);
        body.y = quantize(y);
        body.velocity_x = hull.body.velocity_x;
        body.velocity_y = hull.body.velocity_y;
        true
    }

    /// What a sky enemy notices of `noticed`: a harpy nester ignores prey away from its nest
    /// unless its eggs are gone (then it rages, once per chase).
    pub(super) fn sky_notice(
        &self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        noticed: Option<PlayerId>,
        output: &mut TickOutput,
    ) -> Option<PlayerId> {
        if state.kind != EnemyKind::HarpyNester {
            return noticed;
        }
        let player = noticed?;
        let enraged = !self.nest_has_eggs(state.home);
        let (nx, ny) = tile_centre(state.home.0, state.home.1);
        let near_nest = self
            .rig_position(Some(player), (body.x, body.y))
            .is_some_and(|(x, y)| (x - nx).hypot(y - ny) <= NEST_GUARD_TILES * TILE_SIZE);
        if enraged {
            // Only as a chase starts (not again after every swoop's recovery).
            if matches!(
                state.mode,
                EnemyMode::Idle | EnemyMode::Dormant | EnemyMode::Return
            ) {
                output.push_event(sky_event(
                    id,
                    state.kind,
                    body,
                    SkyEnemyAction::NesterEnraged,
                ));
            }
            return Some(player);
        }
        near_nest.then_some(player)
    }

    /// Whether the nest under nester home `home` still holds its eggs.
    fn nest_has_eggs(&self, home: (i32, i32)) -> bool {
        let floor = home.1 + NESTER_PERCH_ROWS;
        [-1, 1].iter().all(|dx| {
            self.terrain
                .tile(TilePosition {
                    x: home.0 + dx,
                    y: floor - 1,
                })
                .is_some_and(|tile| tile.kind == TileKind::Sky(SkyTile::HarpyEgg))
        })
    }

    /// A dead airship's turrets fall with it.
    pub(super) fn sink_airship(&mut self, hull: EntityId, body: &Body, output: &mut TickOutput) {
        let turrets: std::collections::BTreeSet<EntityId> = self
            .enemies()
            .filter(|(_, state)| {
                state.parent == Some(hull.get()) && state.kind == EnemyKind::AirshipTurret
            })
            .map(|(entity, _)| entity.id)
            .collect();
        if !turrets.is_empty() {
            let _gone = self.entities.despawn_many(&turrets);
        }
        output.push_event(sky_event(
            hull,
            EnemyKind::PirateAirship,
            body,
            SkyEnemyAction::AirshipDowned,
        ));
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::missing_const_for_fn,
    reason = "bounded world pixels; float rounding is not const"
)]
fn sky_event(
    id: EntityId,
    kind: EnemyKind,
    body: &Body,
    action: SkyEnemyAction,
) -> GameplayEventKind {
    GameplayEventKind::SkyEnemyAction {
        entity: id.get(),
        kind,
        action,
        x: body.x.round() as i32,
        y: body.y.round() as i32,
    }
}
