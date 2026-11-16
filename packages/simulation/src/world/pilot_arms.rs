//! The pilot's personal arms in the world (WP-PILOT; data in [`crate::pilot::arsenal`]).
//!
//! * **Hands**: each tick of a pilot on foot, the trigger ([`crate::WeaponControl::fire`])
//!   works the thing in its hands: a gun fires projectiles (the rig's projectile entities, so
//!   hits, kills and drops behave exactly as for rig weapons; friendly fire stays off), the
//!   wrench swings in an arc, and the hand pick chips soft tiles or, when a creature is in
//!   reach, swings at it. Guns repeat while the trigger is held, at their cooldown, so a held
//!   touch button keeps firing.
//! * **Aim**: free aim from the mouse or right stick, else auto-aim (the arrow-keys seat): the
//!   nearest damageable creature in a cone around the pilot's facing.
//! * **Ammo**: the sidearm and melee arms need none; the rifle fires cartridges and the
//!   shotgun shells from [`crate::pilot::arsenal::PilotArsenal::ammo`].
//! * **Swaps**: [`PilotAction::Equip`] / [`PilotAction::CycleWeapon`] (in the rig or on foot).
//! * **Buying**: [`crate::SessionServiceRequest::PilotArms`] at the town's gunsmith (the
//!   Explosives Shack) and the village armories' `VillageDeal::PilotWeapon` / `PilotAmmo`.

use crate::{
    combat::{Damage, DamageSource},
    entity::{Body, EntityId},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    input::ControlState,
    physics::line_clear,
    pilot::{
        PilotAction, PilotMode, PilotState,
        arsenal::{PilotArmsPurchase, is_melee},
    },
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::TILE_SIZE,
    weapon::{AUTO_AIM_CONE_COS, AUTO_AIM_RANGE, WeaponKind, aim_vector, quantize_aim},
};

use super::WorldState;

/// Auto-aim reach on foot (shorter than a rig's sensors).
pub const PILOT_AUTO_AIM_RANGE: f32 = AUTO_AIM_RANGE * 0.8;
/// How long a swing or a shot's recoil shows on the pilot (seconds).
pub const SWING_SHOW_SECONDS: f32 = 0.22;
/// Most creatures one melee swing connects with.
const MELEE_MAX_TARGETS: usize = 2;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn px(value: f32) -> i32 {
    value.round() as i32
}

impl WorldState {
    /// Where a pilot's shots start for aim `aim`: its hand, pushed out along the aim.
    #[must_use]
    pub const fn pilot_muzzle(body: &Body, state: &PilotState, aim: (f32, f32)) -> (f32, f32) {
        let hand_y = state.height().mul_add(-0.62, body.y);
        (aim.0.mul_add(12.0, body.x), aim.1.mul_add(10.0, hand_y))
    }

    /// The creature auto-aim would pick for a pilot at `origin` facing `facing`: the nearest
    /// damageable entity within [`PILOT_AUTO_AIM_RANGE`] in the cone, in clear view and out of
    /// smoke. `(id, x, y)`.
    #[must_use]
    pub fn pilot_auto_aim_target(
        &self,
        origin: (f32, f32),
        facing: (f32, f32),
    ) -> Option<(EntityId, f32, f32)> {
        let length = facing.0.hypot(facing.1);
        let facing = if length > f32::EPSILON {
            (facing.0 / length, facing.1 / length)
        } else {
            (1.0, 0.0)
        };
        let mut best: Option<(f32, EntityId, f32, f32)> = None;
        for id in self
            .entities
            .ids_in_radius(origin.0, origin.1, PILOT_AUTO_AIM_RANGE)
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
            if !(1.0..=PILOT_AUTO_AIM_RANGE).contains(&distance) {
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

    /// Resolves a pilot's aim for this tick from its controls (free aim, else auto-aim around
    /// the facing) and stores it on the state. Returns the unit aim.
    fn resolve_pilot_aim(
        &self,
        body: &Body,
        state: &mut PilotState,
        controls: ControlState,
    ) -> (f32, f32) {
        let weapon = controls.weapon;
        let facing = (f32::from(state.facing), 0.0);
        let free = (!weapon.auto_aim)
            .then(|| aim_vector(weapon.aim_x, weapon.aim_y))
            .flatten();
        let aim = free.unwrap_or_else(|| {
            let origin = Self::pilot_muzzle(body, state, facing);
            self.pilot_auto_aim_target(origin, facing)
                .and_then(|(_, tx, ty)| {
                    quantize_aim(tx - origin.0, ty - origin.1).and_then(|(x, y)| aim_vector(x, y))
                })
                .unwrap_or(facing)
        });
        if let Some((x, y)) = quantize_aim(aim.0, aim.1) {
            state.aim_x = x;
            state.aim_y = y;
            // Aiming behind turns the pilot around (but not while climbing or roped).
            if state.mode == PilotMode::Free && x.abs() > 20 && weapon.fire {
                state.facing = x.signum();
            }
        }
        aim_vector(state.aim_x, state.aim_y).unwrap_or(facing)
    }

    /// One tick of the pilot's hands: the trigger fires the held gun, swings the wrench, or
    /// works the hand pick (tiles, or a creature in reach).
    #[allow(
        clippy::too_many_arguments,
        reason = "the pilot pass's per-pilot context"
    )]
    pub(super) fn use_pilot_hands(
        &mut self,
        body: &Body,
        state: &mut PilotState,
        controls: ControlState,
        owner: PlayerId,
        next_tick: SimulationTick,
        dt: f32,
        output: &mut TickOutput,
    ) {
        state.fire_cooldown = quantize((state.fire_cooldown - dt).max(0.0));
        state.swing = quantize((state.swing - dt).max(0.0));
        let fresh_pull = controls.weapon.fire && !state.trigger_held;
        state.trigger_held = controls.weapon.fire;
        let busy = matches!(state.mode, PilotMode::Ledge | PilotMode::Climb);
        let held = state.weapon;
        if held.is_some() || controls.weapon.fire {
            let _ = self.resolve_pilot_aim(body, state, controls);
        }
        match held {
            Some(weapon) if !busy => {
                state.pick = None;
                if controls.weapon.fire && state.fire_cooldown <= 0.0 {
                    if is_melee(weapon) {
                        self.pilot_swing(body, state, weapon, owner, output);
                    } else {
                        self.pilot_shoot(body, state, weapon, owner, fresh_pull, next_tick, output);
                    }
                }
            }
            Some(_) => state.pick = None,
            None => {
                // The hand pick: a creature in reach takes the swing, else the tile does.
                if controls.weapon.fire
                    && !busy
                    && state.fire_cooldown <= 0.0
                    && self
                        .pilot_melee_targets(body, state, WeaponKind::HandPick)
                        .is_some()
                {
                    state.pick = None;
                    self.pilot_swing(body, state, WeaponKind::HandPick, owner, output);
                } else if state.swing <= 0.0 {
                    self.swing_pick(body, state, controls, owner, dt, output);
                }
            }
        }
    }

    /// Creatures a melee swing of `weapon` would connect with: damageable entities within its
    /// reach of the pilot's hand, on the aimed side, in clear view (nearest first).
    fn pilot_melee_targets(
        &self,
        body: &Body,
        state: &PilotState,
        weapon: WeaponKind,
    ) -> Option<Vec<(EntityId, f32, f32)>> {
        let reach = weapon.stats(0).range;
        let aim =
            aim_vector(state.aim_x, state.aim_y).unwrap_or_else(|| (f32::from(state.facing), 0.0));
        let (hx, hy) = state.middle(body);
        let mut found: Vec<(f32, EntityId, f32, f32)> = self
            .entities
            .ids_in_radius(hx, hy, reach + super::weapons::MAX_HIT_RADIUS)
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                let health = entity.state.health()?;
                let (tx, ty) = (entity.body.x, entity.body.y);
                let (dx, dy) = (tx - hx, ty - hy);
                let distance = dx.hypot(dy);
                if distance > reach + health.radius {
                    return None;
                }
                let facing_share = if distance > 1.0 {
                    dx.mul_add(aim.0, dy * aim.1) / distance
                } else {
                    1.0
                };
                (facing_share > -0.1 && line_clear(&self.terrain, hx, hy, tx, ty))
                    .then_some((distance, id, tx, ty))
            })
            .collect();
        if found.is_empty() {
            return None;
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Some(
            found
                .into_iter()
                .take(MELEE_MAX_TARGETS)
                .map(|(_, id, x, y)| (id, x, y))
                .collect(),
        )
    }

    fn pilot_swing(
        &mut self,
        body: &Body,
        state: &mut PilotState,
        weapon: WeaponKind,
        owner: PlayerId,
        output: &mut TickOutput,
    ) {
        let spec = weapon.stats(0);
        #[allow(clippy::cast_precision_loss, reason = "cooldowns are short")]
        let cooldown = spec.cooldown_millis as f32 / 1_000.0;
        state.fire_cooldown = cooldown;
        state.swing = SWING_SHOW_SECONDS;
        let aim =
            aim_vector(state.aim_x, state.aim_y).unwrap_or_else(|| (f32::from(state.facing), 0.0));
        let (hx, hy) = state.middle(body);
        let targets = self
            .pilot_melee_targets(body, state, weapon)
            .unwrap_or_default();
        output.push_event(GameplayEventKind::PilotFired {
            player_id: owner,
            weapon,
            x: px(hx),
            y: px(hy),
            end_x: px(aim.0.mul_add(spec.range, hx)),
            end_y: px(aim.1.mul_add(spec.range, hy)),
        });
        for (id, tx, ty) in targets {
            let (dx, dy) = (tx - hx, ty - hy);
            let distance = dx.hypot(dy).max(1.0);
            let damage = Damage {
                amount: spec.damage,
                source: DamageSource::Weapon {
                    player: owner,
                    weapon,
                },
                armor_piercing: spec.armor_piercing,
                knockback: (
                    quantize(dx / distance * spec.knockback),
                    quantize((dy / distance).mul_add(spec.knockback, -spec.knockback * 0.3)),
                ),
            };
            let _ = self.damage_entity(id, damage, output);
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the pilot pass's per-pilot context"
    )]
    fn pilot_shoot(
        &mut self,
        body: &Body,
        state: &mut PilotState,
        weapon: WeaponKind,
        owner: PlayerId,
        fresh_pull: bool,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let spec = weapon.stats(0);
        if let Some(ammo) = spec.ammo {
            let spent = self
                .players
                .get_mut(&owner)
                .is_some_and(|player| player.pilot_arsenal.spend(ammo));
            if !spent {
                if fresh_pull {
                    output.push_event(GameplayEventKind::PilotWeaponDry {
                        player_id: owner,
                        weapon,
                    });
                }
                return;
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "cooldowns are short")]
        let cooldown = spec.cooldown_millis as f32 / 1_000.0;
        state.fire_cooldown = cooldown;
        state.swing = SWING_SHOW_SECONDS;
        let aim =
            aim_vector(state.aim_x, state.aim_y).unwrap_or_else(|| (f32::from(state.facing), 0.0));
        let muzzle = Self::pilot_muzzle(body, state, aim);
        self.launch_projectiles(owner, weapon, 0, muzzle, aim, next_tick);
        output.push_event(GameplayEventKind::PilotFired {
            player_id: owner,
            weapon,
            x: px(muzzle.0),
            y: px(muzzle.1),
            end_x: px(aim.0.mul_add(TILE_SIZE, muzzle.0)),
            end_y: px(aim.1.mul_add(TILE_SIZE, muzzle.1)),
        });
    }

    /// [`PilotAction::Equip`] and [`PilotAction::CycleWeapon`]: what the pilot holds (also
    /// chosen from inside the rig, for the next time it climbs out).
    pub(super) fn equip_pilot_weapon(
        &mut self,
        player_id: PlayerId,
        action: PilotAction,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let arsenal = &mut player.pilot_arsenal;
        let next = match action {
            PilotAction::Equip(weapon) => {
                if weapon.is_some_and(|weapon| !arsenal.owned.contains(&weapon)) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                weapon
            }
            PilotAction::CycleWeapon => match arsenal.next_after(arsenal.equipped) {
                Some(next) => next,
                None => return PlayerScopedCommandOutcome::IgnoredUnavailable,
            },
            _ => return PlayerScopedCommandOutcome::IgnoredUnavailable,
        };
        if next == arsenal.equipped {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        arsenal.equipped = next;
        if let Some((id, _, _)) = self.pilot_of(player_id) {
            self.edit_pilot(id, |_, state| {
                state.weapon = next;
                state.fire_cooldown = state.fire_cooldown.max(0.15);
                state.pick = None;
            });
        }
        output.push_event(GameplayEventKind::PilotWeaponSwapped {
            player_id,
            weapon: next,
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// [`crate::SessionServiceRequest::PilotArms`]: a personal weapon (the first one goes
    /// straight into the pilot's hands) or a pack of personal ammo.
    pub(super) fn buy_pilot_arms(
        &mut self,
        player_id: PlayerId,
        purchase: PilotArmsPurchase,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let price = purchase.price();
        if player.credits < price {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let bought = match purchase {
            PilotArmsPurchase::Weapon(weapon) => player.pilot_arsenal.receive(weapon),
            PilotArmsPurchase::Ammo(ammo) => {
                ammo.item() != crate::weapon::AmmoKind::Rocket.item()
                    && player.pilot_arsenal.add_ammo(ammo, ammo.pack().0) > 0
            }
        };
        if !bought {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= price;
        let held = player.pilot_arsenal.equipped;
        if let Some((id, _, _)) = self.pilot_of(player_id) {
            self.edit_pilot(id, |_, state| state.weapon = held);
        }
        PlayerScopedCommandOutcome::Applied
    }
}
