//! The palette's command language: one line in, one [`Command`] out (or a message saying
//! what was wrong). `--start-at` targets parse through the same travel grammar
//! ([`parse_target`]), with `:` between words.

use fatherload_simulation::{
    EnemyDifficulty, EnemyKind,
    descent::{Descent, DescentBoss, Stratum},
    space::Theme,
    test_mode::palette_stock::WeatherSwitch,
};

use super::names::{self, Giveable};
use crate::debug_views::{DebugView, TimeScale};

/// Which candidate of a kind (`tp planet next`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Nearest,
    Next,
    Previous,
    Random,
}

/// Where `tp` goes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TpTarget {
    Planet {
        step: Step,
        theme: Option<Theme>,
    },
    Descent(Descent),
    /// Rows (metres) below the ground.
    Depth(i32),
    Stratum(Stratum),
    Boss(DescentBoss),
    Station(Step),
    /// A local seat's or crewmate's number as the roster shows it (1-based).
    Player(u8),
    Bookmark(String),
    Back,
    Home,
    Moon,
}

impl TpTarget {
    /// The target as the palette's words (`descent v`, `planet random ice`; `--start-at`'s
    /// note, desktop only).
    #[cfg(not(target_os = "emscripten"))]
    #[must_use]
    pub fn words(&self) -> String {
        let step = |step: &Step| match step {
            Step::Nearest => "nearest",
            Step::Next => "next",
            Step::Previous => "prev",
            Step::Random => "random",
        };
        match self {
            Self::Planet { step: how, theme } => format!(
                "planet {}{}",
                step(how),
                theme.map_or_else(String::new, |theme| format!(
                    " {}",
                    names::word(theme.name())
                ))
            ),
            Self::Descent(descent) => format!("descent {}", descent.numeral().to_ascii_lowercase()),
            Self::Depth(rows) => format!("depth {rows}m"),
            Self::Stratum(stratum) => format!("stratum {}", names::word(stratum.name())),
            Self::Boss(boss) => format!("boss {}", names::word(boss.name())),
            Self::Station(how) => format!("station {}", step(how)),
            Self::Player(number) => format!("player {number}"),
            Self::Bookmark(name) => format!("bookmark {name}"),
            Self::Back => "back".to_owned(),
            Self::Home => "home".to_owned(),
            Self::Moon => "moon".to_owned(),
        }
    }
}

/// `give` what.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Give {
    All,
    One { what: Giveable, count: u32 },
}

/// One palette command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Travel (`preview`: `tp … ?`, show where first; `go` then travels).
    Tp {
        target: TpTarget,
        preview: bool,
    },
    /// Travel to the destination last previewed.
    Go,
    /// Save the rig's spot under a name (`tp bookmark <name>` goes back to it).
    Bookmark(String),
    Give(Give),
    /// `gate all` (`None`) or one gate by its boss.
    Gate(Option<DescentBoss>),
    EndingDone,
    Spawn(EnemyKind),
    Clear,
    /// `freeze` toggles; `freeze on|off` sets.
    Freeze(Option<bool>),
    /// Minute of the day.
    Time(u16),
    Weather(WeatherSwitch),
    /// `difficulty` alone reports it.
    Difficulty(Option<EnemyDifficulty>),
    /// A debug view (`view <name> [on|off]`; toggles without on/off).
    View(DebugView, Option<bool>),
    /// Every debug view off.
    ViewsOff,
    /// The local time scale.
    Speed(TimeScale),
    /// Help on everything or one command.
    Help(Option<String>),
    /// Act for another split-screen seat (1-based, as the seats are numbered on screen).
    Seat(u8),
}

/// Every command word, in help order.
pub const COMMANDS: [&str; 18] = [
    "tp",
    "go",
    "bookmark",
    "give",
    "gate",
    "ending",
    "spawn",
    "clear",
    "freeze",
    "time",
    "weather",
    "difficulty",
    "view",
    "speed",
    "pause",
    "help",
    "close",
    "seat",
];

/// One line of help per command.
#[must_use]
pub fn help_line(command: &str) -> Option<&'static str> {
    Some(match command {
        "tp" => {
            "tp planet next|prev|nearest|random [theme] | descent N | depth 4200m | stratum <name> | boss <name> | station nearest | player N | bookmark <name> | home | moon | back  (end with ? to preview)"
        }
        "go" => "go: travel to the destination last previewed with tp … ?",
        "bookmark" => "bookmark <name>: remember where the rig is (tp bookmark <name>)",
        "give" => "give all | give <item> N",
        "gate" => "gate all | gate <boss>: clear Descent gates",
        "ending" => "ending done: the Heart woken, space open",
        "spawn" => "spawn <creature>: a creature or boss beside the rig",
        "clear" => "clear: every hostile near the rig gone",
        "freeze" => "freeze [on|off]: creature AI frozen",
        "time" => "time HH:MM: set the clock",
        "weather" => "weather auto|clear|storm",
        "difficulty" => "difficulty [peaceful|normal|hard]",
        "view" => "view hitboxes|fields|grid|reveal|freecam|fps [on|off] | view off",
        "speed" => "speed pause|0.5|1|2|4: the local time scale",
        "pause" => "pause: the local time scale stops (speed 1 resumes)",
        "help" => "help [command]",
        "close" => "close: put the palette away (or ` / Esc)",
        "seat" => "seat N: act for split-screen seat N (travel, give, spawn for that rig)",
        _ => return None,
    })
}

/// Splits a line into lower-case words.
#[must_use]
pub fn words(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Parses one palette line. `None` for an empty line.
///
/// # Errors
///
/// A message naming what was wrong (unknown command, missing or unknown argument).
pub fn parse(line: &str) -> Result<Option<Command>, String> {
    let mut words = words(line);
    let Some(first) = words.first().cloned() else {
        return Ok(None);
    };
    let args: Vec<String> = words.split_off(1);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match first.as_str() {
        "tp" | "travel" | "goto" => parse_tp(&args)?,
        "go" => Command::Go,
        "bookmark" | "mark" => {
            let name = args.join("_");
            if name.is_empty() {
                return Err("bookmark <name>".to_owned());
            }
            Command::Bookmark(name)
        }
        "give" => parse_give(&args)?,
        "gate" => match args.first() {
            None => return Err("gate all | gate <boss>".to_owned()),
            Some(&"all") => Command::Gate(None),
            Some(_) => Command::Gate(Some(names::pick(
                &args.join("_"),
                "gate boss",
                &names::bosses(),
            )?)),
        },
        "ending" => match args.first() {
            Some(&"done") | None => Command::EndingDone,
            Some(other) => return Err(format!("ending done (not \"{other}\")")),
        },
        "spawn" => {
            if args.is_empty() {
                return Err("spawn <creature>".to_owned());
            }
            Command::Spawn(names::pick(
                &args.join("_"),
                "creature",
                &names::creatures(),
            )?)
        }
        "clear" => Command::Clear,
        "freeze" => Command::Freeze(on_off(args.first().copied())?),
        "time" => Command::Time(parse_time(args.first().copied().unwrap_or(""))?),
        "weather" => Command::Weather(names::pick(
            args.first().copied().unwrap_or(""),
            "weather",
            &names::weathers(),
        )?),
        "difficulty" => Command::Difficulty(match args.first() {
            None => None,
            Some(typed) => Some(names::pick(typed, "difficulty", &names::difficulties())?),
        }),
        "view" | "debug" => match args.first() {
            None => return Err("view hitboxes|fields|grid|reveal|freecam|fps [on|off]".to_owned()),
            Some(&"off" | &"none") => Command::ViewsOff,
            Some(typed) => Command::View(
                names::pick(typed, "debug view", &names::views())?,
                on_off(args.get(1).copied())?,
            ),
        },
        "speed" | "timescale" => Command::Speed(names::pick(
            args.first().copied().unwrap_or(""),
            "speed",
            &names::speeds(),
        )?),
        "pause" => Command::Speed(TimeScale::Paused),
        "help" | "?" => Command::Help(args.first().map(|arg| (*arg).to_owned())),
        "seat" => Command::Seat(
            args.first()
                .and_then(|word| word.parse().ok())
                .filter(|number| *number >= 1)
                .ok_or_else(|| "seat N (1, 2, ...)".to_owned())?,
        ),
        other => return Err(format!("unknown command \"{other}\" (try help)")),
    };
    Ok(Some(command))
}

fn on_off(word: Option<&str>) -> Result<Option<bool>, String> {
    match word {
        None => Ok(None),
        Some("on" | "1" | "true" | "yes") => Ok(Some(true)),
        Some("off" | "0" | "false" | "no") => Ok(Some(false)),
        Some(other) => Err(format!("on or off (not \"{other}\")")),
    }
}

/// `HH:MM`, `HH` or `HHMM` as a minute of the day.
///
/// # Errors
///
/// When it is not a time of day.
pub fn parse_time(text: &str) -> Result<u16, String> {
    let bad = || format!("time HH:MM (not \"{text}\")");
    let (hours, minutes) = match text.split_once(':') {
        Some((hours, minutes)) => (hours, minutes),
        None if text.len() == 4 => text.split_at(2),
        None => (text, "0"),
    };
    let hours: u16 = hours.parse().map_err(|_| bad())?;
    let minutes: u16 = minutes.parse().map_err(|_| bad())?;
    if hours > 24 || minutes > 59 || (hours == 24 && minutes > 0) {
        return Err(bad());
    }
    Ok((hours % 24) * 60 + minutes)
}

/// A depth: `4200m`, `4200`, `4200rows` or `4.2km`, as rows (1 row is 1 m).
///
/// # Errors
///
/// When it is not a positive depth.
pub fn parse_depth(text: &str) -> Result<i32, String> {
    let bad = || format!("depth 4200m (not \"{text}\")");
    let text = text.trim();
    let metres = text
        .strip_suffix("rows")
        .or_else(|| text.strip_suffix("row"))
        .or_else(|| text.strip_suffix('m'))
        .unwrap_or(text);
    let (number, scale) = text
        .strip_suffix("km")
        .map_or((metres, 1.0), |km| (km, 1_000.0));
    let value: f64 = number.trim().parse().map_err(|_| bad())?;
    let rows = (value * scale).round();
    if !(1.0..=100_000.0).contains(&rows) {
        return Err(bad());
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "checked to a small range above"
    )]
    Ok(rows as i32)
}

const STEPS: [(Step, &str); 7] = [
    (Step::Nearest, "nearest"),
    (Step::Next, "next"),
    (Step::Previous, "prev"),
    (Step::Previous, "previous"),
    (Step::Random, "random"),
    (Step::Nearest, "near"),
    (Step::Random, "any"),
];

fn step_of(word: &str) -> Option<Step> {
    STEPS
        .iter()
        .find(|(_, name)| *name == word)
        .map(|(step, _)| *step)
}

fn parse_tp(args: &[&str]) -> Result<Command, String> {
    let (args, preview) = match args.split_last() {
        Some((&"?", rest)) => (rest, true),
        Some((last, rest)) if last.len() > 1 && last.ends_with('?') => {
            // `tp descent 5?`
            let mut owned: Vec<&str> = rest.to_vec();
            owned.push(&last[..last.len() - 1]);
            return Ok(Command::Tp {
                target: parse_target(&owned)?,
                preview: true,
            });
        }
        _ => (args, false),
    };
    Ok(Command::Tp {
        target: parse_target(args)?,
        preview,
    })
}

/// Parses a travel target from its words (`["planet", "random", "ice"]`): the palette's `tp`
/// and `--start-at "planet:random:ice"` (split on `:`).
///
/// # Errors
///
/// A message naming what was wrong.
pub fn parse_target(args: &[&str]) -> Result<TpTarget, String> {
    let Some((kind, rest)) = args.split_first() else {
        return Err(help_line("tp").unwrap_or_default().to_owned());
    };
    let joined = rest.join("_");
    Ok(match *kind {
        "planet" | "world" => {
            let (step, theme_words) = match rest.split_first() {
                Some((first, more)) => {
                    step_of(first).map_or((Step::Nearest, rest), |step| (step, more))
                }
                None => (Step::Nearest, rest),
            };
            let theme = if theme_words.is_empty() {
                None
            } else {
                Some(names::pick(
                    &theme_words.join("_"),
                    "planet theme",
                    &names::themes(),
                )?)
            };
            TpTarget::Planet { step, theme }
        }
        "descent" => TpTarget::Descent(names::pick(&joined, "Descent", &names::descents())?),
        "depth" => TpTarget::Depth(parse_depth(&rest.concat())?),
        "stratum" | "layer" => {
            TpTarget::Stratum(names::pick(&joined, "stratum", &names::strata())?)
        }
        "boss" | "gate" => TpTarget::Boss(names::pick(&joined, "gate boss", &names::bosses())?),
        "station" => TpTarget::Station(
            rest.first()
                .map_or(Some(Step::Nearest), |word| step_of(word))
                .ok_or_else(|| "station nearest|next|prev|random".to_owned())?,
        ),
        "player" => {
            let number: u8 = rest
                .first()
                .and_then(|word| word.parse().ok())
                .filter(|number| *number >= 1)
                .ok_or_else(|| "player N (1, 2, ...)".to_owned())?;
            TpTarget::Player(number)
        }
        "bookmark" | "mark" => {
            if joined.is_empty() {
                return Err("tp bookmark <name>".to_owned());
            }
            TpTarget::Bookmark(joined)
        }
        "back" => TpTarget::Back,
        "home" | "spawn" | "surface" => TpTarget::Home,
        "moon" => TpTarget::Moon,
        other => {
            // `tp 4200m` and `tp leviathan` read as what they can only mean.
            if let Ok(rows) = parse_depth(other) {
                TpTarget::Depth(rows)
            } else if let Ok(boss) = names::pick(other, "gate boss", &names::bosses()) {
                TpTarget::Boss(boss)
            } else {
                return Err(format!(
                    "tp where? \"{other}\" is not planet, descent, depth, stratum, boss, station, player, bookmark, home, moon or back"
                ));
            }
        }
    })
}

/// Parses a `--start-at` target: travel words joined by `:` (`descent:5`, `planet:random:ice`,
/// `boss:colossus`, `station:nearest`, `depth:4200m`).
///
/// # Errors
///
/// A message naming what was wrong.
#[cfg(not(target_os = "emscripten"))]
pub fn parse_start_at(text: &str) -> Result<TpTarget, String> {
    let lowered = text.trim().to_ascii_lowercase();
    let words: Vec<&str> = lowered
        .split([':', ' '])
        .filter(|word| !word.is_empty())
        .collect();
    let target = parse_target(&words)?;
    if matches!(target, TpTarget::Back | TpTarget::Bookmark(_)) {
        return Err(format!(
            "--start-at cannot go to \"{text}\" (no earlier spot)"
        ));
    }
    Ok(target)
}

fn parse_give(args: &[&str]) -> Result<Command, String> {
    match args {
        [] => Err("give all | give <item> N".to_owned()),
        ["all" | "everything"] => Ok(Command::Give(Give::All)),
        [.., last] => {
            let (name, count) = last
                .parse::<u32>()
                .map_or((args, 1), |count| (&args[..args.len() - 1], count));
            if name.is_empty() {
                return Err("give <item> N".to_owned());
            }
            let what = names::pick(&name.join("_"), "item", &names::giveables())?;
            Ok(Command::Give(Give::One {
                what,
                count: count.clamp(1, 9_999),
            }))
        }
    }
}
