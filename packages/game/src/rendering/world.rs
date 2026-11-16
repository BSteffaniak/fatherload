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
pub(super) mod daylight;
mod drops;
pub(super) use drops::drop_sprite;
mod enemies;
mod explosives;
mod fx;
mod homes;
mod lighting;
mod lunar;
mod night;
mod pilot;
mod quest_marks;
mod rig;
mod sky;
pub mod sky_isles;
mod sky_continents;
pub mod sky_lands;
mod speed_fx;
mod surface;
mod villages;
mod weapons;

pub use rig::{RigFacings, player_accent};
pub use sky::SkyArt;

/// World pixels per art pixel.
const ART: f32 = WORLD_PIXELS_PER_ART_PIXEL;
/// World y of the ground line (top of the first ground row).
const GROUND_Y: f32 = 5.0 * TILE_SIZE;

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
    lunar::draw_lunar_glows(draw, scene.terrain, scene.explored, scene.visible, seconds);
    lunar::draw_colony_props(draw, scene.terrain, scene.visible, seconds);
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
        },
    );
    for player in scene.players {
        speed_fx::draw_speed_lines(draw, player, scene.ticks);
    }
    for player in scene.players {
        rig::draw_rig(draw, scene.terrain, player, scene.facings, scene.ticks);
        if scene
            .pilots
            .iter()
            .any(|pilot| pilot.player_id == player.player_id)
        {
            pilot::draw_parked_rig(draw, player, scene.facings, scene.ticks);
        }
    }
    pilot::draw_pilots(draw, scene.pilots, scene.visible, scene.ticks);
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
            seconds,
        },
    );
    if let Some(preview) = scene.aim_preview {
        explosives::draw_aim_preview(draw, &preview.path, preview.sticks, seconds);
    }
    base::draw_base_lights(draw, &base_scene);
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
    let world_left = terrain.min_x() as f32 * TILE_SIZE;
    let world_right = (terrain.max_x() + 1) as f32 * TILE_SIZE;
    let world_bottom = terrain.height() as f32 * TILE_SIZE;
    let top = GROUND_Y.max(visible.y);
    let bottom = visible.y + visible.height + ART;
    if bottom <= top {
        return;
    }
    let tile = props::BEDROCK.width() as f32 * ART;
    if bottom > world_bottom {
        // Anchored to the world grid like the side walls, so the floor does not swim.
        let start = (visible.x / tile).floor() * tile - tile;
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
        let start = (from / tile).floor() * tile;
        let rows_top = ((top - GROUND_Y) / tile).floor().mul_add(tile, GROUND_Y);
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
    (value / ART).round() * ART
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
