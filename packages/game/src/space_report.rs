//! `fatherload --space-report [--seed S] [--sectors R]` prints a summary of the space around
//! home (no window opens): systems per star class, bodies per kind and theme, a radius
//! histogram, belts, comets, stations, derelicts, black holes, nebulae and wormhole pairs
//! (`fatherload_simulation::space::report`), then how long generation took.

use fatherload_simulation::{
    space::{generate_sector, report},
    worldgen::{DEFAULT_WORLD_SEED, parse_seed},
};

/// A parsed `--space-report` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceReportOptions {
    pub seed: u64,
    /// Sectors around home (Chebyshev radius).
    pub sectors: i32,
}

/// Parses the space report flags. `Ok(None)` when `--space-report` is absent.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<SpaceReportOptions>, String> {
    if !args.iter().any(|arg| arg == "--space-report") {
        return Ok(None);
    }
    let mut options = SpaceReportOptions {
        seed: DEFAULT_WORLD_SEED,
        sectors: 4,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--space-report" => {}
            "--seed" => {
                let value = iter.next().ok_or("--seed needs a value")?;
                options.seed = parse_seed(value).ok_or("invalid --seed")?;
            }
            "--sectors" => {
                options.sectors = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|sectors| (0..=64).contains(sectors))
                    .ok_or("--sectors needs a number from 0 to 64")?;
            }
            other => return Err(format!("unknown space report argument {other}")),
        }
    }
    Ok(Some(options))
}

/// Runs the report when `--space-report` is among `args` (exiting on a bad flag); `false`
/// when it is absent.
pub fn run_if_asked(args: &[String]) -> bool {
    match parse_args(args) {
        Ok(Some(options)) => {
            if let Err(error) = run(&options, &mut std::io::stdout()) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            true
        }
        Ok(None) => false,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

/// Prints the report to `out`.
///
/// # Errors
///
/// Returns a message if writing fails.
pub fn run(options: &SpaceReportOptions, out: &mut impl std::io::Write) -> Result<(), String> {
    let text = report::space_report(options.seed, options.sectors);
    write!(out, "{text}").map_err(|error| error.to_string())?;
    // Timing (not part of the deterministic report): every sector generated afresh.
    let sectors = report::sectors_around_home(options.sectors);
    let mut slowest = 0.0_f64;
    let start = std::time::Instant::now();
    for coords in &sectors {
        let one = std::time::Instant::now();
        let _ = generate_sector(
            fatherload_simulation::space::SpaceSeed::from(options.seed),
            *coords,
        );
        slowest = slowest.max(one.elapsed().as_secs_f64() * 1_000.0);
    }
    let total = start.elapsed().as_secs_f64() * 1_000.0;
    #[allow(clippy::cast_precision_loss, reason = "a few thousand sectors")]
    let average = total / sectors.len().max(1) as f64;
    writeln!(
        out,
        "\ngeneration: {} sectors in {total:.1} ms ({average:.3} ms average, {slowest:.3} ms slowest)",
        sectors.len()
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{SpaceReportOptions, parse_args, run};

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_flags_and_prints_the_neighbourhood() {
        let options = parse_args(&args("--space-report --seed 7 --sectors 1"))
            .expect("parses")
            .expect("present");
        assert_eq!(
            options,
            SpaceReportOptions {
                seed: 7,
                sectors: 1
            }
        );
        let mut out = Vec::new();
        run(&options, &mut out).expect("runs");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("wormhole pairs"), "{text}");
        assert!(text.contains("generation:"), "{text}");
        assert!(parse_args(&args("--village-report")).expect("ok").is_none());
        assert!(parse_args(&args("--space-report --sectors 999")).is_err());
        assert!(parse_args(&args("--space-report --bogus")).is_err());
    }
}
