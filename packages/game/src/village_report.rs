//! `fatherload --village-report [--seed S] [--count N]` prints a world's villages nearest town
//! first (no window opens): identity, state, street layout, every counter's deals, residents,
//! neighbours, signposts and the stories each village grows
//! (`fatherload_simulation::village::report`).

use fatherload_simulation::{
    village::report,
    worldgen::{GeneratorVersion, WorldSettings, parse_seed},
};

/// A parsed `--village-report` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VillageReportOptions {
    pub seed: u64,
    pub count: usize,
}

/// Parses the village report flags. `Ok(None)` when `--village-report` is absent.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<VillageReportOptions>, String> {
    if !args.iter().any(|arg| arg == "--village-report") {
        return Ok(None);
    }
    let mut options = VillageReportOptions { seed: 1, count: 8 };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--village-report" => {}
            "--seed" => {
                let value = iter.next().ok_or("--seed needs a value")?;
                options.seed = parse_seed(value).ok_or("invalid --seed")?;
            }
            "--count" => {
                options.count = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|count| (1..=200).contains(count))
                    .ok_or("--count needs a number from 1 to 200")?;
            }
            other => return Err(format!("unknown village report argument {other}")),
        }
    }
    Ok(Some(options))
}

/// Prints the report to `out`.
///
/// # Errors
///
/// Returns a message if writing fails.
pub fn run(options: &VillageReportOptions, out: &mut impl std::io::Write) -> Result<(), String> {
    let settings = WorldSettings {
        generator: GeneratorVersion::CURRENT,
        ..WorldSettings::new(options.seed)
    };
    let text = report::village_report(&settings, options.count);
    write!(out, "{text}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{VillageReportOptions, parse_args, run};

    #[test]
    fn parses_flags_and_prints_villages() {
        let args: Vec<String> = [
            "fatherload",
            "--village-report",
            "--seed",
            "7",
            "--count",
            "2",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        let options = parse_args(&args).expect("parses").expect("present");
        assert_eq!(options, VillageReportOptions { seed: 7, count: 2 });
        let mut out = Vec::new();
        run(&options, &mut out).expect("runs");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("street:"), "{text}");
        assert!(
            parse_args(&["fatherload".to_owned()])
                .expect("ok")
                .is_none()
        );
    }
}
