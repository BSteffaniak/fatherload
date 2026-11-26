//! Space's destinations (W14-B), read from the space generator.
//!
//! The generator's sectors are read in square rings round the player's sector (the shared,
//! memoized [`crate::space::Space`]), never live state. Bodies' sites (alien towns, theme-boss lairs, builder outposts) are read from their
//! planet grounds' pure site functions.
//!
//! A landable body's candidate is a place on it ([`Place::OnBody`]): `x` is its strip column,
//! or [`ARRIVAL_POINT`] for the body's own arrival point; `y` is the floor row it stands on,
//! or [`SURFACE`] for the surface at that column. Everything else is a space tile in free
//! flight, already a safe distance off the thing it names (the world stops the rig there).

use crate::{
    place::{BodyId, Place},
    planet::PlanetGround,
    space::{BodyKind, Space, SpaceBody, SpaceBodyId, SpaceSector, SpaceTile, StarSystem, Theme},
};

use super::{Destination, DestinationFilter, DestinationId, DestinationKind};

/// `x` of a body candidate that lands at the body's own arrival point.
pub const ARRIVAL_POINT: i64 = -1;
/// `y` of a body candidate that stands on the surface at its column.
pub const SURFACE: i64 = i64::MIN / 4;
/// Sector rings searched round the player for most kinds.
pub const RINGS: i32 = 3;
/// Sector rings searched for sites on bodies (towns, outposts), which read the grounds.
pub const SITE_RINGS: i32 = 1;
/// Tiles kept between a stopped rig and a body's surface (or a station's hull).
pub const SAFE_TILES: i64 = 3_000;
/// Tiles kept past a black hole's lensing radius.
pub const BLACK_HOLE_TILES: i64 = 6_000;

/// What a space query knows.
#[derive(Clone, Copy, Debug)]
pub struct SpaceQuery<'a> {
    pub space: &'a Space,
    pub seed: u64,
    /// Where the search is centred (the player's space position).
    pub from: SpaceTile,
    /// The systems the Archive's charts lead to.
    pub charts: &'a [crate::space::SystemRef],
}

impl SpaceQuery<'_> {
    fn sectors(&self, rings: i32) -> Vec<SpaceSector> {
        let centre = SpaceSector::of_tile(self.from);
        let mut out = Vec::new();
        for ring in 0..=rings {
            for y in -ring..=ring {
                for x in -ring..=ring {
                    if x.abs().max(y.abs()) == ring {
                        out.push(SpaceSector::new(centre.x + x, centre.y + y));
                    }
                }
            }
        }
        out
    }
}

/// The space tile `tiles` above `centre` (a stopped rig's spot off a body).
const fn above(centre: SpaceTile, tiles: i64) -> SpaceTile {
    centre.offset(0, -tiles)
}

fn free(kind: DestinationKind, tile: SpaceTile, name: impl Into<String>) -> Destination {
    Destination::new(DestinationId::new(kind, Place::Space, tile.x, tile.y), name)
}

fn on_body(
    kind: DestinationKind,
    body: SpaceBodyId,
    x: i64,
    y: i64,
    name: impl Into<String>,
) -> Destination {
    Destination::new(
        DestinationId::new(kind, Place::OnBody(BodyId::Space(body)), x, y),
        name,
    )
}

/// The body card's figures.
fn describe_body(destination: &mut Destination, system: &StarSystem, body: &SpaceBody) {
    let info = &mut destination.info;
    info.system = Some(system.name.clone());
    info.theme = Some(body.theme);
    info.gravity_milli_g = Some(u32::from(body.gravity_milli_g));
    info.place = system.body_name(body.index);
    info.hazards = vec![
        format!("{} air", body.atmosphere.name()),
        format!("{} °C", body.temperature_c),
    ];
}

fn theme_ok(filter: DestinationFilter, theme: Theme) -> bool {
    match filter {
        DestinationFilter::Theme(only) => only == theme,
        _ => true,
    }
}

const fn body_id(sector: SpaceSector, system: &StarSystem, body: &SpaceBody) -> SpaceBodyId {
    SpaceBodyId {
        sector,
        system: system.index,
        body: body.index,
    }
}

/// A body's destination: on it when it is landable, else stopped off it in space.
fn body_destination(
    query: &SpaceQuery,
    kind: DestinationKind,
    sector: SpaceSector,
    system: &StarSystem,
    body: &SpaceBody,
) -> Destination {
    let id = body_id(sector, system, body);
    let name = system.body_name(body.index);
    let mut destination = if crate::place::planet_params_for(query.space.space_seed(), id).is_some() {
        on_body(kind, id, ARRIVAL_POINT, SURFACE, name)
    } else {
        free(
            kind,
            above(body.centre, i64::from(body.radius) + SAFE_TILES),
            name,
        )
    };
    describe_body(&mut destination, system, body);
    if let Some(species) =
        crate::alien::AlienSpecies::from_folk(crate::alien::folk_of_body(query.seed, id, body))
    {
        destination.info.residents = vec![species.def().name.to_owned()];
    }
    destination
}

/// Every candidate of space `kind` round the query's centre (unordered).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per kind")]
pub fn candidates(
    query: &SpaceQuery,
    kind: DestinationKind,
    filter: DestinationFilter,
) -> Vec<Destination> {
    let mut found = Vec::new();
    match kind {
        DestinationKind::ArchiveSystem => {
            for reference in query.charts {
                if let Some(system) = query.space.system(*reference) {
                    let tile = above(system.centre, star_reach(&system));
                    let mut destination = free(kind, tile, format!("{} (charted)", system.name));
                    destination.info.system = Some(system.name.clone());
                    destination.info.place = format!("{} system", system.name);
                    found.push(destination);
                }
            }
            return found;
        }
        DestinationKind::ThemeBossLair => {
            for boss in crate::endgame::hunt::THEME_BOSSES {
                if !theme_ok(filter, boss.world) {
                    continue;
                }
                let Some(id) = crate::endgame::hunt::nearest_theme_world(query.space.space_seed(), boss.world)
                else {
                    continue;
                };
                let Some(params) = crate::place::planet_params_for(query.space.space_seed(), id) else {
                    continue;
                };
                let ground = PlanetGround::new(params);
                let lair = crate::planet::outlands::boss_lair(&ground)
                    .map(|lair| (lair.centre, lair.middle().1))
                    .or_else(|| {
                        crate::exotic::boss::arena(&ground)
                            .map(|arena| (i64::from(arena.centre), arena.floor - 2))
                    });
                let Some((column, row)) = lair else {
                    continue;
                };
                let mut destination = on_body(kind, id, column, SURFACE, boss.name);
                destination.inside = Some((column, i64::from(row)));
                destination.info.boss = Some(boss.name.to_owned());
                if let (Some(system), Some(body)) =
                    (query.space.system(id.system_ref()), query.space.body(id))
                {
                    describe_body(&mut destination, &system, &body);
                }
                found.push(destination);
            }
            return found;
        }
        _ => {}
    }
    let site_kind = matches!(
        kind,
        DestinationKind::AlienTown | DestinationKind::BuilderOutpost
    );
    let rings = if site_kind { SITE_RINGS } else { RINGS };
    for sector_coords in query.sectors(rings) {
        let sector = query.space.sector(sector_coords);
        match kind {
            DestinationKind::Nebula => {
                for nebula in &sector.nebulae {
                    // Interstellar nebulae are listed by every sector they overlap: once each.
                    if SpaceSector::of_tile(nebula.centre) == sector_coords {
                        found.push(free(
                            kind,
                            nebula.centre,
                            format!("Nebula {:04X}", nebula.id & 0xFFFF),
                        ));
                    }
                }
            }
            DestinationKind::BlackHole => {
                for hole in &sector.black_holes {
                    let reach = i64::from(hole.lensing_radius) + BLACK_HOLE_TILES;
                    let mut destination = free(
                        kind,
                        above(hole.centre, reach),
                        format!("Black hole {:04X}", hole.id & 0xFFFF),
                    );
                    destination.info.hazards =
                        vec!["event horizon".to_owned(), "time dilation".to_owned()];
                    found.push(destination);
                }
            }
            DestinationKind::Wormhole => {
                for mouth in &sector.wormholes {
                    let mut destination = free(
                        kind,
                        above(mouth.centre, i64::from(mouth.radius) + SAFE_TILES),
                        format!(
                            "Wormhole to sector {}, {}",
                            mouth.exit_sector.x, mouth.exit_sector.y
                        ),
                    );
                    "free space".clone_into(&mut destination.info.place);
                    found.push(destination);
                }
            }
            _ => {}
        }
        for system in &sector.systems {
            match kind {
                DestinationKind::System => {
                    let mut destination = free(
                        kind,
                        above(system.centre, star_reach(system)),
                        system.name.clone(),
                    );
                    destination.info.system = Some(system.name.clone());
                    destination.info.place =
                        format!("{} system ({} planets)", system.name, system.planet_count());
                    destination.info.hazards = vec!["star heat".to_owned()];
                    found.push(destination);
                }
                DestinationKind::Planet | DestinationKind::SmallBody => {
                    for body in &system.bodies {
                        let planet = matches!(body.kind, BodyKind::Planet);
                        let small = matches!(body.kind, BodyKind::Moon | BodyKind::CraterRock);
                        if body.is_home()
                            || (kind == DestinationKind::Planet && !planet)
                            || (kind == DestinationKind::SmallBody && !small)
                            || !theme_ok(filter, body.theme)
                        {
                            continue;
                        }
                        found.push(body_destination(query, kind, sector_coords, system, body));
                    }
                }
                DestinationKind::Station => {
                    for station in &system.stations {
                        let mut destination = free(
                            kind,
                            above(station.centre, i64::from(station.size) + SAFE_TILES / 3),
                            format!("{} {}", system.name, station.kind.name()),
                        );
                        destination.info.system = Some(system.name.clone());
                        destination.info.place = format!("{} system", system.name);
                        found.push(destination);
                    }
                }
                DestinationKind::Belt => {
                    for belt in &system.belts {
                        // Just inside the belt's inner edge, above the star.
                        let tile = above(system.centre, i64::from(belt.inner) - SAFE_TILES / 2);
                        let mut destination = free(
                            kind,
                            tile,
                            format!("{} {} belt", system.name, belt.ore.name()),
                        );
                        destination.info.system = Some(system.name.clone());
                        found.push(destination);
                    }
                }
                DestinationKind::Comet => {
                    for (index, comet) in system.comets.iter().enumerate() {
                        let tile = above(
                            comet.centre,
                            i64::from(comet.nucleus_radius) + SAFE_TILES / 3,
                        );
                        let mut destination =
                            free(kind, tile, format!("{} comet {}", system.name, index + 1));
                        destination.info.system = Some(system.name.clone());
                        found.push(destination);
                    }
                }
                DestinationKind::Nebula => {
                    if let Some(nebula) = &system.nebula {
                        let mut destination = free(
                            kind,
                            nebula.centre.offset(0, -star_reach(system)),
                            format!("{} nebula", system.name),
                        );
                        destination.info.system = Some(system.name.clone());
                        found.push(destination);
                    }
                }
                DestinationKind::AlienTown | DestinationKind::BuilderOutpost => {
                    for body in &system.bodies {
                        if body.is_home() || !theme_ok(filter, body.theme) {
                            continue;
                        }
                        let id = body_id(sector_coords, system, body);
                        found.extend(body_sites(query, kind, id, system, body));
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// How far off a system's star(s) a rig stops: past every star's heat.
fn star_reach(system: &StarSystem) -> i64 {
    system
        .stars
        .iter()
        .map(|star| i64::from(star.heat_radius.max(star.radius)) + SAFE_TILES)
        .max()
        .unwrap_or(SAFE_TILES)
}

/// The alien towns or builder outposts on body `id`.
fn body_sites(
    query: &SpaceQuery,
    kind: DestinationKind,
    id: SpaceBodyId,
    system: &StarSystem,
    body: &SpaceBody,
) -> Vec<Destination> {
    let Some(params) = crate::place::planet_params_for(query.space.space_seed(), id) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if kind == DestinationKind::AlienTown {
        if params.folk == 0 {
            return found;
        }
        for town in crate::alien::town::towns_of(&params).iter() {
            let mut destination = on_body(
                kind,
                id,
                i64::from(town.site.centre),
                SURFACE,
                town.name.clone(),
            );
            describe_body(&mut destination, system, body);
            destination.info.residents = vec![town.species.def().name.to_owned()];
            found.push(destination);
        }
        return found;
    }
    let outland = matches!(
        body.theme,
        Theme::Fungal | Theme::MachineWorld | Theme::DeadWorld
    );
    let exotic = matches!(
        body.theme,
        Theme::GasGiant | Theme::TidallyLocked | Theme::RingWorld
    );
    if !outland && !exotic {
        return found;
    }
    let ground = PlanetGround::new(params);
    let name = format!("Builders' outpost on {}", system.body_name(body.index));
    if outland {
        for outpost in crate::planet::outlands::builder_outposts(&ground) {
            let mut destination = on_body(
                kind,
                id,
                i64::from(outpost.site.centre()),
                SURFACE,
                name.clone(),
            );
            describe_body(&mut destination, system, body);
            found.push(destination);
        }
    }
    if exotic && let Some(outpost) = crate::exotic::builder_outpost(&ground) {
        let mut destination = on_body(
            kind,
            id,
            i64::from(outpost.column),
            i64::from(outpost.floor),
            name,
        );
        describe_body(&mut destination, system, body);
        found.push(destination);
    }
    found
}
