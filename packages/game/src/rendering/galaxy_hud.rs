//! The galaxy story on screen (Wave 13 story): the road's journal and the finale's choice, the
//! goal line on the HUD, the road's story cards, and the ending (its lead over the Harbour, the
//! credits over the road's lit waymarks, the epilogue).
//!
//! Nothing strobes: cards slide and fade, the credits climb steadily, lamps brighten slowly.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "presentation maths on small bounded values"
)]

use fatherload_simulation::galaxy_story::{
    CHAPTER_COUNT, Goal,
    finale::{self, CreditLine, FinaleChoice, FinalePhase, FinaleView},
    journal::SPACE_LOGS,
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
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::ViewSnapshot,
    shell::galaxy::{
        CHOICE_ROWS, GalaxyRow, chapter_page, chapter_title, galaxy_rows, goal_line, lead_entry,
        log_entry, site_words, stats_words,
    },
};

/// The colour chapter `chapter` is marked with (the road warms from the Archive's cool to the
/// Harbour's red gold).
#[must_use]
pub const fn chapter_colour(chapter: u8) -> Color {
    match chapter {
        0 => SKY_LIGHT,
        1 => MINT,
        2 => STEEL_LIGHT,
        3 => SAND,
        4 => TEAL,
        5 => LAVENDER,
        6 => CREAM,
        7 => LIME,
        8 => AMBER,
        _ => GOLD,
    }
}

/// The road's journal page.
#[allow(clippy::too_many_lines, reason = "one builder per kind of row")]
pub(super) fn draw_galaxy_journal(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let world = ui.snapshot.world;
    let story = world.galaxy_story();
    let rows = galaxy_rows(world);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let item = |index: usize, icon, label: String, detail: String, value: String, colour| {
        SectionItem::item(ItemEntry {
            icon: Some(icon),
            label,
            detail,
            value,
            value_color: colour,
            coin: false,
            selected: index == selected,
            disabled: false,
        })
    };
    let mut road = Vec::new();
    let mut pages = Vec::new();
    let mut logs = Vec::new();
    let mut leads = Vec::new();
    let mut end = Vec::new();
    if let Some((title, line)) = goal_line(world) {
        road.push(SectionItem::stat(
            "Now",
            title,
            chapter_colour(story.chapter),
        ));
        road.push(SectionItem::text(line));
    } else if story.finished() {
        road.push(SectionItem::stat(
            "The road",
            story
                .choice
                .map_or("walked", |choice| finale::title(choice))
                .to_owned(),
            GOLD,
        ));
        road.push(SectionItem::text(
            "The road is walked; the sky is not. Every new system holds something: the \
             discovery log keeps it."
                .to_owned(),
        ));
    }
    for (index, row) in rows.iter().enumerate() {
        match *row {
            GalaxyRow::SetCourse => road.push(item(
                index,
                &menu_icons::ROCKET,
                "Set course for the road".to_owned(),
                "The current goal becomes your waypoint (the warp's target)".to_owned(),
                "NAV".to_owned(),
                SKY_LIGHT,
            )),
            GalaxyRow::Choose => road.push(item(
                index,
                &menu_icons::CROWN,
                "The bell and the lamp".to_owned(),
                "Make the finale's choice at the Harbour's gate".to_owned(),
                "CHOOSE".to_owned(),
                GOLD,
            )),
            GalaxyRow::AskArchivist => road.push(item(
                index,
                &menu_icons::SIGNAL,
                "Ask Index about the road".to_owned(),
                "The archivist remembers".to_owned(),
                String::new(),
                MINT,
            )),
            GalaxyRow::Chapter(chapter) => {
                let done = chapter < story.chapter;
                pages.push(item(
                    index,
                    &menu_icons::BOOK,
                    chapter_title(chapter),
                    world.galaxy_words(chapter).system.clone(),
                    if done { "DONE" } else { "UNDER WAY" }.to_owned(),
                    chapter_colour(chapter),
                ));
            }
            GalaxyRow::Log(log) => {
                let entry = &SPACE_LOGS[usize::from(log)];
                logs.push(item(
                    index,
                    &menu_icons::LETTER,
                    entry.title.to_owned(),
                    entry.voice.signature().to_owned(),
                    String::new(),
                    AMBER,
                ));
            }
            GalaxyRow::Lead(lead) => {
                if let Some((title, status, _)) = lead_entry(world, lead) {
                    let done = story.leads.get(lead).is_some_and(|record| record.done);
                    leads.push(item(
                        index,
                        if done {
                            &menu_icons::READY
                        } else {
                            &menu_icons::FLAG
                        },
                        title,
                        status,
                        if done { "DONE" } else { "SET COURSE" }.to_owned(),
                        if done { FOG } else { LIME },
                    ));
                }
            }
            GalaxyRow::Stats => end.push(item(
                index,
                &menu_icons::MEDAL_GOLD,
                "The road so far".to_owned(),
                format!(
                    "{} of {} chapters, {} systems visited",
                    story.chapters_done(),
                    CHAPTER_COUNT,
                    story.visited.len()
                ),
                String::new(),
                GOLD,
            )),
            GalaxyRow::TurnPage => end.push(item(
                index,
                &menu_icons::BACK,
                "Turn the page: the map".to_owned(),
                String::new(),
                String::new(),
                GOLD,
            )),
        }
    }
    let mut sections = vec![Section::new("The road", GOLD, road)];
    if !pages.is_empty() {
        sections.push(Section::new(
            format!("Chapters ({} of {CHAPTER_COUNT})", story.chapters_done()),
            SKY_LIGHT,
            pages,
        ));
    }
    if !logs.is_empty() {
        sections.push(Section::new(
            format!(
                "Logs of the road ({} of {})",
                story.logs.len(),
                SPACE_LOGS.len()
            ),
            AMBER,
            logs,
        ));
    }
    if !leads.is_empty() {
        let open = story.leads.iter().filter(|record| !record.done).count();
        sections.push(Section::new(
            format!(
                "Discovery log ({open} open, {} done)",
                story.leads.len() - open
            ),
            LIME,
            leads,
        ));
    }
    sections.push(Section::new("", FOG, end));
    let footer = match rows.get(selected) {
        Some(GalaxyRow::Chapter(chapter)) => Some(chapter_page(world, *chapter)),
        Some(GalaxyRow::Log(log)) => log_entry(*log).map(|(_, text)| text),
        Some(GalaxyRow::Lead(lead)) => lead_entry(world, *lead).map(|(_, _, text)| text),
        Some(GalaxyRow::Stats) => Some(stats_words(world)),
        Some(GalaxyRow::Choose) => Some(finale::HEART_TEXT.to_owned()),
        _ => None,
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "The Builders' Road",
        "The galaxy story: the builders' trail through the stars.",
        &ModalContent::new(sections).footer(footer).prompts(&[
            hint(Prompt::Navigate, "read"),
            hint(Prompt::Confirm, "set course"),
            hint(Prompt::Tab, "pages"),
            hint(Prompt::Back, "close"),
        ]),
    );
}

/// The finale's choice page.
pub(super) fn draw_galaxy_choice(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let selected = ui.seat.selected_menu_item.min(CHOICE_ROWS.len() - 1);
    let mut items = vec![SectionItem::text(finale::HEART_TEXT.to_owned())];
    for (index, row) in CHOICE_ROWS.iter().enumerate() {
        let (icon, label, detail, colour) = match row {
            Some(FinaleChoice::RingTheBell) => (
                &menu_icons::SPEAKER,
                FinaleChoice::RingTheBell.label(),
                "Wake the sleepers",
                GOLD,
            ),
            Some(FinaleChoice::KeepTheLamp) => (
                &menu_icons::KEY,
                FinaleChoice::KeepTheLamp.label(),
                "Let them sleep; keep the road lit",
                AMBER,
            ),
            None => (
                &menu_icons::BACK,
                "Not yet",
                "Come back when you are ready",
                FOG,
            ),
        };
        items.push(SectionItem::item(ItemEntry {
            icon: Some(icon),
            label: label.to_owned(),
            detail: detail.to_owned(),
            value: String::new(),
            value_color: colour,
            coin: false,
            selected: index == selected,
            disabled: false,
        }));
    }
    let footer = CHOICE_ROWS
        .get(selected)
        .copied()
        .flatten()
        .map(|choice| choice.promise().to_owned());
    UiLayout::screen(draw).modal_with_render_plan(
        "The Harbour's Heart",
        "One choice, for the whole crew. Both are kind.",
        &ModalContent::new(vec![Section::new("The bell and the lamp", GOLD, items)])
            .footer(footer)
            .prompts(&[
                hint(Prompt::Navigate, "choose"),
                hint(Prompt::Confirm, "decide"),
                hint(Prompt::Back, "not yet"),
            ]),
    );
}

/// How wide the goal strip is (at most `max_width`), or `None` while the road is not under way
/// (Wave 13 integration: the space HUD's layout reserves its banner before drawing).
pub(super) fn goal_strip_width(
    world: &fatherload_simulation::WorldState,
    max_width: i32,
) -> Option<i32> {
    goal_line(world).map(|(title, line)| strip_width(&title, &line, max_width))
}

fn strip_width(title: &str, line: &str, max_width: i32) -> i32 {
    (ui_kit::text_width(line, TextStyle::Body).max(ui_kit::text_width(title, TextStyle::Bold)) + 22)
        .min(max_width)
}

/// The goal line on the HUD: the chapter and what to do now, in a slim plate at `(x, y)`
/// centred on `middle` (only while the road is under way). Returns its height.
pub(super) fn draw_goal_strip(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    world: &fatherload_simulation::WorldState,
    middle: i32,
    y: i32,
    max_width: i32,
) -> i32 {
    // In a fight the boss's bar and warnings have the top of the screen.
    if world.hostiles().engaged(view.player_id).is_some() {
        return 0;
    }
    // The ground's compass first: the relic (or a lead's ruin) on this world, then the goal.
    let (title, line) = match (goal_line(world), site_words(world, view.player_id)) {
        (Some((title, line)), Some(site)) => (title, format!("{site}. {line}")),
        (Some(goal), None) => goal,
        (None, Some(site)) => ("Discovery log".to_owned(), site),
        (None, None) => return 0,
    };
    let story = world.galaxy_story();
    let colour = chapter_colour(story.chapter);
    let width = strip_width(&title, &line, max_width);
    let text = ui_kit::truncate(&line, width - 14, TextStyle::Body);
    let rect = Rectangle::new((middle - width / 2) as f32, y as f32, width as f32, 26.0);
    ui_kit::panel(draw, rect, PanelStyle::Standard);
    // A waymark lamp at the plate's left edge, brightening slowly.
    let glow = 160.0 + (ui_kit::seconds() * 1.4).sin() * 60.0;
    draw.draw_rectangle(
        rect.x as i32 + 4,
        y + 5,
        3,
        16,
        with_alpha(colour, glow as u8),
    );
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(&title, width - 14, TextStyle::Bold),
        middle + 2,
        y + 3,
        TextStyle::Bold,
        colour,
    );
    ui_kit::text_centred(draw, &text, middle + 2, y + 14, TextStyle::Body, CREAM);
    // A hold's progress under the plate.
    if let Some(goal) = story.current_goal()
        && let Goal::Hold { seconds, .. } = goal.goal
        && story.raid_landed
    {
        let share = (story.held_millis as f32 / (f32::from(seconds) * 1_000.0)).clamp(0.0, 1.0);
        let bar = Rectangle::new(
            rect.x + 6.0,
            rect.y + 23.0,
            (rect.width - 12.0) * share,
            2.0,
        );
        draw.draw_rectangle_rec(bar, colour);
    }
    26
}

/// The road card's decoration: a dotted road of waymarks across the card's head, its lamps
/// lighting one by one toward the Harbour (slowly, never flashing).
pub(super) fn road_card(
    draw: &mut RaylibDrawHandle<'_>,
    (x, y, width, _height): (i32, i32, i32, i32),
    age: u64,
    colour: Color,
) {
    let marks = 9;
    let span = (width - 40).max(40);
    let lit = (age / 6).min(marks) as i32;
    for index in 0..marks as i32 {
        let px = x + 20 + span * index / (marks as i32 - 1);
        let py = y + 3 + ((index * 7) % 3);
        let on = index < lit;
        draw.draw_rectangle(px, py, 2, 2, if on { colour } else { SLATE });
        if on {
            draw.draw_rectangle(px - 1, py - 1, 4, 4, with_alpha(colour, 50));
        }
    }
}

/// The Harbour in the space view (Wave 13 story): ten world-ships moored in a ring round its
/// red star, mooring lines between them, and the gate (two great pylons) on the side facing
/// home. Dark until the choice: the bell wakes their windows, the lamp lights the gate warm.
/// Drawn only once the crew knows where it is and the seat is in its reach.
#[allow(
    clippy::too_many_lines,
    reason = "the ring, the ships, the gate in drawing order"
)]
pub(super) fn draw_harbour(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &crate::render_snapshot::RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    frame: &super::space_view::SpaceFrame,
) {
    use fatherload_simulation::{
        TILE_SIZE, galaxy_story::plan::HARBOUR_RING_TILES, space::SpaceTile,
    };
    let world = snapshot.world;
    let Some(harbour) = world.galaxy_harbour() else {
        return;
    };
    let rig = SpaceTile::of_pos(view.player.flight.pos);
    if !harbour.star.within(rig, HARBOUR_RING_TILES * 6) {
        return;
    }
    let story = world.galaxy_story();
    let bell = story.choice == Some(FinaleChoice::RingTheBell);
    let lamp = story.choice == Some(FinaleChoice::KeepTheLamp);
    let seconds = ui_kit::seconds();
    let star = frame.project(harbour.star.pos());
    // The mooring ring: a faint line of cable round the star.
    let (gx, gy) = (
        (harbour.gate.x - harbour.star.x) as f32,
        (harbour.gate.y - harbour.star.y) as f32,
    );
    let gate_angle = gy.atan2(gx);
    let segments = 96;
    let mut previous: Option<Vector2> = None;
    for index in 0..=segments {
        let angle = gate_angle + index as f32 / segments as f32 * std::f32::consts::TAU;
        let (dx, dy) = (
            angle.cos() * HARBOUR_RING_TILES as f32,
            angle.sin() * HARBOUR_RING_TILES as f32,
        );
        let at = frame.offset_to_screen(
            view.player.flight.pos.delta(harbour.star.pos()).0 + dx * TILE_SIZE,
            view.player.flight.pos.delta(harbour.star.pos()).1 + dy * TILE_SIZE,
        );
        if let Some(from) = previous {
            draw.draw_line_ex(
                from,
                at,
                1.5,
                with_alpha(if lamp { GOLD } else { STEEL }, 90),
            );
        }
        previous = Some(at);
    }
    // Ten world-ships, the gate between the first and the last.
    let (ox, oy) = view.player.flight.pos.delta(harbour.star.pos());
    for ship in 0..10 {
        let angle = gate_angle + (ship as f32 + 0.5) / 10.0 * std::f32::consts::TAU;
        let (cx, cy) = (angle.cos(), angle.sin());
        let centre = (
            ox + cx * HARBOUR_RING_TILES as f32 * TILE_SIZE,
            oy + cy * HARBOUR_RING_TILES as f32 * TILE_SIZE,
        );
        // A hull along the ring: a row of overlapping discs, thicker in the middle.
        let length = 14_000.0 * TILE_SIZE;
        let (tx, ty) = (-cy, cx);
        for part in 0..9 {
            let t = part as f32 / 8.0 - 0.5;
            let thickness = (1.0 - (t * 2.0).powi(2)).max(0.25) * 2_600.0 * TILE_SIZE;
            let at = frame.offset_to_screen(centre.0 + tx * t * length, centre.1 + ty * t * length);
            let radius = (thickness * frame.scale).max(1.5);
            draw.draw_circle_v(at, radius, Color::new(36, 40, 58, 255));
            draw.draw_circle_v(
                Vector2::new(at.x - radius * 0.25, at.y - radius * 0.25),
                radius * 0.7,
                Color::new(58, 64, 86, 255),
            );
            // Windows: dark, or awake after the bell (a slow wave of light along the hull).
            let lit =
                bell && ((seconds * 0.6 + ship as f32 * 0.7 + part as f32 * 0.3).sin() > -0.4);
            if radius > 3.0 {
                let window = if lit {
                    CREAM
                } else {
                    Color::new(24, 26, 40, 255)
                };
                draw.draw_rectangle(at.x as i32, at.y as i32, 2, 2, window);
                draw.draw_rectangle(
                    (at.x - radius * 0.45) as i32,
                    (at.y + radius * 0.2) as i32,
                    2,
                    2,
                    window,
                );
            }
        }
        // The spine's running lights and the bow's lamp (teal asleep, gold awake).
        let running = if bell { GOLD } else { TEAL_DARK };
        for light in 0..7 {
            let t = light as f32 / 6.0 - 0.5;
            let at = frame.offset_to_screen(centre.0 + tx * t * length, centre.1 + ty * t * length);
            draw.draw_rectangle(at.x as i32, at.y as i32 - 1, 1, 1, running);
        }
        let bow =
            frame.offset_to_screen(centre.0 + tx * 0.55 * length, centre.1 + ty * 0.55 * length);
        draw.draw_circle_v(bow, 3.0, with_alpha(if bell { GOLD } else { TEAL }, 90));
        draw.draw_circle_v(bow, 1.5, if bell { CREAM } else { MINT });
    }
    // The gate: two great pylons on the ring facing home, their lamps.
    for side in [-1.0_f32, 1.0] {
        let angle = gate_angle + side * 0.035;
        let base = (
            ox + angle.cos() * HARBOUR_RING_TILES as f32 * TILE_SIZE,
            oy + angle.sin() * HARBOUR_RING_TILES as f32 * TILE_SIZE,
        );
        let at = frame.offset_to_screen(base.0, base.1);
        let size = (1_800.0 * TILE_SIZE * frame.scale).max(3.0);
        draw.draw_circle_v(
            at,
            size * 2.2,
            with_alpha(if lamp { GOLD } else { TEAL }, 40),
        );
        draw.draw_circle_v(at, size, BONE);
        draw.draw_circle_v(at, size * 0.45, if lamp || bell { GOLD } else { TEAL });
    }
    // The star's glow is the space renderer's; a dim red halo marks the ring's heart.
    draw.draw_circle_v(
        star,
        (3_000.0 * TILE_SIZE * frame.scale).max(2.0),
        with_alpha(RED, 50),
    );
}

/// The galaxy story's waymarks on the ground of `scene`'s world (Wave 13): the current
/// chapter's relic and the open leads' ruin marks, at their sites (a buried relic at its row
/// in the rock, the rest standing on the surface).
#[must_use]
pub fn waymark_things(
    world: &fatherload_simulation::WorldState,
    scene: &crate::render_snapshot::PlaceScene<'_>,
) -> Vec<super::space::view::GroundThing> {
    use super::space::view::{GroundThing, GroundThingKind};
    let fatherload_simulation::place::StoreKey::Body(body) = scene.store else {
        return Vec::new();
    };
    let Some(ground) = scene.terrain.planet_ground() else {
        return Vec::new();
    };
    let mut things = Vec::new();
    let mut place = |site: fatherload_simulation::galaxy_story::plan::RelicSite, relic: bool| {
        let column = i32::try_from(site.column).unwrap_or(0);
        things.push(GroundThing {
            column: f64::from(column) + 0.5,
            row: site.row_on(ground) as f32,
            kind: GroundThingKind::Waymark {
                relic,
                buried: site.rows > 0,
            },
        });
    };
    if let Some((on, site, _)) = world.galaxy_relic()
        && on == body
    {
        place(site, true);
    }
    for site in world.galaxy_lead_sites(body) {
        place(site, false);
    }
    things
}

/// The ending of the seat, from the finale's choice (set by the game loop from the seat's
/// [`FinaleView`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinaleSeat {
    pub phase: FinalePhase,
    pub elapsed: u64,
    pub view: FinaleView,
    pub choice: FinaleChoice,
}

/// A deterministic starfield, twinkling gently (brightness only, slowly).
fn stars(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, seconds: f32, count: u32) {
    for index in 0..count {
        let hash = index.wrapping_mul(2_654_435_761) ^ 0x51A7;
        let fx = (hash % 991) as f32 / 991.0;
        let fy = ((hash / 991) % 983) as f32 / 983.0;
        let twinkle = 0.65 + 0.35 * (seconds * 0.7 + index as f32).sin();
        let alpha = (twinkle * 200.0) as u8;
        draw.draw_rectangle(
            (rect.x + fx * rect.width) as i32,
            (rect.y + fy * rect.height) as i32,
            1,
            1,
            with_alpha(if index % 7 == 0 { SKY_LIGHT } else { MIST }, alpha),
        );
    }
}

/// The road drawn as a line of waymarks from home (bottom left) to the Harbour (top right),
/// lit up to `lit` (0..=1) of the way.
fn road(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, lit: f32, seconds: f32) {
    let home = Vector2::new(rect.x + rect.width * 0.08, rect.y + rect.height * 0.86);
    let harbour = Vector2::new(rect.x + rect.width * 0.30, rect.y + rect.height * 0.16);
    let marks = 11;
    for index in 0..=marks {
        let share = index as f32 / marks as f32;
        let wobble = (share * 9.0).sin() * rect.width * 0.03;
        let at = Vector2::new(
            home.x + (harbour.x - home.x) * share + wobble,
            home.y + (harbour.y - home.y) * share,
        );
        let on = share <= lit;
        if on {
            draw.draw_circle_v(at, 5.0, with_alpha(GOLD, 40));
        }
        draw.draw_rectangle(
            at.x as i32 - 1,
            at.y as i32 - 1,
            3,
            3,
            if on { GOLD } else { SLATE },
        );
    }
    // Home: a small blue world with its Moon; the Harbour: a ring round a red star.
    draw.draw_circle_v(home, 9.0, OCEAN);
    draw.draw_circle_v(Vector2::new(home.x - 3.0, home.y - 3.0), 3.0, SKY_LIGHT);
    draw.draw_circle_v(Vector2::new(home.x + 14.0, home.y - 10.0), 3.0, BONE);
    let pulse = 0.85 + 0.15 * (seconds * 0.8).sin();
    draw.draw_circle_v(harbour, 7.0 * pulse, RED);
    draw.draw_circle_v(harbour, 4.0, ORANGE);
    for ship in 0..10 {
        let angle = ship as f32 / 10.0 * std::f32::consts::TAU + seconds * 0.05;
        let at = Vector2::new(
            harbour.x + angle.cos() * 20.0,
            harbour.y + angle.sin() * 9.0,
        );
        draw.draw_rectangle(
            at.x as i32 - 2,
            at.y as i32 - 1,
            4,
            2,
            if lit >= 1.0 { CREAM } else { STEEL },
        );
    }
}

/// The seat's ending over its view: the lead card over the world, the credits and the
/// epilogue full screen. `skip` names the seat's skip button.
pub(super) fn draw_finale(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    skip: &str,
) {
    let Some(finale) = view.galaxy_finale else {
        return;
    };
    let seconds = ui_kit::seconds();
    match finale.phase {
        FinalePhase::Lead => {
            let since = finale.elapsed as f32 / 1_000.0;
            draw_lead(draw, rect, finale.choice, since);
            // The flight HUD is still up under the lead: its prompt row has the corner, so
            // the hint sits on a plate above it.
            let text = format!("{skip}: skip");
            let width = ui_kit::text_width(&text, TextStyle::Body) + 12;
            let plate = Rectangle::new(
                rect.x + rect.width - width as f32 - 6.0,
                rect.y + rect.height - 40.0,
                width as f32,
                16.0,
            );
            ui_kit::panel(draw, plate, PanelStyle::Standard);
            ui_kit::text(
                draw,
                &text,
                plate.x as i32 + 6,
                plate.y as i32 + 3,
                TextStyle::Body,
                CREAM,
            );
        }
        FinalePhase::Credits => {
            draw.draw_rectangle(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
                Color::new(5, 4, 12, 255),
            );
            stars(draw, rect, seconds, 240);
            road(
                draw,
                rect,
                finale.view.credits_progress(finale.elapsed) * 1.4,
                seconds,
            );
            draw_credits(draw, rect, finale.view.credits_progress(finale.elapsed));
            corner_hint(draw, rect, &format!("{skip}: skip"));
        }
        FinalePhase::Epilogue => {
            let since = finale
                .elapsed
                .saturating_sub(finale.view.phase_start(finale.elapsed))
                as f32
                / 1_000.0;
            draw_epilogue(draw, rect, finale.choice, since, seconds);
            corner_hint(draw, rect, &format!("{skip}: continue"));
        }
        FinalePhase::Done => {}
    }
}

fn corner_hint(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, text: &str) {
    let width = ui_kit::text_width(text, TextStyle::Body);
    ui_kit::text(
        draw,
        text,
        (rect.x + rect.width) as i32 - width - 8,
        // Above the flight HUD's prompt row, which shows dimmed under the epilogue.
        (rect.y + rect.height) as i32 - 30,
        TextStyle::Body,
        FOG,
    );
}

/// The choice's moment over the Harbour: its title and lead, fading in.
fn draw_lead(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, choice: FinaleChoice, since: f32) {
    let alpha = (since / 1.5).clamp(0.0, 1.0);
    let a = (alpha * 255.0) as u8;
    let width = (rect.width as i32 - 40).min(330);
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let text_width = width - 2 * ui_kit::PANEL_PADDING - 8;
    let lines = ui_kit::wrap(finale::lead(choice), text_width, TextStyle::Body);
    let height = 40 + lines.len() as i32 * TextStyle::Body.line_height();
    // Low in the view: the Harbour and the rig stay in sight above it, the HUD's panels clear.
    let y = rect.y as i32 + rect.height as i32 * 3 / 5;
    draw.draw_rectangle(
        x - 4,
        y - 4,
        width + 8,
        height + 8,
        with_alpha(INK, (alpha * 120.0) as u8),
    );
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    ui_kit::text_centred(
        draw,
        finale::title(choice),
        x + width / 2,
        y + 8,
        TextStyle::Title,
        with_alpha(GOLD, a),
    );
    ui_kit::paragraph(
        draw,
        finale::lead(choice),
        x + ui_kit::PANEL_PADDING + 4,
        y + 30,
        text_width,
        TextStyle::Body,
        with_alpha(CREAM, a),
    );
}

/// The credits roll, rising from the bottom to past the top, right of the road.
fn draw_credits(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, progress: f32) {
    let column = (rect.x + rect.width * 0.64) as i32;
    let gap = 16;
    let total = finale::CREDITS.len() as i32 * gap + rect.height as i32;
    let offset = (progress * total as f32) as i32;
    let mut y = (rect.y + rect.height) as i32 - offset;
    let fade = |y: i32| {
        let top = (y as f32 - rect.y) / 30.0;
        let bottom = (rect.y + rect.height - y as f32) / 30.0;
        (top.min(bottom).clamp(0.0, 1.0) * 255.0) as u8
    };
    for line in &finale::CREDITS {
        match line {
            CreditLine::Heading(text) => {
                ui_kit::text_centred(
                    draw,
                    text,
                    column,
                    y,
                    TextStyle::Bold,
                    with_alpha(AMBER, fade(y)),
                );
                y += gap;
            }
            CreditLine::Name(text) => {
                ui_kit::text_centred(
                    draw,
                    text,
                    column,
                    y,
                    TextStyle::Body,
                    with_alpha(CREAM, fade(y)),
                );
                y += gap;
            }
            CreditLine::Gap => y += gap,
        }
    }
}

/// The epilogue card: the choice's beats, one after another.
fn draw_epilogue(
    draw: &mut RaylibDrawHandle<'_>,
    rect: Rectangle,
    choice: FinaleChoice,
    since: f32,
    seconds: f32,
) {
    draw.draw_rectangle(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
        Color::new(5, 4, 12, 236),
    );
    stars(draw, rect, seconds, 100);
    let beats = finale::epilogue(choice);
    // A beat every 4.5 s; the last one stays.
    let shown = ((since / 4.5) as usize).min(beats.len() - 1);
    let beat = beats[shown];
    let alpha = ((since - shown as f32 * 4.5) / 1.0).clamp(0.0, 1.0);
    let a = (alpha * 255.0) as u8;
    let width = (rect.width as i32 - 40).min(400);
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let text_width = width - 2 * ui_kit::PANEL_PADDING - 8;
    let lines = ui_kit::wrap(beat.line, text_width, TextStyle::Body);
    let height = 58 + lines.len() as i32 * TextStyle::Body.line_height();
    let y = rect.y as i32 + (rect.height as i32 - height) / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    ui_kit::text_centred(
        draw,
        finale::title(choice),
        x + width / 2,
        y + 7,
        TextStyle::Title,
        GOLD,
    );
    ui_kit::text_centred(
        draw,
        beat.place,
        x + width / 2,
        y + 28,
        TextStyle::Bold,
        with_alpha(AMBER, a),
    );
    ui_kit::paragraph(
        draw,
        beat.line,
        x + ui_kit::PANEL_PADDING + 4,
        y + 42,
        text_width,
        TextStyle::Body,
        with_alpha(CREAM, a),
    );
    // Beat dots under the card.
    for index in 0..beats.len() {
        let on = index <= shown;
        draw.draw_rectangle(
            x + width / 2 - beats.len() as i32 * 5 + index as i32 * 10,
            y + height + 6,
            4,
            4,
            if on { GOLD } else { SLATE },
        );
    }
}
