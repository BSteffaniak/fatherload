//! Saves written by every wave's master build (Wave 13 QA): each wave's own game exported a few
//! of its staged scenes as save files (`--export-saves` on Waves 6 to 12's integrated masters:
//! `b8386fd`, `041890c`, `f76c418`, `eaf3ee8`, `f33d989`, `ef3c404`, `4ee4e8b`). Each one loads
//! in today's build, plays a minute with every rig moving, saves in today's format and reloads
//! to the same world, and both copies play on identically. Formats 1 to 15 are also covered by
//! `every_supported_save_format_loads_plays_and_round_trips` and the v1-v3 fixtures.

use fatherload_simulation::{ControlState, TickInputs, WorldState};

use super::{SAVE_VERSION, SaveFile, WorldSave, parse_save};

/// `(which build and scene, the file it wrote)`.
const WAVE_SAVES: &[(&str, &str)] = &[
    (
        "w6 night_town (format 12)",
        include_str!("fixtures/waves/w6-night_town.json"),
    ),
    (
        "w6 moon_colony (format 12)",
        include_str!("fixtures/waves/w6-moon_colony.json"),
    ),
    (
        "w7 night_town (format 13)",
        include_str!("fixtures/waves/w7-night_town.json"),
    ),
    (
        "w7 sky_city (format 13)",
        include_str!("fixtures/waves/w7-sky_city.json"),
    ),
    (
        "w8 night_town (format 14)",
        include_str!("fixtures/waves/w8-night_town.json"),
    ),
    (
        "w8 crypt_traps (format 14)",
        include_str!("fixtures/waves/w8-crypt_traps.json"),
    ),
    (
        "w9 night_town (format 14)",
        include_str!("fixtures/waves/w9-night_town.json"),
    ),
    (
        "w9 moon_colony (format 14)",
        include_str!("fixtures/waves/w9-moon_colony.json"),
    ),
    (
        "w10 night_town (format 14)",
        include_str!("fixtures/waves/w10-night_town.json"),
    ),
    (
        "w10 sky_city (format 14)",
        include_str!("fixtures/waves/w10-sky_city.json"),
    ),
    (
        "w11 night_town (format 15)",
        include_str!("fixtures/waves/w11-night_town.json"),
    ),
    (
        "w11 space_landed (format 15)",
        include_str!("fixtures/waves/w11-space_landed.json"),
    ),
    (
        "w11 space_split_places (format 15)",
        include_str!("fixtures/waves/w11-space_split_places.json"),
    ),
    (
        "w12 night_town (format 15)",
        include_str!("fixtures/waves/w12-night_town.json"),
    ),
    (
        "w12 space_landed (format 15)",
        include_str!("fixtures/waves/w12-space_landed.json"),
    ),
    (
        "w12 frontier_dunes_surface (format 15)",
        include_str!("fixtures/waves/w12-frontier_dunes_surface.json"),
    ),
    (
        "w12 wild_jungle_surface (format 15)",
        include_str!("fixtures/waves/w12-wild_jungle_surface.json"),
    ),
    (
        "w12 alien_vessi_town (format 15)",
        include_str!("fixtures/waves/w12-alien_vessi_town.json"),
    ),
];

/// Every rig drives, drills and lifts in turn (each its own way) for `ticks` ticks.
fn play(world: &mut WorldState, ticks: u32) {
    let players: Vec<_> = world.player_ids().collect();
    for tick in 0..ticks {
        let mut inputs = TickInputs::default();
        for (index, player) in players.iter().enumerate() {
            let phase = (tick / 45 + u32::try_from(index).unwrap_or(0)) % 4;
            let controls = match phase {
                0 => ControlState::new(1.0, false, false),
                1 => ControlState::new(0.0, false, true),
                2 => ControlState::new(-1.0, true, false),
                _ => ControlState::default(),
            };
            inputs = inputs.hold_as(*player, controls);
        }
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
    }
}

#[test]
fn saves_written_by_every_waves_master_build_load_play_and_round_trip() {
    assert!(WAVE_SAVES.len() >= 14, "every wave's saves are here");
    for (name, json) in WAVE_SAVES {
        let mut save = parse_save(json).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(save.world.player_ids().next().is_some(), "{name}: a rig");
        // Rigs away from home stay where they were (Wave 11 on).
        let away: Vec<_> = save
            .world
            .player_ids()
            .map(|player| (player, save.world.place_of(player).store()))
            .filter(|(_, store)| !store.is_home())
            .collect();
        let rate = save.world.tick_rate_hz();
        play(&mut save.world, rate * 60);
        // Saved in today's format and loaded again: the same world.
        let file = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            save: save.clone(),
        })
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut reloaded: WorldSave =
            parse_save(&file).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            serde_json::to_value(&reloaded.world).expect("json"),
            serde_json::to_value(&save.world).expect("json"),
            "{name}: reloaded as saved"
        );
        assert_eq!(
            reloaded.world.simulation_state_hash(),
            save.world.simulation_state_hash(),
            "{name}"
        );
        // Both copies play on identically.
        play(&mut save.world, rate * 10);
        play(&mut reloaded.world, rate * 10);
        assert_eq!(
            reloaded.world.simulation_state_hash(),
            save.world.simulation_state_hash(),
            "{name}: plays on identically"
        );
        // A rig that was away is still somewhere real: a place with ground.
        for (player, store) in away {
            let now = save.world.store_of(player);
            assert!(
                now.is_home() || save.world.store_terrain(now).is_some(),
                "{name}: {player:?} (was in {store:?}) stands on ground"
            );
        }
    }
}
