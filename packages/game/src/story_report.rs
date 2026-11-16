//! `fatherload --story-report [--seed S] [--count N] [--no-world]` prints sampled villages:
//! each resident's profile, dialogue across several visits (and after the player does
//! something memorable for them), then the story seeds their village grows, with pitches,
//! beats, twists and endings. Rumours and story places come from the seed's generated world
//! unless `--no-world` asks for stand-in places (faster).
//!
//! `fatherload --story-report --arcs [--seed S] [--count N] [--generator V5..V8]` instead walks
//! the world's villages and prints full generated story arcs, bound to the real world exactly
//! as play binds them: who offers it and why, the pitch, every beat's journal text, nudge and
//! contact line, the climax, and every ending's reply and epilogue (WP-STORY).

use fatherload_simulation::{
    people::{WorldFact, report},
    terrain::Terrain,
    worldgen::{GeneratorVersion, SPAWN_TILE_X, WorldSettings, parse_seed},
};

/// Columns either side of town whose features feed the report's world facts.
const STORY_REPORT_HALF_WIDTH: i32 = 180;

/// A parsed `--story-report` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoryReportOptions {
    pub seed: u64,
    pub count: usize,
    pub world: bool,
    /// Print full bound arcs instead of villages (`--arcs`).
    pub arcs: bool,
    /// The generator for `--arcs` (default: the current one).
    pub generator: Option<GeneratorVersion>,
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
        arcs: false,
        generator: None,
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
            "--arcs" => options.arcs = true,
            "--generator" => {
                let value = iter.next().ok_or("--generator needs a value")?;
                options.generator = Some(match value.to_ascii_uppercase().as_str() {
                    "V5" => GeneratorVersion::V5,
                    "V6" => GeneratorVersion::V6,
                    "V7" => GeneratorVersion::V7,
                    "V8" => GeneratorVersion::V8,
                    _ => return Err("--generator takes V5, V6, V7 or V8".to_owned()),
                });
            }
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
    if options.arcs {
        let mut settings = WorldSettings::new(options.seed);
        if let Some(generator) = options.generator {
            settings.generator = generator;
        }
        let text = report::arc_report(&settings, options.count);
        return write!(out, "{text}").map_err(|error| error.to_string());
    }
    let facts = if options.world {
        let terrain = Terrain::generate(&WorldSettings::new(options.seed));
        // The classic world's span around town; endless worlds have no finite feature list.
        WorldFact::near(&terrain, SPAWN_TILE_X, STORY_REPORT_HALF_WIDTH)
    } else {
        report::sample_facts()
    };
    let text = report::story_report(options.seed, options.count, &facts);
    write!(out, "{text}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{GeneratorVersion, StoryReportOptions, parse_args};

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
                world: false,
                arcs: false,
                generator: None,
            }))
        );
        assert_eq!(
            parse_args(&args("--story-report --arcs --generator v6")),
            Ok(Some(StoryReportOptions {
                seed: 1,
                count: 12,
                world: true,
                arcs: true,
                generator: Some(GeneratorVersion::V6),
            }))
        );
        assert!(parse_args(&args("--story-report --count 0")).is_err());
        assert!(parse_args(&args("--story-report --bogus")).is_err());
    }
}
