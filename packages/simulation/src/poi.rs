//! Points of interest (POIs): the hand-authored places generation stamps into a world, the
//! special tiles they are built from, what they pay out, and what a player knows about them.
//!
//! | Kind | Where | What | Reward |
//! |---|---|---|---|
//! | [`PoiKind::SupplyCache`] | every band below the topsoil | a small hollow with crates | fuel, bombs or credits per crate |
//! | [`PoiKind::DerelictRig`] | mid to deep | a crashed prospector rig | salvage credits, a layer material, sometimes a rig part, and a log entry |
//! | [`PoiKind::Vault`] | the deep strata (plus the Basalt Deeps) | a sealed room with blast-proof walls, opened by a small puzzle | a rare rig part (or materials once every part is owned) |
//! | [`PoiKind::Formation`] | the Grey Stone down | a fossil bed or geode, guarded by its layer's hazard | dense artifacts and ore |
//! | [`PoiKind::RivalClaim`] | mid layers | an abandoned competitor outpost | infrastructure kits |
//!
//! Everything here is deterministic: rewards are a pure function of the world seed, the tile's
//! position and its band, and the state of a POI lives in its tiles (crates disappear when
//! opened, switches light up, doors open) plus the vault relay timers and each player's
//! [`PoiJournal`]. POI tiles therefore replicate through the terrain op log and are saved with
//! the terrain like any other tile.
//!
//! ## Reward tables (per depth tier)
//!
//! The tier is [`reward_tier`]: 0 topsoil/clay, 1 grey stone/slate, 2 basalt/Crystal
//! Faults/Fossil Oceans, 3 Pressure Cathedrals/Radioactive Hollow/Ancient Machine Layer,
//! 4 Void Geode Fields/Mantle Storm Zone. Rewards are tuned to be a welcome top-up, never a
//! replacement for selling ore: a whole supply cache is worth about one good dive's worth of
//! fuel and bombs, and upgrades (base cost x level squared, 85-135 x 1..25) stay the main sink.
//!
//! | Source | Roll | Tier 0 | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
//! |---|---|---|---|---|---|---|
//! | Supply crate | 40 % fuel canister | +15 fuel | +25 | +35 | +45 | +55 |
//! | | 30 % bombs | 1 | 1 | 2 | 2 | 3 |
//! | | 30 % credits | 10-20 | 20-40 | 30-60 | 40-80 | 50-100 |
//! | Salvage core (derelict) | always | 40-80 cr | 80-140 | 120-200 | 160-260 | 200-320 |
//! | | layer material (tier 2+) | - | - | 1 | 1 | 2 |
//! | | rig part | 15 % chance of an unowned common part | | | | |
//! | | log entry | always (one per derelict) | | | | |
//! | Vault cache | always | an unowned rare part; once all are owned: a permanent fitting (+15 hull, +15 fuel capacity or +2 cargo) and 2 layer material | | | | |
//! | | credits | 100 x tier | | | | |
//! | Outpost salvage (rival claim) | always | one infrastructure kit + 10 x (tier + 1) credits | | | | |

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    infrastructure::InfrastructureKind,
    rig::RigPartKind,
    terrain::{DeepStratum, StrategicResourceKind, TilePosition},
    worldgen::{Band, FeatureId, TileRect, noise::hash2},
};

/// What kind of place a POI is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PoiKind {
    SupplyCache,
    DerelictRig,
    Vault,
    Formation,
    RivalClaim,
}

impl PoiKind {
    pub const ALL: [Self; 5] = [
        Self::SupplyCache,
        Self::DerelictRig,
        Self::Vault,
        Self::Formation,
        Self::RivalClaim,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SupplyCache => "Supply cache",
            Self::DerelictRig => "Derelict rig",
            Self::Vault => "Vault",
            Self::Formation => "Formation",
            Self::RivalClaim => "Rival claim",
        }
    }

    /// One-line description for the collection log.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::SupplyCache => "A hollow stocked with supply crates.",
            Self::DerelictRig => "A crashed prospector rig. Its core holds salvage and a log.",
            Self::Vault => "A sealed room with blast-proof walls. Solve its lock to get in.",
            Self::Formation => "A dense bed of artifacts or crystal, guarded by the layer.",
            Self::RivalClaim => "An abandoned outpost. Its lockers hold infrastructure kits.",
        }
    }

    /// Bands (by [`Band::index`]) this kind may be placed in.
    #[must_use]
    pub const fn band_window(self) -> (usize, usize) {
        match self {
            Self::SupplyCache => (1, 11),
            Self::DerelictRig => (2, 10),
            Self::Vault => (4, 11),
            Self::Formation => (2, 11),
            Self::RivalClaim => (2, 8),
        }
    }

    /// Whether this kind may be placed in `band`.
    #[must_use]
    pub const fn fits_band(self, band: Band) -> bool {
        let (first, last) = self.band_window();
        let index = band.index();
        index >= first && index <= last
    }
}

/// How a vault is opened.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VaultPuzzle {
    /// Drill the numbered switches in the antechamber floor in order; a wrong switch resets
    /// them all.
    Sequence,
    /// Bomb every pressure seal set into the outer wall.
    Seals,
    /// Drill both relays within [`RELAY_WINDOW_MILLIS`] of each other.
    Relay,
}

impl VaultPuzzle {
    pub const ALL: [Self; 3] = [Self::Sequence, Self::Seals, Self::Relay];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sequence => "sequence lock",
            Self::Seals => "pressure seals",
            Self::Relay => "twin relays",
        }
    }

    /// A short hint shown when the vault is first found.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Sequence => "Drill the floor switches in the order their pips show.",
            Self::Seals => "Blast every pressure seal in the wall to vent the lock.",
            Self::Relay => "Drill both relays before the first one cools down.",
        }
    }
}

/// Time both relays of a [`VaultPuzzle::Relay`] vault must be drilled within.
///
/// A clean solo run through the relay prefab's corridor takes about 7 s (rigs cross about
/// two tiles a second), so the window leaves a human pilot real slack; two rigs can split up.
pub const RELAY_WINDOW_MILLIS: u64 = 15_000;

/// What a formation is made of.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FormationStyle {
    /// Fossil-rich shale packed with artifacts.
    FossilBed,
    /// A hollow crystal shell lined with ore.
    Geode,
}

impl FormationStyle {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FossilBed => "fossil bed",
            Self::Geode => "geode",
        }
    }

    /// The formation style a band grows.
    #[must_use]
    pub const fn for_band(band: Band) -> Self {
        match band {
            Band::Topsoil
            | Band::Clay
            | Band::Stone
            | Band::Slate
            | Band::Deep(DeepStratum::FossilOceans | DeepStratum::RadioactiveHollow) => {
                Self::FossilBed
            }
            Band::Basalt | Band::Deep(_) => Self::Geode,
        }
    }
}

/// The specific flavour of one placed POI (its feature detail).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PoiVariant {
    SupplyCache,
    DerelictRig { log: LoreId },
    Vault(VaultPuzzle),
    Formation(FormationStyle),
    RivalClaim,
}

impl PoiVariant {
    #[must_use]
    pub const fn kind(self) -> PoiKind {
        match self {
            Self::SupplyCache => PoiKind::SupplyCache,
            Self::DerelictRig { .. } => PoiKind::DerelictRig,
            Self::Vault(_) => PoiKind::Vault,
            Self::Formation(_) => PoiKind::Formation,
            Self::RivalClaim => PoiKind::RivalClaim,
        }
    }

    /// Display name, e.g. "Vault (pressure seals)".
    #[must_use]
    pub fn title(self) -> String {
        match self {
            Self::Vault(puzzle) => format!("Vault ({})", puzzle.name()),
            Self::Formation(style) => match style {
                FormationStyle::FossilBed => "Fossil bed".to_owned(),
                FormationStyle::Geode => "Geode".to_owned(),
            },
            _ => self.kind().name().to_owned(),
        }
    }
}

/// The special tiles POIs are built from ([`crate::terrain::TileKind::Poi`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PoiTile {
    /// A supply crate: drill it open for fuel, bombs or credits.
    SupplyCrate,
    /// Hull plating of a wrecked rig (no loot).
    Wreckage,
    /// A derelict's core: salvage, sometimes a part, and a log entry.
    SalvageCore,
    /// Reinforced vault wall: no drill or bomb gets through.
    VaultWall,
    /// A vault door: blast-proof until the vault's lock opens, then it vanishes.
    VaultDoor,
    /// A pressure seal in a vault wall: only a bomb breaks it.
    VaultSeal,
    /// A seal that has been blasted and vented (as tough as the wall).
    VaultSealVented,
    /// A floor switch of a sequence lock; `order` is 1..=3.
    VaultSwitch { order: u8, lit: bool },
    /// A relay of a twin-relay lock.
    VaultRelay { lit: bool },
    /// The vault's prize.
    VaultCache,
    /// Fossil-rich shale around a fossil bed.
    FossilShale,
    /// The crystal shell of a geode.
    GeodeShell,
    /// Metal plating of an abandoned outpost.
    OutpostPlating,
    /// An outpost locker with salvageable infrastructure.
    OutpostSalvage,
}

/// Hardness of tiles no drill can dig.
pub const UNDRILLABLE: u8 = u8::MAX;

impl PoiTile {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SupplyCrate => "supply crate",
            Self::Wreckage => "wreckage",
            Self::SalvageCore => "salvage core",
            Self::VaultWall => "vault wall",
            Self::VaultDoor => "vault door",
            Self::VaultSeal => "pressure seal",
            Self::VaultSealVented => "vented seal",
            Self::VaultSwitch { lit: false, .. } => "vault switch",
            Self::VaultSwitch { lit: true, .. } => "lit vault switch",
            Self::VaultRelay { lit: false } => "vault relay",
            Self::VaultRelay { lit: true } => "lit vault relay",
            Self::VaultCache => "vault cache",
            Self::FossilShale => "fossil shale",
            Self::GeodeShell => "geode shell",
            Self::OutpostPlating => "outpost plating",
            Self::OutpostSalvage => "outpost locker",
        }
    }

    /// Drill strength needed to dig the tile ([`UNDRILLABLE`] for vault fixtures).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::SupplyCrate
            | Self::VaultSwitch { lit: false, .. }
            | Self::VaultRelay { lit: false } => 1,
            Self::Wreckage | Self::SalvageCore | Self::OutpostPlating | Self::OutpostSalvage => 2,
            Self::VaultCache | Self::FossilShale => 3,
            Self::GeodeShell => 4,
            Self::VaultWall
            | Self::VaultDoor
            | Self::VaultSeal
            | Self::VaultSealVented
            | Self::VaultSwitch { lit: true, .. }
            | Self::VaultRelay { lit: true } => UNDRILLABLE,
        }
    }

    /// Chips a fresh tile takes to dig.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::SupplyCrate | Self::VaultSwitch { .. } | Self::VaultRelay { .. } => 3,
            Self::Wreckage | Self::OutpostPlating => 4,
            Self::SalvageCore | Self::OutpostSalvage | Self::VaultCache | Self::FossilShale => 6,
            Self::GeodeShell => 9,
            Self::VaultWall | Self::VaultDoor | Self::VaultSeal | Self::VaultSealVented => 12,
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`]).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::SupplyCrate => 0,
            Self::Wreckage => 1,
            Self::SalvageCore => 2,
            Self::VaultWall => 3,
            Self::VaultDoor => 4,
            Self::VaultSeal => 5,
            Self::VaultSealVented => 6,
            Self::VaultSwitch { order, lit } => 0x10 | ((order as u64) << 1) | lit as u64,
            Self::VaultRelay { lit } => 0x20 | lit as u64,
            Self::VaultCache => 7,
            Self::FossilShale => 8,
            Self::GeodeShell => 9,
            Self::OutpostPlating => 10,
            Self::OutpostSalvage => 11,
        }
    }

    /// What a bomb blast leaves of this tile: `None` if it is blast-proof.
    ///
    /// Loot fixtures (crates, salvage cores, lockers, vault caches) shrug off blasts: a bomb has
    /// no owner to pay, and blowing up a derelict's core would lose its log for good.
    #[must_use]
    pub const fn after_blast(self) -> Option<Option<Self>> {
        match self {
            Self::VaultWall
            | Self::VaultDoor
            | Self::VaultSealVented
            | Self::VaultSwitch { .. }
            | Self::VaultRelay { .. }
            | Self::VaultCache
            | Self::SupplyCrate
            | Self::SalvageCore
            | Self::OutpostSalvage => None,
            Self::VaultSeal => Some(Some(Self::VaultSealVented)),
            Self::Wreckage | Self::FossilShale | Self::GeodeShell | Self::OutpostPlating => {
                Some(None)
            }
        }
    }

    /// The lit form of an unlit switch or relay (what drilling it through turns it into).
    #[must_use]
    pub const fn activated(self) -> Option<Self> {
        match self {
            Self::VaultSwitch { order, lit: false } => Some(Self::VaultSwitch { order, lit: true }),
            Self::VaultRelay { lit: false } => Some(Self::VaultRelay { lit: true }),
            _ => None,
        }
    }

    /// Whether the tile is part of a vault's shell or lock.
    #[must_use]
    pub const fn is_vault_fixture(self) -> bool {
        matches!(
            self,
            Self::VaultWall
                | Self::VaultDoor
                | Self::VaultSeal
                | Self::VaultSealVented
                | Self::VaultSwitch { .. }
                | Self::VaultRelay { .. }
        )
    }
}

/// Depth tier of rewards in `band` (see the module's reward tables).
#[must_use]
pub const fn reward_tier(band: Band) -> u8 {
    match band {
        Band::Topsoil | Band::Clay => 0,
        Band::Stone | Band::Slate => 1,
        Band::Basalt | Band::Deep(DeepStratum::CrystalFaults | DeepStratum::FossilOceans) => 2,
        Band::Deep(
            DeepStratum::PressureCathedrals
            | DeepStratum::RadioactiveHollow
            | DeepStratum::AncientMachineLayer,
        ) => 3,
        Band::Deep(DeepStratum::VoidGeodeFields | DeepStratum::MantleStormZone) => 4,
    }
}

/// The crafting material a band yields.
#[must_use]
pub const fn layer_material(band: Band) -> StrategicResourceKind {
    match band {
        Band::Topsoil | Band::Clay | Band::Stone | Band::Slate | Band::Basalt => {
            StrategicResourceKind::AncientAlloy
        }
        Band::Deep(stratum) => match stratum {
            DeepStratum::CrystalFaults => StrategicResourceKind::CrystalLens,
            DeepStratum::FossilOceans => StrategicResourceKind::RadiantFossil,
            DeepStratum::PressureCathedrals => StrategicResourceKind::PressurePearl,
            DeepStratum::RadioactiveHollow | DeepStratum::MantleStormZone => {
                StrategicResourceKind::CoreShard
            }
            DeepStratum::AncientMachineLayer => StrategicResourceKind::MachineRelic,
            DeepStratum::VoidGeodeFields => StrategicResourceKind::VoidGlass,
        },
    }
}

/// Common rig parts a derelict's core can hold.
pub const SALVAGE_PARTS: [RigPartKind; 6] = [
    RigPartKind::NeedleDrill,
    RigPartKind::LightweightEngine,
    RigPartKind::ImpactHull,
    RigPartKind::ProspectorScanner,
    RigPartKind::HazardScanner,
    RigPartKind::SortedCargoRack,
];

/// Rare rig parts a vault holds, in the order vaults hand them out.
pub const VAULT_PARTS: [RigPartKind; 6] = [
    RigPartKind::TitanDrill,
    RigPartKind::BurstEngine,
    RigPartKind::ThermalHull,
    RigPartKind::ResonanceDrill,
    RigPartKind::RelicScanner,
    RigPartKind::ArmoredCargoBay,
];

/// Infrastructure kits an outpost locker can hold, with weights.
const OUTPOST_KITS: [(InfrastructureKind, u32); 6] = [
    (InfrastructureKind::TunnelSupport, 5),
    (InfrastructureKind::SignalRelay, 4),
    (InfrastructureKind::CargoLift, 3),
    (InfrastructureKind::SurveyDrone, 3),
    (InfrastructureKind::PumpStation, 2),
    (InfrastructureKind::OreProcessor, 1),
];

/// A permanent rig improvement a vault can hold once every rare part is owned.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum RigFitting {
    /// One more crafted bulkhead (+15 maximum hull).
    Bulkhead,
    /// +15 fuel capacity.
    AuxiliaryTank,
    /// +2 cargo capacity.
    CargoRack,
}

impl RigFitting {
    pub const ALL: [Self; 3] = [Self::Bulkhead, Self::AuxiliaryTank, Self::CargoRack];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bulkhead => "vault bulkhead (+15 hull)",
            Self::AuxiliaryTank => "auxiliary tank (+15 fuel)",
            Self::CargoRack => "cargo rack (+2 cargo)",
        }
    }
}

/// What opening one POI tile paid out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PoiReward {
    pub credits: u32,
    pub fuel: u16,
    pub bombs: u8,
    pub material: Option<(StrategicResourceKind, u8)>,
    pub part: Option<RigPartKind>,
    pub fitting: Option<RigFitting>,
    pub kit: Option<InfrastructureKind>,
    pub log: Option<LoreId>,
}

impl PoiReward {
    /// A short "+25 fuel, +1 bomb" summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.fuel > 0 {
            parts.push(format!("+{} fuel", self.fuel));
        }
        if self.bombs > 0 {
            parts.push(format!(
                "+{} bomb{}",
                self.bombs,
                if self.bombs == 1 { "" } else { "s" }
            ));
        }
        if self.credits > 0 {
            parts.push(format!("+{} credits", self.credits));
        }
        if let Some((material, count)) = self.material {
            parts.push(format!("+{count} {}", material.name()));
        }
        if let Some(kit) = self.kit {
            parts.push(format!("{} kit", kit.name()));
        }
        if let Some(part) = self.part {
            parts.push(format!("rig part: {}", part.title()));
        }
        if let Some(fitting) = self.fitting {
            parts.push(fitting.name().to_owned());
        }
        if let Some(log) = self.log {
            parts.push(format!("log: {}", log.entry().title));
        }
        if parts.is_empty() {
            "nothing useful".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

const SALT_CRATE: u64 = 0x9017_0001;
const SALT_SALVAGE: u64 = 0x9017_0002;
const SALT_OUTPOST: u64 = 0x9017_0003;

/// A roll in `low..=high` from `hash`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the remainder is below the span"
)]
const fn roll(hash: u64, low: u32, high: u32) -> u32 {
    if high <= low {
        return low;
    }
    low + (hash % (high - low + 1) as u64) as u32
}

/// What the supply crate at `position` holds.
#[must_use]
pub const fn crate_reward(seed: u64, position: TilePosition, band: Band) -> PoiReward {
    let tier = reward_tier(band) as u32;
    let hash = hash2(seed, SALT_CRATE, position.x, position.y);
    let pick = hash % 100;
    let amount = hash >> 8;
    let mut reward = PoiReward {
        credits: 0,
        fuel: 0,
        bombs: 0,
        material: None,
        part: None,
        fitting: None,
        kit: None,
        log: None,
    };
    if pick < 40 {
        #[allow(clippy::cast_possible_truncation, reason = "at most 55 fuel")]
        {
            reward.fuel = (15 + 10 * tier) as u16;
        }
    } else if pick < 70 {
        #[allow(clippy::cast_possible_truncation, reason = "at most 3 bombs")]
        {
            reward.bombs = (1 + tier / 2) as u8;
        }
    } else {
        reward.credits = roll(amount, 10 * (tier + 1), 20 * (tier + 1));
    }
    reward
}

/// What a derelict's salvage core holds. `owned` are the parts the crew already has.
#[must_use]
pub fn salvage_reward(
    seed: u64,
    position: TilePosition,
    band: Band,
    log: LoreId,
    owned: &BTreeSet<RigPartKind>,
) -> PoiReward {
    let tier = u32::from(reward_tier(band));
    let hash = hash2(seed, SALT_SALVAGE, position.x, position.y);
    let credits = roll(hash >> 16, 40 + 40 * tier, 80 + 60 * tier);
    let material = match tier {
        0 | 1 => None,
        2 | 3 => Some((layer_material(band), 1)),
        _ => Some((layer_material(band), 2)),
    };
    let part = if hash % 100 < 15 {
        let unowned: Vec<RigPartKind> = SALVAGE_PARTS
            .into_iter()
            .filter(|part| !owned.contains(part))
            .collect();
        (!unowned.is_empty()).then(|| unowned[(hash >> 40) as usize % unowned.len()])
    } else {
        None
    };
    PoiReward {
        credits,
        material,
        part,
        log: Some(log),
        ..PoiReward::default()
    }
}

const SALT_VAULT: u64 = 0x9017_0004;

/// What the vault cache at `position` holds: an unowned rare part, or once the crew owns them
/// all, a permanent rig fitting and layer materials. `owned` are the parts the crew already
/// has.
#[must_use]
pub fn vault_reward(
    seed: u64,
    position: TilePosition,
    band: Band,
    owned: &BTreeSet<RigPartKind>,
) -> PoiReward {
    let tier = u32::from(reward_tier(band));
    let part = VAULT_PARTS.into_iter().find(|part| !owned.contains(part));
    let hash = hash2(seed, SALT_VAULT, position.x, position.y);
    let fitting = part
        .is_none()
        .then(|| RigFitting::ALL[usize::try_from(hash % 3).unwrap_or(0)]);
    PoiReward {
        credits: 100 * tier,
        material: part.is_none().then_some((layer_material(band), 2)),
        part,
        fitting,
        ..PoiReward::default()
    }
}

/// What the outpost locker at `position` holds.
#[must_use]
pub fn outpost_reward(seed: u64, position: TilePosition, band: Band) -> PoiReward {
    let tier = u32::from(reward_tier(band));
    let hash = hash2(seed, SALT_OUTPOST, position.x, position.y);
    let total: u64 = OUTPOST_KITS
        .iter()
        .map(|(_, weight)| u64::from(*weight))
        .sum();
    let mut pick = hash % total;
    let mut kit = OUTPOST_KITS[0].0;
    for (candidate, weight) in OUTPOST_KITS {
        if pick < u64::from(weight) {
            kit = candidate;
            break;
        }
        pick -= u64::from(weight);
    }
    PoiReward {
        credits: 10 * (tier + 1),
        kit: Some(kit),
        ..PoiReward::default()
    }
}

/// Id of one entry of the derelict log collection ([`LORE`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct LoreId(pub u16);

impl LoreId {
    /// The entry this id names (wrapping, so any id is valid).
    #[must_use]
    pub const fn entry(self) -> &'static LoreEntry {
        &LORE[self.0 as usize % LORE.len()]
    }
}

/// One recovered log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoreEntry {
    pub title: &'static str,
    pub text: &'static str,
}

/// The logs derelict rigs carry. Each derelict in a world carries a different one.
pub const LORE: [LoreEntry; 12] = [
    LoreEntry {
        title: "Day One",
        text: "Claim staked. The ground hums at night. Probably the generator.",
    },
    LoreEntry {
        title: "Cheap Fuel",
        text: "Bought the discount blend. Engine coughs below the clay. Lesson learned.",
    },
    LoreEntry {
        title: "Second Shift",
        text: "Found old drill marks down here. Older than the town. Nobody wants to talk about it.",
    },
    LoreEntry {
        title: "Hull Notes",
        text: "Crushed a strut on a landing. Patched it with crate wood. Do not tell the yard.",
    },
    LoreEntry {
        title: "The Echo",
        text: "Scanner keeps pinging something square. Rock is not square.",
    },
    LoreEntry {
        title: "Rival Outfit",
        text: "The other crew packed up overnight. Left their lockers full. Strange hurry.",
    },
    LoreEntry {
        title: "Glass Rain",
        text: "Hit a crystal seam and the whole ceiling sang. Backed off slowly.",
    },
    LoreEntry {
        title: "Sealed Door",
        text: "Walls here shrug off charges. Seals hiss when you knock. Someone built this.",
    },
    LoreEntry {
        title: "Low Fuel",
        text: "Should have turned back at half a tank. Beacon is on. Waiting.",
    },
    LoreEntry {
        title: "Warm Stone",
        text: "Rock is warm to the touch this deep. The radiator rattles like teeth.",
    },
    LoreEntry {
        title: "Last Haul",
        text: "Cargo full of the good stuff. Lift is out. Climbing the long way.",
    },
    LoreEntry {
        title: "Heartbeat",
        text: "At the bottom the ground thumps, slow and steady. I am going to find out why.",
    },
];

/// How much a player knows about one POI (ordered: each level includes the ones before).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PoiKnowledge {
    /// A low-tier scanner picked up a "strange echo": something is there.
    Echo,
    /// A high-tier scanner resolved its silhouette and kind.
    Identified,
    /// The player has been inside it.
    Visited,
}

/// One player's knowledge of the world's POIs and the logs they recovered.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PoiJournal {
    /// POIs the player has detected or visited, by feature id.
    #[serde(default)]
    pub known: BTreeMap<FeatureId, PoiKnowledge>,
    /// Derelict logs the player has recovered.
    #[serde(default)]
    pub logs: BTreeSet<LoreId>,
}

impl PoiJournal {
    /// Raises what the player knows about `poi` to at least `knowledge`. Returns whether it
    /// changed.
    pub fn learn(&mut self, poi: FeatureId, knowledge: PoiKnowledge) -> bool {
        match self.known.get_mut(&poi) {
            Some(current) if *current >= knowledge => false,
            Some(current) => {
                *current = knowledge;
                true
            }
            None => {
                self.known.insert(poi, knowledge);
                true
            }
        }
    }

    /// What the player knows about `poi`.
    #[must_use]
    pub fn knowledge(&self, poi: FeatureId) -> Option<PoiKnowledge> {
        self.known.get(&poi).copied()
    }

    /// Number of POIs the player has visited.
    #[must_use]
    pub fn visited_count(&self) -> usize {
        self.known
            .values()
            .filter(|knowledge| **knowledge == PoiKnowledge::Visited)
            .count()
    }
}

/// Range in tiles at which a scanner of `level` senses POIs (0: it does not).
#[must_use]
pub const fn scanner_poi_range(level: u8, relic_scanner: bool) -> i32 {
    let base = if level == 0 { 0 } else { 10 + 6 * level as i32 };
    if relic_scanner { base + 12 } else { base }
}

/// How clearly a scanner of `level` reads a POI in range: low tiers only hear an echo, from
/// [`IDENTIFY_SCANNER_LEVEL`] (or with the relic scanner part) the POI's silhouette and kind
/// resolve.
#[must_use]
pub const fn scanner_poi_knowledge(level: u8, relic_scanner: bool) -> Option<PoiKnowledge> {
    if relic_scanner || level >= IDENTIFY_SCANNER_LEVEL {
        Some(PoiKnowledge::Identified)
    } else if level > 0 {
        Some(PoiKnowledge::Echo)
    } else {
        None
    }
}

/// Whether any part of `bounds` lies within `range` tiles (Chebyshev) of `center`.
#[must_use]
pub const fn in_scanner_range(bounds: &TileRect, center: TilePosition, range: i32) -> bool {
    let dx = max0(max(bounds.min.x - center.x, center.x - bounds.max.x));
    let dy = max0(max(bounds.min.y - center.y, center.y - bounds.max.y));
    max(dx, dy) <= range
}

const fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

const fn max0(value: i32) -> i32 {
    max(value, 0)
}

/// What a player knows about the POI `id`: the clearer of their journal and a live reading.
///
/// The live reading comes from a scanner of `scanner_level` (with or without the relic scanner
/// part) at `center`, if `bounds` is in its range.
#[must_use]
pub fn poi_reading(
    journal: &PoiJournal,
    id: FeatureId,
    bounds: &TileRect,
    center: TilePosition,
    scanner_level: u8,
    relic_scanner: bool,
) -> Option<PoiKnowledge> {
    let live = scanner_poi_knowledge(scanner_level, relic_scanner).filter(|_| {
        in_scanner_range(
            bounds,
            center,
            scanner_poi_range(scanner_level, relic_scanner),
        )
    });
    journal.knowledge(id).max(live)
}

/// Scanner level from which POIs show as silhouettes with their kind instead of an echo.
pub const IDENTIFY_SCANNER_LEVEL: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_codes_are_unique() {
        let mut tiles = vec![
            PoiTile::SupplyCrate,
            PoiTile::Wreckage,
            PoiTile::SalvageCore,
            PoiTile::VaultWall,
            PoiTile::VaultDoor,
            PoiTile::VaultSeal,
            PoiTile::VaultSealVented,
            PoiTile::VaultRelay { lit: false },
            PoiTile::VaultRelay { lit: true },
            PoiTile::VaultCache,
            PoiTile::FossilShale,
            PoiTile::GeodeShell,
            PoiTile::OutpostPlating,
            PoiTile::OutpostSalvage,
        ];
        for order in 1..=3 {
            tiles.push(PoiTile::VaultSwitch { order, lit: false });
            tiles.push(PoiTile::VaultSwitch { order, lit: true });
        }
        let codes: BTreeSet<u64> = tiles.iter().map(|tile| tile.code()).collect();
        assert_eq!(codes.len(), tiles.len());
        assert!(codes.iter().all(|code| *code < 0x100));
    }

    #[test]
    fn rewards_scale_with_depth_but_stay_modest() {
        let seed = 7;
        let mut shallow = 0;
        let mut deep = 0;
        for x in 0..200 {
            let position = TilePosition { x, y: 30 };
            for (band, total) in [
                (Band::Clay, &mut shallow),
                (Band::Deep(DeepStratum::MantleStormZone), &mut deep),
            ] {
                let reward = crate_reward(seed, position, band);
                *total += reward.credits + u32::from(reward.fuel) + 18 * u32::from(reward.bombs);
                assert!(reward.credits <= 100 && reward.fuel <= 55 && reward.bombs <= 3);
            }
        }
        assert!(deep > shallow * 2, "deep {deep} shallow {shallow}");
        // A crate averages well under the price of a level-2 upgrade (340+ credits).
        assert!(deep / 200 < 80);
    }

    #[test]
    fn vaults_hand_out_unowned_parts_first_then_fittings() {
        let band = Band::Deep(DeepStratum::CrystalFaults);
        let position = TilePosition { x: 40, y: 100 };
        let mut owned = BTreeSet::new();
        for expected in VAULT_PARTS {
            let reward = vault_reward(1, position, band, &owned);
            assert_eq!(reward.part, Some(expected));
            assert_eq!(reward.fitting, None);
            owned.insert(expected);
        }
        let reward = vault_reward(1, position, band, &owned);
        assert_eq!(reward.part, None);
        assert!(reward.fitting.is_some());
        assert!(reward.material.is_some());
    }

    #[test]
    fn journals_only_ever_learn_more() {
        let mut journal = PoiJournal::default();
        let poi = FeatureId(3);
        assert!(journal.learn(poi, PoiKnowledge::Echo));
        assert!(!journal.learn(poi, PoiKnowledge::Echo));
        assert!(journal.learn(poi, PoiKnowledge::Visited));
        assert!(!journal.learn(poi, PoiKnowledge::Identified));
        assert_eq!(journal.knowledge(poi), Some(PoiKnowledge::Visited));
    }

    #[test]
    fn scanner_tiers_hint_then_identify() {
        assert_eq!(scanner_poi_knowledge(0, false), None);
        assert_eq!(scanner_poi_knowledge(1, false), Some(PoiKnowledge::Echo));
        assert_eq!(
            scanner_poi_knowledge(IDENTIFY_SCANNER_LEVEL, false),
            Some(PoiKnowledge::Identified)
        );
        assert_eq!(
            scanner_poi_knowledge(0, true),
            Some(PoiKnowledge::Identified)
        );
        assert!(scanner_poi_range(5, false) > scanner_poi_range(1, false));
    }
}
