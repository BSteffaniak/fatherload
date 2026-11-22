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
    terrain::{DeepStratum, MineralKind, StrategicResourceKind, TileKind},
    worldgen::{Band, HazardKind},
};

use crate::fluid::{
    FluidKind::{Acid, Brine, LiquidMetal, Water},
    FluidPocketRule,
};

use super::{
    AmbienceCue, Descent, DescentBoss, DescentKey, FieldProfile, FieldRegions, GateDef,
    GateRequirement, LandmarkKind, LayerResource, MusicCue, OreCurve, Rgb, ScreenEffect,
    ScreenEffectLevel, Stratum, StratumDef, StratumPalette,
};

use DescentKey as Key;
use EnemyKind::{
    AcidLancer, AcidSpitter, AncientGuardian, Angler, BarkBeetle, BilePolyp, Burrower, CapCrawler,
    CaveBat, FossilEel, GlowGnat, GlowGrub, GlowJelly, HiveDrone, HiveSoldier, JetRay, Leechling,
    LureFish, MagmaSalamander, MagmaWisp, MawWorm, MiningDrone, PistonWarden, PrismMoth, RockMimic,
    RogueExcavator, RootSnapper, SentryTurret, Shellback, SporePuffer, ThornWisp, VoidJelly,
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
/// Landmarks of a Strange Layers stratum (anomaly rooms).
const STRANGE: &[LandmarkKind] = &[LandmarkKind::AnomalyRoom, LandmarkKind::Prefabs];

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
    // --- Descent IV: The Drowned and Frozen ---------------------------------------------------
    StratumDef {
        stratum: Stratum::UndergroundOcean,
        descent: Descent::DrownedAndFrozen,
        name: "The Underground Ocean",
        signature: "A vast flooded region: submarine mode, currents and sonar.",
        share_per_mille: 240,
        host_band: deep(DeepStratum::FossilOceans),
        palette: palette(
            Rgb(52, 92, 140),
            Rgb(36, 66, 108),
            Rgb(12, 26, 50),
            Some(Rgb(120, 200, 255)),
        ),
        ores: &[ore(Platinum, 500, 800, 50), ore(Emerald, 300, 600, 30)],
        layer_resource: resource("Deep Pearl", StrategicResourceKind::PressurePearl),
        creatures: &[(Shellback, 4), (RockMimic, 3), (CaveBat, 2)],
        hazards: &[(Pressure, 6), (Gas, 2)],
        field: FieldProfile {
            drag_permille: 1_800,
            screen: &[screen(ScreenEffect::ColourGrade, 300)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::DrownedAndFrozen,
        ambience: AmbienceCue::Water,
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
        signature: "Ice physics: sliding, frozen-over tunnels, hull frost.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Stone,
        palette: palette(
            Rgb(140, 198, 238),
            Rgb(130, 166, 196),
            Rgb(60, 84, 108),
            Some(Rgb(240, 250, 255)),
        ),
        ores: &[ore(Diamond, 500, 800, 60), ore(Silver, 300, 600, 40)],
        layer_resource: resource("Brine Ice", StrategicResourceKind::MoonIce),
        creatures: &[(Shellback, 4), (CaveBat, 3), (RockMimic, 2)],
        hazards: &[(Pressure, 4), (Explosive, 3)],
        field: FieldProfile {
            friction_permille: 300,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::DrownedAndFrozen,
        ambience: AmbienceCue::Ice,
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
        signature: "Timed steam jets launch you upward: elevators or death traps.",
        share_per_mille: 180,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(176, 170, 132),
            Rgb(116, 106, 82),
            Rgb(54, 48, 36),
            Some(Rgb(255, 255, 220)),
        ),
        ores: &[ore(Mythril, 500, 800, 40), ore(Gold, 300, 600, 40)],
        layer_resource: resource("Sinter Crust", StrategicResourceKind::PressurePearl),
        creatures: &[(MagmaWisp, 3), (Shellback, 3), (MiningDrone, 2)],
        hazards: &[(Pressure, 8), (MagmaVent, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Fog, 300)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::DrownedAndFrozen,
        ambience: AmbienceCue::Water,
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
        signature: "A friendly fish-folk city in an air bubble: shop, quests, an outpost.",
        share_per_mille: 160,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(64, 150, 150),
            Rgb(44, 112, 116),
            Rgb(16, 52, 56),
            Some(Rgb(255, 210, 160)),
        ),
        ores: &[ore(Emerald, 500, 800, 40), ore(Platinum, 500, 600, 30)],
        layer_resource: resource("Coral Glass", StrategicResourceKind::CrystalLens),
        creatures: &[(Shellback, 2), (CaveBat, 2)],
        hazards: &[(Pressure, 3)],
        music: MusicCue::DrownedAndFrozen,
        ambience: AmbienceCue::Water,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Settlements,
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
        signature: "Crushing pressure: the screen darkens and desaturates as it rises.",
        share_per_mille: 220,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(40, 48, 92),
            Rgb(28, 32, 68),
            Rgb(8, 10, 28),
            Some(Rgb(230, 230, 255)),
        ),
        ores: &[ore(Mythril, 600, 800, 60), ore(Diamond, 400, 600, 30)],
        layer_resource: resource("Abyss Pearl", StrategicResourceKind::PressurePearl),
        creatures: &[(RockMimic, 4), (Shellback, 3), (MiningDrone, 2)],
        hazards: &[(Pressure, 8), (Gas, 1)],
        field: FieldProfile {
            light_permille: 500,
            screen: &[
                screen(ScreenEffect::Desaturate, 500),
                screen(ScreenEffect::Vignette, 450),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::DrownedAndFrozen,
        ambience: AmbienceCue::Silence,
        landmarks: WITH_GATE,
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
    // --- Descent V: The Hollow World -----------------------------------------------------------
    StratumDef {
        stratum: Stratum::InnerSky,
        descent: Descent::HollowWorld,
        name: "Inner Sky",
        signature: "A giant open cavern under a rock sky, with floating stones and wind.",
        share_per_mille: 330,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(150, 160, 176),
            Rgb(112, 120, 140),
            Rgb(77, 142, 213),
            Some(Rgb(255, 240, 200)),
        ),
        ores: &[ore(Diamond, 500, 800, 40), ore(Platinum, 500, 600, 40)],
        layer_resource: resource("Sky Amber", StrategicResourceKind::AetherCrystal),
        creatures: &[(CaveBat, 5), (MagmaWisp, 2)],
        hazards: &[(Pressure, 3), (Gas, 2)],
        field: FieldProfile {
            // Brighter than any cave: light past 1000 reads as exposure on screen.
            light_permille: 1_400,
            screen: &[screen(ScreenEffect::ColourGrade, 200)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::HollowWorld,
        ambience: AmbienceCue::Wind,
        landmarks: &[
            LandmarkKind::HollowCavern,
            LandmarkKind::Outpost,
            LandmarkKind::Prefabs,
        ],
        variants: &["Cloud Halls", "Stone Archipelago"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::MegafaunaJungle,
        descent: Descent::HollowWorld,
        name: "Megafauna Jungle",
        signature: "Dinosaurs and giant beasts; the jungle tribes' village.",
        share_per_mille: 250,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Stone,
        palette: palette(
            Rgb(52, 120, 44),
            Rgb(52, 92, 46),
            Rgb(22, 40, 20),
            Some(Rgb(255, 120, 200)),
        ),
        ores: &[ore(Emerald, 500, 800, 60), ore(Gold, 300, 600, 40)],
        layer_resource: resource("Giant Hide", StrategicResourceKind::RadiantFossil),
        creatures: &[(Burrower, 4), (Shellback, 4), (AcidSpitter, 2)],
        hazards: &[(Gas, 4), (Explosive, 2)],
        music: MusicCue::HollowWorld,
        ambience: AmbienceCue::Living,
        landmarks: &[
            LandmarkKind::Settlements,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
        ],
        variants: &["Fern Valleys", "Tar Pits"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::UpsideDownIsles,
        descent: Descent::HollowWorld,
        name: "Upside-Down Isles",
        signature: "Local inverted gravity: islands hang from the ceiling.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(196, 164, 120),
            Rgb(150, 124, 88),
            Rgb(70, 56, 40),
            Some(Rgb(160, 220, 255)),
        ),
        ores: &[ore(Mythril, 500, 800, 50), ore(Platinum, 300, 600, 30)],
        layer_resource: resource("Lodestone", StrategicResourceKind::VoidGlass),
        creatures: &[(CaveBat, 4), (RockMimic, 3), (MiningDrone, 2)],
        hazards: &[(Pressure, 5), (Explosive, 2)],
        field: FieldProfile {
            free_rotation: true,
            regions: FieldRegions::GravityTurns {
                per_mille: 350,
                turns: &[180],
            },
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::HollowWorld,
        ambience: AmbienceCue::Wind,
        variants: &["Hanging Isles", "Flip Reefs"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheSunforge,
        descent: Descent::HollowWorld,
        name: "The Sunforge",
        signature: "Overexposure near the inner sun: blooming light and extreme heat.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::MantleStormZone),
        palette: palette(
            Rgb(232, 188, 92),
            Rgb(196, 140, 60),
            Rgb(120, 70, 30),
            Some(Rgb(255, 255, 230)),
        ),
        ores: &[ore(Mythril, 600, 800, 60), ore(Ruby, 400, 600, 30)],
        layer_resource: resource("Sun Glass", StrategicResourceKind::StormGlass),
        creatures: &[(MagmaWisp, 6), (RogueExcavator, 2)],
        hazards: &[(Lava, 8), (MagmaVent, 3)],
        field: FieldProfile {
            light_permille: 1_800,
            screen: &[
                screen(ScreenEffect::Exposure, 600),
                screen(ScreenEffect::Bloom, 500),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::HollowWorld,
        ambience: AmbienceCue::Lava,
        landmarks: WITH_GATE,
        variants: &["Shadow Paths", "Solar Flats"],
        gate: Some(GateDef {
            boss: DescentBoss::SunSerpent,
            requires: &[
                GateRequirement::Key(Key::CryoHull),
                GateRequirement::Boss(DescentBoss::SunSerpent),
            ],
            rewards: &[Key::GravityAnchor],
        }),
        fluids: &[FluidPocketRule::sealed(LiquidMetal, 80)],
        ..TEMPLATE
    },
    // --- Descent VI: The Strange Layers --------------------------------------------------------
    StratumDef {
        stratum: Stratum::MirrorStrata,
        descent: Descent::StrangeLayers,
        name: "Mirror Strata",
        signature: "Screen and controls mirror in zones; mirror twins copy your moves.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::CrystalFaults),
        palette: palette(
            Rgb(232, 236, 244),
            Rgb(140, 148, 164),
            Rgb(60, 64, 76),
            Some(Rgb(255, 255, 255)),
        ),
        ores: &[ore(Diamond, 500, 800, 50), ore(Silver, 500, 800, 50)],
        layer_resource: resource("Mirror Ore", StrategicResourceKind::CrystalLens),
        creatures: &[(RockMimic, 5), (CaveBat, 3)],
        hazards: &[(Explosive, 5), (Pressure, 2)],
        field: FieldProfile {
            regions: FieldRegions::Mirrored { per_mille: 400 },
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Strange,
        landmarks: &[
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Outpost,
            LandmarkKind::Prefabs,
        ],
        variants: &["Glass Halls", "Twin Galleries"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GravityShear,
        descent: Descent::StrangeLayers,
        name: "Gravity Shear",
        signature: "Gravity rotates 90 and 180 degrees in regions; walk walls and ceilings.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(200, 60, 130),
            Rgb(124, 50, 120),
            Rgb(52, 18, 52),
            Some(Rgb(255, 160, 255)),
        ),
        ores: &[ore(Mythril, 500, 800, 50), ore(Platinum, 400, 600, 30)],
        layer_resource: resource("Shear Stone", StrategicResourceKind::VoidGlass),
        creatures: &[(CaveBat, 4), (MiningDrone, 3), (RockMimic, 2)],
        hazards: &[(Pressure, 6), (Explosive, 2)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Wobble, 150)],
            free_rotation: true,
            regions: FieldRegions::GravityTurns {
                per_mille: 450,
                turns: &[90, 180, 270],
            },
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Strange,
        landmarks: STRANGE,
        variants: &["Turned Halls", "Shear Lines"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PixelRot,
        descent: Descent::StrangeLayers,
        name: "Pixel Rot",
        signature: "The deeper you go, the lower-resolution the world gets.",
        share_per_mille: 170,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(64, 200, 96),
            Rgb(40, 148, 68),
            Rgb(10, 50, 24),
            Some(Rgb(255, 60, 255)),
        ),
        ores: &[ore(Uranium, 500, 800, 40), ore(Mythril, 500, 600, 40)],
        layer_resource: resource("Data Ore", StrategicResourceKind::MachineRelic),
        creatures: &[(MiningDrone, 4), (SentryTurret, 2), (CaveBat, 2)],
        hazards: &[(Explosive, 6), (MagmaVent, 2)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::Pixelate, 400),
                screen(ScreenEffect::PaletteReduce, 400),
            ],
            deepening: true,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Strange,
        landmarks: STRANGE,
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
        creatures: &[(Burrower, 3), (AcidSpitter, 3), (Shellback, 3)],
        hazards: &[(MagmaVent, 5), (Gas, 3)],
        field: FieldProfile {
            time_scale_permille: 800,
            screen: &[screen(ScreenEffect::ColourGrade, 350)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Strange,
        landmarks: STRANGE,
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
        creatures: &[(RockMimic, 4), (CaveBat, 4)],
        hazards: &[(Pressure, 5), (Gas, 2)],
        field: FieldProfile {
            light_permille: 100,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Silence,
        landmarks: STRANGE,
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
            Rgb(96, 200, 216),
            Rgb(220, 120, 200),
            Rgb(40, 60, 88),
            Some(Rgb(255, 255, 120)),
        ),
        ores: &[ore(Diamond, 600, 800, 60), ore(Emerald, 400, 600, 30)],
        layer_resource: resource("Prism Sand", StrategicResourceKind::CrystalLens),
        creatures: &[(RockMimic, 3), (MiningDrone, 3), (SentryTurret, 2)],
        hazards: &[(Explosive, 5), (Pressure, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::ChromaticAberration, 500)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::StrangeLayers,
        ambience: AmbienceCue::Strange,
        landmarks: &[
            LandmarkKind::GateChamber,
            LandmarkKind::AnomalyRoom,
            LandmarkKind::Prefabs,
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
        creatures: &[(MiningDrone, 4), (SentryTurret, 3), (AncientGuardian, 1)],
        hazards: &[(Explosive, 5), (Pressure, 2)],
        music: MusicCue::Ancients,
        ambience: AmbienceCue::Machines,
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Settlements,
            LandmarkKind::Prefabs,
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
        creatures: &[(MiningDrone, 4), (RogueExcavator, 3), (SentryTurret, 2)],
        hazards: &[(Explosive, 6), (MagmaVent, 2)],
        music: MusicCue::Ancients,
        ambience: AmbienceCue::Machines,
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
        layer_resource: resource("Star Chart", StrategicResourceKind::AncientAlloy),
        creatures: &[(SentryTurret, 3), (AncientGuardian, 1), (MiningDrone, 3)],
        hazards: &[(Pressure, 5), (Explosive, 3)],
        music: MusicCue::Ancients,
        ambience: AmbienceCue::Silence,
        variants: &["Stacks", "Orrery Halls"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SleepingGarrison,
        descent: Descent::Ancients,
        name: "Sleeping Garrison",
        signature: "Dormant war machines that wake in waves.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(88, 92, 84),
            Rgb(72, 80, 52),
            Rgb(30, 34, 20),
            Some(Rgb(255, 60, 40)),
        ),
        ores: &[ore(Mythril, 600, 800, 60), ore(Uranium, 400, 600, 30)],
        layer_resource: resource("War Plate", StrategicResourceKind::AncientAlloy),
        creatures: &[(SentryTurret, 4), (RogueExcavator, 3), (AncientGuardian, 2)],
        hazards: &[(Explosive, 7), (Lava, 1)],
        music: MusicCue::Ancients,
        ambience: AmbienceCue::Machines,
        variants: &["Barracks", "Hangars"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheSealGate,
        descent: Descent::Ancients,
        name: "The Seal Gate",
        signature: "A gate that opens with the keys of every Descent.",
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
        creatures: &[(AncientGuardian, 2), (SentryTurret, 3)],
        hazards: &[(MagmaVent, 4), (Explosive, 3)],
        music: MusicCue::Ancients,
        ambience: AmbienceCue::Silence,
        landmarks: WITH_GATE,
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
    // --- Descent VIII: The Heart --------------------------------------------------------------
    StratumDef {
        stratum: Stratum::DarkMatterVeins,
        descent: Descent::Heart,
        name: "Dark Matter Veins",
        signature: "Negative-mass ore: the rig gets lighter the more it carries.",
        share_per_mille: 270,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(52, 36, 80),
            Rgb(36, 24, 60),
            Rgb(8, 4, 18),
            Some(Rgb(180, 120, 255)),
        ),
        ores: &[ore(Mythril, 500, 800, 70), ore(Uranium, 500, 600, 30)],
        layer_resource: resource("Dark Matter", StrategicResourceKind::VoidGlass),
        creatures: &[(MagmaWisp, 3), (MiningDrone, 3), (RockMimic, 3)],
        hazards: &[(Pressure, 5), (Lava, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Vignette, 300)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::Heart,
        ambience: AmbienceCue::Strange,
        landmarks: WITH_OUTPOST,
        variants: &["Black Veins", "Weightless Halls"],
        fluids: &[FluidPocketRule::sealed(Acid, 120)],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SingularityFields,
        descent: Descent::Heart,
        name: "Singularity Fields",
        signature: "Gravity wells pull everything in, and light bends.",
        share_per_mille: 250,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(28, 24, 40),
            Rgb(60, 40, 90),
            Rgb(4, 2, 10),
            Some(Rgb(255, 200, 255)),
        ),
        ores: &[ore(Mythril, 600, 800, 70), ore(Diamond, 400, 600, 30)],
        layer_resource: resource("Event Glass", StrategicResourceKind::VoidGlass),
        creatures: &[(MagmaWisp, 4), (RogueExcavator, 3)],
        hazards: &[(Pressure, 6), (Lava, 3)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Lensing, 500)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::Heart,
        ambience: AmbienceCue::Strange,
        variants: &["Wells", "Lens Fields"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::CoreOcean,
        descent: Descent::Heart,
        name: "Core Ocean",
        signature: "A liquid metal sea; ride magnetic rails over it.",
        share_per_mille: 260,
        host_band: deep(DeepStratum::MantleStormZone),
        palette: palette(
            Rgb(180, 190, 203),
            Rgb(197, 107, 53),
            Rgb(70, 40, 30),
            Some(Rgb(255, 230, 160)),
        ),
        ores: &[ore(Mythril, 500, 800, 80), ore(Platinum, 500, 600, 40)],
        layer_resource: resource("Core Metal", StrategicResourceKind::CoreShard),
        creatures: &[(MagmaWisp, 5), (RogueExcavator, 3)],
        hazards: &[(Lava, 8), (MagmaVent, 3)],
        music: MusicCue::Heart,
        ambience: AmbienceCue::Lava,
        variants: &["Mercury Shallows", "Rail Spans"],
        fluids: &[
            FluidPocketRule::sealed(LiquidMetal, 240),
            FluidPocketRule::cave_pools(LiquidMetal, 200),
        ],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheHeart,
        descent: Descent::Heart,
        name: "The Heart",
        signature: "The sleeping Star Drive at the planet's centre.",
        share_per_mille: 220,
        host_band: deep(DeepStratum::MantleStormZone),
        palette: palette(
            Rgb(240, 220, 160),
            Rgb(200, 160, 90),
            Rgb(90, 60, 30),
            Some(Rgb(255, 255, 255)),
        ),
        ores: &[ore(Mythril, 700, 800, 90), ore(Uranium, 300, 600, 40)],
        layer_resource: resource("Star Shard", StrategicResourceKind::CoreShard),
        creatures: &[(MagmaWisp, 4), (AncientGuardian, 2)],
        hazards: &[(Lava, 6), (MagmaVent, 4)],
        field: FieldProfile {
            light_permille: 1_500,
            screen: &[screen(ScreenEffect::Bloom, 400)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::Heart,
        ambience: AmbienceCue::Living,
        landmarks: WITH_GATE,
        variants: &["Drive Chamber", "Cable Root"],
        gate: Some(GateDef {
            boss: DescentBoss::TheHeart,
            requires: &[
                GateRequirement::Key(Key::HeartKey),
                GateRequirement::Boss(DescentBoss::TheHeart),
            ],
            rewards: &[Key::StarDrive],
        }),
        fluids: &[FluidPocketRule::sealed(LiquidMetal, 120)],
        ..TEMPLATE
    },
];
