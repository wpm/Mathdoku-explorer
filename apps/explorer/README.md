# Mathdoku Explorer

`mathdoku-explorer` is a command-line grab-bag of scientific experiments over
the `mathdoku` library, structured like `git`: one binary, many subcommands.
Today it has two — `list` shows the registered experiments, `perf` runs a
performance experiment from a YAML configuration file. The architecture and
the measurement methodology are specified in
[ADR-0007](../../adr/0007-explorer-performance-test-bed.md); this README
explains what is being measured and how, and how to run it.

## Theory: the measurement model

A performance run is built from three nested units:

- **Experiment** — a named, registered unit (e.g. `solve-time`) that turns
  its YAML `parameters` into an ordered list of conditions.
- **Condition** — one level of the independent variable (e.g. `n = 5`). The
  unit of statistical aggregation.
- **Trial** — one timed execution under a condition.

Every trial generates a **fresh problem instance** from its own RNG. Trials
are therefore i.i.d. samples from the population "random instances under
this condition", not repeated measurements of one instance — repeating one
instance would tell you about that instance's variance under OS noise, not
about the population you actually care about.

Per condition, `warmup` trials are run and discarded before the `samples`
measured trials, absorbing cache, branch-predictor, and allocator
transients. Timing is wall-clock (`std::time::Instant`) around the
operation under test only, recorded in integer nanoseconds; instance
generation and the runner's bookkeeping are excluded from the timed region.

Randomness is fully reproducible. A master seed comes from the
configuration (or is drawn and recorded), and each trial's `ChaCha8Rng`
seed is derived from `(master seed, condition index, trial index)` by
SplitMix64 mixing, so any single trial can be re-run in isolation. What
reproduces is the **workload**: conditions, trial indices, derived seeds,
and the generated instances. The measured wall-clock durations do not
reproduce — they depend on the machine and the moment.

Because unoptimised timings say nothing about the optimised library, the
runner refuses to measure a `debug_assertions` build unless you pass
`--allow-debug`.

## Statistics

For every condition × phase, `summary.csv` reports the sample count, mean,
sample standard deviation, min, max, quartiles (p25/median/p75, linear
interpolation), and a 95% confidence interval for the mean using the
Student t critical value for the sample's degrees of freedom.

Read it like this: the mean ± CI estimates the population average, and the
CI narrows as `samples` grows. But timing distributions are right-skewed —
a few trials land on a descheduled core or a slow allocation path — so the
median and IQR are reported alongside; when mean and median disagree
badly, trust the median for "typical" cost and read the mean as including
the tail.

## The `solve-time` experiment

Measures shipping-code solving performance as a function of grid size `n`.
One condition per requested `n`; per trial:

| Phase | Timed? | What happens |
|-------|--------|--------------|
| setup | no | `generate(n, rng)` produces a fresh random puzzle; its cages are collected. |
| `propagate` | yes | A fresh `Puzzle::new(n)` is rebuilt by `insert_cage` for every cage, re-running the constraint-propagation fixpoint from scratch. |
| `search` | yes | `solutions().next()` on the rebuilt puzzle: time to the first solution. |
| `total` | derived | The runner's per-trial sum of the measured phases. |

The rebuild in `propagate` is the crucial step: `generate()` hands back an
*already-narrowed* puzzle, so timing its `solutions()` alone would measure
search on a pre-propagated instance and dramatically understate the cost
of solving a puzzle from cold.

Expect the search phase to be heavily right-tailed as `n` grows: at
`n = 7` the occasional generated instance takes minutes to solve where the
median is around 100 ms. The example configuration stops at `n = 6` for
exactly this reason; the protocol handles the tail statistically (median
alongside mean), but you still have to wait for it. Sizes 8 and 9 are
accepted by the experiment and sit even further out on that curve — a
single trial can take far longer still, with progress reported only per
condition, so silence does not mean a hang.

One caveat to keep in mind when reading results: the instances are drawn
from the **generator's** population. Conclusions are about average-case
behaviour over generated puzzles — the population end users experience —
not worst-case complexity, and they depend on the generator's cage-size
distribution. And as with all wall-clock measurement, comparisons across
machines are invalid; `metadata.yaml` records the machine context that a
number is valid within.

## Praxis: usage

Build and run in release mode from the workspace root:

```sh
cargo run --release -p mathdoku-explorer -- list
cargo run --release -p mathdoku-explorer -- perf apps/explorer/examples/solve_time_vs_n.yaml
```

[`examples/solve_time_vs_n.yaml`](examples/solve_time_vs_n.yaml) is a
ready-to-run configuration with each field commented. Progress streams to
stderr; the final summary table and the run directory path go to stdout.

### Run directory layout

Each run creates `<output_directory>/<experiment>-<UTC timestamp>/`:

| File | Contents |
|------|----------|
| `config.yaml` | The resolved configuration, including the actually-used master seed — everything needed to reproduce the workload. |
| `metadata.yaml` | Environment provenance: package version, git commit, rustc version, profile, target, OS, start time. |
| `raw.csv` | One row per measured trial: `experiment`, `condition`, `trial` (index), `seed` (the trial's derived RNG seed), and one `<phase>_ns` column per phase in integer nanoseconds. |
| `summary.csv` | One row per condition × phase, keyed by leading `condition` and `phase` columns: `count`, `mean_ns`, `std_dev_ns`, `min_ns`, `p25_ns`, `median_ns`, `p75_ns`, `max_ns`, `ci95_low_ns`, `ci95_high_ns`. |

`raw.csv` keeps every measurement so later analyses (plots, regressions)
never need a re-run.

### Adding a new experiment

1. Implement the `Experiment` and `PreparedExperiment` traits in a new
   module under `src/experiments/` (see `solve_time.rs` for the pattern:
   parse the opaque `parameters` YAML, build labelled conditions, time the
   phases you declared — in the declared order, every trial).
2. Register it: append one `Box::new(...)` line to
   `experiments::registry()`.
3. Document it: a section in this README and, ideally, an example
   configuration under `examples/`.

The YAML schema and the runner are untouched; `parameters` is opaque to
the test bed.
