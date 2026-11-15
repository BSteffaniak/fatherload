//! The regional feature index of a streamed world.
//!
//! A streamed world has no finite list of features. Every feature is owned by one region of
//! one placement layer and its id encodes that region ([`feature_id`]), so it can be looked up
//! again from the id alone ([`feature`]). Area queries ([`features_in`]) gather the regions an
//! area overlaps; "nearest X" queries ([`nearest_feature`]) search outward in column strips
//! until nothing unseen can be nearer, so quests and later systems bind anywhere.

use crate::{
    terrain::TilePosition,
    worldgen::{
        FeatureId, FeatureKind,
        features::{FeatureDetail, TileRect, WorldFeature},
        strata::{self, BOUNDARY_COUNT, Band},
    },
};

use super::{
    base::Ctx,
    cave_biomes::{CAVE_REGION, cave_region},
    regions::{
        self, BED_REGION, CAVERN_REGION, CHAMBER_REGION, HAZARD_REGION, MOTHERLODE_REGION,
        POI_REGION_WIDTH, TRANSITION_REGION_WIDTH, bed_region, cavern_region, chamber_region,
        hazard_region, motherlode_region, poi_region, span,
    },
};

/// The placement layer a regional feature comes from (part of its id).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Layer {
    Chamber = 1,
    Hazard = 2,
    Poi = 3,
    Motherlode = 4,
    Bed = 5,
    Transition = 6,
    Cavern = 7,
    Cave = 8,
    /// Villages ([`crate::village`]): `rx` is the site index, `ry` 0 the surface village and
    /// 1 the underground settlement.
    Village = 9,
}

impl Layer {
    const ALL: [Self; 9] = [
        Self::Chamber,
        Self::Hazard,
        Self::Poi,
        Self::Motherlode,
        Self::Bed,
        Self::Transition,
        Self::Cavern,
        Self::Cave,
        Self::Village,
    ];

    const fn kind(self) -> FeatureKind {
        match self {
            Self::Chamber => FeatureKind::Chamber,
            Self::Hazard => FeatureKind::HazardCluster,
            Self::Poi => FeatureKind::PointOfInterest,
            Self::Motherlode => FeatureKind::Motherlode,
            Self::Bed => FeatureKind::ArtifactBed,
            Self::Transition => FeatureKind::StratumTransition,
            Self::Cavern => FeatureKind::LargeCavern,
            Self::Cave => FeatureKind::Cave,
            Self::Village => FeatureKind::Village,
        }
    }
}

/// Marks a regional id (bounded worlds number their features from 0).
const REGIONAL: u64 = 1 << 63;

/// The id of feature `local` of region `(rx, ry)` of `layer`.
///
/// Layout: bit 63 set, layer in bits 59..63, `rx` (32 bits) in 27..59, `ry` (16 bits) in
/// 11..27 and `local` (11 bits) in 0..11.
#[must_use]
pub fn feature_id(layer: Layer, rx: i32, ry: i32, local: u16) -> FeatureId {
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "coordinates are packed as raw bits"
    )]
    let (rx, ry) = (u64::from(rx as u32), u64::from(ry as u16));
    FeatureId(
        REGIONAL
            | (u64::from(layer as u8) << 59)
            | (rx << 27)
            | (ry << 11)
            | u64::from(local & 0x7FF),
    )
}

/// The layer, region and local index a regional id names.
#[must_use]
pub fn decode(id: FeatureId) -> Option<(Layer, i32, i32, u16)> {
    if id.0 & REGIONAL == 0 {
        return None;
    }
    let code = u8::try_from((id.0 >> 59) & 0xF).ok()?;
    let layer = Layer::ALL.into_iter().find(|layer| *layer as u8 == code)?;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "coordinates are unpacked from raw bits"
    )]
    let (rx, ry) = (
        ((id.0 >> 27) & 0xFFFF_FFFF) as u32 as i32,
        ((id.0 >> 11) & 0xFFFF) as u16 as i16,
    );
    let local = u16::try_from(id.0 & 0x7FF).ok()?;
    Some((layer, rx, i32::from(ry), local))
}

/// Every feature region `(rx, ry)` of `layer` overlapping `rect`, with its features.
fn layer_features(ctx: &Ctx, layer: Layer, rect: &TileRect, out: &mut Vec<WorldFeature>) {
    let rows = (rect.min.y.max(0), rect.max.y.min(ctx.height - 1));
    if rows.0 > rows.1 {
        return;
    }
    let grid = |size: (i32, i32)| {
        span(rows.0, rows.1, size.1)
            .flat_map(move |ry| span(rect.min.x, rect.max.x, size.0).map(move |rx| (rx, ry)))
    };
    match layer {
        Layer::Chamber => {
            for (rx, ry) in grid(CHAMBER_REGION) {
                out.extend(chamber_region(ctx, rx, ry).feature);
            }
        }
        Layer::Hazard => {
            for (rx, ry) in grid(HAZARD_REGION) {
                out.extend(
                    hazard_region(ctx, rx, ry)
                        .clusters
                        .iter()
                        .map(|cluster| cluster.feature),
                );
            }
        }
        Layer::Poi => {
            for (index, tier) in ctx.tiers.iter().enumerate() {
                if tier.bottom < rows.0 || tier.top > rows.1 {
                    continue;
                }
                for rx in span(rect.min.x, rect.max.x, POI_REGION_WIDTH) {
                    out.extend(
                        poi_region(ctx, rx, index)
                            .poi
                            .as_ref()
                            .map(|poi| poi.feature),
                    );
                }
            }
        }
        Layer::Motherlode => {
            for (rx, ry) in grid(MOTHERLODE_REGION) {
                out.extend(motherlode_region(ctx, rx, ry).feature);
            }
        }
        Layer::Bed => {
            for (rx, ry) in grid(BED_REGION) {
                out.extend(bed_region(ctx, rx, ry).feature);
            }
        }
        Layer::Cavern => {
            for (rx, ry) in grid(CAVERN_REGION) {
                out.extend(cavern_region(ctx, rx, ry).feature);
            }
        }
        Layer::Cave => {
            if ctx.settings.generator.has_cave_biomes() {
                for (rx, ry) in grid(CAVE_REGION) {
                    out.extend(
                        cave_region(ctx, rx, ry)
                            .biome
                            .as_ref()
                            .map(|biome| biome.feature),
                    );
                }
            }
        }
        Layer::Village => super::villages::layer_features(ctx, rect, out),
        Layer::Transition => {
            for rx in span(rect.min.x, rect.max.x, TRANSITION_REGION_WIDTH) {
                for index in 0..BOUNDARY_COUNT {
                    out.push(transition(ctx, rx, index));
                }
            }
        }
    }
}

/// Where boundary `index` runs deepest in transition region `rx`.
fn transition(ctx: &Ctx, rx: i32, index: usize) -> WorldFeature {
    let first = rx * TRANSITION_REGION_WIDTH;
    let last = first + TRANSITION_REGION_WIDTH - 1;
    let depths =
        (first..=last).map(|x| (x, strata::boundary_column_depth(&ctx.settings, index, x)));
    let (mut deepest_x, mut deepest, mut shallowest) = (first, i32::MIN, i32::MAX);
    for (x, depth) in depths {
        if depth > deepest {
            (deepest_x, deepest) = (x, depth);
        }
        shallowest = shallowest.min(depth);
    }
    let upper = Band::from_index(index);
    let lower = Band::from_index(index + 1);
    WorldFeature {
        id: feature_id(Layer::Transition, rx, i32::try_from(index).unwrap_or(0), 0),
        kind: FeatureKind::StratumTransition,
        detail: FeatureDetail::Transition { upper, lower },
        band: lower,
        center: TilePosition {
            x: deepest_x,
            y: deepest,
        },
        bounds: TileRect {
            min: TilePosition {
                x: first,
                y: shallowest,
            },
            max: TilePosition {
                x: last,
                y: deepest,
            },
        },
        size: u32::try_from(TRANSITION_REGION_WIDTH).unwrap_or(0),
    }
}

fn layers_for(kinds: Option<&[FeatureKind]>) -> impl Iterator<Item = Layer> + '_ {
    Layer::ALL
        .into_iter()
        .filter(move |layer| kinds.is_none_or(|kinds| kinds.contains(&layer.kind())))
}

/// Every feature (of `kinds`, or of every kind) whose centre lies in `rect`, in id order.
#[must_use]
pub fn features_in(ctx: &Ctx, rect: &TileRect, kinds: Option<&[FeatureKind]>) -> Vec<WorldFeature> {
    let mut found = Vec::new();
    for layer in layers_for(kinds) {
        layer_features(ctx, layer, rect, &mut found);
    }
    found.retain(|feature| rect.contains(feature.center));
    found.sort_by_key(|feature| feature.id);
    found.dedup_by_key(|feature| feature.id);
    found
}

/// The points of interest whose bounds overlap `rect`, in id order.
#[must_use]
pub fn pois_touching(ctx: &Ctx, rect: &TileRect) -> Vec<WorldFeature> {
    let mut found: Vec<WorldFeature> = regions::poi_regions_in(ctx, rect)
        .iter()
        .filter_map(|region| region.poi.as_ref().map(|poi| poi.feature))
        .filter(|feature| feature.bounds.intersects(rect))
        .collect();
    found.sort_by_key(|feature| feature.id);
    found
}

/// The feature with regional id `id`, recomputed from its region.
#[must_use]
pub fn feature(ctx: &Ctx, id: FeatureId) -> Option<WorldFeature> {
    let (layer, rx, ry, local) = decode(id)?;
    let found = match layer {
        Layer::Chamber => chamber_region(ctx, rx, ry).feature,
        Layer::Hazard => hazard_region(ctx, rx, ry)
            .clusters
            .get(usize::from(local))
            .map(|cluster| cluster.feature),
        Layer::Poi => {
            let tier = ctx.tiers.iter().position(|tier| tier.key == ry)?;
            poi_region(ctx, rx, tier)
                .poi
                .as_ref()
                .map(|poi| poi.feature)
        }
        Layer::Motherlode => motherlode_region(ctx, rx, ry).feature,
        Layer::Bed => bed_region(ctx, rx, ry).feature,
        Layer::Cavern => cavern_region(ctx, rx, ry).feature,
        Layer::Cave => cave_region(ctx, rx, ry)
            .biome
            .as_ref()
            .map(|biome| biome.feature),
        Layer::Village => super::villages::feature(ctx, rx, ry),
        Layer::Transition => {
            let index = usize::try_from(ry)
                .ok()
                .filter(|index| *index < BOUNDARY_COUNT)?;
            Some(transition(ctx, rx, index))
        }
    };
    found.filter(|feature| feature.id == id)
}

/// Columns searched per step of [`nearest_feature`].
const SEARCH_STEP: i32 = 64;

fn distance_squared(a: TilePosition, b: TilePosition) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    dx * dx + dy * dy
}

/// The feature of `kind` accepted by `accept` whose centre is nearest `from` (ties: lowest id),
/// searching at most `max_columns` columns either side.
pub fn nearest_feature(
    ctx: &Ctx,
    from: TilePosition,
    kind: FeatureKind,
    max_columns: i32,
    mut accept: impl FnMut(&WorldFeature) -> bool,
) -> Option<WorldFeature> {
    let kinds = [kind];
    let full_rows = |min_x: i32, max_x: i32| TileRect {
        min: TilePosition { x: min_x, y: 0 },
        max: TilePosition {
            x: max_x,
            y: ctx.height - 1,
        },
    };
    let mut best: Option<(i64, WorldFeature)> = None;
    let mut consider = |features: Vec<WorldFeature>, best: &mut Option<(i64, WorldFeature)>| {
        for feature in features {
            if !accept(&feature) {
                continue;
            }
            let key = (distance_squared(feature.center, from), feature.id);
            if best
                .as_ref()
                .is_none_or(|(distance, current)| key < (*distance, current.id))
            {
                *best = Some((key.0, feature));
            }
        }
    };
    let mut radius = SEARCH_STEP.min(max_columns.max(0));
    consider(
        features_in(
            ctx,
            &full_rows(from.x - radius, from.x + radius),
            Some(&kinds),
        ),
        &mut best,
    );
    loop {
        if best
            .as_ref()
            .is_some_and(|(distance, _)| *distance <= i64::from(radius) * i64::from(radius))
            || radius >= max_columns
        {
            break;
        }
        let next = (radius + SEARCH_STEP).min(max_columns);
        consider(
            features_in(
                ctx,
                &full_rows(from.x - next, from.x - radius - 1),
                Some(&kinds),
            ),
            &mut best,
        );
        consider(
            features_in(
                ctx,
                &full_rows(from.x + radius + 1, from.x + next),
                Some(&kinds),
            ),
            &mut best,
        );
        radius = next;
    }
    best.map(|(_, feature)| feature)
}
