//! The Atlas's type-ahead search: "planet ice", "leviathan", "descent 5".
//!
//! A query is split into words; a destination matches when every word starts one of the words
//! of its search text (its name, subtitle, kind, category, card and synonyms such as "descent 5"
//! or "d5"). Matches are ranked: a name that starts with the query first, then names holding
//! every word, then the rest, each in the index's own order.

use super::index::Destination;

/// The words of `text`, lower-cased, split on anything that is not a letter or digit.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// How well `destination` answers `query` (`None`: not at all; lower is better).
#[must_use]
pub fn score(destination: &Destination, query: &str) -> Option<u8> {
    let terms: Vec<String> = words(query).collect();
    if terms.is_empty() {
        return Some(3);
    }
    let mut haystack: Vec<String> = words(&destination.keywords).collect();
    for fact in &destination.facts {
        haystack.extend(words(&fact.value));
    }
    if let Some(boss) = destination.boss {
        haystack.extend(words(boss.name()));
    }
    let found = |term: &String, within: &[String]| within.iter().any(|word| word.starts_with(term));
    if !terms.iter().all(|term| found(term, &haystack)) {
        return None;
    }
    let name = destination.name.to_lowercase();
    let name_words: Vec<String> = words(&name).collect();
    let joined = terms.join(" ");
    if name.starts_with(&joined) || name.trim_start_matches("the ").starts_with(&joined) {
        Some(0)
    } else if terms.iter().all(|term| found(term, &name_words)) {
        Some(1)
    } else {
        Some(2)
    }
}

/// `destinations` (the destination index's search results) ranked for `query`: names that
/// start with it first, then names holding every word, then the rest, each in the index's own
/// order (nearest first within a kind).
pub fn rank(destinations: &mut [Destination], query: &str) {
    destinations.sort_by_key(|destination| score(destination, query).unwrap_or(3));
}

/// The indices of `destinations` that match `query`, best first (stable within a rank).
#[cfg(test)]
#[must_use]
pub fn filter(destinations: &[Destination], query: &str) -> Vec<usize> {
    let mut ranked: Vec<(u8, usize)> = destinations
        .iter()
        .enumerate()
        .filter_map(|(index, destination)| Some((score(destination, query)?, index)))
        .collect();
    ranked.sort_unstable();
    ranked.into_iter().map(|(_, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::index::{AtlasCategory, AtlasTarget, DestinationKind};
    use fatherload_simulation::destination::DestinationKind as IndexKind;
    use fatherload_simulation::terrain::TilePosition;

    fn destination(name: &str, keywords: &str) -> Destination {
        Destination {
            kind: DestinationKind::Index(IndexKind::Stratum),
            category: AtlasCategory::Depth,
            name: name.to_owned(),
            subtitle: String::new(),
            target: AtlasTarget::Home(TilePosition { x: 0, y: 0 }),
            facts: Vec::new(),
            boss: None,
            creatures: Vec::new(),
            keywords: format!("{name} {keywords}").to_lowercase(),
            request: None,
        }
    }

    #[test]
    fn every_word_must_start_a_word_of_the_text() {
        let list = [
            destination("Descent V \u{b7} The Drowned Deep", "descent 5 d5 v"),
            destination("Descent IV \u{b7} The Living Dark", "descent 4 d4 iv"),
            destination("Frost IV", "planet world ice moon"),
            destination("Ember II", "planet world volcanic moon"),
            destination("The Leviathan", "gate boss descent 5"),
        ];
        assert_eq!(filter(&list, "descent 5"), vec![0, 4]);
        assert_eq!(filter(&list, "planet ice"), vec![2]);
        assert_eq!(filter(&list, "leviathan"), vec![4]);
        assert_eq!(filter(&list, "d4"), vec![1]);
        // "5" does not match "50": words must start with the term, not hold it.
        assert_eq!(filter(&list, "50"), [] as [usize; 0]);
        // An empty query keeps everything, in order.
        assert_eq!(filter(&list, "  "), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn names_that_start_with_the_query_come_first() {
        let list = [
            destination("Comet of Ivar", "comet ice tail"),
            destination("Ice world", "planet ice"),
            destination("Icebound Hall", "village"),
        ];
        // "ice": the two names starting with it lead (index order between them), the comet
        // (ice only in its text) last.
        assert_eq!(filter(&list, "ice"), vec![1, 2, 0]);
    }
}
