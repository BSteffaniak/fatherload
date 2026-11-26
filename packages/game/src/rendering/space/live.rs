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
    AsteroidView, BeltView, BlackHoleView, BodyLook, BodyTheme, BodyView, CometView, CraftView,
    FlightMotion, GroundThing, GroundView, NebulaView, PolarGeometry, PolarStrip, SpaceCamera,
    SpaceRenderView, StarView, StructureKind, StructureView, WormholeView,
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

/// A generated body's face from orbit (Wave 13 art-audio): its planet theme's registered look
/// (`PlanetThemeHooks::orbital_look`) or, for a theme without one, its tag's preset; a world
/// the aliens settled shows their towns' lights on its night side.
#[must_use]
pub fn body_look(body: &SpaceBody, settled: bool) -> BodyLook {
    let theme = fatherload_simulation::place::planet_theme_of(body.theme);
    let mut look = if matches!(body.theme, Theme::Home) {
        BodyTheme::Home.look()
    } else {
        fatherload_simulation::planet::hooks::hooks(theme)
            .orbital_look()
            .map_or_else(
                || body_theme(body.theme).look(),
                |look| BodyLook::from_theme(&look),
            )
    };
    if settled {
        let (colour, density) = look.lights.unwrap_or((Color::new(255, 214, 140, 255), 0.0));
        look.lights = Some((colour, density.max(0.18)));
    }
    look
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

/// Whether aliens settled `body` (its night side shows their towns' lights).
fn settled(seed: u64, sector: space::SpaceSector, system: u8, body: &SpaceBody) -> bool {
    fatherload_simulation::alien::atlas::folk_of_body(
        seed,
        space::SpaceBodyId {
            sector,
            system,
            body: body.index,
        },
        body,
    ) != 0
}

/// The renderer's view of a generated body.
#[must_use]
pub fn body_view(body: &SpaceBody, id: u64, settled: bool) -> BodyView {
    let radius = (body.radius as f32).max(1.0);
    BodyView {
        id,
        centre: body.centre.pos(),
        radius_tiles: body.radius as f32,
        theme: body_theme(body.theme),
        look: Some(body_look(body, settled)),
        ring: body
            .ring
            .map(|ring| (ring.inner as f32 / radius, ring.outer as f32 / radius)),
        atmosphere: atmosphere_share(body.atmosphere),
        spin: 0.0,
        sun: None,
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
    /// Each body's clock (Wave 12 bodylife), by index into `bodies`: where its star stands
    /// ([`Self::turn_bodies`]). `None` for the home world and the Moon.
    days: Vec<Option<fatherload_simulation::bodylife::BodyClock>>,
    /// How far below the view's middle a seat standing on round ground sits (Wave 13
    /// art-audio, [`super::view::SpaceCamera::drop`]), eased toward what the tall ground round
    /// it asks for.
    pub drop: f32,
    /// Its zoom out ([`super::view::SpaceCamera::zoom`]), eased the same way.
    pub zoom: f32,
}

impl LiveSpace {
    /// Eases the seat's framing toward `target` (a share of the view's half height) over a
    /// frame of `dt` seconds: a canopy coming into view lifts the picture over about half a
    /// second whatever the frame rate.
    pub fn ease_framing(&mut self, (drop, zoom): (f32, f32), dt: f32) {
        // The first framing on the ground takes its place at once (a landing or a load).
        if self.zoom <= 0.0 {
            self.drop = drop;
            self.zoom = zoom;
            return;
        }
        // 8% a frame at 60 fps: the same curve at any frame rate.
        let share = 1.0 - 0.92_f32.powf(dt.clamp(0.0, 0.25) * 60.0);
        let ease = |now: &mut f32, target: f32| {
            *now += (target - *now) * share;
            if (target - *now).abs() < 0.001 {
                *now = target;
            }
        };
        ease(&mut self.drop, drop);
        ease(&mut self.zoom, zoom);
    }

    /// Forgets the ground framing (the seat is in flight): the next landing frames at once.
    pub const fn reset_framing(&mut self) {
        self.drop = 0.0;
        self.zoom = 0.0;
    }

    /// The framing's zoom (1 until the seat stands on round ground).
    #[must_use]
    pub fn framing_zoom(&self) -> f32 {
        if self.zoom <= 0.0 { 1.0 } else { self.zoom }
    }
}

/// Where a body's star stands as its clock says (radians clockwise from up) at body time
/// `millis` (Wave 12 bodylife): local noon at column 0 puts it straight up, and the day
/// sweeps round with the clock, so every column is lit by its own local hour as the
/// simulation reads it.
#[must_use]
pub fn body_sun(clock: &fatherload_simulation::bodylife::BodyClock, millis: u64) -> f32 {
    let turn = clock.turn(millis);
    ((0.5 - turn) * std::f64::consts::TAU).rem_euclid(std::f64::consts::TAU) as f32
}

impl LiveSpace {
    /// Turns every gathered body to body time `millis` (each under its own star, by its own
    /// day; Wave 12 bodylife).
    pub fn turn_bodies(&mut self, millis: u64) {
        for (body, day) in self.bodies.iter_mut().zip(&self.days) {
            body.sun = day.as_ref().map(|clock| body_sun(clock, millis));
        }
    }

    /// Where gathered body `id`'s star stands by its clock (`None` when it is not gathered or
    /// keeps the real star's direction).
    #[must_use]
    pub fn sun_of(&self, id: u64) -> Option<f32> {
        self.bodies
            .iter()
            .find(|body| body.id == id)
            .and_then(|body| body.sun)
    }

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
    #[allow(
        clippy::too_many_lines,
        reason = "one block per kind of thing in a system"
    )]
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
                    let settled = settled(seed, sector.coords, system.index, body);
                    live.bodies.push(body_view(body, id, settled));
                    // A ring world's great ring (and a ringed gas giant's): a dense belt of
                    // ice and rock round the body (Wave 13 planets-d).
                    if let Some(ring) = body.ring {
                        live.belts.push(BeltView {
                            centre: body.centre.pos(),
                            radius_tiles: (ring.inner + ring.outer) as f32 * 0.5,
                            width_tiles: ring.outer.saturating_sub(ring.inner) as f32,
                            density: 3.0,
                            seed: (id ^ 0x5249_4E47) as u32,
                            ring_band: true,
                        });
                    }
                    live.days.push((!body.is_home()).then(|| {
                        fatherload_simulation::bodylife::BodyClock::new(
                            body.day_seconds,
                            body.surface_columns,
                            body.ground_seed,
                        )
                    }));
                }
                for belt in &system.belts {
                    live.belts.push(BeltView {
                        centre: system.centre.pos(),
                        radius_tiles: (belt.inner + belt.outer) as f32 * 0.5,
                        width_tiles: belt.outer.saturating_sub(belt.inner) as f32,
                        density: (f32::from(belt.density) / 8.0).clamp(0.3, 3.0),
                        seed: (system.id ^ u64::from(belt.index)) as u32,
                        ring_band: false,
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
            .filter(|rock| {
                mined.mined_from(rock.id) < fatherload_simulation::flight::systems::rock_yield(rock)
            })
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

/// The buildings of the alien towns on `body` (Wave 12).
fn alien_town_things(
    scene: &crate::render_snapshot::PlaceScene<'_>,
    body: &crate::render_snapshot::PlaceBody,
    things: &mut Vec<GroundThing>,
) {
    use super::view::GroundThingKind;
    use fatherload_simulation::alien::{AlienBuildingKind, town};
    let params = body.ground.params;
    if params.folk == 0 {
        return;
    }
    let towns = town::towns_of(&params);
    for found in towns.iter() {
        for building in &found.buildings {
            let home = (building.kind == AlienBuildingKind::HomePlot)
                .then(|| scene.aliens.home_at(found.id, building.index))
                .flatten();
            let look = crate::rendering::alien_art::BuildingLook {
                raised: building.kind == AlienBuildingKind::Monument
                    && scene.aliens.monuments.contains(&found.id),
                sold: home.is_some(),
                built: home.is_some_and(|home| home.home.is_some()),
                index: building.index,
                // Wave 13 endgame: the estate's wings.
                wings: u8::try_from(
                    scene
                        .endgame
                        .estate(found.id, building.index)
                        .expansions
                        .len(),
                )
                .unwrap_or(u8::MAX),
            };
            #[allow(clippy::cast_precision_loss, reason = "a strip column")]
            let column = f64::from(building.first_column) + f64::from(building.width) * 0.5;
            things.push(GroundThing {
                column,
                #[allow(clippy::cast_precision_loss, reason = "a ground row")]
                row: found.ground_row() as f32,
                kind: GroundThingKind::AlienBuilding {
                    species: found.species,
                    kind: building.kind,
                    width: building.width,
                    height: building.height.max(1),
                    look,
                },
            });
        }
    }
}

/// Things on a body's ground for the polar renderer: the alien towns' buildings (Wave 12), then
/// rigs, pilots, drops and alien residents of its store.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one block per kind of thing on the ground, each track's"
)]
pub fn ground_things(
    scene: &crate::render_snapshot::PlaceScene<'_>,
    skip_rig: Option<fatherload_simulation::PlayerId>,
    seconds: f32,
    millis: u64,
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
    // The alien towns of the body (Wave 12): their buildings first (behind everything).
    if let Some(body) = &scene.body {
        alien_town_things(scene, body, &mut things);
    }
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
        // Raid troopers (Wave 12 hostiles): creatures standing against the body's gravity.
        if let fatherload_simulation::entity::EntityState::Enemy(state) = &entity.state
            && fatherload_simulation::hostiles::raid::is_trooper(state.kind)
        {
            let feet = entity.body.y + state.kind.contact_half().1;
            let (column, row) = strip(entity.body.x, feet);
            let attacking = matches!(
                state.mode,
                fatherload_simulation::enemy::EnemyMode::Windup
                    | fatherload_simulation::enemy::EnemyMode::Strike
            );
            let moving = entity.body.velocity_x.abs() > 4.0;
            // A stride by where it is (no clock here): it walks as it moves.
            let stride = (entity.body.x / 10.0).floor() as i64;
            things.push(GroundThing {
                column,
                row,
                kind: GroundThingKind::Raider {
                    kind: state.kind,
                    frame: if attacking {
                        2
                    } else {
                        u8::from(moving && stride.rem_euclid(2) == 1)
                    },
                    left: state.facing < 0,
                },
            });
            continue;
        }
        // The body's creatures (Wave 12 planets-a): their sprites, standing in the local up.
        if let fatherload_simulation::entity::EntityState::Enemy(state) = &entity.state {
            let feet = crate::rendering::world::enemy_feet_offset(state.kind);
            let (column, row) = strip(entity.body.x, entity.body.y + feet);
            // A burrower inside the rock is hidden there (the flat renderer shows only its
            // cracks): the wyrm's ripple below is its tell.
            let buried = state.kind.stats().locomotion
                == fatherload_simulation::enemy::Locomotion::Burrowing
                && scene
                    .terrain
                    .is_solid_at(scene.frame.tile_at(entity.body.x, entity.body.y));
            if !buried {
                things.push(GroundThing {
                    column,
                    row,
                    kind: GroundThingKind::Enemy {
                        kind: state.kind,
                        frame: u8::try_from(crate::rendering::world::ground_enemy_frame(
                            entity, state, seconds,
                        ))
                        .unwrap_or(0),
                        left: state.facing < 0,
                    },
                });
            }
            if state.kind == fatherload_simulation::enemy::EnemyKind::DuneWyrm
                && state.mode == fatherload_simulation::enemy::EnemyMode::Windup
            {
                // The ripple in the sand where it gathers itself (at the surface or deep in
                // the galleries alike).
                things.push(GroundThing {
                    column,
                    row,
                    kind: GroundThingKind::Frontier(super::view::FrontierMark::Ripple),
                });
            }
            continue;
        }
        if let fatherload_simulation::entity::EntityState::Npc(state) = entity.state {
            // An alien resident (Wave 12), unless indoors.
            if state.inside.is_none()
                && fatherload_simulation::alien::is_alien_town(state.character.village)
            {
                let x = state.x_at(scene.tick, 0.0);
                let (column, row) = strip(x, entity.body.y);
                let walking = (state.target_x - state.from_x).abs() > 0.5
                    && scene.tick.get() < state.arrive.get();
                things.push(GroundThing {
                    column,
                    row,
                    kind: GroundThingKind::AlienFolk {
                        character: state.character,
                        facing: state.facing,
                        stride: walking.then(|| (x.abs() * 0.5) as u16),
                        talking: state.talking_to.is_some(),
                    },
                });
            }
            continue;
        }
        if entity.kind() == EntityKind::Drop {
            let (column, row) = strip(entity.body.x, entity.body.y);
            things.push(GroundThing {
                column,
                row: row + 0.5,
                kind: GroundThingKind::Drop,
            });
        }
    }
    things.extend(frontier_things(scene, millis));
    things.extend(outland_things(scene, millis));
    things.extend(exotic_things(scene, millis));
    things
}

/// An exotic world's tells round the seat (Wave 13 planets-d): the Storm Belt's flickering
/// columns and bolts, falling hail and meteors with their shadows, the geysers' plumes, and an
/// awake boss's marks.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "marks over a few hundred tiles of a body's strip"
)]
fn exotic_things(scene: &crate::render_snapshot::PlaceScene<'_>, millis: u64) -> Vec<GroundThing> {
    use super::view::{ExoticMark, GroundThingKind};
    use fatherload_simulation::{
        exotic::{ExoticStyle, boss, locked, rules, style_of_ground},
        terrain::TilePosition,
    };
    let mut things = Vec::new();
    let Some(ground) = scene.terrain.planet_ground() else {
        return things;
    };
    let Some(style) = style_of_ground(ground) else {
        return things;
    };
    let Some(anchor) = scene
        .rigs
        .first()
        .map(|rig| scene.frame.tile_at(rig.x, rig.y))
    else {
        return things;
    };
    let push = |things: &mut Vec<GroundThing>, column: i32, row: i32, mark: ExoticMark| {
        things.push(GroundThing {
            column: f64::from(column) + 0.5,
            row: row as f32,
            kind: GroundThingKind::Exotic(mark),
        });
    };
    let storm = rules::storm_strength(style, ground.params().seed, millis);
    if style == ExoticStyle::GasGiant {
        for bolt in rules::bolts_near(ground, anchor, 2, millis, millis, storm) {
            let rows = u16::try_from(bolt.bottom - bolt.top + 1).unwrap_or(60);
            if bolt.telling(millis) {
                push(
                    &mut things,
                    bolt.column,
                    bolt.bottom + 1,
                    ExoticMark::BoltTell { rows },
                );
            } else if bolt.striking(millis) {
                push(
                    &mut things,
                    bolt.column,
                    bolt.bottom + 1,
                    ExoticMark::Bolt { rows },
                );
            }
        }
    }
    if matches!(style, ExoticStyle::GasGiant | ExoticStyle::Ring) {
        let meteor = style == ExoticStyle::Ring;
        for faller in rules::fallers_near(
            ground,
            style,
            anchor,
            2,
            millis,
            millis + rules::FALL_TELL_MILLIS,
            storm,
        ) {
            let Some(floor) = (faller.from_row..=faller.to_row).find(|row| {
                scene
                    .terrain
                    .tile(TilePosition {
                        x: faller.column,
                        y: *row,
                    })
                    .is_some_and(|tile| !tile.kind.is_open())
            }) else {
                continue;
            };
            let left = faller.land_millis.saturating_sub(millis) as f32;
            let progress = (1.0 - left / rules::FALL_TELL_MILLIS as f32).clamp(0.0, 1.0);
            push(
                &mut things,
                faller.column,
                floor,
                ExoticMark::FallShadow {
                    progress: (progress * 1_000.0) as u16,
                    meteor,
                },
            );
            let from = faller.from_row.max(floor - 40) as f32;
            things.push(GroundThing {
                column: f64::from(faller.column) + 0.5,
                row: from + (floor as f32 - from) * progress,
                kind: GroundThingKind::Exotic(ExoticMark::Falling { meteor }),
            });
        }
    }
    if style == ExoticStyle::Locked {
        for vent in rules::vents_near(ground, anchor, 30) {
            let alive = scene.terrain.tile(vent).is_some_and(|tile| {
                tile.kind
                    == fatherload_simulation::terrain::TileKind::Exotic(
                        fatherload_simulation::exotic::ExoticTile::GeyserVent,
                    )
            });
            if !alive {
                continue;
            }
            let phase = rules::geyser_phase(ground, vent, millis);
            if phase == rules::GeyserPhase::Quiet {
                continue;
            }
            let rows = (1..=rules::GEYSER_REACH_ROWS)
                .find(|up| {
                    scene
                        .terrain
                        .tile(TilePosition {
                            x: vent.x,
                            y: vent.y - up,
                        })
                        .is_none_or(|tile| !tile.kind.is_open())
                })
                .map_or(rules::GEYSER_REACH_ROWS, |up| up - 1);
            push(
                &mut things,
                vent.x,
                vent.y,
                ExoticMark::Plume {
                    rows: u16::try_from(rows).unwrap_or(0),
                    blowing: phase == rules::GeyserPhase::Blowing,
                    night: locked::side_of(ground, i64::from(vent.x)) == locked::Side::Night,
                },
            );
        }
    }
    // An awake boss's told attack.
    let Some(arena) = boss::arena(ground) else {
        return things;
    };
    let Some((state, body)) = scene
        .entities
        .iter()
        .find_map(|entity| match &entity.state {
            fatherload_simulation::entity::EntityState::Enemy(state)
                if state.kind == style.boss() =>
            {
                Some((*state, entity.body))
            }
            _ => None,
        })
    else {
        return things;
    };
    let told = matches!(
        state.volley,
        fatherload_simulation::world::BOSS_STEP_TELL
            | fatherload_simulation::world::BOSS_STEP_STRIKE
    );
    let Some((x, y)) = state.waypoint.filter(|_| told) else {
        return things;
    };
    let strike = state.volley == fatherload_simulation::world::BOSS_STEP_STRIKE;
    let pattern = boss::pattern(style);
    let attack = pattern[usize::from(state.pattern) % pattern.len()];
    for mark in boss::marks(&arena, attack, TilePosition { x, y }, state.phase >= 2) {
        match mark {
            boss::BossMark::Column {
                column,
                top,
                bottom,
            } => push(
                &mut things,
                column,
                bottom,
                ExoticMark::BossColumn {
                    rows: u16::try_from(bottom - top).unwrap_or(0),
                    strike,
                },
            ),
            // Rows and floors in short pieces (a thing off the view is not drawn).
            boss::BossMark::Row { row, left, right } => {
                for start in (left..right).step_by(6) {
                    push(
                        &mut things,
                        start,
                        row + 1,
                        ExoticMark::BossRow {
                            columns: u16::try_from((right - start).min(6)).unwrap_or(0),
                            strike,
                        },
                    );
                }
            }
            boss::BossMark::Floor { floor, left, right } => {
                for start in (left..right).step_by(6) {
                    push(
                        &mut things,
                        start,
                        floor,
                        ExoticMark::BossFloor {
                            columns: u16::try_from((right - start).min(6)).unwrap_or(0),
                            strike,
                        },
                    );
                }
            }
            boss::BossMark::Gale { .. } => {
                let tile = scene.frame.tile_at(body.x, body.y);
                push(&mut things, tile.x, tile.y, ExoticMark::Gale);
            }
        }
    }
    things
}

/// An outland world's marks round the seat (Wave 13 planets-c): the Grave Mantle's ghost
/// lights, the spore vents' swell and blast, the gear works' sparks while they turn.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "screen-space marks round the seat, one block per world"
)]
fn outland_things(scene: &crate::render_snapshot::PlaceScene<'_>, millis: u64) -> Vec<GroundThing> {
    use super::view::{GroundThingKind, OutlandMark};
    use fatherload_simulation::{
        planet::{
            PlanetThemeId,
            outlands::{
                OutlandTile, fungal,
                life::{self, GearPhase, VentPhase},
            },
        },
        terrain::{TileKind, TilePosition},
    };
    let mut things = Vec::new();
    let Some(ground) = scene.terrain.planet_ground() else {
        return things;
    };
    let theme = ground.params().theme;
    if life::outland_theme(theme).is_none() {
        return things;
    }
    let Some(anchor) = scene
        .rigs
        .first()
        .map(|rig| fatherload_simulation::world::strip_point(scene.frame.world(rig.x, rig.y)))
    else {
        return things;
    };
    let (column, row) = (anchor.0.floor() as i64, anchor.1.floor() as i32);
    match theme {
        PlanetThemeId::DEAD => {
            // The ghost lights of the Grave Mantle round the seat.
            for wisp in life::wisps_near(ground, column, row, millis) {
                let (x, y) = wisp.tiles();
                things.push(GroundThing {
                    column: f64::from(x),
                    row: y + 0.5,
                    kind: GroundThingKind::Outland(OutlandMark::Wisp((wisp.id & 0xFFFF) as u16)),
                });
            }
        }
        PlanetThemeId::FUNGAL => {
            // The vents round the seat: their swell, then their blast up the chimney.
            let mut seen = Vec::new();
            for dx in [-40, 0, 40] {
                for dy in [-30, 0, 30] {
                    let Some(vent) = fungal::vent_in_cell(ground, column + dx, row + dy) else {
                        continue;
                    };
                    if seen.contains(&vent.column) {
                        continue;
                    }
                    seen.push(vent.column);
                    let at = TilePosition {
                        x: vent.column as i32,
                        y: vent.row,
                    };
                    if scene.terrain.tile(at).map(|tile| tile.kind)
                        != Some(TileKind::Outland(OutlandTile::SporeVent))
                    {
                        continue;
                    }
                    let phase = life::vent_phase(ground, vent.column, millis);
                    let progress = |length: u64| {
                        // Where in the phase it is, per mille.
                        let mut start = millis;
                        while start >= 50
                            && life::vent_phase(ground, vent.column, start - 50) == phase
                        {
                            start -= 50;
                        }
                        ((millis - start).min(length) * 1_000 / length) as u16
                    };
                    match phase {
                        VentPhase::Swelling => things.push(GroundThing {
                            column: vent.column as f64 + 0.5,
                            row: vent.row as f32,
                            kind: GroundThingKind::Outland(OutlandMark::VentSwell(progress(
                                life::VENT_SWELL_MILLIS,
                            ))),
                        }),
                        VentPhase::Blasting => {
                            let through = progress(life::VENT_BLAST_MILLIS);
                            for up in 0..vent.chimney + 6 {
                                things.push(GroundThing {
                                    column: vent.column as f64 + 0.5,
                                    row: (vent.row - up) as f32,
                                    kind: GroundThingKind::Outland(OutlandMark::VentBlast(through)),
                                });
                            }
                        }
                        VentPhase::Resting => {}
                    }
                }
            }
        }
        _ => {
            // The gear works' sparks while the gears turn.
            if life::gear_phase(millis) != GearPhase::Turning {
                return things;
            }
            for dy in -14..=14 {
                for dx in -22..=22 {
                    let at = TilePosition {
                        x: (column + dx) as i32,
                        y: row + dy,
                    };
                    if scene.terrain.tile(at).map(|tile| tile.kind)
                        == Some(TileKind::Outland(OutlandTile::Gear))
                        && scene
                            .terrain
                            .tile(TilePosition {
                                x: at.x,
                                y: at.y - 1,
                            })
                            .is_some_and(|tile| tile.kind.is_open())
                    {
                        things.push(GroundThing {
                            column: f64::from(at.x) + 0.5,
                            row: at.y as f32,
                            kind: GroundThingKind::Outland(OutlandMark::GearSpin),
                        });
                    }
                }
            }
        }
    }
    things
}

/// A frontier world's marks round the seat (Wave 12 planets-b): the lava bombs in the air and
/// their shadows, the light wells' beams.
fn frontier_things(
    scene: &crate::render_snapshot::PlaceScene<'_>,
    millis: u64,
) -> Vec<GroundThing> {
    use super::view::{FrontierMark, GroundThingKind};
    use fatherload_simulation::frontier::{FrontierStyle, generate, rules, style_of_ground};
    let mut things = Vec::new();
    let Some(ground) = scene.terrain.planet_ground() else {
        return things;
    };
    let Some(style) = style_of_ground(ground) else {
        return things;
    };
    let Some(anchor) = scene
        .rigs
        .first()
        .map(|rig| fatherload_simulation::world::strip_point(scene.frame.world(rig.x, rig.y)))
    else {
        return things;
    };
    match style {
        FrontierStyle::Volcanic => {
            for cone in rules::cones_near(ground, anchor.0.floor() as i64, 120) {
                for bomb in rules::bombs_in_flight(ground, cone, millis) {
                    let column = bomb.column as f64 + 0.5;
                    let floor = fatherload_simulation::planet::BodyGround::surface_row(
                        ground,
                        bomb.column as i32,
                    );
                    let progress = (millis - bomb.launch) as f32 / rules::BOMB_FLIGHT_MILLIS as f32;
                    things.push(GroundThing {
                        column,
                        row: floor as f32,
                        kind: GroundThingKind::Frontier(FrontierMark::BombShadow(
                            (progress * 1_000.0) as u16,
                        )),
                    });
                    // Its arc: up from the cone and down onto its shadow.
                    let found = generate::cone_at(ground, cone);
                    let from = found.centre as f64 + 0.5;
                    let t = f64::from(progress);
                    let height = 30.0 * 4.0 * t * (1.0 - t);
                    things.push(GroundThing {
                        column: from + (column - from) * t,
                        row: (f64::from(-found.peak) * (1.0 - t) + f64::from(floor) * t - height)
                            as f32,
                        kind: GroundThingKind::Frontier(FrontierMark::Bomb),
                    });
                }
            }
        }
        FrontierStyle::Crystal => {
            let width = i64::from(ground.grid().surface_columns());
            for well in generate::light_wells(ground) {
                let dx = (i64::from(well.column) - anchor.0.floor() as i64).rem_euclid(width);
                if dx.min(width - dx) > 48 {
                    continue;
                }
                let beam =
                    rules::trace_beam(&well, |at| scene.terrain.tile(at).map(|tile| tile.kind));
                let mut previous = fatherload_simulation::terrain::TilePosition {
                    x: well.column,
                    y: well.lens_row,
                };
                for at in beam.path {
                    things.push(GroundThing {
                        column: f64::from(at.x) + 0.5,
                        row: at.y as f32 + 1.0,
                        kind: GroundThingKind::Frontier(FrontierMark::Beam {
                            across: at.y == previous.y,
                        }),
                    });
                    previous = at;
                }
            }
        }
        FrontierStyle::Desert => {}
    }
    things
}
