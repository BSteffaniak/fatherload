//! Wave 9's tracks wired together in a Deep Descent world (generator V11): Descents IV-VI's
//! settlements remembering the Fathers by their real names, the story's sites kept off the new
//! strata's set pieces, every new section shared with clients, and a V11 world with all the new
//! state round-tripping through a save and a client's keyframe. (The Sonar Suite's one API has
//! its own tests in `sonar_tests`; the shops against the gates in `wave9_progression_tests`.)

use crate::{
    PlayerId, SimulationParams, SimulationTick, WorldState,
    expedition::{
        memories::{MEMORIES, Settlement},
        sites,
    },
    village::{self, placement::underground_id},
    world::SectionKey,
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// Descents IV-VI's settlements: their village sites and the memories they keep.
fn deep_settlements() -> [(i32, Settlement, &'static str); 3] {
    [
        (
            crate::drowned::MERROW_SITE,
            Settlement::MerrowCity,
            crate::drowned::MERROW_CITY_NAME,
        ),
        (
            crate::worldgen::streamed::villages::SUNFOLK_SITE,
            Settlement::JungleTribes,
            crate::hollow::SUNFOLK_NAME,
        ),
        (
            crate::distortion::PARALLAX_SITE,
            Settlement::PrismRelay,
            crate::distortion::PARALLAX_STATION_NAME,
        ),
    ]
}

#[test]
fn descents_four_to_six_settlements_remember_the_fathers_by_their_real_names() {
    for seed in [0, 5, 0x00DD_BA11] {
        let settings = v11(seed);
        let mut sites_seen = std::collections::BTreeSet::new();
        for (site, settlement, name) in deep_settlements() {
            assert!(sites_seen.insert(site), "{settlement:?} shares a site id");
            let found = village::village(&settings, underground_id(site))
                .unwrap_or_else(|| panic!("seed {seed:#x}: {name} is built"));
            assert_eq!(found.population.village_name, name);
            assert_eq!(
                Settlement::of_place_name(&found.population.village_name),
                Some(settlement),
                "seed {seed:#x}: {name} remembers the Fathers"
            );
            // Some of its generated people speak for a memory in their own voice.
            let speakers: Vec<&str> = found
                .population
                .characters
                .iter()
                .filter_map(|character| settlement.speaker_for_role(character.role))
                .collect();
            assert!(
                speakers.iter().any(|speaker| MEMORIES
                    .iter()
                    .any(|memory| memory.settlement == settlement
                        && memory.speaker_key() == *speaker)),
                "seed {seed:#x}: someone in {name} tells their own memory ({speakers:?})"
            );
        }
    }
}

#[test]
fn the_story_sites_keep_off_the_drowned_and_frozens_set_pieces() {
    let settings = v11(0);
    // Merrow City's bubble and the Leviathan's arena are set pieces the story keeps off.
    let city = crate::worldgen::descent::drowned::merrow_city(&settings).expect("Merrow City");
    assert!(sites::crosses_strata_set_pieces(&settings, &city.bounds()));
    let layout = crate::descent::fields::cached_layout(&settings);
    let arena = crate::worldgen::descent::drowned::leviathan_arena(&settings, &layout)
        .expect("the Leviathan's arena");
    assert!(sites::crosses_strata_set_pieces(&settings, &arena));
    // Every Descent IV-VI log lands in its own stratum.
    for site in sites::sites(&settings) {
        let (top, bottom) = layout.rows(site.stratum);
        assert!(
            site.bounds.min.y >= top && site.extent().max.y <= bottom,
            "log {} stays in {:?}",
            site.log + 1,
            site.stratum
        );
    }
}

#[test]
fn every_wave_nine_section_is_shared_with_clients() {
    for key in [
        SectionKey::Drowned,
        SectionKey::Hollow,
        SectionKey::Distortion,
        SectionKey::FarStrange,
        SectionKey::Logistics,
        SectionKey::Expedition,
    ] {
        assert!(SectionKey::SHARED.contains(&key), "{key:?}");
        assert!(key.visible_to(None), "{key:?} reaches spectators too");
    }
    // No section is listed twice.
    let unique: std::collections::BTreeSet<SectionKey> = SectionKey::SHARED.into_iter().collect();
    assert_eq!(unique.len(), SectionKey::SHARED.len());
}

/// A V11 world holding some of every Wave 9 section's state.
fn world_with_wave_nine_state() -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &v11(7), SimulationParams::default());
    let now = world.simulation_tick();
    let later = SimulationTick::new(now.get() + 600);
    world.drowned.pings.push(crate::drowned::SonarPing {
        player_id: PLAYER,
        x: -40,
        y: 4_100,
        tick: now,
        reach: 12,
    });
    world.drowned.inks.insert(3, later);
    world.drowned.lore = 0b101;
    world.hollow.revealed.insert(PLAYER, now);
    world.hollow.exposure.insert(PLAYER, 420);
    world.hollow.isle_landings = 2;
    world.distortion.twin_cooldowns.insert((-12, 7_800), later);
    world.distortion.found = 0b11;
    world.distortion.rot_tiers.insert(PLAYER, 3);
    world
        .far_strange
        .filters
        .insert(PLAYER, crate::far_strange::LampFilter::Blue);
    world.far_strange.pings.push(crate::far_strange::SonarPing {
        player: Some(PLAYER),
        x: 60,
        y: 9_000,
        tick: now,
        reach: 14,
    });
    world
        .far_strange
        .found
        .insert(crate::far_strange::FarLandmark::Choir);
    world.logistics.supplies.insert(PLAYER, vec![2, 0, 1]);
    world
}

#[test]
fn a_v11_world_with_every_wave_nine_state_round_trips_through_a_save_and_a_keyframe() {
    let world = world_with_wave_nine_state();
    assert!(!world.drowned().is_empty());
    assert!(!world.hollow().is_empty());
    assert!(!world.distortion().is_empty());
    assert!(!world.far_strange().is_empty());
    let sections = |world: &WorldState| {
        serde_json::json!({
            "drowned": world.drowned(),
            "hollow": world.hollow(),
            "distortion": world.distortion(),
            "far_strange": world.far_strange(),
            "supplies": world.logistics().supplies,
        })
    };
    // A save: JSON (tile-keyed maps travel as pairs) and the compact form.
    let json = serde_json::to_string(&world).expect("json");
    let from_json: WorldState = serde_json::from_str(&json).expect("json restores");
    assert_eq!(sections(&from_json), sections(&world));
    assert_eq!(
        from_json.simulation_state_hash(),
        world.simulation_state_hash()
    );
    let bytes = postcard::to_allocvec(&world).expect("postcard");
    let from_bytes: WorldState = postcard::from_bytes(&bytes).expect("postcard restores");
    assert_eq!(sections(&from_bytes), sections(&world));
    assert_eq!(
        from_bytes.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // A client's keyframe carries the shared sections whole.
    let viewer = Some(PlayerId::new(3));
    let keyframe = world.redacted_for(viewer);
    let decoded: WorldState =
        postcard::from_bytes(&postcard::to_allocvec(&keyframe).expect("encode")).expect("decode");
    assert_eq!(decoded.drowned().pings, world.drowned().pings);
    assert_eq!(
        serde_json::json!(decoded.hollow()),
        serde_json::json!(world.hollow())
    );
    assert_eq!(
        serde_json::json!(decoded.distortion()),
        serde_json::json!(world.distortion())
    );
    assert_eq!(
        serde_json::json!(decoded.far_strange()),
        serde_json::json!(world.far_strange())
    );
    // An older save without any Wave 9 field loads with every section empty.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("value");
    let object = value.as_object_mut().expect("a world object");
    for field in [
        "drowned",
        "hollow",
        "distortion",
        "far_strange",
        "far_schedule",
    ] {
        assert!(object.remove(field).is_some(), "{field} is saved");
    }
    let older: WorldState = serde_json::from_value(value).expect("an older save loads");
    assert!(older.drowned().is_empty());
    assert!(older.hollow().is_empty());
    assert!(older.distortion().is_empty());
    assert!(older.far_strange().is_empty());
}
