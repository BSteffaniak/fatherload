//! `fatherload --balance-report [--seeds N] [--minutes M] [--hz H] [--only a,b]
//! [--strategies a,b]` runs the balance harness (`fatherload_simulation::balance`): bot players
//! on fresh worlds across seeds, duels, exposure runs, the building probe and the climb bot,
//! and prints the Markdown report. Use a release build; runs are spread over every CPU core.

use fatherload_simulation::balance::{BalanceConfig, Strategy, run_report, standard_probes};

/// A parsed `--balance-report` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceReportOptions {
    pub config: BalanceConfig,
}

/// Parses the balance report flags. `Ok(None)` when `--balance-report` is absent.
///
/// # Errors
///
/// Returns a message for a missing or malformed value.
pub fn parse_args(args: &[String]) -> Result<Option<BalanceReportOptions>, String> {
    if !args.iter().any(|arg| arg == "--balance-report") {
        return Ok(None);
    }
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut config = BalanceConfig {
        threads,
        ..BalanceConfig::default()
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--balance-report" => {}
            "--seeds" => {
                let count: usize = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|count| (1..=64).contains(count))
                    .ok_or("--seeds needs a number from 1 to 64")?;
                config.seeds = BalanceConfig::seeds(count);
            }
            "--minutes" => {
                config.minutes = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|minutes| (1..=600).contains(minutes))
                    .ok_or("--minutes needs a number from 1 to 600")?;
            }
            "--hz" => {
                config.tick_rate_hz = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|hz| (10..=240).contains(hz))
                    .ok_or("--hz needs a tick rate from 10 to 240")?;
            }
            "--only" => {
                let names: Vec<String> = iter
                    .next()
                    .ok_or("--only needs probe names")?
                    .split(',')
                    .map(str::to_owned)
                    .collect();
                let known: Vec<&str> = standard_probes().iter().map(|probe| probe.name()).collect();
                if let Some(unknown) = names.iter().find(|name| !known.contains(&name.as_str())) {
                    return Err(format!(
                        "unknown probe {unknown} (known: {})",
                        known.join(", ")
                    ));
                }
                config.only = Some(names);
            }
            "--strategies" => {
                let value = iter.next().ok_or("--strategies needs names")?;
                config.strategies = value
                    .split(',')
                    .map(|name| {
                        Strategy::ALL
                            .into_iter()
                            .find(|strategy| strategy.name() == name)
                            .ok_or_else(|| format!("unknown strategy {name}"))
                    })
                    .collect::<Result<_, _>>()?;
            }
            "--threads" => {
                config.threads = iter
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|threads| *threads >= 1)
                    .ok_or("--threads needs a number")?;
            }
            other => return Err(format!("unknown balance report argument {other}")),
        }
    }
    Ok(Some(BalanceReportOptions { config }))
}

/// Runs the harness and prints the report to `out`.
///
/// # Errors
///
/// Returns a message if writing fails.
pub fn run(options: &BalanceReportOptions, out: &mut impl std::io::Write) -> Result<(), String> {
    let report = run_report(&options.config);
    write!(out, "{}", report.to_markdown()).map_err(|error| error.to_string())
}

/// Runs the report if `args` ask for it (exiting the process on a bad flag or a failed
/// write); `false` when they do not.
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

#[cfg(test)]
mod tests {
    use super::parse_args;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_the_balance_report_flags() {
        assert_eq!(parse_args(&args("--story-report")), Ok(None));
        let options = parse_args(&args(
            "--balance-report --seeds 5 --minutes 90 --hz 30 --only economy,combat,climb --strategies shaft",
        ))
        .expect("valid")
        .expect("present");
        assert_eq!(options.config.seeds.len(), 5);
        assert_eq!(options.config.minutes, 90);
        assert_eq!(options.config.tick_rate_hz, 30);
        assert_eq!(
            options.config.only,
            Some(vec![
                "economy".to_owned(),
                "combat".to_owned(),
                "climb".to_owned()
            ])
        );
        assert_eq!(options.config.strategies.len(), 1);
        assert!(parse_args(&args("--balance-report --seeds 0")).is_err());
        assert!(parse_args(&args("--balance-report --only nonsense")).is_err());
        assert!(parse_args(&args("--balance-report --bogus")).is_err());
    }
}
