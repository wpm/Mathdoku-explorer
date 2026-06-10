# ADR-0007: Mathdoku Explorer — a CLI test bed for scientific performance experiments

## Status

Accepted

## Context

The `mathdoku` library has accumulated performance-sensitive machinery
(constraint propagation, MDD-based cage feasibility, Régin filtering) whose
behaviour as a function of puzzle size `n` is understood anecdotally but not
measured systematically. Past optimisation work (#92, #99, #104, #106, #107)
relied on ad-hoc timing embedded in unit tests, which conflates correctness
testing with measurement and produces no durable, comparable data.

We want a home for *scientific experiments* over the library: measuring
runtime distributions as a function of `n`, comparing algorithm variants,
measuring the distribution of solution counts, and whatever questions come
next. These experiments need statistically defensible results: repeated
trials, controlled randomness, warmup, and summary statistics with
uncertainty estimates — not single-shot timings.

## Decision

### A new `apps/explorer` crate

`mathdoku-explorer` (directory `apps/explorer`, `publish = false`) is a
command-line application structured like `git`: a single binary with
subcommands, expected to grow into a grab-bag of experiments. Initial
subcommands:

- `mathdoku-explorer perf <CONFIG.yaml>` — run a performance experiment
  described by a YAML configuration file.
- `mathdoku-explorer list` — list the registered experiments with one-line
  descriptions.

The crate follows the workspace lint policy (`clippy::all`, `pedantic`,
`nursery`, denied `unwrap`/`expect`/`panic`/`print_stdout`/...). All
user-facing output is written through injected `io::Write` handles, which
both satisfies the lint policy and makes output unit-testable. Errors use
`thiserror` (as in `mathdoku-designer-core`); `main` returns
`std::process::ExitCode` and reports errors on stderr through the same
injected-writer mechanism.

### The performance test bed (Theory)

The test bed is the *general purpose* half of Explorer; individual
experiments plug into it. Its measurement model:

- **Experiment** — a named, registered unit (e.g. `solve-time`) that, given
  its YAML `parameters`, produces an ordered list of **conditions**.
- **Condition** — one level of the independent variable (e.g. `n = 5`). The
  unit of statistical aggregation.
- **Trial** — one timed execution under a condition. Each trial uses a fresh,
  independently generated problem instance so that trials are i.i.d. samples
  from the population "random puzzles of size n", not repeated measurements
  of one instance.
- **Warmup** — per condition, `warmup` trials are run and discarded before
  the `samples` measured trials, absorbing cache/branch-predictor/allocator
  transients.
- **Timing** — wall-clock `std::time::Instant` around the operation under
  test only; instance generation and bookkeeping are excluded from the timed
  region. Measurements are recorded in integer nanoseconds.
- **Reproducibility** — a master seed (from config, or generated and
  recorded) plus the condition and trial indices deterministically derive a
  `ChaCha8Rng` per trial via SplitMix64 mixing, so any individual trial can
  be re-run in isolation and the whole experiment is reproducible bit-for-bit
  given its config and recorded seed.
- **Release-mode guard** — the runner refuses to run (override with
  `--allow-debug`) when compiled with `debug_assertions`, because unoptimised
  measurements of an optimised library are meaningless.

### Statistics

Per condition and per measured phase the test bed reports: sample count,
mean, sample standard deviation, min, max, quartiles (p25/median/p75, linear
interpolation, R-7), and a 95% confidence interval for the mean using the
Student t critical value for the sample's degrees of freedom. Timing
distributions are right-skewed, so the median/IQR are reported alongside the
mean ± CI rather than treating the mean as the whole story. Statistics are
implemented in a small, unit-tested `stats` module rather than pulling in a
statistics dependency.

### Experiment configuration (YAML)

```yaml
# Which registered experiment to run, and the statistical protocol.
experiment: solve-time
seed: 20260609          # optional; omitted → randomly drawn and recorded
warmup: 3               # discarded trials per condition
samples: 50             # measured trials per condition
output_directory: results
parameters:             # experiment-specific, opaque to the test bed
  n: [3, 4, 5, 6, 7]
```

Common fields are strongly typed and validated (`samples >= 2` so a standard
deviation exists, experiment name must be registered, ...). `parameters` is
an opaque YAML mapping handed to the experiment, so new experiments do not
touch the test-bed schema.

### Output

Each run creates `<output_directory>/<experiment>-<UTC timestamp>/`:

- `config.yaml` — the resolved configuration, including the actually-used
  seed (provenance).
- `metadata.yaml` — package version, git commit, rustc version, profile,
  target triple, OS, start time (environment provenance).
- `raw.csv` — one row per measured trial: condition, trial index, derived
  seed, one column per measured phase (ns).
- `summary.csv` — one row per condition × phase with the statistics above.

A human-readable summary table is written to stdout at the end of the run;
progress goes to stderr.

### First experiment: `solve-time`

Measures shipping-code solving performance as a function of `n`. Per trial:

1. *Setup (untimed)* — `generate(n, rng)` a random puzzle, extract its cages.
2. *Propagate (timed)* — rebuild a fresh `Puzzle::new(n)` by `insert_cage`
   for every cage. This re-runs the constraint-propagation fixpoint from
   scratch; the puzzle returned by `generate` has already been narrowed, so
   timing its `solutions()` alone would measure search on a pre-propagated
   instance and dramatically understate solving cost.
3. *Search (timed)* — `solutions().next()` on the rebuilt puzzle: time to
   first solution.

Recorded phases: `propagate_ns`, `search_ns`, `total_ns` (= sum). The
solution-found flag is asserted (a generated puzzle always has a solution).

### Module layout

```
apps/explorer/
  Cargo.toml            # mathdoku-explorer, publish = false
  README.md             # what is measured and how (theory), usage (praxis)
  examples/             # ready-to-run experiment configs
  src/
    main.rs             # thin shim: parse args, dispatch, exit code
    cli.rs              # clap derive types: subcommands
    config.rs           # YAML schema + validation
    stats.rs            # descriptive statistics + t-based CI
    runner.rs           # test bed: warmup/trials/seeding/timing/outputs
    output.rs           # run directory, CSV writers, summary table
    experiments/
      mod.rs            # Experiment trait + registry
      solve_time.rs     # the first experiment
```

Dependencies: `clap` (derive), `serde`, `serde_yaml_ng`, `csv`, `thiserror`,
`rand`, `rand_chacha`, `time` or `chrono`-free timestamping via `std` where
practical. `mathdoku` by path.

### CI

Explorer joins the root workspace, so the existing workspace-wide fmt,
clippy, and doc gates cover it automatically. The CI `test` job and the
pre-commit hook gain `cargo test -p mathdoku-explorer`.

## Consequences

- Performance questions get answered with recorded, reproducible data;
  results land in versionable CSV files that downstream analysis (plots,
  regressions) can consume without re-running experiments.
- Adding an experiment is: implement the `Experiment` trait, register it,
  document it in the README. The YAML schema and runner are untouched.
- Wall-clock timing on a multitasking OS is noisy; the protocol (warmup,
  many i.i.d. trials, medians + CIs) manages but cannot eliminate this.
  Cross-machine comparisons remain invalid; `metadata.yaml` makes the
  machine context explicit.
- The `solve-time` experiment measures the *generator's* puzzle population.
  Conclusions are about average-case behaviour over generated puzzles, not
  worst-case complexity, and depend on the generator's cage-size
  distribution. This is the population end users experience, so it is the
  right default — but it must be stated in the README, and it is.
