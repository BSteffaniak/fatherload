//! The home world's destinations (W14-B): pure functions of the world settings.
//!
//! Every candidate is read from the generator that places it (the Descent layout, the
//! landmark stamps, the regional feature index, tomb sites, camps, the expedition's sites, the
//! gate arenas), inside a window of [`WINDOW_COLUMNS`] either side of an anchor column, so a
//! query never streams the world in. A candidate's tile is where the rig should stand (the
//! world settles it onto the floor and carves its arrival pocket, `world/destinations.rs`).

use crate::{
    descent::{Descent, DescentBoss, Stratum},
    place::{BodyId, Place},
    poi::PoiKind,
    terrain::TilePosition,
    worldgen::{
        FeatureKind, TileRect, WorldSettings,
        descent::{self as layout_gen, DescentLayout, landmarks},
        streamed,
    },
};

use super::{Destination, DestinationFilter, DestinationId, DestinationKind};

/// Columns either side of the anchor a windowed kind searches.
pub const WINDOW_COLUMNS: i32 = 1_536;
/// Depth steps of an unfiltered [`DestinationKind::Depth`] list, metres.
pub const DEPTH_STEP_METRES: i32 = 500;
/// Rows above a boss arena's roof its door pocket is carved at.
pub const ARENA_DOOR_ROWS: i32 = 8;

/// What a home query knows: the settings, where to search round and the world's columns.
#[derive(Clone, Copy, Debug)]
pub struct HomeQuery<'a> {
    pub settings: &'a WorldSettings,
    /// The column windowed kinds search round (and depth kinds stand in).
    pub anchor: TilePosition,
    /// The home town's spawn tile.
    pub spawn: TilePosition,
    /// The world's first and last columns.
    pub columns: (i32, i32),
}

impl HomeQuery<'_> {
    fn layout(&self) -> Option<DescentLayout> {
        self.settings
            .generator
            .has_descents()
            .then(|| crate::descent::fields::cached_layout(self.settings))
    }

    fn window(&self) -> TileRect {
        let first = (self.anchor.x - WINDOW_COLUMNS).max(self.columns.0);
        let last = (self.anchor.x + WINDOW_COLUMNS).min(self.columns.1);
        let top = -crate::worldgen::sky::SKY_CEILING_ROWS.max(0) - 64;
        TileRect {
            min: TilePosition { x: first, y: top },
            max: TilePosition {
                x: last.max(first),
                y: self.settings.height_tiles(),
            },
        }
    }

    fn surface(&self, column: i32) -> i32 {
        crate::worldgen::surface_row(self.settings, column)
    }

    fn column_inside(&self, column: i32) -> i32 {
        column.clamp(self.columns.0 + 4, self.columns.1 - 4)
    }
}

/// A home-world candidate standing at `(x, y)`.
fn at(kind: DestinationKind, x: i32, y: i32, name: impl Into<String>) -> Destination {
    Destination::new(
        DestinationId::new(kind, Place::HOME, i64::from(x), i64::from(y)),
        name,
    )
}

/// The middle of a rect's floor (its last row is the floor; the rig stands on the row above).
const fn floor_middle(rect: &TileRect) -> (i32, i32) {
    (i32::midpoint(rect.min.x, rect.max.x), rect.max.y - 1)
}

/// Fills the card's depth, Descent, stratum, hazards and creatures from where it stands.
pub fn describe(query: &HomeQuery, destination: &mut Destination) {
    let info = &mut destination.info;
    info.gravity_milli_g = Some(1_000);
    if destination.id.place == Place::OnBody(BodyId::Moon) {
        info.place = "The Moon".to_owned();
        info.gravity_milli_g = Some(170);
        return;
    }
    info.place = "Home world".to_owned();
    let (Ok(x), Ok(y)) = (
        i32::try_from(destination.id.x),
        i32::try_from(destination.id.y),
    ) else {
        return;
    };
    let surface = query.surface(x);
    if y <= surface {
        return;
    }
    info.depth_m = Some(y - surface);
    let Some(layout) = query.layout() else {
        return;
    };
    let stratum = layout.stratum_of_row(y);
    info.stratum = Some(stratum);
    info.descent = Some(stratum.descent());
    let def = stratum.def();
    info.hazards = def
        .hazards
        .iter()
        .map(|(hazard, _)| format!("{hazard:?}").to_ascii_lowercase())
        .collect();
    if let Some(fluid) = def.fluids.first() {
        info.hazards.push(format!("{:?}", fluid.kind).to_ascii_lowercase());
    }
    if info.residents.is_empty() {
        info.residents = def
            .creatures
            .iter()
            .take(4)
            .map(|(creature, _)| creature.stats().name.to_owned())
            .collect();
    }
}

/// Whether `destination` passes `filter` (Descent and stratum filters read its card).
fn passes(filter: DestinationFilter, destination: &Destination) -> bool {
    match filter {
        DestinationFilter::Descent(descent) => destination.info.descent == Some(descent),
        DestinationFilter::Stratum(stratum) => destination.info.stratum == Some(stratum),
        DestinationFilter::Any
        | DestinationFilter::Metres(_)
        | DestinationFilter::Row(_)
        | DestinationFilter::Theme(_)
        | DestinationFilter::Player(_) => true,
    }
}

/// Every candidate of home-world `kind` (unordered; the world orders them by distance).
#[must_use]
pub fn candidates(
    query: &HomeQuery,
    kind: DestinationKind,
    filter: DestinationFilter,
) -> Vec<Destination> {
    let mut found = gather(query, kind, filter);
    for destination in &mut found {
        describe(query, destination);
    }
    found.retain(|destination| {
        destination.id.x >= i64::from(query.columns.0)
            && destination.id.x <= i64::from(query.columns.1)
    });
    // Depth and Descent kinds name their own Descent; the rest are narrowed by where they lie.
    if !matches!(
        kind,
        DestinationKind::Descent | DestinationKind::Stratum | DestinationKind::Depth
    ) {
        found.retain(|destination| passes(filter, destination));
    }
    found
}

#[allow(clippy::too_many_lines, reason = "one arm per kind")]
fn gather(
    query: &HomeQuery,
    kind: DestinationKind,
    filter: DestinationFilter,
) -> Vec<Destination> {
    let settings = query.settings;
    let layout = query.layout();
    let column = query.column_inside(query.anchor.x);
    match kind {
        DestinationKind::Descent => {
            let Some(layout) = layout else {
                return Vec::new();
            };
            Descent::ALL
                .iter()
                .filter(|descent| match filter {
                    DestinationFilter::Descent(only) => only == **descent,
                    DestinationFilter::Stratum(stratum) => stratum.descent() == **descent,
                    _ => true,
                })
                .map(|descent| {
                    let row = stratum_entry_row(query, &layout, descent.first_stratum(), column);
                    let mut destination = at(
                        kind,
                        column,
                        row,
                        format!("Descent {}: {}", descent.numeral(), descent.name()),
                    );
                    destination.info.descent = Some(*descent);
                    if let Some(gate) = descent.gate() {
                        destination.info.boss = Some(gate.boss.name().to_owned());
                    }
                    destination
                })
                .collect()
        }
        DestinationKind::Stratum => {
            let Some(layout) = layout else {
                return Vec::new();
            };
            Stratum::ALL
                .iter()
                .filter(|stratum| match filter {
                    DestinationFilter::Descent(only) => stratum.descent() == only,
                    DestinationFilter::Stratum(only) => only == **stratum,
                    _ => true,
                })
                .map(|stratum| {
                    let row = stratum_entry_row(query, &layout, *stratum, column);
                    at(
                        kind,
                        column,
                        row,
                        format!(
                            "{} (Descent {})",
                            stratum.name(),
                            stratum.descent().numeral()
                        ),
                    )
                })
                .collect()
        }
        DestinationKind::Depth => depths(query, filter, column),
        DestinationKind::HomeTown => vec![at(
            kind,
            query.spawn.x,
            query.spawn.y,
            "Home town",
        )],
        DestinationKind::Village => villages(query, false),
        DestinationKind::Settlement => {
            let mut found = villages(query, true);
            found.extend(deep_settlements(query));
            found
        }
        DestinationKind::SkyIsland => features(
            query,
            &[FeatureKind::SkyIsland, FeatureKind::SkyContinent],
        )
        .into_iter()
        .map(|feature| {
            at(
                kind,
                feature.center.x,
                feature.center.y - 1,
                format!("{} at column {}", feature.kind_name(), feature.center.x),
            )
        })
        .collect(),
        DestinationKind::Cave => features(query, &[FeatureKind::Cave, FeatureKind::LargeCavern])
            .into_iter()
            .map(|feature| {
                let name = feature.detail.cave().map_or_else(
                    || "Large cavern".to_owned(),
                    |cave| format!("{cave:?} cave"),
                );
                at(kind, feature.center.x, feature.center.y, name)
            })
            .collect(),
        DestinationKind::Vault | DestinationKind::Landmark => {
            let vaults = kind == DestinationKind::Vault;
            let mut found: Vec<Destination> = features(query, &[FeatureKind::PointOfInterest])
                .into_iter()
                .filter(|feature| (feature.poi_kind() == Some(PoiKind::Vault)) == vaults)
                .map(|feature| {
                    let name = feature
                        .poi_kind()
                        .map_or_else(|| "Point of interest".to_owned(), |poi| format!("{poi:?}"));
                    let (x, y) = floor_middle(&feature.bounds);
                    at(kind, x, y, name)
                })
                .collect();
            if !vaults && let Some(layout) = layout {
                found.extend(set_pieces(settings, &layout));
            }
            found
        }
        DestinationKind::AnomalyRoom => anomaly_rooms(query, layout.as_ref()),
        DestinationKind::Outpost => {
            let Some(layout) = layout else {
                return Vec::new();
            };
            Descent::ALL
                .iter()
                .filter_map(|descent| landmarks::outpost(settings, &layout, *descent))
                .map(|hall| {
                    let (x, y) = floor_middle(&hall.bounds);
                    let name = crate::logistics::sites::OUTPOST_NAMES
                        .get(hall.descent.index())
                        .copied()
                        .unwrap_or("Deep outpost");
                    at(
                        kind,
                        x,
                        y,
                        format!("{name} (Descent {})", hall.descent.numeral()),
                    )
                })
                .collect()
        }
        DestinationKind::StoryLog | DestinationKind::Camp => {
            let camps = kind == DestinationKind::Camp;
            let mut found: Vec<Destination> = crate::expedition::sites::sites(settings)
                .into_iter()
                .filter(|site| (site.kind == crate::expedition::SiteKind::Camp) == camps)
                .map(|site| {
                    let (x, y) = floor_middle(&site.bounds);
                    at(
                        kind,
                        x,
                        y,
                        format!("Log {}: the Fathers' {}", site.log + 1, site.kind.name()),
                    )
                })
                .collect();
            if camps {
                let window = query.window();
                found.extend(
                    crate::people::camps::camps_between(settings, window.min.x, window.max.x)
                        .into_iter()
                        .map(|camp| {
                            at(
                                kind,
                                camp.column,
                                camp.ground_row - 1,
                                format!("{:?} camp", camp.kind),
                            )
                        }),
                );
            }
            found
        }
        DestinationKind::Tomb => {
            if !settings.generator.has_tombs() || !settings.generator.is_streamed() {
                return Vec::new();
            }
            streamed::tomb_sites::sites_touching(settings, &query.window())
                .into_iter()
                .map(|site| {
                    at(
                        kind,
                        site.door.x,
                        site.door.y,
                        format!("{:?} tomb (tier {})", site.style, site.tier),
                    )
                })
                .collect()
        }
        DestinationKind::BossArena => layout.map_or_else(Vec::new, |layout| {
            boss_arenas(settings, &layout)
        }),
        DestinationKind::GateSetPiece => {
            let Some(layout) = layout else {
                return Vec::new();
            };
            Descent::ALL
                .iter()
                .map(|descent| {
                    let chamber = landmarks::gate_chamber(settings, &layout, *descent);
                    let (x, y) = floor_middle(&chamber.bounds);
                    let mut destination = at(
                        kind,
                        x,
                        y,
                        format!("Gate of Descent {}", descent.numeral()),
                    );
                    if let Some(gate) = descent.gate() {
                        destination.info.boss = Some(gate.boss.name().to_owned());
                    }
                    destination
                })
                .collect()
        }
        DestinationKind::Heart => crate::heart::heart_centre(settings)
            .map(|centre| {
                let mut destination = at(
                    kind,
                    centre.x,
                    centre.y - crate::heart::CHAMBER_RADIUS - ARENA_DOOR_ROWS,
                    "The Heart",
                );
                destination.inside = Some((i64::from(centre.x), i64::from(centre.y)));
                destination.info.boss = Some(DescentBoss::TheHeart.name().to_owned());
                destination
            })
            .into_iter()
            .collect(),
        DestinationKind::Moon => crate::worldgen::sky::moon(settings)
            .map(|moon| {
                Destination::new(
                    DestinationId::new(
                        kind,
                        Place::OnBody(BodyId::Moon),
                        i64::from(moon.center_x),
                        i64::from(moon.top_row - 1),
                    ),
                    "The Moon",
                )
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// The row a rig enters `stratum` at above `column`: a few rows into its solid top (the
/// first Descent's is the surface).
fn stratum_entry_row(
    query: &HomeQuery,
    layout: &DescentLayout,
    stratum: Stratum,
    column: i32,
) -> i32 {
    if stratum.index() == 0 {
        return query.surface(column) - 1;
    }
    let top = (column - 2..=column + 2)
        .map(|x| layout_gen::stratum_solid_top_row(query.settings, layout, stratum, x))
        .max()
        .unwrap_or(0);
    let (_, bottom) = layout.rows(stratum);
    (top + 6).min(bottom - 4)
}

/// [`DestinationKind::Depth`]: one exact depth, or a ladder every [`DEPTH_STEP_METRES`].
fn depths(query: &HomeQuery, filter: DestinationFilter, column: i32) -> Vec<Destination> {
    let surface = query.surface(column);
    let floor = if query.settings.generator.has_descents() {
        layout_gen::floor_row(query.settings) - 12
    } else {
        query.settings.height_tiles() - 12
    };
    let make = |row: i32| {
        let row = row.clamp(surface - 1, floor.max(surface));
        at(
            DestinationKind::Depth,
            column,
            row,
            format!("{} m", (row - surface).max(0)),
        )
    };
    match filter {
        DestinationFilter::Metres(metres) => vec![make(surface + metres)],
        DestinationFilter::Row(row) => vec![make(row)],
        _ => (1..)
            .map(|step| surface + step * DEPTH_STEP_METRES)
            .take_while(|row| *row < floor)
            .map(make)
            .collect(),
    }
}

/// The features of `kinds` inside the query's window (streamed worlds only).
fn features(query: &HomeQuery, kinds: &[FeatureKind]) -> Vec<crate::worldgen::features::WorldFeature> {
    if !query.settings.generator.is_streamed() {
        return Vec::new();
    }
    streamed::features_in(query.settings, &query.window(), Some(kinds))
}

trait KindName {
    fn kind_name(&self) -> &'static str;
}

impl KindName for crate::worldgen::features::WorldFeature {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            FeatureKind::SkyContinent => "Sky continent",
            _ => "Sky island",
        }
    }
}

/// Surface villages and sky towns (`underground` false) or underground settlements.
fn villages(query: &HomeQuery, underground: bool) -> Vec<Destination> {
    let kind = if underground {
        DestinationKind::Settlement
    } else {
        DestinationKind::Village
    };
    features(query, &[FeatureKind::Village])
        .into_iter()
        .filter_map(|feature| {
            let id = feature.village()?;
            if crate::people::camps::is_camp(id) {
                return None;
            }
            let sky = crate::village::placement::decode_sky_town(id).is_some();
            let below = !sky && crate::village::placement::decode_id(id).1;
            if below != underground {
                return None;
            }
            let village = streamed::village(query.settings, id);
            let name = village
                .as_ref()
                .map_or_else(|| "Village".to_owned(), |village| village.name.clone());
            let row = if sky || underground {
                feature.center.y
            } else {
                query.surface(feature.center.x) - 1
            };
            let mut destination = at(kind, feature.center.x, row, name.clone());
            destination.info.residents = vec![format!("the folk of {name}")];
            Some(destination)
        })
        .collect()
}

/// The deep settlements' halls (Cathedral Station, the Amber Court, the Archive Post, Heart's
/// Edge).
fn deep_settlements(query: &HomeQuery) -> Vec<Destination> {
    crate::people::deep_folk::DeepSettlement::ALL
        .iter()
        .filter_map(|place| {
            let (ground, first, last) = crate::people::deep_folk::hall(query.settings, *place)?;
            Some(at(
                DestinationKind::Settlement,
                i32::midpoint(first, last),
                ground - 1,
                place.name(),
            ))
        })
        .collect()
}

/// The anomaly rooms in the window (the expedition's and the Strange Layers' crystal rooms).
fn anomaly_rooms(query: &HomeQuery, layout: Option<&DescentLayout>) -> Vec<Destination> {
    let Some(layout) = layout else {
        return Vec::new();
    };
    let window = query.window();
    let mut found = Vec::new();
    let (width, height) = crate::expedition::anomaly::ANOMALY_REGION;
    let rows = (layout.tops[0].max(0), layout.floor);
    for ry in rows.0.div_euclid(height)..=rows.1.div_euclid(height) {
        for rx in window.min.x.div_euclid(width)..=window.max.x.div_euclid(width) {
            if let Some(room) = crate::expedition::anomaly::room_in_region(query.settings, layout, rx, ry)
            {
                let (x, y) = floor_middle(&room.bounds);
                found.push(at(
                    DestinationKind::AnomalyRoom,
                    x,
                    y,
                    format!("{:?} anomaly", room.kind),
                ));
            }
        }
    }
    found.extend(
        landmarks::landmarks_with(query.settings, layout, &window)
            .into_iter()
            .filter(|landmark| landmark.kind == crate::descent::LandmarkKind::AnomalyRoom)
            .map(|landmark| {
                let (x, y) = floor_middle(&landmark.bounds);
                at(DestinationKind::AnomalyRoom, x, y, "Crystal anomaly room")
            }),
    );
    found
}

/// The Descents' named set pieces (landmarks one of a kind per world).
fn set_pieces(settings: &WorldSettings, layout: &DescentLayout) -> Vec<Destination> {
    let mut found = Vec::new();
    let mut push = |rect: Option<TileRect>, name: &str| {
        if let Some(rect) = rect {
            let (x, y) = floor_middle(&rect);
            found.push(at(DestinationKind::Landmark, x, y, name));
        }
    };
    push(
        crate::old_deep::sites::cathedral(settings, layout).map(|site| site.bounds),
        "The Pressure Cathedral",
    );
    push(
        layout_gen::drowned::old_faithful(settings, layout),
        "Old Faithful",
    );
    push(
        layout_gen::ancient_city::lantern_library(settings, layout).map(|stamp| stamp.bounds),
        "The Lantern Library",
    );
    push(
        layout_gen::ancient_city::great_engine(settings, layout).map(|stamp| stamp.bounds),
        "The Great Engine",
    );
    push(
        layout_gen::ancient_city::orrery(settings, layout).map(|stamp| stamp.bounds),
        "The Orrery",
    );
    push(
        layout_gen::far_strange::clock_hall(settings, layout),
        "The Clock Hall",
    );
    push(layout_gen::far_strange::choir(settings, layout), "The Choir");
    found
}

/// Every boss arena: the eight gates' and the Living Dark and Drowned mini-bosses' lairs.
/// Each stands at a pocket over the arena (its door); `inside` is the arena's floor.
fn boss_arenas(settings: &WorldSettings, layout: &DescentLayout) -> Vec<Destination> {
    let mut found = Vec::new();
    let mut push = |rect: Option<TileRect>, boss: &str, gate: Option<Descent>| {
        let Some(rect) = rect else {
            return;
        };
        let x = i32::midpoint(rect.min.x, rect.max.x);
        let mut destination = at(
            DestinationKind::BossArena,
            x,
            rect.min.y - ARENA_DOOR_ROWS,
            boss,
        );
        let (ix, iy) = floor_middle(&rect);
        destination.inside = Some((i64::from(ix), i64::from(iy)));
        destination.info.boss = Some(boss.to_owned());
        if let Some(descent) = gate {
            destination.info.descent = Some(descent);
        }
        found.push(destination);
    };
    push(
        crate::crust::sinkhole_site(settings).map(|site| site.chamber),
        DescentBoss::TheSinkhole.name(),
        Some(Descent::Crust),
    );
    push(
        crate::old_deep::sites::star_core_arena(settings, layout).map(|arena| arena.bounds),
        DescentBoss::StarCore.name(),
        Some(Descent::OldDeep),
    );
    push(
        layout_gen::living::mind_hollow(settings, layout),
        DescentBoss::MycelialMind.name(),
        Some(Descent::LivingDark),
    );
    push(
        layout_gen::living::matriarch_gallery(settings, layout),
        "The Brood Matriarch",
        None,
    );
    push(
        layout_gen::drowned::leviathan_arena(settings, layout),
        DescentBoss::Leviathan.name(),
        Some(Descent::DrownedAndFrozen),
    );
    push(
        layout_gen::drowned::eel_grotto(settings, layout),
        "The Eel-Dragon",
        None,
    );
    push(
        layout_gen::hollow::sun_well(settings, layout).map(layout_gen::hollow::sun_well_rect),
        DescentBoss::SunSerpent.name(),
        Some(Descent::HollowWorld),
    );
    push(
        layout_gen::far_strange::glitch_arena(settings, layout),
        DescentBoss::TheGlitch.name(),
        Some(Descent::StrangeLayers),
    );
    push(
        layout_gen::garrison::colossus_hall(settings, layout),
        DescentBoss::GatekeeperColossus.name(),
        Some(Descent::Ancients),
    );
    found
}
