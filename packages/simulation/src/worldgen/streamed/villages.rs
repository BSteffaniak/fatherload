//! Villages in streamed worlds ([`crate::village`]): resolved site geometry, the tiles each
//! village stamps into the terrain, its regional features and the memoized villages themselves.
//!
//! Three caches, each a pure function of its key:
//!
//! * **geometry** per site: the surface village's span and ground row, and the underground
//!   settlement's hall slid clear of points of interest (cheap: what the feature index and
//!   position lookups read);
//! * **villages** per id: the full [`Village`] (layout grammar, residents);
//! * **stamps** per site: the foundation, paving and hall tiles chunk assembly writes, and the
//!   rectangles in which it turns hazard tiles back into plain rock so a village never sits on
//!   lava.

use std::sync::Arc;

use crate::{
    people::{VillageId, WorldFact},
    terrain::{TileKind, TilePosition},
    village::{
        Village,
        layout::{self, SiteSpec},
        placement::{
            self, FLATTEN_FADE_COLUMNS, HALL_AIR_ROWS, HALL_WIDTH, SurfaceSite, UndergroundSite,
        },
    },
    worldgen::{
        FeatureDetail, FeatureKind, WorldSettings,
        features::{TileRect, WorldFeature},
        noise::hash2,
    },
};

use super::{
    base::Ctx,
    cache::MemoCache,
    ctx,
    index::{Layer, feature_id},
};

/// Columns any part of a site's villages may lie from its nominal centre (the underground hall
/// is the furthest out), so area queries know which sites to consult.
pub const SITE_REACH: i32 = 600;
/// Rows under a surface village's ground in which hazards are turned back into rock.
const CLEAR_ROWS: i32 = 8;
const SALT_FOUNDATION: u64 = 0x7111_A6E5_0301;

/// A site's resolved geometry.
#[derive(Clone, Copy, Debug)]
pub struct SiteGeometry {
    pub surface: SurfaceSite,
    pub ground_row: i32,
    /// The underground settlement and its hall's resolved centre column.
    pub underground: Option<(UndergroundSite, i32)>,
}

impl SiteGeometry {
    /// The surface village's area (see [`Village::area`]).
    const fn surface_area(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.surface.first,
                y: self.ground_row - 24,
            },
            max: TilePosition {
                x: self.surface.last,
                y: self.ground_row + 1,
            },
        }
    }

    /// The underground hall's air (see [`Village::area`]).
    fn hall_area(&self) -> Option<TileRect> {
        self.underground
            .map(|(site, center)| hall_rect(center, site.top))
    }
}

/// The air of a hall centred on column `center` whose first air row is `top`, down to its
/// floor row.
const fn hall_rect(center: i32, top: i32) -> TileRect {
    let first = center - HALL_WIDTH / 2;
    TileRect {
        min: TilePosition { x: first, y: top },
        max: TilePosition {
            x: first + HALL_WIDTH - 1,
            y: top + HALL_AIR_ROWS,
        },
    }
}

/// Tiles a village writes into generated terrain, and where hazards are cleared first.
#[derive(Debug, Default)]
pub struct VillageStamp {
    pub clear: Vec<TileRect>,
    pub cells: Vec<(i32, i32, TileKind)>,
    /// Everything the stamp touches.
    pub bounds: Option<TileRect>,
}

static GEOMETRY: MemoCache<Option<SiteGeometry>> = MemoCache::new(256);
static VILLAGES: MemoCache<Option<Arc<Village>>> = MemoCache::new(64);
static STAMPS: MemoCache<VillageStamp> = MemoCache::new(64);
static FACTS: MemoCache<Vec<WorldFact>> = MemoCache::new(64);

/// Site `site`'s geometry (`None` for sites the world does not place, or a world without
/// villages).
pub fn geometry(ctx: &Ctx, site: i32) -> Option<SiteGeometry> {
    if !ctx.settings.generator.has_villages() || !placement::valid_site(site) {
        return None;
    }
    *GEOMETRY.get_or_compute((ctx.settings, site, 0), || {
        let surface = placement::surface_site(ctx.seed(), site)?;
        let ground_row = super::base::village_ground_row(&ctx.settings, surface.center);
        let underground = placement::underground_site(&ctx.settings, site)
            .and_then(|nominal| resolve_hall(ctx, nominal).map(|center| (nominal, center)));
        Some(SiteGeometry {
            surface,
            ground_row,
            underground,
        })
    })
}

/// Slides an underground hall clear of points of interest: its nominal column first, then up
/// to three steps its preferred way, then three the other way.
fn resolve_hall(ctx: &Ctx, site: UndergroundSite) -> Option<i32> {
    let step = HALL_WIDTH + 24;
    let floor = ctx.floor_row(site.center);
    if site.top + HALL_AIR_ROWS + 3 >= floor {
        return None;
    }
    let candidates = [0, 1, 2, 3, -1, -2, -3].map(|k| site.center + site.slide * k * step);
    candidates.into_iter().find(|center| {
        let rect = hall_rect(*center, site.top).expanded(6);
        super::index::pois_touching(ctx, &rect).is_empty()
    })
}

/// The sites whose villages may reach columns `first..=last`.
fn sites_reaching(first: i32, last: i32) -> impl Iterator<Item = i32> {
    let (low, high) = placement::sites_between(first - SITE_REACH, last + SITE_REACH);
    (low..=high).filter(|site| placement::valid_site(*site))
}

/// The village `id`, built on first use.
pub fn village(settings: &WorldSettings, id: VillageId) -> Option<Arc<Village>> {
    let ctx = ctx(settings);
    let (site, underground) = placement::decode_id(id);
    let geometry = geometry(&ctx, site)?;
    if underground && geometry.underground.is_none() {
        return None;
    }
    let built = VILLAGES.get_or_compute((*settings, site, i32::from(underground)), || {
        let spec = if underground {
            let (site, center) = geometry.underground?;
            SiteSpec::Underground { site, center }
        } else {
            SiteSpec::Surface {
                site: geometry.surface,
                ground_row: geometry.ground_row,
            }
        };
        Some(Arc::new(layout::build(settings, &spec)))
    });
    built.as_ref().clone()
}

/// Every village whose centre column lies in `first..=last`.
pub fn villages_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<Arc<Village>> {
    let ctx = ctx(settings);
    let mut found = Vec::new();
    for site in sites_reaching(first, last) {
        let Some(geometry) = geometry(&ctx, site) else {
            continue;
        };
        if (first..=last).contains(&geometry.surface.center) {
            found.extend(village(settings, geometry.surface.id()));
        }
        if let Some((hall, center)) = geometry.underground
            && (first..=last).contains(&center)
        {
            found.extend(village(settings, hall.id()));
        }
    }
    found
}

/// The village whose area contains `position`.
pub fn village_containing(
    settings: &WorldSettings,
    position: TilePosition,
) -> Option<Arc<Village>> {
    let ctx = ctx(settings);
    if let Some(site) = placement::surface_site_at(ctx.seed(), position.x, 0)
        && let Some(geometry) = geometry(&ctx, site.site)
        && geometry.surface_area().contains(position)
    {
        return village(settings, site.id());
    }
    let near = placement::site_index_near(position.x);
    for site in near - 2..=near + 2 {
        let Some(geometry) = geometry(&ctx, site) else {
            continue;
        };
        if geometry
            .hall_area()
            .is_some_and(|area| area.contains(position))
        {
            return village(settings, placement::underground_id(site));
        }
    }
    None
}

/// Whether `position` lies within `margin` tiles of a village's area (the surface village or
/// its underground hall), without building the village itself (enemy safe zones read this
/// every tick, so it only touches the cheap per-site geometry cache).
pub fn in_village_area(settings: &WorldSettings, position: TilePosition, margin: i32) -> bool {
    if !settings.generator.has_villages() {
        return false;
    }
    let ctx = ctx(settings);
    if let Some(site) = placement::surface_site_at(ctx.seed(), position.x, margin)
        && let Some(geometry) = geometry(&ctx, site.site)
        && geometry.surface_area().expanded(margin).contains(position)
    {
        return true;
    }
    let near = placement::site_index_near(position.x);
    (near - 2..=near + 2).any(|site| {
        geometry(&ctx, site)
            .and_then(|geometry| geometry.hall_area())
            .is_some_and(|area| area.expanded(margin).contains(position))
    })
}

/// The regional features (one per village) of the sites reaching `rect`, unfiltered.
pub fn layer_features(ctx: &Ctx, rect: &TileRect, out: &mut Vec<WorldFeature>) {
    if !ctx.settings.generator.has_villages() {
        return;
    }
    for site in sites_reaching(rect.min.x, rect.max.x) {
        let Some(geometry) = geometry(ctx, site) else {
            continue;
        };
        out.push(surface_feature(ctx, &geometry));
        out.extend(underground_feature(ctx, &geometry));
    }
}

fn surface_feature(ctx: &Ctx, geometry: &SiteGeometry) -> WorldFeature {
    let surface = geometry.surface;
    let center = TilePosition {
        x: surface.center,
        y: geometry.ground_row - 1,
    };
    WorldFeature {
        id: feature_id(Layer::Village, surface.site, 0, 0),
        kind: FeatureKind::Village,
        detail: FeatureDetail::Village(surface.id()),
        band: ctx.band(center.x, geometry.ground_row),
        center,
        bounds: TileRect {
            min: TilePosition {
                x: surface.first,
                y: geometry.ground_row - 6,
            },
            max: TilePosition {
                x: surface.last,
                y: geometry.ground_row + 1,
            },
        },
        size: u32::try_from(surface.last - surface.first + 1).unwrap_or(0),
    }
}

fn underground_feature(ctx: &Ctx, geometry: &SiteGeometry) -> Option<WorldFeature> {
    let (hall, center_column) = geometry.underground?;
    let bounds = hall_rect(center_column, hall.top);
    let center = TilePosition {
        x: center_column,
        y: hall.floor_row() - 1,
    };
    Some(WorldFeature {
        id: feature_id(Layer::Village, hall.site, 1, 0),
        kind: FeatureKind::Village,
        detail: FeatureDetail::Village(hall.id()),
        band: ctx.band(center.x, center.y),
        center,
        bounds,
        size: u32::try_from(HALL_WIDTH).unwrap_or(0),
    })
}

/// The feature of a regional village id (`ry` 0: surface, 1: underground).
pub fn feature(ctx: &Ctx, site: i32, ry: i32) -> Option<WorldFeature> {
    let geometry = geometry(ctx, site)?;
    match ry {
        0 => Some(surface_feature(ctx, &geometry)),
        1 => underground_feature(ctx, &geometry),
        _ => None,
    }
}

/// The stamps of every site reaching `rect` whose tiles touch it.
pub fn stamps_touching(ctx: &Ctx, rect: &TileRect) -> Vec<Arc<VillageStamp>> {
    if !ctx.settings.generator.has_villages() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for site in sites_reaching(rect.min.x, rect.max.x) {
        let Some(geometry) = geometry(ctx, site) else {
            continue;
        };
        let surface = geometry.surface;
        let surface_box = TileRect {
            min: TilePosition {
                x: surface.first - FLATTEN_FADE_COLUMNS,
                y: geometry.ground_row - 1,
            },
            max: TilePosition {
                x: surface.last + FLATTEN_FADE_COLUMNS,
                y: geometry.ground_row + CLEAR_ROWS,
            },
        };
        let touches_hall = geometry
            .hall_area()
            .is_some_and(|area| area.expanded(4).intersects(rect));
        if surface_box.intersects(rect) || touches_hall {
            found.push(
                STAMPS.get_or_compute((ctx.settings, site, 0), || stamp(&ctx.settings, &geometry)),
            );
        }
    }
    found
}

/// Foundations under every building lot (two rows plus a staggered third, as in town), paving
/// on the plaza and the landing pad, and the underground hall carved with its floor.
fn stamp(settings: &WorldSettings, geometry: &SiteGeometry) -> VillageStamp {
    let mut out = VillageStamp::default();
    let surface = geometry.surface;
    let ground = geometry.ground_row;
    out.clear.push(TileRect {
        min: TilePosition {
            x: surface.first - FLATTEN_FADE_COLUMNS,
            y: ground,
        },
        max: TilePosition {
            x: surface.last + FLATTEN_FADE_COLUMNS,
            y: ground + CLEAR_ROWS,
        },
    });
    if let Some(found) = village(settings, surface.id()) {
        lot_foundations(settings.seed, &found, &mut out.cells);
    }
    if let Some((hall, center)) = geometry.underground {
        let air = hall_rect(center, hall.top);
        out.clear.push(air.expanded(3));
        for y in air.min.y..hall.floor_row() {
            for x in air.min.x..=air.max.x {
                out.cells.push((x, y, TileKind::Air));
            }
        }
        for y in hall.floor_row()..hall.floor_row() + 2 {
            for x in air.min.x - 1..=air.max.x + 1 {
                out.cells.push((x, y, TileKind::Foundation));
            }
        }
        if let Some(found) = village(settings, hall.id()) {
            lot_foundations(settings.seed, &found, &mut out.cells);
        }
    }
    out.bounds = out.clear.iter().copied().reduce(|a, b| TileRect {
        min: TilePosition {
            x: a.min.x.min(b.min.x),
            y: a.min.y.min(b.min.y),
        },
        max: TilePosition {
            x: a.max.x.max(b.max.x),
            y: a.max.y.max(b.max.y),
        },
    });
    out
}

fn lot_foundations(seed: u64, village: &Village, cells: &mut Vec<(i32, i32, TileKind)>) {
    let ground = village.ground_row;
    for building in &village.buildings {
        for x in building.first_column..=building.last_column() {
            cells.push((x, ground, TileKind::Foundation));
            cells.push((x, ground + 1, TileKind::Foundation));
            if hash2(seed, SALT_FOUNDATION, x, ground) % 5 < 3 {
                cells.push((x, ground + 2, TileKind::Foundation));
            }
        }
    }
    let paved = std::iter::once(village.plaza).chain(village.landing_pad);
    for (first, last) in paved {
        for x in first..=last {
            cells.push((x, ground, TileKind::Foundation));
        }
    }
}

/// The story-worthy places near village `id` (centre column `center`), cached.
pub fn village_facts(settings: &WorldSettings, id: VillageId, center: i32) -> Arc<Vec<WorldFact>> {
    let key = i32::from_ne_bytes(id.0.to_ne_bytes());
    FACTS.get_or_compute((*settings, key, 1), || {
        let ctx = ctx(settings);
        let radius = crate::village::FACT_RADIUS_COLUMNS;
        let rect = TileRect {
            min: TilePosition {
                x: center - radius,
                y: 0,
            },
            max: TilePosition {
                x: center + radius,
                y: ctx.height - 1,
            },
        };
        super::index::features_in(&ctx, &rect, None)
            .iter()
            .filter_map(WorldFact::from_feature)
            .collect()
    })
}
