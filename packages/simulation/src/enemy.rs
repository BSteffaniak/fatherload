//! Enemies: creatures and machines of the caves and deep layers (expansion plan WP-ENEMIES).
//!
//! Pure data and math (no world access): the kinds and their stats, the per-entity AI state,
//! damage kinds and resistances, enemy shots, the habitat tables that decide what spawns where,
//! loot tables and the world-level [`EnemyDirector`] state (difficulty, rig invulnerability
//! windows, spawn cooldowns, cleared lairs).
//!
//! The behaviour (spawning, AI, attacks, damage, deaths) lives in `world/enemies.rs`; enemies
//! are ordinary entities of the entity store ([`crate::entity::EntityState::Enemy`],
//! [`crate::entity::EntityState::EnemyShot`]), so saves, keyframes, hashing, relevance and
//! per-client replication cover them automatically.
//!
//! # Families
//!
//! | Kind | Family | Habitat | Tell | Counter |
//! |---|---|---|---|---|
//! | [`EnemyKind::CaveBat`] | creature (swarmer) | caves, clay to the void | screech, packs | anything; flares lure them |
//! | [`EnemyKind::Burrower`] | creature | soft rock, drawn to drilling | rumble, then it lunges | stop drilling, bombs |
//! | [`EnemyKind::AcidSpitter`] | creature (ranged) | cave floors | puffs up before spitting | close in, break line of sight |
//! | [`EnemyKind::RockMimic`] | creature (armoured) | looks like a boulder | wakes when you come close | explosives |
//! | [`EnemyKind::Shellback`] | creature (armoured) | deep cave floors | slow charge wind-up | explosives, armour piercing |
//! | [`EnemyKind::MagmaWisp`] | creature (lava elemental) | the mantle | glows before a fireball | kinetic and explosives; fire-proof |
//! | [`EnemyKind::BroodNest`] | creature (hive) | lair caverns | spawns bats while you are near | destroy it (explosives) |
//! | [`EnemyKind::BroodLord`] | creature (elite) | rare lair caverns | roars, charges, spits volleys | explosives, keep moving |
//! | [`EnemyKind::MiningDrone`] | machine | the basalt down to the machine layer | beeps, then rams | EMP |
//! | [`EnemyKind::SentryTurret`] | machine (ranged) | the machine layer | charge glow before each bolt | EMP, explosives |
//! | [`EnemyKind::RogueExcavator`] | machine | deep layers; drills through rock toward you | engine rev before a charge | EMP, explosives |
//! | [`EnemyKind::AncientGuardian`] | machine (elite) | machine-layer and mantle lairs | shielded until an EMP drops it | EMP, then explosives |
//! | [`EnemyKind::RegolithCrawler`] | creature | the Moon's surface and lava tubes | crouches, then a long low-g leap | anything; keep moving |
//! | [`EnemyKind::CrystalMite`] | creature (swarmer) | the Moon, thickest in crystal forests | chitter, packs | blasts and fire; crystal shrugs off bullets |
//! | [`EnemyKind::DerelictAutomaton`] | machine (ranged) | the Moon, around abandoned habitats | red eye charging | EMP, explosives |
//! | [`EnemyKind::Warden`] | machine (boss) | the alien facility's arena (one per Moon) | shield up until a conduit breaks or an EMP lands | the arena's conduits or EMP, then explosives |
//!
//! # Sky families (Wave 4 T21, V9 skies)
//!
//! | Kind | Family | Habitat | Tell | Counter |
//! |---|---|---|---|---|
//! | [`EnemyKind::SkyRay`] | creature (swooper) | archipelago to the continents | climbs above you, hangs, then dives | dodge the dive, then shoot |
//! | [`EnemyKind::Wyvern`] | creature (swooper) | the sky continents | dives; spits fireballs from afar | arc rifle, flak |
//! | [`EnemyKind::StormElemental`] | elemental | the dead-air gap and the continents | crackles, then a lightning arc that jolts machines (capacitor drained, weapon jammed) | kinetic, flak; immune to shock |
//! | [`EnemyKind::PirateAirship`] | machine (elite, multi-part) | the dead-air gap and the continents | holds station above you, drops boarders | flak, rockets; down the turrets first |
//! | [`EnemyKind::AirshipTurret`] | machine (airship part) | rides an airship | charge glow, then a lobbed cannonball | EMP, flak |
//! | [`EnemyKind::SkyPirate`] | creature (boarder) | dropped by airships, jetpacks | cutlass lunge | anything; fight it on foot |
//! | [`EnemyKind::HarpyNester`] | creature (guard) | nests on continent summits | only cares near its nest, unless its eggs are taken | keep away, or fight it at the nest |
//!
//! Night monsters (WP-NIGHT, [`crate::night`]) rise on the open surface at dusk and burrow or
//! flee at dawn:
//!
//! | Kind | Family | Habitat | Tell | Counter |
//! |---|---|---|---|---|
//! | [`EnemyKind::ShadowHound`] | creature (pack) | plains, tundra, badlands, mountains | a howl, glinting eyes, then a pounce | light: it will not enter a lamp's pool; flares send it running |
//! | [`EnemyKind::DuneStalker`] | creature (burrower) | desert and badlands sand | feels footsteps, a ripple in the sand, then it erupts | keep off the sand, palisades, explosives |
//! | [`EnemyKind::WillOWisp`] | creature (spirit) | wetlands, forests, mountains | a chime and a brightening core before a wispfire bolt | close in; it shrugs off bullets, not blasts |
//! | [`EnemyKind::MothSwarm`] | creature (swarm) | forests, wetlands, plains | drawn to light: circles lamps and headlights | a lamp away from camp lures it; fire |
//! | [`EnemyKind::UmbralAlpha`] | creature (elite) | new-moon nights anywhere | a long howl heard across the land | explosives, keep moving, stay lit |

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ids::{PlayerId, SimulationTick},
    item::{ItemKind, ItemStack},
    terrain::{
        ArtifactKind, ChunkPosition, DeepStratum, MineralKind, StrategicResourceKind, TILE_SIZE,
    },
    worldgen::{HazardIntensity, strata::Band},
};

/// The kind of an enemy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EnemyKind {
    CaveBat,
    Burrower,
    AcidSpitter,
    RockMimic,
    Shellback,
    MagmaWisp,
    BroodNest,
    BroodLord,
    MiningDrone,
    SentryTurret,
    RogueExcavator,
    AncientGuardian,
    // --- Sky families (Wave 4 T21) ----------------------------------------------------------
    SkyRay,
    Wyvern,
    StormElemental,
    PirateAirship,
    AirshipTurret,
    SkyPirate,
    HarpyNester,
    /// WP-MOON: a pale many-legged leaper of the lunar dust.
    RegolithCrawler,
    /// WP-MOON: crystal-shelled mites that swarm out of the crystal forests.
    CrystalMite,
    /// WP-MOON: an abandoned colony's security robot, still on patrol.
    DerelictAutomaton,
    /// WP-MOON: the alien facility's guardian, the Moon's boss.
    Warden,
    // --- Night monsters (WP-NIGHT, `crate::night`), appended -----------------------------
    /// A pack hunter of the open surface at night that shuns lamp light.
    ShadowHound,
    /// A sand burrower that feels footsteps and erupts under its prey.
    DuneStalker,
    /// A drifting marsh light that throws wispfire.
    WillOWisp,
    /// A cloud of moths drawn to every light.
    MothSwarm,
    /// The pack leader that walks only on new-moon nights (a rare spawn).
    UmbralAlpha,
}

/// Something a sky enemy did that the game cues (see
/// [`crate::event::GameplayEventKind::SkyEnemyAction`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SkyEnemyAction {
    /// A swooper hangs above its prey and begins its dive.
    Swoop,
    /// An airship dropped a boarding party.
    BoardersDropped,
    /// An airship went down (its turrets with it).
    AirshipDowned,
    /// A harpy nester found its eggs gone.
    NesterEnraged,
}

/// Creature or machine: machines feel EMPs, creatures burn.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EnemyFamily {
    Creature,
    Machine,
}

/// How an enemy moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locomotion {
    /// Flies freely (no gravity), bumping off rock.
    Flying,
    /// Walks on floors under gravity and hops one-tile steps.
    Walking,
    /// Digs real tunnels through soft rock (never ore) while clinging to it; falls in open air.
    Burrowing,
    /// Never moves (turrets, nests).
    Stationary,
}

/// How an enemy hurts a rig.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttackStyle {
    /// Touching hurts whenever its attack is ready (swarmers, drones).
    Contact,
    /// A telegraphed lunge or charge: the wind-up, then a fast dash that hurts on contact.
    Lunge,
    /// A telegraphed shot (or volley) of [`ShotKind`] projectiles.
    Ranged(ShotKind, u8),
    /// Spawns swarmers instead of attacking.
    Spawner,
    /// Climbs above its prey, hangs (the wind-up), then dives through it and climbs away
    /// (sky rays, wyverns, harpies; hurts on contact during the dive).
    Swoop,
    /// A telegraphed lightning arc: an instant strike at range that jolts machines (storm
    /// elementals).
    Lightning,
    /// Holds station above its prey and drops boarders (airships).
    Boarding,
}

/// A projectile enemies fire.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ShotKind {
    /// A lobbed glob of acid (falls under gravity).
    Acid,
    /// A slow, straight fireball.
    Fireball,
    /// A fast, straight energy bolt.
    Bolt,
    /// A lobbed cannonball (airship turrets).
    Cannonball,
    /// A slow, ghostly flame a will-o'-wisp throws (WP-NIGHT).
    Wispfire,
}

impl ShotKind {
    /// Launch speed, pixels per second.
    #[must_use]
    pub const fn speed(self) -> f32 {
        match self {
            Self::Acid => 230.0,
            Self::Fireball => 170.0,
            Self::Bolt => 330.0,
            Self::Cannonball => 280.0,
            Self::Wispfire => 140.0,
        }
    }

    /// Downward acceleration, pixels per second squared.
    #[must_use]
    pub const fn gravity(self) -> f32 {
        match self {
            Self::Acid => 320.0,
            Self::Cannonball => 160.0,
            Self::Fireball | Self::Bolt | Self::Wispfire => 0.0,
        }
    }

    /// Collision half-size, pixels.
    #[must_use]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Acid | Self::Bolt => 4.0,
            Self::Fireball | Self::Cannonball => 6.0,
            Self::Wispfire => 5.0,
        }
    }

    /// Base hull damage of a hit (scaled by the shooter's depth tier and the difficulty).
    #[must_use]
    pub const fn damage(self) -> f32 {
        match self {
            Self::Acid => 7.0,
            Self::Fireball => 10.0,
            Self::Bolt => 8.0,
            Self::Cannonball => 9.0,
            Self::Wispfire => 6.0,
        }
    }

    /// How long a shot flies before fizzling, milliseconds.
    #[must_use]
    pub const fn life_millis(self) -> u64 {
        match self {
            Self::Acid => 2_500,
            Self::Fireball => 4_000,
            Self::Bolt => 2_200,
            Self::Cannonball => 3_500,
            Self::Wispfire => 3_500,
        }
    }
}

/// What kind of harm a hit does (resistances differ per kind).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DamageKind {
    /// Bullets, rivets, blades, rams.
    Kinetic,
    /// Armour piercing (railguns, harpoons, drills).
    Piercing,
    /// Blasts.
    Explosive,
    /// Flames and incendiaries.
    Fire,
    /// Electromagnetic pulses (machines only).
    Emp,
    /// Electric arcs (the arc rifle, lightning): machines conduct them, storm elementals
    /// drink them.
    Shock,
}

/// Who or what dealt a hit (kill credit, aggro).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DamageSource {
    /// A player's weapon, explosive or rig.
    Player(PlayerId),
    /// The world itself (an unowned charge, lava, a cave-in).
    World,
}

impl DamageSource {
    /// The player behind the hit, if any.
    #[must_use]
    pub const fn player(self) -> Option<PlayerId> {
        match self {
            Self::Player(player) => Some(player),
            Self::World => None,
        }
    }
}

/// One hit on an enemy, for `WorldState::damage_entity` (the damage API WP-WEAPONS uses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    /// Raw damage before the target's resistance.
    pub amount: f32,
    pub kind: DamageKind,
    pub source: DamageSource,
    /// Velocity added to the target (pixels per second; stationary enemies ignore it).
    pub impulse: (f32, f32),
}

impl Damage {
    /// `amount` of `kind` from `source`, with no push.
    #[must_use]
    pub const fn new(amount: f32, kind: DamageKind, source: DamageSource) -> Self {
        Self {
            amount,
            kind,
            source,
            impulse: (0.0, 0.0),
        }
    }

    /// The same hit with a push.
    #[must_use]
    pub const fn with_impulse(mut self, x: f32, y: f32) -> Self {
        self.impulse = (x, y);
        self
    }
}

/// What a [`Damage`] did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageOutcome {
    /// Nothing to hit (not an enemy, gone, immune, or absorbed by a shield).
    Ignored,
    /// It took `dealt` damage and lives.
    Hurt { dealt: f32 },
    /// It took `dealt` damage and died (its loot dropped).
    Killed { dealt: f32 },
}

/// The fixed stats of one enemy kind (before depth, difficulty and crew scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyStats {
    pub name: &'static str,
    pub family: EnemyFamily,
    pub locomotion: Locomotion,
    pub attack: AttackStyle,
    /// Collision half-size, pixels.
    pub radius: f32,
    pub health: f32,
    /// Shield absorbing everything but EMPs until an EMP breaks it (0: none).
    pub shield: f32,
    /// Cruise speed, pixels per second.
    pub speed: f32,
    /// Dash speed of a lunge or charge, pixels per second.
    pub dash_speed: f32,
    /// How far it sees a rig (with a clear line of sight), tiles.
    pub sight_tiles: f32,
    /// How far it feels a drilling rig through rock, tiles.
    pub hearing_tiles: f32,
    /// How far from home it chases before giving up, tiles.
    pub leash_tiles: f32,
    /// Hull damage of a contact hit (lunges, rams, bites).
    pub contact_damage: f32,
    /// Telegraph before an attack, milliseconds.
    pub windup_millis: u64,
    /// Time between attacks, milliseconds.
    pub cooldown_millis: u64,
    /// Distance at which it starts an attack, tiles (melee: lunge range; ranged: firing range).
    pub attack_range_tiles: f32,
    /// Push given to a rig it hits, pixels per second.
    pub knockback: f32,
    /// A set-piece elite (lair boss, big loot).
    pub elite: bool,
}

/// Defaults for the stat table (each kind overrides what makes it distinct).
const fn stats(
    name: &'static str,
    family: EnemyFamily,
    locomotion: Locomotion,
    attack: AttackStyle,
) -> EnemyStats {
    EnemyStats {
        name,
        family,
        locomotion,
        attack,
        radius: 8.0,
        health: 30.0,
        shield: 0.0,
        speed: 60.0,
        dash_speed: 0.0,
        sight_tiles: 10.0,
        hearing_tiles: 5.0,
        leash_tiles: 24.0,
        contact_damage: 6.0,
        windup_millis: 500,
        cooldown_millis: 1_500,
        attack_range_tiles: 1.5,
        knockback: 140.0,
        elite: false,
    }
}

impl EnemyKind {
    /// Every kind, in order.
    pub const ALL: [Self; 28] = [
        Self::CaveBat,
        Self::Burrower,
        Self::AcidSpitter,
        Self::RockMimic,
        Self::Shellback,
        Self::MagmaWisp,
        Self::BroodNest,
        Self::BroodLord,
        Self::MiningDrone,
        Self::SentryTurret,
        Self::RogueExcavator,
        Self::AncientGuardian,
        Self::SkyRay,
        Self::Wyvern,
        Self::StormElemental,
        Self::PirateAirship,
        Self::AirshipTurret,
        Self::SkyPirate,
        Self::HarpyNester,
        Self::RegolithCrawler,
        Self::CrystalMite,
        Self::DerelictAutomaton,
        Self::Warden,
        Self::ShadowHound,
        Self::DuneStalker,
        Self::WillOWisp,
        Self::MothSwarm,
        Self::UmbralAlpha,
    ];

    /// The kind's stats.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "the stat table, one block per kind")]
    pub const fn stats(self) -> EnemyStats {
        use EnemyFamily::{Creature, Machine};
        use Locomotion::{Burrowing, Flying, Stationary, Walking};
        match self {
            Self::CaveBat => EnemyStats {
                radius: 6.0,
                health: 10.0,
                speed: 150.0,
                sight_tiles: 9.0,
                hearing_tiles: 4.0,
                leash_tiles: 26.0,
                contact_damage: 4.0,
                windup_millis: 0,
                cooldown_millis: 900,
                attack_range_tiles: 0.6,
                knockback: 90.0,
                ..stats("Cave Bat", Creature, Flying, AttackStyle::Contact)
            },
            Self::Burrower => EnemyStats {
                radius: 9.0,
                health: 40.0,
                speed: 70.0,
                dash_speed: 260.0,
                sight_tiles: 6.0,
                // Balance pass (T19): it heard drills 16 tiles off and lunged every 1.9 s for
                // 10, the worst threat in every band down to the fossil oceans (about 190 hull
                // a minute against an unarmed rig in the clay beds). It now hears 10 tiles,
                // telegraphs longer and bites softer, so a drilling rig has time to react.
                hearing_tiles: 10.0,
                leash_tiles: 30.0,
                contact_damage: 7.0,
                windup_millis: 850,
                cooldown_millis: 3_000,
                attack_range_tiles: 4.0,
                knockback: 170.0,
                ..stats("Burrower", Creature, Burrowing, AttackStyle::Lunge)
            },
            Self::AcidSpitter => EnemyStats {
                radius: 8.0,
                health: 30.0,
                speed: 45.0,
                sight_tiles: 12.0,
                hearing_tiles: 6.0,
                contact_damage: 3.0,
                windup_millis: 700,
                cooldown_millis: 2_200,
                attack_range_tiles: 9.0,
                ..stats(
                    "Acid Spitter",
                    Creature,
                    Walking,
                    AttackStyle::Ranged(ShotKind::Acid, 1),
                )
            },
            Self::RockMimic => EnemyStats {
                radius: 11.0,
                health: 70.0,
                speed: 60.0,
                dash_speed: 190.0,
                sight_tiles: 3.0,
                hearing_tiles: 2.5,
                leash_tiles: 14.0,
                contact_damage: 14.0,
                windup_millis: 400,
                cooldown_millis: 1_300,
                attack_range_tiles: 2.5,
                knockback: 180.0,
                ..stats("Rock Mimic", Creature, Walking, AttackStyle::Lunge)
            },
            Self::Shellback => EnemyStats {
                radius: 12.0,
                health: 120.0,
                speed: 28.0,
                dash_speed: 170.0,
                sight_tiles: 10.0,
                hearing_tiles: 8.0,
                contact_damage: 18.0,
                windup_millis: 650,
                cooldown_millis: 1_700,
                attack_range_tiles: 3.5,
                knockback: 210.0,
                ..stats("Shellback", Creature, Walking, AttackStyle::Lunge)
            },
            Self::MagmaWisp => EnemyStats {
                radius: 8.0,
                health: 35.0,
                speed: 90.0,
                sight_tiles: 12.0,
                hearing_tiles: 6.0,
                contact_damage: 8.0,
                windup_millis: 600,
                cooldown_millis: 2_400,
                attack_range_tiles: 10.0,
                ..stats(
                    "Magma Wisp",
                    Creature,
                    Flying,
                    AttackStyle::Ranged(ShotKind::Fireball, 1),
                )
            },
            Self::BroodNest => EnemyStats {
                radius: 14.0,
                health: 160.0,
                speed: 0.0,
                sight_tiles: 14.0,
                hearing_tiles: 14.0,
                leash_tiles: 0.0,
                contact_damage: 5.0,
                windup_millis: 900,
                cooldown_millis: 6_000,
                attack_range_tiles: 14.0,
                ..stats("Brood Nest", Creature, Stationary, AttackStyle::Spawner)
            },
            Self::BroodLord => EnemyStats {
                radius: 18.0,
                // Balance pass (T19): 600 took a minute with a rivet gun and half a minute
                // with a Mk II cutter at its shallowest lair.
                health: 420.0,
                speed: 50.0,
                dash_speed: 210.0,
                sight_tiles: 14.0,
                hearing_tiles: 12.0,
                leash_tiles: 20.0,
                contact_damage: 35.0,
                windup_millis: 900,
                cooldown_millis: 2_600,
                attack_range_tiles: 8.0,
                knockback: 240.0,
                elite: true,
                ..stats(
                    "Brood Lord",
                    Creature,
                    Walking,
                    AttackStyle::Ranged(ShotKind::Acid, 3),
                )
            },
            Self::MiningDrone => EnemyStats {
                radius: 7.0,
                health: 25.0,
                speed: 120.0,
                dash_speed: 240.0,
                sight_tiles: 11.0,
                hearing_tiles: 8.0,
                contact_damage: 7.0,
                windup_millis: 450,
                cooldown_millis: 1_400,
                attack_range_tiles: 3.0,
                knockback: 150.0,
                ..stats("Mining Drone", Machine, Flying, AttackStyle::Lunge)
            },
            Self::SentryTurret => EnemyStats {
                radius: 10.0,
                health: 80.0,
                speed: 0.0,
                sight_tiles: 14.0,
                hearing_tiles: 0.0,
                leash_tiles: 0.0,
                contact_damage: 4.0,
                windup_millis: 800,
                cooldown_millis: 1_500,
                attack_range_tiles: 13.0,
                ..stats(
                    "Sentry Turret",
                    Machine,
                    Stationary,
                    AttackStyle::Ranged(ShotKind::Bolt, 1),
                )
            },
            Self::RogueExcavator => EnemyStats {
                radius: 13.0,
                health: 150.0,
                speed: 35.0,
                dash_speed: 150.0,
                sight_tiles: 10.0,
                hearing_tiles: 12.0,
                leash_tiles: 28.0,
                contact_damage: 22.0,
                windup_millis: 700,
                cooldown_millis: 1_900,
                attack_range_tiles: 4.0,
                knockback: 230.0,
                ..stats("Rogue Excavator", Machine, Walking, AttackStyle::Lunge)
            },
            Self::AncientGuardian => EnemyStats {
                radius: 18.0,
                // Balance pass (T19): 700 + a 300 shield outlasted everything but rockets and
                // the railgun (40-55 s even with those).
                health: 480.0,
                shield: 180.0,
                speed: 60.0,
                sight_tiles: 15.0,
                hearing_tiles: 10.0,
                leash_tiles: 20.0,
                contact_damage: 40.0,
                windup_millis: 1_000,
                cooldown_millis: 2_200,
                attack_range_tiles: 12.0,
                knockback: 240.0,
                elite: true,
                ..stats(
                    "Ancient Guardian",
                    Machine,
                    Flying,
                    AttackStyle::Ranged(ShotKind::Bolt, 3),
                )
            },
            Self::SkyRay => EnemyStats {
                radius: 10.0,
                health: 24.0,
                speed: 120.0,
                dash_speed: 300.0,
                sight_tiles: 14.0,
                hearing_tiles: 0.0,
                leash_tiles: 40.0,
                contact_damage: 6.0,
                windup_millis: 450,
                cooldown_millis: 2_600,
                attack_range_tiles: 9.0,
                knockback: 200.0,
                ..stats("Sky Ray", Creature, Flying, AttackStyle::Swoop)
            },
            Self::Wyvern => EnemyStats {
                radius: 16.0,
                health: 70.0,
                speed: 140.0,
                dash_speed: 360.0,
                sight_tiles: 16.0,
                hearing_tiles: 0.0,
                leash_tiles: 44.0,
                contact_damage: 12.0,
                windup_millis: 500,
                cooldown_millis: 2_800,
                attack_range_tiles: 10.0,
                knockback: 240.0,
                ..stats("Wyvern", Creature, Flying, AttackStyle::Swoop)
            },
            Self::StormElemental => EnemyStats {
                radius: 12.0,
                health: 45.0,
                speed: 70.0,
                sight_tiles: 15.0,
                hearing_tiles: 0.0,
                leash_tiles: 36.0,
                contact_damage: 5.0,
                windup_millis: 900,
                cooldown_millis: 3_200,
                attack_range_tiles: 9.0,
                knockback: 120.0,
                ..stats("Storm Elemental", Creature, Flying, AttackStyle::Lightning)
            },
            Self::PirateAirship => EnemyStats {
                radius: 20.0,
                health: 220.0,
                speed: 55.0,
                sight_tiles: 18.0,
                hearing_tiles: 0.0,
                leash_tiles: 48.0,
                contact_damage: 10.0,
                windup_millis: 1_200,
                cooldown_millis: 9_000,
                attack_range_tiles: 14.0,
                knockback: 220.0,
                elite: true,
                ..stats("Pirate Airship", Machine, Flying, AttackStyle::Boarding)
            },
            Self::AirshipTurret => EnemyStats {
                radius: 9.0,
                health: 50.0,
                speed: 0.0,
                sight_tiles: 16.0,
                hearing_tiles: 0.0,
                leash_tiles: 0.0,
                contact_damage: 4.0,
                windup_millis: 700,
                cooldown_millis: 1_800,
                attack_range_tiles: 14.0,
                ..stats(
                    "Airship Turret",
                    Machine,
                    Stationary,
                    AttackStyle::Ranged(ShotKind::Cannonball, 1),
                )
            },
            Self::SkyPirate => EnemyStats {
                radius: 9.0,
                health: 28.0,
                speed: 90.0,
                dash_speed: 240.0,
                sight_tiles: 12.0,
                hearing_tiles: 0.0,
                leash_tiles: 30.0,
                contact_damage: 7.0,
                windup_millis: 450,
                cooldown_millis: 1_500,
                attack_range_tiles: 3.0,
                knockback: 160.0,
                ..stats("Sky Pirate", Creature, Flying, AttackStyle::Lunge)
            },
            Self::HarpyNester => EnemyStats {
                radius: 12.0,
                health: 55.0,
                speed: 130.0,
                dash_speed: 330.0,
                // It sees far, but only cares near its nest (until its eggs are taken).
                sight_tiles: 18.0,
                hearing_tiles: 0.0,
                leash_tiles: 16.0,
                contact_damage: 9.0,
                windup_millis: 450,
                cooldown_millis: 2_200,
                attack_range_tiles: 8.0,
                knockback: 220.0,
                ..stats("Harpy Nester", Creature, Flying, AttackStyle::Swoop)
            },
            Self::RegolithCrawler => EnemyStats {
                radius: 10.0,
                health: 45.0,
                speed: 70.0,
                dash_speed: 250.0,
                sight_tiles: 11.0,
                hearing_tiles: 8.0,
                leash_tiles: 30.0,
                contact_damage: 12.0,
                windup_millis: 650,
                cooldown_millis: 2_300,
                attack_range_tiles: 5.0,
                knockback: 170.0,
                ..stats("Regolith Crawler", Creature, Walking, AttackStyle::Lunge)
            },
            Self::CrystalMite => EnemyStats {
                radius: 6.0,
                health: 12.0,
                speed: 140.0,
                sight_tiles: 10.0,
                hearing_tiles: 5.0,
                leash_tiles: 26.0,
                contact_damage: 5.0,
                windup_millis: 0,
                cooldown_millis: 1_000,
                attack_range_tiles: 0.6,
                knockback: 90.0,
                ..stats("Crystal Mite", Creature, Flying, AttackStyle::Contact)
            },
            Self::DerelictAutomaton => EnemyStats {
                radius: 11.0,
                health: 90.0,
                speed: 40.0,
                sight_tiles: 13.0,
                hearing_tiles: 6.0,
                leash_tiles: 22.0,
                contact_damage: 12.0,
                windup_millis: 800,
                cooldown_millis: 2_100,
                attack_range_tiles: 11.0,
                ..stats(
                    "Derelict Automaton",
                    Machine,
                    Walking,
                    AttackStyle::Ranged(ShotKind::Bolt, 2),
                )
            },
            Self::Warden => EnemyStats {
                radius: 20.0,
                health: 520.0,
                shield: 300.0,
                speed: 55.0,
                sight_tiles: 20.0,
                hearing_tiles: 16.0,
                leash_tiles: 40.0,
                contact_damage: 30.0,
                windup_millis: 900,
                cooldown_millis: 1_900,
                attack_range_tiles: 16.0,
                knockback: 260.0,
                elite: true,
                ..stats(
                    "The Warden",
                    Machine,
                    Flying,
                    AttackStyle::Ranged(ShotKind::Bolt, 4),
                )
            },
            // Night monsters (WP-NIGHT): fast enough to make the open surface tense, soft
            // enough that a rig with any weapon (or a lamp to stand under) wins. Balance probe
            // (`night`): a parked stock rig loses well under 100 hull a minute on an ordinary
            // night, so there is time to run for the light.
            Self::ShadowHound => EnemyStats {
                radius: 8.0,
                health: 34.0,
                speed: 112.0,
                dash_speed: 300.0,
                sight_tiles: 13.0,
                hearing_tiles: 8.0,
                leash_tiles: 40.0,
                contact_damage: 5.0,
                windup_millis: 650,
                cooldown_millis: 2_600,
                attack_range_tiles: 4.0,
                knockback: 170.0,
                ..stats("Shadow Hound", Creature, Walking, AttackStyle::Lunge)
            },
            Self::DuneStalker => EnemyStats {
                radius: 10.0,
                health: 60.0,
                speed: 80.0,
                dash_speed: 320.0,
                sight_tiles: 6.0,
                hearing_tiles: 11.0,
                leash_tiles: 30.0,
                contact_damage: 10.0,
                windup_millis: 750,
                cooldown_millis: 3_200,
                attack_range_tiles: 5.0,
                knockback: 200.0,
                ..stats("Dune Stalker", Creature, Burrowing, AttackStyle::Lunge)
            },
            Self::WillOWisp => EnemyStats {
                radius: 7.0,
                health: 24.0,
                speed: 70.0,
                sight_tiles: 14.0,
                hearing_tiles: 0.0,
                leash_tiles: 30.0,
                contact_damage: 4.0,
                windup_millis: 900,
                cooldown_millis: 2_800,
                attack_range_tiles: 11.0,
                knockback: 90.0,
                ..stats(
                    "Will-o'-Wisp",
                    Creature,
                    Flying,
                    AttackStyle::Ranged(ShotKind::Wispfire, 1),
                )
            },
            Self::MothSwarm => EnemyStats {
                radius: 10.0,
                health: 26.0,
                speed: 95.0,
                sight_tiles: 16.0,
                hearing_tiles: 0.0,
                leash_tiles: 40.0,
                contact_damage: 2.0,
                windup_millis: 0,
                cooldown_millis: 1_200,
                attack_range_tiles: 0.8,
                knockback: 60.0,
                ..stats("Moth Swarm", Creature, Flying, AttackStyle::Contact)
            },
            Self::UmbralAlpha => EnemyStats {
                radius: 14.0,
                health: 300.0,
                speed: 100.0,
                dash_speed: 330.0,
                sight_tiles: 16.0,
                hearing_tiles: 12.0,
                leash_tiles: 60.0,
                contact_damage: 14.0,
                windup_millis: 700,
                cooldown_millis: 2_000,
                attack_range_tiles: 5.0,
                knockback: 240.0,
                elite: true,
                ..stats("Umbral Alpha", Creature, Walking, AttackStyle::Lunge)
            },
        }
    }

    /// Whether this is one of the sky families (Wave 4 T21).
    #[must_use]
    pub const fn is_sky(self) -> bool {
        matches!(
            self,
            Self::SkyRay
                | Self::Wyvern
                | Self::StormElemental
                | Self::PirateAirship
                | Self::AirshipTurret
                | Self::SkyPirate
                | Self::HarpyNester
        )
    }

    /// Whether it lives on the Moon (WP-MOON).
    #[must_use]
    pub const fn is_lunar(self) -> bool {
        matches!(
            self,
            Self::RegolithCrawler | Self::CrystalMite | Self::DerelictAutomaton | Self::Warden
        )
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.stats().name
    }

    /// Half extents `(width, height)` of the body's contact box, pixels: how close it comes to
    /// a rig or pilot (it is pushed out to exactly touching) and how far its bites reach (plus a
    /// small margin). Roughly the drawn body, so an enemy pressed against a rig is seen touching
    /// it, not sitting on it. Wider than the terrain box ([`EnemyStats::radius`]), which stays
    /// small so enemies fit the tunnels a rig fits; shots hit the terrain box.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "one row per kind, like the stat table"
    )]
    pub const fn contact_half(self) -> (f32, f32) {
        match self {
            Self::CaveBat => (11.0, 8.0),
            Self::Burrower => (12.0, 8.0),
            Self::AcidSpitter => (13.0, 10.0),
            Self::RockMimic => (16.0, 12.0),
            Self::Shellback => (20.0, 12.0),
            Self::MagmaWisp => (9.0, 10.0),
            Self::BroodNest => (17.0, 14.0),
            Self::BroodLord => (26.0, 20.0),
            Self::MiningDrone => (10.0, 8.0),
            Self::SentryTurret => (12.0, 11.0),
            Self::RogueExcavator => (21.0, 14.0),
            Self::AncientGuardian => (22.0, 19.0),
        }
    }

    #[must_use]
    pub const fn family(self) -> EnemyFamily {
        self.stats().family
    }

    #[must_use]
    pub const fn is_machine(self) -> bool {
        matches!(self.family(), EnemyFamily::Machine)
    }

    /// Damage multiplier of a hit of `kind` on this enemy (0: immune). Armoured kinds shrug off
    /// kinetic hits and fold to explosives; machines feel EMPs, creatures do not.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "one arm per kind's quirk reads best"
    )]
    pub const fn resistance(self, kind: DamageKind) -> f32 {
        let machine = self.is_machine();
        match (self, kind) {
            // Storm elementals drink electricity and shrug off fire; airships fold to flak and
            // rockets (their gas bags).
            (Self::StormElemental, DamageKind::Shock | DamageKind::Fire) => 0.0,
            (Self::StormElemental, DamageKind::Kinetic) => 1.2,
            (Self::PirateAirship, DamageKind::Explosive) => 1.8,
            (Self::PirateAirship, DamageKind::Kinetic) => 0.5,
            (Self::Wyvern, DamageKind::Kinetic) => 0.75,
            (_, DamageKind::Shock) => {
                if machine {
                    1.3
                } else {
                    1.0
                }
            }
            (_, DamageKind::Emp) => {
                if machine {
                    1.0
                } else {
                    0.0
                }
            }
            (Self::RockMimic, DamageKind::Kinetic) => 0.3,
            (Self::Shellback, DamageKind::Kinetic) => 0.25,
            (Self::RockMimic | Self::Shellback, DamageKind::Explosive)
            | (Self::BroodNest, DamageKind::Explosive | DamageKind::Fire) => 1.6,
            (Self::RockMimic | Self::Shellback, DamageKind::Fire)
            | (Self::WillOWisp, DamageKind::Kinetic) => 0.5,
            // A wisp is flame too (WP-NIGHT).
            (Self::MagmaWisp | Self::WillOWisp, DamageKind::Fire) => 0.0,
            (Self::BroodLord | Self::CrystalMite, DamageKind::Kinetic) => 0.6,
            (
                Self::SentryTurret
                | Self::RogueExcavator
                | Self::AncientGuardian
                | Self::DerelictAutomaton
                | Self::Warden,
                DamageKind::Kinetic,
            ) => 0.55,
            (Self::CrystalMite, DamageKind::Fire | DamageKind::Explosive) => 1.3,
            (Self::CaveBat, DamageKind::Fire) => 1.5,
            // Night monsters: moths to the flame, shadows shrug off bullets.
            (Self::MothSwarm, DamageKind::Fire) => 2.0,
            (Self::MothSwarm, DamageKind::Explosive) => 1.2,
            (Self::UmbralAlpha | Self::DuneStalker, DamageKind::Kinetic) => 0.7,
            (_, DamageKind::Fire) if machine => 0.4,
            _ => 1.0,
        }
    }
}

/// What an enemy is doing.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum EnemyMode {
    /// Waiting or drifting near home.
    #[default]
    Idle,
    /// Disguised or switched off until a rig comes close (mimics, nests).
    Dormant,
    /// Going after its target.
    Chase,
    /// Telegraphing an attack (until `mode_until`).
    Windup,
    /// Dashing: a lunge or charge that hurts on contact (until `mode_until`).
    Strike,
    /// Backing off after an attack (until `mode_until`).
    Recover,
    /// Gave up the chase: heading home, healing.
    Return,
    /// Knocked out by an EMP (until `mode_until`).
    Stunned,
    /// Badly hurt: running away from its target (until `mode_until`).
    Flee,
}

impl EnemyMode {
    /// Whether the enemy is actively hostile (presentation: danger music, health bars).
    #[must_use]
    pub const fn hostile(self) -> bool {
        matches!(
            self,
            Self::Chase | Self::Windup | Self::Strike | Self::Recover
        )
    }
}

/// An enemy's per-entity state (the entity's [`crate::entity::Body`] holds its motion).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct EnemyState {
    pub kind: EnemyKind,
    /// Depth tier it spawned at (the band index): scales health and damage.
    pub tier: u8,
    pub health: f32,
    pub max_health: f32,
    /// Remaining shield (see [`EnemyStats::shield`]).
    #[serde(default)]
    pub shield: f32,
    pub mode: EnemyMode,
    /// Deadline of a timed mode (wind-up, strike, recover, stun, flee).
    pub mode_until: SimulationTick,
    /// Earliest tick of the next attack.
    pub attack_ready: SimulationTick,
    /// The rig it is after.
    pub target: Option<PlayerId>,
    /// Last tick it perceived its target.
    pub last_seen: SimulationTick,
    /// Where it was spawned (the leash anchor), in tiles.
    pub home: (i32, i32),
    /// Next tile of its current path, if it is following one.
    pub waypoint: Option<(i32, i32)>,
    /// Earliest tick of its next path search.
    pub next_path: SimulationTick,
    /// Facing: -1 left, 1 right.
    pub facing: i8,
    /// Damage flash until this tick (presentation).
    pub hurt_until: SimulationTick,
    /// Burning until this tick (creatures hit by fire).
    pub burning_until: SimulationTick,
    /// The lair chunk it guards, if it is a lair occupant (killing it clears the lair).
    pub lair: Option<ChunkPosition>,
    /// The nest that spawned it, if any (nests count their brood).
    pub parent: Option<u64>,
    /// Per-enemy variation seed (wander, animation phase).
    pub seed: u32,
    /// Last player that hurt it (kill credit).
    pub last_attacker: Option<PlayerId>,
}

impl EnemyState {
    /// A fresh enemy of `kind` at depth tier `tier`, home tile `home`, with health scaled by
    /// `health_scale` (difficulty and crew size).
    #[must_use]
    pub fn new(
        kind: EnemyKind,
        tier: u8,
        home: (i32, i32),
        health_scale: f32,
        seed: u32,
        now: SimulationTick,
    ) -> Self {
        let stats = kind.stats();
        let health = (stats.health * tier_scale(tier) * health_scale)
            .round()
            .max(1.0);
        let dormant = matches!(kind, EnemyKind::RockMimic);
        Self {
            kind,
            tier,
            health,
            max_health: health,
            shield: (stats.shield * tier_scale(tier)).round(),
            mode: if dormant {
                EnemyMode::Dormant
            } else {
                EnemyMode::Idle
            },
            mode_until: now,
            attack_ready: now,
            target: None,
            last_seen: now,
            home,
            waypoint: None,
            next_path: now,
            facing: if seed & 1 == 0 { 1 } else { -1 },
            hurt_until: now,
            burning_until: now,
            lair: None,
            parent: None,
            seed,
            last_attacker: None,
        }
    }

    /// Health as a fraction of the maximum.
    #[must_use]
    pub fn health_fraction(&self) -> f32 {
        if self.max_health <= 0.0 {
            0.0
        } else {
            (self.health / self.max_health).clamp(0.0, 1.0)
        }
    }
}

/// A projectile an enemy fired (an [`crate::entity::EntityState::EnemyShot`]).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct EnemyShotState {
    pub kind: ShotKind,
    /// Hull damage on a hit (already scaled).
    pub damage: f32,
    /// Who fired it.
    pub shooter: EnemyKind,
}

/// Health and damage multiplier of depth tier `tier` (band index): deeper enemies hit harder,
/// like deeper rigs carry more hull.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a dozen tiers")]
pub fn tier_scale(tier: u8) -> f32 {
    (f32::from(tier)).mul_add(0.32, 1.0)
}

/// The world's enemy setting.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum EnemyDifficulty {
    /// No hostiles at all (existing ones leave).
    Peaceful,
    /// Threatening but fair.
    #[default]
    Normal,
    /// More of them, tougher and meaner.
    Hard,
}

impl EnemyDifficulty {
    pub const ALL: [Self; 3] = [Self::Peaceful, Self::Normal, Self::Hard];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Peaceful => "Peaceful",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
        }
    }

    /// One-line description for menus.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Peaceful => "No hostile creatures or machines",
            Self::Normal => "Threatening but fair",
            Self::Hard => "More enemies, tougher and meaner",
        }
    }

    /// Whether hostiles exist at all.
    #[must_use]
    pub const fn hostile(self) -> bool {
        !matches!(self, Self::Peaceful)
    }

    /// Spawn density multiplier.
    #[must_use]
    pub const fn density(self) -> f32 {
        match self {
            Self::Peaceful => 0.0,
            Self::Normal => 1.0,
            Self::Hard => 1.6,
        }
    }

    /// Damage dealt to rigs multiplier.
    #[must_use]
    pub const fn damage(self) -> f32 {
        match self {
            Self::Peaceful => 0.0,
            Self::Normal => 1.0,
            Self::Hard => 1.5,
        }
    }

    /// Enemy health multiplier.
    #[must_use]
    pub const fn health(self) -> f32 {
        match self {
            Self::Peaceful | Self::Normal => 1.0,
            Self::Hard => 1.3,
        }
    }

    /// Attack cooldown multiplier (lower is faster).
    #[must_use]
    pub const fn cooldown(self) -> f32 {
        match self {
            Self::Peaceful | Self::Normal => 1.0,
            Self::Hard => 0.8,
        }
    }

    /// Extra enemies allowed around each rig.
    #[must_use]
    pub const fn extra_cap(self) -> usize {
        match self {
            Self::Peaceful | Self::Normal => 0,
            Self::Hard => 3,
        }
    }
}

/// Spawn density multiplier of a world's hazard intensity (calm worlds are quieter).
#[must_use]
pub const fn hazard_density(hazards: HazardIntensity) -> f32 {
    match hazards {
        HazardIntensity::Calm => 0.6,
        HazardIntensity::Standard => 1.0,
        HazardIntensity::Brutal => 1.4,
    }
}

/// A chunk's spawn cooldown (see [`EnemyDirector::chunk_cooldowns`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChunkCooldown {
    pub chunk: ChunkPosition,
    pub until: SimulationTick,
}

/// World-level enemy state: saved, hashed and replicated as the shared `Enemies` section.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnemyDirector {
    /// The world's enemy setting.
    #[serde(default)]
    pub difficulty: EnemyDifficulty,
    /// Each rig's invulnerability window after an enemy hit (i-frames): no enemy hurts it
    /// before this tick.
    #[serde(default)]
    pub rig_guard: BTreeMap<PlayerId, SimulationTick>,
    /// Chunks that spawned recently and may not spawn again before their tick (sorted by
    /// chunk; expired entries are pruned).
    #[serde(default)]
    pub chunk_cooldowns: Vec<ChunkCooldown>,
    /// Lair chunks whose occupant was killed: they never refill.
    #[serde(default)]
    pub cleared_lairs: BTreeSet<ChunkPosition>,
    /// Enemies killed in this world.
    #[serde(default)]
    pub kills: u64,
    /// Night facts (WP-NIGHT): the night under way, night kills, nights survived, the last
    /// rare night.
    #[serde(default)]
    pub night: crate::night::NightLedger,
}

impl EnemyDirector {
    /// Whether this is the untouched default (old saves; the state hash skips it then).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether `chunk` is cooling down at `now`.
    #[must_use]
    pub fn chunk_cooling(&self, chunk: ChunkPosition, now: SimulationTick) -> bool {
        self.chunk_cooldowns
            .binary_search_by_key(&chunk, |entry| entry.chunk)
            .is_ok_and(|index| self.chunk_cooldowns[index].until > now)
    }

    /// Starts `chunk`'s cooldown until `until`.
    pub fn cool_chunk(&mut self, chunk: ChunkPosition, until: SimulationTick) {
        match self
            .chunk_cooldowns
            .binary_search_by_key(&chunk, |entry| entry.chunk)
        {
            Ok(index) => self.chunk_cooldowns[index].until = until,
            Err(index) => self
                .chunk_cooldowns
                .insert(index, ChunkCooldown { chunk, until }),
        }
    }

    /// Drops expired cooldowns and rig guards. Returns whether anything changed.
    pub fn prune(&mut self, now: SimulationTick) -> bool {
        let before = (self.chunk_cooldowns.len(), self.rig_guard.len());
        self.chunk_cooldowns.retain(|entry| entry.until > now);
        self.rig_guard.retain(|_, until| *until > now);
        before != (self.chunk_cooldowns.len(), self.rig_guard.len())
    }

    /// Whether `player` is inside its post-hit invulnerability window at `now`.
    #[must_use]
    pub fn guarded(&self, player: PlayerId, now: SimulationTick) -> bool {
        self.rig_guard
            .get(&player)
            .is_some_and(|until| *until > now)
    }
}

/// The kinds that roam band `band`, with relative spawn weights (lair occupants excluded).
#[must_use]
pub const fn band_roster(band: Band) -> &'static [(EnemyKind, u32)] {
    use EnemyKind::{
        AcidSpitter, Burrower, CaveBat, MagmaWisp, MiningDrone, RockMimic, RogueExcavator,
        SentryTurret, Shellback,
    };
    match band {
        Band::Topsoil => &[],
        Band::Clay => &[(CaveBat, 6), (Burrower, 2)],
        Band::Stone => &[
            (CaveBat, 5),
            (Burrower, 3),
            (AcidSpitter, 3),
            (RockMimic, 2),
        ],
        Band::Slate => &[
            (CaveBat, 4),
            (AcidSpitter, 3),
            (RockMimic, 2),
            (Shellback, 2),
            (Burrower, 2),
        ],
        Band::Basalt => &[
            (Shellback, 3),
            (AcidSpitter, 3),
            (MiningDrone, 2),
            (Burrower, 2),
            (CaveBat, 2),
        ],
        Band::Deep(DeepStratum::CrystalFaults) => &[
            (CaveBat, 4),
            (AcidSpitter, 3),
            (RockMimic, 3),
            (MiningDrone, 2),
        ],
        Band::Deep(DeepStratum::FossilOceans) => &[
            (Burrower, 4),
            (Shellback, 3),
            (CaveBat, 3),
            (AcidSpitter, 2),
        ],
        Band::Deep(DeepStratum::PressureCathedrals) => &[
            (RockMimic, 3),
            (Shellback, 3),
            (MiningDrone, 3),
            (SentryTurret, 1),
        ],
        Band::Deep(DeepStratum::RadioactiveHollow) => &[
            (AcidSpitter, 4),
            (CaveBat, 3),
            (RogueExcavator, 2),
            (Burrower, 2),
        ],
        Band::Deep(DeepStratum::AncientMachineLayer) => {
            &[(MiningDrone, 5), (SentryTurret, 3), (RogueExcavator, 3)]
        }
        Band::Deep(DeepStratum::VoidGeodeFields) => &[
            (MiningDrone, 3),
            (RockMimic, 3),
            (CaveBat, 3),
            (Shellback, 2),
        ],
        Band::Deep(DeepStratum::MantleStormZone) => {
            &[(MagmaWisp, 5), (RogueExcavator, 3), (MiningDrone, 2)]
        }
    }
}

/// The kinds that roam sky band `band` of a V9 sky, with relative spawn weights.
///
/// Harpy nesters live at their nests, turrets and pirates come with airships. Empty for the
/// ground, the low isles (early hopping stays calm), orbit and the Moon (WP-MOON's).
#[must_use]
pub const fn sky_roster(band: crate::sky::SkyBand) -> &'static [(EnemyKind, u32)] {
    use crate::sky::SkyBand;
    use EnemyKind::{PirateAirship, SkyRay, StormElemental, Wyvern};
    match band {
        SkyBand::Archipelago => &[(SkyRay, 5), (StormElemental, 1)],
        SkyBand::DeadAir => &[(StormElemental, 4), (SkyRay, 2), (PirateAirship, 1)],
        SkyBand::Continents => &[
            (SkyRay, 4),
            (Wyvern, 3),
            (StormElemental, 2),
            (PirateAirship, 1),
        ],
        SkyBand::Ground | SkyBand::LowIsles | SkyBand::Orbit | SkyBand::Lunar => &[],
    }
}

/// The tier (health and damage scale, loot) of enemies in sky band `band`: about the deep
/// layers' tiers, since the sky is Act II.
#[must_use]
pub const fn sky_tier(band: crate::sky::SkyBand) -> u8 {
    use crate::sky::SkyBand;
    match band {
        SkyBand::Ground | SkyBand::LowIsles => 3,
        SkyBand::Archipelago => 5,
        SkyBand::DeadAir => 6,
        SkyBand::Continents => 7,
        SkyBand::Orbit | SkyBand::Lunar => 8,
    }
}

/// The occupant of a lair cavern in band `band` (`roll` picks among the band's options), or
/// `None` where the band has no lairs.
#[must_use]
pub const fn lair_occupant(band: Band, roll: u64) -> Option<EnemyKind> {
    use EnemyKind::{AncientGuardian, BroodLord, BroodNest, SentryTurret};
    let elite = roll.is_multiple_of(4);
    Some(match band {
        Band::Topsoil | Band::Clay => return None,
        Band::Stone
        | Band::Slate
        | Band::Basalt
        | Band::Deep(DeepStratum::CrystalFaults | DeepStratum::RadioactiveHollow) => BroodNest,
        Band::Deep(DeepStratum::FossilOceans | DeepStratum::VoidGeodeFields) => {
            if elite {
                BroodLord
            } else {
                BroodNest
            }
        }
        Band::Deep(DeepStratum::PressureCathedrals) => SentryTurret,
        Band::Deep(DeepStratum::AncientMachineLayer | DeepStratum::MantleStormZone) => {
            if elite {
                AncientGuardian
            } else {
                SentryTurret
            }
        }
    })
}

/// The ore a creature of depth tier `tier` carries in its gizzard.
#[must_use]
pub const fn tier_mineral(tier: u8) -> MineralKind {
    match tier {
        0 | 1 => MineralKind::Copper,
        2 => MineralKind::Iron,
        3 => MineralKind::Silver,
        4 => MineralKind::Gold,
        5 => MineralKind::Emerald,
        6 => MineralKind::Ruby,
        7 => MineralKind::Diamond,
        8 => MineralKind::Platinum,
        9 => MineralKind::Uranium,
        _ => MineralKind::Mythril,
    }
}

/// What an enemy of `kind` at depth tier `tier` drops when it dies; `roll` is a deterministic
/// per-death hash (varies counts and the rare extras).
#[must_use]
#[allow(clippy::too_many_lines, reason = "the loot table, one arm per kind")]
pub fn loot(kind: EnemyKind, tier: u8, roll: u64) -> Vec<ItemStack> {
    let mineral = ItemKind::Mineral(tier_mineral(tier));
    let richer = ItemKind::Mineral(tier_mineral(tier.saturating_add(1)));
    let extra = u32::try_from(roll % 3).unwrap_or(0);
    let rare = roll % 100;
    let mut drops = Vec::new();
    let mut add = |kind: ItemKind, count: u32| {
        if count > 0 {
            drops.push(ItemStack::new(kind, count));
        }
    };
    match kind {
        EnemyKind::CaveBat => {
            if rare < 35 {
                add(mineral, 1);
            }
        }
        EnemyKind::Burrower => add(mineral, 1 + extra),
        EnemyKind::AcidSpitter => {
            add(mineral, 1 + extra / 2);
            if rare < 10 {
                add(ItemKind::Material(StrategicResourceKind::CrystalLens), 1);
            }
        }
        EnemyKind::RockMimic => {
            add(richer, 2 + extra);
            if rare < 20 {
                add(ItemKind::Material(StrategicResourceKind::PressurePearl), 1);
            }
        }
        EnemyKind::Shellback => {
            add(mineral, 2 + extra);
            add(richer, 1);
        }
        EnemyKind::MagmaWisp => {
            add(ItemKind::Material(StrategicResourceKind::CoreShard), 1);
            if rare < 30 {
                add(richer, 1);
            }
        }
        EnemyKind::BroodNest => {
            add(richer, 3 + extra);
            add(ItemKind::Material(StrategicResourceKind::RadiantFossil), 1);
        }
        EnemyKind::BroodLord => {
            add(richer, 6 + extra);
            add(ItemKind::Material(StrategicResourceKind::RadiantFossil), 2);
            add(
                ItemKind::Artifact(if rare < 50 {
                    ArtifactKind::RadiantShell
                } else {
                    ArtifactKind::Fossil
                }),
                1,
            );
        }
        EnemyKind::MiningDrone => {
            add(ItemKind::Material(StrategicResourceKind::MachineRelic), 1);
            if rare < 40 {
                add(mineral, 1);
            }
        }
        EnemyKind::SentryTurret => {
            add(
                ItemKind::Material(StrategicResourceKind::MachineRelic),
                1 + extra / 2,
            );
            add(ItemKind::Material(StrategicResourceKind::AncientAlloy), 1);
        }
        EnemyKind::RogueExcavator => {
            add(
                ItemKind::Material(StrategicResourceKind::AncientAlloy),
                1 + extra / 2,
            );
            add(richer, 2 + extra);
        }
        EnemyKind::SkyRay => {
            add(mineral, 1);
            if rare < 25 {
                add(ItemKind::Material(StrategicResourceKind::StormGlass), 1);
            }
        }
        EnemyKind::Wyvern => {
            add(richer, 2 + extra);
            add(ItemKind::Material(StrategicResourceKind::StormGlass), 1);
        }
        EnemyKind::StormElemental => {
            add(mineral, 1);
            add(
                ItemKind::Material(StrategicResourceKind::StormGlass),
                2 + extra / 2,
            );
        }
        EnemyKind::PirateAirship => {
            add(richer, 6 + extra);
            add(ItemKind::Material(StrategicResourceKind::AncientAlloy), 2);
            add(ItemKind::Material(StrategicResourceKind::MachineRelic), 1);
            if rare < 30 {
                add(ItemKind::Artifact(ArtifactKind::SkyRelic), 1);
            }
        }
        EnemyKind::AirshipTurret => {
            add(ItemKind::Material(StrategicResourceKind::MachineRelic), 1);
            if rare < 50 {
                add(mineral, 1);
            }
        }
        EnemyKind::SkyPirate => {
            add(mineral, 1 + extra / 2);
            if rare < 5 {
                add(ItemKind::Artifact(ArtifactKind::SkyRelic), 1);
            }
        }
        EnemyKind::HarpyNester => {
            add(richer, 2);
            add(ItemKind::Material(StrategicResourceKind::AetherCrystal), 1);
        }
        EnemyKind::AncientGuardian => {
            add(ItemKind::Material(StrategicResourceKind::MachineRelic), 3);
            add(ItemKind::Material(StrategicResourceKind::AncientAlloy), 2);
            add(
                ItemKind::Artifact(if rare < 50 {
                    ArtifactKind::MachineSigil
                } else {
                    ArtifactKind::OldCircuit
                }),
                1,
            );
            add(richer, 4 + extra);
        }
        EnemyKind::RegolithCrawler => {
            add(
                ItemKind::Material(StrategicResourceKind::Helium3),
                1 + extra / 2,
            );
            if rare < 6 {
                add(ItemKind::Artifact(ArtifactKind::LunarRelic), 1);
            }
        }
        EnemyKind::CrystalMite => {
            if rare < 40 {
                add(ItemKind::Material(StrategicResourceKind::CrystalLens), 1);
            } else {
                add(ItemKind::Material(StrategicResourceKind::Helium3), 1);
            }
        }
        EnemyKind::DerelictAutomaton => {
            add(ItemKind::Material(StrategicResourceKind::AlienAlloy), 1);
            add(
                ItemKind::Material(StrategicResourceKind::MachineRelic),
                1 + extra / 2,
            );
        }
        EnemyKind::Warden => {
            add(ItemKind::Artifact(ArtifactKind::WardenEye), 1);
            add(
                ItemKind::Material(StrategicResourceKind::AlienAlloy),
                6 + extra,
            );
        }
        // Night monsters drop night-only materials (WP-NIGHT): the night market pays well.
        EnemyKind::ShadowHound => {
            if rare < 65 {
                add(ItemKind::Material(StrategicResourceKind::ShadowPelt), 1);
            }
            if rare >= 50 {
                add(mineral, 1);
            }
        }
        EnemyKind::DuneStalker => {
            add(ItemKind::Material(StrategicResourceKind::StalkerFang), 1);
            add(ItemKind::Mineral(MineralKind::Gold), 1 + extra / 2);
        }
        EnemyKind::WillOWisp => {
            add(ItemKind::Material(StrategicResourceKind::WispEssence), 1);
            if rare < 30 {
                add(richer, 1);
            }
        }
        EnemyKind::MothSwarm => {
            add(
                ItemKind::Material(StrategicResourceKind::MothSilk),
                1 + extra / 2,
            );
            if rare < 25 {
                add(mineral, 1);
            }
        }
        EnemyKind::UmbralAlpha => {
            add(ItemKind::Material(StrategicResourceKind::UmbralHeart), 1);
            add(ItemKind::Material(StrategicResourceKind::ShadowPelt), 3);
            add(richer, 4 + extra);
            add(
                ItemKind::Artifact(if rare < 50 {
                    ArtifactKind::RadiantShell
                } else {
                    ArtifactKind::Fossil
                }),
                1,
            );
        }
    }
    drops
}

/// The kinds that roam the Moon with relative spawn weights: crawlers everywhere, mites
/// thickest in the crystal forests, automatons around abandoned habitats.
#[must_use]
pub const fn lunar_roster(in_forest: bool, near_habitat: bool) -> &'static [(EnemyKind, u32)] {
    use EnemyKind::{CrystalMite, DerelictAutomaton, RegolithCrawler};
    match (in_forest, near_habitat) {
        (_, true) => &[
            (DerelictAutomaton, 5),
            (RegolithCrawler, 2),
            (CrystalMite, 1),
        ],
        (true, false) => &[(CrystalMite, 5), (RegolithCrawler, 2)],
        (false, false) => &[
            (RegolithCrawler, 5),
            (CrystalMite, 2),
            (DerelictAutomaton, 1),
        ],
    }
}

/// The depth tier Moon enemies scale at (between the deep strata and the mantle).
pub const LUNAR_TIER: u8 = 7;

/// Tiles to world pixels (centre of the tile).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "tile coordinates are far inside f32 range"
)]
pub fn tile_centre(x: i32, y: i32) -> (f32, f32) {
    ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_sane_stats_and_a_counter() {
        for kind in EnemyKind::ALL {
            let stats = kind.stats();
            assert!(stats.health > 0.0, "{kind:?}");
            assert!(stats.radius >= 6.0 && stats.radius <= 20.0, "{kind:?}");
            assert!(stats.contact_damage > 0.0, "{kind:?}");
            // Every kind takes full (or better) damage from explosives or EMPs: a clear counter.
            assert!(
                kind.resistance(DamageKind::Explosive) >= 1.0
                    || kind.resistance(DamageKind::Emp) >= 1.0,
                "{kind:?}"
            );
            if matches!(stats.locomotion, Locomotion::Stationary) {
                assert!(stats.speed.abs() < f32::EPSILON, "{kind:?}");
            }
        }
        assert!(EnemyKind::Shellback.resistance(DamageKind::Kinetic) < 0.5);
        assert!(EnemyKind::CaveBat.resistance(DamageKind::Emp) <= 0.0);
        assert!(EnemyKind::MiningDrone.resistance(DamageKind::Emp) >= 1.0);
        assert!(EnemyKind::MagmaWisp.resistance(DamageKind::Fire) <= 0.0);
    }

    #[test]
    fn rosters_scale_with_depth_and_keep_the_topsoil_safe() {
        assert!(band_roster(Band::Topsoil).is_empty());
        assert!(lair_occupant(Band::Clay, 0).is_none());
        for band in Band::ALL.into_iter().skip(1) {
            assert!(!band_roster(band).is_empty(), "{band:?}");
        }
        // Machines only from the basalt down; the machine layer is all machines.
        for band in [Band::Clay, Band::Stone, Band::Slate] {
            assert!(band_roster(band).iter().all(|(kind, _)| !kind.is_machine()));
        }
        assert!(
            band_roster(Band::Deep(DeepStratum::AncientMachineLayer))
                .iter()
                .all(|(kind, _)| kind.is_machine())
        );
        assert!(
            band_roster(Band::Deep(DeepStratum::MantleStormZone))
                .iter()
                .any(|(kind, _)| *kind == EnemyKind::MagmaWisp)
        );
        assert_eq!(
            lair_occupant(Band::Deep(DeepStratum::AncientMachineLayer), 0),
            Some(EnemyKind::AncientGuardian)
        );
    }

    #[test]
    fn every_kind_drops_something_worth_more_deeper() {
        for kind in EnemyKind::ALL {
            let shallow_items: u32 = (0..20)
                .flat_map(|roll| loot(kind, 2, roll))
                .map(|stack| stack.count)
                .sum();
            let shallow: u32 = (0..20)
                .flat_map(|roll| loot(kind, 2, roll))
                .map(|stack| stack.kind.base_value() * stack.count)
                .sum();
            let deep: u32 = (0..20)
                .flat_map(|roll| loot(kind, 9, roll))
                .map(|stack| stack.kind.base_value() * stack.count)
                .sum();
            assert!(shallow_items > 0, "{kind:?}");
            assert!(deep >= shallow, "{kind:?}");
        }
    }

    #[test]
    fn the_director_cools_chunks_and_guards_rigs() {
        let mut director = EnemyDirector::default();
        assert!(director.is_default());
        let chunk = ChunkPosition { x: 3, y: 4 };
        director.cool_chunk(chunk, SimulationTick::new(10));
        director.cool_chunk(ChunkPosition { x: -1, y: 4 }, SimulationTick::new(5));
        assert!(director.chunk_cooling(chunk, SimulationTick::new(9)));
        assert!(!director.chunk_cooling(chunk, SimulationTick::new(10)));
        director
            .rig_guard
            .insert(PlayerId::new(1), SimulationTick::new(7));
        assert!(director.guarded(PlayerId::new(1), SimulationTick::new(6)));
        assert!(director.prune(SimulationTick::new(8)));
        assert_eq!(director.chunk_cooldowns.len(), 1);
        assert!(director.rig_guard.is_empty());
        let json = serde_json::to_string(&director).expect("serializes");
        let back: EnemyDirector = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, director);
    }
}
