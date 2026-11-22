//! The designed sound patches: every sound the game makes, synthesized from [`super::dsp`]
//! building blocks. Each patch renders deterministically from a seed (so each of its variations
//! is reproducible), then [`render_patch`] finishes it: DC removal, click-free edges, a seamless
//! seam for loops and loudness normalization to its category's target.
//!
//! The design of each patch is summarized in `packages/game/src/audio/README.md`.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    reason = "patch recipes are long lists of layered formulas kept close to how they sound"
)]

use std::f32::consts::TAU;

mod old_deep;

use super::{
    analysis,
    dsp::{
        Adsr, Decimator, Delay, FilterMode, Fm, Noise, NoiseColor, Osc, Rng, Svf, Wave, db_to_gain,
        decay, equal_power, fold, glide, mix_into, perc, render, samples_for, saturate, segments,
    },
};

/// Loudness class of a patch: every patch in a class is normalized to the same level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    /// Menu ticks and blips.
    Interface,
    /// Rewards and confirmations (coins, pickups, crates).
    Feedback,
    /// Physical hits (landings, bumps, damage, drill transients).
    Impact,
    /// Warning beeps.
    Alarm,
    /// Continuous sounds the director drives (thrust, drilling, hazards).
    Loop,
    /// Set-pieces (explosions, fanfares, stings).
    Big,
}

impl Category {
    /// Target loudness: the loudest 100 ms window's RMS (after a 100 Hz high-pass that stands in
    /// for perceptual weighting), in dBFS.
    #[must_use]
    pub const fn target_db(self) -> f32 {
        match self {
            Self::Interface => -24.0,
            Self::Feedback => -20.0,
            Self::Alarm => -21.0,
            Self::Impact => -17.0,
            Self::Loop => -22.0,
            Self::Big => -15.0,
        }
    }

    #[must_use]
    #[cfg(any(test, not(target_os = "emscripten")))]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Interface => "interface",
            Self::Feedback => "feedback",
            Self::Alarm => "alarm",
            Self::Impact => "impact",
            Self::Loop => "loop",
            Self::Big => "big",
        }
    }
}

/// Highest sample peak a finished patch may reach (-1 dBFS).
pub const PEAK_CEILING: f32 = 0.891;
/// Length of every loop patch (seconds). Long enough that repeating noise textures do not
/// become recognisable during minutes of drilling or flying (a 2 s frozen-noise loop does).
pub const LOOP_SECONDS: f32 = 4.0;
/// Crossfade folded over a loop's seam (seconds).
const LOOP_CROSSFADE_SECONDS: f32 = 0.3;

macro_rules! patches {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// Every designed sound.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum Patch {
            $($variant),+
        }

        impl Patch {
            /// Every patch, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// File-friendly name (`--render-audio` writes `<name>_<variation>.wav`).
            #[must_use]
#[cfg(any(test, not(target_os = "emscripten")))]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }
    };
}

patches! {
    UiMove => "ui_move",
    UiConfirm => "ui_confirm",
    UiBack => "ui_back",
    UiError => "ui_error",
    Toast => "toast",
    Pickup => "pickup",
    Coins => "coins",
    Purchase => "purchase",
    CrateOpen => "crate_open",
    SalvageOpen => "salvage_open",
    BombArm => "bomb_arm",
    FuseTick => "fuse_tick",
    Deploy => "deploy",
    ScannerPulse => "scanner_pulse",
    DrillBite => "drill_bite",
    DrillGlint => "drill_glint",
    ThrustIgnite => "thrust_ignite",
    LandLight => "land_light",
    LandMedium => "land_medium",
    LandHeavy => "land_heavy",
    WallBump => "wall_bump",
    HullHit => "hull_hit",
    DrillCrunch => "drill_crunch",
    BreakSoft => "break_soft",
    BreakHard => "break_hard",
    VaultReset => "vault_reset",
    FuelLow => "alarm_fuel_low",
    FuelCritical => "alarm_fuel_critical",
    HullLow => "alarm_hull_low",
    HullCritical => "alarm_hull_critical",
    Explosion => "explosion",
    PoiDiscovered => "poi_discovered",
    VaultFanfare => "vault_fanfare",
    Milestone => "milestone",
    Rescue => "rescue",
    ThrowWhoosh => "throw_whoosh",
    ChargeStick => "charge_stick",
    Detonator => "detonator",
    SmallBlast => "small_blast",
    ClusterBurst => "cluster_burst",
    ShapedBlast => "shaped_blast",
    DeepBlast => "deep_blast",
    FireWhoosh => "fire_whoosh",
    FireCrackle => "fire_crackle",
    EmpPulse => "emp_pulse",
    FlareIgnite => "flare_ignite",
    // After the explosive patches: a patch's variation seeds follow its declaration index.
    SprayPaint => "spray_paint",
    ThrustLoop => "loop_thrust",
    DrillSoftLoop => "loop_drill_soft",
    DrillHardLoop => "loop_drill_hard",
    GasHissLoop => "loop_gas_hiss",
    LavaSizzleLoop => "loop_lava_sizzle",
    BoulderRumbleLoop => "loop_boulder_rumble",
    // WP-DROPS pickups (appended: earlier patches keep their variation seeds).
    BlockPickup => "pickup_block",
    ArtifactPickup => "pickup_artifact",
    StowageFull => "stowage_full",
    BlockDump => "block_dump",
    // Caves 2.0 (wave 2): appended so every earlier patch keeps its variation seeds.
    CaveDiscovered => "cave_discovered",
    StalactiteCrack => "stalactite_crack",
    CacheOpen => "cache_open",
    MushroomBounce => "mushroom_bounce",
    // Wave 2 (villages): appended so earlier patches keep their variation seeds.
    VillageArrival => "village_arrival",
    ShrineChime => "shrine_chime",
    // Enemies (WP-ENEMIES), appended so earlier patches keep their variation seeds.
    CreatureScreech => "creature_screech",
    CreatureHiss => "creature_hiss",
    AcidSpit => "acid_spit",
    CreatureHurt => "creature_hurt",
    CreatureDeath => "creature_death",
    MachineAlert => "machine_alert",
    MachineCharge => "machine_charge",
    MachineZap => "machine_zap",
    MachineHurt => "machine_hurt",
    MachineDeath => "machine_death",
    EnemyBite => "enemy_bite",
    // Wave 2 (weapons and explosives): appended so earlier patches keep their seeds.
    SmokeBurst => "smoke_burst",
    ScareBang => "scare_bang",
    RivetShot => "rivet_shot",
    ScatterShot => "scatter_shot",
    CutterZap => "cutter_zap",
    RocketLaunch => "rocket_launch",
    HarpoonShot => "harpoon_shot",
    RailgunShot => "railgun_shot",
    WeaponDry => "weapon_dry",
    WeaponMount => "weapon_mount",
    ImpactRock => "impact_rock",
    ImpactHit => "impact_hit",
    TargetDestroyed => "target_destroyed",
    // Base building (WP-BASE), after every earlier patch so their variation seeds stay put.
    BuildPlace => "build_place",
    ModuleBuilt => "module_built",
    BuildRemove => "build_remove",
    ClaimStake => "claim_stake",
    FuelPump => "fuel_pump",
    SupplyDrop => "supply_drop",
    RepairWeld => "repair_weld",
    StorageStow => "storage_stow",
    UplinkBeam => "uplink_beam",
    HomeSet => "home_set",
    RefineryLoad => "refinery_load",
    WorkshopCraft => "workshop_craft",
    // WP-PERSIST (appended: variation seeds follow declaration order).
    CrewReturn => "crew_return",
    // Surface ambience (WP-SURFACE), after every earlier patch so their seeds stay put.
    WindLoop => "loop_wind",
    RainLoop => "loop_rain",
    // World clock (wave 3, T14), appended so every earlier patch keeps its variation seeds.
    DawnChorus => "dawn_chorus",
    DuskCrickets => "dusk_crickets",
    MidnightBell => "midnight_bell",
    BedDown => "bed_down",
    NightSkip => "night_skip",
    NightLoop => "loop_night",
    // The sky (F6, T16), appended so every earlier variation seed stays put.
    SkyBand => "sky_band",
    UpdraftWhoosh => "updraft_whoosh",
    IceCrackle => "ice_crackle",
    // The pilot on foot (Part II F5), after every earlier patch so their seeds stay put.
    HatchOpen => "hatch_open",
    HatchClose => "hatch_close",
    PilotJump => "pilot_jump",
    PilotLand => "pilot_land",
    PilotLandHard => "pilot_land_hard",
    PilotHurt => "pilot_hurt",
    PilotDown => "pilot_down",
    OxygenAlarm => "alarm_oxygen",
    JetpackBurst => "jetpack_burst",
    JetpackSputter => "jetpack_sputter",
    PickChip => "pick_chip",
    PickClink => "pick_clink",
    Splash => "splash",
    LedgeGrab => "ledge_grab",
    // People in the world (WP-PEOPLE), appended so every earlier patch keeps its seeds.
    PersonGreet => "person_greet",
    GiftGiven => "gift_given",
    AffinityUp => "affinity_up",
    RumourHeard => "rumour_heard",
    // Upgrade reward moments (WP-FEEL part 2), appended so earlier seeds stay put.
    UpgradeFanfare => "upgrade_fanfare",
    MilestoneFanfare => "milestone_fanfare",
    ActTwoFanfare => "act_two_fanfare",
    // Sky isles (WP-SKY, T20), appended so every earlier patch keeps its variation seeds.
    SkyCacheOpen => "sky_cache_open",
    BuoyRefuel => "buoy_refuel",
    IsleCollapse => "isle_collapse",
    JetStreamRush => "jet_stream_rush",
    SkyAirLoop => "loop_sky_air",
    WaterfallLoop => "loop_waterfall",
    // Sky continents, sky enemies and Act II weapons (wave 4, T21), appended so every earlier
    // patch keeps its variation seeds.
    ArcCrackle => "arc_crackle",
    GravityHum => "gravity_hum",
    GravityThrow => "gravity_throw",
    TetherShot => "tether_shot",
    TetherHook => "tether_hook",
    FlakThump => "flak_thump",
    FlakBurst => "flak_burst",
    Thunderclap => "thunderclap",
    LeverClunk => "lever_clunk",
    GateRumble => "gate_rumble",
    SwoopScreech => "swoop_screech",
    PirateHorn => "pirate_horn",
    HarpyScreech => "harpy_screech",
    CannonBoom => "cannon_boom",
    // The Moon (WP-MOON, wave 4 T22), appended so every earlier variation seed stays put.
    MoonSting => "moon_sting",
    VacuumAlarm => "alarm_vacuum",
    PylonChime => "pylon_chime",
    FacilityGate => "facility_gate",
    WardenWake => "warden_wake",
    CoreHum => "core_hum",
    UplinkWarp => "uplink_warp",
    // The pilot's arms and gear and ruin mechanisms (WP-PILOT), appended so every earlier
    // patch keeps its variation seeds.
    PistolPop => "pistol_pop",
    RifleCrack => "rifle_crack",
    ShotgunBoom => "shotgun_boom",
    WrenchSwing => "wrench_swing",
    GrappleFire => "grapple_fire",
    GliderOpen => "glider_open",
    GateGrind => "gate_grind",
    KeycardBeep => "keycard_beep",
    // Contractor homes (WP-HOME, wave 4 T24), appended so every earlier seed stays put.
    DeedStamp => "deed_stamp",
    BuildersHammer => "builders_hammer",
    HomeFanfare => "home_fanfare",
    SiteKnock => "site_knock",
    FurnitureThunk => "furniture_thunk",
    // Night-time gameplay (WP-NIGHT), appended so earlier seeds stay put.
    HoundHowl => "hound_howl",
    AlphaHowl => "alpha_howl",
    StalkerBurst => "stalker_burst",
    WispChime => "wisp_chime",
    MothFlutter => "moth_flutter",
    NightFallSting => "night_fall",
    DawnRelief => "dawn_relief",
    BurrowPuff => "burrow_puff",
    NightProwlLoop => "loop_night_prowl",
    // Story arcs (WP-STORY), appended so earlier seeds stay put.
    StoryHook => "story_hook",
    StoryQuill => "story_quill",
    StoryClimax => "story_climax",
    StoryResolved => "story_resolved",
    // Side-view tombs (WP-TOMBS T28), appended so earlier seeds stay put.
    TombRunLoop => "loop_tomb_run",
    TombBossLoop => "loop_tomb_boss",
    BladeSwish => "blade_swish",
    CrusherSlam => "crusher_slam",
    DartFire => "dart_fire",
    FloorCrumble => "floor_crumble",
    SandPour => "sand_pour",
    FogBurst => "fog_burst",
    MummyGroan => "mummy_groan",
    BoneRattle => "bone_rattle",
    StatueGrind => "statue_grind",
    ColossusSlam => "colossus_slam",
    MatriarchWail => "matriarch_wail",
    // Top-down crypts (WP-TOMBS T29), appended so earlier seeds stay put.
    DodgeRoll => "dodge_roll",
    ArrowTwang => "arrow_twang",
    SpikeShing => "spike_shing",
    PitDrop => "pit_drop",
    LordRoar => "lord_roar",
    StoneSlam => "stone_slam",
    HexBolt => "hex_bolt",
    CryptLoop => "loop_crypt",
    CryptBossLoop => "loop_crypt_boss",
    // The Wave 6 audio pass (ambience by environment, clock themes, low-g steps), appended at
    // the end so every earlier patch keeps its variation seeds.
    HighWindLoop => "loop_high_wind",
    MoonLoop => "loop_moon",
    MoonStep => "moon_step",
    TownLoop => "loop_town",
    ColonyLoop => "loop_colony",
    DawnTheme => "dawn_theme",
    DuskTheme => "dusk_theme",
    // Fluids (Part III F9, wave 7), appended at the end so every earlier patch keeps its seeds.
    FluidRush => "fluid_rush",
    SteamHiss => "steam_hiss",
    AcidFizz => "acid_fizz",
    RigSplash => "rig_splash",
    BubbleLoop => "loop_bubbles",
    // The authored Crust (Wave 8), appended at the end so every earlier patch keeps its seeds.
    CartClunk => "cart_clunk",
    CartSwitch => "cart_switch",
    TimberCreak => "timber_creak",
    RoofCollapse => "roof_collapse",
    SinkholeRumble => "sinkhole_rumble",
    DeepClaimFanfare => "deep_claim_fanfare",
    TubeBreath => "tube_breath",
    CrustTopsoilLoop => "loop_crust_topsoil",
    CrustClayLoop => "loop_crust_clay",
    CrustWorksLoop => "loop_crust_works",
    CrustSlateLoop => "loop_crust_slate",
    CrustBasaltLoop => "loop_crust_basalt",
    SinkholeLoop => "loop_sinkhole",
    // Descent II, the Old Deep (wave 8), appended at the end so every earlier patch keeps its
    // seeds.
    ResonanceChime => "resonance_chime",
    ValveTurn => "valve_turn",
    PistonSlam => "piston_slam",
    GeigerAlert => "geiger_alert",
    StormSiren => "storm_siren",
    CoreStrike => "core_strike",
    CoreWake => "core_wake",
    DeepLog => "deep_log",
    BoneCrumble => "bone_crumble",
    CrystalLoop => "loop_crystal_faults",
    FossilLoop => "loop_fossil_oceans",
    CathedralLoop => "loop_pressure_cathedrals",
    HollowLoop => "loop_radioactive_hollow",
    MachineLoop => "loop_machine_layer",
    GeodeLoop => "loop_void_geodes",
    MantleLoop => "loop_mantle_storm",
    StarCoreLoop => "loop_star_core",
    GeigerLoop => "loop_geiger",
    // The Living Dark (Wave 8, Descent III), appended at the end so every earlier patch keeps
    // its seeds.
    SporePuff => "spore_puff",
    RootCreak => "root_creak",
    FleshSquelch => "flesh_squelch",
    HiveChime => "hive_chime",
    MindRoar => "mind_roar",
    FungalLoop => "loop_fungal",
    RootLoop => "loop_roots",
    HiveLoop => "loop_hive",
    HeartbeatLoop => "loop_heartbeat",
    GlowSeaLoop => "loop_glow_sea",
    MindLoop => "loop_mind",
}

/// Playback metadata of a patch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchInfo {
    pub category: Category,
    /// Pre-rendered variations (each from its own seed).
    pub variations: u8,
    /// Voice-stealing priority (higher survives).
    pub priority: u8,
    /// Most simultaneous voices of this patch; the oldest is stolen past it.
    pub max_instances: u8,
    /// Send level to the shared reverb bus.
    pub reverb_send: f32,
    /// Random playback-rate jitter per play (fraction, e.g. 0.04 = ±4%).
    pub pitch_jitter: f32,
    /// Random gain jitter per play (dB, ±).
    pub gain_jitter_db: f32,
    /// How deeply this patch ducks lower-priority voices while it plays (0 = not at all).
    pub ducks: f32,
}

impl PatchInfo {
    const fn new(category: Category) -> Self {
        let (priority, reverb_send) = match category {
            Category::Interface => (3, 0.05),
            Category::Feedback => (4, 0.15),
            Category::Impact => (5, 0.2),
            Category::Alarm => (7, 0.05),
            Category::Loop => (2, 0.12),
            Category::Big => (8, 0.3),
        };
        Self {
            category,
            variations: if matches!(category, Category::Loop) {
                1
            } else {
                4
            },
            priority,
            max_instances: 3,
            reverb_send,
            pitch_jitter: 0.03,
            gain_jitter_db: 1.0,
            ducks: 0.0,
        }
    }

    const fn instances(mut self, max: u8) -> Self {
        self.max_instances = max;
        self
    }

    const fn send(mut self, reverb_send: f32) -> Self {
        self.reverb_send = reverb_send;
        self
    }

    const fn jitter(mut self, pitch: f32, gain_db: f32) -> Self {
        self.pitch_jitter = pitch;
        self.gain_jitter_db = gain_db;
        self
    }

    const fn ducking(mut self, depth: f32) -> Self {
        self.ducks = depth;
        self
    }

    const fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

impl Patch {
    /// Stable index (declaration order) for table lookups.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whether the patch is a seamless loop rather than a one-shot.
    #[must_use]
    pub const fn looped(self) -> bool {
        matches!(self.info().category, Category::Loop)
    }

    /// Playback metadata.
    #[must_use]
    #[allow(
        clippy::match_same_arms,
        reason = "each family of patches keeps its own row"
    )]
    pub const fn info(self) -> PatchInfo {
        use Category::{Alarm, Big, Feedback, Impact, Interface, Loop};
        match self {
            // The Old Deep (wave 8): the strata's beds, the core's theme and the dose crackle
            // loop; struck crystal and story stings ring out; machinery is physical.
            Self::CrystalLoop
            | Self::FossilLoop
            | Self::CathedralLoop
            | Self::HollowLoop
            | Self::MachineLoop
            | Self::GeodeLoop
            | Self::MantleLoop
            | Self::StarCoreLoop
            | Self::GeigerLoop => PatchInfo::new(Loop).instances(4).jitter(0.0, 0.0),
            Self::ResonanceChime | Self::DeepLog => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.5)
                .jitter(0.02, 0.5),
            Self::ValveTurn | Self::PistonSlam | Self::BoneCrumble => PatchInfo::new(Impact)
                .instances(3)
                .send(0.3)
                .jitter(0.05, 1.0),
            Self::GeigerAlert | Self::StormSiren => PatchInfo::new(Alarm)
                .instances(1)
                .send(0.2)
                .jitter(0.0, 0.3),
            Self::CoreStrike | Self::CoreWake => PatchInfo::new(Big)
                .instances(1)
                .send(0.55)
                .jitter(0.0, 0.3)
                .ducking(0.4)
                .priority(8),
            Self::UiMove => PatchInfo::new(Interface).instances(2).jitter(0.02, 0.5),
            Self::UiConfirm | Self::UiBack | Self::Toast => {
                PatchInfo::new(Interface).instances(2).jitter(0.01, 0.5)
            }
            Self::UiError => PatchInfo::new(Interface).instances(1).jitter(0.01, 0.5),
            Self::Pickup | Self::DrillGlint => PatchInfo::new(Feedback).jitter(0.05, 1.5),
            Self::Coins
            | Self::Purchase
            | Self::CrateOpen
            | Self::SalvageOpen
            | Self::SprayPaint
            | Self::CacheOpen
            | Self::BedDown => PatchInfo::new(Feedback).instances(2).jitter(0.02, 0.5),
            Self::ShrineChime => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.45)
                .jitter(0.0, 0.3),
            Self::BombArm | Self::Deploy | Self::ThrustIgnite => {
                PatchInfo::new(Feedback).instances(2)
            }
            Self::FuseTick => PatchInfo::new(Feedback).instances(4).jitter(0.01, 0.5),
            Self::ScannerPulse => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.35)
                .jitter(0.01, 0.5),
            Self::DrillBite | Self::DrillCrunch => {
                PatchInfo::new(Impact).instances(3).jitter(0.08, 2.0)
            }
            Self::LandLight
            | Self::LandMedium
            | Self::WallBump
            | Self::BreakSoft
            | Self::MushroomBounce
            | Self::BlockDump => PatchInfo::new(Impact).instances(2).jitter(0.05, 1.0),
            Self::LandHeavy | Self::HullHit | Self::BreakHard => PatchInfo::new(Impact)
                .instances(2)
                .jitter(0.04, 1.0)
                .ducking(0.25),
            Self::VaultReset => PatchInfo::new(Impact).instances(1).jitter(0.01, 0.5),
            Self::FuelLow | Self::FuelCritical | Self::HullLow | Self::HullCritical => {
                PatchInfo::new(Alarm).instances(1).jitter(0.0, 0.0)
            }
            Self::Explosion => PatchInfo::new(Big)
                .instances(3)
                .send(0.35)
                .jitter(0.06, 1.0)
                .ducking(0.55)
                .priority(9),
            Self::CaveDiscovered => PatchInfo::new(Big)
                .instances(1)
                .send(0.45)
                .jitter(0.0, 0.0)
                .ducking(0.25),
            Self::StalactiteCrack => PatchInfo::new(Impact)
                .instances(2)
                .send(0.35)
                .jitter(0.06, 1.0),
            Self::PoiDiscovered
            | Self::VaultFanfare
            | Self::Milestone
            | Self::Rescue
            | Self::VillageArrival
            | Self::CrewReturn
            | Self::PilotDown
            | Self::UpgradeFanfare
            | Self::MilestoneFanfare => PatchInfo::new(Big)
                .instances(1)
                .jitter(0.0, 0.0)
                .ducking(0.35),
            Self::HoundHowl => PatchInfo::new(Alarm)
                .instances(2)
                .priority(6)
                .send(0.45)
                .jitter(0.05, 1.0),
            Self::AlphaHowl | Self::NightFallSting => PatchInfo::new(Big)
                .instances(1)
                .send(0.55)
                .jitter(0.02, 0.5)
                .ducking(0.35)
                .priority(8),
            Self::StalkerBurst => PatchInfo::new(Impact)
                .instances(2)
                .send(0.3)
                .jitter(0.06, 1.0),
            Self::WispChime | Self::MothFlutter | Self::BurrowPuff => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.35)
                .jitter(0.06, 1.0),
            Self::ActTwoFanfare => PatchInfo::new(Big)
                .instances(1)
                .send(0.45)
                .jitter(0.0, 0.0)
                .ducking(0.6)
                .priority(9),
            Self::MoonSting | Self::CoreHum => PatchInfo::new(Big)
                .instances(1)
                .send(0.6)
                .jitter(0.0, 0.0)
                .ducking(0.5)
                .priority(9),
            Self::VacuumAlarm => PatchInfo::new(Alarm)
                .instances(1)
                .priority(8)
                .jitter(0.0, 0.0),
            Self::PylonChime => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.5)
                .jitter(0.0, 0.5),
            Self::FacilityGate | Self::WardenWake => PatchInfo::new(Big)
                .instances(1)
                .send(0.45)
                .jitter(0.02, 0.5)
                .ducking(0.35),
            Self::UplinkWarp => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.42)
                .jitter(0.015, 0.5),
            Self::PistolPop | Self::RifleCrack | Self::WrenchSwing => {
                PatchInfo::new(Impact).instances(4).jitter(0.05, 1.0)
            }
            Self::ShotgunBoom => PatchInfo::new(Impact)
                .instances(2)
                .jitter(0.04, 1.0)
                .ducking(0.15),
            Self::GrappleFire | Self::GliderOpen => {
                PatchInfo::new(Feedback).instances(2).jitter(0.05, 1.0)
            }
            Self::GateGrind => PatchInfo::new(Impact)
                .instances(2)
                .send(0.4)
                .jitter(0.04, 0.5),
            Self::KeycardBeep => PatchInfo::new(Feedback).instances(1).jitter(0.0, 0.0),
            // Stories: a curious hook when someone pitches one, a quill's scratch and chime for
            // each beat, a music-box swell at the climax and a warm cadence at the end.
            Self::StoryHook | Self::StoryQuill => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.3)
                .jitter(0.01, 0.5),
            Self::StoryClimax | Self::StoryResolved => PatchInfo::new(Big)
                .instances(1)
                .send(0.4)
                .jitter(0.0, 0.0)
                .ducking(0.3),
            Self::DodgeRoll | Self::ArrowTwang | Self::HexBolt => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.25)
                .jitter(0.05, 1.0),
            Self::SpikeShing | Self::PitDrop => PatchInfo::new(Impact)
                .instances(2)
                .send(0.35)
                .jitter(0.04, 1.0),
            Self::StoneSlam => PatchInfo::new(Impact)
                .instances(2)
                .send(0.45)
                .jitter(0.04, 1.0)
                .ducking(0.2),
            Self::LordRoar => PatchInfo::new(Big)
                .instances(1)
                .send(0.55)
                .jitter(0.0, 0.3)
                .ducking(0.4)
                .priority(8),
            Self::CryptLoop | Self::CryptBossLoop => {
                PatchInfo::new(Loop).instances(4).jitter(0.0, 0.0)
            }
            // The authored Crust (Wave 8): carts and collapses are physical, the Sinkhole's
            // rumble and the Deep Claim are set pieces, and every stratum's bed loops.
            Self::CartClunk | Self::CartSwitch => PatchInfo::new(Impact)
                .instances(2)
                .send(0.32)
                .jitter(0.04, 1.0),
            Self::TimberCreak | Self::TubeBreath => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.35)
                .jitter(0.06, 1.0),
            Self::RoofCollapse => PatchInfo::new(Impact)
                .instances(2)
                .send(0.45)
                .jitter(0.05, 1.0)
                .ducking(0.25),
            Self::SinkholeRumble => PatchInfo::new(Big)
                .instances(1)
                .send(0.5)
                .jitter(0.0, 0.3)
                .ducking(0.35)
                .priority(8),
            Self::DeepClaimFanfare => PatchInfo::new(Big)
                .instances(1)
                .send(0.48)
                .jitter(0.0, 0.0)
                .ducking(0.35),
            Self::CrustTopsoilLoop
            | Self::CrustClayLoop
            | Self::CrustWorksLoop
            | Self::CrustSlateLoop
            | Self::CrustBasaltLoop
            | Self::SinkholeLoop => PatchInfo::new(Loop)
                .instances(4)
                .send(0.18)
                .jitter(0.0, 0.0),
            // Fluids (Part III F9): the flood's gush and a rig's plunge are physical; steam
            // and fizz are small feedback; the bubbles under water loop.
            Self::FluidRush | Self::RigSplash => PatchInfo::new(Impact)
                .instances(2)
                .send(0.35)
                .jitter(0.05, 1.0),
            Self::SteamHiss | Self::AcidFizz => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.2)
                .jitter(0.06, 1.0),
            // A boot on the Moon: soft, frequent, never in the way.
            Self::MoonStep => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.05)
                .jitter(0.08, 2.0)
                .priority(2),
            // The world clock's dawn and dusk themes: music, so a little reverb and no jitter.
            Self::DawnTheme | Self::DuskTheme => {
                PatchInfo::new(Big).instances(1).send(0.45).jitter(0.0, 0.0)
            }
            Self::ThrowWhoosh | Self::ChargeStick | Self::FireCrackle => {
                PatchInfo::new(Feedback).instances(3).jitter(0.06, 1.0)
            }
            Self::Detonator | Self::FlareIgnite => {
                PatchInfo::new(Feedback).instances(2).jitter(0.01, 0.5)
            }
            Self::SmallBlast | Self::ClusterBurst => PatchInfo::new(Impact)
                .instances(4)
                .send(0.3)
                .jitter(0.08, 1.5)
                .ducking(0.2),
            Self::FireWhoosh | Self::EmpPulse => PatchInfo::new(Impact)
                .instances(2)
                .send(0.3)
                .jitter(0.04, 1.0),
            Self::ShapedBlast | Self::DeepBlast => PatchInfo::new(Big)
                .instances(2)
                .send(0.35)
                .jitter(0.05, 1.0)
                .ducking(0.55)
                .priority(9),
            Self::SmokeBurst => PatchInfo::new(Impact)
                .instances(2)
                .send(0.25)
                .jitter(0.05, 1.0),
            Self::ScareBang => PatchInfo::new(Big)
                .instances(2)
                .send(0.45)
                .jitter(0.04, 1.0)
                .ducking(0.45)
                .priority(8),
            Self::ImpactRock | Self::ImpactHit => {
                PatchInfo::new(Feedback).instances(4).jitter(0.1, 2.0)
            }
            Self::WeaponDry | Self::WeaponMount => {
                PatchInfo::new(Interface).instances(2).jitter(0.03, 0.5)
            }
            Self::ScatterShot | Self::HarpoonShot | Self::RocketLaunch => PatchInfo::new(Impact)
                .instances(3)
                .send(0.25)
                .jitter(0.05, 1.0),
            Self::RailgunShot | Self::TargetDestroyed => PatchInfo::new(Impact)
                .instances(2)
                .send(0.35)
                .jitter(0.03, 1.0)
                .ducking(0.2),
            Self::ThrustLoop
            | Self::DrillSoftLoop
            | Self::DrillHardLoop
            | Self::GasHissLoop
            | Self::LavaSizzleLoop
            | Self::BoulderRumbleLoop
            | Self::WindLoop
            | Self::RainLoop
            | Self::NightLoop
            | Self::SkyAirLoop
            | Self::WaterfallLoop
            | Self::NightProwlLoop
            | Self::TombRunLoop
            | Self::TombBossLoop
            // Part III F9: under fluid.
            | Self::BubbleLoop
            // Wave 6: the ambience beds chosen by environment.
            | Self::HighWindLoop
            | Self::MoonLoop
            | Self::TownLoop
            | Self::ColonyLoop
            // Wave 8: the Living Dark's music, one piece per stratum and the Mind's fight.
            | Self::FungalLoop
            | Self::RootLoop
            | Self::HiveLoop
            | Self::HeartbeatLoop
            | Self::GlowSeaLoop
            | Self::MindLoop => PatchInfo::new(Loop).instances(4).jitter(0.0, 0.0),
            // The Living Dark's one-shots: living sounds from where they happen; the Mind's
            // roar is big.
            Self::SporePuff | Self::RootCreak | Self::FleshSquelch => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.3)
                .jitter(0.06, 1.0),
            Self::HiveChime => PatchInfo::new(Feedback).instances(2).send(0.3).jitter(0.0, 0.5),
            Self::MindRoar => PatchInfo::new(Big)
                .instances(1)
                .send(0.55)
                .jitter(0.02, 0.5)
                .ducking(0.4)
                .priority(8),
            // Side-view tombs (T28): traps and tomb creatures heard from where they are.
            Self::BladeSwish | Self::DartFire | Self::BoneRattle => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.3)
                .jitter(0.05, 1.0),
            Self::FloorCrumble | Self::SandPour | Self::FogBurst | Self::StatueGrind => {
                PatchInfo::new(Feedback)
                    .instances(2)
                    .send(0.35)
                    .jitter(0.04, 1.0)
            }
            Self::MummyGroan | Self::MatriarchWail => PatchInfo::new(Impact)
                .instances(2)
                .send(0.4)
                .jitter(0.05, 1.0),
            Self::CrusherSlam | Self::ColossusSlam => PatchInfo::new(Impact)
                .instances(2)
                .send(0.4)
                .jitter(0.04, 1.0)
                .ducking(0.2),
            Self::SkyCacheOpen | Self::BuoyRefuel => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.3)
                .jitter(0.03, 1.0),
            Self::IsleCollapse => PatchInfo::new(Big)
                .instances(2)
                .send(0.4)
                .jitter(0.05, 1.0)
                .ducking(0.3),

            Self::DawnChorus | Self::DuskCrickets => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.4)
                .jitter(0.01, 0.5),
            Self::MidnightBell => PatchInfo::new(Big).instances(1).send(0.55).jitter(0.0, 0.3),
            Self::NightSkip | Self::DawnRelief => PatchInfo::new(Big)
                .instances(1)
                .send(0.45)
                .jitter(0.0, 0.0)
                .ducking(0.3),
            // A hoovered spray plays many of these at once: cheap, quiet and well jittered.
            Self::BlockPickup => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.08)
                .jitter(0.09, 2.0),
            Self::ArtifactPickup => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.25)
                .jitter(0.015, 0.5),
            Self::StowageFull => PatchInfo::new(Interface).instances(1).jitter(0.0, 0.3),
            Self::CreatureScreech | Self::MachineAlert => PatchInfo::new(Alarm)
                .instances(2)
                .priority(6)
                .send(0.3)
                .jitter(0.08, 1.5),
            Self::CreatureHiss | Self::MachineCharge => PatchInfo::new(Feedback)
                .instances(3)
                .send(0.25)
                .jitter(0.06, 1.0),
            Self::AcidSpit | Self::MachineZap | Self::RivetShot | Self::CutterZap => {
                PatchInfo::new(Feedback).instances(4).jitter(0.07, 1.5)
            }
            Self::CreatureHurt | Self::MachineHurt => {
                PatchInfo::new(Impact).instances(3).jitter(0.1, 1.5)
            }
            Self::CreatureDeath | Self::MachineDeath => PatchInfo::new(Impact)
                .instances(3)
                .send(0.3)
                .jitter(0.06, 1.0),
            Self::EnemyBite => PatchInfo::new(Impact)
                .instances(2)
                .jitter(0.06, 1.0)
                .ducking(0.2),
            Self::BuildPlace | Self::BuildRemove => {
                PatchInfo::new(Impact).instances(3).jitter(0.06, 1.0)
            }
            Self::ModuleBuilt
            | Self::StorageStow
            | Self::FuelPump
            | Self::RepairWeld
            | Self::RefineryLoad
            | Self::WorkshopCraft => PatchInfo::new(Feedback).instances(2).jitter(0.03, 0.5),
            Self::ClaimStake | Self::SupplyDrop | Self::UplinkBeam | Self::HomeSet => {
                PatchInfo::new(Feedback)
                    .instances(1)
                    .send(0.3)
                    .jitter(0.01, 0.5)
            }
            Self::SkyBand => PatchInfo::new(Big)
                .instances(1)
                .send(0.5)
                .jitter(0.0, 0.0)
                .ducking(0.3),
            Self::UpdraftWhoosh | Self::JetStreamRush => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.35)
                .jitter(0.05, 1.0),
            Self::IceCrackle => PatchInfo::new(Alarm)
                .instances(1)
                .priority(6)
                .send(0.2)
                .jitter(0.04, 0.5),
            Self::HatchOpen | Self::HatchClose => {
                PatchInfo::new(Feedback).instances(2).jitter(0.03, 0.5)
            }
            Self::PilotJump | Self::LedgeGrab => {
                PatchInfo::new(Feedback).instances(2).jitter(0.06, 1.0)
            }
            Self::PilotLand | Self::PickChip | Self::PickClink => {
                PatchInfo::new(Impact).instances(3).jitter(0.07, 1.5)
            }
            Self::PilotLandHard | Self::PilotHurt => PatchInfo::new(Impact)
                .instances(2)
                .jitter(0.05, 1.0)
                .ducking(0.2),
            Self::OxygenAlarm => PatchInfo::new(Alarm).instances(1).jitter(0.0, 0.0),
            Self::JetpackBurst | Self::JetpackSputter | Self::Splash => PatchInfo::new(Impact)
                .instances(2)
                .send(0.2)
                .jitter(0.05, 1.0),
            // People: a friendly murmur per greeting (well varied, so voices differ), a
            // sparkle for a gift, a warm chime when someone warms to you, a hush for rumours.
            Self::PersonGreet => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.12)
                .jitter(0.12, 1.5),
            Self::GiftGiven | Self::RumourHeard => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.3)
                .jitter(0.02, 0.5),
            Self::AffinityUp => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.35)
                .jitter(0.0, 0.0)
                .ducking(0.15),
            // T21: weapons are impacts, enemy calls carry into the sky, thunder ducks the mix.
            Self::ArcCrackle | Self::TetherShot | Self::FlakThump | Self::CannonBoom => {
                PatchInfo::new(Impact)
                    .instances(3)
                    .send(0.25)
                    .jitter(0.05, 1.0)
            }
            Self::GravityHum | Self::GravityThrow | Self::TetherHook => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.2)
                .jitter(0.04, 1.0),
            Self::FlakBurst => PatchInfo::new(Impact)
                .instances(4)
                .send(0.35)
                .jitter(0.07, 1.5)
                .ducking(0.15),
            Self::Thunderclap => PatchInfo::new(Big)
                .instances(2)
                .send(0.5)
                .jitter(0.05, 1.0)
                .ducking(0.4)
                .priority(8),
            Self::GateRumble => PatchInfo::new(Big)
                .instances(1)
                .send(0.45)
                .jitter(0.02, 0.5),
            Self::LeverClunk => PatchInfo::new(Impact)
                .instances(1)
                .send(0.45)
                .jitter(0.02, 0.5),
            Self::SwoopScreech | Self::HarpyScreech | Self::PirateHorn => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.4)
                .jitter(0.08, 1.0),
            // Homes: a stamp and a knock are crisp feedback, hammers ring a little, the keys'
            // fanfare is a big moment, furniture is a soft thud that may stack.
            Self::DeedStamp | Self::SiteKnock => PatchInfo::new(Feedback)
                .instances(1)
                .send(0.2)
                .jitter(0.03, 0.5),
            Self::BuildersHammer => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.3)
                .jitter(0.06, 1.0),
            Self::HomeFanfare => PatchInfo::new(Big)
                .instances(1)
                .send(0.4)
                .jitter(0.0, 0.0)
                .ducking(0.4),
            Self::FurnitureThunk => PatchInfo::new(Feedback)
                .instances(2)
                .send(0.15)
                .jitter(0.08, 1.0),
        }
    }

    /// The raw design (before normalization and edge treatment) for variation `seed`.
    fn design(self, seed: u64) -> Vec<f32> {
        let mut rng = Rng::new(seed);
        let rng = &mut rng;
        match self {
            Self::UiMove => ui_move(rng),
            Self::UiConfirm => ui_confirm(rng),
            Self::UiBack => ui_back(rng),
            Self::UiError => ui_error(rng),
            Self::Toast => toast(rng),
            Self::Pickup => pickup(rng),
            Self::Coins => coins(rng),
            Self::Purchase => purchase(rng),
            Self::CrateOpen => crate_open(rng),
            Self::SalvageOpen => salvage_open(rng),
            Self::BombArm => bomb_arm(rng),
            Self::FuseTick => fuse_tick(rng),
            Self::Deploy => deploy(rng),
            Self::ScannerPulse => scanner_pulse(rng),
            Self::DrillBite => drill_bite(rng),
            Self::DrillGlint => drill_glint(rng),
            Self::ThrustIgnite => thrust_ignite(rng),
            Self::LandLight => landing(rng, 0),
            Self::LandMedium => landing(rng, 1),
            Self::LandHeavy => landing(rng, 2),
            Self::WallBump => wall_bump(rng),
            Self::HullHit => hull_hit(rng),
            Self::DrillCrunch => drill_crunch(rng),
            Self::BreakSoft => break_through(rng, false),
            Self::BreakHard => break_through(rng, true),
            Self::VaultReset => vault_reset(rng),
            Self::FuelLow => fuel_alarm(rng, false),
            Self::FuelCritical => fuel_alarm(rng, true),
            Self::HullLow => hull_alarm(rng, false),
            Self::HullCritical => hull_alarm(rng, true),
            Self::Explosion => explosion(rng),
            Self::PoiDiscovered => poi_discovered(rng),
            Self::VaultFanfare => vault_fanfare(rng),
            Self::Milestone => milestone(rng),
            Self::Rescue => rescue(rng),
            Self::SprayPaint => spray_paint(rng),
            Self::VillageArrival => village_arrival(rng),
            Self::ShrineChime => shrine_chime(rng),
            Self::ThrowWhoosh => throw_whoosh(rng),
            Self::ChargeStick => charge_stick(rng),
            Self::Detonator => detonator(rng),
            Self::SmallBlast => small_blast(rng),
            Self::ClusterBurst => cluster_burst(rng),
            Self::ShapedBlast => shaped_blast(rng),
            Self::DeepBlast => deep_blast(rng),
            Self::FireWhoosh => fire_whoosh(rng),
            Self::FireCrackle => fire_crackle(rng),
            Self::EmpPulse => emp_pulse(rng),
            Self::FlareIgnite => flare_ignite(rng),
            Self::CrewReturn => crew_return(rng),
            Self::PersonGreet => person_greet(rng),
            Self::GiftGiven => gift_given(rng),
            Self::AffinityUp => affinity_up(rng),
            Self::RumourHeard => rumour_heard(rng),
            Self::ThrustLoop => thrust_loop(rng),
            Self::DrillSoftLoop => drill_soft_loop(rng),
            Self::DrillHardLoop => drill_hard_loop(rng),
            Self::GasHissLoop => gas_hiss_loop(rng),
            Self::LavaSizzleLoop => lava_sizzle_loop(rng),
            Self::BoulderRumbleLoop => boulder_rumble_loop(rng),
            Self::BlockPickup => block_pickup(rng),
            Self::ArtifactPickup => artifact_pickup(rng),
            Self::StowageFull => stowage_full(rng),
            Self::BlockDump => block_dump(rng),
            Self::CaveDiscovered => cave_discovered(rng),
            Self::StalactiteCrack => stalactite_crack(rng),
            Self::CacheOpen => cache_open(rng),
            Self::MushroomBounce => mushroom_bounce(rng),
            Self::CreatureScreech => creature_screech(rng),
            Self::CreatureHiss => creature_hiss(rng),
            Self::AcidSpit => acid_spit(rng),
            Self::CreatureHurt => creature_hurt(rng),
            Self::CreatureDeath => creature_death(rng),
            Self::MachineAlert => machine_alert(rng),
            Self::MachineCharge => machine_charge(rng),
            Self::MachineZap => machine_zap(rng),
            Self::MachineHurt => machine_hurt(rng),
            Self::MachineDeath => machine_death(rng),
            Self::EnemyBite => enemy_bite(rng),
            Self::SmokeBurst => smoke_burst(rng),
            Self::ScareBang => scare_bang(rng),
            Self::RivetShot => rivet_shot(rng),
            Self::ScatterShot => scatter_shot(rng),
            Self::CutterZap => cutter_zap(rng),
            Self::RocketLaunch => rocket_launch(rng),
            Self::HarpoonShot => harpoon_shot(rng),
            Self::RailgunShot => railgun_shot(rng),
            Self::WeaponDry => weapon_dry(rng),
            Self::WeaponMount => weapon_mount(rng),
            Self::ImpactRock => impact_rock(rng),
            Self::ImpactHit => impact_hit(rng),
            Self::TargetDestroyed => target_destroyed(rng),
            Self::BuildPlace => build_place(rng),
            Self::ModuleBuilt => module_built(rng),
            Self::BuildRemove => build_remove(rng),
            Self::ClaimStake => claim_stake(rng),
            Self::FuelPump => fuel_pump(rng),
            Self::SupplyDrop => supply_drop(rng),
            Self::RepairWeld => repair_weld(rng),
            Self::StorageStow => storage_stow(rng),
            Self::UplinkBeam => uplink_beam(rng),
            Self::HomeSet => home_set(rng),
            Self::RefineryLoad => refinery_load(rng),
            Self::WorkshopCraft => workshop_craft(rng),
            Self::WindLoop => wind_loop(rng),
            Self::RainLoop => rain_loop(rng),
            Self::DawnChorus => dawn_chorus(rng),
            Self::DuskCrickets => dusk_crickets(rng),
            Self::MidnightBell => midnight_bell(rng),
            Self::BedDown => bed_down(rng),
            Self::NightSkip => night_skip(rng),
            Self::NightLoop => night_loop(rng),
            Self::SkyBand => sky_band(rng),
            Self::UpdraftWhoosh => updraft_whoosh(rng),
            Self::IceCrackle => ice_crackle(rng),
            Self::HatchOpen => hatch(rng, true),
            Self::HatchClose => hatch(rng, false),
            Self::PilotJump => pilot_jump(rng),
            Self::PilotLand => pilot_land(rng, false),
            Self::PilotLandHard => pilot_land(rng, true),
            Self::PilotHurt => pilot_hurt(rng),
            Self::PilotDown => pilot_down(rng),
            Self::OxygenAlarm => oxygen_alarm(rng),
            Self::JetpackBurst => jetpack_burst(rng),
            Self::JetpackSputter => jetpack_sputter(rng),
            Self::PickChip => pick_chip(rng),
            Self::PickClink => pick_clink(rng),
            Self::Splash => splash(rng),
            Self::LedgeGrab => ledge_grab(rng),
            Self::UpgradeFanfare => upgrade_fanfare(rng),
            Self::MilestoneFanfare => milestone_fanfare(rng),
            Self::ActTwoFanfare => act_two_fanfare(rng),
            Self::SkyCacheOpen => sky_cache_open(rng),
            Self::BuoyRefuel => buoy_refuel(rng),
            Self::IsleCollapse => isle_collapse(rng),
            Self::JetStreamRush => jet_stream_rush(rng),
            Self::SkyAirLoop => sky_air_loop(rng),
            Self::WaterfallLoop => waterfall_loop(rng),
            Self::ArcCrackle => arc_crackle(rng),
            Self::GravityHum => gravity_hum(rng),
            Self::GravityThrow => gravity_throw(rng),
            Self::TetherShot => tether_shot(rng),
            Self::TetherHook => tether_hook(rng),
            Self::FlakThump => flak_thump(rng),
            Self::FlakBurst => flak_burst(rng),
            Self::Thunderclap => thunderclap(rng),
            Self::LeverClunk => lever_clunk(rng),
            Self::GateRumble => gate_rumble(rng),
            Self::SwoopScreech => swoop_screech(rng),
            Self::PirateHorn => pirate_horn(rng),
            Self::HarpyScreech => harpy_screech(rng),
            Self::CannonBoom => cannon_boom(rng),
            Self::MoonSting => moon_sting(rng),
            Self::VacuumAlarm => vacuum_alarm(rng),
            Self::PylonChime => pylon_chime(rng),
            Self::FacilityGate => facility_gate(rng),
            Self::WardenWake => warden_wake(rng),
            Self::CoreHum => core_hum(rng),
            Self::UplinkWarp => uplink_warp(rng),
            Self::PistolPop => pistol_pop(rng),
            Self::RifleCrack => rifle_crack(rng),
            Self::ShotgunBoom => shotgun_boom(rng),
            Self::WrenchSwing => wrench_swing(rng),
            Self::GrappleFire => grapple_fire(rng),
            Self::GliderOpen => glider_open(rng),
            Self::GateGrind => gate_grind(rng),
            Self::KeycardBeep => keycard_beep(rng),
            Self::DeedStamp => deed_stamp(rng),
            Self::BuildersHammer => builders_hammer(rng),
            Self::HomeFanfare => home_fanfare(rng),
            Self::SiteKnock => site_knock(rng),
            Self::FurnitureThunk => furniture_thunk(rng),
            Self::HoundHowl => hound_howl(rng, 1.0),
            Self::AlphaHowl => hound_howl(rng, 0.62),
            Self::StalkerBurst => stalker_burst(rng),
            Self::WispChime => wisp_chime(rng),
            Self::MothFlutter => moth_flutter(rng),
            Self::NightFallSting => night_fall(rng),
            Self::DawnRelief => dawn_relief(rng),
            Self::BurrowPuff => burrow_puff(rng),
            Self::NightProwlLoop => night_prowl_loop(rng),
            Self::StoryHook => story_hook(rng),
            Self::StoryQuill => story_quill(rng),
            Self::StoryClimax => story_climax(rng),
            Self::StoryResolved => story_resolved(rng),
            Self::TombRunLoop => tomb_run_loop(rng),
            Self::TombBossLoop => tomb_boss_loop(rng),
            Self::BladeSwish => blade_swish(rng),
            Self::CrusherSlam => crusher_slam(rng),
            Self::DartFire => dart_fire(rng),
            Self::FloorCrumble => floor_crumble(rng),
            Self::SandPour => sand_pour(rng),
            Self::FogBurst => fog_burst(rng),
            Self::MummyGroan => mummy_groan(rng),
            Self::BoneRattle => bone_rattle(rng),
            Self::StatueGrind => statue_grind(rng),
            Self::ColossusSlam => colossus_slam(rng),
            Self::MatriarchWail => matriarch_wail(rng),
            Self::DodgeRoll => dodge_roll(rng),
            Self::ArrowTwang => arrow_twang(rng),
            Self::SpikeShing => spike_shing(rng),
            Self::PitDrop => pit_drop(rng),
            Self::LordRoar => lord_roar(rng),
            Self::StoneSlam => stone_slam(rng),
            Self::HexBolt => hex_bolt(rng),
            Self::CryptLoop => crypt_loop(rng),
            Self::CryptBossLoop => crypt_boss_loop(rng),
            Self::HighWindLoop => high_wind_loop(rng),
            Self::MoonLoop => moon_loop(rng),
            Self::MoonStep => moon_step(rng),
            Self::TownLoop => town_loop(rng),
            Self::ColonyLoop => colony_loop(rng),
            Self::DawnTheme => dawn_theme(rng),
            Self::DuskTheme => dusk_theme(rng),
            Self::FluidRush => fluid_rush(rng),
            Self::SteamHiss => steam_hiss(rng),
            Self::AcidFizz => acid_fizz(rng),
            Self::RigSplash => rig_splash(rng),
            Self::BubbleLoop => bubble_loop(rng),
            Self::CartClunk => cart_clunk(rng),
            Self::CartSwitch => cart_switch(rng),
            Self::TimberCreak => timber_creak(rng),
            Self::RoofCollapse => roof_collapse(rng),
            Self::SinkholeRumble => sinkhole_rumble(rng),
            Self::DeepClaimFanfare => deep_claim_fanfare(rng),
            Self::TubeBreath => tube_breath(rng),
            Self::CrustTopsoilLoop => crust_topsoil_loop(rng),
            Self::CrustClayLoop => crust_clay_loop(rng),
            Self::CrustWorksLoop => crust_works_loop(rng),
            Self::CrustSlateLoop => crust_slate_loop(rng),
            Self::CrustBasaltLoop => crust_basalt_loop(rng),
            Self::SinkholeLoop => sinkhole_loop(rng),
            Self::ResonanceChime => old_deep::resonance_chime(rng),
            Self::ValveTurn => old_deep::valve_turn(rng),
            Self::PistonSlam => old_deep::piston_slam(rng),
            Self::GeigerAlert => old_deep::geiger_alert(rng),
            Self::StormSiren => old_deep::storm_siren(rng),
            Self::CoreStrike => old_deep::core_strike(rng),
            Self::CoreWake => old_deep::core_wake(rng),
            Self::DeepLog => old_deep::deep_log(rng),
            Self::BoneCrumble => old_deep::bone_crumble(rng),
            Self::CrystalLoop => old_deep::crystal_loop(rng),
            Self::FossilLoop => old_deep::fossil_loop(rng),
            Self::CathedralLoop => old_deep::cathedral_loop(rng),
            Self::HollowLoop => old_deep::hollow_loop(rng),
            Self::MachineLoop => old_deep::machine_loop(rng),
            Self::GeodeLoop => old_deep::geode_loop(rng),
            Self::MantleLoop => old_deep::mantle_loop(rng),
            Self::StarCoreLoop => old_deep::star_core_loop(rng),
            Self::GeigerLoop => old_deep::geiger_loop(rng),
            Self::SporePuff => spore_puff(rng),
            Self::RootCreak => root_creak(rng),
            Self::FleshSquelch => flesh_squelch(rng),
            Self::HiveChime => hive_chime(rng),
            Self::MindRoar => mind_roar(rng),
            Self::FungalLoop => fungal_loop(rng),
            Self::RootLoop => root_loop(rng),
            Self::HiveLoop => hive_loop(rng),
            Self::HeartbeatLoop => heartbeat_loop(rng),
            Self::GlowSeaLoop => glow_sea_loop(rng),
            Self::MindLoop => mind_loop(rng),
        }
    }
}

/// The seed of variation `variation` of `patch` under bank seed `seed`.
#[must_use]
pub const fn variation_seed(seed: u64, patch: Patch, variation: u8) -> u64 {
    seed.wrapping_mul(0x0100_0000_01B3)
        ^ ((patch.index() as u64) << 16)
        ^ (variation as u64).wrapping_mul(0x9E37_79B9)
}

/// Renders a finished patch: designed, DC-free, click-free at its edges (or seamless at its loop
/// point) and normalized to its category's loudness, peaking at most [`PEAK_CEILING`].
#[must_use]
pub fn render_patch(patch: Patch, seed: u64) -> Vec<f32> {
    let mut samples = patch.design(seed);
    for sample in &mut samples {
        if !sample.is_finite() {
            *sample = 0.0;
        }
    }
    if patch.looped() {
        // Before the fold, so the seam crossfades filtered audio with filtered audio.
        sub_cut(&mut samples, 2);
        samples = fold_loop(&samples);
        let mean = samples.iter().sum::<f32>() / samples.len().max(1) as f32;
        for sample in &mut samples {
            *sample -= mean;
        }
    } else {
        sub_cut(&mut samples, 1);
        trim_tail(&mut samples);
        fade_edges(&mut samples);
    }
    normalize(&mut samples, patch.info().category.target_db());
    samples
}

/// Below this (Hz) a patch's content is removed: brown-noise rumble and sub glides otherwise
/// pile up infrasonic energy nobody hears but every speaker and the limiter have to carry.
const SUB_CUT_HZ: f32 = 30.0;

/// High-passes `samples` at [`SUB_CUT_HZ`] with `stages` cascaded 12 dB/octave Butterworth
/// sections (this also removes any DC offset).
fn sub_cut(samples: &mut [f32], stages: usize) {
    for _ in 0..stages {
        let mut filter = Svf::new(
            FilterMode::HighPass,
            SUB_CUT_HZ,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        for sample in samples.iter_mut() {
            *sample = filter.process(*sample);
        }
    }
}

/// Folds the last [`LOOP_CROSSFADE_SECONDS`] over the start so the buffer loops seamlessly:
/// the sample after the last one is (nearly) the one that originally followed it.
fn fold_loop(samples: &[f32]) -> Vec<f32> {
    let length = samples_for(LOOP_SECONDS).min(samples.len());
    let fade = samples_for(LOOP_CROSSFADE_SECONDS).min(samples.len() - length);
    let mut out = samples[..length].to_vec();
    for (index, slot) in out.iter_mut().enumerate().take(fade) {
        let (fade_out, fade_in) = equal_power(index as f32 / fade as f32);
        *slot = *slot * fade_in + samples[length + index] * fade_out;
    }
    out
}

/// Drops the inaudible tail (below -70 dBFS relative to the peak).
fn trim_tail(samples: &mut Vec<f32>) {
    let peak = analysis::peak(samples);
    let floor = peak * db_to_gain(-70.0);
    let end = samples
        .iter()
        .rposition(|s| s.abs() > floor)
        .map_or(0, |index| index + 1);
    samples.truncate((end + samples_for(0.01)).min(samples.len()));
}

/// A 1 ms fade-in and a 12 ms fade-out so a one-shot starts and ends at exactly zero.
fn fade_edges(samples: &mut [f32]) {
    let len = samples.len();
    let fade_in = samples_for(0.001).min(len);
    for (index, sample) in samples.iter_mut().enumerate().take(fade_in) {
        *sample *= index as f32 / fade_in as f32;
    }
    let fade_out = samples_for(0.012).min(len);
    for index in 0..fade_out {
        let position = len - 1 - index;
        samples[position] *= index as f32 / fade_out as f32;
    }
}

/// Scales `samples` to `target_db` loudness, then down further if that would exceed the peak
/// ceiling.
fn normalize(samples: &mut [f32], target_db: f32) {
    let loudness = analysis::loudness(samples);
    if loudness <= 1e-6 {
        return;
    }
    let mut gain = db_to_gain(target_db) / loudness;
    let peak = analysis::peak(samples);
    if peak * gain > PEAK_CEILING {
        gain = PEAK_CEILING / peak;
    }
    for sample in samples {
        *sample *= gain;
    }
}

// ---------------------------------------------------------------------------------------------
// Shared layers
// ---------------------------------------------------------------------------------------------

/// A sine at a fixed frequency (stateless).
fn sine(t: f32, frequency: f32) -> f32 {
    (TAU * frequency * t).sin()
}

/// A bank of decaying sine partials `(frequency, amplitude, decay seconds)`: bells, metal.
fn partials(t: f32, bank: &[(f32, f32, f32)]) -> f32 {
    bank.iter()
        .map(|&(frequency, amplitude, time)| sine(t, frequency) * amplitude * decay(t, time))
        .sum()
}

/// A short metallic clank: inharmonic partials around `base` Hz.
fn clank(rng: &mut Rng, base: f32, time: f32) -> Vec<f32> {
    let ratios = [1.0, 2.76, 5.40, 8.93];
    let bank: Vec<(f32, f32, f32)> = ratios
        .iter()
        .enumerate()
        .map(|(index, ratio)| {
            (
                base * ratio * rng.jitter(0.03),
                1.0 / (1.0 + index as f32),
                time * rng.jitter(0.2) / (1.0 + index as f32 * 0.5),
            )
        })
        .collect();
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 3_000.0, 0.7);
    render(time * 1.2, |t| {
        partials(t, &bank) + hp.process(noise.next()) * perc(t, 0.000_2, 0.01) * 0.6
    })
}

/// A low thump: a sine glide with a percussive envelope.
fn thump(start: f32, end: f32, time: f32, drive: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    render(time * 1.1, |t| {
        let frequency = glide(t, start, end, time * 0.15);
        saturate(osc.next(frequency) * perc(t, 0.001, time), drive)
    })
}

/// A filtered noise burst.
fn burst(
    seed: u64,
    color: NoiseColor,
    mode: FilterMode,
    cutoff: f32,
    q: f32,
    time: f32,
) -> Vec<f32> {
    let mut noise = Noise::new(color, seed);
    let mut filter = Svf::new(mode, cutoff, q);
    render(time * 1.1, |t| {
        filter.process(noise.next()) * perc(t, 0.000_5, time)
    })
}

/// `count` event times in `[start, end)`, clustered toward `start` when `skew` > 1.
fn scatter(rng: &mut Rng, count: usize, start: f32, end: f32, skew: f32) -> Vec<f32> {
    let mut times: Vec<f32> = (0..count)
        .map(|_| start + (end - start) * rng.unit().powf(skew))
        .collect();
    times.sort_by(f32::total_cmp);
    times
}

/// Grit: short band-passed noise grains (debris, crumbling dirt, rock chips), bunched early in
/// `span` and thinning out like settling debris.
fn grains(
    rng: &mut Rng,
    out: &mut Vec<f32>,
    count: usize,
    span: (f32, f32),
    band: (f32, f32),
    level: f32,
) {
    scatter_grains(rng, out, count, span, band, level, true);
}

/// Grains over `span`: settling as in [`grains`], or (for loops, where anything bunched or
/// fading pulses at the loop rate) spread evenly at a steady level.
fn scatter_grains(
    rng: &mut Rng,
    out: &mut Vec<f32>,
    count: usize,
    span: (f32, f32),
    band: (f32, f32),
    level: f32,
    settling: bool,
) {
    let skew = if settling { 1.8 } else { 1.0 };
    for time in scatter(rng, count, span.0, span.1, skew) {
        let cutoff = rng.range(band.0, band.1);
        let length = rng.range(0.008, 0.035);
        let grain = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            cutoff,
            1.4,
            length,
        );
        let fade = if settling {
            (1.0 - (time - span.0) / (span.1 - span.0).max(1e-3)).max(0.15)
        } else {
            1.0
        };
        mix_into(out, &grain, time, level * rng.range(0.4, 1.0) * fade);
    }
}

/// A note of a brassy synth voice: detuned saws through an enveloped low-pass.
fn brass(rng: &mut Rng, frequency: f32, gate: f32, brightness: f32) -> Vec<f32> {
    let detune = [0.996, 1.0, 1.004];
    let mut oscs: Vec<Osc> = detune
        .iter()
        .map(|_| Osc::new(Wave::Saw).with_phase(rng.unit()))
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 1.1);
    let amp = Adsr::new(0.02, 0.25, 0.75, 0.35);
    let cut = Adsr::new(0.04, 0.3, 0.45, 0.3);
    let vibrato_rate = rng.range(4.8, 5.6);
    render(gate + 0.5, |t| {
        let vibrato = 1.0 + 0.004 * sine(t, vibrato_rate) * (t / 0.4).min(1.0);
        let raw: f32 = oscs
            .iter_mut()
            .zip(detune)
            .map(|(osc, d)| osc.next(frequency * d * vibrato))
            .sum::<f32>()
            / 3.0;
        let cutoff = frequency * (1.5 + brightness * 6.0 * cut.level(t, gate));
        filter.run(raw, cutoff, 1.1) * amp.level(t, gate)
    })
}

/// An FM bell strike.
fn bell(frequency: f32, ratio: f32, index: f32, time: f32) -> Vec<f32> {
    let mut fm = Fm::default();
    render(time * 1.1, |t| {
        fm.next(frequency, ratio, index * decay(t, time * 0.4)) * perc(t, 0.001, time)
    })
}

// ---------------------------------------------------------------------------------------------
// Interface
// ---------------------------------------------------------------------------------------------

fn ui_move(rng: &mut Rng) -> Vec<f32> {
    let start = 2_300.0 * rng.jitter(0.03);
    let mut osc = Osc::new(Wave::Sine);
    let mut click = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    render(0.07, |t| {
        let tone = osc.next(glide(t, start, start * 0.75, 0.012)) * perc(t, 0.001, 0.045);
        tone + hp.process(click.next()) * perc(t, 0.000_3, 0.006) * 0.12
    })
}

fn two_notes(rng: &mut Rng, notes: [f32; 2], gap: f32, wave: Wave, length: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in notes.into_iter().enumerate() {
        let note = note * rng.jitter(0.006);
        let mut osc = Osc::new(wave);
        osc.width = 0.3;
        let mut sub = Osc::new(Wave::Sine);
        let mut filter = Svf::new(FilterMode::LowPass, 4_000.0, 0.8);
        let tone = render(length + 0.05, |t| {
            let cutoff = note * (1.5 + 5.0 * decay(t, 0.08));
            let raw = osc.next(note) * 0.6 + sub.next(note) * 0.5;
            filter.run(raw, cutoff, 0.8) * perc(t, 0.002, length)
        });
        mix_into(&mut out, &tone, gap * index as f32, 1.0);
    }
    out
}

fn ui_confirm(rng: &mut Rng) -> Vec<f32> {
    two_notes(rng, [784.0, 1_175.0], 0.06, Wave::Square, 0.12)
}

fn ui_back(rng: &mut Rng) -> Vec<f32> {
    two_notes(rng, [880.0, 587.0], 0.055, Wave::Triangle, 0.1)
}

fn ui_error(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for pulse in 0..2 {
        let mut a = Osc::new(Wave::Square);
        let mut b = Osc::new(Wave::Square);
        let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 1.2);
        let base = 150.0 * rng.jitter(0.01);
        let gate = 0.085;
        let env = Adsr::new(0.003, 0.05, 0.7, 0.03);
        let tone = render(gate + 0.06, |t| {
            let raw = (a.next(base) + b.next(base * 1.06)) * 0.5;
            fold(saturate(filter.process(raw), 2.0) * 1.4) * env.level(t, gate)
        });
        mix_into(&mut out, &tone, pulse as f32 * 0.13, 1.0);
    }
    out
}

fn toast(rng: &mut Rng) -> Vec<f32> {
    let root = 1_318.5 * rng.jitter(0.005);
    let mut out = bell(root, 1.0, 1.4, 0.5);
    mix_into(&mut out, &bell(root * 1.5, 2.0, 0.8, 0.35), 0.0, 0.35);
    out
}

// ---------------------------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------------------------

/// Pentatonic choices for sparkly pickups.
const SPARKLE_NOTES: [f32; 6] = [1_568.0, 1_760.0, 2_093.0, 2_349.0, 2_637.0, 3_136.0];

fn pickup(rng: &mut Rng) -> Vec<f32> {
    let first = rng.index(SPARKLE_NOTES.len() - 2);
    let mut out = bell(SPARKLE_NOTES[first], 2.0, 1.6, 0.22);
    mix_into(
        &mut out,
        &bell(SPARKLE_NOTES[first + 2], 2.0, 1.2, 0.28),
        0.045,
        0.8,
    );
    out
}

fn coins(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(150.0, 110.0, 0.08, 1.0);
    for sample in &mut out {
        *sample *= 0.4;
    }
    let mut time = 0.02;
    let hits = 5 + rng.index(3);
    for index in 0..hits {
        let base = rng.range(2_700.0, 3_600.0) * (1.0 + index as f32 * 0.04);
        let ring_time = rng.range(0.12, 0.25);
        let coin = clank(rng, base, ring_time);
        mix_into(&mut out, &coin, time, rng.range(0.35, 0.6));
        time += rng.range(0.035, 0.09);
    }
    // The register's "ching".
    let ching = bell(2_637.0 * rng.jitter(0.01), 1.0, 0.6, 0.55);
    mix_into(&mut out, &ching, time + 0.03, 0.55);
    out
}

fn purchase(rng: &mut Rng) -> Vec<f32> {
    let notes = [523.25, 659.25, 783.99, 1_046.5];
    let mut out = Vec::new();
    for (index, note) in notes.into_iter().enumerate() {
        let gate = if index == 3 { 0.25 } else { 0.1 };
        let voice = brass(rng, note, gate, 0.9);
        mix_into(&mut out, &voice, index as f32 * 0.07, 0.5);
    }
    for time in scatter(rng, 6, 0.22, 0.6, 1.0) {
        let note = SPARKLE_NOTES[rng.index(SPARKLE_NOTES.len())] * 2.0;
        mix_into(&mut out, &bell(note, 2.0, 0.8, 0.12), time, 0.12);
    }
    let mut echo = Delay::new(0.3).with_damping(0.5);
    out.resize(out.len() + samples_for(0.4), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.11, 0.3) * 0.3;
    }
    out
}

fn crate_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_800.0,
        2.0,
        0.03,
    );
    let lid = thump(125.0 * rng.jitter(0.05), 80.0, 0.12, 1.5);
    mix_into(&mut out, &lid, 0.04, 0.9);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        600.0,
        0.7,
        0.1,
    );
    mix_into(&mut out, &body, 0.04, 0.6);
    // A creaking hinge.
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::BandPass, 900.0, 6.0);
    let pitch = 210.0 * rng.jitter(0.08);
    let creak = render(0.3, |t| {
        let frequency = pitch * (1.0 + 0.08 * sine(t, 23.0));
        filter.process(saw.next(frequency)) * segments(t, &[(0.0, 0.0), (0.05, 0.6), (0.3, 0.0)])
    });
    mix_into(&mut out, &creak, 0.08, 0.35);
    mix_into(&mut out, &bell(2_093.0, 2.0, 1.0, 0.18), 0.2, 0.2);
    mix_into(&mut out, &bell(2_637.0, 2.0, 1.0, 0.22), 0.26, 0.2);
    out
}

fn salvage_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 410.0, 0.4);
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::HighPass, 2_000.0, 0.8);
    let hiss = render(0.5, |t| {
        let cutoff = 2_000.0 + 4_000.0 * (t / 0.5);
        filter.run(noise.next(), cutoff, 0.8)
            * segments(t, &[(0.0, 0.0), (0.03, 0.8), (0.2, 0.5), (0.5, 0.0)])
    });
    mix_into(&mut out, &hiss, 0.06, 0.45);
    mix_into(&mut out, &thump(95.0, 60.0, 0.18, 1.5), 0.0, 0.8);
    mix_into(&mut out, &bell(1_760.0, 2.0, 1.0, 0.3), 0.3, 0.2);
    out
}

fn bomb_arm(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 300.0, 0.12);
    let mut osc = Osc::new(Wave::Square);
    osc.width = 0.25;
    let mut filter = Svf::new(FilterMode::LowPass, 3_500.0, 0.8);
    let env = Adsr::new(0.002, 0.02, 0.8, 0.02);
    let beep = render(0.1, |t| {
        filter.process(osc.next(1_760.0)) * env.level(t, 0.06)
    });
    mix_into(&mut out, &beep, 0.12, 0.35);
    out
}

fn fuse_tick(rng: &mut Rng) -> Vec<f32> {
    let tone = 3_200.0 * rng.jitter(0.02);
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 3_000.0, 0.7);
    render(0.05, |t| {
        hp.process(noise.next()) * perc(t, 0.000_3, 0.012)
            + sine(t, tone) * perc(t, 0.0005, 0.02) * 0.3
    })
}

fn deploy(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for click in 0..6 {
        let grain = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            2_500.0 * rng.jitter(0.1),
            2.5,
            0.012,
        );
        mix_into(&mut out, &grain, click as f32 * 0.04, 0.7);
    }
    mix_into(&mut out, &thump(90.0, 55.0, 0.2, 2.0), 0.28, 1.0);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        400.0,
        0.7,
        0.15,
    );
    mix_into(&mut out, &body, 0.28, 0.6);
    mix_into(&mut out, &clank(rng, 620.0, 0.25), 0.3, 0.3);
    out
}

fn scanner_pulse(rng: &mut Rng) -> Vec<f32> {
    let start = 1_400.0 * rng.jitter(0.01);
    let mut osc = Osc::new(Wave::Sine);
    let mut shimmer = Fm::default();
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut sweep = Svf::new(FilterMode::BandPass, 300.0, 1.5);
    let mut echo = Delay::new(0.5).with_damping(0.4);
    render(1.6, |t| {
        let frequency = glide(t, start, start * 0.82, 0.25);
        let ping = (osc.next(frequency) + shimmer.next(frequency * 2.0, 1.5, 0.3) * 0.15)
            * perc(t, 0.002, 0.55);
        let whoosh = sweep.run(noise.next(), 300.0 + 2_700.0 * (t / 0.4).min(1.0), 1.5)
            * segments(t, &[(0.0, 0.0), (0.05, 0.5), (0.4, 0.0)]);
        let dry = ping + whoosh * 0.6;
        dry + echo.process(dry, 0.18, 0.45) * 0.5
    })
}

fn drill_bite(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_500.0 * rng.jitter(0.1),
        1.5,
        0.06,
    );
    for sample in &mut out {
        *sample = saturate(*sample * 2.0, 2.5);
    }
    mix_into(&mut out, &thump(180.0, 120.0, 0.05, 1.5), 0.0, 0.7);
    out
}

fn drill_glint(rng: &mut Rng) -> Vec<f32> {
    let notes = [2_637.0, 3_136.0, 3_520.0, 4_186.0];
    let note = notes[rng.index(notes.len())];
    let mut out = bell(note, 3.01, 0.8, 0.25);
    mix_into(&mut out, &bell(note * 1.5, 2.0, 0.5, 0.15), 0.03, 0.3);
    out
}

fn thrust_ignite(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 200.0, 0.9);
    let mut out = render(0.35, |t| {
        let cutoff = segments(t, &[(0.0, 200.0), (0.06, 2_500.0), (0.35, 400.0)]);
        filter.run(noise.next(), cutoff, 0.9) * perc(t, 0.01, 0.3)
    });
    mix_into(&mut out, &thump(75.0, 45.0, 0.2, 1.5), 0.0, 0.4);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 600.0, 1.0);
    let fwoomp = render(0.3, |t| {
        band.run(air.next(), 500.0 + 2_000.0 * (t / 0.3), 1.0) * perc(t, 0.02, 0.25)
    });
    mix_into(&mut out, &fwoomp, 0.0, 2.5);
    out
}

// ---------------------------------------------------------------------------------------------
// Impacts
// ---------------------------------------------------------------------------------------------

fn landing(rng: &mut Rng, tier: u8) -> Vec<f32> {
    let weight = f32::from(tier);
    let time = 0.15 + weight * 0.22;
    let mut out = thump(
        (95.0 - weight * 12.0) * rng.jitter(0.05),
        55.0 - weight * 9.0,
        time,
        1.0 + weight,
    );
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        500.0 + weight * 350.0,
        0.7,
        0.06 + weight * 0.1,
    );
    mix_into(&mut out, &body, 0.0, 0.7);
    mix_into(
        &mut out,
        &clank(rng, 700.0 * (1.0 + 0.1 * weight), 0.06 + weight * 0.08),
        0.0,
        0.3 + weight * 0.1,
    );
    let knock = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        900.0 + weight * 300.0,
        1.2,
        0.03 + weight * 0.02,
    );
    mix_into(&mut out, &knock, 0.0, 0.5);
    if tier == 2 {
        let mut crunch = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            2_000.0,
            1.0,
            0.12,
        );
        let mut crush = Decimator::default();
        for sample in &mut crunch {
            *sample = crush.process(saturate(*sample * 3.0, 3.0), 3.0);
        }
        mix_into(&mut out, &crunch, 0.0, 0.35);
        grains(rng, &mut out, 18, (0.05, 0.7), (900.0, 3_500.0), 0.25);
    }
    out
}

fn wall_bump(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(140.0 * rng.jitter(0.05), 90.0, 0.09, 1.5);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        700.0,
        0.9,
        0.04,
    );
    mix_into(&mut out, &body, 0.0, 0.6);
    mix_into(&mut out, &clank(rng, 1_300.0, 0.06), 0.0, 0.2);
    out
}

fn hull_hit(rng: &mut Rng) -> Vec<f32> {
    let mut crunch = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        1_100.0 * rng.jitter(0.1),
        0.8,
        0.12,
    );
    let mut crush = Decimator::default();
    for sample in &mut crunch {
        *sample = crush.process(saturate(*sample * 4.0, 4.0), 2.0) * 0.6;
    }
    let mut out = crunch;
    let resonances: [(f32, f32, f32); 3] = [
        (523.0 * rng.jitter(0.05), 0.5, 0.35),
        (1_287.0 * rng.jitter(0.05), 0.35, 0.25),
        (2_411.0 * rng.jitter(0.05), 0.25, 0.18),
    ];
    let resonance = render(0.4, |t| partials(t, &resonances));
    mix_into(&mut out, &resonance, 0.0, 0.7);
    mix_into(&mut out, &thump(110.0, 60.0, 0.15, 2.0), 0.0, 0.9);
    out
}

fn drill_crunch(rng: &mut Rng) -> Vec<f32> {
    let mut grit = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(1_500.0, 3_000.0),
        1.2,
        0.05,
    );
    let mut crush = Decimator::default();
    for sample in &mut grit {
        *sample = crush.process(saturate(*sample * 3.0, 4.0), 3.0);
    }
    mix_into(&mut grit, &thump(220.0, 160.0, 0.03, 1.0), 0.0, 0.5);
    grit
}

fn break_through(rng: &mut Rng, hard: bool) -> Vec<f32> {
    let mut out = if hard {
        burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            2_500.0,
            0.7,
            0.015,
        )
    } else {
        Vec::new()
    };
    let (start, end, time) = if hard {
        (90.0, 50.0, 0.2)
    } else {
        (120.0, 70.0, 0.15)
    };
    mix_into(
        &mut out,
        &thump(start * rng.jitter(0.05), end, time, 2.0),
        0.0,
        0.9,
    );
    let rubble = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        if hard { 900.0 } else { 700.0 },
        0.7,
        0.3,
    );
    mix_into(&mut out, &rubble, 0.0, 0.7);
    if hard {
        grains(rng, &mut out, 22, (0.01, 0.45), (1_800.0, 3_500.0), 0.45);
        for sample in &mut out {
            *sample = saturate(*sample, 1.5);
        }
    } else {
        grains(rng, &mut out, 22, (0.01, 0.35), (500.0, 1_800.0), 0.9);
    }
    out
}

fn vault_reset(rng: &mut Rng) -> Vec<f32> {
    let mut square = Osc::new(Wave::Square);
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 1_500.0, 1.5);
    let start = 330.0 * rng.jitter(0.01);
    let mut out = render(0.6, |t| {
        let frequency = glide(t, start, start / 3.0, 0.2);
        let raw = square.next(frequency) * 0.5 + saw.next(frequency * 1.01) * 0.5;
        saturate(filter.process(raw), 1.5) * perc(t, 0.005, 0.55)
    });
    mix_into(&mut out, &clank(rng, 260.0, 0.3), 0.45, 0.6);
    mix_into(&mut out, &thump(80.0, 50.0, 0.2, 1.5), 0.45, 0.7);
    out
}

// ---------------------------------------------------------------------------------------------
// Alarms
// ---------------------------------------------------------------------------------------------

fn beep(frequency: f32, gate: f32, width: f32, drive: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square);
    osc.width = width;
    let mut filter = Svf::new(FilterMode::LowPass, frequency * 3.5, 0.8);
    let env = Adsr::new(0.003, 0.05, 0.8, 0.02);
    render(gate + 0.05, |t| {
        saturate(filter.process(osc.next(frequency)), drive) * env.level(t, gate)
    })
}

fn fuel_alarm(_rng: &mut Rng, critical: bool) -> Vec<f32> {
    let mut out = Vec::new();
    if critical {
        for index in 0..3 {
            mix_into(
                &mut out,
                &beep(1_046.5, 0.08, 0.35, 1.8),
                index as f32 * 0.13,
                1.0,
            );
        }
    } else {
        mix_into(&mut out, &beep(880.0, 0.12, 0.25, 1.0), 0.0, 1.0);
        mix_into(&mut out, &beep(660.0, 0.12, 0.25, 1.0), 0.15, 1.0);
    }
    out
}

fn hull_alarm(_rng: &mut Rng, critical: bool) -> Vec<f32> {
    let gate = 0.35;
    let env = Adsr::new(0.01, 0.1, 0.8, 0.08);
    if critical {
        let mut a = Osc::new(Wave::Saw);
        let mut b = Osc::new(Wave::Saw);
        let mut filter = Svf::new(FilterMode::LowPass, 2_500.0, 2.0);
        render(gate + 0.1, |t| {
            let raw = (a.next(620.0) + b.next(655.0)) * 0.5;
            let tremolo = 0.65 + 0.35 * sine(t, 18.0);
            saturate(filter.process(raw), 3.0) * tremolo * env.level(t, gate)
        })
    } else {
        let mut tri = Osc::new(Wave::Triangle);
        let mut sub = Osc::new(Wave::Sine);
        render(gate + 0.1, |t| {
            let frequency = 520.0 + 40.0 * sine(t, 14.0);
            (tri.next(frequency) * 0.7 + sub.next(frequency * 0.5) * 0.4) * env.level(t, gate)
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Set-pieces
// ---------------------------------------------------------------------------------------------

fn explosion(rng: &mut Rng) -> Vec<f32> {
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 1_500.0, 0.7);
    let mut boom = Osc::new(Wave::Sine);
    let mut body_noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut body_filter = Svf::new(FilterMode::LowPass, 2_500.0, 0.8);
    let mut mid_noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut mid_filter = Svf::new(FilterMode::BandPass, 600.0, 0.7);
    let low = 60.0 * rng.jitter(0.08);
    let body_time = 1.8 * rng.jitter(0.1);
    let mut out = render(2.2, |t| {
        let crack = crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.035);
        let sub = saturate(
            boom.next(glide(t, low, low * 0.47, 0.15)) * perc(t, 0.002, 1.4),
            2.5,
        );
        let body = body_filter.run(body_noise.next(), glide(t, 2_500.0, 180.0, 0.25), 0.8)
            * perc(t, 0.003, body_time);
        let mid = mid_filter.process(mid_noise.next()) * perc(t, 0.002, 0.9);
        crack * 0.9 + sub * 0.7 + body * 1.0 + mid * 2.2
    });
    grains(rng, &mut out, 60, (0.1, 2.6), (800.0, 4_500.0), 0.45);
    for time in scatter(rng, 8, 0.2, 1.6, 1.5) {
        let chunk = thump(rng.range(150.0, 250.0), 110.0, 0.05, 1.5);
        mix_into(&mut out, &chunk, time, rng.range(0.1, 0.25));
    }
    for sample in &mut out {
        *sample = saturate(*sample, 1.5);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Explosives
// ---------------------------------------------------------------------------------------------

/// A lobbed charge: a short airy swish that rises then falls.
fn throw_whoosh(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 600.0, 1.8);
    let peak = 2_200.0 * rng.jitter(0.1);
    let mut out = render(0.32, |t| {
        let cutoff = 500.0 + peak * segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.32, 0.2)]);
        filter.run(noise.next(), cutoff, 1.8) * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.32, 0.0)])
    });
    mix_into(&mut out, &clank(rng, 900.0, 0.05), 0.0, 0.25);
    out
}

/// A charge slapping onto rock: a wet thunk and a little arming chirp.
fn charge_stick(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(240.0 * rng.jitter(0.05), 140.0, 0.07, 2.0);
    let squelch = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        900.0,
        1.2,
        0.06,
    );
    mix_into(&mut out, &squelch, 0.0, 0.7);
    let mut osc = Osc::new(Wave::Square);
    osc.width = 0.3;
    let chirp = render(0.05, |t| {
        osc.next(glide(t, 2_400.0, 3_000.0, 0.02)) * perc(t, 0.001, 0.04)
    });
    mix_into(&mut out, &chirp, 0.09, 0.18);
    out
}

/// The remote detonator: a switch click and two rising confirm beeps.
fn detonator(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 1_400.0, 0.03);
    let mut osc = Osc::new(Wave::Square);
    osc.width = 0.5;
    let mut filter = Svf::new(FilterMode::LowPass, 4_000.0, 0.8);
    let env = Adsr::new(0.002, 0.02, 0.7, 0.02);
    for (index, note) in [1_318.5, 1_760.0].into_iter().enumerate() {
        let beep = render(0.08, |t| {
            filter.process(osc.next(note)) * env.level(t, 0.05)
        });
        mix_into(&mut out, &beep, 0.05 + index as f32 * 0.07, 0.35);
    }
    out
}

/// A bomblet or a cluster bomb's own small pop: a tight crack and a short boom.
fn small_blast(rng: &mut Rng) -> Vec<f32> {
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 2_000.0, 0.7);
    let mut body_noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut body_filter = Svf::new(FilterMode::LowPass, 1_800.0, 0.8);
    let mut boom = Osc::new(Wave::Sine);
    let low = 110.0 * rng.jitter(0.1);
    let mut out = render(0.6, |t| {
        let crack = crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.02);
        let body = body_filter.run(body_noise.next(), glide(t, 1_800.0, 250.0, 0.08), 0.8)
            * perc(t, 0.002, 0.35);
        let sub = saturate(
            boom.next(glide(t, low, low * 0.6, 0.05)) * perc(t, 0.001, 0.25),
            2.0,
        );
        crack * 0.8 + body * 1.2 + sub * 0.6
    });
    grains(rng, &mut out, 14, (0.03, 0.5), (1_000.0, 4_000.0), 0.3);
    out
}

/// A cluster bomb bursting: a pop and the tinny rattle of bomblets scattering.
fn cluster_burst(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(180.0, 90.0, 0.12, 2.0);
    for time in scatter(rng, 6, 0.02, 0.22, 1.2) {
        let base = rng.range(1_800.0, 2_600.0);
        let tink = clank(rng, base, 0.06);
        mix_into(&mut out, &tink, time, rng.range(0.25, 0.4));
    }
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        3_000.0,
        1.0,
        0.15,
    );
    mix_into(&mut out, &hiss, 0.0, 0.4);
    out
}

/// A shaped charge: a hard, bright crack and a jet roaring away down the tunnel.
fn shaped_blast(rng: &mut Rng) -> Vec<f32> {
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 1_200.0, 0.7);
    let mut jet_noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut jet_filter = Svf::new(FilterMode::BandPass, 3_000.0, 1.2);
    let mut boom = Osc::new(Wave::Sine);
    let low = 75.0 * rng.jitter(0.06);
    let mut out = render(1.5, |t| {
        let crack = crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.05);
        let jet = jet_filter.run(jet_noise.next(), glide(t, 4_000.0, 500.0, 0.35), 1.2)
            * segments(t, &[(0.0, 0.0), (0.02, 1.0), (0.5, 0.45), (1.3, 0.0)]);
        let sub = saturate(
            boom.next(glide(t, low, low * 0.5, 0.12)) * perc(t, 0.002, 0.8),
            2.5,
        );
        crack * 1.1 + jet * 1.6 + sub * 0.6
    });
    for time in scatter(rng, 10, 0.08, 1.1, 1.3) {
        let chunk = thump(rng.range(160.0, 260.0), 110.0, 0.05, 1.5);
        mix_into(&mut out, &chunk, time, rng.range(0.1, 0.2));
    }
    for sample in &mut out {
        *sample = saturate(*sample, 1.4);
    }
    out
}

/// A mining charge: a deep, rolling earth-moving boom with a long rumble of settling rock.
fn deep_blast(rng: &mut Rng) -> Vec<f32> {
    let mut boom = Osc::new(Wave::Sine);
    let mut body_noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut body_filter = Svf::new(FilterMode::LowPass, 900.0, 0.9);
    let low = 48.0 * rng.jitter(0.06);
    let mut out = render(2.6, |t| {
        let sub = saturate(
            boom.next(glide(t, low, low * 0.55, 0.25)) * perc(t, 0.004, 1.8),
            3.0,
        );
        let body = body_filter.run(body_noise.next(), glide(t, 1_400.0, 120.0, 0.4), 0.9)
            * perc(t, 0.004, 2.2);
        sub * 0.9 + body * 1.6
    });
    grains(rng, &mut out, 70, (0.15, 2.4), (300.0, 1_800.0), 0.4);
    for sample in &mut out {
        *sample = saturate(*sample, 1.6);
    }
    out
}

/// An incendiary going up: a soft thump and a roaring, rising rush of flame.
fn fire_whoosh(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 400.0, 0.9);
    let mut out = render(1.0, |t| {
        filter.run(noise.next(), glide(t, 300.0, 2_600.0, 0.15), 0.9)
            * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.4, 0.6), (1.0, 0.0)])
    });
    mix_into(&mut out, &thump(120.0, 70.0, 0.15, 1.8), 0.0, 0.7);
    scatter_grains(
        rng,
        &mut out,
        24,
        (0.05, 0.9),
        (1_500.0, 5_000.0),
        0.25,
        true,
    );
    out
}

/// Fire spreading: dry pops and crackles over a breath of flame.
fn fire_crackle(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 1_200.0, 0.8);
    let mut out = render(0.45, |t| {
        filter.process(noise.next()) * segments(t, &[(0.0, 0.0), (0.05, 0.4), (0.45, 0.0)])
    });
    for time in scatter(rng, 9, 0.0, 0.4, 1.0) {
        let pop = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(1_200.0, 3_200.0),
            1.2,
            0.008,
        );
        mix_into(&mut out, &pop, time, rng.range(0.4, 0.9));
    }
    out
}

/// An EMP: a descending electric zap with a buzzing, ringing tail.
fn emp_pulse(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut fm = Fm::default();
    let mut crush = Decimator::default();
    let mut filter = Svf::new(FilterMode::LowPass, 6_000.0, 1.4);
    let start = 1_800.0 * rng.jitter(0.04);
    let mut echo = Delay::new(0.4).with_damping(0.3);
    render(1.1, |t| {
        let frequency = glide(t, start, 90.0, 0.18);
        let zap = crush.process(saw.next(frequency), 6.0) * perc(t, 0.001, 0.45);
        let tail = fm.next(frequency * 0.5, 1.41, 2.0 * decay(t, 0.3)) * perc(t, 0.005, 0.7);
        let dry = filter.run(zap * 0.6 + tail * 0.5, glide(t, 8_000.0, 900.0, 0.3), 1.4);
        dry + echo.process(dry, 0.11, 0.4) * 0.4
    })
}

/// A smoke charge bursting: a soft canister pop, then a long breathy exhale of smoke.
fn smoke_burst(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(150.0 * rng.jitter(0.06), 70.0, 0.1, 1.5);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 0.7);
    let swirl = rng.range(2.0, 3.5);
    let exhale = render(1.4, |t| {
        let cutoff = 700.0 + 900.0 * decay(t, 0.5) + 200.0 * sine(t, swirl).abs();
        filter.run(noise.next(), cutoff, 0.7)
            * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.5, 0.6), (1.4, 0.0)])
    });
    mix_into(&mut out, &exhale, 0.02, 0.9);
    out
}

/// A scare charge: a deafening crack with a ringing whine and a short boom underneath, all bark
/// and no bite.
fn scare_bang(rng: &mut Rng) -> Vec<f32> {
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 1_500.0, 0.7);
    let mut whine_osc = Osc::new(Wave::Sine);
    let pitch = 3_300.0 * rng.jitter(0.05);
    let mut boom = Osc::new(Wave::Sine);
    let mut echo = Delay::new(0.5).with_damping(0.4);
    render(0.9, |t| {
        let crack = crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.05);
        let whine = whine_osc.next(pitch) * perc(t, 0.002, 0.6) * 0.18;
        let sub = saturate(
            boom.next(glide(t, 160.0, 60.0, 0.06)) * perc(t, 0.001, 0.18),
            3.0,
        );
        let dry = crack * 1.2 + whine + sub * 0.7;
        dry + echo.process(dry, 0.13, 0.35) * 0.35
    })
}

/// A rivet gun shot: a tight pneumatic snap with a metallic tick.
fn rivet_shot(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_600.0 * rng.jitter(0.08),
        1.4,
        0.05,
    );
    mix_into(&mut out, &clank(rng, 1_700.0, 0.04), 0.0, 0.5);
    mix_into(&mut out, &thump(260.0, 140.0, 0.04, 1.5), 0.0, 0.4);
    out
}

/// A scatter blaster: a deep boom with a spray of pellets rattling off.
fn scatter_shot(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        1_800.0,
        0.7,
        0.05,
    );
    let blast = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        4_500.0,
        0.8,
        0.16,
    );
    mix_into(&mut out, &blast, 0.0, 1.0);
    mix_into(
        &mut out,
        &thump(170.0 * rng.jitter(0.05), 80.0, 0.14, 2.0),
        0.0,
        0.45,
    );
    grains(rng, &mut out, 12, (0.01, 0.2), (2_500.0, 6_000.0), 0.35);
    out
}

/// One pulse of the plasma cutter: a short electric sizzle (back to back they buzz).
fn cutter_zap(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut fm = Fm::default();
    let mut filter = Svf::new(FilterMode::BandPass, 1_800.0, 1.2);
    let base = 210.0 * rng.jitter(0.04);
    render(0.13, |t| {
        let buzz = saw.next(base) * 0.6 + fm.next(base * 4.0, 2.01, 3.0) * 0.4;
        filter.run(buzz, 1_400.0 + 900.0 * sine(t, 31.0).abs(), 1.2)
            * segments(t, &[(0.0, 0.0), (0.01, 1.0), (0.1, 0.8), (0.13, 0.0)])
    })
}

/// A rocket leaving the tube: a thump and a rising, roaring whoosh.
fn rocket_launch(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(110.0, 70.0, 0.12, 2.0);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 800.0, 1.0);
    let roar = render(0.5, |t| {
        filter.run(noise.next(), glide(t, 600.0, 2_400.0, 0.25), 1.0)
            * segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.3, 0.7), (0.5, 0.0)])
    });
    mix_into(&mut out, &roar, 0.02, 0.9);
    out
}

/// A harpoon: a taut twang and the whizz of line paying out.
fn harpoon_shot(rng: &mut Rng) -> Vec<f32> {
    let pitch = 160.0 * rng.jitter(0.05);
    let bank = [
        (pitch, 1.0, 0.25),
        (pitch * 2.01, 0.5, 0.18),
        (pitch * 3.03, 0.3, 0.1),
    ];
    let mut out = render(0.35, |t| partials(t, &bank) * perc(t, 0.001, 0.3));
    let whizz = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.8,
        0.25,
    );
    mix_into(&mut out, &whizz, 0.02, 0.3);
    out
}

/// A railgun slug: a rising capacitor whine, a hard crack and a long metallic ring.
fn railgun_shot(rng: &mut Rng) -> Vec<f32> {
    let mut whine = Osc::new(Wave::Sine);
    let mut charge = render(0.07, |t| {
        whine.next(glide(t, 500.0, 3_200.0, 0.06))
            * segments(t, &[(0.0, 0.0), (0.06, 0.6), (0.07, 0.0)])
    });
    let mut crack_noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut crack_filter = Svf::new(FilterMode::HighPass, 1_200.0, 0.7);
    let ring_pitch = 1_100.0 * rng.jitter(0.04);
    let bank = [
        (ring_pitch, 0.5, 0.5),
        (ring_pitch * 2.76, 0.25, 0.3),
        (ring_pitch * 5.4, 0.12, 0.2),
    ];
    let crack = render(0.7, |t| {
        crack_filter.process(crack_noise.next()) * perc(t, 0.000_2, 0.04) + partials(t, &bank)
    });
    mix_into(&mut charge, &crack, 0.065, 1.0);
    mix_into(&mut charge, &thump(180.0, 60.0, 0.15, 3.0), 0.065, 0.6);
    charge
}

/// The empty click of a dry trigger.
fn weapon_dry(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 2_300.0, 0.025);
    mix_into(&mut out, &clank(rng, 1_500.0, 0.02), 0.05, 0.5);
    out
}

/// A weapon locking onto the hardpoint: two mechanical clacks and a servo chirp.
fn weapon_mount(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 900.0, 0.05);
    mix_into(&mut out, &clank(rng, 1_250.0, 0.05), 0.09, 0.8);
    let mut osc = Osc::new(Wave::Square);
    osc.width = 0.4;
    let chirp = render(0.06, |t| {
        osc.next(glide(t, 900.0, 1_500.0, 0.05)) * perc(t, 0.002, 0.05)
    });
    mix_into(&mut out, &chirp, 0.16, 0.15);
    out
}

/// A shot striking rock: a dusty chip.
fn impact_rock(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_400.0 * rng.jitter(0.15),
        1.0,
        0.07,
    );
    mix_into(&mut out, &clank(rng, 700.0, 0.03), 0.0, 0.3);
    grains(rng, &mut out, 4, (0.01, 0.1), (2_000.0, 4_500.0), 0.2);
    out
}

/// A shot striking a target: a solid thwack.
fn impact_hit(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(320.0 * rng.jitter(0.08), 150.0, 0.08, 2.5);
    let slap = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_800.0,
        1.0,
        0.05,
    );
    mix_into(&mut out, &slap, 0.0, 0.7);
    out
}

/// A target breaking apart: a crunch and a clatter of parts.
fn target_destroyed(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(200.0, 70.0, 0.2, 2.5);
    let crunch = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        2_000.0,
        0.8,
        0.2,
    );
    mix_into(&mut out, &crunch, 0.0, 0.9);
    for time in scatter(rng, 5, 0.05, 0.45, 1.3) {
        let base = rng.range(800.0, 1_800.0);
        let part = clank(rng, base, 0.06);
        mix_into(&mut out, &part, time, rng.range(0.2, 0.35));
    }
    out
}

/// A flare striking: a scratch, a pop and a sputtering hiss.
fn flare_ignite(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_200.0, 0.9);
    let sputter_rate = rng.range(18.0, 26.0);
    let mut out = render(0.9, |t| {
        let flutter = 0.6 + 0.4 * sine(t, sputter_rate).abs();
        filter.process(noise.next())
            * segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.25, 0.5), (0.9, 0.0)])
            * flutter
    });
    mix_into(&mut out, &thump(300.0, 180.0, 0.05, 2.0), 0.03, 0.5);
    out
}

fn poi_discovered(rng: &mut Rng) -> Vec<f32> {
    let gate = 1.0;
    let env = Adsr::new(0.25, 0.5, 0.7, 0.8);
    let notes = [220.0, 329.63, 493.88];
    let mut voices: Vec<(Osc, Osc)> = notes
        .iter()
        .map(|_| {
            (
                Osc::new(Wave::Saw).with_phase(rng.unit()),
                Osc::new(Wave::Saw).with_phase(rng.unit()),
            )
        })
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 1_200.0, 0.9);
    let mut out = render(gate + 0.9, |t| {
        let raw: f32 = voices
            .iter_mut()
            .zip(notes)
            .enumerate()
            .map(|(index, ((a, b), note))| {
                let enter = index as f32 * 0.12;
                let level = env.level(t - enter, gate);
                (a.next(note * 0.997) + b.next(note * 1.003)) * level
            })
            .sum::<f32>()
            / 4.0;
        filter.run(raw, 700.0 + 600.0 * sine(t, 0.7).abs(), 0.9)
    });
    mix_into(&mut out, &bell(880.0, 3.5, 2.0, 1.4), 0.0, 0.35);
    mix_into(&mut out, &bell(1_318.5, 3.5, 1.5, 1.2), 0.18, 0.28);
    let mut echo = Delay::new(0.5).with_damping(0.45);
    out.resize(out.len() + samples_for(0.6), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.3, 0.35) * 0.35;
    }
    out
}

/// Finding a cave: a soft low drone opening into a glassy two-note chime that rings out in a
/// long cavern echo.
fn cave_discovered(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 0.7);
    let mut out = render(1.6, |t| {
        filter.process(drone.next(110.0 * (1.0 + 0.004 * sine(t, 3.0))))
            * segments(t, &[(0.0, 0.0), (0.3, 0.5), (1.0, 0.35), (1.6, 0.0)])
    });
    let root = if rng.unit() < 0.5 { 659.25 } else { 587.33 };
    mix_into(&mut out, &bell(root, 2.0, 1.2, 1.6), 0.08, 0.45);
    mix_into(&mut out, &bell(root * 1.5, 2.0, 1.0, 1.4), 0.34, 0.35);
    mix_into(&mut out, &bell(root * 2.0, 3.0, 0.8, 1.0), 0.6, 0.18);
    let mut echo = Delay::new(0.7).with_damping(0.55);
    out.resize(out.len() + samples_for(1.0), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.42, 0.45) * 0.45;
    }
    out
}

/// Arriving in a village: a warm plucked major arpeggio over a soft reed pad, with a
/// welcoming bell on top (friendlier and rounder than the POI discovery).
fn village_arrival(rng: &mut Rng) -> Vec<f32> {
    let gate = 0.9;
    let env = Adsr::new(0.12, 0.4, 0.6, 0.9);
    let pad_notes = [196.0, 246.94, 293.66];
    let mut pads: Vec<Osc> = pad_notes
        .iter()
        .map(|_| Osc::new(Wave::Triangle).with_phase(rng.unit()))
        .collect();
    let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 0.7);
    let mut out = render(gate + 1.0, |t| {
        let raw: f32 = pads
            .iter_mut()
            .zip(pad_notes)
            .map(|(osc, note)| osc.next(note) * env.level(t, gate))
            .sum::<f32>()
            / 3.0;
        filter.run(raw, 900.0 + 300.0 * sine(t, 0.5).abs(), 0.7)
    });
    for (index, note) in [392.0_f32, 493.88, 587.33, 783.99].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four notes")]
        let offset = index as f32 * 0.11;
        mix_into(&mut out, &bell(note, 2.0, 1.2, 0.9), offset, 0.32);
    }
    mix_into(&mut out, &bell(1_174.7, 3.5, 1.4, 1.6), 0.5, 0.22);
    let mut echo = Delay::new(0.32).with_damping(0.5);
    out.resize(out.len() + samples_for(0.5), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.25, 0.3) * 0.3;
    }
    out
}

/// A stalactite breaking off: a sharp stony crack, grit trickling and a short ring.
fn stalactite_crack(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        2_400.0,
        0.8,
        0.05,
    );
    mix_into(&mut out, &clank(rng, 620.0, 0.18), 0.0, 0.5);
    grains(rng, &mut out, 14, (0.04, 0.5), (1_500.0, 4_000.0), 0.4);
    mix_into(&mut out, &thump(180.0, 90.0, 0.12, 1.4), 0.01, 0.4);
    out
}

/// A cave cache opening: an old lid grinding, then coins and a bright glint.
fn cache_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        700.0,
        1.4,
        0.25,
    );
    mix_into(
        &mut out,
        &thump(110.0 * rng.jitter(0.05), 70.0, 0.15, 1.5),
        0.2,
        0.8,
    );
    mix_into(&mut out, &coins(rng), 0.24, 0.6);
    mix_into(&mut out, &bell(1_760.0, 2.0, 1.0, 0.3), 0.3, 0.3);
    mix_into(&mut out, &bell(2_349.3, 2.0, 1.0, 0.35), 0.38, 0.25);
    out
}

/// A rig springing off a giant mushroom cap: a soft rubbery thud and a wobbling rising boing.
fn mushroom_bounce(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(120.0 * rng.jitter(0.05), 70.0, 0.08, 1.2);
    let mut osc = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let base = 180.0 * rng.jitter(0.06);
    let boing = render(0.42, |t| {
        let frequency = base * (1.0 + 1.6 * t) * (1.0 + 0.06 * sine(t, 11.0));
        osc.next(frequency) * perc(t, 0.004, 0.32)
    });
    mix_into(&mut out, &boing, 0.01, 0.55);
    out
}

/// A shrine blessing: three high, slow temple bells, shimmering into a long echo.
fn shrine_chime(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(1.4)];
    let detune = 1.0 + (rng.unit() - 0.5) * 0.004;
    for (index, (note, gain)) in [(1_046.5_f32, 0.4), (1_318.5, 0.32), (1_568.0, 0.28)]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "three notes")]
        let offset = index as f32 * 0.16;
        mix_into(&mut out, &bell(note * detune, 3.5, 1.8, 1.0), offset, gain);
    }
    let mut echo = Delay::new(0.27).with_damping(0.4);
    out.resize(out.len() + samples_for(0.3), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.3, 0.35) * 0.35;
    }
    out
}

fn vault_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(52.0, 35.0, 0.8, 2.0);
    let rumble = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        300.0,
        0.7,
        0.5,
    );
    mix_into(&mut out, &rumble, 0.0, 0.8);
    let arpeggio = [261.63, 329.63, 392.0];
    for (index, note) in arpeggio.into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 0.12, 0.8),
            0.15 + index as f32 * 0.15,
            0.35,
        );
    }
    for note in [261.63, 523.25, 659.25, 783.99] {
        mix_into(&mut out, &brass(rng, note, 1.5, 1.0), 0.6, 0.25);
    }
    for (index, note) in [2_093.0, 2_637.0, 3_136.0, 4_186.0].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.0, 0.6),
            0.6 + index as f32 * 0.06,
            0.08,
        );
    }
    for sample in &mut out {
        *sample = saturate(*sample, 1.3);
    }
    out
}

fn milestone(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for note in [587.33, 739.99, 880.0] {
        mix_into(&mut out, &brass(rng, note, 0.6, 0.7), 0.0, 0.3);
    }
    for (index, note) in [1_174.7, 1_480.0, 1_760.0, 2_349.3].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.2, 0.5),
            index as f32 * 0.07,
            0.18,
        );
    }
    out
}

/// A person greeting you: two soft voiced syllables ("mm-hm") through a vowel-like band, the
/// pitch rolled per variation so a street of people never sounds like one voice.
fn person_greet(rng: &mut Rng) -> Vec<f32> {
    let base = 150.0 + rng.unit() * 120.0;
    let rise = if rng.unit() < 0.5 { 1.12 } else { 1.26 };
    let mut voice = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut formant = Svf::new(FilterMode::BandPass, 700.0, 2.5);
    let mut warmth = Svf::new(FilterMode::LowPass, 1_800.0, 0.7);
    render(0.42, |t| {
        let (note, env) = if t < 0.17 {
            (
                base,
                segments(t, &[(0.0, 0.0), (0.03, 1.0), (0.13, 0.8), (0.17, 0.0)]),
            )
        } else {
            (
                base * rise,
                segments(t, &[(0.19, 0.0), (0.23, 1.0), (0.34, 0.7), (0.42, 0.0)]),
            )
        };
        let vibrato = 1.0 + 0.012 * sine(t, 5.5);
        let raw = voice.next(note * vibrato);
        let vowel = 650.0 + 250.0 * (t / 0.42);
        warmth.run(formant.run(raw, vowel, 2.5), 1_800.0, 0.7) * env * 0.9
    })
}

/// A gift handed over: a little rising sparkle of bells over a paper rustle.
fn gift_given(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::HighPass,
        3_000.0,
        0.7,
        0.12,
    );
    for (index, note) in [783.99_f32, 987.77, 1_174.7, 1_567.98]
        .into_iter()
        .enumerate()
    {
        let offset = 0.05 + index as f32 * 0.07;
        mix_into(
            &mut out,
            &bell(note * rng.jitter(0.003), 3.0, 1.2, 0.6),
            offset,
            0.3,
        );
    }
    out
}

/// Someone warmed to you: a soft major third rising into a warm held chord.
fn affinity_up(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(1.2)];
    let detune = rng.jitter(0.003);
    for (index, (note, gain)) in [(523.25_f32, 0.32), (659.25, 0.3), (783.99, 0.26)]
        .into_iter()
        .enumerate()
    {
        let offset = index as f32 * 0.09;
        mix_into(&mut out, &bell(note * detune, 2.0, 0.8, 0.9), offset, gain);
    }
    let mut pad = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let env = Adsr::new(0.15, 0.3, 0.5, 0.5);
    let hum = render(1.0, |t| pad.next(261.63) * env.level(t, 0.5) * 0.18);
    mix_into(&mut out, &hum, 0.1, 1.0);
    out
}

/// A rumour: a hushed whisper of noise and two low, curious notes.
fn rumour_heard(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        2_200.0,
        1.4,
        0.35,
    );
    for sample in &mut out {
        *sample *= 0.5;
    }
    mix_into(&mut out, &bell(293.66, 1.5, 1.4, 0.8), 0.12, 0.35);
    mix_into(&mut out, &bell(349.23, 1.5, 1.4, 1.0), 0.34, 0.32);
    out
}

/// A deed or a contract signed: two quick quill strokes, then a rubber stamp thumped down and
/// a little bell from the clerk's desk.
fn deed_stamp(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        4_200.0 * rng.jitter(0.05),
        2.2,
        0.11,
    );
    for sample in &mut out {
        *sample *= 0.3;
    }
    let stroke = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        5_100.0,
        2.2,
        0.09,
    );
    mix_into(&mut out, &stroke, 0.13, 0.26);
    mix_into(
        &mut out,
        &thump(150.0 * rng.jitter(0.04), 70.0, 0.12, 2.2),
        0.3,
        0.85,
    );
    let pad = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        900.0,
        0.8,
        0.07,
    );
    mix_into(&mut out, &pad, 0.3, 0.45);
    mix_into(&mut out, &bell(1_046.5, 2.0, 0.7, 0.45), 0.42, 0.1);
    out
}

/// Builders at work: three hammer knocks on timber and a stroke of the saw.
fn builders_hammer(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for index in 0..3 {
        let at = index as f32 * 0.17 + rng.range(0.0, 0.02);
        let pitch = 1_250.0 * rng.jitter(0.06);
        mix_into(&mut out, &clank(rng, pitch, 0.05), at, 0.3);
        mix_into(&mut out, &thump(230.0, 120.0, 0.06, 1.8), at, 0.6);
    }
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_600.0, 1.3);
    let saw = render(0.42, |t| {
        filter.process(noise.next()) * (0.5 + 0.5 * sine(t, 7.5)).max(0.0) * perc(t, 0.03, 0.42)
    });
    mix_into(&mut out, &saw, 0.56, 0.32);
    out
}

/// The keys handed over: a two-tone doorbell, then a warm brass phrase rising to the octave and
/// a bright bell on top, like a front door opening on a new home.
fn home_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(783.99 * rng.jitter(0.003), 2.0, 1.2, 0.8);
    mix_into(&mut out, &bell(622.25, 2.0, 1.2, 0.9), 0.22, 0.85);
    for (index, note) in [261.63, 329.63, 392.0, 523.25].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 0.5 - index as f32 * 0.07, 0.6),
            0.46 + index as f32 * 0.12,
            0.2,
        );
    }
    mix_into(&mut out, &bell(2_093.0, 2.0, 0.9, 0.6), 0.9, 0.14);
    out
}

/// A building site needs you: knock-knock on a door frame and a short whistle that rises like
/// a question.
fn site_knock(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(190.0 * rng.jitter(0.04), 110.0, 0.07, 2.0);
    mix_into(&mut out, &clank(rng, 900.0, 0.04), 0.0, 0.25);
    mix_into(&mut out, &thump(200.0, 115.0, 0.07, 2.0), 0.14, 1.0);
    mix_into(&mut out, &clank(rng, 950.0, 0.04), 0.14, 0.25);
    let mut osc = Osc::new(Wave::Sine);
    let whistle = render(0.36, |t| {
        osc.next(glide(t, 1_300.0, 1_850.0, 0.12)) * perc(t, 0.03, 0.36) * 0.5
    });
    mix_into(&mut out, &whistle, 0.36, 0.5);
    out
}

/// A piece of furniture set down: a soft wooden thud with a creak of joinery.
fn furniture_thunk(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(120.0 * rng.jitter(0.06), 65.0, 0.1, 1.6);
    let pitch = 620.0 * rng.jitter(0.05);
    mix_into(&mut out, &clank(rng, pitch, 0.06), 0.004, 0.22);
    let creak = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_400.0,
        3.0,
        0.12,
    );
    mix_into(&mut out, &creak, 0.06, 0.18);
    out
}

/// A crewmate back in a resumed world ("Welcome back"): a friendly door chime (two bells, low
/// then high) under a warm rising brass third, like the town's bell ringing someone in.
fn crew_return(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(659.25 * rng.jitter(0.004), 2.0, 1.1, 0.7);
    mix_into(&mut out, &bell(987.77, 2.0, 1.0, 0.8), 0.16, 0.9);
    for (index, note) in [392.0, 493.88, 587.33].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 0.42 - index as f32 * 0.08, 0.55),
            0.3 + index as f32 * 0.09,
            0.22,
        );
    }
    mix_into(&mut out, &bell(1_975.5, 2.0, 0.9, 0.45), 0.56, 0.16);
    out
}

fn rescue(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 4_000.0, 0.7);
    let mut out = render(1.4, |t| {
        let frequency = 200.0 * 8.0_f32.powf((t / 1.2).min(1.0));
        let tremolo = 0.6 + 0.4 * sine(t, 12.0);
        let env = segments(t, &[(0.0, 0.0), (0.1, 0.8), (1.1, 1.0), (1.4, 0.0)]);
        let beam = fm.next(frequency, 1.5, 3.0 * (1.0 - t / 1.4)) * tremolo * env;
        let sparkle = hp.process(noise.next()) * tremolo * env * 0.25;
        beam * 0.6 + sparkle
    });
    for note in [659.25, 987.77] {
        mix_into(&mut out, &brass(rng, note, 0.3, 0.6), 1.15, 0.25);
    }
    mix_into(&mut out, &bell(1_975.5, 2.0, 1.0, 0.4), 1.15, 0.2);
    out
}

/// A garage paint job: the rattle of a shaken spray can, two hissing passes of paint and a
/// bright "fresh coat" chime.
fn spray_paint(rng: &mut Rng) -> Vec<f32> {
    // The cap popping off.
    let mut out = clank(rng, 620.0, 0.08);
    // The mixing ball rattling inside the can.
    for index in 0..5_u8 {
        let rattle = burst(
            rng.next_u64(),
            NoiseColor::Pink,
            FilterMode::BandPass,
            2_400.0 * rng.jitter(0.12),
            3.0,
            0.03,
        );
        mix_into(&mut out, &rattle, 0.08 + f32::from(index) * 0.055, 0.6);
    }
    // Two passes of the nozzle: a breathy band of noise that swells and sweeps.
    for (start, length) in [(0.4, 0.34), (0.8, 0.42)] {
        let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
        let mut filter = Svf::new(FilterMode::BandPass, 2_600.0, 0.8);
        let hiss = render(length, |t| {
            let cutoff = 2_200.0 + 1_400.0 * sine(t, 3.0).abs();
            filter.run(noise.next(), cutoff, 0.8)
                * segments(
                    t,
                    &[(0.0, 0.0), (0.03, 0.9), (length * 0.7, 0.7), (length, 0.0)],
                )
        });
        mix_into(&mut out, &hiss, start, 0.55);
    }
    mix_into(&mut out, &bell(1_046.5, 2.0, 1.0, 0.35), 1.24, 0.2);
    mix_into(&mut out, &bell(1_567.98, 2.0, 1.0, 0.4), 1.3, 0.18);
    out
}

// ---------------------------------------------------------------------------------------------
// ---------------------------------------------------------------------------------------------
// The pilot on foot (Part II F5)
// ---------------------------------------------------------------------------------------------

/// The canopy hatch: a latch clank and a pneumatic hiss. Opening, the latch pops and the hiss
/// lifts the canopy (rising); closing, the hiss settles (falling) and the latch bites shut.
fn hatch(rng: &mut Rng, opening: bool) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 3_000.0, 0.9);
    let length = 0.32;
    let hiss = render(length, |t| {
        let progress = t / length;
        let cutoff = if opening {
            2_000.0 + 3_200.0 * progress
        } else {
            5_200.0 - 3_200.0 * progress
        };
        filter.run(noise.next(), cutoff, 0.9)
            * segments(
                t,
                &[(0.0, 0.0), (0.03, 1.0), (length * 0.6, 0.6), (length, 0.0)],
            )
    });
    let latch = clank(rng, if opening { 880.0 } else { 640.0 }, 0.08);
    let thunk = thump(190.0, 120.0, 0.07, 1.5);
    let mut out = Vec::new();
    if opening {
        mix_into(&mut out, &latch, 0.0, 0.6);
        mix_into(&mut out, &thunk, 0.0, 0.45);
        mix_into(&mut out, &hiss, 0.05, 0.55);
    } else {
        mix_into(&mut out, &hiss, 0.0, 0.5);
        mix_into(&mut out, &thunk, 0.24, 0.8);
        mix_into(&mut out, &latch, 0.25, 0.7);
    }
    out
}

/// A jump: a soft scuff of boots leaving the ground and a short airy whoosh rising after it.
fn pilot_jump(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        900.0 * rng.jitter(0.1),
        1.0,
        0.04,
    );
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 1_200.0, 1.2);
    let whoosh = render(0.18, |t| {
        filter.run(noise.next(), 900.0 + 2_600.0 * t / 0.18, 1.2)
            * segments(t, &[(0.0, 0.0), (0.05, 0.8), (0.18, 0.0)])
    });
    mix_into(&mut out, &whoosh, 0.01, 0.5);
    out
}

/// Boots landing: a dull thud and a little grit; the hard landing adds a heavy knock, a
/// crunch of gear and a winded grunt below it.
fn pilot_land(rng: &mut Rng, hard: bool) -> Vec<f32> {
    let weight = if hard { 1.0 } else { 0.0 };
    let mut out = thump(
        (120.0 - weight * 30.0) * rng.jitter(0.06),
        70.0 - weight * 20.0,
        0.09 + weight * 0.12,
        1.2 + weight,
    );
    grains(
        rng,
        &mut out,
        4 + 6 * usize::from(hard),
        (0.0, 0.12),
        (1_200.0, 3_500.0),
        0.25,
    );
    if hard {
        mix_into(&mut out, &clank(rng, 520.0, 0.1), 0.0, 0.3);
        mix_into(&mut out, &grunt(rng, 150.0, 0.18), 0.03, 0.55);
    }
    out
}

/// A voice-like formant "oof": a buzzy low tone through two vowel resonances, pitch sagging.
fn grunt(rng: &mut Rng, pitch: f32, length: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut first = Svf::new(FilterMode::BandPass, 600.0, 4.0);
    let mut second = Svf::new(FilterMode::BandPass, 1_100.0, 5.0);
    let pitch = pitch * rng.jitter(0.06);
    render(length, |t| {
        let raw = osc.next(glide(t, pitch * 1.2, pitch * 0.8, length * 0.5));
        (first.process(raw) + second.process(raw) * 0.6)
            * segments(
                t,
                &[(0.0, 0.0), (0.015, 1.0), (length * 0.5, 0.7), (length, 0.0)],
            )
    })
}

/// The pilot taking a hit: a padded smack on the suit and a short pained grunt.
fn pilot_hurt(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(220.0 * rng.jitter(0.05), 110.0, 0.06, 2.0);
    mix_into(
        &mut out,
        &burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            1_600.0,
            0.9,
            0.05,
        ),
        0.0,
        0.4,
    );
    mix_into(&mut out, &grunt(rng, 190.0, 0.22), 0.02, 0.7);
    out
}

/// The pilot going down: a falling sigh of a tone over a body thud, then the suit's flatline
/// beep fading out (a respawn follows, so it stays light, not grim).
fn pilot_down(rng: &mut Rng) -> Vec<f32> {
    let mut out = grunt(rng, 170.0, 0.4);
    mix_into(&mut out, &thump(90.0, 55.0, 0.25, 2.0), 0.3, 0.9);
    let mut osc = Osc::new(Wave::Sine);
    let beep = render(0.9, |t| {
        osc.next(880.0) * segments(t, &[(0.0, 0.0), (0.02, 0.5), (0.5, 0.35), (0.9, 0.0)])
    });
    mix_into(&mut out, &beep, 0.55, 0.35);
    out
}

/// The suit's low-oxygen alarm: two quick, breathy high pips.
fn oxygen_alarm(_rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, frequency) in [1_320.0, 1_100.0].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Triangle);
        let pip = render(0.12, |t| {
            osc.next(frequency) * segments(t, &[(0.0, 0.0), (0.01, 1.0), (0.09, 0.8), (0.12, 0.0)])
        });
        mix_into(&mut out, &pip, index as f32 * 0.16, 0.8);
    }
    out
}

/// The jetpack lighting: an ignition pop and a short roar of rushing gas.
fn jetpack_burst(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(260.0 * rng.jitter(0.05), 90.0, 0.05, 2.5);
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 1_600.0, 0.8);
    let mut hiss = Noise::new(NoiseColor::White, rng.next_u64());
    let mut high = Svf::new(FilterMode::HighPass, 3_500.0, 0.7);
    let roar = render(0.45, |t| {
        let env = segments(t, &[(0.0, 0.0), (0.04, 1.0), (0.25, 0.7), (0.45, 0.0)]);
        (saturate(low.process(noise.next()) * 3.0, 2.0) * 0.7 + high.process(hiss.next()) * 0.5)
            * env
    });
    mix_into(&mut out, &roar, 0.01, 0.8);
    out
}

/// The jetpack running dry: three sputtering coughs, each weaker.
fn jetpack_sputter(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for index in 0..3_u8 {
        let level = 1.0 - f32::from(index) * 0.28;
        let cough = burst(
            rng.next_u64(),
            NoiseColor::Pink,
            FilterMode::BandPass,
            1_300.0 * rng.jitter(0.15),
            0.8,
            0.06,
        );
        mix_into(&mut out, &cough, f32::from(index) * 0.11, level);
        mix_into(
            &mut out,
            &thump(260.0, 160.0, 0.03, 2.0),
            f32::from(index) * 0.11,
            level * 0.2,
        );
    }
    out
}

/// A hand-pick swing biting soft ground: a small metal tick and crumbling dirt.
fn pick_chip(rng: &mut Rng) -> Vec<f32> {
    let base = 1_900.0 * rng.jitter(0.05);
    let mut out = clank(rng, base, 0.03);
    for sample in &mut out {
        *sample *= 0.5;
    }
    mix_into(&mut out, &thump(160.0, 100.0, 0.05, 1.5), 0.0, 0.5);
    grains(rng, &mut out, 6, (0.005, 0.12), (900.0, 2_800.0), 0.35);
    out
}

/// The hand pick glancing off rock: a bright ringing clink with no give.
fn pick_clink(rng: &mut Rng) -> Vec<f32> {
    let base = 2_400.0 * rng.jitter(0.04);
    let mut out = clank(rng, base, 0.14);
    mix_into(&mut out, &bell(3_150.0, 2.4, 1.2, 0.12), 0.0, 0.25);
    out
}

/// Going into water: a plunge thump, a spray of bright droplets and a bubbling tail.
fn splash(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(140.0 * rng.jitter(0.05), 70.0, 0.1, 1.2);
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_500.0, 0.7);
    let spray = render(0.35, |t| {
        filter.run(noise.next(), 4_000.0 - 2_500.0 * t / 0.35, 0.7) * perc(t, 0.004, 0.18)
    });
    mix_into(&mut out, &spray, 0.0, 0.7);
    for index in 0..5_u8 {
        let mut osc = Osc::new(Wave::Sine);
        let base = rng.range(500.0, 1_100.0);
        let bubble = render(0.05, |t| {
            osc.next(base * (1.0 + t * 14.0)) * perc(t, 0.002, 0.04)
        });
        mix_into(&mut out, &bubble, 0.12 + f32::from(index) * 0.05, 0.25);
    }
    out
}

/// Gloves catching a ledge: a quick leathery slap and a scrape of boots against rock.
fn ledge_grab(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_400.0 * rng.jitter(0.1),
        1.4,
        0.035,
    );
    mix_into(&mut out, &thump(200.0, 140.0, 0.04, 1.2), 0.0, 0.5);
    grains(rng, &mut out, 5, (0.03, 0.16), (1_800.0, 4_000.0), 0.2);
    out
}

// Loops (rendered LOOP_SECONDS + crossfade, then folded seamless by `fold_loop`). Periodic parts
// use frequencies that complete whole cycles in LOOP_SECONDS (multiples of 0.5 Hz). Scattered
// events (pops, knocks, grains) are spread evenly over the whole render at a steady density, so
// nothing marks where the loop restarts.
// ---------------------------------------------------------------------------------------------

fn loop_length() -> f32 {
    LOOP_SECONDS + LOOP_CROSSFADE_SECONDS
}

/// How many scattered events a loop render holds at `per_second` events a second.
fn loop_events(per_second: f32) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive event count"
    )]
    let count = (per_second * loop_length()).round() as usize;
    count
}

/// Slow random flutter: smoothed noise around 1.0 with `depth`.
fn flutter(rng: &mut Rng, speed: f32, depth: f32) -> impl FnMut() -> f32 + use<> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut smooth = Svf::new(FilterMode::LowPass, speed, 0.7);
    move || 1.0 + (smooth.process(noise.next()) * 25.0 * depth).clamp(-depth, depth)
}

fn thrust_loop(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::BandPass, 1_200.0, 0.6);
    let mut hum_a = Osc::new(Wave::Saw);
    let mut hum_b = Osc::new(Wave::Saw);
    let mut hum_filter = Svf::new(FilterMode::LowPass, 250.0, 0.9);
    let mut roar = Noise::new(NoiseColor::White, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::BandPass, 2_800.0, 0.8);
    let mut wobble = flutter(rng, 10.0, 0.15);
    let mut sputter = flutter(rng, 35.0, 0.35);
    render(loop_length(), |_| {
        let body = rumble_filter.process(rumble.next());
        let air = hiss_filter.process(hiss.next()) * wobble();
        let jet = roar_filter.process(roar.next()) * sputter();
        let hum = hum_filter.process(hum_a.next(55.0) + hum_b.next(82.5) * 0.6);
        saturate(body * 0.5 + air * 2.2 + jet * 0.35 + hum * 0.2, 1.5)
    })
}

fn drill_soft_loop(rng: &mut Rng) -> Vec<f32> {
    let mut motor_a = Osc::new(Wave::Saw);
    let mut motor_b = Osc::new(Wave::Square);
    let mut motor_filter = Svf::new(FilterMode::LowPass, 600.0, 0.9);
    let mut scrape = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut scrape_filter = Svf::new(FilterMode::BandPass, 900.0, 0.9);
    let mut clods = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut clod_filter = Svf::new(FilterMode::LowPass, 500.0, 0.7);
    let mut grit = flutter(rng, 30.0, 0.5);
    render(loop_length(), |t| {
        let rotation = sine(t, 16.0).abs();
        let motor = motor_filter.process(motor_a.next(95.0) + motor_b.next(190.0) * 0.4);
        let scraping = scrape_filter.process(scrape.next()) * (0.4 + 0.6 * rotation) * grit();
        let clod = clod_filter.process(clods.next()) * (0.5 + 0.5 * sine(t, 4.0));
        motor * 0.25 * (0.85 + 0.15 * rotation) + scraping * 0.9 + clod * 0.5
    })
}

fn drill_hard_loop(rng: &mut Rng) -> Vec<f32> {
    let mut motor_a = Osc::new(Wave::Saw);
    let mut motor_b = Osc::new(Wave::Saw);
    let mut motor_filter = Svf::new(FilterMode::LowPass, 900.0, 1.0);
    let mut grind = Noise::new(NoiseColor::White, rng.next_u64());
    let mut grind_filter = Svf::new(FilterMode::BandPass, 2_200.0, 1.5);
    let mut grind_post = Svf::new(FilterMode::LowPass, 4_500.0, 0.7);
    let mut rough = flutter(rng, 40.0, 0.6);
    let mut squeal = flutter(rng, 3.0, 0.02);
    // Phase-accumulating: `sine(t, f(t))` would turn the wobble into a sweep growing with `t`.
    let mut squeal_a = Osc::new(Wave::Sine);
    let mut squeal_b = Osc::new(Wave::Sine);
    render(loop_length(), |t| {
        let chatter = 0.55 + 0.45 * sine(t, 24.0).abs();
        let motor = motor_filter.process(motor_a.next(120.0) + motor_b.next(240.0) * 0.5);
        let grinding = grind_post.process(saturate(
            grind_filter.process(grind.next()) * 4.0 * chatter * rough(),
            5.0,
        ));
        let pitch = squeal();
        let metal = (squeal_a.next(3_100.0 * pitch) + squeal_b.next(4_700.0 * pitch) * 0.6) * 0.04;
        motor * 0.3 + grinding * 0.5 + metal
    })
}

/// Wind over open ground: pink noise through a band-pass whose centre wanders slowly (the
/// howl), with gusts swelling the level, and a low brown-noise buffeting underneath.
fn wind_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut howl = Svf::new(FilterMode::BandPass, 600.0, 2.2);
    let mut whistle = Svf::new(FilterMode::BandPass, 1_400.0, 6.0);
    let mut buffet = Svf::new(FilterMode::LowPass, 180.0, 0.7);
    let mut wander = flutter(rng, 0.25, 0.6);
    let mut gust = flutter(rng, 0.4, 0.55);
    render(loop_length(), |_| {
        let center = 600.0 * wander();
        howl.set(center.clamp(220.0, 1_500.0), 2.2);
        whistle.set((center * 2.3).clamp(600.0, 3_800.0), 6.0);
        let pink = noise.next();
        let level = gust();
        (howl.process(pink) * 0.9 + whistle.process(pink) * 0.25 * level) * level
            + buffet.process(low.next()) * 0.5
    })
}

/// Rain: a soft high-passed hiss and many tiny droplet ticks, a few heavier drips.
fn rain_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss = Svf::new(FilterMode::BandPass, 2_600.0, 0.8);
    let mut body = Svf::new(FilterMode::BandPass, 700.0, 0.6);
    let mut swell = flutter(rng, 0.6, 0.2);
    let mut out = render(loop_length(), |_| {
        let pink = noise.next();
        (hiss.process(pink) * 0.3 + body.process(pink) * 0.45) * swell()
    });
    for time in scatter(rng, loop_events(70.0), 0.0, loop_length() - 0.02, 1.0) {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(1_200.0, 4_000.0),
            3.0,
            0.004,
        );
        mix_into(&mut out, &tick, time, rng.range(0.1, 0.4));
    }
    for time in scatter(rng, loop_events(3.0), 0.0, loop_length() - 0.1, 1.0) {
        let mut osc = Osc::new(Wave::Sine);
        let base = rng.range(900.0, 1_500.0);
        let drip = render(0.04, |t| {
            osc.next(base * (1.0 + t * 12.0)) * perc(t, 0.002, 0.03)
        });
        mix_into(&mut out, &drip, time, 0.18);
    }
    out
}

fn gas_hiss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut high = Svf::new(FilterMode::HighPass, 2_500.0, 0.7);
    let mut band = Svf::new(FilterMode::BandPass, 6_000.0, 0.8);
    let mut swell = flutter(rng, 1.5, 0.3);
    render(loop_length(), |_| {
        let white = noise.next();
        (high.process(white) * 0.5 + band.process(white) * 0.6) * swell()
    })
}

fn lava_sizzle_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut sizzle = Svf::new(FilterMode::BandPass, 5_000.0, 0.5);
    let mut crackle = flutter(rng, 60.0, 0.9);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 120.0, 0.8);
    let mut out = render(loop_length(), |_| {
        let crackling = sizzle.process(noise.next()) * crackle().powi(3) * 0.35;
        crackling + rumble_filter.process(rumble.next()) * 0.8
    });
    // Pops and bubbles.
    for time in scatter(rng, loop_events(11.5), 0.0, loop_length() - 0.1, 1.0) {
        let pop = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(3_000.0, 7_000.0),
            2.0,
            0.006,
        );
        mix_into(&mut out, &pop, time, rng.range(0.3, 0.8));
    }
    for time in scatter(rng, loop_events(4.5), 0.0, loop_length() - 0.1, 1.0) {
        let mut osc = Osc::new(Wave::Sine);
        let base = rng.range(180.0, 260.0);
        let bubble = render(0.05, |t| {
            osc.next(base * (1.0 + t * 30.0)) * perc(t, 0.003, 0.04)
        });
        mix_into(&mut out, &bubble, time, 0.25);
    }
    out
}

fn boulder_rumble_loop(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 150.0, 1.0);
    let mut grind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut grind_filter = Svf::new(FilterMode::BandPass, 550.0, 1.0);
    let mut grind_level = flutter(rng, 6.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let roll = 0.7 + 0.3 * sine(t, 4.0);
        low.process(noise.next()) * roll * 0.8
            + grind_filter.process(grind.next()) * grind_level() * 2.0
    });
    for time in scatter(rng, loop_events(3.0), 0.0, loop_length() - 0.15, 1.0) {
        let knock = thump(rng.range(70.0, 100.0), 55.0, 0.1, 1.5);
        mix_into(&mut out, &knock, time, rng.range(0.2, 0.4));
        let crack = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(700.0, 1_400.0),
            1.5,
            0.04,
        );
        mix_into(&mut out, &crack, time, rng.range(0.3, 0.6));
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(13.0),
        (0.0, loop_length() - 0.1),
        (600.0, 2_000.0),
        0.2,
        false,
    );
    out
}

// ---------------------------------------------------------------------------------------------
// Drops and pickups (WP-DROPS)
// ---------------------------------------------------------------------------------------------

/// A plain block clacking into the hopper: a short wooden knock with a little grit.
fn block_pickup(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(420.0 * rng.jitter(0.08), 260.0, 0.05, 2.5);
    let knock = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_600.0 * rng.jitter(0.1),
        2.5,
        0.035,
    );
    mix_into(&mut out, &knock, 0.0, 0.6);
    // A tiny upward blip so it reads as "got it", not as a bump.
    let mut osc = Osc::new(Wave::Triangle);
    let blip = render(0.06, |t| {
        osc.next(glide(t, 900.0, 1_300.0, 0.03)) * perc(t, 0.002, 0.05)
    });
    mix_into(&mut out, &blip, 0.012, 0.25);
    out
}

/// An artifact into the bay: the ore sparkle, richer, with a low bell under it.
fn artifact_pickup(rng: &mut Rng) -> Vec<f32> {
    let first = rng.index(SPARKLE_NOTES.len() - 4);
    let mut out = bell(SPARKLE_NOTES[first] * 0.5, 1.0, 1.8, 0.7);
    for (step, offset) in [0_usize, 2, 4].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(SPARKLE_NOTES[first + offset], 2.0, 1.4, 0.35),
            0.06 * step as f32,
            0.6,
        );
    }
    out
}

/// Bay or hopper full: a muffled double clunk against a closed hatch.
fn stowage_full(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for hit in 0..2 {
        let mut clunk = thump(170.0 * rng.jitter(0.03), 110.0, 0.09, 3.0);
        let pitch = 620.0 * rng.jitter(0.04);
        let lid = clank(rng, pitch, 0.06);
        mix_into(&mut clunk, &lid, 0.0, 0.35);
        mix_into(&mut out, &clunk, hit as f32 * 0.12, 1.0 - hit as f32 * 0.2);
    }
    out
}

/// Blocks tipped out of the hopper: a tumbling rattle of clacks settling.
fn block_dump(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        800.0,
        0.8,
        0.25,
    );
    for time in scatter(rng, 9, 0.0, 0.45, 1.6) {
        let clack = thump(380.0 * rng.jitter(0.15), 240.0, 0.04, 2.5);
        mix_into(&mut out, &clack, time, rng.range(0.35, 0.8));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Enemies (WP-ENEMIES)
// ---------------------------------------------------------------------------------------------

/// A creature noticing you: a shrill, warbling screech that rises then falls.
fn creature_screech(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 2_400.0, 2.0);
    let base = rng.range(900.0, 1_300.0);
    let warble = rng.range(22.0, 34.0);
    render(0.42, |t| {
        let pitch = base * segments(t, &[(0.0, 0.8), (0.08, 1.35), (0.42, 0.9)]);
        let vibrato = 1.0 + 0.04 * sine(t, warble);
        let voice = fm.next(pitch * vibrato, 1.5, 1.6 * decay(t, 0.25));
        let breath = band.run(noise.next(), pitch * 2.0, 2.5) * 0.5;
        saturate((voice * 0.7 + breath) * perc(t, 0.01, 0.36), 2.0)
    })
}

/// A creature winding up: a throaty, swelling hiss.
fn creature_hiss(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 3_000.0, 1.2);
    let mut growl = Osc::new(Wave::Saw);
    let mut low = Svf::new(FilterMode::LowPass, 400.0, 1.5);
    let pitch = rng.range(70.0, 95.0);
    render(0.6, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.4, 1.0), (0.6, 0.0)]);
        let hiss = band.run(noise.next(), glide(t, 2_000.0, 4_500.0, 0.3), 1.2);
        let throat = low.process(growl.next(pitch * (1.0 + 0.1 * sine(t, 13.0))));
        (hiss * 0.6 + throat * 0.5) * swell
    })
}

/// Acid launched: a wet glottal pop and a sizzling spray.
fn acid_spit(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let base = rng.range(260.0, 340.0);
    let mut out = render(0.12, |t| {
        osc.next(base * (1.0 + t * 18.0)) * perc(t, 0.002, 0.08)
    });
    let spray = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(2_200.0, 3_200.0),
        0.8,
        0.22,
    );
    mix_into(&mut out, &spray, 0.02, 0.35);
    for time in scatter(rng, 6, 0.04, 0.25, 1.5) {
        let bubble = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(1_500.0, 3_000.0),
            3.0,
            0.012,
        );
        mix_into(&mut out, &bubble, time, rng.range(0.3, 0.6));
    }
    out
}

/// A creature hit: a short squelch and yelp.
fn creature_hurt(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let start = rng.range(700.0, 1_000.0);
    let mut out = render(0.18, |t| {
        fm.next(
            glide(t, start, start * 0.55, 0.06),
            1.0,
            2.0 * decay(t, 0.05),
        ) * perc(t, 0.003, 0.14)
    });
    let squelch = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        rng.range(500.0, 900.0),
        2.0,
        0.07,
    );
    mix_into(&mut out, &squelch, 0.0, 0.9);
    out
}

/// A creature dying: a falling wail over a wet crunch.
fn creature_death(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let start = rng.range(800.0, 1_100.0);
    let mut out = render(0.7, |t| {
        let pitch = glide(t, start, start * 0.3, 0.25);
        fm.next(
            pitch * (1.0 + 0.05 * sine(t, 18.0)),
            1.5,
            1.8 * decay(t, 0.2),
        ) * perc(t, 0.005, 0.55)
            * 0.7
    });
    let crunch = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(700.0, 1_200.0),
        1.0,
        0.15,
    );
    mix_into(&mut out, &crunch, 0.0, 0.8);
    mix_into(&mut out, &thump(140.0, 60.0, 0.2, 2.0), 0.0, 0.7);
    out
}

/// A machine locking on: two quick square-wave beeps.
fn machine_alert(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Square);
    let mut filter = Svf::new(FilterMode::LowPass, 3_500.0, 0.9);
    let pitch = rng.range(1_150.0, 1_350.0);
    render(0.32, |t| {
        let (beep, local) = if t < 0.14 { (1.0, t) } else { (1.25, t - 0.16) };
        if local < 0.0 {
            return 0.0;
        }
        filter.process(osc.next(pitch * beep))
            * segments(local, &[(0.0, 0.0), (0.005, 1.0), (0.1, 0.8), (0.12, 0.0)])
    })
}

/// A machine charging a shot or a dash: a rising whine.
fn machine_charge(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 1_000.0, 2.5);
    let start = rng.range(180.0, 240.0);
    render(0.75, |t| {
        let pitch = start * (1.0 + t * t * 9.0);
        let tremolo = 0.8 + 0.2 * sine(t, 18.0 + t * 30.0);
        filter.run(saw.next(pitch), pitch * 3.0, 2.5)
            * segments(t, &[(0.0, 0.0), (0.05, 0.6), (0.7, 1.0), (0.75, 0.0)])
            * tremolo
    })
}

/// An energy bolt fired: a bright descending zap.
fn machine_zap(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut crush = Decimator::default();
    let start = rng.range(2_400.0, 3_000.0);
    render(0.22, |t| {
        let pitch = glide(t, start, 300.0, 0.05);
        crush.process(saw.next(pitch), 4.0) * perc(t, 0.001, 0.16) * 0.8
    })
}

/// A machine hit: a dented metal clank with a spark crackle.
fn machine_hurt(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(420.0, 620.0);
    let mut out = clank(rng, base, 0.18);
    for time in scatter(rng, 5, 0.0, 0.12, 1.5) {
        let spark = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            rng.range(4_000.0, 7_000.0),
            0.8,
            0.008,
        );
        mix_into(&mut out, &spark, time, rng.range(0.3, 0.6));
    }
    out
}

/// A machine destroyed: a crunching bang, a power-down whine and scattered rattles.
fn machine_death(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        2_200.0,
        0.8,
        0.3,
    );
    mix_into(&mut out, &thump(120.0, 45.0, 0.35, 3.0), 0.0, 0.9);
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 2_000.0, 1.5);
    let start = rng.range(700.0, 900.0);
    let whine = render(0.6, |t| {
        filter.process(saw.next(glide(t, start, 60.0, 0.2))) * perc(t, 0.01, 0.5) * 0.4
    });
    mix_into(&mut out, &whine, 0.03, 1.0);
    for time in scatter(rng, 4, 0.08, 0.5, 1.4) {
        let base = rng.range(900.0, 1_600.0);
        let rattle = clank(rng, base, 0.06);
        mix_into(&mut out, &rattle, time, rng.range(0.2, 0.4));
    }
    out
}

/// A bite or ram on the rig: a meaty crunch over a hull knock.
fn enemy_bite(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        rng.range(900.0, 1_300.0),
        1.2,
        0.09,
    );
    let mut crush = Decimator::default();
    for sample in &mut out {
        *sample = crush.process(saturate(*sample * 3.0, 3.0), 2.0) * 0.7;
    }
    mix_into(&mut out, &thump(160.0, 70.0, 0.14, 2.5), 0.0, 0.9);
    let base = rng.range(300.0, 380.0);
    let knock = clank(rng, base, 0.12);
    mix_into(&mut out, &knock, 0.01, 0.4);
    out
}

// ---------------------------------------------------------------------------------------------
// Base building (WP-BASE)
// ---------------------------------------------------------------------------------------------

/// A block set in place: a gritty scrape, a dense knock and a little settle of grit.
fn build_place(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_400.0 * rng.jitter(0.1),
        1.2,
        0.05,
    );
    let knock = thump(240.0 * rng.jitter(0.06), 150.0, 0.09, 2.0);
    mix_into(&mut out, &knock, 0.03, 0.9);
    let pitch = 820.0 * rng.jitter(0.05);
    mix_into(&mut out, &clank(rng, pitch, 0.07), 0.035, 0.35);
    for time in scatter(rng, 4, 0.06, 0.2, 1.5) {
        let grit = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            3_200.0 * rng.jitter(0.2),
            2.5,
            0.012,
        );
        mix_into(&mut out, &grit, time, 0.25);
    }
    out
}

/// A module bolted down: a heavy clunk, an air-tool rattle and a ready chime.
fn module_built(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(180.0, 110.0, 0.16, 2.0);
    mix_into(&mut out, &clank(rng, 520.0, 0.3), 0.0, 0.6);
    for index in 0..6_u8 {
        let rattle = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            2_000.0 * rng.jitter(0.08),
            3.0,
            0.02,
        );
        mix_into(&mut out, &rattle, 0.18 + f32::from(index) * 0.035, 0.45);
    }
    mix_into(&mut out, &bell(1_318.5, 2.0, 1.0, 0.35), 0.42, 0.3);
    mix_into(&mut out, &bell(1_975.5, 2.0, 0.8, 0.4), 0.5, 0.28);
    out
}

/// Deconstruction: a ratchet unwinding, then the piece popping free.
fn build_remove(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for click in 0..4_u8 {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            (2_600.0 - f32::from(click) * 250.0) * rng.jitter(0.05),
            3.0,
            0.014,
        );
        mix_into(&mut out, &tick, f32::from(click) * 0.035, 0.6);
    }
    mix_into(&mut out, &thump(300.0, 190.0, 0.08, 1.5), 0.15, 0.7);
    mix_into(&mut out, &clank(rng, 700.0, 0.09), 0.15, 0.3);
    out
}

/// A claim staked: the beacon's spike driven in, then a bright rising call sign.
fn claim_stake(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(200.0, 120.0, 0.12, 2.5);
    mix_into(&mut out, &clank(rng, 980.0, 0.25), 0.0, 0.5);
    for (index, note) in [783.99, 1_046.5, 1_567.98].into_iter().enumerate() {
        let voice = brass(rng, note, 0.09, 0.8);
        mix_into(&mut out, &voice, 0.16 + index as f32 * 0.09, 0.4);
    }
    mix_into(&mut out, &bell(2_093.0, 2.0, 1.2, 0.6), 0.44, 0.3);
    let mut echo = Delay::new(0.4).with_damping(0.4);
    out.resize(out.len() + samples_for(0.3), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.16, 0.3) * 0.35;
    }
    out
}

/// Fuel flowing from a station: a nozzle clunk and a run of gurgling bubbles.
fn fuel_pump(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 460.0, 0.1);
    let mut time = 0.08;
    for _ in 0..9 {
        let start = rng.range(380.0, 620.0);
        let mut osc = Osc::new(Wave::Sine);
        let length = rng.range(0.05, 0.09);
        let bubble = render(length, |t| {
            osc.next(glide(t, start, start * 1.6, length)) * perc(t, 0.004, length)
        });
        mix_into(&mut out, &bubble, time, rng.range(0.3, 0.5));
        time += rng.range(0.05, 0.09);
    }
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 900.0, 1.0);
    let flow = render(time, |t| {
        filter.process(noise.next()) * segments(t, &[(0.0, 0.0), (0.05, 0.25), (time, 0.0)])
    });
    mix_into(&mut out, &flow, 0.06, 0.6);
    out
}

/// A supply drop: a descending whistle and the canister thudding home.
fn supply_drop(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle);
    let start = 1_800.0 * rng.jitter(0.02);
    let mut out = render(0.5, |t| {
        osc.next(glide(t, start, start * 0.45, 0.5))
            * segments(t, &[(0.0, 0.0), (0.05, 0.3), (0.5, 0.15)])
    });
    mix_into(&mut out, &thump(140.0, 90.0, 0.18, 2.5), 0.5, 1.0);
    mix_into(&mut out, &clank(rng, 380.0, 0.3), 0.5, 0.55);
    mix_into(&mut out, &bell(1_174.7, 2.0, 1.0, 0.35), 0.7, 0.25);
    out
}

/// The repair bay at work: arcing crackle and a servo whirr.
fn repair_weld(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for time in scatter(rng, 14, 0.0, 0.5, 1.0) {
        let length = rng.range(0.008, 0.03);
        let spark = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            3_500.0 * rng.jitter(0.2),
            1.0,
            length,
        );
        let gain = rng.range(0.3, 0.7);
        mix_into(&mut out, &spark, time, gain);
    }
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 1.0);
    let whirr = render(0.6, |t| {
        filter.process(saw.next(glide(t, 180.0, 320.0, 0.4)))
            * segments(t, &[(0.0, 0.0), (0.08, 0.4), (0.5, 0.3), (0.6, 0.0)])
    });
    mix_into(&mut out, &whirr, 0.0, 0.5);
    mix_into(&mut out, &bell(1_568.0, 2.0, 0.8, 0.3), 0.6, 0.2);
    out
}

/// Stowing in a chest: a wooden lid thunk and a latch.
fn storage_stow(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(160.0 * rng.jitter(0.04), 110.0, 0.12, 1.5);
    let body = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        700.0,
        1.0,
        0.08,
    );
    mix_into(&mut out, &body, 0.0, 0.6);
    mix_into(&mut out, &clank(rng, 1_250.0, 0.08), 0.14, 0.4);
    mix_into(&mut out, &clank(rng, 1_650.0, 0.06), 0.19, 0.3);
    out
}

/// The cargo uplink: a rising charge-up and a shimmering beam away.
fn uplink_beam(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let mut shimmer = Fm::default();
    let base = 420.0 * rng.jitter(0.01);
    let mut out = render(0.9, |t| {
        let frequency = glide(t, base, base * 4.0, 0.7);
        (osc.next(frequency) + shimmer.next(frequency * 1.5, 2.0, 0.6) * 0.3)
            * segments(t, &[(0.0, 0.0), (0.1, 0.4), (0.65, 0.8), (0.9, 0.0)])
    });
    for time in scatter(rng, 5, 0.6, 0.95, 1.0) {
        let note = SPARKLE_NOTES[rng.index(SPARKLE_NOTES.len())];
        mix_into(&mut out, &bell(note, 2.0, 1.0, 0.25), time, 0.18);
    }
    out
}

/// Home set: a warm major chord on soft brass.
/// Climbing into a new sky band: a swell of airy wind under a rising, open bell arpeggio (a
/// wide fifth over a fourth), with a long echo, like the view opening up.
fn sky_band(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 500.0, 0.9);
    let mut out = render(1.8, |t| {
        let cutoff = 400.0 + 1_400.0 * segments(t, &[(0.0, 0.0), (0.9, 1.0), (1.8, 0.4)]);
        filter.run(noise.next(), cutoff, 0.9) * segments(t, &[(0.0, 0.0), (0.7, 0.45), (1.8, 0.0)])
    });
    let detune = 1.0 + (rng.unit() - 0.5) * 0.004;
    for (index, (note, gain)) in [
        (523.25_f32, 0.32),
        (783.99, 0.28),
        (1_046.5, 0.26),
        (1_396.9, 0.22),
    ]
    .into_iter()
    .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "four notes")]
        let offset = 0.1 + index as f32 * 0.14;
        mix_into(&mut out, &bell(note * detune, 3.0, 1.6, 1.0), offset, gain);
    }
    let mut echo = Delay::new(0.31).with_damping(0.45);
    out.resize(out.len() + samples_for(0.4), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.35, 0.4) * 0.4;
    }
    out
}

/// Catching an updraft: a rising band-passed rush of air that swoops up and away.
fn updraft_whoosh(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 300.0, 1.4);
    let top = 2_600.0 * rng.jitter(0.1);
    render(0.9, |t| {
        let cutoff = 250.0 + top * segments(t, &[(0.0, 0.0), (0.6, 1.0), (0.9, 0.7)]);
        filter.run(noise.next(), cutoff, 1.4) * segments(t, &[(0.0, 0.0), (0.25, 1.0), (0.9, 0.0)])
    })
}

/// The hull icing up: a brittle crackle of frost forming, high glassy ticks over a cold hiss.
fn ice_crackle(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_400.0, 0.9);
    let mut out = render(0.7, |t| {
        filter.process(noise.next()) * segments(t, &[(0.0, 0.0), (0.05, 0.2), (0.7, 0.0)])
    });
    for time in scatter(rng, 12, 0.0, 0.6, 1.0) {
        let pitch = rng.range(1_400.0, 3_200.0);
        let tick = clank(rng, pitch, 0.02);
        let gain = rng.range(0.3, 0.7);
        mix_into(&mut out, &tick, time, gain);
    }
    out
}

fn home_set(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [392.0, 493.88, 587.33, 783.99].into_iter().enumerate() {
        let voice = brass(rng, note, 0.5, 0.6);
        mix_into(&mut out, &voice, index as f32 * 0.05, 0.35);
    }
    mix_into(&mut out, &bell(1_567.98, 2.0, 0.8, 0.8), 0.25, 0.2);
    out
}

/// Ore tipped into a refinery: a rattling slide of rocks, the crusher's grinding bite and a
/// furnace whoomph.
fn refinery_load(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for time in scatter(rng, 10, 0.0, 0.35, 1.0) {
        let pitch = rng.range(500.0, 1_100.0);
        let rock = clank(rng, pitch, 0.05);
        let gain = rng.range(0.2, 0.45);
        mix_into(&mut out, &rock, time, gain);
    }
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 1.2);
    let grind = render(0.45, |t| {
        filter.process(saw.next(70.0 + 8.0 * (t * 40.0).sin()))
            * segments(t, &[(0.0, 0.0), (0.05, 0.5), (0.35, 0.4), (0.45, 0.0)])
    });
    mix_into(&mut out, &grind, 0.3, 0.55);
    let whoomph = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        420.0,
        0.8,
        0.3,
    );
    mix_into(&mut out, &whoomph, 0.7, 0.7);
    out
}

/// A workshop press: three hammer strikes on an anvil and a satisfied latch.
fn workshop_craft(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for strike in 0..3_u8 {
        let time = f32::from(strike) * 0.16;
        let pitch = 1_480.0 * rng.jitter(0.03);
        mix_into(&mut out, &clank(rng, pitch, 0.18), time, 0.6);
        mix_into(&mut out, &thump(260.0, 170.0, 0.06, 1.5), time, 0.5);
    }
    mix_into(&mut out, &clank(rng, 2_100.0, 0.06), 0.56, 0.35);
    mix_into(&mut out, &bell(1_760.0, 2.0, 0.6, 0.3), 0.62, 0.2);
    out
}

/// Dawn: a distant rooster's crow (a buzzy pulse through moving vowel formants, four syllables
/// with the last one long and falling) and a scatter of songbird chirps waking up.
fn dawn_chorus(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    let base = 520.0 * rng.jitter(0.04);
    // (start, length, pitch from, pitch to)
    let syllables = [
        (0.0, 0.11, 1.0, 1.25),
        (0.15, 0.1, 1.2, 1.35),
        (0.3, 0.12, 1.3, 1.45),
        (0.47, 0.55, 1.55, 1.05),
    ];
    for (start, length, from, to) in syllables {
        let mut pulse = Osc::new(Wave::Square);
        pulse.width = 0.3;
        let mut low = Svf::new(FilterMode::BandPass, 900.0, 3.0);
        let mut high = Svf::new(FilterMode::BandPass, 2_300.0, 4.0);
        let voice = render(length, |t| {
            let progress = t / length;
            let frequency = base * (from + (to - from) * progress) * (1.0 + 0.02 * sine(t, 23.0));
            let raw = pulse.next(frequency);
            let vowel = low.process(raw) + high.process(raw) * (0.4 + 0.5 * progress);
            saturate(vowel * 1.6, 1.4)
                * segments(
                    progress,
                    &[(0.0, 0.0), (0.12, 1.0), (0.8, 0.85), (1.0, 0.0)],
                )
        });
        mix_into(&mut out, &voice, start, 0.42);
    }
    for time in scatter(rng, 7, 0.85, 1.5, 1.0) {
        let mut osc = Osc::new(Wave::Sine);
        let top = rng.range(3_200.0, 4_800.0);
        let chirps = 2 + rng.index(3);
        for chirp in 0..chirps {
            let from = top * rng.jitter(0.05);
            let chirp_voice = render(0.07, |t| {
                osc.next(glide(t, from, from * 0.72, 0.03)) * perc(t, 0.004, 0.06)
            });
            mix_into(&mut out, &chirp_voice, time + chirp as f32 * 0.085, 0.16);
        }
    }
    out
}

/// Dusk: crickets tuning up (trilled 4.4 kHz chirps in pairs) over a soft low hoot.
fn dusk_crickets(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for cricket in 0..3 {
        let pitch = rng.range(4_100.0, 4_900.0);
        let offset = cricket as f32 * 0.21 + rng.range(0.0, 0.1);
        for chirp in 0..4 {
            let mut osc = Osc::new(Wave::Sine);
            let voice = render(0.16, |t| {
                let trill = if (t * 32.0).fract() < 0.55 { 1.0 } else { 0.0 };
                osc.next(pitch)
                    * trill
                    * segments(t, &[(0.0, 0.0), (0.02, 1.0), (0.14, 0.8), (0.16, 0.0)])
            });
            mix_into(&mut out, &voice, offset + chirp as f32 * 0.36, 0.13);
        }
    }
    // A wood owl somewhere: two soft hoots, the second lower.
    for (index, frequency) in [380.0, 330.0].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Sine);
        let hoot = render(0.42, |t| {
            osc.next(frequency * (1.0 + 0.04 * (1.0 - t / 0.42)))
                * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.3, 0.7), (0.42, 0.0)])
        });
        mix_into(&mut out, &hoot, 0.8 + index as f32 * 0.5, 0.3);
    }
    out
}

/// Midnight: a far-off tower bell tolling twice, long and dark.
fn midnight_bell(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    let note = 196.0 * rng.jitter(0.005);
    for strike in 0..2 {
        mix_into(
            &mut out,
            &bell(note, 2.76, 1.6, 2.2),
            strike as f32 * 1.1,
            0.5,
        );
        mix_into(
            &mut out,
            &bell(note * 2.0, 2.0, 0.6, 1.4),
            strike as f32 * 1.1,
            0.15,
        );
    }
    out
}

/// Lying down: a blanket rustle and a sleepy falling two-note hum.
fn bed_down(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_400.0,
        0.7,
        0.35,
    );
    for (index, note) in [523.25, 392.0].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Triangle);
        let hum = render(0.4, |t| {
            osc.next(note) * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.3, 0.6), (0.4, 0.0)])
        });
        mix_into(&mut out, &hum, 0.15 + index as f32 * 0.22, 0.32);
    }
    out
}

/// The night skipped: a hushed descending lullaby (bells falling through a G-major arpeggio),
/// then the morning: a warm rising chord with a high bell.
fn night_skip(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [783.99, 587.33, 493.88, 392.0].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note, 2.0, 0.7, 1.0),
            index as f32 * 0.22,
            0.22,
        );
    }
    for (index, note) in [392.0, 493.88, 587.33, 783.99].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 0.7 - index as f32 * 0.1, 0.5),
            1.1 + index as f32 * 0.08,
            0.24,
        );
    }
    mix_into(&mut out, &bell(1_567.98, 2.0, 0.8, 1.1), 1.45, 0.2);
    out
}

/// Night ambience (looped by the director on the surface at night): a bed of crickets, each
/// trilling on its own beat, over a faint breeze.
fn night_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut out = render(loop_length(), |_| 0.0);
    let mut breeze = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breeze_filter = Svf::new(FilterMode::BandPass, 420.0, 0.7);
    let bed = render(loop_length(), |t| {
        breeze_filter.process(breeze.next()) * (0.6 + 0.4 * sine(t, 0.25))
    });
    mix_into(&mut out, &bed, 0.0, 0.25);
    for _ in 0..4 {
        let pitch = rng.range(3_900.0, 5_200.0);
        // Whole chirps per loop so the seam lines up.
        let period = SECONDS / (4 + rng.index(4)) as f32;
        let phase = rng.unit() * period;
        let mut osc = Osc::new(Wave::Sine);
        let voice = render(loop_length(), |t| {
            let local = (t + phase).rem_euclid(period);
            let trill = if (local * 30.0).fract() < 0.5 {
                1.0
            } else {
                0.0
            };
            let gate = segments(local, &[(0.0, 0.0), (0.02, 1.0), (0.18, 0.7), (0.22, 0.0)]);
            osc.next(pitch) * trill * gate
        });
        mix_into(&mut out, &voice, 0.0, 0.1);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Upgrade reward moments (WP-FEEL part 2)
// ---------------------------------------------------------------------------------------------

/// An upgrade fitted at the shop: a ratchet of wrench clicks and a pneumatic hiss as the part
/// bolts on, a power-up sweep, then a bright rising brass fanfare (C-E-G-C) landing on a held
/// chord with a shower of sparkle bells.
fn upgrade_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    // The part bolting on: three ratchet clicks and an air-tool hiss.
    for click in 0..3_u8 {
        let pitch = 2_400.0 * rng.jitter(0.04);
        mix_into(
            &mut out,
            &clank(rng, pitch, 0.05),
            f32::from(click) * 0.06,
            0.4,
        );
    }
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.8,
        0.18,
    );
    mix_into(&mut out, &hiss, 0.16, 0.35);
    // Power-up sweep.
    let mut osc = Osc::new(Wave::Square);
    let mut filter = Svf::new(FilterMode::LowPass, 900.0, 1.6);
    let sweep = render(0.32, |t| {
        let frequency = glide(t, 180.0, 720.0, 0.3);
        filter.run(osc.next(frequency), 600.0 + 5_000.0 * t, 1.6)
            * segments(t, &[(0.0, 0.0), (0.04, 0.5), (0.28, 0.6), (0.32, 0.0)])
    });
    mix_into(&mut out, &sweep, 0.24, 0.3);
    // The fanfare.
    let notes = [523.25, 659.25, 783.99, 1_046.5];
    for (index, note) in notes.into_iter().enumerate() {
        let gate = if index == 3 { 0.7 } else { 0.09 };
        mix_into(
            &mut out,
            &brass(rng, note, gate, 1.0),
            0.5 + index as f32 * 0.08,
            0.42,
        );
    }
    for note in [523.25, 659.25, 783.99] {
        mix_into(&mut out, &brass(rng, note, 0.7, 0.7), 0.74, 0.18);
    }
    for time in scatter(rng, 9, 0.74, 1.5, 1.2) {
        let note = SPARKLE_NOTES[rng.index(SPARKLE_NOTES.len())] * 2.0;
        mix_into(&mut out, &bell(note, 2.0, 0.9, 0.18), time, 0.1);
    }
    out
}

/// A milestone and its unlock: a warm three-note brass call (G-B-D), a bright bell answer and a
/// glittering shimmer for the cosmetic that just unlocked.
fn milestone_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [392.0, 493.88, 587.33].into_iter().enumerate() {
        let gate = if index == 2 { 0.8 } else { 0.12 };
        mix_into(
            &mut out,
            &brass(rng, note, gate, 0.85),
            index as f32 * 0.13,
            0.38,
        );
    }
    for note in [783.99, 987.77] {
        mix_into(&mut out, &brass(rng, note, 0.6, 0.6), 0.26, 0.16);
    }
    for (index, note) in [1_567.98, 1_975.5, 2_349.3, 3_136.0]
        .into_iter()
        .enumerate()
    {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.1, 0.6),
            0.3 + index as f32 * 0.07,
            0.16,
        );
    }
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut hp = Svf::new(FilterMode::HighPass, 7_000.0, 0.7);
    let shimmer = render(0.9, |t| {
        hp.process(noise.next()) * (0.5 + 0.5 * sine(t, 22.0)) * perc(t, 0.15, 0.9)
    });
    mix_into(&mut out, &shimmer, 0.35, 0.12);
    out
}

/// The sky opens (Act I -> II): a deep swell and wind rush, then a broad rising brass anthem
/// (C - G - C - E - G over a sustained C major chord, an octave higher at the top), crowned by
/// slow high bells, long echo.
/// The landing on the Moon: a hush, then a slow rising horn chord with a glassy bell
/// shimmer over it, like the sun breaking over the rim (WP-MOON).
fn moon_sting(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 300.0, 0.8);
    let mut out = render(2.4, |t| {
        let cutoff = 200.0 + 900.0 * segments(t, &[(0.0, 0.0), (1.2, 1.0), (2.4, 0.2)]);
        filter.run(noise.next(), cutoff, 0.8) * segments(t, &[(0.0, 0.0), (0.9, 0.3), (2.4, 0.0)])
    });
    for (index, note) in [130.81_f32, 196.0, 261.63, 329.63, 392.0]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "five notes")]
        let offset = 0.3 + index as f32 * 0.16;
        mix_into(
            &mut out,
            &brass(rng, note, 1.5 - offset * 0.5, 0.7),
            offset,
            0.2,
        );
    }
    for (index, note) in [1_567.98_f32, 1_975.5, 2_349.3, 3_136.0, 2_637.0]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "five notes")]
        let offset = 1.0 + index as f32 * 0.14;
        mix_into(&mut out, &bell(note, 3.5, 1.2, 1.0), offset, 0.1);
    }
    let mut echo = Delay::new(0.4).with_damping(0.5);
    out.resize(out.len() + samples_for(0.3), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.4, 0.45) * 0.35;
        *sample = saturate(*sample, 1.1);
    }
    out
}

/// Vacuum: a low, urgent two-tone siren with a hiss of escaping air.
fn vacuum_alarm(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_000.0,
        0.7,
        0.5,
    );
    for (index, frequency) in [440.0_f32, 330.0, 440.0, 330.0].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Square);
        let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 0.9);
        let pip = render(0.16, |t| {
            filter.process(osc.next(frequency))
                * segments(t, &[(0.0, 0.0), (0.01, 0.5), (0.14, 0.4), (0.16, 0.0)])
        });
        #[allow(clippy::cast_precision_loss, reason = "four pips")]
        mix_into(&mut out, &pip, index as f32 * 0.18, 0.5);
    }
    out
}

/// A pylon lighting: a pure, rising alien chime.
fn pylon_chime(rng: &mut Rng) -> Vec<f32> {
    let base = 880.0 * rng.jitter(0.01);
    let mut out = bell(base, 2.76, 1.4, 1.1);
    mix_into(&mut out, &bell(base * 1.5, 2.76, 1.0, 0.9), 0.08, 0.6);
    let mut echo = Delay::new(0.27).with_damping(0.4);
    out.resize(out.len() + samples_for(0.4), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.45, 0.4) * 0.45;
    }
    out
}

/// The facility's gate grinding open: a deep stone rumble and a closing chord of chimes.
fn facility_gate(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(52.0, 30.0, 1.6, 1.4);
    let grind = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        400.0,
        0.9,
        1.8,
    );
    mix_into(&mut out, &grind, 0.05, 0.7);
    for (index, note) in [523.25_f32, 659.25, 783.99, 1_046.5]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "four notes")]
        mix_into(
            &mut out,
            &bell(note, 2.76, 1.2, 1.2),
            1.0 + index as f32 * 0.12,
            0.18,
        );
    }
    out
}

/// The Warden waking: a machine boom, a rising servo whine and a resonant hum.
fn warden_wake(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(70.0, 28.0, 1.0, 1.8);
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 2.0);
    let top = 520.0 * rng.jitter(0.05);
    let whine = render(1.6, |t| {
        let frequency = 90.0 + top * segments(t, &[(0.0, 0.0), (1.1, 1.0), (1.6, 0.85)]);
        filter.process(saw.next(frequency))
            * segments(t, &[(0.0, 0.0), (0.3, 0.4), (1.3, 0.5), (1.6, 0.0)])
    });
    mix_into(&mut out, &whine, 0.2, 0.6);
    out
}

/// Reaching the core: a vast, slow, shimmering swell with a heartbeat under it.
fn core_hum(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [65.41_f32, 98.0, 130.81, 196.0, 261.63, 392.0]
        .into_iter()
        .enumerate()
    {
        let mut osc = Osc::new(Wave::Sine);
        let detune = rng.jitter(0.004);
        let voice = render(3.0, |t| {
            osc.next(note * detune * (1.0 + 0.004 * (t * 3.0).sin()))
                * segments(t, &[(0.0, 0.0), (1.4, 0.35), (2.3, 0.3), (3.0, 0.0)])
        });
        #[allow(clippy::cast_precision_loss, reason = "six voices")]
        mix_into(&mut out, &voice, index as f32 * 0.1, 0.22);
    }
    for beat in [0.5_f32, 1.2, 1.9, 2.6] {
        mix_into(&mut out, &thump(60.0, 40.0, 0.3, 1.2), beat, 0.35);
    }
    for (index, note) in [2_093.0_f32, 2_637.0, 3_136.0].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "three notes")]
        mix_into(
            &mut out,
            &bell(note, 3.0, 0.8, 1.0),
            1.2 + index as f32 * 0.3,
            0.07,
        );
    }
    out
}

/// An uplink's beam: a descending-then-rising warp sweep with a sparkle at the end.
fn uplink_warp(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 800.0, 1.5);
    let mut out = render(1.1, |t| {
        let frequency = 200.0 + 1_600.0 * segments(t, &[(0.0, 0.6), (0.4, 0.05), (1.0, 1.0)]);
        let sweep = osc.next(frequency) * 0.3;
        let air = filter.run(noise.next(), frequency * 1.5, 1.5) * 0.6;
        (sweep + air) * segments(t, &[(0.0, 0.0), (0.1, 0.8), (0.9, 0.6), (1.1, 0.0)])
    });
    mix_into(&mut out, &bell(2_637.0, 2.0, 1.0, 0.7), 0.9, 0.2);
    out
}

fn act_two_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(48.0, 32.0, 1.2, 1.5);
    let rush = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        900.0,
        0.9,
        1.4,
    );
    mix_into(&mut out, &rush, 0.0, 0.45);
    let climb = [261.63, 392.0, 523.25, 659.25, 783.99];
    for (index, note) in climb.into_iter().enumerate() {
        let gate = if index == climb.len() - 1 { 1.1 } else { 0.16 };
        mix_into(
            &mut out,
            &brass(rng, note, gate, 1.0),
            0.35 + index as f32 * 0.17,
            0.34,
        );
    }
    for note in [130.81, 261.63, 329.63, 392.0, 523.25] {
        mix_into(&mut out, &brass(rng, note, 1.3, 0.8), 1.03, 0.16);
    }
    for (index, note) in [2_093.0, 2_637.0, 3_136.0, 4_186.0, 3_136.0, 4_186.0]
        .into_iter()
        .enumerate()
    {
        mix_into(
            &mut out,
            &bell(note, 2.0, 1.0, 0.9),
            1.05 + index as f32 * 0.14,
            0.09,
        );
    }
    let mut echo = Delay::new(0.5).with_damping(0.45);
    out.resize(out.len() + samples_for(0.4), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.36, 0.4) * 0.35;
        *sample = saturate(*sample, 1.2);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Sky isles (WP-SKY, T20)
// ---------------------------------------------------------------------------------------------

/// A sky cache opening: the brass lid, a few coins, and wind chimes tumbling down a fifth.
fn sky_cache_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = cache_open(rng);
    for (index, note) in [2_637.0, 2_349.3, 1_975.5, 1_760.0, 1_318.5]
        .into_iter()
        .enumerate()
    {
        let chime = bell(note * rng.jitter(0.004), 3.5, 0.6, 0.9);
        #[allow(clippy::cast_precision_loss, reason = "a handful of chimes")]
        mix_into(&mut out, &chime, 0.32 + index as f32 * 0.07, 0.16);
    }
    out
}

/// A buoy refill: the valve clunks open, fuel gurgles through the hose, the valve shuts.
fn buoy_refuel(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(140.0 * rng.jitter(0.05), 90.0, 0.08, 1.4);
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut throat = Svf::new(FilterMode::BandPass, 380.0, 3.0);
    let mut glug = flutter(rng, 9.0, 0.8);
    let flow = render(1.1, |t| {
        let center = 280.0 + 220.0 * glug();
        throat.set(center.clamp(150.0, 900.0), 3.0);
        throat.process(noise.next())
            * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.9, 0.8), (1.1, 0.0)])
    });
    mix_into(&mut out, &flow, 0.06, 1.4);
    mix_into(&mut out, &clank(rng, 620.0, 0.12), 1.15, 0.5);
    out
}

/// A keystone breaking: a violet crack of aether, a long groan of rock, stones tearing loose.
fn isle_collapse(rng: &mut Rng) -> Vec<f32> {
    let mut out = bell(988.0 * rng.jitter(0.02), 1.41, 2.5, 0.6);
    for sample in &mut out {
        *sample *= 0.3;
    }
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 120.0, 1.2);
    let mut groan = Osc::new(Wave::Saw);
    let mut groan_filter = Svf::new(FilterMode::LowPass, 260.0, 2.0);
    let base = 48.0 * rng.jitter(0.06);
    let body = render(2.4, |t| {
        let rumble = low.process(noise.next()) * segments(t, &[(0.0, 0.0), (0.3, 1.0), (2.4, 0.0)]);
        let pitch = base * (1.0 - 0.25 * t / 2.4);
        let moan = groan_filter.process(groan.next(pitch))
            * segments(t, &[(0.0, 0.0), (0.4, 0.6), (1.6, 0.4), (2.4, 0.0)]);
        rumble * 1.4 + moan * 0.35
    });
    mix_into(&mut out, &body, 0.05, 1.0);
    for time in scatter(rng, 9, 0.3, 2.0, 1.2) {
        let crack = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(500.0, 1_300.0),
            1.4,
            0.06,
        );
        mix_into(&mut out, &crack, time, rng.range(0.2, 0.45));
        mix_into(
            &mut out,
            &thump(rng.range(60.0, 95.0), 50.0, 0.12, 1.5),
            time,
            rng.range(0.2, 0.4),
        );
    }
    out
}

/// Slipping into a jet stream: a rising rush of air with a whistle on top.
fn jet_stream_rush(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 400.0, 1.2);
    let mut whistle = Osc::new(Wave::Sine);
    let top = 3_200.0 * rng.jitter(0.08);
    render(1.3, |t| {
        let rise = segments(t, &[(0.0, 0.0), (0.7, 1.0), (1.3, 0.8)]);
        let air = filter.run(noise.next(), 300.0 + top * rise, 1.2);
        let tone = whistle.next(900.0 + 700.0 * rise) * 0.08 * rise;
        (air + tone) * segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.3, 0.0)])
    })
}

/// The high sky: a soft airy pad (fifths that complete whole cycles in the loop), a whisper of
/// wind and wind chimes drifting by now and then. It fades in among the islands.
fn sky_air_loop(rng: &mut Rng) -> Vec<f32> {
    let mut breeze = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breeze_filter = Svf::new(FilterMode::BandPass, 900.0, 0.6);
    let mut pad = [
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
        Osc::new(Wave::Sine),
    ];
    let mut out = render(loop_length(), |t| {
        let swell = 0.65 + 0.35 * sine(t, 0.5);
        let chord = pad[0].next(220.0) * 0.5
            + pad[1].next(330.0) * 0.35
            + pad[2].next(440.0) * 0.22 * swell
            + pad[3].next(659.5) * 0.12 * (1.0 - swell * 0.5);
        chord * 0.12 + breeze_filter.process(breeze.next()) * 0.35 * swell
    });
    let notes = [1_318.5, 1_568.0, 1_760.0, 1_975.5, 2_349.3, 2_637.0];
    for time in scatter(rng, loop_events(0.9), 0.0, loop_length() - 1.0, 1.0) {
        let note = notes[rng.index(notes.len())];
        mix_into(
            &mut out,
            &bell(note, 3.5, 0.5, 1.0),
            time,
            rng.range(0.03, 0.07),
        );
    }
    out
}

/// A waterfall pouring into the void: a broad rushing roar with a bright spray on top.
fn waterfall_loop(rng: &mut Rng) -> Vec<f32> {
    let mut roar = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut roar_filter = Svf::new(FilterMode::LowPass, 900.0, 0.7);
    let mut spray = Noise::new(NoiseColor::White, rng.next_u64());
    let mut spray_filter = Svf::new(FilterMode::BandPass, 3_800.0, 0.7);
    let mut surge = flutter(rng, 1.5, 0.25);
    render(loop_length(), |_| {
        let level = surge();
        roar_filter.process(roar.next()) * 1.4 * level + spray_filter.process(spray.next()) * 0.4
    })
}

// ---------------------------------------------------------------------------------------------
// Sky continents, sky enemies and Act II weapons (Wave 4 T21)
// ---------------------------------------------------------------------------------------------

/// The arc rifle: a sizzling electric crack with a crushed buzz.
fn arc_crackle(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut crush = Decimator::default();
    let mut filter = Svf::new(FilterMode::HighPass, 900.0, 0.8);
    let start = rng.range(3_200.0, 4_200.0);
    let mut out = render(0.32, |t| {
        let pitch = glide(t, start, 600.0, 0.06) * (1.0 + 0.3 * sine(t, 90.0));
        filter.process(crush.process(saw.next(pitch), 5.0)) * perc(t, 0.001, 0.22) * 0.7
    });
    for time in scatter(rng, 8, 0.0, 0.2, 1.4) {
        let spark = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            rng.range(5_000.0, 8_000.0),
            0.8,
            0.01,
        );
        mix_into(&mut out, &spark, time, rng.range(0.3, 0.6));
    }
    out
}

/// The gravity gun taking hold: a deep warbling hum that swells.
fn gravity_hum(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut filter = Svf::new(FilterMode::LowPass, 900.0, 1.2);
    let base = rng.range(150.0, 175.0);
    let mut shimmer = Osc::new(Wave::Triangle);
    render(0.8, |t| {
        let wobble = 1.0 + 0.06 * sine(t, 7.0);
        let sparkle = shimmer.next(base * 6.0 * wobble) * 0.25;
        let tone = fm.next(
            base * wobble,
            2.0,
            1.6 * segments(t, &[(0.0, 0.2), (0.4, 1.0), (0.8, 0.6)]),
        );
        filter.run(
            tone + sparkle,
            900.0 + 1_600.0 * segments(t, &[(0.0, 0.0), (0.3, 1.0), (0.8, 0.5)]),
            1.2,
        ) * segments(t, &[(0.0, 0.0), (0.15, 0.8), (0.6, 0.6), (0.8, 0.0)])
    })
}

/// A held load flung away: a rising whoosh with a hollow pop.
fn gravity_throw(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(220.0 * rng.jitter(0.05), 90.0, 0.18, 1.6);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 400.0, 1.2);
    let rush = render(0.5, |t| {
        filter.run(
            noise.next(),
            300.0 + 3_000.0 * segments(t, &[(0.0, 0.0), (0.2, 1.0), (0.5, 0.6)]),
            1.2,
        ) * segments(t, &[(0.0, 0.0), (0.06, 1.0), (0.5, 0.0)])
    });
    mix_into(&mut out, &rush, 0.0, 0.8);
    out
}

/// The tether harpoon's shot: a sharp crossbow twang and the line paying out.
fn tether_shot(rng: &mut Rng) -> Vec<f32> {
    let mut out = {
        let pitch = rng.range(260.0, 320.0);
        clank(rng, pitch, 0.12)
    };
    let mut noise = Noise::new(NoiseColor::White, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 2_000.0, 3.0);
    let line = render(0.35, |t| {
        filter.run(noise.next(), glide(t, 3_000.0, 900.0, 0.12), 3.0) * perc(t, 0.002, 0.3) * 0.6
    });
    mix_into(&mut out, &line, 0.0, 0.7);
    out
}

/// The harpoon biting rock: a hard thunk and a taut line ringing.
fn tether_hook(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(180.0 * rng.jitter(0.05), 70.0, 0.12, 2.0);
    let line = bell(rng.range(520.0, 600.0), 2.0, 1.4, 0.5);
    mix_into(&mut out, &line, 0.01, 0.35);
    grains(rng, &mut out, 6, (0.0, 0.12), (1_500.0, 3_500.0), 0.4);
    out
}

/// The flak cannon firing: a deep cough with a metallic bark.
fn flak_thump(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(140.0 * rng.jitter(0.05), 50.0, 0.28, 2.4);
    let bark = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        rng.range(900.0, 1_300.0),
        1.0,
        0.12,
    );
    mix_into(&mut out, &bark, 0.0, 0.8);
    out
}

/// A flak shell bursting: a crisp crack and a spray of shrapnel ticks.
fn flak_burst(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::LowPass,
        rng.range(3_000.0, 4_500.0),
        0.8,
        0.25,
    );
    let boom = thump(110.0, 45.0, 0.3, 2.0);
    mix_into(&mut out, &boom, 0.0, 0.7);
    for time in scatter(rng, 10, 0.03, 0.4, 1.6) {
        let tick = {
            let pitch = rng.range(1_800.0, 3_600.0);
            clank(rng, pitch, 0.015)
        };
        mix_into(&mut out, &tick, time, rng.range(0.15, 0.35));
    }
    out
}

/// A storm elemental's lightning: a ripping crack and a rolling rumble of thunder.
fn thunderclap(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        2_000.0,
        0.7,
        0.08,
    );
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let roll = rng.range(4.0, 7.0);
    let rumble = render(1.6, |t| {
        filter.run(noise.next(), 180.0 + 160.0 * sine(t, roll).abs(), 0.8)
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.5, 0.7), (1.6, 0.0)])
    });
    mix_into(&mut out, &rumble, 0.02, 1.0);
    let crack = arc_crackle(rng);
    mix_into(&mut out, &crack, 0.0, 0.6);
    out
}

/// A temple lever thrown: a heavy stone-and-iron clunk, then a ratchet.
fn lever_clunk(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(120.0 * rng.jitter(0.04), 60.0, 0.2, 1.8);
    let iron = {
        let pitch = rng.range(300.0, 380.0);
        clank(rng, pitch, 0.25)
    };
    mix_into(&mut out, &iron, 0.02, 0.6);
    for index in 0..5 {
        #[allow(clippy::cast_precision_loss, reason = "five ratchet clicks")]
        let time = 0.18 + index as f32 * 0.06;
        let click = {
            let pitch = rng.range(1_200.0, 1_600.0);
            clank(rng, pitch, 0.02)
        };
        mix_into(&mut out, &click, time, 0.3);
    }
    out
}

/// A temple gate grinding open: a long low stone rumble with a resonant chime at the end.
fn gate_rumble(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 240.0, 1.0);
    let mut out = render(1.6, |t| {
        filter.run(noise.next(), 160.0 + 120.0 * sine(t, 3.0).abs(), 1.0)
            * segments(t, &[(0.0, 0.0), (0.2, 1.0), (1.3, 0.8), (1.6, 0.0)])
    });
    grains(rng, &mut out, 18, (0.1, 1.4), (500.0, 1_800.0), 0.25);
    let chime = bell(rng.range(392.0, 420.0), 3.0, 1.2, 1.4);
    mix_into(&mut out, &chime, 1.2, 0.4);
    out
}

/// A swooper beginning its dive: a thin falling screech and a wind rush.
fn swoop_screech(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Triangle);
    let mut filter = Svf::new(FilterMode::BandPass, 1_800.0, 2.0);
    let start = rng.range(1_900.0, 2_400.0);
    let mut out = render(0.55, |t| {
        let pitch = glide(t, start, start * 0.45, 0.35) * (1.0 + 0.02 * sine(t, 30.0));
        filter.run(osc.next(pitch), pitch, 2.0)
            * segments(t, &[(0.0, 0.0), (0.04, 0.8), (0.55, 0.0)])
    });
    let rush = updraft_whoosh(rng);
    mix_into(&mut out, &rush, 0.1, 0.4);
    out
}

/// An airship's boarding horn: two brassy blasts.
fn pirate_horn(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    let note = rng.range(146.0, 165.0);
    for (offset, gate) in [(0.0, 0.35), (0.5, 0.6)] {
        let voice = brass(rng, note, gate, 0.5);
        mix_into(&mut out, &voice, offset, 0.5);
        let fifth = brass(rng, note * 1.5, gate, 0.4);
        mix_into(&mut out, &fifth, offset, 0.25);
    }
    out
}

/// A harpy finding its nest robbed: a furious shrieking wail.
fn harpy_screech(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::BandPass, 2_400.0, 2.4);
    let base = rng.range(1_100.0, 1_400.0);
    render(0.9, |t| {
        let pitch = base
            * (1.0 + 0.25 * sine(t, 11.0))
            * segments(t, &[(0.0, 0.8), (0.2, 1.3), (0.9, 0.9)]);
        filter.run(saw.next(pitch), pitch * 1.6, 2.4)
            * segments(t, &[(0.0, 0.0), (0.05, 0.9), (0.7, 0.6), (0.9, 0.0)])
    })
}

/// An airship turret's cannon: a booming report with a smoky tail.
fn cannon_boom(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(90.0 * rng.jitter(0.05), 38.0, 0.4, 2.6);
    let smoke = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        1_200.0,
        0.7,
        0.5,
    );
    mix_into(&mut out, &smoke, 0.01, 0.6);
    out
}

// ---------------------------------------------------------------------------------------------
// The pilot's arms and gear, ruin mechanisms (WP-PILOT)
// ---------------------------------------------------------------------------------------------

/// The sidearm: a tight pop, a short band-passed snap and a small room tail.
fn pistol_pop(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_600.0 * rng.jitter(0.08),
        0.9,
        0.045,
    );
    mix_into(
        &mut out,
        &thump(420.0 * rng.jitter(0.05), 160.0, 0.05, 2.2),
        0.0,
        0.6,
    );
    let mut echo = Delay::new(0.2).with_damping(0.5);
    out.resize(out.len() + samples_for(0.12), 0.0);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.07, 0.25) * 0.25;
    }
    out
}

/// The rifle: a hard supersonic crack, a body thump and a long rolling tail.
fn rifle_crack(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_200.0 * rng.jitter(0.05),
        0.8,
        0.03,
    );
    mix_into(
        &mut out,
        &thump(300.0 * rng.jitter(0.04), 120.0, 0.07, 2.2),
        0.0,
        0.55,
    );
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low = Svf::new(FilterMode::BandPass, 1_400.0, 0.6);
    let tail = render(0.45, |t| {
        low.process(noise.next()) * perc(t, 0.01, 0.32) * 1.2
    });
    mix_into(&mut out, &tail, 0.02, 0.6);
    out
}

/// The shotgun: a big boom of low-passed pink noise over a sub thump, then the pump's clack.
fn shotgun_boom(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(180.0 * rng.jitter(0.05), 55.0, 0.16, 3.0);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 2_400.0, 0.7);
    let blast = render(0.3, |t| {
        low.run(noise.next(), 2_400.0 - 1_800.0 * (t / 0.3), 0.7) * perc(t, 0.001, 0.22) * 1.6
    });
    mix_into(&mut out, &blast, 0.0, 0.9);
    let pump = clank(rng, 900.0, 0.05);
    mix_into(&mut out, &pump, 0.32, 0.35);
    mix_into(&mut out, &clank(rng, 1_150.0, 0.05), 0.4, 0.3);
    out
}

/// A wrench swing: an airy whoosh and a heavy metal clang.
fn wrench_swing(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 1.2);
    let mut out = render(0.16, |t| {
        band.run(noise.next(), 600.0 + 2_200.0 * (t / 0.16), 1.2) * perc(t, 0.06, 0.08)
    });
    let base = 640.0 * rng.jitter(0.05);
    mix_into(&mut out, &clank(rng, base, 0.18), 0.1, 0.7);
    out
}

/// The grapple: a pneumatic thunk, the hook whistling out and a metal bite.
fn grapple_fire(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(220.0 * rng.jitter(0.05), 120.0, 0.05, 1.8);
    let mut osc = Osc::new(Wave::Saw);
    let mut band = Svf::new(FilterMode::BandPass, 1_800.0, 3.0);
    let zip = render(0.22, |t| {
        band.process(osc.next(glide(t, 1_400.0, 2_600.0, 0.18))) * perc(t, 0.01, 0.18) * 0.5
    });
    mix_into(&mut out, &zip, 0.01, 0.7);
    let base = 2_100.0 * rng.jitter(0.04);
    mix_into(&mut out, &clank(rng, base, 0.08), 0.2, 0.45);
    out
}

/// A glider opening: a cloth snap and a soft rush of air.
fn glider_open(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_100.0 * rng.jitter(0.1),
        1.6,
        0.04,
    );
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 1_200.0, 0.6);
    let rush = render(0.5, |t| {
        low.process(noise.next()) * segments(t, &[(0.0, 0.0), (0.08, 0.8), (0.5, 0.0)])
    });
    mix_into(&mut out, &rush, 0.02, 0.8);
    out
}

/// A gate grinding open or shut: a low sawtooth groan over rattling chains and a final clang.
fn gate_grind(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw);
    let mut low = Svf::new(FilterMode::LowPass, 1_800.0, 1.4);
    let base = 120.0 * rng.jitter(0.05);
    let mut out = render(0.7, |t| {
        let wobble = 1.0 + 0.04 * sine(t, 7.0);
        saturate(low.process(osc.next(base * wobble)) * 1.5, 2.0)
            * segments(t, &[(0.0, 0.0), (0.06, 0.9), (0.6, 0.7), (0.7, 0.0)])
    });
    grains(rng, &mut out, 16, (0.05, 0.6), (1_500.0, 4_000.0), 0.35);
    mix_into(&mut out, &clank(rng, 520.0, 0.25), 0.62, 0.7);
    out
}

/// A keycard swipe: a reader chirp and two rising confirm beeps.
fn keycard_beep(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        4_000.0,
        0.7,
        0.03,
    );
    for (index, note) in [1_320.0_f32, 1_760.0].into_iter().enumerate() {
        let mut osc = Osc::new(Wave::Square);
        osc.width = 0.4;
        let mut low = Svf::new(FilterMode::LowPass, 3_000.0, 0.7);
        let beep = render(0.08, |t| {
            low.process(osc.next(note)) * perc(t, 0.002, 0.07) * 0.5
        });
        mix_into(&mut out, &beep, 0.05 + index as f32 * 0.09, 0.8);
    }
    out
}

// --- Night-time gameplay (WP-NIGHT) -------------------------------------------------------

/// A night hound's howl: a breathy voice sliding up to a held, wavering note and falling away,
/// with a softer echo. `depth` below 1 lowers and lengthens it (the umbral alpha).
fn hound_howl(rng: &mut Rng, depth: f32) -> Vec<f32> {
    let length = 1.5 / depth.max(0.4);
    let base = rng.range(420.0, 520.0) * depth;
    let mut out = Vec::new();
    for (voice, (delay, gain, detune)) in [(0.0, 0.55, 1.0), (0.35, 0.2, 0.985)]
        .into_iter()
        .enumerate()
    {
        let mut fm = Fm::default();
        let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
        let mut breath = Svf::new(FilterMode::BandPass, 900.0, 1.4);
        let wobble = rng.range(4.5, 6.0);
        let howl = render(length, |t| {
            let shape = segments(
                t / length,
                &[(0.0, 0.7), (0.22, 1.0), (0.7, 0.97), (1.0, 0.72)],
            );
            let pitch = base * detune * shape * (1.0 + 0.012 * sine(t, wobble));
            let tone = fm.next(pitch, 1.0, 0.6 + 0.3 * decay(t, 0.4));
            let air = breath.run(noise.next(), pitch * 2.2, 1.6) * 0.35;
            (tone * 0.7 + air)
                * segments(
                    t / length,
                    &[(0.0, 0.0), (0.12, 1.0), (0.75, 0.8), (1.0, 0.0)],
                )
        });
        let _ = voice;
        mix_into(&mut out, &howl, delay, gain);
    }
    out
}

/// Sand bursting up round a dune stalker: a low thump, a rushing hiss and grains pattering
/// down.
fn stalker_burst(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let start = rng.range(110.0, 140.0);
    let mut out = render(0.35, |t| {
        osc.next(glide(t, start, start * 0.45, 0.2)) * perc(t, 0.004, 0.3)
    });
    let rush = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        rng.range(1_400.0, 2_000.0),
        0.6,
        0.45,
    );
    mix_into(&mut out, &rush, 0.01, 0.55);
    for time in scatter(rng, 9, 0.12, 0.6, 1.3) {
        let grain = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(3_000.0, 5_500.0),
            2.5,
            0.01,
        );
        mix_into(&mut out, &grain, time, rng.range(0.15, 0.3));
    }
    out
}

/// A will-o'-wisp's chime: three detuned glassy bells in a strange interval, shimmering.
fn wisp_chime(rng: &mut Rng) -> Vec<f32> {
    let root = rng.range(880.0, 1_040.0);
    let mut out = Vec::new();
    for (index, ratio) in [1.0, 1.41, 1.89].into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(root * ratio, 3.5, 0.5, 0.9),
            index as f32 * 0.07,
            0.25,
        );
    }
    let mut osc = Osc::new(Wave::Sine);
    let shimmer = render(0.9, |t| {
        osc.next(root * 2.0 * (1.0 + 0.01 * sine(t, 7.0)))
            * segments(t, &[(0.0, 0.0), (0.3, 0.6), (0.9, 0.0)])
    });
    mix_into(&mut out, &shimmer, 0.0, 0.12);
    out
}

/// A moth swarm: a soft papery flutter beating fast and drifting past.
fn moth_flutter(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 1_800.0, 1.0);
    let beat = rng.range(24.0, 32.0);
    render(0.6, |t| {
        let wings = (0.5 + 0.5 * sine(t, beat)).powi(3);
        band.run(noise.next(), 1_400.0 + 900.0 * sine(t, 2.0), 1.0)
            * wings
            * segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.45, 0.8), (0.6, 0.0)])
    })
}

/// Night falls and the monsters rise: a dark minor swell in low brass under a far howl.
fn night_fall(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [220.0, 261.63, 329.63].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 1.6, 0.45),
            index as f32 * 0.12,
            0.3,
        );
    }
    let mut low = Osc::new(Wave::Triangle);
    let drone = render(2.4, |t| {
        low.next(110.0) * segments(t, &[(0.0, 0.0), (0.5, 1.0), (1.8, 0.7), (2.4, 0.0)])
    });
    mix_into(&mut out, &drone, 0.0, 0.18);
    mix_into(&mut out, &bell(659.25, 1.41, 0.6, 1.4), 0.4, 0.08);
    mix_into(&mut out, &hound_howl(rng, 0.9), 0.9, 0.22);
    out
}

/// Dawn after a night with monsters about: a warm major chord blooming, a high bell, birds.
fn dawn_relief(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [261.63, 329.63, 392.0, 523.25].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 1.1 - index as f32 * 0.12, 0.45),
            index as f32 * 0.09,
            0.22,
        );
    }
    mix_into(&mut out, &bell(1_046.5, 2.0, 0.7, 1.2), 0.45, 0.18);
    for chirp in 0..4 {
        let mut osc = Osc::new(Wave::Sine);
        let pitch = rng.range(2_600.0, 3_400.0);
        let tweet = render(0.09, |t| {
            osc.next(glide(t, pitch, pitch * 1.35, 0.07)) * perc(t, 0.005, 0.08)
        });
        mix_into(&mut out, &tweet, 0.7 + chirp as f32 * 0.17, 0.12);
    }
    out
}

/// A night monster burrowing away: a soft whump and a trickle of dust.
fn burrow_puff(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let start = rng.range(180.0, 230.0);
    let mut out = render(0.2, |t| {
        osc.next(glide(t, start, start * 0.5, 0.15)) * perc(t, 0.006, 0.18)
    });
    let dust = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        rng.range(1_200.0, 1_800.0),
        0.7,
        0.35,
    );
    mix_into(&mut out, &dust, 0.03, 0.4);
    out
}

/// The prowl (looped by the director when night monsters are about): a low uneasy drone, wind
/// in dry grass, and a far howl somewhere in the loop.
fn night_prowl_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut low = Osc::new(Wave::Triangle);
    let mut high = Osc::new(Wave::Sine);
    let mut grass = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut grass_filter = Svf::new(FilterMode::BandPass, 2_400.0, 0.8);
    let mut out = render(loop_length(), |t| {
        // Whole cycles per loop so the seam lines up.
        let swell = 0.6 + 0.4 * sine(t, 1.0 / SECONDS);
        let drone = low.next(110.0) * 0.4 + high.next(165.0) * 0.2 * swell;
        let rustle = grass_filter.process(grass.next()) * (0.5 + 0.5 * sine(t, 2.0 / SECONDS));
        drone * swell * 0.45 + rustle * 0.35
    });
    let howl = hound_howl(rng, 0.85);
    mix_into(&mut out, &howl, rng.range(0.4, 1.2), 0.12);
    out
}

/// Someone has a story for you: a plucked "hm?" of two notes rising a sixth, the second left
/// hanging, over a breath of paper.
fn story_hook(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        3_200.0,
        1.1,
        0.18,
    );
    for sample in &mut out {
        *sample *= 0.35;
    }
    let detune = rng.jitter(0.004);
    mix_into(&mut out, &bell(392.0 * detune, 2.5, 0.9, 0.5), 0.04, 0.34);
    mix_into(&mut out, &bell(659.25 * detune, 2.5, 1.3, 0.9), 0.24, 0.3);
    mix_into(&mut out, &bell(587.33 * detune, 2.0, 1.2, 1.1), 0.5, 0.14);
    out
}

/// A beat of the story written down: a quill's quick scratches and a small bright chime.
fn story_quill(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(0.7)];
    for index in 0..4_u8 {
        let scratch = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            4_800.0 * rng.jitter(0.15),
            2.5,
            0.035 + f32::from(index % 2) * 0.02,
        );
        mix_into(&mut out, &scratch, f32::from(index) * 0.06, 0.32);
    }
    mix_into(
        &mut out,
        &bell(1_046.5 * rng.jitter(0.003), 3.0, 0.9, 0.7),
        0.3,
        0.3,
    );
    mix_into(&mut out, &bell(1_567.98, 3.0, 0.8, 0.8), 0.38, 0.18);
    out
}

/// The climax lands: a music box spelling out a rising arpeggio while a soft pad swells under
/// it, ending on a held, shimmering chord.
fn story_climax(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(2.4)];
    let detune = rng.jitter(0.002);
    let notes = [
        523.25_f32, 659.25, 783.99, 1_046.5, 987.77, 1_174.7, 1_318.5,
    ];
    for (index, note) in notes.into_iter().enumerate() {
        mix_into(
            &mut out,
            &bell(note * detune, 4.0, 1.0, 1.2),
            0.05 + index as f32 * 0.13,
            0.22,
        );
    }
    let mut pad = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let mut fifth = Osc::new(Wave::Triangle).with_phase(rng.unit());
    let env = Adsr::new(0.6, 0.4, 0.7, 0.8);
    let swell = render(2.2, |t| {
        (pad.next(261.63) + fifth.next(392.0) * 0.7) * env.level(t, 1.4) * 0.12
    });
    mix_into(&mut out, &swell, 0.0, 1.0);
    for note in [1_046.5, 1_318.5, 1_567.98] {
        mix_into(&mut out, &bell(note, 2.0, 1.6, 1.6), 1.0, 0.12);
    }
    let mut echo = Delay::new(0.4).with_damping(0.5);
    for sample in &mut out {
        *sample += echo.process(*sample, 0.31, 0.35) * 0.3;
    }
    out
}

/// A story's end: a warm brass cadence (IV, V, I) with a bell on the last chord.
fn story_resolved(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(2.0)];
    let chords: [(f32, &[f32]); 3] = [
        (0.0, &[349.23, 440.0, 523.25]),
        (0.36, &[392.0, 493.88, 587.33]),
        (0.72, &[261.63, 329.63, 392.0, 523.25]),
    ];
    for (index, (start, notes)) in chords.into_iter().enumerate() {
        let gate = if index == 2 { 1.0 } else { 0.3 };
        for note in notes {
            mix_into(&mut out, &brass(rng, *note, gate, 0.7), start, 0.13);
        }
    }
    mix_into(&mut out, &bell(1_046.5, 2.0, 1.4, 1.1), 0.74, 0.2);
    mix_into(&mut out, &bell(1_567.98, 2.0, 1.2, 1.2), 0.86, 0.1);
    out
}

// ---------------------------------------------------------------------------------------------
// Side-view tombs (WP-TOMBS T28): run music, the guardian's theme, traps and tomb creatures.
// ---------------------------------------------------------------------------------------------

/// Mixes `note` at `start` into a loop render, and again a loop later (into the crossfade tail
/// that `fold_loop` folds over the head), so notes near the seam ring across it.
fn loop_note(out: &mut Vec<f32>, note: &[f32], start: f32, gain: f32) {
    mix_into(out, note, start, gain);
    if start + LOOP_SECONDS < loop_length() {
        mix_into(out, note, start + LOOP_SECONDS, gain);
    }
}

/// A side-view tomb's run music (looped while a seat explores one): a low D and A drone under
/// a slow plucked line in D phrygian (90 bpm, two bars of three), a frame drum on the bar.
fn tomb_run_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 700.0, 0.6);
    let mut out = render(loop_length(), |t| {
        // Whole cycles per loop (73.5 and 110 Hz, swells at 0.25 and 0.5 Hz).
        let swell = 0.75 + 0.25 * sine(t, 0.25);
        (low.next(73.5) * 0.5 + fifth.next(110.0) * 0.22) * swell * 0.4
            + air_filter.process(air.next()) * 0.05 * (0.6 + 0.4 * sine(t, 0.5))
    });
    let eighth = LOOP_SECONDS / 12.0;
    let line: [Option<f32>; 12] = [
        Some(293.66),
        None,
        Some(440.0),
        Some(466.16),
        Some(440.0),
        None,
        Some(349.23),
        Some(392.0),
        Some(311.13),
        None,
        Some(293.66),
        Some(261.63),
    ];
    for (step, note) in line.into_iter().enumerate() {
        let Some(note) = note else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "twelve steps")]
        let start = step as f32 * eighth;
        let pluck = bell(note * rng.jitter(0.002), 1.0, 1.6, 0.9);
        loop_note(&mut out, &pluck, start, 0.16);
    }
    for (beat, gain) in [(0.0, 0.5), (2.0, 0.42), (1.333, 0.18), (3.333, 0.18)] {
        let drum = thump(120.0, 55.0, 0.35, 1.2);
        loop_note(&mut out, &drum, beat, gain);
    }
    out
}

/// The guardian's theme (looped while a side-view guardian is awake): driving toms on every
/// eighth (120 bpm), a low D ostinato and brass stabs a half step apart, D to E flat.
fn tomb_boss_loop(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(loop_length(), |_| 0.0);
    let eighth = LOOP_SECONDS / 16.0;
    for step in 0..16_u8 {
        let accent = step % 4 == 0;
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = f32::from(step) * eighth;
        let tom = thump(
            if accent { 150.0 } else { 190.0 },
            if accent { 52.0 } else { 80.0 },
            0.22,
            1.6,
        );
        loop_note(&mut out, &tom, start, if accent { 0.5 } else { 0.24 });
        let bass = bell(73.42 * 2.0, 1.0, 0.9, 0.2);
        loop_note(&mut out, &bass, start, 0.12);
    }
    for (bar, root) in [
        (0.0, 293.66_f32),
        (1.0, 311.13),
        (2.0, 293.66),
        (3.0, 277.18),
    ] {
        for note in [root, root * 1.5] {
            let stab = brass(rng, note * 0.5, 0.28, 0.8);
            loop_note(&mut out, &stab, bar, 0.13);
        }
    }
    out
}

/// A swinging blade passing: a band-passed whoosh sweeping up and back down, a thin ring.
fn blade_swish(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 800.0, 2.2);
    let peak = rng.range(2_200.0, 2_800.0);
    let mut out = render(0.45, |t| {
        let sweep = segments(t, &[(0.0, 600.0), (0.18, peak), (0.42, 700.0)]);
        filter.run(noise.next(), sweep, 2.2) * segments(t, &[(0.0, 0.0), (0.16, 1.0), (0.42, 0.0)])
    });
    mix_into(
        &mut out,
        &bell(1_850.0 * rng.jitter(0.02), 2.76, 0.8, 0.5),
        0.16,
        0.12,
    );
    out
}

/// A crusher slamming down: a heavy stone thud, a crunch and settling grit.
fn crusher_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(95.0 * rng.jitter(0.05), 34.0, 0.55, 2.2);
    let crunch = burst(
        rng.next_u64(),
        NoiseColor::Brown,
        FilterMode::LowPass,
        900.0,
        0.8,
        0.3,
    );
    mix_into(&mut out, &crunch, 0.0, 0.7);
    grains(rng, &mut out, 18, (0.02, 0.6), (900.0, 3_000.0), 0.3);
    out
}

/// A dart loosed from a wall: a dry click and a short falling whistle.
fn dart_fire(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.8,
        0.02,
    );
    let mut osc = Osc::new(Wave::Sine);
    let start = rng.range(2_300.0, 2_700.0);
    let whistle = render(0.2, |t| {
        osc.next(glide(t, start, start * 0.62, 0.12)) * perc(t, 0.004, 0.18)
    });
    mix_into(&mut out, &whistle, 0.01, 0.35);
    out
}

/// A floor slab giving way: a crack, a soft thud and a patter of falling grit.
fn floor_crumble(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_400.0,
        1.2,
        0.05,
    );
    mix_into(&mut out, &thump(140.0, 60.0, 0.3, 1.4), 0.03, 0.6);
    grains(rng, &mut out, 26, (0.02, 0.8), (1_200.0, 4_200.0), 0.35);
    out
}

/// Sand (or water) starting to pour from a sluice: a hiss swelling under a low rush.
fn sand_pour(rng: &mut Rng) -> Vec<f32> {
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::BandPass, 1_900.0, 0.9);
    let mut rush = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rush_filter = Svf::new(FilterMode::LowPass, 420.0, 0.7);
    let mut out = render(1.3, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.4, 0.8), (1.0, 1.0), (1.3, 0.0)]);
        (hiss_filter.process(hiss.next()) * 0.6 + rush_filter.process(rush.next()) * 0.8) * shape
    });
    grains(rng, &mut out, 20, (0.1, 1.2), (2_000.0, 5_000.0), 0.15);
    out
}

/// A cursed vent bursting: a breathy rush of fog and a hollow, ghostly tone bending down.
fn fog_burst(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 600.0, 0.9);
    let mut out = render(0.9, |t| {
        let open = segments(t, &[(0.0, 300.0), (0.15, 1_600.0), (0.9, 500.0)]);
        filter.run(noise.next(), open, 0.9) * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.9, 0.0)])
    });
    let mut fm = Fm::default();
    let base = rng.range(330.0, 380.0);
    let tone = render(0.8, |t| {
        fm.next(glide(t, base, base * 0.7, 0.5), 1.5, 0.8) * perc(t, 0.08, 0.7)
    });
    mix_into(&mut out, &tone, 0.05, 0.2);
    out
}

/// A mummy's groan: a low saw voice through two shifting formants.
fn mummy_groan(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw);
    let mut first = Svf::new(FilterMode::BandPass, 500.0, 3.0);
    let mut second = Svf::new(FilterMode::BandPass, 900.0, 3.5);
    let base = rng.range(92.0, 110.0);
    render(0.9, |t| {
        let pitch = base * segments(t, &[(0.0, 1.0), (0.5, 1.08), (0.9, 0.82)]);
        let raw = osc.next(pitch);
        let vowel = segments(t, &[(0.0, 0.0), (0.9, 1.0)]);
        let voice = first.run(raw, 450.0 + 250.0 * vowel, 3.0) * 0.6
            + second.run(raw, 1_000.0 - 250.0 * vowel, 3.5) * 0.4;
        saturate(voice * 1.4, 1.5) * segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.7, 0.8), (0.9, 0.0)])
    })
}

/// Bones rattling (a skeletal archer drawing or falling): a run of dry clicks.
fn bone_rattle(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples_for(0.35)];
    for time in scatter(rng, 7, 0.0, 0.3, 1.2) {
        let click = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(2_400.0, 3_800.0),
            3.0,
            0.018,
        );
        mix_into(&mut out, &click, time, rng.range(0.4, 0.8));
    }
    out
}

/// Stone grinding as an animated statue wakes or swings.
fn statue_grind(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 380.0, 1.6);
    let mut out = render(0.6, |t| {
        let chatter = 0.6 + 0.4 * sine(t, 23.0);
        filter.process(noise.next()) * chatter * segments(t, &[(0.0, 0.0), (0.08, 1.0), (0.6, 0.0)])
    });
    grains(rng, &mut out, 12, (0.05, 0.55), (700.0, 2_200.0), 0.3);
    out
}

/// The colossus landing: a very low boom, a stone crack and rubble raining down.
fn colossus_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(72.0, 26.0, 0.9, 3.0);
    mix_into(
        &mut out,
        &burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            2_000.0,
            0.8,
            0.04,
        ),
        0.0,
        0.5,
    );
    mix_into(
        &mut out,
        &burst(
            rng.next_u64(),
            NoiseColor::Brown,
            FilterMode::LowPass,
            600.0,
            0.7,
            0.8,
        ),
        0.02,
        0.8,
    );
    grains(rng, &mut out, 30, (0.05, 1.1), (800.0, 3_200.0), 0.35);
    out
}

/// The matriarch's wail: two FM voices rising and falling with a slow, uneasy vibrato.
fn matriarch_wail(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    let base = rng.range(560.0, 640.0);
    for (delay, gain, detune) in [(0.0, 0.5, 1.0), (0.12, 0.3, 1.5)] {
        let mut fm = Fm::default();
        let wail = render(1.3, |t| {
            let shape = segments(t / 1.3, &[(0.0, 0.8), (0.3, 1.25), (0.8, 1.0), (1.0, 0.7)]);
            let pitch = base * detune * shape * (1.0 + 0.02 * sine(t, 5.5));
            fm.next(pitch, 1.0, 1.1)
                * segments(t / 1.3, &[(0.0, 0.0), (0.15, 1.0), (0.8, 0.7), (1.0, 0.0)])
        });
        mix_into(&mut out, &wail, delay, gain);
    }
    out
}

// --- Top-down crypts (WP-TOMBS T29) -------------------------------------------------------

/// A dodge roll: cloth and gear swishing past (a pink-noise sweep) and a soft shoulder thud.
fn dodge_roll(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 900.0, 1.2);
    let top = 2_400.0 * rng.jitter(0.1);
    let mut out = render(0.32, |t| {
        let cutoff = segments(t, &[(0.0, 600.0), (0.12, top), (0.3, 700.0)]);
        band.run(noise.next(), cutoff, 1.2) * segments(t, &[(0.0, 0.0), (0.05, 0.8), (0.3, 0.0)])
    });
    mix_into(&mut out, &thump(150.0, 70.0, 0.12, 1.3), 0.2, 0.5);
    out
}

/// A trap's arrow loosed: a taut string's twang (a plucked saw dropping in pitch) and the
/// shaft hissing away.
fn arrow_twang(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Saw);
    let mut low = Svf::new(FilterMode::LowPass, 2_400.0, 2.0);
    let base = 196.0 * rng.jitter(0.05);
    let mut out = render(0.35, |t| {
        low.process(osc.next(glide(t, base * 1.08, base, 0.02))) * perc(t, 0.001, 0.25) * 0.8
    });
    let hiss = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        5_200.0,
        1.5,
        0.18,
    );
    mix_into(&mut out, &hiss, 0.03, 0.35);
    out
}

/// Floor spikes shooting up: a bright metallic shing over a stony knock.
fn spike_shing(rng: &mut Rng) -> Vec<f32> {
    let mut out = clank(rng, 1_250.0, 0.35);
    for sample in &mut out {
        *sample *= 0.6;
    }
    mix_into(&mut out, &thump(260.0, 120.0, 0.08, 1.6), 0.0, 0.5);
    let scrape = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        3_500.0,
        0.8,
        0.12,
    );
    mix_into(&mut out, &scrape, 0.0, 0.4);
    out
}

/// A fall into a pit: a whistle dropping away, then a far thud and loose pebbles.
fn pit_drop(rng: &mut Rng) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let start = 900.0 * rng.jitter(0.05);
    let mut out = render(0.6, |t| {
        osc.next(glide(t, start, start * 0.35, 0.25))
            * segments(t, &[(0.0, 0.0), (0.04, 0.4), (0.55, 0.0)])
    });
    mix_into(&mut out, &thump(110.0, 45.0, 0.3, 1.8), 0.55, 0.9);
    grains(rng, &mut out, 10, (0.6, 1.0), (1_200.0, 3_500.0), 0.3);
    out
}

/// The crypt lord's roar: a growling low saw through a sweeping vowel filter, breath noise and a
/// tail of reverb-ready rumble.
fn lord_roar(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw);
    let mut sub = Osc::new(Wave::Triangle);
    let mut formant = Svf::new(FilterMode::BandPass, 500.0, 2.5);
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breath_band = Svf::new(FilterMode::BandPass, 1_200.0, 1.0);
    let base = 62.0 * rng.jitter(0.03);
    render(1.7, |t| {
        let wobble = 1.0 + 0.06 * sine(t, 9.0) + 0.03 * sine(t, 23.0);
        let pitch = glide(t, base * 0.8, base * 1.15, 0.3) * wobble;
        let vowel = segments(t, &[(0.0, 380.0), (0.4, 900.0), (1.2, 650.0), (1.7, 300.0)]);
        let voice = formant.run(saw.next(pitch), vowel, 2.5) * 1.6 + sub.next(pitch * 0.5) * 0.5;
        let air = breath_band.process(breath.next()) * 0.35;
        saturate(voice + air, 2.2) * segments(t, &[(0.0, 0.0), (0.15, 1.0), (1.2, 0.8), (1.7, 0.0)])
    })
}

/// Stone on stone: a golem's slam or a boulder let loose (a deep saturated thump and debris).
fn stone_slam(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(110.0 * rng.jitter(0.05), 48.0, 0.4, 2.4);
    for sample in &mut out {
        *sample *= 0.45;
    }
    let crack = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::BandPass,
        1_800.0,
        0.8,
        0.25,
    );
    mix_into(&mut out, &crack, 0.0, 0.8);
    grains(rng, &mut out, 22, (0.03, 0.7), (1_200.0, 4_000.0), 0.6);
    out
}

/// A hex being cast: an FM swirl rising to a ringing point.
fn hex_bolt(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let base = 330.0 * rng.jitter(0.04);
    render(0.45, |t| {
        let frequency = glide(t, base, base * 2.0, 0.15);
        fm.next(frequency, 1.41, 3.0 * decay(t, 0.2))
            * segments(t, &[(0.0, 0.0), (0.06, 0.7), (0.45, 0.0)])
    })
}

/// The crypt's music: a low organ drone (D and A, breathing over the loop), a hollow choir pad
/// and a slow bell motif (D, F, E, A) with a far echo. Frequencies are on the loop's grid.
fn crypt_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut root = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut pad = [
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
        Osc::new(Wave::Saw),
    ];
    let mut pad_filter = Svf::new(FilterMode::LowPass, 700.0, 0.8);
    let mut wind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut wind_band = Svf::new(FilterMode::BandPass, 1_600.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let breath = 0.75 + 0.25 * sine(t, 1.0 / SECONDS);
        let drone = root.next(73.5) * 0.5 + fifth.next(110.0) * 0.3 * breath;
        let choir: f32 = pad
            .iter_mut()
            .zip([146.75_f32, 174.5, 220.0])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            / 3.0;
        let choir = pad_filter.process(choir) * (0.55 + 0.45 * sine(t, 0.5 / SECONDS).abs());
        let air = wind_band.process(wind.next()) * 0.3;
        drone * 0.3 + choir * 0.3 * breath + air
    });
    for (index, note) in [587.5_f32, 698.5, 659.0, 440.0].into_iter().enumerate() {
        let start = index as f32 * (SECONDS / 4.0) + 0.05;
        mix_into(&mut out, &bell(note, 2.0, 0.9, 1.4), start, 0.2);
        mix_into(&mut out, &bell(note, 2.0, 0.6, 1.0), start + 0.42, 0.08);
    }
    out
}

/// The crypt lord's theme: a war drum pulsing twice a second with a tom answer, a growling saw
/// ostinato (D D F E) and dissonant brass stabs on the downbeats.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the loop's step index"
)]
fn crypt_boss_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 400.0, 1.4);
    let notes = [73.5_f32, 73.5, 87.25, 82.5];
    let mut out = render(loop_length(), |t| {
        let step = ((t / (SECONDS / 8.0)) as usize) % 8;
        let local = t % (SECONDS / 8.0);
        let note = notes[step / 2];
        let gate = perc(local, 0.004, 0.35);
        saturate(bass_filter.process(bass.next(note)) * gate * 1.4, 1.6) * 0.3
    });
    for beat in 0..8 {
        let at = beat as f32 * (SECONDS / 8.0);
        mix_into(&mut out, &thump(80.0, 42.0, 0.28, 2.0), at, 0.4);
        if beat % 2 == 1 {
            mix_into(
                &mut out,
                &thump(160.0, 110.0, 0.16, 1.4),
                at + SECONDS / 16.0,
                0.3,
            );
        }
    }
    // A bright tick on every sixteenth: the drums' metal.
    for step in 0..16_u8 {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::HighPass,
            6_000.0,
            0.8,
            0.04,
        );
        let level = if step % 4 == 0 { 0.35 } else { 0.2 };
        mix_into(&mut out, &tick, f32::from(step) * (SECONDS / 16.0), level);
    }
    for (start, chord) in [
        (0.0, [587.5_f32, 622.5, 880.0]),
        (2.0, [523.5, 554.5, 784.0]),
    ] {
        for note in chord {
            mix_into(&mut out, &brass(rng, note, 0.35, 0.9), start, 0.16);
        }
    }
    grains_loop(rng, &mut out);
    out
}

/// Dust trickling evenly over a crypt loop (nothing bunched at the seam).
fn grains_loop(rng: &mut Rng, out: &mut Vec<f32>) {
    scatter_grains(
        rng,
        out,
        loop_events(3.0),
        (0.0, loop_length()),
        (1_500.0, 4_000.0),
        0.08,
        false,
    );
}

// ---------------------------------------------------------------------------------------------
// The Wave 6 audio pass: ambience by environment, the clock's themes, footsteps on the Moon
// ---------------------------------------------------------------------------------------------

/// The thin air high up (Wave 6): a faint whistle wandering over a breathy high hiss, swelling in
/// slow gusts; no low buffeting, there is too little air left to push anything about.
fn high_wind_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::HighPass, 2_400.0, 0.6);
    let mut whistle = Noise::new(NoiseColor::White, rng.next_u64());
    let mut whistle_filter = Svf::new(FilterMode::BandPass, 3_000.0, 6.0);
    let mut gust = flutter(rng, 0.6, 0.4);
    render(loop_length(), |t| {
        let level = gust();
        let centre = 3_000.0 + 700.0 * sine(t, 0.25) + 250.0 * sine(t, 1.0);
        hiss_filter.process(hiss.next()) * 0.4 * level
            + whistle_filter.run(whistle.next(), centre, 6.0) * 0.6 * level * level
    })
}

/// The Moon's near-silence (Wave 6): no air carries anything, so only what the suit hears of
/// itself: the life-support hum, the slow tide of breath in the helmet, a faint servo whine and
/// now and then a tick of radio static.
fn moon_loop(rng: &mut Rng) -> Vec<f32> {
    let mut hum = Osc::new(Wave::Sine);
    let mut hum_octave = Osc::new(Wave::Sine);
    let mut servo = Osc::new(Wave::Sine);
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breath_filter = Svf::new(FilterMode::BandPass, 650.0, 0.8);
    let mut out = render(loop_length(), |t| {
        // Two breaths a loop: in (brighter), out (softer).
        let tide = sine(t, 0.5).max(0.0).powi(2) + 0.4 * (-sine(t, 0.5)).max(0.0).powi(2);
        let air = breath_filter.process(breath.next()) * tide * 0.9;
        let life = hum.next(110.0) * 0.12 + hum_octave.next(220.0) * 0.06;
        let whine = servo.next(1_760.0) * 0.025 * (0.5 + 0.5 * sine(t, 0.25));
        life + air + whine
    });
    for time in scatter(rng, loop_events(0.8), 0.0, loop_length() - 0.1, 1.0) {
        let tick = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            rng.range(2_200.0, 3_600.0),
            1.5,
            0.025,
        );
        mix_into(&mut out, &tick, time, rng.range(0.08, 0.16));
    }
    out
}

/// A boot coming down on the Moon (Wave 6): felt more than heard, a soft low thud through the
/// suit, a regolith crunch muffled to almost nothing and the knee servo's short whirr.
fn moon_step(rng: &mut Rng) -> Vec<f32> {
    let start = rng.range(230.0, 270.0);
    let mut out = thump(start, start * 0.55, 0.14, 0.2);
    let crunch = burst(
        rng.next_u64(),
        NoiseColor::Pink,
        FilterMode::LowPass,
        rng.range(1_400.0, 1_800.0),
        0.7,
        0.08,
    );
    mix_into(&mut out, &crunch, 0.004, 0.6);
    let mut servo = Osc::new(Wave::Saw);
    let mut filter = Svf::new(FilterMode::LowPass, 1_400.0, 0.9);
    let pitch = rng.range(520.0, 640.0);
    let whirr = render(0.12, |t| {
        filter.process(servo.next(glide(t, pitch, pitch * 1.5, 0.05))) * perc(t, 0.01, 0.1)
    });
    mix_into(&mut out, &whirr, 0.03, 0.12);
    out
}

/// A town's bustle (Wave 6), in villages and sky towns: a murmur of far voices (noise through
/// drifting vowel formants, in syllables), a smith's hammer, a creaking cart and birds.
fn town_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut voices = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut low_formant = Svf::new(FilterMode::BandPass, 520.0, 3.0);
    let mut high_formant = Svf::new(FilterMode::BandPass, 1_500.0, 3.5);
    let mut syllables = flutter(rng, 4.0, 0.6);
    let mut crowd = flutter(rng, 0.4, 0.3);
    let mut out = render(loop_length(), |t| {
        let source = voices.next();
        let vowel = sine(t, 0.75);
        let murmur = low_formant.run(source, 520.0 + 160.0 * vowel, 3.0)
            + high_formant.run(source, 1_500.0 - 400.0 * vowel, 3.5) * 0.6;
        murmur * syllables().max(0.4) * crowd() * 0.6
    });
    // A smith working three strikes and a rest, twice a loop.
    for bar in 0..2_u8 {
        for strike in 0..3_u8 {
            let time = f32::from(bar) * (SECONDS / 2.0) + f32::from(strike) * 0.32 + 0.1;
            let pitch = 1_900.0 * rng.jitter(0.03);
            mix_into(&mut out, &clank(rng, pitch, 0.12), time, 0.2);
        }
    }
    // A cart's wheel creaking past.
    let mut creak = Osc::new(Wave::Saw);
    let mut creak_filter = Svf::new(FilterMode::BandPass, 900.0, 4.0);
    let squeak = render(0.5, |t| {
        creak_filter.process(creak.next(glide(t, 380.0, 520.0, 0.2)))
            * segments(t, &[(0.0, 0.0), (0.1, 1.0), (0.35, 0.6), (0.5, 0.0)])
    });
    mix_into(&mut out, &squeak, 2.6, 0.12);
    // Birds on the roofs.
    for time in scatter(rng, loop_events(1.0), 0.0, loop_length() - 0.2, 1.0) {
        let mut osc = Osc::new(Wave::Sine);
        let pitch = rng.range(2_800.0, 3_800.0);
        let tweet = render(0.08, |t| {
            osc.next(glide(t, pitch, pitch * 1.3, 0.05)) * perc(t, 0.004, 0.07)
        });
        mix_into(&mut out, &tweet, time, rng.range(0.05, 0.1));
    }
    out
}

/// The lunar colony's rooms (Wave 6): air recyclers humming, a fan's slow beat, a vent's steady
/// hiss and soft console chirps.
fn colony_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut motor = Osc::new(Wave::Saw);
    let mut motor_filter = Svf::new(FilterMode::LowPass, 280.0, 0.8);
    let mut vent = Noise::new(NoiseColor::White, rng.next_u64());
    let mut vent_filter = Svf::new(FilterMode::BandPass, 3_800.0, 0.8);
    let mut room = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut room_filter = Svf::new(FilterMode::LowPass, 200.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let fan = 0.8 + 0.2 * sine(t, 3.0);
        motor_filter.process(motor.next(55.0)) * 0.5 * fan
            + vent_filter.process(vent.next()) * 0.18
            + room_filter.process(room.next()) * 0.4
    });
    for index in 0..4_u8 {
        let time = f32::from(index) * (SECONDS / 4.0) + 0.3;
        let note = if index % 2 == 0 { 1_318.5 } else { 1_760.0 };
        mix_into(&mut out, &beep(note, 0.05, 0.5, 0.0), time, 0.06);
        mix_into(
            &mut out,
            &beep(note * 1.5, 0.05, 0.5, 0.0),
            time + 0.08,
            0.04,
        );
    }
    out
}

/// Dawn's theme (Wave 6, on the world clock): a warm G major pad swelling up from the root, a
/// rising pentatonic line of bells and a held high bell as the sun clears the hills.
fn dawn_theme(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [196.0, 293.66, 392.0].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 2.4 - index as f32 * 0.2, 0.3),
            index as f32 * 0.22,
            0.16,
        );
    }
    for (index, note) in [392.0, 440.0, 493.88, 587.33, 659.25, 783.99]
        .into_iter()
        .enumerate()
    {
        mix_into(
            &mut out,
            &bell(note, 2.0, 0.8, 1.3),
            0.3 + index as f32 * 0.26,
            0.2,
        );
    }
    mix_into(&mut out, &bell(1_174.66, 3.0, 0.6, 1.6), 1.85, 0.16);
    out
}

/// Dusk's theme (Wave 6, on the world clock): a low E minor pad settling, a falling aeolian line
/// on soft bells and a far horn note fading into the evening.
fn dusk_theme(rng: &mut Rng) -> Vec<f32> {
    let mut out = Vec::new();
    for (index, note) in [164.81, 196.0, 246.94].into_iter().enumerate() {
        mix_into(
            &mut out,
            &brass(rng, note, 2.6, 0.22),
            index as f32 * 0.18,
            0.15,
        );
    }
    for (index, note) in [659.25, 587.33, 493.88, 440.0, 392.0, 329.63]
        .into_iter()
        .enumerate()
    {
        mix_into(
            &mut out,
            &bell(note, 1.0, 0.5, 1.5),
            0.25 + index as f32 * 0.3,
            0.18,
        );
    }
    mix_into(&mut out, &brass(rng, 164.81, 1.0, 0.15), 2.1, 0.12);
    out
}

/// One rising bubble: a short sine chirp sweeping up (the "bloop" of air leaving water).
fn bubble(rng: &mut Rng, low: f32, high: f32) -> Vec<f32> {
    let mut osc = Osc::new(Wave::Sine);
    let base = rng.range(low, high);
    let length = rng.range(0.03, 0.07);
    render(length, |t| {
        osc.next(base * (1.0 + t / length * 1.6)) * perc(t, 0.002, length * 0.8)
    })
}

/// A fluid pocket breached (Part III F9): a low gulp as the wall gives, then a broad, churning
/// gush of brown and pink noise that swells and drains away, with bubbles bursting through it.
fn fluid_rush(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(110.0 * rng.jitter(0.05), 55.0, 0.18, 1.4);
    let mut rush = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rush_filter = Svf::new(FilterMode::LowPass, 700.0, 0.8);
    let mut spray = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut spray_filter = Svf::new(FilterMode::BandPass, 1_600.0, 0.7);
    let churn = rng.range(5.0, 7.0);
    let gush = render(1.6, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.6, 0.8), (1.6, 0.0)]);
        let wobble = 0.75 + 0.25 * sine(t, churn);
        let low = rush_filter.run(rush.next(), 380.0 + 500.0 * swell, 0.8);
        let high = spray_filter.run(spray.next(), 1_200.0 + 900.0 * wobble, 0.7);
        (low * 1.1 + high * 0.45) * swell * wobble
    });
    mix_into(&mut out, &gush, 0.03, 0.9);
    for time in scatter(rng, 14, 0.1, 1.4, 1.4) {
        let pop = bubble(rng, 300.0, 900.0);
        mix_into(&mut out, &pop, time, rng.range(0.08, 0.2));
    }
    out
}

/// Water meeting lava or molten metal: a band-passed sizzle sweeping down into a breathy steam
/// hiss over a low boil, fading, with a few crackles as the crust sets.
fn steam_hiss(rng: &mut Rng) -> Vec<f32> {
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 3_000.0, 0.8);
    let mut body = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut body_filter = Svf::new(FilterMode::LowPass, 500.0, 0.7);
    let mut out = render(1.1, |t| {
        let open = segments(t, &[(0.0, 4_200.0), (0.15, 2_000.0), (1.1, 2_800.0)]);
        let shape = segments(t, &[(0.0, 0.0), (0.02, 1.0), (0.3, 0.6), (1.1, 0.0)]);
        (filter.run(hiss.next(), open, 0.8) + body_filter.process(body.next()) * 0.4) * shape
    });
    grains(rng, &mut out, 9, (0.05, 0.9), (2_000.0, 4_500.0), 0.2);
    out
}

/// Acid eating rock: a thick, fast fizz of tiny bubbles over a low, sour gurgle.
fn acid_fizz(rng: &mut Rng) -> Vec<f32> {
    let mut fizz = Noise::new(NoiseColor::White, rng.next_u64());
    let mut fizz_filter = Svf::new(FilterMode::BandPass, 4_200.0, 1.2);
    let mut gurgle = Osc::new(Wave::Sine);
    let wobble = rng.range(9.0, 13.0);
    let mut out = render(0.9, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.5, 0.7), (0.9, 0.0)]);
        let low = gurgle.next(140.0 + 40.0 * sine(t, wobble)) * 0.3;
        (fizz_filter.process(fizz.next()) * 0.7 + low) * shape
    });
    for time in scatter(rng, 18, 0.0, 0.8, 1.2) {
        let pop = bubble(rng, 1_200.0, 2_800.0);
        mix_into(&mut out, &pop, time, rng.range(0.05, 0.12));
    }
    out
}

/// A rig plunging under: a heavy body-hit into water (deeper and longer than a pilot's splash),
/// a wall of spray and a trail of big bubbles rising past the hull.
fn rig_splash(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(95.0 * rng.jitter(0.05), 45.0, 0.22, 1.6);
    let mut noise = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut filter = Svf::new(FilterMode::BandPass, 1_800.0, 0.6);
    let spray = render(0.6, |t| {
        filter.run(noise.next(), 3_200.0 - 2_200.0 * t / 0.6, 0.6) * perc(t, 0.004, 0.35)
    });
    mix_into(&mut out, &spray, 0.0, 0.8);
    for index in 0..8_u8 {
        let pop = bubble(rng, 220.0, 650.0);
        mix_into(&mut out, &pop, 0.15 + f32::from(index) * 0.07, 0.2);
    }
    out
}

/// Under water (the director's loop while a local rig is submerged): a muffled low rumble of
/// the pumps and a steady, even stream of bubbles (spread evenly so nothing pulses at the
/// loop rate).
fn bubble_loop(rng: &mut Rng) -> Vec<f32> {
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_filter = Svf::new(FilterMode::LowPass, 260.0, 0.7);
    let mut pump = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        let low = rumble_filter.process(rumble.next()) * 0.9;
        let hum = pump.next(62.0) * (0.08 + 0.03 * sine(t, 0.5));
        low + hum
    });
    for time in scatter(rng, loop_events(6.0), 0.0, loop_length() - 0.1, 1.0) {
        let pop = bubble(rng, 260.0, 1_100.0);
        mix_into(&mut out, &pop, time, rng.range(0.05, 0.13));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The authored Crust (Wave 8): its one-shots, each stratum's music bed and the Sinkhole chase.
// Loops keep to whole cycles in LOOP_SECONDS and spread their events evenly (see above).
// ---------------------------------------------------------------------------------------------

/// A mine cart coupling: a heavy iron clunk over a wheel thud, and a short rattle of chain.
fn cart_clunk(rng: &mut Rng) -> Vec<f32> {
    let base = 170.0 * rng.jitter(0.04);
    let mut out = clank(rng, base, 0.4);
    mix_into(&mut out, &thump(140.0, 70.0, 0.16, 1.3), 0.0, 0.7);
    for index in 0..4_u8 {
        let pitch = rng.range(900.0, 1_400.0);
        let link = clank(rng, pitch, 0.06);
        mix_into(&mut out, &link, 0.05 + f32::from(index) * 0.045, 0.22);
    }
    out
}

/// A rail switch thrown at speed: two sharp iron clacks and a short squeal from the points.
fn cart_switch(rng: &mut Rng) -> Vec<f32> {
    let base = 620.0 * rng.jitter(0.03);
    let mut out = clank(rng, base, 0.14);
    mix_into(&mut out, &clank(rng, 540.0, 0.14), 0.085, 0.9);
    let mut squeal = Osc::new(Wave::Saw);
    let mut band = Svf::new(FilterMode::BandPass, 2_400.0, 4.0);
    let whine = render(0.2, |t| {
        band.process(squeal.next(glide(t, 1_850.0, 1_500.0, 0.15))) * perc(t, 0.01, 0.16)
    });
    mix_into(&mut out, &whine, 0.02, 0.18);
    out
}

/// Timber groaning under load: a slow, resonant saw creak that bends as the wood strains, a
/// few dry cracks of splitting grain and a sharp snap at the end.
fn timber_creak(rng: &mut Rng) -> Vec<f32> {
    let mut wood = Osc::new(Wave::Saw);
    let mut body = Svf::new(FilterMode::BandPass, 650.0, 3.2);
    let bend = rng.range(3.0, 4.5);
    let mut out = render(1.3, |t| {
        let pitch = 62.0 + 9.0 * sine(t, bend) + 14.0 * t;
        let shape = segments(t, &[(0.0, 0.0), (0.2, 1.0), (0.9, 0.8), (1.3, 0.0)]);
        saturate(body.process(wood.next(pitch)) * 1.6, 1.3) * shape
    });
    grains(rng, &mut out, 10, (0.15, 1.1), (1_800.0, 4_200.0), 0.3);
    let snap = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::BandPass,
        2_600.0,
        1.1,
        0.04,
    );
    mix_into(&mut out, &snap, 1.12, 0.7);
    out
}

/// A roof coming down: a crack, a heavy thump, a long falling rumble and a rain of rock, with
/// the clatter of broken timber.
fn roof_collapse(rng: &mut Rng) -> Vec<f32> {
    let mut out = burst(
        rng.next_u64(),
        NoiseColor::White,
        FilterMode::HighPass,
        2_000.0,
        0.8,
        0.06,
    );
    mix_into(
        &mut out,
        &thump(95.0 * rng.jitter(0.05), 36.0, 0.55, 2.2),
        0.01,
        1.0,
    );
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut low = Svf::new(FilterMode::LowPass, 420.0, 0.8);
    let roll = render(1.6, |t| {
        let cutoff = segments(t, &[(0.0, 520.0), (1.6, 140.0)]);
        low.run(rumble.next(), cutoff, 0.8)
            * segments(t, &[(0.0, 0.0), (0.05, 1.0), (0.7, 0.7), (1.6, 0.0)])
    });
    mix_into(&mut out, &roll, 0.02, 1.1);
    grains(rng, &mut out, 70, (0.03, 1.5), (700.0, 3_500.0), 0.45);
    for index in 0..3_u8 {
        let pitch = rng.range(180.0, 260.0);
        let plank = clank(rng, pitch, 0.2);
        mix_into(&mut out, &plank, 0.1 + f32::from(index) * 0.21, 0.3);
    }
    out
}

/// The Sinkhole letting go: a sub glide swelling for three seconds under a grinding rumble that
/// churns faster, and the first stones raining down.
fn sinkhole_rumble(rng: &mut Rng) -> Vec<f32> {
    let mut sub = Osc::new(Wave::Sine);
    let mut grind = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut grind_band = Svf::new(FilterMode::BandPass, 900.0, 1.2);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_low = Svf::new(FilterMode::LowPass, 300.0, 0.8);
    let mut out = render(3.2, |t| {
        let swell = segments(t, &[(0.0, 0.0), (2.6, 1.0), (3.2, 0.0)]);
        let churn = 0.6 + 0.4 * sine(t, 4.0 + 5.0 * t / 3.2);
        let deep = saturate(sub.next(glide(t, 50.0, 31.0, 2.8)) * 1.4, 1.8) * 0.6;
        let grit = grind_band.process(grind.next()) * churn * 0.5;
        (deep + grit + rumble_low.process(rumble.next())) * swell
    });
    for time in scatter(rng, 30, 0.6, 3.0, 0.6) {
        let pitch = rng.range(300.0, 900.0);
        let stone = clank(rng, pitch, 0.08);
        mix_into(&mut out, &stone, time, rng.range(0.06, 0.18));
    }
    out
}

/// The Deep Claim won: a horn call rising G-C-E-G over a held brass chord, a run of bells and
/// a long ringing tail.
fn deep_claim_fanfare(rng: &mut Rng) -> Vec<f32> {
    let mut out = render(3.4, |_| 0.0);
    for (index, note) in [392.0_f32, 523.25, 659.25, 783.99].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "four notes")]
        let start = index as f32 * 0.16;
        mix_into(&mut out, &brass(rng, note, 0.22, 1.0), start, 0.3);
    }
    for note in [261.63_f32, 329.63, 392.0, 523.25] {
        mix_into(&mut out, &brass(rng, note, 1.6, 0.7), 0.64, 0.18);
    }
    for (index, note) in [1_046.5_f32, 1_318.5, 1_568.0, 2_093.0]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "four bells")]
        let start = 0.7 + index as f32 * 0.12;
        mix_into(&mut out, &bell(note, 3.5, 1.2, 1.6), start, 0.12);
    }
    mix_into(&mut out, &thump(110.0, 55.0, 0.5, 1.5), 0.64, 0.45);
    out
}

/// A lava tube's hot breath: a slow whoosh of warm air rising and falling, crackle along the
/// glass and a tick of it cooling.
fn tube_breath(rng: &mut Rng) -> Vec<f32> {
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut band = Svf::new(FilterMode::BandPass, 500.0, 0.7);
    let mut out = render(1.6, |t| {
        let shape = segments(t, &[(0.0, 0.0), (0.5, 1.0), (1.6, 0.0)]);
        band.run(air.next(), 350.0 + 700.0 * shape, 0.7) * shape
    });
    grains(rng, &mut out, 16, (0.1, 1.4), (2_500.0, 5_000.0), 0.25);
    mix_into(&mut out, &clank(rng, 2_400.0, 0.05), 1.2, 0.15);
    out
}

/// The Topsoil (looped while a seat digs there): a fingerpicked folk line in G major
/// pentatonic over a soft open-G drone, roots creaking and the scratch of burrowers.
fn crust_topsoil_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut creak = Osc::new(Wave::Saw);
    let mut creak_band = Svf::new(FilterMode::BandPass, 520.0, 3.0);
    let mut out = render(loop_length(), |t| {
        let swell = 0.8 + 0.2 * sine(t, 0.25);
        let drone = (low.next(98.0) * 0.5 + fifth.next(147.0) * 0.25) * swell * 0.35;
        // A root creaking twice a loop, slow and quiet.
        let creaking = creak_band.process(creak.next(58.0 + 6.0 * sine(t, 0.5)))
            * (0.5 + 0.5 * sine(t, 0.5)).powi(4)
            * 0.12;
        drone + creaking
    });
    let eighth = LOOP_SECONDS / 8.0;
    let line: [Option<f32>; 8] = [
        Some(392.0),
        Some(440.0),
        Some(587.33),
        None,
        Some(493.88),
        Some(440.0),
        Some(392.0),
        Some(329.63),
    ];
    for (step, note) in line.into_iter().enumerate() {
        let Some(note) = note else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = step as f32 * eighth;
        let pluck = bell(note * rng.jitter(0.002), 1.0, 1.3, 1.1);
        loop_note(&mut out, &pluck, start, 0.15);
        // The thumb's low string on the beat.
        if step % 2 == 0 {
            let thumb = bell(196.0, 1.0, 0.9, 0.8);
            loop_note(&mut out, &thumb, start, 0.08);
        }
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(4.0),
        (0.0, loop_length()),
        (300.0, 1_200.0),
        0.05,
        false,
    );
    out
}

/// The Clay Beds: a dripping marimba motif in D dorian over a soft A drone, water gurgling
/// behind the clay and clear drips ringing in the dark.
fn crust_clay_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Sine);
    let mut out = render(loop_length(), |t| {
        low.next(110.0) * (0.12 + 0.04 * sine(t, 0.25))
    });
    let step = LOOP_SECONDS / 8.0;
    for (index, note) in [
        293.66_f32, 349.23, 440.0, 392.0, 329.63, 293.66, 261.63, 220.0,
    ]
    .into_iter()
    .enumerate()
    {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let bar = bell(note, 4.0, 0.8, 0.7);
        loop_note(
            &mut out,
            &bar,
            start,
            if index % 4 == 0 { 0.17 } else { 0.12 },
        );
    }
    for time in scatter(rng, loop_events(3.0), 0.0, loop_length() - 0.1, 1.0) {
        let pop = bubble(rng, 180.0, 520.0);
        mix_into(&mut out, &pop, time, rng.range(0.04, 0.09));
    }
    for time in scatter(rng, loop_events(1.2), 0.0, loop_length() - 0.5, 1.0) {
        let drip = bell(rng.range(1_500.0, 2_600.0), 2.0, 0.5, 0.45);
        mix_into(&mut out, &drip, time, 0.05);
    }
    out
}

/// The Grey Stone's old works: a reedy E drone that breathes, a slow work-song line on a low
/// brass voice, far-off pick clinks on the beat and the clack of a rail settling.
fn crust_works_loop(rng: &mut Rng) -> Vec<f32> {
    let mut reed = Osc::new(Wave::Saw);
    let mut reed_band = Svf::new(FilterMode::BandPass, 900.0, 1.4);
    let mut out = render(loop_length(), |t| {
        let breath = 0.7 + 0.3 * sine(t, 0.25);
        reed_band.run(reed.next(82.5), 700.0 + 300.0 * breath, 1.4) * breath * 0.28
    });
    for (start, note, length) in [
        (0.0, 329.63_f32, 0.8),
        (1.0, 392.0, 0.4),
        (1.5, 440.0, 0.4),
        (2.0, 392.0, 0.8),
        (3.0, 293.66, 0.8),
    ] {
        let sung = brass(rng, note / 2.0, length, 0.35);
        loop_note(&mut out, &sung, start, 0.12);
    }
    for beat in 0..4_u8 {
        let pitch = 2_100.0 * rng.jitter(0.05);
        let pick = clank(rng, pitch, 0.12);
        loop_note(&mut out, &pick, f32::from(beat) + 0.5, 0.06);
    }
    let rail = clank(rng, 380.0, 0.25);
    loop_note(&mut out, &rail, 2.75, 0.08);
    out
}

/// The Slate Shelves: a glassy pad in stacked fifths that swells and ebbs, high drips ringing
/// off stalactites and the faint tick of shelves settling.
fn crust_slate_loop(rng: &mut Rng) -> Vec<f32> {
    let mut voices = [
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
        Osc::new(Wave::Triangle),
    ];
    let mut shimmer = Svf::new(FilterMode::LowPass, 1_800.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let swell = 0.6 + 0.4 * sine(t, 0.25).abs();
        let pad: f32 = voices
            .iter_mut()
            .zip([130.5_f32, 196.0, 293.5, 440.0])
            .map(|(osc, note)| osc.next(note))
            .sum::<f32>()
            / 4.0;
        shimmer.process(pad) * swell * 0.35
    });
    for (start, note) in [(0.4_f32, 1_760.0_f32), (1.7, 2_349.0), (2.9, 1_975.5)] {
        let drip = bell(note, 3.5, 1.0, 1.4);
        loop_note(&mut out, &drip, start, 0.08);
    }
    scatter_grains(
        rng,
        &mut out,
        loop_events(2.0),
        (0.0, loop_length()),
        (3_000.0, 6_000.0),
        0.03,
        false,
    );
    out
}

/// The Basalt Columns: a low, hot drone on A with a sour minor second, a heavy slow pulse like
/// the rock breathing, and crackle along the lava tubes.
fn crust_basalt_loop(rng: &mut Rng) -> Vec<f32> {
    let mut root = Osc::new(Wave::Saw);
    let mut second = Osc::new(Wave::Saw);
    let mut low = Svf::new(FilterMode::LowPass, 260.0, 1.1);
    let mut out = render(loop_length(), |t| {
        let heave = 0.7 + 0.3 * sine(t, 0.5);
        let raw = root.next(55.0) * 0.6 + second.next(58.25) * 0.25;
        saturate(low.process(raw) * heave, 1.4) * 0.35
    });
    for beat in [0.0_f32, 2.0] {
        let pulse = thump(70.0, 38.0, 0.6, 1.8);
        loop_note(&mut out, &pulse, beat, 0.35);
    }
    let toll = bell(220.0, 1.41, 1.6, 2.2);
    loop_note(&mut out, &toll, 1.0, 0.07);
    scatter_grains(
        rng,
        &mut out,
        loop_events(10.0),
        (0.0, loop_length()),
        (2_000.0, 5_000.0),
        0.05,
        false,
    );
    out
}

/// The Sinkhole chase (while the collapse climbs): toms driving every eighth (120 bpm), a low
/// A ostinato climbing a half step each bar, brass stabs and the collapse roaring underneath.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the loop's step index"
)]
fn sinkhole_loop(rng: &mut Rng) -> Vec<f32> {
    const SECONDS: f32 = LOOP_SECONDS;
    let mut bass = Osc::new(Wave::Saw);
    let mut bass_filter = Svf::new(FilterMode::LowPass, 500.0, 1.3);
    let mut rumble = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut rumble_low = Svf::new(FilterMode::LowPass, 220.0, 0.8);
    let notes = [110.0_f32, 110.0, 130.81, 116.54];
    let mut out = render(loop_length(), |t| {
        let step = ((t / (SECONDS / 16.0)) as usize) % 16;
        let local = t % (SECONDS / 16.0);
        let note = notes[step / 4];
        let gate = perc(local, 0.003, 0.18);
        let line = saturate(bass_filter.process(bass.next(note)) * gate * 1.5, 1.7) * 0.3;
        line + rumble_low.process(rumble.next()) * 0.45
    });
    for beat in 0..8_u8 {
        let at = f32::from(beat) * (SECONDS / 8.0);
        let tom = thump(if beat % 2 == 0 { 90.0 } else { 140.0 }, 48.0, 0.25, 1.8);
        mix_into(&mut out, &tom, at, if beat % 4 == 0 { 0.5 } else { 0.32 });
    }
    for (start, chord) in [
        (0.0, [440.0_f32, 523.25, 659.25]),
        (2.0, [466.16, 554.37, 698.46]),
    ] {
        for note in chord {
            mix_into(&mut out, &brass(rng, note, 0.3, 1.0), start, 0.13);
        }
    }
    grains_loop(rng, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------
// The Living Dark (Wave 8, Descent III)
// ---------------------------------------------------------------------------------------------

/// A spore pod bursting (or a spore ball landing): a soft, fat "pof" of low noise, then an airy
/// hiss of spores settling, with a few tiny pops.
fn spore_puff(rng: &mut Rng) -> Vec<f32> {
    let mut out = thump(150.0 * rng.jitter(0.06), 70.0, 0.12, 1.1);
    let mut puff = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut puff_filter = Svf::new(FilterMode::LowPass, 900.0, 0.6);
    let mut air = Noise::new(NoiseColor::White, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 5_000.0, 0.8);
    let cloud = render(1.0, |t| {
        let body = puff_filter.run(puff.next(), 1_400.0 - 900.0 * (t / 1.0).min(1.0), 0.6)
            * perc(t, 0.006, 0.25);
        let settle =
            air_filter.process(air.next()) * segments(t, &[(0.0, 0.0), (0.08, 0.5), (1.0, 0.0)]);
        body * 0.9 + settle * 0.25
    });
    mix_into(&mut out, &cloud, 0.0, 0.9);
    grains(rng, &mut out, 7, (0.05, 0.7), (3_000.0, 6_000.0), 0.12);
    out
}

/// Roots tightening: a slow wooden creak (a resonant band-passed saw bending in pitch) and a
/// few fibrous cracks.
fn root_creak(rng: &mut Rng) -> Vec<f32> {
    let mut saw = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut body = Svf::new(FilterMode::BandPass, 420.0, 3.0);
    let base = rng.range(70.0, 95.0);
    let bend = rng.range(0.15, 0.3);
    let mut out = render(1.1, |t| {
        let pitch = base * (1.0 + bend * sine(t, 1.3) * (t / 1.1));
        let shape = segments(t, &[(0.0, 0.0), (0.12, 1.0), (0.8, 0.7), (1.1, 0.0)]);
        saturate(
            body.run(saw.next(pitch), 380.0 + 140.0 * sine(t, 2.1), 3.0),
            1.6,
        ) * shape
            * 0.6
    });
    grains(rng, &mut out, 5, (0.1, 0.9), (900.0, 2_400.0), 0.25);
    out
}

/// Flesh knitting shut: a wet, low squelch that sucks closed.
fn flesh_squelch(rng: &mut Rng) -> Vec<f32> {
    let mut noise = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut filter = Svf::new(FilterMode::LowPass, 500.0, 2.2);
    let mut gulp = Osc::new(Wave::Sine);
    let pitch = rng.range(90.0, 120.0);
    render(0.55, |t| {
        let open = segments(t, &[(0.0, 300.0), (0.12, 1_100.0), (0.55, 200.0)]);
        let wet = filter.run(noise.next(), open, 2.2) * perc(t, 0.01, 0.4);
        let low = gulp.next(pitch * (1.0 - 0.4 * t / 0.55)) * perc(t, 0.02, 0.3) * 0.5;
        saturate(wet * 1.2 + low, 1.4)
    })
}

/// The Amber Court's chime: a warm buzzing fifth and three marimba notes rising (a deal, an
/// errand, a change of heart).
fn hive_chime(rng: &mut Rng) -> Vec<f32> {
    let mut buzz = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut buzz_filter = Svf::new(FilterMode::BandPass, 600.0, 1.4);
    let mut out = render(0.9, |t| {
        let wing = 1.0 + 0.02 * sine(t, 38.0);
        buzz_filter.process(buzz.next(196.0 * wing)) * perc(t, 0.05, 0.7) * 0.25
    });
    for (index, note) in [392.0_f32, 493.88, 587.33].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "three notes")]
        let start = index as f32 * 0.09;
        let marimba = bell(note * rng.jitter(0.003), 4.0, 0.8, 0.45);
        mix_into(&mut out, &marimba, start, 0.5);
    }
    out
}

/// The Mycelial Mind's roar: a vast, warbling FM growl over a sub swell, its breath full of
/// spores.
fn mind_roar(rng: &mut Rng) -> Vec<f32> {
    let mut fm = Fm::default();
    let mut sub = Osc::new(Wave::Sine);
    let mut breath = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut breath_filter = Svf::new(FilterMode::BandPass, 900.0, 0.7);
    let base = rng.range(48.0, 56.0);
    render(2.2, |t| {
        let swell = segments(t, &[(0.0, 0.0), (0.35, 1.0), (1.4, 0.8), (2.2, 0.0)]);
        let warble = 1.0 + 0.06 * sine(t, 5.5) + 0.03 * sine(t, 1.7);
        let growl = fm.next(base * 2.0 * warble, 1.41, 3.5 * swell);
        let low = sub.next(base * (1.0 - 0.15 * t / 2.2));
        let air = breath_filter.run(breath.next(), 700.0 + 900.0 * swell, 0.7);
        saturate(growl * 0.5 + low * 0.6 + air * 0.4 * swell, 2.0) * swell
    })
}

/// Fungal Forest music: a breathing violet pad (a soft fifth that swells), slow plucked glass
/// bells on a pentatonic line and puffs of spore hiss.
fn fungal_loop(rng: &mut Rng) -> Vec<f32> {
    let mut low = Osc::new(Wave::Triangle);
    let mut fifth = Osc::new(Wave::Triangle);
    let mut air = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut air_filter = Svf::new(FilterMode::BandPass, 2_400.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let swell = 0.7 + 0.3 * sine(t, 0.25);
        (low.next(110.0) * 0.45 + fifth.next(164.81) * 0.25) * swell * 0.35
            + air_filter.process(air.next()) * 0.03 * (0.5 + 0.5 * sine(t, 0.5))
    });
    let step = LOOP_SECONDS / 8.0;
    let line: [Option<f32>; 8] = [
        Some(659.26),
        None,
        Some(587.33),
        Some(493.88),
        None,
        Some(440.0),
        Some(493.88),
        None,
    ];
    for (index, note) in line.into_iter().enumerate() {
        let Some(note) = note else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let pluck = bell(note * rng.jitter(0.002), 3.5, 1.2, 1.1);
        loop_note(&mut out, &pluck, start, 0.12);
    }
    out
}

/// Root Labyrinth music: a low, tightening ostinato of bowed saws (D, E flat, D, C) under a
/// creaking drone; claustrophobic, never resolving.
fn root_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut drone_filter = Svf::new(FilterMode::LowPass, 300.0, 1.2);
    let mut out = render(loop_length(), |t| {
        drone_filter.run(drone.next(55.0), 220.0 + 80.0 * sine(t, 0.5), 1.2) * 0.25
    });
    let step = LOOP_SECONDS / 8.0;
    let line = [
        146.83_f32, 155.56, 146.83, 130.81, 146.83, 155.56, 164.81, 155.56,
    ];
    for (index, note) in line.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "eight steps")]
        let start = index as f32 * step;
        let bow = brass(rng, note, step * 0.8, 0.15);
        loop_note(&mut out, &bow, start, 0.14);
    }
    for beat in [0.0, 2.0] {
        let knock = thump(95.0, 50.0, 0.3, 1.3);
        loop_note(&mut out, &knock, beat, 0.3);
    }
    out
}

/// The Hive's music: a buzzing drone of wings, a busy marimba figure in G and a soft click
/// track of mandibles.
fn hive_loop(rng: &mut Rng) -> Vec<f32> {
    let mut wings = Osc::new(Wave::Saw).with_phase(rng.unit());
    let mut wings_filter = Svf::new(FilterMode::BandPass, 700.0, 1.6);
    let mut out = render(loop_length(), |t| {
        let flutter = 1.0 + 0.015 * sine(t, 30.0);
        wings_filter.process(wings.next(98.0 * flutter)) * (0.14 + 0.04 * sine(t, 0.5))
    });
    let step = LOOP_SECONDS / 16.0;
    let figure = [
        392.0_f32, 493.88, 587.33, 493.88, 440.0, 587.33, 659.26, 587.33,
    ];
    for index in 0..16_usize {
        #[allow(clippy::cast_precision_loss, reason = "sixteen steps")]
        let start = index as f32 * step;
        let note = figure[index % figure.len()];
        let marimba = bell(note * rng.jitter(0.002), 4.0, 0.7, 0.35);
        loop_note(
            &mut out,
            &marimba,
            start,
            if index % 4 == 0 { 0.16 } else { 0.09 },
        );
        let click = burst(
            rng.next_u64(),
            NoiseColor::White,
            FilterMode::BandPass,
            3_500.0,
            2.0,
            0.012,
        );
        loop_note(&mut out, &click, start + step * 0.5, 0.06);
    }
    out
}

/// The Flesh Caverns' heartbeat: four lub-dubs a loop (60 beats a minute at normal speed; the
/// director speeds the loop up with the stratum's heartbeat), a wet throbbing bass and a sour
/// minor-second drone.
fn heartbeat_loop(rng: &mut Rng) -> Vec<f32> {
    let mut drone = Osc::new(Wave::Triangle);
    let mut sour = Osc::new(Wave::Triangle);
    let mut wet = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut wet_filter = Svf::new(FilterMode::LowPass, 240.0, 1.0);
    let mut out = render(loop_length(), |t| {
        let throb = 0.6 + 0.4 * sine(t, 1.0);
        (drone.next(65.41) * 0.35 + sour.next(69.30) * 0.18) * throb * 0.3
            + wet_filter.process(wet.next()) * 0.2 * throb
    });
    for beat in 0..4_u8 {
        let start = f32::from(beat);
        let lub = thump(80.0, 42.0, 0.18, 1.8);
        let dub = thump(70.0, 38.0, 0.14, 1.6);
        loop_note(&mut out, &lub, start, 0.7);
        loop_note(&mut out, &dub, start + 0.28, 0.5);
    }
    out
}

/// Glow Sea music: near silence; slow glass chimes far away in a deep reverb and the sea's low
/// moan.
fn glow_sea_loop(rng: &mut Rng) -> Vec<f32> {
    let mut moan = Osc::new(Wave::Sine);
    let mut deep = Noise::new(NoiseColor::Brown, rng.next_u64());
    let mut deep_filter = Svf::new(FilterMode::LowPass, 160.0, 0.7);
    let mut out = render(loop_length(), |t| {
        let swell = 0.5 + 0.5 * sine(t, 0.25);
        moan.next(41.2 * (1.0 + 0.01 * sine(t, 0.5))) * 0.2 * swell
            + deep_filter.process(deep.next()) * 0.3
    });
    for (start, note) in [(0.3_f32, 1_318.5_f32), (1.9, 987.77), (3.1, 1_174.7)] {
        let chime = bell(note * rng.jitter(0.002), 2.76, 1.4, 2.4);
        loop_note(&mut out, &chime, start, 0.07);
    }
    out
}

/// The Mycelial Mind's fight: pounding low toms on every beat, a choir of detuned saws
/// gliding a minor third up and down and spore hiss swelling with the bar.
fn mind_loop(rng: &mut Rng) -> Vec<f32> {
    let mut voices: Vec<Osc> = (0..3)
        .map(|_| Osc::new(Wave::Saw).with_phase(rng.unit()))
        .collect();
    let mut choir = Svf::new(FilterMode::LowPass, 700.0, 1.0);
    let mut hiss = Noise::new(NoiseColor::Pink, rng.next_u64());
    let mut hiss_filter = Svf::new(FilterMode::BandPass, 3_000.0, 0.8);
    let mut out = render(loop_length(), |t| {
        let pitch = if t % LOOP_SECONDS < LOOP_SECONDS * 0.5 {
            146.83
        } else {
            174.61
        };
        let raw: f32 = voices
            .iter_mut()
            .zip([0.995_f32, 1.0, 1.006])
            .map(|(voice, detune)| voice.next(pitch * detune))
            .sum::<f32>()
            / 3.0;
        choir.run(raw, 600.0 + 200.0 * sine(t, 0.5), 1.0) * 0.22
            + hiss_filter.process(hiss.next()) * 0.04 * (0.5 + 0.5 * sine(t, 0.25))
    });
    let beat = LOOP_SECONDS / 8.0;
    for step in 0..8_u8 {
        let start = f32::from(step) * beat;
        let tom = thump(if step % 2 == 0 { 120.0 } else { 160.0 }, 48.0, 0.25, 1.7);
        loop_note(
            &mut out,
            &tom,
            start,
            if step % 4 == 0 { 0.55 } else { 0.3 },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_names_are_unique() {
        let mut names: Vec<&str> = Patch::ALL.iter().map(|patch| patch.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Patch::ALL.len());
        assert!(
            Patch::ALL
                .iter()
                .enumerate()
                .all(|(index, patch)| patch.index() == index)
        );
    }

    #[test]
    fn renders_are_deterministic_per_seed() {
        for patch in [Patch::Coins, Patch::Explosion, Patch::DrillHardLoop] {
            let first = render_patch(patch, variation_seed(7, patch, 0));
            let again = render_patch(patch, variation_seed(7, patch, 0));
            let other = render_patch(patch, variation_seed(7, patch, 1));
            assert_eq!(first, again, "{} is not deterministic", patch.name());
            assert_ne!(first, other, "{} variations are identical", patch.name());
        }
    }
}
