//! Base building (expansion plan WP-BASE): claims, building pieces and functional modules.
//!
//! A player stakes a **claim** by placing a claim beacon anywhere on the surface or underground
//! (outside the town). Inside the claim's square ([`CLAIM_RADIUS`] tiles each way from the
//! beacon) the crew can build:
//!
//! - **Solid pieces** ([`BuiltTile`]: block walls, metal plating, glass, plank floors) are terrain
//!   tiles ([`crate::terrain::TileKind::Built`]), so rigs collide with them and they replicate and
//!   save through the terrain op log and diffs like any dug tile. No drill or blast breaks them
//!   (bases are safe from bombs, cave-ins and, later, enemies); they come down only by
//!   deconstruction.
//! - **Fixtures** ([`TileFixtures`]): a back wall layer (indoor rooms) and a front layer of
//!   passable fixtures: doors, lift ladders, lights and decorations ([`Fixture`]).
//! - **Modules** ([`ModuleKind`]): multi-tile structures with services: the claim beacon itself,
//!   storage chests, a bed (home point), a generator (power), the **underground fuel station**,
//!   a repair bay, a cargo uplink, a refinery (ore refined at the base is worth more), a
//!   workshop (crafts explosives from building blocks) and the surface homestead (a house).
//!
//! Every build action is a [`BuildCommand`] inside [`crate::PlayerCommand::Build`], validated by
//! the host inside the fixed tick (`WorldState::check_build` gives the same verdict to the
//! build-mode preview). Claims and their modules and fixtures live in [`Bases`], one shared
//! on-change replication section (`SectionKey::Bases`) saved with the world.
//!
//! # Power and supply
//!
//! Every claim beacon carries a small power cell ([`BEACON_POWER`]); generators add more,
//! geothermal ones the deeper they sit ([`ModuleKind::power_supply`]). Modules that draw power
//! ([`ModuleKind::power_draw`]) are powered in placement order while the claim's supply lasts. A
//! powered fuel station synthesizes fuel into its reserve; refuelling from the reserve is free,
//! and an empty station can always order a supply drop for credits. Repair bays and cargo
//! uplinks only work while powered.
//!
//! # Permissions
//!
//! Each claim has an owner and a [`ClaimAccess`]: the crew builds together by default; the owner
//! can restrict building (services stay open) or close the claim entirely.
//!
//! # API for other tracks
//!
//! - Enemies (WP-ENEMIES): [`Bases::is_safe_zone`] says where nothing hostile may spawn (every
//!   claim plus [`CLAIM_SAFE_MARGIN`]).
//! - Drops (WP-DROPS): building blocks come from [`crate::Player::block_hopper`]
//!   ([`crate::item::ItemKind::Block`]); deconstruction returns blocks there and spills what does
//!   not fit as item drops (`WorldState::spawn_item_drop`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    explosive::ExplosiveKind,
    ids::PlayerId,
    item::{BlockKind, ItemContainer},
    terrain::TilePosition,
    worldgen::Band,
};

/// Tiles a claim reaches each way from its beacon (a claim is a 21x21-tile square).
pub const CLAIM_RADIUS: i32 = 10;
/// Extra tiles around every claim where nothing hostile spawns ([`Bases::is_safe_zone`]).
pub const CLAIM_SAFE_MARGIN: i32 = 6;
/// How far from its rig (tiles, each axis) a player can build or deconstruct.
pub const BUILD_REACH: i32 = 7;
/// Power every claim beacon supplies on its own.
pub const BEACON_POWER: u32 = 1;
/// Fuel a fuel station's reserve holds.
pub const FUEL_STATION_CAPACITY: u32 = 500;
/// Fuel a freshly built fuel station starts with.
pub const FUEL_STATION_STARTING_FUEL: u32 = 150;
/// How often powered fuel stations synthesize fuel.
pub const FUEL_SYNTH_PERIOD_MILLIS: u64 = 3_000;
/// Fuel a powered station synthesizes each period.
pub const FUEL_SYNTH_PER_PERIOD: u32 = 6;
/// Credits per unit of fuel in a supply drop.
pub const SUPPLY_UNIT_PRICE: u32 = 2;
/// Flat fee of a supply drop.
pub const SUPPLY_BASE_FEE: u32 = 40;
/// Share of the sale value a cargo uplink keeps as its fee.
pub const UPLINK_FEE_PERCENT: u32 = 15;
/// Extra value (percent of the market price) ore gains in a refinery.
pub const REFINERY_BONUS_PERCENT: u32 = 40;
/// Ore units a powered refinery refines each [`FUEL_SYNTH_PERIOD_MILLIS`] (it shares the fuel
/// stations' schedule).
pub const REFINE_PER_PERIOD: u32 = 1;
/// Share of a module's credit price refunded on deconstruction.
pub const MODULE_REFUND_PERCENT: u32 = 75;
/// Credits per building block a piece costs when the hopper has none of the right kind.
#[must_use]
pub const fn block_credit_price(block: BlockKind) -> u32 {
    match block {
        BlockKind::Dirt => 2,
        BlockKind::Clay => 3,
        BlockKind::Stone => 4,
        BlockKind::HardRock => 6,
    }
}
/// Most claims a player can hold at once, however deep they have been.
pub const MAX_CLAIMS: usize = 8;
/// Climb speed on a lift ladder (world pixels per second), with or without fuel.
pub const LIFT_CLIMB_SPEED: f32 = 150.0;
/// Fastest fall on a lift ladder (world pixels per second): always a safe landing.
pub const LIFT_SLIDE_SPEED: f32 = 110.0;

/// A claim's stable id (never reused).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ClaimId(pub u32);

/// A placed module's stable id (never reused).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StructureId(pub u32);

/// Who may build in, and use, a claim.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ClaimAccess {
    /// Every player builds and uses services (co-op default).
    #[default]
    Crew,
    /// Everyone uses the services; only the owner builds.
    Services,
    /// Only the owner builds or uses anything.
    Owner,
}

impl ClaimAccess {
    pub const ALL: [Self; 3] = [Self::Crew, Self::Services, Self::Owner];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crew => "Open to crew",
            Self::Services => "Crew uses, owner builds",
            Self::Owner => "Owner only",
        }
    }

    /// The next setting in the beacon's cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Crew => Self::Services,
            Self::Services => Self::Owner,
            Self::Owner => Self::Crew,
        }
    }

    /// Whether `player` may build or deconstruct in a claim owned by `owner`.
    #[must_use]
    pub fn may_build(self, owner: PlayerId, player: PlayerId) -> bool {
        owner == player || self == Self::Crew
    }

    /// Whether `player` may use the services of a claim owned by `owner`.
    #[must_use]
    pub fn may_use(self, owner: PlayerId, player: PlayerId) -> bool {
        owner == player || self != Self::Owner
    }
}

/// A staked claim.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Claim {
    pub id: ClaimId,
    pub owner: PlayerId,
    /// The beacon's tile (its lower tile).
    pub center: TilePosition,
    pub access: ClaimAccess,
    /// Staked at the surface (the homestead needs one).
    pub surface: bool,
    /// The beacon module.
    pub beacon: StructureId,
}

impl Claim {
    /// Whether `position` lies inside the claim's square.
    #[must_use]
    pub const fn contains(&self, position: TilePosition) -> bool {
        (position.x - self.center.x).abs() <= CLAIM_RADIUS
            && (position.y - self.center.y).abs() <= CLAIM_RADIUS
    }

    /// The claim's square `(min, max)`, inclusive.
    #[must_use]
    pub const fn bounds(&self) -> (TilePosition, TilePosition) {
        (
            TilePosition {
                x: self.center.x - CLAIM_RADIUS,
                y: self.center.y - CLAIM_RADIUS,
            },
            TilePosition {
                x: self.center.x + CLAIM_RADIUS,
                y: self.center.y + CLAIM_RADIUS,
            },
        )
    }
}

/// A solid building piece: a terrain tile ([`crate::terrain::TileKind::Built`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BuiltTile {
    /// Masonry from building blocks (one block each).
    Wall(BlockKind),
    /// Riveted metal plating (bought).
    Plating,
    /// A glass window pane (bought).
    Glass,
    /// A plank floor or scaffold (bought).
    Plank,
    /// A vent grate: solid to rigs, drops, charges and enemies, but a pilot on foot crawls
    /// through it ([`crate::pilot::pilot_passable`]). Capping a 1-tile tunnel with grates
    /// makes a crawlway, a pilot-only space ([`crate::poi::SpaceAccess::RequiresPilot`]).
    VentGrate,
}

impl BuiltTile {
    pub const ALL: [Self; 8] = [
        Self::Wall(BlockKind::Dirt),
        Self::Wall(BlockKind::Clay),
        Self::Wall(BlockKind::Stone),
        Self::Wall(BlockKind::HardRock),
        Self::Plating,
        Self::Glass,
        Self::Plank,
        Self::VentGrate,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wall(BlockKind::Dirt) => "Rammed Earth Wall",
            Self::Wall(BlockKind::Clay) => "Brick Wall",
            Self::Wall(BlockKind::Stone) => "Stone Wall",
            Self::Wall(BlockKind::HardRock) => "Granite Wall",
            Self::Plating => "Metal Plating",
            Self::Glass => "Glass Window",
            Self::Plank => "Plank Floor",
            Self::VentGrate => "Vent Grate",
        }
    }

    /// Stable code inside the built-tile range of [`crate::terrain::Tile::code`].
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Wall(BlockKind::Dirt) => 0,
            Self::Wall(BlockKind::Clay) => 1,
            Self::Wall(BlockKind::Stone) => 2,
            Self::Wall(BlockKind::HardRock) => 3,
            Self::Plating => 4,
            Self::Glass => 5,
            Self::Plank => 6,
            Self::VentGrate => 7,
        }
    }

    /// The built tile with [`Self::code`] `code`.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        Some(match code {
            0 => Self::Wall(BlockKind::Dirt),
            1 => Self::Wall(BlockKind::Clay),
            2 => Self::Wall(BlockKind::Stone),
            3 => Self::Wall(BlockKind::HardRock),
            4 => Self::Plating,
            5 => Self::Glass,
            6 => Self::Plank,
            7 => Self::VentGrate,
            _ => return None,
        })
    }
}

/// A passable fixture in a tile's front layer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Fixture {
    /// A doorway rigs drive through (it slides open as one comes near).
    Door,
    /// A lift ladder: rigs on it climb without fuel and never fall hard.
    Ladder,
    /// A lamp (wall, floor or ceiling mount).
    Light,
    /// A potted cave fern.
    Plant,
    /// A crew banner.
    Banner,
    /// A stack of supply crates.
    Crates,
    /// A workbench table.
    Table,
    /// A climbing rope: pilots on foot climb it like a ladder (rigs ignore it).
    Rope,
}

impl Fixture {
    pub const ALL: [Self; 8] = [
        Self::Door,
        Self::Ladder,
        Self::Light,
        Self::Plant,
        Self::Banner,
        Self::Crates,
        Self::Table,
        Self::Rope,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Door => "Door",
            Self::Ladder => "Lift Ladder",
            Self::Light => "Lamp",
            Self::Plant => "Cave Fern",
            Self::Banner => "Crew Banner",
            Self::Crates => "Supply Crates",
            Self::Table => "Workbench",
            Self::Rope => "Climbing Rope",
        }
    }

    /// Credits the fixture costs.
    #[must_use]
    pub const fn credits(self) -> u32 {
        match self {
            Self::Door => 25,
            Self::Ladder => 6,
            Self::Light => 15,
            Self::Plant => 10,
            Self::Banner => 12,
            Self::Crates => 8,
            Self::Table => 14,
            Self::Rope => 4,
        }
    }

    /// Whether it must stand on something solid.
    #[must_use]
    pub const fn needs_floor(self) -> bool {
        matches!(self, Self::Plant | Self::Crates | Self::Table)
    }

    /// The facings the rotate button cycles through.
    #[must_use]
    pub const fn facings(self) -> &'static [Facing] {
        match self {
            Self::Light => &[Facing::Up, Facing::Left, Facing::Down, Facing::Right],
            Self::Door | Self::Banner | Self::Table | Self::Plant | Self::Crates => {
                &[Facing::Right, Facing::Left]
            }
            Self::Ladder | Self::Rope => &[Facing::Right],
        }
    }
}

/// Which way a piece faces (doors and decorations flip; lamps mount on the surface they face
/// away from: `Up` hangs from the ceiling... see [`Fixture::facings`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Facing {
    #[default]
    Right,
    Down,
    Left,
    Up,
}

impl Facing {
    /// The facing after `self` among `allowed` (the first one when `self` is not allowed).
    #[must_use]
    pub fn next_in(self, allowed: &[Self]) -> Self {
        let Some(first) = allowed.first().copied() else {
            return self;
        };
        allowed
            .iter()
            .position(|facing| *facing == self)
            .map_or(first, |index| allowed[(index + 1) % allowed.len()])
    }

    /// `self` if `allowed` has it, otherwise the first allowed facing.
    #[must_use]
    pub fn within(self, allowed: &[Self]) -> Self {
        if allowed.contains(&self) {
            self
        } else {
            allowed.first().copied().unwrap_or(self)
        }
    }
}

/// What one tile holds besides terrain: a back wall and a front fixture.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileFixtures {
    /// An indoor back wall of this block's masonry.
    #[serde(default)]
    pub back_wall: Option<BlockKind>,
    /// A passable fixture in front.
    #[serde(default)]
    pub front: Option<(Fixture, Facing)>,
}

impl TileFixtures {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.back_wall.is_none() && self.front.is_none()
    }
}

/// A functional base module.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ModuleKind {
    /// Stakes the claim; carries a small power cell; sets who may build.
    Beacon,
    /// A storage chest shared with everyone the claim lets in.
    Storage,
    /// Crew quarters: set it as home and rescues bring you back here.
    Bed,
    /// Power for the claim's modules (geothermal: more the deeper it sits).
    Generator,
    /// Refuel at depth from a reserve the station synthesizes while powered.
    FuelStation,
    /// Hull repairs at town prices while powered.
    RepairBay,
    /// Beams cargo to the town depot for a fee while powered.
    CargoUplink,
    /// A surface house: a home point with a pantry chest and a trophy shelf.
    Homestead,
    /// Refines ore left in it while powered; the refined value is worth
    /// [`REFINERY_BONUS_PERCENT`] more than selling the raw ore.
    Refinery,
    /// A crafting bench: presses building blocks (plus a little credit) into explosives.
    Workshop,
}

impl ModuleKind {
    pub const ALL: [Self; 10] = [
        Self::Beacon,
        Self::Storage,
        Self::Bed,
        Self::Generator,
        Self::FuelStation,
        Self::RepairBay,
        Self::CargoUplink,
        Self::Homestead,
        Self::Refinery,
        Self::Workshop,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Beacon => "Claim Beacon",
            Self::Storage => "Storage Chest",
            Self::Bed => "Crew Quarters",
            Self::Generator => "Generator",
            Self::FuelStation => "Fuel Station",
            Self::RepairBay => "Repair Bay",
            Self::CargoUplink => "Cargo Uplink",
            Self::Homestead => "Homestead",
            Self::Refinery => "Ore Refinery",
            Self::Workshop => "Workshop",
        }
    }

    /// One line on what it does.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Beacon => "Stakes a claim to build in. Its cell powers one small module.",
            Self::Storage => "A chest for ore, blocks and finds, shared with your crew.",
            Self::Bed => "Set it as home: the rescue crew drops you here.",
            Self::Generator => "Powers modules. Geothermal: stronger the deeper it sits.",
            Self::FuelStation => "Refuel at depth. Synthesizes fuel while powered.",
            Self::RepairBay => "Patches hulls at town prices while powered.",
            Self::CargoUplink => "Beams your cargo to the depot for a 15% fee.",
            Self::Homestead => "A house on the surface: home point, pantry, trophy shelf.",
            Self::Refinery => "Leave ore here: refined while powered, worth 40% more.",
            Self::Workshop => "Presses building blocks into bombs and charges.",
        }
    }

    /// Footprint `(width, height)` in tiles.
    #[must_use]
    pub const fn footprint(self) -> (i32, i32) {
        match self {
            Self::Beacon => (1, 2),
            Self::Storage => (1, 1),
            Self::Bed => (2, 1),
            Self::Generator | Self::Workshop => (2, 2),
            Self::FuelStation | Self::RepairBay | Self::Refinery => (3, 3),
            Self::CargoUplink => (2, 3),
            Self::Homestead => (7, 5),
        }
    }

    /// Base credit price (a claim beacon's grows with each claim held, see
    /// [`beacon_price`]).
    #[must_use]
    pub const fn credits(self) -> u32 {
        match self {
            Self::Beacon => 250,
            Self::Storage => 150,
            Self::Bed => 200,
            Self::Generator => 450,
            Self::FuelStation => 900,
            Self::RepairBay => 1_200,
            Self::CargoUplink => 2_400,
            Self::Homestead => 1_500,
            Self::Refinery => 1_800,
            Self::Workshop => 500,
        }
    }

    /// Building blocks of any kind it takes (missing ones are bought, see
    /// [`block_credit_price`] for dirt).
    #[must_use]
    pub const fn blocks(self) -> u32 {
        match self {
            Self::Beacon => 4,
            Self::Storage => 6,
            Self::Bed | Self::Workshop => 8,
            Self::Generator => 10,
            Self::FuelStation | Self::RepairBay => 16,
            Self::CargoUplink => 20,
            Self::Homestead => 30,
            Self::Refinery => 18,
        }
    }

    /// Power it draws while working.
    #[must_use]
    pub const fn power_draw(self) -> u32 {
        match self {
            Self::FuelStation => 1,
            Self::RepairBay | Self::CargoUplink | Self::Refinery => 2,
            Self::Beacon
            | Self::Storage
            | Self::Bed
            | Self::Generator
            | Self::Homestead
            | Self::Workshop => 0,
        }
    }

    /// Power it supplies when standing in `band` (the beacon's cell; a generator's output
    /// grows with the band's heat).
    #[must_use]
    pub const fn power_supply(self, band: Band) -> u32 {
        match self {
            Self::Beacon => BEACON_POWER,
            Self::Generator => 2 + crate::protection::band_heat_level(band) as u32 / 3,
            _ => 0,
        }
    }

    /// The band the world must have been reached to build it (`None`: from the start).
    #[must_use]
    pub const fn unlock_band(self) -> Option<Band> {
        match self {
            Self::FuelStation => Some(Band::Clay),
            Self::RepairBay | Self::Refinery => Some(Band::Stone),
            Self::CargoUplink => Some(Band::Slate),
            _ => None,
        }
    }

    /// Whether it can only go on a surface claim.
    #[must_use]
    pub const fn surface_only(self) -> bool {
        matches!(self, Self::Homestead)
    }

    /// Slots of storage it holds (0: none).
    #[must_use]
    pub const fn storage_capacity(self) -> u32 {
        match self {
            Self::Storage => 80,
            Self::Homestead => 40,
            Self::Refinery => 60,
            _ => 0,
        }
    }

    /// Whether players can call it home.
    #[must_use]
    pub const fn is_home(self) -> bool {
        matches!(self, Self::Bed | Self::Homestead)
    }

    /// The services its menu offers, in menu order.
    #[must_use]
    pub const fn services(self) -> &'static [ModuleService] {
        match self {
            Self::Beacon => &[ModuleService::CycleAccess],
            Self::Storage => &[
                ModuleService::DepositCargo,
                ModuleService::DepositBlocks,
                ModuleService::WithdrawAll,
            ],
            Self::Bed => &[ModuleService::SetHome, ModuleService::Sleep],
            Self::Generator => &[],
            Self::FuelStation => &[ModuleService::Refuel, ModuleService::OrderSupply],
            Self::RepairBay => &[ModuleService::Repair],
            Self::CargoUplink => &[ModuleService::SellCargo],
            Self::Homestead => &[
                ModuleService::SetHome,
                ModuleService::Sleep,
                ModuleService::DepositCargo,
                ModuleService::DepositBlocks,
                ModuleService::WithdrawAll,
            ],
            Self::Refinery => &[ModuleService::RefineOre, ModuleService::CollectRefined],
            Self::Workshop => &[
                ModuleService::Craft(ExplosiveKind::Bomb),
                ModuleService::Craft(ExplosiveKind::C4),
                ModuleService::Craft(ExplosiveKind::Mining),
            ],
        }
    }
}

/// What a workshop charges to craft one `kind`: `(building blocks of any kind, credits)`.
///
/// `None` when the workshop cannot make it. Blocks must be on hand (they are the point); about
/// half the explosives shack's price once T7 drops fill the hopper.
#[must_use]
pub const fn workshop_recipe(kind: ExplosiveKind) -> Option<(u32, u32)> {
    match kind {
        ExplosiveKind::Bomb => Some((3, 6)),
        ExplosiveKind::C4 => Some((5, 15)),
        ExplosiveKind::Mining => Some((8, 30)),
        _ => None,
    }
}

/// A claim beacon's credit price when `held` claims are already held.
#[must_use]
pub const fn beacon_price(held: usize) -> u32 {
    #[allow(clippy::cast_possible_truncation, reason = "a handful of claims")]
    let held = held as u32;
    ModuleKind::Beacon.credits().saturating_mul(held + 1)
}

/// Most claims a player may hold once the world's deepest dig reached band index
/// `deepest_band` (two to start, one more every two bands).
#[must_use]
pub const fn claim_limit(deepest_band: usize) -> usize {
    let limit = 2 + deepest_band / 2;
    if limit > MAX_CLAIMS {
        MAX_CLAIMS
    } else {
        limit
    }
}

/// A service a module offers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ModuleService {
    /// Fill the rig's tank from the station's reserve (free).
    Refuel,
    /// Fill the station's reserve with a supply drop (credits).
    OrderSupply,
    /// Repair the hull (credits, needs power).
    Repair,
    /// Put the cargo bay's ore, artifacts and materials in storage.
    DepositCargo,
    /// Put the block hopper's blocks in storage.
    DepositBlocks,
    /// Take everything that fits out of storage.
    WithdrawAll,
    /// Sell the cargo bay's ore and artifacts through the uplink (needs power).
    SellCargo,
    /// Make this bed or homestead the player's home point.
    SetHome,
    /// The beacon owner cycles who may build and use the claim.
    CycleAccess,
    /// Leave the cargo bay's ore in a refinery to be refined.
    RefineOre,
    /// Take the credits a refinery's refined ore is worth.
    CollectRefined,
    /// Craft one explosive at a workshop ([`workshop_recipe`]).
    Craft(ExplosiveKind),
    /// Lie down in this bed or homestead for the night (see [`crate::clock`]: once everyone is
    /// in bed the night is skipped).
    Sleep,
}

impl ModuleService {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Refuel => "Refuel",
            Self::OrderSupply => "Order supply drop",
            Self::Repair => "Repair hull",
            Self::DepositCargo => "Store cargo",
            Self::DepositBlocks => "Store blocks",
            Self::WithdrawAll => "Take everything",
            Self::SellCargo => "Uplink cargo to depot",
            Self::SetHome => "Make this home",
            Self::CycleAccess => "Claim access",
            Self::RefineOre => "Load ore to refine",
            Self::CollectRefined => "Collect refined value",
            Self::Craft(ExplosiveKind::Bomb) => "Press a bomb",
            Self::Craft(ExplosiveKind::C4) => "Pack a C4 charge",
            Self::Craft(ExplosiveKind::Mining) => "Pack a mining charge",
            Self::Craft(_) => "Craft",
            Self::Sleep => "Sleep until dawn",
        }
    }
}

/// Anything the build menu places.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Buildable {
    Tile(BuiltTile),
    BackWall(BlockKind),
    Fixture(Fixture),
    Module(ModuleKind),
}

/// Build-menu groups.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BuildGroup {
    Structure,
    Interior,
    Modules,
}

impl BuildGroup {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Structure => "Structure",
            Self::Interior => "Interior",
            Self::Modules => "Modules",
        }
    }
}

impl Buildable {
    /// The build menu, in order.
    pub const PALETTE: [Self; 30] = [
        Self::Module(ModuleKind::Beacon),
        Self::Tile(BuiltTile::Wall(BlockKind::Dirt)),
        Self::Tile(BuiltTile::Wall(BlockKind::Clay)),
        Self::Tile(BuiltTile::Wall(BlockKind::Stone)),
        Self::Tile(BuiltTile::Wall(BlockKind::HardRock)),
        Self::Tile(BuiltTile::Plating),
        Self::Tile(BuiltTile::Glass),
        Self::Tile(BuiltTile::Plank),
        Self::Tile(BuiltTile::VentGrate),
        Self::BackWall(BlockKind::Dirt),
        Self::BackWall(BlockKind::Clay),
        Self::BackWall(BlockKind::Stone),
        Self::BackWall(BlockKind::HardRock),
        Self::Fixture(Fixture::Door),
        Self::Fixture(Fixture::Ladder),
        Self::Fixture(Fixture::Rope),
        Self::Fixture(Fixture::Light),
        Self::Fixture(Fixture::Plant),
        Self::Fixture(Fixture::Banner),
        Self::Fixture(Fixture::Crates),
        Self::Fixture(Fixture::Table),
        Self::Module(ModuleKind::Storage),
        Self::Module(ModuleKind::Bed),
        Self::Module(ModuleKind::Generator),
        Self::Module(ModuleKind::FuelStation),
        Self::Module(ModuleKind::RepairBay),
        Self::Module(ModuleKind::CargoUplink),
        Self::Module(ModuleKind::Homestead),
        Self::Module(ModuleKind::Workshop),
        Self::Module(ModuleKind::Refinery),
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tile(tile) => tile.name(),
            Self::BackWall(BlockKind::Dirt) => "Earth Back Wall",
            Self::BackWall(BlockKind::Clay) => "Brick Back Wall",
            Self::BackWall(BlockKind::Stone) => "Stone Back Wall",
            Self::BackWall(BlockKind::HardRock) => "Granite Back Wall",
            Self::Fixture(fixture) => fixture.name(),
            Self::Module(module) => module.name(),
        }
    }

    #[must_use]
    pub const fn group(self) -> BuildGroup {
        match self {
            Self::Tile(_) | Self::Module(ModuleKind::Beacon) => BuildGroup::Structure,
            Self::BackWall(_) | Self::Fixture(_) => BuildGroup::Interior,
            Self::Module(_) => BuildGroup::Modules,
        }
    }

    /// Footprint `(width, height)` in tiles; the placement tile is its bottom-left.
    #[must_use]
    pub const fn footprint(self) -> (i32, i32) {
        match self {
            Self::Module(module) => module.footprint(),
            _ => (1, 1),
        }
    }

    /// The facings rotate cycles through.
    #[must_use]
    pub const fn facings(self) -> &'static [Facing] {
        match self {
            Self::Fixture(fixture) => fixture.facings(),
            Self::Module(_) => &[Facing::Right, Facing::Left],
            Self::Tile(_) | Self::BackWall(_) => &[Facing::Right],
        }
    }

    /// What one costs before stock on hand: a specific block (or its credit price), blocks of
    /// any kind, and credits. A claim beacon's credits grow with claims held
    /// ([`beacon_price`]).
    #[must_use]
    pub const fn base_cost(self) -> BuildCost {
        match self {
            Self::Tile(BuiltTile::Wall(block)) | Self::BackWall(block) => BuildCost {
                block: Some(block),
                any_blocks: 0,
                credits: 0,
            },
            Self::Tile(BuiltTile::Plating) => BuildCost::credits(12),
            Self::Tile(BuiltTile::Glass) => BuildCost::credits(8),
            Self::Tile(BuiltTile::Plank) => BuildCost::credits(4),
            Self::Tile(BuiltTile::VentGrate) => BuildCost::credits(10),
            Self::Fixture(fixture) => BuildCost::credits(fixture.credits()),
            Self::Module(module) => BuildCost {
                block: None,
                any_blocks: module.blocks(),
                credits: module.credits(),
            },
        }
    }
}

/// What a piece costs: one block of a specific kind, some blocks of any kind, and credits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildCost {
    pub block: Option<BlockKind>,
    pub any_blocks: u32,
    pub credits: u32,
}

impl BuildCost {
    #[must_use]
    pub const fn credits(credits: u32) -> Self {
        Self {
            block: None,
            any_blocks: 0,
            credits,
        }
    }
}

/// What a build action takes from a player once stock on hand is counted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BuildPayment {
    /// Blocks taken from the hopper.
    pub blocks: ItemContainer,
    /// Credits charged (missing blocks bought included).
    pub credits: u32,
}

/// A build action, sent inside [`crate::PlayerCommand::Build`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BuildCommand {
    /// Place `item` with its footprint's bottom-left tile at `at`.
    Place {
        item: Buildable,
        at: TilePosition,
        facing: Facing,
    },
    /// Deconstruct what is at `at` (a module covering it, else a solid piece, else the front
    /// fixture, else the back wall) for a refund.
    Remove { at: TilePosition },
    /// Use one of a module's services (the rig stands at it).
    Use {
        structure: StructureId,
        service: ModuleService,
    },
}

/// Why a build action is refused (the preview shows it; the tick ignores the action).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildRefusal {
    UnknownPlayer,
    /// The rig is stranded or wrecked.
    RigDisabled,
    OutOfReach,
    /// Outside every claim (stake one with a beacon first).
    NoClaim,
    /// The claim's owner does not let this player build or use it.
    NotPermitted,
    /// Rock, another piece, a module or a rig is in the way.
    Blocked,
    /// Needs solid ground under it.
    NeedsFloor,
    /// Too close to another claim.
    OverlapsClaim,
    /// The town is not for building.
    InTown,
    /// Outside the world.
    OutsideWorld,
    TooManyClaims,
    /// Not unlocked until the world's digs reach this band.
    Locked(Band),
    /// Only on a surface claim.
    SurfaceOnly,
    /// Spans the edge of its claim.
    LeavesClaim,
    CannotAfford,
    /// A beacon with structures left in its claim, or a chest with things in it.
    NotEmpty,
    /// A floor tile a module or a standing fixture rests on.
    Supporting,
    NothingThere,
    /// The module needs power.
    Unpowered,
    /// The rig is not at that module.
    NotAtModule,
    /// That module does not offer that service.
    NoSuchService,
    /// Nothing to do (tank full, bay empty, ...).
    NothingToDo,
    /// A workshop recipe needs this many building blocks in the hopper.
    NeedsBlocks(u32),
    /// The claim would overlap a village (WP-VILLAGE): villages are not for claiming.
    InVillage,
    /// Beds are for the night: lying down only works at dusk or night (see [`crate::clock`]).
    NotTired,
}

impl BuildRefusal {
    /// A short reason for the preview and toasts.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::UnknownPlayer => "No rig.".to_owned(),
            Self::RigDisabled => "The rig is disabled.".to_owned(),
            Self::OutOfReach => "Out of reach.".to_owned(),
            Self::NoClaim => "Stake a claim beacon here first.".to_owned(),
            Self::NotPermitted => "The claim's owner has not let you in.".to_owned(),
            Self::Blocked => "Something is in the way.".to_owned(),
            Self::NeedsFloor => "Needs solid ground under it.".to_owned(),
            Self::OverlapsClaim => "Too close to another claim.".to_owned(),
            Self::InTown => "Town land: build outside the town.".to_owned(),
            Self::OutsideWorld => "Outside the world.".to_owned(),
            Self::TooManyClaims => "Claim limit reached: dig deeper for more.".to_owned(),
            Self::Locked(band) => format!("Unlocks once the {} is reached.", band.name()),
            Self::SurfaceOnly => "Only on a surface claim.".to_owned(),
            Self::LeavesClaim => "It has to fit inside the claim.".to_owned(),
            Self::CannotAfford => "Not enough credits.".to_owned(),
            Self::NotEmpty => "Empty it first.".to_owned(),
            Self::Supporting => "Something stands on it: take that down first.".to_owned(),
            Self::NothingThere => "Nothing to take down here.".to_owned(),
            Self::Unpowered => "No power: build a generator.".to_owned(),
            Self::NotAtModule => "Stand at the module.".to_owned(),
            Self::NoSuchService => "Not offered here.".to_owned(),
            Self::NothingToDo => "Nothing to do.".to_owned(),
            Self::NeedsBlocks(count) => format!("Needs {count} building blocks in the hopper."),
            Self::InVillage => "Village land: stake your claim outside the village.".to_owned(),
            Self::NotTired => "Too early to sleep: beds are for dusk and night.".to_owned(),
        }
    }
}

/// A placed module.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlacedModule {
    pub id: StructureId,
    pub kind: ModuleKind,
    /// Top-left tile of its footprint.
    pub origin: TilePosition,
    pub facing: Facing,
    pub claim: ClaimId,
    /// Who built it.
    pub owner: PlayerId,
    /// Credits paid (refunded in part on deconstruction).
    #[serde(default)]
    pub paid_credits: u32,
    /// Blocks paid (refunded in full on deconstruction).
    #[serde(default)]
    pub paid_blocks: ItemContainer,
    /// A fuel station's reserve.
    #[serde(default)]
    pub fuel: u32,
    /// A chest's or homestead's contents (a refinery's ore waiting to be refined).
    #[serde(default)]
    pub storage: ItemContainer,
    /// Credits a refinery's refined ore is worth, waiting to be collected.
    #[serde(default)]
    pub refined: u32,
}

impl PlacedModule {
    /// Whether the footprint covers `position`.
    #[must_use]
    pub const fn covers(&self, position: TilePosition) -> bool {
        let (width, height) = self.kind.footprint();
        position.x >= self.origin.x
            && position.x < self.origin.x + width
            && position.y >= self.origin.y
            && position.y < self.origin.y + height
    }

    /// Every tile of the footprint, row by row.
    pub fn tiles(&self) -> impl Iterator<Item = TilePosition> + use<> {
        footprint_tiles(self.origin, self.kind.footprint())
    }

    /// The bottom-left tile (where it was placed).
    #[must_use]
    pub const fn anchor(&self) -> TilePosition {
        let (_, height) = self.kind.footprint();
        TilePosition {
            x: self.origin.x,
            y: self.origin.y + height - 1,
        }
    }

    /// Whether a rig standing on tile `rig` can use it (inside the footprint or touching it).
    #[must_use]
    pub const fn reachable_from(&self, rig: TilePosition) -> bool {
        let (width, height) = self.kind.footprint();
        rig.x >= self.origin.x - 1
            && rig.x <= self.origin.x + width
            && rig.y >= self.origin.y - 1
            && rig.y <= self.origin.y + height
    }
}

/// The tiles of a `(width, height)` footprint with its top-left at `origin`, row by row.
pub fn footprint_tiles(
    origin: TilePosition,
    (width, height): (i32, i32),
) -> impl Iterator<Item = TilePosition> {
    (0..height).flat_map(move |dy| {
        (0..width).map(move |dx| TilePosition {
            x: origin.x + dx,
            y: origin.y + dy,
        })
    })
}

/// The top-left tile of a footprint whose bottom-left tile is `anchor`.
#[must_use]
pub const fn origin_for(anchor: TilePosition, (_, height): (i32, i32)) -> TilePosition {
    TilePosition {
        x: anchor.x,
        y: anchor.y - height + 1,
    }
}

/// A claim's power budget.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PowerBudget {
    pub supply: u32,
    pub demand: u32,
}

/// Every claim, module and fixture in the world (one shared replication section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Bases {
    #[serde(default)]
    next_claim: u32,
    #[serde(default)]
    next_structure: u32,
    #[serde(default)]
    claims: BTreeMap<ClaimId, Claim>,
    #[serde(default)]
    modules: BTreeMap<StructureId, PlacedModule>,
    /// Saved as a list: JSON map keys must be strings.
    #[serde(default, with = "fixture_list")]
    fixtures: BTreeMap<TilePosition, TileFixtures>,
    /// Each player's home point (a bed or homestead).
    #[serde(default)]
    homes: BTreeMap<PlayerId, StructureId>,
}

impl Bases {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_claim: 0,
            next_structure: 0,
            claims: BTreeMap::new(),
            modules: BTreeMap::new(),
            fixtures: BTreeMap::new(),
            homes: BTreeMap::new(),
        }
    }

    /// Whether nothing has been built anywhere.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.claims.is_empty() && self.modules.is_empty() && self.fixtures.is_empty()
    }

    pub fn claims(&self) -> impl Iterator<Item = &Claim> + '_ {
        self.claims.values()
    }

    #[must_use]
    pub fn claim(&self, id: ClaimId) -> Option<&Claim> {
        self.claims.get(&id)
    }

    /// The claim whose square contains `position`, if any (claims never overlap).
    #[must_use]
    pub fn claim_at(&self, position: TilePosition) -> Option<&Claim> {
        self.claims.values().find(|claim| claim.contains(position))
    }

    /// How many claims `player` holds.
    #[must_use]
    pub fn claims_held(&self, player: PlayerId) -> usize {
        self.claims
            .values()
            .filter(|claim| claim.owner == player)
            .count()
    }

    /// Whether nothing hostile may spawn at `position`: inside a claim or within
    /// [`CLAIM_SAFE_MARGIN`] tiles of one (WP-ENEMIES spawn rule).
    #[must_use]
    pub fn is_safe_zone(&self, position: TilePosition) -> bool {
        let reach = CLAIM_RADIUS + CLAIM_SAFE_MARGIN;
        self.claims.values().any(|claim| {
            (position.x - claim.center.x).abs() <= reach
                && (position.y - claim.center.y).abs() <= reach
        })
    }

    pub fn modules(&self) -> impl Iterator<Item = &PlacedModule> + '_ {
        self.modules.values()
    }

    #[must_use]
    pub fn module(&self, id: StructureId) -> Option<&PlacedModule> {
        self.modules.get(&id)
    }

    /// The module whose footprint covers `position`.
    #[must_use]
    pub fn module_at(&self, position: TilePosition) -> Option<&PlacedModule> {
        self.modules.values().find(|module| module.covers(position))
    }

    /// The modules of `claim`, in placement order.
    pub fn modules_in(&self, claim: ClaimId) -> impl Iterator<Item = &PlacedModule> + '_ {
        self.modules
            .values()
            .filter(move |module| module.claim == claim)
    }

    /// The first module (placement order) a rig on tile `rig` can use.
    #[must_use]
    pub fn module_reachable_from(&self, rig: TilePosition) -> Option<&PlacedModule> {
        // Prefer one whose footprint the rig is inside.
        self.modules
            .values()
            .find(|module| module.covers(rig))
            .or_else(|| {
                self.modules
                    .values()
                    .find(|module| module.reachable_from(rig))
            })
    }

    /// Every tile with fixtures, in position order.
    pub fn fixtures(&self) -> impl Iterator<Item = (TilePosition, &TileFixtures)> + '_ {
        self.fixtures
            .iter()
            .map(|(position, fixtures)| (*position, fixtures))
    }

    /// The fixtures with positions in `min..=max` (both corners inclusive), in position order.
    pub fn fixtures_in(
        &self,
        min: TilePosition,
        max: TilePosition,
    ) -> impl Iterator<Item = (TilePosition, &TileFixtures)> + '_ {
        self.fixtures
            .range(
                TilePosition {
                    x: min.x,
                    y: i32::MIN,
                }..=TilePosition {
                    x: max.x,
                    y: i32::MAX,
                },
            )
            .filter(move |(position, _)| position.y >= min.y && position.y <= max.y)
            .map(|(position, fixtures)| (*position, fixtures))
    }

    /// The fixtures at `position` (empty when none).
    #[must_use]
    pub fn fixtures_at(&self, position: TilePosition) -> TileFixtures {
        self.fixtures.get(&position).copied().unwrap_or_default()
    }

    /// Whether a lift ladder stands at `position`.
    #[must_use]
    pub fn ladder_at(&self, position: TilePosition) -> bool {
        matches!(
            self.fixtures
                .get(&position)
                .and_then(|fixtures| fixtures.front),
            Some((Fixture::Ladder, _))
        )
    }

    /// `player`'s home point, if it still stands.
    #[must_use]
    pub fn home_of(&self, player: PlayerId) -> Option<&PlacedModule> {
        self.homes.get(&player).and_then(|id| self.modules.get(id))
    }

    /// `claim`'s power budget; `band_of` names the band a module stands in.
    #[must_use]
    pub fn power(&self, claim: ClaimId, band_of: impl Fn(TilePosition) -> Band) -> PowerBudget {
        self.modules_in(claim)
            .fold(PowerBudget::default(), |budget, module| PowerBudget {
                supply: budget.supply + module.kind.power_supply(band_of(module.anchor())),
                demand: budget.demand + module.kind.power_draw(),
            })
    }

    /// Whether `module` has power: modules draw from their claim's supply in placement order
    /// while it lasts (modules that draw nothing are always powered).
    #[must_use]
    pub fn powered(&self, module: StructureId, band_of: impl Fn(TilePosition) -> Band) -> bool {
        let Some(target) = self.modules.get(&module) else {
            return false;
        };
        if target.kind.power_draw() == 0 {
            return true;
        }
        let budget = self.power(target.claim, &band_of);
        let mut drawn = 0;
        for other in self.modules_in(target.claim) {
            let draw = other.kind.power_draw();
            if draw == 0 {
                continue;
            }
            if drawn + draw > budget.supply {
                return false;
            }
            drawn += draw;
            if other.id == module {
                return true;
            }
        }
        false
    }

    // ----- mutation (crate-internal: only the fixed tick changes bases) -----

    pub(crate) fn stake_claim(
        &mut self,
        owner: PlayerId,
        center: TilePosition,
        surface: bool,
        beacon: PlacedModule,
    ) -> ClaimId {
        let id = ClaimId(self.next_claim);
        self.next_claim = self.next_claim.saturating_add(1);
        let beacon_id = self.insert_module(PlacedModule {
            claim: id,
            ..beacon
        });
        self.claims.insert(
            id,
            Claim {
                id,
                owner,
                center,
                access: ClaimAccess::default(),
                surface,
                beacon: beacon_id,
            },
        );
        id
    }

    pub(crate) fn insert_module(&mut self, module: PlacedModule) -> StructureId {
        let id = StructureId(self.next_structure);
        self.next_structure = self.next_structure.saturating_add(1);
        self.modules.insert(id, PlacedModule { id, ..module });
        id
    }

    pub(crate) fn module_mut(&mut self, id: StructureId) -> Option<&mut PlacedModule> {
        self.modules.get_mut(&id)
    }

    /// Removes a module (and its claim when it is the beacon) and every home pointing at it.
    pub(crate) fn remove_module(&mut self, id: StructureId) -> Option<PlacedModule> {
        let module = self.modules.remove(&id)?;
        self.homes.retain(|_, home| *home != id);
        if module.kind == ModuleKind::Beacon {
            self.claims.remove(&module.claim);
        }
        Some(module)
    }

    pub(crate) fn set_access(&mut self, claim: ClaimId, access: ClaimAccess) {
        if let Some(claim) = self.claims.get_mut(&claim) {
            claim.access = access;
        }
    }

    pub(crate) fn set_home(&mut self, player: PlayerId, module: StructureId) {
        self.homes.insert(player, module);
    }

    pub(crate) fn update_fixtures(
        &mut self,
        position: TilePosition,
        update: impl FnOnce(&mut TileFixtures),
    ) {
        let mut fixtures = self.fixtures_at(position);
        update(&mut fixtures);
        if fixtures.is_empty() {
            self.fixtures.remove(&position);
        } else {
            self.fixtures.insert(position, fixtures);
        }
    }
}

/// (De)serializes the fixture map as a list of `(position, fixtures)` entries.
mod fixture_list {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::TileFixtures;
    use crate::terrain::TilePosition;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<TilePosition, TileFixtures>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let entries: Vec<(&TilePosition, &TileFixtures)> = map.iter().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<TilePosition, TileFixtures>, D::Error> {
        let entries = Vec::<(TilePosition, TileFixtures)>::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}
