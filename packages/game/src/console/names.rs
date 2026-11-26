//! Names the palette understands: every registry's entries as typed words (lower case, `_` for
//! spaces, no leading "the"), and the matching rules shared by the parser and completion.
//!
//! Lists walk the simulation's registries (`ALL` lists), so anything added later is typed and
//! completed with no change here.

use fatherload_simulation::{
    EnemyDifficulty, EnemyKind,
    bodylife::weather::{WeatherId, weather_kinds},
    descent::{Descent, DescentBoss, Stratum},
    destination::DestinationKind,
    endgame::hunt::THEME_BOSSES,
    explosive::ExplosiveKind,
    hostiles::Faction,
    space::Theme,
    test_mode::StockItem,
};

use crate::debug_views::{DebugView, TimeScale};

/// `text` as a palette word: lower case, runs of anything but letters and digits as one `_`,
/// without a leading "the".
#[must_use]
pub fn word(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '.' {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(character.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    out.strip_prefix("the_")
        .map_or_else(|| out.clone(), str::to_owned)
}

/// How `typed` matches `name` (both palette words): exactly, as a prefix, or as the prefix of
/// one of its `_`-separated parts (`colossus` in `gatekeeper_colossus`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Fit {
    Exact,
    Prefix,
    Part,
}

fn fit(typed: &str, name: &str) -> Option<Fit> {
    if typed == name {
        Some(Fit::Exact)
    } else if name.starts_with(typed) {
        Some(Fit::Prefix)
    } else if name.split('_').any(|part| part.starts_with(typed)) {
        Some(Fit::Part)
    } else {
        None
    }
}

/// The one entry of `entries` that `typed` names: an exact name wins, then a single prefix
/// match, then a single part match. Errors say what was meant or what is close.
///
/// # Errors
///
/// When nothing (or more than one thing) matches.
pub fn pick<T: Copy>(typed: &str, kind: &str, entries: &[(T, String)]) -> Result<T, String> {
    let typed = word(typed);
    if typed.is_empty() {
        return Err(format!("which {kind}?"));
    }
    for wanted in [Fit::Exact, Fit::Prefix, Fit::Part] {
        let found: Vec<&(T, String)> = entries
            .iter()
            .filter(|(_, name)| fit(&typed, name) == Some(wanted))
            .collect();
        match found.as_slice() {
            [] => {}
            [(value, _)] => return Ok(*value),
            many if wanted == Fit::Exact => return Ok(many[0].0),
            many => {
                let names: Vec<&str> = many.iter().take(6).map(|(_, name)| name.as_str()).collect();
                return Err(format!(
                    "\"{typed}\" could be any of {}{}",
                    names.join(", "),
                    if many.len() > 6 { ", ..." } else { "" }
                ));
            }
        }
    }
    Err(format!("no {kind} called \"{typed}\""))
}

/// The entries' names that `typed` could complete to (exact and prefix first, then parts).
#[must_use]
pub fn completions<T>(typed: &str, entries: &[(T, String)]) -> Vec<String> {
    let typed = word(typed);
    let mut found: Vec<(Fit, &String)> = entries
        .iter()
        .filter_map(|(_, name)| fit(&typed, name).map(|fit| (fit, name)))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    let mut out: Vec<String> = found.into_iter().map(|(_, name)| name.clone()).collect();
    out.dedup();
    out
}

fn named<T: Copy>(
    items: impl IntoIterator<Item = T>,
    name: impl Fn(T) -> String,
) -> Vec<(T, String)> {
    items.into_iter().map(|item| (item, name(item))).collect()
}

/// The planet themes (§16), by their names (`ice`, `gas_giant`, `ocean_archipelago`).
#[must_use]
pub fn themes() -> Vec<(Theme, String)> {
    named(Theme::PLANET, |theme| word(theme.name()))
}

/// The Descents by number (1-8), numeral and name.
#[must_use]
pub fn descents() -> Vec<(Descent, String)> {
    let mut out = Vec::new();
    for (index, descent) in Descent::ALL.into_iter().enumerate() {
        out.push((descent, (index + 1).to_string()));
        out.push((descent, descent.numeral().to_ascii_lowercase()));
        out.push((descent, word(descent.name())));
    }
    out
}

/// Every stratum by name.
#[must_use]
pub fn strata() -> Vec<(Stratum, String)> {
    named(Stratum::ALL, |stratum| word(stratum.name()))
}

/// Every gate boss by name.
#[must_use]
pub fn bosses() -> Vec<(DescentBoss, String)> {
    named(DescentBoss::ALL, |boss| word(boss.name()))
}

/// A boss `tp boss` can go to: a Descent's gate boss, a planet theme's boss (its index in the
/// hunt list's `THEME_BOSSES`) or a space faction's flagship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BossName {
    Gate(DescentBoss),
    Theme(usize),
    Flagship(Faction),
}

/// Every boss `tp boss` knows: the gate bosses, the twelve theme bosses and the flagships
/// (`pirates`, `swarm`, `machine` and their `_flagship` names).
#[must_use]
pub fn any_bosses() -> Vec<(BossName, String)> {
    let mut out: Vec<(BossName, String)> = bosses()
        .into_iter()
        .map(|(boss, name)| (BossName::Gate(boss), name))
        .collect();
    for (index, boss) in THEME_BOSSES.iter().enumerate() {
        out.push((BossName::Theme(index), word(boss.name)));
    }
    for faction in Faction::ALL {
        out.push((
            BossName::Flagship(faction),
            word(&format!("{} flagship", faction.name())),
        ));
        // The factions' plain names too (`tp boss pirates`).
        let plain = match faction {
            Faction::Pirates => "pirates",
            Faction::Swarm => "swarm",
            Faction::Machine => "machines",
        };
        out.push((BossName::Flagship(faction), plain.to_owned()));
    }
    out
}

/// The boss `typed` names: a gate boss when it names one (`colossus` is the Gatekeeper
/// Colossus, as the plan's `--start-at "boss:colossus"` asks), else any boss.
///
/// # Errors
///
/// When nothing (or more than one boss) matches.
pub fn pick_boss(typed: &str) -> Result<BossName, String> {
    pick(typed, "gate boss", &bosses())
        .map(BossName::Gate)
        .or_else(|_| pick(typed, "boss", &any_bosses()))
}

/// Every creature and boss by name.
#[must_use]
pub fn creatures() -> Vec<(EnemyKind, String)> {
    named(EnemyKind::ALL, |kind| word(kind.name()))
}

/// What `give <item> N` can hand out: every stocked consumable of the god-mode toolkit by its
/// slug (`StockItem::all`, walking the registries), and `dynamite` (the dig's stick of
/// dynamite is the mining charge).
#[must_use]
pub fn giveables() -> Vec<(StockItem, String)> {
    let mut out: Vec<(StockItem, String)> = StockItem::all()
        .into_iter()
        .map(|item| (item, item.slug()))
        .collect();
    out.push((
        StockItem::Explosive(ExplosiveKind::Mining),
        "dynamite".to_owned(),
    ));
    out
}

/// The kinds of place `tp <kind>` names by the destination index's own words (the kinds the
/// palette has no richer grammar for).
#[must_use]
pub fn place_kinds() -> Vec<(DestinationKind, String)> {
    DestinationKind::ALL
        .into_iter()
        .filter(|kind| {
            !matches!(
                kind,
                DestinationKind::Planet
                    | DestinationKind::Descent
                    | DestinationKind::Depth
                    | DestinationKind::Stratum
                    | DestinationKind::Player
                    | DestinationKind::Bookmark
                    | DestinationKind::Recent
                    | DestinationKind::HomeTown
                    | DestinationKind::Moon
            )
        })
        .map(|kind| (kind, word(kind.name())))
        .collect()
}

/// Weathers: `auto` (each world's own forecast) and every registered weather kind.
#[must_use]
pub fn weathers() -> Vec<(Option<WeatherId>, String)> {
    let mut out = vec![(None, "auto".to_owned())];
    out.extend(weather_kinds().map(|kind| (Some(kind.id), word(kind.name))));
    out
}

/// Enemy settings.
#[must_use]
pub fn difficulties() -> Vec<(EnemyDifficulty, String)> {
    named(EnemyDifficulty::ALL, |difficulty| word(difficulty.name()))
}

/// Debug views.
#[must_use]
pub fn views() -> Vec<(DebugView, String)> {
    named(DebugView::ALL, |view| view.word().to_owned())
}

/// Time scales (`pause`, `0.5`, `1`, `2`, `4`, and `half`/`normal`/`double`/`quad`).
#[must_use]
pub fn speeds() -> Vec<(TimeScale, String)> {
    let mut out = named(TimeScale::ALL, |scale| scale.word().to_owned());
    for (scale, alias) in [
        (TimeScale::Half, "half"),
        (TimeScale::Normal, "normal"),
        (TimeScale::Double, "double"),
        (TimeScale::Quadruple, "quad"),
    ] {
        out.push((scale, alias.to_owned()));
    }
    out
}
