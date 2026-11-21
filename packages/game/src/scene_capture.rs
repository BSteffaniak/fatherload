//! `fatherload --screenshot-scenes <dir> [--size WxH] [--only a,b] [--list] [--frame-timing N]`:
//! renders scripted game states to PNG files without user input, then exits. With
//! `--frame-timing N` each scene then draws N more frames and prints the CPU time of the frame
//! work (snapshot, renderer sync and draw calls, not the buffer swap): a quick way to find
//! renderer hotspots (lighting, sky chunks, night, tombs) before measuring the browser build.
//! With `--export-saves` no window opens: each selected scene's staged world is written as a
//! save file `<dir>/<scene name>.json` instead (the browser frame-timing script,
//! `scripts/perf-web.cjs`, loads them to time the WASM build in those states).
//!
//! Each [`Scene`] builds a deterministic state through the same [`LocalGame`] the app runs (fixed
//! frame time, fixed world seed), renders it with the real [`GameRenderer`] for a few frames and
//! saves `<dir>/<scene name>.png`. The window opens briefly while capturing.
//!
//! Adding a scene: write a `fn(&mut SceneStage)` that arranges the state and add a [`Scene`] to
//! [`SCENES`]. `SceneStage` starts on the title screen; `play` enters a staged world, `hold` runs
//! frames with held input, and `shell` exposes the menu state to open screens directly.

mod clock_scenes;
mod crew_scenes;
mod enemy_scenes;
mod moon_scenes;
mod night_scenes;
mod reward_scenes;
mod rig_scenes;
mod sky_isle_scenes;
mod sky_scenes;
mod surface_scenes;
mod village_scenes;

use std::path::{Path, PathBuf};

use fatherload_netcode::{LobbyEntry, PlayerId as NetPlayerId};
use fatherload_simulation::{
    Discovery, PlayerId, SectionKey, SectionPayload, SimulationTick, WorldState,
    economy::SurfaceZone,
    poi::{FormationStyle, LoreId, PoiKnowledge, PoiTile, PoiVariant, VaultPuzzle},
    quest::{BlueprintId, QuestGiver, QuestId},
    surface::SURFACE_BUILDINGS,
    terrain::{DeepStratum, MineralKind, TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        Band, DEFAULT_WORLD_SEED, FeatureDetail, FeatureKind, HazardKind, ResourceRichness,
        WorldFeature, WorldSettings, WorldSize,
    },
};
use raylib::ffi;

use crate::{
    input::{
        PlayerInput,
        devices::{InputDevice, KeyboardLayout, PadFamily},
    },
    local_game::{FrameInput, FrameReport, LocalGame, SaveTarget},
    rendering::GameRenderer,
    runtime::{LobbyInfo, LocalSeat, OnlineRole},
    save::SettingsFile,
    shell::{
        FanfareKind, ModalScreen, OnlineScreen, RunMode, SeatUi, SetupPurpose, ShellState,
        interior_service_x,
    },
};

/// Fixed frame time for every scripted frame, so captures never depend on machine speed.
const FRAME_SECONDS: f32 = 1.0 / 60.0;
/// Frames drawn per scene before the capture (lets terrain textures and UI state settle).
const RENDER_FRAMES: usize = 3;
/// Default capture size in logical screen points.
const DEFAULT_SIZE: (i32, i32) = (1280, 720);

/// What `--screenshot-scenes` was asked to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureOptions {
    pub dir: PathBuf,
    pub size: (i32, i32),
    /// Only these scenes (all when `None`).
    pub only: Option<Vec<String>>,
    /// Print the scene names and exit.
    pub list: bool,
    /// Frames to time after each capture (`--frame-timing N`; none when 0).
    pub timing_frames: usize,
    /// Write each scene's world as a save file instead of capturing it (`--export-saves`).
    pub export_saves: bool,
}

/// Parses the scene-capture flags. `Ok(None)` when `--screenshot-scenes` is absent.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<CaptureOptions>, String> {
    let Some(position) = args.iter().position(|arg| arg == "--screenshot-scenes") else {
        return Ok(None);
    };
    let dir = args
        .get(position + 1)
        .filter(|value| !value.starts_with("--"))
        .ok_or("--screenshot-scenes needs an output directory")?;
    let mut options = CaptureOptions {
        dir: PathBuf::from(dir),
        size: DEFAULT_SIZE,
        only: None,
        list: false,
        timing_frames: 0,
        export_saves: false,
    };
    let mut iter = args
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != position && *index != position + 1);
    while let Some((_, arg)) = iter.next() {
        match arg.as_str() {
            "--size" => {
                let (_, value) = iter.next().ok_or("--size needs WxH, e.g. 1920x1080")?;
                options.size = parse_size(value)?;
            }
            "--only" => {
                let (_, value) = iter
                    .next()
                    .ok_or("--only needs scene names, e.g. title,hud")?;
                options.only = Some(value.split(',').map(str::to_owned).collect());
            }
            "--list" => options.list = true,
            "--export-saves" => options.export_saves = true,
            "--frame-timing" => {
                let (_, value) = iter.next().ok_or("--frame-timing needs a frame count")?;
                options.timing_frames = value
                    .parse()
                    .ok()
                    .filter(|frames| (1..=100_000).contains(frames))
                    .ok_or_else(|| format!("invalid --frame-timing {value}"))?;
            }
            other => return Err(format!("unknown scene-capture argument {other}")),
        }
    }
    if let Some(only) = &options.only
        && let Some(unknown) = only
            .iter()
            .find(|name| all_scenes().all(|scene| scene.name != name.as_str()))
    {
        return Err(format!("unknown scene {unknown} (see --list)"));
    }
    Ok(Some(options))
}

fn parse_size(value: &str) -> Result<(i32, i32), String> {
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("invalid --size {value}, expected WxH"))?;
    let parse = |text: &str| {
        text.parse::<i32>()
            .ok()
            .filter(|size| (320..=7680).contains(size))
            .ok_or_else(|| format!("invalid --size {value}"))
    };
    Ok((parse(width)?, parse(height)?))
}

/// Opens a window, captures every selected scene and exits.
///
/// # Errors
///
/// Returns a message if the output directory cannot be created or a capture is not written.
#[allow(
    clippy::too_many_lines,
    reason = "one capture loop: stage, draw, save, optionally time"
)]
pub fn run(options: &CaptureOptions) -> Result<(), String> {
    let scenes: Vec<&Scene> = all_scenes()
        .filter(|scene| {
            options
                .only
                .as_ref()
                .is_none_or(|only| only.iter().any(|name| name == scene.name))
        })
        .collect();
    if options.list {
        for scene in &scenes {
            println!("{:<22} {}", scene.name, scene.summary);
        }
        return Ok(());
    }
    std::fs::create_dir_all(&options.dir)
        .map_err(|error| format!("cannot create {}: {error}", options.dir.display()))?;
    if options.export_saves {
        return export_saves(&options.dir, &scenes);
    }
    let (width, height) = options.size;
    let (mut raylib, thread) = raylib::init()
        .size(width, height)
        .highdpi()
        .title("Fatherload scene capture")
        .build();
    raylib.set_exit_key(None);
    let scratch_save = options.dir.join(".scene-capture-save.json");
    let mut renderer: Option<GameRenderer> = None;
    for scene in scenes {
        let mut game = LocalGame::new(
            SettingsFile::default(),
            SaveTarget::File(scratch_save.clone()),
        );
        let mut stage = SceneStage {
            game: &mut game,
            report: FrameReport {
                world_replaced: true,
                ..FrameReport::default()
            },
            gallery: false,
            ui_sheet: false,
            prompt_sheet: false,
            rig_sheet: None,
            touch_view: crate::input::touch::TouchView::default(),
        };
        (scene.setup)(&mut stage);
        let (report, gallery, ui_sheet, prompt_sheet, rig_sheet, touch_view) = (
            stage.report,
            stage.gallery,
            stage.ui_sheet,
            stage.prompt_sheet,
            stage.rig_sheet,
            stage.touch_view,
        );
        let renderer = renderer
            .get_or_insert_with(|| GameRenderer::new(&mut raylib, &thread, game.world().terrain()));
        // Each scene starts with fresh dialog focus and scroll, whatever the last one showed.
        renderer.reset_ui_state();
        let path = options.dir.join(format!("{}.png", scene.name));
        for frame in 0..RENDER_FRAMES {
            let screen = crate::window::DesktopWindow::screen(&raylib);
            let snapshot = game.snapshot(screen);
            let frame_report = if frame == 0 {
                report.clone()
            } else {
                FrameReport::default()
            };
            crate::touch_screen::publish_staged(
                &game.shell,
                game.world(),
                snapshot.views.first(),
                screen,
                &touch_view,
            );
            renderer.sync(&mut raylib, &thread, &snapshot, &frame_report);
            let mut draw = raylib.begin_drawing(&thread);
            if gallery {
                renderer.render_art_gallery(&mut draw);
            } else if ui_sheet {
                renderer.render_ui_sheet(&mut draw);
            } else if prompt_sheet {
                renderer.render_prompt_sheet(&mut draw);
            } else if let Some(sheet) = rig_sheet {
                renderer.render_rig_sheet(&mut draw, sheet);
            } else {
                renderer.render(&mut draw, &game.shell, &snapshot);
            }
            if frame + 1 == RENDER_FRAMES {
                save_framebuffer(&draw, &thread, &path)?;
            }
        }
        println!("captured {}", path.display());
        if options.timing_frames > 0
            && !(gallery || ui_sheet || prompt_sheet || rig_sheet.is_some())
        {
            time_frames(
                (&mut raylib, &thread),
                renderer,
                &game,
                scene.name,
                options.timing_frames,
            );
        }
    }
    let _ = std::fs::remove_file(&scratch_save);
    Ok(())
}

/// Draws `frames` more frames of a captured scene and prints the CPU time of the frame work
/// (snapshot, renderer sync and draw calls; the buffer swap is left out).
fn time_frames(
    (raylib, thread): (&mut raylib::RaylibHandle, &raylib::RaylibThread),
    renderer: &mut GameRenderer,
    game: &LocalGame,
    name: &str,
    frames: usize,
) {
    let mut times = Vec::with_capacity(frames);
    for _ in 0..frames {
        let started = std::time::Instant::now();
        let screen = crate::window::DesktopWindow::screen(raylib);
        let snapshot = game.snapshot(screen);
        renderer.sync(raylib, thread, &snapshot, &FrameReport::default());
        let mut draw = raylib.begin_drawing(thread);
        renderer.render(&mut draw, &game.shell, &snapshot);
        times.push(started.elapsed().as_secs_f64() * 1_000.0);
        drop(draw);
    }
    times.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frame counts and percentiles"
    )]
    let at = |fraction: f64| times[((times.len() - 1) as f64 * fraction).round() as usize];
    #[allow(clippy::cast_precision_loss, reason = "frame counts")]
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "timing {name:<28} mean {mean:6.2} ms  median {:6.2}  p95 {:6.2}  max {:6.2}",
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

/// Stages every scene and writes its world as a save file (no window).
fn export_saves(dir: &Path, scenes: &[&Scene]) -> Result<(), String> {
    let scratch_save = dir.join(".scene-capture-save.json");
    for scene in scenes {
        let mut game = LocalGame::new(
            SettingsFile::default(),
            SaveTarget::File(scratch_save.clone()),
        );
        let mut stage = SceneStage {
            game: &mut game,
            report: FrameReport::default(),
            gallery: false,
            ui_sheet: false,
            prompt_sheet: false,
            rig_sheet: None,
            touch_view: crate::input::touch::TouchView::default(),
        };
        (scene.setup)(&mut stage);
        let path = dir.join(format!("{}.json", scene.name));
        crate::save::save_world_to_path(
            &crate::save::WorldSave {
                world: game.world().clone(),
                play_seconds: 60.0,
            },
            &path,
        )
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        println!("exported {}", path.display());
    }
    let _ = std::fs::remove_file(&scratch_save);
    Ok(())
}

/// Writes the frame drawn so far (before the buffer swap) to `path` as a PNG.
fn save_framebuffer(
    draw: &raylib::prelude::RaylibDrawHandle<'_>,
    thread: &raylib::RaylibThread,
    path: &Path,
) -> Result<(), String> {
    // SAFETY: flushes raylib's pending draw batch so the framebuffer holds the whole frame.
    unsafe { ffi::rlDrawRenderBatchActive() };
    let image = draw.load_image_from_screen(thread);
    let text = path.to_str().ok_or("capture path is not UTF-8")?;
    image.export_image(text);
    if path.exists() {
        Ok(())
    } else {
        Err(format!("failed to write {}", path.display()))
    }
}

mod base_scenes;
mod cave_scenes;
mod crypt_scenes;
mod drop_scenes;
mod explosive_scenes;
mod home_scenes;
mod people_scenes;
mod pilot_content_scenes;
mod pilot_scenes;
mod second_act_scenes;
mod side_tomb_scenes;
mod sky_continent_scenes;
mod story_scenes;
mod tomb_scenes;
mod touch_scenes;
mod weapon_scenes;

/// One capturable scene.
pub struct Scene {
    /// File name (without `.png`) and `--only` key.
    pub name: &'static str,
    /// What the scene shows.
    pub summary: &'static str,
    /// Builds the scene's state.
    pub setup: fn(&mut SceneStage<'_>),
}

/// The game a scene is being arranged in.
pub struct SceneStage<'g> {
    game: &'g mut LocalGame,
    report: FrameReport,
    /// Draw the art reference sheet instead of the game.
    gallery: bool,
    /// Draw the UI icon and interior prop sheet instead of the game.
    ui_sheet: bool,
    /// Draw the button-prompt sheet instead of the game.
    prompt_sheet: bool,
    /// Draw a rig reference sheet instead of the game.
    rig_sheet: Option<crate::rendering::rig_art::sheet::RigSheet>,
    /// Fingers on the touch controls (drawn when the first seat's device is the touch screen).
    touch_view: crate::input::touch::TouchView,
}

impl SceneStage<'_> {
    /// Enters a fresh local world with `seats` seats arranged by `arrange`.
    fn play(&mut self, seats: usize, arrange: impl FnOnce(&mut WorldState, &[LocalSeat])) {
        let report = self.game.stage_local_world(seats, arrange);
        self.absorb(report);
    }

    /// Runs `frames` frames with the primary seat holding `input` (other seats idle).
    fn hold(&mut self, frames: usize, input: PlayerInput) {
        for _ in 0..frames {
            let report = self.game.frame(&FrameInput::primary(input), FRAME_SECONDS);
            self.absorb(report);
        }
    }

    /// Runs `frames` frames with every seat holding its input in `inputs` (seat order).
    fn hold_seats(&mut self, frames: usize, inputs: &[PlayerInput]) {
        let frame = FrameInput {
            primary: inputs.first().copied().unwrap_or_default(),
            other_seats: inputs.iter().skip(1).copied().collect(),
            ..FrameInput::default()
        };
        for _ in 0..frames {
            let report = self.game.frame(&frame, FRAME_SECONDS);
            self.absorb(report);
        }
    }

    /// Runs `frames` idle frames (the world ticks while playing).
    fn settle(&mut self, frames: usize) {
        self.hold(frames, PlayerInput::default());
    }

    const fn shell(&mut self) -> &mut ShellState {
        &mut self.game.shell
    }

    /// The world being shown.
    fn world(&self) -> &WorldState {
        self.game.world()
    }

    /// Edits the running world in place (after it has run a while).
    fn play_on(&mut self, edit: impl FnOnce(&mut WorldState)) {
        self.game.edit_world(edit);
    }

    /// Seat `seat`'s own menus (interior, counter panel, dialogs, messages).
    fn seat(&mut self, seat: usize) -> &mut SeatUi {
        self.game.shell.seat_mut(seat)
    }

    /// Shows every seat's prompts for `devices` (in seat order).
    fn use_devices(&mut self, devices: &[InputDevice]) {
        self.game.shell.seat_devices = devices.to_vec();
    }

    fn absorb(&mut self, report: FrameReport) {
        self.report.world_replaced |= report.world_replaced;
        self.report.changed_tiles.extend(report.changed_tiles);
    }
}

/// Every scene: the main list plus the rig customization scenes.
fn all_scenes() -> impl Iterator<Item = &'static Scene> {
    SCENES
        .iter()
        .chain(rig_scenes::RIG_SCENES)
        .chain(village_scenes::VILLAGE_SCENES)
        .chain(people_scenes::PEOPLE_SCENES)
        .chain(enemy_scenes::ENEMY_SCENES)
        .chain(base_scenes::BASE_SCENES)
        .chain(crew_scenes::CREW_SCENES)
        .chain(surface_scenes::SURFACE_SCENES)
        .chain(touch_scenes::TOUCH_SCENES)
        .chain(clock_scenes::CLOCK_SCENES)
        .chain(sky_scenes::SKY_SCENES)
        .chain(sky_isle_scenes::SKY_ISLE_SCENES)
        .chain(sky_continent_scenes::SKY_CONTINENT_SCENES)
        .chain(moon_scenes::MOON_SCENES)
        .chain(pilot_scenes::PILOT_SCENES)
        .chain(pilot_content_scenes::PILOT_CONTENT_SCENES)
        .chain(tomb_scenes::TOMB_SCENES)
        .chain(side_tomb_scenes::SIDE_TOMB_SCENES)
        .chain(crypt_scenes::CRYPT_SCENES)
        .chain(reward_scenes::REWARD_SCENES)
        .chain(home_scenes::HOME_SCENES)
        .chain(night_scenes::NIGHT_SCENES)
        .chain(story_scenes::STORY_SCENES)
        .chain(second_act_scenes::SECOND_ACT_SCENES)
}

/// Same physical bomb scene at successive fixed ticks, for repeatable motion review.
fn bomb_cascade_scene(stage: &mut SceneStage<'_>, frames: usize) {
    stage.play(1, |world, seats| {
        for y in 10..=28 {
            for x in 100..=124 {
                let kind = if y == 28 {
                    TileKind::Bedrock
                } else if y == 20 && (107..=116).contains(&x) {
                    TileKind::Stone
                } else {
                    TileKind::Air
                };
                let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
            }
        }
        park_rig(world, seats[0].player, 111, 24);
        reveal(world, seats[0].player, 99..126, 9..30);
        for id in 0..9_u8 {
            let x = (109.5 + f32::from(id % 5)) * TILE_SIZE;
            let y = (19.0 - f32::from(id / 5)) * TILE_SIZE;
            world.insert_charge(fatherload_simulation::PlacedBomb {
                id: u64::from(id),
                x,
                y,
                previous_x: x,
                previous_y: y,
                timer_seconds: if id == 2 { 0.85 } else { 5.0 },
                detonation_tick: fatherload_simulation::SimulationTick::new(if id == 2 {
                    51
                } else {
                    300
                }),
                ..fatherload_simulation::PlacedBomb::default()
            });
        }
    });
    stage.settle(frames);
}

/// A scene inside `$zone` standing at its counter with `$modal` (the counter panel) open.
macro_rules! interior_scene {
    ($name:literal, $summary:literal, $zone:ident, $modal:ident) => {
        Scene {
            name: $name,
            summary: $summary,
            setup: |stage| {
                enter_interior(stage, SurfaceZone::$zone, Some(ModalScreen::$modal));
            },
        }
    };
}

/// A scene inside `$zone` with no panel open, the rig standing between the door and the counter
/// so the whole room is visible.
macro_rules! room_scene {
    ($name:literal, $summary:literal, $zone:ident) => {
        Scene {
            name: $name,
            summary: $summary,
            setup: |stage| {
                enter_interior(stage, SurfaceZone::$zone, None);
                stage.seat(0).interior_x = interior_service_x(SurfaceZone::$zone) - 150.0;
            },
        }
    };
}

/// An Xbox controller (the default glyph family).
const XBOX: InputDevice = InputDevice::Gamepad {
    index: 0,
    family: PadFamily::Xbox,
};
/// A `PlayStation` controller.
const PLAYSTATION: InputDevice = InputDevice::Gamepad {
    index: 1,
    family: PadFamily::PlayStation,
};
/// The split-screen WASD keyboard seat.
const WASD: InputDevice = InputDevice::Keyboard(KeyboardLayout::Wasd);
/// The split-screen arrow-keys keyboard seat.
const ARROWS: InputDevice = InputDevice::Keyboard(KeyboardLayout::Arrows);
/// A Nintendo controller.
const NINTENDO: InputDevice = InputDevice::Gamepad {
    index: 2,
    family: PadFamily::Nintendo,
};

/// Every scene, in capture order.
pub const SCENES: &[Scene] = &[
    Scene {
        name: "art_gallery",
        summary: "Art reference sheet: palette, UI kit widgets, icons and sprites",
        setup: |stage| stage.gallery = true,
    },
    Scene {
        name: "ui_sheet",
        summary: "UI reference sheet: menu icons and interior props at 1x and 3x",
        setup: |stage| stage.ui_sheet = true,
    },
    Scene {
        name: "title",
        summary: "Title menu over the idle world",
        setup: |_| {},
    },
    Scene {
        name: "bomb_cascade_before",
        summary: "Loose bombs, removed support and delayed blast cascade: before",
        setup: |stage| bomb_cascade_scene(stage, 45),
    },
    Scene {
        name: "bomb_cascade_blast",
        summary: "Loose bombs, removed support and delayed blast cascade: blast",
        setup: |stage| bomb_cascade_scene(stage, 52),
    },
    Scene {
        name: "bomb_cascade_airborne",
        summary: "Loose bombs, removed support and delayed blast cascade: airborne",
        setup: |stage| bomb_cascade_scene(stage, 62),
    },
    Scene {
        name: "bomb_cascade_after",
        summary: "Loose bombs, removed support and delayed blast cascade: after",
        setup: |stage| bomb_cascade_scene(stage, 100),
    },
    Scene {
        name: "surface_town",
        summary: "Rig parked on the ground among the town buildings, daytime sky",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 89, 4);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "town_west",
        summary: "Rig parked between the fuel station and the repair garage",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 60, 4);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "town_market",
        summary: "Rig parked between the upgrade shop and the bank",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 134, 4);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "town_east",
        summary: "Rig parked between the explosives bunker and the salvage yard",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 170, 4);
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "surface_flying",
        summary: "Rig thrusting up through the sky above town",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 104, 4);
            });
            stage.settle(10);
            stage.hold(
                40,
                PlayerInput {
                    thrust: true,
                    horizontal: 0.4,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "max_tier_speed",
        summary: "Top-tier engine at full speed down a long tunnel: light speed lines, a little \
                  camera lead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                speed_tunnel(world, seats[0].player);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    rig.engine_level = fatherload_simulation::economy::MAX_UPGRADE_LEVEL;
                }
            });
            stage.settle(10);
            stage.hold(
                60,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "god_mode_speed",
        summary: "God mode at full speed down a long tunnel: dense speed lines and wake, the \
                  camera leading well ahead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.enable_test_mode();
                speed_tunnel(world, seats[0].player);
            });
            stage.settle(10);
            stage.hold(
                60,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "god_mode_flight",
        summary: "God mode climbing fast over town: speed lines along the flight path",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.enable_test_mode();
                park_rig(world, seats[0].player, 60, 4);
            });
            stage.settle(10);
            stage.hold(
                20,
                PlayerInput {
                    thrust: true,
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "no_clip_flight",
        summary: "Test-mode no-clip flight diving straight through solid rock",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.enable_test_mode();
                let player = seats[0].player;
                sink_rig(world, player, 97, 30);
                reveal(world, player, 80..115, 20..60);
                let tick = world.simulation_tick();
                let mut inputs = fatherload_simulation::TickInputs::default();
                inputs.players.insert(
                    player,
                    fatherload_simulation::PlayerTickInput {
                        controls: None,
                        actions: vec![fatherload_simulation::TickAction {
                            sequence: fatherload_simulation::InputSequence::new(0),
                            command: fatherload_simulation::PlayerCommand::TestMode(
                                fatherload_simulation::TestModeAction::SetNoClip(true),
                            ),
                        }],
                    },
                );
                let _ = world.simulate_tick(tick, &inputs);
            });
            stage.hold(
                40,
                PlayerInput {
                    drill_down: true,
                    horizontal: 0.3,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "shallow_dig",
        summary: "Rig drilling down a few tiles under the surface",
        setup: |stage| {
            stage.play(1, |world, seats| {
                sink_rig(world, seats[0].player, 97, 9);
            });
            stage.settle(20);
            stage.hold(
                24,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "mid_depth_ores",
        summary: "Explored tunnels at mid depth showing ores and strata",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                // Classic row 42 (the grey stone / slate seam), wherever this world puts it.
                let row = world.terrain().scaled_row(42);
                sink_rig(world, player, 120, row);
                carve(world, 108..133, row..row + 1);
                reveal(world, player, 100..141, row - 9..row + 11);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "deep_lava",
        summary: "Deep hazard zone next to lava with the heat warning",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let rows = world.terrain().scaled_row(60)..world.terrain().scaled_row(200);
                let fallback = world.terrain().scaled_row(95);
                let (x, y) = find_tile(world, rows, |kind| kind == TileKind::Lava)
                    .map_or((120, fallback), |tile| (tile.x - 2, tile.y));
                sink_rig(world, player, x, y);
                reveal(world, player, x - 16..x + 17, y - 10..y + 11);
            });
            stage.settle(20);
        },
    },
    interior_scene!(
        "interior_fuel",
        "Fuel station with the pump counter open",
        Fuel,
        Fuel
    ),
    interior_scene!(
        "interior_repair",
        "Repair garage with the repair counter open",
        Repair,
        Repair
    ),
    interior_scene!(
        "interior_depot",
        "Ore depot with the sell counter open",
        Depot,
        Depot
    ),
    interior_scene!(
        "interior_hq",
        "Headquarters with the briefing desk open",
        Headquarters,
        Headquarters
    ),
    interior_scene!(
        "interior_shop",
        "Upgrade shop with the upgrade counter open",
        Shop,
        Shop
    ),
    interior_scene!("interior_bank", "Bank with the teller open", Bank, Bank),
    interior_scene!(
        "interior_explosives",
        "Explosives shack with the counter open",
        Explosives,
        Explosives
    ),
    interior_scene!(
        "interior_salvage",
        "Salvage yard with the counter open",
        Salvage,
        Salvage
    ),
    Scene {
        name: "interior_shop_walk",
        summary: "Upgrade shop room with no panel open, rig walking to the counter",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Shop, None);
            stage.seat(0).interior_x = interior_service_x(SurfaceZone::Shop) - 120.0;
        },
    },
    room_scene!("room_fuel", "Fuel station room, no panel open", Fuel),
    room_scene!("room_repair", "Repair garage room, no panel open", Repair),
    room_scene!("room_depot", "Ore depot room, no panel open", Depot),
    room_scene!("room_hq", "Headquarters room, no panel open", Headquarters),
    room_scene!("room_bank", "Bank room, no panel open", Bank),
    room_scene!(
        "room_explosives",
        "Explosives shack room, no panel open",
        Explosives
    ),
    room_scene!("room_salvage", "Salvage yard room, no panel open", Salvage),
    Scene {
        name: "toast_receipt",
        summary: "Upgrade shop counter just after a purchase, with the receipt toast",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Shop, Some(ModalScreen::Shop));
            "Bought Carbide Drill for 420 credits.".clone_into(&mut stage.seat(0).message);
            stage.settle(12);
        },
    },
    Scene {
        name: "hud_low_fuel",
        summary: "HUD close-up underground with the fuel warning",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 97, 18);
                if let Some(rig) = world.player_mut(player) {
                    rig.fuel = rig.fuel_capacity * 0.12;
                    load_cargo(rig);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "hud_low_hull",
        summary: "HUD close-up deep down with hull and heat warnings",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let row = world.terrain().scaled_row(66);
                sink_rig(world, player, 97, row);
                if let Some(rig) = world.player_mut(player) {
                    rig.hull = rig.max_hull() * 0.22;
                    load_cargo(rig);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "pause_menu",
        summary: "Pause menu over a running game",
        setup: |stage| {
            stage.play(1, |world, seats| {
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(20);
            stage.shell().run_mode = RunMode::Paused;
        },
    },
    Scene {
        name: "modal_exit_confirm",
        summary: "Unsaved-progress exit confirmation dialog",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
            });
            stage.settle(20);
            let shell = stage.shell();
            shell.modal = Some(ModalScreen::UnsavedExitConfirm);
            shell.selected_menu_item = 0;
        },
    },
    Scene {
        name: "modal_inventory",
        summary: "Inventory screen with a loaded cargo hold",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 97, 14);
                if let Some(rig) = world.player_mut(player) {
                    load_cargo(rig);
                }
            });
            stage.settle(20);
            stage.seat(0).modal = Some(ModalScreen::Inventory);
        },
    },
    Scene {
        name: "map",
        summary: "Mine map with explored tunnels",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 97, 30);
                carve(world, 97..98, 5..30);
                carve(world, 80..120, 30..31);
                reveal(world, player, 90..105, 4..32);
                reveal(world, player, 76..124, 26..35);
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::Map);
        },
    },
    Scene {
        name: "online_lobby_host",
        summary: "Hosting an online game: lobby with address and roster",
        setup: |stage| {
            let shell = stage.shell();
            shell.run_mode = RunMode::Lobby;
            shell.online.screen = OnlineScreen::Lobby;
            shell.online.selected = 0;
            shell.online.role = Some(OnlineRole::Host);
            "Hosting: share the address or descriptor, then start the game."
                .clone_into(&mut shell.message);
            shell.online.lobby = Some(LobbyInfo {
                roster: vec![
                    lobby_entry(1, "Ada", true, true, true),
                    lobby_entry(2, "Brutus", true, true, false),
                    lobby_entry(3, "Cass", false, true, false),
                    lobby_entry(4, "Dmitri", false, false, false),
                ],
                addr: "192.168.1.20:47777".parse().ok(),
                descriptor: Some("{}".to_owned()),
                lan_published: true,
                started: false,
                status: "Listening for players. 3 connected.".to_owned(),
                local_player: Some(NetPlayerId(1)),
                ..LobbyInfo::default()
            });
        },
    },
    Scene {
        name: "online_lobby_client",
        summary: "Joined an online game: lobby roster waiting on ready players",
        setup: |stage| {
            let shell = stage.shell();
            shell.run_mode = RunMode::Lobby;
            shell.online.screen = OnlineScreen::Lobby;
            shell.online.selected = 0;
            shell.online.role = Some(OnlineRole::Client);
            shell.online.ready = true;
            shell.message.clear();
            shell.online.lobby = Some(LobbyInfo {
                roster: vec![
                    lobby_entry(1, "Ada", true, true, true),
                    lobby_entry(2, "Brutus", true, true, false),
                    lobby_entry(3, "Cass", false, true, false),
                ],
                addr: "192.168.1.20:47777".parse().ok(),
                descriptor: None,
                lan_published: false,
                started: false,
                status: "Connected to the host. Waiting for the game to start.".to_owned(),
                local_player: Some(NetPlayerId(2)),
                ..LobbyInfo::default()
            });
        },
    },
    Scene {
        name: "online_join_address",
        summary: "Join-by-address form with a partly typed address",
        setup: |stage| {
            let shell = stage.shell();
            shell.run_mode = RunMode::Lobby;
            shell.online.screen = OnlineScreen::JoinAddress;
            "192.168.1.2".clone_into(&mut shell.online.text);
        },
    },
    Scene {
        name: "online_lan_browser",
        summary: "LAN browser listing two discovered sessions",
        setup: |stage| {
            let shell = stage.shell();
            shell.run_mode = RunMode::Lobby;
            shell.online.screen = OnlineScreen::LanBrowser;
            shell.online.selected = 0;
            "Searching the local network...".clone_into(&mut shell.online.lan_status);
            shell.online.lan_games = ["Ada's claim", "Night shift"]
                .into_iter()
                .enumerate()
                .map(
                    |(index, name)| fatherload_quic_transport::lan::LanDiscoveredGame {
                        instance_name: name.to_owned(),
                        host_name: "host.local".to_owned(),
                        game_addr: format!("192.168.1.{}:47777", 20 + index)
                            .parse()
                            .expect("address"),
                        descriptor_addr: format!("192.168.1.{}:47778", 20 + index)
                            .parse()
                            .expect("address"),
                        session_id: format!("session-{index}"),
                    },
                )
                .collect();
        },
    },
    Scene {
        name: "modal_options",
        summary: "Options dialog over the pause menu",
        setup: |stage| {
            stage.play(1, |world, seats| {
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(10);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.modal = Some(ModalScreen::Options);
            shell.selected_menu_item = 0;
        },
    },
    Scene {
        name: "modal_save_slots",
        summary: "Save slot picker",
        setup: |stage| {
            stage.play(1, |world, seats| {
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(10);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.modal = Some(ModalScreen::SaveSlots);
            shell.selected_menu_item = 1;
        },
    },
    Scene {
        name: "net_overlay",
        summary: "F3 network overlay over a hosted session",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
            });
            stage.settle(10);
            let shell = stage.shell();
            shell.online.show_net_debug = true;
            shell.online.net_debug = vec![
                "HOST tick 4210 | epoch 1 | phase Running | clamps 0".to_owned(),
                "clients 2 | up 18.4 KB/s | down 3.1 KB/s".to_owned(),
                "  c1 p2 192.168.1.31:51234 rtt 24ms loss 0.0% margin 2 late 0 held 0 buf 1"
                    .to_owned(),
                "  c2 p3 192.168.1.40:50987 rtt 61ms loss 1.2% margin 1 late 3 held 1 buf 2"
                    .to_owned(),
                "tuning: input delay 2 | snapshot every 3 ticks | buffer 4".to_owned(),
            ];
        },
    },
    Scene {
        name: "split_screen",
        summary: "Local split-screen with two views (surface and underground)",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 86, 4);
                if let Some(second) = seats.get(1) {
                    sink_rig(world, second.player, 112, 16);
                    reveal(world, second.player, 104..121, 4..23);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "split_shop_mining",
        summary: "Split-screen: player 1 drilling underground while player 2 buys at the shop counter",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let miner = seats[0].player;
                sink_rig(world, miner, 112, 14);
                if let Some(rig) = world.player_mut(miner) {
                    load_cargo(rig);
                }
                if let Some(shopper) = seats.get(1) {
                    park_at(world, shopper.player, SurfaceZone::Shop);
                    if let Some(rig) = world.player_mut(shopper.player) {
                        rig.credits = 1_260;
                    }
                }
            });
            put_seat_inside(stage, 1, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.seat(1).selected_menu_item = 1;
            stage.use_devices(&[WASD, ARROWS]);
            // The world keeps running for the miner while the shopper browses.
            stage.hold_seats(
                40,
                &[
                    PlayerInput {
                        drill_down: true,
                        ..PlayerInput::default()
                    },
                    PlayerInput::default(),
                ],
            );
            "Bought Carbide Drill.".clone_into(&mut stage.seat(1).message);
            stage.settle(12);
        },
    },
    Scene {
        name: "split_two_interiors",
        summary: "Split-screen: player 1 at the depot counter, player 2 walking through the fuel station",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Depot);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    load_cargo(rig);
                    rig.credits = 640;
                }
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Fuel);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::Depot));
            put_seat_inside(stage, 1, SurfaceZone::Fuel, None);
            stage.seat(1).interior_x = interior_service_x(SurfaceZone::Fuel) - 40.0;
            stage.use_devices(&[WASD, ARROWS]);
        },
    },
    Scene {
        name: "split_four_mixed",
        summary: "Four seats: depot counter, shop counter, one rig on the surface and one digging",
        setup: |stage| {
            stage.play(4, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Depot);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    load_cargo(rig);
                }
                park_at(world, seats[1].player, SurfaceZone::Shop);
                if let Some(rig) = world.player_mut(seats[1].player) {
                    rig.credits = 2_400;
                }
                park_rig(world, seats[2].player, 104, 4);
                sink_rig(world, seats[3].player, 118, 12);
                reveal(world, seats[3].player, 110..127, 4..20);
            });
            stage.settle(10);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::Depot));
            put_seat_inside(stage, 1, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.use_devices(&[WASD, ARROWS, XBOX, PLAYSTATION]);
        },
    },
    Scene {
        name: "split_three_seats",
        summary: "Three seats: salvage counter (WASD), a rig on the surface (arrow keys), the bank room (Xbox)",
        setup: |stage| {
            stage.play(3, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Salvage);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    rig.credits = 820;
                }
                park_rig(world, seats[1].player, 92, 4);
                park_at(world, seats[2].player, SurfaceZone::Bank);
            });
            stage.settle(10);
            put_seat_inside(stage, 0, SurfaceZone::Salvage, Some(ModalScreen::Salvage));
            put_seat_inside(stage, 2, SurfaceZone::Bank, None);
            stage.use_devices(&[WASD, ARROWS, XBOX]);
        },
    },
    Scene {
        name: "split_pad_counters",
        summary: "Split-screen counters side by side: keyboard prompts for player 1, Xbox for player 2",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Repair);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    rig.hull = rig.max_hull() * 0.45;
                    rig.credits = 900;
                }
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Explosives);
                    if let Some(rig) = world.player_mut(second.player) {
                        rig.credits = 300;
                    }
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Repair, Some(ModalScreen::Repair));
            put_seat_inside(
                stage,
                1,
                SurfaceZone::Explosives,
                Some(ModalScreen::Explosives),
            );
            stage.use_devices(&[WASD, XBOX]);
        },
    },
    Scene {
        name: "split_personal_dialogs",
        summary: "Split-screen: player 1 reading the inventory, player 2 the map, each in their own view",
        setup: |stage| {
            stage.play(2, |world, seats| {
                sink_rig(world, seats[0].player, 97, 14);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    load_cargo(rig);
                }
                if let Some(second) = seats.get(1) {
                    sink_rig(world, second.player, 112, 20);
                    reveal(world, second.player, 100..125, 4..24);
                }
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::Inventory);
            stage.seat(1).modal = Some(ModalScreen::Map);
            stage.use_devices(&[WASD, XBOX]);
        },
    },
    Scene {
        name: "split_room_prompts",
        summary: "Split-screen rooms: player 1 at the bank counter (PlayStation), player 2 at the HQ door (arrow keys)",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Bank);
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Headquarters);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Bank, None);
            put_seat_inside(stage, 1, SurfaceZone::Headquarters, None);
            stage.seat(1).interior_x = 60.0;
            stage.use_devices(&[PLAYSTATION, ARROWS]);
        },
    },
    Scene {
        name: "split_pause",
        summary: "The shared pause menu opened by player 2 (arrow keys) over both views",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Depot);
                if let Some(second) = seats.get(1) {
                    sink_rig(world, second.player, 112, 10);
                    reveal(world, second.player, 104..121, 4..16);
                }
            });
            stage.settle(10);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::Depot));
            stage.use_devices(&[WASD, ARROWS]);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.menu_seat = 1;
            shell.paused_by = 1;
        },
    },
    Scene {
        name: "split_rescue_help",
        summary: "Split-screen: player 1 reading the controls (Xbox), player 2 (arrow keys) stranded with the rescue prompt",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
                if let Some(second) = seats.get(1) {
                    sink_rig(world, second.player, 112, 20);
                    reveal(world, second.player, 104..121, 4..26);
                    if let Some(rig) = world.player_mut(second.player) {
                        rig.fuel = 0.0;
                    }
                }
            });
            stage.settle(20);
            stage.seat(0).modal = Some(ModalScreen::Help);
            stage.use_devices(&[XBOX, ARROWS]);
            "Game saved.".clone_into(&mut stage.shell().message);
            stage.settle(10);
        },
    },
    Scene {
        name: "split_four_rooms",
        summary: "Four seats, each at a different counter: fuel, repair, shop and headquarters",
        setup: |stage| {
            let zones = [
                SurfaceZone::Fuel,
                SurfaceZone::Repair,
                SurfaceZone::Shop,
                SurfaceZone::Headquarters,
            ];
            stage.play(4, |world, seats| {
                for (seat, zone) in seats.iter().zip(zones) {
                    park_at(world, seat.player, zone);
                    if let Some(rig) = world.player_mut(seat.player) {
                        rig.credits = 1_800;
                        rig.fuel = rig.fuel_capacity * 0.4;
                        rig.hull = rig.max_hull() * 0.6;
                    }
                }
            });
            stage.settle(5);
            for (seat, zone) in zones.into_iter().enumerate() {
                let modal = match zone {
                    SurfaceZone::Fuel => ModalScreen::Fuel,
                    SurfaceZone::Repair => ModalScreen::Repair,
                    SurfaceZone::Shop => ModalScreen::Shop,
                    _ => ModalScreen::Headquarters,
                };
                put_seat_inside(stage, seat, zone, Some(modal));
            }
            stage.use_devices(&[WASD, ARROWS, XBOX, PLAYSTATION]);
        },
    },
    Scene {
        name: "split_room_dialogs",
        summary: "Split-screen: personal dialogs opened inside rooms (inventory at the depot, map in the shop)",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Depot);
                if let Some(rig) = world.player_mut(seats[0].player) {
                    load_cargo(rig);
                }
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Shop);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::Inventory));
            put_seat_inside(stage, 1, SurfaceZone::Shop, Some(ModalScreen::Map));
            stage.use_devices(&[WASD, XBOX]);
        },
    },
    Scene {
        name: "split_exit_prompt",
        summary: "Split-screen: the window-close prompt over two seats inside buildings",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_at(world, seats[0].player, SurfaceZone::Bank);
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Shop);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Bank, None);
            put_seat_inside(stage, 1, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.use_devices(&[WASD, ARROWS]);
            stage.shell().save_dirty = true;
            stage.hold(
                1,
                PlayerInput {
                    exit_requested: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "split_pause_handoff",
        summary: "Split-screen: player 2 paused, then unplugged their controller; player 1 has the menu",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Fuel);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 1, SurfaceZone::Fuel, None);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.menu_seat = 1;
            shell.paused_by = 1;
            // Player 2's controller is gone: no gamepads are connected this frame.
            let report = stage.game.frame(
                &FrameInput {
                    devices: vec![WASD, XBOX],
                    ..FrameInput::default()
                },
                FRAME_SECONDS,
            );
            stage.absorb(report);
            stage.settle(10);
        },
    },
    Scene {
        name: "rescue_prompt",
        summary: "Stranded rig with the rescue prompt",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 97, 20);
                if let Some(rig) = world.player_mut(player) {
                    rig.fuel = 0.0;
                }
            });
            stage.settle(30);
        },
    },
    Scene {
        name: "lost_cargo",
        summary: "Rig back on the surface after a rescue, with its lost-cargo crate marked below",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                // Strand the rig close to the rescue drop-off so the crate stays on screen.
                sink_rig(world, player, 17, 10);
                if let Some(rig) = world.player_mut(player) {
                    load_cargo(rig);
                    rig.fuel = 0.0;
                }
            });
            stage.settle(10);
            stage.hold(
                2,
                PlayerInput {
                    confirm: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(30);
        },
    },
    Scene {
        name: "world_props",
        summary: "Tunnel with infrastructure, a ticking bomb, scanner pings and another player's rig",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 102, 30);
                carve(world, 96..122, 30..31);
                carve(world, 112..116, 27..30);
                reveal(world, player, 90..128, 22..40);
                if let Some(rig) = world.player_mut(player) {
                    rig.scanner_level = 3;
                    rig.bombs = 2;
                    rig.signal_relay_kits = 1;
                    rig.survey_drone_kits = 1;
                    rig.cargo_lift_kits = 1;
                    rig.tunnel_support_kits = 1;
                    rig.pump_station_kits = 1;
                    rig.ore_processor_kits = 1;
                }
            });
            stage.settle(10);
            let place = |input: fn(&mut PlayerInput)| {
                let mut held = PlayerInput::default();
                input(&mut held);
                held
            };
            let steps: [fn(&mut PlayerInput); 6] = [
                |input| input.place_support = true,
                |input| input.place_relay = true,
                |input| input.place_lift = true,
                |input| input.place_pump = true,
                |input| input.place_processor = true,
                |input| input.place_drone = true,
            ];
            for step in steps {
                stage.hold(1, place(step));
                stage.settle(2);
                stage.hold(
                    24,
                    PlayerInput {
                        horizontal: 1.0,
                        ..PlayerInput::default()
                    },
                );
                stage.settle(14);
            }
            stage.hold(
                1,
                PlayerInput {
                    bomb: true,
                    ..PlayerInput::default()
                },
            );
            stage.hold(
                10,
                PlayerInput {
                    horizontal: -1.0,
                    ..PlayerInput::default()
                },
            );
            stage.settle(20);
        },
    },
    Scene {
        name: "drill_side",
        summary: "Rig drilling sideways into stone: spinning bit, cracks and flying chips",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 100, 34);
                carve(world, 94..101, 34..35);
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x: 101, y: 34 }, TileKind::Stone);
                reveal(world, player, 88..114, 26..42);
                if let Some(rig) = world.player_mut(player) {
                    rig.drill_strength = 3;
                }
            });
            stage.settle(10);
            stage.hold(
                46,
                PlayerInput {
                    horizontal: 1.0,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "drill_up_crown_bore",
        summary: "Crown bore: the rig thrusts into a stone ceiling and drills upward",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 100, 34);
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x: 100, y: 30 }, TileKind::Stone);
                place_rig(world, player, 100, 31);
                reveal(world, player, 88..114, 22..42);
                if let Some(rig) = world.player_mut(player) {
                    rig.drill_strength = 3;
                    rig.drill_mount_level = 2;
                }
            });
            stage.hold(
                40,
                PlayerInput {
                    thrust: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "drill_side_gimbal_flight",
        summary: "Gimbal mount in a test-mode world: hovering in a shaft, drilling the wall sideways",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.enable_test_mode();
                sink_rig(world, player, 100, 34);
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x: 101, y: 29 }, TileKind::Stone);
                place_rig(world, player, 100, 29);
                reveal(world, player, 88..114, 22..42);
            });
            // Feather the thruster to hover while pushing into the wall.
            for _ in 0..8 {
                stage.hold(
                    3,
                    PlayerInput {
                        horizontal: 1.0,
                        thrust: true,
                        ..PlayerInput::default()
                    },
                );
                stage.hold(
                    2,
                    PlayerInput {
                        horizontal: 1.0,
                        ..PlayerInput::default()
                    },
                );
            }
        },
    },
    Scene {
        name: "shop_drill_mount",
        summary: "Upgrade shop with the late-game Drill Mount row selected (locked until the deep strata)",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.seat(0).selected_menu_item =
                fatherload_simulation::economy::DRILL_MOUNT_OFFER_INDEX;
        },
    },
    Scene {
        name: "shop_hazard_counters",
        summary: "Upgrade shop scrolled to the hazard counters (Boulder Deflector row selected)",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.seat(0).selected_menu_item =
                fatherload_simulation::economy::UPGRADE_OFFER_COUNT - 1;
        },
    },
    Scene {
        name: "test_mode_pause",
        summary: "Pause menu in a test-mode world: warp and god-mode toggles",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.enable_test_mode();
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(20);
            let shell = stage.shell();
            shell.pending_test_mode = Some(fatherload_simulation::TestModeState {
                invulnerable: true,
                infinite_fuel: false,
                no_clip: true,
            });
            shell.run_mode = RunMode::Paused;
            shell.selected_pause_item = 2;
        },
    },
    Scene {
        name: "test_mode_warp",
        summary: "Test mode's Warp to Layer list",
        setup: |stage| {
            stage.play(1, |world, seats| {
                world.enable_test_mode();
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(20);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.modal = Some(ModalScreen::WarpToLayer);
            shell.selected_menu_item = 6;
        },
    },
    Scene {
        name: "remote_rigs",
        summary: "Two local players side by side; each view shows the other rig with its tag",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 108, 4);
                if let Some(second) = seats.get(1) {
                    park_rig(world, second.player, 111, 4);
                    if let Some(rig) = world.player_mut(second.player) {
                        rig.fuel = 24.0;
                    }
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "bomb_blast",
        summary: "A bomb going off in a tunnel",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 104, 26);
                carve(world, 96..112, 26..27);
                reveal(world, player, 92..118, 18..34);
                if let Some(rig) = world.player_mut(player) {
                    rig.bombs = 1;
                }
            });
            stage.settle(5);
            stage.hold(
                1,
                PlayerInput {
                    bomb: true,
                    ..PlayerInput::default()
                },
            );
            stage.hold(
                30,
                PlayerInput {
                    horizontal: -1.0,
                    ..PlayerInput::default()
                },
            );
            stage.settle(126);
        },
    },
    Scene {
        name: "explosives_c4_armed",
        summary: "Five C4 charges stuck to floor, ceiling and wall, waiting for the remote",
        setup: |stage| explosive_scenes::c4_chain(stage, false),
    },
    Scene {
        name: "explosives_c4_chain",
        summary: "The remote detonator firing the C4 in sequence, caught mid-chain",
        setup: |stage| explosive_scenes::c4_chain(stage, true),
    },
    Scene {
        name: "explosives_cluster_split",
        summary: "A lobbed cluster bomb bursting into its fan of bomblets",
        setup: |stage| explosive_scenes::cluster_split(stage, false),
    },
    Scene {
        name: "explosives_cluster_pops",
        summary: "Cluster bomblets landing and popping across the cavern",
        setup: |stage| explosive_scenes::cluster_split(stage, true),
    },
    Scene {
        name: "explosives_shaped_jet",
        summary: "A shaped charge's jet cutting straight down into the floor",
        setup: |stage| explosive_scenes::shaped_tunnel(stage, false),
    },
    Scene {
        name: "explosives_shaped_tunnel",
        summary: "The narrow tunnel a shaped charge leaves behind",
        setup: |stage| explosive_scenes::shaped_tunnel(stage, true),
    },
    Scene {
        name: "explosives_throw_preview",
        summary: "Aiming a sticky bomb: the dotted throw arc and a lit flare lighting the cavern",
        setup: explosive_scenes::throw_preview,
    },
    Scene {
        name: "cave_grand_cavern",
        summary: "A rig in a grand cavern: stalactites, stalagmites, bats and the dark",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::GrandCavern);
        },
    },
    Scene {
        name: "cave_crystal_geode",
        summary: "A glowing crystal geode lined with gems, glow moths drifting",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::CrystalGeode);
        },
    },
    Scene {
        name: "cave_flooded",
        summary: "A flooded cave: an underground lake with a silt bed, glowing algae and cave fish",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::FloodedCave);
        },
    },
    Scene {
        name: "cave_lava_tube",
        summary: "A lava tube: obsidian walls, lava pooling on the floor, ember moths",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::LavaTube);
        },
    },
    Scene {
        name: "cave_fungal_grotto",
        summary: "A fungal grotto: giant bouncy mushrooms, glowcaps and fireflies",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::FungalGrotto);
        },
    },
    Scene {
        name: "cave_worm_burrows",
        summary: "Worm burrows ribbed with old bone, bats in the dark",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::WormBurrows);
        },
    },
    Scene {
        name: "cave_ancient_ruins",
        summary: "Ancient ruins: a brick hall with pillars, glowing glyphs and a relic cache",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::AncientRuins);
        },
    },
    Scene {
        name: "cave_chasm",
        summary: "A chasm: a tall dark crack with ledges and stalactites",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::Chasm);
        },
    },
    Scene {
        name: "cave_network",
        summary: "A cave network: small chambers strung together by tunnels",
        setup: |stage| {
            cave_scenes::cave_of_kind(stage, fatherload_simulation::cave::CaveKind::CaveNetwork);
        },
    },
    Scene {
        name: "cave_dark_lamp",
        summary: "A pitch-dark grand cavern lit only by the rig's lamp",
        setup: |stage| cave_scenes::dark_cavern(stage, false),
    },
    Scene {
        name: "cave_dark_flares",
        summary: "The same dark cavern lit by two flares thrown across the floor",
        setup: |stage| cave_scenes::dark_cavern(stage, true),
    },
    Scene {
        name: "explosives_lineup",
        summary: "Every explosive kind side by side: bomb, sticky, cluster, bomblet, C4, shaped, mining, incendiary, EMP, flare, smoke, scare",
        setup: explosive_scenes::lineup,
    },
    Scene {
        name: "drops_bomb_spray",
        summary: "A C4 blast in an ore seam: gold, gems, an idol and blocks fountaining out as drops",
        setup: drop_scenes::bomb_spray,
    },
    Scene {
        name: "drops_bomb_loot",
        summary: "A mining charge's loot: every ore tile popped out intact, landed and stacked in the crater",
        setup: drop_scenes::bomb_loot,
    },
    Scene {
        name: "drops_magnet_pickup",
        summary: "The rig's magnet pulling a field of drops in, collected ones flying into the hatch",
        setup: drop_scenes::magnet_pickup,
    },
    Scene {
        name: "drops_bay_full",
        summary: "A full cargo bay: ore in reach stays put and marked, BAY FULL over the rig",
        setup: drop_scenes::bay_full,
    },
    Scene {
        name: "modal_hopper",
        summary: "The inventory's block hopper: fill, the collect toggle and dump rows",
        setup: drop_scenes::hopper_inventory,
    },
    Scene {
        name: "explosives_smoke_scare",
        summary: "A smoke screen billowing over the rig and a scare charge's bang further along",
        setup: explosive_scenes::smoke_and_scare,
    },
    Scene {
        name: "weapon_rivet_gun",
        summary: "A rivet gun stream hammering practice targets",
        setup: weapon_scenes::rivet_gun,
    },
    Scene {
        name: "weapon_scatter_blaster",
        summary: "A scatter blaster's pellet fan hitting two targets",
        setup: weapon_scenes::scatter_blaster,
    },
    Scene {
        name: "weapon_plasma_cutter",
        summary: "The plasma cutter's beam slicing into a stone pillar",
        setup: weapon_scenes::plasma_cutter,
    },
    Scene {
        name: "weapon_rocket_launcher",
        summary: "A rocket in flight and another bursting against the far wall",
        setup: weapon_scenes::rocket_launcher,
    },
    Scene {
        name: "weapon_harpoon",
        summary: "A harpoon flying at a gold nugget to reel it in",
        setup: weapon_scenes::harpoon,
    },
    Scene {
        name: "weapon_railgun",
        summary: "A railgun slug piercing two targets and punching through a dirt wall",
        setup: weapon_scenes::railgun,
    },
    Scene {
        name: "weapon_split_reticles",
        summary: "Split-screen aim reticles: a free stick crosshair and an auto-aim lock",
        setup: weapon_scenes::split_reticles,
    },
    Scene {
        name: "weapon_arms_rack",
        summary: "The Explosives Shack counter's arms rack: weapons, upgrades, capacitor, rockets",
        setup: weapon_scenes::arms_rack,
    },
    Scene {
        name: "explosives_inventory",
        summary: "The inventory listing every explosive type, carried or not",
        setup: weapon_scenes::explosives_inventory,
    },
    Scene {
        name: "gas_leak",
        summary: "Drilling into a gas pocket releases a toxic cloud",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = find_tile(world, 26..70, |kind| kind == TileKind::Gas)
                    .map_or((110, 30), |tile| (tile.x, tile.y - 1));
                sink_rig(world, player, x, y);
                reveal(world, player, x - 14..x + 15, y - 8..y + 9);
            });
            stage.settle(10);
            stage.hold(
                40,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
            stage.settle(4);
        },
    },
    Scene {
        name: "cave_in",
        summary: "Drilling an explosive pocket sets off a cave-in of falling boulders",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = find_tile(world, 30..80, |kind| kind == TileKind::ExplosivePocket)
                    .map_or((110, 40), |tile| (tile.x, tile.y - 1));
                sink_rig(world, player, x, y);
                // The pocket right under the rig (also on worlds whose search found none).
                world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y: y + 1 }, TileKind::ExplosivePocket);
                carve(world, x - 2..x + 3, y - 3..y + 1);
                reveal(world, player, x - 14..x + 15, y - 8..y + 9);
            });
            stage.settle(10);
            // Drill it, then catch the boulders hanging (shaking, shedding dust) before they
            // drop.
            stage.hold(
                45,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "entity_hazards",
        summary: "Gas clouds and boulders (hanging and falling) as entities beside the rig",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = (110, 40);
                sink_rig(world, player, x, y);
                carve(world, x - 9..x + 10, y - 6..y + 1);
                reveal(world, player, x - 14..x + 15, y - 8..y + 9);
                // World pixels `dx`, `dy` tiles from the rig's tile.
                #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
                let at =
                    |dx: f32, dy: f32| ((x as f32 + dx) * TILE_SIZE, (y as f32 + dy) * TILE_SIZE);
                let (gas_a, gas_b) = (at(-5.0, -2.0), at(-7.0, -4.0));
                let (rock_a, rock_b) = (at(4.5, -5.5), at(7.5, -5.5));
                let now = world.simulation_tick().get();
                world.restore_legacy_live_entities(
                    vec![
                        fatherload_simulation::HazardCloud {
                            x: gas_a.0,
                            y: gas_a.1,
                            life: 5.0,
                            radius: 26.0,
                        },
                        fatherload_simulation::HazardCloud {
                            x: gas_b.0,
                            y: gas_b.1,
                            life: 7.5,
                            radius: 16.0,
                        },
                    ],
                    vec![
                        fatherload_simulation::FallingBoulder {
                            x: rock_a.0,
                            y: rock_a.1,
                            velocity_y: 0.0,
                            warning_seconds: 2.0,
                            life: 6.0,
                        },
                        fatherload_simulation::FallingBoulder {
                            x: rock_b.0,
                            y: rock_b.1,
                            velocity_y: 0.0,
                            warning_seconds: 0.0,
                            life: 6.0,
                        },
                    ],
                    &[SimulationTick::new(now + 600), SimulationTick::new(now)],
                    &[
                        SimulationTick::new(now + 900),
                        SimulationTick::new(now + 900),
                    ],
                );
            });
            stage.settle(12);
        },
    },
    Scene {
        name: "deep_crystal_faults",
        summary: "The Crystal Faults stratum far below the surface",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let row = band_row(world, Band::Deep(DeepStratum::CrystalFaults), 8);
                sink_rig(world, player, 120, row);
                carve(world, 108..133, row..row + 1);
                reveal(world, player, 98..143, row - 10..row + 12);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "deep_void_geodes",
        summary: "The Void Geode Fields and Mantle Storm strata near the bottom",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let row = band_row(world, Band::Deep(DeepStratum::MantleStormZone), -2);
                sink_rig(world, player, 120, row);
                carve(world, 108..133, row..row + 1);
                reveal(world, player, 98..143, row - 10..row + 13);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "bedrock_floor",
        summary: "The bottom of a deep world: the rig on the bedrock floor, solid rock below",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let settings = *world.world_settings();
                let x = 120;
                let floor = fatherload_simulation::worldgen::strata::bedrock_top_row(&settings, x)
                    .unwrap_or_else(|| world.terrain().height());
                let row = floor - 1;
                sink_rig(world, player, x, row);
                carve(world, x - 12..x + 13, row - 1..row + 1);
                reveal(
                    world,
                    player,
                    x - 24..x + 25,
                    row - 12..world.terrain().height(),
                );
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "bedrock_corner",
        summary: "The bottom-left corner of a deep world: the floor meets the west wall, no sky",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let settings = *world.world_settings();
                let x = 2;
                let floor = fatherload_simulation::worldgen::strata::bedrock_top_row(&settings, x)
                    .unwrap_or_else(|| world.terrain().height());
                let row = floor - 1;
                sink_rig(world, player, x, row);
                carve(world, 0..x + 10, row - 1..row + 1);
                reveal(world, player, 0..x + 30, row - 12..world.terrain().height());
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "endless_west_of_town",
        summary: "An endless world west of town: the ground runs on (no world edge) with the placeholder surface swell lowering it a row in places, the minimap scrolling with the rig",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let x = -128;
                let row = world.terrain().surface_row(x) - 1;
                place_rig(world, player, x, row);
                carve(world, x + 6..x + 9, row + 1..row + 9);
                reveal(world, player, x - 70..x + 90, 0..row + 30);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "endless_far_out",
        summary: "Fifty thousand tiles east of town: streamed chunks, the surface swell, a dug shaft and the scrolling minimap",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let x = 50_000;
                let surface = world.terrain().surface_row(x);
                sink_rig(world, player, x, surface + 14);
                carve(world, x - 6..x + 7, surface + 12..surface + 15);
                reveal(world, player, x - 70..x + 70, 0..surface + 40);
                world.stream_terrain_near_players();
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "new_world_setup",
        summary: "New World dialog over the title: seed, size, resources and hazards",
        setup: |stage| {
            let shell = stage.shell();
            shell.open_world_setup(SetupPurpose::Local { seats: 1 });
            if let Some(setup) = shell.world_setup.as_mut() {
                setup.settings = WorldSettings {
                    size: WorldSize::Large,
                    richness: ResourceRichness::Rich,
                    ..WorldSettings::new(0x3F2A_91C0_77B1)
                };
                setup.selected = 3;
            }
        },
    },
    Scene {
        name: "new_world_seed_entry",
        summary: "Typing a seed into the New World dialog (split-screen setup)",
        setup: |stage| {
            let shell = stage.shell();
            shell.open_world_setup(SetupPurpose::Local { seats: 2 });
            if let Some(setup) = shell.world_setup.as_mut() {
                setup.settings = WorldSettings::new(0x0BAD_5EED);
                setup.seed_edit = Some("deep rocks".to_owned());
            }
        },
    },
    Scene {
        name: "hazard_telltales",
        summary: "Explored rock around hazard pockets showing their tell-tale stains",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                // The default world's first gas cluster below the clay beds.
                let (x, y) = home_features(world)
                    .of_kind(FeatureKind::HazardCluster)
                    .find(|feature| feature.detail == FeatureDetail::Hazard(HazardKind::Gas))
                    .map_or((110, 40), |feature| (feature.center.x, feature.center.y));
                sink_rig(world, player, x - 5, y);
                reveal(world, player, x - 18..x + 19, y - 9..y + 10);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "strata_boundary",
        summary: "The wavy, interleaved seam where the Basalt Deeps give way to the Crystal Faults",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let (x, y) = home_features(world)
                    .of_kind(FeatureKind::StratumTransition)
                    .find(|feature| {
                        matches!(
                            feature.detail,
                            FeatureDetail::Transition {
                                upper: Band::Basalt,
                                ..
                            }
                        )
                    })
                    .map_or((120, 90), |feature| (feature.center.x, feature.center.y));
                let x = x.clamp(world.terrain().min_x() + 30, world.terrain().max_x() - 30);
                sink_rig(world, player, x, y - 3);
                carve(world, x - 12..x + 13, y - 3..y - 2);
                reveal(world, player, x - 24..x + 25, y - 12..y + 12);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_supply_cache",
        summary: "A supply cache: a hollow of crates below the clay",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| variant == PoiVariant::SupplyCache);
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_derelict_rig",
        summary: "A crashed prospector rig with its glowing salvage core",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    matches!(variant, PoiVariant::DerelictRig { .. })
                });
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_vault_sequence",
        summary: "A sealed vault with a sequence lock: numbered floor switches",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Vault(VaultPuzzle::Sequence)
                });
                visit_poi(world, seats[0].player, &poi);
                // The first switch is already lit.
                light_vault_switches(world, &poi, 1);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_vault_seals",
        summary: "A sealed vault with pressure seals in its walls",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Vault(VaultPuzzle::Seals)
                });
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_vault_relay",
        summary: "A sealed vault with twin relays, one lit and its timer draining",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Vault(VaultPuzzle::Relay)
                });
                visit_poi(world, seats[0].player, &poi);
                light_vault_switches(world, &poi, 1);
                start_relay_countdown(world, &poi, 9_000);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_vault_open",
        summary: "An opened vault: the doors are gone and the cache waits on its pedestal",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Vault(VaultPuzzle::Sequence)
                });
                visit_poi(world, seats[0].player, &poi);
                light_vault_switches(world, &poi, 3);
                for position in poi.bounds.positions() {
                    if world.terrain().tile(position).map(|tile| tile.kind)
                        == Some(TileKind::Poi(PoiTile::VaultDoor))
                    {
                        world.terrain_mut().set_kind(position, TileKind::Air);
                    }
                }
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_fossil_bed",
        summary: "A fossil bed formation packed with artifacts, guarded by gas",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Formation(FormationStyle::FossilBed)
                });
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_geode",
        summary: "A deep geode: a crystal shell lined with rich ore, guarded by its layer",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| {
                    variant == PoiVariant::Formation(FormationStyle::Geode)
                });
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_rival_claim",
        summary: "An abandoned rival outpost with salvage lockers",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| variant == PoiVariant::RivalClaim);
                visit_poi(world, seats[0].player, &poi);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_scanner_silhouette",
        summary: "A tier-3 scanner outlining an unexplored vault's silhouette nearby",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| matches!(variant, PoiVariant::Vault(_)));
                approach_poi(world, seats[0].player, &poi, 3);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "poi_scanner_echo",
        summary: "A tier-1 scanner hearing a strange echo from a site it cannot resolve",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let poi = stage_poi(world, |variant| matches!(variant, PoiVariant::Vault(_)));
                approach_poi(world, seats[0].player, &poi, 1);
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "map_poi_contacts",
        summary: "Mine map with visited sites, identified silhouettes and strange echoes",
        setup: |stage| {
            stage.play(1, |world, seats| {
                stage_poi_contacts(world, seats[0].player);
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::Map);
        },
    },
    Scene {
        name: "modal_collection_log",
        summary: "Collection log: sites visited, scanner contacts and recovered logs",
        setup: |stage| {
            stage.play(1, |world, seats| {
                stage_poi_contacts(world, seats[0].player);
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::ResearchLog);
        },
    },
    Scene {
        name: "prompt_sheet",
        summary: "Button-prompt reference: every action on keyboard, Xbox, PlayStation, Nintendo",
        setup: |stage| stage.prompt_sheet = true,
    },
    Scene {
        name: "pad_title",
        summary: "Title menu with Xbox controller prompts",
        setup: |stage| stage.use_devices(&[XBOX]),
    },
    Scene {
        name: "pad_interior_shop",
        summary: "Upgrade shop counter with Xbox controller prompts",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Shop, Some(ModalScreen::Shop));
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "pad_room",
        summary: "Walking up to a counter with Xbox controller prompts",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Repair, None);
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "pad_pause",
        summary: "Pause menu with Xbox controller prompts",
        setup: |stage| {
            stage.play(1, |world, seats| {
                sink_rig(world, seats[0].player, 97, 12);
            });
            stage.settle(20);
            stage.use_devices(&[XBOX]);
            stage.shell().run_mode = RunMode::Paused;
        },
    },
    Scene {
        name: "pad_hud",
        summary: "Rig at the HQ door with the Xbox enter prompt on the HUD",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 106, 4);
            });
            stage.settle(20);
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "keyboard_hud",
        summary: "Rig at the HQ door with the keyboard enter prompt on the HUD",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 106, 4);
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "pad_help",
        summary: "Controls reference for an Xbox controller",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
            });
            stage.settle(10);
            stage.use_devices(&[XBOX]);
            stage.seat(0).modal = Some(ModalScreen::Help);
        },
    },
    Scene {
        name: "pad_split_screen",
        summary: "Split-screen: player 1 on the WASD keyboard, player 2 on an Xbox controller",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 88, 4);
                if let Some(second) = seats.get(1) {
                    park_rig(world, second.player, 124, 4);
                }
            });
            stage.settle(20);
            stage.use_devices(&[InputDevice::Keyboard(KeyboardLayout::Wasd), XBOX]);
        },
    },
    Scene {
        name: "pad_split_join",
        summary: "Three seats: keyboard, controller, and a seat waiting for a controller",
        setup: |stage| {
            stage.play(3, |world, seats| {
                for (seat, x) in seats.iter().zip([52, 106, 130]) {
                    park_rig(world, seat.player, x, 4);
                }
            });
            stage.settle(20);
            stage.use_devices(&[
                InputDevice::Keyboard(KeyboardLayout::Wasd),
                XBOX,
                InputDevice::Unassigned,
            ]);
        },
    },
    Scene {
        name: "pad_playstation_counter",
        summary: "Ore depot counter with PlayStation glyphs",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Depot, Some(ModalScreen::Depot));
            stage.use_devices(&[PLAYSTATION]);
        },
    },
    Scene {
        name: "interior_depot_last_row",
        summary: "Ore depot counter on its last row, scrolled to the hold and prices below",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Depot, Some(ModalScreen::Depot));
            stage.seat(0).selected_menu_item = 4;
        },
    },
    Scene {
        name: "pad_interior_depot_last_row",
        summary: "Ore depot counter on its last row with Xbox controller prompts",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Depot, Some(ModalScreen::Depot));
            stage.seat(0).selected_menu_item = 4;
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "modal_help",
        summary: "Controls reference for the keyboard",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::Help);
        },
    },
    Scene {
        name: "pad_nintendo_options",
        summary: "Options over the pause menu with Nintendo glyphs",
        setup: |stage| {
            stage.play(1, |world, seats| {
                park_rig(world, seats[0].player, 97, 4);
            });
            stage.settle(10);
            stage.use_devices(&[NINTENDO]);
            let shell = stage.shell();
            shell.run_mode = RunMode::Paused;
            shell.modal = Some(ModalScreen::Options);
        },
    },
    Scene {
        name: "quest_desk",
        summary: "Kade's quest desk: a job to turn in, one on offer, one running, locked ones",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                quest_history(world, player);
                accept_for(world, player, QuestId(103));
                accept_for(world, player, QuestId(102));
                finish_quest(world, player, QuestId(102));
                park_at(world, player, SurfaceZone::Depot);
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::QuestDesk));
            stage.seat(0).selected_menu_item = 1;
        },
    },
    Scene {
        name: "quest_counter_tab",
        summary: "The depot counter with its Quests tab and a finished job waiting",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                quest_history(world, player);
                accept_for(world, player, QuestId(102));
                finish_quest(world, player, QuestId(102));
                park_at(world, player, SurfaceZone::Depot);
                if let Some(rig) = world.player_mut(player) {
                    load_cargo(rig);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::Depot));
        },
    },
    Scene {
        name: "quest_board",
        summary: "Director Vale's HQ quest board with today's postings",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                quest_history(world, player);
                park_at(world, player, SurfaceZone::Headquarters);
            });
            stage.settle(5);
            put_seat_inside(
                stage,
                0,
                SurfaceZone::Headquarters,
                Some(ModalScreen::QuestDesk),
            );
        },
    },
    Scene {
        name: "quest_log",
        summary: "The quest log: active jobs, the tracked job's objectives, reputation, blueprints",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                quest_history(world, player);
                accept_for(world, player, QuestId(302));
                accept_for(world, player, QuestId(501));
                accept_for(world, player, QuestId(103));
                sink_rig(world, player, 97, 12);
            });
            stage.settle(20);
            stage.seat(0).modal = Some(ModalScreen::QuestLog);
            stage.seat(0).selected_menu_item = 0;
            stage.seat(0).tracked_quest = Some(QuestId(302));
        },
    },
    Scene {
        name: "quest_hud_tracker",
        summary: "Underground by Nix's first demolition mark: the HUD tracker and quest markers",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                accept_for(world, player, QuestId(301));
                let spots = quest_spots(world, player, QuestId(301));
                let spot = spots[0];
                sink_rig(world, player, spot.x - 3, spot.y);
                carve(world, spot.x - 3..spot.x + 1, spot.y..spot.y + 1);
                reveal(
                    world,
                    player,
                    spot.x - 20..spot.x + 20,
                    spot.y - 12..spot.y + 12,
                );
            });
            stage.settle(20);
            stage.seat(0).tracked_quest = Some(QuestId(301));
        },
    },
    Scene {
        name: "quest_map_markers",
        summary: "The mine map with quest markers: demolition marks and a supply cache target",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                accept_for(world, player, QuestId(301));
                accept_for(world, player, QuestId(502));
                sink_rig(world, player, 97, 24);
                reveal(world, player, 60..140, 4..48);
            });
            stage.settle(10);
            stage.seat(0).modal = Some(ModalScreen::Map);
        },
    },
    Scene {
        name: "quest_complete_toast",
        summary: "The completion fanfare after turning a job in at Kade's desk",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                quest_history(world, player);
                if let Some(rig) = world.player_mut(player) {
                    rig.quests.completed.insert(QuestId(103));
                }
                park_at(world, player, SurfaceZone::Depot);
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Depot, Some(ModalScreen::QuestDesk));
            let def = QuestId(103).def().expect("quest");
            stage.seat(0).message = format!("Kade: {}", def.text.complete);
            stage
                .shell()
                .quest_fanfare(0, FanfareKind::Complete, def.title, def.reward.summary());
            stage.settle(20);
        },
    },
    Scene {
        name: "split_crew_quest",
        summary: "Two seats on a crew job: both see the fuse boxes they must light together",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let (lead, wing) = (seats[0].player, seats[1].player);
                if let Some(rig) = world.player_mut(lead) {
                    rig.quests.completed.insert(QuestId(302));
                    rig.quests.deepest_row = fatherload_simulation::worldgen::strata::scaled_row(
                        &fatherload_simulation::worldgen::WorldSettings::default(),
                        45,
                    );
                }
                accept_crew(world, lead, QuestId(305));
                let spots = quest_spots(world, lead, QuestId(305));
                for (player, spot) in [(lead, spots[0]), (wing, spots[1])] {
                    sink_rig(world, player, spot.x - 2, spot.y);
                    carve(world, spot.x - 2..spot.x + 1, spot.y..spot.y + 1);
                    reveal(
                        world,
                        player,
                        spot.x - 22..spot.x + 22,
                        spot.y - 12..spot.y + 12,
                    );
                }
            });
            stage.settle(20);
        },
    },
];

/// Enters `zone`'s interior standing at its counter, optionally with a counter panel open.
fn enter_interior(stage: &mut SceneStage<'_>, zone: SurfaceZone, modal: Option<ModalScreen>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        park_rig(world, player, 97, 4);
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 4_850;
            rig.fuel = rig.fuel_capacity * 0.55;
            rig.hull = rig.max_hull() * 0.7;
            load_cargo(rig);
        }
    });
    stage.settle(5);
    put_seat_inside(stage, 0, zone, modal);
}

/// Puts seat `seat` inside `zone` at its counter, optionally with a counter panel open.
fn put_seat_inside(
    stage: &mut SceneStage<'_>,
    seat: usize,
    zone: SurfaceZone,
    modal: Option<ModalScreen>,
) {
    let seat_ui = stage.seat(seat);
    seat_ui.interior_zone = Some(zone);
    seat_ui.interior_x = interior_service_x(zone);
    seat_ui.interior_facing = 1.0;
    seat_ui.modal = modal;
    seat_ui.selected_menu_item = usize::from(modal == Some(ModalScreen::Depot));
}

fn lobby_entry(id: u32, name: &str, ready: bool, connected: bool, local: bool) -> LobbyEntry {
    LobbyEntry {
        player: NetPlayerId(id),
        name: name.to_owned(),
        ready,
        connected,
        local,
        parked: false,
    }
}

/// Swaps in the first world (from the default seed on) that has a POI `wanted` accepts, and
/// returns that POI.
fn stage_poi(world: &mut WorldState, wanted: fn(PoiVariant) -> bool) -> WorldFeature {
    let (terrain, poi) = (0..256_u64)
        .find_map(|offset| {
            let terrain =
                Terrain::generate(&WorldSettings::new(DEFAULT_WORLD_SEED.wrapping_add(offset)));
            let poi = terrain_home_features(&terrain)
                .pois()
                .find(|poi| poi.poi().is_some_and(wanted))
                .copied()?;
            Some((terrain, poi))
        })
        .expect("some seed holds the wanted POI");
    *world.terrain_mut() = terrain;
    poi
}

/// A rig dug in just left of `poi`, a tunnel up to its edge, and the POI and its surroundings
/// explored.
fn visit_poi(world: &mut WorldState, player: PlayerId, poi: &WorldFeature) {
    let bounds = poi.bounds;
    let (x, y) = ((bounds.min.x - 3).max(2), bounds.max.y - 1);
    sink_rig(world, player, x, y);
    carve(world, x..bounds.min.x, y..y + 1);
    reveal(
        world,
        player,
        bounds.min.x - 14..bounds.max.x + 15,
        bounds.min.y - 7..bounds.max.y + 8,
    );
}

/// A rig with a scanner of `scanner_level` dug in a few tiles left of `poi`, with only its own
/// surroundings explored.
fn approach_poi(world: &mut WorldState, player: PlayerId, poi: &WorldFeature, scanner_level: u8) {
    let bounds = poi.bounds;
    let (x, y) = ((bounds.min.x - 7).max(2), bounds.max.y - 1);
    sink_rig(world, player, x, y);
    carve(world, x - 3..x + 3, y..y + 1);
    reveal(world, player, x - 6..x + 4, y - 4..y + 3);
    if let Some(rig) = world.player_mut(player) {
        rig.scanner_level = scanner_level;
    }
}

/// Lights the first `count` switches (by order) or relays of a vault.
fn light_vault_switches(world: &mut WorldState, poi: &WorldFeature, count: u8) {
    let mut relays = 0;
    for position in poi.bounds.positions() {
        let lit = match world.terrain().tile(position).map(|tile| tile.kind) {
            Some(TileKind::Poi(PoiTile::VaultSwitch { order, .. })) if order <= count => {
                PoiTile::VaultSwitch { order, lit: true }
            }
            Some(TileKind::Poi(PoiTile::VaultRelay { .. })) if relays < count => {
                relays += 1;
                PoiTile::VaultRelay { lit: true }
            }
            _ => continue,
        };
        world.terrain_mut().set_kind(position, TileKind::Poi(lit));
    }
}

/// Starts `vault`'s twin-relay countdown with `millis` left (as if its first relay was just
/// drilled a while ago).
fn start_relay_countdown(world: &mut WorldState, vault: &WorldFeature, millis: u64) {
    let Some(SectionPayload::Timers(mut timers)) = world.section(SectionKey::Timers) else {
        return;
    };
    let deadline = world.simulation_tick().get() + world.timing().ticks_for_millis(millis);
    timers
        .vault_relay_deadlines
        .insert(vault.id, SimulationTick::new(deadline));
    world.apply_section(SectionKey::Timers, SectionPayload::Timers(timers));
}

/// A mid-depth rig with a tier-2 scanner (live echoes), a few visited and identified sites in
/// its journal and three recovered logs.
fn stage_poi_contacts(world: &mut WorldState, player: PlayerId) {
    let pois: Vec<WorldFeature> = home_features(world).pois().copied().collect();
    let anchor = pois
        .iter()
        .find(|poi| poi.band == Band::Slate)
        .map_or((120, 55), |poi| {
            (poi.bounds.min.x - 3, poi.bounds.max.y - 1)
        });
    sink_rig(world, player, anchor.0, anchor.1);
    carve(world, anchor.0 - 20..anchor.0 + 3, anchor.1..anchor.1 + 1);
    reveal(
        world,
        player,
        anchor.0 - 30..anchor.0 + 30,
        anchor.1 - 20..anchor.1 + 12,
    );
    let Some(rig) = world.player_mut(player) else {
        return;
    };
    rig.scanner_level = 2;
    for (index, poi) in pois.iter().enumerate() {
        let near = (poi.center.y - anchor.1).abs() < 40;
        let knowledge = match index % 3 {
            0 if near => PoiKnowledge::Visited,
            1 if near => PoiKnowledge::Identified,
            _ => continue,
        };
        rig.journal.learn(poi.id, knowledge);
    }
    for log in [0, 4, 7] {
        rig.journal.logs.insert(LoreId(log));
    }
}

fn load_cargo(rig: &mut fatherload_simulation::Player) {
    for (mineral, count) in [
        (MineralKind::Copper, 6),
        (MineralKind::Iron, 4),
        (MineralKind::Silver, 2),
        (MineralKind::Gold, 1),
    ] {
        rig.cargo_bay.insert(mineral, count);
    }
}

/// A rig with some quest history: Kade's first job done, a little reputation, a fragment.
fn quest_history(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.credits = 1_240;
        rig.scanner_level = 1;
        let log = &mut rig.quests;
        log.completed.insert(QuestId(101));
        log.completed.insert(QuestId(301));
        log.reputation.insert(QuestGiver::Kade, 13);
        log.reputation.insert(QuestGiver::Nix, 6);
        log.reputation.insert(QuestGiver::Pip, 6);
        log.deepest_row = fatherload_simulation::worldgen::strata::scaled_row(
            &fatherload_simulation::worldgen::WorldSettings::default(),
            34,
        );
        log.fragments.insert(BlueprintId::HaulerFrame, 1);
        log.fragments.insert(BlueprintId::DampenerCoil, 2);
    }
}

/// Has `player` take `quest` (crew mode when `crew`), standing at the giver's door for one tick
/// and going back where it was.
fn accept_quest_as(world: &mut WorldState, player: PlayerId, quest: QuestId, crew: bool) {
    use fatherload_simulation::{
        InputSequence, PlayerCommand, PlayerTickInput, TickAction, TickInputs,
        quest::binding::giver_door, service::SessionServiceRequest,
    };
    let Some(def) = quest.def() else {
        return;
    };
    let door = giver_door(def.giver);
    let Some(home) = world.player(player).map(|rig| (rig.x, rig.y)) else {
        return;
    };
    park_rig(world, player, door.x, door.y);
    let mut inputs = TickInputs::default();
    inputs.players.insert(
        player,
        PlayerTickInput {
            controls: None,
            actions: vec![TickAction {
                sequence: InputSequence::new(0),
                command: PlayerCommand::Service(SessionServiceRequest::AcceptQuest { quest, crew }),
            }],
        },
    );
    let tick = world.simulation_tick();
    let _output = world.simulate_tick(tick, &inputs);
    if let Some(rig) = world.player_mut(player) {
        (rig.x, rig.y) = home;
    }
}

fn accept_for(world: &mut WorldState, player: PlayerId, quest: QuestId) {
    accept_quest_as(world, player, quest, false);
}

fn accept_crew(world: &mut WorldState, player: PlayerId, quest: QuestId) {
    accept_quest_as(world, player, quest, true);
}

/// Marks every stage of `player`'s `quest` done (ready to turn in).
fn finish_quest(world: &mut WorldState, player: PlayerId, quest: QuestId) {
    let stages = quest.def().map_or(0, |def| def.stages.len());
    if let Some(rig) = world.player_mut(player)
        && let Some(active) = rig
            .quests
            .active
            .iter_mut()
            .find(|active| active.quest == quest)
    {
        for objective in &mut active.objectives {
            objective.done = true;
        }
        active.stage = u8::try_from(stages).unwrap_or(u8::MAX);
        active.ready = true;
    }
}

/// The marked spots of `quest`'s first spotted objective.
fn quest_spots(world: &WorldState, player: PlayerId, quest: QuestId) -> Vec<TilePosition> {
    world
        .visible_quests(player)
        .into_iter()
        .find(|active| active.quest == quest)
        .and_then(|active| {
            active
                .objectives
                .iter()
                .find(|objective| !objective.spots.is_empty())
                .map(|objective| objective.spots.clone())
        })
        .expect("the quest bound its marked spots")
}

/// Parks the rig on the surface at `zone`'s door.
fn park_at(world: &mut WorldState, player: PlayerId, zone: SurfaceZone) {
    let tile_x = SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == zone)
        .map_or(97, |building| building.tile_x + 2);
    park_rig(world, player, tile_x, 4);
}

/// The row `offset` rows below the top of `band` in this world (negative: above it).
const fn band_row(world: &WorldState, band: Band, offset: i32) -> i32 {
    fatherload_simulation::worldgen::strata::band_layout_rows(world.world_settings(), band).0
        + offset
}

/// Puts the rig centred on tile `(x, y)` at rest, clearing that tile.
fn place_rig(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    carve(world, x..x + 1, y..y + 1);
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = tile_centre(y);
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
    }
}

/// World pixel at the centre of tile index `index`.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn tile_centre(index: i32) -> f32 {
    (index as f32 + 0.5) * TILE_SIZE
}

/// A rig standing on the surface (sky row `y`) above tile column `x`.
fn park_rig(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    place_rig(world, player, x, y);
}

/// A rig dug in at tile `(x, y)` underground: the shaft above it is open and explored, and the
/// tile below is solid.
fn sink_rig(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    carve(world, x..x + 1, 5..y + 1);
    let below = TilePosition { x, y: y + 1 };
    if !world.terrain().is_solid_at(below) {
        world.terrain_mut().set_kind(below, TileKind::Stone);
    }
    place_rig(world, player, x, y);
    reveal(world, player, x - 4..x + 5, 4..y + 4);
}

/// A long, explored tunnel two tiles tall at rows 28..30 (floor of stone below), with `player`
/// resting at its west end: room to reach top speed.
fn speed_tunnel(world: &mut WorldState, player: PlayerId) {
    carve(world, 20..220, 28..30);
    for x in 20..220 {
        world
            .terrain_mut()
            .set_kind(TilePosition { x, y: 30 }, TileKind::Stone);
    }
    place_rig(world, player, 24, 29);
    reveal(world, player, 14..226, 22..36);
}

fn carve(world: &mut WorldState, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) {
    for y in ys {
        for x in xs.clone() {
            world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
}

fn reveal(
    world: &mut WorldState,
    player: PlayerId,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
) {
    let tiles = ys
        .flat_map(|y| xs.clone().map(move |x| TilePosition { x, y }))
        .filter(|tile| world.terrain().tile(*tile).is_some())
        .collect();
    world.apply_discovery(&Discovery {
        player_id: player,
        tiles,
    });
}

/// Columns scenes search for places: a bounded world's columns (less a margin), or the classic
/// span around the town in an endless world.
const fn scene_columns(terrain: &Terrain) -> std::ops::Range<i32> {
    if terrain.is_streamed() {
        8..232
    } else {
        terrain.min_x() + 8..terrain.max_x() - 7
    }
}

/// The features of the scene columns ([`scene_columns`]) of `terrain`, as an index (a bounded
/// world's whole index).
fn terrain_home_features(terrain: &Terrain) -> fatherload_simulation::worldgen::FeatureIndex {
    if !terrain.is_streamed() {
        return terrain.features().clone();
    }
    let columns = scene_columns(terrain);
    fatherload_simulation::worldgen::FeatureIndex::from_features(terrain.features_in(
        &fatherload_simulation::worldgen::TileRect {
            min: TilePosition {
                x: columns.start,
                y: 0,
            },
            max: TilePosition {
                x: columns.end - 1,
                y: terrain.height() - 1,
            },
        },
        None,
    ))
}

fn home_features(world: &WorldState) -> fatherload_simulation::worldgen::FeatureIndex {
    terrain_home_features(world.terrain())
}

fn find_tile(
    world: &WorldState,
    ys: std::ops::Range<i32>,
    matches: impl Fn(TileKind) -> bool,
) -> Option<TilePosition> {
    let terrain = world.terrain();
    ys.flat_map(|y| scene_columns(terrain).map(move |x| TilePosition { x, y }))
        .find(|tile| terrain.tile(*tile).is_some_and(|tile| matches(tile.kind)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_capture_flags() {
        assert_eq!(parse_args(&args("--headless-host")), Ok(None));
        let options = parse_args(&args(
            "--screenshot-scenes out --size 1920x1080 --only title",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(options.dir, PathBuf::from("out"));
        assert_eq!(options.size, (1920, 1080));
        assert_eq!(options.only, Some(vec!["title".to_owned()]));
        assert_eq!(
            parse_args(&args("--screenshot-scenes out"))
                .unwrap()
                .unwrap()
                .size,
            DEFAULT_SIZE
        );
        assert!(parse_args(&args("--screenshot-scenes")).is_err());
        assert!(parse_args(&args("--screenshot-scenes out --size big")).is_err());
        assert!(parse_args(&args("--screenshot-scenes out --only nope")).is_err());
        assert_eq!(
            parse_args(&args("--screenshot-scenes out --frame-timing 240"))
                .unwrap()
                .unwrap()
                .timing_frames,
            240
        );
        assert!(parse_args(&args("--screenshot-scenes out --frame-timing 0")).is_err());
        assert!(
            parse_args(&args("--screenshot-scenes out --export-saves"))
                .unwrap()
                .unwrap()
                .export_saves
        );
    }

    #[test]
    fn scene_names_are_unique_file_names() {
        let mut names = std::collections::BTreeSet::new();
        for scene in all_scenes() {
            assert!(names.insert(scene.name), "duplicate scene {}", scene.name);
            assert!(
                scene
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{}",
                scene.name
            );
        }
    }

    #[test]
    fn every_scene_builds_headlessly() {
        let path = std::env::temp_dir().join(format!(
            "fatherload-scene-capture-test-{}.json",
            std::process::id()
        ));
        for scene in all_scenes() {
            let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
            let mut stage = SceneStage {
                game: &mut game,
                report: FrameReport::default(),
                gallery: false,
                ui_sheet: false,
                prompt_sheet: false,
                rig_sheet: None,
                touch_view: crate::input::touch::TouchView::default(),
            };
            (scene.setup)(&mut stage);
            let snapshot = game.snapshot(DEFAULT_SIZE);
            assert!(!snapshot.views.is_empty(), "{}", scene.name);
        }
    }
}
