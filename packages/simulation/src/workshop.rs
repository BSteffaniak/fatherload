//! Layer resources and the deep outposts' workshops (Wave 8, Deep Descent worlds).
//!
//! Every stratum has a layer resource ([`crate::descent::StratumDef::layer_resource`]). On Deep
//! Descent worlds the drill now and then digs one out of a stratum's rock
//! ([`layer_yield`]: about one rock tile in [`LAYER_YIELD_ONE_IN`], decided by a hash of the
//! seed and the tile, so every client agrees) and it pops out as an
//! [`crate::item::ItemKind::Layer`] drop. Layer resources take no cargo slots.
//!
//! The outposts' workshops ([`crate::logistics::LogisticsCommand::Craft`]) turn them into
//! consumables (hull patches, fuel cells, dive charges, blasting charges) and **upgrade parts**:
//! the next tier of a line, fitted for the resources plus [`PART_PRICE_PERCENT`] of its shop
//! price. One recipe per stratum of Descents I-III ([`RECIPES`]), each sold at the outposts of
//! its Descent and every deeper one; Wave 9 appends Descents IV-VI's parts and their deep
//! supplies ([`DeepSupply`], also sold over the deep counters). Wave 10 appends VII-VIII's.
//!
//! **Value curve.** [`layer_resource_worth`] is what one unit is worth in credits (it grows with
//! the Descent value curve and the stratum's place in its Descent): a part recipe saves about
//! its resources' worth over the shop price, and a consumable costs about what its effect
//! would at an outpost.

use crate::{
    depth_progression::descent_value_percent, descent::Stratum, economy::UpgradeKind,
    terrain::TilePosition,
};

/// About one rock tile in this many yields a layer resource when drilled on a Deep Descent
/// world.
pub const LAYER_YIELD_ONE_IN: u64 = 22;

/// Percent of a line's shop price an upgrade part recipe still costs in credits (the layer
/// resources stand in for the rest).
pub const PART_PRICE_PERCENT: u32 = 40;

/// Salt of the layer yield hash.
const YIELD_SALT: u64 = 0x1A7E_5EED_0BAD_C0DE;

/// Whether drilling the rock tile at `position` of a world with `seed` digs out a layer
/// resource (the caller checks that the world has Descents and that the tile was rock).
#[must_use]
#[allow(clippy::cast_sign_loss, reason = "hashing the coordinates' bits")]
pub const fn layer_yield(seed: u64, position: TilePosition) -> bool {
    let mut hash = seed ^ YIELD_SALT;
    hash ^= (position.x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash = hash.rotate_left(27);
    hash ^= (position.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    hash ^= hash >> 31;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 29;
    hash.is_multiple_of(LAYER_YIELD_ONE_IN)
}

/// Credits one unit of `stratum`'s layer resource is worth (the value curve recipes are
/// balanced against).
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "41 strata, 8 Descents")]
pub const fn layer_resource_worth(stratum: Stratum) -> u32 {
    let base = 50 + 12 * stratum.index() as u32;
    base * descent_value_percent(stratum.descent().index() as u8) / 100
}

/// What a recipe makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CraftOutput {
    /// Patches this percent of the hull's capacity on the spot.
    Repair { percent: u32 },
    /// Fills this percent of the tank on the spot.
    Refuel { percent: u32 },
    /// Dive charges into the rack (up to [`crate::logistics::MAX_DIVE_CHARGES`]).
    DiveCharges(u8),
    /// Blasting charges into the bomb rack.
    Bombs(u32),
    /// The next tier of an upgrade line, for [`PART_PRICE_PERCENT`] of its price on top of the
    /// resources (the tier must be on sale and stocked at the outpost).
    Part(UpgradeKind),
    /// Deep supplies into the rig's supply kit (Wave 9: up to [`MAX_SUPPLY_STACK`] of a kind,
    /// [`crate::logistics::Logistics::supplies`]).
    Supply(DeepSupply, u8),
}

// --- Wave 9: deep supplies (Descents IV-VI) ----------------------------------------------------

/// Most of one deep supply a rig's kit holds.
pub const MAX_SUPPLY_STACK: u8 = 9;

/// The deep supplies of Descents IV-VI (Wave 9 progression): consumables crafted from the
/// layer resources and sold at the deep settlements' counters, one for each mechanic that asks
/// for a tool.
///
/// **The economy is the progression track's; the effect is the Descent track's.** A kit holds
/// them ([`crate::logistics::Logistics::supplies`]); the Descent tracks spend one with
/// [`crate::WorldState::take_supply`] when their mechanic uses it (the integrator wires the
/// use action). Append only: a supply's index is its wire and save id.
#[derive(
    Clone, Copy, Debug, serde::Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
pub enum DeepSupply {
    /// The Underground Ocean: a ballast tank to blow or flood, so the submarine sinks or
    /// surfaces at once.
    BallastTank,
    /// The Brine Glaciers: burns off hull frost and keeps the deep cold off for a while.
    FrostHeater,
    /// The Pearl Abyss: a floating flare that lights the black water around it.
    FlareBuoy,
    /// The Megafauna Jungle: a scent lure that draws the predators off (and calms the
    /// herbivores you ride).
    BeastBait,
    /// The Sunforge: a reflective canopy that throws a moving patch of shade (glare and heat).
    ShadeCanopy,
    /// The Time Sludge: a charge that blows a bubble of slow time.
    TimeBubbleBomb,
    /// The Echo Caves: one sonar ping without the Sonar Suite.
    EchoCharge,
    /// The Prism Desert: a lamp filter set that tints the headlamp to open colour-keyed walls.
    LampFilter,
}

impl DeepSupply {
    /// Every deep supply, in kit order.
    pub const ALL: [Self; 8] = [
        Self::BallastTank,
        Self::FrostHeater,
        Self::FlareBuoy,
        Self::BeastBait,
        Self::ShadeCanopy,
        Self::TimeBubbleBomb,
        Self::EchoCharge,
        Self::LampFilter,
    ];

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BallastTank => "Ballast Tank",
            Self::FrostHeater => "Frost Heater",
            Self::FlareBuoy => "Flare Buoy",
            Self::BeastBait => "Beast Bait",
            Self::ShadeCanopy => "Shade Canopy",
            Self::TimeBubbleBomb => "Time-Bubble Bomb",
            Self::EchoCharge => "Echo Charge",
            Self::LampFilter => "Lamp Filter",
        }
    }

    /// `count` of it in words: "1 Ballast Tank", "2 Shade Canopies".
    #[must_use]
    pub fn counted(self, count: u32) -> String {
        if count == 1 {
            return self.name().to_owned();
        }
        match self {
            Self::ShadeCanopy => "Shade Canopies".to_owned(),
            Self::BeastBait => "Beast Bait".to_owned(),
            _ => format!("{}s", self.name()),
        }
    }

    /// What it does, in the counter's words.
    #[must_use]
    pub const fn about(self) -> &'static str {
        match self {
            Self::BallastTank => "Blows when the sea crushes the sub: a surge no current holds",
            Self::FrostHeater => "Burns off hull frost and keeps the deep cold off for a while",
            Self::FlareBuoy => "Dropped when a lure grabs you: lights the water, dazzles anglers",
            Self::BeastBait => "Thrown when a predator hunts you: it chases the bait instead",
            Self::ShadeCanopy => "Throws a patch of shade against the inner sun's glare and heat",
            Self::TimeBubbleBomb => "A time-bubble charge: slows every creature where it bursts",
            Self::EchoCharge => "One sonar ping: the dark cave shows for a moment",
            Self::LampFilter => "Coloured glass a tinted lamp needs: one set lasts ten minutes",
        }
    }

    /// The stratum whose mechanic it answers (its recipe's resource).
    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::BallastTank => Stratum::UndergroundOcean,
            Self::FrostHeater => Stratum::BrineGlaciers,
            Self::FlareBuoy => Stratum::PearlAbyss,
            Self::BeastBait => Stratum::MegafaunaJungle,
            Self::ShadeCanopy => Stratum::TheSunforge,
            Self::TimeBubbleBomb => Stratum::TimeSludge,
            Self::EchoCharge => Stratum::EchoCaves,
            Self::LampFilter => Stratum::PrismDesert,
        }
    }

    /// The Descent (0 = I) whose outposts first sell and craft it: its stratum's.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "8 Descents")]
    pub const fn descent(self) -> u8 {
        self.stratum().descent().index() as u8
    }

    /// The Descent (0 = I) whose outpost counters first sell it: its own, except the two the
    /// Drowned and Frozen asks for before Merrow City's counter (its outpost is the Descent's
    /// fourth stratum): ballast and heaters are stocked from the Hive Gate on.
    #[must_use]
    pub const fn sold_from_descent(self) -> u8 {
        match self {
            Self::BallastTank | Self::FrostHeater => 2,
            _ => self.descent(),
        }
    }

    /// Credits one costs at a Descent I counter before the depth scale
    /// ([`supply_price`]): about what its recipe's resources are worth, so crafting saves a
    /// little and buying saves the digging.
    #[must_use]
    pub const fn base_price(self) -> u32 {
        match self {
            Self::BallastTank => 320,
            Self::FrostHeater => 330,
            Self::FlareBuoy => 200,
            Self::BeastBait => 260,
            Self::ShadeCanopy => 410,
            Self::TimeBubbleBomb => 450,
            Self::EchoCharge => 250,
            Self::LampFilter => 480,
        }
    }

    /// The supply's index in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

// --- Supplies at work (Wave 10 polish) ------------------------------------------------------

/// How long an emergency Ballast Tank blows: the submarine rises at full lift and no current,
/// whirlpool or lure holds it (world milliseconds).
pub const BALLAST_BLOW_MILLIS: u64 = 6_000;
/// A submerged rig at or under this share of its hull blows a Ballast Tank (if it carries one).
pub const BALLAST_BLOW_HULL_SHARE: f32 = 0.3;
/// Extra lift of a blowing Ballast Tank, px/s² (on top of a full trim).
pub const BALLAST_BLOW_LIFT: f32 = 260.0;
/// How long a Flare Buoy burns (world milliseconds).
pub const FLARE_MILLIS: u64 = 45_000;
/// How far a Flare Buoy's light reaches and dazzles lure anglers, tiles.
pub const FLARE_REACH_TILES: f32 = 10.0;
/// How long Beast Bait holds the predators that smell it (world milliseconds).
pub const BAIT_MILLIS: u64 = 25_000;
/// How far Beast Bait's scent carries, tiles: hunting predators this close turn to it.
pub const BAIT_REACH_TILES: f32 = 14.0;
/// How far Beast Bait is thrown, tiles (past the predator, away from the rig).
pub const BAIT_THROW_TILES: i32 = 6;
/// How long a fitted Lamp Filter set of coloured glass lasts (world milliseconds); a
/// coloured lamp needs one ([`DeepSupply::LampFilter`]).
pub const LAMP_FILTER_MILLIS: u64 = 600_000;
/// Flares and baits in the world at once, at most (the oldest go first).
pub const MAX_SUPPLY_LURES: usize = 12;

/// A thrown supply lying in the world: a Flare Buoy's light or Beast Bait's scent.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct SupplyLure {
    /// [`DeepSupply::FlareBuoy`] or [`DeepSupply::BeastBait`].
    pub supply: DeepSupply,
    /// Who threw it.
    pub owner: crate::ids::PlayerId,
    /// Where it lies (absolute tiles).
    pub at: TilePosition,
    /// When it was thrown and when it is spent.
    pub since: crate::ids::SimulationTick,
    pub until: crate::ids::SimulationTick,
}

/// What the spent deep supplies are still doing (Wave 10 polish).
///
/// Shared, saved and replicated in [`crate::logistics::Logistics::supply_effects`]. Every
/// supply in the kit now has an effect, spent automatically where its stratum bites (no new
/// input, so it works for every seat, online and by touch):
///
/// | Supply | Spent when | Effect |
/// |---|---|---|
/// | Ballast Tank | a submerged rig falls to [`BALLAST_BLOW_HULL_SHARE`] of its hull or the abyss's pressure cracks it | an emergency blow: [`BALLAST_BLOW_MILLIS`] of full lift that no current, whirlpool or lure holds |
/// | Frost Heater | hull frost reaches the warning line | burns the frost off (Wave 9) |
/// | Flare Buoy | a lure angler's light catches a rig | a buoy that lights the black water for [`FLARE_MILLIS`]; anglers in its light are dazzled and drag nobody |
/// | Beast Bait | a hunting predator of the Hollow World notices a rig | thrown past the beast: every hunting predator within [`BAIT_REACH_TILES`] leaves the rig for it, [`BAIT_MILLIS`] |
/// | Shade Canopy | the Sunforge's light starts to scorch | cools the rig (Wave 9) |
/// | Time-Bubble Bomb | thrown like any charge | it **is** the far track's time bubble ([`crate::explosive::ExplosiveKind::TimeBubble`]): bought or crafted, it goes to the rig's charges |
/// | Echo Charge | a scanner press in the Echo Caves without the Sonar Suite | one full ping (Wave 9) |
/// | Lamp Filter | the lamp is first tinted (and as a set wears out) | the coloured glass a tinted lamp needs: one set lasts [`LAMP_FILTER_MILLIS`] |
#[derive(Clone, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct SupplyEffects {
    /// Rigs blowing an emergency Ballast Tank, until this tick.
    #[serde(default)]
    pub ballast_blows: std::collections::BTreeMap<crate::ids::PlayerId, crate::ids::SimulationTick>,
    /// Flare buoys and beast bait lying in the world (oldest first).
    #[serde(default)]
    pub lures: Vec<SupplyLure>,
    /// Rigs whose lamp has a Lamp Filter set fitted, until this tick.
    #[serde(default)]
    pub filter_sets: std::collections::BTreeMap<crate::ids::PlayerId, crate::ids::SimulationTick>,
    /// Predators on the scent of bait, with the home each had before (Wave 11 carryover): when
    /// the bait is spent (or the beast is out of its reach) it goes back to that home.
    #[serde(default)]
    pub baited_homes: Vec<BaitedHome>,
}

/// A predator drawn off by Beast Bait and the home it returns to when the bait ends.
#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct BaitedHome {
    /// The predator.
    pub entity: crate::entity::EntityId,
    /// Its home before the bait (the enemy's home tile).
    pub home: (i32, i32),
}

impl SupplyEffects {
    /// Whether nothing is at work.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ballast_blows.is_empty()
            && self.lures.is_empty()
            && self.filter_sets.is_empty()
            && self.baited_homes.is_empty()
    }

    /// Whether `player`'s Ballast Tank is blowing at `now`.
    #[must_use]
    pub fn blowing(&self, player: crate::ids::PlayerId, now: crate::ids::SimulationTick) -> bool {
        self.ballast_blows
            .get(&player)
            .is_some_and(|until| now < *until)
    }

    /// Whether `player`'s lamp has a filter set fitted at `now`.
    #[must_use]
    pub fn filter_fitted(
        &self,
        player: crate::ids::PlayerId,
        now: crate::ids::SimulationTick,
    ) -> bool {
        self.filter_sets
            .get(&player)
            .is_some_and(|until| now < *until)
    }

    /// The live lures of `supply` (spent ones are swept by the tick).
    pub fn lures_of(&self, supply: DeepSupply) -> impl Iterator<Item = &SupplyLure> + '_ {
        self.lures.iter().filter(move |lure| lure.supply == supply)
    }

    /// Forgets a leaving player's blow and filter set (their lures stay until spent).
    pub fn forget(&mut self, player: crate::ids::PlayerId) {
        self.ballast_blows.remove(&player);
        self.filter_sets.remove(&player);
    }
}

/// A supply at work (netcode codes `720 + code`, the polish track's range).
#[derive(
    Clone, Copy, Debug, serde::Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
pub enum SupplyEvent {
    /// A Ballast Tank blew: the submarine surges up.
    BallastBlown,
    /// A Flare Buoy was lit.
    FlareLit,
    /// Beast Bait was thrown and the predators turned to it.
    BaitThrown,
    /// A Lamp Filter set was fitted to the lamp.
    FilterFitted,
    /// A Lamp Filter set wore out with none to replace it: the lamp is white again.
    FilterWornOut,
}

impl SupplyEvent {
    /// Netcode code offset (append only).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// The news line for the seat it concerns.
    #[must_use]
    pub const fn news(self) -> &'static str {
        match self {
            Self::BallastBlown => "Ballast blown: surfacing!",
            Self::FlareLit => "Flare buoy lit: the anglers are dazzled",
            Self::BaitThrown => "Beast bait thrown: the predators turn to it",
            Self::FilterFitted => "Lamp filter glass fitted",
            Self::FilterWornOut => "The lamp filter glass is spent: white light",
        }
    }
}

/// Credits one `supply` costs at an outpost of Descent `outpost_descent` (deeper counters
/// charge more, like every outpost service: [`crate::logistics::scaled_price`]).
#[must_use]
pub const fn supply_price(supply: DeepSupply, outpost_descent: u8) -> u32 {
    crate::logistics::scaled_price(supply.base_price(), outpost_descent)
}

/// Whether an outpost in Descent `outpost_descent` sells `supply` over its counter: from
/// [`DeepSupply::sold_from_descent`] down.
#[must_use]
pub const fn outpost_sells_supply(outpost_descent: u8, supply: DeepSupply) -> bool {
    supply.sold_from_descent() <= outpost_descent
}

/// One workshop recipe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub name: &'static str,
    /// What it is, in the workshop's words.
    pub about: &'static str,
    /// Layer resources it takes (one or two kinds).
    pub inputs: &'static [(Stratum, u32)],
    pub output: CraftOutput,
}

impl Recipe {
    /// The Descent whose outposts first craft it (the deepest of its resources' Descents).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "8 Descents")]
    pub const fn descent(&self) -> u8 {
        let mut deepest = 0_u8;
        let mut index = 0;
        while index < self.inputs.len() {
            let descent = self.inputs[index].0.descent().index() as u8;
            if descent > deepest {
                deepest = descent;
            }
            index += 1;
        }
        deepest
    }

    /// The stratum this recipe is known for (its first resource's).
    #[must_use]
    pub const fn stratum(&self) -> Stratum {
        self.inputs[0].0
    }

    /// What its resources are worth together ([`layer_resource_worth`]).
    #[must_use]
    pub const fn resource_worth(&self) -> u32 {
        let mut total = 0;
        let mut index = 0;
        while index < self.inputs.len() {
            let (stratum, count) = self.inputs[index];
            total += layer_resource_worth(stratum) * count;
            index += 1;
        }
        total
    }
}

/// Every workshop recipe, in workshop order (a recipe's index is its id in
/// [`crate::logistics::LogisticsCommand::Craft`]; append only).
pub const RECIPES: [Recipe; 34] = [
    // Descent I: The Crust.
    Recipe {
        name: "Root-rope Splice",
        about: "Topsoil roots twisted into hull lashings: patches 30% of the hull",
        inputs: &[(Stratum::Topsoil, 5)],
        output: CraftOutput::Repair { percent: 30 },
    },
    Recipe {
        name: "Kiln-fired Radiator Fins",
        about: "Clay fins fired in the outpost's kiln: the next Radiator tier",
        inputs: &[(Stratum::ClayBeds, 6)],
        output: CraftOutput::Part(UpgradeKind::Radiator),
    },
    Recipe {
        name: "Timber-cased Dive Charges",
        about: "Old mine timber packed around a charge: two dive charges",
        inputs: &[(Stratum::GreyStone, 5)],
        output: CraftOutput::DiveCharges(2),
    },
    Recipe {
        name: "Shale Hull Plates",
        about: "Split slate riveted over the plating: the next Hull tier",
        inputs: &[(Stratum::SlateShelves, 6)],
        output: CraftOutput::Part(UpgradeKind::Hull),
    },
    Recipe {
        name: "Basalt Wool Lagging",
        about: "Spun basalt fibre bound with kiln clay: the next Heat Shielding tier",
        inputs: &[(Stratum::BasaltColumns, 6), (Stratum::ClayBeds, 2)],
        output: CraftOutput::Part(UpgradeKind::HeatShield),
    },
    // Descent II: The Old Deep.
    Recipe {
        name: "Ground Lens Array",
        about: "Fault crystal ground into lenses: the next Headlamp tier",
        inputs: &[(Stratum::CrystalFaults, 6)],
        output: CraftOutput::Part(UpgradeKind::Headlamp),
    },
    Recipe {
        name: "Amber Fuel Cells",
        about: "Fossil resin rendered into fuel: fills half the tank",
        inputs: &[(Stratum::FossilOceans, 5)],
        output: CraftOutput::Refuel { percent: 50 },
    },
    Recipe {
        name: "Pearl Pressure Seals",
        about: "Cathedral pearls ground into gaskets: the next Pressure Rating tier",
        inputs: &[(Stratum::PressureCathedrals, 6)],
        output: CraftOutput::Part(UpgradeKind::PressureRating),
    },
    Recipe {
        name: "Isotope Charges",
        about: "Hot isotopes in lead casings: four blasting charges",
        inputs: &[(Stratum::RadioactiveHollow, 5)],
        output: CraftOutput::Bombs(4),
    },
    Recipe {
        name: "Relic Drill Teeth",
        about: "Builder relics set with crystal: the next Drill tier",
        inputs: &[
            (Stratum::AncientMachineLayer, 6),
            (Stratum::CrystalFaults, 2),
        ],
        output: CraftOutput::Part(UpgradeKind::Drill),
    },
    Recipe {
        name: "Void-glass Tank Liner",
        about: "Geode glass that holds more than its size: the next Fuel Tank tier",
        inputs: &[(Stratum::VoidGeodeFields, 6)],
        output: CraftOutput::Part(UpgradeKind::FuelTank),
    },
    Recipe {
        name: "Mantle Engine Core",
        about: "A mantle shard lagged in basalt wool: the next Engine tier",
        inputs: &[(Stratum::MantleStormZone, 6), (Stratum::BasaltColumns, 2)],
        output: CraftOutput::Part(UpgradeKind::Engine),
    },
    // Descent III: The Living Dark.
    Recipe {
        name: "Spore-silk Bandage",
        about: "Fungal silk that knits into torn plating: patches 60% of the hull",
        inputs: &[(Stratum::FungalForest, 5)],
        output: CraftOutput::Repair { percent: 60 },
    },
    Recipe {
        name: "Heartwood Cargo Racks",
        about: "Root heartwood, light and strong: the next Cargo Bay tier",
        inputs: &[(Stratum::RootLabyrinth, 6)],
        output: CraftOutput::Part(UpgradeKind::CargoBay),
    },
    Recipe {
        name: "Royal Jelly Fuel",
        about: "The friendly hive's jelly burns clean: fills 80% of the tank",
        inputs: &[(Stratum::TheHive, 5)],
        output: CraftOutput::Refuel { percent: 80 },
    },
    Recipe {
        name: "Sinew Hull Weave",
        about: "Living sinew woven over shale plates: the next Hull tier",
        inputs: &[(Stratum::FleshCaverns, 6), (Stratum::SlateShelves, 2)],
        output: CraftOutput::Part(UpgradeKind::Hull),
    },
    Recipe {
        name: "Lure-light Lantern",
        about: "A lure fish's light in a crystal globe: the next Headlamp tier",
        inputs: &[(Stratum::GlowSea, 6), (Stratum::CrystalFaults, 2)],
        output: CraftOutput::Part(UpgradeKind::Headlamp),
    },
    // Descent IV: The Drowned and Frozen (Wave 9).
    Recipe {
        name: "Pearl-weighted Ballast Tanks",
        about: "Deep pearls sewn into flood bladders: two ballast tanks for the submarine",
        inputs: &[(Stratum::UndergroundOcean, 5)],
        output: CraftOutput::Supply(DeepSupply::BallastTank, 2),
    },
    Recipe {
        name: "Brine-salt Frost Heaters",
        about: "Brine ice cracked for its salt and lit: two frost heaters",
        inputs: &[(Stratum::BrineGlaciers, 5)],
        output: CraftOutput::Supply(DeepSupply::FrostHeater, 2),
    },
    Recipe {
        name: "Sinter Steam Turbine",
        about: "Geyser sinter lines a turbine that drinks steam: the next Engine tier",
        inputs: &[(Stratum::GeyserFields, 6)],
        output: CraftOutput::Part(UpgradeKind::Engine),
    },
    Recipe {
        name: "Merrow Coral Seals",
        about: "The merrows' coral glass over cathedral pearls: the next Pressure Rating tier",
        inputs: &[(Stratum::MerrowCity, 6), (Stratum::PressureCathedrals, 2)],
        output: CraftOutput::Part(UpgradeKind::PressureRating),
    },
    Recipe {
        name: "Pearl-oil Flare Buoys",
        about: "Abyss pearls pressed for their glowing oil: three flare buoys",
        inputs: &[(Stratum::PearlAbyss, 4)],
        output: CraftOutput::Supply(DeepSupply::FlareBuoy, 3),
    },
    // Descent V: The Hollow World (Wave 9).
    Recipe {
        name: "Sky-amber Tank Liner",
        about: "Inner-sky amber that sets light and tight: the next Fuel Tank tier",
        inputs: &[(Stratum::InnerSky, 6)],
        output: CraftOutput::Part(UpgradeKind::FuelTank),
    },
    Recipe {
        name: "Giant-hide Bait Sacks",
        about: "A beast's hide stuffed with jungle musk: three beast baits",
        inputs: &[(Stratum::MegafaunaJungle, 5)],
        output: CraftOutput::Supply(DeepSupply::BeastBait, 3),
    },
    Recipe {
        name: "Lodestone Keel Plates",
        about: "Plates that remember which way is down: the next Hull tier",
        inputs: &[(Stratum::UpsideDownIsles, 6)],
        output: CraftOutput::Part(UpgradeKind::Hull),
    },
    Recipe {
        name: "Sun-glass Shade Canopy",
        about: "Mirrored sun glass on a folding frame: two shade canopies",
        inputs: &[(Stratum::TheSunforge, 5)],
        output: CraftOutput::Supply(DeepSupply::ShadeCanopy, 2),
    },
    Recipe {
        name: "Sunforged Radiator Core",
        about: "Sun glass fins cooled in geyser sinter: the next Radiator tier",
        inputs: &[(Stratum::TheSunforge, 6), (Stratum::GeyserFields, 2)],
        output: CraftOutput::Part(UpgradeKind::Radiator),
    },
    // Descent VI: The Strange Layers (Wave 9).
    Recipe {
        name: "Mirror-ore Cargo Racks",
        about: "Racks that hold twice what they look like: the next Cargo Bay tier",
        inputs: &[(Stratum::MirrorStrata, 6)],
        output: CraftOutput::Part(UpgradeKind::CargoBay),
    },
    Recipe {
        name: "Shear-stone Drill Crown",
        about: "Shear stone set in builder relics, biting from any side: the next Drill tier",
        inputs: &[
            (Stratum::GravityShear, 6),
            (Stratum::AncientMachineLayer, 2),
        ],
        output: CraftOutput::Part(UpgradeKind::Drill),
    },
    Recipe {
        name: "Data-ore Scanner Core",
        about: "Data ore that still remembers the rock: the next Scanner tier",
        inputs: &[(Stratum::PixelRot, 6)],
        output: CraftOutput::Part(UpgradeKind::Scanner),
    },
    Recipe {
        name: "Chrono-amber Time-Bubble Bombs",
        about: "Slow time trapped in amber, cracked on impact: two time-bubble bombs",
        inputs: &[(Stratum::TimeSludge, 5)],
        output: CraftOutput::Supply(DeepSupply::TimeBubbleBomb, 2),
    },
    Recipe {
        name: "Echo-quartz Charges",
        about: "Quartz that rings when struck: three echo charges",
        inputs: &[(Stratum::EchoCaves, 4)],
        output: CraftOutput::Supply(DeepSupply::EchoCharge, 3),
    },
    Recipe {
        name: "Prism Lamp Filters",
        about: "Prism sand fused into coloured glass: two lamp filter sets",
        inputs: &[(Stratum::PrismDesert, 5)],
        output: CraftOutput::Supply(DeepSupply::LampFilter, 2),
    },
    Recipe {
        name: "Prism-ground Beam Lens",
        about: "Prism sand ground over a fault crystal: the next Headlamp tier",
        inputs: &[(Stratum::PrismDesert, 6), (Stratum::CrystalFaults, 2)],
        output: CraftOutput::Part(UpgradeKind::Headlamp),
    },
];

/// The recipe with id `index`.
#[must_use]
pub fn recipe(index: u8) -> Option<&'static Recipe> {
    RECIPES.get(usize::from(index))
}

/// Whether an outpost in Descent `outpost_descent` crafts `recipe` (its Descent's recipes and
/// every shallower one's).
#[must_use]
pub const fn outpost_crafts(outpost_descent: u8, recipe: &Recipe) -> bool {
    recipe.descent() <= outpost_descent
}

/// The credits a part recipe costs on top of its resources, given the line's shop price.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "checked against u32::MAX")]
pub const fn part_credits(shop_price: u32) -> u32 {
    let credits = shop_price as u64 * PART_PRICE_PERCENT as u64 / 100;
    let rounded = credits.div_ceil(10) * 10;
    if rounded > u32::MAX as u64 {
        u32::MAX
    } else {
        rounded as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stratum_of_descents_one_to_six_has_a_recipe() {
        // Wave 9: Descents IV-VI joined I-III (32 strata).
        for stratum in Stratum::ALL.iter().take(32) {
            assert!(
                RECIPES.iter().any(|recipe| recipe.stratum() == *stratum),
                "{stratum:?} has no recipe"
            );
        }
        for recipe in &RECIPES {
            assert!(!recipe.inputs.is_empty() && recipe.inputs.len() <= 2);
            assert!(recipe.inputs.iter().all(|(_, count)| *count > 0));
            assert!(recipe.descent() <= 5, "{}", recipe.name);
            assert!(outpost_crafts(recipe.descent(), recipe));
            if recipe.descent() > 0 {
                assert!(!outpost_crafts(recipe.descent() - 1, recipe));
            }
        }
    }

    #[test]
    fn every_deep_supply_is_crafted_sold_and_priced_near_its_resources() {
        for supply in DeepSupply::ALL {
            assert_eq!(DeepSupply::ALL[supply.index()], supply);
            let recipes: Vec<&Recipe> = RECIPES
                .iter()
                .filter(|recipe| matches!(recipe.output, CraftOutput::Supply(made, _) if made == supply))
                .collect();
            assert_eq!(recipes.len(), 1, "{supply:?} has one recipe");
            let recipe = recipes[0];
            assert_eq!(recipe.stratum(), supply.stratum(), "{supply:?}");
            assert!((3..=5).contains(&supply.descent()), "{supply:?}");
            // Sold no later than its stratum's Descent, and from there down.
            assert!(supply.sold_from_descent() <= supply.descent());
            assert!(outpost_sells_supply(supply.sold_from_descent(), supply));
            assert!(outpost_sells_supply(7, supply));
            if supply.sold_from_descent() > 0 {
                assert!(!outpost_sells_supply(
                    supply.sold_from_descent() - 1,
                    supply
                ));
            }
            // Crafting saves a little over buying at its own Descent's counter, never more
            // than half.
            let CraftOutput::Supply(_, count) = recipe.output else {
                unreachable!()
            };
            let bought = supply_price(supply, supply.descent()) * u32::from(count);
            let worth = recipe.resource_worth();
            assert!(
                worth * 2 >= bought && worth <= bought * 3 / 2,
                "{supply:?}: resources worth {worth}, buying {count} costs {bought}"
            );
            // Deeper counters charge more.
            assert!(supply_price(supply, 5) > supply_price(supply, 3));
        }
    }

    #[test]
    fn the_deep_recipes_make_valid_parts_and_supplies() {
        let mut parts = std::collections::BTreeSet::new();
        for recipe in RECIPES.iter().skip(17) {
            assert!((3..=5).contains(&recipe.descent()), "{}", recipe.name);
            assert!(!recipe.name.is_empty() && !recipe.about.is_empty());
            match recipe.output {
                CraftOutput::Part(kind) => {
                    // A tiered line the deep counters stock (never a boss's rig system).
                    assert!(kind.rig_system().is_none(), "{}", recipe.name);
                    assert!(
                        crate::depth_progression::outpost_stocks(recipe.descent(), kind, 1),
                        "{}",
                        recipe.name
                    );
                    // One part recipe per line per Descent.
                    assert!(parts.insert((recipe.descent(), kind)), "{}", recipe.name);
                }
                CraftOutput::Supply(_, count) => {
                    assert!(count > 0 && count <= MAX_SUPPLY_STACK);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn layer_worth_climbs_with_depth() {
        let mut last = 0;
        for stratum in Stratum::ALL {
            let worth = layer_resource_worth(stratum);
            assert!(worth > last, "{stratum:?}: {worth} after {last}");
            last = worth;
        }
    }

    #[test]
    fn the_yield_is_rare_and_spread_out() {
        let mut hits = 0;
        for y in 0..200 {
            for x in -100..100 {
                if layer_yield(7, TilePosition { x, y }) {
                    hits += 1;
                }
            }
        }
        let expected = 40_000 / LAYER_YIELD_ONE_IN;
        assert!(
            hits > expected * 8 / 10 && hits < expected * 12 / 10,
            "{hits} hits, about {expected} expected"
        );
        // A different seed digs elsewhere.
        let other = (0..400)
            .filter(|x| {
                layer_yield(8, TilePosition { x: *x, y: 3 })
                    != layer_yield(7, TilePosition { x: *x, y: 3 })
            })
            .count();
        assert!(other > 10);
    }
}
