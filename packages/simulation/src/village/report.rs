//! A readable report of a world's villages (`fatherload --village-report`): identity, state,
//! street layout, counters and their deals, residents, neighbours and the stories they grow.

use std::fmt::Write as _;

use super::{
    SignTarget, Village, VillageSetting, placement, village, village_facts, village_identity,
};
use crate::worldgen::WorldSettings;

/// The villages of the first sites either side of town (surface and underground, nearest
/// first), at most `count`.
#[must_use]
pub fn sample(settings: &WorldSettings, count: usize) -> Vec<std::sync::Arc<Village>> {
    let mut found = Vec::new();
    for site in (1..=placement::MAX_SITE).flat_map(|site| [site, -site]) {
        for id in [placement::surface_id(site), placement::underground_id(site)] {
            if found.len() >= count {
                return found;
            }
            found.extend(village(settings, id));
        }
    }
    found
}

/// The report of the villages [`sample`] picks for `settings` and `count`.
#[must_use]
pub fn village_report(settings: &WorldSettings, count: usize) -> String {
    let mut out = String::new();
    let villages = sample(settings, count);
    let _ = writeln!(
        out,
        "Villages of world {} ({} sampled)\n",
        settings.seed_text(),
        villages.len()
    );
    for found in villages {
        write_village(&mut out, settings, &found);
    }
    out
}

#[allow(clippy::too_many_lines, reason = "one report section after another")]
fn write_village(out: &mut String, settings: &WorldSettings, found: &Village) {
    let setting = match found.setting {
        VillageSetting::Surface => format!("surface, {} landscape", found.landscape.name()),
        VillageSetting::Underground => format!("underground, floor at row {}", found.ground_row),
        VillageSetting::Sky(kind) => format!(
            "{} in the clouds, ground at row {}",
            kind.name(),
            found.ground_row
        ),
    };
    let _ = writeln!(out, "== {} ==", found.describe());
    let _ = writeln!(
        out,
        "   {} culture; {setting}; columns {}..={}; prosperity {}/5; trouble: {}",
        found.culture.name(),
        found.first_column,
        found.last_column,
        found.prosperity,
        found
            .problem
            .map_or_else(|| "none".to_owned(), |problem| format!("{problem:?}")),
    );
    for (neighbour, relation) in &found.relations {
        if let Some((name, culture)) = village_identity(settings, *neighbour) {
            let _ = writeln!(
                out,
                "   neighbour {name} ({}): {relation:?}",
                culture.name()
            );
        }
    }
    for post in &found.signposts {
        let target = match post.target {
            SignTarget::Town => "town".to_owned(),
            SignTarget::Village(id) => {
                village_identity(settings, id).map_or_else(|| "?".to_owned(), |(name, _)| name)
            }
        };
        let way = if post.direction < 0 { "west" } else { "east" };
        let _ = writeln!(out, "   signpost: {target} {} m {way}", post.distance);
    }
    let _ = writeln!(out, "   street:");
    for (index, building) in found.buildings.iter().enumerate() {
        let keeper = building
            .keeper
            .and_then(|who| found.population.characters.get(usize::from(who)))
            .map_or_else(String::new, |who| format!(", kept by {}", who.name.short()));
        let residents: Vec<&str> = building
            .residents
            .iter()
            .filter_map(|who| found.population.characters.get(usize::from(*who)))
            .map(|who| who.name.short())
            .collect();
        let home = if residents.is_empty() {
            String::new()
        } else {
            format!(", home of {}", residents.join(", "))
        };
        let _ = writeln!(
            out,
            "     {:>6} {:<12}{keeper}{home}",
            building.first_column,
            building.kind.label(),
        );
        for offer in found.offers(index) {
            let price = if offer.price > 0 {
                format!(" ({} cr)", offer.price)
            } else {
                String::new()
            };
            let _ = writeln!(out, "              - {}{price}", offer.label());
        }
    }
    let _ = writeln!(out, "   folk:");
    for character in &found.population.characters {
        let _ = writeln!(
            out,
            "     {} ({}, {}), {} tone, wants: {:?}, offers {}",
            character.name.full(),
            character.role.name(),
            character.age,
            character.tone.name(),
            character.want,
            character.offer.name(),
        );
    }
    let facts = village_facts(settings, found);
    let seeds = found.story_seeds(&facts, &[], 0);
    let _ = writeln!(
        out,
        "   {} places nearby to talk about; stories:",
        facts.len()
    );
    for seed in seeds.iter().take(3) {
        let Some(giver) = found.population.get(seed.giver) else {
            continue;
        };
        let _ = writeln!(
            out,
            "     [{}] {}: {}",
            seed.template().title,
            giver.name.short(),
            seed.pitch(giver, &found.name, "Driller")
        );
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::GeneratorVersion;

    #[test]
    fn the_report_names_every_sampled_village() {
        let settings = WorldSettings {
            generator: GeneratorVersion::V6,
            ..WorldSettings::new(42)
        };
        let text = village_report(&settings, 5);
        for found in sample(&settings, 5) {
            assert!(text.contains(&found.name), "{}", found.name);
        }
        assert!(text.contains("street:"));
    }
}
