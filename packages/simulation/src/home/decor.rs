//! Interior decorating: furniture on each room's grid, wallpaper and floors, trophy displays.
//!
//! Every room has a grid of [`ROOM_COLUMNS`] x [`ROOM_ROWS`] cells (cell index =
//! `row * ROOM_COLUMNS + column`): row 0 stands on the floor, row 1 hangs on the wall, row 2
//! hangs from the ceiling. A [`Furniture`] piece takes [`Furniture::width`] cells of its own
//! [`DecorRow`]; a rug lies under floor furniture. Some pieces do something
//! ([`Furniture::service`]): a bed sleeps and sets the home point, a stove cooks, a chest
//! stores, a trophy case shows off an artifact or rare ore, a workbench crafts furniture.
//!
//! Furniture comes from vendors (the land office catalogue, [`Furniture::catalogue_price`]),
//! from crafting at a workbench ([`Furniture::craft_cost`]), as a housewarming gift from the
//! contractor, as rewards of construction events, and from anything that calls
//! `WorldState::grant_furniture` (drops and quests).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::item::{BlockKind, ItemKind};

/// Grid columns across a room.
pub const ROOM_COLUMNS: u8 = 8;
/// Grid rows up a room (floor, wall, ceiling).
pub const ROOM_ROWS: u8 = 3;
/// Cells in a room's grid.
pub const ROOM_CELLS: u8 = ROOM_COLUMNS * ROOM_ROWS;

/// Which band of the room's grid a piece belongs to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DecorRow {
    Floor,
    Wall,
    Ceiling,
}

impl DecorRow {
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Floor => 0,
            Self::Wall => 1,
            Self::Ceiling => 2,
        }
    }

    #[must_use]
    pub const fn of_cell(cell: u8) -> Self {
        match cell / ROOM_COLUMNS {
            0 => Self::Floor,
            1 => Self::Wall,
            _ => Self::Ceiling,
        }
    }
}

/// What a piece of furniture does when used.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DecorService {
    /// Sleep through the night, and make this home your home point.
    Sleep,
    /// Cook a hot meal: the pilot is patched up and breathes easy.
    Cook,
    /// Store ore, relics and materials from the cargo bay.
    Store,
    /// Display an artifact or rare ore.
    Display,
    /// Craft furniture from building blocks.
    Craft,
    /// Look at the sky (the observatory's telescope: a line about the Moon).
    Stargaze,
}

/// A piece of furniture or decoration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Furniture {
    Bed,
    Sofa,
    Table,
    Chair,
    Armchair,
    Bookshelf,
    Wardrobe,
    Stove,
    Workbench,
    Chest,
    TrophyCase,
    Plant,
    Planter,
    Rug,
    Fireplace,
    Telescope,
    Piano,
    Painting,
    Shelf,
    WallLamp,
    Clock,
    Mirror,
    Banner,
    MountedPick,
    Chandelier,
    Lantern,
    HangingPlant,
    StarMobile,
    // Wave 6 content: culture pieces people sell, the sky and lunar catalogues, tomb trophies
    // and story rewards (appended: saved stocks and rooms keep their pieces).
    /// Frontier folk's signature piece.
    RockingChair,
    /// Highland folk's signature piece.
    Loom,
    /// Mesa folk's signature piece: a beehive oven that cooks.
    ClayOven,
    /// Riverside folk's signature piece.
    FishTank,
    /// Scrapper folk's signature piece.
    JunkSculpture,
    /// Deepfolk's signature piece: a lamp of glowing cave caps.
    GlowcapLamp,
    /// Aeronauts' signature piece.
    ModelAirship,
    /// Cloudborn folk's signature piece.
    WindChime,
    /// The lunar colonists' globe of the Moon.
    MoonGlobe,
    /// The lunar colonists' grow rack (it lights the room).
    HydroponicRack,
    /// Sky land offices: a brass barometer.
    Barometer,
    /// Sky land offices: a hammock strung for the wind (sleeps like a bed).
    CloudHammock,
    /// A tomb guardian's mask (a trophy for every guardian brought down).
    GuardianMask,
    /// A plinth that shows off a relic (a trophy for the first tomb seen through to the end).
    RelicPlinth,
    /// A hand-inked chart of the night sky (stories).
    StarChart,
    /// A potted moonflower that glows after dark (stories).
    Moonflower,
    /// A wind-up gramophone (stories, the land office).
    Gramophone,
}

impl Furniture {
    pub const ALL: [Self; 45] = [
        Self::Bed,
        Self::Sofa,
        Self::Table,
        Self::Chair,
        Self::Armchair,
        Self::Bookshelf,
        Self::Wardrobe,
        Self::Stove,
        Self::Workbench,
        Self::Chest,
        Self::TrophyCase,
        Self::Plant,
        Self::Planter,
        Self::Rug,
        Self::Fireplace,
        Self::Telescope,
        Self::Piano,
        Self::Painting,
        Self::Shelf,
        Self::WallLamp,
        Self::Clock,
        Self::Mirror,
        Self::Banner,
        Self::MountedPick,
        Self::Chandelier,
        Self::Lantern,
        Self::HangingPlant,
        Self::StarMobile,
        Self::RockingChair,
        Self::Loom,
        Self::ClayOven,
        Self::FishTank,
        Self::JunkSculpture,
        Self::GlowcapLamp,
        Self::ModelAirship,
        Self::WindChime,
        Self::MoonGlobe,
        Self::HydroponicRack,
        Self::Barometer,
        Self::CloudHammock,
        Self::GuardianMask,
        Self::RelicPlinth,
        Self::StarChart,
        Self::Moonflower,
        Self::Gramophone,
    ];

    /// The land office's catalogue, in listing order.
    pub const CATALOGUE: [Self; 22] = [
        Self::Bed,
        Self::Sofa,
        Self::Table,
        Self::Chair,
        Self::Armchair,
        Self::Bookshelf,
        Self::Wardrobe,
        Self::Stove,
        Self::Chest,
        Self::TrophyCase,
        Self::Plant,
        Self::Rug,
        Self::Fireplace,
        Self::Painting,
        Self::Shelf,
        Self::WallLamp,
        Self::Clock,
        Self::Mirror,
        Self::Chandelier,
        Self::Lantern,
        Self::Gramophone,
        Self::StarChart,
    ];

    /// What a sky town's land office sells on top of [`Self::CATALOGUE`] (Wave 6).
    pub const SKY_CATALOGUE: [Self; 2] = [Self::CloudHammock, Self::Barometer];

    /// Trophies won in tombs and crypts (never sold): a guardian's mask for every guardian
    /// brought down, a relic plinth for the first run seen through to the end.
    pub const TOMB_TROPHIES: [Self; 2] = [Self::GuardianMask, Self::RelicPlinth];

    /// Antique pieces found in cave caches (never sold at the land office), rarest last.
    pub const CACHE_FINDS: [Self; 6] = [
        Self::HangingPlant,
        Self::Banner,
        Self::MountedPick,
        Self::StarMobile,
        Self::Telescope,
        Self::Piano,
    ];

    /// What a workbench can make, in listing order.
    pub const CRAFTABLE: [Self; 10] = [
        Self::Chair,
        Self::Table,
        Self::Shelf,
        Self::Chest,
        Self::Bookshelf,
        Self::Planter,
        Self::Workbench,
        Self::MountedPick,
        Self::RockingChair,
        Self::JunkSculpture,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bed => "Bed",
            Self::Sofa => "Sofa",
            Self::Table => "Table",
            Self::Chair => "Chair",
            Self::Armchair => "Armchair",
            Self::Bookshelf => "Bookshelf",
            Self::Wardrobe => "Wardrobe",
            Self::Stove => "Kitchen stove",
            Self::Workbench => "Workbench",
            Self::Chest => "Storage chest",
            Self::TrophyCase => "Trophy case",
            Self::Plant => "Potted fern",
            Self::Planter => "Planter box",
            Self::Rug => "Woven rug",
            Self::Fireplace => "Fireplace",
            Self::Telescope => "Brass telescope",
            Self::Piano => "Upright piano",
            Self::Painting => "Landscape painting",
            Self::Shelf => "Wall shelf",
            Self::WallLamp => "Wall lamp",
            Self::Clock => "Wall clock",
            Self::Mirror => "Mirror",
            Self::Banner => "Crew banner",
            Self::MountedPick => "Mounted pickaxe",
            Self::Chandelier => "Chandelier",
            Self::Lantern => "Hanging lantern",
            Self::HangingPlant => "Hanging vine",
            Self::StarMobile => "Star mobile",
            Self::RockingChair => "Rocking chair",
            Self::Loom => "Highland loom",
            Self::ClayOven => "Clay oven",
            Self::FishTank => "Fish tank",
            Self::JunkSculpture => "Junk sculpture",
            Self::GlowcapLamp => "Glowcap lamp",
            Self::ModelAirship => "Model airship",
            Self::WindChime => "Wind chimes",
            Self::MoonGlobe => "Moon globe",
            Self::HydroponicRack => "Hydroponic rack",
            Self::Barometer => "Brass barometer",
            Self::CloudHammock => "Cloud hammock",
            Self::GuardianMask => "Guardian's mask",
            Self::RelicPlinth => "Relic plinth",
            Self::StarChart => "Star chart",
            Self::Moonflower => "Moonflower",
            Self::Gramophone => "Gramophone",
        }
    }

    #[must_use]
    pub const fn row(self) -> DecorRow {
        match self {
            Self::Painting
            | Self::Shelf
            | Self::WallLamp
            | Self::Clock
            | Self::Mirror
            | Self::Banner
            | Self::MountedPick
            | Self::Barometer
            | Self::GuardianMask
            | Self::StarChart => DecorRow::Wall,
            Self::Chandelier
            | Self::Lantern
            | Self::HangingPlant
            | Self::StarMobile
            | Self::GlowcapLamp
            | Self::ModelAirship
            | Self::WindChime => DecorRow::Ceiling,
            _ => DecorRow::Floor,
        }
    }

    /// Cells it takes along its row.
    #[must_use]
    pub const fn width(self) -> u8 {
        match self {
            Self::Bed
            | Self::Sofa
            | Self::Table
            | Self::Workbench
            | Self::Rug
            | Self::Fireplace
            | Self::Piano
            | Self::Banner
            | Self::Loom
            | Self::HydroponicRack
            | Self::CloudHammock => 2,
            _ => 1,
        }
    }

    /// Whether it lies on the floor under other floor pieces (rugs).
    #[must_use]
    pub const fn underlay(self) -> bool {
        matches!(self, Self::Rug)
    }

    /// Whether it gives light (rooms with lamps glow at night).
    #[must_use]
    pub const fn lights(self) -> bool {
        matches!(
            self,
            Self::WallLamp
                | Self::Chandelier
                | Self::Lantern
                | Self::Fireplace
                | Self::Stove
                | Self::ClayOven
                | Self::GlowcapLamp
                | Self::HydroponicRack
                | Self::Moonflower
        )
    }

    #[must_use]
    pub const fn service(self) -> Option<DecorService> {
        match self {
            Self::Bed | Self::CloudHammock => Some(DecorService::Sleep),
            Self::Stove | Self::Fireplace | Self::ClayOven => Some(DecorService::Cook),
            Self::Chest | Self::Wardrobe => Some(DecorService::Store),
            Self::TrophyCase | Self::RelicPlinth => Some(DecorService::Display),
            Self::Workbench => Some(DecorService::Craft),
            Self::Telescope => Some(DecorService::Stargaze),
            _ => None,
        }
    }

    /// Credits at the land office.
    #[must_use]
    pub const fn catalogue_price(self) -> u32 {
        match self {
            Self::Bed => 180,
            Self::Sofa => 160,
            Self::Table => 90,
            Self::Chair => 40,
            Self::Armchair => 110,
            Self::Bookshelf => 130,
            Self::Wardrobe => 140,
            Self::Stove => 220,
            Self::Workbench => 200,
            Self::Chest | Self::Painting => 120,
            Self::TrophyCase => 300,
            Self::Plant | Self::HangingPlant => 35,
            Self::Planter => 50,
            Self::Rug => 70,
            Self::Fireplace => 260,
            Self::Telescope => 900,
            Self::Piano => 750,
            Self::Shelf => 45,
            Self::WallLamp => 60,
            Self::Clock => 85,
            Self::Mirror => 95,
            Self::Banner => 80,
            Self::MountedPick => 150,
            Self::Chandelier => 240,
            Self::Lantern => 55,
            Self::StarMobile => 400,
            Self::RockingChair => 130,
            Self::Loom => 340,
            Self::ClayOven => 240,
            Self::FishTank => 260,
            Self::JunkSculpture => 150,
            Self::GlowcapLamp => 90,
            Self::ModelAirship => 420,
            Self::WindChime => 75,
            Self::MoonGlobe => 380,
            Self::HydroponicRack => 520,
            Self::Barometer => 140,
            Self::CloudHammock => 280,
            Self::GuardianMask => 1_200,
            Self::RelicPlinth => 900,
            Self::StarChart => 200,
            Self::Moonflower => 160,
            Self::Gramophone => 450,
        }
    }

    /// `(blocks, credits)` to craft it at a workbench, if it can be.
    #[must_use]
    pub const fn craft_cost(self) -> Option<(u32, u32)> {
        match self {
            Self::Chair => Some((4, 5)),
            Self::Table => Some((8, 10)),
            Self::Shelf => Some((3, 5)),
            Self::Chest => Some((10, 15)),
            Self::Bookshelf => Some((12, 20)),
            Self::Planter => Some((6, 10)),
            Self::Workbench => Some((16, 30)),
            Self::MountedPick => Some((6, 40)),
            Self::RockingChair => Some((6, 20)),
            Self::JunkSculpture => Some((10, 25)),
            _ => None,
        }
    }

    /// Storage slots a chest-like piece holds.
    #[must_use]
    pub const fn storage_slots(self) -> u32 {
        match self {
            Self::Chest => 40,
            Self::Wardrobe => 20,
            _ => 0,
        }
    }
}

/// A room's wallpaper.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Wallpaper {
    #[default]
    Plaster,
    Timber,
    Floral,
    Stripes,
    Brick,
    Midnight,
}

impl Wallpaper {
    pub const ALL: [Self; 6] = [
        Self::Plaster,
        Self::Timber,
        Self::Floral,
        Self::Stripes,
        Self::Brick,
        Self::Midnight,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plaster => "Plaster",
            Self::Timber => "Timber panels",
            Self::Floral => "Floral paper",
            Self::Stripes => "Striped paper",
            Self::Brick => "Bare brick",
            Self::Midnight => "Midnight blue",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Plaster => Self::Timber,
            Self::Timber => Self::Floral,
            Self::Floral => Self::Stripes,
            Self::Stripes => Self::Brick,
            Self::Brick => Self::Midnight,
            Self::Midnight => Self::Plaster,
        }
    }
}

/// A room's floor.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Flooring {
    #[default]
    Planks,
    Stone,
    Tiles,
    Carpet,
    Parquet,
}

impl Flooring {
    pub const ALL: [Self; 5] = [
        Self::Planks,
        Self::Stone,
        Self::Tiles,
        Self::Carpet,
        Self::Parquet,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Planks => "Pine planks",
            Self::Stone => "Flagstones",
            Self::Tiles => "Checked tiles",
            Self::Carpet => "Carpet",
            Self::Parquet => "Parquet",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Planks => Self::Stone,
            Self::Stone => Self::Tiles,
            Self::Tiles => Self::Carpet,
            Self::Carpet => Self::Parquet,
            Self::Parquet => Self::Planks,
        }
    }
}

/// Credits to repaper a room or relay its floor.
pub const RESTYLE_PRICE: u32 = 25;

/// One placed piece.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlacedDecor {
    pub furniture: Furniture,
    /// What a trophy case shows (an artifact or a rare ore).
    #[serde(default)]
    pub display: Option<ItemKind>,
}

/// One room's look: wallpaper, floor and what stands where.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoomDecor {
    #[serde(default)]
    pub wallpaper: Wallpaper,
    #[serde(default)]
    pub flooring: Flooring,
    /// Pieces by their leftmost cell (saved as a list).
    #[serde(default, with = "cell_list")]
    pub pieces: BTreeMap<u8, PlacedDecor>,
    /// Rugs by their leftmost floor cell (they lie under floor furniture).
    #[serde(default, with = "cell_list")]
    pub rugs: BTreeMap<u8, PlacedDecor>,
}

impl RoomDecor {
    /// The piece covering `cell`, with its leftmost cell.
    #[must_use]
    pub fn piece_at(&self, cell: u8) -> Option<(u8, PlacedDecor)> {
        Self::covering(&self.pieces, cell)
    }

    /// The rug covering floor `cell`.
    #[must_use]
    pub fn rug_at(&self, cell: u8) -> Option<(u8, PlacedDecor)> {
        Self::covering(&self.rugs, cell)
    }

    fn covering(map: &BTreeMap<u8, PlacedDecor>, cell: u8) -> Option<(u8, PlacedDecor)> {
        map.range(..=cell).next_back().and_then(|(start, piece)| {
            let same_row = start / ROOM_COLUMNS == cell / ROOM_COLUMNS;
            (same_row && cell < start + piece.furniture.width()).then_some((*start, *piece))
        })
    }

    /// Whether `furniture` fits with its leftmost cell at `cell`.
    #[must_use]
    pub fn fits(&self, furniture: Furniture, cell: u8) -> bool {
        if cell >= ROOM_CELLS || DecorRow::of_cell(cell) != furniture.row() {
            return false;
        }
        let column = cell % ROOM_COLUMNS;
        if column + furniture.width() > ROOM_COLUMNS {
            return false;
        }
        (cell..cell + furniture.width()).all(|at| {
            if furniture.underlay() {
                self.rug_at(at).is_none()
            } else {
                self.piece_at(at).is_none()
            }
        })
    }

    /// Every piece (rugs first), with its leftmost cell.
    pub fn all(&self) -> impl Iterator<Item = (u8, PlacedDecor)> + '_ {
        self.rugs
            .iter()
            .chain(self.pieces.iter())
            .map(|(cell, piece)| (*cell, *piece))
    }

    /// Whether any light source stands in the room.
    #[must_use]
    pub fn lit(&self) -> bool {
        self.pieces.values().any(|piece| piece.furniture.lights())
    }

    /// The services the room's pieces offer, with each piece's cell.
    pub fn services(&self) -> impl Iterator<Item = (u8, Furniture, DecorService)> + '_ {
        self.pieces.iter().filter_map(|(cell, piece)| {
            piece
                .furniture
                .service()
                .map(|service| (*cell, piece.furniture, service))
        })
    }
}

/// The furniture a player owns but has not placed.
pub type FurnitureStock = BTreeMap<Furniture, u32>;

/// Whether `item` may go in a trophy case: any artifact, or one of the rare ores.
#[must_use]
pub const fn displayable(item: ItemKind) -> bool {
    use crate::terrain::MineralKind as M;
    match item {
        ItemKind::Artifact(_) => true,
        ItemKind::Mineral(mineral) => matches!(
            mineral,
            M::Emerald | M::Ruby | M::Diamond | M::Platinum | M::Uranium | M::Mythril
        ),
        _ => false,
    }
}

/// The cheapest blocks first, to pay a crafting cost of `blocks` from a hopper.
#[must_use]
pub fn blocks_to_take(
    hopper: &crate::item::ItemContainer,
    mut blocks: u32,
) -> Option<Vec<(BlockKind, u32)>> {
    let mut taken = Vec::new();
    for (kind, have) in hopper.blocks() {
        if blocks == 0 {
            break;
        }
        let take = have.min(blocks);
        if take > 0 {
            taken.push((kind, take));
            blocks -= take;
        }
    }
    (blocks == 0).then_some(taken)
}

/// (De)serializes a cell map as a list of `(cell, piece)` entries (JSON keys must be strings).
mod cell_list {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::PlacedDecor;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<u8, PlacedDecor>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let entries: Vec<(&u8, &PlacedDecor)> = map.iter().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<u8, PlacedDecor>, D::Error> {
        let entries = Vec::<(u8, PlacedDecor)>::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}

/// One in this many treasure caches holds an antique piece of furniture (relic caches always
/// do).
pub const TREASURE_CACHE_FURNITURE_ODDS: u64 = 3;
const SALT_CACHE_FURNITURE: u64 = 0x4655_524e_4954_5552;

/// The antique piece of furniture a cave cache at `(x, y)` holds, if any.
///
/// WP-HOME furniture found as drops: deterministic in the seed and the tile; relic caches
/// always hold one and favour the rarer pieces.
#[must_use]
pub const fn cache_find(seed: u64, x: i32, y: i32, relic: bool) -> Option<Furniture> {
    let roll = crate::worldgen::noise::hash2(seed, SALT_CACHE_FURNITURE, x, y);
    if !relic && !roll.is_multiple_of(TREASURE_CACHE_FURNITURE_ODDS) {
        return None;
    }
    let finds = Furniture::CACHE_FINDS.len() as u64;
    // Relic caches pick from the upper half of the list, treasure caches from all of it.
    let index = if relic {
        finds / 2 + (roll >> 8) % (finds - finds / 2)
    } else {
        (roll >> 8) % finds
    };
    #[allow(
        clippy::cast_possible_truncation,
        reason = "an index below CACHE_FINDS.len()"
    )]
    let index = index as usize;
    Some(Furniture::CACHE_FINDS[index])
}

#[cfg(test)]
mod tests {
    use super::{DecorRow, Furniture, PlacedDecor, ROOM_COLUMNS, RoomDecor, displayable};
    use crate::{item::ItemKind, terrain::MineralKind};

    #[test]
    fn pieces_fit_their_row_and_never_overlap() {
        let mut room = RoomDecor::default();
        assert!(room.fits(Furniture::Bed, 0));
        assert!(!room.fits(Furniture::Bed, ROOM_COLUMNS - 1), "off the edge");
        assert!(
            !room.fits(Furniture::Painting, 0),
            "wall pieces hang on the wall row"
        );
        room.pieces.insert(
            0,
            PlacedDecor {
                furniture: Furniture::Bed,
                display: None,
            },
        );
        assert!(!room.fits(Furniture::Chair, 1), "the bed covers cell 1");
        assert!(room.fits(Furniture::Chair, 2));
        assert!(room.fits(Furniture::Rug, 0), "a rug lies under the bed");
        assert!(room.fits(Furniture::Painting, ROOM_COLUMNS));
        assert_eq!(room.piece_at(1).map(|(cell, _)| cell), Some(0));
        assert_eq!(DecorRow::of_cell(ROOM_COLUMNS * 2), DecorRow::Ceiling);
    }

    #[test]
    fn trophies_take_relics_and_rare_ore_only() {
        assert!(displayable(ItemKind::Mineral(MineralKind::Diamond)));
        assert!(!displayable(ItemKind::Mineral(MineralKind::Copper)));
        assert!(displayable(ItemKind::Artifact(
            crate::terrain::ArtifactKind::Fossil
        )));
    }
}
