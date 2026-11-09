//! Menu screens drawn with the modal layout: pause, options, save / load slots, exit
//! confirmations, the rescue prompt, and every online screen (join forms, LAN list, lobby roster
//! and session notices). Rows carry icons, values and states so each choice reads at a glance.

use fatherload_simulation::terrain::TILE_SIZE;
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::{
        sprite::SpriteDef,
        sprites::{icons, menu_icons},
    },
    prompts::{self, Hint, hint},
    screen::UiFrame,
};
use crate::{
    input::{
        devices::InputDevice,
        prompts::{Prompt, inline_text},
    },
    runtime::OnlineRole,
    save::{save_slot_count, save_slot_metadata},
    shell::{
        JOIN_MENU, LobbyAction, NoticeAction, OnlineScreen, PauseOption, SetupPurpose, ShellState,
        WARP_BANDS,
    },
};
use fatherload_simulation::{
    TestModeState,
    worldgen::{HazardIntensity, ResourceRichness, strata::band_solid_top_row},
};

/// Select / confirm / back to the previous screen.
const NAVIGATE_BACK: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "back"),
];
/// Select / confirm / back to the title.
const NAVIGATE_TITLE: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "title"),
];
/// Select / confirm / close.
const NAVIGATE_CLOSE: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "close"),
];
/// The LAN browser.
const LAN_PROMPTS: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "join"),
    hint(Prompt::Back, "back"),
];
/// The lobby of a session already playing.
const LOBBY_IN_GAME: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "back to the game"),
];
/// The address form (typed on a keyboard).
const TYPE_FORM: &[Hint] = &[
    hint(Prompt::Confirm, "join"),
    hint(Prompt::Delete, "delete"),
    hint(Prompt::Back, "back"),
];
/// The descriptor form (pasted on a keyboard).
const PASTE_FORM: &[Hint] = &[
    hint(Prompt::Paste, "paste"),
    hint(Prompt::Confirm, "join"),
    hint(Prompt::Delete, "delete"),
    hint(Prompt::Back, "back"),
];

/// The options volume row's detail: how to adjust it on the menu device.
fn adjust_detail() -> String {
    let device = prompts::menu_device();
    let adjust = inline_text(Prompt::Adjust, device).unwrap_or_default();
    inline_text(Prompt::Volume, device).map_or_else(
        || format!("{adjust} adjust"),
        |volume| format!("{adjust} or {volume} adjust"),
    )
}

/// The options fullscreen row's detail: the hotkey when the menu device has one.
fn fullscreen_detail() -> String {
    inline_text(Prompt::Fullscreen, prompts::menu_device()).map_or_else(
        || "Borderless window".to_owned(),
        |key| format!("Borderless, {key} anywhere"),
    )
}

/// The descriptor field's placeholder (pasting needs a keyboard).
fn paste_placeholder() -> String {
    let key = inline_text(Prompt::Paste, InputDevice::KEYBOARD).unwrap_or_default();
    format!("Paste the host's descriptor with {key}")
}

/// A menu row: icon, label, optional detail and right-aligned value.
struct Entry {
    icon: &'static SpriteDef,
    label: String,
    detail: String,
    value: String,
    value_color: Color,
    disabled: bool,
}

impl Entry {
    fn new(icon: &'static SpriteDef, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            detail: String::new(),
            value: String::new(),
            value_color: BONE,
            disabled: false,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    fn value(mut self, value: impl Into<String>, color: Color) -> Self {
        self.value = value.into();
        self.value_color = color;
        self
    }

    const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Section items for `entries` with `selected` highlighted.
fn menu_items(entries: Vec<Entry>, selected: usize) -> Vec<SectionItem> {
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            SectionItem::item(ItemEntry {
                icon: Some(entry.icon),
                label: entry.label,
                detail: entry.detail,
                value: entry.value,
                value_color: entry.value_color,
                coin: false,
                selected: index == selected,
                disabled: entry.disabled,
            })
        })
        .collect()
}

const fn pause_icon(option: PauseOption) -> &'static SpriteDef {
    match option {
        PauseOption::Resume => &menu_icons::PLAY,
        PauseOption::Save => &menu_icons::DISK,
        PauseOption::Load => &menu_icons::FOLDER,
        PauseOption::OpenOnline => &menu_icons::GLOBE,
        PauseOption::SessionInfo => &menu_icons::SIGNAL,
        PauseOption::EndSession => &menu_icons::OFFLINE,
        PauseOption::LeaveSession | PauseOption::ExitToDesktop => &menu_icons::DOOR,
        PauseOption::Options => &menu_icons::GEAR,
        PauseOption::CopySeed => &menu_icons::KEY,
        PauseOption::WarpToLayer => &menu_icons::ROCKET,
        PauseOption::ToggleInvulnerable => &icons::HULL,
        PauseOption::ToggleInfiniteFuel => &icons::FUEL,
    }
}

/// `ON` / `OFF` for a toggle row.
const fn on_off(on: bool) -> (&'static str, Color) {
    if on { ("ON", LIME) } else { ("OFF", FOG) }
}

/// A pause menu row, with the current value of a test-mode toggle.
fn pause_entry(option: PauseOption, test_mode: Option<TestModeState>) -> Entry {
    let entry = Entry::new(pause_icon(option), option.label());
    let toggle = match option {
        PauseOption::ToggleInvulnerable => test_mode.map(|state| state.invulnerable),
        PauseOption::ToggleInfiniteFuel => test_mode.map(|state| state.infinite_fuel),
        _ => None,
    };
    match toggle {
        Some(on) => {
            let (value, color) = on_off(on);
            entry.value(value, color)
        }
        None => entry,
    }
}

pub(super) fn draw_pause(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let test_mode = ui.shell.shown_test_mode(ui.snapshot.test_mode);
    let entries = PauseOption::for_session(ui.shell.online.role, test_mode.is_some())
        .into_iter()
        .map(|option| pause_entry(option, test_mode))
        .collect();
    let player = ui.player;
    let mut rig = vec![
        SectionItem::icon_stat(
            &icons::DEPTH,
            "Depth",
            format!("{} m", (player.y / TILE_SIZE).max(0.0) as i32),
            SKY_LIGHT,
        ),
        SectionItem::icon_stat(&icons::CREDITS, "Credits", player.credits.to_string(), GOLD),
        SectionItem::icon_stat(
            &icons::HULL,
            "Hull",
            format!("{:.0}/{:.0}", player.hull, player.max_hull()),
            if player.hull <= player.max_hull() * 0.35 {
                RED
            } else {
                LIME
            },
        ),
        SectionItem::icon_stat(
            &icons::FUEL,
            "Fuel",
            format!("{:.0}/{:.0}", player.fuel, player.fuel_capacity),
            if player.fuel <= player.fuel_capacity * 0.2 {
                RED
            } else {
                AMBER
            },
        ),
    ];
    rig.push(SectionItem::icon_stat(
        &menu_icons::KEY,
        "World seed",
        ui.snapshot.terrain.settings().seed_text(),
        SKY_LIGHT,
    ));
    if ui.snapshot.won_game {
        rig.push(SectionItem::text(
            "Star Core secured. Deep Claim operations are open.",
        ));
    }
    // In split-screen the seat that paused drives this menu (and the rig shown is theirs).
    let mut subtitle = if ui.snapshot.views.len() > 1 {
        format!("Player {} paused the game.", ui.shell.paused_by + 1)
    } else {
        String::new()
    };
    if test_mode.is_some() {
        subtitle = format!("TEST MODE: records off. {subtitle}");
    }
    UiLayout::screen(draw).modal_with_render_plan(
        if test_mode.is_some() {
            "Paused (Test Mode)"
        } else {
            "Paused"
        },
        &subtitle,
        &ModalContent::new(vec![
            Section::new(
                "Menu",
                SKY_LIGHT,
                menu_items(entries, ui.shell.selected_pause_item),
            ),
            Section::new("Rig", GOLD, rig),
        ])
        .compact()
        .prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, "confirm"),
            hint(Prompt::Back, "resume"),
        ]),
    );
}

pub(super) fn draw_options(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let volume = (ui.shell.master_volume * 100.0).round() as i32;
    let entries = vec![
        Entry::new(&menu_icons::SPEAKER, "Master volume")
            .detail(adjust_detail())
            .value(format!("\u{2190} {volume}% \u{2192}"), GOLD),
        Entry::new(&menu_icons::SCREEN, "Fullscreen")
            .detail(fullscreen_detail())
            .value(
                if ui.shell.fullscreen { "ON" } else { "OFF" },
                if ui.shell.fullscreen { LIME } else { FOG },
            ),
        Entry::new(&menu_icons::BACK, "Back"),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "Options",
        "",
        &ModalContent::new(vec![
            Section::new(
                "Settings",
                SKY_LIGHT,
                menu_items(entries, ui.shell.selected_menu_item),
            ),
            Section::new(
                "Volume",
                FOG,
                vec![SectionItem::meter(
                    "Level",
                    volume as f32,
                    100.0,
                    SKY_LIGHT,
                    SKY_LIGHT,
                )],
            ),
        ])
        .compact()
        .prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Adjust, "adjust"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

pub(super) fn draw_slots(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>, saving: bool) {
    let entries = (0..save_slot_count())
        .map(|slot| {
            let label = format!("Slot {}", slot + 1);
            save_slot_metadata(slot).map_or_else(
                || {
                    let entry = Entry::new(&menu_icons::DISK, label.clone()).detail("Empty slot");
                    if saving {
                        entry.value("NEW", LIME)
                    } else {
                        entry.value("EMPTY", FOG).disabled(true)
                    }
                },
                |meta| {
                    let entry = Entry::new(&menu_icons::DISK, label.clone()).detail(format!(
                        "{}{}m deep | {} cr | {} contracts | {:.0} min",
                        if meta.test_mode { "TEST MODE | " } else { "" },
                        meta.depth,
                        meta.credits,
                        meta.contracts_completed,
                        (meta.play_seconds / 60.0).floor()
                    ));
                    if saving {
                        entry.value("OVERWRITE", ORANGE)
                    } else {
                        entry.value("LOAD", LIME)
                    }
                },
            )
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        if saving { "Save Game" } else { "Load Game" },
        "",
        &ModalContent::new(vec![Section::new(
            "Slots",
            SKY_LIGHT,
            menu_items(entries, ui.shell.selected_menu_item),
        )])
        .prompts(NAVIGATE_CLOSE),
    );
}

/// The world setup dialog while typing a seed.
const SEED_FORM: &[Hint] = &[
    hint(Prompt::Paste, "paste"),
    hint(Prompt::Confirm, "keep"),
    hint(Prompt::Delete, "delete"),
    hint(Prompt::Back, "cancel"),
];
/// The world setup dialog.
const SETUP_PROMPTS: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Adjust, "change"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "back"),
];

/// `← value →` for a row whose value cycles with left/right.
fn cycler(value: &str) -> String {
    format!("\u{2190} {value} \u{2192}")
}

pub(super) fn draw_world_setup(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(setup) = ui.shell.world_setup.as_ref() else {
        return;
    };
    let settings = setup.settings;
    let (subtitle, start_label) = match setup.purpose {
        SetupPurpose::Local { seats: 1 } => (
            "Same seed and settings, same world on any machine.".to_owned(),
            "Start Digging",
        ),
        SetupPurpose::Local { seats } => (
            format!("Local split-screen for {seats} players."),
            "Start Split-Screen",
        ),
        SetupPurpose::HostOnline => (
            "You host: everyone who joins digs this world.".to_owned(),
            "Host Game",
        ),
    };
    let seed_value = if setup.typing() {
        format!("{}_", setup.seed_text())
    } else {
        setup.seed_text()
    };
    let paste = inline_text(Prompt::Paste, InputDevice::KEYBOARD).unwrap_or_default();
    let entries = vec![
        Entry::new(&menu_icons::KEY, "Seed")
            .detail(if setup.typing() {
                "Hex digits or any words".to_owned()
            } else {
                format!("Type, or paste with {paste}")
            })
            .value(seed_value, if setup.typing() { LIME } else { GOLD }),
        Entry::new(&menu_icons::DICE, "Re-roll Seed").detail("Draw a new random world"),
        Entry::new(&menu_icons::GLOBE, "World Size")
            .detail(format!("{} tiles wide", settings.width_tiles()))
            .value(cycler(settings.size.name()), SKY_LIGHT),
        Entry::new(&icons::CREDITS, "Resources")
            .detail(match settings.richness {
                ResourceRichness::Lean => "Sparse ore, every vein counts",
                ResourceRichness::Standard => "Balanced veins and pods",
                ResourceRichness::Rich => "Fat veins, more motherlodes",
            })
            .value(cycler(settings.richness.name()), GOLD),
        Entry::new(&icons::WARNING, "Hazards")
            .detail(match settings.hazards {
                HazardIntensity::Calm => "Few gas and lava pockets",
                HazardIntensity::Standard => "Clustered pockets by layer",
                HazardIntensity::Brutal => "Dense pockets, deep trouble",
            })
            .value(
                cycler(settings.hazards.name()),
                match settings.hazards {
                    HazardIntensity::Calm => LIME,
                    HazardIntensity::Standard => AMBER,
                    HazardIntensity::Brutal => RED,
                },
            ),
        Entry::new(&menu_icons::ROCKET, "Test Mode")
            .detail(if setup.test_mode {
                "Maxed rig, warps, god toggles; no records"
            } else {
                "Normal game"
            })
            .value(
                cycler(if setup.test_mode { "ON" } else { "OFF" }),
                if setup.test_mode { ORANGE } else { FOG },
            ),
        Entry::new(&menu_icons::PLAY, start_label),
        Entry::new(&menu_icons::BACK, "Back"),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "New World",
        &subtitle,
        &ModalContent::new(vec![Section::new(
            "World",
            SKY_LIGHT,
            menu_items(entries, setup.selected),
        )])
        .compact()
        .prompts(if setup.typing() {
            SEED_FORM
        } else {
            SETUP_PROMPTS
        }),
    );
}

/// Test mode's band list: warp the rig to the top of any layer.
pub(super) fn draw_warp_layers(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let settings = ui.snapshot.terrain.settings();
    let here = ui
        .snapshot
        .terrain
        .band_at(ui.player.tile_position(TILE_SIZE));
    let entries = WARP_BANDS
        .iter()
        .map(|band| {
            let top = band_solid_top_row(settings, *band, ui.player.tile_position(TILE_SIZE).x);
            let entry = Entry::new(&icons::DEPTH, band.name()).detail(format!(
                "Top at about {top} m | drill tier {}",
                band.required_drill_strength()
            ));
            if *band == here {
                entry.value("HERE", LIME)
            } else {
                entry
            }
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "Warp to Layer",
        "Test mode: drops your rig into a safe pocket at the top of the layer.",
        &ModalContent::new(vec![Section::new(
            "Layers",
            ORANGE,
            menu_items(entries, ui.shell.selected_menu_item),
        )])
        .prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, "warp"),
            hint(Prompt::Back, "back"),
        ]),
    );
}

pub(super) fn draw_exit_confirm(draw: &mut RaylibDrawHandle<'_>) {
    UiLayout::screen(draw).modal_with_render_plan(
        "Exit to Desktop?",
        "",
        &ModalContent::new(vec![Section::new(
            "Confirm",
            GOLD,
            vec![SectionItem::icon_stat(
                &menu_icons::DOOR,
                "Close Fatherload now",
                "",
                BONE,
            )],
        )])
        .compact()
        .prompts(&[
            hint(Prompt::Confirm, "exit"),
            hint(Prompt::Back, "return to the game"),
        ]),
    );
}

pub(super) fn draw_unsaved_exit_confirm(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let entries = vec![
        Entry::new(&menu_icons::DISK, "Save and exit").detail("Writes the session save first"),
        Entry::new(&menu_icons::DOOR, "Exit without saving")
            .detail("Progress since the last save is lost")
            .value("LOSE", RED),
        Entry::new(&menu_icons::BACK, "Cancel").detail("Back to the game"),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        "Unsaved Progress",
        "",
        &ModalContent::new(vec![Section::new(
            "Before you go",
            ORANGE,
            menu_items(entries, ui.shell.selected_menu_item),
        )])
        .compact()
        .prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, "confirm"),
            hint(Prompt::Back, "cancel"),
        ]),
    );
}

/// The emergency prompt for a stranded rig (out of fuel or hull).
pub(super) fn draw_rescue(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let (icon, headline, body) = match ui.view.failure {
        Some(failure) if failure.disabled => (
            &icons::HULL,
            "Hull breached",
            "The rig's hull gave out. A rescue crew can tow it back to town.".to_owned(),
        ),
        Some(_) => (
            &icons::FUEL,
            "Out of fuel",
            "The tank is dry. A rescue crew can tow the rig back to town.".to_owned(),
        ),
        None => (&icons::WARNING, "Stranded", ui.seat.message.clone()),
    };
    let action = SectionItem::item(ItemEntry {
        icon: Some(&menu_icons::FLAG),
        label: "Call the rescue crew".to_owned(),
        detail: crate::input::prompts::inline_text(Prompt::Confirm, prompts::menu_device())
            .map_or_else(String::new, |key| format!("Press {key}")),
        value: "FEE".to_owned(),
        value_color: GOLD,
        coin: false,
        selected: true,
        disabled: false,
    });
    UiLayout::screen(draw).modal_with_render_plan(
        "Emergency",
        "",
        &ModalContent::new(vec![
            Section::new(
                headline,
                RED,
                vec![
                    SectionItem::text(body),
                    SectionItem::icon_stat(
                        icon,
                        "Rescues called",
                        ui.view.rescue_count.to_string(),
                        MIST,
                    ),
                ],
            ),
            Section::new("Action", GOLD, vec![action]),
        ])
        .warning()
        .compact()
        .prompts(&[
            hint(Prompt::Confirm, "call rescue"),
            hint(Prompt::Pause, "pause menu"),
        ]),
    );
}

/// Lobby roster rows shown before the rest condense into a count, so any player count fits
/// (plan §15).
const MAX_ROSTER_ROWS: usize = 10;

const fn lobby_icon(action: LobbyAction) -> &'static SpriteDef {
    match action {
        LobbyAction::Start | LobbyAction::BackToGame => &menu_icons::PLAY,
        LobbyAction::CopyDescriptor => &menu_icons::KEY,
        LobbyAction::ToggleReady => &menu_icons::READY,
        LobbyAction::Leave => &menu_icons::DOOR,
    }
}

const fn notice_icon(action: NoticeAction) -> &'static SpriteDef {
    match action {
        NoticeAction::Reconnect => &menu_icons::SIGNAL,
        NoticeAction::BackToTitle => &menu_icons::BACK,
    }
}

/// Online screens: join forms, LAN list, lobby roster and session notices. Drawn without a
/// world view (a joining client has no rig until the host's welcome arrives).
pub(super) fn draw_online(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState) {
    let online = &shell.online;
    let (title, help, sections) = match online.screen {
        OnlineScreen::JoinMenu => {
            let icons: [&SpriteDef; 4] = [
                &menu_icons::GLOBE,
                &menu_icons::KEY,
                &menu_icons::SIGNAL,
                &menu_icons::BACK,
            ];
            let details = [
                "Type the host's ip:port",
                "Pins the host's certificate",
                "Sessions advertised on this network",
                "",
            ];
            let entries = JOIN_MENU
                .iter()
                .zip(icons)
                .zip(details)
                .map(|((label, icon), detail)| Entry::new(icon, *label).detail(detail))
                .collect();
            (
                "Join Online Game".to_owned(),
                NAVIGATE_BACK,
                vec![Section::new(
                    "Join",
                    SKY_LIGHT,
                    menu_items(entries, online.selected),
                )],
            )
        }
        OnlineScreen::JoinAddress => (
            "Join by Address".to_owned(),
            TYPE_FORM,
            vec![Section::new(
                "Host address",
                SKY_LIGHT,
                vec![
                    SectionItem::field(online.text.clone(), "e.g. 192.168.1.20:47777"),
                    SectionItem::text(
                        "Direct-IP joins are encrypted but do not authenticate the host; use a descriptor to pin its certificate.",
                    ),
                ],
            )],
        ),
        OnlineScreen::JoinDescriptor => (
            "Join by Descriptor".to_owned(),
            PASTE_FORM,
            vec![Section::new(
                "Descriptor",
                SKY_LIGHT,
                vec![
                    SectionItem::field(
                        online.text.chars().take(96).collect::<String>(),
                        paste_placeholder(),
                    ),
                    SectionItem::text(if online.text.is_empty() {
                        "The host copies it from its lobby (\"Copy descriptor to clipboard\")."
                            .to_owned()
                    } else {
                        format!("{} characters pasted.", online.text.len())
                    }),
                ],
            )],
        ),
        OnlineScreen::LanBrowser => lan_screen(online),
        OnlineScreen::Lobby => lobby_screen(shell),
        OnlineScreen::Notice => {
            let entries = online
                .notice_options()
                .into_iter()
                .map(|action| Entry::new(notice_icon(action), action.label()))
                .collect();
            (
                "Online Session".to_owned(),
                NAVIGATE_TITLE,
                vec![
                    Section::new(
                        "Session over",
                        ORANGE,
                        vec![SectionItem::text(online.notice.clone())],
                    ),
                    Section::new("Next", SKY_LIGHT, menu_items(entries, online.selected)),
                ],
            )
        }
    };
    let subtitle = if online.screen == OnlineScreen::JoinAddress {
        "Type the host's ip:port."
    } else {
        ""
    };
    UiLayout::screen(draw).modal_with_render_plan(
        &title,
        subtitle,
        &ModalContent::new(sections).prompts(help),
    );
}

/// The LAN browser: discovered sessions, then refresh and back.
fn lan_screen(online: &crate::shell::OnlineShell) -> (String, &'static [Hint], Vec<Section>) {
    let mut entries: Vec<Entry> = online
        .lan_games
        .iter()
        .map(|game| {
            Entry::new(&menu_icons::SIGNAL, game.instance_name.clone())
                .detail(game.game_addr.to_string())
                .value("JOIN", LIME)
        })
        .collect();
    entries.push(Entry::new(&icons::SCANNER, "Refresh").detail("Search the network again"));
    entries.push(Entry::new(&menu_icons::BACK, "Back"));
    let mut items = Vec::new();
    if !online.lan_status.is_empty() {
        items.push(SectionItem::text(online.lan_status.clone()));
    }
    items.extend(menu_items(entries, online.selected));
    (
        "LAN Games".to_owned(),
        LAN_PROMPTS,
        vec![Section::new(
            format!("Discovered sessions ({})", online.lan_games.len()),
            SKY_LIGHT,
            items,
        )],
    )
}

/// One row per lobby player with a state chip, condensed past [`MAX_ROSTER_ROWS`].
fn roster_items(lobby: &crate::runtime::LobbyInfo) -> Vec<SectionItem> {
    if lobby.roster.is_empty() {
        vec![SectionItem::text("Waiting for the roster...")]
    } else {
        let hidden = lobby.roster.len().saturating_sub(MAX_ROSTER_ROWS);
        let mut rows: Vec<SectionItem> = lobby
            .roster
            .iter()
            .take(MAX_ROSTER_ROWS)
            .map(|entry| {
                let mut name = entry.name.clone();
                if lobby.local_player == Some(entry.player) {
                    name.push_str(" (you)");
                }
                let (icon, state, color): (&'static SpriteDef, &str, Color) = if !entry.connected {
                    (&menu_icons::OFFLINE, "OFFLINE", ORANGE)
                } else if entry.local {
                    (&menu_icons::CROWN, "HOST", GOLD)
                } else if entry.ready {
                    (&menu_icons::READY, "READY", LIME)
                } else {
                    (&menu_icons::WAITING, "WAITING", FOG)
                };
                SectionItem::icon_stat(icon, name, state, color)
            })
            .collect();
        if hidden > 0 {
            rows.push(SectionItem::text(format!("... and {hidden} more")));
        }
        rows
    }
}

/// The lobby: session facts, the roster with ready states, and the lobby actions.
fn lobby_screen(shell: &ShellState) -> (String, &'static [Hint], Vec<Section>) {
    let online = &shell.online;
    let host = online.role == Some(OnlineRole::Host);
    let lobby = online.lobby.clone().unwrap_or_default();
    let mut session = vec![SectionItem::text(lobby.status.clone())];
    if let Some(addr) = lobby.addr {
        session.push(SectionItem::icon_stat(
            &menu_icons::GLOBE,
            "Address",
            addr.to_string(),
            GOLD,
        ));
    }
    if host {
        session.push(SectionItem::icon_stat(
            &menu_icons::SIGNAL,
            "LAN discovery",
            if lobby.lan_published {
                "advertised"
            } else {
                "off"
            },
            if lobby.lan_published { LIME } else { FOG },
        ));
        session.push(SectionItem::icon_stat(
            &menu_icons::KEY,
            "Descriptor",
            if lobby.descriptor.is_some() {
                "ready to copy"
            } else {
                "pending"
            },
            if lobby.descriptor.is_some() {
                LIME
            } else {
                FOG
            },
        ));
    }
    if !shell.message.is_empty() {
        session.push(SectionItem::text(shell.message.clone()));
    }
    let ready_count = lobby
        .roster
        .iter()
        .filter(|entry| entry.connected && (entry.local || entry.ready))
        .count();
    let players = roster_items(&lobby);
    let entries = online
        .lobby_options()
        .into_iter()
        .map(|action| {
            let entry = Entry::new(lobby_icon(action), action.label(host));
            match action {
                LobbyAction::ToggleReady => entry.value(
                    if online.ready { "READY" } else { "NOT READY" },
                    if online.ready { LIME } else { FOG },
                ),
                LobbyAction::Start => {
                    entry.value(format!("{ready_count}/{} ready", lobby.roster.len()), LIME)
                }
                _ => entry,
            }
        })
        .collect();
    (
        if host {
            "Hosting Online Game".to_owned()
        } else {
            "Online Lobby".to_owned()
        },
        if lobby.started {
            LOBBY_IN_GAME
        } else {
            &NAVIGATE_CLOSE[..2]
        },
        vec![
            Section::new("Session", SKY_LIGHT, session),
            Section::new(format!("Players ({})", lobby.roster.len()), GOLD, players),
            Section::new("Actions", SKY_LIGHT, menu_items(entries, online.selected)),
        ],
    )
}
