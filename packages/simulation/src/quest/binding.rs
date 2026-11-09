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
    worldgen::{FeatureId, FeatureKind, WorldFeature, noise::hash2},
};

use super::{
    BOARD_TEMPLATES, Objective, ObjectiveProgress, QuestDef, QuestGiver, QuestId, QuestTarget,
    SURFACE_ROW, TargetSpec,
};

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
    if position.x < 2
        || position.x > terrain.width() - 3
        || position.y < SURFACE_ROW + 3
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
        FeatureKind::Chamber => Some(FeatureKind::LargeCavern),
        _ => None,
    }
}

/// Candidate features for `spec`, best first.
fn candidates<'a>(
    spec: TargetSpec,
    terrain: &'a Terrain,
    origin: TilePosition,
    journal: &PoiJournal,
    taken: &BTreeSet<FeatureId>,
) -> Vec<&'a WorldFeature> {
    let features = terrain.features();
    let (first, last) = spec.bands();
    let in_bands = |feature: &&WorldFeature| {
        let index = feature.band.index();
        index >= usize::from(first) && index <= usize::from(last)
    };
    let mut found: Vec<&WorldFeature> = match spec {
        TargetSpec::Poi { kind, unopened, .. } => {
            if !terrain.settings().generator.has_points_of_interest() {
                return Vec::new();
            }
            features
                .pois_of(kind)
                .filter(in_bands)
                .filter(|feature| !unopened || poi_unopened(terrain, feature))
                .collect()
        }
        TargetSpec::Feature { kind, .. } => {
            let found: Vec<&WorldFeature> = features.of_kind(kind).filter(in_bands).collect();
            if found.is_empty() {
                // Lean or unlucky worlds can lack a feature kind in the wanted bands (shallow
                // motherlodes especially): fall back to the nearest bands that have one rather
                // than leave the quest unbindable.
                let band_gap = |feature: &&WorldFeature| {
                    let index = feature.band.index();
                    usize::from(first)
                        .saturating_sub(index)
                        .max(index.saturating_sub(usize::from(last)))
                };
                // A world without any of the kind (some lean worlds roll no motherlode at
                // all) uses its closest stand-in.
                let mut all: Vec<&WorldFeature> = features.of_kind(kind).collect();
                if all.is_empty()
                    && let Some(stand_in) = stand_in(kind)
                {
                    all = features.of_kind(stand_in).collect();
                }
                let nearest = all.iter().map(band_gap).min();
                all.into_iter()
                    .filter(|feature| Some(band_gap(feature)) == nearest)
                    .collect()
            } else {
                found
            }
        }
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
    let mut used = taken.clone();
    let mut chosen = BTreeSet::new();
    let mut targets = Vec::with_capacity(def.targets.len());
    for (index, spec) in def.targets.iter().enumerate() {
        let options: Vec<&WorldFeature> = candidates(*spec, terrain, origin, journal, &used)
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
        targets.push(QuestTarget {
            feature: Some(feature.id),
            center: feature.center,
            bounds: feature.bounds,
        });
    }
    let seed = terrain.seed() ^ salt ^ u64::from(def.id.0);
    let mut objectives = Vec::new();
    for objective in def.objectives() {
        objectives.push(objective_progress(def, objective, &targets, terrain, seed)?);
    }
    Some((targets, objectives))
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
        x: slide(center.x, 3, terrain.width() - 4),
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
