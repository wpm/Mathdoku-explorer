//! The performance test bed's protocol engine (ADR-0007).
//!
//! Theory: a run is `conditions × (warmup + samples)` trials. Per condition
//! the runner executes `warmup` discarded trials — absorbing cache, branch
//! predictor, and allocator transients — followed by `samples` measured
//! trials. Trial randomness is fully determined by the configuration: a
//! master seed plus the condition and trial indices derive a per-trial
//! [`ChaCha8Rng`] through [`derive_seed`], so any single trial can be
//! re-run in isolation and a whole run is bit-for-bit reproducible. The
//! trial index space is *unified*: warmup occupies indices `0..warmup` and
//! measurement `warmup..warmup + samples`, so a trial's seed is a pure
//! function of `(master, condition, trial)` and never of whether the trial
//! happened to be warmup — re-running trial 7 of condition 2 in a debugger
//! reproduces exactly what the run measured. (Changing `warmup` in the
//! configuration shifts which indices are measured; that is a different
//! experiment, deliberately.)
//!
//! When an experiment declares more than one phase the runner appends a
//! derived `total` phase, the per-trial sum, so downstream analysis never
//! recomputes it inconsistently.
//!
//! Praxis: [`perf`] is the `perf` subcommand's engine. It takes the
//! registry as a slice so tests inject fakes; [`crate::app`] passes
//! [`crate::experiments::registry`].

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha8Rng;

use crate::config::ExperimentConfig;
use crate::error::Error;
use crate::experiments::{Condition, Experiment, Measurement, PreparedExperiment};
use crate::output;

/// The reserved name of the derived per-trial sum phase.
pub const TOTAL_PHASE: &str = "total";

/// One step of the `SplitMix64` output function: adds the golden-gamma
/// increment to `state` and applies the murmur-inspired finalizer.
///
/// Constants are from Sebastiano Vigna's public-domain reference
/// implementation (<https://prng.di.unimi.it/splitmix64.c>), itself from
/// Steele, Lea & Flood, "Fast splittable pseudorandom number generators",
/// OOPSLA 2014.
const fn splitmix64(state: u64) -> u64 {
    let z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derives the seed of one trial's `ChaCha8Rng` from the master seed and
/// the trial's coordinates:
///
/// ```text
/// splitmix64(master ^ splitmix64(condition + 1) ^ splitmix64((trial + 1) << 32))
/// ```
///
/// Each coordinate is offset by one so that index 0 does not contribute the
/// identity, and the trial index is shifted into the high half so the two
/// coordinates enter the mix on disjoint bits; the outer `splitmix64` then
/// diffuses the combination. The trial index lives in the unified index
/// space described in the module documentation (warmup first, then
/// measured).
#[must_use]
pub const fn derive_seed(master: u64, condition_index: u64, trial_index: u64) -> u64 {
    splitmix64(
        master
            ^ splitmix64(condition_index.wrapping_add(1))
            ^ splitmix64(trial_index.wrapping_add(1) << 32),
    )
}

/// The statistical protocol of one run, resolved from the configuration.
#[derive(Debug, Clone, Copy)]
pub struct Protocol {
    /// The resolved master seed.
    pub master_seed: u64,
    /// Discarded trials per condition.
    pub warmup: u32,
    /// Measured trials per condition.
    pub samples: u32,
}

/// One measured trial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trial {
    /// Index of the trial's condition in [`RunData::conditions`].
    pub condition_index: usize,
    /// The trial's index in the unified index space (`>= warmup`).
    pub index: u32,
    /// The seed of the trial's RNG, from [`derive_seed`].
    pub seed: u64,
    /// One duration per entry of [`RunData::phases`], in that order.
    pub durations: Vec<Duration>,
}

/// Everything a run measured, ready for the output writers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunData {
    /// The conditions, in experiment order.
    pub conditions: Vec<Condition>,
    /// The reported phases: the experiment's declared phases, plus the
    /// derived [`TOTAL_PHASE`] when there is more than one.
    pub phases: Vec<&'static str>,
    /// Every measured trial, condition-major then trial order.
    pub trials: Vec<Trial>,
}

/// Runs the `perf` subcommand: loads the configuration at `config_path`,
/// looks the experiment up in `registry`, executes the protocol, writes the
/// run directory, and prints the human summary — the final stdout line is
/// the run directory path, which is also returned.
///
/// # Errors
///
/// Returns [`Error::DebugBuild`] in a `debug_assertions` build unless
/// `allow_debug` is set, [`Error::UnknownExperiment`] when the
/// configuration names an unregistered experiment, and propagates
/// configuration, experiment, statistics, and output failures.
pub fn perf(
    registry: &[Box<dyn Experiment>],
    config_path: &Path,
    allow_debug: bool,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<PathBuf, Error> {
    if cfg!(debug_assertions) && !allow_debug {
        return Err(Error::DebugBuild);
    }
    let mut config = ExperimentConfig::from_path(config_path)?;
    let experiment = registry
        .iter()
        .find(|candidate| candidate.name() == config.experiment)
        .ok_or_else(|| Error::UnknownExperiment {
            name: config.experiment.clone(),
            known: registry
                .iter()
                .map(|candidate| candidate.name().to_owned())
                .collect(),
        })?;
    // Resolve the master seed so the recorded configuration reproduces the
    // run even when the seed was drawn from entropy.
    let master_seed = config.seed.unwrap_or_else(|| rand::rng().random());
    config.seed = Some(master_seed);
    let protocol = Protocol {
        master_seed,
        warmup: config.warmup,
        samples: config.samples,
    };
    let prepared = experiment.prepare(&config.parameters)?;
    let data = run_prepared(
        experiment.name(),
        prepared.as_ref(),
        experiment.phases(),
        protocol,
        stderr,
    )?;
    let run_directory = output::write_run(&config, &data, &output::now_timestamp())?;
    stdout.write_all(output::render_summary_table(&data)?.as_bytes())?;
    writeln!(stdout, "{}", run_directory.display())?;
    Ok(run_directory)
}

/// Executes the protocol against a prepared experiment, streaming one
/// progress line per condition to `stderr`.
///
/// # Errors
///
/// Returns [`Error::PhaseMismatch`] when a trial's measurements do not
/// match the declared phases, and propagates trial and write failures.
pub fn run_prepared(
    experiment_name: &str,
    prepared: &dyn PreparedExperiment,
    declared_phases: &'static [&'static str],
    protocol: Protocol,
    stderr: &mut impl Write,
) -> Result<RunData, Error> {
    let conditions = prepared.conditions().to_vec();
    let mut phases: Vec<&'static str> = declared_phases.to_vec();
    let derive_total = declared_phases.len() > 1;
    if derive_total {
        phases.push(TOTAL_PHASE);
    }
    let mut trials = Vec::new();
    for (condition_index, condition) in conditions.iter().enumerate() {
        write!(
            stderr,
            "condition {}: {} warmup + {} samples...",
            condition.label, protocol.warmup, protocol.samples
        )?;
        stderr.flush()?;
        let started = Instant::now();
        for trial_index in 0..protocol.warmup + protocol.samples {
            let seed = derive_seed(
                protocol.master_seed,
                condition_index as u64,
                u64::from(trial_index),
            );
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let measurements = prepared.run_trial(condition_index, &mut rng)?;
            if trial_index < protocol.warmup {
                continue;
            }
            let mut durations =
                durations_in_declared_order(experiment_name, declared_phases, &measurements)?;
            if derive_total {
                durations.push(durations.iter().sum());
            }
            trials.push(Trial {
                condition_index,
                index: trial_index,
                seed,
                durations,
            });
        }
        writeln!(stderr, " done in {:.2}s", started.elapsed().as_secs_f64())?;
    }
    Ok(RunData {
        conditions,
        phases,
        trials,
    })
}

/// Checks that `measurements` reports exactly the declared phases, in
/// declared order, and extracts the durations in that order.
fn durations_in_declared_order(
    experiment_name: &str,
    declared_phases: &'static [&'static str],
    measurements: &[Measurement],
) -> Result<Vec<Duration>, Error> {
    let matches = measurements.len() == declared_phases.len()
        && declared_phases
            .iter()
            .zip(measurements)
            .all(|(&declared, measurement)| declared == measurement.phase);
    if !matches {
        return Err(Error::PhaseMismatch {
            experiment: experiment_name.to_owned(),
            expected: declared_phases.join(", "),
            actual: measurements
                .iter()
                .map(|measurement| measurement.phase)
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    Ok(measurements
        .iter()
        .map(|measurement| measurement.duration)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use crate::config::ExperimentConfig;
    use crate::error::Error;
    use crate::experiments::fake::FakeExperiment;
    use crate::experiments::{Experiment as _, PreparedExperiment};
    use crate::testutil::unique_temp_dir;

    use super::{Protocol, RunData, derive_seed, perf, run_prepared};

    /// Prepares a fake experiment with null parameters.
    fn prepare(fake: &FakeExperiment) -> Box<dyn PreparedExperiment> {
        fake.prepare(&serde_yaml_ng::Value::Null)
            .expect("the fake should prepare")
    }

    /// Runs a well-behaved fake under `protocol`, returning the run data
    /// and the stderr progress text.
    fn run_fake(
        phases: &'static [&'static str],
        condition_count: usize,
        protocol: Protocol,
    ) -> (FakeExperiment, RunData, String) {
        let fake = FakeExperiment::new(phases, condition_count);
        let mut stderr = Vec::new();
        let data = run_prepared(
            fake.name(),
            prepare(&fake).as_ref(),
            phases,
            protocol,
            &mut stderr,
        )
        .expect("the fake run should succeed");
        let progress = String::from_utf8(stderr).expect("progress should be UTF-8");
        (fake, data, progress)
    }

    #[test]
    fn derive_seed_is_deterministic() {
        assert_eq!(derive_seed(42, 3, 9), derive_seed(42, 3, 9));
    }

    // Reference values from an independent Python implementation of the
    // SplitMix64 mixing recipe in the `derive_seed` documentation, pinning
    // the derivation: changing it would silently re-randomise every
    // recorded experiment's per-trial seeds.
    #[test]
    fn derive_seed_matches_reference_values() {
        assert_eq!(derive_seed(0, 0, 0), 0x9BAA_BCDA_20DB_2FCE);
        assert_eq!(derive_seed(0, 0, 1), 0xB2FD_E1E9_7BEF_AFB7);
        assert_eq!(derive_seed(0, 1, 0), 0x9D55_856C_F038_13A5);
        assert_eq!(derive_seed(1, 0, 0), 0x8B23_7601_BAF3_B309);
        assert_eq!(derive_seed(20_260_609, 2, 7), 0x69F6_7586_619F_3CC5);
    }

    #[test]
    fn derive_seed_is_distinct_across_masters_conditions_and_trials() {
        let mut seeds = HashSet::new();
        for master in 0..3 {
            for condition in 0..8 {
                for trial in 0..32 {
                    assert!(
                        seeds.insert(derive_seed(master, condition, trial)),
                        "collision at master {master}, condition {condition}, trial {trial}"
                    );
                }
            }
        }
        assert_eq!(seeds.len(), 3 * 8 * 32);
    }

    #[test]
    fn warmup_trials_run_but_are_not_recorded() {
        let protocol = Protocol {
            master_seed: 11,
            warmup: 2,
            samples: 3,
        };
        let (fake, data, _) = run_fake(&["alpha"], 2, protocol);
        // Every condition runs warmup + samples trials, in condition order.
        assert_eq!(
            *fake.calls.lock().expect("no test poisons the calls"),
            vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 1]
        );
        // Only the measured trials are recorded, tagged with their indices
        // in the unified (warmup-first) index space.
        assert_eq!(data.trials.len(), 6);
        for condition_index in 0..2 {
            let indices: Vec<u32> = data
                .trials
                .iter()
                .filter(|trial| trial.condition_index == condition_index)
                .map(|trial| trial.index)
                .collect();
            assert_eq!(indices, vec![2, 3, 4]);
        }
    }

    #[test]
    fn trial_seeds_are_a_pure_function_of_master_condition_and_unified_index() {
        let protocol = Protocol {
            master_seed: 99,
            warmup: 1,
            samples: 2,
        };
        let (_, data, _) = run_fake(&["alpha"], 3, protocol);
        for trial in &data.trials {
            assert_eq!(
                trial.seed,
                derive_seed(
                    protocol.master_seed,
                    trial.condition_index as u64,
                    u64::from(trial.index)
                )
            );
        }
    }

    #[test]
    fn a_single_phase_gets_no_derived_total() {
        let protocol = Protocol {
            master_seed: 1,
            warmup: 0,
            samples: 2,
        };
        let (_, data, _) = run_fake(&["alpha"], 1, protocol);
        assert_eq!(data.phases, vec!["alpha"]);
        for trial in &data.trials {
            assert_eq!(trial.durations.len(), 1);
        }
    }

    #[test]
    fn multiple_phases_get_a_derived_total_equal_to_their_sum() {
        let protocol = Protocol {
            master_seed: 1,
            warmup: 0,
            samples: 2,
        };
        let (_, data, _) = run_fake(&["alpha", "beta"], 2, protocol);
        assert_eq!(data.phases, vec!["alpha", "beta", "total"]);
        for trial in &data.trials {
            assert_eq!(trial.durations.len(), 3);
            assert_eq!(trial.durations[2], trial.durations[0] + trial.durations[1]);
        }
    }

    #[test]
    fn reruns_with_the_same_master_seed_are_bit_identical() {
        let protocol = Protocol {
            master_seed: 7,
            warmup: 1,
            samples: 4,
        };
        let (_, first, _) = run_fake(&["alpha", "beta"], 2, protocol);
        let (_, second, _) = run_fake(&["alpha", "beta"], 2, protocol);
        assert_eq!(first, second);
    }

    #[test]
    fn reruns_with_a_different_master_seed_diverge() {
        let (_, first, _) = run_fake(
            &["alpha"],
            1,
            Protocol {
                master_seed: 7,
                warmup: 0,
                samples: 3,
            },
        );
        let (_, second, _) = run_fake(
            &["alpha"],
            1,
            Protocol {
                master_seed: 8,
                warmup: 0,
                samples: 3,
            },
        );
        assert_ne!(first, second);
    }

    #[test]
    fn progress_goes_to_stderr_one_line_per_condition() {
        let protocol = Protocol {
            master_seed: 5,
            warmup: 1,
            samples: 2,
        };
        let (_, _, progress) = run_fake(&["alpha"], 2, protocol);
        let lines: Vec<&str> = progress.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("condition c0: 1 warmup + 2 samples..."));
        assert!(lines[0].ends_with('s'), "the line should end in a duration");
        assert!(lines[1].starts_with("condition c1:"));
    }

    #[test]
    fn misreported_phases_are_a_phase_mismatch_error() {
        let fake = FakeExperiment::misreporting(&["alpha", "beta"], &["alpha", "gamma"], 1);
        let mut stderr = Vec::new();
        let result = run_prepared(
            fake.name(),
            prepare(&fake).as_ref(),
            &["alpha", "beta"],
            Protocol {
                master_seed: 1,
                warmup: 0,
                samples: 2,
            },
            &mut stderr,
        );
        match result {
            Err(Error::PhaseMismatch {
                experiment,
                expected,
                actual,
            }) => {
                assert_eq!(experiment, "fake");
                assert_eq!(expected, "alpha, beta");
                assert_eq!(actual, "alpha, gamma");
            }
            other => panic!("expected PhaseMismatch, got {other:?}"),
        }
    }

    /// A registry containing one well-behaved two-phase fake.
    fn fake_registry() -> Vec<Box<dyn crate::experiments::Experiment>> {
        vec![Box::new(FakeExperiment::new(&["alpha", "beta"], 2))]
    }

    /// Writes `contents` as the experiment configuration file in
    /// `directory` and returns its path.
    fn write_config(directory: &Path, contents: &str) -> std::path::PathBuf {
        let path = directory.join("experiment.yaml");
        std::fs::write(&path, contents).expect("the config file should be writable");
        path
    }

    #[cfg(debug_assertions)]
    #[test]
    fn a_debug_build_is_refused_without_allow_debug() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = perf(
            &fake_registry(),
            Path::new("never-read.yaml"),
            false,
            &mut stdout,
            &mut stderr,
        );
        assert!(matches!(result, Err(Error::DebugBuild)));
        assert!(stdout.is_empty());
    }

    #[test]
    fn perf_records_the_drawn_seed_when_the_configuration_omits_it() {
        let directory = unique_temp_dir("runner-drawn-seed");
        let config_path = write_config(
            &directory,
            &format!(
                "experiment: fake\nwarmup: 0\nsamples: 2\noutput_directory: {}\n",
                directory.join("results").display()
            ),
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let run_directory = perf(
            &fake_registry(),
            &config_path,
            true,
            &mut stdout,
            &mut stderr,
        )
        .expect("perf should succeed");
        let written = std::fs::File::open(run_directory.join("config.yaml"))
            .expect("config.yaml should exist");
        let resolved = ExperimentConfig::from_reader(written)
            .expect("the recorded configuration should parse and validate");
        std::fs::remove_dir_all(&directory).expect("the temp directory should be removable");
        assert!(
            resolved.seed.is_some(),
            "the drawn master seed must be recorded for reproducibility"
        );
    }

    #[test]
    fn perf_reruns_with_the_same_seed_write_identical_raw_csv() {
        let directory = unique_temp_dir("runner-rerun");
        let results = directory.join("results");
        let same_seed = write_config(
            &directory,
            &format!(
                "experiment: fake\nseed: 7\nwarmup: 1\nsamples: 3\noutput_directory: {}\n",
                results.display()
            ),
        );
        let raw = |config_path: &Path| {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let run_directory = perf(
                &fake_registry(),
                config_path,
                true,
                &mut stdout,
                &mut stderr,
            )
            .expect("perf should succeed");
            std::fs::read(run_directory.join("raw.csv")).expect("raw.csv should exist")
        };
        let first = raw(&same_seed);
        let second = raw(&same_seed);
        let different_seed = write_config(
            &directory,
            &format!(
                "experiment: fake\nseed: 8\nwarmup: 1\nsamples: 3\noutput_directory: {}\n",
                results.display()
            ),
        );
        let third = raw(&different_seed);
        std::fs::remove_dir_all(&directory).expect("the temp directory should be removable");
        assert_eq!(first, second, "same master seed must reproduce raw.csv");
        assert_ne!(first, third, "a different master seed must diverge");
    }
}
