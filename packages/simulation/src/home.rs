//! Contractor-built homes (expansion plan WP-HOME, R21).
//!
//! # Plots and deeds
//!
//! A player buys a **plot** ([`Plot`]) at a land office (the town's headquarters, or a village
//! hall): one of the listed sites ([`plot::listed_sites_between`]: around town, at the edge of
//! every surface village, scenic lakesides, clifftops and meadows) or any free spot of open
//! ground they survey where they stand (outside the town, villages, claims and other plots;
//! sky islands and the Moon included). Plots extend the base-building claims (T12): a deed
//! ([`Deed`]) names the owner, co-owners and who may come in ([`HomeAccess`], like
//! [`crate::base::ClaimAccess`]); claims and plots never overlap, and every plot is an enemy
//! safe zone ([`Homes::is_safe_zone`]).
//!
//! # Blueprints and contractors
//!
//! On a plot the owner signs a contract ([`HomeCommand::Hire`]): a [`Blueprint`] (cabin,
//! cottage, house, workshop home, manor, sky villa, lunar habitat) or later an [`Expansion`]
//! (rooms, floors, a garage bay, a basement down into the mine, a private lift to a base),
//! built by a contractor: a real person ([`crate::people::contractor`]) whose personality sets
//! the time, price and quality, or the town builders' guild on worlds without people.
//! Supplying blocks from the hopper takes money off; repeat clients get a discount. The
//! contractor remembers you (their [`crate::people::CharacterState`], plus [`Homes::jobs_with`]).
//!
//! # Construction in world time
//!
//! A [`Construction`] progresses in world time only (`WorldClock::world_millis`, plan §11
//! decision 3), through visible stages ([`BuildStage`]); sleeping through the night moves it
//! on. During working hours ([`WORK_START_MINUTE`]..[`WORK_END_MINUTE`]) the contractor and
//! helpers work on site as NPC entities (`world/homes.rs`). Small story events
//! ([`SiteEventKind`]) happen along the way; paying rushes the work.
//!
//! # Inside
//!
//! A finished [`Home`] has rooms ([`Room`], by floor) the pilot walks in (the game draws them),
//! each decorated on a grid ([`decor`]): furniture, lamps, wallpaper and floors, plants, rugs,
//! trophy cases, storage, a bed (sleep, home point), a kitchen stove, a workshop bench and a
//! garage bay for the rig (a rig parked in the bay is repaired).
//!
//! # State, saves and replication
//!
//! [`Homes`] is one shared world field (serde default: older saves load with none), the
//! `SectionKey::Homes` on-change section, hashed only once something exists.

pub mod blueprint;
pub mod construction;
pub mod decor;
pub mod plot;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub use blueprint::{Blueprint, BlueprintSite, Expansion, HomeJob, RoomKind};
pub use construction::{BuildStage, Construction, SiteEvent, SiteEventKind};
pub use decor::{
    DecorRow, DecorService, Flooring, Furniture, FurnitureStock, RoomDecor, Wallpaper,
};
pub use plot::{PlotSetting, PlotSite, PlotSize, SiteKey};

use crate::{
    ids::PlayerId,
    item::{ItemContainer, ItemKind},
    people::CharacterId,
    terrain::TilePosition,
};

/// Workers arrive at 07:00 on the world clock ...
pub const WORK_START_MINUTE: u16 = 7 * 60;
/// ... and down tools at 18:00.
pub const WORK_END_MINUTE: u16 = 18 * 60;
/// Tiles around a plot where nothing hostile spawns (plots are safe zones).
pub const PLOT_SAFE_MARGIN: i32 = 6;
/// How far (columns) beyond a plot's ends its site board can be used from.
pub const SITE_REACH_COLUMNS: i32 = 2;
/// How far (rows) above or below the ground row the actor may stand to use the site.
pub const SITE_REACH_ROWS: i32 = 5;
/// The most plots one player may hold.
pub const MAX_PLOTS: usize = 4;
/// Hull a rig parked in a garage bay gets back per second.
pub const GARAGE_REPAIR_PER_SECOND: f32 = 6.0;
/// Credits per point of hull a garage bay charges the rig's player.
///
/// Parts at cost: half the town garage's [`crate::economy::REPAIR_UNIT_COST`]. A point is paid as the hull starts
/// into it; a rig whose player cannot pay waits at the last paid point.
pub const GARAGE_REPAIR_UNIT_COST: u32 = 1;
/// Goodwill a contractor feels for a client when the work is handed over.
pub const HANDOVER_GOODWILL: i16 = 12;

/// A plot's stable id (never reused).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PlotId(pub u32);

/// Who may come into a home, and who may decorate it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HomeAccess {
    /// Everyone visits and decorates (the co-op default for a crew's shared house).
    Crew,
    /// Everyone may visit; only owners decorate.
    #[default]
    Visitors,
    /// Only the owner and co-owners come in.
    Owners,
}

impl HomeAccess {
    pub const ALL: [Self; 3] = [Self::Crew, Self::Visitors, Self::Owners];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crew => "Open house: the crew decorates",
            Self::Visitors => "Visitors welcome",
            Self::Owners => "Owners only",
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Crew => Self::Visitors,
            Self::Visitors => Self::Owners,
            Self::Owners => Self::Crew,
        }
    }
}

/// The ownership deed of a plot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Deed {
    pub owner: PlayerId,
    /// Players who own the plot with the owner (they decorate, hire and use everything; only
    /// the owner changes the deed).
    #[serde(default)]
    pub co_owners: BTreeSet<PlayerId>,
    #[serde(default)]
    pub access: HomeAccess,
    /// World time the deed was signed.
    pub bought_millis: u64,
    /// Credits paid for it.
    pub paid: u32,
}

impl Deed {
    /// Whether `player` owns the plot (owner or co-owner).
    #[must_use]
    pub fn owns(&self, player: PlayerId) -> bool {
        self.owner == player || self.co_owners.contains(&player)
    }

    /// Whether `player` may come in.
    #[must_use]
    pub fn may_enter(&self, player: PlayerId) -> bool {
        self.owns(player) || self.access != HomeAccess::Owners
    }

    /// Whether `player` may move furniture and restyle rooms.
    #[must_use]
    pub fn may_decorate(&self, player: PlayerId) -> bool {
        self.owns(player) || self.access == HomeAccess::Crew
    }
}

/// One room of a home.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Room {
    pub kind: RoomKind,
    /// 0 the ground floor, up from there; -1 the basement.
    pub floor: i8,
    #[serde(default)]
    pub decor: RoomDecor,
}

impl Room {
    /// A new room with the contractor's standard fit-out.
    #[must_use]
    pub fn furnished(kind: RoomKind, floor: i8) -> Self {
        let mut decor = RoomDecor::default();
        for (cell, furniture) in kind.starter() {
            decor.pieces.insert(
                *cell,
                decor::PlacedDecor {
                    furniture: *furniture,
                    display: None,
                },
            );
        }
        decor.wallpaper = match kind {
            RoomKind::Library | RoomKind::Study => Wallpaper::Timber,
            RoomKind::Bedroom => Wallpaper::Floral,
            RoomKind::Basement | RoomKind::GarageBay | RoomKind::Workshop => Wallpaper::Brick,
            RoomKind::Observatory => Wallpaper::Midnight,
            _ => Wallpaper::Plaster,
        };
        decor.flooring = match kind {
            RoomKind::Kitchen | RoomKind::Airlock | RoomKind::Hydroponics => Flooring::Tiles,
            RoomKind::Basement | RoomKind::GarageBay | RoomKind::Workshop => Flooring::Stone,
            RoomKind::Hall | RoomKind::Gallery | RoomKind::Library => Flooring::Parquet,
            _ => Flooring::Planks,
        };
        Self { kind, floor, decor }
    }
}

/// A finished home.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Home {
    pub blueprint: Blueprint,
    /// Every room, by floor then left to right.
    pub rooms: Vec<Room>,
    /// The house's footprint now (expansions widen and raise it).
    pub width: i32,
    pub height: i32,
    /// Floors above ground.
    pub floors: u8,
    #[serde(default)]
    pub basement: bool,
    #[serde(default)]
    pub lift: bool,
    /// 1 (it stands) to 5 (a masterpiece).
    pub quality: u8,
    /// Who built it (`None`: the guild).
    #[serde(default)]
    pub builder: Option<CharacterId>,
    /// What the chests and wardrobes hold (one store for the house).
    #[serde(default)]
    pub storage: ItemContainer,
    /// World time the keys were handed over.
    pub finished_millis: u64,
}

impl Home {
    /// A home built from `blueprint` with quality `quality`.
    #[must_use]
    pub fn built(
        blueprint: Blueprint,
        quality: u8,
        builder: Option<CharacterId>,
        now: u64,
    ) -> Self {
        let (width, height) = blueprint.footprint();
        Self {
            blueprint,
            rooms: blueprint
                .rooms()
                .iter()
                .map(|(floor, kind)| Room::furnished(*kind, *floor))
                .collect(),
            width,
            height,
            floors: blueprint.floors(),
            basement: false,
            lift: false,
            quality,
            builder,
            storage: ItemContainer::new(),
            finished_millis: now,
        }
    }

    /// Whether it has a garage bay.
    #[must_use]
    pub fn has_garage(&self) -> bool {
        self.rooms
            .iter()
            .any(|room| room.kind == RoomKind::GarageBay)
    }

    /// Storage slots its chests and wardrobes hold.
    #[must_use]
    pub fn storage_capacity(&self) -> u32 {
        self.rooms
            .iter()
            .flat_map(|room| room.decor.pieces.values())
            .map(|piece| piece.furniture.storage_slots())
            .sum()
    }

    /// The rooms on `floor`, with their indexes, left to right.
    pub fn rooms_on(&self, floor: i8) -> impl Iterator<Item = (usize, &Room)> + '_ {
        self.rooms
            .iter()
            .enumerate()
            .filter(move |(_, room)| room.floor == floor)
    }

    /// The lowest and highest floors.
    #[must_use]
    pub fn floor_span(&self) -> (i8, i8) {
        let low = self.rooms.iter().map(|room| room.floor).min().unwrap_or(0);
        let high = self.rooms.iter().map(|room| room.floor).max().unwrap_or(0);
        (low, high)
    }

    /// Whether a bed (or anything else to sleep in, like a cloud hammock) stands inside.
    #[must_use]
    pub fn has_bed(&self) -> bool {
        self.rooms.iter().any(|room| {
            room.decor.pieces.values().any(|piece| {
                piece.furniture.service() == Some(crate::home::decor::DecorService::Sleep)
            })
        })
    }

    /// Every displayed trophy.
    pub fn trophies(&self) -> impl Iterator<Item = ItemKind> + '_ {
        self.rooms
            .iter()
            .flat_map(|room| room.decor.pieces.values())
            .filter_map(|piece| piece.display)
    }

    /// Whether any lamp, fire or stove lights a room (windows glow at night).
    #[must_use]
    pub fn lit(&self) -> bool {
        self.rooms.iter().any(|room| room.decor.lit())
    }

    /// Applies a finished expansion.
    pub fn expand(&mut self, expansion: Expansion) {
        match expansion {
            Expansion::Room(kind) => {
                self.rooms.push(Room::furnished(kind, 0));
            }
            Expansion::Garage => {
                self.rooms
                    .insert(0, Room::furnished(RoomKind::GarageBay, 0));
            }
            Expansion::Floor => {
                let floor = i8::try_from(self.floors).unwrap_or(i8::MAX);
                self.rooms.push(Room::furnished(RoomKind::Bedroom, floor));
                self.rooms.push(Room::furnished(RoomKind::Study, floor));
                self.floors = self.floors.saturating_add(1);
                self.height += blueprint::FLOOR_HEIGHT_TILES;
            }
            Expansion::Basement => {
                self.rooms.push(Room::furnished(RoomKind::Basement, -1));
                self.basement = true;
            }
            Expansion::Lift => self.lift = true,
        }
        self.width += expansion.extra_width();
        // Keep floors in walking order (stable: rooms on one floor keep their order).
        self.rooms.sort_by_key(|room| room.floor);
    }
}

/// A plot: the land, its deed, and what stands on it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Plot {
    pub id: PlotId,
    pub site: PlotSite,
    pub deed: Deed,
    /// The contract under way, if any.
    #[serde(default)]
    pub construction: Option<Construction>,
    /// The finished home, if any.
    #[serde(default)]
    pub home: Option<Home>,
    /// Contracts signed on this plot so far (seeds each contract's events).
    #[serde(default)]
    pub contracts: u32,
}

impl Plot {
    /// The column the front door is at (a little right of the house's middle).
    #[must_use]
    pub fn door_column(&self) -> i32 {
        let width = self
            .home
            .as_ref()
            .map(|home| home.width)
            .or_else(|| self.planned_footprint().map(|(width, _)| width))
            .unwrap_or(5);
        self.house_left(width) + width / 2
    }

    /// The leftmost column of a house `width` wide (centred on the plot).
    #[must_use]
    pub const fn house_left(&self, width: i32) -> i32 {
        self.site.left + (self.site.width() - width) / 2
    }

    /// The footprint being built (a new home's blueprint), if a build is under way.
    #[must_use]
    pub fn planned_footprint(&self) -> Option<(i32, i32)> {
        match self.construction.as_ref()?.job {
            HomeJob::Build(blueprint) => Some(blueprint.footprint()),
            HomeJob::Expand(expansion) => self.home.as_ref().map(|home| {
                let height = if expansion == Expansion::Floor {
                    home.height + blueprint::FLOOR_HEIGHT_TILES
                } else {
                    home.height
                };
                (home.width + expansion.extra_width(), height)
            }),
        }
    }

    /// The garage bay's columns, if the home has one (the leftmost room).
    #[must_use]
    pub fn garage_columns(&self) -> Option<(i32, i32)> {
        let home = self.home.as_ref()?;
        if !home.has_garage() {
            return None;
        }
        let left = self.house_left(home.width);
        Some((left, left + blueprint::ROOM_WIDTH_TILES))
    }

    /// Whether `tile` is on the plot's land (its columns, near the ground).
    #[must_use]
    pub const fn at_site(&self, tile: TilePosition) -> bool {
        tile.x >= self.site.left - SITE_REACH_COLUMNS
            && tile.x <= self.site.right() + SITE_REACH_COLUMNS
            && (tile.y - self.site.ground_row).abs() <= SITE_REACH_ROWS
    }
}

/// A contractor's record with one client.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientRecord {
    pub contractor: CharacterId,
    pub player: PlayerId,
    /// Jobs finished for them.
    pub jobs: u32,
}

/// Every plot, deed, home and furniture stock in the world (one shared section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Homes {
    #[serde(default)]
    next_plot: u32,
    #[serde(default)]
    plots: BTreeMap<PlotId, Plot>,
    /// Furniture each player owns but has not placed.
    #[serde(default)]
    furniture: BTreeMap<PlayerId, FurnitureStock>,
    /// Each player's home point (a home with a bed they made theirs).
    #[serde(default)]
    home_points: BTreeMap<PlayerId, PlotId>,
    /// Contractors' memory of the clients they built for.
    #[serde(default)]
    clients: Vec<ClientRecord>,
}

impl Homes {
    /// Whether nothing was ever bought.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plots.is_empty() && self.furniture.is_empty() && self.clients.is_empty()
    }

    pub fn plots(&self) -> impl Iterator<Item = &Plot> + '_ {
        self.plots.values()
    }

    #[must_use]
    pub fn plot(&self, id: PlotId) -> Option<&Plot> {
        self.plots.get(&id)
    }

    /// The plot whose land covers `tile` (see [`Plot::at_site`]).
    #[must_use]
    pub fn plot_at(&self, tile: TilePosition) -> Option<&Plot> {
        self.plots.values().find(|plot| plot.at_site(tile))
    }

    /// Plots overlapping columns `first..=last`.
    pub fn plots_between(&self, first: i32, last: i32) -> impl Iterator<Item = &Plot> + '_ {
        self.plots
            .values()
            .filter(move |plot| plot.site.overlaps_columns(first, last))
    }

    /// The plot sold from listed site `key`, if it was.
    #[must_use]
    pub fn plot_of_site(&self, key: SiteKey) -> Option<&Plot> {
        if key == SiteKey::Surveyed {
            return None;
        }
        self.plots.values().find(|plot| plot.site.key == key)
    }

    /// Plots `player` owns or co-owns.
    pub fn plots_of(&self, player: PlayerId) -> impl Iterator<Item = &Plot> + '_ {
        self.plots
            .values()
            .filter(move |plot| plot.deed.owns(player))
    }

    /// How many plots `player` holds the deed of (co-owned ones do not count).
    #[must_use]
    pub fn deeds_held(&self, player: PlayerId) -> usize {
        self.plots
            .values()
            .filter(|plot| plot.deed.owner == player)
            .count()
    }

    /// Whether nothing hostile may spawn at `tile`: on or near a plot.
    #[must_use]
    pub fn is_safe_zone(&self, tile: TilePosition) -> bool {
        self.plots.values().any(|plot| {
            tile.x >= plot.site.left - PLOT_SAFE_MARGIN
                && tile.x <= plot.site.right() + PLOT_SAFE_MARGIN
                && (tile.y - plot.site.ground_row).abs() <= 12 + PLOT_SAFE_MARGIN
        })
    }

    /// Furniture `player` owns but has not placed.
    #[must_use]
    pub fn furniture_of(&self, player: PlayerId) -> FurnitureStock {
        self.furniture.get(&player).cloned().unwrap_or_default()
    }

    /// How many `furniture` `player` has in stock.
    #[must_use]
    pub fn furniture_count(&self, player: PlayerId, furniture: Furniture) -> u32 {
        self.furniture
            .get(&player)
            .and_then(|stock| stock.get(&furniture))
            .copied()
            .unwrap_or(0)
    }

    /// `player`'s home point, if its home still stands.
    #[must_use]
    pub fn home_point_of(&self, player: PlayerId) -> Option<&Plot> {
        let plot = self.plots.get(self.home_points.get(&player)?)?;
        plot.home.as_ref().filter(|home| home.has_bed())?;
        Some(plot)
    }

    /// Jobs `contractor` finished for `player`.
    #[must_use]
    pub fn jobs_with(&self, contractor: CharacterId, player: PlayerId) -> u32 {
        self.clients
            .iter()
            .find(|record| record.contractor == contractor && record.player == player)
            .map_or(0, |record| record.jobs)
    }

    /// Characters on a contract right now (their plots, in id order).
    pub fn contracted(&self) -> impl Iterator<Item = (PlotId, &Construction)> + '_ {
        self.plots.values().filter_map(|plot| {
            plot.construction
                .as_ref()
                .map(|construction| (plot.id, construction))
        })
    }

    // ----- mutation (crate-internal: only the fixed tick changes homes) -----

    pub(crate) fn insert_plot(&mut self, site: PlotSite, deed: Deed) -> PlotId {
        let id = PlotId(self.next_plot);
        self.next_plot = self.next_plot.saturating_add(1);
        self.plots.insert(
            id,
            Plot {
                id,
                site,
                deed,
                construction: None,
                home: None,
                contracts: 0,
            },
        );
        id
    }

    pub(crate) fn plot_mut(&mut self, id: PlotId) -> Option<&mut Plot> {
        self.plots.get_mut(&id)
    }

    pub(crate) fn plot_ids(&self) -> Vec<PlotId> {
        self.plots.keys().copied().collect()
    }

    pub(crate) fn add_furniture(&mut self, player: PlayerId, furniture: Furniture, count: u32) {
        *self
            .furniture
            .entry(player)
            .or_default()
            .entry(furniture)
            .or_default() += count;
    }

    /// Takes one `furniture` out of `player`'s stock. Returns whether there was one.
    pub(crate) fn take_furniture(&mut self, player: PlayerId, furniture: Furniture) -> bool {
        let Some(stock) = self.furniture.get_mut(&player) else {
            return false;
        };
        let Some(count) = stock.get_mut(&furniture) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            stock.remove(&furniture);
        }
        if stock.is_empty() {
            self.furniture.remove(&player);
        }
        true
    }

    pub(crate) fn set_home_point(&mut self, player: PlayerId, plot: PlotId) {
        self.home_points.insert(player, plot);
    }

    pub(crate) fn record_job(&mut self, contractor: CharacterId, player: PlayerId) {
        if let Some(record) = self
            .clients
            .iter_mut()
            .find(|record| record.contractor == contractor && record.player == player)
        {
            record.jobs = record.jobs.saturating_add(1);
        } else {
            self.clients.push(ClientRecord {
                contractor,
                player,
                jobs: 1,
            });
        }
    }
}

/// A home action (`PlayerCommand::Home`). Validated in the tick against where the actor stands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HomeCommand {
    /// Buy a deed: a listed site (at a land office), or with [`SiteKey::Surveyed`] the free
    /// spot of `size` the actor stands on.
    BuyPlot { site: SiteKey, size: PlotSize },
    /// Sign a contract on a plot you own: `contractor` is a person who takes building work
    /// (`None`: the builders' guild), `supply` hands over hopper blocks for a discount.
    Hire {
        plot: PlotId,
        job: HomeJob,
        contractor: Option<CharacterId>,
        supply: bool,
    },
    /// Pay to bring the finish forward.
    Rush { plot: PlotId },
    /// Answer the open site event: bring what was asked (`pay`: pay for a shortage's stock
    /// instead; for an anecdote, either just nods).
    AnswerEvent { plot: PlotId, pay: bool },
    /// Change who may come in (owner only).
    SetAccess { plot: PlotId, access: HomeAccess },
    /// Add or remove a co-owner (owner only).
    CoOwner {
        plot: PlotId,
        player: PlayerId,
        add: bool,
    },
    /// Buy a piece from the land office catalogue.
    BuyFurniture { furniture: Furniture },
    /// Craft a piece at a workbench in this home (blocks from the hopper plus a few credits).
    CraftFurniture { plot: PlotId, furniture: Furniture },
    /// Place a piece from your stock in room `room` with its leftmost cell at `cell`.
    Place {
        plot: PlotId,
        room: u8,
        cell: u8,
        furniture: Furniture,
    },
    /// Pick up the piece covering `cell` (rugs last) back into your stock (a displayed trophy
    /// goes back into the cargo bay). A crew member who does not own the house moves pieces
    /// but cannot carry them off: they go back to the deed owner's stock.
    PickUp { plot: PlotId, room: u8, cell: u8 },
    /// Cycle the room's wallpaper (`wallpaper`) or floor.
    Restyle {
        plot: PlotId,
        room: u8,
        wallpaper: bool,
    },
    /// Put `item` from the cargo bay in the trophy case at `cell` (swapping what it showed).
    Display {
        plot: PlotId,
        room: u8,
        cell: u8,
        item: ItemKind,
    },
    /// Use the piece at `cell` (a bed sleeps and sets the home point, a stove cooks, a
    /// telescope stargazes).
    UsePiece { plot: PlotId, room: u8, cell: u8 },
    /// Move cargo into the home's storage (`deposit`), or everything stored back into the bay.
    Stash { plot: PlotId, deposit: bool },
    /// Ride the private lift down to your claim (or back up).
    RideLift { plot: PlotId, down: bool },
    /// Climb down the basement hatch into the cellar under the house.
    Hatch { plot: PlotId },
}

/// Why a home action cannot happen.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HomeRefusal {
    NotAtLandOffice,
    NotAtPlot,
    NoSuchPlot,
    SiteTaken,
    TooManyPlots,
    NotOwner,
    NotPermitted,
    CannotAfford,
    /// A surveyed spot: not open ground (underground, in rock or over a gap).
    NotOpenGround,
    /// A surveyed spot: too steep to level.
    TooUneven,
    /// A surveyed spot: in the town, a village, a claim or another plot.
    Occupied,
    /// A blueprint that needs a bigger plot.
    PlotTooSmall,
    /// A blueprint for another setting (the sky villa, the lunar habitat).
    WrongSetting,
    /// Work is already under way.
    Busy,
    /// No home yet (or one already stands, for a new build).
    NoHome,
    AlreadyBuilt,
    /// The contractor is not someone who takes building work (or cannot be found).
    NoContractor,
    /// Nothing to answer, rush or collect.
    NothingToDo,
    /// Short of the blocks or item asked for.
    MissingItems,
    /// No room on the grid there.
    NoSpace,
    NoFurniture,
    /// The expansion cannot be built (too wide, too many floors, already there, missing a
    /// basement or a claim underground).
    CannotExpand,
    /// A bed only works from dusk.
    NotTired,
    /// On foot only (the lift, the hatch).
    NeedsPilot,
    /// The cargo bay or storage has no room.
    Full,
}

impl HomeRefusal {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAtLandOffice => {
                "Deeds are sold at a land office: the town HQ or a village hall."
            }
            Self::NotAtPlot => "Go to the plot first.",
            Self::NoSuchPlot => "That plot doesn't exist.",
            Self::SiteTaken => "Someone already owns that site.",
            Self::TooManyPlots => "You hold as many deeds as the land office allows.",
            Self::NotOwner => "Only the owner can do that.",
            Self::NotPermitted => "The owners haven't given you leave.",
            Self::CannotAfford => "Not enough credits.",
            Self::NotOpenGround => "A plot needs open ground under the sky.",
            Self::TooUneven => "Too steep here to level for a house.",
            Self::Occupied => "Too close to a town, village, claim or another plot.",
            Self::PlotTooSmall => "That plan needs a bigger plot.",
            Self::WrongSetting => "That plan doesn't suit this plot.",
            Self::Busy => "Work is already under way here.",
            Self::NoHome => "There's no house here yet.",
            Self::AlreadyBuilt => "A house already stands here: hire an expansion instead.",
            Self::NoContractor => "They don't take building work.",
            Self::NothingToDo => "Nothing to do here right now.",
            Self::MissingItems => "You're missing what was asked for.",
            Self::NoSpace => "No room there.",
            Self::NoFurniture => "You don't have that piece.",
            Self::CannotExpand => "That can't be added to this house.",
            Self::NotTired => "You're not tired yet: the bed waits for dusk.",
            Self::NeedsPilot => "Climb out of the rig first.",
            Self::Full => "No room left for that.",
        }
    }
}

/// What a home action costs or yields (the tick applies it; the game previews it).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HomeQuote {
    pub credits: u32,
    /// Blocks taken from the hopper.
    pub blocks: u32,
}
