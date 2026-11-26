//! The god-mode toolkit dialog (Wave 14, W14-A; its input is `shell/test_toolkit.rs`): four
//! sections of rows (supplies, progress, sky, creatures), each with its live value read from
//! the frame's snapshot.

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
    prompts::hint,
    screen::UiFrame,
};
use crate::{
    input::prompts::Prompt,
    shell::test_toolkit::{ToolkitRow, is_boss, weather_name},
};
use fatherload_simulation::{
    EnemyDifficulty, TestModeState, descent::DescentBoss, enemy::EnemyKind,
};

/// A row's icon.
const fn icon(row: ToolkitRow) -> &'static SpriteDef {
    match row {
        ToolkitRow::InfiniteSupplies => &icons::BOMB,
        ToolkitRow::Restock => &menu_icons::GIFT,
        ToolkitRow::StockAll => &menu_icons::CROWN,
        ToolkitRow::UnlockStory => &menu_icons::BOOK,
        ToolkitRow::Gate(_) => &menu_icons::KEY,
        ToolkitRow::EndingDone => &menu_icons::MEDAL_GOLD,
        ToolkitRow::TimeOfDay => &icons::SUN,
        ToolkitRow::DayLock => &icons::MOON,
        ToolkitRow::Weather => &icons::AIR,
        ToolkitRow::Creature => &menu_icons::DICE,
        ToolkitRow::SpawnCreature => &menu_icons::PERSON,
        ToolkitRow::ClearEnemies => &menu_icons::COLLAPSE,
        ToolkitRow::FreezeAi => &menu_icons::GHOST,
        ToolkitRow::Difficulty => &icons::WARNING,
    }
}

/// The section a row is listed under.
const fn section_of(row: ToolkitRow) -> usize {
    match row {
        ToolkitRow::InfiniteSupplies | ToolkitRow::Restock | ToolkitRow::StockAll => 0,
        ToolkitRow::UnlockStory | ToolkitRow::Gate(_) | ToolkitRow::EndingDone => 1,
        ToolkitRow::TimeOfDay | ToolkitRow::DayLock | ToolkitRow::Weather => 2,
        ToolkitRow::Creature
        | ToolkitRow::SpawnCreature
        | ToolkitRow::ClearEnemies
        | ToolkitRow::FreezeAi
        | ToolkitRow::Difficulty => 3,
    }
}

/// The colour of an enemy setting: calm green, normal amber, hard red.
const fn difficulty_color(difficulty: EnemyDifficulty) -> Color {
    match difficulty {
        EnemyDifficulty::Peaceful => LIME,
        EnemyDifficulty::Normal => AMBER,
        EnemyDifficulty::Hard => RED,
    }
}

/// A row's detail line, value and colour, and whether it is greyed out.
struct RowText {
    detail: String,
    value: String,
    color: Color,
    disabled: bool,
}

impl RowText {
    fn plain(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            value: String::new(),
            color: BONE,
            disabled: false,
        }
    }

    fn value(detail: impl Into<String>, value: impl Into<String>, color: Color) -> Self {
        Self {
            value: value.into(),
            color,
            ..Self::plain(detail)
        }
    }

    fn toggle(detail: impl Into<String>, on: bool) -> Self {
        if on {
            Self::value(detail, "ON", LIME)
        } else {
            Self::value(detail, "OFF", FOG)
        }
    }
}

/// A row with its current value, read from the frame's snapshot.
fn row_text(row: ToolkitRow, ui: &UiFrame<'_, '_>, state: TestModeState) -> RowText {
    let snapshot = ui.snapshot;
    // What the dialog chose and the world does not show yet (a paused world applies it on
    // resume).
    let pending = ui.shell.toolkit_pending;
    let difficulty = pending.difficulty().unwrap_or(snapshot.enemy_difficulty);
    match row {
        ToolkitRow::InfiniteSupplies => RowText::toggle(
            "Every consumable is topped back up each tick",
            state.infinite_supplies,
        ),
        ToolkitRow::Restock => {
            RowText::plain("Refill your bombs, charges, ammo, kits and supplies once")
        }
        ToolkitRow::StockAll => {
            RowText::plain("Every part, gear, cosmetic, gate reward, space line and chart")
        }
        ToolkitRow::UnlockStory => RowText::toggle(
            "Every log, echo and codex entry read (off: a fresh story)",
            state.story_unlocked,
        ),
        ToolkitRow::Gate(descent) => {
            let boss = DescentBoss::ALL[usize::from(descent).min(7)];
            let cleared = pending.gate(descent).unwrap_or_else(|| {
                snapshot.expedition.beat(boss) || (descent == 0 && snapshot.crust.deep_claim)
            });
            if cleared {
                RowText::value("Its boss beaten, its reward held", "CLEARED", LIME)
            } else {
                RowText::value("Its boss beaten, its reward held", "SEALED", FOG)
            }
        }
        ToolkitRow::EndingDone => RowText::toggle(
            "The Heart woke, the cable lit, the stars open",
            pending
                .ending_done()
                .unwrap_or(snapshot.heart.fight.beaten && snapshot.expedition.cable_lit),
        ),
        ToolkitRow::TimeOfDay => {
            let (hours, minutes) = pending.minutes().map_or_else(
                || snapshot.clock.hours_minutes(),
                |chosen| {
                    (
                        u8::try_from(chosen / 60).unwrap_or(0),
                        u8::try_from(chosen % 60).unwrap_or(0),
                    )
                },
            );
            RowText::value(
                "Confirm: the next stop | left/right: an hour",
                format!("{hours:02}:{minutes:02}"),
                GOLD,
            )
        }
        ToolkitRow::DayLock => RowText::toggle(
            "Hold the time of day where it is",
            pending.held().unwrap_or_else(|| snapshot.clock.is_held()),
        ),
        ToolkitRow::Weather => RowText::value(
            "Forced on every planet and moon",
            weather_name(state.weather),
            SKY,
        ),
        ToolkitRow::Creature => {
            let kind = ui.shell.toolkit_creature();
            let boss = is_boss(kind);
            RowText::value(
                format!(
                    "{} of {} | left/right: one, tabs: ten{}",
                    ui.shell.toolkit_creature % EnemyKind::ALL.len() + 1,
                    EnemyKind::ALL.len(),
                    if boss { " | a boss" } else { "" }
                ),
                kind.name(),
                if boss { RED } else { LAVENDER },
            )
        }
        ToolkitRow::SpawnCreature => RowText {
            disabled: !difficulty.hostile(),
            ..RowText::value(
                if difficulty.hostile() {
                    "At a safe distance to your side"
                } else {
                    "Creatures are off: set Enemies first"
                },
                ui.shell.toolkit_creature().name(),
                LAVENDER,
            )
        },
        ToolkitRow::ClearEnemies => RowText::plain("Every creature and shot within 48 tiles"),
        ToolkitRow::FreezeAi => {
            RowText::toggle("Creatures and their shots stand still", state.ai_frozen)
        }
        ToolkitRow::Difficulty => RowText::value(
            difficulty.description(),
            difficulty.name(),
            difficulty_color(difficulty),
        ),
    }
}

/// Draws the toolkit dialog.
pub(super) fn draw_test_toolkit(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let state = ui
        .shell
        .shown_test_mode(ui.snapshot.test_mode)
        .unwrap_or_default();
    let descents = ui.snapshot.terrain.settings().generator.has_descents();
    let rows = ToolkitRow::rows(descents);
    let selected = ui.shell.selected_menu_item.min(rows.len() - 1);
    let titles = [
        ("Supplies", ORANGE),
        ("Progress", GOLD),
        ("Sky", SKY),
        ("Creatures", RED),
    ];
    let sections = titles
        .iter()
        .enumerate()
        .filter_map(|(section, (title, color))| {
            let items: Vec<SectionItem> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| section_of(**row) == section)
                .map(|(index, row)| {
                    let text = row_text(*row, ui, state);
                    SectionItem::item(ItemEntry {
                        icon: Some(icon(*row)),
                        label: row.label(),
                        detail: text.detail,
                        value: text.value,
                        value_color: text.color,
                        coin: false,
                        selected: index == selected,
                        disabled: text.disabled,
                    })
                })
                .collect();
            (!items.is_empty()).then(|| Section::new(*title, *color, items))
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "God-Mode Toolkit",
        "Test mode: stock your rig, set the world's progress and call up creatures.",
        &ModalContent::new(sections).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Adjust, "change"),
            hint(Prompt::Confirm, "use"),
            hint(Prompt::Back, "back"),
        ]),
    );
}
