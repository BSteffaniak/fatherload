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
/// Sky towns (WP-SKY, V9), keyed by their archipelago block.
static SKY_TOWNS: MemoCache<Option<Arc<Village>>> = MemoCache::new(128);
static STAMPS: MemoCache<VillageStamp> = MemoCache::new(64);
static FACTS: MemoCache<Vec<WorldFact>> = MemoCache::new(64);

/// Gravel Hollow (Wave 8): a Deep Descent world's settlement beside the Descent I outpost in
/// the Grey Stone, the underground settlement of the town's own site 0 (no other world places
/// site 0). Its hall and centre column; `None` before V11.
#[must_use]
pub fn gravel_hollow_hall(settings: &WorldSettings) -> Option<(UndergroundSite, i32)> {
    if !settings.generator.has_villages() {
        return None;
    }
    let (center, top) = crate::crust::gravel_hollow(settings)?;
    Some((
        UndergroundSite {
            site: 0,
            center,
            top,
            slide: 1,
        },
        center,
    ))
}

/// Merrow City (Wave 9): a Deep Descent world's fish-folk settlement beside the Descent IV
/// outpost, under the city's bubble ([`crate::worldgen::descent::drowned::merrow_city`]). Its
/// hall and centre column; `None` before V11.
#[must_use]
pub fn merrow_hall(settings: &WorldSettings) -> Option<(UndergroundSite, i32)> {
    if !settings.generator.has_villages() {
        return None;
    }
    let city = crate::worldgen::descent::drowned::merrow_city(settings)?;
    Some((
        UndergroundSite {
            site: crate::drowned::MERROW_SITE,
            center: city.hall_center,
            top: city.hall_top,
            slide: 1,
        },
        city.hall_center,
    ))
}

/// Merrow City's village (Wave 9), built on first use, named for the place.
fn merrow_village(settings: &WorldSettings) -> Option<Arc<Village>> {
    let (site, center) = merrow_hall(settings)?;
    let built = VILLAGES.get_or_compute((*settings, crate::drowned::MERROW_SITE, 1), || {
        let mut village = layout::build(settings, &SiteSpec::Underground { site, center });
        crate::drowned::MERROW_CITY_NAME.clone_into(&mut village.name);
        crate::drowned::MERROW_CITY_NAME.clone_into(&mut village.population.village_name);
        Some(Arc::new(village))
    });
    built.as_ref().clone()
}

/// Gravel Hollow's village (Wave 8), built on first use, named for the place.
fn gravel_hollow(settings: &WorldSettings) -> Option<Arc<Village>> {
    let (site, center) = gravel_hollow_hall(settings)?;
    let built = VILLAGES.get_or_compute((*settings, 0, 1), || {
        let mut village = layout::build(settings, &SiteSpec::Underground { site, center });
        crate::crust::GRAVEL_HOLLOW_NAME.clone_into(&mut village.name);
        crate::crust::GRAVEL_HOLLOW_NAME.clone_into(&mut village.population.village_name);
        Some(Arc::new(village))
    });
    built.as_ref().clone()
}

/// The site index of the Sunfolk village (Wave 9): two past every site a world places (Merrow
/// City takes the first), so its id never meets another village's.
pub const SUNFOLK_SITE: i32 = placement::MAX_SITE + 2;

/// The Sunfolk village's hall and centre column (Wave 9); `None` before V11.
///
/// The jungle tribes' settlement in the Megafauna Jungle of a Deep Descent world, its hall on
/// the floor of the Sunfolk valley ([`crate::worldgen::descent::hollow::sunfolk_valley`]).
#[must_use]
pub fn sunfolk_hall(settings: &WorldSettings) -> Option<(UndergroundSite, i32)> {
    if !settings.generator.has_villages() || !settings.generator.has_descents() {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let valley = crate::worldgen::descent::hollow::sunfolk_valley(settings, &layout)?;
    let center = valley.center().x;
    Some((
        UndergroundSite {
            site: SUNFOLK_SITE,
            center,
            top: valley.max.y - HALL_AIR_ROWS,
            slide: 1,
        },
        center,
    ))
}

/// The Sunfolk village (Wave 9), built on first use, named for the place.
fn sunfolk(settings: &WorldSettings) -> Option<Arc<Village>> {
    let (site, center) = sunfolk_hall(settings)?;
    let built = VILLAGES.get_or_compute((*settings, SUNFOLK_SITE, 1), || {
        let mut village = layout::build(settings, &SiteSpec::Underground { site, center });
        crate::hollow::SUNFOLK_NAME.clone_into(&mut village.name);
        crate::hollow::SUNFOLK_NAME.clone_into(&mut village.population.village_name);
        Some(Arc::new(village))
    });
    built.as_ref().clone()
}

/// The fixed deep halls (Gravel Hollow, Merrow City, the Sunfolk village) as `(hall, centre)`.
fn deep_halls(settings: &WorldSettings) -> Vec<(UndergroundSite, i32)> {
    gravel_hollow_hall(settings)
        .into_iter()
        .chain(merrow_hall(settings))
        .chain(sunfolk_hall(settings))
        .collect()
}

/// The village of a fixed deep hall's site.
fn deep_hall_village(settings: &WorldSettings, site: i32) -> Option<Arc<Village>> {
    if site == SUNFOLK_SITE {
        sunfolk(settings)
    } else if site == crate::drowned::MERROW_SITE {
        merrow_village(settings)
    } else {
        gravel_hollow(settings)
    }
}

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

/// Tiles every cave keeps from a village's ground and from an underground hall on biome worlds
/// ([`keeps_caves_out`]).
const CAVE_KEEP_OUT: i32 = 6;

/// Whether caves must stay shut at `(x, y)` (biome worlds only: older generators keep their
/// output): under a surface village's ground (down past the cleared rows) and within
/// [`CAVE_KEEP_OUT`] tiles of an underground hall, so no cave undercuts a village and a hall's
/// floor never bridges an open cave.
///
/// Reads only the site geometry (the hall's slide reads points of interest, which never read
/// caves), so the cave layer may call it.
pub fn keeps_caves_out(ctx: &Ctx, x: i32, y: i32) -> bool {
    if !ctx.biomes || !ctx.settings.generator.has_villages() {
        return false;
    }
    if let Some(site) = placement::surface_site_at(ctx.seed(), x, FLATTEN_FADE_COLUMNS)
        && y <= super::base::village_ground_row(&ctx.settings, site.center)
            + CLEAR_ROWS
            + CAVE_KEEP_OUT
    {
        return true;
    }
    if deep_halls(&ctx.settings).iter().any(|(hall, center)| {
        hall_rect(*center, hall.top)
            .expanded(CAVE_KEEP_OUT)
            .contains(TilePosition { x, y })
    }) {
        return true;
    }
    // Merrow City's bubble and the sea round it (Wave 9): no cave pierces the glass.
    if ctx.descent.is_some()
        && crate::worldgen::descent::drowned::merrow_city(&ctx.settings).is_some_and(|city| {
            city.bounds()
                .expanded(CAVE_KEEP_OUT)
                .contains(TilePosition { x, y })
        })
    {
        return true;
    }
    // A hall slides at most three steps either way from its nominal centre.
    let slide_reach = 3 * (HALL_WIDTH + 24) + HALL_WIDTH + CAVE_KEEP_OUT;
    let near = placement::site_index_near(x);
    (near - 2..=near + 2).any(|site| {
        let Some(nominal) = placement::underground_site(&ctx.settings, site) else {
            return false;
        };
        if y < nominal.top - CAVE_KEEP_OUT
            || y > nominal.floor_row() + CAVE_KEEP_OUT
            || (x - nominal.center).abs() > slide_reach
        {
            return false;
        }
        geometry(ctx, site)
            .and_then(|geometry| geometry.hall_area())
            .is_some_and(|area| area.expanded(CAVE_KEEP_OUT).contains(TilePosition { x, y }))
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

/// The sky town of archipelago block `(bx, by)`, built on first use.
fn sky_town(settings: &WorldSettings, bx: i32, by: i32) -> Option<Arc<Village>> {
    let built = SKY_TOWNS.get_or_compute((*settings, bx, by), || {
        let site = crate::worldgen::sky::sky_town_site(settings, bx, by)?;
        Some(Arc::new(layout::build(settings, &SiteSpec::Sky { site })))
    });
    built.as_ref().clone()
}

/// The village `id`, built on first use.
pub fn village(settings: &WorldSettings, id: VillageId) -> Option<Arc<Village>> {
    if let Some((bx, by)) = placement::decode_sky_town(id) {
        return sky_town(settings, bx, by);
    }
    let ctx = ctx(settings);
    let (site, underground) = placement::decode_id(id);
    if site == 0 && underground {
        return gravel_hollow(settings);
    }
    if site == crate::drowned::MERROW_SITE && underground {
        return merrow_village(settings);
    }
    if site == SUNFOLK_SITE && underground {
        return sunfolk(settings);
    }
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
    for (hall, center) in deep_halls(settings) {
        if (first..=last).contains(&center) {
            found.extend(deep_hall_village(settings, hall.site));
        }
    }
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

/// Every sky town whose area overlaps `rect` (built on first use).
pub fn sky_towns_in(settings: &WorldSettings, rect: &TileRect) -> Vec<Arc<Village>> {
    crate::worldgen::sky::sky_towns_touching(settings, rect)
        .iter()
        .filter_map(|site| sky_town(settings, site.block.0, site.block.1))
        .collect()
}

/// The village whose area contains `position`.
pub fn village_containing(
    settings: &WorldSettings,
    position: TilePosition,
) -> Option<Arc<Village>> {
    if position.y < 0
        && let Some(site) = crate::worldgen::sky::sky_town_at(settings, position)
    {
        return sky_town(settings, site.block.0, site.block.1);
    }
    let ctx = ctx(settings);
    for (hall, center) in deep_halls(settings) {
        if hall_rect(center, hall.top).contains(position) {
            return deep_hall_village(settings, hall.site);
        }
    }
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
    if position.y < 0
        && settings.generator.has_sky_isles()
        && !crate::worldgen::sky::sky_towns_touching(
            settings,
            &TileRect {
                min: position,
                max: position,
            }
            .expanded(margin),
        )
        .is_empty()
    {
        return true;
    }
    let ctx = ctx(settings);
    if deep_halls(settings).iter().any(|(hall, center)| {
        hall_rect(*center, hall.top)
            .expanded(margin)
            .contains(position)
    }) {
        return true;
    }
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
    for hall in deep_halls(&ctx.settings) {
        if hall_rect(hall.1, hall.0.top).intersects(&rect.expanded(SITE_REACH)) {
            out.push(hall_feature(ctx, hall));
        }
    }
    for site in crate::worldgen::sky::sky_towns_touching(&ctx.settings, rect) {
        out.push(sky_town_feature(&site));
    }
}

/// The regional `ry` of sky town features: block row `by` is `SKY_TOWN_RY + by` (`rx` is the
/// block column).
pub const SKY_TOWN_RY: i32 = 2;

fn sky_town_feature(site: &crate::worldgen::sky::SkyTownSite) -> WorldFeature {
    let center = TilePosition {
        x: site.center(),
        y: site.ground_row - 1,
    };
    WorldFeature {
        id: feature_id(Layer::Village, site.block.0, SKY_TOWN_RY + site.block.1, 0),
        kind: FeatureKind::Village,
        detail: FeatureDetail::Village(site.id),
        band: crate::worldgen::Band::Topsoil,
        center,
        bounds: TileRect {
            min: TilePosition {
                x: site.first,
                y: site.ground_row - 6,
            },
            max: TilePosition {
                x: site.last,
                y: site.ground_row + 1,
            },
        },
        size: u32::try_from(site.last - site.first + 1).unwrap_or(0),
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
    Some(hall_feature(ctx, geometry.underground?))
}

fn hall_feature(ctx: &Ctx, (hall, center_column): (UndergroundSite, i32)) -> WorldFeature {
    let bounds = hall_rect(center_column, hall.top);
    let center = TilePosition {
        x: center_column,
        y: hall.floor_row() - 1,
    };
    WorldFeature {
        id: feature_id(Layer::Village, hall.site, 1, 0),
        kind: FeatureKind::Village,
        detail: FeatureDetail::Village(hall.id()),
        band: ctx.band(center.x, center.y),
        center,
        bounds,
        size: u32::try_from(HALL_WIDTH).unwrap_or(0),
    }
}

/// The feature of a regional village id (`ry` 0: surface, 1: underground).
pub fn feature(ctx: &Ctx, site: i32, ry: i32) -> Option<WorldFeature> {
    if ry >= SKY_TOWN_RY {
        return crate::worldgen::sky::sky_town_site(&ctx.settings, site, ry - SKY_TOWN_RY)
            .map(|town| sky_town_feature(&town));
    }
    if site == 0 && ry == 1 {
        return gravel_hollow_hall(&ctx.settings).map(|hall| hall_feature(ctx, hall));
    }
    if site == crate::drowned::MERROW_SITE && ry == 1 {
        return merrow_hall(&ctx.settings).map(|hall| hall_feature(ctx, hall));
    }
    if site == SUNFOLK_SITE && ry == 1 {
        return sunfolk_hall(&ctx.settings).map(|hall| hall_feature(ctx, hall));
    }
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
    if let Some((hall, center)) = gravel_hollow_hall(&ctx.settings)
        && hall_rect(center, hall.top).expanded(7).intersects(rect)
    {
        found.push(STAMPS.get_or_compute((ctx.settings, 0, 1), || {
            hall_stamp(&ctx.settings, hall, center)
        }));
    }
    if let Some((hall, center)) = merrow_hall(&ctx.settings)
        && hall_rect(center, hall.top).expanded(7).intersects(rect)
    {
        found.push(
            STAMPS.get_or_compute((ctx.settings, crate::drowned::MERROW_SITE, 1), || {
                merrow_stamp(&ctx.settings, hall, center)
            }),
        );
    }
    if let Some((hall, center)) = sunfolk_hall(&ctx.settings)
        && hall_rect(center, hall.top).expanded(7).intersects(rect)
    {
        found.push(STAMPS.get_or_compute((ctx.settings, SUNFOLK_SITE, 1), || {
            sunfolk_stamp(&ctx.settings, hall, center)
        }));
    }
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
    // Sky towns' foundations on their town isles (V9).
    if rect.min.y < 0 {
        for town in crate::worldgen::sky::sky_towns_touching(&ctx.settings, &rect.expanded(4)) {
            let (bx, by) = town.block;
            found.push(STAMPS.get_or_compute((ctx.settings, bx, 1_000 + by), || {
                let mut out = VillageStamp::default();
                if let Some(found) = sky_town(&ctx.settings, bx, by) {
                    lot_foundations(ctx.seed(), &found, &mut out.cells);
                }
                out
            }));
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

/// Gravel Hollow's stamp (Wave 8): its hall carved out, a packed gravel street over a
/// foundation floor, and its buildings' lots.
fn hall_stamp(settings: &WorldSettings, hall: UndergroundSite, center: i32) -> VillageStamp {
    let mut out = VillageStamp::default();
    let air = hall_rect(center, hall.top);
    out.clear.push(air.expanded(3));
    for y in air.min.y..hall.floor_row() {
        for x in air.min.x..=air.max.x {
            out.cells.push((x, y, TileKind::Air));
        }
    }
    // A gravel lane out of both ends: one of them reaches the outpost's door.
    for y in hall.floor_row() - 3..hall.floor_row() {
        for x in (air.min.x - 5..air.min.x).chain(air.max.x + 1..=air.max.x + 5) {
            out.cells.push((x, y, TileKind::Air));
        }
    }
    for x in air.min.x - 5..=air.max.x + 5 {
        out.cells.push((
            x,
            hall.floor_row(),
            TileKind::Crust(crate::crust::CrustTile::Gravel),
        ));
        out.cells
            .push((x, hall.floor_row() + 1, TileKind::Foundation));
    }
    if let Some(found) = gravel_hollow(settings) {
        lot_foundations(settings.seed, &found, &mut out.cells);
    }
    out.bounds = Some(air.expanded(6));
    out
}

/// Merrow City's hall stamp (Wave 9): its hall carved out over a reef-stone floor and its
/// buildings' lots (the city's bubble, glass and sea are the Drowned and Frozen's pass).
fn merrow_stamp(settings: &WorldSettings, hall: UndergroundSite, center: i32) -> VillageStamp {
    let mut out = VillageStamp::default();
    let air = hall_rect(center, hall.top);
    out.clear.push(air.expanded(3));
    for y in air.min.y..hall.floor_row() {
        for x in air.min.x..=air.max.x {
            out.cells.push((x, y, TileKind::Air));
        }
    }
    for x in air.min.x..=air.max.x {
        out.cells
            .push((x, hall.floor_row() + 1, TileKind::Foundation));
    }
    if let Some(found) = merrow_village(settings) {
        lot_foundations(settings.seed, &found, &mut out.cells);
    }
    out.bounds = Some(air.expanded(6));
    out
}
/// The Sunfolk village's stamp (Wave 9): its hall carved out of the valley's floor, a fern
/// floor over foundations, and its buildings' lots.
fn sunfolk_stamp(settings: &WorldSettings, hall: UndergroundSite, center: i32) -> VillageStamp {
    use crate::hollow::HollowTile;
    let mut out = VillageStamp::default();
    let air = hall_rect(center, hall.top);
    out.clear.push(air.expanded(3));
    for y in air.min.y..hall.floor_row() {
        for x in air.min.x..=air.max.x {
            out.cells.push((x, y, TileKind::Air));
        }
    }
    for x in air.min.x - 2..=air.max.x + 2 {
        out.cells
            .push((x, hall.floor_row(), TileKind::Hollow(HollowTile::Fern)));
        out.cells
            .push((x, hall.floor_row() + 1, TileKind::Foundation));
    }
    if let Some(found) = sunfolk(settings) {
        for building in &found.buildings {
            for x in building.first_column..=building.last_column() {
                out.cells.push((x, hall.floor_row(), TileKind::Foundation));
            }
        }
    }
    out.bounds = Some(air.expanded(6));
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
