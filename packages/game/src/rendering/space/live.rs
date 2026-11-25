#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "presentation: tile counts and seeds become f32 sizes and u32 seeds"
)]

//! The space renderer fed with the real galaxy (Wave 11 integration): what the space
//! generator ([`fatherload_simulation::space::Space`]) placed round a camera, as the
//! renderer's plain views ([`super::view`]), and a body store's round ground as a
//! [`PolarStrip`] (the polar track's geometry: rings that halve toward the core, a ring tile
//! of band `k` kept across `2^k` strip columns).

use fatherload_simulation::{
    planet::PlanetGround,
    polar::PolarGrid,
    position::WorldPos,
    space::{self, Atmosphere, Space, SpaceBody, SpaceTile, Theme},
    terrain::{Terrain, TilePosition},
};
use raylib::prelude::Color;

use crate::screen_fx::EffectsIntensity;

use super::view::{
    AsteroidView, BeltView, BlackHoleView, BodyTheme, BodyView, CometView, CraftView, FlightMotion,
    GroundThing, GroundView, NebulaView, PolarGeometry, PolarStrip, SpaceCamera, SpaceRenderView,
    StarView, StructureKind, StructureView, WormholeView,
};

/// Space sectors' reach round the camera that is gathered (the generator's sectors are 2^22
/// tiles: a system and its neighbours).
const GATHER_TILES: i64 = space::SPACE_SECTOR_TILES * 3 / 4;

/// The renderer's look of a space theme.
#[must_use]
pub const fn body_theme(theme: Theme) -> BodyTheme {
    match theme {
        Theme::Jungle | Theme::Home => BodyTheme::Jungle,
        Theme::OceanArchipelago => BodyTheme::Ocean,
        Theme::Ice => BodyTheme::Ice,
        Theme::DesertDunes | Theme::TidallyLocked => BodyTheme::Desert,
        Theme::Volcanic => BodyTheme::Volcanic,
        Theme::Crystal => BodyTheme::Crystal,
        Theme::Fungal => BodyTheme::Fungal,
        Theme::MachineWorld | Theme::RingWorld => BodyTheme::Machine,
        Theme::GasGiant => BodyTheme::GasGiant,
        Theme::DeadWorld => BodyTheme::Dead,
        Theme::Barren | Theme::Moon => BodyTheme::Barren,
    }
}

/// The air over a body as a share of its radius (0: airless).
#[must_use]
pub const fn atmosphere_share(atmosphere: Atmosphere) -> f32 {
    match atmosphere {
        Atmosphere::Vacuum => 0.0,
        Atmosphere::Thin => 0.018,
        Atmosphere::Breathable | Atmosphere::Spores => 0.03,
        Atmosphere::Thick | Atmosphere::Toxic | Atmosphere::Scalding => 0.045,
    }
}

/// The renderer's view of a generated body.
#[must_use]
pub const fn body_view(body: &SpaceBody, id: u64) -> BodyView {
    BodyView {
        id,
        centre: body.centre.pos(),
        radius_tiles: body.radius as f32,
        theme: body_theme(body.theme),
        atmosphere: atmosphere_share(body.atmosphere),
        spin: 0.0,
        seed: body.ground_seed as u32,
    }
}

/// The renderer's ring layout of a polar grid (row 0 is the datum surface).
#[must_use]
pub fn polar_geometry(grid: &PolarGrid) -> PolarGeometry {
    PolarGeometry {
        radius_tiles: grid.radius() as f32,
        surface_row: 0,
        surface_columns: grid.surface_columns(),
        halvings: grid
            .halving_rows()
            .iter()
            .copied()
            .take(super::view::MAX_HALVINGS)
            .collect(),
        core_radius_tiles: grid.core_radius() as f32,
    }
}

/// A body store's ground as the polar renderer reads it.
pub struct LiveStrip<'a> {
    terrain: &'a Terrain,
    grid: PolarGrid,
    geometry: PolarGeometry,
}

impl<'a> LiveStrip<'a> {
    /// The strip of `terrain` (a round body's ground), if it is one.
    #[must_use]
    pub fn of(terrain: &'a Terrain) -> Option<Self> {
        let planet: &PlanetGround = terrain.planet_ground()?;
        let grid = *planet.grid();
        Some(Self {
            terrain,
            geometry: polar_geometry(&grid),
            grid,
        })
    }
}

impl PolarStrip for LiveStrip<'_> {
    fn geometry(&self) -> &PolarGeometry {
        &self.geometry
    }

    fn terrain(&self) -> &Terrain {
        self.terrain
    }

    fn tile_at(&self, band: u32, column: u32, row: i32) -> Option<TilePosition> {
        // A ring tile of band `k` is kept across `2^k` strip columns from its first one.
        let first = self
            .grid
            .first_strip_column(fatherload_simulation::polar::PolarTile { column, row });
        let _ = band;
        Some(TilePosition {
            x: i32::try_from(first).ok()?,
            y: row,
        })
    }

    fn revision(&self) -> u64 {
        self.terrain.digest()
    }
}

/// Farthest a frame looks for real belt rocks (tiles; farther, the belt's dust stands in).
pub const MAX_ROCK_REACH_TILES: i64 = 2_400;

/// The glint of an asteroid's ore (Wave 12 systems).
#[must_use]
pub const fn ore_colour(ore: space::AsteroidOre) -> Color {
    match ore {
        space::AsteroidOre::Iron => Color::new(196, 112, 84, 255),
        space::AsteroidOre::Nickel => Color::new(200, 204, 196, 255),
        space::AsteroidOre::Ice => Color::new(150, 220, 255, 255),
        space::AsteroidOre::Cobalt => Color::new(80, 120, 240, 255),
        space::AsteroidOre::Platinum => Color::new(232, 232, 240, 255),
        space::AsteroidOre::Iridium => Color::new(255, 214, 120, 255),
        space::AsteroidOre::Voidglass => Color::new(170, 90, 255, 255),
    }
}

/// Everything the generator placed round a camera, as the renderer's views (gathered once per
/// space sector the camera is in; the generator's own cache makes it cheap).
#[derive(Clone, Debug, Default)]
pub struct LiveSpace {
    seed: u64,
    /// The sector the gathering was centred on.
    around: Option<space::SpaceSector>,
    pub stars: Vec<StarView>,
    pub bodies: Vec<BodyView>,
    pub belts: Vec<BeltView>,
    pub asteroids: Vec<AsteroidView>,
    pub nebulae: Vec<NebulaView>,
    pub comets: Vec<CometView>,
    pub structures: Vec<StructureView>,
    pub black_holes: Vec<BlackHoleView>,
    pub wormholes: Vec<WormholeView>,
}

impl LiveSpace {
    /// Refreshes the gathering for a camera at `at` in the world seeded `seed` (only when the
    /// camera moved to another space sector).
    pub fn update(&mut self, seed: u64, at: WorldPos) {
        let sector = space::SpaceSector::of_pos(at);
        if self.around == Some(sector) && self.seed == seed {
            return;
        }
        *self = Self::gather(seed, at);
    }

    /// Everything round `at` (see the type docs).
    #[must_use]
    pub fn gather(seed: u64, at: WorldPos) -> Self {
        let space = Space::shared(seed);
        let centre = SpaceTile::of_pos(at);
        let mut live = Self {
            seed,
            around: Some(space::SpaceSector::of_pos(at)),
            ..Self::default()
        };
        for sector in space.sectors_near(centre, GATHER_TILES) {
            for system in &sector.systems {
                for star in &system.stars {
                    live.stars.push(StarView {
                        centre: star.centre.pos(),
                        radius_tiles: star.radius as f32,
                        class: system.class,
                        seed: (system.id ^ star.centre.x as u64) as u32,
                    });
                }
                for body in &system.bodies {
                    let id = fatherload_simulation::place::SpaceBodyKey {
                        sector: sector.coords,
                        system: system.index,
                        body: body.index,
                    }
                    .mix();
                    live.bodies.push(body_view(body, id));
                }
                for belt in &system.belts {
                    live.belts.push(BeltView {
                        centre: system.centre.pos(),
                        radius_tiles: (belt.inner + belt.outer) as f32 * 0.5,
                        width_tiles: belt.outer.saturating_sub(belt.inner) as f32,
                        density: (f32::from(belt.density) / 8.0).clamp(0.3, 3.0),
                        seed: (system.id ^ u64::from(belt.index)) as u32,
                    });
                }
                for (index, comet) in system.comets.iter().enumerate() {
                    let (dx, dy) = comet.centre.delta(comet.tail_end);
                    live.comets.push(CometView {
                        centre: comet.centre.pos(),
                        radius_tiles: f32::from(comet.nucleus_radius),
                        tail_angle: (dy as f32).atan2(dx as f32),
                        tail_tiles: (dx as f32).hypot(dy as f32),
                        seed: (system.id ^ index as u64) as u32,
                    });
                }
                for station in &system.stations {
                    live.structures.push(StructureView {
                        kind: StructureKind::Station,
                        centre: station.centre.pos(),
                        size_tiles: f32::from(station.size).min(40.0),
                        angle: 0.0,
                        seed: (system.id ^ u64::from(station.id.index)) as u32,
                    });
                }
                for wreck in &system.derelicts {
                    live.structures.push(derelict_view(wreck));
                }
                if let Some(nebula) = system.nebula {
                    live.nebulae.push(nebula_view(&nebula));
                }
            }
            for wreck in &sector.derelicts {
                live.structures.push(derelict_view(wreck));
            }
            for nebula in &sector.nebulae {
                if !live
                    .nebulae
                    .iter()
                    .any(|seen| seen.seed == nebula.id as u32 && seen.centre == nebula.centre.pos())
                {
                    live.nebulae.push(nebula_view(nebula));
                }
            }
            for hole in &sector.black_holes {
                live.black_holes.push(BlackHoleView {
                    centre: hole.centre.pos(),
                    horizon_tiles: hole.horizon as f32,
                    disc_tiles: (hole.lensing_radius as f32 * 0.35).max(hole.horizon as f32 * 3.0),
                    tilt: (hole.id % 628) as f32 / 100.0,
                });
            }
            for mouth in &sector.wormholes {
                live.wormholes.push(WormholeView {
                    centre: mouth.centre.pos(),
                    radius_tiles: mouth.radius as f32,
                    tint: Color::new(150, 120, 255, 255),
                    seed: mouth.centre.x as u32,
                });
            }
        }
        live
    }

    /// The real (solid, mineable) belt rocks within `reach_tiles` of `at` that the crew has not
    /// mined out (Wave 12 systems), refreshed every frame: they are where the flight model's
    /// rocks are, with their ore's glint.
    pub fn refresh_rocks(
        &mut self,
        seed: u64,
        at: WorldPos,
        reach_tiles: i64,
        mined: &fatherload_simulation::flight::systems::SystemsShared,
    ) {
        let space = Space::shared(seed);
        self.asteroids = space
            .asteroids_near(
                SpaceTile::of_pos(at),
                reach_tiles.clamp(0, MAX_ROCK_REACH_TILES),
            )
            .iter()
            .filter(|rock| mined.mined_from(rock.id) < rock.yield_units)
            .map(|rock| AsteroidView {
                centre: rock.centre.pos(),
                radius_tiles: f32::from(rock.radius),
                seed: (rock.id.segment.wrapping_mul(31) ^ u32::from(rock.id.index)) | 1,
                ore: Some(ore_colour(rock.ore)),
            })
            .collect();
    }

    /// The renderer's view with this gathering, from `camera`.
    #[allow(clippy::too_many_arguments, reason = "one seat's picture")]
    #[must_use]
    pub fn view<'a>(
        &'a self,
        camera: SpaceCamera,
        motion: FlightMotion,
        craft: Option<CraftView>,
        ground: Option<GroundView<'a>>,
        seconds: f32,
        effects: EffectsIntensity,
        stations: bool,
    ) -> SpaceRenderView<'a> {
        SpaceRenderView {
            camera,
            sky_seed: self.seed,
            seconds,
            stars: &self.stars,
            bodies: &self.bodies,
            belts: &self.belts,
            asteroids: &self.asteroids,
            nebulae: &self.nebulae,
            comets: &self.comets,
            structures: if stations {
                &self.structures
            } else {
                // Flight draws the stations itself (their ports and the docking guide).
                self.derelicts_only()
            },
            black_holes: &self.black_holes,
            wormholes: &self.wormholes,
            motion,
            craft,
            ground,
            effects,
        }
    }

    /// The derelicts at the end of the structure list (stations are pushed first per system,
    /// so this is the tail after the last station).
    fn derelicts_only(&self) -> &[StructureView] {
        let first = self
            .structures
            .iter()
            .rposition(|structure| structure.kind == StructureKind::Station)
            .map_or(0, |last| last + 1);
        &self.structures[first..]
    }
}

fn derelict_view(wreck: &space::Derelict) -> StructureView {
    StructureView {
        kind: StructureKind::Derelict,
        centre: wreck.centre.pos(),
        size_tiles: f32::from(wreck.size).min(60.0),
        angle: (wreck.seed % 628) as f32 / 100.0,
        seed: wreck.seed as u32,
    }
}

const fn nebula_view(nebula: &space::Nebula) -> NebulaView {
    let [r, g, b] = nebula.colour;
    NebulaView {
        centre: nebula.centre.pos(),
        radius_tiles: nebula.radius as f32,
        colours: [
            Color::new(r, g, b, 255),
            Color::new(r / 2 + 40, g / 2 + 20, b / 2 + 60, 255),
        ],
        seed: nebula.id as u32,
    }
}

/// Things on a body's ground for the polar renderer: rigs, pilots and drops of its store.
#[must_use]
pub fn ground_things(
    scene: &crate::render_snapshot::PlaceScene<'_>,
    skip_rig: Option<fatherload_simulation::PlayerId>,
) -> Vec<GroundThing> {
    use super::view::GroundThingKind;
    use fatherload_simulation::entity::EntityKind;
    let tile = f64::from(fatherload_simulation::TILE_SIZE);
    let strip = |x: f32, y: f32| {
        let (u, v) = fatherload_simulation::world::strip_point(scene.frame.world(x, y));
        (u, v as f32)
    };
    let _ = tile;
    let mut things = Vec::new();
    for rig in &scene.rigs {
        if Some(rig.player_id) == skip_rig {
            continue;
        }
        let (column, row) = strip(rig.x, rig.y);
        things.push(GroundThing {
            column,
            row: row + 1.0,
            kind: GroundThingKind::ParkedRig,
        });
    }
    for pilot in &scene.pilots {
        let (column, row) = strip(pilot.x, pilot.y);
        things.push(GroundThing {
            column,
            row: row + 1.0,
            kind: GroundThingKind::Pilot,
        });
    }
    for entity in scene.entities.iter() {
        if entity.kind() == EntityKind::Drop {
            let (column, row) = strip(entity.body.x, entity.body.y);
            things.push(GroundThing {
                column,
                row: row + 0.5,
                kind: GroundThingKind::Drop,
            });
        }
    }
    things
}
