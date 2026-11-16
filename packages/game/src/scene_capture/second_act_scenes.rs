//! The second act in one frame (wave 4 integration): a sky town on its island at dusk, the
//! Moon looming above, a wyvern circling, and the pilot gliding off the island's rim.

use fatherload_simulation::{
    EnemyKind, PlayerId, WorldState,
    clock::{MoonPhase, WorldClock},
    pilot::PilotGear,
    terrain::TILE_SIZE,
    worldgen::sky::{self, towns::TOWN_BLOCK_COLUMNS},
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::input::PlayerInput;

/// World time of day-slice `slice` (96 a day) on the first day whose Moon is `moon`.
fn time_of(world: &WorldState, slice: f32, moon: MoonPhase) -> u64 {
    let length = world.day_length();
    let seed = world.terrain().seed();
    let day = (0..8)
        .find(|day| {
            WorldClock::from_world_millis(day * length.millis(), length, seed).moon_phase() == moon
        })
        .unwrap_or(0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a slice of a day in milliseconds"
    )]
    let into_day = (f64::from(slice) * length.millis() as f64 / 96.0) as u64;
    day * length.millis() + into_day
}

#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn tile_centre(x: i32) -> f32 {
    (x as f32 + 0.5) * TILE_SIZE
}

/// The sky town nearest the column the Moon hangs over: `(first, last, ground row, id)`.
fn town_under_the_moon(
    world: &WorldState,
) -> Option<(i32, i32, i32, fatherload_simulation::village::VillageId)> {
    let settings = *world.world_settings();
    let moon_x = sky::moon(&settings).map_or(0, |moon| moon.center_x);
    let home = moon_x.div_euclid(TOWN_BLOCK_COLUMNS);
    (-6..=6)
        .flat_map(|dx| (0..12).map(move |by| (home + dx, by)))
        .filter_map(|(bx, by)| sky::sky_town_site(&settings, bx, by))
        .min_by_key(|site| (i32::midpoint(site.first, site.last) - moon_x).abs())
        .map(|site| (site.first, site.last, site.ground_row, site.id))
}

fn second_act(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player: PlayerId = seats[0].player;
        world.set_enemy_difficulty(fatherload_simulation::EnemyDifficulty::Normal);
        let Some((first, last, ground, town)) = town_under_the_moon(world) else {
            return;
        };
        if let Some(rig) = world.player_mut(player) {
            rig.fuel_tank_level = 10;
            rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(10);
            rig.fuel = rig.fuel_capacity * 0.6;
            rig.engine_level = 10;
            rig.hazard_gear.altitude_intake = 4;
            rig.hazard_gear.de_icer = 4;
            rig.villages.visited.insert(town);
        }
        // The rig parked at the town's east end; the pilot leaps off the rim beyond it.
        let edge = last + 3;
        place_rig(world, player, last - 4, ground - 1);
        reveal(
            world,
            player,
            first - 40..last + 80,
            ground - 60..ground + 40,
        );
        world.stream_terrain_near_players();
        world.set_world_time_millis(time_of(world, 76.75, MoonPhase::Full));
        if let Some(rig) = world.player_mut(player) {
            rig.pilot_gear = PilotGear {
                suit: 2,
                glider: 2,
                thermal: 2,
                ..PilotGear::default()
            };
        }
        #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
        let feet = (ground - 7) as f32 * TILE_SIZE;
        let _ = world.place_pilot(player, tile_centre(edge), feet);
        if let Some((pilot, _, _)) = world.pilot_of(player) {
            world.edit_pilot(pilot, |body, state| {
                state.grounded = false;
                body.velocity_x = 90.0;
                body.velocity_y = 40.0;
            });
        }
        #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
        let wyvern_y = (ground - 12) as f32 * TILE_SIZE;
        let _ = world.spawn_enemy(EnemyKind::Wyvern, tile_centre(edge + 7), wyvern_y);
    });
    stage.hold(
        20,
        PlayerInput {
            horizontal: 1.0,
            jump: true,
            ..PlayerInput::default()
        },
    );
    if !stage.game.shell.seats.is_empty() {
        stage.seat(0).village.banner = None;
        stage.seat(0).fanfare = None;
    }
}

pub const SECOND_ACT_SCENES: &[Scene] = &[Scene {
    name: "second_act",
    summary: "The second act: a sky town on its island at dusk under a full Moon looming above, a wyvern circling, the pilot gliding off the rim",
    setup: second_act,
}];
