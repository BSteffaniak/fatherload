//! Villages to explore (expansion plan WP-VILLAGE).
//!
//! A village is a pure function of the world's settings and its site: nothing about it is
//! saved or replicated except what players did there (their [`VillageJournal`]). Worlds from
//! [`GeneratorVersion::V6`](crate::worldgen::GeneratorVersion::V6) on have them
//! ([`GeneratorVersion::has_villages`](crate::worldgen::GeneratorVersion::has_villages)).
//!
//! - **Placement** ([`placement`]): surface villages on a ladder either side of town, every
//!   ~400-800 columns, plus rare underground settlements in carved halls. Both are regional
//!   features ([`crate::worldgen::FeatureKind::Village`], detail
//!   [`crate::worldgen::FeatureDetail::Village`]), so `Terrain::nearest_feature` and
//!   `features_in` find them anywhere; [`nearest_village`] wraps that.
//! - **Ground**: a village's span is flattened to the heightmap's row at its centre, easing back
//!   over [`placement::FLATTEN_FADE_COLUMNS`] (the hook in
//!   [`crate::worldgen::streamed::surface_row`]). On V7 biome worlds the biome heightmap lays
//!   each village on a flat shelf of its own ([`crate::worldgen::biome::flat_sites_in`]) and no
//!   cave undercuts it. Building lots, the plaza and the landing pad stand on foundation tiles.
//! - **Layout** ([`layout`]): a grammar picks the culture (from the landscape flavour, the hook
//!   [`layout::site_landscape`]), the building style, a specialty, the residents (generated with
//!   [`crate::people::generate_population`]) and lays out a hall, a market, the specialty's
//!   building, an inn, workshops, houses, farms, towers and mine heads around a plaza, with a
//!   landing pad, decorations and signposts to the neighbouring villages.
//! - **Services** ([`services`]): every enterable building offers deals ([`VillageDeal`]) bought
//!   with [`crate::SessionServiceRequest::Village`] at the building's door, validated inside the
//!   tick like every town service. Specialties give each village its reason to visit: a
//!   mineral buyer, a parts dealer, a cosmetic vendor, an armory, a fuel depot, a cartographer
//!   (reveals map regions) and a shrine (timed blessings).
//! - **Residents and facts**: [`Village::population`] holds 4-12 residents with families and
//!   relationships; [`village_facts`] caches the story-worthy places near a village
//!   ([`crate::people::WorldFact`]) for dialogue rumours and story seeds
//!   ([`Village::story_seeds`]). NPC entities and routines are WP-PEOPLE's (wave 3); until then
//!   residents stand still and talk through the dialogue system (presentation side).
//!
//! Lookups ([`village`], [`villages_between`], [`village_at`], [`building_at`]) are memoized per
//! process like streamed regions: the first call for a village builds it (well under a
//! millisecond), later ones are a map lookup.

pub mod layout;
pub mod placement;
pub mod report;
pub mod services;
#[cfg(test)]
mod tests;

use std::{collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};

pub use crate::people::VillageId;
pub use layout::{
    BuildingKind, BuildingStyle, Decor, DecorKind, Landscape, SignTarget, Signpost, Specialty,
    VillageBuilding, VillageRelation, VillageSetting, milestone_at,
};
pub use placement::VillageSize;
pub use services::{BlessingKind, VillageDeal, VillageOffer, VillageService};

use crate::{
    ids::SimulationTick,
    people::{Culture, MemoryFact, Population, StorySeed, VillageProblem, WorldFact},
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{FeatureKind, WorldSettings, streamed},
};

/// One village: where it is, who lives there and what it offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Village {
    pub id: VillageId,
    /// The site index it belongs to (see [`placement`]).
    pub site: i32,
    pub setting: VillageSetting,
    pub name: String,
    pub culture: Culture,
    pub landscape: Landscape,
    pub style: BuildingStyle,
    pub size: VillageSize,
    pub specialty: Specialty,
    /// 1 (struggling) to 5 (thriving).
    pub prosperity: u8,
    pub problem: Option<VillageProblem>,
    /// The first solid row under the village (buildings stand on it).
    pub ground_row: i32,
    /// First and last column of the village's flat span (inclusive).
    pub first_column: i32,
    pub last_column: i32,
    /// First and last column of the plaza.
    pub plaza: (i32, i32),
    /// First and last column of the landing pad (surface villages).
    pub landing_pad: Option<(i32, i32)>,
    /// Buildings, west to east.
    pub buildings: Vec<VillageBuilding>,
    /// Decorations, west to east.
    pub decor: Vec<Decor>,
    /// The signposts at the village's edges.
    pub signposts: Vec<Signpost>,
    /// How this village gets on with its neighbours along the ladder (west, east).
    pub relations: Vec<(VillageId, VillageRelation)>,
    pub population: Population,
    /// Residents standing about outdoors: `(resident index, column)`.
    pub loiterers: Vec<(u16, i32)>,
}

impl Village {
    /// The centre column.
    #[must_use]
    pub const fn center_column(&self) -> i32 {
        i32::midpoint(self.first_column, self.last_column)
    }

    /// The tile rectangle a rig counts as "in the village" in: the span, from well above the
    /// rooftops down to just under the ground (an underground settlement: its hall).
    #[must_use]
    pub const fn area(&self) -> crate::worldgen::TileRect {
        let (top, bottom) = match self.setting {
            VillageSetting::Surface => (self.ground_row - 24, self.ground_row + 1),
            VillageSetting::Underground => {
                (self.ground_row - placement::HALL_AIR_ROWS, self.ground_row)
            }
        };
        crate::worldgen::TileRect {
            min: TilePosition {
                x: self.first_column,
                y: top,
            },
            max: TilePosition {
                x: self.last_column,
                y: bottom,
            },
        }
    }

    /// The building whose door a rig centred at world pixel `(x, y)` stands at, if any: inside
    /// the lot's columns and standing (or hovering low) on the village ground.
    #[must_use]
    pub fn building_at_pixel(&self, x: f32, y: f32) -> Option<usize> {
        #[allow(
            clippy::cast_precision_loss,
            reason = "village rows are small; the door band is a few tiles"
        )]
        let ground = self.ground_row as f32 * TILE_SIZE;
        if y > ground || y < DOOR_REACH_TILES.mul_add(-TILE_SIZE, ground) {
            return None;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "world pixel columns fit an i32 within the streamed limit"
        )]
        let column = (x / TILE_SIZE).floor() as i32;
        self.buildings
            .iter()
            .position(|building| building.enterable() && building.contains_column(column))
    }

    /// The deals building `building` offers (empty for buildings without a counter).
    #[must_use]
    pub fn offers(&self, building: usize) -> Vec<VillageOffer> {
        self.buildings
            .get(building)
            .map_or_else(Vec::new, |found| services::offers(self, found))
    }

    /// A display line: "Copperhollow, a timber hamlet (fuel depot)".
    #[must_use]
    pub fn describe(&self) -> String {
        let kind = match self.setting {
            VillageSetting::Surface => self.size.name(),
            VillageSetting::Underground => "settlement",
        };
        let style = self.style.name();
        let article = if style.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!(
            "{}, {article} {style} {kind} ({})",
            self.name,
            self.specialty.name()
        )
    }

    /// Story seeds this village grows for a player with `history` (best first). `facts` are
    /// the places near it ([`village_facts`]); `salt` reshuffles near-ties (a new day).
    #[must_use]
    pub fn story_seeds(
        &self,
        facts: &[WorldFact],
        history: &[MemoryFact],
        salt: u64,
    ) -> Vec<StorySeed> {
        let problems: Vec<VillageProblem> = self.problem.into_iter().collect();
        crate::people::story_seeds(&self.population, facts, &problems, history, salt)
    }
}

/// Tiles above the village ground (rig centre) that still count as standing at a door.
pub const DOOR_REACH_TILES: f32 = 3.0;

/// Columns either side of a village whose story-worthy places its residents talk about.
pub const FACT_RADIUS_COLUMNS: i32 = 160;

/// What one player has done with villages.
///
/// Where they have been, which villages a cartographer charted for them, and an active shrine
/// blessing. Private (the player's loadout), saved with the player; serde defaults keep older
/// saves loading.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct VillageJournal {
    /// Villages the player's rig has entered.
    #[serde(default)]
    pub visited: BTreeSet<VillageId>,
    /// Villages the player knows of without having been there (charted on their map).
    #[serde(default)]
    pub charted: BTreeSet<VillageId>,
    #[serde(default)]
    pub blessing: Option<Blessing>,
}

impl VillageJournal {
    /// Whether the player knows where `village` is (visited or charted).
    #[must_use]
    pub fn knows(&self, village: VillageId) -> bool {
        self.visited.contains(&village) || self.charted.contains(&village)
    }

    /// The blessing in effect at `tick`, if any.
    #[must_use]
    pub fn blessing_at(&self, tick: SimulationTick) -> Option<BlessingKind> {
        self.blessing
            .filter(|blessing| tick.get() < blessing.until.get())
            .map(|blessing| blessing.kind)
    }
}

/// A shrine blessing and when it fades.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Blessing {
    pub kind: BlessingKind,
    /// The first tick it no longer applies.
    pub until: SimulationTick,
}

/// The village `id` of a world with `settings` (`None` for worlds without villages or ids no
/// site holds).
#[must_use]
pub fn village(settings: &WorldSettings, id: VillageId) -> Option<Arc<Village>> {
    if !settings.generator.has_villages() {
        return None;
    }
    streamed::village(settings, id)
}

/// Every village (surface and underground) whose centre column lies in `first..=last`, west to
/// east.
#[must_use]
pub fn villages_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<Arc<Village>> {
    if !settings.generator.has_villages() || first > last {
        return Vec::new();
    }
    let mut found = streamed::villages_between(settings, first, last);
    found.sort_by_key(|village| (village.center_column(), village.id));
    found
}

/// The village whose [`Village::area`] contains `position`, if any.
#[must_use]
pub fn village_at(settings: &WorldSettings, position: TilePosition) -> Option<Arc<Village>> {
    if !settings.generator.has_villages() {
        return None;
    }
    streamed::village_containing(settings, position)
}

/// The village building whose door a rig centred at world pixel `(x, y)` stands at:
/// `(village, building index)`.
#[must_use]
pub fn building_at(settings: &WorldSettings, x: f32, y: f32) -> Option<(Arc<Village>, usize)> {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "world pixel positions fit an i32 tile within the streamed limit"
    )]
    let position = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    let village = village_at(settings, position)?;
    let building = village.building_at_pixel(x, y)?;
    Some((village, building))
}

/// The village nearest `from` (by centre) accepted by `accept`, searching at most
/// `max_columns` either side, through the regional feature index.
pub fn nearest_village(
    settings: &WorldSettings,
    from: TilePosition,
    max_columns: i32,
    mut accept: impl FnMut(&Village) -> bool,
) -> Option<Arc<Village>> {
    if !settings.generator.has_villages() {
        return None;
    }
    let feature =
        streamed::nearest_feature(settings, from, FeatureKind::Village, max_columns, |f| {
            f.village()
                .and_then(|id| village(settings, id))
                .is_some_and(|found| accept(&found))
        })?;
    village(settings, feature.village()?)
}

/// The story-worthy places within [`FACT_RADIUS_COLUMNS`] of `village` (cached per village:
/// regional feature queries are too slow to repeat every frame).
#[must_use]
pub fn village_facts(settings: &WorldSettings, village: &Village) -> Arc<Vec<WorldFact>> {
    streamed::village_facts(settings, village.id, village.center_column())
}

/// The name a village of site `site` goes by and its culture, without building it (signposts,
/// maps).
#[must_use]
pub fn village_identity(settings: &WorldSettings, id: VillageId) -> Option<(String, Culture)> {
    layout::identity(settings, id)
}
