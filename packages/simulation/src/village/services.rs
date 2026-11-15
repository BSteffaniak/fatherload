//! What village buildings sell.
//!
//! Each building's counter offers a few [`VillageOffer`]s, rolled from the village (so every
//! visit, host and client agree), bought with [`crate::SessionServiceRequest::Village`] and
//! resolved in `world/villages.rs`.

use serde::{Deserialize, Serialize};

use super::{
    Village,
    layout::{BuildingKind, Specialty, VillageBuilding},
};
use crate::{
    cosmetics::CosmeticItem,
    explosive::{EXPLOSIVE_OFFERS, ExplosiveKind},
    people::PeopleRng,
    rig::RigPartKind,
    terrain::MineralKind,
};

const SALT_OFFERS: u64 = 0x7111_A6E5_0201;

/// The service a building's counter provides.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VillageService {
    /// The meeting hall: village news (no trade).
    Hall,
    TradingPost,
    Inn,
    Workshop,
    FuelDepot,
    Armory,
    CosmeticVendor,
    AssayOffice,
    Cartographer,
    Shrine,
    /// A house: residents to talk to.
    Home,
}

impl VillageService {
    #[must_use]
    pub const fn of(kind: BuildingKind) -> Option<Self> {
        Some(match kind {
            BuildingKind::Hall => Self::Hall,
            BuildingKind::House => Self::Home,
            BuildingKind::Market => Self::TradingPost,
            BuildingKind::Inn => Self::Inn,
            BuildingKind::Workshop => Self::Workshop,
            BuildingKind::Depot => Self::FuelDepot,
            BuildingKind::Smithy => Self::Armory,
            BuildingKind::Tailor => Self::CosmeticVendor,
            BuildingKind::Assay => Self::AssayOffice,
            BuildingKind::MapRoom => Self::Cartographer,
            BuildingKind::Shrine => Self::Shrine,
            BuildingKind::Tower | BuildingKind::Farm | BuildingKind::MineHead => return None,
        })
    }
}

/// A shrine blessing (timed, see [`super::Blessing`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum BlessingKind {
    /// Hull damage from every source is cut by [`WARD_DAMAGE_PERCENT`].
    Ward,
    /// Thrusters burn [`THRIFT_FUEL_PERCENT`] of their usual fuel.
    Thrift,
    /// Cargo sells for [`FORTUNE_BONUS_PERCENT`] more, in town and in villages.
    Fortune,
}

impl BlessingKind {
    pub const ALL: [Self; 3] = [Self::Ward, Self::Thrift, Self::Fortune];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ward => "Blessing of the Ward",
            Self::Thrift => "Blessing of Thrift",
            Self::Fortune => "Blessing of Fortune",
        }
    }

    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::Ward => "30% less hull damage",
            Self::Thrift => "thrusters burn 25% less fuel",
            Self::Fortune => "cargo sells for 10% more",
        }
    }
}

/// Percent of hull damage a Ward blessing lets through.
pub const WARD_DAMAGE_PERCENT: u32 = 70;
/// Percent of usual thruster fuel burned under a Thrift blessing.
pub const THRIFT_FUEL_PERCENT: u32 = 75;
/// Extra percent cargo fetches under a Fortune blessing.
pub const FORTUNE_BONUS_PERCENT: u32 = 10;
/// How long a blessing lasts (milliseconds of play).
pub const BLESSING_MILLIS: u64 = 10 * 60 * 1_000;

/// One deal at a counter.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VillageDeal {
    /// Sell the bay's minerals and artifacts at `percent` of the town price (only `only`'s
    /// mineral when set; an assay office).
    SellCargo {
        percent: u32,
        only: Option<MineralKind>,
    },
    /// Fill the tank at `percent` of the town's fuel price.
    Refuel { percent: u32 },
    /// Mend the hull at `percent` of the town's repair price.
    Repair { percent: u32 },
    /// A rig part for the shared parts locker.
    Part(RigPartKind),
    /// A cosmetic the garage keeps locked.
    Cosmetic(CosmeticItem),
    /// A pack of special charges, whatever the town's shack stocks.
    Explosives { kind: ExplosiveKind, count: u32 },
    /// Chart the nearest unknown sites and villages onto your map.
    ChartRegion,
    /// A timed blessing.
    Blessing(BlessingKind),
    /// Buy a round: the regulars point you at the nearest site you do not know of.
    Rumour,
}

/// A deal and its price (credits; percentage deals price per unit when bought).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct VillageOffer {
    pub deal: VillageDeal,
    /// Flat price (0 for deals priced by what they buy: selling, refuelling, repairs).
    pub price: u32,
}

impl VillageOffer {
    const fn flat(deal: VillageDeal, price: u32) -> Self {
        Self { deal, price }
    }

    const fn metered(deal: VillageDeal) -> Self {
        Self { deal, price: 0 }
    }

    /// The menu line ("Rare part: Titan Drill").
    #[must_use]
    pub fn label(&self) -> String {
        match self.deal {
            VillageDeal::SellCargo {
                percent,
                only: None,
            } => format!("Sell cargo ({percent}% of town price)"),
            VillageDeal::SellCargo {
                percent,
                only: Some(mineral),
            } => format!("Sell {} ({percent}% of town price)", mineral.name()),
            VillageDeal::Refuel { percent } => format!("Fill the tank ({percent}% of town price)"),
            VillageDeal::Repair { percent } => format!("Mend the hull ({percent}% of town price)"),
            VillageDeal::Part(part) => format!("Rare part: {}", part.title()),
            VillageDeal::Cosmetic(item) => format!("Garage unlock: {}", item.label()),
            VillageDeal::Explosives { kind, count } => {
                format!("{count} x {}", kind.name())
            }
            VillageDeal::ChartRegion => "Chart the surrounding region".to_owned(),
            VillageDeal::Blessing(kind) => format!("{} ({})", kind.name(), kind.effect()),
            VillageDeal::Rumour => "Buy a round and ask for rumours".to_owned(),
        }
    }
}

/// Rare part prices by part.
#[must_use]
pub const fn part_price(part: RigPartKind) -> u32 {
    match part {
        RigPartKind::NeedleDrill
        | RigPartKind::LightweightEngine
        | RigPartKind::ProspectorScanner
        | RigPartKind::SortedCargoRack => 2_400,
        RigPartKind::BurstEngine
        | RigPartKind::ImpactHull
        | RigPartKind::ThermalHull
        | RigPartKind::HazardScanner
        | RigPartKind::ArmoredCargoBay => 3_800,
        RigPartKind::TitanDrill
        | RigPartKind::ResonanceDrill
        | RigPartKind::HaulerEngine
        | RigPartKind::PressureHull
        | RigPartKind::RelicScanner
        | RigPartKind::CargoBalloon => 6_500,
    }
}

/// The offers of `building` in `village`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per service keeps every counter in one place"
)]
pub fn offers(village: &Village, building: &VillageBuilding) -> Vec<VillageOffer> {
    let Some(service) = VillageService::of(building.kind) else {
        return Vec::new();
    };
    let mut rng = PeopleRng::new(
        u64::from(village.id.0),
        SALT_OFFERS,
        u64::from(building.variant),
        u64::try_from(village.population.characters.len()).unwrap_or(0),
    );
    // Prosperous villages trade a little better.
    let prosperity = u32::from(village.prosperity);
    match service {
        VillageService::Hall | VillageService::Home => Vec::new(),
        VillageService::TradingPost => vec![
            VillageOffer::metered(VillageDeal::SellCargo {
                percent: 80 + prosperity * 2,
                only: None,
            }),
            VillageOffer::metered(VillageDeal::Refuel {
                percent: 140 - prosperity * 4,
            }),
        ],
        VillageService::Inn => vec![
            VillageOffer::flat(VillageDeal::Rumour, 25),
            VillageOffer::metered(VillageDeal::Repair { percent: 150 }),
        ],
        VillageService::Workshop => {
            let mut list = vec![VillageOffer::metered(VillageDeal::Repair {
                percent: 125 - prosperity * 3,
            })];
            if village.specialty == Specialty::RareParts {
                let mut parts = RigPartKind::ALL.to_vec();
                for slot in 0..2 {
                    if parts.is_empty() {
                        break;
                    }
                    let part = parts.remove(rng.below(parts.len()));
                    let _ = slot;
                    list.push(VillageOffer::flat(
                        VillageDeal::Part(part),
                        part_price(part),
                    ));
                }
            }
            list
        }
        VillageService::FuelDepot => vec![
            VillageOffer::metered(VillageDeal::Refuel { percent: 70 }),
            VillageOffer::metered(VillageDeal::Repair { percent: 75 }),
        ],
        VillageService::Armory => {
            let mut kinds: Vec<_> = EXPLOSIVE_OFFERS
                .iter()
                .filter(|offer| offer.kind != ExplosiveKind::Flare)
                .collect();
            let mut list = Vec::new();
            for _ in 0..3 {
                if kinds.is_empty() {
                    break;
                }
                let offer = kinds.remove(rng.below(kinds.len()));
                list.push(VillageOffer::flat(
                    VillageDeal::Explosives {
                        kind: offer.kind,
                        count: offer.count + offer.count.div_ceil(2),
                    },
                    offer.cost,
                ));
            }
            list.sort_by_key(|offer| offer.price);
            list
        }
        VillageService::CosmeticVendor => {
            let mut items: Vec<CosmeticItem> = CosmeticItem::all()
                .into_iter()
                .filter(|item| !item.is_starter())
                .collect();
            let mut list = Vec::new();
            for _ in 0..3 {
                if items.is_empty() {
                    break;
                }
                let item = items.remove(rng.below(items.len()));
                let price = 600 + u32::try_from(rng.below(5)).unwrap_or(0) * 150;
                list.push(VillageOffer::flat(VillageDeal::Cosmetic(item), price));
            }
            list
        }
        VillageService::AssayOffice => {
            let Specialty::MineralBuyer(mineral) = village.specialty else {
                return Vec::new();
            };
            vec![VillageOffer::metered(VillageDeal::SellCargo {
                percent: 150 + prosperity * 4,
                only: Some(mineral),
            })]
        }
        VillageService::Cartographer => vec![VillageOffer::flat(VillageDeal::ChartRegion, 240)],
        VillageService::Shrine => BlessingKind::ALL
            .iter()
            .map(|kind| VillageOffer::flat(VillageDeal::Blessing(*kind), 90 + prosperity * 10))
            .collect(),
    }
}
