//! Tombs and crypts on screen (WP-TOMBS, T27): the run HUD (floor, relics, unbanked loot, the
//! party), the entrance banner at a site's door (its style, tier and seal), and the three tomb
//! dialogs: the relic pick between floors, the run summary and the tomb codex.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI layout mixes small integer art pixels and float rectangles"
)]

use fatherload_simulation::tomb::{
    LORE_PAGES, Relic, RelicKind, RunOutcome, RunPhase, SiteOpening, SiteStyle, TombRun,
    relic::synergies_of,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "HUD code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::{RenderSnapshot, ViewSnapshot},
};

/// The accent colour of a site style (tomb sandstone gold, crypt violet).
#[must_use]
pub const fn style_color(style: SiteStyle) -> Color {
    match style {
        SiteStyle::SideView => GOLD,
        SiteStyle::TopDown => LAVENDER,
    }
}

/// A relic's short label colour: bargains read as a warning.
const fn relic_color(relic: Relic, kind: RelicKind) -> Color {
    let _ = relic;
    match kind {
        RelicKind::Perk => MINT,
        RelicKind::Bargain => ROSE,
    }
}

/// The run HUD across the top middle of the view while its player is in a run, and the
/// entrance banner when the pilot stands at a site's door.
pub(super) fn draw_tomb_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    if let Some(run) = snapshot.tombs.run_of(view.player_id) {
        draw_run_banner(draw, snapshot, view, run, rect);
    } else if let Some(site) = view.tomb_door {
        draw_door_banner(draw, view, &site, rect);
    }
}

fn draw_run_banner(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    run: &TombRun,
    rect: Rectangle,
) {
    let site = &run.site;
    let accent = style_color(site.style);
    let title = format!(
        "{} - FLOOR {}/{}",
        site.title().to_uppercase(),
        run.floor + 1,
        site.floors()
    );
    let member = run.member(view.player_id);
    let unbanked = member.map_or(0, |member| member.unbanked);
    let banked = member.map_or(0, |member| member.banked);
    let loot = format!("UNBANKED {unbanked} cr   BANKED {banked} cr");
    let status = match run.phase {
        RunPhase::Picking => "PICKING RELICS".to_owned(),
        RunPhase::Exploring if run.on_last_floor() && !run.boss_defeated => {
            "THE GUARDIAN AWAITS".to_owned()
        }
        RunPhase::Exploring if run.on_last_floor() => "THE WAY OUT IS OPEN".to_owned(),
        RunPhase::Exploring => "FIND THE STAIRS".to_owned(),
    };
    let relics: Vec<Relic> = member
        .map(|member| member.relics.clone())
        .unwrap_or_default();
    let relic_line = if relics.is_empty() {
        "NO RELICS".to_owned()
    } else {
        relics
            .iter()
            .map(|relic| relic.name())
            .collect::<Vec<_>>()
            .join("  ")
    };
    let _ = snapshot;
    let party: Vec<String> = run
        .members
        .iter()
        .enumerate()
        .filter(|(_, other)| other.player != view.player_id)
        .map(|(index, other)| {
            let name = format!("P{}", index + 1);
            if other.is_downed() {
                format!("{name} DOWN")
            } else {
                name
            }
        })
        .collect();
    let downed = member.is_some_and(fatherload_simulation::tomb::RunMember::is_downed);
    let mut lines: Vec<(String, TextStyle, Color)> = vec![
        (title, TextStyle::Bold, accent),
        (loot, TextStyle::Body, GOLD),
        (
            relic_line,
            TextStyle::Body,
            if relics.is_empty() { FOG } else { MINT },
        ),
        (
            status,
            TextStyle::Bold,
            if downed { RED } else { SKY_LIGHT },
        ),
    ];
    if downed {
        lines.push((
            "YOU ARE DOWN - A CREWMATE OR A SHRINE CAN REVIVE YOU".to_owned(),
            TextStyle::Body,
            RED,
        ));
    }
    if !party.is_empty() {
        lines.push((
            format!("PARTY  {}", party.join("  ")),
            TextStyle::Body,
            BONE,
        ));
    }
    let width = lines
        .iter()
        .map(|(text, style, _)| ui_kit::text_width(text, *style))
        .max()
        .unwrap_or(0)
        + ui_kit::PANEL_PADDING * 2;
    let height = 6 + lines.len() as i32 * 11;
    let x = (rect.x + rect.width / 2.0) as i32 - width / 2;
    let y = rect.y as i32 + 4;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    for (index, (text, style, color)) in lines.iter().enumerate() {
        ui_kit::text(
            draw,
            text,
            x + ui_kit::PANEL_PADDING,
            y + 5 + index as i32 * 11,
            *style,
            *color,
        );
    }
}

fn draw_door_banner(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    site: &fatherload_simulation::tomb::TombSite,
    rect: Rectangle,
) {
    let accent = style_color(site.style);
    let sealed = matches!(
        view.pilot_usable,
        Some((
            _,
            fatherload_simulation::mechanism::Usable::TombDoor { sealed: true, .. }
        ))
    );
    let kind = match site.style {
        SiteStyle::SideView => "SIDE-VIEW TOMB: a platformer run",
        SiteStyle::TopDown => "TOP-DOWN CRYPT: a dungeon crawl",
    };
    let seal = if sealed {
        match site.opening {
            SiteOpening::Key => "SEALED - needs a tomb key",
            SiteOpening::Story => "SEALED - answers a story seen through",
            SiteOpening::Explosives => "SEALED - cracked: a blast might open it",
        }
    } else {
        "OPEN - the party goes in together, the rig waits"
    };
    let lines = [
        (site.title().to_uppercase(), TextStyle::Bold, accent),
        (kind.to_owned(), TextStyle::Body, BONE),
        (
            format!("{} floors, a guardian at the bottom", site.floors()),
            TextStyle::Body,
            FOG,
        ),
        (
            seal.to_owned(),
            TextStyle::Bold,
            if sealed { ROSE } else { MINT },
        ),
    ];
    let width = lines
        .iter()
        .map(|(text, style, _)| ui_kit::text_width(text, *style))
        .max()
        .unwrap_or(0)
        + ui_kit::PANEL_PADDING * 2;
    let height = 6 + lines.len() as i32 * 11;
    let x = (rect.x + rect.width / 2.0) as i32 - width / 2;
    let y = rect.y as i32 + 4;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    for (index, (text, style, color)) in lines.iter().enumerate() {
        ui_kit::text(
            draw,
            text,
            x + ui_kit::PANEL_PADDING,
            y + 5 + index as i32 * 11,
            *style,
            *color,
        );
    }
}

/// The relic pick between floors ([`crate::shell::ModalScreen::RelicPick`]).
pub(super) fn draw_relic_pick(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let tombs = ui.snapshot.tombs;
    let player = ui.view.player_id;
    let Some(run) = tombs.run_of(player) else {
        return;
    };
    let Some(member) = run.member(player) else {
        return;
    };
    let selected = ui.seat.selected_menu_item;
    let mut about = None;
    let mut rows: Vec<SectionItem> = member
        .offers
        .iter()
        .enumerate()
        .map(|(index, relic)| {
            let def = relic.def();
            let mut held = member.relics.clone();
            held.push(*relic);
            let synergy = synergies_of(&held)
                .into_iter()
                .find(|synergy| synergy.pair.0 == *relic || synergy.pair.1 == *relic)
                .map(|synergy| format!("  Synergy: {}!", synergy.name))
                .unwrap_or_default();
            if index == selected {
                about = Some(format!("{}{synergy}", def.blurb));
            }
            SectionItem::item(ItemEntry {
                icon: None,
                label: def.name.to_owned(),
                detail: format!("{}{synergy}", def.blurb),
                value: match def.kind {
                    RelicKind::Perk => "Perk".to_owned(),
                    RelicKind::Bargain => "Bargain".to_owned(),
                },
                value_color: relic_color(*relic, def.kind),
                coin: false,
                selected: index == selected,
                disabled: false,
            })
        })
        .collect();
    let pass = member.offers.len();
    rows.push(SectionItem::item(ItemEntry {
        icon: None,
        label: "Take nothing".to_owned(),
        detail: "Go down as you are.".to_owned(),
        value: String::new(),
        value_color: FOG,
        coin: false,
        selected: selected >= pass,
        disabled: false,
    }));
    if selected >= pass {
        about = Some("Leave the relics where they lie.".to_owned());
    }
    let held: Vec<SectionItem> = if member.relics.is_empty() {
        vec![SectionItem::text("None yet.")]
    } else {
        member
            .relics
            .iter()
            .map(|relic| SectionItem::stat(relic.name(), relic.blurb(), MINT))
            .collect()
    };
    let waiting = run
        .members
        .iter()
        .filter(|other| other.player != player && !other.picked)
        .count();
    let mut sections = vec![
        Section::new("Relics on offer", style_color(run.site.style), rows),
        Section::new("Carried this run", MINT, held),
    ];
    if waiting > 0 {
        sections.push(Section::new(
            "Party",
            BONE,
            vec![SectionItem::text(format!(
                "{waiting} crewmate(s) still choosing; the party goes down together."
            ))],
        ));
    }
    UiLayout::screen(draw).modal_with_render_plan(
        &format!("Floor {} cleared", run.floor + 1),
        "Pick one relic for the rest of this run. Bargains pay big and cost you.",
        &ModalContent::new(sections).footer(about).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, "take"),
        ]),
    );
}

/// The run summary ([`crate::shell::ModalScreen::TombSummary`]).
pub(super) fn draw_tomb_summary(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(summary) = ui.snapshot.tombs.summaries.get(&ui.view.player_id) else {
        return;
    };
    let won = summary.outcome.banks();
    let mut stats = vec![
        SectionItem::stat(
            "Floors",
            format!("{} of {}", summary.floors, summary.floors_total),
            BONE,
        ),
        SectionItem::stat("Banked", format!("{} cr", summary.banked), GOLD),
        SectionItem::stat(
            "Lost",
            format!("{} cr", summary.lost),
            if summary.lost > 0 { RED } else { FOG },
        ),
        SectionItem::stat(
            "Time",
            {
                let seconds = summary.ticks / u64::from(ui.snapshot.tick_rate.hz().max(1));
                format!("{}:{:02}", seconds / 60, seconds % 60)
            },
            SKY_LIGHT,
        ),
    ];
    if summary.outcome == RunOutcome::Finished {
        stats.push(SectionItem::stat(
            "Trophy",
            "A trophy case for your home and the Crown decal",
            MINT,
        ));
    }
    let relics: Vec<SectionItem> = if summary.relics.is_empty() {
        vec![SectionItem::text("No relics this time.")]
    } else {
        summary
            .relics
            .iter()
            .map(|relic| SectionItem::stat(relic.name(), relic.blurb(), MINT))
            .collect()
    };
    let mut sections = vec![
        Section::new("The run", style_color(summary.style), stats),
        Section::new("Relics carried", MINT, relics),
    ];
    if let Some((title, text)) = summary
        .lore
        .and_then(|page| LORE_PAGES.get(usize::from(page)))
    {
        sections.push(Section::new(
            format!("Lore found: {title}"),
            LAVENDER,
            vec![SectionItem::text(*text)],
        ));
    }
    let subtitle = match summary.outcome {
        RunOutcome::Finished => "Down every floor and out past the guardian. Everything banked.",
        RunOutcome::Retreated => "Out the way you came, with everything you carried.",
        RunOutcome::Wiped => "The tomb keeps what was not banked at a shrine.",
        RunOutcome::Abandoned => "You left mid-run; the unbanked loot stays below.",
    };
    let mut layout = UiLayout::screen(draw);
    let content = ModalContent::new(sections).prompts(&[hint(Prompt::Confirm, "close")]);
    let content = if won { content } else { content.warning() };
    layout.modal_with_render_plan(summary.outcome.title(), subtitle, &content);
}

/// The tomb codex ([`crate::shell::ModalScreen::TombCodex`]): relics found, maps, trophies and
/// lore pages.
pub(super) fn draw_tomb_codex(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let unlocks = ui.snapshot.tombs.unlocks_of(ui.view.player_id);
    let codex: Vec<SectionItem> = Relic::ALL
        .iter()
        .map(|relic| {
            if unlocks.is_some_and(|unlocks| unlocks.codex.contains(relic)) {
                SectionItem::stat(
                    relic.name(),
                    relic.blurb(),
                    relic_color(*relic, relic.kind()),
                )
            } else {
                SectionItem::stat("???", "Not yet found", FOG)
            }
        })
        .collect();
    let (maps, cleared, trophies, runs, finished, best) = unlocks.map_or((0, 0, 0, 0, 0, 0), |u| {
        (
            u.maps.len(),
            u.cleared.len(),
            u.trophies,
            u.runs,
            u.finished,
            u.best_haul,
        )
    });
    let records = vec![
        SectionItem::stat("Tombs mapped", maps.to_string(), BONE),
        SectionItem::stat("Tombs cleared", cleared.to_string(), MINT),
        SectionItem::stat("Guardians felled", trophies.to_string(), GOLD),
        SectionItem::stat("Runs", format!("{runs} ({finished} finished)"), SKY_LIGHT),
        SectionItem::stat("Best haul", format!("{best} cr"), GOLD),
    ];
    let lore: Vec<SectionItem> = LORE_PAGES
        .iter()
        .enumerate()
        .map(|(index, (title, text))| {
            let found = unlocks.is_some_and(|unlocks| {
                unlocks
                    .lore
                    .contains(&u8::try_from(index).unwrap_or(u8::MAX))
            });
            if found {
                SectionItem::stat(*title, *text, LAVENDER)
            } else {
                SectionItem::stat("Missing page", "Found in tomb vaults", FOG)
            }
        })
        .collect();
    UiLayout::screen(draw).modal_with_render_plan(
        "Tomb Codex",
        "Relics you have carried, tombs you have mapped, and the builders' pages.",
        &ModalContent::new(vec![
            Section::new("Records", GOLD, records),
            Section::new("Relic codex", MINT, codex),
            Section::new("Lore pages", LAVENDER, lore),
        ])
        .prompts(&[
            hint(Prompt::Confirm, "next page"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// Tomb and crypt markers on the mine map: every site in view whose door the seat has seen or
/// whose map it holds (entered once). A gold pyramid marks a side-view tomb, a violet arch a
/// top-down crypt; a red pip is a seal still unbroken, a mint pip a site cleared.
pub(super) fn draw_map_tombs(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
    clip: Rectangle,
    origin: (i32, i32),
    pixel: i32,
) {
    let terrain = ui.snapshot.terrain;
    if !terrain.settings().generator.has_tombs() || pixel <= 0 {
        return;
    }
    let rect = fatherload_simulation::worldgen::features::TileRect {
        min: fatherload_simulation::terrain::TilePosition {
            x: origin.0,
            y: origin.1,
        },
        max: fatherload_simulation::terrain::TilePosition {
            x: origin.0 + clip.width as i32 / pixel,
            y: origin.1 + clip.height as i32 / pixel,
        },
    };
    let unlocks = ui.snapshot.tombs.unlocks_of(ui.view.player_id);
    ui_kit::begin_clip(clip);
    for site in
        fatherload_simulation::worldgen::tomb_sites::sites_touching(terrain.settings(), &rect)
    {
        let mapped = unlocks.is_some_and(|unlocks| unlocks.maps.contains(&site.id));
        if !mapped && !ui.view.is_explored(site.door) {
            continue;
        }
        let x = clip.x as i32 + (site.door.x - origin.0) * pixel;
        let y = clip.y as i32 + (site.door.y - origin.1) * pixel;
        let color = style_color(site.style);
        match site.style {
            SiteStyle::SideView => {
                // A little stepped pyramid.
                for step in 0..4 {
                    draw.draw_rectangle(x - 1 - step, y - 6 + step * 2, 3 + step * 2, 2, INK);
                    draw.draw_rectangle(x - step, y - 6 + step * 2, 1 + step * 2, 1, color);
                }
            }
            SiteStyle::TopDown => {
                // A mausoleum arch.
                draw.draw_rectangle(x - 4, y - 6, 9, 8, INK);
                draw.draw_rectangle(x - 3, y - 5, 7, 2, color);
                draw.draw_rectangle(x - 3, y - 3, 1, 4, color);
                draw.draw_rectangle(x + 3, y - 3, 1, 4, color);
            }
        }
        let sealed = !ui.snapshot.tombs.is_unsealed(site.id);
        let cleared = unlocks.is_some_and(|unlocks| unlocks.cleared.contains(&site.id));
        if cleared || sealed {
            draw.draw_rectangle(x + 4, y - 8, 3, 3, if cleared { MINT } else { RED });
        }
        let label = format!("{} {}", site.style.noun(), site.tier);
        ui_kit::text(draw, &label, x + 7, y - 6, TextStyle::Body, color);
    }
    ui_kit::end_clip();
}
