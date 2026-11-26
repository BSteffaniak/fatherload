//! Names the palette understands: every registry's entries as typed words (lower case, `_` for
//! spaces, no leading "the"), and the matching rules shared by the parser and completion.
//!
//! Lists walk the simulation's registries (`ALL` lists), so anything added later is typed and
//! completed with no change here.

use fatherload_simulation::{
    EnemyDifficulty, EnemyKind,
    descent::{Descent, DescentBoss, Stratum},
    explosive::ExplosiveKind,
    item::ItemKind,
    space::Theme,
    test_mode::palette_stock::WeatherSwitch,
    weapon::{AmmoKind, WeaponKind},
    workshop::DeepSupply,
    worldgen::deposits::MINERALS,
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

/// Every creature and boss by name.
#[must_use]
pub fn creatures() -> Vec<(EnemyKind, String)> {
    named(EnemyKind::ALL, |kind| word(kind.name()))
}

/// What `give <item> N` can hand out: an item for the cargo bay or a deep supply for the kit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Giveable {
    Item(ItemKind),
    Supply(DeepSupply),
}

/// Every item `give` knows: explosives, ammunition, weapons, minerals and the deep supplies.
#[must_use]
pub fn giveables() -> Vec<(Giveable, String)> {
    let mut out: Vec<(Giveable, String)> = Vec::new();
    for kind in ExplosiveKind::ALL {
        if let Some(item) = kind.item() {
            out.push((Giveable::Item(item), word(kind.name())));
        }
    }
    // The plan's "give dynamite 99": the mining charge is the dig's stick of dynamite.
    if let Some(item) = ExplosiveKind::Mining.item() {
        out.push((Giveable::Item(item), "dynamite".to_owned()));
    }
    for kind in AmmoKind::ALL {
        out.push((Giveable::Item(kind.item()), word(kind.name())));
    }
    for kind in WeaponKind::ALL {
        out.push((Giveable::Item(kind.item()), word(kind.name())));
    }
    for mineral in MINERALS {
        out.push((
            Giveable::Item(ItemKind::Mineral(mineral)),
            word(mineral.name()),
        ));
    }
    for supply in DeepSupply::ALL {
        out.push((Giveable::Supply(supply), word(supply.name())));
    }
    out
}

/// Weather switches.
#[must_use]
pub fn weathers() -> Vec<(WeatherSwitch, String)> {
    named(WeatherSwitch::ALL, |weather| weather.word().to_owned())
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
