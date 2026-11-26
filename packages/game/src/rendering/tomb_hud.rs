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
/// entrance banner when the pilot stands at a site's door. The run banner steps down below any
/// of `keep_clear` (the stats and wallet panels, a phone's top-row buttons) it would cover.
pub(super) fn draw_tomb_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    keep_clear: &[Rectangle],
) {
    if let Some(run) = snapshot.tombs.run_of(view.player_id) {
        draw_run_banner(draw, snapshot, view, run, (rect, keep_clear));
    } else if let Some(site) = view.tomb_door {
        draw_door_banner(draw, view, &site, (rect, keep_clear));
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the banner's lines, then the boss bar"
)]
fn draw_run_banner(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    run: &TombRun,
    (rect, keep_clear): (Rectangle, &[Rectangle]),
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
    let loot = format!(
        "UNBANKED {} cr   BANKED {} cr",
        crate::credits_text::grouped(unbanked),
        crate::credits_text::grouped(banked)
    );
    let status = match run.phase {
        RunPhase::Picking => "PICKING RELICS".to_owned(),
        RunPhase::Exploring if run.on_last_floor() && !run.boss_defeated => {
            match (run.site.style, boss_state(snapshot, run)) {
                (SiteStyle::TopDown, Some(boss)) => {
                    format!("THE CRYPT LORD - PHASE {}", boss.phase.max(1))
                }
                (SiteStyle::TopDown, None) => "THE CRYPT LORD AWAITS".to_owned(),
                (SiteStyle::SideView, _) => "THE GUARDIAN AWAITS".to_owned(),
            }
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
    let boss = boss_state(snapshot, run).filter(|_| !run.boss_defeated);
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
    let measure = |lines: &[(String, TextStyle, Color)]| {
        let width = lines
            .iter()
            .map(|(text, style, _)| ui_kit::text_width(text, *style))
            .max()
            .unwrap_or(0)
            + ui_kit::PANEL_PADDING * 2;
        (width, 6 + lines.len() as i32 * 11)
    };
    // The boss bar hangs under the banner: it keeps clear too.
    let bar = if boss.is_some() { 9 } else { 0 };
    let top = rect.y as i32 + 4;
    let (mut width, mut height) = measure(&lines);
    let (mut x, mut y) = banner_place(rect, (width, height + bar), top, keep_clear);
    // Too crowded up top (a phone's top-row buttons, a narrow split-screen view): a compact
    // banner that fits between the panels rather than one hanging over the middle of the view.
    if y > top + COMPACT_DROP {
        let full = lines.clone();
        lines[0].0 = format!("FLOOR {}/{}", run.floor + 1, site.floors());
        lines[1].0 = format!(
            "{} cr  ({banked} BANKED)",
            crate::credits_text::grouped(unbanked)
        );
        if !relics.is_empty() {
            lines[2].0 = format!("{} RELICS", relics.len());
        }
        if downed {
            "YOU ARE DOWN".clone_into(&mut lines[4].0);
        }
        let (compact_width, compact_height) = measure(&lines);
        let (compact_x, compact_y) =
            banner_place(rect, (compact_width, compact_height + bar), top, keep_clear);
        if compact_y < y {
            (width, height, x, y) = (compact_width, compact_height, compact_x, compact_y);
        } else {
            lines = full;
        }
    }
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
    // The boss's health under the banner while it fights.
    if let Some(boss) = boss {
        let bar_width = (width - ui_kit::PANEL_PADDING * 2).max(40);
        let bar_x = x + ui_kit::PANEL_PADDING;
        let bar_y = y + height + 3;
        draw.draw_rectangle(bar_x - 1, bar_y - 1, bar_width + 2, 6, INK);
        let filled = (bar_width as f32 * boss.health_fraction()) as i32;
        draw.draw_rectangle(
            bar_x,
            bar_y,
            filled,
            4,
            if boss.phase >= 3 { ROSE } else { RED },
        );
        // Phase marks at two thirds and one third.
        for share in [1.0 / 3.0, 2.0 / 3.0] {
            draw.draw_rectangle(bar_x + (bar_width as f32 * share) as i32, bar_y, 1, 4, BONE);
        }
    }
}

/// A run banner pushed further down than this (art pixels) switches to its compact lines.
const COMPACT_DROP: i32 = 40;

/// Where a banner `width` x `height` goes in `rect`: centred at `top` if that is clear of
/// `keep_clear`, else as near the middle as it fits in a clear gap at that height, else the
/// same just below the nearest panel in its way, and so on down.
fn banner_place(
    rect: Rectangle,
    (width, height): (i32, i32),
    top: i32,
    keep_clear: &[Rectangle],
) -> (i32, i32) {
    let (left, right) = (rect.x as i32, (rect.x + rect.width) as i32);
    let centred = (rect.x + rect.width / 2.0) as i32 - width / 2;
    let mut y = top;
    for _ in 0..=keep_clear.len() {
        let band: Vec<(i32, i32, i32)> = keep_clear
            .iter()
            .filter(|clear| clear.y < (y + height) as f32 && clear.y + clear.height > y as f32)
            .map(|clear| {
                (
                    clear.x.floor() as i32 - 3,
                    (clear.x + clear.width).ceil() as i32 + 3,
                    (clear.y + clear.height).ceil() as i32 + 3,
                )
            })
            .collect();
        // The clear gaps along this band, widest first.
        let mut edges: Vec<(i32, i32)> = band.iter().map(|(a, b, _)| (*a, *b)).collect();
        edges.sort_unstable();
        let mut gaps = Vec::new();
        let mut from = left;
        for (start, end) in edges {
            if start > from {
                gaps.push((from, start));
            }
            from = from.max(end);
        }
        if right > from {
            gaps.push((from, right));
        }
        if let Some((lo, hi)) = gaps
            .into_iter()
            .filter(|(lo, hi)| hi - lo >= width)
            .min_by_key(|(lo, hi)| (centred.clamp(*lo, hi - width) - centred).abs())
        {
            return (centred.clamp(lo, hi - width), y);
        }
        // Step below the nearest panel in the way and look again.
        let Some(bottom) = band.iter().map(|(_, _, bottom)| *bottom).min() else {
            break;
        };
        y = bottom.max(y + 1);
    }
    (centred, y)
}

/// The run's boss while it is up (its enemy state).
fn boss_state(
    snapshot: &RenderSnapshot<'_>,
    run: &TombRun,
) -> Option<fatherload_simulation::enemy::EnemyState> {
    let id = run.boss?;
    snapshot
        .enemies
        .iter()
        .find_map(|entity| match entity.state {
            fatherload_simulation::entity::EntityState::Enemy(state) if entity.id.get() == id => {
                Some(state)
            }
            _ => None,
        })
}

fn draw_door_banner(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    site: &fatherload_simulation::tomb::TombSite,
    (rect, keep_clear): (Rectangle, &[Rectangle]),
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
    let (x, y) = banner_place(rect, (width, height), rect.y as i32 + 4, keep_clear);
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
            let blurb = relic.blurb_in(run.site.style);
            if index == selected {
                about = Some(format!("{blurb}{synergy}"));
            }
            SectionItem::item(ItemEntry {
                icon: None,
                label: def.name.to_owned(),
                detail: format!("{blurb}{synergy}"),
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
            .map(|relic| SectionItem::stat(relic.name(), relic.blurb_in(run.site.style), MINT))
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
        SectionItem::stat(
            "Banked",
            format!("{} cr", crate::credits_text::grouped(summary.banked)),
            GOLD,
        ),
        SectionItem::stat(
            "Lost",
            format!("{} cr", crate::credits_text::grouped(summary.lost)),
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
            .map(|relic| SectionItem::stat(relic.name(), relic.blurb_in(summary.style), MINT))
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
        SectionItem::stat(
            "Best haul",
            format!("{} cr", crate::credits_text::grouped(best)),
            GOLD,
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 640.0,
        height: 360.0,
    };

    #[test]
    fn a_banner_with_room_stays_centred_at_the_top() {
        let stats = Rectangle::new(4.0, 4.0, 150.0, 60.0);
        assert_eq!(banner_place(VIEW, (200, 50), 4, &[stats]), (220, 4));
    }

    #[test]
    fn a_banner_slides_off_a_panel_it_only_clips() {
        let stats = Rectangle::new(4.0, 4.0, 180.0, 60.0);
        let (x, y) = banner_place(VIEW, (300, 50), 4, &[stats]);
        assert_eq!(y, 4);
        assert!(x >= 187, "clear of the stats panel: {x}");
    }

    #[test]
    fn a_banner_steps_below_the_nearest_panel_in_its_way() {
        // A phone: stats left, wallet right, the top-row buttons in the middle.
        let clear = [
            Rectangle::new(4.0, 4.0, 176.0, 57.0),
            Rectangle::new(318.0, 4.0, 104.0, 87.0),
            Rectangle::new(202.0, 3.0, 20.0, 20.0),
            Rectangle::new(229.0, 3.0, 20.0, 20.0),
            Rectangle::new(255.0, 3.0, 20.0, 20.0),
        ];
        let view = Rectangle::new(0.0, 0.0, 422.0, 195.0);
        let (x, y) = banner_place(view, (106, 50), 4, &clear);
        assert_eq!(y, 26, "just under the buttons");
        assert!(x >= 183 && x + 106 <= 315, "between the panels: {x}");
    }
}
