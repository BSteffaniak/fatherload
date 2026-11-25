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
        Good, ShipUpgrade, StationOwner, StationProfile,
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
    let owner = match profile.owner {
        StationOwner::Human if profile.halcyon => "the home system's dock".to_owned(),
        StationOwner::Human => format!("a human {}", profile.kind.name()),
        StationOwner::Alien(kin) => {
            format!("{} run by {}", article(profile.kind.name()), kin.name())
        }
        StationOwner::Derelict => format!("a derelict {}", profile.kind.name()),
    };
    let subtitle = format!("{} - {owner}. {}", page.title(), page_line(page, &profile));
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

/// "an outpost", "a relay".
fn article(word: &str) -> String {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {word}")
    } else {
        format!("a {word}")
    }
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
        StationRow::Arm(_) => ("Weapons", RED),
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
        StationRow::Arm(kind) => {
            let level = kind.level(player.hazard_gear.arms);
            let maxed = level >= kind.max_level();
            (
                entry(
                    &menu_icons::GEAR,
                    kind.name(),
                    format!("tier {level}/{}", kind.max_level()),
                    (!maxed).then(|| kind.price(level)),
                    picked,
                    maxed || credits < kind.price(level),
                ),
                "For the pirates, the swarm and the machine fleet out past the home sector."
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
                // The effect is the next tier's: say so (a fitted tier reads as its own).
                format!(
                    "tier {level}/{}, next: {}",
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
                        fatherload_simulation::station::MissionKind::Bounty { .. } => {
                            "fly to its system and bring it down"
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
                    patron.rumour.as_ref().filter(|_| !heard).map(|_| price),
                    picked,
                    patron.rumour.is_none() || heard || credits < price,
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

/// People and props are drawn at this many art pixels to their own pixel (the room is big).
const FIGURE: f32 = 2.0;

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
    block(
        draw,
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height,
        colours.shade,
    );
    let floor_y = (stage.y + stage.height * 0.8).floor();
    draw_walls(draw, viewport, floor_y, profile, &colours, seconds);
    if stage.width < 70.0 || stage.height < 60.0 {
        return;
    }
    // The window onto space, high on the stage.
    let window = Rectangle::new(
        (stage.x + stage.width * 0.48).floor(),
        (stage.y + stage.height * 0.08).floor(),
        (stage.width * 0.46).min(150.0).floor(),
        (stage.height * 0.3).min(84.0).floor(),
    );
    draw_window(draw, window, profile, &colours, seconds);
    let left = stage.x + 12.0;
    match page {
        StationPage::Hub => {
            draw_concourse(
                draw, stage, left, floor_y, profile, world, &colours, seconds,
            );
        }
        StationPage::Shipyard => {
            draw_shipyard(draw, stage, left, floor_y, profile, &colours, seconds);
        }
        StationPage::Market => draw_market(
            draw, stage, left, floor_y, profile, world, &colours, seconds,
        ),
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
    let wall_top = top + 14.0;
    let wall_height = floor_y - wall_top;
    block(draw, x0, wall_top, w, wall_height, colours.wall);
    // Ceiling.
    block(draw, x0, top, w, 14.0, colours.shade);
    block(draw, x0, top + 12.0, w, 2.0, colours.trim);
    match profile.owner {
        StationOwner::Human => {
            // Riveted panels with seams, a warm strip light per bay, a dado rail.
            let bay = 48.0;
            let mut x = x0;
            while x < x0 + w {
                block(draw, x, wall_top, 3.0, wall_height, colours.shade);
                block(
                    draw,
                    x + 3.0,
                    wall_top,
                    1.0,
                    wall_height,
                    STEEL_LIGHT.alpha(0.5),
                );
                for y in [wall_top + 8.0, floor_y - 10.0] {
                    block(draw, x + 8.0, y, 1.0, 1.0, STEEL_LIGHT);
                    block(draw, x + bay - 6.0, y, 1.0, 1.0, STEEL_LIGHT);
                }
                block(draw, x + 10.0, top + 4.0, bay - 20.0, 4.0, colours.light);
                block(draw, x + 10.0, top + 8.0, bay - 20.0, 1.0, GOLD);
                // The strip light's wash down the wall.
                for step in 0..6 {
                    block(
                        draw,
                        x + 8.0,
                        wall_top + step as f32 * 3.0,
                        bay - 16.0,
                        3.0,
                        colours.light.alpha(0.05 * (6 - step) as f32),
                    );
                }
                x += bay;
            }
            block(draw, x0, floor_y - 40.0, w, 3.0, colours.shade);
            block(draw, x0, floor_y - 40.0, w, 1.0, STEEL.alpha(0.8));
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
            // Grown columns rising from the floor into pointed arches under the ceiling, a
            // breathing pod at each crown and down the columns.
            let span = 64.0;
            let crown = wall_top + 18.0;
            let spring = crown + span * 0.6;
            let mut x = x0 - span * 0.5;
            let mut index = 0_u64;
            while x < x0 + w + span {
                // The column (a little bulge where it springs).
                block(draw, x - 3.0, spring, 6.0, floor_y - spring, colours.trim);
                block(draw, x - 2.0, spring, 1.0, floor_y - spring, LAVENDER);
                block(draw, x - 5.0, floor_y - 8.0, 10.0, 8.0, colours.trim);
                // The arch to the next column: two arcs meeting at a point.
                for (centre_x, from, to) in [(x + span, 180.0_f32, 240.0_f32), (x, 300.0, 360.0)] {
                    let centre = Vector2::new(centre_x, spring);
                    draw.draw_ring(centre, span - 3.0, span + 3.0, from, to, 16, colours.trim);
                    draw.draw_ring(centre, span - 2.0, span - 1.0, from, to, 16, LAVENDER);
                }
                let glow = 0.7 + 0.3 * (seconds * 0.8 + index as f32).sin();
                let pod = Vector2::new(x + span * 0.5, crown + 6.0);
                draw.draw_circle_v(pod, 5.0, PLUM);
                draw.draw_circle_v(pod, 3.0, colours.light.alpha(glow));
                draw.draw_circle_v(Vector2::new(pod.x - 1.0, pod.y - 1.0), 1.0, CREAM);
                for step in 0..3 {
                    let spot = Vector2::new(x, spring + 30.0 + step as f32 * 40.0);
                    draw.draw_circle_v(spot, 3.0, PLUM);
                    draw.draw_circle_v(spot, 1.5, colours.light.alpha(glow * 0.8));
                }
                x += span;
                index += 1;
            }
            block(draw, x0, floor_y - 2.0, w, 2.0, MINT.alpha(0.25));
        }
        StationOwner::Derelict => {
            // Scorched plating, torn panels, hanging cables, one lamp dimming slowly.
            let bay = 52.0;
            let mut x = x0;
            let mut index = 0_u64;
            while x < x0 + w {
                block(draw, x, wall_top, 3.0, wall_height, INK);
                let scorch = (roll(profile.id.index.into(), index) % 40) as f32;
                block(
                    draw,
                    x + 10.0,
                    wall_top + 30.0 + scorch,
                    20.0,
                    12.0,
                    SOIL_DARK,
                );
                block(draw, x + 14.0, wall_top + 33.0 + scorch, 10.0, 5.0, INK);
                if index % 3 == 1 {
                    // A torn panel showing the frame behind.
                    block(draw, x + 6.0, wall_top + 90.0, 26.0, 40.0, INK);
                    for rib in 0..3 {
                        block(
                            draw,
                            x + 9.0 + rib as f32 * 8.0,
                            wall_top + 90.0,
                            2.0,
                            40.0,
                            RUST,
                        );
                    }
                }
                let sway = (seconds * 0.5 + index as f32).sin() * 2.0;
                for step in 0..22 {
                    block(
                        draw,
                        (x + 34.0 + sway * step as f32 / 22.0).floor(),
                        wall_top + step as f32,
                        1.0,
                        1.0,
                        STONE,
                    );
                }
                x += bay;
                index += 1;
            }
            let dim = 0.3 + 0.25 * (seconds * 0.4).sin().abs();
            block(
                draw,
                x0 + w * 0.7,
                top + 4.0,
                24.0,
                4.0,
                colours.light.alpha(dim),
            );
            for step in 0..8 {
                block(
                    draw,
                    x0 + w * 0.7 - step as f32 * 3.0,
                    wall_top + step as f32 * 4.0,
                    24.0 + step as f32 * 6.0,
                    4.0,
                    colours.light.alpha(dim * 0.06),
                );
            }
        }
    }
    // The floor: plates with seams, a darker band under the wall.
    let bottom = viewport.y + viewport.height;
    block(draw, x0, floor_y, w, bottom - floor_y, colours.floor);
    block(draw, x0, floor_y, w, 3.0, colours.shade);
    let mut y = floor_y + 6.0;
    let mut row = 0;
    while y < bottom {
        block(draw, x0, y, w, 1.0, colours.shade);
        let mut x = x0 + if row % 2 == 0 { 0.0 } else { 20.0 };
        while x < x0 + w {
            block(draw, x, y - 5.0, 1.0, 5.0, colours.shade);
            x += 40.0;
        }
        y += 7.0;
        row += 1;
    }
    if profile.owner == StationOwner::Derelict {
        // Debris on the deck: a tipped crate, loose panels, scattered bolts.
        for index in 0..10_u64 {
            let r = roll(u64::from(profile.id.index) ^ 0xDE, index);
            let x = x0 + (r % (w as u64).max(1)) as f32;
            let yy = floor_y + 4.0 + ((r >> 16) % 30) as f32;
            match index % 3 {
                0 => {
                    block(draw, x, yy - 10.0, 16.0, 10.0, STONE_DARK);
                    block(draw, x + 2.0, yy - 9.0, 12.0, 1.0, RUST);
                }
                1 => block(draw, x, yy, 20.0, 2.0, STONE),
                _ => {
                    block(draw, x, yy, 1.0, 1.0, STEEL_LIGHT);
                    block(draw, x + 4.0, yy + 2.0, 1.0, 1.0, STEEL);
                }
            }
        }
    }
}

/// A disc clipped to `clip` (drawn row by row, so it never spills out of a window).
fn clipped_disc(
    draw: &mut impl RaylibDraw,
    clip: Rectangle,
    centre: Vector2,
    radius: f32,
    colour: Color,
) {
    let top = (centre.y - radius).max(clip.y).floor() as i32;
    let bottom = (centre.y + radius).min(clip.y + clip.height).ceil() as i32;
    for y in top..bottom {
        let dy = y as f32 + 0.5 - centre.y;
        let half = (radius * radius - dy * dy).max(0.0).sqrt();
        let x0 = (centre.x - half).max(clip.x);
        let x1 = (centre.x + half).min(clip.x + clip.width);
        if x1 > x0 {
            block(draw, x0, y as f32, x1 - x0, 1.0, colour);
        }
    }
}

/// The window onto space and the world the station keeps station with (it turns slowly).
fn draw_window(
    draw: &mut impl RaylibDraw,
    window: Rectangle,
    profile: &StationProfile,
    colours: &Palette,
    seconds: f32,
) {
    block(
        draw,
        window.x - 4.0,
        window.y - 4.0,
        window.width + 8.0,
        window.height + 8.0,
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
    for index in 0..50 {
        let r = roll(seed, index);
        let x = window.x + (r % 1_000) as f32 / 1_000.0 * window.width;
        let y = window.y + ((r >> 12) % 1_000) as f32 / 1_000.0 * window.height;
        let colour = match (r >> 30) % 4 {
            0 => FOG,
            1 => MIST,
            2 => SKY_LIGHT,
            _ => CREAM,
        };
        block(draw, x, y, 1.0, 1.0, colour);
    }
    let pick = (seed % 6) as usize;
    let colour = [OCEAN, CLAY, LEAF, SAND, ICE, ROSE][pick];
    let shade = [DEEP_BLUE, RUST, PINE, LOAM, SKY, PLUM][pick];
    let band = [SKY, AMBER, MOSS, CLAY, MIST, VIOLET][pick];
    let centre = Vector2::new(
        window.x + window.width * (0.35 + ((seed >> 8) % 30) as f32 / 100.0),
        window.y + window.height * 1.25,
    );
    let radius = window.height * 0.95;
    clipped_disc(draw, window, centre, radius + 4.0, colour.alpha(0.18));
    clipped_disc(draw, window, centre, radius + 2.0, colour.alpha(0.3));
    clipped_disc(draw, window, centre, radius, shade);
    clipped_disc(
        draw,
        window,
        Vector2::new(centre.x - radius * 0.12, centre.y - radius * 0.06),
        radius * 0.94,
        colour,
    );
    // Bands drifting slowly across its face.
    for stripe in 0..5 {
        let y = centre.y - radius * 0.92 + stripe as f32 * radius * 0.14;
        let drift = (seconds * 0.04 + stripe as f32 * 1.3).sin() * 8.0;
        let half = (radius * radius - (y - centre.y).powi(2)).max(0.0).sqrt() * 0.85;
        let x0 = (centre.x - half + drift).max(window.x);
        let x1 = (centre.x + half * 0.7 + drift).min(window.x + window.width);
        if y > window.y && y < window.y + window.height && x1 > x0 {
            block(draw, x0, y, x1 - x0, 2.0, band.alpha(0.55));
        }
    }
    // Mullions and a glint.
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
    for step in 0..6 {
        block(
            draw,
            window.x + 6.0 + step as f32,
            window.y + 14.0 - step as f32 * 2.0,
            1.0,
            2.0,
            CREAM.alpha(0.25),
        );
    }
}

/// A patron: a people-engine character for a human, alien kin drawn as themselves.
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
                pixel: FIGURE,
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

/// One of a station's alien kin: tall and narrow in a long robe, a domed head with big dark
/// eyes, frills or a crest, colours from its seed; it breathes gently.
fn draw_alien(draw: &mut impl RaylibDraw, look: u64, feet: Vector2, facing: i32, seconds: f32) {
    let skins = [MINT, LAVENDER, SKY_LIGHT, ROSE, LIME];
    let robes = [VIOLET, TEAL_DARK, PLUM, DEEP_BLUE, MAROON];
    let skin = skins[(look % 5) as usize];
    let robe = robes[((look >> 5) % 5) as usize];
    let trim = [GOLD, MINT, CREAM, AMBER, SKY][((look >> 17) % 5) as usize];
    let p = FIGURE;
    let height = 24.0 + ((look >> 9) % 4) as f32;
    let breathe = ((seconds * 1.3 + (look % 7) as f32).sin() * 0.5).round();
    let (x, y) = (feet.x, feet.y);
    let px = |draw: &mut dyn FnMut(f32, f32, f32, f32, Color),
              gx: f32,
              gy: f32,
              gw: f32,
              gh: f32,
              c: Color| {
        draw(x + gx * p, y + gy * p, gw * p, gh * p, c);
    };
    let mut put = |rx: f32, ry: f32, rw: f32, rh: f32, colour: Color| {
        block(draw, rx, ry, rw, rh, colour);
    };
    // Shadow.
    px(&mut put, -6.0, -1.0, 12.0, 1.0, INK.alpha(0.35));
    // The robe, wide at the hem, a trim line down the front.
    let body = height - 9.0;
    for row in 0..(body as i32) {
        let t = row as f32 / body;
        let half = (2.5 + t * 3.0).round();
        let gy = -body + row as f32;
        px(&mut put, -half, gy - 1.0, half * 2.0, 1.0, robe);
        px(&mut put, -half, gy - 1.0, 1.0, 1.0, INK.alpha(0.4));
    }
    px(
        &mut put,
        -0.5 + facing.signum() as f32,
        -body,
        1.0,
        body - 1.0,
        trim.alpha(0.8),
    );
    // Long hands folded at the waist.
    px(&mut put, -2.0, -body * 0.55, 4.0, 1.0, skin);
    // Neck and head (the head bobs with its breath).
    let head = -height + 3.0 + breathe;
    px(&mut put, -0.5, head + 3.0, 1.0, 3.0, skin);
    px(&mut put, -3.0, head - 3.0, 6.0, 6.0, skin);
    px(&mut put, -2.0, head - 4.0, 4.0, 1.0, skin);
    px(&mut put, -2.0, head + 3.0, 4.0, 1.0, skin);
    px(&mut put, -3.0, head - 3.0, 1.0, 6.0, INK.alpha(0.25));
    // Eyes, big and dark with a glint, looking the way it faces.
    let eye = facing.signum() as f32 * 0.5;
    px(&mut put, -2.5 + eye, head - 1.0, 2.0, 2.0, INK);
    px(&mut put, 0.5 + eye, head - 1.0, 2.0, 2.0, INK);
    px(&mut put, -2.5 + eye, head - 1.0, 1.0, 1.0, CREAM);
    px(&mut put, 0.5 + eye, head - 1.0, 1.0, 1.0, CREAM);
    // A crest or frills.
    if (look >> 13).is_multiple_of(2) {
        for index in 0..3 {
            px(
                &mut put,
                -2.0 + index as f32 * 2.0,
                head - 6.0 - (index % 2) as f32,
                1.0,
                2.0,
                trim,
            );
        }
    } else {
        px(&mut put, -5.0, head - 2.0, 2.0, 4.0, robe);
        px(&mut put, 3.0, head - 2.0, 2.0, 4.0, robe);
        px(&mut put, -5.0, head - 2.0, 1.0, 1.0, trim);
        px(&mut put, 4.0, head - 2.0, 1.0, 1.0, trim);
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
    // A directory arch with the counters' signs (only what this station offers lights up).
    let arch = Rectangle::new(left, floor_y - 96.0, 84.0, 96.0);
    block(draw, arch.x, arch.y, arch.width, 10.0, colours.trim);
    block(draw, arch.x, arch.y, 8.0, arch.height, colours.trim);
    block(
        draw,
        arch.x + arch.width - 8.0,
        arch.y,
        8.0,
        arch.height,
        colours.trim,
    );
    block(
        draw,
        arch.x + 8.0,
        arch.y + 10.0,
        arch.width - 16.0,
        arch.height - 10.0,
        INK,
    );
    block(
        draw,
        arch.x + 8.0,
        arch.y + 10.0,
        arch.width - 16.0,
        1.0,
        colours.light.alpha(0.4),
    );
    let signs = [
        ("YARD", GOLD, profile.yard_tier > 0 || profile.gear_tier > 0),
        ("MARKET", MINT, profile.market),
        ("BOARD", SKY_LIGHT, profile.bounties),
        ("BAR", ROSE, profile.bar),
    ];
    for (row, (label, colour, open)) in signs.into_iter().enumerate() {
        let y = arch.y + 18.0 + row as f32 * 14.0;
        let colour = if open { colour } else { SLATE };
        block(draw, arch.x + 14.0, y + 2.0, 4.0, 4.0, colour);
        ui_kit::text_shadowed(
            draw,
            label,
            (arch.x + 22.0) as i32,
            y as i32,
            TextStyle::Body,
            colour,
            None,
        );
    }
    // Spacers walking past (the bar's patrons, out for a stroll).
    let patrons = world.station_patrons(profile.id);
    let span = (stage.width - 40.0).max(20.0);
    for (index, patron) in patrons.iter().enumerate() {
        let speed = 7.0 + index as f32 * 2.5;
        let phase = (index as f32).mul_add(span / 2.0, seconds * speed) % (span * 2.0);
        let (x, facing) = if phase < span {
            (stage.x + 20.0 + phase, 1)
        } else {
            (stage.x + 20.0 + span * 2.0 - phase, -1)
        };
        let feet = Vector2::new(x.floor(), floor_y + 10.0 + index as f32 * 8.0);
        if let Some(character) = &patron.character {
            people_art::draw_person_posed(
                draw,
                PersonPen {
                    x: feet.x,
                    y: feet.y,
                    pixel: FIGURE,
                },
                character,
                Pose {
                    facing,
                    stride: Some(phase * 2.0),
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
    profile: &StationProfile,
    colours: &Palette,
    seconds: f32,
) {
    // The gantry: two lattice pillars and a beam with a hook and a crane trolley.
    let width = (stage.width - 30.0).clamp(100.0, 240.0);
    let top = floor_y - 150.0;
    for pillar in [left, left + width - 10.0] {
        block(draw, pillar, top, 10.0, 150.0, colours.trim);
        let mut y = top + 4.0;
        while y < floor_y - 6.0 {
            block(draw, pillar + 2.0, y, 6.0, 1.0, colours.shade);
            block(draw, pillar + 2.0, y, 1.0, 8.0, colours.shade);
            y += 10.0;
        }
    }
    block(draw, left, top, width, 10.0, colours.trim);
    let mut x = left;
    while x < left + width {
        block(draw, x + 2.0, top + 2.0, 5.0, 6.0, AMBER);
        x += 10.0;
    }
    let trolley = left + width * (0.55 + 0.05 * (seconds * 0.2).sin());
    block(draw, trolley - 8.0, top + 10.0, 16.0, 6.0, STEEL_DARK);
    block(draw, trolley - 0.5, top + 16.0, 1.0, 34.0, STEEL_LIGHT);
    block(draw, trolley - 4.0, top + 50.0, 8.0, 3.0, STEEL);
    // The hull on its cradle: engines aft (left), a long body, a cockpit and a pointed nose.
    let hull_w = (width - 26.0).max(60.0);
    let hull_h = 34.0;
    let hull_x = left + 13.0;
    let hull_y = floor_y - hull_h - 26.0;
    let alien = matches!(profile.owner, StationOwner::Alien(_));
    let (skin, light, dark) = if alien {
        (VIOLET, LAVENDER, PLUM)
    } else {
        (STEEL, STEEL_LIGHT, STEEL_DARK)
    };
    // Fins top and bottom.
    for step in 0..12 {
        let s = step as f32;
        block(
            draw,
            hull_x + 6.0 + s,
            hull_y - 12.0 + s,
            22.0 - s,
            1.0,
            dark,
        );
        block(
            draw,
            hull_x + 6.0 + s,
            hull_y + hull_h + 11.0 - s,
            22.0 - s,
            1.0,
            dark,
        );
    }
    // Body.
    block(draw, hull_x, hull_y, hull_w - 24.0, hull_h, skin);
    block(draw, hull_x, hull_y, hull_w - 24.0, 4.0, light);
    block(
        draw,
        hull_x,
        hull_y + hull_h - 5.0,
        hull_w - 24.0,
        5.0,
        dark,
    );
    // Nose (a wedge of rows).
    for row in 0..(hull_h as i32) {
        let t = (row as f32 - hull_h * 0.5).abs() / (hull_h * 0.5);
        let length = (1.0 - t) * 24.0;
        let colour = if row < 4 {
            light
        } else if row as f32 > hull_h - 6.0 {
            dark
        } else {
            skin
        };
        block(
            draw,
            hull_x + hull_w - 24.0,
            hull_y + row as f32,
            length,
            1.0,
            colour,
        );
    }
    // Panel lines, cockpit, the open panel the welders work in.
    let mut x = hull_x + 18.0;
    while x < hull_x + hull_w - 30.0 {
        block(draw, x, hull_y + 4.0, 1.0, hull_h - 9.0, dark.alpha(0.7));
        x += 22.0;
    }
    block(draw, hull_x + hull_w - 40.0, hull_y + 8.0, 16.0, 9.0, INK);
    block(draw, hull_x + hull_w - 39.0, hull_y + 9.0, 14.0, 7.0, SKY);
    block(draw, hull_x + hull_w - 38.0, hull_y + 9.0, 5.0, 2.0, ICE);
    let panel = Rectangle::new(hull_x + hull_w * 0.38, hull_y + 10.0, 26.0, 14.0);
    block(draw, panel.x, panel.y, panel.width, panel.height, INK);
    block(draw, panel.x + 3.0, panel.y + 3.0, 10.0, 1.0, COPPER);
    block(draw, panel.x + 3.0, panel.y + 7.0, 16.0, 1.0, RED);
    block(draw, panel.x + 3.0, panel.y + 10.0, 8.0, 1.0, MINT);
    // Engines aft with a cold blue idle glow.
    for (index, y) in [hull_y + 5.0, hull_y + hull_h - 15.0]
        .into_iter()
        .enumerate()
    {
        block(draw, hull_x - 8.0, y, 8.0, 10.0, dark);
        block(draw, hull_x - 10.0, y + 1.0, 2.0, 8.0, INK);
        let idle = 0.4 + 0.2 * (seconds * 0.9 + index as f32).sin();
        block(
            draw,
            hull_x - 14.0,
            y + 2.0,
            4.0,
            6.0,
            SKY_LIGHT.alpha(idle),
        );
    }
    // Cradle legs.
    for leg in [hull_x + 16.0, hull_x + hull_w - 50.0] {
        block(
            draw,
            leg,
            hull_y + hull_h,
            6.0,
            floor_y - hull_y - hull_h,
            colours.trim,
        );
        block(draw, leg - 4.0, floor_y - 4.0, 14.0, 4.0, colours.shade);
    }
    // The welder's glow at the open panel (steady, a slow swell; never a flash).
    let glow = 0.55 + 0.25 * (seconds * 1.1).sin();
    let spot = Vector2::new(panel.x + panel.width * 0.5, panel.y + panel.height);
    draw.draw_circle_v(spot, 10.0, ORANGE.alpha(glow * 0.35));
    draw.draw_circle_v(spot, 4.0, AMBER.alpha(glow));
    draw.draw_circle_v(spot, 1.5, CREAM);
    // Parts crates stacked by the pillar.
    for (index, colour) in [COPPER, STEEL_DARK, MOSS].into_iter().enumerate() {
        let x = left + width + 6.0;
        let y = floor_y - 16.0 - index as f32 * 15.0;
        if x + 22.0 < stage.x + stage.width {
            block(draw, x + index as f32 * 2.0, y, 22.0, 15.0, colour);
            block(
                draw,
                x + index as f32 * 2.0,
                y,
                22.0,
                2.0,
                CREAM.alpha(0.35),
            );
            block(
                draw,
                x + index as f32 * 2.0 + 9.0,
                y + 4.0,
                4.0,
                7.0,
                INK.alpha(0.45),
            );
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "the stage's parts")]
fn draw_market(
    draw: &mut impl RaylibDraw,
    stage: Rectangle,
    left: f32,
    floor_y: f32,
    profile: &StationProfile,
    world: &WorldState,
    colours: &Palette,
    seconds: f32,
) {
    // A price board with the goods' arrows and a slow ticker.
    let board = Rectangle::new(
        left,
        floor_y - 150.0,
        (stage.width * 0.42).clamp(70.0, 120.0),
        52.0,
    );
    block(
        draw,
        board.x - 3.0,
        board.y - 3.0,
        board.width + 6.0,
        board.height + 6.0,
        colours.trim,
    );
    block(draw, board.x, board.y, board.width, board.height, INK);
    for row in 0..5 {
        let y = board.y + 4.0 + row as f32 * 9.0;
        let up = roll(station_hash(1, profile.id, 9), row).is_multiple_of(2);
        block(draw, board.x + 4.0, y + 2.0, board.width * 0.45, 2.0, FOG);
        block(draw, board.x + board.width * 0.55, y + 2.0, 12.0, 2.0, GOLD);
        let colour = if up { LIME } else { RED };
        let x = board.x + board.width - 10.0;
        for step in 0..3 {
            let s = step as f32;
            let (yy, ww) = if up {
                (y + s, 1.0 + s * 2.0)
            } else {
                (y + s, 5.0 - s * 2.0)
            };
            block(draw, x + (5.0 - ww) * 0.5, yy, ww, 1.0, colour);
        }
    }
    let ticker = board.x + (seconds * 6.0) % (board.width - 4.0);
    block(draw, ticker, board.y + board.height - 3.0, 4.0, 1.0, GOLD);
    // A stall with an awning, the trader standing at it (a patron at work).
    let patrons = world.station_patrons(profile.id);
    let stall_x = left + board.width + 14.0;
    if stall_x + 60.0 < stage.x + stage.width {
        block(draw, stall_x, floor_y - 30.0, 60.0, 30.0, colours.trim);
        block(draw, stall_x, floor_y - 30.0, 60.0, 4.0, COPPER);
        block(
            draw,
            stall_x + 4.0,
            floor_y - 22.0,
            52.0,
            1.0,
            colours.shade,
        );
        for stripe in 0..6 {
            block(
                draw,
                stall_x - 4.0 + stripe as f32 * 11.0,
                floor_y - 92.0,
                11.0,
                8.0,
                if stripe % 2 == 0 { RED } else { CREAM },
            );
        }
        block(draw, stall_x - 2.0, floor_y - 84.0, 2.0, 54.0, colours.trim);
        block(
            draw,
            stall_x + 60.0,
            floor_y - 84.0,
            2.0,
            54.0,
            colours.trim,
        );
        for (index, colour) in [MINT, AMBER, ROSE, SKY].into_iter().enumerate() {
            let x = stall_x + 6.0 + index as f32 * 13.0;
            block(draw, x, floor_y - 38.0, 10.0, 8.0, colour);
            block(draw, x, floor_y - 38.0, 10.0, 1.0, CREAM.alpha(0.5));
        }
        if let Some(trader) = patrons.first() {
            draw_figure(
                draw,
                trader,
                Vector2::new((stall_x + 30.0).floor(), floor_y + 10.0),
                -1,
                (seconds * 0.4).sin() > 0.4,
                seconds,
            );
        }
    }
    // Stacked crates in the station's goods colours.
    let goods = [OCEAN, LEAF, STEEL, CREAM, AMBER, VIOLET, ROSE];
    for index in 0..6_u64 {
        let (column, level) = match index {
            0 => (0, 0),
            1 => (1, 0),
            2 => (2, 0),
            3 => (0, 1),
            4 => (1, 1),
            _ => (0, 2),
        };
        let colour =
            goods[(roll(station_hash(2, profile.id, 4), index) % goods.len() as u64) as usize];
        let x = left + column as f32 * 26.0 + level as f32 * 6.0;
        let y = floor_y - 22.0 * (level + 1) as f32;
        block(draw, x, y, 24.0, 21.0, colour);
        block(draw, x, y, 24.0, 2.0, CREAM.alpha(0.5));
        block(draw, x + 1.0, y + 2.0, 1.0, 19.0, INK.alpha(0.3));
        block(draw, x + 22.0, y + 2.0, 1.0, 19.0, INK.alpha(0.3));
        block(draw, x + 6.0, y + 8.0, 12.0, 5.0, INK.alpha(0.35));
        block(draw, x + 8.0, y + 9.0, 8.0, 1.0, CREAM.alpha(0.5));
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
    let width = (stage.width - 30.0).clamp(100.0, 200.0);
    let board = Rectangle::new(left, floor_y - 150.0, width, 92.0);
    block(
        draw,
        board.x - 4.0,
        board.y - 4.0,
        board.width + 8.0,
        board.height + 8.0,
        colours.trim,
    );
    block(draw, board.x, board.y, board.width, board.height, CLAY);
    for index in 0..60_u64 {
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
        let x = board.x + 8.0 + slot as f32 * 34.0;
        let y = board.y + 8.0 + (slot % 2) as f32 * 6.0;
        let paper = if offer.is_some() { CREAM } else { SAND };
        block(draw, x + 1.0, y + 1.0, 28.0, 38.0, INK.alpha(0.3));
        block(draw, x, y, 28.0, 38.0, paper);
        block(draw, x + 13.0, y - 1.0, 3.0, 3.0, RED);
        for line in 0..6 {
            block(
                draw,
                x + 4.0,
                y + 8.0 + line as f32 * 4.0,
                20.0 - (line % 3) as f32 * 4.0,
                1.0,
                SLATE,
            );
        }
        if let Some(offer) = offer {
            let stamp = match offer.kind {
                fatherload_simulation::station::MissionKind::Scan { .. } => SKY,
                fatherload_simulation::station::MissionKind::Mine { .. } => COPPER,
                fatherload_simulation::station::MissionKind::Deliver { .. } => LEAF,
                fatherload_simulation::station::MissionKind::Bounty { .. } => RED,
            };
            block(draw, x + 17.0, y + 29.0, 8.0, 6.0, stamp);
            block(draw, x + 18.0, y + 30.0, 6.0, 1.0, CREAM.alpha(0.6));
        }
    }
    // The wanted poster: the board's bounty, if it has one (a hostile faction's named ship,
    // Wave 12 integration), its faction's colour on the face and its name under it.
    let wanted = offers.iter().flatten().find_map(|offer| {
        fatherload_simulation::station::bounty::bounty_target_of(world.world_seed(), offer.kind)
    });
    let poster_x = board.x + board.width - 40.0;
    if poster_x > board.x + 104.0 {
        block(draw, poster_x, board.y + 10.0, 34.0, 46.0, BONE);
        ui_kit::text_shadowed(
            draw,
            "WANTED",
            (poster_x + 3.0) as i32,
            (board.y + 12.0) as i32,
            TextStyle::Body,
            MAROON,
            None,
        );
        draw.draw_circle_v(Vector2::new(poster_x + 17.0, board.y + 35.0), 8.0, INK);
        block(draw, poster_x + 12.0, board.y + 33.0, 3.0, 2.0, RED);
        block(draw, poster_x + 19.0, board.y + 33.0, 3.0, 2.0, RED);
        block(draw, poster_x + 13.0, board.y + 39.0, 8.0, 1.0, CREAM);
        block(draw, poster_x + 6.0, board.y + 48.0, 22.0, 2.0, MAROON);
        block(draw, poster_x + 16.0, board.y + 8.0, 3.0, 3.0, RED);
        if let Some(target) = wanted {
            let mark = match target.faction {
                fatherload_simulation::hostiles::Faction::Pirates => COPPER,
                fatherload_simulation::hostiles::Faction::Swarm => LEAF,
                fatherload_simulation::hostiles::Faction::Machine => SKY,
            };
            block(draw, poster_x + 3.0, board.y + 52.0, 28.0, 3.0, mark);
            let name: String = target.name.chars().take(12).collect();
            ui_kit::text_shadowed(
                draw,
                &name,
                (poster_x - 6.0) as i32,
                (board.y + 58.0) as i32,
                TextStyle::Body,
                CREAM,
                None,
            );
        }
    }
    // A clerk's terminal below, its screen scrolling the board.
    block(draw, left + 8.0, floor_y - 40.0, 54.0, 40.0, colours.trim);
    block(draw, left + 12.0, floor_y - 36.0, 46.0, 18.0, INK);
    for line in 0..3 {
        block(
            draw,
            left + 15.0,
            floor_y - 33.0 + line as f32 * 5.0,
            18.0 + line as f32 * 6.0,
            1.0,
            MINT,
        );
    }
    block(draw, left + 14.0, floor_y - 12.0, 42.0, 3.0, colours.shade);
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
    let width = (stage.width - 24.0).max(60.0);
    let alien = matches!(profile.owner, StationOwner::Alien(_));
    // The back shelves with bottles.
    let shelf_y = floor_y - 82.0;
    for y in [shelf_y, shelf_y - 26.0] {
        block(draw, left, y, width, 3.0, colours.trim);
        block(draw, left, y + 3.0, width, 1.0, INK.alpha(0.4));
    }
    let bottles = [MINT, AMBER, ROSE, SKY, LIME, VIOLET, GOLD];
    for index in 0..((width / 8.0) as u64) {
        let r = roll(station_hash(4, profile.id, 6), index);
        let colour = bottles[(r % bottles.len() as u64) as usize];
        let x = left + 3.0 + index as f32 * 8.0;
        for (row, y) in [shelf_y, shelf_y - 26.0].into_iter().enumerate() {
            if (r >> (8 + row)).is_multiple_of(3) {
                continue;
            }
            let tall = 10.0 + ((r >> 12) % 7) as f32;
            block(draw, x, y - tall, 5.0, tall, colour.alpha(0.85));
            block(draw, x + 1.0, y - tall - 4.0, 3.0, 4.0, colour);
            block(
                draw,
                x + 1.0,
                y - tall + 2.0,
                1.0,
                tall - 4.0,
                CREAM.alpha(0.45),
            );
        }
    }
    // A steady neon sign over the shelves.
    let sign = if alien { "OSSA" } else { "BAR" };
    let glow = 0.85 + 0.15 * (seconds * 0.6).sin();
    let sign_x = (left + width * 0.5 - 16.0) as i32;
    let sign_y = (shelf_y - 62.0) as i32;
    block(
        draw,
        sign_x as f32 - 6.0,
        sign_y as f32 - 4.0,
        ui_kit::text_width(sign, TextStyle::Title) as f32 + 12.0,
        24.0,
        INK.alpha(0.5),
    );
    ui_kit::text_shadowed(
        draw,
        sign,
        sign_x,
        sign_y,
        TextStyle::Title,
        if alien { MINT } else { ROSE }.alpha(glow),
        Some(if alien { TEAL_DARK } else { MAROON }),
    );
    let patrons = world.station_patrons(profile.id);
    let counter_y = floor_y - 26.0;
    // The keeper behind the counter (the bar's last patron works the taps).
    if let Some(keeper) = patrons.last() {
        draw_figure(
            draw,
            keeper,
            Vector2::new((left + width * 0.75).floor(), counter_y + 4.0),
            -1,
            false,
            seconds,
        );
    }
    // The counter, its top polished, over the keeper's legs.
    let (wood, top_colour) = if alien {
        (PLUM, LAVENDER)
    } else {
        (RUST, COPPER)
    };
    block(draw, left - 4.0, counter_y, width + 8.0, 5.0, top_colour);
    block(
        draw,
        left - 4.0,
        counter_y + 5.0,
        width + 8.0,
        floor_y - counter_y - 5.0,
        wood,
    );
    block(
        draw,
        left - 4.0,
        counter_y + 5.0,
        width + 8.0,
        2.0,
        INK.alpha(0.4),
    );
    let mut x = left;
    while x < left + width {
        block(
            draw,
            x,
            counter_y + 9.0,
            1.0,
            floor_y - counter_y - 11.0,
            INK.alpha(0.25),
        );
        x += 16.0;
    }
    // Glasses on the counter.
    for index in 0..3 {
        let x = left + 24.0 + index as f32 * 36.0;
        if x < left + width - 6.0 {
            block(draw, x, counter_y - 7.0, 4.0, 7.0, ICE.alpha(0.7));
            block(draw, x, counter_y - 4.0, 4.0, 4.0, AMBER.alpha(0.8));
        }
    }
    // The other patrons in front of the counter on stools, chatting.
    let front = patrons.len().saturating_sub(1).max(1);
    for (index, patron) in patrons.iter().enumerate().take(front) {
        let x = left + 22.0 + index as f32 * (width * 0.5) / front as f32;
        let feet = Vector2::new(x.floor(), floor_y + 12.0);
        block(draw, feet.x - 7.0, floor_y - 10.0, 14.0, 3.0, colours.trim);
        block(draw, feet.x - 1.0, floor_y - 7.0, 2.0, 19.0, colours.trim);
        let talking = (seconds * 0.5 + index as f32 * 2.0).sin() > 0.5;
        draw_figure(
            draw,
            patron,
            feet,
            if index % 2 == 0 { 1 } else { -1 },
            talking,
            seconds,
        );
    }
}
