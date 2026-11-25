//! Landmarks of a Deep Descent world ([`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)):
//! the generators behind the registry's [`LandmarkKind`]s.
//!
//! * **Outposts:** one hall per Descent in the stratum the registry marks with
//!   [`LandmarkKind::Outpost`], all on one seeded column a short way to one side of the spawn
//!   column (plated walls, glass skylights, doors on both sides). Each is a deep outpost of the
//!   logistics track (fuel, repair, selling, a claim and a respawn point), linked by the
//!   builders' cable of ancient lifts beside the halls ([`logistics_sites`]). The Hollow
//!   World's outpost stands on the cavern's ground.
//! * **Gate chambers:** a builder hall at the end of every Descent, just above its last
//!   boundary (the Heart's above the bedrock floor), where the Descent's gate set piece goes.
//!   Its masonry is ruin brick, which drills, so a gate never blocks the way down while gates
//!   are placeholders.
//! * **Anomaly rooms:** crystal-lined rooms placed per region in the strata marked with
//!   [`LandmarkKind::AnomalyRoom`] (the Strange Layers).
//! * **The Hollow World cavern** is part of the base layer ([`super::hollow_open`]).
//!
//! The other kinds name existing generators (prefab points of interest, cave ecosystems,
//! settlements, tombs), so every kind the registry uses resolves to one
//! ([`landmark_generator`]). Everything is a pure function of the settings.

use crate::{
    base::BuiltTile,
    cave::CaveTile,
    descent::{Descent, LandmarkKind, Stratum},
    logistics::sites::{ANCIENT_STATION_SPACING, LogisticsSites, OUTPOST_NAMES},
    terrain::{TileKind, TilePosition},
    worldgen::{SPAWN_TILE_X, TileRect, WorldSettings, noise::hash2, strata::SKY_ROWS},
};

use super::{DescentLayout, hollow_rows, layout};

/// Which generator builds a landmark kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LandmarkGenerator {
    /// This module's stamps (outposts, gate chambers, anomaly rooms).
    DescentStamp,
    /// The base layer's Hollow World cavern ([`super::hollow_open`]).
    HollowCavern,
    /// Prefab points of interest ([`crate::worldgen::pois`]).
    PointsOfInterest,
    /// Cave ecosystems ([`crate::cave`]).
    CaveEcosystems,
    /// Villages and underground settlements ([`crate::village`]).
    Villages,
    /// Tomb and crypt sites ([`crate::worldgen::tomb_sites`]).
    TombSites,
    /// The Living Dark's stamps and living rock ([`super::living`], Wave 8).
    LivingDark,
    /// The Drowned and Frozen's stamps and rock ([`super::drowned`], Wave 9).
    Drowned,
    /// The Hollow World's caverns and set pieces ([`super::hollow`], Wave 9).
    HollowWorld,
    /// The distortion strata's halls, the rot grid and their landmarks
    /// ([`super::distortion`], Wave 9).
    Distortion,
    /// The far Strange Layers' stamps, rock and alloy seal ([`super::far_strange`], Wave 9).
    FarStrange,
    /// The buried city's stamps and rock ([`super::ancient_city`], Wave 10).
    AncientCity,
    /// Descent VII part B's forts, the Seal Court, the Gatekeeper's hall and the Heart Seal
    /// ([`super::garrison`], Wave 10).
    Garrison,
}

/// The generator behind `kind`.
#[must_use]
pub const fn landmark_generator(kind: LandmarkKind) -> LandmarkGenerator {
    match kind {
        LandmarkKind::Outpost | LandmarkKind::GateChamber | LandmarkKind::AnomalyRoom => {
            LandmarkGenerator::DescentStamp
        }
        LandmarkKind::HollowCavern => LandmarkGenerator::HollowCavern,
        LandmarkKind::Prefabs => LandmarkGenerator::PointsOfInterest,
        LandmarkKind::CaveEcosystems => LandmarkGenerator::CaveEcosystems,
        LandmarkKind::Settlements => LandmarkGenerator::Villages,
        LandmarkKind::Tombs => LandmarkGenerator::TombSites,
        LandmarkKind::LivingDark => LandmarkGenerator::LivingDark,
        LandmarkKind::Drowned => LandmarkGenerator::Drowned,
        LandmarkKind::HollowWorld => LandmarkGenerator::HollowWorld,
        LandmarkKind::Distortion => LandmarkGenerator::Distortion,
        LandmarkKind::FarStrange => LandmarkGenerator::FarStrange,
        LandmarkKind::AncientCity => LandmarkGenerator::AncientCity,
        LandmarkKind::Garrison => LandmarkGenerator::Garrison,
    }
}

/// One stamped landmark.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Landmark {
    pub kind: LandmarkKind,
    pub descent: Descent,
    pub stratum: Stratum,
    /// Every tile the landmark stamps.
    pub bounds: TileRect,
}

/// `(width, height)` of an outpost hall, walls included.
pub const OUTPOST_SIZE: (i32, i32) = (26, 9);
/// `(width, height)` of a gate chamber, walls included.
pub const GATE_CHAMBER_SIZE: (i32, i32) = (44, 12);
/// `(width, height)` of an anomaly room, walls included.
pub const ANOMALY_SIZE: (i32, i32) = (14, 8);
/// The regions anomaly rooms are placed per.
pub const ANOMALY_REGION: (i32, i32) = (80, 64);
/// Chance (per mille) that an anomaly region in a Strange Layers stratum holds a room.
const ANOMALY_CHANCE: u64 = 450;

const SALT_OUTPOST: u64 = 0x5744_1001;
const SALT_GATE: u64 = 0x5744_1002;
const SALT_ANOMALY: u64 = 0x5744_1003;

const fn rect(x: i32, y: i32, (width, height): (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + width - 1,
            y: y + height - 1,
        },
    }
}

/// The stratum of `descent` holding landmarks of `kind` (its first such stratum).
fn stratum_with(descent: Descent, kind: LandmarkKind) -> Option<Stratum> {
    descent
        .strata()
        .find(|stratum| stratum.def().landmarks.contains(&kind))
}

/// The column every outpost hall of a world is centred on: 60-139 columns to one side of the
/// spawn column, by the seed (the builders' cable the ancient lifts run down beside).
#[must_use]
pub const fn outpost_column(settings: &WorldSettings) -> i32 {
    let roll = hash2(settings.seed, SALT_OUTPOST, -1, 0);
    let side = if roll.is_multiple_of(2) { 1 } else { -1 };
    #[allow(clippy::cast_possible_truncation, reason = "below 80")]
    let along = 60 + ((roll >> 8) % 80) as i32;
    SPAWN_TILE_X + side * along
}

/// The column of the ancient lifts' rail: two columns outside the halls' door on the far side
/// from the spawn, so a rig steps out of the hall into the lift's station pocket.
#[must_use]
pub const fn lift_column(settings: &WorldSettings) -> i32 {
    let center = outpost_column(settings);
    let half = OUTPOST_SIZE.0 / 2;
    if center >= SPAWN_TILE_X {
        // The hall spans `center - half ..= center + half - 1`.
        center + half + 1
    } else {
        center - half - 2
    }
}

/// The first station of the cable's top lift: a few rows under the ground line.
pub const LIFT_TOP_ROW: i32 = SKY_ROWS + 6;

/// The depth logistics sites of a Deep Descent world.
///
/// Wave 7 integration: F8's landmarks feed F10's generator hooks. Each Descent's outpost hall is a [`crate::logistics::Outpost`]
/// (its floor is the hall's floor), and the builders' cable runs beside them, an ancient lift
/// from just under the surface down to the first outpost and from each outpost down to the
/// next. A pure function of the settings: the generator carves its tiles
/// ([`LogisticsSites::tile_at`]) and world creation installs its records.
#[must_use]
pub fn logistics_sites(settings: &WorldSettings) -> LogisticsSites {
    let mut sites = LogisticsSites::default();
    if !settings.generator.has_descents() {
        return sites;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let column = lift_column(settings);
    let mut previous_station = LIFT_TOP_ROW;
    for descent in Descent::ALL {
        let Some(hall) = outpost(settings, &layout, descent) else {
            continue;
        };
        let floor_row = hall.bounds.max.y;
        let index = descent.index();
        let name = OUTPOST_NAMES[index.min(OUTPOST_NAMES.len() - 1)];
        sites.place_outpost(
            outpost_column(settings),
            floor_row,
            u8::try_from(index).unwrap_or(u8::MAX),
            name,
        );
        let station = floor_row - 1;
        sites.place_ancient_lift(column, previous_station, station, ANCIENT_STATION_SPACING);
        previous_station = station;
    }
    // Wave 8: the Cathedral Lift, from Cathedral Station down to the Pressure Cathedrals' nave
    // (dead until its three valves are opened; see `crate::old_deep`).
    if let Some(nave) = crate::old_deep::sites::cathedral(settings, &layout) {
        sites.place_ancient_lift(
            nave.lift_column,
            nave.top_station,
            nave.bottom_station,
            nave.bottom_station - nave.top_station,
        );
    }
    sites
}

/// The outpost of `descent` (`None` only if the registry gives the Descent none).
#[must_use]
pub fn outpost(
    settings: &WorldSettings,
    layout: &DescentLayout,
    descent: Descent,
) -> Option<Landmark> {
    let stratum = stratum_with(descent, LandmarkKind::Outpost)?;
    let roll = hash2(
        settings.seed,
        SALT_OUTPOST,
        i32::try_from(descent.index()).unwrap_or(0),
        0,
    );
    // Every outpost stands on the builders' cable: one column for the whole world, the
    // ancient lifts beside the halls (Wave 7 integration, `logistics_sites`).
    let center = outpost_column(settings);
    let x = center - OUTPOST_SIZE.0 / 2;
    let (top, bottom) = layout.rows(stratum);
    let y = if stratum == Stratum::InnerSky {
        // On the cavern's ground: its floor row is the deepest open row under the hall, plus
        // one (the hall carves into higher ground).
        let ground = (x..x + OUTPOST_SIZE.0)
            .map(|column| hollow_rows(settings, layout, column).1 + 1)
            .max()
            .unwrap_or(bottom);
        ground - OUTPOST_SIZE.1 + 1
    } else {
        // Somewhere in the middle third of the stratum.
        let span = (bottom - top + 1) / 3;
        let offset =
            i32::try_from((roll >> 24) % u64::try_from(span.max(1)).unwrap_or(1)).unwrap_or(0);
        top + span + offset - OUTPOST_SIZE.1 / 2
    };
    Some(Landmark {
        kind: LandmarkKind::Outpost,
        descent,
        stratum,
        bounds: rect(x, y, OUTPOST_SIZE),
    })
}

/// The gate chamber at the end of `descent`.
#[must_use]
pub fn gate_chamber(
    settings: &WorldSettings,
    layout: &DescentLayout,
    descent: Descent,
) -> Landmark {
    let stratum = descent.last_stratum();
    let roll = hash2(
        settings.seed,
        SALT_GATE,
        i32::try_from(descent.index()).unwrap_or(0),
        0,
    );
    let offset = i32::try_from(roll % 161).unwrap_or(0) - 80;
    let x = SPAWN_TILE_X + offset - GATE_CHAMBER_SIZE.0 / 2;
    let end = layout.rows(stratum).1 + 1;
    // Just above the boundary's transition zone (or the bedrock floor).
    let clearance = if stratum == Stratum::TheHeart {
        6
    } else {
        let (wave, half) = super::boundary_shape(stratum.index());
        wave + half + 2
    };
    let y = end - clearance - GATE_CHAMBER_SIZE.1;
    Landmark {
        kind: LandmarkKind::GateChamber,
        descent,
        stratum,
        bounds: rect(x, y, GATE_CHAMBER_SIZE),
    }
}

/// The anomaly room of region `(rx, ry)`, if it has one.
fn anomaly_room(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rx: i32,
    ry: i32,
) -> Option<Landmark> {
    let (width, height) = ANOMALY_REGION;
    let center_row = ry * height + height / 2;
    if center_row < layout.tops[0] || center_row >= layout.floor {
        return None;
    }
    let stratum = layout.stratum_of_row(center_row);
    if !stratum.def().landmarks.contains(&LandmarkKind::AnomalyRoom) {
        return None;
    }
    let roll = hash2(settings.seed, SALT_ANOMALY, rx, ry);
    if roll % 1000 >= ANOMALY_CHANCE {
        return None;
    }
    let margin = 6;
    let x = rx * width
        + margin
        + i32::try_from(
            (roll >> 12) % u64::try_from(width - ANOMALY_SIZE.0 - 2 * margin).unwrap_or(1),
        )
        .unwrap_or(0);
    let y = ry * height
        + margin
        + i32::try_from(
            (roll >> 28) % u64::try_from(height - ANOMALY_SIZE.1 - 2 * margin).unwrap_or(1),
        )
        .unwrap_or(0);
    let bounds = rect(x, y, ANOMALY_SIZE);
    // Whole rooms only, inside their stratum's layout rows.
    let (top, bottom) = layout.rows(stratum);
    if bounds.min.y < top + 4 || bounds.max.y > bottom - 4 {
        return None;
    }
    Some(Landmark {
        kind: LandmarkKind::AnomalyRoom,
        descent: stratum.descent(),
        stratum,
        bounds,
    })
}

/// Every stamped landmark whose bounds overlap `area`, outposts first, then gate chambers,
/// then anomaly rooms (by region). Empty for worlds without Descents.
#[must_use]
pub fn landmarks_in(settings: &WorldSettings, area: &TileRect) -> Vec<Landmark> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    landmarks_with(settings, &layout(settings), area)
}

/// [`landmarks_in`] with the layout already known.
#[must_use]
pub fn landmarks_with(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> Vec<Landmark> {
    let mut found: Vec<Landmark> = Descent::ALL
        .iter()
        .filter_map(|descent| outpost(settings, layout, *descent))
        .chain(
            Descent::ALL
                .iter()
                .map(|descent| gate_chamber(settings, layout, *descent)),
        )
        .filter(|landmark| landmark.bounds.intersects(area))
        .collect();
    let grown = area.expanded(ANOMALY_SIZE.0.max(ANOMALY_SIZE.1));
    let (width, height) = ANOMALY_REGION;
    for ry in grown.min.y.div_euclid(height)..=grown.max.y.div_euclid(height) {
        for rx in grown.min.x.div_euclid(width)..=grown.max.x.div_euclid(width) {
            if let Some(room) = anomaly_room(settings, layout, rx, ry)
                && room.bounds.intersects(area)
            {
                found.push(room);
            }
        }
    }
    found
}

/// The tile `landmark` stamps at `(x, y)` (inside its bounds), or `None` to leave the tile.
#[must_use]
pub const fn landmark_tile(landmark: &Landmark, x: i32, y: i32) -> Option<TileKind> {
    let bounds = landmark.bounds;
    if !bounds.contains(TilePosition { x, y }) {
        return None;
    }
    let (left, right, top, bottom) = (
        x == bounds.min.x,
        x == bounds.max.x,
        y == bounds.min.y,
        y == bounds.max.y,
    );
    let height = bounds.max.y - bounds.min.y + 1;
    let column = x - bounds.min.x;
    Some(match landmark.kind {
        LandmarkKind::Outpost => {
            // Doors three rows tall on both sides, at the floor.
            let door = (left || right) && y >= bounds.max.y - 3 && y < bounds.max.y;
            if door {
                TileKind::Air
            } else if top {
                if column % 4 == 2 {
                    TileKind::Built(BuiltTile::Glass)
                } else {
                    TileKind::Built(BuiltTile::Plating)
                }
            } else if bottom || left || right {
                TileKind::Built(BuiltTile::Plating)
            } else {
                TileKind::Air
            }
        }
        LandmarkKind::GateChamber => {
            if top && column % 11 == 5 {
                TileKind::Cave(CaveTile::RuinGlyph)
            } else if top || bottom || left || right {
                TileKind::Cave(CaveTile::RuinBrick)
            } else if column % 8 == 4 && y > bounds.min.y + 1 {
                TileKind::Cave(CaveTile::RuinPillar)
            } else {
                TileKind::Air
            }
        }
        LandmarkKind::AnomalyRoom => {
            if top || bottom || left || right {
                TileKind::Cave(CaveTile::Crystal)
            } else if y == bounds.min.y + height / 2 && column == (bounds.max.x - bounds.min.x) / 2
            {
                TileKind::Cave(CaveTile::RuinGlyph)
            } else {
                TileKind::Air
            }
        }
        LandmarkKind::HollowCavern
        | LandmarkKind::Prefabs
        | LandmarkKind::CaveEcosystems
        | LandmarkKind::Settlements
        | LandmarkKind::Tombs
        | LandmarkKind::LivingDark
        | LandmarkKind::Drowned
        | LandmarkKind::HollowWorld
        | LandmarkKind::Distortion
        | LandmarkKind::FarStrange
        | LandmarkKind::AncientCity
        | LandmarkKind::Garrison => return None,
    })
}
