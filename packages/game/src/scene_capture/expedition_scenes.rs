//! The Fathers' expedition scenes (Wave 8 story) in a Deep Descent world: a camp with its log
//! canister waiting, the card when a log is read, the expedition journal, a radio echo and a
//! ghost recording, and each kind of anomaly room with its rule on screen.
//!
//! The sites and rooms are the generator's own (the scene world is a real V11 world), and the
//! cards come from real events through the fixed tick, so the scenes also prove the story
//! pass, the journal rules and the announcements fit together.

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::ProgressionRules,
    descent::Stratum,
    expedition::{
        ECHOES, ExpeditionState,
        anomaly::{AnomalyKind, AnomalyRoom, rooms_in},
        sites::{self, ExpeditionSite},
    },
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, landmarks},
    },
};

use fatherload_simulation::people::{
    Role,
    deep_folk::{self, DeepSettlement},
};

use super::{Scene, SceneStage, carve, place_rig, reveal};
use crate::shell::ModalScreen;

/// Seed of every expedition scene world.
const SCENE_SEED: u64 = 0x0D3E_9DE5;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

pub const EXPEDITION_SCENES: &[Scene] = &[
    Scene {
        name: "residents_cathedral_station",
        summary: "Wave 9: Cathedral Station's residents in their hall; the medic tells what she remembers of the Fathers in conversation",
        setup: |stage| residents_scene(stage, DeepSettlement::CathedralStation, Some(Role::Healer)),
    },
    Scene {
        name: "residents_amber_court",
        summary: "Wave 9: the Amber Court's insect residents; the queen's voice speaks of the Lamp-Bearers' little sun",
        setup: |stage| residents_scene(stage, DeepSettlement::AmberCourt, Some(Role::ShrineKeeper)),
    },
    Scene {
        name: "residents_amber_court_hall",
        summary: "Wave 9: the Amber Court's residents about their comb hall: the elder, the pump tender, the nurse, the guard and the queen's voice",
        setup: |stage| residents_scene(stage, DeepSettlement::AmberCourt, None),
    },
    Scene {
        name: "expedition_camp",
        summary: "The Fathers' first camp beside the builders' cable: tent, lamp, sled, stove and the cipher canister waiting on its crate",
        setup: |stage| camp_scene(stage, 1, Shot::Waiting),
    },
    Scene {
        name: "expedition_log_read",
        summary: "Reading a log: the rig at the Grey Stone wreck, the canister open and the expedition log card",
        setup: |stage| camp_scene(stage, 6, Shot::Read),
    },
    Scene {
        name: "expedition_cairn",
        summary: "A cairn with its flag and chalk arrow in the Slate Shelves, the canister still sealed (the log before is unread)",
        setup: |stage| camp_scene(stage, 8, Shot::Sealed),
    },
    Scene {
        name: "expedition_raft_camp",
        summary: "Wave 9: the Fathers' camp on the Underground Ocean: frost on the canvas, the sled lashed on fuel drums, a paddle",
        setup: |stage| camp_scene(stage, 31, Shot::Waiting),
    },
    Scene {
        name: "expedition_sky_camp",
        summary: "Wave 9: the camp in the Inner Sky, on a floating stone of its own under the inner sun",
        setup: |stage| camp_scene(stage, 41, Shot::Waiting),
    },
    Scene {
        name: "expedition_jungle_camp",
        summary: "Wave 9: Bran's camp in the Megafauna Jungle: a leaf thatch, a hammock, a tusk and flowers, the log read on its card",
        setup: |stage| camp_scene(stage, 43, Shot::Read),
    },
    Scene {
        name: "expedition_dov_grave",
        summary: "Wave 9: Dov Halloran's grave in the Sunforge: the sunstone with his headset, flowers, the canister at its foot",
        setup: |stage| camp_scene(stage, 48, Shot::Waiting),
    },
    Scene {
        name: "expedition_thin_camp",
        summary: "Wave 9: the camp in the Mirror Strata, its mirror twin standing faintly beside it and its corners rotting to blocks",
        setup: |stage| camp_scene(stage, 51, Shot::Waiting),
    },
    Scene {
        name: "expedition_codex",
        summary: "Wave 9: the codex page: the hired hand's strongbox diary and the builders' tablets and room inscriptions",
        setup: codex_scene,
    },
    Scene {
        name: "expedition_builder_echo",
        summary: "Wave 9: the first builder echo in the Sunforge, Ines's translation on the card",
        setup: |stage| echo_scene(stage, 20),
    },
    Scene {
        name: "anomaly_sideways",
        summary: "Wave 9: a sideways room below Descent IV: gravity pulls to the right wall, rubble heaped on it",
        setup: |stage| anomaly_scene(stage, AnomalyKind::Sideways),
    },
    Scene {
        name: "anomaly_sunlit",
        summary: "Wave 9: a sunlit room: bloom and warm light from nowhere, a brass sun-disc in the wall",
        setup: |stage| anomaly_scene(stage, AnomalyKind::Sunlit),
    },
    Scene {
        name: "anomaly_weightless",
        summary: "Wave 9: a floating room: stones adrift, the caches under the ceiling",
        setup: |stage| anomaly_scene(stage, AnomalyKind::Weightless),
    },
    Scene {
        name: "anomaly_lensing",
        summary: "Wave 9: a bent room: the picture lensed round a dark knot, rings of bent light",
        setup: |stage| anomaly_scene(stage, AnomalyKind::Lensing),
    },
    Scene {
        name: "expedition_journal",
        summary: "The expedition journal: chapter I read with its revelation, chapter II under way, a log open under the list",
        setup: journal_scene,
    },
    Scene {
        name: "expedition_radio_echo",
        summary: "A radio echo in the Topsoil: Dov Halloran on the drop line, static across the card",
        setup: |stage| echo_scene(stage, 0),
    },
    Scene {
        name: "expedition_ghost_recording",
        summary: "A ghost recording in the Fungal Forest: Dov's tape, its reels turning on the card",
        setup: |stage| echo_scene(stage, 7),
    },
    Scene {
        name: "anomaly_gravity_flip",
        summary: "An upturned anomaly room: gravity pulls up, rubble on the ceiling, the cache up there with it",
        setup: |stage| anomaly_scene(stage, AnomalyKind::GravityFlip),
    },
    Scene {
        name: "anomaly_pixel_rot",
        summary: "A rotten anomaly room: the picture in blocks and few colours, crystal walls",
        setup: |stage| anomaly_scene(stage, AnomalyKind::PixelRot),
    },
    Scene {
        name: "anomaly_time_frozen",
        summary: "A stopped anomaly room: drops hung mid-fall, a stopped clock, the relic cache floating",
        setup: |stage| anomaly_scene(stage, AnomalyKind::TimeFrozen),
    },
    Scene {
        name: "anomaly_mirror",
        summary: "A mirror anomaly room: the screen and controls mirrored, glass walls and twin caches",
        setup: |stage| anomaly_scene(stage, AnomalyKind::Mirror),
    },
    // --- Wave 10 story: the finale. ----------------------------------------------------------
    Scene {
        name: "story_megacity_camp",
        summary: "Wave 10: the Fathers' camp on a builder street in the Buried Megacity, under a street lamp that came on for them",
        setup: |stage| camp_scene(stage, 61, Shot::Waiting),
    },
    Scene {
        name: "story_letter_home",
        summary: "Wave 10: Amos's letter home at the Seal Gate, read on its card",
        setup: |stage| camp_scene(stage, 71, Shot::Read),
    },
    Scene {
        name: "story_heart_camp",
        summary: "Wave 10: Amos alone at the Heart: the sled's name board and his rail crutch at the tent, the Star Drive's log on its card",
        setup: |stage| camp_scene(stage, 79, Shot::Read),
    },
    Scene {
        name: "story_last_log",
        summary: "Wave 10: the trail's end: Amos's chair of board and crutch by the cable root, Dov's recorder on it, the last log on its card",
        setup: |stage| camp_scene(stage, 80, Shot::Read),
    },
    Scene {
        name: "story_last_site",
        summary: "Wave 10: the trail's last site in the Heart without its card: the cable root's gold light over the empty chair",
        setup: |stage| camp_scene(stage, 80, Shot::Waiting),
    },
    Scene {
        name: "story_descent_card",
        summary: "Wave 10: the card when the crew first reaches a Descent (VII, the Ancients)",
        setup: descent_card_scene,
    },
    Scene {
        name: "story_descent_progress",
        summary: "Wave 10: the journal's Descent progress page: Descents reached, gates won, logs found per Descent",
        setup: progress_scene,
    },
    Scene {
        name: "story_ending",
        summary: "Wave 10: the ending: the cable lights from the Heart to the Moon, the page where Wren Varga looks up",
        setup: |stage| ending_scene(stage, 7),
    },
    Scene {
        name: "story_ending_last",
        summary: "Wave 10: the ending's last page: the dock wakes and the sky past the Moon is open",
        setup: |stage| ending_scene(stage, 8),
    },
];

/// A crew deep in Descent VI with a rig parked just inside Descent VII: its first tick there
/// raises the new Descent's card.
fn descent_card_scene(stage: &mut SceneStage<'_>) {
    let room = site(61);
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(60, u8::try_from(ECHOES.len()).unwrap_or(u8::MAX));
        journal.descents = 0b0011_1111;
        world.set_expedition(journal);
        let floor = room.floor_row();
        let x = room.bounds.min.x - 24;
        carve(world, x - 8..x + 8, floor - 3..floor);
        place_rig(world, player, x, floor - 1);
        reveal(world, player, x - 30..x + 30, floor - 16..floor + 14);
        world.stream_terrain_near_players();
    });
    stage.settle(30);
}

/// The progress page of a crew partway through the Descent: six Descents reached, five gates
/// won, the trail read to log 52.
fn progress_scene(stage: &mut SceneStage<'_>) {
    use fatherload_simulation::{descent::DescentBoss, expedition::progress::boss_bit};
    let room = site(52);
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(52, u8::try_from(ECHOES.len()).unwrap_or(u8::MAX));
        journal.descents = 0b0011_1111;
        journal.bosses = [
            DescentBoss::TheSinkhole,
            DescentBoss::StarCore,
            DescentBoss::MycelialMind,
            DescentBoss::Leviathan,
            DescentBoss::SunSerpent,
        ]
        .into_iter()
        .map(boss_bit)
        .fold(0, |bits, bit| bits | bit);
        world.set_expedition(journal);
        let floor = room.floor_row();
        let x = room.bounds.min.x - 20;
        carve(world, x - 6..x + 6, floor - 3..floor);
        place_rig(world, player, x, floor - 1);
        reveal(world, player, x - 30..x + 30, floor - 16..floor + 14);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::DescentProgress);
    // The Hollow World: its chapter's revelation pinned under the list.
    seat.selected_menu_item = 4;
    seat.message.clear();
    stage.settle(3);
}

/// The ending on page `page`, the crew at the Heart with the whole trail read and the cable lit.
fn ending_scene(stage: &mut SceneStage<'_>, page: usize) {
    let room = site(80);
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(80, u8::try_from(ECHOES.len()).unwrap_or(u8::MAX));
        journal.descents = 0xFF;
        journal.bosses = 0x00FF;
        journal.cable_lit = true;
        world.set_expedition(journal);
        let floor = room.floor_row();
        let x = room.bounds.min.x - 20;
        carve(world, x - 6..x + 6, floor - 3..floor);
        place_rig(world, player, x, floor - 1);
        reveal(world, player, x - 30..x + 30, floor - 16..floor + 14);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Ending);
    seat.selected_menu_item = page;
    seat.message.clear();
    // Long enough for the cable's light to climb to the page's station.
    stage.settle(150);
}

/// Swaps the staged world's terrain for the scene world, with its depth logistics and rules,
/// and fits the rig for the deep.
fn deep_world(world: &mut WorldState, player: PlayerId) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    // Below Descent I the crew holds the Deep Claim (the Sinkhole's gate).
    world.grant_deep_claim();
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 14;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(14);
        rig.fuel = rig.fuel_capacity * 0.7;
        rig.engine_level = 12;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.radiator_level = 15;
        rig.hull_level = 15;
        rig.hull = rig.max_hull();
    }
}

fn site(number: u8) -> ExpeditionSite {
    sites::sites(&settings())[usize::from(number - 1)]
}

/// The journal of a crew that has read logs `1..=found` and heard `echoes` echoes (every
/// Descent counts as reached, so no new Descent's card covers a scene; Wave 10).
fn read_up_to(found: u8, echoes: u8) -> ExpeditionState {
    ExpeditionState {
        found,
        echoes,
        descents: 0xFF,
        ..ExpeditionState::default()
    }
}

/// How a camp scene shows its canister.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shot {
    /// The rig in a tunnel at the wall, the canister waiting its turn.
    Waiting,
    /// The rig at the wall, the canister sealed (the log before is unread).
    Sealed,
    /// The rig inside: the canister opens through the tick and its card goes up.
    Read,
}

/// The rig at log `number`'s site, its canister as `shot` shows it.
fn camp_scene(stage: &mut SceneStage<'_>, number: u8, shot: Shot) {
    let room = site(number);
    let enter = shot == Shot::Read;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        // Read up to the log before (a sealed shot leaves a gap).
        let before = if shot == Shot::Sealed {
            number - 2
        } else {
            number - 1
        };
        let mut journal = read_up_to(before, 0);
        if shot == Shot::Sealed {
            journal.sealed.insert(number);
        }
        // No echo interrupts these shots.
        journal.echoes = u8::try_from(ECHOES.len()).unwrap_or(u8::MAX);
        world.set_expedition(journal);
        let inner = room.interior();
        let floor = room.floor_row();
        let side = sites::trail_side(&settings());
        // A tunnel up to the room from the town's side, two tiles tall on the room's floor.
        let (tunnel, rig_x) = if side > 0 {
            (inner.min.x - 9..inner.min.x, inner.min.x - 3)
        } else {
            (inner.max.x + 1..inner.max.x + 10, inner.max.x + 3)
        };
        carve(world, tunnel, floor - 2..floor);
        let x = if enter { room.canister().x - 2 } else { rig_x };
        place_rig(world, player, x, floor - 1);
        reveal(
            world,
            player,
            inner.min.x - 30..inner.max.x + 30,
            inner.min.y - 16..floor + 14,
        );
        world.stream_terrain_near_players();
    });
    // Past the stratum banner for a clear look at the site; a read log keeps its card up.
    stage.settle(if enter { 30 } else { 330 });
}

fn journal_scene(stage: &mut SceneStage<'_>) {
    let room = site(13);
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(13, 5);
        journal.anomaly_kinds = AnomalyKind::GravityFlip.bit() | AnomalyKind::Mirror.bit();
        journal.anomalies.insert((0, 0));
        journal.anomalies.insert((1, 4));
        world.set_expedition(journal);
        let floor = room.floor_row();
        let x = room.bounds.min.x - 20;
        carve(world, x - 6..x + 6, floor - 3..floor);
        place_rig(world, player, x, floor - 1);
        reveal(world, player, x - 30..x + 30, floor - 16..floor + 14);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::ExpeditionLog);
    // The last log of the Crust, its chapter's revelation just above it.
    seat.selected_menu_item = 9;
    stage.settle(3);
}

/// The rig dug in just below echo `index`'s depth, every echo before it heard.
fn echo_scene(stage: &mut SceneStage<'_>, index: usize) {
    let echo = &ECHOES[index];
    let layout = descent::layout(&settings());
    let row = fatherload_simulation::world::expedition::echo_row(&layout, echo) + 3;
    let column = site(1).bounds.min.x - 30;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(0, u8::try_from(index).unwrap_or(0));
        journal.found = if echo.stratum == Stratum::Topsoil {
            0
        } else {
            21
        };
        world.set_expedition(journal);
        carve(world, column - 12..column + 12, row - 2..row + 1);
        place_rig(world, player, column, row);
        reveal(world, player, column - 40..column + 40, row - 20..row + 16);
        world.stream_terrain_near_players();
    });
    stage.settle(40);
}

/// The first room of `kind` out from the scene column, through Descents I to VI.
fn room_of(kind: AnomalyKind) -> Option<AnomalyRoom> {
    let layout = descent::layout(&settings());
    let (_, bottom) = layout.descent_rows(fatherload_simulation::descent::Descent::StrangeLayers);
    let area = TileRect {
        min: TilePosition {
            x: -900,
            y: layout.tops[0],
        },
        max: TilePosition { x: 900, y: bottom },
    };
    let mut rooms: Vec<AnomalyRoom> = rooms_in(&settings(), &area)
        .into_iter()
        .filter(|room| room.kind == kind)
        .collect();
    rooms.sort_by_key(|room| (room.bounds.min.y, room.bounds.min.x.abs()));
    rooms.into_iter().next()
}

fn anomaly_scene(stage: &mut SceneStage<'_>, kind: AnomalyKind) {
    let Some(room) = room_of(kind) else {
        return;
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(0, u8::try_from(ECHOES.len()).unwrap_or(u8::MAX));
        journal.found = 0;
        world.set_expedition(journal);
        let inner = room.interior();
        let centre = inner.center();
        if kind == AnomalyKind::GravityFlip {
            // A shaft dug up to the floor hatch, the way a rig climbs in.
            let middle = i32::midpoint(room.bounds.min.x, room.bounds.max.x);
            carve(
                world,
                middle..middle + 2,
                room.bounds.max.y + 1..room.bounds.max.y + 8,
            );
        } else {
            // A tunnel to the left doorway, the rig just inside.
            carve(
                world,
                inner.min.x - 8..inner.min.x,
                inner.max.y - 1..inner.max.y + 1,
            );
        }
        let (column, row) = match kind {
            AnomalyKind::GravityFlip => (inner.min.x + 5, inner.min.y + 1),
            // Resting on the right wall, the room's floor.
            AnomalyKind::Sideways => (inner.max.x - 2, inner.min.y + 2),
            // Adrift in the middle.
            AnomalyKind::Weightless => (inner.min.x + 7, centre.y),
            // Clear of the caches (the mirror room's twins sit three in from each wall).
            _ => (inner.min.x + 5, inner.max.y),
        };
        place_rig(world, player, column, row);
        reveal(
            world,
            player,
            inner.min.x - 30..inner.max.x + 30,
            centre.y - 18..centre.y + 18,
        );
        world.stream_terrain_near_players();
    });
    // Past the room's card and the stratum banner: the room and its rule on their own.
    stage.settle(600);
}

/// The rig in a deep settlement's hall with the trail read deep into the Hollow World: its
/// residents are about, and the one working `role` tells a memory of the Fathers in
/// conversation (Wave 9).
fn residents_scene(stage: &mut SceneStage<'_>, place: DeepSettlement, role: Option<Role>) {
    let Some((ground, first, last)) = deep_folk::hall(&settings(), place) else {
        return;
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(49, 0);
        journal.echoes = u8::try_from(ECHOES.len()).unwrap_or(u8::MAX);
        world.set_expedition(journal);
        let middle = i32::midpoint(first, last);
        place_rig(world, player, middle, ground - 1);
        reveal(
            world,
            player,
            first - 40..last + 40,
            ground - 22..ground + 10,
        );
        world.stream_terrain_near_players();
    });
    // The population refreshes every half second; then let them settle into their day.
    stage.settle(150);
    stage.seat(0).village.banner = None;
    match role {
        Some(Role::Healer) => super::people_scenes::open_talk(stage, |c| c.role == Role::Healer),
        Some(_) => super::people_scenes::open_talk(stage, |c| c.role == Role::ShrineKeeper),
        None => {
            // Back from the residents so the hall and its people show whole.
            stage.play_on(|world| {
                let player = world.player_ids().next();
                if let Some(player) = player {
                    world.teleport_rig_to(
                        player,
                        fatherload_simulation::position::WorldPos::tile_center(
                            i64::from(first + 4),
                            i64::from(ground - 1),
                        ),
                    );
                }
            });
            stage.settle(4);
        }
    }
}

/// The codex page of a crew that has opened four strongboxes, read three tablets and the
/// Star Core's revelation, and entered two of the deeper rooms.
fn codex_scene(stage: &mut SceneStage<'_>) {
    let room = site(13);
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world, player);
        let mut journal = read_up_to(21, u8::try_from(ECHOES.len()).unwrap_or(u8::MAX));
        journal.anomaly_kinds = AnomalyKind::Sunlit.bit() | AnomalyKind::Sideways.bit();
        world.set_expedition(journal);
        let first = fatherload_simulation::old_deep::LOG_TABLET_FIRST;
        world.set_codex_finds_for_tests(
            &[0, 1, 2, 4],
            &[
                first,
                first + 1,
                first + 3,
                fatherload_simulation::old_deep::LOG_CORE_REVEAL,
            ],
        );
        let floor = room.floor_row();
        let x = room.bounds.min.x - 20;
        carve(world, x - 6..x + 6, floor - 3..floor);
        place_rig(world, player, x, floor - 1);
        reveal(world, player, x - 30..x + 30, floor - 16..floor + 14);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::ExpeditionCodex);
    // The Star Core's revelation, pinned under the list.
    seat.selected_menu_item = 7;
    seat.message.clear();
    stage.settle(3);
}
