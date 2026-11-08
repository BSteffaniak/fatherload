#![allow(
    dead_code,
    clippy::cast_sign_loss,
    reason = "terrain bounds are validated before indexing"
)]

use serde::{Deserialize, Serialize};

use crate::surface::building_foundation_at;

/// World-space pixels per terrain tile.
pub const TILE_SIZE: f32 = 32.0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MineralKind {
    Copper,
    Iron,
    Silver,
    Gold,
    Emerald,
    Ruby,
    Diamond,
    Platinum,
    Uranium,
    Mythril,
}

impl MineralKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
            Self::Emerald => "Emerald",
            Self::Ruby => "Ruby",
            Self::Diamond => "Diamond",
            Self::Platinum => "Platinum",
            Self::Uranium => "Uranium",
            Self::Mythril => "Mythril",
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Copper => 8,
            Self::Iron => 14,
            Self::Silver => 26,
            Self::Gold => 44,
            Self::Emerald => 78,
            Self::Ruby => 120,
            Self::Diamond => 190,
            Self::Platinum => 260,
            Self::Uranium => 360,
            Self::Mythril => 520,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum StrategicResourceKind {
    AncientAlloy,
    CoreShard,
    CrystalLens,
    PressurePearl,
    MachineRelic,
    VoidGlass,
    RadiantFossil,
}

impl StrategicResourceKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AncientAlloy => "Ancient Alloy",
            Self::CoreShard => "Core Shard",
            Self::CrystalLens => "Crystal Lens",
            Self::PressurePearl => "Pressure Pearl",
            Self::MachineRelic => "Machine Relic",
            Self::VoidGlass => "Void Glass",
            Self::RadiantFossil => "Radiant Fossil",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DeepStratum {
    CrystalFaults,
    FossilOceans,
    PressureCathedrals,
    RadioactiveHollow,
    AncientMachineLayer,
    VoidGeodeFields,
    MantleStormZone,
}

impl DeepStratum {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CrystalFaults => "Crystal Faults",
            Self::FossilOceans => "Fossil Oceans",
            Self::PressureCathedrals => "Pressure Cathedrals",
            Self::RadioactiveHollow => "Radioactive Hollow",
            Self::AncientMachineLayer => "Ancient Machine Layer",
            Self::VoidGeodeFields => "Void Geode Fields",
            Self::MantleStormZone => "Mantle Storm Zone",
        }
    }

    #[must_use]
    pub const fn primary_resource(self) -> StrategicResourceKind {
        match self {
            Self::CrystalFaults => StrategicResourceKind::CrystalLens,
            Self::FossilOceans => StrategicResourceKind::RadiantFossil,
            Self::PressureCathedrals => StrategicResourceKind::PressurePearl,
            Self::RadioactiveHollow => StrategicResourceKind::CoreShard,
            Self::AncientMachineLayer => StrategicResourceKind::MachineRelic,
            Self::VoidGeodeFields => StrategicResourceKind::VoidGlass,
            Self::MantleStormZone => StrategicResourceKind::AncientAlloy,
        }
    }

    #[must_use]
    pub const fn primary_hazard(self) -> TileKind {
        match self {
            Self::CrystalFaults | Self::AncientMachineLayer => TileKind::ExplosivePocket,
            Self::FossilOceans => TileKind::Gas,
            Self::PressureCathedrals | Self::VoidGeodeFields => TileKind::PressurePocket,
            Self::RadioactiveHollow => TileKind::MagmaVent,
            Self::MantleStormZone => TileKind::Lava,
        }
    }

    #[must_use]
    pub const fn scanner_twist(self) -> &'static str {
        match self {
            Self::CrystalFaults => "crystal echoes duplicate ore pings",
            Self::FossilOceans => "fossil pockets hide artifacts inside soft seams",
            Self::PressureCathedrals => "pressure pockets mask nearby valuables",
            Self::RadioactiveHollow => "radiation ghosts mimic rare ore",
            Self::AncientMachineLayer => "machine relics reveal straight-line patterns",
            Self::VoidGeodeFields => "void glass bends distance estimates",
            Self::MantleStormZone => "storm heat scrambles hazard warnings",
        }
    }

    #[must_use]
    pub const fn unique_objective(self) -> &'static str {
        match self {
            Self::CrystalFaults => "stabilize a crystal fault survey beacon",
            Self::FossilOceans => "recover a radiant fossil slab",
            Self::PressureCathedrals => "seal a pressure cathedral pocket",
            Self::RadioactiveHollow => "scan a radioactive hollow anomaly",
            Self::AncientMachineLayer => "retrieve a machine relic component",
            Self::VoidGeodeFields => "reach a temporary void signal",
            Self::MantleStormZone => "return before a mantle storm surge",
        }
    }
}

#[must_use]
pub const fn deep_stratum_at_depth(depth: i32) -> Option<DeepStratum> {
    match depth {
        90..=109 => Some(DeepStratum::CrystalFaults),
        110..=129 => Some(DeepStratum::FossilOceans),
        130..=149 => Some(DeepStratum::PressureCathedrals),
        150..=169 => Some(DeepStratum::RadioactiveHollow),
        170..=189 => Some(DeepStratum::AncientMachineLayer),
        190..=209 => Some(DeepStratum::VoidGeodeFields),
        210.. => Some(DeepStratum::MantleStormZone),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ArtifactKind {
    Fossil,
    OldCircuit,
    BuriedIdol,
    StarCore,
    CrystalPrism,
    RadiantShell,
    PressureGlyph,
    ReactorBloom,
    MachineSigil,
    VoidPearl,
    MantleHeart,
}

impl ArtifactKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fossil => "Fossil",
            Self::OldCircuit => "Old Circuit",
            Self::BuriedIdol => "Buried Idol",
            Self::StarCore => "Star Core",
            Self::CrystalPrism => "Crystal Prism",
            Self::RadiantShell => "Radiant Shell",
            Self::PressureGlyph => "Pressure Glyph",
            Self::ReactorBloom => "Reactor Bloom",
            Self::MachineSigil => "Machine Sigil",
            Self::VoidPearl => "Void Pearl",
            Self::MantleHeart => "Mantle Heart",
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Fossil => 130,
            Self::OldCircuit => 240,
            Self::BuriedIdol => 420,
            Self::StarCore => 760,
            Self::CrystalPrism => 900,
            Self::RadiantShell => 980,
            Self::PressureGlyph => 1_080,
            Self::ReactorBloom => 1_180,
            Self::MachineSigil => 1_260,
            Self::VoidPearl => 1_420,
            Self::MantleHeart => 1_650,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum TileKind {
    Air,
    Dirt,
    Clay,
    Stone,
    HardRock,
    Foundation,
    Lava,
    Gas,
    ExplosivePocket,
    PressurePocket,
    MagmaVent,
    Ore(MineralKind),
    Artifact(ArtifactKind),
}

impl TileKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Air => "air",
            Self::Dirt => "dirt",
            Self::Clay => "clay",
            Self::Stone => "stone",
            Self::HardRock => "hard rock",
            Self::Foundation => "foundation",
            Self::Lava => "lava",
            Self::Gas => "gas",
            Self::ExplosivePocket => "explosive pocket",
            Self::PressurePocket => "pressure pocket",
            Self::MagmaVent => "magma vent",
            Self::Ore(mineral) => mineral.name(),
            Self::Artifact(artifact) => artifact.name(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tile {
    pub kind: TileKind,
    pub durability: u8,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct TilePosition {
    pub x: i32,
    pub y: i32,
}

/// One terrain tile write: the op-log replication unit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileChange {
    pub position: TilePosition,
    pub tile: Tile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Terrain {
    width: i32,
    height: i32,
    seed: u64,
    tiles: Vec<Tile>,
    /// Tile writes recorded while a simulation tick runs (never serialized or hashed).
    #[serde(skip)]
    journal: Option<Vec<TileChange>>,
}

impl Terrain {
    #[cfg(test)]
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        Self::new_seeded(width, height, 0xD1_11_6A_4E)
    }

    #[must_use]
    pub fn new_seeded(width: i32, height: i32, seed: u64) -> Self {
        let mut tiles = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            for x in 0..width {
                let kind = generated_tile_kind(x, y, seed);
                tiles.push(Tile {
                    kind,
                    durability: tile_durability(kind),
                });
            }
        }

        Self {
            width,
            height,
            seed,
            tiles,
            journal: None,
        }
    }

    /// Starts recording every tile write (see [`Self::finish_journal`]).
    pub(crate) fn start_journal(&mut self) {
        self.journal = Some(Vec::new());
    }

    /// Stops recording and returns the tile writes since [`Self::start_journal`], in order.
    pub(crate) fn finish_journal(&mut self) -> Vec<TileChange> {
        self.journal.take().unwrap_or_default()
    }

    fn record(&mut self, index: usize, position: TilePosition) {
        if let Some(journal) = self.journal.as_mut() {
            journal.push(TileChange {
                position,
                tile: self.tiles[index],
            });
        }
    }

    /// Applies one replicated tile write.
    pub fn apply_tile_change(&mut self, change: &TileChange) -> bool {
        self.set_tile_from_network(change.position, change.tile.kind, change.tile.durability)
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.height
    }

    #[must_use]
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    pub fn ensure_depth(&mut self, minimum_height: i32) {
        if minimum_height <= self.height {
            return;
        }
        self.tiles
            .reserve(((minimum_height - self.height) * self.width) as usize);
        for y in self.height..minimum_height {
            for x in 0..self.width {
                let kind = generated_tile_kind(x, y, self.seed);
                self.tiles.push(Tile {
                    kind,
                    durability: tile_durability(kind),
                });
            }
        }
        self.height = minimum_height;
    }

    #[must_use]
    pub fn tile(&self, position: TilePosition) -> Option<Tile> {
        self.index(position).map(|index| self.tiles[index])
    }

    #[must_use]
    pub fn is_solid_at(&self, position: TilePosition) -> bool {
        if position.y < 0 {
            return false;
        }

        self.tile(position)
            .is_some_and(|tile| tile.kind != TileKind::Air)
    }

    #[must_use]
    pub fn is_lava_at(&self, position: TilePosition) -> bool {
        self.tile(position)
            .is_some_and(|tile| tile.kind == TileKind::Lava)
    }

    #[must_use]
    pub fn hardness_at(&self, position: TilePosition) -> Option<u8> {
        self.tile(position).map(|tile| tile_hardness(tile.kind))
    }

    pub fn set_tile_from_network(
        &mut self,
        position: TilePosition,
        kind: TileKind,
        durability: u8,
    ) -> bool {
        let Some(index) = self.index(position) else {
            return false;
        };
        self.tiles[index] = Tile { kind, durability };
        self.record(index, position);
        true
    }

    pub fn set_kind(&mut self, position: TilePosition, kind: TileKind) -> bool {
        let Some(index) = self.index(position) else {
            return false;
        };
        self.tiles[index] = Tile {
            kind,
            durability: tile_durability(kind),
        };
        self.record(index, position);
        true
    }

    pub fn chip(&mut self, position: TilePosition) -> MineResult {
        let Some(index) = self.index(position) else {
            return MineResult::Blocked;
        };
        let before = self.tiles[index];
        let result = self.chip_index(index);
        let after = self.tiles[index];
        if before.kind != after.kind || before.durability != after.durability {
            self.record(index, position);
        }
        result
    }

    fn chip_index(&mut self, index: usize) -> MineResult {
        let tile = &mut self.tiles[index];

        if tile.kind == TileKind::Air {
            return MineResult::Blocked;
        }

        if matches!(
            tile.kind,
            TileKind::Lava | TileKind::MagmaVent | TileKind::Foundation
        ) {
            return MineResult::TooDangerous;
        }

        if tile.kind == TileKind::Gas {
            tile.kind = TileKind::Air;
            tile.durability = 0;
            return MineResult::Exploded;
        }

        if tile.kind == TileKind::ExplosivePocket {
            tile.kind = TileKind::Air;
            tile.durability = 0;
            return MineResult::Blast;
        }

        tile.durability = tile.durability.saturating_sub(1);
        if tile.durability > 0 {
            return MineResult::Chipped;
        }

        let mined = tile.kind;
        tile.kind = TileKind::Air;
        MineResult::Mined(mined)
    }

    pub fn blast_radius(&mut self, center: TilePosition, radius: i32) -> BlastResult {
        let mut result = BlastResult::default();
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if (x - center.x).abs() + (y - center.y).abs() > radius {
                    continue;
                }
                let position = TilePosition { x, y };
                let Some(index) = self.index(position) else {
                    continue;
                };
                let tile = &mut self.tiles[index];
                if !matches!(
                    tile.kind,
                    TileKind::Air | TileKind::Lava | TileKind::Foundation
                ) {
                    tile.kind = TileKind::Air;
                    tile.durability = 0;
                    result.cleared += 1;
                    result.changed_tiles.push(position);
                    self.record(index, position);
                }
            }
        }
        result
    }

    const fn index(&self, position: TilePosition) -> Option<usize> {
        if position.x < 0 || position.y < 0 || position.x >= self.width || position.y >= self.height
        {
            return None;
        }

        Some((position.y * self.width + position.x) as usize)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlastResult {
    pub cleared: u32,
    pub changed_tiles: Vec<TilePosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MineResult {
    Blocked,
    TooDangerous,
    Exploded,
    Blast,
    Chipped,
    Mined(TileKind),
}

fn generated_tile_kind(x: i32, y: i32, seed: u64) -> TileKind {
    if y <= 4 {
        return TileKind::Air;
    }

    if building_foundation_at(x, y) {
        return TileKind::Foundation;
    }

    if cave_air(x, y, seed) {
        return TileKind::Air;
    }

    if explosive_pocket(x, y, seed) {
        return TileKind::ExplosivePocket;
    }

    if pressure_pocket(x, y, seed) {
        return TileKind::PressurePocket;
    }

    if magma_vent(x, y, seed) {
        return TileKind::MagmaVent;
    }

    if lava_pocket(x, y, seed) {
        return TileKind::Lava;
    }

    if gas_pocket(x, y, seed) {
        return TileKind::Gas;
    }

    if let Some(stratum) = deep_stratum_at_depth(y)
        && seeded_hash(x, y, seed ^ 0xD33F).is_multiple_of(29)
    {
        return stratum.primary_hazard();
    }

    if artifact_spot(x, y, seed) {
        return TileKind::Artifact(artifact_at_depth(x, y));
    }

    if (46..=64).contains(&y) && (x + y).rem_euclid(5) == 0 {
        return ore_or_base_tile(x, y, TileKind::HardRock, seed);
    }

    let base = match y {
        5..=13 => TileKind::Dirt,
        14..=27 => TileKind::Clay,
        28..=64 | 90..=129 => TileKind::Stone,
        _ => TileKind::HardRock,
    };

    ore_or_base_tile(x, y, base, seed)
}

const fn cave_air(x: i32, y: i32, seed: u64) -> bool {
    if y < 12 {
        return false;
    }

    let cavern_mod = match y {
        0..=27 => 61,
        28..=47 => 43,
        48..=67 => 37,
        _ => 31,
    };
    let cavern = seeded_hash(x / 5, y / 4, seed) % cavern_mod;
    let seam = y > 44 && seeded_hash(x / 9, y, seed ^ 0xF00D).is_multiple_of(19);
    let tunnel = seeded_hash(x, y, seed ^ 0xA5A5) % 97;
    cavern == 0 || tunnel == 0 || seam
}

const fn explosive_pocket(x: i32, y: i32, seed: u64) -> bool {
    y > 28 && seeded_hash(x / 2, y / 2, seed ^ 0xE770).is_multiple_of(173)
}

const fn pressure_pocket(x: i32, y: i32, seed: u64) -> bool {
    y > 34 && y < 76 && seeded_hash(x / 3, y / 2, seed ^ 0x9A5).is_multiple_of(149)
}

const fn magma_vent(x: i32, y: i32, seed: u64) -> bool {
    y > 54 && (x + y).rem_euclid(7) == 0 && seeded_hash(x, y / 4, seed ^ 0xA11).is_multiple_of(41)
}

const fn lava_pocket(x: i32, y: i32, seed: u64) -> bool {
    if y < 48 {
        return false;
    }

    let pocket = seeded_hash(x / 4, y / 3, seed ^ 0x1A5A) % 53;
    pocket == 0 || (pocket <= 2 && y > 68)
}

const fn gas_pocket(x: i32, y: i32, seed: u64) -> bool {
    if y < 24 || y > 74 {
        return false;
    }

    let divisor = if y > 52 { 59 } else { 83 };
    seeded_hash(x / 3, y / 3, seed ^ 0x6A5).is_multiple_of(divisor)
}

const fn artifact_spot(x: i32, y: i32, seed: u64) -> bool {
    y > 20 && seeded_hash(x, y, seed ^ 0x5EED).is_multiple_of(211)
}

const fn artifact_at_depth(x: i32, y: i32) -> ArtifactKind {
    if let Some(stratum) = deep_stratum_at_depth(y) {
        return match stratum {
            DeepStratum::CrystalFaults => ArtifactKind::CrystalPrism,
            DeepStratum::FossilOceans => ArtifactKind::RadiantShell,
            DeepStratum::PressureCathedrals => ArtifactKind::PressureGlyph,
            DeepStratum::RadioactiveHollow => ArtifactKind::ReactorBloom,
            DeepStratum::AncientMachineLayer => ArtifactKind::MachineSigil,
            DeepStratum::VoidGeodeFields => ArtifactKind::VoidPearl,
            DeepStratum::MantleStormZone => ArtifactKind::MantleHeart,
        };
    }
    match y {
        0..=30 => ArtifactKind::Fossil,
        31..=50 => ArtifactKind::OldCircuit,
        51..=70 => {
            if (x + y).rem_euclid(2) == 0 {
                ArtifactKind::BuriedIdol
            } else {
                ArtifactKind::OldCircuit
            }
        }
        _ => ArtifactKind::StarCore,
    }
}

const fn ore_or_base_tile(x: i32, y: i32, base: TileKind, seed: u64) -> TileKind {
    if !patterned_ore(x, y, seed) {
        return base;
    }

    if let Some(stratum) = deep_stratum_at_depth(y) {
        return TileKind::Ore(match stratum {
            DeepStratum::CrystalFaults | DeepStratum::VoidGeodeFields => MineralKind::Diamond,
            DeepStratum::FossilOceans => MineralKind::Platinum,
            DeepStratum::PressureCathedrals | DeepStratum::AncientMachineLayer => {
                MineralKind::Mythril
            }
            DeepStratum::RadioactiveHollow => MineralKind::Uranium,
            DeepStratum::MantleStormZone => MineralKind::Ruby,
        });
    }

    match y {
        0..=16 => TileKind::Ore(MineralKind::Copper),
        17..=26 => TileKind::Ore(if (x + y).rem_euclid(3) == 0 {
            MineralKind::Silver
        } else {
            MineralKind::Iron
        }),
        27..=42 => TileKind::Ore(if (x * 5 + y).rem_euclid(5) == 0 {
            MineralKind::Gold
        } else {
            MineralKind::Silver
        }),
        43..=62 => TileKind::Ore(match (x * 13 + y * 7).rem_euclid(7) {
            0 => MineralKind::Ruby,
            1 | 2 => MineralKind::Emerald,
            _ => MineralKind::Gold,
        }),
        _ => TileKind::Ore(match (x * 19 + y * 23).rem_euclid(13) {
            0 => MineralKind::Mythril,
            1 => MineralKind::Uranium,
            2 | 3 => MineralKind::Platinum,
            4 | 5 => MineralKind::Diamond,
            6 | 7 => MineralKind::Ruby,
            _ => MineralKind::Emerald,
        }),
    }
}

const fn patterned_ore(x: i32, y: i32, seed: u64) -> bool {
    let vein_a = seeded_hash(x / 2, y / 2, seed ^ 0xC0DE) % 31;
    let vein_b = seeded_hash((x + y) / 3, y / 2, seed ^ 0xBEEF) % 43;
    let pocket = seeded_hash(x / 4, y / 3, seed ^ 0xFACE) % 23;
    vein_a <= 2 || vein_b <= 1 || pocket <= 1
}

const fn seeded_hash(x: i32, y: i32, seed: u64) -> u64 {
    let mut value =
        seed ^ ((x as u64).wrapping_mul(0x9E37_79B1)) ^ ((y as u64).wrapping_mul(0x85EB_CA77));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value
}

const fn tile_hardness(kind: TileKind) -> u8 {
    match kind {
        TileKind::Air
        | TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent
        | TileKind::Foundation => 0,
        TileKind::Dirt | TileKind::Ore(MineralKind::Copper | MineralKind::Iron) => 1,
        TileKind::Clay | TileKind::Ore(MineralKind::Silver | MineralKind::Gold) => 2,
        TileKind::Stone
        | TileKind::Ore(MineralKind::Emerald | MineralKind::Ruby)
        | TileKind::Artifact(ArtifactKind::Fossil | ArtifactKind::OldCircuit) => 3,
        TileKind::HardRock
        | TileKind::Ore(
            MineralKind::Diamond
            | MineralKind::Platinum
            | MineralKind::Uranium
            | MineralKind::Mythril,
        )
        | TileKind::Artifact(ArtifactKind::BuriedIdol | ArtifactKind::StarCore) => 4,
        TileKind::Artifact(
            ArtifactKind::CrystalPrism
            | ArtifactKind::RadiantShell
            | ArtifactKind::PressureGlyph
            | ArtifactKind::ReactorBloom
            | ArtifactKind::MachineSigil
            | ArtifactKind::VoidPearl
            | ArtifactKind::MantleHeart,
        ) => 5,
    }
}

const fn tile_durability(kind: TileKind) -> u8 {
    match kind {
        TileKind::Air => 0,
        TileKind::Dirt => 2,
        TileKind::Clay => 4,
        TileKind::Stone => 7,
        TileKind::HardRock | TileKind::Foundation => 11,
        TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => 1,
        TileKind::Ore(mineral) => tile_hardness(TileKind::Ore(mineral)) + 2,
        TileKind::Artifact(artifact) => tile_hardness(TileKind::Artifact(artifact)) + 5,
    }
}

#[cfg(test)]
mod tests {
    use super::{Terrain, TileKind, TilePosition};

    #[test]
    fn generated_world_contains_caves_and_hazards() {
        let terrain = Terrain::new(120, 90);
        let mut air_below_surface = 0;
        let mut lava = 0;
        let mut gas = 0;

        for y in 8..terrain.height() {
            for x in 0..terrain.width() {
                match terrain
                    .tile(TilePosition { x, y })
                    .expect("tile exists")
                    .kind
                {
                    TileKind::Air => air_below_surface += 1,
                    TileKind::Lava => lava += 1,
                    TileKind::Gas => gas += 1,
                    _ => {}
                }
            }
        }

        assert!(air_below_surface > 0);
        assert!(lava > 0);
        assert!(gas > 0);
    }
    #[test]
    fn deeper_biomes_have_more_fractured_air() {
        let terrain = Terrain::new(120, 90);
        let shallow_air = count_air(&terrain, 12, 28);
        let deep_air = count_air(&terrain, 58, 84);
        assert!(deep_air > shallow_air);
    }

    fn count_air(terrain: &Terrain, min_y: i32, max_y: i32) -> u32 {
        let mut total = 0;
        for y in min_y..=max_y {
            for x in 0..terrain.width() {
                if terrain
                    .tile(TilePosition { x, y })
                    .is_some_and(|tile| tile.kind == TileKind::Air)
                {
                    total += 1;
                }
            }
        }
        total
    }
}
