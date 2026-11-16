//! Rig weapons (expansion plan WP-WEAPONS part 2): stats, prices, upgrades, energy and ammo.
//!
//! Pure data and math (no world access), so the game reads the same tables for its shop, HUD
//! and aim reticle.
//!
//! A rig owns weapons in its [`WeaponRack`] (bought and upgraded at the Explosives Shack) and
//! mounts one on its hardpoint ([`MountedWeapon`], public: everyone sees the gun on the rig).
//! Firing is a held control ([`crate::input::WeaponControl`]) with a free aim vector, so it is
//! predicted like movement. Energy weapons draw from the rig's capacitor
//! ([`RigWeaponState::energy`], capacity and recharge from [`WeaponRack::battery_level`]), which
//! recharges from fuel; the rocket launcher fires rocket ammo items
//! ([`crate::item::ItemKind::Ammo`]) from the cargo bay.
//!
//! The world side (firing, projectiles as entities, hits, the damage API) lives in
//! `world/weapons.rs`; damage rules in [`crate::combat`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    explosive::ExplosiveKind,
    ids::SimulationTick,
    item::{AmmoId, ItemKind, WeaponId},
    terrain::{StrategicResourceKind, TILE_SIZE},
};

/// A hardpoint weapon.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum WeaponKind {
    /// Fast, light rivets. Chips dirt and clay.
    RivetGun,
    /// A short-range spread of pellets with a kick.
    ScatterBlaster,
    /// A continuous cutting beam that also slices through rock (slowly).
    PlasmaCutter,
    /// Rockets that explode on impact (rocket ammo).
    RocketLauncher,
    /// A barbed harpoon that drags small targets and loose loot back to the rig.
    Harpoon,
    /// An instant piercing slug that punches through soft rock and armour.
    Railgun,
    // --- Act II weapons (Wave 4 T21): sold on worlds with sky content, paid partly in sky
    // materials ([`WeaponKind::materials`]).
    /// An electric arc that leaps from its first target to the next ones nearby.
    ArcRifle,
    /// Holds what it points at (drops, charges, enemies, loose blocks it rips out) and flings
    /// it on release.
    GravityGun,
    /// A harpoon on a line: hook terrain and swing from it while the trigger is held.
    TetherHarpoon,
    /// Shells with a proximity fuse that burst into shrapnel: the answer to airships.
    FlakCannon,
    /// The plasma cutter's beam refocused through alien alloy (WP-MOON, sold at the lunar
    /// colony): longer, hotter, and it cuts rock quickly.
    LunarCutter,
    /// Shells lobbed on a high arc that burst on impact; they sail far in low gravity (sold at
    /// the lunar colony).
    LowGMortar,
    /// Alien tech from the Warden's facility: a lance of void light that passes through foes
    /// and rock alike (dropped by the Warden).
    VoidLance,
}

/// How a weapon delivers its damage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delivery {
    /// Projectile entities that fly, fall (if `gravity` > 0) and hit.
    Projectile,
    /// An instant ray every shot (the cutter's beam pulses, the railgun's slug).
    Hitscan,
    /// An instant ray that chains from its first hit to [`WeaponStats::chains`] more targets
    /// within [`WeaponStats::chain_reach`] (the arc rifle).
    Chain,
    /// A held field that pulls things toward the muzzle and flings them on release (the
    /// gravity gun).
    Field,
    /// A line that hooks terrain within range; the rig swings from it while the trigger is
    /// held (the tether harpoon).
    Tether,
}

/// What a weapon shot does, at one upgrade tier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponStats {
    pub delivery: Delivery,
    /// Damage per hit (per pellet for spreads, per pulse for the beam).
    pub damage: f32,
    /// Time between shots (the beam's pulse interval).
    pub cooldown_millis: u64,
    /// Capacitor energy each shot draws (0 for ammo weapons).
    pub energy: f32,
    /// Ammo item each shot uses, if any.
    pub ammo: Option<AmmoKind>,
    /// Projectile speed (pixels per second) or ray range (pixels, hitscan).
    pub speed: f32,
    /// Projectile range in pixels before it fizzles (hitscan: the ray length).
    pub range: f32,
    /// Downward acceleration of projectiles (pixels per second squared).
    pub gravity: f32,
    /// Projectiles per shot.
    pub pellets: u8,
    /// Targets a hit passes through before stopping.
    pub pierce: u8,
    /// Durability a hit takes off a plain soil or rock tile (0: none).
    pub terrain_bite: u8,
    /// Plain tiles a hitscan shot can punch through (railgun) before stopping.
    pub tunnel: u8,
    /// Fraction of a target's armour this weapon ignores (0..=1).
    pub armor_piercing: f32,
    /// Push given to what it hits (pixels per second).
    pub knockback: f32,
    /// What an impact sets off (rockets), if anything.
    pub blast: Option<ExplosiveKind>,
    /// Hits drag targets and loose drops back toward the shooter.
    pub pulls: bool,
    /// Extra targets a chain hit leaps to (arc rifle).
    pub chains: u8,
    /// How far a chain leaps, pixels.
    pub chain_reach: f32,
    /// A projectile's shrapnel burst radius, pixels (0: none; flak bursts near enemies, at
    /// range or on terrain).
    pub burst_radius: f32,
}

/// Ammunition types (stable item ids, see [`AmmoKind::item`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AmmoKind {
    /// Rocket launcher ammunition.
    Rocket,
}

impl AmmoKind {
    pub const ALL: [Self; 1] = [Self::Rocket];

    #[must_use]
    pub const fn item(self) -> ItemKind {
        ItemKind::Ammo(match self {
            Self::Rocket => AmmoId(1),
        })
    }

    #[must_use]
    pub const fn from_item_id(id: AmmoId) -> Option<Self> {
        match id.0 {
            1 => Some(Self::Rocket),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rocket => "Rocket",
        }
    }

    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Rocket => "rockets",
        }
    }

    /// The shack's pack: `(count, cost)`.
    #[must_use]
    pub const fn pack(self) -> (u32, u32) {
        match self {
            Self::Rocket => (6, 120),
        }
    }
}

/// Highest upgrade tier of a weapon (Mk I is tier 0, so tiers run Mk I..=Mk IV).
pub const MAX_WEAPON_MK: u8 = 3;
/// Highest capacitor (battery) level.
pub const MAX_BATTERY_LEVEL: u8 = 4;
/// Fuel each point of recharged capacitor energy burns.
pub const FUEL_PER_ENERGY: f32 = 0.012;
/// How far auto-aim looks for a target, in pixels.
pub const AUTO_AIM_RANGE: f32 = TILE_SIZE * 11.0;
/// Auto-aim's cone around the facing direction: the cosine of its half-angle (about 50°).
pub const AUTO_AIM_CONE_COS: f32 = 0.64;
/// Height of the hardpoint above the rig's centre, in pixels (where shots start; the gun is
/// drawn on the dome's roof there).
pub const MUZZLE_RISE: f32 = 14.0;
/// Distance from the hardpoint to the muzzle along the aim, in pixels.
pub const MUZZLE_REACH: f32 = 11.0;

impl WeaponKind {
    /// Every weapon, in shop order.
    pub const ALL: [Self; 13] = [
        Self::RivetGun,
        Self::Harpoon,
        Self::ScatterBlaster,
        Self::PlasmaCutter,
        Self::RocketLauncher,
        Self::Railgun,
        Self::TetherHarpoon,
        Self::ArcRifle,
        Self::FlakCannon,
        Self::GravityGun,
        Self::LunarCutter,
        Self::LowGMortar,
        Self::VoidLance,
    ];

    /// Whether this is an Act II weapon (sold only on worlds with sky content, paid partly in
    /// sky materials).
    #[must_use]
    pub const fn act_two(self) -> bool {
        matches!(
            self,
            Self::ArcRifle | Self::GravityGun | Self::TetherHarpoon | Self::FlakCannon
        )
    }

    /// The sky materials buying it takes on top of [`Self::cost`] (Act II weapons).
    #[must_use]
    pub const fn materials(self) -> &'static [(StrategicResourceKind, u32)] {
        match self {
            Self::ArcRifle => &[(StrategicResourceKind::StormGlass, 6)],
            Self::GravityGun => &[(StrategicResourceKind::AetherCrystal, 6)],
            Self::TetherHarpoon => &[(StrategicResourceKind::StormGlass, 2)],
            Self::FlakCannon => &[
                (StrategicResourceKind::StormGlass, 3),
                (StrategicResourceKind::AetherCrystal, 3),
            ],
            _ => &[],
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RivetGun => "Rivet Gun",
            Self::ScatterBlaster => "Scatter Blaster",
            Self::PlasmaCutter => "Plasma Cutter",
            Self::RocketLauncher => "Rocket Launcher",
            Self::Harpoon => "Harpoon",
            Self::Railgun => "Railgun",
            Self::ArcRifle => "Arc Rifle",
            Self::GravityGun => "Gravity Gun",
            Self::TetherHarpoon => "Tether Harpoon",
            Self::FlakCannon => "Flak Cannon",
            Self::LunarCutter => "Lunar Cutter",
            Self::LowGMortar => "Low-G Mortar",
            Self::VoidLance => "Void Lance",
        }
    }

    /// One line for the shop and HUD.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::RivetGun => "Fast, light rivets. Cheap on energy; chips dirt and clay.",
            Self::ScatterBlaster => "A close-range fan of pellets with a hefty kick.",
            Self::PlasmaCutter => "A cutting beam: melts critters and slowly slices rock.",
            Self::RocketLauncher => "Rockets that burst on impact. Uses rocket ammo.",
            Self::Harpoon => "Spears small targets and loose loot and reels them in.",
            Self::Railgun => "An instant slug that pierces armour, foes and soft rock.",
            Self::ArcRifle => "Lightning that leaps from foe to foe. Machines hate it.",
            Self::GravityGun => {
                "Hold to grab loot, charges, foes and loose blocks; let go to fling."
            }
            Self::TetherHarpoon => "Hook rock and swing from the line while you hold the trigger.",
            Self::FlakCannon => "Proximity shells that burst into shrapnel. Downs airships.",
            Self::LunarCutter => "A long, hot alloy-focused beam that cuts rock quickly.",
            Self::LowGMortar => "Lobs shells on a high arc; they sail far in low gravity.",
            Self::VoidLance => "Alien tech: a lance of void light through foes and rock.",
        }
    }

    /// The item of this weapon (stable ids; saved).
    #[must_use]
    pub const fn item(self) -> ItemKind {
        ItemKind::Weapon(WeaponId(match self {
            Self::RivetGun => 1,
            Self::ScatterBlaster => 2,
            Self::PlasmaCutter => 3,
            Self::RocketLauncher => 4,
            Self::Harpoon => 5,
            Self::Railgun => 6,
            Self::ArcRifle => 7,
            Self::GravityGun => 8,
            Self::TetherHarpoon => 9,
            Self::FlakCannon => 10,
            Self::LunarCutter => 11,
            Self::LowGMortar => 12,
            Self::VoidLance => 13,
        }))
    }

    #[must_use]
    pub const fn from_item_id(id: WeaponId) -> Option<Self> {
        Some(match id.0 {
            1 => Self::RivetGun,
            2 => Self::ScatterBlaster,
            3 => Self::PlasmaCutter,
            4 => Self::RocketLauncher,
            5 => Self::Harpoon,
            6 => Self::Railgun,
            7 => Self::ArcRifle,
            8 => Self::GravityGun,
            9 => Self::TetherHarpoon,
            10 => Self::FlakCannon,
            11 => Self::LunarCutter,
            12 => Self::LowGMortar,
            13 => Self::VoidLance,
            _ => return None,
        })
    }

    /// Price at the shack.
    #[must_use]
    pub const fn cost(self) -> u32 {
        match self {
            Self::RivetGun => 450,
            Self::Harpoon => 900,
            Self::ScatterBlaster => 1_400,
            Self::PlasmaCutter => 2_400,
            Self::RocketLauncher => 3_600,
            Self::Railgun => 7_500,
            Self::TetherHarpoon => 6_000,
            Self::ArcRifle => 9_000,
            Self::FlakCannon => 10_000,
            Self::GravityGun => 11_000,
            // Moon weapons: the colony's prices (credits; alloy and helium-3 on top, see
            // `crate::moon::weapon_price`). The lance is never sold.
            Self::LunarCutter => 12_000,
            Self::LowGMortar => 9_000,
            Self::VoidLance => 20_000,
        }
    }

    /// Price of the upgrade to tier `mk` (1..=[`MAX_WEAPON_MK`]).
    #[must_use]
    pub const fn upgrade_cost(self, mk: u8) -> u32 {
        self.cost() / 2 * mk as u32 + self.cost() / 4
    }

    /// Explosives Shack level that stocks it.
    #[must_use]
    pub const fn shack_level(self) -> u8 {
        match self {
            Self::RivetGun | Self::Harpoon => 0,
            Self::ScatterBlaster | Self::PlasmaCutter => 1,
            Self::RocketLauncher | Self::TetherHarpoon => 2,
            Self::Railgun | Self::ArcRifle | Self::FlakCannon | Self::GravityGun => 3,
            // Never stocked by the shack: the lunar colony sells the cutter and the mortar,
            // the Warden drops the lance.
            Self::LunarCutter | Self::LowGMortar | Self::VoidLance => u8::MAX,
        }
    }

    /// Whether the trigger can be held for continuous fire (everything but the harpoon and
    /// the railgun repeats while held; those want a fresh pull).
    #[must_use]
    pub const fn automatic(self) -> bool {
        !matches!(
            self,
            Self::Harpoon | Self::Railgun | Self::TetherHarpoon | Self::VoidLance
        )
    }

    /// Whether it fires a continuous beam (the plasma cutter and its lunar refocus).
    #[must_use]
    pub const fn is_beam(self) -> bool {
        matches!(self, Self::PlasmaCutter | Self::LunarCutter)
    }

    /// Whether it is a Moon weapon (WP-MOON: never sold by the shack).
    #[must_use]
    pub const fn is_lunar(self) -> bool {
        matches!(self, Self::LunarCutter | Self::LowGMortar | Self::VoidLance)
    }

    /// The stats at upgrade tier `mk` (0: Mk I).
    #[must_use]
    pub fn stats(self, mk: u8) -> WeaponStats {
        let base = self.base_stats();
        let mk = f32::from(mk.min(MAX_WEAPON_MK));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "cooldowns are a few hundred milliseconds"
        )]
        let cooldown_millis = (base.cooldown_millis as f32 * 0.1f32.mul_add(-mk, 1.0)) as u64;
        WeaponStats {
            damage: base.damage * 0.25f32.mul_add(mk, 1.0),
            cooldown_millis: cooldown_millis.max(60),
            energy: base.energy * 0.12f32.mul_add(-mk, 1.0),
            range: base.range * 0.08f32.mul_add(mk, 1.0),
            ..base
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the stat table, one block per weapon"
    )]
    const fn base_stats(self) -> WeaponStats {
        let plain = WeaponStats {
            delivery: Delivery::Projectile,
            damage: 0.0,
            cooldown_millis: 500,
            energy: 0.0,
            ammo: None,
            speed: 600.0,
            range: TILE_SIZE * 10.0,
            gravity: 0.0,
            pellets: 1,
            pierce: 0,
            terrain_bite: 0,
            tunnel: 0,
            armor_piercing: 0.0,
            knockback: 0.0,
            blast: None,
            pulls: false,
            chains: 0,
            chain_reach: 0.0,
            burst_radius: 0.0,
        };
        match self {
            Self::RivetGun => WeaponStats {
                damage: 7.0,
                cooldown_millis: 150,
                energy: 3.0,
                speed: 760.0,
                range: TILE_SIZE * 12.0,
                gravity: 40.0,
                terrain_bite: 1,
                knockback: 40.0,
                ..plain
            },
            Self::ScatterBlaster => WeaponStats {
                damage: 6.0,
                cooldown_millis: 650,
                energy: 13.0,
                speed: 640.0,
                range: TILE_SIZE * 6.0,
                pellets: 7,
                terrain_bite: 1,
                knockback: 110.0,
                ..plain
            },
            Self::PlasmaCutter => WeaponStats {
                delivery: Delivery::Hitscan,
                damage: 4.5,
                cooldown_millis: 110,
                energy: 2.4,
                speed: 0.0,
                range: TILE_SIZE * 6.0,
                terrain_bite: 1,
                armor_piercing: 0.5,
                ..plain
            },
            Self::RocketLauncher => WeaponStats {
                damage: 12.0,
                cooldown_millis: 850,
                ammo: Some(AmmoKind::Rocket),
                speed: 430.0,
                range: TILE_SIZE * 22.0,
                gravity: 30.0,
                blast: Some(ExplosiveKind::Rocket),
                knockback: 60.0,
                ..plain
            },
            Self::Harpoon => WeaponStats {
                damage: 14.0,
                cooldown_millis: 900,
                energy: 6.0,
                speed: 580.0,
                range: TILE_SIZE * 9.0,
                gravity: 260.0,
                pulls: true,
                ..plain
            },
            Self::Railgun => WeaponStats {
                delivery: Delivery::Hitscan,
                damage: 75.0,
                cooldown_millis: 1_500,
                energy: 40.0,
                speed: 0.0,
                range: TILE_SIZE * 18.0,
                pierce: 4,
                tunnel: 3,
                terrain_bite: u8::MAX,
                armor_piercing: 1.0,
                knockback: 160.0,
                ..plain
            },
            Self::ArcRifle => WeaponStats {
                delivery: Delivery::Chain,
                damage: 16.0,
                cooldown_millis: 650,
                energy: 14.0,
                speed: 0.0,
                range: TILE_SIZE * 10.0,
                knockback: 40.0,
                chains: 3,
                chain_reach: TILE_SIZE * 5.0,
                ..plain
            },
            Self::GravityGun => WeaponStats {
                delivery: Delivery::Field,
                // Damage of a thrown hit; the hold itself is harmless.
                damage: 18.0,
                cooldown_millis: 100,
                // Energy per second while held.
                energy: 9.0,
                // Fling speed.
                speed: 720.0,
                range: TILE_SIZE * 7.0,
                knockback: 0.0,
                ..plain
            },
            Self::TetherHarpoon => WeaponStats {
                delivery: Delivery::Tether,
                damage: 0.0,
                cooldown_millis: 350,
                energy: 4.0,
                speed: 0.0,
                range: TILE_SIZE * 13.0,
                ..plain
            },
            Self::FlakCannon => WeaponStats {
                damage: 14.0,
                cooldown_millis: 700,
                energy: 12.0,
                speed: 520.0,
                range: TILE_SIZE * 16.0,
                gravity: 60.0,
                knockback: 90.0,
                burst_radius: TILE_SIZE * 2.2,
                ..plain
            },
            Self::LunarCutter => WeaponStats {
                delivery: Delivery::Hitscan,
                damage: 9.0,
                cooldown_millis: 100,
                energy: 3.0,
                speed: 0.0,
                range: TILE_SIZE * 10.0,
                terrain_bite: 3,
                armor_piercing: 0.7,
                ..plain
            },
            Self::LowGMortar => WeaponStats {
                damage: 26.0,
                cooldown_millis: 1_100,
                energy: 22.0,
                speed: 420.0,
                range: TILE_SIZE * 40.0,
                // Heavy shells: a steep arc on the ground, a long lob on the Moon (projectile
                // gravity follows the gravity fields).
                gravity: 520.0,
                blast: Some(ExplosiveKind::Rocket),
                knockback: 90.0,
                ..plain
            },
            Self::VoidLance => WeaponStats {
                delivery: Delivery::Hitscan,
                damage: 140.0,
                cooldown_millis: 1_300,
                energy: 45.0,
                speed: 0.0,
                range: TILE_SIZE * 24.0,
                pierce: 8,
                tunnel: 6,
                terrain_bite: u8::MAX,
                armor_piercing: 1.0,
                knockback: 220.0,
                ..plain
            },
        }
    }
}

/// The capacitor's size at `level`.
#[must_use]
pub fn battery_capacity(level: u8) -> f32 {
    [100.0, 150.0, 210.0, 280.0, 360.0][usize::from(level.min(MAX_BATTERY_LEVEL))]
}

/// Capacitor recharge per second at `level`.
#[must_use]
pub fn battery_recharge(level: u8) -> f32 {
    f32::from(level.min(MAX_BATTERY_LEVEL)).mul_add(7.0, 22.0)
}

/// Price of the capacitor upgrade to `level` (1..=[`MAX_BATTERY_LEVEL`]).
#[must_use]
pub const fn battery_upgrade_cost(level: u8) -> u32 {
    match level {
        0 | 1 => 500,
        2 => 1_100,
        3 => 2_200,
        _ => 4_000,
    }
}

/// The weapons a rig owns and its capacitor (private: the rig's owner sees it).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeaponRack {
    /// Each owned weapon's upgrade tier (0: Mk I).
    pub owned: BTreeMap<WeaponKind, u8>,
    /// Capacitor level (0..=[`MAX_BATTERY_LEVEL`]).
    pub battery_level: u8,
}

impl WeaponRack {
    /// The upgrade tier of `weapon`, if owned.
    #[must_use]
    pub fn mk(&self, weapon: WeaponKind) -> Option<u8> {
        self.owned.get(&weapon).copied()
    }

    /// The owned weapon after `current` in shop order (wrapping), or the first owned one.
    #[must_use]
    pub fn next_after(&self, current: Option<WeaponKind>) -> Option<WeaponKind> {
        let start = current
            .and_then(|kind| WeaponKind::ALL.iter().position(|other| *other == kind))
            .map_or(0, |index| index + 1);
        (0..WeaponKind::ALL.len())
            .map(|offset| WeaponKind::ALL[(start + offset) % WeaponKind::ALL.len()])
            .find(|kind| self.owned.contains_key(kind))
    }
}

/// The weapon on a rig's hardpoint and its tier (public: drawn on the rig).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MountedWeapon {
    pub kind: WeaponKind,
    pub mk: u8,
}

/// A rig's live weapon state (per tick: changes while firing and recharging).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RigWeaponState {
    /// Capacitor energy.
    pub energy: f32,
    /// Tick the weapon can fire again at.
    pub ready_tick: SimulationTick,
    /// The latest aim (quantized direction; presentation draws the gun pointing this way).
    pub aim_x: i8,
    pub aim_y: i8,
    /// The trigger was held last tick (beams draw while held).
    pub firing: bool,
    /// The current trigger pull already fired (semi-automatic weapons need a fresh pull).
    pub pull_spent: bool,
    /// The tether harpoon's line while it is hooked (the rig swings from it).
    #[serde(default)]
    pub tether: Option<Tether>,
}

/// A tether harpoon's line: hooked at world pixel `(x, y)`, `length` pixels long (the rig
/// cannot be farther; holding thrust reels it in).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Tether {
    pub x: f32,
    pub y: f32,
    pub length: f32,
}

impl RigWeaponState {
    /// The aim as a unit vector (right when unset).
    #[must_use]
    pub fn aim(&self) -> (f32, f32) {
        aim_vector(self.aim_x, self.aim_y).unwrap_or((1.0, 0.0))
    }
}

/// The unit vector of a quantized aim, or `None` for a zero aim. Uses only `sqrt` (correctly
/// rounded everywhere), so every peer gets the same bits.
#[must_use]
pub fn aim_vector(x: i8, y: i8) -> Option<(f32, f32)> {
    let (x, y) = (f32::from(x), f32::from(y));
    let length = x.mul_add(x, y * y).sqrt();
    (length > 0.0).then(|| (x / length, y / length))
}

/// Quantizes a direction to the `i8` aim the controls carry (`None` for a zero vector).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "a unit vector scaled to 127 fits i8"
)]
pub fn quantize_aim(x: f32, y: f32) -> Option<(i8, i8)> {
    let length = x.hypot(y);
    if length < f32::EPSILON || !length.is_finite() {
        return None;
    }
    let (qx, qy) = (
        (x / length * 127.0).round() as i8,
        (y / length * 127.0).round() as i8,
    );
    (qx != 0 || qy != 0).then_some((qx, qy))
}

/// `(cos, sin)` of the scatter blaster's pellet offsets (multiples of 0.07 rad), exact literals.
const SPREAD: [(f32, f32); 7] = [
    (0.978_030_9, -0.208_457_8),
    (0.990_217_2, -0.139_543_1),
    (0.997_551, -0.069_942_85),
    (1.0, 0.0),
    (0.997_551, 0.069_942_85),
    (0.990_217_2, 0.139_543_1),
    (0.978_030_9, 0.208_457_8),
];

/// The direction of pellet `index` of `count` around `aim` (deterministic fan; a single pellet
/// flies straight).
#[must_use]
pub fn pellet_direction(aim: (f32, f32), index: u8, count: u8) -> (f32, f32) {
    if count <= 1 {
        return aim;
    }
    let slot = usize::from(index).min(SPREAD.len() - 1);
    let (cos, sin) = SPREAD[slot];
    (
        aim.0.mul_add(cos, -aim.1 * sin),
        aim.0.mul_add(sin, aim.1 * cos),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_round_trip_and_upgrades_scale_up() {
        for kind in WeaponKind::ALL {
            let ItemKind::Weapon(id) = kind.item() else {
                panic!("weapon item");
            };
            assert_eq!(WeaponKind::from_item_id(id), Some(kind));
            let mk1 = kind.stats(0);
            let mk4 = kind.stats(MAX_WEAPON_MK);
            // The tether harpoon deals no damage; everything else hits harder with upgrades.
            assert!(
                mk4.damage > mk1.damage || (mk1.damage == 0.0 && kind == WeaponKind::TetherHarpoon),
                "{kind:?}"
            );
            assert!(mk4.cooldown_millis <= mk1.cooldown_millis, "{kind:?}");
            assert!(mk4.energy <= mk1.energy, "{kind:?}");
            assert!(kind.upgrade_cost(1) < kind.upgrade_cost(MAX_WEAPON_MK));
        }
        for ammo in AmmoKind::ALL {
            let ItemKind::Ammo(id) = ammo.item() else {
                panic!("ammo item");
            };
            assert_eq!(AmmoKind::from_item_id(id), Some(ammo));
        }
    }

    #[test]
    fn aims_quantize_and_normalize_and_pellets_fan_symmetrically() {
        assert_eq!(quantize_aim(0.0, 0.0), None);
        let (x, y) = quantize_aim(3.0, -4.0).expect("aim");
        let (ux, uy) = aim_vector(x, y).expect("unit");
        assert!((ux - 0.6).abs() < 0.01 && (uy + 0.8).abs() < 0.01);
        assert!((ux.hypot(uy) - 1.0).abs() < 1e-5);
        for (cos, sin) in SPREAD {
            assert!((cos.hypot(sin) - 1.0).abs() < 1e-5);
        }
        let left = pellet_direction((1.0, 0.0), 0, 7);
        let right = pellet_direction((1.0, 0.0), 6, 7);
        assert!((left.1 + right.1).abs() < 1e-6 && left.1 < 0.0);
        assert_eq!(pellet_direction((0.0, 1.0), 3, 1), (0.0, 1.0));
    }

    #[test]
    fn racks_cycle_through_owned_weapons_in_shop_order() {
        let mut rack = WeaponRack::default();
        assert_eq!(rack.next_after(None), None);
        rack.owned.insert(WeaponKind::Railgun, 0);
        rack.owned.insert(WeaponKind::RivetGun, 2);
        assert_eq!(rack.next_after(None), Some(WeaponKind::RivetGun));
        assert_eq!(
            rack.next_after(Some(WeaponKind::RivetGun)),
            Some(WeaponKind::Railgun)
        );
        assert_eq!(
            rack.next_after(Some(WeaponKind::Railgun)),
            Some(WeaponKind::RivetGun)
        );
        assert!(battery_capacity(MAX_BATTERY_LEVEL) > battery_capacity(0));
    }
}
