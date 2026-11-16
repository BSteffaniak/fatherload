//! Hardpoint weapons in the world (WP-WEAPONS part 2): the per-tick trigger, capacitor and
//! aim, projectiles as entities, hitscan rays, hits on damageable entities and terrain, the
//! damage API and the shack's weapon services. Rules and tables live in [`crate::weapon`] and
//! [`crate::combat`].
//!
//! Order inside a tick: each player's [`WeaponControl`] is applied right after their movement
//! (recharge, aim, fire: hitscan hits land at once, projectiles spawn at the muzzle);
//! projectiles fly in [`EntityPhase::Late`](crate::entity::EntityPhase::Late), after drilling
//! and the bomb pass, so a rocket's warhead goes off in the next tick's bomb pass.
//!
//! Friendly fire is off: shots and rays never collide with rigs.
//!
//! # Seams for other tracks
//!
//! * Enemies (T10) report a [`crate::combat::Health`] view and [`WorldState::damage_entity`]
//!   (the one way to hurt an entity) routes them to the enemy damage model
//!   (`WorldState::damage_enemy`).
//! * Plain tiles broken by weapons go through [`crate::terrain::Terrain::chip_by`] and so show
//!   up in the tick's terrain journal and `terrain_ops` like drilled tiles, and drop their block
//!   (WP-DROPS, `spawn_tile_drop`) in `WorldState::weapon_broke_tile`, the single place this
//!   module breaks one.

use std::collections::BTreeSet;

use crate::{
    combat::{Damage, DamageOutcome, DamageSource, explosive_damage, explosive_damage_reach},
    entity::{Body, EntityId, EntityKind, EntityState, ProjectileState, TargetState},
    event::{GameplayEventKind, ImpactSurface, TickOutput},
    explosive::ExplosiveKind,
    hazard::PlacedBomb,
    ids::{PlayerId, SimulationTick},
    input::WeaponControl,
    physics::{line_clear, solid_at},
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{MineResult, TILE_SIZE, TileKind, TilePosition},
    weapon::{
        AUTO_AIM_CONE_COS, AUTO_AIM_RANGE, AmmoKind, Delivery, FUEL_PER_ENERGY, MAX_BATTERY_LEVEL,
        MAX_WEAPON_MK, MUZZLE_REACH, MUZZLE_RISE, WeaponKind, WeaponStats, aim_vector,
        battery_recharge, battery_upgrade_cost, pellet_direction, quantize_aim,
    },
};

use super::{WorldState, bombs::Blast};

/// Largest hit radius any damageable entity may have (spatial queries look this far).
pub const MAX_HIT_RADIUS: f32 = TILE_SIZE;
/// How close a shot must pass a live charge to set it off, in pixels.
const CHARGE_HIT_RADIUS: f32 = 9.0;
/// How close a harpoon must pass a drop to spear it, in pixels.
const DROP_HIT_RADIUS: f32 = 14.0;
/// Speed a harpoon reels speared loot in at, in pixels per second.
const HARPOON_REEL_SPEED: f32 = 460.0;
/// Ray march step, in pixels.
const RAY_STEP: f32 = 3.0;

/// One thing a ray or shot hit.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RayHit {
    Target(EntityId, f32, f32),
}

/// Where a hitscan ray ended and what it hit on the way.
#[derive(Clone, Debug, Default, PartialEq)]
struct RayCast {
    end: (f32, f32),
    /// Damageable entities hit, in order along the ray.
    hits: Vec<RayHit>,
    /// Plain tiles the ray punched through (railgun), in order.
    tunnelled: Vec<TilePosition>,
    /// The tile that stopped it, if any.
    stopped_by: Option<TilePosition>,
    /// Live chaining charges the ray passed close to (it sets them off), in order.
    charges: Vec<(EntityId, f32, f32)>,
}

/// Points along the hardpoint-to-muzzle segment, hardpoint first, muzzle last, at most
/// [`RAY_STEP`] apart.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "the muzzle sits a few pixels out: a handful of samples"
)]
fn point_blank_samples(origin: (f32, f32), muzzle: (f32, f32)) -> impl Iterator<Item = (f32, f32)> {
    let length = (muzzle.0 - origin.0).hypot(muzzle.1 - origin.1);
    let steps = ((length / RAY_STEP).ceil() as u32).max(1);
    (0..=steps).map(move |step| {
        let t = step as f32 / steps as f32;
        (
            (muzzle.0 - origin.0).mul_add(t, origin.0),
            (muzzle.1 - origin.1).mul_add(t, origin.1),
        )
    })
}

/// Distance from `point` to the segment `from`-`to`.
fn segment_distance(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let (sx, sy) = (to.0 - from.0, to.1 - from.1);
    let length_sq = sx.mul_add(sx, sy * sy);
    let t = if length_sq > 0.0 {
        ((point.0 - from.0).mul_add(sx, (point.1 - from.1) * sy) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (sx.mul_add(t, from.0) - point.0).hypot(sy.mul_add(t, from.1) - point.1)
/// What a ray found, for the Act II weapons (`world/sky_weapons.rs`).
pub(super) struct RayFind {
    /// Where the ray ended.
    pub end: (f32, f32),
    /// The first damageable entity it hit, and where.
    pub first_target: Option<(EntityId, f32, f32)>,
    /// The tile that stopped it, if any.
    pub stopped_by: Option<TilePosition>,
    /// Live chaining charges it passed close to.
    pub charges: Vec<(EntityId, f32, f32)>,
}

/// Whether a weapon can break `kind`: plain soil and rock only (ore, artifacts, POI fixtures,
/// hazards and bedrock are left alone).
/// Upward pop (pixels per second) of the block a weapon shot out of the rock.
const WEAPON_DROP_POP: f32 = 40.0;

const fn breakable_by_weapons(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "world pixels address terrain tiles"
)]
fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn px(value: f32) -> i32 {
    value.round() as i32
}

impl WorldState {
    /// Where `player_id`'s shots start for aim direction `aim`: the hardpoint above the rig's
    /// centre, pushed out along the aim.
    #[must_use]
    pub fn muzzle(&self, player_id: PlayerId, aim: (f32, f32)) -> Option<(f32, f32)> {
        let player = self.players.get(&player_id)?;
        Some((
            aim.0.mul_add(MUZZLE_REACH, player.x),
            aim.1.mul_add(MUZZLE_REACH, player.y - MUZZLE_RISE),
        ))
    }

    /// The target auto-aim would pick for `player_id` facing `facing`: the nearest damageable
    /// entity within [`AUTO_AIM_RANGE`] inside the cone around `facing`, in clear view and not
    /// hidden in smoke. `(id, x, y)`; presentation draws the lock-on reticle with it.
    #[must_use]
    pub fn auto_aim_target(
        &self,
        player_id: PlayerId,
        facing: (f32, f32),
    ) -> Option<(EntityId, f32, f32)> {
        let player = self.players.get(&player_id)?;
        let origin = (player.x, player.y - MUZZLE_RISE);
        let length = facing.0.hypot(facing.1);
        let facing = if length > f32::EPSILON {
            (facing.0 / length, facing.1 / length)
        } else {
            (1.0, 0.0)
        };
        let mut best: Option<(f32, EntityId, f32, f32)> = None;
        for id in self
            .entities
            .ids_in_radius(origin.0, origin.1, AUTO_AIM_RANGE)
        {
            let Some(entity) = self.entities.get(id) else {
                continue;
            };
            if entity.state.health().is_none() {
                continue;
            }
            let (tx, ty) = (entity.body.x, entity.body.y);
            let (dx, dy) = (tx - origin.0, ty - origin.1);
            let distance = dx.mul_add(dx, dy * dy).sqrt();
            if distance < 1.0 {
                continue;
            }
            let cos = dy.mul_add(facing.1, dx * facing.0) / distance;
            if cos < AUTO_AIM_CONE_COS
                || !line_clear(&self.terrain, origin.0, origin.1, tx, ty)
                || self.smoke_at(tx, ty)
            {
                continue;
            }
            if best.is_none_or(|(nearest, ..)| distance < nearest) {
                best = Some((distance, id, tx, ty));
            }
        }
        best.map(|(_, id, x, y)| (id, x, y))
    }

    /// The beam of `player_id`'s plasma cutter while its trigger is held: muzzle and end
    /// point (presentation). `None` when the cutter is not mounted or not firing.
    #[must_use]
    pub fn active_beam(&self, player_id: PlayerId) -> Option<((f32, f32), (f32, f32))> {
        let player = self.players.get(&player_id)?;
        let mounted = player.mounted_weapon?;
        if !mounted.kind.is_beam() || !player.weapon.firing {
            return None;
        }
        let aim = player.weapon.aim();
        let muzzle = self.muzzle(player_id, aim)?;
        let spec = mounted.kind.stats(mounted.mk);
        let ray = self.cast_ray(muzzle, aim, &spec);
        Some((muzzle, ray.end))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "recharge, aim and fire in tick order"
    )]
    /// Applies one tick of `player_id`'s weapon controls: recharges the capacitor from fuel,
    /// resolves the aim (free or auto) and fires the mounted weapon when the trigger allows.
    pub(super) fn update_weapon(
        &mut self,
        player_id: PlayerId,
        control: WeaponControl,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        // Recharge from fuel (only a rig that owns a weapon has a capacitor worth charging, so
        // unarmed rigs never pay fuel for it).
        let capacity = player.battery_capacity();
        if !player.weapon_rack.owned.is_empty()
            && player.weapon.energy < capacity
            && player.fuel > 0.0
        {
            let charge = (battery_recharge(player.weapon_rack.battery_level) * dt)
                .min(capacity - player.weapon.energy)
                .min(player.fuel / FUEL_PER_ENERGY);
            player.weapon.energy = quantize(player.weapon.energy + charge);
            player.fuel = quantize(charge.mul_add(-FUEL_PER_ENERGY, player.fuel).max(0.0));
        }
        player.weapon.energy = player.weapon.energy.min(capacity);
        let mounted = player.mounted_weapon;
        let held_before = player.weapon.firing;
        player.weapon.firing = control.fire && mounted.is_some();
        if !control.fire {
            player.weapon.pull_spent = false;
        }
        let Some(mounted) = mounted else {
            // Nothing on the hardpoint: no aim to resolve (and no auto-aim search each tick).
            return;
        };
        // Resolve the aim: free aim, or auto-aim around the facing.
        let requested = aim_vector(control.aim_x, control.aim_y);
        let aim = if control.auto_aim {
            let facing = requested.unwrap_or((1.0, 0.0));
            self.auto_aim_target(player_id, facing)
                .and_then(|(_, tx, ty)| {
                    let player = self.players.get(&player_id)?;
                    let (dx, dy) = (tx - player.x, ty - (player.y - MUZZLE_RISE));
                    quantize_aim(dx, dy).and_then(|(x, y)| aim_vector(x, y))
                })
                .unwrap_or(facing)
        } else {
            requested.unwrap_or_else(|| {
                self.players
                    .get(&player_id)
                    .map_or((1.0, 0.0), |player| player.weapon.aim())
            })
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if let Some((x, y)) = quantize_aim(aim.0, aim.1) {
            player.weapon.aim_x = x;
            player.weapon.aim_y = y;
        }
        let aim = player.weapon.aim();
        // Act II weapons with held behaviour (T21, `world/sky_weapons.rs`).
        match mounted.kind.stats(mounted.mk).delivery {
            Delivery::Field => {
                let spec = mounted.kind.stats(mounted.mk);
                self.gravity_gun(
                    player_id,
                    &spec,
                    (control.fire, held_before),
                    aim,
                    next_tick,
                    output,
                );
                return;
            }
            Delivery::Tether => {
                let spec = mounted.kind.stats(mounted.mk);
                self.tether_trigger(player_id, &spec, control.fire, aim, next_tick, output);
                return;
            }
            Delivery::Projectile | Delivery::Hitscan | Delivery::Chain => {}
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if !control.fire
            || next_tick < player.weapon.ready_tick
            || (!mounted.kind.automatic() && player.weapon.pull_spent)
        {
            return;
        }
        let spec = mounted.kind.stats(mounted.mk);
        let affordable = match spec.ammo {
            Some(ammo) => player.ammo(ammo) > 0,
            None => player.weapon.energy >= spec.energy,
        };
        if !affordable {
            if !held_before {
                output.push_event(GameplayEventKind::WeaponDry {
                    player_id,
                    weapon: mounted.kind,
                });
            }
            player.weapon.pull_spent = true;
            return;
        }
        match spec.ammo {
            Some(ammo) => {
                let _ = player.cargo_bay.remove(ammo.item(), 1);
            }
            None => player.weapon.energy = quantize(player.weapon.energy - spec.energy),
        }
        player.weapon.pull_spent = true;
        player.weapon.ready_tick = SimulationTick::new(
            next_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(spec.cooldown_millis).max(1)),
        );
        let Some(muzzle) = self.muzzle(player_id, aim) else {
            return;
        };
        let end = match spec.delivery {
            Delivery::Projectile => {
                self.launch_projectiles(
                    player_id,
                    (mounted.kind, mounted.mk),
                    (muzzle, aim),
                    next_tick,
                    output,
                );
                (
                    aim.0.mul_add(TILE_SIZE, muzzle.0),
                    aim.1.mul_add(TILE_SIZE, muzzle.1),
                )
            }
            Delivery::Hitscan => self.fire_ray(
                player_id,
                mounted.kind,
                &spec,
                (muzzle, aim),
                next_tick,
                output,
            ),
            Delivery::Chain => self.fire_arc(player_id, &spec, (muzzle, aim), next_tick, output),
            // Handled before the trigger checks above.
            Delivery::Field | Delivery::Tether => muzzle,
        };
        output.push_event(GameplayEventKind::WeaponFired {
            player_id,
            weapon: mounted.kind,
            x: px(muzzle.0),
            y: px(muzzle.1),
            end_x: px(end.0),
            end_y: px(end.1),
        });
    }

    /// Spawns the shot's projectiles at the muzzle. Point blank first: an enemy touching the
    /// rig can sit between the hardpoint and the muzzle (or around the muzzle itself), where a
    /// projectile spawned at the muzzle and flying outward would never meet it, so each pellet
    /// first sweeps the hardpoint-to-muzzle stretch of the aim line ([`Self::point_blank`]) and
    /// hits what it finds there; a spent pellet never spawns.
    pub(super) fn launch_projectiles(
        &mut self,
        player_id: PlayerId,
        (weapon, mk): (WeaponKind, u8),
        (muzzle, aim): ((f32, f32), (f32, f32)),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = weapon.stats(mk);
        let lifetime_millis = {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "flight times are a few seconds"
            )]
            let millis = (spec.range / spec.speed.max(1.0) * 1_000.0) as u64;
            millis.max(50)
        };
        let expires_at = SimulationTick::new(
            next_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(lifetime_millis).max(1)),
        );
        let origin = (
            aim.0.mul_add(-MUZZLE_REACH, muzzle.0),
            aim.1.mul_add(-MUZZLE_REACH, muzzle.1),
        );
        for index in 0..spec.pellets.max(1) {
            let (dx, dy) = pellet_direction(aim, index, spec.pellets);
            // Outer pellets fly a touch slower, so a spread reads as a cone, not a line.
            let offset = (f32::from(index) - f32::from(spec.pellets.saturating_sub(1)) / 2.0).abs();
            let speed = spec.speed * 0.04f32.mul_add(-offset, 1.0);
            let mut state = ProjectileState {
                weapon,
                mk,
                owner: player_id,
                travelled: 0.0,
                pierce_left: spec.pierce,
                last_hit: None,
            };
            let candidates = self.point_blank(origin, muzzle);
            let mut spent = false;
            if !candidates.is_empty() {
                for point in point_blank_samples(origin, muzzle) {
                    if self.projectile_hits_entities(
                        &mut state,
                        &spec,
                        point,
                        (dx, dy),
                        &candidates,
                        next_tick,
                        output,
                    ) == Some(true)
                    {
                        spent = true;
                        break;
                    }
                }
            }
            if spent {
                continue;
            }
            let body = Body::at(quantize(muzzle.0), quantize(muzzle.1))
                .moving(quantize(dx * speed), quantize(dy * speed));
            let _ = self.entities.spawn(
                self.simulation_tick,
                body,
                EntityState::Projectile(state),
                Some(expires_at),
            );
        }
    }

    /// One tick of a projectile's flight: gravity, swept substeps against terrain, damageable
    /// entities, live charges and (harpoons) loose drops.
    pub(super) fn advance_projectile(
        &mut self,
        id: EntityId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some((mut body, mut state)) =
            self.entities.get(id).and_then(|entity| match entity.state {
                EntityState::Projectile(state) => Some((entity.body, state)),
                _ => None,
            })
        else {
            return;
        };
        let spec = state.weapon.stats(state.mk);
        if spec.burst_radius > 0.0 {
            // Flak shells (T21) burst instead of hitting.
            self.advance_flak(id, body, state, &spec, output);
            return;
        }
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        // Arcing shots feel the sky's gravity fields (exactly 1.0 on the ground).
        let gravity = spec.gravity
            * crate::environment::environment_at(self.terrain.settings(), body.x, body.y).gravity;
        body.velocity_y = quantize(gravity.mul_add(dt, body.velocity_y));
        let speed = body.velocity_x.hypot(body.velocity_y);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "bounded substep count"
        )]
        let steps = ((speed * dt / RAY_STEP).ceil() as u32).clamp(1, 96);
        #[allow(clippy::cast_precision_loss, reason = "at most 96 substeps")]
        let step = dt / steps as f32;
        let direction = if speed > 0.0 {
            (body.velocity_x / speed, body.velocity_y / speed)
        } else {
            (1.0, 0.0)
        };
        let candidates: Vec<EntityId> = {
            let reach = speed.mul_add(dt, MAX_HIT_RADIUS);
            self.entities
                .ids_in_radius(body.x, body.y, reach)
                .into_iter()
                .filter(|other| *other != id)
                .collect()
        };
        let mut done = false;
        for _ in 0..steps {
            let (last_x, last_y) = (body.x, body.y);
            body.x = body.velocity_x.mul_add(step, body.x);
            body.y = body.velocity_y.mul_add(step, body.y);
            state.travelled = speed.mul_add(step, state.travelled);
            if let Some(stop) = self.projectile_hits_entities(
                &mut state,
                &spec,
                (body.x, body.y),
                direction,
                &candidates,
                next_tick,
                output,
            ) {
                done = stop;
                if done {
                    break;
                }
            }
            if solid_at(&self.terrain, body.x, body.y) {
                self.projectile_hits_terrain(
                    &state,
                    &spec,
                    (last_x, last_y),
                    (body.x, body.y),
                    next_tick,
                    output,
                );
                done = true;
                break;
            }
            if state.travelled >= spec.range {
                done = true;
                break;
            }
        }
        if done {
            let _ = self.entities.despawn(id);
            return;
        }
        body.x = quantize(body.x);
        body.y = quantize(body.y);
        state.travelled = quantize(state.travelled);
        let _ = self.entities.update(id, |entity| {
            entity.body = body;
            entity.state = EntityState::Projectile(state);
        });
    }

    fn projectile_hits_terrain(
        &mut self,
        state: &ProjectileState,
        spec: &WeaponStats,
        before: (f32, f32),
        at: (f32, f32),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if let Some(blast) = spec.blast {
            self.spawn_warhead(blast, state.owner, before, next_tick);
        } else if spec.terrain_bite > 0 {
            let _ = self.weapon_broke_tile(tile_at(at.0, at.1), spec.terrain_bite);
        }
        output.push_event(GameplayEventKind::ProjectileImpact {
            weapon: state.weapon,
            x: px(at.0),
            y: px(at.1),
            surface: ImpactSurface::Terrain,
        });
    }

    /// Checks one substep position against nearby entities. Returns `Some(true)` when the
    /// projectile is spent, `Some(false)` when it hit something and flies on (piercing), `None`
    /// when it hit nothing.
    #[allow(clippy::too_many_arguments, reason = "one substep's full context")]
    fn projectile_hits_entities(
        &mut self,
        state: &mut ProjectileState,
        spec: &WeaponStats,
        at: (f32, f32),
        direction: (f32, f32),
        candidates: &[EntityId],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) -> Option<bool> {
        for other in candidates {
            let Some(entity) = self.entities.get(*other) else {
                continue;
            };
            let (dx, dy) = (entity.body.x - at.0, entity.body.y - at.1);
            let distance = dx.mul_add(dx, dy * dy).sqrt();
            match entity.state {
                state_with_health if state_with_health.health().is_some() => {
                    let radius = state_with_health
                        .health()
                        .map_or(0.0, |health| health.radius);
                    if distance > radius || state.last_hit == Some(*other) {
                        continue;
                    }
                    let (tx, ty) = (entity.body.x, entity.body.y);
                    if let Some(blast) = spec.blast {
                        self.spawn_warhead(blast, state.owner, at, next_tick);
                    }
                    let knockback = if spec.pulls {
                        self.reel_direction(state.owner, (tx, ty), spec.knockback.max(300.0))
                    } else {
                        (direction.0 * spec.knockback, direction.1 * spec.knockback)
                    };
                    let _ = self.damage_entity(
                        *other,
                        Damage {
                            amount: spec.damage,
                            source: DamageSource::Weapon {
                                player: state.owner,
                                weapon: state.weapon,
                            },
                            armor_piercing: spec.armor_piercing,
                            knockback,
                        },
                        output,
                    );
                    output.push_event(GameplayEventKind::ProjectileImpact {
                        weapon: state.weapon,
                        x: px(at.0),
                        y: px(at.1),
                        surface: ImpactSurface::Target,
                    });
                    state.last_hit = Some(*other);
                    if state.pierce_left == 0 || spec.blast.is_some() {
                        return Some(true);
                    }
                    state.pierce_left -= 1;
                    return Some(false);
                }
                EntityState::Charge(charge)
                    if !spec.pulls
                        && distance <= CHARGE_HIT_RADIUS
                        && charge.kind.chains()
                        && charge.stage == 0 =>
                {
                    // Shooting a charge sets it off.
                    self.trigger_charge(*other, next_tick);
                    output.push_event(GameplayEventKind::ProjectileImpact {
                        weapon: state.weapon,
                        x: px(at.0),
                        y: px(at.1),
                        surface: ImpactSurface::Charge,
                    });
                    return Some(true);
                }
                EntityState::Drop(_) if spec.pulls && distance <= DROP_HIT_RADIUS => {
                    let (vx, vy) = self.reel_direction(
                        state.owner,
                        (entity.body.x, entity.body.y),
                        HARPOON_REEL_SPEED,
                    );
                    let _ = self.entities.update(*other, |entity| {
                        entity.body.velocity_x = quantize(vx);
                        entity.body.velocity_y = quantize(vy - 120.0);
                        entity.body.wake();
                    });
                    output.push_event(GameplayEventKind::ProjectileImpact {
                        weapon: state.weapon,
                        x: px(at.0),
                        y: px(at.1),
                        surface: ImpactSurface::Loot,
                    });
                    return Some(true);
                }
                _ => {}
            }
        }
        None
    }

    /// Marches a ray like a hitscan weapon's (read-only) and reports what it found (the Act
    /// II weapons' view of [`Self::cast_ray`]).
    pub(super) fn ray_cast(
        &self,
        muzzle: (f32, f32),
        aim: (f32, f32),
        spec: &WeaponStats,
    ) -> RayFind {
        let ray = self.cast_ray(muzzle, aim, spec);
        RayFind {
            end: ray.end,
            first_target: ray
                .hits
                .first()
                .map(|RayHit::Target(id, x, y)| (*id, *x, *y)),
            stopped_by: ray.stopped_by,
            charges: ray.charges,
        }
    }

    /// Sets off live charge `id` in the next bomb pass (a shot, beam or slug hit it).
    pub(super) fn trigger_charge(&mut self, id: EntityId, next_tick: SimulationTick) {
        let _ = self.entities.update(id, |entity| {
            if let EntityState::Charge(charge) = &mut entity.state {
                charge.chain_triggered = true;
                charge.detonation_tick = charge.detonation_tick.min(next_tick);
            }
        });
    }

    /// A velocity of `speed` from `from` toward `owner`'s rig (a harpoon reeling in).
    fn reel_direction(&self, owner: PlayerId, from: (f32, f32), speed: f32) -> (f32, f32) {
        let Some(player) = self.players.get(&owner) else {
            return (0.0, 0.0);
        };
        let (dx, dy) = (player.x - from.0, player.y - from.1);
        let distance = dx.mul_add(dx, dy * dy).sqrt();
        if distance < 1.0 {
            return (0.0, 0.0);
        }
        (dx / distance * speed, dy / distance * speed)
    }

    /// A rocket's warhead: a charge that goes off in the next bomb pass (chain reactions,
    /// pushes and blast damage come with it).
    fn spawn_warhead(
        &mut self,
        kind: ExplosiveKind,
        owner: PlayerId,
        at: (f32, f32),
        next_tick: SimulationTick,
    ) {
        let (x, y) = (quantize(at.0), quantize(at.1));
        let bomb = PlacedBomb {
            id: self.next_bomb_id,
            x,
            y,
            previous_x: x,
            previous_y: y,
            detonation_tick: next_tick,
            kind,
            owner: Some(owner),
            ..PlacedBomb::default()
        };
        self.insert_charge(bomb);
    }

    /// Breaks (or chips) a plain tile hit by a weapon. The one place weapons change terrain;
    /// the change lands in the tick's terrain journal like a drilled tile. Returns whether the
    /// tile was removed.
    pub(super) fn weapon_broke_tile(&mut self, tile: TilePosition, bite: u8) -> bool {
        let Some(kind) = self
            .terrain
            .tile(tile)
            .map(|tile| tile.kind)
            .filter(|kind| breakable_by_weapons(*kind))
        else {
            return false;
        };
        let broke = matches!(
            self.terrain.chip_by(tile, bite, false),
            MineResult::Mined(_)
        );
        if broke {
            // WP-DROPS: a shot-out tile drops its block like any other broken tile, popping
            // gently up so it lands on the new floor.
            let _ = self.spawn_tile_drop(tile, kind, (0.0, -WEAPON_DROP_POP));
        }
        broke
    }

    /// Live chaining charges (ones a hit would set off) near the box spanned by `from` and
    /// `to`: `(id, x, y)`.
    fn live_charges_between(&self, from: (f32, f32), to: (f32, f32)) -> Vec<(EntityId, f32, f32)> {
        self.entities
            .ids_in_rect(
                from.0.min(to.0) - CHARGE_HIT_RADIUS,
                from.1.min(to.1) - CHARGE_HIT_RADIUS,
                from.0.max(to.0) + CHARGE_HIT_RADIUS,
                from.1.max(to.1) + CHARGE_HIT_RADIUS,
            )
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                match entity.state {
                    EntityState::Charge(charge) if charge.kind.chains() && charge.stage == 0 => {
                        Some((id, entity.body.x, entity.body.y))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Damageable entities whose hit circle the hardpoint-to-muzzle segment `origin`-`muzzle`
    /// passes through (point-blank targets: an enemy touching the rig), in id order.
    fn point_blank(&self, origin: (f32, f32), muzzle: (f32, f32)) -> Vec<EntityId> {
        self.entities
            .ids_in_radius(origin.0, origin.1, MUZZLE_REACH + MAX_HIT_RADIUS)
            .into_iter()
            .filter(|id| {
                self.entities.get(*id).is_some_and(|entity| {
                    entity.state.health().is_some_and(|health| {
                        segment_distance((entity.body.x, entity.body.y), origin, muzzle)
                            <= health.radius
                    })
                })
            })
            .collect()
    }

    /// Marches a hitscan ray from `muzzle` along `aim` (read-only): through damageable entities
    /// (up to the weapon's pierce), through plain tiles it can tunnel (railgun), stopping at
    /// anything else or at its range.
    #[allow(
        clippy::too_many_lines,
        reason = "one march: point blank, then out to range"
    )]
    fn cast_ray(&self, muzzle: (f32, f32), aim: (f32, f32), spec: &WeaponStats) -> RayCast {
        let range = spec.range;
        let far = (
            aim.0.mul_add(range, muzzle.0),
            aim.1.mul_add(range, muzzle.1),
        );
        // The hardpoint behind the muzzle: point-blank targets between it and the muzzle are hit
        // first (see `launch_projectiles`).
        let origin = (
            aim.0.mul_add(-MUZZLE_REACH, muzzle.0),
            aim.1.mul_add(-MUZZLE_REACH, muzzle.1),
        );
        let candidates: Vec<(EntityId, f32, f32, f32)> = self
            .entities
            .ids_in_rect(
                origin.0.min(far.0) - MAX_HIT_RADIUS,
                origin.1.min(far.1) - MAX_HIT_RADIUS,
                origin.0.max(far.0) + MAX_HIT_RADIUS,
                origin.1.max(far.1) + MAX_HIT_RADIUS,
            )
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                let health = entity.state.health()?;
                Some((id, entity.body.x, entity.body.y, health.radius))
            })
            .collect();
        let charges = self.live_charges_between(muzzle, far);
        let mut ray = RayCast::default();
        let mut hit: BTreeSet<EntityId> = BTreeSet::new();
        let mut tunnel_left = spec.tunnel;
        let mut current_tunnel: Option<TilePosition> = None;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "ray lengths are a few hundred pixels"
        )]
        let steps = (range / RAY_STEP).ceil() as u32;
        let mut end = muzzle;
        for point in point_blank_samples(origin, muzzle) {
            for (id, x, y, radius) in &candidates {
                let (dx, dy) = (x - point.0, y - point.1);
                if !hit.contains(id)
                    && ray.hits.len() <= usize::from(spec.pierce)
                    && dx.mul_add(dx, dy * dy) <= radius * radius
                {
                    hit.insert(*id);
                    ray.hits.push(RayHit::Target(*id, point.0, point.1));
                }
            }
        }
        let point_blank_spent = ray.hits.len() > usize::from(spec.pierce);
        for step in 1..=steps {
            if point_blank_spent {
                break;
            }
            #[allow(clippy::cast_precision_loss, reason = "bounded step count")]
            let along = (step as f32 * RAY_STEP).min(range);
            let point = (
                aim.0.mul_add(along, muzzle.0),
                aim.1.mul_add(along, muzzle.1),
            );
            end = point;
            let tile = tile_at(point.0, point.1);
            if solid_at(&self.terrain, point.0, point.1) {
                if current_tunnel == Some(tile) {
                    continue;
                }
                let breakable = self
                    .terrain
                    .tile(tile)
                    .is_some_and(|found| breakable_by_weapons(found.kind));
                if breakable && tunnel_left > 0 {
                    tunnel_left -= 1;
                    current_tunnel = Some(tile);
                    ray.tunnelled.push(tile);
                    continue;
                }
                ray.stopped_by = Some(tile);
                break;
            }
            for (id, x, y) in &charges {
                let (dx, dy) = (x - point.0, y - point.1);
                if dx.mul_add(dx, dy * dy) <= CHARGE_HIT_RADIUS * CHARGE_HIT_RADIUS
                    && !ray.charges.iter().any(|(other, ..)| other == id)
                {
                    ray.charges.push((*id, point.0, point.1));
                }
            }
            let mut spent = false;
            for (id, x, y, radius) in &candidates {
                if hit.contains(id) {
                    continue;
                }
                let (dx, dy) = (x - point.0, y - point.1);
                if dx.mul_add(dx, dy * dy) <= radius * radius {
                    hit.insert(*id);
                    ray.hits.push(RayHit::Target(*id, point.0, point.1));
                    if ray.hits.len() > usize::from(spec.pierce) {
                        spent = true;
                        break;
                    }
                }
            }
            if spent {
                break;
            }
        }
        ray.end = end;
        ray
    }

    /// Fires a hitscan weapon: damages what the ray hit, breaks what it tunnelled through and
    /// bites the tile that stopped it. Returns the ray's end.
    fn fire_ray(
        &mut self,
        player_id: PlayerId,
        weapon: WeaponKind,
        spec: &WeaponStats,
        (muzzle, aim): ((f32, f32), (f32, f32)),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) -> (f32, f32) {
        let ray = self.cast_ray(muzzle, aim, spec);
        for (id, x, y) in &ray.charges {
            // A beam or slug through a charge sets it off, like a shot does.
            self.trigger_charge(*id, next_tick);
            output.push_event(GameplayEventKind::ProjectileImpact {
                weapon,
                x: px(*x),
                y: px(*y),
                surface: ImpactSurface::Charge,
            });
        }
        for hit in &ray.hits {
            let RayHit::Target(id, x, y) = *hit;
            let _ = self.damage_entity(
                id,
                Damage {
                    amount: spec.damage,
                    source: DamageSource::Weapon {
                        player: player_id,
                        weapon,
                    },
                    armor_piercing: spec.armor_piercing,
                    knockback: (aim.0 * spec.knockback, aim.1 * spec.knockback),
                },
                output,
            );
            output.push_event(GameplayEventKind::ProjectileImpact {
                weapon,
                x: px(x),
                y: px(y),
                surface: ImpactSurface::Target,
            });
        }
        for tile in &ray.tunnelled {
            let _ = self.weapon_broke_tile(*tile, u8::MAX);
        }
        if let Some(tile) = ray.stopped_by {
            if spec.terrain_bite > 0 {
                let _ = self.weapon_broke_tile(tile, spec.terrain_bite.min(2));
            }
            output.push_event(GameplayEventKind::ProjectileImpact {
                weapon,
                x: px(ray.end.0),
                y: px(ray.end.1),
                surface: ImpactSurface::Terrain,
            });
        }
        ray.end
    }

    /// Hurts entity `id` (the damage API, see [`crate::combat`]): armour and armour piercing
    /// apply, the body is knocked back if its kind feels pushes, events are emitted and a
    /// destroyed entity is despawned. Entities without health are ignored.
    pub fn damage_entity(
        &mut self,
        id: EntityId,
        damage: Damage,
        output: &mut TickOutput,
    ) -> DamageOutcome {
        let Some(entity) = self.entities.get(id) else {
            return DamageOutcome::Ignored;
        };
        if let EntityState::Pilot(_) = entity.state {
            // Pilots on foot have their own health model (`world/pilot.rs`); weapons never
            // find them (friendly fire is off), but other systems may hurt them here.
            if damage.amount <= 0.0 || !self.damage_pilot(id, damage.amount, output) {
                return DamageOutcome::Ignored;
            }
            return match self.entities.get(id).map(|entity| entity.state) {
                Some(EntityState::Pilot(state)) => DamageOutcome::Hurt {
                    dealt: damage.amount,
                    remaining: state.health,
                },
                _ => DamageOutcome::Destroyed {
                    dealt: damage.amount,
                },
            };
        }
        if entity.state.health().is_none() || damage.amount <= 0.0 {
            return DamageOutcome::Ignored;
        }
        if let EntityState::Enemy(_) = entity.state {
            return self.damage_enemy_from(id, damage, output);
        }
        let pushes = entity.kind().feels_blasts();
        let result = self.entities.update(id, |entity| {
            if pushes {
                entity.body.velocity_x = quantize(entity.body.velocity_x + damage.knockback.0);
                entity.body.velocity_y = quantize(entity.body.velocity_y + damage.knockback.1);
                entity.body.wake();
            }
            let (x, y) = (entity.body.x, entity.body.y);
            entity.state.health_mut().map(|health| {
                let dealt = health.apply(damage.amount, damage.armor_piercing);
                (dealt, health.current, health.is_dead(), x, y)
            })
        });
        let Some(Some((dealt, remaining, dead, x, y))) = result else {
            return DamageOutcome::Ignored;
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "hit sizes are small positive numbers"
        )]
        let amount = dealt.round().clamp(0.0, f32::from(u16::MAX)) as u16;
        output.push_event(GameplayEventKind::EntityDamaged {
            entity: id.get(),
            x: px(x),
            y: px(y),
            amount,
        });
        if dead {
            let _ = self.entities.despawn(id);
            output.push_event(GameplayEventKind::EntityDestroyed {
                entity: id.get(),
                x: px(x),
                y: px(y),
                by: damage.source.player(),
            });
            return DamageOutcome::Destroyed { dealt };
        }
        DamageOutcome::Hurt { dealt, remaining }
    }

    /// One combat hit on an enemy, through the enemy damage model (WP-ENEMIES): the hit's
    /// source picks its [`DamageKind`] (railgun slugs and harpoons pierce, cutter beams burn,
    /// rockets blast, the rest is kinetic; armour piercing above one half also pierces) and the
    /// outcome maps back onto [`DamageOutcome`].
    #[allow(clippy::match_same_arms, reason = "one arm per weapon family")]
    fn damage_enemy_from(
        &mut self,
        id: EntityId,
        damage: Damage,
        output: &mut TickOutput,
    ) -> DamageOutcome {
        use crate::enemy::{self, DamageKind};
        let kind = match damage.source {
            DamageSource::Weapon { weapon, .. } => match weapon {
                WeaponKind::Railgun | WeaponKind::Harpoon | WeaponKind::VoidLance => {
                    DamageKind::Piercing
                }
                WeaponKind::PlasmaCutter | WeaponKind::LunarCutter => DamageKind::Fire,
                WeaponKind::RocketLauncher | WeaponKind::FlakCannon | WeaponKind::LowGMortar => {
                    DamageKind::Explosive
                }
                WeaponKind::RivetGun
                | WeaponKind::ScatterBlaster
                | WeaponKind::GravityGun
                | WeaponKind::Sidearm
                | WeaponKind::Rifle
                | WeaponKind::Shotgun
                | WeaponKind::Wrench
                | WeaponKind::HandPick => DamageKind::Kinetic,
                WeaponKind::ArcRifle => DamageKind::Shock,
                WeaponKind::TetherHarpoon => DamageKind::Piercing,
            },
            DamageSource::Explosive { kind, .. } => match kind {
                ExplosiveKind::Incendiary => DamageKind::Fire,
                ExplosiveKind::Emp => DamageKind::Emp,
                _ => DamageKind::Explosive,
            },
            DamageSource::Environment => DamageKind::Kinetic,
        };
        let kind = if kind == DamageKind::Kinetic && damage.armor_piercing > 0.5 {
            DamageKind::Piercing
        } else {
            kind
        };
        let source = damage
            .source
            .player()
            .map_or(enemy::DamageSource::World, enemy::DamageSource::Player);
        let hit = enemy::Damage::new(damage.amount, kind, source)
            .with_impulse(damage.knockback.0, damage.knockback.1);
        match self.damage_enemy(id, hit, output) {
            enemy::DamageOutcome::Ignored => DamageOutcome::Ignored,
            enemy::DamageOutcome::Hurt { dealt } => DamageOutcome::Hurt {
                dealt,
                remaining: self
                    .entities
                    .get(id)
                    .and_then(|entity| entity.state.health())
                    .map_or(0.0, |health| health.current),
            },
            enemy::DamageOutcome::Killed { dealt } => DamageOutcome::Destroyed { dealt },
        }
    }

    /// Blast damage to damageable entities from this tick's detonations (by distance, with line
    /// of sight on the pre-blast terrain).
    pub(super) fn damage_from_blasts(&mut self, blasts: &[Blast], output: &mut TickOutput) {
        for blast in blasts {
            let reach = explosive_damage_reach(blast.kind);
            for id in self.entities.ids_in_radius(blast.x, blast.y, reach) {
                let Some(entity) = self.entities.get(id) else {
                    continue;
                };
                // Enemies take blasts in `enemies_feel_blasts` (their own falloff, pushes,
                // EMP stuns and scares), so they are skipped here.
                if entity.kind() == EntityKind::Enemy {
                    continue;
                }
                let Some(health) = entity.state.health() else {
                    continue;
                };
                let (dx, dy) = (entity.body.x - blast.x, entity.body.y - blast.y);
                let distance = dx.mul_add(dx, dy * dy).sqrt();
                let peak = explosive_damage(blast.kind, health.machine);
                if peak <= 0.0
                    || distance >= reach
                    || !line_clear(
                        &self.terrain,
                        blast.x,
                        blast.y,
                        entity.body.x,
                        entity.body.y,
                    )
                {
                    continue;
                }
                let amount = peak * (1.0 - distance / reach);
                let _ = self.damage_entity(
                    id,
                    Damage::new(
                        amount,
                        DamageSource::Explosive {
                            owner: blast.owner,
                            kind: blast.kind,
                        },
                    ),
                    output,
                );
            }
        }
    }

    /// Spawns a practice target (a damageable dummy on a stand) at `(x, y)`; scenarios, tests
    /// and capture scenes use it, and it is the reference for damageable kinds.
    pub fn spawn_target_dummy(&mut self, x: f32, y: f32, max_health: f32, armor: f32) -> EntityId {
        self.entities.spawn(
            self.simulation_tick,
            Body::at(x, y),
            EntityState::Target(TargetState {
                health: crate::combat::Health::new(max_health, armor, 13.0),
            }),
            None,
        )
    }

    /// Resolves [`crate::PlayerCommand::MountWeapon`].
    pub(super) fn mount_weapon(
        &mut self,
        player_id: PlayerId,
        weapon: Option<WeaponKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if player.mount_weapon(weapon) {
            player.weapon.firing = false;
            player.weapon.pull_spent = true;
            PlayerScopedCommandOutcome::Applied
        } else {
            PlayerScopedCommandOutcome::IgnoredUnavailable
        }
    }

    /// Buys `weapon` at the shack (stocked from its shack level).
    pub(super) fn buy_weapon(
        &mut self,
        player_id: PlayerId,
        weapon: WeaponKind,
        shack_level: u8,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let cost = weapon.cost();
        // Act II weapons (T21) need a world with sky content and sky materials.
        let sky = self.terrain.settings().generator.has_sky_content();
        let materials = weapon.materials();
        if shack_level < weapon.shack_level()
            || player.weapon_rack.owned.contains_key(&weapon)
            || player.credits < cost
            || (weapon.act_two() && !sky)
            || materials
                .iter()
                .any(|(material, count)| player.cargo_bay.material(*material) < *count)
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= cost;
        for (material, count) in materials {
            let _spent = player.cargo_bay.remove(*material, *count);
        }
        let _ = player.receive_weapon(weapon);
        if weapon == WeaponKind::RocketLauncher {
            // A starter load so the launcher fires straight away.
            player.cargo_bay.add(AmmoKind::Rocket.item(), 4);
        }
        self.apply_quest_rebate(player_id, crate::quest::QuestGiver::Nix, cost);
        PlayerScopedCommandOutcome::Applied
    }

    /// Upgrades an owned weapon one tier.
    pub(super) fn upgrade_weapon(
        &mut self,
        player_id: PlayerId,
        weapon: WeaponKind,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let Some(mk) = player.weapon_rack.mk(weapon) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        // Moon weapons are tuned at the lunar colony (`crate::moon::ColonyOffer`).
        if mk >= MAX_WEAPON_MK || weapon.is_lunar() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let cost = weapon.upgrade_cost(mk + 1);
        if player.credits < cost {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= cost;
        player.weapon_rack.owned.insert(weapon, mk + 1);
        if let Some(mounted) = player.mounted_weapon.as_mut()
            && mounted.kind == weapon
        {
            mounted.mk = mk + 1;
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Upgrades the capacitor one level (topping it up).
    pub(super) fn upgrade_battery(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let level = player.weapon_rack.battery_level;
        if level >= MAX_BATTERY_LEVEL {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let cost = battery_upgrade_cost(level + 1);
        if player.credits < cost {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= cost;
        player.weapon_rack.battery_level = level + 1;
        player.weapon.energy = player.battery_capacity();
        PlayerScopedCommandOutcome::Applied
    }

    /// Buys a pack of `ammo`.
    pub(super) fn buy_ammo(
        &mut self,
        player_id: PlayerId,
        ammo: AmmoKind,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let (count, cost) = ammo.pack();
        if player.credits < cost {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= cost;
        player.cargo_bay.add(ammo.item(), count);
        self.apply_quest_rebate(player_id, crate::quest::QuestGiver::Nix, cost);
        PlayerScopedCommandOutcome::Applied
    }
}
