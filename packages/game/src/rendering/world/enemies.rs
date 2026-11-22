//! Enemies and their shots in world space (WP-ENEMIES), plus the hit flash on struck rigs.
//! Presentation only: every pose and animation derives from entity state and the clocks.
//!
//! Each kind has one animated sprite (`sprites::enemies`); the mode picks frames (a mimic stays
//! a boulder while dormant, a spitter puffs up for its wind-up, a burrower opens its jaws to
//! lunge). Tells are drawn on top: a blinking "!" and a growing glow during wind-ups, sparks
//! while stunned, flames while burning, a shield bubble on guardians, and a health bar once an
//! enemy is hurt or hunting. Burrowers dig open tunnels, so they are drawn whole; one somehow
//! left inside rock (an old save, for a tick) shows only cracks and dust. Enemies
//! in tiles the view's player has not discovered stay hidden in the fog.

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles, PlayerId,
    enemy::{EnemyKind, EnemyMode, EnemyShotState, EnemyState, Locomotion, ShotKind},
    entity::{Entity, EntityState},
    terrain::{TILE_SIZE, Terrain},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{
    sprite::{self, SpriteDef},
    sprites::{
        crypt as crypt_art, enemies as art, living as living_art, moon as moon_art,
        old_deep as deep_art,
        tomb as tomb_art,
    },
};

use super::{ART, Pen, fx::disc, snap};

/// The sprite of an enemy kind (exhaustive: a new kind needs art).
#[must_use]
pub const fn enemy_sprite(kind: EnemyKind) -> &'static SpriteDef {
    match kind {
        EnemyKind::CaveBat => &art::CAVE_BAT,
        EnemyKind::Burrower => &art::BURROWER,
        EnemyKind::AcidSpitter => &art::ACID_SPITTER,
        EnemyKind::RockMimic => &art::ROCK_MIMIC,
        EnemyKind::Shellback => &art::SHELLBACK,
        EnemyKind::MagmaWisp => &art::MAGMA_WISP,
        EnemyKind::BroodNest => &art::BROOD_NEST,
        EnemyKind::BroodLord => &art::BROOD_LORD,
        EnemyKind::MiningDrone => &art::MINING_DRONE,
        EnemyKind::SentryTurret => &art::SENTRY_TURRET,
        EnemyKind::RogueExcavator => &art::ROGUE_EXCAVATOR,
        EnemyKind::AncientGuardian => &art::ANCIENT_GUARDIAN,
        EnemyKind::SkyRay => &art::SKY_RAY,
        EnemyKind::Wyvern => &art::WYVERN,
        EnemyKind::StormElemental => &art::STORM_ELEMENTAL,
        EnemyKind::PirateAirship => &art::PIRATE_AIRSHIP,
        EnemyKind::AirshipTurret => &art::AIRSHIP_TURRET,
        EnemyKind::SkyPirate => &art::SKY_PIRATE,
        EnemyKind::HarpyNester => &art::HARPY_NESTER,
        EnemyKind::RegolithCrawler => &moon_art::REGOLITH_CRAWLER,
        EnemyKind::CrystalMite => &moon_art::CRYSTAL_MITE,
        EnemyKind::DerelictAutomaton => &moon_art::DERELICT_AUTOMATON,
        EnemyKind::Warden => &moon_art::WARDEN,
        EnemyKind::ShadowHound => &art::SHADOW_HOUND,
        EnemyKind::DuneStalker => &art::DUNE_STALKER,
        EnemyKind::WillOWisp => &art::WILL_O_WISP,
        EnemyKind::MothSwarm => &art::NIGHT_MOTH,
        EnemyKind::UmbralAlpha => &art::UMBRAL_ALPHA,
        EnemyKind::Mummy => &tomb_art::MUMMY,
        EnemyKind::CryptBat => &tomb_art::CRYPT_BAT,
        EnemyKind::SkeletalArcher => &tomb_art::SKELETAL_ARCHER,
        EnemyKind::AnimatedStatue => &tomb_art::ANIMATED_STATUE,
        EnemyKind::SunColossus => &tomb_art::SUN_COLOSSUS,
        EnemyKind::HollowMatriarch => &tomb_art::HOLLOW_MATRIARCH,
        // Crypt creatures (WP-TOMBS T29) are seen from above; the crypt view draws them.
        EnemyKind::CryptSkeleton => &crypt_art::CRYPT_SKELETON,
        EnemyKind::Spectre => &crypt_art::SPECTRE,
        EnemyKind::Cultist => &crypt_art::CULTIST,
        EnemyKind::CryptGolem => &crypt_art::CRYPT_GOLEM,
        EnemyKind::CryptLord => &crypt_art::CRYPT_LORD,
        EnemyKind::BurrowMole => &art::BURROW_MOLE,
        EnemyKind::MudLeech => &art::MUD_LEECH,
        EnemyKind::PotterWasp => &art::POTTER_WASP,
        EnemyKind::MineBat => &art::MINE_BAT,
        EnemyKind::TimberBeetle => &art::TIMBER_BEETLE,
        EnemyKind::SlateSkink => &art::SLATE_SKINK,
        EnemyKind::ShelfCrab => &art::SHELF_CRAB,
        EnemyKind::TubeCrawler => &art::TUBE_CRAWLER,
        EnemyKind::CinderMoth => &art::CINDER_MOTH,
        // The Old Deep (Wave 8).
        EnemyKind::PrismMoth => &deep_art::PRISM_MOTH,
        EnemyKind::FossilEel => &deep_art::FOSSIL_EEL,
        EnemyKind::JetRay => &deep_art::JET_RAY,
        EnemyKind::GlowGrub => &deep_art::GLOW_GRUB,
        EnemyKind::Glowmother => &deep_art::GLOWMOTHER,
        EnemyKind::PistonWarden => &deep_art::PISTON_WARDEN,
        EnemyKind::VoidJelly => &deep_art::VOID_JELLY,
        EnemyKind::MagmaSalamander => &deep_art::MAGMA_SALAMANDER,
        // The Living Dark (Wave 8).
        EnemyKind::SporePuffer => &living_art::SPORE_PUFFER,
        EnemyKind::CapCrawler => &living_art::CAP_CRAWLER,
        EnemyKind::GlowGnat => &living_art::GLOW_GNAT,
        EnemyKind::RootSnapper => &living_art::ROOT_SNAPPER,
        EnemyKind::BarkBeetle => &living_art::BARK_BEETLE,
        EnemyKind::ThornWisp => &living_art::THORN_WISP,
        EnemyKind::HiveSoldier => &living_art::HIVE_SOLDIER,
        EnemyKind::HiveDrone => &living_art::HIVE_DRONE,
        EnemyKind::AcidLancer => &living_art::ACID_LANCER,
        EnemyKind::SwarmMatriarch => &living_art::SWARM_MATRIARCH,
        EnemyKind::MawWorm => &living_art::MAW_WORM,
        EnemyKind::BilePolyp => &living_art::BILE_POLYP,
        EnemyKind::Leechling => &living_art::LEECHLING,
        EnemyKind::LureFish => &living_art::LURE_FISH,
        EnemyKind::GlowJelly => &living_art::GLOW_JELLY,
        EnemyKind::Angler => &living_art::ANGLER,
        EnemyKind::MycelialMind => &living_art::MYCELIAL_MIND,
        EnemyKind::MindNode => &living_art::MIND_NODE,
    }
}

/// The sprite of an enemy shot (exhaustive: a new shot needs art).
#[must_use]
pub const fn shot_sprite(shot: ShotKind) -> &'static SpriteDef {
    match shot {
        ShotKind::Acid => &art::SHOT_ACID,
        ShotKind::Fireball => &art::SHOT_FIREBALL,
        ShotKind::Bolt => &art::SHOT_BOLT,
        ShotKind::Cannonball => &art::SHOT_CANNONBALL,
        ShotKind::Wispfire => &art::SHOT_WISPFIRE,
        ShotKind::Arrow => &tomb_art::SHOT_ARROW,
        ShotKind::Rubble => &tomb_art::SHOT_RUBBLE,
        ShotKind::Hex => &tomb_art::SHOT_HEX,
        ShotKind::CryptHex => &crypt_art::SHOT_HEX,
        ShotKind::CryptArrow => &crypt_art::SHOT_ARROW,
        ShotKind::Boulder => &crypt_art::SHOT_BOULDER,
        ShotKind::Spore => &living_art::SHOT_SPORE,
        ShotKind::Thorn => &living_art::SHOT_THORN,
        ShotKind::MindSpore => &living_art::SHOT_MIND_SPORE,
    }
}

/// What one view needs to draw enemies.
pub(super) struct EnemyScene<'a> {
    pub entities: &'a [Entity],
    pub terrain: &'a Terrain,
    pub explored: Option<&'a DiscoveredTiles>,
    pub visible: Rectangle,
    /// Presentation fraction between the last two fixed ticks.
    pub alpha: f32,
    /// The simulation tick of the snapshot (hurt flashes, stun timers).
    pub world_tick: u64,
    /// Shell ticks (60 per second) for animation.
    pub ticks: u64,
}

impl EnemyScene<'_> {
    #[allow(clippy::cast_precision_loss, reason = "animation clock")]
    fn seconds(&self) -> f32 {
        self.ticks as f32 / 60.0
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "world pixels map to tiles for the fog test"
    )]
    fn hidden(&self, x: f32, y: f32) -> bool {
        let tile = crate::view_frame::frame().tile_at(x, y);
        // Open sky (above the local ground, mountains included) is never fogged.
        tile.y >= self.terrain.surface_row(tile.x).min(0)
            && self
                .explored
                .is_some_and(|explored| !explored.contains(tile))
    }

    fn on_screen(&self, x: f32, y: f32, margin: f32) -> bool {
        x > self.visible.x - margin
            && x < self.visible.x + self.visible.width + margin
            && y > self.visible.y - margin
            && y < self.visible.y + self.visible.height + margin
    }
}

/// The rim light of a kind that gets one (Wave 6 art pass): small or dark enemies outlined so
/// they read at game scale against a night, space or Moon sky. Their sprites are in
/// `sprites::SILHOUETTED`.
const fn rim_color(kind: EnemyKind) -> Option<Color> {
    match kind {
        EnemyKind::ShadowHound => Some(SKY_LIGHT),
        EnemyKind::DuneStalker => Some(SAND),
        EnemyKind::UmbralAlpha => Some(LAVENDER),
        EnemyKind::WillOWisp | EnemyKind::Warden => Some(MINT),
        EnemyKind::Wyvern | EnemyKind::SkyRay | EnemyKind::HarpyNester => Some(CREAM),
        EnemyKind::RegolithCrawler | EnemyKind::CrystalMite | EnemyKind::DerelictAutomaton => {
            Some(ICE)
        }
        _ => None,
    }
}

/// How dark the backdrop behind an enemy at world y `y` is regardless of the hour (0..1): the
/// sky darkens to space from ~300 tiles up, and the Moon's creatures live under a black sky or
/// in the dark facility. (Night darkness adds its own rim in [`draw_night_glints`].)
fn dark_backdrop(kind: EnemyKind, y: f32) -> f32 {
    if kind.is_lunar() {
        return 1.0;
    }
    ((-crate::view_frame::frame().legacy_y(y) / TILE_SIZE - 300.0) / 300.0).clamp(0.0, 1.0)
}

/// An outline one art pixel wide around a sprite's silhouette, in `color`.
fn draw_rim(
    draw: &mut impl RaylibDraw,
    sprite: &SpriteDef,
    frame: usize,
    position: Vector2,
    scale: f32,
    flip: bool,
    color: Color,
) {
    for (dx, dy) in [
        (-1.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ] {
        sprite::draw_silhouette(
            draw,
            sprite,
            frame,
            Vector2::new(position.x + dx * scale, position.y + dy * scale),
            scale,
            flip,
            color,
        );
    }
}

/// Draws every visible enemy and shot.
pub(super) fn draw_enemies(draw: &mut impl RaylibDraw, scene: &EnemyScene<'_>) {
    for entity in scene.entities {
        let (x, y) = entity.body.lerp_position(scene.alpha);
        if !scene.on_screen(x, y, TILE_SIZE * 2.0) || scene.hidden(x, y) {
            continue;
        }
        match &entity.state {
            EntityState::Enemy(state) => draw_enemy(draw, scene, entity, state, (x, y)),
            EntityState::EnemyShot(shot) => {
                draw_shot(draw, scene, *shot, (x, y), entity.body.velocity_x < 0.0);
            }
            _ => {}
        }
    }
}

/// The frame showing `state` (mode-driven poses, otherwise the loop).
#[allow(clippy::cast_precision_loss, reason = "per-enemy animation phase")]
#[allow(clippy::match_same_arms, reason = "one arm per kind's pose rule")]
#[allow(clippy::too_many_lines, reason = "one pose rule per kind")]
fn enemy_frame(entity: &Entity, state: &EnemyState, seconds: f32) -> usize {
    let sprite = enemy_sprite(state.kind);
    let phase = seconds + (state.seed % 97) as f32 * 0.013;
    let moving = entity.body.velocity_x.abs() > 4.0;
    let attacking = matches!(state.mode, EnemyMode::Windup | EnemyMode::Strike);
    match state.kind {
        EnemyKind::Burrower
        | EnemyKind::BroodLord
        | EnemyKind::DuneStalker
        | EnemyKind::BurrowMole => usize::from(attacking),
        // The Crust's walkers (Wave 8): a stride while moving, the attack pose winding up.
        EnemyKind::TimberBeetle
        | EnemyKind::SlateSkink
        | EnemyKind::ShelfCrab
        | EnemyKind::TubeCrawler => {
            if attacking {
                2
            } else if moving {
                sprite.frame_at(phase) % 2
            } else {
                0
            }
        }
        EnemyKind::MudLeech => {
            if moving {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::ShadowHound => {
            if attacking {
                2
            } else if moving {
                ((phase * 7.0) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::AcidSpitter => {
            if state.mode == EnemyMode::Windup {
                2
            } else if moving {
                ((phase * 6.0) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::RockMimic | EnemyKind::AnimatedStatue => match state.mode {
            EnemyMode::Dormant => 0,
            EnemyMode::Windup | EnemyMode::Strike => 2,
            _ => 1,
        },
        // Tomb walkers (T28): a stride while moving, their attack pose winding up.
        EnemyKind::Mummy | EnemyKind::SkeletalArcher => {
            if attacking {
                2
            } else if moving {
                ((phase * 4.5) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::SunColossus => {
            if attacking {
                2
            } else if moving {
                ((phase * 2.5) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::HollowMatriarch => {
            if attacking {
                2
            } else {
                ((phase * 3.0) as usize) % 2
            }
        }
        EnemyKind::SentryTurret => usize::from(
            state.mode == EnemyMode::Windup && ((phase * 12.0) as usize).is_multiple_of(2),
        ),
        EnemyKind::Shellback | EnemyKind::RogueExcavator | EnemyKind::UmbralAlpha => {
            if moving || attacking {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::MiningDrone | EnemyKind::Warden if state.mode == EnemyMode::Stunned => 0,
        EnemyKind::AirshipTurret => usize::from(
            state.mode == EnemyMode::Windup && ((phase * 12.0) as usize).is_multiple_of(2),
        ),
        // A swooper folds its wings in the dive.
        EnemyKind::SkyRay | EnemyKind::HarpyNester | EnemyKind::Wyvern
            if state.mode == EnemyMode::Strike =>
        {
            sprite
                .frames
                .len()
                .saturating_sub(1)
                .min(if state.kind == EnemyKind::SkyRay {
                    2
                } else {
                    1
                })
        }
        EnemyKind::RegolithCrawler => {
            if attacking {
                1
            } else if moving {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::DerelictAutomaton => {
            if moving {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::CrystalMite
        | EnemyKind::Warden
        | EnemyKind::CaveBat
        | EnemyKind::MagmaWisp
        | EnemyKind::BroodNest
        | EnemyKind::MiningDrone
        | EnemyKind::AncientGuardian
        | EnemyKind::SkyRay
        | EnemyKind::Wyvern
        | EnemyKind::StormElemental
        | EnemyKind::PirateAirship
        | EnemyKind::SkyPirate
        | EnemyKind::HarpyNester
        | EnemyKind::WillOWisp
        | EnemyKind::MothSwarm
        | EnemyKind::CryptBat
        | EnemyKind::CryptSkeleton
        | EnemyKind::Spectre
        | EnemyKind::Cultist
        | EnemyKind::CryptGolem
        | EnemyKind::CryptLord
        | EnemyKind::PotterWasp
        | EnemyKind::MineBat
        | EnemyKind::CinderMoth
        | EnemyKind::PrismMoth
        | EnemyKind::JetRay
        | EnemyKind::GlowGrub
        | EnemyKind::Glowmother
        | EnemyKind::VoidJelly => sprite.frame_at(phase),
        // A lunge's wind-up shows the drawn-back pose; walkers stride while moving.
        EnemyKind::PistonWarden | EnemyKind::MagmaSalamander => {
            if state.mode == EnemyMode::Windup {
                1
            } else if moving && state.kind == EnemyKind::MagmaSalamander {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        EnemyKind::FossilEel => {
            if moving || attacking {
                sprite.frame_at(phase)
            } else {
                0
            }
        }
        // The Living Dark (Wave 8): walkers stride, a puffer swells before it lobs, the
        // snapper lies still until it wakes, the matriarch lays while she breeds.
        EnemyKind::SporePuffer | EnemyKind::AcidLancer => {
            if state.mode == EnemyMode::Windup {
                2
            } else if moving {
                ((phase * 6.0) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::CapCrawler => {
            if attacking {
                2
            } else if moving {
                ((phase * 8.0) as usize) % 2
            } else {
                0
            }
        }
        EnemyKind::RootSnapper => match state.mode {
            EnemyMode::Dormant => 0,
            EnemyMode::Windup | EnemyMode::Strike => 2,
            _ => 1,
        },
        EnemyKind::SwarmMatriarch => {
            if state.mode == EnemyMode::Windup {
                2
            } else {
                ((phase * 1.5) as usize) % 2
            }
        }
        EnemyKind::BarkBeetle | EnemyKind::MawWorm | EnemyKind::Angler => usize::from(attacking),
        EnemyKind::GlowGnat
        | EnemyKind::ThornWisp
        | EnemyKind::HiveSoldier
        | EnemyKind::HiveDrone
        | EnemyKind::BilePolyp
        | EnemyKind::Leechling
        | EnemyKind::LureFish
        | EnemyKind::GlowJelly
        | EnemyKind::MycelialMind
        | EnemyKind::MindNode => sprite.frame_at(phase),
    }
}

/// World pixels per art pixel of a kind's sprite: the side-view tomb guardians (T28) are drawn
/// half as large again, towering over a pilot on foot.
const fn sprite_scale(kind: EnemyKind) -> f32 {
    match kind {
        EnemyKind::SunColossus | EnemyKind::HollowMatriarch | EnemyKind::MycelialMind => ART * 1.5,
        _ => ART,
    }
}

/// Whether a kind's art faces right and should mirror to face left.
const fn faces(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::Burrower
            | EnemyKind::AcidSpitter
            | EnemyKind::Shellback
            | EnemyKind::SentryTurret
            | EnemyKind::RogueExcavator
            | EnemyKind::Wyvern
            | EnemyKind::PirateAirship
            | EnemyKind::AirshipTurret
            | EnemyKind::SkyPirate
            | EnemyKind::RegolithCrawler
            | EnemyKind::DerelictAutomaton
            | EnemyKind::ShadowHound
            | EnemyKind::DuneStalker
            | EnemyKind::UmbralAlpha
            | EnemyKind::Mummy
            | EnemyKind::SkeletalArcher
            | EnemyKind::BurrowMole
            | EnemyKind::MudLeech
            | EnemyKind::TimberBeetle
            | EnemyKind::SlateSkink
            | EnemyKind::ShelfCrab
            | EnemyKind::TubeCrawler
            // The Old Deep's (Wave 8) asymmetric kinds.
            | EnemyKind::FossilEel
            | EnemyKind::GlowGrub
            | EnemyKind::Glowmother
            | EnemyKind::PistonWarden
            | EnemyKind::MagmaSalamander
            | EnemyKind::SporePuffer
            | EnemyKind::CapCrawler
            | EnemyKind::BarkBeetle
            | EnemyKind::HiveSoldier
            | EnemyKind::HiveDrone
            | EnemyKind::AcidLancer
            | EnemyKind::SwarmMatriarch
            | EnemyKind::MawWorm
            | EnemyKind::Leechling
            | EnemyKind::LureFish
            | EnemyKind::Angler
    )
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    reason = "presentation math on small bounded values; one pass per enemy"
)]
fn draw_enemy(
    draw: &mut impl RaylibDraw,
    scene: &EnemyScene<'_>,
    entity: &Entity,
    state: &EnemyState,
    (x, y): (f32, f32),
) {
    let seconds = scene.seconds();
    let stats = state.kind.stats();
    let sprite = enemy_sprite(state.kind);
    let scale = sprite_scale(state.kind);
    let (width, height) = (
        sprite.width() as f32 * scale,
        sprite.height() as f32 * scale,
    );
    let grounded = matches!(
        stats.locomotion,
        Locomotion::Walking | Locomotion::Stationary
    );
    // Walkers stand on their collision box's floor; flyers and burrowers centre on it.
    let left = x - width * 0.5;
    let top = if grounded {
        y + stats.radius - height + ART
    } else {
        y - height * 0.5
    };
    let world_tick = scene.world_tick;
    let stunned = state.mode == EnemyMode::Stunned;
    // A burrower inside rock is only cracks and dust.
    if stats.locomotion == Locomotion::Burrowing && in_rock(scene.terrain, x, y) {
        draw_burrow_trace(draw, state, x, y, seconds);
        return;
    }
    // A dune stalker bursting out of the ground throws a plume of sand (WP-NIGHT).
    if state.kind == EnemyKind::DuneStalker && state.mode == EnemyMode::Strike {
        for grain in 0..14 {
            let g = grain as f32;
            let spread = (g * 2.399).sin();
            let fall = (seconds * 3.0 + g * 0.37).fract() * 12.0;
            disc(
                draw,
                spread.mul_add(stats.radius * 1.6, x),
                (fall * ART).mul_add(-0.3, y + stats.radius + g * 0.9),
                1 + (grain % 2),
                with_alpha(if grain % 3 == 0 { SAND } else { CLAY }, 190),
            );
        }
        disc(draw, x, y + stats.radius, 8, with_alpha(SAND, 45));
    }
    // A moth swarm is a whirl of moths round its centre (WP-NIGHT).
    if state.kind == EnemyKind::MothSwarm {
        draw_moth_swarm(draw, state, x, y, seconds, world_tick);
        return;
    }
    let hurt = state.hurt_until.get() > world_tick;
    let tint = if hurt {
        PINK
    } else if stunned && (seconds * 10.0) as i32 % 2 == 0 {
        SKY_LIGHT
    } else {
        Color::WHITE
    };
    let shake = if state.mode == EnemyMode::Windup && (seconds * 30.0) as i32 % 2 == 0 {
        ART
    } else {
        0.0
    };
    // Glows behind the body.
    match state.kind {
        EnemyKind::MagmaWisp => {
            let pulse = ((seconds * 5.0).sin() * 1.5) as i32;
            disc(draw, x, y, 9 + pulse, with_alpha(ORANGE, 40));
            disc(draw, x, y, 6, with_alpha(AMBER, 50));
        }
        EnemyKind::AncientGuardian => {
            disc(draw, x, y - ART * 2.0, 8, with_alpha(MINT, 30));
        }
        EnemyKind::Warden => {
            // A slow violet pulse; brighter while its shield holds.
            let pulse = ((seconds * 2.5).sin() * 2.0) as i32;
            let shielded = state.shield > 0.0;
            disc(
                draw,
                x,
                y,
                16 + pulse,
                with_alpha(if shielded { MINT } else { VIOLET }, 28),
            );
            disc(draw, x, y, 10, with_alpha(VIOLET, 40));
        }
        EnemyKind::CrystalMite => {
            disc(draw, x, y, 4, with_alpha(PINK, 40));
        }
        EnemyKind::BroodNest => {
            disc(draw, x, y + ART * 4.0, 7, with_alpha(LIME, 25));
        }
        // Night monsters (WP-NIGHT): a wisp's cold halo, the alpha's violet murk, a hound's
        // trailing smoke.
        EnemyKind::WillOWisp => {
            let pulse = ((seconds * 3.0).sin() * 2.0) as i32;
            disc(draw, x, y, 12 + pulse, with_alpha(MINT, 26));
            disc(draw, x, y, 7, with_alpha(MINT, 40));
        }
        EnemyKind::UmbralAlpha => {
            let pulse = ((seconds * 2.0).sin() * 2.0) as i32;
            disc(draw, x, y - ART * 2.0, 20 + pulse, with_alpha(PLUM, 50));
            disc(draw, x, y - ART * 2.0, 13, with_alpha(VIOLET, 40));
        }
        // Tomb guardians (T28): the colossus' sun glows through its chest, the matriarch
        // trails a violet murk.
        EnemyKind::SunColossus => {
            let pulse = ((seconds * 3.0).sin() * 2.0) as i32;
            disc(draw, x, y - ART * 4.0, 14 + pulse, with_alpha(AMBER, 34));
        }
        EnemyKind::HollowMatriarch => {
            let pulse = ((seconds * 2.0).sin() * 2.0) as i32;
            disc(draw, x, y, 22 + pulse, with_alpha(PLUM, 46));
            disc(draw, x, y - ART * 6.0, 9, with_alpha(MINT, 26));
        }
        EnemyKind::AnimatedStatue if state.mode != EnemyMode::Dormant => {
            disc(draw, x, y - ART * 6.0, 5, with_alpha(AMBER, 40));
        }
        // The Living Dark (Wave 8): the Glow Sea's creatures are its only light; the Mind and
        // its nodes pulse with spore light.
        EnemyKind::GlowJelly => {
            let pulse = ((seconds * 2.2 + state.seed as f32 * 0.01).sin() * 3.0) as i32;
            disc(draw, x, y - ART * 2.0, 15 + pulse, with_alpha(MINT, 34));
            disc(draw, x, y - ART * 2.0, 8, with_alpha(ICE, 44));
        }
        EnemyKind::LureFish => {
            disc(draw, x, y, 10, with_alpha(MINT, 30));
        }
        EnemyKind::Angler => {
            // The bait dangles ahead of the jaws on its rod.
            let side = f32::from(state.facing.signum());
            let bob = (seconds * 2.0).sin() * ART;
            let (bx, by) = (side.mul_add(ART * 3.0, x), y - ART * 6.5 + bob);
            disc(draw, bx, by, 9, with_alpha(MINT, 40));
            disc(draw, bx, by, 4, with_alpha(ICE, 90));
        }
        EnemyKind::GlowGnat | EnemyKind::SporePuffer => {
            disc(draw, x, y - ART * 3.0, 6, with_alpha(LIME, 30));
        }
        EnemyKind::MycelialMind => {
            let pulse = ((seconds * 1.6).sin() * 4.0) as i32;
            disc(draw, x, y, 40 + pulse, with_alpha(PLUM, 60));
            disc(draw, x, y, 26 + pulse / 2, with_alpha(VIOLET, 46));
            disc(draw, x, y - ART * 2.0, 12, with_alpha(LIME, 34));
        }
        EnemyKind::MindNode => {
            let pulse = ((seconds * 3.0).sin() * 2.0) as i32;
            disc(draw, x, y + ART, 11 + pulse, with_alpha(LIME, 34));
        }
        EnemyKind::ShadowHound => {
            for puff in 0..3 {
                let drift = (seconds * 1.5 + puff as f32 * 0.33).fract();
                let side = -f32::from(state.facing);
                disc(
                    draw,
                    side.mul_add(stats.radius + drift * 10.0, x),
                    y - drift * 6.0,
                    2 + (drift * 2.0) as i32,
                    with_alpha(SLATE, (70.0 * (1.0 - drift)) as u8),
                );
            }
        }
        _ => {}
    }
    let frame = enemy_frame(entity, state, seconds);
    let position = Vector2::new(snap(left + shake), snap(top));
    let flip = faces(state.kind) && state.facing < 0;
    // A rim light against a dark backdrop (Wave 6): the black sky up high and the Moon's.
    if let Some(rim) = rim_color(state.kind) {
        let backdrop = dark_backdrop(state.kind, y);
        if backdrop > 0.0 {
            draw_rim(
                draw,
                sprite,
                frame,
                position,
                scale,
                flip,
                with_alpha(rim, (backdrop * 170.0) as u8),
            );
        }
    }
    sprite::draw_sprite_flipped(draw, sprite, frame, position, scale, flip, tint);
    let pen = Pen::at(left, top);
    let art_width = sprite.width();
    // Wind-up tells.
    if state.mode == EnemyMode::Windup {
        if (seconds * 8.0) as i32 % 2 == 0 {
            let cx = art_width / 2 - 1;
            pen.rect(draw, cx, -8, 3, 6, INK);
            pen.rect(draw, cx + 1, -7, 1, 3, RED);
            pen.rect(draw, cx, -1, 3, 2, INK);
            pen.px(draw, cx + 1, -1, RED);
        }
        let charge = (seconds * 6.0).fract();
        let glow = match state.kind {
            EnemyKind::AcidSpitter | EnemyKind::BroodLord => Some(LIME),
            // Tomb creatures (T28): ember eyes, the archer's red spark.
            EnemyKind::MagmaWisp
            | EnemyKind::Mummy
            | EnemyKind::AnimatedStatue
            | EnemyKind::SunColossus => Some(AMBER),
            EnemyKind::SentryTurret
            | EnemyKind::AncientGuardian
            | EnemyKind::MiningDrone
            | EnemyKind::SlateSkink
            | EnemyKind::TimberBeetle => Some(GOLD),
            EnemyKind::RegolithCrawler => Some(BONE),
            EnemyKind::RogueExcavator
            | EnemyKind::Shellback
            | EnemyKind::RockMimic
            | EnemyKind::DerelictAutomaton
            | EnemyKind::UmbralAlpha
            | EnemyKind::SkeletalArcher
            | EnemyKind::ShelfCrab
            | EnemyKind::BurrowMole => Some(RED),
            EnemyKind::Warden | EnemyKind::ShadowHound | EnemyKind::WillOWisp => Some(MINT),
            EnemyKind::DuneStalker => Some(SAND),
            EnemyKind::HollowMatriarch => Some(LAVENDER),
            // The Crust (Wave 8): the crawler's glowing throat.
            EnemyKind::TubeCrawler => Some(ORANGE),
            _ => None,
        };
        if let Some(glow) = glow {
            let side = if faces(state.kind) {
                f32::from(state.facing)
            } else {
                0.0
            };
            let (gx, gy) = (side.mul_add(stats.radius, x), y - ART);
            disc(
                draw,
                gx,
                gy,
                2 + (charge * 3.0) as i32,
                with_alpha(glow, 110),
            );
            disc(draw, gx, gy, 1, with_alpha(CREAM, 200));
        }
    }
    // A shield bubble while it holds.
    if state.shield > 0.0 {
        let shimmer = 90 + ((seconds * 4.0).sin() * 30.0) as i32;
        let radius = (stats.radius / ART) as i32 + 3;
        ring(
            draw,
            x,
            y,
            radius,
            with_alpha(MINT, u8::try_from(shimmer.clamp(0, 255)).unwrap_or(90)),
        );
    }
    if stunned {
        for spark in 0..3 {
            let angle = seconds * 9.0 + spark as f32 * 2.1;
            let (sx, sy) = (
                angle.cos().mul_add(stats.radius, x),
                angle.sin().mul_add(stats.radius * 0.6, y - stats.radius),
            );
            disc(draw, sx, sy, 1, with_alpha(SKY_LIGHT, 220));
        }
    }
    if state.burning_until.get() > world_tick {
        for flame in 0..3 {
            let flicker = ((seconds * 14.0) as i32 + flame * 3) % 5;
            pen.rect(
                draw,
                art_width / 4 + flame * art_width / 4 - 1,
                -2 - flicker,
                2,
                3,
                with_alpha(if flame % 2 == 0 { ORANGE } else { AMBER }, 200),
            );
        }
    }
    // Health once it is hurt or hunting (never on a sleeping mimic).
    let hurt_at_all = state.health < state.max_health - 0.5;
    if state.mode != EnemyMode::Dormant && (hurt_at_all || state.mode.hostile() || stats.elite) {
        let bar = if stats.elite {
            24
        } else {
            (art_width - 2).max(8)
        };
        let bar_x = art_width / 2 - bar / 2;
        let bar_y = -3;
        pen.rect(draw, bar_x - 1, bar_y - 1, bar + 2, 3, INK);
        let filled = ((bar as f32) * state.health_fraction()).round() as i32;
        pen.rect(
            draw,
            bar_x,
            bar_y,
            filled,
            1,
            if stats.elite { GOLD } else { RED },
        );
        if state.shield > 0.0 && stats.shield > 0.0 {
            let max_shield = stats.shield * fatherload_simulation::enemy::tier_scale(state.tier);
            let shield =
                ((bar as f32) * (state.shield / max_shield).clamp(0.0, 1.0)).round() as i32;
            pen.rect(draw, bar_x, bar_y - 2, shield, 1, MINT);
        }
    }
}

/// Whether world pixel `(x, y)` is inside solid rock.
#[allow(clippy::cast_possible_truncation, reason = "world pixels map to tiles")]
fn in_rock(terrain: &Terrain, x: f32, y: f32) -> bool {
    terrain.is_solid_at(crate::view_frame::frame().tile_at(x, y))
}

/// A burrower under the rock: a shifting crack, trickling dust, and glinting eyes when it
/// gathers itself to lunge.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
fn draw_burrow_trace(draw: &mut impl RaylibDraw, state: &EnemyState, x: f32, y: f32, seconds: f32) {
    // A churning pocket in the rock: a dark hollow ringed by a ragged crack, rubble tumbling
    // through it, and the worm's eyes glinting when it hunts.
    let churn = (seconds * 10.0) as i32;
    disc(draw, x, y, 6, with_alpha(INK, 170));
    disc(draw, x, y, 4, with_alpha(SOIL_DARK, 200));
    let pen = Pen::at(x - 8.0 * ART, y - 6.0 * ART);
    for step in 0..16 {
        let wobble = (step * 7 + churn) % 3 - 1;
        let top = i32::from(step % 4 == 1);
        pen.px(draw, step, top + wobble.max(0), with_alpha(INK, 200));
        pen.px(draw, step, 11 - top - wobble.min(0), with_alpha(INK, 200));
    }
    for grit in 0..6 {
        let spin = (churn + grit * 5) % 12;
        let (gx, gy) = (3 + (grit * 3 + spin) % 10, 2 + (spin + grit * 2) % 8);
        pen.px(
            draw,
            gx,
            gy,
            with_alpha(if grit % 2 == 0 { SAND } else { CLAY }, 220),
        );
    }
    let hunting = matches!(
        state.mode,
        EnemyMode::Windup | EnemyMode::Chase | EnemyMode::Strike
    );
    if hunting && (seconds * 4.0) as i32 % 3 != 0 {
        let side = if state.facing < 0 { 4 } else { 9 };
        pen.rect(draw, side, 5, 1, 1, GOLD);
        pen.rect(draw, side + 2, 5, 1, 1, GOLD);
    }
}

/// A moth swarm: a dozen moths whirling round the swarm's centre, each flapping on its own
/// beat, a faint dust haze between them, and a pink flash when it is hurt.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
fn draw_moth_swarm(
    draw: &mut impl RaylibDraw,
    state: &EnemyState,
    x: f32,
    y: f32,
    seconds: f32,
    world_tick: u64,
) {
    let sprite = &art::NIGHT_MOTH;
    let hurt = state.hurt_until.get() > world_tick;
    let tint = if hurt { PINK } else { Color::WHITE };
    disc(draw, x, y, 9, with_alpha(MIST, 18));
    let base = (state.seed % 97) as f32 * 0.07;
    for moth in 0..12 {
        let m = moth as f32;
        let speed = 1.3 + (moth % 4) as f32 * 0.35;
        let angle =
            (seconds * speed).mul_add(if moth % 2 == 0 { 1.0 } else { -1.0 }, base + m * 0.52);
        let reach = 4.0 + (moth % 3) as f32 * 3.5 + ((seconds * 2.0 + m).sin() * 1.5);
        let (mx, my) = (
            angle.cos().mul_add(reach * ART, x),
            (angle * 1.3).sin().mul_add(reach * ART * 0.7, y),
        );
        let frame = sprite.frame_at(seconds + m * 0.031);
        let (w, h) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
        sprite::draw_sprite(
            draw,
            sprite,
            frame,
            Vector2::new(snap(mx - w * 0.5), snap(my - h * 0.5)),
            ART,
            tint,
        );
    }
}

/// Glints that cut through the night (drawn over the light map, WP-NIGHT): the eyes of hounds,
/// stalkers and the alpha shine in the dark, a wisp's core burns, and moth wings catch the
/// light. Only what the view could see.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "presentation math on small bounded values"
)]
pub(super) fn draw_night_glints(draw: &mut impl RaylibDraw, scene: &EnemyScene<'_>, darkness: f32) {
    if darkness < 0.15 {
        return;
    }
    let seconds = scene.seconds();
    let strength = ((darkness - 0.15) / 0.5).clamp(0.0, 1.0);
    for entity in scene.entities {
        let EntityState::Enemy(state) = &entity.state else {
            continue;
        };
        let (x, y) = entity.body.lerp_position(scene.alpha);
        if !scene.on_screen(x, y, TILE_SIZE * 2.0) || scene.hidden(x, y) {
            continue;
        }
        let stats = state.kind.stats();
        let sprite = enemy_sprite(state.kind);
        let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
        let left = x - width * 0.5;
        let grounded = matches!(
            stats.locomotion,
            Locomotion::Walking | Locomotion::Stationary
        );
        let top = if grounded {
            y + stats.radius - height + ART
        } else {
            y - height * 0.5
        };
        // A moonlit rim and the body lifted out of the dark (Wave 6): the rim first, then the
        // sprite again, dimly lit, over the darkness that swallowed it.
        if let Some(rim) = rim_color(state.kind) {
            let frame = enemy_frame(entity, state, seconds);
            let position = Vector2::new(snap(left), snap(top));
            let flip = faces(state.kind) && state.facing < 0;
            draw_rim(
                draw,
                sprite,
                frame,
                position,
                ART,
                flip,
                with_alpha(rim, (strength * 210.0) as u8),
            );
            sprite::draw_sprite_flipped(
                draw,
                sprite,
                frame,
                position,
                ART,
                flip,
                Color::new(150, 156, 196, (strength * 190.0) as u8),
            );
        }
        let eyes: Option<(i32, i32, Color)> = match state.kind {
            EnemyKind::ShadowHound => Some((13, 4, MINT)),
            EnemyKind::UmbralAlpha => Some((22, 3, RED)),
            EnemyKind::DuneStalker if !in_rock(scene.terrain, x, y) => Some((8, 4, RED)),
            _ => None,
        };
        if let Some((ex, ey, color)) = eyes {
            // Blink now and then.
            let blink = ((seconds + (state.seed % 13) as f32 * 0.37) % 4.0) < 0.12;
            if !blink {
                let flip = faces(state.kind) && state.facing < 0;
                let column = if flip { sprite.width() - 1 - ex } else { ex };
                let (gx, gy) = (left + column as f32 * ART, top + ey as f32 * ART);
                let glow = (strength * 90.0) as u8;
                disc(
                    draw,
                    gx + ART,
                    gy + ART * 0.5,
                    4,
                    with_alpha(color, glow / 2),
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(gx), snap(gy), ART * 2.0, ART),
                    with_alpha(color, (strength * 230.0) as u8),
                );
            }
        }
        match state.kind {
            EnemyKind::WillOWisp => {
                let pulse = ((seconds * 4.0 + (state.seed % 7) as f32).sin() * 1.5) as i32;
                disc(
                    draw,
                    x,
                    y,
                    6 + pulse,
                    with_alpha(MINT, (strength * 60.0) as u8),
                );
                disc(
                    draw,
                    x,
                    y + ART,
                    2,
                    with_alpha(BONE, (strength * 200.0) as u8),
                );
            }
            EnemyKind::MothSwarm => {
                // Pale wings catch the light: the swarm shows against the night.
                disc(draw, x, y, 12, with_alpha(MIST, (strength * 26.0) as u8));
                draw_moth_swarm(draw, state, x, y, seconds, scene.world_tick);
            }
            _ => {}
        }
    }
}

/// A one-pixel ring of radius `radius` art pixels.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "small pixel circle"
)]
fn ring(draw: &mut impl RaylibDraw, x: f32, y: f32, radius: i32, color: Color) {
    let (cx, cy) = (snap(x), snap(y));
    let steps = radius * 8;
    for step in 0..steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let px = (angle.cos() * radius as f32).round();
        let py = (angle.sin() * radius as f32).round();
        draw.draw_rectangle_rec(
            Rectangle::new(px.mul_add(ART, cx), py.mul_add(ART, cy), ART, ART),
            color,
        );
    }
}

#[allow(clippy::cast_precision_loss, reason = "sprite sizes are small")]
fn draw_shot(
    draw: &mut impl RaylibDraw,
    scene: &EnemyScene<'_>,
    shot: EnemyShotState,
    (x, y): (f32, f32),
    westward: bool,
) {
    let sprite = shot_sprite(shot.kind);
    let (width, height) = (sprite.width() as f32 * ART, sprite.height() as f32 * ART);
    let glow = match shot.kind {
        ShotKind::Acid => LIME,
        ShotKind::Fireball | ShotKind::Cannonball => ORANGE,
        ShotKind::Bolt | ShotKind::Wispfire => MINT,
        // Tomb shots (T28): a faint glint on an arrow, dust round rubble, a hex's violet halo.
        ShotKind::Arrow => STEEL_LIGHT,
        ShotKind::Rubble => SAND,
        ShotKind::Hex => VIOLET,
        ShotKind::CryptHex => LAVENDER,
        ShotKind::CryptArrow | ShotKind::Boulder => STONE_LIGHT,
        // The Living Dark (Wave 8): spores glow lime, thorns barely at all.
        ShotKind::Spore | ShotKind::MindSpore => LIME,
        ShotKind::Thorn => SAND,
    };
    disc(
        draw,
        x,
        y,
        if shot.kind == ShotKind::Hex { 7 } else { 4 },
        with_alpha(glow, 50),
    );
    // An arrow points the way it flies.
    sprite::draw_sprite_flipped(
        draw,
        sprite,
        sprite.frame_at(scene.seconds()),
        Vector2::new(snap(x - width * 0.5), snap(y - height * 0.5)),
        ART,
        shot.kind == ShotKind::Arrow && westward,
        Color::WHITE,
    );
}

/// The red hit flash on rigs inside their post-hit invulnerability window.
#[allow(clippy::cast_possible_truncation, reason = "a blink phase")]
pub(super) fn draw_rig_hits(
    draw: &mut impl RaylibDraw,
    players: &[crate::render_snapshot::PlayerPresentation],
    guards: &[(PlayerId, f32)],
    ticks: u64,
) {
    for (player_id, seconds) in guards {
        if *seconds <= 0.0 || (ticks / 4) % 2 == 1 {
            continue;
        }
        if let Some(player) = players.iter().find(|player| player.player_id == *player_id) {
            let strength = (seconds / 0.8).clamp(0.0, 1.0);
            let alpha = 80.0f32.mul_add(strength, 110.0) as u8;
            ring(draw, player.x, player.y, 11, with_alpha(RED, alpha));
            disc(draw, player.x, player.y, 9, with_alpha(RED, alpha / 3));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The painted columns `(first, last)` of a sprite across all its frames.
    fn painted_columns(sprite: &SpriteDef) -> (usize, usize) {
        let mut span = (usize::MAX, 0);
        for rows in sprite.frames {
            for row in *rows {
                for (column, symbol) in row.chars().enumerate() {
                    if symbol != '.' && symbol != ' ' {
                        span = (span.0.min(column), span.1.max(column));
                    }
                }
            }
        }
        span
    }

    /// Wave 6: the colossus' drawn body stays inside its contact box, so a pilot standing
    /// beside it (pushed to exactly touching) is never drawn inside its arms.
    #[test]
    #[allow(clippy::cast_precision_loss, reason = "small art sizes")]
    fn the_colossus_is_drawn_no_wider_than_its_contact_box() {
        let kind = EnemyKind::SunColossus;
        let sprite = enemy_sprite(kind);
        let scale = sprite_scale(kind);
        let (first, last) = painted_columns(sprite);
        let half = sprite.width() as f32 * scale * 0.5;
        let left = first as f32 * scale - half;
        let right = (last + 1) as f32 * scale - half;
        let (contact, _) = kind.contact_half();
        assert!(
            left >= -contact && right <= contact,
            "drawn {left}..{right} vs contact ±{contact}"
        );
    }
}
