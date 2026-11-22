//! The stratum registry: all 41 strata of the eight Descents (expansion plan §14).
//!
//! Descents I and II are the twelve classic bands (they keep the classic ore curves, hazards,
//! creatures and art; their entries here describe them). Descents III to VIII are placeholders:
//! each borrows a classic band's rules ([`StratumDef::host_band`]) and carries its own palette,
//! ores, hazards, creature table (existing kinds standing in), field profile, cues, landmarks,
//! variants and gate until its Wave authors it for real.

use crate::{
    enemy::{EnemyKind, band_roster},
    terrain::{DeepStratum, MineralKind, StrategicResourceKind, TileKind},
    worldgen::{Band, HazardKind},
};

use super::{
    AmbienceCue, Descent, DescentBoss, DescentKey, FieldProfile, GateDef, GateRequirement,
    LandmarkKind, LayerResource, MusicCue, OreCurve, Rgb, ScreenEffect, ScreenEffectLevel, Stratum,
    StratumDef, StratumPalette,
};

use DescentKey as Key;
use EnemyKind::{
    AcidSpitter, AncientGuardian, BroodNest, Burrower, CaveBat, MagmaWisp, MiningDrone, RockMimic,
    RogueExcavator, SentryTurret, Shellback,
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

/// Every stratum, in [`Stratum::ALL`] order.
pub const STRATA: [StratumDef; super::STRATUM_COUNT] = [
    // --- Descent I: The Crust (the five classic bands, re-paced) -----------------------------
    StratumDef {
        stratum: Stratum::Topsoil,
        descent: Descent::Crust,
        name: "Topsoil",
        signature: "Roots and burrows, rain-soaked mud, first caves.",
        share_per_mille: 140,
        host_band: Band::Topsoil,
        soft: TileKind::Dirt,
        hard: TileKind::Clay,
        palette: palette(Rgb(126, 86, 52), Rgb(150, 98, 64), Rgb(58, 38, 26), None),
        ores: &[ore(Copper, 300, 700, 100), ore(Iron, 900, 400, 40)],
        layer_resource: resource("Root Fibre", StrategicResourceKind::RadiantFossil),
        creatures: band_roster(Band::Topsoil),
        hazards: &[],
        landmarks: &[
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::Settlements,
        ],
        variants: &["Rootbound", "Rain Mud", "Burrow Warren"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::ClayBeds,
        descent: Descent::Crust,
        name: "Clay Beds",
        signature: "Aquifers: water pockets that flood tunnels when breached.",
        share_per_mille: 190,
        host_band: Band::Clay,
        soft: TileKind::Clay,
        hard: TileKind::Stone,
        palette: palette(Rgb(168, 92, 60), Rgb(120, 110, 104), Rgb(70, 40, 30), None),
        ores: &[
            ore(Copper, 200, 600, 80),
            ore(Iron, 500, 500, 80),
            ore(Silver, 900, 400, 40),
        ],
        layer_resource: resource("Kiln Clay", StrategicResourceKind::RadiantFossil),
        creatures: band_roster(Band::Clay),
        hazards: &[(Gas, 1)],
        variants: &["Potters' Beds", "Aquifer Seams"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GreyStone,
        descent: Descent::Crust,
        name: "Grey Stone",
        signature: "Old mine works: rails to ride, timber that collapses, first gas.",
        share_per_mille: 220,
        host_band: Band::Stone,
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(Rgb(128, 128, 136), Rgb(92, 92, 104), Rgb(44, 44, 52), None),
        ores: &[
            ore(Iron, 200, 600, 80),
            ore(Silver, 500, 500, 60),
            ore(Gold, 900, 500, 46),
        ],
        layer_resource: resource("Mine Timber", StrategicResourceKind::MachineRelic),
        creatures: band_roster(Band::Stone),
        hazards: &[(Gas, 5), (Pressure, 2), (Explosive, 3)],
        landmarks: &[
            LandmarkKind::Outpost,
            LandmarkKind::Prefabs,
            LandmarkKind::CaveEcosystems,
            LandmarkKind::Settlements,
        ],
        variants: &["Old Workings", "Rail Galleries", "Gas Pockets"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::SlateShelves,
        descent: Descent::Crust,
        name: "Slate Shelves",
        signature: "Slate slides: slippery shelves that slide when undercut.",
        share_per_mille: 220,
        host_band: Band::Slate,
        soft: TileKind::Stone,
        hard: TileKind::HardRock,
        palette: palette(Rgb(84, 98, 120), Rgb(60, 70, 92), Rgb(30, 36, 50), None),
        ores: &[
            ore(Silver, 200, 500, 60),
            ore(Gold, 500, 500, 46),
            ore(Emerald, 900, 500, 34),
        ],
        layer_resource: resource("Shale Plate", StrategicResourceKind::CrystalLens),
        creatures: band_roster(Band::Slate),
        hazards: &[
            (Gas, 4),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 2),
            (Explosive, 3),
        ],
        variants: &["Stalactite Galleries", "Sliding Shelves"],
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
        palette: palette(Rgb(70, 70, 80), Rgb(44, 44, 54), Rgb(24, 24, 30), None),
        ores: &[
            ore(Gold, 200, 500, 46),
            ore(Emerald, 500, 500, 34),
            ore(Ruby, 900, 500, 26),
        ],
        layer_resource: resource("Basalt Fibre", StrategicResourceKind::VoidGlass),
        creatures: band_roster(Band::Basalt),
        hazards: &[
            (Gas, 2),
            (Lava, 4),
            (MagmaVent, 3),
            (Pressure, 2),
            (Explosive, 2),
        ],
        landmarks: WITH_GATE,
        variants: &["Lava Tubes", "Column Forest"],
        gate: Some(GateDef {
            boss: DescentBoss::TheSinkhole,
            requires: &[GateRequirement::Boss(DescentBoss::TheSinkhole)],
            rewards: &[Key::DeepClaim, Key::DrillII],
        }),
        ..TEMPLATE
    },
    // --- Descent II: The Old Deep (the seven classic deep strata, expanded) ------------------
    StratumDef {
        stratum: Stratum::CrystalFaults,
        descent: Descent::OldDeep,
        name: "Crystal Faults",
        signature: "Resonance chains and crystal refraction that lights hidden ore.",
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
        layer_resource: resource("Crystal Lens", StrategicResourceKind::CrystalLens),
        creatures: band_roster(deep(DeepStratum::CrystalFaults)),
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
        signature: "Gas tides, amber traps and fossil skeletons to excavate.",
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
        layer_resource: resource("Radiant Fossil", StrategicResourceKind::RadiantFossil),
        creatures: band_roster(deep(DeepStratum::FossilOceans)),
        hazards: &[
            (Gas, 8),
            (Lava, 2),
            (MagmaVent, 1),
            (Pressure, 1),
            (Explosive, 1),
        ],
        music: MusicCue::OldDeep,
        variants: &["Amber Reefs", "Bone Fields"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::PressureCathedrals,
        descent: Descent::OldDeep,
        name: "Pressure Cathedrals",
        signature: "Grand chambers, pressure jets and the Cathedral Lift.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::PressureCathedrals),
        palette: palette(
            Rgb(116, 108, 140),
            Rgb(84, 76, 108),
            Rgb(36, 32, 52),
            Some(Rgb(200, 190, 255)),
        ),
        ores: &[ore(Mythril, 500, 900, 60), ore(Platinum, 300, 600, 16)],
        layer_resource: resource("Pressure Pearl", StrategicResourceKind::PressurePearl),
        creatures: band_roster(deep(DeepStratum::PressureCathedrals)),
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
        signature: "A dose meter, mutation relics and a glowing green palette.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::RadioactiveHollow),
        palette: palette(
            Rgb(92, 128, 72),
            Rgb(64, 96, 52),
            Rgb(24, 40, 20),
            Some(Rgb(140, 255, 90)),
        ),
        ores: &[ore(Uranium, 500, 900, 60), ore(Platinum, 400, 600, 16)],
        layer_resource: resource("Core Shard", StrategicResourceKind::CoreShard),
        creatures: band_roster(deep(DeepStratum::RadioactiveHollow)),
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
        signature: "Machinery that wakes up, conveyors and pistons; first builder ruins.",
        share_per_mille: 145,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(120, 112, 96),
            Rgb(88, 80, 70),
            Rgb(40, 36, 32),
            Some(Rgb(255, 170, 60)),
        ),
        ores: &[ore(Mythril, 500, 900, 60), ore(Uranium, 400, 600, 12)],
        layer_resource: resource("Machine Relic", StrategicResourceKind::MachineRelic),
        creatures: band_roster(deep(DeepStratum::AncientMachineLayer)),
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
        signature: "Low-gravity pockets and floaty flight.",
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
        creatures: band_roster(deep(DeepStratum::VoidGeodeFields)),
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
        signature: "Heat storms and spreading lava.",
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
        layer_resource: resource("Core Shard", StrategicResourceKind::CoreShard),
        creatures: band_roster(deep(DeepStratum::MantleStormZone)),
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
        ..TEMPLATE
    },
    // --- Descent III: The Living Dark ----------------------------------------------------------
    StratumDef {
        stratum: Stratum::FungalForest,
        descent: Descent::LivingDark,
        name: "Fungal Forest",
        signature: "Giant mushrooms that bounce you; spore clouds blur and colour-shift.",
        share_per_mille: 210,
        host_band: deep(DeepStratum::FossilOceans),
        soft: TileKind::Stone,
        palette: palette(
            Rgb(150, 80, 170),
            Rgb(86, 56, 104),
            Rgb(36, 22, 46),
            Some(Rgb(170, 255, 140)),
        ),
        ores: &[ore(Emerald, 400, 700, 50), ore(Platinum, 700, 600, 30)],
        layer_resource: resource("Spore Silk", StrategicResourceKind::RadiantFossil),
        creatures: &[(CaveBat, 4), (AcidSpitter, 4), (Burrower, 2)],
        hazards: &[(Gas, 8), (Explosive, 1)],
        field: FieldProfile {
            screen: &[
                screen(ScreenEffect::Blur, 250),
                screen(ScreenEffect::ColourGrade, 400),
            ],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::LivingDark,
        ambience: AmbienceCue::Living,
        landmarks: WITH_OUTPOST,
        variants: &["Cap Canopy", "Spore Mists", "Puffball Hollows"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::RootLabyrinth,
        descent: Descent::LivingDark,
        name: "Root Labyrinth",
        signature: "Roots regrow behind you and close tunnels over minutes.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::CrystalFaults),
        soft: TileKind::Stone,
        palette: palette(
            Rgb(96, 112, 58),
            Rgb(82, 58, 38),
            Rgb(38, 26, 16),
            Some(Rgb(200, 230, 120)),
        ),
        ores: &[ore(Ruby, 400, 700, 40), ore(Diamond, 700, 600, 30)],
        layer_resource: resource("Heartwood", StrategicResourceKind::RadiantFossil),
        creatures: &[(Burrower, 5), (RockMimic, 3), (CaveBat, 2)],
        hazards: &[(Gas, 4), (Explosive, 4)],
        music: MusicCue::LivingDark,
        ambience: AmbienceCue::Living,
        variants: &["Tangle", "Taproot Shafts"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::TheHive,
        descent: Descent::LivingDark,
        name: "The Hive",
        signature: "An insect civilisation: one faction trades, the other swarms.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::AncientMachineLayer),
        palette: palette(
            Rgb(176, 132, 52),
            Rgb(132, 96, 36),
            Rgb(60, 40, 16),
            Some(Rgb(255, 220, 90)),
        ),
        ores: &[ore(Platinum, 500, 800, 50), ore(Gold, 300, 600, 40)],
        layer_resource: resource("Royal Jelly", StrategicResourceKind::PressurePearl),
        creatures: &[(BroodNest, 2), (AcidSpitter, 4), (CaveBat, 4)],
        hazards: &[(Gas, 4), (Pressure, 3)],
        music: MusicCue::LivingDark,
        ambience: AmbienceCue::Living,
        landmarks: &[
            LandmarkKind::Settlements,
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
        signature: "Pulsing walls heal mined tiles; acid tides; a heartbeat drives the music.",
        share_per_mille: 190,
        host_band: deep(DeepStratum::RadioactiveHollow),
        palette: palette(
            Rgb(160, 72, 84),
            Rgb(120, 48, 60),
            Rgb(58, 18, 26),
            Some(Rgb(255, 120, 140)),
        ),
        ores: &[ore(Ruby, 500, 800, 60), ore(Uranium, 700, 600, 20)],
        layer_resource: resource("Living Sinew", StrategicResourceKind::CoreShard),
        creatures: &[(AcidSpitter, 5), (Burrower, 3), (BroodNest, 1)],
        hazards: &[(Gas, 3), (MagmaVent, 5)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Wobble, 250)],
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::LivingDark,
        ambience: AmbienceCue::Living,
        variants: &["Throbbing Halls", "Acid Gullets"],
        ..TEMPLATE
    },
    StratumDef {
        stratum: Stratum::GlowSea,
        descent: Descent::LivingDark,
        name: "Glow Sea",
        signature: "Near-total darkness: the only light comes from creatures.",
        share_per_mille: 200,
        host_band: deep(DeepStratum::VoidGeodeFields),
        palette: palette(
            Rgb(36, 72, 84),
            Rgb(24, 50, 62),
            Rgb(8, 18, 24),
            Some(Rgb(90, 255, 220)),
        ),
        ores: &[ore(Diamond, 500, 800, 50), ore(Mythril, 700, 600, 30)],
        layer_resource: resource("Lure Light", StrategicResourceKind::CrystalLens),
        creatures: &[(CaveBat, 3), (RockMimic, 4), (Shellback, 2)],
        hazards: &[(Pressure, 4), (Gas, 3)],
        field: FieldProfile {
            light_permille: 250,
            ..FieldProfile::NEUTRAL
        },
        music: MusicCue::LivingDark,
        ambience: AmbienceCue::Water,
        landmarks: WITH_GATE,
        variants: &["Jelly Drifts", "Angler Deeps"],
        gate: Some(GateDef {
            boss: DescentBoss::MycelialMind,
            requires: &[
                GateRequirement::Key(Key::PressureHull),
                GateRequirement::Boss(DescentBoss::MycelialMind),
            ],
            rewards: &[Key::BioFilter, Key::LampII],
        }),
        ..TEMPLATE
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
            Rgb(176, 208, 228),
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
            Rgb(96, 128, 168),
            Some(Rgb(255, 240, 200)),
        ),
        ores: &[ore(Diamond, 500, 800, 40), ore(Platinum, 500, 600, 40)],
        layer_resource: resource("Sky Amber", StrategicResourceKind::AetherCrystal),
        creatures: &[(CaveBat, 5), (MagmaWisp, 2)],
        hazards: &[(Pressure, 3), (Gas, 2)],
        field: FieldProfile {
            light_permille: 1_400,
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
            Rgb(188, 196, 208),
            Rgb(140, 148, 164),
            Rgb(60, 64, 76),
            Some(Rgb(255, 255, 255)),
        ),
        ores: &[ore(Diamond, 500, 800, 50), ore(Silver, 500, 800, 50)],
        layer_resource: resource("Mirror Ore", StrategicResourceKind::CrystalLens),
        creatures: &[(RockMimic, 5), (CaveBat, 3)],
        hazards: &[(Explosive, 5), (Pressure, 2)],
        field: FieldProfile {
            screen: &[screen(ScreenEffect::Mirror, 0)],
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
            Rgb(196, 168, 150),
            Rgb(220, 120, 60),
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
        ..TEMPLATE
    },
];
