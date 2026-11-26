//! Space materials (Wave 13 endgame): what belt rocks and comet tails really give.
//!
//! Wave 12 mined belt ore and comet dust as home minerals (iron, silver, skystone, ruby,
//! platinum, diamond, uranium, mythril) until a track added space items. These are those
//! items: eight minerals appended to [`MineralKind`] (so the cargo bay, every sale counter,
//! the inventory and the drops already know them), never grown in a world's ground
//! ([`crate::worldgen::deposits`] gives them no depth curve).
//!
//! | Material | From | Sells (credits a unit) | Used for |
//! |---|---|---|---|
//! | Meteoric Iron | iron rocks | 40 | ship lines (tier 3), the Halcyon Gate's ring, estates |
//! | Kamacite | nickel rocks | 70 | ship lines (tier 3), the gate's ring, estates |
//! | Volatile Ice | ice rocks | 55 | ship lines (tier 4), the Starwright Engine, the ring |
//! | Cobalt Bloom | cobalt rocks | 160 | ship lines (tier 4), alien tech Mk II, estates |
//! | Star Platinum | platinum rocks (40 a rock) | 1,500 | ship lines (tier 5), the Starwright Hull, the gate's lens |
//! | Iridium | iridium rocks (20 a rock) | 4,000 | tier 5, Mk III, Starwright, the keystone, the Heartline |
//! | Voidglass | voidglass rocks (12 a rock) | 10,000 | Mk III, Starwright, the keystone, the Heartline |
//! | Comet Dust | comet tails (6 a comet) | 5,000 | alien tech marks, the Starwright Engine, the Heartline |
//!
//! Stations sell the four commons (Meteoric Iron, Kamacite, Volatile Ice; Cobalt Bloom at
//! shipyards and outposts) at [`MATERIAL_MARKUP_PERCENT`] of their value, so a big purse can
//! buy its way past the common bills but the rare ones are mined, gathered or salvaged.

use crate::{
    space::AsteroidOre,
    station::{StationKind, StationProfile},
    terrain::MineralKind,
};

/// Every space material, in shop order (commonest first).
pub const SPACE_MATERIALS: [MineralKind; 8] = [
    MineralKind::MeteoricIron,
    MineralKind::Kamacite,
    MineralKind::VolatileIce,
    MineralKind::CobaltBloom,
    MineralKind::StarPlatinum,
    MineralKind::Iridium,
    MineralKind::Voidglass,
    MineralKind::CometDust,
];

/// What a comet's tail gives.
pub const COMET_MATERIAL: MineralKind = MineralKind::CometDust;

/// Percent of a common material's value a station asks for it.
pub const MATERIAL_MARKUP_PERCENT: u32 = 300;

/// Whether `mineral` is a space material.
#[must_use]
pub const fn is_space_material(mineral: MineralKind) -> bool {
    matches!(
        mineral,
        MineralKind::MeteoricIron
            | MineralKind::Kamacite
            | MineralKind::VolatileIce
            | MineralKind::CobaltBloom
            | MineralKind::StarPlatinum
            | MineralKind::Iridium
            | MineralKind::Voidglass
            | MineralKind::CometDust
    )
}

/// The material a belt rock of `ore` gives.
#[must_use]
pub const fn belt_material(ore: AsteroidOre) -> MineralKind {
    match ore {
        AsteroidOre::Iron => MineralKind::MeteoricIron,
        AsteroidOre::Nickel => MineralKind::Kamacite,
        AsteroidOre::Ice => MineralKind::VolatileIce,
        AsteroidOre::Cobalt => MineralKind::CobaltBloom,
        AsteroidOre::Platinum => MineralKind::StarPlatinum,
        AsteroidOre::Iridium => MineralKind::Iridium,
        AsteroidOre::Voidglass => MineralKind::Voidglass,
    }
}

/// Where a space material comes from (the codex line; `None` for a home mineral).
#[must_use]
pub const fn origin(mineral: MineralKind) -> Option<&'static str> {
    Some(match mineral {
        MineralKind::MeteoricIron => "Drilled from iron rocks in any asteroid belt.",
        MineralKind::Kamacite => "Drilled from nickel rocks in asteroid belts.",
        MineralKind::VolatileIce => "Drilled from icy rocks; it fumes in sunlight.",
        MineralKind::CobaltBloom => "Crusts on cobalt rocks, commoner in far belts.",
        MineralKind::StarPlatinum => "Platinum rocks: forty units before one runs dry.",
        MineralKind::Iridium => "Iridium rocks: twenty units a rock, rare near home.",
        MineralKind::Voidglass => "Voidglass rocks: twelve shards a rock, only far out.",
        MineralKind::CometDust => "Swept from a comet's tail: six units a comet.",
        _ => return None,
    })
}

/// What a space material is for (the codex and shop line; `None` for a home mineral).
#[must_use]
pub const fn uses(mineral: MineralKind) -> Option<&'static str> {
    Some(match mineral {
        MineralKind::MeteoricIron | MineralKind::Kamacite => {
            "Ship lines (tier 3), the Halcyon Gate's ring, estates."
        }
        MineralKind::VolatileIce => "Ship lines (tier 4), the Starwright Engine, the gate.",
        MineralKind::CobaltBloom => "Ship lines (tier 4), alien tech Mk II, estates.",
        MineralKind::StarPlatinum => "Ship lines (tier 5), the Starwright Hull, the gate's lens.",
        MineralKind::Iridium => "Tier 5 lines, alien tech Mk III, Starwright, the Heartline.",
        MineralKind::Voidglass => "Alien tech Mk III, Starwright, the keystone, the Heartline.",
        MineralKind::CometDust => "Alien tech marks, the Starwright Engine, the Heartline.",
        _ => return None,
    })
}

/// The space materials `station` sells, with the price a unit (credits).
#[must_use]
pub fn station_stock(station: &StationProfile) -> Vec<(MineralKind, u32)> {
    if !station.market || !station.owner.crewed() {
        return Vec::new();
    }
    let mut stock = vec![
        MineralKind::MeteoricIron,
        MineralKind::Kamacite,
        MineralKind::VolatileIce,
    ];
    if matches!(station.kind, StationKind::Shipyard | StationKind::Outpost)
        || station.yard_tier >= 3
    {
        stock.push(MineralKind::CobaltBloom);
    }
    stock
        .into_iter()
        .map(|mineral| (mineral, material_price(mineral)))
        .collect()
}

/// What a station asks for a unit of `mineral`.
#[must_use]
pub const fn material_price(mineral: MineralKind) -> u32 {
    mineral.value() * MATERIAL_MARKUP_PERCENT / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_belt_ore_and_the_comet_give_their_own_space_material() {
        let ores = [
            AsteroidOre::Iron,
            AsteroidOre::Nickel,
            AsteroidOre::Ice,
            AsteroidOre::Cobalt,
            AsteroidOre::Platinum,
            AsteroidOre::Iridium,
            AsteroidOre::Voidglass,
        ];
        let mut given: Vec<MineralKind> = ores.into_iter().map(belt_material).collect();
        given.push(COMET_MATERIAL);
        given.sort();
        let mut all = SPACE_MATERIALS.to_vec();
        all.sort();
        assert_eq!(given, all, "one material each, none a home mineral");
        for mineral in SPACE_MATERIALS {
            assert!(is_space_material(mineral));
            assert!(origin(mineral).is_some() && uses(mineral).is_some());
            assert!(mineral.value() > 0);
        }
        for mineral in crate::worldgen::deposits::MINERALS {
            assert!(!is_space_material(mineral));
        }
        // Rarer is dearer.
        for pair in SPACE_MATERIALS[3..7].windows(2) {
            assert!(pair[1].value() > pair[0].value());
        }
    }
}
