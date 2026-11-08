//! `drillgame --screenshot-scenes <dir> [--size WxH] [--only a,b] [--list]`: renders scripted game
//! states to PNG files without user input, then exits.
//!
//! Each [`Scene`] builds a deterministic state through the same [`LocalGame`] the app runs (fixed
//! frame time, fixed world seed), renders it with the real [`GameRenderer`] for a few frames and
//! saves `<dir>/<scene name>.png`. The window opens briefly while capturing.
//!
//! Adding a scene: write a `fn(&mut SceneStage)` that arranges the state and add a [`Scene`] to
//! [`SCENES`]. `SceneStage` starts on the title screen; `play` enters a staged world, `hold` runs
//! frames with held input, and `shell` exposes the menu state to open screens directly.

use std::path::{Path, PathBuf};

use drillgame_netcode::{LobbyEntry, PlayerId as NetPlayerId};
use drillgame_simulation::{
    Discovery, PlayerId, WorldState,
    economy::SurfaceZone,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};
use raylib::ffi;

use crate::{
    input::PlayerInput,
    local_game::{FrameInput, FrameReport, LocalGame, SaveTarget},
    rendering::GameRenderer,
    runtime::{LobbyInfo, LocalSeat, OnlineRole},
    save::SettingsFile,
    shell::{ModalScreen, OnlineScreen, RunMode, ShellState, interior_service_x},
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
            other => return Err(format!("unknown scene-capture argument {other}")),
        }
    }
    if let Some(only) = &options.only
        && let Some(unknown) = only
            .iter()
            .find(|name| SCENES.iter().all(|scene| scene.name != name.as_str()))
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
pub fn run(options: &CaptureOptions) -> Result<(), String> {
    let scenes: Vec<&Scene> = SCENES
        .iter()
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
    let (width, height) = options.size;
    let (mut raylib, thread) = raylib::init()
        .size(width, height)
        .highdpi()
        .title("Drillgame scene capture")
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
        };
        (scene.setup)(&mut stage);
        let (report, gallery) = (stage.report, stage.gallery);
        let renderer = renderer
            .get_or_insert_with(|| GameRenderer::new(&mut raylib, &thread, game.world().terrain()));
        let path = options.dir.join(format!("{}.png", scene.name));
        for frame in 0..RENDER_FRAMES {
            let screen = crate::window::DesktopWindow::screen(&raylib);
            let snapshot = game.snapshot(screen);
            let frame_report = if frame == 0 {
                report.clone()
            } else {
                FrameReport::default()
            };
            renderer.sync(&mut raylib, &thread, &snapshot, &frame_report);
            let mut draw = raylib.begin_drawing(&thread);
            if gallery {
                renderer.render_art_gallery(&mut draw);
            } else {
                renderer.render(&mut draw, &game.shell, &snapshot);
            }
            if frame + 1 == RENDER_FRAMES {
                save_framebuffer(&draw, &thread, &path)?;
            }
        }
        println!("captured {}", path.display());
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

    /// Runs `frames` idle frames (the world ticks while playing).
    fn settle(&mut self, frames: usize) {
        self.hold(frames, PlayerInput::default());
    }

    const fn shell(&mut self) -> &mut ShellState {
        &mut self.game.shell
    }

    fn absorb(&mut self, report: FrameReport) {
        self.report.world_replaced |= report.world_replaced;
        self.report.changed_tiles.extend(report.changed_tiles);
    }
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

/// Every scene, in capture order.
pub const SCENES: &[Scene] = &[
    Scene {
        name: "art_gallery",
        summary: "Art reference sheet: palette, UI kit widgets, icons and sprites",
        setup: |stage| stage.gallery = true,
    },
    Scene {
        name: "title",
        summary: "Title menu over the idle world",
        setup: |_| {},
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
                sink_rig(world, player, 120, 42);
                carve(world, 108..133, 42..43);
                reveal(world, player, 100..141, 33..53);
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
                let (x, y) = find_tile(world, 60..200, |kind| kind == TileKind::Lava)
                    .map_or((120, 95), |tile| (tile.x - 2, tile.y));
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
            stage.shell().interior_x = interior_service_x(SurfaceZone::Shop) - 120.0;
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
                sink_rig(world, player, 97, 66);
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
            stage.shell().modal = Some(ModalScreen::Inventory);
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
            stage.shell().modal = Some(ModalScreen::Map);
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
                    lobby_entry(1, "Ada (host)", true, true, true),
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
            });
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
                    // Terrain fog is drawn from the primary view's discoveries, so the first
                    // player must know this area too or the second view renders black.
                    reveal(world, seats[0].player, 104..121, 4..23);
                }
            });
            stage.settle(20);
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
    let shell = stage.shell();
    shell.run_mode = RunMode::Interior;
    shell.interior_zone = Some(zone);
    shell.interior_x = interior_service_x(zone);
    shell.interior_facing = 1.0;
    shell.modal = modal;
    shell.selected_menu_item = usize::from(modal == Some(ModalScreen::Depot));
}

fn lobby_entry(id: u32, name: &str, ready: bool, connected: bool, local: bool) -> LobbyEntry {
    LobbyEntry {
        player: NetPlayerId(id),
        name: name.to_owned(),
        ready,
        connected,
        local,
    }
}

fn load_cargo(rig: &mut drillgame_simulation::Player) {
    for (mineral, count) in [
        (MineralKind::Copper, 6),
        (MineralKind::Iron, 4),
        (MineralKind::Silver, 2),
        (MineralKind::Gold, 1),
    ] {
        rig.cargo.insert(mineral, count);
    }
}

/// Puts the rig centred on tile `(x, y)` at rest, clearing that tile.
fn place_rig(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    carve(world, x..x + 1, y..y + 1);
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = tile_centre(y);
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
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

fn find_tile(
    world: &WorldState,
    ys: std::ops::Range<i32>,
    matches: impl Fn(TileKind) -> bool,
) -> Option<TilePosition> {
    let terrain = world.terrain();
    ys.flat_map(|y| (8..terrain.width() - 8).map(move |x| TilePosition { x, y }))
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
    }

    #[test]
    fn scene_names_are_unique_file_names() {
        let mut names = std::collections::BTreeSet::new();
        for scene in SCENES {
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
            "drillgame-scene-capture-test-{}.json",
            std::process::id()
        ));
        for scene in SCENES {
            let mut game = LocalGame::new(SettingsFile::default(), SaveTarget::File(path.clone()));
            let mut stage = SceneStage {
                game: &mut game,
                report: FrameReport::default(),
                gallery: false,
            };
            (scene.setup)(&mut stage);
            let snapshot = game.snapshot(DEFAULT_SIZE);
            assert!(!snapshot.views.is_empty(), "{}", scene.name);
        }
    }
}
