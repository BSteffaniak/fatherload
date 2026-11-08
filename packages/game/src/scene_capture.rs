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
    input::{
        PlayerInput,
        devices::{InputDevice, KeyboardLayout, PadFamily},
    },
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
            ui_sheet: false,
            prompt_sheet: false,
        };
        (scene.setup)(&mut stage);
        let (report, gallery, ui_sheet, prompt_sheet) = (
            stage.report,
            stage.gallery,
            stage.ui_sheet,
            stage.prompt_sheet,
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
            renderer.sync(&mut raylib, &thread, &snapshot, &frame_report);
            let mut draw = raylib.begin_drawing(&thread);
            if gallery {
                renderer.render_art_gallery(&mut draw);
            } else if ui_sheet {
                renderer.render_ui_sheet(&mut draw);
            } else if prompt_sheet {
                renderer.render_prompt_sheet(&mut draw);
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
    /// Draw the UI icon and interior prop sheet instead of the game.
    ui_sheet: bool,
    /// Draw the button-prompt sheet instead of the game.
    prompt_sheet: bool,
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

    /// Shows every seat's prompts for `devices` (in seat order).
    fn use_devices(&mut self, devices: &[InputDevice]) {
        self.game.shell.seat_devices = devices.to_vec();
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

/// A scene inside `$zone` with no panel open, the rig standing between the door and the counter
/// so the whole room is visible.
macro_rules! room_scene {
    ($name:literal, $summary:literal, $zone:ident) => {
        Scene {
            name: $name,
            summary: $summary,
            setup: |stage| {
                enter_interior(stage, SurfaceZone::$zone, None);
                stage.shell().interior_x = interior_service_x(SurfaceZone::$zone) - 150.0;
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
            "Bought Carbide Drill for 420 credits.".clone_into(&mut stage.shell().message);
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
                    |(index, name)| drillgame_quic_transport::lan::LanDiscoveredGame {
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
                carve(world, x - 2..x + 3, y - 3..y + 1);
                reveal(world, player, x - 14..x + 15, y - 8..y + 9);
            });
            stage.settle(10);
            stage.hold(
                30,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "deep_crystal_faults",
        summary: "The Crystal Faults stratum far below the surface",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                sink_rig(world, player, 120, 100);
                carve(world, 108..133, 100..101);
                reveal(world, player, 98..143, 90..112);
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
                sink_rig(world, player, 120, 205);
                carve(world, 108..133, 205..206);
                reveal(world, player, 98..143, 195..218);
            });
            stage.settle(10);
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
            stage.shell().modal = Some(ModalScreen::Help);
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
            stage.shell().selected_menu_item = 4;
        },
    },
    Scene {
        name: "pad_interior_depot_last_row",
        summary: "Ore depot counter on its last row with Xbox controller prompts",
        setup: |stage| {
            enter_interior(stage, SurfaceZone::Depot, Some(ModalScreen::Depot));
            stage.shell().selected_menu_item = 4;
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
            stage.shell().modal = Some(ModalScreen::Help);
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
                ui_sheet: false,
                prompt_sheet: false,
            };
            (scene.setup)(&mut stage);
            let snapshot = game.snapshot(DEFAULT_SIZE);
            assert!(!snapshot.views.is_empty(), "{}", scene.name);
        }
    }
}
