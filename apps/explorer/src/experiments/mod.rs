//! The Experiment abstraction and registry (ADR-0007).
//!
//! Theory: an experiment's life has two stages, and the trait split mirrors
//! them. An [`Experiment`] is the *registered identity*: a name to look up,
//! a description to list, and the measurement phases it will report —
//! declared up front so output schemas (CSV headers) are fixed before the
//! first trial runs. Calling [`Experiment::prepare`] with the opaque YAML
//! `parameters` from the configuration produces a [`PreparedExperiment`]:
//! the parameters parsed once into whatever state the experiment needs,
//! plus the ordered list of [`Condition`]s — the levels of the independent
//! variable. The runner then drives [`PreparedExperiment::run_trial`] by
//! condition *index*, so the prepared state (not the runner) owns the
//! meaning of each condition; the runner treats conditions opaquely apart
//! from index and label. Both traits are object-safe, so the registry is a
//! plain vector of boxed experiments and registration is one line.
//!
//! Praxis: implement both traits, append one `Box::new(...)` line to
//! [`registry`], and document the experiment in the README. The test bed's
//! YAML schema and runner are untouched.

use std::time::Duration;

use rand_chacha::ChaCha8Rng;

use crate::error::Error;

/// One level of the independent variable (e.g. `n = 5`); the unit of
/// statistical aggregation. The runner treats it opaquely apart from its
/// index in the prepared experiment's condition list and this label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    /// Human- and CSV-facing name of the condition, e.g. `n=5`.
    pub label: String,
}

impl Condition {
    /// Creates a condition with the given label.
    // Real experiments build their condition lists through this; until the
    // first one registers (`solve-time`, next PR) only test code calls it,
    // so the non-test binary sees it as dead.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by experiments; first lands next PR")
    )]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

/// One timed phase of one trial, in wall-clock time measured by the
/// experiment around the operation under test only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Which declared phase this duration belongs to.
    pub phase: &'static str,
    /// The measured wall-clock duration.
    pub duration: Duration,
}

/// A named, registered experiment: identity and output schema, known
/// before any configuration is read.
pub trait Experiment {
    /// The registry name, matched against the `experiment` field of the
    /// configuration.
    fn name(&self) -> &'static str;

    /// A one-line description for `mathdoku-explorer list`.
    fn description(&self) -> &'static str;

    /// The measurement phases every trial reports, in reporting order.
    /// Declared up front so CSV headers are stable before the first trial.
    /// The name `total` is reserved for the runner's derived sum phase.
    fn phases(&self) -> &'static [&'static str];

    /// Parses the opaque YAML `parameters` into a ready-to-run experiment
    /// with its ordered condition list.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] when the parameters do not parse or describe an
    /// invalid experiment (the variant is the experiment's choice).
    fn prepare(
        &self,
        parameters: &serde_yaml_ng::Value,
    ) -> Result<Box<dyn PreparedExperiment>, Error>;
}

/// An experiment with parsed parameters, ready to run trials.
pub trait PreparedExperiment {
    /// The ordered levels of the independent variable.
    fn conditions(&self) -> &[Condition];

    /// Runs one trial under the condition at `condition_index`, generating
    /// any problem instance from `rng` (so trials are i.i.d. samples) and
    /// returning one [`Measurement`] per declared phase.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] when the trial cannot be executed.
    fn run_trial(
        &self,
        condition_index: usize,
        rng: &mut ChaCha8Rng,
    ) -> Result<Vec<Measurement>, Error>;
}

/// Every registered experiment, in listing order.
///
/// Registration is one `Box::new(...)` line per experiment. The first real
/// experiment, `solve-time` (ADR-0007), lands in the next PR.
#[must_use]
pub fn registry() -> Vec<Box<dyn Experiment>> {
    Vec::new()
}

/// A deterministic fake experiment for exercising the runner and output
/// writers without the mathdoku library.
#[cfg(test)]
pub mod fake {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rand::RngExt as _;
    use rand_chacha::ChaCha8Rng;

    use super::{Condition, Experiment, Measurement, PreparedExperiment};
    use crate::error::Error;

    /// A fake experiment with configurable phases and condition count.
    ///
    /// Trial durations are derived from the trial's RNG, so two runs with
    /// the same master seed produce bit-identical measurements and two runs
    /// with different seeds do not — exactly the reproducibility contract
    /// the test bed promises for real experiments.
    pub struct FakeExperiment {
        declared_phases: &'static [&'static str],
        /// Phases actually reported by `run_trial`; differs from
        /// `declared_phases` only to provoke the runner's mismatch check.
        reported_phases: &'static [&'static str],
        condition_count: usize,
        /// Condition index of every `run_trial` call, in call order.
        pub calls: Arc<Mutex<Vec<usize>>>,
    }

    impl FakeExperiment {
        /// A well-behaved fake reporting exactly its declared phases.
        pub fn new(phases: &'static [&'static str], condition_count: usize) -> Self {
            Self {
                declared_phases: phases,
                reported_phases: phases,
                condition_count,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// A misbehaving fake whose trials report `reported` instead of the
        /// declared phases.
        pub fn misreporting(
            declared: &'static [&'static str],
            reported: &'static [&'static str],
            condition_count: usize,
        ) -> Self {
            Self {
                declared_phases: declared,
                reported_phases: reported,
                condition_count,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Experiment for FakeExperiment {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn description(&self) -> &'static str {
            "a deterministic fake for tests"
        }

        fn phases(&self) -> &'static [&'static str] {
            self.declared_phases
        }

        fn prepare(
            &self,
            _parameters: &serde_yaml_ng::Value,
        ) -> Result<Box<dyn PreparedExperiment>, Error> {
            Ok(Box::new(PreparedFake {
                conditions: (0..self.condition_count)
                    .map(|index| Condition::new(format!("c{index}")))
                    .collect(),
                reported_phases: self.reported_phases,
                calls: Arc::clone(&self.calls),
            }))
        }
    }

    struct PreparedFake {
        conditions: Vec<Condition>,
        reported_phases: &'static [&'static str],
        calls: Arc<Mutex<Vec<usize>>>,
    }

    impl PreparedExperiment for PreparedFake {
        fn conditions(&self) -> &[Condition] {
            &self.conditions
        }

        fn run_trial(
            &self,
            condition_index: usize,
            rng: &mut ChaCha8Rng,
        ) -> Result<Vec<Measurement>, Error> {
            self.calls.lock().unwrap().push(condition_index);
            Ok(self
                .reported_phases
                .iter()
                .map(|&phase| Measurement {
                    phase,
                    duration: Duration::from_nanos(rng.random_range(1_000..1_000_000)),
                })
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng as _;
    use rand_chacha::ChaCha8Rng;

    use super::fake::FakeExperiment;
    use super::{Condition, Experiment as _, registry};

    #[test]
    fn registry_is_empty_until_the_first_experiment_lands() {
        // `solve-time` (ADR-0007) registers in the next PR.
        assert!(registry().is_empty());
    }

    #[test]
    fn fake_prepares_labelled_conditions() {
        let fake = FakeExperiment::new(&["alpha", "beta"], 3);
        let prepared = fake
            .prepare(&serde_yaml_ng::Value::Null)
            .expect("the fake should prepare");
        assert_eq!(
            prepared.conditions(),
            &[
                Condition::new("c0"),
                Condition::new("c1"),
                Condition::new("c2"),
            ]
        );
    }

    #[test]
    fn fake_trials_are_seed_deterministic() {
        let fake = FakeExperiment::new(&["alpha", "beta"], 1);
        let prepared = fake
            .prepare(&serde_yaml_ng::Value::Null)
            .expect("the fake should prepare");
        let mut first_rng = ChaCha8Rng::seed_from_u64(7);
        let mut second_rng = ChaCha8Rng::seed_from_u64(7);
        let mut other_rng = ChaCha8Rng::seed_from_u64(8);
        let first = prepared.run_trial(0, &mut first_rng).expect("trial");
        let second = prepared.run_trial(0, &mut second_rng).expect("trial");
        let other = prepared.run_trial(0, &mut other_rng).expect("trial");
        assert_eq!(first, second, "same seed must reproduce measurements");
        assert_ne!(first, other, "different seeds must differ");
        assert_eq!(*fake.calls.lock().unwrap(), vec![0, 0, 0]);
    }
}
