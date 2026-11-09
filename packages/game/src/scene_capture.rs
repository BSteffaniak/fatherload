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
                drillgame_simulation::economy::UPGRADE_OFFER_COUNT - 1;
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
            shell.pending_test_mode = Some(drillgame_simulation::TestModeState {
                invulnerable: true,
                infinite_fuel: false,
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
                let (x, y) = world
                    .world_features()
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
                let (x, y) = world
                    .world_features()
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
                let x = x.clamp(30, world.terrain().width() - 30);
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
                    rig.quests.deepest_row = 45;
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
    }
}

/// Swaps in the first world (from the default seed on) that has a POI `wanted` accepts, and
/// returns that POI.
fn stage_poi(world: &mut WorldState, wanted: fn(PoiVariant) -> bool) -> WorldFeature {
    let (terrain, poi) = (0..256_u64)
        .find_map(|offset| {
            let terrain =
                Terrain::generate(&WorldSettings::new(DEFAULT_WORLD_SEED.wrapping_add(offset)));
            let poi = terrain
                .features()
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
    let pois: Vec<WorldFeature> = world.world_features().pois().copied().collect();
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
        log.deepest_row = 34;
        log.fragments.insert(BlueprintId::HaulerFrame, 1);
        log.fragments.insert(BlueprintId::DampenerCoil, 2);
    }
}

/// Has `player` take `quest` (crew mode when `crew`), standing at the giver's door for one tick
/// and going back where it was.
fn accept_quest_as(world: &mut WorldState, player: PlayerId, quest: QuestId, crew: bool) {
    use drillgame_simulation::{
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
    drillgame_simulation::worldgen::strata::band_layout_rows(world.world_settings(), band).0
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
