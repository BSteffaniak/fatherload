//! What a parsed palette command does: the test-mode actions it submits for the palette's seat
//! (the same `TestModeAction`s the Atlas, `--start-at` and the bots submit: the destination
//! index's `TravelTo` / `TravelBack` / `BookmarkHere` and the god-mode toolkit's stock, progress
//! and creature actions), the local switches it flips (debug views, the time scale) and the
//! lines it prints.

use std::collections::BTreeMap;

use fatherload_simulation::{
    PlayerCommand, PlayerId, TestModeAction, WorldState,
    descent::{Descent, DescentBoss},
    destination::{
        Destination, DestinationFilter, DestinationId, DestinationKind, Selector, TravelRequest,
    },
    place::{BodyId, Place},
};

use super::{
    names::BossName,
    parse::{COMMANDS, Command, Give, Step, TpTarget, help_line},
};
use crate::debug_views::{DebugView, TimeScale};

/// Something a command asks the game to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Submit this for the palette's seat now.
    Act(PlayerCommand),
    /// Switch a debug view (`None` flips it).
    View(DebugView, Option<bool>),
    /// Every debug view off.
    ViewsOff,
    /// The local time scale.
    Speed(TimeScale),
}

/// A trip the palette sends: a request of the destination index, or back down the
/// back-stack the world keeps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trip {
    To(TravelRequest),
    Back,
}

impl Trip {
    /// The action that makes the trip.
    #[must_use]
    pub const fn action(self) -> TestModeAction {
        match self {
            Self::To(request) => TestModeAction::TravelTo(request),
            Self::Back => TestModeAction::TravelBack,
        }
    }
}

/// The seat's own palette memory: bookmark names (the bookmarks themselves are the world's,
/// saved and replicated), the last preview and the `random` rolls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatTravel {
    /// Names given to bookmarks (`bookmark <name>`), with the spot each names.
    pub bookmarks: BTreeMap<String, DestinationId>,
    /// What `go` travels to (the last `tp … ?`).
    pub preview: Option<Trip>,
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

    /// The next `random` roll.
    const fn roll(&mut self) -> u32 {
        self.rolls = self.rolls.wrapping_add(1);
        // A well-mixed roll; any value works (it travels inside the action).
        self.rolls.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995
    }
}

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

    fn test(self, action: TestModeAction) -> Self {
        self.act(PlayerCommand::TestMode(action))
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

/// The Descent whose gate `boss` guards (0 = the Crust .. 7 = the Heart).
#[must_use]
pub fn gate_descent(boss: DescentBoss) -> Option<u8> {
    Descent::ALL
        .iter()
        .position(|descent| descent.gate().is_some_and(|gate| gate.boss == boss))
        .and_then(|index| u8::try_from(index).ok())
}

/// Runs one parsed command for the palette's seat.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per palette command, in help order"
)]
pub fn dispatch(command: Command, ctx: DispatchContext<'_>, seat: &mut SeatTravel) -> Dispatched {
    match command {
        Command::Tp { target, preview } => match trip(&target, ctx, seat) {
            Ok(trip) => {
                if preview {
                    preview_card(trip, ctx, seat)
                } else {
                    travel(trip, ctx)
                }
            }
            Err(error) => Dispatched::default().line(error),
        },
        Command::Go => seat.preview.take().map_or_else(
            || Dispatched::default().line("nothing previewed (tp … ? first)"),
            |trip| travel(trip, ctx),
        ),
        Command::Bookmark(name) => match ctx.world.travel_here(ctx.player) {
            Some(spot) => {
                seat.bookmarks.insert(name.clone(), spot);
                Dispatched::default()
                    .test(TestModeAction::BookmarkHere)
                    .line(format!("bookmarked here as \"{name}\""))
            }
            None => Dispatched::default().line("no rig to bookmark"),
        },
        Command::Give(Give::All) => Dispatched::default()
            .test(TestModeAction::StockAll)
            .line("stocked with everything"),
        Command::Give(Give::One { what, count }) => Dispatched::default()
            .test(TestModeAction::Give(what, count))
            .line(format!(
                "{} set to {}",
                what.name(),
                count.min(what.stack())
            )),
        Command::Restock => Dispatched::default()
            .test(TestModeAction::Restock)
            .line("consumables refilled"),
        Command::Supplies(on) => {
            let on = on.unwrap_or_else(|| {
                !ctx.world
                    .test_mode()
                    .is_some_and(|state| state.infinite_supplies)
            });
            Dispatched::default()
                .test(TestModeAction::SetInfiniteSupplies(on))
                .line(format!(
                    "infinite supplies {}",
                    if on { "on" } else { "off" }
                ))
        }
        Command::Gate(None) => {
            let mut done = Dispatched::default();
            for descent in 0..u8::try_from(Descent::ALL.len()).unwrap_or(u8::MAX) {
                done = done.test(TestModeAction::SetGateCleared {
                    descent,
                    cleared: true,
                });
            }
            done.line("every gate cleared")
        }
        Command::Gate(Some(boss)) => gate_descent(boss).map_or_else(
            || Dispatched::default().line(format!("{} guards no gate", boss.name())),
            |descent| {
                Dispatched::default()
                    .test(TestModeAction::SetGateCleared {
                        descent,
                        cleared: true,
                    })
                    .line(format!("{}'s gate cleared", boss.name()))
            },
        ),
        Command::EndingDone => Dispatched::default()
            .test(TestModeAction::SetEndingDone(true))
            .line("the Heart woke: space is open"),
        Command::Spawn(kind) => Dispatched::default()
            .test(TestModeAction::SpawnCreature(kind))
            .line(format!("{} beside the rig", kind.name())),
        Command::Clear => Dispatched::default()
            .test(TestModeAction::ClearNearbyEnemies)
            .line("hostiles cleared"),
        Command::Freeze(on) => {
            let on =
                on.unwrap_or_else(|| !ctx.world.test_mode().is_some_and(|state| state.ai_frozen));
            Dispatched::default()
                .test(TestModeAction::SetAiFrozen(on))
                .line(if on {
                    "creature AI frozen"
                } else {
                    "creature AI thawed"
                })
        }
        Command::Time(minutes) => Dispatched::default()
            .test(TestModeAction::SetTimeOfDay { minutes })
            .line(format!(
                "clock set to {:02}:{:02}",
                minutes / 60,
                minutes % 60
            )),
        Command::Weather(weather) => Dispatched::default()
            .test(TestModeAction::SetWeather(weather))
            .line(format!(
                "weather: {}",
                weather.map_or("each world's own", |id| {
                    fatherload_simulation::bodylife::weather::weather_kind(id).name
                })
            )),
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

/// The index's selector for a palette step (`random` rolls a fresh seed).
const fn selector(step: Step, seat: &mut SeatTravel) -> Selector {
    match step {
        Step::Nearest => Selector::Nearest,
        Step::Next => Selector::Next,
        Step::Previous => Selector::Previous,
        Step::Random => Selector::Random(seat.roll()),
    }
}

/// The candidate of `kind` whose card answers `test`, picked by `step` among those that do.
fn named(
    ctx: DispatchContext<'_>,
    kind: DestinationKind,
    step: Step,
    seat: &mut SeatTravel,
    what: &str,
    test: impl Fn(&Destination) -> bool,
) -> Result<TravelRequest, String> {
    let found: Vec<Destination> = ctx
        .world
        .destinations(ctx.player, kind, DestinationFilter::Any)
        .into_iter()
        .filter(|candidate| test(candidate))
        .collect();
    let pick = match step {
        Step::Nearest | Step::Next => found.first(),
        Step::Previous => found.last(),
        Step::Random => {
            let roll = usize::try_from(seat.roll()).unwrap_or(0);
            found.get(roll % found.len().max(1))
        }
    };
    pick.map(|candidate| TravelRequest::exact(candidate.id))
        .ok_or_else(|| format!("no {what} near here"))
}

/// The trip `target` names for this seat: a request of the destination index.
///
/// # Errors
///
/// When it names nothing (no such player or bookmark, or the boss has no lair here).
pub fn trip(
    target: &TpTarget,
    ctx: DispatchContext<'_>,
    seat: &mut SeatTravel,
) -> Result<Trip, String> {
    Ok(Trip::To(match target {
        TpTarget::Planet { step, theme } => {
            let request = TravelRequest::new(DestinationKind::Planet, selector(*step, seat));
            theme.map_or(request, |theme| {
                request.filtered(DestinationFilter::Theme(theme))
            })
        }
        TpTarget::Kind { kind, step, name } => match name {
            None => TravelRequest::new(*kind, selector(*step, seat)),
            Some(name) => {
                let words = name.replace('_', " ");
                named(ctx, *kind, *step, seat, &words, |candidate| {
                    candidate.matches(&words)
                })?
            }
        },
        TpTarget::Descent(descent) => TravelRequest::descent(*descent),
        TpTarget::Depth(metres) => TravelRequest::depth_metres(*metres),
        TpTarget::Stratum(stratum) => TravelRequest::nearest(DestinationKind::Stratum)
            .filtered(DestinationFilter::Stratum(*stratum)),
        TpTarget::Boss(boss) => boss_request(*boss, ctx, seat)?,
        TpTarget::Home => TravelRequest::nearest(DestinationKind::HomeTown),
        TpTarget::Moon => TravelRequest::nearest(DestinationKind::Moon),
        TpTarget::Player(number) => {
            let crew: Vec<PlayerId> = ctx.world.player_ids().collect();
            let other = crew
                .get(usize::from(*number) - 1)
                .copied()
                .ok_or_else(|| format!("no player {number} (the crew has {})", crew.len()))?;
            if other == ctx.player {
                return Err(format!("player {number} is you"));
            }
            TravelRequest::nearest(DestinationKind::Player)
                .filtered(DestinationFilter::Player(other))
        }
        TpTarget::Bookmark(name) => {
            let id = match name.parse::<usize>() {
                Ok(number) if number >= 1 => ctx
                    .world
                    .player_travel(ctx.player)
                    .bookmarks
                    .get(number - 1)
                    .copied()
                    .ok_or_else(|| format!("no bookmark {number}"))?,
                _ => *seat
                    .bookmarks
                    .get(name)
                    .ok_or_else(|| format!("no bookmark \"{name}\" (bookmark <name> saves one)"))?,
            };
            TravelRequest::exact(id.as_kind(DestinationKind::Bookmark))
        }
        TpTarget::Back => {
            if ctx.world.player_travel(ctx.player).recent.is_empty() {
                return Err("nowhere to go back to".to_owned());
            }
            return Ok(Trip::Back);
        }
    }))
}

/// A boss's trip: a gate boss's arena (the Heart's chamber), a theme boss's lair or a
/// faction's flagship.
fn boss_request(
    boss: BossName,
    ctx: DispatchContext<'_>,
    seat: &mut SeatTravel,
) -> Result<TravelRequest, String> {
    match boss {
        BossName::Gate(DescentBoss::TheHeart) => Ok(TravelRequest::nearest(DestinationKind::Heart)),
        BossName::Gate(boss) => {
            let descent = gate_descent(boss).and_then(|index| Descent::ALL.get(usize::from(index)));
            named(
                ctx,
                DestinationKind::BossArena,
                Step::Nearest,
                seat,
                boss.name(),
                |candidate| {
                    candidate.info.boss.as_deref() == Some(boss.name())
                        || (candidate.info.boss.is_none()
                            && candidate.info.descent == descent.copied())
                },
            )
            .or_else(|_| {
                // A set piece's gate (the Sinkhole, the Star Core) is its chamber.
                named(
                    ctx,
                    DestinationKind::GateSetPiece,
                    Step::Nearest,
                    seat,
                    boss.name(),
                    |candidate| candidate.info.descent == descent.copied(),
                )
            })
        }
        BossName::Theme(index) => {
            let name = fatherload_simulation::endgame::hunt::THEME_BOSSES
                .get(index)
                .map_or("", |boss| boss.name);
            named(
                ctx,
                DestinationKind::ThemeBossLair,
                Step::Nearest,
                seat,
                name,
                |candidate| candidate.info.boss.as_deref() == Some(name),
            )
        }
        BossName::Flagship(faction) => named(
            ctx,
            DestinationKind::FactionBossLair,
            Step::Nearest,
            seat,
            &format!("{} flagship", faction.name()),
            |candidate| {
                candidate
                    .info
                    .residents
                    .iter()
                    .any(|who| who == faction.name())
            },
        ),
    }
}

/// The preview card for `trip`; `go` travels there.
fn preview_card(trip: Trip, ctx: DispatchContext<'_>, seat: &mut SeatTravel) -> Dispatched {
    let Trip::To(request) = trip else {
        seat.preview = Some(trip);
        return Dispatched::default()
            .line("? back to where you were")
            .line("  go to travel there");
    };
    match ctx.world.resolve_travel(ctx.player, &request) {
        Some((found, _)) => {
            seat.preview = Some(trip);
            let mut done = Dispatched::default()
                .line(format!("? {}", found.name))
                .line(format!("  {}", card_line(&found)));
            if let Some(boss) = &found.info.boss {
                done = done.line(format!("  boss: {boss}"));
            }
            done.line(format!("  lands: {}", lands(&found)))
                .line("  go to travel there")
        }
        None => Dispatched::default().line(no_answer(request.kind)),
    }
}

/// One line of a destination's card: where, how deep, which Descent or theme.
fn card_line(found: &Destination) -> String {
    let mut parts = vec![if found.info.place.is_empty() {
        found.id.kind.name().to_owned()
    } else {
        found.info.place.clone()
    }];
    if let Some(metres) = found.info.depth_m {
        parts.push(format!("{metres} m down"));
    }
    if let Some(stratum) = found.info.stratum {
        parts.push(stratum.name().to_owned());
    } else if let Some(descent) = found.info.descent {
        parts.push(format!("Descent {}", descent.numeral()));
    }
    if let Some(theme) = found.info.theme {
        parts.push(theme.name().to_owned());
    }
    parts.join(", ")
}

/// Where a destination lands, as words.
fn lands(found: &Destination) -> String {
    let id = found.id;
    match id.place {
        Place::Space => format!("free flight at ({}, {})", id.x, id.y),
        Place::OnBody(BodyId::Home) => format!("a fresh pocket at ({}, {})", id.x, id.y),
        Place::OnBody(BodyId::Moon) => format!("the Moon at ({}, {})", id.x, id.y),
        Place::OnBody(BodyId::Space(_)) => {
            if id.x == fatherload_simulation::destination::space::ARRIVAL_POINT {
                format!("the arrival point of {}", found.info.place)
            } else {
                format!("the surface of {} at column {}", found.info.place, id.x)
            }
        }
    }
}

fn no_answer(kind: DestinationKind) -> String {
    if kind.in_space() {
        format!("no {} near here (is space open?)", kind.name())
    } else {
        format!("no {} to land at in this world", kind.name())
    }
}

/// The travel action for `trip`: the world resolves it again and moves the rig (from
/// anywhere: the index crosses places itself).
fn travel(trip: Trip, ctx: DispatchContext<'_>) -> Dispatched {
    let action = PlayerCommand::TestMode(trip.action());
    match trip {
        Trip::Back => Dispatched::default().act(action).line("-> back"),
        Trip::To(request) => match ctx.world.resolve_travel(ctx.player, &request) {
            Some((found, _)) => Dispatched::default().act(action).line(format!(
                "-> {} ({})",
                found.name,
                card_line(&found)
            )),
            None => Dispatched::default().line(no_answer(request.kind)),
        },
    }
}
