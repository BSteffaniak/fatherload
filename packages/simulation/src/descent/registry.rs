//! The stratum registry: all 41 strata of the eight Descents (expansion plan §14).
//!
//! Descents I and II are the twelve classic bands (they keep the classic ore curves, hazards,
//! creatures and art; their entries here describe them). Descents III to VIII are placeholders:
//! each borrows a classic band's rules ([`StratumDef::host_band`]) and carries its own palette,
//! ores, hazards, creature table (existing kinds standing in), field profile, cues, landmarks,
//! variants and gate until its Wave authors it for real.

use crate::{
    cave::CaveTile,
    enemy::EnemyKind,
    heart::CoreTile,
    hollow::HollowTile,
    terrain::{DeepStratum, MineralKind, StrategicResourceKind, TileKind},
    worldgen::{Band, HazardKind},
};

use crate::fluid::{
    FluidKind::{Acid, Brine, LiquidMetal, Water},
    FluidPocketRule,
};

use super::{
    AmbienceCue, AuthoredRegions, Descent, DescentBoss, DescentKey, FieldProfile, FieldRegions,
    GateDef, GateRequirement, LandmarkKind, LayerResource, MusicCue, OreCurve, Rgb, ScreenEffect,
    ScreenEffectLevel, Stratum, StratumDef, StratumPalette,
};

use DescentKey as Key;
use EnemyKind::{
    AcidLancer, AcidSpitter, Angler, BarkBeetle, BilePolyp, Bitworm, Burrower, CapCrawler, CaveBat,
    DeadPixel, FossilEel, Gazer, Glassback, Glitchling, GlowGnat, GlowGrub, GlowJelly, HiveDrone,
    HiveSoldier, JetRay, Leechling, Lodestar, LureFish, MagmaSalamander, MagmaWisp, MawWorm,
    MiningDrone, MirrorTwin, PistonWarden, PrismMoth, RockMimic, RogueExcavator, RootSnapper,
    SentryTurret, ShearMantis, Shellback, SporePuffer, ThornWisp, Tumbler, VoidJelly,
};
// The Hollow World's creatures (Wave 9).
use EnemyKind::{
    CinderDrake, Crestback, Fangrunner, Flarefly, GustBladder, Hangclaw, LodeMite, ShadeStalker,
    SkyManta, Skyneck, StoopRoc,
};
use HazardKind::{Explosive, Gas, Lava, MagmaVent, Pressure};
use MineralKind::{Copper, Diamond, Emerald, Gold, Iron, Mythril, Platinum, Ruby, Silver, Uranium};

const fn ore(mineral: MineralKind, peak: u16, spread: u16, weight: u32) -> OreCurve {
    OreCurve {
        mineral,
        peak_per_mille: peak,
        spread_per_mille: spread,
        weight,
    }
}

const fn palette(rock: Rgb, hard: Rgb, back: Rgb, accent: Option<Rgb>) -> StratumPalette {
    StratumPalette {
        rock,
        hard,
        back,
        accent,
    }
}

const fn resource(name: &'static str, stands_in: StrategicResourceKind) -> LayerResource {
    LayerResource {
        name,
        stands_in: Some(stands_in),
    }
}

const fn screen(effect: ScreenEffect, intensity_permille: u16) -> ScreenEffectLevel {
    ScreenEffectLevel {
        effect,
        intensity_permille,
    }
}

const fn deep(stratum: DeepStratum) -> Band {
    Band::Deep(stratum)
}

/// The defaults every entry starts from (overridden field by field).
const TEMPLATE: StratumDef = StratumDef {
    stratum: Stratum::Topsoil,
    descent: Descent::Crust,
    name: "",
    signature: "",
    share_per_mille: 0,
    host_band: Band::Topsoil,
    soft: TileKind::HardRock,
    hard: TileKind::Stone,
    palette: palette(Rgb(90, 90, 90), Rgb(60, 60, 60), Rgb(30, 30, 30), None),
    ores: &[],
    layer_resource: LayerResource {
        name: "",
        stands_in: None,
    },
    creatures: &[],
    hazards: &[],
    field: FieldProfile::NEUTRAL,
    music: MusicCue::Crust,
    ambience: AmbienceCue::Drips,
    landmarks: &[LandmarkKind::Prefabs, LandmarkKind::CaveEcosystems],
    variants: &[],
    gate: None,
    ore_density_permille: 1_000,
    fluids: &[],
};

/// Landmarks of a stratum holding its Descent's outpost.
const WITH_OUTPOST: &[LandmarkKind] = &[
    LandmarkKind::Outpost,
    LandmarkKind::Prefabs,
    LandmarkKind::CaveEcosystems,
];
/// Landmarks of a Descent's last stratum (its gate chamber).
const WITH_GATE: &[LandmarkKind] = &[
    LandmarkKind::GateChamber,
    LandmarkKind::Prefabs,
    LandmarkKind::CaveEcosystems,
];
/// The far Strange Layers' strata (Wave 9 strange-b): their own set pieces besides.
const FAR_STRANGE: &[LandmarkKind] = &[
    LandmarkKind::AnomalyRoom,
    LandmarkKind::Prefabs,
    LandmarkKind::FarStrange,
];
/// The Sleeping Garrison (Wave 10 ancients-b): its forts and the Grand Bastion.
const GARRISON: &[LandmarkKind] = &[LandmarkKind::Prefabs, LandmarkKind::Garrison];
/// The Seal Gate (Wave 10 ancients-b): the gate chamber's stratum, with the Seal Court, the
/// Gatekeeper's hall and the Heart Seal.
const SEAL_GATE: &[LandmarkKind] = &[
    LandmarkKind::GateChamber,
    LandmarkKind::Prefabs,
    LandmarkKind::Garrison,
];

/// The Crust's creature tables (Wave 8): each stratum's own natives, with a classic kind or two
/// staying on from the stratum above or below.
const CRUST_TOPSOIL: &[(EnemyKind, u32)] = &[
    (EnemyKind::BurrowMole, 5),
    (EnemyKind::MudLeech, 3),
    (CaveBat, 1),
];
const CRUST_CLAY: &[(EnemyKind, u32)] = &[
    (EnemyKind::PotterWasp, 4),
    (EnemyKind::MudLeech, 3),
    (Burrower, 1),
];
const CRUST_STONE: &[(EnemyKind, u32)] = &[
    (EnemyKind::MineBat, 5),
    (EnemyKind::TimberBeetle, 3),
    (AcidSpitter, 1),
];
const CRUST_SLATE: &[(EnemyKind, u32)] = &[
    (EnemyKind::SlateSkink, 4),
    (EnemyKind::ShelfCrab, 3),
    (RockMimic, 1),
];
const CRUST_BASALT: &[(EnemyKind, u32)] = &[
    (EnemyKind::TubeCrawler, 4),
    (EnemyKind::CinderMoth, 4),
    (MiningDrone, 1),
];

/// Every stratum, in [`Stratum::ALL`] order.
pub const STRATA: [StratumDef; super::STRATUM_COUNT] = [
    // --- Descent I: The Crust (authored, Wave 8; see `crate::crust`) -------------------------
    StratumDef {
        stratum: Stratum::Topsoil,
        descent: Descent::Crust,
        name: "Topsoil",
        signature: "Rain-soaked mud that bogs the drive down; roots, burrows and the first caves.",
        share_per_mille: 150,
        host_band: Band::Topsoil,
        soft: TileKind::Dirt,
        hard: TileKind::Clay,
        palette: palette(
            Rgb(128, 84, 47),
            Rgb(168, 101, 63),
            Rgb(58, 36, 24),
            Some(Rgb(212, 155, 92)),
        ),
        ores: &[ore(Copper, 200, 600, 100), ore(Iron, 650, 500, 90)],
        layer_resource: resource("Root Fibre", StrategicResourceKind::RadiantFossil),
        creatures: CRUST_TOPSOIL,
        hazards: &[],
        music: MusicCue::Topsoil,
        ambience: AmbienceCue::Burrows,
        landmarks: &[
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::Settlements,
        ],
        variants: &["Rootbound", "Rain Mud", "Burrow Warren"],
        ore_density_permille: 1_150,
        fluids: &[FluidPocketRule::cave_pools(Water, 160)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::ClayBeds,
        descent: Descent::Crust,
        name: "Clay Beds",
        signature: "Aquifers: water pockets that flood tunnels when breached; clay pots to sell.",
        share_per_mille: 190,
        host_band: Band::Clay,
        soft: TileKind::Clay,
        hard: TileKind::Stone,
        palette: palette(
            Rgb(168, 101, 63),
            Rgb(133, 126, 118),
            Rgb(58, 36, 24),
            Some(Rgb(77, 142, 213)),
        ),
        ores: &[
            ore(Copper, 100, 400, 30),
            ore(Iron, 300, 600, 64),
            ore(Silver, 600, 500, 84),
            ore(Gold, 950, 300, 28),
        ],
        layer_resource: resource("Kiln Clay", StrategicResourceKind::RadiantFossil),
        creatures: CRUST_CLAY,
        hazards: &[(Gas, 1)],
        music: MusicCue::ClayBeds,
        ambience: AmbienceCue::Aquifer,
        variants: &["Potters' Beds", "Aquifer Seams"],
        ore_density_permille: 1_100,
        fluids: &[
            FluidPocketRule {
                min_radius: 3,
                max_radius: 6,
                ..FluidPocketRule::sealed(Water, 260)
            },
            FluidPocketRule::cave_pools(Water, 260),
        ],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GreyStone,
        descent: Descent::Crust,
        name: "Grey Stone",
        signature: "Old mine works: rails to ride, timber sets that collapse when undercut, first gas.",
        share_per_mille: 220,
        host_band: Band::Stone,
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(133, 126, 118),
            Rgb(87, 82, 80),
            Rgb(52, 48, 47),
            Some(Rgb(242, 169, 59)),
        ),
        ores: &[
            ore(Iron, 100, 400, 40),
            ore(Silver, 300, 600, 72),
            ore(Gold, 650, 500, 48),
            ore(Emerald, 980, 200, 10),
        ],
        layer_resource: resource("Mine Timber", StrategicResourceKind::MachineRelic),
        creatures: CRUST_STONE,
        hazards: &[(Gas, 6), (Pressure, 1), (Explosive, 2)],
        music: MusicCue::GreyStone,
        ambience: AmbienceCue::MineWorks,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::Settlements,
        ],
        variants: &["Old Workings", "Rail Galleries", "Gas Pockets"],
        ore_density_permille: 1_050,
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SlateShelves,
        descent: Descent::Crust,
        name: "Slate Shelves",
        signature: "Slate slides: slick shelves that slide when undercut; stalactite galleries.",
        share_per_mille: 210,
        host_band: Band::Slate,
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(60, 56, 84),
            Rgb(40, 36, 58),
            Rgb(40, 36, 58),
            Some(Rgb(138, 197, 238)),
        ),
        ores: &[
            ore(Silver, 100, 400, 30),
            ore(Gold, 300, 600, 56),
            ore(Emerald, 650, 500, 36),
            ore(Ruby, 980, 200, 8),
        ],
        layer_resource: resource("Shale Plate", StrategicResourceKind::CrystalLens),
        creatures: CRUST_SLATE,
        hazards: &[(Gas, 3), (Pressure, 2), (Explosive, 3)],
        music: MusicCue::SlateShelves,
        ambience: AmbienceCue::SlateDrip,
        variants: &["Stalactite Galleries", "Sliding Shelves"],
        ore_density_permille: 1_000,
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::BasaltColumns,
        descent: Descent::Crust,
        name: "Basalt Columns",
        signature: "Lava tubes: long natural tunnels and the first heat.",
        share_per_mille: 230,
        host_band: Band::Basalt,
        soft: TileKind::HardRock,
        hard: TileKind::Stone,
        palette: palette(
            Rgb(78, 48, 44),
            Rgb(26, 22, 38),
            Rgb(26, 22, 38),
            Some(Rgb(232, 112, 42)),
        ),
        ores: &[
            ore(Gold, 100, 400, 30),
            ore(Emerald, 250, 500, 42),
            ore(Ruby, 600, 500, 32),
            ore(Diamond, 950, 250, 8),
        ],
        layer_resource: resource("Basalt Fibre", StrategicResourceKind::VoidGlass),
        creatures: CRUST_BASALT,
        hazards: &[
            (Gas, 2),
            (Lava, 4),
            (MagmaVent, 3),
            (Pressure, 1),
            (Explosive, 1),
        ],
        music: MusicCue::BasaltColumns,
        ambience: AmbienceCue::LavaTube,
        landmarks: WITH_GATE,
        variants: &["Lava Tubes", "Column Forest"],
        gate: Some(GateDef {
            boss: DescentBoss::TheSinkhole,
            requires: &[GateRequirement::Boss(DescentBoss::TheSinkhole)],
            rewards: &[Key::DeepClaim, Key::DrillII],
        }),
        ore_density_permille: 1_000,
        ..TEMPLATE
    },
    // --- Descent II: The Old Deep (the seven classic deep strata, expanded) ------------------
    StratumDef {
        stratum: Stratum::CrystalFaults,
        descent: Descent::OldDeep,
        name: "Crystal Faults",
        signature: "Resonance: drilled crystal rings its fault; prisms split the lamp into beams that light hidden gems.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Stone,
        hard: TileKind::Stone,
        palette: palette(
            Rgb(96, 120, 168),
            Rgb(70, 84, 132),
            Rgb(28, 34, 60),
            Some(Rgb(160, 220, 255)),
        ),
        ores: &[ore(Diamond, 500, 900, 80), ore(Ruby, 300, 600, 26)],
        layer_resource: resource("Resonant Shards", StrategicResourceKind::CrystalLens),
        creatures: &[(PrismMoth, 5), (CaveBat, 3), (RockMimic, 3)],
        hazards: &[
            (Gas, 1),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 1),
            (Explosive, 8),
        ],
        music: MusicCue::OldDeep,
        landmarks: WITH_OUTPOST,
        variants: &["Singing Faults", "Prism Seams"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::FossilOceans,
        descent: Descent::OldDeep,
        name: "Fossil Oceans",
        signature: "Gas tides breathe from the pockets, amber holds rigs fast, skeletons are dug out bone by bone.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Stone,
        hard: TileKind::Stone,
        palette: palette(
            Rgb(150, 128, 92),
            Rgb(112, 96, 70),
            Rgb(52, 44, 32),
            Some(Rgb(240, 180, 60)),
        ),
        ores: &[ore(Platinum, 500, 900, 80), ore(Diamond, 400, 600, 20)],
        layer_resource: resource("Amber Resin", StrategicResourceKind::RadiantFossil),
        creatures: &[(FossilEel, 5), (Shellback, 3), (AcidSpitter, 2)],
        hazards: &[
            (Gas, 8),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 1),
            (Explosive, 1),
        ],
        music: MusicCue::OldDeep,
        variants: &["Amber Reefs", "Bone Fields"],
        fluids: &[FluidPocketRule::sealed(Water, 60)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PressureCathedrals,
        descent: Descent::OldDeep,
        name: "Pressure Cathedrals",
        signature: "Grand naves where timed pressure jets throw rigs; three valves restore the Cathedral Lift.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(116, 108, 140),
            Rgb(84, 76, 108),
            Rgb(36, 32, 52),
            Some(Rgb(200, 190, 255)),
        ),
        ores: &[ore(Mythril, 500, 900, 60), ore(Platinum, 300, 600, 16)],
        layer_resource: resource("Compressed Basalt", StrategicResourceKind::PressurePearl),
        creatures: &[(JetRay, 5), (RockMimic, 3), (SentryTurret, 1)],
        hazards: &[
            (Gas, 1),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 8),
            (Explosive, 1),
        ],
        music: MusicCue::OldDeep,
        variants: &["Nave Halls", "Jet Vaults"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::RadioactiveHollow,
        descent: Descent::OldDeep,
        name: "Radioactive Hollow",
        signature: "A dose meter fills near glow rock and costs fuel, sight and hull; mutation relics change the rig.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::RadioactiveHollow),
        palette: palette(
            Rgb(92, 128, 72),
            Rgb(64, 96, 52),
            Rgb(24, 40, 20),
            Some(Rgb(140, 255, 90)),
        ),
        ores: &[ore(Uranium, 500, 900, 60), ore(Platinum, 400, 600, 16)],
        layer_resource: resource("Glow Isotopes", StrategicResourceKind::CoreShard),
        creatures: &[(GlowGrub, 6), (AcidSpitter, 3), (RogueExcavator, 2)],
        hazards: &[
            (Gas, 1),
            (Lava, 2),
            (MagmaVent, 8),
            (Pressure, 1),
            (Explosive, 1),
        ],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 250)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::OldDeep,
        variants: &["Glow Pits", "Mutant Warrens"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::AncientMachineLayer,
        descent: Descent::OldDeep,
        name: "Ancient Machine Layer",
        signature: "Builder halls wake as you near: conveyors carry rigs, pistons slam, tablets tell of the builders.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(120, 112, 96),
            Rgb(88, 80, 70),
            Rgb(40, 36, 32),
            Some(Rgb(255, 170, 60)),
        ),
        ores: &[ore(Mythril, 500, 900, 60), ore(Uranium, 400, 600, 12)],
        layer_resource: resource("Machine Alloy", StrategicResourceKind::MachineRelic),
        creatures: &[(PistonWarden, 4), (MiningDrone, 4), (SentryTurret, 2)],
        hazards: &[
            (Gas, 1),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 1),
            (Explosive, 8),
        ],
        music: MusicCue::OldDeep,
        ambience: AmbienceCue::Machines,
        variants: &["Conveyor Works", "Piston Halls"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::VoidGeodeFields,
        descent: Descent::OldDeep,
        name: "Void Geode Fields",
        signature: "Geodes hold low gravity that pulls out to their shells: float, turn the rig and land on the walls.",
        share_per_mille: 140,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(112, 80, 140),
            Rgb(80, 56, 108),
            Rgb(28, 20, 44),
            Some(Rgb(255, 140, 220)),
        ),
        ores: &[ore(Mythril, 500, 900, 70), ore(Diamond, 300, 600, 30)],
        layer_resource: resource("Void Glass", StrategicResourceKind::VoidGlass),
        creatures: &[(VoidJelly, 5), (CaveBat, 3), (MiningDrone, 2)],
        hazards: &[
            (Gas, 1),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 8),
            (Explosive, 1),
        ],
        music: MusicCue::OldDeep,
        variants: &["Hollow Geodes", "Drifting Shards"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::MantleStormZone,
        descent: Descent::OldDeep,
        name: "Mantle Storm Zone",
        signature: "Heat storms on a timer scorch open caves and make storm vents pour molten metal: dig in.",
        share_per_mille: 135,
        host_band: deep(DeepStratum::MantleStormZone),
        palette: palette(
            Rgb(150, 72, 44),
            Rgb(110, 52, 34),
            Rgb(56, 20, 14),
            Some(Rgb(255, 150, 40)),
        ),
        ores: &[
            ore(Mythril, 600, 900, 80),
            ore(Uranium, 400, 600, 20),
            ore(Ruby, 200, 500, 12),
        ],
        layer_resource: resource("Mantle Fragments", StrategicResourceKind::CoreShard),
        creatures: &[(MagmaSalamander, 5), (MagmaWisp, 4), (RogueExcavator, 2)],
        hazards: &[
            (Gas, 1),
            (Lava, 8),
            (MagmaVent, 1),
            (Pressure, 1),
            (Explosive, 1),
        ],
        music: MusicCue::OldDeep,
        ambience: AmbienceCue::Lava,
        landmarks: WITH_GATE,
        variants: &["Storm Front", "Lava Sea"],
        gate: Some(GateDef {
            boss: DescentBoss::StarCore,
            requires: &[
                GateRequirement::Key(Key::DeepClaim),
                GateRequirement::Boss(DescentBoss::StarCore),
            ],
            rewards: &[Key::PressureHull],
        }),
        fluids: &[FluidPocketRule::sealed(LiquidMetal, 30)],
        ..TEMPLATE
    },
    // --- Descent III: The Living Dark (authored, Wave 8; `crate::living_dark`) --------------
    // Biology takes over: every stratum's walls are alive in their own way, and each one's
    // signature rule is introduced gently near its top and pushed by its bottom.
    StratumDef {
        stratum: Stratum::FungalForest,
        descent: Descent::LivingDark,
        name: "Fungal Forest",
        signature: "Giant mushrooms bounce you; spore clouds blur and colour-shift the screen.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(146, 76, 166),
            Rgb(92, 50, 112),
            Rgb(34, 20, 44),
            Some(Rgb(176, 255, 132)),
        ),
        ores: &[
            ore(Silver, 150, 450, 30),
            ore(Emerald, 450, 600, 55),
            ore(Platinum, 800, 500, 30),
        ],
        layer_resource: resource("Spore Silk", StrategicResourceKind::RadiantFossil),
        creatures: &[(SporePuffer, 5), (CapCrawler, 4), (GlowGnat, 4)],
        hazards: &[(Gas, 6), (Explosive, 1)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::Blur, 140),
                screen(ScreenEffect::ColourGrade, 260),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::FungalForest,
        ambience: AmbienceCue::Spores,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::LivingDark,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Cap Canopy", "Spore Mists", "Puffball Hollows"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::RootLabyrinth,
        descent: Descent::LivingDark,
        name: "Root Labyrinth",
        signature: "Roots regrow behind you and close tunnels over minutes: axes and fire keep paths.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(92, 82, 52),
            Rgb(70, 54, 36),
            Rgb(30, 24, 14),
            Some(Rgb(206, 232, 120)),
        ),
        ores: &[
            ore(Gold, 200, 500, 35),
            ore(Ruby, 450, 600, 45),
            ore(Diamond, 800, 500, 30),
        ],
        layer_resource: resource("Heartwood", StrategicResourceKind::RadiantFossil),
        creatures: &[(RootSnapper, 4), (BarkBeetle, 4), (ThornWisp, 4)],
        hazards: &[(Gas, 2), (Explosive, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Vignette, 260)],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::RootLabyrinth,
        ambience: AmbienceCue::Roots,
        landmarks: &[
            LandmarkKind::LivingDark,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Tangle", "Taproot Shafts"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheHive,
        descent: Descent::LivingDark,
        name: "The Hive",
        signature: "An insect civilisation: the Amber Court trades, the Swarm attacks; your choices change the hive.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::AncientMachineLayer),
        soft: TileKind::HardRock,
        hard: TileKind::Stone,
        palette: palette(
            Rgb(168, 118, 46),
            Rgb(124, 84, 30),
            Rgb(56, 34, 12),
            Some(Rgb(255, 214, 86)),
        ),
        ores: &[
            ore(Gold, 250, 600, 45),
            ore(Platinum, 550, 700, 45),
            ore(Uranium, 850, 400, 15),
        ],
        layer_resource: resource("Royal Jelly", StrategicResourceKind::PressurePearl),
        creatures: &[(HiveSoldier, 6), (AcidLancer, 3), (HiveDrone, 3)],
        hazards: &[(Gas, 2), (Pressure, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 220)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::Hive,
        ambience: AmbienceCue::Hive,
        landmarks: &[
            LandmarkKind::Settlements,
            LandmarkKind::LivingDark,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Comb Halls", "Brood Galleries"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::FleshCaverns,
        descent: Descent::LivingDark,
        name: "Flesh Caverns",
        signature: "Pulsing walls heal what you mine; acid tides rise and fall; a heartbeat sets the tempo.",
        share_per_mille: 190,
        host_band: deep(DeepStratum::RadioactiveHollow),
        soft: TileKind::Cave(CaveTile::Flesh),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(156, 64, 78),
            Rgb(214, 196, 170),
            Rgb(54, 14, 24),
            Some(Rgb(255, 116, 136)),
        ),
        ores: &[
            ore(Ruby, 350, 700, 55),
            ore(Diamond, 600, 600, 30),
            ore(Uranium, 850, 450, 25),
        ],
        layer_resource: resource("Living Sinew", StrategicResourceKind::CoreShard),
        creatures: &[(MawWorm, 4), (BilePolyp, 2), (Leechling, 4)],
        hazards: &[(Gas, 3)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::Wobble, 220),
                screen(ScreenEffect::ColourGrade, 200),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::FleshCaverns,
        ambience: AmbienceCue::Heartbeat,
        landmarks: &[
            LandmarkKind::LivingDark,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Throbbing Halls", "Acid Gullets"],
        fluids: &[FluidPocketRule::cave_pools(Acid, 90)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GlowSea,
        descent: Descent::LivingDark,
        name: "Glow Sea",
        signature: "Near-total darkness: the only light comes from creatures, until Lamp II.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(30, 58, 70),
            Rgb(20, 40, 52),
            Rgb(6, 12, 18),
            Some(Rgb(96, 255, 222)),
        ),
        ores: &[
            ore(Diamond, 300, 700, 50),
            ore(Platinum, 500, 600, 25),
            ore(Mythril, 800, 500, 35),
        ],
        layer_resource: resource("Lure Light", StrategicResourceKind::CrystalLens),
        creatures: &[(LureFish, 5), (GlowJelly, 4), (Angler, 3)],
        hazards: &[(Pressure, 4), (Gas, 2)],
        field: FieldProfile {
            light_permille: 60,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::GlowSea,
        ambience: AmbienceCue::Abyss,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::LivingDark,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Jelly Drifts", "Angler Deeps"],
        gate: Some(GateDef {
            boss: DescentBoss::MycelialMind,
            requires: &[
                GateRequirement::Key(Key::PressureHull),
                GateRequirement::Boss(DescentBoss::MycelialMind),
            ],
            rewards: &[Key::BioFilter, Key::LampII],
        }),
        fluids: &[
            FluidPocketRule::cave_pools(Water, 260),
            FluidPocketRule::sealed(Water, 60),
        ],
        ore_density_permille: 1_000,
    },
    // --- Descent IV: The Drowned and Frozen (authored, Wave 9; `crate::drowned`) -----------
    StratumDef {
        stratum: Stratum::UndergroundOcean,
        descent: Descent::DrownedAndFrozen,
        name: "The Underground Ocean",
        signature: "A sea under the rock: submarine mode with ballast, gyre currents and sonar.",
        share_per_mille: 240,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Cave(CaveTile::Seabed),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(58, 96, 128),
            Rgb(36, 62, 96),
            Rgb(10, 30, 52),
            Some(Rgb(120, 220, 255)),
        ),
        ores: &[
            ore(Platinum, 350, 700, 50),
            ore(Diamond, 550, 700, 30),
            ore(Mythril, 850, 400, 20),
        ],
        layer_resource: resource("Deep Pearl", StrategicResourceKind::PressurePearl),
        creatures: &[
            (EnemyKind::ReefShark, 5),
            (EnemyKind::InkSquid, 4),
            (LureFish, 1),
        ],
        hazards: &[(Pressure, 4), (Gas, 1)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 260)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::UndergroundOcean,
        ambience: AmbienceCue::Reef,
        landmarks: &[
            LandmarkKind::Drowned,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Kelp Trench", "Current Gyres"],
        fluids: &[
            FluidPocketRule::sealed(Water, 260),
            FluidPocketRule::cave_pools(Water, 300),
        ],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::BrineGlaciers,
        descent: Descent::DrownedAndFrozen,
        name: "Brine Glaciers",
        signature: "Ice physics: rigs slide, tunnels freeze over, hull frost slows you until a vent warms you.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Cave(CaveTile::BrineIce),
        hard: TileKind::Cave(CaveTile::PackIce),
        palette: palette(
            Rgb(150, 206, 238),
            Rgb(96, 150, 204),
            Rgb(48, 78, 108),
            Some(Rgb(240, 250, 255)),
        ),
        ores: &[
            ore(Diamond, 300, 700, 50),
            ore(Platinum, 600, 600, 30),
            ore(Uranium, 850, 400, 20),
        ],
        layer_resource: resource("Brine Ice", StrategicResourceKind::MoonIce),
        creatures: &[
            (EnemyKind::FrostMite, 5),
            (EnemyKind::TuskSeal, 3),
            (Shellback, 1),
        ],
        hazards: &[(Pressure, 3), (Explosive, 2)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::ColourGrade, 220),
                screen(ScreenEffect::Vignette, 140),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::BrineGlaciers,
        ambience: AmbienceCue::Ice,
        landmarks: &[
            LandmarkKind::Drowned,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Blue Ice", "Brine Channels"],
        fluids: &[
            FluidPocketRule::sealed(Brine, 200),
            FluidPocketRule::cave_pools(Brine, 160),
        ],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GeyserFields,
        descent: Descent::DrownedAndFrozen,
        name: "Geyser Fields",
        signature: "Geysers blow on a beat: ride their jets up like elevators, never sit on a throat.",
        share_per_mille: 180,
        host_band: deep(DeepStratum::PressureCathedrals),
        soft: TileKind::Cave(CaveTile::Sinter),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(222, 200, 148),
            Rgb(128, 112, 80),
            Rgb(58, 48, 34),
            Some(Rgb(255, 250, 220)),
        ),
        ores: &[
            ore(Platinum, 300, 600, 40),
            ore(Mythril, 500, 700, 40),
            ore(Uranium, 750, 500, 25),
        ],
        layer_resource: resource("Sinter Crust", StrategicResourceKind::PressurePearl),
        creatures: &[
            (EnemyKind::ScaldBeetle, 4),
            (EnemyKind::SinterSnail, 3),
            (MagmaWisp, 1),
        ],
        hazards: &[(Pressure, 5), (MagmaVent, 2)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::Fog, 240),
                screen(ScreenEffect::Wobble, 90),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::GeyserFields,
        ambience: AmbienceCue::Geysers,
        landmarks: &[
            LandmarkKind::Drowned,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Steam Vents", "Mud Pots"],
        fluids: &[
            FluidPocketRule::sealed(Water, 160),
            FluidPocketRule::cave_pools(Water, 120),
        ],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::MerrowCity,
        descent: Descent::DrownedAndFrozen,
        name: "Merrow City",
        signature: "A friendly fish-folk city in an air bubble: people, a shop, an outpost, aquarium furniture.",
        share_per_mille: 160,
        host_band: deep(DeepStratum::PressureCathedrals),
        soft: TileKind::Cave(CaveTile::MerrowStone),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(116, 204, 172),
            Rgb(70, 120, 120),
            Rgb(18, 56, 62),
            Some(Rgb(255, 196, 150)),
        ),
        ores: &[
            ore(Emerald, 300, 700, 40),
            ore(Platinum, 500, 600, 35),
            ore(Diamond, 700, 500, 25),
        ],
        layer_resource: resource("Coral Glass", StrategicResourceKind::CrystalLens),
        creatures: &[
            (EnemyKind::Spinepuff, 5),
            (EnemyKind::ReefShark, 2),
            (LureFish, 2),
        ],
        hazards: &[(Pressure, 2)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 180)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::MerrowCity,
        ambience: AmbienceCue::Bubble,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Settlements,
            LandmarkKind::Drowned,
            LandmarkKind::Prefabs,
        ],
        variants: &["Bubble Domes", "Reef Terraces"],
        fluids: &[FluidPocketRule::cave_pools(Water, 200)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PearlAbyss,
        descent: Descent::DrownedAndFrozen,
        name: "Pearl Abyss",
        signature: "Crushing pressure past your seals: the deeper, the darker and greyer the world.",
        share_per_mille: 220,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Cave(CaveTile::PillowBasalt),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(30, 34, 104),
            Rgb(28, 30, 64),
            Rgb(8, 10, 28),
            Some(Rgb(236, 232, 255)),
        ),
        ores: &[
            ore(Diamond, 300, 600, 30),
            ore(Mythril, 500, 700, 55),
            ore(Uranium, 800, 500, 20),
        ],
        layer_resource: resource("Abyss Pearl", StrategicResourceKind::PressurePearl),
        creatures: &[
            (EnemyKind::LureAngler, 4),
            (EnemyKind::GulperEel, 3),
            (GlowJelly, 2),
        ],
        hazards: &[(Pressure, 6), (Gas, 1)],
        field: FieldProfile {
            light_permille: 820,
            screen: &[
                screen(ScreenEffect::Desaturate, 240),
                screen(ScreenEffect::Vignette, 240),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::PearlAbyss,
        ambience: AmbienceCue::Trench,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::Drowned,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Black Trench", "Pearl Beds"],
        gate: Some(GateDef {
            boss: DescentBoss::Leviathan,
            requires: &[
                GateRequirement::Key(Key::BioFilter),
                GateRequirement::Boss(DescentBoss::Leviathan),
            ],
            rewards: &[Key::SonarSuite, Key::CryoHull],
        }),
        fluids: &[
            FluidPocketRule::sealed(Water, 240),
            FluidPocketRule::sealed(Brine, 100),
        ],
        ..TEMPLATE
    },
    // --- Descent V: The Hollow World (Wave 9, authored: `crate::hollow`) ---------------------
    StratumDef {
        stratum: Stratum::InnerSky,
        descent: Descent::HollowWorld,
        name: "Inner Sky",
        signature: "The breakthrough: a giant open cavern under a rock sky, lit by an inner sun, \
                    its wind bands gusting harder deeper.",
        share_per_mille: 330,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Hollow(HollowTile::SkyStone),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(206, 196, 178),
            Rgb(150, 142, 132),
            Rgb(58, 66, 92),
            Some(Rgb(255, 214, 120)),
        ),
        ores: &[
            ore(Diamond, 400, 700, 40),
            ore(Platinum, 500, 600, 35),
            ore(Gold, 300, 500, 20),
        ],
        layer_resource: resource("Sky Amber", StrategicResourceKind::AetherCrystal),
        creatures: &[(SkyManta, 4), (StoopRoc, 4), (GustBladder, 5)],
        hazards: &[(Gas, 2), (Pressure, 1)],
        field: FieldProfile {
            // Brighter than any cave: a touch of glare from the inner sun.
            light_permille: 1_250,
            screen: &[screen(ScreenEffect::ColourGrade, 160)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::InnerSky,
        ambience: AmbienceCue::Wind,
        landmarks: &[
            LandmarkKind::HollowCavern,
            LandmarkKind::Outpost,
            LandmarkKind::HollowWorld,
            LandmarkKind::Prefabs,
        ],
        variants: &["Cloud Halls", "Stone Archipelago"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::MegafaunaJungle,
        descent: Descent::HollowWorld,
        name: "Megafauna Jungle",
        signature: "Giant beasts under giant trees: ride a crestback, mind the fangrunner packs \
                    at night and the tar on the floors; the Sunfolk's village.",
        share_per_mille: 250,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Hollow(HollowTile::Loam),
        hard: TileKind::Stone,
        palette: palette(
            Rgb(58, 70, 34),
            Rgb(46, 52, 34),
            Rgb(26, 54, 30),
            Some(Rgb(150, 230, 90)),
        ),
        ores: &[
            ore(Emerald, 500, 800, 55),
            ore(Gold, 300, 600, 35),
            ore(Ruby, 700, 400, 15),
        ],
        layer_resource: resource("Giant Hide", StrategicResourceKind::RadiantFossil),
        creatures: &[(Crestback, 5), (Skyneck, 2), (Fangrunner, 4)],
        hazards: &[(Gas, 3), (Explosive, 1)],
        field: FieldProfile {
            light_permille: 900,
            screen: &[
                screen(ScreenEffect::ColourGrade, 220),
                screen(ScreenEffect::Fog, 90),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::MegafaunaJungle,
        ambience: AmbienceCue::Jungle,
        landmarks: &[
            LandmarkKind::Settlements,
            LandmarkKind::HollowWorld,
            LandmarkKind::Prefabs,
        ],
        variants: &["Fern Valleys", "Tar Pits"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::UpsideDownIsles,
        descent: Descent::HollowWorld,
        name: "Upside-Down Isles",
        signature: "Local inverted gravity: isles hang from the roof and gravity points up under \
                    them; flip the rig with its thrusters to land on them.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Hollow(HollowTile::IsleRock),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(184, 142, 160),
            Rgb(140, 106, 124),
            Rgb(70, 92, 136),
            Some(Rgb(150, 210, 255)),
        ),
        ores: &[
            ore(Mythril, 500, 800, 50),
            ore(Platinum, 300, 600, 30),
            ore(Diamond, 700, 400, 15),
        ],
        layer_resource: resource("Lodestone", StrategicResourceKind::VoidGlass),
        creatures: &[(Hangclaw, 4), (LodeMite, 5), (SkyManta, 1)],
        hazards: &[(Pressure, 3), (Explosive, 2)],
        field: FieldProfile {
            // The rig turns freely anywhere here: the player flips it to land under an isle
            // (the isles' own upside-down pull is `hollow::fields`).
            free_rotation: true,
            light_permille: 1_050,
            screen: &[screen(ScreenEffect::ColourGrade, 140)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::UpsideDownIsles,
        ambience: AmbienceCue::Updraft,
        landmarks: &[LandmarkKind::HollowWorld, LandmarkKind::Prefabs],
        variants: &["Hanging Isles", "Flip Reefs"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheSunforge,
        descent: Descent::HollowWorld,
        name: "The Sunforge",
        signature: "Overexposure near the inner sun: the light below blooms the screen and heats \
                    the rig; the floating stones' shade cools it, so routes hop from shadow to \
                    shadow.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::MantleStormZone),
        soft: TileKind::Hollow(HollowTile::SunRock),
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(236, 196, 112),
            Rgb(196, 140, 72),
            Rgb(132, 72, 38),
            Some(Rgb(255, 250, 214)),
        ),
        ores: &[
            ore(Ruby, 400, 700, 45),
            ore(Mythril, 600, 800, 45),
            ore(Diamond, 800, 400, 15),
        ],
        layer_resource: resource("Sun Glass", StrategicResourceKind::StormGlass),
        creatures: &[(CinderDrake, 4), (ShadeStalker, 3), (Flarefly, 5)],
        hazards: &[(Lava, 6), (MagmaVent, 3)],
        field: FieldProfile {
            light_permille: 1_300,
            screen: &[
                screen(ScreenEffect::Exposure, 180),
                screen(ScreenEffect::Bloom, 420),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::Sunforge,
        ambience: AmbienceCue::Forge,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::HollowWorld,
            LandmarkKind::Prefabs,
        ],
        variants: &["Shadow Paths", "Solar Flats"],
        gate: Some(GateDef {
            boss: DescentBoss::SunSerpent,
            requires: &[
                GateRequirement::Key(Key::CryoHull),
                GateRequirement::Boss(DescentBoss::SunSerpent),
            ],
            rewards: &[Key::GravityAnchor],
        }),
        fluids: &[FluidPocketRule::sealed(LiquidMetal, 60)],
        ..TEMPLATE
    },
    // --- Descent VI: The Strange Layers --------------------------------------------------------
    StratumDef {
        stratum: Stratum::MirrorStrata,
        descent: Descent::StrangeLayers,
        name: "Mirror Strata",
        signature: "Mirror halls flip the screen and the controls; mirror twins copy your \
                    moves and shots; mirror ore lies in pairs, one real, one reflection.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(170, 160, 204),
            Rgb(116, 112, 150),
            Rgb(30, 32, 48),
            Some(Rgb(200, 236, 255)),
        ),
        ores: &[
            ore(Silver, 150, 450, 40),
            ore(Diamond, 500, 600, 45),
            ore(Platinum, 850, 500, 35),
        ],
        layer_resource: resource("Mirror Ore", StrategicResourceKind::CrystalLens),
        // Twins never roam (they step out of the halls); the weight names them for the probe.
        creatures: &[(Glassback, 5), (Gazer, 3), (MirrorTwin, 1)],
        hazards: &[(Explosive, 3), (Pressure, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 120)],
            regions: FieldRegions::Authored(AuthoredRegions::MirrorHalls),
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::MirrorStrata,
        ambience: AmbienceCue::Glass,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Distortion,
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Prefabs,
        ],
        variants: &["Glass Halls", "Twin Galleries", "Silver Seams"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GravityShear,
        descent: Descent::StrangeLayers,
        name: "Gravity Shear",
        signature: "Gravity turns 90 and then 180 degrees in shear halls; turn the rig with its \
                    thrusters and walk the walls and ceilings.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(156, 82, 142),
            Rgb(102, 56, 108),
            Rgb(38, 16, 44),
            Some(Rgb(255, 176, 244)),
        ),
        ores: &[
            ore(Platinum, 250, 500, 45),
            ore(Mythril, 650, 500, 45),
            ore(Uranium, 900, 400, 25),
        ],
        layer_resource: resource("Shear Stone", StrategicResourceKind::VoidGlass),
        creatures: &[(Tumbler, 4), (Lodestar, 3), (ShearMantis, 3)],
        hazards: &[(Pressure, 5), (Explosive, 2)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ColourGrade, 100)],
            free_rotation: true,
            regions: FieldRegions::Authored(AuthoredRegions::ShearHalls),
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::GravityShear,
        ambience: AmbienceCue::Shear,
        landmarks: &[
            LandmarkKind::Distortion,
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Prefabs,
        ],
        variants: &["Turned Halls", "Shear Lines"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PixelRot,
        descent: Descent::StrangeLayers,
        name: "Pixel Rot",
        signature: "The deeper you go, the lower the resolution: bigger pixels, fewer colours, \
                    glitching sprites and rock quantized into blocks.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::AncientMachineLayer),
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(
            Rgb(54, 126, 82),
            Rgb(34, 90, 62),
            Rgb(10, 34, 24),
            Some(Rgb(214, 96, 230)),
        ),
        ores: &[
            ore(Uranium, 200, 450, 40),
            ore(Mythril, 550, 500, 45),
            ore(Diamond, 850, 450, 30),
        ],
        layer_resource: resource("Data Ore", StrategicResourceKind::MachineRelic),
        creatures: &[(Glitchling, 5), (Bitworm, 3), (DeadPixel, 2)],
        hazards: &[(Explosive, 4), (MagmaVent, 2)],
        field: FieldProfile {
            regions: FieldRegions::Authored(AuthoredRegions::RotTiers),
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::PixelRot,
        ambience: AmbienceCue::Static,
        landmarks: &[
            LandmarkKind::Distortion,
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Prefabs,
        ],
        variants: &["Low Res", "Glitch Fields"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TimeSludge,
        descent: Descent::StrangeLayers,
        name: "Time Sludge",
        signature: "Slow-motion and fast-forward zones; creatures live at other speeds.",
        share_per_mille: 160,
        host_band: deep(DeepStratum::RadioactiveHollow),
        palette: palette(
            Rgb(188, 150, 96),
            Rgb(120, 94, 62),
            Rgb(56, 42, 26),
            Some(Rgb(255, 200, 120)),
        ),
        ores: &[ore(Platinum, 500, 800, 40), ore(Uranium, 500, 600, 40)],
        layer_resource: resource("Chrono Amber", StrategicResourceKind::RadiantFossil),
        // Wave 9 strange-b: creatures that live at their own speeds.
        creatures: &[
            (EnemyKind::TickMite, 5),
            (EnemyKind::AmberSloth, 3),
            (EnemyKind::RewindNewt, 3),
        ],
        hazards: &[(MagmaVent, 5), (Gas, 3)],
        field: FieldProfile {
            time_scale_permille: 850,
            screen: &[screen(ScreenEffect::ColourGrade, 300)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::TimeSludge,
        ambience: AmbienceCue::Clockwork,
        landmarks: FAR_STRANGE,
        variants: &["Slow Pools", "Fast Lanes"],
        fluids: &[FluidPocketRule::cave_pools(Acid, 80)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::EchoCaves,
        descent: Descent::StrangeLayers,
        name: "Echo Caves",
        signature: "Total darkness where sonar pings reveal the cave for a moment.",
        share_per_mille: 160,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(52, 52, 52),
            Rgb(52, 52, 60),
            Rgb(12, 12, 16),
            Some(Rgb(140, 200, 255)),
        ),
        ores: &[ore(Mythril, 500, 800, 50), ore(Diamond, 500, 600, 30)],
        layer_resource: resource("Echo Quartz", StrategicResourceKind::CrystalLens),
        // Wave 9 strange-b: blind hunters, their alarms and the moths that drink sound.
        creatures: &[
            (EnemyKind::EchoStalker, 4),
            (EnemyKind::Shrieker, 3),
            (EnemyKind::HushMoth, 2),
        ],
        hazards: &[(Pressure, 5), (Gas, 2)],
        field: FieldProfile {
            light_permille: 20,
            screen: &[screen(ScreenEffect::Vignette, 300)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::EchoCaves,
        ambience: AmbienceCue::Echoes,
        landmarks: FAR_STRANGE,
        variants: &["Whisper Halls", "Dead Air"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PrismDesert,
        descent: Descent::StrangeLayers,
        name: "Prism Desert",
        signature: "Colours split; colour-keyed walls open only to the matching light.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::CrystalFaults),
        palette: palette(
            Rgb(150, 176, 196),
            Rgb(178, 132, 176),
            Rgb(36, 46, 70),
            Some(Rgb(255, 250, 170)),
        ),
        ores: &[ore(Diamond, 600, 800, 60), ore(Emerald, 400, 600, 30)],
        layer_resource: resource("Prism Sand", StrategicResourceKind::CrystalLens),
        // Wave 9 strange-b: creatures of colour.
        creatures: &[
            (EnemyKind::PrismScarab, 4),
            (EnemyKind::SplitterSpire, 3),
            (EnemyKind::LensWraith, 2),
        ],
        hazards: &[(Explosive, 5), (Pressure, 3)],
        field: FieldProfile {
            // Colours split, more the deeper (Wave 9: introduced gently, then pushed).
            screen: &[screen(ScreenEffect::ChromaticAberration, 560)],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::PrismDesert,
        ambience: AmbienceCue::Prism,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Prefabs,
            LandmarkKind::FarStrange,
        ],
        variants: &["Spectrum Dunes", "Keyed Walls"],
        gate: Some(GateDef {
            boss: DescentBoss::TheGlitch,
            requires: &[
                GateRequirement::Key(Key::GravityAnchor),
                GateRequirement::Boss(DescentBoss::TheGlitch),
            ],
            rewards: &[Key::PhaseDrill],
        }),
        ..TEMPLATE
    },
    // --- Descent VII: The Ancients ------------------------------------------------------------
    StratumDef {
        stratum: Stratum::BuriedMegacity,
        descent: Descent::Ancients,
        name: "Buried Megacity",
        signature: "Streets, towers, trams and a friendly AI archivist.",
        share_per_mille: 230,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(140, 110, 80),
            Rgb(116, 92, 60),
            Rgb(52, 40, 26),
            Some(Rgb(120, 220, 255)),
        ),
        ores: &[ore(Mythril, 500, 800, 50), ore(Platinum, 500, 600, 40)],
        layer_resource: resource("Ancient Alloy", StrategicResourceKind::AncientAlloy),
        // Wave 10 ancients-a: the city's machines.
        creatures: &[
            (EnemyKind::MaintenanceDrone, 4),
            (EnemyKind::ScrapCrawler, 3),
            (EnemyKind::StreetSentinel, 2),
        ],
        hazards: &[(Explosive, 5), (Pressure, 2)],
        field: FieldProfile {
            // The lamps' warm amber over cold stone.
            screen: &[screen(ScreenEffect::ColourGrade, 220)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::BuriedMegacity,
        ambience: AmbienceCue::CityHum,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Settlements,
            LandmarkKind::Prefabs,
            LandmarkKind::AncientCity,
        ],
        variants: &["Tram Avenues", "Fallen Towers"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::EngineHalls,
        descent: Descent::Ancients,
        name: "Engine Halls",
        signature: "Giant moving machinery on a timed scale; ride the pistons.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(110, 96, 90),
            Rgb(80, 88, 100),
            Rgb(32, 36, 44),
            Some(Rgb(255, 140, 40)),
        ),
        ores: &[ore(Uranium, 500, 800, 40), ore(Mythril, 500, 600, 50)],
        layer_resource: resource("Engine Core", StrategicResourceKind::MachineRelic),
        // Wave 10 ancients-a: the halls' machines and the sparks they throw.
        creatures: &[
            (EnemyKind::CogRoller, 4),
            (EnemyKind::SparkWisp, 3),
            (EnemyKind::RivetHound, 2),
        ],
        hazards: &[(Explosive, 6), (MagmaVent, 2)],
        field: FieldProfile {
            // Hot, oily air: a little haze and a vignette, heavier deeper.
            screen: &[
                screen(ScreenEffect::Wobble, 160),
                screen(ScreenEffect::Vignette, 260),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::EngineHalls,
        ambience: AmbienceCue::EngineBeat,
        landmarks: &[
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::AncientCity,
        ],
        variants: &["Crusher Lines", "Piston Shafts"],
        fluids: &[FluidPocketRule::sealed(LiquidMetal, 90)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::ArchiveOfStars,
        descent: Descent::Ancients,
        name: "The Archive of Stars",
        signature: "Lore, star charts and puzzle vaults.",
        share_per_mille: 190,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(72, 72, 140),
            Rgb(52, 52, 108),
            Rgb(18, 18, 48),
            Some(Rgb(255, 255, 200)),
        ),
        ores: &[ore(Diamond, 500, 800, 40), ore(Mythril, 500, 600, 50)],
        layer_resource: resource("Star Glass", StrategicResourceKind::AncientAlloy),
        // Wave 10 ancients-a: the stacks' keepers and pests.
        creatures: &[
            (EnemyKind::PageMoth, 4),
            (EnemyKind::IndexSpider, 3),
            (EnemyKind::ArchiveWarden, 2),
        ],
        hazards: &[(Pressure, 5), (Explosive, 3)],
        field: FieldProfile {
            // Starlit and hushed: dimmer, a deep blue grade and a vignette.
            light_permille: 760,
            screen: &[
                screen(ScreenEffect::ColourGrade, 320),
                screen(ScreenEffect::Vignette, 240),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::ArchiveOfStars,
        ambience: AmbienceCue::Stacks,
        landmarks: &[
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::AncientCity,
        ],
        variants: &["Stacks", "Orrery Halls"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SleepingGarrison,
        descent: Descent::Ancients,
        name: "Sleeping Garrison",
        signature: "Dormant war machines wake in waves: hold a fort with turrets, barricades and \
                    traps until the garrison sleeps again.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::AncientMachineLayer),
        // Wave 10 ancients-b: olive drab and gunmetal, the alarm lamps' red.
        palette: palette(
            Rgb(98, 102, 84),
            Rgb(76, 84, 60),
            Rgb(30, 34, 24),
            Some(Rgb(255, 72, 48)),
        ),
        ores: &[ore(Mythril, 600, 800, 60), ore(Uranium, 400, 600, 30)],
        layer_resource: resource("War Plate", StrategicResourceKind::AncientAlloy),
        // Wave 10 ancients-b: the garrison's machines; the forts wake them in waves.
        creatures: &[
            (EnemyKind::WarSentry, 4),
            (EnemyKind::LancerDrone, 3),
            (EnemyKind::SiegeWalker, 1),
        ],
        hazards: &[(Explosive, 7), (Lava, 1)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Vignette, 220)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::SleepingGarrison,
        ambience: AmbienceCue::Barracks,
        landmarks: GARRISON,
        variants: &["Barracks", "Hangars"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheSealGate,
        descent: Descent::Ancients,
        name: "The Seal Gate",
        signature: "Tremors shake stones from the ceilings; a gate that opens with the keys of \
                    every Descent.",
        share_per_mille: 180,
        host_band: deep(DeepStratum::RadioactiveHollow),
        palette: palette(
            Rgb(196, 164, 72),
            Rgb(150, 120, 48),
            Rgb(64, 50, 18),
            Some(Rgb(255, 250, 200)),
        ),
        ores: &[ore(Mythril, 600, 800, 70), ore(Platinum, 400, 600, 30)],
        layer_resource: resource("Seal Gold", StrategicResourceKind::AncientAlloy),
        // Wave 10 ancients-b: the gate's wardens and its gilded crawlers.
        creatures: &[(EnemyKind::SealWarden, 3), (EnemyKind::GildedCrawler, 3)],
        hazards: &[(MagmaVent, 4), (Explosive, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Bloom, 160)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::SealGate,
        ambience: AmbienceCue::SealHall,
        landmarks: SEAL_GATE,
        variants: &["Collapsing Hall", "Key Altars"],
        gate: Some(GateDef {
            boss: DescentBoss::GatekeeperColossus,
            requires: &[
                GateRequirement::EveryEarlierKey,
                GateRequirement::Key(Key::PhaseDrill),
                GateRequirement::Boss(DescentBoss::GatekeeperColossus),
            ],
            rewards: &[Key::HeartKey],
        }),
        ..TEMPLATE
    },
    // --- Descent VIII: The Heart (Wave 10, authored: `crate::heart`) -------------------------
    StratumDef {
        stratum: Stratum::DarkMatterVeins,
        descent: Descent::Heart,
        name: "Dark Matter Veins",
        signature: "Negative-mass ore: every lump of dark matter in the bay makes the rig \
                    lighter; carry too much and it falls up onto the ceilings.",
        share_per_mille: 270,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Core(CoreTile::VeinShale),
        hard: TileKind::Core(CoreTile::NullStone),
        palette: palette(
            Rgb(88, 62, 122),
            Rgb(40, 30, 58),
            Rgb(16, 10, 30),
            Some(Rgb(196, 140, 255)),
        ),
        ores: &[
            ore(Mythril, 500, 800, 60),
            ore(Uranium, 500, 600, 30),
            ore(Diamond, 600, 500, 20),
        ],
        layer_resource: resource("Dark Matter", StrategicResourceKind::VoidGlass),
        creatures: &[
            (EnemyKind::NullMoth, 5),
            (EnemyKind::UpfallBeetle, 3),
            (EnemyKind::VeinGnawer, 2),
        ],
        hazards: &[(Pressure, 5), (Gas, 2)],
        field: FieldProfile {
            light_permille: 820,
            screen: &[
                screen(ScreenEffect::Vignette, 260),
                screen(ScreenEffect::ColourGrade, 180),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::DarkMatterVeins,
        ambience: AmbienceCue::VoidHum,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Heart,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Black Veins", "Weightless Halls"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SingularityFields,
        descent: Descent::Heart,
        name: "Singularity Fields",
        signature: "Gravity wells pull rigs, drops and creatures in, and bend the light; the \
                    horizon at a well's heart crushes hulls.",
        share_per_mille: 250,
        host_band: deep(DeepStratum::VoidGeodeFields),
        soft: TileKind::Core(CoreTile::EventShale),
        hard: TileKind::Core(CoreTile::HorizonRock),
        palette: palette(
            Rgb(44, 28, 64),
            Rgb(28, 24, 44),
            Rgb(6, 4, 14),
            Some(Rgb(255, 196, 255)),
        ),
        ores: &[ore(Mythril, 600, 800, 70), ore(Diamond, 400, 600, 30)],
        layer_resource: resource("Event Glass", StrategicResourceKind::VoidGlass),
        creatures: &[
            (EnemyKind::Orbiter, 4),
            (EnemyKind::AccretionMote, 5),
            (EnemyKind::HorizonLurker, 2),
        ],
        hazards: &[(Pressure, 6), (Gas, 2)],
        field: FieldProfile {
            // A faint bend everywhere; the wells' own field bends far more near them.
            screen: &[
                screen(ScreenEffect::Lensing, 120),
                screen(ScreenEffect::Vignette, 220),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::SingularityFields,
        ambience: AmbienceCue::Gravity,
        landmarks: &[
            LandmarkKind::Heart,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Wells", "Lens Fields"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::CoreOcean,
        descent: Descent::Heart,
        name: "Core Ocean",
        signature: "A liquid metal sea under a rock sky, crossed on magnetic rails: switches to \
                    the low line, boosters, jumps and the magnetic catch.",
        share_per_mille: 260,
        host_band: deep(DeepStratum::MantleStormZone),
        soft: TileKind::Core(CoreTile::CoreBasalt),
        hard: TileKind::Core(CoreTile::IronCrust),
        palette: palette(
            Rgb(112, 72, 84),
            Rgb(76, 60, 56),
            Rgb(40, 22, 18),
            Some(Rgb(255, 214, 150)),
        ),
        ores: &[ore(Platinum, 500, 800, 60), ore(Mythril, 500, 600, 40)],
        layer_resource: resource("Core Metal", StrategicResourceKind::CoreShard),
        creatures: &[
            (EnemyKind::MercuryEel, 4),
            (EnemyKind::RailLeech, 3),
            (EnemyKind::SlagSkimmer, 3),
        ],
        hazards: &[(Lava, 5), (MagmaVent, 3)],
        field: FieldProfile {
            light_permille: 1_100,
            screen: &[
                screen(ScreenEffect::Wobble, 160),
                screen(ScreenEffect::ColourGrade, 160),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::CoreOcean,
        ambience: AmbienceCue::MetalSea,
        landmarks: &[
            LandmarkKind::Heart,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Mercury Shallows", "Rail Spans"],
        fluids: &[FluidPocketRule::cave_pools(LiquidMetal, 120)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheHeart,
        descent: Descent::Heart,
        name: "The Heart",
        signature: "The sleeping Star Drive at the planet's centre, at the root of the \
                    builders' cable: wake it.",
        share_per_mille: 220,
        host_band: deep(DeepStratum::MantleStormZone),
        soft: TileKind::Core(CoreTile::StarRock),
        hard: TileKind::Core(CoreTile::DriveAlloy),
        palette: palette(
            Rgb(196, 140, 40),
            Rgb(150, 122, 82),
            Rgb(70, 44, 26),
            Some(Rgb(255, 250, 220)),
        ),
        ores: &[ore(Mythril, 700, 800, 70), ore(Diamond, 300, 600, 40)],
        layer_resource: resource("Star Shard", StrategicResourceKind::CoreShard),
        creatures: &[(EnemyKind::CoronaWisp, 5), (EnemyKind::CableWarden, 2)],
        hazards: &[(Lava, 4), (MagmaVent, 4)],
        field: FieldProfile {
            light_permille: 1_300,
            screen: &[
                screen(ScreenEffect::Bloom, 320),
                screen(ScreenEffect::ColourGrade, 140),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::TheHeart,
        ambience: AmbienceCue::StarDrive,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::Heart,
            LandmarkKind::Prefabs,
        ],
        variants: &["Drive Chamber", "Cable Root"],
        gate: Some(GateDef {
            boss: DescentBoss::TheHeart,
            requires: &[
                GateRequirement::Key(Key::HeartKey),
                GateRequirement::Boss(DescentBoss::TheHeart),
            ],
            rewards: &[Key::StarDrive],
        }),
        ..TEMPLATE
    },
];
