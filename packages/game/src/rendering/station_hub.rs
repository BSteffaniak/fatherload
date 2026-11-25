//! A docked station's hub on screen (Wave 12): the station's interior behind its counter panel.
//!
//! Docking puts the seat inside the station. The counter panel (the hub's rows, the same layout
//! as a town building's counter: beside the room on a wide view, along the bottom of a narrow
//! split-screen one) sits over a pixel-art interior drawn for the page and the station's owner:
//!
//! * **who runs it**: a human station's riveted steel bulkheads and warm strip lights, an
//!   alien-run station's grown, curving ribs in teal and violet with glowing pods, a derelict's
//!   dark, scorched plating, dead lights and drifting debris;
//! * **the page**: the concourse (a directory arch and spacers passing), the shipyard (a hull on
//!   its gantry, a welder's steady glow, parts crates), the market (stacked goods, a price
//!   board), the bounty board (pinned notices, one per offer, and a wanted poster) and the bar
//!   (bottles, the counter, its patrons: the people engine's characters, alien kin drawn as
//!   themselves, and their speech);
//! * a window onto space with the world the station keeps station with.
//!
//! Nothing flashes: lights hold steady or breathe slowly, and a derelict's lamp only dims.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::while_float,
    clippy::too_many_lines,
    reason = "screen layout mixes whole pixels and float positions; art walks its pixels"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    station::{
        Good, ShipUpgrade, StationKind, StationOwner, StationProfile,
        bar::{ALIEN_ROUND_PRICE, Patron, ROUND_PRICE},
        bounty::describe,
        market::Role,
        station_hash,
    },
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{self, ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    people_art::{self, PersonPen, Pose},
    pixel_art::sprites::{icons, menu_icons},
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    shell::station::{StationPage, StationRow, station_rows},
};

/// An art-pixel rectangle.
fn block(draw: &mut impl RaylibDraw, x: f32, y: f32, w: f32, h: f32, colour: Color) {
    if w > 0.0 && h > 0.0 {
        draw.draw_rectangle_rec(
            Rectangle::new(x.floor(), y.floor(), w.floor(), h.floor()),
            colour,
        );
    }
}

/// A small deterministic roll from a seed and an index.
const fn roll(seed: u64, index: u64) -> u64 {
    let mut z = seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The station's colours: wall, wall shade, trim, light, floor.
struct Palette {
    wall: Color,
    shade: Color,
    trim: Color,
    light: Color,
    floor: Color,
}

const fn palette_of(owner: StationOwner) -> Palette {
    match owner {
        StationOwner::Human => Palette {
            wall: SLATE,
            shade: SLATE_DARK,
            trim: STEEL,
            light: CREAM,
            floor: STEEL_DARK,
        },
        StationOwner::Alien(_) => Palette {
            wall: TEAL_DARK,
            shade: NIGHT,
            trim: VIOLET,
            light: MINT,
            floor: PLUM,
        },
        StationOwner::Derelict => Palette {
            wall: STONE_DARK,
            shade: INK,
            trim: RUST,
            light: AMBER,
            floor: NIGHT,
        },
    }
}

/// Where the interior is drawn: the view minus the counter panel.
fn stage_rect(viewport: Rectangle) -> Rectangle {
    if layout::counter_panel_is_sheet(viewport) {
        let height = (viewport.height * 0.4).floor();
        layout::set_counter_sheet_top(Some(height));
        Rectangle::new(viewport.x, viewport.y, viewport.width, height)
    } else {
        let panel = (viewport.width * 0.52)
            .clamp(250.0, 330.0)
            .min(viewport.width - 16.0)
            .max(0.0);
        let x = (viewport.x + 8.0 + panel + 4.0).floor();
        Rectangle::new(
            x,
            viewport.y,
            (viewport.x + viewport.width - x).max(0.0),
            viewport.height,
        )
    }
}

/// The hub on screen (see the module docs).
pub(super) fn draw_station_hub(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let world = ui.snapshot.space.world;
    let player_id = ui.view.player_id;
    let Some(profile) = world.docked_station(player_id) else {
        return;
    };
    let page = ui.seat.station.page;
    let seconds = ui_kit::seconds();
    let viewport = ui_kit::canvas().rect();
    draw_interior(draw, viewport, &profile, page, world, seconds);
    let rows = station_rows(world, player_id, &profile, page);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let mut sections: Vec<(&'static str, Color, Vec<SectionItem>)> = Vec::new();
    let mut footer = String::new();
    for (index, row) in rows.iter().enumerate() {
        let picked = index == selected;
        let (group, colour) = group_of(*row);
        let (entry, note) = entry_for(world, player_id, &profile, *row, picked, ui);
        if picked {
            footer = note;
        }
        match sections.last_mut() {
            Some((title, _, items)) if *title == group => items.push(SectionItem::item(entry)),
            _ => sections.push((group, colour, vec![SectionItem::item(entry)])),
        }
    }
    let hold = world.station_ledger().hold_used(player_id);
    let capacity = world.hold_capacity_of(player_id);
    let status = format!(
        "Credits {} | Hold {hold}/{capacity} | Missions {}/3",
        ui.player.credits,
        world.station_ledger().missions_of(player_id).len()
    );
    let footer = if footer.is_empty() {
        status
    } else {
        format!("{footer}\n{status}")
    };
    let subtitle = format!(
        "{} {}, {}. {}",
        capitalised(&profile.owner.label()),
        profile.kind.name(),
        page.title(),
        page_line(page, &profile)
    );
    UiLayout::screen(draw).modal_with_render_plan(
        &profile.name,
        &subtitle,
        &ModalContent::new(
            sections
                .into_iter()
                .map(|(title, colour, items)| Section::new(title, colour, items))
                .collect(),
        )
        .footer(Some(footer))
        .compact()
        .prompts(&[
            hint(Prompt::Navigate, "pick"),
            hint(Prompt::Confirm, "choose"),
            hint(Prompt::Back, "back"),
        ]),
    );
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A line under the title for each page.
const fn page_line(page: StationPage, profile: &StationProfile) -> &'static str {
    match page {
        StationPage::Hub if profile.salvage => "Dead and cold. Whatever is left is yours.",
        StationPage::Hub => "Docked. The air is topped up.",
        StationPage::Shipyard => "Refits for the long haul.",
        StationPage::Market => "Fill the hold where it's cheap, empty it where it's wanted.",
        StationPage::Board => "Work for hire. Paid at any board, no clock running.",
        StationPage::Bar => "Buy a round and people talk.",
    }
}

/// The section a row sits in.
const fn group_of(row: StationRow) -> (&'static str, Color) {
    match row {
        StationRow::Open(_) => ("Counters", SKY_LIGHT),
        StationRow::Refuel | StationRow::Repair | StationRow::Salvage => ("Services", MINT),
        StationRow::Fit(ShipUpgrade::Line(_)) => ("Ship lines", GOLD),
        StationRow::Fit(ShipUpgrade::Gear(_)) => ("Planet gear", ORANGE),
        StationRow::Fit(ShipUpgrade::Alien(_)) => ("Alien tech", LAVENDER),
        StationRow::Buy(_) => ("Flight gear", SKY_LIGHT),
        StationRow::SellOre => ("Ore", COPPER),
        StationRow::BuyGood(_) => ("Buy", MINT),
        StationRow::SellGood(_) => ("Sell", GOLD),
        StationRow::TurnIn | StationRow::Mission(_) => ("Under way", GOLD),
        StationRow::Offer(_) => ("Offered", SKY_LIGHT),
        StationRow::Patron(_) => ("Patrons", ROSE),
        StationRow::Save | StationRow::Undock | StationRow::Back | StationRow::Close => ("", FOG),
    }
}

fn entry(
    icon: &'static super::pixel_art::sprite::SpriteDef,
    label: impl Into<String>,
    detail: impl Into<String>,
    value: Option<u32>,
    picked: bool,
    disabled: bool,
) -> ItemEntry {
    ItemEntry {
        icon: Some(icon),
        label: label.into(),
        detail: detail.into(),
        value: value.map_or_else(String::new, |value| value.to_string()),
        value_color: GOLD,
        coin: value.is_some(),
        selected: picked,
        disabled,
    }
}

/// A row's entry and the note the footer shows while it is picked.
#[allow(clippy::too_many_lines, reason = "one arm per row kind")]
fn entry_for(
    world: &WorldState,
    player_id: PlayerId,
    profile: &StationProfile,
    row: StationRow,
    picked: bool,
    ui: &UiFrame<'_, '_>,
) -> (ItemEntry, String) {
    let player = ui.player;
    let credits = player.credits;
    let ledger = world.station_ledger();
    let galaxy = world.space();
    let space = galaxy.space();
    match row {
        StationRow::Open(page) => {
            let (icon, detail) = match page {
                StationPage::Shipyard => (&menu_icons::ROCKET, "hull, engine, drive, hold, gear"),
                StationPage::Market => (&menu_icons::BANK, "goods and ore"),
                StationPage::Board => (&menu_icons::CONTRACT, "missions for hire"),
                StationPage::Bar => (&menu_icons::PERSON, "patrons and rumours"),
                StationPage::Hub => (&menu_icons::BACK, ""),
            };
            (
                entry(icon, page.title(), detail, None, picked, false),
                String::new(),
            )
        }
        StationRow::Refuel => {
            let missing = (player.fuel_capacity - player.fuel).max(0.0).ceil() as u32;
            let cost = missing * profile.fuel_price;
            (
                entry(
                    &icons::FUEL,
                    "Refuel",
                    format!("{missing} units at {} each", profile.fuel_price),
                    Some(cost),
                    picked,
                    missing == 0,
                ),
                String::new(),
            )
        }
        StationRow::Repair => {
            let missing = (player.max_hull() - player.hull).max(0.0).ceil() as u32;
            (
                entry(
                    &icons::HULL,
                    "Repair",
                    format!("{missing} hull at {} each", profile.repair_price),
                    Some(missing * profile.repair_price),
                    picked,
                    missing == 0,
                ),
                String::new(),
            )
        }
        StationRow::Salvage => {
            let stripped = ledger.salvaged.contains(&profile.id);
            (
                entry(
                    &menu_icons::SCRAP,
                    "Strip the wreck",
                    if stripped {
                        "stripped bare"
                    } else {
                        "credits and salvage for the hold"
                    },
                    None,
                    picked,
                    stripped,
                ),
                "Whatever the crew left behind: credits in the safe, alloy in the frames, \
                 maybe a builders' relic."
                    .to_owned(),
            )
        }
        StationRow::Buy(kind) => {
            let level = kind.level(&player.hazard_gear);
            let maxed = level >= kind.max_level();
            (
                entry(
                    &menu_icons::GEAR,
                    kind.name(),
                    if kind.max_level() > 1 {
                        format!("tier {level}/{}", kind.max_level())
                    } else if maxed {
                        "fitted (switch it on in Options)".to_owned()
                    } else {
                        "not fitted".to_owned()
                    },
                    (!maxed).then(|| kind.price(level)),
                    picked,
                    maxed || credits < kind.price(level),
                ),
                String::new(),
            )
        }
        StationRow::Fit(upgrade) => {
            let level = upgrade.level(&player.hazard_gear.ship);
            let sold = upgrade.sold_to(profile);
            let maxed = level >= upgrade.max_level();
            let yard_limit = !maxed && level >= sold;
            let price = upgrade.price(level);
            let label = match upgrade {
                ShipUpgrade::Line(line) if level > 0 => {
                    format!("{}: {}", upgrade.name(), line.tier_name(level))
                }
                _ => upgrade.name().to_owned(),
            };
            let detail = if maxed {
                "top tier fitted".to_owned()
            } else if yard_limit {
                format!(
                    "tier {level}/{}: a bigger yard fits more",
                    upgrade.max_level()
                )
            } else {
                format!(
                    "tier {level}/{}: {}",
                    upgrade.max_level(),
                    upgrade.effect(level)
                )
            };
            let note = match upgrade {
                ShipUpgrade::Line(line) if !maxed => {
                    format!(
                        "Next: {}. {}.",
                        line.tier_name(level + 1),
                        upgrade.effect(level)
                    )
                }
                ShipUpgrade::Gear(gear) => format!(
                    "Counters a world's {}: tier 1 the mild kind (the severe kind is halved), \
                     tier 2 both.",
                    gear.counters().name()
                ),
                ShipUpgrade::Alien(tech) => format!("{}: {}.", tech.name(), tech.effect()),
                ShipUpgrade::Line(_) => String::new(),
            };
            (
                entry(
                    &menu_icons::ANVIL,
                    label,
                    detail,
                    (!maxed && !yard_limit).then_some(price),
                    picked,
                    maxed || yard_limit || credits < price,
                ),
                note,
            )
        }
        StationRow::SellOre => {
            let offer = world.ore_offer(player_id, profile);
            (
                entry(
                    &icons::CARGO,
                    "Sell ore and relics",
                    format!("{}% of the home price", profile.ore_percent),
                    Some(offer),
                    picked,
                    offer == 0,
                ),
                String::new(),
            )
        }
        StationRow::BuyGood(good) => {
            let quote = world.station_quote(profile, good);
            let room = world
                .hold_capacity_of(player_id)
                .saturating_sub(ledger.hold_used(player_id));
            let role = quote.and_then(|quote| quote.role);
            (
                entry(
                    good_icon(good),
                    format!("Buy {}", good.name()),
                    role.map_or("", Role::label),
                    quote.map(|quote| quote.buy),
                    picked,
                    room == 0 || quote.is_none_or(|quote| credits < quote.buy),
                ),
                format!("{room} room in the hold."),
            )
        }
        StationRow::SellGood(good) => {
            let quote = world.station_quote(profile, good);
            let aboard = ledger.hold(player_id).map_or(0, |hold| hold.count(good));
            (
                entry(
                    good_icon(good),
                    format!("Sell {}", good.name()),
                    format!("{aboard} aboard"),
                    quote.map(|quote| quote.sell),
                    picked,
                    aboard == 0,
                ),
                String::new(),
            )
        }
        StationRow::TurnIn => {
            let missions = ledger.missions_of(player_id);
            let due: u32 = missions
                .iter()
                .filter(|mission| {
                    mission.ready()
                        || matches!(mission.kind,
                            fatherload_simulation::station::MissionKind::Deliver { to, .. }
                                if to == profile.id)
                })
                .map(|mission| mission.reward)
                .sum();
            (
                entry(
                    &menu_icons::MEDAL_GOLD,
                    "Collect rewards",
                    "finished missions, deliveries for here",
                    Some(due),
                    picked,
                    due == 0,
                ),
                String::new(),
            )
        }
        StationRow::Mission(index) => {
            let mission = ledger
                .missions_of(player_id)
                .get(usize::from(index))
                .copied();
            let Some(mission) = mission else {
                return (
                    entry(&menu_icons::CONTRACT, "-", "", None, picked, true),
                    String::new(),
                );
            };
            let armed = ui.seat.station.abandon_armed == Some(index);
            let detail = if armed {
                "confirm again to abandon".to_owned()
            } else if mission.ready() {
                "done: collect at any board".to_owned()
            } else {
                format!("{}/{}", mission.progress, mission.kind.goal())
            };
            (
                entry(
                    &menu_icons::CONTRACT,
                    describe(space, mission.kind),
                    detail,
                    Some(mission.reward),
                    picked,
                    false,
                ),
                "Confirm twice to abandon (a delivery's goods go back).".to_owned(),
            )
        }
        StationRow::Offer(slot) => {
            let mission = world
                .station_board(profile.id)
                .get(usize::from(slot))
                .copied()
                .flatten();
            let Some(mission) = mission else {
                return (
                    entry(&menu_icons::LETTER, "-", "", None, picked, true),
                    String::new(),
                );
            };
            let full = ledger.missions_of(player_id).len() >= 3;
            (
                entry(
                    &menu_icons::LETTER,
                    describe(space, mission.kind),
                    match mission.kind {
                        fatherload_simulation::station::MissionKind::Deliver { .. } => {
                            "the goods are loaded here"
                        }
                        fatherload_simulation::station::MissionKind::Scan { .. } => {
                            "ping the scanner near it, or on its ground"
                        }
                        fatherload_simulation::station::MissionKind::Mine { .. } => {
                            "dig the ore on its ground"
                        }
                    },
                    Some(mission.reward),
                    picked,
                    full,
                ),
                if full {
                    "Three missions under way: finish or abandon one first.".to_owned()
                } else {
                    String::new()
                },
            )
        }
        StationRow::Patron(index) => {
            let patrons = world.station_patrons(profile.id);
            let Some(patron) = patrons.get(usize::from(index)) else {
                return (
                    entry(&menu_icons::PERSON, "-", "", None, picked, true),
                    String::new(),
                );
            };
            let heard = ledger.heard.contains(&(profile.id, index));
            let price = if matches!(profile.owner, StationOwner::Alien(_)) {
                ALIEN_ROUND_PRICE
            } else {
                ROUND_PRICE
            };
            let note = match (&patron.rumour, heard) {
                (Some(rumour), true) => format!("\"{}\"", rumour.text),
                (Some(_), false) => format!("\"{}\" Buy a round to hear more.", patron.line),
                (None, _) => format!("\"{}\"", patron.line),
            };
            (
                entry(
                    &menu_icons::PERSON,
                    patron.name.clone(),
                    if heard {
                        format!("{}: rumour heard", patron.trade)
                    } else if patron.rumour.is_some() {
                        format!("{}: has a rumour", patron.trade)
                    } else {
                        patron.trade.to_owned()
                    },
                    patron.rumour.as_ref().map(|_| price),
                    picked,
                    patron.rumour.is_none() || credits < price,
                ),
                note,
            )
        }
        StationRow::Save => (
            entry(&menu_icons::DISK, "Save the game", "", None, picked, false),
            String::new(),
        ),
        StationRow::Undock => (
            entry(&menu_icons::ROCKET, "Undock", "", None, picked, false),
            String::new(),
        ),
        StationRow::Back => (
            entry(&menu_icons::BACK, "Back", "", None, picked, false),
            String::new(),
        ),
        StationRow::Close => (
            entry(&menu_icons::BACK, "Close", "", None, picked, false),
            String::new(),
        ),
    }
}

const fn good_icon(good: Good) -> &'static super::pixel_art::sprite::SpriteDef {
    match good {
        Good::WaterIce => &icons::AIR,
        Good::Hydroponics | Good::AlienSpice => &menu_icons::GIFT,
        Good::MachineParts | Good::SalvagedAlloy => &menu_icons::GEAR,
        Good::MedKits => &icons::HULL,
        Good::ReactorCells => &icons::FUEL,
        Good::XenoSilk => &menu_icons::FOLDER,
        Good::BuilderRelic => &menu_icons::KEY,
    }
}

// --- The interior ---------------------------------------------------------------------------

/// The station's interior over the whole view (the panel covers its left part).
fn draw_interior(
    draw: &mut RaylibDrawHandle<'_>,
    viewport: Rectangle,
    profile: &StationProfile,
    page: StationPage,
    world: &WorldState,
    seconds: f32,
) {
    let colours = palette_of(profile.owner);
    let stage = stage_rect(viewport);
    // The back wall over everything.
    block(
        draw,
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height,
        colours.shade,
    );
    let floor_y = (stage.y + stage.height * 0.78).floor();
    draw_walls(draw, viewport, floor_y, profile, &colours, seconds);
    if stage.width < 70.0 || stage.height < 60.0 {
        return;
    }
    // The window onto space, high on the stage.
    let window = Rectangle::new(
        stage.x + stage.width * 0.5,
        stage.y + stage.height * 0.08,
        (stage.width * 0.42).min(150.0),
        (stage.height * 0.3).min(80.0),
    );
    draw_window(draw, window, profile, &colours, seconds);
    let left = stage.x + 10.0;
    match page {
        StationPage::Hub => draw_concourse(
            draw, stage, left, floor_y, profile, world, &colours, seconds,
        ),
        StationPage::Shipyard => draw_shipyard(draw, stage, left, floor_y, &colours, seconds),
        StationPage::Market => draw_market(draw, stage, left, floor_y, profile, &colours, seconds),
        StationPage::Board => draw_board(draw, stage, left, floor_y, profile, world, &colours),
        StationPage::Bar => draw_bar(
            draw, stage, left, floor_y, profile, world, &colours, seconds,
        ),
    }
}

/// Bulkheads, ribs, strip lights and the floor (by owner).
fn draw_walls(
    draw: &mut impl RaylibDraw,
    viewport: Rectangle,
    floor_y: f32,
    profile: &StationProfile,
    colours: &Palette,
    seconds: f32,
) {
    let (x0, w) = (viewport.x, viewport.width);
    let top = viewport.y;
    block(draw, x0, top + 14.0, w, floor_y - top - 14.0, colours.wall);
    // Ceiling.
    block(draw, x0, top, w, 14.0, colours.shade);
    block(draw, x0, top + 12.0, w, 2.0, colours.trim);
    match profile.owner {
        StationOwner::Human => {
            // Riveted panels with seams, a warm strip light per bay.
            let bay = 48.0;
            let mut x = x0 - (x0 % bay);
            while x < x0 + w {
                block(
                    draw,
                    x,
                    top + 14.0,
                    3.0,
                    floor_y - top - 14.0,
                    colours.shade,
                );
                block(
                    draw,
                    x + 3.0,
                    top + 14.0,
                    1.0,
                    floor_y - top - 14.0,
                    STEEL_LIGHT,
                );
                for y in [top + 22.0, floor_y - 10.0] {
                    block(draw, x + 8.0, y, 1.0, 1.0, STEEL_LIGHT);
                    block(draw, x + bay - 6.0, y, 1.0, 1.0, STEEL_LIGHT);
                }
                block(draw, x + 10.0, top + 4.0, bay - 20.0, 4.0, colours.light);
                block(draw, x + 10.0, top + 8.0, bay - 20.0, 1.0, GOLD);
                // A stencilled bay number.
                block(draw, x + 8.0, floor_y - 30.0, 10.0, 6.0, SLATE_DARK);
                block(draw, x + 9.0, floor_y - 29.0, 2.0, 4.0, AMBER);
                block(draw, x + 13.0, floor_y - 29.0, 4.0, 1.0, AMBER);
                x += bay;
            }
            // A warning stripe along the floor's edge.
            let mut x = x0;
            let mut index = 0;
            while x < x0 + w {
                block(
                    draw,
                    x,
                    floor_y - 3.0,
                    6.0,
                    3.0,
                    if index % 2 == 0 { AMBER } else { INK },
                );
                x += 6.0;
                index += 1;
            }
        }
        StationOwner::Alien(_) => {
            // Grown ribs curving up into the ceiling, glowing pods between them (they breathe).
            let rib = 40.0;
            let mut x = x0 - (x0 % rib);
            let mut index = 0_u64;
            while x < x0 + w {
                let height = floor_y - top - 14.0;
                for step in 0..(height as i32) {
                    let t = step as f32 / height;
                    let bend = (t * 3.0).sin() * 4.0;
                    block(
                        draw,
                        x + bend,
                        top + 14.0 + step as f32,
                        4.0,
                        1.0,
                        colours.trim,
                    );
                    block(
                        draw,
                        x + bend + 1.0,
                        top + 14.0 + step as f32,
                        1.0,
                        1.0,
                        LAVENDER,
                    );
                }
                let glow = 0.75 + 0.25 * (seconds * 0.8 + index as f32).sin();
                let pod_y = top + 30.0 + (roll(index, 3) % 20) as f32;
                let pod = Vector2::new(x + rib * 0.5, pod_y);
                draw.draw_circle_v(pod, 6.0, PLUM);
                draw.draw_circle_v(pod, 4.0, colours.light.alpha(glow));
                draw.draw_circle_v(Vector2::new(pod.x - 1.0, pod.y - 1.0), 1.5, CREAM);
                x += rib;
                index += 1;
            }
        }
        StationOwner::Derelict => {
            // Scorched plating, a torn panel, one lamp dimming slowly, hanging cables.
            let bay = 52.0;
            let mut x = x0 - (x0 % bay);
            let mut index = 0_u64;
            while x < x0 + w {
                block(draw, x, top + 14.0, 3.0, floor_y - top - 14.0, INK);
                let scorch = (roll(profile.id.index.into(), index) % 30) as f32;
                block(draw, x + 10.0, top + 30.0 + scorch, 18.0, 10.0, SOIL_DARK);
                block(draw, x + 14.0, top + 33.0 + scorch, 9.0, 4.0, INK);
                // A hanging cable.
                let sway = (seconds * 0.6 + index as f32).sin() * 2.0;
                for step in 0..16 {
                    block(
                        draw,
                        x + 30.0 + sway * step as f32 / 16.0,
                        top + 14.0 + step as f32,
                        1.0,
                        1.0,
                        STONE,
                    );
                }
                x += bay;
                index += 1;
            }
            let dim = 0.35 + 0.25 * (seconds * 0.5).sin().abs();
            block(
                draw,
                x0 + w * 0.6,
                top + 4.0,
                24.0,
                4.0,
                colours.light.alpha(dim),
            );
        }
    }
    // The floor.
    let bottom = viewport.y + viewport.height;
    block(draw, x0, floor_y, w, bottom - floor_y, colours.floor);
    let mut y = floor_y + 4.0;
    while y < bottom {
        block(draw, x0, y, w, 1.0, colours.shade);
        y += 5.0;
    }
}

/// The window onto space and the world the station keeps station with.
fn draw_window(
    draw: &mut impl RaylibDraw,
    window: Rectangle,
    profile: &StationProfile,
    colours: &Palette,
    seconds: f32,
) {
    block(
        draw,
        window.x - 3.0,
        window.y - 3.0,
        window.width + 6.0,
        window.height + 6.0,
        colours.trim,
    );
    block(
        draw,
        window.x - 1.0,
        window.y - 1.0,
        window.width + 2.0,
        window.height + 2.0,
        INK,
    );
    block(
        draw,
        window.x,
        window.y,
        window.width,
        window.height,
        Color::new(8, 7, 20, 255),
    );
    let seed = station_hash(0x57A7, profile.id, 0x77);
    for index in 0..40 {
        let r = roll(seed, index);
        let x = window.x + (r % 1_000) as f32 / 1_000.0 * window.width;
        let y = window.y + ((r >> 12) % 1_000) as f32 / 1_000.0 * window.height;
        let bright = (r >> 30) % 3;
        let colour = match bright {
            0 => FOG,
            1 => MIST,
            _ => CREAM,
        };
        block(draw, x, y, 1.0, 1.0, colour);
    }
    // The planet below, turning slowly (its bands drift).
    let colour = [OCEAN, CLAY, LEAF, SAND, ICE, ROSE][(seed % 6) as usize];
    let shade = [DEEP_BLUE, RUST, PINE, LOAM, SKY, PLUM][(seed % 6) as usize];
    let centre = Vector2::new(
        window.x + window.width * (0.3 + (seed >> 8) as f32 % 40.0 / 100.0),
        window.y + window.height * 1.15,
    );
    let radius = window.height * 0.75;
    draw.draw_circle_v(centre, radius + 3.0, colour.alpha(0.25));
    draw.draw_circle_v(centre, radius, shade);
    draw.draw_circle_v(
        Vector2::new(centre.x - radius * 0.15, centre.y - radius * 0.1),
        radius * 0.9,
        colour,
    );
    for band in 0..4 {
        let y = centre.y - radius * 0.8 + band as f32 * radius * 0.22;
        let drift = (seconds * 0.05 + band as f32).sin() * 6.0;
        block(
            draw,
            centre.x - radius * 0.6 + drift,
            y,
            radius * 1.1,
            2.0,
            shade.alpha(0.6),
        );
    }
    // Cover what spilled under the window.
    block(
        draw,
        window.x - 3.0,
        window.y + window.height + 3.0,
        window.width + 6.0,
        radius * 2.0,
        colours.wall,
    );
    block(
        draw,
        window.x - 3.0,
        window.y + window.height + 1.0,
        window.width + 6.0,
        2.0,
        colours.trim,
    );
    // Mullions.
    block(
        draw,
        window.x + window.width / 3.0,
        window.y,
        2.0,
        window.height,
        colours.trim,
    );
    block(
        draw,
        window.x + window.width * 2.0 / 3.0,
        window.y,
        2.0,
        window.height,
        colours.trim,
    );
}

/// The station's crew: a people-engine character for a human, `None` for alien kin.
fn draw_figure(
    draw: &mut impl RaylibDraw,
    patron: &Patron,
    feet: Vector2,
    facing: i32,
    talking: bool,
    seconds: f32,
) {
    if let Some(character) = &patron.character {
        people_art::draw_person_posed(
            draw,
            PersonPen {
                x: feet.x,
                y: feet.y,
                pixel: 1.0,
            },
            character,
            Pose {
                facing,
                stride: None,
                talking,
            },
            seconds,
        );
    } else {
        draw_alien(draw, patron.look, feet, facing, seconds);
    }
}

/// One of a station's alien kin: a tall, narrow figure in a robe, a domed head with big eyes,
/// frills or a crest, its colours from its seed. Breathes gently.
fn draw_alien(draw: &mut impl RaylibDraw, look: u64, feet: Vector2, facing: i32, seconds: f32) {
    let skins = [MINT, LAVENDER, SKY_LIGHT, ROSE, LIME];
    let robes = [VIOLET, TEAL_DARK, PLUM, DEEP_BLUE, MAROON];
    let skin = skins[(look % 5) as usize];
    let robe = robes[((look >> 5) % 5) as usize];
    let height = 26.0 + ((look >> 9) % 4) as f32;
    let breathe = (seconds * 1.4 + (look % 7) as f32).sin() * 0.6;
    let (x, y) = (feet.x, feet.y);
    // Robe (wide at the hem), arms folded.
    for row in 0..(height as i32 - 9) {
        let t = row as f32 / (height - 9.0);
        let half = 3.0 + t * 3.0;
        block(
            draw,
            x - half,
            y - t.mul_add(-(height - 9.0), height - 9.0) - 1.0,
            half * 2.0,
            1.0,
            robe,
        );
    }
    block(draw, x - 6.0, y - 2.0, 12.0, 2.0, INK.alpha(0.4));
    block(draw, x - 4.0, y - height + 14.0, 8.0, 2.0, robe.alpha(0.6));
    // Neck and head.
    let head_y = y - height + 4.0 + breathe;
    block(draw, x - 1.0, head_y + 4.0, 2.0, 3.0, skin);
    draw.draw_ellipse(x as i32, head_y as i32, 5.0, 5.0, skin);
    // Eyes, big and dark, a glint (looking where it faces).
    let eye = facing.signum() as f32;
    block(draw, x - 3.0 + eye, head_y - 1.0, 2.0, 3.0, INK);
    block(draw, x + 1.0 + eye, head_y - 1.0, 2.0, 3.0, INK);
    block(draw, x - 3.0 + eye, head_y - 1.0, 1.0, 1.0, CREAM);
    block(draw, x + 1.0 + eye, head_y - 1.0, 1.0, 1.0, CREAM);
    // A crest or frills.
    if (look >> 13).is_multiple_of(2) {
        for index in 0..3 {
            block(
                draw,
                x - 3.0 + index as f32 * 3.0,
                head_y - 8.0,
                1.0,
                3.0,
                robe,
            );
        }
    } else {
        block(draw, x - 7.0, head_y - 1.0, 2.0, 4.0, robe);
        block(draw, x + 5.0, head_y - 1.0, 2.0, 4.0, robe);
    }
}

#[allow(clippy::too_many_arguments, reason = "the stage's parts")]
fn draw_concourse(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    world: &WorldState,
    colours: &Palette,
    seconds: f32,
) {
    // A directory arch with the station's name.
    let arch = Rectangle::new(left, floor_y - 70.0, 70.0, 70.0);
    block(draw, arch.x, arch.y, arch.width, 8.0, colours.trim);
    block(draw, arch.x, arch.y, 6.0, arch.height, colours.trim);
    block(
        draw,
        arch.x + arch.width - 6.0,
        arch.y,
        6.0,
        arch.height,
        colours.trim,
    );
    block(
        draw,
        arch.x + 6.0,
        arch.y + 8.0,
        arch.width - 12.0,
        arch.height - 8.0,
        INK,
    );
    for (row, (label, colour)) in [
        ("YARD", GOLD),
        ("MARKET", MINT),
        ("BOARD", SKY_LIGHT),
        ("BAR", ROSE),
    ]
    .into_iter()
    .enumerate()
    {
        let y = arch.y + 14.0 + row as f32 * 12.0;
        block(draw, arch.x + 10.0, y + 2.0, 3.0, 3.0, colour);
        ui_kit::text_shadowed(
            draw,
            label,
            (arch.x + 16.0) as i32,
            y as i32,
            TextStyle::Body,
            colour,
            None,
        );
    }
    // Spacers walking past (the bar's patrons, out for a stroll).
    let patrons = world.station_patrons(profile.id);
    for (index, patron) in patrons.iter().enumerate() {
        let span = (stage.width - 30.0).max(20.0);
        let speed = 6.0 + index as f32 * 2.5;
        let phase = (seconds * speed + index as f32 * span / 3.0) % (span * 2.0);
        let (x, facing) = if phase < span {
            (stage.x + 15.0 + phase, 1)
        } else {
            (stage.x + 15.0 + span * 2.0 - phase, -1)
        };
        let feet = Vector2::new(x.floor(), floor_y + 6.0 + index as f32 * 4.0);
        if let Some(character) = &patron.character {
            people_art::draw_person_posed(
                draw,
                PersonPen {
                    x: feet.x,
                    y: feet.y,
                    pixel: 1.0,
                },
                character,
                Pose {
                    facing,
                    stride: Some(phase * 4.0),
                    talking: false,
                },
                seconds,
            );
        } else {
            draw_alien(draw, patron.look, feet, facing, seconds);
        }
    }
}

fn draw_shipyard(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    colours: &Palette,
    seconds: f32,
) {
    // The gantry: two pillars and a beam with a hook.
    let width = (stage.width - 30.0).clamp(80.0, 200.0);
    let top = floor_y - 90.0;
    block(draw, left, top, 6.0, 90.0, colours.trim);
    block(draw, left + width - 6.0, top, 6.0, 90.0, colours.trim);
    block(draw, left, top, width, 6.0, colours.trim);
    for x in (0..(width as i32)).step_by(8) {
        block(draw, left + x as f32, top + 1.0, 4.0, 4.0, AMBER);
    }
    let hook_x = left + width * 0.62;
    block(draw, hook_x, top + 6.0, 1.0, 18.0, STEEL_LIGHT);
    // The hull on its cradle: a pointed nose, a cockpit, fins and an open panel.
    let hull_y = floor_y - 34.0;
    let hull_w = width * 0.8;
    let hull_x = left + (width - hull_w) / 2.0;
    block(draw, hull_x + 10.0, hull_y, hull_w - 20.0, 18.0, STEEL);
    block(draw, hull_x + 10.0, hull_y, hull_w - 20.0, 3.0, STEEL_LIGHT);
    block(
        draw,
        hull_x + 10.0,
        hull_y + 15.0,
        hull_w - 20.0,
        3.0,
        STEEL_DARK,
    );
    for step in 0..10 {
        let s = step as f32;
        block(
            draw,
            hull_x + hull_w - 10.0 + s,
            hull_y + s * 0.9,
            1.0,
            18.0 - s * 1.8,
            STEEL,
        );
    }
    block(draw, hull_x + hull_w - 24.0, hull_y + 4.0, 8.0, 5.0, SKY);
    block(draw, hull_x + hull_w - 23.0, hull_y + 4.0, 3.0, 2.0, ICE);
    block(draw, hull_x + 2.0, hull_y - 6.0, 10.0, 6.0, STEEL_DARK);
    block(draw, hull_x + 2.0, hull_y + 18.0, 10.0, 6.0, STEEL_DARK);
    block(draw, hull_x + 30.0, hull_y + 5.0, 18.0, 8.0, INK);
    block(draw, hull_x + 32.0, hull_y + 7.0, 6.0, 1.0, COPPER);
    block(draw, hull_x + 32.0, hull_y + 10.0, 10.0, 1.0, RED);
    // Cradle.
    block(
        draw,
        hull_x + 16.0,
        hull_y + 18.0,
        4.0,
        floor_y - hull_y - 18.0,
        colours.trim,
    );
    block(
        draw,
        hull_x + hull_w - 30.0,
        hull_y + 18.0,
        4.0,
        floor_y - hull_y - 18.0,
        colours.trim,
    );
    // The welder's glow at the open panel (steady, a slow swell).
    let glow = 0.5 + 0.3 * (seconds * 1.2).sin();
    draw.draw_circle_v(
        Vector2::new(hull_x + 48.0, hull_y + 9.0),
        6.0,
        ORANGE.alpha(glow * 0.5),
    );
    draw.draw_circle_v(
        Vector2::new(hull_x + 48.0, hull_y + 9.0),
        2.0,
        CREAM.alpha(glow),
    );
    // Parts crates.
    for (index, colour) in [COPPER, STEEL_DARK, MOSS].into_iter().enumerate() {
        let x = left + width + 6.0 + index as f32 * 2.0;
        let y = floor_y - 12.0 - index as f32 * 11.0;
        if x + 16.0 < stage.x + stage.width {
            block(draw, x, y, 16.0, 11.0, colour);
            block(draw, x, y, 16.0, 1.0, CREAM.alpha(0.4));
            block(draw, x + 7.0, y + 3.0, 2.0, 5.0, INK.alpha(0.5));
        }
    }
}

fn draw_market(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    colours: &Palette,
    seconds: f32,
) {
    // A price board with the goods' arrows.
    let board = Rectangle::new(
        left,
        floor_y - 92.0,
        (stage.width * 0.45).clamp(60.0, 110.0),
        40.0,
    );
    block(
        draw,
        board.x - 2.0,
        board.y - 2.0,
        board.width + 4.0,
        board.height + 4.0,
        colours.trim,
    );
    block(draw, board.x, board.y, board.width, board.height, INK);
    for row in 0..4 {
        let y = board.y + 4.0 + row as f32 * 9.0;
        let up = roll(station_hash(1, profile.id, 9), row).is_multiple_of(2);
        block(draw, board.x + 4.0, y + 2.0, board.width * 0.5, 2.0, FOG);
        let colour = if up { LIME } else { RED };
        let x = board.x + board.width - 10.0;
        if up {
            block(draw, x + 2.0, y, 1.0, 1.0, colour);
            block(draw, x + 1.0, y + 1.0, 3.0, 1.0, colour);
            block(draw, x, y + 2.0, 5.0, 1.0, colour);
        } else {
            block(draw, x, y, 5.0, 1.0, colour);
            block(draw, x + 1.0, y + 1.0, 3.0, 1.0, colour);
            block(draw, x + 2.0, y + 2.0, 1.0, 1.0, colour);
        }
    }
    // A scrolling ticker under it (slow).
    let ticker_x = board.x + (seconds * 8.0) % board.width;
    block(
        draw,
        ticker_x.min(board.x + board.width - 3.0),
        board.y + board.height - 3.0,
        3.0,
        1.0,
        GOLD,
    );
    // Stacked crates in the station's goods colours.
    let goods = [OCEAN, LEAF, STEEL, CREAM, AMBER, VIOLET, ROSE];
    for index in 0..9_u64 {
        let column = index % 3;
        let level = index / 3;
        if level > column {
            continue;
        }
        let colour =
            goods[(roll(station_hash(2, profile.id, 4), index) % goods.len() as u64) as usize];
        let x = left + column as f32 * 18.0;
        let y = floor_y - 14.0 * (level + 1) as f32;
        block(draw, x, y, 16.0, 13.0, colour);
        block(draw, x, y, 16.0, 1.0, CREAM.alpha(0.5));
        block(draw, x + 1.0, y + 1.0, 1.0, 12.0, INK.alpha(0.3));
        block(draw, x + 14.0, y + 1.0, 1.0, 12.0, INK.alpha(0.3));
        block(draw, x + 5.0, y + 5.0, 6.0, 3.0, INK.alpha(0.4));
    }
    // A hand cart.
    let cart_x = left + 64.0;
    if cart_x + 24.0 < stage.x + stage.width {
        block(draw, cart_x, floor_y - 10.0, 22.0, 4.0, colours.trim);
        block(draw, cart_x + 20.0, floor_y - 24.0, 2.0, 14.0, colours.trim);
        draw.draw_circle_v(Vector2::new(cart_x + 4.0, floor_y - 4.0), 3.0, INK);
        draw.draw_circle_v(Vector2::new(cart_x + 17.0, floor_y - 4.0), 3.0, INK);
        block(draw, cart_x + 2.0, floor_y - 20.0, 14.0, 10.0, COPPER);
    }
}

fn draw_board(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    world: &WorldState,
    colours: &Palette,
) {
    // A corkboard with a pinned notice per offer and a wanted poster.
    let width = (stage.width - 26.0).clamp(80.0, 170.0);
    let board = Rectangle::new(left, floor_y - 100.0, width, 66.0);
    block(
        draw,
        board.x - 3.0,
        board.y - 3.0,
        board.width + 6.0,
        board.height + 6.0,
        colours.trim,
    );
    block(draw, board.x, board.y, board.width, board.height, CLAY);
    for index in 0..30_u64 {
        let r = roll(station_hash(3, profile.id, 5), index);
        block(
            draw,
            board.x + (r % board.width as u64) as f32,
            board.y + ((r >> 16) % board.height as u64) as f32,
            1.0,
            1.0,
            LOAM,
        );
    }
    let offers = world.station_board(profile.id);
    for (slot, offer) in offers.iter().enumerate() {
        let x = board.x + 6.0 + slot as f32 * 26.0;
        let y = board.y + 6.0 + (slot % 2) as f32 * 4.0;
        let colour = if offer.is_some() { CREAM } else { SAND };
        block(draw, x, y, 22.0, 28.0, colour);
        block(draw, x + 10.0, y - 1.0, 2.0, 2.0, RED);
        for line in 0..5 {
            block(
                draw,
                x + 3.0,
                y + 6.0 + line as f32 * 4.0,
                16.0 - (line % 3) as f32 * 3.0,
                1.0,
                SLATE,
            );
        }
        if let Some(offer) = offer {
            let stamp = match offer.kind {
                fatherload_simulation::station::MissionKind::Scan { .. } => SKY,
                fatherload_simulation::station::MissionKind::Mine { .. } => COPPER,
                fatherload_simulation::station::MissionKind::Deliver { .. } => LEAF,
            };
            block(draw, x + 13.0, y + 20.0, 6.0, 5.0, stamp);
        }
    }
    // The wanted poster (the hostiles track's factions will fill it).
    let poster_x = board.x + board.width - 32.0;
    if poster_x > board.x + 80.0 {
        block(draw, poster_x, board.y + 8.0, 26.0, 34.0, BONE);
        ui_kit::text_shadowed(
            draw,
            "WANTED",
            (poster_x + 1.0) as i32,
            (board.y + 9.0) as i32,
            TextStyle::Body,
            MAROON,
            None,
        );
        draw.draw_circle_v(Vector2::new(poster_x + 13.0, board.y + 26.0), 6.0, INK);
        block(draw, poster_x + 9.0, board.y + 24.0, 2.0, 2.0, RED);
        block(draw, poster_x + 15.0, board.y + 24.0, 2.0, 2.0, RED);
        block(draw, poster_x + 10.0, board.y + 31.0, 6.0, 1.0, CREAM);
        block(draw, poster_x + 13.0, board.y + 7.0, 2.0, 2.0, RED);
    }
    // A clerk's terminal below.
    block(draw, left + 8.0, floor_y - 26.0, 40.0, 26.0, colours.trim);
    block(draw, left + 12.0, floor_y - 22.0, 32.0, 12.0, INK);
    block(draw, left + 14.0, floor_y - 20.0, 12.0, 1.0, MINT);
    block(draw, left + 14.0, floor_y - 17.0, 20.0, 1.0, MINT);
}

#[allow(clippy::too_many_arguments, reason = "the stage's parts")]
fn draw_bar(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    world: &WorldState,
    colours: &Palette,
    seconds: f32,
) {
    let width = (stage.width - 20.0).max(60.0);
    // The back shelf with bottles.
    let shelf_y = floor_y - 64.0;
    block(draw, left, shelf_y, width, 3.0, colours.trim);
    block(draw, left, shelf_y - 22.0, width, 3.0, colours.trim);
    let bottles = [MINT, AMBER, ROSE, SKY, LIME, VIOLET, GOLD];
    for index in 0..((width / 7.0) as u64) {
        let r = roll(station_hash(4, profile.id, 6), index);
        let colour = bottles[(r % bottles.len() as u64) as usize];
        let x = left + 2.0 + index as f32 * 7.0;
        for (row, y) in [shelf_y, shelf_y - 22.0].into_iter().enumerate() {
            if (r >> (8 + row)).is_multiple_of(3) {
                continue;
            }
            let tall = 8.0 + ((r >> 12) % 6) as f32;
            block(draw, x, y - tall, 4.0, tall, colour.alpha(0.85));
            block(draw, x + 1.0, y - tall - 3.0, 2.0, 3.0, colour);
            block(draw, x, y - tall + 2.0, 1.0, tall - 4.0, CREAM.alpha(0.4));
        }
    }
    // A steady neon sign.
    let sign = if matches!(profile.owner, StationOwner::Alien(_)) {
        "~ OSSA ~"
    } else {
        "BAR"
    };
    let glow = 0.85 + 0.15 * (seconds * 0.7).sin();
    ui_kit::text_shadowed(
        draw,
        sign,
        (left + width * 0.5 - 10.0) as i32,
        (shelf_y - 40.0) as i32,
        TextStyle::Bold,
        ROSE.alpha(glow),
        Some(MAROON),
    );
    // The patrons in front of the counter, the one picked talking.
    let patrons = world.station_patrons(profile.id);
    let counter_y = floor_y - 18.0;
    for (index, patron) in patrons.iter().enumerate() {
        let x = left + 14.0 + index as f32 * (width - 20.0) / patrons.len().max(1) as f32;
        let facing = if index % 2 == 0 { 1 } else { -1 };
        let talking = (seconds * 0.5 + index as f32).sin() > 0.6;
        draw_figure(
            draw,
            patron,
            Vector2::new(x.floor(), counter_y + 2.0),
            facing,
            talking,
            seconds,
        );
    }
    // The counter, over their legs.
    block(draw, left - 4.0, counter_y, width + 8.0, 4.0, colours.trim);
    block(
        draw,
        left - 4.0,
        counter_y + 4.0,
        width + 8.0,
        floor_y - counter_y - 4.0,
        colours.wall,
    );
    block(
        draw,
        left - 4.0,
        counter_y + 4.0,
        width + 8.0,
        1.0,
        INK.alpha(0.5),
    );
    // Glasses on the counter.
    for index in 0..3 {
        let x = left + 22.0 + index as f32 * 31.0;
        if x < left + width - 4.0 {
            block(draw, x, counter_y - 5.0, 3.0, 5.0, ICE.alpha(0.7));
            block(draw, x, counter_y - 3.0, 3.0, 3.0, AMBER.alpha(0.8));
        }
    }
    // A stool.
    block(
        draw,
        left + width - 16.0,
        floor_y - 12.0,
        10.0,
        2.0,
        colours.trim,
    );
    block(
        draw,
        left + width - 12.0,
        floor_y - 10.0,
        2.0,
        10.0,
        colours.trim,
    );
    let _ = StationKind::Relay;
}
