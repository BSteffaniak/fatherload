//! What the game asks to hear: [`SoundCue`]s placed at a source, the cue -> patch table, and the
//! gameplay event -> cue table. Pure data, no device access.

use fatherload_simulation::{
    EnemyKind, ExplosiveKind, GameplayEventKind, PlayerId, PoiKind, ShotKind, WeaponKind,
    base::{Buildable, ModuleKind, ModuleService},
    event::ImpactSurface,
    explosive::Deployment,
    fluid::FluidReaction,
    item::{ItemCategory, ItemKind},
    tomb::crypt::{CryptAttack, CryptTrapKind},
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
    /// The store a world sound happened in (Wave 13 QA): only seats there hear it.
    pub store: fatherload_simulation::place::StoreKey,
}

impl PlacedCue {
    /// An interface (centred) cue.
    #[must_use]
    pub const fn interface(cue: SoundCue) -> Self {
        Self {
            cue,
            source: SoundSource::Interface,
            store: fatherload_simulation::place::StoreKey::Home,
        }
    }

    /// A cue from `player`'s rig.
    #[must_use]
    pub const fn player(cue: SoundCue, player: PlayerId) -> Self {
        Self {
            cue,
            source: SoundSource::Player(player),
            store: fatherload_simulation::place::StoreKey::Home,
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
            store: fatherload_simulation::place::StoreKey::Home,
        }
    }
}

/// The designed patch each cue plays.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per cue")]
pub const fn cue_patch(cue: SoundCue) -> Patch {
    match cue {
        SoundCue::Drill => Patch::DrillBite,
        SoundCue::SkyBand => Patch::SkyBand,
        SoundCue::Updraft => Patch::UpdraftWhoosh,
        SoundCue::Icing => Patch::IceCrackle,
        SoundCue::SkyCache => Patch::SkyCacheOpen,
        SoundCue::BuoyRefuel => Patch::BuoyRefuel,
        SoundCue::IsleCollapse => Patch::IsleCollapse,
        SoundCue::JetStream => Patch::JetStreamRush,
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
        SoundCue::BuildPlace => Patch::BuildPlace,
        SoundCue::ModuleBuilt => Patch::ModuleBuilt,
        SoundCue::BuildRemove => Patch::BuildRemove,
        SoundCue::ClaimStaked => Patch::ClaimStake,
        SoundCue::FuelPump => Patch::FuelPump,
        SoundCue::SupplyDrop => Patch::SupplyDrop,
        SoundCue::RepairWeld => Patch::RepairWeld,
        SoundCue::StorageStow => Patch::StorageStow,
        SoundCue::UplinkBeam => Patch::UplinkBeam,
        SoundCue::HomeSet => Patch::HomeSet,
        SoundCue::RefineryLoad => Patch::RefineryLoad,
        SoundCue::WorkshopCraft => Patch::WorkshopCraft,
        SoundCue::CrewReturn => Patch::CrewReturn,
        SoundCue::Dawn => Patch::DawnChorus,
        SoundCue::Dusk => Patch::DuskCrickets,
        SoundCue::Midnight => Patch::MidnightBell,
        SoundCue::BedDown => Patch::BedDown,
        SoundCue::NightSkipped => Patch::NightSkip,
        SoundCue::HatchOpen => Patch::HatchOpen,
        SoundCue::HatchClose => Patch::HatchClose,
        SoundCue::PilotJump => Patch::PilotJump,
        SoundCue::PilotLand => Patch::PilotLand,
        SoundCue::PilotLandHard => Patch::PilotLandHard,
        SoundCue::PilotHurt => Patch::PilotHurt,
        SoundCue::PilotDown => Patch::PilotDown,
        SoundCue::OxygenAlarm => Patch::OxygenAlarm,
        SoundCue::JetpackBurst => Patch::JetpackBurst,
        SoundCue::JetpackSputter => Patch::JetpackSputter,
        SoundCue::PickChip => Patch::PickChip,
        SoundCue::PickBreak => Patch::BreakSoft,
        SoundCue::PickClink => Patch::PickClink,
        SoundCue::PistolPop => Patch::PistolPop,
        SoundCue::RifleCrack => Patch::RifleCrack,
        SoundCue::ShotgunBoom => Patch::ShotgunBoom,
        SoundCue::WrenchSwing => Patch::WrenchSwing,
        SoundCue::GrappleFire => Patch::GrappleFire,
        SoundCue::GliderOpen => Patch::GliderOpen,
        SoundCue::GateGrind => Patch::GateGrind,
        SoundCue::KeycardBeep => Patch::KeycardBeep,
        SoundCue::Splash => Patch::Splash,
        SoundCue::LedgeGrab => Patch::LedgeGrab,
        SoundCue::PersonGreet => Patch::PersonGreet,
        SoundCue::GiftGiven => Patch::GiftGiven,
        SoundCue::AffinityUp => Patch::AffinityUp,
        SoundCue::RumourHeard => Patch::RumourHeard,
        SoundCue::UpgradeFanfare => Patch::UpgradeFanfare,
        SoundCue::MilestoneFanfare => Patch::MilestoneFanfare,
        SoundCue::ActTwoFanfare => Patch::ActTwoFanfare,
        SoundCue::ArcZap => Patch::ArcCrackle,
        SoundCue::GravityHum => Patch::GravityHum,
        SoundCue::GravityThrow => Patch::GravityThrow,
        SoundCue::TetherShot => Patch::TetherShot,
        SoundCue::TetherHook => Patch::TetherHook,
        SoundCue::FlakShot => Patch::FlakThump,
        SoundCue::FlakBurst => Patch::FlakBurst,
        SoundCue::Thunderclap => Patch::Thunderclap,
        SoundCue::LeverClunk => Patch::LeverClunk,
        SoundCue::GateRumble => Patch::GateRumble,
        SoundCue::SwoopScreech => Patch::SwoopScreech,
        SoundCue::PirateHorn => Patch::PirateHorn,
        SoundCue::HarpyScreech => Patch::HarpyScreech,
        SoundCue::CannonBoom => Patch::CannonBoom,
        SoundCue::MoonLanding => Patch::MoonSting,
        SoundCue::VacuumAlarm => Patch::VacuumAlarm,
        SoundCue::PylonChime => Patch::PylonChime,
        SoundCue::GateOpen => Patch::FacilityGate,
        SoundCue::WardenWake => Patch::WardenWake,
        SoundCue::CoreReached => Patch::CoreHum,
        SoundCue::UplinkWarp => Patch::UplinkWarp,
        SoundCue::DeedSigned => Patch::DeedStamp,
        SoundCue::BuildersHammer => Patch::BuildersHammer,
        SoundCue::HomeFanfare => Patch::HomeFanfare,
        SoundCue::SiteKnock => Patch::SiteKnock,
        SoundCue::FurnitureThunk => Patch::FurnitureThunk,
        SoundCue::HoundHowl => Patch::HoundHowl,
        SoundCue::AlphaHowl => Patch::AlphaHowl,
        SoundCue::StalkerBurst => Patch::StalkerBurst,
        SoundCue::WispChime => Patch::WispChime,
        SoundCue::MothFlutter => Patch::MothFlutter,
        SoundCue::NightFalls => Patch::NightFallSting,
        SoundCue::DawnRelief => Patch::DawnRelief,
        SoundCue::Burrow => Patch::BurrowPuff,
        SoundCue::StoryHook => Patch::StoryHook,
        SoundCue::StoryQuill => Patch::StoryQuill,
        SoundCue::StoryClimax => Patch::StoryClimax,
        SoundCue::StoryResolved => Patch::StoryResolved,
        SoundCue::BladeSwish => Patch::BladeSwish,
        SoundCue::CrusherSlam => Patch::CrusherSlam,
        SoundCue::DartFire => Patch::DartFire,
        SoundCue::FloorCrumble => Patch::FloorCrumble,
        SoundCue::SandPour => Patch::SandPour,
        SoundCue::FogBurst => Patch::FogBurst,
        SoundCue::MummyGroan => Patch::MummyGroan,
        SoundCue::BoneRattle => Patch::BoneRattle,
        SoundCue::StatueGrind => Patch::StatueGrind,
        SoundCue::ColossusSlam => Patch::ColossusSlam,
        SoundCue::MatriarchWail => Patch::MatriarchWail,
        SoundCue::DodgeRoll => Patch::DodgeRoll,
        SoundCue::ArrowTwang => Patch::ArrowTwang,
        SoundCue::SpikeShing => Patch::SpikeShing,
        SoundCue::PitDrop => Patch::PitDrop,
        SoundCue::LordRoar => Patch::LordRoar,
        SoundCue::StoneSlam => Patch::StoneSlam,
        SoundCue::HexBolt => Patch::HexBolt,
        SoundCue::FluidRush => Patch::FluidRush,
        SoundCue::SporePuff => Patch::SporePuff,
        SoundCue::RootCreak => Patch::RootCreak,
        SoundCue::FleshSquelch => Patch::FleshSquelch,
        SoundCue::HiveChime => Patch::HiveChime,
        SoundCue::MindRoar => Patch::MindRoar,
        SoundCue::SteamHiss => Patch::SteamHiss,
        SoundCue::AcidFizz => Patch::AcidFizz,
        SoundCue::RigSplash => Patch::RigSplash,
        SoundCue::CartClunk => Patch::CartClunk,
        SoundCue::CartSwitch => Patch::CartSwitch,
        SoundCue::TimberCreak => Patch::TimberCreak,
        SoundCue::RoofCollapse => Patch::RoofCollapse,
        SoundCue::SinkholeRumble => Patch::SinkholeRumble,
        SoundCue::DeepClaimFanfare => Patch::DeepClaimFanfare,
        SoundCue::TubeBreath => Patch::TubeBreath,
        SoundCue::ResonanceChime => Patch::ResonanceChime,
        SoundCue::ValveTurn => Patch::ValveTurn,
        SoundCue::PistonSlam => Patch::PistonSlam,
        SoundCue::GeigerAlert => Patch::GeigerAlert,
        SoundCue::StormSiren => Patch::StormSiren,
        SoundCue::CoreStrike => Patch::CoreStrike,
        SoundCue::CoreWake => Patch::CoreWake,
        SoundCue::DeepLog => Patch::DeepLog,
        SoundCue::BoneCrumble => Patch::BoneCrumble,
        SoundCue::RadioEcho => Patch::RadioEcho,
        SoundCue::GhostVoice => Patch::GhostVoice,
        SoundCue::LogCanister => Patch::LogCanister,
        SoundCue::AnomalyHum => Patch::AnomalyHum,
        SoundCue::SonarPing => Patch::SonarPing,
        SoundCue::GeyserBlast => Patch::GeyserBlast,
        SoundCue::IceCrack => Patch::IceCrack,
        SoundCue::HullGroan => Patch::HullGroan,
        SoundCue::ClamSnap => Patch::ClamSnap,
        SoundCue::LeviathanRoar => Patch::LeviathanRoar,
        SoundCue::MerrowChime => Patch::MerrowChime,
        SoundCue::SunSting => Patch::SunSting,
        SoundCue::WindGust => Patch::WindGust,
        SoundCue::BeastBellow => Patch::BeastBellow,
        SoundCue::RocCry => Patch::RocCry,
        SoundCue::SerpentRoar => Patch::SerpentRoar,
        SoundCue::FlareSwell => Patch::FlareSwell,
        SoundCue::AnchorThunk => Patch::AnchorThunk,
        SoundCue::GlassChime => Patch::GlassChime,
        SoundCue::MirrorShatter => Patch::MirrorShatter,
        SoundCue::GravityGroan => Patch::GravityGroan,
        SoundCue::GlitchBlip => Patch::GlitchBlip,
        SoundCue::DataChirp => Patch::DataChirp,
        SoundCue::EchoPing => Patch::EchoPing,
        SoundCue::TimeBubbleBurst => Patch::TimeBubbleBurst,
        SoundCue::RewindWarp => Patch::RewindWarp,
        SoundCue::Shriek => Patch::Shriek,
        SoundCue::QuartzRing => Patch::QuartzRing,
        SoundCue::KeyedGlass => Patch::KeyedGlass,
        SoundCue::GlitchRewrite => Patch::GlitchRewrite,
        SoundCue::GlitchBlink => Patch::GlitchBlink,
        SoundCue::CityTramBell => Patch::CityTramBell,
        SoundCue::CityLiftChime => Patch::CityLiftChime,
        SoundCue::CityPistonHiss => Patch::CityPistonHiss,
        SoundCue::CityPistonThump => Patch::CityPistonThump,
        SoundCue::CityCrusherSlam => Patch::CityCrusherSlam,
        SoundCue::CityArchivistChirp => Patch::CityArchivistChirp,
        SoundCue::CityArchivistScan => Patch::CityArchivistScan,
        SoundCue::CityChartChord => Patch::CityChartChord,
        SoundCue::CityVaultGive => Patch::CityVaultGive,
        SoundCue::CityAlarm => Patch::CityAlarm,
        SoundCue::CityCargoSnatch => Patch::CityCargoSnatch,
        SoundCue::CitySparkZap => Patch::CitySparkZap,
        SoundCue::GarrisonTurret => Patch::AbTurretShot,
        SoundCue::GarrisonTrap => Patch::AbTrapZap,
        SoundCue::GarrisonKlaxon => Patch::AbKlaxon,
        SoundCue::GarrisonBay => Patch::AbBayOpen,
        SoundCue::GarrisonBreak => Patch::AbDefenceBreak,
        SoundCue::ColossusFistSlam => Patch::AbFistSlam,
        SoundCue::ColossusCollapse => Patch::AbHallCollapse,
        SoundCue::SealSocket => Patch::AbSocketChime,
        SoundCue::SealDoorGrind => Patch::AbDoorGrind,
        SoundCue::SealTremor => Patch::AbTremor,
        SoundCue::HeartCableLight => Patch::HeartCableLight,
        SoundCue::HeartInhale => Patch::HeartInhale,
        SoundCue::HeartPull => Patch::HeartPull,
        SoundCue::HeartIgnite => Patch::HeartIgnite,
        SoundCue::HeartExhale => Patch::HeartExhale,
        SoundCue::HeartCoilBreak => Patch::HeartCoilBreak,
        SoundCue::HeartDarkMatter => Patch::HeartDarkMatter,
        SoundCue::HeartRailZap => Patch::HeartRailZap,
        SoundCue::HeartWellGroan => Patch::HeartWellGroan,
        SoundCue::HeartMetalSplash => Patch::HeartMetalSplash,
        SoundCue::StoryDescentCall => Patch::StoryDescentCall,
        SoundCue::StoryCableLit => Patch::StoryCableLit,
        SoundCue::SystemsFlareWarning => Patch::SystemsFlareWarning,
        SoundCue::SystemsFlareBurst => Patch::SystemsFlareBurst,
        SoundCue::SystemsHoleWarning => Patch::SystemsHoleWarning,
        SoundCue::SystemsTransit => Patch::SystemsTransit,
        SoundCue::SystemsOreCrunch => Patch::SystemsOreCrunch,
        // The frontier worlds (Wave 12 planets-b).
        SoundCue::FrontierSandSlide => Patch::PbSandSlide,
        SoundCue::FrontierFloorCrumble => Patch::PbFloorCrumble,
        SoundCue::FrontierGlassShatter => Patch::PbGlassShatter,
        SoundCue::FrontierCrystalRing => Patch::PbCrystalRing,
        SoundCue::FrontierSplinter => Patch::PbSplinter,
        SoundCue::FrontierBeamLit => Patch::PbBeamLit,
        SoundCue::FrontierVaultOpen => Patch::PbVaultOpen,
        SoundCue::FrontierRumble => Patch::PbRumble,
        SoundCue::FrontierEruption => Patch::PbEruption,
        SoundCue::FrontierLavaBomb => Patch::PbLavaBomb,
        SoundCue::FrontierStormRising => Patch::PbStormRising,
        SoundCue::FrontierStormBreak => Patch::PbStormBreak,
        SoundCue::FrontierCrystalGrow => Patch::PbCrystalGrow,
        SoundCue::FrontierChime => Patch::PbChime,
        SoundCue::FrontierWyrmRipple => Patch::PbWyrmRipple,
        SoundCue::AlienVessiVoice => Patch::AlienVessiVoice,
        SoundCue::AlienTokkariVoice => Patch::AlienTokkariVoice,
        SoundCue::AlienSiphraVoice => Patch::AlienSiphraVoice,
        SoundCue::AlienFirstContact => Patch::AlienFirstContact,
        SoundCue::AlienWordLearned => Patch::AlienWordLearned,
        SoundCue::SpaceCannon => Patch::HxCannon,
        SoundCue::SpaceMissile => Patch::HxMissile,
        SoundCue::SpaceShipHit => Patch::HxShipHit,
        SoundCue::SpaceShipBoom => Patch::HxShipBoom,
        SoundCue::SpaceShieldDown => Patch::HxShieldDown,
        SoundCue::SpaceHail => Patch::HxHail,
        SoundCue::SpaceBossWarn => Patch::HxBossWarn,
        SoundCue::SpaceBossAppear => Patch::HxBossAppear,
        SoundCue::SpaceEnemyShot => Patch::HxEnemyShot,
        // The outland worlds (Wave 13 planets-c).
        SoundCue::OutlandCapBounce => Patch::PcCapBounce,
        SoundCue::OutlandMyceliumCreep => Patch::PcMyceliumCreep,
        SoundCue::OutlandGlowflash => Patch::PcGlowflash,
        SoundCue::OutlandVentSwell => Patch::PcVentSwell,
        SoundCue::OutlandVentBlast => Patch::PcVentBlast,
        SoundCue::OutlandGearClank => Patch::PcGearClank,
        SoundCue::OutlandRelayFlip => Patch::PcRelayFlip,
        SoundCue::OutlandTombOpen => Patch::PcTombOpen,
        SoundCue::OutlandWispChill => Patch::PcWispChill,
        SoundCue::OutlandBossWake => Patch::PcBossWake,
        SoundCue::OutlandBroodCall => Patch::PcBroodCall,
        SoundCue::OutlandBossFall => Patch::PcBossFall,
        SoundCue::ExoticCrackle => Patch::PdCrackle,
        SoundCue::ExoticBolt => Patch::PdBolt,
        SoundCue::ExoticPressure => Patch::PdPressure,
        SoundCue::ExoticImpact => Patch::PdImpact,
        SoundCue::ExoticHiss => Patch::PdHiss,
        SoundCue::ExoticGeyser => Patch::PdGeyser,
        SoundCue::ExoticQuake => Patch::PdQuake,
        SoundCue::ExoticWhistle => Patch::PdWhistle,
        SoundCue::ExoticVent => Patch::PdVent,
        SoundCue::ExoticZap => Patch::PdZap,
        SoundCue::ExoticPowerDown => Patch::PdPowerDown,
        SoundCue::ExoticStormRising => Patch::PdStormRising,
        SoundCue::ExoticBossAwake => Patch::PdBossAwake,
        SoundCue::ExoticBossTell => Patch::PdBossTell,
        SoundCue::ExoticBossDown => Patch::PdBossDown,
        SoundCue::ExoticDock => Patch::PdDock,
        SoundCue::SpaceWarpEngage => Patch::AaWarpEngage,
        SoundCue::SpaceWarpArrive => Patch::AaWarpArrive,
        SoundCue::SpaceCruiseEngage => Patch::AaCruiseEngage,
        SoundCue::SpaceDocked => Patch::AaDockChime,
        SoundCue::LairBossWake => Patch::F14LairWake,
        SoundCue::LairBossFall => Patch::F14LairFall,
    }
}

/// The voice of an alien species (Wave 12).
#[must_use]
pub const fn alien_voice(species: fatherload_simulation::alien::AlienSpecies) -> SoundCue {
    use fatherload_simulation::alien::AlienSpecies as S;
    match species {
        S::Vessi => SoundCue::AlienVessiVoice,
        S::Tokkari => SoundCue::AlienTokkariVoice,
        S::Siphra => SoundCue::AlienSiphraVoice,
    }
}

/// What each alien event sounds like (Wave 12; exhaustive: a new event needs a sound).
#[must_use]
pub const fn alien_cue(
    what: fatherload_simulation::alien::AlienEvent,
    species: fatherload_simulation::alien::AlienSpecies,
) -> SoundCue {
    use fatherload_simulation::alien::AlienEvent as E;
    match what {
        E::FirstContact => SoundCue::AlienFirstContact,
        // Each word they teach you, you hear them say.
        E::Learned | E::Lesson | E::ChapterOffered => alien_voice(species),
        E::FluencyRose | E::LexiconFound => SoundCue::AlienWordLearned,
        E::Sold | E::Bought => SoundCue::Sell,
        E::ChapterAccepted => SoundCue::StoryHook,
        E::ChapterReached => SoundCue::StoryQuill,
        E::ChapterCompleted => SoundCue::StoryResolved,
        E::PassengerAboard => SoundCue::HatchClose,
        E::PassengerDelivered => SoundCue::HatchOpen,
        E::MonumentRaised => SoundCue::MilestoneFanfare,
        E::HomeBought => SoundCue::DeedSigned,
        E::HomeBuilt => SoundCue::HomeFanfare,
        E::TrailLead => SoundCue::StoryClimax,
        E::Refused => SoundCue::UiError,
        E::Rested => SoundCue::BedDown,
    }
}

/// The sound of a galaxy story event (Wave 13 story): the story's own hook, quill, resolution
/// and climax, the archivist's chirp, the space boss's arrival, a fanfare when the Warden is
/// calm.
#[must_use]
pub const fn galaxy_cue(what: fatherload_simulation::galaxy_story::GalaxyEvent) -> SoundCue {
    use fatherload_simulation::galaxy_story::GalaxyEvent as E;
    match what {
        E::Begun | E::ChapterBegun | E::LeadFound => SoundCue::StoryHook,
        E::GoalDone | E::LogFound | E::LeadStep | E::FragmentFound => SoundCue::StoryQuill,
        E::ChapterDone | E::LeadDone => SoundCue::StoryResolved,
        E::RelicFound | E::Chosen => SoundCue::StoryClimax,
        E::HoldBegun => SoundCue::StormSiren,
        E::WardenAppeared => SoundCue::SpaceBossAppear,
        E::WardenCalmed => SoundCue::MilestoneFanfare,
        E::ArchivistRemark => SoundCue::CityArchivistChirp,
        E::CourseSet => SoundCue::UplinkWarp,
        E::Refused => SoundCue::UiError,
    }
}

/// The voice of a side-view tomb creature (WP-TOMBS T28) for its tell and wind-up: a mummy's
/// groan, an archer's rattling bones, a statue's grinding stone, the guardians' slam and wail.
/// `None` for every other kind.
#[must_use]
pub const fn tomb_voice(kind: EnemyKind) -> Option<SoundCue> {
    Some(match kind {
        EnemyKind::Mummy => SoundCue::MummyGroan,
        EnemyKind::SkeletalArcher => SoundCue::BoneRattle,
        EnemyKind::AnimatedStatue => SoundCue::StatueGrind,
        EnemyKind::SunColossus => SoundCue::ColossusSlam,
        EnemyKind::HollowMatriarch => SoundCue::MatriarchWail,
        _ => return None,
    })
}

/// The sound of a side-view trap going off (WP-TOMBS T28).
#[must_use]
pub const fn trap_cue(kind: fatherload_simulation::tomb::realize::TrapKind) -> Option<SoundCue> {
    use fatherload_simulation::tomb::realize::TrapKind;
    Some(match kind {
        TrapKind::Blade => SoundCue::BladeSwish,
        TrapKind::Crusher => SoundCue::CrusherSlam,
        TrapKind::DartWall => SoundCue::DartFire,
        TrapKind::Crumble => SoundCue::FloorCrumble,
        TrapKind::SandSluice => SoundCue::SandPour,
        TrapKind::FogVent => SoundCue::FogBurst,
        TrapKind::Spikes | TrapKind::FalseWall => return None,
    })
}

/// The voice of a night monster (WP-NIGHT) for its tells and wind-ups: a hound's howl, the
/// alpha's deeper one, sand bursting round a stalker, a wisp's glassy chime, a swarm's
/// flutter. `None` for the deep enemies (they use their family's sounds).
#[must_use]
pub const fn night_voice(kind: EnemyKind) -> Option<SoundCue> {
    Some(match kind {
        EnemyKind::ShadowHound => SoundCue::HoundHowl,
        EnemyKind::UmbralAlpha => SoundCue::AlphaHowl,
        EnemyKind::DuneStalker => SoundCue::StalkerBurst,
        EnemyKind::WillOWisp => SoundCue::WispChime,
        EnemyKind::MothSwarm => SoundCue::MothFlutter,
        _ => return None,
    })
}

/// The cue of an enemy event: creatures screech, hiss, squelch and wail; machines beep, whine,
/// clank and blow apart (exhaustive: a new kind picks its family's sounds).
#[must_use]
pub const fn enemy_cue(kind: EnemyKind, creature: SoundCue, machine: SoundCue) -> SoundCue {
    if kind.is_machine() { machine } else { creature }
}

/// The sound of an enemy projectile being fired (exhaustive: a new shot needs a sound).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "the Wave 10 shots keep rows of their own"
)]
pub const fn shot_cue(shot: ShotKind) -> SoundCue {
    match shot {
        ShotKind::Acid => SoundCue::AcidSpit,
        ShotKind::Fireball | ShotKind::Wispfire => SoundCue::FireWhoosh,
        ShotKind::Bolt | ShotKind::SentinelBolt => SoundCue::MachineZap,
        ShotKind::Cannonball => SoundCue::CannonBoom,
        ShotKind::Arrow | ShotKind::Thorn => SoundCue::DartFire,
        ShotKind::Rubble => SoundCue::FloorCrumble,
        ShotKind::Hex => SoundCue::FogBurst,
        ShotKind::CryptHex => SoundCue::HexBolt,
        ShotKind::CryptArrow => SoundCue::ArrowTwang,
        ShotKind::Boulder => SoundCue::GateRumble,
        ShotKind::Spore | ShotKind::MindSpore => SoundCue::SporePuff,
        ShotKind::PressureBubble => SoundCue::FluidRush,
        ShotKind::MirrorShard => SoundCue::GlassChime,
        ShotKind::RotPulse => SoundCue::GlitchBlip,
        ShotKind::PrismRed | ShotKind::PrismGreen | ShotKind::PrismBlue => SoundCue::KeyedGlass,
        ShotKind::GlitchBolt => SoundCue::GlitchBlink,
        ShotKind::ReadingBeam => SoundCue::CityArchivistScan,
        // Descent VII part B (Wave 10).
        ShotKind::SentryBolt | ShotKind::ColossusBolt => SoundCue::MachineZap,
        ShotKind::SiegeShell => SoundCue::CannonBoom,
        ShotKind::WardenOrb => SoundCue::SealSocket,
        ShotKind::FallingStone => SoundCue::FloorCrumble,
    }
}

/// The sound of a Descent VII part B event (Wave 10; exhaustive: a new event decides how it
/// sounds).
#[must_use]
pub const fn garrison_cue(what: fatherload_simulation::garrison::GarrisonEvent) -> SoundCue {
    use fatherload_simulation::garrison::GarrisonEvent as E;
    match what {
        E::FortStirred | E::WaveTelegraphed => SoundCue::GarrisonKlaxon,
        E::WaveWoke => SoundCue::GarrisonBay,
        E::WaveCleared => SoundCue::Upgrade,
        E::GarrisonSlept | E::GateRewards => SoundCue::MilestoneFanfare,
        E::StoodDown => SoundCue::UiBack,
        E::TurretFired => SoundCue::GarrisonTurret,
        E::TrapSprung => SoundCue::GarrisonTrap,
        E::DefenceDestroyed => SoundCue::GarrisonBreak,
        E::Tremor => SoundCue::SealTremor,
        E::SocketLit => SoundCue::SealSocket,
        E::SealOpening => SoundCue::SealDoorGrind,
        E::SealRefused => SoundCue::UiError,
        E::ColossusAwoke | E::HallCollapsing | E::HallCollapsed => SoundCue::ColossusCollapse,
        E::FistSlam => SoundCue::ColossusFistSlam,
        E::ColossusDefeated => SoundCue::CreatureDeath,
        E::LandmarkFound => SoundCue::AnomalyHum,
    }
}

/// The sound of a Drowned and Frozen event (Wave 9; exhaustive: a new event decides how it
/// sounds).
#[must_use]
pub const fn drowned_cue(what: fatherload_simulation::drowned::DrownedEvent) -> SoundCue {
    use fatherload_simulation::drowned::DrownedEvent as E;
    match what {
        E::SonarPing => SoundCue::SonarPing,
        E::FrostWarning | E::TunnelRefroze => SoundCue::IceCrack,
        E::GeyserErupted | E::GeyserScalded => SoundCue::GeyserBlast,
        E::GeyserLaunched => SoundCue::SteamHiss,
        E::ClamPearl => SoundCue::ArtifactPickup,
        E::ClamSnapped => SoundCue::ClamSnap,
        E::PressureWarning => SoundCue::HullGroan,
        E::InkCloud => SoundCue::FleshSquelch,
        E::EelDragonAwoke | E::LeviathanAwoke | E::LeviathanPhase | E::LeviathanCharge => {
            SoundCue::LeviathanRoar
        }
        E::EelDragonPulse => SoundCue::MachineZap,
        E::EelDragonDefeated => SoundCue::CreatureDeath,
        E::LeviathanDefeated | E::GateRewards => SoundCue::DeepClaimFanfare,
        E::LoreFound | E::CityReached => SoundCue::MerrowChime,
    }
}

/// The sound of a distortion strata event (Wave 9; exhaustive: a new event needs a sound).
#[must_use]
pub const fn distortion_cue(what: fatherload_simulation::distortion::DistortionEvent) -> SoundCue {
    use fatherload_simulation::distortion::DistortionEvent as E;
    match what {
        E::MirrorEntered | E::MirrorLeft | E::TwinAppeared | E::TwinFaded | E::Gazed => {
            SoundCue::GlassChime
        }
        E::TwinFired => SoundCue::DartFire,
        E::TwinBroken | E::GhostShattered | E::ShotReflected => SoundCue::MirrorShatter,
        E::MirrorOreMined => SoundCue::ResonanceChime,
        E::ShearEntered | E::LodestarPull | E::MantisDropped => SoundCue::GravityGroan,
        E::RotDeepened | E::GlitchTeleport | E::RotBurst => SoundCue::GlitchBlip,
        E::DataCore => SoundCue::DataChirp,
        E::LandmarkFound => SoundCue::AnomalyHum,
    }
}

/// The sound of a Living Dark event (Wave 8; exhaustive: a new event needs a sound).
#[must_use]
pub const fn living_dark_cue(
    what: fatherload_simulation::living_dark::LivingDarkEvent,
) -> SoundCue {
    use fatherload_simulation::living_dark::LivingDarkEvent as E;
    match what {
        E::SporeBurst => SoundCue::SporePuff,
        E::RootsRegrew | E::RootAxeFired => SoundCue::RootCreak,
        E::FleshHealed => SoundCue::FleshSquelch,
        E::RootsBurned => SoundCue::FireCrackle,
        E::TideRising | E::TideEbbing => SoundCue::FluidRush,
        E::CourtStanding | E::ErrandDone | E::SwarmTruce => SoundCue::HiveChime,
        E::SwarmBroken | E::GateRewards => SoundCue::MilestoneFanfare,
        E::CourtHostile => SoundCue::CreatureScreech,
        E::MindAwakened | E::MindExposed | E::MindRegrew => SoundCue::MindRoar,
        E::MindPossessed => SoundCue::WispChime,
        E::MindDefeated => SoundCue::CreatureDeath,
        E::CapBounce => SoundCue::BombBounce,
        E::Bought => SoundCue::Upgrade,
    }
}

/// The voice of a Hollow World beast (Wave 9) for its tell: a roc's cry from its perch, a
/// longneck's or crestback's bellow, the serpent's roar. `None` for the rest.
#[must_use]
pub const fn hollow_voice(kind: EnemyKind) -> Option<SoundCue> {
    Some(match kind {
        EnemyKind::StoopRoc => SoundCue::RocCry,
        EnemyKind::Skyneck | EnemyKind::Crestback => SoundCue::BeastBellow,
        EnemyKind::SunSerpent => SoundCue::SerpentRoar,
        _ => return None,
    })
}

/// The sound of a Hollow World event (Wave 9; exhaustive: a new event needs a sound).
#[must_use]
pub const fn hollow_cue(what: fatherload_simulation::hollow::HollowEvent) -> SoundCue {
    use fatherload_simulation::hollow::HollowEvent as E;
    match what {
        E::Breakthrough => SoundCue::SunSting,
        E::InnerDawn => SoundCue::DawnRelief,
        E::InnerNight => SoundCue::NightFalls,
        E::Mounted => SoundCue::BeastBellow,
        E::Dismounted | E::LandedUpsideDown => SoundCue::BombBounce,
        E::RoofLanding => SoundCue::Damage,
        E::Overexposed => SoundCue::GeigerAlert,
        E::AnchorEngaged => SoundCue::AnchorThunk,
        E::AnchorReleased => SoundCue::UiBack,
        E::SerpentAwakened | E::SerpentExposed => SoundCue::SerpentRoar,
        E::ScaleBroken => SoundCue::CoreStrike,
        E::ScalesReignited | E::SerpentTell => SoundCue::FireWhoosh,
        E::FlareWarning => SoundCue::FlareSwell,
        E::SerpentDefeated => SoundCue::MilestoneFanfare,
        E::GustBurst => SoundCue::WindGust,
        E::MiteLatched => SoundCue::MothFlutter,
        E::FlareflyBurst => SoundCue::SmallBlast,
    }
}

/// The sound of a far Strange Layers event (Wave 9; exhaustive: a new event needs a sound).
#[must_use]
pub const fn far_strange_cue(
    what: fatherload_simulation::far_strange::FarStrangeEvent,
) -> SoundCue {
    use fatherload_simulation::far_strange::FarStrangeEvent as E;
    match what {
        E::TimeShift | E::TimeBubble => SoundCue::TimeBubbleBurst,
        E::Ambered => SoundCue::FleshSquelch,
        E::Rewound => SoundCue::RewindWarp,
        E::MiteCrumbled => SoundCue::BoneCrumble,
        E::SonarPing => SoundCue::EchoPing,
        E::QuartzRang => SoundCue::QuartzRing,
        E::Shriek => SoundCue::Shriek,
        E::FilterSwapped | E::KeyedOpened => SoundCue::KeyedGlass,
        E::LandmarkFound => SoundCue::AnomalyHum,
        E::GlitchAwoke | E::GlitchRewriting => SoundCue::GlitchRewrite,
        E::GlitchRewrote | E::GlitchBlinked => SoundCue::GlitchBlink,
        E::GlitchDefeated => SoundCue::CreatureDeath,
        E::GateRewards => SoundCue::MilestoneFanfare,
        E::Bought => SoundCue::Upgrade,
    }
}

/// The sound of a deep supply spent (Wave 10 polish): the ballast's rush, the flare's hiss,
/// the bait's throw, the filter glass's clink.
#[must_use]
pub const fn supply_cue(what: fatherload_simulation::workshop::SupplyEvent) -> SoundCue {
    use fatherload_simulation::workshop::SupplyEvent as E;
    match what {
        E::BallastBlown => SoundCue::SteamHiss,
        E::FlareLit => SoundCue::FlareIgnite,
        E::BaitThrown => SoundCue::ExplosiveThrow,
        E::FilterFitted => SoundCue::KeyedGlass,
        E::FilterWornOut => SoundCue::UiError,
    }
}

/// The sound of a buried city event (Wave 10; exhaustive: a new event needs a sound).
#[must_use]
pub const fn city_cue(what: fatherload_simulation::ancient_city::CityEvent) -> SoundCue {
    use fatherload_simulation::ancient_city::CityEvent as E;
    match what {
        E::TramBoarded | E::TramArrived => SoundCue::CityTramBell,
        E::TramAlighted | E::LiftBoarded | E::LiftAlighted => SoundCue::CityLiftChime,
        E::PistonBoarded => SoundCue::CityPistonHiss,
        E::PistonLaunched => SoundCue::CityPistonThump,
        E::Crushed => SoundCue::CityCrusherSlam,
        E::AlarmRising | E::AlarmHunt => SoundCue::CityAlarm,
        E::ArchivistWoke | E::ArchivistSpoke => SoundCue::CityArchivistChirp,
        E::ArchivistScanned => SoundCue::CityArchivistScan,
        E::ChartRead => SoundCue::CityChartChord,
        E::VaultOpened => SoundCue::CityVaultGive,
        E::LandmarkFound => SoundCue::AnomalyHum,
        E::CargoStolen => SoundCue::CityCargoSnatch,
        E::FuelJolted => SoundCue::CitySparkZap,
    }
}

/// The sound of a Heart event (Wave 10; exhaustive: a new event needs a sound).
#[must_use]
pub const fn heart_cue(what: fatherload_simulation::heart::HeartEvent) -> SoundCue {
    use fatherload_simulation::heart::HeartEvent as E;
    match what {
        E::DarkMatterMined | E::RigFloating | E::RigSettled => SoundCue::HeartDarkMatter,
        // Wave 11 carryover: the eclipse begins with a well's deep groan.
        E::WellCaught | E::HorizonCrush | E::Eclipse => SoundCue::HeartWellGroan,
        E::RailBoost | E::RailCatch => SoundCue::HeartRailZap,
        E::LandmarkFound => SoundCue::AnomalyHum,
        E::HeartStirred | E::Ignition | E::Collapse | E::Reforged => SoundCue::HeartIgnite,
        E::CoilBroken => SoundCue::HeartCoilBreak,
        E::PulseTell | E::ExhaleTell => SoundCue::HeartInhale,
        E::Pulse => SoundCue::HeartPull,
        E::Exhale => SoundCue::HeartExhale,
        E::SparkBorn => SoundCue::FireWhoosh,
        E::HeartSlept => SoundCue::UiBack,
        E::HeartAwakened | E::CableLit => SoundCue::HeartCableLight,
        E::StarDriveFitted | E::SpaceUnlocked | E::StarsOpen => SoundCue::MilestoneFanfare,
        E::EelLeapt => SoundCue::HeartMetalSplash,
        E::MothDrank => SoundCue::MothFlutter,
    }
}

/// The sound of a round body's ground event (Wave 11; a clean touchdown is the landing's own
/// thud). Exhaustive: a new event needs an answer.
#[must_use]
pub const fn planet_cue(
    what: fatherload_simulation::planet::landing::PlanetEvent,
) -> Option<SoundCue> {
    use fatherload_simulation::planet::landing::PlanetEvent as E;
    match what {
        E::Landed => None,
        E::Slid => Some(SoundCue::CartClunk),
        E::HullScraped | E::Tipped => Some(SoundCue::Damage),
        E::Righted => Some(SoundCue::LeverClunk),
    }
}

/// What each wild world event sounds like (Wave 12 planets-a; exhaustive: a new event needs a
/// sound).
#[must_use]
pub const fn wilds_cue(what: fatherload_simulation::planet::wilds::WildsEvent) -> SoundCue {
    use fatherload_simulation::planet::wilds::WildsEvent as E;
    match what {
        E::RootsSnag => SoundCue::ImpactRock,
        E::SporeBurst => SoundCue::SmokeBurst,
        E::LifesapMended => SoundCue::RepairWeld,
        E::Scalded => SoundCue::FireCrackle,
        E::PressureCreak => SoundCue::GateRumble,
        E::ColdBite | E::Refroze => SoundCue::Icing,
        E::IcicleCrack => SoundCue::StalactiteCrack,
        E::LandmarkRelic => SoundCue::MilestoneFanfare,
        E::SecretCache => SoundCue::CacheOpened,
        E::StormRising => SoundCue::Thunderclap,
        E::RimeBroke => SoundCue::PickBreak,
    }
}

/// The sound of an outland world's event (Wave 13 planets-c; exhaustive: a new event needs an
/// answer). A hazard's bite is the hull's own hit.
#[must_use]
pub const fn outlands_cue(
    what: fatherload_simulation::planet::outlands::OutlandsEvent,
) -> Option<SoundCue> {
    use fatherload_simulation::planet::outlands::OutlandsEvent as E;
    Some(match what {
        E::CapBounce => SoundCue::OutlandCapBounce,
        E::MyceliumRegrew => SoundCue::OutlandMyceliumCreep,
        E::GlowFlash => SoundCue::OutlandGlowflash,
        E::VentSwelling => SoundCue::OutlandVentSwell,
        E::VentBlast => SoundCue::OutlandVentBlast,
        E::PuffballBurst => SoundCue::SmokeBurst,
        E::GearsClank => SoundCue::OutlandGearClank,
        E::GearGrind => SoundCue::ImpactRock,
        E::RelayFlipped => SoundCue::OutlandRelayFlip,
        E::VaultOpened => SoundCue::FrontierVaultOpen,
        E::AshPoured => SoundCue::FrontierSandSlide,
        E::TombOpened => SoundCue::OutlandTombOpen,
        E::WispChill => SoundCue::OutlandWispChill,
        E::RelicFound => SoundCue::MilestoneFanfare,
        E::SecretCache => SoundCue::CacheOpened,
        E::BossWoke => SoundCue::OutlandBossWake,
        E::BossSummons => SoundCue::OutlandBroodCall,
        E::BossDefeated => SoundCue::OutlandBossFall,
        E::StormRising => SoundCue::FrontierStormRising,
        E::HazardBite => return None,
    })
}

/// The sound of a Wave 12 world's theme boss's event (Wave 14 finish; exhaustive): the music's
/// stings for the wake and the fall, the outland bosses' brood call, a fanfare for the relic.
#[must_use]
pub const fn lair_cue(what: fatherload_simulation::planet::lairs::LairEvent) -> SoundCue {
    use fatherload_simulation::planet::lairs::LairEvent as E;
    match what {
        E::BossWoke => SoundCue::LairBossWake,
        E::BossSummons => SoundCue::OutlandBroodCall,
        E::BossDefeated => SoundCue::LairBossFall,
        E::RelicFound => SoundCue::MilestoneFanfare,
    }
}

/// The sound of a frontier world's event (Wave 12 planets-b; exhaustive: a new event needs an
/// answer). The storm's own bed and the volcano's rumble loop are the place sounds.
#[must_use]
pub const fn frontier_cue(
    what: fatherload_simulation::frontier::FrontierEvent,
) -> Option<SoundCue> {
    use fatherload_simulation::frontier::FrontierEvent as E;
    Some(match what {
        E::SandSlid => SoundCue::FrontierSandSlide,
        E::FloorCrumbled => SoundCue::FrontierFloorCrumble,
        E::GlassShattered => SoundCue::FrontierGlassShatter,
        E::CrystalRang => SoundCue::FrontierCrystalRing,
        E::Splintered => SoundCue::FrontierSplinter,
        E::BeamLit => SoundCue::FrontierBeamLit,
        E::VaultOpened => SoundCue::FrontierVaultOpen,
        E::EruptionRumble => SoundCue::FrontierRumble,
        E::EruptionStarted => SoundCue::FrontierEruption,
        E::LavaBombLanded => SoundCue::FrontierLavaBomb,
        E::StormRising => SoundCue::FrontierStormRising,
        E::StormStarted => SoundCue::FrontierStormBreak,
        E::CrystalRegrew => SoundCue::FrontierCrystalGrow,
        E::DrifterChimed => SoundCue::FrontierChime,
        E::WyrmRipple => SoundCue::FrontierWyrmRipple,
        E::StormEnded | E::HazardBite => return None,
    })
}

/// The sound of an exotic world's event (Wave 13 planets-d; exhaustive: a new event needs an
/// answer). The hazards' bite is the hull's own hit.
#[must_use]
pub const fn exotic_cue(what: fatherload_simulation::exotic::ExoticEvent) -> Option<SoundCue> {
    use fatherload_simulation::exotic::ExoticEvent as E;
    Some(match what {
        E::LightningTell => SoundCue::ExoticCrackle,
        E::LightningStrike => SoundCue::ExoticBolt,
        E::PressureWarning => SoundCue::ExoticPressure,
        E::DiamondHail | E::MeteorImpact => SoundCue::ExoticImpact,
        E::GeyserHiss => SoundCue::ExoticHiss,
        E::GeyserErupt => SoundCue::ExoticGeyser,
        E::QuakeRumble => SoundCue::ExoticQuake,
        E::QuakeShock => SoundCue::StalactiteCrack,
        E::MeteorIncoming => SoundCue::ExoticWhistle,
        E::IceVented => SoundCue::ExoticVent,
        E::ConduitShock => SoundCue::ExoticZap,
        E::ConduitCut => SoundCue::ExoticPowerDown,
        E::StormRising => SoundCue::ExoticStormRising,
        E::BossAwoke => SoundCue::ExoticBossAwake,
        E::BossTell => SoundCue::ExoticBossTell,
        E::BossDefeated => SoundCue::ExoticBossDown,
        E::RelicFound => SoundCue::CacheOpened,
        E::CityDocked => SoundCue::ExoticDock,
        E::HazardBite => return None,
    })
}

/// What each flight event sounds like (Wave 11; exhaustive: a new event needs a sound).
#[must_use]
pub const fn flight_cue(what: fatherload_simulation::flight::FlightEvent) -> SoundCue {
    use fatherload_simulation::flight::FlightEvent as E;
    match what {
        E::EnteredSpace | E::ReturnedHome | E::ApproachBody => SoundCue::SkyBand,
        // Wave 13 art-audio: the drive's own moments.
        E::WarpArrived => SoundCue::SpaceWarpArrive,
        E::CruiseEngaged => SoundCue::SpaceCruiseEngage,
        E::CruiseDropped => SoundCue::UiBack,
        E::WarpSpooling => SoundCue::MachineCharge,
        E::WarpEngaged => SoundCue::SpaceWarpEngage,
        E::WarpInterrupted => SoundCue::GravityHum,
        E::WarpRefused => SoundCue::UiError,
        E::Landed => SoundCue::MoonLanding,
        E::TookOff => SoundCue::JetpackBurst,
        E::HeatWarning => SoundCue::StormSiren,
        E::HullScraped => SoundCue::ImpactRock,
        E::AirLow => SoundCue::OxygenAlarm,
        E::BeaconReturn => SoundCue::Rescue,
        E::Docked => SoundCue::SpaceDocked,
        E::Undocked => SoundCue::HatchOpen,
        E::ScannerPing => SoundCue::Scanner,
        E::NavUpdated => SoundCue::UiMove,
    }
}

/// What space's things sound like (Wave 12 systems; exhaustive: a new event needs an answer).
#[must_use]
pub const fn systems_cue(what: fatherload_simulation::flight::systems::SystemsEvent) -> SoundCue {
    use fatherload_simulation::flight::systems::SystemsEvent as E;
    match what {
        E::StarHeat => SoundCue::StormSiren,
        E::FlareWarning => SoundCue::SystemsFlareWarning,
        E::FlareStruck => SoundCue::SystemsFlareBurst,
        E::FlareSheltered => SoundCue::Ui,
        E::HoleWell | E::NebulaEntered => SoundCue::GravityHum,
        E::HoleStrain | E::HoleNoReturn => SoundCue::SystemsHoleWarning,
        E::HoleRescued => SoundCue::Rescue,
        E::WormholeEntered => SoundCue::SystemsTransit,
        E::WormholeExited => SoundCue::SkyBand,
        E::NebulaLeft | E::AsteroidDepleted => SoundCue::UiMove,
        E::OreMined | E::CometDust => SoundCue::SystemsOreCrunch,
        E::HoldFull => SoundCue::UiError,
        E::RockBump => SoundCue::ImpactRock,
        E::ParkingHold => SoundCue::HatchClose,
        E::MoonApproach => SoundCue::MoonLanding,
    }
}

/// What a body's life sounds like (Wave 12 bodylife): its weather rolling in, its dawn and
/// dusk, a sale at its market (exhaustive: a new event needs a sound).
#[must_use]
pub const fn body_life_cue(
    what: fatherload_simulation::world::BodyLifeEvent,
    detail: u32,
) -> SoundCue {
    use fatherload_simulation::{
        bodylife::WeatherId,
        bodylife::weather::{WeatherParticles, weather_kind},
        world::BodyLifeEvent as E,
    };
    match what {
        E::WeatherTurned => {
            #[allow(clippy::cast_possible_truncation, reason = "weather ids are 16-bit")]
            let kind = weather_kind(WeatherId(detail as u16));
            match kind.look.particles {
                WeatherParticles::Rain => SoundCue::Thunderclap,
                WeatherParticles::Aurora => SoundCue::WispChime,
                WeatherParticles::None => SoundCue::UiMove,
                WeatherParticles::Snow
                | WeatherParticles::Dust
                | WeatherParticles::Ash
                | WeatherParticles::Spores => SoundCue::WindGust,
            }
        }
        E::Dawn => SoundCue::Dawn,
        E::Dusk => SoundCue::Dusk,
        E::Sold => SoundCue::Sell,
    }
}

/// A station's sound for each of its events (exhaustive: a new event needs a sound).
#[must_use]
pub const fn station_cue(what: fatherload_simulation::station::StationEvent) -> SoundCue {
    use fatherload_simulation::station::StationEvent as E;
    match what {
        E::ShipUpgraded | E::AlienTechFitted => SoundCue::UpgradeFanfare,
        E::GearFitted => SoundCue::Upgrade,
        E::GoodsBought => SoundCue::StorageStow,
        E::GoodsSold | E::OreSold => SoundCue::Sell,
        E::MissionAccepted => SoundCue::StoryHook,
        E::MissionProgress => SoundCue::UiMove,
        E::MissionReady => SoundCue::StoryResolved,
        E::MissionPaid => SoundCue::MilestoneFanfare,
        E::MissionAbandoned => SoundCue::UiBack,
        E::RumourHeard => SoundCue::RumourHeard,
        E::Salvaged => SoundCue::SalvageOpened,
        E::Refused => SoundCue::UiError,
        E::BodyScanned => SoundCue::Scanner,
        E::HazardBiting => SoundCue::VacuumAlarm,
    }
}

/// The cue of an endgame event (Wave 13 endgame; exhaustive: a new event needs a sound).
#[must_use]
pub const fn endgame_cue(what: fatherload_simulation::endgame::EndgameEvent) -> SoundCue {
    use fatherload_simulation::endgame::EndgameEvent as E;
    match what {
        E::MaterialsBought => SoundCue::StorageStow,
        E::MarkRaised | E::StarwrightFitted | E::EstateExpanded => SoundCue::UpgradeFanfare,
        E::HeartlineFitted | E::GateFinished | E::QuarryDown => SoundCue::MilestoneFanfare,
        E::GatePartDelivered => SoundCue::Upgrade,
        E::GateJumped => SoundCue::UplinkWarp,
        E::CosmeticBought => SoundCue::Sell,
        E::NeighboursCalled => SoundCue::HomeFanfare,
        E::WorldLanded => SoundCue::MoonLanding,
        E::Refused => SoundCue::UiError,
    }
}

/// The cue of a space combat event (Wave 12 hostiles; exhaustive: a new event needs a sound).
/// `detail` tells a missile from the guns.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "each event's sound reads next to its own"
)]
pub const fn hostile_cue(
    what: fatherload_simulation::hostiles::HostileEvent,
    detail: u32,
) -> Option<SoundCue> {
    use fatherload_simulation::hostiles::{HostileEvent as E, ShotKind};
    match what {
        E::EncounterStarted | E::Interdicted => Some(SoundCue::StormSiren),
        E::Hailed => Some(SoundCue::SpaceHail),
        E::TributePaid => Some(SoundCue::Sell),
        E::TurnedHostile => Some(SoundCue::SpaceBossWarn),
        E::ShipDestroyed => Some(SoundCue::SpaceShipBoom),
        // The hull hit is `PlayerDamaged`'s.
        E::RigHit => None,
        E::ShieldDown => Some(SoundCue::SpaceShieldDown),
        E::Fired => match ShotKind::from_code(detail) {
            Some(ShotKind::Missile) => Some(SoundCue::SpaceMissile),
            Some(ShotKind::Cannon | ShotKind::Turret) => Some(SoundCue::SpaceCannon),
            Some(_) | None => Some(SoundCue::SpaceEnemyShot),
        },
        E::ShipHit => Some(SoundCue::SpaceShipHit),
        E::EscapeSpooling => Some(SoundCue::MachineCharge),
        E::Escaped => Some(SoundCue::UplinkWarp),
        E::EncounterOver => Some(SoundCue::UiBack),
        E::WreckLooted => Some(SoundCue::Sell),
        E::BossAppeared => Some(SoundCue::SpaceBossAppear),
        E::BossTelegraph => Some(SoundCue::SpaceBossWarn),
        E::BossDefeated | E::BountyClaimed => Some(SoundCue::Milestone),
        E::RaidLanded => Some(SoundCue::StormSiren),
        E::GearBought => Some(SoundCue::Upgrade),
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
        ExplosiveKind::TimeBubble => SoundCue::TimeBubbleBurst,
    }
}

/// The shot each weapon fires with (exhaustive: a new weapon needs a sound).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "a Moon weapon shares its family's sound, kept on its own line"
)]
pub const fn weapon_cue(weapon: WeaponKind) -> SoundCue {
    match weapon {
        WeaponKind::RivetGun => SoundCue::RivetShot,
        WeaponKind::ScatterBlaster => SoundCue::ScatterShot,
        WeaponKind::PlasmaCutter => SoundCue::CutterZap,
        WeaponKind::RocketLauncher => SoundCue::RocketLaunch,
        WeaponKind::Harpoon => SoundCue::HarpoonShot,
        WeaponKind::Railgun | WeaponKind::VoidLance => SoundCue::RailgunShot,
        WeaponKind::ArcRifle => SoundCue::ArcZap,
        WeaponKind::GravityGun => SoundCue::GravityHum,
        WeaponKind::TetherHarpoon => SoundCue::TetherShot,
        WeaponKind::FlakCannon => SoundCue::FlakShot,
        WeaponKind::LunarCutter => SoundCue::CutterZap,
        WeaponKind::LowGMortar => SoundCue::RocketLaunch,
        WeaponKind::Sidearm => SoundCue::PistolPop,
        WeaponKind::Rifle => SoundCue::RifleCrack,
        WeaponKind::Shotgun => SoundCue::ShotgunBoom,
        WeaponKind::Wrench | WeaponKind::HandPick => SoundCue::WrenchSwing,
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

/// The sound of placing `item` (WP-BASE): a claim beacon's call sign, a module bolting down, or
/// a block set in place.
#[must_use]
#[allow(clippy::match_same_arms, reason = "defences keep a row of their own")]
pub const fn build_cue(item: Buildable) -> SoundCue {
    match item {
        Buildable::Module(ModuleKind::Beacon) => SoundCue::ClaimStaked,
        Buildable::Module(_) => SoundCue::ModuleBuilt,
        Buildable::Tile(_) | Buildable::BackWall(_) | Buildable::Fixture(_) => SoundCue::BuildPlace,
        Buildable::Defence(_) => SoundCue::ModuleBuilt,
    }
}

/// The sound of a base module service (exhaustive: a new service needs a sound).
#[must_use]
pub const fn service_cue(service: ModuleService) -> SoundCue {
    match service {
        ModuleService::Refuel => SoundCue::FuelPump,
        ModuleService::OrderSupply => SoundCue::SupplyDrop,
        ModuleService::Repair => SoundCue::RepairWeld,
        ModuleService::DepositCargo | ModuleService::DepositBlocks | ModuleService::WithdrawAll => {
            SoundCue::StorageStow
        }
        ModuleService::SellCargo => SoundCue::UplinkBeam,
        ModuleService::SetHome => SoundCue::HomeSet,
        ModuleService::CycleAccess => SoundCue::Ui,
        ModuleService::RefineOre => SoundCue::RefineryLoad,
        ModuleService::CollectRefined => SoundCue::Sell,
        ModuleService::Craft(_) => SoundCue::WorkshopCraft,
        ModuleService::Sleep => SoundCue::BedDown,
        ModuleService::Beam => SoundCue::UplinkWarp,
    }
}

/// The one-shot cue a gameplay event plays, if any. Exhaustive on purpose: a new event kind
/// fails to compile until someone decides how it sounds. Events answered `None` are covered
/// elsewhere (noted per arm).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "each event's sound reads next to its own comment"
)]
#[allow(clippy::too_many_lines, reason = "one arm per gameplay event kind")]
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
            VillageService::Shrine | VillageService::Monastery => Some(SoundCue::ShrineBlessing),
            VillageService::Cartographer => Some(SoundCue::Scanner),
            _ => None,
        },
        // Enemies: every tell, wind-up, shot, hit and death is heard from where it happens.
        // Night monsters (WP-NIGHT) have their own voices for the tell and the wind-up.
        GameplayEventKind::EnemyAlerted { kind, .. } => match night_voice(*kind) {
            Some(voice) => Some(voice),
            // The Hollow World's beasts (Wave 9): a roc's cry, a longneck's bellow.
            None if hollow_voice(*kind).is_some() => hollow_voice(*kind),
            None if tomb_voice(*kind).is_some() => tomb_voice(*kind),
            None => Some(enemy_cue(
                *kind,
                SoundCue::CreatureScreech,
                SoundCue::MachineAlert,
            )),
        },
        GameplayEventKind::EnemyWindup { kind, .. } => match night_voice(*kind) {
            None if tomb_voice(*kind).is_some() => tomb_voice(*kind),
            Some(SoundCue::HoundHowl | SoundCue::AlphaHowl) | None => Some(enemy_cue(
                *kind,
                SoundCue::CreatureHiss,
                SoundCue::MachineCharge,
            )),
            voice => voice,
        },
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
        GameplayEventKind::WeaponFired { weapon, .. }
        | GameplayEventKind::PilotFired { weapon, .. } => Some(weapon_cue(*weapon)),
        GameplayEventKind::WeaponDry { .. } | GameplayEventKind::PilotWeaponDry { .. } => {
            Some(SoundCue::WeaponDry)
        }
        GameplayEventKind::WeaponMounted { .. } | GameplayEventKind::PilotWeaponSwapped { .. } => {
            Some(SoundCue::WeaponMount)
        }
        GameplayEventKind::ProjectileImpact { surface, .. } => Some(impact_cue(*surface)),
        GameplayEventKind::EntityDestroyed { .. } => Some(SoundCue::TargetDestroyed),
        // Base building: placed where the piece went (the whole crew hears it), services from
        // the rig at the module.
        GameplayEventKind::BasePlaced { item, .. } => Some(build_cue(*item)),
        GameplayEventKind::BaseRemoved { .. } => Some(SoundCue::BuildRemove),
        GameplayEventKind::BaseServiceUsed { service, .. } => Some(service_cue(*service)),
        // The world clock: the rooster and birds at dawn, crickets at dusk, a tower bell at
        // midnight (the effects layer keeps these to seats out under the sky), the lullaby of
        // a skipped night; getting up and the host's sleep rule tick like a menu confirm.
        GameplayEventKind::Dawn { .. } => Some(SoundCue::Dawn),
        GameplayEventKind::Dusk { .. } => Some(SoundCue::Dusk),
        GameplayEventKind::NewDay { .. } => Some(SoundCue::Midnight),
        GameplayEventKind::NightSkipped { .. } => Some(SoundCue::NightSkipped),
        GameplayEventKind::GotUp { .. } | GameplayEventKind::SleepRuleChanged { .. } => {
            Some(SoundCue::Ui)
        }
        // The sky (F6): a swell as the view opens on a new band, a whoosh into an updraft and
        // frost crackling on a hull its de-icer cannot cover.
        GameplayEventKind::SkyBandEntered { .. } => Some(SoundCue::SkyBand),
        GameplayEventKind::UpdraftCaught { .. } => Some(SoundCue::Updraft),
        GameplayEventKind::RigIcing { .. } => Some(SoundCue::Icing),
        // Sky isles (WP-SKY): the cache's chimes, the buoy's pump, the collapse (placed in the
        // world by the effects layer) and the jet stream's rush.
        GameplayEventKind::SkyCacheOpened { .. } => Some(SoundCue::SkyCache),
        GameplayEventKind::BuoyRefuelled { .. } => Some(SoundCue::BuoyRefuel),
        GameplayEventKind::IsleCollapsing { .. } => Some(SoundCue::IsleCollapse),
        GameplayEventKind::JetStreamCaught { .. } => Some(SoundCue::JetStream),
        // The Moon (WP-MOON): the vacuum siren, the landing's sunrise sting, the colony's
        // till, the facility's chimes, rumble and the Warden's waking boom, a fanfare when it
        // falls, the core's vast hum and the uplink's warp.
        GameplayEventKind::MoonVacuumBreach { .. } => Some(SoundCue::VacuumAlarm),
        GameplayEventKind::MoonLanded { .. } => Some(SoundCue::MoonLanding),
        GameplayEventKind::ColonyServiceUsed {
            offer: fatherload_simulation::moon::ColonyOffer::SellHaul,
            ..
        } => Some(SoundCue::Sell),
        GameplayEventKind::ColonyServiceUsed { .. } => Some(SoundCue::Upgrade),
        GameplayEventKind::FacilityPylon { reset: false, .. } => Some(SoundCue::PylonChime),
        GameplayEventKind::FacilityPylon { reset: true, .. } => Some(SoundCue::UiError),
        GameplayEventKind::FacilityGateOpened { .. } => Some(SoundCue::GateOpen),
        GameplayEventKind::WardenAwakened { .. } => Some(SoundCue::WardenWake),
        GameplayEventKind::WardenStaggered { .. } => Some(SoundCue::GateOpen),
        GameplayEventKind::WardenDefeated { .. } => Some(SoundCue::Milestone),
        GameplayEventKind::MoonCoreReached { .. } => Some(SoundCue::CoreReached),
        GameplayEventKind::UplinkBeamed { .. } => Some(SoundCue::UplinkWarp),
        // Night (WP-NIGHT): a low sting as the monsters rise, warm relief at dawn, the
        // alpha's long howl over the land, and a puff of dust as each one burrows.
        GameplayEventKind::NightFell { .. } => Some(SoundCue::NightFalls),
        GameplayEventKind::NightSurvived { .. } => Some(SoundCue::DawnRelief),
        GameplayEventKind::RareNightSpawn { .. } => Some(SoundCue::AlphaHowl),
        GameplayEventKind::NightMonsterBurrowed { .. } => Some(SoundCue::Burrow),
        // The pilot on foot (Part II F5): from the player, like the rig's own sounds.
        GameplayEventKind::PilotExited { .. } => Some(SoundCue::HatchOpen),
        GameplayEventKind::PilotBoarded { .. } => Some(SoundCue::HatchClose),
        GameplayEventKind::PilotJumped { .. } | GameplayEventKind::PilotWallJumped { .. } => {
            Some(SoundCue::PilotJump)
        }
        GameplayEventKind::PilotLanded { hard: false, .. } => Some(SoundCue::PilotLand),
        GameplayEventKind::PilotLanded { hard: true, .. } => Some(SoundCue::PilotLandHard),
        GameplayEventKind::PilotLedgeGrab { .. } => Some(SoundCue::LedgeGrab),
        GameplayEventKind::PilotSplashed { .. } => Some(SoundCue::Splash),
        GameplayEventKind::PilotHurt { .. } => Some(SoundCue::PilotHurt),
        GameplayEventKind::PilotDied { .. } => Some(SoundCue::PilotDown),
        GameplayEventKind::PilotOxygenLow { .. } => Some(SoundCue::OxygenAlarm),
        GameplayEventKind::JetpackIgnited { .. } => Some(SoundCue::JetpackBurst),
        GameplayEventKind::JetpackEmpty { .. } => Some(SoundCue::JetpackSputter),
        GameplayEventKind::PickStruck { broke: false, .. } => Some(SoundCue::PickChip),
        GameplayEventKind::PickStruck { broke: true, .. } => Some(SoundCue::PickBreak),
        GameplayEventKind::PickClinked { .. } => Some(SoundCue::PickClink),
        // WP-PILOT (shots, dry clicks, swaps and wall jumps share the arms above).
        GameplayEventKind::PilotRope { action, .. } => Some(match action {
            fatherload_simulation::pilot::RopeEvent::Hooked
            | fatherload_simulation::pilot::RopeEvent::Winched => SoundCue::GrappleFire,
            fatherload_simulation::pilot::RopeEvent::Missed => SoundCue::ExplosiveThrow,
            fatherload_simulation::pilot::RopeEvent::Released => SoundCue::LedgeGrab,
        }),
        GameplayEventKind::PilotGlide { hover, .. } => Some(if *hover {
            SoundCue::JetpackBurst
        } else {
            SoundCue::GliderOpen
        }),
        GameplayEventKind::MechanismUsed { mechanism, .. } => Some(match mechanism {
            fatherload_simulation::mechanism::MechanismEvent::LeverPulled { .. }
            | fatherload_simulation::mechanism::MechanismEvent::PlatePressed
            | fatherload_simulation::mechanism::MechanismEvent::PlateReleased => {
                SoundCue::LeverClunk
            }
            fatherload_simulation::mechanism::MechanismEvent::GateOpened
            | fatherload_simulation::mechanism::MechanismEvent::GateClosed => SoundCue::GateGrind,
            fatherload_simulation::mechanism::MechanismEvent::KeycardAccepted { .. } => {
                SoundCue::KeycardBeep
            }
            fatherload_simulation::mechanism::MechanismEvent::Locked { .. } => SoundCue::UiError,
        }),
        GameplayEventKind::ChestOpened { .. } => Some(SoundCue::CrateOpened),
        GameplayEventKind::RigRecall { outcome, .. } => Some(match outcome {
            fatherload_simulation::pilot::RecallOutcome::Started => SoundCue::UplinkBeam,
            fatherload_simulation::pilot::RecallOutcome::Arrived => SoundCue::HomeSet,
            fatherload_simulation::pilot::RecallOutcome::NoPath => SoundCue::UiError,
        }),
        GameplayEventKind::PilotExposed { heat, .. } => Some(if *heat {
            SoundCue::FireCrackle
        } else {
            SoundCue::Icing
        }),
        // Tombs and crypts (WP-TOMBS, T27): cues from the existing patches; T28 added the
        // side-view traps and creatures below (and the run music and guardian theme, looped by
        // the director); T29 adds the crypts'.
        GameplayEventKind::TombEntered { .. } => Some(SoundCue::VaultOpened),
        GameplayEventKind::TombSeal { opened: true, .. } => Some(SoundCue::GateGrind),
        GameplayEventKind::TombSeal { opened: false, .. } => Some(SoundCue::UiError),
        GameplayEventKind::TombAmbush { boss: false, .. } => Some(SoundCue::CreatureScreech),
        GameplayEventKind::TombAmbush { boss: true, .. } => Some(SoundCue::AlphaHowl),
        // Side-view tombs (T28): each trap's own sound where it went off, the guardian's
        // voice as it turns.
        GameplayEventKind::TombTrapSprung { kind, .. } => trap_cue(*kind),
        GameplayEventKind::TombGuardianPhase { kind, .. } => match tomb_voice(*kind) {
            Some(voice) => Some(voice),
            None => Some(SoundCue::AlphaHowl),
        },
        GameplayEventKind::TombGuardianDefeated { .. } => Some(SoundCue::Milestone),
        GameplayEventKind::TombFloorCleared { .. } => Some(SoundCue::PoiDiscovered),
        GameplayEventKind::TombRelicPicked { relic: Some(_), .. }
        | GameplayEventKind::TombShrineRested { .. } => Some(SoundCue::ShrineBlessing),
        GameplayEventKind::TombRelicPicked { relic: None, .. } => Some(SoundCue::UiBack),
        GameplayEventKind::TombCacheOpened { .. } => Some(SoundCue::CacheOpened),
        GameplayEventKind::TombMemberDowned { revived: false, .. } => Some(SoundCue::PilotDown),
        GameplayEventKind::TombMemberDowned { revived: true, .. } => Some(SoundCue::HomeSet),
        GameplayEventKind::TombRunEnded { outcome, .. } => Some(if outcome.banks() {
            SoundCue::Milestone
        } else {
            SoundCue::PilotDown
        }),
        // Top-down crypts (WP-TOMBS, T29): every trap warns before it bites, out loud.
        GameplayEventKind::CryptRoll { .. } => Some(SoundCue::DodgeRoll),
        GameplayEventKind::CryptTrap { kind, fired, .. } => match (kind, fired) {
            (CryptTrapKind::Spikes, false) => Some(SoundCue::PickClink),
            (CryptTrapKind::Spikes, true) => Some(SoundCue::SpikeShing),
            (CryptTrapKind::FlameJet, false) => Some(SoundCue::FireCrackle),
            (CryptTrapKind::FlameJet, true) => Some(SoundCue::FireWhoosh),
            (CryptTrapKind::ArrowPlate, false) => Some(SoundCue::LeverClunk),
            (CryptTrapKind::ArrowPlate, true) => Some(SoundCue::ArrowTwang),
            (CryptTrapKind::BoulderChute, false) => Some(SoundCue::GateRumble),
            (CryptTrapKind::BoulderChute, true) => Some(SoundCue::StoneSlam),
            (CryptTrapKind::CrackedFloor, false) => Some(SoundCue::StalactiteCrack),
            (CryptTrapKind::CrackedFloor | CryptTrapKind::Pit, true) => Some(SoundCue::PickBreak),
            (CryptTrapKind::Pit, false) => None,
        },
        GameplayEventKind::CryptPitFall { .. } => Some(SoundCue::PitDrop),
        GameplayEventKind::CryptLordPhase { .. } => Some(SoundCue::LordRoar),
        GameplayEventKind::CryptTell { attack, .. } => match attack {
            CryptAttack::Slam => Some(SoundCue::MachineCharge),
            CryptAttack::Fan | CryptAttack::Ring | CryptAttack::Spiral => Some(SoundCue::HexBolt),
            CryptAttack::Charge => Some(SoundCue::AlphaHowl),
        },
        GameplayEventKind::CryptSlam { .. } => Some(SoundCue::StoneSlam),
        GameplayEventKind::CryptSummon { .. } => Some(SoundCue::CreatureScreech),
        // Fluids (Part III F9): the breach gushes where it opened, reactions hiss or fizz where
        // they happen, and a rig plunges in (or splashes out) from the player.
        GameplayEventKind::FluidBreached { .. } => Some(SoundCue::FluidRush),
        GameplayEventKind::FluidReacted {
            reaction: FluidReaction::Quenched | FluidReaction::Solidified,
            ..
        } => Some(SoundCue::SteamHiss),
        GameplayEventKind::FluidReacted {
            reaction: FluidReaction::Dissolved,
            ..
        } => Some(SoundCue::AcidFizz),
        GameplayEventKind::RigSubmerged { .. } => Some(SoundCue::RigSplash),
        GameplayEventKind::RigSurfaced { .. } => Some(SoundCue::Splash),
        // Depth logistics (Part III F10): the cab's machinery, a new stop on the network, the
        // counter's own sounds, the jump's warp, the dive's blast and the gate's alarm.
        GameplayEventKind::LiftRideStarted { .. } => Some(SoundCue::GateGrind),
        GameplayEventKind::LiftArrived { .. } => Some(SoundCue::LeverClunk),
        GameplayEventKind::LiftRestored { .. } => Some(SoundCue::MachineCharge),
        GameplayEventKind::LogisticsPlaced { .. } => Some(SoundCue::BuildPlace),
        GameplayEventKind::OutpostReached { .. } => Some(SoundCue::VillageArrival),
        GameplayEventKind::OutpostClaimed { .. } => Some(SoundCue::ClaimStaked),
        GameplayEventKind::OutpostServiced { service, .. } => Some(match service {
            fatherload_simulation::logistics::OutpostService::Refuel => SoundCue::FuelPump,
            fatherload_simulation::logistics::OutpostService::Repair => SoundCue::RepairWeld,
            fatherload_simulation::logistics::OutpostService::SellCargo => SoundCue::Sell,
            fatherload_simulation::logistics::OutpostService::BuyDiveCharges
            | fatherload_simulation::logistics::OutpostService::Claim => SoundCue::Upgrade,
            fatherload_simulation::logistics::OutpostService::SetHome => SoundCue::HomeSet,
        }),
        GameplayEventKind::BeaconTravelled { .. } => Some(SoundCue::UplinkWarp),
        GameplayEventKind::DiveChargeFired { .. } => Some(SoundCue::ShapedBlast),
        GameplayEventKind::DescentGateBlocked { .. } => Some(SoundCue::VacuumAlarm),
        // The authored Crust (Wave 8): the cart's clunk and clack, timber groaning and the roof
        // coming down, slate shuddering and sliding, a strongbox's latch, the Sinkhole's rumble,
        // escape and fanfare, the gate's alarm, the tube's breath and the creatures' calls.
        GameplayEventKind::CartBoarded { .. } => Some(SoundCue::CartClunk),
        GameplayEventKind::CartSwitched { .. } => Some(SoundCue::CartSwitch),
        GameplayEventKind::CartLeft { .. } => Some(SoundCue::CartClunk),
        GameplayEventKind::SupportCreaked { .. } => Some(SoundCue::TimberCreak),
        GameplayEventKind::SupportCollapsed { .. } | GameplayEventKind::SlateSlid { .. } => {
            Some(SoundCue::RoofCollapse)
        }
        GameplayEventKind::SlateShudder { .. } => Some(SoundCue::StalactiteCrack),
        GameplayEventKind::StrongboxOpened { .. } => Some(SoundCue::CrateOpened),
        GameplayEventKind::SinkholeRumble { .. } => Some(SoundCue::SinkholeRumble),
        GameplayEventKind::SinkholeEscaped { .. } => Some(SoundCue::Milestone),
        GameplayEventKind::SinkholeCaught { .. } => Some(SoundCue::Damage),
        GameplayEventKind::SinkholeReset { .. } => Some(SoundCue::VaultReset),
        GameplayEventKind::DeepClaimGranted { .. } => Some(SoundCue::DeepClaimFanfare),
        GameplayEventKind::DeepClaimMissing { .. } => Some(SoundCue::VacuumAlarm),
        GameplayEventKind::LavaTubeEntered { .. } => Some(SoundCue::TubeBreath),
        GameplayEventKind::CrustCreature { .. } => Some(SoundCue::CreatureHiss),
        // Descent II, the Old Deep (Wave 8).
        GameplayEventKind::ResonanceChain { .. } => Some(SoundCue::ResonanceChime),
        GameplayEventKind::FossilBoneFreed { intact, .. } => Some(if *intact {
            SoundCue::ArtifactPickup
        } else {
            SoundCue::BoneCrumble
        }),
        GameplayEventKind::SkeletonCompleted { .. } => Some(SoundCue::Sell),
        GameplayEventKind::GasTideTurned { high: true } => Some(SoundCue::SteamHiss),
        GameplayEventKind::GasTideTurned { high: false } => None,
        GameplayEventKind::AmberStuck { .. } => Some(SoundCue::AcidFizz),
        GameplayEventKind::CathedralValveOpened { .. } => Some(SoundCue::ValveTurn),
        GameplayEventKind::CathedralLiftRestored { .. } => Some(SoundCue::MachineCharge),
        GameplayEventKind::DoseWarning { .. } => Some(SoundCue::GeigerAlert),
        GameplayEventKind::MutationGained { .. } => Some(SoundCue::DeepLog),
        GameplayEventKind::MachineryWoke { .. } => Some(SoundCue::GateGrind),
        GameplayEventKind::PistonStruck { .. } => Some(SoundCue::PistonSlam),
        GameplayEventKind::HeatStormChanged { phase } => match phase {
            fatherload_simulation::old_deep::StormPhase::Gathering => Some(SoundCue::StormSiren),
            fatherload_simulation::old_deep::StormPhase::Raging => Some(SoundCue::FireWhoosh),
            fatherload_simulation::old_deep::StormPhase::Calm => None,
        },
        GameplayEventKind::LavaPoured { .. } => Some(SoundCue::FireCrackle),
        GameplayEventKind::DeepLogRead { .. } => Some(SoundCue::DeepLog),
        GameplayEventKind::StarCoreAwoke | GameplayEventKind::StarCoreBeaten { .. } => {
            Some(SoundCue::CoreWake)
        }
        GameplayEventKind::StarCorePhase { phase, .. } => match phase {
            fatherload_simulation::old_deep::CorePhase::Exposed => Some(SoundCue::MachineCharge),
            fatherload_simulation::old_deep::CorePhase::Shielded => Some(SoundCue::CoreStrike),
            _ => None,
        },
        GameplayEventKind::GlowmotherAwoke { .. } => Some(SoundCue::CreatureScreech),
        GameplayEventKind::GlowmotherDefeated { .. } => Some(SoundCue::CreatureDeath),
        // The Living Dark (Wave 8).
        GameplayEventKind::LivingDark { what, .. } => Some(living_dark_cue(*what)),
        // The distortion strata (Wave 9).
        GameplayEventKind::Distortion { what, .. } => Some(distortion_cue(*what)),
        GameplayEventKind::DistortionLandmarkFound { .. } => Some(SoundCue::AnomalyHum),
        // The far Strange Layers (Wave 9).
        GameplayEventKind::FarStrange { what, .. } => Some(far_strange_cue(*what)),
        // Deep supplies at work (Wave 10 polish).
        GameplayEventKind::Supply { what, .. } => Some(supply_cue(*what)),
        // The builders' buried city (Wave 10).
        GameplayEventKind::AncientCity { what, .. } => Some(city_cue(*what)),
        // Descent VII part B (Wave 10).
        GameplayEventKind::Garrison { what, .. } => Some(garrison_cue(*what)),
        // The Fathers' expedition (wave 8 story): the canister's tumblers, a sealed one's
        // refusal, Dov's voice (static on the line, or off the tape) and the anomaly's swell.
        GameplayEventKind::ExpeditionLogFound { .. } => Some(SoundCue::LogCanister),
        // The story finale (wave 10 story): a new Descent's bell call and the ending's swell
        // (the gate fights play their own fanfares).
        GameplayEventKind::Story { what, .. } => match what {
            fatherload_simulation::expedition::progress::StoryEvent::DescentReached(_) => {
                Some(SoundCue::StoryDescentCall)
            }
            fatherload_simulation::expedition::progress::StoryEvent::BossBeaten(_) => None,
            fatherload_simulation::expedition::progress::StoryEvent::CableLit => {
                Some(SoundCue::StoryCableLit)
            }
        },
        // Arriving in another place (F11: landed, took off, came home): the warp hum the
        // beacons use, until the flight track gives arrivals their own sounds.
        GameplayEventKind::PlaceChanged { .. } => Some(SoundCue::UplinkWarp),
        GameplayEventKind::ExpeditionLogSealed { .. } => Some(SoundCue::LeverClunk),
        GameplayEventKind::ExpeditionEcho { echo, .. } => {
            let index = echo.saturating_sub(1) as usize;
            let echoes = &fatherload_simulation::expedition::ECHOES;
            if index < echoes.len()
                && matches!(
                    echoes[index].kind,
                    fatherload_simulation::expedition::EchoKind::Ghost
                )
            {
                Some(SoundCue::GhostVoice)
            } else {
                Some(SoundCue::RadioEcho)
            }
        }
        GameplayEventKind::AnomalyEntered { .. } => Some(SoundCue::AnomalyHum),
        // The Drowned and Frozen (Wave 9).
        GameplayEventKind::Drowned { what, .. } => Some(drowned_cue(*what)),
        // The Hollow World (Wave 9).
        GameplayEventKind::Hollow { what, .. } => Some(hollow_cue(*what)),
        // Descent VIII, the Heart (Wave 10).
        GameplayEventKind::Heart { what, .. } => Some(heart_cue(*what)),
        GameplayEventKind::HeartLandmarkFound { .. } => Some(SoundCue::AnomalyHum),
        // Round bodies (Wave 11): a bad touchdown scrapes or knocks the hull; righting clunks.
        GameplayEventKind::Planet { what, .. } => planet_cue(*what),
        // The frontier worlds (Wave 12 planets-b).
        GameplayEventKind::Frontier { what, .. } => frontier_cue(*what),
        // The exotic worlds (Wave 13 planets-d).
        GameplayEventKind::Exotic { what, .. } => exotic_cue(*what),
        // Space flight (Wave 11).
        GameplayEventKind::Flight { what, .. } => Some(flight_cue(*what)),
        // Space's things (Wave 12 systems).
        GameplayEventKind::SpaceSystems { what, .. } => Some(systems_cue(*what)),
        GameplayEventKind::BodyLife { what, detail, .. } => Some(body_life_cue(*what, *detail)),
        // The wild worlds (Wave 12 planets-a).
        GameplayEventKind::Wilds { what, .. } => Some(wilds_cue(*what)),
        // The outland worlds (Wave 13 planets-c).
        GameplayEventKind::Outlands { what, .. } => outlands_cue(*what),
        // The Wave 12 worlds' theme bosses (Wave 14 finish).
        GameplayEventKind::Lair { what, .. } => Some(lair_cue(*what)),
        GameplayEventKind::Alien { what, species, .. } => Some(alien_cue(*what, *species)),
        // Space stations (Wave 12).
        GameplayEventKind::Station { what, .. } => Some(station_cue(*what)),
        // Space combat (Wave 12).
        GameplayEventKind::Hostile { what, detail, .. } => hostile_cue(*what, *detail),
        // The endgame (Wave 13).
        GameplayEventKind::Endgame { what, .. } => Some(endgame_cue(*what)),
        // The galaxy story (Wave 13).
        GameplayEventKind::Galaxy { what, .. } => Some(galaxy_cue(*what)),

        GameplayEventKind::PackTransferred { .. } => Some(SoundCue::StorageStow),
        // People: a murmur from whoever you talk to, coins for a trade, a sparkle for a gift, a
        // warm chime when someone warms to you (a cooler standing gets the back blip), a hush
        // when a rumour marks your map. The command's generic event is silent.
        GameplayEventKind::PersonTalked { .. } => Some(SoundCue::PersonGreet),
        GameplayEventKind::PersonTraded { .. } | GameplayEventKind::HomeRushed { .. } => {
            Some(SoundCue::Sell)
        }
        GameplayEventKind::PersonGifted { .. } | GameplayEventKind::SiteEventAnswered { .. } => {
            Some(SoundCue::GiftGiven)
        }
        GameplayEventKind::AffinityChanged { warmer, .. } => Some(if *warmer {
            SoundCue::AffinityUp
        } else {
            SoundCue::UiBack
        }),
        GameplayEventKind::RumourMarked { .. } => Some(SoundCue::RumourHeard),
        // Stories: a curious hook when a person pitches one, a quill for taking it on and for
        // every beat, the music box at the climax, a warm cadence at the end, the back blip
        // when one is given up.
        GameplayEventKind::ArcOffered { .. } => Some(SoundCue::StoryHook),
        GameplayEventKind::ArcAccepted { .. } | GameplayEventKind::ArcStepDone { .. } => {
            Some(SoundCue::StoryQuill)
        }
        GameplayEventKind::ArcClimax { .. } => Some(SoundCue::StoryClimax),
        GameplayEventKind::ArcCompleted { .. } => Some(SoundCue::StoryResolved),
        GameplayEventKind::ArcAbandoned { .. } => Some(SoundCue::UiBack),
        // Upgrade reward moments: the purchase fanfare (a bomb pack is just the purchase
        // chime), a milestone's fanfare, and the big one when the sky opens.
        GameplayEventKind::UpgradeInstalled { upgrade, .. } => Some(
            if matches!(
                upgrade,
                fatherload_simulation::economy::UpgradeKind::BombPack
            ) {
                SoundCue::Upgrade
            } else {
                SoundCue::UpgradeFanfare
            },
        ),
        GameplayEventKind::MilestoneEarned { milestone, .. } => Some(
            if matches!(
                milestone,
                fatherload_simulation::milestone::Milestone::ActTwoUnlocked
            ) {
                SoundCue::ActTwoFanfare
            } else {
                SoundCue::MilestoneFanfare
            },
        ),
        // Sky continents, sky enemies and Act II weapons (T21): thunder for lightning, a
        // crackle per arc link, the gravity gun's fling, the harpoon biting rock (the line
        // letting go clicks like a menu back), shrapnel bursts, temple stone and bronze, and
        // the sky enemies' calls (an airship going down is a big blast).
        GameplayEventKind::LightningArc { .. } => Some(SoundCue::Thunderclap),
        GameplayEventKind::ArcChained { .. } => Some(SoundCue::ArcZap),
        GameplayEventKind::GravityThrow { .. } => Some(SoundCue::GravityThrow),
        GameplayEventKind::TetherAttached { .. } => Some(SoundCue::TetherHook),
        GameplayEventKind::TetherReleased { .. } => Some(SoundCue::UiBack),
        GameplayEventKind::FlakBurst { .. } => Some(SoundCue::FlakBurst),
        GameplayEventKind::TempleLeverPulled { .. } => Some(SoundCue::LeverClunk),
        GameplayEventKind::TempleGateOpened { .. } => Some(SoundCue::GateRumble),
        GameplayEventKind::RelicChestOpened { .. } => Some(SoundCue::CacheOpened),
        GameplayEventKind::SkyEnemyAction { action, .. } => Some(match action {
            fatherload_simulation::enemy::SkyEnemyAction::Swoop => SoundCue::SwoopScreech,
            fatherload_simulation::enemy::SkyEnemyAction::BoardersDropped => SoundCue::PirateHorn,
            fatherload_simulation::enemy::SkyEnemyAction::AirshipDowned => SoundCue::Explosion,
            fatherload_simulation::enemy::SkyEnemyAction::NesterEnraged => SoundCue::HarpyScreech,
        }),
        // Homes: a stamp for a deed or a contract, hammers for each new stage, the keys'
        // fanfare when it is finished, a knock when the site needs you, coins for a rush, a
        // handover sparkle for an answer and a wooden thunk for furniture. The command's
        // generic event is silent.
        GameplayEventKind::PlotBought { .. } | GameplayEventKind::ContractSigned { .. } => {
            Some(SoundCue::DeedSigned)
        }
        GameplayEventKind::ConstructionStage { stage, .. } => Some(
            if matches!(stage, fatherload_simulation::home::BuildStage::Done) {
                SoundCue::HomeFanfare
            } else {
                SoundCue::BuildersHammer
            },
        ),
        GameplayEventKind::HomeCompleted { .. } => Some(SoundCue::HomeFanfare),
        GameplayEventKind::SiteEventRaised { .. } => Some(SoundCue::SiteKnock),
        GameplayEventKind::FurniturePlaced { .. } | GameplayEventKind::FurnitureRemoved { .. } => {
            Some(SoundCue::FurnitureThunk)
        }
        // The shell plays coins/purchase when the service it requested resolves. Hazards are
        // continuous: the director's gas-hiss and boulder-rumble loops follow them directly.
        // Tunnel edits are terrain changes with no sound of their own. Accepting a quest and
        // test-mode toggles/warps are answered by the counter or menu that requested them.
        GameplayEventKind::PurchaseCompleted { .. }
        | GameplayEventKind::EntityDamaged { .. }
        | GameplayEventKind::QuestAccepted { .. }
        | GameplayEventKind::TestModeChanged { .. }
        | GameplayEventKind::TestStock { .. }
        | GameplayEventKind::PeopleInteraction { .. }
        | GameplayEventKind::HomeInteraction { .. }
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
            | GameplayEventKind::EntityDestroyed { .. }
            | GameplayEventKind::BasePlaced { .. }
            | GameplayEventKind::BaseRemoved { .. }
            | GameplayEventKind::BaseServiceUsed { .. }
            | GameplayEventKind::Dawn { .. }
            | GameplayEventKind::Dusk { .. }
            | GameplayEventKind::NewDay { .. }
            | GameplayEventKind::NightSkipped { .. }
            | GameplayEventKind::GotUp { .. }
            | GameplayEventKind::SleepRuleChanged { .. }
            | GameplayEventKind::SkyBandEntered { .. }
            | GameplayEventKind::UpdraftCaught { .. }
            | GameplayEventKind::RigIcing { .. }
            | GameplayEventKind::SkyCacheOpened { .. }
            | GameplayEventKind::BuoyRefuelled { .. }
            | GameplayEventKind::IsleCollapsing { .. }
            | GameplayEventKind::JetStreamCaught { .. }
            | GameplayEventKind::PilotExited { .. }
            | GameplayEventKind::PilotBoarded { .. }
            | GameplayEventKind::PilotJumped { .. }
            | GameplayEventKind::PilotLanded { .. }
            | GameplayEventKind::PilotLedgeGrab { .. }
            | GameplayEventKind::PilotSplashed { .. }
            | GameplayEventKind::PilotHurt { .. }
            | GameplayEventKind::PilotDied { .. }
            | GameplayEventKind::PilotOxygenLow { .. }
            | GameplayEventKind::JetpackIgnited { .. }
            | GameplayEventKind::JetpackEmpty { .. }
            | GameplayEventKind::PickStruck { .. }
            | GameplayEventKind::PickClinked { .. }
            | GameplayEventKind::PackTransferred { .. }
            | GameplayEventKind::PersonTalked { .. }
            | GameplayEventKind::PersonTraded { .. }
            | GameplayEventKind::PersonGifted { .. }
            | GameplayEventKind::AffinityChanged { .. }
            | GameplayEventKind::RumourMarked { .. }
            | GameplayEventKind::PeopleInteraction { .. }
            | GameplayEventKind::ArcOffered { .. }
            | GameplayEventKind::ArcAccepted { .. }
            | GameplayEventKind::ArcStepDone { .. }
            | GameplayEventKind::ArcClimax { .. }
            | GameplayEventKind::ArcCompleted { .. }
            | GameplayEventKind::ArcAbandoned { .. }
            | GameplayEventKind::UpgradeInstalled { .. }
            | GameplayEventKind::MilestoneEarned { .. }
            | GameplayEventKind::LightningArc { .. }
            | GameplayEventKind::ArcChained { .. }
            | GameplayEventKind::GravityThrow { .. }
            | GameplayEventKind::TetherAttached { .. }
            | GameplayEventKind::TetherReleased { .. }
            | GameplayEventKind::FlakBurst { .. }
            | GameplayEventKind::TempleLeverPulled { .. }
            | GameplayEventKind::TempleGateOpened { .. }
            | GameplayEventKind::RelicChestOpened { .. }
            | GameplayEventKind::SkyEnemyAction { .. }
            | GameplayEventKind::MoonVacuumBreach { .. }
            | GameplayEventKind::MoonLanded { .. }
            | GameplayEventKind::ColonyServiceUsed { .. }
            | GameplayEventKind::FacilityPylon { .. }
            | GameplayEventKind::FacilityGateOpened { .. }
            | GameplayEventKind::WardenAwakened { .. }
            | GameplayEventKind::WardenStaggered { .. }
            | GameplayEventKind::WardenDefeated { .. }
            | GameplayEventKind::MoonCoreReached { .. }
            | GameplayEventKind::UplinkBeamed { .. }
            | GameplayEventKind::PilotFired { .. }
            | GameplayEventKind::PilotWeaponDry { .. }
            | GameplayEventKind::PilotWeaponSwapped { .. }
            | GameplayEventKind::PilotRope { .. }
            | GameplayEventKind::PilotGlide { .. }
            | GameplayEventKind::MechanismUsed { .. }
            | GameplayEventKind::ChestOpened { .. }
            | GameplayEventKind::RigRecall { .. }
            | GameplayEventKind::PilotExposed { .. }
            | GameplayEventKind::PilotWallJumped { .. }
            | GameplayEventKind::PlotBought { .. }
            | GameplayEventKind::ContractSigned { .. }
            | GameplayEventKind::ConstructionStage { .. }
            | GameplayEventKind::HomeCompleted { .. }
            | GameplayEventKind::SiteEventRaised { .. }
            | GameplayEventKind::SiteEventAnswered { .. }
            | GameplayEventKind::FurniturePlaced { .. }
            | GameplayEventKind::FurnitureRemoved { .. }
            | GameplayEventKind::HomeRushed { .. }
            | GameplayEventKind::HomeInteraction { .. }
            | GameplayEventKind::NightFell { .. }
            | GameplayEventKind::NightSurvived { .. }
            | GameplayEventKind::RareNightSpawn { .. }
            | GameplayEventKind::NightMonsterBurrowed { .. }
            | GameplayEventKind::TombEntered { .. }
            | GameplayEventKind::TombSeal { .. }
            | GameplayEventKind::TombAmbush { .. }
            | GameplayEventKind::TombGuardianDefeated { .. }
            | GameplayEventKind::TombFloorCleared { .. }
            | GameplayEventKind::TombRelicPicked { .. }
            | GameplayEventKind::TombShrineRested { .. }
            | GameplayEventKind::TombCacheOpened { .. }
            | GameplayEventKind::TombMemberDowned { .. }
            | GameplayEventKind::TombRunEnded { .. }
            | GameplayEventKind::TombTrapSprung { .. }
            | GameplayEventKind::TombGuardianPhase { .. }
            | GameplayEventKind::CryptRoll { .. }
            | GameplayEventKind::CryptTrap { .. }
            | GameplayEventKind::CryptPitFall { .. }
            | GameplayEventKind::CryptLordPhase { .. }
            | GameplayEventKind::CryptTell { .. }
            | GameplayEventKind::CryptSlam { .. }
            | GameplayEventKind::CryptSummon { .. }
            | GameplayEventKind::FluidBreached { .. }
            | GameplayEventKind::FluidReacted { .. }
            | GameplayEventKind::RigSubmerged { .. }
            | GameplayEventKind::RigSurfaced { .. }
            | GameplayEventKind::LiftRideStarted { .. }
            | GameplayEventKind::LiftArrived { .. }
            | GameplayEventKind::LiftRestored { .. }
            | GameplayEventKind::LogisticsPlaced { .. }
            | GameplayEventKind::OutpostReached { .. }
            | GameplayEventKind::OutpostClaimed { .. }
            | GameplayEventKind::OutpostServiced { .. }
            | GameplayEventKind::BeaconTravelled { .. }
            | GameplayEventKind::DiveChargeFired { .. }
            | GameplayEventKind::DescentGateBlocked { .. }
            | GameplayEventKind::CartBoarded { .. }
            | GameplayEventKind::CartSwitched { .. }
            | GameplayEventKind::CartLeft { .. }
            | GameplayEventKind::SupportCreaked { .. }
            | GameplayEventKind::SupportCollapsed { .. }
            | GameplayEventKind::SlateShudder { .. }
            | GameplayEventKind::SlateSlid { .. }
            | GameplayEventKind::StrongboxOpened { .. }
            | GameplayEventKind::SinkholeRumble { .. }
            | GameplayEventKind::SinkholeEscaped { .. }
            | GameplayEventKind::SinkholeCaught { .. }
            | GameplayEventKind::SinkholeReset { .. }
            | GameplayEventKind::DeepClaimGranted { .. }
            | GameplayEventKind::DeepClaimMissing { .. }
            | GameplayEventKind::LavaTubeEntered { .. }
            | GameplayEventKind::CrustCreature { .. }
            | GameplayEventKind::ResonanceChain { .. }
            | GameplayEventKind::FossilBoneFreed { .. }
            | GameplayEventKind::SkeletonCompleted { .. }
            | GameplayEventKind::GasTideTurned { .. }
            | GameplayEventKind::AmberStuck { .. }
            | GameplayEventKind::CathedralValveOpened { .. }
            | GameplayEventKind::CathedralLiftRestored { .. }
            | GameplayEventKind::DoseWarning { .. }
            | GameplayEventKind::MutationGained { .. }
            | GameplayEventKind::MachineryWoke { .. }
            | GameplayEventKind::PistonStruck { .. }
            | GameplayEventKind::HeatStormChanged { .. }
            | GameplayEventKind::LavaPoured { .. }
            | GameplayEventKind::DeepLogRead { .. }
            | GameplayEventKind::StarCoreAwoke
            | GameplayEventKind::StarCorePhase { .. }
            | GameplayEventKind::StarCoreBeaten { .. }
            | GameplayEventKind::GlowmotherAwoke { .. }
            | GameplayEventKind::GlowmotherDefeated { .. }
            | GameplayEventKind::LivingDark { .. }
            | GameplayEventKind::ExpeditionLogFound { .. }
            | GameplayEventKind::ExpeditionLogSealed { .. }
            | GameplayEventKind::ExpeditionEcho { .. }
            | GameplayEventKind::AnomalyEntered { .. }
            | GameplayEventKind::Drowned { .. }
            | GameplayEventKind::Hollow { .. }
            | GameplayEventKind::Distortion { .. }
            | GameplayEventKind::DistortionLandmarkFound { .. }
            | GameplayEventKind::FarStrange { .. }
            | GameplayEventKind::Supply { .. }
            | GameplayEventKind::AncientCity { .. }
            | GameplayEventKind::Garrison { .. }
            | GameplayEventKind::Heart { .. }
            | GameplayEventKind::HeartLandmarkFound { .. }
            | GameplayEventKind::Story { .. }
            | GameplayEventKind::PlaceChanged { .. }
            | GameplayEventKind::Planet { .. }
            | GameplayEventKind::Flight { .. }
            | GameplayEventKind::SpaceSystems { .. }
            | GameplayEventKind::BodyLife { .. }
            | GameplayEventKind::Wilds { .. }
            | GameplayEventKind::Outlands { .. }
            | GameplayEventKind::Lair { .. }
            | GameplayEventKind::Frontier { .. }
            | GameplayEventKind::Exotic { .. }
            | GameplayEventKind::Alien { .. }
            | GameplayEventKind::Station { .. }
            | GameplayEventKind::Hostile { .. }
            | GameplayEventKind::Endgame { .. }
            | GameplayEventKind::Galaxy { .. }
            | GameplayEventKind::TestStock { .. } => {}
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
                furniture: None,
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
        events.extend([
            GameplayEventKind::LightningArc {
                x: 0,
                y: 0,
                end_x: 9,
                end_y: 9,
                struck: Some(player_id),
            },
            GameplayEventKind::ArcChained {
                player_id,
                x: 0,
                y: 0,
                end_x: 5,
                end_y: 0,
            },
            GameplayEventKind::GravityThrow {
                player_id,
                x: 0,
                y: 0,
                count: 2,
            },
            GameplayEventKind::TetherAttached {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::TetherReleased { player_id },
            GameplayEventKind::FlakBurst {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::TempleLeverPulled {
                player_id,
                x: 0,
                y: 0,
                opened: 2,
            },
            GameplayEventKind::TempleGateOpened {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::RelicChestOpened {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::NightFell {
                night: 1,
                moon: fatherload_simulation::clock::MoonPhase::Full,
            },
            GameplayEventKind::NightSurvived { night: 1, kills: 2 },
            GameplayEventKind::RareNightSpawn {
                kind: EnemyKind::UmbralAlpha,
                x: 0,
                y: 0,
            },
            GameplayEventKind::NightMonsterBurrowed {
                entity: 1,
                kind: EnemyKind::ShadowHound,
                x: 0,
                y: 0,
            },
            GameplayEventKind::TombEntered {
                player_id,
                style: fatherload_simulation::tomb::SiteStyle::TopDown,
                tier: 2,
                party: 1,
            },
            GameplayEventKind::TombSeal {
                player_id: Some(player_id),
                x: 0,
                y: 0,
                opening: fatherload_simulation::tomb::SiteOpening::Key,
                opened: true,
            },
            GameplayEventKind::TombAmbush {
                x: 0,
                y: 0,
                boss: true,
            },
            GameplayEventKind::TombGuardianDefeated {
                by: Some(player_id),
            },
            GameplayEventKind::TombFloorCleared {
                player_id,
                floor: 0,
            },
            GameplayEventKind::TombRelicPicked {
                player_id,
                relic: Some(fatherload_simulation::tomb::Relic::SpringHeels),
            },
            GameplayEventKind::TombShrineRested {
                player_id,
                banked: 10,
            },
            GameplayEventKind::TombCacheOpened {
                player_id,
                value: 10,
                key: false,
                lore: None,
            },
            GameplayEventKind::TombMemberDowned {
                player_id,
                revived: false,
            },
            GameplayEventKind::TombRunEnded {
                player_id,
                outcome: fatherload_simulation::tomb::RunOutcome::Finished,
                banked: 10,
                lost: 0,
            },
            GameplayEventKind::TombTrapSprung {
                kind: fatherload_simulation::tomb::realize::TrapKind::Crusher,
                x: 0,
                y: 0,
            },
            GameplayEventKind::TombGuardianPhase {
                kind: EnemyKind::SunColossus,
                phase: 2,
                x: 0,
                y: 0,
            },
            GameplayEventKind::CryptRoll {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::CryptTrap {
                kind: CryptTrapKind::Spikes,
                x: 0,
                y: 0,
                fired: true,
            },
            GameplayEventKind::CryptPitFall {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::CryptLordPhase {
                phase: 2,
                x: 0,
                y: 0,
            },
            GameplayEventKind::CryptTell {
                kind: EnemyKind::CryptLord,
                attack: CryptAttack::Ring,
                x: 0,
                y: 0,
                aim_x: 0,
                aim_y: 0,
            },
            GameplayEventKind::CryptSlam {
                x: 0,
                y: 0,
                radius: 70,
            },
            GameplayEventKind::CryptSummon {
                x: 0,
                y: 0,
                count: 3,
            },
            GameplayEventKind::FluidBreached {
                kind: fatherload_simulation::fluid::FluidKind::Water,
                x: 0,
                y: 0,
            },
            GameplayEventKind::FluidReacted {
                reaction: FluidReaction::Quenched,
                x: 0,
                y: 0,
            },
            GameplayEventKind::FluidReacted {
                reaction: FluidReaction::Dissolved,
                x: 0,
                y: 0,
            },
            GameplayEventKind::RigSubmerged {
                player_id,
                kind: fatherload_simulation::fluid::FluidKind::Water,
            },
            GameplayEventKind::RigSurfaced {
                player_id,
                kind: fatherload_simulation::fluid::FluidKind::Brine,
            },
            GameplayEventKind::LiftRideStarted {
                player_id,
                shaft: 0,
                to_row: 10,
            },
            GameplayEventKind::LiftArrived {
                player_id,
                shaft: 0,
            },
            GameplayEventKind::LiftRestored {
                player_id,
                shaft: 0,
            },
            GameplayEventKind::LogisticsPlaced {
                player_id,
                x: 0,
                y: 0,
                beacon: true,
            },
            GameplayEventKind::OutpostReached {
                player_id,
                outpost: 0,
            },
            GameplayEventKind::OutpostClaimed {
                player_id,
                outpost: 0,
            },
            GameplayEventKind::BeaconTravelled {
                player_id,
                from_x: 0,
                from_y: 0,
                to_x: 0,
                to_y: 0,
            },
            GameplayEventKind::DiveChargeFired {
                player_id,
                x: 0,
                y: 0,
                rows: 20,
            },
            GameplayEventKind::DescentGateBlocked {
                player_id,
                descent: 2,
                system: fatherload_simulation::depth_progression::RigSystem::PressureHull,
            },
            GameplayEventKind::CartBoarded { player_id },
            GameplayEventKind::CartSwitched { player_id },
            GameplayEventKind::CartLeft {
                player_id,
                speed: 6,
            },
            GameplayEventKind::SupportCreaked { x: 0, y: 0 },
            GameplayEventKind::SupportCollapsed { x: 0, y: 0 },
            GameplayEventKind::SlateShudder { x: 0, y: 0 },
            GameplayEventKind::SlateSlid { x: 0, y: 0 },
            GameplayEventKind::StrongboxOpened {
                player_id,
                log: 0,
                first: true,
            },
            GameplayEventKind::SinkholeRumble { player_id },
            GameplayEventKind::SinkholeEscaped { player_id },
            GameplayEventKind::SinkholeCaught { player_id },
            GameplayEventKind::SinkholeReset { player_id },
            GameplayEventKind::DeepClaimGranted { player_id },
            GameplayEventKind::DeepClaimMissing { player_id },
            GameplayEventKind::LavaTubeEntered { player_id },
            GameplayEventKind::CrustCreature {
                action: fatherload_simulation::crust::CrustCreatureAction::BeetleChewed,
                x: 0,
                y: 0,
            },
            GameplayEventKind::ExpeditionLogFound { player_id, log: 1 },
            GameplayEventKind::Story {
                what: fatherload_simulation::expedition::progress::StoryEvent::DescentReached(
                    fatherload_simulation::descent::Descent::Ancients,
                ),
                player_id: Some(player_id),
            },
            GameplayEventKind::Story {
                what: fatherload_simulation::expedition::progress::StoryEvent::CableLit,
                player_id: None,
            },
            GameplayEventKind::ExpeditionLogSealed { player_id, log: 3 },
            GameplayEventKind::ExpeditionEcho { player_id, echo: 1 },
            GameplayEventKind::ExpeditionEcho { player_id, echo: 7 },
            GameplayEventKind::PlaceChanged {
                player_id,
                from: fatherload_simulation::place::Place::HOME,
                to: fatherload_simulation::place::Place::Space,
            },
        ]);
        for what in [
            fatherload_simulation::planet::landing::PlanetEvent::Landed,
            fatherload_simulation::planet::landing::PlanetEvent::Slid,
            fatherload_simulation::planet::landing::PlanetEvent::HullScraped,
            fatherload_simulation::planet::landing::PlanetEvent::Tipped,
            fatherload_simulation::planet::landing::PlanetEvent::Righted,
        ] {
            events.push(GameplayEventKind::Planet {
                what,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::flight::FlightEvent::ALL {
            events.push(GameplayEventKind::Flight {
                what,
                player_id,
                detail: 0,
            });
        }
        for what in fatherload_simulation::flight::systems::SystemsEvent::ALL {
            events.push(GameplayEventKind::SpaceSystems {
                what,
                player_id,
                detail: 0,
            });
        }
        for what in fatherload_simulation::planet::wilds::WildsEvent::ALL {
            events.push(GameplayEventKind::Wilds {
                what,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::planet::outlands::OutlandsEvent::ALL {
            events.push(GameplayEventKind::Outlands {
                what,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::planet::lairs::LairEvent::ALL {
            events.push(GameplayEventKind::Lair {
                what,
                boss: fatherload_simulation::EnemyKind::BrambleKing,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::frontier::FrontierEvent::ALL {
            events.push(GameplayEventKind::Frontier {
                what,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::exotic::ExoticEvent::ALL {
            events.push(GameplayEventKind::Exotic {
                what,
                player_id: Some(player_id),
                x: 0,
                y: 0,
            });
        }
        for what in fatherload_simulation::hostiles::HostileEvent::ALL {
            events.push(GameplayEventKind::Hostile {
                what,
                player_id: Some(player_id),
                detail: 0,
            });
        }
        for what in fatherload_simulation::endgame::EndgameEvent::ALL {
            events.push(GameplayEventKind::Endgame {
                what,
                player_id: Some(player_id),
                detail: 0,
            });
        }
        for what in fatherload_simulation::galaxy_story::GalaxyEvent::ALL {
            events.push(GameplayEventKind::Galaxy {
                what,
                player_id: Some(player_id),
                detail: 0,
            });
        }
        for what in fatherload_simulation::test_mode::TestStockEvent::ALL {
            events.push(GameplayEventKind::TestStock {
                what,
                player_id: Some(player_id),
                detail: 0,
            });
        }
        for kind in fatherload_simulation::expedition::anomaly::AnomalyKind::ALL {
            events.push(GameplayEventKind::AnomalyEntered {
                player_id,
                kind,
                first_of_kind: true,
            });
        }
        for service in fatherload_simulation::logistics::OutpostService::ALL {
            events.push(GameplayEventKind::OutpostServiced {
                player_id,
                outpost: 0,
                service,
            });
        }
        for action in [
            fatherload_simulation::enemy::SkyEnemyAction::Swoop,
            fatherload_simulation::enemy::SkyEnemyAction::BoardersDropped,
            fatherload_simulation::enemy::SkyEnemyAction::AirshipDowned,
            fatherload_simulation::enemy::SkyEnemyAction::NesterEnraged,
        ] {
            events.push(GameplayEventKind::SkyEnemyAction {
                entity: 1,
                kind: EnemyKind::SkyRay,
                action,
                x: 0,
                y: 0,
            });
        }
        for shot in [
            ShotKind::Acid,
            ShotKind::Fireball,
            ShotKind::Bolt,
            ShotKind::Cannonball,
            ShotKind::Wispfire,
        ] {
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
        for item in Buildable::PALETTE {
            events.push(GameplayEventKind::BasePlaced {
                player_id,
                item,
                x: 0,
                y: 0,
            });
        }
        events.push(GameplayEventKind::BaseRemoved {
            player_id,
            x: 0,
            y: 0,
        });
        for service in [
            ModuleService::Refuel,
            ModuleService::OrderSupply,
            ModuleService::Repair,
            ModuleService::DepositCargo,
            ModuleService::DepositBlocks,
            ModuleService::WithdrawAll,
            ModuleService::SellCargo,
            ModuleService::SetHome,
            ModuleService::CycleAccess,
            ModuleService::Sleep,
        ] {
            events.push(GameplayEventKind::BaseServiceUsed {
                player_id,
                structure: fatherload_simulation::base::StructureId(0),
                service,
            });
        }
        events.extend([
            GameplayEventKind::Dawn { day: 2 },
            GameplayEventKind::Dusk { day: 2 },
            GameplayEventKind::NewDay { day: 3 },
            GameplayEventKind::NightSkipped { day: 3 },
            GameplayEventKind::GotUp { player_id },
            GameplayEventKind::SleepRuleChanged {
                player_id,
                rule: fatherload_simulation::clock::SleepRule::Anyone,
            },
        ]);
        for band in fatherload_simulation::sky::SkyBand::ALL {
            events.push(GameplayEventKind::SkyBandEntered { player_id, band });
        }
        events.push(GameplayEventKind::UpdraftCaught { player_id });
        events.push(GameplayEventKind::RigIcing { player_id });
        events.push(GameplayEventKind::SkyCacheOpened {
            player_id,
            tile: fatherload_simulation::sky::SkyTile::Cache,
            credits: 120,
        });
        events.push(GameplayEventKind::BuoyRefuelled { player_id });
        events.push(GameplayEventKind::IsleCollapsing { x: 3, y: -300 });
        events.push(GameplayEventKind::JetStreamCaught { player_id });
        events.extend([
            GameplayEventKind::PilotExited {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::PilotBoarded {
                player_id,
                transferred: 2,
            },
            GameplayEventKind::PilotJumped { player_id },
            GameplayEventKind::PilotLanded {
                player_id,
                hard: false,
            },
            GameplayEventKind::PilotLanded {
                player_id,
                hard: true,
            },
            GameplayEventKind::PilotLedgeGrab { player_id },
            GameplayEventKind::PilotSplashed { player_id },
            GameplayEventKind::PilotHurt {
                player_id,
                amount: 5,
                x: 0,
                y: 0,
            },
            GameplayEventKind::PilotDied {
                player_id,
                x: 0,
                y: 0,
            },
            GameplayEventKind::PilotOxygenLow {
                player_id,
                empty: false,
            },
            GameplayEventKind::JetpackIgnited { player_id },
            GameplayEventKind::JetpackEmpty { player_id },
            GameplayEventKind::PickStruck {
                player_id,
                x: 0,
                y: 0,
                broke: false,
            },
            GameplayEventKind::PickStruck {
                player_id,
                x: 0,
                y: 0,
                broke: true,
            },
            GameplayEventKind::PickClinked { player_id },
            GameplayEventKind::PackTransferred {
                player_id,
                count: 1,
                to_rig: true,
            },
        ]);
        events.push(GameplayEventKind::PersonTalked {
            player_id,
            npc: 1,
            first_meeting: true,
        });
        events.push(GameplayEventKind::PersonTraded {
            player_id,
            npc: 1,
            kind: fatherload_simulation::people::OfferKind::Rumours,
        });
        events.push(GameplayEventKind::PersonGifted {
            player_id,
            npc: 1,
            reaction: fatherload_simulation::people::offers::GiftReaction::Loved,
        });
        for warmer in [true, false] {
            events.push(GameplayEventKind::AffinityChanged {
                player_id,
                npc: 1,
                standing: fatherload_simulation::people::Standing::Friendly,
                warmer,
            });
        }
        events.push(GameplayEventKind::RumourMarked {
            player_id,
            npc: 1,
            feature,
        });
        events.push(GameplayEventKind::PeopleInteraction { player_id });
        let arc = fatherload_simulation::people::ArcId(1);
        events.extend([
            GameplayEventKind::ArcOffered { player_id, arc },
            GameplayEventKind::ArcAccepted { player_id, arc },
            GameplayEventKind::ArcStepDone {
                player_id,
                arc,
                step: 0,
            },
            GameplayEventKind::ArcClimax { player_id, arc },
            GameplayEventKind::ArcCompleted {
                player_id,
                arc,
                outcome: 0,
            },
            GameplayEventKind::ArcAbandoned { player_id, arc },
        ]);
        events.extend(
            fatherload_simulation::economy::UpgradeKind::ALL.map(|upgrade| {
                GameplayEventKind::UpgradeInstalled {
                    player_id,
                    upgrade,
                    level: 2,
                }
            }),
        );
        events.extend(
            fatherload_simulation::milestone::Milestone::ALL.map(|milestone| {
                GameplayEventKind::MilestoneEarned {
                    player_id,
                    milestone,
                }
            }),
        );
        // WP-PILOT.
        for weapon in fatherload_simulation::pilot::arsenal::PILOT_WEAPONS {
            events.push(GameplayEventKind::PilotFired {
                player_id,
                weapon,
                x: 0,
                y: 0,
                end_x: 1,
                end_y: 0,
            });
            events.push(GameplayEventKind::PilotWeaponDry { player_id, weapon });
        }
        events.push(GameplayEventKind::PilotWeaponSwapped {
            player_id,
            weapon: None,
        });
        for action in [
            fatherload_simulation::pilot::RopeEvent::Hooked,
            fatherload_simulation::pilot::RopeEvent::Missed,
            fatherload_simulation::pilot::RopeEvent::Winched,
            fatherload_simulation::pilot::RopeEvent::Released,
        ] {
            events.push(GameplayEventKind::PilotRope {
                player_id,
                action,
                x: 0,
                y: 0,
            });
        }
        for hover in [true, false] {
            events.push(GameplayEventKind::PilotGlide { player_id, hover });
            events.push(GameplayEventKind::PilotExposed {
                player_id,
                heat: hover,
            });
        }
        {
            use fatherload_simulation::mechanism::MechanismEvent;
            for mechanism in [
                MechanismEvent::LeverPulled { on: true },
                MechanismEvent::PlatePressed,
                MechanismEvent::PlateReleased,
                MechanismEvent::GateOpened,
                MechanismEvent::GateClosed,
                MechanismEvent::KeycardAccepted { color: 1 },
                MechanismEvent::Locked { color: 1 },
            ] {
                events.push(GameplayEventKind::MechanismUsed {
                    player_id: Some(player_id),
                    x: 0,
                    y: 0,
                    mechanism,
                });
            }
        }
        events.push(GameplayEventKind::ChestOpened {
            player_id,
            x: 0,
            y: 0,
        });
        for outcome in [
            fatherload_simulation::pilot::RecallOutcome::Started,
            fatherload_simulation::pilot::RecallOutcome::Arrived,
            fatherload_simulation::pilot::RecallOutcome::NoPath,
        ] {
            events.push(GameplayEventKind::RigRecall { player_id, outcome });
        }
        events.push(GameplayEventKind::PilotWallJumped { player_id });
        events.extend(PoiKind::ALL.map(|poi| GameplayEventKind::PoiOpened {
            player_id,
            poi,
            reward: PoiReward::default(),
        }));
        let plot = fatherload_simulation::home::PlotId(0);
        events.push(GameplayEventKind::PlotBought {
            player_id,
            plot,
            size: fatherload_simulation::home::PlotSize::Small,
            setting: fatherload_simulation::home::PlotSetting::Town,
        });
        events.push(GameplayEventKind::ContractSigned {
            player_id,
            plot,
            job: fatherload_simulation::home::HomeJob::Build(
                fatherload_simulation::home::Blueprint::Cabin,
            ),
        });
        events.extend(fatherload_simulation::home::BuildStage::ALL.map(|stage| {
            GameplayEventKind::ConstructionStage {
                plot,
                owner: player_id,
                stage,
            }
        }));
        events.push(GameplayEventKind::HomeCompleted {
            plot,
            owner: player_id,
            job: fatherload_simulation::home::HomeJob::Build(
                fatherload_simulation::home::Blueprint::Cabin,
            ),
        });
        events.push(GameplayEventKind::SiteEventRaised {
            plot,
            owner: player_id,
            event: fatherload_simulation::home::SiteEventKind::Anecdote { index: 0 },
        });
        events.push(GameplayEventKind::SiteEventAnswered { player_id, plot });
        events.push(GameplayEventKind::FurniturePlaced {
            player_id,
            plot,
            furniture: fatherload_simulation::home::Furniture::Bed,
        });
        events.push(GameplayEventKind::FurnitureRemoved {
            player_id,
            plot,
            furniture: fatherload_simulation::home::Furniture::Bed,
        });
        events.push(GameplayEventKind::HomeRushed { player_id, plot });
        events.push(GameplayEventKind::HomeInteraction { player_id });
        events
    }

    #[test]
    fn every_event_is_either_heard_or_deliberately_silent() {
        let silent: Vec<GameplayEventKind> = every_event()
            .into_iter()
            .filter(|event| event_cue(event).is_none())
            // The god-mode toolkit's tools are answered by its menu (Wave 14): all silent.
            .filter(|event| !matches!(event, GameplayEventKind::TestStock { .. }))
            .collect();
        assert!(
            every_event()
                .iter()
                .filter(|event| matches!(event, GameplayEventKind::TestStock { .. }))
                .all(|event| event_cue(event).is_none())
        );
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
                // A clean touchdown on a round body is the landing's own thud (Wave 11).
                GameplayEventKind::Planet {
                    what: fatherload_simulation::planet::landing::PlanetEvent::Landed,
                    player_id: Some(PlayerId::new(1)),
                    x: 0,
                    y: 0,
                },
                // An outland hazard's bite is the hull hit's (Wave 13 planets-c).
                GameplayEventKind::Outlands {
                    what: fatherload_simulation::planet::outlands::OutlandsEvent::HazardBite,
                    player_id: Some(PlayerId::new(1)),
                    x: 0,
                    y: 0,
                },
                // A frontier storm passing is the wind bed fading; a hazard's bite is the hull
                // hit's (Wave 12).
                GameplayEventKind::Frontier {
                    what: fatherload_simulation::frontier::FrontierEvent::StormEnded,
                    player_id: Some(PlayerId::new(1)),
                    x: 0,
                    y: 0,
                },
                GameplayEventKind::Frontier {
                    what: fatherload_simulation::frontier::FrontierEvent::HazardBite,
                    player_id: Some(PlayerId::new(1)),
                    x: 0,
                    y: 0,
                },
                // An exotic world's hazard bite is the hull's own hit (Wave 13).
                GameplayEventKind::Exotic {
                    what: fatherload_simulation::exotic::ExoticEvent::HazardBite,
                    player_id: Some(PlayerId::new(1)),
                    x: 0,
                    y: 0,
                },
                // A hit on the rig in space is `PlayerDamaged`'s hull hit (Wave 12).
                GameplayEventKind::Hostile {
                    what: fatherload_simulation::hostiles::HostileEvent::RigHit,
                    player_id: Some(PlayerId::new(1)),
                    detail: 0,
                },
                GameplayEventKind::PeopleInteraction {
                    player_id: PlayerId::new(1)
                },
                GameplayEventKind::HomeInteraction {
                    player_id: PlayerId::new(1)
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
    #[allow(clippy::too_many_lines, reason = "the list names every cue")]
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
            SoundCue::BuildPlace,
            SoundCue::ModuleBuilt,
            SoundCue::BuildRemove,
            SoundCue::ClaimStaked,
            SoundCue::FuelPump,
            SoundCue::SupplyDrop,
            SoundCue::RepairWeld,
            SoundCue::StorageStow,
            SoundCue::UplinkBeam,
            SoundCue::HomeSet,
            SoundCue::RefineryLoad,
            SoundCue::WorkshopCraft,
            SoundCue::CrewReturn,
            SoundCue::Dawn,
            SoundCue::Dusk,
            SoundCue::Midnight,
            SoundCue::BedDown,
            SoundCue::NightSkipped,
            SoundCue::HatchOpen,
            SoundCue::HatchClose,
            SoundCue::PilotJump,
            SoundCue::PilotLand,
            SoundCue::PilotLandHard,
            SoundCue::PilotHurt,
            SoundCue::PilotDown,
            SoundCue::OxygenAlarm,
            SoundCue::JetpackBurst,
            SoundCue::JetpackSputter,
            SoundCue::PickChip,
            SoundCue::PickBreak,
            SoundCue::PickClink,
            SoundCue::Splash,
            SoundCue::LedgeGrab,
            SoundCue::PersonGreet,
            SoundCue::GiftGiven,
            SoundCue::AffinityUp,
            SoundCue::RumourHeard,
            SoundCue::UpgradeFanfare,
            SoundCue::MilestoneFanfare,
            SoundCue::ActTwoFanfare,
            SoundCue::ArcZap,
            SoundCue::GravityHum,
            SoundCue::GravityThrow,
            SoundCue::TetherShot,
            SoundCue::TetherHook,
            SoundCue::FlakShot,
            SoundCue::FlakBurst,
            SoundCue::Thunderclap,
            SoundCue::LeverClunk,
            SoundCue::GateRumble,
            SoundCue::SwoopScreech,
            SoundCue::PirateHorn,
            SoundCue::HarpyScreech,
            SoundCue::CannonBoom,
            SoundCue::MoonLanding,
            SoundCue::VacuumAlarm,
            SoundCue::PylonChime,
            SoundCue::GateOpen,
            SoundCue::WardenWake,
            SoundCue::CoreReached,
            SoundCue::UplinkWarp,
            SoundCue::PistolPop,
            SoundCue::RifleCrack,
            SoundCue::ShotgunBoom,
            SoundCue::WrenchSwing,
            SoundCue::GrappleFire,
            SoundCue::GliderOpen,
            SoundCue::GateGrind,
            SoundCue::KeycardBeep,
            SoundCue::DeedSigned,
            SoundCue::BuildersHammer,
            SoundCue::HomeFanfare,
            SoundCue::SiteKnock,
            SoundCue::FurnitureThunk,
            SoundCue::HoundHowl,
            SoundCue::AlphaHowl,
            SoundCue::StalkerBurst,
            SoundCue::WispChime,
            SoundCue::MothFlutter,
            SoundCue::NightFalls,
            SoundCue::DawnRelief,
            SoundCue::Burrow,
            SoundCue::StoryHook,
            SoundCue::StoryQuill,
            SoundCue::StoryClimax,
            SoundCue::StoryResolved,
            SoundCue::BladeSwish,
            SoundCue::CrusherSlam,
            SoundCue::DartFire,
            SoundCue::FloorCrumble,
            SoundCue::SandPour,
            SoundCue::FogBurst,
            SoundCue::MummyGroan,
            SoundCue::BoneRattle,
            SoundCue::StatueGrind,
            SoundCue::ColossusSlam,
            SoundCue::MatriarchWail,
            SoundCue::DodgeRoll,
            SoundCue::ArrowTwang,
            SoundCue::SpikeShing,
            SoundCue::PitDrop,
            SoundCue::LordRoar,
            SoundCue::StoneSlam,
            SoundCue::HexBolt,
            SoundCue::FluidRush,
            SoundCue::SteamHiss,
            SoundCue::AcidFizz,
            SoundCue::RigSplash,
            SoundCue::CartClunk,
            SoundCue::CartSwitch,
            SoundCue::TimberCreak,
            SoundCue::RoofCollapse,
            SoundCue::SinkholeRumble,
            SoundCue::DeepClaimFanfare,
            SoundCue::TubeBreath,
            SoundCue::ResonanceChime,
            SoundCue::ValveTurn,
            SoundCue::PistonSlam,
            SoundCue::GeigerAlert,
            SoundCue::StormSiren,
            SoundCue::CoreStrike,
            SoundCue::CoreWake,
            SoundCue::DeepLog,
            SoundCue::BoneCrumble,
            SoundCue::RadioEcho,
            SoundCue::GhostVoice,
            SoundCue::LogCanister,
            SoundCue::AnomalyHum,
            SoundCue::SonarPing,
            SoundCue::GeyserBlast,
            SoundCue::IceCrack,
            SoundCue::HullGroan,
            SoundCue::ClamSnap,
            SoundCue::LeviathanRoar,
            SoundCue::MerrowChime,
            SoundCue::SunSting,
            SoundCue::WindGust,
            SoundCue::BeastBellow,
            SoundCue::RocCry,
            SoundCue::SerpentRoar,
            SoundCue::FlareSwell,
            SoundCue::AnchorThunk,
            SoundCue::GlassChime,
            SoundCue::MirrorShatter,
            SoundCue::GravityGroan,
            SoundCue::GlitchBlip,
            SoundCue::DataChirp,
            SoundCue::EchoPing,
            SoundCue::TimeBubbleBurst,
            SoundCue::RewindWarp,
            SoundCue::Shriek,
            SoundCue::QuartzRing,
            SoundCue::KeyedGlass,
            SoundCue::GlitchRewrite,
            SoundCue::GlitchBlink,
            SoundCue::CityTramBell,
            SoundCue::CityLiftChime,
            SoundCue::CityPistonHiss,
            SoundCue::CityPistonThump,
            SoundCue::CityCrusherSlam,
            SoundCue::CityArchivistChirp,
            SoundCue::CityArchivistScan,
            SoundCue::CityChartChord,
            SoundCue::CityVaultGive,
            SoundCue::CityAlarm,
            SoundCue::CityCargoSnatch,
            SoundCue::CitySparkZap,
            SoundCue::GarrisonTurret,
            SoundCue::GarrisonTrap,
            SoundCue::GarrisonKlaxon,
            SoundCue::GarrisonBay,
            SoundCue::GarrisonBreak,
            SoundCue::ColossusFistSlam,
            SoundCue::ColossusCollapse,
            SoundCue::SealSocket,
            SoundCue::SealDoorGrind,
            SoundCue::SealTremor,
            SoundCue::HeartCableLight,
            SoundCue::HeartInhale,
            SoundCue::HeartPull,
            SoundCue::HeartIgnite,
            SoundCue::HeartExhale,
            SoundCue::HeartCoilBreak,
            SoundCue::HeartDarkMatter,
            SoundCue::HeartRailZap,
            SoundCue::HeartWellGroan,
            SoundCue::HeartMetalSplash,
            SoundCue::StoryDescentCall,
            SoundCue::StoryCableLit,
            SoundCue::SystemsFlareWarning,
            SoundCue::SystemsFlareBurst,
            SoundCue::SystemsHoleWarning,
            SoundCue::SystemsTransit,
            SoundCue::SystemsOreCrunch,
            SoundCue::FrontierSandSlide,
            SoundCue::FrontierFloorCrumble,
            SoundCue::FrontierGlassShatter,
            SoundCue::FrontierCrystalRing,
            SoundCue::FrontierSplinter,
            SoundCue::FrontierBeamLit,
            SoundCue::FrontierVaultOpen,
            SoundCue::FrontierRumble,
            SoundCue::FrontierEruption,
            SoundCue::FrontierLavaBomb,
            SoundCue::FrontierStormRising,
            SoundCue::FrontierStormBreak,
            SoundCue::FrontierCrystalGrow,
            SoundCue::FrontierChime,
            SoundCue::FrontierWyrmRipple,
            SoundCue::AlienVessiVoice,
            SoundCue::AlienTokkariVoice,
            SoundCue::AlienSiphraVoice,
            SoundCue::AlienFirstContact,
            SoundCue::AlienWordLearned,
            SoundCue::SpaceCannon,
            SoundCue::SpaceMissile,
            SoundCue::SpaceShipHit,
            SoundCue::SpaceShipBoom,
            SoundCue::SpaceShieldDown,
            SoundCue::SpaceHail,
            SoundCue::SpaceBossWarn,
            SoundCue::SpaceBossAppear,
            SoundCue::SpaceEnemyShot,
            SoundCue::OutlandCapBounce,
            SoundCue::OutlandMyceliumCreep,
            SoundCue::OutlandGlowflash,
            SoundCue::OutlandVentSwell,
            SoundCue::OutlandVentBlast,
            SoundCue::OutlandGearClank,
            SoundCue::OutlandRelayFlip,
            SoundCue::OutlandTombOpen,
            SoundCue::OutlandWispChill,
            SoundCue::OutlandBossWake,
            SoundCue::OutlandBroodCall,
            SoundCue::OutlandBossFall,
            SoundCue::ExoticCrackle,
            SoundCue::ExoticBolt,
            SoundCue::ExoticPressure,
            SoundCue::ExoticImpact,
            SoundCue::ExoticHiss,
            SoundCue::ExoticGeyser,
            SoundCue::ExoticQuake,
            SoundCue::ExoticWhistle,
            SoundCue::ExoticVent,
            SoundCue::ExoticZap,
            SoundCue::ExoticPowerDown,
            SoundCue::ExoticStormRising,
            SoundCue::ExoticBossAwake,
            SoundCue::ExoticBossTell,
            SoundCue::ExoticBossDown,
            SoundCue::ExoticDock,
            SoundCue::SpaceWarpEngage,
            SoundCue::SpaceWarpArrive,
            SoundCue::SpaceCruiseEngage,
            SoundCue::SpaceDocked,
            SoundCue::LairBossWake,
            SoundCue::LairBossFall,
        ];

        let mut patches: Vec<Patch> = cues.iter().map(|cue| cue_patch(*cue)).collect();
        assert!(patches.iter().all(|patch| !patch.looped()));
        patches.sort();
        patches.dedup();
        assert_eq!(patches.len(), cues.len());
    }
}
