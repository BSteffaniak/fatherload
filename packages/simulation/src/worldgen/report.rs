//! World statistics for tuning and fairness checks (`fatherload --worldgen-report`).

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{poi::PoiKind, terrain::TileKind};

use super::{
    Band, FeatureKind, GeneratedWorld, HazardKind, WorldSettings, band_at,
    deposits::{SPECK_MAX_TILES, ore_deposits, starter_field_value},
};

/// Statistics of one band of one world.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BandStats {
    pub tiles: u64,
    pub open_tiles: u64,
    pub ore_tiles: u64,
    pub ore_value: u64,
    pub artifacts: u64,
    pub artifact_value: u64,
    pub hazard_tiles: u64,
    /// Points of interest centred in the band.
    pub pois: u64,
    /// Ore deposits of at most [`SPECK_MAX_TILES`] tiles.
    pub ore_specks: u64,
}

/// Statistics of one world.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorldStats {
    pub width: i32,
    pub bands: [BandStats; 12],
    pub hazard_tiles: BTreeMap<HazardKind, u64>,
    pub hazard_clusters: BTreeMap<HazardKind, u64>,
    pub features: BTreeMap<FeatureKind, u64>,
    pub pois: BTreeMap<PoiKind, u64>,
    pub starter_field_value: u64,
}

impl WorldStats {
    /// Measures a generated world.
    #[must_use]
    pub fn measure(settings: &WorldSettings, world: &GeneratedWorld) -> Self {
        let mut stats = Self {
            width: world.width,
            ..Self::default()
        };
        let kind_at = |x: i32, y: i32| -> Option<TileKind> {
            if x < 0 || y < 0 || x >= world.width || y >= world.height {
                return None;
            }
            usize::try_from(y * world.width + x)
                .ok()
                .and_then(|index| world.tiles.get(index))
                .map(|tile| tile.kind)
        };
        for y in super::strata::SKY_ROWS..world.height {
            for x in 0..world.width {
                let Some(kind) = kind_at(x, y) else {
                    continue;
                };
                if kind == TileKind::Bedrock {
                    continue;
                }
                let band = &mut stats.bands[band_at(settings, x, y).index()];
                band.tiles += 1;
                match kind {
                    TileKind::Air => band.open_tiles += 1,
                    TileKind::Ore(mineral) => {
                        band.ore_tiles += 1;
                        band.ore_value += u64::from(mineral.value());
                    }
                    TileKind::Artifact(artifact) => {
                        band.artifacts += 1;
                        band.artifact_value += u64::from(artifact.value());
                    }
                    other => {
                        if let Some(hazard) = HazardKind::of_tile(other) {
                            band.hazard_tiles += 1;
                            *stats.hazard_tiles.entry(hazard).or_default() += 1;
                        }
                    }
                }
            }
        }
        for feature in world.features.all() {
            *stats.features.entry(feature.kind).or_default() += 1;
            if let super::FeatureDetail::Hazard(hazard) = feature.detail {
                *stats.hazard_clusters.entry(hazard).or_default() += 1;
            }
            if let Some(kind) = feature.poi_kind() {
                *stats.pois.entry(kind).or_default() += 1;
                stats.bands[feature.band.index()].pois += 1;
            }
        }
        for deposit in ore_deposits(world.width, world.height, kind_at, |_, _| true) {
            if deposit.len() <= SPECK_MAX_TILES {
                let (x, y) = deposit[0];
                stats.bands[band_at(settings, x, y).index()].ore_specks += 1;
            }
        }
        stats.starter_field_value = starter_field_value(kind_at);
        stats
    }

    /// Share of below-ground tiles that are open caves, per mille.
    #[must_use]
    pub fn cave_per_mille(&self) -> u64 {
        let (open, total) = self.bands.iter().fold((0, 0), |(open, total), band| {
            (open + band.open_tiles, total + band.tiles)
        });
        open * 1000 / total.max(1)
    }
}

/// Min / mean / max of one metric over many worlds.
#[derive(Clone, Copy, Debug, Default)]
struct Spread {
    min: u64,
    max: u64,
    sum: u64,
    count: u64,
}

impl Spread {
    const fn add(&mut self, value: u64) {
        if self.count == 0 || value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.sum += value;
        self.count += 1;
    }

    fn cell(&self) -> String {
        let mean = self.sum / self.count.max(1);
        format!("{:>7} {:>7} {:>7}", self.min, mean, self.max)
    }
}

/// Generates `seeds` worlds (seeds `first_seed..`) with `template`'s other settings and
/// formats their distributions as a text report.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one table per metric, gathered and printed in reading order"
)]
pub fn distribution_report(template: WorldSettings, first_seed: u64, seeds: u64) -> String {
    let mut ore = [Spread::default(); 12];
    let mut open = [Spread::default(); 12];
    let mut hazards = [Spread::default(); 12];
    let mut artifacts = [Spread::default(); 12];
    let mut pois = [Spread::default(); 12];
    let mut specks = [Spread::default(); 12];
    let mut band_tops = [Spread::default(); 12];
    let mut band_rows = [Spread::default(); 12];
    let mut poi_kinds: BTreeMap<PoiKind, Spread> = PoiKind::ALL
        .into_iter()
        .map(|kind| (kind, Spread::default()))
        .collect();
    let mut caves = Spread::default();
    let mut starter = Spread::default();
    let mut features: BTreeMap<FeatureKind, Spread> = FeatureKind::ALL
        .into_iter()
        .map(|kind| (kind, Spread::default()))
        .collect();
    let mut clusters: BTreeMap<HazardKind, Spread> = HazardKind::ALL
        .into_iter()
        .map(|kind| (kind, Spread::default()))
        .collect();
    for seed in first_seed..first_seed + seeds {
        let settings = WorldSettings { seed, ..template };
        let world = super::generate(&settings);
        let stats = WorldStats::measure(&settings, &world);
        for (index, band) in stats.bands.iter().enumerate() {
            ore[index].add(band.ore_value * 10 / band.tiles.max(1));
            open[index].add(band.open_tiles * 1000 / band.tiles.max(1));
            hazards[index].add(band.hazard_tiles);
            artifacts[index].add(band.artifacts);
            pois[index].add(band.pois);
            specks[index].add(band.ore_specks);
        }
        for band in Band::ALL {
            let (top, bottom) = super::strata::band_layout_rows(&settings, band);
            band_tops[band.index()].add(u64::try_from(top).unwrap_or_default());
            band_rows[band.index()].add(u64::try_from(bottom - top + 1).unwrap_or_default());
        }
        for (kind, spread) in &mut poi_kinds {
            spread.add(stats.pois.get(kind).copied().unwrap_or_default());
        }
        caves.add(stats.cave_per_mille());
        starter.add(stats.starter_field_value);
        for (kind, spread) in &mut features {
            spread.add(stats.features.get(kind).copied().unwrap_or_default());
        }
        for (kind, spread) in &mut clusters {
            spread.add(stats.hazard_clusters.get(kind).copied().unwrap_or_default());
        }
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "worldgen report: {seeds} seeds from {first_seed}, generator {}, size {}, richness {}, hazards {}",
        template.generator.name(),
        template.size.name(),
        template.richness.name(),
        template.hazards.name(),
    );
    let _ = writeln!(
        out,
        "world height {} rows{}",
        template.height_tiles(),
        if template.generator.is_deep() {
            " (bedrock floor at the bottom)"
        } else {
            ""
        }
    );
    let _ = writeln!(out, "(each cell: min mean max over seeds)\n");
    let _ = writeln!(
        out,
        "{:<22} {:^23} {:^23} {:^23} {:^23} {:^23} {:^23} {:^23} {:^23}",
        "band",
        "top row",
        "rows",
        "ore value / 10 tiles",
        "ore specks (<=3)",
        "caves (per mille)",
        "hazard tiles",
        "artifacts",
        "POIs"
    );
    for band in Band::ALL {
        let index = band.index();
        let _ = writeln!(
            out,
            "{:<22} {} {} {} {} {} {} {} {}",
            band.name(),
            band_tops[index].cell(),
            band_rows[index].cell(),
            ore[index].cell(),
            specks[index].cell(),
            open[index].cell(),
            hazards[index].cell(),
            artifacts[index].cell(),
            pois[index].cell(),
        );
    }
    let _ = writeln!(out, "\n{:<22} {}", "caves overall", caves.cell());
    let _ = writeln!(out, "{:<22} {}", "starter field value", starter.cell());
    let _ = writeln!(out, "\nfeatures");
    for (kind, spread) in &features {
        let _ = writeln!(out, "  {:<20} {}", kind.name(), spread.cell());
    }
    let _ = writeln!(out, "\npoints of interest");
    for (kind, spread) in &poi_kinds {
        let _ = writeln!(out, "  {:<20} {}", kind.name(), spread.cell());
    }
    let _ = writeln!(out, "\nhazard clusters");
    for (kind, spread) in &clusters {
        let _ = writeln!(out, "  {:<20} {}", kind.name(), spread.cell());
    }
    out
}
