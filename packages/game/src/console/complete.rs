//! Tab completion: what the word under the cursor could be, from the same name lists the
//! parser reads (so anything the parser accepts can be completed).

use fatherload_simulation::test_mode::travel::DESTINATION_WORDS;

use super::{names, parse::COMMANDS};

/// The words that can follow `before` (the line's finished words).
fn next_words(before: &[&str], bookmarks: &[String]) -> Vec<String> {
    let own = |words: &[&str]| {
        words
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };
    match before {
        [] | ["help" | "?"] => own(&COMMANDS),
        ["tp" | "travel" | "goto"] => own(&DESTINATION_WORDS),
        ["tp", "planet"] => {
            let mut out = own(&["next", "prev", "nearest", "random"]);
            out.extend(names::themes().into_iter().map(|(_, name)| name));
            out
        }
        [
            "tp",
            "planet",
            "next" | "prev" | "previous" | "nearest" | "random",
        ] => names::themes().into_iter().map(|(_, name)| name).collect(),
        ["tp", "descent"] => names::descents()
            .into_iter()
            .map(|(_, name)| name)
            .collect(),
        ["tp", "depth"] => own(&[
            "500m", "1100m", "2700m", "4200m", "6000m", "9000m", "12000m",
        ]),
        ["tp", "stratum"] => names::strata().into_iter().map(|(_, name)| name).collect(),
        ["tp", "boss"] | ["gate"] => {
            let mut out = if before == ["gate"] {
                own(&["all"])
            } else {
                Vec::new()
            };
            out.extend(names::bosses().into_iter().map(|(_, name)| name));
            out
        }
        ["tp", "station"] => own(&["nearest", "next", "prev", "random"]),
        ["tp", "player"] => own(&["1", "2", "3", "4"]),
        ["tp", "bookmark"] => bookmarks.to_vec(),
        ["give"] => {
            let mut out = own(&["all"]);
            out.extend(names::giveables().into_iter().map(|(_, name)| name));
            out
        }
        ["ending"] => own(&["done"]),
        ["spawn"] => names::creatures()
            .into_iter()
            .map(|(_, name)| name)
            .collect(),
        ["freeze"] | ["view" | "debug", _] => own(&["on", "off"]),
        ["time"] => own(&["06:00", "12:00", "18:00", "22:00", "00:00"]),
        ["weather"] => names::weathers()
            .into_iter()
            .map(|(_, name)| name)
            .collect(),
        ["difficulty"] => names::difficulties()
            .into_iter()
            .map(|(_, name)| name)
            .collect(),
        ["view" | "debug"] => {
            let mut out: Vec<String> = names::views().into_iter().map(|(_, name)| name).collect();
            out.push("off".to_owned());
            out
        }
        ["speed" | "timescale"] => names::speeds().into_iter().map(|(_, name)| name).collect(),
        _ => Vec::new(),
    }
}

/// Every full line `line` could complete to at its last word (empty when nothing fits).
#[must_use]
pub fn complete(line: &str, bookmarks: &[String]) -> Vec<String> {
    let lowered = line.to_ascii_lowercase();
    let mut words: Vec<&str> = lowered.split_whitespace().collect();
    let current = if lowered.ends_with(' ') || words.is_empty() {
        ""
    } else {
        words.pop().unwrap_or("")
    };
    let candidates: Vec<((), String)> = next_words(&words, bookmarks)
        .into_iter()
        .map(|word| ((), word))
        .collect();
    let fitting = if current.is_empty() {
        let mut all: Vec<String> = candidates.into_iter().map(|((), word)| word).collect();
        all.dedup();
        all
    } else {
        names::completions(current, &candidates)
    };
    let prefix = if words.is_empty() {
        String::new()
    } else {
        format!("{} ", words.join(" "))
    };
    fitting
        .into_iter()
        .map(|word| format!("{prefix}{word}"))
        .collect()
}

/// The longest prefix every candidate shares.
#[must_use]
pub fn common_prefix(candidates: &[String]) -> String {
    let Some(first) = candidates.first() else {
        return String::new();
    };
    let mut end = first.len();
    for other in &candidates[1..] {
        end = end.min(
            first
                .bytes()
                .zip(other.bytes())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    first[..end].to_owned()
}
