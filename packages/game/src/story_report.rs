//! `fatherload --story-report [--seed S] [--count N] [--no-world]` prints sampled villages:
//! each resident's profile, dialogue across several visits (and after the player does
//! something memorable for them), then the story seeds their village grows, with pitches,
//! beats, twists and endings. Rumours and story places come from the seed's generated world
//! unless `--no-world` asks for stand-in places (faster).

use fatherload_simulation::{
    people::{WorldFact, report},
    terrain::Terrain,
    worldgen::{WorldSettings, parse_seed},
};

/// A parsed `--story-report` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoryReportOptions {
    pub seed: u64,
    pub count: usize,
    pub world: bool,
}

/// Parses the story report flags. `Ok(None)` when `--story-report` is absent.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<StoryReportOptions>, String> {
    if !args.iter().any(|arg| arg == "--story-report") {
        return Ok(None);
    }
    let mut options = StoryReportOptions {
        seed: 1,
        count: 12,
        world: true,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--story-report" => {}
            "--seed" => {
                let value = iter.next().ok_or("--seed needs a value")?;
                options.seed = parse_seed(value).ok_or("invalid --seed")?;
            }
            "--count" => {
                options.count = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|count| (1..=500).contains(count))
                    .ok_or("--count needs a number from 1 to 500")?;
            }
            "--no-world" => options.world = false,
            other => return Err(format!("unknown story report argument {other}")),
        }
    }
    Ok(Some(options))
}

/// Prints the report to `out`.
///
/// # Errors
///
/// Returns a message if writing fails.
pub fn run(options: &StoryReportOptions, out: &mut impl std::io::Write) -> Result<(), String> {
    let facts = if options.world {
        let terrain = Terrain::generate(&WorldSettings::new(options.seed));
        WorldFact::from_features(terrain.features().all())
    } else {
        report::sample_facts()
    };
    let text = report::story_report(options.seed, options.count, &facts);
    write!(out, "{text}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{StoryReportOptions, parse_args};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_flags() {
        assert_eq!(parse_args(&args("--seeds 3")), Ok(None));
        assert_eq!(
            parse_args(&args("--story-report --seed 7 --count 30 --no-world")),
            Ok(Some(StoryReportOptions {
                seed: 7,
                count: 30,
                world: false
            }))
        );
        assert!(parse_args(&args("--story-report --count 0")).is_err());
        assert!(parse_args(&args("--story-report --bogus")).is_err());
    }
}
