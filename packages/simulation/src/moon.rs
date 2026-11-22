//! The Moon (expansion plan Part II, WP-MOON): the vocabulary of the lunar content.
//!
//! The Moon is one giant landmass hanging still above a fixed column of the same map
//! ([`crate::worldgen::sky::moon`]). On [`GeneratorVersion::V9`] worlds
//! ([`GeneratorVersion::has_moon_content`]) it is filled with content
//! ([`crate::worldgen::moon`]): grey regolith and dark maria, craters, lava tubes to explore on
//! foot, ice fields, alien crystal forests, a mineable rocky underside, helium-3 and alien
//! alloy, abandoned habitats, the lunar research colony, the ancient alien facility (a
//! pilot-only puzzle dungeon with a boss guardian, the Warden) and the strange core at its
//! heart. This module holds the shared data: [`LunarTile`] (the tiles), [`LunarProgress`] (what
//! a player has done on the Moon), the approach checklist ([`MoonRequirement`],
//! [`approach_checklist`]), the colony's offers ([`ColonyOffer`]) and [`MoonCommand`].
//!
//! The world side (vacuum, landing, breathing on foot, the colony counter, the facility's
//! pylons, gate and Warden, the core, lunar enemies and the uplink) lives in `world/moon.rs`.
//!
//! [`GeneratorVersion::V9`]: crate::worldgen::GeneratorVersion::V9
//! [`GeneratorVersion::has_moon_content`]: crate::worldgen::GeneratorVersion::has_moon_content

use serde::{Deserialize, Serialize};

use crate::{
    item::ItemKind,
    item::{BlockKind, ItemContainer},
    player::Player,
    terrain::{ArtifactKind, StrategicResourceKind},
    weapon::WeaponKind,
};

// ---------------------------------------------------------------------------------------------
// Tiles.
// ---------------------------------------------------------------------------------------------

/// A tile of the filled Moon ([`crate::terrain::TileKind::Lunar`], codes `0x700`).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LunarTile {
    /// Bright highland dust: the Moon's grey top.
    Regolith,
    /// Dark basalt dust of the maria (the "seas").
    MareDust,
    /// The Moon's rocky body.
    Rock,
    /// The dense rock of the underside (a long dig from below).
    DeepRock,
    /// Glassy basalt lining the lava tubes.
    Basalt,
    /// Moon ice: water frozen in shadowed ground (melts into premium fuel at the colony).
    Ice,
    /// Regolith rich in helium-3 (fusion fuel; the colony pays well).
    Helium3,
    /// A vein of alien alloy deep in the rock (endgame drill and weapon tiers).
    AlloyVein,
    /// A sealed cache of lunar relics.
    RelicCache,
    /// The trunk and branches of an alien crystal tree.
    CrystalStalk,
    /// A glowing crystal bloom (lights the crystal forests; breaks into crystal lenses).
    CrystalBloom,
    /// The colony's hull plating (indestructible: the colonists would mind).
    ColonyHull,
    /// The colony's dome glass.
    ColonyGlass,
    /// The colony's landing pad.
    ColonyPad,
    /// The wall panels of an abandoned habitat (breakable).
    HabitatPanel,
    /// The alien facility's stone: nothing breaks it.
    FacilityWall,
    /// Facility stone carved with glowing glyphs.
    FacilityGlyph,
    /// An energy veil: a pilot on foot walks through it, a rig cannot.
    Veil,
    /// A dark pylon of the facility's puzzle: strike it with the hand pick to light it.
    Pylon,
    /// A lit pylon.
    PylonLit,
    /// The facility's sealed gate: it opens when every pylon in the puzzle hall is lit.
    Gate,
    /// A power conduit in the Warden's arena: breaking one with the hand pick knocks the
    /// Warden's shield down.
    Conduit,
    /// The seal between the arena and the way down to the core: it falls when the Warden does.
    CoreSeal,
    /// The shell around the Moon's core: no drill, blast or pick gets through it.
    CoreShell,
    /// The strange core at the Moon's heart.
    CoreHeart,
}

impl LunarTile {
    /// Every lunar tile, in code order.
    pub const ALL: [Self; 25] = [
        Self::Regolith,
        Self::MareDust,
        Self::Rock,
        Self::DeepRock,
        Self::Basalt,
        Self::Ice,
        Self::Helium3,
        Self::AlloyVein,
        Self::RelicCache,
        Self::CrystalStalk,
        Self::CrystalBloom,
        Self::ColonyHull,
        Self::ColonyGlass,
        Self::ColonyPad,
        Self::HabitatPanel,
        Self::FacilityWall,
        Self::FacilityGlyph,
        Self::Veil,
        Self::Pylon,
        Self::PylonLit,
        Self::Gate,
        Self::Conduit,
        Self::CoreSeal,
        Self::CoreShell,
        Self::CoreHeart,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Regolith => "regolith",
            Self::MareDust => "mare dust",
            Self::Rock => "lunar rock",
            Self::DeepRock => "deep lunar rock",
            Self::Basalt => "tube basalt",
            Self::Ice => "moon ice",
            Self::Helium3 => "helium-3 regolith",
            Self::AlloyVein => "alien alloy vein",
            Self::RelicCache => "relic cache",
            Self::CrystalStalk => "crystal stalk",
            Self::CrystalBloom => "crystal bloom",
            Self::ColonyHull => "colony hull",
            Self::ColonyGlass => "dome glass",
            Self::ColonyPad => "landing pad",
            Self::HabitatPanel => "habitat panel",
            Self::FacilityWall => "facility stone",
            Self::FacilityGlyph => "glyph stone",
            Self::Veil => "energy veil",
            Self::Pylon => "dark pylon",
            Self::PylonLit => "lit pylon",
            Self::Gate => "sealed gate",
            Self::Conduit => "power conduit",
            Self::CoreSeal => "core seal",
            Self::CoreShell => "core shell",
            Self::CoreHeart => "the core",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`], in the `0x700` family). Never
    /// renumber: saves and digests use it.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Regolith => 0x00,
            Self::MareDust => 0x01,
            Self::Rock => 0x02,
            Self::DeepRock => 0x03,
            Self::Basalt => 0x04,
            Self::Ice => 0x05,
            Self::Helium3 => 0x06,
            Self::AlloyVein => 0x07,
            Self::RelicCache => 0x08,
            Self::CrystalStalk => 0x09,
            Self::CrystalBloom => 0x0A,
            Self::ColonyHull => 0x10,
            Self::ColonyGlass => 0x11,
            Self::ColonyPad => 0x12,
            Self::HabitatPanel => 0x13,
            Self::FacilityWall => 0x20,
            Self::FacilityGlyph => 0x21,
            Self::Veil => 0x22,
            Self::Pylon => 0x23,
            Self::PylonLit => 0x24,
            Self::Gate => 0x25,
            Self::Conduit => 0x26,
            Self::CoreSeal => 0x27,
            Self::CoreShell => 0x30,
            Self::CoreHeart => 0x31,
        }
    }

    /// The tile with code `code`, if any (saves and chunk diffs decode lunar tiles with it).
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        let mut index = 0;
        while index < Self::ALL.len() {
            if Self::ALL[index].code() == code {
                return Some(Self::ALL[index]);
            }
            index += 1;
        }
        None
    }

    /// Whether no drill, blast or pick ever breaks it (the colony, the facility, the core).
    /// The world opens gates and seals itself, by rule.
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::ColonyHull
                | Self::ColonyGlass
                | Self::ColonyPad
                | Self::FacilityWall
                | Self::FacilityGlyph
                | Self::Veil
                | Self::Pylon
                | Self::PylonLit
                | Self::Gate
                | Self::Conduit
                | Self::CoreSeal
                | Self::CoreShell
                | Self::CoreHeart
        )
    }

    /// Drill strength needed to dig it (the classic ladder; the deep rock and alloy want a
    /// late drill). Indestructible tiles report the bedrock hardness.
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::Regolith | Self::MareDust => 1,
            Self::Ice | Self::Helium3 | Self::CrystalBloom => 2,
            Self::Rock | Self::RelicCache | Self::CrystalStalk => 3,
            Self::DeepRock | Self::Basalt | Self::HabitatPanel => 4,
            Self::AlloyVein => 5,
            Self::ColonyHull
            | Self::ColonyGlass
            | Self::ColonyPad
            | Self::FacilityWall
            | Self::FacilityGlyph
            | Self::Veil
            | Self::Pylon
            | Self::PylonLit
            | Self::Gate
            | Self::Conduit
            | Self::CoreSeal
            | Self::CoreShell
            | Self::CoreHeart => crate::terrain::UNBREAKABLE_HARDNESS,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Regolith | Self::MareDust => 2,
            Self::Helium3 | Self::CrystalBloom => 3,
            Self::Ice => 4,
            Self::Rock | Self::CrystalStalk => 7,
            Self::RelicCache | Self::Basalt => 8,
            Self::DeepRock | Self::HabitatPanel => 9,
            Self::AlloyVein => 12,
            Self::ColonyHull
            | Self::ColonyGlass
            | Self::ColonyPad
            | Self::FacilityWall
            | Self::FacilityGlyph
            | Self::Veil
            | Self::Pylon
            | Self::PylonLit
            | Self::Gate
            | Self::Conduit
            | Self::CoreSeal
            | Self::CoreShell
            | Self::CoreHeart => u8::MAX,
        }
    }

    /// The item a broken tile yields (resources, relics, or the classic block of its
    /// hardness).
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::Ice => ItemKind::Material(StrategicResourceKind::MoonIce),
            Self::Helium3 => ItemKind::Material(StrategicResourceKind::Helium3),
            Self::AlloyVein => ItemKind::Material(StrategicResourceKind::AlienAlloy),
            Self::RelicCache => ItemKind::Artifact(ArtifactKind::LunarRelic),
            Self::CrystalBloom => ItemKind::Material(StrategicResourceKind::CrystalLens),
            Self::Regolith | Self::MareDust => ItemKind::Block(BlockKind::Dirt),
            Self::Rock | Self::CrystalStalk => ItemKind::Block(BlockKind::Stone),
            Self::DeepRock | Self::Basalt | Self::HabitatPanel => {
                ItemKind::Block(BlockKind::HardRock)
            }
            _ => return None,
        })
    }

    /// The plain block it breaks into, if it is plain rock or dust.
    #[must_use]
    pub const fn block(self) -> Option<BlockKind> {
        match self.item() {
            Some(ItemKind::Block(block)) => Some(block),
            _ => None,
        }
    }

    /// Whether a pilot on foot passes through it (the facility's veils).
    #[must_use]
    pub const fn pilot_passable(self) -> bool {
        matches!(self, Self::Veil)
    }

    /// Light it gives off (0-255, the renderer's light map): crystal blooms, glyphs, lit
    /// pylons, veils and the core glow.
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::CoreHeart => 255,
            Self::CrystalBloom | Self::PylonLit | Self::Veil => 200,
            Self::FacilityGlyph | Self::Conduit | Self::CoreSeal => 150,
            Self::Gate | Self::Ice => 70,
            _ => 0,
        }
    }

    /// Whether it is part of the colony.
    #[must_use]
    pub const fn is_colony(self) -> bool {
        matches!(self, Self::ColonyHull | Self::ColonyGlass | Self::ColonyPad)
    }

    /// Whether it is part of the alien facility.
    #[must_use]
    pub const fn is_facility(self) -> bool {
        matches!(
            self,
            Self::FacilityWall
                | Self::FacilityGlyph
                | Self::Veil
                | Self::Pylon
                | Self::PylonLit
                | Self::Gate
                | Self::Conduit
                | Self::CoreSeal
        )
    }
}

// ---------------------------------------------------------------------------------------------
// The approach.
// ---------------------------------------------------------------------------------------------

/// Pressurized Hull tiers: one ladder for the sky and the Moon.
///
/// The shop sells it ([`crate::economy::UpgradeKind::PressurizedHull`]), and a cloud farm
/// crafts tier 1 too ([`crate::skycraft::SkyRecipe::PressurizedHull`]). Every tier keeps one cold level out ([`crate::skycraft::PRESSURIZED_HULL_COLD_COVER`]);
/// tier [`VACUUM_SEAL_TIER`] seals the rig against vacuum and tier [`HULL_ARMOUR_TIER`] also
/// turns a fifth of Moon enemies' hits.
pub const PRESSURIZED_HULL_MAX_LEVEL: u8 = 3;
/// The pressurized hull tier that seals a rig against the vacuum of orbit and the Moon (the
/// first tier is the sky towns' cold seal).
pub const VACUUM_SEAL_TIER: u8 = 2;
/// The pressurized hull tier whose void-rated plating turns a fifth of Moon creatures' bites.
pub const HULL_ARMOUR_TIER: u8 = 3;
/// Life Support tiers ([`crate::economy::UpgradeKind::LifeSupport`]): keeps the crew alive in
/// vacuum and runs an air line to the pilot on foot ([`life_support_reach_tiles`]).
pub const LIFE_SUPPORT_MAX_LEVEL: u8 = 3;
/// Hull damage per second a rig takes in vacuum without a pressurized hull (the frame
/// buckles).
pub const VACUUM_HULL_DAMAGE_PER_SECOND: f32 = 14.0;
/// Hull damage per second without life support (the crew struggles at the controls).
pub const VACUUM_CREW_DAMAGE_PER_SECOND: f32 = 9.0;
/// Breathable air ([`crate::environment::Environment::breathable`]) under which a place
/// counts as vacuum (orbit and the Moon, in every sky's bands).
pub const VACUUM_AIR_DENSITY: f32 = 0.02;
/// The fuel tank tier the checklist asks for (the Moon is 85 % of a maxed rig's still-air
/// reach; the last climb through orbit is cheap).
pub const MOON_FUEL_TANK_TIER: u8 = 14;
/// The Altitude Intake tier the checklist asks for: the top tier (V9's climb table: nothing
/// less lifts a tier 14 tank to the Moon).
pub const MOON_INTAKE_TIER: u8 = 6;
/// The De-Icer tier that covers the Moon's cold ([`crate::environment::MAX_COLD`]).
pub const MOON_DE_ICER_TIER: u8 = 5;

/// How far (tiles) a rig's life support reaches its pilot on foot with an air line, at tier
/// `level` (0: no line).
#[must_use]
pub const fn life_support_reach_tiles(level: u8) -> f32 {
    match level {
        0 => 0.0,
        1 => 6.0,
        2 => 12.0,
        _ => 24.0,
    }
}

/// One thing a rig needs for the trip to the Moon.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum MoonRequirement {
    /// A near-max fuel tank ([`MOON_FUEL_TANK_TIER`]).
    TopFuel,
    /// High-altitude intakes ([`MOON_INTAKE_TIER`]): the dead-air gap.
    Intakes,
    /// A De-Icer that covers the cold up there ([`MOON_DE_ICER_TIER`]).
    DeIcer,
    /// A pressurized hull: vacuum crushes anything else.
    PressurizedHull,
    /// A life-support module: air for the crew and the pilot.
    LifeSupport,
}

impl MoonRequirement {
    pub const ALL: [Self; 5] = [
        Self::TopFuel,
        Self::Intakes,
        Self::DeIcer,
        Self::PressurizedHull,
        Self::LifeSupport,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TopFuel => "Top-tier fuel tank",
            Self::Intakes => "High-altitude intakes",
            Self::DeIcer => "De-Icer",
            Self::PressurizedHull => "Pressurized hull",
            Self::LifeSupport => "Life-support module",
        }
    }

    /// Whether `player`'s rig has it.
    #[must_use]
    pub const fn met_by(self, player: &Player) -> bool {
        let gear = player.hazard_gear;
        match self {
            Self::TopFuel => player.fuel_tank_level >= MOON_FUEL_TANK_TIER,
            Self::Intakes => gear.altitude_intake >= MOON_INTAKE_TIER,
            Self::DeIcer => gear.de_icer >= MOON_DE_ICER_TIER,
            Self::PressurizedHull => gear.pressurized_hull >= VACUUM_SEAL_TIER,
            Self::LifeSupport => gear.life_support > 0,
        }
    }
}

/// The Moon checklist for `player`: each requirement and whether it is met.
#[must_use]
pub fn approach_checklist(player: &Player) -> [(MoonRequirement, bool); 5] {
    MoonRequirement::ALL.map(|requirement| (requirement, requirement.met_by(player)))
}

/// Whether world pixel `(x, y)` is in vacuum: orbit and the Moon.
///
/// Never inside the lunar colony's pressurized rooms or behind the alien facility's veil
/// ([`crate::worldgen::moon::MoonLayout::pressurized`]). The game reads it too (the muffled
/// sound, the HUD).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
pub fn vacuum_at(settings: &crate::worldgen::WorldSettings, x: f32, y: f32) -> bool {
    if !settings.generator.has_sky() || y > 0.0 {
        return false;
    }
    if crate::environment::environment_at(settings, x, y).breathable >= VACUUM_AIR_DENSITY {
        return false;
    }
    let (tile_x, tile_y) = (
        (x / crate::terrain::TILE_SIZE).floor() as i32,
        (y / crate::terrain::TILE_SIZE).floor() as i32,
    );
    !crate::worldgen::moon::layout(settings)
        .is_some_and(|layout| layout.pressurized(tile_x, tile_y))
}

/// [`vacuum_at`] for local pixel `(x, y)` of a world in `frame` (F7).
#[must_use]
pub fn vacuum_in(
    settings: &crate::worldgen::WorldSettings,
    frame: crate::position::Frame,
    x: f32,
    y: f32,
) -> bool {
    let (x, y) = frame.legacy_px(x, y);
    vacuum_at(settings, x, y)
}

/// [`sheltered_at`] for local pixel `(x, y)` of a world in `frame` (F7).
#[must_use]
pub fn sheltered_in(
    settings: &crate::worldgen::WorldSettings,
    frame: crate::position::Frame,
    x: f32,
    y: f32,
) -> bool {
    let (x, y) = frame.legacy_px(x, y);
    sheltered_at(settings, x, y)
}

/// Whether world pixel `(x, y)` is sheltered from the cold of space.
///
/// That is inside the lunar colony's pressurized, heated rooms or behind the alien facility's
/// veil ([`crate::worldgen::moon::MoonLayout::pressurized`]). A pilot on foot there needs no
/// thermal lining (the pilot's exposure reads it, T23).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
pub fn sheltered_at(settings: &crate::worldgen::WorldSettings, x: f32, y: f32) -> bool {
    if !settings.generator.has_moon_content() || y > 0.0 {
        return false;
    }
    let (tile_x, tile_y) = (
        (x / crate::terrain::TILE_SIZE).floor() as i32,
        (y / crate::terrain::TILE_SIZE).floor() as i32,
    );
    crate::worldgen::moon::layout(settings).is_some_and(|layout| layout.pressurized(tile_x, tile_y))
}

/// The requirements `player` still lacks.
#[must_use]
pub fn missing_requirements(player: &Player) -> Vec<MoonRequirement> {
    MoonRequirement::ALL
        .into_iter()
        .filter(|requirement| !requirement.met_by(player))
        .collect()
}

/// What the rig is missing in vacuum (the warning when it first meets it).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum VacuumBreach {
    /// No pressurized hull (and maybe no life support either).
    Hull,
    /// A pressurized hull but no life support.
    LifeSupport,
}

impl VacuumBreach {
    /// One line for the HUD.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Hull => {
                "Hull breach in vacuum! Fit a Pressurized Hull before you head for the Moon."
            }
            Self::LifeSupport => {
                "No air up here! Fit a Life-Support module before you head for the Moon."
            }
        }
    }
}

/// The breach `player`'s rig suffers in vacuum, if any.
#[must_use]
pub const fn vacuum_breach(player: &Player) -> Option<VacuumBreach> {
    if player.hazard_gear.pressurized_hull < VACUUM_SEAL_TIER {
        Some(VacuumBreach::Hull)
    } else if player.hazard_gear.life_support == 0 {
        Some(VacuumBreach::LifeSupport)
    } else {
        None
    }
}

/// Hull damage per second `player`'s rig takes in vacuum.
#[must_use]
pub fn vacuum_damage_per_second(player: &Player) -> f32 {
    let gear = player.hazard_gear;
    let mut damage = 0.0;
    if gear.pressurized_hull < VACUUM_SEAL_TIER {
        damage += VACUUM_HULL_DAMAGE_PER_SECOND;
    }
    if gear.life_support == 0 {
        damage += VACUUM_CREW_DAMAGE_PER_SECOND;
    }
    damage
}

// ---------------------------------------------------------------------------------------------
// What a player has done on the Moon.
// ---------------------------------------------------------------------------------------------

/// A player's lunar record and the colony's one-off upgrades (private; saved with the
/// player, serde defaults for old saves).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent one-way flags, each saved with serde defaults"
)]
pub struct LunarProgress {
    /// Landed on the Moon's top side at least once (the set-piece moment plays once).
    #[serde(default)]
    pub landed: bool,
    /// Reached the strange core at the Moon's heart.
    #[serde(default)]
    pub core_reached: bool,
    /// Helped bring the Warden down.
    #[serde(default)]
    pub warden_defeated: bool,
    /// The colony's Helium-3 Reactor: every engine burns a quarter less fuel.
    #[serde(default)]
    pub reactor: bool,
    /// The colony's Alloy Drill Head: the drill bites one layer deeper per chip.
    #[serde(default)]
    pub alloy_drill: bool,
}

impl LunarProgress {
    /// Whether this is the untouched default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The fuel burn multiplier of the reactor.
    #[must_use]
    pub const fn burn_factor(&self) -> f32 {
        if self.reactor {
            REACTOR_BURN_FACTOR
        } else {
            1.0
        }
    }

    /// Extra drill bite of the alloy head.
    #[must_use]
    pub const fn drill_bonus(&self) -> u8 {
        if self.alloy_drill { 1 } else { 0 }
    }
}

/// Fuel burn with the Helium-3 Reactor.
pub const REACTOR_BURN_FACTOR: f32 = 0.75;

// ---------------------------------------------------------------------------------------------
// The colony.
// ---------------------------------------------------------------------------------------------

/// What the colony charges for an offer: credits plus lunar materials from the cargo bay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ColonyPrice {
    pub credits: u32,
    pub helium: u32,
    pub alloy: u32,
}

impl ColonyPrice {
    /// Whether `player` can pay it.
    #[must_use]
    pub fn affordable(&self, player: &Player) -> bool {
        player.credits >= self.credits
            && player.cargo_bay.material(StrategicResourceKind::Helium3) >= self.helium
            && player.cargo_bay.material(StrategicResourceKind::AlienAlloy) >= self.alloy
    }

    /// A short label ("12,000 c + 6 alloy").
    #[must_use]
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.credits > 0 {
            parts.push(format!("{} c", self.credits));
        }
        if self.helium > 0 {
            parts.push(format!("{} He-3", self.helium));
        }
        if self.alloy > 0 {
            parts.push(format!("{} alloy", self.alloy));
        }
        if parts.is_empty() {
            "Free".to_owned()
        } else {
            parts.join(" + ")
        }
    }
}

/// Credits per point of fuel at the colony's depot (dearer than town: it came a long way).
pub const COLONY_FUEL_UNIT_COST: u32 = 3;
/// Credits per point of hull at the colony's repair bay.
pub const COLONY_REPAIR_UNIT_COST: u32 = 4;
/// Fuel each block of moon ice melts into (premium fuel: free at the colony's still).
pub const MOON_ICE_FUEL: f32 = 90.0;
/// What the colony pays for a unit of helium-3.
pub const HELIUM3_PRICE: u32 = 240;
/// What the colony pays for a block of moon ice (when the tank is already full).
pub const MOON_ICE_PRICE: u32 = 45;

/// A deal at the lunar research colony's counter ([`MoonCommand::Colony`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ColonyOffer {
    /// Fill the tank at [`COLONY_FUEL_UNIT_COST`].
    Refuel,
    /// Mend the hull at [`COLONY_REPAIR_UNIT_COST`].
    Repair,
    /// Melt the moon ice in the bay into premium fuel ([`MOON_ICE_FUEL`] each, free).
    MeltIce,
    /// Sell the lunar haul: helium-3, spare moon ice and lunar relics.
    SellHaul,
    /// The Helium-3 Reactor ([`LunarProgress::reactor`]).
    Reactor,
    /// The Alloy Drill Head ([`LunarProgress::alloy_drill`]).
    AlloyDrill,
    /// A Moon weapon for the hardpoint (the Lunar Cutter or the Low-G Mortar).
    BuyWeapon(WeaponKind),
    /// The next tier of an owned Moon weapon.
    UpgradeWeapon(WeaponKind),
}

impl ColonyOffer {
    /// Every row of the counter, in order.
    pub const ALL: [Self; 10] = [
        Self::Refuel,
        Self::Repair,
        Self::MeltIce,
        Self::SellHaul,
        Self::Reactor,
        Self::AlloyDrill,
        Self::BuyWeapon(WeaponKind::LunarCutter),
        Self::BuyWeapon(WeaponKind::LowGMortar),
        Self::UpgradeWeapon(WeaponKind::LunarCutter),
        Self::UpgradeWeapon(WeaponKind::LowGMortar),
    ];

    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Refuel => "Refuel".to_owned(),
            Self::Repair => "Repair".to_owned(),
            Self::MeltIce => "Melt Moon Ice".to_owned(),
            Self::SellHaul => "Sell Lunar Haul".to_owned(),
            Self::Reactor => "Helium-3 Reactor".to_owned(),
            Self::AlloyDrill => "Alloy Drill Head".to_owned(),
            Self::BuyWeapon(weapon) => weapon.name().to_owned(),
            Self::UpgradeWeapon(weapon) => format!("Upgrade {}", weapon.name()),
        }
    }

    /// One line for the counter.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Refuel => "Imported fuel, priced for the trip it made.",
            Self::Repair => "The colony's repair arms patch the hull.",
            Self::MeltIce => "The still melts moon ice into premium fuel. Free for prospectors.",
            Self::SellHaul => "The colony buys helium-3, spare moon ice and lunar relics.",
            Self::Reactor => {
                "A fusion reactor on helium-3: every engine burns a quarter less fuel."
            }
            Self::AlloyDrill => {
                "Alien alloy drill teeth: the drill bites one layer deeper per chip."
            }
            Self::BuyWeapon(WeaponKind::LunarCutter) => {
                "The plasma cutter, refocused through alien alloy: a long, hot beam."
            }
            Self::BuyWeapon(WeaponKind::LowGMortar) => {
                "Lobs shells that sail far in low gravity and burst on impact."
            }
            Self::BuyWeapon(_) => "A weapon for the hardpoint.",
            Self::UpgradeWeapon(_) => "Harder hits, faster cycling, less energy.",
        }
    }

    /// The price for `player` now (metered deals report 0; `None` when the offer is not
    /// available to this player: already owned, maxed, nothing to sell or melt).
    #[must_use]
    pub fn price(self, player: &Player) -> Option<ColonyPrice> {
        let credits = |credits| ColonyPrice {
            credits,
            ..ColonyPrice::default()
        };
        match self {
            Self::Refuel => (player.fuel + 0.5 < player.fuel_capacity).then(|| credits(0)),
            Self::Repair => (player.hull + 0.5 < player.max_hull()).then(|| credits(0)),
            Self::MeltIce => (player.cargo_bay.material(StrategicResourceKind::MoonIce) > 0
                && player.fuel + 0.5 < player.fuel_capacity)
                .then(|| credits(0)),
            Self::SellHaul => (haul_value(&player.cargo_bay, player.fuel >= player.fuel_capacity)
                > 0)
            .then(|| credits(0)),
            Self::Reactor => (!player.lunar.reactor).then_some(ColonyPrice {
                credits: 15_000,
                helium: 10,
                alloy: 0,
            }),
            Self::AlloyDrill => (!player.lunar.alloy_drill).then_some(ColonyPrice {
                credits: 18_000,
                helium: 0,
                alloy: 8,
            }),
            Self::BuyWeapon(weapon) => (colony_weapon(weapon)
                && player.weapon_rack.mk(weapon).is_none())
            .then(|| weapon_price(weapon)),
            Self::UpgradeWeapon(weapon) => match player.weapon_rack.mk(weapon) {
                Some(mk) if colony_weapon(weapon) && mk < crate::weapon::MAX_WEAPON_MK => {
                    let base = weapon_price(weapon);
                    Some(ColonyPrice {
                        credits: weapon.upgrade_cost(mk + 1),
                        helium: base.helium / 2,
                        alloy: base.alloy / 2,
                    })
                }
                _ => None,
            },
        }
    }
}

/// Whether the colony sells `weapon`.
#[must_use]
pub const fn colony_weapon(weapon: WeaponKind) -> bool {
    matches!(weapon, WeaponKind::LunarCutter | WeaponKind::LowGMortar)
}

/// The colony's price for `weapon`.
#[must_use]
pub const fn weapon_price(weapon: WeaponKind) -> ColonyPrice {
    match weapon {
        WeaponKind::LowGMortar => ColonyPrice {
            credits: 9_000,
            helium: 4,
            alloy: 0,
        },
        _ => ColonyPrice {
            credits: 12_000,
            helium: 0,
            alloy: 6,
        },
    }
}

/// What the colony pays for the lunar haul in `bay` (moon ice only when `tank_full`: otherwise
/// it is melted into fuel first).
#[must_use]
pub fn haul_value(bay: &ItemContainer, tank_full: bool) -> u32 {
    let helium = bay.material(StrategicResourceKind::Helium3) * HELIUM3_PRICE;
    let ice = if tank_full {
        bay.material(StrategicResourceKind::MoonIce) * MOON_ICE_PRICE
    } else {
        0
    };
    let relics = bay
        .artifacts()
        .filter(|(artifact, _)| artifact.is_lunar())
        .map(|(artifact, count)| artifact.value() * count)
        .sum::<u32>();
    helium + ice + relics
}

/// A Moon action: a deal at the colony's counter (validated there inside the tick).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MoonCommand {
    /// Take colony offer `offer` (the rig or the pilot must be inside the colony).
    Colony(ColonyOffer),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lunar_tile_codes_are_unique_and_round_trip() {
        let mut codes: Vec<u64> = LunarTile::ALL.iter().map(|tile| tile.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LunarTile::ALL.len());
        for tile in LunarTile::ALL {
            assert_eq!(LunarTile::from_code(tile.code()), Some(tile));
            assert!(tile.code() <= 0xFF);
            if tile.indestructible() {
                assert_eq!(tile.hardness(), crate::terrain::UNBREAKABLE_HARDNESS);
                assert!(tile.item().is_none(), "{tile:?}");
            } else {
                assert!(tile.hardness() <= 5, "{tile:?}");
                assert!(tile.item().is_some(), "{tile:?}");
            }
        }
        assert_eq!(LunarTile::from_code(0xEE), None);
        assert!(LunarTile::Veil.pilot_passable());
        assert!(!LunarTile::Gate.pilot_passable());
    }

    #[test]
    fn the_checklist_reads_the_rig() {
        let mut player = Player::new(0.0, 0.0);
        assert_eq!(missing_requirements(&player).len(), 5);
        assert_eq!(vacuum_breach(&player), Some(VacuumBreach::Hull));
        // The sky towns' cold seal is not a vacuum seal.
        player.hazard_gear.pressurized_hull = 1;
        assert_eq!(vacuum_breach(&player), Some(VacuumBreach::Hull));
        player.hazard_gear.pressurized_hull = VACUUM_SEAL_TIER;
        assert_eq!(vacuum_breach(&player), Some(VacuumBreach::LifeSupport));
        player.hazard_gear.life_support = 1;
        assert_eq!(vacuum_breach(&player), None);
        assert!(vacuum_damage_per_second(&player) <= 0.0);
        player.fuel_tank_level = MOON_FUEL_TANK_TIER;
        player.hazard_gear.altitude_intake = MOON_INTAKE_TIER;
        player.hazard_gear.de_icer = MOON_DE_ICER_TIER;
        assert!(missing_requirements(&player).is_empty());
        assert!(approach_checklist(&player).iter().all(|(_, met)| *met));
    }

    #[test]
    fn colony_prices_follow_what_the_player_has() {
        let mut player = Player::new(0.0, 0.0);
        player.fuel = player.fuel_capacity;
        assert!(ColonyOffer::Refuel.price(&player).is_none());
        assert!(ColonyOffer::SellHaul.price(&player).is_none());
        player.cargo_bay.add(StrategicResourceKind::Helium3, 3);
        assert!(ColonyOffer::SellHaul.price(&player).is_some());
        assert_eq!(haul_value(&player.cargo_bay, true), 3 * HELIUM3_PRICE);
        let reactor = ColonyOffer::Reactor.price(&player).expect("for sale");
        assert!(!reactor.affordable(&player));
        player.lunar.reactor = true;
        assert!(ColonyOffer::Reactor.price(&player).is_none());
        assert!(
            ColonyOffer::UpgradeWeapon(WeaponKind::LunarCutter)
                .price(&player)
                .is_none()
        );
        assert!(
            ColonyOffer::BuyWeapon(WeaponKind::Railgun)
                .price(&player)
                .is_none()
        );
        assert_eq!(
            ColonyPrice {
                credits: 5,
                helium: 1,
                alloy: 2
            }
            .label(),
            "5 c + 1 He-3 + 2 alloy"
        );
    }
}
