//! Talking with the friendly aliens (Wave 12 aliens track): the talk panel of an alien
//! resident, with its pages.
//!
//! The panel opens like any conversation (the use key beside a resident; a tap of the prompt
//! on touch), and every row is an ordinary menu row (keys, pad, mouse and touch alike: a tap
//! selects a row and confirms it). What the resident says is shown through the seat's own
//! player's understanding of their tongue (`rendering/alien_hud.rs`), so in split screen two
//! seats can read the same alien differently.
//!
//! * **Talk**: chat (the day's words), the keeper's trade (the trader's goods, the lexicon
//!   keeper's lesson, the elder's chapter, the builder's homes), a gift, goodbye.
//! * **Trade**: sell the ores the species prizes from the bay, buy their goods.
//! * **Homes**: the town's plots for sale (and the seat's own home: rest there).
//! * **Gift**: the bay's ore, relics and materials.
//!
//! Every action is a [`PlayerCommand::Alien`] or [`PlayerCommand::People`] command for the
//! seat's own player, validated in the tick (so it works the same online).

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    alien::{
        AlienCommand, AlienSpecies, AlienTown,
        quest::chapter,
        town::{self, BUILDER, ELDER, TEACHER, TRADER},
        trade::AlienGood,
    },
    item::ItemKind,
    people::{CharacterId, PeopleCommand, TalkTopic},
    terrain::MineralKind,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// The page of an alien resident's panel a seat is on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlienPage {
    #[default]
    Talk,
    Trade,
    Homes,
    Gift,
}

/// A seat's alien panel state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeatAlien {
    pub page: AlienPage,
}

impl SeatAlien {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            page: AlienPage::Talk,
        }
    }
}

/// What the elder's chapter row does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChapterRow {
    /// Ask what the town needs.
    Ask,
    /// Take on the chapter asked for.
    Accept(u8),
    /// Say it is done.
    TurnIn(u8),
}

/// One row of the panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlienRow {
    Chat,
    /// The trader's page.
    Trade,
    /// A lesson at the lexicon house.
    Lesson,
    /// The elder's chapter.
    Chapter(ChapterRow),
    /// The builder's homes page.
    Homes,
    Gift,
    Goodbye,
    /// Sell all `count` of an ore they prize.
    Sell(MineralKind, u32),
    Buy(AlienGood),
    /// Buy a plot (its building index).
    Plot(u8),
    /// Rest at one's own home on a plot.
    Rest(u8),
    GiftItem(ItemKind),
    Back,
    /// Raise the next wing of one's own home on a plot (Wave 13 endgame).
    Expand(u8),
    /// Have the neighbours over at one's own home on a plot (Wave 13 endgame).
    Neighbours(u8),
}

/// The town and resident slot of `character`, if an alien resident.
#[must_use]
pub fn resident(character: CharacterId) -> Option<(std::sync::Arc<AlienTown>, usize)> {
    let found = town::town_by_id(character.village)?;
    Some((found, usize::from(character.index)))
}

/// The rows of `page` for `player` talking to `character`.
#[must_use]
pub fn alien_rows(
    world: &WorldState,
    player: Option<PlayerId>,
    character: CharacterId,
    page: AlienPage,
) -> Vec<AlienRow> {
    let Some((found, slot)) = resident(character) else {
        return vec![AlienRow::Goodbye];
    };
    let species = found.species;
    let rig = player.and_then(|player| world.player(player));
    match page {
        AlienPage::Talk => {
            let mut rows = vec![AlienRow::Chat];
            match slot {
                TRADER => rows.push(AlienRow::Trade),
                TEACHER => rows.push(AlienRow::Lesson),
                ELDER => {
                    let log = player.map(|player| world.aliens().log(player, species));
                    let row = match log.as_ref() {
                        Some(log) if log.active.is_some() => {
                            ChapterRow::TurnIn(log.active.map_or(0, |active| active.chapter))
                        }
                        Some(log) if log.offered.is_some() => {
                            ChapterRow::Accept(log.offered.unwrap_or(0))
                        }
                        _ => ChapterRow::Ask,
                    };
                    rows.push(AlienRow::Chapter(row));
                }
                BUILDER => rows.push(AlienRow::Homes),
                _ => {}
            }
            rows.extend([AlienRow::Gift, AlienRow::Goodbye]);
            rows
        }
        AlienPage::Trade => {
            let mut rows: Vec<AlienRow> = species
                .def()
                .valued_ores
                .iter()
                .filter_map(|(ore, _)| {
                    let count = rig.map_or(0, |rig| rig.cargo_bay.count(*ore));
                    (count > 0).then_some(AlienRow::Sell(*ore, count))
                })
                .collect();
            rows.extend(species.def().goods.iter().map(|good| AlienRow::Buy(*good)));
            rows.push(AlienRow::Back);
            rows
        }
        AlienPage::Homes => {
            let mut rows: Vec<AlienRow> = found
                .home_plots()
                .flat_map(|plot| {
                    let home = world.aliens().home_at(found.id, plot.index);
                    match home {
                        // A finished home of one's own: rest, grow the estate (Wave 13), have
                        // the neighbours over.
                        Some(home) if Some(home.owner) == player && home.home.is_some() => vec![
                            AlienRow::Rest(plot.index),
                            AlienRow::Expand(plot.index),
                            AlienRow::Neighbours(plot.index),
                        ],
                        _ => vec![AlienRow::Plot(plot.index)],
                    }
                })
                .collect();
            rows.push(AlienRow::Back);
            rows
        }
        AlienPage::Gift => {
            let mut rows: Vec<AlienRow> = rig
                .map_or_default(super::village::gift_items)
                .into_iter()
                .map(|(item, _)| AlienRow::GiftItem(item))
                .collect();
            rows.push(AlienRow::Back);
            rows
        }
    }
}

/// What the HUD shows for a row: its label and the right-hand detail.
#[must_use]
pub fn row_text(
    world: &WorldState,
    player: Option<PlayerId>,
    found: &AlienTown,
    row: AlienRow,
) -> (String, String) {
    let species = found.species;
    let player_id = player.unwrap_or(PlayerId::new(0));
    match row {
        AlienRow::Chat => ("Chat".to_owned(), "learn a few words".to_owned()),
        AlienRow::Trade => ("Trade".to_owned(), "ores for goods".to_owned()),
        AlienRow::Lesson => (
            "Take a lesson".to_owned(),
            format!(
                "{} cr, once a day",
                fatherload_simulation::alien::language::LESSON_PRICE
            ),
        ),
        AlienRow::Chapter(ChapterRow::Ask) => {
            ("What does the town need?".to_owned(), String::new())
        }
        AlienRow::Chapter(ChapterRow::Accept(index)) => (
            "I'll do it".to_owned(),
            chapter(species, index).map_or_else(String::new, |def| def.title.to_owned()),
        ),
        AlienRow::Chapter(ChapterRow::TurnIn(index)) => (
            "It is done".to_owned(),
            chapter(species, index).map_or_else(String::new, |def| def.title.to_owned()),
        ),
        AlienRow::Homes => ("Homes for sale".to_owned(), String::new()),
        AlienRow::Gift => ("Give a gift".to_owned(), "from your cargo bay".to_owned()),
        // Their farewell in their own tongue (the meaning ran past the row's width).
        AlienRow::Goodbye => (species.def().farewell.0.to_owned(), "goodbye".to_owned()),
        AlienRow::Sell(ore, count) => {
            let unit = world.alien_ore_price(player_id, species, ore).unwrap_or(0);
            (
                format!("Sell {count} {}", ore.name()),
                format!("{} cr", u64::from(unit) * u64::from(count)),
            )
        }
        AlienRow::Buy(good) => (
            good.name().to_owned(),
            format!("{} cr", world.alien_good_price(player_id, species, good)),
        ),
        AlienRow::Plot(plot) => {
            let home = world.aliens().home_at(found.id, plot);
            let now = world.world_millis();
            let detail = match home {
                None => format!(
                    "{} cr",
                    fatherload_simulation::world::alien_home_price(species)
                ),
                Some(home) if Some(home.owner) == player => {
                    format!("yours: building {}%", home.progress_per_mille(now) / 10)
                }
                Some(_) => "sold".to_owned(),
            };
            (format!("Plot {plot}"), detail)
        }
        AlienRow::Rest(_) => ("Rest at your home".to_owned(), "air and hull".to_owned()),
        AlienRow::Expand(plot) => world.estate_quote(found.id, plot).map_or_else(
            || ("Your estate".to_owned(), "every wing raised".to_owned()),
            |(wing, bill)| {
                let materials =
                    fatherload_simulation::endgame::works::materials_line(bill.materials);
                (
                    format!("Raise a wing: {}", wing.name().to_lowercase()),
                    format!("{} cr + {materials}", bill.credits),
                )
            },
        ),
        AlienRow::Neighbours(plot) => {
            let today = world.people_clock().day;
            let called = world.endgame().estate(found.id, plot).visited_day == Some(today);
            (
                "Have the neighbours over".to_owned(),
                if called {
                    "they called today".to_owned()
                } else {
                    "a gift and their words".to_owned()
                },
            )
        }
        AlienRow::GiftItem(item) => (item_label(item), String::new()),
        AlienRow::Back => ("Back".to_owned(), String::new()),
    }
}

impl SeatUi {
    /// Opens the alien panel for the talk the seat just started (`start_talk`).
    pub(super) const fn open_alien_talk(&mut self) {
        self.alien.page = AlienPage::Talk;
        self.selected_menu_item = 0;
        self.modal = Some(ModalScreen::AlienTalk);
    }

    fn send_alien(
        &mut self,
        ctx: &SeatContext<'_>,
        command: PlayerCommand,
        requests: &mut Vec<ShellRequest>,
    ) {
        if ctx.player.is_some() {
            self.village.hold_people_rows();
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command,
            });
        }
    }

    /// The alien panel.
    #[allow(clippy::too_many_lines, reason = "one arm per row reads best together")]
    pub(super) fn handle_alien_talk(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(talk) = self.village.talk.clone() else {
            self.modal = None;
            return;
        };
        if ctx.world.npc(talk.npc).is_none() {
            self.village.talk = None;
            self.modal = None;
            return;
        }
        if input.backs_out() {
            if self.alien.page == AlienPage::Talk {
                self.end_alien_talk(&ctx, requests);
            } else {
                self.alien.page = AlienPage::Talk;
                self.selected_menu_item = 0;
            }
            cues.push(SoundCue::UiBack);
            return;
        }
        let rows = alien_rows(ctx.world, ctx.player, talk.speaker, self.alien.page);
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if !(input.confirm || input.interact) {
            return;
        }
        let npc = talk.npc;
        let Some(row) = rows.get(self.selected_menu_item).copied() else {
            return;
        };
        cues.push(SoundCue::Ui);
        let alien = |command: AlienCommand| PlayerCommand::Alien(command);
        match row {
            AlienRow::Chat => self.send_alien(
                &ctx,
                PlayerCommand::People(PeopleCommand::Talk {
                    npc,
                    topic: TalkTopic::Chat,
                }),
                requests,
            ),
            AlienRow::Trade => self.turn_page(AlienPage::Trade),
            AlienRow::Homes => self.turn_page(AlienPage::Homes),
            AlienRow::Gift => self.turn_page(AlienPage::Gift),
            AlienRow::Back => self.turn_page(AlienPage::Talk),
            AlienRow::Lesson => {
                self.send_alien(&ctx, alien(AlienCommand::Lesson { npc }), requests);
            }
            AlienRow::Chapter(ChapterRow::Ask) => {
                self.send_alien(&ctx, alien(AlienCommand::AskChapter { npc }), requests);
            }
            AlienRow::Chapter(ChapterRow::Accept(_)) => {
                self.send_alien(&ctx, alien(AlienCommand::AcceptChapter { npc }), requests);
            }
            AlienRow::Chapter(ChapterRow::TurnIn(_)) => {
                self.send_alien(&ctx, alien(AlienCommand::TurnInChapter { npc }), requests);
            }
            AlienRow::Sell(mineral, count) => self.send_alien(
                &ctx,
                alien(AlienCommand::Sell {
                    npc,
                    mineral,
                    count,
                }),
                requests,
            ),
            AlienRow::Buy(good) => {
                self.send_alien(&ctx, alien(AlienCommand::Buy { npc, good }), requests);
            }
            AlienRow::Plot(plot) => {
                self.send_alien(&ctx, alien(AlienCommand::BuyHome { npc, plot }), requests);
            }
            AlienRow::Rest(plot) => {
                if let Some((found, _)) = resident(talk.speaker) {
                    self.send_alien(
                        &ctx,
                        alien(AlienCommand::RestAtHome {
                            town: found.id,
                            plot,
                        }),
                        requests,
                    );
                }
            }
            AlienRow::GiftItem(item) => {
                self.send_alien(
                    &ctx,
                    PlayerCommand::People(PeopleCommand::Gift { npc, item }),
                    requests,
                );
                self.turn_page(AlienPage::Talk);
            }
            AlienRow::Expand(plot) => {
                if let Some((found, _)) = resident(talk.speaker) {
                    self.send_alien(
                        &ctx,
                        alien(AlienCommand::ExpandHome {
                            town: found.id,
                            plot,
                        }),
                        requests,
                    );
                }
            }
            AlienRow::Neighbours(plot) => {
                if let Some((found, _)) = resident(talk.speaker) {
                    self.send_alien(
                        &ctx,
                        alien(AlienCommand::CallNeighbours {
                            town: found.id,
                            plot,
                        }),
                        requests,
                    );
                }
            }
            AlienRow::Goodbye => self.end_alien_talk(&ctx, requests),
        }
    }

    const fn turn_page(&mut self, page: AlienPage) {
        self.alien.page = page;
        self.selected_menu_item = 0;
    }

    fn end_alien_talk(&mut self, ctx: &SeatContext<'_>, requests: &mut Vec<ShellRequest>) {
        self.village.talk = None;
        self.send_alien(ctx, PlayerCommand::People(PeopleCommand::Goodbye), requests);
        self.selected_menu_item = 0;
        self.alien.page = AlienPage::Talk;
        self.modal = None;
    }
}

/// A gift row's name.
#[must_use]
pub fn item_label(item: ItemKind) -> String {
    match item {
        ItemKind::Mineral(mineral) => mineral.name().to_owned(),
        ItemKind::Artifact(artifact) => artifact.name().to_owned(),
        ItemKind::Material(material) => material.name().to_owned(),
        other => format!("{other:?}"),
    }
}

/// The species of the person a seat talks to, if an alien.
#[must_use]
pub fn talk_species(character: CharacterId) -> Option<AlienSpecies> {
    resident(character).map(|(found, _)| found.species)
}
