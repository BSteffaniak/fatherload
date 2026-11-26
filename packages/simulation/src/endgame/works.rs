//! The Works: the late goals a big purse still works toward (Wave 13 endgame).
//!
//! Everything here is a **bill**: credits on the space scale *and* space materials from the
//! cargo bay. A post-Heart purse pays the credits of the early goals at once, but the
//! materials are mined in belts and comet tails (four commons can be bought at stations for
//! three times their value, the rare ones cannot), so the purse is a head start, never the
//! finish ([`super::curve`]).
//!
//! | Goal | Where | Credits | Materials | What it does |
//! |---|---|---|---|---|
//! | Ship line tier 3-5 | any yard of that tier | the line's price | [`line_bill`] | (Wave 12) |
//! | Alien tech Mk II / III | alien-run stations | 4x / 8x Mk I | [`mark_bill`] | stronger (see [`ShipEffects`]) |
//! | Starwright Hull | human shipyards (tier 5) | 3,600,000 | 60 iridium, 80 star platinum, 8 voidglass | space damage x0.35 |
//! | Starwright Engine | human shipyards (tier 5) | 4,000,000 | 200 volatile ice, 40 iridium, 20 comet dust | thrust +90 % |
//! | Heartline Drive | human shipyards (tier 5) | 8,000,000 | 24 voidglass, 40 comet dust, 80 iridium | warp range x2, spool /2 |
//! | Halcyon Gate (8 parts) | Halcyon Dock | 1.2-3.2 million a part | rings of common ore, the lens, the keystone | a gate to any known system |
//! | Estates | your alien home | grows with each wing | ore by wing | rooms, floors, a bay; the neighbours call |
//! | Outfitter | trade posts, shipyards | 250,000-1,500,000 | - | paints and a flame |
//!
//! The Starwright line asks for the Hull and Engine lines at tier 5; the Heartline Drive asks
//! for the whole Starwright ship and every faction boss on the hunt list beaten (plus up to
//! [`HEARTLINE_THEME_BOSSES`] theme bosses, as many as the theme tracks registered).

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::{CosmeticItem, FlameColor, PaintColor},
    home::blueprint::{Expansion, RoomKind},
    station::{AlienTech, ShipGear, ShipLine, StationKind, StationOwner, StationProfile},
    terrain::MineralKind,
};

use super::{economy::space_credits, state::Career};

use MineralKind::{
    CobaltBloom, CometDust, Iridium, Kamacite, MeteoricIron, StarPlatinum, Voidglass, VolatileIce,
};

/// A bill of materials (units of each).
pub type Materials = &'static [(MineralKind, u16)];

/// The materials tier `tier` of `line` asks for (none below tier 3).
#[must_use]
pub const fn line_bill(line: ShipLine, tier: u8) -> Materials {
    match (line, tier) {
        (ShipLine::Hull, 3) => &[(MeteoricIron, 90), (Kamacite, 50)],
        (ShipLine::Hull, 4) => &[(Kamacite, 120), (CobaltBloom, 60)],
        (ShipLine::Hull, 5) => &[(StarPlatinum, 30), (Iridium, 14)],
        (ShipLine::Engine, 3) => &[(MeteoricIron, 60), (VolatileIce, 60)],
        (ShipLine::Engine, 4) => &[(VolatileIce, 140), (CobaltBloom, 50)],
        (ShipLine::Engine, 5) => &[(StarPlatinum, 24), (Iridium, 16)],
        (ShipLine::DriveRange, 3) => &[(Kamacite, 60), (VolatileIce, 40)],
        (ShipLine::DriveRange, 4) => &[(CobaltBloom, 70), (VolatileIce, 60)],
        (ShipLine::DriveRange, 5) => &[(StarPlatinum, 20), (CometDust, 6)],
        (ShipLine::Hold, 3) => &[(MeteoricIron, 120)],
        (ShipLine::Hold, 4) => &[(Kamacite, 100), (MeteoricIron, 100)],
        (ShipLine::Hold, 5) => &[(StarPlatinum, 16), (CobaltBloom, 60)],
        (ShipLine::LifeSupport, 3) => &[(VolatileIce, 80)],
        (ShipLine::LifeSupport, 4) => &[(VolatileIce, 120), (CobaltBloom, 30)],
        (ShipLine::LifeSupport, 5) => &[(CometDust, 6), (Iridium, 8)],
        _ => &[],
    }
}

/// The top alien tech mark.
pub const MARK_MAX: u8 = 3;

/// Credits for `tech`'s mark `mark` (2 or 3).
#[must_use]
pub const fn mark_price(tech: AlienTech, mark: u8) -> u32 {
    let times = if mark >= 3 { 8 } else { 4 };
    tech.price().saturating_mul(times)
}

/// The materials `tech`'s mark `mark` (2 or 3) asks for.
#[must_use]
pub const fn mark_bill(tech: AlienTech, mark: u8) -> Materials {
    match (tech, mark >= 3) {
        (AlienTech::PhaseLattice, false) => &[(CobaltBloom, 40), (CometDust, 4)],
        (AlienTech::PhaseLattice, true) => &[(Iridium, 20), (Voidglass, 4), (CometDust, 10)],
        (AlienTech::GravitonSpool, false) => &[(VolatileIce, 80), (CometDust, 6)],
        (AlienTech::GravitonSpool, true) => &[(Voidglass, 6), (CometDust, 12)],
        (AlienTech::XenoLens, false) => &[(CobaltBloom, 30), (StarPlatinum, 8)],
        (AlienTech::XenoLens, true) => &[(StarPlatinum, 24), (Voidglass, 3)],
        (AlienTech::SymbiontWeave, false) => &[(CobaltBloom, 50), (CometDust, 6)],
        (AlienTech::SymbiontWeave, true) => &[(Iridium, 24), (Voidglass, 4), (CometDust, 10)],
    }
}

/// The tongue a station's kin must be spoken in for `mark` (`None`: no tongue needed).
///
/// A Mk II is talked through at Conversant, a Mk III only Fluent: learning the species'
/// language is part of the endgame.
#[must_use]
pub const fn mark_fluency(mark: u8) -> crate::alien::language::Fluency {
    use crate::alien::language::Fluency;
    if mark >= 3 {
        Fluency::Fluent
    } else {
        Fluency::Conversant
    }
}

/// A Starwright part: the best hull and engine line, made to order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum StarwrightPart {
    Hull,
    Engine,
}

impl StarwrightPart {
    pub const ALL: [Self; 2] = [Self::Hull, Self::Engine];

    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Hull => 1,
            Self::Engine => 2,
        }
    }

    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hull => "Starwright Hull",
            Self::Engine => "Starwright Engine",
        }
    }

    /// The ship line that must be at tier 5 first.
    #[must_use]
    pub const fn line(self) -> ShipLine {
        match self {
            Self::Hull => ShipLine::Hull,
            Self::Engine => ShipLine::Engine,
        }
    }

    #[must_use]
    pub const fn price(self) -> u32 {
        match self {
            Self::Hull => space_credits(180_000),
            Self::Engine => space_credits(200_000),
        }
    }

    #[must_use]
    pub const fn bill(self) -> Materials {
        match self {
            Self::Hull => &[(Iridium, 60), (StarPlatinum, 80), (Voidglass, 8)],
            Self::Engine => &[(VolatileIce, 200), (Iridium, 40), (CometDust, 20)],
        }
    }

    /// The shop line.
    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::Hull => "space damage x0.35 (tier 5: x0.5)",
            Self::Engine => "thrust and impulse +90% (tier 5: +60%)",
        }
    }
}

/// The capstone's price and bill.
pub const HEARTLINE_PRICE: u32 = space_credits(400_000);
pub const HEARTLINE_BILL: Materials = &[(Voidglass, 24), (CometDust, 40), (Iridium, 80)];
/// Theme bosses the Heartline asks for at most (as many as the theme tracks registered, up to
/// this many).
pub const HEARTLINE_THEME_BOSSES: usize = 3;

/// Parts of the Halcyon Gate.
pub const GATE_PARTS: u8 = 8;

/// A gate part's name.
#[must_use]
pub const fn gate_part_name(index: u8) -> &'static str {
    match index {
        0 => "Ring Segment I",
        1 => "Ring Segment II",
        2 => "Ring Segment III",
        3 => "Ring Segment IV",
        4 => "Ring Segment V",
        5 => "Ring Segment VI",
        6 => "The Lens",
        _ => "The Keystone",
    }
}

/// A gate part's credits.
#[must_use]
pub const fn gate_part_price(index: u8) -> u32 {
    match index {
        0..=5 => space_credits(60_000),
        6 => space_credits(120_000),
        _ => space_credits(160_000),
    }
}

/// A gate part's materials.
#[must_use]
pub const fn gate_part_bill(index: u8) -> Materials {
    match index {
        0..=5 => &[(MeteoricIron, 400), (Kamacite, 200), (VolatileIce, 100)],
        6 => &[(StarPlatinum, 60), (CometDust, 20)],
        _ => &[(Voidglass, 12), (Iridium, 40)],
    }
}

/// Seconds a ride through the gate takes (a streak like a warp's).
pub const GATE_JUMP_MILLIS: u32 = 6_000;
/// How far from the target's star the gate puts a rig at least (tiles; farther for a star whose
/// approach reaches past it: half again its approach).
pub const GATE_ARRIVAL_TILES: f64 = 30_000.0;

/// The wings an estate on an alien world can grow, in order.
pub const ESTATE_WINGS: [Expansion; 7] = [
    Expansion::Room(RoomKind::Bedroom),
    Expansion::Room(RoomKind::Kitchen),
    Expansion::Garage,
    Expansion::Room(RoomKind::Hydroponics),
    Expansion::Floor,
    Expansion::Room(RoomKind::Gallery),
    Expansion::Room(RoomKind::Observatory),
];

/// The estate's grandest form (every wing raised).
pub const ESTATE_FULL: usize = ESTATE_WINGS.len();

/// Credits for an estate's wing `wing` when `done` wings stand already.
#[must_use]
pub const fn estate_price(wing: Expansion, done: usize) -> u32 {
    #[allow(clippy::cast_possible_truncation, reason = "at most seven wings")]
    let step = done as u32 + 1;
    space_credits(wing.job().price).saturating_mul(3 * step)
}

/// The materials an estate wing asks for.
#[must_use]
pub const fn estate_bill(wing: Expansion) -> Materials {
    match wing {
        Expansion::Floor => &[(Kamacite, 120), (CobaltBloom, 30)],
        Expansion::Garage => &[(MeteoricIron, 80), (VolatileIce, 40)],
        Expansion::Room(RoomKind::Hydroponics) => &[(VolatileIce, 80), (CometDust, 4)],
        Expansion::Room(RoomKind::Observatory) => &[(StarPlatinum, 10), (CobaltBloom, 40)],
        Expansion::Room(RoomKind::Gallery) => &[(Kamacite, 60), (StarPlatinum, 4)],
        _ => &[(MeteoricIron, 60), (Kamacite, 30)],
    }
}

/// What the outfitter sells (space-only cosmetics, never at village vendors).
pub const OUTFITTER: [(CosmeticItem, u32); 5] = [
    (
        CosmeticItem::Paint(PaintColor::Aurora),
        space_credits(12_500),
    ),
    (
        CosmeticItem::Paint(PaintColor::Solar),
        space_credits(12_500),
    ),
    (
        CosmeticItem::Paint(PaintColor::Corsair),
        space_credits(20_000),
    ),
    (
        CosmeticItem::Paint(PaintColor::Voidblack),
        space_credits(50_000),
    ),
    (CosmeticItem::Flame(FlameColor::Ion), space_credits(75_000)),
];

/// Whether `item` is the outfitter's (space) stock.
#[must_use]
pub fn is_outfitter_stock(item: CosmeticItem) -> bool {
    OUTFITTER.iter().any(|(stock, _)| *stock == item)
}

/// Whether `station` keeps an outfitter (crewed human trade posts and shipyards, and Halcyon).
#[must_use]
pub fn has_outfitter(station: &StationProfile) -> bool {
    station.owner == StationOwner::Human
        && (station.halcyon
            || matches!(station.kind, StationKind::TradePost | StationKind::Shipyard))
}

/// Whether `station` builds Starwright parts and the Heartline Drive (a yard of every tier
/// run by people).
#[must_use]
pub fn builds_starwright(station: &StationProfile) -> bool {
    station.owner == StationOwner::Human && station.yard_tier >= crate::station::shipyard::LINE_MAX
}

/// What the ship's gear and the endgame's upgrades do together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipEffects {
    /// Share of space and hazard damage the hull takes.
    pub damage_scale: f32,
    /// Thrust and impulse scale in space.
    pub engine_scale: f32,
    /// Warp range scale (a warp costs its fuel over this).
    pub range_scale: f32,
    /// Millis the warp spools.
    pub spool_millis: u64,
    /// Re-entry shield tiers the phase lattice adds.
    pub lattice_tiers: u8,
    /// Scanner reach factor (the xeno lens).
    pub lens: f32,
    /// Hull the weave knits a second in space, and up to what share (0: no weave).
    pub weave_rate: f32,
    pub weave_share: f32,
    /// Seconds of air life support adds.
    pub extra_air: f32,
}

/// Damage share of the Starwright Hull.
pub const STARWRIGHT_DAMAGE: f32 = 0.35;
/// Engine scale of the Starwright Engine.
pub const STARWRIGHT_ENGINE: f32 = 1.9;

/// The effects of `gear` with `career`'s endgame upgrades.
#[must_use]
pub fn ship_effects(gear: &ShipGear, career: Option<&Career>) -> ShipEffects {
    let default = Career::default();
    let career = career.unwrap_or(&default);
    let mark = |tech: AlienTech| {
        if gear.has(tech) {
            career.mark(tech).min(MARK_MAX)
        } else {
            0
        }
    };
    let mut damage = gear.damage_scale();
    let mut engine = gear.engine_scale();
    if career.has_starwright(StarwrightPart::Hull) {
        damage = damage.min(STARWRIGHT_DAMAGE);
    }
    if career.has_starwright(StarwrightPart::Engine) {
        engine = engine.max(STARWRIGHT_ENGINE);
    }
    let mut range = gear.drive_range_scale();
    let mut spool = crate::flight::physics::WARP_SPOOL_MILLIS;
    match mark(AlienTech::GravitonSpool) {
        0 => {}
        level => spool /= u64::from(level) + 1,
    }
    if career.heartline {
        range *= 2.0;
        spool /= 2;
    }
    let (weave_rate, weave_share) = match mark(AlienTech::SymbiontWeave) {
        0 => (0.0, 0.0),
        1 => (
            crate::station::shipyard::WEAVE_HULL_PER_SECOND,
            crate::station::shipyard::WEAVE_HULL_SHARE,
        ),
        2 => (0.9, 0.85),
        _ => (1.2, 1.0),
    };
    ShipEffects {
        damage_scale: damage,
        engine_scale: engine,
        range_scale: range,
        spool_millis: spool,
        lattice_tiers: match mark(AlienTech::PhaseLattice) {
            0 => 0,
            level => level + 1,
        },
        lens: match mark(AlienTech::XenoLens) {
            0 => 1.0,
            1 => 2.0,
            2 => 2.5,
            _ => 3.0,
        },
        weave_rate,
        weave_share,
        extra_air: gear.extra_air(),
    }
}

/// The station-side bill for one Works order: credits and materials.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bill {
    pub credits: u32,
    pub materials: Materials,
}

impl Bill {
    /// Whether `bay` holds every material of the bill.
    #[must_use]
    pub fn materials_in(&self, bay: &crate::item::ItemContainer) -> bool {
        self.materials
            .iter()
            .all(|(mineral, units)| bay.mineral(*mineral) >= u32::from(*units))
    }

    /// Takes the bill's materials out of `bay` (call only when [`Self::materials_in`]).
    pub fn take_from(&self, bay: &mut crate::item::ItemContainer) {
        for (mineral, units) in self.materials {
            bay.remove(*mineral, u32::from(*units));
        }
    }

    /// The bill as a shop line ("60 Iridium, 8 Voidglass").
    #[must_use]
    pub fn materials_line(&self) -> String {
        materials_line(self.materials)
    }
}

/// `materials` as a shop line.
#[must_use]
pub fn materials_line(materials: Materials) -> String {
    materials
        .iter()
        .map(|(mineral, units)| format!("{units} {}", mineral.name()))
        .collect::<Vec<_>>()
        .join(", ")
}
