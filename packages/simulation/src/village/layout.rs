//! The village grammar: identity (landscape, culture, style, name, specialty, state), residents,
//! and the street layout (buildings around a plaza, a landing pad, decorations, signposts).
//!
//! Everything is rolled from `(seed, village id)` with [`PeopleRng`] streams, so a village is
//! the same on every machine and every visit.

use serde::{Deserialize, Serialize};

use super::{
    Village, VillageId,
    placement::{self, HALL_WIDTH, SurfaceSite, UndergroundSite, VillageSize},
};
use crate::{
    people::{Culture, PeopleRng, Role, VillageProblem, generate_population, names::village_name},
    terrain::MineralKind,
    worldgen::{
        WorldSettings,
        biome::{self, Biome},
        noise::{NOISE_MAX, hash2, line_noise},
    },
};

const SALT_IDENTITY: u64 = 0x7111_A6E5_0101;
const SALT_LAYOUT: u64 = 0x7111_A6E5_0102;
const SALT_LANDSCAPE: u64 = 0x7111_A6E5_0103;
const SALT_RESIDENTS: u64 = 0x7111_A6E5_0104;

/// The [`Village::site`] of every sky town: they stand on no ladder site, and 0 is never a
/// surface site.
pub const SKY_TOWN_SITE: i32 = 0;

/// Surface, underground or up in the sky.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VillageSetting {
    Surface,
    /// A settlement in a hall carved into the strata.
    Underground,
    /// A sky town on a town isle of the cloud archipelago (WP-SKY, V9; see
    /// [`crate::worldgen::sky::towns`]).
    Sky(crate::sky::SkyTownKind),
}

impl VillageSetting {
    /// The sky town kind, for sky towns.
    #[must_use]
    pub const fn sky_town(self) -> Option<crate::sky::SkyTownKind> {
        match self {
            Self::Sky(kind) => Some(kind),
            Self::Surface | Self::Underground => None,
        }
    }

    /// Whether this village stands on open ground (surface villages and sky towns): it has a
    /// landing pad, an open sky over its roofs and a street a rig flies over.
    #[must_use]
    pub const fn is_open_air(self) -> bool {
        !matches!(self, Self::Underground)
    }
}

/// The landscape a surface village stands in: the "biome flavour" that picks its culture.
///
/// On biome worlds (V7) it follows the surface biome; on V6 worlds it is a coarse noise band
/// along the surface, so neighbouring villages often share a flavour (both in
/// [`site_landscape`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Landscape {
    Plains,
    Forest,
    Hills,
    Mesa,
    Scrapland,
    /// Underground settlements.
    Caverns,
    /// Sky towns (the cloud archipelago).
    Clouds,
}

impl Landscape {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plains => "plains",
            Self::Forest => "forest",
            Self::Hills => "hills",
            Self::Mesa => "mesa",
            Self::Scrapland => "scrapland",
            Self::Caverns => "caverns",
            Self::Clouds => "clouds",
        }
    }
}

/// How a village's buildings are built (rendering dresses them by it).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum BuildingStyle {
    /// Plank walls, tin or shingle roofs (Frontier, Riverside).
    Timber,
    /// Dry-stone walls, slate roofs (Highland).
    Stone,
    /// Rounded adobe, flat roofs, vigas (Mesa).
    Adobe,
    /// Corrugated sheets and salvaged panels (Scrapper).
    Scrap,
    /// Rock-cut fronts lit by lamps (Deepfolk).
    Carved,
    /// Canvas walls on brass frames under striped gas-bag roofs (Aeronaut sky towns).
    Skyrig,
    /// White stone with blue-tiled upswept roofs and bells (Cloudborn sky towns).
    Cloister,
}

impl BuildingStyle {
    #[must_use]
    pub const fn of(culture: Culture) -> Self {
        match culture {
            Culture::Frontier | Culture::Riverside => Self::Timber,
            Culture::Highland => Self::Stone,
            Culture::Mesa => Self::Adobe,
            Culture::Scrapper => Self::Scrap,
            Culture::Deepfolk => Self::Carved,
            Culture::Aeronaut => Self::Skyrig,
            Culture::Cloudborn => Self::Cloister,
            // The colony is no village (never laid out); its glass and hull read as the
            // cloister's white stone where a style is asked for.
            Culture::Lunar => Self::Cloister,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Timber => "timber",
            Self::Stone => "stone",
            Self::Adobe => "adobe",
            Self::Scrap => "scrap-metal",
            Self::Carved => "rock-cut",
            Self::Skyrig => "canvas-and-brass",
            Self::Cloister => "white-stone",
        }
    }
}

/// What makes a village worth the trip (plan WP-VILLAGE "services and specialties").
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Specialty {
    /// An assay office pays well over the depot price for one mineral.
    MineralBuyer(MineralKind),
    /// A workshop sells rare rig parts you cannot buy in town.
    RareParts,
    /// A paint-and-trim shop sells cosmetics the garage keeps locked.
    Cosmetics,
    /// A smithy sells special charges cheaper and in bigger packs, whatever the shack stocks.
    Armory,
    /// A fuel depot: cheap fuel and repairs out in the wilds.
    FuelDepot,
    /// A cartographer charts nearby sites and villages onto your map.
    InfoBroker,
    /// A shrine grants timed blessings.
    Shrine,
    /// Sky town (WP-SKY): an airship dock fits aether thrusters.
    SkyDock,
    /// Sky town: a floating market buys sky ore dear and fences smugglers' wares.
    SkyBazaar,
    /// Sky town: a sky monastery blesses travellers and works cloud silver into de-icers.
    SkyCloister,
    /// Sky town: cloud farmers sell condensed fuel cheap and seal pressurized hulls.
    SkyGranary,
}

impl Specialty {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MineralBuyer(_) => "mineral buyer",
            Self::RareParts => "rare parts",
            Self::Cosmetics => "cosmetics",
            Self::Armory => "armory",
            Self::FuelDepot => "fuel depot",
            Self::InfoBroker => "cartographer",
            Self::Shrine => "shrine",
            Self::SkyDock => "airship dock",
            Self::SkyBazaar => "floating market",
            Self::SkyCloister => "sky monastery",
            Self::SkyGranary => "cloud granary",
        }
    }

    /// The specialty of a sky town of `kind`.
    #[must_use]
    pub const fn of_sky_town(kind: crate::sky::SkyTownKind) -> Self {
        match kind {
            crate::sky::SkyTownKind::AirshipDock => Self::SkyDock,
            crate::sky::SkyTownKind::FloatingMarket => Self::SkyBazaar,
            crate::sky::SkyTownKind::Monastery => Self::SkyCloister,
            crate::sky::SkyTownKind::CloudFarm => Self::SkyGranary,
        }
    }

    /// The building the specialty lives in.
    #[must_use]
    pub const fn building(self) -> BuildingKind {
        match self {
            Self::MineralBuyer(_) => BuildingKind::Assay,
            Self::RareParts => BuildingKind::Workshop,
            Self::Cosmetics => BuildingKind::Tailor,
            Self::Armory => BuildingKind::Smithy,
            Self::FuelDepot => BuildingKind::Depot,
            Self::InfoBroker => BuildingKind::MapRoom,
            Self::Shrine => BuildingKind::Shrine,
            Self::SkyDock => BuildingKind::AirshipDock,
            Self::SkyBazaar => BuildingKind::Bazaar,
            Self::SkyCloister => BuildingKind::Monastery,
            Self::SkyGranary => BuildingKind::Granary,
        }
    }
}

/// A building's purpose and look.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum BuildingKind {
    /// The meeting hall at the plaza: the elder, village news.
    Hall,
    House,
    /// The trading post: sells cargo, pumps fuel.
    Market,
    /// Rumours and a round for the house.
    Inn,
    /// Repairs (and, as a specialty, rare parts).
    Workshop,
    /// Fuel depot (specialty): cheap fuel and repairs.
    Depot,
    /// Armory (specialty).
    Smithy,
    /// Cosmetic vendor (specialty).
    Tailor,
    /// Mineral buyer (specialty).
    Assay,
    /// Cartographer (specialty).
    MapRoom,
    /// Blessings (specialty).
    Shrine,
    /// A lookout (decoration, no door).
    Tower,
    /// Fields and a barn (decoration, no door).
    Farm,
    /// An old mine head (decoration, no door).
    MineHead,
    /// Sky town (WP-SKY): moorings, a docked airship and the aether forge (specialty).
    AirshipDock,
    /// Sky town: the floating market's awnings (specialty).
    Bazaar,
    /// Sky town: the monastery and its bell tower (specialty).
    Monastery,
    /// Sky town: the cloud granary with its vapour nets (specialty).
    Granary,
}

impl BuildingKind {
    /// Whether rigs can walk in (it has a door and a counter or residents).
    #[must_use]
    pub const fn enterable(self) -> bool {
        !matches!(self, Self::Tower | Self::Farm | Self::MineHead)
    }

    /// Lot width in tiles.
    #[must_use]
    pub const fn width(self) -> i32 {
        match self {
            Self::Hall => 10,
            Self::House | Self::MineHead => 6,
            Self::Market
            | Self::Inn
            | Self::Depot
            | Self::Smithy
            | Self::Assay
            | Self::Bazaar
            | Self::Granary => 8,
            Self::Workshop | Self::Tailor | Self::MapRoom | Self::Shrine => 7,
            Self::Tower => 4,
            Self::Farm | Self::Monastery => 9,
            Self::AirshipDock => 12,
        }
    }

    /// The building's sign / name plate.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hall => "Meeting Hall",
            Self::House => "House",
            Self::Market => "Trading Post",
            Self::Inn => "Inn",
            Self::Workshop => "Workshop",
            Self::Depot => "Fuel Depot",
            Self::Smithy => "Smithy",
            Self::Tailor => "Paint & Trim",
            Self::Assay => "Assay Office",
            Self::MapRoom => "Map Room",
            Self::Shrine => "Shrine",
            Self::Tower => "Lookout",
            Self::Farm => "Farm",
            Self::MineHead => "Old Mine",
            Self::AirshipDock => "Airship Dock",
            Self::Bazaar => "Floating Market",
            Self::Monastery => "Sky Monastery",
            Self::Granary => "Cloud Granary",
        }
    }

    /// The resident roles that fit behind this building's counter, best first.
    #[must_use]
    pub const fn keeper_roles(self) -> &'static [Role] {
        match self {
            Self::Hall => &[Role::Warden, Role::Storyteller, Role::Scholar],
            Self::Market => &[Role::Trader, Role::Courier, Role::Baker],
            Self::Inn => &[Role::Innkeeper, Role::Brewer, Role::Baker],
            Self::Workshop | Self::Depot => &[Role::Mechanic, Role::Tinkerer, Role::Smith],
            Self::Smithy => &[Role::Smith, Role::Tinkerer, Role::Mechanic],
            Self::Tailor => &[Role::Tinkerer, Role::Trader, Role::Apprentice],
            Self::Assay => &[Role::Prospector, Role::RetiredMiner, Role::Scholar],
            Self::MapRoom => &[Role::Cartographer, Role::Scholar, Role::Courier],
            Self::Shrine => &[Role::ShrineKeeper, Role::Healer, Role::Hermit],
            Self::AirshipDock => &[Role::Aeronaut, Role::Mechanic, Role::StormChaser],
            Self::Bazaar => &[Role::Smuggler, Role::Trader, Role::Courier],
            Self::Monastery => &[Role::Monk, Role::ShrineKeeper, Role::Healer],
            Self::Granary => &[Role::Farmer, Role::Monk, Role::Brewer],
            Self::House | Self::Tower | Self::Farm | Self::MineHead => &[],
        }
    }
}

/// One building on the street.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VillageBuilding {
    pub kind: BuildingKind,
    /// First column of its lot.
    pub first_column: i32,
    /// Lot width in tiles.
    pub width: i32,
    /// Who works the counter (a resident index), if anyone.
    pub keeper: Option<u16>,
    /// Who lives here (resident indices; houses and the hall).
    pub residents: Vec<u16>,
    /// A per-building roll for presentation (roof colour, window count, props).
    pub variant: u32,
}

impl VillageBuilding {
    #[must_use]
    pub const fn enterable(&self) -> bool {
        self.kind.enterable()
    }

    #[must_use]
    pub const fn contains_column(&self, column: i32) -> bool {
        column >= self.first_column && column < self.first_column + self.width
    }

    #[must_use]
    pub const fn last_column(&self) -> i32 {
        self.first_column + self.width - 1
    }
}

/// A decoration's kind.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DecorKind {
    /// Plaza centrepieces.
    Well,
    /// Sky towns: a brass wind vane on a mast (Aeronaut centrepiece).
    WindVane,
    /// Sky towns: a hanging bronze bell (Cloudborn centrepiece).
    Bell,
    /// Sky towns: a mooring post with a coiled line.
    MooringPost,
    /// Sky towns: a string of prayer flags.
    PrayerFlags,
    Fountain,
    Statue,
    Bonfire,
    Totem,
    CrystalCluster,
    /// Street dressing.
    Lamp,
    Tree,
    Cart,
    Crates,
    Fence,
    Banner,
    Mushrooms,
    Cactus,
}

/// A decoration at a column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decor {
    pub kind: DecorKind,
    pub column: i32,
}

/// Where a signpost points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignTarget {
    Town,
    Village(VillageId),
}

/// A signpost at a village edge pointing the way to the next place along the surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signpost {
    pub column: i32,
    /// `-1` points west, `1` east.
    pub direction: i32,
    pub target: SignTarget,
    /// Columns to the target's nearest edge.
    pub distance: i32,
}

/// How two neighbouring villages get on (story input).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VillageRelation {
    Friendly,
    Trading,
    Rival,
}

/// The landscape flavour at column `x` (surface-biome hook; see [`Landscape`]).
///
/// On biome worlds ([`crate::worldgen::GeneratorVersion::V7`]) it follows the surface biome
/// there ([`biome::biome_at`]): plains stay plains, forests and wetlands are forest (riverside
/// timber), mountains and tundra are hills (highland stone), deserts are mesa (adobe) and
/// badlands are mesa or scrapland. Older worlds read a coarse noise band along the surface.
#[must_use]
pub fn site_landscape(settings: &WorldSettings, x: i32) -> Landscape {
    if settings.generator.has_biomes() {
        return match biome::biome_at(settings, x) {
            Biome::Plains => Landscape::Plains,
            Biome::Forest | Biome::Wetlands => Landscape::Forest,
            Biome::Mountains | Biome::Tundra => Landscape::Hills,
            Biome::Desert => Landscape::Mesa,
            Biome::Badlands => {
                if hash2(settings.seed, SALT_LANDSCAPE, x, 1).is_multiple_of(2) {
                    Landscape::Scrapland
                } else {
                    Landscape::Mesa
                }
            }
        };
    }
    let band = line_noise(settings.seed, SALT_LANDSCAPE, x, 1_400);
    let detail = line_noise(settings.seed, SALT_LANDSCAPE ^ 0xFF, x, 500);
    let value = (band * 3 + detail) / 4;
    let step = (NOISE_MAX + 1) / 5;
    match value / step {
        0 => Landscape::Plains,
        1 => Landscape::Forest,
        2 => Landscape::Hills,
        3 => Landscape::Mesa,
        _ => Landscape::Scrapland,
    }
}

fn culture_for(landscape: Landscape, rng: &mut PeopleRng) -> Culture {
    match landscape {
        Landscape::Plains => *rng.weighted(&[(Culture::Frontier, 3), (Culture::Riverside, 2)]),
        Landscape::Forest => *rng.weighted(&[(Culture::Riverside, 3), (Culture::Frontier, 1)]),
        Landscape::Hills => *rng.weighted(&[(Culture::Highland, 4), (Culture::Frontier, 1)]),
        Landscape::Mesa => Culture::Mesa,
        Landscape::Scrapland => *rng.weighted(&[(Culture::Scrapper, 4), (Culture::Frontier, 1)]),
        Landscape::Caverns => Culture::Deepfolk,
        Landscape::Clouds => *rng.weighted(&[(Culture::Aeronaut, 1), (Culture::Cloudborn, 1)]),
    }
}

/// The landscape, culture and name of village `id` (cheap: no residents or layout).
#[must_use]
pub fn identity(settings: &WorldSettings, id: VillageId) -> Option<(String, Culture)> {
    let (landscape, culture) = landscape_and_culture(settings, id)?;
    let _ = landscape;
    Some((village_name(settings.seed, id.0, culture), culture))
}

fn landscape_and_culture(settings: &WorldSettings, id: VillageId) -> Option<(Landscape, Culture)> {
    if placement::decode_sky_town(id).is_some() {
        let site = crate::worldgen::sky::towns::sky_town_of(settings, id)?;
        return Some((Landscape::Clouds, sky_culture(site.kind)));
    }
    let (site, underground) = placement::decode_id(id);
    let surface = placement::surface_site(settings.seed, site)?;
    if underground {
        placement::underground_site(settings, site)?;
        return Some((Landscape::Caverns, Culture::Deepfolk));
    }
    let landscape = site_landscape(settings, surface.center);
    let mut rng = PeopleRng::new(settings.seed, SALT_IDENTITY, u64::from(id.0), 0);
    let culture = culture_for(landscape, &mut rng);
    // On biome worlds the look follows the biome: hill (mountain and tundra) villages are
    // always Highland stone and scrapland camps always Scrapper; older worlds keep their mix.
    let culture = match landscape {
        Landscape::Hills if settings.generator.has_biomes() => Culture::Highland,
        Landscape::Scrapland if settings.generator.has_biomes() => Culture::Scrapper,
        _ => culture,
    };
    Some((landscape, culture))
}

/// The culture of a sky town of `kind`: aeronauts run the docks and markets, the Cloudborn
/// the monasteries and the cloud farms.
#[must_use]
pub const fn sky_culture(kind: crate::sky::SkyTownKind) -> Culture {
    match kind {
        crate::sky::SkyTownKind::AirshipDock | crate::sky::SkyTownKind::FloatingMarket => {
            Culture::Aeronaut
        }
        crate::sky::SkyTownKind::Monastery | crate::sky::SkyTownKind::CloudFarm => {
            Culture::Cloudborn
        }
    }
}

fn pick_specialty(culture: Culture, rng: &mut PeopleRng) -> Specialty {
    let table: &[(u8, u32)] = match culture {
        Culture::Frontier => &[(0, 3), (1, 2), (2, 1), (3, 2), (4, 3), (5, 2), (6, 1)],
        Culture::Highland => &[(0, 2), (1, 1), (2, 1), (3, 1), (4, 1), (5, 3), (6, 3)],
        Culture::Mesa => &[(0, 4), (1, 1), (2, 3), (3, 1), (4, 2), (5, 1), (6, 2)],
        Culture::Riverside => &[(0, 2), (1, 1), (2, 3), (3, 1), (4, 2), (5, 2), (6, 2)],
        Culture::Scrapper => &[(0, 1), (1, 4), (2, 2), (3, 4), (4, 2), (5, 1), (6, 0)],
        Culture::Deepfolk => &[(0, 4), (1, 2), (2, 1), (3, 1), (4, 3), (5, 2), (6, 3)],
        // Sky towns take their specialty from their kind (`build`); this is never asked.
        Culture::Aeronaut | Culture::Cloudborn | Culture::Lunar => &[(1, 1), (2, 1), (4, 1)],
    };
    match *rng.weighted(table) {
        0 => {
            let minerals: &[MineralKind] = if culture == Culture::Deepfolk {
                &[
                    MineralKind::Diamond,
                    MineralKind::Platinum,
                    MineralKind::Uranium,
                    MineralKind::Mythril,
                ]
            } else {
                &[
                    MineralKind::Silver,
                    MineralKind::Gold,
                    MineralKind::Emerald,
                    MineralKind::Ruby,
                    MineralKind::Diamond,
                ]
            };
            Specialty::MineralBuyer(*rng.pick(minerals))
        }
        1 => Specialty::RareParts,
        2 => Specialty::Cosmetics,
        3 => Specialty::Armory,
        4 => Specialty::FuelDepot,
        5 => Specialty::InfoBroker,
        _ => Specialty::Shrine,
    }
}

const fn centrepiece(culture: Culture) -> DecorKind {
    match culture {
        Culture::Frontier => DecorKind::Well,
        Culture::Highland => DecorKind::Statue,
        Culture::Mesa => DecorKind::Fountain,
        Culture::Riverside => DecorKind::Totem,
        Culture::Scrapper => DecorKind::Bonfire,
        Culture::Deepfolk => DecorKind::CrystalCluster,
        Culture::Aeronaut => DecorKind::WindVane,
        Culture::Cloudborn => DecorKind::Bell,
        Culture::Lunar => DecorKind::Lamp,
    }
}

const fn dressing(landscape: Landscape) -> &'static [DecorKind] {
    match landscape {
        Landscape::Plains => &[
            DecorKind::Lamp,
            DecorKind::Cart,
            DecorKind::Fence,
            DecorKind::Tree,
        ],
        Landscape::Forest => &[DecorKind::Tree, DecorKind::Tree, DecorKind::Lamp],
        Landscape::Hills => &[
            DecorKind::Lamp,
            DecorKind::Banner,
            DecorKind::Tree,
            DecorKind::Fence,
        ],
        Landscape::Mesa => &[DecorKind::Cactus, DecorKind::Banner, DecorKind::Crates],
        Landscape::Scrapland => &[DecorKind::Crates, DecorKind::Lamp, DecorKind::Cart],
        Landscape::Caverns => &[
            DecorKind::Mushrooms,
            DecorKind::Lamp,
            DecorKind::CrystalCluster,
        ],
        Landscape::Clouds => &[
            DecorKind::MooringPost,
            DecorKind::PrayerFlags,
            DecorKind::Lamp,
            DecorKind::Crates,
        ],
    }
}

/// Where a village is, before layout.
pub enum SiteSpec {
    Surface {
        site: SurfaceSite,
        ground_row: i32,
    },
    Underground {
        site: UndergroundSite,
        /// The hall's centre column after sliding clear of points of interest.
        center: i32,
    },
    /// A sky town on its town isle (WP-SKY).
    Sky {
        site: crate::worldgen::sky::SkyTownSite,
    },
}

/// Builds a village from its site.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the grammar reads best as one pass from identity to street"
)]
pub fn build(settings: &WorldSettings, spec: &SiteSpec) -> Village {
    let (id, site_index, setting, size, first, last, ground_row) = match *spec {
        SiteSpec::Surface { site, ground_row } => (
            site.id(),
            site.site,
            VillageSetting::Surface,
            site.size,
            site.first,
            site.last,
            ground_row,
        ),
        SiteSpec::Underground { site, center } => (
            site.id(),
            site.site,
            VillageSetting::Underground,
            VillageSize::Hamlet,
            center - HALL_WIDTH / 2,
            center - HALL_WIDTH / 2 + HALL_WIDTH - 1,
            site.floor_row(),
        ),
        // Sky towns have no ladder site: their neighbours are islands, not villages.
        SiteSpec::Sky { site } => (
            site.id,
            SKY_TOWN_SITE,
            VillageSetting::Sky(site.kind),
            site.size,
            site.first,
            site.last,
            site.ground_row,
        ),
    };
    let (landscape, culture) =
        landscape_and_culture(settings, id).unwrap_or((Landscape::Plains, Culture::Frontier));
    let seed = settings.seed;
    let mut rng = PeopleRng::new(seed, SALT_LAYOUT, u64::from(id.0), 0);
    let specialty = match setting {
        VillageSetting::Sky(kind) => Specialty::of_sky_town(kind),
        VillageSetting::Surface | VillageSetting::Underground => pick_specialty(culture, &mut rng),
    };
    let problem = rng.chance(55).then(|| *rng.pick(&VillageProblem::ALL));
    let mut prosperity = 2 + i32::try_from(rng.below(3)).unwrap_or(0);
    if problem.is_none() {
        prosperity += 1;
    }
    if size == VillageSize::Town {
        prosperity += 1;
    }
    let prosperity = u8::try_from(prosperity.clamp(1, 5)).unwrap_or(3);

    // Residents.
    let (fewest, most) = size.residents();
    let mut resident_rng = PeopleRng::new(seed, SALT_RESIDENTS, u64::from(id.0), 0);
    let count = fewest + resident_rng.below(most - fewest + 1);
    let population = generate_population(seed, id, culture, count);

    // Buildings: the hall and market flank the plaza, the specialty and the rest alternate
    // outward, decoration buildings take the far ends.
    let mut core = vec![
        BuildingKind::Hall,
        BuildingKind::Market,
        specialty.building(),
    ];
    if size != VillageSize::Hamlet || setting == VillageSetting::Underground {
        core.push(BuildingKind::Inn);
    }
    let repair_elsewhere = matches!(specialty, Specialty::FuelDepot | Specialty::RareParts);
    if !repair_elsewhere && (size == VillageSize::Town || rng.chance(45)) {
        core.push(BuildingKind::Workshop);
    }
    let fewest_houses = count.div_ceil(3);
    let mut houses = count.div_ceil(2).max(fewest_houses);
    let mut extras: Vec<BuildingKind> = Vec::new();
    if setting == VillageSetting::Surface {
        match landscape {
            Landscape::Plains | Landscape::Forest => {
                if size != VillageSize::Hamlet || rng.chance(50) {
                    extras.push(BuildingKind::Farm);
                }
            }
            Landscape::Hills => extras.push(BuildingKind::Tower),
            Landscape::Mesa => {
                if rng.chance(50) {
                    extras.push(BuildingKind::Tower);
                }
            }
            Landscape::Scrapland => extras.push(BuildingKind::MineHead),
            Landscape::Caverns | Landscape::Clouds => {}
        }
        if culture == Culture::Highland && rng.chance(50) {
            extras.push(BuildingKind::MineHead);
        }
    }
    let plaza_width = match size {
        VillageSize::Hamlet => 10,
        VillageSize::Village => 12,
        VillageSize::Town => 14,
    };
    let pad_width = if setting.is_open_air() { 7 } else { 0 };
    let span = last - first + 1;
    let margin = 1;
    let gap = 2;
    let fits = |core: &[BuildingKind], houses: usize, extras: &[BuildingKind]| {
        let lots: i32 = core
            .iter()
            .chain(extras)
            .map(|kind| kind.width())
            .sum::<i32>()
            + i32::try_from(houses).unwrap_or(0) * BuildingKind::House.width();
        let slots = i32::try_from(core.len() + extras.len() + houses).unwrap_or(0)
            + 1
            + i32::from(pad_width > 0);
        lots + gap * (slots + 1) + plaza_width + pad_width + margin * 2 <= span
    };
    while !fits(&core, houses, &extras) {
        if extras.pop().is_some() {
            continue;
        }
        if houses > fewest_houses {
            houses -= 1;
            continue;
        }
        if core.len() > 3 {
            core.pop();
            continue;
        }
        if houses > 1 {
            houses -= 1;
            continue;
        }
        break;
    }
    // West and east of the plaza: alternate, hall west, market east.
    let mut west: Vec<BuildingKind> = Vec::new();
    let mut east: Vec<BuildingKind> = Vec::new();
    let mut sequence: Vec<BuildingKind> = core.clone();
    sequence.extend(std::iter::repeat_n(BuildingKind::House, houses));
    for (index, kind) in sequence.into_iter().enumerate() {
        if index % 2 == 0 {
            west.push(kind);
        } else {
            east.push(kind);
        }
    }
    for (index, kind) in extras.into_iter().enumerate() {
        if index % 2 == 0 {
            east.push(kind);
        } else {
            west.push(kind);
        }
    }
    let pad_east = rng.chance(50);
    // Street order west to east: [pad?] west.rev() [plaza] east [pad?].
    let mut street: Vec<Slot> = Vec::new();
    if pad_width > 0 && !pad_east {
        street.push(Slot::Pad);
    }
    street.extend(west.iter().rev().map(|kind| Slot::Building(*kind)));
    street.push(Slot::Plaza);
    street.extend(east.iter().map(|kind| Slot::Building(*kind)));
    if pad_width > 0 && pad_east {
        street.push(Slot::Pad);
    }
    let slot_width = |slot: &Slot| match slot {
        Slot::Pad => pad_width,
        Slot::Plaza => plaza_width,
        Slot::Building(kind) => kind.width(),
    };
    let used: i32 = street.iter().map(slot_width).sum();
    let gaps = i32::try_from(street.len()).unwrap_or(1) + 1;
    let mut spare = (span - margin * 2 - used - gap * gaps).max(0);
    let mut gap_widths = vec![gap; usize::try_from(gaps).unwrap_or(1)];
    // Spread the spare columns over the gaps evenly, the remainder from the plaza outward.
    let each = spare / gaps;
    for width in &mut gap_widths {
        *width += each;
    }
    spare -= each * gaps;
    let plaza_index = street
        .iter()
        .position(|slot| matches!(slot, Slot::Plaza))
        .unwrap_or(0);
    let mut offset = 0_usize;
    while spare > 0 {
        let index = if offset.is_multiple_of(2) {
            plaza_index + 1 + offset / 2
        } else {
            plaza_index.saturating_sub(offset / 2)
        };
        if let Some(width) = gap_widths.get_mut(index) {
            *width += 1;
            spare -= 1;
        }
        offset += 1;
        if offset > gap_widths.len() * 2 {
            offset = 0;
        }
    }
    let mut column = first + margin + gap_widths[0];
    let mut buildings: Vec<VillageBuilding> = Vec::new();
    let mut plaza = (column, column);
    let mut landing_pad = None;
    let mut open_gaps: Vec<(i32, i32)> = Vec::new();
    for (index, slot) in street.iter().enumerate() {
        let width = slot_width(slot);
        match slot {
            Slot::Pad => landing_pad = Some((column, column + width - 1)),
            Slot::Plaza => plaza = (column, column + width - 1),
            Slot::Building(kind) => buildings.push(VillageBuilding {
                kind: *kind,
                first_column: column,
                width,
                keeper: None,
                residents: Vec::new(),
                variant: u32::try_from(rng.next_u64() & 0xFFFF_FFFF).unwrap_or(0),
            }),
        }
        column += width;
        let gap_after = gap_widths.get(index + 1).copied().unwrap_or(0);
        if gap_after > 0 {
            open_gaps.push((column, column + gap_after - 1));
        }
        column += gap_after;
    }
    assign_residents(&population, &mut buildings);

    // Decorations: the plaza centrepiece, lamps flanking it, dressing in wide gaps.
    let mut decor = Vec::new();
    let plaza_center = i32::midpoint(plaza.0, plaza.1);
    decor.push(Decor {
        kind: centrepiece(culture),
        column: plaza_center,
    });
    decor.push(Decor {
        kind: DecorKind::Lamp,
        column: plaza.0 + 1,
    });
    decor.push(Decor {
        kind: DecorKind::Lamp,
        column: plaza.1 - 1,
    });
    let dressing = dressing(landscape);
    for &(gap_first, gap_last) in &open_gaps {
        let width = gap_last - gap_first + 1;
        if width >= 3 && rng.chance(70) {
            decor.push(Decor {
                kind: *rng.pick(dressing),
                column: gap_first + width / 2,
            });
        }
    }
    if setting == VillageSetting::Surface {
        decor.push(Decor {
            kind: DecorKind::Banner,
            column: if pad_east { last - 1 } else { first + 1 },
        });
    }
    decor.sort_by_key(|item| (item.column, item.kind));
    decor.dedup_by_key(|item| item.column);

    // Residents loitering outdoors: two to four, at the plaza and by doors.
    let mut loiterers = Vec::new();
    let outdoor = 2 + rng
        .below(3)
        .min(population.characters.len().saturating_sub(2));
    for slot in 0..outdoor.min(population.characters.len()) {
        let resident =
            u16::try_from((slot * 3 + rng.below(2)) % population.characters.len()).unwrap_or(0);
        if loiterers.iter().any(|(who, _)| *who == resident) {
            continue;
        }
        let column = if slot == 0 {
            plaza.0 + 3
        } else if slot == 1 {
            plaza.1 - 3
        } else {
            let building = &buildings[rng.below(buildings.len())];
            building.first_column + building.width / 2 + 1
        };
        loiterers.push((resident, column));
    }

    let signposts = if setting == VillageSetting::Surface {
        signposts(settings, site_index, first, last)
    } else {
        Vec::new()
    };
    let relations = [site_index - 1, site_index + 1]
        .into_iter()
        .filter(|neighbour| setting.sky_town().is_none() && placement::valid_site(*neighbour))
        .map(|neighbour| {
            let other = placement::surface_id(neighbour);
            let (a, b) = if id.0 < other.0 {
                (id, other)
            } else {
                (other, id)
            };
            let mut pair =
                PeopleRng::new(seed, SALT_IDENTITY ^ 0xF00D, u64::from(a.0), u64::from(b.0));
            let relation = *pair.weighted(&[
                (VillageRelation::Friendly, 3),
                (VillageRelation::Trading, 3),
                (VillageRelation::Rival, 2),
            ]);
            (other, relation)
        })
        .collect();

    Village {
        id,
        site: site_index,
        setting,
        name: population.village_name.clone(),
        culture,
        landscape,
        style: BuildingStyle::of(culture),
        size,
        specialty,
        prosperity,
        problem,
        ground_row,
        first_column: first,
        last_column: last,
        plaza,
        landing_pad,
        buildings,
        decor,
        signposts,
        relations,
        population,
        loiterers,
    }
}

enum Slot {
    Pad,
    Plaza,
    Building(BuildingKind),
}

/// Keepers by role (best fit first), then homes: families together, a few to a house; the hall
/// houses the eldest.
fn assign_residents(population: &crate::people::Population, buildings: &mut [VillageBuilding]) {
    let count = population.characters.len();
    let mut keeping = vec![false; count];
    for building in buildings.iter_mut() {
        let roles = building.kind.keeper_roles();
        if roles.is_empty() {
            continue;
        }
        let pick = roles
            .iter()
            .find_map(|role| {
                (0..count)
                    .find(|index| !keeping[*index] && population.characters[*index].role == *role)
            })
            .or_else(|| (0..count).find(|index| !keeping[*index]))
            .or_else(|| (count > 0).then_some(0));
        if let Some(index) = pick {
            if index < count {
                keeping[index] = true;
            }
            building.keeper = u16::try_from(index).ok();
        }
    }
    // Homes: residents sorted by family name (families share a roof), dealt round the houses.
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|a, b| {
        population.characters[*a]
            .name
            .family
            .cmp(&population.characters[*b].name.family)
            .then(a.cmp(b))
    });
    let eldest =
        (0..count).max_by_key(|index| (population.characters[*index].age, usize::MAX - index));
    let houses: Vec<usize> = buildings
        .iter()
        .enumerate()
        .filter(|(_, building)| building.kind == BuildingKind::House)
        .map(|(index, _)| index)
        .collect();
    if let Some(hall) = buildings
        .iter_mut()
        .find(|building| building.kind == BuildingKind::Hall)
        && let Some(eldest) = eldest
    {
        hall.residents.push(u16::try_from(eldest).unwrap_or(0));
    }
    if houses.is_empty() {
        return;
    }
    // The eldest lives at the hall when there is one; everyone else is dealt into the houses
    // in family order, in balanced blocks, so no house stands empty.
    let at_hall = buildings
        .iter()
        .any(|building| building.kind == BuildingKind::Hall)
        .then_some(eldest)
        .flatten();
    order.retain(|resident| Some(*resident) != at_hall);
    let count = order.len().max(1);
    for (position, resident) in order.into_iter().enumerate() {
        let house = houses[(position * houses.len() / count).min(houses.len() - 1)];
        buildings[house]
            .residents
            .push(u16::try_from(resident).unwrap_or(0));
    }
}

/// Columns between the milestones along the trails that join the town and the villages.
pub const MILESTONE_SPACING: i32 = 80;
/// Columns a milestone keeps clear of a village span or the town's flat ground.
const MILESTONE_CLEARANCE: i32 = 24;

/// The trail milestone standing at column `x`, if any.
///
/// The trails between the town and the surface villages carry a post every
/// [`MILESTONE_SPACING`] columns (counted from the town's centre) pointing at the nearest place
/// along the surface, town or village, and how far its nearest edge is. Pure in the seed and
/// the column, so every view and seat agrees; columns in or near a village or the town have
/// none (their own signposts do the job there).
#[must_use]
pub fn milestone_at(settings: &WorldSettings, x: i32) -> Option<Signpost> {
    if !settings.generator.has_villages()
        || (x - placement::TOWN_CENTER_COLUMN).rem_euclid(MILESTONE_SPACING) != 0
    {
        return None;
    }
    let (town_first, town_last) = crate::surface::town_span();
    let town_distance = if x < town_first {
        town_first - x
    } else {
        x - town_last
    };
    if town_distance <= placement::FLATTEN_FADE_COLUMNS + MILESTONE_CLEARANCE
        || placement::surface_site_at(settings.seed, x, MILESTONE_CLEARANCE).is_some()
    {
        return None;
    }
    let mut best = (
        town_distance,
        if x < town_first { 1 } else { -1 },
        SignTarget::Town,
    );
    let near = placement::site_index_near(x);
    for site in near - 1..=near + 1 {
        let Some(found) = placement::surface_site(settings.seed, site) else {
            continue;
        };
        let distance = found.distance(x);
        if distance < best.0 {
            let direction = if found.first > x { 1 } else { -1 };
            best = (distance, direction, SignTarget::Village(found.id()));
        }
    }
    let (distance, direction, target) = best;
    Some(Signpost {
        column: x,
        direction,
        target,
        distance,
    })
}

/// The signposts at a surface village's two edges: west to the next village west (or the
/// town), east likewise.
fn signposts(settings: &WorldSettings, site: i32, first: i32, last: i32) -> Vec<Signpost> {
    let mut posts = Vec::new();
    for direction in [-1, 1] {
        let neighbour = site + direction;
        let column = if direction < 0 { first - 3 } else { last + 3 };
        let (target, distance) = if neighbour == 0 {
            let (town_first, town_last) = crate::surface::town_span();
            let edge = if direction < 0 { town_last } else { town_first };
            (SignTarget::Town, (column - edge).abs())
        } else if let Some(other) = placement::surface_site(settings.seed, neighbour) {
            let edge = if direction < 0 {
                other.last
            } else {
                other.first
            };
            (SignTarget::Village(other.id()), (column - edge).abs())
        } else {
            continue;
        };
        posts.push(Signpost {
            column,
            direction,
            target,
            distance,
        });
    }
    posts
}
