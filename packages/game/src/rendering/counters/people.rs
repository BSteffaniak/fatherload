//! Talking to people (WP-PEOPLE): the conversation panel with the person's portrait, how they
//! see you and what you can say; their trade list; and the gift list.
//!
//! What was said comes from the world's people ledger (the same lines on every peer); the
//! rows are the shell's ([`shell::talk_choices`], [`offers::offers_of`], [`shell::gift_items`]),
//! so the labels always describe what confirm does.

use fatherload_simulation::{
    item::ItemKind,
    people::{
        Character, MoodBand, Standing,
        ledger::Speaker,
        offers::{self, Perk, PersonDeal},
        routine,
    },
    village::VillageDeal,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::{
    super::{
        layout::{self, ModalContent, Section, SectionItem, UiLayout},
        people_art::{self, PersonPen},
        pixel_art::sprites::{explosives as explosive_art, icons, menu_icons},
        prompts::hint,
        screen::UiFrame,
        ui_kit::{self, PanelStyle, TextStyle},
        world::drop_sprite,
    },
    Offer, credits_item, offer_items,
};
use crate::{
    input::prompts::Prompt,
    shell::{self, TalkChoice},
};

const TALK_HELP: &[super::super::prompts::Hint] = &[
    hint(Prompt::Navigate, "choose"),
    hint(Prompt::Confirm, "say"),
    hint(Prompt::Back, "goodbye"),
];

const LIST_HELP: &[super::super::prompts::Hint] = &[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "confirm"),
    hint(Prompt::Back, "back to talking"),
];

/// A standing's word and colour.
const fn standing_look(standing: Standing) -> (&'static str, Color) {
    match standing {
        Standing::Cold => ("Holds a grudge", SKY_LIGHT),
        Standing::Stranger => ("Stranger", MIST),
        Standing::Acquainted => ("Acquainted", BONE),
        Standing::Friendly => ("Friendly", LIME),
        Standing::Close => ("Close friend", GOLD),
    }
}

/// The person the seat talks to and how they see its player.
struct Who {
    character: Character,
    standing: Standing,
    affinity: i16,
    mood: MoodBand,
}

fn who(ui: &UiFrame<'_, '_>) -> Option<Who> {
    let talk = ui.seat.village.talk.as_ref()?;
    let character = routine::character(ui.snapshot.terrain.settings(), talk.speaker)?;
    let player = ui.view.player_id;
    let state = ui.snapshot.people.character(talk.speaker);
    let day = ui.snapshot.people_day;
    Some(Who {
        standing: state.memory(player).standing(),
        affinity: ui.snapshot.people.affinity(talk.speaker, player),
        mood: MoodBand::of(state.mood(&character, day, Some(player))),
        character,
    })
}

/// The "how they see you" rows: standing with the affinity behind it, and today's mood.
fn standing_items(who: &Who) -> Vec<SectionItem> {
    let (word, color) = standing_look(who.standing);
    vec![
        SectionItem::stat("Standing", format!("{word} ({:+})", who.affinity), color),
        SectionItem::stat("Mood today", who.mood.name(), AMBER),
    ]
}

/// Lines of the conversation the panel shows (the newest).
const SHOWN_LINES: usize = 3;

/// Draws seat `ui.seat`'s open conversation: the lines so far, how the person sees you, what you
/// can say, and their portrait beside the panel.
pub(in crate::rendering) fn draw_village_talk(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
) {
    let Some(talk) = &ui.seat.village.talk else {
        return;
    };
    let Some(who) = who(ui) else {
        return;
    };
    let conversation = ui
        .snapshot
        .people
        .conversation(ui.view.player_id)
        .filter(|conversation| conversation.npc == talk.npc);
    let mut lines: Vec<SectionItem> = conversation.map_or_else(Vec::new, |conversation| {
        let skip = conversation.lines.len().saturating_sub(SHOWN_LINES);
        conversation
            .lines
            .iter()
            .skip(skip)
            .map(|line| match line.speaker {
                Speaker::Them => {
                    SectionItem::text(format!("{}: \"{}\"", who.character.name.short(), line.text))
                }
                Speaker::You => SectionItem::text(format!("You: {}", line.text)),
                // Notes wrap (a story's title can run long).
                Speaker::Note => SectionItem::text(format!("> {}", line.text)),
            })
            .collect()
    });
    if lines.is_empty() {
        lines.push(SectionItem::text(format!(
            "{} looks up as you come over...",
            who.character.name.short()
        )));
    }
    let choices: Vec<Offer> = shell::talk_choices(Some(&who.character), &ui.view.arc_talk)
        .into_iter()
        .map(|choice| choice_row(ui, &who, choice))
        .collect();
    let (items, about) = offer_items(choices, ui.seat.selected_menu_item);
    let sections = vec![
        Section::new("Conversation", GOLD, lines),
        Section::new("How they see you", SKY_LIGHT, standing_items(&who)),
        Section::new("Say", LIME, items),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        &talk.name,
        &talk.about,
        &ModalContent::new(sections)
            .compact()
            .footer(about)
            .prompts(TALK_HELP),
    );
    draw_portrait_card(draw, &who);
}

fn choice_row(ui: &UiFrame<'_, '_>, who: &Who, choice: TalkChoice) -> Offer {
    let name = who.character.name.short();
    match choice {
        TalkChoice::Story(story) => story_row(ui, name, story),
        TalkChoice::Topic(topic) => {
            use fatherload_simulation::people::TalkTopic;
            let (icon, about) = match topic {
                TalkTopic::Chat => (
                    &menu_icons::PERSON,
                    format!("Pass the time of day. {name} remembers who stops to chat."),
                ),
                TalkTopic::Rumours => (
                    &menu_icons::LETTER,
                    "Rumours: once a day each person may mark a site they know of on your map."
                        .to_owned(),
                ),
                TalkTopic::Life => (
                    &menu_icons::BOOK,
                    format!("What {name} does, wants and worries about."),
                ),
                TalkTopic::BuildingWork => (
                    &menu_icons::HOUSE,
                    format!("{name} takes building work: ask for a quote."),
                ),
            };
            Offer::new(icon, choice.label(), "").about(about)
        }
        TalkChoice::Trade => {
            let count = offers::offers_of(&who.character).len();
            Offer::new(
                &menu_icons::GIFT,
                choice.label(),
                format!("{count} on offer"),
            )
            .about(format!(
                "{name} offers {}. Friends pay less.",
                who.character.offer.name()
            ))
        }
        TalkChoice::Gift => {
            let any = !shell::gift_items(ui.player).is_empty();
            Offer::new(&menu_icons::GIFT, choice.label(), "from your cargo bay")
                .unavailable_if(!any)
                .about("Ore, relics and materials. Some gifts land better than others.")
        }
        TalkChoice::Goodbye => Offer::new(&menu_icons::BACK, choice.label(), "")
            .about(format!("Leave {name} to their day.")),
    }
}

/// A story row of the talk panel: asking, taking a story on, or how it ended.
fn story_row(ui: &UiFrame<'_, '_>, name: &str, story: shell::StoryChoice) -> Offer {
    use fatherload_simulation::world::ArcTalk;
    let arc = match &ui.view.arc_talk {
        ArcTalk::HasWork(arc)
        | ArcTalk::Pitched(arc)
        | ArcTalk::Running(arc)
        | ArcTalk::Ready { arc, .. } => Some(*arc),
        ArcTalk::Nothing => None,
    };
    let template = arc.and_then(fatherload_simulation::people::ArcId::template);
    let title = template.map_or("", |template| template.title);
    match story {
        shell::StoryChoice::Ask { running: false } => {
            Offer::new(&menu_icons::BOOK, story.label(), "a story").about(format!(
                "{name} has something on their mind. Ask, and they'll tell you."
            ))
        }
        shell::StoryChoice::Ask { running: true } => {
            Offer::new(&menu_icons::BOOK, story.label(), title.to_owned()).about(format!(
                "Ask {name} how \"{title}\" stands, and what to do next."
            ))
        }
        shell::StoryChoice::Accept => {
            let full = ui.player.quests.arcs.active.len()
                >= fatherload_simulation::people::story::run::MAX_ACTIVE_ARCS;
            Offer::new(&menu_icons::CONTRACT, story.label(), title.to_owned())
                .unavailable_if(full)
                .about(if full {
                    "Your journal is full: finish or give up a story first.".to_owned()
                } else {
                    format!("Take on \"{title}\". It goes in your journal.")
                })
        }
        shell::StoryChoice::Ending { outcome, .. } => {
            let ending = template.and_then(|template| template.outcomes.get(usize::from(outcome)));
            let default = ending.is_some_and(|ending| {
                ending.when == fatherload_simulation::people::story::OutcomeWhen::Completed
            });
            Offer::new(
                if default {
                    &menu_icons::READY
                } else {
                    &menu_icons::SPLIT
                },
                story.label(),
                if default { "the ending" } else { "another way" },
            )
            .about(format!("Tell {name} how \"{title}\" ends."))
        }
    }
}

/// The person's portrait in a card beside the panel, when the view has room for it.
fn draw_portrait_card(draw: &mut RaylibDrawHandle<'_>, who: &Who) {
    let canvas = ui_kit::canvas();
    let Some(panel) = layout::peek_last_dialog_rect() else {
        return;
    };
    let scale = if canvas.height >= 300 { 4 } else { 2 };
    let art = people_art::PORTRAIT_SIZE * scale;
    let card_w = art + 16;
    let card_h = art + 44;
    let free_left = (panel.x + panel.width) as i32 + 8;
    let right_x = canvas.width - card_w - 8;
    let (x, y) = if right_x >= free_left && card_h + 16 <= canvas.height {
        (right_x, 8)
    } else if (panel.x as i32) - card_w - 8 >= 8 && card_h + 16 <= canvas.height {
        (8, 8)
    } else {
        return;
    };
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, card_w as f32, card_h as f32),
        PanelStyle::Accent,
    );
    ui_kit::text(
        draw,
        who.character.culture.name(),
        x + 8,
        y + 6,
        TextStyle::Bold,
        GOLD,
    );
    let well = Rectangle::new(
        (x + 6) as f32,
        (y + 18) as f32,
        (art + 4) as f32,
        (art + 4) as f32,
    );
    ui_kit::well(draw, well);
    let seconds = ui_kit::seconds();
    people_art::draw_portrait(
        draw,
        PersonPen {
            x: well.x + 2.0,
            y: well.y + 2.0,
            pixel: scale as f32,
        },
        &who.character,
        ((seconds * 0.8) as i32) % 3 != 0,
        seconds,
    );
    let (word, color) = standing_look(who.standing);
    ui_kit::text(draw, word, x + 8, y + card_h - 16, TextStyle::Body, color);
}

/// Draws the trade list of the person seat `ui.seat` talks to.
pub(in crate::rendering) fn draw_person_trade(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
) {
    let Some(talk) = &ui.seat.village.talk else {
        return;
    };
    let Some(who) = who(ui) else {
        return;
    };
    let player = ui.view.player_id;
    let journal = ui.snapshot.people.journal(player);
    let haggler = ui.snapshot.people.has_perk(player, Perk::Haggler);
    let rows: Vec<Offer> = offers::offers_of(&who.character)
        .into_iter()
        .enumerate()
        .map(|(row, offer)| {
            let sold = offer.once
                && journal.is_some_and(|journal| {
                    journal
                        .bought
                        .contains(&(talk.speaker, u8::try_from(row).unwrap_or(u8::MAX)))
                });
            let owned_perk = matches!(offer.deal, PersonDeal::Apprenticeship(perk)
                if journal.is_some_and(|journal| journal.has(perk)));
            let price = offers::price_for(&offer, who.standing, haggler);
            let item = Offer::new(deal_icon(offer.deal), offer.title(), offer.kind.name())
                .about(offer.describe());
            if sold || owned_perk {
                item.tag("SOLD", LIME).unavailable_if(true)
            } else if !offers::standing_allows(who.standing, offer.needs) {
                let (word, color) = standing_look(offer.needs);
                item.tag(&format!("needs {}", word.to_lowercase()), color)
                    .unavailable_if(true)
            } else if offer.price == 0 {
                item.tag("METERED", AMBER)
            } else {
                item.price(price, ui.player.credits)
            }
        })
        .collect();
    let (items, about) = offer_items(rows, ui.seat.selected_menu_item);
    let (word, color) = standing_look(who.standing);
    let status = vec![
        credits_item(ui.player.credits),
        SectionItem::stat("Standing", word, color),
    ];
    UiLayout::screen(draw).modal_with_render_plan(
        &format!("{}'s wares", who.character.name.short()),
        &talk.about,
        &ModalContent::new(vec![
            Section::new("Status", SKY_LIGHT, status),
            Section::new("On offer", GOLD, items),
        ])
        .compact()
        .footer(about)
        .prompts(LIST_HELP),
    );
    draw_portrait_card(draw, &who);
}

const fn deal_icon(deal: PersonDeal) -> &'static super::super::pixel_art::sprite::SpriteDef {
    match deal {
        PersonDeal::Goods(VillageDeal::Part(_)) => &menu_icons::GEAR,
        PersonDeal::Goods(VillageDeal::Cosmetic(_)) => &menu_icons::CROWN,
        PersonDeal::Goods(VillageDeal::Explosives { kind, .. }) => explosive_art::icon(kind),
        PersonDeal::Goods(VillageDeal::Blessing(_)) => &menu_icons::FLAG,
        PersonDeal::Goods(VillageDeal::Repair { .. }) => &menu_icons::HAMMER,
        PersonDeal::Goods(VillageDeal::Refuel { .. }) => &icons::FUEL,
        PersonDeal::Goods(_) => &menu_icons::GIFT,
        PersonDeal::MapSecret => &menu_icons::GLOBE,
        PersonDeal::Rumour => &menu_icons::LETTER,
        PersonDeal::Apprenticeship(_) => &menu_icons::BOOK,
    }
}

/// Draws the gift list for the person seat `ui.seat` talks to.
pub(in crate::rendering) fn draw_person_gift(
    draw: &mut RaylibDrawHandle<'_>,
    ui: &UiFrame<'_, '_>,
) {
    let Some(talk) = &ui.seat.village.talk else {
        return;
    };
    let Some(who) = who(ui) else {
        return;
    };
    let rows: Vec<Offer> = shell::gift_items(ui.player)
        .into_iter()
        .map(|(item, count)| {
            Offer::new(
                drop_sprite(item),
                item_label(item),
                format!("{count} in the bay"),
            )
            .about("Hand one over.")
        })
        .collect();
    let (items, about) = offer_items(rows, ui.seat.selected_menu_item);
    // Friends let slip what they love.
    let hint_line = if matches!(who.standing, Standing::Friendly | Standing::Close) {
        format!(
            "{} has a soft spot for {}.",
            who.character.name.short(),
            offers::favourite_mineral(&who.character).name()
        )
    } else {
        "Get to know them to learn what they like.".to_owned()
    };
    UiLayout::screen(draw).modal_with_render_plan(
        &format!("A gift for {}", who.character.name.short()),
        &talk.about,
        &ModalContent::new(vec![
            Section::new("Your cargo", GOLD, items),
            Section::new("", SKY_LIGHT, vec![SectionItem::text(hint_line)]),
        ])
        .compact()
        .footer(about)
        .prompts(LIST_HELP),
    );
    draw_portrait_card(draw, &who);
}

fn item_label(item: ItemKind) -> String {
    match item {
        ItemKind::Mineral(mineral) => mineral.name().to_owned(),
        ItemKind::Artifact(artifact) => artifact.name().to_owned(),
        ItemKind::Material(material) => material.name().to_owned(),
        other => format!("{other:?}"),
    }
}
