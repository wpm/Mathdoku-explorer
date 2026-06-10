//! Run-directory writers and the human summary table (ADR-0007 §Output).
//!
//! Theory: a run's output directory is its complete provenance record.
//! `config.yaml` holds the *resolved* configuration — with the
//! actually-used master seed — so the run's workload (conditions, trial
//! indices, derived seeds, generated instances) can be reproduced exactly;
//! a re-run is identical apart from the measured duration columns;
//! `metadata.yaml` pins the environment (versions, commit, profile,
//! platform) that makes timings comparable or not; `raw.csv` keeps every
//! measured trial so future analyses never need a re-run; `summary.csv`
//! holds the per-condition × per-phase descriptive statistics of
//! [`crate::stats`]. The human-readable table on stdout is a courtesy view
//! of `summary.csv`, durations scaled to a readable unit per row.
//!
//! Timestamps are UTC, formatted `YYYYMMDDThhmmssZ`, computed from
//! [`std::time::SystemTime`] with Howard Hinnant's `civil_from_days`
//! algorithm (the inverse of days-from-civil,
//! <https://howardhinnant.github.io/date_algorithms.html>) rather than a
//! date crate: the epoch count is split into 400-year eras (146 097 days),
//! the year and day-of-year are recovered from the day-of-era by undoing
//! the leap-day arithmetic, and months are counted from March so the leap
//! day lands at the end of the shifted year.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::config::ExperimentConfig;
use crate::error::Error;
use crate::runner::RunData;
use crate::stats::Summary;

/// The current UTC time as `YYYYMMDDThhmmssZ`. A system clock before the
/// epoch (misconfigured hardware) is clamped to the epoch rather than
/// failing: a wrong-but-valid run name beats refusing to record results.
#[must_use]
pub fn now_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_secs());
    format_utc_timestamp(seconds)
}

/// Formats seconds since the Unix epoch as `YYYYMMDDThhmmssZ`.
#[must_use]
pub fn format_utc_timestamp(unix_seconds: u64) -> String {
    const SECONDS_PER_DAY: u64 = 86_400;
    let (year, month, day) = civil_from_days(unix_seconds / SECONDS_PER_DAY);
    let second_of_day = unix_seconds % SECONDS_PER_DAY;
    let hour = second_of_day / 3_600;
    let minute = second_of_day % 3_600 / 60;
    let second = second_of_day % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Converts days since the Unix epoch to a civil `(year, month, day)`
/// (proleptic Gregorian, UTC). Howard Hinnant's `civil_from_days`,
/// restricted to `days >= 0` so unsigned arithmetic suffices: shift the
/// epoch to 0000-03-01 (719 468 days earlier) so years start in March and
/// the leap day is the last day of the year, split into 400-year eras of
/// 146 097 days, recover the year-of-era by discounting the era's leap
/// days, and map the day-of-year to month and day with the 153-day
/// five-month cycle (Mar–Jul, Aug–Dec each 31+30+31+30+31 days).
const fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_shifted + 2) / 5 + 1;
    let month = if month_shifted < 10 {
        month_shifted + 3
    } else {
        month_shifted - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

/// Writes a complete run directory under the configuration's
/// `output_directory` and returns its path.
///
/// The directory is named `<experiment>-<timestamp>`; if that name is
/// taken (two runs in the same second), `-2`, `-3`, ... suffixes are
/// tried, claiming the directory atomically via `create_dir`.
///
/// # Errors
///
/// Returns [`Error::OutputDir`] when the directory cannot be created and
/// [`Error::OutputWrite`]/[`Error::Csv`] when a file cannot be written.
pub fn write_run(
    config: &ExperimentConfig,
    data: &RunData,
    timestamp: &str,
) -> Result<PathBuf, Error> {
    let run_directory =
        create_run_directory(&config.output_directory, &config.experiment, timestamp)?;
    write_config(&run_directory, config)?;
    write_metadata(&run_directory, timestamp)?;
    write_raw_csv(&run_directory, config, data)?;
    write_summary_csv(&run_directory, data)?;
    Ok(run_directory)
}

/// Creates and returns the run directory, retrying with numeric suffixes
/// on a name collision.
fn create_run_directory(
    parent: &Path,
    experiment: &str,
    timestamp: &str,
) -> Result<PathBuf, Error> {
    let to_error = |source: io::Error| Error::OutputDir {
        path: parent.to_path_buf(),
        source,
    };
    fs::create_dir_all(parent).map_err(to_error)?;
    let base = format!("{experiment}-{timestamp}");
    for attempt in 1..=u32::MAX {
        let name = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let candidate = parent.join(name);
        // `create_dir` (not `create_dir_all`) claims the name atomically.
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(to_error(source)),
        }
    }
    // Unreachable short of 2^32 same-second runs; report the collision.
    Err(to_error(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "every candidate run directory name is taken",
    )))
}

/// Writes the resolved configuration (seed filled in) as `config.yaml`.
fn write_config(run_directory: &Path, config: &ExperimentConfig) -> Result<(), Error> {
    let yaml = serde_yaml_ng::to_string(config).map_err(|source| Error::OutputSerialize {
        file: "config.yaml",
        source,
    })?;
    write_text(&run_directory.join("config.yaml"), &yaml)
}

/// Environment provenance recorded alongside every run.
#[derive(Debug, Serialize)]
struct Metadata {
    /// This crate's version.
    package_version: &'static str,
    /// `git rev-parse HEAD` at run time, or `unknown`.
    git_commit: String,
    /// `rustc --version` at run time, or `unknown`. Best effort: the
    /// `rustc` on `PATH` at run time is almost always the compiling one
    /// here, but nothing guarantees it.
    rustc_version: String,
    /// `debug` or `release`, from `debug_assertions`.
    profile: &'static str,
    /// An approximation of the target triple composed from
    /// [`std::env::consts`]: `ARCH-FAMILY-OS` (e.g. `x86_64-unix-linux`).
    /// The real vendor and ABI components are not exposed by the standard
    /// library at run time; the family stands in for the vendor slot.
    target_triple: String,
    /// The operating system the binary runs on.
    os: &'static str,
    /// The run's UTC start time, identical to the directory timestamp.
    start_time: String,
}

/// Writes environment provenance as `metadata.yaml`.
fn write_metadata(run_directory: &Path, timestamp: &str) -> Result<(), Error> {
    let metadata = Metadata {
        package_version: env!("CARGO_PKG_VERSION"),
        git_commit: command_output("git", &["rev-parse", "HEAD"]),
        rustc_version: command_output("rustc", &["--version"]),
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        target_triple: format!(
            "{}-{}-{}",
            std::env::consts::ARCH,
            std::env::consts::FAMILY,
            std::env::consts::OS
        ),
        os: std::env::consts::OS,
        start_time: timestamp.to_owned(),
    };
    let yaml = serde_yaml_ng::to_string(&metadata).map_err(|source| Error::OutputSerialize {
        file: "metadata.yaml",
        source,
    })?;
    write_text(&run_directory.join("metadata.yaml"), &yaml)
}

/// Runs `program` with `args` and returns its trimmed stdout, or
/// `unknown` when the program is missing, fails, or prints non-UTF-8.
fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |stdout| stdout.trim().to_owned())
}

/// Writes `contents` to `path`, mapping failure to [`Error::OutputWrite`]
/// so the report names the file that could not be written.
fn write_text(path: &Path, contents: &str) -> Result<(), Error> {
    fs::write(path, contents).map_err(|source| Error::OutputWrite {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes `raw.csv`: one row per measured trial with the trial's identity,
/// derived seed, and one integer-nanosecond column per phase.
fn write_raw_csv(
    run_directory: &Path,
    config: &ExperimentConfig,
    data: &RunData,
) -> Result<(), Error> {
    let mut writer = csv::Writer::from_path(run_directory.join("raw.csv"))?;
    let mut header = vec![
        "experiment".to_owned(),
        "condition".to_owned(),
        "trial".to_owned(),
        "seed".to_owned(),
    ];
    header.extend(data.phases.iter().map(|phase| format!("{phase}_ns")));
    writer.write_record(&header)?;
    for trial in &data.trials {
        let mut record = vec![
            config.experiment.clone(),
            data.conditions[trial.condition_index].label.clone(),
            trial.index.to_string(),
            trial.seed.to_string(),
        ];
        record.extend(
            trial
                .durations
                .iter()
                .map(|&duration| saturating_nanos(duration).to_string()),
        );
        writer.write_record(&record)?;
    }
    writer.flush()?;
    Ok(())
}

/// Writes `summary.csv`: one row of [`Summary`] statistics per condition ×
/// phase, in nanoseconds.
fn write_summary_csv(run_directory: &Path, data: &RunData) -> Result<(), Error> {
    let mut writer = csv::Writer::from_path(run_directory.join("summary.csv"))?;
    writer.write_record([
        "condition",
        "phase",
        "count",
        "mean_ns",
        "std_dev_ns",
        "min_ns",
        "p25_ns",
        "median_ns",
        "p75_ns",
        "max_ns",
        "ci95_low_ns",
        "ci95_high_ns",
    ])?;
    for (condition, phase, summary) in condition_phase_summaries(data)? {
        writer.write_record([
            condition,
            phase.to_owned(),
            summary.count.to_string(),
            summary.mean.to_string(),
            summary.std_dev.to_string(),
            summary.min.to_string(),
            summary.p25.to_string(),
            summary.median.to_string(),
            summary.p75.to_string(),
            summary.max.to_string(),
            summary.ci95_low.to_string(),
            summary.ci95_high.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

/// Renders the human-readable per-condition × per-phase summary table:
/// count, mean ± half-CI, and median, scaled per row to a readable unit.
///
/// # Errors
///
/// Returns [`Error::Stats`] when a condition × phase cell cannot be
/// summarised (fewer than two samples).
pub fn render_summary_table(data: &RunData) -> Result<String, Error> {
    const HEADER: [&str; 5] = ["condition", "phase", "count", "mean ± ci95", "median"];
    /// Right-align all but the two label columns.
    const RIGHT_ALIGNED: [bool; 5] = [false, false, true, true, true];
    let rows: Vec<[String; 5]> = condition_phase_summaries(data)?
        .into_iter()
        .map(|(condition, phase, summary)| {
            let (unit, divisor) = readable_unit(summary.mean);
            let half_width = (summary.ci95_high - summary.ci95_low) / 2.0;
            [
                condition,
                phase.to_owned(),
                summary.count.to_string(),
                format!(
                    "{:.2} ± {:.2} {unit}",
                    summary.mean / divisor,
                    half_width / divisor
                ),
                format!("{:.2} {unit}", summary.median / divisor),
            ]
        })
        .collect();
    let mut widths = HEADER.map(str::len);
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut table = String::new();
    push_row(
        &mut table,
        &widths,
        RIGHT_ALIGNED,
        &HEADER.map(str::to_owned),
    );
    for row in rows {
        push_row(&mut table, &widths, RIGHT_ALIGNED, &row);
    }
    Ok(table)
}

/// Appends one table row, columns separated by two spaces, padded by
/// character count (the unit column contains the two-byte `µ`).
fn push_row(
    table: &mut String,
    widths: &[usize; 5],
    right_aligned: [bool; 5],
    cells: &[String; 5],
) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            table.push_str("  ");
        }
        let padding = widths[index].saturating_sub(cell.chars().count());
        if right_aligned[index] {
            table.extend(std::iter::repeat_n(' ', padding));
        }
        table.push_str(cell);
        if !right_aligned[index] && index < cells.len() - 1 {
            table.extend(std::iter::repeat_n(' ', padding));
        }
    }
    table.push('\n');
}

/// Picks a display unit for a row from its mean in nanoseconds.
fn readable_unit(mean_ns: f64) -> (&'static str, f64) {
    if mean_ns < 1e3 {
        ("ns", 1.0)
    } else if mean_ns < 1e6 {
        ("µs", 1e3)
    } else if mean_ns < 1e9 {
        ("ms", 1e6)
    } else {
        ("s", 1e9)
    }
}

/// Summarises every condition × phase cell, in condition-major order.
fn condition_phase_summaries(
    data: &RunData,
) -> Result<Vec<(String, &'static str, Summary)>, Error> {
    let mut summaries = Vec::with_capacity(data.conditions.len() * data.phases.len());
    for (condition_index, condition) in data.conditions.iter().enumerate() {
        for (phase_index, &phase) in data.phases.iter().enumerate() {
            let samples: Vec<f64> = data
                .trials
                .iter()
                .filter(|trial| trial.condition_index == condition_index)
                .map(|trial| nanos_as_f64(trial.durations[phase_index]))
                .collect();
            summaries.push((
                condition.label.clone(),
                phase,
                Summary::from_samples(&samples)?,
            ));
        }
    }
    Ok(summaries)
}

/// A duration as integer nanoseconds, saturating at `u64::MAX` (about 584
/// years — no phase of any credible trial overflows it).
fn saturating_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// A duration as `f64` nanoseconds for the statistics.
// Exact below 2^53 ns ≈ 104 days; beyond that the relative error is
// negligible against scheduler noise.
#[allow(clippy::cast_precision_loss)]
fn nanos_as_f64(duration: Duration) -> f64 {
    saturating_nanos(duration) as f64
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::ExperimentConfig;
    use crate::experiments::Condition;
    use crate::runner::{RunData, Trial};
    use crate::stats::Summary;
    use crate::testutil::unique_temp_dir;

    use super::{format_utc_timestamp, now_timestamp, render_summary_table, write_run};

    // Reference instants from Python's
    // `datetime.fromtimestamp(s, timezone.utc)`. The set exercises the
    // epoch itself, a day boundary, the controversial century leap day
    // (2000-02-29 exists; 2100-02-29 does not), and the 32-bit rollover.
    #[test]
    fn utc_timestamps_match_reference_instants() {
        assert_eq!(format_utc_timestamp(0), "19700101T000000Z");
        assert_eq!(format_utc_timestamp(86_399), "19700101T235959Z");
        assert_eq!(format_utc_timestamp(86_400), "19700102T000000Z");
        assert_eq!(format_utc_timestamp(951_782_400), "20000229T000000Z");
        assert_eq!(format_utc_timestamp(951_868_800), "20000301T000000Z");
        assert_eq!(format_utc_timestamp(1_234_567_890), "20090213T233130Z");
        assert_eq!(format_utc_timestamp(2_147_483_647), "20380119T031407Z");
        assert_eq!(format_utc_timestamp(4_102_444_800), "21000101T000000Z");
        assert_eq!(format_utc_timestamp(4_107_456_000), "21000228T000000Z");
        assert_eq!(format_utc_timestamp(4_107_542_400), "21000301T000000Z");
        assert_eq!(format_utc_timestamp(1_767_225_600), "20260101T000000Z");
    }

    #[test]
    fn now_timestamp_has_the_directory_name_shape() {
        let stamp = now_timestamp();
        assert_eq!(stamp.len(), 16);
        assert_eq!(&stamp[8..9], "T");
        assert!(stamp.ends_with('Z'));
        assert!(
            stamp[..8].chars().all(|c| c.is_ascii_digit())
                && stamp[9..15].chars().all(|c| c.is_ascii_digit()),
            "unexpected timestamp {stamp}"
        );
    }

    /// A two-condition, three-phase fixture with hand-picked durations:
    /// condition-major, two measured trials per condition.
    fn fixture() -> (ExperimentConfig, RunData) {
        let config = ExperimentConfig::from_reader(
            "experiment: fake\nseed: 7\nwarmup: 1\nsamples: 2\n".as_bytes(),
        )
        .expect("the fixture configuration should parse");
        let trial = |condition_index, index, seed, nanos: [u64; 3]| Trial {
            condition_index,
            index,
            seed,
            durations: nanos.map(Duration::from_nanos).to_vec(),
        };
        let data = RunData {
            conditions: vec![Condition::new("n=3"), Condition::new("n=4")],
            phases: vec!["alpha", "beta", "total"],
            trials: vec![
                trial(0, 1, 101, [1_000, 2_000, 3_000]),
                trial(0, 2, 102, [1_500, 2_500, 4_000]),
                trial(1, 1, 201, [2_000_000, 3_000_000, 5_000_000]),
                trial(1, 2, 202, [4_000_000, 5_000_000, 9_000_000]),
            ],
        };
        (config, data)
    }

    #[test]
    fn write_run_creates_the_named_directory_and_every_file() {
        let parent = unique_temp_dir("output-files");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let run_directory =
            write_run(&config, &data, "20260609T231500Z").expect("write_run should succeed");
        assert_eq!(run_directory, parent.join("fake-20260609T231500Z"));
        for file in ["config.yaml", "metadata.yaml", "raw.csv", "summary.csv"] {
            assert!(run_directory.join(file).is_file(), "{file} should exist");
        }
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
    }

    #[test]
    fn same_second_runs_get_numeric_suffixes() {
        let parent = unique_temp_dir("output-collision");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let first = write_run(&config, &data, "20260609T231500Z").expect("first run");
        let second = write_run(&config, &data, "20260609T231500Z").expect("second run");
        let third = write_run(&config, &data, "20260609T231500Z").expect("third run");
        assert_eq!(first, parent.join("fake-20260609T231500Z"));
        assert_eq!(second, parent.join("fake-20260609T231500Z-2"));
        assert_eq!(third, parent.join("fake-20260609T231500Z-3"));
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
    }

    #[test]
    fn raw_csv_is_one_integer_nanosecond_row_per_measured_trial() {
        let parent = unique_temp_dir("output-raw");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let run_directory = write_run(&config, &data, "20260609T231500Z").expect("write_run");
        let raw = std::fs::read_to_string(run_directory.join("raw.csv")).expect("raw.csv");
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
        assert_eq!(
            raw,
            "experiment,condition,trial,seed,alpha_ns,beta_ns,total_ns\n\
             fake,n=3,1,101,1000,2000,3000\n\
             fake,n=3,2,102,1500,2500,4000\n\
             fake,n=4,1,201,2000000,3000000,5000000\n\
             fake,n=4,2,202,4000000,5000000,9000000\n"
        );
    }

    #[test]
    fn summary_csv_agrees_with_the_stats_module() {
        let parent = unique_temp_dir("output-summary");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let run_directory = write_run(&config, &data, "20260609T231500Z").expect("write_run");
        let summary_csv =
            std::fs::read_to_string(run_directory.join("summary.csv")).expect("summary.csv");
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
        let mut lines = summary_csv.lines();
        assert_eq!(
            lines.next(),
            Some(
                "condition,phase,count,mean_ns,std_dev_ns,min_ns,p25_ns,median_ns,p75_ns,\
                 max_ns,ci95_low_ns,ci95_high_ns"
            )
        );
        // One row per condition x phase, in condition-major order, each
        // matching `Summary::from_samples` over that cell's nanoseconds.
        let cells: [(&str, &str, [f64; 2]); 6] = [
            ("n=3", "alpha", [1_000.0, 1_500.0]),
            ("n=3", "beta", [2_000.0, 2_500.0]),
            ("n=3", "total", [3_000.0, 4_000.0]),
            ("n=4", "alpha", [2_000_000.0, 4_000_000.0]),
            ("n=4", "beta", [3_000_000.0, 5_000_000.0]),
            ("n=4", "total", [5_000_000.0, 9_000_000.0]),
        ];
        for (condition, phase, samples) in cells {
            let expected = Summary::from_samples(&samples).expect("the cell should summarise");
            let line = lines.next().expect("a summary row should exist");
            let expected_line = format!(
                "{condition},{phase},{},{},{},{},{},{},{},{},{},{}",
                expected.count,
                expected.mean,
                expected.std_dev,
                expected.min,
                expected.p25,
                expected.median,
                expected.p75,
                expected.max,
                expected.ci95_low,
                expected.ci95_high
            );
            assert_eq!(line, expected_line);
        }
        assert_eq!(lines.next(), None, "no extra rows");
    }

    #[test]
    fn recorded_config_round_trips_with_the_resolved_seed() {
        let parent = unique_temp_dir("output-config");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let run_directory = write_run(&config, &data, "20260609T231500Z").expect("write_run");
        let written = std::fs::read_to_string(run_directory.join("config.yaml"))
            .expect("config.yaml should exist");
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
        let reparsed = ExperimentConfig::from_reader(written.as_bytes())
            .expect("the recorded configuration should parse and validate");
        assert_eq!(reparsed, config);
        assert_eq!(reparsed.seed, Some(7));
    }

    #[test]
    fn metadata_parses_with_the_expected_keys() {
        let parent = unique_temp_dir("output-metadata");
        let (mut config, data) = fixture();
        config.output_directory.clone_from(&parent);
        let run_directory = write_run(&config, &data, "20260609T231500Z").expect("write_run");
        let metadata = std::fs::read_to_string(run_directory.join("metadata.yaml"))
            .expect("metadata.yaml should exist");
        std::fs::remove_dir_all(&parent).expect("the temp directory should be removable");
        let parsed: std::collections::BTreeMap<String, String> =
            serde_yaml_ng::from_str(&metadata).expect("metadata.yaml should parse");
        let keys: Vec<&str> = parsed.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "git_commit",
                "os",
                "package_version",
                "profile",
                "rustc_version",
                "start_time",
                "target_triple"
            ]
        );
        assert_eq!(parsed["package_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            parsed["profile"], "debug",
            "tests compile without optimisation"
        );
        assert_eq!(parsed["start_time"], "20260609T231500Z");
        assert!(!parsed["git_commit"].is_empty());
        assert!(!parsed["rustc_version"].is_empty());
    }

    #[test]
    fn summary_table_is_aligned_and_scales_units_per_row() {
        let (_, data) = fixture();
        let table = render_summary_table(&data).expect("the table should render");
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 1 + 6, "a header plus one row per cell");
        assert!(lines[0].starts_with("condition  phase  count"));
        assert!(lines[0].contains("mean ± ci95"));
        // Microsecond-range cells scale to µs, millisecond-range to ms.
        assert!(lines[1].contains("µs"), "got: {}", lines[1]);
        assert!(lines[4].contains("ms"), "got: {}", lines[4]);
        // Fixed width: the last column is right-aligned, so every row is
        // padded to the same character width.
        let width = lines[0].chars().count();
        for line in &lines {
            assert_eq!(
                line.chars().count(),
                width,
                "row not padded to the table width: {line:?}"
            );
        }
        assert!(table.ends_with('\n'));
    }
}
