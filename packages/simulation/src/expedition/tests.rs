use std::collections::BTreeSet;

use super::{
    CHAPTERS, ECHOES, EchoKind, ExpeditionState, LOGS, Member,
    anomaly::{self, AnomalyKind},
    memories::{MEMORIES, Settlement, lines_for},
    sites::{self, site_tile},
};
use crate::{
    descent::{Descent, Stratum},
    field,
    position::Frame,
    terrain::{TileKind, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{landmarks, layout},
    },
};

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

const SEEDS: [u64; 4] = [0, 0x5EED, 0x00FA_7E45, 0x0DD_BA11];

#[test]
fn logs_follow_the_strata_in_depth_order_and_cover_descents_one_to_three() {
    let mut previous = 0;
    for log in &LOGS {
        let index = log.stratum.index();
        assert!(
            index >= previous,
            "{} is above the log before it",
            log.title
        );
        previous = index;
        assert!(log.descent() <= Descent::LivingDark);
        assert!(
            !log.text.is_empty() && log.text.len() < 340,
            "{}",
            log.title
        );
    }
    assert!(LOGS.windows(2).all(|pair| pair[0].day < pair[1].day));
    // Every stratum of Descents I-III holds at least one log.
    for descent in [Descent::Crust, Descent::OldDeep, Descent::LivingDark] {
        for stratum in descent.strata() {
            assert!(
                LOGS.iter().any(|log| log.stratum == stratum),
                "{stratum:?} has no log"
            );
        }
        assert!(CHAPTERS.iter().any(|chapter| chapter.descent == descent));
    }
    // Every member speaks.
    for member in Member::ALL {
        assert!(LOGS.iter().any(|log| log.author == member) || member == Member::Dov);
    }
    assert!(LOGS.iter().any(|log| log.author == Member::Dov));
}

#[test]
fn echoes_run_down_the_strata_radio_first_then_ghosts() {
    let order: Vec<(usize, u16)> = ECHOES
        .iter()
        .map(|echo| (echo.stratum.index(), echo.along))
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
    let first_ghost = ECHOES
        .iter()
        .position(|echo| echo.kind == EchoKind::Ghost)
        .expect("a ghost recording");
    assert!(
        ECHOES[..first_ghost]
            .iter()
            .all(|e| e.kind == EchoKind::Radio)
    );
    assert!(
        ECHOES[first_ghost..]
            .iter()
            .all(|e| e.kind == EchoKind::Ghost)
    );
}

#[test]
fn later_logs_lie_deeper_beside_the_cable() {
    for seed in SEEDS {
        let settings = v11(seed);
        let all = sites::sites(&settings);
        assert_eq!(all.len(), LOGS.len());
        let lift = landmarks::lift_column(&settings);
        let side = sites::trail_side(&settings);
        let layout = layout(&settings);
        for (index, site) in all.iter().enumerate() {
            assert_eq!(usize::from(site.log), index);
            assert_eq!(site.kind, LOGS[index].site);
            // In its stratum's layout rows, clear of the ragged boundaries.
            let (top, bottom) = layout.rows(site.stratum);
            assert!(site.bounds.min.y > top + 12 && site.bounds.max.y < bottom - 12);
            // Beside the cable, on the far side from the town.
            let near = if side > 0 {
                site.bounds.min.x - lift
            } else {
                lift - site.bounds.max.x
            };
            assert!(
                (24..=121).contains(&near),
                "seed {seed:#x} log {index}: {near}"
            );
        }
        for pair in all.windows(2) {
            assert!(
                pair[1].bounds.min.y > pair[0].bounds.max.y,
                "seed {seed:#x}: log {} is not below log {}",
                pair[1].log + 1,
                pair[0].log + 1
            );
        }
    }
}

#[test]
fn placement_is_deterministic_and_seeded() {
    let a = sites::sites(&v11(7));
    let b = (0..LOGS.len())
        .map(|log| {
            sites::site(&v11(7), &layout(&v11(7)), u8::try_from(log).expect("small")).expect("site")
        })
        .collect::<Vec<_>>();
    assert_eq!(a, b);
    assert_ne!(a, sites::sites(&v11(8)));
    // Every older world has no trail and no rooms.
    for generator in [GeneratorVersion::V5, GeneratorVersion::V10] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(7)
        };
        assert!(sites::sites(&settings).is_empty());
        let everywhere = TileRect {
            min: TilePosition { x: -400, y: 0 },
            max: TilePosition { x: 400, y: 900 },
        };
        assert!(anomaly::rooms_in(&settings, &everywhere).is_empty());
        assert!(field::stratum_profile_at(&settings, Frame::default(), 0.0, 9_000.0).is_none());
    }
}

#[test]
fn sites_carve_a_room_on_a_floor_and_keep_off_the_landmarks() {
    let settings = v11(0x5EED);
    let layout = layout(&settings);
    for site in sites::sites(&settings) {
        let floor = site_tile(&site, site.bounds.min.x + 2, site.floor_row());
        assert!(matches!(floor, Some(TileKind::Built(_))));
        let inside = site.interior().center();
        assert_eq!(site_tile(&site, inside.x, inside.y), Some(TileKind::Air));
        assert!(site.holds(site.canister()));
        assert!(
            landmarks::landmarks_with(&settings, &layout, &site.bounds).is_empty(),
            "log {} lands on a landmark",
            site.log + 1
        );
    }
}

/// The rooms of a strip of columns either side of the town, down through Descents I-III.
fn rooms_of(settings: &WorldSettings) -> Vec<anomaly::AnomalyRoom> {
    let layout = layout(settings);
    let area = TileRect {
        min: TilePosition {
            x: -1_200,
            y: layout.tops[0],
        },
        max: TilePosition {
            x: 1_200,
            y: layout.descent_rows(Descent::LivingDark).1,
        },
    };
    anomaly::rooms_in(settings, &area)
}

#[test]
fn anomaly_rooms_are_rare_varied_and_found_at_every_depth() {
    let settings = v11(0x5EED);
    let rooms = rooms_of(&settings);
    // 22 % of the regions: rare, but a few dozen in a wide strip.
    assert!(rooms.len() > 40, "{} rooms", rooms.len());
    let kinds: BTreeSet<AnomalyKind> = rooms.iter().map(|room| room.kind).collect();
    assert_eq!(kinds.len(), AnomalyKind::ALL.len());
    for descent in [Descent::Crust, Descent::OldDeep, Descent::LivingDark] {
        assert!(rooms.iter().any(|room| room.descent() == descent));
    }
    let layout = layout(&settings);
    let lift = landmarks::lift_column(&settings);
    for room in &rooms {
        assert!(room.bounds.min.y >= layout.tops[0] + anomaly::FIRST_ROWS);
        assert!(room.bounds.max.x < lift - 2 || room.bounds.min.x > lift + 2);
        assert!(landmarks::landmarks_with(&settings, &layout, &room.bounds).is_empty());
        assert!(sites::sites_in(&settings, &room.bounds).is_empty());
        // Lined walls, an open interior and its caches.
        let corner = anomaly::room_tile(room, room.bounds.min.x, room.bounds.min.y);
        assert!(matches!(
            corner,
            Some(TileKind::Cave(_) | TileKind::Built(_))
        ));
        for cache in room.caches().into_iter().flatten() {
            assert!(room.interior().contains(cache));
            assert!(matches!(
                anomaly::room_tile(room, cache.x, cache.y),
                Some(TileKind::Cave(_))
            ));
        }
        assert_eq!(
            anomaly::room_near(&settings, room.interior().center()),
            Some(*room)
        );
    }
}

#[test]
fn anomaly_room_fields_hold_inside_and_fade_out_past_the_walls() {
    let settings = v11(0x5EED);
    let rooms = rooms_of(&settings);
    let zones = field::FieldZones::default();
    for kind in AnomalyKind::ALL {
        let room = rooms
            .iter()
            .find(|room| room.kind == kind && room.descent() == Descent::Crust)
            .or_else(|| rooms.iter().find(|room| room.kind == kind))
            .expect("a room of every kind");
        let centre = room.interior().center();
        let frame = Frame::default();
        let (x, y) = frame.tile_center(centre);
        let inside = field::sample_in(&zones, &settings, frame, x, y);
        match kind {
            AnomalyKind::GravityFlip => {
                assert!(inside.gravity_y < -0.9, "{inside:?}");
                assert!(inside.rotation_unlimited());
            }
            AnomalyKind::PixelRot => {
                assert!(inside.screen[field::ScreenEffect::Pixelate.index()] > 0.5);
            }
            AnomalyKind::TimeFrozen => assert!(inside.time_scale < 0.2),
            AnomalyKind::Mirror => {
                assert!(inside.mirrored());
                assert!(inside.screen[field::ScreenEffect::Mirror.index()] > 0.9);
            }
        }
        // Six tiles outside the walls, the room's rule is gone.
        let out = TilePosition {
            x: room.bounds.max.x + 6,
            y: centre.y,
        };
        let (x, y) = frame.tile_center(out);
        let outside = field::sample_in(&zones, &settings, frame, x, y);
        let stratum = field::FieldProfile::NEUTRAL.sample();
        let base = crate::descent::fields::stratum_field_source(&settings, frame, x, y)
            .map_or(stratum, |profile| profile.sample());
        assert_eq!(outside, base, "{kind:?} leaks out of its room");
        // The same field in a frame one sector away.
        let far = Frame::floating_at(1, 1);
        let (fx, fy) = far.tile_center(centre);
        assert_eq!(field::sample_in(&zones, &settings, far, fx, fy), inside);
    }
}

#[test]
fn a_journal_reads_as_a_prefix_and_memories_open_with_the_trail() {
    let mut state = ExpeditionState::default();
    assert!(state.is_empty());
    assert_eq!(state.read_logs().count(), 0);
    assert_eq!(state.chapter_progress(Descent::Crust), (0, 10));
    let early = lines_for(Settlement::GravelHollow, &state).count();
    state.found = 10;
    assert!(state.chapter_read(Descent::Crust));
    assert!(!state.chapter_read(Descent::OldDeep));
    assert!(lines_for(Settlement::GravelHollow, &state).count() > early);
    assert!(
        !state.chapter_read(Descent::Heart),
        "no logs written there yet"
    );
    let ids: BTreeSet<&str> = MEMORIES.iter().map(|memory| memory.id).collect();
    assert_eq!(ids.len(), MEMORIES.len(), "memory ids are unique");
    for settlement in Settlement::ALL {
        assert!(
            MEMORIES
                .iter()
                .filter(|m| m.settlement == settlement)
                .count()
                >= 4
        );
    }
    for memory in &MEMORIES {
        assert!(memory.id.starts_with(match memory.settlement {
            Settlement::GravelHollow => "gravel_hollow.",
            Settlement::OldDeepCamp => "old_deep_camp.",
            Settlement::TheHive => "hive.",
        }));
        assert!(
            memory
                .after_log
                .is_none_or(|log| usize::from(log) <= LOGS.len())
        );
    }
    assert_eq!(Stratum::TheHive.descent(), Descent::LivingDark);
}

#[test]
fn story_stamps_keep_off_pilot_spaces_and_tomb_entrances() {
    use crate::worldgen::streamed::{pilot_spaces, tomb_sites};
    for seed in SEEDS {
        let settings = v11(seed);
        let mut stamps: Vec<(String, TileRect)> = rooms_of(&settings)
            .iter()
            .map(|room| (format!("{:?} room", room.kind), room.bounds))
            .collect();
        stamps.extend(
            sites::sites(&settings)
                .iter()
                .map(|site| (format!("log {} site", site.log + 1), site.bounds)),
        );
        for (name, bounds) in stamps {
            let spaces = pilot_spaces::spaces_touching(&settings, &bounds);
            assert!(
                spaces
                    .iter()
                    .all(|space| !space.bounds().intersects(&bounds)),
                "seed {seed:#x}: {name} at {bounds:?} over a pilot space"
            );
            for tomb in tomb_sites::sites_touching(&settings, &bounds) {
                assert!(
                    tomb_sites::entrance_cells(&tomb)
                        .iter()
                        .all(|(tile, _)| !bounds.expanded(1).contains(*tile)),
                    "seed {seed:#x}: {name} at {bounds:?} under a tomb entrance"
                );
            }
        }
    }
}

#[test]
fn every_anomaly_room_opens_through_a_door_and_its_caches_open_with_the_local_drill() {
    use crate::terrain::{is_indestructible, tile_hardness};
    use crate::worldgen::streamed::generated_tile;
    let settings = v11(0x00FA_7E45);
    let rooms = rooms_of(&settings);
    assert!(!rooms.is_empty());
    // A sample of rooms per kind (generation is slow in debug).
    for kind in AnomalyKind::ALL {
        for room in rooms.iter().filter(|room| room.kind == kind).take(3) {
            let strength = room.stratum.host_band().required_drill_strength();
            for (wall, beside) in [
                (room.bounds.min.x, room.bounds.min.x - 1),
                (room.bounds.max.x, room.bounds.max.x + 1),
            ] {
                for y in room.bounds.max.y - 3..room.bounds.max.y {
                    let door = TilePosition { x: wall, y };
                    assert!(room.is_door(door));
                    assert_eq!(
                        generated_tile(&settings, door).kind,
                        TileKind::Air,
                        "{kind:?} door at {door:?}"
                    );
                    // The rock beside the door is ordinary ground, never a sealed wall.
                    let outside = generated_tile(&settings, TilePosition { x: beside, y }).kind;
                    assert!(!is_indestructible(outside), "{kind:?}: {outside:?}");
                }
            }
            for cache in room.caches().into_iter().flatten() {
                let tile = generated_tile(&settings, cache).kind;
                assert!(
                    matches!(tile, TileKind::Cave(cave) if cave.is_cache()),
                    "{kind:?}: {tile:?}"
                );
                assert!(
                    tile_hardness(tile) <= strength,
                    "{kind:?} cache {tile:?} too hard for {:?}",
                    room.stratum
                );
            }
        }
    }
}
