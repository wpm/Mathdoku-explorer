//! Descriptive statistics for the performance test bed (ADR-0007).
//!
//! Theory: timing distributions on a multitasking operating system are
//! right-skewed: scheduler preemption, page faults, and cache misses stretch
//! the upper tail, while the cost of the work itself puts a hard floor under
//! the lower one. A [`Summary`] therefore reports two complementary views of
//! the same sample. The inferential view is the mean with a 95% confidence
//! interval: were the whole experiment repeated many times, about 95% of the
//! intervals so constructed would contain the true population mean. The
//! robust view is the median flanked by the quartiles (p25/p75), which
//! outliers barely move. When the two views disagree, the disagreement is
//! itself information — it measures the skew.
//!
//! Quartiles use linear interpolation between order statistics, method R-7
//! of Hyndman & Fan (1996): for quantile `p` over `n` ascending samples,
//! `h = (n - 1) * p` and the result is
//! `x[floor(h)] + (h - floor(h)) * (x[floor(h) + 1] - x[floor(h)])`.
//! R-7 is the default in numpy, R, and Excel, so the numbers in
//! `summary.csv` are directly reproducible in downstream analysis tools.
//!
//! The confidence interval uses the two-sided Student t critical value at
//! `n - 1` degrees of freedom rather than the normal 1.96 because the
//! population standard deviation is itself estimated from the sample; at the
//! small sample counts a slow condition forces, the correction is large
//! (df = 1 gives 12.706).

use thiserror::Error;

/// Why a set of samples cannot be summarised.
#[derive(Debug, Error)]
pub enum Error {
    /// Fewer than two samples were supplied; the sample standard deviation
    /// divides by `n - 1`, so two is the floor.
    #[error(
        "cannot summarise {count} sample(s): at least 2 are needed for a sample standard deviation"
    )]
    TooFewSamples {
        /// How many samples were supplied.
        count: usize,
    },

    /// A sample is NaN or infinite, so every statistic would be poisoned.
    #[error("cannot summarise: sample at index {index} is {value}, not a finite number")]
    NonFinite {
        /// Position of the offending sample in the input slice.
        index: usize,
        /// The offending value.
        value: f64,
    },
}

/// Descriptive statistics over one sample of finite measurements.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    /// Number of samples summarised.
    pub count: usize,
    /// Arithmetic mean.
    pub mean: f64,
    /// Sample standard deviation (`n - 1` denominator).
    pub std_dev: f64,
    /// Smallest sample.
    pub min: f64,
    /// Largest sample.
    pub max: f64,
    /// First quartile (R-7).
    pub p25: f64,
    /// Second quartile (R-7).
    pub median: f64,
    /// Third quartile (R-7).
    pub p75: f64,
    /// Lower endpoint of the 95% confidence interval for the mean.
    pub ci95_low: f64,
    /// Upper endpoint of the 95% confidence interval for the mean.
    pub ci95_high: f64,
}

impl Summary {
    /// Summarises `samples`; the slice need not be sorted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TooFewSamples`] for fewer than two samples and
    /// [`Error::NonFinite`] if any sample is NaN or infinite.
    pub fn from_samples(samples: &[f64]) -> Result<Self, Error> {
        if samples.len() < 2 {
            return Err(Error::TooFewSamples {
                count: samples.len(),
            });
        }
        for (index, &value) in samples.iter().enumerate() {
            if !value.is_finite() {
                return Err(Error::NonFinite { index, value });
            }
        }
        // Every sample is finite, so `total_cmp` is a plain ascending sort.
        let mut sorted = samples.to_vec();
        sorted.sort_unstable_by(f64::total_cmp);

        let count = sorted.len();
        let n = count_as_f64(count);
        let mean = sorted.iter().sum::<f64>() / n;
        let variance = sorted.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_dev = variance.sqrt();
        let half_width = t_critical(count - 1) * std_dev / n.sqrt();

        Ok(Self {
            count,
            mean,
            std_dev,
            min: sorted[0],
            max: sorted[count - 1],
            p25: quantile_r7(&sorted, 0.25),
            median: quantile_r7(&sorted, 0.5),
            p75: quantile_r7(&sorted, 0.75),
            ci95_low: mean - half_width,
            ci95_high: mean + half_width,
        })
    }
}

/// Two-sided Student t critical values at the 0.975 quantile as `(df, t)`
/// pairs: every df from 1 to 30, then the anchors 40, 60, and 120. Values
/// from the standard t table (R: `qt(0.975, df)`), rounded to three
/// decimals.
const T_TABLE: [(usize, f64); 33] = [
    (1, 12.706),
    (2, 4.303),
    (3, 3.182),
    (4, 2.776),
    (5, 2.571),
    (6, 2.447),
    (7, 2.365),
    (8, 2.306),
    (9, 2.262),
    (10, 2.228),
    (11, 2.201),
    (12, 2.179),
    (13, 2.160),
    (14, 2.145),
    (15, 2.131),
    (16, 2.120),
    (17, 2.110),
    (18, 2.101),
    (19, 2.093),
    (20, 2.086),
    (21, 2.080),
    (22, 2.074),
    (23, 2.069),
    (24, 2.064),
    (25, 2.060),
    (26, 2.056),
    (27, 2.052),
    (28, 2.048),
    (29, 2.045),
    (30, 2.042),
    (40, 2.021),
    (60, 2.000),
    (120, 1.980),
];

/// The two-sided 0.975 normal quantile, used beyond df 120 where the t
/// distribution is normal for all practical purposes.
const T_BEYOND_TABLE: f64 = 1.96;

/// Two-sided 95% Student t critical value for `degrees_of_freedom >= 1`.
///
/// Untabulated df (31..=39, 41..=59, 61..=119) use the next *lower*
/// tabulated df, whose critical value is larger: the resulting interval is
/// wider, never narrower (conservative). Beyond df 120 the normal 1.96 is
/// used. Maximum approximation error, against exact quantiles: the worst
/// cases sit just below each anchor and just above 120 — t(39) = 2.023 vs
/// the 2.042 used (+0.95%), t(59) = 2.001 vs 2.021 (+1.0%), t(119) = 1.980
/// vs 2.000 (+1.0%), and t(121) = 1.980 vs 1.96 (-1.0%, the only
/// anti-conservative case). The value used is therefore always within about
/// 1% (absolute error below 0.021) of the exact critical value.
fn t_critical(degrees_of_freedom: usize) -> f64 {
    if degrees_of_freedom > 120 {
        return T_BEYOND_TABLE;
    }
    T_TABLE
        .iter()
        .rev()
        .find(|&&(df, _)| df <= degrees_of_freedom)
        // Unreachable for df >= 1 (the table starts at 1), but the normal
        // quantile is the only sane fallback and avoids a panic path.
        .map_or(T_BEYOND_TABLE, |&(_, critical)| critical)
}

/// R-7 quantile (Hyndman & Fan 1996) of `sorted`, which must be ascending,
/// non-empty, and all-finite: `h = (n - 1) * p`, linearly interpolating
/// between `x[floor(h)]` and `x[floor(h) + 1]`.
fn quantile_r7(sorted: &[f64], p: f64) -> f64 {
    let h = (count_as_f64(sorted.len()) - 1.0) * p;
    let floor = h.floor();
    // `h` lies in `[0, n - 1]` for `p` in `[0, 1]`, so the cast neither
    // truncates a fractional bound nor loses a sign.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lower = floor as usize;
    let fraction = h - floor;
    let below = sorted[lower];
    // `lower + 1` runs off the end only when `h` is exactly `n - 1`; the
    // fraction is then 0 and the lower order statistic is the exact answer.
    sorted
        .get(lower + 1)
        .map_or(below, |&above| fraction.mul_add(above - below, below))
}

/// Converts a sample count to `f64`.
// Exact for every count below 2^53; no experiment records that many trials.
#[allow(clippy::cast_precision_loss)]
const fn count_as_f64(count: usize) -> f64 {
    count as f64
}

#[cfg(test)]
mod tests {
    // Exact float equality below is deliberate: the same multiset of inputs
    // must produce bit-identical statistics, and some fixtures have exact
    // binary answers.
    #![allow(clippy::float_cmp)]

    use super::{Error, Summary};

    /// Asserts that `actual` is within `epsilon` of `expected`.
    fn assert_close(label: &str, actual: f64, expected: f64, epsilon: f64) {
        assert!(
            (actual - expected).abs() <= epsilon,
            "{label}: expected {expected}, got {actual}"
        );
    }

    fn summarise(samples: &[f64]) -> Summary {
        Summary::from_samples(samples).expect("samples should summarise")
    }

    // Reference values for x = [1, 2, 3, 4, 5] from R:
    //   mean(x) = 3, sd(x) = sqrt(2.5) = 1.5811388,
    //   quantile(x, c(.25, .5, .75), type = 7) = (2, 3, 4).
    // CI with this module's 3-decimal table entry t(df = 4) = 2.776:
    //   half-width = 2.776 * 1.5811388 / sqrt(5) = 1.9629284
    //   -> (1.0370716, 4.9629284).
    // scipy's full-precision t.ppf(0.975, 4) = 2.7764451 gives
    // (1.0367568, 4.9632432); the ~3e-4 difference is the documented
    // rounding of the lookup table.
    #[test]
    fn five_point_fixture_matches_references() {
        let summary = summarise(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(summary.count, 5);
        assert_close("mean", summary.mean, 3.0, 1e-12);
        assert_close("std_dev", summary.std_dev, 1.581_138_8, 1e-6);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 5.0);
        assert_close("p25", summary.p25, 2.0, 1e-12);
        assert_close("median", summary.median, 3.0, 1e-12);
        assert_close("p75", summary.p75, 4.0, 1e-12);
        assert_close("ci95_low", summary.ci95_low, 1.037_071_6, 1e-5);
        assert_close("ci95_high", summary.ci95_high, 4.962_928_4, 1e-5);
    }

    // Even n with non-integer R-7 quartile positions, x = [1, 2, 4, 7, 11, 16].
    // h = (n - 1) * p with n = 6:
    //   p25: h = 5 * 0.25 = 1.25 -> x[1] + 0.25 * (x[2] - x[1]) = 2 + 0.25 * 2 = 2.5
    //   p50: h = 5 * 0.50 = 2.50 -> x[2] + 0.50 * (x[3] - x[2]) = 4 + 0.50 * 3 = 5.5
    //   p75: h = 5 * 0.75 = 3.75 -> x[3] + 0.75 * (x[4] - x[3]) = 7 + 0.75 * 4 = 10.0
    // numpy agrees: np.percentile(x, [25, 50, 75]) = [2.5, 5.5, 10.0]
    // (numpy's default interpolation is R-7).
    //   mean = 41 / 6 = 6.8333333
    //   variance = (sum(x^2) - n * mean^2) / (n - 1) = (447 - 1681 / 6) / 5
    //            = 1001 / 30
    //   sd = sqrt(1001 / 30) = 5.7763890 (R: sd(x) = 5.776389)
    // CI with t(df = 5) = 2.571:
    //   half-width = 2.571 * 5.7763890 / sqrt(6) = 6.0629345
    //   -> (0.7703988, 12.8962678)
    #[test]
    fn even_count_fixture_interpolates_quartiles() {
        let summary = summarise(&[1.0, 2.0, 4.0, 7.0, 11.0, 16.0]);
        assert_eq!(summary.count, 6);
        assert_close("mean", summary.mean, 6.833_333_3, 1e-6);
        assert_close("std_dev", summary.std_dev, 5.776_389_0, 1e-6);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 16.0);
        assert_close("p25", summary.p25, 2.5, 1e-12);
        assert_close("median", summary.median, 5.5, 1e-12);
        assert_close("p75", summary.p75, 10.0, 1e-12);
        assert_close("ci95_low", summary.ci95_low, 0.770_398_8, 1e-4);
        assert_close("ci95_high", summary.ci95_high, 12.896_267_8, 1e-4);
    }

    // The degenerate-but-legal n = 2 case, x = [10, 20]: df = 1, t = 12.706.
    //   mean = 15, sd = sqrt(5^2 + 5^2) = sqrt(50) = 7.0710678
    //   half-width = 12.706 * sqrt(50) / sqrt(2) = 12.706 * 5 = 63.53 exactly
    //   R-7: p25 at h = 0.25 -> 10 + 0.25 * 10 = 12.5; median 15; p75 17.5.
    #[test]
    fn two_samples_use_the_df_one_critical_value() {
        let summary = summarise(&[10.0, 20.0]);
        assert_eq!(summary.count, 2);
        assert_close("mean", summary.mean, 15.0, 1e-12);
        assert_close("std_dev", summary.std_dev, 7.071_067_8, 1e-6);
        assert_close("p25", summary.p25, 12.5, 1e-12);
        assert_close("median", summary.median, 15.0, 1e-12);
        assert_close("p75", summary.p75, 17.5, 1e-12);
        assert_close("ci95_low", summary.ci95_low, -48.53, 1e-9);
        assert_close("ci95_high", summary.ci95_high, 78.53, 1e-9);
    }

    #[test]
    fn identical_samples_collapse_every_statistic_to_the_value() {
        let summary = summarise(&[7.0, 7.0, 7.0, 7.0]);
        assert_eq!(summary.std_dev, 0.0);
        assert_eq!(summary.mean, 7.0);
        assert_eq!(summary.ci95_low, 7.0);
        assert_eq!(summary.ci95_high, 7.0);
        assert_eq!(summary.min, 7.0);
        assert_eq!(summary.p25, 7.0);
        assert_eq!(summary.median, 7.0);
        assert_eq!(summary.p75, 7.0);
        assert_eq!(summary.max, 7.0);
    }

    #[test]
    fn input_order_does_not_change_the_summary() {
        let shuffled = summarise(&[3.0, 1.0, 4.0, 1.5, 9.0, 2.6, 5.0]);
        let sorted = summarise(&[1.0, 1.5, 2.6, 3.0, 4.0, 5.0, 9.0]);
        assert_eq!(shuffled.count, sorted.count);
        assert_eq!(shuffled.mean, sorted.mean);
        assert_eq!(shuffled.std_dev, sorted.std_dev);
        assert_eq!(shuffled.min, sorted.min);
        assert_eq!(shuffled.max, sorted.max);
        assert_eq!(shuffled.p25, sorted.p25);
        assert_eq!(shuffled.median, sorted.median);
        assert_eq!(shuffled.p75, sorted.p75);
        assert_eq!(shuffled.ci95_low, sorted.ci95_low);
        assert_eq!(shuffled.ci95_high, sorted.ci95_high);
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(matches!(
            Summary::from_samples(&[]),
            Err(Error::TooFewSamples { count: 0 })
        ));
    }

    #[test]
    fn a_single_sample_is_an_error() {
        assert!(matches!(
            Summary::from_samples(&[42.0]),
            Err(Error::TooFewSamples { count: 1 })
        ));
    }

    #[test]
    fn nan_is_an_error_naming_the_offender() {
        let result = Summary::from_samples(&[1.0, f64::NAN, 3.0]);
        assert!(matches!(result, Err(Error::NonFinite { index: 1, .. })));
    }

    #[test]
    fn infinity_is_an_error_naming_the_offender() {
        let result = Summary::from_samples(&[1.0, 2.0, f64::NEG_INFINITY]);
        assert!(matches!(result, Err(Error::NonFinite { index: 2, .. })));
    }

    #[test]
    fn ordering_properties_hold_across_fixtures() {
        let fixtures: [&[f64]; 4] = [
            &[1.0, 2.0, 3.0, 4.0, 5.0],
            &[10.0, 20.0],
            &[7.0, 7.0, 7.0, 7.0],
            // Strongly right-skewed, like a timing distribution.
            &[1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.4, 50.0],
        ];
        for samples in fixtures {
            let summary = summarise(samples);
            assert!(summary.ci95_low <= summary.mean && summary.mean <= summary.ci95_high);
            assert!(summary.min <= summary.p25);
            assert!(summary.p25 <= summary.median);
            assert!(summary.median <= summary.p75);
            assert!(summary.p75 <= summary.max);
        }
    }
}
