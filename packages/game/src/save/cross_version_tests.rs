//! Saves across versions (wave 6 QA): a world from every generator version, played by bots
//! through the evening into the night, written and read back by the game's own save code,
//! must continue exactly as the world that was never saved (and survive a second round trip).

use fatherload_simulation::{
    PlayerId, SimulationParams, TickInputs, WorldState,
    balance::{
        bot::{MinerBot, Strategy},
        economy::inputs_for,
    },
    clock::DayLength,
    enemy::EnemyDifficulty,
    terrain::TilePosition,
    tomb::SiteStyle,
    worldgen::{GeneratorVersion, TileRect, WorldSettings, streamed::tomb_sites},
};

use super::{WorldSave, load_world_from_path, save_world_to_path};

const FIRST: PlayerId = PlayerId::new(0);
const SECOND: PlayerId = PlayerId::new(1);
/// A low tick rate keeps a debug run short while the bots still get a minute of play.
const HZ: u32 = 20;

/// 18:15 on day 1 of the short (24-minute) day, in world milliseconds: dusk. A minute of play
/// later the save is written at 19:15; the 40 s after loading cross into the night (19:30).
const DUSK_MILLIS: u64 = (18 * 60 + 15) * 1_000;

/// A two-rig world on `generator`, Normal enemies, the short day, at dusk.
fn world_on(generator: GeneratorVersion) -> WorldState {
    let settings = WorldSettings {
        generator,
        ..WorldSettings::new(0x0057_A1E5)
    };
    let mut world = WorldState::new_game_with_settings(FIRST, &settings, SimulationParams::new(HZ));
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    world.set_day_length(DayLength::Short);
    world.add_player(SECOND);
    world.set_world_time_millis(DUSK_MILLIS);
    world
}

struct Crew {
    bots: [MinerBot; 2],
}

impl Crew {
    fn new() -> Self {
        Self {
            bots: [
                MinerBot::new(FIRST, Strategy::Prospector),
                MinerBot::new(SECOND, Strategy::Charges),
            ],
        }
    }

    /// One tick of bot play; returns the inputs it fed, so another world can replay them.
    fn play(&mut self, world: &mut WorldState) -> TickInputs {
        let mut inputs = TickInputs::default();
        for (bot, player) in self.bots.iter_mut().zip([FIRST, SECOND]) {
            if world.tomb_run_of(player).is_some() {
                // Inside a tomb: walk east and use whatever is in reach.
                inputs.players.extend(
                    inputs_for(
                        player,
                        fatherload_simulation::ControlState::new(1.0, false, false),
                        Vec::new(),
                    )
                    .players,
                );
                continue;
            }
            let (controls, actions) = bot.decide(world);
            inputs
                .players
                .extend(inputs_for(player, controls, actions).players);
        }
        let output = world.simulate_tick(world.simulation_tick(), &inputs);
        for bot in &mut self.bots {
            bot.observe(world, &output);
        }
        inputs
    }
}

fn round_trip(world: &WorldState, name: &str) -> WorldState {
    let path = std::env::temp_dir().join(format!(
        "fatherload-cross-version-{name}-{}.json",
        std::process::id()
    ));
    save_world_to_path(
        &WorldSave {
            world: world.clone(),
            play_seconds: 60.0,
        },
        &path,
    )
    .expect("the world saves");
    let loaded = load_world_from_path(&path).expect("the save loads");
    let _ = std::fs::remove_file(&path);
    loaded.world
}

fn as_json(world: &WorldState) -> serde_json::Value {
    serde_json::to_value(world).expect("world serializes")
}

/// Plays `generator`'s world a minute, saves and reloads it, replays the same inputs on both
/// for another 40 seconds (into the night), and checks they stay identical, then round-trips
/// once more.
fn check(generator: GeneratorVersion, tomb: bool) {
    let name = format!("{generator:?}");
    let mut world = world_on(generator);
    let mut crew = Crew::new();
    for _ in 0..HZ * 60 {
        let _ = crew.play(&mut world);
    }
    if tomb {
        // The second rig's pilot goes down into a crypt with the first one's pilot: the save
        // is written mid-floor.
        let rect = TileRect {
            min: TilePosition { x: -6_000, y: -100 },
            max: TilePosition { x: 6_000, y: 900 },
        };
        let site = tomb_sites::sites_touching(world.world_settings(), &rect)
            .into_iter()
            .find(|site| site.style == SiteStyle::TopDown)
            .expect("a crypt near town");
        world
            .scenario_enter_tomb(&site, &[SECOND])
            .expect("the run starts");
        for _ in 0..HZ * 3 {
            let _ = crew.play(&mut world);
        }
        assert!(
            world.tomb_run_of(SECOND).is_some(),
            "{name}: still in the run"
        );
    }
    let mut loaded = round_trip(&world, &name);
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash(),
        "{name}: the loaded world hashes like the saved one"
    );
    assert_eq!(as_json(&loaded), as_json(&world), "{name}: loaded as saved");
    let was_night = world.is_night();
    for second in 0..40 {
        for _ in 0..HZ {
            let inputs = crew.play(&mut world);
            let _ = loaded.simulate_tick(loaded.simulation_tick(), &inputs);
        }
        assert_eq!(
            loaded.simulation_state_hash(),
            world.simulation_state_hash(),
            "{name}: the loaded world diverged {second} s after loading"
        );
    }
    assert_eq!(
        as_json(&loaded),
        as_json(&world),
        "{name}: same world after play"
    );
    assert!(
        !was_night && world.is_night(),
        "{name}: loaded at dusk, played into the night"
    );
    for player in [FIRST, SECOND] {
        let rig = world.player(player).expect("rig");
        assert!(rig.fuel.is_finite() && rig.fuel >= 0.0 && rig.fuel <= rig.fuel_capacity);
        assert!(rig.hull.is_finite());
        assert!(rig.cargo_used() <= rig.cargo_capacity);
    }
    let again = round_trip(&loaded, &name);
    assert_eq!(
        as_json(&again),
        as_json(&loaded),
        "{name}: second round trip"
    );
}

#[test]
fn every_generator_version_saves_mid_play_and_continues_identically() {
    for generator in GeneratorVersion::ALL {
        check(generator, generator == GeneratorVersion::V10);
    }
}
