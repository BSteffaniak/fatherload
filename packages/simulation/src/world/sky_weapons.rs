//! Act II weapons for the rig (Wave 4 T21, expansion plan WP-SKY "weapons"): the arc rifle,
//! the gravity gun, the tether harpoon and the flak cannon. Tables live in [`crate::weapon`];
//! this module is their world side, called from `world/weapons.rs`'s trigger pass and
//! projectile flight. Everything hurts through [`WorldState::damage_entity`] (enemies map the
//! arc to [`crate::enemy::DamageKind::Shock`] and flak to explosive), so resistances, kill
//! credit and loot are the shared rules.
//!
//! * **Arc rifle** ([`Delivery::Chain`]): an instant ray to the first target, then the arc
//!   leaps to the nearest unhit target within [`WeaponStats::chain_reach`] in clear line, up
//!   to [`WeaponStats::chains`] times, losing a quarter each leap (`ArcChained` per link).
//! * **Gravity gun** ([`Delivery::Field`]): while held (draining [`WeaponStats::energy`] a
//!   second) it pulls drops, live charges, boulders and ordinary enemies inside its cone toward
//!   a hold point in front of the muzzle (held enemies hang helpless), and every
//!   [`RIP_MILLIS`] it rips a loose block (soft soil, clay, sand, turf) out of the rock it
//!   points at, as a drop it then holds. Letting go flings everything held along the aim
//!   (`GravityThrow`); thrown enemies take [`WeaponStats::damage`].
//! * **Tether harpoon** ([`Delivery::Tether`]): a fresh pull hooks the first solid tile within
//!   range (`TetherAttached`); while the trigger is held the rig cannot get farther than the
//!   line from the hook ([`WorldState::apply_tether`], after movement: it swings like a
//!   pendulum, and holding thrust reels the line in), and letting go (or the hooked tile
//!   breaking) frees it (`TetherReleased`). The line is per-tick weapon state
//!   ([`crate::weapon::RigWeaponState::tether`]), so it is predicted like movement.
//! * **Flak cannon** (a projectile with [`WeaponStats::burst_radius`]): the shell bursts when
//!   an enemy comes within half its burst radius, on terrain, or at the end of its range,
//!   hurting every damageable entity in the burst with falloff (`FlakBurst`). Airships take
//!   explosive damage nearly double.

use std::collections::BTreeSet;

use crate::{
    combat::{Damage, DamageSource},
    enemy::EnemyMode,
    entity::{Body, EntityId, EntityKind, EntityState, ProjectileState},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    item::BlockKind,
    physics::{line_clear, solid_at},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition, is_indestructible, tile_hardness},
    weapon::{MUZZLE_REACH, Tether, WeaponKind, WeaponStats},
};

use super::{WorldState, movement::authoritative_player_collides};

/// How often a held gravity gun rips a loose block out of the rock.
pub const RIP_MILLIS: u64 = 600;
/// Distance from the muzzle to the gravity gun's hold point, pixels.
pub const HOLD_REACH: f32 = TILE_SIZE * 1.6;
/// Things within this of the hold point are held (and flung on release), pixels.
pub const HOLD_RADIUS: f32 = TILE_SIZE * 1.6;
/// Fastest a held thing is pulled, pixels per second.
const PULL_SPEED: f32 = 420.0;
/// Speed the tether reels in at while thrust is held, pixels per second.
pub const REEL_SPEED: f32 = 150.0;
/// The shortest a tether reels to, pixels.
pub const MIN_TETHER: f32 = TILE_SIZE * 1.5;
/// A held enemy hangs helpless this long after the last tick it was held.
const HOLD_STUN_MILLIS: u64 = 250;
/// A thrown enemy stays helpless this long.
const THROW_STUN_MILLIS: u64 = 600;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn px(value: f32) -> i32 {
    value.round() as i32
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

/// Whether a gravity gun can rip `kind` out of the rock: plain soft ground that breaks into a
/// block (dirt, clay, sand, mud, sky turf and soil).
const fn loose(kind: TileKind) -> bool {
    BlockKind::from_tile(kind).is_some() && tile_hardness(kind) <= 2 && !is_indestructible(kind)
}

impl WorldState {
    /// The arc rifle's shot: the first target along the ray, then the chain (see the module
    /// docs). Returns where the ray ended (the first hit, or its range or the rock).
    pub(super) fn fire_arc(
        &mut self,
        player_id: PlayerId,
        spec: &WeaponStats,
        (muzzle, aim): ((f32, f32), (f32, f32)),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) -> (f32, f32) {
        let weapon = WeaponKind::ArcRifle;
        let ray = self.ray_cast(muzzle, aim, spec);
        for (id, ..) in &ray.charges {
            self.trigger_charge(*id, next_tick);
        }
        let Some((first, hit_x, hit_y)) = ray.first_target else {
            return ray.end;
        };
        let mut visited = BTreeSet::from([first]);
        let mut from = muzzle;
        let mut current = Some((first, (hit_x, hit_y)));
        let mut amount = spec.damage;
        let mut links = 0_u8;
        while let Some((id, at)) = current {
            let (dx, dy) = (at.0 - from.0, at.1 - from.1);
            let length = dx.hypot(dy).max(1.0);
            output.push_event(GameplayEventKind::ArcChained {
                player_id,
                x: px(from.0),
                y: px(from.1),
                end_x: px(at.0),
                end_y: px(at.1),
            });
            let _ = self.damage_entity(
                id,
                Damage {
                    amount,
                    source: DamageSource::Weapon {
                        player: player_id,
                        weapon,
                    },
                    armor_piercing: 0.0,
                    knockback: (dx / length * spec.knockback, dy / length * spec.knockback),
                },
                output,
            );
            if links >= spec.chains {
                break;
            }
            links += 1;
            amount *= 0.75;
            from = at;
            current = self
                .entities
                .ids_in_radius(at.0, at.1, spec.chain_reach)
                .into_iter()
                .filter(|other| !visited.contains(other))
                .filter_map(|other| {
                    let entity = self.entities.get(other)?;
                    entity.state.health()?;
                    let (x, y) = (entity.body.x, entity.body.y);
                    let distance = (x - at.0).hypot(y - at.1);
                    (distance <= spec.chain_reach && line_clear(&self.terrain, at.0, at.1, x, y))
                        .then_some((distance, other, (x, y)))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                .map(|(_, other, at)| (other, at));
            if let Some((next, _)) = current {
                visited.insert(next);
            }
        }
        (hit_x, hit_y)
    }

    /// One tick of a held (or just released) gravity gun (see the module docs).
    #[allow(clippy::too_many_arguments, reason = "the trigger's whole context")]
    pub(super) fn gravity_gun(
        &mut self,
        player_id: PlayerId,
        spec: &WeaponStats,
        (fire, held_before): (bool, bool),
        aim: (f32, f32),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(muzzle) = self.muzzle(player_id, aim) else {
            return;
        };
        let hold = (
            aim.0.mul_add(HOLD_REACH, muzzle.0),
            aim.1.mul_add(HOLD_REACH, muzzle.1),
        );
        if !fire {
            if held_before {
                self.gravity_throw(player_id, spec, aim, hold, output);
            }
            return;
        }
        let dt = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let drain = spec.energy * dt;
        if player.weapon.energy < drain {
            if !held_before {
                output.push_event(GameplayEventKind::WeaponDry {
                    player_id,
                    weapon: WeaponKind::GravityGun,
                });
            }
            player.weapon.firing = false;
            return;
        }
        player.weapon.energy = quantize(player.weapon.energy - drain);
        let rip = next_tick >= player.weapon.ready_tick;
        if rip {
            player.weapon.ready_tick = SimulationTick::new(
                next_tick
                    .get()
                    .saturating_add(self.timing.ticks_for_millis(RIP_MILLIS).max(1)),
            );
        }
        if !held_before {
            output.push_event(GameplayEventKind::WeaponFired {
                player_id,
                weapon: WeaponKind::GravityGun,
                x: px(muzzle.0),
                y: px(muzzle.1),
                end_x: px(hold.0),
                end_y: px(hold.1),
            });
        }
        if rip {
            self.rip_loose_block(muzzle, aim, spec.range, hold);
        }
        let stun_until = SimulationTick::new(
            next_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(HOLD_STUN_MILLIS).max(1)),
        );
        for id in self.gravity_targets(muzzle, aim, spec.range, hold) {
            let _ = self.entities.update(id, |entity| {
                let (dx, dy) = (hold.0 - entity.body.x, hold.1 - entity.body.y);
                let distance = dx.hypot(dy).max(1.0);
                let speed = (distance * 6.0).min(PULL_SPEED);
                let (want_x, want_y) = (dx / distance * speed, dy / distance * speed);
                entity.body.velocity_x = quantize(
                    (want_x - entity.body.velocity_x).mul_add(0.35, entity.body.velocity_x),
                );
                entity.body.velocity_y = quantize(
                    (want_y - entity.body.velocity_y).mul_add(0.35, entity.body.velocity_y),
                );
                entity.body.wake();
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.mode = EnemyMode::Stunned;
                    state.mode_until = state.mode_until.max(stun_until);
                    state.waypoint = None;
                }
            });
        }
    }

    /// What a gravity gun at `muzzle` aiming `aim` grabs: drops, live charges, boulders and
    /// ordinary enemies (no elites, nothing stationary) in its cone within `range`, already
    /// near the hold point, or point blank (touching the hardpoint-to-muzzle stretch of the
    /// aim line, where an enemy pressed against the rig sits behind the muzzle). In id order.
    fn gravity_targets(
        &self,
        muzzle: (f32, f32),
        aim: (f32, f32),
        range: f32,
        hold: (f32, f32),
    ) -> Vec<EntityId> {
        let hardpoint = (
            aim.0.mul_add(-MUZZLE_REACH, muzzle.0),
            aim.1.mul_add(-MUZZLE_REACH, muzzle.1),
        );
        self.entities
            .ids_in_radius(muzzle.0, muzzle.1, range)
            .into_iter()
            .filter(|id| {
                let Some(entity) = self.entities.get(*id) else {
                    return false;
                };
                let grabbable = match &entity.state {
                    EntityState::Drop(_) | EntityState::Boulder(_) => true,
                    EntityState::Charge(charge) => charge.stage == 0,
                    EntityState::Enemy(state) => {
                        let spec = state.kind.stats();
                        !spec.elite
                            && !matches!(spec.locomotion, crate::enemy::Locomotion::Stationary)
                    }
                    _ => false,
                };
                if !grabbable {
                    return false;
                }
                let (x, y) = (entity.body.x, entity.body.y);
                let (dx, dy) = (x - muzzle.0, y - muzzle.1);
                let distance = dx.hypot(dy).max(1.0);
                let near_hold = (x - hold.0).hypot(y - hold.1) <= HOLD_RADIUS;
                let in_cone = dy.mul_add(aim.1, dx * aim.0) / distance >= 0.6;
                let point_blank = entity.state.health().is_some_and(|health| {
                    super::weapons::segment_distance((x, y), hardpoint, muzzle) <= health.radius
                });
                point_blank
                    || ((near_hold || in_cone)
                        && line_clear(&self.terrain, muzzle.0, muzzle.1, x, y))
            })
            .collect()
    }

    /// Rips the first loose tile along the aim out of the rock as a drop flying to the hold
    /// point.
    fn rip_loose_block(
        &mut self,
        muzzle: (f32, f32),
        aim: (f32, f32),
        range: f32,
        hold: (f32, f32),
    ) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a ray of a few hundred pixels"
        )]
        let steps = (range / 4.0) as u32;
        for step in 1..=steps {
            #[allow(clippy::cast_precision_loss, reason = "bounded step count")]
            let along = step as f32 * 4.0;
            let (x, y) = (
                aim.0.mul_add(along, muzzle.0),
                aim.1.mul_add(along, muzzle.1),
            );
            if !solid_at(&self.terrain, x, y) {
                continue;
            }
            let tile = tile_at(x, y);
            let Some(kind) = self.terrain.tile(tile).map(|tile| tile.kind) else {
                return;
            };
            if loose(kind) && self.terrain.set_kind(tile, TileKind::Air) {
                let (cx, cy) = crate::enemy::tile_centre(tile.x, tile.y);
                let (dx, dy) = (hold.0 - cx, hold.1 - cy);
                let distance = dx.hypot(dy).max(1.0);
                let _ = self.spawn_tile_drop(
                    tile,
                    kind,
                    (dx / distance * 200.0, dy / distance * 200.0),
                );
            }
            return;
        }
    }

    /// Flings everything held at `hold` along `aim`.
    fn gravity_throw(
        &mut self,
        player_id: PlayerId,
        spec: &WeaponStats,
        aim: (f32, f32),
        hold: (f32, f32),
        output: &mut TickOutput,
    ) {
        let flung: Vec<EntityId> = self
            .gravity_targets(hold, aim, HOLD_RADIUS, hold)
            .into_iter()
            .filter(|id| {
                self.entities.get(*id).is_some_and(|entity| {
                    (entity.body.x - hold.0).hypot(entity.body.y - hold.1) <= HOLD_RADIUS
                })
            })
            .collect();
        let stun_until = self
            .simulation_tick
            .next()
            .get()
            .saturating_add(self.timing.ticks_for_millis(THROW_STUN_MILLIS).max(1));
        let velocity = (aim.0 * spec.speed, aim.1 * spec.speed);
        for id in &flung {
            let enemy = self
                .entities
                .get(*id)
                .is_some_and(|entity| entity.kind() == EntityKind::Enemy);
            let _ = self.entities.update(*id, |entity| {
                entity.body.velocity_x = quantize(velocity.0);
                entity.body.velocity_y = quantize(velocity.1);
                entity.body.wake();
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.mode = EnemyMode::Stunned;
                    state.mode_until = SimulationTick::new(stun_until);
                }
            });
            if enemy {
                let _ = self.damage_entity(
                    *id,
                    Damage {
                        amount: spec.damage,
                        source: DamageSource::Weapon {
                            player: player_id,
                            weapon: WeaponKind::GravityGun,
                        },
                        armor_piercing: 0.0,
                        knockback: (0.0, 0.0),
                    },
                    output,
                );
            }
        }
        output.push_event(GameplayEventKind::GravityThrow {
            player_id,
            x: px(hold.0),
            y: px(hold.1),
            count: u16::try_from(flung.len()).unwrap_or(u16::MAX),
        });
    }

    /// One tick of a tether harpoon's trigger: hook on a fresh pull, hold, let go.
    #[allow(clippy::too_many_arguments, reason = "the trigger's whole context")]
    pub(super) fn tether_trigger(
        &mut self,
        player_id: PlayerId,
        spec: &WeaponStats,
        fire: bool,
        aim: (f32, f32),
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let weapon = player.weapon;
        if let Some(tether) = weapon.tether {
            let hooked = solid_at(&self.terrain, tether.x, tether.y);
            if !fire || !hooked {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.weapon.tether = None;
                }
                output.push_event(GameplayEventKind::TetherReleased { player_id });
            }
            return;
        }
        if !fire || weapon.pull_spent || next_tick < weapon.ready_tick {
            return;
        }
        if weapon.energy < spec.energy {
            output.push_event(GameplayEventKind::WeaponDry {
                player_id,
                weapon: WeaponKind::TetherHarpoon,
            });
            if let Some(player) = self.players.get_mut(&player_id) {
                player.weapon.pull_spent = true;
            }
            return;
        }
        let Some(muzzle) = self.muzzle(player_id, aim) else {
            return;
        };
        let ray = self.ray_cast(
            muzzle,
            aim,
            &WeaponStats {
                pierce: u8::MAX,
                tunnel: 0,
                ..*spec
            },
        );
        let (x, y) = (player.x, player.y);
        let cooldown = self.timing.ticks_for_millis(spec.cooldown_millis).max(1);
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.weapon.pull_spent = true;
        player.weapon.ready_tick = SimulationTick::new(next_tick.get().saturating_add(cooldown));
        player.weapon.energy = quantize(player.weapon.energy - spec.energy);
        let end = if ray.stopped_by.is_some() {
            // Hook just inside the rock face.
            let hook = (
                quantize(aim.0.mul_add(2.0, ray.end.0)),
                quantize(aim.1.mul_add(2.0, ray.end.1)),
            );
            let length = quantize((hook.0 - x).hypot(hook.1 - y).max(MIN_TETHER));
            player.weapon.tether = Some(Tether {
                x: hook.0,
                y: hook.1,
                length,
            });
            output.push_event(GameplayEventKind::TetherAttached {
                player_id,
                x: px(hook.0),
                y: px(hook.1),
            });
            hook
        } else {
            ray.end
        };
        output.push_event(GameplayEventKind::WeaponFired {
            player_id,
            weapon: WeaponKind::TetherHarpoon,
            x: px(muzzle.0),
            y: px(muzzle.1),
            end_x: px(end.0),
            end_y: px(end.1),
        });
    }

    /// Keeps a tethered rig within its line after movement (a pendulum swing; holding thrust
    /// reels the line in). Called right after the rig's movement step.
    pub(super) fn apply_tether(&mut self, player_id: PlayerId, thrust: bool) {
        let dt = self.timing.delta_seconds;
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let Some(mut tether) = player.weapon.tether else {
            return;
        };
        if player.mounted_weapon.map(|mounted| mounted.kind) != Some(WeaponKind::TetherHarpoon) {
            player.weapon.tether = None;
            return;
        }
        if thrust {
            tether.length = quantize(REEL_SPEED.mul_add(-dt, tether.length).max(MIN_TETHER));
        }
        let (dx, dy) = (player.x - tether.x, player.y - tether.y);
        let distance = dx.hypot(dy);
        if distance > tether.length && distance > 0.0 {
            let (nx, ny) = (dx / distance, dy / distance);
            let (tx, ty) = (
                quantize(nx.mul_add(tether.length, tether.x)),
                quantize(ny.mul_add(tether.length, tether.y)),
            );
            if !authoritative_player_collides(&self.terrain, tx, ty) {
                player.x = tx;
                player.y = ty;
                player.x_remainder = 0.0;
            }
            let radial = player.velocity_y.mul_add(ny, player.velocity_x * nx);
            if radial > 0.0 {
                player.velocity_x = quantize(radial.mul_add(-nx, player.velocity_x));
                player.velocity_y = quantize(radial.mul_add(-ny, player.velocity_y));
            }
        }
        player.weapon.tether = Some(tether);
    }

    /// One tick of a flak shell (a projectile with a burst radius): flies like any shot, then
    /// bursts near an enemy, on rock or at the end of its range.
    pub(super) fn advance_flak(
        &mut self,
        id: EntityId,
        mut body: Body,
        mut state: ProjectileState,
        spec: &WeaponStats,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        body.previous_x = body.x;
        body.previous_y = body.y;
        let gravity = spec.gravity
            * crate::environment::environment_at(self.terrain.settings(), body.x, body.y).gravity;
        body.velocity_y = quantize(gravity.mul_add(dt, body.velocity_y));
        let speed = body.velocity_x.hypot(body.velocity_y);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "bounded substep count"
        )]
        let steps = ((speed * dt / 4.0).ceil() as u32).clamp(1, 64);
        #[allow(clippy::cast_precision_loss, reason = "at most 64 substeps")]
        let step = dt / steps as f32;
        let fuse = spec.burst_radius * 0.5;
        let mut burst = None;
        for _ in 0..steps {
            let (last_x, last_y) = (body.x, body.y);
            body.x = body.velocity_x.mul_add(step, body.x);
            body.y = body.velocity_y.mul_add(step, body.y);
            state.travelled = speed.mul_add(step, state.travelled);
            if solid_at(&self.terrain, body.x, body.y) {
                burst = Some((last_x, last_y));
                break;
            }
            let near = self
                .enemies_near(body.x, body.y, fuse + crate::world::weapons::MAX_HIT_RADIUS)
                .into_iter()
                .any(|enemy| {
                    self.entities.get(enemy).is_some_and(|entity| {
                        let radius = entity.state.health().map_or(0.0, |health| health.radius);
                        (entity.body.x - body.x).hypot(entity.body.y - body.y) <= fuse + radius
                    })
                });
            if near || state.travelled >= spec.range {
                burst = Some((body.x, body.y));
                break;
            }
        }
        if let Some(at) = burst {
            let _ = self.entities.despawn(id);
            self.flak_burst(state.owner, spec, at, output);
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

    /// A flak burst at `at`: every damageable entity within the burst radius (in clear line)
    /// takes the shell's damage with falloff.
    fn flak_burst(
        &mut self,
        owner: PlayerId,
        spec: &WeaponStats,
        at: (f32, f32),
        output: &mut TickOutput,
    ) {
        let radius = spec.burst_radius;
        for target in
            self.entities
                .ids_in_radius(at.0, at.1, radius + crate::world::weapons::MAX_HIT_RADIUS)
        {
            let Some(entity) = self.entities.get(target) else {
                continue;
            };
            let Some(health) = entity.state.health() else {
                continue;
            };
            let (dx, dy) = (entity.body.x - at.0, entity.body.y - at.1);
            let distance = dx.hypot(dy);
            let edge = radius + health.radius;
            if distance >= edge
                || !line_clear(&self.terrain, at.0, at.1, entity.body.x, entity.body.y)
            {
                continue;
            }
            let falloff = (1.0 - distance / edge).clamp(0.35, 1.0);
            let length = distance.max(1.0);
            let _ = self.damage_entity(
                target,
                Damage {
                    amount: spec.damage * falloff,
                    source: DamageSource::Weapon {
                        player: owner,
                        weapon: WeaponKind::FlakCannon,
                    },
                    armor_piercing: 0.0,
                    knockback: (dx / length * spec.knockback, dy / length * spec.knockback),
                },
                output,
            );
        }
        output.push_event(GameplayEventKind::FlakBurst {
            player_id: owner,
            x: px(at.0),
            y: px(at.1),
        });
    }
}
