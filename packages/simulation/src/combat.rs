//! The damage model shared by weapons, explosives and (WP-ENEMIES) creatures.
//!
//! # The seam for enemies (T10) and other damageable entities
//!
//! An entity can be hurt when its per-kind state carries a [`Health`]: add a match arm for the
//! new kind to [`crate::entity::EntityState::health`] and
//! [`crate::entity::EntityState::health_mut`]. Everything else is already wired:
//!
//! * `WorldState::damage_entity(id, damage, output)` (inside the tick) applies a [`Damage`]
//!   through [`Health::apply`] (armour, armour piercing), knocks the body back, emits
//!   [`crate::GameplayEventKind::EntityDamaged`] / [`crate::GameplayEventKind::EntityDestroyed`]
//!   and despawns a destroyed entity. It returns the [`DamageOutcome`]; a creature kind that
//!   wants loot or a death animation reacts to `Destroyed` there (or to the event).
//! * Weapon projectiles and hitscan rays hit any entity with health inside its
//!   [`Health::radius`]; auto-aim targets them (skipping smoke screens); explosives damage
//!   them by distance ([`explosive_damage`]); EMP pulses only hurt [`Health::machine`] targets.
//! * Friendly fire is off: nothing in this model ever damages a rig. Rigs are hurt only by the
//!   hazards in `world/hazards.rs` (and, later, creature attacks, which should go through
//!   `WorldState::damage_player`).
//!
//! The practice target ([`crate::entity::EntityKind::Target`],
//! `WorldState::spawn_target_dummy`) is the reference damageable entity used by tests and
//! capture scenes.

use serde::{Deserialize, Serialize};

use crate::{explosive::ExplosiveKind, ids::PlayerId, terrain::TILE_SIZE, weapon::WeaponKind};

/// Hit points, armour and hitbox of a damageable entity.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    /// Flat damage soaked from every hit (never below a tenth of the hit).
    pub armor: f32,
    /// Hit radius around the body's centre, in pixels.
    pub radius: f32,
    /// A machine (EMP pulses hurt it).
    pub machine: bool,
}

/// Where a damageable entity can be hit (Wave 10 polish).
///
/// Its hit circle ([`Health::radius`]) widened to the ellipse of its drawn body (`half`, half
/// extents in pixels; zero for none). A creature's drawn body is its contact box
/// ([`crate::enemy::EnemyKind::contact_half`]), so a shot that visibly strikes the Leviathan's
/// head or tail, or a boss's reach, lands; the circle alone missed most of a long body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitArea {
    pub radius: f32,
    pub half: (f32, f32),
    /// How far below the body's centre the drawn body's centre sits (negative: above; a
    /// walker stands on its feet, so its drawn body rises over its centre).
    pub centre_dy: f32,
}

impl HitArea {
    /// Just a circle of `radius`.
    #[must_use]
    pub const fn circle(radius: f32) -> Self {
        Self {
            radius,
            half: (0.0, 0.0),
            centre_dy: 0.0,
        }
    }

    /// How far from the centre the area reaches toward the direction `(dx, dy)` (the
    /// ellipse taken about the body's centre: for reach tests, not exact hits).
    #[must_use]
    pub fn reach_toward(&self, dx: f32, dy: f32) -> f32 {
        let (a, b) = self.half;
        if a <= 0.0 || b <= 0.0 {
            return self.radius;
        }
        let distance = dx.hypot(dy);
        if distance <= f32::EPSILON {
            return self.radius.max(a.min(b));
        }
        // The ellipse's radius along the direction.
        let ellipse = a * b * distance / (b * dx).hypot(a * dy);
        self.radius.max(ellipse)
    }

    /// Whether the point at offset `(ox, oy)` from the body's centre is inside: within the hit
    /// circle, or the drawn body's ellipse.
    #[must_use]
    pub fn contains(&self, ox: f32, oy: f32) -> bool {
        if ox.hypot(oy) <= self.radius {
            return true;
        }
        let (a, b) = self.half;
        if a <= 0.0 || b <= 0.0 {
            return false;
        }
        let (ex, ey) = (ox / a, (oy - self.centre_dy) / b);
        ex.mul_add(ex, ey * ey) <= 1.0
    }

    /// The farthest the area reaches from the body's centre in any direction.
    #[must_use]
    pub fn extent(&self) -> f32 {
        self.radius
            .max(self.half.0)
            .max(self.half.1 + self.centre_dy.abs())
    }
}

impl Health {
    /// Full health.
    #[must_use]
    pub const fn new(max: f32, armor: f32, radius: f32) -> Self {
        Self {
            current: max,
            max,
            armor,
            radius,
            machine: false,
        }
    }

    /// The damage a hit of `amount` deals after armour: armour reduced by `armor_piercing`
    /// (0..=1) is subtracted, but at least a tenth of the hit always lands.
    #[must_use]
    pub fn mitigate(&self, amount: f32, armor_piercing: f32) -> f32 {
        let armor = self.armor * (1.0 - armor_piercing.clamp(0.0, 1.0));
        (amount - armor).max(amount * 0.1).max(0.0)
    }

    /// Applies a hit; returns the damage dealt.
    pub fn apply(&mut self, amount: f32, armor_piercing: f32) -> f32 {
        let dealt = self
            .mitigate(amount, armor_piercing)
            .min(self.current.max(0.0));
        self.current = crate::quantize::quantize_simulation_scalar(self.current - dealt);
        dealt
    }

    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

/// Who or what dealt damage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DamageSource {
    /// A rig's hardpoint weapon.
    Weapon {
        player: PlayerId,
        weapon: WeaponKind,
    },
    /// An explosive's blast (its owner, when known).
    Explosive {
        owner: Option<PlayerId>,
        kind: ExplosiveKind,
    },
    /// Terrain, hazards or other creatures.
    Environment,
}

impl DamageSource {
    /// The player credited with the hit, if any.
    #[must_use]
    pub const fn player(self) -> Option<PlayerId> {
        match self {
            Self::Weapon { player, .. } => Some(player),
            Self::Explosive { owner, .. } => owner,
            Self::Environment => None,
        }
    }
}

/// One hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    pub amount: f32,
    pub source: DamageSource,
    /// Fraction of the target's armour ignored (0..=1).
    pub armor_piercing: f32,
    /// Velocity change given to the target's body (pixels per second).
    pub knockback: (f32, f32),
}

impl Damage {
    /// A plain hit with no armour piercing or knockback.
    #[must_use]
    pub const fn new(amount: f32, source: DamageSource) -> Self {
        Self {
            amount,
            source,
            armor_piercing: 0.0,
            knockback: (0.0, 0.0),
        }
    }
}

/// What a hit did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageOutcome {
    /// The entity is gone or cannot be damaged.
    Ignored,
    /// It took `dealt` damage and has `remaining` left.
    Hurt { dealt: f32, remaining: f32 },
    /// It took the killing blow and was despawned.
    Destroyed { dealt: f32 },
}

/// Peak damage an explosive of `kind` deals to an entity at its centre (falling off linearly
/// to nothing at [`explosive_damage_reach`]); EMPs only hurt machines.
#[must_use]
pub const fn explosive_damage(kind: ExplosiveKind, machine: bool) -> f32 {
    match kind {
        ExplosiveKind::Bomb | ExplosiveKind::Sticky => 60.0,
        ExplosiveKind::C4 => 110.0,
        ExplosiveKind::Cluster | ExplosiveKind::Incendiary => 30.0,
        ExplosiveKind::Bomblet => 25.0,
        ExplosiveKind::Rocket => 45.0,
        ExplosiveKind::Shaped => 50.0,
        ExplosiveKind::Mining => 35.0,
        ExplosiveKind::Scare => 4.0,
        ExplosiveKind::Emp => {
            if machine {
                90.0
            } else {
                0.0
            }
        }
        ExplosiveKind::Flare | ExplosiveKind::Smoke | ExplosiveKind::TimeBubble => 0.0,
    }
}

/// How far an explosive of `kind` hurts entities, in pixels (its push reach, at least two
/// tiles; an EMP's pulse reaches five).
#[must_use]
pub fn explosive_damage_reach(kind: ExplosiveKind) -> f32 {
    if kind == ExplosiveKind::Emp {
        return TILE_SIZE * 5.0;
    }
    kind.push_reach().max(TILE_SIZE * 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armour_soaks_hits_but_never_all_of_them_and_piercing_ignores_it() {
        let mut health = Health::new(100.0, 8.0, 10.0);
        assert!((health.mitigate(20.0, 0.0) - 12.0).abs() < f32::EPSILON);
        assert!(
            (health.mitigate(5.0, 0.0) - 0.5).abs() < f32::EPSILON,
            "a tenth lands"
        );
        assert!((health.mitigate(20.0, 1.0) - 20.0).abs() < f32::EPSILON);
        assert!((health.mitigate(20.0, 0.5) - 16.0).abs() < f32::EPSILON);
        let dealt = health.apply(300.0, 0.0);
        assert!(
            (dealt - 100.0).abs() < f32::EPSILON,
            "cannot deal more than is left"
        );
        assert!(health.is_dead());
        assert!((explosive_damage(ExplosiveKind::Emp, false)).abs() < f32::EPSILON);
        assert!(explosive_damage(ExplosiveKind::Emp, true) > 0.0);
    }
}
