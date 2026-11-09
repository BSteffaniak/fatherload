//! Quest UI: the quest desk tab at keepers' counters, the per-seat quest log, the tracked
//! objective on the HUD, quest markers on the minimap and mine map, and the fanfare banner.

use drillgame_simulation::{
    ActiveQuest, Player,
    quest::{
        BlueprintId, QuestDef, QuestGiver, QuestTier,
        describe::{QuestMarker, QuestMarkerKind, current_lines, seconds_left, twist_lines},
        rank_name, reputation_rank,
    },
    world::QuestAvailability,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::sprites::menu_icons,
    prompts::{Hint, hint},
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
    shell::{DeskRow, Fanfare, FanfareKind, SeatUi, has_quest_tab, tracked_quest},
};

const DESK_HELP: &[Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Tab, "services"),
    hint(Prompt::Back, "close"),
];

/// Hints for a counter that has a quest tab.
pub(super) fn with_quest_tab(ui: &UiFrame<'_, '_>, hints: &[Hint]) -> Vec<Hint> {
    let mut hints = hints.to_vec();
    if ui
        .seat
        .modal
        .is_some_and(|modal| has_quest_tab(ui.seat.interior_zone, modal))
    {
        let at = hints.len().saturating_sub(1);
        hints.insert(at, hint(Prompt::Tab, "quests"));
    }
    hints
}

/// The counter's subtitle when it has a quest tab: what waits on the desk.
pub(super) fn quest_tab_subtitle(ui: &UiFrame<'_, '_>) -> String {
    let Some(giver) = ui.seat.interior_zone.and_then(QuestGiver::at) else {
        return String::new();
    };
    if !ui
        .seat
        .modal
        .is_some_and(|modal| has_quest_tab(ui.seat.interior_zone, modal))
    {
        return String::new();
    }
    let ready = ui
        .view
        .quests
        .iter()
        .filter(|quest| quest.owner == ui.view.player_id && quest.ready)
        .filter(|quest| quest.def().is_some_and(|def| def.giver == giver))
        .count();
    if ready > 0 {
        format!(
            "{} has {ready} finished job(s) to pay out: see Quests.",
            giver.name()
        )
    } else {
        format!("{} has work for you: see Quests.", giver.name())
    }
}

/// Short tag for a tier, co-op flag and repeatability.
fn quest_kind(def: &QuestDef) -> String {
    let mut tags = vec![match def.tier {
        QuestTier::Shallow => "Shallow",
        QuestTier::Mid => "Mid",
        QuestTier::Deep => "Deep",
    }];
    if def.is_coop() {
        tags.push("co-op");
    }
    if def.repeatable {
        tags.push("board");
    }
    tags.join(" | ")
}

/// The footer for a desk row: the pitch, the objectives, twists and the pay.
fn desk_about(row: &DeskRow, ui: &UiFrame<'_, '_>) -> String {
    let Some(def) = row.quest().def() else {
        return String::new();
    };
    let quest = ui
        .view
        .quests
        .iter()
        .find(|quest| quest.quest == def.id && quest.owner == ui.view.player_id);
    let mut about = match row {
        DeskRow::TurnIn(_) => def.text.progress.to_owned(),
        DeskRow::Active(_) => quest
            .map(|quest| first_line(quest, ui.player, ui.snapshot))
            .unwrap_or_default(),
        _ => def.text.offer.to_owned(),
    };
    let twists: Vec<String> = match row {
        DeskRow::Offer(offer) | DeskRow::SoloOffer(offer) | DeskRow::Locked(offer) => {
            offer.posting.map_or_else(
                || {
                    def.twists
                        .iter()
                        .copied()
                        .map(drillgame_simulation::quest::Twist::describe)
                        .collect()
                },
                |posting| {
                    def.active_twists(posting.variant)
                        .into_iter()
                        .map(drillgame_simulation::quest::Twist::describe)
                        .collect()
                },
            )
        }
        _ => quest.map(|quest| twist_lines(quest)).unwrap_or_default(),
    };
    if !twists.is_empty() {
        about.push_str(" [");
        about.push_str(&twists.join("; "));
        about.push(']');
    }
    match row {
        DeskRow::Offer(_) if def.is_coop() && ui.snapshot.players.len() >= 2 => {
            about.push_str(" Crew job: shared with every rig, tight co-op windows.");
        }
        DeskRow::Offer(_) | DeskRow::SoloOffer(_) if def.is_coop() => {
            about.push_str(" Solo variant: one rig, longer windows.");
        }
        _ => {}
    }
    about.push_str(" Pays ");
    about.push_str(&def.reward.summary());
    about.push('.');
    about
}

/// The first objective line of a quest.
fn first_line(quest: &ActiveQuest, owner: &Player, snapshot: &RenderSnapshot<'_>) -> String {
    current_lines(quest, Some(owner), snapshot.tick_rate)
        .into_iter()
        .find(|(_, done)| !done)
        .map(|(line, _)| line)
        .unwrap_or_default()
}

/// A keeper's quest desk.
#[allow(
    clippy::too_many_lines,
    reason = "the desk lays out its rows in reading order in one place"
)]
pub(super) fn draw_quest_desk(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(giver) = ui.seat.interior_zone.and_then(QuestGiver::at) else {
        return;
    };
    let rows = &ui.view.quest_desk;
    let selected = ui.seat.selected_menu_item;
    let about = rows.get(selected).map(|row| desk_about(row, ui));
    let items: Vec<SectionItem> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let def = row.quest().def()?;
            let (icon, detail, value, color, coin, disabled) = match row {
                DeskRow::TurnIn(_) => (
                    &menu_icons::CONTRACT,
                    "Finished: turn it in".to_owned(),
                    format!("+{}", def.reward.credits),
                    LIME,
                    true,
                    false,
                ),
                DeskRow::Offer(offer) | DeskRow::SoloOffer(offer) => (
                    &menu_icons::LETTER,
                    match row {
                        DeskRow::SoloOffer(_) => format!("{} | take solo", quest_kind(def)),
                        _ if def.is_coop() && ui.snapshot.players.len() >= 2 => {
                            format!("{} | take with crew", quest_kind(def))
                        }
                        _ => quest_kind(def),
                    },
                    format!("+{}", def.reward.credits),
                    if offer.availability == QuestAvailability::Available {
                        GOLD
                    } else {
                        FOG
                    },
                    true,
                    offer.availability != QuestAvailability::Available,
                ),
                DeskRow::Active(quest) => {
                    let armed = ui.seat.quest_abandon_armed() == Some(*quest);
                    (
                        &menu_icons::FLAG,
                        if armed {
                            "Confirm again to drop it".to_owned()
                        } else {
                            "In progress (confirm to drop)".to_owned()
                        },
                        "DROP".to_owned(),
                        if armed { RED } else { ORANGE },
                        false,
                        false,
                    )
                }
                DeskRow::Locked(offer) => (
                    &menu_icons::BOOK,
                    match &offer.availability {
                        QuestAvailability::Locked(missing) => format!(
                            "Needs {}",
                            missing
                                .iter()
                                .map(|prerequisite| prerequisite
                                    .describe(ui.snapshot.terrain.settings()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        _ => String::new(),
                    },
                    "LOCKED".to_owned(),
                    FOG,
                    false,
                    true,
                ),
            };
            Some(SectionItem::item(ItemEntry {
                icon: Some(icon),
                label: def.title.to_owned(),
                detail,
                value,
                value_color: color,
                coin,
                selected: index == selected,
                disabled,
            }))
        })
        .collect();
    let points = ui.player.quests.reputation(giver);
    let rank = reputation_rank(points);
    let keeper = vec![
        SectionItem::icon_stat(
            &menu_icons::HOUSE,
            giver.name(),
            format!("{} ({points} rep)", rank_name(rank)),
            GOLD,
        ),
        SectionItem::text(giver.personality()),
        SectionItem::text(format!("Reputation perk: {}.", giver.perk())),
    ];
    let jobs = if items.is_empty() {
        vec![SectionItem::text(if giver == QuestGiver::Vale {
            "The board is empty until tomorrow's postings."
        } else {
            "Nothing for you right now. Dig deeper, find more, come back."
        })]
    } else {
        items
    };
    let title = if giver == QuestGiver::Vale {
        "HQ Quest Board".to_owned()
    } else {
        format!("{}'s Jobs", giver.name())
    };
    UiLayout::screen(draw).modal_with_render_plan(
        &title,
        "",
        &ModalContent::new(vec![
            Section::new("Keeper", SKY_LIGHT, keeper),
            Section::new("Jobs", GOLD, jobs),
        ])
        .footer(about)
        .prompts(DESK_HELP),
    );
}

/// The seat's quest log: active quests (the selected one is tracked), the tracked quest's
/// objectives, reputation and blueprints.
#[allow(
    clippy::too_many_lines,
    reason = "the log lays out its sections in reading order in one place"
)]
pub(super) fn draw_quest_log(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let quests = &ui.view.quests;
    let selected = ui.seat.selected_menu_item;
    let mut sections = Vec::new();
    if quests.is_empty() {
        sections.push(Section::new(
            "Active",
            GOLD,
            vec![SectionItem::text(
                "No jobs yet. Keepers post work at their counters: use the quest tab (Tab) at \
                 the depot, salvage yard, explosives shack, garage, fuel station and HQ.",
            )],
        ));
    } else {
        let rows = quests
            .iter()
            .enumerate()
            .filter_map(|(index, quest)| {
                let def = quest.def()?;
                let value = if quest.ready {
                    "READY".to_owned()
                } else if let Some(left) =
                    seconds_left(quest, ui.snapshot.tick, ui.snapshot.tick_rate)
                {
                    format!("{}:{:02}", left / 60, left % 60)
                } else if quest.crew {
                    "CREW".to_owned()
                } else {
                    String::new()
                };
                Some(SectionItem::item(ItemEntry {
                    icon: Some(if quest.crew {
                        &menu_icons::FLAG
                    } else {
                        &menu_icons::CONTRACT
                    }),
                    label: def.title.to_owned(),
                    detail: format!(
                        "{} | {}",
                        def.giver.name(),
                        first_line(quest, ui.player, ui.snapshot)
                    ),
                    value,
                    value_color: if quest.ready { LIME } else { GOLD },
                    coin: false,
                    selected: index == selected,
                    disabled: false,
                }))
            })
            .collect();
        sections.push(Section::new("Active (selected is tracked)", GOLD, rows));
        if let Some(quest) = quests.get(selected).copied() {
            sections.push(tracked_section(quest, ui));
        }
    }
    let log = &ui.player.quests;
    let reputation = QuestGiver::ALL
        .into_iter()
        .map(|giver| {
            let points = log.reputation(giver);
            SectionItem::stat(
                giver.name(),
                format!("{points} | {}", rank_name(reputation_rank(points))),
                if points > 0 { MINT } else { FOG },
            )
        })
        .collect();
    sections.push(Section::new("Reputation", SKY_LIGHT, reputation));
    let blueprints = BlueprintId::ALL
        .into_iter()
        .map(|blueprint| {
            let have = log.fragments_of(blueprint);
            SectionItem::meter(
                format!(
                    "{}{}",
                    blueprint.name(),
                    if log.blueprints.contains(&blueprint) {
                        " (done)"
                    } else {
                        ""
                    }
                ),
                f32::from(have),
                f32::from(blueprint.fragments_needed()),
                VIOLET,
                VIOLET,
            )
        })
        .collect();
    sections.push(Section::new("Blueprint fragments", PINK, blueprints));
    sections.push(Section::new(
        "Record",
        FOG,
        vec![SectionItem::stat(
            "Jobs finished",
            format!(
                "{} | board {} | failed {}",
                log.completed.len(),
                log.board_completed,
                log.failures
                    .values()
                    .map(|count| u32::from(*count))
                    .sum::<u32>()
            ),
            MIST,
        )],
    ));
    UiLayout::screen(draw).modal_with_render_plan(
        "Quest Log",
        "Your jobs and the crew's. The selected job is tracked on your HUD.",
        &ModalContent::new(sections).prompts(&[
            hint(Prompt::Navigate, "track"),
            hint(Prompt::Confirm, "map"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The tracked quest in detail.
fn tracked_section(quest: &ActiveQuest, ui: &UiFrame<'_, '_>) -> Section {
    let Some(def) = quest.def() else {
        return Section::new("Tracked", GOLD, Vec::new());
    };
    let mut items: Vec<SectionItem> = current_lines(quest, Some(ui.player), ui.snapshot.tick_rate)
        .into_iter()
        .map(|(line, done)| {
            SectionItem::stat(
                if done {
                    format!("[x] {line}")
                } else {
                    format!("[ ] {line}")
                },
                "",
                if done { LIME } else { BONE },
            )
        })
        .collect();
    for twist in twist_lines(quest) {
        items.push(SectionItem::stat(twist, "", ORANGE));
    }
    items.push(SectionItem::text(format!(
        "{}: \"{}\"",
        def.giver.name(),
        def.text.progress
    )));
    items.push(SectionItem::stat("Pays", def.reward.summary(), GOLD));
    Section::new(
        format!(
            "Stage {} of {}",
            (usize::from(quest.stage) + 1).min(def.stages.len()),
            def.stages.len()
        ),
        LIME,
        items,
    )
}

/// The tracked quest's next objective, under the HUD's warning badges.
pub(super) fn draw_quest_tracker(
    draw: &mut RaylibDrawHandle<'_>,
    seat: &SeatUi,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    (x, y, width): (i32, i32, i32),
) {
    let Some(quest) = tracked_quest(&view.quests, seat.tracked_quest) else {
        return;
    };
    let Some(def) = quest.def() else {
        return;
    };
    let line = if quest.ready {
        format!("Turn in to {}", def.giver.name())
    } else {
        first_line(quest, view.player, snapshot)
    };
    let timer = seconds_left(quest, snapshot.tick, snapshot.tick_rate)
        .filter(|_| !quest.ready)
        .map(|left| format!("{}:{:02}", left / 60, left % 60));
    let inner = width - 12;
    let mut lines = ui_kit::wrap(&line, inner, TextStyle::Body);
    lines.truncate(2);
    let height = 18 + i32::try_from(lines.len()).unwrap_or(1) * TextStyle::Body.line_height();
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    ui_kit::icon(draw, &menu_icons::CONTRACT, x + 4, y + 4);
    let title_width = inner
        - 14
        - timer
            .as_ref()
            .map_or(0, |timer| ui_kit::text_width(timer, TextStyle::Bold) + 4);
    ui_kit::text(
        draw,
        &ui_kit::truncate(def.title, title_width, TextStyle::Bold),
        x + 18,
        y + 5,
        TextStyle::Bold,
        if quest.ready { LIME } else { GOLD },
    );
    if let Some(timer) = timer {
        let urgent =
            seconds_left(quest, snapshot.tick, snapshot.tick_rate).is_some_and(|left| left < 30);
        ui_kit::text_right(
            draw,
            &timer,
            x + width - 6,
            y + 5,
            TextStyle::Bold,
            if urgent { RED } else { BONE },
        );
    }
    for (index, text) in lines.iter().enumerate() {
        ui_kit::text(
            draw,
            text,
            x + 6,
            y + 16 + i32::try_from(index).unwrap_or(0) * TextStyle::Body.line_height(),
            TextStyle::Body,
            MIST,
        );
    }
}

/// Colour of a marker on the maps.
const fn marker_color(kind: QuestMarkerKind) -> Color {
    match kind {
        QuestMarkerKind::Target => GOLD,
        QuestMarkerKind::Spot { lit: true, .. } | QuestMarkerKind::Switch { held: true } => LIME,
        QuestMarkerKind::Spot { next: true, .. } => CREAM,
        QuestMarkerKind::Spot { .. } => ORANGE,
        QuestMarkerKind::Haul => COPPER,
        QuestMarkerKind::Beacon => MINT,
        QuestMarkerKind::Switch { held: false } => RED,
    }
}

/// Quest markers on the minimap; `to_map` maps a tile to a minimap pixel.
pub(super) fn draw_minimap_quest_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[QuestMarker],
    to_map: impl Fn(i32, i32) -> Option<(i32, i32)>,
) {
    let blink = (ui_kit::seconds() * 2.0) as i32 % 2 == 0;
    for marker in markers {
        let Some((x, y)) = to_map(marker.position.x, marker.position.y) else {
            continue;
        };
        draw.draw_rectangle(x - 2, y - 2, 5, 5, INK);
        draw.draw_rectangle(
            x - 1,
            y - 1,
            3,
            3,
            if blink {
                marker_color(marker.kind)
            } else {
                BONE
            },
        );
    }
}

/// Quest markers on the mine map, clipped to `clip`. `(x0, y0)` shows tile `origin`; each
/// tile is `pixel` canvas pixels.
pub(super) fn draw_map_quest_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[QuestMarker],
    clip: Rectangle,
    origin: (i32, i32),
    pixel: i32,
) {
    ui_kit::begin_clip(clip);
    let (x0, y0) = (clip.x as i32, clip.y as i32);
    for marker in markers {
        let x = x0 + (marker.position.x - origin.0) * pixel + pixel / 2;
        let y = y0 + (marker.position.y - origin.1) * pixel + pixel / 2;
        // A small diamond.
        for row in -3_i32..=3 {
            let half = 3 - row.abs();
            draw.draw_rectangle(x - half - 1, y + row, half * 2 + 3, 1, INK);
        }
        for row in -2_i32..=2 {
            let half = 2 - row.abs();
            draw.draw_rectangle(
                x - half,
                y + row,
                half * 2 + 1,
                1,
                marker_color(marker.kind),
            );
        }
    }
    ui_kit::end_clip();
}

/// The quest fanfare: a banner across the top of the seat's canvas.
pub(super) fn draw_fanfare(draw: &mut RaylibDrawHandle<'_>, fanfare: &Fanfare, now: u64) {
    if !fanfare.showing(now) {
        return;
    }
    let age = now.saturating_sub(fanfare.tick);
    let canvas = ui_kit::canvas();
    let width = (canvas.width - 24).min(300);
    let (heading, color, style) = match fanfare.kind {
        FanfareKind::Complete => ("QUEST COMPLETE", GOLD, PanelStyle::Accent),
        FanfareKind::Ready => ("JOB DONE", LIME, PanelStyle::Accent),
        FanfareKind::Failed => ("QUEST FAILED", RED, PanelStyle::Warning),
        FanfareKind::Blueprint => ("BLUEPRINT COMPLETE", VIOLET, PanelStyle::Accent),
    };
    let mut details = ui_kit::wrap(&fanfare.detail, width - 16, TextStyle::Body);
    details.truncate(2);
    let heading_height = TextStyle::Title.line_height();
    let title_y = 5 + heading_height;
    let details_y = title_y + TextStyle::Bold.line_height();
    let height =
        details_y + 4 + i32::try_from(details.len()).unwrap_or(0) * TextStyle::Body.line_height();
    let drop = i32::try_from(12_u64.saturating_sub(age)).unwrap_or(0) * 3;
    let x = (canvas.width - width) / 2;
    let y = 46 - drop;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        style,
    );
    // Sparkles on the corners for a completion.
    if fanfare.kind == FanfareKind::Complete || fanfare.kind == FanfareKind::Blueprint {
        let twinkle = (age / 6).is_multiple_of(2);
        for (sx, sy) in [(x + 5, y + 5), (x + width - 8, y + 5)] {
            draw.draw_rectangle(sx, sy + 1, 3, 1, if twinkle { CREAM } else { color });
            draw.draw_rectangle(sx + 1, sy, 1, 3, if twinkle { CREAM } else { color });
        }
    }
    ui_kit::text_centred(draw, heading, x + width / 2, y + 5, TextStyle::Title, color);
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(&fanfare.title, width - 16, TextStyle::Bold),
        x + width / 2,
        y + title_y,
        TextStyle::Bold,
        BONE,
    );
    for (index, line) in details.iter().enumerate() {
        ui_kit::text_centred(
            draw,
            line,
            x + width / 2,
            y + details_y + i32::try_from(index).unwrap_or(0) * TextStyle::Body.line_height(),
            TextStyle::Body,
            MIST,
        );
    }
}
