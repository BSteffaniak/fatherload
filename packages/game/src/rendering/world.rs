//! Everything drawn in world space: the sky and town behind the terrain, the terrain itself, and
//! the rigs, props and effects in front of it. All of it is pixel art on the world art grid (one
//! art pixel = [`WORLD_PIXELS_PER_ART_PIXEL`] world pixels).

#![allow(
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "procedural art mixes small hashed integers and short coordinate names"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, DrillState, FallingBoulder, HazardCloud, PlacedBomb, PlacedInfrastructure,
    terrain::{TILE_SIZE, Terrain},
};

use super::{
    pixel_art::{
        palette,
        sprite::{self, SpriteDef},
        sprites::world as props,
    },
    terrain::TerrainRenderer,
    ui_kit::{self, TextStyle},
};
use crate::{
    effects::{Explosion, SparkParticle},
    pixel_scale::{WORLD_PIXELS_PER_ART_PIXEL, snap_to_pixel_grid},
    render_snapshot::{PlayerPresentation, RelayCountdown},
};

pub(super) mod base;
mod buildings;
mod caves;
mod crust;
mod crypt;
pub(super) mod daylight;
mod distortion;
mod drops;
pub(super) use drops::drop_sprite;
mod ancient_city;
mod drowned;
pub mod enemies;
mod expedition;
mod explosives;
mod far_strange;
mod fluids;
mod fx;
pub mod garrison;
mod heart;
mod hollow;
mod homes;
mod lighting;
mod living_dark;

pub(super) use far_strange::rule_color as far_rule_color;
mod logistics;
mod lunar;
mod night;
mod old_deep;
mod pilot;
pub(super) mod place_view;
mod quest_marks;
mod rig;
mod sky;
mod sky_continents;
pub mod sky_isles;
pub mod sky_lands;
mod speed_fx;
mod supplies;
mod surface;
mod tomb_traps;
mod tombs;
mod villages;
mod weapons;

pub use enemies::{
    enemy_art_scale, enemy_faces_right, enemy_feet_offset, enemy_sprite, ground_enemy_frame,
};
pub use rig::{RigFacings, draw_rig, player_accent};

/// How far above a rig's position its body's middle is drawn (world pixels; the space view
/// turns the rig about it, Wave 11).
#[allow(clippy::cast_precision_loss, reason = "a small art size")]
pub const RIG_BODY_CENTRE_Y: f32 =
    rig::FOOT_OFFSET - crate::rendering::rig_art::BODY_HEIGHT as f32 * ART * 0.5;
pub use sky::SkyArt;

/// World pixels per art pixel.
const ART: f32 = WORLD_PIXELS_PER_ART_PIXEL;
/// World y of the town's ground line (top of row 5), in the presented frame (F7).
fn ground_y() -> f32 {
    crate::view_frame::frame().row_px(5)
}

/// The camera that draws world pixel `camera` at the origin, `zoom` physical pixels per world
/// pixel. The target is snapped to the physical pixel grid so pixel art never shimmers while the
/// camera scrolls (see [`crate::pixel_scale`]).
pub(super) fn world_camera(camera: Vector2, zoom: f32) -> Camera2D {
    let snapped_camera = Vector2::new(
        snap_to_pixel_grid(camera.x, zoom),
        snap_to_pixel_grid(camera.y, zoom),
    );
    Camera2D {
        offset: Vector2::zero(),
        target: snapped_camera,
        rotation: 0.0,
        zoom,
    }
}

/// Turns everything drawn while it lives by `angle` (radians, clockwise) about `(x, y)` in
/// world space, through raylib's matrix stack (shapes and sprites alike). Level: no-op.
struct TurnedAbout(bool);

impl TurnedAbout {
    fn new(angle: f32, x: f32, y: f32) -> Self {
        if angle == 0.0 {
            return Self(false);
        }
        // SAFETY: matrix stack calls balanced by `Drop`; they only change the transform
        // applied to vertices batched until the pop.
        unsafe {
            raylib::ffi::rlPushMatrix();
            raylib::ffi::rlTranslatef(x, y, 0.0);
            raylib::ffi::rlRotatef(angle.to_degrees(), 0.0, 0.0, 1.0);
            raylib::ffi::rlTranslatef(-x, -y, 0.0);
        }
        Self(true)
    }
}

impl Drop for TurnedAbout {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: pops the matrix pushed in `new`.
            unsafe { raylib::ffi::rlPopMatrix() };
        }
    }
}

/// Everything one view draws in world space this frame.
pub(super) struct WorldScene<'a> {
    /// The world rectangle the view shows (world pixels).
    pub visible: Rectangle,
    pub terrain_renderer: &'a TerrainRenderer,
    pub terrain: &'a Terrain,
    pub sky: &'a SkyArt,
    pub infrastructure: &'a [PlacedInfrastructure],
    pub hazards: &'a [HazardCloud],
    pub bombs: &'a [PlacedBomb],
    pub bomb_alpha: f32,
    pub boulders: &'a [FallingBoulder],
    /// Entities (item drops are drawn from it, see `drops`).
    pub entities: &'a fatherload_simulation::entity::EntityStore,
    /// Collected drops flying into their rigs.
    pub pickups: &'a [crate::effects::PickupFlight],
    pub sparks: &'a [SparkParticle],
    pub explosions: &'a [Explosion],
    /// Twin-relay vault locks counting down.
    pub relay_countdowns: &'a [RelayCountdown],
    /// Markers of the view player's quests.
    pub quest_markers: &'a [fatherload_simulation::quest::describe::QuestMarker],
    pub players: &'a [PlayerPresentation],
    /// Pilots on foot (Part II F5).
    pub pilots: &'a [crate::render_snapshot::PilotPresentation],
    /// Each rig's last facing.
    pub facings: &'a RigFacings,
    /// The rig this view follows.
    pub local: Option<&'a PlayerPresentation>,
    pub active_drill: Option<DrillState>,
    pub lost_cargo: Option<(f32, f32)>,
    /// Tiles this view's player has discovered (glows never give away hidden lava).
    pub explored: Option<&'a DiscoveredTiles>,
    /// Shell ticks (60 per second); drives every animation deterministically.
    pub ticks: u64,
    /// The view's throw aim preview, if its seat has a throwable charge selected.
    pub aim_preview: Option<&'a crate::render_snapshot::AimPreview>,
    /// Claims, base modules and fixtures (WP-BASE).
    pub bases: &'a fatherload_simulation::base::Bases,
    /// The base modules with power.
    pub powered_modules: &'a std::collections::BTreeSet<fatherload_simulation::base::StructureId>,
    /// The view's build-mode preview, while its seat is building.
    pub build_preview: Option<&'a crate::base_view::BuildPreview>,
    /// Cave critters (WP-CAVES).
    pub critters: &'a [crate::render_snapshot::CritterPresentation],
    /// Enemies and enemy shots (entities copied out of the world).
    pub enemies: &'a [fatherload_simulation::entity::Entity],
    /// Rigs still inside their post-hit invulnerability window, with the seconds left.
    pub rig_guards: &'a [(fatherload_simulation::PlayerId, f32)],
    /// The snapshot's simulation tick (enemy hurt flashes and stun timers).
    pub world_tick: u64,
    /// Projectiles in flight.
    pub projectiles: &'a [crate::render_snapshot::ProjectileView],
    /// Practice targets.
    pub targets: &'a [crate::render_snapshot::TargetView],
    /// Plasma beams being fired.
    pub beams: &'a [crate::render_snapshot::BeamView],
    /// Muzzle flashes and tracers.
    pub shots: &'a [crate::effects::ShotFlash],
    /// The view's weapon reticle.
    pub reticle: Option<crate::weapon_aim::Reticle>,
    /// The world clock (F4): the sky, the sun and Moon and the night light map follow it.
    pub clock: fatherload_simulation::clock::WorldClock,
    /// Where the Moon hangs over the world.
    pub moon: fatherload_simulation::clock::MoonAnchor,
    /// The clock's state: who is in bed (a sleeping rig's lamps are off).
    pub clock_state: &'a fatherload_simulation::clock::ClockState,
    /// The person the view's rig would talk to (name tag), if any (WP-PEOPLE).
    pub talk_target: Option<fatherload_simulation::entity::EntityId>,
    /// Plots, building sites and homes (WP-HOME).
    pub homes: &'a fatherload_simulation::home::Homes,
    /// Tomb runs (WP-TOMBS): crypt floors, trap sight and downed members.
    pub tombs: &'a fatherload_simulation::tomb::Tombs,
    /// World milliseconds of the drawn moment (side-view traps move on the world clock).
    pub world_millis: u64,
    /// The session's tick rate (crypt trap cycles count milliseconds from ticks).
    pub tick_rate_hz: u32,
    /// Lifts, outposts and beacons (Part III F10).
    pub logistics: &'a fatherload_simulation::logistics::Logistics,
    /// The authored Crust (Wave 8): the Sinkhole collapse.
    pub crust: &'a fatherload_simulation::crust::CrustState,
    /// Descent II's live state and set pieces (Wave 8).
    pub old_deep: &'a fatherload_simulation::old_deep::OldDeep,
    pub deep_sites: &'a crate::render_snapshot::DeepSites,
    /// Descent IV's live state and set pieces (Wave 9).
    pub drowned: &'a fatherload_simulation::drowned::DrownedState,
    pub drowned_sites: &'a crate::render_snapshot::DrownedSites,
    /// How much of the screen and world effects the accessibility setting lets through (the
    /// Flesh Caverns' pulse, Wave 8): 1 full, 0 off.
    pub effects_strength: f32,
    /// The Fathers' expedition journal (Wave 8 story): which canisters are read or sealed.
    pub expedition: &'a fatherload_simulation::expedition::ExpeditionState,
    /// The Hollow World's shared state (Wave 9): the Sun Serpent's fight and the flares.
    pub hollow: &'a fatherload_simulation::hollow::Hollow,
    /// The far Strange Layers (Wave 9): lamp filters, pings, amber, The Glitch's fight.
    pub far_strange: &'a fatherload_simulation::far_strange::FarStrange,
    /// The Rewrite Hall (The Glitch's arena), on Deep Descent worlds.
    pub glitch_arena: Option<fatherload_simulation::worldgen::TileRect>,
    /// The builders' buried city (Wave 10): rides, alarms, archivists, star charts.
    pub ancient_city: &'a fatherload_simulation::ancient_city::AncientCity,
    /// Descent VII part B (Wave 10): forts, defences, the gate, the Colossus's fight.
    pub garrison: &'a fatherload_simulation::garrison::Garrison,
    pub forts: &'a [fatherload_simulation::worldgen::descent::garrison::Fort],
    pub colossus_hall: Option<fatherload_simulation::worldgen::TileRect>,
    pub seal_door: Option<fatherload_simulation::worldgen::TileRect>,
    /// The Seal Gate's keys the view's rig carries.
    pub seal_keys: Option<[bool; 6]>,
    /// Descent VIII (Wave 10): the Heart's fight and the cable's light.
    pub heart: &'a fatherload_simulation::heart::Heart,
    pub heart_centre: Option<fatherload_simulation::terrain::TilePosition>,
    pub cable: Option<fatherload_simulation::world::CablePath>,
    pub cable_rows: Option<(i32, i32)>,
}

impl WorldScene<'_> {
    fn seconds(&self) -> f32 {
        self.ticks as f32 / 60.0
    }
}

/// Draws one view's world: backdrop, town, terrain, props, rigs and effects, back to front.
#[allow(
    clippy::too_many_lines,
    reason = "the world's draw order in one place: each layer is one call"
)]
pub(super) fn draw_world_scene(draw: &mut impl RaylibDraw, scene: &WorldScene<'_>) {
    // A seat in a top-down crypt run sees the crypt from above (WP-TOMBS T29), whatever the
    // other seats see.
    if let Some(run) = crypt::crypt_run_for_view(scene) {
        crypt::draw_crypt_scene(draw, scene, run);
        return;
    }
    let seconds = scene.seconds();
    let base_scene = base::BaseScene {
        bases: scene.bases,
        terrain: scene.terrain,
        powered: scene.powered_modules,
        players: scene.players,
        local: scene.local,
        explored: scene.explored,
        visible: scene.visible,
        seconds,
    };
    let look = daylight::DayLook::at(scene.clock);
    sky::draw_sky(
        draw,
        scene.sky,
        scene.terrain.settings(),
        scene.visible,
        (
            seconds,
            &sky::SkyTime {
                clock: scene.clock,
                look,
                moon: scene.moon,
            },
        ),
    );
    // The high sky of a V8 world: cloud seas, wind, updrafts and the Moon overhead (F6).
    sky_lands::draw_sky_backdrop(draw, scene.terrain.settings(), scene.visible, seconds);
    // A V9 sky's islands: jet streams and vent columns behind them (WP-SKY).
    sky_isles::draw_isle_backdrop(draw, scene.terrain.settings(), scene.visible, seconds);
    // The Hollow World's skies and its inner sun behind its open caverns (Wave 9).
    let hollow_scene = hollow::HollowScene {
        terrain: scene.terrain,
        hollow: scene.hollow,
        enemies: scene.enemies,
        visible: scene.visible,
        // The inner day runs on the world clock (it saves and moves with a skipped night).
        world_millis: scene.clock.world_millis(),
        seconds,
        effects_strength: scene.effects_strength,
    };
    hollow::draw_hollow_backdrop(draw, &hollow_scene);
    buildings::draw_town(draw, scene.visible, scene.local, seconds);
    villages::draw_villages(
        draw,
        (scene.terrain, scene.explored),
        scene.visible,
        scene.local,
        seconds,
        villages::VillagePass::Behind,
    );
    villages::draw_trails(
        draw,
        scene.terrain,
        scene.visible,
        villages::VillagePass::Behind,
    );
    // Contractor-built homes and their building sites (WP-HOME), behind the terrain.
    let look_darkness = look.darkness;
    homes::draw_homes(
        draw,
        &homes::HomesScene {
            homes: scene.homes,
            terrain: scene.terrain,
            visible: scene.visible,
            now: scene.clock.world_millis(),
            darkness: look_darkness,
            seconds,
            working: {
                let (hours, minutes) = scene.clock.hours_minutes();
                let minute = u16::from(hours) * 60 + u16::from(minutes);
                (fatherload_simulation::home::WORK_START_MINUTE
                    ..fatherload_simulation::home::WORK_END_MINUTE)
                    .contains(&minute)
            },
        },
    );
    draw_world_edges(draw, scene.terrain, scene.visible);
    // V9 sky continents (T21): back walls in their caves and temples, and their cities.
    sky_continents::draw_continent_backs(draw, scene.terrain, scene.visible);
    sky_continents::draw_sky_cities(draw, scene.terrain, scene.visible, seconds);
    scene.terrain_renderer.draw(draw, scene.visible);
    // Tomb runs (WP-TOMBS): the top-down crypt floor's flagstones over its open tiles.
    tombs::draw_tomb_floors(
        draw,
        scene.terrain,
        scene.tombs,
        scene.explored,
        scene.visible,
    );
    // Trap sight is per seat: only this view's own pilot's lantern shows hidden traps.
    let trap_sight =
        tombs::seat_trap_sight_of(scene.pilots, scene.local.map(|local| local.player_id));
    tombs::draw_tomb_marks(
        draw,
        scene.terrain,
        scene.tombs,
        trap_sight,
        scene.visible,
        seconds,
    );
    // Side-view tomb traps in motion (WP-TOMBS T28).
    tomb_traps::draw_side_traps(
        draw,
        &tomb_traps::TrapScene {
            terrain: scene.terrain,
            tombs: scene.tombs,
            explored: scene.explored,
            visible: scene.visible,
            millis: scene.world_millis,
            trap_sight,
        },
    );
    sky_continents::draw_continent_fronts(draw, scene.terrain, scene.visible, seconds);
    surface::draw_flora(draw, &scene.sky.surface, scene.terrain, scene.visible);
    surface::draw_water(draw, scene.terrain, scene.visible, scene.ticks);
    // Flora, waterfalls, buoys and keystones on the sky isles (WP-SKY).
    sky_isles::draw_isle_front(draw, scene.terrain, scene.visible, seconds);
    base::draw_back_walls(draw, &base_scene);
    villages::draw_trails(
        draw,
        scene.terrain,
        scene.visible,
        villages::VillagePass::Front,
    );
    villages::draw_villages(
        draw,
        (scene.terrain, scene.explored),
        scene.visible,
        scene.local,
        seconds,
        villages::VillagePass::Front,
    );
    fx::draw_lava_glow(draw, scene.terrain, scene.explored, scene.visible, seconds);
    caves::draw_cave_glows(draw, scene.terrain, scene.explored, scene.visible, seconds);
    crust::draw_crust_glows(draw, scene.terrain, scene.explored, scene.visible, seconds);
    // The Flesh Caverns' walls swell with the heartbeat (Wave 8).
    living_dark::draw_flesh_pulse(
        draw,
        scene.terrain,
        scene.visible,
        scene.world_millis,
        scene.effects_strength,
    );
    // The distortion strata (Wave 9): mirror halls' curtains, shear flow, breathing reflections.
    distortion::draw_distortion_world(
        draw,
        scene.terrain,
        scene.visible,
        seconds,
        scene.effects_strength,
    );
    lunar::draw_lunar_glows(draw, scene.terrain, scene.explored, scene.visible, seconds);
    lunar::draw_colony_props(draw, scene.terrain, scene.visible, seconds);
    // Descent II, the Old Deep (Wave 8): glows, jets, rams, belts, valves and the core.
    let deep_scene = old_deep::DeepScene {
        terrain: scene.terrain,
        old_deep: scene.old_deep,
        visible: scene.visible,
        world_tick: scene.world_tick,
        tick_rate_hz: scene.tick_rate_hz,
        world_millis: scene.world_millis,
        seconds,
        local: scene.local,
        cathedral: scene.deep_sites.cathedral.as_ref(),
        arena: scene.deep_sites.arena.as_ref(),
    };
    old_deep::draw_deep_world(draw, &deep_scene);
    // The Drowned and Frozen (Wave 9): geysers, clams, vents, currents, the Leviathan's tells.
    let drowned_scene = drowned::DrownedScene {
        terrain: scene.terrain,
        drowned: scene.drowned,
        sites: scene.drowned_sites,
        visible: scene.visible,
        world_millis: scene.world_millis,
        tick_rate_hz: scene.tick_rate_hz,
        seconds,
        players: scene.players,
        enemies: scene.enemies,
        effects_strength: scene.effects_strength,
    };
    drowned::draw_drowned_world(draw, &drowned_scene);
    let refraction = old_deep::local_refraction(&deep_scene);
    let beam_lights: Vec<(f32, f32, f32)> = refraction
        .beams
        .iter()
        .flat_map(|beam| {
            let frame = crate::view_frame::frame();
            let (fx, fy) = frame.tile_center(beam.from);
            let (tx, ty) = frame.tile_center(beam.to);
            (0..=4).map(move |step| {
                let t = step as f32 / 4.0;
                (
                    (tx - fx).mul_add(t, fx),
                    (ty - fy).mul_add(t, fy),
                    TILE_SIZE * 2.2,
                )
            })
        })
        .collect();
    caves::draw_critters(draw, scene.critters, scene.players, scene.visible, seconds);
    fx::draw_infrastructure(draw, scene.infrastructure, seconds);
    if let Some(drill) = scene.active_drill {
        fx::draw_drill_damage(draw, scene.terrain, drill, scene.ticks);
    }
    pilot::draw_pick_cracks(draw, scene.pilots);
    if let Some((x, y)) = scene.lost_cargo {
        fx::draw_lost_cargo(draw, x, y, seconds);
    }
    fx::draw_relay_countdowns(draw, scene.terrain, scene.relay_countdowns, seconds);
    for bomb in scene.bombs {
        if bomb.kind == fatherload_simulation::ExplosiveKind::Bomb {
            fx::draw_bomb(draw, bomb, seconds, scene.bomb_alpha);
        } else {
            explosives::draw_charge(draw, bomb, seconds, scene.bomb_alpha);
        }
    }
    for boulder in scene.boulders {
        fx::draw_boulder(draw, boulder, seconds);
    }
    villages::draw_village_people(
        draw,
        scene.terrain,
        scene.entities,
        scene.visible,
        scene.explored,
        (scene.world_tick, scene.bomb_alpha),
        scene.talk_target,
        seconds,
    );
    homes::draw_crew_hats(
        draw,
        scene.terrain,
        scene.entities,
        scene.visible,
        (scene.world_tick, scene.bomb_alpha),
        seconds,
    );
    drops::draw_drops(draw, scene);
    weapons::draw_targets(draw, scene.targets, seconds);
    quest_marks::draw_quest_markers(draw, scene.quest_markers, seconds);
    base::draw_structures(draw, &base_scene);
    // Depth logistics (Part III F10): outposts, lift rails and cabs, beacons.
    let logistics_scene = logistics::LogisticsScene {
        logistics: scene.logistics,
        visible: scene.visible,
        world_tick: scene.world_tick,
        alpha: scene.bomb_alpha,
        seconds,
        players: scene.players,
    };
    logistics::draw_logistics(draw, &logistics_scene);
    // The Sun Well's flares and the Sun Serpent's body behind its sprites (Wave 9).
    hollow::draw_sun_well(draw, &hollow_scene);
    // Descent VIII (Wave 10): the wells' horizons, the builders' cable and its light, the Star
    // Drive in its chamber.
    let wells = heart::wells_in_view(scene.terrain, scene.visible);
    let heart_scene = heart::HeartScene {
        terrain: scene.terrain,
        heart: scene.heart,
        visible: scene.visible,
        world_millis: scene.world_millis,
        seconds,
        effects_strength: scene.effects_strength,
        wells: &wells,
        centre: scene.heart_centre,
        cable: scene.cable,
        cable_rows: scene.cable_rows,
    };
    heart::draw_heart_world(draw, &heart_scene);
    // The Fathers' expedition (Wave 8 story): camps, wrecks, cairns and anomaly rooms.
    let expedition_scene = expedition::ExpeditionScene {
        settings: scene.terrain.settings(),
        expedition: scene.expedition,
        visible: scene.visible,
        seconds,
    };
    expedition::draw_expedition(draw, &expedition_scene);
    // The far Strange Layers (Wave 9): time pockets, keyed glass and lamp pools, the Rewrite
    // Hall's glyphs, The Glitch's torn channels.
    let far_scene = far_strange::FarScene {
        terrain: scene.terrain,
        visible: scene.visible,
        far: scene.far_strange,
        players: scene.players,
        bombs: scene.bombs,
        explosions: scene.explosions,
        enemies: scene.enemies,
        arena: scene.glitch_arena,
        world_millis: scene.world_millis,
        tick_rate_hz: scene.tick_rate_hz,
        world_tick: scene.world_tick,
        seconds,
        effects: scene.effects_strength,
    };
    far_strange::draw_time_air(draw, &far_scene);
    far_strange::draw_prism_light(draw, &far_scene);
    far_strange::draw_rewrite_hall(draw, &far_scene);
    far_strange::draw_glitch_channels(draw, &far_scene);
    // The builders' buried city (Wave 10): trams, lifts, pistons, crushers, belts, the Great
    // Engine's flywheel, the Orrery and the plinths' light, behind the rigs.
    let city_scene = ancient_city::CityScene {
        terrain: scene.terrain,
        visible: scene.visible,
        city: scene.ancient_city,
        players: scene.players,
        world_millis: scene.world_millis,
        tick_rate_hz: scene.tick_rate_hz,
        alpha: scene.bomb_alpha,
        seconds,
        effects: scene.effects_strength,
        local: scene.local.map(|local| local.player_id),
    };
    ancient_city::draw_machinery(draw, &city_scene);
    // Descent VII part B (Wave 10): fort lights and bays, the gate's sockets, the hall's marks
    // and collapse, dust over falling stones, then the defences.
    let garrison_scene = garrison::GarrisonScene {
        terrain: scene.terrain,
        visible: scene.visible,
        garrison: scene.garrison,
        forts: scene.forts,
        hall: scene.colossus_hall,
        door: scene.seal_door,
        keys: scene.seal_keys,
        world_tick: scene.world_tick,
        tick_rate_hz: scene.tick_rate_hz,
        seconds,
        effects: scene.effects_strength,
    };
    garrison::draw_garrison_back(draw, &garrison_scene);
    garrison::draw_defences(draw, &garrison_scene);
    if let Some(local) = scene.local {
        fx::draw_scanner_marks(draw, scene.terrain, local, seconds);
        fx::draw_poi_scanner_marks(draw, scene.terrain, local, seconds);
    }
    enemies::draw_enemies(
        draw,
        &enemies::EnemyScene {
            entities: scene.enemies,
            terrain: scene.terrain,
            explored: scene.explored,
            visible: scene.visible,
            alpha: scene.bomb_alpha,
            world_tick: scene.world_tick,
            ticks: scene.ticks,
            lamp_filter: scene.local.map_or_else(Default::default, |local| {
                scene.far_strange.filter_of(local.player_id)
            }),
            world_millis: scene.world_millis,
        },
    );
    // The Sinkhole's collapse front (Wave 8): dust boiling up the zone.
    crust::draw_sinkhole_front(draw, scene.terrain, scene.crust, scene.visible, seconds);
    for player in scene.players {
        speed_fx::draw_speed_lines(draw, player, scene.ticks);
    }
    for player in scene.players {
        // F9 craft rotation: the rig is drawn turned about its middle (the camera of its own
        // seat turns the world back the other way, so it looks upright there).
        let _turned = TurnedAbout::new(player.orientation, player.x, player.y);
        rig::draw_rig(draw, scene.terrain, player, scene.facings, scene.ticks);
        // A mine cart under a riding rig (Wave 8, the Crust's old works).
        if player.riding_cart {
            crust::draw_cart(draw, player, scene.ticks);
        }
        if scene
            .pilots
            .iter()
            .any(|pilot| pilot.player_id == player.player_id)
        {
            pilot::draw_parked_rig(draw, player, scene.facings, scene.ticks);
        }
    }
    pilot::draw_pilots(draw, scene.pilots, scene.visible, scene.ticks);
    // Every player's archivist hovering by its rig (Wave 10).
    ancient_city::draw_archivists(draw, &city_scene);
    // Fluids over the rigs and pilots in them (Part III F9).
    fluids::draw_fluids(draw, scene.terrain, scene.explored, scene.visible, seconds);
    tombs::draw_downed(draw, scene.tombs, scene.pilots, seconds);
    enemies::draw_rig_hits(draw, scene.players, scene.rig_guards, scene.ticks);
    if let Some(drill) = scene.active_drill {
        fx::draw_drill_debris(draw, scene.terrain, drill, scene.ticks);
    }
    weapons::draw_projectiles(
        draw,
        scene.projectiles,
        scene.players,
        scene.bomb_alpha,
        seconds,
    );
    weapons::draw_beams(draw, scene.beams, seconds);
    weapons::draw_sky_weapon_lines(
        draw,
        scene.players,
        |player| scene.facings.facing_left(player),
        seconds,
    );
    weapons::draw_shots(draw, scene.shots);
    for cloud in scene.hazards {
        fx::draw_gas_cloud(draw, cloud, seconds);
    }
    explosives::draw_smoke_screens(draw, scene.bombs, seconds, scene.bomb_alpha);
    // Time bubbles and amber (Wave 9, the Time Sludge).
    far_strange::draw_time_bubbles(draw, &far_scene);
    far_strange::draw_amber(draw, &far_scene);
    fx::draw_sparks(draw, scene.sparks);
    drops::draw_pickup_flights(draw, scene);
    for explosion in scene.explosions {
        if explosion.kind == fatherload_simulation::ExplosiveKind::Bomb {
            fx::draw_explosion(draw, explosion);
        } else {
            explosives::draw_blast(draw, explosion);
        }
    }
    // Night on the surface (and the warm and dusky washes of the day's ends).
    let settlement_lights = if look.darkness > 0.05 {
        let mut lights = buildings::town_lights(scene.visible);
        lights.extend(villages::village_lights(scene.terrain, scene.visible));
        lights.extend(homes::home_lights(scene.homes, scene.visible));
        lights
    } else {
        Vec::new()
    };
    lighting::draw_light_map(
        draw,
        &lighting::LightScene {
            terrain: scene.terrain,
            visible: scene.visible,
            look: &look,
            players: scene.players,
            pilots: scene.pilots,
            facings: scene.facings,
            bombs: scene.bombs,
            explosions: scene.explosions,
            critters: scene.critters,
            bases: scene.bases,
            powered: scene.powered_modules,
            settlement_lights: &settlement_lights,
            asleep: &scene.clock_state.sleepers,
            night: scene.clock.is_night(),
            enemies: scene.enemies,
            seconds,
        },
    );
    // Nocturnal ore glowing through the turf (WP-NIGHT).
    night::draw_ore_glow(
        draw,
        scene.terrain,
        scene.explored,
        scene.visible,
        scene.clock,
        seconds,
    );
    // The night market's lanterns and shut day counters (WP-NIGHT).
    villages::draw_night_markets(draw, scene.terrain, scene.visible, scene.clock, seconds);
    // Eyes and wisp-light that cut through the night (WP-NIGHT).
    enemies::draw_night_glints(
        draw,
        &enemies::EnemyScene {
            entities: scene.enemies,
            terrain: scene.terrain,
            explored: scene.explored,
            visible: scene.visible,
            alpha: scene.bomb_alpha,
            world_tick: scene.world_tick,
            ticks: scene.ticks,
            lamp_filter: scene.local.map_or_else(Default::default, |local| {
                scene.far_strange.filter_of(local.player_id)
            }),
            world_millis: scene.world_millis,
        },
        look.darkness,
    );
    caves::draw_cave_darkness(
        draw,
        &caves::DarknessScene {
            terrain: scene.terrain,
            visible: scene.visible,
            players: scene.players,
            pilots: scene.pilots,
            facings: scene.facings,
            bombs: scene.bombs,
            explosions: scene.explosions,
            critters: scene.critters,
            enemies: scene.enemies,
            seconds,
            beam_lights: &beam_lights,
        },
    );
    // The refracted headlamp cuts through the dark; embers ride a heat storm (Wave 8).
    old_deep::draw_refraction(draw, &deep_scene, &refraction);
    old_deep::draw_storm_air(
        draw,
        &deep_scene,
        scene.local.is_some_and(|local| {
            scene
                .terrain
                .stratum_at(crate::view_frame::frame().tile_at(local.x, local.y))
                == fatherload_simulation::descent::Stratum::MantleStormZone
        }),
    );
    // The Glow Sea's dark: only living lights cut it back (Wave 8).
    living_dark::draw_glow_sea_darkness(
        draw,
        &living_dark::GlowSeaScene {
            terrain: scene.terrain,
            visible: scene.visible,
            players: scene.players,
            pilots: scene.pilots,
            facings: scene.facings,
            bombs: scene.bombs,
            explosions: scene.explosions,
            enemies: scene.enemies,
            seconds,
        },
    );
    // The Echo Caves' dark: only sonar shows the cave (Wave 9).
    far_strange::draw_echo_darkness(draw, &far_scene);
    // The builders' cable lit from the Heart to the Moon, over the dark (Wave 10).
    heart::draw_cable_light(draw, &heart_scene);
    // The Mycelial Mind's nerves: wired to its nodes while they shield it, a raw core when
    // exposed (Wave 8).
    living_dark::draw_mind_nerves(draw, scene.enemies, seconds, scene.effects_strength);
    // The Drowned and Frozen's swimmers are drawn again over the water and the dark, so the
    // deep's creatures and the Leviathan read (Wave 9); then sonar rings, tells and frost.
    let swimmers: Vec<fatherload_simulation::entity::Entity> = scene
        .enemies
        .iter()
        .filter(|entity| {
            matches!(&entity.state, fatherload_simulation::entity::EntityState::Enemy(state)
                if state.kind.is_drowned() && state.kind.can_swim())
        })
        .cloned()
        .collect();
    if !swimmers.is_empty() {
        enemies::draw_enemies(
            draw,
            &enemies::EnemyScene {
                entities: &swimmers,
                terrain: scene.terrain,
                explored: scene.explored,
                visible: scene.visible,
                alpha: scene.bomb_alpha,
                world_tick: scene.world_tick,
                ticks: scene.ticks,
                lamp_filter: scene.local.map_or_else(Default::default, |local| {
                    scene.far_strange.filter_of(local.player_id)
                }),
                world_millis: scene.world_millis,
            },
        );
    }
    // ... and so are the submarines themselves: under the water's tint and the deep's dark a
    // rig all but vanished, and Descent IV is played under water (Wave 9 review).
    if scene.drowned_sites.arena.is_some() {
        for player in scene.players.iter().filter(|player| player.submerged) {
            let _turned = TurnedAbout::new(player.orientation, player.x, player.y);
            rig::draw_rig(draw, scene.terrain, player, scene.facings, scene.ticks);
        }
    }
    drowned::draw_drowned_lights(draw, &drowned_scene);
    // Flares and bait lying in the world (Wave 10 polish), over the dark.
    supplies::draw_supply_lures(
        draw,
        &supplies::SupplyScene {
            logistics: scene.logistics,
            visible: scene.visible,
            world_tick: scene.world_tick,
            seconds,
            effects: scene.effects_strength,
        },
    );
    if let Some(preview) = scene.aim_preview {
        explosives::draw_aim_preview(draw, &preview.path, preview.sticks, seconds);
    }
    base::draw_base_lights(draw, &base_scene);
    logistics::draw_logistics_lights(draw, &logistics_scene);
    expedition::draw_expedition_lights(draw, &expedition_scene);
    if let Some(preview) = scene.build_preview {
        base::draw_claims(draw, &base_scene, preview.claim.as_ref());
        base::draw_build_ghost(draw, &base_scene, preview);
    }
    if let (Some(reticle), Some(local)) = (scene.reticle, scene.local) {
        let accent = player_accent(local.player_id, Some(local.cosmetics));
        weapons::draw_reticle(draw, &reticle, accent, seconds);
    }
    surface::draw_weather(draw, scene.terrain, scene.visible, scene.ticks);
    for player in scene.players {
        let is_local = scene
            .local
            .is_some_and(|local| local.player_id == player.player_id);
        if !is_local {
            rig::draw_remote_tag(draw, player, scene.ticks);
        }
    }
    drops::draw_full_tag(draw, scene);
}

/// Solid bedrock beyond the left and right edges of the world below the ground line, and under
/// the world's last row, so the view never shows sky past the floor (deep worlds end in their
/// own bedrock rows, which this continues).
fn draw_world_edges(draw: &mut impl RaylibDraw, terrain: &Terrain, visible: Rectangle) {
    let world_left = crate::view_frame::frame().rel_col(terrain.min_x()) * TILE_SIZE;
    let world_right = crate::view_frame::frame().rel_col(terrain.max_x() + 1) * TILE_SIZE;
    let world_bottom = crate::view_frame::frame().row_px(terrain.height());
    let top = ground_y().max(visible.y);
    let bottom = visible.y + visible.height + ART;
    if bottom <= top {
        return;
    }
    let tile = props::BEDROCK.width() as f32 * ART;
    if bottom > world_bottom {
        // Anchored to the world grid like the side walls, so the floor does not swim.
        let start = crate::view_frame::pattern_start_x(visible.x, tile) - tile;
        let floor_top = world_bottom.max(top);
        let rows_top = ((floor_top - world_bottom) / tile)
            .floor()
            .mul_add(tile, world_bottom);
        sprite::draw_tiled(
            draw,
            &props::BEDROCK,
            0,
            Rectangle::new(
                start,
                rows_top,
                visible.x + visible.width + tile * 2.0 - start,
                bottom - rows_top,
            ),
            ART,
            Color::WHITE,
        );
    }
    let fill = |draw: &mut dyn FnMut(Rectangle), from: f32, to: f32| {
        // Anchor the pattern to the world grid so it does not swim while scrolling.
        let start = crate::view_frame::pattern_start_x(from, tile);
        let rows_top = ((top - ground_y()) / tile)
            .floor()
            .mul_add(tile, ground_y());
        draw(Rectangle::new(
            start,
            rows_top,
            to - start,
            bottom - rows_top,
        ));
    };
    if visible.x < world_left {
        fill(
            &mut |rect| {
                sprite::draw_tiled(draw, &props::BEDROCK, 0, rect, ART, Color::WHITE);
            },
            visible.x - ART,
            world_left,
        );
        draw.draw_rectangle_rec(
            Rectangle::new(world_left - ART, top, ART, bottom - top),
            palette::INK,
        );
    }
    let right = visible.x + visible.width + ART;
    if right > world_right {
        fill(
            &mut |rect| {
                sprite::draw_tiled(draw, &props::BEDROCK, 0, rect, ART, Color::WHITE);
            },
            world_right,
            right,
        );
        draw.draw_rectangle_rec(
            Rectangle::new(world_right, top, ART, bottom - top),
            palette::INK,
        );
    }
}

/// Snaps a world coordinate to the art-pixel grid (whole physical pixels at any snapped zoom).
fn snap(value: f32) -> f32 {
    // Halfway cases up on both sides of zero, so the art-pixel grid is the same in every local
    // frame (F7).
    crate::pixel_scale::round_half_up(value / ART) * ART
}

/// Draws `sprite` frame `frame` with its top-left at world `(x, y)`, one art pixel per
/// [`WORLD_PIXELS_PER_ART_PIXEL`] world pixels, snapped to the art-pixel grid.
fn draw_world_sprite(
    draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    x: f32,
    y: f32,
    flip: bool,
) {
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        frame,
        Vector2::new(snap(x), snap(y)),
        ART,
        flip,
        Color::WHITE,
    );
}

/// Draws in art pixels relative to an origin snapped to the art grid.
#[derive(Clone, Copy, Debug)]
struct Pen {
    x: f32,
    y: f32,
}

impl Pen {
    fn at(x: f32, y: f32) -> Self {
        Self {
            x: snap(x),
            y: snap(y),
        }
    }

    /// A `w`x`h` art-pixel rectangle at art `(x, y)`.
    fn rect(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        draw.draw_rectangle_rec(
            Rectangle::new(
                (x as f32).mul_add(ART, self.x),
                (y as f32).mul_add(ART, self.y),
                w as f32 * ART,
                h as f32 * ART,
            ),
            color,
        );
    }

    fn px(self, draw: &mut impl RaylibDraw, x: i32, y: i32, color: Color) {
        self.rect(draw, x, y, 1, 1, color);
    }

    /// A filled box with a one-pixel `INK` outline.
    fn boxed(self, draw: &mut impl RaylibDraw, x: i32, y: i32, w: i32, h: i32, fill: Color) {
        self.rect(draw, x, y, w, h, palette::INK);
        self.rect(draw, x + 1, y + 1, w - 2, h - 2, fill);
    }

    fn sprite(self, draw: &mut impl RaylibDraw, sprite: &SpriteDef, frame: usize, x: i32, y: i32) {
        draw_world_sprite(
            draw,
            sprite,
            frame,
            (x as f32).mul_add(ART, self.x),
            (y as f32).mul_add(ART, self.y),
            false,
        );
    }

    /// Bold pixel text (one font pixel per art pixel) with its top-left at art `(x, y)`.
    fn text(self, draw: &mut impl RaylibDraw, text: &str, x: i32, y: i32, color: Color) {
        ui_kit::text_shadowed(
            draw,
            text,
            (x as f32).mul_add(ART, self.x) as i32,
            (y as f32).mul_add(ART, self.y) as i32,
            TextStyle::Title,
            color,
            None,
        );
    }
}

/// Width of `text` drawn by [`Pen::text`], in art pixels.
fn text_art_width(text: &str) -> i32 {
    (ui_kit::text_width(text, TextStyle::Title) as f32 / ART).round() as i32
}
