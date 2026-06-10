//! Experiment configuration schema (ADR-0007).
//!
//! Theory: a YAML file drives every `perf` run. The fields of the common
//! statistical protocol — which experiment to run, the master seed, warmup
//! and sample counts, the output location — are strongly typed and
//! validated here, once, so the runner can rely on their invariants (for
//! example `samples >= 2`, without which a sample standard deviation does
//! not exist). The `parameters` field is deliberately opaque YAML owned by
//! the experiment it names: adding a new experiment never touches this
//! schema. Unknown fields are rejected so that configuration typos fail
//! loudly instead of being silently ignored.
//!
//! Praxis: load with [`ExperimentConfig::from_path`], or
//! [`ExperimentConfig::from_reader`] for in-memory sources; both validate.
//! The type also serializes, because the runner writes the resolved
//! configuration — including the actually-used seed — back out alongside
//! the results for provenance.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The common statistical protocol of a performance experiment run.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentConfig {
    /// The name of the registered experiment to run.
    pub experiment: String,

    /// The master seed for reproducible randomness. When omitted, the
    /// runner draws one and records it in the resolved configuration.
    #[serde(default)]
    pub seed: Option<u64>,

    /// Discarded trials per condition, run before measurement to absorb
    /// cache/branch-predictor/allocator transients.
    #[serde(default = "default_warmup")]
    pub warmup: u32,

    /// Measured trials per condition.
    #[serde(default = "default_samples")]
    pub samples: u32,

    /// The directory under which the run's output directory is created.
    #[serde(default = "default_output_directory")]
    pub output_directory: PathBuf,

    /// Experiment-specific parameters, opaque to the test bed.
    #[serde(default)]
    pub parameters: serde_yaml_ng::Value,
}

impl ExperimentConfig {
    /// Parses and validates a configuration from a YAML source.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigParse`] when the source is not YAML matching
    /// the schema (including unknown fields), and [`Error::InvalidConfig`]
    /// when a field violates the protocol's invariants.
    pub fn from_reader(reader: impl Read) -> Result<Self, Error> {
        let config: Self = serde_yaml_ng::from_reader(reader)?;
        config.validate()?;
        Ok(config)
    }

    /// Parses and validates the configuration file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigRead`] when the file cannot be opened, and
    /// [`Error::ConfigFile`] wrapping the underlying failure when its
    /// contents do not parse or do not validate.
    pub fn from_path(path: &Path) -> Result<Self, Error> {
        let file = File::open(path).map_err(|source| Error::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_reader(file).map_err(|source| Error::ConfigFile {
            path: path.to_path_buf(),
            source: Box::new(source),
        })
    }

    /// Checks the protocol invariants that the schema alone cannot express.
    fn validate(&self) -> Result<(), Error> {
        if self.experiment.trim().is_empty() {
            return Err(Error::InvalidConfig {
                field: "experiment",
                reason: "must name a registered experiment, not be empty".to_owned(),
            });
        }
        if self.samples < 2 {
            return Err(Error::InvalidConfig {
                field: "samples",
                reason: format!(
                    "must be at least 2 so a sample standard deviation exists, got {}",
                    self.samples
                ),
            });
        }
        Ok(())
    }
}

/// Default number of discarded warmup trials per condition.
const fn default_warmup() -> u32 {
    3
}

/// Default number of measured trials per condition.
const fn default_samples() -> u32 {
    50
}

/// Default directory under which run output directories are created.
fn default_output_directory() -> PathBuf {
    PathBuf::from("results")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde::Deserialize;

    use super::ExperimentConfig;
    use crate::error::Error;

    /// The example configuration from ADR-0007, comments included.
    const ADR_EXAMPLE: &str = "\
# Which registered experiment to run, and the statistical protocol.
experiment: solve-time
seed: 20260609          # optional; omitted -> randomly drawn and recorded
warmup: 3               # discarded trials per condition
samples: 50             # measured trials per condition
output_directory: results
parameters:             # experiment-specific, opaque to the test bed
  n: [3, 4, 5, 6, 7]
";

    fn parse(yaml: &str) -> Result<ExperimentConfig, Error> {
        ExperimentConfig::from_reader(yaml.as_bytes())
    }

    #[test]
    fn adr_example_parses_to_expected_values() {
        /// The shape the `solve-time` experiment will read its opaque
        /// parameters into.
        #[derive(Debug, Deserialize)]
        struct SolveTimeParameters {
            n: Vec<u32>,
        }

        let config = parse(ADR_EXAMPLE).expect("the ADR example should parse");
        assert_eq!(config.experiment, "solve-time");
        assert_eq!(config.seed, Some(20_260_609));
        assert_eq!(config.warmup, 3);
        assert_eq!(config.samples, 50);
        assert_eq!(config.output_directory, PathBuf::from("results"));

        let parameters: SolveTimeParameters = serde_yaml_ng::from_value(config.parameters)
            .expect("the parameters mapping should deserialize");
        assert_eq!(parameters.n, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let config = parse("experiment: foo\n").expect("a minimal config should parse");
        assert_eq!(config.experiment, "foo");
        assert_eq!(config.seed, None);
        assert_eq!(config.warmup, 3);
        assert_eq!(config.samples, 50);
        assert_eq!(config.output_directory, PathBuf::from("results"));
        assert_eq!(config.parameters, serde_yaml_ng::Value::Null);
    }

    #[test]
    fn empty_experiment_name_is_rejected() {
        let result = parse("experiment: \"\"\n");
        assert!(
            matches!(
                result,
                Err(Error::InvalidConfig {
                    field: "experiment",
                    ..
                })
            ),
            "expected InvalidConfig on `experiment`, got {result:?}"
        );
    }

    #[test]
    fn zero_samples_is_rejected() {
        let result = parse("experiment: foo\nsamples: 0\n");
        assert!(
            matches!(
                result,
                Err(Error::InvalidConfig {
                    field: "samples",
                    ..
                })
            ),
            "expected InvalidConfig on `samples`, got {result:?}"
        );
    }

    #[test]
    fn one_sample_is_rejected() {
        let result = parse("experiment: foo\nsamples: 1\n");
        assert!(
            matches!(
                result,
                Err(Error::InvalidConfig {
                    field: "samples",
                    ..
                })
            ),
            "expected InvalidConfig on `samples`, got {result:?}"
        );
    }

    #[test]
    fn two_samples_is_accepted() {
        let config = parse("experiment: foo\nsamples: 2\n").expect("samples: 2 should validate");
        assert_eq!(config.samples, 2);
    }

    #[test]
    fn unknown_field_is_rejected() {
        let result = parse("experiment: foo\nsample: 10\n");
        assert!(
            matches!(result, Err(Error::ConfigParse { .. })),
            "expected ConfigParse for an unknown field, got {result:?}"
        );
    }

    #[test]
    fn malformed_yaml_is_rejected() {
        let result = parse("experiment: [unclosed\n");
        assert!(
            matches!(result, Err(Error::ConfigParse { .. })),
            "expected ConfigParse for malformed YAML, got {result:?}"
        );
    }

    #[test]
    fn serialize_then_parse_round_trips() {
        let original = parse(ADR_EXAMPLE).expect("the ADR example should parse");
        let yaml = serde_yaml_ng::to_string(&original).expect("the config should serialize");
        let reparsed = parse(&yaml).expect("the serialized config should parse back");
        assert_eq!(reparsed, original);
    }

    #[test]
    fn from_path_reads_a_file() {
        let path = std::env::temp_dir().join(format!(
            "mathdoku-explorer-config-test-{}.yaml",
            std::process::id()
        ));
        std::fs::write(&path, ADR_EXAMPLE).expect("the temporary file should be writable");
        let result = ExperimentConfig::from_path(&path);
        std::fs::remove_file(&path).expect("the temporary file should be removable");
        let config = result.expect("the file should parse");
        assert_eq!(config.experiment, "solve-time");
    }

    #[test]
    fn from_path_names_the_missing_file() {
        let path = Path::new("/nonexistent/experiment.yaml");
        let result = ExperimentConfig::from_path(path);
        match result {
            Err(Error::ConfigRead { path: reported, .. }) => assert_eq!(reported, path),
            other => panic!("expected ConfigRead naming the path, got {other:?}"),
        }
    }

    #[test]
    fn from_path_names_the_file_on_invalid_contents() {
        let path = std::env::temp_dir().join(format!(
            "mathdoku-explorer-config-invalid-test-{}.yaml",
            std::process::id()
        ));
        std::fs::write(&path, "experiment: foo\nsamples: 1\n")
            .expect("the temporary file should be writable");
        let result = ExperimentConfig::from_path(&path);
        std::fs::remove_file(&path).expect("the temporary file should be removable");
        match result {
            Err(Error::ConfigFile {
                path: reported,
                source,
            }) => {
                assert_eq!(reported, path);
                assert!(
                    matches!(
                        *source,
                        Error::InvalidConfig {
                            field: "samples",
                            ..
                        }
                    ),
                    "expected a wrapped InvalidConfig, got {source:?}"
                );
            }
            other => panic!("expected ConfigFile naming the path, got {other:?}"),
        }
    }
}
