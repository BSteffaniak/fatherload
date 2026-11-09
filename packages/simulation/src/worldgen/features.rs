//! The feature index: notable places in a generated world (caverns, motherlodes, artifact beds,
//! hazard clusters, band transitions, points of interest and reserved prefab slots).
//!
//! It is a deterministic by-product of generation, recomputable from [`super::WorldSettings`],
//! so it is never replicated or saved. Later systems (quests, POI prefabs, scanners) bind to it
//! by kind, depth or region.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::{
    poi::{PoiKind, PoiVariant},
    terrain::{ArtifactKind, MineralKind, TileKind, TilePosition},
};

use super::strata::Band;

/// Stable id of a feature within one world (its index in generation order).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct FeatureId(pub u32);

/// What a feature is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FeatureKind {
    /// A large connected open cave system.
    LargeCavern,
    /// A big hollow carved deep down (deep strata only).
    Chamber,
    /// A rare, dense deposit of one valuable mineral.
    Motherlode,
    /// A small cluster of artifacts.
    ArtifactBed,
    /// A pocket of clustered hazard tiles.
    HazardCluster,
    /// Where one band gives way to the next.
    StratumTransition,
    /// A spot reserved for a future hand-authored prefab room (left untouched for now).
    PrefabSlot,
    /// A stamped prefab point of interest (see [`crate::poi`]); its detail is
    /// [`FeatureDetail::Poi`].
    PointOfInterest,
}

impl FeatureKind {
    pub const ALL: [Self; 8] = [
        Self::LargeCavern,
        Self::Chamber,
        Self::Motherlode,
        Self::ArtifactBed,
        Self::HazardCluster,
        Self::StratumTransition,
        Self::PrefabSlot,
        Self::PointOfInterest,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LargeCavern => "large cavern",
            Self::Chamber => "chamber",
            Self::Motherlode => "motherlode",
            Self::ArtifactBed => "artifact bed",
            Self::HazardCluster => "hazard cluster",
            Self::StratumTransition => "stratum transition",
            Self::PrefabSlot => "prefab slot",
            Self::PointOfInterest => "point of interest",
        }
    }
}

/// Hazard families a [`FeatureKind::HazardCluster`] can be made of.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HazardKind {
    Gas,
    Lava,
    MagmaVent,
    Pressure,
    Explosive,
}

impl HazardKind {
    pub const ALL: [Self; 5] = [
        Self::Gas,
        Self::Lava,
        Self::MagmaVent,
        Self::Pressure,
        Self::Explosive,
    ];

    #[must_use]
    pub const fn tile(self) -> TileKind {
        match self {
            Self::Gas => TileKind::Gas,
            Self::Lava => TileKind::Lava,
            Self::MagmaVent => TileKind::MagmaVent,
            Self::Pressure => TileKind::PressurePocket,
            Self::Explosive => TileKind::ExplosivePocket,
        }
    }

    /// The hazard family of a tile, if it is one.
    #[must_use]
    pub const fn of_tile(kind: TileKind) -> Option<Self> {
        match kind {
            TileKind::Gas => Some(Self::Gas),
            TileKind::Lava => Some(Self::Lava),
            TileKind::MagmaVent => Some(Self::MagmaVent),
            TileKind::PressurePocket => Some(Self::Pressure),
            TileKind::ExplosivePocket => Some(Self::Explosive),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gas => "gas",
            Self::Lava => "lava",
            Self::MagmaVent => "magma vent",
            Self::Pressure => "pressure",
            Self::Explosive => "explosive",
        }
    }
}

/// Extra detail about a feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FeatureDetail {
    None,
    Mineral(MineralKind),
    Artifact(ArtifactKind),
    Hazard(HazardKind),
    Transition { upper: Band, lower: Band },
    Poi(PoiVariant),
}

impl FeatureDetail {
    /// The POI variant, if this is a point of interest.
    #[must_use]
    pub const fn poi(self) -> Option<PoiVariant> {
        match self {
            Self::Poi(variant) => Some(variant),
            _ => None,
        }
    }
}

/// An inclusive tile rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TileRect {
    pub min: TilePosition,
    pub max: TilePosition,
}

impl TileRect {
    #[must_use]
    pub const fn around(center: TilePosition, half_width: i32, half_height: i32) -> Self {
        Self {
            min: TilePosition {
                x: center.x - half_width,
                y: center.y - half_height,
            },
            max: TilePosition {
                x: center.x + half_width,
                y: center.y + half_height,
            },
        }
    }

    #[must_use]
    pub const fn contains(&self, position: TilePosition) -> bool {
        position.x >= self.min.x
            && position.x <= self.max.x
            && position.y >= self.min.y
            && position.y <= self.max.y
    }

    #[must_use]
    pub const fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.max.x - self.min.x + 1
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.max.y - self.min.y + 1
    }

    /// The centre tile (rounded towards the top-left).
    #[must_use]
    pub const fn center(&self) -> TilePosition {
        TilePosition {
            x: i32::midpoint(self.min.x, self.max.x),
            y: i32::midpoint(self.min.y, self.max.y),
        }
    }

    /// The rectangle grown by `margin` tiles on every side.
    #[must_use]
    pub const fn expanded(&self, margin: i32) -> Self {
        Self {
            min: TilePosition {
                x: self.min.x - margin,
                y: self.min.y - margin,
            },
            max: TilePosition {
                x: self.max.x + margin,
                y: self.max.y + margin,
            },
        }
    }

    /// Every tile position inside, row by row.
    pub fn positions(&self) -> impl Iterator<Item = TilePosition> + use<> {
        let (min, max) = (self.min, self.max);
        (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| TilePosition { x, y }))
    }
}

/// One notable place in a world.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct WorldFeature {
    pub id: FeatureId,
    pub kind: FeatureKind,
    pub detail: FeatureDetail,
    /// The band the feature's centre lies in.
    pub band: Band,
    pub center: TilePosition,
    pub bounds: TileRect,
    /// Tiles the feature covers (open tiles of a cavern, ore of a motherlode, ...).
    pub size: u32,
}

impl WorldFeature {
    /// The POI variant, if this feature is a point of interest.
    #[must_use]
    pub const fn poi(&self) -> Option<PoiVariant> {
        self.detail.poi()
    }

    /// The POI kind, if this feature is a point of interest.
    #[must_use]
    pub const fn poi_kind(&self) -> Option<PoiKind> {
        match self.detail.poi() {
            Some(variant) => Some(variant.kind()),
            None => None,
        }
    }
}

/// Every notable feature of one world, in deterministic generation order.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct FeatureIndex {
    features: Vec<WorldFeature>,
}

impl FeatureIndex {
    pub(crate) fn push(
        &mut self,
        kind: FeatureKind,
        detail: FeatureDetail,
        band: Band,
        center: TilePosition,
        bounds: TileRect,
        size: u32,
    ) -> FeatureId {
        let id = FeatureId(u32::try_from(self.features.len()).unwrap_or(u32::MAX));
        self.features.push(WorldFeature {
            id,
            kind,
            detail,
            band,
            center,
            bounds,
            size,
        });
        id
    }

    /// Every feature, in id order.
    #[must_use]
    pub fn all(&self) -> &[WorldFeature] {
        &self.features
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.features.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: FeatureId) -> Option<&WorldFeature> {
        self.features.get(id.0 as usize)
    }

    /// Features of one kind.
    pub fn of_kind(&self, kind: FeatureKind) -> impl Iterator<Item = &WorldFeature> {
        self.features
            .iter()
            .filter(move |feature| feature.kind == kind)
    }

    #[must_use]
    pub fn count(&self, kind: FeatureKind) -> usize {
        self.of_kind(kind).count()
    }

    /// Features whose centre lies in `band`.
    pub fn in_band(&self, band: Band) -> impl Iterator<Item = &WorldFeature> {
        self.features
            .iter()
            .filter(move |feature| feature.band == band)
    }

    /// Features whose centre row lies in `rows`.
    pub fn in_depth(&self, rows: RangeInclusive<i32>) -> impl Iterator<Item = &WorldFeature> {
        self.features
            .iter()
            .filter(move |feature| rows.contains(&feature.center.y))
    }

    /// Features whose bounds overlap `region`.
    pub fn in_region(&self, region: TileRect) -> impl Iterator<Item = &WorldFeature> {
        self.features
            .iter()
            .filter(move |feature| feature.bounds.intersects(&region))
    }

    /// Every point of interest, in id order.
    pub fn pois(&self) -> impl Iterator<Item = &WorldFeature> {
        self.of_kind(FeatureKind::PointOfInterest)
    }

    /// Points of interest of one kind.
    pub fn pois_of(&self, kind: PoiKind) -> impl Iterator<Item = &WorldFeature> {
        self.pois()
            .filter(move |feature| feature.poi_kind() == Some(kind))
    }

    /// Points of interest whose centre lies in `band`.
    pub fn pois_in_band(&self, band: Band) -> impl Iterator<Item = &WorldFeature> {
        self.pois().filter(move |feature| feature.band == band)
    }

    /// The point of interest whose bounds contain `position` (lowest id if several overlap).
    #[must_use]
    pub fn poi_at(&self, position: TilePosition) -> Option<&WorldFeature> {
        self.pois()
            .find(|feature| feature.bounds.contains(position))
    }

    /// The point of interest (of `kind`, or any kind) whose centre is nearest `position`
    /// (ties: lowest id).
    #[must_use]
    pub fn nearest_poi(
        &self,
        position: TilePosition,
        kind: Option<PoiKind>,
    ) -> Option<&WorldFeature> {
        self.pois()
            .filter(|feature| kind.is_none_or(|kind| feature.poi_kind() == Some(kind)))
            .min_by_key(|feature| distance_squared(feature.center, position))
    }

    /// The feature of `kind` whose centre is nearest `position` (ties: lowest id).
    #[must_use]
    pub fn nearest(&self, position: TilePosition, kind: FeatureKind) -> Option<&WorldFeature> {
        self.of_kind(kind)
            .min_by_key(|feature| distance_squared(feature.center, position))
    }
}

fn distance_squared(a: TilePosition, b: TilePosition) -> i64 {
    let dx = i64::from(a.x - b.x);
    let dy = i64::from(a.y - b.y);
    dx * dx + dy * dy
}
