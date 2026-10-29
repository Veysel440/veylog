//! Statistics: tests whether a change between two time ranges could be chance, and measures the
//! middle and the spread of a series of numbers without being swayed by extreme values.

/// Upper tail of the binomial distribution: P(X ≥ k), X ~ Binom(n, p).
///
/// The probability of at least k successes in n independent trials that each succeed with
/// probability p. In a comparison, if a pattern's frequency did not change, each of its n
/// records falls into the "after" half with a probability equal to that half's share of the
/// time span (p); if the probability that at least k of them fall after is small, the increase
/// is not chance.
///
/// The terms are added one by one: P(X = i + 1) = P(X = i) · (n − i)/(i + 1) · p/(1 − p). Very
/// small probabilities (below 10⁻³⁰⁰) would round to zero in `f64`, so the logarithms of the
/// terms are kept and added relative to the largest term.
pub fn binomial_upper_tail(n: usize, k: usize, p: f64) -> f64 {
    if k == 0 {
        return 1.0;
    }
    if k > n || p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let (ln_p, ln_q) = (p.ln(), (1.0 - p).ln());
    // ln P(X = k) = ln C(n, k) + k·ln p + (n − k)·ln(1 − p)
    let mut ln_term = ln_choose(n, k) + k as f64 * ln_p + (n - k) as f64 * ln_q;
    // The sum is kept as a multiple of the largest term so far:
    // sum = (sum of the terms) / exp(ln_max)
    let mut ln_max = ln_term;
    let mut sum = 1.0;
    for i in k..n {
        ln_term += ((n - i) as f64 / (i + 1) as f64).ln() + ln_p - ln_q;
        if ln_term > ln_max {
            // A new largest term: convert the old sum to the new scale
            sum = sum * (ln_max - ln_term).exp() + 1.0;
            ln_max = ln_term;
        } else {
            let relative = (ln_term - ln_max).exp();
            sum += relative;
            // Past the peak the terms only get smaller; a term this small and the ones after
            // it no longer change the sum
            if relative < 1e-17 {
                break;
            }
        }
    }
    (ln_max + sum.ln()).exp().min(1.0)
}

/// ln C(n, k): the natural logarithm of the number of ways to choose k things out of n.
///
/// C(n, k) = C(n, n − k), so the shorter side is summed.
fn ln_choose(n: usize, k: usize) -> f64 {
    let k = k.min(n - k);
    (1..=k).map(|i| ((n - k + i) as f64 / i as f64).ln()).sum()
}

/// The middle and the spread of a series of numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedianMad {
    /// The middle element of the sorted series; with an even number of elements, the mean of
    /// the middle two.
    pub median: f64,
    /// Median absolute deviation: the median of the elements' absolute deviations from the
    /// median.
    pub mad: f64,
}

/// The median and MAD of a sparse series: `zeros` zeros plus `counts`. None if the series is
/// empty: an empty series has no middle.
///
/// The mean and the standard deviation shift with a single extreme value; the median and MAD
/// stay put even if up to half of the elements are extreme. That is what spike detection
/// needs: the burst being looked for must not distort the measure of the usual level.
///
/// Most of a pattern's per-minute counts are 0; for an error seen a few times a month, nearly
/// all of 43 200 minutes. The zeros are not stored one by one, only counted; memory is
/// proportional to the number of nonzero elements. The middle elements of the sorted series
/// are found with `kth_with_copies`.
pub fn median_and_mad(zeros: usize, counts: &[usize]) -> Option<MedianMad> {
    if zeros + counts.len() == 0 {
        return None;
    }
    let mut values: Vec<f64> = counts.iter().map(|&count| count as f64).collect();
    // f64 is not `Ord` (NaN cannot be compared with any number), so `sort` does not work.
    // `total_cmp` defines a total order that includes NaN.
    values.sort_unstable_by(f64::total_cmp);
    let median = median_with_copies(&values, 0.0, zeros);
    // The deviation of each zero from the median is the median itself: |0 − median|
    let mut deviations: Vec<f64> = values.iter().map(|value| (value - median).abs()).collect();
    deviations.sort_unstable_by(f64::total_cmp);
    let mad = median_with_copies(&deviations, median, zeros);
    Some(MedianMad { median, mad })
}

/// The median of the series that inserting `copies` copies of `value` into the sorted series
/// `sorted` would give. That series must not be empty.
fn median_with_copies(sorted: &[f64], value: f64, copies: usize) -> f64 {
    let n = sorted.len() + copies;
    // With an odd number of elements both positions are the same: the exact middle element
    let lower = kth_with_copies(sorted, value, copies, (n - 1) / 2);
    let upper = kth_with_copies(sorted, value, copies, n / 2);
    // `midpoint`: exactly halfway between two numbers, (lower + upper) / 2. The sum does not
    // overflow even for very large numbers.
    lower.midpoint(upper)
}

/// Element `k` (counting from 0) of the sorted series that inserting `copies` copies of `value`
/// into the sorted series `sorted` would give.
///
/// The inserted values would line up right after the elements of `sorted` that are smaller than
/// them. `partition_point` finds how many elements are smaller with a binary search: since the
/// series is sorted, the elements that meet the condition all come first.
///
/// A generic function: `T` can be any type that can be copied and ordered. Used with f64 for
/// the median, and with integers in the tests because they are easier to read.
fn kth_with_copies<T: Copy + PartialOrd>(sorted: &[T], value: T, copies: usize, k: usize) -> T {
    let smaller = sorted.partition_point(|&element| element < value);
    if k < smaller {
        sorted[k]
    } else if k < smaller + copies {
        value
    } else {
        sorted[k - copies]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equal if the relative error is at most 1e-9.
    fn approx_eq(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() <= 1e-9 * expected.abs().max(f64::MIN_POSITIVE)
    }

    #[test]
    fn ln_choose_is_correct() {
        assert!(approx_eq(ln_choose(6, 5).exp(), 6.0));
        assert!(approx_eq(ln_choose(10, 3).exp(), 120.0));
        assert!(approx_eq(ln_choose(52, 5).exp(), 2_598_960.0));
        assert_eq!(ln_choose(7, 0), 0.0);
        assert_eq!(ln_choose(7, 7), 0.0);
    }

    #[test]
    fn edge_cases() {
        assert_eq!(binomial_upper_tail(10, 0, 0.3), 1.0);
        assert_eq!(binomial_upper_tail(10, 11, 0.3), 0.0);
        assert_eq!(binomial_upper_tail(10, 1, 0.0), 0.0);
        assert_eq!(binomial_upper_tail(10, 10, 1.0), 1.0);
        // All of them: P(X = n) = pⁿ
        assert!(approx_eq(binomial_upper_tail(4, 4, 0.5), 0.0625));
    }

    #[test]
    fn matches_scipy() {
        // Expected values: scipy.stats.binom.sf(k - 1, n, p)
        for (n, k, p, expected) in [
            (6, 5, 0.2453, 0.00423959789579656),
            (10, 3, 0.3, 0.6172172136000003),
            (33, 3, 0.0217, 0.03435805459866819),
            (100, 60, 0.5, 0.028443966820490444),
            (1_000, 10, 0.001, 1.074283386846483e-7),
            // k slightly below the mean: the terms first grow, then shrink after the peak.
            // This tests rescaling the sum at each new largest term.
            (100, 45, 0.5, 0.8643734879630823),
            (10_000, 8_950, 0.9, 0.9530727017180728),
            // k far below the mean: the tail is almost 1. The first term is small enough to
            // round to zero in f64; adding logarithms matters here.
            (10_000, 10, 0.9, 1.0),
            // k far above the mean: very small but not zero
            (10_000, 9_500, 0.9, 9.661759316586807e-75),
        ] {
            let actual = binomial_upper_tail(n, k, p);
            assert!(
                approx_eq(actual, expected),
                "n={n} k={k} p={p}: {actual} ≠ {expected}"
            );
        }
    }

    #[test]
    fn copies_take_their_sorted_place() {
        // Inserting two 2s into [1, 3, 5]: [1, 2, 2, 3, 5]
        let actual: Vec<i32> = (0..5)
            .map(|k| kth_with_copies(&[1, 3, 5], 2, 2, k))
            .collect();
        assert_eq!(actual, [1, 2, 2, 3, 5]);
        // Values smaller than the smallest, larger than the largest and equal to an element
        let actual: Vec<i32> = (0..4).map(|k| kth_with_copies(&[1, 3], 0, 2, k)).collect();
        assert_eq!(actual, [0, 0, 1, 3]);
        let actual: Vec<i32> = (0..4).map(|k| kth_with_copies(&[1, 3], 9, 2, k)).collect();
        assert_eq!(actual, [1, 3, 9, 9]);
        let actual: Vec<i32> = (0..4)
            .map(|k| kth_with_copies(&[1, 3, 3], 3, 1, k))
            .collect();
        assert_eq!(actual, [1, 3, 3, 3]);
        // With no copies, the series itself
        let actual: Vec<i32> = (0..3)
            .map(|k| kth_with_copies(&[4, 5, 6], 1, 0, k))
            .collect();
        assert_eq!(actual, [4, 5, 6]);
    }

    #[test]
    fn median_and_mad_match_numpy() {
        // Expected values: numpy.median and scipy.stats.median_abs_deviation(scale=1), on the
        // series with its zeros written out
        for (zeros, counts, median, mad) in [
            // The example from Wikipedia: (1, 1, 2, 2, 4, 6, 9)
            (0, &[1, 1, 2, 2, 4, 6, 9][..], 2.0, 1.0),
            // The zeros reach up to the middle: the median and MAD depend on them
            (7, &[3, 1, 4, 1, 5, 9, 2, 6], 1.0, 1.0),
            // An even number of elements: the median is the mean of the middle two (1 and 5)
            (2, &[5, 5, 5, 1], 3.0, 2.0),
            (3, &[2, 8, 4], 1.0, 1.0),
            (0, &[10, 20, 30, 40], 25.0, 10.0),
            // 574 minutes of a sparse pattern: 570 of them empty
            (570, &[6, 1, 1, 2], 0.0, 0.0),
            (5, &[], 0.0, 0.0),
            (0, &[7], 7.0, 0.0),
            // Halfway: [0, 3]
            (1, &[3], 1.5, 1.5),
        ] {
            let actual = median_and_mad(zeros, counts).unwrap();
            assert_eq!(
                actual,
                MedianMad { median, mad },
                "{zeros} zeros and {counts:?}"
            );
        }
        assert_eq!(median_and_mad(0, &[]), None);
    }

    /// The direct, textbook calculation that writes out every zero and sorts.
    fn naive(zeros: usize, counts: &[usize]) -> MedianMad {
        fn median_of(array: &mut [f64]) -> f64 {
            array.sort_by(f64::total_cmp);
            let n = array.len();
            (array[(n - 1) / 2] + array[n / 2]) / 2.0
        }
        let mut array: Vec<f64> = vec![0.0; zeros];
        array.extend(counts.iter().map(|&count| count as f64));
        let median = median_of(&mut array);
        let mut deviations: Vec<f64> = array.iter().map(|value| (value - median).abs()).collect();
        MedianMad {
            median,
            mad: median_of(&mut deviations),
        }
    }

    #[test]
    fn sparse_matches_naive() {
        // 2 000 pseudo-random series (the same on every run): different lengths, shares of
        // zeros and repeated values. A simple linear congruential generator (LCG) is enough.
        let mut seed: u64 = 2026;
        let mut next = |bound: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % bound
        };
        for _ in 0..2_000 {
            let zeros = next(12) as usize;
            let len = next(12) as usize;
            let max = 1 + next(20);
            let counts: Vec<usize> = (0..len).map(|_| 1 + next(max) as usize).collect();
            if zeros + len == 0 {
                continue;
            }
            assert_eq!(
                median_and_mad(zeros, &counts).unwrap(),
                naive(zeros, &counts),
                "{zeros} zeros and {counts:?}"
            );
        }
    }
}
