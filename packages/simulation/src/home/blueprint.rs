//! Blueprints, rooms and expansions: what a contractor can build on a plot.
//!
//! A [`Blueprint`] is a house plan: its footprint on the plot (tiles wide and tall above the
//! ground row), the smallest plot it fits, where it may stand ([`super::PlotSetting`]), its
//! room layout ([`Blueprint::rooms`]: room kinds per floor), the usual build time and price (a
//! [`BuildJob`] the hired contractor turns into a quote) and the building blocks a player may
//! supply for a discount. An [`Expansion`] is a later job on a finished home: one more room, a
//! floor, a garage bay, a basement down into the mine or a private lift to a base below.

use serde::{Deserialize, Serialize};

use super::{PlotSetting, PlotSize};
use crate::people::contractor::BuildJob;

/// A house plan.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Blueprint {
    /// One snug room: a bed by the stove. About a day to build.
    Cabin,
    /// Two rooms and a porch.
    Cottage,
    /// Two floors: living room and kitchen downstairs, bedroom and study up.
    House,
    /// A workshop and garage bay with the living quarters behind.
    WorkshopHome,
    /// A grand hall, a library, a gallery and a garage. About a week to build.
    Manor,
    /// Airy rooms and an observatory, only on a floating island.
    SkyVilla,
    /// A pressurized dome with an airlock, only on the Moon.
    LunarHabitat,
    // Wave 6 content (appended: saved homes keep their plans).
    /// A whitewashed cottage with a sky porch, only on a floating island.
    CloudCottage,
    /// A tall, narrow tower with a lookout on top, only on a floating island.
    Aerie,
    /// A small buried crater dome, only on the Moon.
    CraterDome,
    /// One long timber hall under a single roof: a hearth, a kitchen and a sleeping end.
    Longhouse,
}

/// Where a blueprint may stand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlueprintSite {
    /// On the ground (town, villages, scenic spots, the wilds) or a sky island.
    Anywhere,
    /// Only on a floating sky land.
    Sky,
    /// Only on the Moon.
    Moon,
}

impl Blueprint {
    pub const ALL: [Self; 11] = [
        Self::Cabin,
        Self::Cottage,
        Self::House,
        Self::WorkshopHome,
        Self::Manor,
        Self::SkyVilla,
        Self::LunarHabitat,
        Self::CloudCottage,
        Self::Aerie,
        Self::CraterDome,
        Self::Longhouse,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cabin => "Cabin",
            Self::Cottage => "Cottage",
            Self::House => "House",
            Self::WorkshopHome => "Workshop home",
            Self::Manor => "Manor",
            Self::SkyVilla => "Sky villa",
            Self::LunarHabitat => "Lunar habitat",
            Self::CloudCottage => "Cloud cottage",
            Self::Aerie => "Aerie",
            Self::CraterDome => "Crater dome",
            Self::Longhouse => "Longhouse",
        }
    }

    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Cabin => "One snug room: a bed by the stove. Up in about a day.",
            Self::Cottage => "A sitting room, a bedroom and a porch for watching the sky.",
            Self::House => "Two floors: living room and kitchen below, bedroom and study above.",
            Self::WorkshopHome => {
                "A workshop and a garage bay for the rig, the living quarters behind."
            }
            Self::Manor => "A grand hall, library, gallery and garage. A week of work.",
            Self::SkyVilla => "Open rooms and an observatory, only on a floating island.",
            Self::LunarHabitat => "A pressurized dome with an airlock, only on the Moon.",
            Self::CloudCottage => {
                "A whitewashed cottage with a porch over the drop, only on a floating island."
            }
            Self::Aerie => "Three floors stacked into the wind, a lookout on top. Islands only.",
            Self::CraterDome => "A snug half-buried dome for one, only on the Moon.",
            Self::Longhouse => "One long hall under a single roof: hearth, kitchen and beds.",
        }
    }

    /// Footprint `(width, height)` in tiles: width along the plot, height above the ground row.
    #[must_use]
    pub const fn footprint(self) -> (i32, i32) {
        match self {
            Self::Cabin => (7, 4),
            Self::Cottage => (9, 5),
            Self::House => (10, 7),
            Self::WorkshopHome => (13, 6),
            Self::Manor => (18, 9),
            Self::SkyVilla => (11, 6),
            Self::LunarHabitat => (12, 5),
            Self::CloudCottage => (8, 5),
            Self::Aerie => (7, 10),
            Self::CraterDome => (8, 4),
            Self::Longhouse => (15, 5),
        }
    }

    /// The smallest plot it fits on.
    #[must_use]
    pub const fn min_plot(self) -> PlotSize {
        match self {
            Self::Cabin | Self::Cottage | Self::CloudCottage | Self::Aerie | Self::CraterDome => {
                PlotSize::Small
            }
            Self::House
            | Self::WorkshopHome
            | Self::SkyVilla
            | Self::LunarHabitat
            | Self::Longhouse => PlotSize::Medium,
            Self::Manor => PlotSize::Large,
        }
    }

    #[must_use]
    pub const fn site(self) -> BlueprintSite {
        match self {
            Self::SkyVilla | Self::CloudCottage | Self::Aerie => BlueprintSite::Sky,
            Self::LunarHabitat | Self::CraterDome => BlueprintSite::Moon,
            _ => BlueprintSite::Anywhere,
        }
    }

    /// Whether the plan may stand on a plot with `setting`.
    #[must_use]
    pub const fn allowed_on(self, setting: PlotSetting) -> bool {
        match self.site() {
            BlueprintSite::Anywhere => !matches!(setting, PlotSetting::Moon),
            BlueprintSite::Sky => matches!(setting, PlotSetting::SkyIsland),
            BlueprintSite::Moon => matches!(setting, PlotSetting::Moon),
        }
    }

    /// The usual build time and price before the contractor's own terms.
    #[must_use]
    pub const fn job(self) -> BuildJob {
        match self {
            Self::Cabin => BuildJob::CABIN,
            Self::Cottage => BuildJob {
                tenth_days: 18,
                price: 4_500,
            },
            Self::House => BuildJob::HOUSE,
            Self::WorkshopHome => BuildJob {
                tenth_days: 36,
                price: 11_000,
            },
            Self::Manor => BuildJob::MANOR,
            Self::SkyVilla => BuildJob {
                tenth_days: 40,
                price: 16_000,
            },
            Self::LunarHabitat => BuildJob {
                tenth_days: 50,
                price: 30_000,
            },
            Self::CloudCottage => BuildJob {
                tenth_days: 24,
                price: 8_500,
            },
            Self::Aerie => BuildJob {
                tenth_days: 34,
                price: 13_500,
            },
            Self::CraterDome => BuildJob {
                tenth_days: 30,
                price: 18_000,
            },
            Self::Longhouse => BuildJob {
                tenth_days: 30,
                price: 8_000,
            },
        }
    }

    /// Building blocks the job uses: a player who supplies them pays less.
    #[must_use]
    pub const fn blocks(self) -> u32 {
        match self {
            Self::Cabin => 30,
            Self::Cottage => 50,
            Self::House => 90,
            Self::WorkshopHome => 120,
            Self::Manor => 260,
            Self::SkyVilla => 100,
            Self::LunarHabitat => 150,
            Self::CloudCottage => 60,
            Self::Aerie => 95,
            Self::CraterDome => 80,
            Self::Longhouse => 110,
        }
    }

    /// The rooms, as `(floor, kind)` in walking order: floor 0 is the ground floor, left to
    /// right; floors above follow.
    #[must_use]
    pub const fn rooms(self) -> &'static [(i8, RoomKind)] {
        match self {
            Self::Cabin => &[(0, RoomKind::Main)],
            Self::Cottage => &[(0, RoomKind::Living), (0, RoomKind::Bedroom)],
            Self::House => &[
                (0, RoomKind::Living),
                (0, RoomKind::Kitchen),
                (1, RoomKind::Bedroom),
                (1, RoomKind::Study),
            ],
            Self::WorkshopHome => &[
                (0, RoomKind::GarageBay),
                (0, RoomKind::Workshop),
                (0, RoomKind::Living),
                (1, RoomKind::Bedroom),
            ],
            Self::Manor => &[
                (0, RoomKind::GarageBay),
                (0, RoomKind::Hall),
                (0, RoomKind::Kitchen),
                (0, RoomKind::Library),
                (1, RoomKind::Bedroom),
                (1, RoomKind::Gallery),
                (1, RoomKind::Study),
            ],
            Self::SkyVilla => &[
                (0, RoomKind::Atrium),
                (0, RoomKind::Kitchen),
                (1, RoomKind::Bedroom),
                (1, RoomKind::Observatory),
            ],
            Self::LunarHabitat => &[
                (0, RoomKind::Airlock),
                (0, RoomKind::Living),
                (0, RoomKind::Hydroponics),
                (0, RoomKind::Bunks),
            ],
            Self::CloudCottage => &[(0, RoomKind::Atrium), (0, RoomKind::Bedroom)],
            Self::Aerie => &[
                (0, RoomKind::Living),
                (1, RoomKind::Bedroom),
                (2, RoomKind::Observatory),
            ],
            Self::CraterDome => &[(0, RoomKind::Airlock), (0, RoomKind::Bunks)],
            Self::Longhouse => &[
                (0, RoomKind::Hall),
                (0, RoomKind::Kitchen),
                (0, RoomKind::Bedroom),
            ],
        }
    }

    /// Floors above ground (1 for a single-storey plan).
    #[must_use]
    pub const fn floors(self) -> u8 {
        let rooms = self.rooms();
        let mut top = 0;
        let mut index = 0;
        while index < rooms.len() {
            if rooms[index].0 > top {
                top = rooms[index].0;
            }
            index += 1;
        }
        top.unsigned_abs() + 1
    }
}

/// What a room is for (its look, and what starts in it).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RoomKind {
    /// A cabin's one room.
    Main,
    Living,
    Kitchen,
    Bedroom,
    Study,
    Workshop,
    /// The rig parks here (a home with a bay repairs a rig left in it).
    GarageBay,
    Hall,
    Library,
    Gallery,
    Atrium,
    Observatory,
    Airlock,
    Hydroponics,
    Bunks,
    /// Below ground: a hatch down into the mine, and the private lift when there is one.
    Basement,
}

impl RoomKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Main => "Main room",
            Self::Living => "Living room",
            Self::Kitchen => "Kitchen",
            Self::Bedroom => "Bedroom",
            Self::Study => "Study",
            Self::Workshop => "Workshop",
            Self::GarageBay => "Garage bay",
            Self::Hall => "Great hall",
            Self::Library => "Library",
            Self::Gallery => "Gallery",
            Self::Atrium => "Atrium",
            Self::Observatory => "Observatory",
            Self::Airlock => "Airlock",
            Self::Hydroponics => "Hydroponics",
            Self::Bunks => "Bunk room",
            Self::Basement => "Basement",
        }
    }

    /// Whether the room has outside windows (a basement, an airlock and a garage have none).
    #[must_use]
    pub const fn has_windows(self) -> bool {
        !matches!(self, Self::Basement | Self::Airlock | Self::GarageBay)
    }

    /// The furniture a finished room comes with (the contractor's standard fit-out), as
    /// `(cell, furniture)`.
    #[must_use]
    pub const fn starter(self) -> &'static [(u8, super::Furniture)] {
        use super::Furniture as F;
        match self {
            Self::Main => &[(0, F::Bed), (5, F::Stove), (8 + 3, F::WallLamp)],
            Self::Living => &[(1, F::Sofa), (4, F::Fireplace), (8 + 6, F::Painting)],
            Self::Kitchen => &[(1, F::Stove), (3, F::Table), (8 + 6, F::Shelf)],
            Self::Bedroom => &[(1, F::Bed), (5, F::Wardrobe), (8 + 4, F::WallLamp)],
            Self::Study => &[(2, F::Bookshelf), (5, F::Armchair)],
            Self::Workshop => &[(1, F::Workbench), (5, F::Chest)],
            Self::GarageBay => &[(8 + 4, F::WallLamp)],
            Self::Hall => &[(16 + 3, F::Chandelier), (1, F::Armchair), (6, F::Plant)],
            Self::Library => &[(0, F::Bookshelf), (2, F::Bookshelf), (5, F::Armchair)],
            Self::Gallery => &[
                (2, F::TrophyCase),
                (5, F::TrophyCase),
                (16 + 3, F::Chandelier),
            ],
            Self::Atrium => &[(1, F::Plant), (6, F::Plant), (3, F::Sofa)],
            Self::Observatory => &[(3, F::Telescope)],
            Self::Airlock => &[(6, F::Chest)],
            Self::Hydroponics => &[(0, F::Planter), (2, F::Planter), (4, F::Planter)],
            Self::Bunks => &[(1, F::Bed), (5, F::Stove)],
            Self::Basement => &[(1, F::Chest), (8 + 4, F::WallLamp)],
        }
    }
}

/// A later job on a finished home.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Expansion {
    /// One more room on the ground floor (widens the house by [`ROOM_WIDTH_TILES`]).
    Room(RoomKind),
    /// A new top floor with a bedroom and a study.
    Floor,
    /// A garage bay for the rig on the ground floor.
    Garage,
    /// A basement and a hatch down into the mine below the house.
    Basement,
    /// A private lift from the basement to your claim underground (needs the basement and a
    /// claim of yours under the plot).
    Lift,
}

/// Tiles one extra ground-floor room widens a house by.
pub const ROOM_WIDTH_TILES: i32 = 4;
/// Tiles one extra floor raises a house by.
pub const FLOOR_HEIGHT_TILES: i32 = 3;
/// The most floors a house may have above ground.
pub const MAX_FLOORS: u8 = 4;
/// How far (columns) a claim's beacon may be from the plot's middle for the private lift.
pub const LIFT_REACH_COLUMNS: i32 = 24;

impl Expansion {
    /// The expansions offered at the site board, in order (rooms first).
    pub const OFFERED: [Self; 8] = [
        Self::Room(RoomKind::Bedroom),
        Self::Room(RoomKind::Kitchen),
        Self::Room(RoomKind::Gallery),
        Self::Room(RoomKind::Workshop),
        Self::Floor,
        Self::Garage,
        Self::Basement,
        Self::Lift,
    ];

    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Room(kind) => format!("Add a {}", kind.name().to_lowercase()),
            Self::Floor => "Add a floor".to_owned(),
            Self::Garage => "Add a garage bay".to_owned(),
            Self::Basement => "Dig a basement".to_owned(),
            Self::Lift => "Private lift to your base".to_owned(),
        }
    }

    /// The usual time and price.
    #[must_use]
    pub const fn job(self) -> BuildJob {
        match self {
            Self::Room(_) => BuildJob {
                tenth_days: 6,
                price: 1_800,
            },
            Self::Floor => BuildJob {
                tenth_days: 12,
                price: 4_000,
            },
            Self::Garage => BuildJob {
                tenth_days: 8,
                price: 2_600,
            },
            Self::Basement => BuildJob {
                tenth_days: 10,
                price: 3_200,
            },
            Self::Lift => BuildJob {
                tenth_days: 15,
                price: 6_500,
            },
        }
    }

    #[must_use]
    pub const fn blocks(self) -> u32 {
        match self {
            Self::Room(_) => 20,
            Self::Floor => 40,
            Self::Garage => 30,
            Self::Basement => 25,
            Self::Lift => 35,
        }
    }

    /// Extra width the job adds to the house.
    #[must_use]
    pub const fn extra_width(self) -> i32 {
        match self {
            Self::Room(_) => ROOM_WIDTH_TILES,
            Self::Garage => ROOM_WIDTH_TILES + 1,
            Self::Floor | Self::Basement | Self::Lift => 0,
        }
    }
}

/// What a contract builds: a new home, or an expansion of one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HomeJob {
    Build(Blueprint),
    Expand(Expansion),
}

impl HomeJob {
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Build(blueprint) => blueprint.name().to_owned(),
            Self::Expand(expansion) => expansion.name(),
        }
    }

    /// The usual time and price (`plot`: a new home's grounds add to it).
    #[must_use]
    pub const fn job(self, plot: PlotSize) -> BuildJob {
        match self {
            Self::Build(blueprint) => {
                let base = blueprint.job();
                BuildJob {
                    tenth_days: base.tenth_days + plot.grounds_tenth_days(),
                    price: base.price + plot.grounds_price(),
                }
            }
            Self::Expand(expansion) => expansion.job(),
        }
    }

    #[must_use]
    pub const fn blocks(self) -> u32 {
        match self {
            Self::Build(blueprint) => blueprint.blocks(),
            Self::Expand(expansion) => expansion.blocks(),
        }
    }
}
