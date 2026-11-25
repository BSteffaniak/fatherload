//! Binding a quest to the current world, and the HQ board's daily postings.
//!
//! [`bind_quest`] turns a definition's [`TargetSpec`]s into real places through the world's
//! feature index and lays out marked spots for multi-spot objectives. It is a pure function of
//! the world, the accepting player's position and journal, a salt and the places other active
//! quests already use, so the host and every replica bind identically.

use std::collections::BTreeSet;

use crate::{
    poi::{PoiJournal, PoiKind, PoiKnowledge, PoiTile},
    surface::SURFACE_BUILDINGS,
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{FeatureId, FeatureKind, TileRect, WorldFeature, noise::hash2},
};

use super::{
    BOARD_TEMPLATES, Objective, ObjectiveProgress, QuestDef, QuestGiver, QuestId, QuestTarget,
    SURFACE_ROW, StoryPlace, TargetSpec,
};
use crate::{people::VillageId, village::VillageSetting};

const SALT_SPOTS: u64 = 0x0E57_0001;
const SALT_BOARD: u64 = 0x0E57_0002;

/// Postings on the HQ board each day.
pub const BOARD_SLOTS: u8 = 3;

/// Row a rig stands on at a building's door.
pub const DOOR_ROW: i32 = 4;

/// Gap between the spots of a simultaneous objective, in tiles: more than a rig at full speed
/// covers inside a crew window, so a crew window really takes two rigs.
const SIMULTANEOUS_SPREAD: i32 = 26;
/// Smallest gap between two detonation spots (a bomb's blast never reaches two).
const DETONATE_SPACING: i32 = 5;

/// Whether a POI's loot or lock is still intact.
#[must_use]
pub fn poi_unopened(terrain: &Terrain, feature: &WorldFeature) -> bool {
    let wanted = |tile: PoiTile| match feature.poi_kind() {
        Some(PoiKind::SupplyCache) => tile == PoiTile::SupplyCrate,
        Some(PoiKind::DerelictRig) => tile == PoiTile::SalvageCore,
        Some(PoiKind::Vault) => matches!(tile, PoiTile::VaultDoor | PoiTile::VaultCache),
        Some(PoiKind::RivalClaim) => tile == PoiTile::OutpostSalvage,
        Some(PoiKind::Formation) | None => true,
    };
    feature.bounds.positions().any(|position| {
        matches!(terrain.tile(position).map(|tile| tile.kind), Some(TileKind::Poi(tile)) if wanted(tile))
    })
}

/// Whether a rig can dig to and stand at `position` (inside the world, below the crust, not a
/// vault fixture or a building foundation).
#[must_use]
pub fn diggable_spot(terrain: &Terrain, position: TilePosition) -> bool {
    if position.x < terrain.min_x() + 2
        || position.x > terrain.max_x() - 2
        || position.y < super::surface_row_at(terrain, position.x).max(SURFACE_ROW) + 3
        || position.y > terrain.height() - 3
    {
        return false;
    }
    match terrain.tile(position).map(|tile| tile.kind) {
        Some(TileKind::Poi(tile)) => !tile.is_vault_fixture(),
        Some(TileKind::Foundation) | None => false,
        Some(_) => true,
    }
}

/// The nearest diggable tile to `position` (searching outward a few rings).
fn nearest_diggable(terrain: &Terrain, position: TilePosition) -> Option<TilePosition> {
    for radius in 0_i32..=6 {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs().max(dy.abs()) != radius {
                    continue;
                }
                let candidate = TilePosition {
                    x: position.x + dx,
                    y: position.y + dy,
                };
                if diggable_spot(terrain, candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn distance_squared(a: TilePosition, b: TilePosition) -> i64 {
    let dx = i64::from(a.x - b.x);
    let dy = i64::from(a.y - b.y);
    dx * dx + dy * dy
}

/// Chebyshev (king-move) distance between two tiles.
#[must_use]
pub const fn tile_distance(a: TilePosition, b: TilePosition) -> i32 {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    if dx > dy { dx } else { dy }
}

/// Where a rig stands at `giver`'s door.
#[must_use]
pub fn giver_door(giver: QuestGiver) -> TilePosition {
    let zone = giver.zone();
    let building = SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == zone)
        .unwrap_or(&SURFACE_BUILDINGS[0]);
    TilePosition {
        x: building.tile_x + building.tile_width / 2,
        y: DOOR_ROW,
    }
}

/// The feature kind a quest binds to when a world has none of `kind` anywhere.
const fn stand_in(kind: FeatureKind) -> Option<FeatureKind> {
    match kind {
        FeatureKind::Motherlode => Some(FeatureKind::ArtifactBed),
        // Worlds before V6 have no cave ecosystems: a cave quest goes to a large cavern.
        FeatureKind::Chamber | FeatureKind::Cave => Some(FeatureKind::LargeCavern),
        _ => None,
    }
}

/// Columns either side of the binding origin a streamed world searches for quest targets.
///
/// Nearest first: a quest should send a rig somewhere it can reach, and the search widens only
/// when a window holds no candidate at all.
pub const STREAMED_BIND_COLUMNS: [i32; 3] = [160, 640, 2_048];

/// Every feature a quest could bind to for `kinds`: the whole index of a bounded world, or,
/// in a streamed one, the wanted bands' rows (then every row) of the columns around `origin`
/// (widening while that finds nothing).
fn world_features(
    terrain: &Terrain,
    origin: TilePosition,
    (first, last): (u8, u8),
    kinds: &[FeatureKind],
    keep: impl Fn(&WorldFeature) -> bool,
) -> Vec<WorldFeature> {
    if !terrain.is_streamed() {
        return terrain
            .features()
            .all()
            .iter()
            .filter(|feature| kinds.contains(&feature.kind) && keep(feature))
            .copied()
            .collect();
    }
    let settings = terrain.settings();
    let band = |index: u8| crate::worldgen::Band::ALL[usize::from(index).min(11)];
    let band_rows = (
        crate::worldgen::strata::band_layout_rows(settings, band(first)).0 - 8,
        crate::worldgen::strata::band_layout_rows(settings, band(last)).1 + 8,
    );
    for columns in STREAMED_BIND_COLUMNS {
        for (top, bottom) in [band_rows, (0, terrain.height() - 1)] {
            let rect = TileRect {
                min: TilePosition {
                    x: origin.x - columns,
                    y: top,
                },
                max: TilePosition {
                    x: origin.x + columns,
                    y: bottom,
                },
            };
            let found: Vec<WorldFeature> = terrain
                .features_in(&rect, Some(kinds))
                .into_iter()
                .filter(|feature| keep(feature))
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
    }
    Vec::new()
}

/// Candidate features for `spec`, best first.
fn candidates(
    spec: TargetSpec,
    terrain: &Terrain,
    origin: TilePosition,
    journal: &PoiJournal,
    taken: &BTreeSet<FeatureId>,
) -> Vec<WorldFeature> {
    let (first, last) = spec.bands();
    // How many bands a feature lies outside the wanted range (0 inside it).
    let band_gap = |feature: &WorldFeature| {
        let index = feature.band.index();
        usize::from(first)
            .saturating_sub(index)
            .max(index.saturating_sub(usize::from(last)))
    };
    // Lean or unlucky worlds (and generators whose bands vary in thickness) can lack a kind in
    // the wanted bands: keep the candidates in the nearest bands that have one rather than leave
    // the quest unbindable.
    let nearest_bands = |all: Vec<WorldFeature>| -> Vec<WorldFeature> {
        let nearest = all.iter().map(band_gap).min();
        all.into_iter()
            .filter(|feature| Some(band_gap(feature)) == nearest)
            .collect()
    };
    let mut found: Vec<WorldFeature> = match spec {
        TargetSpec::Poi { kind, unopened, .. } => {
            if !terrain.settings().generator.has_points_of_interest() {
                return Vec::new();
            }
            nearest_bands(world_features(
                terrain,
                origin,
                (first, last),
                &[FeatureKind::PointOfInterest],
                |feature| {
                    feature.poi_kind() == Some(kind)
                        && (!unopened || poi_unopened(terrain, feature))
                },
            ))
        }
        TargetSpec::Feature { kind, .. } => {
            // A world without any of the kind (some lean worlds roll no motherlode at all)
            // uses its closest stand-in.
            let mut all = world_features(terrain, origin, (first, last), &[kind], |_| true);
            if all.is_empty()
                && let Some(stand_in) = stand_in(kind)
            {
                all = world_features(terrain, origin, (first, last), &[stand_in], |_| true);
            }
            nearest_bands(all)
        }
        // Story places bind through `story_place`.
        TargetSpec::Story(_) => Vec::new(),
    };
    found.sort_by_key(|feature| {
        (
            taken.contains(&feature.id),
            journal.knowledge(feature.id) == Some(PoiKnowledge::Visited),
            distance_squared(feature.center, origin),
            feature.id,
        )
    });
    found
}

/// What a quest target was bound to (story text names it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundTarget {
    /// A feature from the index: a point of interest, cave, village, sky land or the Moon.
    Feature(WorldFeature),
    /// The town (the keepers' street).
    Town,
    /// A wanderers' camp.
    Camp(VillageId),
    /// The giver's own village, by id (no feature needed).
    Village(VillageId),
    /// The highest ground near the origin: the sky's stand-in on worlds without one.
    Ridge,
    /// A tomb or crypt site (V10; Wave 6 story places): the target is its door.
    Tomb(crate::tomb::TombSite),
    /// A sealed ruin of the pilot spaces (V9): the target is its vent grate.
    Ruin(crate::worldgen::streamed::pilot_spaces::PilotSpace),
    /// A village's trading post, which turns night market after dusk.
    Market(VillageId),
    /// A plot of land the land office lists.
    Plot(crate::home::PlotSite),
    /// A sky town of the archipelago (V9).
    SkyTown(VillageId),
    /// One of the Fathers' expedition sites (Wave 10): the log it holds (0-based).
    Expedition(u8),
    /// A seam of rock in a deep stratum (Wave 10).
    Seam(crate::descent::Stratum),
}

/// Columns either side of the origin a story searches for villages, camps and halls.
pub const STORY_PLACE_COLUMNS: i32 = 1_400;
/// ... for sky islands.
const STORY_ISLAND_COLUMNS: i32 = 3_000;
/// ... for sky continents.
const STORY_CONTINENT_COLUMNS: i32 = 14_000;
/// Columns either side of the origin searched for the ridge (the highest ground).
const RIDGE_COLUMNS: i32 = 240;

const fn target_of(feature: &WorldFeature) -> QuestTarget {
    QuestTarget {
        feature: Some(feature.id),
        center: feature.center,
        bounds: feature.bounds,
    }
}

/// The surface rectangle a settlement spanning `first..=last` on `ground` covers.
const fn surface_target(first: i32, last: i32, ground: i32) -> QuestTarget {
    QuestTarget {
        feature: None,
        center: TilePosition {
            x: i32::midpoint(first, last),
            y: ground - 1,
        },
        bounds: TileRect {
            min: TilePosition {
                x: first,
                y: ground - 12,
            },
            max: TilePosition { x: last, y: ground },
        },
    }
}

fn town_target(terrain: &Terrain) -> (QuestTarget, BoundTarget) {
    let (first, last) = crate::surface::town_span();
    let ground = terrain.surface_row(i32::midpoint(first, last));
    (surface_target(first, last, ground), BoundTarget::Town)
}

/// The nearest village of `setting` other than `home` and the features in `skip`.
fn village_target(
    terrain: &Terrain,
    origin: TilePosition,
    setting: VillageSetting,
    home: Option<VillageId>,
    skip: &BTreeSet<FeatureId>,
) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    if !settings.generator.has_villages() {
        return None;
    }
    let feature = terrain.nearest_feature(
        origin,
        FeatureKind::Village,
        STORY_PLACE_COLUMNS,
        |feature| {
            !skip.contains(&feature.id)
                && feature.village().is_some_and(|id| {
                    Some(id) != home
                        && crate::village::village(settings, id)
                            .is_some_and(|found| found.setting == setting)
                })
        },
    )?;
    let found = crate::village::village(settings, feature.village()?)?;
    Some((
        QuestTarget {
            feature: Some(feature.id),
            center: TilePosition {
                x: found.center_column(),
                y: found.ground_row - 1,
            },
            bounds: found.area(),
        },
        BoundTarget::Feature(feature),
    ))
}

fn camp_target(
    terrain: &Terrain,
    origin: TilePosition,
    home: Option<VillageId>,
) -> Option<(QuestTarget, BoundTarget)> {
    let camps = crate::people::camps::camps_between(
        terrain.settings(),
        origin.x - STORY_PLACE_COLUMNS,
        origin.x + STORY_PLACE_COLUMNS,
    );
    let camp = camps
        .iter()
        .filter(|camp| Some(camp.id) != home)
        .min_by_key(|camp| ((camp.column - origin.x).abs(), camp.id))?;
    let (first, last) = camp.span();
    Some((
        surface_target(first, last, camp.ground_row),
        BoundTarget::Camp(camp.id),
    ))
}

/// The giver's own settlement.
/// The nearest of the Fathers' expedition sites to `origin` (Wave 10): sites in the origin's
/// own Descent first, then by distance (rows count four times columns: the trail runs down).
fn fathers_site_target(
    terrain: &Terrain,
    origin: TilePosition,
) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let here = crate::depth_progression::descent_index_at_row(settings, origin.y);
    let site = crate::expedition::sites::sites(settings)
        .into_iter()
        .min_by_key(|site| {
            let centre = site.interior().center();
            let other = crate::depth_progression::descent_index_at_row(settings, centre.y);
            (
                other != here,
                i64::from((centre.y - origin.y).abs()) * 4 + i64::from((centre.x - origin.x).abs()),
                site.log,
            )
        })?;
    let interior = site.interior();
    Some((
        QuestTarget {
            feature: None,
            center: site.canister(),
            bounds: interior,
        },
        BoundTarget::Expedition(site.log),
    ))
}

/// Rows below a settlement's ground the deep seam starts, at least.
const SEAM_MIN_DROP: i32 = 14;
/// Half the width of a deep seam, and its height.
const SEAM_HALF_WIDTH: i32 = 3;
const SEAM_HEIGHT: i32 = 5;

/// A seam of rock in `origin`'s own stratum, a little below and beside it (Wave 10): a pure
/// function of the settings and the origin, kept inside the stratum's rows and clear of the
/// builders' cable.
fn deep_seam_target(terrain: &Terrain, origin: TilePosition) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let stratum = crate::descent::Stratum::ALL
        .into_iter()
        .rev()
        .find(|stratum| layout.rows(*stratum).0 <= origin.y)?;
    let (_, last) = layout.rows(stratum);
    let roll = crate::worldgen::noise::hash2(settings.seed, SALT_DEEP_SEAM, origin.x, origin.y);
    let drop = SEAM_MIN_DROP + i32::try_from(roll % 12).unwrap_or(0);
    let top = (origin.y + drop)
        .min(last - SEAM_HEIGHT - 4)
        .max(origin.y + 4);
    let side = if roll & 0x100 == 0 { 1 } else { -1 };
    let out = 26 + i32::try_from((roll >> 12) % 30).unwrap_or(0);
    let mut x = origin.x + side * out;
    let cable = crate::worldgen::descent::landmarks::lift_column(settings);
    if (x - cable).abs() < 8 {
        x = cable + side * 12;
    }
    let bounds = TileRect {
        min: TilePosition {
            x: x - SEAM_HALF_WIDTH,
            y: top,
        },
        max: TilePosition {
            x: x + SEAM_HALF_WIDTH,
            y: top + SEAM_HEIGHT - 1,
        },
    };
    Some((
        QuestTarget {
            feature: None,
            center: bounds.center(),
            bounds,
        },
        BoundTarget::Seam(stratum),
    ))
}

const SALT_DEEP_SEAM: u64 = 0x5745_1001;

fn home_target(terrain: &Terrain, home: Option<VillageId>) -> (QuestTarget, BoundTarget) {
    let settings = terrain.settings();
    let Some(id) = home else {
        return town_target(terrain);
    };
    // Cathedral Station and the Amber Court (Wave 9 deep folk; Wave 10 fix): their halls.
    if let Some(place) = crate::people::deep_folk::DeepSettlement::of_id(id)
        && let Some((ground, first, last)) = crate::people::deep_folk::hall(settings, place)
    {
        return (
            QuestTarget {
                feature: None,
                center: TilePosition {
                    x: i32::midpoint(first, last),
                    y: ground - 1,
                },
                bounds: TileRect {
                    min: TilePosition {
                        x: first - 2,
                        y: ground - 8,
                    },
                    max: TilePosition {
                        x: last + 2,
                        y: ground,
                    },
                },
            },
            BoundTarget::Village(id),
        );
    }
    if crate::people::camps::is_camp(id) {
        if let Some(camp) = crate::people::camps::camp(settings, id) {
            let (first, last) = camp.span();
            return (
                surface_target(first, last, camp.ground_row),
                BoundTarget::Camp(id),
            );
        }
    } else if id != crate::people::routine::TOWN_VILLAGE
        && let Some(found) = crate::village::village(settings, id)
    {
        return (
            QuestTarget {
                feature: None,
                center: TilePosition {
                    x: found.center_column(),
                    y: found.ground_row - 1,
                },
                bounds: found.area(),
            },
            BoundTarget::Village(id),
        );
    }
    town_target(terrain)
}

/// The highest ground within [`RIDGE_COLUMNS`] of `origin` (the westmost on ties).
fn ridge_target(terrain: &Terrain, origin: TilePosition) -> (QuestTarget, BoundTarget) {
    let (x, row) = (origin.x - RIDGE_COLUMNS..=origin.x + RIDGE_COLUMNS)
        .step_by(2)
        .map(|x| (x, terrain.surface_row(x)))
        .min_by_key(|(x, row)| (*row, *x))
        .unwrap_or_else(|| (origin.x, terrain.surface_row(origin.x)));
    (
        QuestTarget {
            feature: None,
            center: TilePosition { x, y: row - 1 },
            bounds: TileRect {
                min: TilePosition {
                    x: x - 3,
                    y: row - 5,
                },
                max: TilePosition { x: x + 3, y: row },
            },
        },
        BoundTarget::Ridge,
    )
}

fn sky_target(
    terrain: &Terrain,
    origin: TilePosition,
    kind: FeatureKind,
    columns: i32,
    skip: &BTreeSet<FeatureId>,
) -> Option<(QuestTarget, BoundTarget)> {
    if !terrain.settings().generator.has_sky() {
        return None;
    }
    let feature =
        terrain.nearest_feature(origin, kind, columns, |feature| !skip.contains(&feature.id))?;
    Some((target_of(&feature), BoundTarget::Feature(feature)))
}

/// The nearest sky island, preferring the low isles (below the cloud archipelago): story
/// islands are offered once a rig reaches the low isles
/// ([`crate::milestone::island_capable`]), so the first hop stays the one it can make.
fn island_target(
    terrain: &Terrain,
    origin: TilePosition,
    skip: &BTreeSet<FeatureId>,
) -> Option<(QuestTarget, BoundTarget)> {
    let bands = terrain.settings().generator.sky_bands()?;
    let archipelago = crate::worldgen::sky::row_of(bands.archipelago);
    terrain
        .nearest_feature(
            origin,
            FeatureKind::SkyIsland,
            STORY_ISLAND_COLUMNS,
            |feature| !skip.contains(&feature.id) && feature.center.y > archipelago,
        )
        .map(|feature| (target_of(&feature), BoundTarget::Feature(feature)))
        .or_else(|| {
            sky_target(
                terrain,
                origin,
                FeatureKind::SkyIsland,
                STORY_ISLAND_COLUMNS,
                skip,
            )
        })
}

/// A sky continent story target in its sky city when the continent has one (V9, T21): a safe
/// street of people to arrive in rather than the middle of the land.
fn in_sky_city(terrain: &Terrain, mut feature: WorldFeature) -> WorldFeature {
    let city = crate::worldgen::sky::land(terrain.settings(), feature.id)
        .as_ref()
        .and_then(crate::worldgen::sky::continent::city_of);
    if let Some(city) = city {
        feature.center = TilePosition {
            x: city.plaza.0,
            y: city.street_row - 1,
        };
        feature.bounds = TileRect {
            min: TilePosition {
                x: city.first_column,
                y: city.street_row - 12,
            },
            max: TilePosition {
                x: city.last_column,
                y: city.street_row,
            },
        };
    }
    feature
}

/// The Moon story target at the lunar research colony when the Moon is filled (V9, T22): the
/// landing pad and dome, safe and breathable, rather than the middle of the landmass.
fn at_lunar_colony(terrain: &Terrain, mut feature: WorldFeature) -> WorldFeature {
    if let Some(layout) = crate::worldgen::moon::layout(terrain.settings()) {
        let (first, last, row) = layout.pad();
        feature.center = TilePosition {
            x: i32::midpoint(first, last),
            y: row - 1,
        };
        feature.bounds = layout.colony_area();
    }
    feature
}

/// Rows under the origin a story searches for tomb sites (the deep strata's first sites).
const STORY_TOMB_DEPTH_ROWS: i32 = 420;

/// The nearest tomb or crypt site on the ground or under it (never the sky's: those wait for
/// Act II), of `style` if given.
fn tomb_target(
    terrain: &Terrain,
    origin: TilePosition,
    style: Option<crate::tomb::SiteStyle>,
) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    if !settings.generator.has_tombs() {
        return None;
    }
    let rect = TileRect {
        min: TilePosition {
            x: origin.x - STORY_PLACE_COLUMNS,
            y: -crate::worldgen::biome::SKY_HEADROOM_ROWS,
        },
        max: TilePosition {
            x: origin.x + STORY_PLACE_COLUMNS,
            y: origin.y + STORY_TOMB_DEPTH_ROWS,
        },
    };
    let site = crate::worldgen::streamed::tomb_sites::sites_touching(settings, &rect)
        .into_iter()
        .filter(|site| {
            !matches!(
                site.setting,
                crate::tomb::SiteSetting::SkyContinent | crate::tomb::SiteSetting::Moon
            ) && style.is_none_or(|style| site.style == style)
        })
        .min_by_key(|site| (distance_squared(site.door, origin), site.id))?;
    Some((
        QuestTarget {
            feature: None,
            center: site.door,
            bounds: crate::worldgen::streamed::tomb_sites::entrance_bounds(&site),
        },
        BoundTarget::Tomb(site),
    ))
}

/// The nearest sealed ruin of the pilot spaces (V9).
fn ruin_target(terrain: &Terrain, origin: TilePosition) -> Option<(QuestTarget, BoundTarget)> {
    use crate::worldgen::streamed::pilot_spaces::{self, PilotSpaceKind, REGION};
    let settings = terrain.settings();
    let rect = TileRect {
        min: TilePosition {
            x: origin.x - STORY_PLACE_COLUMNS,
            y: 0,
        },
        max: TilePosition {
            x: origin.x + STORY_PLACE_COLUMNS,
            y: origin.y + REGION.1 * 4,
        },
    };
    let space = pilot_spaces::spaces_touching(settings, &rect)
        .into_iter()
        .filter(|space| space.kind == PilotSpaceKind::Ruin)
        .min_by_key(|space| (distance_squared(space.entrance(), origin), *space))?;
    Some((
        QuestTarget {
            feature: None,
            center: space.entrance(),
            bounds: space.bounds(),
        },
        BoundTarget::Ruin(space),
    ))
}

/// The trading post of the nearest other surface village (its night market after dusk), else
/// the giver's own village's.
fn market_target(
    terrain: &Terrain,
    origin: TilePosition,
    home: Option<VillageId>,
    skip: &BTreeSet<FeatureId>,
) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    let id = village_target(terrain, origin, VillageSetting::Surface, home, skip)
        .and_then(|(_, bound)| match bound {
            BoundTarget::Feature(feature) => feature.village(),
            _ => None,
        })
        .or(home)?;
    let found = crate::village::village(settings, id)?;
    let lot = found
        .buildings
        .iter()
        .find(|lot| lot.kind == crate::village::BuildingKind::Market)?;
    let target = surface_target(
        lot.first_column - 1,
        lot.first_column + lot.width,
        found.ground_row,
    );
    Some((target, BoundTarget::Market(id)))
}

/// The nearest plot of land the land office lists (an empty homestead).
fn homestead_target(terrain: &Terrain, origin: TilePosition) -> Option<(QuestTarget, BoundTarget)> {
    let site = crate::home::plot::listed_sites_between(
        terrain.settings(),
        origin.x - STORY_PLACE_COLUMNS,
        origin.x + STORY_PLACE_COLUMNS,
    )
    .into_iter()
    .min_by_key(|site| ((site.middle() - origin.x).abs(), site.left))?;
    Some((
        surface_target(site.left, site.right(), site.ground_row),
        BoundTarget::Plot(site),
    ))
}

/// The nearest sky town in the lowest town row over the origin (V9): the first one a rig
/// climbing the archipelago meets.
fn sky_town_target(terrain: &Terrain, origin: TilePosition) -> Option<(QuestTarget, BoundTarget)> {
    let settings = terrain.settings();
    // A town whose id also names a sky city (block column 0) has the city's folk, not its
    // own: stories pass it by.
    let towns: Vec<_> = crate::worldgen::sky::towns::sky_towns_between(
        settings,
        origin.x - STORY_ISLAND_COLUMNS,
        origin.x + STORY_ISLAND_COLUMNS,
    )
    .into_iter()
    .filter(|town| crate::worldgen::sky::continent::sky_city(settings, town.id).is_none())
    .collect();
    let lowest = towns.iter().map(|town| town.block.1).min()?;
    let town = towns
        .iter()
        .filter(|town| town.block.1 == lowest)
        .min_by_key(|town| ((town.center() - origin.x).abs(), town.id))?;
    Some((
        surface_target(town.first, town.last, town.ground_row),
        BoundTarget::SkyTown(town.id),
    ))
}

/// Binds a story place from `origin` (the giver's settlement), with its fallbacks.
#[allow(clippy::too_many_lines, reason = "one arm per story place")]
fn story_place(
    place: StoryPlace,
    terrain: &Terrain,
    origin: TilePosition,
    journal: &PoiJournal,
    home: Option<VillageId>,
    skip: &BTreeSet<FeatureId>,
) -> Option<(QuestTarget, BoundTarget)> {
    let feature_fallback = |kind: FeatureKind, bands: (u8, u8)| {
        candidates(
            TargetSpec::Feature { kind, bands },
            terrain,
            origin,
            journal,
            skip,
        )
        .into_iter()
        .find(|feature| !skip.contains(&feature.id))
        .map(|feature| (target_of(&feature), BoundTarget::Feature(feature)))
    };
    let poi_fallback = |kind: PoiKind, bands: (u8, u8)| {
        candidates(
            TargetSpec::Poi {
                kind,
                bands,
                unopened: false,
            },
            terrain,
            origin,
            journal,
            skip,
        )
        .into_iter()
        .find(|feature| !skip.contains(&feature.id))
        .map(|feature| (target_of(&feature), BoundTarget::Feature(feature)))
    };
    match place {
        // Far out in the endless world the town may be tens of thousands of columns away, so
        // a camp nearby stands in for a village before the town does.
        StoryPlace::Village => Some(
            village_target(terrain, origin, VillageSetting::Surface, home, skip)
                .or_else(|| camp_target(terrain, origin, home))
                .unwrap_or_else(|| town_target(terrain)),
        ),
        StoryPlace::Hall => {
            village_target(terrain, origin, VillageSetting::Underground, home, skip)
                .or_else(|| feature_fallback(FeatureKind::LargeCavern, (2, 4)))
        }
        StoryPlace::Camp => Some(
            camp_target(terrain, origin, home)
                .or_else(|| village_target(terrain, origin, VillageSetting::Surface, home, skip))
                .unwrap_or_else(|| town_target(terrain)),
        ),
        StoryPlace::Town => Some(town_target(terrain)),
        StoryPlace::Home => Some(home_target(terrain, home)),
        StoryPlace::SkyIsland => Some(
            island_target(terrain, origin, skip).unwrap_or_else(|| ridge_target(terrain, origin)),
        ),
        StoryPlace::SkyContinent => Some(
            sky_target(
                terrain,
                origin,
                FeatureKind::SkyContinent,
                STORY_CONTINENT_COLUMNS,
                skip,
            )
            .map(|(target, bound)| match bound {
                BoundTarget::Feature(feature) => {
                    let feature = in_sky_city(terrain, feature);
                    (target_of(&feature), BoundTarget::Feature(feature))
                }
                other => (target, other),
            })
            .or_else(|| {
                sky_target(
                    terrain,
                    origin,
                    FeatureKind::SkyIsland,
                    STORY_ISLAND_COLUMNS,
                    skip,
                )
            })
            .unwrap_or_else(|| ridge_target(terrain, origin)),
        ),
        StoryPlace::Moon => crate::worldgen::sky::moon(terrain.settings())
            .map(|moon| {
                let feature = at_lunar_colony(terrain, moon.feature());
                (target_of(&feature), BoundTarget::Feature(feature))
            })
            .or_else(|| feature_fallback(FeatureKind::Chamber, (6, 9)))
            .or_else(|| feature_fallback(FeatureKind::LargeCavern, (4, 9))),
        StoryPlace::Tomb | StoryPlace::Crypt => {
            let style = if place == StoryPlace::Crypt {
                crate::tomb::SiteStyle::TopDown
            } else {
                crate::tomb::SiteStyle::SideView
            };
            let style = Some(style);
            tomb_target(terrain, origin, style)
                .or_else(|| tomb_target(terrain, origin, None))
                .or_else(|| poi_fallback(PoiKind::Vault, (2, 5)))
                .or_else(|| feature_fallback(FeatureKind::LargeCavern, (2, 4)))
                .or_else(|| feature_fallback(FeatureKind::Chamber, (2, 9)))
        }
        StoryPlace::Ruin => ruin_target(terrain, origin)
            .or_else(|| poi_fallback(PoiKind::DerelictRig, (1, 4)))
            .or_else(|| feature_fallback(FeatureKind::LargeCavern, (2, 4)))
            .or_else(|| feature_fallback(FeatureKind::Chamber, (2, 9))),
        StoryPlace::NightMarket => {
            Some(market_target(terrain, origin, home, skip).unwrap_or_else(|| town_target(terrain)))
        }
        StoryPlace::Homestead => {
            Some(homestead_target(terrain, origin).unwrap_or_else(|| home_target(terrain, home)))
        }
        StoryPlace::SkyTown => Some(
            sky_town_target(terrain, origin)
                .or_else(|| island_target(terrain, origin, skip))
                .unwrap_or_else(|| ridge_target(terrain, origin)),
        ),
        StoryPlace::FathersSite => fathers_site_target(terrain, origin)
            .or_else(|| feature_fallback(FeatureKind::Chamber, (2, 9)))
            .or_else(|| feature_fallback(FeatureKind::LargeCavern, (2, 4))),
        StoryPlace::DeepSeam => deep_seam_target(terrain, origin)
            .or_else(|| feature_fallback(FeatureKind::LargeCavern, (2, 4)))
            .or_else(|| feature_fallback(FeatureKind::Chamber, (2, 9))),
    }
}

/// Binds `def` to the world: one place per target and the progress (with marked spots) of
/// every objective. `None` when a target has no candidate in this world.
///
/// `salt` 0 binds the best candidates (nearest unvisited); board postings pass their own salt to
/// spread across the closest few. `taken` are features other active quests use (avoided when
/// possible).
#[must_use]
pub fn bind_quest(
    def: &QuestDef,
    terrain: &Terrain,
    origin: TilePosition,
    journal: &PoiJournal,
    salt: u64,
    taken: &BTreeSet<FeatureId>,
) -> Option<(Vec<QuestTarget>, Vec<ObjectiveProgress>)> {
    bind_quest_from(def, terrain, origin, journal, salt, taken, None)
        .map(|(targets, objectives, _)| (targets, objectives))
}

/// [`bind_quest`] for stories, from the giver's settlement.
///
/// `home` is the giver's settlement: story places search from `origin`, skip `home` where they
/// want somewhere else, and [`StoryPlace::Home`] is it. What each target was bound to comes
/// back too.
#[must_use]
pub fn bind_quest_from(
    def: &QuestDef,
    terrain: &Terrain,
    origin: TilePosition,
    journal: &PoiJournal,
    salt: u64,
    taken: &BTreeSet<FeatureId>,
    home: Option<VillageId>,
) -> Option<(Vec<QuestTarget>, Vec<ObjectiveProgress>, Vec<BoundTarget>)> {
    let mut used = taken.clone();
    let mut chosen = BTreeSet::new();
    let mut targets = Vec::with_capacity(def.targets.len());
    let mut bound = Vec::with_capacity(def.targets.len());
    for (index, spec) in def.targets.iter().enumerate() {
        if let TargetSpec::Story(place) = spec {
            let (target, what) = story_place(*place, terrain, origin, journal, home, &chosen)?;
            if let Some(feature) = target.feature {
                chosen.insert(feature);
                used.insert(feature);
            }
            targets.push(target);
            bound.push(what);
            continue;
        }
        let options: Vec<WorldFeature> = candidates(*spec, terrain, origin, journal, &used)
            .into_iter()
            .filter(|feature| !chosen.contains(&feature.id))
            .collect();
        if options.is_empty() {
            return None;
        }
        let spread = options.len().min(3) as u64;
        let pick = if salt == 0 {
            0
        } else {
            usize::try_from(hash2(salt, SALT_BOARD, i32::try_from(index).unwrap_or(0), 0) % spread)
                .unwrap_or(0)
        };
        let feature = options[pick];
        chosen.insert(feature.id);
        used.insert(feature.id);
        targets.push(target_of(&feature));
        bound.push(BoundTarget::Feature(feature));
    }
    let seed = terrain.seed() ^ salt ^ u64::from(def.id.0);
    let mut objectives = Vec::new();
    for objective in def.objectives() {
        objectives.push(objective_progress(def, objective, &targets, terrain, seed)?);
    }
    Some((targets, objectives, bound))
}

/// Fresh progress for one objective, with its spots laid out.
fn objective_progress(
    def: &QuestDef,
    objective: Objective,
    targets: &[QuestTarget],
    terrain: &Terrain,
    seed: u64,
) -> Option<ObjectiveProgress> {
    let target = |index: u8| targets.get(usize::from(index)).copied();
    let mut progress = ObjectiveProgress::default();
    match objective {
        Objective::Simultaneous {
            target: index,
            spots,
            ..
        } => {
            let center = target(index)?.center;
            progress.spots = simultaneous_spots(terrain, center, spots)?;
            progress.marks = vec![0; usize::from(spots)];
        }
        Objective::Detonate {
            target: index,
            spots,
        } => {
            let area = target(index)?.bounds.expanded(5);
            let mut options: Vec<TilePosition> = area
                .positions()
                .filter(|position| {
                    diggable_spot(terrain, *position) && terrain.is_solid_at(*position)
                })
                .collect();
            options.sort_by_key(|position| {
                (hash2(seed, SALT_SPOTS, position.x, position.y), *position)
            });
            for option in options {
                if progress.spots.len() == usize::from(spots) {
                    break;
                }
                if progress
                    .spots
                    .iter()
                    .all(|spot| tile_distance(*spot, option) >= DETONATE_SPACING)
                {
                    progress.spots.push(option);
                }
            }
            if progress.spots.len() < usize::from(spots) {
                return None;
            }
        }
        Objective::Relay { target: index, .. } => {
            target(index)?;
            let door = giver_door(def.giver);
            progress.position = Some(door);
            progress.anchor = door.y;
        }
        Objective::Escort { target: index, .. } => {
            let bound = target(index)?;
            let open = bound
                .bounds
                .positions()
                .filter(|position| !terrain.is_solid_at(*position))
                .min_by_key(|position| (distance_squared(*position, bound.center), *position));
            progress.position = Some(open.unwrap_or(bound.center));
        }
        Objective::HoldAndClimb { switch, goal, .. } => {
            let switch = target(switch)?;
            target(goal)?;
            progress.spots =
                vec![nearest_diggable(terrain, switch.center).unwrap_or(switch.center)];
            progress.marks = vec![0];
        }
        Objective::SplitDelivery { .. } => {
            progress.marks = vec![0; 2];
            progress.counts = vec![0; 2];
        }
        Objective::ReachDepth { row } => {
            progress.anchor = terrain.scaled_row(row);
        }
        other => {
            for index in other.targets() {
                target(index)?;
            }
        }
    }
    Some(progress)
}

/// Marked spots for a simultaneous objective: `spots` spots [`SIMULTANEOUS_SPREAD`] apart in a
/// row through `center`, slid inward when the row would leave the world, and stacked in a column
/// instead when a row does not fit (a feature under a vault or against the world's edge).
fn simultaneous_spots(
    terrain: &Terrain,
    center: TilePosition,
    spots: u8,
) -> Option<Vec<TilePosition>> {
    let count = i32::from(spots);
    let offsets: Vec<i32> = (0..count)
        .map(|spot| (2 * spot - (count - 1)) * SIMULTANEOUS_SPREAD / 2)
        .collect();
    let first = *offsets.first()?;
    let last = *offsets.last()?;
    let slide = |center: i32, low: i32, high: i32| {
        // Keep `center + first ..= center + last` inside `low..=high` where it fits.
        if center + first < low {
            low - first
        } else if center + last > high {
            high - last
        } else {
            center
        }
    };
    let row = TilePosition {
        x: slide(center.x, terrain.min_x() + 3, terrain.max_x() - 3),
        y: center.y,
    };
    let column = TilePosition {
        x: center.x,
        y: slide(center.y, SURFACE_ROW + 4, terrain.height() - 4),
    };
    let layouts = [(row, true), (column, false)];
    layouts.into_iter().find_map(|(origin, across)| {
        let mut laid: Vec<TilePosition> = Vec::with_capacity(offsets.len());
        for offset in &offsets {
            let wanted = if across {
                TilePosition {
                    x: origin.x + offset,
                    y: origin.y,
                }
            } else {
                TilePosition {
                    x: origin.x,
                    y: origin.y + offset,
                }
            };
            let spot = nearest_diggable(terrain, wanted)?;
            if laid.contains(&spot) {
                return None;
            }
            laid.push(spot);
        }
        Some(laid)
    })
}

/// One posting on the HQ board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardOffer {
    pub slot: u8,
    pub quest: QuestId,
    /// Which of the template's twists this posting carries.
    pub variant: u8,
    /// Binding salt (spreads postings over different places).
    pub salt: u64,
}

/// The board's postings for `day` in a world with `seed`: distinct templates, each with a
/// twist pick and a binding salt.
#[must_use]
pub fn board_offers(seed: u64, day: u32) -> Vec<BoardOffer> {
    if BOARD_TEMPLATES.is_empty() {
        return Vec::new();
    }
    let day = i32::try_from(day % 1_000_000).unwrap_or(0);
    let count = BOARD_TEMPLATES.len();
    let mut offers: Vec<BoardOffer> = Vec::new();
    for slot in 0..BOARD_SLOTS.min(u8::try_from(count).unwrap_or(u8::MAX)) {
        let hash = hash2(seed, SALT_BOARD, day, i32::from(slot));
        let mut index = usize::try_from(hash % count as u64).unwrap_or(0);
        while offers
            .iter()
            .any(|offer| offer.quest == BOARD_TEMPLATES[index].id)
        {
            index = (index + 1) % count;
        }
        let template = &BOARD_TEMPLATES[index];
        let twists = template.twists.len().max(1) as u64;
        offers.push(BoardOffer {
            slot,
            quest: template.id,
            variant: u8::try_from((hash >> 20) % twists).unwrap_or(0),
            salt: (hash | 1).rotate_left(7),
        });
    }
    offers
}

#[cfg(test)]
mod wave4_place_tests {
    //! Story places bound to what the wave 4 tracks placed (integration seams T21/T22 x T26).
    use std::collections::BTreeSet;

    use super::{BoundTarget, StoryPlace, story_place};
    use crate::{
        PlayerId, SimulationParams, WorldState,
        poi::PoiJournal,
        terrain::TilePosition,
        worldgen::{GeneratorVersion, WorldSettings, sky::continent},
    };

    fn v9_world() -> WorldState {
        let settings = WorldSettings {
            generator: GeneratorVersion::V9,
            ..WorldSettings::new(0x5EED)
        };
        WorldState::new_game_with_settings(PlayerId::new(1), &settings, SimulationParams::new(60))
    }

    #[test]
    fn the_moon_binds_at_the_colony_and_continents_in_their_sky_city() {
        let world = v9_world();
        let terrain = world.terrain();
        let origin = TilePosition { x: 0, y: 0 };
        let journal = PoiJournal::default();
        let (target, _) = story_place(
            StoryPlace::Moon,
            terrain,
            origin,
            &journal,
            None,
            &BTreeSet::new(),
        )
        .expect("the Moon");
        let layout = world.moon_layout().expect("a filled Moon");
        assert!(layout.colony_area().contains(target.center), "{target:?}");
        let (target, bound) = story_place(
            StoryPlace::SkyContinent,
            terrain,
            origin,
            &journal,
            None,
            &BTreeSet::new(),
        )
        .expect("a sky place");
        if let BoundTarget::Feature(feature) = bound
            && feature.kind == crate::worldgen::FeatureKind::SkyContinent
        {
            let city = continent::sky_city_at(terrain.settings(), target.center, 2)
                .expect("the target stands in the continent's sky city");
            assert_eq!(target.center.y, city.street_row - 1);
        }
    }
}
