//! What a parsed palette command does: the test-mode actions it submits for the palette's seat
//! (the same `TestModeAction`s the Atlas, `--start-at` and the bots submit), the local switches
//! it flips (debug views, the time scale) and the lines it prints.

use std::collections::BTreeMap;

use fatherload_simulation::{
    PlayerCommand, PlayerId, TestModeAction, WorldState,
    place::Place,
    test_mode::{
        palette_stock::StockRequest,
        travel::{ArrivalTarget, Destination, Pick},
    },
};

use super::{
    names::Giveable,
    parse::{COMMANDS, Command, Give, Step, TpTarget, help_line},
};
use crate::debug_views::{DebugView, TimeScale};

/// Something a command asks the game to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Submit this for the palette's seat now.
    Act(PlayerCommand),
    /// Submit this once the seat's rig stands on the home world (a home-depth travel from
    /// another place goes home first; TODO-SEAM(w14-index): the index lands from anywhere).
    ActWhenHome(PlayerCommand),
    /// Switch a debug view (`None` flips it).
    View(DebugView, Option<bool>),
    /// Every debug view off.
    ViewsOff,
    /// The local time scale.
    Speed(TimeScale),
}

/// The seat's own travel memory: candidate cursors, the back-stack, bookmarks and the last
/// preview. TODO-SEAM(w14-index): the index keeps bookmarks and the back-stack in the world
/// (saved and replicated); until then the palette keeps them for the session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatTravel {
    /// The `next`/`prev` cursor of each kind (by its key).
    cursors: BTreeMap<String, i32>,
    /// Spots travelled from, newest last.
    pub back: Vec<Destination>,
    /// Named spots.
    pub bookmarks: BTreeMap<String, Destination>,
    /// What `go` travels to (the last `tp … ?`).
    pub preview: Option<Destination>,
    /// Rolls for `random` picks (each roll sent in the action, so every peer agrees).
    rolls: u32,
}

impl SeatTravel {
    /// A seat's travel memory whose `random` picks start from the wall clock, so `random`
    /// differs from one session to the next (the roll travels inside the action, so the
    /// simulation stays deterministic; tests use `default`, which starts from zero).
    #[must_use]
    pub fn seeded() -> Self {
        let clock = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| {
                u32::try_from(since.as_secs() & 0xFFFF_FFFF).unwrap_or(0) ^ since.subsec_nanos()
            });
        Self {
            rolls: clock,
            ..Self::default()
        }
    }
}

/// Most spots the back-stack keeps.
const MAX_BACK: usize = 32;

/// What a command did: its effects and the lines to print.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Dispatched {
    pub effects: Vec<Effect>,
    pub lines: Vec<String>,
}

impl Dispatched {
    fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    fn act(mut self, command: PlayerCommand) -> Self {
        self.effects.push(Effect::Act(command));
        self
    }
}

/// What dispatching reads: the world and who the palette acts for.
#[derive(Clone, Copy, Debug)]
pub struct DispatchContext<'w> {
    pub world: &'w WorldState,
    /// The palette seat's player.
    pub player: PlayerId,
    /// An online session (the time scale stays real time there).
    pub online: bool,
}

/// Whether the console track's stub already applies `request` (the rest wait for w14-stock).
/// TODO-SEAM(w14-stock): every request is applied once w14-stock's actions are wired.
#[must_use]
pub const fn stock_stub_applies(request: StockRequest) -> bool {
    matches!(
        request,
        StockRequest::GiveAll
            | StockRequest::Give { .. }
            | StockRequest::EndingDone
            | StockRequest::Spawn(_)
    )
}

fn stock(request: StockRequest, what: &str) -> Dispatched {
    let done = Dispatched::default().act(PlayerCommand::TestMode(TestModeAction::PaletteStock(
        request,
    )));
    if stock_stub_applies(request) {
        done.line(what.to_owned())
    } else {
        done.line(format!("{what} (sent; applies once w14-stock lands)"))
    }
}

/// The rig's own spot, for the back-stack and bookmarks.
fn here(world: &WorldState, player: PlayerId) -> Option<Destination> {
    let (place, pos) = world.rig_world_pos(player)?;
    let (column, row) = pos.tile();
    Some(Destination::Spot { place, column, row })
}

/// Runs one parsed command for the palette's seat.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per palette command, in help order"
)]
pub fn dispatch(command: Command, ctx: DispatchContext<'_>, seat: &mut SeatTravel) -> Dispatched {
    match command {
        Command::Tp { target, preview } => match destination(&target, ctx, seat) {
            Ok(destination) => {
                if preview {
                    preview_card(destination, ctx, seat)
                } else {
                    travel(destination, !matches!(target, TpTarget::Back), ctx, seat)
                }
            }
            Err(error) => Dispatched::default().line(error),
        },
        Command::Go => seat.preview.take().map_or_else(
            || Dispatched::default().line("nothing previewed (tp … ? first)"),
            |destination| travel(destination, true, ctx, seat),
        ),
        Command::Bookmark(name) => match here(ctx.world, ctx.player) {
            Some(spot) => {
                seat.bookmarks.insert(name.clone(), spot);
                Dispatched::default().line(format!("bookmarked here as \"{name}\""))
            }
            None => Dispatched::default().line("no rig to bookmark"),
        },
        Command::Give(Give::All) => stock(StockRequest::GiveAll, "gave everything"),
        Command::Give(Give::One { what, count }) => match what {
            Giveable::Item(item) => stock(
                StockRequest::Give { item, count },
                &format!("gave {count} {}", item.name()),
            ),
            Giveable::Supply(supply) => stock(
                StockRequest::GiveSupply {
                    supply,
                    count: u8::try_from(count).unwrap_or(u8::MAX),
                },
                &format!("gave {count} {}", supply.name()),
            ),
        },
        Command::Gate(None) => stock(StockRequest::GateAll, "every gate cleared"),
        Command::Gate(Some(boss)) => stock(
            StockRequest::Gate(boss),
            &format!("{}'s gate cleared", boss.name()),
        ),
        Command::EndingDone => stock(StockRequest::EndingDone, "the Heart woke: space is open"),
        Command::Spawn(kind) => stock(
            StockRequest::Spawn(kind),
            &format!("{} beside the rig", kind.name()),
        ),
        Command::Clear => stock(StockRequest::ClearEnemies, "hostiles cleared"),
        Command::Freeze(on) => {
            let on = on.unwrap_or(true);
            stock(
                StockRequest::FreezeAi(on),
                if on {
                    "creature AI frozen"
                } else {
                    "creature AI thawed"
                },
            )
        }
        Command::Time(minute) => stock(
            StockRequest::SetTime {
                minute_of_day: minute,
            },
            &format!("clock set to {:02}:{:02}", minute / 60, minute % 60),
        ),
        Command::Weather(weather) => stock(
            StockRequest::SetWeather(weather),
            &format!("weather: {}", weather.word()),
        ),
        Command::Difficulty(None) => Dispatched::default().line(format!(
            "difficulty: {}",
            ctx.world.enemy_difficulty().name()
        )),
        Command::Difficulty(Some(difficulty)) => Dispatched::default()
            .act(PlayerCommand::SetEnemyDifficulty(difficulty))
            .line(format!("difficulty: {}", difficulty.name())),
        Command::View(view, on) => {
            let mut done = Dispatched::default();
            done.effects.push(Effect::View(view, on));
            done
        }
        Command::ViewsOff => {
            let mut done = Dispatched::default().line("every debug view off");
            done.effects.push(Effect::ViewsOff);
            done
        }
        Command::Speed(scale) => {
            if ctx.online {
                return Dispatched::default()
                    .line("the time scale runs on a local game only (online keeps real time)");
            }
            let mut done = Dispatched::default().line(if scale == TimeScale::Paused {
                "paused (speed 1 resumes)".to_owned()
            } else {
                format!("speed x{}", scale.word())
            });
            done.effects.push(Effect::Speed(scale));
            done
        }
        Command::Help(None) => {
            let mut done = Dispatched::default();
            for command in COMMANDS {
                if let Some(line) = help_line(command) {
                    done.lines.push(line.to_owned());
                }
            }
            done
        }
        Command::Help(Some(command)) => Dispatched::default().line(
            help_line(&command).map_or_else(|| format!("no command \"{command}\""), str::to_owned),
        ),
        // The palette itself switches seats (`Console::run`).
        Command::Seat(_) => Dispatched::default(),
    }
}

/// The cursor of `key` after `step` (`next` and `prev` cycle outward from the nearest).
fn cursor(seat: &mut SeatTravel, key: String, step: Step) -> Pick {
    let cursor = seat.cursors.entry(key).or_insert(0);
    match step {
        Step::Nearest => {
            *cursor = 0;
            Pick::NEAREST
        }
        Step::Next => {
            *cursor += 1;
            Pick::Nth(*cursor)
        }
        Step::Previous => {
            *cursor -= 1;
            Pick::Nth(*cursor)
        }
        Step::Random => {
            seat.rolls = seat.rolls.wrapping_add(1);
            // A well-mixed roll; any value works (it travels inside the action).
            Pick::Random(seat.rolls.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995)
        }
    }
}

/// The destination `target` names for this seat.
///
/// # Errors
///
/// When it names nothing (no such player, bookmark or earlier spot).
pub fn destination(
    target: &TpTarget,
    ctx: DispatchContext<'_>,
    seat: &mut SeatTravel,
) -> Result<Destination, String> {
    Ok(match target {
        TpTarget::Planet { step, theme } => Destination::Planet {
            theme: *theme,
            pick: cursor(
                seat,
                format!("planet:{}", theme.map_or("any", |theme| theme.name())),
                *step,
            ),
        },
        TpTarget::Station(step) => Destination::Station {
            pick: cursor(seat, "station".to_owned(), *step),
        },
        TpTarget::Descent(descent) => Destination::Descent(*descent),
        TpTarget::Depth(rows) => Destination::Depth(*rows),
        TpTarget::Stratum(stratum) => Destination::Stratum(*stratum),
        TpTarget::Boss(boss) => Destination::Boss(*boss),
        TpTarget::Home => Destination::Home,
        TpTarget::Moon => Destination::Moon,
        TpTarget::Player(number) => {
            let crew: Vec<PlayerId> = ctx.world.player_ids().collect();
            let other = crew
                .get(usize::from(*number) - 1)
                .copied()
                .ok_or_else(|| format!("no player {number} (the crew has {})", crew.len()))?;
            if other == ctx.player {
                return Err(format!("player {number} is you"));
            }
            Destination::Player(other)
        }
        TpTarget::Bookmark(name) => *seat
            .bookmarks
            .get(name)
            .ok_or_else(|| format!("no bookmark \"{name}\" (bookmark <name> saves one)"))?,
        TpTarget::Back => seat
            .back
            .pop()
            .ok_or_else(|| "nowhere to go back to".to_owned())?,
    })
}

/// The preview card for `destination`; `go` travels there.
fn preview_card(
    destination: Destination,
    ctx: DispatchContext<'_>,
    seat: &mut SeatTravel,
) -> Dispatched {
    match ctx.world.resolve_destination(ctx.player, destination) {
        Some(arrival) => {
            seat.preview = Some(destination);
            Dispatched::default()
                .line(format!("? {}", arrival.name))
                .line(format!("  {}", arrival.detail))
                .line(format!("  lands: {}", target_words(arrival.target)))
                .line("  go to travel there")
        }
        None => Dispatched::default().line(no_answer(destination)),
    }
}

fn no_answer(destination: Destination) -> String {
    match destination {
        Destination::Station { .. } => {
            "stations come with the destination index (TODO-SEAM w14-index)".to_owned()
        }
        Destination::Planet { .. } => "no such planet near here (is space open?)".to_owned(),
        _ => "nowhere to land in this world".to_owned(),
    }
}

fn target_words(target: ArrivalTarget) -> String {
    match target {
        ArrivalTarget::HomeRow { row } => format!("a fresh pocket at row {row}"),
        ArrivalTarget::Place(place) => format!("the arrival point of {}", place_words(place)),
        ArrivalTarget::Spot { place, column, row } => {
            format!("({column}, {row}) on {}", place_words(place))
        }
    }
}

fn place_words(place: Place) -> String {
    match place {
        Place::Space => "free flight".to_owned(),
        Place::OnBody(fatherload_simulation::place::BodyId::Home) => "home".to_owned(),
        Place::OnBody(fatherload_simulation::place::BodyId::Moon) => "the Moon".to_owned(),
        Place::OnBody(fatherload_simulation::place::BodyId::Space(key)) => format!(
            "body {} of system {} in sector ({}, {})",
            key.body, key.system, key.sector.x, key.sector.y
        ),
    }
}

/// Whether the seat's rig stands on the home world's ground (not away in space or on a body).
fn at_home(world: &WorldState, player: PlayerId) -> bool {
    world.place_of(player).is_home_store()
        && world.player(player).is_some_and(|rig| !rig.flight.away())
}

/// The travel actions for `destination` (pushing where the rig was onto the back-stack).
fn travel(
    destination: Destination,
    remember: bool,
    ctx: DispatchContext<'_>,
    seat: &mut SeatTravel,
) -> Dispatched {
    let Some(arrival) = ctx.world.resolve_destination(ctx.player, destination) else {
        return Dispatched::default().line(no_answer(destination));
    };
    if remember && let Some(spot) = here(ctx.world, ctx.player) {
        seat.back.push(spot);
        if seat.back.len() > MAX_BACK {
            seat.back.remove(0);
        }
    }
    let travel = PlayerCommand::TestMode(TestModeAction::PaletteTravel(destination));
    let mut done = Dispatched::default().line(format!("-> {} ({})", arrival.name, arrival.detail));
    if matches!(arrival.target, ArrivalTarget::HomeRow { .. }) && !at_home(ctx.world, ctx.player) {
        done.effects.push(Effect::Act(PlayerCommand::TestMode(
            TestModeAction::PaletteTravel(Destination::Home),
        )));
        done.effects.push(Effect::ActWhenHome(travel));
    } else {
        done.effects.push(Effect::Act(travel));
    }
    done
}
