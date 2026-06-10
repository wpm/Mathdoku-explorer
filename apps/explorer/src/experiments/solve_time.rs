//! The `solve-time` experiment: solving performance of the shipping
//! mathdoku code as a function of grid size `n` (ADR-0007).
//!
//! Theory: each trial generates a fresh random puzzle (untimed), then
//! measures two phases. *Propagate* rebuilds the puzzle from scratch — a
//! fresh [`Puzzle::new`] fed every cage through `insert_cage` — which
//! re-runs the constraint-propagation fixpoint in full. The rebuild is
//! essential: [`mathdoku::generate`] hands back an *already-narrowed*
//! puzzle, so timing its `solutions()` alone would measure search on a
//! pre-propagated instance and dramatically understate the cost of
//! solving. *Search* then times `solutions().next()` on the rebuilt
//! puzzle: the time to the first solution.
//!
//! The population being sampled is the *generator's*: every instance comes
//! from `generate(n, rng)`, so conclusions are about average-case
//! behaviour over generated puzzles — the population end users experience
//! — not worst-case complexity, and they depend on the generator's
//! cage-size distribution.

use std::time::Instant;

use mathdoku::{Cage, Puzzle};
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use super::{Condition, Experiment, Measurement, PreparedExperiment};
use crate::error::Error;

/// The registry name of this experiment.
const NAME: &str = "solve-time";

/// The measured phases, in declared (and reporting) order.
const PHASES: &[&str] = &["propagate", "search"];

/// The grid sizes the mathdoku library accepts.
const VALID_N: std::ops::RangeInclusive<usize> = 1..=9;

/// The registered identity of the `solve-time` experiment.
pub struct SolveTime;

impl Experiment for SolveTime {
    fn name(&self) -> &'static str {
        NAME
    }

    fn description(&self) -> &'static str {
        "time to propagate and solve a freshly generated random puzzle, by grid size n"
    }

    fn phases(&self) -> &'static [&'static str] {
        PHASES
    }

    fn prepare(
        &self,
        parameters: &serde_yaml_ng::Value,
    ) -> Result<Box<dyn PreparedExperiment>, Error> {
        Ok(Box::new(PreparedSolveTime::new(Parameters::parse(
            parameters,
        )?)))
    }
}

/// The `parameters` schema of the experiment configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    /// The grid sizes to measure, one condition each, in measurement order.
    n: Vec<usize>,
}

impl Parameters {
    /// Parses and validates the opaque `parameters` YAML value: a mapping
    /// with one required key `n`, a non-empty list of distinct integer
    /// grid sizes in `1..=9`.
    fn parse(parameters: &serde_yaml_ng::Value) -> Result<Self, Error> {
        let parsed: Self = serde_yaml_ng::from_value(parameters.clone())
            .map_err(|source| invalid_parameters(source.to_string()))?;
        if parsed.n.is_empty() {
            return Err(invalid_parameters(
                "`n` must list at least one grid size".to_owned(),
            ));
        }
        for (position, &n) in parsed.n.iter().enumerate() {
            if !VALID_N.contains(&n) {
                return Err(invalid_parameters(format!(
                    "`n` entries must be grid sizes in 1..=9, got {n}"
                )));
            }
            if parsed.n[..position].contains(&n) {
                return Err(invalid_parameters(format!(
                    "`n` lists the grid size {n} more than once"
                )));
            }
        }
        Ok(parsed)
    }
}

/// Builds the experiment's parameter-rejection error.
fn invalid_parameters(reason: String) -> Error {
    Error::InvalidParameters {
        experiment: NAME.to_owned(),
        reason,
    }
}

/// Builds the experiment's trial-failure error for grid size `n`.
fn trial_error(n: usize, reason: String) -> Error {
    Error::Trial {
        experiment: NAME.to_owned(),
        n,
        reason,
    }
}

/// The experiment with parsed parameters: one condition per grid size.
struct PreparedSolveTime {
    /// The grid size of each condition, parallel to `conditions`.
    sizes: Vec<usize>,
    /// The conditions, labelled `n=<size>`, in parameter order.
    conditions: Vec<Condition>,
}

impl PreparedSolveTime {
    /// Builds the condition list from validated parameters.
    fn new(parameters: Parameters) -> Self {
        let conditions = parameters
            .n
            .iter()
            .map(|n| Condition::new(format!("n={n}")))
            .collect();
        Self {
            sizes: parameters.n,
            conditions,
        }
    }
}

impl PreparedExperiment for PreparedSolveTime {
    fn conditions(&self) -> &[Condition] {
        &self.conditions
    }

    fn run_trial(
        &self,
        condition_index: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<Vec<Measurement>, Error> {
        let &n = self.sizes.get(condition_index).ok_or_else(|| {
            trial_error(
                0,
                format!(
                    "condition index {condition_index} is out of range ({} conditions)",
                    self.sizes.len()
                ),
            )
        })?;

        // Untimed setup: a fresh instance per trial, drawn from the
        // generator's population, so trials are i.i.d. samples.
        let generated = mathdoku::generate(n, rng)
            .map_err(|error| trial_error(n, format!("generate failed: {error}")))?;
        let cages: Vec<Cage> = generated.cages().cloned().collect();

        // Timed `propagate`: rebuild from scratch so the constraint
        // propagation fixpoint runs in full (the generated puzzle is
        // already narrowed; see the module documentation).
        let propagate_started = Instant::now();
        let mut rebuilt = Puzzle::new(n)
            .map_err(|error| trial_error(n, format!("Puzzle::new failed: {error}")))?;
        for cage in &cages {
            rebuilt = rebuilt
                .insert_cage(cage)
                .map_err(|error| trial_error(n, format!("insert_cage failed: {error}")))?
                .ok_or_else(|| {
                    trial_error(
                        n,
                        "library invariant violated: insert_cage found a cage of a \
                         generated puzzle infeasible"
                            .to_owned(),
                    )
                })?;
        }
        let propagate = propagate_started.elapsed();

        // Timed `search`: time to the first solution of the rebuilt puzzle.
        let search_started = Instant::now();
        let first_solution = rebuilt.solutions().next();
        let search = search_started.elapsed();
        match first_solution {
            Some(Ok(_)) => {}
            Some(Err(error)) => {
                return Err(trial_error(n, format!("solutions failed: {error}")));
            }
            None => {
                return Err(trial_error(
                    n,
                    "library invariant violated: a generated puzzle has no solution".to_owned(),
                ));
            }
        }

        Ok(PHASES
            .iter()
            .zip([propagate, search])
            .map(|(&phase, duration)| Measurement { phase, duration })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::Parser as _;
    use rand::SeedableRng as _;
    use rand_chacha::ChaCha8Rng;

    use super::{NAME, PHASES, SolveTime};
    use crate::cli::Cli;
    use crate::error::Error;
    use crate::experiments::{Experiment as _, PreparedExperiment};
    use crate::testutil::unique_temp_dir;

    /// Prepares the experiment from a YAML `parameters` snippet.
    fn prepare(parameters_yaml: &str) -> Result<Box<dyn PreparedExperiment>, Error> {
        let value: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(parameters_yaml).expect("the test YAML should parse");
        SolveTime.prepare(&value)
    }

    /// Asserts that `result` is an [`Error::InvalidParameters`] for this
    /// experiment and returns the reason.
    fn invalid_parameters_reason(result: Result<Box<dyn PreparedExperiment>, Error>) -> String {
        match result {
            Err(Error::InvalidParameters { experiment, reason }) => {
                assert_eq!(experiment, NAME);
                reason
            }
            Ok(_) => panic!("expected InvalidParameters, got a prepared experiment"),
            Err(other) => panic!("expected InvalidParameters, got {other:?}"),
        }
    }

    #[test]
    fn identity_matches_the_adr() {
        assert_eq!(SolveTime.name(), "solve-time");
        assert!(SolveTime.description().contains("grid size n"));
        assert_eq!(SolveTime.phases(), PHASES);
        assert_eq!(PHASES, &["propagate", "search"]);
    }

    #[test]
    fn valid_parameters_produce_one_labelled_condition_per_n_in_given_order() {
        let prepared = prepare("n: [5, 3, 4]").expect("valid parameters should prepare");
        let labels: Vec<&str> = prepared
            .conditions()
            .iter()
            .map(|condition| condition.label.as_str())
            .collect();
        assert_eq!(labels, vec!["n=5", "n=3", "n=4"]);
    }

    #[test]
    fn missing_n_is_rejected() {
        let reason = invalid_parameters_reason(prepare("{}"));
        assert!(reason.contains('n'), "the reason should name `n`: {reason}");
    }

    #[test]
    fn an_empty_n_list_is_rejected() {
        let reason = invalid_parameters_reason(prepare("n: []"));
        assert!(
            reason.contains("at least one"),
            "unexpected reason: {reason}"
        );
    }

    #[test]
    fn n_zero_is_rejected() {
        let reason = invalid_parameters_reason(prepare("n: [3, 0]"));
        assert!(reason.contains("got 0"), "unexpected reason: {reason}");
    }

    #[test]
    fn n_ten_is_rejected() {
        let reason = invalid_parameters_reason(prepare("n: [10]"));
        assert!(reason.contains("got 10"), "unexpected reason: {reason}");
    }

    #[test]
    fn a_duplicated_n_is_rejected() {
        let reason = invalid_parameters_reason(prepare("n: [3, 4, 3]"));
        assert!(
            reason.contains("more than once"),
            "unexpected reason: {reason}"
        );
    }

    #[test]
    fn a_non_integer_n_entry_is_rejected() {
        let _ = invalid_parameters_reason(prepare("n: [3, 4.5]"));
        let _ = invalid_parameters_reason(prepare("n: [3, four]"));
    }

    #[test]
    fn an_unknown_parameter_key_is_rejected() {
        let _ = invalid_parameters_reason(prepare("n: [3]\ngrid: 4\n"));
    }

    #[test]
    fn a_trial_reports_the_declared_phases_in_order_with_positive_durations() {
        let prepared = prepare("n: [3]").expect("valid parameters should prepare");
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let measurements = prepared
            .run_trial(0, &mut rng)
            .expect("the trial should run");
        let phases: Vec<&str> = measurements
            .iter()
            .map(|measurement| measurement.phase)
            .collect();
        assert_eq!(phases, PHASES);
        for measurement in &measurements {
            assert!(
                measurement.duration > Duration::ZERO,
                "phase {} should take measurable time",
                measurement.phase
            );
        }
    }

    #[test]
    fn the_trial_workload_is_exactly_one_seed_deterministic_generation() {
        // The trial draws randomness only to generate its instance, so a
        // reference generation from an identically seeded RNG must leave
        // both RNGs in the same state — proving the trial's instance is the
        // (deterministic) generated puzzle and nothing else consumed
        // randomness.
        let prepared = prepare("n: [4]").expect("valid parameters should prepare");
        let mut trial_rng = ChaCha8Rng::seed_from_u64(99);
        let mut reference_rng = trial_rng.clone();
        let _ = prepared
            .run_trial(0, &mut trial_rng)
            .expect("the trial should run");
        let _ = mathdoku::generate(4, &mut reference_rng).expect("generation should succeed");
        assert_eq!(trial_rng, reference_rng);

        // And generation itself is seed-deterministic: identical seeds
        // yield identical cage structures. Compare a stable projection
        // (polyomino, operator, target) per cage: the full Cage Debug
        // output includes propagation support structures whose hash-map
        // iteration order is not deterministic.
        let cage_structure = |puzzle: &mathdoku::Puzzle| -> Vec<String> {
            puzzle
                .cages()
                .map(|cage| format!("{:?} {:?}", cage.polyomino, cage.op_target()))
                .collect()
        };
        let mut first_rng = ChaCha8Rng::seed_from_u64(99);
        let mut second_rng = ChaCha8Rng::seed_from_u64(99);
        let first = mathdoku::generate(4, &mut first_rng).expect("generation should succeed");
        let second = mathdoku::generate(4, &mut second_rng).expect("generation should succeed");
        assert_eq!(cage_structure(&first), cage_structure(&second));
    }

    #[test]
    fn an_out_of_range_condition_index_is_a_trial_error() {
        let prepared = prepare("n: [3]").expect("valid parameters should prepare");
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        match prepared.run_trial(5, &mut rng) {
            Err(Error::Trial {
                experiment, reason, ..
            }) => {
                assert_eq!(experiment, NAME);
                assert!(reason.contains("out of range"), "unexpected: {reason}");
            }
            other => panic!("expected a Trial error, got {other:?}"),
        }
    }

    /// Splits a CSV file into its header and data rows.
    fn read_csv_rows(path: &std::path::Path) -> (String, Vec<String>) {
        let contents = std::fs::read_to_string(path).expect("the CSV file should be readable");
        let mut lines = contents.lines().map(str::to_owned);
        let header = lines.next().expect("the CSV file should have a header");
        (header, lines.collect())
    }

    // A real (if tiny) end-to-end run through `app::run`; slow in a debug
    // build, so opt-in: `cargo test -p mathdoku-explorer -- --ignored`.
    #[test]
    #[ignore = "runs real puzzle generation and solving; slow in debug builds"]
    fn end_to_end_smoke_run_writes_the_expected_run_directory() {
        let directory = unique_temp_dir("solve-time-smoke");
        let config_path = directory.join("experiment.yaml");
        let output_directory = directory.join("results");
        std::fs::write(
            &config_path,
            format!(
                "experiment: solve-time\nseed: 20260609\nwarmup: 1\nsamples: 3\n\
                 output_directory: {}\nparameters:\n  n: [3, 4]\n",
                output_directory.display()
            ),
        )
        .expect("the config file should be writable");
        let cli = Cli::try_parse_from([
            "mathdoku-explorer",
            "perf",
            config_path.to_str().expect("the path should be UTF-8"),
            "--allow-debug",
        ])
        .expect("the test arguments should parse");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        crate::app::run(cli, &mut stdout, &mut stderr).expect("the smoke run should succeed");
        let stdout = String::from_utf8(stdout).expect("output should be UTF-8");

        let run_directories: Vec<_> = std::fs::read_dir(&output_directory)
            .expect("the output directory should exist")
            .collect::<Result<_, _>>()
            .expect("the output directory should be readable");
        assert_eq!(run_directories.len(), 1, "exactly one run directory");
        let run_directory = run_directories[0].path();
        assert!(
            stdout.trim_end().ends_with(
                run_directory
                    .to_str()
                    .expect("the run directory should be UTF-8")
            ),
            "stdout should name the run directory, got:\n{stdout}"
        );

        // raw.csv: one row per measured trial = 2 conditions x 3 samples.
        let (raw_header, raw_rows) = read_csv_rows(&run_directory.join("raw.csv"));
        assert_eq!(
            raw_header,
            "experiment,condition,trial,seed,propagate_ns,search_ns,total_ns"
        );
        assert_eq!(raw_rows.len(), 6);

        // summary.csv: one row per condition x phase (incl. derived total).
        let (_, summary_rows) = read_csv_rows(&run_directory.join("summary.csv"));
        let condition_phases: Vec<(String, String)> = summary_rows
            .iter()
            .map(|row| {
                let mut fields = row.split(',');
                (
                    fields.next().expect("a condition column").to_owned(),
                    fields.next().expect("a phase column").to_owned(),
                )
            })
            .collect();
        let expected: Vec<(String, String)> = ["n=3", "n=4"]
            .iter()
            .flat_map(|&condition| {
                ["propagate", "search", "total"]
                    .iter()
                    .map(move |&phase| (condition.to_owned(), phase.to_owned()))
            })
            .collect();
        assert_eq!(condition_phases, expected);

        std::fs::remove_dir_all(&directory).expect("the temp directory should be removable");
    }
}
