//! Generalized items and item containers (expansion plan §3 F1).
//!
//! Every thing a rig can carry is an [`ItemKind`]: minerals, artifacts, strategic materials,
//! building blocks, modules, weapons, ammo, explosives, cosmetics and quest items. A carried
//! quantity lives in an [`ItemContainer`] (a count per kind). Each kind says which container
//! it is stowed in ([`ItemKind::stowage`]) and how many capacity slots one unit takes
//! ([`ItemKind::slot_cost`]):
//!
//! | Category | Stowed in | Slots per unit |
//! |---|---|---|
//! | minerals, artifacts | cargo bay | 1 |
//! | strategic materials | cargo bay | 0 (as before: they never crowded out ore) |
//! | building blocks | block hopper | 1 (of the hopper's own capacity) |
//! | modules, weapons | cargo bay | 1 |
//! | ammo, explosives, cosmetics, quest items | cargo bay | 0 |
//!
//! A rig's cargo bay ([`crate::Player::cargo_bay`]) holds what used to be the separate
//! mineral, artifact and material maps; its capacity stays the rig stat
//! [`crate::Player::cargo_capacity`]. Building blocks go to the separate block hopper
//! ([`crate::Player::block_hopper`], capacity [`crate::Player::block_hopper_capacity`]) so dirt
//! never crowds ore out of the bay (WP-DROPS).
//!
//! Content for the open-ended categories (modules, weapons, ammo, explosives, cosmetics, quest
//! items) belongs to the tracks that add those systems; the ids here are small stable
//! newtypes so those tracks can define their tables without touching this module.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::terrain::{ArtifactKind, MineralKind, StrategicResourceKind, SurfaceMaterial, TileKind};

/// A building block: what a plain broken tile becomes (WP-DROPS, used by WP-BASE).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum BlockKind {
    Dirt,
    Clay,
    Stone,
    HardRock,
}

impl BlockKind {
    /// Every block kind, in order.
    pub const ALL: [Self; 4] = [Self::Dirt, Self::Clay, Self::Stone, Self::HardRock];

    /// The block a plain tile breaks into (`None` for anything that is not a plain block).
    #[must_use]
    pub const fn from_tile(kind: TileKind) -> Option<Self> {
        // Surface-biome materials break into the classic block of the same hardness, so
        // building (WP-BASE) needs no new block kinds; snow melts away. Cave rock (WP-CAVES)
        // breaks into the plain block it is closest to; living and glowing cave tiles, crystal
        // and caches (which pay out their own loot) give none.
        match kind {
            TileKind::Dirt
            | TileKind::Surface(SurfaceMaterial::Sand | SurfaceMaterial::Mud)
            | TileKind::Sky(
                crate::sky::SkyTile::Turf(_)
                | crate::sky::SkyTile::Soil
                | crate::sky::SkyTile::Roots
                | crate::sky::SkyTile::Vines
                | crate::sky::SkyTile::Spring
                | crate::sky::SkyTile::Regolith,
            )
            | TileKind::Cave(crate::cave::CaveTile::Mycelium | crate::cave::CaveTile::Silt) => {
                Some(Self::Dirt)
            }
            TileKind::Clay
            | TileKind::Surface(SurfaceMaterial::Sandstone | SurfaceMaterial::RedRock)
            | TileKind::Sky(crate::sky::SkyTile::Rock(_) | crate::sky::SkyTile::Vent) => {
                Some(Self::Clay)
            }
            TileKind::Stone
            | TileKind::Surface(SurfaceMaterial::Granite)
            | TileKind::Sky(
                crate::sky::SkyTile::Crystal
                | crate::sky::SkyTile::MoonRock
                | crate::sky::SkyTile::RuinBrick
                | crate::sky::SkyTile::RuinPillar
                | crate::sky::SkyTile::Keystone,
            )
            | TileKind::Cave(
                crate::cave::CaveTile::RuinBrick
                | crate::cave::CaveTile::RuinPillar
                | crate::cave::CaveTile::RuinGlyph
                | crate::cave::CaveTile::FossilRib
                | crate::cave::CaveTile::Stalactite,
            ) => Some(Self::Stone),
            TileKind::HardRock
            | TileKind::Cave(crate::cave::CaveTile::Obsidian)
            | TileKind::Sky(crate::sky::SkyTile::ContinentRock | crate::sky::SkyTile::BuoyPad) => {
                Some(Self::HardRock)
            }
            _ => None,
        }
    }

    /// The tile a placed block becomes.
    #[must_use]
    pub const fn tile(self) -> TileKind {
        match self {
            Self::Dirt => TileKind::Dirt,
            Self::Clay => TileKind::Clay,
            Self::Stone => TileKind::Stone,
            Self::HardRock => TileKind::HardRock,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt Block",
            Self::Clay => "Clay Block",
            Self::Stone => "Stone Block",
            Self::HardRock => "Hard Rock Block",
        }
    }
}

macro_rules! item_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
        )]
        pub struct $name(pub u16);
    };
}

item_id!(
    /// A placeable base module or building kit (WP-BASE defines the table).
    ModuleId
);
item_id!(
    /// A rig weapon (WP-WEAPONS defines the table).
    WeaponId
);
item_id!(
    /// An ammunition type (WP-WEAPONS defines the table).
    AmmoId
);
item_id!(
    /// A throwable or placeable explosive (WP-WEAPONS explosives define the table).
    ExplosiveId
);
item_id!(
    /// A cosmetic unlock (WP-RIG defines the table).
    CosmeticId
);
item_id!(
    /// A quest item (the quest engine / WP-STORY defines the table).
    QuestItemId
);

/// The quest item id of a sky temple's keystone (Wave 4 T21; ids 210-219 are the sky
/// temples').
pub const TEMPLE_KEYSTONE_ID: u16 = 210;
/// A sky temple's keystone: prised off its plinth by a pilot, it opens the temple's keystone
/// gate (`world/sky_temples.rs`).
pub const TEMPLE_KEYSTONE: ItemKind = ItemKind::Quest(QuestItemId(TEMPLE_KEYSTONE_ID));
/// The tomb key's quest item id (WP-TOMBS, [`crate::tomb::TOMB_KEY`]).
pub const TOMB_KEY_ID: u16 = 0xC200;

/// Broad item category (UI grouping, stowage rules, drop presentation).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ItemCategory {
    Mineral,
    Artifact,
    Material,
    Block,
    Module,
    Weapon,
    Ammo,
    Explosive,
    Cosmetic,
    Quest,
}

/// Which container a kind is stowed in.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Stowage {
    /// The rig's cargo bay (capacity: [`crate::Player::cargo_capacity`]).
    CargoBay,
    /// The rig's block hopper (capacity: [`crate::Player::block_hopper_capacity`]).
    BlockHopper,
}

/// Any item a rig can carry, drop or trade.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ItemKind {
    Mineral(MineralKind),
    Artifact(ArtifactKind),
    Material(StrategicResourceKind),
    Block(BlockKind),
    Module(ModuleId),
    Weapon(WeaponId),
    Ammo(AmmoId),
    Explosive(ExplosiveId),
    Cosmetic(CosmeticId),
    Quest(QuestItemId),
    /// A stratum's layer resource (Wave 8, Deep Descent worlds): dug out of the stratum's rock
    /// now and then ([`crate::workshop::layer_yield`]) and worked into consumables and upgrade
    /// parts at the deep outposts' workshops ([`crate::workshop::RECIPES`]). Named by the
    /// stratum registry ([`crate::descent::StratumDef::layer_resource`]).
    Layer(crate::descent::Stratum),
}

impl ItemKind {
    #[must_use]
    pub const fn category(self) -> ItemCategory {
        match self {
            Self::Mineral(_) => ItemCategory::Mineral,
            Self::Artifact(_) => ItemCategory::Artifact,
            Self::Material(_) | Self::Layer(_) => ItemCategory::Material,
            Self::Block(_) => ItemCategory::Block,
            Self::Module(_) => ItemCategory::Module,
            Self::Weapon(_) => ItemCategory::Weapon,
            Self::Ammo(_) => ItemCategory::Ammo,
            Self::Explosive(_) => ItemCategory::Explosive,
            Self::Cosmetic(_) => ItemCategory::Cosmetic,
            Self::Quest(_) => ItemCategory::Quest,
        }
    }

    /// Which container this kind is stowed in.
    #[must_use]
    pub const fn stowage(self) -> Stowage {
        match self {
            Self::Block(_) => Stowage::BlockHopper,
            _ => Stowage::CargoBay,
        }
    }

    /// Capacity slots one unit takes in its container.
    #[must_use]
    pub const fn slot_cost(self) -> u32 {
        match self {
            Self::Mineral(_)
            | Self::Artifact(_)
            | Self::Block(_)
            | Self::Module(_)
            | Self::Weapon(_) => 1,
            Self::Material(_)
            | Self::Ammo(_)
            | Self::Explosive(_)
            | Self::Cosmetic(_)
            | Self::Quest(_)
            | Self::Layer(_) => 0,
        }
    }

    /// The item a broken tile yields, if any (ore, artifacts and plain blocks).
    #[must_use]
    pub const fn from_tile(kind: TileKind) -> Option<Self> {
        use crate::sky::SkyTile;
        match kind {
            TileKind::Ore(mineral) => Some(Self::Mineral(mineral)),
            TileKind::Artifact(artifact) => Some(Self::Artifact(artifact)),
            // Sky continent content (V9): the sky ores are strategic materials, eggs are
            // artifacts and a keystone is the temple's key.
            TileKind::Sky(SkyTile::AetherCrystal) => {
                Some(Self::Material(StrategicResourceKind::AetherCrystal))
            }
            TileKind::Sky(SkyTile::StormGlass) => {
                Some(Self::Material(StrategicResourceKind::StormGlass))
            }
            TileKind::Sky(SkyTile::HarpyEgg) => Some(Self::Artifact(ArtifactKind::HarpyEgg)),
            TileKind::Sky(SkyTile::TempleKeystone) => Some(TEMPLE_KEYSTONE),
            TileKind::Lunar(tile) => tile.item(),
            TileKind::Deep(tile) => tile.item(),
            TileKind::Hollow(tile) => tile.item(),
            TileKind::Warped(tile) => tile.item(),
            TileKind::City(tile) => tile.item(),
            TileKind::Core(tile) => tile.item(),
            TileKind::Wild(tile) => tile.item(),
            _ => match BlockKind::from_tile(kind) {
                Some(block) => Some(Self::Block(block)),
                None => None,
            },
        }
    }

    /// Base sale value of one unit (minerals and artifacts; everything else is 0 here).
    #[must_use]
    pub const fn base_value(self) -> u32 {
        match self {
            Self::Mineral(mineral) => mineral.value(),
            Self::Artifact(artifact) => artifact.value(),
            _ => 0,
        }
    }

    /// Display name for the kinds this crate knows; open-ended categories name their category.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mineral(mineral) => mineral.name(),
            Self::Artifact(artifact) => artifact.name(),
            Self::Material(material) => material.name(),
            Self::Block(block) => block.name(),
            Self::Module(_) => "Module",
            Self::Weapon(_) => "Weapon",
            Self::Ammo(_) => "Ammo",
            Self::Explosive(id) => match crate::explosive::ExplosiveKind::from_item_id(id) {
                Some(kind) => kind.name(),
                None => "Explosive",
            },
            Self::Cosmetic(_) => "Cosmetic",
            Self::Quest(QuestItemId(TEMPLE_KEYSTONE_ID)) => "Temple Keystone",
            Self::Quest(QuestItemId(TOMB_KEY_ID)) => "Tomb Key",
            Self::Quest(_) => "Quest Item",
            Self::Layer(stratum) => stratum.def().layer_resource.name,
        }
    }

    #[must_use]
    pub const fn as_mineral(self) -> Option<MineralKind> {
        match self {
            Self::Mineral(mineral) => Some(mineral),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_artifact(self) -> Option<ArtifactKind> {
        match self {
            Self::Artifact(artifact) => Some(artifact),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_material(self) -> Option<StrategicResourceKind> {
        match self {
            Self::Material(material) => Some(material),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_block(self) -> Option<BlockKind> {
        match self {
            Self::Block(block) => Some(block),
            _ => None,
        }
    }
}

impl From<MineralKind> for ItemKind {
    fn from(mineral: MineralKind) -> Self {
        Self::Mineral(mineral)
    }
}

impl From<ArtifactKind> for ItemKind {
    fn from(artifact: ArtifactKind) -> Self {
        Self::Artifact(artifact)
    }
}

impl From<StrategicResourceKind> for ItemKind {
    fn from(material: StrategicResourceKind) -> Self {
        Self::Material(material)
    }
}

impl From<BlockKind> for ItemKind {
    fn from(block: BlockKind) -> Self {
        Self::Block(block)
    }
}

/// A count of one item kind (the serialized form of a container entry, and a drop's payload).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ItemStack {
    pub kind: ItemKind,
    pub count: u32,
}

impl ItemStack {
    #[must_use]
    pub const fn new(kind: ItemKind, count: u32) -> Self {
        Self { kind, count }
    }
}

/// A count per [`ItemKind`], iterated in kind order. Zero counts are never stored.
///
/// Capacity is not part of the container: the owner checks it against its own stat (see
/// [`Self::slots_used`] and [`Self::accepts`]), so a capacity upgrade never rewrites contents.
/// Serialized as a list of [`ItemStack`]s (item kinds are not string map keys in JSON).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(from = "Vec<ItemStack>", into = "Vec<ItemStack>")]
pub struct ItemContainer {
    items: BTreeMap<ItemKind, u32>,
}

impl From<Vec<ItemStack>> for ItemContainer {
    fn from(stacks: Vec<ItemStack>) -> Self {
        let mut container = Self::new();
        for stack in stacks {
            container.add(stack.kind, stack.count);
        }
        container
    }
}

impl From<ItemContainer> for Vec<ItemStack> {
    fn from(container: ItemContainer) -> Self {
        container.stacks().collect()
    }
}

impl FromIterator<(ItemKind, u32)> for ItemContainer {
    fn from_iter<T: IntoIterator<Item = (ItemKind, u32)>>(iter: T) -> Self {
        let mut container = Self::new();
        for (kind, count) in iter {
            container.add(kind, count);
        }
        container
    }
}

#[allow(
    clippy::missing_const_for_fn,
    reason = "BTreeMap methods are not const"
)]
impl ItemContainer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: BTreeMap::new(),
        }
    }

    /// Builds a cargo bay from the pre-item-model maps (save migration).
    #[must_use]
    pub fn from_legacy_cargo(
        minerals: &BTreeMap<MineralKind, u32>,
        artifacts: &BTreeMap<ArtifactKind, u32>,
        materials: &BTreeMap<StrategicResourceKind, u32>,
    ) -> Self {
        let mut container = Self::new();
        for (mineral, count) in minerals {
            container.add(ItemKind::Mineral(*mineral), *count);
        }
        for (artifact, count) in artifacts {
            container.add(ItemKind::Artifact(*artifact), *count);
        }
        for (material, count) in materials {
            container.add(ItemKind::Material(*material), *count);
        }
        container
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of distinct kinds held.
    #[must_use]
    pub fn kinds(&self) -> usize {
        self.items.len()
    }

    /// Units of `kind` held.
    #[must_use]
    pub fn count(&self, kind: impl Into<ItemKind>) -> u32 {
        self.items.get(&kind.into()).copied().unwrap_or(0)
    }

    /// The count of `kind`, if held (map-style lookup).
    #[must_use]
    pub fn get(&self, kind: impl Into<ItemKind>) -> Option<&u32> {
        self.items.get(&kind.into())
    }

    /// Whether any `kind` is held.
    #[must_use]
    pub fn contains(&self, kind: impl Into<ItemKind>) -> bool {
        self.items.contains_key(&kind.into())
    }

    /// Sets the count of `kind` (0 removes it), returning the previous count if any.
    pub fn insert(&mut self, kind: impl Into<ItemKind>, count: u32) -> Option<u32> {
        let kind = kind.into();
        if count == 0 {
            self.items.remove(&kind)
        } else {
            self.items.insert(kind, count)
        }
    }

    /// Units of every kind held.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.items
            .values()
            .fold(0, |sum, count| sum.saturating_add(*count))
    }

    /// Capacity slots the contents take ([`ItemKind::slot_cost`] per unit).
    #[must_use]
    pub fn slots_used(&self) -> u32 {
        self.items.iter().fold(0, |sum, (kind, count)| {
            sum.saturating_add(kind.slot_cost().saturating_mul(*count))
        })
    }

    /// How many of `count` units of `kind` fit under `capacity` slots (all of them for a
    /// zero-slot kind).
    #[must_use]
    pub fn accepts(&self, kind: ItemKind, count: u32, capacity: u32) -> u32 {
        let cost = kind.slot_cost();
        if cost == 0 {
            return count;
        }
        let free = capacity.saturating_sub(self.slots_used());
        count.min(free / cost)
    }

    /// Adds `count` units of `kind` (no capacity check).
    pub fn add(&mut self, kind: impl Into<ItemKind>, count: u32) {
        if count == 0 {
            return;
        }
        let entry = self.items.entry(kind.into()).or_default();
        *entry = entry.saturating_add(count);
    }

    /// Removes up to `count` units of `kind`; returns how many were removed.
    pub fn remove(&mut self, kind: impl Into<ItemKind>, count: u32) -> u32 {
        let kind = kind.into();
        let Some(held) = self.items.get_mut(&kind) else {
            return 0;
        };
        let removed = (*held).min(count);
        *held -= removed;
        if *held == 0 {
            self.items.remove(&kind);
        }
        removed
    }

    /// Removes every unit of `kind`; returns how many there were.
    pub fn take_all(&mut self, kind: impl Into<ItemKind>) -> u32 {
        self.items.remove(&kind.into()).unwrap_or(0)
    }

    /// Sets the count of `kind` (0 removes it).
    pub fn set(&mut self, kind: impl Into<ItemKind>, count: u32) {
        let kind = kind.into();
        if count == 0 {
            self.items.remove(&kind);
        } else {
            self.items.insert(kind, count);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Removes every kind `keep` rejects.
    pub fn retain(&mut self, mut keep: impl FnMut(ItemKind, u32) -> bool) {
        self.items.retain(|kind, count| keep(*kind, *count));
    }

    /// Rewrites every count in kind order (`update` returns the new count; 0 removes).
    pub fn update_counts(&mut self, mut update: impl FnMut(ItemKind, u32) -> u32) {
        self.items.retain(|kind, count| {
            *count = update(*kind, *count);
            *count > 0
        });
    }

    /// Removes every mineral and artifact (what a sale empties), keeping everything else.
    pub fn clear_ore_and_artifacts(&mut self) {
        self.items.retain(|kind, _| {
            !matches!(
                kind.category(),
                ItemCategory::Mineral | ItemCategory::Artifact
            )
        });
    }

    /// Removes every kind in `category`, returning them.
    #[must_use]
    pub fn take_category(&mut self, category: ItemCategory) -> Self {
        let mut taken = Self::new();
        self.items.retain(|kind, count| {
            if kind.category() == category {
                taken.items.insert(*kind, *count);
                false
            } else {
                true
            }
        });
        taken
    }

    /// Adds everything in `other`.
    pub fn merge(&mut self, other: &Self) {
        for (kind, count) in &other.items {
            self.add(*kind, *count);
        }
    }

    /// `(kind, count)` in kind order.
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (ItemKind, u32)> + '_ {
        self.items.iter().map(|(kind, count)| (*kind, *count))
    }

    /// Every entry as an [`ItemStack`], in kind order.
    pub fn stacks(&self) -> impl Iterator<Item = ItemStack> + '_ {
        self.iter().map(|(kind, count)| ItemStack { kind, count })
    }

    /// Entries of one category, in kind order.
    #[must_use]
    pub fn in_category(
        &self,
        category: ItemCategory,
    ) -> impl DoubleEndedIterator<Item = (ItemKind, u32)> + '_ {
        self.iter()
            .filter(move |(kind, _)| kind.category() == category)
    }

    /// Minerals held, in [`MineralKind`] order.
    #[must_use]
    pub fn minerals(&self) -> impl DoubleEndedIterator<Item = (MineralKind, u32)> + '_ {
        self.iter()
            .filter_map(|(kind, count)| kind.as_mineral().map(|mineral| (mineral, count)))
    }

    /// Artifacts held, in [`ArtifactKind`] order.
    #[must_use]
    pub fn artifacts(&self) -> impl DoubleEndedIterator<Item = (ArtifactKind, u32)> + '_ {
        self.iter()
            .filter_map(|(kind, count)| kind.as_artifact().map(|artifact| (artifact, count)))
    }

    /// Strategic materials held, in [`StrategicResourceKind`] order.
    #[must_use]
    pub fn materials(&self) -> impl DoubleEndedIterator<Item = (StrategicResourceKind, u32)> + '_ {
        self.iter()
            .filter_map(|(kind, count)| kind.as_material().map(|material| (material, count)))
    }

    /// Building blocks held, in [`BlockKind`] order.
    #[must_use]
    pub fn blocks(&self) -> impl DoubleEndedIterator<Item = (BlockKind, u32)> + '_ {
        self.iter()
            .filter_map(|(kind, count)| kind.as_block().map(|block| (block, count)))
    }

    #[must_use]
    pub fn mineral(&self, mineral: MineralKind) -> u32 {
        self.count(mineral)
    }

    #[must_use]
    pub fn artifact(&self, artifact: ArtifactKind) -> u32 {
        self.count(artifact)
    }

    #[must_use]
    pub fn material(&self, material: StrategicResourceKind) -> u32 {
        self.count(material)
    }

    /// Units of every mineral held.
    #[must_use]
    pub fn mineral_total(&self) -> u32 {
        self.minerals().map(|(_, count)| count).sum()
    }

    /// Units of every artifact held.
    #[must_use]
    pub fn artifact_total(&self) -> u32 {
        self.artifacts().map(|(_, count)| count).sum()
    }

    /// Whether any mineral is held.
    #[must_use]
    pub fn has_minerals(&self) -> bool {
        self.minerals().next().is_some()
    }

    /// Whether any artifact is held.
    #[must_use]
    pub fn has_artifacts(&self) -> bool {
        self.artifacts().next().is_some()
    }

    /// The minerals as a map (legacy-shaped views for callers that want one).
    #[must_use]
    pub fn mineral_map(&self) -> BTreeMap<MineralKind, u32> {
        self.minerals().collect()
    }

    /// The artifacts as a map.
    #[must_use]
    pub fn artifact_map(&self) -> BTreeMap<ArtifactKind, u32> {
        self.artifacts().collect()
    }

    /// The materials as a map.
    #[must_use]
    pub fn material_map(&self) -> BTreeMap<StrategicResourceKind, u32> {
        self.materials().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containers_keep_kind_order_and_drop_zero_counts() {
        let mut bay = ItemContainer::new();
        bay.add(MineralKind::Gold, 2);
        bay.add(ArtifactKind::Fossil, 1);
        bay.add(MineralKind::Copper, 3);
        bay.add(StrategicResourceKind::CoreShard, 4);
        assert_eq!(
            bay.minerals().collect::<Vec<_>>(),
            vec![(MineralKind::Copper, 3), (MineralKind::Gold, 2)]
        );
        assert_eq!(bay.slots_used(), 6, "materials take no slots");
        assert_eq!(bay.total(), 10);
        assert_eq!(bay.remove(MineralKind::Gold, 5), 2);
        assert_eq!(bay.count(MineralKind::Gold), 0);
        assert!(
            !bay.minerals()
                .any(|(mineral, _)| mineral == MineralKind::Gold)
        );
        bay.update_counts(|_, count| count.saturating_sub(1));
        assert_eq!(bay.count(ArtifactKind::Fossil), 0);
        assert_eq!(bay.kinds(), 2);
    }

    #[test]
    fn capacity_checks_follow_slot_costs() {
        let mut bay = ItemContainer::new();
        bay.add(MineralKind::Iron, 10);
        assert_eq!(bay.accepts(ItemKind::Mineral(MineralKind::Iron), 5, 12), 2);
        assert_eq!(
            bay.accepts(ItemKind::Material(StrategicResourceKind::VoidGlass), 5, 12),
            5
        );
        assert_eq!(
            ItemKind::Block(BlockKind::Dirt).stowage(),
            Stowage::BlockHopper
        );
        assert_eq!(
            ItemKind::from_tile(TileKind::Ore(MineralKind::Ruby)),
            Some(ItemKind::Mineral(MineralKind::Ruby))
        );
        assert_eq!(
            ItemKind::from_tile(TileKind::Clay),
            Some(ItemKind::Block(BlockKind::Clay))
        );
        assert_eq!(ItemKind::from_tile(TileKind::Lava), None);
    }

    #[test]
    fn containers_round_trip_through_json_and_postcard() {
        let mut bay = ItemContainer::new();
        bay.add(MineralKind::Mythril, 1);
        bay.add(BlockKind::Stone, 40);
        bay.add(ItemKind::Weapon(WeaponId(3)), 1);
        let json = serde_json::to_string(&bay).expect("json");
        assert_eq!(
            serde_json::from_str::<ItemContainer>(&json).expect("back"),
            bay
        );
        let bytes = postcard::to_allocvec(&bay).expect("postcard");
        assert_eq!(
            postcard::from_bytes::<ItemContainer>(&bytes).expect("back"),
            bay
        );
        // Duplicate and zero entries normalize on load.
        let messy = r#"[{"kind":{"Mineral":"Gold"},"count":1},{"kind":{"Mineral":"Gold"},"count":2},{"kind":{"Mineral":"Iron"},"count":0}]"#;
        let loaded: ItemContainer = serde_json::from_str(messy).expect("loads");
        assert_eq!(loaded.count(MineralKind::Gold), 3);
        assert_eq!(loaded.kinds(), 1);
    }
}
