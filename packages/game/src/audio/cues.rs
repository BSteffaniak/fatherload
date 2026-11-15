//! What the game asks to hear: [`SoundCue`]s placed at a source, the cue -> patch table, and the
//! gameplay event -> cue table. Pure data, no device access.

use fatherload_simulation::{
    EnemyKind, ExplosiveKind, GameplayEventKind, PlayerId, PoiKind, ShotKind, WeaponKind,
    event::ImpactSurface,
    explosive::Deployment,
    item::{ItemCategory, ItemKind},
    village::VillageService,
};

use super::patch::Patch;
use crate::shell::SoundCue;

/// Where a cue comes from, which decides its pan and distance attenuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundSource {
    /// Interface and global sounds: centred, never attenuated.
    Interface,
    /// A player's rig: a local seat's sounds pan toward its split-screen view; other players'
    /// rigs are placed in the world.
    Player(PlayerId),
    /// A point in the world (world pixels): attenuated by distance to the nearest local rig and
    /// panned by its side.
    World { x: i32, y: i32 },
}

/// A cue and where it comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacedCue {
    pub cue: SoundCue,
    pub source: SoundSource,
}

impl PlacedCue {
    /// An interface (centred) cue.
    #[must_use]
    pub const fn interface(cue: SoundCue) -> Self {
        Self {
            cue,
            source: SoundSource::Interface,
        }
    }

    /// A cue from `player`'s rig.
    #[must_use]
    pub const fn player(cue: SoundCue, player: PlayerId) -> Self {
        Self {
            cue,
            source: SoundSource::Player(player),
        }
    }

    /// A cue at world position `(x, y)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::missing_const_for_fn,
        reason = "world pixel positions are far inside i32; float rounding is not const"
    )]
    pub fn world(cue: SoundCue, x: f32, y: f32) -> Self {
        Self {
            cue,
            source: SoundSource::World {
                x: x.round() as i32,
                y: y.round() as i32,
            },
        }
    }
}

/// The designed patch each cue plays.
#[must_use]
pub const fn cue_patch(cue: SoundCue) -> Patch {
    match cue {
        SoundCue::Drill => Patch::DrillBite,
        SoundCue::Sell => Patch::Coins,
        SoundCue::Upgrade => Patch::Purchase,
        SoundCue::Damage => Patch::HullHit,
        SoundCue::Milestone => Patch::Milestone,
        SoundCue::Rescue => Patch::Rescue,
        SoundCue::Explosion => Patch::Explosion,
        SoundCue::Ui => Patch::UiConfirm,
        SoundCue::UiMove => Patch::UiMove,
        SoundCue::UiBack => Patch::UiBack,
        SoundCue::UiError => Patch::UiError,
        SoundCue::Pickup => Patch::Pickup,
        SoundCue::BlockPickup => Patch::BlockPickup,
        SoundCue::ArtifactPickup => Patch::ArtifactPickup,
        SoundCue::StowageFull => Patch::StowageFull,
        SoundCue::BlocksDumped => Patch::BlockDump,
        SoundCue::BombPlaced => Patch::BombArm,
        SoundCue::BombBounce => Patch::LandLight,
        SoundCue::InfrastructurePlaced => Patch::Deploy,
        SoundCue::Scanner => Patch::ScannerPulse,
        SoundCue::PoiDiscovered => Patch::PoiDiscovered,
        SoundCue::CrateOpened => Patch::CrateOpen,
        SoundCue::SalvageOpened => Patch::SalvageOpen,
        SoundCue::VaultOpened => Patch::VaultFanfare,
        SoundCue::VaultReset => Patch::VaultReset,
        SoundCue::SprayPaint => Patch::SprayPaint,
        SoundCue::ExplosiveThrow => Patch::ThrowWhoosh,
        SoundCue::ChargeStuck => Patch::ChargeStick,
        SoundCue::Detonator => Patch::Detonator,
        SoundCue::SmallBlast => Patch::SmallBlast,
        SoundCue::ClusterBurst => Patch::ClusterBurst,
        SoundCue::ShapedBlast => Patch::ShapedBlast,
        SoundCue::DeepBlast => Patch::DeepBlast,
        SoundCue::FireWhoosh => Patch::FireWhoosh,
        SoundCue::FireCrackle => Patch::FireCrackle,
        SoundCue::EmpPulse => Patch::EmpPulse,
        SoundCue::FlareIgnite => Patch::FlareIgnite,
        SoundCue::CaveDiscovered => Patch::CaveDiscovered,
        SoundCue::StalactiteCrack => Patch::StalactiteCrack,
        SoundCue::CacheOpened => Patch::CacheOpen,
        SoundCue::VillageArrival => Patch::VillageArrival,
        SoundCue::ShrineBlessing => Patch::ShrineChime,
        SoundCue::CreatureScreech => Patch::CreatureScreech,
        SoundCue::CreatureHiss => Patch::CreatureHiss,
        SoundCue::AcidSpit => Patch::AcidSpit,
        SoundCue::CreatureHurt => Patch::CreatureHurt,
        SoundCue::CreatureDeath => Patch::CreatureDeath,
        SoundCue::MachineAlert => Patch::MachineAlert,
        SoundCue::MachineCharge => Patch::MachineCharge,
        SoundCue::MachineZap => Patch::MachineZap,
        SoundCue::MachineHurt => Patch::MachineHurt,
        SoundCue::MachineDeath => Patch::MachineDeath,
        SoundCue::EnemyBite => Patch::EnemyBite,
        SoundCue::SmokeBurst => Patch::SmokeBurst,
        SoundCue::ScareBang => Patch::ScareBang,
        SoundCue::RivetShot => Patch::RivetShot,
        SoundCue::ScatterShot => Patch::ScatterShot,
        SoundCue::CutterZap => Patch::CutterZap,
        SoundCue::RocketLaunch => Patch::RocketLaunch,
        SoundCue::HarpoonShot => Patch::HarpoonShot,
        SoundCue::RailgunShot => Patch::RailgunShot,
        SoundCue::WeaponDry => Patch::WeaponDry,
        SoundCue::WeaponMount => Patch::WeaponMount,
        SoundCue::ImpactRock => Patch::ImpactRock,
        SoundCue::ImpactHit => Patch::ImpactHit,
        SoundCue::TargetDestroyed => Patch::TargetDestroyed,
    }
}

/// The cue of an enemy event: creatures screech, hiss, squelch and wail; machines beep, whine,
/// clank and blow apart (exhaustive: a new kind picks its family's sounds).
#[must_use]
pub const fn enemy_cue(kind: EnemyKind, creature: SoundCue, machine: SoundCue) -> SoundCue {
    if kind.is_machine() { machine } else { creature }
}

/// The sound of an enemy projectile being fired (exhaustive: a new shot needs a sound).
#[must_use]
pub const fn shot_cue(shot: ShotKind) -> SoundCue {
    match shot {
        ShotKind::Acid => SoundCue::AcidSpit,
        ShotKind::Fireball => SoundCue::FireWhoosh,
        ShotKind::Bolt => SoundCue::MachineZap,
    }
}

/// The blast each explosive kind goes off with (exhaustive: a new kind needs a sound).
#[must_use]
pub const fn detonation_cue(kind: ExplosiveKind) -> SoundCue {
    match kind {
        ExplosiveKind::Bomb | ExplosiveKind::Sticky | ExplosiveKind::C4 => SoundCue::Explosion,
        ExplosiveKind::Cluster | ExplosiveKind::Bomblet | ExplosiveKind::Rocket => {
            SoundCue::SmallBlast
        }
        ExplosiveKind::Shaped => SoundCue::ShapedBlast,
        ExplosiveKind::Mining => SoundCue::DeepBlast,
        ExplosiveKind::Incendiary => SoundCue::FireWhoosh,
        ExplosiveKind::Emp => SoundCue::EmpPulse,
        ExplosiveKind::Flare => SoundCue::FlareIgnite,
        ExplosiveKind::Smoke => SoundCue::SmokeBurst,
        ExplosiveKind::Scare => SoundCue::ScareBang,
    }
}

/// The shot each weapon fires with (exhaustive: a new weapon needs a sound).
#[must_use]
pub const fn weapon_cue(weapon: WeaponKind) -> SoundCue {
    match weapon {
        WeaponKind::RivetGun => SoundCue::RivetShot,
        WeaponKind::ScatterBlaster => SoundCue::ScatterShot,
        WeaponKind::PlasmaCutter => SoundCue::CutterZap,
        WeaponKind::RocketLauncher => SoundCue::RocketLaunch,
        WeaponKind::Harpoon => SoundCue::HarpoonShot,
        WeaponKind::Railgun => SoundCue::RailgunShot,
    }
}

/// The sound of a shot striking `surface`.
#[must_use]
pub const fn impact_cue(surface: ImpactSurface) -> SoundCue {
    match surface {
        ImpactSurface::Terrain => SoundCue::ImpactRock,
        ImpactSurface::Target | ImpactSurface::Charge | ImpactSurface::Loot => SoundCue::ImpactHit,
    }
}

/// The sound of deploying `kind`: a whoosh for throws, the arming clank for drops and placed
/// charges.
#[must_use]
pub const fn deploy_cue(kind: ExplosiveKind) -> SoundCue {
    match kind.deployment() {
        Deployment::Throw => SoundCue::ExplosiveThrow,
        Deployment::Drop | Deployment::Place => SoundCue::BombPlaced,
    }
}

/// The sound of picking up `item` (WP-DROPS): a clack for blocks, the bell sparkle for ore and
/// materials, a richer chime for artifacts.
#[must_use]
pub const fn pickup_cue(item: ItemKind) -> SoundCue {
    match item.category() {
        ItemCategory::Block => SoundCue::BlockPickup,
        ItemCategory::Artifact => SoundCue::ArtifactPickup,
        ItemCategory::Mineral
        | ItemCategory::Material
        | ItemCategory::Module
        | ItemCategory::Weapon
        | ItemCategory::Ammo
        | ItemCategory::Explosive
        | ItemCategory::Cosmetic
        | ItemCategory::Quest => SoundCue::Pickup,
    }
}

/// The one-shot cue a gameplay event plays, if any. Exhaustive on purpose: a new event kind
/// fails to compile until someone decides how it sounds. Events answered `None` are covered
/// elsewhere (noted per arm).
#[must_use]
pub const fn event_cue(kind: &GameplayEventKind) -> Option<SoundCue> {
    match kind {
        GameplayEventKind::DrillStarted { .. } => Some(SoundCue::Drill),
        // Only when the bay grew (selling also changes cargo; it gets coins).
        GameplayEventKind::CargoChanged { .. } => Some(SoundCue::Pickup),
        GameplayEventKind::BombPlaced { .. } => Some(SoundCue::BombPlaced),
        GameplayEventKind::InfrastructurePlaced { .. } => Some(SoundCue::InfrastructurePlaced),
        GameplayEventKind::ScannerUsed { .. } => Some(SoundCue::Scanner),
        // Milestones, including the quest fanfare when a job is ready or paid.
        GameplayEventKind::EscapeStarted { .. }
        | GameplayEventKind::DeepRewardGranted { .. }
        | GameplayEventKind::QuestReady { .. }
        | GameplayEventKind::QuestCompleted { .. } => Some(SoundCue::Milestone),
        GameplayEventKind::RescueTriggered { .. } => Some(SoundCue::Rescue),
        GameplayEventKind::PlayerDamaged { .. } => Some(SoundCue::Damage),
        GameplayEventKind::BombDetonated { .. } => Some(SoundCue::Explosion),
        GameplayEventKind::BombBounced { .. } => Some(SoundCue::BombBounce),
        GameplayEventKind::ExplosiveDeployed { kind, .. } => Some(deploy_cue(*kind)),
        GameplayEventKind::RemoteDetonation { .. } => Some(SoundCue::Detonator),
        GameplayEventKind::ChargeStuck { .. } => Some(SoundCue::ChargeStuck),
        GameplayEventKind::ExplosiveDetonated { kind, .. } => Some(detonation_cue(*kind)),
        GameplayEventKind::ClusterSplit { .. } => Some(SoundCue::ClusterBurst),
        GameplayEventKind::FireSpread { .. } => Some(SoundCue::FireCrackle),
        GameplayEventKind::PoiOpened { poi, .. } => Some(match poi {
            PoiKind::SupplyCache | PoiKind::RivalClaim => SoundCue::CrateOpened,
            PoiKind::DerelictRig | PoiKind::Vault | PoiKind::Formation => SoundCue::SalvageOpened,
        }),
        GameplayEventKind::PoiDiscovered { .. } => Some(SoundCue::PoiDiscovered),
        GameplayEventKind::VaultOpened { .. } => Some(SoundCue::VaultOpened),
        GameplayEventKind::VaultLockReset { .. } => Some(SoundCue::VaultReset),
        // Quests: a tick per finished stage, the error buzz when a twist fails a job, and the
        // upgrade chime for a completed blueprint.
        // (The collect-blocks toggle and a changed enemy setting tick like a menu confirm.)
        GameplayEventKind::QuestStageDone { .. }
        | GameplayEventKind::CollectBlocksToggled { .. }
        | GameplayEventKind::EnemyDifficultyChanged { .. } => Some(SoundCue::Ui),
        GameplayEventKind::QuestFailed { .. } => Some(SoundCue::UiError),
        GameplayEventKind::BlueprintCompleted { .. } => Some(SoundCue::Upgrade),
        // A garage paint job: the spray can, heard from the rig (other players too).
        GameplayEventKind::RigCustomized { .. } => Some(SoundCue::SprayPaint),
        // Drops (WP-DROPS): the pickup sound by item, the closed-hatch clunk when the bay or
        // hopper is full and the rattle of dumped blocks.
        GameplayEventKind::ItemsCollected { item, .. } => Some(pickup_cue(*item)),
        GameplayEventKind::PickupBlocked { .. } => Some(SoundCue::StowageFull),
        GameplayEventKind::BlocksDumped { .. } => Some(SoundCue::BlocksDumped),
        // Caves: the echoing discovery chime, the cache's lid and coins, the stalactite's crack
        // (placed in the world by the effects layer).
        GameplayEventKind::CaveDiscovered { .. } => Some(SoundCue::CaveDiscovered),
        GameplayEventKind::CaveCacheOpened { .. } => Some(SoundCue::CacheOpened),
        GameplayEventKind::StalactiteFell { .. } => Some(SoundCue::StalactiteCrack),
        // Villages: a welcome on the first visit; at a counter, the shrine's bells and the
        // cartographer's chart pulse (other deals get the shell's coins / purchase chime).
        GameplayEventKind::VillageDiscovered { .. } => Some(SoundCue::VillageArrival),
        GameplayEventKind::VillageServiceUsed { service, .. } => match service {
            VillageService::Shrine => Some(SoundCue::ShrineBlessing),
            VillageService::Cartographer => Some(SoundCue::Scanner),
            _ => None,
        },
        // Enemies: every tell, wind-up, shot, hit and death is heard from where it happens.
        GameplayEventKind::EnemyAlerted { kind, .. } => Some(enemy_cue(
            *kind,
            SoundCue::CreatureScreech,
            SoundCue::MachineAlert,
        )),
        GameplayEventKind::EnemyWindup { kind, .. } => Some(enemy_cue(
            *kind,
            SoundCue::CreatureHiss,
            SoundCue::MachineCharge,
        )),
        GameplayEventKind::EnemyFired { shot, .. } => Some(shot_cue(*shot)),
        GameplayEventKind::EnemyHurt { kind, .. } => Some(enemy_cue(
            *kind,
            SoundCue::CreatureHurt,
            SoundCue::MachineHurt,
        )),
        GameplayEventKind::EnemyKilled { kind, .. } => Some(enemy_cue(
            *kind,
            SoundCue::CreatureDeath,
            SoundCue::MachineDeath,
        )),
        GameplayEventKind::EnemyStunned { .. } => Some(SoundCue::MachineHurt),
        GameplayEventKind::RigStruck { .. } => Some(SoundCue::EnemyBite),
        // Weapons: each gun's shot, the empty click, the hardpoint clack, impacts and a
        // target breaking apart. Damage numbers are silent: the impact or blast is the sound.
        GameplayEventKind::WeaponFired { weapon, .. } => Some(weapon_cue(*weapon)),
        GameplayEventKind::WeaponDry { .. } => Some(SoundCue::WeaponDry),
        GameplayEventKind::WeaponMounted { .. } => Some(SoundCue::WeaponMount),
        GameplayEventKind::ProjectileImpact { surface, .. } => Some(impact_cue(*surface)),
        GameplayEventKind::EntityDestroyed { .. } => Some(SoundCue::TargetDestroyed),
        // The shell plays coins/purchase when the service it requested resolves. Hazards are
        // continuous: the director's gas-hiss and boulder-rumble loops follow them directly.
        // Tunnel edits are terrain changes with no sound of their own. Accepting a quest and
        // test-mode toggles/warps are answered by the counter or menu that requested them.
        GameplayEventKind::PurchaseCompleted { .. }
        | GameplayEventKind::EntityDamaged { .. }
        | GameplayEventKind::QuestAccepted { .. }
        | GameplayEventKind::TestModeChanged { .. }
        | GameplayEventKind::HazardChanged
        | GameplayEventKind::BouldersChanged
        | GameplayEventKind::EscapeTunnelChanged => None,
    }
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::{
        BlueprintId, PoiReward, QuestFailure, QuestId, worldgen::FeatureId,
    };

    use super::*;

    /// One of every event kind (the match below must stay exhaustive, so adding a kind breaks
    /// this test until it is listed).
    #[allow(clippy::too_many_lines, reason = "one sample of every event kind")]
    fn every_event() -> Vec<GameplayEventKind> {
        let player_id = PlayerId::new(1);
        let feature = FeatureId(0);
        let quest = QuestId(101);
        let sample = GameplayEventKind::HazardChanged;
        match sample {
            GameplayEventKind::DrillStarted { .. }
            | GameplayEventKind::PurchaseCompleted { .. }
            | GameplayEventKind::CargoChanged { .. }
            | GameplayEventKind::BombPlaced { .. }
            | GameplayEventKind::InfrastructurePlaced { .. }
            | GameplayEventKind::ScannerUsed { .. }
            | GameplayEventKind::EscapeStarted { .. }
            | GameplayEventKind::EscapeTunnelChanged
            | GameplayEventKind::DeepRewardGranted { .. }
            | GameplayEventKind::RescueTriggered { .. }
            | GameplayEventKind::PlayerDamaged { .. }
            | GameplayEventKind::HazardChanged
            | GameplayEventKind::BouldersChanged
            | GameplayEventKind::BombDetonated { .. }
            | GameplayEventKind::BombBounced { .. }
            | GameplayEventKind::ExplosiveDeployed { .. }
            | GameplayEventKind::RemoteDetonation { .. }
            | GameplayEventKind::ChargeStuck { .. }
            | GameplayEventKind::ExplosiveDetonated { .. }
            | GameplayEventKind::ClusterSplit { .. }
            | GameplayEventKind::FireSpread { .. }
            | GameplayEventKind::PoiOpened { .. }
            | GameplayEventKind::PoiDiscovered { .. }
            | GameplayEventKind::VaultOpened { .. }
            | GameplayEventKind::VaultLockReset { .. }
            | GameplayEventKind::TestModeChanged { .. }
            | GameplayEventKind::QuestAccepted { .. }
            | GameplayEventKind::QuestStageDone { .. }
            | GameplayEventKind::QuestReady { .. }
            | GameplayEventKind::QuestCompleted { .. }
            | GameplayEventKind::QuestFailed { .. }
            | GameplayEventKind::BlueprintCompleted { .. }
            | GameplayEventKind::RigCustomized { .. }
            | GameplayEventKind::ItemsCollected { .. }
            | GameplayEventKind::PickupBlocked { .. }
            | GameplayEventKind::BlocksDumped { .. }
            | GameplayEventKind::CollectBlocksToggled { .. }
            | GameplayEventKind::CaveDiscovered { .. }
            | GameplayEventKind::CaveCacheOpened { .. }
            | GameplayEventKind::StalactiteFell { .. }
            | GameplayEventKind::VillageDiscovered { .. }
            | GameplayEventKind::VillageServiceUsed { .. }
            | GameplayEventKind::EnemyAlerted { .. }
            | GameplayEventKind::EnemyWindup { .. }
            | GameplayEventKind::EnemyFired { .. }
            | GameplayEventKind::EnemyHurt { .. }
            | GameplayEventKind::EnemyKilled { .. }
            | GameplayEventKind::EnemyStunned { .. }
            | GameplayEventKind::RigStruck { .. }
            | GameplayEventKind::EnemyDifficultyChanged { .. }
            | GameplayEventKind::WeaponFired { .. }
            | GameplayEventKind::WeaponDry { .. }
            | GameplayEventKind::WeaponMounted { .. }
            | GameplayEventKind::ProjectileImpact { .. }
            | GameplayEventKind::EntityDamaged { .. }
            | GameplayEventKind::EntityDestroyed { .. } => {}
        }
        let mut events = vec![
            GameplayEventKind::DrillStarted { player_id },
            GameplayEventKind::PurchaseCompleted { player_id },
            GameplayEventKind::CargoChanged { player_id },
            GameplayEventKind::BombPlaced { player_id },
            GameplayEventKind::InfrastructurePlaced { player_id },
            GameplayEventKind::ScannerUsed { player_id },
            GameplayEventKind::EscapeStarted { player_id },
            GameplayEventKind::EscapeTunnelChanged,
            GameplayEventKind::DeepRewardGranted { player_id },
            GameplayEventKind::RescueTriggered { player_id },
            GameplayEventKind::PlayerDamaged { player_id },
            GameplayEventKind::HazardChanged,
            GameplayEventKind::BouldersChanged,
            GameplayEventKind::BombDetonated {
                bomb_id: 1,
                x: 0,
                y: 0,
            },
            GameplayEventKind::BombBounced {
                bomb_id: 1,
                x: 0,
                y: 0,
            },
            GameplayEventKind::PoiDiscovered {
                player_id,
                feature,
                poi: PoiKind::Vault,
            },
            GameplayEventKind::VaultOpened { feature },
            GameplayEventKind::VaultLockReset { feature },
            GameplayEventKind::TestModeChanged { player_id },
            GameplayEventKind::QuestAccepted { player_id, quest },
            GameplayEventKind::QuestStageDone { player_id, quest },
            GameplayEventKind::QuestReady { player_id, quest },
            GameplayEventKind::QuestCompleted { player_id, quest },
            GameplayEventKind::QuestFailed {
                player_id,
                quest,
                reason: QuestFailure::OutOfTime,
            },
            GameplayEventKind::BlueprintCompleted {
                player_id,
                blueprint: BlueprintId::HaulerFrame,
            },
            GameplayEventKind::RigCustomized { player_id },
            GameplayEventKind::CaveDiscovered {
                player_id,
                feature,
                cave: fatherload_simulation::cave::CaveKind::CrystalGeode,
            },
            GameplayEventKind::CaveCacheOpened {
                player_id,
                tile: fatherload_simulation::cave::CaveTile::TreasureCache,
                credits: 100,
            },
            GameplayEventKind::StalactiteFell { x: 0, y: 0 },
            GameplayEventKind::VillageDiscovered {
                player_id,
                village: fatherload_simulation::village::VillageId(2),
            },
            GameplayEventKind::VillageServiceUsed {
                player_id,
                village: fatherload_simulation::village::VillageId(2),
                service: VillageService::Shrine,
            },
            GameplayEventKind::VillageServiceUsed {
                player_id,
                village: fatherload_simulation::village::VillageId(2),
                service: VillageService::TradingPost,
            },
        ];
        events.push(GameplayEventKind::RemoteDetonation { player_id });
        for item in [
            ItemKind::Mineral(fatherload_simulation::terrain::MineralKind::Gold),
            ItemKind::Artifact(fatherload_simulation::terrain::ArtifactKind::Fossil),
            ItemKind::Block(fatherload_simulation::item::BlockKind::Stone),
        ] {
            events.push(GameplayEventKind::ItemsCollected {
                player_id,
                item,
                count: 1,
                x: 0,
                y: 0,
            });
        }
        for stowage in [
            fatherload_simulation::item::Stowage::CargoBay,
            fatherload_simulation::item::Stowage::BlockHopper,
        ] {
            events.push(GameplayEventKind::PickupBlocked { player_id, stowage });
        }
        events.push(GameplayEventKind::BlocksDumped { player_id });
        events.push(GameplayEventKind::CollectBlocksToggled {
            player_id,
            enabled: false,
        });
        events.push(GameplayEventKind::ClusterSplit {
            bomb_id: 1,
            x: 0,
            y: 0,
        });
        events.push(GameplayEventKind::FireSpread {
            bomb_id: 1,
            x: 0,
            y: 0,
        });
        for kind in ExplosiveKind::ALL {
            events.push(GameplayEventKind::ExplosiveDeployed { player_id, kind });
            events.push(GameplayEventKind::ChargeStuck {
                bomb_id: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::ExplosiveDetonated {
                bomb_id: 1,
                kind,
                x: 0,
                y: 0,
                direction: fatherload_simulation::BlastDirection::Down,
            });
        }
        for kind in EnemyKind::ALL {
            events.push(GameplayEventKind::EnemyAlerted {
                entity: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::EnemyWindup {
                entity: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::EnemyHurt {
                entity: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::EnemyKilled {
                entity: 1,
                kind,
                x: 0,
                y: 0,
                by: Some(player_id),
            });
            events.push(GameplayEventKind::EnemyStunned {
                entity: 1,
                kind,
                x: 0,
                y: 0,
            });
            events.push(GameplayEventKind::RigStruck { player_id, kind });
        }
        for shot in [ShotKind::Acid, ShotKind::Fireball, ShotKind::Bolt] {
            events.push(GameplayEventKind::EnemyFired {
                entity: 1,
                shot,
                x: 0,
                y: 0,
            });
        }
        events.push(GameplayEventKind::EnemyDifficultyChanged {
            player_id,
            difficulty: fatherload_simulation::EnemyDifficulty::Hard,
        });
        for weapon in WeaponKind::ALL {
            events.push(GameplayEventKind::WeaponFired {
                player_id,
                weapon,
                x: 0,
                y: 0,
                end_x: 1,
                end_y: 0,
            });
            events.push(GameplayEventKind::WeaponDry { player_id, weapon });
        }
        events.push(GameplayEventKind::WeaponMounted {
            player_id,
            weapon: Some(WeaponKind::RivetGun),
        });
        for surface in [
            ImpactSurface::Terrain,
            ImpactSurface::Target,
            ImpactSurface::Charge,
            ImpactSurface::Loot,
        ] {
            events.push(GameplayEventKind::ProjectileImpact {
                weapon: WeaponKind::RivetGun,
                x: 0,
                y: 0,
                surface,
            });
        }
        events.push(GameplayEventKind::EntityDestroyed {
            entity: 1,
            x: 0,
            y: 0,
            by: Some(player_id),
        });
        events.extend(PoiKind::ALL.map(|poi| GameplayEventKind::PoiOpened {
            player_id,
            poi,
            reward: PoiReward::default(),
        }));
        events
    }

    #[test]
    fn every_event_is_either_heard_or_deliberately_silent() {
        let silent: Vec<GameplayEventKind> = every_event()
            .into_iter()
            .filter(|event| event_cue(event).is_none())
            .collect();
        // These are covered by the shell (purchases) or the director's continuous loops.
        assert_eq!(
            silent,
            vec![
                GameplayEventKind::PurchaseCompleted {
                    player_id: PlayerId::new(1)
                },
                GameplayEventKind::EscapeTunnelChanged,
                GameplayEventKind::HazardChanged,
                GameplayEventKind::BouldersChanged,
                GameplayEventKind::TestModeChanged {
                    player_id: PlayerId::new(1)
                },
                GameplayEventKind::QuestAccepted {
                    player_id: PlayerId::new(1),
                    quest: QuestId(101)
                },
                GameplayEventKind::VillageServiceUsed {
                    player_id: PlayerId::new(1),
                    village: fatherload_simulation::village::VillageId(2),
                    service: VillageService::TradingPost,
                },
            ]
        );
        assert!(
            event_cue(&GameplayEventKind::EntityDamaged {
                entity: 1,
                x: 0,
                y: 0,
                amount: 3
            })
            .is_none(),
            "damage numbers are silent: the impact plays"
        );
    }

    #[test]
    fn every_explosive_kind_has_its_own_blast_and_throws_whoosh() {
        for kind in ExplosiveKind::ALL {
            let cue = detonation_cue(kind);
            assert!(!cue_patch(cue).looped(), "{kind:?}");
        }
        assert_eq!(detonation_cue(ExplosiveKind::Shaped), SoundCue::ShapedBlast);
        assert_eq!(deploy_cue(ExplosiveKind::Sticky), SoundCue::ExplosiveThrow);
        assert_eq!(deploy_cue(ExplosiveKind::C4), SoundCue::BombPlaced);
    }

    #[test]
    fn every_cue_has_a_distinct_rendered_patch() {
        let cues = [
            SoundCue::Drill,
            SoundCue::Sell,
            SoundCue::Upgrade,
            SoundCue::Damage,
            SoundCue::Milestone,
            SoundCue::Rescue,
            SoundCue::Explosion,
            SoundCue::Ui,
            SoundCue::UiMove,
            SoundCue::UiBack,
            SoundCue::UiError,
            SoundCue::Pickup,
            SoundCue::BlockPickup,
            SoundCue::ArtifactPickup,
            SoundCue::StowageFull,
            SoundCue::BlocksDumped,
            SoundCue::BombPlaced,
            SoundCue::BombBounce,
            SoundCue::InfrastructurePlaced,
            SoundCue::Scanner,
            SoundCue::PoiDiscovered,
            SoundCue::CrateOpened,
            SoundCue::SalvageOpened,
            SoundCue::VaultOpened,
            SoundCue::VaultReset,
            SoundCue::ExplosiveThrow,
            SoundCue::ChargeStuck,
            SoundCue::Detonator,
            SoundCue::SmallBlast,
            SoundCue::ClusterBurst,
            SoundCue::ShapedBlast,
            SoundCue::DeepBlast,
            SoundCue::FireWhoosh,
            SoundCue::FireCrackle,
            SoundCue::EmpPulse,
            SoundCue::FlareIgnite,
            SoundCue::CaveDiscovered,
            SoundCue::StalactiteCrack,
            SoundCue::CacheOpened,
            SoundCue::VillageArrival,
            SoundCue::ShrineBlessing,
            SoundCue::CreatureScreech,
            SoundCue::CreatureHiss,
            SoundCue::AcidSpit,
            SoundCue::CreatureHurt,
            SoundCue::CreatureDeath,
            SoundCue::MachineAlert,
            SoundCue::MachineCharge,
            SoundCue::MachineZap,
            SoundCue::MachineHurt,
            SoundCue::MachineDeath,
            SoundCue::EnemyBite,
            SoundCue::SmokeBurst,
            SoundCue::ScareBang,
            SoundCue::RivetShot,
            SoundCue::ScatterShot,
            SoundCue::CutterZap,
            SoundCue::RocketLaunch,
            SoundCue::HarpoonShot,
            SoundCue::RailgunShot,
            SoundCue::WeaponDry,
            SoundCue::WeaponMount,
            SoundCue::ImpactRock,
            SoundCue::ImpactHit,
            SoundCue::TargetDestroyed,
        ];
        let mut patches: Vec<Patch> = cues.iter().map(|cue| cue_patch(*cue)).collect();
        assert!(patches.iter().all(|patch| !patch.looped()));
        patches.sort();
        patches.dedup();
        assert_eq!(patches.len(), cues.len());
    }
}
