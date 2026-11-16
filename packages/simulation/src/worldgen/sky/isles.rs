//! What a V9 sky grows on its islands (expansion plan WP-SKY, T20).
//!
//! V8 islands are bare lumps of turf, soil and rock. [`GeneratorVersion::V9`] keeps their
//! shapes and places and furnishes them, as a pure function of each island's shape seed (so
//! every chunk agrees, in any order):
//!
//! | Content | Low isles | Archipelago isles | Town isles |
//! |---|---|---|---|
//! | skystone ore (veins in the rock) | yes | yes | deep only |
//! | cloud silver ore (deeper rock) | no | yes | no |
//! | sky artifacts (rare, in the rock) | wind astrolabes | astrolabes, cloud pearls | no |
//! | a sky cache ([`SkyTile::Cache`]) | 40 % | 55 % | no |
//! | a nest on the turf | no | 40 % | no |
//! | a ruin (two pillars, a stone floor) | wide isles, 15 % | wide isles, 40 % | no |
//! | a spring pouring a waterfall | 20 % | 60 % | always |
//! | an updraft vent | 25 % | 35 % | no |
//! | a keystone (a crumbling isle) | 30 % | 15 % | never |
//! | a fuel buoy | half the clusters, on the biggest isle | no | no |
//! | vines among the roots | yes | yes | yes |
//!
//! A crumbling isle's [`SkyTile::Keystone`] holds it together: break it and the island falls
//! apart (`world/sky_isles.rs`). A buoy island never crumbles.
//!
//! [`GeneratorVersion::V9`]: crate::worldgen::GeneratorVersion::V9

use crate::{
    sky::SkyTile,
    terrain::{ArtifactKind, MineralKind, TileKind, TilePosition},
    worldgen::{
        FeatureId,
        noise::{RegionRng, hash2},
    },
};

use super::{SALT_SHAPE, SkyLand, SkyLandKind};

/// Rows island props (nests, ruin pillars) may stand above the turf.
pub const PROP_ROWS: i32 = 4;
/// Rows vines may dangle beyond the longest roots.
pub const VINE_EXTRA_ROWS: i32 = 5;
/// Credits a buoy charges for a refill (expensive on purpose: early hopping costs).
pub const BUOY_PRICE: u32 = 450;
/// Fuel a buoy holds: one refill adds at most this much.
///
/// A small, fixed amount, and a buoy only fills a tank up to [`buoy_fill_level`] (a third of
/// it plus this much): enough to carry early rigs from isle to isle (a tier 1-3 tank, up to
/// 1,070 fuel, fills right up), little for a big tank. Sized for the second fuel curve (about
/// 4.7 times the first's tanks: it was 160 then) and V9's taller low isle band. A rig leaving
/// a buoy is never better off than one that climbed to the buoy's altitude on a full tank, so
/// buoys never open the dead-air gap
/// (`world::sky_isle_tests::buoys_never_open_the_dead_air_gap`).
pub const BUOY_FUEL: f32 = 720.0;

/// The most fuel a buoy fills a tank of `capacity` up to: a third of the tank plus
/// [`BUOY_FUEL`] (never more than the tank).
#[must_use]
pub fn buoy_fill_level(capacity: f32) -> f32 {
    (capacity / 3.0 + BUOY_FUEL).min(capacity)
}

/// The fuel a buoy adds to a tank of `capacity` holding `fuel` (0 when it is already at or
/// over [`buoy_fill_level`]).
#[must_use]
pub fn buoy_refill(capacity: f32, fuel: f32) -> f32 {
    (buoy_fill_level(capacity) - fuel).clamp(0.0, BUOY_FUEL)
}

const SALT_CONTENT: u64 = 0x5C1E_0101;
const SALT_BUOY: u64 = 0x5C1E_0102;
const SALT_ORE: u64 = 0x5C1E_0103;
const SALT_ARTIFACT: u64 = 0x5C1E_0104;
const SALT_PILLAR: u64 = 0x5C1E_0105;

/// Columns a ruin spans (pillars at both ends, ruin stone between).
pub const RUIN_WIDTH: i32 = 7;

/// The furnishings of one V9 island (positions are tiles; every field is optional).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IsleContent {
    /// A sky cache buried in the rock.
    pub cache: Option<TilePosition>,
    /// A nest standing on the turf.
    pub nest: Option<TilePosition>,
    /// First column of a ruin ([`RUIN_WIDTH`] wide).
    pub ruin: Option<i32>,
    /// A spring at the island's rim: a waterfall pours from it into the void.
    pub spring: Option<TilePosition>,
    /// The keystone of a crumbling isle.
    pub keystone: Option<TilePosition>,
    /// An updraft vent in the turf (its column of air rises from the row above it).
    pub vent: Option<TilePosition>,
    /// The centre tile of a fuel buoy's pad (three pad tiles wide, in the turf row).
    pub buoy: Option<TilePosition>,
}

/// A fuel buoy: where its pad is and which island it moors to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuoySite {
    /// The island's feature id (the buoy's identity: one refill per player per buoy).
    pub isle: FeatureId,
    /// The centre tile of its pad (the turf row).
    pub pad: TilePosition,
}

impl BuoySite {
    /// Whether a rig at tile `at` is parked on (or hovering just over) the pad.
    #[must_use]
    pub const fn serves(&self, at: TilePosition) -> bool {
        (at.x - self.pad.x).abs() <= 2 && at.y < self.pad.y && at.y >= self.pad.y - 3
    }
}

/// How tall the pillars of the ruin starting at `first` stand: `(west, east)`.
#[must_use]
pub const fn pillar_heights(shape_seed: u64, first: i32) -> (i32, i32) {
    let roll = hash2(shape_seed, SALT_PILLAR, first, 0);
    (2 + (roll % 2) as i32, 1 + ((roll >> 8) % 3) as i32)
}

impl IsleContent {
    /// The tile of `land` (whose content this is) at `(x, y)`; `dx` is `x - land.center_x`
    /// and `top` the turf row of column `x`.
    pub(super) fn tile_at(
        &self,
        land: &SkyLand,
        x: i32,
        dx: i32,
        y: i32,
        top: i32,
    ) -> Option<TileKind> {
        if y < top {
            return self.prop_at(land, x, y, top).map(TileKind::Sky);
        }
        let bottom = land.bottom(x, dx, top);
        if y > bottom {
            return dangling(land, x, dx, y - bottom).map(TileKind::Sky);
        }
        let here = TilePosition { x, y };
        let layer = y - top;
        let special = if self.cache == Some(here) {
            Some(SkyTile::Cache)
        } else if self.keystone == Some(here) {
            Some(SkyTile::Keystone)
        } else if self.spring == Some(here) {
            Some(SkyTile::Spring)
        } else if self.vent == Some(here) {
            Some(SkyTile::Vent)
        } else if layer == 0 && self.buoy.is_some_and(|pad| (x - pad.x).abs() <= 1) {
            Some(SkyTile::BuoyPad)
        } else if layer == 0
            && self
                .ruin
                .is_some_and(|first| x >= first && x < first + RUIN_WIDTH)
        {
            Some(SkyTile::RuinBrick)
        } else {
            None
        };
        if let Some(tile) = special {
            return Some(TileKind::Sky(tile));
        }
        let body = land.body_tile(x, y, layer);
        if matches!(body, SkyTile::Rock(_)) {
            if let Some(artifact) = artifact_at(land, x, y) {
                return Some(TileKind::Artifact(artifact));
            }
            if let Some(mineral) = ore_at(land, x, y, layer) {
                return Some(TileKind::Ore(mineral));
            }
        }
        Some(TileKind::Sky(body))
    }

    /// A prop standing on the turf at `(x, y)` (above `top`, the turf row of column `x`).
    fn prop_at(&self, land: &SkyLand, x: i32, y: i32, top: i32) -> Option<SkyTile> {
        if self.nest == Some(TilePosition { x, y }) {
            return Some(SkyTile::Nest);
        }
        let first = self.ruin?;
        let (west, east) = pillar_heights(land.shape, first);
        let height = if x == first {
            west
        } else if x == first + RUIN_WIDTH - 1 {
            east
        } else {
            return None;
        };
        (y >= top - height).then_some(SkyTile::RuinPillar)
    }
}

/// Roots, vines and crystal hanging `below` rows under the underside of column `x` of a V9
/// island: V8's roots with every third strand a longer leafy vine.
fn dangling(land: &SkyLand, x: i32, dx: i32, below: i32) -> Option<SkyTile> {
    let roll = hash2(land.shape, SALT_SHAPE + 4, x, 0);
    if roll.is_multiple_of(4) {
        let length =
            1 + i32::try_from((roll >> 8) % 4).unwrap_or(0) + (land.half_width - dx.abs()) / 8;
        if (roll >> 20).is_multiple_of(3) {
            let vine = length + 1 + i32::try_from((roll >> 24) % 4).unwrap_or(0);
            (below <= vine.min(length + VINE_EXTRA_ROWS)).then_some(SkyTile::Vines)
        } else {
            (below <= length).then_some(SkyTile::Roots)
        }
    } else if roll % 9 == 1 && dx.abs() * 2 < land.half_width {
        let length = 1 + i32::try_from((roll >> 8) % 3).unwrap_or(0);
        (below <= length).then_some(SkyTile::Crystal)
    } else {
        None
    }
}

/// The sky ore at `(x, y)` in `land`'s rock (`layer` rows under the turf), if any: veins in
/// clusters of a few tiles.
fn ore_at(land: &SkyLand, x: i32, y: i32, layer: i32) -> Option<MineralKind> {
    let cluster = hash2(land.shape, SALT_ORE, x.div_euclid(2), y.div_euclid(2));
    if hash2(land.shape, SALT_ORE + 1, x, y).is_multiple_of(3) {
        return None;
    }
    match land.kind {
        SkyLandKind::Isle => cluster.is_multiple_of(7).then_some(MineralKind::Skystone),
        SkyLandKind::ArchipelagoIsle => {
            if layer >= 5 && cluster % 9 == 1 {
                Some(MineralKind::CloudSilver)
            } else {
                cluster.is_multiple_of(10).then_some(MineralKind::Skystone)
            }
        }
        SkyLandKind::TownIsle => {
            (layer >= 6 && cluster.is_multiple_of(12)).then_some(MineralKind::Skystone)
        }
        SkyLandKind::Continent | SkyLandKind::Moon => None,
    }
}

/// A sky artifact buried at `(x, y)`, if any (rare).
fn artifact_at(land: &SkyLand, x: i32, y: i32) -> Option<ArtifactKind> {
    let roll = hash2(land.shape, SALT_ARTIFACT, x, y);
    match land.kind {
        SkyLandKind::Isle => roll
            .is_multiple_of(420)
            .then_some(ArtifactKind::WindAstrolabe),
        SkyLandKind::ArchipelagoIsle => match roll % 520 {
            0 => Some(ArtifactKind::WindAstrolabe),
            1 => Some(ArtifactKind::CloudPearl),
            _ => None,
        },
        _ => None,
    }
}

/// Which band an island's furnishings follow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flavour {
    Low,
    Archipelago,
    Town,
}

/// The low isles of a cell, furnished; the biggest isle of about half the clusters gets a
/// fuel buoy (and so never crumbles).
pub(super) fn furnish_low_cell(seed: u64, rx: i32, ry: i32, lands: Vec<SkyLand>) -> Vec<SkyLand> {
    let mut lands: Vec<SkyLand> = lands
        .into_iter()
        .map(|land| furnish(land, Flavour::Low))
        .collect();
    let mut rng = RegionRng::new(seed, SALT_BUOY, rx, ry);
    if rng.chance(500)
        && let Some(index) = lands
            .iter()
            .enumerate()
            .filter(|(_, land)| land.half_width >= 6)
            .max_by_key(|(_, land)| (land.half_width, std::cmp::Reverse(land.id)))
            .map(|(index, _)| index)
    {
        let land = &mut lands[index];
        let center = land.center_x;
        let pad = TilePosition {
            x: center,
            y: land.top(center, 0),
        };
        if let Some(content) = land.content.as_mut() {
            content.buoy = Some(pad);
            content.keystone = None;
            // Nothing else on the pad's columns.
            if content
                .vent
                .is_some_and(|vent| (vent.x - center).abs() <= 2)
            {
                content.vent = None;
            }
            if content
                .ruin
                .is_some_and(|first| first <= center + 2 && first + RUIN_WIDTH > center - 2)
            {
                content.ruin = None;
            }
        }
    }
    lands
}

/// An archipelago cell's islands, furnished (a town isle gets only its spring).
pub(super) fn furnish_archipelago_cell(lands: Vec<SkyLand>) -> Vec<SkyLand> {
    lands
        .into_iter()
        .map(|land| {
            let flavour = if land.kind == SkyLandKind::TownIsle {
                Flavour::Town
            } else {
                Flavour::Archipelago
            };
            furnish(land, flavour)
        })
        .collect()
}

/// `land` with its furnishings rolled from its shape seed.
fn furnish(mut land: SkyLand, flavour: Flavour) -> SkyLand {
    // Mark the land first, so its geometry (rise, hang) is the V9 one while placing.
    land.content = Some(IsleContent::default());
    let mut rng = RegionRng::new(land.shape, SALT_CONTENT, 0, 0);
    let hw = land.half_width;
    let center = land.center_x;
    let top = |x: i32| land.top(x, x - center);
    let bottom = |x: i32| land.bottom(x, x - center, top(x));
    let mut content = IsleContent::default();
    let (cache, nest, ruin, spring, vent, keystone) = match flavour {
        Flavour::Low => (400, 0, 150, 200, 250, 300),
        Flavour::Archipelago => (550, 400, 400, 600, 350, 150),
        Flavour::Town => (0, 0, 0, 1_000, 0, 0),
    };
    // Every roll is drawn, used or not, so one feature never shifts another.
    let rolls: [bool; 6] = [
        rng.chance(cache),
        rng.chance(nest),
        rng.chance(ruin),
        rng.chance(spring),
        rng.chance(vent),
        rng.chance(keystone),
    ];
    let cache_dx = rng.range(-(hw / 3), hw / 3);
    let cache_depth = rng.range(4, 6);
    let nest_dx = rng.range(-(hw - 3).max(0), (hw - 3).max(0));
    let ruin_dx = rng.range(-(hw - 9).max(0), (hw - 9).max(0));
    let spring_side = if rng.chance(500) { 1 } else { -1 };
    let vent_dx = rng.range(-(hw / 3), hw / 3);

    if rolls[2] && hw >= 9 {
        content.ruin = Some(center + ruin_dx - RUIN_WIDTH / 2);
    }
    let on_ruin = |x: i32| {
        content
            .ruin
            .is_some_and(|first| x >= first - 1 && x <= first + RUIN_WIDTH)
    };
    if rolls[0] {
        let x = center + cache_dx;
        let y = (top(x) + cache_depth).min(bottom(x) - 1);
        if y > top(x) + 1 {
            content.cache = Some(TilePosition { x, y });
        }
    }
    if rolls[1] && hw >= 6 {
        let x = center + nest_dx;
        if !on_ruin(x) {
            content.nest = Some(TilePosition { x, y: top(x) - 1 });
        }
    }
    if rolls[3] && hw >= 6 {
        let x = center + spring_side * (hw - 1);
        let y = top(x) + 1;
        if y <= bottom(x) {
            content.spring = Some(TilePosition { x, y });
        }
    }
    if rolls[4] && hw >= 7 {
        let x = center + vent_dx;
        if !on_ruin(x) && content.nest.is_none_or(|nest| (nest.x - x).abs() > 1) {
            content.vent = Some(TilePosition { x, y: top(x) });
        }
    }
    if rolls[5] {
        let x = center;
        let (t, b) = (top(x), bottom(x));
        let y = t + (b - t) / 2;
        if y > t + 1 && content.cache != Some(TilePosition { x, y }) {
            content.keystone = Some(TilePosition { x, y });
        }
    }
    land.content = Some(content);
    land
}

/// The fuel buoy of `land`, if it has one.
#[must_use]
pub fn buoy_of(land: &SkyLand) -> Option<BuoySite> {
    let pad = land.content.as_ref()?.buoy?;
    Some(BuoySite { isle: land.id, pad })
}
